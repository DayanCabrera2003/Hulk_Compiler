# Stress Tests

Programas HULK grandes que ejercitan combinaciones extensas de la sintaxis. Cada uno se ejecuta con `hulkc run <archivo>` y produce salida determinista.

## Programas

| Archivo | Líneas | Cubre |
|---------|--------|-------|
| `01_math.hulk` | 70 | Precedencia, builtins (sqrt/sin/cos/exp/log/rand), constantes (PI/E), recursión (fact, gcd, lcm, pow), módulo, comparaciones |
| `02_oop.hulk` | 110 | Tipos, herencia (Shape→Circle/Square/Rectangle, Animal→Dog/Cat, Dog→Puppy), polimorfismo virtual (`speak`), `self`, `base()`, estado mutable (`Counter`) |
| `03_strings.hulk` | 60 | Concatenación `@`/`@@`, escapes (`\"`, `\n`, `\t`, `\\`), mezcla string+number, deep nesting, conditional strings, loops generando texto |
| `04_iterables.hulk` | 60 | `range(...)` builtin, `new Range(...)` del prelude, nested loops, `while` con asignación destructiva, primes sieve, parallel iterators |
| `05_vectors.hulk` | 50 | Literales `[1,2,3]`, indexing `v[i]`, comprehensions `[x^2 \| x in ...]`, iteración con `for`, suma con acumulador |
| `06_recursion.hulk` | 70 | Fibonacci (naive + iterativo), Ackermann, even/odd mutuos, sumTo, sumRange, fast power, GCD encadenado |
| `07_mega.hulk` | 150 | Combina todos los anteriores: tipos con jerarquía (Person→Employee→Manager), Stack mutable, primes, vector pipeline, conditional cascade |

## Cómo ejecutar

```sh
cargo build --release -p hulk-cli
for f in stress-test/*.hulk; do
    echo "=== $f ==="
    ./target/release/hulkc run "$f"
done
```

Todos los 7 stress tests deben terminar con éxito (exit 0).

## Bugs encontrados y corregidos durante el testing

Esta sesión de testing exhaustivo descubrió y corrigió **11 bugs** en el compilador:

1. **`hulkc` no encuentra `libhulkruntime.a` fuera de cargo** — fallback a `env!("OUT_DIR")` baked at build time.
2. **`emit_pow` redeclara `llvm.pow.f64` cada vez** — usar `get_function` antes de `add_function`.
3. **`hulk_log` declarado con 1 arg pero llamado con 2** — Hulk.md especifica `log(base, value)`; runtime corregido.
4. **Multi-binding `let n=42, t="abc" in print(t @ n)` segfaultea** — el inferer no registraba el tipo de la binding's symbol; ahora sí.
5. **`for x in new Range(...)` no produce salida** — colisión entre el tipo `Range` del prelude y la sentinel del builtin `range()`; sentinel renombrado a `$range`.
6. **`__vec_new()` llamado con 0 args pero declarado con 1** — desugar de comprensiones ahora pasa `0` como capacidad inicial.
7. **`gcd(48, 36)` cuelga durante codegen LLVM** — oscilación en `infer_temp_kinds` entre F64 y Ptr cuando un temp tiene múltiples definiciones; ahora prefiere F64/I1 sobre Ptr.
8. **Llamadas virtuales con args numéricos fallan en LLVM verifier** — `emit_method_call` ahora coerce cada arg a ptr antes del indirect call.
9. **Subclase no incluye los campos del padre en su layout** — `collect_fields` ahora recorre la cadena de padres y prepende sus campos.
10. **`__hulk_concat` recibe bits de float bitcasteados como ptr** — `resolve_call` detecta el callee `__hulk_concat` y convierte cada arg via `hulk_number_to_string` si es Float/Int.
11. **Parámetros de función no obtienen su tipo declarado registrado en el type env** — el resolver ahora guarda `function_param_symbols`, y el inferer expone `register_function_params_by_name` que es invocado por el driver antes de inferir el cuerpo.

## Limitaciones conocidas

- **`functors.hulk` falla** (`unresolved global '__arg_0'`): pasar una referencia a función como argumento no está implementado.
- **`vectors.hulk` falla parcialmente**: iterar un parámetro de función con tipo `Number*`/`Number[]` (`for x in xs`) requiere tracking de tipos a través de parámetros, no implementado.
- **Property test `identifier_program_always_parses` falla** (pre-existente): genera `"as"` como identificador, pero `as` es palabra reservada en HULK.
