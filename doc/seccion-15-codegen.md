# Sección 15 — Codegen LLVM

## Qué se implementó

Esta sección implementa el crate `hulk-codegen`, que traduce el HIR desugared a un
ejecutable nativo usando LLVM a través de la librería `inkwell`.

La pipeline completa es:

```
Hir → hulk_banner::lower_program → BannerProgram
    → Codegen::predeclare_all     → declaraciones LLVM
    → Codegen::emit_program       → módulo LLVM IR
    → module.verify()             → validación
    → compile_to_object           → archivo .o
    → link_executable             → ejecutable nativo
```

### Archivos creados

| Archivo | Responsabilidad |
|---|---|
| `src/error.rs` | `CodegenError` y `CodegenResult<T>` |
| `src/layout.rs` | Cálculo estático de vtables y offsets de campos |
| `src/rt.rs` | Declaraciones de funciones C del runtime |
| `src/codegen.rs` | Struct `Codegen<'ctx>` y utilidades base |
| `src/emit.rs` | Orquestación de emisión LLVM (funciones, bloques, instrucciones) |
| `src/emit_ops.rs` | Operaciones binarias y unarias |
| `src/emit_call.rs` | Llamadas directas, virtuales y dispatch de `print` |
| `src/emit_mem.rs` | `New`, `GetField`, `SetField`, índices, TypeTags, vtables |
| `src/link.rs` | Emisión de objeto `.o` y enlazado con `libhulkruntime.a` |
| `src/pipeline.rs` | API pública `compile()` y `emit_ir_string()` |

## Subsección 15.1 — Infraestructura inkwell

### Representación LLVM de los tipos HULK

| Tipo HULK | Tipo LLVM | Justificación |
|---|---|---|
| `Number` | `f64` | La especificación pide 64-bit IEEE 754. |
| `Boolean` | `i1` | Resultado de comparaciones; LLVM representa booleans como enteros de 1 bit. |
| `String`, objetos, `null` | `ptr` (opaque, `i8*`) | Uniform representation; simplifica la interfaz con el GC. |
| Struct de tipo usuario | `{ ptr vtable, ptr field... }` | El primer campo siempre es el puntero a la vtable. |

Se usa `i8*` como puntero opaco porque inkwell 0.4.0 no expone el tipo `ptr` sin tipo de
`Context::ptr_type()`; la forma canónica es `ctx.i8_type().ptr_type(AddressSpace::default())`.

### ProgramLayout

Antes de emitir código se computa un `ProgramLayout` que establece:
- Orden estable de campos por tipo (necesario para que los GEP sean consistentes).
- Orden estable de métodos en la vtable (lexicográfico global, no por tipo).

Este orden se establece una sola vez y todas las emisiones posteriores lo respetan.

## Subsección 15.2 — Traducción de instrucciones BANNER

### Tabla Instr → LLVM

| Instrucción BANNER | Emisión LLVM |
|---|---|
| `BinOp(Add/Sub/Mul/Div/Mod)` | `build_float_{add,sub,mul,div,rem}` |
| `BinOp(Pow)` | Llamada al intrinsic `llvm.pow.f64` |
| `BinOp(Eq/Ne)` | Dispatch por `TempKind`: `feq/fne` si `F64`, `icmp eq` si `I1`, `__hulk_str_eq` si `Ptr` |
| `BinOp(Lt/Le/Gt/Ge)` | `build_float_compare` con predicado OLT/OLE/OGT/OGE |
| `BinOp(And/Or)` | `build_and` / `build_or` sobre `i1` |
| `BinOp(Concat)` | Llamada a `__hulk_concat(a, b)` del runtime |
| `UnOp(Neg)` | `build_float_neg` |
| `UnOp(Not)` | `build_not` sobre `i1` |
| `Copy(dst, src)` | `load` desde el slot del origen, `store` en el slot del destino |
| `Call(dst, name, args)` | `build_call` a la función declarada; `print` hace dispatch por tipo |
| `MethodCall(dst, recv, method, args)` | GEP en vtable + `build_indirect_call` |
| `StaticCall(dst, type, method, args)` | `build_call` directo a `Type.method` |
| `New(dst, type, args)` | `hulk_alloc` + store vtable + llamada a `__init__` |
| `GetField(dst, obj, field)` | `build_struct_gep` + `build_load` |
| `SetField(obj, field, val)` | `build_struct_gep` + `build_store` |
| `GetIndex(dst, vec, idx)` | Llamada a `__vec_get(vec, idx)` |
| `SetIndex(vec, idx, val)` | Llamada a `__vec_set(vec, idx, val)` |
| `Label(name)` | Pre-creado como `BasicBlock`; `build_unconditional_branch` desde bloque anterior |
| `Jump(label)` | `build_unconditional_branch` |
| `JumpIf(cond, t, f)` | `build_conditional_branch` |
| `Return(val)` | `build_return` o `build_return_void` |
| `ShadowPush(val)` | Llamada a `hulk_shadow_push(ptr)` |
| `ShadowPop` | Llamada a `hulk_shadow_pop()` |
| `Alloc(dst, type)` | `hulk_alloc` sin `__init__` (uso interno del GC) |

### Estrategia alloca + mem2reg

