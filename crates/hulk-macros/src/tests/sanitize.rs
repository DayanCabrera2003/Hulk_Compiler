use std::sync::Arc;

use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{
    AssignTarget, BinOpKind, Expr, ExprKind, Hir, LetBinding, MacroDecl, MacroParam, NodeIdGen,
    Program, Resolver, SourceFile, Span, TypeAnn, TypeEnv, TypedAst,
};

use crate::expand_macros;
use crate::tests::common::{collect_identifiers, intrinsic_call, number};

#[test]
fn repeat_macro_is_expanded_with_sanitized_locals() {
    let source = Arc::new(SourceFile::new("macros.hulk", "def repeat ..."));
    let mut node_ids = NodeIdGen::new();
    let span = Span::new(source, 0, 12);

    let macro_decl = MacroDecl {
        name: "repeat".to_owned(),
        params: vec![
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
            ExprKind::Let {
                bindings: vec![Expr::new(
                    ExprKind::LetBinding(LetBinding {
                        name: "total".to_owned(),
                        type_ann: None,
                        value: Box::new(Expr::new(
                            ExprKind::Ident("n".to_owned()),
                            span.clone(),
                            node_ids.next_id(),
                        )),
                        span: span.clone(),
                    }),
                    span.clone(),
                    node_ids.next_id(),
                )],
                body: Box::new(Expr::new(
                    ExprKind::While {
                        condition: Box::new(Expr::new(
                            ExprKind::BinOp {
                                op: BinOpKind::Ge,
                                left: Box::new(Expr::new(
                                    ExprKind::Ident("total".to_owned()),
                                    span.clone(),
                                    node_ids.next_id(),
                                )),
                                right: Box::new(Expr::new(
                                    ExprKind::Number(0.0),
                                    span.clone(),
                                    node_ids.next_id(),
                                )),
                            },
                            span.clone(),
                            node_ids.next_id(),
                        )),
                        body: Box::new(Expr::new(
                            ExprKind::Block(vec![
                                Expr::new(
                                    ExprKind::Assign {
                                        target: Box::new(Expr::new(
                                            ExprKind::AssignTarget(AssignTarget::Ident(
                                                "total".to_owned(),
                                            )),
                                            span.clone(),
                                            node_ids.next_id(),
                                        )),
                                        value: Box::new(Expr::new(
                                            ExprKind::BinOp {
                                                op: BinOpKind::Sub,
                                                left: Box::new(Expr::new(
                                                    ExprKind::Ident("total".to_owned()),
                                                    span.clone(),
                                                    node_ids.next_id(),
                                                )),
                                                right: Box::new(Expr::new(
                                                    ExprKind::Number(1.0),
                                                    span.clone(),
                                                    node_ids.next_id(),
                                                )),
                                            },
                                            span.clone(),
                                            node_ids.next_id(),
                                        )),
                                    },
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
                        )),
                    },
                    span.clone(),
                    node_ids.next_id(),
                )),
            },
            span.clone(),
            node_ids.next_id(),
        ),
        span: span.clone(),
    };

    let call = Expr::new(
        ExprKind::Call {
            callee: Box::new(Expr::new(
                ExprKind::Ident("repeat".to_owned()),
                span.clone(),
                node_ids.next_id(),
            )),
            args: vec![
                Expr::new(ExprKind::Number(10.0), span.clone(), node_ids.next_id()),
                Expr::new(
                    ExprKind::Block(vec![Expr::new(
                        ExprKind::Call {
                            callee: Box::new(Expr::new(
                                ExprKind::Ident("print".to_owned()),
                                span.clone(),
                                node_ids.next_id(),
                            )),
                            args: vec![Expr::new(
                                ExprKind::StringLit("hello".to_owned()),
                                span.clone(),
                                node_ids.next_id(),
                            )],
                        },
                        span.clone(),
                        node_ids.next_id(),
                    )]),
                    span.clone(),
                    node_ids.next_id(),
                ),
            ],
        },
        span.clone(),
        node_ids.next_id(),
    );

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![macro_decl],
        body: call,
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

    let mut idents = Vec::new();
    collect_identifiers(&expanded.program.body, &mut idents);

    assert!(!idents.iter().any(|name| name == "repeat"));
    assert!(!idents.iter().any(|name| name == "expr"));
    assert!(idents
        .iter()
        .any(|name| name.starts_with("__hulk_macro_repeat_0_total")));
    assert!(idents.iter().any(|name| name == "print"));
}

#[test]
fn macro_local_sanitization_does_not_capture_outer_total() {
    let source = Arc::new(SourceFile::new("sanitize.hulk", "def with_total ..."));
    let mut node_ids = NodeIdGen::new();
    let span = Span::new(source, 0, 16);

    let with_total_decl = MacroDecl {
        name: "with_total".to_owned(),
        params: vec![MacroParam::Regular {
            name: "n".to_owned(),
            type_ann: TypeAnn::Named("Number".to_owned()),
            span: span.clone(),
        }],
        body: Expr::new(
            ExprKind::Let {
                bindings: vec![Expr::new(
                    ExprKind::LetBinding(LetBinding {
                        name: "total".to_owned(),
                        type_ann: None,
                        value: Box::new(Expr::new(
                            ExprKind::Ident("n".to_owned()),
                            span.clone(),
                            node_ids.next_id(),
                        )),
                        span: span.clone(),
                    }),
                    span.clone(),
                    node_ids.next_id(),
                )],
                body: Box::new(Expr::new(
                    ExprKind::Ident("total".to_owned()),
                    span.clone(),
                    node_ids.next_id(),
                )),
            },
            span.clone(),
            node_ids.next_id(),
        ),
        span: span.clone(),
    };

    let outer_total_binding = Expr::new(
        ExprKind::LetBinding(LetBinding {
            name: "total".to_owned(),
            type_ann: None,
            value: Box::new(number(100.0, &span, &mut node_ids)),
            span: span.clone(),
        }),
        span.clone(),
        node_ids.next_id(),
    );

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![with_total_decl],
        body: Expr::new(
            ExprKind::Let {
                bindings: vec![outer_total_binding],
                body: Box::new(Expr::new(
                    ExprKind::BinOp {
                        op: BinOpKind::Add,
                        left: Box::new(crate::tests::common::ident("total", &span, &mut node_ids)),
                        right: Box::new(intrinsic_call(
                            "with_total",
                            vec![number(5.0, &span, &mut node_ids)],
                            &span,
                            &mut node_ids,
                        )),
                    },
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
    let hir = Hir::from_typed(TypedAst {
        program,
        symbols,
        types: TypeEnv::new(),
    });

    let mut bag = DiagnosticBag::new();
    let expanded = expand_macros(hir, &mut bag);
    assert!(
        !bag.has_errors(),
        "unexpected diagnostics: {:?}",
        bag.diagnostics()
    );

    let mut idents = Vec::new();
    collect_identifiers(&expanded.program.body, &mut idents);

    assert!(idents.iter().any(|name| name == "total"));
    assert!(idents
        .iter()
        .any(|name| name.starts_with("__hulk_macro_with_total_0_total")));
}
