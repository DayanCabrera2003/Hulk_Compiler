use std::collections::HashMap;

use inkwell::{
    AddressSpace,
    basic_block::BasicBlock,
    builder::Builder,
    context::Context,
    module::Module,
    types::{BasicMetadataTypeEnum, BasicTypeEnum, StructType},
    values::{BasicValueEnum, FunctionValue, GlobalValue, PointerValue},
};

use hulk_banner::{BannerProgram, TempId};

use crate::{
    emit::{infer_return_kind, infer_temp_kinds},
    error::CodegenResult,
    layout::ProgramLayout,
    rt::RuntimeFunctions,
};

/// The LLVM type category used for a BANNER temporary's alloca slot.
///
/// Determines whether the alloca holds an f64, an i1, or an opaque pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempKind {
    /// f64 — for Number-typed temporaries (arithmetic results, literals).
    F64,
    /// i1 — for Boolean-typed temporaries (comparisons, logical ops).
    I1,
    /// ptr — for all reference-typed temporaries (strings, objects, null).
    Ptr,
}


/// An LLVM value that may represent a Number, Boolean, reference, or void result.
///
/// BANNER temporaries that hold the result of void calls (e.g., `print`) are
/// represented as `Void` rather than a real LLVM value.
#[derive(Debug, Clone)]
pub enum LlvmVal<'ctx> {
    Float(inkwell::values::FloatValue<'ctx>),
    Int(inkwell::values::IntValue<'ctx>),
    Ptr(PointerValue<'ctx>),
    /// Result of a void call; used as a sentinel when a temp is "unused".
    Void,
}

impl<'ctx> LlvmVal<'ctx> {
    /// Converts to `BasicValueEnum` when the value is non-void.
    pub fn as_basic(&self) -> Option<BasicValueEnum<'ctx>> {
        match self {
            Self::Float(v) => Some((*v).into()),
            Self::Int(v) => Some((*v).into()),
            Self::Ptr(v) => Some((*v).into()),
            Self::Void => None,
        }
    }
}

/// The central code generator; holds LLVM context, module, and per-function state.
///
/// Lifetime `'ctx` is tied to the `inkwell::context::Context` that owns all
/// LLVM values and types. The module is consumed (or printed/written) once
/// all functions have been emitted.
pub struct Codegen<'ctx> {
    pub ctx: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    /// Static layout: vtable method ordering and field ordering per type.
    pub layout: ProgramLayout,
    /// Runtime function declarations (`hulk_alloc`, `hulk_print`, etc.).
    pub rt: RuntimeFunctions<'ctx>,
    /// Interned string literal globals, keyed by the original string content.
    pub str_globals: HashMap<String, GlobalValue<'ctx>>,
    /// LLVM struct types for each user-defined type (payload, without GC header).
    pub struct_types: HashMap<String, StructType<'ctx>>,
    /// Vtable globals for each user-defined type.
    pub vtable_globals: HashMap<String, GlobalValue<'ctx>>,
    /// Declared LLVM functions (user-defined + runtime); keyed by BANNER function name.
    pub functions: HashMap<String, FunctionValue<'ctx>>,

    // ---- Per-function state (reset by `reset_frame`) ----

    /// Maps each BANNER `TempId` to the corresponding LLVM value (reserved for
    /// future direct-SSA emission without alloca; currently unused).
    #[allow(dead_code)]
    pub temps: HashMap<TempId, LlvmVal<'ctx>>,
    /// Maps label strings to LLVM basic blocks within the current function.
    pub blocks: HashMap<String, BasicBlock<'ctx>>,
    /// The alloca-backed slot for each temp (all temps use alloca).
    pub temp_slots: HashMap<TempId, PointerValue<'ctx>>,
    /// Inferred type kind for each temp (determines alloca LLVM type).
    pub temp_kinds: HashMap<TempId, TempKind>,
    /// Type name (user-defined) for temps produced by `New` instructions.
    /// Used by GetField/SetField to resolve struct field indices.
    pub temp_type_names: HashMap<TempId, String>,
    /// Inferred return kind for every user function/method (keyed by short name and full name).
    /// Populated by `predeclare_all` and used by emit_method_call for indirect call typing.
    pub fn_return_kinds: HashMap<String, TempKind>,
    /// Field type map: (type_name, field_name) → TempKind, derived from TypeDescriptor.pointer_map.
    /// false in pointer_map → F64 (numeric), true → Ptr (reference).
    pub field_kind_map: HashMap<(String, String), TempKind>,
}

