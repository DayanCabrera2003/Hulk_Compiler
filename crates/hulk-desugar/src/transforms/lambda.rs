use std::collections::HashSet;

use hulk_hir::{
    BuiltinType, Expr, ExprKind, FunctionDecl, LetBinding, Member, MemberKind, NodeId, Param, Span,
    SymbolKind, TypeAnn, TypeDecl, TypeId, TypeKind,
};

use crate::Desugarer;

impl<'a> Desugarer<'a> {
    /// Lowers a lambda expression into a fresh synthetic type with an
    /// `__invoke` method plus a `new T(captures...)` constructor expression.
    ///
    /// Free variables referenced by the body (variables/parameters from the
    /// enclosing scope) become both fields of the synthetic type and arguments
    /// to its constructor. The body is rewritten so each free-variable Ident
    /// becomes a `self.<name>` field access, and the call site `new T(...)`
    /// forwards the captured values from the outer scope.
    ///
    /// The leading underscores in the method name (`__invoke`) guarantee the
    /// synthesised slot can never collide with a user-defined method called
    /// `invoke` in the global vtable, where method slots are keyed by short
    /// name.
    pub(crate) fn lower_lambda(
        &mut self,
        params: Vec<Param>,
        return_type: Option<TypeAnn>,
        body: Expr,
        span: Span,
        id: NodeId,
    ) -> Expr {
        let type_name = self.fresh_type_name("Lambda");

        // Names that resolve INSIDE the lambda body: its params plus any
        // let-bindings it introduces. Anything else used inside is a free
        // variable to be captured.
        let mut bound: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
        let mut free: Vec<FreeVar> = Vec::new();
        self.collect_free_vars(&body, &mut bound, &mut free);

        if free.is_empty() {
            self.register_generated_type(
                type_name.clone(),
                params,
                return_type,
                body,
                span.clone(),
            );
            return Expr::new(
                ExprKind::New {
                    type_ann: TypeAnn::Named(type_name),
                    args: Vec::new(),
                },
                span,
                id,
            );
        }

        // Rewrite the body so each free-var Ident becomes `self.<name>`.
        let capture_names: HashSet<String> = free.iter().map(|v| v.name.clone()).collect();
        let rewritten_body = self.rewrite_free_vars(body, &capture_names);

        // Build the synthetic type's constructor params and matching attribute
        // initialisers (the constructor stores each captured value into a
        // same-named field).
        let ctor_params: Vec<Param> = free
            .iter()
            .map(|v| Param {
                name: v.name.clone(),
                type_ann: Some(v.type_ann.clone()),
                span: v.span.clone(),
            })
            .collect();

        let attrs: Vec<Member> = free
            .iter()
            .map(|v| Member {
                kind: MemberKind::Attribute {
                    name: v.name.clone(),
                    type_ann: Some(v.type_ann.clone()),
                    value: Expr::new(
                        ExprKind::Ident(v.name.clone()),
                        v.span.clone(),
                        self.node_ids.next_id(),
                    ),
                },
                span: v.span.clone(),
            })
            .collect();

        let invoke_method = FunctionDecl {
            name: "invoke".to_owned(),
            params,
            return_type,
            body: rewritten_body,
            span: span.clone(),
        };
        let mut members: Vec<Member> = attrs;
        members.push(Member {
            kind: MemberKind::Method(invoke_method),
            span: span.clone(),
        });

        let generated = TypeDecl {
            name: type_name.clone(),
            params: ctor_params,
            parent: None,
            members,
            span: span.clone(),
        };
        self.types
            .register_type(type_name.clone(), Some(TypeId::OBJECT));
        self.generated_types.push(generated);

        // Build the `new SyntheticLambda(cap1, cap2, ...)` call site. Each
        // ctor arg gets a freshly minted NodeId; bind it to the captured
        // variable's resolved symbol so the BANNER lowerer (which keys
        // Ident emissions off `expr_symbols`) can find the right local /
        // param when the lambda is constructed.
        let ctor_args: Vec<Expr> = free
            .iter()
            .map(|v| {
                let id = self.node_ids.next_id();
                if let Some(sym) = v.symbol_id {
                    self.resolver.bind_expr_symbol(id, sym);
                }
                Expr::new(ExprKind::Ident(v.name.clone()), v.span.clone(), id)
            })
            .collect();

        Expr::new(
            ExprKind::New {
                type_ann: TypeAnn::Named(type_name),
                args: ctor_args,
            },
            span,
            id,
        )
    }

