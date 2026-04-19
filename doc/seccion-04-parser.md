# Seccion 04 - Parser

## Que se implemento

- Se implemento el parser base en [crates/hulk-parser/src/lib.rs](crates/hulk-parser/src/lib.rs).
- Se agrego la estructura Parser con estado:
  - tokens
  - pos
  - bag
  - node_ids
- Se agregaron metodos de navegacion:
  - peek
  - advance
  - expect
  - at
  - skip_until
  - peek_span
- Se agrego la API publica parse(tokens, source) -> (Program, DiagnosticBag).
- Se implemento parse_expr_bp(min_bp) con algoritmo Pratt.
- Se implemento parse_nud para:
  - Number, StringLit, True, False
  - Ident (incluyendo mapeo especial a Self_ y Base)
  - agrupacion con parentesis
  - unarios Neg y Not
  - bloques con llaves
- Se implemento la tabla de precedencias infix_bp(token) para operadores binarios.
- Se construyen nodos BinOp y UnaryOp con spans combinados.
- Se agregaron tests unitarios en el propio crate para:
  - precedencia aritmetica
  - unarios + agrupacion
  - booleanos y concatenacion

## Decisiones de diseno

- Pratt parser para expresiones:
  - Se eligio Pratt porque simplifica el manejo de precedencias y asociatividad sin escribir un arbol grande de funciones por nivel.
  - La tabla infix_bp centraliza todas las precedencias en un solo punto.

- Ejemplo 1 + 2 * 3:
  - parse_expr_bp(0) consume 1 como nud.
  - Ve + con binding power suficiente y consume rhs con min_bp mayor.
  - En rhs, consume 2 y luego detecta * con mayor precedencia que +.
  - Resultado: 1 + (2 * 3), que coincide con la precedencia esperada.

- Tabla de precedencias usada (de menor a mayor):
  - Or (|)
  - And (&)
  - Igualdad (==, !=)
  - Comparacion (<, <=, >, >=)
  - Concatenacion (@, @@)
  - Aditivos (+, -)
  - Multiplicativos (*, /, %)
  - Potencia (^), derecha-asociativa

- parse devuelve Program aunque 4.1 solo parsea el body:
  - En esta etapa, Program se rellena con listas vacias de declaraciones y body parseado.
  - Esto permite integrar luego 4.2 sin romper la API publica.

## Gotchas

- Si faltan delimitadores en bloques, el parser reporta diagnostico y aplica skip_until para recuperacion local.
- El lexer no emite tokens reservados para self/base, por eso parse_nud mapea Ident("self") y Ident("base") a ExprKind especificos.
- Span::dummy requiere SourceFile, asi que los nodos sinteticos se construyen con el source recibido por parse.

## Ejemplos de uso

Uso minimo desde otro crate:

- Crear SourceFile.
- Tokenizar con hulk_lexer::lex.
- Parsear con hulk_parser::parse.
- Revisar DiagnosticBag.

Flujo real aplicado en tests de [crates/hulk-parser/src/lib.rs](crates/hulk-parser/src/lib.rs):

- source "1 + 2 * 3;"
- lex(source) -> tokens
- parse(tokens, source) -> Program
- Program.body = BinOp(Add, Number(1), BinOp(Mul, Number(2), Number(3)))
