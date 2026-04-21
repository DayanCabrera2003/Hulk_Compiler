use std::collections::HashMap;
use std::sync::Arc;

use hulk_diagnostics::DiagnosticBag;
use hulk_span::{SourceFile, Span};

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
    bag: DiagnosticBag,
    current_type: Option<SymbolId>,
    current_method: Option<SymbolId>,
}

impl Resolver {
    /// Creates a resolver preloaded with builtin bindings.
    #[must_use]
    pub fn new() -> Self {
        let mut resolver = Self {
            table: SymbolTable::new(),
            scopes: vec![Scope::new()],
            bag: DiagnosticBag::new(),
            current_type: None,
            current_method: None,
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

    fn register_builtins(&mut self) {
        let file = Arc::new(SourceFile::new("<builtins>", ""));
        let span = Span::dummy(file);

        for name in ["print", "sqrt", "sin", "cos", "exp", "log", "rand", "range"] {
            self.define(name, SymbolKind::BuiltinFunction, span.clone());
        }

        for name in ["PI", "E"] {
            self.define(name, SymbolKind::BuiltinValue, span.clone());
        }
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

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
}
