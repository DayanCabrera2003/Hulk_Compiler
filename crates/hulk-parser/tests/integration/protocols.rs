//! `protocol` con extends y conformance implícita a través de un tipo concreto.

use hulk_ast::{ExprKind, TypeAnn};

use crate::common::parse_ok;

const SRC: &str = include_str!("../../../../examples/protocols.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("protocols.hulk", SRC);
}

#[test]
fn declares_two_protocols_in_order() {
    // Iterable and Enumerable are defined in prelude/prelude.hulk (see prelude.rs tests).
    let program = parse_ok("protocols.hulk", SRC);
    let names: Vec<_> = program.protocols.iter().map(|p| p.name.clone()).collect();
    assert_eq!(names, vec!["Hashable", "Equatable"]);
}

#[test]
fn hashable_has_one_method_with_number_return_type() {
    let program = parse_ok("protocols.hulk", SRC);
    let hashable = program
        .protocols
        .iter()
        .find(|p| p.name == "Hashable")
        .expect("Hashable");
    assert_eq!(hashable.methods.len(), 1);
    assert_eq!(hashable.methods[0].name, "hash");
    assert_eq!(
        hashable.methods[0].return_type,
        TypeAnn::Named("Number".into())
    );
}

#[test]
fn equatable_extends_hashable() {
    let program = parse_ok("protocols.hulk", SRC);
    let eq = program
        .protocols
        .iter()
        .find(|p| p.name == "Equatable")
        .expect("Equatable");
    assert_eq!(eq.extends, vec!["Hashable".to_string()]);
}

#[test]
fn hashable_person_declares_hash_and_equals() {
    let program = parse_ok("protocols.hulk", SRC);
    let ty = program
        .types
        .iter()
        .find(|t| t.name == "HashablePerson")
        .expect("HashablePerson");
    let method_names: Vec<_> = ty
        .members
        .iter()
        .filter_map(|m| match &m.kind {
            hulk_ast::MemberKind::Method(f) => Some(f.name.clone()),
            hulk_ast::MemberKind::Attribute { .. } => None,
        })
        .collect();
    assert!(method_names.contains(&"hash".to_string()));
    assert!(method_names.contains(&"equals".to_string()));
}

#[test]
fn body_uses_protocol_annotation_in_let() {
    let program = parse_ok("protocols.hulk", SRC);
    let ExprKind::Let { bindings, .. } = &program.body.kind else {
        panic!("body debe ser Let");
    };
    let ExprKind::LetBinding(b) = &bindings[0].kind else {
        panic!("binding debe ser LetBinding");
    };
    assert_eq!(b.type_ann, Some(TypeAnn::Named("Hashable".into())));
}
