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
- Se creo el modulo de declaraciones en crates/hulk-ast/src/decl.rs.
- Se definieron Program, FunctionDecl, TypeDecl, ProtocolDecl y MacroDecl.
- Se agregaron Param, Member, ParentSpec y MethodSig para modelar firmas y miembros.
- Se agrego MacroParam con sus variantes Regular, Body, Symbolic y Placeholder.
- Se extendio ExprKind para cuerpos de declaraciones con:
  - Let, Assign, AssignTarget, LetBinding
  - If, While, For
  - New, Is, As, Lambda
- Se agrego TypeAnn para anotaciones de tipo:
  - Named(T)
  - Iterable(T*)
  - Vector(T[])
  - Functor((A)->B)
- Se incorporo el modulo crates/hulk-ast/src/visitor.rs.
- Se definieron los traits Visitor (solo lectura) y VisitorMut (transformador).
- Se implementaron recorridos por defecto para Program, declaraciones, TypeAnn y Expr.
- Se agrego un test de recorrido completo y transformacion simple con VisitorMut.

## Decisiones de diseno

- Por que NodeId es necesario:
  - Las fases de resolucion de nombres y de tipos necesitan mapear metadata hacia nodos de expresion concretos.
  - Tener NodeId estable permite tablas como NodeId -> SymbolId y NodeId -> TypeId sin depender de punteros, direccion de memoria o spans ambiguos.

- Box<Expr> vs Rc<Expr> vs indices:
  - Se eligio Box<Expr> para hijos de expresion porque representa arbol ownership-unico de forma simple y sin contadores atomicos.
  - Rc<Expr> se reservo como alternativa no necesaria por ahora; agregaria costo y complejidad para compartir subarboles, algo que el AST inicial no requiere.
  - Indices en arena son una opcion valida para etapas posteriores, pero en esta subsesion Box mantiene el codigo mas directo, legible y con menos infraestructura.

- Funcion inline (`=>`) vs full-form (`{}`):
  - Ambas formas se representan con la misma `FunctionDecl`.
  - El campo `body` siempre es `Expr`.
  - Si la funcion es inline, `body` puede ser cualquier expresion simple.
  - Si la funcion es full-form, `body` es `ExprKind::Block`.
  - Esta unificacion evita duplicar nodos para funciones y simplifica parser, visitors y fases semanticas posteriores.

- Patron Visitor para fases posteriores:
  - Se eligio un Visitor con metodos `visit_*` y funciones `walk_*` por defecto.
  - Las fases que solo inspeccionan (resolver, typer) pueden sobreescribir pocos hooks y reutilizar el recorrido base.
  - Las fases transformadoras usan VisitorMut para editar in-place cuando conviene (por ejemplo, desugar o normalizaciones locales).
  - Este enfoque evita duplicar logica de traversal y reduce errores al agregar nuevas variantes del AST.

## Gotchas

- El operador de división en HULK es `/` (verificado en hulk-docs.pdf Apéndice A §A.2.1 y todos sus ejemplos de código). El AST lo modela como `BinOpKind::Div`.
- El operador % no aparece en la lista inicial de arithmetic operators del texto, pero si aparece en ejemplos de conditionals. Se incluyo como Mod.
- El operador @@ se incluye como binario separado (ConcatSpaced) para conservar intencion semantica en AST antes de desugaring.
- Las anotaciones de tipo se modelan con `TypeAnn` en lugar de strings crudos para que parser y fases semanticas compartan una representacion estructural unica.
- `NodeIdGen` hace panic en overflow de `u32` (más de 4 mil millones de nodos). En la práctica inalcanzable, pero documentado: las alternativas eran wrap-around silencioso (peor) o Result (ruido excesivo en API).
- `f64::NAN` rompe la igualdad estructural de `Expr`: dos `ExprKind::Number(NaN)` no son `PartialEq`. El parser no produce NaN y el tipo checker detectaría cualquier aritmética que lo genere; aun así hay un test específico que documenta este quirk.

## Bugs encontrados y corregidos en revisión exhaustiva

Durante la revisión posterior a la implementación inicial se encontraron **cuatro bugs reales** que se corrigieron antes de cerrar la sesión:

### Bug 1 — `FunctionDecl` sin `return_type`

La struct original no tenía campo para el tipo de retorno, impidiendo representar `function tan(x: Number): Number => sin(x) / cos(x);` (sintaxis válida en HULK y usada por la spec de protocolos para verificar conformance).

**Fix**: se añadió `return_type: Option<TypeAnn>` a `FunctionDecl`. `None` cuando el tipo se debe inferir, `Some(ann)` cuando está anotado.

### Bug 2 — `MacroParam` descartaba las anotaciones de tipo

La definición original era `enum MacroParam { Regular(String), Body(String), Symbolic(String), Placeholder(String) }`. La spec de HULK (sección Macros) especifica que cada parámetro de macro lleva su tipo: `def repeat(n: Number, *expr: Object)`, `def swap(@a: Object, @b: Object)`, `def repeat($iter: Number, ...)`. Sin el tipo, el verificador de macros no puede chequear que los argumentos coinciden con el parámetro declarado.

