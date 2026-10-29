//! Rastreio no vídeo: detectar (YOLOX, 80 classes + ByteTrack), seguir manchas marcadas à mão,
//! editar as trilhas, e gerar as
//! saídas (MP4 com a sobreposição, quadros no storyboard, tabela CSV/HTML).
//! O vídeo original nunca é tocado; tudo que sai tem SHA-256 e entra na trilha de operações.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::commands::settings_commands::get_app_settings;
use crate::commands::video_commands::collect_video_frame;
use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::video_people_repo::{self as repo, Run};
use crate::database::repositories::video_repo;
use crate::error::{Result, SicroError};
use crate::hashing::sha256::sha256_file;
use crate::models::{CollectFrameInput, VideoMedia, VideoStoryboardFrame};
use crate::video::pessoas::classes::{self, ClasseInfo, CLASSE_MANUAL};
use crate::video::pessoas::mancha::{self, Motivo, OpcoesMancha};
use crate::video::pessoas::pipeline::{self, OpcoesRastreio};
use crate::video::pessoas::render::Opcoes as OpcoesDesenho;
use crate::video::pessoas::{cor_padrao, Amostra, Cadencia, Pessoa};
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};

const PESSOAS_SUBDIR: &str = "videos/pessoas";

/// Só uma tarefa longa de pessoas por vez (rastrear ou gravar o vídeo).
static TAREFA: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

fn iniciar_tarefa() -> Result<Arc<AtomicBool>> {
    let mut g = TAREFA.lock().map_err(|_| SicroError::Workspace("estado interno".into()))?;
    if g.is_some() {
        return Err(SicroError::Validation("já existe uma tarefa de pessoas em andamento".into()));
    }
    let flag = Arc::new(AtomicBool::new(false));
    *g = Some(flag.clone());
    Ok(flag)
}

fn encerrar_tarefa() {
    if let Ok(mut g) = TAREFA.lock() {
        *g = None;
    }
}

