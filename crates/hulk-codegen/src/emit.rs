use std::collections::HashMap;

use inkwell::{
    basic_block::BasicBlock,
    values::{BasicValueEnum, FunctionValue, PointerValue},
};

use hulk_banner::{BannerFunction, BannerProgram, Instr, TempId, Value};
use hulk_hir::BinOpKind;

use crate::{
    codegen::{Codegen, LlvmVal, TempKind},
    error::{CodegenError, CodegenResult},
};

// ─── Program-level orchestration ────────────────────────────────────────────

impl<'ctx> Codegen<'ctx> {
    /// Lower a complete `BannerProgram` into LLVM IR.
    ///
    /// Order: build struct types → build vtable globals → emit type methods →
    /// emit standalone functions → emit `__main__` as C `main`.
    pub fn emit_program(&mut self, program: &BannerProgram) -> CodegenResult<()> {
        // Build LLVM struct types for every user-defined type.
        let type_names: Vec<String> = program.types.iter().map(|td| td.name.clone()).collect();
        for name in &type_names {
            self.build_struct_type(name);
        }

        // Build TypeTag globals for every user-defined type.
        for td in &program.types {
            self.build_type_tag(&td.name, &td.pointer_map);
        }

        // Build vtable constant globals (references user-defined functions by name).
        for td in &program.types {
            let method_names: Vec<String> = self
                .layout
                .vtable_methods
                .get(&td.name)
                .cloned()
                .unwrap_or_default();
            self.build_vtable_global(&td.name, &method_names);
        }

        // Emit bodies for each method of each type.
        let type_methods: Vec<(String, Vec<BannerFunction>)> = program
            .types
            .iter()
            .map(|td| (td.name.clone(), td.methods.clone()))
            .collect();

        for (type_name, methods) in &type_methods {
            for method in methods {
                let is_init = method.name.ends_with(".__init__");
                self.emit_function(&method.name, method, &type_name.clone(), is_init)?;
            }
        }

        // Emit standalone functions.
        let functions: Vec<BannerFunction> = program.functions.clone();
        for func in &functions {
            self.emit_function(&func.name, func, "", false)?;
        }

        // Emit the program entry point as C `main`.
        let main_banner = program.main.clone();
        self.emit_main(&main_banner)?;
        Ok(())
    }

    /// Emit the `__main__` BANNER function as a C-compatible `main() -> i32`.
    fn emit_main(&mut self, banner_fn: &BannerFunction) -> CodegenResult<()> {
        self.reset_frame();
        let main_fv = *self
            .functions
            .get("main")
            .ok_or_else(|| CodegenError::Llvm("main function not pre-declared".to_string()))?;

        let entry = self.ctx.append_basic_block(main_fv, "entry");
        self.builder.position_at_end(entry);

        let kinds = infer_temp_kinds(
            &banner_fn.body,
            &self.fn_return_kinds.clone(),
            &self.field_kind_map.clone(),
            &HashMap::new(),
        );
        self.temp_kinds.clone_from(&kinds);
        self.create_temp_allocas(&banner_fn.body, main_fv)?;
        self.pre_create_label_blocks(&banner_fn.body, main_fv);

        for instr in &banner_fn.body {
            self.emit_instr(instr, main_fv, true)?;
        }

        // Ensure the function is terminated (if no Return was emitted).
        if !is_terminated(self.builder.get_insert_block()) {
            let zero = self.ctx.i32_type().const_int(0, false);
            self.builder.build_return(Some(&zero))?;
        }
        Ok(())
    }

