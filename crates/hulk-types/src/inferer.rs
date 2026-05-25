use hulk_ast::{BinOpKind, Expr, ExprKind, TypeAnn, UnaryOpKind};
use hulk_diagnostics::DiagnosticBag;
use hulk_semantic::{Resolver, SymbolId};

use crate::env::TypeEnv;
use crate::type_id::{TypeId, TypeKind};

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

    /// Register the declared types of a function's parameters so that
    /// subsequent identifier lookups inside the body see the correct type.
    /// Without this, every param defaults to Object and codegen fails to
    /// coerce numeric args (e.g. for `print(s @ n)` inside a function).
    pub fn register_function_params_by_name(&mut self, name: &str) {
        let Some(fn_id) = self.resolver.lookup(name) else {
            return;
        };
        self.register_function_params(fn_id);
    }

    /// Register a user-defined type by name (idempotent). No-op when the
    /// type is already in the env so the driver can call this freely on
    /// every program type.
    pub fn register_user_type(&mut self, name: &str) {
        if self.env.type_id_by_name(name).is_none() {
            self.env.register_type(name.to_owned(), None);
        }
    }

    /// Register a protocol type by name (idempotent).
    pub fn register_protocol(&mut self, name: &str) {
        if self.env.type_id_by_name(name).is_none() {
            self.env.register_protocol(name.to_owned());
        }
    }

    /// Same as [`register_function_params_by_name`] but takes a SymbolId.
    pub fn register_function_params(&mut self, function_id: SymbolId) {
        let syms = self
            .resolver
            .function_param_symbols(function_id)
            .map(<[SymbolId]>::to_vec);
        let anns = self
            .resolver
            .function_param_annotations(function_id)
            .map(<[Option<TypeAnn>]>::to_vec);
        let (Some(syms), Some(anns)) = (syms, anns) else {
            return;
        };
        for (sym, ann) in syms.iter().zip(anns.iter()) {
            let ty = match ann {
                Some(TypeAnn::Named(n)) => self.resolve_named_type(n),
                _ => continue,
            };
            self.env.register_symbol_type(*sym, ty);
        }
    }

    fn resolve_named_type(&self, name: &str) -> TypeId {
        match name {
            "Number" => TypeId::NUMBER,
            "String" => TypeId::STRING,
            "Boolean" => TypeId::BOOLEAN,
            "Object" => TypeId::OBJECT,
            // For user-defined types we don't have a fast lookup; fall back
            // to OBJECT — codegen handles user types as ptr anyway.
            _ => TypeId::OBJECT,
        }
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

            // AssignTarget carries no type itself; leave it as Object.
            ExprKind::AssignTarget(_) => TypeId::OBJECT,

            // LetBinding must recurse into the value so that the value's NodeId
            // is registered in the type env. Without this, the lowerer cannot
            // retrieve the value's type via hir.expr_type(lb.value.id) and
            // falls back to TypeId::OBJECT, incorrectly treating numeric
            // bindings as reference types and emitting spurious ShadowPush.
            //
            // Also propagate the inferred value type to the binding's symbol
            // so subsequent Ident references (e.g. `t @ n` where `n` was
            // bound to a Number) resolve to the right type.
            ExprKind::LetBinding(lb) => {
                let value_ty = self.infer_expr(&lb.value);
                if let Some(symbol_id) = self.resolver.expr_symbol(expr.id) {
                    self.env.register_symbol_type(symbol_id, value_ty);
                }
                value_ty
            }
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

    fn infer_new(&mut self, _expr: &Expr, type_ann: &hulk_ast::TypeAnn) -> TypeId {
        if let hulk_ast::TypeAnn::Named(name) = type_ann {
            if let Some(id) = self.env.type_id_by_name(name) {
                return id;
            }
        }
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