    /// When a bare function identifier is passed as a call argument, wraps it
    /// in a synthetic functor type so the receiver appears as a value.
    pub(crate) fn wrap_function_argument_if_needed(&mut self, arg: Expr) -> Expr {
        let Some(function_name) = self.function_symbol_name(&arg) else {
            return arg;
        };

        let wrapper_name = if let Some(existing) = self.wrapper_cache.get(&function_name) {
            existing.clone()
        } else {
            let created = self.create_function_wrapper(&function_name, arg.span.clone());
            self.wrapper_cache
                .insert(function_name.clone(), created.clone());
            created
        };

        Expr::new(
            ExprKind::New {
                type_ann: TypeAnn::Named(wrapper_name),
                args: Vec::new(),
            },
            arg.span,
            arg.id,
        )
    }

    /// Synthesises a wrapper type whose `invoke` method forwards to
    /// `function_name`, reusing the original function's parameter list and
    /// return type when available.
    fn create_function_wrapper(&mut self, function_name: &str, span: Span) -> String {
        let wrapper_name = self.fresh_type_name(&format!("Wrapper{function_name}"));
        let signature = self.function_sigs.get(function_name).cloned();

        let (invoke_params, invoke_return_type, call_args) = if let Some(sig) = signature {
            let params = sig
                .params
                .iter()
                .enumerate()
                .map(|(idx, param)| Param {
                    name: format!("__arg_{idx}"),
                    type_ann: param.type_ann.clone(),
                    span: param.span.clone(),
                })
                .collect::<Vec<_>>();
            let args = params
                .iter()
                .map(|param| {
                    Expr::new(
                        ExprKind::Ident(param.name.clone()),
                        span.clone(),
                        self.node_ids.next_id(),
                    )
                })
                .collect::<Vec<_>>();

            (params, sig.return_type.clone(), args)
        } else {
            (Vec::new(), None, Vec::new())
        };

        let invoke_body = Expr::new(
            ExprKind::Call {
                callee: Box::new(Expr::new(
                    ExprKind::Ident(function_name.to_owned()),
                    span.clone(),
                    self.node_ids.next_id(),
                )),
                args: call_args,
            },
            span.clone(),
            self.node_ids.next_id(),
        );

        self.register_generated_type(
            wrapper_name.clone(),
            invoke_params,
            invoke_return_type,
            invoke_body,
            span,
        );

        wrapper_name
    }

    /// Registers a freshly generated type with a single `invoke` method in
    /// both the `TypeEnv` and the `generated_types` accumulator.
    pub(crate) fn register_generated_type(
        &mut self,
        type_name: String,
        params: Vec<Param>,
        return_type: Option<TypeAnn>,
        body: Expr,
        span: Span,
    ) {
        let invoke_method = FunctionDecl {
            name: "invoke".to_owned(),
            params,
            return_type,
            body,
            span: span.clone(),
        };

        let generated = TypeDecl {
            name: type_name.clone(),
            params: Vec::new(),
            parent: None,
            members: vec![Member {
                kind: MemberKind::Method(invoke_method),
                span: span.clone(),
            }],
            span,
        };

        self.types.register_type(type_name, Some(TypeId::OBJECT));
        self.generated_types.push(generated);
    }

    /// If `expr` is an identifier that resolves to a top-level function,
    /// returns the function's name; otherwise `None`.
    fn function_symbol_name(&self, expr: &Expr) -> Option<String> {
        let ExprKind::Ident(name) = &expr.kind else {
            return None;
        };

        let symbol_id = self.resolver.expr_symbol(expr.id)?;
        let symbol = self.resolver.table().get(symbol_id)?;
        match symbol.kind {
            SymbolKind::Function => Some(name.clone()),
            _ => None,
        }
    }

