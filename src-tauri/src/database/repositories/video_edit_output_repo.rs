//! Tabela `video_edit_outputs` (migration 027): exportações finais da Edição.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::models::{EstadoSaida, VideoEditOutput};

const COLS: &str = "id, occurrence_id, projeto_id, titulo, pasta, arquivo, sha256, composto_sha256, \
    decodificado_sha256, audio_sha256, tamanho, duracao_s, n_quadros, predefinicao, codificador, \
    tecnico_rel, retrato_rel, projeto_sha256, autor_json, refeita_de, estado, conferida_por, \
    conferida_em, criado_em, exige_conferencia";

pub fn inserir(conn: &Connection, s: &VideoEditOutput) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO video_edit_outputs ({COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)"
        ),
        params![
            s.id.to_string(),
            s.occurrence_id.to_string(),
            s.projeto_id,
            s.titulo,
            s.pasta,
            s.arquivo,
            s.sha256,
            s.composto_sha256,
            s.decodificado_sha256,
            s.audio_sha256,
            s.tamanho as i64,
            s.duracao_s,
            s.n_quadros,
            s.predefinicao,
            s.codificador,
            s.tecnico_rel,
            s.retrato_rel,
            s.projeto_sha256,
            serde_json::to_string(&s.autor)?,
            s.refeita_de.map(|u| u.to_string()),
            s.estado.as_str(),
            s.conferida_por,
            s.conferida_em.map(|d| d.to_rfc3339()),
            s.criado_em.to_rfc3339(),
            s.exige_conferencia,
        ],
    )?;
    Ok(())
}

pub fn listar_do_caso(conn: &Connection, occ: &Uuid) -> Result<Vec<VideoEditOutput>> {
    let mut st = conn.prepare(&format!(
        "SELECT {COLS} FROM video_edit_outputs WHERE occurrence_id = ?1 ORDER BY criado_em DESC, id"
    ))?;
    let rows = st
        .query_map([occ.to_string()], linha)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn listar_do_projeto(conn: &Connection, occ: &Uuid, projeto_id: &str) -> Result<Vec<VideoEditOutput>> {
    let mut st = conn.prepare(&format!(
        "SELECT {COLS} FROM video_edit_outputs WHERE occurrence_id = ?1 AND projeto_id = ?2 \
         ORDER BY criado_em DESC, id"
    ))?;
    let rows = st
        .query_map(params![occ.to_string(), projeto_id], linha)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn obter(conn: &Connection, occ: &Uuid, id: &Uuid) -> Result<Option<VideoEditOutput>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM video_edit_outputs WHERE occurrence_id = ?1 AND id = ?2"),
            params![occ.to_string(), id.to_string()],
            linha,
        )
        .optional()?)
}

/// `a_conferir` → `concluida`. Devolve `false` se não existe ou já estava concluída
/// (a primeira conferência fica).
pub fn marcar_conferida(
    conn: &Connection,
    occ: &Uuid,
    id: &Uuid,
    por: &str,
    em: DateTime<Utc>,
) -> Result<bool> {
    concluir(conn, occ, id, Some((por, em)), None)
}

/// `a_conferir` → `concluida`, conferida (quem, quando) ou não (`None`: "Concluir sem conferir").
/// `tecnico_rel` entra junto quando o relatório só sai aqui; `None` mantém o que havia.
pub fn concluir(
    conn: &Connection,
    occ: &Uuid,
    id: &Uuid,
    conferida: Option<(&str, DateTime<Utc>)>,
    tecnico_rel: Option<&str>,
) -> Result<bool> {
    let n = conn.execute(
        "UPDATE video_edit_outputs SET estado = 'concluida', conferida_por = ?3, conferida_em = ?4,
                tecnico_rel = COALESCE(?5, tecnico_rel)
         WHERE occurrence_id = ?1 AND id = ?2 AND estado = 'a_conferir'",
        params![
            occ.to_string(),
            id.to_string(),
            conferida.map(|c| c.0),
            conferida.map(|c| c.1.to_rfc3339()),
            tecnico_rel,
        ],
    )?;
    Ok(n > 0)
}

pub fn excluir(conn: &Connection, occ: &Uuid, id: &Uuid) -> Result<bool> {
    let n = conn.execute(
        "DELETE FROM video_edit_outputs WHERE occurrence_id = ?1 AND id = ?2",
        params![occ.to_string(), id.to_string()],
    )?;
    Ok(n > 0)
}

