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
    /// Quadros a partir do instante `inicio_s` (busca exata), no tamanho `w`×`h`.
    /// `inicio_s` é absoluto (pts do ffprobe): `-seek_timestamp 1` impede o `-ss` de
    /// entrada de somar o `start_time` do arquivo.
    pub fn open_from(path: &Path, inicio_s: f64, w: u32, h: u32) -> Result<Self> {
        let ffmpeg = detect_ffmpeg()?;
        let vf = format!("scale={w}:{h}:flags=area,setsar=1");
        let mut child = crate::tools::command(&ffmpeg)
            .args(["-v", "error", "-nostdin", "-seek_timestamp", "1"])
            .args(["-ss", &format!("{:.4}", inicio_s.max(0.0)), "-i"])
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Vídeo sintético 320×240, 25 qps, 6 s, que começa em 3 s (`-output_ts_offset 3`).
    fn gerar(dir: &Path, nome: &str, entrada: &[&str], filtro: Option<&str>) -> Option<PathBuf> {
        let ffmpeg = detect_ffmpeg().ok()?;
        let p = dir.join(nome);
        let mut cmd = crate::tools::command(ffmpeg);
        cmd.args(["-hide_banner", "-loglevel", "error"]).args(entrada);
        if let Some(f) = filtro {
            cmd.args(["-filter_complex", f]);
        }
        let ok = cmd
            .args(["-t", "6", "-c:v", "libx264", "-qp", "0", "-g", "25", "-pix_fmt", "yuv420p"])
            .args(["-output_ts_offset", "3", "-y"])
            .arg(&p)
            .status()
            .ok()?
            .success();
        ok.then_some(p)
    }

    /// Com `SICRO_EXIGE_FFMPEG=1` a falta do ffmpeg reprova em vez de pular (§11).
    fn sem_ffmpeg() {
        assert!(std::env::var_os("SICRO_EXIGE_FFMPEG").is_none(), "ffmpeg ausente ou sem libx264");
        eprintln!("ffmpeg ausente — teste pulado");
    }

    #[test]
    fn open_from_em_arquivo_que_comeca_em_3s_le_o_quadro_pedido() {
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = gerar(dir.path(), "d.mp4", &["-f", "lavfi", "-i", "testsrc=size=320x240:rate=25"], None) else {
            sem_ffmpeg();
            return;
        };
        let times = frame_times(&src).unwrap();
        assert_eq!(times.len(), 150);
        assert!((times[0] - 3.0).abs() < 1e-3, "início {}", times[0]);

        let mut seq = FrameReader::open(&src, 1, 320, 240).unwrap();
        let mut todos = Vec::new();
        while let Some(f) = seq.next().unwrap() {
            todos.push(f.to_vec());
        }
        assert_eq!(todos.len(), 150);
        for idx in [0usize, 1, 57, 149] {
            let mut r = FrameReader::open_from(&src, times[idx] - 0.0005, 320, 240).unwrap();
            let f = r.next().unwrap().expect("quadro");
            assert!(f == todos[idx].as_slice(), "busca em {:.4} não deu o quadro {idx}", times[idx]);
        }
    }

    #[test]
    fn mancha_em_arquivo_que_comeca_em_3s_parte_do_quadro_certo() {
        use crate::video::pessoas::mancha::{seguir, OpcoesMancha};
        use std::sync::atomic::AtomicBool;
        // Bloco texturizado 40×60 em x = 30 + 3n, y = 80, só nos quadros n < 40.
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = gerar(
            dir.path(),
            "m.mp4",
            &[
                "-f", "lavfi", "-i", "color=c=0x404040:s=320x240:r=25",
                "-f", "lavfi", "-i", "color=c=black:s=40x60:r=25",
            ],
            Some("[1:v]format=gray,geq=lum='mod(X*X*31+Y*Y*17+X*Y*7\\,256)'[b];[0:v][b]overlay=x='30+3*n':y=80:enable='lt(n\\,40)',format=yuv420p"),
        ) else {
            sem_ffmpeg();
            return;
        };
        let times = frame_times(&src).unwrap();
        assert!((times[0] - 3.0).abs() < 1e-3, "início {}", times[0]);
        let idx0 = 10;
        let r = seguir(
            &src,
            &times,
            320,
            240,
            idx0,
            (30.0 + 3.0 * idx0 as f32, 80.0, 40.0, 60.0),
            true,
            OpcoesMancha { min_conf: 0.55, max_quadros: 20 },
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .expect("rastreio a partir do quadro 10");
        assert_eq!(r.amostras.len(), 21);
        for (k, a) in r.amostras.iter().enumerate() {
            let n = idx0 + k;
            assert!((a.t - times[n]).abs() < 1e-9);
            let x = 30.0 + 3.0 * n as f32;
            assert!((a.x - x).abs() <= 1.0 && (a.y - 80.0).abs() <= 1.0, "quadro {n}: ({}, {}) vs ({x}, 80)", a.x, a.y);
        }
    }
}
