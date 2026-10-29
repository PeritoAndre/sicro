//! Leitura e realce de imagem no vídeo (5.3): média de quadros alinhados, super-resolução,
//! estabilização, lente e perspectiva. Realce é apoio de leitura, não prova: o original nunca é
//! alterado; tudo que sai é arquivo novo com SHA-256, parâmetros e aviso, e entra na trilha.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use image::{DynamicImage, RgbImage};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::commands::settings_commands::get_app_settings;
use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::video_realce_repo::{self as repo, PerfilLente, Resultado};
use crate::database::repositories::{occurrence_repo, video_repo};
use crate::error::{Result, SicroError};
use crate::hashing::sha256::sha256_file;
use crate::image_processing::lens_correction::{
    apply_radial_correction_centered, corrigir_rgb, para_ffmpeg, LensCoefficients,
};
use crate::models::{VideoMedia, VideoStoryboardFrame};
use crate::video::realce::average::{self, InfoQuadro, Opcoes};
use crate::video::realce::janela::{self, Lente};
use crate::video::realce::perspective::{self, Par};
use crate::video::realce::stabilize::{self, Borda, OpcoesEstabilizar};
use crate::video::realce::superres::{self, SuperRes};
use crate::video::{extract_frame, ExtractFrameOptions};
use crate::workspace::manifest::{Manifest, APP_VERSION, SQLITE_FILENAME};

const REALCE_SUBDIR: &str = "videos/realce";
const ESTAB_SUBDIR: &str = "videos/estabilizados";

const AVISO_REALCE: &str = "Realce de imagem para leitura: não substitui o original e não é prova por si só.";

// ---------------------------------------------------------------------------
// Tarefa única (média, super-resolução, estabilização, cópia com lente)

static TAREFA: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

