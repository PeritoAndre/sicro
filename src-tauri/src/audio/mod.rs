//! Áudio: extração/conversão para WAV PCM via FFmpeg, whisper.cpp (rascunho
//! de transcrição) e medições do FFmpeg. Nada aqui altera o áudio original.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Result, SicroError};

pub mod analysis;
pub mod authenticity;
pub mod diarize;
pub mod enf;
pub mod enhance;
pub mod spectro;

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
    crate::tools::find_ffmpeg_tool(bin).ok_or_else(|| {
        SicroError::Validation(format!(
            "'{bin}' não encontrado. Instale o FFmpeg (com ffprobe) e garanta ffmpeg + ffprobe no PATH."
        ))
    })
}

/// O arquivo tem ao menos uma trilha de áudio? (ffprobe; vídeo de câmera de
/// segurança muitas vezes só tem imagem.)
pub fn has_audio_stream(path: &Path) -> Result<bool> {
    let ffprobe = detect("ffprobe")?;
    let out = crate::tools::command(&ffprobe)
        .args(["-v", "error", "-select_streams", "a", "-show_entries", "stream=index", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .map_err(|e| SicroError::Workspace(format!("não foi possível rodar o ffprobe: {e}")))?;
    if !out.status.success() {
        return Err(SicroError::Validation(format!(
            "o ffprobe não conseguiu ler o arquivo: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(!String::from_utf8_lossy(&out.stdout).trim().is_empty())
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
    /// Palavras com tempo (no áudio original) e confiança — para o perito ouvir
    /// de novo as duvidosas.
    pub words: Vec<WhisperWord>,
}

/// Uma palavra do rascunho da IA: tempo no áudio original e confiança (0..1,
/// a menor entre os pedaços que a formam).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WhisperWord {
    pub text: String,
    pub t_start: f64,
    pub t_end: f64,
    pub p: f64,
}

/// Roda o whisper.cpp sobre um WAV 16 kHz mono (decodificação gulosa,
/// reproduzível). A saída é rascunho a ser revisado.
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

    let output = crate::tools::command(bin)
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

    // Lido como bytes: um token pode cortar um caractere UTF-8 ao meio.
    let raw = std::fs::read(&out_json).map_err(|e| {
        SicroError::Validation(format!("não foi possível ler a saída do whisper: {e}"))
    })?;
    let _ = std::fs::remove_file(&out_json);
    parse_whisper_output(&String::from_utf8_lossy(&raw), &String::from_utf8_lossy(&output.stderr))
}

/// Lê o JSON `-ojf`: trechos, confiança e palavras. Com VAD, o tempo dos
/// TRECHOS é no áudio original, mas o das PALAVRAS é no áudio só de fala; o
/// mapa vem das linhas `vad_segment_info` do log e só é aplicado se encaixar
/// as palavras melhor que o tempo cru (não corrige duas vezes).
pub fn parse_whisper_output(json_text: &str, stderr: &str) -> Result<Vec<WhisperSegment>> {
    let v: serde_json::Value = serde_json::from_str(json_text)
        .map_err(|e| SicroError::Validation(format!("JSON do whisper inválido: {e}")))?;
    let vad_map = vad_map_from_log(stderr);
    let mut parsed: Vec<(f64, f64, String, Option<f64>, Vec<Token>)> = Vec::new();
    let Some(arr) = v.get("transcription").and_then(|t| t.as_array()) else {
        return Ok(Vec::new());
    };
    for item in arr {
        let off = |k: &str| item.get("offsets").and_then(|o| o.get(k)).and_then(|x| x.as_f64());
        let (Some(from), Some(to)) = (off("from"), off("to")) else { continue };
        let text = item.get("text").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        if text.is_empty() {
            continue;
        }
        let (t_start, t_end) = (from / 1000.0, to / 1000.0);
        // Tokens REAIS (ignora [_BEG_], [_TT_*]…): (texto, início, fim, p).
        let toks: Vec<Token> = item
            .get("tokens")
            .and_then(|t| t.as_array())
            .map(|toks| {
                toks.iter()
                    .filter_map(|tok| {
                        let t = tok.get("text")?.as_str()?;
                        if t.trim_start().starts_with("[_") {
                            return None;
                        }
                        let o = tok.get("offsets")?;
                        Some((
                            t.to_string(),
                            o.get("from")?.as_f64()? / 1000.0,
                            o.get("to")?.as_f64()? / 1000.0,
                            tok.get("p")?.as_f64()?,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Confiança = média do `p` dos tokens reais.
        let confidence = item.get("tokens").and_then(|t| t.as_array()).map(|_| {
            if toks.is_empty() {
                1.0
            } else {
                toks.iter().map(|t| t.3).sum::<f64>() / toks.len() as f64
            }
        });
        parsed.push((t_start, t_end, text, confidence, toks));
    }

    // Decide UMA vez para a rodada (é comportamento da versão do whisper):
    // aplica o mapa se ele encaixa mais palavras nos seus trechos. Empate: se
    // algum tempo cru passa do fim do áudio só de fala, ele já é o original.
    let use_map = !vad_map.is_empty() && {
        let fits = |f: &dyn Fn(f64) -> f64| -> usize {
            parsed
                .iter()
                .map(|(a, b, _, _, toks)| {
                    toks.iter().filter(|t| f(t.1) >= a - 0.5 && f(t.2) <= b + 0.5).count()
                })
                .sum()
        };
        let mapped = fits(&|t| vad_to_original(t, &vad_map));
        let raw = fits(&|t| t);
        let vad_end = vad_map.last().map_or(0.0, |p| p.0);
        let raw_past_end = parsed.iter().flat_map(|p| &p.4).any(|t| t.1 > vad_end + 1.0);
        mapped > raw || (mapped == raw && !raw_past_end)
    };
    Ok(parsed
        .into_iter()
        .map(|(t_start, t_end, text, confidence, toks)| {
            let toks: Vec<Token> = if use_map {
                toks.into_iter()
                    .map(|(x, a, b, p)| (x, vad_to_original(a, &vad_map), vad_to_original(b, &vad_map), p))
                    .collect()
            } else {
                toks
            };
            WhisperSegment { t_start, t_end, text, confidence, words: words_from_tokens(&toks) }
        })
        .collect())
}

/// Token real do whisper: (texto, início s, fim s, probabilidade).
type Token = (String, f64, f64, f64);

/// Pontos (tempo no áudio só de fala, tempo original), em segundos, ordenados.
fn vad_map_from_log(stderr: &str) -> Vec<(f64, f64)> {
    let num = |line: &str, key: &str| -> Option<f64> {
        let rest = &line[line.find(key)? + key.len()..];
        rest.trim_start().split(|c: char| c == ',' || c.is_whitespace()).next()?.parse().ok()
    };
    let segs: Vec<(f64, f64, f64, f64)> = stderr
        .lines()
        .filter(|l| l.contains("vad_segment_info:"))
        .filter_map(|l| {
            Some((num(l, "orig_start:")?, num(l, "orig_end:")?, num(l, "vad_start:")?, num(l, "vad_end:")?))
        })
        .collect();
    let mut pts = Vec::new();
    for (i, &(os, oe, vs, ve)) in segs.iter().enumerate() {
        pts.push((vs, os));
        pts.push((ve, oe));
        // Entre um trecho e o próximo o whisper põe 0,1 s de silêncio, que
        // corresponde à pausa original inteira.
        if let Some(&(_, _, next_vs, _)) = segs.get(i + 1) {
            pts.push((next_vs - 0.10, oe));
        }
    }
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
    pts
}

fn vad_to_original(t: f64, map: &[(f64, f64)]) -> f64 {
    let (Some(first), Some(last)) = (map.first(), map.last()) else { return t };
    if t <= first.0 {
        return first.1;
    }
    if t >= last.0 {
        return last.1;
    }
    let i = map.partition_point(|p| p.0 < t);
    let (lo, hi) = (map[i - 1], map[i]);
    if hi.0 - lo.0 <= 0.0 {
        return lo.1;
    }
    lo.1 + (t - lo.0) * (hi.1 - lo.1) / (hi.0 - lo.0)
}

/// Junta os tokens em palavras (um token que começa com espaço abre palavra
/// nova). A confiança da palavra é a MENOR entre os seus pedaços com letra ou
/// número (pontuação não conta).
fn words_from_tokens(toks: &[Token]) -> Vec<WhisperWord> {
    let mut words: Vec<WhisperWord> = Vec::new();
    let mut cur: Option<WhisperWord> = None;
    for &(ref text, a, b, ref p) in toks {
        let has_alnum = text.chars().any(char::is_alphanumeric);
        let starts_word = text.starts_with(' ') && has_alnum;
        match cur.as_mut() {
            Some(w) if !starts_word => {
                w.text.push_str(text);
                // Pontuação não estica a palavra (o tempo dela cobre a pausa).
                if has_alnum {
                    w.t_end = w.t_end.max(b);
                    w.p = if w.p.is_nan() { *p } else { w.p.min(*p) };
                }
            }
            _ => {
                if let Some(w) = cur.take() {
                    words.push(w);
                }
                cur = Some(WhisperWord {
                    text: text.clone(),
                    t_start: a,
                    t_end: b,
                    p: if has_alnum { *p } else { f64::NAN },
                });
            }
        }
    }
    words.extend(cur);
    words
        .into_iter()
        .filter(|w| !w.p.is_nan())
        .map(|w| WhisperWord { text: w.text.trim().to_string(), ..w })
        .filter(|w| !w.text.is_empty())
        .collect()
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

/// Concatena trechos (WAV de origem + [start, end] em s) num WAV PCM 16-bit
/// 44,1 kHz mono, com pausa de `gap_s` s entre eles (junção audível).
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

/// Espectrograma PNG (tempo × frequência) via FFmpeg.
pub fn spectrogram_png(wav: &Path, out_png: &Path) -> Result<()> {
    let i = wav.to_string_lossy();
    let o = out_png.to_string_lossy();
    run_ffmpeg(&[
        "-y", "-i", i.as_ref(),
        "-lavfi", "showspectrumpic=s=1280x540:legend=1",
        o.as_ref(),
    ])
}

pub(crate) fn run_ffmpeg(args: &[&str]) -> Result<()> {
    let ffmpeg = detect("ffmpeg")?;
    let output = crate::tools::command(&ffmpeg)
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

/// Medições do FFmpeg (ruído de fundo, bits efetivos, EBU R128, true peak,
/// silêncios) sobre o WAV de análise. Só lê; nada é gravado.
pub fn extended_measure(wav: &Path, duration_s: f64) -> Result<analysis::ExtendedMeasurements> {
    const SILENCE_DB: f32 = -50.0;
    const SILENCE_MIN_S: f32 = 0.5;
    let ffmpeg = detect("ffmpeg")?;
    let af = format!(
        "astats=measure_perchannel=none:measure_overall=Noise_floor+Bit_depth,ebur128=peak=true:framelog=verbose,silencedetect=n={SILENCE_DB}dB:d={SILENCE_MIN_S}"
    );
    let out = crate::tools::command(&ffmpeg)
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(wav)
        .args(["-af", &af, "-f", "null", "-"])
        .output()
        .map_err(|e| SicroError::Validation(format!("falha ao executar ffmpeg: {e}")))?;
    if !out.status.success() {
        return Err(SicroError::Validation("ffmpeg não conseguiu medir o áudio".into()));
    }
    Ok(analysis::parse_extended(
        &String::from_utf8_lossy(&out.stderr),
        SILENCE_DB,
        SILENCE_MIN_S,
        duration_s,
    ))
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
    let output = crate::tools::command(&ffprobe)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Gera um vídeo curto (com ou sem trilha de áudio); None sem ffmpeg.
    fn sample(dir: &Path, with_audio: bool) -> Option<PathBuf> {
        let ffmpeg = detect("ffmpeg").ok()?;
        let p = dir.join(if with_audio { "com_audio.mp4" } else { "so_imagem.mp4" });
        let mut cmd = crate::tools::command(ffmpeg);
        cmd.args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=size=160x120:rate=10"]);
        if with_audio {
            cmd.args(["-f", "lavfi", "-i", "sine=frequency=440"]);
        }
        cmd.args(["-t", "1", "-pix_fmt", "yuv420p", "-y"]).arg(&p);
        cmd.status().ok()?.success().then_some(p)
    }

    #[test]
    fn detecta_video_sem_trilha_de_audio() {
        let dir = tempfile::tempdir().unwrap();
        let (Some(com), Some(sem)) = (sample(dir.path(), true), sample(dir.path(), false)) else {
            eprintln!("ffmpeg ausente — teste pulado");
            return;
        };
        assert!(has_audio_stream(&com).unwrap());
        assert!(!has_audio_stream(&sem).unwrap());
    }

    /// Rodada real do whisper.cpp 1.8.5 (modelo base, VAD silero) sobre o
    /// jfk.wav com 5 s de silêncio antes: o tempo das palavras vem no áudio
    /// só de fala e precisa do mapa do log.
    const LOG_VAD: &str = "\
whisper_vad: vad_segment_info: orig_start: 5.31, orig_end: 7.23, vad_start: 0.00, vad_end: 1.92
whisper_vad: vad_segment_info: orig_start: 8.26, orig_end: 9.41, vad_start: 2.12, vad_end: 3.27
whisper_vad: vad_segment_info: orig_start: 10.40, orig_end: 12.67, vad_start: 3.47, vad_end: 5.74
whisper_vad: vad_segment_info: orig_start: 13.15, orig_end: 15.58, vad_start: 5.94, vad_end: 8.37
";

    fn tok(text: &str, from: u32, to: u32, p: f64) -> serde_json::Value {
        serde_json::json!({"text": text, "offsets": {"from": from, "to": to}, "p": p})
    }

    #[test]
    fn palavras_juntam_pedacos_e_pontuacao_nao_conta() {
        let j = serde_json::json!({"transcription": [{
            "offsets": {"from": 0, "to": 3000},
            "text": " Perícia, documento.",
            "tokens": [
                tok("[_BEG_]", 0, 0, 0.9),
                tok(" Per", 100, 300, 0.9),
                tok("ícia", 300, 600, 0.42),
                tok(",", 600, 650, 0.05),
                tok(" documento", 900, 1500, 0.97),
                tok(".", 1500, 1550, 0.3),
            ]
        }]});
        let segs = parse_whisper_output(&j.to_string(), "").unwrap();
        let w = &segs[0].words;
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].text, "Perícia,");
        assert!((w[0].p - 0.42).abs() < 1e-9, "a vírgula não pode puxar a confiança");
        assert!((w[0].t_start - 0.1).abs() < 1e-9 && (w[0].t_end - 0.6).abs() < 1e-9);
        assert_eq!(w[1].text, "documento.");
        assert!((w[1].p - 0.97).abs() < 1e-9);
    }

    #[test]
    fn com_vad_o_tempo_das_palavras_volta_ao_audio_original() {
        // Segundo trecho da rodada real: "ask what you can do for your country."
        let j = serde_json::json!({"transcription": [{
            "offsets": {"from": 13210, "to": 15510},
            "text": " ask what you can do for your country.",
            "tokens": [tok(" ask", 6020, 6210, 0.89), tok(" what", 6370, 6490, 0.97), tok(" country", 7830, 8100, 0.99)]
        }]});
        let segs = parse_whisper_output(&j.to_string(), LOG_VAD).unwrap();
        let w = &segs[0].words;
        // Sem VAD o whisper dá ask 13,19 · what 13,63 · country 15,00.
        for (word, real) in w.iter().zip([13.19, 13.63, 15.00]) {
            assert!((word.t_start - real).abs() < 0.3, "{} em {:.2} (real {real})", word.text, word.t_start);
        }
        // Sem as linhas do log, fica o tempo cru (nada a corrigir).
        let cru = parse_whisper_output(&j.to_string(), "").unwrap();
        assert!((cru[0].words[0].t_start - 6.02).abs() < 1e-9);
    }

    #[test]
    fn nao_corrige_duas_vezes_se_o_whisper_ja_der_o_tempo_original() {
        let j = serde_json::json!({"transcription": [{
            "offsets": {"from": 13210, "to": 15510},
            "text": " ask what",
            "tokens": [tok(" ask", 13190, 13400, 0.89), tok(" what", 13630, 13800, 0.97)]
        }]});
        let segs = parse_whisper_output(&j.to_string(), LOG_VAD).unwrap();
        assert!((segs[0].words[0].t_start - 13.19).abs() < 1e-9);
        assert!((segs[0].words[1].t_start - 13.63).abs() < 1e-9);
    }
}
