//! Persist mobile JSONs into the Dossiê tables introduced in MVP 3.
//!
//! Each `persist_*` function is **idempotent within a single import**:
//! it deletes any prior rows for the (occurrence_id) and re-inserts from
//! the JSON payload. This makes the orchestrator and the rehydrator share
//! the same code path.
//!
//! The mobile contract v0.6 uses Portuguese keys; this module is the place
//! that knows the wire shape. The repositories never see raw JSON keys.

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde_json::Value;
use uuid::Uuid;

use crate::database::repositories::dossie_repo;
use crate::error::Result;
use crate::models::{
    ChecklistItem, Entity, FieldNote, Measurement, OccurrenceStats, TimelineEvent, Trace,
};

#[derive(Debug, Default, Clone)]
pub struct DossieLoadCounts {
    pub checklist: u32,
    pub entities: u32,
    pub traces: u32,
    pub measurements: u32,
    pub notes: u32,
    pub timeline: u32,
    pub stats_loaded: bool,
    pub warnings: Vec<String>,
}

/// Run all persisters in one call. Each input is optional — None means the
/// JSON wasn't in the package and we skip without warning.
#[allow(clippy::too_many_arguments)]
pub fn persist_all(
    conn: &Connection,
    occurrence_id: Uuid,
    import_id: Uuid,
    checklist: Option<&Value>,
    veiculos: Option<&Value>,
    vitimas: Option<&Value>,
    vestigios: Option<&Value>,
    medicoes: Option<&Value>,
    observacoes: Option<&Value>,
    timeline: Option<&Value>,
    estatisticas: Option<&Value>,
) -> Result<DossieLoadCounts> {
    let mut counts = DossieLoadCounts::default();

    // Wipe any prior dossier state for this occurrence so re-importing /
    // rehydrating is naturally idempotent.
    dossie_repo::delete_checklist_for_occurrence(conn, &occurrence_id)?;
    dossie_repo::delete_entities_for_occurrence(conn, &occurrence_id)?;
    dossie_repo::delete_traces_for_occurrence(conn, &occurrence_id)?;
    dossie_repo::delete_measurements_for_occurrence(conn, &occurrence_id)?;
    dossie_repo::delete_field_notes_for_occurrence(conn, &occurrence_id)?;
    dossie_repo::delete_timeline_for_occurrence(conn, &occurrence_id)?;
    dossie_repo::delete_stats_for_occurrence(conn, &occurrence_id)?;

    counts.checklist = persist_checklist(conn, occurrence_id, import_id, checklist, &mut counts.warnings)?;
    let (v_count, w_count) = persist_entities(
        conn,
        occurrence_id,
        import_id,
        veiculos,
        vitimas,
        &mut counts.warnings,
    )?;
    counts.entities = v_count + w_count;
    counts.traces = persist_traces(conn, occurrence_id, import_id, vestigios, &mut counts.warnings)?;
    counts.measurements =
        persist_measurements(conn, occurrence_id, import_id, medicoes, &mut counts.warnings)?;
    counts.notes = persist_notes(conn, occurrence_id, import_id, observacoes, &mut counts.warnings)?;
    counts.timeline = persist_timeline(conn, occurrence_id, import_id, timeline, &mut counts.warnings)?;
    counts.stats_loaded =
        persist_stats(conn, occurrence_id, import_id, estatisticas, &mut counts.warnings)?;

    Ok(counts)
}

// ---------------------------------------------------------------------------
// checklist

