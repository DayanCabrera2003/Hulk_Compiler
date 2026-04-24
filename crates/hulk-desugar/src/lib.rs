use hulk_diagnostics::DiagnosticBag;
use hulk_hir::visitor::walk_expr;
use hulk_hir::{
    BinOpKind, Expr, ExprKind, FunctionDecl, Hir, Member, MemberKind, NodeId, NodeIdGen, Param,
    Program, Resolver, Span, SymbolKind, TypeAnn, TypeEnv, TypeId, TypeKind, TypeDecl, Visitor,
};
use std::collections::HashMap;

/// Desugars high-level HULK constructs into their lower-level equivalents.
///
/// Session 11.1 transforms:
/// - `for (x in e) body` into explicit `let` + `while` + iterator protocol calls.
/// - `a @@ b` into `a @ " " @ b`.
#[must_use]
pub fn desugar(hir: Hir, _bag: &mut DiagnosticBag) -> Hir {
    let Hir {
        mut program,
        symbols,
        mut types,
    } = hir;

    let function_sigs = collect_function_signatures(&program.functions);
    let start_id = max_node_id_in_program(&program).saturating_add(1);
    let mut desugarer = Desugarer {
        node_ids: NodeIdGen::with_start(start_id),
        temp_counter: 0,
        type_counter: 0,
        resolver: &symbols,
        types: &mut types,
        function_sigs,
        wrapper_cache: HashMap::new(),
        generated_types: Vec::new(),
    };

    for function in &mut program.functions {
        function.body = desugarer.desugar_expr(function.body.clone());
    }

    for type_decl in &mut program.types {
        for member in &mut type_decl.members {
            match &mut member.kind {
                hulk_hir::MemberKind::Attribute { value, .. } => {
                    *value = desugarer.desugar_expr(value.clone());
                }
                hulk_hir::MemberKind::Method(method) => {
                    method.body = desugarer.desugar_expr(method.body.clone());
                }
            }
        }
    }

    for macro_decl in &mut program.macros {
        macro_decl.body = desugarer.desugar_expr(macro_decl.body.clone());
    }

    program.body = desugarer.desugar_expr(program.body);
    program.types.extend(desugarer.generated_types);

    Hir {
        program,
        symbols,
        types,
    }
}

struct Desugarer<'a> {
    node_ids: NodeIdGen,
    temp_counter: u64,
    type_counter: u64,
    resolver: &'a Resolver,
    types: &'a mut TypeEnv,
    function_sigs: HashMap<String, FunctionSignature>,
    wrapper_cache: HashMap<String, String>,
    generated_types: Vec<TypeDecl>,
}

