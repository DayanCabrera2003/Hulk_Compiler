# Sección 8 - HIR

## Qué se implementó

En esta sesión se definió el punto de entrada unificado del middleend y backend: `hulk-hir` ahora expone una estructura `Hir` que agrupa el programa, la resolución semántica y el entorno de tipos.

Archivos tocados:
- `crates/hulk-hir/src/lib.rs`

API pública añadida:

En `hulk-hir`:
- `TypedAst`, contenedor intermedio con `program`, `symbols` y `types`.
- `Hir`, contenedor final con `program`, `symbols` y `types`.
- `Hir::from_typed(TypedAst)`.
- Consultas `expr_type(NodeId)`, `symbol_type(SymbolId)` y `resolved_symbol(NodeId)`.

En `hulk-driver`:
- `build_hir(SourceFile, &mut DiagnosticBag) -> Option<Hir>`.

## Decisiones de diseño

### 1. El HIR agrupa semántica y tipos

El HIR no intenta duplicar el AST ni inventar una forma nueva de representar el programa. Su papel es empaquetar el AST ya resuelto junto con la información necesaria para consultas posteriores: qué símbolo resolvió cada nodo y qué tipo se inferió para símbolos y expresiones.

### 2. Se reutiliza el resolver existente

En lugar de crear una tabla de referencias paralela, `Hir` conserva el resolver semántico. Eso permite consultar `resolved_symbol(NodeId)` directamente sin perder la información de resolución ya producida por `hulk-semantic`.

### 3. `TypedAst` como frontera de construcción

`TypedAst` representa el estado justo antes de entrar al middleend: programa parseado, nombres resueltos y tipos inferidos. `Hir::from_typed` consume ese paquete y lo convierte en la estructura final.

### 4. El flujo de `build_hir`

```text
SourceFile
    → lexer
    → parser
    → resolver
    → type checker
    → Hir
```

La función no se detiene al primer diagnóstico. Acumula errores de todas las fases para dar una vista completa del frontend en una sola ejecución.

La única parada temprana real ocurre cuando el AST no puede construirse: en la práctica, el parser siempre retorna un `Program`, así que el flujo continúa hasta tipos y luego devuelve `None` si el bag contiene errores.

## Gotchas

### 1. Los identificadores resueltos viven en semántica, no en AST

`resolved_symbol(NodeId)` no puede salir del AST puro porque los `NodeId` de las expresiones no cargan el símbolo resuelto. Por eso el HIR conserva el resolver semántico completo.

### 2. Los tipos de símbolos y expresiones ya están centralizados

`hulk-types` ya ofrece consultas para símbolos y expresiones. El HIR solo las reexpone para dar un único punto de entrada al resto del compilador.

### 3. El frontend completo depende de datos parciales

Aunque haya errores semánticos, `build_hir` sigue adelante para inferir los nodos que sí son analizables. Esto evita perder diagnósticos posteriores y mantiene el bag como fuente única de errores.

## Ejemplos de uso

### Construcción

```rust
let hir = Hir::from_typed(TypedAst {
    program,
    symbols,
    types,
});
```

### Consulta de tipos y resolución

```rust
if let Some(symbol_id) = hir.resolved_symbol(node_id) {
    let symbol_ty = hir.symbol_type(symbol_id);
    let expr_ty = hir.expr_type(node_id);
}
```

### Inspección de un ejemplo

```rust
let mut bag = DiagnosticBag::new();
let source = SourceFile::new("hello.hulk", include_str!("../../../examples/hello.hulk"));
let hir = build_hir(source, &mut bag).expect("hello.hulk should be valid");
```

## Validación

- `cargo build -p hulk-hir`: correcto.
- `cargo test -p hulk-hir`: 27/27 tests correctos.
- `cargo test -p hulk-driver`: correcto (incluye los tests de integración de `build_hir`).

### Cobertura de la suite

La suite de `hulk-hir` se reparte en tres áreas:

- **Programas válidos basados en ejemplos** (`tests/semantic/valid/examples.rs`): construye el HIR para los 13 programas de `examples/` y verifica resolución de `print`, `self`, `base`, protocolos, tipos de operaciones de concatenación y forma de condicionales.
- **Inferencia de tipos** (`tests/semantic/valid/inference.rs`): cubre cadenas `is`/`as`, llamadas en estilo protocolo, herencia multinivel y funciones sin anotaciones.
- **Errores semánticos** (`tests/semantic/errors/mod.rs`): 15 tests que validan mensajes específicos para identificadores no declarados, redefiniciones, tipos inexistentes, asignación a `self`, uso de `base` fuera de método o sin padre, herencia inválida, anotaciones incompatibles, métodos inexistentes, no-conformancia con protocolos, inferencias ambiguas, ciclos de herencia y acumulación de varios errores en una sola pasada.

Los tests end-to-end que ejercitan `build_hir` sobre el pipeline completo viven en `hulk-driver/tests/build_hir.rs` y `hulk-driver/tests/error_cases.rs`.

## Inspección del HIR

El HIR se usa como punto de entrada único para middleend y backend. La forma más práctica de consultarlo es construirlo desde un `SourceFile` y luego preguntar por símbolos y tipos ya resueltos.

```rust
let mut bag = DiagnosticBag::new();
let source = SourceFile::new("hello.hulk", include_str!("../../../examples/hello.hulk"));
let hir = build_hir(source, &mut bag).expect("hello.hulk debe ser válido");

let ExprKind::Call { callee, .. } = &hir.program.body.kind else {
    unreachable!("el ejemplo hello.hulk debe terminar en una llamada");
};

let symbol_id = hir.resolved_symbol(callee.id).expect("print debe resolverse");
let symbol_ty = hir.symbol_type(symbol_id);
let expr_ty = hir.expr_type(hir.program.body.id);
```

Con esto, el resto del compilador puede consultar el AST, la resolución semántica y la inferencia de tipos sin depender de tres estructuras separadas.
