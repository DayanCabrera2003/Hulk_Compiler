//! Vector literal vs generator.

use super::*;

#[test]
fn vec_literal_basic() {
    let program = parse_ok("[1, 2, 3];");
    let ExprKind::VecLiteral(items) = &body(&program).kind else {
        panic!()
    };
    assert_eq!(items.len(), 3);
}

#[test]
fn empty_vec_literal() {
    let program = parse_ok("[];");
    let ExprKind::VecLiteral(items) = &body(&program).kind else {
        panic!()
    };
    assert!(items.is_empty());
}

#[test]
fn vec_generator() {
    let program = parse_ok("[x * 2 | x in range(0, 10)];");
    let ExprKind::VecGenerator {
        binding,
        element,
        iterable,
    } = &body(&program).kind
    else {
        panic!()
    };
    assert_eq!(binding, "x");
    assert!(matches!(element.kind, ExprKind::BinOp { .. }));
    assert!(matches!(iterable.kind, ExprKind::Call { .. }));
}

#[test]
fn vec_index_literal() {
    let program = parse_ok("[10, 20, 30][1];");
    let ExprKind::Index { target, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(target.kind, ExprKind::VecLiteral(_)));
}
