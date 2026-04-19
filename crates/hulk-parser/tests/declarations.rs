//! Integration tests for subsession 4.2: declarations and complex constructs.
//!
//! Each test exercises a specific syntactic form from Hulk.md and verifies the
//! resulting AST structurally. The tests do NOT assert on spans (spans are
//! tested separately) and they always check that the parser produces no
//! diagnostics on well-formed input.

use hulk_ast::{
    AssignTarget, BinOpKind, Expr, ExprKind, MacroParam, MemberKind, Param, Program, TypeAnn,
    UnaryOpKind,
};
use hulk_diagnostics::DiagnosticBag;
use hulk_lexer::lex;
use hulk_parser::parse;
use hulk_tokens::SourceFile;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_ok(source: &str) -> Program {
    let file = SourceFile::new("test.hulk", source);
    let mut lex_bag = DiagnosticBag::new();
    let tokens = lex(&file, &mut lex_bag);
    assert!(
        lex_bag.is_empty(),
        "lexer diagnostics on `{source}`: {:?}",
        lex_bag.diagnostics()
    );
    let (program, bag) = parse(tokens, &file);
    assert!(
        bag.is_empty(),
        "parser diagnostics on `{source}`: {:?}",
        bag.diagnostics()
    );
    program
}

fn parse_with_errors(source: &str) -> (Program, DiagnosticBag) {
    let file = SourceFile::new("test.hulk", source);
    let mut lex_bag = DiagnosticBag::new();
    let tokens = lex(&file, &mut lex_bag);
    let (program, bag) = parse(tokens, &file);
    (program, bag)
}

fn body(program: &Program) -> &Expr {
    &program.body
}

// ---------------------------------------------------------------------------
// let ... in ...
// ---------------------------------------------------------------------------

#[test]
fn let_single_binding_without_annotation() {
    let program = parse_ok("let x = 42 in x;");
    let ExprKind::Let { bindings, body } = &body(&program).kind else {
        panic!("expected Let, got {:?}", body(&program).kind);
    };
    assert_eq!(bindings.len(), 1);
    let ExprKind::LetBinding(binding) = &bindings[0].kind else {
        panic!("binding not wrapped in LetBinding");
    };
    assert_eq!(binding.name, "x");
    assert!(binding.type_ann.is_none());
    assert!(matches!(binding.value.kind, ExprKind::Number(_)));
    assert!(matches!(body.kind, ExprKind::Ident(ref n) if n == "x"));
}

#[test]
fn let_with_type_annotation() {
    let program = parse_ok("let x: Number = 42 in x;");
    let ExprKind::Let { bindings, .. } = &body(&program).kind else {
        panic!("expected Let");
    };
    let ExprKind::LetBinding(binding) = &bindings[0].kind else {
        panic!()
    };
    assert_eq!(binding.type_ann, Some(TypeAnn::Named("Number".into())));
}

#[test]
fn let_multiple_bindings_in_one_let() {
    // Per spec: `let a = 6, b = a * 7 in print(b);` — multiple bindings in a
    // single let, later bindings may reference earlier ones.
    let program = parse_ok("let a = 6, b = 7 in a + b;");
    let ExprKind::Let { bindings, .. } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(bindings.len(), 2);
    let names: Vec<String> = bindings
        .iter()
        .map(|e| {
            if let ExprKind::LetBinding(b) = &e.kind {
                b.name.clone()
            } else {
                panic!("expected LetBinding")
            }
        })
        .collect();
    assert_eq!(names, vec!["a", "b"]);
}

#[test]
fn let_with_block_body() {
    let program = parse_ok("let x = 1 in { x; x + 1; };");
    let ExprKind::Let { body: let_body, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(let_body.kind, ExprKind::Block(_)));
}

#[test]
fn nested_let_right_associative() {
    // let a = 1 in let b = 2 in a + b
    let program = parse_ok("let a = 1 in let b = 2 in a + b;");
    let ExprKind::Let { body: outer_body, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(outer_body.kind, ExprKind::Let { .. }));
}

// ---------------------------------------------------------------------------
// if / elif / else
// ---------------------------------------------------------------------------

