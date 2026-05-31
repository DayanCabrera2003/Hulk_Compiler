# Sesión 04 — Parser

> Crate: `hulk-parser`.
> Subsesiones completadas: 4.1 (Pratt parser base), 4.2 (declaraciones + construcciones complejas) y 4.3 (error recovery + tests + docs).

---

## Qué se implementó

### 4.1 — Pratt parser base

- `Parser` struct con estado `{ tokens, pos, bag, node_ids, eof_span }`.
- Navegación: `peek`, `peek_at(offset)`, `peek_span`, `at`, `advance`, `expect`, `expect_ident`, `skip_until`, `error_here`.
- `parse(tokens, source) -> (Program, DiagnosticBag)` entry point.
- `parse_expr_bp(min_bp)` — loop Pratt con postfijos, `:=` y binarios.
- `parse_nud` — literales, identificadores, `self`, `base`, unarios, agrupación, bloques.
- `infix_bp(token)` — tabla centralizada de precedencias.

### 4.2 — Declaraciones y construcciones complejas

**Nuevos módulos** (el crate está dividido en varios archivos para respetar el límite de 500 líneas por archivo de `rules.md`):

| Archivo | Responsabilidad |
|---|---|
| [`src/lib.rs`](crates/hulk-parser/src/lib.rs) | `Parser` struct, helpers de navegación, API pública `parse()` |
| [`src/expr.rs`](crates/hulk-parser/src/expr.rs) | `parse_expression`, `parse_expr_bp`, `parse_nud`, postfijos, `:=`, bloques, tabla `infix_bp` |
| [`src/complex.rs`](crates/hulk-parser/src/complex.rs) | `let`, `if/elif/else`, `while`, `for`, `new`, lambdas, vectores (literal/generador), `parse_param_list` |
| [`src/decl.rs`](crates/hulk-parser/src/decl.rs) | `parse_program`, `parse_function_decl`, `parse_type_decl` + miembros, `parse_protocol_decl`, `parse_macro_decl` |
| [`src/type_ann.rs`](crates/hulk-parser/src/type_ann.rs) | `parse_type_ann` para `T`, `T*`, `T[]`, `(A)->B` |
| [`tests/declarations.rs`](crates/hulk-parser/tests/declarations.rs) | 89 tests de integración (ver "Cobertura de tests") |

**Funciones públicas** expuestas desde `hulk-parser`: solo `parse()`. Todo lo demás es `pub(crate)` para que los submódulos del parser puedan compartir la lógica sin exponerla fuera del crate.

### Bugs encontrados y corregidos durante 4.2

| # | Bug | Archivo afectado | Fix |
|---|---|---|---|
| 1 | `LetBinding` no tenía `type_ann`, impedía parsear `let x: Number = 42 in ...` | `hulk-ast/src/decl.rs` | Añadido `type_ann: Option<TypeAnn>` (consistente con `Param` y `Attribute`) |
| 2 | Test de arquitectura contaba dev-dependencies como violación de capas | `hulk-driver/tests/architecture.rs` | Filtrado por `DependencyKind::Normal` |
| 3 | Unary tenía `bp=17`, causaba `-x ^ 2` → `-(x^2)` en vez de `(-x)^2` | `hulk-parser/src/expr.rs` | Subido a `bp=19` (mayor que `l_bp` de `^`) |
| 4 | Generador `[e | x in it]` se rompía porque `|` se consumía como `Or` binario | `hulk-parser/src/complex.rs` | Primer elemento parseado con `parse_expr_bp(4)` (stop antes de `Or`) |

Los 4 pasaron a tests de regresión en `tests/declarations.rs`.

### Limitación conocida (no bug)

- `MacroDecl` no tiene campo `return_type`, pero la sintaxis `def foo(...): Type => ...` de hulk-docs.pdf sí permite anotación. El parser acepta y **descarta** el `: Type` después de `)` en macros, con un comentario `// TODO` en `parse_macro_decl` apuntando al fix futuro. Cuando se añada `return_type: Option<TypeAnn>` a `MacroDecl`, reemplazar el `let _discarded = ...` por un store.

---

## Decisiones de diseño

### 1. División del parser en submódulos

Con la suma de declaraciones, construcciones complejas y type annotations, el parser superó las ~500 líneas que `rules.md` fija como límite por archivo. Se partió en 5 archivos que corresponden a capas naturales del parser:

