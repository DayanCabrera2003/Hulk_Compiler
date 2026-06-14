//! Lambda expressions.
//!
//! Covers both the old `(params) => body` syntax and the new
//! `function (params): T -> body` syntax added in Tarea 4.2.

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

// ---- Tarea 4.2: `function (params): T -> body` lambda syntax ---------------

#[test]
fn function_keyword_lambda_one_arg_no_capture() {
    // `function (x: Number): Number -> x * 2` parses to Lambda.
    let program = parse_ok("function (x: Number): Number -> x * 2;");
    let ExprKind::Lambda {
        params,
        return_type,
        body: lambda_body,
    } = &body(&program).kind
    else {
        panic!("expected Lambda, got {:?}", body(&program).kind)
    };
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "x");
    assert_eq!(params[0].type_ann, Some(TypeAnn::Named("Number".into())));
    assert_eq!(*return_type, Some(TypeAnn::Named("Number".into())));
    assert!(matches!(lambda_body.kind, ExprKind::BinOp { .. }));
}

#[test]
fn function_keyword_lambda_zero_args() {
    // `function (): Number -> 42` — no parameters.
    let program = parse_ok("function (): Number -> 42;");
    let ExprKind::Lambda { params, .. } = &body(&program).kind else {
        panic!("expected Lambda");
    };
    assert!(params.is_empty());
}

#[test]
fn function_keyword_lambda_no_return_type_annotation() {
    // `function (x) -> x + 1` — no return type annotation.
    let program = parse_ok("function (x) -> x + 1;");
    let ExprKind::Lambda {
        params,
        return_type,
        ..
    } = &body(&program).kind
    else {
        panic!("expected Lambda");
    };
    assert_eq!(params.len(), 1);
    assert!(return_type.is_none());
}

#[test]
fn function_keyword_lambda_in_let_binding() {
    // `let f: (Number) -> Number = function (x: Number): Number -> x * 2 in f`
    let program =
        parse_ok("let f: (Number) -> Number = function (x: Number): Number -> x * 2 in f;");
    let ExprKind::Let {
        bindings,
        body: let_body,
    } = &body(&program).kind
    else {
        panic!("expected Let");
    };
    // The binding value is a Lambda.
    let ExprKind::LetBinding(lb) = &bindings[0].kind else {
        panic!("expected LetBinding");
    };
    assert!(matches!(lb.value.kind, ExprKind::Lambda { .. }));
    // Body is just the ident `f`.
    assert!(matches!(let_body.kind, ExprKind::Ident(ref n) if n == "f"));
}

#[test]
fn function_keyword_lambda_as_argument() {
    // `apply(function (x: Number): Number -> x + 1, 5)` — lambda as call arg.
    let program = parse_ok("apply(function (x: Number): Number -> x + 1, 5);");
    let ExprKind::Call { args, .. } = &body(&program).kind else {
        panic!("expected Call");
    };
    assert_eq!(args.len(), 2);
    assert!(matches!(args[0].kind, ExprKind::Lambda { .. }));
}

#[test]
fn function_keyword_lambda_does_not_interfere_with_named_decl() {
    // A named function at top level still produces a FunctionDecl, not a Lambda.
    let program = parse_ok("function double(x: Number): Number => x * 2; double(3);");
    assert_eq!(program.functions.len(), 1);
    assert_eq!(program.functions[0].name, "double");
    // Body is the call expression, not a lambda.
    assert!(matches!(body(&program).kind, ExprKind::Call { .. }));
}
