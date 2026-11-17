fn main() {
    let mut atributos = tauri_build::Attributes::new();
    // Windows: o manifesto (Common-Controls 6, o mesmo do tauri-build) vai pelo linker para valer também nos
    // executáveis de teste; sem ele o `cargo test` não abre (STATUS_ENTRYPOINT_NOT_FOUND).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        atributos = atributos.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        let manifesto = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifesto.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifesto.display());
    }
    tauri_build::try_build(atributos).expect("tauri-build");
}
