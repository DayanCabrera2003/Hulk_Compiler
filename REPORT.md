# HULK Compiler — Reporte del proyecto

## Tabla de contenidos

1. [Introducción](#1-introducción)
2. [Arquitectura general](#2-arquitectura-general)
3. [Análisis léxico](#3-análisis-léxico)
4. [Análisis sintáctico](#4-análisis-sintáctico)
5. [Árbol de sintaxis abstracta](#5-árbol-de-sintaxis-abstracta-ast)
6. [Resolución de nombres y análisis semántico](#6-resolución-de-nombres-y-análisis-semántico)
7. [Inferencia de tipos](#7-inferencia-de-tipos)
8. [HIR — representación intermedia tipada](#8-hir--representación-intermedia-tipada)
9. [Expansión de macros](#9-expansión-de-macros)
10. [Desugaring](#10-desugaring)
11. [BANNER — IR de tres direcciones](#11-banner--ir-de-tres-direcciones)
12. [Generación de código LLVM](#12-generación-de-código-llvm)
13. [Runtime en C](#13-runtime-en-c)
14. [**Garbage collector — extra principal**](#14-garbage-collector--extra-principal)
15. [Prelude y biblioteca estándar](#15-prelude-y-biblioteca-estándar)
16. [Sistema de diagnósticos](#16-sistema-de-diagnósticos)
17. [Interfaces de línea de comandos](#17-interfaces-de-línea-de-comandos)
18. [Sistema de pruebas](#18-sistema-de-pruebas)
19. [Construcción y dependencias](#19-construcción-y-dependencias)
20. [Limitaciones conocidas](#20-limitaciones-conocidas)
21. [Conclusión](#21-conclusión)

---

## 1. Introducción

Este repositorio contiene una implementación completa del compilador del lenguaje
**HULK** (Havana University Language for Kompilers) descrito en `hulk-docs.pdf`
(Apéndice A). El compilador transforma un programa HULK arbitrario en un binario
nativo ejecutable para Linux x86_64, pasando por todas las fases canónicas de un
compilador moderno: análisis léxico, sintáctico, resolución de nombres,
inferencia de tipos, expansión de macros, transformaciones de azúcar sintáctico
(desugaring), una representación intermedia de tres direcciones llamada
**BANNER**, generación de IR de LLVM mediante `inkwell`, y enlazado contra una
biblioteca runtime de C que provee un **recolector de basura mark-and-sweep
preciso** — el extra principal del proyecto y la pieza que se describe con más
detalle en la sección 14.

El proyecto está organizado como un **workspace de Cargo** con 15 crates
internos, cada uno con una responsabilidad delimitada y comprobada por un test
de arquitectura que prohíbe dependencias que violen la regla de capas. Esta
separación se diseñó para que cada subsesión del trabajo pudiera tocar
exclusivamente la fase relevante sin arrastrar regresiones a fases ajenas, para
que las pruebas unitarias de cada crate sean rápidas y enfocadas, y para que
crates como `hulk-cli` puedan depender solamente de la interfaz pública del
driver sin necesidad de exponer el árbol completo de tipos internos.

El compilador expone dos binarios. `hulkc` es la herramienta de desarrollo,
con subcomandos para emitir cualquier representación intermedia (tokens, AST,
HIR, BANNER, LLVM IR, objeto o ejecutable). `hulk` es una interfaz minimalista
que acepta un único archivo `.hulk`, produce un ejecutable `./output` en el
directorio corriente y reporta errores a `stderr` en el formato
`(line,col) TYPE: message` con un código de salida que identifica la fase del
error (1 léxico, 2 sintáctico, 3 semántico). Las pruebas del proyecto están
organizadas en tres niveles —unit tests dentro de cada crate, tests de
integración cruzando módulos, y programas HULK completos compilados y
ejecutados— complementadas con pruebas basadas en propiedades (`proptest`)
para invariantes de los pases sensibles a casos límite (parser, desugaring).

---

## 2. Arquitectura general

### 2.1 Pipeline de compilación

```
            ┌─────────┐
   .hulk → │ Prelude │ → concat
            └─────────┘
                 │
                 ▼
            ┌─────────┐    ┌──────────┐    ┌──────────────┐
            │  Lex    │ → │  Parse   │ → │   Resolve    │
            │ (Token) │   │  (AST)   │   │  (Symbols)   │
            └─────────┘    └──────────┘    └──────────────┘
                                                  │
                                                  ▼
            ┌─────────┐    ┌──────────┐    ┌──────────────┐
            │ Desugar │ ← │  Macros  │ ← │ Type-infer   │
            │  (HIR)  │   │  (HIR)   │   │  (TypedAst)  │
            └─────────┘    └──────────┘    └──────────────┘
                  │
                  ▼
            ┌─────────┐    ┌─────────────┐    ┌──────────┐
            │ BANNER  │ → │ LLVM (codgen)│ → │  Linker  │
            │  (IR)   │   │   (inkwell)  │   │   (cc)   │
            └─────────┘    └─────────────┘    └──────────┘
                                                  │
                                                  ▼
                                              ./output
                                            (binario ELF)
```

### 2.2 Crates del workspace

| Crate | Responsabilidad | LOC aprox. |
|-------|----------------|-----------|
| `hulk-span` | Posiciones y SourceFile compartidos | ~150 |
| `hulk-tokens` | Definición del enum `Token` y SpannedToken | ~250 |
| `hulk-ast` | AST tipado, `Expr`/`ExprKind`/`Program` | ~900 |
| `hulk-diagnostics` | `Diagnostic`, `DiagnosticKind`, `DiagnosticBag` | ~250 |
| `hulk-lexer` | Análisis léxico tolerante a errores | ~600 |
| `hulk-parser` | Parser descendente recursivo con Pratt | ~2 500 |
| `hulk-semantic` | Resolución de nombres, scopes, validación | ~1 600 |
| `hulk-types` | Inferencia bottom-up, LCA, subtipado | ~1 100 |
| `hulk-hir` | Estructura de unificación AST + Resolver + TypeEnv | ~300 |
| `hulk-macros` | Expansión de macros (después de type-infer) | ~600 |
| `hulk-desugar` | Lambdas, for, vector generators, string concat | ~1 200 |
| `hulk-banner` | IR de tres direcciones; lowerer desde HIR | ~2 800 |
| `hulk-codegen` | LLVM IR mediante `inkwell`; integración con GC y linker | ~3 500 |
| `hulk-driver` | Orquestación del pipeline, prelude, opciones | ~400 |
| `hulk-cli` | Binarios `hulkc` (subcomandos de desarrollo) y `hulk` (CLI simple) | ~250 |

### 2.3 Regla de capas

Las dependencias permitidas (sólo "hacia adentro") son:

```
cli → driver → {lexer, parser, semantic, types, hir, macros,
                desugar, banner, codegen, diagnostics}
lexer    → {tokens, diagnostics, span}
parser   → {ast, tokens, diagnostics, span}
semantic → {ast, diagnostics, span}
types    → {ast, semantic, diagnostics}
hir      → {ast, semantic, types}
macros   → {hir, ast, diagnostics}
desugar  → {hir, ast, diagnostics}
banner   → {hir, ast, types}
codegen  → {banner, diagnostics}
{tokens, ast, diagnostics} → span
```

Esto se verifica con el test `crates/hulk-driver/tests/architecture.rs`, que lee
los `Cargo.toml` de cada crate y panic-ea si encuentra una dependencia prohibida.
Como consecuencia de esta regla, el crate `hulk-cli` accede a tipos como
`DiagnosticKind` y a la conversión de posiciones a `(line, col)` exclusivamente
a través de re-exports y métodos públicos de `hulk-driver` y `hulk-diagnostics`,
sin importar `hulk-span` directamente.

### 2.4 Inmutabilidad por defecto

El AST, el HIR y el programa BANNER son estructuras **inmutables** una vez
construidas. Las transformaciones (resolver, type-inferer, expansor de macros,
desugarer, lowerer a BANNER) producen estructuras nuevas en lugar de mutar las
existentes. Las únicas estructuras explícitamente mutables son los almacenes que
crecen durante el análisis: `SymbolTable`, `TypeEnv` y `DiagnosticBag`.

---

## 3. Análisis léxico

**Crate**: `hulk-lexer` — **archivo principal**: `crates/hulk-lexer/src/lib.rs`

### 3.1 Estrategia

El lexer es una máquina manual basada en cursor (no usa generadores tipo
`logos`), implementada como una `impl Lexer` que mantiene un `&str` al fuente,
un cursor byte-offset, una referencia mutable al `DiagnosticBag`, y un `Vec` de
tokens generados. El bucle principal (`lex_all`, líneas 46-120 de `lib.rs`)
consume caracteres uno a uno y dispatch-ea por la primera letra/símbolo a
sub-rutinas en `tokens/numbers.rs`, `tokens/strings.rs`, `tokens/idents.rs`,
`tokens/operators.rs`.

### 3.2 Inventario completo de tokens

**Palabras reservadas** (`crates/hulk-tokens/src/lib.rs`):
`function`, `let`, `in`, `if`, `elif`, `else`, `while`, `for`, `type`,
`inherits`, `new`, `protocol`, `extends`, `def`, `match`, `case`, `default`,
`is`, `as`.

**Literales**: `Number(f64)`, `StringLit(String)`, `true`, `false`,
`Ident(String)`.

**Operadores aritméticos**: `+`, `-`, `*`, `/`, `^` (potencia), `%`.

**Operadores lógicos**: `&` (and), `|` (or), `!` (negación).

**Comparadores**: `==`, `!=`, `<`, `<=`, `>`, `>=`.

**Operadores de concatenación de cadenas**: `@` (concat directa) y `@@`
(concatenación con un espacio intermedio insertado en tiempo de desugaring).

**Operadores de asignación**: `=` (binding en `let`/atributo),
`:=` (asignación destructiva sobre referencias).

**Separadores y delimitadores**: `(`, `)`, `{`, `}`, `[`, `]`, `,`, `.`, `;`,
`:`, `=>` (FatArrow para cuerpo de función inline y `match`/`case`),
`->` (Arrow para tipos de funciones).

**Macro placeholders**: `$` (prefijo de placeholder de macro). Outside de un
contexto de declaración de macro válido, `$` se reporta como error léxico (ver
3.4).

### 3.3 Reconocimiento de operadores compuestos

Operadores de dos caracteres se reconocen con una función auxiliar
`double_or_single(next_char, two_char_token, one_char_token)` (en
`tokens/operators.rs`). Si el carácter siguiente al actual es `next_char`, se
emite el token de dos caracteres; si no, se retrocede y se emite el de uno.
Esto cubre `->`, `@@`, `:=`, `<=`, `>=`, `!=`. Los casos `==` y `=>` se
discriminan a mano porque `=` puede ir seguido de `=` o `>` (líneas 91-102 de
`lib.rs`).

### 3.4 Tratamiento de `$`

`$` está reservado en HULK como prefijo de placeholders de macro (`$x: Number`).
El lexer hace lookahead un carácter: si después de `$` viene una letra ASCII o
`_`, emite el token `Dollar` para que el parser de declaraciones de macro lo
consuma; en cualquier otro contexto el carácter es inválido y se reporta como
`LEXICAL: caracter inesperado '$'`, avanzando un byte sin emitir token para
que el resto del programa pueda seguir lexando. Implementación en
`lib.rs:85-99`:

```rust
'$' => {
    if self.peek_next_char()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
    {
        self.single_char(Token::Dollar);
    } else {
        self.advance_char();
        self.report_error(start, self.cursor, "caracter inesperado '$'");
    }
}
```

### 3.5 Strings y escapes

`tokens/strings.rs` maneja literales `"..."` con escapes `\"`, `\n`, `\t`, `\\`.
Strings sin cerrar (lo que llega al final de archivo sin la `"` correspondiente)
emiten `LEXICAL: string sin cerrar`. El cursor avanza por codepoints, no por
bytes, así que strings con UTF-8 multibyte (`"hola, ñañá"`) lexan correctamente.

### 3.6 Comentarios

Sólo se soportan comentarios de una línea con `//`. La función
`consume_comment` (en `cursor.rs:39-46`) avanza por codepoint completo hasta
llegar a `\n` o EOF. Esto fue una regresión arreglada antes de la entrega: una
versión previa avanzaba byte por byte y paniqueaba si el comentario contenía
`—`, `á` o un emoji.

### 3.7 Recuperación de errores

El lexer **nunca aborta**. Ante un carácter inesperado, llama a `report_error`
(que añade un `Diagnostic` al bag) y continúa con el siguiente carácter. Esto
garantiza que el usuario pueda ver todos los errores léxicos en una sola
pasada. El driver luego retaguea estos diagnósticos como
`DiagnosticKind::Lexical` antes de fusionarlos al bag global.

### 3.8 Tests del lexer

15 tests en total (10 unitarios en `lib.rs`, 5 de integración). Cubren:
- Reconocimiento de todas las familias de tokens
- Recuperación tras errores múltiples
- Escapes de string
- Tolerancia a UTF-8 en comentarios y caracteres inesperados
- Programas pequeños integración-style

---

## 4. Análisis sintáctico

**Crate**: `hulk-parser` — **archivos principales**: `src/expr.rs`, `src/lib.rs`,
`src/decl/`, `src/complex.rs`.

### 4.1 Estrategia: Pratt + descenso recursivo

El parser es **descendente recursivo escrito a mano**, con **Pratt parsing**
para expresiones. Cada operador binario tiene un par `(left_bp, right_bp)` de
binding powers que codifica precedencia y asociatividad. Para expresiones
prefijas se usa el patrón `nud`/`led` (null-denotation / left-denotation), y la
función principal es `parse_expr_bp(min_bp)` en `expr.rs`.

Se prefirió la implementación manual sobre un generador como `LALRPOP` por dos
razones: (a) HULK tiene varias construcciones con ambigüedades sutiles (lambdas
`(x) => expr`, expresiones bloque `{ ... }`, declaraciones de macro con
parámetros marcados por prefijo) que se manejan mejor con código explícito;
(b) la recuperación de errores se controla mejor cuando el parser puede decidir,
en cada punto de fallo, qué sincronizador buscar.

### 4.2 Tabla de precedencias

De más floja a más apretada (de `expr.rs:357-380`):

| Operador | Binding power (l, r) | Asociatividad |
|----------|--------------------:|:--------------|
| `:=`     | (2, 1)              | Derecha       |
| `\|`/`or` | (3, 4)              | Izquierda     |
| `&`/`and`| (5, 6)              | Izquierda     |
| `==`, `!=`| (7, 8)             | Izquierda     |
| `<`, `<=`, `>`, `>=` | (9, 10) | Izquierda     |
| `@`, `@@`| (11, 12)            | Izquierda     |
| `+`, `-` | (13, 14)            | Izquierda     |
| `*`, `/`, `%`| (15, 16)        | Izquierda     |
| `^` (potencia) | (18, 17)      | **Derecha**   |
| unarios `-x`, `!x` | bp=19    | Prefijo       |
| llamadas, accesos, `is`/`as` | postfijos | — |

Que `^` sea derecha-asociativa es importante: `2 ^ 3 ^ 2` se parsea como
`2 ^ (3 ^ 2) = 2 ^ 9 = 512`, no como `(2 ^ 3) ^ 2 = 64`. Esto se logra
poniendo `r_bp < l_bp` (18 > 17) — un truco clásico de Pratt.

### 4.3 Construcciones soportadas

**Let con múltiples bindings y tipo opcional**:
```
let x = 1, y: Number = 2 in body
```
Implementado en `complex.rs:23-51`. Las bindings se acumulan en
`Vec<Expr>` (cada binding es un `ExprKind::LetBinding`), y el `body` es la
expresión que sigue a `in`.

**If/elif/else como expresión**:
```
if (cond) then_expr elif (cond) elif_expr else else_expr
```
Múltiples `elif` soportados. Los paréntesis alrededor de la condición son
obligatorios. Implementado en `complex.rs:83-119`.

**While**: `while (cond) body` — `complex.rs:122-134`.

**For**: `for (binding in iterable) body` — `complex.rs:137-158`. Se desugar
después a un `while` con un iterador (sección 10).

**Bloques**: `{ expr; expr; ...; expr }`. Los `;` separan expresiones; el bloque
evalúa a la última. Permitido como cuerpo de método, función o body de `let`.

**Declaraciones de función**: dos formas:
- Inline: `function name(p1: T1, p2: T2): R => expr;` (requiere `;` final)
- Bloque: `function name(p1: T1, p2: T2): R { ... }` (sin `;`)
El tipo de retorno es opcional. Parser en `decl/function.rs`.

**Declaraciones de tipo**:
```
type Counter(start: Number) inherits Animal(start) {
    val: Number = start;
    increment(): Number => self.val := self.val + 1;
}
```
Los parámetros de constructor son opcionales (para tipos sin estado); el
`inherits Parent(args)` también. Miembros pueden ser atributos
(`name [:Type] = expr;`) o métodos (mismas dos formas de función). Parser en
`decl/type_decl.rs`.

**Protocolos** (`decl/protocol.rs`):
```
protocol Iterable extends OtherProto {
    next(): Boolean;
    current(): Object;
}
```
Sólo firmas, sin cuerpos. El tipo de retorno es obligatorio en protocolos.

**Lambdas** (`complex.rs:184-229`):
```
(x: Number, y: Number): Number => x + y
```
Reconocidas con lookahead: tras `(`, si vemos `)=>` o `Ident,` o `Ident:`,
asumimos lambda; si no, expresión entre paréntesis.

**Literales y generadores de vectores** (`complex.rs:268-336`):
- Literal: `[1, 2, 3]` o `[]`
- Generador: `[x * 2 | x in xs]`

**`is` y `as`** (`expr.rs:257-274`): operadores postfijos
- `obj is Type` → Boolean (chequeo de subtipo en runtime)
- `obj as Type` → downcast verificado (abort si falla)

**Macro** (`decl/macro_decl.rs`):
```
def name(p1: T, $placeholder: T, @symbolic: T, *body: T) => expr
```
Cuatro tipos de parámetro distinguidos por prefijo (sección 9). Todos requieren
anotación de tipo explícita.

### 4.4 `match`/`case` lowereado a intrinsics

A diferencia de las otras construcciones, `match` no tiene un `ExprKind`
propio. El parser lo lowerea durante la construcción del AST a llamadas a
funciones intrínsecas: `__hulk_match(subject, __hulk_case_lit(...), ...,
__hulk_default(...))`. Los patrones soportados son:

- **Literal**: `case 42 => ...`, `case "hi" => ...`, `case true => ...`
- **Variable tipada**: `case x: Number => ...`
- **Binop**: `case (l: Number + r: Number) => ...`

Estas llamadas se traducen luego en el expansor de macros, que las reconoce
con `match_pattern()` (en `hulk-macros/src/lib.rs`) y emite el código de
despacho real.

### 4.5 Recuperación de errores y tokens de sincronización

Los errores sintácticos no abortan el parsing. Cuando se encuentra un token
inesperado, el parser:

1. Añade un `Diagnostic` al bag (con kind `Syntactic` después de retagueo en
   el driver).
2. Llama a `skip_to_sync()` para avanzar hasta uno de los **tokens de
   sincronización**: `Semicolon`, `RBrace`, `Eof`, `Function`, `Type`,
   `Protocol`, `Def`.
3. Llama a `ensure_progress()`: si tras `skip_to_sync` no se ha consumido
   ningún token (estamos al inicio de un sincronizador), avanza uno
   forzosamente para evitar bucles infinitos.

Esta estrategia permite reportar varios errores sintácticos en una sola
pasada, sin que el primero oculte los siguientes.

### 4.6 Tests del parser

272 tests en total (3 unit, 269 integration). Cubren:
- Precedencia y asociatividad de cada operador (con casos límite como
  `2 ^ 3 ^ 2`)
- Todas las construcciones gramaticales con happy paths
- Recuperación de errores (`error_recovery.rs`)
- Errores sintácticos específicos (`errors/syntactic.rs`)
- Programas combinados de `hulk-docs.pdf` (`declarations/hulk_md.rs`)

---

## 5. Árbol de sintaxis abstracta (AST)

**Crate**: `hulk-ast`

### 5.1 Top-level

`Program` (en `decl.rs:16-24`) agrupa todas las declaraciones globales:

```rust
pub struct Program {
    pub functions:  Vec<FunctionDecl>,
    pub types:      Vec<TypeDecl>,
    pub protocols:  Vec<ProtocolDecl>,
    pub macros:     Vec<MacroDecl>,
    pub body:       Expr,
}
```

`body` es la expresión final (lo que en otros lenguajes sería `main`).

### 5.2 Expr y NodeId

Cada nodo expresión es un `Expr { kind: ExprKind, span: Span, id: NodeId }`:

- `NodeId(u32)`: identificador único monotónico asignado durante el parsing.
  Es la "llave" que usan fases posteriores para colgar información sobre el
  nodo sin mutarlo (tipos inferidos, símbolos resueltos, etc.).
- `Span`: rango byte-offset en el `SourceFile`. Usado para diagnósticos y para
  computar `(line, col)`.
- `ExprKind`: el variante de la expresión.

### 5.3 Variantes de ExprKind

De `expr.rs:36-135`, en orden:

```
Number(f64), StringLit(String), Bool(bool),
Ident(String), Self_, Base,

BinOp { op, left, right },
UnaryOp { op, expr },

Call { callee, args },
MethodCall { receiver, method, args },
FieldAccess { receiver, field },
Index { target, index },

Block(Vec<Expr>),
VecLiteral(Vec<Expr>),
VecGenerator { element, binding, iterable },

Let { bindings, body },
LetBinding(LetBinding),

Assign { target, value },
AssignTarget(AssignTarget),

If { condition, then_branch, elif_branches, else_branch },
While { condition, body },
For { binding, iterable, body },

New { type_ann, args },
Is { expr, type_ann },
As { expr, type_ann },

Lambda { params, return_type, body },
```

### 5.4 Operadores

```rust
enum BinOpKind {
    Add, Sub, Mul, Div, Mod, Pow,
    Concat, ConcatSpaced,
    Lt, Le, Gt, Ge, Eq, Ne,
    And, Or,
}
enum UnaryOpKind { Neg, Not }
```

### 5.5 Anotaciones de tipo

```rust
enum TypeAnn {
    Named(String),                          // Number, String, MyType
    Iterable(Box<TypeAnn>),                 // T*
    Vector(Box<TypeAnn>),                   // T[]
    Functor { params: Vec<TypeAnn>,
              ret:    Box<TypeAnn> },       // (A, B) -> R
}
```

### 5.6 Visitors

`hulk-ast/src/visitor/` provee dos traits genéricos para recorrer el AST:
`Visit` (recorrido inmutable) y `VisitMut` (con mutación). Usados por
`hulk-semantic`, `hulk-types`, `hulk-macros`, `hulk-desugar`.

---

## 6. Resolución de nombres y análisis semántico

**Crate**: `hulk-semantic`

### 6.1 El Resolver

`Resolver` (en `resolver/mod.rs`) implementa la fase de resolución de nombres.
Sus responsabilidades:

1. **Construir la tabla de símbolos** (`SymbolTable`) con IDs estables (`SymbolId`).
2. **Mantener una pila de scopes** (`Vec<Scope>`) que se empuja/desempila al
   entrar a una función, método, tipo, `let` o bloque.
3. **Anotar cada referencia** (`Ident`, `MethodCall`, `FieldAccess`) con el
   `SymbolId` correspondiente, almacenado en `expr_symbols:
   HashMap<NodeId, SymbolId>`.
4. **Validar reglas estructurales** del lenguaje (lista en 6.4).
5. **Detectar ciclos de herencia** entre tipos.

### 6.2 Kinds de símbolo

Definidos en `symbols.rs:11-32`:

```
Variable, Function, Type, Protocol, Macro,
Parameter, SelfValue, BuiltinFunction, BuiltinValue, BuiltinType
```

`SelfValue` es el "tipo" del símbolo `self` dentro de un método (se define al
entrar al método y se elimina al salir).

### 6.3 Mapas auxiliares del Resolver

Además de la tabla y los scopes, el Resolver mantiene varios `HashMap`s para
responder consultas que los pases posteriores necesitan:

- `type_parents: HashMap<SymbolId, Option<SymbolId>>` — padre de cada tipo (o
  None si es raíz). Permite responder "¿es A subtipo de B?".
- `type_methods: HashMap<SymbolId, HashMap<String, SymbolId>>` — métodos por
  tipo.
- `protocol_methods: HashMap<SymbolId, HashSet<String>>` — métodos declarados
  por cada protocolo.
- `protocol_extends: HashMap<SymbolId, Vec<SymbolId>>` — protocolos que extiende
  cada protocolo.
- `function_param_annotations: HashMap<SymbolId, Vec<Option<TypeAnn>>>` —
  anotaciones declaradas de los parámetros de cada función, constructor de
  tipo y método (ver 6.5 para el detalle de cómo se indexan).
- `function_param_symbols: HashMap<SymbolId, Vec<SymbolId>>` — IDs de los
  parámetros de cada función/método/constructor.

### 6.4 Validaciones que reporta como `Semantic`

- Redefinición de un nombre en el mismo scope: `redefinicion de X`.
- Identificador no declarado en uso: `identificador no declarado: X`.
- Método declarado fuera de un tipo: `metodo fuera de una declaracion de tipo`.
- Llamada a método que no existe: `método no existe: X`.
- Uso de `self` fuera de un método: `self usado fuera de un método`.
- Uso de `base` fuera de un método, o en un tipo sin padre:
  `base usado en un tipo sin padre`.
- Asignación a `self`: `no se puede asignar a self`.
- `self`/`base` como nombre de parámetro: `'X' es palabra reservada y no
  puede ser nombre de parámetro`.
- Ciclos de herencia: `ciclos en herencia`.
- Herencia de un tipo primitivo (Number/String/Boolean): prohibida.
- Función sin anotaciones cuyo cuerpo es un único parámetro con tipo no
  inferible: `tipo no inferible, añade anotación`.
- Anotación de retorno incompatible con el literal que retorna:
  `tipo inferido incompatible con anotación`.

### 6.5 Indexación de parámetros de constructor y método

Los parámetros de un constructor de tipo (`type Counter(start: Number) { ... }`)
y los parámetros de un método (`add(n: Number) => ...`) se almacenan en los
mismos mapas `function_param_symbols` y `function_param_annotations` que los
parámetros de las funciones libres, pero indexados por el `SymbolId` del tipo y
del método respectivamente.

Esto permite que la fase de inferencia de tipos, antes de recorrer el cuerpo
de un atributo o método, pre-registre los tipos declarados de los parámetros
visibles desde ese cuerpo. Sin esta pre-registración, una expresión como
`val = start` dentro del constructor de `Counter` no sabría que `start` es
`Number` (porque su scope ya está desmontado cuando llega la inferencia) y la
inferiría como `Object`, lo que rompería más adelante la elección del
`FieldKind` correcto al construir el `TypeDescriptor` de BANNER.

El accesor público `Resolver::method_symbol(type_id, name) -> Option<SymbolId>`
permite que el driver mapee `(type_name, method_name) → SymbolId` y solicite
la registración de tipos en el `TypeEnv` antes de inferir cada cuerpo.

### 6.6 Detección de ciclos de herencia

`resolver/inheritance.rs:33-73` ejecuta una búsqueda por camino: para cada
tipo, sigue `type_parents` hasta llegar a `None` o detectar que un tipo ya
visto reaparece en el camino. Si se detecta repetición, reporta `ciclos en
herencia`. Esto evita que el codegen entre en bucle infinito al construir el
descriptor de un tipo.

### 6.7 Tests del módulo semántico

15 tests unitarios cubren cada validación con happy paths y casos negativos
(inheritance cycles, redefiniciones, `self`/`base` mal usados, métodos fuera de
tipos, parámetros con nombres reservados, conformidad protocolo-tipo).

---

## 7. Inferencia de tipos

**Crate**: `hulk-types`

### 7.1 Estrategia

**No es Hindley-Milner.** Es una inferencia **bottom-up** que asigna un `TypeId`
a cada nodo expresión en una sola pasada, aprovechando que la especificación
del lenguaje exige anotaciones explícitas en parámetros de función, parámetros
de tipo y atributos cuando son referencias.

El walker principal `TypeInferer::infer_expr` (en `inferer.rs:99-216`) recorre
el AST recursivamente, computa el tipo de cada subexpresión, y lo registra en
`TypeEnv::expr_types` (un `HashMap<NodeId, TypeId>`).

### 7.2 TypeEnv

Container con tres tablas (`env.rs:8-15`):
- `types: Vec<TypeKind>` — todos los tipos del programa, indexados por `TypeId`.
- `symbol_types: HashMap<SymbolId, TypeId>` — tipo asignado a cada símbolo.
- `expr_types: HashMap<NodeId, TypeId>` — tipo inferido de cada expresión.

### 7.3 TypeId y TypeKind

`TypeId(u32)` es un índice opaco. IDs reservados:
- `OBJECT = TypeId(0)`
- `NUMBER = TypeId(1)`
- `STRING = TypeId(2)`
- `BOOLEAN = TypeId(3)`

`TypeKind` (`type_id.rs:24-47`):
```rust
enum TypeKind {
    Builtin(BuiltinType),                              // Object/Number/String/Boolean
    UserDefined { name: String, parent: Option<TypeId> },
    Protocol    { name: String },
    Iterable(TypeId),                                  // T*
    Vector(TypeId),                                    // T[]
    Functor     { params: Vec<TypeId>, ret: Box<TypeId> },
    Unknown,                                           // no usado en la práctica
}
```

### 7.4 Qué chequea (y qué no)

**Sí chequea**:
- Tipos de literales (Number, String, Boolean directos).
- Operadores aritméticos: ambos operandos Number, resultado Number.
- Comparadores: resultado Boolean.
- Operadores lógicos `&`, `|`: operandos Boolean.
- `infer_vec_literal`: computa el LCA de los tipos de los elementos.
- `infer_if`: el tipo de un `if` es el LCA de todas las ramas.
- **Aridad y tipos de argumentos en llamadas** (`check_call_arity_and_types`,
  `inferer.rs:299-348`): cuando el callee es un identificador resoluble, valida
  que el número de argumentos coincida con la declaración y que cada argumento
  conforme con la anotación declarada del parámetro. Reporta `tipo incompatible
  en argumento de 'X': esperaba Y, recibio Z` o `numero incorrecto de
  argumentos para 'X'`.

**No chequea** (devuelve `OBJECT`):
- `infer_self` e `infer_base`: tienen un TODO para resolver al tipo
  envolvente. Esto no rompe nada porque el codegen calcula los tipos de
  `self`/`base` con su propio `current_type`.
- `infer_method_call` y `infer_field_access`: devuelven `Object` porque el
  receiver puede ser de un tipo arbitrario.
- `infer_lambda`: devuelve `Object` (los lambdas se reportan como functors
  opacos).

Estas limitaciones son conscientes: pasar `Object` no genera falsos positivos
(no se rechaza código válido) y el codegen tiene su propia resolución basada
en BANNER. Una versión futura podría endurecer esto.

### 7.5 LCA y subtipado

`env.conforms(t1, t2)` (`env.rs:130-152`) devuelve `true` si:
- `t1 == t2`, o
- `t2 == OBJECT` (Object es el tope), o
- `t2` aparece en la cadena de padres de `t1`.

`env.lca(t1, t2)` (`env.rs:157-179`) computa el supertipo más específico
común: si `t1` conforme con `t2`, devuelve `t2`; si no, sube por los padres
de `t1` recursivamente. Fallback a `OBJECT` si no hay otro común.

### 7.6 Pre-registro de parámetros

Antes de inferir cada cuerpo, el driver llama a:
- `inferer.register_function_params_by_name(name)` para cada función global.
- `inferer.register_function_params_by_name(type_name)` para cada tipo (esto
  registra los params del **constructor** del tipo gracias al cambio en 6.5).
- `inferer.register_method_params(type_name, method_name)` para cada método.

Esto garantiza que cuando el cuerpo referencia un parámetro, el inferidor
encuentra su tipo en `symbol_types` sin tener que reconstruir el contexto
lexical.

### 7.7 Tests

15 tests unitarios en `types/src/tests.rs` cubriendo cada `infer_*` con happy
paths y combinaciones de tipos. Plus toda la suite del driver y codegen que
ejecuta el inferidor.

---

## 8. HIR — representación intermedia tipada

**Crate**: `hulk-hir`

### 8.1 Qué es

El HIR (High-level Intermediate Representation) **no es** una transformación
estructural — es una **estructura de unificación** que empaqueta tres
artefactos producidos independientemente:

```rust
pub struct Hir {
    pub program: Program,    // AST original (puede mutarse en macros/desugar)
    pub symbols: Resolver,   // tabla de símbolos y bindings
    pub types:   TypeEnv,    // tipos de expresiones y símbolos
}
```

`Hir::from_typed` (`lib.rs:34-40`) es un constructor trivial que mueve los tres
campos a una nueva struct. La razón de existir es ofrecer a los pases
posteriores (macros, desugar, banner) una **única referencia inmutable** que
les permita consultar tanto la estructura sintáctica como la información
semántica derivada.

### 8.2 API expuesta

- `expr_type(node: NodeId) -> Option<TypeId>` — tipo inferido del nodo.
- `symbol_type(symbol: SymbolId) -> Option<TypeId>` — tipo del símbolo.
- `resolved_symbol(node: NodeId) -> Option<SymbolId>` — a qué símbolo apunta
  un `Ident`/`MethodCall`/`FieldAccess`.

### 8.3 Pipeline en el driver

```rust
let mut symbols = Resolver::new();
symbols.resolve_program(&program);

let mut types = TypeEnv::new();
let mut inferer = TypeInferer::new(&mut types, &symbols, bag);
infer_all(&program, &mut inferer);

let hir = Hir::from_typed(TypedAst { program, symbols, types });
```

`TypedAst` es la estructura intermedia que existe sólo durante el ensamblaje;
una vez construido `Hir`, `TypedAst` desaparece.

---

## 9. Expansión de macros

**Crate**: `hulk-macros`

### 9.1 Declaración de macros

Sintaxis (parseada por `hulk-parser/src/decl/macro_decl.rs`):

```
def name(params) => body
def name(params) { block }
```

### 9.2 Cuatro tipos de parámetro

Distinguidos por prefijo:

| Prefijo | Tipo | Significado |
|---------|------|------------|
| (ninguno) | `Regular` | Expresión normal substituida tal cual |
| `@` | `Symbolic` | Identificador/símbolo; no se alpha-renombra |
| `$` | `Placeholder` | Variable fresca asignada por el resolver |
| `*` | `Body` | Bloque `{ ... }` (validado como `ExprKind::Block`) |

Todos los parámetros requieren anotación de tipo explícita.

### 9.3 Pipeline

La expansión corre **después de la inferencia de tipos** y **antes del
desugaring**. Concretamente, en `hulk-driver/src/compile.rs:149-155`:

```rust
let hir = build_hir(program, &mut bag)?;     // lex+parse+resolve+infer
let hir = expand_macros(hir, &mut bag);      // ← expansión aquí
let hir = desugar(hir, &mut bag);
```

### 9.4 Mecanismo de expansión

El expansor (`expander.rs`) hace tres pasos por cada llamada de macro:

1. **Sanitización de locals**: cualquier identificador local del cuerpo de la
   macro se renombra prefijando el nombre de la macro y un contador de
   expansión (`__macro_<name>_<n>_<local>`). Esto evita captura accidental.
2. **Substitución de parámetros**: cuatro modos según el tipo:
   - Regular: clona la expresión del argumento.
   - Symbolic: sustituye el nombre del identificador.
   - Placeholder: aloca un símbolo fresco vía
     `resolver.allocate_symbol(...)` y lo enlaza al nodo.
   - Body: valida que el argumento sea un `Block` y lo inserta.
3. **Expansión recursiva**: si el cuerpo expandido contiene más llamadas a
   macros, se vuelve a expandir.

Después de la expansión, `refresh_node_ids_with_resolver` recorre el nuevo
sub-árbol y asigna `NodeId`s frescos (a partir de `max_node_id_in_program +
1`), también enlazando cada nodo con el resolver vía `bind_expr_symbol`.

### 9.5 Patrón especial: `__hulk_match`

El expansor también reconoce las llamadas a `__hulk_match` (las que el parser
genera al lowerear `match`) y las traduce en código de despacho que evalúa
los casos en orden y ejecuta el primero que matchea. Esto se hace en
`match_pattern()` en `hulk-macros/src/lib.rs`.

### 9.6 Tests

20 tests cubriendo:
- Substitución correcta por tipo de parámetro
- Hygiene (no captura de locals)
- Patrón `match` con literales, variables tipadas y binops
- Recursión de macros

---

## 10. Desugaring

**Crate**: `hulk-desugar`

### 10.1 Transformaciones implementadas

Cuatro pases en `src/transforms/`:

| Transform | Construcción origen | Destino |
|-----------|---------------------|---------|
| `for_loop` | `for (x in xs) body` | `let it = xs in while (it.next()) { let x = it.current() in body }` |
| `lambda` | `(p) => expr` con captura de libres | Tipo sintético con campo `__invoke` y constructor que recibe las capturas |
| `vec_generator` | `[e \| x in xs]` | `let v = __vec_new() in { for (x in xs) __vec_push(v, e); v }` |
| `string_concat` | `a @@ b` | `a @ " " @ b` |

### 10.2 Estrategia for-loop

`for_loop.rs:26-35` distingue dos protocolos:
- **Iterable**: el objeto tiene `next()` y `current()` directamente. Se usa tal cual.
- **Enumerable**: el objeto tiene `iter()` pero no `next()`. Se invoca `xs.iter()` para obtener un Iterable.

La decisión se basa en consultar `hir.symbols.type_has_method(type_id, "next")`.

### 10.3 Lambda → tipo sintético

`lambda.rs` recorre el cuerpo del lambda y recolecta las **variables libres**
(identificadores que no son parámetros del lambda ni globales). Luego:

1. Genera un nombre único para un tipo sintético (`__Lambda_<id>`).
2. Lo declara con los parámetros del lambda como params del constructor, las
   variables libres como atributos, y un método `__invoke` cuyo cuerpo es el
   del lambda con cada referencia a una variable libre reescrita como
   `self.<name>`.
3. Reemplaza el lambda en el AST por `new __Lambda_<id>(captures...)`.

Esto convierte las clausuras en objetos planos que el codegen ya sabe manejar,
sin necesidad de un sistema de clausuras dedicado en el back-end.

### 10.4 Tests

26 tests entre unit, integration, equivalencia (verificando que el desugaring
preserva la semántica operacional) y propiedad (`proptest`).

---

## 11. BANNER — IR de tres direcciones

**Crate**: `hulk-banner`

### 11.1 Naturaleza

IR **lineal, tipado, no-SSA** estilo LLVM-light. Cada instrucción tiene un
destino opcional, un opcode y operandos simples. La elección de no-SSA
simplifica el lowerer (un mismo temporal se puede reutilizar) a costa de
sacrificar análisis SSA-based; dado que el codegen lleva el IR directo a LLVM
(que sí es SSA y hace mem2reg), esa pérdida no afecta el resultado final.

### 11.2 Tipos de instrucción

24 variantes en `ir.rs:26-98`:

**Aritmética/lógica**: `Copy`, `BinOp`, `UnOp`.

**Llamadas**:
- `Call { dst, callee, args }` — llamada a función global o builtin.
- `MethodCall { dst, receiver, method, args }` — dispatch dinámico vía vtable.
- `StaticCall { dst, callee, args }` — llamada estática (constructor, super-call).

**Memoria**:
- `New { dst, type_name, args }` — `new T(args)` — emite `hulk_alloc` + `__init__`.
- `GetField { dst, obj, field }`, `SetField { obj, field, value }`.
- `GetIndex { dst, target, index }`, `SetIndex { target, index, value }`.

**Control de flujo**: `Label`, `Jump`, `JumpIf`, `Return`.

**GC**: `ShadowPush(Value)`, `ShadowPop` — empuja/saca referencias del shadow
stack del recolector (sección 14).

### 11.3 TempKind

```rust
enum TempKind { F64, I1, Ptr, Void }
```

Cada temporal lleva su tipo explícito. Esto permite al codegen elegir la
representación LLVM correcta (f64 vs i1 vs ptr opaco) sin re-inferir.

### 11.4 TypeDescriptor

`ir.rs:137-144`:

```rust
struct TypeDescriptor {
    name:        String,
    parent:      Option<String>,
    fields:      Vec<String>,
    pointer_map: Vec<bool>,         // true = campo referencia (GC-traced)
    field_kinds: Vec<FieldKind>,    // F64 | Boolean | Reference
    methods:     Vec<BannerFunction>,
}
```

El `pointer_map` es la pieza clave que enlaza BANNER con el GC: indica al
codegen qué campos son referencias para construir el `TypeTag` con los offsets
correctos (sección 14.4).

### 11.5 Layout final del programa

```rust
struct BannerProgram {
    types:     Vec<TypeDescriptor>,
    functions: Vec<BannerFunction>,
    main:      BannerFunction,    // expresión top-level del programa
}
```

### 11.6 Lowerer

`hulk-banner/src/lowerer.rs` (~2200 líneas) convierte cada expresión HIR en una
secuencia de instrucciones BANNER:

- Aloca temporales lineares (un contador `next_temp`).
- Mantiene un `HashMap<SymbolId, TempId>` para variables locales.
- Para tipos de usuario, genera una función `__init__` que rellena los campos
  desde los args del constructor y delega al `__init__` del padre cuando hay
  herencia.
- Emite `ShadowPush`/`ShadowPop` para variables locales de tipo referencia
  (sección 14.5).

### 11.7 Tests

26 tests, incluyendo `tests/shadow_stack.rs` que verifica que números/booleanos
**no** disparan `ShadowPush`, pero strings y objetos sí.

---

## 12. Generación de código LLVM

**Crate**: `hulk-codegen`

### 12.1 Stack

Usa **`inkwell`** (bindings seguros de Rust sobre LLVM 17) — versión declarada
en `Cargo.toml` raíz: `inkwell = { version = "0.4.0", features = ["llvm17-0"] }`.
El código usa la sintaxis de **punteros opacos** (`ptr` en lugar de `i8*`), lo
cual es la convención moderna de LLVM ≥ 14.

### 12.2 Tabla de runtime functions

El codegen declara, antes de emitir cualquier código, las firmas de todas las
funciones del runtime C que va a invocar. Esto vive en
`hulk-codegen/src/rt.rs` y tiene una struct `RtFunctions` con campos para cada
una: `hulk_alloc`, `hulk_shadow_push`, `hulk_shadow_pop`, `hulk_print*`,
`hulk_string_new`, `hulk_string_concat`, `__hulk_concat`, `__hulk_is`,
`__hulk_as`, `__vec_*`, `__range_*`, `hulk_sqrt`/`sin`/`cos`/`exp`/`log`,
`hulk_rand`, etc.

### 12.3 Layout de tipos

Los tipos de usuario se layoutean como structs LLVM con un **puntero a vtable
en el offset 0**, seguido de los campos del usuario (declarados primero los
del padre, luego los propios). El cálculo del orden global vive en
`ProgramLayout::compute` (en `hulk-codegen/src/layout.rs:29-41`), que recorre
la cadena de herencia. El acceso a campos vía GEP usa índices con offset +1
para saltar la vtable (línea 56-57: `Some((pos as u32) + 1)`).

### 12.4 Dispatch de métodos

Tres modos:

1. **Vtable**: para `obj.method(args)`. Se carga el puntero a vtable del campo 0
   del receptor, se indexa por el slot del método (computado en
   `ProgramLayout::method_index`), se carga el puntero a función, y se emite
   `call` indirecta (`emit_call.rs:117-191`).
2. **Estático**: para constructores y super-llamadas (`base.method()`).
   Llamada directa a `TypeName.method` (`emit_call.rs:210-237`).
3. **Builtin del runtime C**: para tipos `$range`, `$vector`, `$string` y
   funciones globales como `print`, `sqrt`, etc. Llamada directa a la función
   C correspondiente (`emit_call.rs:75-111`).

### 12.5 Emisión de tipos GC-aware (TypeTag globals)

Para cada tipo de usuario, el codegen genera **tres globales LLVM**:

1. Un `[N x i64]` constante con los offsets en bytes de los campos referencia
   (computados a partir de `pointer_map` de BANNER).
2. Un `i8*` constante con el nombre del tipo (para impresión y debugging).
3. Un struct `TypeTag` constante con `{ name_ptr, num_pointers,
   offsets_array_ptr, parent_tag_ptr }`. El `parent_tag_ptr` se rellena con la
   dirección del `TypeTag` del padre (que ya existe porque el codegen ordena
   los tipos parent-first).

Estos globales son los que `hulk_alloc` consume vía argumento `TypeTag*`, y los
que `mark` lee para saber qué campos trazar (sección 14).

### 12.6 Linking final

`hulk-codegen/src/link.rs:99-106`:

```bash
<cc> <object.o> -o <output> [-L<lib_dir>] -lhulkruntime -lm
```

`cc` es el de sistema (`gcc` en Fedora/Ubuntu); el `libhulkruntime.a` lo
construye el `build.rs` del propio crate (sección 19). `-lm` aporta `sqrt`,
`sin`, `cos`, `exp`, `log` desde `libm`.

### 12.7 Tests

102 tests de integración en `hulk-codegen/tests/`, incluyendo `comprehensive.rs`
(programas exhaustivos), `integration.rs`, y otros específicos por feature.

---

## 13. Runtime en C

**Carpeta**: `runtime/` — librería `libhulkruntime.a`.

### 13.1 Archivos

- `gc.h`/`gc.c` — recolector mark-and-sweep + shadow stack (sección 14).
- `strings.h`/`strings.c` — tipo `HulkStr` inmutable.
- `builtins.h`/`builtins.c` — print, math, vectores, rangos, `__hulk_is`/`__hulk_as`.
- `test_gc.c` y `test_strings.c` — tests en C (compilados manualmente con `gcc`).

### 13.2 Strings

`HulkStr` usa un **flexible array member** (`char data[]`):

```c
typedef struct HulkStr {
    size_t len;
    char   data[];
} HulkStr;
```

Esto permite alojar el string completo (header + bytes) con un único
`hulk_alloc`. El `hulk_string_tag` declara `num_pointers = 0` porque el
payload es bytes crudos, no punteros al heap. Operaciones públicas:

- `hulk_string_new(const char*)` — copia un C string.
- `hulk_string_concat(void*, void*)` — concatenación inmutable.
- `hulk_number_to_string(double)` — formatea con `%g`.

Los strings son **inmutables**: cada operación produce un objeto nuevo. Esto
simplifica la concurrencia (no hay race conditions) y el GC (no hay mutación
de campos referencia).

### 13.3 Vectores

`HulkVec` mantiene `len`, `cap` y un `double* data` (heap C separado, no GC-traced).
Operaciones: `__vec_new`, `__vec_push` (con realloc cuando `len == cap`),
`__vec_get`, `__vec_set`, `__vec_size`, `__vec_next`, `__vec_current` (protocolo
Iterable). La elección de mantener el array de datos fuera del heap GC simplifica
el resize (`realloc` no requiere actualizar el header GC) a costa de que los
vectores no pueden contener referencias (sólo `double`). Aceptable para HULK
estándar.

### 13.4 Rangos

`HulkRange` tiene tres `double`s: `min`, `max`, `step`. Implementa el protocolo
Iterable directamente: `__range_next` incrementa `current` y devuelve true
mientras `current < max`; `__range_current` devuelve el valor actual. Layout
diseñado para coincidir bit-a-bit con la definición de `Range` en el prelude
(sección 15), de modo que `new Range(0, 10)` en HULK y `hulk_range_new(0, 10,
1)` en C producen objetos compatibles.

### 13.5 Print

Tres variantes para evitar boxing innecesario:
- `hulk_print(void*)` — para referencias; inspecciona `TypeTag` y elige
  formato (String → bytes, Number-objeto → %g, default → `<TypeName>`).
- `hulk_print_number(double)` — para Number unboxed (camino caliente).
- `hulk_print_bool(int)` — imprime literalmente `"true"`/`"false"`.

### 13.6 Math y rand

Wrappers de `<math.h>`: `hulk_sqrt`, `hulk_sin`, `hulk_cos`, `hulk_exp`,
`hulk_log` (la última con dos argumentos: base y x). `hulk_rand` devuelve un
`double` en `[0, 1)` usando `rand() / (double)RAND_MAX`. El seed no se
inicializa explícitamente; los tests no dependen del valor concreto.

### 13.7 `__hulk_is` y `__hulk_as`

`__hulk_is(obj, target_tag)` (`builtins.c:133-156`) recupera el header del
objeto (`HULK_HEADER(obj)`), toma el `TypeTag*` y camina hacia arriba por el
campo `parent` hasta encontrar `target_tag` (devuelve 1) o llegar a `NULL`
(devuelve 0). `__hulk_as` lo invoca y aborta con mensaje si falla.

### 13.8 Tests en C

`test_gc.c` y `test_strings.c` son binarios independientes compilados con
`gcc` directamente (no por `cargo`). Validan el GC y las strings respectivamente,
y se ejecutan localmente para sanity-checking del runtime.

---

## 14. Garbage collector — extra principal

Esta sección describe el extra principal del proyecto: un **recolector de
basura preciso de tipo mark-and-sweep** con shadow stack para los roots,
integrado de extremo a extremo entre el codegen y el runtime. Es lo que
permite que programas HULK con clausuras, herencia profunda y cadenas largas
de objetos liberen memoria correctamente sin filtraciones ni necesidad de
disciplina manual del usuario.

### 14.1 Motivación

HULK tiene asignaciones implícitas en cada `new T(...)`, cada string nuevo
(producido por `@` o `@@`), cada lambda (desugareado a `new __Lambda_N(...)`),
cada vector, cada range. Sin un GC, el programa filtraría memoria
indefinidamente. Las alternativas consideradas y descartadas fueron:

- **Reference counting**: requiere campos `refcount` mutables en cada objeto
  (rompe la regla de inmutabilidad), no maneja ciclos (los objetos con
  referencias mutuas nunca se liberan), e introduce trabajo en cada
  asignación.
- **GC conservativo (estilo Boehm)**: requiere escanear toda la stack del
  programa C como roots, incluyendo basura como punteros de función y
  variables de control. Más simple de integrar pero menos preciso (puede
  retener falsos positivos), y no descubre raíces "ocultas" si el optimizador
  las mueve a registros.
- **Compactación / mover objetos**: requiere actualizar punteros tras cada
  colección, lo cual obliga a saber dónde están todos los punteros. Muy
  complejo para un proyecto académico.

Elegimos **mark-and-sweep preciso con shadow stack** porque:
- Es preciso (no falsos positivos en marcado).
- Maneja ciclos correctamente (la marca es un bit, no un contador).
- La shadow stack la mantiene el compilador, así que es robusta ante
  optimizaciones de LLVM (los roots están explícitos, no inferidos de
  registros).
- La implementación cabe en ~100 líneas de C.

### 14.2 Layout del objeto en memoria

Cada objeto alojado por `hulk_alloc` tiene un **header de 32 bytes** seguido
por el payload del usuario:

```
+-----------------------+   ←  dirección que devuelve malloc()
| TypeTag*    tag       |   8 bytes — descriptor de tipo
| size_t      size      |   8 bytes — total bytes (header + payload)
| int         mark      |   4 bytes — bit de marcado (con padding)
| ObjHeader*  next      |   8 bytes — siguiente en la lista intrusiva
+-----------------------+   ←  HULK_PAYLOAD(hdr) — dirección que ve el usuario
| payload del usuario   |
| ...                   |
+-----------------------+
```

Los macros `HULK_PAYLOAD(hdr) = (void*)((ObjHeader*)hdr + 1)` y
`HULK_HEADER(pay) = (ObjHeader*)pay - 1` convierten entre ambas direcciones
con aritmética de punteros. El compilador siempre maneja la dirección del
payload (es lo que devuelve `hulk_alloc`), y la lista intrusiva la mantiene
el GC para poder visitarla en el sweep.

**Alternativa descartada**: una lista de asignaciones separada
(`Vec<*ObjHeader>`). Empeora la localidad de caché y requiere asignaciones
adicionales en cada `hulk_alloc`. La lista intrusiva añade 8 bytes por objeto
pero los punteros viajan junto con los datos.

### 14.3 TypeTag — descriptor por tipo

```c
typedef struct TypeTag {
    const char*       name;
    size_t            num_pointers;
    size_t*           pointer_offsets;
    struct TypeTag*   parent;
} TypeTag;
```

- `name`: nombre del tipo (para impresión y debugging).
- `num_pointers`: cuántos campos referencia tiene el payload.
- `pointer_offsets`: array de byte-offsets desde el inicio del payload, uno
  por cada campo referencia.
- `parent`: puntero al `TypeTag` del padre directo (NULL si es raíz). Usado
  por `__hulk_is` para responder "¿X conforma a Y?".

Esta es la pieza que enlaza el compilador con el GC: el compilador **genera**
estos descriptores como globales LLVM (sección 12.5) basándose en el
`pointer_map` de BANNER (sección 11.4), y el GC los **lee** durante el mark
para saber qué punteros trazar.

### 14.4 Construcción del pointer_offsets

El flujo es:

1. En BANNER (`hulk-banner/src/lowerer.rs:173-198`): para cada campo de un
   tipo, se determina su `FieldKind` (Number, Boolean o Reference) inspeccionando
   la anotación de tipo o el tipo inferido del valor inicial. Se construye
   un `pointer_map: Vec<bool>` con `true` en cada campo referencia.

2. En codegen (`hulk-codegen/src/emit_mem.rs:269-346`): se construye el array
   `[N x i64]` con los offsets. Cada campo referencia contribuye con su
   posición `i` multiplicada por 8 (tamaño de un puntero) más 8 (skip de la
   vtable que va en el campo 0):

```rust
let ptr_offsets: Vec<u64> = pointer_map
    .iter().enumerate()
    .filter_map(|(i, &is_ptr)| {
        if is_ptr { Some((i as u64 + 1) * 8) } else { None }
    })
    .collect();
```

3. Se emite el `TypeTag` global con `{ name, num_pointers = ptr_offsets.len(),
   offsets = &ptr_offsets[0], parent = &Parent_tag }`. Los tipos se ordenan
   parent-first para que la dirección del padre ya exista cuando se referencia.

**Resultado**: el GC traza solamente los campos que realmente contienen
punteros, ignorando f64 y i1. Esto es **trazado preciso** — un GC
conservativo trataría todo el payload como posibles punteros.

### 14.5 Shadow stack — registración de roots

Las **raíces** del grafo de objetos vivos son todas las variables locales y
temporales de tipo referencia que están en scope en el momento de una
colección. Como LLVM puede colocarlas en registros (donde el GC no las ve),
las registramos explícitamente en una **shadow stack**:

```c
#define HULK_SHADOW_STACK_CAPACITY 4096
void*  __hulk_shadow_stack[HULK_SHADOW_STACK_CAPACITY];
size_t __hulk_shadow_top;

void hulk_shadow_push(void* val);   // empuja un puntero
void hulk_shadow_pop(void);         // saca uno
```

La capacidad de 4096 ranuras cubre cualquier programa HULK realista (4096
variables de referencia simultáneamente activas en el call stack). Si se
supera, el runtime aborta con `shadow stack overflow` — falla controlada, no
corrupción.

**Por qué array fijo en lugar de growable**: simplicidad y predictabilidad.
Un array dinámico con `realloc` añadiría una posible asignación en el camino
caliente de `hulk_shadow_push`, lo cual podría disparar GC recursivamente y
romper invariantes. Una lista enlazada por frame tendría overhead de puntero
por entrada y mala localidad de caché.

### 14.6 Emisión de push/pop por el compilador

En BANNER (`hulk-banner/src/ir.rs:91-92`):

```rust
ShadowPush(Value),
ShadowPop,
```

El lowerer (`hulk-banner/src/lowerer.rs:774-792`) emite estos sólo para
bindings de tipo referencia:

```rust
fn emit_let_binding_expr(&mut self, lb: &LetBinding, expr: &Expr) -> Value {
    let val = self.emit_expr(&lb.value);
    let ty = self.hir.expr_type(lb.value.id).unwrap_or(TypeId::OBJECT);
    if Self::is_reference(ty) {
        self.emit(Instr::ShadowPush(Value::Temp(dst)));
        self.shadow_count += 1;
    }
    ...
}
```

Al salir del scope del `let`, se emite un `ShadowPop` por cada `ShadowPush`
acumulado. Esto se verifica con tests específicos en
`hulk-banner/tests/shadow_stack.rs` que confirman: variables Number/Boolean
**no** disparan push; variables String/objeto **sí**.

En codegen (`hulk-codegen/src/emit.rs:435-446`):

```rust
fn emit_shadow_push(&mut self, val: &Value) -> CodegenResult<()> {
    let ptr_val = self.coerce_to_ptr(v)?;
    self.builder.build_call(
        self.rt.hulk_shadow_push, &[ptr_val.into()], "shadow_push"
    )?;
    Ok(())
}
```

### 14.7 Allocator — `hulk_alloc`

`runtime/gc.c:59-84`. Cuando el compilador emite `new T(args)`:

1. El codegen calcula el tamaño total `sizeof(TStruct)`, recupera el
   `TypeTag*` global, y emite `call ptr @hulk_alloc(ptr @T_tag, i64 size)`.
2. `hulk_alloc` chequea si añadir esta asignación superaría el threshold
   actual. Si sí, dispara `hulk_gc()` antes de alocar.
3. Aloca con `malloc(sizeof(ObjHeader) + payload_size)`.
4. Rellena el header: `tag`, `size`, `mark = 0`, `next = __hulk_alloc_list`,
   y enlaza al principio de la lista intrusiva.
5. **Inicializa el payload a cero** con `memset(HULK_PAYLOAD(hdr), 0,
   payload_size)`. Esto es **crítico** porque garantiza que campos
   referencia no inicializados son `NULL`, evitando que el GC siga
   punteros basura en una colección temprana.
6. Devuelve el puntero al payload.

Si `malloc` devuelve NULL (memoria agotada del sistema) o si tras GC sigue
sin caber, el runtime aborta con mensaje claro.

### 14.8 Mark phase

`runtime/gc.c:18-28`:

```c
static void mark(void* payload) {
    if (payload == NULL) return;
    ObjHeader* hdr = HULK_HEADER(payload);
    if (hdr->mark) return;             // ← evita ciclos infinitos
    hdr->mark = 1;
    char* base = (char*)payload;
    for (size_t i = 0; i < hdr->tag->num_pointers; i++) {
        void* child = *(void**)(base + hdr->tag->pointer_offsets[i]);
        mark(child);                   // ← recursión
    }
}
```

Para cada raíz en la shadow stack, llama a `mark()`. La recursión sigue los
campos referencia indicados por el `TypeTag`. La guarda `if (hdr->mark)
return` maneja correctamente **grafos cíclicos**: cuando un objeto A apunta a
B que apunta de vuelta a A, el segundo `mark(A)` retorna inmediatamente sin
re-procesar.

El test `stress-test/gc/cycles.hulk` verifica esto: construye una estructura
con ciclos y comprueba que (a) la colección no entra en bucle infinito y
(b) los objetos del ciclo se liberan correctamente cuando ya no son alcanzables
desde ningún root.

### 14.9 Sweep phase

`runtime/gc.c:30-57`:

```c
ObjHeader** cursor = &__hulk_alloc_list;
while (*cursor != NULL) {
    ObjHeader* obj = *cursor;
    if (obj->mark) {
        obj->mark = 0;                   // limpia para el próximo ciclo
        cursor = &obj->next;
    } else {
        *cursor = obj->next;             // desenlaza de la lista
        __hulk_alloc_bytes -= obj->size; // actualiza contador
        free(obj);                       // libera al sistema
    }
}
```

El **truco del doble puntero** (`ObjHeader** cursor = &__hulk_alloc_list`) es
clásico de listas intrusivas: permite eliminar un nodo del medio sin tener
que llevar un puntero al anterior. El `*cursor = obj->next` reescribe lo que
apuntaba al objeto eliminado para que ahora apunte al siguiente.

### 14.10 Threshold adaptativo

Después del sweep:

```c
size_t new_threshold = __hulk_alloc_bytes * GC_GROWTH_FACTOR;  // ×2
if (new_threshold < GC_INITIAL_THRESHOLD) {                    // mínimo 1 MiB
    new_threshold = GC_INITIAL_THRESHOLD;
}
__hulk_gc_threshold = new_threshold;
```

- `GC_INITIAL_THRESHOLD = 1 MiB`: floor por debajo del cual no bajamos
  nunca. Evita colecciones degeneradas cuando el conjunto vivo es casi nulo
  (programas muy pequeños).
- `GC_GROWTH_FACTOR = 2`: multiplicador sobre el live-set actual. Si tras
  una colección quedan 4 MiB vivos, el threshold se vuelve 8 MiB; cuando se
  alcance, el live-set será comparable y la próxima colección amortiza el
  costo en proporción.

Esto garantiza que **programas con poco heap recolectan raramente** (siempre
caben en el threshold floor), y **programas con mucho heap recolectan
proporcionalmente** (la frecuencia decae linealmente con el tamaño del
working set). El costo amortizado por byte asignado tiende a una constante.

### 14.11 Integración build — `build.rs`

`crates/hulk-codegen/build.rs` compila el runtime cada vez que cambia. El
script:

1. Localiza `runtime/` (vía `CARGO_MANIFEST_DIR`).
2. Compila cada `.c` con `gcc -O2 -Wall -Werror -I runtime/` produciendo
   `.o` en `$OUT_DIR`.
3. Empaqueta los `.o` con `ar rcs $OUT_DIR/libhulkruntime.a`.
4. Emite las directivas de cargo:
   - `cargo:rustc-link-search=native=$OUT_DIR`
   - `cargo:rustc-link-lib=static=hulkruntime`
   - `cargo:rustc-link-lib=m`
5. Emite `cargo:rerun-if-changed=` por cada `.c` para que Cargo recompile
   cuando el runtime cambie.

Esto significa que un cambio en `runtime/gc.c` dispara automáticamente la
recompilación, sin necesidad de pasos manuales. Tanto el compilador (que
linkea el runtime) como el binario final del usuario heredan ese
`libhulkruntime.a`.

### 14.12 Tests del GC

**Tests en C** (`runtime/test_gc.c`):
- Asignación básica y free.
- Marcado correcto de un objeto con campo referencia.
- Sweep que elimina objetos no alcanzables.
- Shadow stack push/pop preservan el alive set.
- Grafos con ciclos (la marca evita recursión infinita).
- Threshold growth tras colección.

**Tests HULK end-to-end** (`stress-test/gc/`):
- `allocs_many.hulk`: alloca miles de objetos secuencialmente para verificar
  que el GC se dispara y libera correctamente.
- `cycles.hulk`: construye listas con referencias circulares; verifica que
  el GC las libera cuando se rompe el último root externo.
- `tree_walk.hulk`: construye y recorre un árbol de profundidad arbitraria,
  testeando que el marcado recursivo profundo no desborda la stack C.

Cada uno tiene un `.expected` con la salida esperada que confirma que el
programa terminó sin OOM ni segfault.

### 14.13 Trade-offs y limitaciones

**Lo que funciona muy bien**:
- Trazado preciso (cero falsos positivos).
- Ciclos manejados sin contabilidad adicional.
- Threshold amortizado.
- Integración 100% controlada por el compilador (sin guesswork).

**Limitaciones reconocidas**:
- **Stop-the-world** (no concurrente). En programas single-threaded HULK no
  importa; un programa multi-thread necesitaría sincronización.
- **Sin compactación**: la fragmentación puede acumularse con churn alto.
  En la práctica `malloc` del libc moderno mitiga esto razonablemente.
- **Recursión en `mark`** puede desbordar la stack C en árboles
  extremadamente profundos (>~50000 niveles). En programas reales este
  límite nunca se alcanza; mitigable con una pila explícita si fuera
  necesario.
- **Shadow stack fija**: 4096 entradas es generoso pero finito; un programa
  patológico podría agotarla. La verificación es runtime (no compile-time),
  así que el fallo es controlado.

---

## 15. Prelude y biblioteca estándar

**Archivo**: `prelude/prelude.hulk`

Se incluye con `include_str!("../../../prelude/prelude.hulk")` en
`hulk-driver/src/compile.rs:19` y se prepende al fuente del usuario:

```rust
let combined = format!("{PRELUDE}\n{source_text}");
```

Contenido completo (22 líneas):

```hulk
protocol Iterable {
    next(): Boolean;
    current(): Object;
}

protocol Enumerable {
    iter(): Iterable;
}

type Range(min: Number, max: Number) {
    min: Number = min;
    max: Number = max;
    current: Number = min - 1;

    next(): Boolean => (self.current := self.current + 1) < self.max;
    current(): Number => self.current;
}
```

Esto garantiza que `Range`, `Iterable` y `Enumerable` están en scope para
cualquier programa de usuario sin imports.

**Implicación para diagnósticos**: las posiciones reportadas por el compilador
están relativas al fuente combinado, así que un error en la línea 1 del
archivo del usuario aparece reportado en la línea 23 del combinado (22 del
prelude + 1 del separador `\n`). El binario `hulk` resta este offset usando
`hulk_driver::prelude_line_offset()` (calcula en runtime contando `\n`s en el
PRELUDE) para que el usuario vea las posiciones correctas en su archivo.

---

## 16. Sistema de diagnósticos

**Crate**: `hulk-diagnostics`

### 16.1 Tipo Diagnostic

```rust
pub struct Diagnostic {
    pub severity: Severity,         // Error | Warning | Note
    pub kind:     DiagnosticKind,   // Lexical | Syntactic | Semantic
    pub message:  String,
    pub labels:   Vec<Label>,       // spans etiquetados
    pub notes:    Vec<String>,
}
```

### 16.2 DiagnosticKind

```rust
pub enum DiagnosticKind { Lexical, Syntactic, Semantic }
```

Con métodos:
- `tag()` → `"LEXICAL"` / `"SYNTACTIC"` / `"SEMANTIC"` (etiqueta textual del
  kind, útil para el formato de salida del CLI).
- `exit_code()` → 1 / 2 / 3 (código de salida convencional por fase).
- `priority()` → 0 / 1 / 2 (orden de "fundamentalidad" del error: cuando
  coexisten varios kinds, el de menor priority gana — léxico > sintáctico >
  semántico, porque un error más temprano en la pipeline suele ser la causa
  raíz de los posteriores).

### 16.3 Retagueo por fase

El default `kind` es `Semantic`. El driver usa `DiagnosticBag::set_kind_all`
para retaguear los bags del lexer y parser antes de fusionarlos:

```rust
let mut lex_bag = DiagnosticBag::new();
let tokens = lex(&source, &mut lex_bag);
lex_bag.set_kind_all(DiagnosticKind::Lexical);
merge(&mut bag, &lex_bag);
```

Esto evita tener que modificar cada call site dentro del lexer/parser.

### 16.4 Cálculo de (line, col)

`Diagnostic::primary_line_col() -> Option<(usize, usize)>` devuelve la
posición 1-based del primer label, computada con `SourceFile::line_col(offset)`
del crate `hulk-span`. El método permite que los crates de nivel superior
calculen posiciones a partir de un `Diagnostic` sin importar `hulk-span`
directamente, manteniendo la regla de capas.

### 16.5 Renderizado bonito

Para el binario de desarrollo `hulkc` y los tests, los diagnósticos también
se pueden renderizar con `codespan-reporting`, que produce salida estilo
Rust/Clang con sangría, fragmentos del fuente y subrayado de la zona ofensora.

---

## 17. Interfaces de línea de comandos

**Crate**: `hulk-cli`. Expone dos binarios con propósitos complementarios.

### 17.1 `hulk` — CLI minimalista

**Archivo**: `crates/hulk-cli/src/bin/hulk.rs`. Sin subcomandos ni banderas;
acepta como único argumento la ruta a un archivo `.hulk`. Comportamiento:

| Caso | Comportamiento |
|------|----------------|
| `./hulk programa.hulk` (válido) | Produce `./output` en el CWD, exit 0 |
| `./hulk` (sin args) o más de un arg | Imprime `usage: hulk <file.hulk>`, exit 2 |
| Archivo inexistente | `(0,0) SEMANTIC: input file '...' not found`, exit 3 |
| Error léxico | `(l,c) LEXICAL: msg` a stderr, exit 1 |
| Error sintáctico | `(l,c) SYNTACTIC: msg` a stderr, exit 2 |
| Error semántico | `(l,c) SEMANTIC: msg` a stderr, exit 3 |
| Mezcla de errores | Exit code del más fundamental (LEXICAL > SYNTACTIC > SEMANTIC) |

El ejecutable de salida siempre se escribe como `./output` relativo al CWD del
proceso, no al directorio donde reside el fuente. Esto desacopla la
compilación del lugar donde está el archivo de entrada y permite que un mismo
fuente compilado desde directorios distintos deje el binario en cada uno de
ellos.

Las posiciones reportadas se traducen al sistema de coordenadas del archivo
original del usuario descontando el offset del prelude prepended (sección 15),
para que la línea reportada coincida con la línea real del archivo `.hulk`.

### 17.2 `hulkc` — CLI con subcomandos

**Archivo**: `crates/hulk-cli/src/main.rs`. Construido sobre `clap` con
subcomandos:

- `hulkc compile <file> [--emit tokens|ast|hir|banner|llvm-ir|object|executable]
   [-o output]` — compila e imprime/escribe la representación intermedia
   elegida. Sirve para inspeccionar el resultado de cada fase.
- `hulkc run <file>` — compila a un binario temporal y lo ejecuta.
- `hulkc check <file>` — sólo análisis semántico (lex + parse + resolve +
  type-infer), sin codegen. Pensado para verificación tipo IDE.

### 17.3 Makefile

`Makefile` en la raíz del proyecto con tres targets:

```makefile
build:
    cargo build --release --bin hulk
    cp target/release/hulk ./hulk

clean:
    cargo clean
    rm -f ./hulk ./output

test:
    cargo test --workspace
```

`make build` produce el binario `hulk` en la raíz del proyecto. `make clean`
borra los artefactos de Cargo más `./hulk` y `./output`. `make test` ejecuta
la suite completa del workspace.

---

## 18. Sistema de pruebas

### 18.1 Niveles de testing

La validación del compilador se estructura en cuatro niveles, cada uno con
una razón de ser distinta:

- **Unit tests** (`#[cfg(test)] mod tests` dentro de cada crate). Cubren
  funciones individuales con casos pequeños donde el setup cabe en pocas
  líneas: una helper, un nodo AST sintético, una conversión.
- **Integration tests** (carpeta `tests/` de cada crate). Cruzan módulos del
  mismo crate o invocan a otros crates como caja negra. Aquí viven los tests
  que escriben programas HULK como strings y verifican lo que se obtiene
  tras varias fases.
- **Property tests con `proptest`**. Sirven para invariantes que se pueden
  enunciar pero no enumerar — por ejemplo, "el parser nunca paniquea sobre
  ningún input ASCII", "el desugaring de un Expr idéntico produce el mismo
  HIR" o "la inferencia es estable bajo permutación de declaraciones
  independientes". `proptest` genera entradas aleatorias y reduce el
  contraejemplo si encuentra uno.
- **End-to-end con programas HULK**. Compilan un programa real y comparan
  la salida del binario producido contra un archivo `.expected`. Viven
  principalmente en `crates/hulk-driver/tests/` y `crates/hulk-codegen/tests/`.

### 18.2 Distribución por crate

La tabla siguiente lista cuántos tests vive cada crate, separados en unit y
integration. Da una idea del peso relativo de cada capa y de los puntos donde
se concentra la validación:

| Crate | Unit | Integration | Total |
|-------|-----:|------------:|------:|
| hulk-ast | 4 | 42 | 46 |
| hulk-banner | 0 | 26 | 26 |
| hulk-codegen | 0 | 102 | 102 |
| hulk-desugar | 10 | 16 | 26 |
| hulk-diagnostics | 4 | 0 | 4 |
| hulk-driver | 1 | 187 | 188 |
| hulk-hir | 1 | 28 | 29 |
| hulk-lexer | 10 | 5 | 15 |
| hulk-macros | 11 | 9 | 20 |
| hulk-parser | 3 | 269 | 272 |
| hulk-semantic | 15 | 0 | 15 |
| hulk-span | 2 | 0 | 2 |
| hulk-tokens | 2 | 0 | 2 |
| hulk-types | 15 | 0 | 15 |

La concentración masiva en `hulk-parser` (272) refleja que la gramática es
la superficie con más casos discretos a chequear. `hulk-driver` (188) y
`hulk-codegen` (102) acumulan los tests end-to-end que ejercitan el pipeline
completo desde fuente HULK hasta binario nativo.

### 18.3 Programas HULK de prueba

Repartidos en cuatro carpetas según propósito:

- **`examples/`**: 21 programas que demuestran features individuales o
  pequeñas combinaciones — el "Game of Life" como ejemplo de OOP + bucles
  anidados, un árbol de expresiones como ejemplo de tipos algebraicos
  pequeños, programas de "Hello World" de varios niveles de complejidad.
- **`tests/`**: 60 programas numerados (`01_hello_world.hulk` a
  `60_math_library_program.hulk`) que constituyen una matriz organizada por
  feature: aritmética, operadores, scoping, recursión, OOP, herencia,
  vectores, protocolos, lambdas, etc. Cada uno tiene un `.expected`
  asociado.
- **`stress-test/`**: 7 programas pesados que combinan varias features bajo
  carga — math intensivo, OOP profundo, strings, iterables, vectores,
  recursión profunda, y un programa "mega" que combina todo.
- **`stress-test/gc/`**: 3 programas para ejercer al recolector — uno con
  sustained allocation, uno con ciclos, uno con walk de árbol profundo
  (sección 14).
- **`stress-test/torture/`**: 3 programas torture — multiplicación de
  matrices, calculadora RPN basada en stack, sorting de vectores.

### 18.4 Test de arquitectura

`crates/hulk-driver/tests/architecture.rs` lee los `Cargo.toml` de cada
crate y panic-ea si encuentra una dependencia que viole la regla de capas
(sección 2.3). Funciona como guardia mecánico: cuando alguien añade una
dependencia nueva por accidente o por presión de tiempo, el test falla y
fuerza la conversación arquitectónica.

---

## 19. Construcción y dependencias

### 19.1 Dependencias del workspace

Centralizadas en `Cargo.toml` raíz bajo `[workspace.dependencies]`:

- `thiserror = "1.0"` — errores tipados.
- `codespan-reporting = "0.11"` — renderizado de diagnósticos.
- `clap = "4.5"` — parser de CLI para `hulkc`.
- `insta = "1.38"` — snapshot tests.
- `proptest = "1.4"` — property-based testing.
- `inkwell = { version = "0.4.0", features = ["llvm17-0"] }` — bindings a LLVM.
- `tracing` + `tracing-subscriber` — instrumentación.

Cada crate referencia con `nombre.workspace = true` para versión única.

### 19.2 Lints estrictos

`Cargo.toml` raíz aplica a todos los crates:

```toml
[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
nursery = "deny"
unwrap_used = "deny"
expect_used = "deny"
```

`unwrap_used`/`expect_used` están denegados: cualquier `.unwrap()` o
`.expect()` en código productivo requiere `#[allow(...)]` con comentario que
justifique por qué es seguro. Esto fuerza manejo explícito de errores en
toda la base de código.

### 19.3 Requisitos de build

- **Rust ≥ 1.75** (la versión del `rust-toolchain.toml` lo fija).
- **LLVM 17** (libs de desarrollo: `llvm17-devel` en Fedora,
  `llvm-17-dev` + `libllvm17` en Ubuntu).
- **gcc + ar + cc** (estándares en cualquier distro).
- **libm** (siempre presente).

En Ubuntu LTS reciente: `apt install llvm-17-dev libllvm17 build-essential`.
En Fedora: `dnf install llvm17 llvm17-devel llvm17-static clang17 lld
libffi-devel`.

### 19.4 build.rs del codegen

`crates/hulk-codegen/build.rs` ya descrito en sección 14.11.

### 19.5 Manejo de variables de entorno

El proyecto no lee ninguna variable de entorno crítica en runtime. El
`build.rs` usa `OUT_DIR` y `CARGO_MANIFEST_DIR` (estándares de Cargo).
No hay `.env` ni secretos hardcoded.

---

## 20. Limitaciones conocidas

Algunas features del lenguaje están parcialmente implementadas o tienen
huecos identificados durante el desarrollo. Se documentan aquí para que el
lector tenga la imagen completa:

### 20.1 Front-end / semántica

- **Subtipado en validación de calls**: el chequeo de tipos de argumento usa
  igualdad exacta más Object como comodín; no permite `Dog` donde se espera
  `Animal`. Ningún programa válido se rechaza, pero un error tipo `pasar
  Number donde se espera Animal` no se captura si `Animal` se trata como
  Object.

- **`infer_self`/`infer_base` devuelven Object**: el inferidor no resuelve el
  tipo del `self` enclosing. El codegen lo resuelve con su propio
  `current_type`, así que esto no rompe programas, sólo es sub-óptimo.

- **`infer_method_call`/`infer_field_access` devuelven Object**: idem; el
  codegen tiene la información real.

### 20.2 Closures con captura de scope exterior

`function f(n) => (x) => x + n;` (una función que retorna una clausura que
captura su parámetro) crashea con `param not in param_temps` durante el
lowerer a BANNER. La sintaxis de lambda con captura sí funciona dentro de un
bloque (`let n = 5 in (x) => x + n` se desugarea correctamente como tipo
sintético con campo capturado, ver 10.3), pero el camino donde la clausura
es el valor de retorno de una función global no completa el cierre. Se
documenta en `doc/seccion-17-e2e-tests.md` como limitación #1. Mitigación:
modelar la operación con subtipos en lugar de clausuras de orden superior
(por ejemplo, declarar un tipo `Adder(n: Number)` con método `apply(x)`).

### 20.3 `as` downcast

`(obj as ConcreteType).method()` reporta `unresolved callee '__hulk_as'` en
el linker. Mitigación: dispatch virtual exclusivo (`obj.method()` directo
funciona). `is` para chequeo de tipo sí funciona; `as` para casting es lo
limitado.

### 20.4 Acceso a caracteres de string

Sólo existen `hulk_string_new` y `hulk_string_concat`. No hay `length`,
`char_at`, `substring` operativos a nivel HULK. Mitigación: trabajar con
`Number[]` cuando se necesite manipulación carácter-a-carácter.

### 20.5 Field access sobre valor retornado por función no-`new`

`mk().field` reporta `cannot resolve field 'field' on object — struct type
not statically known`. Mitigación: enlazar a una variable explícita primero
(`let o = mk() in o.field`).

### 20.6 Macros con cuerpo trailing-block

La sintaxis `name(args) { body }` (con el bloque como argumento implícito
después de los paréntesis) no está soportada. Sólo `name(args, { body })`.

### 20.7 Recursión profunda en `mark` del GC

Como se mencionó en 14.13, árboles extremadamente profundos podrían desbordar
la stack C en `mark`. En la práctica nunca se ha disparado; sería mitigable
con una pila explícita.

---

## 21. Conclusión

El compilador HULK descrito en este reporte es una implementación completa
del lenguaje especificado en `hulk-docs.pdf`, organizada como un workspace
de 15 crates con regla de capas verificada por test. Cada fase del pipeline
—lex, parse, resolución, inferencia, expansión de macros, desugaring,
lowering a BANNER, codegen LLVM, enlazado contra runtime C— vive en su
propio crate con responsabilidad delimitada e interfaz pública mínima.

El extra principal del proyecto es el **recolector de basura mark-and-sweep
preciso con shadow stack** descrito en la sección 14. La pieza está
integrada de extremo a extremo: BANNER define las instrucciones
`ShadowPush`/`ShadowPop` y el lowerer las emite para variables locales de
tipo referencia; el codegen las traduce a llamadas al runtime y, además,
emite `TypeTag` globales LLVM cuyos `pointer_offsets` se derivan
automáticamente del análisis de tipos de cada `TypeDescriptor`; los
`TypeTag*` se pasan como argumento a `hulk_alloc` en cada `new T(...)`; y
el runtime los consume para realizar trazado preciso (sin falsos positivos),
con manejo correcto de grafos cíclicos vía bit de marca, y threshold
adaptativo que mantiene amortizado el costo por byte asignado. La pieza está
validada por tests en C (`runtime/test_gc.c`), por tests BANNER que
verifican la emisión correcta de roots según el tipo del binding
(`crates/hulk-banner/tests/shadow_stack.rs`), y por programas HULK
end-to-end con presión sostenida de asignación, estructuras cíclicas y
árboles profundos (`stress-test/gc/`).

El sistema de diagnósticos clasificados por fase, con posiciones
trasladables al sistema de coordenadas del fuente original del usuario
(descontando el prelude prepended automáticamente), y con mensajes en
español, hace que los errores reportados sean accionables: el usuario lee
exactamente la línea y columna del archivo que escribió, con una categoría
clara del tipo de error.

Las limitaciones de la sección 20 son aspectos del lenguaje cuya
implementación está parcial o donde se prefirió un diseño conservador
(igualdad exacta en chequeo de tipos en lugar de subtipado estructural,
recursión en `mark` en lugar de pila explícita, shadow stack de tamaño
fijo). Todas tienen una mitigación clara documentada y no comprometen la
ejecución de programas HULK estándar.