#[tauri::command]
pub async fn pessoas_cancelar() -> Result<()> {
    if let Ok(g) = TAREFA.lock() {
        if let Some(f) = g.as_ref() {
            f.store(true, Ordering::Relaxed);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tipos para a tela

#[derive(Debug, Serialize)]
pub struct PessoaView {
    pub id: String,
    pub numero: i64,
    pub nome: String,
    pub cor: String,
    pub estado: String,
    /// Classe COCO (0 = pessoa); -1 = mancha à mão.
    pub classe: i32,
    pub classe_nome: String,
    /// "auto" | "manual"
    pub origem: String,
    pub primeiro_s: f64,
    pub ultimo_s: f64,
    pub quadros: usize,
    /// Caixas efetivas (automática + correções): [t, x, y, w, h].
    pub caixas: Vec<[f64; 5]>,
    /// Instantes com correção do perito.
    pub ajustes_t: Vec<f64>,
}

#[derive(Debug, Serialize)]
pub struct PessoasEstado {
    pub run: Option<Run>,
    pub pessoas: Vec<PessoaView>,
    /// Tamanho do vídeo exibido, quando ainda não há rastreio (para desenhar a mancha).
    pub tamanho: Option<TamanhoVideo>,
}

#[derive(Debug, Serialize)]
pub struct TamanhoVideo {
    pub width: u32,
    pub height: u32,
}

fn view(p: &Pessoa) -> PessoaView {
    PessoaView {
        id: p.id.clone(),
        numero: p.numero,
        nome: p.nome.clone(),
        cor: p.cor.clone(),
        estado: p.estado.clone(),
        classe: p.classe,
        classe_nome: p.rotulo(),
        origem: p.origem.clone(),
        primeiro_s: p.primeiro_s(),
        ultimo_s: p.ultimo_s(),
        quadros: p.amostras.len(),
        caixas: p
            .caixas()
            .iter()
            .map(|a| [a.t, a.x as f64, a.y as f64, a.w as f64, a.h as f64])
            .collect(),
        ajustes_t: p.ajustes.iter().map(|a| a.t).collect(),
    }
}

fn estado(conn: &Connection, occ: &Uuid, hash: &str) -> Result<PessoasEstado> {
    Ok(PessoasEstado {
        run: repo::get_run(conn, occ, hash)?,
        pessoas: repo::list(conn, occ, hash)?.iter().map(view).collect(),
        tamanho: None,
    })
}

#[derive(Debug, Serialize, Clone)]
struct Progresso<'a> {
    media_hash: &'a str,
    fase: &'a str,
    feitos: u32,
    total: u32,
    pessoas: u32,
}

struct Contexto {
    ws: PathBuf,
    occ: Uuid,
    conn: Connection,
    media: VideoMedia,
}

fn contexto(workspace_path: &str, media_hash: &str) -> Result<Contexto> {
    let ws = PathBuf::from(workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let media = video_repo::find_media_by_sha256(&conn, &manifest.occurrence_id, media_hash)?
        .ok_or_else(|| SicroError::Validation("vídeo não encontrado nesta ocorrência".into()))?;
    Ok(Contexto { ws, occ: manifest.occurrence_id, conn, media })
}

fn cadencia(run: &Run) -> Cadencia {
    Cadencia { frame_dur: run.frame_dur_s, passo: run.step.max(1) }
}

fn tempo(t: f64) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as i64;
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000)
}

// ---------------------------------------------------------------------------
// Rastrear

/// As 80 classes (nome em português e grupo) para o seletor.
#[tauri::command]
pub async fn pessoas_classes() -> Result<Vec<ClasseInfo>> {
    Ok(classes::catalogo())
}

#[tauri::command]
pub async fn pessoas_listar(workspace_path: String, media_hash: String) -> Result<PessoasEstado> {
    let c = contexto(&workspace_path, &media_hash)?;
    let mut e = estado(&c.conn, &c.occ, &media_hash)?;
    if e.run.is_none() {
        let video = c.ws.join(&c.media.relative_path);
        if let Ok((width, height)) = crate::video::pessoas::decode::display_size(&video) {
            e.tamanho = Some(TamanhoVideo { width, height });
        }
    }
    Ok(e)
}

#[tauri::command]
pub async fn pessoas_rastrear(
    app: AppHandle,
    workspace_path: String,
    media_hash: String,
    opcoes: Option<OpcoesRastreio>,
) -> Result<PessoasEstado> {
    let ai = get_app_settings(app.clone()).await?.ai;
    if !crate::commands::ai_commands::pessoas_ready(&ai) {
        return Err(SicroError::Validation("pessoas_nao_instalado".into()));
    }
    let c = contexto(&workspace_path, &media_hash)?;
    let video = c.ws.join(&c.media.relative_path);
    if !video.is_file() {
        return Err(SicroError::Filesystem(format!("arquivo de vídeo ausente: {}", c.media.relative_path)));
    }
    let op = opcoes.unwrap_or_default();
    let op_run = op.clone();
    let flag = iniciar_tarefa()?;
    let (app2, hash2) = (app.clone(), media_hash.clone());
    let (lib, modelo) = (PathBuf::from(&ai.ort_lib_path), PathBuf::from(&ai.pessoas_model_path));
    let r = tauri::async_runtime::spawn_blocking(move || {
        pipeline::rastrear(&video, &hash2, &lib, &modelo, op, &flag, &mut |a| {
            let _ = app2.emit(
                "pessoas-progress",
                Progresso { media_hash: &hash2, fase: "rastrear", feitos: a.feitos, total: a.total, pessoas: a.pessoas },
            );
        })
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("tarefa de rastreio: {e}")));
    encerrar_tarefa();
    let r = r??;

    // as manchas marcadas à mão sobrevivem e passam a ter os números seguintes
    repo::delete_auto(&c.conn, &c.occ, &media_hash)?;
    for p in &r.pessoas {
        repo::insert(&c.conn, &c.occ, p)?;
    }
    let mut n = r.pessoas.len() as i64;
    for mut m in repo::list(&c.conn, &c.occ, &media_hash)?.into_iter().filter(|p| p.origem == "manual") {
        n += 1;
        m.numero = n;
        repo::update(&c.conn, &m)?;
    }
    let run = Run {
        media_hash: media_hash.clone(),
        model: "YOLOX-S (Apache-2.0)".into(),
        runtime: format!("onnxruntime {}", ai.pessoas_version),
        params_json: json!({
            "limiar": op_run.limiar, "passo": op_run.passo, "tracker": "ByteTrack",
            "classes": op_run.classes,
            "classes_nomes": op_run.classes.iter().map(|&c| classes::nome(c as i32)).collect::<Vec<_>>(),
        }).to_string(),
        width: r.meta.largura,
        height: r.meta.altura,
        frame_dur_s: r.meta.frame_dur_s,
        step: r.meta.passo,
        frames_total: r.meta.quadros_total,
        frames_analyzed: r.meta.quadros_analisados,
        elapsed_s: r.meta.tempo_s,
        created_at: Utc::now().to_rfc3339(),
    };
    repo::replace_run(&c.conn, &c.occ, &run)?;
    video_repo::insert_log(
        &c.conn,
        &c.occ,
        Some(&media_hash),
        "pessoas.rastrear",
        &json!({ "pessoas": r.pessoas.len(), "meta": r.meta, "params": run.params_json, "modelo": run.model }).to_string(),
    )?;
    estado(&c.conn, &c.occ, &media_hash)
}

// ---------------------------------------------------------------------------
// Edição

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Edicao {
    /// A `origem` é a mesma pessoa que o `destino`: vira uma trilha só (fica o número do destino).
    Juntar { destino: String, origem: String },
    /// Do instante `t` em diante vira outra pessoa (número novo).
    Separar { id: String, t: f64 },
    Apagar { id: String },
    Nome { id: String, nome: String },
    Cor { id: String, cor: String },
    /// Renumera de 1 a n na ordem da lista.
    Ordem { ids: Vec<String> },
    /// Troca o número (quem já o tinha fica com o antigo).
    Numero { id: String, numero: i64 },
    /// Caixa que o perito desenhou no quadro do instante `t`.
    Ajustar { id: String, t: f64, x: f32, y: f32, w: f32, h: f32 },
    LimparAjuste { id: String, t: f64 },
}

#[tauri::command]
pub async fn pessoas_editar(
    workspace_path: String,
    media_hash: String,
    edicao: Edicao,
) -> Result<PessoasEstado> {
    let c = contexto(&workspace_path, &media_hash)?;
    let mut todas = repo::list(&c.conn, &c.occ, &media_hash)?;
    let achar = |todas: &[Pessoa], id: &str| -> Result<usize> {
        todas
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| SicroError::Validation("pessoa não encontrada".into()))
    };
    let log = |acao: &str, d: serde_json::Value| video_repo::insert_log(&c.conn, &c.occ, Some(&media_hash), acao, &d.to_string());

    match edicao {
        Edicao::Juntar { destino, origem } => {
            if destino == origem {
                return Err(SicroError::Validation("escolha duas pessoas diferentes".into()));
            }
            let (d, o) = (achar(&todas, &destino)?, achar(&todas, &origem)?);
            let outra = todas[o].clone();
            todas[d].juntar(&outra);
            repo::update(&c.conn, &todas[d])?;
            repo::delete(&c.conn, &outra.id)?;
            log("pessoas.juntar", json!({ "destino": todas[d].numero, "origem": outra.numero }))?;
        }
        Edicao::Separar { id, t } => {
            let i = achar(&todas, &id)?;
            let numero = todas.iter().map(|p| p.numero).max().unwrap_or(0) + 1;
            let nova = todas[i]
                .separar(t, Uuid::new_v4().to_string(), numero)
                .ok_or_else(|| SicroError::Validation("separe em um instante com a pessoa visível dos dois lados".into()))?;
            repo::update(&c.conn, &todas[i])?;
            repo::insert(&c.conn, &c.occ, &nova)?;
            log("pessoas.separar", json!({ "pessoa": todas[i].numero, "t": t, "nova": numero }))?;
        }
        Edicao::Apagar { id } => {
            let i = achar(&todas, &id)?;
            repo::delete(&c.conn, &id)?;
            log("pessoas.apagar", json!({ "pessoa": todas[i].numero }))?;
        }
        Edicao::Nome { id, nome } => {
            let i = achar(&todas, &id)?;
            todas[i].nome = nome.trim().to_string();
            repo::update(&c.conn, &todas[i])?;
        }
        Edicao::Cor { id, cor } => {
            let i = achar(&todas, &id)?;
            todas[i].cor = cor;
            repo::update(&c.conn, &todas[i])?;
        }
        Edicao::Ordem { ids } => {
            let mut n = 0;
            let mut resto: Vec<usize> = (0..todas.len()).collect();
            for id in &ids {
                if let Some(i) = todas.iter().position(|p| &p.id == id) {
                    n += 1;
                    todas[i].numero = n;
                    resto.retain(|&r| r != i);
                }
            }
            for i in resto {
                n += 1;
                todas[i].numero = n;
            }
            for p in &mut todas {
                if p.estado == "auto" && p.cor == cor_padrao(p.numero) {
                    continue;
                }
                p.estado = "editado".into();
            }
            for p in &todas {
                repo::update(&c.conn, p)?;
            }
            log("pessoas.renumerar", json!({ "ordem": ids.len() }))?;
        }
        Edicao::Numero { id, numero } => {
            if numero < 1 {
                return Err(SicroError::Validation("o número começa em 1".into()));
            }
            let i = achar(&todas, &id)?;
            let antigo = todas[i].numero;
            if let Some(j) = todas.iter().position(|p| p.numero == numero && p.id != id) {
                todas[j].numero = antigo;
                repo::update(&c.conn, &todas[j])?;
            }
            todas[i].numero = numero;
            repo::update(&c.conn, &todas[i])?;
            log("pessoas.numero", json!({ "de": antigo, "para": numero }))?;
        }
        Edicao::Ajustar { id, t, x, y, w, h } => {
            let i = achar(&todas, &id)?;
            if !todas[i].ajustar(t, x, y, w, h) {
                return Err(SicroError::Validation("pessoa sem quadros".into()));
            }
            repo::update(&c.conn, &todas[i])?;
            log("pessoas.ajustar", json!({ "pessoa": todas[i].numero, "t": t }))?;
        }
        Edicao::LimparAjuste { id, t } => {
            let i = achar(&todas, &id)?;
            todas[i].limpar_ajuste(t);
            repo::update(&c.conn, &todas[i])?;
        }
    }
    estado(&c.conn, &c.occ, &media_hash)
}

