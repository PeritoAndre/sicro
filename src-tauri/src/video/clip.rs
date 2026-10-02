//! Exporta um trecho do vídeo como arquivo novo (o original não é tocado).
//!   - `Copy`: sem recompressão; só pode começar em quadro-chave, então o
//!     início recua até o anterior ao marcado (o recuo é devolvido).
//!   - `Reencode`: corte exato, mas recodificado (H.264, CRF 16).
//! Tempos em segundos de apresentação (os do player/ffprobe, inclusive com
//! start_time > 0).

use std::path::{Path, PathBuf};

use crate::error::{Result, SicroError};
use crate::video::frame_export::detect_ffmpeg;
use crate::video::probe::detect_ffprobe;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipMode {
    Copy,
    Reencode,
}

impl ClipMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ClipMode::Copy => "copy",
            ClipMode::Reencode => "reencode",
        }
    }
}

pub struct ClipOptions<'a> {
    pub video: &'a Path,
    pub out: &'a Path,
    pub start_s: f64,
    pub end_s: f64,
    pub mode: ClipMode,
    pub include_audio: bool,
}

#[derive(Debug, Clone)]
pub struct ClipResult {
    /// Onde o trecho realmente começa no vídeo de origem (Copy: o quadro-chave).
    pub actual_start_s: f64,
    /// Onde o trecho termina (fim do último quadro incluído).
    pub actual_end_s: f64,
    /// Fim do quadro mostrado no instante de saída (o que foi marcado).
    pub marked_end_s: f64,
    /// Linha de comando do ffmpeg, para a trilha/sidecar.
    pub command: Vec<String>,
}

