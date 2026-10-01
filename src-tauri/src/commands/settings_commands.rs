//! App-level (GLOBAL) settings — o "cofrinho" que vive FORA de qualquer
//! workspace `.sicro`, no diretório de config do app (per-user, gerenciado
//! pelo Tauri). Guarda o que é do PERITO/da MÁQUINA e deve valer em todas as
//! ocorrências: perfil, marca institucional padrão, aparência e caminhos.
//!
//! Armazenamento: um único `app-settings.json` em `app_config_dir`.
//! Compatibilidade: TODO campo é `#[serde(default)]`, então arquivos antigos
//! carregam sem erro e campos desconhecidos são ignorados. Um arquivo corrompido
//! degrada para os defaults em vez de quebrar a tela de Configurações.
//!
//! NÃO guardamos segredos aqui. A senha do SIGDOC continua no Windows
//! Credential Manager (ver `sigdocs_commands`); estas configs são texto claro.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::{Result, SicroError};

const SETTINGS_FILENAME: &str = "app-settings.json";
const SCHEMA_VERSION: &str = "1";

/// Calibração das posições da numeração POP do croqui corporal — arquivo
/// PRÓPRIO (isolado das app-settings pra não correr risco de ser apagado ao
/// salvar outras configurações). Mapa opaco { "tpl_vista_n_idx": [nx, ny] };
/// o front é dono da estrutura. Global, vale para todas as ocorrências.
const POP_CALIBRATION_FILENAME: &str = "pop_calibration.json";

/// Perfil do perito — pré-preenche autoria de laudos/medições.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeritoProfile {
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub registration: String, // matrícula
    #[serde(default)]
    pub role: String, // cargo
    #[serde(default)]
    pub formation: String, // formação
    #[serde(default)]
    pub signature_image_path: String,
    #[serde(default)]
    pub photo_path: String, // foto do perito (avatar)
    #[serde(default)]
    pub municipio_atuacao: String, // município de atuação/lotação
}

/// Marca institucional padrão — alimenta o cabeçalho dos laudos.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstitutionSettings {
    #[serde(default)]
    pub organization: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub footer_text: String,
    #[serde(default)]
    pub brasao_left_path: String,
    #[serde(default)]
    pub brasao_right_path: String,
}

/// Aparência da interface. `theme`: "dark" | "light" | "auto".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceSettings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_accent")]
    pub accent: String,
    /// Zoom da interface inteira (1.0 = 100%), aplicado no webview pelo frontend.
    #[serde(default = "default_ui_zoom")]
    pub ui_zoom: f64,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            accent: default_accent(),
            ui_zoom: default_ui_zoom(),
        }
    }
}

fn default_theme() -> String {
    "dark".to_string()
}
fn default_accent() -> String {
    "#d7a84f".to_string()
}
fn default_ui_zoom() -> f64 {
    1.0
}

/// Caminhos padrão. Guardados globalmente; o uso efetivo em cada fluxo
/// (criar ocorrência / exportar) entra incrementalmente.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathsSettings {
    #[serde(default)]
    pub default_workspace_dir: String,
    #[serde(default)]
    pub default_export_dir: String,
}

/// Fase 2.1 — IA de transcrição (whisper.cpp + modelo) instalada pelo
/// gerenciador. Caminhos globais (valem em todos os casos). Texto claro.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiSettings {
    #[serde(default)]
    pub whisper_bin_path: String,
    #[serde(default)]
    pub model_path: String,
    #[serde(default)]
    pub vad_model_path: String,
    #[serde(default)]
    pub whisper_version: String,
    /// Separação de locutores (sherpa-onnx, local): executável e modelos.
    #[serde(default)]
    pub diar_bin_path: String,
    #[serde(default)]
    pub diar_segmentation_path: String,
    #[serde(default)]
    pub diar_embedding_path: String,
    #[serde(default)]
    pub diar_version: String,
}

/// Documentoscopia — motor de OCR (Tesseract) + dados de idioma instalados pelo
/// gerenciador. `engine_bin_path` aponta para o executável (ex.: tesseract.exe);
/// `tessdata_dir` é a pasta dos `.traineddata` baixados. Caminhos globais.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OcrSettings {
    #[serde(default)]
    pub engine_bin_path: String,
    #[serde(default)]
    pub engine_version: String,
    #[serde(default)]
    pub tessdata_dir: String,
    #[serde(default)]
    pub ocr_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_schema")]
    pub schema_version: String,
    #[serde(default)]
    pub profile: PeritoProfile,
    #[serde(default)]
    pub institution: InstitutionSettings,
    #[serde(default)]
    pub appearance: AppearanceSettings,
    #[serde(default)]
    pub paths: PathsSettings,
    #[serde(default)]
    pub ai: AiSettings,
    #[serde(default)]
    pub ocr: OcrSettings,
    /// Biblioteca de cabeçalhos oficiais salvos pelo perito (criador de
    /// cabeçalho do Laudo). Conteúdo opaco aqui (cada item é um doc
    /// ProseMirror + metadados) — o front é dono da estrutura. Global e
    /// reutilizável entre todos os laudos/casos.
    #[serde(default)]
    pub header_templates: Vec<serde_json::Value>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: default_schema(),
            profile: PeritoProfile::default(),
            institution: InstitutionSettings::default(),
            appearance: AppearanceSettings::default(),
            paths: PathsSettings::default(),
            ai: AiSettings::default(),
            ocr: OcrSettings::default(),
            header_templates: Vec::new(),
        }
    }
}

