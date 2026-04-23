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

- En placeholders (`$`), el `SymbolId` nuevo se reserva mediante `Resolver::allocate_symbol`, que inserta directamente en la `SymbolTable` sin usar el scope stack.
  - Alternativa considerada: `push_scope` + `define` + `pop_scope` (usado originalmente).
  - Problema detectado: al popear el scope, el binding quedaba huerfano y ninguna fase posterior podia resolver el identificador.
  - Decision: usar `allocate_symbol` para crear el simbolo y `record_expr_symbol` para registrar `NodeId -> SymbolId` en `expr_symbols` despues de `refresh_node_ids`, garantizando que todos los `Ident` con el nombre del placeholder (incluidos los que vienen dentro de un `*body`) resuelvan al simbolo fresco.

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

## Extension 10.2 - Pattern matching en macros

### Que se implemento

- Evaluacion de pattern matching en tiempo de compilacion dentro de `expand_macros`.
- Motor de casos con rebinding para reemplazar variables capturadas en el body de cada `case`.
- Soporte MVP de patrones:
  - Literal exacto (`Number`, `String`, `Boolean`).
  - Variable tipada (`x:Type`) para `Number`, `String`, `Boolean`, `Object`.
  - BinOp tipado (incluye `+` para MVP y operadores aritmeticos/comparacion soportados por la representacion interna).
  - `default`.

### Representacion interna usada

Debido a que el parser actual aun no construye nodos AST para la sintaxis `match/case` de Hulk.md, esta subsesion usa una representacion interna temporal en el cuerpo de macros:

- `__hulk_match(subject, case..., default)`
- `__hulk_case_lit(pattern_literal, body)`
- `__hulk_case_var(var_ident, "Type", body)`
- `__hulk_case_binop("+", left_ident, "Type", right_ident, "Type", body)`
- `__hulk_case_binop_right_lit("*", left_ident, "Type", literal, body)`
- `__hulk_default(body)`

Esta representacion permite validar la semantica de matching en `hulk-macros` sin adelantar cambios de parser fuera del alcance de 10.2.

### Desviacion conocida respecto al pipeline

El pipeline pide "evaluacion de `match(expr) { case ... }` en tiempo de compilacion" sobre nodos reales. La implementacion actual usa codificacion por intrinsics (`__hulk_match`, `__hulk_case_*`) y solo dispara cuando `ExprKind::Call { callee: Ident("__hulk_match"), ... }`. Consecuencia: el flujo real `source -> parser -> HIR -> expander` todavia no ejercita pattern matching, solo tests que construyen el AST a mano.

**Deferral**: promover `ExprKind::Match { subject, cases, default }` al AST/HIR queda pendiente para una sesion posterior (candidato: durante o despues de 11 Desugaring). Cuando se agregue el nodo, el expander puede consumirlo directamente y los intrinsics quedarian como fallback o se eliminarian.

### Verificacion agregada

- Test de ejemplo `simplify((42+0)*1)` construido en AST dentro de `crates/hulk-macros/src/lib.rs`.
- Resultado esperado verificado: expansion final a `42`.

### Decisiones y limitaciones del MVP

- El parser aun no produce nodos sintacticos dedicados para `match/case`.
  - Para no adelantar trabajo de 10.2 fuera de alcance, el motor usa una codificacion interna con llamadas especiales `__hulk_match` y `__hulk_case_*`.

- El matching soporta literales, variables tipadas y binops de la forma usada por `simplify`.
  - Se priorizo cubrir el ejemplo canónico de Hulk.md antes que modelar todas las formas posibles de patrones.

- La simplificacion aritmetica neutra se aplico como post-proceso local.
  - Esto permite que `simplify((42+0)*1)` termine en `42` aun cuando el primer caso de suma genere una expresion equivalente pero no final.

## Cierre 10.3 - Tests + casos edge

### Cobertura agregada

- Tests nuevos en `crates/hulk-macros/src/lib.rs` para los casos requeridos por 10.3:
  - `swap(@x, @y)`: verificacion de sustitucion simbolica en objetivos de asignacion y valores.
  - `repeat($iter, 10) { print(iter); }`: verificacion de placeholder con registro de `SymbolId` y tipo `Number` en `TypeEnv`.
  - Sanitizacion con colision de nombres (`total` en contexto externo y dentro de macro): se verifica no captura accidental.
  - Errores por parametros mal pasados: cantidad invalida de argumentos y placeholder no-identificador.

### Casos edge encontrados

- Si una macro falla por argumentos invalidos, la expansion retorna el nodo original (fallback) y reporta diagnostico.
  - Esto evita destruir estructura del AST/HIR cuando hay errores recuperables.

- Los parametros simbolicos (`@`) solo aceptan identificadores como argumento.
  - Pasar literales o expresiones compuestas reporta error semantico en esta fase.

- Los placeholders (`$`) requieren identificador y ademas registran tipo en `TypeEnv`.
  - Si el tipo anotado no es builtin conocido en esta fase, cae conservadoramente en `Object`.

- La sanitizacion local debe respetar scope externo.
  - El prefijo `__hulk_macro_<macro>_<id>_` evita interferencia incluso cuando el nombre externo coincide exactamente.

- Los placeholders (`$`) deliberadamente sombrean el scope del caller.
  - Si el caller tiene un binding con el mismo nombre (`let iter = ... in repeat($iter, ...)`), el `SymbolId` alocado para el placeholder es distinto del caller y todos los `Ident` con ese nombre en el cuerpo expandido apuntan al simbolo nuevo via `expr_symbols`.
  - Regresion cubierta por `placeholder_does_not_reuse_caller_scope_symbol`.

- El parametro `*body` exige un argumento de tipo `ExprKind::Block`.
  - Pasar un literal o expresion simple como argumento `*body` emite diagnostico `el parametro de cuerpo '<name>' requiere un bloque` y devuelve el nodo original como fallback.

- `simplify_algebraic` ya no se ejecuta como post-proceso de toda expansion de macro.
  - Originalmente corria dentro de `expand_macro_call` y reescribia silenciosamente `x + 0`, `x * 1`, etc. en cualquier macro.
  - Ahora solo aplica dentro de la rama `default` del pattern matching, donde es el contrato esperado.
  - Las reducciones algebraicas deben expresarse como cases de patron especificos ordenados antes que los generales.