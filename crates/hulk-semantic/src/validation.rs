use hulk_ast::{Expr, ExprKind, FunctionDecl, TypeAnn};
use hulk_diagnostics::Diagnostic;

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
}
