//! Parser tests for iterables.hulk.
//!
//! The Iterable protocol and Range type are now defined in prelude/prelude.hulk
//! (see prelude.rs tests). This file only tests what iterables.hulk still contains.

use hulk_ast::ExprKind;

use crate::common::parse_ok;

const SRC: &str = include_str!("../../../../examples/iterables.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("iterables.hulk", SRC);
}

#[test]
fn body_is_for_loop_over_new_range() {
    let program = parse_ok("iterables.hulk", SRC);
    let ExprKind::For { iterable, .. } = &program.body.kind else {
        panic!("body debe ser For");
    };
    assert!(matches!(iterable.kind, ExprKind::New { .. }));
}
