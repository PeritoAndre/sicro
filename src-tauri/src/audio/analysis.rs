//! Análise de áudio em Rust puro (`hound` + `rustfft`): medições objetivas e
//! espectro (Welch). Só lê o WAV de análise; nunca altera o áudio.

use std::path::Path;

use rustfft::{num_complex::Complex, FftPlanner};
use serde::Serialize;

use crate::error::{Result, SicroError};

const DB_FLOOR: f32 = -120.0;

// ---------------------------------------------------------------------------
// Leitura do WAV → mono f32 [-1,1]

/// Lê um WAV PCM (int 8/16/24/32 ou float) → (mono f32 em [-1, 1] pela média
/// dos canais, taxa, nº de canais).
pub fn read_wav_mono(path: &Path) -> Result<(Vec<f32>, u32, u16)> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| SicroError::Validation(format!("não foi possível ler o WAV: {e}")))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1);
    let sr = spec.sample_rate;

    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.unwrap_or(0.0))
            .collect(),
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample.max(1) as u32;
            let max = (1i64 << (bits - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.unwrap_or(0) as f32 / max)
                .collect()
        }
    };

    let ch = channels as usize;
    let frames = interleaved.len() / ch;
    let mut mono = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut acc = 0.0f32;
        for c in 0..ch {
            acc += interleaved[f * ch + c];
        }
        mono.push(acc / ch as f32);
    }
    Ok((mono, sr, channels))
}

#[inline]
fn to_dbfs(linear: f32) -> f32 {
    if linear <= 1e-7 {
        DB_FLOOR
    } else {
        (20.0 * linear.log10()).max(DB_FLOOR)
    }
}

// ---------------------------------------------------------------------------
// Medições objetivas

#[derive(Debug, Clone, Serialize)]
pub struct AudioMeasurements {
    pub duration_s: f64,
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: u64,
    /// Pico absoluto (linear 0..1) e em dBFS.
    pub peak_linear: f32,
    pub peak_dbfs: f32,
    /// RMS (raiz da média dos quadrados) em dBFS — não ponderado.
    pub rms_dbfs: f32,
    /// Fator de crista (pico − RMS, em dB) — indica dinâmica/compressão.
    pub crest_factor_db: f32,
    /// Offset DC (média do sinal) linear e em % de fundo de escala.
    pub dc_offset: f32,
    pub dc_offset_pct: f32,
    /// Clipping: nº de amostras saturadas, nº de "corridas" contíguas e %.
    pub clipped_samples: u64,
    pub clipped_runs: u64,
    pub clipped_pct: f32,
    /// Medições do FFmpeg (ruído de fundo, loudness, silêncios); None sem ffmpeg.
    #[serde(default)]
    pub extended: Option<ExtendedMeasurements>,
}

/// Medições objetivas feitas pelo FFmpeg (`astats`, `ebur128`, `silencedetect`).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ExtendedMeasurements {
    /// Piso de ruído (dB) estimado pelo `astats`.
    pub noise_floor_db: Option<f32>,
    /// Profundidade de bits EFETIVA (usada pelo sinal) e a do arquivo.
    pub bit_depth_effective: Option<u32>,
    pub bit_depth_container: Option<u32>,
    /// Loudness integrado (LUFS), faixa de loudness (LU) e true peak (dBFS) — EBU R128.
    pub integrated_lufs: Option<f32>,
    pub loudness_range_lu: Option<f32>,
    pub true_peak_dbfs: Option<f32>,
    /// Silêncios abaixo de `silence_threshold_db` por pelo menos `silence_min_s`.
    pub silence_threshold_db: f32,
    pub silence_min_s: f32,
    pub silences: Vec<(f64, f64)>,
}