    /// Emit a single user-defined function (type method or standalone).
    fn emit_function(
        &mut self,
        fn_name: &str,
        banner_fn: &BannerFunction,
        type_name: &str,
        _is_init: bool,
    ) -> CodegenResult<()> {
        self.reset_frame();
        let fv = *self
            .functions
            .get(fn_name)
            .ok_or_else(|| CodegenError::Llvm(format!("function '{fn_name}' not pre-declared")))?;

        let entry = self.ctx.append_basic_block(fv, "entry");
        self.builder.position_at_end(entry);

        // For methods, the first param is `self`; tell the inference which type it is
        // so GetField results on self can be typed via field_kind_map.
        let object_type_hints: HashMap<TempId, String> =
            if !type_name.is_empty() && !banner_fn.params.is_empty() {
                [(banner_fn.params[0], type_name.to_string())]
                    .into_iter()
                    .collect()
            } else {
                HashMap::new()
            };

        let kinds = infer_temp_kinds(
            &banner_fn.body,
            &self.fn_return_kinds.clone(),
            &self.field_kind_map.clone(),
            &object_type_hints,
        );
        self.temp_kinds.clone_from(&kinds);

        // For methods: track self param in temp_type_names so field accesses resolve.
        if !type_name.is_empty() && !banner_fn.params.is_empty() {
            self.temp_type_names
                .insert(banner_fn.params[0], type_name.to_string());
        }

        // Store each function parameter into its own alloca.
        let params_raw: Vec<_> = fv.get_params();
        for (i, &tid) in banner_fn.params.iter().enumerate() {
            let kind = kinds.get(&tid).copied().unwrap_or(TempKind::Ptr);
            let alloca = self.build_alloca_for_kind(kind, &format!("p{i}"))?;
            self.builder.build_store(alloca, params_raw[i])?;
            self.temp_slots.insert(tid, alloca);
        }

        self.create_temp_allocas(&banner_fn.body, fv)?;
        self.pre_create_label_blocks(&banner_fn.body, fv);

        for instr in &banner_fn.body {
            self.emit_instr(instr, fv, false)?;
        }

        if !is_terminated(self.builder.get_insert_block()) {
            let null = self.ptr_type().const_null();
            self.builder.build_return(Some(&null))?;
        }
        Ok(())
    }

    /// Scan instructions and create alloca slots in the entry block for every
    /// temp that is written (appears as `dst`).
    fn create_temp_allocas(
        &mut self,
        instrs: &[Instr],
        fv: FunctionValue<'ctx>,
    ) -> CodegenResult<()> {
        let entry = fv
            .get_first_basic_block()
            .ok_or_else(|| CodegenError::Llvm("function has no entry block".to_string()))?;

        // Position at the very start of the entry block so allocas precede all code.
        match entry.get_first_instruction() {
            Some(first) => self.builder.position_before(&first),
            None => self.builder.position_at_end(entry),
        }

        let mut dsts: Vec<TempId> = Vec::new();
        for instr in instrs {
            if let Some(dst) = instr_dst(instr) {
                dsts.push(dst);
            }
        }
        dsts.sort_unstable_by_key(|t| t.0);
        dsts.dedup();

        for dst in &dsts {
            if self.temp_slots.contains_key(dst) {
                continue; // Already allocated (param case).
            }
            let kind = self.temp_kinds.get(dst).copied().unwrap_or(TempKind::Ptr);
            let alloca = self.build_alloca_for_kind(kind, &format!("t{}", dst.0))?;
            self.temp_slots.insert(*dst, alloca);
        }

        // Restore position to end of entry block so subsequent instructions go there.
        self.builder.position_at_end(entry);
        Ok(())
    }

    fn build_alloca_for_kind(
        &self,
        kind: TempKind,
        name: &str,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let ptr = match kind {
            TempKind::F64 => self.builder.build_alloca(self.ctx.f64_type(), name)?,
            TempKind::I1 => self.builder.build_alloca(self.ctx.bool_type(), name)?,
            TempKind::Ptr => self.builder.build_alloca(self.ptr_type(), name)?,
        };
        Ok(ptr)
    }

    /// Pre-create LLVM basic blocks for every `Label` instruction in the body.
    fn pre_create_label_blocks(&mut self, instrs: &[Instr], fv: FunctionValue<'ctx>) {
        for instr in instrs {
            if let Instr::Label(name) = instr {
                if !self.blocks.contains_key(name) {
                    let bb = self.ctx.append_basic_block(fv, name);
                    self.blocks.insert(name.clone(), bb);
                }
            }
        }
    }
}

// ─── Instruction dispatcher ─────────────────────────────────────────────────

