use std::sync::Arc;

use hulk_ast::{Expr, ExprKind, NodeId, Program, SourceFile, Span, Member, MemberKind, Param, TypeDecl};

use crate::Resolver;

use super::common::{call_expr, function_decl, ident_expr};

#[test]
fn resolve_mutual_recursion_between_functions() {
    let source = "function f() => g(); function g() => f();";
    let file = Arc::new(SourceFile::new("mutual.hulk", source));
    let span = Span::new(file, 0, source.len());

    let f_body = call_expr(ident_expr("g", 1, &span), vec![], 2, &span);
    let g_body = call_expr(ident_expr("f", 3, &span), vec![], 4, &span);
    let program = Program {
        functions: vec![
            function_decl("f", f_body, &span),
            function_decl("g", g_body, &span),
        ],
        types: vec![],
        protocols: vec![],
        macros: vec![],
        body: Expr::new(ExprKind::Number(0.0), span.clone(), NodeId(5)),
    };

    let mut resolver = Resolver::new();
    resolver.resolve_program(&program);

    let f_symbol = resolver.lookup("f").expect("f should be registered");
    let g_symbol = resolver.lookup("g").expect("g should be registered");
    assert_eq!(resolver.expr_symbol(NodeId(1)), Some(g_symbol));
    assert_eq!(resolver.expr_symbol(NodeId(3)), Some(f_symbol));
}

#[test]
fn resolve_let_sequentially() {
    use hulk_ast::LetBinding;

    let source = "let a = 1, b = a in b;";
    let file = Arc::new(SourceFile::new("let.hulk", source));
    let span = Span::new(file, 0, source.len());

    let binding_a = Expr::new(
        ExprKind::LetBinding(LetBinding {
            name: "a".to_owned(),
            type_ann: None,
            value: Box::new(Expr::new(ExprKind::Number(1.0), span.clone(), NodeId(10))),
            span: span.clone(),
        }),
        span.clone(),
        NodeId(11),
    );
    let binding_b = Expr::new(
        ExprKind::LetBinding(LetBinding {
            name: "b".to_owned(),
            type_ann: None,
            value: Box::new(ident_expr("a", 12, &span)),
            span: span.clone(),
        }),
        span.clone(),
        NodeId(13),
    );
    let let_expr = Expr::new(
        ExprKind::Let {
            bindings: vec![binding_a, binding_b],
            body: Box::new(ident_expr("b", 14, &span)),
        },
        span.clone(),
        NodeId(15),
    );
    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![],
        body: let_expr,
    };

    let mut resolver = Resolver::new();
    resolver.resolve_program(&program);

    assert!(resolver.has_expr_symbol(NodeId(12)));
    assert!(resolver.has_expr_symbol(NodeId(14)));
}

#[test]
fn resolve_type_members_in_type_scope() {
    let file = Arc::new(SourceFile::new("type.hulk", "type Point(x) { x; }"));
    let span = Span::new(file, 0, 20);

    let type_param = Param {
        name: "x".to_owned(),
        type_ann: None,
        span: span.clone(),
    };
    let attr = Member {
        kind: MemberKind::Attribute {
            name: "value".to_owned(),
            type_ann: None,
            value: ident_expr("x", 20, &span),
        },
        span: span.clone(),
    };
    let type_decl = TypeDecl {
        name: "Point".to_owned(),
        params: vec![type_param],
        parent: None,
        members: vec![attr],
        span: span.clone(),
    };
    let program = Program {
        functions: vec![],
        types: vec![type_decl],
        protocols: vec![],
        macros: vec![],
        body: Expr::new(ExprKind::Number(0.0), span, NodeId(21)),
    };

    let mut resolver = Resolver::new();
    resolver.resolve_program(&program);

    assert!(resolver.has_expr_symbol(NodeId(20)));
}
