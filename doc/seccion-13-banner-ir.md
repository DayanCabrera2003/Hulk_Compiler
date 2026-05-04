# Sección 13 — BANNER IR

## Qué se implementó

Crate nuevo: `hulk-banner`. Contiene la representación intermedia de tres direcciones
que sirve de puente entre el HIR desugareado (sesión 11) y la generación de código C
(sesión 15).

### Archivos creados

| Archivo | Responsabilidad |
|---------|----------------|
| `crates/hulk-banner/src/ir.rs` | Tipos de datos del IR: `TempId`, `Value`, `Instr`, `BannerFunction`, `TypeDescriptor`, `BannerProgram` |
| `crates/hulk-banner/src/lowerer.rs` | `Lowerer<'h>`: traduce HIR a `Vec<Instr>` |
| `crates/hulk-banner/src/print.rs` | `Display` para imprimir el IR en forma textual |
| `crates/hulk-banner/src/lib.rs` | Punto de entrada público: `lower_program(hir: &Hir) -> BannerProgram` |
| `crates/hulk-banner/tests/translation.rs` | Tests de traducción de expresiones y funciones |
| `crates/hulk-banner/tests/control_flow.rs` | Tests de control de flujo |
| `crates/hulk-banner/tests/shadow_stack.rs` | Tests del shadow stack para GC |
| `crates/hulk-banner/tests/ir_types.rs` | Tests de traducción de tipos |
| `crates/hulk-banner/tests/print.rs` | Tests del pretty-printer |
| `crates/hulk-banner/tests/support/mod.rs` | Helper `build_banner` para los tests de integración |

### Modificaciones en otros crates

| Archivo | Cambio |
|---------|--------|
| `crates/hulk-semantic/src/resolver/names/exprs.rs` | `resolve_let` almacena `expr_symbols[binding_expr.id]`; `resolve_assign_target` recibe `node_id` y almacena `expr_symbols[node_id]` para el brazo `Ident` |
| `crates/hulk-types/src/inferer.rs` | Bug fix: `ExprKind::LetBinding` recursaba sobre el valor en lugar de devolver `TypeId::OBJECT` |
| `crates/hulk-driver/tests/resolver_extensions.rs` | Tests para las extensiones del resolver |

## Estructura del IR

### Temporales (`TempId`)

Todo valor intermedio vive en un temporal `t0`, `t1`, ... creado por el lowerer.
Los temporales son opacos; son internos a una función y no se reúsan entre funciones.

### Valores (`Value`)

```
Value::Temp(TempId)          — temporal
Value::ConstNum(f64)         — literal numérico
Value::ConstStr(String)      — literal de cadena
Value::ConstBool(bool)       — literal booleano
Value::ConstNull             — null
Value::Global(String)        — nombre de función, tipo o builtin global
```

### Instrucciones (`Instr`)

Hay 18 variantes agrupadas en cuatro categorías:

**Asignación y aritmética**:
`Copy`, `BinOp`, `UnOp`

**Llamadas**:
`Call` (callee dinámico), `MethodCall` (dispatch por nombre en objeto receptor),
`StaticCall` (dispatch estático conocido en compile time, usado para `base()`),
`New` (construcción de tipo)

**Acceso a memoria**:
`GetField`, `SetField`, `GetIndex`, `SetIndex`

**Control de flujo**:
`Label`, `Jump`, `JumpIf`, `Return`

**GC shadow stack**:
`ShadowPush`, `ShadowPop`

**Reservada** (nunca emitida por el lowerer; disponible para codegen):
`Alloc`

### Formato textual

```
t0 = 1
t1 = 2
t2 = t0 + t1
return t2
```

Instrucciones de control de flujo:
```
label L0
jumpif t0 -> L1
jump L2
label L1
```

GC:
```
shadow_push t3
; ... uso de t3 ...
shadow_pop
```

## Decisiones de diseño

### Nombres de métodos calificados

Los métodos se nombran como `"TypeName.method"` y los constructores como
`"TypeName.__init__"`. Esto permite que codegen distinga métodos de diferentes
tipos sin necesidad de una tabla de tipos en tiempo de ejecución.

Alternativa descartada: usar solo el nombre corto del método (e.g., `"get"`).
El problema es que tipos distintos pueden tener métodos con el mismo nombre y
el codegen no podría generar símbolos únicos para las funciones C.

