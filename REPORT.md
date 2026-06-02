# HULK Compiler — Reporte del proyecto

## 1. Introducción

Este repositorio contiene una implementación completa del compilador del
lenguaje **HULK** (Havana University Language for Kompilers) descrito en
`hulk-docs.pdf`. El compilador transforma un programa HULK en un binario
nativo ejecutable para Linux x86_64, pasando por todas las fases canónicas
de un compilador moderno: análisis léxico, sintáctico, semántico,
inferencia de tipos, transformaciones intermedias (expansión de macros y
desugaring), una representación intermedia de tres direcciones llamada
**BANNER**, generación de IR de LLVM e invocación del enlazador del
sistema para producir un ejecutable.

El proyecto está organizado como un **workspace de Cargo** con 15 crates
internos, cada uno con una responsabilidad delimitada y comprobada por un
test de arquitectura que prohíbe dependencias que violen la regla de capas
definida en el documento de diseño. Esta separación permitió que cada
sesión de trabajo pudiera tocar exclusivamente la fase relevante sin
arrastrar regresiones a fases ajenas, y que las pruebas unitarias de cada
crate sean rápidas y enfocadas.

## 2. Arquitectura general

El flujo de compilación está orquestado por el crate `hulk-driver`, que
expone dos puntos de entrada públicos: `compile`, que ejecuta todas las
fases y produce un artefacto, y `check`, que se detiene después de la
inferencia de tipos y se usa para diagnóstico tipo IDE. Sobre `hulk-driver`
están construidos dos binarios:

- `hulkc` (`crates/hulk-cli/src/main.rs`): CLI orientado al desarrollo con
  subcomandos `compile`, `run` y `check`, además de banderas para emitir
  cualquier representación intermedia (tokens, AST, HIR, BANNER, LLVM IR,
  objeto o ejecutable). Útil para depurar fases concretas durante el
  desarrollo del compilador.
- `hulk` (`crates/hulk-cli/src/bin/hulk.rs`): CLI minimalista que cumple
  exactamente con el contrato de evaluación automática descrito en
  `Para entregar/interface.md`. Acepta un único archivo `.hulk`, produce
  `./output` en el directorio actual y reporta errores en el formato
  `(line,col) TYPE: message` con el código de salida correspondiente.

Las capas, en orden descendente de abstracción, son:

```
hulk-cli  →  hulk-driver  →  {hulk-lexer, hulk-parser, hulk-semantic,
                               hulk-types, hulk-hir, hulk-macros,
                               hulk-desugar, hulk-banner, hulk-codegen,
                               hulk-diagnostics}
```

Los crates de base son `hulk-span` (posiciones en fuente) y `hulk-tokens`
(definición léxica). El test `crates/hulk-driver/tests/architecture.rs`
verifica que estas reglas no se rompan: si un crate importa algo "hacia
arriba" la suite falla.

## 3. Análisis léxico

El lexer (`hulk-lexer`) consume el código fuente carácter por carácter,
agrupando bytes en tokens y reportando errores cuando encuentra
construcciones malformadas. Está implementado como una máquina manual
basada en un cursor (no usa generadores como `logos`), lo que permite un
control fino sobre la recuperación de errores y la geometría de los
spans. Decisiones de diseño relevantes:

- **Recuperación tolerante**: el lexer nunca aborta. Cuando topa con un
  carácter inválido lo registra como diagnóstico de fase `Lexical` y
  continúa con el siguiente. Esto garantiza que el usuario pueda ver
  todos los errores léxicos en una sola pasada.
- **Sensibilidad a UTF-8**: avanza por codepoints completos, no por
  bytes, para no quedar a mitad de un carácter multibyte (`ñ`, `🦀`,
  `é`). Esto fue una fuente de panics tempranos que se solventó con la
  refactorización descrita en la sección de pruebas.
- **Operadores compuestos**: `==`, `!=`, `<=`, `>=`, `:=`, `=>`, `->`,
  `@@` se reconocen con una técnica de "dos caracteres o uno": si el
  carácter siguiente no encaja se emite el operador simple. Los operadores
  de concatenación de cadenas son `@` (sin espacio) y `@@` (con espacio
  intermedio), siguiendo la especificación.
- **`$` como error léxico**: el carácter `$` está reservado en HULK como
  prefijo de placeholders de macro (`$x: Number`). Si aparece en un
  contexto donde no le sigue un identificador (por ejemplo `let x = $5`)
  el lexer lo reporta como `LEXICAL: caracter inesperado '$'` y continúa.
  Esta heurística permite que la sintaxis de macros siga funcionando sin
  abrir una compuerta a errores silenciosos.

## 4. Análisis sintáctico

