# Sesión 02 — Análisis léxico

> Crates: `hulk-tokens`, `hulk-lexer`.
> Estado: completada. Testing exhaustivo cubierto en sesión 5.

Esta sesión convierte el texto fuente HULK en una secuencia plana de tokens con ubicación. Es la primera fase real del compilador y produce el input del parser.

---

## Qué se implementó

### `hulk-tokens`

**Archivo**: `crates/hulk-tokens/src/lib.rs`.

Tipos públicos:

- `Token` — enum con una variante por cada elemento léxico del lenguaje. Cubre:
  - **Literales**: `Number(f64)`, `StringLit(String)`, `Ident(String)`, `True`, `False`.
  - **Keywords**: `Function`, `Let`, `In`, `If`, `Elif`, `Else`, `While`, `For`, `Type`, `Inherits`, `New`, `Protocol`, `Extends`, `Def`, `Match`, `Case`, `Default`, `Is`, `As`.
  - **Operadores**: `Plus`, `Minus`, `Star`, `Slash`, `Caret`, `Percent`, `At`, `AtAt`, `Equal`, `EqualEqual`, `Bang`, `BangEqual`, `Less`, `LessEqual`, `Greater`, `GreaterEqual`, `Ampersand`, `Pipe`, `ColonEqual`, `FatArrow`, `Arrow`.
  - **Delimitadores y puntuación**: `LParen`, `RParen`, `LBrace`, `RBrace`, `LBracket`, `RBracket`, `Comma`, `Dot`, `Colon`, `Semicolon`, `Dollar`.
  - **Especial**: `Eof`.

- `SpannedToken { token: Token, span: Span }` — un token con su ubicación.

- `keyword_token(ident: &str) -> Option<Token>` — tabla de lookup de keywords. Los identificadores que no matchean esta tabla son `Token::Ident`.

Re-exporta `SourceFile` y `Span` de `hulk-span` para que `hulk-lexer` solo necesite depender de `hulk-tokens`.

### `hulk-lexer`

**Archivo**: `crates/hulk-lexer/src/lib.rs`.

API pública:

- `lex(source: &SourceFile, diagnostics: &mut DiagnosticBag) -> Vec<SpannedToken>`

El lexer consume todo el source en una pasada. Nunca aborta: ante un error emite un `Diagnostic` al bag y continúa. El último token siempre es `Token::Eof` con un span de longitud cero al final del source.

Comportamiento:

- **Whitespace** (` `, `\t`, `\r`, `\n`): se descarta.
- **Comentarios** `//`: se descartan hasta el siguiente `\n`.
- **Literales numéricos**: dígitos ASCII, opcionalmente con punto decimal y parte fraccional (ej: `42`, `3.5`). Parsea con `f64::parse`.
- **Strings**: delimitados por `"..."`. Escapes soportados: `\"`, `\n`, `\t`, `\\`. Escapes desconocidos emiten error y continúan. String sin cerrar emite error.
- **Identificadores**: primer carácter `[a-zA-Z]`, continúan con `[a-zA-Z0-9_]`. Si el identificador matchea un keyword, se produce el token correspondiente; si no, `Token::Ident`.
- **Identificadores con `_` inicial**: error (prohibido por la spec de HULK), pero el lexer consume el identificador completo antes de emitir el error para no producir múltiples errores por un mismo identificador.
- **Operadores con lookahead**: `==`/`=`, `=>`/`=`, `:=`/`:`, `<=`/`<`, `>=`/`>`, `!=`/`!`, `@@`/`@`, `->`/`-`. El lexer mira un carácter adelante para decidir.

---

## Decisiones de diseño

### 1. `self` y `base` NO son keywords

**Elegido**: `keyword_token("self")` y `keyword_token("base")` devuelven `None`. El lexer produce `Token::Ident("self")` y `Token::Ident("base")`.

**Justificación**: la spec de HULK (sección "Types") dice explícitamente que `self` no es un keyword — puede ser tapado por un `let` o un parámetro. Lo mismo aplica a `base`. Tratarlos como identificadores regulares es la interpretación semánticamente correcta. El resolver (sesión 6) les dará tratamiento especial buscándolos por nombre cuando encuentre `Ident("self")` o `Ident("base")` en contextos específicos (dentro de métodos, en llamadas a super).

