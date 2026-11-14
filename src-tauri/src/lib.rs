//! Crate biblioteca do SICRO Desktop; `main.rs` só chama `run()`. Separar
//! permite testar os módulos sem o runtime do Tauri.

pub mod audio;
pub mod commands;
pub mod database;
pub mod edicao;
pub mod error;
pub mod filesystem;
pub mod hashing;
pub mod image_editor;
pub mod image_processing;
pub mod importer;
pub mod models;
pub mod registry;
pub mod relatorio_pdf;
pub mod state;
pub mod tools;
pub mod video;
pub mod workspace;

use tracing_subscriber::EnvFilter;

/// Linux: o player do WebKitGTK (GStreamer) não lê o asset protocol do Tauri, então
/// o front passa mídia por file://, o que exige o esquema `tauri` registrado como
/// local. Tem de rodar ANTES de a página carregar (o WebKit decide no nascimento).
/// Efeito colateral: página local não carrega nada de outra origem — por isso todo
/// acesso à internet (Overpass, tiles do mapa) sai pelo Rust. Não ligar
/// `allow_universal_access_from_file_urls`: quebra o IPC do Tauri ("missing Origin header").
/// file:// de página local não tem escopo por pasta: vale também para o cache da Edição
/// (`app_cache_dir()/edicao`). O asset protocol tem (ver `tauri.conf.json` e o teste abaixo).
#[cfg(target_os = "linux")]
fn allow_local_media(app: &tauri::App) {
    use tauri::Manager;
    use webkit2gtk::{SecurityManagerExt, SettingsExt, WebContextExt, WebViewExt};

    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let result = main.with_webview(|webview| {
        let inner = webview.inner();
        if let Some(security) = inner.context().and_then(|c| c.security_manager()) {
            security.register_uri_scheme_as_local("tauri");
        }
        // console.* da página no stdout: é como se depura o release (sem devtools).
        if let Some(settings) = WebViewExt::settings(&inner) {
            settings.set_enable_write_console_messages_to_stdout(true);
            // Narração da Montagem: libera só o microfone (nunca câmera nem tela).
            settings.set_enable_media_stream(true);
        }
        inner.connect_permission_request(|_, req| {
            use webkit2gtk::glib::Cast;
            use webkit2gtk::{PermissionRequestExt, UserMediaPermissionRequest, UserMediaPermissionRequestExt};
            if let Some(m) = req.downcast_ref::<UserMediaPermissionRequest>() {
                if m.is_for_audio_device() && !m.is_for_video_device() {
                    req.allow();
                    return true;
                }
            }
            false
        });
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

    // `generate_handler!` precisa do caminho completo do módulo dono do
    // `#[tauri::command]`: os símbolos gerados (`__cmd__*`) não passam por `pub use`.
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Fluxo gov.br: abrir browser + copiar caminho.
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(target_os = "linux")]
            allow_local_media(app);
            // Edição: cache da sonda, eventos das filas e limpeza do cache
            commands::edicao_commands::iniciar(app.handle());
            Ok(())
        })
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            // workspace / occurrence
            commands::workspace_commands::default_workspace_parent,
            commands::workspace_commands::create_occurrence,
            commands::workspace_commands::open_occurrence,
            commands::workspace_commands::get_occurrence,
            commands::workspace_commands::update_occurrence,
            commands::workspace_commands::set_occurrence_status,
            commands::workspace_commands::list_recent_occurrences,
            commands::workspace_commands::forget_recent_occurrence,
            commands::workspace_commands::delete_occurrence,
            // importação (.sicroapp)
            commands::import_commands::import_sicroapp,
            commands::import_commands::list_workspace_imports,
            commands::import_commands::read_import_report,
            // dossiê
            // croqui
            commands::croqui_commands::create_croqui,
            commands::croqui_commands::list_croquis,
            commands::croqui_commands::read_croqui,
            commands::croqui_commands::save_croqui,
            commands::croqui_commands::delete_croqui,
            commands::croqui_commands::export_croqui_png,
            commands::croqui_commands::import_drone_image,
            commands::croqui_commands::fetch_overpass,
            commands::croqui_commands::fetch_osm_tile,
            commands::croqui_commands::fetch_map_tile,
            commands::croqui_commands::geocode_place,
            commands::croqui_commands::save_croqui_background,
            commands::croqui_commands::save_croqui_thumbnail,
            // vídeo
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
            commands::video_commands::delete_video_media,
            commands::video_commands::list_montagens,
            commands::video_commands::save_montagem,
            commands::video_commands::delete_montagem,
            commands::video_commands::render_montagem,
            commands::video_commands::importar_audio_montagem,
            commands::video_commands::gravar_audio_montagem,
            // edição: diagnóstico da prévia (Configurações > Vídeo)
            commands::edicao_diagnostico_commands::edicao_diagnostico_midias,
            commands::edicao_diagnostico_commands::edicao_diagnostico_salvar,
            // edição (a lista do teste `comandos_registraveis`)
            commands::edicao_commands::edicao_listar,
            commands::edicao_commands::edicao_abrir,
            commands::edicao_commands::edicao_salvar,
            commands::edicao_commands::edicao_excluir,
            commands::edicao_commands::edicao_duplicar,
            commands::edicao_commands::edicao_analisar,
            commands::edicao_commands::edicao_midia_info,
            commands::edicao_commands::edicao_midia_previa,
            commands::edicao_commands::edicao_tabela_quadros,
            commands::edicao_commands::edicao_quadro_em,
            commands::edicao_commands::edicao_midia_importar,
            commands::edicao_commands::edicao_usos,
            commands::edicao_commands::edicao_gravacao_salvar,
            commands::edicao_commands::edicao_sprite_texto,
            commands::edicao_commands::edicao_atlas_carimbo,
            commands::edicao_commands::edicao_capacidades,
            commands::edicao_commands::edicao_enfileirar,
            commands::edicao_commands::edicao_quadro_final,
            commands::edicao_commands::edicao_conferir,
            commands::edicao_commands::edicao_cancelar,
            commands::edicao_commands::edicao_fila,
            commands::edicao_commands::edicao_fila_limpar,
            commands::edicao_commands::edicao_fundo,
            commands::edicao_commands::edicao_saidas,
            commands::edicao_commands::edicao_saida_excluir,
            commands::edicao_commands::edicao_refazer,
            commands::edicao_commands::edicao_origem_pedir,
            commands::edicao_commands::edicao_onda,
            commands::edicao_commands::edicao_degravacao,
            commands::edicao_commands::edicao_retrato_calibracao,
            commands::edicao_commands::edicao_retrato_calculo,
            commands::video_proveniencia_commands::video_proveniencia,
            commands::video_proveniencia_commands::video_proveniencia_salvar,
            // áudio
            commands::audio_commands::extract_audio_from_video,
            commands::audio_commands::import_audio_file,
            commands::audio_commands::list_audio_media,
            commands::audio_commands::open_audio_media,
            commands::audio_commands::audio_spectrogram,
            commands::audio_commands::audio_measure,
            commands::audio_commands::audio_spectrum,
            commands::audio_commands::audio_spectrogram_data,
            commands::ai_commands::install_diarization,
            commands::ai_commands::remove_diarization,
            commands::ai_commands::install_pessoas,
            commands::ai_commands::remove_pessoas,
            commands::pessoas_commands::pessoas_classes,
            commands::pessoas_commands::pessoas_seguir_mancha,
            commands::ai_commands::install_superres,
            commands::ai_commands::remove_superres,
            commands::realce_commands::realce_cancelar,
            commands::realce_commands::realce_listar,
            commands::realce_commands::realce_para_storyboard,
            commands::realce_commands::realce_quadro,
            commands::realce_commands::realce_media,
            commands::realce_commands::realce_superres,
            commands::realce_commands::realce_superres_imagem,
            commands::realce_commands::realce_lente_obter,
            commands::realce_commands::realce_lente_salvar,
            commands::realce_commands::realce_lente_apagar,
            commands::realce_commands::realce_lente_listar,
            commands::realce_commands::realce_lente_previa,
            commands::realce_commands::realce_quadro_corrigido,
            commands::realce_commands::realce_perspectiva,
            commands::realce_commands::realce_estabilizacao_disponivel,
            commands::realce_commands::realce_estabilizar,
            commands::realce_commands::realce_lente_video,
            commands::pessoas_commands::pessoas_listar,
            commands::pessoas_commands::pessoas_rastrear,
            commands::pessoas_commands::pessoas_cancelar,
            commands::pessoas_commands::pessoas_editar,
            commands::pessoas_commands::pessoas_exportar_video,
            commands::pessoas_commands::pessoas_exportar_quadros,
            commands::pessoas_commands::pessoas_exportar_tabela,
            commands::audio_commands::diarize_audio,
            commands::audio_commands::get_audio_diarization,
            commands::audio_commands::save_diarization_names,
            commands::audio_commands::audio_authenticity,
            commands::audio_commands::audio_enf_compare,
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
            // gerenciador de IA
            commands::ai_commands::get_ai_catalog,
            commands::ai_commands::get_ai_status,
            commands::ai_commands::install_ai_asset,
            commands::ai_commands::remove_ai_asset,
            commands::ai_commands::check_ai_updates,
            commands::ai_commands::update_whisper_engine,
            // calculador de velocidade
            commands::video_speed_commands::create_speed_calibration,
            commands::video_speed_commands::compute_speed,
            commands::video_speed_commands::list_speed_calibrations,
            commands::video_speed_commands::list_speed_calculations,
            commands::video_speed_commands::list_speed_calculations_for_occurrence,
            commands::video_speed_commands::get_speed_calibration,
            // medição de distância
            commands::video_distance_commands::create_distance_measurement,
            commands::video_distance_commands::list_distance_measurements,
            commands::video_distance_commands::get_distance_measurement,
            commands::video_distance_commands::list_distance_measurements_for_occurrence,
            // central de evidências + integridade
            commands::registry_commands::list_evidence_registry_items,
            commands::registry_commands::get_evidence_registry_summary,
            commands::registry_commands::verify_workspace_integrity,
            commands::registry_commands::list_evidence_links,
            commands::registry_commands::open_evidence_file,
            commands::registry_commands::reveal_evidence_in_folder,
            commands::registry_commands::generate_workspace_integrity_report,
            // editor de imagem
            commands::image_commands::create_image_analysis_from_evidence,
            commands::image_commands::create_image_analysis_from_file,
            commands::image_commands::list_image_analyses,
            commands::image_commands::read_image_analysis,
            commands::image_commands::save_image_analysis,
            commands::image_commands::export_image_derivative,
            commands::image_commands::read_image_asset,
            commands::image_commands::get_image_metadata,
            commands::image_commands::read_all_image_metadata,
            commands::image_commands::filter_thumbnails,
            commands::image_commands::list_image_operation_logs,
            commands::image_commands::compute_image_histogram,
            commands::image_commands::apply_operation_stack,
            commands::image_commands::apply_operation_stack_preview,
            commands::image_commands::copy_region_to_layer,
            commands::image_commands::generate_image_analysis_report,
            // backup / diagnóstico
            commands::alpha_commands::generate_workspace_backup,
            commands::alpha_commands::generate_global_backup,
            commands::alpha_commands::restore_backup,
            commands::alpha_commands::get_system_health_snapshot,
            commands::alpha_commands::generate_system_health_report,
            commands::alpha_commands::get_occurrence_counts,
            // sistema operacional
            commands::os_open::reveal_path_in_explorer,
            // configurações globais
            commands::settings_commands::get_app_settings,
            commands::settings_commands::save_app_settings,
            commands::settings_commands::get_settings_file_path,
            commands::settings_commands::load_pop_calibration,
            commands::settings_commands::save_pop_calibration,
            // índice global de casos
            commands::case_index_commands::get_case_index,
            commands::case_index_commands::upsert_case_index,
            commands::case_index_commands::remove_case_index,
        ])
        .build(tauri::generate_context!())
        .expect("error while building SICRO Desktop")
        .run(|_app, evento| {
            // filas canceladas e filhos mortos antes de sair (ffmpeg órfão seguiria gravando no cache)
            if let tauri::RunEvent::ExitRequested { .. } = evento {
                commands::edicao_commands::encerrar();
            }
        });
}