fn iniciar_tarefa() -> Result<Arc<AtomicBool>> {
    let mut g = TAREFA.lock().map_err(|_| SicroError::Workspace("estado interno".into()))?;
    if g.is_some() {
        return Err(SicroError::Validation("já existe uma tarefa de realce em andamento".into()));
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
pub async fn realce_cancelar() -> Result<()> {
    if let Ok(g) = TAREFA.lock() {
        if let Some(f) = g.as_ref() {
            f.store(true, Ordering::Relaxed);
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, Clone)]
struct Progresso<'a> {
    media_hash: &'a str,
    fase: &'a str,
    feitos: u32,
    total: u32,
}

fn emitir(app: &AppHandle, hash: &str, fase: &str, feitos: u32, total: u32) {
    let _ = app.emit("realce-progress", Progresso { media_hash: hash, fase, feitos, total });
}

// ---------------------------------------------------------------------------
// Contexto e utilidades

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

impl Contexto {
    fn video(&self) -> Result<PathBuf> {
        let v = self.ws.join(&self.media.relative_path);
        if !v.is_file() {
            return Err(SicroError::Filesystem(format!("arquivo de vídeo ausente: {}", self.media.relative_path)));
        }
        Ok(v)
    }
    fn lente(&self) -> Result<Option<Lente>> {
        Ok(repo::obter_perfil(&self.conn, &self.occ, &self.media.sha256)?.map(|p| perfil_para_lente(&p)))
    }
    fn stem(&self) -> String {
        Path::new(&self.media.filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(crate::filesystem::sanitize_folder_name)
            .unwrap_or_else(|| "video".into())
    }
}

fn tem_audio(raw_probe_json: &str) -> bool {
    serde_json::from_str::<Value>(raw_probe_json)
        .ok()
        .and_then(|v| v["streams"].as_array().map(|l| l.iter().any(|s| s["codec_type"] == "audio")))
        .unwrap_or(false)
}

fn perfil_para_lente(p: &PerfilLente) -> Lente {
    Lente { coef: LensCoefficients { k1: p.k1, k2: p.k2, k3: 0.0 }, cx: p.cx, cy: p.cy }
}

fn pasta_cache(app: &AppHandle) -> Result<PathBuf> {
    let d = app
        .path()
        .app_cache_dir()
        .map_err(|e| SicroError::Filesystem(format!("cache dir: {e}")))?
        .join("realce");
    std::fs::create_dir_all(&d).map_err(|e| SicroError::Filesystem(format!("criar {}: {e}", d.display())))?;
    Ok(d)
}

fn pasta_saida(c: &Contexto, sub: &str) -> Result<PathBuf> {
    let d = c.ws.join(sub);
    std::fs::create_dir_all(&d).map_err(|e| SicroError::Filesystem(format!("criar {}: {e}", d.display())))?;
    Ok(d)
}

fn carimbo() -> String {
    Utc::now().format("%Y%m%d_%H%M%S").to_string()
}

fn ms(t: f64) -> i64 {
    (t.max(0.0) * 1000.0).round() as i64
}

fn salvar_rgb(rgb: Vec<u8>, w: u32, h: u32, destino: &Path) -> Result<()> {
    let img = RgbImage::from_raw(w, h, rgb).ok_or_else(|| SicroError::Workspace("tamanho de imagem incoerente".into()))?;
    img.save(destino).map_err(|e| SicroError::Filesystem(format!("gravar {}: {e}", destino.display())))
}

fn ler_rgb(path: &Path) -> Result<(Vec<u8>, u32, u32)> {
    let img = image::open(path)
        .map_err(|e| SicroError::Filesystem(format!("abrir {}: {e}", path.display())))?
        .to_rgb8();
    let (w, h) = img.dimensions();
    Ok((img.into_raw(), w, h))
}

/// Quadro exato do instante `t` (PNG do ffmpeg) no cache: (arquivo, largura, altura, instante real).
fn quadro_png(c: &Contexto, cache: &Path, t: f64) -> Result<(PathBuf, u32, u32, f64)> {
    let video = c.video()?;
    let png = cache.join(format!("{}_{}ms.png", &c.media.sha256[..12], ms(t)));
    let meta = png.with_extension("t");
    let real = if png.is_file() {
        std::fs::read_to_string(&meta).ok().and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(t)
    } else {
        let ex = extract_frame(ExtractFrameOptions {
            video_path: &video,
            timestamp_s: t,
            out_png: &png,
            sidecar_json: None,
            sidecar_extra: Value::Null,
            start_time_s: crate::video::probe::container_start_time(&c.media.raw_probe_json),
        })?;
        let r = ex.actual_timestamp_s.unwrap_or(t);
        let _ = std::fs::write(&meta, r.to_string());
        r
    };
    let (w, h) = image::image_dimensions(&png).map_err(|e| SicroError::Filesystem(format!("ler o quadro: {e}")))?;
    Ok((png, w, h, real))
}

// ---------------------------------------------------------------------------
// Registro das saídas

/// Grava o sidecar JSON, a linha do resultado, a de `video_exports` (mesmo id), a trilha e a auditoria.
fn registrar(
    c: &Contexto,
    tipo: &str,
    rel: &str,
    params: Value,
    aviso: &str,
    dims: Option<(u32, u32)>,
    com_export: bool,
) -> Result<Resultado> {
    let abs = c.ws.join(rel);
    let sha256 = sha256_file(&abs)?;
    let tamanho = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
    let id = Uuid::new_v4();
    let agora = Utc::now();
    let sidecar_rel = format!("{rel}.json");
    let doc = json!({
        "id": id.to_string(), "tipo": tipo, "sha256": sha256, "tamanho": tamanho,
        "origem": { "sha256": c.media.sha256, "arquivo": c.media.filename },
        "parametros": params, "aviso": aviso, "aplicativo": format!("SICRO {APP_VERSION}"),
        "criado_em": agora.to_rfc3339(),
    });
    std::fs::write(c.ws.join(&sidecar_rel), serde_json::to_vec_pretty(&doc).unwrap_or_default())
        .map_err(|e| SicroError::Filesystem(format!("gravar {sidecar_rel}: {e}")))?;
    let nome = Path::new(rel).file_name().and_then(|s| s.to_str()).unwrap_or(rel).to_string();
    if com_export {
        let x = crate::models::VideoExport {
            id,
            occurrence_id: c.occ,
            media_hash: c.media.sha256.clone(),
            event_id: None,
            r#type: format!("realce_{tipo}"),
            requested_timestamp_s: params["t"].as_f64().unwrap_or(0.0),
            actual_timestamp_s: params["t"].as_f64(),
            delta_s: None,
            output_path: rel.to_string(),
            filename: nome,
            sidecar_json_path: Some(sidecar_rel),
            details_json: json!({ "sha256": sha256, "size_bytes": tamanho, "parametros": params, "aviso": aviso }).to_string(),
            created_at: agora,
        };
        video_repo::insert_export(&c.conn, &x)?;
    }
    let r = Resultado {
        id: id.to_string(),
        media_hash: c.media.sha256.clone(),
        tipo: tipo.to_string(),
        caminho: rel.to_string(),
        sha256: sha256.clone(),
        tamanho,
        largura: dims.map(|d| d.0),
        altura: dims.map(|d| d.1),
        parametros: params.clone(),
        aviso: aviso.to_string(),
        created_at: agora.to_rfc3339(),
    };
    repo::inserir_resultado(&c.conn, &c.occ, &r)?;
    video_repo::insert_log(
        &c.conn,
        &c.occ,
        Some(&c.media.sha256),
        &format!("realce.{tipo}"),
        &json!({ "arquivo": rel, "sha256": sha256, "parametros": params }).to_string(),
    )?;
    occurrence_repo::record_audit(
        &c.conn,
        Some(&c.occ),
        &format!("video.realce.{tipo}"),
        Some("video"),
        Some("video_realce_results"),
        Some(&id),
        Some(&sha256),
    )?;
    Ok(r)
}

#[derive(Debug, Serialize)]
pub struct Arquivo {
    pub resultado: Resultado,
    pub absoluto: String,
}

fn arquivo(c: &Contexto, r: Resultado) -> Arquivo {
    let absoluto = c.ws.join(&r.caminho).to_string_lossy().to_string();
    Arquivo { resultado: r, absoluto }
}

#[tauri::command]
pub async fn realce_listar(workspace_path: String, media_hash: String) -> Result<Vec<Arquivo>> {
    let c = contexto(&workspace_path, &media_hash)?;
    let lista = repo::listar_resultados(&c.conn, &c.occ, &media_hash)?;
    Ok(lista.into_iter().map(|r| arquivo(&c, r)).collect())
}

/// Leva um resultado (imagem) ao storyboard do vídeo, com a legenda "Realce".
#[tauri::command]
pub async fn realce_para_storyboard(
    workspace_path: String,
    media_hash: String,
    resultado_id: String,
    titulo: Option<String>,
) -> Result<VideoStoryboardFrame> {
    let c = contexto(&workspace_path, &media_hash)?;
    let r = repo::obter_resultado(&c.conn, &c.occ, &resultado_id)?
        .ok_or_else(|| SicroError::Validation("resultado não encontrado".into()))?;
    if !r.caminho.to_lowercase().ends_with(".png") {
        return Err(SicroError::Validation("só imagens vão para o storyboard".into()));
    }
    let t = r.parametros["t"].as_f64().unwrap_or(0.0);
    let agora = Utc::now();
    let legenda = match r.tipo.as_str() {
        "media" | "media_quadro" => format!(
            "Realce: média de {} quadros alinhados.",
            r.parametros["usados"].as_u64().unwrap_or(0)
        ),
        "superres" => "Realce por rede neural (pode inventar detalhe; não usar para identificação sem o original ao lado).".into(),
        "retificada" => "Realce: imagem retificada (vista de cima).".into(),
        _ => "Realce de imagem.".into(),
    };
    let f = VideoStoryboardFrame {
        id: Uuid::new_v4(),
        occurrence_id: c.occ,
        media_hash: media_hash.clone(),
        event_id: None,
        export_id: Uuid::parse_str(&r.id).ok(),
        title: titulo.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "Realce".into()),
        caption: legenda,
        notes: String::new(),
        requested_timestamp_s: t,
        actual_timestamp_s: Some(t),
        delta_s: None,
        observed_frame_index: None,
        estimated_total_frames: c.media.frame_count,
        frame_index_is_estimated: true,
        pts: None,
        time_base: c.media.time_base.clone(),
        output_path: r.caminho.clone(),
        sidecar_json_path: Some(format!("{}.json", r.caminho)),
        reviewed: false,
        created_at: agora,
        updated_at: agora,
    };
    video_repo::insert_storyboard_frame(&c.conn, &f)?;
    video_repo::insert_log(&c.conn, &c.occ, Some(&media_hash), "realce.storyboard", &json!({ "resultado": r.id }).to_string())?;
    Ok(f)
}

// ---------------------------------------------------------------------------
// Quadro de trabalho

#[derive(Debug, Serialize)]
pub struct QuadroTrabalho {
    pub absoluto: String,
    pub largura: u32,
    pub altura: u32,
    /// Instante real do quadro mostrado.
    pub t: f64,
    pub com_lente: bool,
}

/// Quadro exato do instante `t` para trabalhar em cima (com a lente do vídeo, se pedida).
#[tauri::command]
pub async fn realce_quadro(
    app: AppHandle,
    workspace_path: String,
    media_hash: String,
    t: f64,
    com_lente: bool,
) -> Result<QuadroTrabalho> {
    let cache = pasta_cache(&app)?;
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<QuadroTrabalho> {
        let c = contexto(&workspace_path, &media_hash)?;
        let (png, w, h, real) = quadro_png(&c, &cache, t)?;
        let lente = if com_lente { c.lente()? } else { None };
        if let Some(l) = lente.filter(|l| !l.coef.is_identity()) {
            let corrigido = png.with_file_name(format!(
                "{}_l{:.4}_{:.4}_{:.3}_{:.3}.png",
                png.file_stem().and_then(|s| s.to_str()).unwrap_or("q"),
                l.coef.k1, l.coef.k2, l.cx, l.cy
            ));
            if !corrigido.is_file() {
                let (rgb, w, h) = ler_rgb(&png)?;
                salvar_rgb(corrigir_rgb(&rgb, w, h, l.coef, l.cx, l.cy), w, h, &corrigido)?;
            }
            return Ok(QuadroTrabalho { absoluto: corrigido.to_string_lossy().into(), largura: w, altura: h, t: real, com_lente: true });
        }
        Ok(QuadroTrabalho { absoluto: png.to_string_lossy().into(), largura: w, altura: h, t: real, com_lente: false })
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("quadro de trabalho: {e}")))??;
    Ok(r)
}

// ---------------------------------------------------------------------------
// Média de quadros alinhados

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct Roi {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Deserialize)]
pub struct ParamsMedia {
    pub t: f64,
    pub roi: Roi,
    /// Quantos quadros em volta (5 a 60).
    pub quadros: u32,
    pub mediana: bool,
    pub homografia: bool,
    pub com_lente: bool,
}

