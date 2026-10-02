//! Ocorrência: `Occurrence` é a linha canônica no SQLite do workspace;
//! `RecentOccurrence` é o resumo global guardado em `recent.json`.

use chrono::{DateTime, Local, Utc};
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
    /// Nome livre do caso dado pelo perito ("Laudo 63404/26 — Km 09");
    /// é o rótulo principal, os demais campos são complemento.
    #[serde(default)]
    pub titulo: Option<String>,
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

    // Proveniência do .sicroapp; NULL em caso criado à mão.
    #[serde(default)]
    pub import_id: Option<Uuid>,
    #[serde(default)]
    pub original_mobile_id: Option<String>,
    #[serde(default)]
    pub primary_accuracy_m: Option<f64>,
    #[serde(default)]
    pub resultado: Option<String>,
    /// `caso.json` íntegro.
    #[serde(default)]
    pub raw_case_json: Option<String>,
    /// `metadados.json` íntegro.
    #[serde(default)]
    pub raw_metadata_json: Option<String>,
    /// `localizacao.json` íntegro.
    #[serde(default)]
    pub raw_location_json: Option<String>,
}

/// Entrada de `create_occurrence`. Tudo opcional — ocorrência vazia é válida.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct NewOccurrenceInput {
    /// O único campo que o Início pede (e mesmo ele é opcional).
    #[serde(default)]
    pub titulo: Option<String>,
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
    /// `None` = pasta Documentos do usuário.
    pub parent_directory: Option<String>,
}

/// Patch de `update_occurrence`. O perito é a palavra final; strings em branco
/// viram NULL no comando. A proveniência (import_id, original_mobile_id,
/// primary_accuracy_m, raw_*) nunca é tocada — o .sicroapp original fica intacto.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct OccurrenceEdit {
    #[serde(default)]
    pub titulo: Option<String>,
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

/// Dados + caminho do workspace numa ida só.
#[derive(Debug, Clone, Serialize)]
pub struct LoadedOccurrence {
    pub occurrence: Occurrence,
    pub workspace_path: String,
}

/// Entrada do `recent.json` global.
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

/// Rótulo do caso: o nome dado pelo perito; sem nome, "BO — tipo — município"
/// (casos antigos); sem nada, a data de criação. Espelhado em
/// `occurrenceLabel` (src/types/occurrence.ts) — mudar nos dois.
pub fn build_label(o: &Occurrence) -> String {
    if let Some(t) = o.titulo.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        return t.to_string();
    }
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
        format!(
            "Caso de {}",
            o.created_at.with_timezone(&Local).format("%d/%m/%Y")
        )
    } else {
        parts.join(" — ")
    }
}
