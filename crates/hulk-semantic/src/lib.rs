use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use hulk_ast::{
    AssignTarget, Expr, ExprKind, FunctionDecl, MacroDecl, MacroParam, MemberKind, NodeId, Param,
    ParentSpec, Program, ProtocolDecl, SourceFile, Span, TypeAnn, TypeDecl,
};
use hulk_diagnostics::Diagnostic;
use hulk_diagnostics::DiagnosticBag;

mod validation;

/// Stable identifier for a symbol stored in the semantic table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(pub u32);

/// Kind of symbol tracked by the resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    /// A global or local variable binding.
    Variable,
    /// A function or method.
    Function,
    /// A type declaration.
    Type,
    /// A protocol declaration.
    Protocol,
    /// A macro declaration.
    Macro,
    /// A parameter binding.
    Parameter,
    /// The implicit `self` binding inside a method.
    SelfValue,
    /// A builtin function.
    BuiltinFunction,
    /// A builtin constant or value.
    BuiltinValue,
    /// A builtin type.
    BuiltinType,
}

/// Symbol metadata stored by the semantic resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// Stable identifier assigned by the table.
    pub id: SymbolId,
    /// User-visible name for the symbol.
    pub name: String,
    /// Semantic category of the symbol.
    pub kind: SymbolKind,
    /// Source location where the symbol was defined.
    pub span: Span,
}

/// Dense symbol table backed by a vector.
#[derive(Debug, Default)]
pub struct SymbolTable {
    symbols: Vec<Symbol>,
}

impl SymbolTable {
    /// Creates an empty symbol table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a new symbol and returns its stable identifier.
    #[must_use]
    pub fn add(&mut self, name: impl Into<String>, kind: SymbolKind, span: Span) -> SymbolId {
        let id = SymbolId(match u32::try_from(self.symbols.len()) {
            Ok(value) => value,
            Err(_) => unreachable!("symbol table exceeded the u32 range"),
        });

        self.symbols.push(Symbol {
            id,
            name: name.into(),
            kind,
            span,
        });
        id
    }

    /// Returns the symbol stored at `id`, if it exists.
    #[must_use]
    pub fn get(&self, id: SymbolId) -> Option<&Symbol> {
        usize::try_from(id.0)
            .ok()
            .and_then(|index| self.symbols.get(index))
    }

    /// Returns the name stored at `id`, if it exists.
    #[must_use]
    pub fn name_of(&self, id: SymbolId) -> Option<&str> {
        self.get(id).map(|symbol| symbol.name.as_str())
    }
}

#[derive(Debug)]
struct Scope {
    bindings: HashMap<String, SymbolId>,
}

impl Scope {
    fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }
}

/// Name resolver with a symbol table and lexical scope stack.
#[derive(Debug)]
pub struct Resolver {
    table: SymbolTable,
    scopes: Vec<Scope>,
    expr_symbols: HashMap<NodeId, SymbolId>,
    type_parents: HashMap<SymbolId, Option<SymbolId>>,
    type_methods: HashMap<SymbolId, HashMap<String, SymbolId>>,
    protocol_methods: HashMap<SymbolId, HashSet<String>>,
    protocol_extends: HashMap<SymbolId, Vec<SymbolId>>,
    function_param_annotations: HashMap<SymbolId, Vec<Option<TypeAnn>>>,
    bag: DiagnosticBag,
    current_type: Option<SymbolId>,
    current_method: Option<SymbolId>,
    current_method_name: Option<String>,
}

impl Resolver {
    /// Creates a resolver preloaded with builtin bindings.
    #[must_use]
    pub fn new() -> Self {
        let mut resolver = Self {
            table: SymbolTable::new(),
            scopes: vec![Scope::new()],
            expr_symbols: HashMap::new(),
            type_parents: HashMap::new(),
            type_methods: HashMap::new(),
            protocol_methods: HashMap::new(),
            protocol_extends: HashMap::new(),
            function_param_annotations: HashMap::new(),
            bag: DiagnosticBag::new(),
            current_type: None,
            current_method: None,
            current_method_name: None,
        };
        resolver.register_builtins();
        resolver
    }

