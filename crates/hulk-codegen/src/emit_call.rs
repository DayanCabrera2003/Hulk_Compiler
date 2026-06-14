use inkwell::{
    types::BasicTypeEnum,
    values::{BasicMetadataValueEnum, CallSiteValue, FunctionValue},
};

use hulk_banner::{TempId, Value};

use crate::{
    codegen::{Codegen, LlvmVal, TempKind},
    error::{CodegenError, CodegenResult},
};

impl<'ctx> Codegen<'ctx> {
    /// Emit a direct function call instruction.
    ///
    /// The callee is resolved from the runtime functions table or from the
    /// module's declared user functions. If the callee is `print`, we dispatch
    /// to the correct typed variant based on the argument's kind.
    pub(crate) fn emit_call(
        &mut self,
        dst: TempId,
        callee: &Value,
        args: &[Value],
    ) -> CodegenResult<()> {
        // Intercept the HULK `print` builtin so it dispatches to the right
        // runtime function based on the argument type.
        if let Value::Global(name) = callee {
            if name == "print" {
                return self.emit_print_dispatch(dst, args);
            }
        }

        let (fv, llvm_args) = self.resolve_call(callee, args)?;
        let call_site = self.builder.build_call(fv, &llvm_args, "call")?;
        let val = extract_call_result(call_site);
        // Track type names for the builtin range/vector so method dispatch can
        // use direct C-runtime calls. We use sentinel names ($range / $vector)
        // to avoid colliding with user-defined types of the same name.
        if let Value::Global(name) = callee {
            let type_hint = match name.as_str() {
                "range" => Some("$range".to_string()),
                "__vec_new" | "__arr_new" => Some("$vector".to_string()),
                "__objarr_new" => Some("$objarr".to_string()),
                // `__hulk_as` returns a value of the requested target type,
                // which is the second argument (a Global naming that type).
                "__hulk_as" => match args.get(1) {
                    Some(Value::Global(target)) => Some(target.clone()),
                    _ => None,
                },
                _ => self.fn_return_struct.get(name).cloned(),
            };
            if let Some(hint) = type_hint {
                self.temp_type_names.insert(dst, hint);
            }
        }
        self.store_temp(dst, val)
    }

