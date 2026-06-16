//! Calculador de Velocidade — modelos persistidos (Fase 2: camada de dados).
//!
//! Espelham `src/types/video_speed.ts` no front-end. Os nomes de campo são
//! snake_case para casar com o wire serde. Os structs carregam tipos
//! **estruturados** (matriz da homografia, pontos da trajetória, sigmas do
//! Monte Carlo); o repositório (`video_speed_repo`) é quem (de)serializa
//! esses tipos para as colunas `*_json` da tabela.
//!
//! ## Reprodutibilidade pericial
//!
//! O número do laudo precisa ser reproduzível. Por isso:
//!   - [`VideoSpeedCalculation::mc_seed`] e [`VideoSpeedCalculation::mc_sigmas`]
//!     guardam **exatamente** o que o Monte Carlo usou.
//!   - Cada [`TrajectoryPoint`] aponta para um frame coletado real do
//!     storyboard (`storyboard_frame_id` / `export_id`), herdando
//!     `actual_timestamp_s` e `delta_s` — o tempo nunca é "inventado".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Calibração

/// Uma correspondência pixel↔mundo usada para ajustar a homografia.
///
/// Para `method = "plane"` (DLT) há 4 pontos; para `method = "line"` há 2
/// (as duas pontas de um segmento cujo comprimento real está embutido nas
/// coordenadas de mundo).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlPoint {
    pub px: f64,
    pub py: f64,
    pub world_x_m: f64,
    pub world_y_m: f64,
    #[serde(default)]
    pub label: Option<String>,
}

/// Calibração congelada: mapeia pixels da imagem para metros no plano da via.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoSpeedCalibration {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    /// sha256 do vídeo de origem.
    pub media_hash: String,
    /// `"line"` | `"plane"`.
    pub method: String,
    pub control_points: Vec<ControlPoint>,
    /// `"campo"` | `"norma_viaria"` | `"entre_eixos"`.
    pub reference_source: String,
    /// Homografia 3x3 row-major (imagem px → mundo m), 9 f64.
    pub homography: [f64; 9],
    /// RMS de reprojeção em pixels (None se não calculado).
    pub residuals_px: Option<f64>,
    /// Reservado para um futuro modelo de distorção de lente; NULL hoje.
    #[serde(default)]
    pub distortion_model: Option<serde_json::Value>,
    pub author: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Cálculo

/// Uma posição marcada do veículo, amarrada a um frame coletado real para
/// herdar o tempo do frame (reprodutibilidade pericial).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrajectoryPoint {
    /// Frame do storyboard onde o ponto foi marcado (o frame coletado).
    #[serde(default)]
    pub storyboard_frame_id: Option<Uuid>,
    /// O export (PNG) que respalda o frame do storyboard, quando disponível.
    #[serde(default)]
    pub export_id: Option<Uuid>,
    pub px: f64,
    pub py: f64,
    /// Incerteza de marcação em pixels (1σ) deste ponto.
    pub u_px: f64,
    /// Tempo real do frame, herdado do storyboard.
    pub actual_timestamp_s: f64,
    /// Erro de seek do ffmpeg (pedido − real), herdado para auditoria.
    #[serde(default)]
    pub delta_s: Option<f64>,
    /// True quando marcado por um humano (sempre true nesta fase).
    pub manual: bool,
}

/// Sigmas por fonte usados no Monte Carlo — persistidos para reprodutibilidade.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McSigmas {
    pub calibration_px: f64,
    pub world_m: f64,
    pub trajectory_px: f64,
    pub time_s: f64,
}