fn linha(r: &Row<'_>) -> rusqlite::Result<VideoEditOutput> {
    let estado: String = r.get("estado")?;
    Ok(VideoEditOutput {
        id: uuid(r.get("id")?)?,
        occurrence_id: uuid(r.get("occurrence_id")?)?,
        projeto_id: r.get("projeto_id")?,
        titulo: r.get("titulo")?,
        pasta: r.get("pasta")?,
        arquivo: r.get("arquivo")?,
        sha256: r.get("sha256")?,
        composto_sha256: r.get("composto_sha256")?,
        decodificado_sha256: r.get("decodificado_sha256")?,
        audio_sha256: r.get("audio_sha256")?,
        tamanho: r.get::<_, i64>("tamanho")? as u64,
        duracao_s: r.get("duracao_s")?,
        n_quadros: r.get("n_quadros")?,
        predefinicao: r.get("predefinicao")?,
        codificador: r.get("codificador")?,
        tecnico_rel: r.get("tecnico_rel")?,
        retrato_rel: r.get("retrato_rel")?,
        projeto_sha256: r.get("projeto_sha256")?,
        autor: serde_json::from_str(&r.get::<_, String>("autor_json")?)
            .map_err(|e| conv(Box::new(e)))?,
        refeita_de: r.get::<_, Option<String>>("refeita_de")?.map(uuid).transpose()?,
        estado: EstadoSaida::from_db(&estado).ok_or_else(|| conv(format!("estado {estado}").into()))?,
        conferida_por: r.get("conferida_por")?,
        conferida_em: r.get::<_, Option<String>>("conferida_em")?.map(data).transpose()?,
        criado_em: data(r.get("criado_em")?)?,
        exige_conferencia: r.get("exige_conferencia")?,
    })
}

fn conv(e: Box<dyn std::error::Error + Send + Sync>) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e)
}

fn uuid(s: String) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&s).map_err(|e| conv(Box::new(e)))
}

