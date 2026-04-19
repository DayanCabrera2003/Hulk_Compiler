//! Casos cruzados: features combinadas que no caben en un solo `.hulk` de
//! `examples/`. Sirven como red de seguridad para detectar regresiones en
//! las intersecciones (lambda dentro de generador, for dentro de método,
//! macro con body, etc.).

use hulk_ast::{BinOpKind, ExprKind, TypeAnn};

use crate::common::{contains_expr, count_exprs, parse_ok};

#[test]
fn program_with_all_decl_kinds_plus_body() {
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
    let program = parse_ok("all.hulk", src);
    assert_eq!(program.functions.len(), 1);
    assert_eq!(program.types.len(), 1);
    assert_eq!(program.protocols.len(), 1);
    assert_eq!(program.macros.len(), 1);
    assert!(matches!(program.body.kind, ExprKind::Call { .. }));
}

#[test]
fn lambda_coexists_with_vec_generator() {
    // Nota: el parser trata `|` como operador binario (Or) con BP 3/4 — usar
    // una lambda como elemento directo del generador (ej:
    // `[ (y) => y*y | x in it ]`) colisiona con esa ambigüedad. El test
    // coexiste lambda y generador via `let`.
    let src = "let f = (y) => y * y in [ f(x) | x in range(0, 5) ];";
    let program = parse_ok("lambda_gen.hulk", src);
    let ExprKind::Let { body, .. } = &program.body.kind else {
        panic!("se esperaba Let en raíz");
    };
    assert!(matches!(body.kind, ExprKind::VecGenerator { .. }));
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::Lambda { .. }
    )));
}

#[test]
fn for_inside_method_body() {
    let src = r#"type Counter {
        acc: Number = 0;
        sum(xs: Number*): Number {
            for (x in xs) self.acc := self.acc + x;
            self.acc;
        }
    }
    0;"#;
    let program = parse_ok("for_in_method.hulk", src);
    assert_eq!(program.types.len(), 1);
    let ty = &program.types[0];
    // Uno de los miembros es el método `sum` cuyo body es un block que contiene un For.
    let has_for_in_sum = ty.members.iter().any(|m| match &m.kind {
        hulk_ast::MemberKind::Method(f) if f.name == "sum" => {
            contains_expr_in(&f.body, |k| matches!(k, ExprKind::For { .. }))
        }
        _ => false,
    });
    assert!(has_for_in_sum);
}

#[test]
fn assign_nested_inside_while_condition_body() {
    let src = r#"let a = 5 in while ((a := a - 1) > 0) print(a);"#;
    let program = parse_ok("assign_in_while.hulk", src);
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::While { .. }
    )));
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::Assign { .. }
    )));
}

#[test]
fn is_and_as_chain_with_inheritance() {
    let src = r#"
        type A { a = 1; }
        type B inherits A { b = 2; }
        let x: A = new B() in
            if (x is B) (x as B).b else 0;
    "#;
    let program = parse_ok("is_as.hulk", src);
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::Is { .. }
    )));
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::As { .. }
    )));
}

#[test]
fn method_call_chain_with_indexing() {
    let src = "obj.list()[0].toString();";
    let program = parse_ok("chain.hulk", src);
    assert!(matches!(program.body.kind, ExprKind::MethodCall { .. }));
}

#[test]
fn let_with_functor_type_annotation() {
    let src = r#"let f: (Number) -> Boolean = (x: Number): Boolean => x > 0 in f(1);"#;
    let program = parse_ok("let_functor.hulk", src);
    let ExprKind::Let { bindings, .. } = &program.body.kind else {
        panic!("se esperaba Let");
    };
    let ExprKind::LetBinding(b) = &bindings[0].kind else {
        panic!();
    };
    let Some(TypeAnn::Functor { params, ret }) = &b.type_ann else {
        panic!("la anotación de f debe ser Functor");
    };
    assert_eq!(params.len(), 1);
    assert_eq!(**ret, TypeAnn::Named("Boolean".into()));
}

