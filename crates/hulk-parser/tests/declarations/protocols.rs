//! Protocol declarations.

use super::*;

#[test]
fn protocol_with_required_return_types() {
    let program = parse_ok(
        r#"protocol Iterable {
            next(): Boolean;
            current(): Object;
        }
        0;"#,
    );
    let proto = &program.protocols[0];
    assert_eq!(proto.name, "Iterable");
    assert_eq!(proto.methods.len(), 2);
    assert_eq!(
        proto.methods[0].return_type,
        TypeAnn::Named("Boolean".into())
    );
    assert_eq!(
        proto.methods[1].return_type,
        TypeAnn::Named("Object".into())
    );
}

#[test]
fn protocol_without_extends() {
    let program = parse_ok("protocol Hashable { hash(): Number; } 0;");
    assert!(program.protocols[0].extends.is_empty());
}

#[test]
fn protocol_with_extends() {
    let program =
        parse_ok("protocol Equatable extends Hashable { equals(other: Object): Boolean; } 0;");
    assert_eq!(program.protocols[0].extends, vec!["Hashable".to_string()]);
}

#[test]
fn protocol_method_without_return_type_reports_error() {
    let (program, bag) = parse_with_errors("protocol Bad { foo(); } 0;");
    assert!(bag.has_errors(), "missing return type must be an error");
    // Parser still produces a protocol entry for error recovery.
    assert_eq!(program.protocols.len(), 1);
}
