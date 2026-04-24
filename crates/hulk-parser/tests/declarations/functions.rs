//! Function declarations (inline and full-form).

use super::*;

#[test]
fn inline_function_without_return_type() {
    let program = parse_ok("function id(x) => x; 0;");
    assert_eq!(program.functions.len(), 1);
    let f = &program.functions[0];
    assert_eq!(f.name, "id");
    assert!(f.return_type.is_none());
    assert!(matches!(f.body.kind, ExprKind::Ident(_)));
}

#[test]
fn inline_function_with_types() {
    let program = parse_ok("function tan(x: Number): Number => x; 0;");
    let f = &program.functions[0];
    assert_eq!(f.params.len(), 1);
    assert_eq!(f.params[0].type_ann, Some(TypeAnn::Named("Number".into())));
    assert_eq!(f.return_type, Some(TypeAnn::Named("Number".into())));
}

#[test]
fn full_form_function_has_block_body() {
    let program = parse_ok(
        r#"function operate(x, y) {
            x;
            y;
        }
        0;"#,
    );
    let f = &program.functions[0];
    assert!(matches!(f.body.kind, ExprKind::Block(_)));
}

#[test]
fn function_with_recursive_self_call() {
    let program = parse_ok("function fib(n) => fib(n); 0;");
    let f = &program.functions[0];
    assert!(matches!(f.body.kind, ExprKind::Call { .. }));
}

#[test]
fn multiple_functions_preserve_order() {
    let program = parse_ok("function a() => 1; function b() => 2; 0;");
    let names: Vec<_> = program.functions.iter().map(|f| f.name.clone()).collect();
    assert_eq!(names, vec!["a", "b"]);
}