**Fix**: cada variante ahora es `{ name: String, type_ann: TypeAnn, span: Span }`. Se añadieron métodos `name()` y `type_ann()` para acceso uniforme.

### Bug 3 — `MemberKind::Attribute.value` era `Option<Expr>`

La spec dice explícitamente: *"All attributes must be given an initialization expression"*. Tener `value: Option<Expr>` permitía representar programas sintácticamente inválidos (atributos sin inicializador), haciendo que el parser o el resolver tuvieran que validar lo que el AST ya debería garantizar.

**Fix**: `value` ahora es `Expr` (obligatorio). El test `attribute_value_is_required_not_optional` en `tests/coverage.rs` hace un check a nivel de tipos que rompería la compilación si alguien vuelve a optional.

### Bug 4 — `walk_expr` mezclaba tres variantes con re-match fragil

El código original era:

```rust
ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) | ExprKind::Let { bindings: exprs, .. } => {
    for item in exprs { visitor.visit_expr(item); }
    if let ExprKind::Let { body, .. } = &expr.kind {
        visitor.visit_expr(body);
    }
}
```

Funcionaba pero era fragil: si alguien añadía un campo nuevo a `Let` o introducía otra variante con un `Vec<Expr>`, el pattern podía matchear por accidente y silenciar un bug. Se separó en dos brazos independientes: uno para `Block | VecLiteral` y otro explícito para `Let`.

**Fix + test de regresión**: `visitor_visits_let_body_after_bindings` verifica que el body de un `Let` se visita después de las bindings, usando NodeIds específicos que detectarían si el visitor se salta el body.

## Cobertura de tests

**Tests en `crates/hulk-ast/src/*.rs`** (unitarios): 4 tests.

**Tests en `crates/hulk-ast/tests/coverage.rs`** (integración, 42 tests):

- **`NodeIdGen`** (7 tests): secuencial, offset de inicio, 10.000 ids únicos, overflow panic, clonado independiente, copy + hashable, ordenable.
- **`ExprKind`** (14 tests): cada grupo de variantes (literales, átomos, 16 BinOpKind, 2 UnaryOpKind, Call/MethodCall, FieldAccess/Index, Block/VecLiteral vacíos, VecGenerator, Let, Assign con los 3 targets, If 0..=5 elif, While/For, New/Is/As, Lambda).
- **`TypeAnn`** (2 tests): nesting arbitrario (`Number[][]`, `Number*[]`, `(Number, Number) -> Boolean`, `(Number*) -> Number[]`), functor sin parámetros.
- **Declaraciones** (6 tests): `FunctionDecl` con/sin return type, `TypeDecl` con herencia + mix de miembros, verificación compile-time de que attribute value no es Optional, ProtocolDecl con/sin extends, MethodSig con return type obligatorio, MacroDecl con los 4 tipos de parámetro.
- **`Visitor`/`VisitorMut`** (7 tests): cobertura de cada variante en kitchen-sink (26 variantes de ExprKind visitadas), annotations de macro params alcanzadas, inicializadores de attribute alcanzados, args de parent spec alcanzados, Let.body visitado (regresión bug 4), transformación de números en árbol anidado.
- **Robustez** (6 tests): nesting profundo (500 BinOps = 1001 nodos, sin stack overflow), strings UTF-8 multibyte (`"héllo 🦀 ñ"`), identificadores con dígitos y guiones bajos no-iniciales, clone estructural, programa vacío, NodeIds únicos en kitchen-sink.

**Ejecución**: `cargo test -p hulk-ast` → 46/46 passed. `cargo clippy -p hulk-ast --all-targets -- -D warnings` limpio.

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

// Declaracion de funcion: inline o full-form usando la misma struct.
use hulk_ast::{ExprKind, FunctionDecl, Param};

let inline_fn = FunctionDecl {
  name: "id".to_owned(),
  params: vec![Param {
    name: "x".to_owned(),
    type_name: None,
    span: Span::dummy(),
  }],
  body: Expr::new(ExprKind::Ident("x".to_owned()), Span::dummy(), ids.next_id()),
  span: Span::dummy(),
};

let full_fn = FunctionDecl {
  name: "sum".to_owned(),
  params: vec![],
  body: Expr::new(
    ExprKind::Block(vec![Expr::new(ExprKind::Number(42.0), Span::dummy(), ids.next_id())]),
    Span::dummy(),
    ids.next_id(),
  ),
  span: Span::dummy(),
};

assert!(matches!(inline_fn.body.kind, ExprKind::Ident(_)));
assert!(matches!(full_fn.body.kind, ExprKind::Block(_)));

// Visitor: contar expresiones sin reimplementar el recorrido.
use hulk_ast::visitor::walk_expr;
use hulk_ast::{Expr, Program, Visitor};

struct CountExpr {
  n: usize,
}

impl Visitor for CountExpr {
  fn visit_expr(&mut self, expr: &Expr) {
    self.n += 1;
    walk_expr(self, expr);
  }
}

let mut counter = CountExpr { n: 0 };
counter.visit_program(&Program {
  functions: vec![],
  types: vec![],
  protocols: vec![],
  macros: vec![],
  body: expr,
});

assert!(counter.n >= 1);
```
