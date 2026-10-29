//! Tabelas `video_lens_profiles` e `video_realce_results` (migration 025).

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerfilLente {
    #[serde(default)]
    pub nome: String,
    pub k1: f32,
    pub k2: f32,
    pub cx: f32,
    pub cy: f32,
}

pub fn salvar_perfil(conn: &Connection, occ: &Uuid, media_hash: &str, p: &PerfilLente) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO video_lens_profiles
            (occurrence_id, media_hash, nome, k1, k2, cx, cy, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![occ.to_string(), media_hash, p.nome, p.k1, p.k2, p.cx, p.cy, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

pub fn obter_perfil(conn: &Connection, occ: &Uuid, media_hash: &str) -> Result<Option<PerfilLente>> {
    Ok(conn
        .query_row(
            "SELECT nome, k1, k2, cx, cy FROM video_lens_profiles WHERE occurrence_id = ?1 AND media_hash = ?2",
            params![occ.to_string(), media_hash],
            |r| Ok(PerfilLente { nome: r.get(0)?, k1: r.get(1)?, k2: r.get(2)?, cx: r.get(3)?, cy: r.get(4)? }),
        )
        .optional()?)
}

pub fn apagar_perfil(conn: &Connection, occ: &Uuid, media_hash: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM video_lens_profiles WHERE occurrence_id = ?1 AND media_hash = ?2",
        params![occ.to_string(), media_hash],
    )?;
    Ok(())
}

pub fn listar_perfis(conn: &Connection, occ: &Uuid) -> Result<Vec<(String, PerfilLente)>> {
    let mut st = conn.prepare(
        "SELECT media_hash, nome, k1, k2, cx, cy FROM video_lens_profiles
         WHERE occurrence_id = ?1 ORDER BY updated_at DESC",
    )?;
    let rows = st
        .query_map([occ.to_string()], |r| {
            Ok((r.get(0)?, PerfilLente { nome: r.get(1)?, k1: r.get(2)?, k2: r.get(3)?, cx: r.get(4)?, cy: r.get(5)? }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize)]
pub struct Resultado {
    pub id: String,
    pub media_hash: String,
    pub tipo: String,
    /// Relativo ao caso.
    pub caminho: String,
    pub sha256: String,
    pub tamanho: u64,
    pub largura: Option<u32>,
    pub altura: Option<u32>,
    pub parametros: serde_json::Value,
    pub aviso: String,
    pub created_at: String,
}

pub fn inserir_resultado(conn: &Connection, occ: &Uuid, r: &Resultado) -> Result<()> {
    conn.execute(
        "INSERT INTO video_realce_results
            (id, occurrence_id, media_hash, tipo, caminho, sha256, tamanho, largura, altura,
             parametros_json, aviso, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            r.id, occ.to_string(), r.media_hash, r.tipo, r.caminho, r.sha256, r.tamanho as i64,
            r.largura, r.altura, r.parametros.to_string(), r.aviso, r.created_at,
        ],
    )?;
    Ok(())
}

fn linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<Resultado> {
    let p: String = r.get(9)?;
    Ok(Resultado {
        id: r.get(0)?,
        media_hash: r.get(1)?,
        tipo: r.get(2)?,
        caminho: r.get(3)?,
        sha256: r.get(4)?,
        tamanho: r.get::<_, i64>(5)? as u64,
        largura: r.get(6)?,
        altura: r.get(7)?,
        aviso: r.get(8)?,
        parametros: serde_json::from_str(&p).unwrap_or(serde_json::Value::Null),
        created_at: r.get(10)?,
    })
}

const COLS: &str = "id, media_hash, tipo, caminho, sha256, tamanho, largura, altura, aviso, parametros_json, created_at";

pub fn listar_resultados(conn: &Connection, occ: &Uuid, media_hash: &str) -> Result<Vec<Resultado>> {
    let mut st = conn.prepare(&format!(
        "SELECT {COLS} FROM video_realce_results WHERE occurrence_id = ?1 AND media_hash = ?2 ORDER BY created_at DESC"
    ))?;
    let rows = st
        .query_map(params![occ.to_string(), media_hash], linha)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn obter_resultado(conn: &Connection, occ: &Uuid, id: &str) -> Result<Option<Resultado>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM video_realce_results WHERE occurrence_id = ?1 AND id = ?2"),
            params![occ.to_string(), id],
            linha,
        )
        .optional()?)
}

pub fn apagar_resultado(conn: &Connection, occ: &Uuid, id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM video_realce_results WHERE occurrence_id = ?1 AND id = ?2",
        params![occ.to_string(), id],
    )?;
    Ok(())
}
