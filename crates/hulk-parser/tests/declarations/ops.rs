//! `is` / `as` operators, function calls, `base`, and `new Type(args)`.

use super::*;

#[test]
fn is_operator_with_named_type() {
    let program = parse_ok("x is Point;");
    let ExprKind::Is { type_ann, .. } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(*type_ann, TypeAnn::Named("Point".into()));
}

#[test]
fn as_operator_with_named_type() {
    let program = parse_ok("x as Point;");
    assert!(matches!(body(&program).kind, ExprKind::As { .. }));
}

#[test]
fn call_a_function() {
    let program = parse_ok("f(1, 2, 3);");
    let ExprKind::Call { callee, args } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(callee.kind, ExprKind::Ident(ref n) if n == "f"));
    assert_eq!(args.len(), 3);
}

#[test]
fn call_with_no_args() {
    let program = parse_ok("f();");
    let ExprKind::Call { args, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(args.is_empty());
}

#[test]
fn base_call() {
    // `base()` is the call to the parent implementation.
    let program = parse_ok("base();");
    let ExprKind::Call { callee, args } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(callee.kind, ExprKind::Base));
    assert!(args.is_empty());
}

#[test]
fn new_with_args() {
    let program = parse_ok("new Point(1, 2);");
    let ExprKind::New { type_ann, args } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(*type_ann, TypeAnn::Named("Point".into()));
    assert_eq!(args.len(), 2);
}

#[test]
fn new_without_args() {
    let program = parse_ok("new Point();");
    let ExprKind::New { args, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(args.is_empty());
}
