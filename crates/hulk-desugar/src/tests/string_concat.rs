use std::sync::Arc;

use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{BinOpKind, Expr, ExprKind, NodeIdGen, SourceFile, Span};

use crate::desugar;

use super::common::make_hir;

#[test]
fn desugars_concat_spaced_into_two_concat_ops() {
    let source = Arc::new(SourceFile::new("desugar.hulk", "\"a\" @@ \"b\""));
    let span = Span::new(source, 0, 10);
    let mut ids = NodeIdGen::new();

    let body = Expr::new(
        ExprKind::BinOp {
            op: BinOpKind::ConcatSpaced,
            left: Box::new(Expr::new(
                ExprKind::StringLit("a".to_owned()),
                span.clone(),
                ids.next_id(),
            )),
            right: Box::new(Expr::new(
                ExprKind::StringLit("b".to_owned()),
                span.clone(),
                ids.next_id(),
            )),
        },
        span.clone(),
        ids.next_id(),
    );

    let hir = make_hir(body);
    let mut bag = DiagnosticBag::new();
    let transformed = desugar(hir, &mut bag);

    match transformed.program.body.kind {
        ExprKind::BinOp {
            op: BinOpKind::Concat,
            left,
            right,
        } => {
            assert!(matches!(right.kind, ExprKind::StringLit(ref s) if s == "b"));
            match left.kind {
                ExprKind::BinOp {
                    op: BinOpKind::Concat,
                    left: inner_left,
                    right: inner_right,
                } => {
                    assert!(matches!(inner_left.kind, ExprKind::StringLit(ref s) if s == "a"));
                    assert!(matches!(inner_right.kind, ExprKind::StringLit(ref s) if s == " "));
                }
                _ => panic!("expected nested concat in left branch"),
            }
        }
        _ => panic!("expected concat expression after desugar"),
    }
}