    /// Walk `body` collecting every variable/parameter name referenced that
    /// is not bound by an enclosing let / for / inner lambda within the body.
    /// `bound` tracks names defined so far; updated in place across recursive
    /// scopes. Each free variable is recorded once with its resolved symbol
    /// type so the caller can synthesise a typed field.
    fn collect_free_vars(&self, expr: &Expr, bound: &mut HashSet<String>, free: &mut Vec<FreeVar>) {
        match &expr.kind {
            ExprKind::Number(_) | ExprKind::StringLit(_) | ExprKind::Bool(_) | ExprKind::Self_ => {}
            // `base` parses as `ExprKind::Base`. When the user names a local variable
            // `base`, the resolver binds it to a Variable symbol. In that case it acts
            // as a normal free variable and must be captured by the enclosing lambda.
            ExprKind::Base => {
                let name = "base";
                if bound.contains(name) {
                    return;
                }
                if free.iter().any(|v| v.name == name) {
                    return;
                }
                let Some(symbol_id) = self.resolver.expr_symbol(expr.id) else {
                    return;
                };
                let Some(symbol) = self.resolver.table().get(symbol_id) else {
                    return;
                };
                if !matches!(symbol.kind, SymbolKind::Variable | SymbolKind::Parameter) {
                    return; // actual `base` keyword — not a local variable
                }
                let type_ann = self.symbol_type_ann(symbol_id);
                free.push(FreeVar {
                    name: name.to_owned(),
                    type_ann,
                    span: expr.span.clone(),
                    symbol_id: Some(symbol_id),
                });
            }
            ExprKind::Ident(name) => {
                if bound.contains(name) {
                    return;
                }
                if free.iter().any(|v| v.name == *name) {
                    return;
                }
                // Only Variables and Parameters are captured. Top-level
                // functions, types, protocols and macros are resolved by
                // global lookup at codegen and need no capture.
                let Some(symbol_id) = self.resolver.expr_symbol(expr.id) else {
                    return;
                };
                let Some(symbol) = self.resolver.table().get(symbol_id) else {
                    return;
                };
                if !matches!(symbol.kind, SymbolKind::Variable | SymbolKind::Parameter) {
                    return;
                }
                let type_ann = self.symbol_type_ann(symbol_id);
                free.push(FreeVar {
                    name: name.clone(),
                    type_ann,
                    span: expr.span.clone(),
                    symbol_id: Some(symbol_id),
                });
            }
            ExprKind::BinOp { left, right, .. } => {
                self.collect_free_vars(left, bound, free);
                self.collect_free_vars(right, bound, free);
            }
            ExprKind::UnaryOp { expr: inner, .. } => {
                self.collect_free_vars(inner, bound, free);
            }
            ExprKind::Call { callee, args } => {
                self.collect_free_vars(callee, bound, free);
                for a in args {
                    self.collect_free_vars(a, bound, free);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                self.collect_free_vars(receiver, bound, free);
                for a in args {
                    self.collect_free_vars(a, bound, free);
                }
            }
            ExprKind::FieldAccess { receiver, .. } => {
                self.collect_free_vars(receiver, bound, free);
            }
            ExprKind::Index { target, index } => {
                self.collect_free_vars(target, bound, free);
                self.collect_free_vars(index, bound, free);
            }
            ExprKind::Block(children) | ExprKind::VecLiteral(children) => {
                for child in children {
                    self.collect_free_vars(child, bound, free);
                }
            }
            ExprKind::VecGenerator {
                element,
                binding,
                iterable,
            } => {
                self.collect_free_vars(iterable, bound, free);
                let added = bound.insert(binding.clone());
                self.collect_free_vars(element, bound, free);
                if added {
                    bound.remove(binding);
                }
            }
            ExprKind::Let { bindings, body } => {
                let mut added: Vec<String> = Vec::new();
                for binding in bindings {
                    if let ExprKind::LetBinding(lb) = &binding.kind {
                        self.collect_free_vars(&lb.value, bound, free);
                        if bound.insert(lb.name.clone()) {
                            added.push(lb.name.clone());
                        }
                    }
                }
                self.collect_free_vars(body, bound, free);
                for name in added {
                    bound.remove(&name);
                }
            }
            ExprKind::Assign { target, value } => {
                self.collect_free_vars(target, bound, free);
                self.collect_free_vars(value, bound, free);
            }
            ExprKind::AssignTarget(_) | ExprKind::LetBinding(_) => {}
            ExprKind::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                self.collect_free_vars(condition, bound, free);
                self.collect_free_vars(then_branch, bound, free);
                for (c, b) in elif_branches {
                    self.collect_free_vars(c, bound, free);
                    self.collect_free_vars(b, bound, free);
                }
                if let Some(eb) = else_branch {
                    self.collect_free_vars(eb, bound, free);
                }
            }
            ExprKind::While { condition, body } => {
                self.collect_free_vars(condition, bound, free);
                self.collect_free_vars(body, bound, free);
            }
            ExprKind::For {
                binding,
                iterable,
                body,
            } => {
                self.collect_free_vars(iterable, bound, free);
                let added = bound.insert(binding.clone());
                self.collect_free_vars(body, bound, free);
                if added {
                    bound.remove(binding);
                }
            }
            ExprKind::New { args, .. } => {
                for a in args {
                    self.collect_free_vars(a, bound, free);
                }
            }
            ExprKind::Is { expr: inner, .. } | ExprKind::As { expr: inner, .. } => {
                self.collect_free_vars(inner, bound, free);
            }
            ExprKind::Lambda { params, body, .. } => {
                let added: Vec<String> = params
                    .iter()
                    .filter(|p| bound.insert(p.name.clone()))
                    .map(|p| p.name.clone())
                    .collect();
                self.collect_free_vars(body, bound, free);
                for name in added {
                    bound.remove(&name);
                }
            }
        }
    }