impl<'ctx> Codegen<'ctx> {
    pub(crate) fn emit_instr(
        &mut self,
        instr: &Instr,
        fv: FunctionValue<'ctx>,
        is_main: bool,
    ) -> CodegenResult<()> {
        match instr {
            Instr::Copy { dst, src } => self.emit_copy(*dst, src),
            Instr::BinOp {
                dst,
                op,
                left,
                right,
            } => self.emit_binop(*dst, *op, left, right),
            Instr::UnOp { dst, op, operand } => self.emit_unop(*dst, *op, operand),
            Instr::Call { dst, callee, args } => self.emit_call(*dst, callee, args),
            Instr::MethodCall {
                dst,
                receiver,
                method,
                args,
            } => self.emit_method_call(*dst, receiver, method, args),
            Instr::StaticCall {
                dst,
                type_name,
                method,
                args,
            } => self.emit_static_call(*dst, type_name, method, args),
            Instr::New {
                dst,
                type_name,
                args,
            } => self.emit_new(*dst, type_name, args),
            Instr::GetField { dst, object, field } => self.emit_get_field(*dst, object, field),
            Instr::SetField {
                object,
                field,
                value,
            } => self.emit_set_field(object, field, value),
            Instr::GetIndex { dst, target, index } => self.emit_get_index(*dst, target, index),
            Instr::SetIndex {
                target,
                index,
                value,
            } => self.emit_set_index(target, index, value),
            Instr::Label(name) => self.emit_label(name),
            Instr::Jump(label) => self.emit_jump(label),
            Instr::JumpIf { condition, label } => self.emit_jumpif(condition, label, fv),
            Instr::Return(val) => self.emit_return(val, is_main),
            Instr::ShadowPush(val) => self.emit_shadow_push(val),
            Instr::ShadowPop => self.emit_shadow_pop(),
            Instr::Alloc { dst, type_name } => self.emit_alloc(*dst, type_name),
        }
    }
}

// ─── Control-flow and scalar-copy instructions ──────────────────────────────

impl<'ctx> Codegen<'ctx> {
    fn emit_copy(&mut self, dst: TempId, src: &Value) -> CodegenResult<()> {
        let val = self.load_val(src)?;
        // Propagate type name (Range/Vector) from src temp to dst temp.
        if let Value::Temp(src_tid) = src {
            if let Some(tname) = self.temp_type_names.get(src_tid).cloned() {
                self.temp_type_names.insert(dst, tname);
            }
        }
        self.store_temp(dst, val)
    }

    fn emit_label(&mut self, name: &str) -> CodegenResult<()> {
        let bb = *self.blocks.get(name).ok_or_else(|| {
            CodegenError::Llvm(format!("label '{name}' has no pre-created basic block"))
        })?;

        // Fall-through from the previous block if it is not yet terminated.
        if !is_terminated(self.builder.get_insert_block()) {
            self.builder.build_unconditional_branch(bb)?;
        }
        self.builder.position_at_end(bb);
        Ok(())
    }

    fn emit_jump(&mut self, label: &str) -> CodegenResult<()> {
        let bb = self.get_or_err_block(label)?;
        if !is_terminated(self.builder.get_insert_block()) {
            self.builder.build_unconditional_branch(bb)?;
        }
        Ok(())
    }

    fn emit_jumpif(
        &mut self,
        condition: &Value,
        label: &str,
        fv: FunctionValue<'ctx>,
    ) -> CodegenResult<()> {
        let cond_val = self.load_val(condition)?;
        let cond_i1 = self.coerce_to_bool(cond_val)?;

        let then_bb = self.get_or_err_block(label)?;
        let cont_bb = self.ctx.append_basic_block(fv, "cont");
        self.builder
            .build_conditional_branch(cond_i1, then_bb, cont_bb)?;
        self.builder.position_at_end(cont_bb);
        Ok(())
    }