impl<'a> Desugarer<'a> {
    fn desugar_expr(&mut self, expr: Expr) -> Expr {
        let span = expr.span.clone();
        let id = expr.id;

        match expr.kind {
            ExprKind::Number(_)
            | ExprKind::StringLit(_)
            | ExprKind::Bool(_)
            | ExprKind::Ident(_)
            | ExprKind::Self_
            | ExprKind::Base => expr,
            ExprKind::UnaryOp { op, expr } => Expr::new(
                ExprKind::UnaryOp {
                    op,
                    expr: Box::new(self.desugar_expr(*expr)),
                },
                span,
                id,
            ),
            ExprKind::BinOp { op, left, right } => {
                let left = self.desugar_expr(*left);
                let right = self.desugar_expr(*right);

                if op == BinOpKind::ConcatSpaced {
                    self.desugar_concat_spaced(left, right, span, id)
                } else {
                    Expr::new(
                        ExprKind::BinOp {
                            op,
                            left: Box::new(left),
                            right: Box::new(right),
                        },
                        span,
                        id,
                    )
                }
            }
            ExprKind::Call { callee, args } => {
                let callee_expr = self.desugar_expr(*callee);
                let mut lowered_args = Vec::with_capacity(args.len());
                for arg in args {
                    let lowered = self.desugar_expr(arg);
                    lowered_args.push(self.wrap_function_argument_if_needed(lowered));
                }

                if self.should_rewrite_functor_call(&callee_expr) {
                    Expr::new(
                        ExprKind::MethodCall {
                            receiver: Box::new(callee_expr),
                            method: "invoke".to_owned(),
                            args: lowered_args,
                        },
                        span,
                        id,
                    )
                } else {
                    Expr::new(
                        ExprKind::Call {
                            callee: Box::new(callee_expr),
                            args: lowered_args,
                        },
                        span,
                        id,
                    )
                }
            }
            ExprKind::MethodCall {
                receiver,
                method,
                args,
            } => Expr::new(
                ExprKind::MethodCall {
                    receiver: Box::new(self.desugar_expr(*receiver)),
                    method,
                    args: args.into_iter().map(|arg| self.desugar_expr(arg)).collect(),
                },
                span,
                id,
            ),
            ExprKind::FieldAccess { receiver, field } => Expr::new(
                ExprKind::FieldAccess {
                    receiver: Box::new(self.desugar_expr(*receiver)),
                    field,
                },
                span,
                id,
            ),
            ExprKind::Index { target, index } => Expr::new(
                ExprKind::Index {
                    target: Box::new(self.desugar_expr(*target)),
                    index: Box::new(self.desugar_expr(*index)),
                },
                span,
                id,
            ),
            ExprKind::Block(exprs) => Expr::new(
                ExprKind::Block(
                    exprs
                        .into_iter()
                        .map(|inner| self.desugar_expr(inner))
                        .collect(),
                ),
                span,
                id,
            ),
            ExprKind::VecLiteral(exprs) => Expr::new(
                ExprKind::VecLiteral(
                    exprs
                        .into_iter()
                        .map(|inner| self.desugar_expr(inner))
                        .collect(),
                ),
                span,
                id,
            ),
            ExprKind::VecGenerator {
                element,
                binding,
                iterable,
            } => Expr::new(
                ExprKind::VecGenerator {
                    element: Box::new(self.desugar_expr(*element)),
                    binding,
                    iterable: Box::new(self.desugar_expr(*iterable)),
                },
                span,
                id,
            ),
            ExprKind::Let { bindings, body } => Expr::new(
                ExprKind::Let {
                    bindings: bindings
                        .into_iter()
                        .map(|binding| self.desugar_expr(binding))
                        .collect(),
                    body: Box::new(self.desugar_expr(*body)),
                },
                span,
                id,
            ),
            ExprKind::Assign { target, value } => Expr::new(
                ExprKind::Assign {
                    target: Box::new(self.desugar_expr(*target)),
                    value: Box::new(self.desugar_expr(*value)),
                },
                span,
                id,
            ),
            ExprKind::AssignTarget(target) => Expr::new(ExprKind::AssignTarget(target), span, id),
            ExprKind::LetBinding(mut binding) => {
                binding.value = Box::new(self.desugar_expr(*binding.value));
                Expr::new(ExprKind::LetBinding(binding), span, id)
            }
            ExprKind::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => Expr::new(
                ExprKind::If {
                    condition: Box::new(self.desugar_expr(*condition)),
                    then_branch: Box::new(self.desugar_expr(*then_branch)),
                    elif_branches: elif_branches
                        .into_iter()
                        .map(|(cond, branch)| (self.desugar_expr(cond), self.desugar_expr(branch)))
                        .collect(),
                    else_branch: else_branch.map(|branch| Box::new(self.desugar_expr(*branch))),
                },
                span,
                id,
            ),
            ExprKind::While { condition, body } => Expr::new(
                ExprKind::While {
                    condition: Box::new(self.desugar_expr(*condition)),
                    body: Box::new(self.desugar_expr(*body)),
                },
                span,
                id,
            ),
            ExprKind::For {
                binding,
                iterable,
                body,
            } => {
                let desugared_iterable = self.desugar_expr(*iterable);
                let desugared_body = self.desugar_expr(*body);
                self.desugar_for(binding, desugared_iterable, desugared_body, span, id)
            }
            ExprKind::New { type_ann, args } => Expr::new(
                ExprKind::New {
                    type_ann,
                    args: args.into_iter().map(|arg| self.desugar_expr(arg)).collect(),
                },
                span,
                id,
            ),
            ExprKind::Is { expr, type_ann } => Expr::new(
                ExprKind::Is {
                    expr: Box::new(self.desugar_expr(*expr)),
                    type_ann,
                },
                span,
                id,
            ),
            ExprKind::As { expr, type_ann } => Expr::new(
                ExprKind::As {
                    expr: Box::new(self.desugar_expr(*expr)),
                    type_ann,
                },
                span,
                id,
            ),
            ExprKind::Lambda {
                params,
                return_type,
                body,
            } => {
                let lowered_body = self.desugar_expr(*body);
                self.lower_lambda(params, return_type, lowered_body, span, id)
            }
        }
    }

