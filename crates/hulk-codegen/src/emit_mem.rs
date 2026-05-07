use inkwell::{module::Linkage, types::BasicTypeEnum, values::GlobalValue, AddressSpace};

use hulk_banner::{TempId, Value};

use crate::{
    codegen::{Codegen, LlvmVal, TempKind},
    error::{CodegenError, CodegenResult},
};

impl<'ctx> Codegen<'ctx> {
    /// Emit a `New { type_name, args }` instruction.
    ///
    /// Steps:
    /// 1. Get the LLVM struct type for `type_name`.
    /// 2. Compute the payload size with a GEP-based sizeof trick.
    /// 3. Call `hulk_alloc(TypeTag*, size)` to allocate the object.
    /// 4. Store the vtable pointer into field 0.
    /// 5. Call `TypeName.__init__(self, args...)` to initialize fields.
    pub(crate) fn emit_new(
        &mut self,
        dst: TempId,
        type_name: &str,
        args: &[Value],
    ) -> CodegenResult<()> {
        let struct_ty = *self
            .struct_types
            .get(type_name)
            .ok_or_else(|| CodegenError::Llvm(format!("struct type '{type_name}' not built")))?;
        let tag_global = *self
            .vtable_globals
            .get(&format!("{type_name}_tag"))
            .ok_or_else(|| CodegenError::Llvm(format!("TypeTag for '{type_name}' not built")))?;

        // Compute sizeof(struct_type) using the null-GEP trick.
        let size = self.compute_sizeof(struct_ty)?;
        let tag_ptr = tag_global.as_pointer_value();

        // Allocate heap memory for the object.
        let obj_ptr_call =
            self.builder
                .build_call(self.rt.hulk_alloc, &[tag_ptr.into(), size.into()], "alloc")?;
        let obj_ptr = obj_ptr_call
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodegenError::Llvm("hulk_alloc returned void".to_string()))?
            .into_pointer_value();

        // Store vtable pointer into field 0.
        let vtable_global = *self.vtable_globals.get(type_name).ok_or_else(|| {
            CodegenError::Llvm(format!("vtable global for '{type_name}' not built"))
        })?;
        let vtable_ptr = vtable_global.as_pointer_value();

        let vtable_field = unsafe {
            self.builder.build_gep(
                self.ptr_type(),
                obj_ptr,
                &[self.ctx.i32_type().const_int(0, false)],
                "vtable_field",
            )?
        };
        self.builder.build_store(vtable_field, vtable_ptr)?;

        // Invoke the constructor, coercing each arg to the declared param type.
        let init_name = format!("{type_name}.__init__");
        let init_fv = *self
            .functions
            .get(&init_name)
            .ok_or_else(|| CodegenError::Llvm(format!("constructor '{init_name}' not declared")))?;

        let init_param_types: Vec<BasicTypeEnum<'ctx>> = init_fv.get_type().get_param_types();
        let mut init_args: Vec<inkwell::values::BasicMetadataValueEnum<'ctx>> =
            vec![obj_ptr.into()];
        for (i, arg) in args.iter().enumerate() {
            let v = self.load_val(arg)?;
            let expected_ty = init_param_types.get(i + 1).copied(); // +1: skip self
            let coerced = coerce_to_param(v, expected_ty, self)?;
            init_args.push(coerced);
        }
        self.builder.build_call(init_fv, &init_args, "init")?;

