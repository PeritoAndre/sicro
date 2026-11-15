//! Proveniência da mídia na análise (F6, O3): "Formato do gravador" (Sim, Não), "Observação", "Desvio do relógio (s)"
//! e "Gravador", nas colunas da migração 027 de `video_media` (`recebido_nativo`, `recebido_obs`, `relogio_desvio_s`,
//! `gravador`). A Edição copia para o `MidiaRef` ao trazer a mídia (`recebido`, `relogio_desvio_s`, `gravador`).
//! O `occurrence_id` vem do Manifest. Dono: P1.10.
//! Erro de leitura (banco ausente, valor fora do tipo da coluna) sobe para quem chama: nunca vira "não informado".

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::database::repositories::{occurrence_repo, video_repo};
use crate::edicao::executar::abrir_banco;
use crate::edicao::mensagens::{self, codigo};
use crate::error::{Result, SicroError};

/// Tetos do texto livre, em caracteres.
pub const MAX_OBS: usize = 4_000;
pub const MAX_GRAVADOR: usize = 200;

/// Os quatro campos; None = não informado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvenienciaMidia {
    pub recebido_nativo: Option<bool>,
    pub recebido_obs: String,
    pub relogio_desvio_s: Option<f64>,
    pub gravador: Option<String>,
}

fn invalido(campo: &str) -> SicroError {
    SicroError::Validation(format!("{}: {campo}", codigo::ITEM_INVALIDO))
}

fn id_da_midia(media_id: &str) -> Result<Uuid> {
    Uuid::parse_str(media_id.trim()).map_err(|_| invalido("media_id"))
}