    /// Walk `expr` replacing each Ident whose name is in `captures` with a
    /// `self.<name>` field access. Mirrors the AST shape verbatim everywhere
    /// else so node IDs and spans of unrelated nodes survive untouched.
    fn rewrite_free_vars(&mut self, expr: Expr, captures: &HashSet<String>) -> Expr {
        let Expr { kind, span, id } = expr;
        let new_kind = match kind {
            ExprKind::Ident(name) if captures.contains(&name) => ExprKind::FieldAccess {
                receiver: Box::new(Expr::new(
                    ExprKind::Self_,
                    span.clone(),
                    self.node_ids.next_id(),
                )),
                field: name,
            },
            // `base` parses as ExprKind::Base when used as a variable name. When
            // collected as a free variable (captured), rewrite it to `self.base`
            // so the synthetic lambda type's field is accessed correctly.
            ExprKind::Base if captures.contains("base") => ExprKind::FieldAccess {
                receiver: Box::new(Expr::new(
                    ExprKind::Self_,
                    span.clone(),
                    self.node_ids.next_id(),
                )),
                field: "base".to_owned(),
            },
            ExprKind::BinOp { op, left, right } => ExprKind::BinOp {
                op,
                left: Box::new(self.rewrite_free_vars(*left, captures)),
                right: Box::new(self.rewrite_free_vars(*right, captures)),
            },
            ExprKind::UnaryOp { op, expr: inner } => ExprKind::UnaryOp {
                op,
                expr: Box::new(self.rewrite_free_vars(*inner, captures)),
            },
            ExprKind::Call { callee, args } => ExprKind::Call {
                callee: Box::new(self.rewrite_free_vars(*callee, captures)),
                args: args
                    .into_iter()
                    .map(|a| self.rewrite_free_vars(a, captures))
                    .collect(),
            },
            ExprKind::MethodCall {
                receiver,
                method,
                args,
            } => ExprKind::MethodCall {
                receiver: Box::new(self.rewrite_free_vars(*receiver, captures)),
                method,
                args: args
                    .into_iter()
                    .map(|a| self.rewrite_free_vars(a, captures))
                    .collect(),
            },
            ExprKind::FieldAccess { receiver, field } => ExprKind::FieldAccess {
                receiver: Box::new(self.rewrite_free_vars(*receiver, captures)),
                field,
            },
            ExprKind::Index { target, index } => ExprKind::Index {
                target: Box::new(self.rewrite_free_vars(*target, captures)),
                index: Box::new(self.rewrite_free_vars(*index, captures)),
            },
            ExprKind::Block(children) => ExprKind::Block(
                children
                    .into_iter()
                    .map(|c| self.rewrite_free_vars(c, captures))
                    .collect(),
            ),
            ExprKind::VecLiteral(children) => ExprKind::VecLiteral(
                children
                    .into_iter()
                    .map(|c| self.rewrite_free_vars(c, captures))
                    .collect(),
            ),
            ExprKind::VecGenerator {
                element,
                binding,
                iterable,
            } => ExprKind::VecGenerator {
                element: Box::new(self.rewrite_free_vars(*element, captures)),
                binding,
                iterable: Box::new(self.rewrite_free_vars(*iterable, captures)),
            },
            ExprKind::Let { bindings, body } => ExprKind::Let {
                bindings: bindings
                    .into_iter()
                    .map(|b| self.rewrite_free_vars(b, captures))
                    .collect(),
                body: Box::new(self.rewrite_free_vars(*body, captures)),
            },
            ExprKind::LetBinding(lb) => ExprKind::LetBinding(LetBinding {
                name: lb.name,
                type_ann: lb.type_ann,
                value: Box::new(self.rewrite_free_vars(*lb.value, captures)),
                span: lb.span,
            }),
            ExprKind::Assign { target, value } => ExprKind::Assign {
                target: Box::new(self.rewrite_free_vars(*target, captures)),
                value: Box::new(self.rewrite_free_vars(*value, captures)),
            },
            ExprKind::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => ExprKind::If {
                condition: Box::new(self.rewrite_free_vars(*condition, captures)),
                then_branch: Box::new(self.rewrite_free_vars(*then_branch, captures)),
                elif_branches: elif_branches
                    .into_iter()
                    .map(|(c, b)| {
                        (
                            self.rewrite_free_vars(c, captures),
                            self.rewrite_free_vars(b, captures),
                        )
                    })
                    .collect(),
                else_branch: else_branch.map(|b| Box::new(self.rewrite_free_vars(*b, captures))),
            },
            ExprKind::While { condition, body } => ExprKind::While {
                condition: Box::new(self.rewrite_free_vars(*condition, captures)),
                body: Box::new(self.rewrite_free_vars(*body, captures)),
            },
            ExprKind::For {
                binding,
                iterable,
                body,
            } => ExprKind::For {
                binding,
                iterable: Box::new(self.rewrite_free_vars(*iterable, captures)),
                body: Box::new(self.rewrite_free_vars(*body, captures)),
            },
            ExprKind::New { type_ann, args } => ExprKind::New {
                type_ann,
                args: args
                    .into_iter()
                    .map(|a| self.rewrite_free_vars(a, captures))
                    .collect(),
            },
            ExprKind::Is {
                expr: inner,
                type_ann,
            } => ExprKind::Is {
                expr: Box::new(self.rewrite_free_vars(*inner, captures)),
                type_ann,
            },
            ExprKind::As {
                expr: inner,
                type_ann,
            } => ExprKind::As {
                expr: Box::new(self.rewrite_free_vars(*inner, captures)),
                type_ann,
            },
            // Inner lambdas already had their own captures resolved by a
            // separate `lower_lambda` invocation in the desugar pass, so
            // they are opaque to the outer rewrite.
            // `ExprKind::Base` when it is NOT a captured variable (i.e., it is the
            // genuine `base(...)` method-override call) is also left untouched here.
            kind @ (ExprKind::Number(_)
            | ExprKind::StringLit(_)
            | ExprKind::Bool(_)
            | ExprKind::Ident(_)
            | ExprKind::Self_
            | ExprKind::Base
            | ExprKind::AssignTarget(_)
            | ExprKind::Lambda { .. }) => kind,
        };
        Expr::new(new_kind, span, id)
    }