#[test]
fn if_else_returns_value() {
    let program = parse_ok("if (true) 1 else 2;");
    let ExprKind::If {
        elif_branches,
        else_branch,
        ..
    } = &body(&program).kind
    else {
        panic!()
    };
    assert!(elif_branches.is_empty());
    assert!(else_branch.is_some());
}

#[test]
fn if_with_elif_chain() {
    let program = parse_ok(r#"if (a) 1 elif (b) 2 elif (c) 3 else 4;"#);
    let ExprKind::If {
        elif_branches,
        else_branch,
        ..
    } = &body(&program).kind
    else {
        panic!()
    };
    assert_eq!(elif_branches.len(), 2);
    assert!(else_branch.is_some());
}

#[test]
fn if_without_else_is_legal() {
    let program = parse_ok("if (true) 1;");
    let ExprKind::If { else_branch, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(else_branch.is_none());
}

#[test]
fn if_with_block_branches() {
    let program = parse_ok(r#"if (true) { 1; 2; } else { 3; };"#);
    let ExprKind::If {
        then_branch,
        else_branch,
        ..
    } = &body(&program).kind
    else {
        panic!()
    };
    assert!(matches!(then_branch.kind, ExprKind::Block(_)));
    assert!(matches!(
        else_branch.as_deref().unwrap().kind,
        ExprKind::Block(_)
    ));
}

// ---------------------------------------------------------------------------
// while / for
// ---------------------------------------------------------------------------

#[test]
fn while_with_destructive_assignment() {
    let program = parse_ok("let a = 10 in while (a > 0) a := a - 1;");
    let ExprKind::Let { body: let_body, .. } = &body(&program).kind else {
        panic!()
    };
    let ExprKind::While { condition, body: while_body } = &let_body.kind else {
        panic!()
    };
    assert!(matches!(
        condition.kind,
        ExprKind::BinOp {
            op: BinOpKind::Gt,
            ..
        }
    ));
    assert!(matches!(while_body.kind, ExprKind::Assign { .. }));
}

#[test]
fn for_in_range_iterates_binding() {
    let program = parse_ok("for (x in range(0, 10)) print(x);");
    let ExprKind::For { binding, iterable, body: for_body } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(binding, "x");
    assert!(matches!(iterable.kind, ExprKind::Call { .. }));
    assert!(matches!(for_body.kind, ExprKind::Call { .. }));
}

// ---------------------------------------------------------------------------
// := destructive assignment
// ---------------------------------------------------------------------------

#[test]
fn assign_to_ident() {
    let program = parse_ok("x := 1;");
    let ExprKind::Assign { target, value } = &body(&program).kind else {
        panic!()
    };
    let ExprKind::AssignTarget(target_inner) = &target.kind else {
        panic!()
    };
    assert_eq!(*target_inner, AssignTarget::Ident("x".into()));
    assert!(matches!(value.kind, ExprKind::Number(_)));
}

#[test]
fn assign_to_field() {
    let program = parse_ok("p.x := 1;");
    let ExprKind::Assign { target, .. } = &body(&program).kind else {
        panic!()
    };
    let ExprKind::AssignTarget(AssignTarget::Field { field, .. }) = &target.kind else {
        panic!()
    };
    assert_eq!(field, "x");
}

#[test]
fn assign_to_index() {
    let program = parse_ok("v[0] := 1;");
    let ExprKind::Assign { target, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(
        target.kind,
        ExprKind::AssignTarget(AssignTarget::Index { .. })
    ));
}

#[test]
fn assign_right_associative() {
    // a := b := 1 parses as a := (b := 1)
    let program = parse_ok("a := b := 1;");
    let ExprKind::Assign { value: outer_value, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(outer_value.kind, ExprKind::Assign { .. }));
}

#[test]
fn assign_to_invalid_target_emits_diagnostic() {
    let (_, bag) = parse_with_errors("(1 + 2) := 3;");
    assert!(bag.has_errors(), "expected diagnostic for invalid := target");
}

// ---------------------------------------------------------------------------
// Postfix: field access, method call, index, is/as, call
// ---------------------------------------------------------------------------

#[test]
fn field_access_chain() {
    let program = parse_ok("p.x.y;");
    let ExprKind::FieldAccess { receiver, field } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(field, "y");
    assert!(matches!(receiver.kind, ExprKind::FieldAccess { .. }));
}

#[test]
fn method_call_with_args() {
    let program = parse_ok("obj.method(1, 2);");
    let ExprKind::MethodCall { method, args, .. } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(method, "method");
    assert_eq!(args.len(), 2);
}

#[test]
fn method_call_chain() {
    let program = parse_ok("obj.a().b();");
    let ExprKind::MethodCall { receiver, method, args } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(method, "b");
    assert!(args.is_empty());
    assert!(matches!(receiver.kind, ExprKind::MethodCall { .. }));
}

#[test]
fn index_access() {
    let program = parse_ok("v[3];");
    let ExprKind::Index { target, index } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(target.kind, ExprKind::Ident(_)));
    assert!(matches!(index.kind, ExprKind::Number(_)));
}

#[test]
fn index_of_method_result() {
    let program = parse_ok("obj.items()[0];");
    assert!(matches!(body(&program).kind, ExprKind::Index { .. }));
}

#[test]
fn is_operator_with_named_type() {
    let program = parse_ok("x is Point;");
    let ExprKind::Is { type_ann, .. } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(*type_ann, TypeAnn::Named("Point".into()));
}

#[test]
fn as_operator_with_named_type() {
    let program = parse_ok("x as Point;");
    assert!(matches!(body(&program).kind, ExprKind::As { .. }));
}

#[test]
fn call_a_function() {
    let program = parse_ok("f(1, 2, 3);");
    let ExprKind::Call { callee, args } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(callee.kind, ExprKind::Ident(ref n) if n == "f"));
    assert_eq!(args.len(), 3);
}

#[test]
fn call_with_no_args() {
    let program = parse_ok("f();");
    let ExprKind::Call { args, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(args.is_empty());
}

#[test]
fn base_call() {
    // `base()` is the call to the parent implementation.
    let program = parse_ok("base();");
    let ExprKind::Call { callee, args } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(callee.kind, ExprKind::Base));
    assert!(args.is_empty());
}

// ---------------------------------------------------------------------------
// new Type(args)
// ---------------------------------------------------------------------------

#[test]
fn new_with_args() {
    let program = parse_ok("new Point(1, 2);");
    let ExprKind::New { type_ann, args } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(*type_ann, TypeAnn::Named("Point".into()));
    assert_eq!(args.len(), 2);
}

#[test]
fn new_without_args() {
    let program = parse_ok("new Point();");
    let ExprKind::New { args, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(args.is_empty());
}

// ---------------------------------------------------------------------------
// Vector literal vs generator
// ---------------------------------------------------------------------------

#[test]
fn vec_literal_basic() {
    let program = parse_ok("[1, 2, 3];");
    let ExprKind::VecLiteral(items) = &body(&program).kind else {
        panic!()
    };
    assert_eq!(items.len(), 3);
}

#[test]
fn empty_vec_literal() {
    let program = parse_ok("[];");
    let ExprKind::VecLiteral(items) = &body(&program).kind else {
        panic!()
    };
    assert!(items.is_empty());
}

#[test]
fn vec_generator() {
    let program = parse_ok("[x * 2 | x in range(0, 10)];");
    let ExprKind::VecGenerator { binding, element, iterable } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(binding, "x");
    assert!(matches!(element.kind, ExprKind::BinOp { .. }));
    assert!(matches!(iterable.kind, ExprKind::Call { .. }));
}

#[test]
fn vec_index_literal() {
    let program = parse_ok("[10, 20, 30][1];");
    let ExprKind::Index { target, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(target.kind, ExprKind::VecLiteral(_)));
}

// ---------------------------------------------------------------------------
// Lambda expressions
// ---------------------------------------------------------------------------

#[test]
fn lambda_without_annotations() {
    let program = parse_ok("(x) => x + 1;");
    let ExprKind::Lambda { params, return_type, body: lambda_body } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "x");
    assert!(params[0].type_ann.is_none());
    assert!(return_type.is_none());
    assert!(matches!(lambda_body.kind, ExprKind::BinOp { .. }));
}

#[test]
fn lambda_with_typed_param_and_return() {
    let program = parse_ok("(x: Number): Boolean => x > 0;");
    let ExprKind::Lambda { params, return_type, .. } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(params[0].type_ann, Some(TypeAnn::Named("Number".into())));
    assert_eq!(*return_type, Some(TypeAnn::Named("Boolean".into())));
}

#[test]
fn lambda_with_no_params() {
    let program = parse_ok("() => 42;");
    let ExprKind::Lambda { params, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(params.is_empty());
}

#[test]
fn lambda_multiple_params() {
    let program = parse_ok("(a, b, c) => a + b + c;");
    let ExprKind::Lambda { params, .. } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(params.len(), 3);
}

#[test]
fn grouping_vs_lambda_is_disambiguated() {
    // `(x)` is grouping when not followed by `=>`.
    let program = parse_ok("(1 + 2);");
    assert!(matches!(body(&program).kind, ExprKind::BinOp { .. }));

    // `(x) =>` is a lambda.
    let program = parse_ok("(x) => x;");
    assert!(matches!(body(&program).kind, ExprKind::Lambda { .. }));
}

// ---------------------------------------------------------------------------
// Type annotations
// ---------------------------------------------------------------------------

#[test]
fn iterable_type_annotation() {
    let program = parse_ok("function sum(xs: Number*): Number => 0;");
    let params = &program.functions[0].params;
    let expected = TypeAnn::Iterable(Box::new(TypeAnn::Named("Number".into())));
    assert_eq!(params[0].type_ann, Some(expected));
}

#[test]
fn vector_type_annotation() {
    let program = parse_ok("function mean(xs: Number[]): Number => 0;");
    let params = &program.functions[0].params;
    let expected = TypeAnn::Vector(Box::new(TypeAnn::Named("Number".into())));
    assert_eq!(params[0].type_ann, Some(expected));
}

#[test]
fn functor_type_annotation() {
    let program =
        parse_ok("function apply(f: (Number) -> Boolean, x: Number): Boolean => x > 0;");
    let params = &program.functions[0].params;
    match &params[0].type_ann {
        Some(TypeAnn::Functor { params: fn_params, ret }) => {
            assert_eq!(fn_params.len(), 1);
            assert_eq!(fn_params[0], TypeAnn::Named("Number".into()));
            assert_eq!(**ret, TypeAnn::Named("Boolean".into()));
        }
        other => panic!("expected Functor type annotation, got {other:?}"),
    }
}

#[test]
fn functor_with_zero_params_type() {
    let program = parse_ok("function once(f: () -> Number): Number => 0;");
    match &program.functions[0].params[0].type_ann {
        Some(TypeAnn::Functor { params, .. }) => assert!(params.is_empty()),
        other => panic!("expected empty-param functor, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Function declarations
// ---------------------------------------------------------------------------

#[test]
fn inline_function_without_return_type() {
    let program = parse_ok("function id(x) => x; 0;");
    assert_eq!(program.functions.len(), 1);
    let f = &program.functions[0];
    assert_eq!(f.name, "id");
    assert!(f.return_type.is_none());
    assert!(matches!(f.body.kind, ExprKind::Ident(_)));
}

#[test]
fn inline_function_with_types() {
    let program = parse_ok("function tan(x: Number): Number => x; 0;");
    let f = &program.functions[0];
    assert_eq!(f.params.len(), 1);
    assert_eq!(f.params[0].type_ann, Some(TypeAnn::Named("Number".into())));
    assert_eq!(f.return_type, Some(TypeAnn::Named("Number".into())));
}

#[test]
fn full_form_function_has_block_body() {
    let program = parse_ok(
        r#"function operate(x, y) {
            x;
            y;
        }
        0;"#,
    );
    let f = &program.functions[0];
    assert!(matches!(f.body.kind, ExprKind::Block(_)));
}

#[test]
fn function_with_recursive_self_call() {
    let program = parse_ok("function fib(n) => fib(n); 0;");
    let f = &program.functions[0];
    assert!(matches!(f.body.kind, ExprKind::Call { .. }));
}

#[test]
fn multiple_functions_preserve_order() {
    let program = parse_ok("function a() => 1; function b() => 2; 0;");
    let names: Vec<_> = program.functions.iter().map(|f| f.name.clone()).collect();
    assert_eq!(names, vec!["a", "b"]);
}

// ---------------------------------------------------------------------------
// Type declarations
// ---------------------------------------------------------------------------

#[test]
fn type_with_attribute() {
    let program = parse_ok(
        r#"type Point {
            x = 0;
            y = 0;
        }
        0;"#,
    );
    let ty = &program.types[0];
    assert_eq!(ty.name, "Point");
    assert_eq!(ty.members.len(), 2);
    for member in &ty.members {
        assert!(matches!(member.kind, MemberKind::Attribute { .. }));
    }
}

#[test]
fn type_attribute_with_annotation() {
    let program = parse_ok(
        r#"type Box {
            value: Number = 0;
        }
        0;"#,
    );
    match &program.types[0].members[0].kind {
        MemberKind::Attribute {
            type_ann,
            name,
            value,
        } => {
            assert_eq!(name, "value");
            assert_eq!(*type_ann, Some(TypeAnn::Named("Number".into())));
            assert!(matches!(value.kind, ExprKind::Number(_)));
        }
        _ => panic!("expected annotated attribute"),
    }
}

#[test]
fn type_with_inline_method() {
    let program = parse_ok(
        r#"type Point {
            x = 0;
            getX() => self.x;
        }
        0;"#,
    );
    let members = &program.types[0].members;
    match &members[1].kind {
        MemberKind::Method(f) => {
            assert_eq!(f.name, "getX");
            assert!(matches!(f.body.kind, ExprKind::FieldAccess { .. }));
        }
        _ => panic!("expected method"),
    }
}

#[test]
fn type_with_full_form_method() {
    let program = parse_ok(
        r#"type Point {
            x = 0;
            describe() {
                self.x;
            }
        }
        0;"#,
    );
    match &program.types[0].members[1].kind {
        MemberKind::Method(f) => assert!(matches!(f.body.kind, ExprKind::Block(_))),
        _ => panic!(),
    }
}

