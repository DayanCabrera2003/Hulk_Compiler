# Rules for Implementation — HULK Compiler

> **Audiencia**: este documento está dirigido al modelo (o desarrollador asistido por modelo) que ejecuta las tareas definidas en `PIPELINE.md`. Define el conjunto de reglas de comportamiento, arquitectura, calidad, y comunicación que deben seguirse **sin excepción**.
>
> **Regla de oro**: ante cualquier conflicto entre una regla aquí y una instrucción puntual de una tarea, **prevalecen las reglas de este documento**. Si la tarea parece contradecir estas reglas, es una señal para **detenerse y preguntar al humano**.

---

## Tabla de contenidos

1. [Alcance del rol del modelo](#1-alcance-del-rol-del-modelo)
2. [Arquitectura y diseño](#2-arquitectura-y-diseño)
3. [Control de versiones y Git](#3-control-de-versiones-y-git)
4. [Comunicación con el humano](#4-comunicación-con-el-humano)
5. [Calidad del código](#5-calidad-del-código)
6. [Testing](#6-testing)
7. [Documentación](#7-documentación)
8. [Manejo de errores y robustez](#8-manejo-de-errores-y-robustez)
9. [Gestión de dependencias](#9-gestión-de-dependencias)
10. [Ejecución de tareas](#10-ejecución-de-tareas)
11. [Situaciones especiales](#11-situaciones-especiales)
12. [Anti-patrones prohibidos](#12-anti-patrones-prohibidos)
13. [Checklist pre-entrega](#13-checklist-pre-entrega)

---

## 1. Alcance del rol del modelo

### 1.1 Qué hace el modelo

El modelo es responsable de:

- **Leer y entender** la tarea asignada del `PIPELINE.md`, su contexto previo, objetivos, y criterios de aceptación.
- **Leer el código existente** en el repositorio antes de modificarlo o crear código nuevo.
- **Implementar** el código fuente (`.rs`, `.c`, `.h`, `.hulk`, `.toml`, etc.) para cumplir los criterios de la tarea.
- **Escribir tests** (unit, integration, snapshot, property) conforme a lo que la tarea especifique.
- **Actualizar la documentación** (`/doc/seccion-XX.md`, rustdoc, README, CHANGELOG) de acuerdo con lo que la tarea indique.
- **Ejecutar** comandos de build, test, lint, formato, para validar que los criterios de aceptación se cumplen.
- **Comunicar** al humano cualquier ambigüedad, inconsistencia, bloqueo, o decisión no trivial.

### 1.2 Qué NO hace el modelo

- **NO** ejecuta comandos de Git. Todo lo relacionado con branching, commits, pushes, merges, tags, y PRs lo hace el humano. El modelo puede **sugerir** mensajes de commit pero nunca los crea.
- **NO** modifica configuración remota (GitHub settings, protecciones de ramas, secrets de CI).
- **NO** hace release, tag, ni merges a `main` o `develop`.
- **NO** instala software de sistema (LLVM, compiladores) ni toca el entorno del usuario fuera del repositorio.
- **NO** toma decisiones de diseño que no estén cubiertas por la tarea o por `PIPELINE.md`, **sin preguntar primero**.
- **NO** inventa features o funcionalidad que la tarea no pida, aunque "parezcan útiles".
- **NO** refactoriza código ajeno a la tarea, a menos que la tarea lo requiera explícitamente.

### 1.3 Alcance por tarea

Cada sesión del modelo se enfoca en **exactamente una tarea** del `PIPELINE.md`. No debe:

- Mezclar cambios de múltiples tareas en la misma sesión.
- Adelantarse a tareas futuras.
- "Preparar el terreno" para tareas siguientes con código especulativo.

Si durante la tarea el modelo descubre que necesita algo que corresponde a otra tarea (anterior o posterior), debe **detenerse y preguntar** (ver sección 4).

---

## 2. Arquitectura y diseño

### 2.1 Clean Architecture es obligatoria

El proyecto sigue **Clean Architecture** con capas que solo dependen hacia adentro. El modelo debe respetar esto en cada línea de código:

- **Núcleo** (`hulk-span`, `hulk-diagnostics`): no depende de nada del proyecto.
- **Frontend** (`hulk-tokens`, `hulk-lexer`, `hulk-ast`, `hulk-parser`): depende solo del núcleo.
- **Semántico** (`hulk-semantic`, `hulk-types`, `hulk-hir`): depende de frontend.
- **Middleend** (`hulk-macros`, `hulk-desugar`): depende de HIR.
- **Backend** (`hulk-banner`, `hulk-codegen`): depende de HIR y tipos.
- **Orquestación** (`hulk-driver`, `hulk-cli`): depende de todo lo anterior.

**Reglas concretas**:

- **Nunca** agregar una dependencia a un `Cargo.toml` que vaya "hacia arriba" (ej: `hulk-lexer` dependiendo de `hulk-parser`).
- **Nunca** exponer tipos de una capa superior en la API pública de una inferior.
- **Nunca** usar `dyn Trait` para evadir la regla de capas.
- Si una tarea parece exigir una dependencia invertida, **parar y preguntar** — casi siempre hay un refactor correcto (mover el trait a una capa inferior, o compartir el tipo a través de un crate de contratos).

### 2.2 Principios SOLID y buenas prácticas

- **Single Responsibility**: cada módulo, struct y función tiene una responsabilidad clara. Si un nombre empieza a ser genérico tipo `Utils`, `Helpers`, `Manager` — es una señal de que la abstracción no está bien pensada.
- **Open/Closed**: preferir traits y composición sobre herencia. En Rust es natural.
- **Separación de datos y operaciones**: los tipos del AST, HIR, BANNER son **datos puros** (structs y enums). Las operaciones viven en funciones y visitors, no como métodos mezclados con los datos.
- **Inmutabilidad por defecto**: las estructuras de datos del compilador (AST, HIR, BANNER) son inmutables tras ser construidas. Las transformaciones producen **nuevos** árboles, no mutan los existentes. Excepción: el `TypeTable`, `SymbolTable` y estructuras similares que crecen durante el análisis.

### 2.3 Tamaño y forma del código

- **Funciones**: ≤ 50 líneas como regla general. Si hace falta más, partir en funciones privadas auxiliares con nombres descriptivos.
- **Archivos**: ≤ 500 líneas. Si crece más, subdividir en submódulos.
- **Complejidad ciclomática**: ≤ 15 por función (lo que clippy detecta).
- **Anidamiento**: ≤ 4 niveles. Si es más, extraer a función o usar `?` / early returns.
- **Argumentos por función**: ≤ 5. Si son más, agrupar en structs.

Si el modelo se encuentra violando alguno de estos límites, debe considerar si hay un refactor natural. Si el refactor excede el alcance de la tarea, anotarlo y **preguntar** si vale la pena hacerlo ahora o diferirlo a un issue.

### 2.4 Convenciones de naming

Seguir las convenciones de Rust:

- **Tipos y traits**: `PascalCase` (ej: `TypeChecker`, `DiagnosticSink`).
- **Funciones, métodos, variables**: `snake_case` (ej: `parse_expr`, `token_kind`).
- **Constantes y statics**: `UPPER_SNAKE_CASE`.
- **Módulos**: `snake_case`.
- **Crates**: `kebab-case` (ej: `hulk-lexer`).
- **Lifetimes**: cortos y descriptivos (`'src`, `'tok`), nunca `'a` a menos que sea realmente genérico.
- **Parámetros de tipo**: `PascalCase` de una letra (`T`, `E`) o palabra corta descriptiva (`Ty`, `Err`).

En el código HULK, los identificadores internos generados por el compilador empiezan con `_` (ej: `_FWrapper_is_odd`), siguiendo la spec.

---

## 3. Control de versiones y Git

### 3.1 El modelo no opera Git

**Prohibido absoluto**: el modelo no ejecuta ningún comando de Git bajo ninguna circunstancia. Esto incluye, sin limitarse a:

- `git add`, `git commit`, `git push`, `git pull`
- `git branch`, `git checkout`, `git switch`, `git merge`
- `git tag`, `git stash`, `git rebase`, `git reset`
- `gh pr create`, `gh release`, ni ningún comando de GitHub CLI
- Edición manual de `.git/` o `.github/workflows/` más allá de lo que la tarea especifique.

**Razón**: el humano lleva el control del flujo GitFlow. El modelo que opere Git puede desordenar ramas, hacer merges prematuros, o perder trabajo en un stash.

### 3.2 Qué sí hace el modelo respecto a Git

- **Leer archivos** en el repositorio (el estado actual del working tree).
- **Modificar archivos** dentro del working tree.
- **Crear archivos nuevos**.
- **Sugerir mensajes de commit** que cumplan con el formato `[SNN.M.T] descripción imperativa` (como ya define cada tarea del pipeline).
- **Asumir que está en la rama correcta**: al iniciar la tarea, el humano ya habrá hecho checkout a la rama `feature/NN.M-...` apropiada. El modelo no verifica en qué rama está.

### 3.3 Consistencia del working tree

- El modelo asume que el working tree está **limpio** al inicio de una tarea (sin cambios sin commitear que no sean de la tarea actual).
- Si el modelo detecta modificaciones inesperadas (archivos modificados que no debería estar tocando), debe **detenerse y preguntar**.
- Al terminar la tarea, el working tree debe tener **solo** los cambios relacionados con la tarea. No "de paso" arreglos de otras cosas, a menos que estén específicamente relacionados (ej: un lint que rompe al compilar).

---

## 4. Comunicación con el humano

### 4.1 Cuándo detenerse y preguntar

El modelo **debe** detenerse y preguntar al humano en cualquiera de estas situaciones:

1. **Ambigüedad en la tarea**: la descripción es interpretable de más de una forma razonable, y la elección afecta el resultado.
2. **Inconsistencia con tareas anteriores**: la tarea actual contradice algo implementado en una tarea previa, o sugiere modificar código de otra sección sin explicación.
3. **Falta de información**: la tarea asume conocimiento que no está en `PIPELINE.md` ni en `rules.md` ni en el código existente.
4. **Conflicto con las reglas de este documento**: la tarea parece exigir algo que viola una regla aquí.
5. **Decisión de diseño no trivial**: hay que elegir entre dos implementaciones con trade-offs distintos y la tarea no especifica cuál.
6. **Descubrimiento de un bug en código preexistente**: encontrar un error real en algo ya mergeado que impide completar la tarea.
7. **La tarea requiere instalar/configurar algo externo**: dependencias de sistema, secrets, servicios externos.
8. **Cobertura de test desciende** por debajo del umbral como consecuencia directa del cambio.
9. **Se necesita un refactor gordo** fuera del alcance anunciado de la tarea.
10. **Error/warning al compilar o testear** que no tiene fix obvio y rápido.

### 4.2 Cómo preguntar

La pregunta al humano debe ser:

- **Específica**: no "¿qué hago?" sino "en la tarea X.Y.Z, el criterio N dice A pero el contexto previo asume B; ¿cuál prevalece?".
- **Enmarcada**: incluir el nombre de la tarea, el archivo o sección relevante.
- **Con opciones**: cuando sea posible, listar 2-4 alternativas con pros y contras breves para que el humano elija rápido.
- **Con una recomendación**: si hay una opción claramente mejor, señalarla; el humano puede aprobar con "sí" rápido.

**Plantilla sugerida**:

```
[TAREA X.Y.Z] Bloqueo: <descripción corta>

Contexto:
<2-3 frases explicando la situación>

Opciones:
A) <opción A> — pros: ..., contras: ...
B) <opción B> — pros: ..., contras: ...

Mi recomendación: <A o B>, porque <razón de 1 línea>.

¿Procedo con <recomendación> o prefieres otra cosa?
```

### 4.3 Cuándo NO preguntar

No preguntar cuando:

- La tarea da suficiente información para decidir.
- Las reglas de este documento resuelven la ambigüedad.
- Es una decisión **trivial** (nombre de una variable local, orden de imports, etc.) donde cualquier elección razonable es correcta.
- El modelo ya preguntó por lo mismo y el humano respondió — no repreguntar.

### 4.4 Reportar al terminar

Al completar una tarea, el modelo informa brevemente:

- **Qué se hizo**: lista de archivos creados/modificados.
- **Qué se validó**: comandos ejecutados (cargo test, cargo clippy) y su resultado.
- **Qué se decidió**: cualquier elección no trivial tomada sin preguntar (con justificación breve).
- **Qué queda pendiente**: si algo se quedó sin hacer por buena razón (ej: "diferido a tarea siguiente").
- **Mensaje de commit sugerido**: el string exacto que el humano puede copiar.

---

## 5. Calidad del código

### 5.1 Debe compilar sin warnings

- `cargo build --workspace` debe pasar sin warnings.
- `cargo clippy --workspace --all-targets -- -D warnings` debe pasar.
- No usar `#[allow(...)]` salvo con justificación en un comentario que explique por qué.
- No usar `#[deny(...)]` para silenciar errores que el código debería arreglar.

### 5.2 Debe estar formateado

- `cargo fmt --all` debe ser idempotente (correr una segunda vez no produce cambios).
- El `rustfmt.toml` del proyecto es autoritativo.
- No formatear a mano contra las reglas del `rustfmt.toml`.

### 5.3 Safety y manejo de `unsafe`

- **Rust**: no usar `unsafe` salvo en situaciones específicas y justificadas (interfaces con C, bit-casting que no se puede expresar safe). Cuando se usa:
  - Envolver en una función safe con precondiciones documentadas.
  - Explicar la invariante mantenida.
  - Testear exhaustivamente.
- **C runtime**: todas las funciones del runtime en C son fundamentalmente `unsafe` desde la perspectiva Rust. Documentar en los comentarios `unsafe extern "C" fn ...` cuáles son las precondiciones.

### 5.4 Panics

- Las fases del compilador **no deben paniquear** ante input malformado del usuario. Todo error de usuario se reporta como `Diagnostic`.
- Los panics solo son aceptables para:
  - Bugs internos del compilador que indican invariantes rotas (usar `debug_assert!` o `unreachable!()` con mensaje descriptivo).
  - Situaciones literalmente imposibles según el diseño (y si ocurren, es bug).
- Nunca usar `.unwrap()` o `.expect()` sin un comentario que explique por qué es seguro, salvo en tests.

### 5.5 Alocaciones y performance

- No preocuparse prematuramente por performance en fases no críticas.
- Pero evitar obvio: no clonar Strings grandes en loops, no reconstruir HashMaps innecesariamente, no hacer O(n²) cuando O(n) es tan legible.
- En caminos calientes (lexer, parser): preferir `&str` sobre `String`, `Cow<str>` cuando hace falta.
- Benchmarks con `criterion` solo cuando la tarea los pida.

### 5.6 Comentarios en el código

- Los comentarios explican el **porqué**, no el **qué**. El código ya dice el qué.
- Comentarios `TODO`: solo si la tarea los pide. Incluir referencia a issue o sección (`// TODO(S06): ...`).
- Comentarios `FIXME`: prohibido — o se arregla, o se convierte en `TODO` con issue.
- Comentarios obvios ("incrementa x en 1") son ruido, no añadirlos.

### 5.7 Rustdoc en API pública

- Todo `pub fn`, `pub struct`, `pub enum`, `pub trait` tiene un doc comment `///`.
- El doc comment explica: qué hace, qué recibe, qué devuelve, condiciones de error.
- Incluir ejemplos en `/// # Examples` cuando aporten (tests de doctest).

---

## 6. Testing

### 6.1 Todo cambio de código lleva test

- Si la tarea agrega una función pública, debe haber al menos un test que la ejercite.
- Si la tarea arregla un bug, debe haber un test de regresión que reproduce el bug antes del fix.
- No mergear código sin tests correspondientes.

### 6.2 Tipos de test

Seguir la pirámide definida en `PIPELINE.md`:

- **Unit tests**: dentro del módulo con `#[cfg(test)]`. Cubren unidades pequeñas.
- **Integration tests**: en `tests/` del crate. Cruzan módulos.
- **Snapshot tests**: con `insta`. Para output comparable (ASTs, tokens, diagnósticos, BANNER IR).
- **Property tests**: con `proptest`. Para invariantes sobre inputs arbitrarios.
- **Fuzzing**: con `cargo-fuzz`. Para robustez ante inputs maliciosos.
- **E2E**: en `tests-e2e/`. Programas HULK completos.

### 6.3 Tests deben pasar

- `cargo test --workspace` debe pasar al terminar la tarea.
- Si el modelo rompe tests existentes con su cambio, debe:
  - Entender por qué (puede ser un test obsoleto que necesita actualización).
  - Preguntar al humano antes de "arreglar" tests ajenos a la tarea.

### 6.4 No hay tests `#[ignore]` sin justificación

- `#[ignore]` se usa solo en casos documentados (ej: la tarea 5.2.4 documenta explícitamente un `#[ignore = "verified in S06"]`).
- Un test que "ignora" un caso que no funciona **no es testing** — o se arregla o se elimina.

### 6.5 Snapshot tests

- Al generar un snapshot por primera vez, revisar el contenido **antes** de aceptarlo.
- Cambios en snapshots requieren revisión: un diff en un snapshot puede indicar un bug o una mejora.
- Ejecutar `cargo insta review` antes de commitear.

### 6.6 Cobertura

- Objetivo: ≥85% en crates de lógica pura (lexer, parser, types, banner), ≥70% en codegen, ≥60% en driver/cli.
- Medición con `cargo tarpaulin`.
- Si una tarea reduce cobertura por debajo del umbral, preguntar al humano si es aceptable (a veces lo es: código de manejo de errores raros).

### 6.7 Test fixture naming

- `test_<cosa>_<escenario>`. Ej: `test_lexer_string_with_escape_newline`.
- En español si es el idioma predominante del proyecto; consistencia dentro del crate.

### 6.8 No tests "felices" solamente

Cada feature tiene tests de:

- Caso feliz (happy path).
- Casos borde (vacío, máximo, mínimo, boundary).
- Casos de error (input inválido, ¿produce el diagnóstico correcto con el span correcto?).
- UTF-8 multibyte donde aplique (lexer, parser, strings).

---

## 7. Documentación

### 7.1 Tres niveles

1. **README.md** (raíz): visión general, instrucciones de build/run, badges, link a `PIPELINE.md`.
2. **doc/seccion-XX-*.md**: documentación detallada de la sección con decisiones técnicas justificadas.
3. **rustdoc** (`///` en el código): documentación de API.

### 7.2 Cada tarea actualiza la documentación

- Si la tarea introduce una decisión técnica, debe aparecer en `doc/seccion-XX.md` con: qué se eligió, alternativas consideradas, justificación, ejemplo de código.
- Si la tarea introduce API pública nueva, debe tener rustdoc.
- Si la tarea completa una feature user-visible, actualizar el README (checklist de features).

### 7.3 Formato de decisión técnica en `doc/`

Seguir el template del `PIPELINE.md` (sección "Sistema de documentación"). No improvisar formatos distintos.

### 7.4 Idioma

- **Código y comentarios de código**: inglés (convención Rust + internacionalización).
- **Documentación en `/doc`**: español (el proyecto es didáctico y en español).
- **Mensajes de diagnóstico**: inglés (consistencia con el ecosistema Rust y los ejemplos de la spec).
- **Mensajes de commit**: español o inglés, consistente dentro del proyecto (el humano decide).

### 7.5 CHANGELOG

- Cada cambio relevante se registra en `CHANGELOG.md` bajo `## [Unreleased]`.
- Categorías: `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`.
- El humano promueve `[Unreleased]` a una versión concreta al hacer tag.

### 7.6 Diagramas

- Solo cuando realmente aporten. No ponerlos "por ponerlos".
- Preferir Mermaid (renderiza en GitHub) sobre ASCII art.
- Actualizar los diagramas si la arquitectura cambia.

---

## 8. Manejo de errores y robustez

### 8.1 Diagnósticos, no excepciones

- Todos los errores reportables al usuario pasan por `hulk-diagnostics`.
- Nunca un `eprintln!("error: ...")` directo; siempre construir un `Diagnostic` con span.
- Los diagnósticos tienen: severidad, mensaje, código (`E001`, etc.), labels con spans, notas, sugerencias.

### 8.2 Error recovery

- El lexer y parser **no abortan** al primer error. Recolectan múltiples errores en el `DiagnosticSink`.
- Tras un error, usar estrategia de recovery (panic mode con anchor set, o lookahead) para seguir.
- El objetivo: en una corrida de compilación, reportar **todos** los errores que se pueda, no solo el primero.

### 8.3 Spans siempre correctos

- Todo `Diagnostic` tiene un span primario apuntando al código ofensor.
- Los spans nunca son "dummy" en código productivo (solo en nodos AST sintéticos internos).
- Verificar en tests que los spans reportados coincidan con lo esperado.

### 8.4 Mensajes útiles

- Estilo `rustc`: explicar qué está mal, dónde está, y cuándo sea posible, sugerir el fix.
- "expected X, found Y" es el mínimo. "help: did you mean Z?" es lo deseable.
- No mensajes vagos tipo "syntax error" o "type error".

### 8.5 Nunca silenciar errores

- `let _ = function_that_returns_result();` está prohibido salvo justificación.
- `.ok()` para descartar errores está prohibido salvo justificación.
- Si un error no se puede manejar en ese punto, propagarlo con `?`.

---

## 9. Gestión de dependencias

### 9.1 Usar workspace dependencies

- Todas las versiones de crates externos se centralizan en el `Cargo.toml` raíz (`[workspace.dependencies]`).
- Cada crate las referencia con `nombre.workspace = true`.
- Cambiar la versión de una dep se hace en **un solo lugar**.

### 9.2 Añadir una dependencia nueva

Antes de añadir un crate externo:

1. **Verificar** que no existe ya otra solución en el proyecto.
2. **Evaluar** si la funcionalidad realmente justifica la dep, o es implementable en pocas líneas.
3. **Verificar licencia** compatible (el `deny.toml` define las permitidas).
4. **Verificar mantenimiento**: último release reciente, autor conocido, downloads razonables.
5. **Preguntar al humano** si la dep no estaba ya en `[workspace.dependencies]`.

### 9.3 No duplicar funcionalidad

- Si dos crates del workspace necesitan la misma función, **extraerla** a un crate común (o a un crate más interno) en lugar de duplicar.
- Cuidado: no violar la regla de capas por "reuso" (ej: no meter lógica de parser en `hulk-ast` solo para reusarla).

### 9.4 Features de crates

- Usar features de crates con parsimonia.
- `default-features = false` cuando no se necesiten todas las features (reduce compile time).

---

## 10. Ejecución de tareas

### 10.1 Orden correcto al ejecutar una tarea

1. **Leer** la tarea completa en `PIPELINE.md`: contexto, objetivo, archivos, descripción, criterios, tests, doc, commit sugerido.
2. **Leer** los archivos existentes que la tarea va a tocar.
3. **Leer** los archivos de tareas anteriores relacionadas (ver "contexto previo" de la tarea).
4. **Identificar ambigüedades**: si hay, preguntar antes de escribir código.
5. **Planear** mentalmente la implementación.
6. **Escribir** el código.
7. **Escribir** los tests (en paralelo al código — idealmente TDD, pero no es obligatorio).
8. **Ejecutar** tests: `cargo test -p <crate>` del crate afectado.
9. **Ejecutar** lint y formato: `cargo clippy -- -D warnings` y `cargo fmt`.
10. **Ejecutar** el workspace entero: `cargo test --workspace` para verificar que no rompió nada más.
11. **Actualizar** la documentación.
12. **Reportar** al humano (ver sección 4.4).

### 10.2 Ningún paso se salta

- Si un paso falla, arreglarlo antes de continuar.
- Si un paso no aplica (ej: la tarea es solo doc), explicitarlo al reportar.
- No reportar "tarea completa" si hay un test rojo o clippy warning pendiente.

### 10.3 Tareas del tipo "merge de sección"

Las tareas sufijadas con "Merge de la sección N" (ej: 1.3.2) son **responsabilidad del humano**, no del modelo. El modelo solo:

- Verifica que todos los criterios de la subsección de tests exhaustivos pasan.
- Verifica que la documentación está completa.
- Reporta al humano: "Sección N lista para merge a develop. Checklist: [...]".

El humano hace el merge.

### 10.4 Implementaciones incrementales

- Si la tarea es grande, el modelo puede implementar en pasos, validando cada uno antes del siguiente.
- No commitear en medio (recordar: el modelo no usa Git). Simplemente implementar, testear sub-conjunto, seguir.
- Al final, todo debe pasar junto.

---

## 11. Situaciones especiales

### 11.1 Ambigüedades de la spec de HULK

La spec de HULK (`Hulk.md`) tiene inconsistencias conocidas. Algunas ya están resueltas en `PIPELINE.md` (ej: `/` vs `\`, `^` vs `**`). Si el modelo encuentra una **nueva** inconsistencia no resuelta:

1. **NO** inventar una resolución.
2. Anotarla con detalle (citar las líneas de `Hulk.md` en conflicto).
3. Proponer 2-3 resoluciones con trade-offs.
4. Preguntar al humano.

### 11.2 Bugs encontrados en código previo

Si al leer código de secciones anteriores (ya mergeadas a `develop`) el modelo encuentra un bug:

1. **NO** arreglarlo silenciosamente dentro de la tarea actual.
2. **NO** ignorarlo si afecta la tarea actual.
3. Describir el bug al humano (qué archivo, línea, síntoma) y preguntar: ¿lo arreglo aquí, o lo anoto como issue y continuo?

### 11.3 Cambios que romperían el contrato de otra capa

Si la tarea requiere agregar un campo a un tipo público de una capa anterior (ej: agregar `span_end` a `Span` en sección 7):

1. Evaluar si es necesario o si hay una alternativa menos invasiva.
2. Si es necesario, implementar el cambio **y** actualizar todos los consumidores (idealmente en la misma tarea).
3. Si son muchos consumidores, partir en subtareas y **preguntar al humano** cómo proceder.

### 11.4 Feature flag, cfg(test), entornos

- No usar `#[cfg(feature = "...")]` salvo que la tarea lo pida explícitamente.
- `#[cfg(test)]` es para código solo usado en tests; legítimo.
- `#[cfg(debug_assertions)]` para validaciones que solo corren en debug; legítimo pero documentarlo.

### 11.5 Interacción con LLVM y código C

- El modelo **no** instala LLVM. Asume que el humano ya lo hizo.
- Si `cargo build` falla por LLVM no disponible, reportarlo al humano, no intentar bypass.
- El runtime en C se compila con `make`. Si `make` falla, reportar el error, no intentar workarounds.

### 11.6 Cuando se necesita ejecutar un programa HULK

Durante tests E2E y al final de algunas secciones, los tests ejecutan programas HULK. El modelo puede:

- Ejecutar `cargo test` que los corre automáticamente.
- Ejecutar `cargo run -- run examples/foo.hulk` para validar manualmente.

Si el programa se cuelga o da output inesperado, reportar con el programa exacto, output obtenido, y output esperado.

---

## 12. Anti-patrones prohibidos

Esta lista no es exhaustiva, pero captura los errores más comunes.

### 12.1 Código

- `unsafe` sin justificación.
- `.unwrap()` / `.expect()` en código productivo sin comentario que explique por qué es seguro.
- `panic!()` en lugar de diagnóstico para error de usuario.
- Strings mágicos repetidos (usar constantes).
- Números mágicos (usar constantes con nombre).
- Clonar `String` innecesariamente (usar `&str` o `Cow`).
- `impl Drop` con lógica compleja (preferir patrones más explícitos).
- `mut` en parámetros `&mut T` cuando se puede tomar ownership y devolver.
- Variables de una letra fuera de loops cortos.
- Comentarios que repiten lo que dice el código.
- Funciones con más de 5 niveles de indentación.
- Uso de `unsafe_trait_impl!` u otras macros exóticas sin necesidad.

### 12.2 Arquitectura

- Dependencias circulares entre módulos.
- Dependencias que violan la regla de capas.
- "God objects" / "God structs" con muchos fields y responsabilidades.
- Lógica de negocio en `main.rs` del CLI (debe vivir en `hulk-driver`).
- State global mutable (salvo allocated list del GC que es inherentemente global).
- Acoplamiento entre el lexer y el parser más allá de la interfaz de `Token`.

### 12.3 Testing

- Tests que dependen de orden de ejecución.
- Tests que dependen de side effects (filesystem, red, time).
- Tests que compartan state mutable.
- Assertions vagas (`assert!(result.is_ok())` sin más).
- Tests "smoke" que solo verifican que no paniquea.
- Tests con nombres genéricos (`test1`, `test_it_works`).

### 12.4 Git y flujo

- Cualquier operación de Git por parte del modelo.
- Mezclar múltiples tareas en un mismo "bloque" de cambios.
- Trabajar sobre `develop` o `main` directamente.
- Cambiar archivos que no están listados en la tarea actual.

### 12.5 Documentación

- Documentar el "qué" del código en lugar del "por qué".
- Copiar grandes bloques de código a la doc (enlazar a la línea en su lugar).
- Doc desactualizada respecto al código — preferir eliminarla a mantenerla mintiendo.
- Redactar en primera persona ("yo implementé...") en lugar de impersonal.

---

## 13. Checklist pre-entrega

Antes de reportar "tarea completa", el modelo verifica:

### 13.1 Código

- [ ] Todos los criterios de aceptación de la tarea se cumplen.
- [ ] Los archivos listados en la tarea están creados/modificados (y nada más que esos, salvo justificación).
- [ ] `cargo build --workspace` pasa.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` pasa.
- [ ] `cargo fmt --all --check` pasa.
- [ ] No hay `TODO`/`FIXME`/`unimplemented!()` nuevos sin justificación.
- [ ] No hay `println!`/`dbg!` olvidados.
- [ ] No hay `unsafe` nuevo sin comentario de justificación.

### 13.2 Tests

- [ ] `cargo test --workspace` pasa.
- [ ] Los tests que la tarea pedía están escritos.
- [ ] Los tests cubren happy path + casos borde + errores.
- [ ] No se agregaron `#[ignore]` sin justificación.
- [ ] Snapshots nuevos revisados.

### 13.3 Documentación

- [ ] Rustdoc agregada a nueva API pública.
- [ ] `doc/seccion-XX-*.md` actualizado con las decisiones de esta tarea.
- [ ] `CHANGELOG.md` actualizado bajo `## [Unreleased]` si el cambio es user-visible.
- [ ] README actualizado si corresponde.

### 13.4 Reglas

- [ ] No se ejecutó ningún comando de Git.
- [ ] La arquitectura clean se respetó (sin dependencias "hacia arriba").
- [ ] Las decisiones no triviales se comunicaron o preguntaron al humano.
- [ ] El working tree tiene solo los cambios de esta tarea.

### 13.5 Reporte

- [ ] Lista de archivos creados/modificados lista para pegar.
- [ ] Mensaje de commit sugerido (en el formato `[SNN.M.T] descripción`).
- [ ] Resumen de decisiones no triviales.
- [ ] Cualquier bloqueo o pendiente claramente marcado.

---

## Apéndice A — Ejemplo completo de sesión

Para ilustrar cómo debería verse una sesión ideal del modelo:

**Humano**: "Implementa la tarea 3.1.1 del pipeline (TokenKind)."

**Modelo** (lee la tarea, lee `rules.md`, lee código existente):

> Antes de comenzar, quiero confirmar dos cosas:
>
> 1. La tarea lista `Match`, `Case`, `Default` como keywords para el pattern matching de macros. ¿Se agregan ya aquí aunque el parsing no se implemente hasta 4.5.4? Mi recomendación: sí, para evitar que el lexer los trate como `Ident` y tener problemas después.
> 2. El token `Caret` (`^`) representa exponenciación. Confirmo que no agregamos `**` como alias (según la decisión documentada en PIPELINE sobre ambigüedad de la spec).
>
> Si OK ambos puntos, procedo.

**Humano**: "Sí a los dos."

**Modelo** (implementa, escribe tests, ejecuta):

> Tarea 3.1.1 completada.
>
> **Archivos creados/modificados**:
> - `crates/hulk-tokens/src/kind.rs` (nuevo, 180 líneas)
> - `crates/hulk-tokens/src/lib.rs` (modificado, re-export de `TokenKind`)
> - `crates/hulk-tokens/tests/token_kind.rs` (nuevo, 12 tests)
> - `doc/seccion-03-lexer.md` (modificado, +2 decisiones documentadas)
>
> **Validación**:
> - `cargo test -p hulk-tokens`: 12/12 passed.
> - `cargo clippy -p hulk-tokens -- -D warnings`: clean.
> - `cargo fmt --check`: clean.
>
> **Decisiones no triviales**: ninguna fuera de las preguntadas.
>
> **Mensaje de commit sugerido**: `[S03.1.1] Define TokenKind con todos los tokens de HULK`
>
> Listo para que commitees cuando quieras.

---

## Apéndice B — Escalabilidad del documento

Este `rules.md` es un documento vivo. Si al avanzar el proyecto:

- Aparecen patrones que se repiten y no están cubiertos → agregar regla.
- Una regla resulta contraproducente → discutirla y modificarla (con consenso del humano).
- Se abandona una decisión → actualizar para reflejar realidad.

Los cambios a `rules.md` los hace el humano, no el modelo. Si el modelo cree que una regla debe cambiar, lo propone y el humano decide.

---

**Fin del documento de reglas.**
