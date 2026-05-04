use std::collections::HashMap;

use hulk_ast::Span;

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
pub(crate) struct Scope {
    pub(crate) bindings: HashMap<String, SymbolId>,
}

impl Scope {
    pub(crate) fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }
}
