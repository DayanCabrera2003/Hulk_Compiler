use std::collections::HashMap;

use hulk_hir::{FunctionDecl, Param, TypeAnn};

/// Captured parameter/return-type information about a top-level function,
/// used when synthesising wrapper types that forward a call to the original
/// function symbol.
#[derive(Clone)]
pub(crate) struct FunctionSignature {
    pub(crate) params: Vec<Param>,
    pub(crate) return_type: Option<TypeAnn>,
}

/// Builds a name-indexed table of function signatures from a slice of
/// function declarations.
pub(crate) fn collect_function_signatures(
    functions: &[FunctionDecl],
) -> HashMap<String, FunctionSignature> {
    let mut signatures = HashMap::new();
    for function in functions {
        signatures.insert(
            function.name.clone(),
            FunctionSignature {
                params: function.params.clone(),
                return_type: function.return_type.clone(),
            },
        );
    }
    signatures
}
