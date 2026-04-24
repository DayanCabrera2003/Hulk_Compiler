//! `let ... in ...` declarations.

use super::*;

#[test]
fn let_single_binding_without_annotation() {
    let program = parse_ok("let x = 42 in x;");
    let ExprKind::Let { bindings, body } = &body(&program).kind else {
        panic!("expected Let, got {:?}", body(&program).kind);
    };
    assert_eq!(bindings.len(), 1);
    let ExprKind::LetBinding(binding) = &bindings[0].kind else {
        panic!("binding not wrapped in LetBinding");
    };
    assert_eq!(binding.name, "x");
    assert!(binding.type_ann.is_none());
    assert!(matches!(binding.value.kind, ExprKind::Number(_)));
    assert!(matches!(body.kind, ExprKind::Ident(ref n) if n == "x"));
}

#[test]
fn let_with_type_annotation() {
    let program = parse_ok("let x: Number = 42 in x;");
    let ExprKind::Let { bindings, .. } = &body(&program).kind else {
        panic!("expected Let");
    };
    let ExprKind::LetBinding(binding) = &bindings[0].kind else {
        panic!()
    };
    assert_eq!(binding.type_ann, Some(TypeAnn::Named("Number".into())));
}

#[test]
fn let_multiple_bindings_in_one_let() {
    // Per spec: `let a = 6, b = a * 7 in print(b);` — multiple bindings in a
    // single let, later bindings may reference earlier ones.
    let program = parse_ok("let a = 6, b = 7 in a + b;");
    let ExprKind::Let { bindings, .. } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(bindings.len(), 2);
    let names: Vec<String> = bindings
        .iter()
        .map(|e| {
            if let ExprKind::LetBinding(b) = &e.kind {
                b.name.clone()
            } else {
                panic!("expected LetBinding")
            }
        })
        .collect();
    assert_eq!(names, vec!["a", "b"]);
}

#[test]
fn let_with_block_body() {
    let program = parse_ok("let x = 1 in { x; x + 1; };");
    let ExprKind::Let { body: let_body, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(let_body.kind, ExprKind::Block(_)));
}

#[test]
fn nested_let_right_associative() {
    // let a = 1 in let b = 2 in a + b
    let program = parse_ok("let a = 1 in let b = 2 in a + b;");
    let ExprKind::Let {
        body: outer_body, ..
    } = &body(&program).kind
    else {
        panic!()
    };
    assert!(matches!(outer_body.kind, ExprKind::Let { .. }));
}
