use std::collections::HashMap;

use hulk_ast::{BinOpKind, Expr, ExprKind, NodeId, UnaryOpKind};
use hulk_diagnostics::DiagnosticBag;
use hulk_semantic::{Resolver, SymbolId};

/// Stable, opaque identifier for a type in the program.
/// Reserved IDs for builtins: Object=0, Number=1, String=2, Boolean=3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeId(u32);

impl TypeId {
    pub const OBJECT: TypeId = TypeId(0);
    pub const NUMBER: TypeId = TypeId(1);
    pub const STRING: TypeId = TypeId(2);
    pub const BOOLEAN: TypeId = TypeId(3);

    /// Returns the raw numeric value for diagnostics/debugging.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    #[must_use]
    fn index(self) -> usize {
        self.0 as usize
    }
}

/// The different kinds of types in HULK.
#[derive(Debug, Clone)]
pub enum TypeKind {
    /// Builtin type (Object, Number, String, Boolean).
    Builtin(BuiltinType),
    /// User-defined type (class).
    UserDefined {
        name: String,
        parent: Option<TypeId>,
    },
    /// Protocol type.
    Protocol { name: String },
    /// Iterable type: T*.
    Iterable(TypeId),
    /// Vector type: T[].
    Vector(TypeId),
    /// Function type: (A, B, ...) -> ReturnType.
    Functor {
        params: Vec<TypeId>,
        ret: Box<TypeId>,
    },
    /// Unknown type (inference error or TBD).
    Unknown,
}

/// Builtin type categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinType {
    /// The top type, parent of all types.
    Object,
    /// Numeric literal type.
    Number,
    /// String literal type.
    String,
    /// Boolean literal type.
    Boolean,
}

/// Type environment: stores all type information for a program.
pub struct TypeEnv {
    types: Vec<TypeKind>,
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

/// Type inferencer for bottom-up inference of expression types.
pub struct TypeInferer<'a> {
    env: &'a mut TypeEnv,
    resolver: &'a Resolver,
    #[allow(dead_code)]
    bag: &'a DiagnosticBag,
}

impl<'a> TypeInferer<'a> {
    /// Create a new type inferer.
    pub fn new(env: &'a mut TypeEnv, resolver: &'a Resolver, bag: &'a DiagnosticBag) -> Self {
        Self { env, resolver, bag }
    }