#[derive(Debug, Serialize)]
pub struct ResultadoMedia {
    pub roi: Arquivo,
    pub quadro: Arquivo,
    /// A mesma região, crua, do quadro de referência (para o lado a lado).
    pub crua: String,
    pub usados: usize,
    pub total: usize,
    pub quadros: Vec<InfoQuadro>,
    pub t_ref: f64,
    pub tempo_s: f64,
}

#[tauri::command]
pub async fn realce_media(
    app: AppHandle,
    workspace_path: String,
    media_hash: String,
    params: ParamsMedia,
) -> Result<ResultadoMedia> {
    let cache = pasta_cache(&app)?;
    let flag = iniciar_tarefa()?;
    let (app2, hash2) = (app.clone(), media_hash.clone());
    let r = tauri::async_runtime::spawn_blocking(move || {
        media_blocante(&app2, &workspace_path, &hash2, params, &cache, &flag)
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("tarefa de realce: {e}")));
    encerrar_tarefa();
    r?
}

fn media_blocante(
    app: &AppHandle,
    workspace_path: &str,
    media_hash: &str,
    p: ParamsMedia,
    cache: &Path,
    flag: &AtomicBool,
) -> Result<ResultadoMedia> {
    let inicio = std::time::Instant::now();
    let c = contexto(workspace_path, media_hash)?;
    let video = c.video()?;
    let n = p.quadros.clamp(5, 60) as usize;
    let (_png, w, h, t_ref) = quadro_png(&c, cache, p.t)?;
    let roi = p.roi;
    if roi.w < 16 || roi.h < 16 || roi.x + roi.w > w || roi.y + roi.h > h {
        return Err(SicroError::Validation("a região marcada é pequena demais ou sai do quadro".into()));
    }
    // margem em volta da região: é por onde o alinhamento enxerga o deslocamento
    let margem = (roi.w.max(roi.h) / 4).clamp(12, 64);
    let px0 = roi.x.saturating_sub(margem);
    let py0 = roi.y.saturating_sub(margem);
    let px1 = (roi.x + roi.w + margem).min(w);
    let py1 = (roi.y + roi.h + margem).min(h);
    let (pw, ph) = (px1 - px0, py1 - py0);
    let bytes = ((pw * ph) as u64 + (roi.w * roi.h) as u64) * 3 * n as u64;
    if bytes > 900_000_000 {
        return Err(SicroError::Validation(
            "região grande demais para tantos quadros: marque só a placa ou o rosto, ou use menos quadros".into(),
        ));
    }
    emitir(app, media_hash, "ler", 0, n as u32);
    let (tempos, ref_pos) = janela::escolher_janela(&video, t_ref, n)?;
    let lente = if p.com_lente { c.lente()? } else { None };
    let lida = janela::ler_janela(
        &video,
        crate::video::probe::container_start_time(&c.media.raw_probe_json),
        &tempos,
        w,
        h,
        (px0, py0, pw, ph),
        ref_pos,
        lente.as_ref(),
        flag,
    )?;
    let busca = (margem as usize).saturating_sub(2).max(4);
    let comb = average::combinar(
        &lida.recortes,
        Opcoes {
            roi: ((roi.x - px0) as usize, (roi.y - py0) as usize, roi.w as usize, roi.h as usize),
            referencia: ref_pos,
            homografia: p.homografia,
            mediana: p.mediana,
            busca,
        },
        flag,
        &|f, t| emitir(app, media_hash, "alinhar", f as u32, t as u32),
    )
    .map_err(SicroError::Validation)?;

    let dir = pasta_saida(&c, REALCE_SUBDIR)?;
    let base = format!("{}_media_{}_{}ms", c.stem(), carimbo(), ms(t_ref));
    let (rw, rh) = (roi.w, roi.h);
    // quadro inteiro: o de referência com a região trocada pela média
    let mut inteiro = lida.quadro_ref.clone();
    for y in 0..rh as usize {
        let dst = ((roi.y as usize + y) * w as usize + roi.x as usize) * 3;
        let src = y * rw as usize * 3;
        inteiro[dst..dst + rw as usize * 3].copy_from_slice(&comb.rgb[src..src + rw as usize * 3]);
    }
    let crua_abs = dir.join(format!("{base}_crua.png"));
    salvar_rgb(comb.rgb_ref.clone(), rw, rh, &crua_abs)?;
    let roi_rel = format!("{REALCE_SUBDIR}/{base}_roi.png");
    let full_rel = format!("{REALCE_SUBDIR}/{base}_quadro.png");
    salvar_rgb(comb.rgb.clone(), rw, rh, &c.ws.join(&roi_rel))?;
    salvar_rgb(inteiro, w, h, &c.ws.join(&full_rel))?;

    let tempo_s = inicio.elapsed().as_secs_f64();
    let params_json = json!({
        "t": t_ref, "roi": roi, "quadros_pedidos": n, "quadros_lidos": tempos.len(),
        "usados": comb.usados, "metodo": if p.mediana { "mediana" } else { "media" },
        "alinhamento": if p.homografia { "homografia (Gauss-Newton sobre translação inicial)" } else { "translação (correlação de fase + refinamento subpixel)" },
        "lente": lente.as_ref().map(|l| json!({ "k1": l.coef.k1, "k2": l.coef.k2, "cx": l.cx, "cy": l.cy })),
        "tempos_s": tempos, "quadro_de_referencia": ref_pos,
        "por_quadro": comb.quadros, "crua_arquivo": crua_abs.file_name().and_then(|s| s.to_str()),
        "crua_sha256": sha256_file(&crua_abs)?, "tempo_de_processamento_s": tempo_s,
    });
    let r_roi = registrar(&c, "media", &roi_rel, params_json.clone(), AVISO_REALCE, Some((rw, rh)), true)?;
    let r_full = registrar(&c, "media_quadro", &full_rel, params_json, AVISO_REALCE, Some((w, h)), true)?;
    Ok(ResultadoMedia {
        roi: arquivo(&c, r_roi),
        quadro: arquivo(&c, r_full),
        crua: crua_abs.to_string_lossy().into(),
        usados: comb.usados,
        total: lida.recortes.len(),
        quadros: comb.quadros,
        t_ref,
        tempo_s,
    })
}

