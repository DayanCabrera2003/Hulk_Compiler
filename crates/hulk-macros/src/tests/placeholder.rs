use std::sync::Arc;

use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{
    Expr, ExprKind, Hir, LetBinding, MacroDecl, MacroParam, NodeIdGen, Program, Resolver,
    SourceFile, Span, SymbolId, TypeAnn, TypeEnv, TypeId, TypedAst,
};

use crate::expand_macros;
use crate::tests::common::{
    collect_ident_node_ids, collect_identifiers, ident, intrinsic_call, number,
};

#[test]
fn placeholder_params_register_symbol_type_and_substitute_identifier() {
    let source = Arc::new(SourceFile::new("repeat.hulk", "def repeat ..."));
    let mut node_ids = NodeIdGen::new();
    let span = Span::new(source, 0, 12);

    let repeat_decl = MacroDecl {
        name: "repeat".to_owned(),
        params: vec![
            MacroParam::Placeholder {
                name: "iter".to_owned(),
                type_ann: TypeAnn::Named("Number".to_owned()),
                span: span.clone(),
            },
            MacroParam::Regular {
                name: "n".to_owned(),
                type_ann: TypeAnn::Named("Number".to_owned()),
                span: span.clone(),
            },
            MacroParam::Body {
                name: "expr".to_owned(),
                type_ann: TypeAnn::Named("Object".to_owned()),
                span: span.clone(),
            },
        ],
        body: Expr::new(
            ExprKind::Block(vec![
                Expr::new(
                    ExprKind::Ident("iter".to_owned()),
                    span.clone(),
                    node_ids.next_id(),
                ),
                Expr::new(
                    ExprKind::Ident("n".to_owned()),
                    span.clone(),
                    node_ids.next_id(),
                ),
                Expr::new(
                    ExprKind::Ident("expr".to_owned()),
                    span.clone(),
                    node_ids.next_id(),
                ),
            ]),
            span.clone(),
            node_ids.next_id(),
        ),
        span: span.clone(),
    };

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![repeat_decl],
        body: intrinsic_call(
            "repeat",
            vec![
                ident("iter", &span, &mut node_ids),
                number(10.0, &span, &mut node_ids),
                Expr::new(
                    ExprKind::Block(vec![intrinsic_call(
                        "print",
                        vec![ident("iter", &span, &mut node_ids)],
                        &span,
                        &mut node_ids,
                    )]),
                    span.clone(),
                    node_ids.next_id(),
                ),
            ],
            &span,
            &mut node_ids,
        ),
    };

    let mut symbols = Resolver::new();
    symbols.resolve_program(&program);
    let hir = Hir::from_typed(TypedAst {
        program,
        symbols,
        types: TypeEnv::new(),
    });

    let mut bag = DiagnosticBag::new();
    let expanded = expand_macros(hir, &mut bag);
    assert!(!bag.has_errors(), "unexpected diagnostics: {:?}", bag.diagnostics());

    let iter_symbol = expanded
        .types
        .symbol_type_symbols()
        .find(|symbol| expanded.symbols.table().name_of(*symbol) == Some("iter"));

    assert!(iter_symbol.is_some(), "expected placeholder symbol for 'iter'");
    assert_eq!(expanded.symbol_type(iter_symbol.unwrap()), Some(TypeId::NUMBER));

    let mut idents = Vec::new();
    collect_identifiers(&expanded.program.body, &mut idents);
    assert!(idents.iter().any(|name| name == "iter"));
    assert!(idents.iter().any(|name| name == "print"));
    assert!(!idents.iter().any(|name| name == "repeat"));
}

