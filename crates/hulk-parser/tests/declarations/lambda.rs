//! Lambda expressions.

use super::*;

#[test]
fn lambda_without_annotations() {
    let program = parse_ok("(x) => x + 1;");
    let ExprKind::Lambda {
        params,
        return_type,
        body: lambda_body,
    } = &body(&program).kind
    else {
        panic!()
    };
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "x");
    assert!(params[0].type_ann.is_none());
    assert!(return_type.is_none());
    assert!(matches!(lambda_body.kind, ExprKind::BinOp { .. }));
}

#[test]
fn lambda_with_typed_param_and_return() {
    let program = parse_ok("(x: Number): Boolean => x > 0;");
    let ExprKind::Lambda {
        params,
        return_type,
        ..
    } = &body(&program).kind
    else {
        panic!()
    };
    assert_eq!(params[0].type_ann, Some(TypeAnn::Named("Number".into())));
    assert_eq!(*return_type, Some(TypeAnn::Named("Boolean".into())));
}

#[test]
fn lambda_with_no_params() {
    let program = parse_ok("() => 42;");
    let ExprKind::Lambda { params, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(params.is_empty());
}

#[test]
fn lambda_multiple_params() {
    let program = parse_ok("(a, b, c) => a + b + c;");
    let ExprKind::Lambda { params, .. } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(params.len(), 3);
}

#[test]
fn grouping_vs_lambda_is_disambiguated() {
    // `(x)` is grouping when not followed by `=>`.
    let program = parse_ok("(1 + 2);");
    assert!(matches!(body(&program).kind, ExprKind::BinOp { .. }));

    // `(x) =>` is a lambda.
    let program = parse_ok("(x) => x;");
    assert!(matches!(body(&program).kind, ExprKind::Lambda { .. }));
}
