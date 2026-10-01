//! SICRO Desktop — library crate.
//!
//! The Tauri entry point in `main.rs` re-exports `run()` from this crate.
//! Splitting library from binary lets us run unit tests against modules
//! without dragging the Tauri runtime into the test harness.

pub mod audio;
pub mod commands;
pub mod database;
pub mod error;
pub mod filesystem;
pub mod hashing;
pub mod image_editor;
pub mod image_processing;
pub mod importer;
pub mod models;
pub mod registry;
pub mod state;
pub mod tools;
pub mod video;
pub mod workspace;

use tracing_subscriber::EnvFilter;

/// Linux: o WebView é o WebKitGTK, cujo player de mídia (GStreamer) só lê
/// http(s), blob e file:// — pelo asset protocol do Tauri dá "No URI handler
/// implemented for asset" e o <video>/<audio> falha com MediaError 4. O
/// frontend passa a mídia por file:// (`src/core/mediaSrc.ts`), o que exige que
/// a página do app (esquema `tauri`) seja "local" para o WebKit.
///
/// Tem de rodar ANTES de a página carregar: o WebKit decide no nascimento do
/// documento se ele pode abrir file://, então registrar depois não vale. O
/// `setup` roda na thread principal logo após criar a janela do config, e aí o
/// `with_webview` executa na hora (sem ir para a fila do event loop).
///
/// Não amplia o acesso: o escopo do asset protocol já é `**`.
#[cfg(target_os = "linux")]
fn allow_local_media(app: &tauri::App) {
    use tauri::Manager;
    use webkit2gtk::{SecurityManagerExt, WebContextExt, WebViewExt};

    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let result = main.with_webview(|webview| {
        if let Some(security) = webview.inner().context().and_then(|c| c.security_manager()) {
            security.register_uri_scheme_as_local("tauri");
        }
    });
    if let Err(e) = result {
        tracing::warn!("não foi possível liberar file:// para mídia: {e}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let app_state = state::AppState::init().expect("failed to initialize AppState");

    // NOTE: `generate_handler!` needs the full path to the module that owns the
    // `#[tauri::command]` annotation. The macro generates sibling symbols
    // (`__cmd__*`, `__tauri_command_name_*`) next to the function, and those
    // siblings are not carried over by a `pub use` re-export.
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // H — Plugins para o fluxo gov.br (abrir browser + copiar caminho).
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            #[cfg(target_os = "linux")]
            allow_local_media(app);
            Ok(())
        })
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            // workspace / occurrence
            commands::workspace_commands::create_occurrence,
            commands::workspace_commands::open_occurrence,
            commands::workspace_commands::get_occurrence,
            commands::workspace_commands::update_occurrence,
            commands::workspace_commands::set_occurrence_status,
            commands::workspace_commands::list_recent_occurrences,
            commands::workspace_commands::forget_recent_occurrence,
            commands::workspace_commands::delete_occurrence,
            // laudo (Spike B)
            // SICRO 3.0 — laudo COMO .docx (registra + abre no Word/LibreOffice)
            // H — Fluxo gov.br externo
            // O — Drag & drop de fotos no editor de laudo
            // T — Paste (Ctrl+V) de fotos no editor de laudo
            // export (Spike C)
            // importer (Spike D — .sicroapp)
            commands::import_commands::import_sicroapp,
            commands::import_commands::list_workspace_imports,
            commands::import_commands::read_import_report,
            // dossiê operacional (MVP 3)
            commands::dossie_commands::list_dossie_photos,
            // croqui (Spike E)
            commands::croqui_commands::create_croqui,
            commands::croqui_commands::list_croquis,
            commands::croqui_commands::read_croqui,
            commands::croqui_commands::save_croqui,
            commands::croqui_commands::delete_croqui,
            commands::croqui_commands::export_croqui_png,
            commands::croqui_commands::import_drone_image,
            // video (Spike F)
            commands::video_commands::register_video_media,
            commands::video_commands::list_video_media,
            commands::video_commands::open_video_media,
            commands::video_commands::create_video_event,
            commands::video_commands::update_video_event,
            commands::video_commands::delete_video_event,
            commands::video_commands::collect_video_frame,
            commands::video_commands::update_storyboard_frame,
            commands::video_commands::delete_storyboard_frame,
            commands::video_commands::list_video_operation_logs,
            commands::video_commands::list_video_clocks,
            commands::video_commands::set_video_clock,
            commands::video_commands::delete_video_clock,
            commands::video_commands::video_thumbnail,
            commands::video_commands::export_video_clip,
            // áudio (módulo Áudio — Camada 1)
            commands::audio_commands::extract_audio_from_video,
            commands::audio_commands::import_audio_file,
            commands::audio_commands::list_audio_media,
            commands::audio_commands::open_audio_media,
            commands::audio_commands::audio_spectrogram,
            commands::audio_commands::audio_measure,
            commands::audio_commands::audio_spectrum,
            commands::audio_commands::audio_spectrogram_data,
            commands::audio_commands::audio_enf,
            commands::audio_commands::extract_audio_clip,
            commands::audio_commands::compile_audio_clips,
            commands::audio_commands::add_audio_marker,
            commands::audio_commands::list_audio_markers,
            commands::audio_commands::delete_audio_marker,
            commands::audio_commands::enhance_audio,
            commands::audio_commands::list_audio_transcript,
            commands::audio_commands::save_audio_transcript,
            commands::audio_commands::whisper_status,
            commands::audio_commands::transcribe_audio,
            // documentoscopia (OCR, layout, campos, regiões, comparação)
            // gerenciador de IA (download do whisper.cpp + modelos)
            commands::ai_commands::get_ai_catalog,
            commands::ai_commands::get_ai_status,
            commands::ai_commands::install_ai_asset,
            commands::ai_commands::remove_ai_asset,
            commands::ai_commands::check_ai_updates,
            commands::ai_commands::update_whisper_engine,
            // gerenciador de dependência LibreOffice (PDF com diagramação Word)
            // gerenciador de OCR (Documentoscopia — RapidOCR/PP-OCRv5 + modelos)
            // calculador de velocidade (vídeo / speed)
            commands::video_speed_commands::create_speed_calibration,
            commands::video_speed_commands::compute_speed,
            commands::video_speed_commands::list_speed_calibrations,
            commands::video_speed_commands::list_speed_calculations,
            commands::video_speed_commands::list_speed_calculations_for_occurrence,
            commands::video_speed_commands::get_speed_calibration,
            // medição de distância (vídeo / measure)
            commands::video_distance_commands::create_distance_measurement,
            commands::video_distance_commands::list_distance_measurements,
            commands::video_distance_commands::get_distance_measurement,
            commands::video_distance_commands::list_distance_measurements_for_occurrence,
            // evidência → laudo (MVP 4)
            // central de evidências + integridade (MVP 5)
            commands::registry_commands::list_evidence_registry_items,
            commands::registry_commands::get_evidence_registry_summary,
            commands::registry_commands::verify_workspace_integrity,
            commands::registry_commands::list_evidence_links,
            commands::registry_commands::open_evidence_file,
            commands::registry_commands::reveal_evidence_in_folder,
            commands::registry_commands::generate_workspace_integrity_report,
            // editor de imagem pericial (MVP 7)
            commands::image_commands::create_image_analysis_from_evidence,
            commands::image_commands::create_image_analysis_from_file,
            commands::image_commands::list_image_analyses,
            commands::image_commands::read_image_analysis,
            commands::image_commands::save_image_analysis,
            commands::image_commands::export_image_derivative,
            commands::image_commands::read_image_asset,
            commands::image_commands::get_image_metadata,
            commands::image_commands::list_image_operation_logs,
            // G12 — Image Engine Pro
            commands::image_commands::compute_image_histogram,
            commands::image_commands::apply_operation_stack,
            commands::image_commands::apply_operation_stack_preview,
            commands::image_commands::copy_region_to_layer,
            commands::image_commands::generate_image_analysis_report,
            // consolidação alpha (MVP 8)
            commands::alpha_commands::generate_workspace_backup,
            commands::alpha_commands::generate_global_backup,
            commands::alpha_commands::restore_backup,
            commands::alpha_commands::get_system_health_snapshot,
            commands::alpha_commands::generate_system_health_report,
            commands::alpha_commands::get_occurrence_counts,
            // I/J — Integração SIGDOC (janela secundária + cover webview)
            commands::os_open::reveal_path_in_explorer,
            // K — Credenciais SIGDOC (Windows Credential Manager)
            // Configurações globais do app (perfil, instituição, aparência, caminhos)
            commands::settings_commands::get_app_settings,
            commands::settings_commands::save_app_settings,
            commands::settings_commands::get_settings_file_path,
            commands::settings_commands::load_pop_calibration,
            commands::settings_commands::save_pop_calibration,
            // Cabeçalhos oficiais — pasta dedicada cabecalhos/
            // Estatísticas — exportação do dashboard (HTML/CSV/JSON)
            // Índice global de casos (estatísticas gerais de trabalho)
            commands::case_index_commands::get_case_index,
            commands::case_index_commands::upsert_case_index,
            commands::case_index_commands::remove_case_index,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SICRO Desktop");
}
