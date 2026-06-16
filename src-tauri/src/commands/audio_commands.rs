//! Comandos Tauri do módulo Áudio (Camada 1).
//!
//! Determinístico e com cadeia de custódia (igual ao Vídeo): hash do material,
//! cópia para subpasta do workspace, metadados via ffprobe, persistência +
//! log de operação + auditoria. Nada de interpretação.

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::audio::analysis::{self, AudioMeasurements, EnfResult, SpectrumResult};
use crate::audio::{convert_to_wav, enhance_to_wav, extract_audio_to_wav, probe_audio};
use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::{audio_repo, occurrence_repo};
use crate::error::{Result, SicroError};
use crate::hashing::sha256::sha256_file;
use crate::models::{
    AudioEnhancement, AudioMarker, AudioMedia, AudioTranscriptSegment, TranscriptSegmentInput,
};
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};

const AUDIO_ORIG_SUBDIR: &str = "audio/originais";
const AUDIO_WAV_SUBDIR: &str = "audio/wav";
const AUDIO_SPECTRO_SUBDIR: &str = "audio/espectrogramas";

/// Extrai a trilha de áudio de um vídeo (do caso ou externo) para WAV de
/// análise. `source_video_sha256` registra a proveniência quando o vídeo já
/// está no caso.
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

    // 1. Hash do ORIGINAL (a evidência) + dedupe por ele.
    let original_sha256 = sha256_file(&source)?;
    if let Some(existing) =
        audio_repo::find_media_by_original_sha256(&conn, &occurrence_id, &original_sha256)?
    {
        return Err(SicroError::Validation(format!(
            "este áudio já foi importado nesta ocorrência (id {}).",
            existing.id
        )));
    }

    // 2. Preserva o original.
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

    // 3. WAV de análise a partir do original preservado.
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
// Espectrograma (Camada 4 — visualização objetiva do sinal)

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
// W12 (paridade Audacity) — Análise forense em Rust puro (medição/espectro/ENF)
//
// Tudo aqui é ANÁLISE: lê o WAV de análise e devolve números determinísticos.
// Não altera o áudio (o realce continua sendo um derivado FFmpeg). Cada
// chamada é registrada no log de auditoria do áudio (reprodutível).

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

/// Medições objetivas: pico/RMS (dBFS), offset DC, clipping, fator de crista,
/// duração. Equivalente ao Sample Data Export + Find Clipping do Audacity.
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
    let m = analysis::measure(&samples, sr, ch, 0.997);
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

/// Extração da curva ENF (Electric Network Frequency) e checagem de
/// continuidade (maior salto = indício de edição/splice). `nominal_hz` 50 ou 60.
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
    let (samples, sr, _ch) = analysis::read_wav_mono(&wav_abs)?;
    let e = analysis::enf(&samples, sr, nominal_hz.unwrap_or(60.0), 10.0, 5.0);
    audio_repo::insert_log(
        &conn,
        &manifest.occurrence_id,
        Some(&media.sha256),
        "audio.enf",
        &json!({
            "nominal_hz": e.nominal_hz,
            "mean_hz": e.mean_hz,
            "std_hz": e.std_hz,
            "max_jump_hz": e.max_jump_hz,
        })
        .to_string(),
    )?;
    Ok(e)
}

// ---------------------------------------------------------------------------
// Extração de trecho (Camada 4 — recorte com custódia)

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

/// Compila vários trechos (de um ou mais áudios) num novo derivado rotulado
/// (`kind="compilacao"`), com hash + custódia + um manifesto `.compilacao.json`
/// documentando a origem (arquivo + sha256) e os tempos de cada trecho, na
/// ordem. NÃO-destrutivo: os áudios de origem permanecem intactos. Não junta
/// "disfarçando" — há uma pausa audível entre trechos e o manifesto é explícito.
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

/// Escreve, ao lado do WAV compilado, o manifesto JSON com a origem e os tempos
/// de cada trecho — o "rótulo" forense que torna a montagem reproduzível.
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

/// Converte chaves de filtro (do front) na cadeia FFmpeg `-af` correspondente.
/// Cada filtro é padrão e reproduzível; nada interpreta o conteúdo.
fn build_filter_chain(keys: &[String]) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for k in keys {
        match k.as_str() {
            // Redução de ruído de banda larga (FFT denoise).
            "denoise" => parts.push("afftdn"),
            // Remove ronco/rumble de baixa frequência (< 80 Hz).
            "highpass" => parts.push("highpass=f=80"),
            // Corta sibilância/chiado acima de 8 kHz.
            "lowpass" => parts.push("lowpass=f=8000"),
            // Normalização dinâmica de volume (equaliza trechos baixos/altos).
            "normalize" => parts.push("dynaudnorm"),
            // W12 — Remove zumbido da rede elétrica (50 Hz + harmônicos) por
            // bandreject (notch) estreito. Subtrativo: só REMOVE energia da rede.
            "notch_hum_50" => {
                parts.push("bandreject=f=50:width_type=h:width=4");
                parts.push("bandreject=f=100:width_type=h:width=4");
                parts.push("bandreject=f=150:width_type=h:width=4");
            }
            // Idem para 60 Hz (rede das Américas).
            "notch_hum_60" => {
                parts.push("bandreject=f=60:width_type=h:width=4");
                parts.push("bandreject=f=120:width_type=h:width=4");
                parts.push("bandreject=f=180:width_type=h:width=4");
            }
            // Banda de voz telefônica (300–3400 Hz): foca a inteligibilidade
            // da fala cortando o que está fora dela.
            "bandpass_voice" => {
                parts.push("highpass=f=300");
                parts.push("lowpass=f=3400");
            }
            _ => {}
        }
    }
    parts.join(",")
}

/// Gera um DERIVADO realçado (auxílio de escuta) a partir do WAV de análise.
///
/// NÃO-destrutivo: o original e o WAV de análise permanecem intactos. O realce
/// é uma nova mídia (`kind="realce"`) e a cadeia EXATA de filtros fica
/// registrada em `audio_enhancements` (reproduzível). Isto é um auxílio de
/// escuta — NÃO "limpa" nem "recupera" conteúdo, apenas filtra de forma
/// determinística para facilitar a audição pelo perito.
#[tauri::command]
pub async fn enhance_audio(
    workspace_path: String,
    source_audio_id: String,
    filters: Vec<String>,
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

    let chain = build_filter_chain(&filters);
    if chain.is_empty() {
        return Err(SicroError::Validation(
            "selecione ao menos um filtro de realce".into(),
        ));
    }

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

    enhance_to_wav(&src_abs, &out_path, &chain)?;
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
        filters_json: json!({ "keys": filters, "chain": chain }).to_string(),
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

/// Salva (substitui) toda a degravação MANUAL de um áudio. A transcrição é
/// trabalho do perito — o tool não transcreve nem interpreta nada. Devolve os
/// segmentos persistidos (com ids gerados) para o front reidratar.
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
    /// Caminho/nome do executável whisper.cpp (opcional; senão procura no PATH).
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

/// Gera um RASCUNHO de transcrição (whisper.cpp local, offline) para o áudio.
///
/// A saída é rascunho de máquina — o perito DEVE revisar. Não persiste nada:
/// devolve segmentos candidatos para a tela de degravação. Determinístico
/// (decodificação gulosa). Não identifica locutor nem interpreta conteúdo.
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
        })
        .collect())
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

/// Devolve um nome de arquivo único dentro de `dir` a partir de `desired`,
/// acrescentando `_1`, `_2`… ao radical se já existir.
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
