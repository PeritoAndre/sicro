//! Medição de distância por fotogrametria — modelos persistidos. Espelhados em
//! `src/types/video_distance.ts` — mudar nos dois. A medição consome uma
//! `VideoSpeedCalibration` existente; nunca recalibra a cena.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Sigmas do Monte Carlo de distância — sem σ temporal (distância não tem tempo).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McSigmasDistance {
    /// Marcação dos pontos de calibração (px).
    pub calibration_px: f64,
    /// Dimensões reais da calibração (m).
    pub world_m: f64,
    /// Marcação dos dois pontos medidos (px).
    pub measure_px: f64,
}

/// Dois pontos em pixel + distância real. Não há IC de regressão: a única
/// incerteza é o Monte Carlo, por isso todo o bloco `mc_*` é `Option`
/// (sem σ do perito, sai só `distance_m`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoDistanceMeasurement {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub media_hash: String,
    /// FK → `VideoSpeedCalibration` (geometria da cena consumida).
    pub calibration_id: Uuid,
    pub p1_px: f64,
    pub p1_py: f64,
    pub p2_px: f64,
    pub p2_py: f64,
    /// Metros; sempre presente.
    pub distance_m: f64,
    /// Semente RNG — persistida quando o MC roda.
    pub mc_seed: Option<i64>,
    /// Sigmas exatos usados.
    pub mc_sigmas: Option<McSigmasDistance>,
    /// Iterações pedidas.
    pub mc_n: Option<i64>,
    /// Iterações descartadas (calibração singular sob perturbação).
    pub mc_failed: Option<i64>,
    pub mc_mean_m: Option<f64>,
    pub mc_median_m: Option<f64>,
    pub mc_p2_5_m: Option<f64>,
    pub mc_p97_5_m: Option<f64>,
    /// Ressalvas técnicas a transcrever no laudo.
    pub limitations: Vec<String>,
    /// Trilha de auditoria livre.
    #[serde(default)]
    pub audit: serde_json::Value,
    pub author: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Input do comando Tauri

/// Entrada de `create_distance_measurement`. O `media_hash` vem da calibração
/// referenciada, não daqui.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateDistanceMeasurementInput {
    pub calibration_id: Uuid,
    pub p1_px: f64,
    pub p1_py: f64,
    pub p2_px: f64,
    pub p2_py: f64,
    /// >= 10; ignorado sem `mc_sigmas`.
    #[serde(default)]
    pub mc_n: Option<u32>,
    /// Sem isto, sai só a distância pontual.
    #[serde(default)]
    pub mc_sigmas: Option<McSigmasDistance>,
    /// Default vazio.
    #[serde(default)]
    pub author: Option<String>,
}
