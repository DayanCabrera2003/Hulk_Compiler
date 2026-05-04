use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::support::merge_diagnostics;
use hulk_ast::{Expr, ExprKind, MemberKind, NodeId, Program};
use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{Hir, SourceFile};
use hulk_lexer::lex;
use hulk_parser::parse;
use hulk_semantic::Resolver as SemanticResolver;
use hulk_types::{TypeEnv, TypeInferer};
use proptest::prelude::*;

fn build_source(name: &str, source: &str) -> (Option<Hir>, DiagnosticBag) {
    let source = SourceFile::new(name, source);

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
        let mut inferer = TypeInferer::new(&mut types, &symbols, &bag);
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
                MemberKind::Attribute { value, .. } => inferer.infer_expr(value),
                MemberKind::Method(method) => inferer.infer_expr(&method.body),
            };
        }
    }

    for macro_decl in &program.macros {
        inferer.infer_expr(&macro_decl.body);
    }

    inferer.infer_expr(&program.body);
}

fn collect_program_node_ids(program: &Program) -> HashSet<NodeId> {
    let mut ids = HashSet::new();

    for function in &program.functions {
        collect_expr_ids(&function.body, &mut ids);
    }

    for type_decl in &program.types {
        if let Some(parent) = &type_decl.parent {
            for arg in &parent.args {
                collect_expr_ids(arg, &mut ids);
            }
        }

        for member in &type_decl.members {
            match &member.kind {
                MemberKind::Attribute { value, .. } => collect_expr_ids(value, &mut ids),
                MemberKind::Method(method) => collect_expr_ids(&method.body, &mut ids),
            }
        }
    }

    for macro_decl in &program.macros {
        collect_expr_ids(&macro_decl.body, &mut ids);
    }

    collect_expr_ids(&program.body, &mut ids);
    ids
}

fn collect_expr_ids(expr: &Expr, ids: &mut HashSet<NodeId>) {
    ids.insert(expr.id);

    match &expr.kind {
        ExprKind::BinOp { left, right, .. } => {
            collect_expr_ids(left, ids);
            collect_expr_ids(right, ids);
        }
        ExprKind::UnaryOp { expr: inner, .. } => collect_expr_ids(inner, ids),
        ExprKind::Call { callee, args } => {
            collect_expr_ids(callee, ids);
            for arg in args {
                collect_expr_ids(arg, ids);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            collect_expr_ids(receiver, ids);
            for arg in args {
                collect_expr_ids(arg, ids);
            }
        }
        ExprKind::FieldAccess { receiver, .. } => collect_expr_ids(receiver, ids),
        ExprKind::Index { target, index } => {
            collect_expr_ids(target, ids);
            collect_expr_ids(index, ids);
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
            for child in exprs {
                collect_expr_ids(child, ids);
            }
        }
        ExprKind::VecGenerator {
            element, iterable, ..
        } => {
            collect_expr_ids(element, ids);
            collect_expr_ids(iterable, ids);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                collect_expr_ids(binding, ids);
            }
            collect_expr_ids(body, ids);
        }
        ExprKind::Assign { target, value } => {
            collect_expr_ids(target, ids);
            collect_expr_ids(value, ids);
        }
        ExprKind::LetBinding(binding) => collect_expr_ids(&binding.value, ids),
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            collect_expr_ids(condition, ids);
            collect_expr_ids(then_branch, ids);
            for (elif_condition, elif_branch) in elif_branches {
                collect_expr_ids(elif_condition, ids);
                collect_expr_ids(elif_branch, ids);
            }
            if let Some(else_expr) = else_branch {
                collect_expr_ids(else_expr, ids);
            }
        }
        ExprKind::While { condition, body } => {
            collect_expr_ids(condition, ids);
            collect_expr_ids(body, ids);
        }
        ExprKind::For { iterable, body, .. } => {
            collect_expr_ids(iterable, ids);
            collect_expr_ids(body, ids);
        }
        ExprKind::New { args, .. } => {
            for arg in args {
                collect_expr_ids(arg, ids);
            }
        }
        ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => collect_expr_ids(expr, ids),
        ExprKind::Lambda { body, .. } => collect_expr_ids(body, ids),
        ExprKind::Number(_)
        | ExprKind::StringLit(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_)
        | ExprKind::Self_
        | ExprKind::Base
        | ExprKind::AssignTarget(_) => {}
    }
}

fn assert_hir_consistency(hir: &Hir) {
    let ast_nodes = collect_program_node_ids(&hir.program);

    for node_id in hir.types.expr_type_nodes() {
        assert!(
            ast_nodes.contains(&node_id),
            "expr_types contains NodeId not present in AST: {node_id:?}"
        );
    }

    for symbol_id in hir.types.symbol_type_symbols() {
        assert!(
            hir.symbols.table().get(symbol_id).is_some(),
            "symbol_types contains SymbolId not present in SymbolTable: {symbol_id:?}"
        );
    }
}

