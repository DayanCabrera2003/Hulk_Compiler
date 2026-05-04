//! Literales `[a, b, c]`, generadores `[e | x in it]`, indexing, `T[]` y `T*`.

use hulk_ast::{ExprKind, TypeAnn};

use crate::common::{contains_expr, count_exprs, parse_ok};

const SRC: &str = include_str!("../../../../examples/vectors.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("vectors.hulk", SRC);
}

#[test]
fn declares_sum_with_iterable_type_param() {
    let program = parse_ok("vectors.hulk", SRC);
    let sum_fn = program
        .functions
        .iter()
        .find(|f| f.name == "sum")
        .expect("sum no declarada");
    let expected = TypeAnn::Iterable(Box::new(TypeAnn::Named("Number".into())));
    assert_eq!(sum_fn.params[0].type_ann, Some(expected));
}

#[test]
fn declares_first_with_vector_type_param() {
    let program = parse_ok("vectors.hulk", SRC);
    let first_fn = program
        .functions
        .iter()
        .find(|f| f.name == "first")
        .expect("first no declarada");
    let expected = TypeAnn::Vector(Box::new(TypeAnn::Named("Number".into())));
    assert_eq!(first_fn.params[0].type_ann, Some(expected));
}

#[test]
fn program_contains_vec_literal() {
    let program = parse_ok("vectors.hulk", SRC);
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::VecLiteral(_)
    )));
}

#[test]
fn program_contains_vec_generator() {
    let program = parse_ok("vectors.hulk", SRC);
    let has_gen = contains_expr(&program, |k| {
        matches!(
            k,
            ExprKind::VecGenerator { binding, .. } if binding == "x"
        )
    });
    assert!(has_gen);
}

#[test]
fn program_contains_indexing() {
    let program = parse_ok("vectors.hulk", SRC);
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::Index { .. }
    )));
}

#[test]
fn vec_literal_with_five_items_exists() {
    let program = parse_ok("vectors.hulk", SRC);
    let has_five = contains_expr(&program, |k| {
        matches!(
            k,
            ExprKind::VecLiteral(items) if items.len() == 5
        )
    });
    assert!(has_five);
}

#[test]
fn at_least_three_vec_literals() {
    let program = parse_ok("vectors.hulk", SRC);
    let n = count_exprs(&program, |k| matches!(k, ExprKind::VecLiteral(_)));
    assert!(n >= 3, "esperaba >=3 VecLiteral, obtuvo {n}");
}
