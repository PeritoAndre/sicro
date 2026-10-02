//! Comandos Tauri do módulo Áudio. Determinístico e com cadeia de custódia (hash,
//! cópia para o workspace, ffprobe, log + auditoria); nada de interpretação.

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::audio::analysis::{self, AudioMeasurements, SpectrumResult};
use crate::audio::enf::{EnfMatch, EnfResult};
use crate::audio::{convert_to_wav, extract_audio_to_wav, probe_audio};
use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::{audio_repo, occurrence_repo};
use crate::error::{Result, SicroError};
use crate::hashing::sha256::sha256_file;
use crate::models::{
    AudioDiarization, AudioEnhancement, AudioMarker, AudioMedia, AudioTranscriptSegment,
    TranscriptSegmentInput,
};
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};

const AUDIO_ORIG_SUBDIR: &str = "audio/originais";
const AUDIO_WAV_SUBDIR: &str = "audio/wav";
const AUDIO_SPECTRO_SUBDIR: &str = "audio/espectrogramas";

/// Extrai a trilha de áudio de um vídeo para WAV de análise. `source_video_sha256`
/// registra a proveniência quando o vídeo já está no caso.
#[tauri::command]
pub async fn extract_audio_from_video(
    workspace_path: String,
    video_path: String,
    source_video_sha256: Option<String>,
) -> Result<AudioMedia> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let source = PathBuf::from(&video_path);
    if !source.is_file() {
        return Err(SicroError::Filesystem(format!(
            "vídeo não encontrado: {}",
            source.display()
        )));
    }
    // Vídeo só com imagem: o ffmpeg daria "Output file does not contain any
    // stream", incompreensível para o perito.
    if !crate::audio::has_audio_stream(&source)? {
        return Err(SicroError::Validation(format!(
            "o vídeo \"{}\" não tem trilha de áudio — só imagem. Não há áudio para extrair.",
            source.file_name().and_then(|n| n.to_str()).unwrap_or("?")
        )));
    }

    let wav_dir = ws.join(AUDIO_WAV_SUBDIR);
    create_dir(&wav_dir)?;
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("audio");
    let wav_name = unique_name(&wav_dir, &format!("{stem}.wav"));
    let wav_path = wav_dir.join(&wav_name);

    extract_audio_to_wav(&source, &wav_path)?;
    let sha256 = sha256_file(&wav_path)?;

    if let Some(existing) =
        audio_repo::find_media_by_sha256(&conn, &occurrence_id, &sha256)?
    {
        let _ = std::fs::remove_file(&wav_path);
        return Err(SicroError::Validation(format!(
            "este áudio já foi extraído nesta ocorrência (id {}).",
            existing.id
        )));
    }

    let media = build_and_persist(
        &conn,
        occurrence_id,
        "extraido",
        Some(video_path),
        None,
        None,
        source_video_sha256,
        &wav_path,
        &wav_name,
        sha256,
        "audio.extract",
    )?;
    Ok(media)
}

/// Importa um áudio externo (WhatsApp/gravador). Preserva o ORIGINAL e gera um
/// WAV de análise determinístico.
#[tauri::command]
pub async fn import_audio_file(
    workspace_path: String,
    source_path: String,
) -> Result<AudioMedia> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let source = PathBuf::from(&source_path);
    if !source.is_file() {
        return Err(SicroError::Filesystem(format!(
            "arquivo de áudio não encontrado: {}",
            source.display()
        )));
    }

    // Hash do ORIGINAL (a evidência); dedupe por ele.
    let original_sha256 = sha256_file(&source)?;
    if let Some(existing) =
        audio_repo::find_media_by_original_sha256(&conn, &occurrence_id, &original_sha256)?
    {
        return Err(SicroError::Validation(format!(
            "este áudio já foi importado nesta ocorrência (id {}).",
            existing.id
        )));
    }

    let orig_dir = ws.join(AUDIO_ORIG_SUBDIR);
    create_dir(&orig_dir)?;
    let orig_name = unique_name(
        &orig_dir,
        source.file_name().and_then(|s| s.to_str()).unwrap_or("audio.bin"),
    );
    let orig_path = orig_dir.join(&orig_name);
    std::fs::copy(&source, &orig_path).map_err(|e| {
        SicroError::Filesystem(format!("não foi possível copiar o original: {e}"))
    })?;

    let wav_dir = ws.join(AUDIO_WAV_SUBDIR);
    create_dir(&wav_dir)?;
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let wav_name = unique_name(&wav_dir, &format!("{stem}.wav"));
    let wav_path = wav_dir.join(&wav_name);
    convert_to_wav(&orig_path, &wav_path)?;
    let sha256 = sha256_file(&wav_path)?;

    if let Some(existing) =
        audio_repo::find_media_by_sha256(&conn, &occurrence_id, &sha256)?
    {
        let _ = std::fs::remove_file(&wav_path);
        let _ = std::fs::remove_file(&orig_path);
        return Err(SicroError::Validation(format!(
            "áudio equivalente já existe nesta ocorrência (id {}).",
            existing.id
        )));
    }

    let media = build_and_persist(
        &conn,
        occurrence_id,
        "importado",
        Some(source_path),
        Some(format!("{AUDIO_ORIG_SUBDIR}/{orig_name}")),
        Some(original_sha256),
        None,
        &wav_path,
        &wav_name,
        sha256,
        "audio.import",
    )?;
    Ok(media)
}

