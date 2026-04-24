//! Expr / ExprKind — every variant is constructible and keeps its fields.

use super::*;

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

        if let ExprKind::BinOp {
            op: got,
            left,
            right,
        } = &e.kind
        {
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
            ExprKind::UnaryOp {
                op,
                expr: Box::new(num(1.0, 100 + i as u32)),
            },
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
        ExprKind::MethodCall {
            method: name, args, ..
        } => {
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
