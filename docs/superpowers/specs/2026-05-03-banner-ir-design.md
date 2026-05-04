# BANNER IR — Diseño de sesión 13

**Fecha**: 2026-05-03  
**Crate**: `hulk-banner`  
**Sesiones**: 13.1 · 13.2 · 13.3

---

## Contexto

BANNER (Basic 3-Address liNear iNtErmediate Representation) es el IR de 3 direcciones del compilador HULK. Recibe el HIR ya desazucarado (sin `For`, `Lambda`, `VecGenerator`, `@@`) y lo convierte en una secuencia plana de instrucciones con temporales explícitos, etiquetas y saltos.

El crate `hulk-banner` es la primera capa del backend. Su único consumidor previsto es `hulk-codegen` (sesión 15), que traduce BANNER a LLVM IR.

---

## Estructura de módulos (Opción B)

```
crates/hulk-banner/src/
  lib.rs        ← pub fn lower_program(hir: &Hir) -> BannerProgram
  ir.rs         ← todos los tipos de datos del IR
  print.rs      ← Display impls (pretty-printer)
  lowerer.rs    ← Lowerer struct + emit_expr + lower_function
```

---

## Sección 1 — Tipos de datos (`ir.rs`)

### TempId

```rust
pub struct TempId(pub u32);
```

Identificador opaco de variable temporal. El lowerer los asigna en orden creciente con `fresh_temp()`.

### Value

Operandos que pueden aparecer en cualquier posición de una instrucción.

```rust
pub enum Value {
    Temp(TempId),
    ConstNum(f64),
    ConstStr(String),
    ConstBool(bool),
    ConstNull,
    Global(String),  // nombre de función global o tipo (para Call/New)
}
```

### Instr

Instrucción de 3 direcciones. Un `BannerFunction` es una secuencia de `Instr`.

```rust
pub enum Instr {
    // asignación directa
    Copy       { dst: TempId, src: Value },

    // operaciones aritméticas/lógicas
    BinOp      { dst: TempId, op: BinOpKind, left: Value, right: Value },
    UnOp       { dst: TempId, op: UnaryOpKind, operand: Value },

    // llamadas
    Call       { dst: TempId, callee: Value, args: Vec<Value> },
    MethodCall { dst: TempId, receiver: Value, method: String, args: Vec<Value> },
    StaticCall { dst: TempId, type_name: String, method: String, args: Vec<Value> },

    // objetos
    New        { dst: TempId, type_name: String, args: Vec<Value> },
    GetField   { dst: TempId, object: Value, field: String },
    SetField   { object: Value, field: String, value: Value },

    // vectores
    GetIndex   { dst: TempId, target: Value, index: Value },
    SetIndex   { target: Value, index: Value, value: Value },

    // control de flujo
    Label      (String),
    Jump       (String),
    JumpIf     { condition: Value, label: String },
    Return     (Value),

    // GC shadow stack
    ShadowPush (Value),   // empuja referencia al shadow stack
    ShadowPop,            // saca una referencia del shadow stack

    // reservado; el lowerer nunca lo emite
    Alloc      { dst: TempId, type_name: String },
}
```

`BinOpKind` y `UnaryOpKind` se reusan de `hulk_hir` (re-exportados de `hulk_ast`).

`StaticCall` se usa para `base()` — llamada estática al método del padre con `self` explícito.

`Alloc` está reservada para que codegen descomponga `New` en alocación + construcción si lo necesita. El lowerer de sesión 13 nunca la emite.

### BannerFunction

```rust
pub struct BannerFunction {
    pub name: String,
    pub params: Vec<TempId>,      // TempId asignado a cada parámetro
    pub param_names: Vec<String>, // nombre original (para pretty-printer y debug)
    pub body: Vec<Instr>,
}
```

### TypeDescriptor

Describe un tipo usuario para el GC. El `pointer_map` indica qué campos son referencias (heap-managed) y cuáles son valores primitivos (stack/valor inmediato).

```rust
pub struct TypeDescriptor {
    pub name: String,
    pub parent: Option<String>,
    pub fields: Vec<String>,
    pub pointer_map: Vec<bool>, // parallel a fields: true = referencia, false = primitivo
    pub methods: Vec<BannerFunction>,
}
```

