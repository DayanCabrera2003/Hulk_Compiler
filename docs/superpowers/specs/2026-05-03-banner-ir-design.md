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
    locals: HashMap<SymbolId, TempId>,
    shadow_count: usize,  // referencias activas empujadas al shadow stack en el scope actual
}
```

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
| `Ident` | `Temp(locals[sym])` si local/param; `Global(name)` si función global |
| `Self_` | `Temp(locals[self_sym])` |
| `BinOp` | emite left, right → `Instr::BinOp { dst: fresh() }` |
| `UnaryOp` | emite operand → `Instr::UnOp { dst: fresh() }` |
| `Call` | emite callee + args → `Instr::Call { dst: fresh() }` |
| `MethodCall` | emite receiver + args → `Instr::MethodCall { dst: fresh() }` |
| `New` | emite args → `Instr::New { dst: fresh() }` |
| `Block(es)` | emite cada `e`; retorna valor del último |
| `Let { bindings, body }` | emite bindings, body, luego `ShadowPop` × refs |
| `LetBinding(lb)` | emite value → `fresh_temp`; `ShadowPush` si tipo ref |
| `Assign { target, value }` | emite value; emite `SetField` o `Copy` según target |
| `FieldAccess` | `Instr::GetField { dst: fresh() }` |
| `Index` | `Instr::GetIndex { dst: fresh() }` |
| `If` | serie de JumpIf + Jump + Labels |
| `While` | label de loop + JumpIf de salida |

### Control de flujo — `if/elif/else`

```
    t_cond = <condición principal>
    jumpif t_cond then_N
    [elif: t_cXX = <cond>; jumpif t_cXX then_elif_XX; ...]
    <else branch o ConstNull>
    jump end_N
  then_elif_XX:
    <elif body>
    jump end_N
  then_N:
    <then branch>
  end_N:
```

El resultado de la expresión `if` se escribe en un `TempId` de resultado común antes de cada `jump end_N`.

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

### Shadow stack

**Regla de referencia:**

```rust
fn is_reference(ty: TypeId) -> bool {
    ty != TypeId::NUMBER && ty != TypeId::BOOLEAN
}
```

**En `emit_let_binding`:**  
Si el tipo inferido del binding es referencia → emitir `ShadowPush(Temp(dst))` inmediatamente después del valor.

**Al salir de `Let`:**  
Después de emitir el body, emitir `ShadowPop` tantas veces como `ShadowPush` se emitieron para este scope.

El tipo se consulta con `hir.types.expr_type(binding.value.id)`.

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
