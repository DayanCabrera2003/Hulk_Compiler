use std::path::PathBuf;

/// Controls what intermediate representation the driver produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmitKind {
    /// Token stream in debug format.
    Tokens,
    /// Parsed AST in debug format.
    Ast,
    /// Fully typed HIR in debug format.
    Hir,
    /// BANNER three-address IR.
    Banner,
    /// LLVM IR text (`.ll`).
    LlvmIr,
    /// Native object file (`.o`).
    Object,
    /// Linked native executable.
    #[default]
    Executable,
}

/// Options controlling how the driver compiles a HULK program.
#[derive(Debug, Clone, Default)]
pub struct CompileOptions {
    /// What to produce. Defaults to `EmitKind::Executable`.
    pub emit: EmitKind,
    /// Destination path. If `None`, a path is derived from the source file stem.
    pub output: Option<PathBuf>,
    /// LLVM optimization level (0–3). Forwarded to the codegen backend.
    pub optimization_level: u8,
}
