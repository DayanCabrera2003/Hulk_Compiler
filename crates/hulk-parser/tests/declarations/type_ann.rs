//! Type annotations: iterable (`T*`), vector (`T[]`), functor (`(T) -> U`).
//!
//! Tarea 4.1 — functor-type tests added to verify all forms of `(T) -> R`
//! annotations: one-arg, two-arg, zero-arg and nested functor returns.

use super::*;

// ---- Tarea 4.1: functor type annotation tests --------------------------------

#[test]
fn type_lambda_one_arg() {
    // `(Number) -> Number` in a let binding annotation.
    let program = parse_ok("let f: (Number) -> Number = 0 in f;");
    if let ExprKind::Let { bindings, .. } = &program.body.kind {
        let ExprKind::LetBinding(lb) = &bindings[0].kind else {
            panic!("expected LetBinding");
        };
        let ann = lb.type_ann.as_ref().expect("expected type annotation");
        match ann {
            TypeAnn::Functor { params, ret } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0], TypeAnn::Named("Number".into()));
                assert_eq!(**ret, TypeAnn::Named("Number".into()));
            }
            other => panic!("expected Functor, got {other:?}"),
        }
    } else {
        panic!("expected Let expression");
    }
}

#[test]
fn type_lambda_two_args() {
    // `(Number, Boolean) -> Number` as a function parameter annotation.
    let program =
        parse_ok("function apply(f: (Number, Boolean) -> Number, x: Number): Number => x;");
    match &program.functions[0].params[0].type_ann {
        Some(TypeAnn::Functor { params, ret }) => {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0], TypeAnn::Named("Number".into()));
            assert_eq!(params[1], TypeAnn::Named("Boolean".into()));
            assert_eq!(**ret, TypeAnn::Named("Number".into()));
        }
        other => panic!("expected two-arg Functor, got {other:?}"),
    }
}

#[test]
fn type_lambda_zero_args() {
    // `() -> Number` — nullary functor.
    let program = parse_ok("function thunk(f: () -> Number): Number => 0;");
    match &program.functions[0].params[0].type_ann {
        Some(TypeAnn::Functor { params, ret }) => {
            assert!(params.is_empty());
            assert_eq!(**ret, TypeAnn::Named("Number".into()));
        }
        other => panic!("expected zero-arg Functor, got {other:?}"),
    }
}

#[test]
fn type_lambda_nested() {
    // `(Number) -> (Number) -> Number` — curried/nested functor.
    let program =
        parse_ok("function curry(f: (Number) -> (Number) -> Number, x: Number): Number => 0;");
    match &program.functions[0].params[0].type_ann {
        Some(TypeAnn::Functor { params, ret }) => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0], TypeAnn::Named("Number".into()));
            match ret.as_ref() {
                TypeAnn::Functor {
                    params: inner_params,
                    ret: inner_ret,
                } => {
                    assert_eq!(inner_params.len(), 1);
                    assert_eq!(inner_params[0], TypeAnn::Named("Number".into()));
                    assert_eq!(**inner_ret, TypeAnn::Named("Number".into()));
                }
                other => panic!("expected nested Functor, got {other:?}"),
            }
        }
        other => panic!("expected outer Functor, got {other:?}"),
    }
}

// ---- Pre-existing tests -------------------------------------------------------

#[test]
fn iterable_type_annotation() {
    let program = parse_ok("function sum(xs: Number*): Number => 0;");
    let params = &program.functions[0].params;
    let expected = TypeAnn::Iterable(Box::new(TypeAnn::Named("Number".into())));
    assert_eq!(params[0].type_ann, Some(expected));
}

#[test]
fn vector_type_annotation() {
    let program = parse_ok("function mean(xs: Number[]): Number => 0;");
    let params = &program.functions[0].params;
    let expected = TypeAnn::Vector(Box::new(TypeAnn::Named("Number".into())));
    assert_eq!(params[0].type_ann, Some(expected));
}

#[test]
fn functor_type_annotation() {
    let program = parse_ok("function apply(f: (Number) -> Boolean, x: Number): Boolean => x > 0;");
    let params = &program.functions[0].params;
    match &params[0].type_ann {
        Some(TypeAnn::Functor {
            params: fn_params,
            ret,
        }) => {
            assert_eq!(fn_params.len(), 1);
            assert_eq!(fn_params[0], TypeAnn::Named("Number".into()));
            assert_eq!(**ret, TypeAnn::Named("Boolean".into()));
        }
        other => panic!("expected Functor type annotation, got {other:?}"),
    }
}

#[test]
fn functor_with_zero_params_type() {
    let program = parse_ok("function once(f: () -> Number): Number => 0;");
    match &program.functions[0].params[0].type_ann {
        Some(TypeAnn::Functor { params, .. }) => assert!(params.is_empty()),
        other => panic!("expected empty-param functor, got {other:?}"),
    }
}
