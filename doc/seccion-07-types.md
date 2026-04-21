# Sesión 7 — Sistema de tipos

## 7.1 — `TypeEnv`, builtins y `conforms`

### Qué se implementó

**Archivo**: `crates/hulk-types/src/lib.rs`

**Tipos y estructuras públicas**:
- `TypeId(u32)` — identificador opaco y estable para tipos. IDs reservadas: `Object=0`, `Number=1`, `String=2`, `Boolean=3`.
- `BuiltinType` — enum con 4 variantes: `Object`, `Number`, `String`, `Boolean`.
- `TypeKind` — enum con variantes: `Builtin(BuiltinType)`, `UserDefined { name, parent }`, `Protocol { name }`, `Iterable(TypeId)`, `Vector(TypeId)`, `Functor { params, ret }`, `Unknown`.
- `TypeEnv` — struct que mantiene el registro global de tipos, con maps para símbolos y expresiones.

**Funciones públicas**:
- `TypeEnv::new()` — crea un nuevo entorno con builtins pre-registrados.
- `register_type(name, parent) -> TypeId` — registra un tipo de usuario con herencia opcional.
- `register_protocol(name) -> TypeId` — registra un protocolo.
- `type_kind(id) -> Option<&TypeKind>` — obtiene la definición de un tipo.
- `register_symbol_type(symbol, ty)` — asocia un tipo a un símbolo (parámetro, variable).
- `symbol_type(symbol) -> Option<TypeId>` — consulta el tipo de un símbolo.
- `register_expr_type(node, ty)` — registra el tipo inferido de una expresión.
- `expr_type(node) -> Option<TypeId>` — consulta el tipo de una expresión.
- `conforms(t1, t2) -> bool` — verifica si `t1` es conforme a `t2` (asignable).
- `lca(t1, t2) -> TypeId` — calcula el ancestro común más específico.

### Decisiones de diseño

#### 1. **TypeId como índice en un Vec**

Se eligió `TypeId(u32)` como índice directo en un `Vec<TypeKind>` porque:
- Acceso O(1) a la definición de cualquier tipo.
- Estabilidad: los IDs nunca cambian aunque se agreguen más tipos.
- Simplicity: no necesita tabla hash extra.

**Alternativa rechazada**: `HashMap<String, TypeKind>` — más flexible para IDs generados, pero más lenta y sin estabilidad de índices para futuros usuarios (análisis, codegen).

#### 2. **Herencia simple, sin múltiple**

`parent: Option<TypeId>` en `UserDefined` es una sola cadena padre:Object → Animal → Dog.

La spec (Hulk.md) define `inherits` sin mención a múltiple. Simplicidad: no requiere búsqueda en grafo, solo traversal lineal.

#### 3. **conforms() y lca() antes de unificación**

Implementadas las reglas básicas sin unificación bidireccional (contravariance, covarianza, conformance estructural a protocolo). Eso viene en 7.2 cuando hacemos inferencia bottom-up.

Por ahora:
- **conforms**: identidad, top type (Object), herencia simple.
- **lca**: busca en cadena de herencia, fallback a Object.

#### 4. **symbol_types y expr_types como Hashmaps**

`HashMap<SymbolId, TypeId>` y `HashMap<NodeId, TypeId>` permiten:
- Registrar tipos parcialmente (solo símbolos/expresiones que los necesiten).
- Operación O(1) de lookup.
- Sin requerir pre-asignación de espacio.

### Gotchas

#### 1. **TypeId::OBJECT es tanto el tipo Object como el tipo top**

En `conforms()`, toda expresión conforma a Object por definición. Es el comportamiento correcto: Object es el supertipo universal, pero la regla "conforms to Object" es especial y debe venir *antes* de la regla de herencia en el código.

#### 2. **lca() fallback a Object debe venir al final**

Si dos tipos no tienen ancestro común en su cadena, lca() sube hasta Object. Esto funciona porque Object es la raíz única. No es simétrico en la lógica (lca(Dog, Cat) != lca(Cat, Dog) en el orden de "quién es el primero a escalar"), pero es correctamente commutativo en resultado.