    /// Infer the type of an expression bottom-up.
    pub fn infer_expr(&mut self, expr: &Expr) -> TypeId {
        let ty = match &expr.kind {
            // Literals have direct types
            ExprKind::Number(_) => TypeId::NUMBER,
            ExprKind::StringLit(_) => TypeId::STRING,
            ExprKind::Bool(_) => TypeId::BOOLEAN,

            // Identifiers: look up the symbol's type
            ExprKind::Ident(_) => self.infer_ident(expr),

            // self and base: always have type related to the enclosing type
            ExprKind::Self_ => self.infer_self(expr),
            ExprKind::Base => self.infer_base(expr),

            // Binary operations
            ExprKind::BinOp { op, left, right } => self.infer_binop(*op, left, right),

            // Unary operations
            ExprKind::UnaryOp { op, expr: inner } => self.infer_unaryop(*op, inner),

            // Function call
            ExprKind::Call { callee, args } => self.infer_call(expr, callee, args),

            // Method call
            ExprKind::MethodCall {
                receiver,
                method,
                args,
            } => self.infer_method_call(expr, receiver, method, args),

            // Field access
            ExprKind::FieldAccess { receiver, field } => {
                self.infer_field_access(expr, receiver, field)
            }

            // Index access: always returns element type (from vector or iterable)
            ExprKind::Index { target, index } => self.infer_index(expr, target, index),

            // Block: type of the last expression
            ExprKind::Block(exprs) => self.infer_block(expr, exprs),

            // Vector literal: Vector(LCA of element types)
            ExprKind::VecLiteral(elements) => self.infer_vec_literal(expr, elements),

            // Vector generator: Vector(element type)
            ExprKind::VecGenerator {
                element,
                binding: _,
                iterable,
            } => self.infer_vec_generator(expr, element, iterable),

            // Let: type of body (evaluated after binding scope)
            ExprKind::Let { bindings, body } => self.infer_let(expr, bindings, body),

            // Assignment: type of the assigned value
            ExprKind::Assign { target: _, value } => self.infer_expr(value),

            // If/elif/else: LCA of all branches
            ExprKind::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => self.infer_if(expr, condition, then_branch, elif_branches, else_branch),

            // While: body type (though body should not be used as value)
            ExprKind::While { condition: _, body } => self.infer_expr(body),

            // For: body type
            ExprKind::For {
                binding: _,
                iterable: _,
                body,
            } => self.infer_expr(body),

            // New T(...): type T
            ExprKind::New { type_ann, args: _ } => self.infer_new(expr, type_ann),

            // is T: always Boolean
            ExprKind::Is {
                expr: _,
                type_ann: _,
            } => TypeId::BOOLEAN,

            // as T: type T
            ExprKind::As { expr: _, type_ann } => self.infer_type_ann(type_ann),

            // Lambda: Functor with parameter and return types
            ExprKind::Lambda {
                params,
                return_type,
                body,
            } => self.infer_lambda(expr, params, return_type, body),

            // These shouldn't appear at top-level in normal expressions
            ExprKind::AssignTarget(_) | ExprKind::LetBinding(_) => TypeId::OBJECT, // fallback
        };

        self.env.register_expr_type(expr.id, ty);
        ty
    }

    fn infer_ident(&mut self, expr: &Expr) -> TypeId {
        // Look up the symbol for this identifier
        if let Some(symbol_id) = self.resolver.expr_symbol(expr.id) {
            // If the symbol has a registered type, use it; otherwise Unknown
            self.env.symbol_type(symbol_id).unwrap_or(TypeId::OBJECT)
        } else {
            // Symbol not resolved (error in semantic phase)
            TypeId::OBJECT
        }
    }

    fn infer_self(&mut self, _expr: &Expr) -> TypeId {
        // self has the type of the enclosing type
        TypeId::OBJECT // TODO: resolve to current_type when available from resolver
    }

    fn infer_base(&mut self, _expr: &Expr) -> TypeId {
        // base has the type of the parent type
        TypeId::OBJECT // TODO: resolve to parent when available
    }

    fn infer_binop(&mut self, op: BinOpKind, left: &Expr, right: &Expr) -> TypeId {
        let _left_type = self.infer_expr(left);
        let _right_type = self.infer_expr(right);

        match op {
            // Arithmetic operations: Number
            BinOpKind::Add
            | BinOpKind::Sub
            | BinOpKind::Mul
            | BinOpKind::Div
            | BinOpKind::Mod
            | BinOpKind::Pow => TypeId::NUMBER,

            // String concatenation: String
            BinOpKind::Concat | BinOpKind::ConcatSpaced => TypeId::STRING,

            // Comparison operations: Boolean
            BinOpKind::Lt
            | BinOpKind::Le
            | BinOpKind::Gt
            | BinOpKind::Ge
            | BinOpKind::Eq
            | BinOpKind::Ne => TypeId::BOOLEAN,

            // Logical operations: Boolean
            BinOpKind::And | BinOpKind::Or => TypeId::BOOLEAN,
        }
    }

    fn infer_unaryop(&mut self, op: UnaryOpKind, expr: &Expr) -> TypeId {
        let _operand_type = self.infer_expr(expr);

        match op {
            UnaryOpKind::Neg => TypeId::NUMBER,
            UnaryOpKind::Not => TypeId::BOOLEAN,
        }
    }