#[test]
fn type_with_constructor_params() {
    let program = parse_ok(
        r#"type Point(x: Number, y: Number) {
            x: Number = x;
            y: Number = y;
        }
        0;"#,
    );
    let ty = &program.types[0];
    assert_eq!(ty.params.len(), 2);
    assert_eq!(
        ty.params[0].type_ann,
        Some(TypeAnn::Named("Number".into()))
    );
}

#[test]
fn type_with_inheritance() {
    let program = parse_ok(
        r#"type PolarPoint(phi, rho) inherits Point(rho, phi) {
            rho() => self.rho;
        }
        0;"#,
    );
    let parent = program.types[0].parent.as_ref().expect("parent missing");
    assert_eq!(parent.name, "Point");
    assert_eq!(parent.args.len(), 2);
}

#[test]
fn type_without_inheritance_has_no_parent() {
    let program = parse_ok(
        r#"type Empty {
            a = 1;
        }
        0;"#,
    );
    assert!(program.types[0].parent.is_none());
}

// ---------------------------------------------------------------------------
// Protocol declarations
// ---------------------------------------------------------------------------

#[test]
fn protocol_with_required_return_types() {
    let program = parse_ok(
        r#"protocol Iterable {
            next(): Boolean;
            current(): Object;
        }
        0;"#,
    );
    let proto = &program.protocols[0];
    assert_eq!(proto.name, "Iterable");
    assert_eq!(proto.methods.len(), 2);
    assert_eq!(proto.methods[0].return_type, TypeAnn::Named("Boolean".into()));
    assert_eq!(proto.methods[1].return_type, TypeAnn::Named("Object".into()));
}

