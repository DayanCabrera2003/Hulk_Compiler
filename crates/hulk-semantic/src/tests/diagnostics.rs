use std::sync::Arc;

use hulk_ast::{
    AssignTarget, Expr, ExprKind, FunctionDecl, Member, MemberKind, NodeId, Param, ParentSpec,
    Program, SourceFile, Span, TypeDecl,
};

use crate::Resolver;

use super::common::{diagnostic_messages, run_program, test_span};

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
    let resolver = run_program(Expr::new(
        ExprKind::Ident("missing".to_owned()),
        span,
        NodeId(32),
    ));

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
            callee: Box::new(Expr::new(
                ExprKind::Ident("missing".to_owned()),
                span.clone(),
                NodeId(35),
            )),
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
    use hulk_ast::TypeAnn;

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
