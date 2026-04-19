//! Integration tests exercising every variant and edge case of the AST.
//!
//! These tests construct ASTs by hand and verify that:
//! - every `ExprKind` variant is constructible and preserves its fields,
//! - every `BinOpKind`, `UnaryOpKind`, `TypeAnn` variant is constructible,
//! - every declaration kind (`FunctionDecl`, `TypeDecl`, `ProtocolDecl`, `MacroDecl`)
//!   round-trips through Clone/PartialEq,
//! - the `Visitor` and `VisitorMut` traits reach every node,
//! - edge cases (empty collections, deep nesting, multibyte identifiers, NodeId
//!   overflow) behave as expected.

use std::sync::Arc;

use hulk_ast::{
    AssignTarget, BinOpKind, Expr, ExprKind, FunctionDecl, LetBinding, MacroDecl, MacroParam,
    Member, MemberKind, MethodSig, NodeId, NodeIdGen, Param, ParentSpec, Program, ProtocolDecl,
    TypeAnn, TypeDecl, UnaryOpKind, Visitor, VisitorMut,
};
use hulk_span::{SourceFile, Span};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fresh_span() -> Span {
    let file = Arc::new(SourceFile::new("test.hulk", "..."));
    Span::new(file, 0, 3)
}

fn expr(kind: ExprKind, id: u32) -> Expr {
    Expr::new(kind, fresh_span(), NodeId(id))
}

fn num(n: f64, id: u32) -> Expr {
    expr(ExprKind::Number(n), id)
}

fn ident(name: &str, id: u32) -> Expr {
    expr(ExprKind::Ident(name.to_owned()), id)
}

// ---------------------------------------------------------------------------
// NodeIdGen
// ---------------------------------------------------------------------------

#[test]
fn node_id_gen_starts_from_zero_by_default() {
    let mut gen = NodeIdGen::new();
    assert_eq!(gen.next_id(), NodeId(0));
    assert_eq!(gen.next_id(), NodeId(1));
    assert_eq!(gen.next_id(), NodeId(2));
}

#[test]
fn node_id_gen_with_start_uses_the_given_offset() {
    let mut gen = NodeIdGen::with_start(100);
    assert_eq!(gen.next_id(), NodeId(100));
    assert_eq!(gen.next_id(), NodeId(101));
}

#[test]
fn node_id_gen_produces_unique_ids_for_many_nodes() {
    let mut gen = NodeIdGen::new();
    let mut seen = std::collections::HashSet::new();
    for _ in 0..10_000 {
        let id = gen.next_id();
        assert!(seen.insert(id), "duplicate id: {id:?}");
    }
    assert_eq!(seen.len(), 10_000);
}

#[test]
#[should_panic(expected = "NodeIdGen overflowed u32 range")]
fn node_id_gen_panics_on_overflow() {
    let mut gen = NodeIdGen::with_start(u32::MAX);
    let _last = gen.next_id(); // consumes u32::MAX, next call overflows
    let _ = gen.next_id();
}

#[test]
fn node_id_gen_state_is_cloneable_independently() {
    let mut a = NodeIdGen::new();
    a.next_id();
    a.next_id();
    let mut b = a.clone();
    assert_eq!(a.next_id(), NodeId(2));
    assert_eq!(b.next_id(), NodeId(2));
    assert_eq!(a.next_id(), NodeId(3));
    // Generators are independent after cloning.
    assert_eq!(b.next_id(), NodeId(3));
}

#[test]
fn node_id_is_copy_and_hashable() {
    let id = NodeId(42);
    let copied = id;
    let _ = id;
    assert_eq!(copied, NodeId(42));

    let mut map = std::collections::HashMap::new();
    map.insert(NodeId(1), "one");
    map.insert(NodeId(2), "two");
    assert_eq!(map.get(&NodeId(1)), Some(&"one"));
}

#[test]
fn node_ids_are_orderable() {
    // The derived Ord must put smaller ids first — needed by any code that
    // sorts nodes by generation order.
    let mut ids = vec![NodeId(5), NodeId(1), NodeId(3)];
    ids.sort();
    assert_eq!(ids, vec![NodeId(1), NodeId(3), NodeId(5)]);
}