#[test]
fn protocol_without_extends() {
    let program = parse_ok("protocol Hashable { hash(): Number; } 0;");
    assert!(program.protocols[0].extends.is_empty());
}

#[test]
fn protocol_with_extends() {
    let program = parse_ok("protocol Equatable extends Hashable { equals(other: Object): Boolean; } 0;");
    assert_eq!(program.protocols[0].extends, vec!["Hashable".to_string()]);
}

#[test]
fn protocol_method_without_return_type_reports_error() {
    let (program, bag) = parse_with_errors("protocol Bad { foo(); } 0;");
    assert!(bag.has_errors(), "missing return type must be an error");
    // Parser still produces a protocol entry for error recovery.
    assert_eq!(program.protocols.len(), 1);
}

// ---------------------------------------------------------------------------
// Macro declarations
// ---------------------------------------------------------------------------

#[test]
fn macro_with_regular_and_body_params() {
    let program = parse_ok(
        r#"def repeat(n: Number, *expr: Object): Object => n;
        0;"#,
    );
    let mac = &program.macros[0];
    assert_eq!(mac.name, "repeat");
    assert_eq!(mac.params.len(), 2);
    assert!(matches!(mac.params[0], MacroParam::Regular { .. }));
    assert!(matches!(mac.params[1], MacroParam::Body { .. }));
}

