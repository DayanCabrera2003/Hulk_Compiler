use std::sync::Arc;

use hulk_ast::{
    AssignTarget, Expr, ExprKind, FunctionDecl, LetBinding, Member, MemberKind, NodeId, Param,
    ParentSpec, Program, SourceFile, Span, TypeAnn, TypeDecl,
};
use hulk_diagnostics::Severity;

use crate::{Resolver, SymbolKind, SymbolTable};

fn test_span() -> Span {
    let file = Arc::new(SourceFile::new("test.hulk", "x"));
    Span::new(file, 0, 1)
}

#[test]
fn symbol_table_add_get_and_name_of_work() {
    let mut table = SymbolTable::new();
    let span = test_span();
    let id = table.add("x", SymbolKind::Variable, span.clone());

    let Some(symbol) = table.get(id) else {
        panic!("symbol should exist");
    };
    assert_eq!(symbol.id, id);
    assert_eq!(symbol.name, "x");
    assert_eq!(symbol.kind, SymbolKind::Variable);
    assert_eq!(symbol.span, span);
    assert_eq!(table.name_of(id), Some("x"));
}

#[test]
fn resolver_push_and_pop_scopes() {
    let mut resolver = Resolver::new();
    let root_len = resolver.scopes.len();

    resolver.push_scope();
    assert_eq!(resolver.scopes.len(), root_len + 1);
    assert!(resolver.pop_scope().is_some());
    assert_eq!(resolver.scopes.len(), root_len);
    assert!(resolver.pop_scope().is_none());
}

#[test]
fn resolver_lookup_finds_local_and_outer_bindings() {
    let mut resolver = Resolver::new();
    let span = test_span();

    let global = resolver.define("x", SymbolKind::Variable, span.clone());
    resolver.push_scope();
    let local = resolver.define("y", SymbolKind::Variable, span);

    assert_eq!(resolver.lookup("x"), Some(global));
    assert_eq!(resolver.lookup("y"), Some(local));
    assert_eq!(resolver.lookup("missing"), None);
}

#[test]
fn resolver_registers_builtins_in_global_scope() {
    let resolver = Resolver::new();

    for name in [
        "print", "sqrt", "sin", "cos", "exp", "log", "rand", "range", "PI", "E",
    ] {
        let Some(id) = resolver.lookup(name) else {
            panic!("builtin should resolve: {name}");
        };
        assert_eq!(resolver.table().name_of(id), Some(name));
    }
}

fn ident_expr(name: &str, id: u32, span: &Span) -> Expr {
    Expr::new(ExprKind::Ident(name.to_owned()), span.clone(), NodeId(id))
}

fn call_expr(callee: Expr, args: Vec<Expr>, id: u32, span: &Span) -> Expr {
    Expr::new(
        ExprKind::Call {
            callee: Box::new(callee),
            args,
        },
        span.clone(),
        NodeId(id),
    )
}

fn function_decl(name: &str, body: Expr, span: &Span) -> FunctionDecl {
    FunctionDecl {
        name: name.to_owned(),
        params: vec![],
        return_type: None,
        body,
        span: span.clone(),
    }
}

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

fn diagnostic_messages(resolver: &Resolver) -> Vec<String> {
    resolver
        .diagnostics()
        .diagnostics()
        .iter()
        .filter(|diagnostic| matches!(diagnostic.severity, Severity::Error))
        .map(|diagnostic| diagnostic.message.clone())
        .collect()
}

