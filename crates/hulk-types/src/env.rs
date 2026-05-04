use std::collections::HashMap;

use hulk_ast::NodeId;
use hulk_semantic::SymbolId;

use crate::type_id::{BuiltinType, TypeId, TypeKind};

/// Type environment: stores all type information for a program.
pub struct TypeEnv {
    pub(crate) types: Vec<TypeKind>,
    /// Maps SymbolId → TypeId for declarations.
    symbol_types: HashMap<SymbolId, TypeId>,
    /// Maps NodeId (expression) → TypeId for inferred types.
    expr_types: HashMap<NodeId, TypeId>,
}

impl TypeEnv {
    /// Create a new type environment with builtins pre-registered.
    pub fn new() -> Self {
        let mut env = TypeEnv {
            types: Vec::new(),
            symbol_types: HashMap::new(),
            expr_types: HashMap::new(),
        };
        env.register_builtins();
        env
    }

    /// Register the four builtin types: Object, Number, String, Boolean.
    fn register_builtins(&mut self) {
        // Object (ID 0)
        self.types.push(TypeKind::Builtin(BuiltinType::Object));

        // Number (ID 1)
        self.types.push(TypeKind::Builtin(BuiltinType::Number));

        // String (ID 2)
        self.types.push(TypeKind::Builtin(BuiltinType::String));

        // Boolean (ID 3)
        self.types.push(TypeKind::Builtin(BuiltinType::Boolean));
    }

    /// Register a new user-defined type in the environment.
    pub fn register_type(&mut self, name: String, parent: Option<TypeId>) -> TypeId {
        let id = TypeId(self.types.len() as u32);
        self.types.push(TypeKind::UserDefined { name, parent });
        id
    }

    /// Register a new protocol type.
    pub fn register_protocol(&mut self, name: String) -> TypeId {
        let id = TypeId(self.types.len() as u32);
        self.types.push(TypeKind::Protocol { name });
        id
    }

    /// Get the kind of a type by its ID.
    pub fn type_kind(&self, id: TypeId) -> Option<&TypeKind> {
        self.types.get(id.index())
    }

    /// Register the type of a symbol (e.g., a function parameter or variable).
    pub fn register_symbol_type(&mut self, symbol: SymbolId, ty: TypeId) {
        self.symbol_types.insert(symbol, ty);
    }

    /// Get the type of a symbol.
    pub fn symbol_type(&self, symbol: SymbolId) -> Option<TypeId> {
        self.symbol_types.get(&symbol).copied()
    }

    /// Register the inferred type of an expression (by its NodeId).
    pub fn register_expr_type(&mut self, node: NodeId, ty: TypeId) {
        self.expr_types.insert(node, ty);
    }

    /// Get the inferred type of an expression.
    pub fn expr_type(&self, node: NodeId) -> Option<TypeId> {
        self.expr_types.get(&node).copied()
    }

    /// Returns all expression nodes that currently have an inferred type.
    pub fn expr_type_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.expr_types.keys().copied()
    }

    /// Returns all symbols that currently have a registered type.
    pub fn symbol_type_symbols(&self) -> impl Iterator<Item = SymbolId> + '_ {
        self.symbol_types.keys().copied()
    }

    /// Check if type `t1` conforms to (is assignable to) type `t2`.
    ///
    /// Rules:
    /// - Identity: t1 == t2
    /// - Top type: any type conforms to Object
    /// - Subtyping by inheritance: if t1 inherits from t2
    /// - Structural conformance to protocol (future subsession)
    pub fn conforms(&self, t1: TypeId, t2: TypeId) -> bool {
        // Identity
        if t1 == t2 {
            return true;
        }

        // Any type conforms to Object (top type)
        if t2 == TypeId::OBJECT {
            return true;
        }

        // Subtyping by inheritance: check if t1's parent chain includes t2
        if let Some(TypeKind::UserDefined {
            parent: Some(parent),
            ..
        }) = self.type_kind(t1)
        {
            if self.conforms(*parent, t2) {
                return true;
            }
        }

        false
    }

    /// Find the least common ancestor of two types.
    /// Returns the most specific type that both types conform to.
    pub fn lca(&self, t1: TypeId, t2: TypeId) -> TypeId {
        // If t1 conforms to t2, LCA is t2
        if self.conforms(t1, t2) {
            return t2;
        }

        // If t2 conforms to t1, LCA is t1
        if self.conforms(t2, t1) {
            return t1;
        }

        // Otherwise, climb the inheritance chain of t1 and test against t2
        if let Some(TypeKind::UserDefined {
            parent: Some(parent),
            ..
        }) = self.type_kind(t1)
        {
            return self.lca(*parent, t2);
        }

        // Fallback: Object is the top type
        TypeId::OBJECT
    }
}

impl Default for TypeEnv {
    fn default() -> Self {
        Self::new()
    }
}
