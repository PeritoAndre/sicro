//! Leitura de quadros do vídeo pelo ffmpeg em pipe (rawvideo RGB). Os tempos de
//! cada quadro vêm do ffprobe (pts de apresentação), não de uma conta com fps.

use std::io::Read;
use std::path::Path;
use std::process::{Child, ChildStdout, Stdio};

use crate::error::{Result, SicroError};
use crate::video::frame_export::detect_ffmpeg;
use crate::video::probe::detect_ffprobe;

/// Tamanho com que o player mostra o vídeo (giro e proporção do pixel aplicados).
pub fn display_size(path: &Path) -> Result<(u32, u32)> {
    let ffprobe = detect_ffprobe()?;
    let out = crate::tools::command(&ffprobe)
        .args(["-v", "error", "-select_streams", "v:0", "-show_streams", "-of", "json"])
        .arg(path)
        .output()
        .map_err(|e| SicroError::Workspace(format!("could not spawn ffprobe: {e}")))?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let s = v["streams"].get(0).ok_or_else(|| SicroError::Validation("vídeo sem imagem".into()))?;
    let (w, h) = (s["width"].as_f64().unwrap_or(0.0), s["height"].as_f64().unwrap_or(0.0));
    if w < 2.0 || h < 2.0 {
        return Err(SicroError::Validation("dimensões do vídeo ilegíveis".into()));
    }
    let sar = s["sample_aspect_ratio"]
        .as_str()
        .and_then(|r| {
            let (a, b) = r.split_once(':')?;
            let (a, b) = (a.parse::<f64>().ok()?, b.parse::<f64>().ok()?);
            (a > 0.0 && b > 0.0).then_some(a / b)
        })
        .unwrap_or(1.0);
    let rot = s["side_data_list"]
        .as_array()
        .and_then(|l| l.iter().find_map(|d| d["rotation"].as_f64()))
        .or_else(|| s["tags"]["rotate"].as_str().and_then(|r| r.parse::<f64>().ok()))
        .unwrap_or(0.0);
    let (mut w, h) = ((w * sar).round(), h);
    let (w2, h2) = if (rot.abs() as i64) % 180 == 90 { (h, w) } else { (w, h) };
    w = w2;
    let even = |v: f64| ((v as u32) / 2 * 2).max(2);
    Ok((even(w), even(h2)))
}

/// Instantes (s) de todos os quadros de vídeo, em ordem de apresentação.
pub fn frame_times(path: &Path) -> Result<Vec<f64>> {
    let t = crate::video::clip::all_frame_times(path)?;
    if t.is_empty() {
        return Err(SicroError::Validation("nenhum quadro de vídeo encontrado".into()));
    }
    Ok(t)
}

/// Intervalo típico entre quadros (mediana).
pub fn frame_dur(times: &[f64]) -> f64 {
    let mut d: Vec<f64> = times.windows(2).map(|w| w[1] - w[0]).filter(|d| *d > 0.0).collect();
    d.sort_by(|a, b| a.total_cmp(b));
    d.get(d.len() / 2).copied().unwrap_or(1.0 / 30.0)
}

pub struct FrameReader {
    child: Child,
    out: ChildStdout,
    buf: Vec<u8>,
}

impl FrameReader {
    /// `passo` > 1 entrega só um quadro a cada `passo`; `w`×`h` é o tamanho de saída.
    pub fn open(path: &Path, passo: u32, w: u32, h: u32) -> Result<Self> {
        let ffmpeg = detect_ffmpeg()?;
        let mut vf = String::new();
        if passo > 1 {
            vf.push_str(&format!("select='not(mod(n\\,{passo}))',"));
        }
        vf.push_str(&format!("scale={w}:{h}:flags=area,setsar=1"));
        let mut child = crate::tools::command(&ffmpeg)
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(path)
            .args(["-map", "0:v:0", "-vf", &vf, "-fps_mode", "passthrough"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| SicroError::Workspace(format!("could not spawn ffmpeg: {e}")))?;
        let out = child.stdout.take().ok_or_else(|| SicroError::Workspace("ffmpeg sem saída".into()))?;
        Ok(Self { child, out, buf: vec![0u8; (w * h * 3) as usize] })
    }

    /// Próximo quadro RGB, ou `None` no fim.
    pub fn next(&mut self) -> Result<Option<&[u8]>> {
        let mut got = 0;
        while got < self.buf.len() {
            let n = self.out.read(&mut self.buf[got..])?;
            if n == 0 {
                return Ok(None);
            }
            got += n;
        }
        Ok(Some(&self.buf))
    }
}

impl Drop for FrameReader {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
