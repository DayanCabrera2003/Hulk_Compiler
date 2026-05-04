use std::sync::Arc;

use hulk_ast::{BinOpKind, Expr, ExprKind, NodeId};
use hulk_diagnostics::DiagnosticBag;
use hulk_semantic::{Resolver, SymbolId};

use crate::env::TypeEnv;
use crate::inferer::TypeInferer;
use crate::symbol_inferer::SymbolInferer;
use crate::type_id::{BuiltinType, TypeId, TypeKind};

fn test_expr(kind: ExprKind, id: u32) -> Expr {
    let file = Arc::new(hulk_ast::SourceFile::new("test.hulk", ""));
    let span = hulk_ast::Span::dummy(file);
    Expr::new(kind, span, NodeId(id))
}

#[test]
fn type_env_registers_builtins() {
    let env = TypeEnv::new();
    assert!(matches!(
        env.type_kind(TypeId::OBJECT),
        Some(TypeKind::Builtin(BuiltinType::Object))
    ));
    assert!(matches!(
        env.type_kind(TypeId::NUMBER),
        Some(TypeKind::Builtin(BuiltinType::Number))
    ));
    assert!(matches!(
        env.type_kind(TypeId::STRING),
        Some(TypeKind::Builtin(BuiltinType::String))
    ));
    assert!(matches!(
        env.type_kind(TypeId::BOOLEAN),
        Some(TypeKind::Builtin(BuiltinType::Boolean))
    ));
}

#[test]
fn conforms_identity() {
    let env = TypeEnv::new();
    assert!(env.conforms(TypeId::NUMBER, TypeId::NUMBER));
    assert!(env.conforms(TypeId::STRING, TypeId::STRING));
}

#[test]
fn conforms_to_object() {
    let env = TypeEnv::new();
    assert!(env.conforms(TypeId::NUMBER, TypeId::OBJECT));
    assert!(env.conforms(TypeId::STRING, TypeId::OBJECT));
    assert!(env.conforms(TypeId::BOOLEAN, TypeId::OBJECT));
}

#[test]
fn conforms_inheritance() {
    let mut env = TypeEnv::new();
    let animal = env.register_type("Animal".to_string(), Some(TypeId::OBJECT));
    let dog = env.register_type("Dog".to_string(), Some(animal));

    // Dog conforms to Animal
    assert!(env.conforms(dog, animal));
    // Dog conforms to Object (through Animal)
    assert!(env.conforms(dog, TypeId::OBJECT));
    // Animal does not conform to Dog
    assert!(!env.conforms(animal, dog));
}

#[test]
fn lca_same_type() {
    let env = TypeEnv::new();
    assert_eq!(env.lca(TypeId::NUMBER, TypeId::NUMBER), TypeId::NUMBER);
}

#[test]
fn lca_subtype_and_parent() {
    let mut env = TypeEnv::new();
    let animal = env.register_type("Animal".to_string(), Some(TypeId::OBJECT));
    let dog = env.register_type("Dog".to_string(), Some(animal));

    // LCA(Dog, Animal) = Animal (Dog conforms to Animal)
    assert_eq!(env.lca(dog, animal), animal);
    // LCA(Animal, Dog) = Animal (Dog conforms to Animal)
    assert_eq!(env.lca(animal, dog), animal);
}

#[test]
fn lca_different_types_both_subtype() {
    let mut env = TypeEnv::new();
    let animal = env.register_type("Animal".to_string(), Some(TypeId::OBJECT));
    let dog = env.register_type("Dog".to_string(), Some(animal));
    let cat = env.register_type("Cat".to_string(), Some(animal));

    // LCA(Dog, Cat) = Animal (common parent)
    assert_eq!(env.lca(dog, cat), animal);
}

#[test]
fn symbol_and_expr_types() {
    let mut env = TypeEnv::new();
    let symbol = SymbolId(42);
    let node = NodeId(100);

    env.register_symbol_type(symbol, TypeId::NUMBER);
    env.register_expr_type(node, TypeId::STRING);

    assert_eq!(env.symbol_type(symbol), Some(TypeId::NUMBER));
    assert_eq!(env.expr_type(node), Some(TypeId::STRING));
}