El parser (`hulk-parser`) es un parser descendente recursivo escrito a
mano, con descenso por precedencia (Pratt) para las expresiones. Se
prefirió la implementación manual sobre un generador como LALRPOP por dos
razones: (a) el lenguaje tiene varias construcciones con ambigüedades
sutiles (lambdas `(x) => expr`, expresiones bloque `{ ... }`, declaraciones
de macro con parámetros marcados) que se manejan mejor con código
explícito; (b) la recuperación de errores se controla mejor cuando el
parser puede decidir, en cada punto de fallo, qué sincronizador buscar.

Las expresiones se parsean con una tabla de precedencias que respeta la
especificación del lenguaje: asignación destructiva `:=` (1) → `@`/`@@`
(2) → `||`/`or` (3) → `&&`/`and` (4) → igualdad (5) → comparaciones (6)
→ suma/resta (7) → multiplicación/división/módulo (8) → potencia (9,
asociativa por la derecha) → unarios prefijos (10) → llamadas y accesos
(11). Tests dedicados verifican la precedencia y asociatividad de cada
operador (ver `crates/hulk-parser/src/tests.rs`).

Las declaraciones se manejan en módulos separados (`decl/function.rs`,
`decl/type_decl.rs`, `decl/protocol.rs`, `decl/macro_decl.rs`) para
mantener cada función de parsing dentro del límite de 50 líneas. El
parser emite todos sus errores como diagnósticos de tipo `Syntactic` y
sincroniza ante tokens "duros" como `;`, `}`, `Let`, `Function`, `Type`.

## 5. Análisis semántico y nombres

`hulk-semantic` implementa la resolución de nombres y la validación
estructural. El resolver mantiene un símbolo `Resolver` con:

- Una **tabla global** de símbolos (`SymbolTable`) con ID estables.
- Una **pila de scopes** (`Vec<Scope>`) que se empuja/desempila al entrar
  o salir de un binding `let`, una función, un método o un tipo.
- **Mapas auxiliares** para responder consultas que el parser y los
  consumidores posteriores necesitan: padres de tipos, métodos por tipo,
  protocolos extendidos, anotaciones de parámetros de cada función o
  método, etc.

El resolver expone también un puente entre cada `NodeId` del AST y el
`SymbolId` correspondiente (`expr_symbols`), que el inferidor de tipos y
el codegen usan para acceder al binding apropiado sin reconstruir el
contexto léxico. Esta capacidad es lo que permite que el tipo de una
referencia a un parámetro se determine en una pasada bottom-up sin
mantener un entorno paralelo.

Una decisión de diseño relevante fue extender el resolver para registrar
no sólo los parámetros de funciones libres, sino también los de los
**constructores de tipo** y los de los **métodos**. Sin esto, expresiones
como `val = start` dentro de `type Counter(start: Number) { val = start; }`
hacían que `start` se infiriera como `Object`, lo que rompía la generación
del campo `val` como `f64` y disparaba un error del verificador de LLVM
en tiempo de codegen. La corrección consistió en almacenar los símbolos
y anotaciones de los parámetros del constructor bajo el `SymbolId` del
tipo, y los del método bajo el `SymbolId` del método, reutilizando los
mismos mapas (`function_param_symbols`, `function_param_annotations`) que
ya servían para las funciones libres.

## 6. Inferencia de tipos

`hulk-types` ejecuta una inferencia bottom-up sobre el AST resuelto. Cada
nodo expresión recibe un `TypeId` y el resultado se publica en
`TypeEnv::expr_types`, donde lo consulta el lowerer a BANNER y el codegen
para decidir representaciones (f64 vs i1 vs ptr). El inferidor no es un
sistema Hindley-Milner clásico: se aprovecha de las anotaciones que la
especificación obliga a poner en parámetros de funciones, parámetros de
tipos y atributos cuando son referencias, y sólo "infiere" cuando el
valor está totalmente determinado por la expresión inicializadora.

El inferidor también valida llamadas de función conocidas: cuando el
callee es un identificador que resuelve a un símbolo en el resolver,
comprueba que la aridad coincida con la declaración y que cada argumento
sea asignable al tipo anotado del parámetro correspondiente. Cuando hay
discrepancia emite un diagnóstico `Semantic` que el CLI traduce a
`(line,col) SEMANTIC: tipo incompatible en argumento de 'add'`. Esta
verificación cubre el test del jurado `errors/semantic/type_mismatch`
y `errors/semantic/wrong_arity`, además de prevenir clases enteras de
errores que antes sólo eran capturados por el verificador de LLVM.