    /// Best-effort recovery of a TypeAnn from a captured symbol. Falls back
    /// to `Object` when the type cannot be expressed by a single name (e.g.
    /// functor or iterable types we have no syntax for in field annotations).
    fn symbol_type_ann(&self, symbol_id: hulk_hir::SymbolId) -> TypeAnn {
        let Some(type_id) = self.types.symbol_type(symbol_id) else {
            return TypeAnn::Named("Object".to_owned());
        };
        match self.types.type_kind(type_id) {
            Some(TypeKind::Builtin(BuiltinType::Number)) => TypeAnn::Named("Number".to_owned()),
            Some(TypeKind::Builtin(BuiltinType::Boolean)) => TypeAnn::Named("Boolean".to_owned()),
            Some(TypeKind::Builtin(BuiltinType::String)) => TypeAnn::Named("String".to_owned()),
            Some(TypeKind::UserDefined { name, .. }) | Some(TypeKind::Protocol { name }) => {
                TypeAnn::Named(name.clone())
            }
            _ => TypeAnn::Named("Object".to_owned()),
        }
    }

    /// Returns `true` when `callee` is a bound variable or parameter — i.e. a
    /// functor-style call that should become a `.invoke(...)` method call.
    pub(crate) fn should_rewrite_functor_call(&self, callee: &Expr) -> bool {
        match &callee.kind {
            ExprKind::Ident(_) => {
                let Some(symbol_id) = self.resolver.expr_symbol(callee.id) else {
                    return false;
                };
                let Some(symbol) = self.resolver.table().get(symbol_id) else {
                    return false;
                };
                matches!(symbol.kind, SymbolKind::Variable | SymbolKind::Parameter)
            }
            // Chained call like `scaler(2)(5)`: the result of the inner call
            // is a functor object and the outer `(5)` invokes it. Rewrite
            // unconditionally — if the value happens not to expose `invoke`
            // we fail later with a clear "method 'invoke' not found" error
            // rather than the cryptic "indirect call through non-global
            // callee not supported".
            ExprKind::Call { .. } | ExprKind::MethodCall { .. } | ExprKind::FieldAccess { .. } => {
                true
            }
            _ => false,
        }
    }
}

/// One free variable picked up during `collect_free_vars` — used to lift the
/// captured name into both a constructor parameter and an attribute on the
/// synthetic lambda type.
struct FreeVar {
    name: String,
    type_ann: TypeAnn,
    span: Span,
    /// Resolver SymbolId of the captured variable in the enclosing scope.
    /// Used to bind the ctor-arg Ident's NodeId so the BANNER lowerer
    /// resolves it to the correct local / parameter.
    symbol_id: Option<hulk_hir::SymbolId>,
}
