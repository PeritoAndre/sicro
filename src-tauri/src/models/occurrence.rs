//! Occurrence domain model.
//!
//! `Occurrence` is the canonical row stored in each workspace's SQLite.
//! `RecentOccurrence` is the lightweight summary kept globally in the app's
//! config dir (independent from any single workspace).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OccurrenceStatus {
    Aberta,
    EmAndamento,
    Concluida,
    Arquivada,
}

impl OccurrenceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Aberta => "aberta",
            Self::EmAndamento => "em_andamento",
            Self::Concluida => "concluida",
            Self::Arquivada => "arquivada",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "aberta" => Some(Self::Aberta),
            "em_andamento" => Some(Self::EmAndamento),
            "concluida" => Some(Self::Concluida),
            "arquivada" => Some(Self::Arquivada),
            _ => None,
        }
    }
}

impl Default for OccurrenceStatus {
    fn default() -> Self {
        Self::Aberta
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Occurrence {
    pub id: Uuid,
    pub numero_bo: Option<String>,
    pub protocolo: Option<String>,
    pub requisicao: Option<String>,
    pub oficio: Option<String>,
    pub delegacia: Option<String>,
    pub tipo_pericia: Option<String>,
    pub natureza: Option<String>,
    pub municipio: Option<String>,
    pub bairro: Option<String>,
    pub logradouro: Option<String>,
    pub referencia: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub data_fato: Option<DateTime<Utc>>,
    pub data_acionamento: Option<DateTime<Utc>>,
    pub data_chegada: Option<DateTime<Utc>>,
    pub data_encerramento: Option<DateTime<Utc>>,
    pub peritos: Vec<String>,
    pub status: OccurrenceStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Spike D — fields populated when the occurrence is materialised from a
    // .sicroapp. NULL for hand-created (Spike A) rows.
    #[serde(default)]
    pub import_id: Option<Uuid>,
    #[serde(default)]
    pub original_mobile_id: Option<String>,
    #[serde(default)]
    pub primary_accuracy_m: Option<f64>,
    #[serde(default)]
    pub resultado: Option<String>,
    /// Verbatim payload of `caso.json` from the .sicroapp.
    #[serde(default)]
    pub raw_case_json: Option<String>,
    /// Verbatim payload of `metadados.json` from the .sicroapp.
    #[serde(default)]
    pub raw_metadata_json: Option<String>,
    /// Verbatim payload of `localizacao.json` from the .sicroapp.
    #[serde(default)]
    pub raw_location_json: Option<String>,
}

/// Input payload accepted by `create_occurrence`. All fields are optional —
/// even an entirely empty occurrence is valid (the perito fills it in later).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct NewOccurrenceInput {
    pub numero_bo: Option<String>,
    pub protocolo: Option<String>,
    /// Nº do ofício de requisição da Polícia Civil (origem externa) — distinto
    /// do `protocolo` (gerado na Polícia Científica e usado como nº do laudo).
    #[serde(default)]
    pub oficio: Option<String>,
    pub tipo_pericia: Option<String>,
    pub municipio: Option<String>,
    #[serde(default)]
    pub peritos: Vec<String>,
    /// If `None`, the workspace is created inside the user's Documents folder.
    pub parent_directory: Option<String>,
}

/// Patch aceito por `update_occurrence`. O perito é a palavra final (caso de
/// expediente nasce no Desktop; coleta de campo é corrigida depois). Todos os
/// campos são opcionais; strings em branco viram NULL no comando. A proveniência
/// (import_id, original_mobile_id, primary_accuracy_m, raw_*) NUNCA é tocada —
/// o pacote .sicroapp original permanece intacto no disco.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct OccurrenceEdit {
    pub numero_bo: Option<String>,
    pub protocolo: Option<String>,
    pub requisicao: Option<String>,
    pub oficio: Option<String>,
    pub delegacia: Option<String>,
    pub tipo_pericia: Option<String>,
    pub natureza: Option<String>,
    pub resultado: Option<String>,
    pub municipio: Option<String>,
    pub bairro: Option<String>,
    pub logradouro: Option<String>,
    pub referencia: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// String do enum (`aberta`/`em_andamento`/`concluida`/`arquivada`).
    pub status: Option<String>,
    pub peritos: Option<Vec<String>>,
}

/// Returned by create/open commands so the front-end gets both the persistent
/// data and the on-disk path in a single round-trip.
#[derive(Debug, Clone, Serialize)]
pub struct LoadedOccurrence {
    pub occurrence: Occurrence,
    pub workspace_path: String,
}

/// Entry persisted in the global `recent.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentOccurrence {
    pub workspace_id: Uuid,
    pub workspace_path: String,
    pub occurrence_label: String,
    pub tipo_pericia: Option<String>,
    pub municipio: Option<String>,
    pub status: OccurrenceStatus,
    pub last_opened_at: DateTime<Utc>,
}

impl RecentOccurrence {
    pub fn from_occurrence(
        occurrence: &Occurrence,
        workspace_path: &str,
        workspace_id: Uuid,
    ) -> Self {
        Self {
            workspace_id,
            workspace_path: workspace_path.to_string(),
            occurrence_label: build_label(occurrence),
            tipo_pericia: occurrence.tipo_pericia.clone(),
            municipio: occurrence.municipio.clone(),
            status: occurrence.status,
            last_opened_at: Utc::now(),
        }
    }
}

fn build_label(o: &Occurrence) -> String {
    let mut parts = Vec::new();
    if let Some(bo) = &o.numero_bo {
        parts.push(format!("BO {bo}"));
    }
    if let Some(tipo) = &o.tipo_pericia {
        parts.push(tipo.clone());
    }
    if let Some(municipio) = &o.municipio {
        parts.push(municipio.clone());
    }
    if parts.is_empty() {
        format!("Ocorrência {}", &o.id.to_string()[..8])
    } else {
        parts.join(" — ")
    }
}