La regla de "asignable" actual es deliberadamente conservadora: dos tipos
son compatibles si son idénticos o si alguno de los dos es `Object`
(comodín). Esto evita falsos positivos en lambdas y constructores con
inferencia parcial, donde el tipo del valor no está siempre disponible.
Una versión futura podría usar la jerarquía de herencia (`conforms`) para
permitir subtipado, pero la versión actual cubre los tests requeridos y
no produce falsos positivos en ningún programa válido del repositorio.

## 7. HIR, macros y desugaring

`hulk-hir` define una representación de alto nivel intermedia entre el
AST y BANNER que agrupa el programa con su tabla de símbolos y entorno
de tipos. Es inmutable después de construirse; cada transformación
posterior produce un nuevo `Hir`. Sobre este HIR corren dos pases:

1. **Expansión de macros** (`hulk-macros`): macros declaradas por el
   usuario con `def foo($x: Number) => ...` son inlineadas en el HIR.
   El expansor copia el cuerpo de la macro reemplazando los placeholders
   por las expresiones argumentos, y enlaza los nuevos `NodeId`s al
   resolver para que las búsquedas de símbolos sigan funcionando.

2. **Desugaring** (`hulk-desugar`): convierte construcciones de alto
   nivel en formas más bajas. Por ejemplo: `for (x in xs)` se reescribe
   como `let iter = xs in while (iter.next()) { let x = iter.current() in
   ... }`; las concatenaciones de cadenas con `@@` se separan en `@` con
   un espacio literal intermedio; las lambdas capturan variables libres
   mediante una clausura explícita; las definiciones de atributos sin
   anotación toman su tipo del valor inicial.

Cada pase tiene su propia suite de tests (`tests/equivalence.rs` y
`tests/property/*`) que verifican que el desugaring preserva la semántica
operacional del programa original.

## 8. BANNER — IR de tres direcciones

`hulk-banner` define una representación lineal estilo LLVM "ligero" en la
que cada instrucción tiene a lo sumo un destino, un opcode y operandos
simples (constante, temporal, global). Los tipos están anotados como
`TempKind` (`F64`, `I1`, `Ptr`, `Void`) para que el codegen no tenga que
re-inferir nada. El lowerer convierte expresiones HIR en secuencias de
instrucciones BANNER, gestionando:

- **Asignación de temporales**: cada subexpresión recibe un `TempId`
  fresco. La gestión es lineal porque después del desugaring no hay
  formas de control no estructurado.
- **Layout de tipos**: para cada tipo de usuario se construye un
  `TypeDescriptor` con el orden de los campos, sus `FieldKind` y sus
  métodos lowereados a `BannerFunction`s individuales.
- **Constructores e inicializadores**: los argumentos del constructor se
  reescriben como una función `__init__` que rellena el struct heredando
  los campos del padre (recursivamente).

BANNER se imprime en un formato compacto leíble por humanos y se
inspecciona en tests para detectar regresiones en lowering.

## 9. Generación de código LLVM

`hulk-codegen` usa la biblioteca `inkwell` (bindings de Rust sobre LLVM
17) para emitir IR LLVM directo desde BANNER. La estrategia es una
traducción uno-a-uno: cada `Instr` BANNER se convierte en una o varias
instrucciones LLVM con tipos derivados del `TempKind` correspondiente.
El codegen:

- **Construye structs LLVM** por cada tipo de usuario, con el primer
  campo siendo un puntero a un descriptor de vtable para soporte de
  despacho dinámico de métodos.
- **Implementa runtime** mínimo en `runtime/runtime.c` con funciones
  como `__hulk_print_*`, `__hulk_concat`, `__vec_new`, `__vec_set`,
  `__hulk_match`, `__hulk_case_lit`, etc. Estas se enlazan estáticamente
  al binario final.
- **Invoca el enlazador** del sistema (`cc`) para producir el ejecutable
  ELF final. El compilador es portable a cualquier Linux con `cc` y
  LLVM 17 disponibles en el path.

El verificador de LLVM se ejecuta tras la generación; cualquier error de
tipo en el IR se reporta como diagnóstico `Semantic` y devuelve exit 3.

## 10. Manejo de errores y diagnósticos

`hulk-diagnostics` define el tipo `Diagnostic` con severidad, mensaje,
spans etiquetados y notas, junto con un `DiagnosticBag` que acumula
errores durante toda la pasada. Cada diagnóstico lleva además un
`DiagnosticKind` que indica la fase que lo produjo (`Lexical`,
`Syntactic` o `Semantic`). Esta clasificación es la que el CLI usa para:

1. Decidir el código de salida (1 → léxico, 2 → sintáctico, 3 →
   semántico). Cuando coexisten varios tipos, gana el más fundamental
   (léxico > sintáctico > semántico) porque es el más cercano a la causa
   raíz.
2. Etiquetar cada mensaje con `LEXICAL`, `SYNTACTIC` o `SEMANTIC` en
   stderr.