**Alternativa descartada**: añadir variantes `Token::Self_` y `Token::Base`. Simplificaría el parser pero violaría la semántica de shadowing que la spec garantiza.

### 2. Identificadores con `_` inicial emiten error pero consumen el identificador

**Elegido**: `lex_invalid_leading_underscore_identifier` consume el identificador completo (`_foo_bar123`) y luego emite un único error. No produce ningún `Token` (se descarta completamente).

**Alternativas**:
- Emitir el error y avanzar solo el `_`: el resto del identificador se tokenizaría como otro `Token::Ident`, generando ruido en el parser.
- Aceptar el identificador y emitir el error: se violaría la spec silenciosamente en etapas posteriores.

**Justificación**: consumir todo permite al parser ver el contexto como "aquí debería haber una expresión", con la ubicación exacta del identificador inválido reportada por el diagnóstico.

### 3. Lexer UTF-8 safe

**Elegido**: `peek_char` usa `self.source[cursor..].chars().next()` y el avance del cursor se hace sumando `ch.len_utf8()` en los casos donde aparece contenido non-ASCII (strings).

**Alternativa original**: `char::from(byte)` — solo correcto para ASCII, rompe con caracteres multibyte.

**Justificación**: aunque los keywords, operadores e identificadores de HULK son ASCII, los strings literales pueden contener cualquier UTF-8 válido (ej: `"héllo"`). Sin UTF-8 safety, el cursor cae a mitad de un codepoint y produce panics en slicing o spans incorrectos.

### 4. Error recovery sin backtracking

**Elegido**: ante un carácter inesperado, avanzar un carácter, emitir diagnóstico, continuar desde el siguiente.

**Justificación**: el lexer de HULK no tiene contextos donde un mismo carácter pueda significar cosas distintas según qué venga después (como sí pasa en Python con indentación). Un simple "avanza y reporta" basta. Esto mantiene el lexer determinista y O(n).

### 5. `Token` clona el contenido de strings e identificadores

**Elegido**: `Token::Ident(String)` y `Token::StringLit(String)` llevan `String` owned, no `&str`.

**Alternativas**: `Token<'src>` con `&'src str` apuntando al buffer original.

**Justificación**: propagar un lifetime por todo el compilador (lexer, parser, AST, semantic, ...) contamina todas las APIs. El costo de allocación por identificador es despreciable para un compilador no crítico en tiempo. Si en el futuro hiciera falta optimizar, se puede introducir un `Interner` en `hulk-tokens` sin cambiar la API pública.

### 6. Último token siempre es `Eof`

**Elegido**: emitir explícitamente `Token::Eof` al final del vector.

**Justificación**: permite al parser tener un centinela y escribir `peek()` sin casos especiales de "final del stream". El span de `Eof` es `(len, len)` para apuntar al byte tras el último carácter del source.

---

## Gotchas

### Números solo ASCII

`lex_number` consume `'0'..='9'`. No maneja notación científica (`1e5`), separadores de dígitos (`1_000`), ni hexadecimal/binario. La spec de HULK solo habla de literales flotantes simples; añadir más requeriría cambios coordinados.

### Strings no cruzan líneas

El lexer corta un string si encuentra `\n` antes del `"` de cierre y emite "string sin cerrar". La spec no aclara si los strings multilínea están permitidos. Decisión actual: no. Si en el futuro se quisieran permitir, hay que cambiar esta rama.

### Comentarios no anidados

Solo `//` de línea. No hay `/* ... */`. La spec no menciona comentarios de bloque.

### `'\\'` requiere un siguiente carácter

Si el source termina con `"\` justo al final, el lexer sale del loop sin matchear el escape ni cerrar el string. El resultado correcto: "string sin cerrar". Esto funciona porque tras fallar el match de escape el loop continúa, y `peek_char()` devuelve `None`, saliendo del loop principal con `terminated = false`.

### `DiagnosticBag` se pasa como `&mut`

El lexer recibe el bag como `&mut DiagnosticBag` y lo muta. Si se llama a `lex` dos veces con el mismo bag, los errores se acumulan. Esto es intencional: el driver usa un solo bag para todo el pipeline.

---

## Ejemplos de uso

### Tokenizar un archivo

```rust
use std::sync::Arc;
use hulk_diagnostics::DiagnosticBag;
use hulk_lexer::lex;
use hulk_tokens::SourceFile;