#[tauri::command]
pub async fn list_audio_media(workspace_path: String) -> Result<Vec<AudioMedia>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    audio_repo::list_for_occurrence(&conn, &manifest.occurrence_id)
}

#[tauri::command]
pub async fn open_audio_media(
    workspace_path: String,
    audio_id: String,
) -> Result<AudioMedia> {
    let ws = PathBuf::from(&workspace_path);
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    let id = Uuid::parse_str(&audio_id)
        .map_err(|e| SicroError::Validation(format!("id de áudio inválido: {e}")))?;
    audio_repo::find_media_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("áudio {audio_id} não encontrado")))
}

// ---------------------------------------------------------------------------
// Espectrograma

/// Gera o espectrograma PNG do WAV de análise (FFmpeg `showspectrumpic`) e
/// devolve o caminho relativo. Determinístico; não interpreta o conteúdo.
#[tauri::command]
pub async fn audio_spectrogram(
    workspace_path: String,
    audio_id: String,
) -> Result<String> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let id = Uuid::parse_str(&audio_id)
        .map_err(|e| SicroError::Validation(format!("id de áudio inválido: {e}")))?;
    let media = audio_repo::find_media_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation("áudio não encontrado".into()))?;
    let wav_abs = ws.join(&media.relative_path);
    if !wav_abs.is_file() {
        return Err(SicroError::Filesystem(format!(
            "WAV de análise ausente: {}",
            wav_abs.display()
        )));
    }

    let dir = ws.join(AUDIO_SPECTRO_SUBDIR);
    create_dir(&dir)?;
    let stem = Path::new(&media.filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("audio");
    let out_name = format!("{stem}.png");
    let out_path = dir.join(&out_name);
    crate::audio::spectrogram_png(&wav_abs, &out_path)?;

    audio_repo::insert_log(
        &conn,
        &occurrence_id,
        Some(&media.sha256),
        "audio.spectrogram",
        &json!({ "file": out_name }).to_string(),
    )?;
    Ok(format!("{AUDIO_SPECTRO_SUBDIR}/{out_name}"))
}

// ---------------------------------------------------------------------------
// Análise (medição/espectro/ENF): só lê o WAV de análise e devolve números
// determinísticos; cada chamada vai para o log do áudio.

/// Resolve um `audio_id` → (mídia, caminho absoluto do WAV de análise).
fn resolve_wav(
    ws: &Path,
    conn: &rusqlite::Connection,
    audio_id: &str,
) -> Result<(AudioMedia, PathBuf)> {
    let id = Uuid::parse_str(audio_id)
        .map_err(|e| SicroError::Validation(format!("id de áudio inválido: {e}")))?;
    let media = audio_repo::find_media_by_id(conn, &id)?
        .ok_or_else(|| SicroError::Validation("áudio não encontrado".into()))?;
    let wav_abs = ws.join(&media.relative_path);
    if !wav_abs.is_file() {
        return Err(SicroError::Filesystem(format!(
            "WAV de análise ausente: {}",
            wav_abs.display()
        )));
    }
    Ok((media, wav_abs))
}

/// Medições objetivas: pico/RMS (dBFS), offset DC, clipping, fator de crista, duração.
#[tauri::command]
pub async fn audio_measure(
    workspace_path: String,
    audio_id: String,
) -> Result<AudioMeasurements> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let (media, wav_abs) = resolve_wav(&ws, &conn, &audio_id)?;
    let (samples, sr, ch) = analysis::read_wav_mono(&wav_abs)?;
    let mut m = analysis::measure(&samples, sr, ch, 0.997);
    // Ruído de fundo, loudness e silêncios (FFmpeg); sem ffmpeg, fica só o básico.
    m.extended = crate::audio::extended_measure(&wav_abs, m.duration_s).ok();
    audio_repo::insert_log(
        &conn,
        &manifest.occurrence_id,
        Some(&media.sha256),
        "audio.measure",
        &json!({
            "peak_dbfs": m.peak_dbfs,
            "rms_dbfs": m.rms_dbfs,
            "dc_offset": m.dc_offset,
            "clipped_samples": m.clipped_samples,
        })
        .to_string(),
    )?;
    Ok(m)
}

/// Espectrograma interativo da janela [t0, t1], no tamanho pedido pela tela.
/// Só lê; não registra log (é chamado a cada zoom e arraste).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn audio_spectrogram_data(
    workspace_path: String,
    audio_id: String,
    t0: f64,
    t1: f64,
    width: usize,
    height: usize,
    fft_size: usize,
    log_freq: bool,
    f_max: Option<f32>,
) -> Result<crate::audio::spectro::SpectroImage> {
    let ws = PathBuf::from(&workspace_path);
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let (_media, wav_abs) = resolve_wav(&ws, &conn, &audio_id)?;
    let req = crate::audio::spectro::Request { t0, t1, width, height, fft_size, log_freq, f_max };
    tauri::async_runtime::spawn_blocking(move || crate::audio::spectro::render(&wav_abs, &req))
        .await
        .map_err(|e| SicroError::Validation(format!("falha ao calcular o espectrograma: {e}")))?
}

