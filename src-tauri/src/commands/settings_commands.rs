//! Configurações GLOBAIS do app (fora de qualquer `.sicro`, em `app_config_dir`):
//! perfil, instituição, aparência, caminhos, IA. Todo campo é `#[serde(default)]`
//! para arquivos antigos carregarem; corrompido degrada para defaults. Sem segredos aqui.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::{Result, SicroError};

const SETTINGS_FILENAME: &str = "app-settings.json";
const SCHEMA_VERSION: &str = "1";

/// Calibração da numeração POP do croqui corporal em arquivo PRÓPRIO, para não
/// ser apagada ao salvar as outras configurações. Mapa opaco; o front é dono.
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
    #[serde(default)]
    pub uf: String,
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

/// Caminhos padrão (globais).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathsSettings {
    #[serde(default)]
    pub default_workspace_dir: String,
    #[serde(default)]
    pub default_export_dir: String,
    // CONTRATO: acréscimo do P2.1 (§5.6, contratos-fase2 (f) "Configurações → ffmpeg").
    /// ffmpeg escolhido nas Configurações (vazio = automático: o embutido, senão o do sistema). `SICRO_FFMPEG`
    /// definido vence sempre.
    #[serde(default)]
    pub ffmpeg_path: String,
}

/// IA de transcrição (whisper.cpp + modelo) instalada pelo gerenciador; caminhos globais.
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
    /// Pessoas no vídeo: biblioteca do onnxruntime e modelo YOLOX.
    #[serde(default)]
    pub ort_lib_path: String,
    #[serde(default)]
    pub pessoas_model_path: String,
    #[serde(default)]
    pub pessoas_version: String,
    /// Super-resolução (Real-ESRGAN): modelo, onnxruntime próprio (vazio = o das Pessoas), versão.
    #[serde(default)]
    pub sr_model_path: String,
    #[serde(default)]
    pub sr_ort_lib_path: String,
    #[serde(default)]
    pub sr_version: String,
    // CONTRATO: abertura da Fase 2 (§6.5, §6.6; contratos-fase2.md (b)). Dono: P2.1.
    /// Interruptor "Usar modelos de IA" desligado (padrão: ligado).
    #[serde(default)]
    pub modelos_desligados: bool,
    /// Aceleração por GPU: pasta `ai/gpu` instalada (vazio = não instalada); "pronta" só pelo `estado.json`.
    #[serde(default)]
    pub gpu_dir: String,
    /// Rastreador EdgeTAM: pasta `ai/rastreio` com os 4 grafos (vazio = não instalado); versão instalada.
    #[serde(default)]
    pub rastreio_dir: String,
    #[serde(default)]
    pub rastreio_version: String,
    // CONTRATO: acréscimo (P4.5). Item "Ocultação": pasta `ai/redacao` com os 6 modelos (vazio = não instalado).
    #[serde(default)]
    pub redacao_dir: String,
    #[serde(default)]
    pub redacao_version: String,
}

/// OCR (Tesseract) instalado pelo gerenciador: `engine_bin_path` é o executável;
/// `tessdata_dir` a pasta dos `.traineddata`.
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
    /// Cabeçalhos oficiais salvos pelo perito. Conteúdo opaco (doc ProseMirror
    /// + metadados): o front é dono da estrutura.
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

static FFMPEG: std::sync::RwLock<Option<std::path::PathBuf>> = std::sync::RwLock::new(None);
static PERFIL: std::sync::RwLock<Option<(PeritoProfile, InstitutionSettings)>> = std::sync::RwLock::new(None);

/// Guarda o que vale no processo: a IA (interruptor e GPU, para a análise), o ffmpeg escolhido e o perfil.
fn lembrar(s: &AppSettings) {
    crate::ia::sessao::lembrar(&s.ai);
    *FFMPEG.write().unwrap_or_else(|e| e.into_inner()) = ffmpeg_de(&s.paths);
    *PERFIL.write().unwrap_or_else(|e| e.into_inner()) = Some((s.profile.clone(), s.institution.clone()));
}

