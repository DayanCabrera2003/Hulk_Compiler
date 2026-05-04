# Sesión 01 — Infraestructura base

> Crates: `hulk-span`, `hulk-diagnostics`.
> Estado: completada.

Esta sesión implementa la base común sobre la que se construye todo el compilador: representación de ubicaciones en el código fuente y sistema de reporte de errores.

---

## Qué se implementó

### `hulk-span`

**Archivo**: `crates/hulk-span/src/lib.rs`.

Tipos públicos:

- `SourceFile`: archivo fuente con nombre y contenido completo. Al crearlo pre-calcula un vector `line_starts` con los offsets del inicio de cada línea, permitiendo conversión `offset → (línea, columna)` en O(log n) con búsqueda binaria.
- `LineCol { line, column }`: par 1-based de coordenadas.
- `Span { file, start, end }`: rango de bytes semi-abierto `[start, end)` vinculado a un `SourceFile`. El archivo se comparte como `Arc<SourceFile>` para que múltiples spans del mismo source no dupliquen el texto.

API principal:

- `SourceFile::new(name, source)` — construye el archivo y pre-calcula `line_starts`.
- `SourceFile::line_col(offset)` — devuelve `LineCol` 1-based.
- `Span::new(file, start, end)` — valida que `start ≤ end` y `end ≤ len(source)`.
- `Span::merge(self, other)` — unión de dos spans: `min(start)..max(end)`.
- `Span::dummy(file)` — span vacío en la posición 0, para nodos AST sintéticos generados durante error recovery.

Sin dependencias externas.

### `hulk-diagnostics`

**Archivo**: `crates/hulk-diagnostics/src/lib.rs`.

Tipos públicos:

- `Severity` — `Error`, `Warning`, `Note`.
- `Label { span, message }` — zona de código a subrayar con mensaje asociado.
- `Diagnostic { severity, message, labels, notes }` — error completo con builder fluido (`with_label`, `with_note`).
- `DiagnosticBag` — acumulador. Nunca aborta el flujo: todas las fases depositan errores aquí y se emiten al final.

API principal:

- `DiagnosticBag::push(diag)` / `push_error(msg)`.
- `has_errors()`, `is_empty()`, `len()`.
- `diagnostics()` — referencia al slice interno.
- `drain()` — consume todos los diagnósticos y deja el bag vacío; lo usa el driver para pasar errores entre fases.
- `emit_stderr()` — renderiza vía `codespan-reporting` en stderr con colores.
- `emit(writer)` — renderiza a cualquier `Write + WriteColor` (útil para tests).

Dependencia externa: `codespan-reporting` 0.11.

---

## Decisiones de diseño

### 1. `Arc<SourceFile>` vs `&SourceFile`

**Elegido**: `Arc<SourceFile>` dentro de cada `Span`.

**Alternativas**:
- `&'src SourceFile` con lifetime propagado por todo el AST. Obliga a cada tipo del compilador a llevar un lifetime genérico. Contamina APIs de lexer, parser, semantic, types, etc.
- Índices a una tabla global de SourceFiles. Funciona pero requiere pasar esa tabla a todo quien inspecciona spans, incluyendo tests.

**Justificación**: el overhead de `Arc` es despreciable comparado con el costo arquitectónico de propagar un lifetime por todo el compilador. Clonar un `Span` es una operación de bump al contador atómico.

### 2. `Span` es `Clone` pero no `Copy`

**Elegido**: derive `Clone, PartialEq, Eq` pero no `Copy`.

**Justificación**: `Copy` requeriría que `Span` no contenga `Arc`. Como en la práctica los spans rara vez se copian en hot paths (solo cuando se construye un nodo), el uso explícito de `.clone()` es aceptable y documenta el aliasing.

### 3. `line_starts` precomputado vs recálculo por consulta

**Elegido**: precomputar en `SourceFile::new` y guardar en el struct.

