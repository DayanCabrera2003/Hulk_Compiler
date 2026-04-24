use std::collections::HashMap;

use hulk_diagnostics::{Diagnostic, DiagnosticBag};
use hulk_hir::visitor::walk_expr_mut;
use hulk_hir::{AssignTarget, Expr, ExprKind, SymbolId, TypeAnn, TypeId, VisitorMut};

#[derive(Clone)]
pub(crate) enum Substitution {
    Expr(Expr),
    Symbol(String),
    Placeholder { ident: String, symbol: SymbolId },
}

struct Substituter<'a> {
    substitutions: &'a HashMap<String, Substitution>,
    bag: &'a mut DiagnosticBag,
}

impl<'a> VisitorMut for Substituter<'a> {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        if let ExprKind::Ident(name) = &mut expr.kind {
            if let Some(sub) = self.substitutions.get(name).cloned() {
                match sub {
                    Substitution::Expr(value) => {
                        *expr = value;
                    }
                    Substitution::Symbol(symbol_name) => {
                        *name = symbol_name;
                    }
                    Substitution::Placeholder { ident, symbol } => {
                        let _ = symbol;
                        *name = ident;
                    }
                }
                return;
            }
        }

        if let ExprKind::AssignTarget(AssignTarget::Ident(name)) = &mut expr.kind {
            if let Some(sub) = self.substitutions.get(name) {
                match sub {
                    Substitution::Expr(_) => {
                        self.bag.push(
                            Diagnostic::error(
                                "un parametro de expresion no puede ser destino de asignacion",
                            )
                            .with_label(expr.span.clone(), "destino de asignacion invalido"),
                        );
                    }
                    Substitution::Symbol(symbol_name) => {
                        *name = symbol_name.clone();
                    }
                    Substitution::Placeholder { ident, symbol } => {
                        let _ = symbol;
                        *name = ident.clone();
                    }
                }
                return;
            }
        }

        walk_expr_mut(self, expr);
    }
}

pub(crate) fn substitute_params(
    expr: &mut Expr,
    substitutions: &HashMap<String, Substitution>,
    bag: &mut DiagnosticBag,
) {
    Substituter { substitutions, bag }.visit_expr_mut(expr);
}

pub(crate) fn map_type_ann_to_type_id(type_ann: &TypeAnn) -> TypeId {
    match type_ann {
        TypeAnn::Named(name) => match name.as_str() {
            "Number" => TypeId::NUMBER,
            "String" => TypeId::STRING,
            "Boolean" => TypeId::BOOLEAN,
            _ => TypeId::OBJECT,
        },
        TypeAnn::Iterable(_) | TypeAnn::Vector(_) | TypeAnn::Functor { .. } => TypeId::OBJECT,
    }
}
