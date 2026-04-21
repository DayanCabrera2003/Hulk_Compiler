use super::*;

#[test]
fn all_example_programs_build_hir() {
    let mut examples = vec![
        "arithmetic.hulk",
        "classes.hulk",
        "conditionals.hulk",
        "functions.hulk",
        "functors.hulk",
        "hello.hulk",
        "iterables.hulk",
        "let_scoping.hulk",
        "loops.hulk",
        "macros.hulk",
        "protocols.hulk",
        "strings.hulk",
        "vectors.hulk",
    ];

    examples.sort_unstable();

    for example in examples {
        let (name, source) = load_example(example);
        let (hir, bag) = build_source(&name, &source);

        assert_no_errors(&bag, &name);
        assert!(hir.is_some(), "expected {name} to produce a HIR");
    }
}

#[test]
fn hello_example_resolves_print_builtin() {
    let (name, source) = load_example("hello.hulk");
    let (hir, bag) = build_source(&name, &source);

    assert_no_errors(&bag, &name);
    let hir = hir.expect("hello.hulk should build a HIR");

    assert_symbol_kind(&hir, "print", SymbolKind::BuiltinFunction);
    assert_eq!(node_type(&hir, hir.program.body.id), TypeId::OBJECT);
}

#[test]
fn strings_example_infers_concat_expressions() {
    let (name, source) = load_example("strings.hulk");
    let (hir, bag) = build_source(&name, &source);

    assert_no_errors(&bag, &name);
    let hir = hir.expect("strings.hulk should build a HIR");

    let ExprKind::Block(exprs) = &hir.program.body.kind else {
        panic!("strings.hulk should start with a block");
    };

    let concat = exprs
        .iter()
        .filter_map(|expr| match &expr.kind {
            ExprKind::Call { args, .. } => args.first(),
            _ => None,
        })
        .find(|expr| matches!(expr.kind, ExprKind::BinOp { op: hulk_ast::BinOpKind::Concat, .. }))
        .expect("expected a concatenation expression");

    let spaced_concat = exprs
        .iter()
        .filter_map(|expr| match &expr.kind {
            ExprKind::Call { args, .. } => args.first(),
            _ => None,
        })
        .find(|expr| matches!(expr.kind, ExprKind::BinOp { op: hulk_ast::BinOpKind::ConcatSpaced, .. }))
        .expect("expected a spaced concatenation expression");

    assert_eq!(node_type(&hir, concat.id), TypeId::STRING);
    assert_eq!(node_type(&hir, spaced_concat.id), TypeId::STRING);
}

