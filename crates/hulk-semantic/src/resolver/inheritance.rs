use std::collections::HashSet;

use hulk_ast::{Expr, ExprKind, ParentSpec, TypeAnn};
use hulk_diagnostics::Diagnostic;

use crate::symbols::{SymbolId, SymbolKind};
use crate::Resolver;

impl Resolver {
    pub(crate) fn resolve_parent_spec(&mut self, parent: Option<&ParentSpec>) -> Option<SymbolId> {
        let parent = parent?;
        self.resolve_type_name(&parent.name, parent.span.clone());
        for arg in &parent.args {
            self.resolve_expr(arg);
        }
        let parent_id = self.lookup(&parent.name);
        if let Some(parent_symbol) = parent_id.and_then(|id| self.table.get(id)) {
            if matches!(parent_symbol.kind, SymbolKind::BuiltinType)
                && matches!(parent_symbol.name.as_str(), "Number" | "String" | "Boolean")
            {
                self.bag.push(
                    Diagnostic::error(format!("no se puede heredar de {}", parent_symbol.name))
                        .with_label(
                            parent.span.clone(),
                            "los tipos primitivos no son heredables",
                        ),
                );
            }
        }
        parent_id
    }

    pub(crate) fn detect_inheritance_cycles(&mut self) {
        let type_ids = self.type_parents.keys().copied().collect::<Vec<_>>();
        let mut nodes_in_reported_cycles = HashSet::new();

        for root in type_ids {
            if nodes_in_reported_cycles.contains(&root) {
                continue;
            }

            let mut path = vec![];
            let mut cursor = Some(root);

            while let Some(current) = cursor {
                if let Some(cycle_start_idx) = path.iter().position(|&n| n == current) {
                    for &node_in_cycle in &path[cycle_start_idx..] {
                        nodes_in_reported_cycles.insert(node_in_cycle);
                    }

                    let type_name = self
                        .table
                        .get(root)
                        .map(|symbol| symbol.name.clone())
                        .unwrap_or_else(|| "<tipo>".to_owned());
                    let span = self
                        .table
                        .get(root)
                        .map(|symbol| symbol.span.clone())
                        .unwrap_or_else(|| self.synthetic_span());
                    self.bag
                        .push(Diagnostic::error("ciclos en herencia").with_label(
                            span,
                            format!("se detectó un ciclo que involucra a {type_name}"),
                        ));
                    break;
                }

                path.push(current);
                cursor = self.type_parents.get(&current).and_then(|parent| *parent);
            }
        }
    }

    /// Returns true if the type named `type_name` (or any of its ancestors)
    /// declares a method called `method_name`. Convenience wrapper around
    /// `type_has_method` for callers that only know the type name string.
    #[must_use]
    pub fn type_with_name_has_method(&self, type_name: &str, method_name: &str) -> bool {
        let Some(symbol_id) = self.lookup(type_name) else {
            return false;
        };
        self.type_has_method(symbol_id, method_name)
    }

    pub(crate) fn type_has_method(&self, type_id: SymbolId, method_name: &str) -> bool {
        if self
            .type_methods
            .get(&type_id)
            .is_some_and(|methods| methods.contains_key(method_name))
        {
            return true;
        }

        if let Some(parent) = self.type_parents.get(&type_id).and_then(|parent| *parent) {
            return self.type_has_method(parent, method_name);
        }

        false
    }

    pub(crate) fn resolve_concrete_type_symbol(&self, expr: &Expr) -> Option<SymbolId> {
        match &expr.kind {
            ExprKind::New {
                type_ann: TypeAnn::Named(name),
                ..
            } => self.lookup(name),
            ExprKind::Self_ => self.current_type,
            _ => None,
        }
    }
}
