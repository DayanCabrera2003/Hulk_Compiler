use inkwell::{
    AddressSpace,
    context::Context,
    module::Module,
    values::FunctionValue,
};

/// Handles for every runtime C function declared in the LLVM module.
///
/// These correspond 1-to-1 with declarations in `runtime/gc.h`,
/// `runtime/strings.h`, and `runtime/builtins.h`. All pointer
/// arguments use the LLVM 17 opaque pointer type (`ptr`).
pub struct RuntimeFunctions<'ctx> {
    // GC
    pub hulk_alloc: FunctionValue<'ctx>,
    pub hulk_shadow_push: FunctionValue<'ctx>,
    pub hulk_shadow_pop: FunctionValue<'ctx>,
    // Strings
    pub hulk_string_new: FunctionValue<'ctx>,
    // Reserved for use by future codegen passes that bypass the BANNER `__hulk_concat` alias.
    #[allow(dead_code)]
    pub hulk_string_concat: FunctionValue<'ctx>,
    pub hulk_number_to_string: FunctionValue<'ctx>,
    // Builtins
    pub hulk_print: FunctionValue<'ctx>,
    pub hulk_print_number: FunctionValue<'ctx>,
    pub hulk_print_bool: FunctionValue<'ctx>,
    pub hulk_sqrt: FunctionValue<'ctx>,
    pub hulk_sin: FunctionValue<'ctx>,
    pub hulk_cos: FunctionValue<'ctx>,
    pub hulk_exp: FunctionValue<'ctx>,
    pub hulk_log: FunctionValue<'ctx>,
    pub hulk_rand: FunctionValue<'ctx>,
    pub hulk_range_new: FunctionValue<'ctx>,
    // Helpers emitted by the BANNER lowerer
    pub hulk_concat: FunctionValue<'ctx>,
    pub vec_new: FunctionValue<'ctx>,
    pub vec_push: FunctionValue<'ctx>,
}

impl<'ctx> RuntimeFunctions<'ctx> {
    /// Declare all runtime functions in `module`.
    ///
    /// Uses `add_function`, which is idempotent: calling it twice with the
    /// same name returns the existing declaration.
    pub fn declare(ctx: &'ctx Context, module: &Module<'ctx>) -> Self {
        let f64_t = ctx.f64_type();
        let i32_t = ctx.i32_type();
        let i64_t = ctx.i64_type();
        let bool_t = ctx.bool_type();
        let void_t = ctx.void_type();
        let ptr_t = ctx.i8_type().ptr_type(AddressSpace::default());

        // hulk_alloc(TypeTag* tag, size_t payload_size) -> void*
        let hulk_alloc = module.add_function(
            "hulk_alloc",
            ptr_t.fn_type(&[ptr_t.into(), i64_t.into()], false),
            None,
        );

        // hulk_shadow_push(void* val)
        let hulk_shadow_push = module.add_function(
            "hulk_shadow_push",
            void_t.fn_type(&[ptr_t.into()], false),
            None,
        );

        // hulk_shadow_pop()
        let hulk_shadow_pop =
            module.add_function("hulk_shadow_pop", void_t.fn_type(&[], false), None);

        // hulk_string_new(const char* s) -> void*
        let hulk_string_new = module.add_function(
            "hulk_string_new",
            ptr_t.fn_type(&[ptr_t.into()], false),
            None,
        );

        // hulk_string_concat(void* a, void* b) -> void*
        let hulk_string_concat = module.add_function(
            "hulk_string_concat",
            ptr_t.fn_type(&[ptr_t.into(), ptr_t.into()], false),
            None,
        );

        // hulk_number_to_string(double n) -> void*
        let hulk_number_to_string = module.add_function(
            "hulk_number_to_string",
            ptr_t.fn_type(&[f64_t.into()], false),
            None,
        );

        // hulk_print(void* obj)
        let hulk_print =
            module.add_function("hulk_print", void_t.fn_type(&[ptr_t.into()], false), None);

        // hulk_print_number(double n)
        let hulk_print_number = module.add_function(
            "hulk_print_number",
            void_t.fn_type(&[f64_t.into()], false),
            None,
        );

        // hulk_print_bool(int b)
        let hulk_print_bool = module.add_function(
            "hulk_print_bool",
            void_t.fn_type(&[i32_t.into()], false),
            None,
        );

        // Math builtins: double fn(double)
        let math_sig = f64_t.fn_type(&[f64_t.into()], false);
        let hulk_sqrt = module.add_function("hulk_sqrt", math_sig, None);
        let hulk_sin = module.add_function("hulk_sin", math_sig, None);
        let hulk_cos = module.add_function("hulk_cos", math_sig, None);
        let hulk_exp = module.add_function("hulk_exp", math_sig, None);
        let hulk_log = module.add_function("hulk_log", math_sig, None);

        // hulk_rand() -> double
        let hulk_rand =
            module.add_function("hulk_rand", f64_t.fn_type(&[], false), None);

        // hulk_range_new(double min, double max, double step) -> void*
        let hulk_range_new = module.add_function(
            "hulk_range_new",
            ptr_t.fn_type(&[f64_t.into(), f64_t.into(), f64_t.into()], false),
            None,
        );

        // __hulk_concat(void* a, void* b) -> void*  (alias for hulk_string_concat used by BANNER)
        let hulk_concat = module.add_function(
            "__hulk_concat",
            ptr_t.fn_type(&[ptr_t.into(), ptr_t.into()], false),
            None,
        );

        // __vec_new(double n) -> void*
        let vec_new = module.add_function(
            "__vec_new",
            ptr_t.fn_type(&[f64_t.into()], false),
            None,
        );

        // __vec_push(void* vec, void* elem) -> void*
        let vec_push = module.add_function(
            "__vec_push",
            ptr_t.fn_type(&[ptr_t.into(), ptr_t.into()], false),
            None,
        );

        // Mark unused for now (will be implemented in session 16+)
        let _ = bool_t;

        Self {
            hulk_alloc,
            hulk_shadow_push,
            hulk_shadow_pop,
            hulk_string_new,
            hulk_string_concat,
            hulk_number_to_string,
            hulk_print,
            hulk_print_number,
            hulk_print_bool,
            hulk_sqrt,
            hulk_sin,
            hulk_cos,
            hulk_exp,
            hulk_log,
            hulk_rand,
            hulk_range_new,
            hulk_concat,
            vec_new,
            vec_push,
        }
    }

    /// Returns the `FunctionValue` for a builtin function referenced by its HULK name.
    #[must_use]
    pub fn resolve_builtin(&self, name: &str) -> Option<FunctionValue<'ctx>> {
        match name {
            "print" | "hulk_print" => Some(self.hulk_print),
            "hulk_print_number" => Some(self.hulk_print_number),
            "hulk_print_bool" => Some(self.hulk_print_bool),
            "sqrt" => Some(self.hulk_sqrt),
            "sin" => Some(self.hulk_sin),
            "cos" => Some(self.hulk_cos),
            "exp" => Some(self.hulk_exp),
            "log" => Some(self.hulk_log),
            "rand" => Some(self.hulk_rand),
            "range" => Some(self.hulk_range_new),
            "hulk_string_new" => Some(self.hulk_string_new),
            "hulk_string_concat" | "__hulk_concat" => Some(self.hulk_concat),
            "hulk_number_to_string" => Some(self.hulk_number_to_string),
            "__vec_new" => Some(self.vec_new),
            "__vec_push" => Some(self.vec_push),
            _ => None,
        }
    }
}