Todos los temporales de BANNER usan slots alloca en el bloque de entrada de cada función.
Esto evita la necesidad de insertar phi-nodes durante la generación, que requeriría un
algoritmo de dominadores. LLVM aplica el pase `mem2reg` y convierte las cargas/stores a
registros SSA de forma automática.

### Inferencia de tipo de temporales

`infer_temp_kinds` hace una pasada única sobre las instrucciones para clasificar cada
temporal como `F64`, `I1` o `Ptr`. Luego propaga tipos a través de instrucciones `Copy`
(hasta 16 iteraciones). Los temporales que no se pueden inferir se tratan como `Ptr`.

### Dispatch de `print`

La función `print` es un caso especial: el compilador inspecciona el `LlvmVal` del
argumento en tiempo de compilación y redirige a:
- `hulk_print_number(f64)` si el argumento es `LlvmVal::Float`,
- `hulk_print_bool(i32)` si el argumento es `LlvmVal::Int` (i1),
- `hulk_print(ptr)` en cualquier otro caso (strings, objetos).

## Subsección 15.3 — Linking + ejecutables

### Compilación a objeto

`compile_to_object` inicializa el target nativo, crea una `TargetMachine` con
optimización nivel 2 y emite un archivo `.o` con `write_to_file(FileType::Object)`.

### Linking

`link_executable` busca `clang-17`, `clang`, `cc` o `gcc` en ese orden y llama al
compilador con:

```
cc hulk_out.o -L<lib_dir> -lhulkruntime -lm -o <output>
```

El `lib_dir` lo proporciona `build.rs` a través de la variable de entorno `OUT_DIR`.
En tiempo de tests, se puede pasar explícitamente en `CompileOptions`.

### API pública

```rust
// Compila HIR → ejecutable nativo.
pub fn compile(hir: &Hir, output: &Path, opts: &CompileOptions) -> CodegenResult<PathBuf>;

// Retorna el IR textual de LLVM sin producir ejecutable.
pub fn emit_ir_string(hir: &Hir) -> CodegenResult<String>;
```

## Nota sobre libffi en el sistema

El paquete `libffi-devel` no está instalado en este entorno. `llvm-sys` lo necesita para
enlazar binarios de test. Solución temporal: crear un symlink en un directorio temporal y
pasar `RUSTFLAGS="-L /tmp"` al ejecutar tests:

```sh
ln -s /usr/lib64/libffi.so.8 /tmp/libffi.so
RUSTFLAGS="-L /tmp" cargo test -p hulk-codegen
```

La solución permanente es instalar `libffi-devel`:

```sh
sudo dnf install libffi-devel
```

## Restricciones conocidas

- Los campos de structs de usuario se almacenan siempre como `ptr` en LLVM, incluso si el
  tipo HULK es `Number`. La carga/store con el tipo correcto queda pendiente para la sesión 16.
- `GetIndex`/`SetIndex` delegan en `__vec_get`/`__vec_set` que deben estar implementadas en
  el runtime (pendiente sesión 16).
- Los punteros `null` en HULK se representan como `ptr null` de LLVM; no hay comprobación
  de nulidad en tiempo de ejecución en esta sesión.

---

## Corrección post-sesión: bucle infinito en `infer_temp_kinds`

### Problema

Al compilar programas con tipos cuyos inicializadores de campo llaman a funciones libres
(p.ej. `width: Number = abs_num(w)`), el compilador colgaba indefinidamente durante la
generación de código. El cuelgue ocurría en la función `infer_temp_kinds` de este crate
(`crates/hulk-codegen/src/emit.rs`).

`infer_temp_kinds` ejecuta un bucle de punto fijo sobre las instrucciones BANNER para
determinar si cada temporal es `F64`, `I1` o `Ptr`. La propagación hacia atrás para
`SetField` contenía este código defectuoso:

```rust
// Bug: or_insert no sobreescribe un Ptr existente, pero changed = true se fija
// siempre que la condición es verdadera, produciendo un bucle infinito.
if kinds.get(val_tid).copied() != Some(TempKind::F64) {
    kinds.entry(*val_tid).or_insert(TempKind::F64);  // ← no sobreescribe Ptr
    changed = true;                                    // ← siempre se ejecuta
}
```

Si `val_tid` ya tenía tipo `Ptr` (porque `abs_num` retorna `Object` en el
inferidor de tipos, que mapea a `Ptr` en codegen), la condición era verdadera, pero
`or_insert` no modificaba el mapa. Sin embargo `changed = true` se ejecutaba, lo
que reiniciaba el bucle en cada iteración sin ningún progreso real → bucle infinito.

### Solución

Cambiar `or_insert` por `insert`, que sí sobreescribe el valor existente. Así la
actualización ocurre de verdad, `changed = true` refleja un cambio real, y el bucle
converge:

```rust
// Fix: insert sobreescribe Ptr → F64; changed = true solo después de un cambio real.
if kinds.get(val_tid).copied() != Some(TempKind::F64) {
    kinds.insert(*val_tid, TempKind::F64);  // ← sobreescribe Ptr correctamente
    changed = true;
}
```

La corrección está en `crates/hulk-codegen/src/emit.rs`, función `infer_temp_kinds`,
bloque de propagación hacia atrás para `Instr::SetField`.

Se añadió un test de regresión en
`crates/hulk-driver/tests/hang_field_calls_function.rs` que verifica que la
compilación de tipos con campos numéricos inicializados por llamadas a funciones libres
termina en menos de 5 segundos.
