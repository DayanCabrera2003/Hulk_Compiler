//! let con múltiples bindings, redefinición anidada y asignación destructiva.

use hulk_ast::ExprKind;

use crate::common::{contains_expr, count_exprs, parse_ok};

const SRC: &str = include_str!("../../../../examples/let_scoping.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("let_scoping.hulk", SRC);
}

#[test]
fn body_is_block_with_four_lets() {
    let program = parse_ok("let_scoping.hulk", SRC);
    let ExprKind::Block(exprs) = &program.body.kind else {
        panic!("body debe ser Block");
    };
    assert_eq!(exprs.len(), 4);
    for e in exprs {
        assert!(matches!(e.kind, ExprKind::Let { .. }));
    }
}

#[test]
fn at_least_one_let_has_multiple_bindings() {
    let program = parse_ok("let_scoping.hulk", SRC);
    let has_multi = contains_expr(&program, |k| {
        matches!(
            k,
            ExprKind::Let { bindings, .. } if bindings.len() >= 2
        )
    });
    assert!(has_multi);
}

#[test]
fn nested_let_redefinition_is_preserved() {
    // `let a = 20 in { let a = 42 in print(a); print(a); };`
    // El let externo tiene un block como body; dentro, un let anidado.
    let program = parse_ok("let_scoping.hulk", SRC);
    let nested_count = count_exprs(&program, |k| {
        matches!(
            k,
            ExprKind::Let { body, .. } if matches!(body.kind, ExprKind::Block(_))
        )
    });
    assert!(nested_count >= 2, "esperaba >=2 lets con body de bloque");
}

#[test]
fn contains_destructive_assignment() {
    let program = parse_ok("let_scoping.hulk", SRC);
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::Assign { .. }
    )));
}

#[test]
fn total_let_expressions_matches_expected() {
    let program = parse_ok("let_scoping.hulk", SRC);
    let lets = count_exprs(&program, |k| matches!(k, ExprKind::Let { .. }));
    // 4 en el bloque + 1 let anidado dentro del tercer let.
    assert!(lets >= 5, "esperaba >=5 lets, obtuvo {lets}");
}

#[test]
fn right_associative_nested_let_inline() {
    // `let a = 1 in let b = 2 in a + b;`
    let program = parse_ok("rassoc.hulk", "let a = 1 in let b = 2 in a + b;");
    let ExprKind::Let { body, .. } = &program.body.kind else {
        panic!();
    };
    assert!(matches!(body.kind, ExprKind::Let { .. }));
}