impl<'ctx> Codegen<'ctx> {
    /// Create a new `Codegen` for a given module name and program layout.
    #[must_use]
    pub fn new(ctx: &'ctx Context, module_name: &str, program: &BannerProgram) -> Self {
        let module = ctx.create_module(module_name);
        let builder = ctx.create_builder();
        let layout = ProgramLayout::compute(program);
        let rt = RuntimeFunctions::declare(ctx, &module);

        Self {
            ctx,
            module,
            builder,
            layout,
            rt,
            str_globals: HashMap::new(),
            struct_types: HashMap::new(),
            vtable_globals: HashMap::new(),
            functions: HashMap::new(),
            temps: HashMap::new(),
            blocks: HashMap::new(),
            temp_slots: HashMap::new(),
            temp_kinds: HashMap::new(),
            temp_type_names: HashMap::new(),
            fn_return_kinds: HashMap::new(),
            field_kind_map: HashMap::new(),
        }
    }

    /// Reset all per-function state before emitting a new function body.
    #[allow(dead_code)]
    pub fn reset_frame(&mut self) {
        self.temps.clear();
        self.blocks.clear();
        self.temp_slots.clear();
        self.temp_kinds.clear();
        self.temp_type_names.clear();
        // fn_return_kinds and field_kind_map are program-level — don't reset.
    }

    /// Intern a string literal as a private null-terminated `i8` array global.
    ///
    /// Identical strings reuse the same global to avoid duplication.
    pub fn intern_str(&mut self, s: &str) -> GlobalValue<'ctx> {
        if let Some(&g) = self.str_globals.get(s) {
            return g;
        }
        let name = format!("__str_{}", self.str_globals.len());
        let bytes: Vec<u8> = s.bytes().chain(std::iter::once(0u8)).collect();
        let arr_ty = self.ctx.i8_type().array_type(bytes.len() as u32);
        let global = self.module.add_global(arr_ty, Some(AddressSpace::default()), &name);

        let init_vals: Vec<inkwell::values::IntValue<'ctx>> = bytes
            .iter()
            .map(|&b| self.ctx.i8_type().const_int(u64::from(b), false))
            .collect();
        let init = self.ctx.i8_type().const_array(&init_vals);
        global.set_initializer(&init);
        global.set_constant(true);
        global.set_linkage(inkwell::module::Linkage::Private);
        global.set_unnamed_addr(true);

