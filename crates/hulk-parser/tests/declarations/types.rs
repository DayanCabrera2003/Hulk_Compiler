//! Type declarations: attributes, methods, constructor params, and inheritance.

use super::*;

#[test]
fn type_with_attribute() {
    let program = parse_ok(
        r#"type Point {
            x = 0;
            y = 0;
        }
        0;"#,
    );
    let ty = &program.types[0];
    assert_eq!(ty.name, "Point");
    assert_eq!(ty.members.len(), 2);
    for member in &ty.members {
        assert!(matches!(member.kind, MemberKind::Attribute { .. }));
    }
}

#[test]
fn type_attribute_with_annotation() {
    let program = parse_ok(
        r#"type Box {
            value: Number = 0;
        }
        0;"#,
    );
    match &program.types[0].members[0].kind {
        MemberKind::Attribute {
            type_ann,
            name,
            value,
        } => {
            assert_eq!(name, "value");
            assert_eq!(*type_ann, Some(TypeAnn::Named("Number".into())));
            assert!(matches!(value.kind, ExprKind::Number(_)));
        }
        _ => panic!("expected annotated attribute"),
    }
}

#[test]
fn type_with_inline_method() {
    let program = parse_ok(
        r#"type Point {
            x = 0;
            getX() => self.x;
        }
        0;"#,
    );
    let members = &program.types[0].members;
    match &members[1].kind {
        MemberKind::Method(f) => {
            assert_eq!(f.name, "getX");
            assert!(matches!(f.body.kind, ExprKind::FieldAccess { .. }));
        }
        _ => panic!("expected method"),
    }
}

#[test]
fn type_with_full_form_method() {
    let program = parse_ok(
        r#"type Point {
            x = 0;
            describe() {
                self.x;
            }
        }
        0;"#,
    );
    match &program.types[0].members[1].kind {
        MemberKind::Method(f) => assert!(matches!(f.body.kind, ExprKind::Block(_))),
        _ => panic!(),
    }
}

#[test]
fn type_with_constructor_params() {
    let program = parse_ok(
        r#"type Point(x: Number, y: Number) {
            x: Number = x;
            y: Number = y;
        }
        0;"#,
    );
    let ty = &program.types[0];
    assert_eq!(ty.params.len(), 2);
    assert_eq!(ty.params[0].type_ann, Some(TypeAnn::Named("Number".into())));
}

#[test]
fn type_with_inheritance() {
    let program = parse_ok(
        r#"type PolarPoint(phi, rho) inherits Point(rho, phi) {
            rho() => self.rho;
        }
        0;"#,
    );
    let parent = program.types[0].parent.as_ref().expect("parent missing");
    assert_eq!(parent.name, "Point");
    assert_eq!(parent.args.len(), 2);
}

#[test]
fn type_without_inheritance_has_no_parent() {
    let program = parse_ok(
        r#"type Empty {
            a = 1;
        }
        0;"#,
    );
    assert!(program.types[0].parent.is_none());
}
