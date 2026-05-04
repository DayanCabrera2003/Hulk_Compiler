// Tests for resolver extensions: happy-path behaviors that produce no
// diagnostics but populate internal resolver state (e.g. expr_symbols).

use hulk_diagnostics::DiagnosticBag;
use hulk_driver::build_hir;
use hulk_hir::{AssignTarget, Expr, ExprKind, Hir, SourceFile};

fn build_source(name: &str, source: &str) -> (Option<Hir>, DiagnosticBag) {
    let source = SourceFile::new(name, source);
    let mut bag = DiagnosticBag::new();
    let hir = build_hir(source, &mut bag);
    (hir, bag)
}

// Walks the expression tree looking for the first ExprKind::AssignTarget whose
// inner variant is AssignTarget::Ident, and returns the NodeId of that node.
fn find_assign_target_ident(expr: &Expr) -> Option<hulk_hir::NodeId> {
    match &expr.kind {
        ExprKind::AssignTarget(AssignTarget::Ident(_)) => Some(expr.id),
        ExprKind::Let { bindings, body } => bindings
            .iter()
            .find_map(|b| find_assign_target_ident(b))
            .or_else(|| find_assign_target_ident(body)),
        ExprKind::Assign { target, value } => find_assign_target_ident(target)
            .or_else(|| find_assign_target_ident(value)),
        ExprKind::LetBinding(lb) => find_assign_target_ident(&lb.value),
        ExprKind::Block(exprs) => exprs.iter().find_map(find_assign_target_ident),
        ExprKind::BinOp { left, right, .. } => find_assign_target_ident(left)
            .or_else(|| find_assign_target_ident(right)),
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => find_assign_target_ident(condition)
            .or_else(|| find_assign_target_ident(then_branch))
            .or_else(|| {
                elif_branches
                    .iter()
                    .find_map(|(c, b)| find_assign_target_ident(c).or_else(|| find_assign_target_ident(b)))
            })
            .or_else(|| {
                else_branch
                    .as_deref()
                    .and_then(find_assign_target_ident)
            }),
        _ => None,
    }
}

#[test]
fn assign_target_ident_has_resolved_symbol() {
    // A let-binding introduces `x` into scope; the assignment `x := 2` then
    // uses it as an AssignTarget.  The resolver must record the symbol in
    // expr_symbols for that AssignTarget node so that later passes (e.g. the
    // lowerer) can look up which variable is being mutated.
    let (hir, bag) = build_source("assign_target_sym", "let x = 1 in x := 2;");

    assert!(
        !bag.has_errors(),
        "unexpected errors: {:?}",
        bag.diagnostics()
    );
    let hir = hir.expect("should produce a HIR for a valid assignment expression");

    let node_id = find_assign_target_ident(&hir.program.body).expect(
        "the program body should contain an AssignTarget::Ident node for `x`",
    );

    assert!(
        hir.resolved_symbol(node_id).is_some(),
        "AssignTarget::Ident(`x`) node (id={node_id:?}) should have a resolved symbol \
         in expr_symbols, but resolved_symbol returned None",
    );
}
