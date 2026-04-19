//! Programa mínimo: `print("Hello World");`.

use hulk_ast::ExprKind;

use crate::common::parse_ok;

const SRC: &str = include_str!("../../../../examples/hello.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("hello.hulk", SRC);
}

#[test]
fn body_is_call_to_print_with_string_arg() {
    let program = parse_ok("hello.hulk", SRC);
    let ExprKind::Call { callee, args } = &program.body.kind else {
        panic!("se esperaba Call como body, got {:?}", program.body.kind);
    };
    assert!(matches!(callee.kind, ExprKind::Ident(ref n) if n == "print"));
    assert_eq!(args.len(), 1);
    let ExprKind::StringLit(ref s) = args[0].kind else {
        panic!("el argumento debe ser un StringLit");
    };
    assert_eq!(s, "Hello World");
}

#[test]
fn program_has_no_declarations() {
    let program = parse_ok("hello.hulk", SRC);
    assert!(program.functions.is_empty());
    assert!(program.types.is_empty());
    assert!(program.protocols.is_empty());
    assert!(program.macros.is_empty());
}
