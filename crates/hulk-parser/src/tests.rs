//! Inline tests preserved from subsession 4.1 to guard against regressions.

use hulk_ast::{BinOpKind, ExprKind, Program, UnaryOpKind};
use hulk_diagnostics::DiagnosticBag;
use hulk_lexer::lex;
use hulk_tokens::SourceFile;

use crate::parse;

fn parse_expr(source: &str) -> (Program, DiagnosticBag) {
    let source = SourceFile::new("test.hulk", source);
    let mut lex_bag = DiagnosticBag::new();
    let tokens = lex(&source, &mut lex_bag);
    assert!(
        lex_bag.is_empty(),
        "lexer diagnostics: {:?}",
        lex_bag.diagnostics()
    );
    parse(tokens, &source)
}

#[test]
fn parses_arithmetic_precedence() {
    let (program, bag) = parse_expr("1 + 2 * 3;");
    assert!(
        bag.is_empty(),
        "parser diagnostics: {:?}",
        bag.diagnostics()
    );

    match &program.body.kind {
        ExprKind::BinOp {
            op: BinOpKind::Add,
            left,
            right,
        } => {
            assert!(matches!(left.kind, ExprKind::Number(v) if (v - 1.0).abs() < f64::EPSILON));
            match &right.kind {
                ExprKind::BinOp {
                    op: BinOpKind::Mul,
                    left: mul_l,
                    right: mul_r,
                } => {
                    assert!(matches!(
                        mul_l.kind,
                        ExprKind::Number(v) if (v - 2.0).abs() < f64::EPSILON
                    ));
                    assert!(matches!(
                        mul_r.kind,
                        ExprKind::Number(v) if (v - 3.0).abs() < f64::EPSILON
                    ));
                }
                other => panic!("expected mul expression, got {other:?}"),
            }
        }
        other => panic!("expected add expression, got {other:?}"),
    }
}

#[test]
fn parses_unary_and_grouping() {
    let (program, bag) = parse_expr("-(1 + 2);");
    assert!(
        bag.is_empty(),
        "parser diagnostics: {:?}",
        bag.diagnostics()
    );

    match &program.body.kind {
        ExprKind::UnaryOp {
            op: UnaryOpKind::Neg,
            expr,
        } => match &expr.kind {
            ExprKind::BinOp {
                op: BinOpKind::Add, ..
            } => {}
            other => panic!("expected grouped add expression, got {other:?}"),
        },
        other => panic!("expected unary negation, got {other:?}"),
    }
}

#[test]
fn parses_boolean_and_concat() {
    let (program, bag) = parse_expr("true & false | \"a\" @@ \"b\";");
    assert!(
        bag.is_empty(),
        "parser diagnostics: {:?}",
        bag.diagnostics()
    );

    // Expected precedence: `&` binds tighter than `|` and `@@`.
    // Parser resolves the expression as:  (true & false) | ("a" @@ "b")
    match &program.body.kind {
        ExprKind::BinOp {
            op: BinOpKind::Or,
            left,
            right,
        } => {
            assert!(matches!(
                left.kind,
                ExprKind::BinOp {
                    op: BinOpKind::And,
                    ..
                }
            ));
            assert!(matches!(
                right.kind,
                ExprKind::BinOp {
                    op: BinOpKind::ConcatSpaced,
                    ..
                }
            ));
        }
        other => panic!("expected or expression, got {other:?}"),
    }
}
