use hulk_banner::{BannerFunction, BannerProgram, Instr, TempId, TypeDescriptor, Value};

#[test]
fn temp_id_is_transparent() {
    let t = TempId(0);
    assert_eq!(t.0, 0);
}

#[test]
fn banner_program_holds_types_functions_main() {
    let main = BannerFunction {
        name: "__main__".to_string(),
        params: vec![],
        param_names: vec![],
        body: vec![Instr::Return(Value::ConstNull)],
    };
    let prog = BannerProgram {
        types: vec![],
        functions: vec![],
        main,
    };
    assert_eq!(prog.main.name, "__main__");
    assert!(prog.types.is_empty());
}

#[test]
fn type_descriptor_fields_and_pointer_map_are_parallel() {
    let td = TypeDescriptor {
        name: "Point".to_string(),
        parent: None,
        fields: vec!["x".to_string(), "y".to_string()],
        pointer_map: vec![false, false],
        field_kinds: vec![
            hulk_banner::FieldKind::Number,
            hulk_banner::FieldKind::Number,
        ],
        methods: vec![],
    };
    assert_eq!(td.fields.len(), td.pointer_map.len());
    assert_eq!(td.fields.len(), td.field_kinds.len());
}
