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

### Qué se implementó

- **`Resolver::resolve_program`**: ejecuta la pasada global de registro y luego recorre funciones, tipos, macros y el cuerpo del programa.
- **`expr_symbols: HashMap<NodeId, SymbolId>`**: guarda la referencia resuelta para cada `Expr::Ident` encontrada durante la segunda pasada.
- **Resolución de funciones globales**: cada `FunctionDecl` abre un scope propio, define parámetros y resuelve el cuerpo.
- **Resolución de tipos**: cada `TypeDecl` abre un scope propio para parámetros del tipo y miembros; los inicializadores de atributos se resuelven dentro de ese scope.
- **Resolución de macros**: cada `MacroDecl` abre un scope propio, define sus parámetros y resuelve el cuerpo.
- **Resolución secuencial de `let`**: cada inicializador se resuelve antes de introducir el nombre nuevo en un scope adicional, de modo que los bindings previos sí son visibles para los siguientes.
- **Resolución de `for`, bloques y lambdas**: cada una de estas construcciones abre scopes temporales cuando corresponde y resuelve sus subexpresiones recursivamente.
- **Tests de regresión**: programas sintéticos que validan recursión mutua entre funciones, scoping secuencial de `let` y resolución de tipos dentro de su scope.

### Orden de pasadas

1. **Pre-registro global**: se registran funciones, tipos, protocolos y macros en el scope global antes de entrar a sus cuerpos.
2. **Resolución de cuerpos**: se recorren declaraciones y expresiones con los scopes ya preparados.
3. **Registro de referencias**: cada `Expr::Ident` resuelta queda asociada a su `NodeId` en `expr_symbols`.

### Decisiones de diseño

- **Pre-registro obligatorio**: permite que la recursión mutua funcione sin depender del orden físico de las declaraciones en el archivo.
- **Una pasada de nombres, otra de cuerpos**: separa la visibilidad de símbolos de la resolución de sus usos, que es la forma más estable de modelar HULK en esta etapa.
- **`let` secuencial con scopes encadenados**: refleja que cada binding puede ver los anteriores, pero no a sí mismo ni a los posteriores.

### Gotchas

- **El source de prueba debe ser consistente con los spans**: los tests del resolver construyen ASTs a mano; si un `Span` excede la longitud real del `SourceFile`, `Span::new` paniquea. Se ajustaron los tests para usar `source.len()`.
- **`expr_symbols` solo guarda identificadores resueltos**: no intenta sustituir toda la semántica de tipos ni de miembros; esa parte queda para las subsesiones siguientes.

### Ejemplos de uso

Resolver un programa completo:

```rust
let mut resolver = Resolver::new();
resolver.resolve_program(&program);

if resolver.diagnostics().is_empty() {
    // nombres resueltos correctamente
}
```

Consultar la referencia de un identificador:

```rust
if let Some(symbol_id) = resolver.expr_symbols.get(&expr.id) {
    println!("identificador resuelto: {:?}", symbol_id);
}
```

## 6.3 — `self`/`base`, errores y tests

### Qué se implementó

- **`self` especial dentro de métodos**: `Expr::Self_` y `Ident("self")` resuelven al símbolo implícito del método actual cuando existe un contexto de método.
- **`base` especial dentro de métodos con herencia**: `Expr::Base` resuelve a la implementación del mismo método en el padre cuando el tipo actual hereda y el padre implementa ese método.
- **Diagnósticos de asignación inválida**: `AssignTarget::Ident("self")` reporta que `self` no se puede reasignar.
- **Diagnósticos de nombres faltantes**: se reportan `variable no declarada`, `funcion no existe` y `tipo no existe` cuando la resolución no encuentra un símbolo válido.
- **Diagnósticos de redeclaración**: `define(...)` detecta si un nombre ya existe en el scope actual y emite un error antes de reutilizarlo.
- **Tests de regresión**: casos para `self` fuera de método, asignación a `self`, variable inexistente, función inexistente, tipo inexistente, redeclaración y `base` con/sin padre.

### Lista de errores detectables

- `self usado fuera de un método`
- `base usado fuera de un método`
- `base usado en un tipo sin padre`
- `no se puede asignar a self`
- `identificador no declarado: <nombre>`
- `funcion no existe: <nombre>`
- `tipo no existe: <nombre>`
- `redefinicion de <nombre>`

### Decisiones de diseño

- **`self` se resuelve a un símbolo sintético local al método**: esto permite tratarlo como cualquier otro identificador resuelto por el analizador semántico.
- **`base` se valida con contexto de tipo y nombre de método**: el resolver necesita saber tanto el tipo actual como el método actual para encontrar la implementación padre correcta.
- **Los errores se registran en `DiagnosticBag` y no como `panic`**: así la fase semántica puede reportar múltiples problemas en una sola pasada.

### Gotchas

- **`Expr::Base` no trae el nombre del método**: el resolver debe recordar el nombre del método actual mientras procesa el cuerpo para localizar la implementación padre adecuada.
- **Las anotaciones de tipo se validan con builtins de tipo**: `Object`, `Number`, `String` y `Boolean` se registran como tipos builtin para que la validación no dependa de declaraciones del usuario.

### Ejemplos de uso

Errores reportados en una sola pasada:

```rust
let mut resolver = Resolver::new();
resolver.resolve_program(&program);

for diagnostic in resolver.diagnostics().diagnostics() {
    println!("{}", diagnostic.message);
}
```
