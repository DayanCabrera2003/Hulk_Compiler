use hulk_ast::{MacroParam, Span, TypeAnn};
use hulk_diagnostics::Diagnostic;

use crate::symbols::{SymbolId, SymbolKind};
use crate::Resolver;

impl Resolver {
    pub(crate) fn resolve_type_ann_option(&mut self, type_ann: &Option<TypeAnn>) {
        if let Some(type_ann) = type_ann {
            self.resolve_type_ann(type_ann, self.synthetic_span());
        }
    }

    pub(crate) fn resolve_macro_param_type(&mut self, param: &MacroParam) {
        self.resolve_type_ann(param.type_ann(), self.synthetic_span());
    }

    pub(crate) fn resolve_type_ann(&mut self, type_ann: &TypeAnn, span: Span) {
        match type_ann {
            TypeAnn::Named(name) => self.resolve_type_name(name, span),
            TypeAnn::Iterable(inner) | TypeAnn::Vector(inner) => self.resolve_type_ann(inner, span),
            TypeAnn::Functor { params, ret } => {
                for param in params {
                    self.resolve_type_ann(param, span.clone());
                }
                self.resolve_type_ann(ret, span);
            }
        }
    }

    pub(crate) fn resolve_type_name(&mut self, name: &str, span: Span) {
        match self.lookup(name) {
            Some(symbol_id) => {
                self.validate_type_symbol(name, symbol_id, span);
            }
            None => {
                self.bag.push(
                    Diagnostic::error(format!("tipo no existe: {name}"))
                        .with_label(span, "no hay un tipo visible con ese nombre"),
                );
            }
        }
    }

    pub(crate) fn validate_symbol_use(&mut self, name: &str, symbol_id: SymbolId, span: Span) {
        if matches!(
            name,
            "print"
                | "sqrt"
                | "sin"
                | "cos"
                | "exp"
                | "log"
                | "rand"
                | "range"
                | "PI"
                | "E"
                | "Object"
                | "Number"
                | "String"
                | "Boolean"
        ) {
            return;
        }

        if let Some(symbol) = self.table.get(symbol_id) {
            if matches!(symbol.kind, SymbolKind::BuiltinType | SymbolKind::Type) {
                self.bag.push(
                    Diagnostic::error(format!("{name} no es una variable"))
                        .with_label(span, "el identificador resuelto no es asignable"),
                );
            }
        }
    }

    pub(crate) fn validate_type_symbol(&mut self, name: &str, symbol_id: SymbolId, span: Span) {
        if let Some(symbol) = self.table.get(symbol_id) {
            if !matches!(
                symbol.kind,
                SymbolKind::Type | SymbolKind::BuiltinType | SymbolKind::Protocol
            ) {
                self.bag.push(
                    Diagnostic::error(format!("tipo no existe: {name}"))
                        .with_label(span, "el nombre visible no corresponde a un tipo"),
                );
            }
        }
    }
}
