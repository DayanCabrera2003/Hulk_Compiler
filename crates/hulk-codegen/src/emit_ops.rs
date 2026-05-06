use inkwell::FloatPredicate;

use hulk_banner::{TempId, Value};
use hulk_hir::{BinOpKind, UnaryOpKind};

use crate::{
    codegen::{Codegen, LlvmVal, TempKind},
    error::{CodegenError, CodegenResult},
};

impl<'ctx> Codegen<'ctx> {
    /// Emit a binary operation instruction.
    pub(crate) fn emit_binop(
        &mut self,
        dst: TempId,
        op: BinOpKind,
        left: &Value,
        right: &Value,
    ) -> CodegenResult<()> {
        let result = match op {
            BinOpKind::Add => {
                let (l, r) = self.load_float_pair(left, right, "add")?;
                LlvmVal::Float(self.builder.build_float_add(l, r, "fadd")?)
            }
            BinOpKind::Sub => {
                let (l, r) = self.load_float_pair(left, right, "sub")?;
                LlvmVal::Float(self.builder.build_float_sub(l, r, "fsub")?)
            }
            BinOpKind::Mul => {
                let (l, r) = self.load_float_pair(left, right, "mul")?;
                LlvmVal::Float(self.builder.build_float_mul(l, r, "fmul")?)
            }
            BinOpKind::Div => {
                let (l, r) = self.load_float_pair(left, right, "div")?;
                LlvmVal::Float(self.builder.build_float_div(l, r, "fdiv")?)
            }
            BinOpKind::Mod => {
                let (l, r) = self.load_float_pair(left, right, "mod")?;
                LlvmVal::Float(self.builder.build_float_rem(l, r, "frem")?)
            }
            BinOpKind::Pow => self.emit_pow(left, right)?,
            BinOpKind::Eq => self.emit_float_cmp(left, right, FloatPredicate::OEQ, "feq")?,
            BinOpKind::Ne => self.emit_float_cmp(left, right, FloatPredicate::ONE, "fne")?,
            BinOpKind::Lt => self.emit_float_cmp(left, right, FloatPredicate::OLT, "flt")?,
            BinOpKind::Le => self.emit_float_cmp(left, right, FloatPredicate::OLE, "fle")?,
            BinOpKind::Gt => self.emit_float_cmp(left, right, FloatPredicate::OGT, "fgt")?,
            BinOpKind::Ge => self.emit_float_cmp(left, right, FloatPredicate::OGE, "fge")?,
            BinOpKind::And => {
                let lv = self.load_val(left)?;
                let rv = self.load_val(right)?;
                let l = self.coerce_to_bool(lv)?;
                let r = self.coerce_to_bool(rv)?;
                LlvmVal::Int(self.builder.build_and(l, r, "and")?)
            }
            BinOpKind::Or => {
                let lv = self.load_val(left)?;
                let rv = self.load_val(right)?;
                let l = self.coerce_to_bool(lv)?;
                let r = self.coerce_to_bool(rv)?;
                LlvmVal::Int(self.builder.build_or(l, r, "or")?)
            }
            BinOpKind::Concat => self.emit_concat_op(left, right)?,
            BinOpKind::ConcatSpaced => {
                return Err(CodegenError::Llvm(
                    "ConcatSpaced reached codegen — desugaring incomplete".to_string(),
                ));
            }
        };
        self.store_temp(dst, result)
    }

    /// Emit a unary operation.
    pub(crate) fn emit_unop(
        &mut self,
        dst: TempId,
        op: UnaryOpKind,
        operand: &Value,
    ) -> CodegenResult<()> {
        let v = self.load_val(operand)?;
        let result = match op {
            UnaryOpKind::Neg => {
                let f = extract_float(v)?;
                LlvmVal::Float(self.builder.build_float_neg(f, "fneg")?)
            }
            UnaryOpKind::Not => {
                let b = self.coerce_to_bool(v)?;
                LlvmVal::Int(self.builder.build_not(b, "not")?)
            }
        };
        self.store_temp(dst, result)
    }

    // ─── Private arithmetic helpers ─────────────────────────────────────────

    fn load_float_pair(
        &mut self,
        left: &Value,
        right: &Value,
        ctx: &str,
    ) -> CodegenResult<(inkwell::values::FloatValue<'ctx>, inkwell::values::FloatValue<'ctx>)> {
        let lv = self.load_val(left)?;
        let rv = self.load_val(right)?;
        let l = extract_float_ctx(lv, ctx, "lhs")?;
        let r = extract_float_ctx(rv, ctx, "rhs")?;
        Ok((l, r))
    }

    fn emit_pow(&mut self, left: &Value, right: &Value) -> CodegenResult<LlvmVal<'ctx>> {
        let (l, r) = self.load_float_pair(left, right, "pow")?;
        let f64_t = self.ctx.f64_type();
        let pow_ty = f64_t.fn_type(&[f64_t.into(), f64_t.into()], false);
        let pow_fn = self.module.add_function("llvm.pow.f64", pow_ty, None);
        let result = self.builder.build_call(pow_fn, &[l.into(), r.into()], "pow")?;
        let fv = result
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodegenError::Llvm("llvm.pow.f64 returned void".to_string()))?
            .into_float_value();
        Ok(LlvmVal::Float(fv))
    }

    fn emit_float_cmp(
        &mut self,
        left: &Value,
        right: &Value,
        pred: FloatPredicate,
        name: &str,
    ) -> CodegenResult<LlvmVal<'ctx>> {
        let (l, r) = self.load_float_pair(left, right, name)?;
        Ok(LlvmVal::Int(self.builder.build_float_compare(pred, l, r, name)?))
    }

    fn emit_concat_op(&mut self, left: &Value, right: &Value) -> CodegenResult<LlvmVal<'ctx>> {
        let lv = self.load_val(left)?;
        let rv = self.load_val(right)?;
        let l = self.coerce_to_ptr(lv)?;
        let r = self.coerce_to_ptr(rv)?;
        let result =
            self.builder.build_call(self.rt.hulk_concat, &[l.into(), r.into()], "concat")?;
        let ptr = result
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodegenError::Llvm("hulk_string_concat returned void".to_string()))?
            .into_pointer_value();
        Ok(LlvmVal::Ptr(ptr))
    }
}

fn extract_float<'ctx>(val: LlvmVal<'ctx>) -> CodegenResult<inkwell::values::FloatValue<'ctx>> {
    extract_float_ctx(val, "operation", "operand")
}

fn extract_float_ctx<'ctx>(
    val: LlvmVal<'ctx>,
    op: &str,
    side: &str,
) -> CodegenResult<inkwell::values::FloatValue<'ctx>> {
    match val {
        LlvmVal::Float(f) => Ok(f),
        _ => Err(CodegenError::Llvm(format!("expected f64 for {side} of {op}"))),
    }
}

/// Classify a `BinOpKind` for type inference of the result temporary.
#[must_use]
#[allow(dead_code)]
pub fn binop_result_kind(op: BinOpKind) -> TempKind {
    match op {
        BinOpKind::Add
        | BinOpKind::Sub
        | BinOpKind::Mul
        | BinOpKind::Div
        | BinOpKind::Mod
        | BinOpKind::Pow => TempKind::F64,
        BinOpKind::Concat | BinOpKind::ConcatSpaced => TempKind::Ptr,
        _ => TempKind::I1,
    }
}