        self.str_globals.insert(s.to_string(), global);
        global
    }

    /// Return the opaque pointer type used for all reference types.
    #[must_use]
    pub fn ptr_type(&self) -> inkwell::types::PointerType<'ctx> {
        self.ctx.i8_type().ptr_type(AddressSpace::default())
    }

    /// Build the LLVM struct type for a user-defined type.
    ///
    /// Layout: `{ ptr, field0_type, field1_type, ... }` where `ptr` at index 0
    /// is the vtable pointer. All user fields use `ptr` as a conservative type
    /// (codegen loads/stores with the correct LLVM type when the field type is known).
    pub fn build_struct_type(&mut self, type_name: &str) -> StructType<'ctx> {
        if let Some(&st) = self.struct_types.get(type_name) {
            return st;
        }
        let fields = self.layout.fields.get(type_name).cloned().unwrap_or_default();
        let ptr_ty = self.ptr_type();
        // Field 0 = vtable pointer; remaining = one ptr per declared field.
        let mut field_types: Vec<BasicTypeEnum<'ctx>> =
            std::iter::once(ptr_ty.into()).chain(fields.iter().map(|_| ptr_ty.into())).collect();

        // Ensure at least one field (LLVM requires non-empty struct for alloca of user types).
        if field_types.is_empty() {
            field_types.push(ptr_ty.into());
        }

        let st = self.ctx.struct_type(&field_types, false);
        self.struct_types.insert(type_name.to_string(), st);
        st
    }

    /// Declare a typed user function.
    ///
    /// Standalone functions get typed params/return from inference.
    /// Methods get all-ptr params with typed return (for vtable uniformity).
    pub fn declare_typed_function(
        &mut self,
        name: &str,
        param_kinds: &[TempKind],
        ret_kind: TempKind,
    ) -> FunctionValue<'ctx> {
        if let Some(&fv) = self.functions.get(name) {
            return fv;
        }
        let params: Vec<BasicMetadataTypeEnum<'ctx>> = param_kinds
            .iter()
            .map(|k| self.kind_to_metadata_type(*k))
            .collect();
        let fn_ty = match ret_kind {
            TempKind::F64 => self.ctx.f64_type().fn_type(&params, false),
            TempKind::I1 => self.ctx.bool_type().fn_type(&params, false),
            TempKind::Ptr => self.ptr_type().fn_type(&params, false),
        };
        let fv = self.module.add_function(name, fn_ty, None);
        self.functions.insert(name.to_string(), fv);
        fv
    }

    /// Convert a TempKind to a BasicMetadataTypeEnum for function param declarations.
    pub fn kind_to_metadata_type(&self, kind: TempKind) -> BasicMetadataTypeEnum<'ctx> {
        match kind {
            TempKind::F64 => self.ctx.f64_type().into(),
            TempKind::I1 => self.ctx.bool_type().into(),
            TempKind::Ptr => self.ptr_type().into(),
        }
    }

    /// Declare all user functions from a `BannerProgram` in the LLVM module.
    ///
    /// Runs iterative type inference to determine typed signatures for all functions.
    /// Methods use all-ptr params with typed return; standalone functions use fully
    /// typed signatures (params and return inferred from the function body).
    pub fn predeclare_all(&mut self, program: &BannerProgram) -> CodegenResult<()> {
        let field_kind_map = build_field_kind_map(program);
        self.field_kind_map = field_kind_map.clone();

        // Iterative inference: 4 passes to handle mutual recursion.
        let mut fn_return_kinds: HashMap<String, TempKind> = HashMap::new();
        // Seed builtin protocol method return types so inference knows what
        // Range/Vector next() and current() return without a HULK definition.
        fn_return_kinds.insert("next".to_string(), TempKind::I1);
        fn_return_kinds.insert("current".to_string(), TempKind::F64);
        for _ in 0..4 {
            for td in &program.types {
                for method in &td.methods {
                    let obj_hints: HashMap<TempId, String> = if !method.params.is_empty() {
                        [(method.params[0], td.name.clone())].into_iter().collect()
                    } else {
                        HashMap::new()
                    };
                    let kinds = infer_temp_kinds(
                        &method.body, &fn_return_kinds, &field_kind_map, &obj_hints,
                    );
                    let ret_kind = infer_return_kind(&method.body, &kinds);
                    fn_return_kinds.insert(method.name.clone(), ret_kind);
                    let short = method.name.split('.').next_back().unwrap_or(&method.name);
                    fn_return_kinds.entry(short.to_string()).or_insert(ret_kind);
                }
            }
            for func in &program.functions {
                let kinds = infer_temp_kinds(
                    &func.body, &fn_return_kinds, &field_kind_map, &HashMap::new(),
                );
                let ret_kind = infer_return_kind(&func.body, &kinds);
                fn_return_kinds.insert(func.name.clone(), ret_kind);
            }
        }
        self.fn_return_kinds = fn_return_kinds.clone();

        // Declare methods: all-ptr params, typed return.
        for td in &program.types {
            for method in &td.methods {
                let ret_kind = fn_return_kinds.get(&method.name).copied().unwrap_or(TempKind::Ptr);
                let param_kinds: Vec<TempKind> =
                    std::iter::repeat(TempKind::Ptr).take(method.params.len()).collect();
                self.declare_typed_function(&method.name, &param_kinds, ret_kind);
            }
        }
        // Declare standalone functions: inferred param types + inferred return.
        for func in &program.functions {
            let kinds = infer_temp_kinds(
                &func.body, &fn_return_kinds, &field_kind_map, &HashMap::new(),
            );
            let param_kinds: Vec<TempKind> = func.params.iter()
                .map(|p| kinds.get(p).copied().unwrap_or(TempKind::Ptr))
                .collect();
            let ret_kind = fn_return_kinds.get(&func.name).copied().unwrap_or(TempKind::Ptr);
            self.declare_typed_function(&func.name, &param_kinds, ret_kind);
        }
        // __main__ returns i32 for C compatibility.
        let main_ty = self.ctx.i32_type().fn_type(&[], false);
        let main_fn = self.module.add_function("main", main_ty, None);
        self.functions.insert("main".to_string(), main_fn);
        Ok(())
    }
}

/// Build a map from (type_name, field_name) → TempKind using pointer_map.
///
/// pointer_map[i] == false → numeric field (F64); true → reference field (Ptr).
pub(crate) fn build_field_kind_map(
    program: &BannerProgram,
) -> HashMap<(String, String), TempKind> {
    let mut map = HashMap::new();
    for td in &program.types {
        for (i, field_name) in td.fields.iter().enumerate() {
            let is_ptr = td.pointer_map.get(i).copied().unwrap_or(true);
            let kind = if is_ptr { TempKind::Ptr } else { TempKind::F64 };
            map.insert((td.name.clone(), field_name.clone()), kind);
        }
    }
    map
}