        // Record that this temp holds an object of `type_name` (for field lookups).
        self.temp_type_names.insert(dst, type_name.to_string());
        self.store_temp(dst, LlvmVal::Ptr(obj_ptr))
    }

    /// Emit a `GetField { dst, object, field }` instruction.
    ///
    /// Fields are always stored as opaque `ptr` in the struct layout (8 bytes).
    /// If the field holds a numeric value (TempKind::F64 for dst), the loaded ptr
    /// is re-interpreted as f64 via ptrtoint → bitcast.
    pub(crate) fn emit_get_field(
        &mut self,
        dst: TempId,
        object: &Value,
        field: &str,
    ) -> CodegenResult<()> {
        let obj_val = self.load_val(object)?;
        let obj_ptr = self.coerce_to_ptr(obj_val)?;

        let (struct_ty, field_idx) = self.resolve_field(object, field)?;
        let field_ptr = self
            .builder
            .build_struct_gep(struct_ty, obj_ptr, field_idx, "fld_ptr")?;
        let loaded_ptr = self
            .builder
            .build_load(self.ptr_type(), field_ptr, field)?
            .into_pointer_value();

        let dst_kind = self.temp_kinds.get(&dst).copied().unwrap_or(TempKind::Ptr);
        match dst_kind {
            TempKind::F64 => {
                let i64_ty = self.ctx.i64_type();
                let bits = self.builder.build_ptr_to_int(loaded_ptr, i64_ty, "ptoi")?;
                let f = self
                    .builder
                    .build_bitcast(bits, self.ctx.f64_type(), "cast_f64")?;
                self.store_temp(dst, LlvmVal::Float(f.into_float_value()))
            }
            TempKind::I1 => {
                let i64_ty = self.ctx.i64_type();
                let bits = self.builder.build_ptr_to_int(loaded_ptr, i64_ty, "ptoi")?;
                let b = self
                    .builder
                    .build_int_truncate(bits, self.ctx.bool_type(), "cast_i1")?;
                self.store_temp(dst, LlvmVal::Int(b))
            }
            TempKind::Ptr => self.store_temp(dst, LlvmVal::Ptr(loaded_ptr)),
        }
    }

    /// Emit a `SetField { object, field, value }` instruction.
    ///
    /// Numeric values (Float/Int) are bit-cast to an opaque pointer before storing
    /// so all field slots have a uniform `ptr` layout.
    pub(crate) fn emit_set_field(
        &mut self,
        object: &Value,
        field: &str,
        value: &Value,
    ) -> CodegenResult<()> {
        let obj_val = self.load_val(object)?;
        let obj_ptr = self.coerce_to_ptr(obj_val)?;
        let (struct_ty, field_idx) = self.resolve_field(object, field)?;
        let field_ptr = self
            .builder
            .build_struct_gep(struct_ty, obj_ptr, field_idx, "fld_ptr")?;
        let v = self.load_val(value)?;
        let i64_ty = self.ctx.i64_type();
        let stored: inkwell::values::BasicValueEnum<'ctx> = match v {
            LlvmVal::Float(f) => {
                // Bitcast f64 bits → i64 → ptr so numeric fields fit in a ptr slot.
                let bits = self.builder.build_bitcast(f, i64_ty, "fbits")?;
                self.builder
                    .build_int_to_ptr(bits.into_int_value(), self.ptr_type(), "fptr")?
                    .into()
            }
            LlvmVal::Int(i) => {
                let ext = self.builder.build_int_z_extend(i, i64_ty, "ibits")?;
                self.builder
                    .build_int_to_ptr(ext, self.ptr_type(), "iptr")?
                    .into()
            }
            LlvmVal::Ptr(p) => p.into(),
            LlvmVal::Void => self.ptr_type().const_null().into(),
        };
        self.builder.build_store(field_ptr, stored)?;
        Ok(())
    }

    /// Emit a `GetIndex { dst, target, index }` instruction.
    ///
    /// Vectors are represented as `{ ptr data, f64 len, f64 capacity }` objects.
    /// Index access is forwarded to `__vec_get` which returns f64.
    pub(crate) fn emit_get_index(
        &mut self,
        dst: TempId,
        target: &Value,
        index: &Value,
    ) -> CodegenResult<()> {
        let tv = self.load_val(target)?;
        let iv = self.load_val(index)?;
        let vec_ptr = self.coerce_to_ptr(tv)?;
        let idx_float = match iv {
            LlvmVal::Float(f) => f,
            _ => return Err(CodegenError::Llvm("vector index must be f64".to_string())),
        };

        let vec_get = self.rt.vec_get;
        let call =
            self.builder
                .build_call(vec_get, &[vec_ptr.into(), idx_float.into()], "vec_get")?;
        let f = call
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodegenError::Llvm("__vec_get returned void".to_string()))?
            .into_float_value();
        self.store_temp(dst, LlvmVal::Float(f))
    }

    /// Emit a `SetIndex { target, index, value }` instruction.
    pub(crate) fn emit_set_index(
        &mut self,
        target: &Value,
        index: &Value,
        value: &Value,
    ) -> CodegenResult<()> {
        let tv = self.load_val(target)?;
        let iv = self.load_val(index)?;
        let vv = self.load_val(value)?;
        let vec_ptr = self.coerce_to_ptr(tv)?;
        let idx_float = match iv {
            LlvmVal::Float(f) => f,
            _ => return Err(CodegenError::Llvm("vector index must be f64".to_string())),
        };
        let val_ptr = self.coerce_to_ptr(vv)?;

        let vec_set = self.get_or_declare_vec_set();
        self.builder.build_call(
            vec_set,
            &[vec_ptr.into(), idx_float.into(), val_ptr.into()],
            "vec_set",
        )?;
        Ok(())
    }

    /// Emit an `Alloc { dst, type_name }` instruction (raw allocation, no init).
    pub(crate) fn emit_alloc(&mut self, dst: TempId, type_name: &str) -> CodegenResult<()> {
        let struct_ty = *self.struct_types.get(type_name).ok_or_else(|| {
            CodegenError::Llvm(format!("struct type '{type_name}' not built for Alloc"))
        })?;
        let tag_global = *self
            .vtable_globals
            .get(&format!("{type_name}_tag"))
            .ok_or_else(|| {
                CodegenError::Llvm(format!("TypeTag for '{type_name}' not built for Alloc"))
            })?;
        let size = self.compute_sizeof(struct_ty)?;
        let tag_ptr = tag_global.as_pointer_value();
        let call = self.builder.build_call(
            self.rt.hulk_alloc,
            &[tag_ptr.into(), size.into()],
            "raw_alloc",
        )?;
        let ptr = call
            .try_as_basic_value()
            .left()
            .ok_or_else(|| CodegenError::Llvm("hulk_alloc returned void".to_string()))?
            .into_pointer_value();
        self.temp_type_names.insert(dst, type_name.to_string());
        self.store_temp(dst, LlvmVal::Ptr(ptr))
    }
}