Regla de `pointer_map`: un campo es primitivo si y solo si su tipo inferido es `TypeId::NUMBER` o `TypeId::BOOLEAN`. Todo lo demás (String, Object, tipos usuario, protocolos) es referencia.

### BannerProgram

```rust
pub struct BannerProgram {
    pub types: Vec<TypeDescriptor>,
    pub functions: Vec<BannerFunction>,
    pub main: BannerFunction,
}
```

---

## Sección 2 — Pretty-printer (`print.rs`)

Implementa `std::fmt::Display` para todos los tipos. El formato sirve para tests de snapshot y para inspección manual.

**Formato:**

```
type Point {
  parent: none
  fields: [x (ptr), y (ptr)]
  fn Point.__init__(t0 /* self */, t1 /* x */, t2 /* y */) {
      setfield t0.x = t1
      setfield t0.y = t2
      return t0
  }
}

fn fib(t0 /* n */) {
    t1 = t0 <= 1.0
    jumpif t1 then_0
    t2 = t0 - 1.0
    t3 = call fib(t2)
    t4 = t0 - 2.0
    t5 = call fib(t4)
    t6 = t3 + t5
    jump end_0
  then_0:
    t6 = copy t0
  end_0:
    return t6
}

fn __main__() {
    ...
}
```

Reglas:
- Instrucciones indentadas 4 espacios; `LabelString:` sin indentación (2 espacios).
- `TempId(n)` → `t{n}`.
- `ConstStr(s)` → `"s"` con escape de `\n`, `\t`, `\"`, `\\`.
- Secciones en orden: `type` descriptors, luego `fn` globales, luego `fn __main__`.

---

## Sección 3 — Lowerer (`lowerer.rs`)

### Struct

```rust
pub struct Lowerer<'h> {
    hir: &'h Hir,
    instrs: Vec<Instr>,
    next_temp: u32,
    next_label: u32,
    locals: HashMap<SymbolId, TempId>,    // SymbolKind::Variable let bindings
    param_temps: HashMap<String, TempId>, // SymbolKind::Parameter: name → TempId
    shadow_count: usize,       // refs pushed in the current Let scope; saved/restored per Let
    self_temp: Option<TempId>, // TempId asignado al param self del método actual
    current_type_name: Option<String>,
    current_parent_type_name: Option<String>,
    current_method_name: Option<String>,
}
```

`shadow_count`, `param_temps`, `locals` y `self_temp` se reinician al inicio de cada función/método. `instrs` también se vacía. `next_temp` y `next_label` deben producir valores únicos dentro de cada `BannerFunction` (pueden ser globales o reiniciarse por función).

**Separación `locals` vs `param_temps`:** `hulk_ast::Param` no tiene `node_id`, por lo que `hir.resolved_symbol(param.node_id)` no está disponible para params. Los params se indexan por nombre (único dentro de una función). Los `LetBinding` sí tienen `NodeId` y su `SymbolId` se obtiene vía la extensión del resolver descrita en §3.5.

### API pública (`lib.rs`)

```rust
pub fn lower_program(hir: &Hir) -> BannerProgram
```

### Función principal: `emit_expr(expr: &Expr) -> Value`

Mapa `ExprKind` → emisión de instrucciones:

