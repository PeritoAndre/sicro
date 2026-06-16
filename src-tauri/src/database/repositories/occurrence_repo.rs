//! Read/write helpers for the `occurrences` table and the `audit_logs` table.
//!
//! Keep SQL strings here — modules above this layer never touch raw SQL.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::{Result, SicroError};
use crate::models::{Occurrence, OccurrenceStatus};

/// Column order MUST match the `params![...]` order in `insert` / `update_full`
/// and the field order accessed in `row_to_occurrence`.
const COLUMNS: &str = "
    id, numero_bo, protocolo, requisicao, oficio, delegacia,
    tipo_pericia, natureza, municipio, bairro, logradouro, referencia,
    latitude, longitude,
    data_fato, data_acionamento, data_chegada, data_encerramento,
    peritos, status, created_at, updated_at,
    import_id, original_mobile_id, primary_accuracy_m, resultado,
    raw_case_json, raw_metadata_json, raw_location_json
";

pub fn insert(conn: &Connection, occ: &Occurrence) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO occurrences ({COLUMNS}) VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, \
              ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, \
              ?23, ?24, ?25, ?26, ?27, ?28, ?29)"
        ),
        params![
            occ.id.to_string(),
            occ.numero_bo,
            occ.protocolo,
            occ.requisicao,
            occ.oficio,
            occ.delegacia,
            occ.tipo_pericia,
            occ.natureza,
            occ.municipio,
            occ.bairro,
            occ.logradouro,
            occ.referencia,
            occ.latitude,
            occ.longitude,
            occ.data_fato.map(|d| d.to_rfc3339()),
            occ.data_acionamento.map(|d| d.to_rfc3339()),
            occ.data_chegada.map(|d| d.to_rfc3339()),
            occ.data_encerramento.map(|d| d.to_rfc3339()),
            serde_json::to_string(&occ.peritos)?,
            occ.status.as_str(),
            occ.created_at.to_rfc3339(),
            occ.updated_at.to_rfc3339(),
            occ.import_id.map(|u| u.to_string()),
            occ.original_mobile_id,
            occ.primary_accuracy_m,
            occ.resultado,
            occ.raw_case_json,
            occ.raw_metadata_json,
            occ.raw_location_json,
        ],
    )?;
    Ok(())
}

/// Sobrescreve as colunas EDITÁVEIS de uma ocorrência (palavra final do perito).
/// Preserva `id`, `created_at` e a proveniência (`import_id`,
/// `original_mobile_id`, `primary_accuracy_m`, `raw_*`). O chamador define o novo
/// `updated_at` em `occ`.
pub fn update_full(conn: &Connection, occ: &Occurrence) -> Result<()> {
    let n = conn.execute(
        "UPDATE occurrences SET
            numero_bo = ?2, protocolo = ?3, requisicao = ?4, oficio = ?5,
            delegacia = ?6, tipo_pericia = ?7, natureza = ?8, municipio = ?9,
            bairro = ?10, logradouro = ?11, referencia = ?12,
            latitude = ?13, longitude = ?14,
            data_fato = ?15, data_acionamento = ?16, data_chegada = ?17,
            data_encerramento = ?18, peritos = ?19, status = ?20,
            resultado = ?21, updated_at = ?22
         WHERE id = ?1",
        params![
            occ.id.to_string(),
            occ.numero_bo,
            occ.protocolo,
            occ.requisicao,
            occ.oficio,
            occ.delegacia,
            occ.tipo_pericia,
            occ.natureza,
            occ.municipio,
            occ.bairro,
            occ.logradouro,
            occ.referencia,
            occ.latitude,
            occ.longitude,
            occ.data_fato.map(|d| d.to_rfc3339()),
            occ.data_acionamento.map(|d| d.to_rfc3339()),
            occ.data_chegada.map(|d| d.to_rfc3339()),
            occ.data_encerramento.map(|d| d.to_rfc3339()),
            serde_json::to_string(&occ.peritos)?,
            occ.status.as_str(),
            occ.resultado,
            occ.updated_at.to_rfc3339(),
        ],
    )?;
    if n == 0 {
        return Err(SicroError::Validation(format!(
            "ocorrência {} não encontrada para atualizar",
            occ.id
        )));
    }
    Ok(())
}