    fn emit_return(&mut self, val: &Value, is_main: bool) -> CodegenResult<()> {
        if is_main {
            // HULK main always exits with status 0.
            let zero = self.ctx.i32_type().const_int(0, false);
            if !is_terminated(self.builder.get_insert_block()) {
                self.builder.build_return(Some(&zero))?;
            }
            return Ok(());
        }
        if is_terminated(self.builder.get_insert_block()) {
            return Ok(());
        }
        let llvm_val = self.load_val(val)?;
        match llvm_val {
            LlvmVal::Void => {
                let null = self.ptr_type().const_null();
                self.builder.build_return(Some(&null))?;
            }
            other => {
                if let Some(bv) = other.as_basic() {
                    self.builder.build_return(Some(&bv))?;
                } else {
                    let null = self.ptr_type().const_null();
                    self.builder.build_return(Some(&null))?;
                }
            }
        }
        Ok(())
    }

    fn emit_shadow_push(&mut self, val: &Value) -> CodegenResult<()> {
        let v = self.load_val(val)?;
        let ptr_val = self.coerce_to_ptr(v)?;
        self.builder
            .build_call(self.rt.hulk_shadow_push, &[ptr_val.into()], "shadow_push")?;
        Ok(())
    }

    fn emit_shadow_pop(&mut self) -> CodegenResult<()> {
        self.builder
            .build_call(self.rt.hulk_shadow_pop, &[], "shadow_pop")?;
        Ok(())
    }
}

// ─── Value loading and storing ───────────────────────────────────────────────

