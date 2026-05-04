//! Robustness: deep nesting, multibyte strings, edge cases.

use super::*;

#[test]
fn deeply_nested_binop_does_not_blow_node_ids() {
    let mut gen = NodeIdGen::new();
    let mut current = Expr::new(ExprKind::Number(0.0), fresh_span(), gen.next_id());
    for _ in 0..500 {
        let next = Expr::new(ExprKind::Number(1.0), fresh_span(), gen.next_id());
        current = Expr::new(
            ExprKind::BinOp {
                op: BinOpKind::Add,
                left: Box::new(current),
                right: Box::new(next),
            },
            fresh_span(),
            gen.next_id(),
        );
    }

    // Count depth via a visitor to make sure walking does not stack-overflow
    // for modest nesting.
    struct DepthCounter(usize);
    impl Visitor for DepthCounter {
        fn visit_expr(&mut self, e: &Expr) {
            self.0 += 1;
            hulk_ast::visitor::walk_expr(self, e);
        }
    }
    let mut dc = DepthCounter(0);
    dc.visit_expr(&current);
    // 500 BinOps + 500 right-hand numbers + 1 leftmost = 1001
    assert_eq!(dc.0, 1001);
}

#[test]
fn string_literal_preserves_multibyte_content() {
    // The AST must be able to hold any UTF-8 string produced by the lexer.
    let kind = ExprKind::StringLit("héllo 🦀 ñ".to_owned());
    let e = expr(kind, 0);
    if let ExprKind::StringLit(s) = e.kind {
        assert_eq!(s, "héllo 🦀 ñ");
    } else {
        panic!();
    }
}

#[test]
fn identifier_preserves_ascii_and_digits() {
    // HULK identifiers are ASCII alphanumeric + underscore (not leading).
    let e = expr(ExprKind::Ident("x_0_TitleCase".to_owned()), 0);
    if let ExprKind::Ident(n) = e.kind {
        assert_eq!(n, "x_0_TitleCase");
    } else {
        panic!();
    }
}

#[test]
fn clone_produces_structurally_equal_program() {
    let program = build_kitchen_sink_program();
    let cloned = program.clone();
    assert_eq!(program, cloned);
}

#[test]
fn empty_program_is_representable() {
    let p = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![],
        body: num(0.0, 0),
    };
    assert!(p.functions.is_empty());
    assert!(p.types.is_empty());
    assert!(p.protocols.is_empty());
    assert!(p.macros.is_empty());
}

#[test]
fn node_ids_in_kitchen_sink_are_all_distinct() {
    let program = build_kitchen_sink_program();

    #[derive(Default)]
    struct IdCollector(Vec<NodeId>);
    impl Visitor for IdCollector {
        fn visit_expr(&mut self, e: &Expr) {
            self.0.push(e.id);
            hulk_ast::visitor::walk_expr(self, e);
        }
    }

    let mut ids = IdCollector::default();
    ids.visit_program(&program);

    let unique: std::collections::HashSet<_> = ids.0.iter().copied().collect();
    assert_eq!(
        ids.0.len(),
        unique.len(),
        "duplicate NodeIds in kitchen sink program"
    );
}
