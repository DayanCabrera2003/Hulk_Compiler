use hulk_ast::{
    AssignTarget, Expr, ExprKind, FunctionDecl, MacroDecl, MacroParam, MemberKind, NodeId, Param,
    Program, Span, TypeAnn, TypeDecl,
};
use hulk_diagnostics::Diagnostic;

use crate::resolver::param_span;
use crate::symbols::{SymbolId, SymbolKind};
use crate::Resolver;

impl Resolver {
    /// Resolves all names in a program and records expression references.
    pub fn resolve_program(&mut self, program: &Program) {
        self.expr_symbols.clear();
        self.type_parents.clear();
        self.type_methods.clear();
        self.protocol_methods.clear();
        self.protocol_extends.clear();
        self.function_param_annotations.clear();
        self.register_global_declarations(program);
        self.register_protocol_details(&program.protocols);

        for function in &program.functions {
            self.resolve_function_decl(function);
        }

        for type_decl in &program.types {
            self.resolve_type_decl(type_decl);
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
        self.define_params(&function.params);
        self.resolve_expr(&function.body);
        self.validate_expr_against_annotation(&function.body, function.return_type.as_ref());
        self.report_ambiguous_function_inference(function);
        self.pop_scope();
    }

    pub(crate) fn resolve_type_decl(&mut self, type_decl: &TypeDecl) {
        let previous_type = self.current_type;
        self.current_type = self.lookup(&type_decl.name);

        self.push_scope();
        for param in &type_decl.params {
            self.resolve_type_ann_option(&param.type_ann);
            self.define(
                param.name.clone(),
                SymbolKind::Parameter,
                param.span.clone(),
            );
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
        self.define_params(&method.params);
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

    pub(crate) fn define_params(&mut self, params: &[Param]) {
        for param in params {
            self.resolve_type_ann_option(&param.type_ann);
            self.define(
                param.name.clone(),
                SymbolKind::Parameter,
                param.span.clone(),
            );
        }
    }

    pub(crate) fn resolve_assign_target(&mut self, target: &AssignTarget, span: Span) {
        match target {
            AssignTarget::Ident(name) => {
                if name == "self" {
                    self.bag.push(
                        Diagnostic::error("no se puede asignar a self")
                            .with_label(span, "self es inmutable como destino"),
                    );
                    return;
                }
                if self.lookup(name).is_none() {
                    self.bag.push(
                        Diagnostic::error(format!("identificador no declarado: {name}"))
                            .with_label(span, "no existe en el scope visible"),
                    );
                }
            }
            AssignTarget::Field { receiver, .. } => self.resolve_expr(receiver),
            AssignTarget::Index { target, index } => {
                self.resolve_expr(target);
                self.resolve_expr(index);
            }
        }
    }

    // Not migrated to hulk_ast::Visitor: resolve_expr threads an active
    // scope stack (push_scope/define/pop_scope around Block, VecGenerator,
    // For, Lambda) and dispatches custom per-variant logic (resolve_ident,
    // resolve_self, resolve_base, resolve_call, validate_method_call, type
    // annotation resolution). The Visitor's default walk does not know
    // about HULK's scoping semantics, so a migration would leave the
    // majority of the match in place while only saving ~15 lines on the
    // trivial variants. Staying hand-written keeps control flow clear.
    pub(crate) fn resolve_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Number(_) | ExprKind::StringLit(_) | ExprKind::Bool(_) => {}
            ExprKind::Ident(name) => self.resolve_ident_with_id(name, expr.id, expr.span.clone()),
            ExprKind::Self_ => self.resolve_self(expr.id, expr.span.clone()),
            ExprKind::Base => self.resolve_base(expr.id, expr.span.clone()),
            ExprKind::BinOp { left, right, .. } => {
                self.resolve_expr(left);
                self.resolve_expr(right);
            }
            ExprKind::UnaryOp { expr, .. } => self.resolve_expr(expr),
            ExprKind::Call { callee, .. } => {
                self.resolve_call(callee, expr);
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                self.resolve_expr(receiver);
                self.validate_method_call(receiver, expr);
                self.resolve_exprs(args);
            }
            ExprKind::FieldAccess { receiver, .. } => self.resolve_expr(receiver),
            ExprKind::Index { target, index } => {
                self.resolve_expr(target);
                self.resolve_expr(index);
            }
            ExprKind::Block(exprs) => {
                self.push_scope();
                self.resolve_exprs(exprs);
                self.pop_scope();
            }
            ExprKind::VecLiteral(exprs) => self.resolve_exprs(exprs),
            ExprKind::VecGenerator {
                element,
                binding,
                iterable,
            } => {
                self.resolve_expr(iterable);
                self.push_scope();
                self.define(binding.clone(), SymbolKind::Variable, expr.span.clone());
                self.resolve_expr(element);
                self.pop_scope();
            }
            ExprKind::Let { bindings, body } => self.resolve_let(bindings, body),
            ExprKind::Assign { target, value } => {
                self.resolve_expr(target);
                self.resolve_expr(value);
            }
            ExprKind::AssignTarget(target) => self.resolve_assign_target(target, expr.span.clone()),
            ExprKind::LetBinding(binding) => self.resolve_expr(&binding.value),
            ExprKind::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                self.resolve_expr(condition);
                self.resolve_expr(then_branch);
                for (elif_condition, elif_branch) in elif_branches {
                    self.resolve_expr(elif_condition);
                    self.resolve_expr(elif_branch);
                }
                if let Some(else_branch) = else_branch {
                    self.resolve_expr(else_branch);
                }
            }
            ExprKind::While { condition, body } => {
                self.resolve_expr(condition);
                self.resolve_expr(body);
            }
            ExprKind::For {
                binding,
                iterable,
                body,
            } => {
                self.resolve_expr(iterable);
                self.push_scope();
                self.define(binding.clone(), SymbolKind::Variable, expr.span.clone());
                self.resolve_expr(body);
                self.pop_scope();
            }
            ExprKind::New { type_ann, args } => {
                self.resolve_type_ann(type_ann, expr.span.clone());
                self.resolve_exprs(args);
            }
            ExprKind::Is { expr, type_ann } | ExprKind::As { expr, type_ann } => {
                self.resolve_expr(expr);
                self.resolve_type_ann(type_ann, expr.span.clone());
            }
            ExprKind::Lambda { params, body, .. } => {
                self.push_scope();
                self.define_params(params);
                self.resolve_expr(body);
                self.pop_scope();
            }
        }
    }

