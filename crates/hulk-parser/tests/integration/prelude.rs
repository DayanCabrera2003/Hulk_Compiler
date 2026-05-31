//! Parser tests for prelude/prelude.hulk.
//!
//! The prelude defines Iterable, Enumerable, and Range (hulk-docs.pdf A.15).
//! Every user program has the prelude prepended before parsing, so these
//! declarations are always in scope.

use hulk_ast::MemberKind;

use crate::common::parse_ok;

const SRC: &str = include_str!("../../../../prelude/prelude.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("prelude.hulk", SRC);
}

#[test]
fn declares_iterable_and_enumerable_protocols() {
    let program = parse_ok("prelude.hulk", SRC);
    let names: Vec<_> = program.protocols.iter().map(|p| p.name.clone()).collect();
    assert!(names.contains(&"Iterable".to_string()));
    assert!(names.contains(&"Enumerable".to_string()));
}

#[test]
fn range_has_three_attrs_and_two_methods() {
    let program = parse_ok("prelude.hulk", SRC);
    let range = program
        .types
        .iter()
        .find(|t| t.name == "Range")
        .expect("Range not declared in prelude");
    let mut attrs = 0;
    let mut methods = 0;
    for m in &range.members {
        match m.kind {
            MemberKind::Attribute { .. } => attrs += 1,
            MemberKind::Method(_) => methods += 1,
        }
    }
    assert_eq!(attrs, 3);
    assert_eq!(methods, 2);
    assert_eq!(range.params.len(), 2);
}

#[test]
fn range_exposes_next_and_current_methods() {
    let program = parse_ok("prelude.hulk", SRC);
    let range = program
        .types
        .iter()
        .find(|t| t.name == "Range")
        .expect("Range not declared in prelude");
    let method_names: Vec<_> = range
        .members
        .iter()
        .filter_map(|m| match &m.kind {
            MemberKind::Method(f) => Some(f.name.clone()),
            MemberKind::Attribute { .. } => None,
        })
        .collect();
    assert!(method_names.contains(&"next".to_string()));
    assert!(method_names.contains(&"current".to_string()));
}
