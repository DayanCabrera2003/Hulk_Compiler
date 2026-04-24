use hulk_hir::visitor::{walk_assign_target, walk_expr};
use hulk_hir::{AssignTarget, Expr, ExprKind, NodeIdGen, Span, Visitor};

pub(crate) fn intrinsic_call(name: &str, args: Vec<Expr>, span: &Span, ids: &mut NodeIdGen) -> Expr {
    Expr::new(
        ExprKind::Call {
            callee: Box::new(ident(name, span, ids)),
            args,
        },
        span.clone(),
        ids.next_id(),
    )
}

pub(crate) fn ident(name: &str, span: &Span, ids: &mut NodeIdGen) -> Expr {
    Expr::new(ExprKind::Ident(name.to_owned()), span.clone(), ids.next_id())
}

pub(crate) fn number(value: f64, span: &Span, ids: &mut NodeIdGen) -> Expr {
    Expr::new(ExprKind::Number(value), span.clone(), ids.next_id())
}

pub(crate) fn string_lit(value: &str, span: &Span, ids: &mut NodeIdGen) -> Expr {
    Expr::new(
        ExprKind::StringLit(value.to_owned()),
        span.clone(),
        ids.next_id(),
    )
}

struct CollectIdentNodeIds<'a> {
    target: &'a str,
    out: &'a mut Vec<hulk_hir::NodeId>,
}

impl<'a> Visitor for CollectIdentNodeIds<'a> {
    fn visit_expr(&mut self, expr: &Expr) {
        if let ExprKind::Ident(name) = &expr.kind {
            if name == self.target {
                self.out.push(expr.id);
            }
        }
        walk_expr(self, expr);
    }

    fn visit_assign_target(&mut self, _target: &AssignTarget) {
        // Preserve original behavior: do not descend into AssignTarget.
    }
}

pub(crate) fn collect_ident_node_ids(expr: &Expr, target: &str, out: &mut Vec<hulk_hir::NodeId>) {
    CollectIdentNodeIds { target, out }.visit_expr(expr);
}

struct CollectIdentifiers<'a>(&'a mut Vec<String>);

impl<'a> Visitor for CollectIdentifiers<'a> {
    fn visit_expr(&mut self, expr: &Expr) {
        if let ExprKind::Ident(name) = &expr.kind {
            self.0.push(name.clone());
        }
        walk_expr(self, expr);
    }

    fn visit_assign_target(&mut self, target: &AssignTarget) {
        if let AssignTarget::Ident(name) = target {
            self.0.push(name.clone());
        }
        walk_assign_target(self, target);
    }
}

pub(crate) fn collect_identifiers(expr: &Expr, out: &mut Vec<String>) {
    CollectIdentifiers(out).visit_expr(expr);
}
