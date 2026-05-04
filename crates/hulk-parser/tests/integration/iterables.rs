//! Implementación canónica de `Iterable` (tipo `Range`).

use hulk_ast::{ExprKind, MemberKind};

use crate::common::parse_ok;

const SRC: &str = include_str!("../../../../examples/iterables.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("iterables.hulk", SRC);
}

#[test]
fn program_declares_iterable_protocol() {
    let program = parse_ok("iterables.hulk", SRC);
    assert!(program.protocols.iter().any(|p| p.name == "Iterable"));
}

#[test]
fn range_has_three_attrs_and_two_methods() {
    let program = parse_ok("iterables.hulk", SRC);
    let range = program
        .types
        .iter()
        .find(|t| t.name == "Range")
        .expect("Range no declarado");
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
    let program = parse_ok("iterables.hulk", SRC);
    let range = program
        .types
        .iter()
        .find(|t| t.name == "Range")
        .expect("Range");
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

#[test]
fn body_is_for_loop_over_new_range() {
    let program = parse_ok("iterables.hulk", SRC);
    let ExprKind::For { iterable, .. } = &program.body.kind else {
        panic!("body debe ser For");
    };
    assert!(matches!(iterable.kind, ExprKind::New { .. }));
}