#[test]
fn deep_nesting_does_not_stack_overflow() {
    // 30 niveles de paréntesis anidados.
    let mut src = String::new();
    for _ in 0..30 {
        src.push('(');
    }
    src.push('1');
    for _ in 0..30 {
        src.push(')');
    }
    src.push(';');
    let program = parse_ok("deep.hulk", &src);
    assert!(matches!(program.body.kind, ExprKind::Number(_)));
}

#[test]
fn node_ids_unique_in_dense_program() {
    let src = r#"
        function mul(x: Number, y: Number): Number => x * y;
        type Wrap(v: Number) {
            v: Number = v;
            doubled(): Number => mul(self.v, 2);
        }
        let w = new Wrap(5) in print(w.doubled());
    "#;
    let program = parse_ok("ids.hulk", src);
    assert_unique_ids(&program);
}

#[test]
fn mixed_arithmetic_string_and_boolean_operators() {
    let src = r#"{
        print(1 + 2 * 3 ^ 2);
        print("a" @ "b" @@ "c");
        print((true & !false) | (1 == 1));
        print(-1 * -(2 + 3));
    };"#;
    let program = parse_ok("mixed.hulk", src);
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::BinOp {
            op: BinOpKind::Pow,
            ..
        }
    )));
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::BinOp {
            op: BinOpKind::And,
            ..
        }
    )));
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::BinOp {
            op: BinOpKind::Or,
            ..
        }
    )));
}

#[test]
fn macro_body_param_combined_with_regular_param() {
    let src = "def repeat(n: Number, *expr: Object): Object => n; 0;";
    let program = parse_ok("macro_body.hulk", src);
    let m = &program.macros[0];
    assert_eq!(m.params.len(), 2);
    assert!(matches!(m.params[1], hulk_ast::MacroParam::Body { .. }));
}

#[test]
fn total_declarations_and_bindings_in_stress_program() {
    let src = r#"
        function a() => 1;
        function b() => 2;
        function c() => 3;
        type P { x = 1; }
        protocol Q { q(): Number; }
        def m(*e: Object): Object => e;
        let x = 1, y = 2, z = 3 in print(x + y + z);
    "#;
    let program = parse_ok("stress.hulk", src);
    assert_eq!(program.functions.len(), 3);
    assert_eq!(program.types.len(), 1);
    assert_eq!(program.protocols.len(), 1);
    assert_eq!(program.macros.len(), 1);
    let lets = count_exprs(&program, |k| matches!(k, ExprKind::Let { .. }));
    assert_eq!(lets, 1);
}

fn contains_expr_in<F: Fn(&ExprKind) -> bool>(expr: &hulk_ast::Expr, pred: F) -> bool {
    use hulk_ast::Visitor;
    struct Finder<F> {
        pred: F,
        hit: bool,
    }
    impl<F: Fn(&ExprKind) -> bool> Visitor for Finder<F> {
        fn visit_expr(&mut self, e: &hulk_ast::Expr) {
            if (self.pred)(&e.kind) {
                self.hit = true;
            }
            hulk_ast::visitor::walk_expr(self, e);
        }
    }
    let mut f = Finder { pred, hit: false };
    f.visit_expr(expr);
    f.hit
}

fn assert_unique_ids(program: &hulk_ast::Program) {
    use hulk_ast::Visitor;
    use std::collections::HashSet;

    struct IdSet(HashSet<hulk_ast::NodeId>);
    impl Visitor for IdSet {
        fn visit_expr(&mut self, e: &hulk_ast::Expr) {
            assert!(self.0.insert(e.id), "NodeId duplicado: {:?}", e.id);
            hulk_ast::visitor::walk_expr(self, e);
        }
    }
    let mut ids = IdSet(HashSet::new());
    ids.visit_program(program);
}