/// Espectro (Welch FFT) de um áudio inteiro. `fft_size` potência de 2.
#[tauri::command]
pub async fn audio_spectrum(
    workspace_path: String,
    audio_id: String,
    fft_size: Option<usize>,
) -> Result<SpectrumResult> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let (media, wav_abs) = resolve_wav(&ws, &conn, &audio_id)?;
    let (samples, sr, _ch) = analysis::read_wav_mono(&wav_abs)?;
    let sp = analysis::spectrum(&samples, sr, fft_size.unwrap_or(4096));
    audio_repo::insert_log(
        &conn,
        &manifest.occurrence_id,
        Some(&media.sha256),
        "audio.spectrum",
        &json!({ "fft_size": sp.fft_size, "peak_freq_hz": sp.peak_freq_hz })
            .to_string(),
    )?;
    Ok(sp)
}

/// ENF (frequência da rede elétrica gravada como zumbido): curva, confiança,
/// variações bruscas e trechos sem ENF. `nominal_hz` None = automático (50/60).
#[tauri::command]
pub async fn audio_enf(
    workspace_path: String,
    audio_id: String,
    nominal_hz: Option<f32>,
) -> Result<EnfResult> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let (media, wav_abs) = resolve_wav(&ws, &conn, &audio_id)?;
    let e = tauri::async_runtime::spawn_blocking(move || -> Result<EnfResult> {
        let (x, sr) = crate::audio::enf::load_for_enf(&wav_abs)?;
        Ok(crate::audio::enf::extract(&x, sr, nominal_hz))
    })
    .await
    .map_err(|e| SicroError::Validation(format!("tarefa do ENF: {e}")))??;
    audio_repo::insert_log(
        &conn,
        &manifest.occurrence_id,
        Some(&media.sha256),
        "audio.enf",
        &json!({
            "nominal_hz": e.nominal_hz,
            "automatico": e.auto,
            "harmonicos": e.harmonics.iter().map(|h| h.k).collect::<Vec<_>>(),
            "confianca": e.confidence,
            "mean_hz": e.mean_hz,
            "std_hz": e.std_hz,
            "variacoes_bruscas": e.jumps.len(),
            "janela_s": e.window_s,
            "passo_s": e.step_s,
        })
        .to_string(),
    )?;
    Ok(e)
}

#[derive(Debug, Clone, Serialize)]
pub struct EnfComparison {
    pub question: EnfResult,
    pub reference: EnfResult,
    /// None = curvas curtas/sem ENF suficiente para comparar.
    pub matching: Option<EnfMatch>,
}

/// Compara o ENF do áudio com o de uma gravação de REFERÊNCIA da rede (outro
/// áudio do caso, mais longo): onde o áudio se encaixa nela e quão bem.
#[tauri::command]
pub async fn audio_enf_compare(
    workspace_path: String,
    audio_id: String,
    reference_audio_id: String,
    nominal_hz: Option<f32>,
) -> Result<EnfComparison> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let (media, wav_q) = resolve_wav(&ws, &conn, &audio_id)?;
    let (ref_media, wav_r) = resolve_wav(&ws, &conn, &reference_audio_id)?;
    let cmp = tauri::async_runtime::spawn_blocking(move || -> Result<EnfComparison> {
        let (xq, srq) = crate::audio::enf::load_for_enf(&wav_q)?;
        let question = crate::audio::enf::extract(&xq, srq, nominal_hz);
        let (xr, srr) = crate::audio::enf::load_for_enf(&wav_r)?;
        let reference = crate::audio::enf::extract(&xr, srr, Some(question.nominal_hz));
        let matching = crate::audio::enf::compare(&question, &reference);
        Ok(EnfComparison { question, reference, matching })
    })
    .await
    .map_err(|e| SicroError::Validation(format!("tarefa do ENF: {e}")))??;
    audio_repo::insert_log(
        &conn,
        &manifest.occurrence_id,
        Some(&media.sha256),
        "audio.enf.compare",
        &json!({
            "referencia_sha256": ref_media.sha256,
            "referencia": ref_media.filename,
            "nominal_hz": cmp.question.nominal_hz,
            "encaixe_s": cmp.matching.as_ref().map(|m| m.offset_s),
            "correlacao": cmp.matching.as_ref().map(|m| m.correlation),
            "segunda_correlacao": cmp.matching.as_ref().map(|m| m.second_correlation),
            "diferenca_media_hz": cmp.matching.as_ref().map(|m| m.mean_abs_diff_hz),
        })
        .to_string(),
    )?;
    Ok(cmp)
}