fn data(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| conv(Box::new(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations::run_migrations;

    fn banco() -> (Connection, Uuid) {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        run_migrations(&mut conn).unwrap();
        let occ = Uuid::new_v4();
        conn.execute(
            "INSERT INTO occurrences (id, created_at, updated_at) VALUES (?1, ?2, ?2)",
            params![occ.to_string(), "2026-11-14T12:00:00Z"],
        )
        .unwrap();
        (conn, occ)
    }

    fn dt(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn saida(occ: Uuid, projeto: &str, criado: &str) -> VideoEditOutput {
        let id = Uuid::new_v4();
        VideoEditOutput {
            id,
            occurrence_id: occ,
            projeto_id: projeto.into(),
            titulo: "Abordagem".into(),
            pasta: "Exportações/Vídeos/Edições/Abordagem 2026-11-14 12h00m00s".into(),
            arquivo: "Exportações/Vídeos/Edições/Abordagem 2026-11-14 12h00m00s/Abordagem.mp4".into(),
            sha256: "a".repeat(64),
            composto_sha256: Some("b".repeat(64)),
            decodificado_sha256: None,
            audio_sha256: Some("c".repeat(64)),
            tamanho: 6_000_000_000,
            duracao_s: 47.3,
            n_quadros: 1419,
            predefinicao: "juri".into(),
            codificador: "libx264".into(),
            tecnico_rel: Some("tecnico.json".into()),
            retrato_rel: format!("Sistema/videos/edicao/{projeto}/exportacoes/{id}"),
            projeto_sha256: "d".repeat(64),
            autor: serde_json::json!({ "nome": "André", "matricula": "1" }),
            refeita_de: None,
            estado: EstadoSaida::AConferir,
            conferida_por: None,
            conferida_em: None,
            criado_em: dt(criado),
            exige_conferencia: true,
        }
    }

    #[test]
    fn migracao_027_em_memoria() {
        let (conn, _) = banco();
        let aplicada: bool = conn
            .query_row("SELECT 1 FROM schema_migrations WHERE version = '027_edicao'", [], |_| Ok(true))
            .unwrap();
        assert!(aplicada);
        let mut st = conn.prepare("SELECT name, \"notnull\", dflt_value FROM pragma_table_info('video_media')").unwrap();
        let cols: Vec<(String, i64, Option<String>)> = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        for nome in ["recebido_nativo", "recebido_obs", "relogio_desvio_s", "gravador"] {
            let c = cols.iter().find(|c| c.0 == nome).unwrap_or_else(|| panic!("falta {nome}"));
            assert_eq!((c.1, c.2.clone()), (0, None), "{nome} nula por padrão");
        }
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_table_info('video_edit_outputs') WHERE name = 'media_hash'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn inserir_obter_ida_e_volta() {
        let (conn, occ) = banco();
        let mut s = saida(occ, "p1", "2026-11-14T12:00:00Z");
        s.refeita_de = Some(Uuid::new_v4());
        inserir(&conn, &s).unwrap();
        assert_eq!(obter(&conn, &occ, &s.id).unwrap(), Some(s.clone()));
        assert_eq!(obter(&conn, &Uuid::new_v4(), &s.id).unwrap(), None, "outro caso não vê");
        assert!(inserir(&conn, &s).is_err(), "id repetido");
    }

    #[test]
    fn listar_por_caso_e_por_projeto() {
        let (conn, occ) = banco();
        let a = saida(occ, "p1", "2026-11-14T12:00:00Z");
        let b = saida(occ, "p2", "2026-11-14T13:00:00Z");
        let c = saida(occ, "p1", "2026-11-14T14:00:00Z");
        for s in [&a, &b, &c] {
            inserir(&conn, s).unwrap();
        }
        let ids = |v: Vec<VideoEditOutput>| v.into_iter().map(|s| s.id).collect::<Vec<_>>();
        assert_eq!(ids(listar_do_caso(&conn, &occ).unwrap()), vec![c.id, b.id, a.id]);
        assert_eq!(ids(listar_do_projeto(&conn, &occ, "p1").unwrap()), vec![c.id, a.id]);
        assert!(listar_do_projeto(&conn, &occ, "p3").unwrap().is_empty());
        assert!(listar_do_caso(&conn, &Uuid::new_v4()).unwrap().is_empty());
    }

    #[test]
    fn marcar_conferida_uma_vez() {
        let (conn, occ) = banco();
        let s = saida(occ, "p1", "2026-11-14T12:00:00Z");
        inserir(&conn, &s).unwrap();
        let em = dt("2026-11-14T15:00:00Z");
        assert!(marcar_conferida(&conn, &occ, &s.id, "André", em).unwrap());
        assert!(!marcar_conferida(&conn, &occ, &s.id, "Outro", dt("2026-11-15T15:00:00Z")).unwrap());
        assert!(!marcar_conferida(&conn, &occ, &Uuid::new_v4(), "André", em).unwrap());
        let lida = obter(&conn, &occ, &s.id).unwrap().unwrap();
        assert_eq!(lida.estado, EstadoSaida::Concluida);
        assert_eq!(lida.conferida_por.as_deref(), Some("André"));
        assert_eq!(lida.conferida_em, Some(em));
    }

    #[test]
    fn concluir_sem_conferir_e_com_relatorio() {
        let (conn, occ) = banco();
        let mut s = saida(occ, "p1", "2026-11-14T12:00:00Z");
        s.tecnico_rel = None;
        inserir(&conn, &s).unwrap();
        assert!(concluir(&conn, &occ, &s.id, None, Some("tecnico.json")).unwrap());
        let lida = obter(&conn, &occ, &s.id).unwrap().unwrap();
        assert_eq!(lida.estado, EstadoSaida::Concluida);
        assert_eq!((lida.conferida_por.as_deref(), lida.conferida_em), (None, None));
        assert_eq!(lida.tecnico_rel.as_deref(), Some("tecnico.json"));
        // Já concluída: não muda mais, nem o relatório.
        let em = dt("2026-11-14T15:00:00Z");
        assert!(!concluir(&conn, &occ, &s.id, Some(("André", em)), Some("outro.json")).unwrap());
        assert_eq!(obter(&conn, &occ, &s.id).unwrap().unwrap(), lida);

        // Sem `tecnico_rel` novo, o que havia fica.
        let t = saida(occ, "p1", "2026-11-14T13:00:00Z");
        inserir(&conn, &t).unwrap();
        assert!(concluir(&conn, &occ, &t.id, Some(("André", em)), None).unwrap());
        let lida = obter(&conn, &occ, &t.id).unwrap().unwrap();
        assert_eq!(lida.tecnico_rel.as_deref(), Some("tecnico.json"));
        assert_eq!((lida.conferida_por.as_deref(), lida.conferida_em), (Some("André"), Some(em)));
    }

    #[test]
    fn estado_fora_da_lista_recusado() {
        let (conn, occ) = banco();
        let s = saida(occ, "p1", "2026-11-14T12:00:00Z");
        inserir(&conn, &s).unwrap();
        assert!(conn
            .execute("UPDATE video_edit_outputs SET estado = 'pronta' WHERE id = ?1", [s.id.to_string()])
            .is_err());
    }

    #[test]
    fn excluir_saida() {
        let (conn, occ) = banco();
        let a = saida(occ, "p1", "2026-11-14T12:00:00Z");
        let b = saida(occ, "p1", "2026-11-14T13:00:00Z");
        inserir(&conn, &a).unwrap();
        inserir(&conn, &b).unwrap();
        assert!(!excluir(&conn, &Uuid::new_v4(), &a.id).unwrap(), "outro caso não exclui");
        assert!(excluir(&conn, &occ, &a.id).unwrap());
        assert!(!excluir(&conn, &occ, &a.id).unwrap());
        assert_eq!(obter(&conn, &occ, &a.id).unwrap(), None);
        assert_eq!(listar_do_caso(&conn, &occ).unwrap().len(), 1);
    }
}