    pub(crate) fn resolve_exprs(&mut self, exprs: &[Expr]) {
        for expr in exprs {
            self.resolve_expr(expr);
        }
    }

    pub(crate) fn resolve_let(&mut self, bindings: &[Expr], body: &Expr) {
        let mut pushed_scopes = 0usize;

        for binding_expr in bindings {
            let ExprKind::LetBinding(binding) = &binding_expr.kind else {
                continue;
            };

            self.resolve_type_ann_option(&binding.type_ann);
            self.resolve_expr(&binding.value);
            self.validate_expr_against_annotation(&binding.value, binding.type_ann.as_ref());
            self.push_scope();
            self.define(
                binding.name.clone(),
                SymbolKind::Variable,
                binding.span.clone(),
            );
            pushed_scopes += 1;
        }

        self.resolve_expr(body);

        for _ in 0..pushed_scopes {
            self.pop_scope();
        }
    }

    pub(crate) fn resolve_ident_with_id(&mut self, name: &str, node_id: NodeId, span: Span) {
        if name == "self" {
            self.resolve_self(node_id, span);
            return;
        }

        if name == "base" {
            self.resolve_base(node_id, span);
            return;
        }

        match self.lookup(name) {
            Some(symbol_id) => {
                self.validate_symbol_use(name, symbol_id, span.clone());
                self.expr_symbols.insert(node_id, symbol_id);
            }
            None => {
                self.bag.push(
                    Diagnostic::error(format!("identificador no declarado: {name}"))
                        .with_label(span, "no existe en el scope visible"),
                );
            }
        }
    }

    pub(crate) fn resolve_self(&mut self, node_id: NodeId, span: Span) {
        match self.current_method {
            Some(symbol_id) => {
                self.expr_symbols.insert(node_id, symbol_id);
            }
            None => {
                self.bag.push(
                    Diagnostic::error("self usado fuera de un método")
                        .with_label(span, "self solo es valido dentro de métodos"),
                );
            }
        }
    }

    pub(crate) fn resolve_base(&mut self, node_id: NodeId, span: Span) {
        let Some(current_type) = self.current_type else {
            self.bag.push(
                Diagnostic::error("base usado fuera de un método")
                    .with_label(span, "base solo es valido dentro de métodos"),
            );
            return;
        };

        let Some(current_name) = self.current_method_name.as_deref() else {
            self.bag.push(
                Diagnostic::error("base usado fuera de un método")
                    .with_label(span, "base solo es valido dentro de métodos"),
            );
            return;
        };

        let Some(parent_type) = self
            .type_parents
            .get(&current_type)
            .and_then(|parent| *parent)
        else {
            self.bag.push(
                Diagnostic::error("base usado en un tipo sin padre")
                    .with_label(span, "el tipo actual no hereda de otro"),
            );
            return;
        };

        match self
            .type_methods
            .get(&parent_type)
            .and_then(|methods| methods.get(current_name).copied())
        {
            Some(symbol_id) => {
                self.expr_symbols.insert(node_id, symbol_id);
            }
            None => {
                self.bag.push(
                    Diagnostic::error(format!(
                        "no existe implementacion de base para {current_name}"
                    ))
                    .with_label(span, "el padre no implementa este método"),
                );
            }
        }
    }

    pub(crate) fn resolve_call(&mut self, callee: &Expr, call_expr: &Expr) {
        let mut callee_symbol = None;

        if let ExprKind::Ident(name) = &callee.kind {
            match self.lookup(name) {
                Some(symbol_id) => {
                    let callable = matches!(
                        self.table.get(symbol_id).map(|symbol| &symbol.kind),
                        Some(
                            SymbolKind::Function
                                | SymbolKind::BuiltinFunction
                                | SymbolKind::Parameter
                                | SymbolKind::Variable
                                | SymbolKind::SelfValue
                        )
                    );

                    if callable {
                        self.expr_symbols.insert(callee.id, symbol_id);
                        callee_symbol = Some(symbol_id);
                    } else {
                        self.bag.push(
                            Diagnostic::error(format!("{name} no es una funcion"))
                                .with_label(callee.span.clone(), "se esperaba una funcion"),
                        );
                    }
                }
                None => {
                    self.bag.push(
                        Diagnostic::error(format!("funcion no existe: {name}"))
                            .with_label(callee.span.clone(), "no hay una declaracion visible"),
                    );
                }
            }
        } else {
            self.resolve_expr(callee);
        }

        if let ExprKind::Call { args, .. } = &call_expr.kind {
            self.resolve_exprs(args);
            if let Some(symbol_id) = callee_symbol {
                self.validate_call_argument_protocol_conformance(
                    symbol_id,
                    args,
                    call_expr.span.clone(),
                );
            }
        }
    }

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

    fn validate_symbol_use(&mut self, name: &str, symbol_id: SymbolId, span: Span) {
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

    fn validate_type_symbol(&mut self, name: &str, symbol_id: SymbolId, span: Span) {
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
