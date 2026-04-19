//! `type` con atributos, métodos, herencia, `self` y `base`.

use hulk_ast::{ExprKind, MemberKind};

use crate::common::{contains_expr, parse_ok};

const SRC: &str = include_str!("../../../../examples/classes.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("classes.hulk", SRC);
}

#[test]
fn declares_four_types_in_order() {
    let program = parse_ok("classes.hulk", SRC);
    let names: Vec<_> = program.types.iter().map(|t| t.name.clone()).collect();
    assert_eq!(names, vec!["Point", "PolarPoint", "Person", "Knight"]);
}

#[test]
fn point_has_two_attributes_and_four_methods() {
    let program = parse_ok("classes.hulk", SRC);
    let point = program
        .types
        .iter()
        .find(|t| t.name == "Point")
        .expect("Point no declarado");
    let (attrs, methods) = count_members(point);
    assert_eq!(attrs, 2);
    assert_eq!(methods, 4);
    assert_eq!(point.params.len(), 2, "Point tiene 2 params de constructor");
}

#[test]
fn polar_point_inherits_point_with_forwarded_args() {
    let program = parse_ok("classes.hulk", SRC);
    let polar = program
        .types
        .iter()
        .find(|t| t.name == "PolarPoint")
        .expect("PolarPoint no declarado");
    let parent = polar.parent.as_ref().expect("PolarPoint sin parent");
    assert_eq!(parent.name, "Point");
    assert_eq!(parent.args.len(), 2);
}

#[test]
fn knight_inherits_person_without_args() {
    let program = parse_ok("classes.hulk", SRC);
    let knight = program
        .types
        .iter()
        .find(|t| t.name == "Knight")
        .expect("Knight no declarado");
    let parent = knight.parent.as_ref().expect("Knight sin parent");
    assert_eq!(parent.name, "Person");
    assert!(parent.args.is_empty(), "Knight no forwardea args");
}

#[test]
fn knight_name_method_calls_base() {
    let program = parse_ok("classes.hulk", SRC);
    let knight = program
        .types
        .iter()
        .find(|t| t.name == "Knight")
        .expect("Knight no declarado");
    let name_method = knight
        .members
        .iter()
        .find_map(|m| match &m.kind {
            MemberKind::Method(f) if f.name == "name" => Some(f),
            _ => None,
        })
        .expect("método name no encontrado");
    // body: "Sir" @@ base();
    let has_base_call = contains_expr_body(&name_method.body, |k| matches!(k, ExprKind::Base));
    assert!(has_base_call, "se esperaba Base dentro del método name");
}

#[test]
fn types_contain_self_references() {
    let program = parse_ok("classes.hulk", SRC);
    assert!(contains_expr(&program, |k| matches!(k, ExprKind::Self_)));
}

#[test]
fn body_constructs_instance_with_new_and_calls_method() {
    let program = parse_ok("classes.hulk", SRC);
    // Body: let p = new Knight(...) in print(p.name());
    let ExprKind::Let { body, .. } = &program.body.kind else {
        panic!("body debe ser Let");
    };
    let ExprKind::Call { callee, args } = &body.kind else {
        panic!("body del let debe ser Call");
    };
    assert!(matches!(callee.kind, ExprKind::Ident(ref n) if n == "print"));
    assert!(matches!(args[0].kind, ExprKind::MethodCall { .. }));
}

fn count_members(ty: &hulk_ast::TypeDecl) -> (usize, usize) {
    let mut attrs = 0;
    let mut methods = 0;
    for m in &ty.members {
        match m.kind {
            MemberKind::Attribute { .. } => attrs += 1,
            MemberKind::Method(_) => methods += 1,
        }
    }
    (attrs, methods)
}

fn contains_expr_body<F: Fn(&ExprKind) -> bool>(expr: &hulk_ast::Expr, pred: F) -> bool {
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