- **Navegación** (lib.rs) — primitivas mínimas que no saben nada del lenguaje.
- **Expresiones y Pratt** (expr.rs) — lógica de precedencia y postfijos.
- **Complejas** (complex.rs) — construcciones que empiezan con un keyword (`let`, `if`, etc.) o un delimitador (`[`).
- **Declaraciones** (decl.rs) — el nivel más alto: `function`, `type`, `protocol`, `def`.
- **Tipos** (type_ann.rs) — aislado para reutilizar desde todas las demás.

Alternativas descartadas:
- Un solo archivo grande: rompe `rules.md`, difícil de navegar.
- Un submódulo por construcción (`let.rs`, `if.rs`, etc.): sobre-granular, multiplica boilerplate.

### 2. Postfijos en el loop Pratt con bp "infinita"

`.field`, `.method()`, `[i]`, `f(args)`, `is Type`, `as Type` son postfijos que **siempre** ligan más fuerte que cualquier binario. En vez de añadirlos a `infix_bp` con bp alta, se chequean antes en el loop de `parse_expr_bp`:

```rust
loop {
    if self.can_start_postfix() { lhs = self.parse_postfix(lhs); continue; }
    if ColonEqual { ... }
    if let Some((op, l_bp, r_bp)) = self.infix_bp() { ... }
    break;
}
```

**Justificación**: los postfijos no tienen "right operand" — consumen tokens específicos (como el nombre del método, los args, o la anotación de tipo), no otra expresión genérica. Meterlos en `infix_bp` requeriría una bp alta pero también caso especial en el manejo del rhs, lo que complica la tabla. Separarlos es más limpio.

### 3. Detección lambda vs grouping por lookahead

Cuando `parse_nud` ve `(`, la ambigüedad entre lambda `(x) => body` y grupo `(expr)` se resuelve con lookahead de 3 tokens (`is_lambda_start`):

| Patrón detectado (offsets desde `(`) | Interpretación |
|---|---|
| `( ) =>` o `( ) :` | Lambda sin parámetros |
| `( ident : …` | Lambda con param tipado |
| `( ident , …` | Lambda con múltiples params |
| `( ident ) =>` o `( ident ) :` | Lambda con un param |
| cualquier otro | Grouping (paréntesis de expresión) |

**Justificación**: lookahead de ventana fija es O(1) y mantiene el parser en single-pass sin backtracking. Alternativa (try-parse + rollback) requiere manejo de estado y puede duplicar trabajo.

### 4. `:=` con manejo explícito fuera de `infix_bp`

`:=` necesita convertir el lhs en un `AssignTarget` (no en cualquier expresión). Si estuviera en `infix_bp`, el flujo genérico construiría un `BinOp` y luego habría que reescribirlo. Con un check explícito en el loop:

```rust
if matches!(self.peek(), Token::ColonEqual) {
    // parse rhs, then lhs_to_assign_target(lhs)
}
```

se valida la conversión y se emite un diagnóstico específico si el lhs no es válido (ej: `(1 + 2) := 3` reporta "objetivo de ':=' inválido").

### 5. `[e | x in it]` vs `[a | b]` — prioridad al generador

`|` es el operador `Or` en expresiones y también el separador del generador. En `[x*2 | x in range(0,10)]`, si parseáramos el primer ítem como expresión completa, `|` se consumiría como `Or` y el parser fallaría al llegar a `x in range(0,10)]`.

**Solución**: el primer ítem de un vector se parsea con `parse_expr_bp(4)`, que detiene el parser antes de un `Or` (l_bp=3). Luego se chequea explícitamente:

- Si el siguiente token es `|` → generador.
- Si es `,` o `]` → literal, continuar con el resto.

**Consecuencia**: dentro de un vector no se puede poner un `Or` o un `:=` sin paréntesis. `[a | b]` se interpreta como generador (y falla luego porque no hay `in`). Si el usuario quiere Or, escribe `[(a | b)]`.

### 6. Parser nunca devuelve `Result`

Siguiendo rules.md sección 8, el parser siempre devuelve un `Program` aun con errores de sintaxis. En puntos de error:

- Emite un `Diagnostic` al bag.
- Llama a `skip_until` con un set de tokens de sincronización.
- Produce un nodo sintético (bloque vacío, target inválido) con span válido.
- Continúa parseando.

