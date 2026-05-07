//! Suite de programas HULK válidos (subsesión 5.1).
//!
//! Cada módulo embebe con `include_str!` uno de los programas de `examples/`
//! en la raíz del repo, lo parsea y verifica la estructura del AST resultante.
//! Ninguno de estos tests debe producir diagnósticos: si lo hace, es un
//! regression y se investiga.
//!
//! Además de los 13 programas del PIPELINE 5.1, el módulo `combined` cubre
//! variantes cruzadas que no caben en un único `.hulk`.

#![allow(clippy::missing_panics_doc)]

mod common;

mod arithmetic;
mod classes;
mod combined;
mod prelude;
mod conditionals;
mod functions;
mod functors;
mod hello;
mod iterables;
mod let_scoping;
mod loops;
mod macros;
mod protocols;
mod strings;
mod vectors;
