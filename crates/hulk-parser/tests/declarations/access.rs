//! Postfix access: field access, method call, index.

use super::*;

#[test]
fn field_access_chain() {
    let program = parse_ok("p.x.y;");
    let ExprKind::FieldAccess { receiver, field } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(field, "y");
    assert!(matches!(receiver.kind, ExprKind::FieldAccess { .. }));
}

#[test]
fn method_call_with_args() {
    let program = parse_ok("obj.method(1, 2);");
    let ExprKind::MethodCall { method, args, .. } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(method, "method");
    assert_eq!(args.len(), 2);
}

#[test]
fn method_call_chain() {
    let program = parse_ok("obj.a().b();");
    let ExprKind::MethodCall {
        receiver,
        method,
        args,
    } = &body(&program).kind
    else {
        panic!()
    };
    assert_eq!(method, "b");
    assert!(args.is_empty());
    assert!(matches!(receiver.kind, ExprKind::MethodCall { .. }));
}

#[test]
fn index_access() {
    let program = parse_ok("v[3];");
    let ExprKind::Index { target, index } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(target.kind, ExprKind::Ident(_)));
    assert!(matches!(index.kind, ExprKind::Number(_)));
}

#[test]
fn index_of_method_result() {
    let program = parse_ok("obj.items()[0];");
    assert!(matches!(body(&program).kind, ExprKind::Index { .. }));
}
