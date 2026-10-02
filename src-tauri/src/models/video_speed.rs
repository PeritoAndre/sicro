//! Calculador de Velocidade — modelos persistidos. Espelhados em
//! `src/types/video_speed.ts` — mudar nos dois. Tipos estruturados aqui;
//! o `video_speed_repo` (de)serializa para as colunas `*_json`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Calibração

/// Correspondência pixel↔mundo da homografia: 4 pontos em "plane" (DLT),
/// 2 em "line" (pontas de um segmento de comprimento real conhecido).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlPoint {
    pub px: f64,
    pub py: f64,
    pub world_x_m: f64,
    pub world_y_m: f64,
    #[serde(default)]
    pub label: Option<String>,
}

/// Calibração congelada: pixels da imagem → metros no plano da via.
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
    /// 3x3 row-major (px → m).
    pub homography: [f64; 9],
    /// RMS de reprojeção em px.
    pub residuals_px: Option<f64>,
    /// Reservado para modelo de distorção de lente; NULL hoje.
    #[serde(default)]
    pub distortion_model: Option<serde_json::Value>,
    pub author: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Cálculo

/// Posição marcada do veículo, amarrada a um frame coletado real: o tempo é
/// herdado do frame, nunca inventado (reprodutibilidade pericial).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrajectoryPoint {
    /// Frame do storyboard onde o ponto foi marcado.
    #[serde(default)]
    pub storyboard_frame_id: Option<Uuid>,
    /// PNG que respalda o frame, quando houver.
    #[serde(default)]
    pub export_id: Option<Uuid>,
    pub px: f64,
    pub py: f64,
    /// Incerteza de marcação (1σ, px).
    pub u_px: f64,
    /// Herdado do storyboard.
    pub actual_timestamp_s: f64,
    /// Erro de seek do ffmpeg (pedido − real), para auditoria.
    #[serde(default)]
    pub delta_s: Option<f64>,
    /// Marcado por humano (sempre true por enquanto).
    pub manual: bool,
}

/// Sigmas por fonte do Monte Carlo — persistidos para reprodutibilidade.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McSigmas {
    pub calibration_px: f64,
    pub world_m: f64,
    pub trajectory_px: f64,
    pub time_s: f64,
}

/// Resultado do cálculo: regressão + Monte Carlo. Os `Option` ficam `None`
/// conforme o caso: 2 pontos → sem IC e sem MC; ≥3 pontos sem σ ou calibração
/// por linha → IC sem MC (MC exige 4 pontos coplanares e σ informados).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoSpeedCalculation {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub media_hash: String,
    /// FK → `VideoSpeedCalibration`.
    pub calibration_id: Uuid,
    pub points: Vec<TrajectoryPoint>,
    /// |v| em km/h (valor de destaque do laudo).
    pub velocity_kmh: f64,
    pub vx_m_per_s: f64,
    pub vy_m_per_s: f64,
    /// Erro-padrão de |v| (m/s).
    pub se_m_per_s: Option<f64>,
    /// Intervalo de confiança (km/h).
    pub ci_low: Option<f64>,
    pub ci_high: Option<f64>,
    /// Nível do IC, ex.: 0.95.
    pub confidence: Option<f64>,
    pub r_squared: Option<f64>,
    /// Resíduo 2D por ponto.
    pub residuals: Vec<f64>,
    /// Semente RNG — sempre persistida quando o MC roda.
    pub mc_seed: Option<i64>,
    /// Sigmas exatos usados.
    pub mc_sigmas: Option<McSigmas>,
    /// Iterações pedidas.
    pub mc_n: Option<i64>,
    /// Iterações descartadas (ex.: homografia singular).
    pub mc_failed: Option<i64>,
    pub mc_mean_kmh: Option<f64>,
    pub mc_median_kmh: Option<f64>,
    pub mc_p2_5_kmh: Option<f64>,
    pub mc_p97_5_kmh: Option<f64>,
    /// Ressalvas técnicas a transcrever no laudo.
    pub limitations: Vec<String>,
    /// Trilha de auditoria livre.
    #[serde(default)]
    pub audit: serde_json::Value,
    pub author: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Inputs dos comandos Tauri

/// Entrada de `create_speed_calibration`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSpeedCalibrationInput {
    /// sha256 de um vídeo já registrado na ocorrência.
    pub media_hash: String,
    /// `"line"` (2 pontos) | `"plane"` (4 pontos, DLT).
    pub method: String,
    pub control_points: Vec<ControlPoint>,
    /// `"campo"` | `"norma_viaria"` | `"entre_eixos"`.
    pub reference_source: String,
    /// Default vazio.
    #[serde(default)]
    pub author: Option<String>,
}

/// Entrada de `compute_speed`.
#[derive(Debug, Clone, Deserialize)]
pub struct ComputeSpeedInput {
    pub calibration_id: Uuid,
    /// Cada ponto amarrado a um frame coletado real.
    pub points: Vec<TrajectoryPoint>,
    /// >= 10; ignorado com 2 pontos.
    #[serde(default)]
    pub mc_n: Option<u32>,
    /// Ignorado com 2 pontos.
    #[serde(default)]
    pub mc_sigmas: Option<McSigmas>,
    /// Só 0.95 é suportado por enquanto.
    #[serde(default)]
    pub confidence: Option<f64>,
    /// Default vazio.
    #[serde(default)]
    pub author: Option<String>,
}