    /// Returns the symbol table currently owned by the resolver.
    #[must_use]
    pub fn table(&self) -> &SymbolTable {
        &self.table
    }

    /// Returns the diagnostics collected so far.
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticBag {
        &self.bag
    }

    /// Returns the resolved symbol for an expression node, if available.
    #[must_use]
    pub fn expr_symbol(&self, node_id: NodeId) -> Option<SymbolId> {
        self.expr_symbols.get(&node_id).copied()
    }

    /// Returns true when an expression node has an associated symbol.
    #[must_use]
    pub fn has_expr_symbol(&self, node_id: NodeId) -> bool {
        self.expr_symbols.contains_key(&node_id)
    }

    /// Binds an expression node to a resolved symbol. Later passes (e.g. the
    /// macro expander) use this to record placeholder symbols that were
    /// introduced outside the normal scope-walking phase.
    pub fn record_expr_symbol(&mut self, node_id: NodeId, symbol_id: SymbolId) {
        self.expr_symbols.insert(node_id, symbol_id);
    }

    /// Allocates a fresh symbol in the global symbol table, bypassing scope
    /// insertion and redefinition checks. Intended for synthetic bindings
    /// (macro placeholders) that must not clash with user-declared names.
    pub fn allocate_symbol(
        &mut self,
        name: impl Into<String>,
        symbol_kind: SymbolKind,
        span: Span,
    ) -> SymbolId {
        self.table.add(name, symbol_kind, span)
    }

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

    /// Returns the current enclosing type, if any.
    #[must_use]
    pub fn current_type(&self) -> Option<SymbolId> {
        self.current_type
    }

    /// Sets the current enclosing type.
    pub fn set_current_type(&mut self, current_type: Option<SymbolId>) {
        self.current_type = current_type;
    }

    /// Returns the current enclosing method, if any.
    #[must_use]
    pub fn current_method(&self) -> Option<SymbolId> {
        self.current_method
    }

    /// Sets the current enclosing method.
    pub fn set_current_method(&mut self, current_method: Option<SymbolId>) {
        self.current_method = current_method;
    }

    /// Pushes a new empty lexical scope on the stack.
    pub fn push_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    /// Removes the innermost lexical scope if one exists above the global scope.
    pub fn pop_scope(&mut self) -> Option<HashMap<String, SymbolId>> {
        if self.scopes.len() > 1 {
            self.scopes.pop().map(|scope| scope.bindings)
        } else {
            None
        }
    }

    /// Defines a new symbol in the innermost scope and returns its identifier.
    pub fn define(
        &mut self,
        name: impl Into<String>,
        symbol_kind: SymbolKind,
        span: Span,
    ) -> SymbolId {
        let name = name.into();

        if let Some(scope) = self.scopes.last_mut() {
            if let Some(existing) = scope.bindings.get(&name).copied() {
                self.bag.push(
                    Diagnostic::error(format!("redefinicion de {name}"))
                        .with_label(span, "ya estaba definida en este scope"),
                );
                return existing;
            }
        }

        let id = self.table.add(name.clone(), symbol_kind, span);

        if let Some(scope) = self.scopes.last_mut() {
            scope.bindings.insert(name, id);
        }

        id
    }

