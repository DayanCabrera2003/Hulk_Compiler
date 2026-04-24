//! Visitor / VisitorMut — exhaustive coverage.

use super::*;

/// Counts every expression variant visited by walk, to assert that the
/// visitor reaches every node type.
#[derive(Default)]
struct VariantCounter {
    counts: std::collections::HashMap<&'static str, usize>,
    type_anns: usize,
    params: usize,
}

impl VariantCounter {
    fn bump(&mut self, name: &'static str) {
        *self.counts.entry(name).or_default() += 1;
    }
}

impl Visitor for VariantCounter {
    fn visit_expr(&mut self, e: &Expr) {
        let name = match &e.kind {
            ExprKind::Number(_) => "Number",
            ExprKind::StringLit(_) => "StringLit",
            ExprKind::Bool(_) => "Bool",
            ExprKind::Ident(_) => "Ident",
            ExprKind::Self_ => "Self_",
            ExprKind::Base => "Base",
            ExprKind::BinOp { .. } => "BinOp",
            ExprKind::UnaryOp { .. } => "UnaryOp",
            ExprKind::Call { .. } => "Call",
            ExprKind::MethodCall { .. } => "MethodCall",
            ExprKind::FieldAccess { .. } => "FieldAccess",
            ExprKind::Index { .. } => "Index",
            ExprKind::Block(_) => "Block",
            ExprKind::VecLiteral(_) => "VecLiteral",
            ExprKind::VecGenerator { .. } => "VecGenerator",
            ExprKind::Let { .. } => "Let",
            ExprKind::Assign { .. } => "Assign",
            ExprKind::AssignTarget(_) => "AssignTarget",
            ExprKind::LetBinding(_) => "LetBinding",
            ExprKind::If { .. } => "If",
            ExprKind::While { .. } => "While",
            ExprKind::For { .. } => "For",
            ExprKind::New { .. } => "New",
            ExprKind::Is { .. } => "Is",
            ExprKind::As { .. } => "As",
            ExprKind::Lambda { .. } => "Lambda",
        };
        self.bump(name);
        hulk_ast::visitor::walk_expr(self, e);
    }

    fn visit_type_ann(&mut self, ann: &TypeAnn) {
        self.type_anns += 1;
        hulk_ast::visitor::walk_type_ann(self, ann);
    }

    fn visit_param(&mut self, p: &Param) {
        self.params += 1;
        hulk_ast::visitor::walk_param(self, p);
    }
}

#[test]
fn visitor_reaches_every_expr_variant_in_kitchen_sink() {
    let program = build_kitchen_sink_program();
    let mut counter = VariantCounter::default();
    counter.visit_program(&program);

    // Every expression variant defined by ExprKind must have been visited
    // at least once.
    let expected_variants = [
        "Number",
        "StringLit",
        "Bool",
        "Ident",
        "Self_",
        "Base",
        "BinOp",
        "UnaryOp",
        "Call",
        "MethodCall",
        "FieldAccess",
        "Index",
        "Block",
        "VecLiteral",
        "VecGenerator",
        "Let",
        "Assign",
        "AssignTarget",
        "LetBinding",
        "If",
        "While",
        "For",
        "New",
        "Is",
        "As",
        "Lambda",
    ];

    for name in expected_variants {
        assert!(
            counter.counts.contains_key(name),
            "visitor did not reach variant {name}: counts = {:?}",
            counter.counts
        );
    }

    // Params and type annotations must also have been visited (proves that
    // walk_macro_decl reaches param type annotations and walk_function_decl
    // reaches params).
    assert!(counter.params > 0);
    assert!(counter.type_anns > 0);
}

/// Verifies that walk_macro_decl visits each macro parameter's type annotation.
#[test]
fn visitor_reaches_macro_param_type_annotations() {
    struct AnnCollector(Vec<String>);
    impl Visitor for AnnCollector {
        fn visit_type_ann(&mut self, ann: &TypeAnn) {
            if let TypeAnn::Named(n) = ann {
                self.0.push(n.clone());
            }
            hulk_ast::visitor::walk_type_ann(self, ann);
        }
    }

    let span = fresh_span();
    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![MacroDecl {
            name: "m".to_owned(),
            params: vec![
                MacroParam::Regular {
                    name: "a".to_owned(),
                    type_ann: TypeAnn::Named("Number".to_owned()),
                    span: span.clone(),
                },
                MacroParam::Body {
                    name: "b".to_owned(),
                    type_ann: TypeAnn::Named("Object".to_owned()),
                    span: span.clone(),
                },
            ],
            body: num(0.0, 1),
            span: span.clone(),
        }],
        body: num(0.0, 2),
    };

    let mut collector = AnnCollector(Vec::new());
    collector.visit_program(&program);

    assert!(collector.0.contains(&"Number".to_owned()));
    assert!(collector.0.contains(&"Object".to_owned()));
}

