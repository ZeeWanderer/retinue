use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const BLEND2D_SOURCE: &str = "vendor/blend2d";
const ASMJIT_SOURCE: &str = "vendor/asmjit";

fn main() {
    println!("cargo:rerun-if-changed=native");
    println!("cargo:rerun-if-changed={BLEND2D_SOURCE}");
    println!("cargo:rerun-if-changed={ASMJIT_SOURCE}");
    println!("cargo:rerun-if-env-changed=WFCLI_CPU_BASELINE");
    for variable in [
        "PATH",
        "CC",
        "CXX",
        "CFLAGS",
        "CXXFLAGS",
        "LDFLAGS",
        "CMAKE_C_COMPILER_LAUNCHER",
        "CMAKE_CXX_COMPILER_LAUNCHER",
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    for tool in [
        env::var("CC").unwrap_or_else(|_| "cc".into()),
        env::var("CXX").unwrap_or_else(|_| "c++".into()),
        "cmake".into(),
        "make".into(),
    ] {
        watch_tool(&tool);
    }

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    let blend2d_dir = manifest_dir.join(BLEND2D_SOURCE);
    let asmjit_dir = manifest_dir.join(ASMJIT_SOURCE);
    require_submodule(&blend2d_dir, "Blend2D");
    require_submodule(&asmjit_dir, "AsmJit");

    let build_dir = out_dir.join("native");
    let mut configure = Command::new("cmake");
    configure
        .arg("--fresh")
        .arg("-G")
        .arg("Unix Makefiles")
        .arg("-S")
        .arg(manifest_dir.join("native"))
        .arg("-B")
        .arg(&build_dir)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg("-DCMAKE_EXPORT_COMPILE_COMMANDS=ON")
        .arg(format!("-DBLEND2D_DIR={}", blend2d_dir.display()))
        .arg(format!("-DASMJIT_DIR={}", asmjit_dir.display()));
    let cpu = env::var_os("WFCLI_CPU_BASELINE").unwrap_or_default();
    configure.arg(format!("-DRETINUE_CPU_BASELINE={}", cpu.to_string_lossy()));
    run(&mut configure, "configure native renderer");
    let mut build = Command::new("cmake");
    build
        .arg("--build")
        .arg(&build_dir)
        .arg("--config")
        .arg("Release")
        .arg("--target")
        .arg("retinue_blend2d_bridge");
    if let Some(flags) = env::var_os("CARGO_MAKEFLAGS") {
        build.env("MAKEFLAGS", flags);
    }
    run(&mut build, "build native renderer");

    println!(
        "cargo:rustc-link-search=native={}",
        build_dir.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=retinue_blend2d_bridge");
    println!("cargo:rustc-link-lib=static=blend2d");
    println!("cargo:rustc-link-lib=dylib=stdc++");
    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-lib=dylib=dl");
    println!("cargo:rustc-link-lib=dylib=m");
}

fn watch_tool(tool: &str) {
    let path = env::split_paths(&env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join(tool))
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(tool));
    println!("cargo:rerun-if-changed={}", path.display());
    if let Ok(resolved) = path.canonicalize() {
        println!("cargo:rerun-if-changed={}", resolved.display());
    }
}

fn require_submodule(path: &Path, name: &str) {
    assert!(
        path.join("CMakeLists.txt").is_file(),
        "{name} submodule is missing; run `git submodule update --init`"
    );
}

fn run(command: &mut Command, action: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("could not {action}: {error}"));
    assert!(status.success(), "could not {action}: {status}");
}