// ---------------------------------------------------------------------------
// Super-resolução

#[derive(Debug, Deserialize)]
pub struct ParamsSuperRes {
    /// Resultado anterior (a média, por exemplo); sem ele usa o quadro do instante `t`.
    pub resultado_id: Option<String>,
    pub t: f64,
    /// Recorte do quadro (ignorado quando há `resultado_id`); sem ele, o quadro inteiro.
    pub roi: Option<Roi>,
    /// 2 ou 4.
    pub escala: u32,
    pub com_lente: bool,
}

#[derive(Debug, Serialize)]
pub struct ResultadoSuperRes {
    pub arquivo: Arquivo,
    /// Entrada, sem realce, para o lado a lado.
    pub original: String,
    pub tempo_s: f64,
}

#[tauri::command]
pub async fn realce_superres(
    app: AppHandle,
    workspace_path: String,
    media_hash: String,
    params: ParamsSuperRes,
) -> Result<ResultadoSuperRes> {
    let ai = get_app_settings(app.clone()).await?.ai;
    if !crate::commands::ai_commands::superres_ready(&ai) {
        return Err(SicroError::Validation("superres_nao_instalado".into()));
    }
    if params.escala != 2 && params.escala != 4 {
        return Err(SicroError::Validation("a escala é 2 ou 4".into()));
    }
    let cache = pasta_cache(&app)?;
    let flag = iniciar_tarefa()?;
    let (app2, hash2) = (app.clone(), media_hash.clone());
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<ResultadoSuperRes> {
        let inicio = std::time::Instant::now();
        let c = contexto(&workspace_path, &hash2)?;
        let (entrada, origem) = if let Some(id) = &params.resultado_id {
            let r = repo::obter_resultado(&c.conn, &c.occ, id)?
                .ok_or_else(|| SicroError::Validation("resultado não encontrado".into()))?;
            (c.ws.join(&r.caminho), json!({ "resultado": r.id, "sha256": r.sha256, "tipo": r.tipo }))
        } else {
            let (png, w, h, real) = quadro_png(&c, &cache, params.t)?;
            let lente = if params.com_lente { c.lente()? } else { None };
            let (mut rgb, _, _) = ler_rgb(&png)?;
            if let Some(l) = &lente {
                rgb = corrigir_rgb(&rgb, w, h, l.coef, l.cx, l.cy);
            }
            let (rgb, w2, h2) = match params.roi {
                Some(r) if r.x + r.w <= w && r.y + r.h <= h && r.w >= 8 && r.h >= 8 => {
                    let mut o = Vec::with_capacity((r.w * r.h * 3) as usize);
                    for y in r.y..r.y + r.h {
                        let a = ((y * w + r.x) * 3) as usize;
                        o.extend_from_slice(&rgb[a..a + (r.w * 3) as usize]);
                    }
                    (o, r.w, r.h)
                }
                Some(_) => return Err(SicroError::Validation("região inválida".into())),
                None => (rgb, w, h),
            };
            let tmp = cache.join(format!("sr_entrada_{}.png", Uuid::new_v4()));
            salvar_rgb(rgb, w2, h2, &tmp)?;
            (tmp, json!({ "quadro_t": real, "roi": params.roi, "lente": lente.is_some() }))
        };
        let (rgb, w, h) = ler_rgb(&entrada)?;
        let entrada_sha = sha256_file(&entrada)?;
        let lib = crate::commands::ai_commands::superres_lib(&ai)
            .ok_or_else(|| SicroError::Validation("superres_nao_instalado".into()))?;
        let mut sr = SuperRes::load(&lib, Path::new(&ai.sr_model_path))?;
        let up = sr.ampliar4(&rgb, w as usize, h as usize, &flag, &mut |f, t| emitir(&app2, &hash2, "superres", f as u32, t as u32))?;
        let (ow, oh, saida) = if params.escala == 4 {
            (w * 4, h * 4, up)
        } else {
            let (nw, nh) = (w * 2, h * 2);
            (nw, nh, superres::reduzir(&up, (w * 4) as usize, (h * 4) as usize, nw as usize, nh as usize))
        };
        let dir = pasta_saida(&c, REALCE_SUBDIR)?;
        let rel = format!("{REALCE_SUBDIR}/{}_superres{}x_{}.png", c.stem(), params.escala, carimbo());
        salvar_rgb(saida, ow, oh, &c.ws.join(&rel))?;
        let original = dir.join(format!("{}_entrada_{}.png", c.stem(), carimbo()));
        std::fs::copy(&entrada, &original).map_err(|e| SicroError::Filesystem(format!("copiar entrada: {e}")))?;
        if params.resultado_id.is_none() {
            let _ = std::fs::remove_file(&entrada);
        }
        let tempo_s = inicio.elapsed().as_secs_f64();
        let pj = json!({
            "t": params.t, "escala": params.escala, "modelo": "Real-ESRGAN general-x4v3 (BSD-3)",
            "runtime": ai.sr_version, "ladrilho": 160, "origem": origem,
            "entrada": { "largura": w, "altura": h, "sha256": entrada_sha },
            "tempo_de_processamento_s": tempo_s,
        });
        let r = registrar(&c, "superres", &rel, pj, superres::AVISO, Some((ow, oh)), true)?;
        Ok(ResultadoSuperRes { arquivo: arquivo(&c, r), original: original.to_string_lossy().into(), tempo_s })
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("super-resolução: {e}")));
    encerrar_tarefa();
    r?
}

