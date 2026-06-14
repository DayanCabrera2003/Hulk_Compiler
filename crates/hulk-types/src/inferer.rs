use hulk_ast::{BinOpKind, Expr, ExprKind, TypeAnn, UnaryOpKind};
use hulk_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind};
use hulk_semantic::{Resolver, SymbolId};

use crate::env::TypeEnv;
use crate::type_id::{TypeId, TypeKind};

/// Type inferencer for bottom-up inference of expression types.
pub struct TypeInferer<'a> {
    env: &'a mut TypeEnv,
    resolver: &'a Resolver,
    bag: &'a mut DiagnosticBag,
}

impl<'a> TypeInferer<'a> {
    /// Create a new type inferer.
    pub fn new(env: &'a mut TypeEnv, resolver: &'a Resolver, bag: &'a mut DiagnosticBag) -> Self {
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

    /// Register the declared param types of a method `method_name` defined
    /// inside type `type_name`. Looks the method up through the resolver's
    /// `method_symbol` table — without this, references to method params
    /// inside the body collapse to `Object` and codegen produces type-
    /// incorrect IR (e.g. `(self.val + n)` storing into a Number field
    /// when `n: Number` was unknown to the inferer).
    pub fn register_method_params(&mut self, type_name: &str, method_name: &str) {
        let Some(type_id) = self.resolver.lookup(type_name) else {
            return;
        };
        let Some(method_id) = self.resolver.method_symbol(type_id, method_name) else {
            return;
        };
        self.register_function_params(method_id);
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

            // For: validate the iterable, then infer body type.
            ExprKind::For {
                binding: _,
                iterable,
                body,
            } => self.infer_for(expr, iterable, body),

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

    fn infer_call(&mut self, expr: &Expr, callee: &Expr, args: &[Expr]) -> TypeId {
        // Infer all argument types first so the bag also receives diagnostics
        // for malformed sub-expressions, regardless of arity/type errors below.
        let arg_types: Vec<TypeId> = args.iter().map(|a| self.infer_expr(a)).collect();

        // When the callee is a plain identifier we can look the symbol up in
        // the resolver and validate the call against the declared signature.
        if let ExprKind::Ident(name) = &callee.kind {
            if let Some(fn_id) = self.resolver.lookup(name) {
                self.check_call_arity_and_types(expr, name, fn_id, args, &arg_types);
            }
        } else {
            // Higher-order calls: just walk the callee for its side effects.
            let _ = self.infer_expr(callee);
        }

        TypeId::OBJECT
    }

    /// Verifies that a call to `name` matches the declared arity and that
    /// every argument's inferred type is compatible with the parameter's
    /// declared annotation. Records diagnostics on the bag — does not abort.
    fn check_call_arity_and_types(
        &mut self,
        call_expr: &Expr,
        name: &str,
        fn_id: SymbolId,
        args: &[Expr],
        arg_types: &[TypeId],
    ) {
        let Some(anns) = self
            .resolver
            .function_param_annotations(fn_id)
            .map(<[Option<TypeAnn>]>::to_vec)
        else {
            return;
        };

        if args.len() != anns.len() {
            self.bag.push(
                Diagnostic::error(format!(
                    "numero incorrecto de argumentos para '{name}': esperaba {}, recibio {}",
                    anns.len(),
                    args.len()
                ))
                .with_kind(DiagnosticKind::Semantic)
                .with_label(call_expr.span.clone(), "llamada con aridad incorrecta"),
            );
            return;
        }

        for ((arg, arg_ty), ann) in args.iter().zip(arg_types.iter().copied()).zip(anns.iter()) {
            let Some(TypeAnn::Named(expected_name)) = ann else {
                continue;
            };
            let expected = self.resolve_named_type(expected_name);
            if expected == TypeId::OBJECT {
                continue;
            }
            if !self.is_assignable(arg_ty, expected) {
                let got = Self::display_type(self.env, arg_ty);
                let want = Self::display_type(self.env, expected);
                self.bag.push(
                    Diagnostic::error(format!(
                        "tipo incompatible en argumento de '{name}': esperaba {want}, recibio {got}"
                    ))
                    .with_kind(DiagnosticKind::Semantic)
                    .with_label(arg.span.clone(), "tipo incorrecto en argumento"),
                );
            }
        }
    }

    /// Returns true when `actual` can be passed where `expected` is required.
    /// For now we treat OBJECT as a wildcard (anything is assignable to/from
    /// OBJECT) and otherwise require an exact match on the TypeId.
    fn is_assignable(&self, actual: TypeId, expected: TypeId) -> bool {
        if actual == expected {
            return true;
        }
        if actual == TypeId::OBJECT || expected == TypeId::OBJECT {
            return true;
        }
        false
    }

    /// Best-effort human-readable name for a [`TypeId`] used in diagnostics.
    fn display_type(env: &TypeEnv, id: TypeId) -> String {
        if id == TypeId::NUMBER {
            return "Number".to_owned();
        }
        if id == TypeId::STRING {
            return "String".to_owned();
        }
        if id == TypeId::BOOLEAN {
            return "Boolean".to_owned();
        }
        if id == TypeId::OBJECT {
            return "Object".to_owned();
        }
        match env.type_kind(id) {
            Some(TypeKind::UserDefined { name, .. }) | Some(TypeKind::Protocol { name }) => {
                name.clone()
            }
            _ => format!("<type {}>", id.as_u32()),
        }
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
        // Infer condition and verify it is Boolean.
        // Skip the check when the type collapsed to OBJECT (error already
        // emitted by a prior phase) to avoid cascading diagnostics.
        let cond_type = self.infer_expr(condition);
        if cond_type != TypeId::BOOLEAN && cond_type != TypeId::OBJECT {
            let got = Self::display_type(self.env, cond_type);
            self.bag.push(
                Diagnostic::error(format!(
                    "la condición del if debe ser Boolean, se obtuvo {got}"
                ))
                .with_kind(DiagnosticKind::Semantic)
                .with_label(condition.span.clone(), "se esperaba una expresión Boolean"),
            );
        }

        // Infer then-branch type
        let then_type = self.infer_expr(then_branch);

        // Infer elif-branch types
        let mut all_types = vec![then_type];
        for (elif_cond, elif_body) in elif_branches {
            let elif_cond_type = self.infer_expr(elif_cond);
            if elif_cond_type != TypeId::BOOLEAN && elif_cond_type != TypeId::OBJECT {
                let got = Self::display_type(self.env, elif_cond_type);
                self.bag.push(
                    Diagnostic::error(format!(
                        "la condición del elif debe ser Boolean, se obtuvo {got}"
                    ))
                    .with_kind(DiagnosticKind::Semantic)
                    .with_label(elif_cond.span.clone(), "se esperaba una expresión Boolean"),
                );
            }
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

    /// Validates that the iterable expression in `for (x in <iterable>)` has
    /// a type that supports the iteration protocol (Iterable, Enumerable,
    /// Vector, or a user-defined type with `next()`/`current()` / `iter()`).
    ///
    /// Number and Boolean are explicitly not iterable; any other type
    /// that cannot be classified defaults to Object and passes silently to
    /// avoid false positives when type information is unavailable.
    fn infer_for(&mut self, _expr: &Expr, iterable: &Expr, body: &Expr) -> TypeId {
        let iter_ty = self.infer_expr(iterable);
        if !self.is_iterable_type(iter_ty) && iter_ty != TypeId::OBJECT {
            let got = Self::display_type(self.env, iter_ty);
            self.bag.push(
                Diagnostic::error(format!("{got} no es iterable"))
                    .with_kind(DiagnosticKind::Semantic)
                    .with_label(
                        iterable.span.clone(),
                        "se esperaba un tipo iterable (Iterable, Enumerable o Vector)",
                    ),
            );
        }
        self.infer_expr(body)
    }

    /// Returns `true` when `ty` can be used as the source of a `for` loop.
    ///
    /// Accepted types:
    /// - `TypeKind::Vector` — built-in vector type.
    /// - `TypeKind::Iterable` — `T*` annotation.
    /// - Protocols or user-defined types named `Iterable` or `Enumerable`.
    /// - User-defined types that have a `next()` and `current()` method
    ///   (structural Iterable conformance) or an `iter()` method (Enumerable).
    ///
    /// Number, String, and Boolean are **not** iterable.
    fn is_iterable_type(&self, ty: TypeId) -> bool {
        // Builtin Number and Boolean are never iterable.
        if ty == TypeId::NUMBER || ty == TypeId::BOOLEAN {
            return false;
        }
        // String: conservatively rejected (no next()/current() in the runtime).
        if ty == TypeId::STRING {
            return false;
        }
        match self.env.type_kind(ty) {
            Some(TypeKind::Vector(_)) | Some(TypeKind::Iterable(_)) => true,
            Some(TypeKind::Protocol { name }) => name == "Iterable" || name == "Enumerable",
            Some(TypeKind::UserDefined { name, .. }) => {
                let has_next = self.resolver.type_with_name_has_method(name, "next");
                let has_current = self.resolver.type_with_name_has_method(name, "current");
                let has_iter = self.resolver.type_with_name_has_method(name, "iter");
                (has_next && has_current) || has_iter
            }
            _ => false,
        }
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