/// Relatório de autenticidade: estrutura do arquivo ORIGINAL (ou do vídeo de
/// origem) e detectores sobre o sinal. Só indícios — não conclui sobre edição.
#[tauri::command]
pub async fn audio_authenticity(
    workspace_path: String,
    audio_id: String,
) -> Result<crate::audio::authenticity::Report> {
    use crate::audio::authenticity as au;
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let (media, wav_abs) = resolve_wav(&ws, &conn, &audio_id)?;

    let mut extra_notes = Vec::new();
    let (kind, file) = match media.kind.as_str() {
        "importado" => match media.original_relative_path.as_ref().map(|r| ws.join(r)) {
            Some(p) if p.is_file() => ("original importado", p),
            _ => {
                extra_notes.push("O arquivo original importado não está na pasta do caso; a estrutura examinada é a do WAV de trabalho.".to_string());
                ("WAV de trabalho", wav_abs.clone())
            }
        },
        "extraido" => {
            let video = match media.source_video_sha256.as_deref() {
                Some(h) => crate::database::repositories::video_repo::find_media_by_sha256(&conn, &manifest.occurrence_id, h)?,
                None => None,
            };
            match video.map(|v| ws.join(&v.relative_path)) {
                Some(p) if p.is_file() => ("vídeo de origem", p),
                _ => {
                    extra_notes.push("O vídeo de origem não foi encontrado no caso; a estrutura examinada é a do WAV de trabalho.".to_string());
                    ("WAV de trabalho", wav_abs.clone())
                }
            }
        }
        _ => {
            extra_notes.push(format!(
                "Este áudio foi gerado pelo SICRO ({}): a estrutura e parte dos sinais refletem o processamento. Para autenticidade, examine o áudio original.",
                media.kind
            ));
            ("derivado do SICRO", wav_abs.clone())
        }
    };

    let file2 = file.clone();
    let wav2 = wav_abs.clone();
    let mut report = tauri::async_runtime::spawn_blocking(move || -> Result<au::Report> {
        let (structure, packets) = au::probe(&file2)?;
        let (decode_errors, decode_error_samples) = au::decode_errors(&file2)?;
        let (samples, sr, _) = analysis::read_wav_mono(&wav2)?;
        let all_clicks = au::clicks(&samples, sr);
        let silences = au::digital_silences(&samples, sr);
        Ok(au::Report {
            source_kind: String::new(),
            source_file: String::new(),
            structure,
            packets,
            decode_errors,
            decode_error_samples,
            bandwidth: au::bandwidth(&samples, sr),
            clicks_total: all_clicks.len(),
            clicks: all_clicks.into_iter().take(200).collect(),
            digital_silences_total: silences.len(),
            digital_silences: silences.into_iter().take(100).collect(),
            noise_jumps: au::noise_jumps(&samples, sr),
            notes: Vec::new(),
        })
    })
    .await
    .map_err(|e| SicroError::Validation(format!("tarefa de autenticidade: {e}")))??;
    report.source_kind = kind.to_string();
    report.source_file = file.file_name().and_then(|f| f.to_str()).unwrap_or("").to_string();
    report.notes = extra_notes;
    report.notes.extend(au::notes(&report));

    audio_repo::insert_log(
        &conn,
        &manifest.occurrence_id,
        Some(&media.sha256),
        "audio.authenticity",
        &json!({
            "examinado": report.source_kind,
            "arquivo": report.source_file,
            "corte_hz": report.bandwidth.cutoff_hz,
            "lacunas_pacotes": report.packets.as_ref().map(|p| p.gaps.len()),
            "erros_decodificacao": report.decode_errors,
            "cliques": report.clicks_total,
            "silencios_digitais": report.digital_silences_total,
            "saltos_ruido": report.noise_jumps.len(),
        })
        .to_string(),
    )?;
    Ok(report)
}

// ---------------------------------------------------------------------------
// Recorte de trecho

/// Recorta o trecho [start_s, end_s] do áudio num NOVO clipe (`kind="recorte"`),
/// com hash + custódia + proveniência. NÃO-destrutivo: o original permanece.
#[tauri::command]
pub async fn extract_audio_clip(
    workspace_path: String,
    audio_id: String,
    start_s: f64,
    end_s: f64,
) -> Result<AudioMedia> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    if !(end_s > start_s) || start_s < 0.0 {
        return Err(SicroError::Validation(
            "trecho inválido: defina início < fim (use o loop A-B no player).".into(),
        ));
    }

    let id = Uuid::parse_str(&audio_id)
        .map_err(|e| SicroError::Validation(format!("id de áudio inválido: {e}")))?;
    let source = audio_repo::find_media_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation("áudio de origem não encontrado".into()))?;
    let src_abs = ws.join(&source.relative_path);
    if !src_abs.is_file() {
        return Err(SicroError::Filesystem(format!(
            "WAV de análise ausente: {}",
            src_abs.display()
        )));
    }

    let wav_dir = ws.join(AUDIO_WAV_SUBDIR);
    create_dir(&wav_dir)?;
    let stem = Path::new(&source.filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("audio");
    let out_name = unique_name(
        &wav_dir,
        &format!("{stem}-trecho-{start_s:.0}s-{end_s:.0}s.wav"),
    );
    let out_path = wav_dir.join(&out_name);

    crate::audio::extract_clip_wav(&src_abs, &out_path, start_s, end_s)?;
    let sha256 = sha256_file(&out_path)?;
    if let Some(existing) = audio_repo::find_media_by_sha256(&conn, &occurrence_id, &sha256)? {
        let _ = std::fs::remove_file(&out_path);
        return Err(SicroError::Validation(format!(
            "este trecho já existe nesta ocorrência (id {}).",
            existing.id
        )));
    }

    let media = build_and_persist(
        &conn,
        occurrence_id,
        "recorte",
        Some(format!("trecho {start_s:.2}s–{end_s:.2}s de {}", source.filename)),
        None,
        None,
        None,
        &out_path,
        &out_name,
        sha256,
        "audio.clip",
    )?;
    Ok(media)
}

/// Um trecho a compilar: áudio de origem + intervalo + rótulo opcional.
#[derive(Deserialize)]
pub struct ClipSegmentInput {
    pub audio_id: String,
    pub start_s: f64,
    pub end_s: f64,
    #[serde(default)]
    pub label: String,
}

