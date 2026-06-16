//! Áudio — núcleo determinístico (Camada 1).
//!
//! Só faz duas coisas, ambas reproduzíveis e sem interpretação: extrair a
//! trilha de áudio de um vídeo e converter um arquivo de áudio para WAV PCM
//! 16-bit (formato de análise sem perda). Localiza os binários FFmpeg/ffprobe
//! no PATH — mesma estratégia do módulo Vídeo.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Result, SicroError};

/// W12 (paridade Audacity) — análise forense de áudio em Rust puro
/// (medição, espectro, ENF). Determinístico e testável; não altera o áudio.
pub mod analysis;

/// Metadados técnicos lidos do áudio via ffprobe (best-effort).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioProbe {
    pub duration_s: Option<f64>,
    pub codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub bitrate: Option<i64>,
    pub raw_json: String,
    pub warnings: Vec<String>,
}

fn detect(bin: &str) -> Result<PathBuf> {
    which::which(bin).map_err(|_| {
        SicroError::Validation(format!(
            "binário '{bin}' não encontrado no PATH. Instale o FFmpeg (com ffprobe) e garanta ffmpeg + ffprobe no PATH."
        ))
    })
}

/// Extrai a trilha de áudio de um vídeo para WAV PCM 16-bit (sem perda), 48 kHz.
pub fn extract_audio_to_wav(video: &Path, out_wav: &Path) -> Result<()> {
    let i = video.to_string_lossy();
    let o = out_wav.to_string_lossy();
    run_ffmpeg(&[
        "-y", "-i", i.as_ref(), "-vn", "-acodec", "pcm_s16le", "-ar", "48000",
        o.as_ref(),
    ])
}

/// Converte um áudio qualquer (.opus/.amr/.m4a/.mp3…) para WAV PCM 16-bit,
/// preservando a taxa de amostragem de origem.
pub fn convert_to_wav(audio: &Path, out_wav: &Path) -> Result<()> {
    let i = audio.to_string_lossy();
    let o = out_wav.to_string_lossy();
    run_ffmpeg(&["-y", "-i", i.as_ref(), "-vn", "-acodec", "pcm_s16le", o.as_ref()])
}

/// Aplica uma cadeia de filtros FFmpeg (`-af`) ao WAV, gerando um DERIVADO de
/// realce (auxílio de escuta). Determinístico/reproduzível dada a mesma cadeia.
pub fn enhance_to_wav(src: &Path, out_wav: &Path, af_chain: &str) -> Result<()> {
    let i = src.to_string_lossy();
    let o = out_wav.to_string_lossy();
    run_ffmpeg(&[
        "-y", "-i", i.as_ref(), "-af", af_chain, "-acodec", "pcm_s16le", o.as_ref(),
    ])
}

/// Converte um WAV para 16 kHz mono PCM 16-bit — formato exigido pelo whisper.cpp.
pub fn to_wav_16k_mono(src: &Path, out_wav: &Path) -> Result<()> {
    let i = src.to_string_lossy();
    let o = out_wav.to_string_lossy();
    run_ffmpeg(&[
        "-y", "-i", i.as_ref(), "-ar", "16000", "-ac", "1", "-acodec", "pcm_s16le",
        o.as_ref(),
    ])
}

/// Localiza o executável do whisper.cpp (nomes comuns) ou usa o caminho dado.
pub fn detect_whisper(custom: Option<&str>) -> Result<PathBuf> {
    if let Some(c) = custom {
        if !c.trim().is_empty() {
            let p = PathBuf::from(c);
            if p.is_file() {
                return Ok(p);
            }
            if let Ok(found) = which::which(c) {
                return Ok(found);
            }
        }
    }
    // NÃO incluir "main": no Windows colide com C:\Windows\System32\main.cpl
    // (Painel de Controle), causando "não é um aplicativo Win32 válido" (erro 193).
    for name in ["whisper-cli", "whisper"] {
        if let Ok(p) = which::which(name) {
            return Ok(p);
        }
    }
    Err(SicroError::Validation(
        "whisper.cpp não encontrado. Instale o whisper-cli e garanta que esteja no PATH \
         (ou informe o caminho do executável)."
            .into(),
    ))
}

/// Um segmento transcrito — RASCUNHO de máquina (o perito revisa).
#[derive(Debug, Clone)]
pub struct WhisperSegment {
    pub t_start: f64,
    pub t_end: f64,
    pub text: String,
    /// Confiança média (0..1) dos tokens reais do trecho; `None` se indisponível.
    pub confidence: Option<f64>,
}