fn default_schema() -> String {
    SCHEMA_VERSION.to_string()
}

/// Caminho absoluto do `app-settings.json`, criando o diretório se preciso.
/// `pub(crate)` para o backup geral incluir um snapshot da config no conjunto.
pub(crate) fn settings_path(app: &AppHandle) -> Result<std::path::PathBuf> {
    let base = app
        .path()
        .app_config_dir()
        .map_err(|e| SicroError::Filesystem(format!("config dir: {e}")))?;
    std::fs::create_dir_all(&base).map_err(|e| {
        SicroError::Filesystem(format!("cannot create config dir: {e}"))
    })?;
    Ok(base.join(SETTINGS_FILENAME))
}

/// Lê as configurações globais. Arquivo ausente → defaults. Arquivo
/// corrompido → defaults (a tela nunca quebra por causa de um JSON ruim).
#[tauri::command]
pub async fn get_app_settings(app: AppHandle) -> Result<AppSettings> {
    let path = settings_path(&app)?;
    if !path.is_file() {
        return Ok(AppSettings::default());
    }
    let bytes = std::fs::read(&path)
        .map_err(|e| SicroError::Filesystem(format!("read settings: {e}")))?;
    Ok(serde_json::from_slice::<AppSettings>(&bytes).unwrap_or_default())
}

/// Grava as configurações globais (escrita atômica).
#[tauri::command]
pub async fn save_app_settings(app: AppHandle, settings: AppSettings) -> Result<()> {
    let path = settings_path(&app)?;
    let bytes = serde_json::to_vec_pretty(&settings)
        .map_err(|e| SicroError::Workspace(format!("serialize settings: {e}")))?;
    crate::filesystem::atomic_write_bytes(&path, &bytes)?;
    Ok(())
}

/// Caminho do arquivo de configurações — exibido na seção de diagnóstico.
#[tauri::command]
pub async fn get_settings_file_path(app: AppHandle) -> Result<String> {
    Ok(settings_path(&app)?.to_string_lossy().to_string())
}

/// Caminho do `pop_calibration.json` no `app_config_dir` (cria a pasta).
fn pop_calibration_path(app: &AppHandle) -> Result<std::path::PathBuf> {
    let base = app
        .path()
        .app_config_dir()
        .map_err(|e| SicroError::Filesystem(format!("config dir: {e}")))?;
    std::fs::create_dir_all(&base).map_err(|e| {
        SicroError::Filesystem(format!("cannot create config dir: {e}"))
    })?;
    Ok(base.join(POP_CALIBRATION_FILENAME))
}

/// Lê a calibração da numeração POP. Ausente/corrompido → objeto vazio `{}`
/// (a numeração cai nos defaults do código, nunca quebra).
#[tauri::command]
pub async fn load_pop_calibration(app: AppHandle) -> Result<serde_json::Value> {
    let path = pop_calibration_path(&app)?;
    if !path.is_file() {
        return Ok(serde_json::json!({}));
    }
    let bytes = std::fs::read(&path)
        .map_err(|e| SicroError::Filesystem(format!("read pop_calibration: {e}")))?;
    Ok(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or_else(|_| serde_json::json!({})))
}

/// Grava a calibração da numeração POP (escrita atômica). `calibration` é um
/// mapa opaco vindo do front (chave → [nx, ny]).
#[tauri::command]
pub async fn save_pop_calibration(
    app: AppHandle,
    calibration: serde_json::Value,
) -> Result<()> {
    let path = pop_calibration_path(&app)?;
    let bytes = serde_json::to_vec_pretty(&calibration)
        .map_err(|e| SicroError::Workspace(format!("serialize pop_calibration: {e}")))?;
    crate::filesystem::atomic_write_bytes(&path, &bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let s = AppSettings::default();
        assert_eq!(s.schema_version, "1");
        assert_eq!(s.appearance.theme, "dark");
        assert_eq!(s.appearance.accent, "#d7a84f");
        assert!(s.profile.full_name.is_empty());
    }

    #[test]
    fn partial_json_fills_missing_with_defaults() {
        // Só "profile.full_name" presente — todo o resto cai no default.
        let json = r#"{ "profile": { "full_name": "André" } }"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.profile.full_name, "André");
        assert_eq!(s.appearance.theme, "dark"); // default preenchido
        assert_eq!(s.schema_version, "1");
    }

    #[test]
    fn roundtrips_through_json() {
        let mut s = AppSettings::default();
        s.profile.full_name = "André Ricardo".to_string();
        s.institution.organization = "Polícia Científica do Amapá".to_string();
        s.appearance.theme = "light".to_string();
        let bytes = serde_json::to_vec(&s).unwrap();
        let back: AppSettings = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.profile.full_name, "André Ricardo");
        assert_eq!(back.institution.organization, "Polícia Científica do Amapá");
        assert_eq!(back.appearance.theme, "light");
    }
}