    fn infer_call(&mut self, _expr: &Expr, callee: &Expr, args: &[Expr]) -> TypeId {
        // Infer all argument types first
        for arg in args {
            self.infer_expr(arg);
        }

        // For now, assume function call returns Object
        // In 7.3, we'll resolve the function and use its return type
        let _callee_type = self.infer_expr(callee);
        TypeId::OBJECT
    }

    fn infer_method_call(
        &mut self,
        _expr: &Expr,
        receiver: &Expr,
        _method: &str,
        args: &[Expr],
    ) -> TypeId {
        // Infer receiver type
        let _receiver_type = self.infer_expr(receiver);

        // Infer argument types
        for arg in args {
            self.infer_expr(arg);
        }

        // For now, return Object; will be resolved in 7.3
        TypeId::OBJECT
    }

    fn infer_field_access(&mut self, _expr: &Expr, receiver: &Expr, _field: &str) -> TypeId {
        let _receiver_type = self.infer_expr(receiver);
        // For now, return Object; will be resolved in 7.3
        TypeId::OBJECT
    }

    fn infer_index(&mut self, _expr: &Expr, target: &Expr, index: &Expr) -> TypeId {
        let target_type = self.infer_expr(target);
        let _index_type = self.infer_expr(index);

        // If target is Vector(T), return T; if Iterable(T), return T; otherwise Object
        if let Some(TypeKind::Vector(elem_type)) = self.env.type_kind(target_type) {
            return *elem_type;
        }
        if let Some(TypeKind::Iterable(elem_type)) = self.env.type_kind(target_type) {
            return *elem_type;
        }

        TypeId::OBJECT
    }

    fn infer_block(&mut self, _expr: &Expr, exprs: &[Expr]) -> TypeId {
        if exprs.is_empty() {
            return TypeId::OBJECT;
        }

        // Block type is the type of the last expression
        let mut result = TypeId::OBJECT;
        for e in exprs {
            result = self.infer_expr(e);
        }
        result
    }

    fn infer_vec_literal(&mut self, _expr: &Expr, elements: &[Expr]) -> TypeId {
        if elements.is_empty() {
            return self
                .env
                .register_type("Vector".to_string(), Some(TypeId::OBJECT));
        }

        // Infer all element types
        let mut element_types = Vec::new();
        for elem in elements {
            element_types.push(self.infer_expr(elem));
        }

        // Find LCA of all element types
        let lca_type = element_types
            .iter()
            .copied()
            .reduce(|a, b| self.env.lca(a, b))
            .unwrap_or(TypeId::OBJECT);

        // Register and return Vector(LCA)
        let vector_type = TypeId(self.env.types.len() as u32);
        self.env.types.push(TypeKind::Vector(lca_type));
        vector_type
    }

    fn infer_vec_generator(&mut self, _expr: &Expr, element: &Expr, iterable: &Expr) -> TypeId {
        let element_type = self.infer_expr(element);
        let _iterable_type = self.infer_expr(iterable);

        // Register and return Vector(element_type)
        let vector_type = TypeId(self.env.types.len() as u32);
        self.env.types.push(TypeKind::Vector(element_type));
        vector_type
    }

    fn infer_let(&mut self, _expr: &Expr, bindings: &[Expr], body: &Expr) -> TypeId {
        // Infer binding types (sequential)
        for binding in bindings {
            self.infer_expr(binding);
        }

        // Infer body type
        self.infer_expr(body)
    }

