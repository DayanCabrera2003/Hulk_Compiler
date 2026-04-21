# Sección 8 - HIR

## Qué se implementó

En esta sesión se definió el punto de entrada unificado del middleend y backend: `hulk-hir` ahora expone una estructura `Hir` que agrupa el programa, la resolución semántica y el entorno de tipos.

Archivos tocados:
- `crates/hulk-hir/src/lib.rs`

API pública añadida:
- `TypedAst`, contenedor intermedio con `program`, `symbols` y `types`.
- `Hir`, contenedor final con `program`, `symbols` y `types`.
- `Hir::from_typed(TypedAst)`.
- Consultas `expr_type(NodeId)`, `symbol_type(SymbolId)` y `resolved_symbol(NodeId)`.

## Decisiones de diseño

### 1. El HIR agrupa semántica y tipos

El HIR no intenta duplicar el AST ni inventar una forma nueva de representar el programa. Su papel es empaquetar el AST ya resuelto junto con la información necesaria para consultas posteriores: qué símbolo resolvió cada nodo y qué tipo se inferió para símbolos y expresiones.

### 2. Se reutiliza el resolver existente

En lugar de crear una tabla de referencias paralela, `Hir` conserva el resolver semántico. Eso permite consultar `resolved_symbol(NodeId)` directamente sin perder la información de resolución ya producida por `hulk-semantic`.

### 3. `TypedAst` como frontera de construcción

`TypedAst` representa el estado justo antes de entrar al middleend: programa parseado, nombres resueltos y tipos inferidos. `Hir::from_typed` consume ese paquete y lo convierte en la estructura final.

## Gotchas

### 1. Los identificadores resueltos viven en semántica, no en AST

`resolved_symbol(NodeId)` no puede salir del AST puro porque los `NodeId` de las expresiones no cargan el símbolo resuelto. Por eso el HIR conserva el resolver semántico completo.

### 2. Los tipos de símbolos y expresiones ya están centralizados

`hulk-types` ya ofrece consultas para símbolos y expresiones. El HIR solo las reexpone para dar un único punto de entrada al resto del compilador.

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

## Validación

- `cargo build -p hulk-hir`: correcto.
- `cargo test -p hulk-hir`: 1/1 tests correctos.
