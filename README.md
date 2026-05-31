# HULK Compiler

[![CI](https://github.com/DayanCabrera2003/hulk_compiler/actions/workflows/ci.yml/badge.svg)](https://github.com/DayanCabrera2003/hulk_compiler/actions/workflows/ci.yml)
[![Coverage](https://github.com/DayanCabrera2003/hulk_compiler/actions/workflows/coverage.yml/badge.svg)](https://github.com/DayanCabrera2003/hulk_compiler/actions/workflows/coverage.yml)

Compilador completo del lenguaje **HULK** (Havana University Language for Kompilers).
Toma código fuente HULK y produce un ejecutable nativo mediante LLVM.

---

## Arquitectura

El compilador sigue un pipeline de nueve etapas:

```
Fuente HULK
    │
    ▼
hulk-lexer         Tokenización (comentarios //, strings, operadores)
    │
    ▼
hulk-parser        AST con NodeId únicos, recuperación de errores
    │
    ▼
hulk-semantic      Resolución de nombres y tabla de símbolos
    │
    ▼
hulk-types         Inferencia y verificación de tipos
    │
    ▼
hulk-hir           High-level IR con tipos anotados
    │
    ▼
hulk-macros        Expansión de macros y pattern matching
    │
    ▼
hulk-desugar       For → while, @@ → @ " " @, etc.
    │
    ▼
hulk-banner        BANNER three-address IR
    │
    ▼
hulk-codegen       LLVM IR + enlazado con runtime C → ejecutable nativo
```

Crates de soporte: `hulk-span`, `hulk-diagnostics`, `hulk-tokens`, `hulk-ast`,
`hulk-driver`, `hulk-cli`.

---

## Características implementadas

### Expresiones
- [x] Literales: números (`f64`), cadenas, `true`/`false`
- [x] Aritmética: `+` `-` `*` `/` `^` `%`
- [x] Comparaciones: `<` `<=` `>` `>=` `==` `!=`
- [x] Booleanos: `&` `|` `!`
- [x] Concatenación de cadenas: `@` (simple) y `@@` (con espacio)
- [x] Bloques de expresión: `{ e1; e2; ... }`
- [x] Bloques de expresión como valor (el último elemento es el resultado)

### Variables
- [x] Enlace léxico: `let x = e in body`
- [x] Múltiples bindings: `let x = e1, y = e2 in body`
- [x] Asignación destructiva: `x := e`
- [x] Anotaciones de tipo opcionales: `let x: Number = 42 in ...`

### Funciones
- [x] Funciones inline: `function f(x) => expr;`
- [x] Funciones full-form: `function f(x) { ... }`
- [x] Recursión y mutua recursión
- [x] Funciones tipadas: `function f(x: Number): Number => ...`
- [x] Lambdas / functores: `(x: Number) => x * 2`

### Control de flujo
- [x] Condicional: `if (cond) e elif (cond) e else e`
- [x] Bucle while: `while (cond) body`
- [x] Bucle for: `for (x in iterable) body`

### Sistema de tipos (OOP)
- [x] Declaración de tipos: `type Point(x, y) { ... }`
- [x] Atributos con inicializador
- [x] Métodos virtuales por defecto
- [x] Herencia: `type B inherits A(...) { ... }`
- [x] Polimorfismo y despacho virtual (vtable)
- [x] Llamada al padre: `base()`
- [x] Verificación dinámica: `expr is Type`
- [x] Downcast: `expr as Type`

### Protocolos
- [x] Definición: `protocol P { method(): Type; }`
- [x] Extensión: `protocol Q extends P { ... }`
- [x] Conformance estructural (cualquier tipo que implemente los métodos conforma)
- [x] Iterables vía protocolo (`next()` / `current()`)

### Vectores
- [x] Literal explícito: `[e1, e2, e3]`
- [x] Generador: `[expr | x in range(start, end)]`
- [x] Indexación: `v[i]`
- [x] Mutación de elemento: `v[i] := expr`
- [x] Tamaño: `v.size()`

### Macros
- [x] Declaración: `def macro(*args: Type): Type => body`
- [x] Argumentos variadic `*`, simbólicos `@`, valor `$`
- [x] Pattern matching en cuerpo de macros
- [x] Sintaxis trailing-block: `macro(args) { body }`

### Builtins
- [x] `print(expr)` — imprime cualquier valor
- [x] `range(start, end)` — iterable de números
- [x] `sqrt`, `sin`, `cos`, `exp`, `log(base, value)`, `rand`
- [x] Constantes `PI` y `E`
- [x] Métodos de cadena: `.size()`, `.charAt(i)`, `.substring(start, len)`

### Infraestructura
- [x] Recuperación de errores en lexer y parser (múltiples errores en un pase)
- [x] Diagnósticos con span, etiquetas y notas
- [x] Inferencia de tipos (sistema de constraints + unificación)
- [x] Generación de código nativo vía LLVM (`inkwell`)
- [x] Runtime en C con GC (Boehm) y funciones de soporte
- [x] Prelude inyectado automáticamente (`Iterable`, `Enumerable`, `Range`)

---

## Requisitos

- **Rust** 1.75.0 o superior
- **LLVM 17** con `clang-17`

### Instalar LLVM

```sh
# Debian / Ubuntu
sudo apt install llvm-17-dev clang-17

# macOS
brew install llvm@17
```

Si la variable `LLVM_SYS_170_PREFIX` no se detecta automáticamente, apúntala al
directorio de instalación de LLVM 17.

---

## Compilar el proyecto

```sh
cargo build --release
```

El binario queda en `target/release/hulkc`.

---

## Uso

### Ejecutar un programa HULK

```sh
hulkc run programa.hulk
```

### Compilar a ejecutable

```sh
hulkc compile programa.hulk -o salida
./salida
```

### Verificar errores sin compilar

```sh
hulkc check programa.hulk
```

### Inspeccionar etapas intermedias

```sh
hulkc compile programa.hulk --emit tokens    # stream de tokens
hulkc compile programa.hulk --emit ast       # AST
hulkc compile programa.hulk --emit hir       # HIR tipado
hulkc compile programa.hulk --emit banner    # BANNER three-address IR
hulkc compile programa.hulk --emit llvm-ir   # LLVM IR textual
hulkc compile programa.hulk --emit object    # archivo .o
```

---

## El lenguaje en un vistazo

```hulk
// Función recursiva con tipos
function fib(n: Number): Number =>
    if (n <= 1) n else fib(n - 1) + fib(n - 2);

// Tipos con herencia y polimorfismo
type Animal(name: String) {
    name: String = name;
    speak(): String => "...";
}

type Dog(name: String) inherits Animal(name) {
    speak(): String => "Woof!";
}

// Protocolos (structural typing)
protocol Printable {
    show(): String;
}

// Vectores y for
let squares = [x ^ 2 | x in range(1, 6)] in
    for (s in squares) print(s);

// let, destructive assignment, blocks
let a = 10 in {
    a := a + 1;
    print("fib(10) = " @ fib(10));
    print(new Dog("Rex").speak());
};
```

Los ejemplos completos están en [`examples/`](examples/) y
[`stress-test/`](stress-test/).

---

## Herramientas de desarrollo

| Comando | Propósito |
|---|---|
| `cargo test --workspace` | Ejecuta los 750+ tests |
| `cargo fmt --all --check` | Verifica formato |
| `cargo clippy --workspace --all-targets -- -D warnings` | Linter |
| `cargo deny check` | Auditoría de dependencias y licencias |

Instalar `cargo-deny`:

```sh
cargo install cargo-deny
```

---

## Especificación del lenguaje

La definición oficial del lenguaje HULK está en [`hulk-docs.pdf`](hulk-docs.pdf)
(_Principles of Programming Languages Design and Implementation_,
Alejandro Piad Morffis, 2026). El **Apéndice A** es la referencia canónica para
sintaxis, semántica y sistema de tipos.

La documentación interna de cada etapa del pipeline está en [`doc/`](doc/).

---

## Meta

- [`doc/`](doc/): Documentación técnica de cada sesión del pipeline.
- [`LICENSE`](LICENSE): MIT License.