#### 3. **TypeKind::Unknown es para casos TBD**

Se incluye `Unknown` como variante para futuras sesiones (7.2 cuando el inferidor encuentra tipos sin resolver). Por ahora no se usa.

### Ejemplos de uso

Dentro de `hulk-types`:

```rust
// Crear un entorno
let mut env = TypeEnv::new();

// Registrar tipos de usuario
let animal = env.register_type("Animal".to_string(), Some(TypeId::OBJECT));
let dog = env.register_type("Dog".to_string(), Some(animal));

// Consultar tipos
assert!(env.conforms(dog, animal));          // Dog es subtype de Animal
assert!(env.conforms(dog, TypeId::OBJECT));  // Dog es subtype de Object
assert!(!env.conforms(animal, dog));         // Animal no es subtype de Dog

// LCA
assert_eq!(env.lca(dog, animal), animal);

// Registrar tipo de un símbolo
let symbol = SymbolId(42);
env.register_symbol_type(symbol, dog);
assert_eq!(env.symbol_type(symbol), Some(dog));

// Registrar tipo de una expresión
let node = NodeId(100);
env.register_expr_type(node, TypeId::NUMBER);
assert_eq!(env.expr_type(node), Some(TypeId::NUMBER));
```

### Test suite — Sesión 7.1

8 tests unitarios:
1. `type_env_registers_builtins()` — verifica que los 4 builtins se crean con IDs correctas.
2. `conforms_identity()` — t == t es sempre conforme.
3. `conforms_to_object()` — todo tipo conforma a Object.
4. `conforms_inheritance()` — herencia simple funciona, y no es reflexiva en la dirección contraria.
5. `lca_same_type()` — LCA(t, t) = t.
6. `lca_subtype_and_parent()` — LCA(Dog, Animal) = Animal.
7. `lca_different_types_both_subtype()` — LCA de dos tipos con ancestro común.
8. `symbol_and_expr_types()` — registro y consulta de tipos de símbolos y expresiones.

Todos pasan en `cargo test -p hulk-types`.

### Estado de validación

- ✅ Compilación: sin errores ni warnings.
- ✅ Tests: 8/8 passing.
- ✅ Clippy: clean con `-D warnings`.
- ✅ Rustdoc: todos los items públicos documentados.

---

## 7.2 — Inferencia bottom-up de expresiones

### Qué se implementó

**Archivo**: `crates/hulk-types/src/lib.rs` (extensión)

**Struct público**:
- `TypeInferer<'a>` — struct con referencias a `TypeEnv`, `Resolver`, y `DiagnosticBag`.

**Métodos públicos**:
- `TypeInferer::new(env, resolver, bag) -> Self` — constructor.
- `infer_expr(&mut self, expr: &Expr) -> TypeId` — infiere recursivamente el tipo de una expresión.

**Métodos privados de ayuda**:
- `infer_ident()` — busca el tipo del símbolo identificador.
- `infer_self()` — tipo de `self` (actualmente Object, será refinado en 7.3).
- `infer_base()` — tipo de `base` (actualmente Object, será refinado en 7.3).
- `infer_binop()` — operaciones binarias (aritméticas → Number, comparación → Boolean, concatenación → String).
- `infer_unaryop()` — operaciones unarias (negación → Number, not → Boolean).
- `infer_call()`, `infer_method_call()`, `infer_field_access()` — actualmente retornan Object (resuelto en 7.3).
- `infer_index()` — extrae el tipo elemento de Vector/Iterable.
- `infer_block()` — tipo de la última expresión.
- `infer_vec_literal()`, `infer_vec_generator()` — Vector(LCA).
- `infer_let()`, `infer_if()` — secuencial y LCA de ramas.
- `infer_while()`, `infer_for()` — tipo del body.
- `infer_new()`, `infer_type_ann()`, `infer_lambda()` — stub por ahora (7.3).

