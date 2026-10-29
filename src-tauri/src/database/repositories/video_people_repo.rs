//! Tabelas `video_people_runs` e `video_people_tracks` (migration 023).

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use uuid::Uuid;

use crate::error::Result;
use crate::video::pessoas::Pessoa;

#[derive(Debug, Clone, Serialize)]
pub struct Run {
    pub media_hash: String,
    pub model: String,
    pub runtime: String,
    pub params_json: String,
    pub width: u32,
    pub height: u32,
    pub frame_dur_s: f64,
    pub step: u32,
    pub frames_total: u32,
    pub frames_analyzed: u32,
    pub elapsed_s: f64,
    pub created_at: String,
}

pub fn replace_run(conn: &Connection, occ: &Uuid, r: &Run) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO video_people_runs
            (occurrence_id, media_hash, model, runtime, params_json, width, height, frame_dur_s,
             step, frames_total, frames_analyzed, elapsed_s, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            occ.to_string(), r.media_hash, r.model, r.runtime, r.params_json, r.width, r.height,
            r.frame_dur_s, r.step, r.frames_total, r.frames_analyzed, r.elapsed_s, r.created_at,
        ],
    )?;
    Ok(())
}

pub fn get_run(conn: &Connection, occ: &Uuid, media_hash: &str) -> Result<Option<Run>> {
    Ok(conn
        .query_row(
            "SELECT media_hash, model, runtime, params_json, width, height, frame_dur_s, step,
                    frames_total, frames_analyzed, elapsed_s, created_at
             FROM video_people_runs WHERE occurrence_id = ?1 AND media_hash = ?2",
            params![occ.to_string(), media_hash],
            |r| {
                Ok(Run {
                    media_hash: r.get(0)?,
                    model: r.get(1)?,
                    runtime: r.get(2)?,
                    params_json: r.get(3)?,
                    width: r.get(4)?,
                    height: r.get(5)?,
                    frame_dur_s: r.get(6)?,
                    step: r.get(7)?,
                    frames_total: r.get(8)?,
                    frames_analyzed: r.get(9)?,
                    elapsed_s: r.get(10)?,
                    created_at: r.get(11)?,
                })
            },
        )
        .optional()?)
}

/// Apaga as trilhas do detector; as manchas marcadas à mão ficam.
pub fn delete_auto(conn: &Connection, occ: &Uuid, media_hash: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM video_people_tracks WHERE occurrence_id = ?1 AND media_hash = ?2 AND origem = 'auto'",
        params![occ.to_string(), media_hash],
    )?;
    Ok(())
}

pub fn insert(conn: &Connection, occ: &Uuid, p: &Pessoa) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO video_people_tracks
            (id, occurrence_id, media_hash, numero, nome, cor, estado, amostras_json, ajustes_json,
             created_at, updated_at, classe, origem)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10,?11,?12)",
        params![
            p.id, occ.to_string(), p.media_hash, p.numero, p.nome, p.cor, p.estado,
            serde_json::to_string(&p.amostras)?, serde_json::to_string(&p.ajustes)?, now,
            p.classe, p.origem,
        ],
    )?;
    Ok(())
}

pub fn update(conn: &Connection, p: &Pessoa) -> Result<()> {
    conn.execute(
        "UPDATE video_people_tracks
            SET numero = ?2, nome = ?3, cor = ?4, estado = ?5, amostras_json = ?6, ajustes_json = ?7,
                updated_at = ?8
          WHERE id = ?1",
        params![
            p.id, p.numero, p.nome, p.cor, p.estado,
            serde_json::to_string(&p.amostras)?, serde_json::to_string(&p.ajustes)?,
            Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM video_people_tracks WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn list(conn: &Connection, occ: &Uuid, media_hash: &str) -> Result<Vec<Pessoa>> {
    let mut stmt = conn.prepare(
        "SELECT id, media_hash, numero, nome, cor, estado, amostras_json, ajustes_json, classe, origem
         FROM video_people_tracks WHERE occurrence_id = ?1 AND media_hash = ?2 ORDER BY numero",
    )?;
    let rows = stmt.query_map(params![occ.to_string(), media_hash], |r| {
        let amostras: String = r.get(6)?;
        let ajustes: String = r.get(7)?;
        Ok(Pessoa {
            id: r.get(0)?,
            media_hash: r.get(1)?,
            numero: r.get(2)?,
            nome: r.get(3)?,
            cor: r.get(4)?,
            estado: r.get(5)?,
            classe: r.get(8)?,
            origem: r.get(9)?,
            amostras: serde_json::from_str(&amostras).unwrap_or_default(),
            ajustes: serde_json::from_str(&ajustes).unwrap_or_default(),
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations::run_migrations;
    use crate::video::pessoas::Amostra;

    fn pessoa(id: &str, numero: i64, classe: i32, origem: &str) -> Pessoa {
        Pessoa {
            id: id.into(),
            media_hash: "h".into(),
            numero,
            nome: String::new(),
            cor: "#e6194b".into(),
            estado: "auto".into(),
            classe,
            origem: origem.into(),
            amostras: vec![Amostra { t: 0.0, x: 1.0, y: 2.0, w: 3.0, h: 4.0 }],
            ajustes: vec![],
        }
    }

    #[test]
    fn classe_e_origem_gravam_e_o_rastreio_novo_poupa_as_manchas() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        let occ = Uuid::new_v4();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        insert(&conn, &occ, &pessoa("a", 1, 0, "auto")).unwrap();
        insert(&conn, &occ, &pessoa("b", 2, 2, "auto")).unwrap();
        insert(&conn, &occ, &pessoa("m", 3, -1, "manual")).unwrap();
        let l = list(&conn, &occ, "h").unwrap();
        assert_eq!(l.iter().map(|p| (p.classe, p.origem.as_str())).collect::<Vec<_>>(), [(0, "auto"), (2, "auto"), (-1, "manual")]);
        delete_auto(&conn, &occ, "h").unwrap();
        let l = list(&conn, &occ, "h").unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].id, "m");
    }

    /// Trilhas da 5.2 (sem as colunas novas) viram "pessoa detectada automaticamente".
    #[test]
    fn trilhas_antigas_viram_pessoa_automatica() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE occurrences (id TEXT PRIMARY KEY); INSERT INTO occurrences VALUES ('o');").unwrap();
        conn.execute_batch(include_str!("../../../migrations/023_video_pessoas.sql")).unwrap();
        conn.execute(
            "INSERT INTO video_people_tracks (id, occurrence_id, media_hash, numero, cor, amostras_json, created_at, updated_at)
             VALUES ('x','o','h',1,'#fff','[]','t','t')",
            [],
        )
        .unwrap();
        conn.execute_batch(include_str!("../../../migrations/024_video_rastreio_classes.sql")).unwrap();
        let (c, o): (i32, String) = conn
            .query_row("SELECT classe, origem FROM video_people_tracks WHERE id='x'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((c, o.as_str()), (0, "auto"));
    }
}
