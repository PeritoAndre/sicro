//! Rastreio no vídeo: detectar (YOLOX, 80 classes + ByteTrack), seguir manchas marcadas à mão,
//! editar as trilhas, e gerar as
//! saídas (MP4 com a sobreposição, quadros no storyboard, tabela CSV/HTML).
//! O vídeo original nunca é tocado; tudo que sai tem SHA-256 e entra na trilha de operações.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::commands::settings_commands::{get_app_settings, AiSettings};
use crate::commands::video_commands::collect_video_frame;
use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::video_people_repo::{self as repo, Run};
use crate::database::repositories::video_repo;
use crate::edicao::ffmpeg::sonda::{self, Capacidades};
use crate::edicao::mensagens::{self, codigo};
use crate::edicao::modelo::{
    BaseRastreio, Caixa, Chave, Confirmacao, Denso, EstadoPessoa, Interp, OrigemRastreio, Problema, ProjetoEdicao, Racional,
    Rastreio, TamanhoPx, TipoRastreio,
};
use crate::edicao::quadros::{self, OrigemTabela, TabelaQuadros};
use crate::edicao::rastreio::{
    self as nucleo, arquivo, corrida, ArquivoDenso, Corrida, EscolhaMotor, MotorRastreio, PedidoCorrida, ResultadoCorrida,
    Sentido, ESTADO_AUSENTE, ESTADO_OK, TIPO_INTERPOLADO, TIPO_MEDIDO,
};
use crate::edicao::{armazem, executar};
use crate::error::{Result, SicroError};
use crate::hashing::sha256::{sha256_bytes, sha256_file};
use crate::models::{CollectFrameInput, VideoMedia, VideoStoryboardFrame};
use crate::video::pessoas::classes::{self, ClasseInfo, CLASSE_MANUAL};
use crate::video::pessoas::mancha::{self, Motivo, OpcoesMancha};
use crate::video::pessoas::pipeline::{self, OpcoesRastreio};
use crate::video::pessoas::render::Opcoes as OpcoesDesenho;
use crate::video::pessoas::{cor_padrao, decode, Amostra, Cadencia, Pessoa};
use crate::workspace::manifest::Manifest;
use crate::workspace::paths::{self, REL_PESSOAS, dir_pessoas, resolver_relativo, sqlite_path};


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
    contexto_em(Path::new(workspace_path), media_hash)
}

fn contexto_em(ws: &Path, media_hash: &str) -> Result<Contexto> {
    let ws = ws.to_path_buf();
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&sqlite_path(&ws))?;
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
        let video = resolver_relativo(&c.ws, &c.media.relative_path);
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
    let video = resolver_relativo(&c.ws, &c.media.relative_path);
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
    let abs = resolver_relativo(&c.ws, &rel);
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
    let dir = dir_pessoas(&c.ws);
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
    let video = resolver_relativo(&c.ws, &c.media.relative_path);
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
        &format!("{REL_PESSOAS}/{nome}"),
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
        let png = resolver_relativo(&c.ws, &col.export.output_path);
        let t_real = col.export.actual_timestamp_s.unwrap_or(t);
        let (p2, png2) = (pessoas.clone(), png.clone());
        let numeros = tauri::async_runtime::spawn_blocking(move || pipeline::sobrepor_png(&png2, t_real, &p2, cad, desenho))
            .await
            .map_err(|e| SicroError::Workspace(format!("sobreposição: {e}")))??;
        let sha = sha256_file(&png)?;
        // sidecar e legenda dizem o que está desenhado e o hash do arquivo FINAL
        if let Some(rel) = &col.export.sidecar_json_path {
            let sp = resolver_relativo(&c.ws, &rel);
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
    let csv = registrar_saida(&c, "pessoas_csv", &format!("{REL_PESSOAS}/{nome_csv}"), &nome_csv, det.clone())?;
    let html = registrar_saida(&c, "pessoas_html", &format!("{REL_PESSOAS}/{nome_html}"), &nome_html, det)?;
    Ok(TabelaGerada { csv, html })
}

// ---------------------------------------------------------------------------
// Mancha marcada à mão

fn motor_auto() -> EscolhaMotor {
    EscolhaMotor::Auto
}

#[derive(Debug, Clone, Deserialize)]
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
    // CONTRATO: acréscimo da abertura da Fase 2 (contratos-fase2 (a), P2.5).
    /// Motor do rastreio (§6.2); ausente = `auto`.
    #[serde(default = "motor_auto")]
    pub motor: EscolhaMotor,
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

/// `app_cache_dir()/edicao/<caso>`: a mesma pasta da Edição (tabela do decodificador em cache).
fn cache_da_edicao(app: &AppHandle, ws: &Path) -> Result<PathBuf> {
    let raiz = app.path().app_cache_dir().map_err(|e| SicroError::Filesystem(format!("cache dir: {e}")))?;
    let c = paths::dir_edicao_cache(&raiz, &Manifest::read(ws)?.occurrence_id);
    std::fs::create_dir_all(&c)?;
    Ok(c)
}

#[tauri::command]
pub async fn pessoas_seguir_mancha(
    app: AppHandle,
    workspace_path: String,
    media_hash: String,
    marca: MarcaMancha,
) -> Result<ResultadoMancha> {
    let ai = get_app_settings(app.clone()).await?.ai;
    let ws = PathBuf::from(&workspace_path);
    let cache = cache_da_edicao(&app, &ws)?;
    let flag = iniciar_tarefa()?;
    let (app2, hash2) = (app.clone(), media_hash.clone());
    let r = tauri::async_runtime::spawn_blocking(move || {
        seguir_mancha_em(&ws, &cache, &ai, &hash2, &marca, &flag, &mut |n| {
            let _ = app2.emit("pessoas-progress", Progresso { media_hash: &hash2, fase: "mancha", feitos: n, total: 0, pessoas: 0 });
        })
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("tarefa de mancha: {e}")));
    encerrar_tarefa();
    r?
}

/// Um sentido seguido, já em pixels do vídeo exibido e em segundos (o que `video_people_tracks` guarda).
struct SaidaSentido {
    amostras: Vec<Amostra>,
    motivo: Motivo,
    parou_em: Option<f64>,
    conf_media: f32,
}

/// Resultado das corridas de uma marca. `corridas` vazio = seguidor antigo.
struct Seguido {
    sentidos: Vec<(bool, SaidaSentido)>,
    largura: u32,
    altura: u32,
    frame_dur_s: f64,
    quadros_total: u32,
    corridas: Vec<Corrida>,
}

fn nao_suportado(e: &SicroError) -> bool {
    matches!(e, SicroError::Validation(m) if m.starts_with(codigo::NAO_SUPORTADO))
}

/// Corpo de `pessoas_seguir_mancha` (sem o `AppHandle`): segue a caixa pelo motor novo (§6.4) e grava a trilha como
/// antes em `video_people_tracks`.
pub(crate) fn seguir_mancha_em(
    ws: &Path,
    cache: &Path,
    ai: &AiSettings,
    media_hash: &str,
    marca: &MarcaMancha,
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(u32),
) -> Result<ResultadoMancha> {
    let c = contexto_em(ws, media_hash)?;
    let video = resolver_relativo(&c.ws, &c.media.relative_path);
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
    let run_atual = repo::get_run(&c.conn, &c.occ, media_hash)?;
    let dims = run_atual.as_ref().map(|r| (r.width, r.height));
    let seguido = match pelo_motor(cache, ai, &video, media_hash, dims, marca, &direcoes, cancelar, progresso) {
        // FECHO: sai com o P2.2 (sem o núcleo do rastreio, o seguidor de antes)
        Err(e) if nao_suportado(&e) => pela_mancha_antiga(&video, marca, &direcoes, cancelar, progresso)?,
        r => r?,
    };

    // união das amostras (o quadro inicial vem nas duas direções)
    let mut novas: Vec<Amostra> = Vec::new();
    let mut avisos = Vec::new();
    let mut conf = 0.0f32;
    for (frente, r) in &seguido.sentidos {
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
    if novas.is_empty() {
        return Err(SicroError::Validation("não foi possível ler o quadro".into()));
    }
    novas.sort_by(|a, b| a.t.total_cmp(&b.t));
    let (tmin, tmax) = (novas[0].t, novas[novas.len() - 1].t);

    let mut todas = repo::list(&c.conn, &c.occ, media_hash)?;
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
                media_hash: media_hash.to_string(),
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
    if run_atual.is_none() {
        let (model, runtime, params) = descricao_da_mancha(seguido.corridas.first());
        let run = Run {
            media_hash: media_hash.to_string(),
            model,
            runtime,
            params_json: params.to_string(),
            width: seguido.largura,
            height: seguido.altura,
            frame_dur_s: seguido.frame_dur_s,
            step: 1,
            frames_total: seguido.quadros_total,
            frames_analyzed: 0,
            elapsed_s: 0.0,
            created_at: Utc::now().to_rfc3339(),
        };
        repo::replace_run(&c.conn, &c.occ, &run)?;
    }
    video_repo::insert_log(
        &c.conn,
        &c.occ,
        Some(media_hash),
        "rastreio.mancha",
        &json!({
            "t": marca.t, "caixa": [marca.x, marca.y, marca.w, marca.h], "direcao": marca.direcao,
            "quadros": novas.len(), "confianca_media": conf, "continuacao": marca.id.is_some(),
            "avisos": avisos.iter().map(|a| json!({"sentido": a.sentido, "motivo": a.motivo, "parou_em": a.parou_em})).collect::<Vec<_>>(),
            "motor": seguido.corridas.first().map(|c| c.motor.motor),
            "corridas": seguido.corridas,
        })
        .to_string(),
    )?;
    Ok(ResultadoMancha { estado: estado(&c.conn, &c.occ, media_hash)?, id, avisos })
}

/// Modelo, runtime e parâmetros do `Run` criado por uma mancha (caso sem análise automática).
fn descricao_da_mancha(c: Option<&Corrida>) -> (String, String, Value) {
    let Some(c) = c else { return ("Mancha manual (correlação)".into(), "SICRO".into(), json!({ "manual": true })) };
    let nome = match c.motor.motor {
        MotorRastreio::Mancha => "correlação",
        MotorRastreio::Vittrack => "VitTrack",
        MotorRastreio::Edgetam => "EdgeTAM",
    };
    let runtime = if c.motor.runtime.is_empty() { "SICRO".to_string() } else { c.motor.runtime.clone() };
    (format!("Mancha manual ({nome})"), runtime, json!({ "manual": true, "motor": c.motor.motor, "provedor": c.motor.provedor }))
}

/// O seguidor de antes (`video::pessoas::mancha`), por busca de entrada em segundos.
fn pela_mancha_antiga(
    video: &Path,
    marca: &MarcaMancha,
    direcoes: &[bool],
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(u32),
) -> Result<Seguido> {
    let times = decode::frame_times(video)?;
    let (dw, dh) = decode::display_size(video)?;
    let t = marca.t;
    let idx0 = times
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - t).abs().total_cmp(&(b.1 - t).abs()))
        .map_or(0, |(i, _)| i);
    let (x0, y0) = (marca.x.clamp(0.0, dw as f32 - 2.0), marca.y.clamp(0.0, dh as f32 - 2.0));
    let caixa = (x0, y0, marca.w.min(dw as f32 - x0), marca.h.min(dh as f32 - y0));
    let mut sentidos = Vec::new();
    for &frente in direcoes {
        let r = mancha::seguir(video, &times, dw, dh, idx0, caixa, frente, OpcoesMancha::default(), cancelar, progresso)?;
        sentidos.push((frente, SaidaSentido { amostras: r.amostras, motivo: r.motivo, parou_em: r.parou_em, conf_media: r.conf_media }));
    }
    Ok(Seguido {
        sentidos,
        largura: dw,
        altura: dh,
        frame_dur_s: decode::frame_dur(&times),
        quadros_total: times.len() as u32,
        corridas: Vec::new(),
    })
}