#[test]
fn conditionals_example_infers_string_if_expression() {
    let (name, source) = load_example("conditionals.hulk");
    let (hir, bag) = build_source(&name, &source);

    assert_no_errors(&bag, &name);
    let hir = hir.expect("conditionals.hulk should build a HIR");

    let ExprKind::Block(exprs) = &hir.program.body.kind else {
        panic!("conditionals.hulk should start with a block");
    };

    fn contains_string_literal(expr: &Expr, target: &str) -> bool {
        match &expr.kind {
            ExprKind::StringLit(text) => text == target,
            ExprKind::BinOp { left, right, .. } => {
                contains_string_literal(left, target) || contains_string_literal(right, target)
            }
            ExprKind::UnaryOp { expr: inner, .. } => contains_string_literal(inner, target),
            ExprKind::Call { callee, args } => {
                contains_string_literal(callee, target)
                    || args.iter().any(|arg| contains_string_literal(arg, target))
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                contains_string_literal(receiver, target)
                    || args.iter().any(|arg| contains_string_literal(arg, target))
            }
            ExprKind::FieldAccess { receiver, .. } => contains_string_literal(receiver, target),
            ExprKind::Index { target: target_expr, index } => {
                contains_string_literal(target_expr, target) || contains_string_literal(index, target)
            }
            ExprKind::Block(children) | ExprKind::VecLiteral(children) => {
                children.iter().any(|child| contains_string_literal(child, target))
            }
            ExprKind::VecGenerator { element, iterable, .. } => {
                contains_string_literal(element, target) || contains_string_literal(iterable, target)
            }
            ExprKind::Let { bindings, body } => {
                bindings.iter().any(|binding| contains_string_literal(binding, target))
                    || contains_string_literal(body, target)
            }
            ExprKind::Assign { target: target_expr, value } => {
                contains_string_literal(target_expr, target) || contains_string_literal(value, target)
            }
            ExprKind::LetBinding(binding) => contains_string_literal(&binding.value, target),
            ExprKind::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                contains_string_literal(condition, target)
                    || contains_string_literal(then_branch, target)
                    || elif_branches.iter().any(|(elif_condition, elif_branch)| {
                        contains_string_literal(elif_condition, target)
                            || contains_string_literal(elif_branch, target)
                    })
                    || else_branch
                        .as_deref()
                        .is_some_and(|expr| contains_string_literal(expr, target))
            }
            ExprKind::While { condition, body } => {
                contains_string_literal(condition, target) || contains_string_literal(body, target)
            }
            ExprKind::For { iterable, body, .. } => {
                contains_string_literal(iterable, target) || contains_string_literal(body, target)
            }
            ExprKind::New { args, .. } => args.iter().any(|arg| contains_string_literal(arg, target)),
            ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => contains_string_literal(expr, target),
            ExprKind::Lambda { body, .. } => contains_string_literal(body, target),
            ExprKind::Number(_) | ExprKind::Bool(_) | ExprKind::Ident(_) | ExprKind::Self_ | ExprKind::Base | ExprKind::AssignTarget(_) => false,
        }
    }

    let conditional = exprs
        .iter()
        .filter_map(|expr| match &expr.kind {
            ExprKind::Let { body, .. } => Some(body.as_ref()),
            _ => None,
        })
        .find(|body| contains_string_literal(body, "Magic"))
        .and_then(|body| find_expr_by_predicate(body, &|expr| matches!(expr.kind, ExprKind::If { .. })))
        .expect("expected the string-producing if expression inside the let body");

    assert_eq!(node_type(&hir, conditional.id), TypeId::STRING);
}

#[test]
fn classes_example_resolves_self_and_base() {
    let (name, source) = load_example("classes.hulk");
    let (hir, bag) = build_source(&name, &source);

    assert_no_errors(&bag, &name);
    let hir = hir.expect("classes.hulk should build a HIR");

    assert_symbol_kind(&hir, "Point", SymbolKind::Type);
    assert_symbol_kind(&hir, "PolarPoint", SymbolKind::Type);
    assert_symbol_kind(&hir, "Person", SymbolKind::Type);
    assert_symbol_kind(&hir, "Knight", SymbolKind::Type);

    let get_x_body = find_method_expr(&hir.program, "Point", "getX");
    let ExprKind::FieldAccess { receiver, .. } = &get_x_body.kind else {
        panic!("Point.getX should use a field access");
    };
    let ExprKind::Self_ = receiver.kind else {
        panic!("Point.getX should resolve self");
    };

    let self_symbol = hir
        .resolved_symbol(receiver.id)
        .expect("self should resolve inside Point.getX");
    let self_entry = hir
        .symbols
        .table()
        .get(self_symbol)
        .expect("self symbol must exist");
    assert_eq!(self_entry.kind, SymbolKind::SelfValue);

    let knight_body = find_method_expr(&hir.program, "Knight", "name");
    let base_call = first_expr(knight_body, |expr| matches!(expr.kind, ExprKind::Base));
    let base_symbol = hir
        .resolved_symbol(base_call.id)
        .expect("base should resolve in Knight.name");
    let base_entry = hir
        .symbols
        .table()
        .get(base_symbol)
        .expect("base symbol must exist");
    assert_eq!(base_entry.name, "name");
    assert_eq!(base_entry.kind, SymbolKind::Function);
}

#[test]
fn protocols_example_registers_protocol_symbols() {
    let (name, source) = load_example("protocols.hulk");
    let (hir, bag) = build_source(&name, &source);

    assert_no_errors(&bag, &name);
    let hir = hir.expect("protocols.hulk should build a HIR");

    assert_symbol_kind(&hir, "Hashable", SymbolKind::Protocol);
    assert_symbol_kind(&hir, "Equatable", SymbolKind::Protocol);
    assert_symbol_kind(&hir, "Iterable", SymbolKind::Protocol);
    assert_symbol_kind(&hir, "Enumerable", SymbolKind::Protocol);
}