/// Super-resolução de uma imagem do módulo Imagem (arquivo do caso), com recorte opcional.
#[derive(Debug, Deserialize)]
pub struct ParamsSuperResImagem {
    /// Caminho relativo ao caso.
    pub relative_path: String,
    pub roi: Option<Roi>,
    pub escala: u32,
}

#[derive(Debug, Serialize)]
pub struct ResultadoSuperResImagem {
    pub relative_path: String,
    pub absoluto: String,
    pub sha256: String,
    pub largura: u32,
    pub altura: u32,
    pub aviso: &'static str,
    pub tempo_s: f64,
}

#[tauri::command]
pub async fn realce_superres_imagem(
    app: AppHandle,
    workspace_path: String,
    params: ParamsSuperResImagem,
) -> Result<ResultadoSuperResImagem> {
    let ai = get_app_settings(app.clone()).await?.ai;
    if !crate::commands::ai_commands::superres_ready(&ai) {
        return Err(SicroError::Validation("superres_nao_instalado".into()));
    }
    if params.escala != 2 && params.escala != 4 {
        return Err(SicroError::Validation("a escala é 2 ou 4".into()));
    }
    let flag = iniciar_tarefa()?;
    let app2 = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<ResultadoSuperResImagem> {
        let inicio = std::time::Instant::now();
        let ws = PathBuf::from(&workspace_path);
        let manifest = Manifest::read(&ws)?;
        let origem = crate::filesystem::resolve_workspace_relative(&ws, &params.relative_path)?;
        let (rgb, w, h) = ler_rgb(&origem)?;
        let (rgb, w, h) = match params.roi {
            Some(r) if r.x + r.w <= w && r.y + r.h <= h && r.w >= 8 && r.h >= 8 => {
                let mut o = Vec::with_capacity((r.w * r.h * 3) as usize);
                for y in r.y..r.y + r.h {
                    let a = ((y * w + r.x) * 3) as usize;
                    o.extend_from_slice(&rgb[a..a + (r.w * 3) as usize]);
                }
                (o, r.w, r.h)
            }
            Some(_) => return Err(SicroError::Validation("região inválida".into())),
            None => (rgb, w, h),
        };
        let lib = crate::commands::ai_commands::superres_lib(&ai)
            .ok_or_else(|| SicroError::Validation("superres_nao_instalado".into()))?;
        let mut sr = SuperRes::load(&lib, Path::new(&ai.sr_model_path))?;
        let up = sr.ampliar4(&rgb, w as usize, h as usize, &flag, &mut |f, t| emitir(&app2, "imagem", "superres", f as u32, t as u32))?;
        let (ow, oh, saida) = if params.escala == 4 {
            (w * 4, h * 4, up)
        } else {
            let (nw, nh) = (w * 2, h * 2);
            (nw, nh, superres::reduzir(&up, (w * 4) as usize, (h * 4) as usize, nw as usize, nh as usize))
        };
        let stem = origem.file_stem().and_then(|s| s.to_str()).map(crate::filesystem::sanitize_folder_name).unwrap_or_else(|| "imagem".into());
        let rel = format!("imagens/realce/{stem}_superres{}x_{}.png", params.escala, carimbo());
        let destino = ws.join(&rel);
        if let Some(p) = destino.parent() {
            std::fs::create_dir_all(p).map_err(|e| SicroError::Filesystem(format!("criar {}: {e}", p.display())))?;
        }
        salvar_rgb(saida, ow, oh, &destino)?;
        let sha256 = sha256_file(&destino)?;
        let tempo_s = inicio.elapsed().as_secs_f64();
        std::fs::write(
            format!("{}.json", destino.display()),
            serde_json::to_vec_pretty(&json!({
                "tipo": "superres_imagem", "sha256": sha256, "origem": params.relative_path,
                "roi": params.roi, "escala": params.escala, "modelo": "Real-ESRGAN general-x4v3 (BSD-3)",
                "runtime": ai.sr_version, "aviso": superres::AVISO, "aplicativo": format!("SICRO {APP_VERSION}"),
                "criado_em": Utc::now().to_rfc3339(), "tempo_de_processamento_s": tempo_s,
            }))
            .unwrap_or_default(),
        )
        .map_err(|e| SicroError::Filesystem(format!("gravar sidecar: {e}")))?;
        let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
        occurrence_repo::record_audit(
            &conn,
            Some(&manifest.occurrence_id),
            "imagem.superres",
            Some("imagem"),
            None,
            None,
            Some(&sha256),
        )?;
        Ok(ResultadoSuperResImagem {
            relative_path: rel,
            absoluto: destino.to_string_lossy().into(),
            sha256,
            largura: ow,
            altura: oh,
            aviso: superres::AVISO,
            tempo_s,
        })
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("super-resolução: {e}")));
    encerrar_tarefa();
    r?
}

