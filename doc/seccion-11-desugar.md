# Seccion 11 - Desugar

## Que se implemento

- Archivo: crates/hulk-desugar/src/lib.rs
  - Funcion publica `desugar(hir: Hir, bag: &mut DiagnosticBag) -> Hir`.
  - Reescritura de `for (x in expr) body` a forma explicita con `let` + `while` + llamadas a protocolo:
    - Caso iterable:
      - `let __it_N = expr in while (__it_N.next()) let x = __it_N.current() in body`
    - Caso enumerable:
      - `let __enum_N = expr in let __it_M = __enum_N.iter() in while (__it_M.next()) let x = __it_M.current() in body`
  - Reescritura de `a @@ b` a `a @ " " @ b` usando `BinOpKind::Concat`.
  - Generacion de nombres frescos para temporales (`__it_N`, `__enum_N`) con contador monotono.
  - Conservacion del `NodeId` del nodo raiz transformado y asignacion de `NodeId` nuevos para nodos introducidos por el desugaring.

- Tests unitarios agregados en crates/hulk-desugar/src/lib.rs:
  - `desugars_concat_spaced_into_two_concat_ops`
  - `desugars_for_with_iterable_to_let_while_shape`
  - `desugars_for_with_enumerable_to_enum_iter_then_while`

## Decisiones de diseno

- Se implemento un transformador estructural recursivo sobre expresiones, sin mutar el HIR original.
  - Alternativa: mutar nodos in-place.
  - Decision: devolver un HIR nuevo para mantener inmutabilidad por defecto en middleend.

- Deteccion de rama `Enumerable` basada en el `TypeKind` del tipo inferido de la expresion iterable.
  - Se considera enumerable cuando el `TypeKind` es `Protocol { name: "Enumerable" }` o `UserDefined { name: "Enumerable", .. }`.
  - En otro caso se aplica la rama iterable por defecto.

- Para 11.2 se eligio representar lambdas y wrappers como tipos sinteticos concretos con metodo `invoke`.
  - Esto permite reutilizar el mismo mecanismo de llamada de metodos en middleend.
  - Los nombres generados siguen el esquema `__LambdaN` y `__Wrapper<Function>N`.

- El wrapping de funciones como funtores usa la firma de la funcion original cuando esta disponible en `Program.functions`.
  - Si no hay firma accesible, se genera `invoke` sin parametros como fallback seguro.

## Gotchas

- La forma enumerable correcta requiere dos `let` anidados (`__enum` y luego `__it`).
  - Una forma intermedia incorrecta (`let __it = (let __enum = ... in __enum.iter())`) fue descartada para respetar la transpilacion esperada.

- `@@` se desazucara preservando asociatividad izquierda en la forma final:
  - `(a @ " ") @ b`.

- Como la inferencia de protocolos en etapas previas es parcial, el desugar mantiene fallback seguro a rama iterable cuando no hay evidencia de `Enumerable`.

- El tipo functor aun no tiene una API publica completa en `TypeEnv` para sintetizar `TypeKind::Functor` en tests de integracion de desugar.
  - En 11.2 la deteccion de llamada functor-style se apoya en simbolos (`Variable`/`Parameter`) y forma del AST.
  - Cuando avance el typer, esta heuristica se puede refinar con tipos functor reales.

## Ejemplos de uso

```rust
use hulk_desugar::desugar;
use hulk_diagnostics::DiagnosticBag;
use hulk_hir::Hir;

fn run_desugar(hir: Hir) -> (Hir, DiagnosticBag) {
    let mut bag = DiagnosticBag::new();
    let lowered = desugar(hir, &mut bag);
    (lowered, bag)
}
```

Ejemplo conceptual de transformacion:

```text
for (x in numbers) print(x)
```

se transforma a:

```text
let __it_0 = numbers in
  while (__it_0.next())
    let x = __it_0.current() in print(x)
```

y:

```text
a @@ b
```

se transforma a:

```text
(a @ " ") @ b
```

## Extension 11.3 - Vector generators

### Que se implemento