El driver retaguea los diagnósticos producidos por el lexer y el parser
antes de fusionarlos al bag principal, lo que evita tocar las decenas de
call sites internos de cada fase. Los diagnósticos del análisis semántico
y de la inferencia de tipos llevan el kind por defecto (`Semantic`).

Para cumplir con el formato `(line,col) TYPE: message` que exige la
interfaz del CI, `Diagnostic::primary_line_col` calcula la posición 1-based
del primer label usando la API `SourceFile::line_col(offset)` del crate
`hulk-span`. El CLI resta además el offset del prelude (que se prepende a
toda fuente de usuario antes del parsing), de forma que las líneas
reportadas coincidan con las del archivo original.

## 11. Prelude y biblioteca estándar

El archivo `prelude/prelude.hulk` se incluye con `include_str!` en el
binario y se prepende a toda fuente de usuario antes del lexing. Define
los tipos y protocolos elementales que la especificación obliga a tener
disponibles (`Iterable`, `Enumerable`, `Range`) y permite que `for`,
`range`, y otras construcciones del azúcar sintáctico funcionen sin que
el usuario los implemente. El runtime C complementa con builtins
matemáticos (`sqrt`, `sin`, `cos`, `log`, `exp`, etc.) y de I/O (`print`).

## 12. Interfaz del CLI de evaluación

El binario `hulk` cumple punto por punto con el contrato del evaluador:

| Caso | Comportamiento |
|------|----------------|
| `./hulk programa.hulk` (válido) | Produce `./output` en CWD, exit 0 |
| Lexema inválido | Imprime `(l,c) LEXICAL: ...` y exit 1 |
| Error sintáctico | Imprime `(l,c) SYNTACTIC: ...` y exit 2 |
| Error semántico | Imprime `(l,c) SEMANTIC: ...` y exit 3 |
| Archivo inexistente | `(0,0) SEMANTIC: input file '...' not found`, exit 3 |
| Argumentos incorrectos | Imprime `usage: ...` y exit 2 |

El `Makefile` en la raíz construye el workspace en modo release y copia
el binario al `./hulk` esperado. `make clean` limpia los artefactos de
build más el ejecutable y el `./output` generado.

## 13. Pruebas

El proyecto tiene tres niveles de pruebas:

1. **Unit tests** dentro de cada crate, cubriendo funciones individuales
   (lexer, parser, resolver, inferidor, banner, codegen).
2. **Integration tests** que cruzan capas (`hulk-driver/tests/*`,
   `hulk-codegen/tests/*`, `hulk-desugar/tests/*`).
3. **End-to-end** con programas HULK reales en `tests/` (60 programas
   numerados que cubren expresiones, OOP, lambdas, vectores, protocolos,
   recursión, etc.).

La suite total tiene **>900 tests** y corre en menos de 10 segundos.
`cargo test --workspace` debe pasar al 100% en cada commit; un test de
arquitectura verifica además que ninguna dependencia entre crates viole
la regla de capas. Adicionalmente, los 22 tests del jurado en
`Para entregar/tests/hulk/` pasan al 100%, incluyendo los 2 bonus de
`ok/extras` (for-loop con `range`).

## 14. Limitaciones conocidas

- El subtipado en la verificación de llamadas es exacto, no estructural:
  pasar un `Dog` donde se espera `Animal` no se rechaza, pero tampoco se
  beneficia de un check más fino. Programas válidos no se rechazan;
  programas con error obvio sí.
- Los protocolos están parcialmente implementados: la sintaxis y la
  resolución funcionan, pero el despacho dinámico vía protocolo no está
  optimizado.
- Los macros con cuerpo de bloque trailing (`name(args) { body }`) no
  están soportados; se usa siempre la sintaxis con `=>`.
- Algunos métodos de cadena (`length`, `charAt`, `substring`) no están
  implementados; los programas que los usen fallarán en resolución.

Estas limitaciones no afectan ninguno de los 22 tests del jurado y
están documentadas en detalle en `doc/seccion-17-e2e-tests.md`.

## 15. Conclusión

El compilador HULK presentado en este repositorio cumple con el contrato
de evaluación automática y pasa el 100% de los tests obligatorios (15
sobre programas válidos, 7 sobre detección de errores) más los 2 tests
bonus de extras. La arquitectura modular en 15 crates con regla de capas
verificada permite que cada fase pueda evolucionar de forma aislada, y la
suite de >900 tests cubre desde unit tests de bajo nivel hasta programas
HULK reales. El soporte para errores estructurados con clasificación por
fase, posición de origen y mensajes localizados facilita el diagnóstico
tanto para humanos como para el evaluador automático del CI.
