# Sesión 5 — Testing exhaustivo del frontend

Esta sesión no añade funcionalidad: valida exhaustivamente que lexer + parser
cubren HULK. Se divide en tres subsesiones.

## 5.1 — Suite de programas HULK válidos ✓

### Qué se implementó

- **`examples/` en la raíz del repo** (nuevo): 13 programas `.hulk` canónicos,
  uno por categoría listada en `PIPELINE.md § 5.1`. Cada archivo lleva un
  comentario de cabecera con la sección de `hulk-docs.pdf` de donde se tomó el
  snippet; cuando el snippet original era un *fragmento* (declaración sin
  expresión global final) se añadió la expresión mínima de cierre (`0;` o
  una invocación del símbolo declarado) para que forme un programa HULK
  completo.
- **`crates/hulk-parser/tests/integration/`** (nuevo, binario de test
  auto-descubierto por Cargo vía `tests/integration/main.rs`): un módulo por
  feature + `combined.rs` para intersecciones. Cada módulo embebe el `.hulk`
  correspondiente con `include_str!` y verifica estructuralmente el AST
  resultante (no solo que "no panice").
- **`crates/hulk-parser/examples/parse_file.rs`** (nuevo, binario de
  inspección manual): utilidad para parsear un archivo y volcar tokens,
  resumen de declaraciones, AST completo y diagnósticos. Uso:
  ```
  cargo run -p hulk-parser --example parse_file -- examples/hello.hulk
  cargo run -p hulk-parser --example parse_file -- examples/hello.hulk --tokens
  cargo run -p hulk-parser --example parse_file -- examples/hello.hulk --ast
  ```
- **Fix de lexer detectado durante la sesión (reportado al humano antes del
  commit)**: `hulk_lexer::Lexer::consume_comment` avanzaba `self.cursor += 1`
  dentro de `//`, lo cual provocaba `panic` en cualquier fuente con caracteres
  multibyte UTF-8 (tildes, em-dash, emoji) dentro de comentarios. Se reemplazó
  por `self.advance_char()` y se añadieron dos tests de regresión en
  `crates/hulk-lexer/src/lib.rs` (`consume_comment_tolerates_multibyte_utf8`,
  `utf8_in_comment_between_tokens`). El fix pertenece semánticamente a la
  sesión 2.2 y debería commit-earse aparte con mensaje
  `[S 2.2 fix] consume_comment avanza por codepoint UTF-8`.

### Programas cubiertos

| Archivo `.hulk`       | Features ejercitadas                                                                   | Origen en `hulk-docs.pdf`         |
|-----------------------|-----------------------------------------------------------------------------------------|-----------------------------|
| `hello.hulk`          | `print`, literal de string                                                              | línea 82                    |
| `arithmetic.hulk`     | precedencia, potencia (`^`), `%`, unarios, builtins `sin`/`cos`/`log`/`PI`              | 72, 115                     |
| `strings.hulk`        | `@`, `@@`, escapes `\"`/`\n`/`\t`, concat con número                                    | 82, 88, 96                  |
| `let_scoping.hulk`    | múltiples bindings, anidamiento, redefinición en block, `:=`                            | 211, 237, 284, 311          |
| `conditionals.hulk`   | `if`/`elif`/`else`, ramas con bloques, `if` sin `else`                                  | 362, 383, 396               |
| `loops.hulk`          | `while` con `:=`, `for` sobre `range`, `for` sobre literal de vector                    | 416, 439                    |
| `functions.hulk`      | inline (`=>`), full-form (`{}`), recursión (`fib`), mutuas (`cot`/`tan`)                | 148, 158, 174               |
| `classes.hulk`        | constructor, atributos anotados, métodos, `inherits` con forwarding, `self`, `base()`   | 468, 529, 570               |
| `protocols.hulk`      | `protocol`, `extends`, conformance implícita con anotación de tipo protocolo            | 882, 892, 938, 1039         |
| `iterables.hulk`      | implementación canónica de `Iterable` vía `Range` + `for` sobre instancia               | 938, 947                    |
| `vectors.hulk`        | literal `[…]`, generador `[e | x in it]`, indexing, `T*`, `T[]`                         | 994, 1003, 1067, 1077, 1087 |
| `functors.hulk`       | `protocol NumberFilter`, lambdas anotadas y sin anotar, anotación `(A)->B`              | 1154, 1196, 1228, 1261      |
| `macros.hulk`         | `def` con `Regular`, `*` (body), `@` (symbolic), `$` (placeholder)                      | 1342, 1396, 1437            |