/// Roda o whisper.cpp sobre um WAV 16 kHz mono e devolve os segmentos do JSON.
/// Decodificação gulosa padrão (reproduzível). NÃO interpreta — só transcreve;
/// a saída é rascunho a ser revisado.
pub fn transcribe_wav(
    bin: &Path,
    model: &Path,
    wav16k: &Path,
    language: &str,
    vad_model: Option<&Path>,
) -> Result<Vec<WhisperSegment>> {
    let out_prefix = std::env::temp_dir().join(format!("sicro-whisper-{}", Uuid::new_v4()));
    let out_json = out_prefix.with_extension("json");

    let mut args: Vec<String> = vec![
        "-m".into(),
        model.to_string_lossy().to_string(),
        "-f".into(),
        wav16k.to_string_lossy().to_string(),
        "-l".into(),
        language.to_string(),
        "-ojf".into(), // JSON full: inclui tokens com probabilidade (p) por palavra
        "-of".into(),
        out_prefix.to_string_lossy().to_string(),
    ];
    if let Some(vad) = vad_model {
        args.push("--vad".into());
        args.push("--vad-model".into());
        args.push(vad.to_string_lossy().to_string());
    }

    let output = Command::new(bin)
        .args(&args)
        .output()
        .map_err(|e| SicroError::Validation(format!("falha ao executar whisper.cpp: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let lines: Vec<&str> = stderr.lines().collect();
        let tail = lines[lines.len().saturating_sub(4)..].join(" | ");
        let _ = std::fs::remove_file(&out_json);
        return Err(SicroError::Validation(format!("whisper.cpp falhou: {tail}")));
    }

    let raw = std::fs::read_to_string(&out_json).map_err(|e| {
        SicroError::Validation(format!("não foi possível ler a saída do whisper: {e}"))
    })?;
    let _ = std::fs::remove_file(&out_json);

    let v: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| SicroError::Validation(format!("JSON do whisper inválido: {e}")))?;
    let mut segs = Vec::new();
    if let Some(arr) = v.get("transcription").and_then(|t| t.as_array()) {
        for item in arr {
            let from = item
                .get("offsets")
                .and_then(|o| o.get("from"))
                .and_then(|x| x.as_f64());
            let to = item
                .get("offsets")
                .and_then(|o| o.get("to"))
                .and_then(|x| x.as_f64());
            let text = item
                .get("text")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            // Confiança = média do `p` dos tokens REAIS (ignora [_BEG_], [_TT_*]…).
            let confidence = item.get("tokens").and_then(|t| t.as_array()).map(|toks| {
                let ps: Vec<f64> = toks
                    .iter()
                    .filter(|tok| {
                        let t = tok.get("text").and_then(|x| x.as_str()).unwrap_or("");
                        !t.trim_start().starts_with("[_")
                    })
                    .filter_map(|tok| tok.get("p").and_then(|x| x.as_f64()))
                    .collect();
                if ps.is_empty() {
                    1.0
                } else {
                    ps.iter().sum::<f64>() / ps.len() as f64
                }
            });
            if let (Some(from), Some(to)) = (from, to) {
                if !text.is_empty() {
                    segs.push(WhisperSegment {
                        t_start: from / 1000.0,
                        t_end: to / 1000.0,
                        text,
                        confidence,
                    });
                }
            }
        }
    }
    Ok(segs)
}

/// Recorta o trecho [start_s, end_s] (segundos) do WAV → novo WAV PCM 16-bit.
/// Re-encoda para corte preciso. Determinístico; não altera o original.
pub fn extract_clip_wav(src: &Path, out_wav: &Path, start_s: f64, end_s: f64) -> Result<()> {
    let dur = (end_s - start_s).max(0.01);
    let i = src.to_string_lossy();
    let o = out_wav.to_string_lossy();
    let ss = format!("{start_s:.3}");
    let t = format!("{dur:.3}");
    run_ffmpeg(&[
        "-y", "-i", i.as_ref(), "-ss", &ss, "-t", &t, "-acodec", "pcm_s16le",
        o.as_ref(),
    ])
}

/// Concatena vários trechos (cada um: WAV de origem + [start,end] em segundos)
/// num único WAV PCM 16-bit, normalizado para 44,1 kHz mono, com uma pausa de
/// `gap_s` segundos entre trechos (junção audível, transparente). Cada trecho
/// entra como uma entrada independente do FFmpeg. NÃO altera os originais.
pub fn concat_clips_wav(
    segments: &[(std::path::PathBuf, f64, f64)],
    gap_s: f64,
    out_wav: &Path,
) -> Result<()> {
    if segments.len() < 2 {
        return Err(crate::error::SicroError::Validation(
            "compilação exige ao menos 2 trechos".into(),
        ));
    }
    let n = segments.len();
    let mut args: Vec<String> = vec!["-y".into()];
    for (src, _, _) in segments {
        args.push("-i".into());
        args.push(src.to_string_lossy().into_owned());
    }
    let mut graph = String::new();
    let mut concat_in = String::new();
    for (i, (_, start, end)) in segments.iter().enumerate() {
        let pad = if i + 1 < n && gap_s > 0.0 {
            format!(",apad=pad_dur={gap_s:.3}")
        } else {
            String::new()
        };
        graph.push_str(&format!(
            "[{i}:a]atrim=start={start:.3}:end={end:.3},asetpts=PTS-STARTPTS,\
             aresample=44100,aformat=sample_fmts=s16:channel_layouts=mono{pad}[a{i}];"
        ));
        concat_in.push_str(&format!("[a{i}]"));
    }
    graph.push_str(&format!("{concat_in}concat=n={n}:v=0:a=1[out]"));

    args.push("-filter_complex".into());
    args.push(graph);
    args.push("-map".into());
    args.push("[out]".into());
    args.push("-acodec".into());
    args.push("pcm_s16le".into());
    args.push(out_wav.to_string_lossy().into_owned());

    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_ffmpeg(&refs)
}

/// Gera um espectrograma (tempo × frequência) PNG via FFmpeg — visualização
/// OBJETIVA do sinal (não interpreta nada). Determinístico dada a mesma entrada.
pub fn spectrogram_png(wav: &Path, out_png: &Path) -> Result<()> {
    let i = wav.to_string_lossy();
    let o = out_png.to_string_lossy();
    run_ffmpeg(&[
        "-y", "-i", i.as_ref(),
        "-lavfi", "showspectrumpic=s=1280x540:legend=1",
        o.as_ref(),
    ])
}

fn run_ffmpeg(args: &[&str]) -> Result<()> {
    let ffmpeg = detect("ffmpeg")?;
    let output = Command::new(&ffmpeg)
        .args(args)
        .output()
        .map_err(|e| SicroError::Validation(format!("falha ao executar ffmpeg: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let lines: Vec<&str> = stderr.lines().collect();
        let tail = lines[lines.len().saturating_sub(4)..].join(" | ");
        return Err(SicroError::Validation(format!("ffmpeg falhou: {tail}")));
    }
    Ok(())
}

/// Lê metadados de áudio com ffprobe. Best-effort: falha vira aviso, não erro.
pub fn probe_audio(path: &Path) -> AudioProbe {
    let mut probe = AudioProbe {
        raw_json: "{}".to_string(),
        ..Default::default()
    };
    let ffprobe = match detect("ffprobe") {
        Ok(p) => p,
        Err(e) => {
            probe.warnings.push(format!("{e}"));
            return probe;
        }
    };
    let p = path.to_string_lossy();
    let output = Command::new(&ffprobe)
        .args([
            "-v", "quiet", "-print_format", "json", "-show_format", "-show_streams",
            p.as_ref(),
        ])
        .output();
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => {
            probe
                .warnings
                .push("ffprobe falhou ao ler metadados do áudio".to_string());
            return probe;
        }
    };
    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    probe.raw_json = raw.clone();

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
        if let Some(fmt) = v.get("format") {
            probe.duration_s = fmt
                .get("duration")
                .and_then(|d| d.as_str())
                .and_then(|s| s.parse::<f64>().ok());
            probe.bitrate = fmt
                .get("bit_rate")
                .and_then(|d| d.as_str())
                .and_then(|s| s.parse::<i64>().ok());
        }
        let audio_stream = v
            .get("streams")
            .and_then(|s| s.as_array())
            .and_then(|arr| {
                arr.iter().find(|s| {
                    s.get("codec_type").and_then(|t| t.as_str()) == Some("audio")
                })
            });
        match audio_stream {
            Some(a) => {
                probe.codec = a
                    .get("codec_name")
                    .and_then(|c| c.as_str())
                    .map(String::from);
                probe.sample_rate = a
                    .get("sample_rate")
                    .and_then(|c| c.as_str())
                    .and_then(|s| s.parse::<u32>().ok());
                probe.channels = a
                    .get("channels")
                    .and_then(|c| c.as_u64())
                    .map(|n| n as u32);
                if probe.duration_s.is_none() {
                    probe.duration_s = a
                        .get("duration")
                        .and_then(|d| d.as_str())
                        .and_then(|s| s.parse::<f64>().ok());
                }
            }
            None => probe
                .warnings
                .push("nenhuma trilha de áudio encontrada no arquivo".to_string()),
        }
    }
    probe
}