Esto garantiza que el driver pueda pedir cualquier fase posterior aun con errores, y que múltiples errores se reporten en una sola pasada (criterio de `rules.md` 8.2).

### 7. Return types en macros: parse-and-discard

La sintaxis `def foo(params): RetType => ...` aparece en ejemplos de hulk-docs.pdf, pero `MacroDecl` no tiene campo `return_type`. En lugar de bloquear la subsesión modificando el AST de nuevo o rechazar la sintaxis, el parser consume y descarta el `: Type` tras `)`. Se dejó un TODO claro para añadir el campo cuando se trabaje macros (sesión 10).

---

## Gotchas

- **`parse_function_body` requiere `;` en la forma inline pero no en la de bloque.** El `;` final de `function f() => expr;` lo consume el propio `parse_function_body`. En la forma `{ }`, el bloque maneja sus propios `;` internos y no hay `;` extra al final.

- **`parse_type_ann` consume `*` y `[]` postfijos greedy.** Si escribes `Number[]*` produces `Iterable(Vector(Number))` — poco idiomático pero representable. Si el parser no encuentra `]` después de `[`, no intenta consumirlo como vector y lo deja para el caller (evita confundir acceso a índice con anotación de tipo).

- **`parse_macro_param` no clasifica por prefijo hasta después de consumirlo.** La clasificación pasa por una variable `kind_tag: &'static str` y se materializa al construir la variante al final. Esto simplifica el control de flujo sin duplicar 4 ramas paralelas.

- **`parse_member` distingue atributo vs método por lookahead tras el nombre.** Tras consumir el ident del miembro, si sigue `(` → método; si sigue `:` o `=` → atributo. Esto implementa la regla informal del PIPELINE "ident(…) es método, ident = o ident : es atributo".

- **`previous_span()` se usa para cerrar spans de construcciones de longitud variable.** Por ejemplo, `new Point(1, 2, 3)` no sabe dónde termina hasta que `parse_paren_args` consumió el `)` — `previous_span()` da el span de ese `)` para mergear con el span inicial de `new`.

- **El `Arc<SourceFile>` del spans se clona implícitamente** cada vez que se construye un nuevo `Expr` con `span.merge(...)`. Esto es barato (contador atómico) pero conviene tenerlo en mente si en el futuro se optimiza: podría usarse `Cow` o índices a un pool.

- **`is_lambda_start` asume que `(` aún no fue consumido** — offset 0 es el propio `(`. Si cambiara la convención (consumir `(` antes de llamar), los offsets +1/+2/+3 quedarían desfasados. El debug_assert lo documenta.

---

## Cobertura de tests

**Tests totales**: 92 (`cargo test -p hulk-parser`)
- 3 unit tests en `src/tests.rs` (4.1 regresión: precedencia, unarios, booleanos+concat).
- 89 integration tests en `tests/declarations.rs` (4.2).

**Tests organizados por sección**:

| Sección | Tests | Cubre |
|---|---|---|
| `let ... in ...` | 5 | Binding simple, con anotación, múltiples, body bloque, anidamiento derecho |
| `if / elif / else` | 4 | If+else, if+elif+elif+else, if sin else, bloques en ramas |
| `while / for` | 2 | While con `:=`, for sobre `range(...)` |
| `:=` destructive | 5 | A ident, a field, a index, asociatividad derecha, target inválido → error |
| Postfijos | 10 | `.field` chain, método con/sin args, cadena de métodos, `v[i]`, `obj.items()[0]`, `is`, `as`, `f(args)`, `f()`, `base()` |
| `new` | 2 | Con args, sin args |
| Vector literal/generator | 4 | Literal, vacío, generador, `[lit][idx]` |
| Lambdas | 5 | Sin anotación, con tipos, sin params, múltiples params, grouping vs lambda |
| Type annotations | 4 | `T*`, `T[]`, `(A)->B`, `()->A` |
| Function decls | 5 | Inline, con tipos, full-form, recursivo, múltiples |
| Type decls | 7 | Atributo, atributo con tipo, método inline, método bloque, constructor params, herencia, sin herencia |
| Protocol decls | 4 | Con return types, sin extends, con extends, sin return type → error |
| Macro decls | 4 | Regular+body, symbolic, placeholder, tipos preservados |
| Programa completo | 1 | `function`+`type`+`protocol`+`def`+body |
| Precedencia | 4 | Postfijo vs binario, unario vs `^`, `^` derecha, `:=` lowest |
| Edge cases | 7 | Source vacío, solo `;`, 50 calls anidadas, `let...in let...in`, `base.foo()`, `for` sobre vector, `new` con vector arg |
| NodeId uniqueness | 1 | Todos los NodeIds en un programa grande son únicos |
| Errores esperados | 3 | `type Point 0;`, `let x = 1 x;`, solo decl sin body |
| Ejemplos hulk-docs.pdf | 14 | Hello world, aritmética, let mult bindings, let redef, `:=`, elif, gcd while, `for` range, Point methods, Knight inherits + base, Iterable protocol, vector generator, lambda, is/as, macro repeat |

