use std::sync::Arc;

use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{
    BinOpKind, Expr, ExprKind, Hir, MacroDecl, MacroParam, NodeIdGen, Program, Resolver,
    SourceFile, Span, TypeAnn, TypeEnv, TypedAst,
};

use crate::expand_macros;
use crate::expander::MATCH_INTRINSIC;
use crate::pattern::{
    CASE_BINOP_INTRINSIC, CASE_BINOP_RIGHT_LITERAL_INTRINSIC, DEFAULT_CASE_INTRINSIC,
};
use crate::tests::common::{ident, intrinsic_call, number, string_lit};

#[test]
fn simplify_macro_pattern_matching_reduces_expression() {
    let source = Arc::new(SourceFile::new("simplify.hulk", "def simplify ..."));
    let mut node_ids = NodeIdGen::new();
    let span = Span::new(source, 0, 14);

    let expr_param = MacroParam::Regular {
        name: "expr".to_owned(),
        type_ann: TypeAnn::Named("Number".to_owned()),
        span: span.clone(),
    };

    // Pattern cases are ordered specific-first: `x + 0` and `x * 1` must
    // match before the generic `x1 + x2` case so that the concrete
    // reductions win without relying on algebraic post-processing.
    let simplify_body = intrinsic_call(
        MATCH_INTRINSIC,
        vec![
            ident("expr", &span, &mut node_ids),
            intrinsic_call(
                CASE_BINOP_RIGHT_LITERAL_INTRINSIC,
                vec![
                    string_lit("+", &span, &mut node_ids),
                    ident("x1", &span, &mut node_ids),
                    string_lit("Number", &span, &mut node_ids),
                    number(0.0, &span, &mut node_ids),
                    intrinsic_call(
                        "simplify",
                        vec![ident("x1", &span, &mut node_ids)],
                        &span,
                        &mut node_ids,
                    ),
                ],
                &span,
                &mut node_ids,
            ),
            intrinsic_call(
                CASE_BINOP_RIGHT_LITERAL_INTRINSIC,
                vec![
                    string_lit("*", &span, &mut node_ids),
                    ident("x1", &span, &mut node_ids),
                    string_lit("Number", &span, &mut node_ids),
                    number(1.0, &span, &mut node_ids),
                    intrinsic_call(
                        "simplify",
                        vec![ident("x1", &span, &mut node_ids)],
                        &span,
                        &mut node_ids,
                    ),
                ],
                &span,
                &mut node_ids,
            ),
            intrinsic_call(
                CASE_BINOP_INTRINSIC,
                vec![
                    string_lit("+", &span, &mut node_ids),
                    ident("x1", &span, &mut node_ids),
                    string_lit("Number", &span, &mut node_ids),
                    ident("x2", &span, &mut node_ids),
                    string_lit("Number", &span, &mut node_ids),
                    Expr::new(
                        ExprKind::BinOp {
                            op: BinOpKind::Add,
                            left: Box::new(intrinsic_call(
                                "simplify",
                                vec![ident("x1", &span, &mut node_ids)],
                                &span,
                                &mut node_ids,
                            )),
                            right: Box::new(intrinsic_call(
                                "simplify",
                                vec![ident("x2", &span, &mut node_ids)],
                                &span,
                                &mut node_ids,
                            )),
                        },
                        span.clone(),
                        node_ids.next_id(),
                    ),
                ],
                &span,
                &mut node_ids,
            ),
            intrinsic_call(
                DEFAULT_CASE_INTRINSIC,
                vec![ident("expr", &span, &mut node_ids)],
                &span,
                &mut node_ids,
            ),
        ],
        &span,
        &mut node_ids,
    );

    let simplify_decl = MacroDecl {
        name: "simplify".to_owned(),
        params: vec![expr_param],
        body: simplify_body,
        span: span.clone(),
    };

    let input_expr = Expr::new(
        ExprKind::BinOp {
            op: BinOpKind::Mul,
            left: Box::new(Expr::new(
                ExprKind::BinOp {
                    op: BinOpKind::Add,
                    left: Box::new(number(42.0, &span, &mut node_ids)),
                    right: Box::new(number(0.0, &span, &mut node_ids)),
                },
                span.clone(),
                node_ids.next_id(),
            )),
            right: Box::new(number(1.0, &span, &mut node_ids)),
        },
        span.clone(),
        node_ids.next_id(),
    );

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![simplify_decl],
        body: intrinsic_call("simplify", vec![input_expr], &span, &mut node_ids),
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
    match expanded.program.body.kind {
        ExprKind::Number(value) => {
            assert!((value - 42.0).abs() < f64::EPSILON);
        }
        other => panic!("unexpected simplified expression: {other:?}"),
    }
}