    /// Emit a virtual method call through the object's vtable.
    ///
    /// Layout assumption: every heap object's first field (index 0 in the
    /// payload struct) holds a pointer to the type's vtable constant array.
    /// The vtable is an array of function pointers in the global method order
    /// computed by `ProgramLayout`.
    pub(crate) fn emit_method_call(
        &mut self,
        dst: TempId,
        receiver: &Value,
        method: &str,
        args: &[Value],
    ) -> CodegenResult<()> {
        // For Range and Vector (C runtime types with no HULK vtable), dispatch
        // directly to the corresponding C helper functions.
        if let Value::Temp(recv_tid) = receiver {
            if let Some(type_name) = self.temp_type_names.get(recv_tid).cloned() {
                match (type_name.as_str(), method) {
                    ("$range", "next") => {
                        return self.emit_builtin_method_i32(dst, receiver, self.rt.range_next)
                    }
                    ("$range", "current") => {
                        return self.emit_builtin_method_f64(dst, receiver, self.rt.range_current)
                    }
                    ("$vector", "next") => {
                        return self.emit_builtin_method_i32(dst, receiver, self.rt.vec_next)
                    }
                    ("$vector", "current") => {
                        return self.emit_builtin_method_f64(dst, receiver, self.rt.vec_current)
                    }
                    ("$vector", "size") => {
                        return self.emit_builtin_method_f64(dst, receiver, self.rt.vec_size)
                    }
                    ("$objarr", "size") => {
                        return self.emit_builtin_method_f64(dst, receiver, self.rt.objarr_size)
                    }
                    ("$string", "size") => {
                        return self.emit_builtin_method_f64(dst, receiver, self.rt.str_size)
                    }
                    ("$string", "charAt") => {
                        return self.emit_string_builtin_arg_ptr(
                            dst,
                            receiver,
                            args,
                            self.rt.str_char_at,
                            1,
                        );
                    }
                    ("$string", "substring") => {
                        return self.emit_string_builtin_arg_ptr(
                            dst,
                            receiver,
                            args,
                            self.rt.str_substring,
                            2,
                        );
                    }
                    _ => {}
                }
            }
        }

        let method_idx = self.layout.vtable_index(method).ok_or_else(|| {
            CodegenError::Llvm(format!(
                "method '{method}' not found in global method table"
            ))
        })?;

        let recv_val = self.load_val(receiver)?;
        let recv_ptr = self.coerce_to_ptr(recv_val)?;

        // Load vtable pointer from first field (GEP index 0 in the opaque ptr layout).
        // We treat the object as an array of `ptr` for field access.
        let ptr_ty = self.ptr_type();
        let vtable_field_ptr = unsafe {
            self.builder.build_gep(
                ptr_ty,
                recv_ptr,
                &[self.ctx.i32_type().const_int(0, false)],
                "vtable_field_ptr",
            )?
        };
        let vtable_ptr = self
            .builder
            .build_load(ptr_ty, vtable_field_ptr, "vtable")?
            .into_pointer_value();

        // Load the specific method pointer from the vtable at index `method_idx`.
        let method_slot = unsafe {
            self.builder.build_gep(
                ptr_ty,
                vtable_ptr,
                &[self.ctx.i64_type().const_int(method_idx as u64, false)],
                "method_slot",
            )?
        };
        let method_fn_ptr = self
            .builder
            .build_load(ptr_ty, method_slot, "method_fn")?
            .into_pointer_value();

        // Build the indirect call: self + args, all passed as opaque pointers.
        // Each arg must be coerced to ptr because the indirect call type below
        // declares all params as `ptr` (we don't know the concrete signature
        // through the vtable). Without coercion, passing a double directly
        // produces an LLVM verification error.
        let mut llvm_args: Vec<BasicMetadataValueEnum<'ctx>> = vec![recv_ptr.into()];
        for arg in args {
            let v = self.load_val(arg)?;
            llvm_args.push(coerce_val_to_param(v, Some(ptr_ty.into()), self)?);
        }

        // Determine the return type from fn_return_kinds. Prefer the
        // qualified `<TypeName>.<method>` entry when the receiver's static
        // type is known, so two different methods sharing the same short
        // name (e.g. a user protocol's `invoke` returning a struct vs a
        // lambda's synthesised `invoke` returning Boolean) don't collide.
        let recv_type_name: Option<String> = match receiver {
            Value::Temp(tid) => self.temp_type_names.get(tid).cloned(),
            _ => None,
        };
        let ret_kind = recv_type_name
            .as_ref()
            .and_then(|tn| self.fn_return_kinds.get(&format!("{tn}.{method}")).copied())
            .or_else(|| self.fn_return_kinds.get(method).copied())
            .unwrap_or(TempKind::Ptr);
        let n_params = llvm_args.len();
        let params: Vec<_> = (0..n_params).map(|_| ptr_ty.into()).collect();
        let fn_ty = match ret_kind {
            TempKind::F64 => self.ctx.f64_type().fn_type(&params, false),
            TempKind::I1 => self.ctx.bool_type().fn_type(&params, false),
            TempKind::Ptr => ptr_ty.fn_type(&params, false),
        };

        let call_site =
            self.builder
                .build_indirect_call(fn_ty, method_fn_ptr, &llvm_args, "vcall")?;
        let result = extract_call_result(call_site);
        let struct_hint = recv_type_name
            .as_ref()
            .and_then(|tn| {
                self.fn_return_struct
                    .get(&format!("{tn}.{method}"))
                    .cloned()
            })
            .or_else(|| self.fn_return_struct.get(method).cloned());
        if let Some(struct_name) = struct_hint {
            self.temp_type_names.insert(dst, struct_name);
        }
        self.store_temp(dst, result)
    }

    /// Emit a static (non-virtual) call to a known type's method.
    ///
    /// Used for constructor calls (`__init__`) and for `base()` super-dispatch.
    pub(crate) fn emit_static_call(
        &mut self,
        dst: TempId,
        type_name: &str,
        method: &str,
        args: &[Value],
    ) -> CodegenResult<()> {
        let fn_name = format!("{type_name}.{method}");
        let fv = *self.functions.get(&fn_name).ok_or_else(|| {
            CodegenError::Llvm(format!("static call target '{fn_name}' not declared"))
        })?;

        let param_types: Vec<BasicTypeEnum<'ctx>> = fv.get_type().get_param_types();
        let mut llvm_args: Vec<BasicMetadataValueEnum<'ctx>> = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let v = self.load_val(arg)?;
            let expected_ty = param_types.get(i).copied();
            let coerced = coerce_val_to_param(v, expected_ty, self)?;
            llvm_args.push(coerced);
        }