fn run_program(body: Expr) -> Resolver {
    let source = "test";
    let file = Arc::new(SourceFile::new("test.hulk", source));
    let span = Span::new(file, 0, source.len());
    let program = Program {
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

#[test]
fn self_outside_method_reports_error() {
    let span = test_span();
    let resolver = run_program(Expr::new(ExprKind::Self_, span, NodeId(30)));

    assert!(diagnostic_messages(&resolver)
        .iter()
        .any(|message| message.contains("self usado fuera de un método")));
}

#[test]
fn assigning_to_self_reports_error() {
    let span = test_span();
    let resolver = run_program(Expr::new(
        ExprKind::AssignTarget(AssignTarget::Ident("self".to_owned())),
        span,
        NodeId(31),
    ));

    assert!(diagnostic_messages(&resolver)
        .iter()
        .any(|message| message.contains("no se puede asignar a self")));
}

#[test]
fn undefined_variable_reports_error() {
    let span = test_span();
    let resolver = run_program(ident_expr("missing", 32, &span));

    assert!(diagnostic_messages(&resolver)
        .iter()
        .any(|message| message.contains("identificador no declarado")));
}

#[test]
fn duplicate_parameter_in_same_scope_reports_error() {
    let source = "function f(x, x) => 0;";
    let file = Arc::new(SourceFile::new("dup.hulk", source));
    let span = Span::new(file, 0, source.len());
    let program = Program {
        functions: vec![FunctionDecl {
            name: "f".to_owned(),
            params: vec![
                Param {
                    name: "x".to_owned(),
                    type_ann: None,
                    span: span.clone(),
                },
                Param {
                    name: "x".to_owned(),
                    type_ann: None,
                    span: span.clone(),
                },
            ],
            return_type: None,
            body: Expr::new(ExprKind::Number(0.0), span.clone(), NodeId(33)),
            span: span.clone(),
        }],
        types: vec![],
        protocols: vec![],
        macros: vec![],
        body: Expr::new(ExprKind::Number(0.0), span, NodeId(34)),
    };

    let mut resolver = Resolver::new();
    resolver.resolve_program(&program);

    assert!(diagnostic_messages(&resolver)
        .iter()
        .any(|message| message.contains("redefinicion de x")));
}

#[test]
fn missing_function_call_reports_error() {
    let span = test_span();
    let resolver = run_program(Expr::new(
        ExprKind::Call {
            callee: Box::new(ident_expr("missing", 35, &span)),
            args: vec![],
        },
        span,
        NodeId(36),
    ));

    assert!(diagnostic_messages(&resolver)
        .iter()
        .any(|message| message.contains("funcion no existe")));
}

#[test]
fn missing_type_annotation_reports_error() {
    let span = test_span();
    let resolver = run_program(Expr::new(
        ExprKind::New {
            type_ann: TypeAnn::Named("Ghost".to_owned()),
            args: vec![],
        },
        span,
        NodeId(37),
    ));

    assert!(diagnostic_messages(&resolver)
        .iter()
        .any(|message| message.contains("tipo no existe")));
}

#[test]
fn base_without_parent_reports_error() {
    let source = "type Child() { method() => base; }";
    let file = Arc::new(SourceFile::new("base.hulk", source));
    let span = Span::new(file, 0, source.len());

    let method = FunctionDecl {
        name: "method".to_owned(),
        params: vec![],
        return_type: None,
        body: Expr::new(ExprKind::Base, span.clone(), NodeId(38)),
        span: span.clone(),
    };
    let program = Program {
        functions: vec![],
        types: vec![TypeDecl {
            name: "Child".to_owned(),
            params: vec![],
            parent: None,
            members: vec![Member {
                kind: MemberKind::Method(method),
                span: span.clone(),
            }],
            span: span.clone(),
        }],
        protocols: vec![],
        macros: vec![],
        body: Expr::new(ExprKind::Number(0.0), span, NodeId(39)),
    };

    let mut resolver = Resolver::new();
    resolver.resolve_program(&program);

    assert!(diagnostic_messages(&resolver)
        .iter()
        .any(|message| message.contains("base usado en un tipo sin padre")));
}

#[test]
fn base_resolves_to_parent_method() {
    let source =
        "type Parent() { method() => 1; } type Child() inherits Parent { method() => base; }";
    let file = Arc::new(SourceFile::new("inherit.hulk", source));
    let span = Span::new(file, 0, source.len());

    let parent_method = FunctionDecl {
        name: "method".to_owned(),
        params: vec![],
        return_type: None,
        body: Expr::new(ExprKind::Number(1.0), span.clone(), NodeId(40)),
        span: span.clone(),
    };
    let child_method = FunctionDecl {
        name: "method".to_owned(),
        params: vec![],
        return_type: None,
        body: Expr::new(ExprKind::Base, span.clone(), NodeId(41)),
        span: span.clone(),
    };
    let program = Program {
        functions: vec![],
        types: vec![
            TypeDecl {
                name: "Parent".to_owned(),
                params: vec![],
                parent: None,
                members: vec![Member {
                    kind: MemberKind::Method(parent_method),
                    span: span.clone(),
                }],
                span: span.clone(),
            },
            TypeDecl {
                name: "Child".to_owned(),
                params: vec![],
                parent: Some(ParentSpec {
                    name: "Parent".to_owned(),
                    args: vec![],
                    span: span.clone(),
                }),
                members: vec![Member {
                    kind: MemberKind::Method(child_method),
                    span: span.clone(),
                }],
                span: span.clone(),
            },
        ],
        protocols: vec![],
        macros: vec![],
        body: Expr::new(ExprKind::Number(0.0), span, NodeId(42)),
    };

    let mut resolver = Resolver::new();
    resolver.resolve_program(&program);

    assert!(resolver.has_expr_symbol(NodeId(41)));
}