// ---------------------------------------------------------------------------
// Saídas

#[derive(Debug, Serialize)]
pub struct ArquivoGerado {
    /// Relativo ao caso.
    pub caminho: String,
    pub absoluto: String,
    pub sha256: String,
    pub tamanho: u64,
}

fn registrar_saida(c: &Contexto, tipo: &str, rel: &str, nome: &str, detalhes: serde_json::Value) -> Result<ArquivoGerado> {
    let abs = c.ws.join(rel);
    let sha256 = sha256_file(&abs)?;
    let tamanho = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
    let mut d = detalhes;
    d["sha256"] = json!(sha256);
    d["size_bytes"] = json!(tamanho);
    let x = crate::models::VideoExport {
        id: Uuid::new_v4(),
        occurrence_id: c.occ,
        media_hash: c.media.sha256.clone(),
        event_id: None,
        r#type: tipo.to_string(),
        requested_timestamp_s: 0.0,
        actual_timestamp_s: None,
        delta_s: None,
        output_path: rel.to_string(),
        filename: nome.to_string(),
        sidecar_json_path: None,
        details_json: d.to_string(),
        created_at: Utc::now(),
    };
    video_repo::insert_export(&c.conn, &x)?;
    video_repo::insert_log(
        &c.conn,
        &c.occ,
        Some(&c.media.sha256),
        &format!("pessoas.exportar.{tipo}"),
        &json!({ "arquivo": rel, "sha256": sha256 }).to_string(),
    )?;
    Ok(ArquivoGerado { caminho: rel.to_string(), absoluto: abs.to_string_lossy().to_string(), sha256, tamanho })
}

