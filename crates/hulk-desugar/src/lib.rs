//! HULK desugaring pass.
//!
//! Lowers high-level HULK constructs into a smaller core language so later
//! passes (type check, codegen) only need to deal with the primitive shapes.
//!
//! Session 11.1 transforms:
//! - `for (x in e) body` into explicit `let` + `while` + iterator protocol calls.
//! - `a @@ b` into `a @ " " @ b`.
//! - Lambdas and function-reference arguments into synthetic invocable types.
//!
//! The dispatcher [`Desugarer::desugar_expr`] walks an [`Expr`] tree, leaving
//! already-lowered nodes untouched and delegating the interesting cases to
//! submodule-specific methods (see [`transforms`]).

use std::collections::HashMap;

use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{
    BinOpKind, Expr, ExprKind, Hir, NodeIdGen, Resolver, Span, TypeDecl, TypeEnv,
};

use crate::node_ids::max_node_id_in_program;
use crate::signatures::{collect_function_signatures, FunctionSignature};

mod node_ids;
mod signatures;
mod transforms;

#[cfg(test)]
mod tests;

/// Desugars high-level HULK constructs into their lower-level equivalents.
///
/// Runs the transforms in [`transforms`] across every function body, type
/// member, macro body, and the root program body, then folds any synthetic
/// types produced along the way (lambdas, function wrappers) back into the
/// program.
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

/// Mutable state shared across every transform while desugaring a program.
pub(crate) struct Desugarer<'a> {
    pub(crate) node_ids: NodeIdGen,
    pub(crate) temp_counter: u64,
    pub(crate) type_counter: u64,
    pub(crate) resolver: &'a Resolver,
    pub(crate) types: &'a mut TypeEnv,
    pub(crate) function_sigs: HashMap<String, FunctionSignature>,
    pub(crate) wrapper_cache: HashMap<String, String>,
    pub(crate) generated_types: Vec<TypeDecl>,
}

impl<'a> Desugarer<'a> {
    /// Top-level dispatcher. Recursively desugars `expr`, delegating the
    /// non-trivial arms to transform-specific methods defined in
    /// [`crate::transforms`].
    pub(crate) fn desugar_expr(&mut self, expr: Expr) -> Expr {
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

    /// Allocates a fresh temporary variable name with a monotonic counter,
    /// used as `let` binding names when shaping `for`-loop desugarings.
    pub(crate) fn fresh_temp(&mut self, suffix: &str) -> String {
        let current = self.temp_counter;
        self.temp_counter = self.temp_counter.saturating_add(1);
        format!("__{suffix}_{current}")
    }

    /// Allocates a fresh synthetic type name used when lowering lambdas and
    /// function-reference arguments into concrete `TypeDecl`s.
    pub(crate) fn fresh_type_name(&mut self, suffix: &str) -> String {
        let current = self.type_counter;
        self.type_counter = self.type_counter.saturating_add(1);
        format!("__{suffix}{current}")
    }

    /// Builds an `Ident` expression node with a fresh node id.
    pub(crate) fn ident(&mut self, name: &str, span: &Span) -> Expr {
        Expr::new(
            ExprKind::Ident(name.to_owned()),
            span.clone(),
            self.node_ids.next_id(),
        )
    }

    /// Builds a `receiver.method(args)` expression node with a fresh node id.
    pub(crate) fn method_call(
        &mut self,
        receiver: Expr,
        method: &str,
        args: Vec<Expr>,
        span: &Span,
    ) -> Expr {
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

    /// Builds a bare `LetBinding` expression (not wrapped in `Let`) usable as
    /// an element of `ExprKind::Let::bindings`.
    pub(crate) fn let_binding(&mut self, name: String, value: Expr, span: &Span) -> Expr {
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

    /// Builds a complete `let name = value in body` expression.
    pub(crate) fn let_expr(
        &mut self,
        name: String,
        value: Expr,
        body: Expr,
        span: &Span,
    ) -> Expr {
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