/// Lê a saída (stderr) do ffmpeg com `astats`, `ebur128` e `silencedetect`.
pub fn parse_extended(stderr: &str, threshold_db: f32, min_s: f32, duration_s: f64) -> ExtendedMeasurements {
    // "-inf" (sinal digital zerado) vira None.
    let num = |s: &str| -> Option<f32> {
        s.split_whitespace().next().and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite())
    };
    let mut m = ExtendedMeasurements {
        silence_threshold_db: threshold_db,
        silence_min_s: min_s,
        ..Default::default()
    };
    let mut in_overall = false;
    let mut open: Option<f64> = None;
    for line in stderr.lines() {
        let l = line.trim();
        if line.contains("Parsed_astats") {
            if l.ends_with("Overall") {
                in_overall = true;
                continue;
            }
            if in_overall {
                let body = l.splitn(2, "] ").nth(1).unwrap_or("");
                if let Some(v) = body.strip_prefix("Noise floor dB:") {
                    m.noise_floor_db = num(v);
                } else if let Some(v) = body.strip_prefix("Bit depth:") {
                    let parts: Vec<u32> = v.trim().split('/').filter_map(|x| x.trim().parse().ok()).collect();
                    m.bit_depth_effective = parts.first().copied();
                    m.bit_depth_container = parts.get(1).copied();
                }
            }
        } else if let Some(v) = l.strip_prefix("I:") {
            m.integrated_lufs = num(v);
        } else if let Some(v) = l.strip_prefix("LRA:") {
            m.loudness_range_lu = num(v);
        } else if let Some(v) = l.strip_prefix("Peak:") {
            m.true_peak_dbfs = num(v);
        } else if let Some(i) = l.find("silence_start:") {
            open = l[i + 14..].split_whitespace().next().and_then(|v| v.parse().ok());
        } else if let Some(i) = l.find("silence_end:") {
            let end: Option<f64> = l[i + 12..].split_whitespace().next().and_then(|v| v.parse().ok());
            if let (Some(a), Some(b)) = (open.take(), end) {
                m.silences.push((a.max(0.0), b));
            }
        }
    }
    // Silêncio que vai até o fim do arquivo não tem "silence_end".
    if let Some(a) = open {
        m.silences.push((a.max(0.0), duration_s));
    }
    m
}

/// `clip_threshold` em [0, 1]; 0.997 ≈ fundo de escala.
pub fn measure(samples: &[f32], sr: u32, channels: u16, clip_threshold: f32) -> AudioMeasurements {
    let n = samples.len();
    let thr = clip_threshold.clamp(0.5, 1.0);
    let mut peak = 0.0f32;
    let mut sum_sq = 0.0f64;
    let mut sum = 0.0f64;
    let mut clipped = 0u64;
    let mut runs = 0u64;
    let mut in_run = false;
    for &s in samples {
        let a = s.abs();
        if a > peak {
            peak = a;
        }
        sum_sq += (s as f64) * (s as f64);
        sum += s as f64;
        if a >= thr {
            clipped += 1;
            if !in_run {
                runs += 1;
                in_run = true;
            }
        } else {
            in_run = false;
        }
    }
    let rms = if n > 0 {
        (sum_sq / n as f64).sqrt() as f32
    } else {
        0.0
    };
    let dc = if n > 0 { (sum / n as f64) as f32 } else { 0.0 };
    let peak_db = to_dbfs(peak);
    let rms_db = to_dbfs(rms);
    AudioMeasurements {
        duration_s: if sr > 0 { n as f64 / sr as f64 } else { 0.0 },
        sample_rate: sr,
        channels,
        samples: n as u64,
        peak_linear: peak,
        peak_dbfs: peak_db,
        rms_dbfs: rms_db,
        crest_factor_db: peak_db - rms_db,
        dc_offset: dc,
        dc_offset_pct: dc * 100.0,
        clipped_samples: clipped,
        clipped_runs: runs,
        clipped_pct: if n > 0 {
            clipped as f32 / n as f32 * 100.0
        } else {
            0.0
        },
        extended: None,
    }
}

// ---------------------------------------------------------------------------
// Janelas

fn hann(n: usize) -> Vec<f32> {
    if n <= 1 {
        return vec![1.0; n.max(1)];
    }
    (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n as f32 - 1.0)).cos())
        .collect()
}

// ---------------------------------------------------------------------------
// Espectro (Welch)

#[derive(Debug, Clone, Serialize)]
pub struct SpectrumResult {
    pub sample_rate: u32,
    pub fft_size: usize,
    pub window: String,
    /// Frequência central de cada bin (Hz), 0..sr/2.
    pub freqs_hz: Vec<f32>,
    /// Magnitude média em dB (normalizada: senoide 0 dBFS ≈ 0 dB).
    pub mag_db: Vec<f32>,
    pub peak_freq_hz: f32,
    pub peak_db: f32,
}

