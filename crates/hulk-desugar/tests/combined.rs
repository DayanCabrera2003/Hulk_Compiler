// Session 12.1 — Combined desugaring tests.
//
// Each test exercises two or more sugar constructs simultaneously so that
// interactions between transforms are verified, not just individual transforms
// in isolation.

#[path = "support/mod.rs"]
mod support;

use hulk_hir::{BinOpKind, Expr, ExprKind, MemberKind, NodeIdGen, Param, Span, TypeAnn};

use support::{contains_any_sugar, make_hir, program_has_sugar, run_desugar, source_span};

fn ids() -> NodeIdGen {
    NodeIdGen::new()
}

fn ident(name: &str, span: &Span, g: &mut NodeIdGen) -> Expr {
    Expr::new(ExprKind::Ident(name.to_owned()), span.clone(), g.next_id())
}

fn number(v: f64, span: &Span, g: &mut NodeIdGen) -> Expr {
    Expr::new(ExprKind::Number(v), span.clone(), g.next_id())
}

fn call(callee: Expr, args: Vec<Expr>, span: &Span, g: &mut NodeIdGen) -> Expr {
    Expr::new(
        ExprKind::Call {
            callee: Box::new(callee),
            args,
        },
        span.clone(),
        g.next_id(),
    )
}

// ─── tests ───────────────────────────────────────────────────────────────────