- Archivo nuevo: crates/hulk-desugar/src/transforms/vec_generator.rs
  - Metodo `desugar_vec_generator` en `Desugarer`.
  - Transforma `[element | binding in iterable]` en:
    ```
    let __vec_N = __vec_new() in {
        for (binding in iterable) __vec_push(__vec_N, element);
        __vec_N
    }
    ```
  - El `for` interno se delega a `desugar_for`, de modo que el HIR resultante no contiene ningun nodo `For`.
  - Los nombres `__vec_new` y `__vec_push` se tratan como builtins del runtime (se implementan en sesion 14).

- Actualizaciones en crates/hulk-desugar/src/lib.rs:
  - Arm `VecGenerator` en `desugar_expr` reemplazado: desugar interno de `element` e `iterable`, luego llamada a `desugar_vec_generator`.

- Actualizaciones en crates/hulk-desugar/src/transforms/mod.rs:
  - Modulo `vec_generator` declarado.

- Tests agregados en crates/hulk-desugar/src/tests/vec_generator.rs:
  - `desugars_vec_generator_into_let_vec_new_block_shape`: verifica forma exterior `let __vec_N = __vec_new() in { ...; __vec_N }`.
  - `desugars_vec_generator_push_call_receives_element`: verifica que `__vec_push` recibe el elemento original.
  - `desugars_vec_generator_for_body_is_already_lowered`: verifica que ningun nodo `For` queda en el HIR resultante.
  - `desugars_for_loop_containing_vec_generator_in_body`: test combinado — for externo con generador de vector en el body; ambas transformaciones se aplican correctamente.

### Decisiones de diseno

- Se delega el for interno a `desugar_for` en lugar de construir el `let + while` manualmente.
  - Esto garantiza que la estrategia iterable/enumerable se aplica de igual forma que para los `for` del usuario.
  - Alternativa descartada: construir el `let + while` directamente — duplicaria logica y podria divergir de la estrategia de `desugar_for`.

- El `id` original del nodo `VecGenerator` se preserva en el nodo `Let` externo, igual que hace `desugar_for` con los nodos `For`.

- Los nombres de builtin `__vec_new` y `__vec_push` siguen el esquema de prefijo `__` del proyecto para distinguir nombres sinteticos.

### Gotchas

- El arm `VecGenerator` en `desugar_expr` anteriormente solo recursaba las sub-expresiones sin transformar el nodo. Esto era un stub incompleto de 11.1 que se corrigio en esta subsesion.

### Ejemplos de uso

Entrada:

```text
[x * 2 | x in numbers]
```

Forma desazucarada (con for interno ya lowereado):

```text
let __vec_0 = __vec_new() in {
    let __it_0 = numbers in
        while (__it_0.next())
            let x = __it_0.current() in __vec_push(__vec_0, x * 2);
    __vec_0
}
```

---

## Extension 11.2 - Lambdas y functores

### Que se implemento

- `Lambda { params, return_type, body }` se transforma a:
  - `type __LambdaN { invoke(params): return_type => body; }`
  - reemplazo del nodo lambda por `new __LambdaN()`.
- Funciones pasadas como valor (por ejemplo `apply(inc, 1)`) se envuelven en tipos sinteticos:
  - `type __WrapperincN { invoke(__arg_0, ...) => inc(__arg_0, ...); }`
  - reemplazo del argumento funcion por `new __WrapperincN()`.
- Llamadas estilo functor `filter(x)` cuando `filter` es valor invocable se reescriben a `filter.invoke(x)`.
- Registro de tipos sinteticos en `TypeEnv` con parent `Object`.

### Tests agregados

- `lowers_lambda_into_synthetic_type_and_new`
- `wraps_function_arguments_with_synthetic_wrapper_type`
- `rewrites_functor_style_call_to_invoke_method_call`

### Ejemplo conceptual

Entrada:

```text
let f = (x: Number) => x + 1 in f(10)
```

Forma desazucarada:

```text
type __Lambda0 {
  invoke(x: Number): Number => x + 1;
}

let f = new __Lambda0() in f.invoke(10)
```
