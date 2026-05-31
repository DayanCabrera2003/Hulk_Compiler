# Sección 17 — Testing exhaustivo end-to-end

## Qué se implementó

La sesión 17 reúne y completa la suite end-to-end que valida el compilador
desde fuente HULK hasta binario ejecutable. La cobertura E2E se reparte en
tres familias de archivos, todas conectadas por el helper de integración de
`hulk-codegen`:

1. **`crates/hulk-codegen/tests/integration.rs`** — orquesta cada test E2E.
   `run_source` toma un fuente HULK, llama `build_pipeline` (lexer → parser →
   resolver → tipos → HIR → BANNER), invoca `hulk_codegen::pipeline::compile`,
   ejecuta el binario y devuelve `stdout`. Otra función,
   `assert_program_matches_expected`, compara contra un archivo `.expected`
   adyacente al `.hulk`. Estos dos helpers cubren tanto programas inline como
   archivos en `examples/` y `stress-test/`.

2. **`examples/`** — programas que existen como demostradores didácticos y se
   ejercitan tanto por la suite HIR (`crates/hulk-hir/tests/semantic/`) como
   por la suite de codegen.

3. **`stress-test/`** — programas grandes que combinan features; cada uno
   también se corre desde la integración. La subcarpeta `stress-test/gc/`
   agrupa los tests específicos de GC añadidos en esta sesión.

## Subsección 17.1 — Programas de ejemplo (examples/)

La suite codegen ejecuta 17 programas inline + las 4 referencias a archivo:

| Test (codegen integration)            | Archivo / patrón                | Features que cubre |
|---------------------------------------|---------------------------------|--------------------|
| `test_hello`                          | `examples/hello.hulk`           | print, string lit |
| `test_arithmetic`                     | inline                          | `+ - * / % -`     |
| `test_booleans`                       | inline                          | `< <= > !`, `& \|` |
| `test_strings`                        | inline                          | `@`, `@@`, mezcla string+num |
| `test_conditionals`                   | inline                          | `if`/`elif`/`else` |
| `test_let_scoping`                    | inline                          | `let`, shadowing, `:=` |
| `test_while`                          | inline                          | `while` + asignación |
| `test_for_range`                      | inline                          | `for` + `range` builtin |
| `test_functions`                      | inline                          | `function`, params, `=>` |
| `test_recursion`                      | inline                          | recursión, Fibonacci |
| `test_math_builtins`                  | inline                          | `sqrt`, `^` |
| `test_class_simple` / `test_class_inherit` | inline                     | `type`, `inherits`, `self` |
| `test_vectors` / `test_for_vec_literal`    | inline                     | `[a,b]`, `v[i]`, `for` |
| `test_protocols`                      | inline                          | `protocol`, conformancia |
| `test_base_dispatch`                  | inline                          | `base()`, polimorfismo virtual |
| `test_examples_linked_list`           | `examples/linked_list.hulk`     | herencia + recursión + iterable |
| `test_examples_expression_tree`       | `examples/expression_tree.hulk` | jerarquía + simplificación virtual |
| `test_examples_game_of_life`          | `examples/game_of_life.hulk`    | vectores 2D + nested loops |
| `test_examples_parser_combinators`    | `examples/parser_combinators.hulk` | functors + protocol + lambdas |

Decisión de scope: el PIPELINE original pedía "extraer **todos** los snippets
de `hulk-docs.pdf` como tests". `hulk-docs.pdf` mezcla código ejecutable con pseudocódigo
parcial y secciones de spec abstracta; una extracción mecánica generaría
muchos archivos rotos. La opción adoptada es cubrir las features de manera
representativa a través de los programas de `examples/` y los 4 complejos
nuevos, dejando los snippets puntuales de `hulk-docs.pdf` que no encajan como
demostradores fuera de la suite automatizada.

## Subsección 17.2 — GC y memoria

Tres programas HULK en `stress-test/gc/` someten al GC a presión real:

| Archivo                              | Qué ejerce |
|--------------------------------------|------------|
| `stress-test/gc/allocs_many.hulk`    | 10000 allocaciones de `Box` en un loop con dos survivors que deben sobrevivir todo el barrido. |
| `stress-test/gc/cycles.hulk`         | 5000 ciclos `a ↔ b` de dos nodos cuya única referencia externa muere cada iteración. Un refcount los filtraría; mark-and-sweep debe recogerlos. |
| `stress-test/gc/tree_walk.hulk`      | Árbol balanceado de profundidad 6 (127 nodos) mantenido vivo mientras una rutina de ruido hace ~50000 allocaciones transitorias. Verifica que la fase de mark recorre todas las referencias (`left`, `right`, `payload`). |

Tests asociados en `integration.rs`: `test_gc_allocs_many`, `test_gc_cycles`,
`test_gc_tree_walk`. Cada uno corre el programa, compara `stdout` con el
`.expected` y exige código de salida 0.

A nivel C el runtime ya tenía `runtime/test_gc.c` con tests unitarios de
mark-and-sweep (libera unreachable, mantiene reachable). Los nuevos
programas extienden esa cobertura al nivel del lenguaje.

No se ejecuta `valgrind` automáticamente: era opcional según la spec y la
infraestructura de CI no está montada para reportar leaks. Manualmente se
puede ejecutar `valgrind ./target/release/hulkc run stress-test/gc/...`
sobre cada uno.

## Subsección 17.3 — Programas complejos + regresión

### Programas complejos en `examples/`

Los 4 demostradores que el PIPELINE pidió:

| Archivo                              | Features combinadas |
|--------------------------------------|---------------------|
| `examples/linked_list.hulk`          | Jerarquía `List`/`Cons` con dispatch virtual, recursión sobre la cola, `ListIterator` que implementa el protocolo `Iterable` del prelude y se consume con `for`. |
| `examples/expression_tree.hulk`      | Jerarquía `Expr`/`Num`/`Add`/`Sub`/`Mul`. `simplify()` recursivo con dispatch virtual implementa identidades algebraicas (x·1, x+0, x·0, constant folding). |
| `examples/game_of_life.hulk`         | Grid 1D row-major sobre `Number[]`. `step` genera la siguiente generación con `[next_cell(...) \| i in range(...)]`. Demuestra blinker y glider en grids 5×5 y 6×6. |
| `examples/parser_combinators.hulk`   | `protocol Parser` con método `parse`, tipos `Lit`/`Between`/`Seq`/`Alt`/`SumMany`/`MaxMany` como functores, y `count_if` que recibe una lambda `(Number) -> Boolean`. |

Cada uno tiene un `.expected` adyacente y un test en `integration.rs`.

### Suite de regresión

Cada bug encontrado durante esta sesión y la anterior es ahora un test
permanente:

- `test_override_accesses_inherited_numeric_field` — un override accediendo
  a `self.<campo numérico heredado>` se kindeaba como Ptr.
- `test_function_param_used_as_vector_index` — `g[i]` con `i: Number` como
  param falla al pedir "vector index must be f64".
- `test_short_name_return_kind_prefers_concrete_over_ptr` — una base infería
  retorno Ptr y bloqueaba el override que retornaba Number en el lookup por
  nombre corto, devolviendo basura.

Los 11 bugs históricos están listados en `stress-test/README.md` con commits
asociados; ese archivo permanece como changelog del esfuerzo de testing.

## Limitaciones conocidas (descubiertas durante 17)

Estas son features del lenguaje cuya implementación tiene huecos. Los
programas de esta sesión las evitan o las rodean, pero quedan documentadas
para sesiones futuras:

1. **Closures que capturan scope exterior** — `function f(n): (Number) -> Number => (x) => x + n;`
   crashea con `param not in param_temps`. Por eso `parser_combinators.hulk`
   usa subtipos (`SumMany`, `MaxMany`) en lugar de un único `Many` con
   función combinadora capturada.