// ---------------------------------------------------------------------------
// Lente

#[tauri::command]
pub async fn realce_lente_obter(workspace_path: String, media_hash: String) -> Result<Option<PerfilLente>> {
    let c = contexto(&workspace_path, &media_hash)?;
    repo::obter_perfil(&c.conn, &c.occ, &media_hash)
}

#[tauri::command]
pub async fn realce_lente_salvar(workspace_path: String, media_hash: String, perfil: PerfilLente) -> Result<()> {
    if !(-1.0..=1.0).contains(&perfil.k1) || !(-1.0..=1.0).contains(&perfil.k2) {
        return Err(SicroError::Validation("k1 e k2 ficam entre −1 e 1".into()));
    }
    let c = contexto(&workspace_path, &media_hash)?;
    repo::salvar_perfil(&c.conn, &c.occ, &media_hash, &perfil)?;
    video_repo::insert_log(&c.conn, &c.occ, Some(&media_hash), "realce.lente.perfil", &serde_json::to_string(&perfil).unwrap_or_default())?;
    Ok(())
}

#[tauri::command]
pub async fn realce_lente_apagar(workspace_path: String, media_hash: String) -> Result<()> {
    let c = contexto(&workspace_path, &media_hash)?;
    repo::apagar_perfil(&c.conn, &c.occ, &media_hash)
}

#[derive(Debug, Serialize)]
pub struct PerfilDeVideo {
    pub media_hash: String,
    pub filename: String,
    pub perfil: PerfilLente,
}

/// Perfis de lente já salvos para os vídeos do caso (para reaproveitar o da mesma câmera).
#[tauri::command]
pub async fn realce_lente_listar(workspace_path: String) -> Result<Vec<PerfilDeVideo>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let medias = video_repo::list_media_for_occurrence(&conn, &manifest.occurrence_id)?;
    Ok(repo::listar_perfis(&conn, &manifest.occurrence_id)?
        .into_iter()
        .map(|(h, perfil)| PerfilDeVideo {
            filename: medias.iter().find(|m| m.sha256 == h).map(|m| m.filename.clone()).unwrap_or_default(),
            media_hash: h,
            perfil,
        })
        .collect())
}

#[derive(Debug, Serialize)]
pub struct PreviaLente {
    pub absoluto: String,
    pub largura: u32,
    pub altura: u32,
}

/// Prévia da correção sobre o quadro de trabalho (reduzida a `max_lado`), com bordas transparentes.
#[tauri::command]
pub async fn realce_lente_previa(
    app: AppHandle,
    workspace_path: String,
    media_hash: String,
    t: f64,
    perfil: PerfilLente,
    max_lado: Option<u32>,
) -> Result<PreviaLente> {
    let cache = pasta_cache(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<PreviaLente> {
        let c = contexto(&workspace_path, &media_hash)?;
        let (png, _, _, _) = quadro_png(&c, &cache, t)?;
        let mut img = image::open(&png).map_err(|e| SicroError::Filesystem(format!("abrir quadro: {e}")))?;
        let max = max_lado.unwrap_or(1280).clamp(320, 4096);
        if img.width().max(img.height()) > max {
            img = img.resize(max, max, image::imageops::FilterType::Triangle);
        }
        let coef = LensCoefficients { k1: perfil.k1, k2: perfil.k2, k3: 0.0 };
        let out = apply_radial_correction_centered(&DynamicImage::ImageRgba8(img.to_rgba8()), coef, perfil.cx, perfil.cy);
        let destino = cache.join(format!("lente_previa_{}.png", &media_hash[..12]));
        out.save(&destino).map_err(|e| SicroError::Filesystem(format!("gravar prévia: {e}")))?;
        Ok(PreviaLente { absoluto: destino.to_string_lossy().into(), largura: out.width(), altura: out.height() })
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("prévia da lente: {e}")))?
}

/// Quadro do instante `t` com a lente do vídeo corrigida, gravado como resultado do realce.
#[tauri::command]
pub async fn realce_quadro_corrigido(
    app: AppHandle,
    workspace_path: String,
    media_hash: String,
    t: f64,
) -> Result<Arquivo> {
    let cache = pasta_cache(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<Arquivo> {
        let c = contexto(&workspace_path, &media_hash)?;
        let l = c.lente()?.ok_or_else(|| SicroError::Validation("este vídeo ainda não tem perfil de lente salvo".into()))?;
        let (png, w, h, real) = quadro_png(&c, &cache, t)?;
        let (rgb, _, _) = ler_rgb(&png)?;
        let rel = format!("{REALCE_SUBDIR}/{}_lente_{}_{}ms.png", c.stem(), carimbo(), ms(real));
        pasta_saida(&c, REALCE_SUBDIR)?;
        salvar_rgb(corrigir_rgb(&rgb, w, h, l.coef, l.cx, l.cy), w, h, &c.ws.join(&rel))?;
        let pj = json!({ "t": real, "k1": l.coef.k1, "k2": l.coef.k2, "cx": l.cx, "cy": l.cy, "modelo": "radial k1/k2 (Brown-Conrady)" });
        let r = registrar(&c, "quadro_lente", &rel, pj, AVISO_REALCE, Some((w, h)), true)?;
        Ok(arquivo(&c, r))
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("quadro corrigido: {e}")))?
}

