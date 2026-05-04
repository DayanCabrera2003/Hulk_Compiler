#![allow(dead_code)]

use std::sync::Arc;

use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{Expr, ExprKind, Hir, Program, Resolver, SourceFile, Span, TypeEnv, TypedAst};

pub fn source_span(content: &'static str) -> (Arc<SourceFile>, Span) {
    let src = Arc::new(SourceFile::new("test.hulk", content));
    let len = content.len();
    let span = Span::new(Arc::clone(&src), 0, len);
    (src, span)
}

pub fn make_hir_from_program(program: Program) -> Hir {
    let mut symbols = Resolver::new();
    symbols.resolve_program(&program);
    Hir::from_typed(TypedAst {
        program,
        symbols,
        types: TypeEnv::new(),
    })
}

pub fn run_expand(hir: Hir) -> (Hir, DiagnosticBag) {
    let mut bag = DiagnosticBag::new();
    let result = hulk_macros::expand_macros(hir, &mut bag);
    (result, bag)
}

/// Collects all identifier strings reachable from `expr`.
pub fn collect_all_idents(expr: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    collect_idents_rec(expr, &mut out);
    out
}

fn collect_idents_rec(expr: &Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Ident(name) => out.push(name.clone()),
        ExprKind::BinOp { left, right, .. } => {
            collect_idents_rec(left, out);
            collect_idents_rec(right, out);
        }
        ExprKind::UnaryOp { expr, .. } => collect_idents_rec(expr, out),
        ExprKind::Call { callee, args } => {
            collect_idents_rec(callee, out);
            for a in args {
                collect_idents_rec(a, out);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            collect_idents_rec(receiver, out);
            for a in args {
                collect_idents_rec(a, out);
            }
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
            for e in exprs {
                collect_idents_rec(e, out);
            }
        }
        ExprKind::Let { bindings, body } => {
            for b in bindings {
                collect_idents_rec(b, out);
            }
            collect_idents_rec(body, out);
        }
        ExprKind::LetBinding(lb) => collect_idents_rec(&lb.value, out),
        ExprKind::While { condition, body } => {
            collect_idents_rec(condition, out);
            collect_idents_rec(body, out);
        }
        ExprKind::For { iterable, body, .. } => {
            collect_idents_rec(iterable, out);
            collect_idents_rec(body, out);
        }
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            collect_idents_rec(condition, out);
            collect_idents_rec(then_branch, out);
            for (c, b) in elif_branches {
                collect_idents_rec(c, out);
                collect_idents_rec(b, out);
            }
            if let Some(e) = else_branch {
                collect_idents_rec(e, out);
            }
        }
        ExprKind::New { args, .. } => {
            for a in args {
                collect_idents_rec(a, out);
            }
        }
        ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => collect_idents_rec(expr, out),
        ExprKind::Assign { target, value } => {
            collect_idents_rec(target, out);
            collect_idents_rec(value, out);
        }
        ExprKind::Lambda { body, .. } => collect_idents_rec(body, out),
        ExprKind::VecGenerator {
            element, iterable, ..
        } => {
            collect_idents_rec(element, out);
            collect_idents_rec(iterable, out);
        }
        _ => {}
    }
}

/// Returns `true` if any identifier in the program body starts with the given prefix.
pub fn any_ident_starts_with(hir: &Hir, prefix: &str) -> bool {
    collect_all_idents(&hir.program.body)
        .iter()
        .any(|name| name.starts_with(prefix))
}