#[test]
fn macro_with_symbolic_param() {
    let program = parse_ok(
        r#"def swap(@a: Object, @b: Object) {
            a;
        }
        0;"#,
    );
    let mac = &program.macros[0];
    assert_eq!(mac.params.len(), 2);
    for p in &mac.params {
        assert!(matches!(p, MacroParam::Symbolic { .. }));
    }
}

#[test]
fn macro_with_placeholder_param() {
    let program = parse_ok(
        r#"def repeat($iter: Number, n: Number, *expr: Object) {
            iter;
        }
        0;"#,
    );
    let mac = &program.macros[0];
    assert!(matches!(mac.params[0], MacroParam::Placeholder { .. }));
    assert!(matches!(mac.params[1], MacroParam::Regular { .. }));
    assert!(matches!(mac.params[2], MacroParam::Body { .. }));
}

#[test]
fn macro_params_keep_type_annotation() {
    let program = parse_ok("def m(n: Number) => n; 0;");
    let mac = &program.macros[0];
    assert_eq!(*mac.params[0].type_ann(), TypeAnn::Named("Number".into()));
}

// ---------------------------------------------------------------------------
// Integration: a full program with functions + types + protocols + macros
// ---------------------------------------------------------------------------

#[test]
fn full_program_with_all_decl_kinds() {
    let src = r#"
        function fib(n: Number): Number =>
            if (n <= 1) n else fib(n - 1) + fib(n - 2);

        type Point(x: Number, y: Number) {
            x: Number = x;
            y: Number = y;
            getX(): Number => self.x;
        }

        protocol Hashable {
            hash(): Number;
        }

        def noop(*expr: Object): Object => expr;

        print(fib(10));
    "#;

    let program = parse_ok(src);

    assert_eq!(program.functions.len(), 1);
    assert_eq!(program.types.len(), 1);
    assert_eq!(program.protocols.len(), 1);
    assert_eq!(program.macros.len(), 1);

    // Body is a call to print.
    assert!(matches!(program.body.kind, ExprKind::Call { .. }));
}