fn pasta_saida(c: &Contexto) -> Result<(PathBuf, String)> {
    let dir = c.ws.join(PESSOAS_SUBDIR);
    std::fs::create_dir_all(&dir)
        .map_err(|e| SicroError::Filesystem(format!("criar {}: {e}", dir.display())))?;
    let stem = std::path::Path::new(&c.media.filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(crate::filesystem::sanitize_folder_name)
        .unwrap_or_else(|| "video".into());
    Ok((dir, format!("{stem}_pessoas_{}", Utc::now().format("%Y%m%d_%H%M%S"))))
}

fn carregar(c: &Contexto) -> Result<(Vec<Pessoa>, Run)> {
    let run = repo::get_run(&c.conn, &c.occ, &c.media.sha256)?
        .ok_or_else(|| SicroError::Validation("este vídeo ainda não tem rastreio".into()))?;
    let pessoas = repo::list(&c.conn, &c.occ, &c.media.sha256)?;
    if pessoas.is_empty() {
        return Err(SicroError::Validation("não há nada numerado para exportar".into()));
    }
    Ok((pessoas, run))
}

#[tauri::command]
pub async fn pessoas_exportar_video(
    app: AppHandle,
    workspace_path: String,
    media_hash: String,
    desenho: OpcoesDesenho,
    com_audio: bool,
) -> Result<ArquivoGerado> {
    let c = contexto(&workspace_path, &media_hash)?;
    let (pessoas, run) = carregar(&c)?;
    let (dir, base) = pasta_saida(&c)?;
    let nome = format!("{base}.mp4");
    let video = c.ws.join(&c.media.relative_path);
    let saida = dir.join(&nome);
    let flag = iniciar_tarefa()?;
    let (app2, hash2, saida2) = (app.clone(), media_hash.clone(), saida.clone());
    let cad = cadencia(&run);
    let r = tauri::async_runtime::spawn_blocking(move || {
        pipeline::exportar_video(&video, &saida2, &pessoas, cad, desenho, com_audio, &flag, &mut |a| {
            let _ = app2.emit(
                "pessoas-progress",
                Progresso { media_hash: &hash2, fase: "exportar", feitos: a.feitos, total: a.total, pessoas: 0 },
            );
        })
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("tarefa de exportação: {e}")));
    encerrar_tarefa();
    let quadros = r??;
    registrar_saida(
        &c,
        "pessoas_mp4",
        &format!("{PESSOAS_SUBDIR}/{nome}"),
        &nome,
        json!({ "quadros": quadros, "desenho": desenho, "com_audio": com_audio, "origem_sha256": c.media.sha256 }),
    )
}

#[derive(Debug, Serialize)]
pub struct QuadroPessoas {
    pub frame: VideoStoryboardFrame,
    pub sha256: String,
    pub numeros: Vec<i64>,
}

#[tauri::command]
pub async fn pessoas_exportar_quadros(
    workspace_path: String,
    media_hash: String,
    tempos: Vec<f64>,
    desenho: OpcoesDesenho,
) -> Result<Vec<QuadroPessoas>> {
    let c = contexto(&workspace_path, &media_hash)?;
    let (pessoas, run) = carregar(&c)?;
    let cad = cadencia(&run);
    let mut saida = Vec::new();
    for t in tempos {
        let col = collect_video_frame(
            workspace_path.clone(),
            CollectFrameInput {
                media_hash: media_hash.clone(),
                timestamp_s: t,
                event_id: None,
                title: Some("Rastreio no vídeo".into()),
                caption: None,
                notes: None,
            },
        )
        .await?;
        let png = c.ws.join(&col.export.output_path);
        let t_real = col.export.actual_timestamp_s.unwrap_or(t);
        let (p2, png2) = (pessoas.clone(), png.clone());
        let numeros = tauri::async_runtime::spawn_blocking(move || pipeline::sobrepor_png(&png2, t_real, &p2, cad, desenho))
            .await
            .map_err(|e| SicroError::Workspace(format!("sobreposição: {e}")))??;
        let sha = sha256_file(&png)?;
        // sidecar e legenda dizem o que está desenhado e o hash do arquivo FINAL
        if let Some(rel) = &col.export.sidecar_json_path {
            let sp = c.ws.join(rel);
            if let Ok(txt) = std::fs::read_to_string(&sp) {
                if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&txt) {
                    v["sobreposicao_pessoas"] = json!({
                        "sha256_png_final": sha, "pessoas": numeros, "desenho": desenho,
                        "modelo": run.model, "analise_em": run.created_at,
                    });
                    let _ = std::fs::write(&sp, serde_json::to_string_pretty(&v).unwrap_or(txt));
                }
            }
        }
        let lista = numeros.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ");
        let mut frame = col.storyboard_frame;
        frame.caption = if lista.is_empty() { "Nenhum objeto numerado neste quadro.".into() } else { format!("Numerados: {lista}.") };
        frame.updated_at = Utc::now();
        video_repo::update_storyboard_frame(&c.conn, &frame)?;
        video_repo::insert_log(
            &c.conn,
            &c.occ,
            Some(&media_hash),
            "pessoas.exportar.quadro",
            &json!({ "t": t_real, "sha256": sha, "pessoas": numeros }).to_string(),
        )?;
        saida.push(QuadroPessoas { frame, sha256: sha, numeros });
    }
    Ok(saida)
}