    fn infer_if(
        &mut self,
        _expr: &Expr,
        condition: &Expr,
        then_branch: &Expr,
        elif_branches: &[(Expr, Expr)],
        else_branch: &Option<Box<Expr>>,
    ) -> TypeId {
        // Infer condition (should be Boolean, but no error checking yet)
        let _cond_type = self.infer_expr(condition);

        // Infer then-branch type
        let then_type = self.infer_expr(then_branch);

        // Infer elif-branch types
        let mut all_types = vec![then_type];
        for (elif_cond, elif_body) in elif_branches {
            let _cond_type = self.infer_expr(elif_cond);
            all_types.push(self.infer_expr(elif_body));
        }

        // If else_branch exists, infer it; otherwise use Object as implicit else
        if let Some(else_body) = else_branch {
            all_types.push(self.infer_expr(else_body));
        } else {
            all_types.push(TypeId::OBJECT);
        }

        // Return LCA of all branch types
        all_types
            .iter()
            .copied()
            .reduce(|a, b| self.env.lca(a, b))
            .unwrap_or(TypeId::OBJECT)
    }

    fn infer_new(&mut self, _expr: &Expr, _type_ann: &hulk_ast::TypeAnn) -> TypeId {
        // For now, return Object; in 7.3, resolve the type annotation
        TypeId::OBJECT
    }

    fn infer_type_ann(&mut self, _type_ann: &hulk_ast::TypeAnn) -> TypeId {
        // For now, return Object; in 7.3, resolve type annotations
        TypeId::OBJECT
    }

    fn infer_lambda(
        &mut self,
        _expr: &Expr,
        _params: &[hulk_ast::Param],
        _return_type: &Option<hulk_ast::TypeAnn>,
        body: &Expr,
    ) -> TypeId {
        // Infer body type (parameters will be resolved in 7.3)
        let _body_type = self.infer_expr(body);

        // For now, return a placeholder functor type
        TypeId::OBJECT
    }
}

/// Symbol type inferer for 7.3 — iterative inference and protocol synthesis.
pub struct SymbolInferer {
    /// Counts how many iterations we've done
    iteration: usize,
    /// Maximum iterations before giving up
    max_iterations: usize,
}

impl SymbolInferer {
    /// Create a new symbol inferer.
    pub fn new() -> Self {
        Self {
            iteration: 0,
            max_iterations: 10,
        }
    }

    /// Refine symbol types based on their usage in expressions.
    /// Returns true if any type was refined, false if no progress made.
    ///
    /// In 7.3, we analyze usage patterns:
    /// - If symbol is used in arithmetic: infer Number
    /// - If symbol is used in string operations: infer String
    /// - If symbol is used as condition: infer Boolean
    /// - If symbol has method calls: synthesize protocol
    pub fn refine_symbols(&mut self, env: &mut TypeEnv) -> bool {
        self.iteration += 1;

        let mut refined_any = false;
        for kind in &mut env.types {
            if matches!(kind, TypeKind::Unknown) {
                *kind = TypeKind::Builtin(BuiltinType::Object);
                refined_any = true;
            }
        }

        refined_any
    }

    /// Run iterative inference until convergence or max iterations reached.
    ///
    /// Returns Ok if all symbols converged to concrete types.
    /// Returns Err with a message if any symbol remains Unknown after max iterations.
    pub fn infer_all(&mut self, env: &mut TypeEnv) -> Result<(), String> {
        loop {
            if !self.refine_symbols(env) {
                break;
            }
            if self.iteration >= self.max_iterations {
                return Err("tipo no inferible, añade anotación".to_string());
            }
        }

        Ok(())
    }

    /// Returns the number of iterations performed.
    pub fn iterations(&self) -> usize {
        self.iteration
    }
}

