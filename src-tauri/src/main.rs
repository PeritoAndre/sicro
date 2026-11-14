// Sem janela de console extra no Windows em release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // O onnxruntime oficial envia telemetria à Microsoft; só a variável, posta antes de a lib
    // carregar, impede. Vale também para os processos filhos (separação de locutores).
    std::env::set_var("ORT_DISABLE_TELEMETRY", "1");
    // WebKitGTK com DMABUF deixa a janela cinza em várias GPUs ("Failed to
    // create GBM buffer"); o AppImage e o .deb não têm o lançador que desligava.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    sicro_desktop_lib::run();
}