// ---------------------------------------------------------------------------
// Perspectiva

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct ParIn {
    pub px: f64,
    pub py: f64,
    pub mx: f64,
    pub my: f64,
}

#[derive(Debug, Deserialize)]
pub struct ParamsPerspectiva {
    pub t: f64,
    pub com_lente: bool,
    /// Quatro pontos: pixel do quadro e metros no plano.
    pub pontos: Vec<ParIn>,
    /// Folga em volta do quadrilátero, em fração do maior lado.
    pub margem: f64,
    pub px_por_m: Option<f64>,
    /// De onde vieram as medidas ("manual" ou "calibracao:<id>").
    pub origem: String,
}

#[derive(Debug, Serialize)]
pub struct ResultadoPerspectiva {
    pub arquivo: Arquivo,
    pub info: perspective::InfoRetificada,
}

#[tauri::command]
pub async fn realce_perspectiva(
    app: AppHandle,
    workspace_path: String,
    media_hash: String,
    params: ParamsPerspectiva,
) -> Result<ResultadoPerspectiva> {
    let cache = pasta_cache(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<ResultadoPerspectiva> {
        if params.pontos.len() != 4 {
            return Err(SicroError::Validation("marque os 4 pontos do plano".into()));
        }
        let c = contexto(&workspace_path, &media_hash)?;
        let (png, w, h, real) = quadro_png(&c, &cache, params.t)?;
        let (mut rgb, _, _) = ler_rgb(&png)?;
        let lente = if params.com_lente { c.lente()? } else { None };
        if let Some(l) = &lente {
            rgb = corrigir_rgb(&rgb, w, h, l.coef, l.cx, l.cy);
        }
        let pares: [Par; 4] = std::array::from_fn(|i| {
            let p = params.pontos[i];
            Par { px: p.px, py: p.py, mx: p.mx, my: p.my }
        });
        let (img, info) = perspective::retificar(&rgb, w, h, &pares, params.margem, params.px_por_m)
            .map_err(SicroError::Validation)?;
        pasta_saida(&c, REALCE_SUBDIR)?;
        let rel = format!("{REALCE_SUBDIR}/{}_vista_de_cima_{}_{}ms.png", c.stem(), carimbo(), ms(real));
        img.save(c.ws.join(&rel)).map_err(|e| SicroError::Filesystem(format!("gravar retificada: {e}")))?;
        let pj = json!({
            "t": real, "pontos": params.pontos, "margem": params.margem, "origem_das_medidas": params.origem,
            "px_por_m": info.px_por_m, "mundo_m": info.mundo,
            "lente": lente.as_ref().map(|l| json!({ "k1": l.coef.k1, "k2": l.coef.k2 })),
            "metodo": "homografia DLT (4 pontos, normalização de Hartley)",
        });
        let r = registrar(&c, "retificada", &rel, pj, AVISO_REALCE, Some((info.largura, info.altura)), true)?;
        Ok(ResultadoPerspectiva { arquivo: arquivo(&c, r), info })
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("perspectiva: {e}")))?
}

// ---------------------------------------------------------------------------
// Vídeos derivados: estabilizado e com a lente corrigida

#[derive(Debug, Serialize)]
pub struct VideoDerivado {
    pub media: VideoMedia,
    pub arquivo: Arquivo,
    pub motor: String,
}

#[derive(Debug, Serialize)]
pub struct EstadoEstabilizacao {
    pub vidstab: bool,
    pub deshake: bool,
}

#[tauri::command]
pub async fn realce_estabilizacao_disponivel() -> Result<EstadoEstabilizacao> {
    tauri::async_runtime::spawn_blocking(|| EstadoEstabilizacao {
        vidstab: stabilize::tem_filtro("vidstabdetect") && stabilize::tem_filtro("vidstabtransform"),
        deshake: stabilize::tem_filtro("deshake"),
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("ffmpeg: {e}")))
}

/// Registra o arquivo gravado como vídeo do caso, ligado à origem (como o trecho exportado).
fn registrar_video_derivado(c: &Contexto, rel: &str, tipo: &str, derivacao: Value, aviso: &str) -> Result<(VideoMedia, Resultado)> {
    let abs = c.ws.join(rel);
    let sha256 = sha256_file(&abs)?;
    if let Some(existente) = video_repo::find_media_by_sha256(&c.conn, &c.occ, &sha256)? {
        return Err(SicroError::Validation(format!("este resultado já existe no caso ({})", existente.filename)));
    }
    let probe = crate::video::probe_media(&abs).ok();
    let agora = Utc::now();
    let size_bytes = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
    let nome = Path::new(rel).file_name().and_then(|s| s.to_str()).unwrap_or(rel).to_string();
    let mut deriv = derivacao.clone();
    deriv["kind"] = json!(tipo);
    deriv["source"] = json!({ "sha256": c.media.sha256, "filename": c.media.filename, "relative_path": c.media.relative_path });
    deriv["created_at"] = json!(agora.to_rfc3339());
    deriv["note"] = json!(aviso);
    let media = VideoMedia {
        id: Uuid::new_v4(),
        occurrence_id: c.occ,
        original_path: None,
        relative_path: rel.to_string(),
        filename: nome,
        sha256,
        size_bytes,
        duration_s: probe.as_ref().and_then(|p| p.duration_s),
        codec: probe.as_ref().and_then(|p| p.video_codec.clone()),
        width: probe.as_ref().and_then(|p| p.width),
        height: probe.as_ref().and_then(|p| p.height),
        pixel_format: probe.as_ref().and_then(|p| p.pixel_format.clone()),
        fps_declared: probe.as_ref().and_then(|p| p.fps_declared),
        avg_frame_rate: probe.as_ref().and_then(|p| p.avg_frame_rate.clone()),
        r_frame_rate: probe.as_ref().and_then(|p| p.r_frame_rate.clone()),
        time_base: probe.as_ref().and_then(|p| p.time_base.clone()),
        frame_count: probe.as_ref().and_then(|p| p.frame_count),
        bitrate: probe.as_ref().and_then(|p| p.bitrate),
        raw_probe_json: probe.as_ref().map(|p| p.raw_json.clone()).unwrap_or_else(|| "{}".into()),
        warnings_json: serde_json::to_string(&probe.as_ref().map(|p| p.warnings.clone()).unwrap_or_default()).unwrap_or_else(|_| "[]".into()),
        created_at: agora,
        updated_at: agora,
        derived_from_hash: Some(c.media.sha256.clone()),
        derivation_json: Some(deriv.to_string()),
    };
    video_repo::insert_media(&c.conn, &media)?;
    let r = registrar(c, tipo, rel, derivacao, aviso, media.width.zip(media.height), false)?;
    Ok((media, r))
}

