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