/// Pacotes de vídeo (tempo de apresentação, é quadro-chave) entre `from` e
/// `to`, em ordem de apresentação. Lê só cabeçalhos (sem decodificar).
fn scan_frames(video: &Path, from: f64, to: f64) -> Result<Vec<(f64, bool)>> {
    let ffprobe = detect_ffprobe()?;
    let out = crate::tools::command(&ffprobe)
        .args(["-v", "error", "-select_streams", "v:0"])
        .args(["-show_entries", "packet=pts_time,flags", "-of", "csv=p=0"])
        .args(if to.is_finite() {
            vec!["-read_intervals".to_string(), format!("{:.6}%{:.6}", from.max(0.0), to)]
        } else {
            vec![] // arquivo inteiro
        })
        .arg(video)
        .output()
        .map_err(|e| SicroError::Workspace(format!("could not spawn ffprobe: {e}")))?;
    if !out.status.success() {
        return Err(SicroError::Workspace(format!(
            "ffprobe falhou ao ler os quadros: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let mut v: Vec<(f64, bool)> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut parts = l.split(',');
            let pts = parts.next()?.trim().parse::<f64>().ok()?;
            Some((pts, parts.next().unwrap_or("").contains('K')))
        })
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-7);
    Ok(v)
}

/// O que o trecho vai conter, em tempos do vídeo de origem.
#[derive(Debug, Clone, Copy)]
pub struct ClipPlan {
    /// Quadro-chave em ou antes do 1º quadro (onde a cópia sem recompressão começa).
    pub keyframe: f64,
    /// Quadro que o player mostra no instante de entrada (pts <= entrada).
    pub first_frame: f64,
    /// Quadro que o player mostra no instante de saída (pts <= saída).
    pub last_frame: f64,
    /// Duração do último quadro (até o seguinte; senão, a mediana do trecho).
    pub frame_dur: f64,
}

/// Monta o plano do trecho [start, end] — o que o player mostra nesses dois
/// instantes, quadro a quadro.
pub fn plan_clip(video: &Path, start: f64, end: f64) -> Result<ClipPlan> {
    // Janela crescente para trás até achar um quadro-chave (quase sempre < 30 s).
    for back in [30.0_f64, 300.0, f64::INFINITY] {
        let from = if back.is_finite() { (start - back).max(0.0) } else { 0.0 };
        let frames = scan_frames(video, from, end + 2.0)?;
        let keyframe = frames
            .iter()
            .filter(|(t, k)| *k && *t <= start + 1e-6)
            .map(|(t, _)| *t)
            .fold(None, |a: Option<f64>, t| Some(a.map_or(t, |a| a.max(t))));
        if let Some(keyframe) = keyframe {
            let at_or_before = |x: f64| {
                frames
                    .iter()
                    .map(|(t, _)| *t)
                    .filter(|t| *t <= x + 1e-6)
                    .fold(None, |a: Option<f64>, t| Some(a.map_or(t, |a| a.max(t))))
            };
            let first_frame = at_or_before(start).unwrap_or(keyframe);
            let last_frame = at_or_before(end).unwrap_or(first_frame).max(first_frame);
            let next = frames.iter().map(|(t, _)| *t).find(|t| *t > last_frame + 1e-6);
            let frame_dur = match next {
                Some(n) => n - last_frame,
                None => {
                    // fim do vídeo: mediana dos intervalos do trecho
                    let mut d: Vec<f64> = frames.windows(2).map(|w| w[1].0 - w[0].0).filter(|d| *d > 0.0).collect();
                    d.sort_by(|a, b| a.total_cmp(b));
                    d.get(d.len() / 2).copied().unwrap_or(1.0 / 30.0)
                }
            };
            return Ok(ClipPlan { keyframe, first_frame, last_frame, frame_dur });
        }
        if from <= 0.0 {
            break;
        }
    }
    Err(SicroError::Validation(format!("nenhum quadro-chave encontrado antes de {start:.3} s")))
}

/// Instante (s) do último quadro-chave de vídeo em ou antes de `t`.
pub fn keyframe_at_or_before(video: &Path, t: f64) -> Result<f64> {
    Ok(plan_clip(video, t, t)?.keyframe)
}

pub fn export_clip(opts: &ClipOptions<'_>) -> Result<ClipResult> {
    if !(opts.end_s > opts.start_s) {
        return Err(SicroError::Validation("o fim do trecho precisa vir depois do início".into()));
    }
    if let Some(parent) = opts.out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| SicroError::Filesystem(format!("cannot create {}: {e}", parent.display())))?;
    }
    let ffmpeg = detect_ffmpeg()?;
    let plan = plan_clip(opts.video, opts.start_s, opts.end_s)?;
    // O `-ss` de entrada conta do start_time do arquivo (probe::container_start_time).
    let file_start = crate::video::probe::read_start_time(opts.video).unwrap_or(0.0);

    // Copy: busca um tiquinho DEPOIS do quadro-chave (a busca de entrada cai no
    // quadro-chave <= alvo). Reencode: 1/4 de quadro ANTES do quadro mostrado
    // (a busca exata descarta o anterior e fica com este, sem arredondamento).
    let (actual_start, seek) = match opts.mode {
        ClipMode::Copy => (plan.keyframe, plan.keyframe + 0.0005),
        ClipMode::Reencode => (plan.first_frame, plan.first_frame - plan.frame_dur * 0.25),
    };
    // Até o meio do quadro seguinte ao último: entra o último, não o próximo.
    let cut_end = plan.last_frame + plan.frame_dur * 0.5;
    let duration = cut_end - seek.max(0.0);
    let seek = seek.max(0.0);

    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-ss"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    args.push(format!("{:.6}", (seek - file_start).max(0.0)));
    args.push("-i".into());
    args.push(opts.video.to_string_lossy().into_owned());
    args.push("-t".into());
    args.push(format!("{duration:.6}"));
    args.extend(["-map", "0:v:0"].iter().map(|s| s.to_string()));
    if opts.include_audio {
        args.extend(["-map", "0:a?"].iter().map(|s| s.to_string()));
    }
    match opts.mode {
        ClipMode::Copy => {
            args.extend(["-c", "copy", "-avoid_negative_ts", "make_zero"].iter().map(|s| s.to_string()));
        }
        ClipMode::Reencode => {
            args.extend(
                [
                    "-c:v", "libx264", "-preset", "medium", "-crf", "16", "-pix_fmt", "yuv420p",
                    "-fps_mode", "passthrough", "-c:a", "aac", "-b:a", "192k", "-movflags",
                    "+faststart",
                ]
                .iter()
                .map(|s| s.to_string()),
            );
        }
    }
    args.push("-y".into());
    args.push(opts.out.to_string_lossy().into_owned());

    let out = crate::tools::command(&ffmpeg)
        .args(&args)
        .output()
        .map_err(|e| SicroError::Workspace(format!("could not spawn ffmpeg: {e}")))?;
    if !out.status.success() || !opts.out.is_file() {
        let _ = std::fs::remove_file(opts.out);
        return Err(SicroError::Workspace(format!(
            "ffmpeg falhou ao exportar o trecho: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }

    let mut command = vec![ffmpeg.to_string_lossy().into_owned()];
    command.extend(args);
    // Reencode corta exato. Copy só corta em pacote inteiro: com quadros B o
    // arquivo leva alguns quadros ALÉM da saída (os que o decodificador precisa
    // para montar os anteriores) — mede o último quadro que ficou no arquivo.
    let actual_end = match opts.mode {
        ClipMode::Reencode => plan.last_frame + plan.frame_dur,
        ClipMode::Copy => {
            let frames = scan_frames(opts.out, 0.0, f64::INFINITY).unwrap_or_default();
            match (frames.first(), frames.last()) {
                (Some(a), Some(b)) => actual_start + (b.0 - a.0) + plan.frame_dur,
                _ => plan.last_frame + plan.frame_dur,
            }
        }
    };
    Ok(ClipResult {
        actual_start_s: actual_start,
        actual_end_s: actual_end,
        marked_end_s: plan.last_frame + plan.frame_dur,
        command,
    })
}

/// "00h00m12s480" — trecho do nome do arquivo (sem ":" por causa do Windows).
pub fn filename_stamp(t: f64) -> String {
    let total_ms = (t.max(0.0) * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let s = (total_ms / 1000) % 60;
    let m = (total_ms / 60_000) % 60;
    let h = total_ms / 3_600_000;
    format!("{h:02}h{m:02}m{s:02}s{ms:03}")
}

/// Nome do arquivo do trecho: `<origem>_trecho_<ini>-<fim>.<ext>`.
pub fn clip_filename(source: &Path, start: f64, end: f64, mode: ClipMode) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");
    let ext = match mode {
        ClipMode::Reencode => "mp4".to_string(),
        ClipMode::Copy => source
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .unwrap_or_else(|| "mp4".into()),
    };
    PathBuf::from(format!(
        "{stem}_trecho_{}-{}.{ext}",
        filename_stamp(start),
        filename_stamp(end)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vídeo de teste: 4 s, 25 fps, quadro-chave a cada 1 s (g=25).
    fn sample(dir: &Path) -> Option<PathBuf> {
        let ffmpeg = detect_ffmpeg().ok()?;
        let p = dir.join("amostra.mp4");
        let ok = crate::tools::command(ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
            .arg("testsrc=size=320x240:rate=25")
            .args(["-t", "4", "-c:v", "libx264", "-g", "25", "-keyint_min", "25"])
            .args(["-sc_threshold", "0", "-pix_fmt", "yuv420p", "-y"])
            .arg(&p)
            .status()
            .ok()?
            .success();
        ok.then_some(p)
    }

    #[test]
    fn stamp_and_name() {
        assert_eq!(filename_stamp(12.48), "00h00m12s480");
        assert_eq!(filename_stamp(3723.25), "01h02m03s250");
        let n = clip_filename(Path::new("/x/cam 1.MOV"), 1.0, 2.5, ClipMode::Copy);
        assert_eq!(n.to_str(), Some("cam 1_trecho_00h00m01s000-00h00m02s500.mov"));
        let n = clip_filename(Path::new("/x/cam.avi"), 1.0, 2.5, ClipMode::Reencode);
        assert!(n.to_str().unwrap().ends_with(".mp4"));
    }

    fn count_frames(p: &Path) -> u64 {
        let out = crate::tools::command(detect_ffprobe().unwrap())
            .args(["-v", "error", "-count_frames", "-select_streams", "v:0"])
            .args(["-show_entries", "stream=nb_read_frames", "-of", "csv=p=0"])
            .arg(p)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
    }

    #[test]
    fn copy_starts_on_previous_keyframe_and_reencode_is_exact() {
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = sample(dir.path()) else {
            eprintln!("ffmpeg ausente — teste pulado");
            return;
        };

        // 25 fps (quadros a cada 0,04 s), quadro-chave a cada 1 s.
        // Entrada 1,30 s → o player mostra o quadro de 1,28; saída 2,50 → 2,48.
        let plan = plan_clip(&src, 1.3, 2.5).unwrap();
        assert!((plan.keyframe - 1.0).abs() < 0.001, "{plan:?}");
        assert!((plan.first_frame - 1.28).abs() < 0.001, "{plan:?}");
        assert!((plan.last_frame - 2.48).abs() < 0.001, "{plan:?}");
        assert!((plan.frame_dur - 0.04).abs() < 0.001, "{plan:?}");

        let copy_out = dir.path().join("copia.mp4");
        let r = export_clip(&ClipOptions {
            video: &src,
            out: &copy_out,
            start_s: 1.3,
            end_s: 2.5,
            mode: ClipMode::Copy,
            include_audio: false,
        })
        .unwrap();
        assert!((r.actual_start_s - 1.0).abs() < 0.001);
        assert!((r.marked_end_s - 2.52).abs() < 0.001);
        // Pelo menos 1,00 … 2,48 (38 quadros); quadros B podem deixar alguns
        // quadros soltos depois da saída — e o actual_end_s tem de cobri-los.
        let n = count_frames(&copy_out);
        assert!((38..=42).contains(&n), "cópia com {n} quadros");
        assert!(r.actual_end_s >= r.marked_end_s - 1e-6, "{r:?}");
        assert!(r.actual_end_s - r.actual_start_s >= n as f64 * 0.04 - 0.002, "{r:?} / {n}");

        let enc_out = dir.path().join("recomp.mp4");
        let r = export_clip(&ClipOptions {
            video: &src,
            out: &enc_out,
            start_s: 1.3,
            end_s: 2.5,
            mode: ClipMode::Reencode,
            include_audio: false,
        })
        .unwrap();
        assert!((r.actual_start_s - 1.28).abs() < 0.001);
        // 1,28 … 2,48 = 31 quadros, exatamente os que o player mostra no trecho
        assert_eq!(count_frames(&enc_out), 31);
    }

    /// Vídeo que COMEÇA DEPOIS DO 0 (como os recortados): start_time 3,0 s,
    /// 25 fps, quadro-chave a cada 1 s (3, 4, 5…).
    fn sample_offset(dir: &Path) -> Option<PathBuf> {
        let ffmpeg = detect_ffmpeg().ok()?;
        let p = dir.join("deslocado.mp4");
        let ok = crate::tools::command(ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
            .arg("testsrc=size=320x240:rate=25")
            .args(["-t", "6", "-c:v", "libx264", "-g", "25", "-keyint_min", "25"])
            .args(["-sc_threshold", "0", "-pix_fmt", "yuv420p", "-output_ts_offset", "3", "-y"])
            .arg(&p)
            .status()
            .ok()?
            .success();
        ok.then_some(p)
    }

    /// md5 dos pixels de UM quadro decodificado: o `n`-ésimo do arquivo
    /// (ou o primeiro, com `n = None`), em rgb24 para PNG e vídeo baterem.
    fn frame_md5(p: &Path, n: Option<u32>) -> String {
        let mut cmd = crate::tools::command(detect_ffmpeg().unwrap());
        cmd.args(["-v", "error", "-i"]).arg(p);
        if let Some(n) = n {
            cmd.args(["-vf", &format!("select=eq(n\\,{n})"), "-fps_mode", "passthrough"]);
        }
        let out = cmd
            .args(["-frames:v", "1", "-pix_fmt", "rgb24", "-f", "md5", "-"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn video_starting_after_zero_collects_and_clips_the_frames_the_player_shows() {
        use crate::video::frame_export::{extract_frame, ExtractFrameOptions};
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = sample_offset(dir.path()) else {
            eprintln!("ffmpeg ausente — teste pulado");
            return;
        };
        let st = crate::video::probe::read_start_time(&src).unwrap();
        assert!((st - 3.0).abs() < 0.001, "start_time {st}");

        // Coletar em 5,30 s: o player mostra o quadro de 5,28 s = o 58º do
        // arquivo (n = (5,28 − 3,00) / 0,04 = 57). Nem 5,32 (o seguinte), nem
        // 8,28 (a busca sem descontar o início).
        let png = dir.path().join("q.png");
        let got = extract_frame(ExtractFrameOptions {
            video_path: &src,
            timestamp_s: 5.30,
            out_png: &png,
            sidecar_json: None,
            sidecar_extra: serde_json::Value::Null,
            start_time_s: st,
        })
        .unwrap();
        assert_eq!(frame_md5(&png, None), frame_md5(&src, Some(57)), "quadro coletado errado");
        assert!((got.actual_timestamp_s.unwrap() - 5.28).abs() < 0.001, "{got:?}");

        // Trecho sem recompressão 5,30–6,50: começa no quadro-chave de 5,00 s
        // (n = 50), com os MESMOS pixels do original.
        let clip = dir.path().join("t.mp4");
        let r = export_clip(&ClipOptions {
            video: &src,
            out: &clip,
            start_s: 5.30,
            end_s: 6.50,
            mode: ClipMode::Copy,
            include_audio: false,
        })
        .unwrap();
        assert!((r.actual_start_s - 5.0).abs() < 0.001, "{r:?}");
        assert_eq!(frame_md5(&clip, None), frame_md5(&src, Some(50)), "trecho começa no quadro errado");

        // Recomprimido: exatamente 5,28 … 6,48 = 31 quadros.
        let enc = dir.path().join("r.mp4");
        let r = export_clip(&ClipOptions {
            video: &src,
            out: &enc,
            start_s: 5.30,
            end_s: 6.50,
            mode: ClipMode::Reencode,
            include_audio: false,
        })
        .unwrap();
        assert!((r.actual_start_s - 5.28).abs() < 0.001);
        assert_eq!(count_frames(&enc), 31);
    }
}