// ─── TypeTag and vtable global builders ─────────────────────────────────────

impl<'ctx> Codegen<'ctx> {
    /// Build a `TypeTag` global constant for `type_name`.
    ///
    /// Layout (matches C's `TypeTag` struct): `{ ptr name, i64 num_ptrs, ptr offsets }`.
    /// The `pointer_map` slice determines which fields hold GC-traced references.
    pub fn build_type_tag(&mut self, type_name: &str, pointer_map: &[bool]) {
        // String constant for the type name.
        let tag_name_global = self.intern_str(type_name);
        let name_ptr = tag_name_global.as_pointer_value();

        // Count pointer fields (all user fields at offset (i+1)*8, where i is 0-indexed).
        let ptr_offsets: Vec<u64> = pointer_map
            .iter()
            .enumerate()
            .filter_map(|(i, &is_ptr)| {
                if is_ptr {
                    Some((i as u64 + 1) * 8)
                } else {
                    None
                }
            })
            .collect();

        let num_ptrs = ptr_offsets.len() as u64;
        let i64_t = self.ctx.i64_type();
        let ptr_t = self.ptr_type();

        // Build the pointer_offsets array global (or null if no pointers).
        let offsets_ptr = if ptr_offsets.is_empty() {
            ptr_t.const_null()
        } else {
            let arr: Vec<_> = ptr_offsets
                .iter()
                .map(|&o| i64_t.const_int(o, false))
                .collect();
            let arr_global = self.module.add_global(
                i64_t.array_type(arr.len() as u32),
                Some(AddressSpace::default()),
                &format!("{type_name}_ptr_offsets"),
            );
            arr_global.set_initializer(&i64_t.const_array(&arr));
            arr_global.set_constant(true);
            arr_global.set_linkage(Linkage::Private);
            arr_global.as_pointer_value()
        };

        // Build the TypeTag struct global: { ptr, i64, ptr }
        let tag_struct_ty = self
            .ctx
            .struct_type(&[ptr_t.into(), i64_t.into(), ptr_t.into()], false);
        let tag_global = self.module.add_global(
            tag_struct_ty,
            Some(AddressSpace::default()),
            &format!("{type_name}_tag"),
        );
        let tag_init = tag_struct_ty.const_named_struct(&[
            name_ptr.into(),
            i64_t.const_int(num_ptrs, false).into(),
            offsets_ptr.into(),
        ]);
        tag_global.set_initializer(&tag_init);
        tag_global.set_constant(true);
        tag_global.set_linkage(Linkage::Private);

        self.vtable_globals
            .insert(format!("{type_name}_tag"), tag_global);
    }

    /// Build a vtable constant global for `type_name`.
    ///
    /// The vtable is an array of function pointers in the global method order.
    /// Each slot holds the function implementing that method for this type,
    /// or a null pointer if not implemented by this type or any ancestor.
    pub fn build_vtable_global(&mut self, type_name: &str, method_impls: &[String]) {
        let ptr_t = self.ptr_type();
        let n = method_impls.len();

        let entries: Vec<inkwell::values::BasicValueEnum<'ctx>> = method_impls
            .iter()
            .map(|impl_name| {
                if impl_name.is_empty() {
                    ptr_t.const_null().into()
                } else if let Some(&fv) = self.functions.get(impl_name) {
                    fv.as_global_value().as_pointer_value().into()
                } else {
                    ptr_t.const_null().into()
                }
            })
            .collect();

        let arr_ty = ptr_t.array_type(n as u32);
        let vtable_global =
            self.module
                .add_global(arr_ty, Some(AddressSpace::default()), type_name);
        vtable_global.set_linkage(Linkage::Private);
        vtable_global.set_constant(true);

        // LLVM requires a ConstantArray initializer from pointer values.
        let ptr_vals: Vec<_> = entries.iter().map(|bv| bv.into_pointer_value()).collect();
        let init = ptr_t.const_array(&ptr_vals);
        vtable_global.set_initializer(&init);

        self.vtable_globals
            .insert(type_name.to_string(), vtable_global);
    }
}

