use std::path::PathBuf;
use std::process::Command;

fn main() {
    build_ven();
    // Release builds demand elevation; dev builds stay asInvoker so `tauri dev` can launch from a normal shell.
    let manifest = if std::env::var("PROFILE").as_deref() == Ok("release") {
        include_str!("manifest.admin.xml")
    } else {
        include_str!("manifest.dev.xml")
    };
    let attrs = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new().app_manifest(manifest));
    tauri_build::try_build(attrs).expect("tauri build failed");
}

/// Builds ../ven (the Discord/Vencord startup app) for the same target, always in release, into its
/// own target dir; src/ven.rs embeds the result via VEN_EXE.
fn build_ven() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../ven");
    println!("cargo:rerun-if-changed={}", manifest.join("src").display());
    println!("cargo:rerun-if-changed={}", manifest.join("Cargo.toml").display());
    println!("cargo:rerun-if-changed={}", manifest.join("Cargo.lock").display());
    let target = std::env::var("TARGET").unwrap();
    let target_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("ven-target");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .args(["build", "--release", "--locked", "--target", &target])
        .arg("--manifest-path")
        .arg(manifest.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target_dir)
        // Flags meant for setup-hub (crt-static, coverage…) aren't ven's.
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_TARGET_DIR")
        .status()
        .expect("cargo failed to start for ven");
    assert!(status.success(), "building ven failed");
    let exe = target_dir.join(&target).join("release").join("ven.exe");
    assert!(exe.is_file(), "ven.exe not found at {}", exe.display());
    println!("cargo:rustc-env=VEN_EXE={}", exe.display());
}