pub fn find_by_id(conn: &Connection, id: &Uuid) -> Result<Option<Occurrence>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM occurrences WHERE id = ?1"
    ))?;
    let occ = stmt
        .query_row([id.to_string()], row_to_occurrence)
        .optional()?;
    Ok(occ)
}

fn row_to_occurrence(row: &Row<'_>) -> rusqlite::Result<Occurrence> {
    let id_str: String = row.get("id")?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;

    let peritos_json: String = row.get("peritos")?;
    let peritos: Vec<String> = serde_json::from_str(&peritos_json).unwrap_or_default();

    let status_str: String = row.get("status")?;
    let status = OccurrenceStatus::parse(&status_str).unwrap_or_default();

    let import_id = row
        .get::<_, Option<String>>("import_id")?
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());

    Ok(Occurrence {
        id,
        numero_bo: row.get("numero_bo")?,
        protocolo: row.get("protocolo")?,
        requisicao: row.get("requisicao")?,
        oficio: row.get("oficio")?,
        delegacia: row.get("delegacia")?,
        tipo_pericia: row.get("tipo_pericia")?,
        natureza: row.get("natureza")?,
        municipio: row.get("municipio")?,
        bairro: row.get("bairro")?,
        logradouro: row.get("logradouro")?,
        referencia: row.get("referencia")?,
        latitude: row.get("latitude")?,
        longitude: row.get("longitude")?,
        data_fato: parse_optional_dt(row.get::<_, Option<String>>("data_fato")?),
        data_acionamento: parse_optional_dt(row.get::<_, Option<String>>("data_acionamento")?),
        data_chegada: parse_optional_dt(row.get::<_, Option<String>>("data_chegada")?),
        data_encerramento: parse_optional_dt(row.get::<_, Option<String>>("data_encerramento")?),
        peritos,
        status,
        created_at: parse_dt(row.get::<_, String>("created_at")?)
            .ok_or_else(|| rusqlite::Error::InvalidQuery)?,
        updated_at: parse_dt(row.get::<_, String>("updated_at")?)
            .ok_or_else(|| rusqlite::Error::InvalidQuery)?,
        import_id,
        original_mobile_id: row.get("original_mobile_id")?,
        primary_accuracy_m: row.get("primary_accuracy_m")?,
        resultado: row.get("resultado")?,
        raw_case_json: row.get("raw_case_json")?,
        raw_metadata_json: row.get("raw_metadata_json")?,
        raw_location_json: row.get("raw_location_json")?,
    })
}

fn parse_dt(s: String) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))
}

fn parse_optional_dt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(parse_dt)
}

/// Append an entry to `audit_logs`. Best-effort: errors are logged but never
/// bubble up — audit must not block the user.
pub fn record_audit(
    conn: &Connection,
    occurrence_id: Option<&Uuid>,
    action: &str,
    module: Option<&str>,
    entity_type: Option<&str>,
    entity_id: Option<&Uuid>,
    details_json: Option<&str>,
) -> Result<()> {
    let res = conn.execute(
        "INSERT INTO audit_logs
         (occurrence_id, action, module, entity_type, entity_id, details_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            occurrence_id.map(|u| u.to_string()),
            action,
            module,
            entity_type,
            entity_id.map(|u| u.to_string()),
            details_json,
            Utc::now().to_rfc3339(),
        ],
    );
    if let Err(e) = res {
        tracing::warn!("failed to write audit log entry '{action}': {e}");
        return Err(SicroError::from(e));
    }
    Ok(())
}
