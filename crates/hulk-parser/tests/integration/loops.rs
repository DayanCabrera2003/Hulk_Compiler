//! while + for, incluyendo for sobre range y sobre literal de vector.

use hulk_ast::ExprKind;

use crate::common::{contains_expr, count_exprs, parse_ok};

const SRC: &str = include_str!("../../../../examples/loops.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("loops.hulk", SRC);
}

#[test]
fn body_is_block_with_while_and_two_fors() {
    let program = parse_ok("loops.hulk", SRC);
    let ExprKind::Block(exprs) = &program.body.kind else {
        panic!();
    };
    assert_eq!(exprs.len(), 3);
    assert!(matches!(exprs[0].kind, ExprKind::Let { .. })); // let a=10 in while…
    assert!(matches!(exprs[1].kind, ExprKind::For { .. }));
    assert!(matches!(exprs[2].kind, ExprKind::For { .. }));
}

#[test]
fn while_body_is_block_with_destructive_assignment() {
    let program = parse_ok("loops.hulk", SRC);
    let has_while_with_block = contains_expr(&program, |k| {
        matches!(
            k,
            ExprKind::While { body, .. } if matches!(body.kind, ExprKind::Block(_))
        )
    });
    assert!(has_while_with_block);

    let has_assign = contains_expr(&program, |k| matches!(k, ExprKind::Assign { .. }));
    assert!(has_assign);
}

#[test]
fn for_iterates_over_range_call() {
    let program = parse_ok("loops.hulk", SRC);
    let has_range_for = contains_expr(&program, |k| {
        let ExprKind::For {
            iterable, binding, ..
        } = k
        else {
            return false;
        };
        binding == "x"
            && matches!(&iterable.kind, ExprKind::Call { callee, .. }
                if matches!(callee.kind, ExprKind::Ident(ref n) if n == "range"))
    });
    assert!(has_range_for);
}

#[test]
fn for_iterates_over_vec_literal() {
    let program = parse_ok("loops.hulk", SRC);
    let has_vec_for = contains_expr(&program, |k| {
        matches!(
            k,
            ExprKind::For { iterable, .. } if matches!(iterable.kind, ExprKind::VecLiteral(_))
        )
    });
    assert!(has_vec_for);
}

#[test]
fn total_loop_expressions() {
    let program = parse_ok("loops.hulk", SRC);
    let whiles = count_exprs(&program, |k| matches!(k, ExprKind::While { .. }));
    let fors = count_exprs(&program, |k| matches!(k, ExprKind::For { .. }));
    assert_eq!(whiles, 1);
    assert_eq!(fors, 2);
}
