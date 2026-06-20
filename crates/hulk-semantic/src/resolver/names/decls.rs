use hulk_ast::{FunctionDecl, MacroDecl, MemberKind, Param, Program, Span, TypeAnn, TypeDecl};
use hulk_diagnostics::Diagnostic;

use crate::resolver::param_span;
use crate::symbols::SymbolKind;
use crate::Resolver;

impl Resolver {
    /// Resolves all names in a program and records expression references.
    pub fn resolve_program(&mut self, program: &Program) {
        self.expr_symbols.clear();
        self.type_parents.clear();
        self.type_methods.clear();
        self.type_attributes.clear();
        self.protocol_methods.clear();
        self.protocol_extends.clear();
        self.function_param_annotations.clear();
        self.register_global_declarations(program);
        self.register_protocol_details(&program.protocols);

        // Resolve type declarations before function bodies so that any method
        // signature is registered in `type_methods` by the time a function
        // body validates calls like `(new T()).method()`. Otherwise the
        // function body sees an empty method table for `T` and the validator
        // reports a false "método no existe".
        for type_decl in &program.types {
            self.resolve_type_decl(type_decl);
        }

        for function in &program.functions {
            self.resolve_function_decl(function);
        }

        for macro_decl in &program.macros {
            self.resolve_macro_decl(macro_decl);
        }

        self.resolve_expr(&program.body);
        self.detect_inheritance_cycles();
    }

    pub(crate) fn register_global_declarations(&mut self, program: &Program) {
        for function in &program.functions {
            let function_id = self.define(
                function.name.clone(),
                SymbolKind::Function,
                function.span.clone(),
            );
            let param_annotations = function
                .params
                .iter()
                .map(|param| param.type_ann.clone())
                .collect();
            self.function_param_annotations
                .insert(function_id, param_annotations);
        }

        for type_decl in &program.types {
            let type_id = self.define(
                type_decl.name.clone(),
                SymbolKind::Type,
                type_decl.span.clone(),
            );
            self.type_parents.insert(type_id, None);
            // Record this type's own attribute names up front, before any
            // method body is resolved, so field-access validation can run
            // regardless of member declaration order or cross-type references.
            let own_attributes = type_decl
                .members
                .iter()
                .filter_map(|member| match &member.kind {
                    MemberKind::Attribute { name, .. } => Some(name.clone()),
                    MemberKind::Method(_) => None,
                })
                .collect();
            self.type_attributes.insert(type_id, own_attributes);
        }

        for protocol in &program.protocols {
            self.define(
                protocol.name.clone(),
                SymbolKind::Protocol,
                protocol.span.clone(),
            );
        }

        for macro_decl in &program.macros {
            self.define(
                macro_decl.name.clone(),
                SymbolKind::Macro,
                macro_decl.span.clone(),
            );
        }
    }

    pub(crate) fn resolve_function_decl(&mut self, function: &FunctionDecl) {
        self.push_scope();
        self.resolve_type_ann_option(&function.return_type);
        let param_ids = self.define_params(&function.params);
        // Look up the function's own symbol (registered earlier in
        // register_global_declarations) and link the param symbol ids to it.
        // This lets the type inferer register param types from declared
        // annotations before walking the body.
        if let Some(fn_id) = self.lookup(&function.name) {
            self.function_param_symbols.insert(fn_id, param_ids);
        }
        self.resolve_expr(&function.body);
        self.validate_expr_against_annotation(&function.body, function.return_type.as_ref());
        self.report_ambiguous_function_inference(function);
        self.pop_scope();
    }