| ExprKind | Acción |
|---|---|
| `Number(v)` | retorna `ConstNum(v)` directamente |
| `StringLit(s)` | retorna `ConstStr(s)` directamente |
| `Bool(b)` | retorna `ConstBool(b)` directamente |
| `Ident` | obtener `sym = hir.resolved_symbol(ident.id).unwrap()` y `kind = hir.symbols.table().get(sym).kind`; `Variable` → `Temp(locals[sym])`; `Parameter` → `Temp(param_temps[hir.symbols.table().name_of(sym).unwrap()])`; `SelfValue` → `Temp(self_temp.unwrap())`; `Function`/`BuiltinFunction`/`Macro` → `Global(name)`; `BuiltinValue` → `ConstNum` (`"PI"` → `PI`, `"E"` → `E`, vía `name_of`); `BuiltinType` no aparece como valor |
| `Self_` | `Temp(self_temp.unwrap())` |
| `Base` | ver §3.3 — emite `StaticCall` cuando aparece como callee; standalone retorna `Temp(self_temp.unwrap())` |
| `BinOp` | emite left, right → `Instr::BinOp { dst: fresh() }`; excepción: `BinOpKind::Concat` (`@`) → `Call { callee: Global("__hulk_concat"), args: [left, right] }` (requiere heap) |
| `UnaryOp` | emite operand → `Instr::UnOp { dst: fresh() }` |
| `Call` | emite callee + args → `Instr::Call { dst: fresh() }`; si callee es `Base`: ver §3.3 |
| `MethodCall` | emite receiver + args → `Instr::MethodCall { dst: fresh() }` |
| `New { type_ann, args }` | `type_ann` es siempre `TypeAnn::Named(name)` tras el análisis semántico; extrae `name` con `if let TypeAnn::Named(name) = type_ann`; emite args → `New { dst: fresh(), type_name: name, args }` |
| `Block(es)` | emite cada `e`; retorna valor del último; si `es` está vacío retorna `ConstNull` |
| `Let { bindings, body }` | emite bindings, body, luego `ShadowPop` × refs |
| `LetBinding(lb)` | emite value → `fresh_temp`; `ShadowPush` si tipo ref |
| `Assign { target, value }` | emite value; target descompuesto según §3.4: `Copy`, `SetField`, o `SetIndex` |
| `AssignTarget` | nodo auxiliar; solo aparece envuelto en `Assign`; procesado por §3.4 |
| `FieldAccess` | `Instr::GetField { dst: fresh() }` |
| `Index` | `Instr::GetIndex { dst: fresh() }` |
| `If { condition, then_branch, elif_branches, else_branch }` | ver §3.3: JumpIf + Labels por cada rama elif; sin `else_branch` retorna `ConstNull` |
| `While` | label de loop + JumpIf de salida; retorna `ConstNull` |
| `VecLiteral(es)` | `n = es.len() as f64`; `t = call Global("__vec_new")(ConstNum(n))`; por cada elem: `call Global("__vec_push")(t, elem_val)`; retorna `Temp(t)` |
| `Is { expr, type_ann }` | emite expr → `t`; `dst = call Global("__hulk_is")(t, Global(type_name))`; retorna `Temp(dst)` |
| `As { expr, type_ann }` | emite expr → `t`; `dst = call Global("__hulk_as")(t, Global(type_name))`; retorna `Temp(dst)` |

### Control de flujo — `if/elif/else`

El resultado de la expresión `if` se almacena en un `TempId` de resultado común (`t_res`) que se escribe antes de cada `jump end_N`. Si `else_branch` es `None`, la rama de fallthrough asigna `Copy { dst: t_res, src: ConstNull }`.

Cada entrada de `elif_branches` genera su propio par `(t_cXX, then_elif_XX)`. El orden de emisión es: condición principal → condiciones elif en orden → fallthrough (else o ConstNull). Todos saltan a `end_N`.

```
    t_res = (fresh TempId para el resultado)
    t_cond = <condición principal>
    jumpif t_cond then_N
    t_c0 = <elif_branches[0].cond>
    jumpif t_c0 then_elif_0
    [t_c1 = <elif_branches[1].cond>; jumpif t_c1 then_elif_1; ...]
    <else branch body, o Copy t_res = ConstNull si no hay else>
    jump end_N
  then_elif_0:
    t_res = <elif_branches[0].body>
    jump end_N
  [then_elif_1: ...]
  then_N:
    t_res = <then_branch body>
  end_N:
    (t_res tiene el valor del if)
```

Si `elif_branches` está vacío, los pasos intermedios se omiten.

La rama `then_N:` es la última antes de `end_N:`, por lo que cae directamente hacia `end_N:` — no requiere un `jump end_N` explícito. Todas las demás ramas (else y cada `then_elif_K:`) sí necesitan `jump end_N` porque están antes de `then_N:` en la secuencia lineal.

