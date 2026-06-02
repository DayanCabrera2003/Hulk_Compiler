use std::fs;
use std::path::{Path, PathBuf};

use crate::support::merge_diagnostics;
use hulk_ast::{Expr, ExprKind, FunctionDecl, MemberKind, NodeId, Program};
use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{Hir, SourceFile, SymbolKind, TypeId};
use hulk_lexer::lex;
use hulk_parser::parse;
use hulk_semantic::Resolver as SemanticResolver;
use hulk_types::{TypeEnv, TypeInferer};

const PRELUDE: &str = include_str!("../../../../../prelude/prelude.hulk");

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn load_example(name: &str) -> (String, String) {
    let path = examples_dir().join(name);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    (name.to_owned(), source)
}

fn build_source(name: &str, source: &str) -> (Option<Hir>, DiagnosticBag) {
    let combined = format!("{PRELUDE}\n{source}");
    let source = SourceFile::new(name, &combined);

    let mut bag = DiagnosticBag::new();

    let mut lexer_bag = DiagnosticBag::new();
    let tokens = lex(&source, &mut lexer_bag);
    merge_diagnostics(&mut bag, &lexer_bag);

    let (program, parser_bag) = parse(tokens, &source);
    merge_diagnostics(&mut bag, &parser_bag);

    let mut symbols = SemanticResolver::new();
    symbols.resolve_program(&program);
    merge_diagnostics(&mut bag, symbols.diagnostics());

    let mut types = TypeEnv::new();
    {
        let mut inferer = TypeInferer::new(&mut types, &symbols, &mut bag);
        infer_program(&program, &mut inferer);
    }

    if bag.has_errors() {
        (None, bag)
    } else {
        (
            Some(Hir::from_typed(hulk_hir::TypedAst {
                program,
                symbols,
                types,
            })),
            bag,
        )
    }
}

fn infer_program(program: &Program, inferer: &mut TypeInferer<'_>) {
    for function in &program.functions {
        inferer.infer_expr(&function.body);
    }

    for type_decl in &program.types {
        if let Some(parent) = &type_decl.parent {
            for arg in &parent.args {
                inferer.infer_expr(arg);
            }
        }

        for member in &type_decl.members {
            match &member.kind {
                MemberKind::Attribute { value, .. } => {
                    inferer.infer_expr(value);
                }
                MemberKind::Method(method) => {
                    inferer.infer_expr(&method.body);
                }
            }
        }
    }

    for macro_decl in &program.macros {
        inferer.infer_expr(&macro_decl.body);
    }

    inferer.infer_expr(&program.body);
}

fn assert_no_errors(bag: &DiagnosticBag, name: &str) {
    assert!(
        bag.is_empty(),
        "unexpected diagnostics for {name}: {:?}",
        bag.diagnostics()
    );
}

fn assert_symbol_kind(hir: &Hir, name: &str, kind: SymbolKind) {
    let symbol_id = hir
        .symbols
        .lookup(name)
        .unwrap_or_else(|| panic!("expected symbol `{name}` to exist"));
    let symbol = hir
        .symbols
        .table()
        .get(symbol_id)
        .expect("symbol id should point to a table entry");
    assert_eq!(symbol.kind, kind, "unexpected kind for symbol `{name}`");
}

fn find_function<'a>(program: &'a Program, name: &str) -> &'a FunctionDecl {
    program
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("function `{name}` not found"))
}

fn find_type<'a>(program: &'a Program, name: &str) -> &'a hulk_ast::TypeDecl {
    program
        .types
        .iter()
        .find(|type_decl| type_decl.name == name)
        .unwrap_or_else(|| panic!("type `{name}` not found"))
}

fn find_method_expr<'a>(program: &'a Program, type_name: &str, method_name: &str) -> &'a Expr {
    let type_decl = find_type(program, type_name);
    let member = type_decl
        .members
        .iter()
        .find(|member| matches!(&member.kind, MemberKind::Method(method) if method.name == method_name))
        .unwrap_or_else(|| panic!("method `{method_name}` not found in `{type_name}`"));

    match &member.kind {
        MemberKind::Method(method) => &method.body,
        MemberKind::Attribute { .. } => unreachable!("member should be a method"),
    }
}