        let call_site = self.builder.build_call(fv, &llvm_args, "scall")?;
        let result = extract_call_result(call_site);
        if let Some(struct_name) = self.fn_return_struct.get(&fn_name).cloned() {
            self.temp_type_names.insert(dst, struct_name);
        }
        self.store_temp(dst, result)
    }

    // ─── Private helpers ──────────────────────────────────────────────────

    /// Resolve a callee `Value` to a `FunctionValue` and evaluate arguments,
    /// coercing each argument to match the callee's declared LLVM parameter type.
    fn resolve_call(
        &mut self,
        callee: &Value,
        args: &[Value],
    ) -> CodegenResult<(FunctionValue<'ctx>, Vec<BasicMetadataValueEnum<'ctx>>)> {
        let callee_name = match callee {
            Value::Global(name) => Some(name.clone()),
            _ => None,
        };
        let fv = match callee {
            Value::Global(name) => {
                if let Some(rt_fv) = self.rt.resolve_builtin(name) {
                    rt_fv
                } else {
                    *self
                        .functions
                        .get(name)
                        .ok_or_else(|| CodegenError::Llvm(format!("unresolved callee '{name}'")))?
                }
            }
            other => {
                let v = self.load_val(other)?;
                let ptr = self.coerce_to_ptr(v)?;
                return self.emit_indirect_call(ptr, args);
            }
        };

        // __hulk_concat takes two strings: any Number/Boolean argument must be
        // converted via hulk_number_to_string before passing. The default
        // bitcast coercion would corrupt numeric values into bogus pointers
        // and segfault inside the runtime.
        let is_concat = callee_name.as_deref() == Some("__hulk_concat");

        let param_types: Vec<BasicTypeEnum<'ctx>> = fv.get_type().get_param_types();
        let mut llvm_args: Vec<BasicMetadataValueEnum<'ctx>> = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let v = self.load_val(arg)?;
            let coerced = if is_concat {
                self.coerce_val_for_concat(v)?
            } else {
                let expected_ty = param_types.get(i).copied();
                coerce_val_to_param(v, expected_ty, self)?
            };
            llvm_args.push(coerced);
        }
        Ok((fv, llvm_args))
    }

    /// Coerce a value to a `ptr` for `__hulk_concat`, calling
    /// `hulk_number_to_string` if the value is an unboxed Number/Boolean.
    fn coerce_val_for_concat(
        &mut self,
        val: LlvmVal<'ctx>,
    ) -> CodegenResult<BasicMetadataValueEnum<'ctx>> {
        match val {
            LlvmVal::Float(f) => {
                let call =
                    self.builder
                        .build_call(self.rt.hulk_number_to_string, &[f.into()], "ntos")?;
                let ptr = call
                    .try_as_basic_value()
                    .left()
                    .ok_or_else(|| {
                        CodegenError::Llvm("hulk_number_to_string returned void".to_string())
                    })?
                    .into_pointer_value();
                Ok(ptr.into())
            }
            LlvmVal::Int(i) => {
                // Booleans: convert via i64 → f64 via uitofp, then to_string.
                let f = self
                    .builder
                    .build_unsigned_int_to_float(i, self.ctx.f64_type(), "btof")?;
                let call =
                    self.builder
                        .build_call(self.rt.hulk_number_to_string, &[f.into()], "ntos")?;
                let ptr = call
                    .try_as_basic_value()
                    .left()
                    .ok_or_else(|| {
                        CodegenError::Llvm("hulk_number_to_string returned void".to_string())
                    })?
                    .into_pointer_value();
                Ok(ptr.into())
            }
            LlvmVal::Ptr(p) => Ok(p.into()),
            LlvmVal::Void => Ok(self.ptr_type().const_null().into()),
        }
    }

    fn emit_indirect_call(
        &mut self,
        fn_ptr: inkwell::values::PointerValue<'ctx>,
        args: &[Value],
    ) -> CodegenResult<(FunctionValue<'ctx>, Vec<BasicMetadataValueEnum<'ctx>>)> {
        // Indirect calls via function pointer are handled separately in emit_call.
        // This path should rarely be taken in well-formed programs.
        let _ = (fn_ptr, args);
        Err(CodegenError::Llvm(
            "indirect call through non-global callee not yet supported".to_string(),
        ))
    }

    /// Call a C function that takes `(ptr receiver)` and returns `i32` (used as bool).
    fn emit_builtin_method_i32(
        &mut self,
        dst: TempId,
        receiver: &Value,
        fv: inkwell::values::FunctionValue<'ctx>,
    ) -> CodegenResult<()> {
        let recv_val = self.load_val(receiver)?;
        let recv_ptr = self.coerce_to_ptr(recv_val)?;
        let call = self.builder.build_call(fv, &[recv_ptr.into()], "bm_i32")?;
        let i32_val = call
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodegenError::Llvm("builtin method returned void".to_string()))?
            .into_int_value();
        // Truncate i32 → i1 for boolean semantics.
        let i1_val = self
            .builder
            .build_int_truncate(i32_val, self.ctx.bool_type(), "bm_i1")?;
        self.store_temp(dst, LlvmVal::Int(i1_val))
    }

    /// Call a string-method runtime helper that takes `(ptr receiver, ...n
    /// double args)` and returns a `ptr` (a fresh HulkStr). Coerces each
    /// arg to `f64` and tags the result with `$string` so further chained
    /// calls like `s.substring(0, 3).size()` dispatch correctly.
    fn emit_string_builtin_arg_ptr(
        &mut self,
        dst: TempId,
        receiver: &Value,
        args: &[Value],
        fv: inkwell::values::FunctionValue<'ctx>,
        expected_args: usize,
    ) -> CodegenResult<()> {
        if args.len() != expected_args {
            return Err(CodegenError::Llvm(format!(
                "string builtin expected {expected_args} arg(s), got {}",
                args.len()
            )));
        }
        let recv_val = self.load_val(receiver)?;
        let recv_ptr = self.coerce_to_ptr(recv_val)?;
        let mut llvm_args: Vec<BasicMetadataValueEnum<'ctx>> = vec![recv_ptr.into()];
        for arg in args {
            let v = self.load_val(arg)?;
            let f = self.coerce_to_f64(v)?;
            llvm_args.push(f.into());
        }
        let call = self.builder.build_call(fv, &llvm_args, "str_bm")?;
        let ptr = call
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodegenError::Llvm("string builtin returned void".to_string()))?
            .into_pointer_value();
        self.temp_type_names.insert(dst, "$string".to_owned());
        self.store_temp(dst, LlvmVal::Ptr(ptr))
    }

    /// Call a C function that takes `(ptr receiver)` and returns `double`.
    fn emit_builtin_method_f64(
        &mut self,
        dst: TempId,
        receiver: &Value,
        fv: inkwell::values::FunctionValue<'ctx>,
    ) -> CodegenResult<()> {
        let recv_val = self.load_val(receiver)?;
        let recv_ptr = self.coerce_to_ptr(recv_val)?;
        let call = self.builder.build_call(fv, &[recv_ptr.into()], "bm_f64")?;
        let f_val = call
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodegenError::Llvm("builtin method returned void".to_string()))?
            .into_float_value();
        self.store_temp(dst, LlvmVal::Float(f_val))
    }

    /// Dispatch a `print` call to the correct typed runtime function.
    ///
    /// - ConstNum argument → `hulk_print_number`
    /// - ConstBool argument → `hulk_print_bool`
    /// - String / reference → materialize and call `hulk_print`
    fn emit_print_dispatch(&mut self, dst: TempId, args: &[Value]) -> CodegenResult<()> {
        let arg = args
            .first()
            .ok_or_else(|| CodegenError::Llvm("print called with no arguments".to_string()))?;

        let val = self.load_val(arg)?;
        match &val {
            LlvmVal::Float(f) => {
                let f = *f;
                self.builder
                    .build_call(self.rt.hulk_print_number, &[f.into()], "print_num")?;
            }
            LlvmVal::Int(i) => {
                // Extend i1 → i32 for hulk_print_bool(int b).
                let i = *i;
                let i32_val =
                    self.builder
                        .build_int_z_extend(i, self.ctx.i32_type(), "bool_ext")?;
                self.builder.build_call(
                    self.rt.hulk_print_bool,
                    &[i32_val.into()],
                    "print_bool",
                )?;
            }
            _ => {
                let ptr = self.coerce_to_ptr(val)?;
                self.builder
                    .build_call(self.rt.hulk_print, &[ptr.into()], "print_ref")?;
            }
        }
        // `print` returns void; store null ptr so the dest temp is defined.
        let null = self.ptr_type().const_null();
        let slot = self.temp_slots.get(&dst).copied();
        if let Some(alloca) = slot {
            self.builder.build_store(alloca, null)?;
        }
        Ok(())
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Extract the return value of a `CallSiteValue`, or `LlvmVal::Void` if void.
fn extract_call_result(call_site: CallSiteValue<'_>) -> LlvmVal<'_> {
    match call_site.try_as_basic_value().left() {
        Some(bv) => match bv {
            inkwell::values::BasicValueEnum::FloatValue(f) => LlvmVal::Float(f),
            inkwell::values::BasicValueEnum::IntValue(i) => LlvmVal::Int(i),
            inkwell::values::BasicValueEnum::PointerValue(p) => LlvmVal::Ptr(p),
            _ => LlvmVal::Void,
        },
        None => LlvmVal::Void,
    }
}

/// Convert an `LlvmVal` to a `BasicMetadataValueEnum` for use as a call argument.
fn val_to_metadata<'ctx>(
    val: LlvmVal<'ctx>,
    cg: &mut Codegen<'ctx>,
) -> CodegenResult<BasicMetadataValueEnum<'ctx>> {
    Ok(match val {
        LlvmVal::Float(f) => f.into(),
        LlvmVal::Int(i) => i.into(),
        LlvmVal::Ptr(p) => p.into(),
        LlvmVal::Void => cg.ptr_type().const_null().into(),
    })
}

