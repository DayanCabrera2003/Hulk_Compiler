# Sección 14 — Runtime C

## Qué se implementó

Biblioteca estática `libhulkruntime.a` escrita en C11. Proporciona el recolector de basura
mark-and-sweep, el tipo string inmutable y las funciones builtin del lenguaje HULK.
El crate `hulk-codegen` la compila e integra automáticamente mediante `build.rs`.

### Archivos creados

| Archivo | Responsabilidad |
|---------|----------------|
| `runtime/gc.h` | API pública del GC: `TypeTag`, `ObjHeader`, macros `HULK_PAYLOAD`/`HULK_HEADER`, shadow stack |
| `runtime/gc.c` | Implementación de mark-and-sweep + shadow stack |
| `runtime/test_gc.c` | Tests en C del GC (compilados y ejecutados con gcc directamente) |
| `runtime/strings.h` | API de strings: `HulkStr`, `hulk_string_tag`, declaraciones |
| `runtime/strings.c` | `hulk_string_new`, `hulk_string_concat`, `hulk_number_to_string` |
| `runtime/test_strings.c` | Tests en C de operaciones de string |
| `runtime/builtins.h` | Declaraciones de funciones builtin |
| `runtime/builtins.c` | `hulk_print*`, wrappers matemáticos, `hulk_range_new` |
| `crates/hulk-codegen/build.rs` | Compila los tres .c con gcc, los empaqueta en `libhulkruntime.a` |

### Funciones públicas expuestas (C ABI)

**GC** (`gc.h`):

| Función | Descripción |
|---------|-------------|
| `void* hulk_alloc(TypeTag*, size_t)` | Asigna objeto gestionado; activa GC si se supera el threshold |
| `void hulk_gc(void)` | Ciclo completo mark-and-sweep |
| `void hulk_shadow_push(void*)` | Empuja un valor de referencia a la shadow stack |
| `void hulk_shadow_pop(void)` | Saca una entrada de la shadow stack |

**Strings** (`strings.h`):

| Función | Descripción |
|---------|-------------|
| `void* hulk_string_new(const char*)` | Crea nuevo `HulkStr` copiando un C string |
| `void* hulk_string_concat(void*, void*)` | Concatenación inmutable: produce un nuevo `HulkStr` |
| `void* hulk_number_to_string(double)` | Formatea un Number a String con `%g` |
| `int __hulk_str_eq(void*, void*)` | Compara dos `HulkStr` byte a byte; devuelve 1 si iguales, 0 si distintos |

**Builtins** (`builtins.h`):

| Función | Descripción |
|---------|-------------|
| `void hulk_print(void*)` | Imprime objeto de referencia (inspecciona `TypeTag`) |
| `void hulk_print_number(double)` | Imprime Number directamente (evita boxing) |
| `void hulk_print_bool(int)` | Imprime Boolean como `"true"` o `"false"` |
| `double hulk_sqrt/sin/cos/exp/log(double)` | Wrappers de `<math.h>` |
| `double hulk_rand(void)` | Número aleatorio en `[0.0, 1.0)` |
| `void* hulk_range_new(double, double, double)` | Crea objeto Range (min, max, step) |

## Decisiones de diseño

### Layout del ObjHeader

```
+------------------+  <- dirección devuelta por malloc
| TypeTag* tag     |  8 bytes (descriptor de tipo: nombre, pointer map)
| size_t   size    |  8 bytes (total: sizeof(ObjHeader) + payload)
| int      mark    |  4 bytes (bit de marcado para el colector)
| ObjHeader* next  |  8 bytes (lista intrusiva de todas las asignaciones)
+------------------+  <- HULK_PAYLOAD: dirección devuelta al usuario
| payload...       |
```

Los macros `HULK_PAYLOAD(hdr)` y `HULK_HEADER(pay)` convierten entre ambas
direcciones con aritmética de punteros. Alternativa descartada: lista auxiliar
separada fuera del objeto. La lista intrusiva es mejor en localidad de caché y
no requiere asignaciones adicionales.

### Shadow stack de tamaño fijo

La shadow stack es un array estático de 4096 `void*`. Alternativas descartadas:
- **Array dinámico con `realloc`**: añade complejidad y una posible asignación
  en el camino caliente de push/pop.
- **Lista enlazada por frame**: overhead de puntero por entrada y mala localidad.

4096 ranuras cubre cualquier programa HULK realista: se saturarían 4096 variables
de referencia simultáneamente activas en el call stack. Si se supera, el runtime
aborta con mensaje claro (falla controlada, no corrupción silenciosa).