**Ejemplos literales de hulk-docs.pdf** parseados correctamente: el bloque "Ejemplos hulk-docs.pdf" toma snippets directamente de la spec y verifica que el parser no emite errores sobre ellos.

**Validación final**:
- `cargo test -p hulk-parser` → 92/92 passed
- `cargo clippy -p hulk-parser --all-targets -- -D warnings` → limpio
- `cargo test --workspace` → todos los crates verdes (138 tests totales)
- `cargo clippy --workspace --all-targets -- -D warnings` → limpio
- `cargo fmt --all --check` → limpio

---

## 4.3 — Error recovery, tests y docs

### Qué se implementó

**Nuevas primitivas en `src/lib.rs`**:

| Símbolo | Propósito |
|---|---|
| `skip_to_sync()` | Recorta hasta el conjunto canónico de tokens de sincronización: `; } EOF function type protocol def`. |
| `position()` | Accesor del cursor; las iteraciones lo usan para detectar falta de progreso. |
| `ensure_progress(before)` | Fuerza un `advance` si no hubo progreso desde `before`, garantizando terminación. |
| `peek_is_recovery_boundary()` | `true` cuando el token actual es `Eof` o el inicio de una nueva declaración top-level (`function`/`type`/`protocol`/`def`). Señal para abortar un parse con header roto. |

**Puntos con guarda de progreso** (todos los loops que parsean una lista delimitada):

| Loop | Archivo | Recovery |
|---|---|---|
| Declaraciones top-level (`while is_decl_start`) | [src/decl.rs](crates/hulk-parser/src/decl.rs) | `skip_to_sync` + `ensure_progress` si una iteración no consumió tokens |
| Miembros de `type { … }` | [src/decl.rs](crates/hulk-parser/src/decl.rs) | `skip_until(&[Semicolon, RBrace, Eof])`, come `;` si corresponde |
| Firmas de `protocol { … }` | [src/decl.rs](crates/hulk-parser/src/decl.rs) | Idéntica estrategia que miembros |
| Statements de `{ … }` | [src/expr.rs](crates/hulk-parser/src/expr.rs) (preexistente en 4.2) | Diagnóstico + `skip_until(&[Semicolon, RBrace, Eof])` |

**Gate de header en `type` / `protocol`**:

`parse_type_decl` y `parse_protocol_decl` verifican explícitamente antes de llamar al loop de miembros/firmas:

```rust
if !self.at(&Token::LBrace) && self.peek_is_recovery_boundary() {
    self.expect(&Token::LBrace, "…");     // emite el diagnóstico
    return <TypeDecl|ProtocolDecl> sintético con members vacíos;
}
```

Sin esta guarda, `type Foo function bar() => 1;` haría que el loop de miembros (que itera hasta `RBrace` o `EOF`) consuma `function bar() => 1;` interpretándolo como miembros rotos, perdiendo la siguiente declaración. El test `type_without_opening_brace_does_not_swallow_next_decl` cubre exactamente este caso y era rojo antes del fix.

### Puntos de sincronización

Estos son los tokens que "cierran" una construcción y permiten al parser reincorporarse al flujo normal después de un error. La tabla es el contrato sobre el que los tests de recovery razonan:

| Token de sync | Abre qué | Usado por |
|---|---|---|
| `;` | Fin de statement / atributo / firma | bloques, miembros, firmas de protocolo |
| `}` | Fin de bloque / cuerpo de type / cuerpo de protocolo | bloques, miembros, firmas |
| `function` / `type` / `protocol` / `def` | Inicio de la siguiente declaración top-level | `parse_program` |
| `Eof` | Fin del programa | todos (implícito en `skip_until`) |

