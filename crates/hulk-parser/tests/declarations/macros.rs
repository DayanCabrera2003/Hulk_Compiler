//! Macro declarations (`def ...`).

use super::*;

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
