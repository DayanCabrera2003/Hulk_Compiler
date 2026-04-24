use std::sync::Arc;

use hulk_ast::{Expr, ExprKind, FunctionDecl, NodeId, SourceFile, Span};
use hulk_diagnostics::Severity;

use crate::Resolver;

pub(super) fn test_span() -> Span {
    let file = Arc::new(SourceFile::new("test.hulk", "x"));
    Span::new(file, 0, 1)
}

pub(super) fn ident_expr(name: &str, id: u32, span: &Span) -> Expr {
    Expr::new(ExprKind::Ident(name.to_owned()), span.clone(), NodeId(id))
}

pub(super) fn call_expr(callee: Expr, args: Vec<Expr>, id: u32, span: &Span) -> Expr {
    Expr::new(
        ExprKind::Call {
            callee: Box::new(callee),
            args,
        },
        span.clone(),
        NodeId(id),
    )
}

pub(super) fn function_decl(name: &str, body: Expr, span: &Span) -> FunctionDecl {
    FunctionDecl {
        name: name.to_owned(),
        params: vec![],
        return_type: None,
        body,
        span: span.clone(),
    }
}

pub(super) fn diagnostic_messages(resolver: &Resolver) -> Vec<String> {
    resolver
        .diagnostics()
        .diagnostics()
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .map(|d| d.message.clone())
        .collect()
}

pub(super) fn run_program(body: Expr) -> Resolver {
    let source = "test";
    let file = Arc::new(SourceFile::new("test.hulk", source));
    let span = Span::new(file, 0, source.len());
    let program = hulk_ast::Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![],
        body: Expr::new(body.kind, span, body.id),
    };

    let mut resolver = Resolver::new();
    resolver.resolve_program(&program);
    resolver
}
