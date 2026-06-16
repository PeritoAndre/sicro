//! Repositories for the Dossiê Operacional (MVP 3).
//!
//! Kept as a single module because the seven tables introduced by
//! `005_dossie.sql` share the same shape (occurrence_id + import_id + a
//! couple of structured columns + raw_json). Splitting into seven files
//! would be cargo-culting — one module is easier to audit.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::models::{
    ChecklistItem, ChecklistSummary, Entity, FieldNote, Measurement, OccurrenceStats,
    TimelineEvent, Trace,
};

// ---------------------------------------------------------------------------
// checklist_items

const CHECKLIST_COLS: &str = "
    id, occurrence_id, import_id, original_id, category, question, required,
    answer, note, default_note, origin, sort_order, raw_json, created_at
";

pub fn insert_checklist_item(conn: &Connection, item: &ChecklistItem) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO checklist_items ({CHECKLIST_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)"
        ),
        params![
            item.id.to_string(),
            item.occurrence_id.to_string(),
            item.import_id.to_string(),
            item.original_id,
            item.category,
            item.question,
            item.required as i64,
            item.answer,
            item.note,
            item.default_note,
            item.origin,
            item.sort_order,
            item.raw_json,
            item.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_checklist(conn: &Connection, occurrence_id: &Uuid) -> Result<Vec<ChecklistItem>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CHECKLIST_COLS} FROM checklist_items
         WHERE occurrence_id = ?1
         ORDER BY sort_order ASC, created_at ASC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_checklist_item)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn delete_checklist_for_occurrence(conn: &Connection, occurrence_id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM checklist_items WHERE occurrence_id = ?1",
        [occurrence_id.to_string()],
    )?;
    Ok(())
}