fn persist_checklist(
    conn: &Connection,
    occurrence_id: Uuid,
    import_id: Uuid,
    payload: Option<&Value>,
    _warnings: &mut Vec<String>,
) -> Result<u32> {
    let arr = match payload.and_then(Value::as_array) {
        Some(a) => a,
        None => return Ok(0),
    };
    let mut count = 0u32;
    let now = Utc::now();
    for (idx, raw) in arr.iter().enumerate() {
        let original_id = string_field(raw, "id");
        let category = string_field(raw, "categoria");
        let question = string_field(raw, "pergunta").unwrap_or_default();
        if question.trim().is_empty() {
            continue;
        }
        let required = bool_field(raw, "obrigatorio").unwrap_or(false);
        let answer = string_field(raw, "resposta").unwrap_or_else(|| "nao_verificado".to_string());
        let note = string_field(raw, "observacao");
        let default_note = string_field(raw, "observacao_padrao");
        let origin = string_field(raw, "origem").unwrap_or_else(|| "base".to_string());

        dossie_repo::insert_checklist_item(
            conn,
            &ChecklistItem {
                id: Uuid::new_v4(),
                occurrence_id,
                import_id,
                original_id,
                category,
                question,
                required,
                answer,
                note,
                default_note,
                origin,
                sort_order: idx as i32,
                raw_json: serde_json::to_string(raw).unwrap_or_else(|_| "{}".into()),
                created_at: now,
            },
        )?;
        count += 1;
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// entities (veículos + vítimas)

fn persist_entities(
    conn: &Connection,
    occurrence_id: Uuid,
    import_id: Uuid,
    veiculos: Option<&Value>,
    vitimas: Option<&Value>,
    _warnings: &mut Vec<String>,
) -> Result<(u32, u32)> {
    let mut vehicles_count = 0u32;
    let mut victims_count = 0u32;
    let now = Utc::now();

    if let Some(arr) = veiculos.and_then(Value::as_array) {
        for (idx, raw) in arr.iter().enumerate() {
            let identifier = string_field(raw, "identifier")
                .or_else(|| string_field(raw, "identificador"));
            let placa = string_field(raw, "placa");
            let modelo = string_field(raw, "modelo");
            let cor = string_field(raw, "cor");
            let label_parts = [
                identifier.clone(),
                placa.clone(),
                modelo.clone(),
                cor.clone(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            let label = if label_parts.is_empty() {
                None
            } else {
                Some(label_parts.join(" — "))
            };
            let summary = string_field(raw, "observacao")
                .or_else(|| string_field(raw, "note"))
                .filter(|s| !s.trim().is_empty());

            dossie_repo::insert_entity(
                conn,
                &Entity {
                    id: Uuid::new_v4(),
                    occurrence_id,
                    import_id,
                    original_id: string_field(raw, "id"),
                    r#type: "vehicle".to_string(),
                    identifier,
                    label,
                    summary,
                    photo_ids_json: extract_photo_ids(raw, &["fotos", "photoIds"]),
                    raw_json: serde_json::to_string(raw).unwrap_or_else(|_| "{}".into()),
                    sort_order: idx as i32,
                    created_at: now,
                },
            )?;
            vehicles_count += 1;
        }
    }

    if let Some(arr) = vitimas.and_then(Value::as_array) {
        for (idx, raw) in arr.iter().enumerate() {
            let identifier = string_field(raw, "identifier")
                .or_else(|| string_field(raw, "identificador"));
            let name = string_field(raw, "nome").or_else(|| string_field(raw, "name"));
            let condition = string_field(raw, "condicao").or_else(|| string_field(raw, "condition"));
            let label_parts = [identifier.clone(), name.clone(), condition.clone()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            let label = if label_parts.is_empty() {
                None
            } else {
                Some(label_parts.join(" — "))
            };
            let summary = string_field(raw, "observacao").or_else(|| string_field(raw, "note"));

            dossie_repo::insert_entity(
                conn,
                &Entity {
                    id: Uuid::new_v4(),
                    occurrence_id,
                    import_id,
                    original_id: string_field(raw, "id"),
                    r#type: "victim".to_string(),
                    identifier,
                    label,
                    summary,
                    photo_ids_json: extract_photo_ids(raw, &["fotos", "photoIds"]),
                    raw_json: serde_json::to_string(raw).unwrap_or_else(|_| "{}".into()),
                    sort_order: idx as i32,
                    created_at: now,
                },
            )?;
            victims_count += 1;
        }
    }

    Ok((vehicles_count, victims_count))
}

// ---------------------------------------------------------------------------
// traces

fn persist_traces(
    conn: &Connection,
    occurrence_id: Uuid,
    import_id: Uuid,
    payload: Option<&Value>,
    _warnings: &mut Vec<String>,
) -> Result<u32> {
    let arr = match payload.and_then(Value::as_array) {
        Some(a) => a,
        None => return Ok(0),
    };
    let mut count = 0u32;
    let now = Utc::now();
    for (idx, raw) in arr.iter().enumerate() {
        dossie_repo::insert_trace(
            conn,
            &Trace {
                id: Uuid::new_v4(),
                occurrence_id,
                import_id,
                original_id: string_field(raw, "id"),
                identifier: string_field(raw, "identifier")
                    .or_else(|| string_field(raw, "identificador")),
                r#type: string_field(raw, "tipo").or_else(|| string_field(raw, "type")),
                description: string_field(raw, "descricao")
                    .or_else(|| string_field(raw, "description")),
                location_description: string_field(raw, "localizacao_textual")
                    .or_else(|| string_field(raw, "location_description")),
                length: number_field(raw, "comprimento").or_else(|| number_field(raw, "length")),
                width: number_field(raw, "largura").or_else(|| number_field(raw, "width")),
                unit: string_field(raw, "unidade").or_else(|| string_field(raw, "unit")),
                direction: string_field(raw, "direcao").or_else(|| string_field(raw, "direction")),
                note: string_field(raw, "observacao").or_else(|| string_field(raw, "note")),
                photo_ids_json: extract_photo_ids(raw, &["fotos", "photo_ids", "photoIds"]),
                sketch_element_ids_json: extract_photo_ids(
                    raw,
                    &["croqui", "sketch_element_ids"],
                ),
                raw_json: serde_json::to_string(raw).unwrap_or_else(|_| "{}".into()),
                sort_order: idx as i32,
                created_at: now,
            },
        )?;
        count += 1;
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// measurements

fn persist_measurements(
    conn: &Connection,
    occurrence_id: Uuid,
    import_id: Uuid,
    payload: Option<&Value>,
    _warnings: &mut Vec<String>,
) -> Result<u32> {
    let arr = match payload.and_then(Value::as_array) {
        Some(a) => a,
        None => return Ok(0),
    };
    let mut count = 0u32;
    let now = Utc::now();
    for (idx, raw) in arr.iter().enumerate() {
        dossie_repo::insert_measurement(
            conn,
            &Measurement {
                id: Uuid::new_v4(),
                occurrence_id,
                import_id,
                original_id: string_field(raw, "id"),
                label: string_field(raw, "rotulo").or_else(|| string_field(raw, "label")),
                point_a: string_field(raw, "ponto_a").or_else(|| string_field(raw, "point_a")),
                point_b: string_field(raw, "ponto_b").or_else(|| string_field(raw, "point_b")),
                value: number_field(raw, "valor").or_else(|| number_field(raw, "value")),
                unit: string_field(raw, "unidade").or_else(|| string_field(raw, "unit")),
                method: string_field(raw, "metodo").or_else(|| string_field(raw, "method")),
                note: string_field(raw, "observacao").or_else(|| string_field(raw, "note")),
                photo_ids_json: extract_photo_ids(raw, &["fotos", "photo_ids", "photoIds"]),
                sketch_element_ids_json: extract_photo_ids(
                    raw,
                    &["croqui", "sketch_element_ids"],
                ),
                raw_json: serde_json::to_string(raw).unwrap_or_else(|_| "{}".into()),
                sort_order: idx as i32,
                created_at: now,
            },
        )?;
        count += 1;
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// field_notes

fn persist_notes(
    conn: &Connection,
    occurrence_id: Uuid,
    import_id: Uuid,
    payload: Option<&Value>,
    _warnings: &mut Vec<String>,
) -> Result<u32> {
    let arr = match payload.and_then(Value::as_array) {
        Some(a) => a,
        None => return Ok(0),
    };
    let mut count = 0u32;
    let now = Utc::now();
    for (idx, raw) in arr.iter().enumerate() {
        dossie_repo::insert_field_note(
            conn,
            &FieldNote {
                id: Uuid::new_v4(),
                occurrence_id,
                import_id,
                original_id: string_field(raw, "id"),
                text: string_field(raw, "texto").or_else(|| string_field(raw, "text")),
                category: string_field(raw, "categoria").or_else(|| string_field(raw, "category")),
                priority: string_field(raw, "prioridade").or_else(|| string_field(raw, "priority")),
                note_created_at: parse_iso(string_field(raw, "criado_em").as_deref())
                    .or_else(|| parse_iso(string_field(raw, "created_at").as_deref())),
                note_updated_at: parse_iso(string_field(raw, "editado_em").as_deref())
                    .or_else(|| parse_iso(string_field(raw, "updated_at").as_deref())),
                raw_json: serde_json::to_string(raw).unwrap_or_else(|_| "{}".into()),
                sort_order: idx as i32,
                created_at: now,
            },
        )?;
        count += 1;
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// timeline

fn persist_timeline(
    conn: &Connection,
    occurrence_id: Uuid,
    import_id: Uuid,
    payload: Option<&Value>,
    _warnings: &mut Vec<String>,
) -> Result<u32> {
    let arr = match payload.and_then(Value::as_array) {
        Some(a) => a,
        None => return Ok(0),
    };
    let mut count = 0u32;
    let now = Utc::now();
    for (idx, raw) in arr.iter().enumerate() {
        dossie_repo::insert_timeline_event(
            conn,
            &TimelineEvent {
                id: Uuid::new_v4(),
                occurrence_id,
                import_id,
                original_id: string_field(raw, "id"),
                r#type: string_field(raw, "tipo").or_else(|| string_field(raw, "type")),
                title: string_field(raw, "titulo").or_else(|| string_field(raw, "title")),
                description: string_field(raw, "descricao")
                    .or_else(|| string_field(raw, "description")),
                occurred_at: parse_iso(string_field(raw, "ocorrido_em").as_deref())
                    .or_else(|| parse_iso(string_field(raw, "occurred_at").as_deref())),
                raw_json: serde_json::to_string(raw).unwrap_or_else(|_| "{}".into()),
                sort_order: idx as i32,
                created_at: now,
            },
        )?;
        count += 1;
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// stats

fn persist_stats(
    conn: &Connection,
    occurrence_id: Uuid,
    import_id: Uuid,
    payload: Option<&Value>,
    _warnings: &mut Vec<String>,
) -> Result<bool> {
    let raw = match payload {
        Some(v) if v.is_object() => v,
        _ => return Ok(false),
    };
    let stats = OccurrenceStats {
        id: Uuid::new_v4(),
        occurrence_id,
        import_id,
        duration_seconds: i64_field(raw, "duracao_segundos")
            .or_else(|| i64_field(raw, "duration_seconds")),
        photos_count: i64_field(raw, "total_fotos").or_else(|| i64_field(raw, "photos_count")),
        victims_count: i64_field(raw, "total_vitimas")
            .or_else(|| i64_field(raw, "victims_count")),
        vehicles_count: i64_field(raw, "total_veiculos")
            .or_else(|| i64_field(raw, "vehicles_count")),
        traces_count: i64_field(raw, "total_vestigios")
            .or_else(|| i64_field(raw, "traces_count")),
        measurements_count: i64_field(raw, "total_medicoes")
            .or_else(|| i64_field(raw, "measurements_count")),
        notes_count: i64_field(raw, "total_observacoes")
            .or_else(|| i64_field(raw, "notes_count")),
        checklist_items_count: i64_field(raw, "total_checklist")
            .or_else(|| i64_field(raw, "checklist_items_count")),
        answered_checklist_items_count: i64_field(raw, "checklist_respondidos")
            .or_else(|| i64_field(raw, "answered_checklist_items_count")),
        not_applicable_items_count: i64_field(raw, "checklist_nao_aplicavel")
            .or_else(|| i64_field(raw, "not_applicable_items_count")),
        best_gps_accuracy_m: number_field(raw, "melhor_precisao_gps_m")
            .or_else(|| number_field(raw, "best_gps_accuracy_m")),
        gps_readings_count: i64_field(raw, "leituras_gps")
            .or_else(|| i64_field(raw, "gps_readings_count")),
        raw_json: serde_json::to_string(raw).unwrap_or_else(|_| "{}".into()),
        created_at: Utc::now(),
    };
    dossie_repo::upsert_stats(conn, &stats)?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// helpers

fn string_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn bool_field(v: &Value, key: &str) -> Option<bool> {
    v.get(key).and_then(Value::as_bool)
}

fn number_field(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}

fn i64_field(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(Value::as_i64)
}

fn parse_iso(s: Option<&str>) -> Option<DateTime<Utc>> {
    let s = s?;
    DateTime::parse_from_rfc3339(s)
        .ok()
        .or_else(|| DateTime::parse_from_str(&format!("{s}Z"), "%Y-%m-%dT%H:%M:%S%.f%#z").ok())
        .map(|d| d.with_timezone(&Utc))
}

/// Extract an array of strings (photo IDs, sketch element IDs) from any of
/// the candidate keys, in priority order. Returns "[]" if nothing matches.
fn extract_photo_ids(v: &Value, keys: &[&str]) -> String {
    for k in keys {
        if let Some(arr) = v.get(*k).and_then(Value::as_array) {
            let ids: Vec<String> = arr
                .iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect();
            return serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into());
        }
    }
    "[]".to_string()
}
