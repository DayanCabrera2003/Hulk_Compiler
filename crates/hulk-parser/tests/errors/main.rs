//! Suite de errores esperados (subsesión 5.2).
//!
//! Cada módulo ejercita una categoría de errores lexicográficos o sintácticos
//! y verifica tanto la presencia del diagnóstico como su contenido (substring
//! del mensaje) y la correcta recuperación del parser cuando aplica.

#![allow(clippy::missing_panics_doc)]

mod common;

mod lexical;
mod multi_error;
mod syntactic;