fn row_to_checklist_item(row: &Row<'_>) -> rusqlite::Result<ChecklistItem> {
    Ok(ChecklistItem {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        import_id: parse_uuid(row, "import_id")?,
        original_id: row.get("original_id")?,
        category: row.get("category")?,
        question: row.get("question")?,
        required: row.get::<_, i64>("required")? != 0,
        answer: row.get("answer")?,
        note: row.get("note")?,
        default_note: row.get("default_note")?,
        origin: row.get("origin")?,
        sort_order: row.get("sort_order")?,
        raw_json: row.get("raw_json")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

/// Derived counters for the checklist UI.
pub fn summarise_checklist(items: &[ChecklistItem]) -> ChecklistSummary {
    let mut s = ChecklistSummary {
        total: items.len() as u32,
        ..Default::default()
    };
    for item in items {
        if item.required {
            s.required_total += 1;
        }
        match item.answer.as_str() {
            "nao_verificado" => s.not_verified += 1,
            "nao_se_aplica" => s.not_applicable += 1,
            "sim" | "nao" => s.answered += 1,
            _ => s.not_verified += 1,
        }
        if item.required && item.answer == "nao_verificado" {
            s.required_pending += 1;
        }
    }
    s
}

// ---------------------------------------------------------------------------
// entities

const ENTITY_COLS: &str = "
    id, occurrence_id, import_id, original_id, type, identifier, label, summary,
    photo_ids_json, raw_json, sort_order, created_at
";

pub fn insert_entity(conn: &Connection, e: &Entity) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO entities ({ENTITY_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"
        ),
        params![
            e.id.to_string(),
            e.occurrence_id.to_string(),
            e.import_id.to_string(),
            e.original_id,
            e.r#type,
            e.identifier,
            e.label,
            e.summary,
            e.photo_ids_json,
            e.raw_json,
            e.sort_order,
            e.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_entities(conn: &Connection, occurrence_id: &Uuid) -> Result<Vec<Entity>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ENTITY_COLS} FROM entities
         WHERE occurrence_id = ?1
         ORDER BY type ASC, sort_order ASC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_entity)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn delete_entities_for_occurrence(conn: &Connection, occurrence_id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM entities WHERE occurrence_id = ?1",
        [occurrence_id.to_string()],
    )?;
    Ok(())
}

fn row_to_entity(row: &Row<'_>) -> rusqlite::Result<Entity> {
    Ok(Entity {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        import_id: parse_uuid(row, "import_id")?,
        original_id: row.get("original_id")?,
        r#type: row.get("type")?,
        identifier: row.get("identifier")?,
        label: row.get("label")?,
        summary: row.get("summary")?,
        photo_ids_json: row.get("photo_ids_json")?,
        raw_json: row.get("raw_json")?,
        sort_order: row.get("sort_order")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

// ---------------------------------------------------------------------------
// traces

const TRACE_COLS: &str = "
    id, occurrence_id, import_id, original_id, identifier, type, description,
    location_description, length, width, unit, direction, note,
    photo_ids_json, sketch_element_ids_json, raw_json, sort_order, created_at
";

pub fn insert_trace(conn: &Connection, t: &Trace) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO traces ({TRACE_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)"
        ),
        params![
            t.id.to_string(),
            t.occurrence_id.to_string(),
            t.import_id.to_string(),
            t.original_id,
            t.identifier,
            t.r#type,
            t.description,
            t.location_description,
            t.length,
            t.width,
            t.unit,
            t.direction,
            t.note,
            t.photo_ids_json,
            t.sketch_element_ids_json,
            t.raw_json,
            t.sort_order,
            t.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_traces(conn: &Connection, occurrence_id: &Uuid) -> Result<Vec<Trace>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {TRACE_COLS} FROM traces
         WHERE occurrence_id = ?1
         ORDER BY sort_order ASC, created_at ASC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_trace)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn delete_traces_for_occurrence(conn: &Connection, occurrence_id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM traces WHERE occurrence_id = ?1",
        [occurrence_id.to_string()],
    )?;
    Ok(())
}

fn row_to_trace(row: &Row<'_>) -> rusqlite::Result<Trace> {
    Ok(Trace {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        import_id: parse_uuid(row, "import_id")?,
        original_id: row.get("original_id")?,
        identifier: row.get("identifier")?,
        r#type: row.get("type")?,
        description: row.get("description")?,
        location_description: row.get("location_description")?,
        length: row.get("length")?,
        width: row.get("width")?,
        unit: row.get("unit")?,
        direction: row.get("direction")?,
        note: row.get("note")?,
        photo_ids_json: row.get("photo_ids_json")?,
        sketch_element_ids_json: row.get("sketch_element_ids_json")?,
        raw_json: row.get("raw_json")?,
        sort_order: row.get("sort_order")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

// ---------------------------------------------------------------------------
// measurements

const MEAS_COLS: &str = "
    id, occurrence_id, import_id, original_id, label, point_a, point_b,
    value, unit, method, note, photo_ids_json, sketch_element_ids_json,
    raw_json, sort_order, created_at
";

pub fn insert_measurement(conn: &Connection, m: &Measurement) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO measurements ({MEAS_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)"
        ),
        params![
            m.id.to_string(),
            m.occurrence_id.to_string(),
            m.import_id.to_string(),
            m.original_id,
            m.label,
            m.point_a,
            m.point_b,
            m.value,
            m.unit,
            m.method,
            m.note,
            m.photo_ids_json,
            m.sketch_element_ids_json,
            m.raw_json,
            m.sort_order,
            m.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_measurements(conn: &Connection, occurrence_id: &Uuid) -> Result<Vec<Measurement>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEAS_COLS} FROM measurements
         WHERE occurrence_id = ?1
         ORDER BY sort_order ASC, created_at ASC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_measurement)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn delete_measurements_for_occurrence(conn: &Connection, occurrence_id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM measurements WHERE occurrence_id = ?1",
        [occurrence_id.to_string()],
    )?;
    Ok(())
}

fn row_to_measurement(row: &Row<'_>) -> rusqlite::Result<Measurement> {
    Ok(Measurement {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        import_id: parse_uuid(row, "import_id")?,
        original_id: row.get("original_id")?,
        label: row.get("label")?,
        point_a: row.get("point_a")?,
        point_b: row.get("point_b")?,
        value: row.get("value")?,
        unit: row.get("unit")?,
        method: row.get("method")?,
        note: row.get("note")?,
        photo_ids_json: row.get("photo_ids_json")?,
        sketch_element_ids_json: row.get("sketch_element_ids_json")?,
        raw_json: row.get("raw_json")?,
        sort_order: row.get("sort_order")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

// ---------------------------------------------------------------------------
// field_notes

const NOTE_COLS: &str = "
    id, occurrence_id, import_id, original_id, text, category, priority,
    note_created_at, note_updated_at, raw_json, sort_order, created_at
";

pub fn insert_field_note(conn: &Connection, n: &FieldNote) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO field_notes ({NOTE_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"
        ),
        params![
            n.id.to_string(),
            n.occurrence_id.to_string(),
            n.import_id.to_string(),
            n.original_id,
            n.text,
            n.category,
            n.priority,
            n.note_created_at.map(|d| d.to_rfc3339()),
            n.note_updated_at.map(|d| d.to_rfc3339()),
            n.raw_json,
            n.sort_order,
            n.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_field_notes(conn: &Connection, occurrence_id: &Uuid) -> Result<Vec<FieldNote>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {NOTE_COLS} FROM field_notes
         WHERE occurrence_id = ?1
         ORDER BY COALESCE(note_created_at, created_at) DESC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_field_note)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn delete_field_notes_for_occurrence(conn: &Connection, occurrence_id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM field_notes WHERE occurrence_id = ?1",
        [occurrence_id.to_string()],
    )?;
    Ok(())
}

fn row_to_field_note(row: &Row<'_>) -> rusqlite::Result<FieldNote> {
    Ok(FieldNote {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        import_id: parse_uuid(row, "import_id")?,
        original_id: row.get("original_id")?,
        text: row.get("text")?,
        category: row.get("category")?,
        priority: row.get("priority")?,
        note_created_at: parse_optional_dt(row.get::<_, Option<String>>("note_created_at")?),
        note_updated_at: parse_optional_dt(row.get::<_, Option<String>>("note_updated_at")?),
        raw_json: row.get("raw_json")?,
        sort_order: row.get("sort_order")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

// ---------------------------------------------------------------------------
// timeline_events

const TIMELINE_COLS: &str = "
    id, occurrence_id, import_id, original_id, type, title, description,
    occurred_at, raw_json, sort_order, created_at
";

pub fn insert_timeline_event(conn: &Connection, e: &TimelineEvent) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO timeline_events ({TIMELINE_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)"
        ),
        params![
            e.id.to_string(),
            e.occurrence_id.to_string(),
            e.import_id.to_string(),
            e.original_id,
            e.r#type,
            e.title,
            e.description,
            e.occurred_at.map(|d| d.to_rfc3339()),
            e.raw_json,
            e.sort_order,
            e.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_timeline(conn: &Connection, occurrence_id: &Uuid) -> Result<Vec<TimelineEvent>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {TIMELINE_COLS} FROM timeline_events
         WHERE occurrence_id = ?1
         ORDER BY occurred_at ASC, sort_order ASC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_timeline_event)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn delete_timeline_for_occurrence(conn: &Connection, occurrence_id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM timeline_events WHERE occurrence_id = ?1",
        [occurrence_id.to_string()],
    )?;
    Ok(())
}

fn row_to_timeline_event(row: &Row<'_>) -> rusqlite::Result<TimelineEvent> {
    Ok(TimelineEvent {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        import_id: parse_uuid(row, "import_id")?,
        original_id: row.get("original_id")?,
        r#type: row.get("type")?,
        title: row.get("title")?,
        description: row.get("description")?,
        occurred_at: parse_optional_dt(row.get::<_, Option<String>>("occurred_at")?),
        raw_json: row.get("raw_json")?,
        sort_order: row.get("sort_order")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

// ---------------------------------------------------------------------------
// occurrence_stats

const STATS_COLS: &str = "
    id, occurrence_id, import_id, duration_seconds, photos_count, victims_count,
    vehicles_count, traces_count, measurements_count, notes_count,
    checklist_items_count, answered_checklist_items_count,
    not_applicable_items_count, best_gps_accuracy_m, gps_readings_count,
    raw_json, created_at
";

pub fn upsert_stats(conn: &Connection, s: &OccurrenceStats) -> Result<()> {
    conn.execute(
        "DELETE FROM occurrence_stats WHERE occurrence_id = ?1",
        [s.occurrence_id.to_string()],
    )?;
    conn.execute(
        &format!(
            "INSERT INTO occurrence_stats ({STATS_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)"
        ),
        params![
            s.id.to_string(),
            s.occurrence_id.to_string(),
            s.import_id.to_string(),
            s.duration_seconds,
            s.photos_count,
            s.victims_count,
            s.vehicles_count,
            s.traces_count,
            s.measurements_count,
            s.notes_count,
            s.checklist_items_count,
            s.answered_checklist_items_count,
            s.not_applicable_items_count,
            s.best_gps_accuracy_m,
            s.gps_readings_count,
            s.raw_json,
            s.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn find_stats(conn: &Connection, occurrence_id: &Uuid) -> Result<Option<OccurrenceStats>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {STATS_COLS} FROM occurrence_stats
         WHERE occurrence_id = ?1
         LIMIT 1"
    ))?;
    let row = stmt
        .query_row([occurrence_id.to_string()], row_to_stats)
        .optional()?;
    Ok(row)
}

pub fn delete_stats_for_occurrence(conn: &Connection, occurrence_id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM occurrence_stats WHERE occurrence_id = ?1",
        [occurrence_id.to_string()],
    )?;
    Ok(())
}

fn row_to_stats(row: &Row<'_>) -> rusqlite::Result<OccurrenceStats> {
    Ok(OccurrenceStats {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        import_id: parse_uuid(row, "import_id")?,
        duration_seconds: row.get("duration_seconds")?,
        photos_count: row.get("photos_count")?,
        victims_count: row.get("victims_count")?,
        vehicles_count: row.get("vehicles_count")?,
        traces_count: row.get("traces_count")?,
        measurements_count: row.get("measurements_count")?,
        notes_count: row.get("notes_count")?,
        checklist_items_count: row.get("checklist_items_count")?,
        answered_checklist_items_count: row.get("answered_checklist_items_count")?,
        not_applicable_items_count: row.get("not_applicable_items_count")?,
        best_gps_accuracy_m: row.get("best_gps_accuracy_m")?,
        gps_readings_count: row.get("gps_readings_count")?,
        raw_json: row.get("raw_json")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

// ---------------------------------------------------------------------------
// helpers

fn parse_uuid(row: &Row<'_>, col: &str) -> rusqlite::Result<Uuid> {
    let s: String = row.get(col)?;
    Uuid::parse_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn parse_optional_dt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|s| {
        DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|d| d.with_timezone(&Utc))
    })
}