**Ejemplo con N=2 elif branches:**
```
    t_res = fresh
    t_cond = <cond>
    jumpif t_cond then_0
    t_c0 = <elif[0].cond>
    jumpif t_c0 then_elif_0
    t_c1 = <elif[1].cond>
    jumpif t_c1 then_elif_1
    t_res = copy <else body o ConstNull>
    jump end_0
  then_elif_0:
    t_res = <elif[0].body>
    jump end_0
  then_elif_1:
    t_res = <elif[1].body>
    jump end_0
  then_0:
    t_res = <then body>   ← fall-through a end_0, sin jump explícito
  end_0:
```

La regla general: los `then_elif_K:` se emiten en orden creciente de K, cada uno terminando con `jump end_N`. `then_N:` siempre va último antes de `end_N:`.

### Control de flujo — `while`

```
  loop_N:
    t_cond = <condición>
    t_neg = not t_cond
    jumpif t_neg end_N
    <body>
    jump loop_N
  end_N:
```

El `while` no produce un valor útil; retorna `ConstNull`.

### §3.3 Lowering de `Base`

`Base` en HULK solo aparece como callee de `Call { callee: Base, args }` (sintaxis `base()`). El lowerer detecta este patrón antes de procesar el `Call` genérico y emite:

```rust
Instr::StaticCall {
    dst: fresh_temp(),
    type_name: current_parent_type_name.clone().unwrap(),
    method: current_method_name.clone().unwrap(),
    args: [Value::Temp(self_temp.unwrap())]
          .into_iter().chain(lowered_args).collect(),
}
```

Si `Base` aparece en cualquier otro contexto (standalone, receiver de MethodCall con nombre explícito), el lowerer retorna `Value::Temp(self_temp.unwrap())` como valor conservador.

### §3.4 Descomposición de `AssignTarget`

`Assign { target: Expr, value: Expr }` — el lowerer primero emite `value` para obtener `val: Value`, luego descompone el target:

| AssignTarget variant | Instrucción emitida |
|---|---|
| `Ident(name)` | `Copy { dst: locals[sym_of_name], src: val }` |
| `Field { receiver, field }` | emite receiver → `obj`; `SetField { object: obj, field, value: val }` |
| `Index { target, index }` | emite target → `t`; emite index → `i`; `SetIndex { target: t, index: i, value: val }` |

El valor de retorno de la expresión `Assign` es `val` (la asignación retorna el valor asignado).

### §3.5 Lowering de `TypeDecl`

`lower_program` itera `hir.program.types` para construir los `TypeDescriptor`.

**`fields` y `pointer_map`** (paralelos, en orden de declaración):

Los campos vienen de `type_decl.members` (campo directo, sin `.body`) filtrados por `MemberKind::Attribute`. `TypeEnv` no tiene `field_type`; usar `hir.expr_type(attr_value_expr.id)`:
```
let attrs: Vec<_> = type_decl.members
    .iter()
    .filter_map(|m| if let MemberKind::Attribute { name, value, .. } = &m.kind {
        Some((name, value))
    } else { None })
    .collect();

for (name, value_expr) in &attrs {
    fields.push(name.to_string())
    let ty = hir.expr_type(value_expr.id).unwrap_or(TypeId::OBJECT)
    pointer_map.push(is_reference(ty))
}
```

Los métodos se obtienen filtrando `MemberKind::Method(FunctionDecl)` de `type_decl.members`.

`TypeId::STRING` es referencia (`is_reference` retorna `true`) — las strings HULK son heap-managed.

**Constructor `__init__` (`format!("{}.__init__", type_decl.name)`):**

Antes de lowering, reiniciar el Lowerer:
```rust
self.instrs.clear();
self.locals.clear();
self.shadow_count = 0;
let t_self = self.fresh_temp();
self.self_temp = Some(t_self);
self.current_type_name = Some(type_decl.name.clone());
self.current_parent_type_name = type_decl.parent.as_ref().map(|p| p.name.clone());
self.current_method_name = Some("__init__".to_string());
```

Params del constructor = `type_decl.params` (la lista de `Param` del TypeDecl). Asignar un TempId fresh a cada param e insertar en `locals` (ver "SymbolId de params" abajo).