    /// Looks up a name starting from the innermost scope.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<SymbolId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.bindings.get(name).copied())
    }

    fn register_global_declarations(&mut self, program: &Program) {
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

    fn register_protocol_details(&mut self, protocols: &[ProtocolDecl]) {
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

    fn resolve_function_decl(&mut self, function: &FunctionDecl) {
        self.push_scope();
        self.resolve_type_ann_option(&function.return_type);
        self.define_params(&function.params);
        self.resolve_expr(&function.body);
        self.validate_expr_against_annotation(&function.body, function.return_type.as_ref());
        self.report_ambiguous_function_inference(function);
        self.pop_scope();
    }

    fn resolve_type_decl(&mut self, type_decl: &TypeDecl) {
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

    fn resolve_member(&mut self, member: &hulk_ast::decl::Member) {
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

    fn resolve_method_decl(&mut self, method: &FunctionDecl, span: &Span) {
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

    fn resolve_macro_decl(&mut self, macro_decl: &MacroDecl) {
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

    fn define_params(&mut self, params: &[Param]) {
        for param in params {
            self.resolve_type_ann_option(&param.type_ann);
            self.define(
                param.name.clone(),
                SymbolKind::Parameter,
                param.span.clone(),
            );
        }
    }

    fn resolve_assign_target(&mut self, target: &AssignTarget, span: Span) {
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

    fn resolve_expr(&mut self, expr: &Expr) {
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

    fn resolve_exprs(&mut self, exprs: &[Expr]) {
        for expr in exprs {
            self.resolve_expr(expr);
        }
    }

    fn resolve_let(&mut self, bindings: &[Expr], body: &Expr) {
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

    fn resolve_ident_with_id(&mut self, name: &str, node_id: NodeId, span: Span) {
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

    fn register_builtins(&mut self) {
        let file = Arc::new(SourceFile::new("<builtins>", ""));
        let span = Span::dummy(file);

        for name in ["print", "sqrt", "sin", "cos", "exp", "log", "rand", "range"] {
            self.define(name, SymbolKind::BuiltinFunction, span.clone());
        }

        for name in ["PI", "E"] {
            self.define(name, SymbolKind::BuiltinValue, span.clone());
        }

        for name in ["Object", "Number", "String", "Boolean"] {
            self.define(name, SymbolKind::BuiltinType, span.clone());
        }
    }

    fn resolve_self(&mut self, node_id: NodeId, span: Span) {
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

    fn resolve_base(&mut self, node_id: NodeId, span: Span) {
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

    fn resolve_call(&mut self, callee: &Expr, call_expr: &Expr) {
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

    fn resolve_type_ann_option(&mut self, type_ann: &Option<TypeAnn>) {
        if let Some(type_ann) = type_ann {
            self.resolve_type_ann(type_ann, self.synthetic_span());
        }
    }

    fn resolve_macro_param_type(&mut self, param: &MacroParam) {
        self.resolve_type_ann(param.type_ann(), self.synthetic_span());
    }

    fn resolve_parent_spec(&mut self, parent: Option<&ParentSpec>) -> Option<SymbolId> {
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

    fn resolve_type_ann(&mut self, type_ann: &TypeAnn, span: Span) {
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

    fn resolve_type_name(&mut self, name: &str, span: Span) {
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

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

fn param_span(param: &hulk_ast::decl::MacroParam) -> Span {
    match param {
        hulk_ast::decl::MacroParam::Regular { span, .. }
        | hulk_ast::decl::MacroParam::Body { span, .. }
        | hulk_ast::decl::MacroParam::Symbolic { span, .. }
        | hulk_ast::decl::MacroParam::Placeholder { span, .. } => span.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use hulk_ast::{
        Expr, ExprKind, FunctionDecl, LetBinding, Member, MemberKind, Param, Program, TypeDecl,
    };
    use hulk_diagnostics::Severity;

    use super::*;

    fn test_span() -> Span {
        let file = Arc::new(SourceFile::new("test.hulk", "x"));
        Span::new(file, 0, 1)
    }

    #[test]
    fn symbol_table_add_get_and_name_of_work() {
        let mut table = SymbolTable::new();
        let span = test_span();
        let id = table.add("x", SymbolKind::Variable, span.clone());

        let Some(symbol) = table.get(id) else {
            panic!("symbol should exist");
        };
        assert_eq!(symbol.id, id);
        assert_eq!(symbol.name, "x");
        assert_eq!(symbol.kind, SymbolKind::Variable);
        assert_eq!(symbol.span, span);
        assert_eq!(table.name_of(id), Some("x"));
    }

    #[test]
    fn resolver_push_and_pop_scopes() {
        let mut resolver = Resolver::new();
        let root_len = resolver.scopes.len();

        resolver.push_scope();
        assert_eq!(resolver.scopes.len(), root_len + 1);
        assert!(resolver.pop_scope().is_some());
        assert_eq!(resolver.scopes.len(), root_len);
        assert!(resolver.pop_scope().is_none());
    }

    #[test]
    fn resolver_lookup_finds_local_and_outer_bindings() {
        let mut resolver = Resolver::new();
        let span = test_span();

        let global = resolver.define("x", SymbolKind::Variable, span.clone());
        resolver.push_scope();
        let local = resolver.define("y", SymbolKind::Variable, span);

        assert_eq!(resolver.lookup("x"), Some(global));
        assert_eq!(resolver.lookup("y"), Some(local));
        assert_eq!(resolver.lookup("missing"), None);
    }

    #[test]
    fn resolver_registers_builtins_in_global_scope() {
        let resolver = Resolver::new();

        for name in [
            "print", "sqrt", "sin", "cos", "exp", "log", "rand", "range", "PI", "E",
        ] {
            let Some(id) = resolver.lookup(name) else {
                panic!("builtin should resolve: {name}");
            };
            assert_eq!(resolver.table().name_of(id), Some(name));
        }
    }

    fn ident_expr(name: &str, id: u32, span: &Span) -> Expr {
        Expr::new(ExprKind::Ident(name.to_owned()), span.clone(), NodeId(id))
    }

    fn call_expr(callee: Expr, args: Vec<Expr>, id: u32, span: &Span) -> Expr {
        Expr::new(
            ExprKind::Call {
                callee: Box::new(callee),
                args,
            },
            span.clone(),
            NodeId(id),
        )
    }

    fn function_decl(name: &str, body: Expr, span: &Span) -> FunctionDecl {
        FunctionDecl {
            name: name.to_owned(),
            params: vec![],
            return_type: None,
            body,
            span: span.clone(),
        }
    }

    #[test]
    fn resolve_mutual_recursion_between_functions() {
        let source = "function f() => g(); function g() => f();";
        let file = Arc::new(SourceFile::new("mutual.hulk", source));
        let span = Span::new(file, 0, source.len());

        let f_body = call_expr(ident_expr("g", 1, &span), vec![], 2, &span);
        let g_body = call_expr(ident_expr("f", 3, &span), vec![], 4, &span);
        let program = Program {
            functions: vec![
                function_decl("f", f_body, &span),
                function_decl("g", g_body, &span),
            ],
            types: vec![],
            protocols: vec![],
            macros: vec![],
            body: Expr::new(ExprKind::Number(0.0), span.clone(), NodeId(5)),
        };

        let mut resolver = Resolver::new();
        resolver.resolve_program(&program);

        let f_symbol = resolver.lookup("f").expect("f should be registered");
        let g_symbol = resolver.lookup("g").expect("g should be registered");
        assert_eq!(resolver.expr_symbol(NodeId(1)), Some(g_symbol));
        assert_eq!(resolver.expr_symbol(NodeId(3)), Some(f_symbol));
    }

    #[test]
    fn resolve_let_sequentially() {
        let source = "let a = 1, b = a in b;";
        let file = Arc::new(SourceFile::new("let.hulk", source));
        let span = Span::new(file, 0, source.len());

        let binding_a = Expr::new(
            ExprKind::LetBinding(LetBinding {
                name: "a".to_owned(),
                type_ann: None,
                value: Box::new(Expr::new(ExprKind::Number(1.0), span.clone(), NodeId(10))),
                span: span.clone(),
            }),
            span.clone(),
            NodeId(11),
        );
        let binding_b = Expr::new(
            ExprKind::LetBinding(LetBinding {
                name: "b".to_owned(),
                type_ann: None,
                value: Box::new(ident_expr("a", 12, &span)),
                span: span.clone(),
            }),
            span.clone(),
            NodeId(13),
        );
        let let_expr = Expr::new(
            ExprKind::Let {
                bindings: vec![binding_a, binding_b],
                body: Box::new(ident_expr("b", 14, &span)),
            },
            span.clone(),
            NodeId(15),
        );
        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![],
            body: let_expr,
        };

        let mut resolver = Resolver::new();
        resolver.resolve_program(&program);

        assert!(resolver.has_expr_symbol(NodeId(12)));
        assert!(resolver.has_expr_symbol(NodeId(14)));
    }

    #[test]
    fn resolve_type_members_in_type_scope() {
        let file = Arc::new(SourceFile::new("type.hulk", "type Point(x) { x; }"));
        let span = Span::new(file, 0, 20);

        let type_param = Param {
            name: "x".to_owned(),
            type_ann: None,
            span: span.clone(),
        };
        let attr = Member {
            kind: MemberKind::Attribute {
                name: "value".to_owned(),
                type_ann: None,
                value: ident_expr("x", 20, &span),
            },
            span: span.clone(),
        };
        let type_decl = TypeDecl {
            name: "Point".to_owned(),
            params: vec![type_param],
            parent: None,
            members: vec![attr],
            span: span.clone(),
        };
        let program = Program {
            functions: vec![],
            types: vec![type_decl],
            protocols: vec![],
            macros: vec![],
            body: Expr::new(ExprKind::Number(0.0), span, NodeId(21)),
        };

        let mut resolver = Resolver::new();
        resolver.resolve_program(&program);

        assert!(resolver.has_expr_symbol(NodeId(20)));
    }

    fn diagnostic_messages(resolver: &Resolver) -> Vec<String> {
        resolver
            .diagnostics()
            .diagnostics()
            .iter()
            .filter(|diagnostic| matches!(diagnostic.severity, Severity::Error))
            .map(|diagnostic| diagnostic.message.clone())
            .collect()
    }

    fn run_program(body: Expr) -> Resolver {
        let source = "test";
        let file = Arc::new(SourceFile::new("test.hulk", source));
        let span = Span::new(file, 0, source.len());
        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![],
            body: Expr::new(body.kind, span, body.id),
        };

        let mut resolver = Resolver::new();
        resolver.resolve_program(&program);
        resolver
    }

    #[test]
    fn self_outside_method_reports_error() {
        let span = test_span();
        let resolver = run_program(Expr::new(ExprKind::Self_, span, NodeId(30)));

        assert!(diagnostic_messages(&resolver)
            .iter()
            .any(|message| message.contains("self usado fuera de un método")));
    }

    #[test]
    fn assigning_to_self_reports_error() {
        let span = test_span();
        let resolver = run_program(Expr::new(
            ExprKind::AssignTarget(AssignTarget::Ident("self".to_owned())),
            span,
            NodeId(31),
        ));

        assert!(diagnostic_messages(&resolver)
            .iter()
            .any(|message| message.contains("no se puede asignar a self")));
    }

    #[test]
    fn undefined_variable_reports_error() {
        let span = test_span();
        let resolver = run_program(ident_expr("missing", 32, &span));

        assert!(diagnostic_messages(&resolver)
            .iter()
            .any(|message| message.contains("identificador no declarado")));
    }

    #[test]
    fn duplicate_parameter_in_same_scope_reports_error() {
        let source = "function f(x, x) => 0;";
        let file = Arc::new(SourceFile::new("dup.hulk", source));
        let span = Span::new(file, 0, source.len());
        let program = Program {
            functions: vec![FunctionDecl {
                name: "f".to_owned(),
                params: vec![
                    Param {
                        name: "x".to_owned(),
                        type_ann: None,
                        span: span.clone(),
                    },
                    Param {
                        name: "x".to_owned(),
                        type_ann: None,
                        span: span.clone(),
                    },
                ],
                return_type: None,
                body: Expr::new(ExprKind::Number(0.0), span.clone(), NodeId(33)),
                span: span.clone(),
            }],
            types: vec![],
            protocols: vec![],
            macros: vec![],
            body: Expr::new(ExprKind::Number(0.0), span, NodeId(34)),
        };

        let mut resolver = Resolver::new();
        resolver.resolve_program(&program);

        assert!(diagnostic_messages(&resolver)
            .iter()
            .any(|message| message.contains("redefinicion de x")));
    }

    #[test]
    fn missing_function_call_reports_error() {
        let span = test_span();
        let resolver = run_program(Expr::new(
            ExprKind::Call {
                callee: Box::new(ident_expr("missing", 35, &span)),
                args: vec![],
            },
            span,
            NodeId(36),
        ));

        assert!(diagnostic_messages(&resolver)
            .iter()
            .any(|message| message.contains("funcion no existe")));
    }

    #[test]
    fn missing_type_annotation_reports_error() {
        let span = test_span();
        let resolver = run_program(Expr::new(
            ExprKind::New {
                type_ann: TypeAnn::Named("Ghost".to_owned()),
                args: vec![],
            },
            span,
            NodeId(37),
        ));

        assert!(diagnostic_messages(&resolver)
            .iter()
            .any(|message| message.contains("tipo no existe")));
    }

    #[test]
    fn base_without_parent_reports_error() {
        let source = "type Child() { method() => base; }";
        let file = Arc::new(SourceFile::new("base.hulk", source));
        let span = Span::new(file, 0, source.len());

        let method = FunctionDecl {
            name: "method".to_owned(),
            params: vec![],
            return_type: None,
            body: Expr::new(ExprKind::Base, span.clone(), NodeId(38)),
            span: span.clone(),
        };
        let program = Program {
            functions: vec![],
            types: vec![TypeDecl {
                name: "Child".to_owned(),
                params: vec![],
                parent: None,
                members: vec![Member {
                    kind: MemberKind::Method(method),
                    span: span.clone(),
                }],
                span: span.clone(),
            }],
            protocols: vec![],
            macros: vec![],
            body: Expr::new(ExprKind::Number(0.0), span, NodeId(39)),
        };

        let mut resolver = Resolver::new();
        resolver.resolve_program(&program);

        assert!(diagnostic_messages(&resolver)
            .iter()
            .any(|message| message.contains("base usado en un tipo sin padre")));
    }

    #[test]
    fn base_resolves_to_parent_method() {
        let source =
            "type Parent() { method() => 1; } type Child() inherits Parent { method() => base; }";
        let file = Arc::new(SourceFile::new("inherit.hulk", source));
        let span = Span::new(file, 0, source.len());

        let parent_method = FunctionDecl {
            name: "method".to_owned(),
            params: vec![],
            return_type: None,
            body: Expr::new(ExprKind::Number(1.0), span.clone(), NodeId(40)),
            span: span.clone(),
        };
        let child_method = FunctionDecl {
            name: "method".to_owned(),
            params: vec![],
            return_type: None,
            body: Expr::new(ExprKind::Base, span.clone(), NodeId(41)),
            span: span.clone(),
        };
        let program = Program {
            functions: vec![],
            types: vec![
                TypeDecl {
                    name: "Parent".to_owned(),
                    params: vec![],
                    parent: None,
                    members: vec![Member {
                        kind: MemberKind::Method(parent_method),
                        span: span.clone(),
                    }],
                    span: span.clone(),
                },
                TypeDecl {
                    name: "Child".to_owned(),
                    params: vec![],
                    parent: Some(ParentSpec {
                        name: "Parent".to_owned(),
                        args: vec![],
                        span: span.clone(),
                    }),
                    members: vec![Member {
                        kind: MemberKind::Method(child_method),
                        span: span.clone(),
                    }],
                    span: span.clone(),
                },
            ],
            protocols: vec![],
            macros: vec![],
            body: Expr::new(ExprKind::Number(0.0), span, NodeId(42)),
        };

        let mut resolver = Resolver::new();
        resolver.resolve_program(&program);

        assert!(resolver.has_expr_symbol(NodeId(41)));
    }
}