El módulo `combined.rs` añade **13 tests cruzados** que intersectan múltiples
features (lambda + generador, `for` dentro de método, `is`/`as` con herencia,
cadena method-call + index, anotación functor en `let`, anidamiento profundo,
unicidad de `NodeId` en un programa denso, etc.).

Total tests de 5.1: **98 tests de integración** (`cargo test -p hulk-parser
--test integration`), todos en verde.

### Decisiones de diseño

- **Ubicación de `examples/`**: raíz del repo. Las sesiones 16 y 17 ya asumen
  esta ruta, y que exista desde ahora evita duplicación. Los tests acceden por
  `include_str!("../../../../examples/<feature>.hulk")`.
- **Estructura `tests/integration/main.rs` + módulos**: Cargo auto-descubre
  `tests/<dir>/main.rs` como un único binario de test. Ventaja: un solo
  `cargo test --test integration`, y un único ciclo de compilación en lugar
  de ≈14 binarios separados. Ventaja secundaria: un módulo `common` que se
  reutiliza (`parse_ok`, `count_exprs`, `contains_expr`).
- **Carga de examples con `include_str!` vs `fs::read`**: se eligió
  `include_str!`. El path es relativo al archivo fuente y resuelto en
  build-time, así que los tests no dependen del directorio de ejecución
  (funcionan igual al lanzar `cargo test` desde cualquier subdirectorio).
- **Las aserciones verifican estructura, no texto**: cada test descompone el
  AST con `let … else { panic!(…) }` y verifica formas precisas (`Block` con
  N elementos, `FunctionDecl` con cierto nombre, `TypeAnn::Functor { … }`,
  etc.). Nunca se hace `assert!(result.is_ok())` genérico (prohibido por
  `rules.md § 12`).
- **Copia literal de `hulk-docs.pdf` con *envoltura mínima*** cuando la spec ofrece
  un fragmento: el cuerpo del fragmento se mantiene textual; solo se añade la
  expresión global final imprescindible (p. ej. `operate(10, 5);` tras la
  declaración de `operate`). La envoltura se documenta con un comentario de
  cabecera en el `.hulk`.
- **Exclusión de `match`/`case` en `macros.hulk`**: `hulk-parser` aún no
  parsea `match { case … }` dentro de bodies de macro (los tokens `Match` y
  `Case` existen pero no hay gramática). Se difiere a la sesión 10
  (`hulk-macros`), cuando la sintaxis se pueda ejecutar y probar end-to-end.

### Gotchas

- **Ambigüedad `|` en generadores con lambda como elemento**: el token `Pipe`
  tiene doble rol — separador del generador `[ expr | x in it ]` y operador
  binario `Or` (BP 3/4 en la tabla de precedencias). Un programa como
  `[ (y) => y*y | x in range(0, 5) ]` falla porque el body de la lambda (BP
  0) consume el `|` como `Or`. El parser resuelve la ambigüedad parseando el
  elemento del generador con `parse_expr_bp(4)`, pero esto solo funciona
  cuando el elemento NO es una sub-expresión que haya iniciado antes su
  propio frame con BP bajo (como la lambda, que llama a `parse_expression()`
  para su body). El test `combined::lambda_coexists_with_vec_generator`
  documenta esta limitación y rodea el caso colocando la lambda en un `let`
  externo.
- **Panic UTF-8 en comentarios del lexer**: descubierto al correr el parser
  sobre los primeros `.hulk` con tildes/em-dash. Fix separado en `hulk-lexer`
  (ver arriba). Regla: cualquier iteración de cursor en el lexer debe usar
  `advance_char()` o sumar `ch.len_utf8()`, nunca `+= 1`, salvo que el
  contexto garantice ASCII (como `skip_whitespace`).
- **Rustfmt pedido `imports_granularity = Crate`** (nightly feature): el
  warning aparece en estable pero no impide el formato; no se cambia el
  `rustfmt.toml`.

### Ejemplos de uso

Inspeccionar manualmente un programa (CLI ad-hoc):

