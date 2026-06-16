//! Read/write helpers for the `croquis` table (Spike E).
//!
//! Pattern mirrors `laudo_repo`: row in SQLite, `.sicrocroqui` file on disk.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::models::{Croqui, CroquiStatus};

const COLUMNS: &str = "
    id, occurrence_id, title, relative_path, status, schema_version,
    last_export_relative_path, kind, created_at, updated_at
";

pub fn insert(conn: &Connection, c: &Croqui) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO croquis ({COLUMNS}) VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
        ),
        params![
            c.id.to_string(),
            c.occurrence_id.to_string(),
            c.title,
            c.relative_path,
            c.status.as_str(),
            c.schema_version,
            c.last_export_relative_path,
            c.kind,
            c.created_at.to_rfc3339(),
            c.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_by_occurrence(conn: &Connection, occurrence_id: &Uuid) -> Result<Vec<Croqui>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM croquis
         WHERE occurrence_id = ?1
         ORDER BY updated_at DESC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_croqui)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn find_by_id(conn: &Connection, id: &Uuid) -> Result<Option<Croqui>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM croquis WHERE id = ?1"
    ))?;
    let row = stmt.query_row([id.to_string()], row_to_croqui).optional()?;
    Ok(row)
}

/// Update the `updated_at` (used after save / after export). Optionally bumps
/// `last_export_relative_path` and `status` in the same call.
pub fn touch(
    conn: &Connection,
    id: &Uuid,
    when: DateTime<Utc>,
    last_export: Option<&str>,
    status: Option<CroquiStatus>,
) -> Result<()> {
    if let Some(path) = last_export {
        if let Some(s) = status {
            conn.execute(
                "UPDATE croquis SET updated_at = ?2, last_export_relative_path = ?3, status = ?4
                 WHERE id = ?1",
                params![id.to_string(), when.to_rfc3339(), path, s.as_str()],
            )?;
        } else {
            conn.execute(
                "UPDATE croquis SET updated_at = ?2, last_export_relative_path = ?3
                 WHERE id = ?1",
                params![id.to_string(), when.to_rfc3339(), path],
            )?;
        }
    } else if let Some(s) = status {
        conn.execute(
            "UPDATE croquis SET updated_at = ?2, status = ?3 WHERE id = ?1",
            params![id.to_string(), when.to_rfc3339(), s.as_str()],
        )?;
    } else {
        conn.execute(
            "UPDATE croquis SET updated_at = ?2 WHERE id = ?1",
            params![id.to_string(), when.to_rfc3339()],
        )?;
    }
    Ok(())
}

/// Remove a linha do croqui da tabela. NÃO mexe no `.sicrocroqui` em
/// disco — o command em `commands/croqui_commands.rs` cuida disso.
pub fn delete(conn: &Connection, id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM croquis WHERE id = ?1",
        [id.to_string()],
    )?;
    Ok(())
}

fn row_to_croqui(row: &Row<'_>) -> rusqlite::Result<Croqui> {
    let id: String = row.get("id")?;
    let id = Uuid::parse_str(&id).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let occ: String = row.get("occurrence_id")?;
    let occurrence_id = Uuid::parse_str(&occ).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let status_str: String = row.get("status")?;
    let status = CroquiStatus::parse(&status_str).unwrap_or_default();

    Ok(Croqui {
        id,
        occurrence_id,
        title: row.get("title")?,
        relative_path: row.get("relative_path")?,
        status,
        schema_version: row.get("schema_version")?,
        last_export_relative_path: row.get("last_export_relative_path")?,
        kind: row
            .get::<_, Option<String>>("kind")?
            .unwrap_or_else(|| "viario".to_string()),
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
        updated_at: parse_dt(row.get::<_, String>("updated_at")?)?,
    })
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations::run_migrations;

    fn mk(kind: &str) -> Croqui {
        let now = Utc::now();
        let id = Uuid::new_v4();
        Croqui {
            id,
            occurrence_id: Uuid::new_v4(),
            title: "T".into(),
            relative_path: format!("croquis/x_{id}.json"),
            status: CroquiStatus::Draft,
            schema_version: "0.1".into(),
            last_export_relative_path: None,
            kind: kind.into(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn migration_017_kind_round_trips() {
        // FK off por padrão num Connection in-memory → não precisa de occurrence.
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        // Testamos só o round-trip de `kind`, não a integridade FK → desliga.
        conn.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();

        let corporal = mk("corporal");
        insert(&conn, &corporal).unwrap();
        let back = find_by_id(&conn, &corporal.id).unwrap().unwrap();
        assert_eq!(back.kind, "corporal");

        let viario = mk("viario");
        insert(&conn, &viario).unwrap();
        let listed = list_by_occurrence(&conn, &viario.occurrence_id).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].kind, "viario");
    }

    #[test]
    fn old_row_without_kind_defaults_to_viario() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        // Testamos só o round-trip de `kind`, não a integridade FK → desliga.
        conn.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        // Insert sem a coluna kind (simula croqui pré-017) → DEFAULT 'viario'.
        let id = Uuid::new_v4();
        let occ = Uuid::new_v4();
        conn.execute(
            "INSERT INTO croquis
             (id, occurrence_id, title, relative_path, status, schema_version, created_at, updated_at)
             VALUES (?1, ?2, 'old', 'croquis/o.sicrocroqui', 'draft', '0.1', ?3, ?3)",
            params![id.to_string(), occ.to_string(), Utc::now().to_rfc3339()],
        )
        .unwrap();
        let back = find_by_id(&conn, &id).unwrap().unwrap();
        assert_eq!(back.kind, "viario");
    }
}
