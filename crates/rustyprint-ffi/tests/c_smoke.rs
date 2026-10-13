//! Builds `smoke.c` against the shared library and the generated header with the platform's C
//! compiler, as a C program using the library would be built, and runs it.

use std::path::{Path, PathBuf};
use std::process::Command;

const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// The directory with the shared library Cargo built for these tests, beside the test binary.
fn library_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let deps = exe.parent().unwrap();
    let file = if cfg!(windows) {
        "rustyprint_ffi.dll.lib"
    } else if cfg!(target_os = "macos") {
        "librustyprint_ffi.dylib"
    } else {
        "librustyprint_ffi.so"
    };
    [deps, deps.parent().unwrap()]
        .into_iter()
        .find(|dir| dir.join(file).exists())
        .unwrap_or_else(|| panic!("no {file} beside {}", exe.display()))
        .to_owned()
}

fn compile(library: &Path, out: &Path) -> PathBuf {
    let compiler = cc::Build::new()
        .target(env!("RUSTYPRINT_FFI_TARGET"))
        .host(env!("RUSTYPRINT_FFI_HOST"))
        .opt_level(0)
        .debug(false)
        .cargo_metadata(false)
        .cargo_warnings(false)
        .emit_rerun_if_env_changed(false)
        .get_compiler();
    let source = Path::new(MANIFEST_DIR).join("tests/smoke.c");
    let include = Path::new(MANIFEST_DIR).join("include");
    let exe = out.join(if cfg!(windows) { "smoke.exe" } else { "smoke" });
    let mut command = compiler.to_command();
    if compiler.is_like_msvc() {
        command
            .args(["/W4", "/WX"])
            .arg(format!("/I{}", include.display()))
            .arg(&source)
            .arg(format!("/Fe{}", exe.display()))
            .arg(format!("/Fo{}\\", out.display()))
            .arg("/link")
            .arg(library.join("rustyprint_ffi.dll.lib"));
    } else {
        command
            .args(["-std=c99", "-Wall", "-Wextra", "-pedantic", "-Werror"])
            .arg("-I")
            .arg(&include)
            .arg(&source)
            .arg("-o")
            .arg(&exe)
            .arg("-L")
            .arg(library)
            .arg("-lrustyprint_ffi")
            .arg(format!("-Wl,-rpath,{}", library.display()));
    }
    let output = command.output().expect("the C compiler runs");
    assert!(
        output.status.success(),
        "compiling smoke.c failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    exe
}

#[test]
fn a_c_program_renders_through_the_library() {
    let library = library_dir();
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("c-smoke");
    std::fs::create_dir_all(&out).unwrap();
    let exe = compile(&library, &out);

    let mut command = Command::new(&exe);
    if cfg!(windows) {
        // Windows finds DLLs on the PATH.
        let path = std::env::var_os("PATH").unwrap_or_default();
        let paths = std::iter::once(library.clone()).chain(std::env::split_paths(&path));
        command.env("PATH", std::env::join_paths(paths).unwrap());
    }
    let output = command.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "smoke test failed:\n{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains(concat!("rustyprint ", env!("CARGO_PKG_VERSION"))),
        "{stdout}"
    );
    assert!(stdout.trim_end().ends_with("ok"), "{stdout}");
}