/// A lambda whose body is a `for` loop: both constructs must be lowered.
/// Expected after desugar:
///   - lambda → new __LambdaN() + synthetic type with invoke method
///   - for in invoke body → let + while (no For nodes)
#[test]
fn for_inside_lambda_body_both_lowered() {
    let (_, span) = source_span("(x: Number) => for (y in xs) print(y)");
    let mut g = ids();

    let xs = ident("xs", &span, &mut g);
    let print_y = call(
        ident("print", &span, &mut g),
        vec![ident("y", &span, &mut g)],
        &span,
        &mut g,
    );

    let for_expr = Expr::new(
        ExprKind::For {
            binding: "y".to_owned(),
            iterable: Box::new(xs),
            body: Box::new(print_y),
        },
        span.clone(),
        g.next_id(),
    );

    let lambda = Expr::new(
        ExprKind::Lambda {
            params: vec![Param {
                name: "x".to_owned(),
                type_ann: Some(TypeAnn::Named("Number".to_owned())),
                span: span.clone(),
            }],
            return_type: None,
            body: Box::new(for_expr),
        },
        span.clone(),
        g.next_id(),
    );

    let hir = make_hir(lambda);
    let result = run_desugar(hir);

    // Body must be New(...) — lambda was synthesized.
    assert!(
        matches!(result.program.body.kind, ExprKind::New { .. }),
        "lambda body must become new synthetic type constructor"
    );

    // No sugar constructs anywhere in the program (body + generated types).
    assert!(
        !program_has_sugar(&result),
        "sugar nodes remain after desugaring lambda+for"
    );

    // Exactly one synthetic type was generated for the lambda.
    assert_eq!(
        result.program.types.len(),
        1,
        "expected one generated type for the lambda"
    );

    // The generated invoke method body contains no For nodes — it was desugared.
    let invoke = result.program.types[0]
        .members
        .iter()
        .find_map(|m| {
            if let MemberKind::Method(method) = &m.kind {
                if method.name == "invoke" {
                    Some(method)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .expect("generated type must have an invoke method");

    assert!(
        !contains_any_sugar(&invoke.body),
        "invoke method body still contains sugar after desugaring"
    );
}

/// A `for` loop whose body uses `@@` (ConcatSpaced): both must be lowered.
#[test]
fn concat_spaced_inside_for_body_both_lowered() {
    let (_, span) = source_span("for (x in xs) a @@ b");
    let mut g = ids();

    let xs = ident("xs", &span, &mut g);
    let concat_body = Expr::new(
        ExprKind::BinOp {
            op: BinOpKind::ConcatSpaced,
            left: Box::new(ident("a", &span, &mut g)),
            right: Box::new(ident("b", &span, &mut g)),
        },
        span.clone(),
        g.next_id(),
    );

    let for_expr = Expr::new(
        ExprKind::For {
            binding: "x".to_owned(),
            iterable: Box::new(xs),
            body: Box::new(concat_body),
        },
        span.clone(),
        g.next_id(),
    );

    let hir = make_hir(for_expr);
    let result = run_desugar(hir);

    assert!(
        !contains_any_sugar(&result.program.body),
        "sugar nodes remain after for+@@ desugar"
    );

    // Top-level result is a let (from for desugar), not a for.
    assert!(
        matches!(result.program.body.kind, ExprKind::Let { .. }),
        "for loop must become a let expression"
    );
}

/// A `VecGenerator` whose element expression uses `@@`: both are lowered and
/// the desugared push call must receive the concat form of the element.
#[test]
fn concat_spaced_element_in_vec_generator_both_lowered() {
    let (_, span) = source_span("[a @@ b | x in xs]");
    let mut g = ids();

    let xs = ident("xs", &span, &mut g);
    let element = Expr::new(
        ExprKind::BinOp {
            op: BinOpKind::ConcatSpaced,
            left: Box::new(ident("a", &span, &mut g)),
            right: Box::new(ident("b", &span, &mut g)),
        },
        span.clone(),
        g.next_id(),
    );

    let gen = Expr::new(
        ExprKind::VecGenerator {
            element: Box::new(element),
            binding: "x".to_owned(),
            iterable: Box::new(xs),
        },
        span.clone(),
        g.next_id(),
    );

    let hir = make_hir(gen);
    let result = run_desugar(hir);

    assert!(
        !contains_any_sugar(&result.program.body),
        "sugar nodes remain after vec_generator+@@ desugar"
    );
}

/// Program with all four sugar constructs simultaneously:
/// For, @@, VecGenerator, Lambda — none must survive desugaring.
#[test]
fn all_sugar_constructs_eliminated_together() {
    let (_, span) = source_span("all sugar");
    let mut g = ids();

    // Lambda containing a for loop with @@ body.
    let concat_spaced = Expr::new(
        ExprKind::BinOp {
            op: BinOpKind::ConcatSpaced,
            left: Box::new(ident("a", &span, &mut g)),
            right: Box::new(ident("b", &span, &mut g)),
        },
        span.clone(),
        g.next_id(),
    );
    let inner_for = Expr::new(
        ExprKind::For {
            binding: "x".to_owned(),
            iterable: Box::new(ident("xs", &span, &mut g)),
            body: Box::new(concat_spaced),
        },
        span.clone(),
        g.next_id(),
    );
    let lambda = Expr::new(
        ExprKind::Lambda {
            params: vec![],
            return_type: None,
            body: Box::new(inner_for),
        },
        span.clone(),
        g.next_id(),
    );

    // Vec generator with the lambda as element expression.
    let vec_gen = Expr::new(
        ExprKind::VecGenerator {
            element: Box::new(lambda),
            binding: "y".to_owned(),
            iterable: Box::new(ident("ys", &span, &mut g)),
        },
        span.clone(),
        g.next_id(),
    );

    // Block combining the vec generator with a standalone @@ expression.
    let standalone_concat = Expr::new(
        ExprKind::BinOp {
            op: BinOpKind::ConcatSpaced,
            left: Box::new(number(1.0, &span, &mut g)),
            right: Box::new(number(2.0, &span, &mut g)),
        },
        span.clone(),
        g.next_id(),
    );
    let body = Expr::new(
        ExprKind::Block(vec![vec_gen, standalone_concat]),
        span.clone(),
        g.next_id(),
    );

    let hir = make_hir(body);
    let result = run_desugar(hir);

    assert!(
        !program_has_sugar(&result),
        "one or more sugar constructs survived desugaring"
    );
}

/// Node IDs assigned to synthetic nodes introduced by desugaring must be
/// unique — the Desugarer's counter must not produce collisions.
#[test]
fn synthetic_node_ids_are_unique_after_desugaring() {
    use std::collections::HashSet;

    let (_, span) = source_span("node id uniqueness");
    let mut g = ids();

    // Two independent for loops so multiple sets of synthetic nodes are created.
    let for_a = Expr::new(
        ExprKind::For {
            binding: "x".to_owned(),
            iterable: Box::new(ident("xs", &span, &mut g)),
            body: Box::new(number(1.0, &span, &mut g)),
        },
        span.clone(),
        g.next_id(),
    );
    let for_b = Expr::new(
        ExprKind::For {
            binding: "y".to_owned(),
            iterable: Box::new(ident("ys", &span, &mut g)),
            body: Box::new(number(2.0, &span, &mut g)),
        },
        span.clone(),
        g.next_id(),
    );
    let body = Expr::new(
        ExprKind::Block(vec![for_a, for_b]),
        span.clone(),
        g.next_id(),
    );

    let hir = make_hir(body);
    let result = run_desugar(hir);

    // Collect all node IDs and verify uniqueness.
    let mut seen: HashSet<hulk_hir::NodeId> = HashSet::new();
    collect_node_ids(&result.program.body, &mut seen);

    // If any ID appeared twice, it would have been replaced; the set size
    // equals the number of unique IDs. We verify by collecting with a counter.
    let total = count_nodes(&result.program.body);
    assert_eq!(
        seen.len(),
        total,
        "node ID collision detected: {total} nodes but only {} unique IDs",
        seen.len()
    );
}

fn collect_node_ids(expr: &Expr, out: &mut std::collections::HashSet<hulk_hir::NodeId>) {
    out.insert(expr.id);
    match &expr.kind {
        ExprKind::BinOp { left, right, .. } => {
            collect_node_ids(left, out);
            collect_node_ids(right, out);
        }
        ExprKind::UnaryOp { expr: inner, .. } => collect_node_ids(inner, out),
        ExprKind::Call { callee, args } => {
            collect_node_ids(callee, out);
            for a in args {
                collect_node_ids(a, out);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            collect_node_ids(receiver, out);
            for a in args {
                collect_node_ids(a, out);
            }
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
            for e in exprs {
                collect_node_ids(e, out);
            }
        }
        ExprKind::Let { bindings, body } => {
            for b in bindings {
                collect_node_ids(b, out);
            }
            collect_node_ids(body, out);
        }
        ExprKind::LetBinding(lb) => collect_node_ids(&lb.value, out),
        ExprKind::While { condition, body } => {
            collect_node_ids(condition, out);
            collect_node_ids(body, out);
        }
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            collect_node_ids(condition, out);
            collect_node_ids(then_branch, out);
            for (c, b) in elif_branches {
                collect_node_ids(c, out);
                collect_node_ids(b, out);
            }
            if let Some(e) = else_branch {
                collect_node_ids(e, out);
            }
        }
        ExprKind::New { args, .. } => {
            for a in args {
                collect_node_ids(a, out);
            }
        }
        ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => collect_node_ids(expr, out),
        _ => {}
    }
}

fn count_nodes(expr: &Expr) -> usize {
    let mut count = 1;
    match &expr.kind {
        ExprKind::BinOp { left, right, .. } => {
            count += count_nodes(left) + count_nodes(right);
        }
        ExprKind::UnaryOp { expr: inner, .. } => count += count_nodes(inner),
        ExprKind::Call { callee, args } => {
            count += count_nodes(callee) + args.iter().map(count_nodes).sum::<usize>();
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            count += count_nodes(receiver) + args.iter().map(count_nodes).sum::<usize>();
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
            count += exprs.iter().map(count_nodes).sum::<usize>();
        }
        ExprKind::Let { bindings, body } => {
            count += bindings.iter().map(count_nodes).sum::<usize>() + count_nodes(body);
        }
        ExprKind::LetBinding(lb) => count += count_nodes(&lb.value),
        ExprKind::While { condition, body } => {
            count += count_nodes(condition) + count_nodes(body);
        }
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            count += count_nodes(condition) + count_nodes(then_branch);
            for (c, b) in elif_branches {
                count += count_nodes(c) + count_nodes(b);
            }
            if let Some(e) = else_branch {
                count += count_nodes(e);
            }
        }
        ExprKind::New { args, .. } => {
            count += args.iter().map(count_nodes).sum::<usize>();
        }
        ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => count += count_nodes(expr),
        _ => {}
    }
    count
}
