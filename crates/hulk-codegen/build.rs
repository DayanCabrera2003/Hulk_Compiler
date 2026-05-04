use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| ".".to_owned()));
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned()));
    // hulk-codegen lives at crates/hulk-codegen/; runtime is at ../../runtime/.
    let runtime_dir = manifest_dir.join("../../runtime");

    let sources: &[&str] = &["gc.c", "strings.c", "builtins.c"];
    let mut obj_files: Vec<PathBuf> = Vec::new();

    for &src_name in sources {
        let src = runtime_dir.join(src_name);
        let stem = src_name.strip_suffix(".c").unwrap_or(src_name);
        let obj = out_dir.join(format!("{stem}.o"));

        println!("cargo:rerun-if-changed={}", src.display());

        let result = Command::new("gcc")
            .args(["-O2", "-Wall", "-Werror"])
            .arg("-I")
            .arg(&runtime_dir)
            .arg("-c")
            .arg(&src)
            .arg("-o")
            .arg(&obj)
            .status();

        match result {
            Ok(s) if s.success() => {}
            Ok(s) => {
                eprintln!("cargo:warning=gcc exited with {s} while compiling {src_name}");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("cargo:warning=failed to run gcc for {src_name}: {e}");
                std::process::exit(1);
            }
        }

        obj_files.push(obj);
    }

    let lib = out_dir.join("libhulkruntime.a");
    let result = Command::new("ar")
        .arg("rcs")
        .arg(&lib)
        .args(&obj_files)
        .status();

    match result {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("cargo:warning=ar exited with {s}");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("cargo:warning=failed to run ar: {e}");
            std::process::exit(1);
        }
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=hulkruntime");
    // libm provides sqrt, sin, cos, exp, log on Linux.
    println!("cargo:rustc-link-lib=m");
}
