fn main() {
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