struct ResolvedClip {
    src: PathBuf,
    start: f64,
    end: f64,
    label: String,
    filename: String,
    sha: String,
}

/// Compila trechos num derivado `kind="compilacao"` com hash, custódia e manifesto
/// `.compilacao.json`. Não disfarça a montagem: pausa audível entre trechos.
#[tauri::command]
pub async fn compile_audio_clips(
    workspace_path: String,
    segments: Vec<ClipSegmentInput>,
    gap_ms: Option<u64>,
) -> Result<AudioMedia> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    if segments.len() < 2 {
        return Err(SicroError::Validation(
            "a compilação precisa de pelo menos 2 trechos.".into(),
        ));
    }
    if segments.len() > 50 {
        return Err(SicroError::Validation(
            "máximo de 50 trechos por compilação.".into(),
        ));
    }
    let gap_s = gap_ms.unwrap_or(500) as f64 / 1000.0;

    let mut resolved: Vec<ResolvedClip> = Vec::with_capacity(segments.len());
    for (i, seg) in segments.iter().enumerate() {
        if !(seg.end_s > seg.start_s) || seg.start_s < 0.0 {
            return Err(SicroError::Validation(format!(
                "trecho {} inválido: defina início < fim.",
                i + 1
            )));
        }
        let id = Uuid::parse_str(&seg.audio_id)
            .map_err(|e| SicroError::Validation(format!("id de áudio inválido: {e}")))?;
        let m = audio_repo::find_media_by_id(&conn, &id)?.ok_or_else(|| {
            SicroError::Validation(format!("trecho {}: áudio de origem não encontrado.", i + 1))
        })?;
        let src = ws.join(&m.relative_path);
        if !src.is_file() {
            return Err(SicroError::Filesystem(format!(
                "WAV de análise ausente: {}",
                src.display()
            )));
        }
        resolved.push(ResolvedClip {
            src,
            start: seg.start_s,
            end: seg.end_s,
            label: seg.label.trim().to_string(),
            filename: m.filename,
            sha: m.sha256,
        });
    }

    let wav_dir = ws.join(AUDIO_WAV_SUBDIR);
    create_dir(&wav_dir)?;
    let out_name = unique_name(&wav_dir, "compilacao-rotulada.wav");
    let out_path = wav_dir.join(&out_name);

    let segs: Vec<(PathBuf, f64, f64)> = resolved
        .iter()
        .map(|r| (r.src.clone(), r.start, r.end))
        .collect();
    crate::audio::concat_clips_wav(&segs, gap_s, &out_path)?;

    let sha256 = sha256_file(&out_path)?;
    if let Some(existing) = audio_repo::find_media_by_sha256(&conn, &occurrence_id, &sha256)? {
        let _ = std::fs::remove_file(&out_path);
        return Err(SicroError::Validation(format!(
            "esta compilação (mesmos trechos e ordem) já existe nesta ocorrência (id {}).",
            existing.id
        )));
    }

    write_compilation_manifest(&out_path, &resolved, gap_s, &sha256)?;

    let media = build_and_persist(
        &conn,
        occurrence_id,
        "compilacao",
        Some(format!(
            "compilação rotulada de {} trechos (ver .compilacao.json)",
            resolved.len()
        )),
        None,
        None,
        None,
        &out_path,
        &out_name,
        sha256,
        "audio.compile",
    )?;
    Ok(media)
}