**Justificación**: `line_col` se llama muchas veces al emitir diagnósticos (una vez por cada label de cada error). Recalcular lineales en cada llamada sería O(n) donde n es el offset; con precómputo es O(log k) donde k es el número de líneas.

### 4. Labels sin distinción primary/secondary

**Elegido**: todos los labels se emiten como `primary` en `codespan-reporting`.

**Alternativas**: añadir un enum `LabelStyle` y pasarlo al renderizado.

**Justificación**: la distinción primary/secondary aporta valor solo cuando hay múltiples ubicaciones con roles distintos en un mismo diagnóstico. Para un compilador didáctico, todos los labels son puntos de interés; introducirlo sin necesidad real sería sobre-ingeniería. Se puede añadir cuando haga falta.

### 5. `DiagnosticBag::drain` devuelve `Vec<Diagnostic>`

**Elegido**: `drain(&mut self) -> Vec<Diagnostic>` usando `std::mem::take`.

**Alternativas**: `drain(&mut self) -> Drain<'_, Diagnostic>` (iterador con lifetime).

**Justificación**: el driver necesita propietariar los diagnósticos para pasarlos entre fases o emitirlos diferidamente. Un `Vec` es más simple de mover. El costo de allocación es irrelevante porque se hace una vez al final de cada fase.

---

## Gotchas

### `Span::dummy` apunta al offset 0

Un `Span::dummy(file)` produce un span vacío en la posición 0, no `None`. Esto permite que cualquier nodo del AST siempre tenga un span real (aunque sea vacío) sin que nada más tenga que manejar `Option<Span>`. La regla: los spans dummy **solo** se usan en nodos sintéticos generados durante error recovery del parser. Nunca deben aparecer en un `Diagnostic` reportado al usuario final (la sección 8.3 de `rules.md` lo prohíbe).

### Panics en `Span::new`

`Span::new(file, start, end)` hace `assert!(start <= end)` y `assert!(end <= file.source().len())`. Estos son bugs del compilador, no errores del usuario. Si algún código productivo tropieza con estos asserts significa que hay un cálculo de offset incorrecto — no silenciarlo con `saturating_sub` ni similares. Solución correcta: arreglar el cálculo.

### `Diagnostic` requiere UTF-8 válido en mensajes

`codespan-reporting` asume UTF-8. Como `Diagnostic::message` es `String`, Rust garantiza UTF-8 — pero si en algún momento se construye un mensaje a partir de bytes crudos (ej: un lexema con bytes inválidos), hay que sanitizar antes.

---

## Ejemplos de uso

### Construir un `SourceFile` y un `Span`

```rust
use std::sync::Arc;
use hulk_span::{SourceFile, Span};

let file = Arc::new(SourceFile::new("demo.hulk", "let x = 42;"));
let span = Span::new(file.clone(), 4, 5); // cubre la 'x'
assert_eq!(&file.source()[span.range()], "x");
```

### Construir y emitir un diagnóstico

```rust
use hulk_diagnostics::{Diagnostic, DiagnosticBag};

let mut bag = DiagnosticBag::new();
bag.push(
    Diagnostic::error("token inesperado")
        .with_label(span, "se esperaba una expresion")
        .with_note("revisa la sintaxis despues de '='")
);

if bag.has_errors() {
    bag.emit_stderr().unwrap();
}
```

### Drenar errores entre fases (patrón del driver)

```rust
let mut bag = DiagnosticBag::new();
let tokens = hulk_lexer::lex(&file, &mut bag);

if bag.has_errors() {
    bag.emit_stderr().unwrap();
    return; // abortamos antes de pasar al parser
}

let program = hulk_parser::parse(tokens, &mut bag);
// ...
```

### Merge de spans en el parser

```rust
// Al construir un BinOp, el span del nodo abarca desde el inicio del lhs hasta el final del rhs:
let node_span = lhs.span.clone().merge(rhs.span.clone());
```