Body emission:
1. Si tiene padre: emitir `StaticCall { dst: fresh, type_name: parent_name, method: "__init__", args: [Temp(t_self)] + type_decl.parent.args.iter().map(|e| emit_expr(e)) }`
2. Por cada `(name, value)` en `attrs` (orden de declaración): emitir `SetField { object: Temp(t_self), field: name, value: emit_expr(value) }`
3. Emitir `Return(Temp(t_self))`

**Métodos (`format!("{}.{}", type_decl.name, method.name)`):**

Antes de lowering, reiniciar el Lowerer igual que para `__init__`, pero con `current_method_name = Some(method.name.clone())`.

Params del método = `method.params`. Emitir el body del método → `body_val`; luego `Return(body_val)`.

**Params en `param_temps` (no requiere extensión del resolver):**

`hulk_ast::Param` no tiene `node_id`, por lo que no se puede usar `hir.resolved_symbol` para params. En cambio, los params se almacenan en `param_temps: HashMap<String, TempId>` indexados por nombre:

```rust
// Al inicio de cada función/método:
param_temps.clear();
for param in &function.params {
    let t = fresh_temp();
    param_temps.insert(param.name.clone(), t);
    param_id_list.push(t);  // para BannerFunction::params
}
```

Cuando `emit_expr` encuentra `ExprKind::Ident` con `SymbolKind::Parameter`, usa:
```rust
let name = hir.symbols.table().name_of(sym_id).unwrap();
Value::Temp(param_temps[name])
```

**LetBinding SymbolId — extensión requerida del resolver:**

`resolver/names/exprs.rs::resolve_let` debe almacenar el `SymbolId` del binding:
```rust
let sym_id = self.define(binding.name.clone(), SymbolKind::Variable, binding.span.clone());
self.expr_symbols.insert(binding_expr.id, sym_id);  // ← agregar
```

Con esta extensión, el lowerer usa `hir.resolved_symbol(binding_expr.id).unwrap()` para LetBinding.

Al terminar cada función/método, restaurar `self_temp = None` y los campos `current_*` a `None`.

### Shadow stack

**Regla de referencia:**

```rust
fn is_reference(ty: TypeId) -> bool {
    ty != TypeId::NUMBER && ty != TypeId::BOOLEAN
}
```

**En `emit_let_binding`:**  
Con la extensión del resolver descrita en §3.5, el `SymbolId` del binding se obtiene de `hir.resolved_symbol(binding_expr.id)`. El lowerer:
1. `sym_id = hir.resolved_symbol(binding_expr.id).unwrap()`
2. `dst = fresh_temp()`
3. `val = emit_expr(binding.value)`
4. Emite `Copy { dst, src: val }`
5. `locals.insert(sym_id, dst)`
6. Si `is_reference(type)`: emite `ShadowPush(Temp(dst))`; incrementa `shadow_count` local

**Al salir de `Let`:**  
Después de emitir el body, emitir `ShadowPop` tantas veces como el contador local del scope.

El tipo se consulta con `hir.expr_type(binding.value.id)` (método directo en `Hir`, delega a `TypeEnv::expr_type`).

**`shadow_count` en scopes anidados:**  
`shadow_count` es local al lowering de cada `Let`. Al entrar en un `Let`, el lowerer guarda el valor actual y lo reinicia a 0. Al salir, emite `ShadowPop` × `shadow_count_local` y restaura el valor guardado. Esto garantiza que scopes anidados (`let x = let y = ... in y in x`) emitan el número correcto de pops para cada nivel independientemente.

**Fallback cuando `expr_type` retorna `None`:**  
Si el tipo inferido no está disponible (resultado `None`), tratar el binding como referencia y emitir `ShadowPush`. Esta política conservadora garantiza corrección de GC a costa de un `ShadowPush` innecesario en casos donde el tipo es realmente primitivo; es preferible a perder una referencia viva.

### Algoritmo de `lower_program`

```
fn lower_program(hir: &Hir) -> BannerProgram:
    let types: Vec<TypeDescriptor> = hir.program.types
        .iter().map(|td| lower_type_decl(td)).collect()

    let functions: Vec<BannerFunction> = hir.program.functions
        .iter().map(|fd| lower_function_decl(fd)).collect()

    let main: BannerFunction = lower_main_body(&hir.program.body)
    // main se llama "__main__", sin params, wrapping el body expression del programa

    BannerProgram { types, functions, main }
```