fn leaf_expr() -> impl Strategy<Value = String> {
    let ident = prop::sample::select(vec!["a", "b", "c", "x", "y", "z", "u", "v", "w"])
        .prop_map(str::to_owned);
    let num = any::<i16>().prop_map(|n| n.to_string());
    let booleans = prop_oneof![Just("true".to_owned()), Just("false".to_owned())];
    let text = prop::collection::vec(
        prop::sample::select(vec!['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j']),
        1..=5,
    )
    .prop_map(|chars| {
        let value = chars.into_iter().collect::<String>();
        format!("\"{value}\"")
    });

    prop_oneof![ident, num, booleans, text]
}

fn expr_strategy() -> impl Strategy<Value = String> {
    leaf_expr().prop_recursive(4, 64, 2, |inner| {
        prop_oneof![
            inner.clone().prop_map(|e| format!("({e})")),
            inner.clone().prop_map(|e| format!("-{e}")),
            inner.clone().prop_map(|e| format!("!{e}")),
            (inner.clone(), inner.clone()).prop_map(|(l, r)| format!("({l} + {r})")),
            (inner.clone(), inner.clone()).prop_map(|(l, r)| format!("({l} * {r})")),
            (inner.clone(), inner.clone()).prop_map(|(l, r)| format!("({l} @ {r})")),
            (inner.clone(), inner).prop_map(|(l, r)| format!("({l} @@ {r})")),
        ]
    })
}

fn syntactically_valid_program_strategy() -> impl Strategy<Value = String> {
    (
        // Optional functions
        prop::collection::vec(
            (
                prop::sample::select(vec!["f", "g", "h", "p", "q"]).prop_map(str::to_owned),
                prop::sample::select(vec!["x", "y", "z"]).prop_map(str::to_owned),
                expr_strategy(),
            ),
            0..=3,
        ),
        // Optional type declarations
        prop::collection::vec(
            (
                prop::sample::select(vec!["A", "B", "C", "T", "U"]).prop_map(str::to_owned),
                prop::collection::vec(
                    (
                        prop::sample::select(vec!["x", "y", "z", "value", "data"])
                            .prop_map(str::to_owned),
                        prop::sample::select(vec!["Number", "String", "Boolean"])
                            .prop_map(str::to_owned),
                    ),
                    0..=2,
                ),
            ),
            0..=2,
        ),
        // Let bindings
        prop::collection::vec(
            (
                prop::sample::select(vec!["a", "b", "c", "x", "y", "z"]).prop_map(str::to_owned),
                expr_strategy(),
            ),
            1..=4,
        ),
        expr_strategy(),
        prop::sample::select(vec!["missing", "ghost", "unknown"]).prop_map(str::to_owned),
        any::<bool>(),
    )
        .prop_map(
            |(functions, types, bindings, body_expr, missing_name, use_missing_body)| {
                let mut program = String::new();

                // Add function declarations
                for (fname, param, body) in functions {
                    program.push_str(&format!("function {fname}({param}) => {body};\n"));
                }

                // Add type declarations
                for (tname, attrs) in types {
                    if attrs.is_empty() {
                        program.push_str(&format!("type {tname} {{}}\n"));
                    } else {
                        let attr_src = attrs
                            .iter()
                            .map(|(name, type_name)| format!("{name}: {type_name}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        program.push_str(&format!("type {tname} {{ {attr_src}; }}\n"));
                    }
                }

                // Add let bindings and body
                let bindings_src = bindings
                    .iter()
                    .map(|(name, value)| format!("{name} = {value}"))
                    .collect::<Vec<_>>()
                    .join(", ");

                let body = if use_missing_body {
                    missing_name
                } else {
                    body_expr
                };

                program.push_str(&format!("let {bindings_src} in {body};"));
                program
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn generated_semantic_inputs_never_panic_and_report_result(source in syntactically_valid_program_strategy()) {
        let result = catch_unwind(AssertUnwindSafe(|| build_source("generated.hulk", &source)));

        prop_assert!(result.is_ok(), "pipeline panicked for source: {source}");

        let (hir, bag) = result.expect("already checked as Ok");
        prop_assert!(hir.is_some() || !bag.is_empty());
    }

    #[test]
    fn hir_maps_are_consistent_with_ast_and_symbol_table(source in syntactically_valid_program_strategy()) {
        let (hir, _bag) = build_source("generated_consistency.hulk", &source);

        if let Some(hir) = hir {
            assert_hir_consistency(&hir);
        }
    }
}