// ---------------------------------------------------------------------------
// Expr / ExprKind — every variant is constructible and keeps its fields.
// ---------------------------------------------------------------------------

#[test]
fn all_literal_and_atom_variants_are_constructible() {
    let forms = [
        ExprKind::Number(0.0),
        ExprKind::Number(f64::INFINITY),
        ExprKind::Number(f64::NEG_INFINITY),
        ExprKind::StringLit(String::new()),
        ExprKind::StringLit("hello".to_owned()),
        ExprKind::Bool(true),
        ExprKind::Bool(false),
        ExprKind::Ident("x".to_owned()),
        ExprKind::Self_,
        ExprKind::Base,
    ];

    for (i, kind) in forms.into_iter().enumerate() {
        let e = expr(kind, i as u32);
        assert_eq!(e.id, NodeId(i as u32));
    }
}

#[test]
fn number_nan_is_representable_but_not_equal_to_itself() {
    // f64 NaN != NaN, so two ExprKind::Number(NaN) are never PartialEq.
    // This is a quirk but intentional — surface it so the parser never relies
    // on structural equality of NaN-bearing expressions.
    let a = ExprKind::Number(f64::NAN);
    let b = ExprKind::Number(f64::NAN);
    assert_ne!(a, b);
}

#[test]
fn all_binop_kinds_construct_and_preserve_operands() {
    let ops = [
        BinOpKind::Add,
        BinOpKind::Sub,
        BinOpKind::Mul,
        BinOpKind::Div,
        BinOpKind::Mod,
        BinOpKind::Pow,
        BinOpKind::Concat,
        BinOpKind::ConcatSpaced,
        BinOpKind::Lt,
        BinOpKind::Le,
        BinOpKind::Gt,
        BinOpKind::Ge,
        BinOpKind::Eq,
        BinOpKind::Ne,
        BinOpKind::And,
        BinOpKind::Or,
    ];

    for (i, op) in ops.iter().copied().enumerate() {
        let e = expr(
            ExprKind::BinOp {
                op,
                left: Box::new(num(1.0, 100 + i as u32)),
                right: Box::new(num(2.0, 200 + i as u32)),
            },
            i as u32,
        );

        if let ExprKind::BinOp { op: got, left, right } = &e.kind {
            assert_eq!(*got, op);
            assert!(matches!(left.kind, ExprKind::Number(_)));
            assert!(matches!(right.kind, ExprKind::Number(_)));
        } else {
            panic!("expected BinOp for {op:?}");
        }
    }
}

#[test]
fn all_unary_op_kinds_are_constructible() {
    for (i, op) in [UnaryOpKind::Neg, UnaryOpKind::Not].into_iter().enumerate() {
        let e = expr(
            ExprKind::UnaryOp { op, expr: Box::new(num(1.0, 100 + i as u32)) },
            i as u32,
        );
        if let ExprKind::UnaryOp { op: got, .. } = e.kind {
            assert_eq!(got, op);
        } else {
            panic!("expected UnaryOp");
        }
    }
}

#[test]
fn call_and_method_call_distinguish_callee_and_receiver() {
    let call = expr(
        ExprKind::Call {
            callee: Box::new(ident("f", 1)),
            args: vec![num(1.0, 2), num(2.0, 3)],
        },
        0,
    );
    let method = expr(
        ExprKind::MethodCall {
            receiver: Box::new(ident("obj", 1)),
            method: "m".to_owned(),
            args: vec![num(1.0, 2)],
        },
        0,
    );

    match call.kind {
        ExprKind::Call { args, .. } => assert_eq!(args.len(), 2),
        _ => panic!(),
    }
    match method.kind {
        ExprKind::MethodCall { method: name, args, .. } => {
            assert_eq!(name, "m");
            assert_eq!(args.len(), 1);
        }
        _ => panic!(),
    }
}

#[test]
fn field_access_and_index_preserve_their_receivers() {
    let field = expr(
        ExprKind::FieldAccess {
            receiver: Box::new(ident("p", 1)),
            field: "x".to_owned(),
        },
        0,
    );
    let index = expr(
        ExprKind::Index {
            target: Box::new(ident("v", 1)),
            index: Box::new(num(3.0, 2)),
        },
        0,
    );

    if let ExprKind::FieldAccess { field: f, .. } = field.kind {
        assert_eq!(f, "x");
    } else {
        panic!("expected FieldAccess");
    }
    if let ExprKind::Index { .. } = index.kind {
        // ok
    } else {
        panic!("expected Index");
    }
}

