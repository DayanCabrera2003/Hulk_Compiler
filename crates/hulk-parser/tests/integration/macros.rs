//! `def` con parámetros regulares, `*`, `@` y `$`.

use hulk_ast::{MacroParam, TypeAnn};

use crate::common::parse_ok;

const SRC: &str = include_str!("../../../../examples/macros.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("macros.hulk", SRC);
}

#[test]
fn declares_three_macros_in_order() {
    let program = parse_ok("macros.hulk", SRC);
    let names: Vec<_> = program.macros.iter().map(|m| m.name.clone()).collect();
    assert_eq!(names, vec!["repeat", "swap", "repeat_iter"]);
}

#[test]
fn repeat_has_regular_then_body_params() {
    let program = parse_ok("macros.hulk", SRC);
    let repeat = program
        .macros
        .iter()
        .find(|m| m.name == "repeat")
        .expect("repeat");
    assert_eq!(repeat.params.len(), 2);
    assert!(matches!(repeat.params[0], MacroParam::Regular { .. }));
    assert!(matches!(repeat.params[1], MacroParam::Body { .. }));
}

#[test]
fn swap_has_two_symbolic_params() {
    let program = parse_ok("macros.hulk", SRC);
    let swap = program
        .macros
        .iter()
        .find(|m| m.name == "swap")
        .expect("swap");
    assert_eq!(swap.params.len(), 2);
    for p in &swap.params {
        assert!(matches!(p, MacroParam::Symbolic { .. }));
    }
}

#[test]
fn repeat_iter_has_placeholder_then_regular_then_body() {
    let program = parse_ok("macros.hulk", SRC);
    let ri = program
        .macros
        .iter()
        .find(|m| m.name == "repeat_iter")
        .expect("repeat_iter");
    assert_eq!(ri.params.len(), 3);
    assert!(matches!(ri.params[0], MacroParam::Placeholder { .. }));
    assert!(matches!(ri.params[1], MacroParam::Regular { .. }));
    assert!(matches!(ri.params[2], MacroParam::Body { .. }));
}

#[test]
fn every_macro_param_retains_type_annotation() {
    let program = parse_ok("macros.hulk", SRC);
    for m in &program.macros {
        for p in &m.params {
            match p.type_ann() {
                TypeAnn::Named(_) => {}
                other => panic!("tipo inesperado {other:?} en macro {}", m.name),
            }
        }
    }
}