    pub(crate) fn resolve_type_decl(&mut self, type_decl: &TypeDecl) {
        let previous_type = self.current_type;
        self.current_type = self.lookup(&type_decl.name);

        self.push_scope();
        let mut ctor_param_ids = Vec::with_capacity(type_decl.params.len());
        for param in &type_decl.params {
            self.resolve_type_ann_option(&param.type_ann);
            let pid = self.define(
                param.name.clone(),
                SymbolKind::Parameter,
                param.span.clone(),
            );
            ctor_param_ids.push(pid);
        }
        // Record the constructor's param symbols + annotations keyed by the
        // type's own symbol so the type inferer can register their declared
        // types before walking attribute initialisers (which may reference
        // these params, e.g. `val = start` with `start: Number`).
        if let Some(type_id) = self.current_type {
            let anns: Vec<Option<TypeAnn>> = type_decl
                .params
                .iter()
                .map(|p| p.type_ann.clone())
                .collect();
            self.function_param_annotations.insert(type_id, anns);
            self.function_param_symbols.insert(type_id, ctor_param_ids);
        }

        self.push_scope();

        if let Some(current_type) = self.current_type {
            let parent_id = self.resolve_parent_spec(type_decl.parent.as_ref());
            self.type_parents.insert(current_type, parent_id);
        }

        for member in &type_decl.members {
            self.resolve_member(member);
        }

        self.pop_scope();
        self.pop_scope();
        self.current_type = previous_type;
    }

    pub(crate) fn resolve_member(&mut self, member: &hulk_ast::decl::Member) {
        match &member.kind {
            MemberKind::Attribute {
                type_ann, value, ..
            } => {
                self.resolve_type_ann_option(type_ann);
                self.resolve_expr(value);
            }
            MemberKind::Method(method) => self.resolve_method_decl(method, &member.span),
        }
    }

    pub(crate) fn resolve_method_decl(&mut self, method: &FunctionDecl, span: &Span) {
        self.resolve_type_ann_option(&method.return_type);
        if let Some(current_type) = self.current_type {
            let method_id = self.define(
                method.name.clone(),
                SymbolKind::Function,
                method.span.clone(),
            );
            self.type_methods
                .entry(current_type)
                .or_default()
                .insert(method.name.clone(), method_id);
        } else {
            self.bag.push(
                Diagnostic::error("metodo fuera de una declaracion de tipo")
                    .with_label(span.clone(), "solo los tipos pueden declarar metodos"),
            );
        }

        self.push_scope();
        let self_symbol = self.define("self", SymbolKind::SelfValue, method.span.clone());
        let previous_method = self.current_method;
        let previous_method_name = self.current_method_name.clone();
        self.current_method = Some(self_symbol);
        self.current_method_name = Some(method.name.clone());
        let method_param_ids = self.define_params(&method.params);
        // Mirror what we do for free functions: link the method's params to
        // its own symbol so the type inferer can pre-register their declared
        // types before walking the body.
        if let Some(method_id) = self
            .current_type
            .and_then(|t| self.type_methods.get(&t))
            .and_then(|methods| methods.get(&method.name))
            .copied()
        {
            let anns: Vec<Option<TypeAnn>> =
                method.params.iter().map(|p| p.type_ann.clone()).collect();
            self.function_param_annotations.insert(method_id, anns);
            self.function_param_symbols
                .insert(method_id, method_param_ids);
        }
        self.resolve_expr(&method.body);
        self.current_method = previous_method;
        self.current_method_name = previous_method_name;
        self.pop_scope();
    }

    pub(crate) fn resolve_macro_decl(&mut self, macro_decl: &MacroDecl) {
        self.push_scope();
        for param in &macro_decl.params {
            self.resolve_macro_param_type(param);
            self.define(
                param.name().to_owned(),
                SymbolKind::Parameter,
                param_span(param),
            );
        }
        self.resolve_expr(&macro_decl.body);
        self.pop_scope();
    }

    pub(crate) fn define_params(&mut self, params: &[Param]) -> Vec<crate::symbols::SymbolId> {
        let mut ids = Vec::with_capacity(params.len());
        for param in params {
            // Reject reserved keywords as parameter names. Without this the
            // user gets a confusing 'base usado fuera de un método' or
            // 'self usado fuera de un método' diagnostic emitted by the
            // body resolver instead of being told the name itself is the
            // problem.
            if matches!(param.name.as_str(), "base" | "self") {
                self.bag.push(
                    Diagnostic::error(format!(
                        "'{}' es palabra reservada y no puede ser nombre de parámetro",
                        param.name
                    ))
                    .with_label(param.span.clone(), "renombra este parámetro"),
                );
            }
            self.resolve_type_ann_option(&param.type_ann);
            let id = self.define(
                param.name.clone(),
                SymbolKind::Parameter,
                param.span.clone(),
            );
            ids.push(id);
        }
        ids
    }
}