/// Resultado de um cálculo de velocidade: ajuste por regressão + distribuição
/// Monte Carlo, com tudo o que o laudo precisa para reproduzir o número.
///
/// Os campos de incerteza são `Option` porque nem todo cálculo os tem:
///   - **2 pontos** (média): há `velocity_kmh`/`vx`/`vy`, mas SEM IC e SEM
///     Monte Carlo — todos os `Option` ficam `None`, e `residuals` vazio.
///   - **≥3 pontos, calibração de plano (4 pts) E σ informados pelo perito**:
///     regressão (IC) + Monte Carlo, todos preenchidos.
///   - **≥3 pontos SEM σ, ou calibração por linha (2 pts)**: regressão (IC)
///     preenchida, mas Monte Carlo `None` (MC exige 4 pontos coplanares e
///     incertezas informadas — não inventamos σ).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoSpeedCalculation {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub media_hash: String,
    /// FK → [`VideoSpeedCalibration::id`].
    pub calibration_id: Uuid,
    pub points: Vec<TrajectoryPoint>,
    /// |v| = sqrt(vx²+vy²), em km/h (valor de destaque do laudo).
    pub velocity_kmh: f64,
    pub vx_m_per_s: f64,
    pub vy_m_per_s: f64,
    /// Erro-padrão de |v| em m/s (None no caso de 2 pontos).
    pub se_m_per_s: Option<f64>,
    /// Limite inferior do intervalo de confiança, em km/h (None se 2 pontos).
    pub ci_low: Option<f64>,
    /// Limite superior do intervalo de confiança, em km/h (None se 2 pontos).
    pub ci_high: Option<f64>,
    /// Nível de confiança de `[ci_low, ci_high]`, ex.: 0.95 (None se 2 pontos).
    pub confidence: Option<f64>,
    pub r_squared: Option<f64>,
    /// Resíduo 2D por ponto (vazio no caso de 2 pontos).
    pub residuals: Vec<f64>,
    /// Semente RNG resolvida usada pelo Monte Carlo (reprodutibilidade).
    /// QUANDO o MC roda, é SEMPRE persistida. None se o MC não rodou.
    pub mc_seed: Option<i64>,
    /// Sigmas por fonte exatos usados (reprodutibilidade). None se sem MC.
    pub mc_sigmas: Option<McSigmas>,
    /// Iterações Monte Carlo pedidas (None se sem MC).
    pub mc_n: Option<i64>,
    /// Iterações descartadas (ex.: homografia singular). None se sem MC.
    pub mc_failed: Option<i64>,
    pub mc_mean_kmh: Option<f64>,
    pub mc_median_kmh: Option<f64>,
    pub mc_p2_5_kmh: Option<f64>,
    pub mc_p97_5_kmh: Option<f64>,
    /// Ressalvas técnicas a transcrever no laudo.
    pub limitations: Vec<String>,
    /// Trilha de auditoria livre (versões de ferramentas, notas do operador…).
    #[serde(default)]
    pub audit: serde_json::Value,
    pub author: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Inputs dos comandos Tauri (Fase 3)

/// Entrada de `create_speed_calibration`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSpeedCalibrationInput {
    /// sha256 do vídeo já registrado nesta ocorrência.
    pub media_hash: String,
    /// `"line"` (2 pontos) | `"plane"` (4 pontos, DLT).
    pub method: String,
    /// Correspondências pixel↔mundo (4 para plane, 2 para line).
    pub control_points: Vec<ControlPoint>,
    /// `"campo"` | `"norma_viaria"` | `"entre_eixos"`.
    pub reference_source: String,
    /// Autor da calibração (opcional; default vazio).
    #[serde(default)]
    pub author: Option<String>,
}

/// Entrada de `compute_speed`.
#[derive(Debug, Clone, Deserialize)]
pub struct ComputeSpeedInput {
    /// Calibração já gravada a usar na projeção pixel→mundo.
    pub calibration_id: Uuid,
    /// Trajetória marcada — cada ponto amarrado a um frame coletado real.
    pub points: Vec<TrajectoryPoint>,
    /// Iterações Monte Carlo (>= 10). Ignorado no caso de 2 pontos.
    #[serde(default)]
    pub mc_n: Option<u32>,
    /// Sigmas por fonte para o Monte Carlo. Ignorado no caso de 2 pontos.
    #[serde(default)]
    pub mc_sigmas: Option<McSigmas>,
    /// Nível de confiança desejado para o IC (apenas 0.95 suportado nesta fase).
    #[serde(default)]
    pub confidence: Option<f64>,
    /// Autor do cálculo (opcional; default vazio).
    #[serde(default)]
    pub author: Option<String>,
}
