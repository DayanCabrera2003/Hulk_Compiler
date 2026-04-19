//! `protocol NumberFilter`, lambdas y anotación functor `(A) -> B`.

use hulk_ast::{ExprKind, TypeAnn};

use crate::common::{contains_expr, count_exprs, parse_ok};

const SRC: &str = include_str!("../../../../examples/functors.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("functors.hulk", SRC);
}

#[test]
fn declares_number_filter_protocol_with_invoke() {
    let program = parse_ok("functors.hulk", SRC);
    let proto = program
        .protocols
        .iter()
        .find(|p| p.name == "NumberFilter")
        .expect("NumberFilter");
    assert_eq!(proto.methods.len(), 1);
    assert_eq!(proto.methods[0].name, "invoke");
    assert_eq!(
        proto.methods[0].return_type,
        TypeAnn::Named("Boolean".into())
    );
}

#[test]
fn count_when_has_functor_typed_filter_param() {
    let program = parse_ok("functors.hulk", SRC);
    let count_when = program
        .functions
        .iter()
        .find(|f| f.name == "count_when")
        .expect("count_when");
    let filter_ann = &count_when.params[1].type_ann;
    let Some(TypeAnn::Functor { params, ret }) = filter_ann else {
        panic!("filter debe ser Functor, got {filter_ann:?}");
    };
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], TypeAnn::Named("Number".into()));
    assert_eq!(**ret, TypeAnn::Named("Boolean".into()));
}

#[test]
fn declares_is_odd_function() {
    let program = parse_ok("functors.hulk", SRC);
    assert!(program.functions.iter().any(|f| f.name == "is_odd"));
}

#[test]
fn program_contains_at_least_two_lambdas() {
    let program = parse_ok("functors.hulk", SRC);
    let lambdas = count_exprs(&program, |k| matches!(k, ExprKind::Lambda { .. }));
    assert!(lambdas >= 2, "esperaba >=2 lambdas, obtuvo {lambdas}");
}

#[test]
fn one_lambda_has_typed_param_and_return() {
    let program = parse_ok("functors.hulk", SRC);
    let has_typed = contains_expr(&program, |k| {
        matches!(
            k,
            ExprKind::Lambda { params, return_type, .. }
                if return_type.is_some() && params.first().is_some_and(|p| p.type_ann.is_some())
        )
    });
    assert!(has_typed);
}

#[test]
fn one_lambda_has_no_annotations() {
    let program = parse_ok("functors.hulk", SRC);
    let has_bare = contains_expr(&program, |k| {
        matches!(
            k,
            ExprKind::Lambda { params, return_type, .. }
                if return_type.is_none() && params.first().is_some_and(|p| p.type_ann.is_none())
        )
    });
    assert!(has_bare);
}