### Pre-extracción en `lower_program` para evitar conflictos de borrows

`lower_program` necesita iterar sobre los tipos y funciones del HIR mientras
llama a `&mut self` para emitir instrucciones. El compilador de Rust rechaza
el patrón `for td in self.hir.types.values() { self.emit_expr(...) }` porque
el iterador toma un borrow de `self.hir` y `emit_expr` necesita `&mut self`.

Solución adoptada: `let hir = self.hir;` extrae la referencia fuera del método,
y se pre-extraen los datos iterados en colecciones de tipos propios (`Vec<TypeEntry>`).
Así, el for loop no tiene referencias activas al mismo tiempo que las llamadas mut.

### Parámetros por nombre vs SymbolId

Los parámetros de función se almacenan en `param_temps: HashMap<String, TempId>`
(indexados por nombre) en lugar de `HashMap<SymbolId, TempId>`. El motivo es que
los nodos `Param` del HIR no tienen `NodeId` y por tanto no pueden resolver su
SymbolId a través de `hir.resolved_symbol(expr.id)`.

### Shadow stack conservador

Se emite `ShadowPush` solo para variables cuyo tipo es una referencia
(cualquier tipo que no sea `Number` ni `Boolean`). La clasificación usa
`is_reference(ty): bool = ty != TypeId::NUMBER && ty != TypeId::BOOLEAN`.

Si el tipo es desconocido (`TypeId::OBJECT`), se trata como referencia para
garantizar que el GC no pierda punteros.

El shadow stack se gestiona por scope de `let`: al entrar se guarda `shadow_count`,
al salir se emiten tantos `ShadowPop` como pushs se hicieron en ese scope.

### Vectores (`VecLiteral`)

Un literal `[a, b, c]` se traduce a:
```
t0 = call __vec_new(3)
call __vec_push(t0, a_val)
call __vec_push(t0, b_val)
call __vec_push(t0, c_val)
```

Alternativa descartada: `__hulk_vec_new(a, b, c)` con todos los elementos en la
llamada. El problema es que el ABI de C necesita un tamaño conocido en compile time;
construir el vector con `n` llamadas a `push` es más simple y uniforme.

## Gotchas

### Bug en el inferidor de tipos para `LetBinding`

El inferidor de tipos (`hulk-types`) retornaba `TypeId::OBJECT` para
`ExprKind::LetBinding` sin recursar sobre el valor. Esto hacía que el lowerer
tratara todos los bindings como referencias y emitiera `ShadowPush` incluso para
variables `Number`.

El fix es: `ExprKind::LetBinding(lb) => self.infer_expr(&lb.value)`.

Este bug era silencioso hasta que se implementaron los tests de shadow stack.

### Extensión del resolver necesaria antes del lowerer

El lowerer necesita el `SymbolId` de cada `LetBinding` y de cada
`AssignTarget::Ident` para poder insertar y buscar en `locals`. El resolver
original no guardaba estos ids en `expr_symbols`.

Se añadieron dos cambios al resolver:
1. `resolve_let`: `let sym_id = self.define(...); self.expr_symbols.insert(binding_expr.id, sym_id);`
2. `resolve_assign_target`: `if let Some(sym_id) = self.lookup(name) { self.expr_symbols.insert(node_id, sym_id); }`

Sin estos cambios, `hir.resolved_symbol(expr.id)` devuelve `None` para bindings
y el lowerer paniquea.

## Ejemplos de uso

```rust
use hulk_banner::lower_program;
use hulk_driver::build_hir;
use hulk_hir::SourceFile;
use hulk_diagnostics::DiagnosticBag;

let src = "let x = 42 in print(x);";
let sf = SourceFile::new("example", src);
let mut bag = DiagnosticBag::new();
let hir = build_hir(sf, &mut bag).unwrap();
let program = lower_program(&hir);

// El programa tiene una función main con las instrucciones de nivel superior.
println!("{}", program.main);
```

Salida típica del pretty-printer para `let x = 42 in print(x);`:
```
function main():
  t0 = 42
  shadow_push t0    ; si x fuera String (en este caso Number, no se emite)
  t1 = call print(t0)
  return t1
```
