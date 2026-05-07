use std::path::{Path, PathBuf};

use inkwell::context::Context;

use hulk_hir::Hir;

use crate::{
    codegen::Codegen,
    error::{CodegenError, CodegenResult},
    link,
};

/// Options controlling how a HULK program is compiled.
#[derive(Debug, Clone)]
pub struct CompileOptions {
    /// Directory to use for intermediate files (object file).
    /// Defaults to the system temp directory.
    pub work_dir: Option<PathBuf>,
    /// If `Some`, also write the LLVM IR text to this path.
    pub emit_ir: Option<PathBuf>,
    /// Directory containing `libhulkruntime.a` (injected by the build script).
    pub lib_dir: Option<PathBuf>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            work_dir: None,
            emit_ir: None,
            lib_dir: out_dir(),
        }
    }
}

/// Compile a typed HULK HIR into a native executable.
///
/// The pipeline is:
/// 1. Lower HIR → `BannerProgram` (three-address IR).
/// 2. Generate LLVM IR from the `BannerProgram`.
/// 3. Emit a native object file.
/// 4. Link with `libhulkruntime.a` + `libm`.
///
/// Returns the path to the produced executable.
pub fn compile(hir: &Hir, output: &Path, opts: &CompileOptions) -> CodegenResult<PathBuf> {
    let banner = hulk_banner::lower_program(hir);
    let ctx = Context::create();
    let mut cg = Codegen::new(&ctx, "hulk_module", &banner);

    cg.predeclare_all(&banner)?;
    cg.emit_program(&banner)?;

    // Optionally dump the LLVM IR text for inspection.
    if let Some(ref ir_path) = opts.emit_ir {
        link::write_ir(&cg.module, ir_path)?;
    }

    // Validate the module before emitting code.
    cg.module
        .verify()
        .map_err(|e| CodegenError::Llvm(format!("LLVM module verification failed: {e}")))?;

    let work_dir = opts.work_dir.clone().unwrap_or_else(std::env::temp_dir);
    let obj_path = work_dir.join("hulk_out.o");

    link::compile_to_object(&cg.module, &obj_path)?;
    link::link_executable(&obj_path, output, opts.lib_dir.as_deref())?;

    Ok(output.to_path_buf())
}

/// Return the LLVM IR string for a HIR, without producing an executable.
///
/// Useful for the `--emit=llvm-ir` flag in `hulk-driver` (session 16).
pub fn emit_ir_string(hir: &Hir) -> CodegenResult<String> {
    let banner = hulk_banner::lower_program(hir);
    let ctx = Context::create();
    let mut cg = Codegen::new(&ctx, "hulk_module", &banner);
    cg.predeclare_all(&banner)?;
    cg.emit_program(&banner)?;
    Ok(link::ir_string(&cg.module))
}

/// Directory containing `libhulkruntime.a`.
///
/// Prefers the runtime `OUT_DIR` env var (set by cargo while building
/// hulk-codegen and its tests). Falls back to the build-time `OUT_DIR`
/// captured via `env!`, so binaries (like `hulkc`) that link against
/// hulk-codegen still find the runtime when invoked outside cargo.
fn out_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("OUT_DIR") {
        return Some(PathBuf::from(dir));
    }
    Some(PathBuf::from(env!("OUT_DIR")))
}