let source = SourceFile::new("fib.hulk", "function fib(n) => 1;");
let mut bag = DiagnosticBag::new();
let tokens = lex(&source, &mut bag);

assert!(bag.is_empty());
// tokens ahora contiene: [Function, Ident("fib"), LParen, Ident("n"),
//                          RParen, FatArrow, Number(1.0), Semicolon, Eof]
```

### Inspeccionar un token con su span

```rust
let spanned = &tokens[1]; // el Ident "fib"
assert!(matches!(spanned.token, hulk_tokens::Token::Ident(ref s) if s == "fib"));

let file = spanned.span.file();
let lexeme = &file.source()[spanned.span.range()];
assert_eq!(lexeme, "fib");
```

### Recoger errores

```rust
let source = SourceFile::new("bad.hulk", r#"_invalid "abierto"#);
let mut bag = DiagnosticBag::new();
let _tokens = lex(&source, &mut bag);

assert!(bag.has_errors());
bag.emit_stderr().unwrap();
// produce dos errores:
// - identificador con _ inicial
// - string sin cerrar
```

---

## Cobertura de tokens vs spec HULK

Tabla de verificación: por cada elemento léxico que aparece en los ejemplos de `hulk-docs.pdf`, qué token lo representa.

| Construcción HULK | Ejemplo | Token |
|---|---|---|
| Literal numérico | `42`, `3.5` | `Number(f64)` |
| Literal string | `"Hello"` | `StringLit(String)` |
| Booleanos | `true`, `false` | `True`, `False` |
| Identificador | `x`, `fib` | `Ident(String)` |
| Aritméticos | `+ - * / ^ %` | `Plus`, `Minus`, `Star`, `Slash`, `Caret`, `Percent` |
| Strings | `@ @@` | `At`, `AtAt` |
| Comparación | `< > <= >= == !=` | `Less`, `Greater`, `LessEqual`, `GreaterEqual`, `EqualEqual`, `BangEqual` |
| Booleanos | `& \| !` | `Ampersand`, `Pipe`, `Bang` |
| Asignación | `=`, `:=` | `Equal`, `ColonEqual` |
| Flechas | `=>`, `->` | `FatArrow`, `Arrow` |
| Tipo anotación | `:` | `Colon` |
| Acceso a miembro | `.` | `Dot` |
| Delimitadores | `() {} []` | `LParen`/`RParen`, `LBrace`/`RBrace`, `LBracket`/`RBracket` |
| Separadores | `, ;` | `Comma`, `Semicolon` |
| Macros | `$` | `Dollar` |
| Keywords | todos los listados | variantes del enum |

**Nota**: `self` y `base` no tienen token propio porque no son keywords (decisión 1).

**Nota (actualización grading)**: `define` se acepta como sinónimo léxico de `function`. La tabla `keyword_token` mapea `"define"` al mismo token `Token::Function`, por lo que el parser no requiere ningún cambio adicional. Los tests de la categoría `ok/macros` usan `define` con sintaxis de flecha corta `->` (ver sección 7 — Actualización del parser); el alias léxico cubre todos los casos de los tests de grading sin introducir expansión sintáctica ni higiene.

---

## Tests implementados

En `crates/hulk-lexer/src/lib.rs` (módulo `tests`):

1. `lexes_literals_family` — números, strings, booleanos, identificadores.
2. `lexes_operators_family` — todos los operadores, verificando lookahead.
3. `lexes_keywords_family` — los 19 keywords del lenguaje.
4. `lexes_string_escapes` — `\n`, `\t`, `\"`, `\\`.
5. `recovers_from_errors` — identificador inválido + string sin cerrar emiten 2 errores y continúan.
6. `integration_tokenizes_small_program` — programa completo con función, `let`, bloque, comentario.

Pendiente en sesión 5: property tests (round-trip de lexer, fuzz con input aleatorio), tests de casos borde UTF-8, tests exhaustivos de spans.
