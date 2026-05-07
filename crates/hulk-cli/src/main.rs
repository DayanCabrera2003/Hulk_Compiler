use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand, ValueEnum};
use hulk_driver::{check, compile, CompileOptions, EmitKind};

/// HULK language compiler.
#[derive(Parser)]
#[command(name = "hulkc", version, about = "HULK language compiler")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a HULK source file.
    Compile {
        /// Source file to compile.
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// What to emit.
        #[arg(long, value_enum, default_value = "executable")]
        emit: EmitArg,
        /// Output path (defaults to source stem with appropriate extension).
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
    },
    /// Compile and immediately run a HULK program.
    Run {
        /// Source file to compile and run.
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
    /// Check a HULK source file for errors without producing output.
    Check {
        /// Source file to check.
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
}

/// What intermediate representation to emit.
#[derive(Clone, ValueEnum)]
enum EmitArg {
    /// Token stream.
    Tokens,
    /// Parsed AST.
    Ast,
    /// Typed HIR.
    Hir,
    /// BANNER three-address IR.
    Banner,
    /// LLVM IR text.
    #[value(name = "llvm-ir")]
    LlvmIr,
    /// Native object file.
    Object,
    /// Linked native executable (default).
    Executable,
}

impl From<EmitArg> for EmitKind {
    fn from(arg: EmitArg) -> Self {
        match arg {
            EmitArg::Tokens => EmitKind::Tokens,
            EmitArg::Ast => EmitKind::Ast,
            EmitArg::Hir => EmitKind::Hir,
            EmitArg::Banner => EmitKind::Banner,
            EmitArg::LlvmIr => EmitKind::LlvmIr,
            EmitArg::Object => EmitKind::Object,
            EmitArg::Executable => EmitKind::Executable,
        }
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compile { file, emit, output } => {
            let opts = CompileOptions {
                emit: emit.into(),
                output,
                ..Default::default()
            };
            match compile(&file, &opts) {
                Ok(out) => eprintln!("Written: {}", out.display()),
                Err(diagnostics) => {
                    print_diagnostics(&diagnostics);
                    process::exit(1);
                }
            }
        }

        Commands::Run { file } => {
            let tmp = std::env::temp_dir().join("hulkc_run");
            if let Err(e) = std::fs::create_dir_all(&tmp) {
                eprintln!("error: cannot create '{}': {e}", tmp.display());
                process::exit(1);
            }
            let exe_path = tmp.join(
                file.file_stem()
                    .unwrap_or_else(|| std::ffi::OsStr::new("program")),
            );
            let opts = CompileOptions {
                emit: EmitKind::Executable,
                output: Some(exe_path.clone()),
                ..Default::default()
            };
            match compile(&file, &opts) {
                Err(diagnostics) => {
                    print_diagnostics(&diagnostics);
                    process::exit(1);
                }
                Ok(exe) => {
                    let status = process::Command::new(&exe).status().unwrap_or_else(|e| {
                        eprintln!("error: cannot run '{}': {e}", exe.display());
                        process::exit(1);
                    });
                    process::exit(status.code().unwrap_or(1));
                }
            }
        }

        Commands::Check { file } => match check(&file) {
            Ok(()) => eprintln!("OK"),
            Err(diagnostics) => {
                print_diagnostics(&diagnostics);
                process::exit(1);
            }
        },
    }
}

fn print_diagnostics(diagnostics: &[hulk_driver::Diagnostic]) {
    for d in diagnostics {
        eprintln!("error: {}", d.message);
    }
}
