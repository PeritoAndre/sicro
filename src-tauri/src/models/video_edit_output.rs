//! Exportação final da Edição (migration 027, `video_edit_outputs`). Caminhos
//! relativos ao caso.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EstadoSaida {
    #[default]
    AConferir,
    Concluida,
}

impl EstadoSaida {
    pub fn as_str(self) -> &'static str {
        match self {
            EstadoSaida::AConferir => "a_conferir",
            EstadoSaida::Concluida => "concluida",
        }
    }

    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "a_conferir" => Some(EstadoSaida::AConferir),
            "concluida" => Some(EstadoSaida::Concluida),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoEditOutput {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub projeto_id: String,
    pub titulo: String,
    pub pasta: String,
    pub arquivo: String,
    /// Do arquivo.
    pub sha256: String,
    /// Quadros do grafo antes de codificar (serve ao "refazer").
    pub composto_sha256: Option<String>,
    /// Quadros do arquivo entregue, decodificado.
    pub decodificado_sha256: Option<String>,
    pub audio_sha256: Option<String>,
    pub tamanho: u64,
    pub duracao_s: f64,
    pub n_quadros: i64,
    pub predefinicao: String,
    pub codificador: String,
    pub tecnico_rel: Option<String>,
    pub retrato_rel: String,
    pub projeto_sha256: String,
    pub autor: serde_json::Value,
    pub refeita_de: Option<Uuid>,
    pub estado: EstadoSaida,
    /// Tem ocultação ou silêncio: concluída sem `conferida_por` = não conferida.
    pub exige_conferencia: bool,
    pub conferida_por: Option<String>,
    pub conferida_em: Option<DateTime<Utc>>,
    pub criado_em: DateTime<Utc>,
}
