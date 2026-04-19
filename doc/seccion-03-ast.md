# Seccion 03 - AST

## Que se implemento

- Se creo el modulo de expresiones en crates/hulk-ast/src/expr.rs.
- Se definio Expr con los campos kind, span e id.
- Se agregaron NodeId y NodeIdGen para generar ids estables por nodo.
- Se definio ExprKind con las variantes pedidas en la subsesion 3.1:
  - Number, StringLit, Bool, Ident, Self_, Base
  - BinOp, UnaryOp
  - Call, MethodCall, FieldAccess, Index
  - Block, VecLiteral, VecGenerator
- Se definio BinOpKind con operadores de Expressions y Conditionals:
  - Aritmeticos: Add, Sub, Mul, Div, Mod, Pow
  - Strings: Concat (@), ConcatSpaced (@@)
  - Comparacion: Lt, Le, Gt, Ge, Eq, Ne
  - Booleanos: And (&), Or (|)
- Se definio UnaryOpKind con Neg y Not.
- Se exporto la API publica desde crates/hulk-ast/src/lib.rs.
- Se agregaron tests unitarios basicos para NodeIdGen y constructor de Expr.

## Decisiones de diseno

- Por que NodeId es necesario:
  - Las fases de resolucion de nombres y de tipos necesitan mapear metadata hacia nodos de expresion concretos.
  - Tener NodeId estable permite tablas como NodeId -> SymbolId y NodeId -> TypeId sin depender de punteros, direccion de memoria o spans ambiguos.

- Box<Expr> vs Rc<Expr> vs indices:
  - Se eligio Box<Expr> para hijos de expresion porque representa arbol ownership-unico de forma simple y sin contadores atomicos.
  - Rc<Expr> se reservo como alternativa no necesaria por ahora; agregaria costo y complejidad para compartir subarboles, algo que el AST inicial no requiere.
  - Indices en arena son una opcion valida para etapas posteriores, pero en esta subsesion Box mantiene el codigo mas directo, legible y con menos infraestructura.

## Gotchas

- Hulk.md menciona division con barra invertida en texto, pero los ejemplos usan /. El AST modela division como Div, alineado con ejemplos y con PIPELINE.md.
- El operador % no aparece en la lista inicial de arithmetic operators del texto, pero si aparece en ejemplos de conditionals. Se incluyo como Mod.
- El operador @@ se incluye como binario separado (ConcatSpaced) para conservar intencion semantica en AST antes de desugaring.

## Ejemplos de uso

```rust
use hulk_ast::{BinOpKind, Expr, ExprKind, NodeIdGen};
use hulk_span::{SourceFile, Span};
use std::sync::Arc;

let mut ids = NodeIdGen::new();
let file = Arc::new(SourceFile::new("sample.hulk", "1 + 2"));

let lhs = Expr::new(ExprKind::Number(1.0), Span::new(file.clone(), 0, 1), ids.next_id());
let rhs = Expr::new(ExprKind::Number(2.0), Span::new(file.clone(), 4, 5), ids.next_id());

let expr = Expr::new(
    ExprKind::BinOp {
        op: BinOpKind::Add,
        left: Box::new(lhs),
        right: Box::new(rhs),
    },
    Span::new(file, 0, 5),
    ids.next_id(),
);
```