/// Pelo núcleo do rastreio (§6.4, contratos-fase2 (c) P2.5): motor por `corrida::escolher`, quadros pelo
/// `LeitorPipe` (busca do §5.3 sobre a tabela do decodificador do cache), laço por `rastreio::correr`.
/// `dims` = espaço das trilhas (`run.width/height`); sem análise, o tamanho exibido.
#[allow(clippy::too_many_arguments)]
fn pelo_motor(
    cache: &Path,
    ai: &AiSettings,
    video: &Path,
    media_hash: &str,
    dims: Option<(u32, u32)>,
    marca: &MarcaMancha,
    direcoes: &[bool],
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(u32),
) -> Result<Seguido> {
    let mut primeiro = Some(corrida::escolher(ai, marca.motor)?);
    let cap = sonda::capacidades()?;
    let tabela = tabela_do_decodificador(&cap, cache, media_hash, video, cancelar)?;
    let fonte = quadros::fonte_info(&cap, video, media_hash, tabela.clone(), None)?;
    let dims = match dims {
        Some(d) => d,
        None => decode::display_size(video)?,
    };
    let mut sentidos = Vec::new();
    let mut corridas = Vec::new();
    for &frente in direcoes {
        // um rastreador por corrida: o estado do motor não passa de um sentido ao outro
        let mut r = match primeiro.take() {
            Some(r) => r,
            None => corrida::escolher(ai, marca.motor)?,
        };
        let sentido = if frente { Sentido::Frente } else { Sentido::Tras };
        let p = pedido_da_marca(marca, &tabela, fonte.exibido, dims, media_hash, sentido)?;
        let de_pts = p.ancora.pts.unwrap_or_default();
        let mut feitos = 0u32;
        let res = corrida::LeitorPipe::abrir(&cap, &fonte, de_pts, None, sentido, p.pixel).and_then(|mut leitor| {
            nucleo::correr(&p, &mut leitor, r.as_mut(), cancelar, &mut |a| {
                feitos = a.feitos;
                if a.feitos % 8 == 0 {
                    progresso(a.feitos);
                }
            })
        });
        progresso(feitos);
        // cancelado pelo perito: a tela de sempre ("cancelado" não vira erro)
        let res = match res {
            Ok(r) if r.parou.motivo != nucleo::Motivo::Cancelado && !cancelar.load(Ordering::Relaxed) => r,
            Err(e) if !cancelar.load(Ordering::Relaxed) => return Err(e),
            _ => return Err(SicroError::Validation("cancelado".into())),
        };
        sentidos.push((frente, de_corrida(&res, tabela.tb, dims)));
        corridas.push(res.corrida);
    }
    Ok(Seguido {
        sentidos,
        largura: dims.0,
        altura: dims.1,
        frame_dur_s: segundos(tabela.passo_mediano, tabela.tb),
        quadros_total: tabela.pts.len() as u32,
        corridas,
    })
}

/// A tabela do decodificador (cache da Edição); só com a de pacotes, faz a do decodificador uma vez e guarda.
fn tabela_do_decodificador(cap: &Capacidades, cache: &Path, sha: &str, video: &Path, cancelar: &AtomicBool) -> Result<Arc<TabelaQuadros>> {
    let t = TabelaQuadros::obter_com(cap, cache, sha, video)?;
    if t.origem == OrigemTabela::Decodificador {
        return Ok(t);
    }
    match TabelaQuadros::do_decodificador(cap, video, cancelar, &mut |_| {}) {
        Ok(t) => t.guardar(cache, sha),
        Err(_) if cancelar.load(Ordering::Relaxed) => Err(SicroError::Validation("cancelado".into())),
        Err(e) => Err(e),
    }
}

fn segundos(pts: i64, tb: Racional) -> f64 {
    pts as f64 * tb.num as f64 / tb.den as f64
}

/// Índice do quadro mais perto de `t` (s); None = tabela vazia.
fn quadro_mais_proximo(tab: &TabelaQuadros, t: f64) -> Option<usize> {
    let n = tab.pts.len();
    if n == 0 {
        return None;
    }
    let k = tab.pts.partition_point(|&p| segundos(p, tab.tb) < t);
    let candidatos = [k.saturating_sub(1), k.min(n - 1)];
    candidatos.into_iter().min_by(|&a, &b| (tab.tempo(a) - t).abs().total_cmp(&(tab.tempo(b) - t).abs()))
}

/// Pedido da corrida a partir da marca da análise: âncora no quadro mais perto de `marca.t` (a regra de antes), caixa
/// presa à imagem e passada a 0..1 pelo espaço das trilhas.
fn pedido_da_marca(
    marca: &MarcaMancha,
    tab: &TabelaQuadros,
    exibido: (u32, u32),
    dims: (u32, u32),
    media_hash: &str,
    sentido: Sentido,
) -> Result<PedidoCorrida> {
    let j = quadro_mais_proximo(tab, marca.t).ok_or_else(|| SicroError::Validation("instante fora do vídeo".into()))?;
    if dims.0 < 2 || dims.1 < 2 {
        return Err(SicroError::Validation("dimensões do vídeo ilegíveis".into()));
    }
    let (rw, rh) = (dims.0 as f64, dims.1 as f64);
    let (x0, y0) = ((marca.x as f64).clamp(0.0, rw - 2.0), (marca.y as f64).clamp(0.0, rh - 2.0));
    let (w, h) = ((marca.w as f64).min(rw - x0), (marca.h as f64).min(rh - y0));
    Ok(PedidoCorrida {
        rastreio: marca.id.clone().unwrap_or_else(|| "mancha".into()),
        midia_sha256: media_hash.to_string(),
        espaco: "original".into(),
        pixel: TamanhoPx { w: exibido.0, h: exibido.1 },
        ancora: Chave { t: tab.tempo(j), pts: Some(tab.pts[j]), v: Caixa { x: x0 / rw, y: y0 / rh, w: w / rw, h: h / rh }, interp: Interp::Linear },
        sentido,
        ate_pts: None,
        max_quadros: OpcoesMancha::default().max_quadros,
        fps: if tab.passo_mediano > 0 { 1.0 / segundos(tab.passo_mediano, tab.tb) } else { 30.0 },
    })
}

/// Corrida → amostras da análise (px de `dims`, tempo = pts × tb, em ordem de tempo; quadro sem caixa fica de fora)
/// e o aviso no formato de antes.
fn de_corrida(res: &ResultadoCorrida, tb: Racional, dims: (u32, u32)) -> SaidaSentido {
    let (rw, rh) = (dims.0 as f64, dims.1 as f64);
    let mut amostras: Vec<Amostra> = res
        .amostras
        .iter()
        .filter_map(|a| {
            let c = a.caixa?;
            Some(Amostra { t: segundos(a.pts, tb), x: (c.x * rw) as f32, y: (c.y * rh) as f32, w: (c.w * rw) as f32, h: (c.h * rh) as f32 })
        })
        .collect();
    amostras.sort_by(|a, b| a.t.total_cmp(&b.t));
    // a âncora (primeira da corrida) não entra na média, como no seguidor de antes
    let confs: Vec<f64> = res.amostras.iter().skip(1).filter(|a| a.caixa.is_some()).map(|a| a.conf).collect();
    let conf_media = if confs.is_empty() { 0.0 } else { (confs.iter().sum::<f64>() / confs.len() as f64) as f32 };
    let (motivo, perdeu) = match res.parou.motivo {
        nucleo::Motivo::Fim | nucleo::Motivo::Ancora | nucleo::Motivo::Cancelado => (Motivo::FimDoVideo, false),
        nucleo::Motivo::Perdido => (Motivo::Confianca, true),
        nucleo::Motivo::SaiuDoQuadro => (Motivo::SaiuDoQuadro, true),
        nucleo::Motivo::Limite => (Motivo::Limite, false),
    };
    let parou_em = if perdeu { res.parou.pts.map(|p| segundos(p, tb)) } else { None };
    SaidaSentido { amostras, motivo, parou_em, conf_media }
}

