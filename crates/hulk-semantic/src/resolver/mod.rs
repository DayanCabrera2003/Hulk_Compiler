use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use hulk_ast::{NodeId, SourceFile, Span, TypeAnn};
use hulk_diagnostics::{Diagnostic, DiagnosticBag};

use crate::symbols::{Scope, SymbolId, SymbolKind, SymbolTable};

mod builtins;
mod inheritance;
mod names;
mod protocols;

/// Name resolver with a symbol table and lexical scope stack.
#[derive(Debug)]
pub struct Resolver {
    pub(crate) table: SymbolTable,
    pub(crate) scopes: Vec<Scope>,
    pub(crate) expr_symbols: HashMap<NodeId, SymbolId>,
    pub(crate) type_parents: HashMap<SymbolId, Option<SymbolId>>,
    pub(crate) type_methods: HashMap<SymbolId, HashMap<String, SymbolId>>,
    pub(crate) protocol_methods: HashMap<SymbolId, HashSet<String>>,
    pub(crate) protocol_extends: HashMap<SymbolId, Vec<SymbolId>>,
    pub(crate) function_param_annotations: HashMap<SymbolId, Vec<Option<TypeAnn>>>,
    /// Per-function map: function symbol → list of its param symbol ids in
    /// declaration order. Populated when resolving a function body so the
    /// type inferer can register each param's declared type before walking
    /// the body.
    pub(crate) function_param_symbols: HashMap<SymbolId, Vec<SymbolId>>,
    pub(crate) bag: DiagnosticBag,
    pub(crate) current_type: Option<SymbolId>,
    pub(crate) current_method: Option<SymbolId>,
    pub(crate) current_method_name: Option<String>,
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
            function_param_symbols: HashMap::new(),
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

    /// Register an expression node → symbol binding. Used by later passes
    /// (like the desugarer's closure capture) that synthesise new Ident
    /// nodes which still need to resolve to a pre-existing symbol.
    pub fn bind_expr_symbol(&mut self, node_id: NodeId, symbol: SymbolId) {
        self.expr_symbols.insert(node_id, symbol);
    }

    /// Returns the param symbol ids of a function (in declaration order),
    /// or None if the function symbol is unknown.
    #[must_use]
    pub fn function_param_symbols(&self, function: SymbolId) -> Option<&[SymbolId]> {
        self.function_param_symbols
            .get(&function)
            .map(|v| v.as_slice())
    }

    /// Returns the param annotations of a function (in declaration order),
    /// or None if the function symbol is unknown.
    #[must_use]
    pub fn function_param_annotations(&self, function: SymbolId) -> Option<&[Option<TypeAnn>]> {
        self.function_param_annotations
            .get(&function)
            .map(|v| v.as_slice())
    }

    /// Returns the symbol id of a method `name` declared inside `type_id`,
    /// or `None` when the type has no such method.
    ///
    /// Exposed so the type inferer can pre-register a method's declared
    /// parameter types before walking its body — without this, references
    /// to method params inside the body collapse to `Object` and downstream
    /// codegen produces type-incorrect IR.
    #[must_use]
    pub fn method_symbol(&self, type_id: SymbolId, name: &str) -> Option<SymbolId> {
        self.type_methods.get(&type_id)?.get(name).copied()
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

    pub(crate) fn synthetic_span(&self) -> Span {
        Span::dummy(Arc::new(SourceFile::new("<synthetic>", "")))
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn param_span(param: &hulk_ast::decl::MacroParam) -> Span {
    match param {
        hulk_ast::decl::MacroParam::Regular { span, .. }
        | hulk_ast::decl::MacroParam::Body { span, .. }
        | hulk_ast::decl::MacroParam::Symbolic { span, .. }
        | hulk_ast::decl::MacroParam::Placeholder { span, .. } => span.clone(),
    }
}