/// Manifesto JSON ao lado do WAV compilado: origem e tempos de cada trecho,
/// o "rótulo" que torna a montagem reproduzível.
fn write_compilation_manifest(
    out_wav: &Path,
    resolved: &[ResolvedClip],
    gap_s: f64,
    sha256: &str,
) -> Result<()> {
    let trechos: Vec<serde_json::Value> = resolved
        .iter()
        .enumerate()
        .map(|(i, r)| {
            json!({
                "ordem": i + 1,
                "rotulo": r.label,
                "origem_arquivo": r.filename,
                "origem_sha256": r.sha,
                "inicio_s": r.start,
                "fim_s": r.end,
                "duracao_s": (r.end - r.start),
            })
        })
        .collect();
    let doc = json!({
        "tipo": "compilacao_rotulada",
        "gerado_por": "SICRO 2.0 — módulo Áudio",
        "formato": "WAV PCM 16-bit, 44100 Hz, mono (normalizado na compilação)",
        "gap_entre_trechos_ms": (gap_s * 1000.0).round() as u64,
        "sha256_compilacao": sha256,
        "trechos": trechos,
        "observacao": "Montagem de trechos selecionados pelo perito. Os áudios de \
origem permanecem intactos e com hash próprio. A ordem e os limites de cada \
trecho estão documentados acima para reprodutibilidade. NÃO constitui áudio \
contínuo original.",
    });
    let bytes = serde_json::to_vec_pretty(&doc)
        .map_err(|e| SicroError::Filesystem(format!("manifesto da compilação: {e}")))?;
    let path = out_wav.with_extension("compilacao.json");
    std::fs::write(&path, bytes)
        .map_err(|e| SicroError::Filesystem(format!("manifesto da compilação: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Marcadores

#[tauri::command]
pub async fn add_audio_marker(
    workspace_path: String,
    audio_sha256: String,
    t_seconds: f64,
    label: String,
) -> Result<AudioMarker> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let marker = AudioMarker {
        id: Uuid::new_v4(),
        occurrence_id: manifest.occurrence_id,
        audio_sha256,
        t_seconds,
        label,
        created_at: Utc::now(),
    };
    audio_repo::insert_marker(&conn, &marker)?;
    Ok(marker)
}

#[tauri::command]
pub async fn list_audio_markers(
    workspace_path: String,
    audio_sha256: String,
) -> Result<Vec<AudioMarker>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    audio_repo::list_markers_for_audio(&conn, &manifest.occurrence_id, &audio_sha256)
}

#[tauri::command]
pub async fn delete_audio_marker(workspace_path: String, marker_id: String) -> Result<()> {
    let ws = PathBuf::from(&workspace_path);
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let id = Uuid::parse_str(&marker_id)
        .map_err(|e| SicroError::Validation(format!("id de marcador inválido: {e}")))?;
    audio_repo::delete_marker(&conn, &id)
}

// ---------------------------------------------------------------------------
// Realce (auxílio de escuta — NÃO-destrutivo)

/// Trecho só de ruído (A–B do player) para a redução de ruído por amostra.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct NoiseProfileInput {
    pub start_s: f64,
    pub end_s: f64,
}

/// Derivado realçado (`kind="realce"`): original intacto, cadeia exata de filtros
/// gravada em `audio_enhancements`. Auxílio de escuta — não "limpa" nem "recupera".
#[tauri::command]
pub async fn enhance_audio(
    workspace_path: String,
    source_audio_id: String,
    filters: Vec<String>,
    noise_profile: Option<NoiseProfileInput>,
) -> Result<AudioMedia> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let id = Uuid::parse_str(&source_audio_id)
        .map_err(|e| SicroError::Validation(format!("id de áudio inválido: {e}")))?;
    let source = audio_repo::find_media_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation("áudio de origem não encontrado".into()))?;


    let src_abs = ws.join(&source.relative_path);
    if !src_abs.is_file() {
        return Err(SicroError::Filesystem(format!(
            "WAV de análise ausente: {}",
            src_abs.display()
        )));
    }

    let wav_dir = ws.join(AUDIO_WAV_SUBDIR);
    create_dir(&wav_dir)?;
    let stem = Path::new(&source.filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("audio");
    let out_name = unique_name(&wav_dir, &format!("{stem}-realce.wav"));
    let out_path = wav_dir.join(&out_name);

    let profile = noise_profile.map(|p| crate::audio::enhance::NoiseProfile {
        start_s: p.start_s.min(p.end_s),
        end_s: p.start_s.max(p.end_s),
    });
    let recipe = crate::audio::enhance::run(&src_abs, &out_path, &filters, profile)?;
    let sha256 = sha256_file(&out_path)?;
    if let Some(existing) = audio_repo::find_media_by_sha256(&conn, &occurrence_id, &sha256)? {
        let _ = std::fs::remove_file(&out_path);
        return Err(SicroError::Validation(format!(
            "este realce (mesma cadeia de filtros) já existe nesta ocorrência (id {}).",
            existing.id
        )));
    }

    let media = build_and_persist(
        &conn,
        occurrence_id,
        "realce",
        Some(format!("realce de {}", source.filename)),
        None,
        None,
        None,
        &out_path,
        &out_name,
        sha256,
        "audio.enhance",
    )?;

    let enh = AudioEnhancement {
        id: Uuid::new_v4(),
        occurrence_id,
        source_audio_sha256: source.sha256.clone(),
        output_audio_sha256: media.sha256.clone(),
        filters_json: json!({ "keys": filters, "receita": recipe }).to_string(),
        created_at: Utc::now(),
    };
    audio_repo::insert_enhancement(&conn, &enh)?;

    Ok(media)
}

// ---------------------------------------------------------------------------
// Degravação (transcrição assistida MANUAL)

#[tauri::command]
pub async fn list_audio_transcript(
    workspace_path: String,
    audio_sha256: String,
) -> Result<Vec<AudioTranscriptSegment>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    audio_repo::list_segments(&conn, &manifest.occurrence_id, &audio_sha256)
}

/// Salva (substitui) toda a degravação MANUAL — a transcrição é trabalho do
/// perito. Devolve os segmentos persistidos (com ids) para o front reidratar.
#[tauri::command]
pub async fn save_audio_transcript(
    workspace_path: String,
    audio_sha256: String,
    segments: Vec<TranscriptSegmentInput>,
) -> Result<Vec<AudioTranscriptSegment>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    audio_repo::replace_segments(&mut conn, &occurrence_id, &audio_sha256, &segments)?;

    audio_repo::insert_log(
        &conn,
        &occurrence_id,
        Some(&audio_sha256),
        "audio.transcript.save",
        &json!({ "segments": segments.len() }).to_string(),
    )?;
    occurrence_repo::record_audit(
        &conn,
        Some(&occurrence_id),
        "audio.transcript.save",
        Some("audio"),
        Some("audio_transcript_segments"),
        None,
        Some(&audio_sha256),
    )?;

    audio_repo::list_segments(&conn, &occurrence_id, &audio_sha256)
}

// ---------------------------------------------------------------------------
// Transcrição assistida por IA (whisper.cpp local) — RASCUNHO

