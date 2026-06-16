//! Medição de Distância por fotogrametria — modelos persistidos.
//!
//! Espelham `src/types/video_distance.ts` no front-end (snake_case para casar
//! com o wire serde). A medição CONSOME uma [`super::VideoSpeedCalibration`]
//! existente (FK `calibration_id`) — nunca recalibra a cena. O repositório
//! (`video_distance_repo`) (de)serializa as colunas `*_json`.
//!
//! ## Incerteza: só Monte Carlo (sem IC de regressão)
//!
//! Diferente da velocidade, a distância entre 2 pontos NÃO tem intervalo de
//! confiança de regressão — a ÚNICA fonte de incerteza é o Monte Carlo. Por
//! isso TODO o bloco `mc_*` é `Option`: sem σ informado pelo perito, sai só a
//! distância pontual (`distance_m`) e os `mc_*` ficam `None`. Reprodutibilidade
//! pericial: quando o MC roda, `mc_seed` e `mc_sigmas` guardam exatamente o que
//! foi usado.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Sigmas por fonte usados no Monte Carlo de distância — persistidos para
/// reprodutibilidade. SEM σ temporal: distância não tem tempo (≠ velocidade).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McSigmasDistance {
    /// σ de marcação dos pontos de calibração (px).
    pub calibration_px: f64,
    /// σ de medição das dimensões reais da calibração (m).
    pub world_m: f64,
    /// σ de marcação dos DOIS pontos medidos (px).
    pub measure_px: f64,
}

/// Uma medição de distância persistida: dois pontos em pixel + a distância
/// real, e (quando o perito informou σ) a distribuição Monte Carlo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoDistanceMeasurement {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub media_hash: String,
    /// FK → [`super::VideoSpeedCalibration::id`] (a geometria da cena consumida).
    pub calibration_id: Uuid,
    pub p1_px: f64,
    pub p1_py: f64,
    pub p2_px: f64,
    pub p2_py: f64,
    /// Distância pontual em metros (sempre presente).
    pub distance_m: f64,
    /// Semente RNG usada pelo Monte Carlo (None se o MC não rodou).
    pub mc_seed: Option<i64>,
    /// Sigmas por fonte exatos usados (None se sem MC).
    pub mc_sigmas: Option<McSigmasDistance>,
    /// Iterações Monte Carlo pedidas (None se sem MC).
    pub mc_n: Option<i64>,
    /// Iterações descartadas (calibração singular sob perturbação). None se sem MC.
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

/// Entrada de `create_distance_measurement`. O `media_hash` NÃO vem daqui — é
/// herdado da calibração referenciada (a calibração já fixou a mídia).
#[derive(Debug, Clone, Deserialize)]
pub struct CreateDistanceMeasurementInput {
    /// Calibração já gravada a consumir na projeção pixel→mundo.
    pub calibration_id: Uuid,
    pub p1_px: f64,
    pub p1_py: f64,
    pub p2_px: f64,
    pub p2_py: f64,
    /// Iterações Monte Carlo (>= 10). Ignorado se `mc_sigmas` ausente/zerado.
    #[serde(default)]
    pub mc_n: Option<u32>,
    /// Sigmas por fonte para o Monte Carlo. Sem isto, sai só a distância pontual.
    #[serde(default)]
    pub mc_sigmas: Option<McSigmasDistance>,
    /// Autor da medição (opcional; default vazio).
    #[serde(default)]
    pub author: Option<String>,
}