#[test]
fn infer_literals() {
    let mut env = TypeEnv::new();
    let resolver = Resolver::new();
    let bag = DiagnosticBag::new();
    let mut inferer = TypeInferer::new(&mut env, &resolver, &bag);

    let number_expr = test_expr(ExprKind::Number(1.0), 1);
    let string_expr = test_expr(ExprKind::StringLit("hello".to_string()), 2);
    let bool_expr = test_expr(ExprKind::Bool(true), 3);

    assert_eq!(inferer.infer_expr(&number_expr), TypeId::NUMBER);
    assert_eq!(inferer.infer_expr(&string_expr), TypeId::STRING);
    assert_eq!(inferer.infer_expr(&bool_expr), TypeId::BOOLEAN);
}

#[test]
fn infer_binop_arithmetic() {
    let mut env = TypeEnv::new();
    let resolver = Resolver::new();
    let bag = DiagnosticBag::new();
    let mut inferer = TypeInferer::new(&mut env, &resolver, &bag);

    let expr = test_expr(
        ExprKind::BinOp {
            op: BinOpKind::Add,
            left: Box::new(test_expr(ExprKind::Number(1.0), 2)),
            right: Box::new(test_expr(ExprKind::Number(2.0), 3)),
        },
        1,
    );

    assert_eq!(inferer.infer_expr(&expr), TypeId::NUMBER);
}

#[test]
fn infer_binop_boolean() {
    let mut env = TypeEnv::new();
    let resolver = Resolver::new();
    let bag = DiagnosticBag::new();
    let mut inferer = TypeInferer::new(&mut env, &resolver, &bag);

    let expr = test_expr(
        ExprKind::BinOp {
            op: BinOpKind::Lt,
            left: Box::new(test_expr(ExprKind::Number(1.0), 2)),
            right: Box::new(test_expr(ExprKind::Number(2.0), 3)),
        },
        1,
    );

    assert_eq!(inferer.infer_expr(&expr), TypeId::BOOLEAN);
}

#[test]
fn infer_vec_literal_registers_vector_type() {
    let mut env = TypeEnv::new();
    let resolver = Resolver::new();
    let bag = DiagnosticBag::new();

    let expr = test_expr(
        ExprKind::VecLiteral(vec![
            test_expr(ExprKind::Number(1.0), 2),
            test_expr(ExprKind::Number(2.0), 3),
        ]),
        1,
    );

    let before_len = env.types.len();
    let inferred = {
        let mut inferer = TypeInferer::new(&mut env, &resolver, &bag);
        inferer.infer_expr(&expr)
    };

    assert_eq!(inferred.as_u32() as usize, before_len);
    assert!(matches!(
        env.type_kind(inferred),
        Some(TypeKind::Vector(TypeId::NUMBER))
    ));
}

#[test]
fn infer_vec_generator_registers_vector_type() {
    let mut env = TypeEnv::new();
    let resolver = Resolver::new();
    let bag = DiagnosticBag::new();

    let expr = test_expr(
        ExprKind::VecGenerator {
            element: Box::new(test_expr(ExprKind::Number(1.0), 2)),
            binding: "x".to_string(),
            iterable: Box::new(test_expr(ExprKind::Number(42.0), 3)),
        },
        1,
    );

    let before_len = env.types.len();
    let inferred = {
        let mut inferer = TypeInferer::new(&mut env, &resolver, &bag);
        inferer.infer_expr(&expr)
    };

    assert_eq!(inferred.as_u32() as usize, before_len);
    assert!(matches!(
        env.type_kind(inferred),
        Some(TypeKind::Vector(TypeId::NUMBER))
    ));
}

#[test]
fn symbol_inferer_creates() {
    let mut inferer = SymbolInferer::new();
    assert_eq!(inferer.iterations(), 0);

    // Single refinement cycle (no progress)
    let mut env = TypeEnv::new();
    assert!(!inferer.refine_symbols(&mut env));

    // iterations incremented
    assert_eq!(inferer.iterations(), 1);
}

#[test]
fn symbol_inferer_converges() {
    let mut inferer = SymbolInferer::new();
    let mut env = TypeEnv::new();
    let unknown_id = TypeId(env.types.len() as u32);
    env.types.push(TypeKind::Unknown);

    let result = inferer.infer_all(&mut env);
    assert!(result.is_ok());
    assert!(inferer.iterations() >= 2);
    assert!(!matches!(
        env.type_kind(unknown_id),
        Some(TypeKind::Unknown)
    ));
}