#[test]
fn empty_block_and_vec_literal_are_valid() {
    // A block with no expressions is legal in the AST (semantic phase may
    // reject it). Make sure it is constructible.
    let _ = expr(ExprKind::Block(vec![]), 0);
    let _ = expr(ExprKind::VecLiteral(vec![]), 1);
}

#[test]
fn vec_generator_preserves_binding_and_iterable() {
    let gen = expr(
        ExprKind::VecGenerator {
            element: Box::new(ident("x", 1)),
            binding: "x".to_owned(),
            iterable: Box::new(ident("range", 2)),
        },
        0,
    );
    if let ExprKind::VecGenerator { binding, .. } = gen.kind {
        assert_eq!(binding, "x");
    } else {
        panic!("expected VecGenerator");
    }
}

#[test]
fn let_expression_keeps_bindings_and_body() {
    let let_expr = expr(
        ExprKind::Let {
            bindings: vec![expr(
                ExprKind::LetBinding(LetBinding {
                    name: "a".to_owned(),
                    type_ann: None,
                    value: Box::new(num(1.0, 10)),
                    span: fresh_span(),
                }),
                1,
            )],
            body: Box::new(ident("a", 2)),
        },
        0,
    );

    if let ExprKind::Let { bindings, body } = let_expr.kind {
        assert_eq!(bindings.len(), 1);
        assert!(matches!(body.kind, ExprKind::Ident(_)));
    } else {
        panic!("expected Let");
    }
}

#[test]
fn assignment_with_all_target_forms_is_constructible() {
    let targets = [
        AssignTarget::Ident("x".to_owned()),
        AssignTarget::Field {
            receiver: Box::new(ident("p", 10)),
            field: "x".to_owned(),
        },
        AssignTarget::Index {
            target: Box::new(ident("v", 20)),
            index: Box::new(num(0.0, 21)),
        },
    ];

    for (i, target) in targets.into_iter().enumerate() {
        let e = expr(
            ExprKind::Assign {
                target: Box::new(expr(ExprKind::AssignTarget(target), i as u32 + 100)),
                value: Box::new(num(42.0, i as u32 + 200)),
            },
            i as u32,
        );
        if let ExprKind::Assign { .. } = e.kind {
            // ok
        } else {
            panic!();
        }
    }
}

#[test]
fn if_expression_supports_zero_to_many_elif_branches() {
    // Zero elif, no else (still valid syntactically).
    let no_elif = expr(
        ExprKind::If {
            condition: Box::new(ident("c", 1)),
            then_branch: Box::new(num(1.0, 2)),
            elif_branches: vec![],
            else_branch: None,
        },
        0,
    );
    // Many elif branches + else.
    let many = expr(
        ExprKind::If {
            condition: Box::new(ident("c", 1)),
            then_branch: Box::new(num(1.0, 2)),
            elif_branches: (0..5)
                .map(|i| (ident("c", 100 + i), num(i as f64, 200 + i)))
                .collect(),
            else_branch: Some(Box::new(num(999.0, 300))),
        },
        0,
    );

    if let ExprKind::If { elif_branches, else_branch, .. } = no_elif.kind {
        assert!(elif_branches.is_empty());
        assert!(else_branch.is_none());
    } else {
        panic!();
    }
    if let ExprKind::If { elif_branches, else_branch, .. } = many.kind {
        assert_eq!(elif_branches.len(), 5);
        assert!(else_branch.is_some());
    } else {
        panic!();
    }
}

#[test]
fn while_and_for_keep_their_body_and_iterable() {
    let w = expr(
        ExprKind::While {
            condition: Box::new(ident("c", 1)),
            body: Box::new(num(1.0, 2)),
        },
        0,
    );
    let f = expr(
        ExprKind::For {
            binding: "x".to_owned(),
            iterable: Box::new(ident("range", 1)),
            body: Box::new(ident("x", 2)),
        },
        0,
    );
    if let ExprKind::While { .. } = w.kind {} else { panic!() }
    if let ExprKind::For { binding, .. } = f.kind {
        assert_eq!(binding, "x");
    } else {
        panic!()
    }
}

