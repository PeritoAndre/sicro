//! SICRO Desktop — library crate.
//!
//! The Tauri entry point in `main.rs` re-exports `run()` from this crate.
//! Splitting library from binary lets us run unit tests against modules
//! without dragging the Tauri runtime into the test harness.

pub mod audio;
pub mod commands;
pub mod database;
pub mod docanalysis;
pub mod docforensics;
pub mod error;
pub mod exporters;
pub mod filesystem;
pub mod hashing;
pub mod image_editor;
pub mod image_processing;
pub mod importer;
pub mod models;
pub mod ocr;
pub mod registry;
pub mod state;
pub mod video;
pub mod workspace;

use tracing_subscriber::EnvFilter;

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
        // I/J — Estado em memória do cover SIGDOC (mantém os bounds
        // atuais para o listener de resize reposicionar a janela
        // secundária que cobre a área do editor).
        .manage(commands::sigdocs_commands::SigdocsCoverState::default())
        .setup(|app| {
            // J — Instala o listener de resize/move na main window para
            // reposicionar o cover do SIGDOC quando a janela muda.
            commands::sigdocs_commands::install_cover_resize_listener(
                &app.handle().clone(),
            );
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
            commands::laudo_commands::create_laudo,
            commands::laudo_commands::import_docx_as_laudo,
            commands::laudo_commands::list_laudos,
            commands::laudo_commands::read_laudo,
            commands::laudo_commands::save_laudo,
            commands::laudo_commands::delete_laudo,
            // H — Fluxo gov.br externo
            commands::laudo_commands::import_signed_pdf,
            // O — Drag & drop de fotos no editor de laudo
            commands::laudo_photo_drop::import_dragged_photos_to_laudo,
            // T — Paste (Ctrl+V) de fotos no editor de laudo
            commands::laudo_photo_drop::import_pasted_photos_to_laudo,
            // export (Spike C)
            commands::export_commands::export_laudo_html,
            commands::export_commands::export_laudo_pdf,
            commands::export_commands::export_laudo_pdf_libreoffice,
            commands::export_commands::export_laudo_docx,
            commands::export_commands::list_laudo_exports,
            // importer (Spike D — .sicroapp)
            commands::import_commands::import_sicroapp,
            commands::import_commands::list_workspace_imports,
            commands::import_commands::read_import_report,
            // dossiê operacional (MVP 3)
            commands::dossie_commands::get_dossie_summary,
            commands::dossie_commands::list_dossie_photos,
            commands::dossie_commands::list_dossie_checklist,
            commands::dossie_commands::list_dossie_entities,
            commands::dossie_commands::list_dossie_traces,
            commands::dossie_commands::list_dossie_measurements,
            commands::dossie_commands::list_dossie_notes,
            commands::dossie_commands::list_dossie_timeline,
            commands::dossie_commands::get_dossie_stats,
            commands::dossie_commands::rehydrate_dossie,
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
            // áudio (módulo Áudio — Camada 1)
            commands::audio_commands::extract_audio_from_video,
            commands::audio_commands::import_audio_file,
            commands::audio_commands::list_audio_media,
            commands::audio_commands::open_audio_media,
            commands::audio_commands::audio_spectrogram,
            commands::audio_commands::audio_measure,
            commands::audio_commands::audio_spectrum,
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
            commands::documentoscopia_commands::import_document,
            commands::documentoscopia_commands::list_documents,
            commands::documentoscopia_commands::get_document,
            commands::documentoscopia_commands::delete_document,
            commands::documentoscopia_commands::update_document_meta,
            commands::documentoscopia_commands::set_document_pageinfo,
            commands::documentoscopia_commands::save_ocr_run,
            commands::documentoscopia_commands::run_ocr,
            commands::documentoscopia_commands::run_ocr_page_image,
            commands::documentoscopia_commands::list_ocr_runs,
            commands::documentoscopia_commands::get_run_blocks,
            commands::documentoscopia_commands::review_text_block,
            commands::documentoscopia_commands::add_manual_block,
            commands::documentoscopia_commands::delete_text_block,
            commands::documentoscopia_commands::set_block_bbox,
            commands::documentoscopia_commands::export_searchable_pdf,
            commands::documentoscopia_commands::preprocess_image,
            commands::documentoscopia_commands::perspective_image,
            commands::documentoscopia_commands::detect_layout,
            commands::documentoscopia_commands::decode_region,
            commands::documentoscopia_commands::save_confronto_image,
            commands::documentoscopia_commands::doc_ela,
            commands::documentoscopia_commands::doc_noise_map,
            commands::documentoscopia_commands::doc_copy_move,
            commands::documentoscopia_commands::extract_pdf_jpeg,
            commands::documentoscopia_commands::generate_ela_test_sample,
            commands::documentoscopia_commands::save_doc_indicio,
            commands::documentoscopia_commands::list_doc_indicios,
            commands::documentoscopia_commands::generate_doc_report,
            commands::documentoscopia_commands::save_fields,
            commands::documentoscopia_commands::list_fields,
            commands::documentoscopia_commands::review_field,
            commands::documentoscopia_commands::save_region,
            commands::documentoscopia_commands::list_regions,
            commands::documentoscopia_commands::delete_region,
            commands::documentoscopia_commands::save_comparison,
            commands::documentoscopia_commands::list_comparisons,
            commands::documentoscopia_commands::list_document_log,
            // gerenciador de IA (download do whisper.cpp + modelos)
            commands::ai_commands::get_ai_catalog,
            commands::ai_commands::get_ai_status,
            commands::ai_commands::install_ai_asset,
            commands::ai_commands::remove_ai_asset,
            commands::ai_commands::check_ai_updates,
            commands::ai_commands::update_whisper_engine,
            // gerenciador de dependência LibreOffice (PDF com diagramação Word)
            commands::libreoffice_commands::get_libreoffice_status,
            commands::libreoffice_commands::download_libreoffice_installer,
            // gerenciador de OCR (Documentoscopia — RapidOCR/PP-OCRv5 + modelos)
            commands::ocr_commands::get_ocr_catalog,
            commands::ocr_commands::get_ocr_status,
            commands::ocr_commands::install_ocr_asset,
            commands::ocr_commands::remove_ocr_asset,
            commands::ocr_commands::check_ocr_updates,
            commands::ocr_commands::update_ocr_models,
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
            commands::evidence_commands::record_evidence_link,
            commands::evidence_commands::read_evidence_asset,
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
            commands::sigdocs_commands::get_sigdocs_url,
            commands::sigdocs_commands::open_sigdocs_window,
            commands::sigdocs_commands::close_sigdocs_window,
            commands::sigdocs_commands::open_sigdocs_cover,
            commands::sigdocs_commands::update_sigdocs_cover_bounds,
            commands::sigdocs_commands::close_sigdocs_cover,
            commands::sigdocs_commands::reveal_path_in_explorer,
            // K — Credenciais SIGDOC (Windows Credential Manager)
            commands::sigdocs_commands::save_sigdoc_credentials,
            commands::sigdocs_commands::get_sigdoc_credentials_status,
            commands::sigdocs_commands::delete_sigdoc_credentials,
            // Configurações globais do app (perfil, instituição, aparência, caminhos)
            commands::settings_commands::get_app_settings,
            commands::settings_commands::save_app_settings,
            commands::settings_commands::get_settings_file_path,
            commands::settings_commands::load_pop_calibration,
            commands::settings_commands::save_pop_calibration,
            // Cabeçalhos oficiais — pasta dedicada cabecalhos/
            commands::header_templates_commands::list_header_templates,
            commands::header_templates_commands::save_header_template,
            commands::header_templates_commands::delete_header_template,
            // Estatísticas — exportação do dashboard (HTML/CSV/JSON)
            commands::statistics_commands::save_statistics_export,
            commands::statistics_commands::save_general_statistics_export,
            // Índice global de casos (estatísticas gerais de trabalho)
            commands::case_index_commands::get_case_index,
            commands::case_index_commands::upsert_case_index,
            commands::case_index_commands::remove_case_index,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SICRO Desktop");
}