fn csv_campo(s: &str) -> String {
    if s.contains([';', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[derive(Debug, Serialize)]
pub struct TabelaGerada {
    pub csv: ArquivoGerado,
    pub html: ArquivoGerado,
}

/// Tabela por objeto (número, classe, nome, entra em, sai de, quadros) em CSV e em relatório HTML.
#[tauri::command]
pub async fn pessoas_exportar_tabela(workspace_path: String, media_hash: String) -> Result<TabelaGerada> {
    let c = contexto(&workspace_path, &media_hash)?;
    let (pessoas, run) = carregar(&c)?;
    let (dir, base) = pasta_saida(&c)?;

    let mut csv = String::from("\u{feff}numero;classe;nome;entra_em;sai_de;entra_s;sai_s;quadros;situacao;origem\r\n");
    let mut linhas = String::new();
    for p in &pessoas {
        let situ = if p.estado == "editado" { "editada" } else { "automática" };
        let origem = if p.origem == "manual" { "manual" } else { "automática" };
        let classe = if p.classe == CLASSE_MANUAL { "mancha".to_string() } else { classes::nome(p.classe).to_string() };
        csv.push_str(&format!(
            "{};{};{};{};{};{:.3};{:.3};{};{};{}\r\n",
            p.numero, csv_campo(&classe), csv_campo(&p.nome), tempo(p.primeiro_s()), tempo(p.ultimo_s()),
            p.primeiro_s(), p.ultimo_s(), p.amostras.len(), situ, origem
        ));
        linhas.push_str(&format!(
            "<tr><td class=n>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class=n>{}</td><td>{}</td><td>{}</td></tr>\n",
            p.numero, esc(&classe), esc(&p.nome), tempo(p.primeiro_s()), tempo(p.ultimo_s()), p.amostras.len(), situ, origem
        ));
    }
    let nome_csv = format!("{base}.csv");
    std::fs::write(dir.join(&nome_csv), csv)?;
    let html = format!(
        "<!doctype html><html lang=\"pt-BR\"><meta charset=\"utf-8\"><title>Rastreio no vídeo</title>\
<style>body{{font:14px system-ui,sans-serif;margin:32px;color:#222}}table{{border-collapse:collapse;margin-top:12px}}\
td,th{{border:1px solid #bbb;padding:4px 10px;text-align:left}}.n{{text-align:right;font-variant-numeric:tabular-nums}}\
code{{font-size:12px}}dl{{display:grid;grid-template-columns:max-content 1fr;gap:2px 14px}}dt{{color:#666}}</style>\
<h1>Rastreio no vídeo</h1><dl>\
<dt>Vídeo</dt><dd>{}</dd><dt>SHA-256 do vídeo</dt><dd><code>{}</code></dd>\
<dt>Análise</dt><dd>{} · {} · quadros analisados: {} de {}{}</dd>\
<dt>Parâmetros</dt><dd><code>{}</code></dd><dt>Gerado em</dt><dd>{}</dd></dl>\
<table><thead><tr><th>Nº</th><th>Classe</th><th>Nome</th><th>Entra em</th><th>Sai de</th><th>Quadros</th><th>Situação</th><th>Origem</th></tr></thead><tbody>\n{}</tbody></table>\
<p>Detecção e acompanhamento automáticos (origem automática) e manchas marcadas pelo perito e seguidas por correlação de pixels (origem manual), conferidos pelo perito. Cruzamentos, oclusões longas e imagens escuras ou de baixa resolução podem trocar ou perder números.</p></html>",
        esc(&c.media.filename), c.media.sha256, esc(&run.model), esc(&run.runtime),
        run.frames_analyzed, run.frames_total,
        if run.step > 1 { format!(" (um a cada {})", run.step) } else { String::new() },
        esc(&run.params_json), Utc::now().format("%d/%m/%Y %H:%M"), linhas
    );
    let nome_html = format!("{base}.html");
    std::fs::write(dir.join(&nome_html), html)?;
    let det = json!({ "pessoas": pessoas.len(), "origem_sha256": c.media.sha256 });
    let csv = registrar_saida(&c, "pessoas_csv", &format!("{PESSOAS_SUBDIR}/{nome_csv}"), &nome_csv, det.clone())?;
    let html = registrar_saida(&c, "pessoas_html", &format!("{PESSOAS_SUBDIR}/{nome_html}"), &nome_html, det)?;
    Ok(TabelaGerada { csv, html })
}

// ---------------------------------------------------------------------------
// Mancha marcada à mão

#[derive(Debug, Deserialize)]
pub struct MarcaMancha {
    /// Trilha a continuar; ausente = nova mancha.
    pub id: Option<String>,
    pub t: f64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default)]
    pub nome: String,
    /// "frente" | "tras" | "ambos"
    pub direcao: String,
}

#[derive(Debug, Serialize)]
pub struct AvisoMancha {
    /// "frente" | "tras"
    pub sentido: String,
    pub motivo: Motivo,
    /// Instante em que a mancha se perdeu (se parou por isso).
    pub parou_em: Option<f64>,
    pub quadros: usize,
}

#[derive(Debug, Serialize)]
pub struct ResultadoMancha {
    pub estado: PessoasEstado,
    pub id: String,
    pub avisos: Vec<AvisoMancha>,
}

#[tauri::command]
pub async fn pessoas_seguir_mancha(
    app: AppHandle,
    workspace_path: String,
    media_hash: String,
    marca: MarcaMancha,
) -> Result<ResultadoMancha> {
    let c = contexto(&workspace_path, &media_hash)?;
    let video = c.ws.join(&c.media.relative_path);
    if !video.is_file() {
        return Err(SicroError::Filesystem(format!("arquivo de vídeo ausente: {}", c.media.relative_path)));
    }
    if marca.w < 8.0 || marca.h < 8.0 {
        return Err(SicroError::Validation("desenhe uma caixa maior sobre a mancha".into()));
    }
    let direcoes: Vec<bool> = match marca.direcao.as_str() {
        "frente" => vec![true],
        "tras" => vec![false],
        "ambos" => vec![true, false],
        _ => return Err(SicroError::Validation("direção inválida".into())),
    };
    let flag = iniciar_tarefa()?;
    let (app2, hash2) = (app.clone(), media_hash.clone());
    let (t, caixa0) = (marca.t, (marca.x, marca.y, marca.w, marca.h));
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<_> {
        let times = crate::video::pessoas::decode::frame_times(&video)?;
        let (dw, dh) = crate::video::pessoas::decode::display_size(&video)?;
        let idx0 = times
            .iter()
            .enumerate()
            .min_by(|a, b| (a.1 - t).abs().total_cmp(&(b.1 - t).abs()))
            .map_or(0, |(i, _)| i);
        let (x0, y0) = (caixa0.0.clamp(0.0, dw as f32 - 2.0), caixa0.1.clamp(0.0, dh as f32 - 2.0));
        let caixa = (x0, y0, caixa0.2.min(dw as f32 - x0), caixa0.3.min(dh as f32 - y0));
        let mut saidas = Vec::new();
        for frente in direcoes {
            let r = mancha::seguir(&video, &times, dw, dh, idx0, caixa, frente, OpcoesMancha::default(), &flag, &mut |n| {
                let _ = app2.emit(
                    "pessoas-progress",
                    Progresso { media_hash: &hash2, fase: "mancha", feitos: n, total: 0, pessoas: 0 },
                );
            })?;
            saidas.push((frente, r));
        }
        Ok((saidas, times, dw, dh))
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("tarefa de mancha: {e}")));
    encerrar_tarefa();
    let (saidas, times, dw, dh) = r??;

    // união das amostras (o quadro inicial vem nas duas direções)
    let mut novas: Vec<Amostra> = Vec::new();
    let mut avisos = Vec::new();
    let mut conf = 0.0f32;
    for (frente, r) in &saidas {
        for a in &r.amostras {
            if !novas.iter().any(|b| (b.t - a.t).abs() < 1e-6) {
                novas.push(*a);
            }
        }
        conf = conf.max(r.conf_media);
        avisos.push(AvisoMancha {
            sentido: if *frente { "frente" } else { "tras" }.into(),
            motivo: r.motivo,
            parou_em: r.parou_em,
            quadros: r.amostras.len().saturating_sub(1),
        });
    }
    novas.sort_by(|a, b| a.t.total_cmp(&b.t));
    let (tmin, tmax) = (novas[0].t, novas[novas.len() - 1].t);

    let mut todas = repo::list(&c.conn, &c.occ, &media_hash)?;
    let id = match &marca.id {
        Some(id) => {
            let i = todas
                .iter()
                .position(|p| &p.id == id)
                .ok_or_else(|| SicroError::Validation("trilha não encontrada".into()))?;
            let p = &mut todas[i];
            // o trecho refeito substitui o que havia nele
            p.amostras.retain(|a| a.t < tmin - 1e-6 || a.t > tmax + 1e-6);
            p.ajustes.retain(|j| j.t < tmin - 1e-6 || j.t > tmax + 1e-6);
            p.amostras.extend(novas.iter().copied());
            p.amostras.sort_by(|a, b| a.t.total_cmp(&b.t));
            if !marca.nome.trim().is_empty() {
                p.nome = marca.nome.trim().to_string();
            }
            p.estado = "editado".into();
            repo::update(&c.conn, p)?;
            p.id.clone()
        }
        None => {
            let numero = todas.iter().map(|p| p.numero).max().unwrap_or(0) + 1;
            let p = Pessoa {
                id: Uuid::new_v4().to_string(),
                media_hash: media_hash.clone(),
                numero,
                nome: marca.nome.trim().to_string(),
                cor: cor_padrao(numero),
                estado: "auto".into(),
                classe: CLASSE_MANUAL,
                origem: "manual".into(),
                amostras: novas.clone(),
                ajustes: vec![],
            };
            repo::insert(&c.conn, &c.occ, &p)?;
            p.id
        }
    };
    if repo::get_run(&c.conn, &c.occ, &media_hash)?.is_none() {
        let fdur = crate::video::pessoas::decode::frame_dur(&times);
        let run = Run {
            media_hash: media_hash.clone(),
            model: "Mancha manual (correlação)".into(),
            runtime: "SICRO".into(),
            params_json: json!({ "manual": true }).to_string(),
            width: dw,
            height: dh,
            frame_dur_s: fdur,
            step: 1,
            frames_total: times.len() as u32,
            frames_analyzed: 0,
            elapsed_s: 0.0,
            created_at: Utc::now().to_rfc3339(),
        };
        repo::replace_run(&c.conn, &c.occ, &run)?;
    }
    video_repo::insert_log(
        &c.conn,
        &c.occ,
        Some(&media_hash),
        "rastreio.mancha",
        &json!({
            "t": marca.t, "caixa": [marca.x, marca.y, marca.w, marca.h], "direcao": marca.direcao,
            "quadros": novas.len(), "confianca_media": conf, "continuacao": marca.id.is_some(),
            "avisos": avisos.iter().map(|a| json!({"sentido": a.sentido, "motivo": a.motivo, "parou_em": a.parou_em})).collect::<Vec<_>>(),
        })
        .to_string(),
    )?;
    Ok(ResultadoMancha { estado: estado(&c.conn, &c.occ, &media_hash)?, id, avisos })
}