### GC threshold adaptativo

Tras cada colección, el threshold se ajusta a `live_bytes × 2`. Esto garantiza que:
- Programas con poco heap vivo recolectan raramente (threshold >= 1 MiB siempre).
- Programas con mucho heap recolectan proporcionalmente, no en bucle constante.

El threshold mínimo fijo (1 MiB) evita colecciones degeneradas cuando el conjunto
vivo es casi nulo.

### Strings inmutables con flexible array member

`HulkStr` usa `char data[]` (flexible array member) en lugar de `char*`:
- Un único `malloc` por string (no dos: header separado + datos).
- El GC solo necesita un nodo en `__hulk_alloc_list`.
- `num_pointers = 0` en `hulk_string_tag` porque el payload son bytes crudos,
  no punteros al heap.

Alternativa descartada: `char* data` con asignación separada. Requeriría un
campo puntero en el payload, que el GC debería trazar (complicando el pointer map),
o un finalizador para liberar los datos (que HULK no tiene).

### Separación de `hulk_print` en variantes por tipo

`print()` en HULK acepta cualquier valor. `Number` y `Boolean` son value types:
no viven en el heap y no tienen `ObjHeader`, por lo que no pueden pasarse a
`hulk_print(void*)`. Se exponen funciones separadas:

- `hulk_print(void*)` para tipos de referencia (String, tipos de usuario).
- `hulk_print_number(double)` para Number.
- `hulk_print_bool(int)` para Boolean.

El codegen (sesión 15) decide cuál llamar según el tipo estático del argumento.
Alternativa descartada: boxing automático de Numbers antes de llamar a print.
Añadiría presión al GC y haría más compleja la generación de código.

### `hulk_range_new` y la sesión 16 (Prelude)

El layout interno de `HulkRange` (tres doubles: `current`, `max`, `step`) coincide
con los atributos del tipo `Range` del prelude HULK. El codegen accederá a los
campos por offset; el runtime solo necesita alocar el objeto con el tamaño y
TypeTag correctos. La integración completa ocurre en la sesión 16.

### build.rs con `gcc` y `ar` directos

El `build.rs` invoca `gcc` y `ar` mediante `std::process::Command` en lugar de
la crate `cc`. Se eligió esta opción porque:
- Evita añadir una dependencia de build nueva (regla 9.2 de `rules.md`).
- Los flags necesarios (`-O2 -Wall -Werror -I`) son simples y portables a cualquier
  sistema con GCC disponible (el proyecto ya requiere GCC para el enlace final
  en sesión 15).

## Gotchas

### Ciclos en mark-and-sweep

La guarda `if (hdr->mark) return;` en `mark()` evita recursión infinita en grafos
con ciclos. Esta es la ventaja principal de mark-and-sweep sobre conteo de
referencias: los ciclos se recolectan correctamente sin esfuerzo adicional.

### Estado compartido entre tests en C

El test `test_gc_traces_pointer_fields` captura `__hulk_alloc_bytes` después de
alocar dos objetos y antes de llamar a GC. Si hay objetos inalcanzables de tests
anteriores en el heap, el GC los libera también y el baseline ya no coincide.
La solución fue llamar a `hulk_gc()` al inicio del test para limpiar el heap antes
de capturar el baseline.

### `-lm` en Linux

Las funciones `sqrt`, `sin`, `cos`, `exp`, `log` de `<math.h>` viven en `libm`
en Linux (no en `libc`). El `build.rs` emite `cargo:rustc-link-lib=m`.

### Lints del workspace en build.rs

Los lints del workspace (`unwrap_used = "deny"`, `expect_used = "deny"`) se aplican
también a `build.rs`. Por este motivo, el script usa `match` explícito y
`std::process::exit(1)` en lugar de `.unwrap()` / `.expect()`.

## Ejemplos de uso (desde el codegen en sesión 15, en pseudocódigo LLVM IR)

```llvm
; Crear un string
%s = call i8* @hulk_string_new(i8* @".hello")
call void @hulk_shadow_push(i8* %s)

; Concatenar strings
%cat = call i8* @hulk_string_concat(i8* %s1, i8* %s2)

; Imprimir un Number directamente (sin boxing)
call void @hulk_print_number(double 42.0)

; Imprimir un String
call void @hulk_print(i8* %cat)

; Salir del scope
call void @hulk_shadow_pop()

; Crear un Range para un for loop
%range = call i8* @hulk_range_new(double 0.0, double 10.0, double 1.0)
```
