//! Per-construct desugaring transformations applied by [`super::Desugarer`].
//!
//! Each submodule owns a cohesive group of `impl Desugarer` methods for a
//! single high-level HULK construct:
//!
//! - [`for_loop`]: `for (x in e) body` → `let` + `while` + iterator protocol.
//! - [`string_concat`]: `a @@ b` → `a @ " " @ b`.
//! - [`lambda`]: lambdas and function-reference arguments into synthetic
//!   invocable types.

pub(crate) mod for_loop;
pub(crate) mod lambda;
pub(crate) mod string_concat;
