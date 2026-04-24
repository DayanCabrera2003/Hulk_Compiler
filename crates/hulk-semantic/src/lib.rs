//! Semantic analysis for HULK: name resolution, symbol tables, and
//! lightweight validation passes over the AST.

mod resolver;
mod symbols;
mod validation;

#[cfg(test)]
mod tests;

pub use resolver::*;
pub use symbols::*;