Para `lower_function_decl(fd)`: similar a los métodos de §3.5 pero sin self. `fd.params` → TempIds (extensión del resolver requerida). Body: `emit_expr(fd.body)` → `body_val`; `Return(body_val)`.

Para `lower_main_body(body_expr)`: sin params; body es `emit_expr(body_expr)` seguido de `Return(body_val)`.

**`Instr::New` vs `__init__`:**  
`Instr::New { type_name, args }` es una instrucción de alto nivel. El lowerer **no** emite llamadas explícitas a `TypeName.__init__`. Es responsabilidad de codegen (sesión 15) expandir `New` en alocación (`Alloc`) + llamada al método `__init__` generado en §3.5. Esta separación mantiene el lowerer agnóstico a la ABI de objetos.

### Builtins vs funciones globales

Tanto `SymbolKind::Function` como `SymbolKind::BuiltinFunction` se lowerizan a `Global(name)` en `emit_expr(Ident)`. La distinción `Function` vs `BuiltinFunction` no afecta al lowerer: ambos se emiten como `Instr::Call { callee: Global(name), ... }`. La diferencia es visible solo en codegen, que enlaza los builtins con el runtime en C (`__hulk_print`, `__hulk_sqrt`, etc.) y los globales con funciones HULK compiladas.

---

## Sección 4 — Tests

```
crates/hulk-banner/tests/
  support/mod.rs      ← build_banner(src: &str) -> BannerProgram
  translation.rs      ← 13.2: fib, llamadas, new, field access
  control_flow.rs     ← 13.3: if, while, let con shadowing
  shadow_stack.rs     ← 13.3: ShadowPush/Pop para variables String/Object
```

`hulk-driver` se agrega como dev-dependency para que `build_banner` use `build_pipeline`.

### Invariantes que los tests verifican

Los tests **no** comparan el vector completo de instrucciones (frágil ante cambio de contadores). Verifican invariantes estructurales:

- `fib` genera al menos una `Instr::Call` cuyo callee es `Global("fib")`.
- Un `if` genera al menos un `Instr::JumpIf` y al menos dos `Instr::Label`.
- Un `while` genera un `Instr::Label` de loop y un `Instr::JumpIf` de salida.
- Un `let s: String = "x"` genera al menos un `ShadowPush` y un `ShadowPop` balanceados.
- Un `let n: Number = 1` **no** genera `ShadowPush`.

---

## Decisiones de diseño

| Decisión | Razón |
|---|---|
| `BinOpKind`/`UnaryOpKind` reusados de `hulk_hir` | Evita duplicación; banner ya depende de hir |
| `Alloc` presente pero no emitida | Cumple la spec del PIPELINE; codegen puede usarla si necesita descomponer `New` |
| `StaticCall` para `base()` | Permite que codegen genere vtable lookup estático sin analizar la jerarquía de nuevo |
| Shadow stack en `Let` scope | Granularidad mínima correcta: cada `let` introduce y cierra su propio scope |
| Tests por invariantes, no por snapshot | Menos frágil; el conteo de temps cambia con cualquier refactor del lowerer |
| `param_names` paralelo a `params` en `BannerFunction` | El pretty-printer lo usa; codegen lo ignora |
| `VecLiteral` → `__vec_new` + `__vec_push` calls | VecGenerator se desugariza, pero VecLiteral sobrevive; se lowerizan a llamadas de runtime para que codegen no necesite conocer la representación interna del vector |
| `Is`/`As` → `__hulk_is` / `__hulk_as` calls | Delegado al runtime; la tabla de tipos vivos no está disponible en tiempo de compilación BANNER |
| Fallback `None` de `expr_type` → es referencia | Política conservadora: mejor un ShadowPush innecesario que perder una referencia viva |
| Builtins y functions globales → ambos `Global(name)` | El lowerer no distingue origen; codegen enlaza builtins al runtime C y funciones al código compilado |
| `If` sin `else` retorna `ConstNull` | Consistente con la semántica de HULK donde `if` sin `else` produce un valor null |
