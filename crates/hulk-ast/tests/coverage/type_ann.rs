//! TypeAnn variants including deep nesting.

use super::*;

#[test]
fn type_ann_supports_arbitrary_nesting() {
    // Number[][] (vector of vectors)
    let nested_vec = TypeAnn::Vector(Box::new(TypeAnn::Vector(Box::new(TypeAnn::Named(
        "Number".to_owned(),
    )))));
    assert!(matches!(nested_vec, TypeAnn::Vector(_)));

    // Number*[] (iterable returning vectors) — syntactically allowed by the AST
    let iter_of_vec = TypeAnn::Iterable(Box::new(TypeAnn::Vector(Box::new(TypeAnn::Named(
        "Number".to_owned(),
    )))));
    assert!(matches!(iter_of_vec, TypeAnn::Iterable(_)));

    // (Number, Number) -> Boolean
    let functor = TypeAnn::Functor {
        params: vec![
            TypeAnn::Named("Number".to_owned()),
            TypeAnn::Named("Number".to_owned()),
        ],
        ret: Box::new(TypeAnn::Named("Boolean".to_owned())),
    };
    assert!(matches!(functor, TypeAnn::Functor { .. }));

    // (Number*) -> Number[]
    let high_order = TypeAnn::Functor {
        params: vec![TypeAnn::Iterable(Box::new(TypeAnn::Named(
            "Number".to_owned(),
        )))],
        ret: Box::new(TypeAnn::Vector(Box::new(TypeAnn::Named(
            "Number".to_owned(),
        )))),
    };
    assert!(matches!(high_order, TypeAnn::Functor { .. }));
}

#[test]
fn functor_with_zero_params_is_representable() {
    let thunk = TypeAnn::Functor {
        params: vec![],
        ret: Box::new(TypeAnn::Named("Object".to_owned())),
    };
    if let TypeAnn::Functor { params, .. } = thunk {
        assert!(params.is_empty());
    } else {
        panic!();
    }
}
