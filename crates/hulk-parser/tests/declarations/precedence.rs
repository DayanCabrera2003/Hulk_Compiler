//! Precedence and associativity regression tests.

use super::*;

#[test]
fn postfix_binds_tighter_than_binary() {
    // obj.f() + 1  ⇒  (obj.f()) + 1
    let program = parse_ok("obj.f() + 1;");
    let ExprKind::BinOp { left, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(left.kind, ExprKind::MethodCall { .. }));
}

#[test]
fn unary_binds_tighter_than_power() {
    // -x ^ 2 parses as (-x) ^ 2 in HULK (unary > pow per PIPELINE).
    let program = parse_ok("-x ^ 2;");
    let ExprKind::BinOp {
        op: BinOpKind::Pow,
        left,
        ..
    } = &body(&program).kind
    else {
        panic!("expected pow at root")
    };
    assert!(matches!(
        left.kind,
        ExprKind::UnaryOp {
            op: UnaryOpKind::Neg,
            ..
        }
    ));
}

#[test]
fn power_is_right_associative() {
    // 2 ^ 3 ^ 2 parses as 2 ^ (3 ^ 2)
    let program = parse_ok("2 ^ 3 ^ 2;");
    let ExprKind::BinOp {
        op: BinOpKind::Pow,
        right,
        ..
    } = &body(&program).kind
    else {
        panic!()
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
fn assignment_has_lowest_precedence() {
    // x := 1 + 2 parses as x := (1 + 2), not (x := 1) + 2
    let program = parse_ok("x := 1 + 2;");
    let ExprKind::Assign { value, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(
        value.kind,
        ExprKind::BinOp {
            op: BinOpKind::Add,
            ..
        }
    ));
}