El método `skip_to_sync()` encapsula la lista completa para que las decisiones de alto nivel no dupliquen el set.

### Regla invariante

**El parser nunca devuelve `Result`**. Siempre retorna un `(Program, DiagnosticBag)`. En cualquier sitio donde un `expect` falla:

1. Se emite un `Diagnostic` con `span` primario al token ofensor.
2. Si la falla podría dejar al parser atascado (sin consumir tokens), se llama a `skip_until` o `skip_to_sync`.
3. Se devuelve un nodo sintético (bloque vacío, `AssignTarget::Ident("")`, `TypeAnn::Named("")`, etc.) con un span válido o dummy.
4. El loop padre usa `ensure_progress` para garantizar que la iteración termina.

Consecuencia directa: el driver puede pedir cualquier fase posterior aun con errores de sintaxis, y **todos los errores de un archivo se reportan en una sola pasada** (requisito de `rules.md` §8.2).

### Garantía de terminación

Toda lista de construcciones (declaraciones, miembros, firmas) respeta este patrón:

```rust
let before = self.position();
items.push(self.parse_item());
if self.position() == before {
    self.skip_until(&[...]);
    self.ensure_progress(before);
}
```

Si `parse_item` falla sin consumir tokens, el bloque de recovery salta al próximo token sync y —si sigue sin haber progreso— fuerza un `advance` individual. El parser no puede iterar infinitamente sobre el mismo token, incluso frente a input sintético adversario.

Esto se valida en [`tests/error_recovery.rs`](crates/hulk-parser/tests/error_recovery.rs) con:

- `fuzz_like_inputs_dont_panic_or_hang`: 200 entradas aleatorias cortas, seed fijo, alfabeto ASCII que el lexer acepta.
- `parser_always_returns_a_program_even_on_garbage`: 8 entradas con secuencias patológicas (`}{(`, `;;;;;`, `@@@@`, etc.).

### Tests nuevos (4.3)

**Ubicación**: [`crates/hulk-parser/tests/error_recovery.rs`](crates/hulk-parser/tests/error_recovery.rs).
**Total**: 25 tests agrupados en:

| Grupo | Qué verifica | Tests |
|---|---|---|
| Terminación | Inputs malformados terminan en tiempo acotado | 5 |
| Sincronización | Contenido post-error sigue parseando | 5 |
| Recovery específico | Cada sitio de `expect` puntual | 6 |
| Header roto no engulle | `type`/`protocol`/`function`/`def` sin body dejan intacta la siguiente decl | 5 |
| Nodos sintéticos | `parse` siempre devuelve un `Program` (incluso para visitors) | 3 |
| Fuzz | 200 entradas aleatorias sin panic/loop | 1 |

### Gotchas encontrados en 4.3

- **`parse_member` sin guarda es vulnerable**: si el miembro empieza con `@` o `$`, `expect_ident` emite un diagnóstico pero no avanza, luego las comprobaciones de `(`/`:`/`=` son todas false, y el parser queda con `pos` sin cambiar. La guarda en `parse_type_members` detecta esto y salta a `;`/`}`.
- **`parse_method_sig` tiene el mismo riesgo** si el primer token no es ident. Resuelto por la misma guarda en `parse_protocol_decl`.
- **Top-level progress guard no es redundante**: aun con ident-expect y body-expect fallando, un `function` sin nada después deja el parser en la misma posición. El `skip_to_sync` del loop top-level salta al próximo `function`/`type`/etc.
- **`skip_until` nunca incluye el token sync entre los consumidos**: se detiene *antes* de él. Es responsabilidad del caller decidir si avanzar (`advance` explícito) o delegar al siguiente parse. Los tests verifican esta semántica implícitamente.

### Cobertura de tests tras 4.3

**Total**: 120 tests en `hulk-parser` (`cargo test -p hulk-parser`).
- 3 unit tests en `src/tests.rs`.
- 92 integration tests en `tests/declarations.rs`.
- 25 integration tests en `tests/error_recovery.rs` (4.3).

**Validación final**:
- `cargo test -p hulk-parser` → 120/120 passed
- `cargo clippy -p hulk-parser --all-targets -- -D warnings` → limpio
- `cargo test --workspace` → todos los crates verdes
- `cargo clippy --workspace --all-targets -- -D warnings` → limpio
- `cargo fmt --all --check` → limpio