fn find_expr_by_predicate<'a>(
    expr: &'a Expr,
    predicate: &dyn Fn(&Expr) -> bool,
) -> Option<&'a Expr> {
    if predicate(expr) {
        return Some(expr);
    }

    match &expr.kind {
        ExprKind::BinOp { left, right, .. } => find_expr_by_predicate(left, predicate)
            .or_else(|| find_expr_by_predicate(right, predicate)),
        ExprKind::UnaryOp { expr: inner, .. } => find_expr_by_predicate(inner, predicate),
        ExprKind::Call { callee, args } => {
            find_expr_by_predicate(callee, predicate).or_else(|| {
                args.iter()
                    .find_map(|arg| find_expr_by_predicate(arg, predicate))
            })
        }
        ExprKind::MethodCall { receiver, args, .. } => find_expr_by_predicate(receiver, predicate)
            .or_else(|| {
                args.iter()
                    .find_map(|arg| find_expr_by_predicate(arg, predicate))
            }),
        ExprKind::FieldAccess { receiver, .. } => find_expr_by_predicate(receiver, predicate),
        ExprKind::Index { target, index } => find_expr_by_predicate(target, predicate)
            .or_else(|| find_expr_by_predicate(index, predicate)),
        ExprKind::Block(exprs) => exprs
            .iter()
            .find_map(|child| find_expr_by_predicate(child, predicate)),
        ExprKind::VecLiteral(exprs) => exprs
            .iter()
            .find_map(|child| find_expr_by_predicate(child, predicate)),
        ExprKind::VecGenerator {
            element, iterable, ..
        } => find_expr_by_predicate(element, predicate)
            .or_else(|| find_expr_by_predicate(iterable, predicate)),
        ExprKind::Let { bindings, body } => bindings
            .iter()
            .find_map(|binding| find_expr_by_predicate(binding, predicate))
            .or_else(|| find_expr_by_predicate(body, predicate)),
        ExprKind::Assign { target, value } => find_expr_by_predicate(target, predicate)
            .or_else(|| find_expr_by_predicate(value, predicate)),
        ExprKind::AssignTarget(_) => None,
        ExprKind::LetBinding(binding) => find_expr_by_predicate(&binding.value, predicate),
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => find_expr_by_predicate(condition, predicate)
            .or_else(|| find_expr_by_predicate(then_branch, predicate))
            .or_else(|| {
                elif_branches
                    .iter()
                    .find_map(|(elif_condition, elif_branch)| {
                        find_expr_by_predicate(elif_condition, predicate)
                            .or_else(|| find_expr_by_predicate(elif_branch, predicate))
                    })
            })
            .or_else(|| {
                else_branch
                    .as_deref()
                    .and_then(|expr| find_expr_by_predicate(expr, predicate))
            }),
        ExprKind::While { condition, body } => find_expr_by_predicate(condition, predicate)
            .or_else(|| find_expr_by_predicate(body, predicate)),
        ExprKind::For { iterable, body, .. } => find_expr_by_predicate(iterable, predicate)
            .or_else(|| find_expr_by_predicate(body, predicate)),
        ExprKind::New { args, .. } => args
            .iter()
            .find_map(|arg| find_expr_by_predicate(arg, predicate)),
        ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => {
            find_expr_by_predicate(expr, predicate)
        }
        ExprKind::Lambda { body, .. } => find_expr_by_predicate(body, predicate),
        ExprKind::Number(_)
        | ExprKind::StringLit(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_)
        | ExprKind::Self_
        | ExprKind::Base => None,
    }
}

fn first_expr(expr: &Expr, predicate: impl Fn(&Expr) -> bool) -> &Expr {
    find_expr_by_predicate(expr, &predicate)
        .unwrap_or_else(|| panic!("matching expression not found in {:?}", expr.kind))
}

fn node_type(hir: &Hir, node_id: NodeId) -> TypeId {
    hir.expr_type(node_id)
        .unwrap_or_else(|| panic!("missing inferred type for node {node_id:?}"))
}

mod examples;
mod inference;