/// Linha da mídia no caso: a proveniência gravada e o SHA-256 (para o log).
fn ler_na(conn: &Connection, occ: &Uuid, id: &Uuid) -> Result<(ProvenienciaMidia, String)> {
    let linha = conn
        .query_row(
            "SELECT sha256, recebido_nativo, recebido_obs, relogio_desvio_s, gravador
             FROM video_media WHERE id = ?1 AND occurrence_id = ?2",
            params![id.to_string(), occ.to_string()],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<f64>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()?;
    let Some((sha256, nativo, obs, desvio, gravador)) = linha else {
        return Err(SicroError::Validation(format!("{}: {id}", codigo::MIDIA_AUSENTE)));
    };
    // valor que a tela não grava é dado estragado, não "não informado"
    let recebido_nativo = match nativo {
        None => None,
        Some(0) => Some(false),
        Some(1) => Some(true),
        Some(_) => return Err(SicroError::Database(format!("video_media.recebido_nativo fora de 0/1 ({id})"))),
    };
    if desvio.is_some_and(|d| !d.is_finite()) {
        return Err(SicroError::Database(format!("video_media.relogio_desvio_s não finito ({id})")));
    }
    let p = ProvenienciaMidia {
        recebido_nativo,
        recebido_obs: obs.unwrap_or_default(),
        relogio_desvio_s: desvio,
        gravador: gravador.filter(|g| !g.is_empty()),
    };
    Ok((p, sha256))
}

/// Texto aparado, vazio = não informado; desvio finito.
fn normalizar(p: ProvenienciaMidia) -> Result<ProvenienciaMidia> {
    let obs = p.recebido_obs.trim().to_string();
    if obs.chars().count() > MAX_OBS {
        return Err(invalido("recebido_obs"));
    }
    let gravador = p.gravador.map(|g| g.trim().to_string()).filter(|g| !g.is_empty());
    if gravador.as_ref().is_some_and(|g| g.chars().count() > MAX_GRAVADOR) {
        return Err(invalido("gravador"));
    }
    let relogio_desvio_s = match p.relogio_desvio_s {
        Some(d) if !d.is_finite() => return Err(invalido("relogio_desvio_s")),
        // -0 grava como 0
        d => d.map(|d| d + 0.0),
    };
    Ok(ProvenienciaMidia { recebido_nativo: p.recebido_nativo, recebido_obs: obs, relogio_desvio_s, gravador })
}

fn ler(ws: &Path, media_id: &str) -> Result<ProvenienciaMidia> {
    let id = id_da_midia(media_id)?;
    let (conn, occ) = abrir_banco(ws)?;
    Ok(ler_na(&conn, &occ, &id)?.0)
}

/// Grava numa transação (colunas, log da mídia e auditoria) e relê o que ficou. Sem mudança, não grava nada.
fn salvar(ws: &Path, media_id: &str, p: ProvenienciaMidia) -> Result<ProvenienciaMidia> {
    let id = id_da_midia(media_id)?;
    let novo = normalizar(p)?;
    let (mut conn, occ) = abrir_banco(ws)?;
    // IMMEDIATE: a trava de escrita vem antes da leitura e espera o busy_timeout; em DEFERRED, outro escritor no meio
    // faria o UPDATE falhar na hora (retrato de leitura vencido)
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (antes, sha256) = ler_na(&tx, &occ, &id)?;
    if antes == novo {
        return Ok(antes);
    }
    // `updated_at` fica: a lista de vídeos do caso (e a Integridade) ordena por ele; a data da mudança está no log
    let n = tx.execute(
        "UPDATE video_media SET recebido_nativo = ?1, recebido_obs = ?2, relogio_desvio_s = ?3, gravador = ?4
         WHERE id = ?5 AND occurrence_id = ?6",
        params![
            novo.recebido_nativo.map(i64::from),
            Some(novo.recebido_obs.as_str()).filter(|o| !o.is_empty()),
            novo.relogio_desvio_s,
            novo.gravador.as_deref(),
            id.to_string(),
            occ.to_string(),
        ],
    )?;
    if n != 1 {
        return Err(SicroError::Validation(format!("{}: {id}", codigo::MIDIA_AUSENTE)));
    }
    let detalhe = json!({ "media_id": id.to_string(), "sha256": sha256, "antes": antes, "depois": novo }).to_string();
    video_repo::insert_log(&tx, &occ, Some(&sha256), "media.proveniencia", &detalhe)?;
    occurrence_repo::record_audit(&tx, Some(&occ), "video.proveniencia", Some("video"), Some("video_media"), Some(&id), Some(&detalhe))?;
    let (gravado, _) = ler_na(&tx, &occ, &id)?;
    tx.commit()?;
    Ok(gravado)
}

/// Thread própria para o SQLite; erro com código passa pelo catálogo (`"<código>: <frase>"`).
async fn em_fundo<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| SicroError::Workspace(format!("proveniência: {e}")))
        .and_then(|r| r)
        .map_err(mensagens::erro_de_comando)
}

/// Proveniência gravada da mídia `media_id` (`video_media.id`) do caso.
#[tauri::command]
pub async fn video_proveniencia(workspace_path: String, media_id: String) -> Result<ProvenienciaMidia> {
    em_fundo(move || ler(&PathBuf::from(workspace_path), &media_id)).await
}