#[cfg(test)]
mod tests {
    /// O `invoke_handler` registra cada comando da Edição que o teste `comandos_registraveis` monta (a mesma lista).
    #[test]
    fn comandos_da_edicao_registrados() {
        let fonte = include_str!("lib.rs");
        let teste = include_str!("commands/edicao_commands.rs");
        let ini = teste.find("fn comandos_registraveis").unwrap();
        let lista: Vec<&str> = teste[ini..]
            .lines()
            .map(str::trim)
            .filter_map(|l| l.strip_suffix(',').filter(|n| n.starts_with("edicao_")))
            .collect();
        assert_eq!(lista.len(), 31, "{lista:?}");
        // só o corpo de `run()` (este teste também traz os nomes)
        let corpo = &fonte[fonte.find("pub fn run()").unwrap()..fonte.find("#[cfg(test)]").unwrap()];
        for n in &lista {
            assert!(corpo.contains(&format!("commands::edicao_commands::{n},")), "{n} fora do invoke_handler");
        }
        assert!(corpo.contains("edicao_commands::iniciar(app.handle())") && corpo.contains("edicao_commands::encerrar()"));
    }

    /// No Unix o `**` do escopo não entra em pasta oculta (`~/.cache`): o cache da Edição
    /// precisa da própria entrada. Mesmas opções de `tauri::scope::fs::Scope`.
    #[test]
    fn asset_protocol_le_cache_da_edicao() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let cache = "/home/u/.cache/br.org.sicro.desktop";
        let padroes: Vec<glob::Pattern> = conf["app"]["security"]["assetProtocol"]["scope"]["allow"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| glob::Pattern::new(&p.as_str().unwrap().replace("$APPCACHE", cache)).unwrap())
            .collect();
        let opcoes = glob::MatchOptions {
            require_literal_separator: true,
            require_literal_leading_dot: true,
            ..Default::default()
        };
        let aceita = |caminho: &str| padroes.iter().any(|p| p.matches_with(caminho, opcoes));
        let alvo = format!(
            "{cache}/edicao/6f1c2a52-0d5e-4c1b-9a43-2f7d0c1e9b10/previa/{}.mp4",
            "a".repeat(64)
        );
        assert!(aceita(&alvo));
        assert!(aceita("/casos/A.sicro/Mídias/Vídeos/a.mp4"));
        assert!(!glob::Pattern::new("**").unwrap().matches_with(&alvo, opcoes), "só o ** não basta");
        assert!(!aceita(&format!("{cache}/realce/x.png")), "só a pasta da Edição");
    }
}