#[derive(Debug, Clone, Deserialize)]
pub struct TranscribeOptions {
    /// Caminho do modelo GGUF do whisper (obrigatório).
    pub model_path: String,
    /// Executável whisper.cpp; ausente = procura no PATH.
    #[serde(default)]
    pub whisper_bin: Option<String>,
    /// Idioma (default "pt").
    #[serde(default)]
    pub language: Option<String>,
    /// Modelo VAD (silero) opcional — anti-alucinação.
    #[serde(default)]
    pub vad_model_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WhisperStatus {
    pub available: bool,
    pub path: Option<String>,
}

/// Candidato de transcrição devolvido pela IA (com confiança por trecho).
#[derive(Debug, Clone, Serialize)]
pub struct TranscriptCandidate {
    pub idx: i64,
    pub t_start: f64,
    pub t_end: Option<f64>,
    pub speaker: String,
    pub text: String,
    pub confidence: Option<f64>,
    /// Palavras com tempo e confiança (para ouvir de novo as duvidosas).
    pub words: Vec<crate::audio::WhisperWord>,
}

/// Diz se o whisper.cpp está disponível (PATH ou caminho informado). Não roda nada.
#[tauri::command]
pub async fn whisper_status(whisper_bin: Option<String>) -> Result<WhisperStatus> {
    Ok(match crate::audio::detect_whisper(whisper_bin.as_deref()) {
        Ok(p) => WhisperStatus {
            available: true,
            path: Some(p.display().to_string()),
        },
        Err(_) => WhisperStatus {
            available: false,
            path: None,
        },
    })
}

/// RASCUNHO de transcrição (whisper.cpp local, offline). Não persiste: devolve
/// candidatos para a tela de degravação; o perito DEVE revisar.
#[tauri::command]
pub async fn transcribe_audio(
    workspace_path: String,
    audio_id: String,
    options: TranscribeOptions,
) -> Result<Vec<TranscriptCandidate>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let id = Uuid::parse_str(&audio_id)
        .map_err(|e| SicroError::Validation(format!("id de áudio inválido: {e}")))?;
    let media = audio_repo::find_media_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation("áudio não encontrado".into()))?;

    let bin = crate::audio::detect_whisper(options.whisper_bin.as_deref())?;
    let model = PathBuf::from(&options.model_path);
    if !model.is_file() {
        return Err(SicroError::Validation(format!(
            "modelo do whisper não encontrado: {}",
            model.display()
        )));
    }
    let wav_abs = ws.join(&media.relative_path);
    if !wav_abs.is_file() {
        return Err(SicroError::Filesystem(format!(
            "WAV de análise ausente: {}",
            wav_abs.display()
        )));
    }

    // whisper.cpp exige 16 kHz mono → gera WAV temporário descartável.
    let tmp16k = std::env::temp_dir().join(format!("sicro-16k-{}.wav", Uuid::new_v4()));
    crate::audio::to_wav_16k_mono(&wav_abs, &tmp16k)?;

    let lang = options.language.as_deref().unwrap_or("pt");
    let vad = options.vad_model_path.as_ref().map(PathBuf::from);
    let result = crate::audio::transcribe_wav(&bin, &model, &tmp16k, lang, vad.as_deref());
    let _ = std::fs::remove_file(&tmp16k);
    let segs = result?;

    audio_repo::insert_log(
        &conn,
        &occurrence_id,
        Some(&media.sha256),
        "audio.transcribe",
        &json!({
            "model": model.file_name().and_then(|s| s.to_str()).unwrap_or(""),
            "language": lang,
            "segments": segs.len(),
            "tool": bin.file_name().and_then(|s| s.to_str()).unwrap_or("whisper"),
        })
        .to_string(),
    )?;
    occurrence_repo::record_audit(
        &conn,
        Some(&occurrence_id),
        "audio.transcribe",
        Some("audio"),
        Some("audio_transcript_segments"),
        None,
        Some(&media.sha256),
    )?;

    Ok(segs
        .into_iter()
        .enumerate()
        .map(|(i, s)| TranscriptCandidate {
            idx: i as i64,
            t_start: s.t_start,
            t_end: Some(s.t_end),
            speaker: String::new(),
            text: s.text,
            confidence: s.confidence,
            words: s.words,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Separação de locutores (sherpa-onnx local) — apoio à degravação

/// "Quem fala quando" (separador local de Configurações → IA). `num_speakers`
/// informado pelo perito é mais confiável que o automático; substitui a anterior.
#[tauri::command]
pub async fn diarize_audio(
    app: tauri::AppHandle,
    workspace_path: String,
    audio_id: String,
    num_speakers: Option<u32>,
) -> Result<AudioDiarization> {
    let s = crate::commands::settings_commands::get_app_settings(app).await?;
    let (bin, seg, emb) = (
        PathBuf::from(&s.ai.diar_bin_path),
        PathBuf::from(&s.ai.diar_segmentation_path),
        PathBuf::from(&s.ai.diar_embedding_path),
    );
    if !(bin.is_file() && seg.is_file() && emb.is_file()) {
        return Err(SicroError::Validation(
            "separador de locutores não instalado — baixe em Configurações → IA".into(),
        ));
    }
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let (media, wav_abs) = resolve_wav(&ws, &conn, &audio_id)?;

    let tmp16k = std::env::temp_dir().join(format!("sicro-diar-{}.wav", Uuid::new_v4()));
    crate::audio::to_wav_16k_mono(&wav_abs, &tmp16k)?;
    let tmp2 = tmp16k.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::audio::diarize::run(&bin, &seg, &emb, &tmp2, num_speakers)
    })
    .await
    .map_err(|e| SicroError::Validation(format!("tarefa de separação: {e}")));
    let _ = std::fs::remove_file(&tmp16k);
    let turns = result??;
    if turns.is_empty() {
        return Err(SicroError::Validation("nenhuma fala encontrada no áudio".into()));
    }

    let file_name = |p: &str| Path::new(p).file_name().and_then(|f| f.to_str()).unwrap_or("").to_string();
    let params = json!({
        "programa": "sherpa-onnx",
        "versao": s.ai.diar_version,
        "segmentacao": file_name(&s.ai.diar_segmentation_path),
        "assinatura_de_voz": file_name(&s.ai.diar_embedding_path),
        "locutores_informados": num_speakers.filter(|n| *n > 0),
        "limiar_automatico": if num_speakers.unwrap_or(0) > 0 { None } else { Some(crate::audio::diarize::AUTO_THRESHOLD) },
    });
    let d = audio_repo::replace_diarization(&conn, &occurrence_id, &media.sha256, &turns, &params)?;
    let speakers = turns.iter().map(|t| t.speaker).max().unwrap_or(0);
    audio_repo::insert_log(
        &conn,
        &occurrence_id,
        Some(&media.sha256),
        "audio.diarize",
        &json!({ "turnos": turns.len(), "locutores": speakers, "parametros": params }).to_string(),
    )?;
    occurrence_repo::record_audit(
        &conn,
        Some(&occurrence_id),
        "audio.diarize",
        Some("audio"),
        Some("audio_diarizations"),
        Some(&d.id),
        Some(&media.sha256),
    )?;
    Ok(d)
}

/// Separação de locutores gravada para o áudio (ou nada).
#[tauri::command]
pub async fn get_audio_diarization(
    workspace_path: String,
    audio_sha256: String,
) -> Result<Option<AudioDiarization>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    audio_repo::get_diarization(&conn, &manifest.occurrence_id, &audio_sha256)
}