// ---------------------------------------------------------------------------
// "Da análise" e "Atualizar" (§6.4; contratos-fase2 (c) P2.5)

/// Quando a trilha mudou pela última vez (`updated_at`); None = não existe.
fn atualizada_em(conn: &Connection, occ: &Uuid, id: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT updated_at FROM video_people_tracks WHERE id = ?1 AND occurrence_id = ?2",
            params![id, occ.to_string()],
            |r| r.get::<_, String>(0),
        )
        .optional()?)
}

/// Trilha e quando mudou pela última vez (`updated_at`).
fn trilha(conn: &Connection, occ: &Uuid, id: &str) -> Result<Option<(Pessoa, String)>> {
    Ok(conn
        .query_row(
            "SELECT id, media_hash, numero, nome, cor, estado, amostras_json, ajustes_json, classe, origem, updated_at
             FROM video_people_tracks WHERE id = ?1 AND occurrence_id = ?2",
            params![id, occ.to_string()],
            |r| {
                let amostras: String = r.get(6)?;
                let ajustes: String = r.get(7)?;
                Ok((
                    Pessoa {
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
                    },
                    r.get::<_, String>(10)?,
                ))
            },
        )
        .optional()?)
}

fn revisada(p: &Pessoa) -> bool {
    p.estado == "editado"
}

/// Impressão da trilha copiada (caixas efetivas e revisão): `trilha_sha256` na corrida do denso. Renomear, trocar a
/// cor ou o número não muda.
fn assinatura_da_trilha(p: &Pessoa) -> String {
    let caixas: Vec<[f64; 5]> = p.caixas().iter().map(|a| [a.t, a.x as f64, a.y as f64, a.w as f64, a.h as f64]).collect();
    sha256_bytes(&serde_json::to_vec(&json!({ "revisada": revisada(p), "caixas": caixas })).unwrap_or_default())
}

