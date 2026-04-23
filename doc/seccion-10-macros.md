# Seccion 10 - Macros

## Que se implemento

- `crates/hulk-macros/src/lib.rs`
  - Funcion publica `expand_macros(hir: Hir, bag: &mut DiagnosticBag) -> Hir`.
  - Motor de expansion para llamadas a macros por nombre.
  - Construccion de mapa `MacroParam -> argumento` por posicion.
  - Sanitizacion de variables locales con prefijo `__hulk_macro_<macro>_<counter>_<original>`.
  - Sustitucion de parametros:
    - `Regular`: reemplazo por la expresion argumento.
    - `Body` (`*`): interpolacion de la expresion argumento.
    - `Symbolic` (`@`): sustitucion por identificador.
    - `Placeholder` (`$`): creacion de `SymbolId` nuevo y sustitucion por nombre.
  - Reasignacion de `NodeId` para nodos expandidos, evitando colisiones con el AST original.
- Tests unitarios en `crates/hulk-macros/src/lib.rs`:
  - Expansion del caso `repeat(10, { print("hello") })` con chequeo de sanitizacion y sustitucion.
  - Verificacion de que una llamada normal no macro no se transforma.

## Decisiones de diseno

- Se implemento la expansion como transformacion estructural del `Program` dentro del `Hir`.
  - Alternativa considerada: usar `VisitorMut` de `hulk-ast`.
  - Decision: recursion explicita para tener control fino del punto exacto donde se reemplaza un `ExprKind::Call` completo por el cuerpo expandido.

- El lookup de macros usa un `HashMap<String, MacroDecl>` construido una sola vez.
  - Alternativa considerada: busqueda lineal en `program.macros` por cada llamada.
  - Decision: `HashMap` por simplicidad y menor costo amortizado.

- En placeholders (`$`), el `SymbolId` nuevo se reserva mediante `Resolver::define` en un scope temporal.
  - Alternativa considerada: no crear simbolo en esta fase y postergarlo.
  - Decision: crear el simbolo en 10.1 para cumplir la semantica pedida en el pipeline.

- Para anotaciones de tipo de placeholders, el mapeo inicial se hace a builtins (`Number`, `String`, `Boolean`, `Object`) y el resto cae en `Object`.
  - Esto evita introducir logica de resolucion de tipos extra en esta subsesion.

## Gotchas

- La expansion de macros puede introducir nuevas llamadas a macro dentro del cuerpo sustituido.
  - Solucion: despues de sustituir, se vuelve a recorrer recursivamente el cuerpo expandido.

- La captura accidental de variables locales del macro es facil de introducir en `let`, `for`, `lambda` y generadores de vector.
  - Solucion: sanitizacion por scopes lexicos para nombres declarados localmente y sus usos.

- Los `NodeId` del cuerpo original del macro no pueden reutilizarse en cada invocacion.
  - Solucion: regenerar todos los `NodeId` del subarbol expandido con `NodeIdGen` iniciado despues del maximo id del programa.

## Ejemplos de uso

```rust
use hulk_diagnostics::DiagnosticBag;
use hulk_hir::Hir;
use hulk_macros::expand_macros;

fn run_middleend_step(hir: Hir) -> (Hir, DiagnosticBag) {
    let mut bag = DiagnosticBag::new();
    let expanded = expand_macros(hir, &mut bag);
    (expanded, bag)
}
```

Ejemplo conceptual de expansion:

```text
def repeat(n: Number, *expr: Object) =>
    let total = n in { expr; };

repeat(10, { print("hello"); });
```

Se transforma a un cuerpo equivalente donde:

- `n` se reemplaza por `10`.
- `expr` se reemplaza por `{ print("hello"); }`.
- `total` se renombra a `__hulk_macro_repeat_0_total`.