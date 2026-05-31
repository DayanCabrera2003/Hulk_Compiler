//! Integration: full programs, NodeId uniqueness, and spec examples from
//! `hulk-docs.pdf` parse successfully.

use super::*;

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
// Sanity: specific examples from hulk-docs.pdf parse successfully
// ---------------------------------------------------------------------------

#[test]
fn spec_hello_world() {
    parse_ok(r#"print("Hello World");"#);
}

#[test]
fn spec_arithmetic_example() {
    parse_ok("print((((1 + 2) ^ 3) * 4) / 5);");
}

#[test]
fn spec_let_multiple_bindings() {
    parse_ok(r#"let number = 42, text = "x" in print(text @ number);"#);
}

#[test]
fn spec_let_redefining() {
    parse_ok(r#"let a = 20 in { let a = 42 in a; a; };"#);
}

#[test]
fn spec_destructive_assignment_example() {
    parse_ok(r#"let a = 0 in { a; a := 1; a; };"#);
}

#[test]
fn spec_conditional_elif() {
    parse_ok(
        r#"let a = 42, m = a in
             if (m == 0) "A"
             elif (m == 1) "B"
             else "C";"#,
    );
}

#[test]
fn spec_while_gcd() {
    parse_ok(r#"function gcd(a, b) => while (a > 0) let m = a in { a := 1; }; 0;"#);
}

#[test]
fn spec_for_over_range() {
    parse_ok("for (x in range(0, 10)) print(x);");
}

#[test]
fn spec_type_with_methods() {
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
fn spec_type_inherits_with_base_call() {
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
fn spec_protocol_iterable() {
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
fn spec_vector_literal_and_generator() {
    parse_ok(r#"let squares = [x ^ 2 | x in range(1, 10)] in squares[0];"#);
}

#[test]
fn spec_lambda_as_filter() {
    parse_ok(r#"let f = (x: Number): Boolean => x % 2 == 0 in f;"#);
}

#[test]
fn spec_is_and_as() {
    parse_ok(
        r#"type A { a = 1; }
        type B inherits A { b = 2; }
        let x: A = new B() in
            if (x is B) (x as B) else x;"#,
    );
}

#[test]
fn spec_macro_repeat() {
    parse_ok(
        r#"def repeat(n: Number, *expr: Object): Object =>
             while (n > 0) { n := n - 1; expr; };
           0;"#,
    );
}
