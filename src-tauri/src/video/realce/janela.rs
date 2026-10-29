//! Leitura de uma janela de quadros consecutivos (RGB) em torno de um instante, só das regiões
//! que interessam, pelo ffmpeg em pipe. Os tempos vêm dos pacotes do arquivo (ffprobe).

use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{Result, SicroError};
use crate::image_processing::lens_correction::{corrigir_rgb, LensCoefficients};
use crate::video::frame_export::detect_ffmpeg;
use crate::video::probe::detect_ffprobe;

use super::average::Recorte;

/// Instantes de apresentação dos quadros entre `from` e `to`, em ordem.
pub fn tempos_entre(video: &Path, from: f64, to: f64) -> Result<Vec<f64>> {
    let ffprobe = detect_ffprobe()?;
    let out = crate::tools::command(&ffprobe)
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts_time", "-of", "csv=p=0"])
        .args(["-read_intervals", &format!("{:.6}%{:.6}", from.max(0.0), to)])
        .arg(video)
        .output()
        .map_err(|e| SicroError::Workspace(format!("could not spawn ffprobe: {e}")))?;
    if !out.status.success() {
        return Err(SicroError::Workspace(format!(
            "ffprobe falhou ao ler os quadros: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let mut v: Vec<f64> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split(',').next()?.trim().parse::<f64>().ok())
        .collect();
    v.sort_by(|a, b| a.total_cmp(b));
    v.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    Ok(v)
}

/// Os `n` quadros em volta do que o player mostra em `t`: (instantes, posição do quadro de `t`).
pub fn escolher_janela(video: &Path, t: f64, n: usize) -> Result<(Vec<f64>, usize)> {
    let mut span = 2.0f64.max(n as f64 * 0.05);
    loop {
        let todos = tempos_entre(video, t - span, t + span)?;
        let Some(i0) = todos.iter().rposition(|x| *x <= t + 1e-6) else {
            if span > 600.0 {
                return Err(SicroError::Validation("nenhum quadro encontrado nesse instante".into()));
            }
            span *= 3.0;
            continue;
        };
        let tenho_antes = i0;
        let tenho_depois = todos.len() - 1 - i0;
        let fim_do_video = tenho_depois < n / 2 && span > 600.0;
        if (tenho_antes >= n / 2 && tenho_depois >= n - n / 2 - 1) || span > 600.0 || fim_do_video {
            let mut ini = i0.saturating_sub(n / 2);
            if ini + n > todos.len() {
                ini = todos.len().saturating_sub(n);
            }
            let fim = (ini + n).min(todos.len());
            return Ok((todos[ini..fim].to_vec(), i0 - ini));
        }
        // perto do começo do arquivo o que falta antes não aparece nunca: não insiste
        if t - span <= 0.0 && tenho_depois >= n - n / 2 - 1 {
            let ini = 0;
            let fim = (ini + n).min(todos.len());
            return Ok((todos[ini..fim].to_vec(), i0.min(fim - 1)));
        }
        span *= 3.0;
    }
}

pub struct Lente {
    pub coef: LensCoefficients,
    pub cx: f32,
    pub cy: f32,
}

pub struct JanelaLida {
    pub recortes: Vec<Recorte>,
    /// Quadro de referência inteiro (já com a lente), RGB.
    pub quadro_ref: Vec<u8>,
}

/// Lê `tempos.len()` quadros a partir de `tempos[0]`, guardando só o retângulo `rect`
/// (x, y, largura, altura) de cada um e o quadro inteiro da posição `ref_pos`.
#[allow(clippy::too_many_arguments)]
pub fn ler_janela(
    video: &Path,
    start_time_s: f64,
    tempos: &[f64],
    w: u32,
    h: u32,
    rect: (u32, u32, u32, u32),
    ref_pos: usize,
    lente: Option<&Lente>,
    cancelar: &AtomicBool,
) -> Result<JanelaLida> {
    let ffmpeg = detect_ffmpeg()?;
    let dur = tempos.windows(2).map(|p| p[1] - p[0]).filter(|d| *d > 0.0).fold(f64::MAX, f64::min);
    let dur = if dur.is_finite() && dur < 10.0 { dur } else { 1.0 / 30.0 };
    // 1/4 de quadro antes do primeiro: o 1º quadro com pts >= alvo é ele (igual ao extract_frame)
    let alvo = (tempos[0] - dur * 0.25).max(0.0);
    let grosso = (alvo - 2.0 - start_time_s.max(0.0)).max(0.0);
    let mut child = crate::tools::command(&ffmpeg)
        .args(["-v", "error", "-nostdin", "-ss", &format!("{grosso:.6}"), "-copyts", "-i"])
        .arg(video)
        .args(["-ss", &format!("{alvo:.6}"), "-map", "0:v:0", "-frames:v", &tempos.len().to_string()])
        .args(["-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| SicroError::Workspace(format!("could not spawn ffmpeg: {e}")))?;
    let mut out = child.stdout.take().ok_or_else(|| SicroError::Workspace("ffmpeg sem saída".into()))?;
    let mut buf = vec![0u8; (w * h * 3) as usize];
    let (rx, ry, rw, rh) = (rect.0 as usize, rect.1 as usize, rect.2 as usize, rect.3 as usize);
    let mut recortes = Vec::with_capacity(tempos.len());
    let mut quadro_ref = Vec::new();
    let resultado = (|| -> Result<()> {
        for i in 0..tempos.len() {
            if cancelar.load(Ordering::Relaxed) {
                return Err(SicroError::Validation("cancelado".into()));
            }
            let mut got = 0;
            while got < buf.len() {
                let n = out.read(&mut buf[got..])?;
                if n == 0 {
                    return Err(SicroError::Validation(format!(
                        "o vídeo acabou depois de {i} quadros (esperava {})",
                        tempos.len()
                    )));
                }
                got += n;
            }
            let quadro = match lente {
                Some(l) => corrigir_rgb(&buf, w, h, l.coef, l.cx, l.cy),
                None => buf.clone(),
            };
            let mut rgb = Vec::with_capacity(rw * rh * 3);
            for y in ry..ry + rh {
                let a = (y * w as usize + rx) * 3;
                rgb.extend_from_slice(&quadro[a..a + rw * 3]);
            }
            recortes.push(Recorte { rgb, w: rw, h: rh });
            if i == ref_pos {
                quadro_ref = quadro;
            }
        }
        Ok(())
    })();
    let _ = child.kill();
    let _ = child.wait();
    resultado?;
    Ok(JanelaLida { recortes, quadro_ref })
}
