use hulk_banner::{BannerFunction, BannerProgram, Instr, TempId, TypeDescriptor, Value};

fn simple_main(body: Vec<Instr>) -> BannerProgram {
    BannerProgram {
        types: vec![],
        functions: vec![],
        main: BannerFunction {
            name: "__main__".to_string(),
            params: vec![],
            param_names: vec![],
            param_runtime_hints: vec![],
            body,
        },
    }
}

#[test]
fn print_return_null() {
    let prog = simple_main(vec![Instr::Return(Value::ConstNull)]);
    let s = format!("{prog}");
    assert!(s.contains("fn __main__()"));
    assert!(s.contains("return null"));
}

#[test]
fn print_copy_instr() {
    let prog = simple_main(vec![
        Instr::Copy {
            dst: TempId(0),
            src: Value::ConstNum(1.0),
        },
        Instr::Return(Value::Temp(TempId(0))),
    ]);
    let s = format!("{prog}");
    assert!(s.contains("t0 = copy 1"));
    assert!(s.contains("return t0"));
}

#[test]
fn print_label_indentation() {
    let prog = simple_main(vec![
        Instr::Label("loop_0".to_string()),
        Instr::Jump("loop_0".to_string()),
    ]);
    let s = format!("{prog}");
    // Labels are indented 2 spaces, not 4
    assert!(s.contains("  loop_0:"));
    assert!(s.contains("    jump loop_0"));
}

#[test]
fn print_const_str_escaping() {
    let prog = simple_main(vec![Instr::Return(Value::ConstStr(
        "hello\nworld".to_string(),
    ))]);
    let s = format!("{prog}");
    assert!(s.contains(r#"return "hello\nworld""#));
}

#[test]
fn print_type_descriptor() {
    let prog = BannerProgram {
        types: vec![TypeDescriptor {
            name: "Point".to_string(),
            parent: None,
            fields: vec!["x".to_string()],
            pointer_map: vec![false],
            field_kinds: vec![hulk_banner::FieldKind::Number],
            methods: vec![],
        }],
        functions: vec![],
        main: BannerFunction {
            name: "__main__".to_string(),
            params: vec![],
            param_names: vec![],
            param_runtime_hints: vec![],
            body: vec![Instr::Return(Value::ConstNull)],
        },
    };
    let s = format!("{prog}");
    assert!(s.contains("type Point {"));
    assert!(s.contains("parent: none"));
    assert!(s.contains("x (val)")); // false pointer_map -> "val"
}