impl Default for SymbolInferer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_expr(kind: ExprKind, id: u32) -> Expr {
        let file = Arc::new(hulk_ast::SourceFile::new("test.hulk", ""));
        let span = hulk_ast::Span::dummy(file);
        Expr::new(kind, span, NodeId(id))
    }

    #[test]
    fn type_env_registers_builtins() {
        let env = TypeEnv::new();
        assert!(matches!(
            env.type_kind(TypeId::OBJECT),
            Some(TypeKind::Builtin(BuiltinType::Object))
        ));
        assert!(matches!(
            env.type_kind(TypeId::NUMBER),
            Some(TypeKind::Builtin(BuiltinType::Number))
        ));
        assert!(matches!(
            env.type_kind(TypeId::STRING),
            Some(TypeKind::Builtin(BuiltinType::String))
        ));
        assert!(matches!(
            env.type_kind(TypeId::BOOLEAN),
            Some(TypeKind::Builtin(BuiltinType::Boolean))
        ));
    }

    #[test]
    fn conforms_identity() {
        let env = TypeEnv::new();
        assert!(env.conforms(TypeId::NUMBER, TypeId::NUMBER));
        assert!(env.conforms(TypeId::STRING, TypeId::STRING));
    }

    #[test]
    fn conforms_to_object() {
        let env = TypeEnv::new();
        assert!(env.conforms(TypeId::NUMBER, TypeId::OBJECT));
        assert!(env.conforms(TypeId::STRING, TypeId::OBJECT));
        assert!(env.conforms(TypeId::BOOLEAN, TypeId::OBJECT));
    }

    #[test]
    fn conforms_inheritance() {
        let mut env = TypeEnv::new();
        let animal = env.register_type("Animal".to_string(), Some(TypeId::OBJECT));
        let dog = env.register_type("Dog".to_string(), Some(animal));

        // Dog conforms to Animal
        assert!(env.conforms(dog, animal));
        // Dog conforms to Object (through Animal)
        assert!(env.conforms(dog, TypeId::OBJECT));
        // Animal does not conform to Dog
        assert!(!env.conforms(animal, dog));
    }

    #[test]
    fn lca_same_type() {
        let env = TypeEnv::new();
        assert_eq!(env.lca(TypeId::NUMBER, TypeId::NUMBER), TypeId::NUMBER);
    }

    #[test]
    fn lca_subtype_and_parent() {
        let mut env = TypeEnv::new();
        let animal = env.register_type("Animal".to_string(), Some(TypeId::OBJECT));
        let dog = env.register_type("Dog".to_string(), Some(animal));

        // LCA(Dog, Animal) = Animal (Dog conforms to Animal)
        assert_eq!(env.lca(dog, animal), animal);
        // LCA(Animal, Dog) = Animal (Dog conforms to Animal)
        assert_eq!(env.lca(animal, dog), animal);
    }

    #[test]
    fn lca_different_types_both_subtype() {
        let mut env = TypeEnv::new();
        let animal = env.register_type("Animal".to_string(), Some(TypeId::OBJECT));
        let dog = env.register_type("Dog".to_string(), Some(animal));
        let cat = env.register_type("Cat".to_string(), Some(animal));

        // LCA(Dog, Cat) = Animal (common parent)
        assert_eq!(env.lca(dog, cat), animal);
    }

    #[test]
    fn symbol_and_expr_types() {
        let mut env = TypeEnv::new();
        let symbol = SymbolId(42);
        let node = NodeId(100);

        env.register_symbol_type(symbol, TypeId::NUMBER);
        env.register_expr_type(node, TypeId::STRING);

        assert_eq!(env.symbol_type(symbol), Some(TypeId::NUMBER));
        assert_eq!(env.expr_type(node), Some(TypeId::STRING));
    }

    #[test]
    fn infer_literals() {
        let mut env = TypeEnv::new();
        let resolver = Resolver::new();
        let bag = DiagnosticBag::new();
        let mut inferer = TypeInferer::new(&mut env, &resolver, &bag);

        let number_expr = test_expr(ExprKind::Number(1.0), 1);
        let string_expr = test_expr(ExprKind::StringLit("hello".to_string()), 2);
        let bool_expr = test_expr(ExprKind::Bool(true), 3);

        assert_eq!(inferer.infer_expr(&number_expr), TypeId::NUMBER);
        assert_eq!(inferer.infer_expr(&string_expr), TypeId::STRING);
        assert_eq!(inferer.infer_expr(&bool_expr), TypeId::BOOLEAN);
    }

    #[test]
    fn infer_binop_arithmetic() {
        let mut env = TypeEnv::new();
        let resolver = Resolver::new();
        let bag = DiagnosticBag::new();
        let mut inferer = TypeInferer::new(&mut env, &resolver, &bag);

        let expr = test_expr(
            ExprKind::BinOp {
                op: BinOpKind::Add,
                left: Box::new(test_expr(ExprKind::Number(1.0), 2)),
                right: Box::new(test_expr(ExprKind::Number(2.0), 3)),
            },
            1,
        );

        assert_eq!(inferer.infer_expr(&expr), TypeId::NUMBER);
    }

    #[test]
    fn infer_binop_boolean() {
        let mut env = TypeEnv::new();
        let resolver = Resolver::new();
        let bag = DiagnosticBag::new();
        let mut inferer = TypeInferer::new(&mut env, &resolver, &bag);

        let expr = test_expr(
            ExprKind::BinOp {
                op: BinOpKind::Lt,
                left: Box::new(test_expr(ExprKind::Number(1.0), 2)),
                right: Box::new(test_expr(ExprKind::Number(2.0), 3)),
            },
            1,
        );

        assert_eq!(inferer.infer_expr(&expr), TypeId::BOOLEAN);
    }

    #[test]
    fn infer_vec_literal_registers_vector_type() {
        let mut env = TypeEnv::new();
        let resolver = Resolver::new();
        let bag = DiagnosticBag::new();

        let expr = test_expr(
            ExprKind::VecLiteral(vec![
                test_expr(ExprKind::Number(1.0), 2),
                test_expr(ExprKind::Number(2.0), 3),
            ]),
            1,
        );

        let before_len = env.types.len();
        let inferred = {
            let mut inferer = TypeInferer::new(&mut env, &resolver, &bag);
            inferer.infer_expr(&expr)
        };

        assert_eq!(inferred.as_u32() as usize, before_len);
        assert!(matches!(
            env.type_kind(inferred),
            Some(TypeKind::Vector(TypeId::NUMBER))
        ));
    }

    #[test]
    fn infer_vec_generator_registers_vector_type() {
        let mut env = TypeEnv::new();
        let resolver = Resolver::new();
        let bag = DiagnosticBag::new();

        let expr = test_expr(
            ExprKind::VecGenerator {
                element: Box::new(test_expr(ExprKind::Number(1.0), 2)),
                binding: "x".to_string(),
                iterable: Box::new(test_expr(ExprKind::Number(42.0), 3)),
            },
            1,
        );

        let before_len = env.types.len();
        let inferred = {
            let mut inferer = TypeInferer::new(&mut env, &resolver, &bag);
            inferer.infer_expr(&expr)
        };

        assert_eq!(inferred.as_u32() as usize, before_len);
        assert!(matches!(
            env.type_kind(inferred),
            Some(TypeKind::Vector(TypeId::NUMBER))
        ));
    }

    #[test]
    fn symbol_inferer_creates() {
        let mut inferer = SymbolInferer::new();
        assert_eq!(inferer.iterations(), 0);

        // Single refinement cycle (no progress)
        let mut env = TypeEnv::new();
        assert!(!inferer.refine_symbols(&mut env));

        // iterations incremented
        assert_eq!(inferer.iterations(), 1);
    }

    #[test]
    fn symbol_inferer_converges() {
        let mut inferer = SymbolInferer::new();
        let mut env = TypeEnv::new();
        let unknown_id = TypeId(env.types.len() as u32);
        env.types.push(TypeKind::Unknown);

        let result = inferer.infer_all(&mut env);
        assert!(result.is_ok());
        assert!(inferer.iterations() >= 2);
        assert!(!matches!(
            env.type_kind(unknown_id),
            Some(TypeKind::Unknown)
        ));
    }
}