// ---------------------------------------------------------------------------
// Precedence & associativity regression tests
// ---------------------------------------------------------------------------

#[test]
fn postfix_binds_tighter_than_binary() {
    // obj.f() + 1  ⇒  (obj.f()) + 1
    let program = parse_ok("obj.f() + 1;");
    let ExprKind::BinOp { left, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(left.kind, ExprKind::MethodCall { .. }));
}

#[test]
fn unary_binds_tighter_than_power() {
    // -x ^ 2 parses as (-x) ^ 2 in HULK (unary > pow per PIPELINE).
    let program = parse_ok("-x ^ 2;");
    let ExprKind::BinOp { op: BinOpKind::Pow, left, .. } = &body(&program).kind else {
        panic!("expected pow at root")
    };
    assert!(matches!(
        left.kind,
        ExprKind::UnaryOp { op: UnaryOpKind::Neg, .. }
    ));
}

#[test]
fn power_is_right_associative() {
    // 2 ^ 3 ^ 2 parses as 2 ^ (3 ^ 2)
    let program = parse_ok("2 ^ 3 ^ 2;");
    let ExprKind::BinOp { op: BinOpKind::Pow, right, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(
        right.kind,
        ExprKind::BinOp {
            op: BinOpKind::Pow,
            ..
        }
    ));
}

