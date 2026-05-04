//! `:=` destructive assignment.

use super::*;

#[test]
fn assign_to_ident() {
    let program = parse_ok("x := 1;");
    let ExprKind::Assign { target, value } = &body(&program).kind else {
        panic!()
    };
    let ExprKind::AssignTarget(target_inner) = &target.kind else {
        panic!()
    };
    assert_eq!(*target_inner, AssignTarget::Ident("x".into()));
    assert!(matches!(value.kind, ExprKind::Number(_)));
}

#[test]
fn assign_to_field() {
    let program = parse_ok("p.x := 1;");
    let ExprKind::Assign { target, .. } = &body(&program).kind else {
        panic!()
    };
    let ExprKind::AssignTarget(AssignTarget::Field { field, .. }) = &target.kind else {
        panic!()
    };
    assert_eq!(field, "x");
}

#[test]
fn assign_to_index() {
    let program = parse_ok("v[0] := 1;");
    let ExprKind::Assign { target, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(
        target.kind,
        ExprKind::AssignTarget(AssignTarget::Index { .. })
    ));
}

#[test]
fn assign_right_associative() {
    // a := b := 1 parses as a := (b := 1)
    let program = parse_ok("a := b := 1;");
    let ExprKind::Assign {
        value: outer_value, ..
    } = &body(&program).kind
    else {
        panic!()
    };
    assert!(matches!(outer_value.kind, ExprKind::Assign { .. }));
}

#[test]
fn assign_to_invalid_target_emits_diagnostic() {
    let (_, bag) = parse_with_errors("(1 + 2) := 3;");
    assert!(
        bag.has_errors(),
        "expected diagnostic for invalid := target"
    );
}
