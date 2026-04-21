# Sesión 6 — Resolución de nombres

Esta sesión introduce la base del análisis semántico: tabla de símbolos, pila de scopes y resolución de identificadores. Se divide en tres subsesiones.

## 6.1 — `SymbolTable` y scope stack

### Qué se implementó

- **`crates/hulk-semantic/src/lib.rs`**: se reemplazó el stub inicial por la base del resolver semántico.
- **`SymbolId(u32)`**: identificador estable para símbolos registrados en la tabla.
- **`SymbolKind`**: clasificación semántica de símbolos para distinguir variables, funciones, tipos, protocolos, macros, parámetros y símbolos builtin.
- **`Symbol`**: metadatos del símbolo, con `id`, `name`, `kind` y `span`.
- **`SymbolTable`**: tabla densa respaldada por `Vec<Symbol>`, con métodos `add`, `get` y `name_of`.
- **`Resolver`**: estructura que combina tabla de símbolos, pila de scopes, `DiagnosticBag` y contexto actual (`current_type`, `current_method`).
- **Builtins registrados al crear el resolver**: `print`, `sqrt`, `sin`, `cos`, `exp`, `log`, `rand`, `range`, `PI`, `E`.
- **Tests unitarios** dentro del mismo crate para cubrir tabla de símbolos, push/pop de scopes, lookup anidado y builtins.

### Decisiones de diseño

- **Tabla densa con `Vec<Symbol>`**: el identificador estable es un índice lógico en la tabla. Esto hace que el acceso sea O(1) y evita mantener estructuras auxiliares para buscar por id.
- **Stack de scopes con `Vec<HashMap<String, SymbolId>>`**: cada scope modela un nivel léxico. El vector permite recorrer de adentro hacia afuera al hacer lookup, y el `HashMap` mantiene inserción y consulta simples por nombre.
- **Builtins en el scope global**: se registran al construir el `Resolver` para que estén disponibles desde el inicio sin depender de una pasada adicional.
- **`pop_scope` preserva el scope global**: el resolver nunca queda sin scope base, lo que simplifica la inicialización y evita estados inválidos durante la resolución.

### Gotchas

- **Conversión de índices**: `SymbolId` usa `u32`, así que la tabla convierte cuidadosamente entre `u32` y `usize` al insertar y consultar. Se evitó usar `unwrap`/`expect` para respetar las reglas del proyecto.
- **Builtins sin span real**: se usan spans sintéticos sobre un archivo interno `"<builtins>"`, suficiente para registrar símbolos sin inventar una fuente de usuario.

### Ejemplos de uso

Registrar y resolver un símbolo local:

```rust
let mut resolver = Resolver::new();
let span = ...;
let id = resolver.define("x", SymbolKind::Variable, span);
assert_eq!(resolver.lookup("x"), Some(id));
assert_eq!(resolver.table().name_of(id), Some("x"));
```

Crear un nuevo scope temporal:

```rust
resolver.push_scope();
resolver.define("y", SymbolKind::Variable, span);
resolver.pop_scope();
```

## 6.2 — Resolución de expresiones y declaraciones

Pendiente.

## 6.3 — `self`/`base`, errores y tests

Pendiente.
