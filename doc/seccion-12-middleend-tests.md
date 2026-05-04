# Sección 12 — Testing exhaustivo del middleend

## Objetivo

Verificar con tests integración y tests de propiedades que los dos pases del middleend (`hulk-macros` y `hulk-desugar`) producen HIR correcto, equivalente semánticamente a la entrada, libre de nodos azucarados, y sin colisiones de nombres internos.

---

## 12.1 Tests de expansión correcta

### hulk-desugar — `crates/hulk-desugar/tests/combined.rs`

Cada test construye un HIR con combinaciones de transformaciones y verifica que el resultado no contiene ningún nodo azucarado.

| Test | Descripción |
|------|-------------|
| `for_inside_lambda_body_both_lowered` | Un `for` dentro del cuerpo de un `Lambda` — ambos nodos deben desaparecer. El `Lambda` se convierte en `New(SyntheticType)` y el `for` en `let+while`. |
| `concat_spaced_inside_for_body_both_lowered` | Un `@@` en el cuerpo de un `for`. Ambos se desazucaran: el `for` a `let+while` y `@@` a `@ " " @`. |
| `concat_spaced_element_in_vec_generator_both_lowered` | El elemento de un `VecGenerator` es un `@@`. El generador se baja a `let __vec = __vec_new()...` y el `@@` a `@ " " @`. |
| `all_sugar_constructs_eliminated_together` | Un `Block` con un `for`, un `@@`, un `Lambda` y un `VecGenerator`. Ninguno debe sobrevivir en el HIR resultante. |
| `synthetic_node_ids_are_unique_after_desugaring` | El pase produce nodos frescos; sus IDs no pueden repetirse en el árbol resultante. |

### hulk-macros — `crates/hulk-macros/tests/combined.rs`

| Test | Descripción |
|------|-------------|
| `regular_param_substitution_with_lambda_argument` | Macro `wrap(f) => f` llamada con una lambda. El cuerpo expandido debe ser la lambda, sin intermediario. |
| `two_macros_with_same_local_names_produce_distinct_sanitized_idents` | Dos macros con local `count`. Después de expandir ambas, ningún `count` pelado debe quedar en el output, y cada macro debe producir idents con su propio prefijo. |
| `body_param_with_multi_statement_block_preserves_all_statements` | Macro con parámetro `*expr` llamada con un bloque de tres llamadas a `print`. Las tres deben aparecer en el resultado. |

---

## 12.2 Tests de equivalencia semántica

### `crates/hulk-desugar/tests/equivalence.rs`

Cada test construye dos versiones del mismo programa: una usando el constructor de alto nivel (sintaxis azucarada) y otra escrita manualmente en la forma desazucarada equivalente. Se comparan estructuralmente usando `shape_eq`, que ignora `NodeId` y `Span` para enfocarse en la forma del árbol.

| Test | Equivalencia verificada |
|------|------------------------|
| `concat_spaced_desugar_equals_manually_written_form` | `a @@ b` → `(a @ " ") @ b` |
| `for_loop_desugar_produces_correct_let_while_skeleton` | `for (x in xs) body` → esqueleto `let __iter = xs.__iter() in let __val = __iter.__next() in while __val.__has_value() { ... }` |
| `vec_generator_desugar_produces_correct_let_new_block_shape` | `[e \| x in xs]` → `let __vec = __vec_new() in { for ...; __vec }` |

**Decisión de diseño**: la comparación normaliza solo `NodeId` y `Span`; los nombres de variables temporales se comparan tal como son. Esto detecta regresiones en los nombres de los temporales.

---

## 12.3 Property tests y métricas

### hulk-desugar — `crates/hulk-desugar/tests/property/mod.rs`

Configuración: 128 casos por propiedad con `proptest`.

| Propiedad | Entradas generadas | Invariante |
|-----------|-------------------|-----------|
| `desugar_eliminates_for_nodes` | binding y iterable como `[a-z]{1,6}` | Ningún nodo `For` en el output |
| `desugar_eliminates_concat_spaced` | dos `f64` arbitrarios | Ningún `BinOp::ConcatSpaced` en el output |
| `desugar_eliminates_vec_generator` | binding `[a-z]{1,6}`, body `f64` | Ningún `VecGenerator` en el output |
| `desugar_eliminates_lambda` | param `[a-z]{1,6}`, body `f64` | Ningún `Lambda` y exactamente un tipo sintético |
| `desugar_is_idempotent_for_lambdas` | igual que arriba | Segunda pasada no produce tipos sintéticos nuevos |
| `desugar_is_idempotent_for_for_loops` | binding e iterable | Segunda pasada no cambia conteo de tipos |

Tests deterministas adicionales:
- `desugar_handles_three_nested_for_loops_without_panic` — tres `for` anidados, ningún nodo azucarado sobrevive.
- `two_lambdas_produce_distinct_synthetic_type_names` — dos lambdas en bloque, los nombres de sus tipos sintéticos son distintos.

### hulk-macros — `crates/hulk-macros/tests/property/mod.rs`

Configuración: 128 casos por propiedad con `proptest`.

| Propiedad | Entradas generadas | Invariante |
|-----------|-------------------|-----------|
| `expansion_sanitizes_local_bindings` | nombre macro, param y local como `[a-z]{4,8}` | Ningún ident pelado con el nombre del local |
| `sanitized_idents_carry_macro_name_prefix` | ídem | Al menos un ident con prefijo `__hulk_macro_<macro>_` |
| `two_macros_same_local_name_produce_non_overlapping_sanitized_idents` | dos nombres de macro distintos, mismo local | Los idents de cada macro no cruzan el prefijo del otro |
| `expansion_never_panics` | cualquier `f64` como argumento | El pase termina sin panic |

Tests deterministas adicionales:
- `macro_with_no_locals_introduces_no_sanitized_idents` — una macro `identity(x) => x` no debe generar ningún ident con prefijo `__hulk_macro_`.
- `repeated_expansion_produces_distinct_idents_per_call` — diez llamadas a la misma macro producen diez idents sanitizados distintos.

---

## Métricas finales (sesión 12)

| Crate | Archivo de tests | Tests unitarios | Proptest casos |
|-------|-----------------|----------------|----------------|
| hulk-desugar | tests/combined.rs | 5 | — |
| hulk-desugar | tests/equivalence.rs | 3 | — |
| hulk-desugar | tests/property/mod.rs | 2 + 6×128 | 768 |
| hulk-macros | tests/combined.rs | 3 | — |
| hulk-macros | tests/property/mod.rs | 2 + 4×128 | 512 |
| **Total** | | **15** | **1280** |

Todos los tests pasan con `cargo test --workspace`.

---

## Decisiones de diseño

- **Helpers compartidos via `#[path]`**: cada binario de tests compila `support/mod.rs` de forma independiente. Se usa `#![allow(dead_code)]` porque no todos los binarios usan todos los helpers.
- **Comparación estructural sin PartialEq**: `shape_eq` se implementa recursivamente sobre `ExprKind` en lugar de derivar `PartialEq` en el HIR, ya que `NodeId` y `Span` siempre difieren entre dos construcciones independientes.
- **Idempotencia por conteo de tipos**: en lugar de comparar árboles completos (que requeriría `PartialEq` en todo el HIR), la idempotencia se verifica comprobando que la segunda pasada no introduce tipos sintéticos nuevos y no deja nodos azucarados.
- **`prop_assume!` para evitar colisiones triviales**: en los tests de macros se asume que los nombres generados son distintos entre sí para que la propiedad sea significativa (si `param == local`, el test verifica algo trivial).