fn sha_valido(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn cor_valida(c: &str) -> bool {
    matches!(c.len(), 7 | 9) && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// O denso da trilha (§6.1), sem disco: uma linha por quadro da tabela do primeiro ao último quadro com amostra.
/// Amostra casada com o quadro mais perto (até meio quadro) = medida, com a caixa efetiva (correções do perito);
/// entre amostras, a caixa que a análise mostra nesse quadro (`Pessoa::caixa_em`: segura o quadro e, com passo > 1,
/// interpola) = interpolada; sem caixa = ausente. Coordenadas divididas por `run.width/height` (§3.2).
pub(crate) fn denso_da_pessoa(p: &Pessoa, run: &Run, tab: &TabelaQuadros, rastreio_id: &str, midia_sha256: &str) -> Result<ArquivoDenso> {
    let ausente = || SicroError::Validation(codigo::PESSOA_AUSENTE.into());
    if run.width == 0 || run.height == 0 {
        return Err(ausente());
    }
    let (rw, rh) = (run.width as f64, run.height as f64);
    // amostra → quadro; no mesmo quadro fica a mais perto dele
    let mut medidas: std::collections::BTreeMap<usize, (f64, Amostra)> = Default::default();
    for a in p.caixas() {
        let Some(j) = quadro_mais_proximo(tab, a.t) else { continue };
        let d = (tab.tempo(j) - a.t).abs();
        if d > 0.5 * tab.duracao(j).max(segundos(tab.passo_mediano, tab.tb)) {
            continue;
        }
        if medidas.get(&j).is_none_or(|(d0, _)| d < *d0) {
            medidas.insert(j, (d, a));
        }
    }
    let (Some(&j0), Some(&j1)) = (medidas.keys().next(), medidas.keys().next_back()) else { return Err(ausente()) };
    let cad = cadencia(run);
    let n = j1 - j0 + 1;
    let mut d = ArquivoDenso {
        esquema: arquivo::ESQUEMA_DENSO,
        rastreio: rastreio_id.to_string(),
        midia_sha256: midia_sha256.to_string(),
        espaco: "original".into(),
        pixel: TamanhoPx { w: run.width, h: run.height },
        pts: Vec::with_capacity(n),
        x: Vec::with_capacity(n),
        y: Vec::with_capacity(n),
        w: Vec::with_capacity(n),
        h: Vec::with_capacity(n),
        conf: Vec::with_capacity(n),
        estado: Vec::with_capacity(n),
        tipo: Vec::with_capacity(n),
        corrida: vec![0; n],
        corridas: vec![json!({
            "motor": "pessoas",
            "pessoa_id": p.id,
            "numero": p.numero,
            "classe": p.classe,
            "origem": p.origem,
            "estado": if revisada(p) { "revisada" } else { "auto" },
            "trilha_sha256": assinatura_da_trilha(p),
            "analise": {
                "modelo": run.model,
                "runtime": run.runtime,
                "parametros": serde_json::from_str::<Value>(&run.params_json).unwrap_or_else(|_| Value::String(run.params_json.clone())),
                "passo": run.step,
                "pixel": { "w": run.width, "h": run.height },
                "feita_em": run.created_at,
            },
        })],
    };
    for j in j0..=j1 {
        let (caixa, tipo) = match medidas.get(&j) {
            Some((_, a)) => (Some(*a), TIPO_MEDIDO),
            None => (p.caixa_em(tab.tempo(j), cad), TIPO_INTERPOLADO),
        };
        d.pts.push(tab.pts[j]);
        match caixa {
            Some(a) => {
                d.x.push(a.x as f64 / rw);
                d.y.push(a.y as f64 / rh);
                d.w.push(a.w as f64 / rw);
                d.h.push(a.h as f64 / rh);
                d.conf.push(1.0);
                d.estado.push(ESTADO_OK);
                d.tipo.push(tipo);
            }
            None => {
                for c in [&mut d.x, &mut d.y, &mut d.w, &mut d.h, &mut d.conf] {
                    c.push(0.0);
                }
                d.estado.push(ESTADO_AUSENTE);
                d.tipo.push(TIPO_MEDIDO);
            }
        }
    }
    Ok(d)
}

/// O `Rastreio` da trilha com o ponteiro do denso já gravado. Automática entra sem confirmação; revisada pelo perito na
/// análise, confirmada no conteúdo copiado (`denso_sha256`) e no intervalo dele (§6.4, §6.6). `revisada_em` =
/// `updated_at` da trilha; `por` fica vazio (a análise não guarda quem revisou).
pub(crate) fn rastreio_de(p: &Pessoa, rastreio_id: &str, midia_id: &str, denso: Denso, copiado_em: &str, revisada_em: &str) -> Rastreio {
    let estado = if revisada(p) { EstadoPessoa::Revisada } else { EstadoPessoa::Auto };
    let confirmacao = (estado == EstadoPessoa::Revisada).then(|| Confirmacao {
        por: String::new(),
        em: revisada_em.to_string(),
        denso_sha256: Some(denso.sha256.clone()),
        de_s: denso.de_s,
        ate_s: denso.ate_s,
    });
    let rotulo = if p.nome.trim().is_empty() { p.rotulo() } else { p.nome.trim().to_string() };
    Rastreio {
        id: rastreio_id.to_string(),
        nome: format!("{} · {}", p.numero, rotulo),
        cor: if cor_valida(&p.cor) { p.cor.clone() } else { cor_padrao(p.numero) },
        midia: Some(midia_id.to_string()),
        base: BaseRastreio::Fonte,
        tipo: TipoRastreio::Caixa,
        espaco: "original".into(),
        ancoras: Vec::new(),
        denso: Some(denso),
        silhuetas: None,
        ausencias: Vec::new(),
        origem: OrigemRastreio::Pessoas { pessoa_id: p.id.clone(), estado, copiado_em: copiado_em.to_string() },
        confirmacao,
        ancoras_do_programa: Vec::new(),
    }
}

/// O que "Da análise" lê do caso: a trilha (com `updated_at`), a análise dela e a tabela de quadros da mídia.
struct DaAnalise {
    pessoa: Pessoa,
    atualizada_em: String,
    run: Run,
    tabela: Arc<TabelaQuadros>,
}

fn ler_da_analise(ws: &Path, cache: &Path, media_sha256: &str, pessoa_id: &str) -> Result<DaAnalise> {
    let ausente = || SicroError::Validation(codigo::PESSOA_AUSENTE.into());
    let midia = || SicroError::Validation(codigo::MIDIA_AUSENTE.into());
    let (conn, occ) = executar::abrir_banco(ws)?;
    let media = video_repo::find_media_by_sha256(&conn, &occ, media_sha256)?.ok_or_else(midia)?;
    let (pessoa, atualizada_em) = trilha(&conn, &occ, pessoa_id)?.filter(|(p, _)| p.media_hash == media_sha256).ok_or_else(ausente)?;
    let run = repo::get_run(&conn, &occ, media_sha256)?.ok_or_else(ausente)?;
    let video = resolver_relativo(ws, &media.relative_path);
    if !video.is_file() {
        return Err(midia());
    }
    // a do decodificador: quadro que só os pacotes têm (TS cortado, gravador) não vira linha
    let tabela = tabela_do_decodificador(&*sonda::capacidades()?, cache, media_sha256, &video, &AtomicBool::new(false))?;
    Ok(DaAnalise { pessoa, atualizada_em, run, tabela })
}

/// "Da análise" (§6.4): a trilha `pessoa_id` da mídia, com as correções do perito, vira `Rastreio { origem: pessoas }`
/// (divide por `run.width/height`), com o denso gravado em `rastreios/<rid>/` do projeto e `midia = midia_id`.
/// Trilha `auto` entra sem confirmação; a revisada, confirmada. Trilha inexistente: `pessoa_ausente`.
/// `cache` = `app_cache_dir()/edicao/<caso>`: a tabela do decodificador (`TabelaQuadros::obter`) dá o pts de cada
/// quadro do denso e a base de tempo de `arquivo::gravar` (as trilhas guardam só `t`).
pub(crate) fn rastreio_da_pessoa(
    ws: &Path,
    cache: &Path,
    projeto_id: &str,
    midia_id: &str,
    media_sha256: &str,
    pessoa_id: &str,
) -> Result<Rastreio> {
    if !armazem::id_valido(projeto_id) || !sha_valido(media_sha256) {
        return Err(SicroError::Validation(codigo::ITEM_INVALIDO.into()));
    }
    // antes de ler: mudança gravada durante a cópia fica depois de `copiado_em`
    let copiado_em = Utc::now().to_rfc3339();
    let a = ler_da_analise(ws, cache, media_sha256, pessoa_id)?;
    let rid = Uuid::new_v4().to_string();
    let d = denso_da_pessoa(&a.pessoa, &a.run, &a.tabela, &rid, media_sha256)?;
    let ponteiro = arquivo::gravar(&armazem::pasta_projeto(ws, projeto_id), &d, a.tabela.tb)?;
    Ok(rastreio_de(&a.pessoa, &rid, midia_id, ponteiro, &copiado_em, &a.atualizada_em))
}

/// `a` depois de `b` (RFC 3339); data ilegível conta como depois.
fn depois(a: &str, b: &str) -> bool {
    match (chrono::DateTime::parse_from_rfc3339(a), chrono::DateTime::parse_from_rfc3339(b)) {
        (Ok(a), Ok(b)) => a > b,
        _ => true,
    }
}

/// A trilha de agora é a que foi copiada: `trilha_sha256` do denso igual ao de agora. Denso ilegível = não se sabe.
fn mesma_trilha(ws: &Path, projeto_id: &str, r: &Rastreio, p: &Pessoa) -> bool {
    let Some(d) = r.denso.as_ref() else { return false };
    let Ok(a) = arquivo::ler(&armazem::pasta_projeto(ws, projeto_id), &r.id, &d.sha256) else { return false };
    a.corridas.first().and_then(|c| c.get("trilha_sha256")).and_then(Value::as_str) == Some(assinatura_da_trilha(p).as_str())
}

#[cfg(test)]
thread_local! {
    /// Aberturas do banco por `desatualizados` (teste de "sai sem abrir o banco").
    static ABERTURAS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// `retrato_desatualizado` dos rastreios vindos de Pessoas cuja trilha mudou depois de `copiado_em` (§6.4,
/// "Atualizar"); `edicao_analisar` acrescenta à `Analise`. Mudou = a trilha sumiu, ou foi gravada depois da cópia
/// com outras caixas ou outra revisão (renomear não conta, quando o denso dá a impressão da cópia). Um por rastreio:
/// `midia` = a do rastreio, `campo` = o id do rastreio. Sem rastreio de Pessoas, sai sem abrir o banco; a trilha
/// inteira só é lida quando a data diz que mudou.
pub(crate) fn desatualizados(ws: &Path, p: &ProjetoEdicao) -> Vec<Problema> {
    let copias: Vec<(&Rastreio, &str, &str)> = p
        .rastreios
        .iter()
        .filter_map(|r| match &r.origem {
            OrigemRastreio::Pessoas { pessoa_id, copiado_em, .. } => Some((r, pessoa_id.as_str(), copiado_em.as_str())),
            _ => None,
        })
        .collect();
    if copias.is_empty() {
        return Vec::new();
    }
    #[cfg(test)]
    ABERTURAS.with(|n| n.set(n.get() + 1));
    let Ok((conn, occ)) = executar::abrir_banco(ws) else { return Vec::new() };
    let mut v: Vec<Problema> = Vec::new();
    for (r, pessoa_id, copiado_em) in copias {
        let mudou = match atualizada_em(&conn, &occ, pessoa_id) {
            Ok(Some(em)) if depois(&em, copiado_em) => match trilha(&conn, &occ, pessoa_id) {
                Ok(Some((atual, _))) => !mesma_trilha(ws, &p.id, r, &atual),
                Ok(None) => true,
                Err(_) => false,
            },
            Ok(Some(_)) => false,
            Ok(None) => true,
            Err(_) => false,
        };
        if mudou {
            let mut pr = mensagens::problema(codigo::RETRATO_DESATUALIZADO, None, None);
            pr.midia = r.midia.clone();
            pr.campo = Some(r.id.clone());
            v.push(pr);
        }
    }
    v
}

#[cfg(test)]
mod testes {
    use std::cell::Cell;
    use std::time::Duration;

    use super::*;
    use crate::edicao::modelo::Nivel;
    use crate::edicao::quadros::apoio;
    use crate::edicao::rastreio::{AmostraCorrida, Limiares, ModeloCorrida, MotorCorrida, Parou};
    use crate::edicao::testes::aceite;
    use crate::edicao::testes::fontes::{self, Fonte};
    use crate::edicao::testes::medir;
    use crate::video::pessoas::Ajuste;

    fn sha_falso() -> String {
        "5a".repeat(32)
    }

    fn run(w: u32, h: u32, passo: u32, frame_dur: f64) -> Run {
        Run {
            media_hash: "h".into(),
            model: "YOLOX-S (Apache-2.0)".into(),
            runtime: "onnxruntime 1.22.0".into(),
            params_json: json!({ "limiar": 0.5, "passo": passo }).to_string(),
            width: w,
            height: h,
            frame_dur_s: frame_dur,
            step: passo,
            frames_total: 0,
            frames_analyzed: 0,
            elapsed_s: 0.0,
            created_at: "2026-11-15T10:00:00+00:00".into(),
        }
    }

    fn pessoa(id: &str, media: &str, estado: &str, amostras: Vec<Amostra>) -> Pessoa {
        Pessoa {
            id: id.into(),
            media_hash: media.into(),
            numero: 3,
            nome: String::new(),
            cor: cor_padrao(3),
            estado: estado.into(),
            classe: 0,
            origem: "auto".into(),
            amostras,
            ajustes: vec![],
        }
    }

    /// Tempo como o ffprobe o imprime (6 casas).
    fn t6(t: f64) -> f64 {
        (t * 1e6).round() / 1e6
    }

    /// TS 30 qps a partir de 3 s, base 1/90000.
    fn tabela_ts() -> TabelaQuadros {
        apoio::tabela((1, 90000), apoio::pts_regulares(40, (1, 30), (1, 90000), 270_000))
    }

    fn denso_falso() -> Denso {
        Denso { sha256: "c3".repeat(32), n: 13, de_s: 3.133, ate_s: 3.533, ok: 8, incertos: 0, ausentes: 5 }
    }

    // ---------------------------------------------------------------- "Da análise" (puro)

    #[test]
    fn denso_da_pessoa_com_correcoes() {
        let tab = tabela_ts();
        let am = |j: usize| Amostra { t: t6(tab.tempo(j)), x: 10.0 * j as f32, y: 20.0, w: 40.0, h: 100.0 };
        // amostra depois do fim da mídia (mais de meio quadro): fica de fora
        let fora = Amostra { t: tab.tempo(39) + 2.0 / 30.0, x: 1.0, y: 1.0, w: 9.0, h: 9.0 };
        let mut p = pessoa("pa", "h", "editado", vec![am(4), am(6), am(8), am(16), fora]);
        // correção do perito no quadro 6: vale dali em diante
        p.ajustes = vec![Ajuste { t: p.amostras[1].t, dx: 8.0, dy: -4.0, dw: 2.0, dh: 0.0 }];
        // passo 2 e quadro de 0,0333 s: a análise segura ou interpola até 0,0999 s depois da amostra
        let d = denso_da_pessoa(&p, &run(640, 360, 2, 0.0333), &tab, "r1", &sha_falso()).unwrap();

        const A: Option<[f64; 4]> = None;
        let esperado: Vec<(usize, u8, Option<[f64; 4]>)> = vec![
            (4, TIPO_MEDIDO, Some([40.0, 20.0, 40.0, 100.0])),
            (5, TIPO_INTERPOLADO, Some([54.0, 18.0, 41.0, 100.0])),
            (6, TIPO_MEDIDO, Some([68.0, 16.0, 42.0, 100.0])),
            (7, TIPO_INTERPOLADO, Some([78.0, 16.0, 42.0, 100.0])),
            (8, TIPO_MEDIDO, Some([88.0, 16.0, 42.0, 100.0])),
            (9, TIPO_INTERPOLADO, Some([88.0, 16.0, 42.0, 100.0])),
            (10, TIPO_INTERPOLADO, Some([88.0, 16.0, 42.0, 100.0])),
            (11, TIPO_MEDIDO, A),
            (12, TIPO_MEDIDO, A),
            (13, TIPO_MEDIDO, A),
            (14, TIPO_MEDIDO, A),
            (15, TIPO_MEDIDO, A),
            (16, TIPO_MEDIDO, Some([168.0, 16.0, 42.0, 100.0])),
        ];
        assert_eq!(d.pts, esperado.iter().map(|e| tab.pts[e.0]).collect::<Vec<_>>());
        assert_eq!(d.tipo, esperado.iter().map(|e| e.1).collect::<Vec<_>>());
        assert_eq!(d.estado, esperado.iter().map(|e| if e.2.is_some() { ESTADO_OK } else { ESTADO_AUSENTE }).collect::<Vec<_>>());
        assert_eq!(d.conf, esperado.iter().map(|e| if e.2.is_some() { 1.0 } else { 0.0 }).collect::<Vec<_>>());
        for (i, (j, _, c)) in esperado.iter().enumerate() {
            let c = c.unwrap_or([0.0; 4]);
            let lido = [d.x[i] * 640.0, d.y[i] * 360.0, d.w[i] * 640.0, d.h[i] * 360.0];
            for k in 0..4 {
                assert!((lido[k] - c[k]).abs() < 1e-3, "quadro {j}, coluna {k}: {} ≠ {}", lido[k], c[k]);
            }
        }
        assert_eq!(d.corrida, vec![0; esperado.len()]);
        assert_eq!((d.esquema, d.rastreio.as_str(), d.midia_sha256.as_str(), d.espaco.as_str()), (arquivo::ESQUEMA_DENSO, "r1", sha_falso().as_str(), "original"));
        assert_eq!(d.pixel, TamanhoPx { w: 640, h: 360 });
        let c = &d.corridas[0];
        assert_eq!((c["motor"].as_str(), c["pessoa_id"].as_str(), c["estado"].as_str()), (Some("pessoas"), Some("pa"), Some("revisada")));
        assert_eq!(c["trilha_sha256"].as_str(), Some(assinatura_da_trilha(&p).as_str()));
        assert_eq!(c["analise"]["parametros"]["passo"], json!(2));
        assert_eq!(d.corridas.len(), 1);
    }

    /// Renomear, trocar a cor ou o número não muda a impressão; caixa ou revisão mudam.
    #[test]
    fn impressao_da_trilha_so_pelas_caixas_e_revisao() {
        let p = pessoa("pa", "h", "auto", vec![Amostra { t: 1.0, x: 1.0, y: 2.0, w: 30.0, h: 40.0 }]);
        let base = assinatura_da_trilha(&p);
        let mut q = p.clone();
        (q.nome, q.cor, q.numero) = ("Outro".into(), "#000000".into(), 9);
        assert_eq!(assinatura_da_trilha(&q), base);
        q.estado = "editado".into();
        assert_ne!(assinatura_da_trilha(&q), base);
        let mut r = p.clone();
        r.ajustes = vec![Ajuste { t: 1.0, dx: 0.5, dy: 0.0, dw: 0.0, dh: 0.0 }];
        assert_ne!(assinatura_da_trilha(&r), base);
    }

    #[test]
    fn automatica_sem_confirmacao() {
        for origem in ["auto", "manual"] {
            let mut p = pessoa("pa", "h", "auto", vec![]);
            p.origem = origem.into();
            let r = rastreio_de(&p, "r1", "m1", denso_falso(), "2026-11-15T12:00:00+00:00", "2026-11-15T11:00:00+00:00");
            assert_eq!(r.confirmacao, None);
            assert_eq!(
                r.origem,
                OrigemRastreio::Pessoas { pessoa_id: "pa".into(), estado: EstadoPessoa::Auto, copiado_em: "2026-11-15T12:00:00+00:00".into() }
            );
            assert_eq!((r.id.as_str(), r.midia.as_deref(), r.espaco.as_str(), r.nome.as_str()), ("r1", Some("m1"), "original", "3 · pessoa"));
            assert_eq!((r.base, r.tipo), (BaseRastreio::Fonte, TipoRastreio::Caixa));
            assert_eq!(r.denso, Some(denso_falso()));
            assert!(r.ancoras.is_empty() && r.ausencias.is_empty() && r.silhuetas.is_none());
            // o documento da tela lê o mesmo rastreio
            let v = serde_json::to_value(&r).unwrap();
            assert_eq!(serde_json::from_value::<Rastreio>(v.clone()).unwrap(), r);
            assert_eq!(v["confirmacao"], Value::Null);
        }
    }

    #[test]
    fn revisada_confirmada() {
        let mut p = pessoa("pa", "h", "editado", vec![]);
        (p.nome, p.cor) = ("Suspeito".into(), "vermelho".into());
        let d = denso_falso();
        let r = rastreio_de(&p, "r1", "m1", d.clone(), "2026-11-15T12:00:00+00:00", "2026-11-15T11:00:00+00:00");
        assert_eq!(
            r.confirmacao,
            Some(Confirmacao { por: String::new(), em: "2026-11-15T11:00:00+00:00".into(), denso_sha256: Some(d.sha256.clone()), de_s: d.de_s, ate_s: d.ate_s })
        );
        assert!(matches!(r.origem, OrigemRastreio::Pessoas { estado: EstadoPessoa::Revisada, .. }));
        assert_eq!((r.nome.as_str(), r.cor.as_str()), ("3 · Suspeito", cor_padrao(3).as_str()));
    }

    // ---------------------------------------------------------------- caso de verdade (banco)

    fn caso() -> (tempfile::TempDir, PathBuf, Uuid) {
        let d = tempfile::Builder::new().prefix("p2.5-pessoas-").tempdir().unwrap();
        let pai = d.path().join("Casos ç");
        std::fs::create_dir_all(&pai).unwrap();
        let ws = crate::workspace::create::create_workspace(crate::models::NewOccurrenceInput::default(), &pai).unwrap().path;
        let occ = Manifest::read(&ws).unwrap().occurrence_id;
        (d, ws, occ)
    }

    fn banco(ws: &Path) -> Connection {
        open_connection(&sqlite_path(ws)).unwrap()
    }

    /// Mídia registrada no caso: `src` copiado para dentro dele; devolve o caminho e o SHA-256.
    fn midia_no_caso(ws: &Path, occ: &Uuid, src: &Path, nome: &str) -> (PathBuf, String) {
        let rel = format!("Mídias/Vídeos/{nome}");
        let dest = resolver_relativo(ws, &rel);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::copy(src, &dest).unwrap();
        let sha = sha256_file(&dest).unwrap();
        banco(ws)
            .execute(
                "INSERT INTO video_media (id, occurrence_id, relative_path, filename, sha256, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![Uuid::new_v4().to_string(), occ.to_string(), rel, nome, sha, Utc::now().to_rfc3339()],
            )
            .unwrap();
        (dest, sha)
    }

    fn projeto(rastreios: Vec<Value>) -> ProjetoEdicao {
        aceite::projeto(320, 180, (30, 1), vec![aceite::midia("m1", "video", json!([]))], vec![], json!({ "rastreios": rastreios }))
    }

    fn aberturas() -> u32 {
        ABERTURAS.with(Cell::get)
    }

    #[test]
    fn desatualizado_quando_a_trilha_muda() {
        let (_d, ws, occ) = caso();
        let conn = banco(&ws);
        let mut a = pessoa("pa", "h", "auto", vec![Amostra { t: 1.0, x: 10.0, y: 20.0, w: 30.0, h: 40.0 }, Amostra { t: 1.1, x: 12.0, y: 20.0, w: 30.0, h: 40.0 }]);
        repo::insert(&conn, &occ, &a).unwrap();
        std::thread::sleep(Duration::from_millis(5));
        let copiado = rastreio_de(&a, "r-pessoas", "m1", denso_falso(), &Utc::now().to_rfc3339(), "");
        let manual = aceite::rastreio("r-manual", Some("m1"), "fonte", vec![]);
        let p = projeto(vec![serde_json::to_value(&copiado).unwrap(), manual]);
        assert_eq!(desatualizados(&ws, &p), vec![]);

        // o perito corrige a caixa na análise depois da cópia
        std::thread::sleep(Duration::from_millis(5));
        assert!(a.ajustar(1.1, 20.0, 20.0, 30.0, 40.0));
        repo::update(&conn, &a).unwrap();
        let d = desatualizados(&ws, &p);
        let resumo: Vec<_> = d.iter().map(|x| (x.codigo, x.nivel, x.clipe.as_deref(), x.efeito.as_deref(), x.midia.as_deref(), x.campo.as_deref())).collect();
        assert_eq!(resumo, vec![(codigo::RETRATO_DESATUALIZADO, Nivel::Aviso, None, None, Some("m1"), Some("r-pessoas"))]);
        assert_eq!(d[0].mensagem, mensagens::frase(codigo::RETRATO_DESATUALIZADO, None, None));

        // copiado de novo depois da correção: em dia
        std::thread::sleep(Duration::from_millis(5));
        let de_novo = rastreio_de(&a, "r-2", "m1", denso_falso(), &Utc::now().to_rfc3339(), "");
        assert_eq!(desatualizados(&ws, &projeto(vec![serde_json::to_value(&de_novo).unwrap()])), vec![]);

        // a trilha some da análise (apagada, ou "Rastrear de novo"): desatualizado
        repo::delete(&conn, "pa").unwrap();
        let d = desatualizados(&ws, &projeto(vec![serde_json::to_value(&de_novo).unwrap()]));
        assert_eq!(d.iter().map(|x| x.campo.as_deref()).collect::<Vec<_>>(), vec![Some("r-2")]);
    }

    #[test]
    fn sem_rastreio_de_pessoas_nao_abre_o_banco() {
        let (_d, ws, _) = caso();
        let antes = aberturas();
        assert_eq!(desatualizados(&ws, &projeto(vec![aceite::rastreio("r-manual", Some("m1"), "fonte", vec![])])), vec![]);
        assert_eq!(aberturas(), antes);
        let sumida = rastreio_de(&pessoa("nao-existe", "h", "auto", vec![]), "r1", "m1", denso_falso(), "2026-11-15T12:00:00+00:00", "");
        assert_eq!(desatualizados(&ws, &projeto(vec![serde_json::to_value(&sumida).unwrap()])).len(), 1);
        assert_eq!(aberturas(), antes + 1);
    }

    #[test]
    fn da_analise_recusa_o_que_nao_existe() {
        let (d, ws, occ) = caso();
        let cache = d.path().join("cache");
        let codigo_de = |r: Result<Rastreio>| match r {
            Err(SicroError::Validation(m)) => m,
            outro => panic!("{outro:?}"),
        };
        let sha_ok = sha_falso();
        assert_eq!(codigo_de(rastreio_da_pessoa(&ws, &cache, "../fora", "m1", &sha_ok, "pa")), codigo::ITEM_INVALIDO);
        assert_eq!(codigo_de(rastreio_da_pessoa(&ws, &cache, "p1", "m1", "../../x", "pa")), codigo::ITEM_INVALIDO);
        assert_eq!(codigo_de(rastreio_da_pessoa(&ws, &cache, "p1", "m1", &sha_ok.to_uppercase(), "pa")), codigo::ITEM_INVALIDO);
        assert_eq!(codigo_de(rastreio_da_pessoa(&ws, &cache, "p1", "m1", &sha_ok, "pa")), codigo::MIDIA_AUSENTE);
        // mídia registrada, mas o arquivo não está no caso
        let falso = d.path().join("v.mp4");
        std::fs::write(&falso, b"nada").unwrap();
        let (dest, sha) = midia_no_caso(&ws, &occ, &falso, "v.mp4");
        assert_eq!(codigo_de(rastreio_da_pessoa(&ws, &cache, "p1", "m1", &sha, "pa")), codigo::PESSOA_AUSENTE);
        let conn = banco(&ws);
        // trilha de outra mídia
        repo::insert(&conn, &occ, &pessoa("pb", "outra", "auto", vec![Amostra { t: 0.0, x: 1.0, y: 1.0, w: 9.0, h: 9.0 }])).unwrap();
        assert_eq!(codigo_de(rastreio_da_pessoa(&ws, &cache, "p1", "m1", &sha, "pb")), codigo::PESSOA_AUSENTE);
        // trilha sem a análise (run)
        repo::insert(&conn, &occ, &pessoa("pa", &sha, "auto", vec![Amostra { t: 0.0, x: 1.0, y: 1.0, w: 9.0, h: 9.0 }])).unwrap();
        assert_eq!(codigo_de(rastreio_da_pessoa(&ws, &cache, "p1", "m1", &sha, "pa")), codigo::PESSOA_AUSENTE);
        repo::replace_run(&conn, &occ, &Run { media_hash: sha.clone(), ..run(320, 180, 1, 1.0 / 30.0) }).unwrap();
        std::fs::remove_file(&dest).unwrap();
        assert_eq!(codigo_de(rastreio_da_pessoa(&ws, &cache, "p1", "m1", &sha, "pa")), codigo::MIDIA_AUSENTE);
    }

    /// A trilha do caso pelo leitor da Edição: a tabela é a do decodificador (o TS cortado não entrega os quadros
    /// antes do quadro-chave seguinte; os pacotes, sim) e cada linha medida é a amostra da análise naquele quadro.
    #[test]
    fn da_analise_le_a_trilha_do_caso() {
        let Some(cap) = fontes::cap() else { return };
        for f in [Fonte::TsMeioDoGop, Fonte::Ts5994] {
            let (d, ws, occ) = caso();
            let src = fontes::gerar(f).expect("fonte");
            let (video, sha) = midia_no_caso(&ws, &occ, &src, "camera.ts");
            let pacotes = decode::frame_times(&video).unwrap();
            let pts = medir::pts(&cap, &video).unwrap();
            // uma amostra a cada dois pacotes, x = índice do pacote
            let amostras: Vec<Amostra> =
                pacotes.iter().enumerate().step_by(2).map(|(i, &t)| Amostra { t, x: i as f32, y: 10.0, w: 30.0, h: 40.0 }).collect();
            let conn = banco(&ws);
            repo::insert(&conn, &occ, &pessoa("pa", &sha, "auto", amostras)).unwrap();
            repo::replace_run(&conn, &occ, &Run { media_hash: sha.clone(), ..run(320, 180, 2, decode::frame_dur(&pacotes)) }).unwrap();
            let a = ler_da_analise(&ws, &d.path().join("cache"), &sha, "pa").unwrap();
            assert_eq!(a.tabela.origem, OrigemTabela::Decodificador, "{f:?}");
            assert_eq!(a.tabela.pts, pts, "{f:?}: tabela ≠ pts do decodificador");
            let den = denso_da_pessoa(&a.pessoa, &a.run, &a.tabela, "r1", &sha).unwrap();
            // linhas = quadros decodificados seguidos, do primeiro ao último com amostra
            let k0 = pts.iter().position(|&p| p == den.pts[0]).expect("primeira linha fora do decodificado");
            assert_eq!(den.pts, pts[k0..k0 + den.pts.len()], "{f:?}");
            let tb = a.tabela.tb;
            let mut medidas = 0;
            for (i, &p) in den.pts.iter().enumerate() {
                let t = segundos(p, tb);
                let par = pacotes.iter().position(|&s| (s - t).abs() < 2e-6).expect("quadro sem pacote");
                if par % 2 == 0 {
                    assert_eq!((den.tipo[i], den.estado[i]), (TIPO_MEDIDO, ESTADO_OK), "{f:?} quadro {}", k0 + i);
                    assert!((den.x[i] * 320.0 - par as f64).abs() < 1e-4, "{f:?} quadro {}: x {} ≠ {par}", k0 + i, den.x[i] * 320.0);
                    medidas += 1;
                } else {
                    assert_eq!(den.tipo[i], TIPO_INTERPOLADO, "{f:?} quadro {}", k0 + i);
                }
            }
            assert!(medidas * 2 >= den.pts.len(), "{f:?}: {medidas} medidas em {}", den.pts.len());
            if f == Fonte::TsMeioDoGop {
                // os pacotes antes do quadro-chave seguinte não viram linha
                assert!(pacotes.len() > pts.len(), "TS cortado sem pacotes a mais");
                assert_eq!(den.pts[0], pts[0]);
            }
        }
    }

    // ---------------------------------------------------------------- mancha pelo motor novo (puro)

    #[test]
    fn marca_sem_motor_e_auto() {
        let m: MarcaMancha = serde_json::from_value(json!({ "id": null, "t": 1.0, "x": 1.0, "y": 2.0, "w": 10.0, "h": 10.0, "direcao": "frente" })).unwrap();
        assert_eq!(m.motor, EscolhaMotor::Auto);
        let m: MarcaMancha =
            serde_json::from_value(json!({ "t": 1.0, "x": 1.0, "y": 2.0, "w": 10.0, "h": 10.0, "nome": "", "direcao": "tras", "motor": "vittrack" })).unwrap();
        assert_eq!(m.motor, EscolhaMotor::Vittrack);
    }

    fn marca(t: f64, x: f32, y: f32, w: f32, h: f32, direcao: &str, motor: EscolhaMotor) -> MarcaMancha {
        MarcaMancha { id: None, t, x, y, w, h, nome: String::new(), direcao: direcao.into(), motor }
    }

    #[test]
    fn marca_vira_pedido_no_quadro_certo() {
        let tab = tabela_ts();
        // 4 ms antes do quadro 17 (o player parado nele): o quadro 17, não o 16
        let m = marca(t6(tab.tempo(17)) - 0.004, 630.0, -5.0, 40.0, 100.0, "frente", EscolhaMotor::Auto);
        let p = pedido_da_marca(&m, &tab, (1280, 720), (640, 360), &sha_falso(), Sentido::Tras).unwrap();
        assert_eq!((p.ancora.pts, p.ancora.t), (Some(tab.pts[17]), tab.tempo(17)));
        // presa à imagem como antes: x até w − 2, y ≥ 0, largura até a borda
        assert_eq!(p.ancora.v, Caixa { x: 630.0 / 640.0, y: 0.0, w: 10.0 / 640.0, h: 100.0 / 360.0 });
        assert_eq!(p.pixel, TamanhoPx { w: 1280, h: 720 });
        assert_eq!((p.sentido, p.ate_pts, p.max_quadros, p.espaco.as_str()), (Sentido::Tras, None, OpcoesMancha::default().max_quadros, "original"));
        assert_eq!(p.midia_sha256, sha_falso());
        assert!((p.fps - 30.0).abs() < 1e-9, "{}", p.fps);
        // depois do último quadro: o último
        let m = marca(100.0, 1.0, 1.0, 20.0, 20.0, "frente", EscolhaMotor::Auto);
        assert_eq!(pedido_da_marca(&m, &tab, (640, 360), (640, 360), "x", Sentido::Frente).unwrap().ancora.pts, Some(tab.pts[39]));
    }

    fn corrida_falsa(motor: MotorRastreio) -> Corrida {
        Corrida {
            motor: MotorCorrida {
                motor,
                modelo: ModeloCorrida { nome: "falso".into(), origem: String::new(), commit: String::new(), arquivos: vec![], desvio: String::new() },
                runtime: "onnxruntime 1.31.1".into(),
                provedor: "cpu".into(),
            },
            sentido: Sentido::Tras,
            ancora_pts: 0,
            de_pts: 0,
            ate_pts: 0,
            limiares: Limiares { ok: 0.45, perdido: 0.3, paciencia: 60 },
            parou: nucleo::Motivo::Perdido,
            feito_em: String::new(),
        }
    }

    #[test]
    fn corrida_vira_amostras_da_analise() {
        let tab = tabela_ts();
        let am = |j: usize, c: Option<[f64; 4]>, conf: f64| AmostraCorrida {
            pts: tab.pts[j],
            caixa: c.map(|c| Caixa { x: c[0], y: c[1], w: c[2], h: c[3] }),
            conf,
            estado: if c.is_some() { ESTADO_OK } else { ESTADO_AUSENTE },
            silhueta: None,
        };
        // para trás a partir do 20 (âncora); o 18 sem o alvo; perdido no 16
        let mut res = ResultadoCorrida {
            amostras: vec![
                am(20, Some([0.5, 0.25, 0.125, 0.5]), 1.0),
                am(19, Some([0.25, 0.5, 0.0625, 0.25]), 0.8),
                am(18, None, 0.1),
                am(17, Some([0.125, 0.75, 0.25, 0.125]), 0.6),
            ],
            parou: Parou { motivo: nucleo::Motivo::Perdido, pts: Some(tab.pts[16]) },
            corrida: corrida_falsa(MotorRastreio::Vittrack),
        };
        let s = de_corrida(&res, tab.tb, (640, 360));
        let lido: Vec<(f64, [f32; 4])> = s.amostras.iter().map(|a| (a.t, [a.x, a.y, a.w, a.h])).collect();
        assert_eq!(
            lido,
            vec![
                (tab.tempo(17), [80.0, 270.0, 160.0, 45.0]),
                (tab.tempo(19), [160.0, 180.0, 40.0, 90.0]),
                (tab.tempo(20), [320.0, 90.0, 80.0, 180.0])
            ]
        );
        assert_eq!((s.motivo, s.parou_em), (Motivo::Confianca, Some(tab.tempo(16))));
        assert!((s.conf_media - 0.7).abs() < 1e-6, "{}", s.conf_media);
        for (m, esperado) in [
            (nucleo::Motivo::SaiuDoQuadro, (Motivo::SaiuDoQuadro, Some(tab.tempo(16)))),
            (nucleo::Motivo::Limite, (Motivo::Limite, None)),
            (nucleo::Motivo::Fim, (Motivo::FimDoVideo, None)),
        ] {
            res.parou.motivo = m;
            let s = de_corrida(&res, tab.tb, (640, 360));
            assert_eq!((s.motivo, s.parou_em), esperado, "{m:?}");
        }
    }

    #[test]
    fn run_da_mancha_diz_o_motor() {
        assert_eq!(descricao_da_mancha(None), ("Mancha manual (correlação)".into(), "SICRO".into(), json!({ "manual": true })));
        let c = corrida_falsa(MotorRastreio::Edgetam);
        assert_eq!(
            descricao_da_mancha(Some(&c)),
            ("Mancha manual (EdgeTAM)".into(), "onnxruntime 1.31.1".into(), json!({ "manual": true, "motor": "edgetam", "provedor": "cpu" }))
        );
    }

    // ---------------------------------------------------------------- mancha no caso (ffmpeg)

    /// Bloco texturizado 40×60 em x = 20 + 2n, y = 80, nos quadros n < 130, sobre fundo liso; 25 qps, 6 s, início
    /// em 3 s, quadro-chave a cada 25. `ts` = MPEG-TS.
    fn bloco(dir: &Path, ts: bool) -> Option<PathBuf> {
        let cap = fontes::cap()?;
        let p = dir.join(if ts { "bloco.ts" } else { "bloco.mp4" });
        let mut cmd = crate::tools::command(&cap.binario);
        cmd.args(["-hide_banner", "-loglevel", "error"])
            .args(["-f", "lavfi", "-i", "color=c=0x404040:s=320x240:r=25", "-f", "lavfi", "-i", "color=c=black:s=40x60:r=25"])
            .args([
                "-filter_complex",
                "[1:v]format=gray,geq=lum='mod(X*X*31+Y*Y*17+X*Y*7\\,256)'[b];[0:v][b]overlay=x='20+2*n':y=80:enable='lt(n\\,130)',format=yuv420p",
            ])
            .args(["-t", "6", "-c:v", "libx264", "-qp", "0", "-g", "25", "-keyint_min", "25", "-sc_threshold", "0", "-bf", "0", "-pix_fmt", "yuv420p"])
            .args(["-output_ts_offset", "3"]);
        if ts {
            cmd.args(["-f", "mpegts"]);
        }
        assert!(cmd.arg("-y").arg(&p).status().ok()?.success(), "ffmpeg não gerou o bloco");
        Some(p)
    }

    /// Amostras gravadas da trilha `id`: (quadro, x, y), quadro pelos pts do decodificador.
    fn quadros_da_trilha(ws: &Path, occ: &Uuid, sha: &str, id: &str, pts: &[i64], tb: Racional) -> Vec<(usize, f32, f32)> {
        let p = repo::list(&banco(ws), occ, sha).unwrap().into_iter().find(|p| p.id == id).expect("trilha gravada");
        p.amostras
            .iter()
            .map(|a| {
                let j = pts.iter().position(|&q| (segundos(q, tb) - a.t).abs() < 2e-6).unwrap_or_else(|| panic!("t {} fora dos quadros", a.t));
                (j, a.x, a.y)
            })
            .collect()
    }

    fn json_da_trilha(ws: &Path, id: &str) -> Value {
        let s: String = banco(ws).query_row("SELECT amostras_json FROM video_people_tracks WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        serde_json::from_str(&s).unwrap()
    }

    fn ultimo_log(ws: &Path) -> Value {
        let s: String = banco(ws)
            .query_row("SELECT details_json FROM video_operation_logs WHERE action = 'rastreio.mancha' ORDER BY rowid DESC LIMIT 1", [], |r| r.get(0))
            .unwrap();
        serde_json::from_str(&s).unwrap()
    }

    /// Bloco do quadro `j0` em diante, a cada quadro: posição a até 1 px e tempo = pts do decodificador.
    fn confere_bloco(trilha: &[(usize, f32, f32)], j0: usize) {
        let ate_129: Vec<_> = trilha.iter().filter(|q| q.0 <= 129).collect();
        assert_eq!(ate_129.iter().map(|q| q.0).collect::<Vec<_>>(), (j0..=129).collect::<Vec<_>>(), "quadros seguidos");
        for &&(j, x, y) in &ate_129 {
            let xe = 20.0 + 2.0 * j as f32;
            assert!((x - xe).abs() <= 1.0 && (y - 80.0).abs() <= 1.0, "quadro {j}: ({x}, {y}) ≠ ({xe}, 80)");
        }
    }

    /// `pessoas_seguir_mancha` (corpo): segue, une, grava a trilha no formato de sempre, cria a análise e o registro.
    /// Roda pelo motor novo quando o núcleo do rastreio existe; sem ele, pelo seguidor de antes.
    #[test]
    fn seguir_mancha_grava_a_trilha_como_antes() {
        let (d, ws, occ) = caso();
        let Some(src) = bloco(d.path(), false) else { return };
        let (video, sha) = midia_no_caso(&ws, &occ, &src, "bloco.mp4");
        let cap = fontes::cap().unwrap();
        let pts = medir::pts(&cap, &video).unwrap();
        let tb = TabelaQuadros::de_pacotes(&cap, &video).unwrap().tb;
        assert!((segundos(pts[0], tb) - 3.0).abs() < 1e-3, "início {}", segundos(pts[0], tb));
        let m = marca(segundos(pts[10], tb), 40.0, 80.0, 40.0, 60.0, "frente", EscolhaMotor::Mancha);
        let mut n = Vec::new();
        let r = seguir_mancha_em(&ws, &d.path().join("cache"), &AiSettings::default(), &sha, &m, &AtomicBool::new(false), &mut |k| n.push(k)).unwrap();
        confere_bloco(&quadros_da_trilha(&ws, &occ, &sha, &r.id, &pts, tb), 10);
        assert_eq!(r.avisos.len(), 1);
        let a = &r.avisos[0];
        assert_eq!((a.sentido.as_str(), a.motivo), ("frente", Motivo::Confianca));
        // perdeu no 130 (com a paciência do laço, até 2 s depois)
        let parou = a.parou_em.expect("parou_em");
        assert!(parou >= segundos(pts[130], tb) - 1e-6 && parou <= segundos(pts[130], tb) + 2.0, "parou em {parou}");
        assert!(!n.is_empty() && n.windows(2).all(|w| w[0] <= w[1]), "progresso {n:?}");
        // o que vai para `video_people_tracks` não muda
        let j = json_da_trilha(&ws, &r.id);
        let chaves: Vec<&str> = j[0].as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(chaves, ["h", "t", "w", "x", "y"]);
        let p = &r.estado.pessoas[0];
        assert_eq!((p.origem.as_str(), p.classe, p.estado.as_str(), p.numero), ("manual", CLASSE_MANUAL, "auto", 1));
        let run = r.estado.run.as_ref().expect("análise criada");
        assert_eq!((run.width, run.height, run.frames_total), (320, 240, 150));
        assert_eq!(ultimo_log(&ws)["continuacao"], json!(false));
    }

    /// Cancelada pelo perito: erro "cancelado" (a tela não mostra) e nada gravado no caso.
    #[test]
    fn mancha_cancelada_nao_grava_nada() {
        let (d, ws, occ) = caso();
        let Some(src) = bloco(d.path(), false) else { return };
        let (video, sha) = midia_no_caso(&ws, &occ, &src, "bloco.mp4");
        let cap = fontes::cap().unwrap();
        let pts = medir::pts(&cap, &video).unwrap();
        let tb = TabelaQuadros::de_pacotes(&cap, &video).unwrap().tb;
        let m = marca(segundos(pts[10], tb), 40.0, 80.0, 40.0, 60.0, "ambos", EscolhaMotor::Mancha);
        match seguir_mancha_em(&ws, &d.path().join("cache"), &AiSettings::default(), &sha, &m, &AtomicBool::new(true), &mut |_| {}) {
            Err(SicroError::Validation(e)) => assert_eq!(e, "cancelado"),
            outro => panic!("{:?}", outro.map(|r| r.id)),
        }
        let conn = banco(&ws);
        assert!(repo::list(&conn, &occ, &sha).unwrap().is_empty(), "trilha gravada depois de cancelar");
        assert!(repo::get_run(&conn, &occ, &sha).unwrap().is_none(), "análise criada depois de cancelar");
    }

    // ---------------------------------------------------------------- fecho da Fase 2

    /// "Da análise" grava o denso na pasta do projeto, com o SHA-256 do ponteiro; renomear a trilha não a desatualiza,
    /// corrigir a caixa sim.
    #[test]
    #[ignore = "fecho da Fase 2: P2.2"]
    fn rastreio_da_pessoa_grava_no_projeto() {
        let Some(cap) = fontes::cap() else { return };
        let (d, ws, occ) = caso();
        let src = fontes::gerar(Fonte::Ts5994).expect("fonte");
        let (video, sha) = midia_no_caso(&ws, &occ, &src, "camera.ts");
        let pacotes = decode::frame_times(&video).unwrap();
        let amostras: Vec<Amostra> = pacotes.iter().skip(5).take(40).map(|&t| Amostra { t, x: 50.0, y: 60.0, w: 30.0, h: 40.0 }).collect();
        let conn = banco(&ws);
        let mut a = pessoa("pa", &sha, "editado", amostras);
        repo::insert(&conn, &occ, &a).unwrap();
        repo::replace_run(&conn, &occ, &Run { media_hash: sha.clone(), ..run(320, 180, 1, decode::frame_dur(&pacotes)) }).unwrap();
        let cache = d.path().join("cache");
        let r = rastreio_da_pessoa(&ws, &cache, "proj-1", "m1", &sha, "pa").unwrap();
        let den = r.denso.clone().expect("denso");
        let pasta = armazem::pasta_projeto(&ws, "proj-1");
        let arq = arquivo::caminho(&pasta, &r.id, &den.sha256);
        assert_eq!(sha256_file(&arq).unwrap(), den.sha256);
        let lido = arquivo::ler(&pasta, &r.id, &den.sha256).unwrap();
        let tab = TabelaQuadros::obter_com(&cap, &cache, &sha, &video).unwrap();
        assert_eq!(lido, denso_da_pessoa(&a, &run(320, 180, 1, decode::frame_dur(&pacotes)), &tab, &r.id, &sha).unwrap());
        assert_eq!((den.n as usize, den.ok as usize), (lido.pts.len(), lido.estado.iter().filter(|&&e| e == ESTADO_OK).count()));
        // revisada: confirmada no conteúdo gravado
        let c = r.confirmacao.clone().expect("confirmação");
        assert_eq!((c.denso_sha256.as_deref(), c.de_s, c.ate_s), (Some(den.sha256.as_str()), den.de_s, den.ate_s));

        let p = projeto(vec![serde_json::to_value(&r).unwrap()]);
        let p = ProjetoEdicao { id: "proj-1".into(), ..p };
        assert_eq!(desatualizados(&ws, &p), vec![]);
        std::thread::sleep(Duration::from_millis(5));
        a.nome = "Outro nome".into();
        repo::update(&conn, &a).unwrap();
        assert_eq!(desatualizados(&ws, &p), vec![], "renomear não muda a trilha");
        a.ajustar(a.amostras[3].t, 55.0, 60.0, 30.0, 40.0);
        repo::update(&conn, &a).unwrap();
        assert_eq!(desatualizados(&ws, &p).len(), 1);
    }

    /// A mancha pelo motor novo num MPEG-TS marcado no meio do GOP: parte do quadro marcado (o seguidor de antes
    /// partia do quadro-chave seguinte) e grava a corrida no registro.
    #[test]
    #[ignore = "fecho da Fase 2: P2.2"]
    fn seguir_mancha_pelo_motor_novo() {
        let (d, ws, occ) = caso();
        let Some(src) = bloco(d.path(), true) else { return };
        let (video, sha) = midia_no_caso(&ws, &occ, &src, "bloco.ts");
        let cap = fontes::cap().unwrap();
        let pts = medir::pts(&cap, &video).unwrap();
        let tb = TabelaQuadros::de_pacotes(&cap, &video).unwrap().tb;
        // 37: 12 quadros depois do quadro-chave 25
        let m = marca(t6(segundos(pts[37], tb)), 94.0, 80.0, 40.0, 60.0, "ambos", EscolhaMotor::Mancha);
        let r = seguir_mancha_em(&ws, &d.path().join("cache"), &AiSettings::default(), &sha, &m, &AtomicBool::new(false), &mut |_| {}).unwrap();
        let trilha = quadros_da_trilha(&ws, &occ, &sha, &r.id, &pts, tb);
        confere_bloco(&trilha, 0);
        let log = ultimo_log(&ws);
        assert_eq!(log["motor"], json!("mancha"));
        assert_eq!(log["corridas"].as_array().map(Vec::len), Some(2));
        assert_eq!(log["corridas"][0]["ancora_pts"], json!(pts[37]));
        assert_eq!(r.estado.run.as_ref().map(|r| r.model.as_str()), Some("Mancha manual (correlação)"));
    }

    /// O9: VitTrack e EdgeTAM pela aba Rastreio da análise, em 2 sequências de `p3a`, com os limiares do motor
    /// (os mesmos da Edição) e o IoU do aceite do P2.3 (≥ 0,75 e ≥ 0,85).
    #[test]
    #[ignore = "fecho da Fase 2: P2.2 e P2.3"]
    fn dois_motores_neurais_em_2_sequencias() {
        use crate::ia::testes as ia;
        let Some(cap) = fontes::cap() else { return };
        let Some(seq) = ia::ou_dispensa(ia::sequencias(), "dois_motores_neurais_em_2_sequencias", "p3a") else { return };
        let Some(ai) = ia::ou_dispensa(ia::ambiente(), "dois_motores_neurais_em_2_sequencias", "onnxruntime") else { return };
        let iou = |a: [f64; 4], b: [f64; 4]| {
            let (x0, y0) = (a[0].max(b[0]), a[1].max(b[1]));
            let (x1, y1) = ((a[0] + a[2]).min(b[0] + b[2]), (a[1] + a[3]).min(b[1] + b[3]));
            let i = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
            let u = a[2] * a[3] + b[2] * b[3] - i;
            if u > 0.0 { i / u } else { 0.0 }
        };
        for nome in ["escala", "rapido"] {
            let gt: Value = serde_json::from_str(&std::fs::read_to_string(seq.join(format!("sint/{nome}.json"))).unwrap()).unwrap();
            let gt = gt["gt"].as_array().unwrap().clone();
            let caixa = |g: &Value| -> Option<[f64; 4]> {
                let c = g["inteira"].as_array()?;
                Some([c[0].as_f64()?, c[1].as_f64()?, c[2].as_f64()?, c[3].as_f64()?])
            };
            for (motor, minimo) in [(EscolhaMotor::Vittrack, 0.75), (EscolhaMotor::Edgetam, 0.85)] {
                let (d, ws, occ) = caso();
                let (video, sha) = midia_no_caso(&ws, &occ, &seq.join(format!("sint/{nome}.mp4")), &format!("{nome}.mp4"));
                let pts = medir::pts(&cap, &video).unwrap();
                let tb = TabelaQuadros::de_pacotes(&cap, &video).unwrap().tb;
                let c0 = caixa(&gt[0]).unwrap();
                let m = marca(segundos(pts[0], tb), c0[0] as f32, c0[1] as f32, c0[2] as f32, c0[3] as f32, "frente", motor);
                let r = seguir_mancha_em(&ws, &d.path().join("cache"), &ai, &sha, &m, &AtomicBool::new(false), &mut |_| {}).unwrap();
                let p = repo::list(&banco(&ws), &occ, &sha).unwrap().into_iter().find(|p| p.id == r.id).unwrap();
                let mut soma = 0.0;
                let mut n = 0;
                for (j, g) in gt.iter().enumerate() {
                    let Some(v) = caixa(g).filter(|_| g["fracao"].as_f64().unwrap_or(1.0) >= 0.5) else { continue };
                    let t = segundos(pts[j], tb);
                    let achada = p.amostras.iter().find(|a| (a.t - t).abs() < 2e-6).map(|a| [a.x as f64, a.y as f64, a.w as f64, a.h as f64]);
                    soma += achada.map_or(0.0, |a| iou(a, v));
                    n += 1;
                }
                let media = soma / n as f64;
                assert!(media >= minimo, "{nome} {motor:?}: IoU médio {media:.3} < {minimo}");
                // limiares gravados = os do motor na taxa da mídia (os mesmos da Edição)
                let log = ultimo_log(&ws);
                let fps = 1.0 / segundos(TabelaQuadros::obter_com(&cap, &d.path().join("cache"), &sha, &video).unwrap().passo_mediano, tb);
                let esperado = corrida::escolher(&ai, motor).unwrap().limiares(fps);
                assert_eq!(serde_json::from_value::<Limiares>(log["corridas"][0]["limiares"].clone()).unwrap(), esperado, "{nome} {motor:?}");
                let (ok, perdido) = if motor == EscolhaMotor::Vittrack {
                    (nucleo::laco::T_INCERTO, nucleo::laco::T_PERDA)
                } else {
                    (nucleo::laco::P_INCERTO, nucleo::laco::P_PRESENTE)
                };
                assert_eq!((esperado.ok, esperado.perdido), (ok, perdido), "{motor:?}");
                assert_eq!(log["motor"], serde_json::to_value(if motor == EscolhaMotor::Vittrack { MotorRastreio::Vittrack } else { MotorRastreio::Edgetam }).unwrap());
            }
        }
    }
}
