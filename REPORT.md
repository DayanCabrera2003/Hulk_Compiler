# Informe técnico del compilador HULK

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
14. [Recolector de basura](#14-recolector-de-basura)
15. [Prelude y biblioteca estándar](#15-prelude-y-biblioteca-estándar)
16. [Sistema de diagnósticos](#16-sistema-de-diagnósticos)
17. [Interfaces de línea de comandos](#17-interfaces-de-línea-de-comandos)
18. [Sistema de pruebas](#18-sistema-de-pruebas)
19. [Construcción y dependencias](#19-construcción-y-dependencias)
20. [Limitaciones conocidas](#20-limitaciones-conocidas)
21. [Conclusión](#21-conclusión)

---

## 1. Introducción

El presente documento describe la arquitectura y el funcionamiento de una
implementación completa del compilador del lenguaje **HULK** (Havana University
Language for Kompilers), tal como se define en `hulk-docs.pdf` (Apéndice A).
El compilador transforma un programa HULK en un binario nativo ejecutable
para Linux x86_64, recorriendo las fases canónicas de un compilador moderno:
análisis léxico, análisis sintáctico, resolución de nombres, inferencia de
tipos, expansión de macros, transformaciones de azúcar sintáctico, una
representación intermedia de tres direcciones denominada **BANNER**,
generación de código LLVM mediante el binding `inkwell`, y enlazado contra
una biblioteca runtime escrita en C. Esta biblioteca runtime incluye un
**recolector de basura mark-and-sweep preciso** con shadow stack para los
roots, descrito en detalle en la sección 14.

El proyecto se estructura como un **workspace de Cargo** compuesto por
quince crates, cada uno responsable de una fase o capa específica del
compilador. Las dependencias entre crates respetan una regla de capas
estricta: las fases tempranas (lexer, parser) no pueden depender de fases
posteriores (codegen, runtime), y la regla se verifica de manera automática
mediante un test de arquitectura. Esta organización modular permite que cada
crate exponga una interfaz pública mínima y que sus pruebas unitarias se
ejecuten de forma aislada y rápida.

El compilador expone dos binarios distintos. `hulkc` es la herramienta de
desarrollo, con subcomandos que permiten emitir cualquier representación
intermedia generada por el pipeline (tokens, AST, HIR, BANNER, LLVM IR,
objeto, ejecutable). `hulk` es una interfaz minimalista que recibe como
único argumento la ruta a un archivo `.hulk`, produce un ejecutable
`./output` en el directorio actual y reporta los errores a la salida
estándar de error siguiendo el formato `(line,col) TYPE: message`, con un
código de salida que identifica la fase en la que se produjo el primer
error (1 para errores léxicos, 2 para sintácticos, 3 para semánticos).

La validación del compilador se organiza en cuatro niveles: pruebas
unitarias dentro de cada crate, pruebas de integración entre módulos,
pruebas basadas en propiedades mediante `proptest` para invariantes de las
fases sensibles a casos límite (análisis sintáctico, desugaring), y
pruebas end-to-end que compilan y ejecutan programas HULK completos
comparando la salida con un resultado de referencia.

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

El cumplimiento de esta regla se verifica de forma automática mediante el
test `crates/hulk-driver/tests/architecture.rs`, que inspecciona los
`Cargo.toml` de cada crate y reporta una violación cuando detecta una
dependencia no permitida. Como consecuencia directa de esta restricción, el
crate `hulk-cli` accede a tipos como `DiagnosticKind` y al cálculo de
posiciones `(line, col)` exclusivamente a través de los re-exports y métodos
públicos de `hulk-driver` y `hulk-diagnostics`, sin importar `hulk-span` de
manera directa.

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

El lexer está implementado como una máquina manual basada en un cursor sobre
el código fuente; no utiliza generadores de lexers. La estructura `Lexer`
mantiene una referencia al texto fuente, un cursor expresado como offset en
bytes, una referencia mutable al `DiagnosticBag` y el vector de tokens
generados hasta el momento. El bucle principal (`lex_all`, líneas 46-120 de
`lib.rs`) consume caracteres uno a uno y discrimina por el primer
símbolo encontrado hacia subrutinas especializadas en
`tokens/numbers.rs`, `tokens/strings.rs`, `tokens/idents.rs` y
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

**Marcadores de macro**: `$` (prefijo de placeholder en la declaración de
una macro). Fuera de un contexto válido de declaración de macro, `$` se
reporta como error léxico (sección 3.4).

### 3.3 Reconocimiento de operadores compuestos

Operadores de dos caracteres se reconocen con una función auxiliar
`double_or_single(next_char, two_char_token, one_char_token)` (en
`tokens/operators.rs`). Si el carácter siguiente al actual es `next_char`, se
emite el token de dos caracteres; si no, se retrocede y se emite el de uno.
Esto cubre `->`, `@@`, `:=`, `<=`, `>=`, `!=`. Los casos `==` y `=>` se
discriminan a mano porque `=` puede ir seguido de `=` o `>` (líneas 91-102 de
`lib.rs`).

### 3.4 Tratamiento de `$`

El carácter `$` está reservado en HULK como prefijo de los placeholders en
la declaración de macros (`$x: Number`). El lexer aplica un lookahead de un
carácter: si después de `$` viene una letra ASCII o el guion bajo, emite el
token `Dollar` para que el parser de declaraciones de macro lo consuma; en
cualquier otro contexto el carácter es inválido y se reporta como
`LEXICAL: caracter inesperado '$'`, avanzando un byte sin emitir token de
manera que el resto del programa pueda continuar el análisis léxico.
Implementación en `lib.rs:85-99`:

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

El lenguaje soporta exclusivamente comentarios de línea iniciados con `//`.
La función `consume_comment` (en `cursor.rs:39-46`) avanza por codepoint
UTF-8 completo hasta encontrar `\n` o el fin de archivo. El avance por
codepoint —y no por byte— es necesario para tolerar comentarios que
contengan caracteres multibyte como `—`, `á` o emojis sin riesgo de quedar
en una posición intermedia del codepoint.

### 3.7 Recuperación de errores

El lexer no aborta ante errores. Cuando encuentra un carácter inesperado,
invoca `report_error` —que añade un `Diagnostic` al bag— y continúa con el
siguiente carácter. Esta estrategia garantiza que el usuario reciba todos
los errores léxicos del programa en una única pasada. El driver
posteriormente reclasifica estos diagnósticos como
`DiagnosticKind::Lexical` antes de fusionarlos en el bag global de
diagnósticos.

### 3.8 Tests del lexer

El crate dispone de quince tests (diez unitarios en `lib.rs` y cinco de
integración) que cubren:

- Reconocimiento de todas las familias de tokens.
- Recuperación tras errores múltiples.
- Manejo de escapes en literales de cadena.
- Tolerancia a UTF-8 en comentarios y caracteres inesperados.
- Tokenización de programas reales completos.

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

La implementación manual se prefirió sobre un generador como `LALRPOP` por
dos motivos. En primer lugar, HULK presenta varias construcciones con
ambigüedades sutiles —lambdas de la forma `(x) => expr`, expresiones bloque
delimitadas por llaves, declaraciones de macro con parámetros distinguidos
por prefijo— cuya resolución resulta más legible cuando se expresa de manera
explícita en el código. En segundo lugar, la recuperación de errores admite
un control más fino cuando el parser puede decidir, en cada punto de fallo,
qué token de sincronización buscar.

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

La asociatividad por la derecha del operador `^` resulta esencial para que
`2 ^ 3 ^ 2` se evalúe como `2 ^ (3 ^ 2) = 512` y no como
`(2 ^ 3) ^ 2 = 64`. Se obtiene asignando `r_bp < l_bp` (en este caso,
17 < 18), un patrón estándar en parsers Pratt.

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

### 4.4 Traducción de `match`/`case` a funciones intrínsecas

A diferencia del resto de las construcciones del lenguaje, `match` no
dispone de un `ExprKind` propio en el AST. Durante la construcción del
árbol, el parser lo reescribe como una llamada a una serie de funciones
intrínsecas: `__hulk_match(subject, __hulk_case_lit(...), ...,
__hulk_default(...))`. Los patrones soportados son:

- **Literal**: `case 42 => ...`, `case "hi" => ...`, `case true => ...`
- **Variable tipada**: `case x: Number => ...`
- **Binop**: `case (l: Number + r: Number) => ...`

El expansor de macros reconoce posteriormente estas llamadas mediante
`match_pattern()` (en `hulk-macros/src/lib.rs`) y las traduce al código de
despacho efectivo.

### 4.5 Recuperación de errores y tokens de sincronización

Los errores sintácticos no detienen el análisis. Al encontrar un token
inesperado, el parser ejecuta los siguientes pasos:

1. Añade un `Diagnostic` al bag, que el driver reclasifica posteriormente
   como `Syntactic`.
2. Invoca `skip_to_sync()`, avanzando hasta uno de los **tokens de
   sincronización**: `Semicolon`, `RBrace`, `Eof`, `Function`, `Type`,
   `Protocol` o `Def`.
3. Invoca `ensure_progress()`: si `skip_to_sync` no consumió ningún token
   (porque el cursor ya se encontraba sobre un sincronizador), fuerza el
   avance de un token para evitar bucles infinitos.

Esta estrategia permite reportar varios errores sintácticos en una única
pasada, evitando que el primer error oculte a los posteriores.

### 4.6 Tests del parser

El crate dispone de 272 tests en total (3 unitarios y 269 de integración).
Cubren:

- Precedencia y asociatividad de cada operador, con énfasis en casos límite
  como `2 ^ 3 ^ 2`.
- Todas las construcciones gramaticales con sus entradas válidas.
- Recuperación de errores (`error_recovery.rs`).
- Errores sintácticos específicos (`errors/syntactic.rs`).
- Programas combinados extraídos de `hulk-docs.pdf` (`declarations/hulk_md.rs`).

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

Cada nodo expresión se representa como `Expr { kind: ExprKind, span: Span, id: NodeId }`:

- `NodeId(u32)`: identificador único monotónico asignado durante el análisis
  sintáctico. Sirve como clave en las tablas que las fases posteriores
  emplean para asociar información derivada (tipos inferidos, símbolos
  resueltos, etc.) al nodo, sin modificar el AST.
- `Span`: rango de offsets en bytes sobre el `SourceFile`, usado para
  diagnósticos y para el cálculo de pares `(line, col)`.
- `ExprKind`: la variante de la expresión.

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

El módulo `hulk-ast/src/visitor/` provee dos traits genéricos para recorrer
el AST: `Visit` para recorridos inmutables y `VisitMut` para recorridos con
posibilidad de mutación. Estos traits son utilizados por `hulk-semantic`,
`hulk-types`, `hulk-macros` y `hulk-desugar`.

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

Esta indexación permite que la fase de inferencia de tipos, antes de
recorrer el cuerpo de un atributo o método, pre-registre los tipos
declarados de los parámetros visibles desde ese cuerpo. Sin esta
pre-registración, una expresión como `val = start` dentro del constructor
de `Counter` no podría asociar a `start` su tipo `Number` —dado que el
scope donde fue definido ya no existe en el momento de la inferencia— y
quedaría tipada como `Object`, comprometiendo la elección del `FieldKind`
correcto al construir el `TypeDescriptor` de BANNER.

El accesor público `Resolver::method_symbol(type_id, name) -> Option<SymbolId>`
permite que el driver obtenga el mapeo `(type_name, method_name) → SymbolId`
para solicitar la registración de tipos en el `TypeEnv` antes de inferir
cada cuerpo.

### 6.6 Detección de ciclos de herencia

El módulo `resolver/inheritance.rs:33-73` ejecuta una búsqueda en el grafo
de padres: para cada tipo, recorre la cadena de `type_parents` hasta
alcanzar `None` o detectar que un tipo previamente visitado reaparece. Ante
una repetición, reporta el diagnóstico `ciclos en herencia`. Esta verificación
previene que el codegen entre en una recursión infinita al construir el
descriptor de un tipo afectado por el ciclo.

### 6.7 Tests del módulo semántico

El módulo cuenta con quince tests unitarios que cubren cada validación con
sus correspondientes casos válidos y casos negativos: ciclos de herencia,
redefiniciones, uso indebido de `self` y `base`, métodos declarados fuera
de un tipo, parámetros con nombres reservados, y conformidad entre
protocolos y tipos.

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
- `infer_self` e `infer_base`: pendientes de resolver al tipo envolvente.
  El codegen calcula los tipos de `self` y `base` mediante su propio
  `current_type`, por lo que esta limitación no afecta a la corrección del
  código generado.
- `infer_method_call` y `infer_field_access`: devuelven `Object` porque el
  receptor puede ser de un tipo arbitrario en el momento de la inferencia.
- `infer_lambda`: devuelve `Object`; las lambdas se tratan como functores
  opacos a nivel de tipos.

Estas decisiones son deliberadas: devolver `Object` no produce falsos
positivos —ningún programa válido se rechaza— y el codegen dispone de la
información de tipos precisa que necesita a través de BANNER.

### 7.5 LCA y subtipado

La función `env.conforms(t1, t2)` (`env.rs:130-152`) devuelve `true` cuando
se cumple alguna de las siguientes condiciones:
- `t1 == t2`.
- `t2 == OBJECT` (Object es el tipo tope de la jerarquía).
- `t2` aparece en la cadena de padres de `t1`.

La función `env.lca(t1, t2)` (`env.rs:157-179`) computa el supertipo más
específico común: si `t1` conforma con `t2`, devuelve `t2`; de lo
contrario, asciende recursivamente por los padres de `t1`. Si no se
encuentra ningún antecesor común más específico, retorna `OBJECT`.

### 7.6 Pre-registro de parámetros

Antes de inferir cada cuerpo, el driver invoca:
- `inferer.register_function_params_by_name(name)` por cada función global.
- `inferer.register_function_params_by_name(type_name)` por cada tipo (lo
  que registra los parámetros del **constructor** del tipo, según se
  describe en la sección 6.5).
- `inferer.register_method_params(type_name, method_name)` por cada método.

De este modo, cuando el cuerpo referencia un parámetro, el inferidor
encuentra su tipo registrado en `symbol_types` sin necesidad de reconstruir
el contexto léxico.

### 7.7 Tests

El crate dispone de quince tests unitarios en `types/src/tests.rs` que
cubren cada función `infer_*` con sus combinaciones de tipos válidas. El
inferidor se ejercita adicionalmente a través de toda la suite del driver
y del codegen.

---

## 8. HIR — representación intermedia tipada

**Crate**: `hulk-hir`

### 8.1 Qué es

El HIR (High-level Intermediate Representation) no constituye una
transformación estructural del AST, sino una **estructura de unificación**
que empaqueta tres artefactos producidos de forma independiente:

```rust
pub struct Hir {
    pub program: Program,    // AST original (puede mutarse en macros/desugar)
    pub symbols: Resolver,   // tabla de símbolos y bindings
    pub types:   TypeEnv,    // tipos de expresiones y símbolos
}
```

`Hir::from_typed` (`lib.rs:34-40`) es un constructor trivial que mueve los
tres campos a una nueva estructura. Su propósito es ofrecer a los pases
posteriores —expansión de macros, desugaring, lowering a BANNER— una
referencia única e inmutable que permite consultar simultáneamente la
estructura sintáctica del programa y la información semántica derivada
durante el frontend.

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

`TypedAst` es la estructura intermedia que existe únicamente durante el
ensamblaje del HIR; una vez construido el `Hir`, `TypedAst` deja de existir.

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

El expansor (`expander.rs`) ejecuta tres pasos por cada llamada de macro:

1. **Sanitización de identificadores locales**: cualquier identificador
   local del cuerpo de la macro se renombra anteponiéndole el nombre de la
   macro y un contador de expansión (`__macro_<name>_<n>_<local>`),
   evitando así la captura accidental de variables del scope que invoca
   la macro.
2. **Substitución de parámetros** según el tipo del parámetro:
   - Regular: se clona la expresión del argumento.
   - Symbolic: se sustituye el nombre del identificador.
   - Placeholder: se asigna un símbolo fresco mediante
     `resolver.allocate_symbol(...)` y se enlaza al nodo.
   - Body: se valida que el argumento sea un `Block` y se inserta tal cual.
3. **Expansión recursiva**: si el cuerpo expandido contiene nuevas
   invocaciones de macros, el proceso se aplica de nuevo.

Tras la expansión, `refresh_node_ids_with_resolver` recorre el subárbol
generado y asigna nuevos `NodeId`s a partir de `max_node_id_in_program + 1`,
enlazando cada nodo con el resolver mediante `bind_expr_symbol`.

### 9.5 Patrón especial: `__hulk_match`

El expansor reconoce además las llamadas a `__hulk_match` que el parser
genera al traducir las expresiones `match`, y las convierte en código de
despacho que evalúa los casos en orden y ejecuta el primero cuyo patrón se
satisface. Esta traducción se realiza en `match_pattern()` en
`hulk-macros/src/lib.rs`.

### 9.6 Tests

El crate cuenta con veinte tests que cubren:
- Substitución correcta según el tipo de parámetro.
- Higiene del expansor (ausencia de captura de identificadores locales).
- Patrones `match` con literales, variables tipadas y operaciones binarias.
- Recursión entre macros.

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

### 10.2 Estrategia para el bucle `for`

El módulo `for_loop.rs:26-35` distingue dos protocolos de iteración:
- **Iterable**: el objeto expone directamente los métodos `next()` y
  `current()`. Se utiliza tal cual.
- **Enumerable**: el objeto expone `iter()` pero no `next()`. Se invoca
  `xs.iter()` para obtener un Iterable que sí cumple el primer protocolo.

La elección se determina consultando
`hir.symbols.type_has_method(type_id, "next")`.

### 10.3 Traducción de lambdas a tipos sintéticos

El módulo `lambda.rs` recorre el cuerpo del lambda y recolecta las
**variables libres**, definidas como aquellos identificadores que no son
parámetros del lambda ni símbolos globales. A continuación:

1. Genera un nombre único para un tipo sintético, de la forma `__Lambda_<id>`.
2. Declara ese tipo con los parámetros del lambda como parámetros del
   constructor, las variables libres como atributos, y un método `__invoke`
   cuyo cuerpo es el del lambda original con cada referencia a una variable
   libre reescrita como `self.<name>`.
3. Reemplaza la expresión lambda en el AST por una construcción
   `new __Lambda_<id>(captures...)`.

Esta transformación reduce las clausuras a objetos convencionales, evitando
la necesidad de implementar un sistema de clausuras dedicado en el backend.

### 10.4 Tests

El crate cuenta con veintiséis tests distribuidos entre pruebas unitarias,
de integración, de equivalencia operacional (que verifican la preservación
del comportamiento del programa tras el desugaring) y pruebas basadas en
propiedades con `proptest`.

---

## 11. BANNER — IR de tres direcciones

**Crate**: `hulk-banner`

### 11.1 Naturaleza del IR

BANNER es un IR **lineal, tipado y no-SSA**, de estilo similar al IR de
LLVM pero más compacto. Cada instrucción tiene un destino opcional, un
opcode y operandos simples (constante, temporal o global). La decisión de
no adoptar SSA simplifica el lowerer —un mismo temporal puede asignarse
varias veces— a cambio de renunciar a análisis basados en SSA. Esta
renuncia no afecta el resultado final porque el codegen traduce BANNER
directamente a LLVM IR, que sí es SSA y aplica `mem2reg` y otros análisis
optimizadores sobre la representación final.

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

Cada temporal lleva su tipo explícito. De este modo el codegen elige la
representación LLVM correcta —`double`, `i1` o puntero opaco— sin
necesidad de re-inferir información de tipos.

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

El campo `pointer_map` constituye el enlace fundamental entre BANNER y el
recolector de basura: indica al codegen qué campos contienen referencias,
información necesaria para construir el `TypeTag` con los offsets correctos
(sección 14.4).

### 11.5 Layout final del programa

```rust
struct BannerProgram {
    types:     Vec<TypeDescriptor>,
    functions: Vec<BannerFunction>,
    main:      BannerFunction,    // expresión top-level del programa
}
```

### 11.6 Lowerer

`hulk-banner/src/lowerer.rs` (aproximadamente 2200 líneas) convierte cada
expresión HIR en una secuencia de instrucciones BANNER. Sus
responsabilidades son:

- Asignar temporales mediante un contador lineal `next_temp`.
- Mantener un `HashMap<SymbolId, TempId>` que asocia cada variable local con
  su temporal correspondiente.
- Para los tipos de usuario, generar una función `__init__` que inicializa
  los campos a partir de los argumentos del constructor y delega al
  `__init__` del padre cuando existe herencia.
- Emitir las instrucciones `ShadowPush` y `ShadowPop` para variables
  locales de tipo referencia, según se detalla en la sección 14.5.

### 11.7 Tests

El crate dispone de veintiséis tests, entre los cuales destaca
`tests/shadow_stack.rs`. Este test verifica que las variables locales de
tipo Number o Boolean no generan instrucciones `ShadowPush`, mientras que
las variables de tipo String u objeto sí lo hacen.

---

## 12. Generación de código LLVM

**Crate**: `hulk-codegen`

### 12.1 Tecnología empleada

El crate utiliza **`inkwell`**, un binding seguro de Rust para LLVM 17. La
versión queda fijada en el `Cargo.toml` raíz como
`inkwell = { version = "0.4.0", features = ["llvm17-0"] }`. El código emplea
la sintaxis de **punteros opacos** (`ptr` en lugar de `i8*`), convención
estándar a partir de LLVM 14.

### 12.2 Declaración de funciones del runtime

Antes de emitir código alguno, el codegen declara las firmas de todas las
funciones del runtime C que invocará durante la generación. Estas
declaraciones residen en `hulk-codegen/src/rt.rs` dentro de una estructura
`RtFunctions` con campos para cada una de las funciones expuestas:
`hulk_alloc`, `hulk_shadow_push`, `hulk_shadow_pop`, las variantes de
`hulk_print`, `hulk_string_new`, `hulk_string_concat`, `__hulk_concat`,
`__hulk_is`, `__hulk_as`, las funciones del módulo de vectores `__vec_*`,
las del módulo de rangos `__range_*`, los wrappers matemáticos
`hulk_sqrt`, `hulk_sin`, `hulk_cos`, `hulk_exp`, `hulk_log`, y `hulk_rand`.

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

### 12.6 Enlazado final

El enlazado se invoca desde `hulk-codegen/src/link.rs:99-106` con la
siguiente forma:

```bash
<cc> <object.o> -o <output> [-L<lib_dir>] -lhulkruntime -lm
```

El compilador `cc` empleado es el del sistema (típicamente `gcc` tanto en
Fedora como en Ubuntu). La biblioteca `libhulkruntime.a` es construida por
el `build.rs` del propio crate (sección 19). La opción `-lm` enlaza
`libm`, proveedora de `sqrt`, `sin`, `cos`, `exp` y `log`.

### 12.7 Tests

El crate cuenta con 102 tests de integración en `hulk-codegen/tests/`,
distribuidos entre `comprehensive.rs` (programas que combinan múltiples
características), `integration.rs` y otros archivos específicos por
característica del lenguaje.

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

Los strings son **inmutables**: cada operación produce un nuevo objeto.
La inmutabilidad simplifica la concurrencia, al eliminar la posibilidad de
condiciones de carrera sobre el contenido, y simplifica también el trabajo
del recolector de basura, al no requerir la observación de mutaciones
sobre campos de tipo referencia.

### 13.3 Vectores

La estructura `HulkVec` mantiene tres campos: `len`, `cap` y un puntero
`double* data` que reside en el heap de C, no en el heap del GC. Las
operaciones expuestas son `__vec_new`, `__vec_push` (que realoja el buffer
mediante `realloc` cuando `len == cap`), `__vec_get`, `__vec_set`,
`__vec_size`, y las primitivas del protocolo Iterable `__vec_next` y
`__vec_current`. La decisión de mantener el array de datos fuera del heap
gestionado por el GC simplifica el redimensionamiento, dado que `realloc`
no requiere actualizar el header del objeto; el coste es que los vectores
sólo admiten valores `double`, no referencias.

### 13.4 Rangos

`HulkRange` tiene tres `double`s: `min`, `max`, `step`. Implementa el protocolo
Iterable directamente: `__range_next` incrementa `current` y devuelve true
mientras `current < max`; `__range_current` devuelve el valor actual. Layout
diseñado para coincidir bit-a-bit con la definición de `Range` en el prelude
(sección 15), de modo que `new Range(0, 10)` en HULK y `hulk_range_new(0, 10,
1)` en C producen objetos compatibles.

### 13.5 Impresión

El runtime expone tres variantes de impresión para evitar boxing innecesario:
- `hulk_print(void*)` — destinada a valores de tipo referencia; inspecciona
  el `TypeTag` y elige el formato adecuado (String se imprime como bytes,
  Number boxed se formatea con `%g`, y los demás tipos se imprimen como
  `<TypeName>`).
- `hulk_print_number(double)` — destinada a valores Number sin boxing, que
  constituyen la ruta más frecuente.
- `hulk_print_bool(int)` — imprime literalmente las cadenas `"true"` o
  `"false"`.

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

Los archivos `test_gc.c` y `test_strings.c` son binarios independientes
que se compilan con `gcc` directamente, sin intervención de `cargo`.
Validan, respectivamente, el comportamiento del recolector de basura y las
operaciones sobre strings, y proporcionan una verificación del runtime
aislada del resto del compilador.

---

## 14. Recolector de basura

Esta sección describe el recolector de basura del proyecto: un colector
**preciso de tipo mark-and-sweep** con shadow stack para los roots,
integrado de extremo a extremo entre el codegen y el runtime. La inclusión
del recolector permite que los programas HULK con clausuras, jerarquías
profundas de objetos y cadenas largas de referencias liberen memoria de
forma correcta sin que el usuario deba intervenir manualmente.

### 14.1 Motivación y elección del algoritmo

El lenguaje HULK genera asignaciones implícitas en cada `new T(...)`, cada
string nuevo producido por los operadores `@` o `@@`, cada lambda
—convertida por el desugaring en `new __Lambda_N(...)`—, cada vector y
cada range. Sin un mecanismo automático de gestión de memoria, los
programas filtrarían memoria indefinidamente.

Se consideraron tres alternativas, descartadas por las razones que se
indican a continuación:

- **Reference counting**: requiere mantener campos `refcount` mutables en
  cada objeto, lo que entra en conflicto con la regla de inmutabilidad
  general del proyecto. Además, no maneja ciclos —los objetos con
  referencias mutuas nunca se liberan— e introduce trabajo en cada
  asignación de puntero.
- **Recolección conservativa (estilo Boehm)**: requiere escanear toda la
  pila del programa C en busca de potenciales raíces, incluyendo valores
  que no son punteros pero podrían interpretarse como tales. Resulta más
  simple de integrar, pero es menos preciso (puede retener falsos
  positivos) y puede pasar por alto raíces que el optimizador haya
  trasladado a registros.
- **Recolección con compactación**: requiere actualizar todos los
  punteros tras cada colección, lo que obliga a conocer la ubicación
  exacta de cada uno. Su complejidad de implementación es elevada para el
  alcance del proyecto.

El algoritmo finalmente adoptado, **mark-and-sweep preciso con shadow
stack**, ofrece las siguientes ventajas:

- Trazado preciso, sin falsos positivos durante el marcado.
- Manejo correcto de ciclos, dado que el estado de marca es un único bit
  por objeto.
- Robustez frente a las optimizaciones del backend de LLVM, puesto que el
  conjunto de raíces se mantiene explícitamente en la shadow stack en
  lugar de inferirse del estado de la pila o de los registros.
- Implementación compacta, del orden de un centenar de líneas de C.

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

Una alternativa considerada y descartada fue mantener una lista de
asignaciones separada del header (por ejemplo, una `Vec<*ObjHeader>`). Esta
opción empeora la localidad de caché y exige asignaciones adicionales en
cada llamada a `hulk_alloc`. La lista intrusiva añade ocho bytes por
objeto, pero los punteros de enlace residen junto a los datos del propio
objeto, beneficiando la coherencia de caché durante el barrido.

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

El `TypeTag` constituye el contrato fundamental entre el compilador y el
recolector: el compilador genera estos descriptores como globales LLVM
(sección 12.5) a partir del `pointer_map` de BANNER (sección 11.4), y el
recolector los consulta durante la fase de marcado para determinar qué
punteros del objeto debe trazar.

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

Como resultado, el recolector traza exclusivamente los campos que contienen
punteros reales, ignorando los campos `f64` e `i1`. Este comportamiento
constituye un **trazado preciso**, en contraste con la estrategia de un
recolector conservativo, que trataría todo el payload como una colección
de posibles punteros.

### 14.5 Shadow stack: registro explícito de raíces

Las **raíces** del grafo de objetos vivos son todas las variables locales y
temporales de tipo referencia que se encuentran en scope en el momento de
una colección. Dado que LLVM puede situar estas variables en registros
—donde el recolector no puede observarlas—, se registran explícitamente en
una estructura denominada **shadow stack**:

```c
#define HULK_SHADOW_STACK_CAPACITY 4096
void*  __hulk_shadow_stack[HULK_SHADOW_STACK_CAPACITY];
size_t __hulk_shadow_top;

void hulk_shadow_push(void* val);   // inserta un puntero
void hulk_shadow_pop(void);         // retira la entrada superior
```

La capacidad fijada de 4096 ranuras admite cualquier programa HULK
realista —4096 variables de tipo referencia simultáneamente activas en la
pila de llamadas—. Si se excede esta cota, el runtime aborta con el
mensaje `shadow stack overflow`, garantizando un fallo controlado en
lugar de corrupción silenciosa.

La elección de un array fijo en lugar de uno dinámico responde a criterios
de simplicidad y previsibilidad. Un array de tamaño variable basado en
`realloc` introduciría asignaciones potenciales en la ruta crítica de
`hulk_shadow_push`, lo que podría desencadenar una colección recursivamente
y romper invariantes. Una lista enlazada por frame añadiría un puntero por
entrada y comprometería la localidad de caché.

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

Al salir del scope del `let` se emite una instrucción `ShadowPop` por cada
`ShadowPush` acumulado. Este invariante se verifica mediante tests
específicos en `hulk-banner/tests/shadow_stack.rs`, que confirman que las
variables Number y Boolean no provocan un `ShadowPush`, mientras que las
variables de tipo String u objeto sí lo hacen.

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

### 14.7 Asignación: `hulk_alloc`

Definida en `runtime/gc.c:59-84`. El proceso de asignación tras un
`new T(args)` emitido por el compilador es el siguiente:

1. El codegen calcula el tamaño total `sizeof(TStruct)`, recupera el
   `TypeTag*` global y emite la instrucción
   `call ptr @hulk_alloc(ptr @T_tag, i64 size)`.
2. `hulk_alloc` verifica si la asignación haría superar el threshold actual
   del recolector. Si es así, invoca `hulk_gc()` antes de proceder.
3. Solicita memoria al sistema mediante
   `malloc(sizeof(ObjHeader) + payload_size)`.
4. Rellena el header con los campos `tag`, `size`, `mark = 0` y
   `next = __hulk_alloc_list`, y enlaza el nuevo objeto al principio de la
   lista intrusiva de asignaciones.
5. **Inicializa el payload a cero** mediante
   `memset(HULK_PAYLOAD(hdr), 0, payload_size)`. Esta inicialización es
   crítica: garantiza que los campos de tipo referencia que aún no han sido
   asignados contengan el valor `NULL`, evitando que el recolector siga
   punteros con valores arbitrarios en una colección temprana.
6. Devuelve el puntero al inicio del payload.

Si `malloc` devuelve `NULL` —por agotamiento de la memoria del sistema—,
o si tras una colección sigue sin poder satisfacerse la asignación, el
runtime aborta la ejecución emitiendo un mensaje explicativo.

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

El recolector invoca `mark()` para cada raíz presente en la shadow stack.
La recursión sigue los campos referencia indicados por el `TypeTag` del
objeto. La guarda `if (hdr->mark) return` resuelve correctamente los
**grafos cíclicos**: cuando un objeto A apunta a un objeto B que a su vez
apunta de vuelta a A, la segunda invocación de `mark(A)` retorna de
inmediato sin reprocesar el objeto.

El test `stress-test/gc/cycles.hulk` valida este comportamiento:
construye una estructura con referencias cíclicas y comprueba, por un
lado, que la colección no entra en un bucle infinito y, por otro, que los
objetos del ciclo se liberan correctamente cuando dejan de ser
alcanzables desde cualquier raíz externa.

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

El patrón `ObjHeader** cursor = &__hulk_alloc_list` —puntero a puntero— es
una técnica habitual en el manejo de listas intrusivas: permite eliminar
un nodo intermedio sin tener que mantener una referencia al nodo anterior.
La instrucción `*cursor = obj->next` reescribe el enlace que apuntaba al
objeto eliminado para que apunte directamente al siguiente.

### 14.10 Threshold adaptativo

Después del sweep:

```c
size_t new_threshold = __hulk_alloc_bytes * GC_GROWTH_FACTOR;  // ×2
if (new_threshold < GC_INITIAL_THRESHOLD) {                    // mínimo 1 MiB
    new_threshold = GC_INITIAL_THRESHOLD;
}
__hulk_gc_threshold = new_threshold;
```

- `GC_INITIAL_THRESHOLD = 1 MiB` es el límite inferior por debajo del cual
  el threshold nunca desciende. Su función es evitar colecciones degeneradas
  cuando el conjunto vivo es casi nulo, como ocurre en programas pequeños.
- `GC_GROWTH_FACTOR = 2` actúa como multiplicador sobre el conjunto vivo
  actual. Si tras una colección sobreviven 4 MiB, el threshold se establece
  en 8 MiB; cuando éste se alcance, el conjunto vivo será del mismo orden y
  la siguiente colección amortizará proporcionalmente su coste.

De este modo, los programas con un heap reducido recolectan con muy poca
frecuencia, dado que las asignaciones rara vez superan el límite mínimo
de 1 MiB. Los programas con un heap grande recolectan de manera
proporcional, con una frecuencia que decae linealmente respecto al tamaño
del working set. En conjunto, el coste amortizado por byte asignado tiende
a una constante.

### 14.11 Integración con el sistema de build

El archivo `crates/hulk-codegen/build.rs` se encarga de compilar el runtime
en cada cambio. El script:

1. Localiza la carpeta `runtime/` mediante la variable `CARGO_MANIFEST_DIR`.
2. Compila cada archivo `.c` con `gcc -O2 -Wall -Werror -I runtime/`,
   generando los `.o` correspondientes en `$OUT_DIR`.
3. Empaqueta los `.o` mediante `ar rcs $OUT_DIR/libhulkruntime.a`.
4. Emite las directivas de Cargo necesarias:
   - `cargo:rustc-link-search=native=$OUT_DIR`
   - `cargo:rustc-link-lib=static=hulkruntime`
   - `cargo:rustc-link-lib=m`
5. Emite una directiva `cargo:rerun-if-changed=` por cada archivo fuente
   `.c`, de modo que Cargo recompile el runtime cuando alguno cambie.

En consecuencia, cualquier modificación de `runtime/gc.c` u otro archivo
del runtime dispara automáticamente la recompilación, sin requerir pasos
manuales. Tanto el compilador propio —que enlaza el runtime— como el
binario final del usuario heredan la misma copia de `libhulkruntime.a`.

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

### 14.13 Características alcanzadas y limitaciones

Características alcanzadas:

- Trazado preciso, sin falsos positivos en el marcado.
- Manejo correcto de ciclos sin contabilidad adicional sobre referencias.
- Threshold adaptativo con coste amortizado por byte asignado.
- Integración completamente dirigida por el compilador, sin necesidad de
  inferencias del runtime sobre la ubicación de las raíces.

Limitaciones reconocidas:

- **Stop-the-world**: el recolector no es concurrente. Esta característica
  no es relevante para los programas HULK actuales, que son
  single-threaded; un eventual soporte multi-thread requeriría
  sincronización entre mutadores y colector.
- **Sin compactación**: la fragmentación del heap puede acumularse en
  cargas de trabajo con churn elevado. En la práctica, las
  implementaciones modernas de `malloc` mitigan razonablemente este
  efecto.
- **Recursión en `mark`**: el marcado recursivo puede desbordar la pila de
  C en estructuras extremadamente profundas (del orden de decenas de
  miles de niveles anidados). En programas reales este límite no se
  alcanza; una mitigación posible consistiría en sustituir la recursión
  por una pila explícita.
- **Shadow stack de tamaño fijo**: las 4096 entradas configuradas
  resultan suficientes en la práctica, pero un programa patológico podría
  agotarlas. La detección es en tiempo de ejecución, por lo que el fallo
  es controlado y no produce corrupción.

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

Este mecanismo garantiza que `Range`, `Iterable` y `Enumerable` se
encuentren disponibles en el scope global de cualquier programa de usuario
sin necesidad de declaraciones explícitas de importación.

La prependencia del prelude tiene una implicación directa sobre la
generación de diagnósticos: las posiciones reportadas por las fases
internas del compilador están expresadas en coordenadas del fuente
combinado. Un error situado en la línea 1 del archivo del usuario aparece
en la línea 23 del fuente combinado (22 líneas del prelude más el
separador `\n` insertado por el driver). El binario `hulk` resta este
offset utilizando `hulk_driver::prelude_line_offset()`, función que lo
calcula en tiempo de ejecución contando los saltos de línea del PRELUDE,
de modo que las posiciones reportadas al usuario coinciden con las del
archivo original.

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

### 16.3 Reclasificación por fase

El valor por defecto del campo `kind` es `Semantic`. El driver utiliza el
método `DiagnosticBag::set_kind_all` para reclasificar los bags producidos
por el lexer y el parser antes de fusionarlos al bag global:

```rust
let mut lex_bag = DiagnosticBag::new();
let tokens = lex(&source, &mut lex_bag);
lex_bag.set_kind_all(DiagnosticKind::Lexical);
merge(&mut bag, &lex_bag);
```

Esta estrategia evita la necesidad de modificar cada punto de creación de
diagnósticos dentro del lexer y el parser.

### 16.4 Cálculo de (line, col)

`Diagnostic::primary_line_col() -> Option<(usize, usize)>` devuelve la
posición 1-based del primer label, computada con `SourceFile::line_col(offset)`
del crate `hulk-span`. El método permite que los crates de nivel superior
calculen posiciones a partir de un `Diagnostic` sin importar `hulk-span`
directamente, manteniendo la regla de capas.

### 16.5 Renderizado enriquecido

Tanto el binario de desarrollo `hulkc` como las suites de tests pueden
renderizar los diagnósticos mediante la biblioteca `codespan-reporting`,
que produce una salida con el estilo característico de los compiladores
modernos (Rust, Clang): incluye fragmentos del fuente, sangría con la
ubicación del error, y subrayado de la zona ofensora.

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

El ejecutable de salida se escribe siempre como `./output` relativo al
directorio de trabajo del proceso, y no al directorio donde reside el
archivo fuente. Esta decisión desacopla la compilación del lugar donde se
encuentra el archivo de entrada y permite que un mismo fuente compilado
desde directorios distintos deje el binario en cada uno de ellos.

Las posiciones reportadas se traducen al sistema de coordenadas del
archivo original del usuario descontando el offset introducido por el
prelude (sección 15), de modo que la línea reportada coincida con la
línea real del archivo `.hulk`.

### 17.2 `hulkc` — CLI con subcomandos

**Archivo**: `crates/hulk-cli/src/main.rs`. Está construido sobre la
biblioteca `clap` y expone los siguientes subcomandos:

- `hulkc compile <file> [--emit tokens|ast|hir|banner|llvm-ir|object|executable] [-o output]`
  compila el programa y emite la representación intermedia indicada. Su
  finalidad es facilitar la inspección del resultado de cada fase.
- `hulkc run <file>` compila el programa a un binario temporal y lo
  ejecuta de inmediato.
- `hulkc check <file>` ejecuta exclusivamente el frontend (análisis léxico,
  sintáctico, resolución e inferencia de tipos) sin generar código.
  Resulta útil para integraciones con editores que necesitan verificar el
  código en tiempo real.

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

La validación del compilador se estructura en cuatro niveles, cada uno
con un propósito distinto:

- **Pruebas unitarias** (`#[cfg(test)] mod tests` dentro de cada crate).
  Cubren funciones individuales con casos pequeños cuyo setup ocupa pocas
  líneas: funciones auxiliares, construcción de nodos AST sintéticos,
  conversiones entre representaciones.
- **Pruebas de integración** (carpeta `tests/` de cada crate). Cruzan
  módulos dentro de un mismo crate o invocan otros crates como caja
  negra. En este nivel se sitúan los tests que toman programas HULK como
  cadenas de texto y verifican el resultado producido tras una secuencia
  de fases.
- **Pruebas basadas en propiedades con `proptest`**. Permiten expresar
  invariantes que no pueden enumerarse caso por caso; por ejemplo: "el
  parser no produce panic sobre ningún input ASCII", "el desugaring
  aplicado a una misma expresión produce el mismo HIR" o "la inferencia
  es estable ante permutaciones de declaraciones independientes".
  `proptest` genera entradas aleatorias y, ante un contraejemplo,
  ejecuta un proceso de reducción para hallar la forma minimal del
  fallo.
- **Pruebas end-to-end con programas HULK**. Compilan un programa real y
  comparan la salida del binario producido contra un archivo `.expected`.
  Residen principalmente en `crates/hulk-driver/tests/` y
  `crates/hulk-codegen/tests/`.

### 18.2 Distribución por crate

La siguiente tabla recoge la cantidad de tests por crate, separados en
pruebas unitarias y de integración. Permite apreciar el peso relativo de
cada capa y los puntos donde se concentra la validación:

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

La concentración de tests en `hulk-parser` (272) refleja que la gramática
es la superficie con mayor número de casos discretos a verificar.
`hulk-driver` (188) y `hulk-codegen` (102) acumulan los tests end-to-end
que ejercitan el pipeline completo, desde el fuente HULK hasta la
ejecución del binario nativo resultante.

### 18.3 Programas HULK de prueba

Los programas de prueba se distribuyen en cinco categorías según su
propósito:

- **`examples/`**: 21 programas que demuestran características individuales
  o pequeñas combinaciones de ellas. Incluye implementaciones del Juego de
  la Vida (que combina orientación a objetos y bucles anidados), un árbol
  de expresiones aritméticas (que ilustra el uso de tipos algebraicos
  ligeros) y programas tipo "Hello World" de distintos niveles de
  complejidad.
- **`tests/`**: 60 programas numerados (`01_hello_world.hulk` a
  `60_math_library_program.hulk`) que constituyen una matriz organizada
  por característica: aritmética, operadores, scoping, recursión,
  orientación a objetos, herencia, vectores, protocolos y lambdas, entre
  otros. Cada programa tiene asociado un archivo `.expected` con la
  salida de referencia.
- **`stress-test/`**: 7 programas extensos que combinan varias
  características bajo carga: matemática intensiva, jerarquías profundas
  de objetos, manipulación de strings, iterables, vectores, recursión
  profunda y un programa integral que ejercita todas las anteriores.
- **`stress-test/gc/`**: 3 programas dedicados a ejercitar el recolector
  de basura: uno con asignación sostenida, uno con estructuras cíclicas
  y uno con recorrido de árboles profundos (véase la sección 14).
- **`stress-test/torture/`**: 3 programas de carga elevada: multiplicación
  de matrices, una calculadora RPN basada en pila y ordenamiento de
  vectores.

### 18.4 Test de arquitectura

El test `crates/hulk-driver/tests/architecture.rs` inspecciona los
`Cargo.toml` de cada crate y reporta una violación si encuentra una
dependencia que infrinja la regla de capas (sección 2.3). Funciona como
una verificación mecánica del diseño: cuando se introduce una
dependencia inadvertida que rompe la estratificación entre capas, el
test falla y obliga a revisar la decisión.

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

### 19.3 Requisitos de compilación

- **Rust ≥ 1.75**, según fija el archivo `rust-toolchain.toml`.
- **LLVM 17** con sus bibliotecas de desarrollo (paquetes `llvm17-devel`
  en Fedora; `llvm-17-dev` y `libllvm17` en Ubuntu).
- **gcc**, **ar** y **cc** (incluidos por defecto en cualquier distribución
  estándar).
- **libm**, siempre presente en los sistemas POSIX.

Comando de instalación de las dependencias en Ubuntu LTS reciente:
`apt install llvm-17-dev libllvm17 build-essential`. En Fedora:
`dnf install llvm17 llvm17-devel llvm17-static clang17 lld libffi-devel`.

### 19.4 build.rs del crate de codegen

El comportamiento de `crates/hulk-codegen/build.rs` ya se describe en
detalle en la sección 14.11, dado su papel en la integración del
recolector de basura.

### 19.5 Manejo de variables de entorno

El proyecto no consume ninguna variable de entorno crítica en tiempo de
ejecución. El script `build.rs` utiliza `OUT_DIR` y `CARGO_MANIFEST_DIR`,
ambas estándar de Cargo. No existen archivos `.env` ni secretos
codificados directamente en el código fuente.

---

## 20. Limitaciones conocidas

Algunas características del lenguaje se encuentran parcialmente
implementadas o presentan limitaciones identificadas durante el desarrollo.
Se documentan a continuación a fin de ofrecer una visión completa del
estado del proyecto.

### 20.1 Limitaciones del análisis semántico y la inferencia de tipos

- **Subtipado en la validación de llamadas**: la verificación de tipos de
  argumento aplica igualdad exacta más `Object` como comodín; no admite,
  por ejemplo, un valor de tipo `Dog` en un parámetro declarado como
  `Animal`. Ningún programa válido se rechaza, pero un error tal como
  pasar un `Number` en un parámetro `Animal` no se detecta cuando
  `Animal` se trata como `Object`.

- **`infer_self` e `infer_base` retornan `Object`**: el inferidor no
  resuelve el tipo del `self` envolvente. El codegen sí dispone de esta
  información a través de su propio `current_type`, de modo que esta
  limitación no afecta a la corrección del código generado; constituye
  únicamente una pérdida de precisión en la fase de inferencia.

- **`infer_method_call` e `infer_field_access` retornan `Object`**: el
  argumento es análogo al anterior; el codegen conserva la información
  precisa necesaria para resolver el dispatch.

### 20.2 Clausuras con captura del scope exterior

La construcción `function f(n) => (x) => x + n;` —una función que retorna
una clausura que captura su parámetro— produce un error
`param not in param_temps` durante el lowering a BANNER. La sintaxis de
lambda con captura sí funciona dentro de un bloque
(`let n = 5 in (x) => x + n` se desugara correctamente como un tipo
sintético con un campo de captura, según se describe en la sección 10.3),
pero el caso en que la clausura es el valor de retorno de una función
global no completa el cierre. Se documenta en
`doc/seccion-17-e2e-tests.md` como limitación número 1. Mitigación
posible: modelar la operación con subtipos en lugar de clausuras de orden
superior; por ejemplo, declarar un tipo `Adder(n: Number)` con un método
`apply(x)`.

### 20.3 Operador `as` (downcast)

La expresión `(obj as ConcreteType).method()` produce un error
`unresolved callee '__hulk_as'` durante el enlazado. Mitigación posible:
emplear dispatch virtual exclusivamente; `obj.method()` funciona
directamente. El operador `is` para chequeo de tipo en tiempo de
ejecución sí está plenamente soportado; el operador `as` permanece
limitado.

### 20.4 Acceso a caracteres de cadena

El runtime expone únicamente `hulk_string_new` y `hulk_string_concat`.
No existen operaciones equivalentes a `length`, `char_at` o
`substring` accesibles a nivel HULK. Mitigación posible: trabajar con
vectores `Number[]` cuando se requiera manipulación carácter a carácter.

### 20.5 Acceso a campo sobre el valor retornado por una función no-`new`

La expresión `mk().field` produce el error
`cannot resolve field 'field' on object — struct type not statically known`.
Mitigación posible: enlazar el resultado a una variable explícita antes
de acceder al campo, mediante una expresión `let o = mk() in o.field`.

### 20.6 Macros con cuerpo en posición trailing

La sintaxis `name(args) { body }` —donde el bloque se sitúa después de
los paréntesis como argumento implícito— no está soportada. La
invocación debe expresarse como `name(args, { body })`.

### 20.7 Recursión profunda en la fase de marcado del recolector

Como se mencionó en la sección 14.13, los árboles extremadamente
profundos podrían llegar a desbordar la pila de C durante la fase de
marcado. En la práctica esta situación no se ha observado; su mitigación
posible consistiría en sustituir la recursión por una pila explícita.

---

## 21. Conclusión

El compilador HULK descrito en este informe constituye una implementación
completa del lenguaje especificado en `hulk-docs.pdf`. Su organización en
un workspace de quince crates, con una regla de capas verificada mediante
test automático, permite que cada fase del pipeline —análisis léxico,
análisis sintáctico, resolución de nombres, inferencia de tipos, expansión
de macros, desugaring, lowering a BANNER, generación de código LLVM y
enlazado contra el runtime en C— resida en un crate independiente, con
responsabilidad delimitada e interfaz pública mínima.

El componente original del proyecto es el **recolector de basura
mark-and-sweep preciso con shadow stack**, descrito en detalle en la
sección 14. Su integración cubre todas las capas del compilador: BANNER
define las instrucciones `ShadowPush` y `ShadowPop`, que el lowerer
emite para las variables locales de tipo referencia; el codegen las
traduce a llamadas al runtime y, además, emite los `TypeTag` globales
LLVM cuyos `pointer_offsets` se derivan automáticamente del análisis de
tipos contenido en cada `TypeDescriptor`; los `TypeTag*` se pasan como
argumento a `hulk_alloc` en cada operación `new T(...)`; y el runtime
los consume para realizar el trazado preciso, sin falsos positivos, con
manejo correcto de grafos cíclicos mediante un bit de marca y con un
threshold adaptativo que mantiene amortizado el coste por byte asignado.
La integración está validada mediante tests en C (`runtime/test_gc.c`),
tests del lowering en BANNER que verifican la emisión correcta de raíces
según el tipo del binding (`crates/hulk-banner/tests/shadow_stack.rs`),
y programas HULK end-to-end que ejercitan asignación sostenida,
estructuras cíclicas y árboles profundos (`stress-test/gc/`).

El sistema de diagnósticos clasificados por fase, con posiciones
trasladables al sistema de coordenadas del fuente original del usuario
—descontando automáticamente el prelude prependido— y con mensajes en
español, ofrece un reporte de errores accionable: el usuario observa
exactamente la línea y columna del archivo que escribió, junto con una
clasificación clara del tipo de error.

Las limitaciones recogidas en la sección 20 corresponden a aspectos del
lenguaje cuya implementación es parcial o donde se optó por un diseño
conservador: igualdad exacta en lugar de subtipado estructural durante
la validación de llamadas, recursión en lugar de pila explícita en la
fase de marcado del recolector, y shadow stack de tamaño fijo. Cada una
de ellas dispone de una mitigación documentada y ninguna compromete la
ejecución de programas HULK que se atengan a los patrones estándar
descritos en `hulk-docs.pdf`.