#[test]
fn new_is_and_as_carry_type_annotations() {
    let new = expr(
        ExprKind::New {
            type_ann: TypeAnn::Named("Point".to_owned()),
            args: vec![num(1.0, 1), num(2.0, 2)],
        },
        0,
    );
    let is = expr(
        ExprKind::Is {
            expr: Box::new(ident("x", 1)),
            type_ann: TypeAnn::Named("Point".to_owned()),
        },
        0,
    );
    let as_ = expr(
        ExprKind::As {
            expr: Box::new(ident("x", 1)),
            type_ann: TypeAnn::Named("Point".to_owned()),
        },
        0,
    );

    assert!(matches!(new.kind, ExprKind::New { .. }));
    assert!(matches!(is.kind, ExprKind::Is { .. }));
    assert!(matches!(as_.kind, ExprKind::As { .. }));
}

#[test]
fn lambda_keeps_params_return_type_and_body() {
    let lambda = expr(
        ExprKind::Lambda {
            params: vec![Param {
                name: "n".to_owned(),
                type_ann: Some(TypeAnn::Named("Number".to_owned())),
                span: fresh_span(),
            }],
            return_type: Some(TypeAnn::Named("Boolean".to_owned())),
            body: Box::new(ident("n", 1)),
        },
        0,
    );
    if let ExprKind::Lambda {
        params,
        return_type,
        ..
    } = lambda.kind
    {
        assert_eq!(params.len(), 1);
        assert_eq!(return_type, Some(TypeAnn::Named("Boolean".to_owned())));
    } else {
        panic!();
    }
}

// ---------------------------------------------------------------------------
// TypeAnn variants including deep nesting.
// ---------------------------------------------------------------------------

#[test]
fn type_ann_supports_arbitrary_nesting() {
    // Number[][] (vector of vectors)
    let nested_vec = TypeAnn::Vector(Box::new(TypeAnn::Vector(Box::new(TypeAnn::Named(
        "Number".to_owned(),
    )))));
    assert!(matches!(nested_vec, TypeAnn::Vector(_)));

    // Number*[] (iterable returning vectors) — syntactically allowed by the AST
    let iter_of_vec = TypeAnn::Iterable(Box::new(TypeAnn::Vector(Box::new(TypeAnn::Named(
        "Number".to_owned(),
    )))));
    assert!(matches!(iter_of_vec, TypeAnn::Iterable(_)));

    // (Number, Number) -> Boolean
    let functor = TypeAnn::Functor {
        params: vec![
            TypeAnn::Named("Number".to_owned()),
            TypeAnn::Named("Number".to_owned()),
        ],
        ret: Box::new(TypeAnn::Named("Boolean".to_owned())),
    };
    assert!(matches!(functor, TypeAnn::Functor { .. }));

    // (Number*) -> Number[]
    let high_order = TypeAnn::Functor {
        params: vec![TypeAnn::Iterable(Box::new(TypeAnn::Named(
            "Number".to_owned(),
        )))],
        ret: Box::new(TypeAnn::Vector(Box::new(TypeAnn::Named(
            "Number".to_owned(),
        )))),
    };
    assert!(matches!(high_order, TypeAnn::Functor { .. }));
}

#[test]
fn functor_with_zero_params_is_representable() {
    let thunk = TypeAnn::Functor {
        params: vec![],
        ret: Box::new(TypeAnn::Named("Object".to_owned())),
    };
    if let TypeAnn::Functor { params, .. } = thunk {
        assert!(params.is_empty());
    } else {
        panic!();
    }
}

// ---------------------------------------------------------------------------
// Declarations: FunctionDecl, TypeDecl, ProtocolDecl, MacroDecl.
// ---------------------------------------------------------------------------

#[test]
fn function_decl_preserves_return_type_annotation() {
    let f = FunctionDecl {
        name: "tan".to_owned(),
        params: vec![Param {
            name: "x".to_owned(),
            type_ann: Some(TypeAnn::Named("Number".to_owned())),
            span: fresh_span(),
        }],
        return_type: Some(TypeAnn::Named("Number".to_owned())),
        body: num(0.0, 1),
        span: fresh_span(),
    };
    assert_eq!(f.return_type, Some(TypeAnn::Named("Number".to_owned())));

    let no_return = FunctionDecl {
        name: "id".to_owned(),
        params: vec![],
        return_type: None,
        body: num(0.0, 1),
        span: fresh_span(),
    };
    assert!(no_return.return_type.is_none());
}