#[test]
fn assignment_has_lowest_precedence() {
    // x := 1 + 2 parses as x := (1 + 2), not (x := 1) + 2
    let program = parse_ok("x := 1 + 2;");
    let ExprKind::Assign { value, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(
        value.kind,
        ExprKind::BinOp {
            op: BinOpKind::Add,
            ..
        }
    ));
}

// ---------------------------------------------------------------------------
// Edge cases and robustness
// ---------------------------------------------------------------------------

#[test]
fn empty_source_produces_empty_block() {
    let program = parse_ok("");
    assert!(matches!(program.body.kind, ExprKind::Block(_)));
}

#[test]
fn only_semicolon_source() {
    // A lone `;` is malformed (no expression); parser emits an error but
    // recovers and produces a body.
    let (_, bag) = parse_with_errors(";");
    assert!(bag.has_errors());
}

#[test]
fn deep_call_nesting_does_not_blow_stack() {
    // 50 nested calls: f(f(f(... 0 ...)))
    let mut src = String::new();
    for _ in 0..50 {
        src.push_str("f(");
    }
    src.push('0');
    for _ in 0..50 {
        src.push(')');
    }
    src.push(';');
    let program = parse_ok(&src);
    assert!(matches!(program.body.kind, ExprKind::Call { .. }));
}

#[test]
fn if_condition_with_assignment_inside() {
    // `let a = 0 in let b = a := 1 in { print(a); print(b); };`
    let program = parse_ok("let a = 0 in let b = a := 1 in { a; b; };");
    assert!(matches!(body(&program).kind, ExprKind::Let { .. }));
}

#[test]
fn method_call_on_base() {
    // base is an expression; `base()` is a call; `base.foo()` is a method
    // call on base. Both are supported.
    let program = parse_ok("base.foo();");
    let ExprKind::MethodCall { receiver, method, .. } = &body(&program).kind else {
        panic!()
    };
    assert_eq!(method, "foo");
    assert!(matches!(receiver.kind, ExprKind::Base));
}

#[test]
fn for_over_vector_literal() {
    let program = parse_ok("for (x in [1, 2, 3]) x;");
    let ExprKind::For { iterable, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(iterable.kind, ExprKind::VecLiteral(_)));
}

#[test]
fn new_type_with_vector_annotation_in_args() {
    // `new Container(v)` where v is a vector.
    let program = parse_ok("new Container([1, 2, 3]);");
    let ExprKind::New { args, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(args[0].kind, ExprKind::VecLiteral(_)));
}

// ---------------------------------------------------------------------------
// NodeId uniqueness across a large program
// ---------------------------------------------------------------------------

#[test]
fn node_ids_in_full_program_are_unique() {
    let src = r#"
        function fib(n) =>
            if (n <= 1) n else fib(n - 1) + fib(n - 2);

        type Pair(a, b) {
            a = a;
            b = b;
            sum() => self.a + self.b;
        }

        protocol Countable { count(): Number; }

        def once(*expr: Object): Object => expr;

        let p = new Pair(1, 2) in p.sum();
    "#;

    let program = parse_ok(src);

    // Collect every NodeId from the program by walking with a visitor.
    use hulk_ast::visitor::walk_expr;
    use hulk_ast::Visitor;
    use std::collections::HashSet;

    #[derive(Default)]
    struct IdSet(HashSet<hulk_ast::NodeId>);

    impl Visitor for IdSet {
        fn visit_expr(&mut self, expr: &Expr) {
            // Insert returns false if already present.
            assert!(
                self.0.insert(expr.id),
                "duplicate NodeId {:?} in expression {:?}",
                expr.id,
                expr.kind
            );
            walk_expr(self, expr);
        }
    }

    let mut ids = IdSet::default();
    ids.visit_program(&program);
    assert!(ids.0.len() > 20, "expected many nodes, got {}", ids.0.len());
}

