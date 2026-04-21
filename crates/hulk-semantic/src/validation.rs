use std::collections::HashSet;

use hulk_ast::{Expr, ExprKind, FunctionDecl, SourceFile, Span, TypeAnn};
use hulk_diagnostics::Diagnostic;

use crate::{Resolver, SymbolId, SymbolKind};

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

    pub(crate) fn is_protocol_symbol(&self, symbol_id: SymbolId) -> bool {
        self.table
            .get(symbol_id)
            .is_some_and(|symbol| matches!(symbol.kind, SymbolKind::Protocol))
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

    pub(crate) fn type_conforms_protocol(&self, type_id: SymbolId, protocol_id: SymbolId) -> bool {
        let required = self.collect_protocol_methods(protocol_id);
        required
            .iter()
            .all(|method_name| self.type_has_method(type_id, method_name))
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

    pub(crate) fn synthetic_span(&self) -> Span {
        Span::dummy(std::sync::Arc::new(SourceFile::new("<synthetic>", "")))
    }
}