/// Welch: blocos de `fft_size` (potência de 2, 256..65536) com 50% de
/// sobreposição, Hann, média de potência. Normalizado pelo ganho coerente da
/// janela: senoide de fundo de escala ≈ 0 dB.
pub fn spectrum(samples: &[f32], sr: u32, fft_size: usize) -> SpectrumResult {
    let n = fft_size.clamp(256, 65536).next_power_of_two();
    let bins = n / 2 + 1;
    let win = hann(n);
    let coherent_gain: f32 = win.iter().sum::<f32>() / n as f32;

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);

    let hop = n / 2;
    let mut power = vec![0.0f64; bins];
    let mut blocks = 0u32;

    if samples.len() >= n {
        let mut start = 0;
        while start + n <= samples.len() {
            let mut buf: Vec<Complex<f32>> = (0..n)
                .map(|i| Complex::new(samples[start + i] * win[i], 0.0))
                .collect();
            fft.process(&mut buf);
            for (k, p) in power.iter_mut().enumerate() {
                let mag = buf[k].norm() / (n as f32 * coherent_gain);
                // ×2 para bins não-DC/Nyquist (energia do lado negativo).
                let scale = if k == 0 || k == bins - 1 { 1.0 } else { 2.0 };
                *p += (mag * scale) as f64 * (mag * scale) as f64;
            }
            blocks += 1;
            start += hop;
        }
    }

    let mut freqs = Vec::with_capacity(bins);
    let mut mag_db = Vec::with_capacity(bins);
    let mut peak_db = DB_FLOOR;
    let mut peak_freq = 0.0f32;
    for k in 0..bins {
        let f = k as f32 * sr as f32 / n as f32;
        let avg_pow = if blocks > 0 {
            (power[k] / blocks as f64) as f32
        } else {
            0.0
        };
        let db = to_dbfs(avg_pow.sqrt());
        if db > peak_db {
            peak_db = db;
            peak_freq = f;
        }
        freqs.push(f);
        mag_db.push(db);
    }

    SpectrumResult {
        sample_rate: sr,
        fft_size: n,
        window: "hann".to_string(),
        freqs_hz: freqs,
        mag_db,
        peak_freq_hz: peak_freq,
        peak_db,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn sine(freq: f32, amp: f32, dur_s: f32, sr: u32) -> Vec<f32> {
        let n = (sr as f32 * dur_s) as usize;
        (0..n)
            .map(|i| amp * (2.0 * PI * freq * i as f32 / sr as f32).sin())
            .collect()
    }

    #[test]
    fn measure_full_scale_sine_peak_near_0dbfs() {
        let s = sine(1000.0, 1.0, 0.5, 48000);
        let m = measure(&s, 48000, 1, 0.997);
        assert!(m.peak_dbfs > -0.5, "pico deveria ~0 dBFS, veio {}", m.peak_dbfs);
        // RMS de senoide = pico/√2 ≈ -3.01 dB rel. ao pico.
        assert!((m.rms_dbfs - (-3.01)).abs() < 0.3, "rms={}", m.rms_dbfs);
    }

    #[test]
    fn measure_detects_dc_offset() {
        let mut s = sine(440.0, 0.3, 0.2, 48000);
        for v in s.iter_mut() {
            *v += 0.25;
        }
        let m = measure(&s, 48000, 1, 0.997);
        assert!((m.dc_offset - 0.25).abs() < 0.01, "dc={}", m.dc_offset);
    }

    #[test]
    fn measure_detects_clipping_runs() {
        let mut s = vec![0.0f32; 100];
        for v in &mut s[10..15] {
            *v = 1.0;
        }
        for v in &mut s[40..42] {
            *v = -1.0;
        }
        for v in &mut s[70..73] {
            *v = 1.0;
        }
        let m = measure(&s, 48000, 1, 0.997);
        assert_eq!(m.clipped_samples, 5 + 2 + 3);
        assert_eq!(m.clipped_runs, 3);
    }

    #[test]
    fn spectrum_peak_at_input_frequency() {
        let s = sine(1000.0, 0.9, 1.0, 48000);
        let sp = spectrum(&s, 48000, 4096);
        assert!(
            (sp.peak_freq_hz - 1000.0).abs() < 30.0,
            "pico em {} Hz",
            sp.peak_freq_hz
        );
    }
}

#[cfg(test)]
mod extended_tests {
    use super::*;

    #[test]
    fn le_astats_ebur128_e_silencios() {
        let err = "[Parsed_astats_0 @ 0x1] Channel: 1\n[Parsed_astats_0 @ 0x1] Noise floor dB: -70.0\n[Parsed_astats_0 @ 0x1] Overall\n[Parsed_astats_0 @ 0x1] Peak level dB: -16.08\n[Parsed_astats_0 @ 0x1] Noise floor dB: -61.25\n[Parsed_astats_0 @ 0x1] Bit depth: 13/16/16/16\n[silencedetect @ 0x2] silence_start: 1.5\n[silencedetect @ 0x2] silence_end: 3.25 | silence_duration: 1.75\n[silencedetect @ 0x2] silence_start: 9\n[Parsed_ebur128_1 @ 0x3] Summary:\n    I:         -21.8 LUFS\n    LRA:         4.2 LU\n    Peak:      -16.1 dBFS\n";
        let m = parse_extended(err, -50.0, 0.5, 10.0);
        assert_eq!(m.noise_floor_db, Some(-61.25));
        assert_eq!(m.bit_depth_effective, Some(13));
        assert_eq!(m.bit_depth_container, Some(16));
        assert_eq!(m.integrated_lufs, Some(-21.8));
        assert_eq!(m.loudness_range_lu, Some(4.2));
        assert_eq!(m.true_peak_dbfs, Some(-16.1));
        assert_eq!(m.silences, vec![(1.5, 3.25), (9.0, 10.0)]);
    }
}
