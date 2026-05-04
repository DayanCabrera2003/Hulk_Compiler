# HULK Compiler

[![CI](https://github.com/DayanCabrera2003/hulk_compiler/actions/workflows/ci.yml/badge.svg)](https://github.com/DayanCabrera2003/hulk_compiler/actions/workflows/ci.yml)
[![Coverage](https://github.com/DayanCabrera2003/hulk_compiler/actions/workflows/coverage.yml/badge.svg)](https://github.com/DayanCabrera2003/hulk_compiler/actions/workflows/coverage.yml)

Compilador del lenguaje HULK (Havana University Language for Kompilers).

## Build

Requisitos:
- Rust 1.75.0 o superior
- LLVM 17

### Instalación de LLVM
- Linux: `sudo apt install llvm-17-dev clang-17`
- macOS: `brew install llvm@17`
- Windows: Ver notas en la documentación y posibles limitaciones.

Si es necesario, configura la variable de entorno `LLVM_SYS_170_PREFIX`.

### Compilar

```sh
cargo build --release
```

### Ejecutar

```sh
./target/release/hulk run archivo.hulk
```

Más detalles en `PIPELINE.md` y en la carpeta `doc/`.

## Herramientas de calidad

- Formato: `cargo fmt --all --check`
- Linter: `cargo clippy --workspace --all-targets -- -D warnings`
- Auditoría de licencias/vulnerabilidades: `cargo deny check`

Instala `cargo-deny` con:

```sh
cargo install cargo-deny
```

Las tres herramientas se ejecutan automáticamente en CI.

## HULK features supported

- [x] Expressions
- [ ] Variables
- [ ] Functions
- [ ] Control flow (if, while, for)
- [ ] OOP (classes, inheritance)
- [ ] Pattern matching
- [ ] Modules
- [ ] Type inference
- [ ] Error handling
- [ ] Macros
- [ ] Desugaring
- [ ] Code generation (LLVM)
- [ ] CLI interface

## Meta

- [PIPELINE.md](PIPELINE.md): Detalle del pipeline y tareas.
- [CHANGELOG.md](CHANGELOG.md): Historial de cambios.
- [LICENSE](LICENSE): MIT License.
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md): Contributor Covenant.