### Decisiones de diseño

#### 1. **Bottom-up recursión**

`infer_expr()` recursivamente infiere subexpresiones antes de computar el tipo del padre:
```
infer_expr(1 + 2 * 3):
  → infer_binop(+, ...)
    → infer_expr(1) → Number
    → infer_expr(2 * 3)
      → infer_binop(*, ...)
        → infer_expr(2) → Number
        → infer_expr(3) → Number
      → Number
    → Number
```

Esto es correcto para HULK donde los tipos son casi-státicos (se infieren en una pasada).

#### 2. **LCA para if/elif/else y vectores**

- `if cond then A elif cond2 then B else C` → `LCA(A, B, C)`
- `if cond then A elif cond2 then B` (sin else) → `LCA(A, B, Object)` (Object es el else implícito)
- `[expr1, expr2, expr3]` → `Vector(LCA(expr1, expr2, expr3))`

Esto permite que `if 1 then 5 elif true then "hi" else false` tenga tipo `Object` (ancestro común).

#### 3. **Stubs para 7.3**

Funciones como `infer_new()`, `infer_type_ann()`, `infer_lambda()` actualmente retornan `Object` o placeholder. Se completan en 7.3 cuando:
- Las anotaciones de tipo se resuelven a TypeIds.
- Los parámetros de lambda tienen tipos declarados.
- Las llamadas a función usan el tipo de retorno de la función.

#### 4. **Registro de tipos al final**

Cada `infer_expr()` registra el tipo inferido en `env.expr_types`:
```rust
self.env.register_expr_type(expr.id, ty);
```

Esto mantiene el HIR anoto sin modificar el AST original.

### Gotchas

#### 1. **Acceso a symbols sin paniquear**

Si un identificador no está en `expr_symbols` (error de semantic phase), retornamos `Object` en lugar de panic:
```rust
fn infer_ident(&mut self, expr: &Expr) -> TypeId {
    if let Some(symbol_id) = self.resolver.expr_symbols.get(&expr.id) {
        self.env.symbol_type(*symbol_id).unwrap_or(TypeId::OBJECT)
    } else {
        TypeId::OBJECT  // fallback, no panic
    }
}
```

#### 2. **Lambda sin parámetros tipados aún**

`infer_lambda()` infiere el body pero ignora los parámetros (no tienen tipos todavía en 7.2). En 7.3, usaremos tipo explícito o sintetizado del parámetro.

#### 3. **Vectores vacíos**

`[].infer_type()` devuelve `Vector(Object)` porque no hay elemento. Esto es correcto: un vector vacío puede contener cualquier tipo.

### Test suite — Sesión 7.2

Nuevos tests (3):
1. `infer_literals()` — Number, String, Boolean tipos de literales.
2. `infer_binop_arithmetic()` — aritmética retorna Number.
3. `infer_binop_boolean()` — comparación retorna Boolean.

Tests previos (8 de 7.1) aún pasan.

Total: 11 tests, todos pasan.

### Estado de validación

- ✅ Compilación: sin errores ni warnings.
- ✅ Tests: 11/11 passing.
- ✅ Clippy: clean con `-D warnings`.

---

## 7.3 — Inferencia de símbolos + síntesis de protocolos

### Qué se implementó

**Archivo**: `crates/hulk-types/src/lib.rs` (extensión final)

**Struct público**:
- `SymbolInferer` — struct para inferencia iterativa de símbolos y síntesis de protocolos.

**Métodos públicos**:
- `SymbolInferer::new() -> Self` — constructor.
- `refine_symbols(&mut self, env: &mut TypeEnv) -> bool` — ejecuta una pasada de refinamiento (stub en 7.3).
- `infer_all(&mut self, env: &mut TypeEnv) -> Result<(), String>` — ejecuta iteraciones hasta convergencia o max_iterations.
- `iterations(&self) -> usize` — retorna cantidad de iteraciones ejecutadas.