#[derive(Debug, Deserialize)]
pub struct ParamsEstabilizar {
    /// Quadros de cada lado usados para suavizar (1 a 60).
    pub suavizar: u32,
    pub borda: Borda,
}

#[tauri::command]
pub async fn realce_estabilizar(
    app: AppHandle,
    workspace_path: String,
    media_hash: String,
    params: ParamsEstabilizar,
) -> Result<VideoDerivado> {
    let flag = iniciar_tarefa()?;
    let (app2, hash2) = (app.clone(), media_hash.clone());
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<VideoDerivado> {
        let c = contexto(&workspace_path, &hash2)?;
        let video = c.video()?;
        let dir = pasta_saida(&c, ESTAB_SUBDIR)?;
        let saida = stabilize::nome_unico(&dir, &format!("{}_estabilizado", c.stem()), "mp4");
        let rel = format!("{ESTAB_SUBDIR}/{}", saida.file_name().and_then(|s| s.to_str()).unwrap_or("saida.mp4"));
        let total = c.media.duration_s.unwrap_or(0.0);
        let tem_audio = tem_audio(&c.media.raw_probe_json);
        let dims = (c.media.width.unwrap_or(0), c.media.height.unwrap_or(0));
        let est = stabilize::estabilizar(
            &video,
            &saida,
            OpcoesEstabilizar { suavizar: params.suavizar, borda: params.borda },
            total,
            tem_audio,
            dims,
            &flag,
            &mut |fase, f| emitir(&app2, &hash2, if fase == 1 { "estab-analise" } else { "estab-grava" }, (f * 1000.0) as u32, 1000),
        );
        let est = match est {
            Ok(e) => e,
            Err(e) => {
                let _ = std::fs::remove_file(&saida);
                return Err(e);
            }
        };
        let pj = json!({
            "suavizar": params.suavizar, "borda": params.borda, "motor": est.motor,
            "comandos": est.comandos, "codec": "H.264 CRF 16 (recomprimido)",
        });
        let (media, r) = registrar_video_derivado(
            &c, &rel, "estabilizado", pj,
            "Cópia estabilizada e recomprimida: os pixels não são os do original, a borda foi cortada ou preenchida e o movimento da câmera foi alterado. O original está intacto.",
        )?;
        Ok(VideoDerivado { arquivo: arquivo(&c, r), media, motor: est.motor.to_string() })
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("estabilização: {e}")));
    encerrar_tarefa();
    r?
}

/// Cópia do vídeo com a lente corrigida (ffmpeg `lenscorrection`), usando o perfil salvo.
#[tauri::command]
pub async fn realce_lente_video(app: AppHandle, workspace_path: String, media_hash: String) -> Result<VideoDerivado> {
    let flag = iniciar_tarefa()?;
    let (app2, hash2) = (app.clone(), media_hash.clone());
    let r = tauri::async_runtime::spawn_blocking(move || -> Result<VideoDerivado> {
        let c = contexto(&workspace_path, &hash2)?;
        let video = c.video()?;
        let l = c.lente()?.ok_or_else(|| SicroError::Validation("este vídeo ainda não tem perfil de lente salvo".into()))?;
        let (w, h) = (c.media.width.unwrap_or(0), c.media.height.unwrap_or(0));
        let (k1, k2) = para_ffmpeg(l.coef, w.max(2), h.max(2))
            .ok_or_else(|| SicroError::Validation("a correção é forte demais para o filtro do ffmpeg (k fora de −1..1)".into()))?;
        let dir = pasta_saida(&c, ESTAB_SUBDIR)?;
        let saida = stabilize::nome_unico(&dir, &format!("{}_lente", c.stem()), "mp4");
        let rel = format!("{ESTAB_SUBDIR}/{}", saida.file_name().and_then(|s| s.to_str()).unwrap_or("saida.mp4"));
        let tem_audio = tem_audio(&c.media.raw_probe_json);
        let cmd = stabilize::corrigir_lente(
            &video, &saida, k1, k2, l.cx as f64, l.cy as f64, c.media.duration_s.unwrap_or(0.0), tem_audio, &flag,
            &mut |f| emitir(&app2, &hash2, "lente-video", (f * 1000.0) as u32, 1000),
        );
        let cmd = match cmd {
            Ok(x) => x,
            Err(e) => {
                let _ = std::fs::remove_file(&saida);
                return Err(e);
            }
        };
        let pj = json!({
            "k1": l.coef.k1, "k2": l.coef.k2, "cx": l.cx, "cy": l.cy, "k1_ffmpeg": k1, "k2_ffmpeg": k2,
            "comando": cmd, "codec": "H.264 CRF 16 (recomprimido)",
        });
        let (media, r) = registrar_video_derivado(
            &c, &rel, "lente_video", pj,
            "Cópia com a distorção da lente corrigida e recomprimida: os pixels não são os do original. O original está intacto.",
        )?;
        Ok(VideoDerivado { arquivo: arquivo(&c, r), media, motor: "lenscorrection".into() })
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("cópia com lente: {e}")));
    encerrar_tarefa();
    r?
}