/// Perfil e instituição das Configurações (vazios antes da primeira leitura): o relatório da Edição usa quando o caso
/// não diz o perito ou o órgão.
pub fn perfil_configurado() -> (PeritoProfile, InstitutionSettings) {
    PERFIL.read().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

/// O campo "ffmpeg" como vale: vazio ou só espaço = automático.
fn ffmpeg_de(p: &PathsSettings) -> Option<std::path::PathBuf> {
    Some(p.ffmpeg_path.trim()).filter(|p| !p.is_empty()).map(std::path::PathBuf::from)
}

/// ffmpeg escolhido nas Configurações (None = automático). Quem escolhe o binário (`sonda::escolher`) põe este
/// depois de `SICRO_FFMPEG` e antes do embutido.
pub fn ffmpeg_configurado() -> Option<std::path::PathBuf> {
    FFMPEG.read().unwrap_or_else(|e| e.into_inner()).clone()
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
    let s = if path.is_file() {
        let bytes = std::fs::read(&path)
            .map_err(|e| SicroError::Filesystem(format!("read settings: {e}")))?;
        serde_json::from_slice::<AppSettings>(&bytes).unwrap_or_default()
    } else {
        AppSettings::default()
    };
    lembrar(&s);
    Ok(s)
}

/// Grava as configurações globais (escrita atômica).
#[tauri::command]
pub async fn save_app_settings(app: AppHandle, settings: AppSettings) -> Result<()> {
    let path = settings_path(&app)?;
    let bytes = serde_json::to_vec_pretty(&settings)
        .map_err(|e| SicroError::Workspace(format!("serialize settings: {e}")))?;
    crate::filesystem::atomic_write_bytes(&path, &bytes)?;
    lembrar(&settings);
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

    /// Campos novos da Fase 2 com padrão (arquivo antigo abre com IA ligada, sem GPU, ffmpeg automático) e o
    /// ffmpeg escolhido vale no processo; espaço em branco = automático.
    #[test]
    fn campos_da_fase_2_e_ffmpeg() {
        let s: AppSettings = serde_json::from_str(r#"{ "ai": { "whisper_bin_path": "/w" }, "paths": { "default_workspace_dir": "/c" } }"#).unwrap();
        assert!(!s.ai.modelos_desligados && s.ai.gpu_dir.is_empty() && s.ai.rastreio_dir.is_empty());
        assert_eq!((s.ai.whisper_bin_path.as_str(), s.paths.ffmpeg_path.as_str()), ("/w", ""));
        let mut c = AppSettings::default();
        c.paths.ffmpeg_path = "/opt/ff/ffmpeg".into();
        c.ai.modelos_desligados = true;
        let volta: AppSettings = serde_json::from_slice(&serde_json::to_vec(&c).unwrap()).unwrap();
        assert_eq!(volta.paths.ffmpeg_path, "/opt/ff/ffmpeg");
        assert!(volta.ai.modelos_desligados);
        // o global do processo não muda aqui (a escolha do ffmpeg dos outros testes o lê): vale no filho abaixo
        assert_eq!(ffmpeg_de(&c.paths), Some(std::path::PathBuf::from("/opt/ff/ffmpeg")));
        c.paths.ffmpeg_path = "  ".into();
        assert_eq!(ffmpeg_de(&c.paths), None);
    }

    /// No filho: grava as Configurações em vigor com o campo apontando para `SICRO_TESTE_CAMPO_FFMPEG` e imprime o
    /// binário que a Edição escolhe.
    #[test]
    fn filho_campo_ffmpeg() {
        let Some(campo) = std::env::var_os("SICRO_TESTE_CAMPO_FFMPEG") else { return };
        let mut c = AppSettings::default();
        c.paths.ffmpeg_path = campo.to_string_lossy().into_owned();
        lembrar(&c);
        let r = crate::edicao::ffmpeg::sonda::escolher().map(|(p, o)| format!("{}|{o:?}", p.display()));
        println!("ESCOLHIDO {}", r.unwrap_or_else(|e| format!("erro|{e}")));
    }

    /// O campo das Configurações vale de fato na escolha do binário da Edição (processo filho: o global é do
    /// processo), e `SICRO_FFMPEG` definido vence sempre.
    #[test]
    fn campo_do_ffmpeg_vale_na_edicao() {
        let d = tempfile::tempdir().unwrap();
        let (campo, var) = (d.path().join("ffmpeg do campo"), d.path().join("ffmpeg da variável"));
        for f in [&campo, &var] {
            std::fs::write(f, "x").unwrap();
        }
        let filho = |sicro_ffmpeg: Option<&std::path::Path>| {
            let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
            cmd.args(["--exact", "commands::settings_commands::tests::filho_campo_ffmpeg", "--nocapture", "--test-threads", "1"])
                .env("SICRO_TESTE_CAMPO_FFMPEG", &campo);
            match sicro_ffmpeg {
                Some(v) => cmd.env("SICRO_FFMPEG", v),
                None => cmd.env_remove("SICRO_FFMPEG"),
            };
            let o = cmd.output().unwrap();
            let out = String::from_utf8_lossy(&o.stdout).into_owned();
            assert!(o.status.success(), "{out}");
            out.lines().find_map(|l| l.split_once("ESCOLHIDO ").map(|x| x.1.to_string())).unwrap_or_else(|| panic!("{out}"))
        };
        assert_eq!(filho(None), format!("{}|Configurado", campo.display()));
        assert_eq!(filho(Some(&var)), format!("{}|Configurado", var.display()));
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
