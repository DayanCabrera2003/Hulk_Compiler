# Sección 9 - Testing exhaustivo de semántica

## Qué se implementó

Se creó la suite de validación semántica para `hulk-hir`, centrada en programas válidos que deben construir HIR sin diagnósticos y en un conjunto pequeño de aserciones de resolución e inferencia sobre nodos concretos.

Archivos tocados:
- `crates/hulk-hir/tests/semantic.rs`
- `crates/hulk-hir/tests/semantic/valid/mod.rs`
- `crates/hulk-hir/tests/semantic/valid/examples.rs`
- `crates/hulk-hir/tests/semantic/valid/inference.rs`
- `crates/hulk-hir/tests/semantic/errors/mod.rs`
- `crates/hulk-hir/tests/property.rs`
- `crates/hulk-hir/tests/property/mod.rs`
- `crates/hulk-hir/Cargo.toml`
- `crates/hulk-types/src/lib.rs`

### Suite de programas válidos

`all_example_programs_build_hir` valida que todos los programas en `examples/` construyen HIR sin errores semánticos.

Tabla de cobertura 9.1 (programa -> feature semántica principal -> verificación clave):

| Programa | Feature principal | Verificación de tipo/símbolo clave |
|---|---|---|
| `hello.hulk` | Builtins globales | `print` resuelve como `BuiltinFunction`; `program.body` infiere `Object` |
| `strings.hulk` | Concatenación `@` y `@@` | expresiones `Concat` y `ConcatSpaced` infieren `String` |
| `conditionals.hulk` | Tipado de `if/elif/else` | `if` interno productor de texto infiere `String` |
| `classes.hulk` | `self`/`base` + herencia | `self` resuelve a `SelfValue`; `base` resuelve al método padre |
| `protocols.hulk` | Declaración de protocolos | símbolos `Hashable`, `Equatable`, `Iterable`, `Enumerable` como `Protocol` |

### Casos de inferencia adicionales

| Caso ad hoc | Feature principal | Verificación de tipo/símbolo clave |
|---|---|---|
| `add.hulk` | Función sin anotaciones | cuerpo `x + y` infiere `Number`; ambos operandos resuelven a `Parameter` |
| `implicit_protocol.hulk` | Síntesis implícita por uso de métodos (`x.speak()`, `x.title()`) | receptor en ambos method calls resuelve al parámetro `x`; `@@` infiere `String` |
| `inheritance.hulk` | Herencia multinivel con `base` | `base()` en `B.foo` y `C.foo` resuelve a símbolo de función `foo` |
| `is_as.hulk` | Cadena polimórfica `is`/`as` | condición `is` infiere `Boolean`; rama `as` mantiene resolución simbólica |

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

## Errores semánticos cubiertos (9.2)

Se agregó una suite dedicada en `crates/hulk-hir/tests/semantic/errors/mod.rs` que valida mensajes canónicos de diagnósticos para casos de error semántico y de firma de protocolo.

Tabla de casos cubiertos:

| Caso | Programa de prueba | Mensaje esperado (canónico) |
|---|---|---|
| Variable no declarada | `x;` | `identificador no declarado: x` |
| Redefinición en mismo scope | `function dup(x, x) => x;` | `redefinicion de x` |
| Tipo no existe en anotación | `function id(x: MissingType): Number => x;` | `tipo no existe: MissingType` |
| Asignar a `self` | método con `self := v;` | `no se puede asignar a self` |
| `base` fuera de método | `base();` | `base usado fuera de un método` |
| `base` en tipo sin padre | método `foo() => base();` en tipo sin `inherits` | `base usado en un tipo sin padre` |
| Heredar de primitivo builtin | `type Bad inherits Number { }` | `no se puede heredar de Number` |
| Tipo inferido incompatible con anotación | `let value: Number = "text" in value;` | `tipo inferido incompatible con anotación` |
| Llamada a método inexistente | `new A().missing();` | `metodo no existe: missing` |
| Tipo no conforma a protocolo requerido | `use_printable(new Plain());` con `Printable` | `tipo no conforma al protocolo requerido: Printable` |
| Inferencia ambigua (requiere anotación) | `function identity(x) => x;` | `tipo no inferible, añade anotación` |
| Ciclos en herencia | `type A inherits B` + `type B inherits A` | `ciclos en herencia` |
| Llamada a función inexistente | `missing();` | `funcion no existe: missing` |
| Firma de protocolo sin retorno | `protocol P { foo(); }` | `firma de metodo sin tipo de retorno` |

Adicionalmente, la suite verifica recuperación y reporte múltiple de errores en una sola pasada (redefinición + identificador no declarado + función no existente) para confirmar que el frontend no aborta en el primer fallo.

## Property tests y robustez (9.3)

Se agregó una suite de propiedades en `crates/hulk-hir/tests/property/` para validar robustez del pipeline semántico con generación aleatoria de programas sintácticamente válidos.

Propiedades implementadas:

- `generated_semantic_inputs_never_panic_and_report_result`: para cada programa generado, verifica que `lex -> parse -> resolve -> infer -> Hir` nunca paniquea y siempre retorna un resultado válido (`Some(Hir)` o diagnóstico en `DiagnosticBag`).
- `hir_maps_are_consistent_with_ast_and_symbol_table`: cuando hay `HIR` exitoso, verifica invariantes internos: todo `NodeId` en `expr_types` existe en el AST y todo `SymbolId` en `symbol_types` existe en la `SymbolTable`.

### Métricas (9.3)

| Suite | Casos por propiedad | Propiedades | Total de casos generados |
|---|---:|---:|---:|
| `hulk-hir/tests/property.rs` | 256 | 2 | 512 |

Validación ejecutada:

- `cargo test -p hulk-hir --test property` (2/2 tests OK)
- `cargo test --workspace` (workspace completo en verde)
