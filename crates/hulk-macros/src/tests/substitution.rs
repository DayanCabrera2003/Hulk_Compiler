use std::sync::Arc;

use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{
    AssignTarget, Expr, ExprKind, Hir, MacroDecl, MacroParam, NodeIdGen, Program, Resolver,
    SourceFile, Span, TypeAnn, TypeEnv, TypedAst,
};

use crate::expand_macros;
use crate::tests::common::{ident, intrinsic_call};

#[test]
fn swap_symbolic_params_are_substituted_as_identifiers() {
    let source = Arc::new(SourceFile::new("swap.hulk", "def swap ..."));
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
        body: Expr::new(
            ExprKind::Block(vec![
                Expr::new(
                    ExprKind::Assign {
                        target: Box::new(Expr::new(
                            ExprKind::AssignTarget(AssignTarget::Ident("x".to_owned())),
                            span.clone(),
                            node_ids.next_id(),
                        )),
                        value: Box::new(Expr::new(
                            ExprKind::Ident("y".to_owned()),
                            span.clone(),
                            node_ids.next_id(),
                        )),
                    },
                    span.clone(),
                    node_ids.next_id(),
                ),
                Expr::new(
                    ExprKind::Assign {
                        target: Box::new(Expr::new(
                            ExprKind::AssignTarget(AssignTarget::Ident("y".to_owned())),
                            span.clone(),
                            node_ids.next_id(),
                        )),
                        value: Box::new(Expr::new(
                            ExprKind::Ident("x".to_owned()),
                            span.clone(),
                            node_ids.next_id(),
                        )),
                    },
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
        macros: vec![swap_decl],
        body: intrinsic_call(
            "swap",
            vec![
                ident("left", &span, &mut node_ids),
                ident("right", &span, &mut node_ids),
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

    assert!(!bag.has_errors());

    match &expanded.program.body.kind {
        ExprKind::Block(exprs) => {
            assert_eq!(exprs.len(), 2);

            match &exprs[0].kind {
                ExprKind::Assign { target, value } => {
                    assert!(matches!(
                        &target.kind,
                        ExprKind::AssignTarget(AssignTarget::Ident(name)) if name == "left"
                    ));
                    assert!(matches!(&value.kind, ExprKind::Ident(name) if name == "right"));
                }
                _ => panic!("expected first expression to be assignment"),
            }

            match &exprs[1].kind {
                ExprKind::Assign { target, value } => {
                    assert!(matches!(
                        &target.kind,
                        ExprKind::AssignTarget(AssignTarget::Ident(name)) if name == "right"
                    ));
                    assert!(matches!(&value.kind, ExprKind::Ident(name) if name == "left"));
                }
                _ => panic!("expected second expression to be assignment"),
            }
        }
        _ => panic!("expected expanded swap macro to produce a block"),
    }
}
