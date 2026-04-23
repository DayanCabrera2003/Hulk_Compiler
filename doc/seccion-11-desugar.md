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

- No se introdujeron cambios de semantica fuera de 11.1.
  - No se implementaron transformaciones de lambdas/functores ni de generadores de vector (corresponden a 11.2 y 11.3).

## Gotchas

- La forma enumerable correcta requiere dos `let` anidados (`__enum` y luego `__it`).
  - Una forma intermedia incorrecta (`let __it = (let __enum = ... in __enum.iter())`) fue descartada para respetar la transpilacion esperada.

- `@@` se desazucara preservando asociatividad izquierda en la forma final:
  - `(a @ " ") @ b`.

- Como la inferencia de protocolos en etapas previas es parcial, el desugar mantiene fallback seguro a rama iterable cuando no hay evidencia de `Enumerable`.

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
