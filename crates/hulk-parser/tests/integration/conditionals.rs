//! if / elif / else con ramas múltiples y bloques como cuerpo.

use hulk_ast::ExprKind;

use crate::common::{contains_expr, count_exprs, parse_ok};

const SRC: &str = include_str!("../../../../examples/conditionals.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("conditionals.hulk", SRC);
}

#[test]
fn body_is_block_with_four_statements() {
    let program = parse_ok("conditionals.hulk", SRC);
    let ExprKind::Block(exprs) = &program.body.kind else {
        panic!();
    };
    assert_eq!(exprs.len(), 4);
}

#[test]
fn at_least_one_if_has_elif_chain() {
    let program = parse_ok("conditionals.hulk", SRC);
    let has_elif = contains_expr(&program, |k| {
        matches!(
            k,
            ExprKind::If { elif_branches, .. } if !elif_branches.is_empty()
        )
    });
    assert!(has_elif);
}

#[test]
fn at_least_one_if_without_else() {
    let program = parse_ok("conditionals.hulk", SRC);
    let has_bare_if = contains_expr(&program, |k| {
        matches!(
            k,
            ExprKind::If {
                else_branch: None,
                ..
            }
        )
    });
    assert!(has_bare_if);
}

#[test]
fn at_least_one_if_with_block_branches() {
    let program = parse_ok("conditionals.hulk", SRC);
    let has_block_branch = contains_expr(&program, |k| {
        matches!(
            k,
            ExprKind::If { then_branch, .. } if matches!(then_branch.kind, ExprKind::Block(_))
        )
    });
    assert!(has_block_branch);
}

#[test]
fn total_if_expressions() {
    let program = parse_ok("conditionals.hulk", SRC);
    let ifs = count_exprs(&program, |k| matches!(k, ExprKind::If { .. }));
    assert!(ifs >= 4, "esperaba >=4 ifs, obtuvo {ifs}");
}

#[test]
fn elif_nested_chain_on_inline_source() {
    let program = parse_ok(
        "elif.hulk",
        r#"if (a) 1 elif (b) 2 elif (c) 3 elif (d) 4 else 5;"#,
    );
    let ExprKind::If {
        elif_branches,
        else_branch,
        ..
    } = &program.body.kind
    else {
        panic!();
    };
    assert_eq!(elif_branches.len(), 3);
    assert!(else_branch.is_some());
}