#[test]
fn type_decl_supports_inheritance_and_mixed_members() {
    let parent = ParentSpec {
        name: "Point".to_owned(),
        args: vec![num(0.0, 1), num(0.0, 2)],
        span: fresh_span(),
    };
    let attr = Member {
        kind: MemberKind::Attribute {
            name: "x".to_owned(),
            type_ann: Some(TypeAnn::Named("Number".to_owned())),
            value: num(0.0, 3),
        },
        span: fresh_span(),
    };
    let method = Member {
        kind: MemberKind::Method(FunctionDecl {
            name: "getX".to_owned(),
            params: vec![],
            return_type: Some(TypeAnn::Named("Number".to_owned())),
            body: ident("self", 4),
            span: fresh_span(),
        }),
        span: fresh_span(),
    };

    let type_decl = TypeDecl {
        name: "PolarPoint".to_owned(),
        params: vec![Param {
            name: "phi".to_owned(),
            type_ann: None,
            span: fresh_span(),
        }],
        parent: Some(parent),
        members: vec![attr, method],
        span: fresh_span(),
    };

    assert_eq!(type_decl.members.len(), 2);
    assert!(type_decl.parent.is_some());
    assert!(matches!(
        type_decl.members[0].kind,
        MemberKind::Attribute { .. }
    ));
    assert!(matches!(
        type_decl.members[1].kind,
        MemberKind::Method(_)
    ));
}

#[test]
fn attribute_value_is_required_not_optional() {
    // This test encodes the fact that attributes MUST have an initializer
    // (per Hulk.md "Types" section). If someone tries to make `value`
    // optional again, the type of the field will change and this test
    // will fail at compile time.
    let attr = MemberKind::Attribute {
        name: "x".to_owned(),
        type_ann: None,
        value: num(42.0, 1),
    };
    // Destructure to force a compile-time check that `value` is `Expr`, not
    // `Option<Expr>`.
    if let MemberKind::Attribute { value, .. } = attr {
        // If value were Option<Expr>, the next line wouldn't compile.
        let _kind: &ExprKind = &value.kind;
    } else {
        panic!();
    }
}

#[test]
fn protocol_decl_supports_zero_or_many_extensions() {
    let p0 = ProtocolDecl {
        name: "Hashable".to_owned(),
        extends: vec![],
        methods: vec![MethodSig {
            name: "hash".to_owned(),
            params: vec![],
            return_type: TypeAnn::Named("Number".to_owned()),
            span: fresh_span(),
        }],
        span: fresh_span(),
    };
    let p1 = ProtocolDecl {
        name: "Equatable".to_owned(),
        extends: vec!["Hashable".to_owned()],
        methods: vec![],
        span: fresh_span(),
    };
    assert!(p0.extends.is_empty());
    assert_eq!(p1.extends, vec!["Hashable".to_owned()]);
}

#[test]
fn method_sig_requires_return_type_annotation() {
    // Compile-time check: return_type on MethodSig is TypeAnn, not Option.
    // Protocols cannot have unannotated return types per the spec.
    let sig = MethodSig {
        name: "next".to_owned(),
        params: vec![],
        return_type: TypeAnn::Named("Boolean".to_owned()),
        span: fresh_span(),
    };
    let _: &TypeAnn = &sig.return_type;
}

#[test]
fn macro_decl_supports_all_four_param_kinds() {
    let mac = MacroDecl {
        name: "repeat".to_owned(),
        params: vec![
            MacroParam::Placeholder {
                name: "iter".to_owned(),
                type_ann: TypeAnn::Named("Number".to_owned()),
                span: fresh_span(),
            },
            MacroParam::Regular {
                name: "n".to_owned(),
                type_ann: TypeAnn::Named("Number".to_owned()),
                span: fresh_span(),
            },
            MacroParam::Symbolic {
                name: "target".to_owned(),
                type_ann: TypeAnn::Named("Object".to_owned()),
                span: fresh_span(),
            },
            MacroParam::Body {
                name: "expr".to_owned(),
                type_ann: TypeAnn::Named("Object".to_owned()),
                span: fresh_span(),
            },
        ],
        body: num(0.0, 1),
        span: fresh_span(),
    };

    assert_eq!(mac.params.len(), 4);
    assert_eq!(mac.params[0].name(), "iter");
    assert_eq!(mac.params[1].name(), "n");
    assert_eq!(mac.params[2].name(), "target");
    assert_eq!(mac.params[3].name(), "expr");

    for p in &mac.params {
        assert_eq!(*p.type_ann(), p.type_ann().clone());
    }
}