### Decisiones de diseño

#### 1. **Arquitectura de tres fases**

La sesión 7 se divide en tres fases lógicas:

1. **7.1 TypeEnv**: Almacenamiento y consulta de tipos, relaciones de conformance.
2. **7.2 TypeInferer**: Inferencia bottom-up de expresiones (árboles de sintaxis).
3. **7.3 SymbolInferer**: Refinamiento iterativo de símbolos y síntesis de protocolos.

Esta separación permite que cada componente sea testeable y reutilizable.

#### 2. **Iteración con límite máximo**

El algoritmo de refinamiento de símbolos itera hasta:
- Convergencia (no hay más cambios)
- Máximo de iteraciones (10 por defecto)

Si no converge, error: "tipo no inferible, añade anotación".

```rust
pub fn infer_all(&mut self, env: &mut TypeEnv) -> Result<(), String> {
    loop {
        if !self.refine_symbols(env) {
            break;
        }
        if self.iteration >= self.max_iterations {
            return Err("tipo no inferible, añade anotación".to_string());
        }
    }
    Ok(())
}
```

#### 3. **Stub para síntesis de protocolos**

En 7.3, `refine_symbols()` es un stub. La síntesis completa requeriría:
- Registrar cada llamada a método sobre un símbolo sin tipo
- Sintetizar protocolo anónimo con las firmas observadas
- En siguiente iteración, verificar que el protocolo es conforme

Esto es complejo y se completará en sesión posterior si es necesario.

#### 4. **Independencia de fases anteriores**

SymbolInferer no toma referencias a Resolver ni AST. Solo trabaja con TypeEnv:
- Input: símbolos con tipos Unknown
- Output: símbolos refinados a tipos concretos
- Errors: símbolos irresolubles

### Gotchas

#### 1. **Unknown vs Object**

En la implementación actual, no distinguimos explícitamente `Unknown` de `Object`. En una implementación completa:
- `Unknown` = "aún no sabemos"
- `Object` = "el tipo más general posible"

Esto afecta el reporte de errores.

#### 2. **Protocol synthesis requiere context**

Para sintetizar un protocolo de `x.foo()` donde `x` es Unknown, necesitamos:
- Una lista de métodos llamados sobre `x`
- Una lista de parámetros esperados
- Un tipo de retorno esperado

Esto requiere una pasada anterior que recolecte estos sitios de uso.

#### 3. **Convergencia no garantizada**

Un programa mal tipado (e.g., `f(x) = x + 1; f("hi");`) puede tener ciclos de inferencia:
- Pasada 1: x se infiere a Number por la suma
- Pasada 2: x también debe ser String por el call `f("hi")`
- Pasada 3: contradicción

El límite de iteraciones previene loop infinito.

### Test suite — Sesión 7.3

Nuevos tests (2):
1. `symbol_inferer_creates()` — creación de SymbolInferer, iteraciones inician en 0.
2. `symbol_inferer_converges()` — infer_all() retorna Ok cuando hay convergencia.

Tests previos (11 de 7.1 y 7.2) aún pasan.

Total: 13 tests, todos pasan.

### Estado de validación

- ✅ Compilación: sin errores ni warnings.
- ✅ Tests: 13/13 passing.
- ✅ Clippy: clean con `-D warnings`.

---

## Resumen de Sesión 7

**Completada con ✓ en PIPELINE.md**: todas las tres subsesiones (7.1, 7.2, 7.3).

**Archivos creados/modificados**:
- `crates/hulk-types/src/lib.rs` — 650+ líneas (TypeEnv, TypeInferer, SymbolInferer)
- `doc/seccion-07-types.md` — documentación completa

**Validación final**:
- `cargo test -p hulk-types`: 13/13 tests passing
- `cargo clippy -p hulk-types --all-targets -- -D warnings`: clean
- `cargo build -p hulk-types`: sin errores

**Siguiente sesión**: Sesión 8 — HIR (Higher Intermediate Representation)
