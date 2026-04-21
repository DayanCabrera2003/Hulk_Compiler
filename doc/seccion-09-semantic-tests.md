# Sección 9 - Testing exhaustivo de semántica

## Qué se implementó

Se creó la suite de validación semántica para `hulk-hir`, centrada en programas válidos que deben construir HIR sin diagnósticos y en un conjunto pequeño de aserciones de resolución e inferencia sobre nodos concretos.

Archivos tocados:
- `crates/hulk-hir/tests/semantic.rs`
- `crates/hulk-hir/tests/semantic/valid/mod.rs`
- `crates/hulk-hir/tests/semantic/valid/examples.rs`
- `crates/hulk-hir/tests/semantic/valid/inference.rs`

### Suite de programas válidos

- `examples/*.hulk` se cargan y se verifican como programas válidos hasta HIR.
- `hello.hulk` comprueba la resolución del builtin `print`.
- `strings.hulk` comprueba la inferencia de `Concat` y `ConcatSpaced` como `String`.
- `conditionals.hulk` comprueba que el `if` interno de tipo rama string también infiere `String`.
- `classes.hulk` comprueba la resolución de `self` y `base` dentro de métodos.
- `protocols.hulk` comprueba el registro de símbolos de tipo `Protocol`.

### Casos de inferencia adicionales

- `function add(x, y) => x + y` valida que la expresión del cuerpo se infiere como `Number` y que ambos operandos resuelven a parámetros.
- Una cadena `is` / `as` con herencia valida que la condición de `if` infiere `Boolean`.
- Una jerarquía `A -> B -> C` valida que `base()` resuelve al método correcto en cada nivel.

## Decisiones de diseño

### 1. La suite vive en `hulk-hir`

La batería se colocó en el crate que consume el frontend completo porque la validación de esta sesión no es del lexer o del parser aislados, sino del paquete final de resolución + tipos.

### 2. El harness vuelve a ejecutar el pipeline explícitamente

Los tests construyen HIR paso a paso con `lex -> parse -> resolve -> infer -> Hir` para observar exactamente qué fase introduce cada dato. Esto evita depender de una función de ayuda opaca y hace más fácil depurar fallos de semántica.

### 3. Las aserciones se concentran en nodos concretos

En esta etapa, la inferencia de tipos todavía no llena todos los símbolos con tipos explícitos. Por eso las comprobaciones se orientan a `ExprKind`, `resolved_symbol(NodeId)` y `SymbolKind`, que son las señales estables que ya produce el frontend.

## Gotchas

### 1. `tests/semantic/valid/` necesita un harness raíz

Cargo solo descubre archivos de integración en la raíz de `tests/`. Para conservar la ruta pedida por el pipeline, se añadió `tests/semantic.rs` como punto de entrada que importa el submódulo ubicado en `tests/semantic/valid/`.

### 2. `self` y `base` no se validan igual que un identificador normal

Ambos se resuelven mediante reglas especiales del resolver, así que las pruebas verifican directamente el símbolo final asociado a sus `NodeId` y no una búsqueda por nombre simple.

### 3. La inferencia de funciones aún es parcial

El checker actual ya infiere el tipo de expresiones como binop, `if`, `String` concatenado y `base`, pero no completa aún tipos ricos para todos los símbolos. Por eso la suite combina inferencia de nodos con resolución simbólica.

## Ejemplos de uso

### Ejecutar un ejemplo completo

```rust
let (hir, bag) = build_source("hello.hulk", include_str!("../../../examples/hello.hulk"));
assert!(bag.is_empty());
assert!(hir.is_some());
```

### Comprobar resolución simbólica

```rust
let symbol_id = hir.symbols.lookup("print").expect("print must exist");
let symbol = hir.symbols.table().get(symbol_id).unwrap();
assert_eq!(symbol.kind, SymbolKind::BuiltinFunction);
```

### Comprobar inferencia de una expresión

```rust
assert_eq!(hir.expr_type(expr.id), Some(TypeId::STRING));
```

## Validación

- La suite cubre todos los programas de `examples/`.
- Incluye programas ad hoc para inferencia de binop, `if`, `is` / `as` y `base`.