impl<'ctx> Codegen<'ctx> {
    /// Load a BANNER `Value` operand as an `LlvmVal`.
    pub(crate) fn load_val(&mut self, val: &Value) -> CodegenResult<LlvmVal<'ctx>> {
        match val {
            Value::ConstNum(n) => Ok(LlvmVal::Float(self.ctx.f64_type().const_float(*n))),
            Value::ConstBool(b) => Ok(LlvmVal::Int(
                self.ctx.bool_type().const_int(u64::from(*b), false),
            )),
            Value::ConstNull => Ok(LlvmVal::Ptr(self.ptr_type().const_null())),
            Value::ConstStr(s) => {
                // Materialize as a GC-managed HulkStr object.
                let ptr = self.materialize_string(s)?;
                Ok(LlvmVal::Ptr(ptr))
            }
            Value::Global(name) => {
                // Global function references are used as callees; return as ptr.
                if let Some(fv) = self.functions.get(name).copied() {
                    let ptr = fv.as_global_value().as_pointer_value();
                    Ok(LlvmVal::Ptr(ptr))
                } else if let Some(fv) = self.rt.resolve_builtin(name) {
                    let ptr = fv.as_global_value().as_pointer_value();
                    Ok(LlvmVal::Ptr(ptr))
                } else {
                    Err(CodegenError::Llvm(format!("unresolved global '{name}'")))
                }
            }
            Value::Temp(tid) => self.load_temp(*tid),
        }
    }

    /// Load the current value of a BANNER temporary from its alloca slot.
    fn load_temp(&mut self, tid: TempId) -> CodegenResult<LlvmVal<'ctx>> {
        let &alloca = self.temp_slots.get(&tid).ok_or_else(|| {
            CodegenError::Llvm(format!("temporary t{} has no alloca slot", tid.0))
        })?;
        let kind = self.temp_kinds.get(&tid).copied().unwrap_or(TempKind::Ptr);
        let llvm_ty = self.kind_to_llvm_type(kind);
        let loaded = self
            .builder
            .build_load(llvm_ty, alloca, &format!("ld{}", tid.0))?;
        Ok(llvm_val_from_basic(loaded, kind))
    }

    /// Store an `LlvmVal` into the alloca slot for a given temporary.
    pub(crate) fn store_temp(&mut self, dst: TempId, val: LlvmVal<'ctx>) -> CodegenResult<()> {
        if let LlvmVal::Void = val {
            // Void calls produce no value; store null ptr so the slot is defined.
            if let Some(&alloca) = self.temp_slots.get(&dst) {
                let null = self.ptr_type().const_null();
                self.builder.build_store(alloca, null)?;
            }
            return Ok(());
        }
        let &alloca = self.temp_slots.get(&dst).ok_or_else(|| {
            CodegenError::Llvm(format!("temporary t{} has no alloca slot", dst.0))
        })?;
        let bv = val.as_basic().ok_or_else(|| {
            CodegenError::Llvm(format!("cannot store void value into t{}", dst.0))
        })?;
        self.builder.build_store(alloca, bv)?;
        Ok(())
    }

    /// Materialize a string literal as a GC-managed `HulkStr` object.
    pub(crate) fn materialize_string(&mut self, s: &str) -> CodegenResult<PointerValue<'ctx>> {
        let str_global = self.intern_str(s);
        let c_str_ptr = str_global.as_pointer_value();
        let result =
            self.builder
                .build_call(self.rt.hulk_string_new, &[c_str_ptr.into()], "str_new")?;
        let ptr = result
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodegenError::Llvm("hulk_string_new returned void".to_string()))?
            .into_pointer_value();
        Ok(ptr)
    }

    /// Convert an `LlvmVal` to an `i1` boolean, inserting a comparison if needed.
    pub(crate) fn coerce_to_bool(
        &mut self,
        val: LlvmVal<'ctx>,
    ) -> CodegenResult<inkwell::values::IntValue<'ctx>> {
        match val {
            LlvmVal::Int(i) => Ok(i),
            LlvmVal::Float(f) => {
                let zero = self.ctx.f64_type().const_float(0.0);
                Ok(self.builder.build_float_compare(
                    inkwell::FloatPredicate::ONE,
                    f,
                    zero,
                    "bool_cast",
                )?)
            }
            LlvmVal::Ptr(p) => Ok(self
                .builder
                .build_is_not_null(p, "bool_cast")
                .unwrap_or_else(|_| self.ctx.bool_type().const_int(1, false))),
            LlvmVal::Void => Ok(self.ctx.bool_type().const_int(0, false)),
        }
    }

    /// Convert an `LlvmVal` to an opaque `ptr`, bitcasting or converting as needed.
    pub(crate) fn coerce_to_ptr(
        &mut self,
        val: LlvmVal<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        match val {
            LlvmVal::Ptr(p) => Ok(p),
            LlvmVal::Void => Ok(self.ptr_type().const_null()),
            _ => {
                // Numbers and bools are not heap-allocated; shadow-pushing them is a no-op.
                Ok(self.ptr_type().const_null())
            }
        }
    }

    pub(crate) fn kind_to_llvm_type(&self, kind: TempKind) -> inkwell::types::BasicTypeEnum<'ctx> {
        match kind {
            TempKind::F64 => self.ctx.f64_type().into(),
            TempKind::I1 => self.ctx.bool_type().into(),
            TempKind::Ptr => self.ptr_type().into(),
        }
    }

    fn get_or_err_block(&self, label: &str) -> CodegenResult<BasicBlock<'ctx>> {
        self.blocks
            .get(label)
            .copied()
            .ok_or_else(|| CodegenError::Llvm(format!("label '{label}' has no basic block")))
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn llvm_val_from_basic(bv: BasicValueEnum<'_>, kind: TempKind) -> LlvmVal<'_> {
    match kind {
        TempKind::F64 => LlvmVal::Float(bv.into_float_value()),
        TempKind::I1 => LlvmVal::Int(bv.into_int_value()),
        TempKind::Ptr => LlvmVal::Ptr(bv.into_pointer_value()),
    }
}

fn is_terminated(bb: Option<BasicBlock<'_>>) -> bool {
    bb.and_then(|b| b.get_terminator()).is_some()
}

/// Returns the `TempId` written by an instruction, if any.
fn instr_dst(instr: &Instr) -> Option<TempId> {
    match instr {
        Instr::Copy { dst, .. }
        | Instr::BinOp { dst, .. }
        | Instr::UnOp { dst, .. }
        | Instr::Call { dst, .. }
        | Instr::MethodCall { dst, .. }
        | Instr::StaticCall { dst, .. }
        | Instr::New { dst, .. }
        | Instr::GetField { dst, .. }
        | Instr::GetIndex { dst, .. }
        | Instr::Alloc { dst, .. } => Some(*dst),
        _ => None,
    }
}

/// Infer the LLVM type kind for every temporary in `instrs`.
///
/// Parameters:
/// - `fn_return_kinds`: map from function/method name → return TempKind, used to
///   classify the destinations of Call/MethodCall/StaticCall instructions.
/// - `field_kind`: map from (type_name, field_name) → TempKind derived from the
///   `pointer_map`; false in pointer_map → F64, true → Ptr.
/// - `object_type_hints`: for method bodies, maps the self TempId → type name so
///   GetField results can be typed using field_kind.
///
/// The algorithm runs forward + copy-propagation + backward passes until stable.
pub(crate) fn infer_temp_kinds(
    instrs: &[Instr],
    fn_return_kinds: &HashMap<String, TempKind>,
    field_kind: &HashMap<(String, String), TempKind>,
    object_type_hints: &HashMap<TempId, String>,
) -> HashMap<TempId, TempKind> {
    let mut kinds: HashMap<TempId, TempKind> = HashMap::new();

    let mut changed = true;
    while changed {
        changed = false;

        // Forward pass: classify dsts from producer instructions.
        for instr in instrs {
            let prev = kinds.clone();
            match instr {
                Instr::BinOp { dst, op, .. } => {
                    let k = if op.is_arithmetic() {
                        TempKind::F64
                    } else {
                        TempKind::I1
                    };
                    kinds.insert(*dst, k);
                }
                Instr::UnOp { dst, op, .. } => {
                    let k = if *op == hulk_hir::UnaryOpKind::Neg {
                        TempKind::F64
                    } else {
                        TempKind::I1
                    };
                    kinds.insert(*dst, k);
                }
                Instr::Copy { dst, src } => match src {
                    // Use `entry().or_insert` so a concrete kind set on a
                    // previous pass (e.g. F64 from another branch's copy of
                    // the same dst) is not clobbered. Otherwise dual-branch
                    // Copy targets oscillate between F64 and Ptr each pass.
                    Value::ConstNum(_) => {
                        kinds.entry(*dst).or_insert(TempKind::F64);
                    }
                    Value::ConstBool(_) => {
                        kinds.entry(*dst).or_insert(TempKind::I1);
                    }
                    Value::Temp(_) => {} // handled by copy propagation below
                    _ => {
                        kinds.entry(*dst).or_insert(TempKind::Ptr);
                    }
                },
                Instr::Call { dst, callee, .. } => {
                    let k = match callee {
                        Value::Global(name) if is_math_builtin(name) => TempKind::F64,
                        Value::Global(name) => fn_return_kinds
                            .get(name.as_str())
                            .copied()
                            .unwrap_or(TempKind::Ptr),
                        _ => TempKind::Ptr,
                    };
                    kinds.entry(*dst).or_insert(k);
                }
                Instr::MethodCall { dst, method, .. } => {
                    let k = fn_return_kinds
                        .get(method.as_str())
                        .copied()
                        .unwrap_or(TempKind::Ptr);
                    kinds.entry(*dst).or_insert(k);
                }
                Instr::StaticCall {
                    dst,
                    type_name,
                    method,
                    ..
                } => {
                    let full = format!("{type_name}.{method}");
                    let k = fn_return_kinds
                        .get(full.as_str())
                        .or_else(|| fn_return_kinds.get(method.as_str()))
                        .copied()
                        .unwrap_or(TempKind::Ptr);
                    kinds.entry(*dst).or_insert(k);
                }
                Instr::GetField { dst, object, field } => {
                    let type_name = match object {
                        Value::Temp(tid) => object_type_hints.get(tid).map(String::as_str),
                        _ => None,
                    };
                    let k = type_name
                        .and_then(|tname| field_kind.get(&(tname.to_string(), field.clone())))
                        .copied()
                        .unwrap_or(TempKind::Ptr);
                    kinds.entry(*dst).or_insert(k);
                }
                Instr::GetIndex { dst, .. } => {
                    // __vec_get returns f64.
                    kinds.insert(*dst, TempKind::F64);
                }
                instr => {
                    if let Some(dst) = instr_dst(instr) {
                        kinds.entry(dst).or_insert(TempKind::Ptr);
                    }
                }
            }
            if kinds != prev {
                changed = true;
            }
        }

        // Copy propagation: propagate kind from src temp to dst temp.
        // Don't downgrade an already-concrete kind (F64/I1) to Ptr — that
        // would oscillate when the same temp has multiple definitions
        // across branches (e.g. `t = copy 0` in one arm and `t = copy ptr`
        // in the other).
        for instr in instrs {
            if let Instr::Copy {
                dst,
                src: Value::Temp(src_tid),
            } = instr
            {
                if let Some(&src_kind) = kinds.get(src_tid) {
                    let cur = kinds.get(dst).copied();
                    let promote = match (cur, src_kind) {
                        (None, _) => true,
                        (Some(TempKind::Ptr), TempKind::F64 | TempKind::I1) => true,
                        (Some(_), _) => false,
                    };
                    if promote {
                        kinds.insert(*dst, src_kind);
                        changed = true;
                    }
                }
            }
        }

        // Backward propagation: operands of arithmetic BinOp must be F64.
        for instr in instrs {
            if let Instr::BinOp {
                op, left, right, ..
            } = instr
            {
                if op.is_arithmetic() {
                    for operand in [left, right] {
                        if let Value::Temp(tid) = operand {
                            if kinds.get(tid).copied() != Some(TempKind::F64) {
                                kinds.insert(*tid, TempKind::F64);
                                changed = true;
                            }
                        }
                    }
                } else {
                    // Comparison operands: set F64 only if not yet classified.
                    for operand in [left, right] {
                        if let Value::Temp(tid) = operand {
                            if !kinds.contains_key(tid) {
                                kinds.insert(*tid, TempKind::F64);
                                changed = true;
                            }
                        }
                    }
                }
            }
            // SetField: if field is numeric, the value being stored should be F64.
            if let Instr::SetField {
                object: Value::Temp(obj_tid),
                field,
                value: Value::Temp(val_tid),
            } = instr
            {
                if let Some(tname) = object_type_hints.get(obj_tid) {
                    if let Some(&TempKind::F64) = field_kind.get(&(tname.clone(), field.clone())) {
                        if kinds.get(val_tid).copied() != Some(TempKind::F64) {
                            kinds.entry(*val_tid).or_insert(TempKind::F64);
                            changed = true;
                        }
                    }
                }
            }
            // Vector indices are always Numbers, so any temp used as an index
            // to GetIndex/SetIndex must be F64.
            let index_temp = match instr {
                Instr::GetIndex {
                    index: Value::Temp(tid),
                    ..
                }
                | Instr::SetIndex {
                    index: Value::Temp(tid),
                    ..
                } => Some(*tid),
                _ => None,
            };
            if let Some(tid) = index_temp {
                if kinds.get(&tid).copied() != Some(TempKind::F64) {
                    kinds.insert(tid, TempKind::F64);
                    changed = true;
                }
            }
        }
    }

    kinds
}

/// Infer the return kind of a function body by examining its Return instructions.
pub(crate) fn infer_return_kind(instrs: &[Instr], kinds: &HashMap<TempId, TempKind>) -> TempKind {
    for instr in instrs {
        if let Instr::Return(val) = instr {
            return match val {
                Value::ConstNum(_) => TempKind::F64,
                Value::ConstBool(_) => TempKind::I1,
                Value::Temp(tid) => kinds.get(tid).copied().unwrap_or(TempKind::Ptr),
                _ => TempKind::Ptr,
            };
        }
    }
    TempKind::Ptr
}

fn is_math_builtin(name: &str) -> bool {
    matches!(name, "sqrt" | "sin" | "cos" | "exp" | "log" | "rand")
}

// Make BinOpKind::is_arithmetic available in this module.
trait BinOpExt {
    fn is_arithmetic(&self) -> bool;
}

impl BinOpExt for BinOpKind {
    fn is_arithmetic(&self) -> bool {
        matches!(
            self,
            BinOpKind::Add
                | BinOpKind::Sub
                | BinOpKind::Mul
                | BinOpKind::Div
                | BinOpKind::Mod
                | BinOpKind::Pow
        )
    }
}