/// Grava a proveniência (audita `video.proveniencia`) e devolve o que ficou gravado.
#[tauri::command]
pub async fn video_proveniencia_salvar(workspace_path: String, media_id: String, proveniencia: ProvenienciaMidia) -> Result<ProvenienciaMidia> {
    em_fundo(move || salvar(&PathBuf::from(workspace_path), &media_id, proveniencia)).await
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::database::connection::open_connection;
    use crate::workspace::manifest::Manifest;
    use crate::workspace::paths::sqlite_path;

    /// Caso de verdade (manifesto e banco, acento no caminho) com uma mídia de vídeo; devolve o id dela.
    fn caso() -> (tempfile::TempDir, PathBuf, String) {
        let d = tempfile::Builder::new().prefix("p1.10-prov-").tempdir().unwrap();
        let pai = d.path().join("Casos ç");
        std::fs::create_dir_all(&pai).unwrap();
        let ws = crate::workspace::create::create_workspace(crate::models::NewOccurrenceInput::default(), &pai).unwrap().path;
        let id = Uuid::new_v4().to_string();
        inserir(&ws, &id, &Manifest::read(&ws).unwrap().occurrence_id.to_string(), "h1");
        (d, ws, id)
    }

    fn inserir(ws: &Path, id: &str, occ: &str, sha: &str) {
        let conn = open_connection(&sqlite_path(ws)).unwrap();
        if occ != Manifest::read(ws).unwrap().occurrence_id.to_string() {
            conn.execute("INSERT INTO occurrences (id, created_at, updated_at) VALUES (?1, 'x', 'x')", [occ]).unwrap();
        }
        conn.execute(
            "INSERT INTO video_media (id, occurrence_id, relative_path, filename, sha256, created_at, updated_at)
             VALUES (?1, ?2, 'Mídias/Vídeos/cam 1.mp4', 'cam 1.mp4', ?3, 'x', 'x')",
            params![id, occ, sha],
        )
        .unwrap();
    }

    type Colunas = (Option<i64>, Option<String>, Option<f64>, Option<String>);
    fn colunas(ws: &Path, id: &str) -> Colunas {
        open_connection(&sqlite_path(ws))
            .unwrap()
            .query_row(
                "SELECT recebido_nativo, recebido_obs, relogio_desvio_s, gravador FROM video_media WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap()
    }

    fn contar(ws: &Path, sql: &str) -> i64 {
        open_connection(&sqlite_path(ws)).unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
    }
    fn auditorias(ws: &Path) -> i64 {
        contar(ws, "SELECT COUNT(*) FROM audit_logs WHERE action = 'video.proveniencia'")
    }

    fn prov(nativo: Option<bool>, obs: &str, desvio: Option<f64>, gravador: Option<&str>) -> ProvenienciaMidia {
        ProvenienciaMidia { recebido_nativo: nativo, recebido_obs: obs.into(), relogio_desvio_s: desvio, gravador: gravador.map(Into::into) }
    }

    fn rodar<T>(f: impl std::future::Future<Output = T>) -> T {
        tauri::async_runtime::block_on(f)
    }

    fn codigo_de(r: Result<impl std::fmt::Debug>) -> String {
        match r {
            Err(SicroError::Validation(t)) => t.split(':').next().unwrap().to_string(),
            outro => panic!("esperava Validation, veio {outro:?}"),
        }
    }

    /// Grava pelos comandos (na thread de fundo), lê de volta, confere as colunas, o log da mídia e a auditoria.
    #[test]
    fn grava_e_le_num_caso() {
        let (_d, ws, id) = caso();
        let w = ws.to_string_lossy().to_string();

        // nada gravado: tudo "não informado"
        assert_eq!(rodar(video_proveniencia(w.clone(), id.clone())).unwrap(), prov(None, "", None, None));

        let gravado = rodar(video_proveniencia_salvar(
            w.clone(),
            id.to_uppercase(),
            prov(Some(true), "  veio do DVR pelo pendrive  ", Some(-12.25), Some(" Intelbras MHDX 1016 ")),
        ))
        .unwrap();
        let esperado = prov(Some(true), "veio do DVR pelo pendrive", Some(-12.25), Some("Intelbras MHDX 1016"));
        assert_eq!(gravado, esperado);
        assert_eq!(colunas(&ws, &id), (Some(1), Some("veio do DVR pelo pendrive".into()), Some(-12.25), Some("Intelbras MHDX 1016".into())));
        assert_eq!(rodar(video_proveniencia(w.clone(), id.clone())).unwrap(), esperado);
        assert_eq!(auditorias(&ws), 1);
        let detalhe: String = open_connection(&sqlite_path(&ws))
            .unwrap()
            .query_row("SELECT details_json FROM video_operation_logs WHERE action = 'media.proveniencia' AND media_hash = 'h1'", [], |r| r.get(0))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&detalhe).unwrap();
        assert_eq!(v["antes"]["recebido_nativo"], serde_json::Value::Null);
        assert_eq!(v["depois"]["relogio_desvio_s"], -12.25);
        let entidade: String = open_connection(&sqlite_path(&ws))
            .unwrap()
            .query_row("SELECT entity_id FROM audit_logs WHERE action = 'video.proveniencia'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(entidade, id);

        // gravar a proveniência não leva a mídia ao topo da lista de vídeos (ordenada por `updated_at`)
        assert_eq!(contar(&ws, "SELECT COUNT(*) FROM video_media WHERE updated_at <> 'x'"), 0);

        // o mesmo de novo: nada a gravar, nenhuma auditoria a mais
        assert_eq!(rodar(video_proveniencia_salvar(w.clone(), id.clone(), esperado.clone())).unwrap(), esperado);
        assert_eq!(auditorias(&ws), 1);

        // "Não", sem observação, sem desvio e gravador em branco: Não fica 0, o resto volta a nulo
        let limpo = rodar(video_proveniencia_salvar(w.clone(), id.clone(), prov(Some(false), "   ", None, Some("  ")))).unwrap();
        assert_eq!(limpo, prov(Some(false), "", None, None));
        assert_eq!(colunas(&ws, &id), (Some(0), None, None, None));
        assert_eq!(auditorias(&ws), 2);

        // desvio -0 grava 0; Sim volta
        let z = salvar(&ws, &id, prov(Some(true), "", Some(-0.0), None)).unwrap();
        assert_eq!(z.relogio_desvio_s.map(f64::to_bits), Some(0.0f64.to_bits()));
    }

    /// Valor fora da regra é recusado com código e o banco fica como estava.
    #[test]
    fn recusa_valor_invalido_sem_gravar() {
        let (_d, ws, id) = caso();
        salvar(&ws, &id, prov(Some(true), "x", Some(1.5), Some("g"))).unwrap();
        let antes = colunas(&ws, &id);
        for (p, campo) in [
            (prov(None, "", Some(f64::NAN), None), "relogio_desvio_s"),
            (prov(None, "", Some(f64::INFINITY), None), "relogio_desvio_s"),
            (prov(None, &"a".repeat(MAX_OBS + 1), None, None), "recebido_obs"),
            (prov(None, "", None, Some(&"g".repeat(MAX_GRAVADOR + 1))), "gravador"),
        ] {
            match salvar(&ws, &id, p) {
                Err(SicroError::Validation(t)) => assert_eq!(t, format!("item_invalido: {campo}")),
                outro => panic!("{campo}: {outro:?}"),
            }
        }
        // no teto, vale (acentos contam como um caractere)
        salvar(&ws, &id, prov(None, &"ç".repeat(MAX_OBS), None, Some(&"é".repeat(MAX_GRAVADOR)))).unwrap();
        assert_eq!(colunas(&ws, &id).1.unwrap().chars().count(), MAX_OBS);
        salvar(&ws, &id, prov(Some(true), "x", Some(1.5), Some("g"))).unwrap();
        assert_eq!(colunas(&ws, &id), antes);
        assert_eq!(codigo_de(salvar(&ws, "não-é-uuid", prov(None, "", None, None))), "item_invalido");
        assert_eq!(codigo_de(ler(&ws, "")), "item_invalido");
    }

    /// Mídia de outro caso (mesmo banco) ou inexistente: `midia_ausente`, pelo comando com a frase do catálogo.
    #[test]
    fn midia_de_outro_caso_e_ausente() {
        let (_d, ws, id) = caso();
        let outra = Uuid::new_v4().to_string();
        inserir(&ws, &outra, &Uuid::new_v4().to_string(), "h2");
        assert_eq!(codigo_de(ler(&ws, &outra)), "midia_ausente");
        assert_eq!(codigo_de(salvar(&ws, &outra, prov(Some(true), "", None, None))), "midia_ausente");
        assert_eq!(colunas(&ws, &outra), (None, None, None, None));
        assert_eq!(codigo_de(ler(&ws, &Uuid::new_v4().to_string())), "midia_ausente");
        let w = ws.to_string_lossy().to_string();
        match rodar(video_proveniencia(w, outra)) {
            Err(SicroError::Validation(t)) => assert_eq!(t, "midia_ausente: Mídia ausente"),
            r => panic!("{r:?}"),
        }
        // a do caso continua lendo
        assert_eq!(ler(&ws, &id).unwrap(), prov(None, "", None, None));
    }

    /// Outro escritor segura o banco e grava no meio: a gravação espera a vez (busy_timeout) em vez de falhar com o
    /// retrato de leitura vencido.
    #[test]
    fn espera_outro_escritor_em_vez_de_falhar() {
        // o outro escritor solta bem antes do busy_timeout (1 s) da conexão
        const SEGURA_MS: u64 = 300;
        let (_d, ws, id) = caso();
        // esquema já migrado: o `salvar` não tem o que escrever antes da transação dele
        drop(abrir_banco(&ws).unwrap());
        let outro = open_connection(&sqlite_path(&ws)).unwrap();
        outro.execute_batch("BEGIN IMMEDIATE; UPDATE video_media SET updated_at = 'y';").unwrap();
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(SEGURA_MS));
            outro.execute_batch("COMMIT").unwrap();
        });
        let r = salvar(&ws, &id, prov(Some(true), "", None, Some("g")));
        t.join().unwrap();
        assert_eq!(r.unwrap(), prov(Some(true), "", None, Some("g")));
        assert_eq!(colunas(&ws, &id), (Some(1), None, None, Some("g".into())));
        assert_eq!(auditorias(&ws), 1);
    }

    /// Erro de leitura sobe: banco ausente, valor fora do tipo da coluna, 0/1 estragado, desvio infinito.
    #[test]
    fn erro_de_leitura_nao_vira_nao_informado() {
        let (_d, ws, id) = caso();
        let estragar = |sql: &str| {
            let conn = open_connection(&sqlite_path(&ws)).unwrap();
            conn.execute("UPDATE video_media SET recebido_nativo = NULL, relogio_desvio_s = NULL, gravador = NULL, recebido_obs = NULL", []).unwrap();
            conn.execute(sql, [&id]).unwrap();
        };
        for sql in [
            "UPDATE video_media SET relogio_desvio_s = 'doze' WHERE id = ?1",
            "UPDATE video_media SET recebido_nativo = 7 WHERE id = ?1",
            "UPDATE video_media SET recebido_nativo = 'sim' WHERE id = ?1",
            "UPDATE video_media SET relogio_desvio_s = 9e999 WHERE id = ?1",
            "UPDATE video_media SET gravador = X'00ff' WHERE id = ?1",
        ] {
            estragar(sql);
            let r = ler(&ws, &id);
            assert!(matches!(r, Err(SicroError::Database(_) | SicroError::Sqlite(_))), "{sql}: {r:?}");
            // gravar por cima de linha estragada também falha (lê antes de comparar)
            assert!(salvar(&ws, &id, prov(Some(true), "", None, None)).is_err(), "{sql}");
        }
        estragar("UPDATE video_media SET recebido_nativo = 1 WHERE id = ?1");
        assert_eq!(ler(&ws, &id).unwrap().recebido_nativo, Some(true));

        std::fs::remove_file(sqlite_path(&ws)).unwrap();
        let r = rodar(video_proveniencia(ws.to_string_lossy().to_string(), id.clone()));
        assert!(matches!(r, Err(SicroError::Database(_))), "{r:?}");
        assert!(!sqlite_path(&ws).exists(), "a leitura criou um banco vazio");
    }
}