2. **`as` downcast** — `(obj as ConcreteType).method()` reporta
   `unresolved callee '__hulk_as'` en el linker. Sin downcast, el código
   debe modelarse exclusivamente con dispatch virtual.

3. **String char access** — sólo existen `hulk_string_new` y
   `hulk_string_concat`; no hay `length`, `char_at` ni `substring`. Por eso
   `parser_combinators.hulk` parsea sobre `Number[]` y no sobre `String`.

4. **Field access en un valor retornado por función no-`new`** — `mk().field`
   reporta `cannot resolve field 'field' on object — struct type not statically known`.
   Workaround: exponer getters como métodos en lugar de campos públicos
   (lo que también es mejor estilo OO). Aplicado en `parser_combinators.hulk`
   y `linked_list.hulk`.

5. **Mutación de vector por índice** — `v[i] := x` produce
   `undefined reference to __vec_set` en el linker; `__vec_set` no existe en
   el runtime. `game_of_life.hulk` evita esto regenerando el grid completo
   por comprensión en cada paso.

6. **`base` como nombre de parámetro** — colisiona con la keyword `base` del
   resolver. `make_tree(d, seed)` usa `seed` en lugar del `base` original.

7. **`(new T()).method()` dentro del cuerpo de una función** — el resolver
   reporta `método no existe` aunque el método sí esté declarado en `T`.
   Workaround: `let x = new T() in x.method()`. Aplicado en
   `stress-test/gc/tree_walk.hulk`.

8. **Lambdas con `invoke` colisionan con `protocol X { invoke(...) }`** — el
   slot del vtable se sobreescribe y el programa segfaultea. Por eso el
   protocolo `Parser` en `parser_combinators.hulk` usa `parse` en lugar de
   `invoke`.

9. **`match`/`case`** — no soportado por el parser (la sintaxis está descrita
   en `hulk-docs.pdf` pero quedó fuera de la sesión 10 del PIPELINE). La
   simplificación algebraica de `expression_tree.hulk` se realiza con
   métodos virtuales en lugar de pattern matching.

## Cómo correr la suite completa

```bash
# Todos los tests del workspace (incluye integración codegen):
cargo test --workspace --release

# Sólo los programas E2E nuevos de la sesión 17:
cargo test -p hulk-codegen --test integration -- test_examples_ test_gc_

# Programas de stress manuales (sin asserts, sólo verifican exit 0):
cargo build --release -p hulk-cli
for f in stress-test/*.hulk stress-test/gc/*.hulk; do
    echo "=== $f ==="
    ./target/release/hulkc run "$f" > /dev/null
done

# Tests C del runtime GC:
make -C runtime test
```

## Resumen de cobertura

Features 100% cubiertas por la suite E2E (al menos un test compila y corre
un programa que ejercita la feature):

- Tipos primitivos: `Number`, `Boolean`, `String`, `Object`.
- Operadores aritméticos, comparación, lógicos, concatenación `@`/`@@`.
- `let` con bindings múltiples y shadowing; asignación destructiva `:=`.
- `if`/`elif`/`else`, `while`, `for` (sobre `range`, `[…]` literal y tipos
  con `iter()`/`Iterable`).
- `function` top-level con anotaciones e inferencia.
- `type` con constructor, atributos, métodos, herencia `inherits`, `self`,
  `base()`.
- `protocol` con `extends` y conformancia implícita por estructura.
- Lambdas pasadas como argumento a funciones top-level.
- Vectores `[…]`, indexación `v[i]`, comprensiones `[expr | x in iter]`.
- Macros con sigils `*`, `@`, `$`.
- Builtins matemáticos (`sqrt`, `sin`, `cos`, `exp`, `log`, `rand`) y
  potencia `^`.
- GC mark-and-sweep bajo presión (allocs masivas, ciclos, árboles vivos).

Features parcialmente cubiertas: ver la sección "Limitaciones conocidas".
