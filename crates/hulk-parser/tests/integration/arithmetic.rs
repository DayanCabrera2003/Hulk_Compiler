//! Precedencia, potencia, builtins matemáticas y módulo.

use hulk_ast::{BinOpKind, ExprKind, UnaryOpKind};

use crate::common::{contains_expr, count_exprs, parse_ok};

const SRC: &str = include_str!("../../../../examples/arithmetic.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("arithmetic.hulk", SRC);
}

#[test]
fn body_is_block_with_six_expressions() {
    let program = parse_ok("arithmetic.hulk", SRC);
    let ExprKind::Block(exprs) = &program.body.kind else {
        panic!("body debe ser Block");
    };
    assert_eq!(exprs.len(), 6);
}

#[test]
fn contains_power_operator() {
    let program = parse_ok("arithmetic.hulk", SRC);
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::BinOp {
            op: BinOpKind::Pow,
            ..
        }
    )));
}

#[test]
fn contains_modulo_operator() {
    let program = parse_ok("arithmetic.hulk", SRC);
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::BinOp {
            op: BinOpKind::Mod,
            ..
        }
    )));
}

#[test]
fn contains_unary_operators() {
    let program = parse_ok("arithmetic.hulk", SRC);
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::UnaryOp {
            op: UnaryOpKind::Neg,
            ..
        }
    )));
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::UnaryOp {
            op: UnaryOpKind::Not,
            ..
        }
    )));
}

#[test]
fn contains_builtin_math_identifiers() {
    let program = parse_ok("arithmetic.hulk", SRC);
    for name in ["sin", "cos", "log", "PI"] {
        assert!(
            contains_expr(&program, |k| matches!(k, ExprKind::Ident(n) if n == name)),
            "esperaba referencia a {name}"
        );
    }
}

#[test]
fn nested_power_is_right_associative_on_inline_source() {
    // 2 ^ 3 ^ 2 parses como 2 ^ (3 ^ 2). Verificado con include_str! no es
    // trivial (hay muchos Pow); corroboramos con source inline.
    let program = parse_ok("pow.hulk", "2 ^ 3 ^ 2;");
    let ExprKind::BinOp {
        op: BinOpKind::Pow,
        right,
        ..
    } = &program.body.kind
    else {
        panic!("se esperaba Pow en raíz");
    };
    assert!(matches!(
        right.kind,
        ExprKind::BinOp {
            op: BinOpKind::Pow,
            ..
        }
    ));
}

#[test]
fn at_least_three_call_expressions() {
    // Seis `print(...)` a nivel de bloque + los internos (sin, cos, log).
    let program = parse_ok("arithmetic.hulk", SRC);
    let calls = count_exprs(&program, |k| matches!(k, ExprKind::Call { .. }));
    assert!(calls >= 8, "esperaba >=8 calls, obtuvo {calls}");
}