// ---------------------------------------------------------------------------
// Make sure certain constructs don't accidentally succeed
// ---------------------------------------------------------------------------

#[test]
fn bare_type_without_braces_reports_error() {
    let (_, bag) = parse_with_errors("type Point 0;");
    assert!(bag.has_errors());
}

#[test]
fn missing_in_in_let_reports_error() {
    let (_, bag) = parse_with_errors("let x = 1 x;");
    assert!(bag.has_errors());
}

#[test]
fn dangling_decl_without_body_recovers_with_empty_block() {
    // Only a function declaration, no program body.
    let program = parse_ok("function foo() => 1;");
    assert!(matches!(program.body.kind, ExprKind::Block(_)));
    assert_eq!(program.functions.len(), 1);
}

// ---------------------------------------------------------------------------
// Sanity: specific examples from Hulk.md parse successfully
// ---------------------------------------------------------------------------

#[test]
fn hulk_md_hello_world() {
    parse_ok(r#"print("Hello World");"#);
}

#[test]
fn hulk_md_arithmetic_example() {
    parse_ok("print((((1 + 2) ^ 3) * 4) / 5);");
}

#[test]
fn hulk_md_let_multiple_bindings() {
    parse_ok(r#"let number = 42, text = "x" in print(text @ number);"#);
}

#[test]
fn hulk_md_let_redefining() {
    parse_ok(r#"let a = 20 in { let a = 42 in a; a; };"#);
}

#[test]
fn hulk_md_destructive_assignment_example() {
    parse_ok(r#"let a = 0 in { a; a := 1; a; };"#);
}

#[test]
fn hulk_md_conditional_elif() {
    parse_ok(
        r#"let a = 42, m = a in
             if (m == 0) "A"
             elif (m == 1) "B"
             else "C";"#,
    );
}

#[test]
fn hulk_md_while_gcd() {
    parse_ok(r#"function gcd(a, b) => while (a > 0) let m = a in { a := 1; }; 0;"#);
}

#[test]
fn hulk_md_for_over_range() {
    parse_ok("for (x in range(0, 10)) print(x);");
}

#[test]
fn hulk_md_type_with_methods() {
    parse_ok(
        r#"type Point {
            x = 0;
            y = 0;
            getX() => self.x;
            getY() => self.y;
            setX(x) => self.x := x;
            setY(y) => self.y := y;
        }
        let pt = new Point() in pt.getX();"#,
    );
}

#[test]
fn hulk_md_type_inherits_with_base_call() {
    parse_ok(
        r#"type Person(firstname, lastname) {
            firstname = firstname;
            lastname = lastname;
            name() => self.firstname @@ self.lastname;
        }
        type Knight inherits Person {
            name() => "Sir" @@ base();
        }
        let p = new Knight("Phil", "Collins") in print(p.name());"#,
    );
}

#[test]
fn hulk_md_protocol_iterable() {
    parse_ok(
        r#"protocol Iterable {
            next(): Boolean;
            current(): Object;
        }
        protocol Enumerable extends Iterable {
            iter(): Iterable;
        }
        0;"#,
    );
}

#[test]
fn hulk_md_vector_literal_and_generator() {
    parse_ok(r#"let squares = [x ^ 2 | x in range(1, 10)] in squares[0];"#);
}

#[test]
fn hulk_md_lambda_as_filter() {
    parse_ok(r#"let f = (x: Number): Boolean => x % 2 == 0 in f;"#);
}

#[test]
fn hulk_md_is_and_as() {
    parse_ok(
        r#"type A { a = 1; }
        type B inherits A { b = 2; }
        let x: A = new B() in
            if (x is B) (x as B) else x;"#,
    );
}

#[test]
fn hulk_md_macro_repeat() {
    parse_ok(
        r#"def repeat(n: Number, *expr: Object): Object =>
             while (n > 0) { n := n - 1; expr; };
           0;"#,
    );
}

// Dead-code suppression for helpers used across tests.
#[allow(dead_code)]
fn _use_imports(_: AssignTarget, _: Param) {}