/// Verifies that walk_member visits the attribute initializer expression.
#[test]
fn visitor_reaches_attribute_initializers() {
    #[derive(Default)]
    struct NumberCollector(Vec<f64>);
    impl Visitor for NumberCollector {
        fn visit_expr(&mut self, e: &Expr) {
            if let ExprKind::Number(n) = &e.kind {
                self.0.push(*n);
            }
            hulk_ast::visitor::walk_expr(self, e);
        }
    }

    let span = fresh_span();
    let program = Program {
        functions: vec![],
        types: vec![TypeDecl {
            name: "P".to_owned(),
            params: vec![],
            parent: None,
            members: vec![Member {
                kind: MemberKind::Attribute {
                    name: "x".to_owned(),
                    type_ann: None,
                    value: num(1234.5, 1),
                },
                span: span.clone(),
            }],
            span: span.clone(),
        }],
        protocols: vec![],
        macros: vec![],
        body: num(0.0, 2),
    };

    let mut col = NumberCollector::default();
    col.visit_program(&program);
    assert!(col.0.contains(&1234.5));
}

/// Verifies that walk_type_decl visits parent-spec argument expressions.
#[test]
fn visitor_reaches_parent_spec_arguments() {
    #[derive(Default)]
    struct NumberCollector(Vec<f64>);
    impl Visitor for NumberCollector {
        fn visit_expr(&mut self, e: &Expr) {
            if let ExprKind::Number(n) = &e.kind {
                self.0.push(*n);
            }
            hulk_ast::visitor::walk_expr(self, e);
        }
    }

    let program = Program {
        functions: vec![],
        types: vec![TypeDecl {
            name: "Child".to_owned(),
            params: vec![],
            parent: Some(ParentSpec {
                name: "Parent".to_owned(),
                args: vec![num(99.0, 1), num(100.0, 2)],
                span: fresh_span(),
            }),
            members: vec![],
            span: fresh_span(),
        }],
        protocols: vec![],
        macros: vec![],
        body: num(0.0, 3),
    };

    let mut col = NumberCollector::default();
    col.visit_program(&program);
    assert!(col.0.contains(&99.0));
    assert!(col.0.contains(&100.0));
}

/// Verifies that Let.body is visited (regression test for the old combined
/// match that relied on a fragile re-match).
#[test]
fn visitor_visits_let_body_after_bindings() {
    #[derive(Default)]
    struct Ids(Vec<NodeId>);
    impl Visitor for Ids {
        fn visit_expr(&mut self, e: &Expr) {
            self.0.push(e.id);
            hulk_ast::visitor::walk_expr(self, e);
        }
    }

    let let_expr = expr(
        ExprKind::Let {
            bindings: vec![expr(
                ExprKind::LetBinding(LetBinding {
                    name: "a".to_owned(),
                    type_ann: None,
                    value: Box::new(num(1.0, 10)),
                    span: fresh_span(),
                }),
                5,
            )],
            // The body expression has id 99 — if the visitor skipped
            // the body we would never see this id.
            body: Box::new(num(42.0, 99)),
        },
        0,
    );

    let mut visited = Ids::default();
    visited.visit_expr(&let_expr);
    assert!(visited.0.contains(&NodeId(99)), "Let body was not visited");
    assert!(
        visited.0.contains(&NodeId(5)),
        "Let binding was not visited"
    );
}

/// Mutating visitor rewrites every number, reaching all nested locations.
#[test]
fn visitor_mut_transforms_deeply_nested_numbers() {
    struct AddOne;
    impl VisitorMut for AddOne {
        fn visit_expr_mut(&mut self, e: &mut Expr) {
            if let ExprKind::Number(n) = &mut e.kind {
                *n += 1.0;
            }
            hulk_ast::visitor::walk_expr_mut(self, e);
        }
    }

    let mut program = build_kitchen_sink_program();
    AddOne.visit_program_mut(&mut program);

    // Collect all numbers and ensure none are their original 0.0/1.0/2.0
    // (they must all have been bumped by at least 1).
    #[derive(Default)]
    struct Collect(Vec<f64>);
    impl Visitor for Collect {
        fn visit_expr(&mut self, e: &Expr) {
            if let ExprKind::Number(n) = &e.kind {
                self.0.push(*n);
            }
            hulk_ast::visitor::walk_expr(self, e);
        }
    }
    let mut col = Collect::default();
    col.visit_program(&program);

    // At least one of the numbers should have been transformed.
    assert!(col.0.iter().any(|n| *n >= 1.0));
    // Every number should be >= 1.0 after +1, since every original was >= 0.0.
    assert!(col.0.iter().all(|n| *n >= 1.0));
}