```
$ cargo run -q -p hulk-parser --example parse_file -- examples/classes.hulk
=== Fuente: examples/classes.hulk (1018 bytes) ===
…

=== Resumen del programa ===
  Funciones: 0
  Tipos: 4
    type Point(2 params)  attrs=2 methods=4
    type PolarPoint(2 params) inherits Point  attrs=0 methods=1
    type Person(2 params)  attrs=2 methods=1
    type Knight(0 params) inherits Person  attrs=0 methods=1
  Protocolos: 0
  Macros: 0
  Cuerpo (tipo): Let

OK — parse limpio, sin diagnósticos.
```

Con `--tokens` se muestra la lista completa de tokens; con `--ast` se vuelca
el `Debug` pretty-printed del `Program`.

Ejecutar solo la suite de 5.1:

```
cargo test -p hulk-parser --test integration
```

## 5.2 — Suite de programas con errores esperados ✓

### Qué se implementó

- **`crates/hulk-parser/tests/errors/`** (nuevo, binario de test independiente
  auto-descubierto vía `tests/errors/main.rs`): 47 tests repartidos en tres
  módulos:
  - `lexical.rs` (14 tests) — las cuatro categorías de errores léxicos
    exigidas por el PIPELINE, cada una con 2-3 variantes.
  - `syntactic.rs` (22 tests) — las siete categorías de errores sintácticos
    exigidas + variantes bonus (asignación a literal, vector sin cerrar,
    lambda sin `=>`, etc.) + verificación de recovery a nivel de declaración.
  - `multi_error.rs` (8 tests) — reporte múltiple en una sola pasada y no-hang
    ante entradas patológicas.
- **`common.rs`** con helpers `parse_capturing_all` (combina diagnósticos de
  lexer + parser), `assert_has_message` (substring del mensaje) y
  `assert_any_error` (severity Error presente).
