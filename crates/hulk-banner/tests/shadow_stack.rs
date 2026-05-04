mod support;

use hulk_banner::Instr;

fn count_shadow_push(instrs: &[Instr]) -> usize {
    instrs.iter().filter(|i| matches!(i, Instr::ShadowPush(_))).count()
}

fn count_shadow_pop(instrs: &[Instr]) -> usize {
    instrs.iter().filter(|i| matches!(i, Instr::ShadowPop)).count()
}

#[test]
fn string_let_binding_generates_shadow_push() {
    let prog = support::build_banner(
        "shadow_str",
        r#"let s: String = "hello" in print(s);"#,
    );
    assert!(
        count_shadow_push(&prog.main.body) >= 1,
        "String let binding should generate ShadowPush: {:?}", prog.main.body
    );
}

#[test]
fn shadow_push_and_pop_are_balanced() {
    let prog = support::build_banner(
        "shadow_balanced",
        r#"let s: String = "hello" in print(s);"#,
    );
    assert_eq!(
        count_shadow_push(&prog.main.body),
        count_shadow_pop(&prog.main.body),
        "ShadowPush and ShadowPop must be balanced"
    );
}

#[test]
fn number_let_binding_does_not_generate_shadow_push() {
    let prog = support::build_banner(
        "shadow_num",
        "let n: Number = 42 in print(n);",
    );
    assert_eq!(
        count_shadow_push(&prog.main.body),
        0,
        "Number let binding should NOT generate ShadowPush: {:?}", prog.main.body
    );
}

#[test]
fn object_let_binding_generates_shadow_push() {
    let prog = support::build_banner(
        "shadow_obj",
        "type Box(v: Number) { v: Number = v; }
         let b = new Box(1) in b;",
    );
    assert!(
        count_shadow_push(&prog.main.body) >= 1,
        "Object let binding should generate ShadowPush: {:?}", prog.main.body
    );
}

#[test]
fn nested_let_shadow_balanced() {
    let prog = support::build_banner(
        "nested_shadow",
        r#"let a: String = "x" in let b: String = "y" in print(a);"#,
    );
    assert_eq!(
        count_shadow_push(&prog.main.body),
        count_shadow_pop(&prog.main.body),
        "Nested let bindings: ShadowPush/ShadowPop must be balanced"
    );
}
