//! Funciones inline, full-form, recursivas y mutuamente recursivas.

use hulk_ast::ExprKind;

use crate::common::parse_ok;

const SRC: &str = include_str!("../../../../examples/functions.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("functions.hulk", SRC);
}

#[test]
fn program_declares_four_functions_in_order() {
    let program = parse_ok("functions.hulk", SRC);
    let names: Vec<_> = program.functions.iter().map(|f| f.name.clone()).collect();
    assert_eq!(names, vec!["cot", "tan", "fib", "operate"]);
}

#[test]
fn inline_functions_have_non_block_body() {
    let program = parse_ok("functions.hulk", SRC);
    for name in ["cot", "tan", "fib"] {
        let f = program
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no se encontró la función {name}"));
        assert!(
            !matches!(f.body.kind, ExprKind::Block(_)),
            "{name} debe ser inline"
        );
    }
}

#[test]
fn operate_has_block_body_with_four_prints() {
    let program = parse_ok("functions.hulk", SRC);
    let op = program
        .functions
        .iter()
        .find(|f| f.name == "operate")
        .expect("operate no declarada");
    let ExprKind::Block(exprs) = &op.body.kind else {
        panic!("operate debe tener body Block");
    };
    assert_eq!(exprs.len(), 4);
    for e in exprs {
        let ExprKind::Call { callee, .. } = &e.kind else {
            panic!("cada expr del block debe ser Call");
        };
        assert!(matches!(callee.kind, ExprKind::Ident(ref n) if n == "print"));
    }
}

#[test]
fn fib_body_is_a_conditional_recursion() {
    let program = parse_ok("functions.hulk", SRC);
    let fib = program
        .functions
        .iter()
        .find(|f| f.name == "fib")
        .expect("fib no declarada");
    assert!(matches!(fib.body.kind, ExprKind::If { .. }));
}

#[test]
fn program_body_uses_declared_functions() {
    let program = parse_ok("functions.hulk", SRC);
    let ExprKind::Block(exprs) = &program.body.kind else {
        panic!();
    };
    // El último elemento del bloque es la llamada a `operate(10, 5)`.
    let last = exprs.last().expect("bloque vacío");
    let ExprKind::Call { callee, args } = &last.kind else {
        panic!("se esperaba Call");
    };
    assert!(matches!(callee.kind, ExprKind::Ident(ref n) if n == "operate"));
    assert_eq!(args.len(), 2);
}

#[test]
fn mutual_recursion_is_allowed_even_when_declared_before_callee_inline() {
    // cot llama a tan y tan se declara DESPUÉS de cot.
    let program = parse_ok(
        "mutual.hulk",
        "function cot(x) => 1 / tan(x); function tan(x) => 1 / cot(x); 0;",
    );
    assert_eq!(program.functions.len(), 2);
}