- **Fix de lexer detectado durante la sesión (reportado al humano)**: el
  fallthrough del `match` en `Lexer::lex_all` ([crates/hulk-lexer/src/lib.rs:100-103](../crates/hulk-lexer/src/lib.rs#L100-L103))
  hacía `cursor += 1` sobre caracteres inesperados multibyte, paniqueando con
  cualquier emoji o letra acentuada fuera de string/comentario (ej: `let 🦀 = 1`).
  Sustituido por `self.advance_char()` y añadidos dos tests de regresión
  (`multibyte_unexpected_char_does_not_panic`,
  `multiple_multibyte_unexpected_chars_each_report_independently`). Este fix es
  el segundo bug de UTF-8 encontrado en el lexer; pertenece semánticamente a
  la sesión 2.2 y debería commit-earse aparte con mensaje
  `[S 2.2 fix] lex_all fallthrough avanza por codepoint UTF-8`.

### Tabla de errores cubiertos

**Léxicos**:

| Error                                      | Mensaje del diagnóstico (substring)                         | Tests                  |
|--------------------------------------------|-------------------------------------------------------------|------------------------|
| Identificador con `_` inicial              | `identificadores en HULK no pueden empezar con '_'`         | 3                      |
| String sin cerrar                          | `string sin cerrar`                                         | 3 (EOF, newline, bare) |
| Secuencia de escape inválida               | `secuencia de escape invalida: \<char>`                      | 3 (uno, dígito, varios)|
| Carácter inesperado                        | `caracter inesperado`                                       | 3 (`#`, `?`, emoji)    |
| Recovery tras error léxico                 | —                                                            | 2                      |

**Sintácticos**:

| Error                                              | Tests                           |
|----------------------------------------------------|---------------------------------|
| `(` sin cerrar (let, call, params)                 | 3                               |
| `function` sin nombre                              | 2 (con `()`, con `;`)           |
| `let` sin `in`                                     | 2 (simple, múltiples bindings)  |
| `if` sin cuerpo / condición / paréntesis           | 3                               |
| `{` sin cerrar (block, type body, function body)   | 3                               |
| Firma de protocolo sin tipo de retorno             | 2 (solo, mezclada con válidos)  |
| `type` sin body                                    | 3 (con `;`, con número, con EOF)|
| Bonus: `let x 1`, lambda sin `=>`, vec abierto, `1 := 2` | 4                         |
| Recovery preserva declaraciones siguientes         | 2                               |
| Forma robusta ante basura total                    | 1                               |

**Multi-error**:

| Escenario                                          | Tests |
|----------------------------------------------------|-------|
| Dos funciones malformadas + una válida             | 1     |
| Error léxico + error sintáctico coexisten          | 1     |
| Múltiples `caracter inesperado` acumulados         | 1     |
| Tres declaraciones rotas → tres válidas recuperadas| 1     |
| `{` sin cerrar + decl posterior, recovery parcial  | 1     |
| No-hang ante 9 entradas patológicas distintas      | 1     |
| 4 escapes inválidos en un solo string              | 1     |
| `tokens extra al final del programa`               | 1     |

### Decisiones de diseño

- **Ubicación paralela a 5.1**: `tests/errors/` junto a `tests/integration/`,
  en lugar de mezclar en `error_recovery.rs` (que pertenece a S 4.3 y usa
  aserciones genéricas `bag.has_errors()`). Cada `tests/<dir>/main.rs` es un
  binario de test independiente, lo que aísla los fallos por categoría.
- **Aserciones por substring del mensaje**: `assert_has_message(&diags,
  "string sin cerrar")` en lugar de comparación literal. Las wording exactas
  pueden cambiar sin invalidar la semántica del test, pero el keyword
  identifica unívocamente la categoría.
- **Recovery verificado explícitamente donde cuenta**: no basta con
  `has_errors()`; cuando aplica, el test comprueba que las declaraciones
  posteriores siguen presentes en el AST (ejemplo:
  `protocol_method_valid_and_invalid_signatures_both_reported` verifica que
  las tres firmas — 2 válidas y 1 rota — aparecen en
  `program.protocols[0].methods`).
- **Separar diagnósticos de lexer y parser en una sola lista**: el helper
  `parse_capturing_all` concatena ambos bags respetando el orden natural
  (lexer primero). Tests como
  `lexical_and_syntactic_errors_coexist_in_one_pass` dependen de esta
  visibilidad unificada.
- **No se añadieron programas `.hulk` en disco para errores**: a diferencia
  de 5.1, cada caso es un snippet minúsculo cuyo interés está justo en su
  forma literal; tenerlos inline hace el test legible sin saltar entre
  archivos.

### Gotchas

- **`$` es un token válido** (`Token::Dollar`, usado en macros
  `$placeholder`). Mi primer intento de test multi-`caracter inesperado`
  usaba `# $ ? ;` y solo contaba 2 errores. Reemplazado por `# ~ ?`.
- **Parser consume `function` durante el recovery de un `let x = in`**: ante
  `let x = in 1; function ok() => 42;` el parser ve el `in` como inicio
  fallido de expresión, y su `skip_to_sync` puede incluir `function` como
  límite pero al reanudar parsea la siguiente declaración de forma parcial.
  El test final no exige que `ok` sobreviva intacto — exige que al menos una
  declaración válida de las que siguen al error esté presente.
- **Forma del body ante basura total no es estable**: `"@@@@"` produce un
  `BinOp(Concat, …)` (no un `Block` vacío). La aserción correcta es "parse
  retornó un Program sin panic" — no "body es Block".
- **Segunda aparición del mismo bug UTF-8** (diferente función del lexer):
  confirmado que hace falta una revisión sistemática de `cursor += 1` en el
  lexer; ambos fixes aislados no descartan una tercera ocurrencia oculta.
  Recomendado auditar en 5.3 con property tests de fuzzing sobre entradas
  UTF-8 aleatorias.

### Ejemplos de uso

Ejecutar solo la suite de 5.2:

```
cargo test -p hulk-parser --test errors
```

Inspeccionar un programa con errores esperados (reusa el CLI de 5.1):

```
cargo run -p hulk-parser --example parse_file -- /dev/stdin <<'EOF'
let _x = 1 x;
EOF
```

Salida esperada (fragmentos):
```
=== Diagnósticos del lexer ===
error: identificadores en HULK no pueden empezar con '_'

=== Diagnósticos del parser ===
error: token inesperado: se esperaba In
```

## 5.3 — Property tests + fuzzing + robustez ✓

### Qué se implementó

- **`crates/hulk-lexer/tests/property.rs`** (nuevo, binario de test
  independiente): 5 tests que combinan property-based testing con `proptest`
  y un fuzz manual determinista:
  - `valid_sources_have_partition_consistent_spans` (unit, sobre los 13
    programas de 5.1): verifica que las spans emitidas particionan el source
    dejando solo whitespace/comentarios en los gaps — la versión testeable
    del requisito "reconstruir el source desde los spans".
  - `lexer_never_panics_on_arbitrary_ascii` (proptest, 512 casos): entradas
    ASCII de hasta 200 caracteres.
  - `lexer_never_panics_on_arbitrary_utf8` (proptest, 512 casos): entradas
    Unicode arbitrarias (regex `\PC`).
  - `lexer_preserves_byte_length_via_spans` (proptest, 512 casos): la suma
    de longitudes de span de los tokens no-EOF es ≤ `source.len()`.
  - `lexer_fuzz_10k_iterations_without_panic` (unit): 10 000 iteraciones
    generadas con LCG determinista, mezclando 70 % alfabeto ASCII + 25 %
    keywords HULK + 5 % caracteres multibyte.
- **`crates/hulk-parser/tests/property.rs`** (nuevo, binario independiente):
  7 tests:
  - `valid_sources_parse_without_diagnostics` (unit): los 13 programas
    parsean sin diagnósticos.
  - `parser_never_panics_on_arbitrary_ascii` (proptest, 512 casos).
  - `parser_never_panics_on_arbitrary_utf8` (proptest, 512 casos).
  - `balanced_parens_always_parse_to_number` (proptest, 512 casos con
    `n in 1..60`): N pares de `()` alrededor de `1;` parsean a `Number(1)` —
    el requisito exacto del PIPELINE.
  - `let_with_arbitrary_number_parses` (proptest): `let x = N in x;` parsea
    a un `Let` para cualquier N finito.
  - `identifier_program_always_parses` (proptest): identificadores
    alfanuméricos simples parsean.
  - `parser_fuzz_10k_iterations_without_panic` (unit): 10 000 iteraciones
    con LCG alimentando al pipeline lexer→parser completo.
- **`proptest` añadido a `[dev-dependencies]`** de `hulk-lexer` y
  `hulk-parser` vía `proptest.workspace = true` (ya estaba en
  `[workspace.dependencies]`).

### Invariantes probadas

| Invariante                                                        | Test(s)                                                            | Nº casos |
|-------------------------------------------------------------------|--------------------------------------------------------------------|----------|
| Partición span-consistente en fuentes válidas                     | `valid_sources_have_partition_consistent_spans`                    | 13       |
| Spans internamente consistentes en ASCII                          | `lexer_never_panics_on_arbitrary_ascii`                            | 512      |
| Spans internamente consistentes en Unicode                        | `lexer_never_panics_on_arbitrary_utf8`                             | 512      |
| Cobertura de span ≤ longitud del source                           | `lexer_preserves_byte_length_via_spans`                            | 512      |
| Lexer no paniquea sobre input pseudo-aleatorio (ASCII + multibyte)| `lexer_fuzz_10k_iterations_without_panic`                          | 10 000   |
| Parser no paniquea sobre input ASCII                              | `parser_never_panics_on_arbitrary_ascii`                           | 512      |
| Parser no paniquea sobre input Unicode                            | `parser_never_panics_on_arbitrary_utf8`                            | 512      |
| N paréntesis balanceados → `Number(1)` (n ∈ 1..60)                | `balanced_parens_always_parse_to_number`                           | 512      |
| `let x = N in x;` parsea como `Let` para N ∈ [-1000, 1000]        | `let_with_arbitrary_number_parses`                                 | 512      |
| Identificadores alfanuméricos parsean                             | `identifier_program_always_parses`                                 | 512      |
| Pipeline lexer→parser no paniquea con LCG                         | `parser_fuzz_10k_iterations_without_panic`                         | 10 000   |

### Decisiones de diseño

- **Interpretación de "reconstruir el source desde los spans"**: los tokens
  no llevan lexeme explícito (`Token::Number(f64)` vs el dígito literal), así
  que reconstrucción literal es imposible sin el source. La formulación
  testeable es la **partición span-consistente**: iterar los tokens en orden
  y verificar que `source[prev.end..curr.start]` contiene solo whitespace o
  `//…\n`. Esta propiedad implica que concatenando lexemas con gaps
  intactos se recupera el source — objetivo semántico equivalente.
- **Property test + fuzz complementarios**: `proptest` explora mutaciones
  guiadas del shrinker (tras un fallo, busca el input mínimo); el fuzz LCG
  garantiza reproducibilidad determinista y volumen alto (10 000 × 2 =
  20 000 iteraciones combinadas). Ambos son necesarios — proptest encuentra
  regressions, el fuzz garantiza ausencia de panic bajo escala.
- **LCG con seed fijo**: `0x1234_5678_9ABC_DEF0` y `0xFEED_FACE_CAFE_BABE`.
  Reproducibilidad entre corridas y CI, sin dependencias adicionales
  (`rand` hubiera sido una dep más para un uso trivial).
- **Mezcla ASCII + keywords + UTF-8 en el fuzz**: 70/25/5. Los keywords
  estresan al parser en rutas que la basura aleatoria tarda en alcanzar; el
  5 % de multibyte garantiza que las dos rutas UTF-8 del lexer que fixeamos
  en 2.2 se ejercitan constantemente.
- **512 casos de proptest**: por encima del default (256) para aumentar
  confianza sin inflar el tiempo de test (la suite completa corre en
  ~0.5 s por test binario).

### Gotchas

- **`\PC` en proptest regex**: la clase `\P{C}` (Unicode non-control) debe
  escribirse `"\\PC"` (dos backslashes) dentro de un string literal de Rust,
  o usar raw strings. Sin escape correcto, la compilación del regex falla
  en runtime.
- **`prop_assert!` y `matches!` con `..`**: `prop_assert!(matches!(x, Let {
  .. }))` falla porque `prop_assert!` concatena el literal del assert como
  format string y `{ .. }` se interpreta como placeholder. Solución:
  pre-computar el booleano (`let is_let = matches!(…); prop_assert!(is_let);`).
- **Clippy `manual_is_multiple_of`**: clippy 1.95 exige
  `n.is_multiple_of(20)` en lugar de `n % 20 == 0`. Aplicado.
- **`parse_capturing_all` vs `lex_and_parse`**: la suite 5.3 combina los
  dos bags (lexer + parser) en el bag del parser via `push` en lugar del
  vector externo que usa 5.2. Misma funcionalidad, adaptado al sitio donde
  hace falta.

### Métricas globales de la sesión 5

| Categoría                              | Archivos | Tests  |
|----------------------------------------|----------|--------|
| 5.1 — programas válidos                | 16       | 98     |
| 5.2 — errores esperados                | 5        | 47     |
| 5.3 — property + fuzz (lexer)          | 1        | 5      |
| 5.3 — property + fuzz (parser)         | 1        | 7      |
| **Totales sección 5**                  | **23**   | **157**|
| Tests restantes del workspace (S 1-4)  | —        | 199    |
| **Total workspace**                    | —        | **356**|

Casos de property/fuzz ejecutados por corrida:

| Categoría                               | Casos  |
|-----------------------------------------|--------|
| Proptest lexer (3 tests × 512)          | 1 536  |
| Proptest parser (5 tests × 512)         | 2 560  |
| Fuzz LCG lexer                          | 10 000 |
| Fuzz LCG parser                         | 10 000 |
| **Total por corrida**                   | **24 096** |

Features cubiertas al 100 % (accesibles desde el parser actual):

- Expresiones, literales, operadores (incl. `^`, `%`, `@`, `@@`, `:=`).
- `let` (bindings múltiples, anidamiento, redefinición).
- `if`/`elif`/`else`, `while`, `for`.
- `function` inline y full-form, recursión.
- `type` con atributos/métodos/herencia/`base`.
- `protocol` con `extends` y firmas tipadas.
- Vectores: literales, generadores, indexing, `T*`, `T[]`.
- Lambdas, functores `(A)->B`.
- Macros: `def` con `Regular`, `*body`, `@symbolic`, `$placeholder`.
- Error recovery: reporte múltiple en una sola pasada.

Features NO cubiertas aún (fuera del alcance del parser / diferidas):

- `match`/`case` en bodies de macro (diferido a S 10 — parser no lo
  contempla todavía).
- Expansión de macros en tiempo de compilación (S 10).
- Desugaring (`for` → `while`, `@@` → `@ " " @`, etc.) (S 11).

### Ejemplos de uso

Ejecutar solo la suite 5.3:

```
cargo test -p hulk-lexer --test property
cargo test -p hulk-parser --test property
```

Forzar más casos en proptest (variable de entorno):

```
PROPTEST_CASES=4096 cargo test -p hulk-parser --test property
```

Reproducir un fallo de proptest concreto:

```
PROPTEST_REGRESSIONS=/tmp/regressions.txt cargo test -p hulk-parser --test property
```