#[test]
fn placeholder_idents_resolve_to_allocated_symbol_after_expansion() {
    // Regression: a `$placeholder` parameter used to create a SymbolId
    // inside a temporary scope that was popped immediately, leaving the
    // symbol orphaned. After expansion, every Ident in the expanded
    // program whose name matches the placeholder must resolve (via
    // expr_symbols) to the freshly allocated SymbolId.
    let source = Arc::new(SourceFile::new("repeat.hulk", "def repeat ..."));
    let mut node_ids = NodeIdGen::new();
    let span = Span::new(source, 0, 12);

    let repeat_decl = MacroDecl {
        name: "repeat".to_owned(),
        params: vec![
            MacroParam::Placeholder {
                name: "iter".to_owned(),
                type_ann: TypeAnn::Named("Number".to_owned()),
                span: span.clone(),
            },
            MacroParam::Body {
                name: "expr".to_owned(),
                type_ann: TypeAnn::Named("Object".to_owned()),
                span: span.clone(),
            },
        ],
        body: Expr::new(
            ExprKind::Block(vec![Expr::new(
                ExprKind::Ident("iter".to_owned()),
                span.clone(),
                node_ids.next_id(),
            )]),
            span.clone(),
            node_ids.next_id(),
        ),
        span: span.clone(),
    };

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![repeat_decl],
        body: intrinsic_call(
            "repeat",
            vec![
                ident("iter", &span, &mut node_ids),
                Expr::new(
                    ExprKind::Block(vec![ident("iter", &span, &mut node_ids)]),
                    span.clone(),
                    node_ids.next_id(),
                ),
            ],
            &span,
            &mut node_ids,
        ),
    };

    let mut symbols = Resolver::new();
    symbols.resolve_program(&program);
    let hir = Hir::from_typed(TypedAst {
        program,
        symbols,
        types: TypeEnv::new(),
    });

    let mut bag = DiagnosticBag::new();
    let expanded = expand_macros(hir, &mut bag);
    assert!(!bag.has_errors(), "unexpected diagnostics: {:?}", bag.diagnostics());

    let mut iter_ident_ids = Vec::new();
    collect_ident_node_ids(&expanded.program.body, "iter", &mut iter_ident_ids);
    assert!(
        !iter_ident_ids.is_empty(),
        "expected at least one `iter` ident in expanded body"
    );

    let first_symbol = expanded
        .symbols
        .expr_symbol(iter_ident_ids[0])
        .expect("placeholder ident must resolve to a symbol");
    assert_eq!(
        expanded.symbol_type(first_symbol),
        Some(TypeId::NUMBER),
        "placeholder symbol must carry the declared type"
    );

    for node_id in &iter_ident_ids {
        assert_eq!(
            expanded.symbols.expr_symbol(*node_id),
            Some(first_symbol),
            "all `iter` idents should share the freshly allocated SymbolId"
        );
    }
}

#[test]
fn placeholder_does_not_reuse_caller_scope_symbol() {
    // Regression: when the caller already has a binding sharing the
    // placeholder's identifier, the expansion must introduce a DIFFERENT
    // SymbolId for the placeholder. Caller references to the outer name
    // remain bound to the caller's symbol.
    let source = Arc::new(SourceFile::new("shadow.hulk", "def repeat ..."));
    let mut node_ids = NodeIdGen::new();
    let span = Span::new(source, 0, 12);

    let repeat_decl = MacroDecl {
        name: "repeat".to_owned(),
        params: vec![MacroParam::Placeholder {
            name: "iter".to_owned(),
            type_ann: TypeAnn::Named("Number".to_owned()),
            span: span.clone(),
        }],
        body: ident("iter", &span, &mut node_ids),
        span: span.clone(),
    };

    let outer_ident = ident("iter", &span, &mut node_ids);
    let outer_ident_id = outer_ident.id;
    let call = intrinsic_call(
        "repeat",
        vec![ident("iter", &span, &mut node_ids)],
        &span,
        &mut node_ids,
    );

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![repeat_decl],
        body: Expr::new(
            ExprKind::Let {
                bindings: vec![Expr::new(
                    ExprKind::LetBinding(LetBinding {
                        name: "iter".to_owned(),
                        type_ann: None,
                        value: Box::new(number(1.0, &span, &mut node_ids)),
                        span: span.clone(),
                    }),
                    span.clone(),
                    node_ids.next_id(),
                )],
                body: Box::new(Expr::new(
                    ExprKind::Block(vec![outer_ident, call]),
                    span.clone(),
                    node_ids.next_id(),
                )),
            },
            span.clone(),
            node_ids.next_id(),
        ),
    };

    let mut symbols = Resolver::new();
    symbols.resolve_program(&program);
    let caller_symbol = symbols
        .expr_symbol(outer_ident_id)
        .expect("outer `iter` should resolve to the caller's let binding");

    let hir = Hir::from_typed(TypedAst {
        program,
        symbols,
        types: TypeEnv::new(),
    });

    let mut bag = DiagnosticBag::new();
    let expanded = expand_macros(hir, &mut bag);
    assert!(!bag.has_errors());

    let caller_symbol_after = expanded
        .symbols
        .expr_symbol(outer_ident_id)
        .expect("caller ident mapping must survive expansion");
    assert_eq!(
        caller_symbol_after, caller_symbol,
        "caller `iter` must keep its original SymbolId"
    );

    let mut expanded_iter_ids = Vec::new();
    collect_ident_node_ids(&expanded.program.body, "iter", &mut expanded_iter_ids);
    let placeholder_symbols: Vec<SymbolId> = expanded_iter_ids
        .iter()
        .filter(|id| **id != outer_ident_id)
        .filter_map(|id| expanded.symbols.expr_symbol(*id))
        .collect();
    assert!(!placeholder_symbols.is_empty());
    for symbol in placeholder_symbols {
        assert_ne!(
            symbol, caller_symbol,
            "placeholder SymbolId must differ from caller's"
        );
    }
}