/// Coerce `val` to the `expected_ty` LLVM type for use as a function argument.
///
/// - Float → ptr: bitcast f64 bits to i64, then inttoptr.
/// - Int   → ptr: zero-extend i1/i64 to i64, then inttoptr.
/// - Ptr   → f64: ptrtoint to i64, then bitcast to f64.
/// - Same type: pass through unchanged.
fn coerce_val_to_param<'ctx>(
    val: LlvmVal<'ctx>,
    expected_ty: Option<BasicTypeEnum<'ctx>>,
    cg: &mut Codegen<'ctx>,
) -> CodegenResult<BasicMetadataValueEnum<'ctx>> {
    let Some(expected) = expected_ty else {
        return val_to_metadata(val, cg);
    };
    let ptr_ty: BasicTypeEnum = cg.ptr_type().into();
    let f64_ty: BasicTypeEnum = cg.ctx.f64_type().into();
    let i64_ty = cg.ctx.i64_type();

    match (&val, expected) {
        // Float → ptr: bitcast bits to i64, then inttoptr.
        (LlvmVal::Float(f), t) if t == ptr_ty => {
            let f = *f;
            let bits = cg.builder.build_bitcast(f, i64_ty, "fbits_to_i64")?;
            let ptr = cg.builder.build_int_to_ptr(
                bits.into_int_value(),
                cg.ptr_type(),
                "fbits_as_ptr",
            )?;
            Ok(ptr.into())
        }
        // Int → ptr: zero-extend, then inttoptr.
        (LlvmVal::Int(i), t) if t == ptr_ty => {
            let i = *i;
            let ext = cg.builder.build_int_z_extend(i, i64_ty, "ibits_to_i64")?;
            let ptr = cg
                .builder
                .build_int_to_ptr(ext, cg.ptr_type(), "ibits_as_ptr")?;
            Ok(ptr.into())
        }
        // Ptr → f64: ptrtoint to i64, then bitcast to f64.
        (LlvmVal::Ptr(p), t) if t == f64_ty => {
            let p = *p;
            let bits = cg.builder.build_ptr_to_int(p, i64_ty, "ptr_to_i64")?;
            let f = cg
                .builder
                .build_bitcast(bits, cg.ctx.f64_type(), "i64_as_f64")?;
            Ok(f.into())
        }
        _ => val_to_metadata(val, cg),
    }
}
