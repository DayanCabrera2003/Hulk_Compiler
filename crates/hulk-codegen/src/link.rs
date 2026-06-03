use std::path::Path;
use std::process::Command;

use inkwell::{
    module::Module,
    targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine},
    OptimizationLevel,
};

use crate::error::{CodegenError, CodegenResult};

/// Emit the LLVM module as a native object file.
///
/// The target machine defaults to the host triple. Object files are later
/// linked by `link_executable` to produce the final HULK binary.
pub fn compile_to_object(module: &Module<'_>, output: &Path) -> CodegenResult<()> {
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|e| CodegenError::Llvm(format!("LLVM target initialization failed: {e}")))?;

    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple)
        .map_err(|e| CodegenError::Llvm(format!("cannot get target for {triple}: {e}")))?;

    let cpu = TargetMachine::get_host_cpu_name();
    let features = TargetMachine::get_host_cpu_features();

    let machine = target
        .create_target_machine(
            &triple,
            cpu.to_str().unwrap_or(""),
            features.to_str().unwrap_or(""),
            OptimizationLevel::Default,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| CodegenError::Llvm("failed to create TargetMachine".to_string()))?;

    machine
        .write_to_file(module, FileType::Object, output)
        .map_err(|e| CodegenError::Llvm(format!("failed to write object file: {e}")))?;

    Ok(())
}

/// Link an object file into an executable, linking against `libhulkruntime`.
///
/// Tries `clang` first, then `cc` as a fallback. Both linkers can locate
/// `libhulkruntime.a` through the search path emitted by the build script
/// via `cargo:rustc-link-search`.
///
/// The `lib_search_dir` parameter is the directory containing `libhulkruntime.a`
/// (the cargo `OUT_DIR` at build time).
pub fn link_executable(
    object: &Path,
    output: &Path,
    lib_search_dir: Option<&Path>,
) -> CodegenResult<()> {
    let linker = find_linker()
        .ok_or_else(|| CodegenError::Link("neither 'clang' nor 'cc' found in PATH".to_string()))?;

    let mut cmd = Command::new(&linker);
    cmd.arg(object).arg("-o").arg(output);

    if let Some(dir) = lib_search_dir {
        cmd.arg(format!("-L{}", dir.display()));
    }

    cmd.arg("-lhulkruntime").arg("-lm");

    let status = cmd
        .status()
        .map_err(|e| CodegenError::Link(format!("failed to invoke linker '{}': {}", linker, e)))?;

    if !status.success() {
        return Err(CodegenError::Link(format!(
            "linker '{}' exited with non-zero status: {status}",
            linker
        )));
    }
    Ok(())
}

/// Emit the LLVM IR as a human-readable `.ll` text file.
///
/// Useful for debugging and for the `--emit=llvm-ir` driver flag.
pub fn write_ir(module: &Module<'_>, output: &Path) -> CodegenResult<()> {
    module
        .print_to_file(output)
        .map_err(|e| CodegenError::Llvm(format!("failed to write LLVM IR: {e}")))?;
    Ok(())
}

/// Return the IR as a string (for tests and diagnostics).
#[must_use]
pub fn ir_string(module: &Module<'_>) -> String {
    module.print_to_string().to_string()
}

fn find_linker() -> Option<String> {
    for candidate in ["clang", "clang-17", "cc", "gcc"] {
        if Command::new(candidate).arg("--version").output().is_ok() {
            return Some(candidate.to_string());
        }
    }
    None
}
