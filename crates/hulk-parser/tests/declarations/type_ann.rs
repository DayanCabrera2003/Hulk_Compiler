//! Type annotations: iterable (`T*`), vector (`T[]`), functor (`(T) -> U`).

use super::*;

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