#[test]
fn macro_param_name_and_type_ann_are_accessible_uniformly() {
    let span = fresh_span();
    let cases = [
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
        MacroParam::Symbolic {
            name: "c".to_owned(),
            type_ann: TypeAnn::Named("Object".to_owned()),
            span: span.clone(),
        },
        MacroParam::Placeholder {
            name: "d".to_owned(),
            type_ann: TypeAnn::Named("Number".to_owned()),
            span,
        },
    ];
    let names: Vec<_> = cases.iter().map(MacroParam::name).collect();
    assert_eq!(names, vec!["a", "b", "c", "d"]);
}

// ---------------------------------------------------------------------------
// Visitor / VisitorMut — exhaustive coverage.
// ---------------------------------------------------------------------------

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

/// Factory that produces `Expr` values with monotonically increasing NodeIds.
///
/// Wrapping the generator in a struct with a single `&mut self` method avoids
/// the borrow-checker conflicts that arise when trying to call the same
/// closure multiple times in the same expression.
struct ExprFactory {
    gen: NodeIdGen,
}

impl ExprFactory {
    fn new() -> Self {
        Self {
            gen: NodeIdGen::new(),
        }
    }

    fn e(&mut self, kind: ExprKind) -> Expr {
        Expr::new(kind, fresh_span(), self.gen.next_id())
    }

    fn eb(&mut self, kind: ExprKind) -> Box<Expr> {
        Box::new(self.e(kind))
    }
}

