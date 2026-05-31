//! Declarations: FunctionDecl, TypeDecl, ProtocolDecl, MacroDecl.

use super::*;

#[test]
fn function_decl_preserves_return_type_annotation() {
    let f = FunctionDecl {
        name: "tan".to_owned(),
        params: vec![Param {
            name: "x".to_owned(),
            type_ann: Some(TypeAnn::Named("Number".to_owned())),
            span: fresh_span(),
        }],
        return_type: Some(TypeAnn::Named("Number".to_owned())),
        body: num(0.0, 1),
        span: fresh_span(),
    };
    assert_eq!(f.return_type, Some(TypeAnn::Named("Number".to_owned())));

    let no_return = FunctionDecl {
        name: "id".to_owned(),
        params: vec![],
        return_type: None,
        body: num(0.0, 1),
        span: fresh_span(),
    };
    assert!(no_return.return_type.is_none());
}

#[test]
fn type_decl_supports_inheritance_and_mixed_members() {
    let parent = ParentSpec {
        name: "Point".to_owned(),
        args: vec![num(0.0, 1), num(0.0, 2)],
        span: fresh_span(),
    };
    let attr = Member {
        kind: MemberKind::Attribute {
            name: "x".to_owned(),
            type_ann: Some(TypeAnn::Named("Number".to_owned())),
            value: num(0.0, 3),
        },
        span: fresh_span(),
    };
    let method = Member {
        kind: MemberKind::Method(FunctionDecl {
            name: "getX".to_owned(),
            params: vec![],
            return_type: Some(TypeAnn::Named("Number".to_owned())),
            body: ident("self", 4),
            span: fresh_span(),
        }),
        span: fresh_span(),
    };

    let type_decl = TypeDecl {
        name: "PolarPoint".to_owned(),
        params: vec![Param {
            name: "phi".to_owned(),
            type_ann: None,
            span: fresh_span(),
        }],
        parent: Some(parent),
        members: vec![attr, method],
        span: fresh_span(),
    };

    assert_eq!(type_decl.members.len(), 2);
    assert!(type_decl.parent.is_some());
    assert!(matches!(
        type_decl.members[0].kind,
        MemberKind::Attribute { .. }
    ));
    assert!(matches!(type_decl.members[1].kind, MemberKind::Method(_)));
}

#[test]
fn attribute_value_is_required_not_optional() {
    // This test encodes the fact that attributes MUST have an initializer
    // (per hulk-docs.pdf "Types" section). If someone tries to make `value`
    // optional again, the type of the field will change and this test
    // will fail at compile time.
    let attr = MemberKind::Attribute {
        name: "x".to_owned(),
        type_ann: None,
        value: num(42.0, 1),
    };
    // Destructure to force a compile-time check that `value` is `Expr`, not
    // `Option<Expr>`.
    if let MemberKind::Attribute { value, .. } = attr {
        // If value were Option<Expr>, the next line wouldn't compile.
        let _kind: &ExprKind = &value.kind;
    } else {
        panic!();
    }
}

#[test]
fn protocol_decl_supports_zero_or_many_extensions() {
    let p0 = ProtocolDecl {
        name: "Hashable".to_owned(),
        extends: vec![],
        methods: vec![MethodSig {
            name: "hash".to_owned(),
            params: vec![],
            return_type: TypeAnn::Named("Number".to_owned()),
            span: fresh_span(),
        }],
        span: fresh_span(),
    };
    let p1 = ProtocolDecl {
        name: "Equatable".to_owned(),
        extends: vec!["Hashable".to_owned()],
        methods: vec![],
        span: fresh_span(),
    };
    assert!(p0.extends.is_empty());
    assert_eq!(p1.extends, vec!["Hashable".to_owned()]);
}

#[test]
fn method_sig_requires_return_type_annotation() {
    // Compile-time check: return_type on MethodSig is TypeAnn, not Option.
    // Protocols cannot have unannotated return types per the spec.
    let sig = MethodSig {
        name: "next".to_owned(),
        params: vec![],
        return_type: TypeAnn::Named("Boolean".to_owned()),
        span: fresh_span(),
    };
    let _: &TypeAnn = &sig.return_type;
}

#[test]
fn macro_decl_supports_all_four_param_kinds() {
    let mac = MacroDecl {
        name: "repeat".to_owned(),
        params: vec![
            MacroParam::Placeholder {
                name: "iter".to_owned(),
                type_ann: TypeAnn::Named("Number".to_owned()),
                span: fresh_span(),
            },
            MacroParam::Regular {
                name: "n".to_owned(),
                type_ann: TypeAnn::Named("Number".to_owned()),
                span: fresh_span(),
            },
            MacroParam::Symbolic {
                name: "target".to_owned(),
                type_ann: TypeAnn::Named("Object".to_owned()),
                span: fresh_span(),
            },
            MacroParam::Body {
                name: "expr".to_owned(),
                type_ann: TypeAnn::Named("Object".to_owned()),
                span: fresh_span(),
            },
        ],
        body: num(0.0, 1),
        span: fresh_span(),
    };

    assert_eq!(mac.params.len(), 4);
    assert_eq!(mac.params[0].name(), "iter");
    assert_eq!(mac.params[1].name(), "n");
    assert_eq!(mac.params[2].name(), "target");
    assert_eq!(mac.params[3].name(), "expr");

    for p in &mac.params {
        assert_eq!(*p.type_ann(), p.type_ann().clone());
    }
}

#[test]
fn macro_param_name_and_type_ann_are_accessible_uniformly() {
    let span = fresh_span();
    let cases = [
        MacroParam::Regular {
            name: "a".to_owned(),
            type_ann: TypeAnn::Named("Number".to_owned()),
            span: span.clone(),
        },
        MacroParam::Body {
            name: "b".to_owned(),
            type_ann: TypeAnn::Named("Object".to_owned()),
            span: span.clone(),
        },
        MacroParam::Symbolic {
            name: "c".to_owned(),
            type_ann: TypeAnn::Named("Object".to_owned()),
            span: span.clone(),
        },
        MacroParam::Placeholder {
            name: "d".to_owned(),
            type_ann: TypeAnn::Named("Number".to_owned()),
            span,
        },
    ];
    let names: Vec<_> = cases.iter().map(MacroParam::name).collect();
    assert_eq!(names, vec!["a", "b", "c", "d"]);
}
