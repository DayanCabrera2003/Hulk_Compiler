//! LLVM code generator for the HULK compiler.
//!
//! Translates a desugared [`Hir`](hulk_hir::Hir) into a native executable
//! by first lowering to the BANNER three-address IR and then emitting LLVM IR
//! via the `inkwell` crate.
//!
//! # Pipeline
//!
//! ```text
//! Hir  →  hulk_banner::lower_program  →  BannerProgram
//!      →  Codegen::emit_program        →  LLVM Module
//!      →  compile_to_object            →  .o
//!      →  link_executable              →  executable
//! ```
//!
//! # Public API
//!
//! The main entry point is [`compile`](pipeline::compile):
//!
//! ```ignore
//! use hulk_codegen::pipeline::{compile, CompileOptions};
//! let opts = CompileOptions::default();
//! let exe = compile(&hir, Path::new("/tmp/my_program"), &opts)?;
//! ```

mod codegen;
mod emit;
mod emit_call;
mod emit_mem;
mod emit_ops;
mod error;
mod layout;
mod link;
mod rt;

pub mod pipeline;

pub use error::{CodegenError, CodegenResult};
pub use pipeline::{CompileOptions, compile, emit_ir_string};