fn build_kitchen_sink_program() -> Program {
    let mut f = ExprFactory::new();

    // Atoms used repeatedly.
    let ident_x_eb = |f: &mut ExprFactory| f.eb(ExprKind::Ident("x".to_owned()));
    let num_0_eb = |f: &mut ExprFactory| f.eb(ExprKind::Number(0.0));
    let num_1_eb = |f: &mut ExprFactory| f.eb(ExprKind::Number(1.0));

    // --- atoms & assignment -----------------------------------------------
    let assign_target = f.eb(ExprKind::AssignTarget(AssignTarget::Ident("x".to_owned())));
    let assign_value = num_1_eb(&mut f);
    let assign = f.e(ExprKind::Assign {
        target: assign_target,
        value: assign_value,
    });

    // --- let --------------------------------------------------------------
    let binding_value = num_0_eb(&mut f);
    let let_binding = f.e(ExprKind::LetBinding(LetBinding {
        name: "x".to_owned(),
        type_ann: None,
        value: binding_value,
        span: fresh_span(),
    }));
    let body_let = f.e(ExprKind::Let {
        bindings: vec![let_binding],
        body: Box::new(assign),
    });

    // --- if with elif+else ------------------------------------------------
    let if_cond = f.eb(ExprKind::Bool(true));
    let if_then = f.eb(ExprKind::Self_);
    let elif_cond = f.e(ExprKind::Bool(false));
    let elif_body = f.e(ExprKind::Base);
    let else_branch = f.eb(ExprKind::StringLit("s".to_owned()));
    let if_expr = f.e(ExprKind::If {
        condition: if_cond,
        then_branch: if_then,
        elif_branches: vec![(elif_cond, elif_body)],
        else_branch: Some(else_branch),
    });

    // --- while ------------------------------------------------------------
    let while_cond = f.eb(ExprKind::Bool(true));
    let while_body = ident_x_eb(&mut f);
    let while_expr = f.e(ExprKind::While {
        condition: while_cond,
        body: while_body,
    });

    // --- for --------------------------------------------------------------
    let for_iter = f.eb(ExprKind::Ident("range".to_owned()));
    let for_body = num_1_eb(&mut f);
    let for_expr = f.e(ExprKind::For {
        binding: "x".to_owned(),
        iterable: for_iter,
        body: for_body,
    });

    // --- binop ------------------------------------------------------------
    let binop_l = num_1_eb(&mut f);
    let binop_r = f.eb(ExprKind::Number(2.0));
    let binop = f.e(ExprKind::BinOp {
        op: BinOpKind::Add,
        left: binop_l,
        right: binop_r,
    });

    // --- unary ------------------------------------------------------------
    let unary_inner = num_1_eb(&mut f);
    let unary = f.e(ExprKind::UnaryOp {
        op: UnaryOpKind::Neg,
        expr: unary_inner,
    });

    // --- call -------------------------------------------------------------
    let call_callee = f.eb(ExprKind::Ident("f".to_owned()));
    let call_arg = num_1_eb(&mut f);
    let call = f.e(ExprKind::Call {
        callee: call_callee,
        args: vec![*call_arg],
    });

    // --- method call ------------------------------------------------------
    let method_recv = f.eb(ExprKind::Ident("obj".to_owned()));
    let method_arg = num_1_eb(&mut f);
    let method_call = f.e(ExprKind::MethodCall {
        receiver: method_recv,
        method: "m".to_owned(),
        args: vec![*method_arg],
    });

    // --- field access & index --------------------------------------------
    let field_recv = f.eb(ExprKind::Ident("p".to_owned()));
    let field = f.e(ExprKind::FieldAccess {
        receiver: field_recv,
        field: "x".to_owned(),
    });
    let index_target = f.eb(ExprKind::Ident("v".to_owned()));
    let index_index = num_0_eb(&mut f);
    let index = f.e(ExprKind::Index {
        target: index_target,
        index: index_index,
    });

    // --- vec literal & generator -----------------------------------------
    let vec_items = vec![*num_1_eb(&mut f), *f.eb(ExprKind::Number(2.0))];
    let vec_lit = f.e(ExprKind::VecLiteral(vec_items));

    let vec_gen_element = ident_x_eb(&mut f);
    let vec_gen_iter = f.eb(ExprKind::Ident("range".to_owned()));
    let vec_gen = f.e(ExprKind::VecGenerator {
        element: vec_gen_element,
        binding: "x".to_owned(),
        iterable: vec_gen_iter,
    });

    // --- new / is / as / lambda ------------------------------------------
    let new_arg = num_0_eb(&mut f);
    let new_obj = f.e(ExprKind::New {
        type_ann: TypeAnn::Named("Point".to_owned()),
        args: vec![*new_arg],
    });
    let is_inner = ident_x_eb(&mut f);
    let is = f.e(ExprKind::Is {
        expr: is_inner,
        type_ann: TypeAnn::Named("Point".to_owned()),
    });
    let as_inner = ident_x_eb(&mut f);
    let as_expr = f.e(ExprKind::As {
        expr: as_inner,
        type_ann: TypeAnn::Named("Point".to_owned()),
    });
    let lambda_body = f.eb(ExprKind::Ident("n".to_owned()));
    let lambda = f.e(ExprKind::Lambda {
        params: vec![Param {
            name: "n".to_owned(),
            type_ann: Some(TypeAnn::Named("Number".to_owned())),
            span: fresh_span(),
        }],
        return_type: Some(TypeAnn::Named("Number".to_owned())),
        body: lambda_body,
    });

    let block = f.e(ExprKind::Block(vec![
        body_let, if_expr, while_expr, for_expr, binop, unary, call, method_call, field, index,
        vec_lit, vec_gen, new_obj, is, as_expr, lambda,
    ]));

    // --- declarations -----------------------------------------------------
    let fn_body = ident_x_eb(&mut f);
    let attr_value = num_0_eb(&mut f);
    let macro_body = num_0_eb(&mut f);

    Program {
        functions: vec![FunctionDecl {
            name: "id".to_owned(),
            params: vec![Param {
                name: "x".to_owned(),
                type_ann: Some(TypeAnn::Named("Number".to_owned())),
                span: fresh_span(),
            }],
            return_type: Some(TypeAnn::Named("Number".to_owned())),
            body: *fn_body,
            span: fresh_span(),
        }],
        types: vec![TypeDecl {
            name: "Point".to_owned(),
            params: vec![],
            parent: None,
            members: vec![Member {
                kind: MemberKind::Attribute {
                    name: "x".to_owned(),
                    type_ann: Some(TypeAnn::Named("Number".to_owned())),
                    value: *attr_value,
                },
                span: fresh_span(),
            }],
            span: fresh_span(),
        }],
        protocols: vec![ProtocolDecl {
            name: "Iterable".to_owned(),
            extends: vec![],
            methods: vec![MethodSig {
                name: "next".to_owned(),
                params: vec![],
                return_type: TypeAnn::Named("Boolean".to_owned()),
                span: fresh_span(),
            }],
            span: fresh_span(),
        }],
        macros: vec![MacroDecl {
            name: "noop".to_owned(),
            params: vec![MacroParam::Body {
                name: "expr".to_owned(),
                type_ann: TypeAnn::Named("Object".to_owned()),
                span: fresh_span(),
            }],
            body: *macro_body,
            span: fresh_span(),
        }],
        body: block,
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
    assert!(visited.0.contains(&NodeId(5)), "Let binding was not visited");
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

// ---------------------------------------------------------------------------
// Robustness: deep nesting, multibyte strings, edge cases.
// ---------------------------------------------------------------------------

#[test]
fn deeply_nested_binop_does_not_blow_node_ids() {
    let mut gen = NodeIdGen::new();
    let mut current = Expr::new(ExprKind::Number(0.0), fresh_span(), gen.next_id());
    for _ in 0..500 {
        let next = Expr::new(ExprKind::Number(1.0), fresh_span(), gen.next_id());
        current = Expr::new(
            ExprKind::BinOp {
                op: BinOpKind::Add,
                left: Box::new(current),
                right: Box::new(next),
            },
            fresh_span(),
            gen.next_id(),
        );
    }

    // Count depth via a visitor to make sure walking does not stack-overflow
    // for modest nesting.
    struct DepthCounter(usize);
    impl Visitor for DepthCounter {
        fn visit_expr(&mut self, e: &Expr) {
            self.0 += 1;
            hulk_ast::visitor::walk_expr(self, e);
        }
    }
    let mut dc = DepthCounter(0);
    dc.visit_expr(&current);
    // 500 BinOps + 500 right-hand numbers + 1 leftmost = 1001
    assert_eq!(dc.0, 1001);
}

#[test]
fn string_literal_preserves_multibyte_content() {
    // The AST must be able to hold any UTF-8 string produced by the lexer.
    let kind = ExprKind::StringLit("héllo 🦀 ñ".to_owned());
    let e = expr(kind, 0);
    if let ExprKind::StringLit(s) = e.kind {
        assert_eq!(s, "héllo 🦀 ñ");
    } else {
        panic!();
    }
}

#[test]
fn identifier_preserves_ascii_and_digits() {
    // HULK identifiers are ASCII alphanumeric + underscore (not leading).
    let e = expr(ExprKind::Ident("x_0_TitleCase".to_owned()), 0);
    if let ExprKind::Ident(n) = e.kind {
        assert_eq!(n, "x_0_TitleCase");
    } else {
        panic!();
    }
}

#[test]
fn clone_produces_structurally_equal_program() {
    let program = build_kitchen_sink_program();
    let cloned = program.clone();
    assert_eq!(program, cloned);
}

#[test]
fn empty_program_is_representable() {
    let p = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![],
        body: num(0.0, 0),
    };
    assert!(p.functions.is_empty());
    assert!(p.types.is_empty());
    assert!(p.protocols.is_empty());
    assert!(p.macros.is_empty());
}

#[test]
fn node_ids_in_kitchen_sink_are_all_distinct() {
    let program = build_kitchen_sink_program();

    #[derive(Default)]
    struct IdCollector(Vec<NodeId>);
    impl Visitor for IdCollector {
        fn visit_expr(&mut self, e: &Expr) {
            self.0.push(e.id);
            hulk_ast::visitor::walk_expr(self, e);
        }
    }

    let mut ids = IdCollector::default();
    ids.visit_program(&program);

    let unique: std::collections::HashSet<_> = ids.0.iter().copied().collect();
    assert_eq!(
        ids.0.len(),
        unique.len(),
        "duplicate NodeIds in kitchen sink program"
    );
}
