//! Stages the Windows App SDK runtime used by the WinUI-backed launcher.

use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn main() {
    windows_reactor_setup::as_self_contained();
    compile_windows_icon_resource();
}

fn compile_windows_icon_resource() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app-icon.ico");

    if !cfg!(target_os = "windows") {
        return;
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let resource = out_dir.join("app.res");
    let compiler = resource_compiler();
    let status = Command::new(compiler)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg("assets/app.rc")
        .status()
        .unwrap_or_else(|error| panic!("failed to run {compiler}: {error}"));

    if !status.success() {
        panic!("{compiler} failed while compiling assets/app.rc");
    }

    println!("cargo:rustc-link-arg-bins={}", resource.display());
}

fn resource_compiler() -> &'static str {
    if command_exists("rc.exe") {
        "rc.exe"
    } else if command_exists("llvm-rc.exe") {
        "llvm-rc.exe"
    } else {
        panic!("could not find rc.exe or llvm-rc.exe for Windows icon resources");
    }
}

fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("/?")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}
