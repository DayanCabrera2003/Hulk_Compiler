use std::sync::Arc;

use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{
    BinOpKind, Expr, ExprKind, Hir, MacroDecl, MacroParam, NodeIdGen, Program, Resolver,
    SourceFile, Span, TypeAnn, TypeEnv, TypedAst, UnaryOpKind,
};

use crate::expand_macros;
use crate::tests::common::{ident, intrinsic_call, number};

#[test]
fn non_macro_call_is_not_modified() {
    let source = Arc::new(SourceFile::new("macros.hulk", "print(1);"));
    let mut node_ids = NodeIdGen::new();
    let span = Span::new(source, 0, 9);

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![],
        body: Expr::new(
            ExprKind::Call {
                callee: Box::new(Expr::new(
                    ExprKind::Ident("print".to_owned()),
                    span.clone(),
                    node_ids.next_id(),
                )),
                args: vec![Expr::new(
                    ExprKind::UnaryOp {
                        op: UnaryOpKind::Neg,
                        expr: Box::new(Expr::new(
                            ExprKind::Number(-1.0),
                            span.clone(),
                            node_ids.next_id(),
                        )),
                    },
                    span.clone(),
                    node_ids.next_id(),
                )],
            },
            span.clone(),
            node_ids.next_id(),
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

    match expanded.program.body.kind {
        ExprKind::Call { .. } => {}
        _ => panic!("expected call expression to remain unchanged"),
    }
}

#[test]
fn invalid_macro_arguments_emit_errors() {
    let source = Arc::new(SourceFile::new("errors.hulk", "def swap ..."));
    let mut node_ids = NodeIdGen::new();
    let span = Span::new(source, 0, 10);

    let swap_decl = MacroDecl {
        name: "swap".to_owned(),
        params: vec![
            MacroParam::Symbolic {
                name: "x".to_owned(),
                type_ann: TypeAnn::Named("Object".to_owned()),
                span: span.clone(),
            },
            MacroParam::Symbolic {
                name: "y".to_owned(),
                type_ann: TypeAnn::Named("Object".to_owned()),
                span: span.clone(),
            },
        ],
        body: ident("x", &span, &mut node_ids),
        span: span.clone(),
    };

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

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![swap_decl, repeat_decl],
        body: Expr::new(
            ExprKind::Block(vec![
                intrinsic_call(
                    "swap",
                    vec![ident("left", &span, &mut node_ids)],
                    &span,
                    &mut node_ids,
                ),
                intrinsic_call(
                    "repeat",
                    vec![number(10.0, &span, &mut node_ids)],
                    &span,
                    &mut node_ids,
                ),
            ]),
            span.clone(),
            node_ids.next_id(),
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

    assert!(bag.has_errors());
    let diagnostics = bag.diagnostics();
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("cantidad de argumentos invalida para macro 'swap'")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("el placeholder 'iter' requiere un identificador")
    }));

    match expanded.program.body.kind {
        ExprKind::Block(ref exprs) => {
            assert_eq!(exprs.len(), 2);
            assert!(matches!(exprs[0].kind, ExprKind::Call { .. }));
            assert!(matches!(exprs[1].kind, ExprKind::Call { .. }));
        }
        _ => panic!("expected top-level block with fallback calls"),
    }
}

#[test]
fn body_param_requires_block_expression() {
    // Regression: a `*body` parameter used to accept any expression
    // silently. The spec says body parameters receive a block so the
    // expander must reject non-block arguments with a diagnostic.
    let source = Arc::new(SourceFile::new("block.hulk", "def run ..."));
    let mut node_ids = NodeIdGen::new();
    let span = Span::new(source, 0, 10);

    let run_decl = MacroDecl {
        name: "run".to_owned(),
        params: vec![MacroParam::Body {
            name: "expr".to_owned(),
            type_ann: TypeAnn::Named("Object".to_owned()),
            span: span.clone(),
        }],
        body: ident("expr", &span, &mut node_ids),
        span: span.clone(),
    };

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![run_decl],
        body: intrinsic_call(
            "run",
            vec![number(7.0, &span, &mut node_ids)],
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
    let _expanded = expand_macros(hir, &mut bag);

    assert!(bag.has_errors(), "expected diagnostic for non-block body argument");
    assert!(bag.diagnostics().iter().any(|diag| diag
        .message
        .contains("el parametro de cuerpo 'expr' requiere un bloque")));
}

#[test]
fn expansion_preserves_algebraic_structure_outside_pattern_matching() {
    // Regression: `simplify_algebraic` used to leak into every macro
    // expansion and silently rewrite `x + 0` to `x`. A macro that is not
    // doing pattern matching must produce the body literally.
    let source = Arc::new(SourceFile::new("identity.hulk", "def id ..."));
    let mut node_ids = NodeIdGen::new();
    let span = Span::new(source, 0, 6);

    let id_decl = MacroDecl {
        name: "id".to_owned(),
        params: vec![MacroParam::Regular {
            name: "x".to_owned(),
            type_ann: TypeAnn::Named("Number".to_owned()),
            span: span.clone(),
        }],
        body: Expr::new(
            ExprKind::BinOp {
                op: BinOpKind::Add,
                left: Box::new(ident("x", &span, &mut node_ids)),
                right: Box::new(number(0.0, &span, &mut node_ids)),
            },
            span.clone(),
            node_ids.next_id(),
        ),
        span: span.clone(),
    };

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![id_decl],
        body: intrinsic_call(
            "id",
            vec![number(42.0, &span, &mut node_ids)],
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
    assert!(!bag.has_errors());

    match &expanded.program.body.kind {
        ExprKind::BinOp { op, left, right } => {
            assert_eq!(*op, BinOpKind::Add);
            assert!(matches!(&left.kind, ExprKind::Number(v) if (*v - 42.0).abs() < f64::EPSILON));
            assert!(matches!(&right.kind, ExprKind::Number(v) if v.abs() < f64::EPSILON));
        }
        other => panic!("expected `42 + 0` to be preserved, got {other:?}"),
    }
}
