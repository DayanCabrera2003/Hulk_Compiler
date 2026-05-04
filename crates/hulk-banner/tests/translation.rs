mod support;

use hulk_banner::{Instr, Value};

fn has_call_to(instrs: &[Instr], name: &str) -> bool {
    instrs.iter().any(|i| {
        matches!(i,
            Instr::Call { callee: Value::Global(n), .. } if n == name
        )
    })
}

fn has_method_call(instrs: &[Instr], method: &str) -> bool {
    instrs.iter().any(|i| {
        matches!(i,
            Instr::MethodCall { method: m, .. } if m == method
        )
    })
}

fn has_new(instrs: &[Instr], type_name: &str) -> bool {
    instrs.iter().any(|i| {
        matches!(i,
            Instr::New { type_name: n, .. } if n == type_name
        )
    })
}

fn has_get_field(instrs: &[Instr], field: &str) -> bool {
    instrs.iter().any(|i| {
        matches!(i,
            Instr::GetField { field: f, .. } if f == field
        )
    })
}

#[test]
fn fib_generates_recursive_call() {
    let prog = support::build_banner(
        "fib",
        "function fib(n: Number): Number => if (n <= 1) n else fib(n-1) + fib(n-2);
         fib(5);",
    );
    let fib_fn = prog
        .functions
        .iter()
        .find(|f| f.name == "fib")
        .expect("fib function not found");
    assert!(
        has_call_to(&fib_fn.body, "fib"),
        "fib should have a recursive Call to Global(\"fib\"): {:?}",
        fib_fn.body
    );
}

#[test]
fn main_calls_builtin_print() {
    let prog = support::build_banner("print_test", "print(42);");
    assert!(
        has_call_to(&prog.main.body, "print"),
        "main should call Global(\"print\")"
    );
}

#[test]
fn new_expr_generates_new_instr() {
    let prog = support::build_banner(
        "new_test",
        "type Point(x: Number, y: Number) { x: Number = x; y: Number = y; }
         new Point(1, 2);",
    );
    assert!(
        has_new(&prog.main.body, "Point"),
        "main should contain New {{ type_name: \"Point\" }}: {:?}",
        prog.main.body
    );
}

#[test]
fn field_access_generates_get_field() {
    let prog = support::build_banner(
        "field_test",
        "type Box(v: Number) { v: Number = v; }
         let b = new Box(5) in b.v;",
    );
    assert!(
        has_get_field(&prog.main.body, "v"),
        "main should contain GetField {{ field: \"v\" }}: {:?}",
        prog.main.body
    );
}

#[test]
fn method_call_generates_method_call_instr() {
    let prog = support::build_banner(
        "method_test",
        "type Counter(n: Number) {
           n: Number = n;
           get(): Number => self.n;
         }
         let c = new Counter(3) in c.get();",
    );
    assert!(
        has_method_call(&prog.main.body, "get"),
        "main should contain MethodCall with method \"get\": {:?}",
        prog.main.body
    );
}

#[test]
fn arithmetic_produces_binop_instrs() {
    let prog = support::build_banner("arith", "1 + 2 * 3;");
    let has_binop = prog
        .main
        .body
        .iter()
        .any(|i| matches!(i, Instr::BinOp { .. }));
    assert!(has_binop, "arithmetic should produce BinOp instructions");
}

#[test]
fn type_decl_generates_init_method() {
    let prog = support::build_banner(
        "init_test",
        "type Foo(x: Number) { x: Number = x; } new Foo(1);",
    );
    let foo_type = prog
        .types
        .iter()
        .find(|t| t.name == "Foo")
        .expect("Foo TypeDescriptor not found");
    assert!(
        foo_type.methods.iter().any(|m| m.name == "Foo.__init__"),
        "Foo should have a __init__ method: {:?}",
        foo_type.methods.iter().map(|m| &m.name).collect::<Vec<_>>()
    );
}

#[test]
fn pretty_print_fib_program() {
    let prog = support::build_banner(
        "fib_print",
        "function fib(n: Number): Number => if (n <= 1) n else fib(n-1) + fib(n-2);
         print(fib(5));",
    );
    let s = format!("{prog}");
    // Verify structure without checking exact temps
    assert!(s.contains("fn fib("), "should contain fib function");
    assert!(s.contains("fn __main__()"), "should contain main");
    assert!(
        s.contains("call fib"),
        "fib body should have recursive call"
    );
}
