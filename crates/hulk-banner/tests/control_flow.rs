mod support;

use hulk_banner::Instr;

fn count_labels(instrs: &[Instr]) -> usize {
    instrs
        .iter()
        .filter(|i| matches!(i, Instr::Label(_)))
        .count()
}

fn count_jumpif(instrs: &[Instr]) -> usize {
    instrs
        .iter()
        .filter(|i| matches!(i, Instr::JumpIf { .. }))
        .count()
}

fn has_loop_label(instrs: &[Instr]) -> bool {
    instrs.iter().any(|i| {
        if let Instr::Label(name) = i {
            name.starts_with("loop_")
        } else {
            false
        }
    })
}

#[test]
fn if_generates_jumpif_and_labels() {
    let prog = support::build_banner("if_test", "let x = 1 in if (x > 0) x else 0;");
    assert!(
        count_jumpif(&prog.main.body) >= 1,
        "if needs at least one JumpIf"
    );
    assert!(
        count_labels(&prog.main.body) >= 2,
        "if needs at least two Labels (then + end)"
    );
}

#[test]
fn if_no_else_is_valid() {
    let prog = support::build_banner("if_no_else", "let x = 1 in if (x > 0) print(x);");
    // Should compile without panic. Result is ConstNull.
    assert!(count_jumpif(&prog.main.body) >= 1);
}

#[test]
fn elif_branch_generates_extra_jumpif_and_labels() {
    let prog = support::build_banner(
        "elif_test",
        "let x = 2 in
         if (x == 1) 1
         elif (x == 2) 2
         else 3;",
    );
    // 2 JumpIf: one for main cond, one for elif cond
    assert!(
        count_jumpif(&prog.main.body) >= 2,
        "elif needs at least 2 JumpIfs, got {}: {:?}",
        count_jumpif(&prog.main.body),
        prog.main.body
    );
    // At least 3 labels: then_elif_0, then, end
    assert!(
        count_labels(&prog.main.body) >= 3,
        "elif needs at least 3 labels"
    );
}

#[test]
fn while_generates_loop_label_and_jumpif() {
    let prog = support::build_banner("while_test", "let x = 0 in while (x < 5) { x := x + 1; };");
    assert!(has_loop_label(&prog.main.body), "while needs a loop_ label");
    assert!(
        count_jumpif(&prog.main.body) >= 1,
        "while needs a JumpIf exit"
    );
}

#[test]
fn let_shadowing_resolves_inner_binding() {
    // Should lower without panics — shadowing is handled by the resolver.
    let prog = support::build_banner("shadow", "let x = 1 in let x = 2 in x;");
    // The main body should return 2 (inner x).
    // We just check it doesn't panic and produces some Copy instructions.
    let has_copy = prog
        .main
        .body
        .iter()
        .any(|i| matches!(i, Instr::Copy { .. }));
    assert!(has_copy, "let bindings should produce Copy instructions");
}