// ─── Private helpers ─────────────────────────────────────────────────────────

impl<'ctx> Codegen<'ctx> {
    /// Compute `sizeof(struct_ty)` as an `i64` LLVM value using the null-GEP trick.
    fn compute_sizeof(
        &self,
        struct_ty: inkwell::types::StructType<'ctx>,
    ) -> CodegenResult<inkwell::values::IntValue<'ctx>> {
        let size = struct_ty.size_of().ok_or_else(|| {
            CodegenError::Llvm("cannot compute sizeof for struct type".to_string())
        })?;
        // size_of() returns i64 on 64-bit targets.
        Ok(size)
    }

    /// Resolve the struct type and GEP field index for a field access.
    fn resolve_field(
        &self,
        object: &Value,
        field: &str,
    ) -> CodegenResult<(inkwell::types::StructType<'ctx>, u32)> {
        // Try to resolve from `temp_type_names` for temps produced by `New`.
        let type_name = match object {
            Value::Temp(tid) => self.temp_type_names.get(tid).cloned(),
            _ => None,
        };

        if let Some(ref tname) = type_name {
            if let Some(&st) = self.struct_types.get(tname) {
                if let Some(idx) = self.layout.field_index(tname, field) {
                    return Ok((st, idx));
                }
            }
        }

        Err(CodegenError::Llvm(format!(
            "cannot resolve field '{field}' on object — struct type not statically known"
        )))
    }

    fn get_or_declare_vec_set(&mut self) -> inkwell::values::FunctionValue<'ctx> {
        let name = "__vec_set";
        if let Some(&fv) = self.functions.get(name) {
            return fv;
        }
        let f64_t = self.ctx.f64_type();
        let ptr_t = self.ptr_type();
        let void_t = self.ctx.void_type();
        let fn_ty = void_t.fn_type(&[ptr_t.into(), f64_t.into(), ptr_t.into()], false);
        let fv = self.module.add_function(name, fn_ty, None);
        self.functions.insert(name.to_string(), fv);
        fv
    }

    /// Returns the `GlobalValue` for a global variable in the module, if it exists.
    #[allow(dead_code)]
    pub(crate) fn get_global(&self, name: &str) -> Option<GlobalValue<'ctx>> {
        self.module.get_global(name)
    }
}

/// Coerce `val` to `expected_ty` for use as a function argument.
/// Mirrors the coerce_val_to_param logic in emit_call.rs.
fn coerce_to_param<'ctx>(
    val: crate::codegen::LlvmVal<'ctx>,
    expected_ty: Option<BasicTypeEnum<'ctx>>,
    cg: &mut crate::codegen::Codegen<'ctx>,
) -> crate::error::CodegenResult<inkwell::values::BasicMetadataValueEnum<'ctx>> {
    use crate::codegen::LlvmVal;
    let Some(expected) = expected_ty else {
        return Ok(match val {
            LlvmVal::Float(f) => f.into(),
            LlvmVal::Int(i) => i.into(),
            LlvmVal::Ptr(p) => p.into(),
            LlvmVal::Void => cg.ptr_type().const_null().into(),
        });
    };
    let ptr_ty: BasicTypeEnum = cg.ptr_type().into();
    let f64_ty: BasicTypeEnum = cg.ctx.f64_type().into();
    let i64_ty = cg.ctx.i64_type();
    match (&val, expected) {
        (LlvmVal::Float(f), t) if t == ptr_ty => {
            let f = *f;
            let bits = cg.builder.build_bitcast(f, i64_ty, "fbits")?;
            let ptr = cg
                .builder
                .build_int_to_ptr(bits.into_int_value(), cg.ptr_type(), "fptr")?;
            Ok(ptr.into())
        }
        (LlvmVal::Int(i), t) if t == ptr_ty => {
            let i = *i;
            let ext = cg.builder.build_int_z_extend(i, i64_ty, "ibits")?;
            let ptr = cg.builder.build_int_to_ptr(ext, cg.ptr_type(), "iptr")?;
            Ok(ptr.into())
        }
        (LlvmVal::Ptr(p), t) if t == f64_ty => {
            let p = *p;
            let bits = cg.builder.build_ptr_to_int(p, i64_ty, "ptoi")?;
            let f = cg.builder.build_bitcast(bits, cg.ctx.f64_type(), "i64f")?;
            Ok(f.into())
        }
        _ => Ok(match val {
            LlvmVal::Float(f) => f.into(),
            LlvmVal::Int(i) => i.into(),
            LlvmVal::Ptr(p) => p.into(),
            LlvmVal::Void => cg.ptr_type().const_null().into(),
        }),
    }
}