    fn lower_lambda(
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

    fn wrap_function_argument_if_needed(&mut self, arg: Expr) -> Expr {
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

    fn register_generated_type(
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

        self.types
            .register_type(type_name, Some(TypeId::OBJECT));
        self.generated_types.push(generated);
    }

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

    fn should_rewrite_functor_call(&self, callee: &Expr) -> bool {
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

    fn desugar_concat_spaced(&mut self, left: Expr, right: Expr, span: Span, id: hulk_hir::NodeId) -> Expr {
        let inner = Expr::new(
            ExprKind::BinOp {
                op: BinOpKind::Concat,
                left: Box::new(left),
                right: Box::new(Expr::new(
                    ExprKind::StringLit(" ".to_owned()),
                    span.clone(),
                    self.node_ids.next_id(),
                )),
            },
            span.clone(),
            self.node_ids.next_id(),
        );

        Expr::new(
            ExprKind::BinOp {
                op: BinOpKind::Concat,
                left: Box::new(inner),
                right: Box::new(right),
            },
            span,
            id,
        )
    }

    fn desugar_for(
        &mut self,
        binding: String,
        iterable_expr: Expr,
        body: Expr,
        span: Span,
        id: hulk_hir::NodeId,
    ) -> Expr {
        let strategy = self.for_strategy(iterable_expr.id);
        let iter_name = self.fresh_temp("it");

        if strategy == ForStrategy::Enumerable {
            let enum_name = self.fresh_temp("enum");
            let enum_ident = self.ident(&enum_name, &span);
            let iter_call = self.method_call(enum_ident, "iter", Vec::new(), &span);
            let inner_loop = self.wrap_for_loop(binding, iter_name, iter_call, body, span.clone(), id);
            return self.let_expr(enum_name, iterable_expr, inner_loop, &span);
        }

        self.wrap_for_loop(binding, iter_name, iterable_expr, body, span, id)
    }

    fn wrap_for_loop(
        &mut self,
        binding: String,
        iter_name: String,
        iter_source: Expr,
        body: Expr,
        span: Span,
        id: hulk_hir::NodeId,
    ) -> Expr {
        let iter_ident_for_next = self.ident(&iter_name, &span);
        let iter_ident_for_current = self.ident(&iter_name, &span);

        let next_call = self.method_call(iter_ident_for_next, "next", Vec::new(), &span);
        let current_call = self.method_call(iter_ident_for_current, "current", Vec::new(), &span);

        let body_with_binding = self.let_expr(binding, current_call, body, &span);

        let while_expr = Expr::new(
            ExprKind::While {
                condition: Box::new(next_call),
                body: Box::new(body_with_binding),
            },
            span.clone(),
            self.node_ids.next_id(),
        );

        Expr::new(
            ExprKind::Let {
                bindings: vec![self.let_binding(iter_name, iter_source, &span)],
                body: Box::new(while_expr),
            },
            span,
            id,
        )
    }

    fn for_strategy(&self, iterable_node: hulk_hir::NodeId) -> ForStrategy {
        let Some(ty) = self.types.expr_type(iterable_node) else {
            return ForStrategy::Iterable;
        };

        let Some(kind) = self.types.type_kind(ty) else {
            return ForStrategy::Iterable;
        };

        if is_enumerable_kind(kind) {
            ForStrategy::Enumerable
        } else {
            ForStrategy::Iterable
        }
    }

    fn fresh_temp(&mut self, suffix: &str) -> String {
        let current = self.temp_counter;
        self.temp_counter = self.temp_counter.saturating_add(1);
        format!("__{suffix}_{current}")
    }

    fn fresh_type_name(&mut self, suffix: &str) -> String {
        let current = self.type_counter;
        self.type_counter = self.type_counter.saturating_add(1);
        format!("__{suffix}{current}")
    }

    fn ident(&mut self, name: &str, span: &Span) -> Expr {
        Expr::new(
            ExprKind::Ident(name.to_owned()),
            span.clone(),
            self.node_ids.next_id(),
        )
    }

    fn method_call(&mut self, receiver: Expr, method: &str, args: Vec<Expr>, span: &Span) -> Expr {
        Expr::new(
            ExprKind::MethodCall {
                receiver: Box::new(receiver),
                method: method.to_owned(),
                args,
            },
            span.clone(),
            self.node_ids.next_id(),
        )
    }

    fn let_binding(&mut self, name: String, value: Expr, span: &Span) -> Expr {
        Expr::new(
            ExprKind::LetBinding(hulk_hir::LetBinding {
                name,
                type_ann: None,
                value: Box::new(value),
                span: span.clone(),
            }),
            span.clone(),
            self.node_ids.next_id(),
        )
    }

    fn let_expr(&mut self, name: String, value: Expr, body: Expr, span: &Span) -> Expr {
        Expr::new(
            ExprKind::Let {
                bindings: vec![self.let_binding(name, value, span)],
                body: Box::new(body),
            },
            span.clone(),
            self.node_ids.next_id(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForStrategy {
    Iterable,
    Enumerable,
}

#[derive(Clone)]
struct FunctionSignature {
    params: Vec<Param>,
    return_type: Option<TypeAnn>,
}

fn collect_function_signatures(functions: &[FunctionDecl]) -> HashMap<String, FunctionSignature> {
    let mut signatures = HashMap::new();
    for function in functions {
        signatures.insert(
            function.name.clone(),
            FunctionSignature {
                params: function.params.clone(),
                return_type: function.return_type.clone(),
            },
        );
    }
    signatures
}

fn is_enumerable_kind(kind: &TypeKind) -> bool {
    match kind {
        TypeKind::Protocol { name } | TypeKind::UserDefined { name, .. } => name == "Enumerable",
        _ => false,
    }
}

fn max_node_id_in_program(program: &Program) -> u32 {
    let mut max_id = 0u32;

    for function in &program.functions {
        visit_max_node_id(&function.body, &mut max_id);
    }

    for type_decl in &program.types {
        for member in &type_decl.members {
            match &member.kind {
                hulk_hir::MemberKind::Attribute { value, .. } => visit_max_node_id(value, &mut max_id),
                hulk_hir::MemberKind::Method(method) => visit_max_node_id(&method.body, &mut max_id),
            }
        }
    }

    for macro_decl in &program.macros {
        visit_max_node_id(&macro_decl.body, &mut max_id);
    }

    visit_max_node_id(&program.body, &mut max_id);
    max_id
}

struct MaxNodeId {
    max: u32,
}

impl Visitor for MaxNodeId {
    fn visit_expr(&mut self, expr: &Expr) {
        self.max = self.max.max(expr.id.0);
        walk_expr(self, expr);
    }
}

fn visit_max_node_id(expr: &Expr, max_id: &mut u32) {
    let mut v = MaxNodeId { max: *max_id };
    v.visit_expr(expr);
    *max_id = v.max;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use hulk_hir::{Program, SourceFile, TypeEnv, TypeId, TypedAst};

    use super::*;

    #[test]
    fn desugars_concat_spaced_into_two_concat_ops() {
        let source = Arc::new(SourceFile::new("desugar.hulk", "\"a\" @@ \"b\""));
        let span = Span::new(source, 0, 10);
        let mut ids = NodeIdGen::new();

        let body = Expr::new(
            ExprKind::BinOp {
                op: BinOpKind::ConcatSpaced,
                left: Box::new(Expr::new(
                    ExprKind::StringLit("a".to_owned()),
                    span.clone(),
                    ids.next_id(),
                )),
                right: Box::new(Expr::new(
                    ExprKind::StringLit("b".to_owned()),
                    span.clone(),
                    ids.next_id(),
                )),
            },
            span.clone(),
            ids.next_id(),
        );

        let hir = make_hir(body);
        let mut bag = DiagnosticBag::new();
        let transformed = desugar(hir, &mut bag);

        match transformed.program.body.kind {
            ExprKind::BinOp {
                op: BinOpKind::Concat,
                left,
                right,
            } => {
                assert!(matches!(right.kind, ExprKind::StringLit(ref s) if s == "b"));
                match left.kind {
                    ExprKind::BinOp {
                        op: BinOpKind::Concat,
                        left: inner_left,
                        right: inner_right,
                    } => {
                        assert!(matches!(inner_left.kind, ExprKind::StringLit(ref s) if s == "a"));
                        assert!(matches!(inner_right.kind, ExprKind::StringLit(ref s) if s == " "));
                    }
                    _ => panic!("expected nested concat in left branch"),
                }
            }
            _ => panic!("expected concat expression after desugar"),
        }
    }

    #[test]
    fn desugars_for_with_iterable_to_let_while_shape() {
        let source = Arc::new(SourceFile::new("desugar.hulk", "for (x in xs) print(x);"));
        let span = Span::new(source, 0, 22);
        let mut ids = NodeIdGen::new();

        let iterable = Expr::new(ExprKind::Ident("xs".to_owned()), span.clone(), ids.next_id());
        let iterable_id = iterable.id;

        let body = Expr::new(
            ExprKind::For {
                binding: "x".to_owned(),
                iterable: Box::new(iterable),
                body: Box::new(call_print("x", &span, &mut ids)),
            },
            span.clone(),
            ids.next_id(),
        );

        let mut hir = make_hir(body);
        let iterable_ty = hir.types.register_protocol("Iterable".to_owned());
        hir.types.register_expr_type(iterable_id, iterable_ty);

        let mut bag = DiagnosticBag::new();
        let transformed = desugar(hir, &mut bag);

        assert_for_let_while_shape(&transformed.program.body, false);
    }

    #[test]
    fn desugars_for_with_enumerable_to_enum_iter_then_while() {
        let source = Arc::new(SourceFile::new("desugar.hulk", "for (x in values) print(x);"));
        let span = Span::new(source, 0, 26);
        let mut ids = NodeIdGen::new();

        let iterable = Expr::new(
            ExprKind::Ident("values".to_owned()),
            span.clone(),
            ids.next_id(),
        );
        let iterable_id = iterable.id;

        let body = Expr::new(
            ExprKind::For {
                binding: "x".to_owned(),
                iterable: Box::new(iterable),
                body: Box::new(call_print("x", &span, &mut ids)),
            },
            span.clone(),
            ids.next_id(),
        );

        let mut hir = make_hir(body);
        let enumerable_ty = hir.types.register_protocol("Enumerable".to_owned());
        hir.types.register_expr_type(iterable_id, enumerable_ty);

        let mut bag = DiagnosticBag::new();
        let transformed = desugar(hir, &mut bag);

        assert_for_let_while_shape(&transformed.program.body, true);
    }

    #[test]
    fn lowers_lambda_into_synthetic_type_and_new() {
        let source = Arc::new(SourceFile::new("desugar.hulk", "(x) => x + 1"));
        let span = Span::new(source, 0, 12);
        let mut ids = NodeIdGen::new();

        let lambda = Expr::new(
            ExprKind::Lambda {
                params: vec![Param {
                    name: "x".to_owned(),
                    type_ann: Some(TypeAnn::Named("Number".to_owned())),
                    span: span.clone(),
                }],
                return_type: Some(TypeAnn::Named("Number".to_owned())),
                body: Box::new(Expr::new(
                    ExprKind::BinOp {
                        op: BinOpKind::Add,
                        left: Box::new(Expr::new(
                            ExprKind::Ident("x".to_owned()),
                            span.clone(),
                            ids.next_id(),
                        )),
                        right: Box::new(Expr::new(
                            ExprKind::Number(1.0),
                            span.clone(),
                            ids.next_id(),
                        )),
                    },
                    span.clone(),
                    ids.next_id(),
                )),
            },
            span.clone(),
            ids.next_id(),
        );

        let hir = make_hir(lambda);
        let mut bag = DiagnosticBag::new();
        let transformed = desugar(hir, &mut bag);

        let generated_name = match &transformed.program.body.kind {
            ExprKind::New {
                type_ann: TypeAnn::Named(name),
                args,
            } => {
                assert!(args.is_empty());
                name.clone()
            }
            _ => panic!("expected lambda to be replaced by new synthetic type"),
        };

        let generated = transformed
            .program
            .types
            .iter()
            .find(|decl| decl.name == generated_name)
            .expect("expected generated lambda type declaration");

        assert_eq!(generated.members.len(), 1);
        match &generated.members[0].kind {
            MemberKind::Method(method) => {
                assert_eq!(method.name, "invoke");
                assert_eq!(method.params.len(), 1);
                assert_eq!(method.params[0].name, "x");
                assert!(matches!(
                    method.body.kind,
                    ExprKind::BinOp {
                        op: BinOpKind::Add,
                        ..
                    }
                ));
            }
            _ => panic!("expected invoke method in synthetic lambda type"),
        }
    }

    #[test]
    fn wraps_function_arguments_with_synthetic_wrapper_type() {
        let source = Arc::new(SourceFile::new("desugar.hulk", "apply(inc, 1);"));
        let span = Span::new(source, 0, 13);
        let mut ids = NodeIdGen::new();

        let inc_function = FunctionDecl {
            name: "inc".to_owned(),
            params: vec![Param {
                name: "x".to_owned(),
                type_ann: Some(TypeAnn::Named("Number".to_owned())),
                span: span.clone(),
            }],
            return_type: Some(TypeAnn::Named("Number".to_owned())),
            body: Expr::new(ExprKind::Ident("x".to_owned()), span.clone(), ids.next_id()),
            span: span.clone(),
        };

        let apply_call = Expr::new(
            ExprKind::Call {
                callee: Box::new(Expr::new(
                    ExprKind::Ident("apply".to_owned()),
                    span.clone(),
                    ids.next_id(),
                )),
                args: vec![
                    Expr::new(
                        ExprKind::Ident("inc".to_owned()),
                        span.clone(),
                        ids.next_id(),
                    ),
                    Expr::new(ExprKind::Number(1.0), span.clone(), ids.next_id()),
                ],
            },
            span.clone(),
            ids.next_id(),
        );

        let program = Program {
            functions: vec![inc_function],
            types: vec![],
            protocols: vec![],
            macros: vec![],
            body: apply_call,
        };

        let hir = make_hir_from_program(program);
        let mut bag = DiagnosticBag::new();
        let transformed = desugar(hir, &mut bag);

        let wrapper_name = match &transformed.program.body.kind {
            ExprKind::Call { args, .. } => match &args[0].kind {
                ExprKind::New {
                    type_ann: TypeAnn::Named(name),
                    args,
                } => {
                    assert!(args.is_empty());
                    name.clone()
                }
                _ => panic!("expected first argument to be wrapper constructor"),
            },
            _ => panic!("expected call to remain call after wrapper injection"),
        };

        let wrapper = transformed
            .program
            .types
            .iter()
            .find(|decl| decl.name == wrapper_name)
            .expect("expected generated wrapper type");

        match &wrapper.members[0].kind {
            MemberKind::Method(method) => {
                assert_eq!(method.name, "invoke");
                assert_eq!(method.params.len(), 1);
                assert_eq!(method.params[0].name, "__arg_0");
                assert!(matches!(
                    method.body.kind,
                    ExprKind::Call { ref callee, .. }
                        if matches!(callee.kind, ExprKind::Ident(ref name) if name == "inc")
                ));
            }
            _ => panic!("expected wrapper to expose invoke method"),
        }
    }

    #[test]
    fn rewrites_functor_style_call_to_invoke_method_call() {
        let source = Arc::new(SourceFile::new("desugar.hulk", "filter(x);"));
        let span = Span::new(source, 0, 10);
        let mut ids = NodeIdGen::new();

        let apply_functor = FunctionDecl {
            name: "apply_functor".to_owned(),
            params: vec![
                Param {
                    name: "filter".to_owned(),
                    type_ann: Some(TypeAnn::Named("Object".to_owned())),
                    span: span.clone(),
                },
                Param {
                    name: "x".to_owned(),
                    type_ann: Some(TypeAnn::Named("Number".to_owned())),
                    span: span.clone(),
                },
            ],
            return_type: Some(TypeAnn::Named("Object".to_owned())),
            body: Expr::new(
                ExprKind::Call {
                    callee: Box::new(Expr::new(
                        ExprKind::Ident("filter".to_owned()),
                        span.clone(),
                        ids.next_id(),
                    )),
                    args: vec![Expr::new(
                        ExprKind::Ident("x".to_owned()),
                        span.clone(),
                        ids.next_id(),
                    )],
                },
                span.clone(),
                ids.next_id(),
            ),
            span: span.clone(),
        };

        let program = Program {
            functions: vec![apply_functor],
            types: vec![],
            protocols: vec![],
            macros: vec![],
            body: Expr::new(ExprKind::Number(0.0), span.clone(), ids.next_id()),
        };

        let hir = make_hir_from_program(program);
        let mut bag = DiagnosticBag::new();
        let transformed = desugar(hir, &mut bag);

        let fun = transformed
            .program
            .functions
            .iter()
            .find(|f| f.name == "apply_functor")
            .expect("expected apply_functor function");

        assert!(matches!(
            fun.body.kind,
            ExprKind::MethodCall {
                ref method,
                ref receiver,
                ..
            }
                if method == "invoke"
                    && matches!(receiver.kind, ExprKind::Ident(ref name) if name == "filter")
        ));
    }

    fn assert_for_let_while_shape(expr: &Expr, expect_enumerable: bool) {
        let ExprKind::Let { bindings, body } = &expr.kind else {
            panic!("expected outer let");
        };

        assert_eq!(bindings.len(), 1);
        if expect_enumerable {
            let ExprKind::LetBinding(first_binding) = &bindings[0].kind else {
                panic!("expected first let binding");
            };
            assert!(first_binding.name.starts_with("__enum_"));

            let ExprKind::Let {
                bindings: inner_bindings,
                body: inner_body,
            } = &body.kind
            else {
                panic!("expected inner let for enumerable iter");
            };

            assert_eq!(inner_bindings.len(), 1);
            let ExprKind::LetBinding(iter_binding) = &inner_bindings[0].kind else {
                panic!("expected iter binding");
            };
            assert!(iter_binding.name.starts_with("__it_"));
            assert!(matches!(
                iter_binding.value.kind,
                ExprKind::MethodCall { ref method, .. } if method == "iter"
            ));

            assert_while_shape(inner_body, &iter_binding.name);
        } else {
            let ExprKind::LetBinding(iter_binding) = &bindings[0].kind else {
                panic!("expected iter binding");
            };
            assert!(iter_binding.name.starts_with("__it_"));
            assert_while_shape(body, &iter_binding.name);
        }
    }

    fn assert_while_shape(expr: &Expr, iter_name: &str) {
        let ExprKind::While { condition, body } = &expr.kind else {
            panic!("expected while expression");
        };

        assert!(matches!(
            condition.kind,
            ExprKind::MethodCall { ref method, ref receiver, .. }
                if method == "next"
                    && matches!(receiver.kind, ExprKind::Ident(ref name) if name == iter_name)
        ));

        let ExprKind::Let {
            bindings,
            body: loop_body,
        } = &body.kind
        else {
            panic!("expected let binding for loop variable");
        };

        assert_eq!(bindings.len(), 1);
        let ExprKind::LetBinding(binding) = &bindings[0].kind else {
            panic!("expected let binding in while body");
        };
        assert_eq!(binding.name, "x");
        assert!(matches!(
            binding.value.kind,
            ExprKind::MethodCall { ref method, ref receiver, .. }
                if method == "current"
                    && matches!(receiver.kind, ExprKind::Ident(ref name) if name == iter_name)
        ));
        assert!(matches!(
            loop_body.kind,
            ExprKind::Call { ref callee, .. }
                if matches!(callee.kind, ExprKind::Ident(ref name) if name == "print")
        ));
    }

    fn call_print(name: &str, span: &Span, ids: &mut NodeIdGen) -> Expr {
        Expr::new(
            ExprKind::Call {
                callee: Box::new(Expr::new(
                    ExprKind::Ident("print".to_owned()),
                    span.clone(),
                    ids.next_id(),
                )),
                args: vec![Expr::new(
                    ExprKind::Ident(name.to_owned()),
                    span.clone(),
                    ids.next_id(),
                )],
            },
            span.clone(),
            ids.next_id(),
        )
    }

    fn make_hir(body: Expr) -> Hir {
        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![],
            body,
        };

        make_hir_from_program(program)
    }

    fn make_hir_from_program(program: Program) -> Hir {

        let mut symbols = hulk_hir::Resolver::new();
        symbols.resolve_program(&program);

        let mut types = TypeEnv::new();
        types.register_symbol_type(hulk_hir::SymbolId(0), TypeId::OBJECT);

        Hir::from_typed(TypedAst {
            program,
            symbols,
            types,
        })
    }
}
