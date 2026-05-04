use hulk_hir::{Expr, Hir, Program, TypeEnv, TypeId, TypedAst};

pub(super) fn make_hir(body: Expr) -> Hir {
    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![],
        body,
    };
    make_hir_from_program(program)
}

pub(super) fn make_hir_from_program(program: Program) -> Hir {
    let mut symbols = hulk_hir::Resolver::new();
    symbols.resolve_program(&program);

    let mut types = TypeEnv::new();
    types.register_symbol_type(hulk_hir::SymbolId(0), TypeId::OBJECT);

    Hir::from_typed(TypedAst {
        program,
        symbols,
        types,
    })
}
