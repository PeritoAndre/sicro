//! Espectrograma interativo: só a janela pedida pela tela (áudio em cache).
//! Cada coluna é o MÁXIMO de alguns quadros de FFT no seu intervalo (clique
//! curto não some afastado). u8: 0 = −120 dB … 255 = 0 dB (seno a fundo de escala).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rustfft::{num_complex::Complex32, FftPlanner};
use serde::Serialize;

use crate::error::Result;

pub const DB_FLOOR: f32 = -120.0;

#[derive(Debug, Clone, Serialize)]
pub struct SpectroImage {
    pub width: usize,
    pub height: usize,
    pub t0: f64,
    pub t1: f64,
    pub f_min: f32,
    pub f_max: f32,
    pub log_freq: bool,
    pub fft_size: usize,
    pub sample_rate: u32,
    pub duration_s: f64,
    /// width × height valores u8, linha 0 = frequência MAIS ALTA, em base64.
    pub data_b64: String,
}

pub struct Request {
    pub t0: f64,
    pub t1: f64,
    pub width: usize,
    pub height: usize,
    pub fft_size: usize,
    pub log_freq: bool,
    pub f_max: Option<f32>,
}

/// Último áudio lido (o arraste e o zoom pedem várias vezes o mesmo).
static CACHE: Mutex<Option<(PathBuf, std::time::SystemTime, std::sync::Arc<Vec<f32>>, u32)>> = Mutex::new(None);

fn samples_of(wav: &Path) -> Result<(std::sync::Arc<Vec<f32>>, u32)> {
    let mtime = std::fs::metadata(wav).and_then(|m| m.modified()).ok();
    if let Ok(guard) = CACHE.lock() {
        if let Some((p, t, s, sr)) = guard.as_ref() {
            if p == wav && Some(*t) == mtime {
                return Ok((s.clone(), *sr));
            }
        }
    }
    let (samples, sr, _) = super::analysis::read_wav_mono(wav)?;
    let arc = std::sync::Arc::new(samples);
    if let (Ok(mut guard), Some(t)) = (CACHE.lock(), mtime) {
        *guard = Some((wav.to_path_buf(), t, arc.clone(), sr));
    }
    Ok((arc, sr))
}

pub fn render(wav: &Path, req: &Request) -> Result<SpectroImage> {
    let (samples, sr) = samples_of(wav)?;
    Ok(render_samples(&samples, sr, req))
}

