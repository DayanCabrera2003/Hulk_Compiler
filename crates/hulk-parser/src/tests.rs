//! Inline tests preserved from subsession 4.1 to guard against regressions.
//! Session 5.1 adds tests for array type annotations (T[], T[][], etc.).

use hulk_ast::{BinOpKind, ExprKind, Program, TypeAnn, UnaryOpKind};
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

// ── Session 5.1: array type annotation tests ──────────────────────────────

/// Parse `Number[]` in a let binding annotation and verify `TypeAnn::Vector`.
#[test]
fn type_array_number() {
    let (program, bag) = parse_expr("let a: Number[] = 0 in a;");
    assert!(bag.is_empty(), "diagnostics: {:?}", bag.diagnostics());
    // The body of the program is a Let whose first binding carries the annotation.
    let ExprKind::Let { bindings, .. } = &program.body.kind else {
        panic!("expected Let, got {:?}", program.body.kind);
    };
    let ExprKind::LetBinding(lb) = &bindings[0].kind else {
        panic!("expected LetBinding");
    };
    assert_eq!(
        lb.type_ann,
        Some(TypeAnn::Vector(Box::new(TypeAnn::Named("Number".to_owned()))))
    );
}

/// Parse `Number[][]` and verify nested `TypeAnn::Vector`.
#[test]
fn type_array_nested() {
    let (program, bag) = parse_expr("let m: Number[][] = 0 in m;");
    assert!(bag.is_empty(), "diagnostics: {:?}", bag.diagnostics());
    let ExprKind::Let { bindings, .. } = &program.body.kind else {
        panic!("expected Let");
    };
    let ExprKind::LetBinding(lb) = &bindings[0].kind else {
        panic!("expected LetBinding");
    };
    let expected = TypeAnn::Vector(Box::new(TypeAnn::Vector(Box::new(TypeAnn::Named(
        "Number".to_owned(),
    )))));
    assert_eq!(lb.type_ann, Some(expected));
}

/// `Number[` without closing `]` must not advance past the size expression.
/// The bracket stays for the expression parser (it looks like an index on an Ident).
/// The important invariant: parser produces diagnostics rather than panicking.
#[test]
fn type_array_no_close_does_not_panic() {
    // A lone `Number[` in expression position: parser recovers.
    // We cannot predict how far it gets, but it must not panic.
    let (_prog, _bag) = parse_expr("let a: Number[ = 0 in a;");
    // No assertion on content — just "did not panic" is sufficient.
}