/// Nomes que o perito deu aos locutores (índice = locutor − 1).
#[tauri::command]
pub async fn save_diarization_names(
    workspace_path: String,
    audio_sha256: String,
    names: Vec<String>,
) -> Result<()> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    audio_repo::set_diarization_names(&conn, &manifest.occurrence_id, &audio_sha256, &names)
}

// ---------------------------------------------------------------------------
// Helpers

#[allow(clippy::too_many_arguments)]
fn build_and_persist(
    conn: &rusqlite::Connection,
    occurrence_id: Uuid,
    kind: &str,
    original_path: Option<String>,
    original_relative_path: Option<String>,
    original_sha256: Option<String>,
    source_video_sha256: Option<String>,
    wav_path: &Path,
    wav_name: &str,
    sha256: String,
    log_action: &str,
) -> Result<AudioMedia> {
    let probe = probe_audio(wav_path);
    let size_bytes = std::fs::metadata(wav_path).map(|m| m.len()).unwrap_or(0);
    let warnings_json =
        serde_json::to_string(&probe.warnings).unwrap_or_else(|_| "[]".to_string());
    let now = Utc::now();
    let media = AudioMedia {
        id: Uuid::new_v4(),
        occurrence_id,
        kind: kind.to_string(),
        original_path,
        original_relative_path,
        relative_path: format!("{AUDIO_WAV_SUBDIR}/{wav_name}"),
        filename: wav_name.to_string(),
        sha256,
        original_sha256,
        source_video_sha256,
        size_bytes,
        duration_s: probe.duration_s,
        sample_rate: probe.sample_rate,
        channels: probe.channels,
        codec: probe.codec,
        bitrate: probe.bitrate,
        raw_probe_json: probe.raw_json,
        warnings_json,
        created_at: now,
        updated_at: now,
    };
    audio_repo::insert_media(conn, &media)?;
    audio_repo::insert_log(
        conn,
        &occurrence_id,
        Some(&media.sha256),
        log_action,
        &json!({
            "media_id": media.id.to_string(),
            "kind": media.kind,
            "filename": media.filename,
            "size_bytes": media.size_bytes,
            "duration_s": media.duration_s,
        })
        .to_string(),
    )?;
    occurrence_repo::record_audit(
        conn,
        Some(&occurrence_id),
        log_action,
        Some("audio"),
        Some("audio_media"),
        Some(&media.id),
        Some(&media.sha256),
    )?;
    Ok(media)
}

fn create_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)
        .map_err(|e| SicroError::Filesystem(format!("cannot create {}: {e}", dir.display())))
}

/// Nome único dentro de `dir`: acrescenta `_1`, `_2`… ao radical se já existir.
fn unique_name(dir: &Path, desired: &str) -> String {
    if !dir.join(desired).exists() {
        return desired.to_string();
    }
    let path = Path::new(desired);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let mut n = 1;
    loop {
        let candidate = if ext.is_empty() {
            format!("{stem}_{n}")
        } else {
            format!("{stem}_{n}.{ext}")
        };
        if !dir.join(&candidate).exists() {
            return candidate;
        }
        n += 1;
    }
}
