# Sesión 16 — Prelude, Driver y CLI

## Qué se implementó

### 16.1 Prelude

**Archivo**: `prelude/prelude.hulk`

El prelude es un fragmento de código HULK que se prepende automáticamente al fuente del usuario antes de cualquier fase de compilación. Define los protocolos y tipos fundamentales descritos en Hulk.md §15:

- `protocol Iterable`: interfaz mínima para iterables con `next(): Boolean` y `current(): Object`.
- `protocol Enumerable`: extiende la idea de colecciones que producen un iterador vía `iter(): Iterable`.
- `type Range(min, max)`: tipo concreto que implementa el protocolo iterable con un contador interno; permite la construcción `new Range(a, b)` usada en los bucles `for`.

El prelude no tiene expresión de cuerpo; el parser lo acepta porque ya manejaba programas sin cuerpo desde la sesión 4.

**Cómo se procesa**: en `crates/hulk-driver/src/compile.rs`, la constante `PRELUDE` se incrusta en el binario con `include_str!("../../../prelude/prelude.hulk")`. Antes de lexar, el driver combina ambos textos:

```rust
let combined = format!("{PRELUDE}\n{source_text}");
```

Esto garantiza que `Range`, `Iterable` y `Enumerable` estén en scope para cualquier programa de usuario sin que el usuario tenga que importarlos.

---

### 16.2 `hulk-driver`

**Crate**: `crates/hulk-driver/`

#### `CompileOptions` y `EmitKind`

`crates/hulk-driver/src/options.rs` define:

```
EmitKind: Tokens | Ast | Hir | Banner | LlvmIr | Object | Executable (default)
CompileOptions { emit: EmitKind, output: Option<PathBuf>, optimization_level: u8 }
```

#### Pipeline de compilación

`crates/hulk-driver/src/compile.rs` implementa las dos entradas públicas del driver:

| Función | Propósito |
|---------|-----------|
| `compile(path, opts)` | Compila un archivo HULK hasta el artefacto indicado por `EmitKind`; retorna la ruta del artefacto o los diagnósticos de error. |
| `check(path)` | Ejecuta solo las fases semánticas (lex → parse → resolve → type-infer) sin producir código; útil para verificación en IDE. |

El pipeline de `compile` sigue este orden con cortes tempranos por `EmitKind`:

```
lex  →[Tokens]→  parse  →[Ast]→  resolve+infer  →[Hir]→
macros+desugar  →[Banner]→  emit_ir_string  →[LlvmIr]→
codegen_compile  →[Object|Executable]
```

Cada corte escribe el texto o binario en `options.output` (o en un path derivado del nombre del fuente) y retorna esa ruta.

#### Diagnósticos de I/O

Los errores de lectura/escritura se convierten en `Vec<Diagnostic>` para que la interfaz de error sea uniforme en toda la toolchain.

#### Inspección de representaciones intermedias

Con `--emit tokens` se puede ver la secuencia de tokens en formato debug:
```
hulkc compile examples/hello.hulk --emit tokens
```

Con `--emit ast` se obtiene el AST antes de la resolución de nombres.
Con `--emit hir` se obtiene el árbol después de resolución e inferencia de tipos.
Con `--emit banner` se obtiene el IR de tres direcciones listo para codegen.
Con `--emit llvm-ir` se obtiene el IR de LLVM en texto plano (`.ll`).

---

### 16.3 `hulk-cli`

**Crate**: `crates/hulk-cli/` — binario `hulkc`

La CLI usa `clap` con la macro derive para definir tres subcomandos:

#### `hulkc compile <FILE> [--emit <KIND>] [--output <PATH>]`

Compila un archivo HULK. `--emit` acepta: `tokens`, `ast`, `hir`, `banner`, `llvm-ir`, `object`, `executable` (default). `--output` sobreescribe la ruta de salida.

```sh
hulkc compile examples/hello.hulk
hulkc compile examples/hello.hulk --emit banner
hulkc compile examples/hello.hulk --emit llvm-ir --output /tmp/hello.ll
hulkc compile examples/hello.hulk --emit object --output /tmp/hello.o
```

#### `hulkc run <FILE>`

Compila el archivo en un ejecutable temporal y lo ejecuta inmediatamente. El binario se deposita en `$TMPDIR` con un nombre derivado del stem del fuente.

```sh
hulkc run examples/hello.hulk
# → imprime: hello
```

#### `hulkc check <FILE>`

Ejecuta solo el análisis semántico (sin producir código) e informa si hay errores. Útil en pre-commit hooks o integraciones con editores.

```sh
hulkc check examples/classes.hulk
# → "Sin errores" / lista de errores semánticos
```

---

## Decisiones de diseño

### Por qué el prelude usa `include_str!`

Incrustarlo en el binario garantiza que el compilador siempre tenga acceso al prelude sin depender de la ruta del sistema de archivos en tiempo de ejecución. Si el prelude fuera un archivo de configuración externo, cualquier instalación incompleta rompería todos los programas.

### Por qué `build_pipeline` y `build_hir` públicos en `lib.rs`

Los tests de sesiones anteriores (hulk-hir, full_pipeline) acceden directamente a estas funciones de bajo nivel para construir HIRs sin pasar por el pipeline completo. Mantener la API pública preserva esos tests sin modificar. Los tests que usan ejemplos ahora prependen `PRELUDE` manualmente mediante `format!("{PRELUDE}\n{source}")`.

### Por qué `EmitKind` tiene `Executable` como default

El caso de uso más común del compilador es producir un ejecutable. El default evita que el usuario tenga que especificar `--emit executable` en el caso mayoritario.

### `hulkc run` vs scripts de shell

La alternativa era documentar `hulkc compile FILE && ./FILE` como flujo estándar. Se eligió `run` como subcomando porque simplifica el ciclo de desarrollo (un solo comando) y es coherente con herramientas como `cargo run`.

---

## Gotchas conocidos

- **`identifier_program_always_parses`** (hulk-parser property test): falla con el input `"as;"` porque `as` es palabra reservada. Este fallo es anterior a la sesión 16 y está fuera del scope de este trabajo.
- **`hulk-codegen` depende de `hulk-hir`**: la whitelist del test de arquitectura original no incluía esta dependencia. Se actualizó en `tests/architecture.rs` para reflejar la realidad del Cargo.toml de codegen.
- Los tests de parser que verificaban que `iterables.hulk` declarara `Iterable` y `Range` se actualizaron: esas declaraciones viven ahora en el prelude, y el nuevo módulo `prelude.rs` de tests cubre esa funcionalidad.