pub fn render_samples(samples: &[f32], sr: u32, req: &Request) -> SpectroImage {
    use base64::Engine as _;
    let n = req.fft_size.clamp(256, 16384).next_power_of_two();
    let width = req.width.clamp(16, 4096);
    let height = req.height.clamp(16, 1024);
    let duration = samples.len() as f64 / sr.max(1) as f64;
    let t0 = req.t0.clamp(0.0, duration);
    let t1 = req.t1.clamp(t0 + 1e-3, duration.max(t0 + 1e-3));
    let nyq = sr as f32 / 2.0;
    let f_max = req.f_max.unwrap_or(nyq).clamp(100.0, nyq);
    let f_min = if req.log_freq { 20.0f32.min(f_max / 2.0) } else { 0.0 };

    let window: Vec<f32> = (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos())
        .collect();
    // Seno a fundo de escala: |X| ≈ N/4 com janela Hann → 0 dB.
    let norm = n as f32 / 4.0;
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let bins = n / 2 + 1;
    let bin_hz = sr as f32 / n as f32;

    // Linha → faixa de bins [lo, hi] (a linha 0 é a frequência mais alta).
    let row_freq = |r: f32| -> f32 {
        let u = r / height as f32;
        if req.log_freq {
            f_min * (f_max / f_min).powf(u)
        } else {
            f_min + (f_max - f_min) * u
        }
    };
    let rows: Vec<(usize, usize)> = (0..height)
        .map(|i| {
            let r = (height - 1 - i) as f32;
            let lo = (row_freq(r) / bin_hz).floor() as usize;
            let hi = ((row_freq(r + 1.0) / bin_hz).ceil() as usize).max(lo);
            (lo.min(bins - 1), hi.min(bins - 1))
        })
        .collect();

    let span = (t1 - t0) * sr as f64 / width as f64; // amostras por coluna
    let sub = ((span / (n as f64 / 2.0)).ceil() as usize).clamp(1, 8);
    let mut out = vec![0u8; width * height];
    let mut buf = vec![Complex32::new(0.0, 0.0); n];
    let mut col_db = vec![DB_FLOOR; bins];
    for c in 0..width {
        col_db.iter_mut().for_each(|v| *v = DB_FLOOR);
        let c0 = t0 * sr as f64 + span * c as f64;
        // Quadros espaçados na coluna + um centrado no pico (afastado, um
        // transiente curto não pode cair entre os quadros e sumir).
        let mut centers: Vec<f64> = (0..sub).map(|k| c0 + span * (k as f64 + 0.5) / sub as f64).collect();
        if span > n as f64 {
            let a = (c0.max(0.0) as usize).min(samples.len());
            let b = ((c0 + span) as usize).min(samples.len());
            if let Some((i, _)) = samples[a..b].iter().enumerate().max_by(|x, y| x.1.abs().total_cmp(&y.1.abs())) {
                centers.push((a + i) as f64);
            }
        }
        for center in centers {
            let start = center as i64 - n as i64 / 2;
            for (i, b) in buf.iter_mut().enumerate() {
                let idx = start + i as i64;
                let v = if idx >= 0 && (idx as usize) < samples.len() { samples[idx as usize] } else { 0.0 };
                *b = Complex32::new(v * window[i], 0.0);
            }
            fft.process(&mut buf);
            for (k2, v) in col_db.iter_mut().enumerate() {
                let db = 20.0 * (buf[k2].norm() / norm + 1e-12).log10();
                if db > *v {
                    *v = db;
                }
            }
        }
        for (r, &(lo, hi)) in rows.iter().enumerate() {
            let m = col_db[lo..=hi].iter().copied().fold(DB_FLOOR, f32::max);
            let q = ((m - DB_FLOOR) / -DB_FLOOR * 255.0).round().clamp(0.0, 255.0) as u8;
            out[r * width + c] = q;
        }
    }

    SpectroImage {
        width,
        height,
        t0,
        t1,
        f_min,
        f_max,
        log_freq: req.log_freq,
        fft_size: n,
        sample_rate: sr,
        duration_s: duration,
        data_b64: base64::engine::general_purpose::STANDARD.encode(&out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    #[test]
    fn seno_aparece_na_linha_certa_com_0_db() {
        let sr: u32 = 48000;
        let x: Vec<f32> = (0..sr as usize * 2)
            .map(|i| (2.0 * std::f32::consts::PI * 3000.0 * i as f32 / sr as f32).sin())
            .collect();
        let req = Request { t0: 0.0, t1: 2.0, width: 64, height: 240, fft_size: 4096, log_freq: false, f_max: Some(12000.0) };
        let img = render_samples(&x, sr, &req);
        let data = base64::engine::general_purpose::STANDARD.decode(&img.data_b64).unwrap();
        let col = 32;
        let (row, &v) = (0..img.height)
            .map(|r| (r, &data[r * img.width + col]))
            .max_by_key(|(_, v)| **v)
            .unwrap();
        // Linha 0 = 12 kHz; 3 kHz fica a 3/4 da altura.
        let f = 12000.0 * (1.0 - (row as f32 + 0.5) / img.height as f32);
        assert!((f - 3000.0).abs() < 100.0, "pico em {f} Hz");
        // ≈ 0 dB → perto de 255.
        assert!(v >= 245, "valor {v}");
    }

    #[test]
    fn clique_curto_aparece_mesmo_afastado() {
        let sr: u32 = 48000;
        let mut x = vec![0.0f32; sr as usize * 60];
        x[sr as usize * 30] = 1.0; // um clique de uma amostra no meio de 1 minuto
        let req = Request { t0: 0.0, t1: 60.0, width: 200, height: 64, fft_size: 1024, log_freq: true, f_max: None };
        let img = render_samples(&x, sr, &req);
        let data = base64::engine::general_purpose::STANDARD.decode(&img.data_b64).unwrap();
        let col_max = |c: usize| (0..img.height).map(|r| data[r * img.width + c]).max().unwrap();
        assert!(col_max(100) > 60, "o clique sumiu no zoom afastado");
        assert_eq!(col_max(10), 0);
    }
}
