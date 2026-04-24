use std::collections::HashSet;

use hulk_ast::{Expr, ProtocolDecl, Span, TypeAnn};
use hulk_diagnostics::Diagnostic;

use crate::symbols::{SymbolId, SymbolKind};
use crate::Resolver;

impl Resolver {
    pub(crate) fn register_protocol_details(&mut self, protocols: &[ProtocolDecl]) {
        for protocol in protocols {
            let Some(protocol_id) = self.lookup(&protocol.name) else {
                continue;
            };

            let methods = protocol
                .methods
                .iter()
                .map(|method| method.name.clone())
                .collect::<HashSet<_>>();
            self.protocol_methods.insert(protocol_id, methods);

            let mut extends = Vec::new();
            for parent_name in &protocol.extends {
                if let Some(parent_id) = self.lookup(parent_name) {
                    if self.is_protocol_symbol(parent_id) {
                        extends.push(parent_id);
                    }
                }
            }
            self.protocol_extends.insert(protocol_id, extends);
        }
    }

    pub(crate) fn collect_protocol_methods(&self, protocol_id: SymbolId) -> HashSet<String> {
        let mut methods = HashSet::new();
        let mut stack = vec![protocol_id];
        let mut seen = HashSet::new();

        while let Some(current) = stack.pop() {
            if !seen.insert(current) {
                continue;
            }

            if let Some(current_methods) = self.protocol_methods.get(&current) {
                methods.extend(current_methods.iter().cloned());
            }

            if let Some(parents) = self.protocol_extends.get(&current) {
                stack.extend(parents.iter().copied());
            }
        }

        methods
    }

    pub(crate) fn type_conforms_protocol(&self, type_id: SymbolId, protocol_id: SymbolId) -> bool {
        let required = self.collect_protocol_methods(protocol_id);
        required
            .iter()
            .all(|method_name| self.type_has_method(type_id, method_name))
    }

    pub(crate) fn is_protocol_symbol(&self, symbol_id: SymbolId) -> bool {
        self.table
            .get(symbol_id)
            .is_some_and(|symbol| matches!(symbol.kind, SymbolKind::Protocol))
    }

    pub(crate) fn validate_call_argument_protocol_conformance(
        &mut self,
        callee_symbol: SymbolId,
        args: &[Expr],
        span: Span,
    ) {
        let Some(param_annotations) = self.function_param_annotations.get(&callee_symbol).cloned()
        else {
            return;
        };

        for (arg, annotation) in args.iter().zip(param_annotations.iter()) {
            let Some(TypeAnn::Named(type_name)) = annotation else {
                continue;
            };

            let Some(protocol_id) = self.lookup(type_name) else {
                continue;
            };

            if !self.is_protocol_symbol(protocol_id) {
                continue;
            }

            let Some(concrete_type) = self.resolve_concrete_type_symbol(arg) else {
                continue;
            };

            if !self.type_conforms_protocol(concrete_type, protocol_id) {
                self.bag.push(
                    Diagnostic::error(format!(
                        "tipo no conforma al protocolo requerido: {type_name}"
                    ))
                    .with_label(
                        span.clone(),
                        "el argumento no implementa la interfaz esperada",
                    ),
                );
            }
        }
    }
}
