use hulk_hir::{
    Expr, ExprKind, FunctionDecl, Member, MemberKind, NodeId, Param, Span, SymbolKind, TypeAnn,
    TypeDecl, TypeId,
};

use crate::Desugarer;

impl<'a> Desugarer<'a> {
    /// Lowers a lambda expression into a fresh synthetic type with an
    /// `invoke` method plus a `new T()` constructor expression.
    pub(crate) fn lower_lambda(
        &mut self,
        params: Vec<Param>,
        return_type: Option<TypeAnn>,
        body: Expr,
        span: Span,
        id: NodeId,
    ) -> Expr {
        let type_name = self.fresh_type_name("Lambda");
        self.register_generated_type(type_name.clone(), params, return_type, body, span.clone());

        Expr::new(
            ExprKind::New {
                type_ann: TypeAnn::Named(type_name),
                args: Vec::new(),
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

    /// Returns `true` when `callee` is a bound variable or parameter — i.e. a
    /// functor-style call that should become a `.invoke(...)` method call.
    pub(crate) fn should_rewrite_functor_call(&self, callee: &Expr) -> bool {
        let ExprKind::Ident(_) = &callee.kind else {
            return false;
        };

        let Some(symbol_id) = self.resolver.expr_symbol(callee.id) else {
            return false;
        };

        let Some(symbol) = self.resolver.table().get(symbol_id) else {
            return false;
        };

        matches!(symbol.kind, SymbolKind::Variable | SymbolKind::Parameter)
    }
}
