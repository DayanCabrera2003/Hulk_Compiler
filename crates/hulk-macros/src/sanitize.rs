use std::collections::HashMap;

use hulk_hir::visitor::{walk_assign_target_mut, walk_expr_mut};
use hulk_hir::{AssignTarget, Expr, ExprKind, Param, VisitorMut};

pub(crate) fn sanitize_locals(expr: &mut Expr, macro_name: &str, expansion_id: u64) {
    let mut sanitizer = LocalSanitizer {
        macro_name,
        expansion_id,
        scopes: Vec::new(),
    };
    sanitizer.visit_expr_mut(expr);
}

struct LocalSanitizer<'a> {
    macro_name: &'a str,
    expansion_id: u64,
    scopes: Vec<HashMap<String, String>>,
}

impl<'a> VisitorMut for LocalSanitizer<'a> {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Ident(name) => {
                if let Some(renamed) = self.lookup(name) {
                    *name = renamed;
                }
            }
            ExprKind::Block(exprs) => {
                self.scopes.push(HashMap::new());
                for item in exprs {
                    self.visit_expr_mut(item);
                }
                let _ = self.scopes.pop();
            }
            ExprKind::VecGenerator {
                element,
                binding,
                iterable,
            } => {
                self.visit_expr_mut(iterable);
                let fresh = self.fresh_name(binding);
                self.scopes
                    .push(HashMap::from([(binding.clone(), fresh.clone())]));
                *binding = fresh;
                self.visit_expr_mut(element);
                let _ = self.scopes.pop();
            }
            ExprKind::Let { bindings, body } => {
                let mut pushed = 0usize;
                for binding_expr in bindings {
                    if let ExprKind::LetBinding(binding) = &mut binding_expr.kind {
                        self.visit_expr_mut(&mut binding.value);
                        let fresh = self.fresh_name(&binding.name);
                        self.scopes
                            .push(HashMap::from([(binding.name.clone(), fresh.clone())]));
                        binding.name = fresh;
                        pushed = pushed.saturating_add(1);
                    } else {
                        self.visit_expr_mut(binding_expr);
                    }
                }

                self.visit_expr_mut(body);
                for _ in 0..pushed {
                    let _ = self.scopes.pop();
                }
            }
            ExprKind::For {
                binding,
                iterable,
                body,
            } => {
                self.visit_expr_mut(iterable);
                let fresh = self.fresh_name(binding);
                self.scopes
                    .push(HashMap::from([(binding.clone(), fresh.clone())]));
                *binding = fresh;
                self.visit_expr_mut(body);
                let _ = self.scopes.pop();
            }
            ExprKind::Lambda { params, body, .. } => {
                let mut frame = HashMap::new();
                for Param { name, .. } in params {
                    let fresh = self.fresh_name(name);
                    frame.insert(name.clone(), fresh.clone());
                    *name = fresh;
                }
                self.scopes.push(frame);
                self.visit_expr_mut(body);
                let _ = self.scopes.pop();
            }
            _ => walk_expr_mut(self, expr),
        }
    }

    fn visit_assign_target_mut(&mut self, target: &mut AssignTarget) {
        if let AssignTarget::Ident(name) = target {
            if let Some(renamed) = self.lookup(name) {
                *name = renamed;
            }
            return;
        }
        walk_assign_target_mut(self, target);
    }
}

impl<'a> LocalSanitizer<'a> {
    fn lookup(&self, name: &str) -> Option<String> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn fresh_name(&self, original: &str) -> String {
        format!(
            "__hulk_macro_{}_{}_{}",
            self.macro_name, self.expansion_id, original
        )
    }
}
