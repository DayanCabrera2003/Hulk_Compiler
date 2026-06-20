use hulk_ast::{Expr, ExprKind, FunctionDecl, TypeAnn};
use hulk_diagnostics::Diagnostic;

use crate::symbols::SymbolId;
use crate::Resolver;

impl Resolver {
    pub(crate) fn report_ambiguous_function_inference(&mut self, function: &FunctionDecl) {
        let ExprKind::Ident(ref name) = function.body.kind else {
            return;
        };

        let is_untyped_param = function
            .params
            .iter()
            .any(|param| param.name == *name && param.type_ann.is_none());

        if is_untyped_param {
            self.bag.push(
                Diagnostic::error("tipo no inferible, añade anotación").with_label(
                    function.span.clone(),
                    "la función no aporta restricciones de tipo suficientes",
                ),
            );
        }
    }

    pub(crate) fn validate_expr_against_annotation(
        &mut self,
        expr: &Expr,
        annotation: Option<&TypeAnn>,
    ) {
        let Some(TypeAnn::Named(expected)) = annotation else {
            return;
        };

        // Only validate literal types that can be determined at resolve time.
        let actual = match &expr.kind {
            ExprKind::Number(_) => Some("Number"),
            ExprKind::StringLit(_) => Some("String"),
            ExprKind::Bool(_) => Some("Boolean"),
            _ => None,
        };

        if let Some(actual_name) = actual {
            if actual_name != expected {
                self.bag.push(
                    Diagnostic::error("tipo inferido incompatible con anotación").with_label(
                        expr.span.clone(),
                        format!("se esperaba {expected}, pero se infirió {actual_name}"),
                    ),
                );
            }
        }
    }

    pub(crate) fn validate_method_call(&mut self, receiver: &Expr, call_expr: &Expr) {
        let ExprKind::MethodCall { method, .. } = &call_expr.kind else {
            return;
        };

        let Some(type_id) = self.resolve_concrete_type_symbol(receiver) else {
            return;
        };

        if !self.type_has_method(type_id, method) {
            self.bag.push(
                Diagnostic::error(format!("método no existe: {method}"))
                    .with_label(call_expr.span.clone(), "el receptor no define ese método"),
            );
        }
    }

    /// Validates a field access `receiver.field`. The field must name an
    /// attribute reachable from the enclosing type — one it declares itself or
    /// inherits from an ancestor. Naming an attribute that exists nowhere in
    /// that hierarchy is a typo and is rejected. The "current type" is the
    /// lexical type whose method body performs the access.
    pub(crate) fn validate_field_access(&mut self, field: &str, access_expr: &Expr) {
        // Field access outside any type still resolves its receiver, whose
        // own diagnostic (e.g. `self` outside a method) already fires; there
        // are no attributes in scope here, so add nothing further.
        let Some(current_type) = self.current_type else {
            return;
        };

        if self.type_owns_attribute(current_type, field)
            || self.ancestor_owns_attribute(current_type, field)
        {
            return;
        }

        self.bag.push(
            Diagnostic::error(format!("atributo no existe: {field}")).with_label(
                access_expr.span.clone(),
                "no es un atributo del tipo ni de sus ancestros",
            ),
        );
    }

    /// Returns true when `type_id` declares `field` as one of its own
    /// attributes (inherited attributes do not count).
    fn type_owns_attribute(&self, type_id: SymbolId, field: &str) -> bool {
        self.type_attributes
            .get(&type_id)
            .is_some_and(|attributes| attributes.contains(field))
    }

    /// Returns true when any strict ancestor of `type_id` declares `field` as
    /// an own attribute — i.e. `field` is reachable through inheritance.
    fn ancestor_owns_attribute(&self, type_id: SymbolId, field: &str) -> bool {
        let mut cursor = self.type_parents.get(&type_id).and_then(|parent| *parent);
        while let Some(ancestor) = cursor {
            if self.type_owns_attribute(ancestor, field) {
                return true;
            }
            cursor = self.type_parents.get(&ancestor).and_then(|parent| *parent);
        }
        false
    }
}
