//! W12 (paridade Audacity) — **Análise forense de áudio** em Rust puro.
//!
//! Inspiração: Audacity → Analyze (Plot Spectrum, Find Clipping, Sample Data
//! Export) + técnica forense de **ENF** (Electric Network Frequency). Tudo
//! aqui é ANÁLISE/MEDIÇÃO — lê o WAV de análise (PCM, já produzido pelo
//! pipeline FFmpeg) e devolve NÚMEROS determinísticos e reproduzíveis. NÃO
//! altera o áudio (realce continua via cadeia FFmpeg). Sem fabricar nada (§13).
//!
//! Núcleo DSP: `hound` (lê WAV) + `rustfft` (FFT). Determinístico e testável.

use std::path::Path;

use rustfft::{num_complex::Complex, FftPlanner};
use serde::Serialize;

use crate::error::{Result, SicroError};

const DB_FLOOR: f32 = -120.0;

// ---------------------------------------------------------------------------
// Leitura do WAV → mono f32 [-1,1]

/// Lê um WAV PCM (int 8/16/24/32 ou float) e devolve (amostras mono f32, taxa,
/// nº de canais). Canais são somados → mono (média) para a análise.
pub fn read_wav_mono(path: &Path) -> Result<(Vec<f32>, u32, u16)> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| SicroError::Validation(format!("não foi possível ler o WAV: {e}")))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1);
    let sr = spec.sample_rate;

    // Normaliza cada amostra para [-1,1] conforme o formato.
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

    // Intercalado → mono (média dos canais).
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
// R4 — Medições objetivas

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
}

/// Calcula as medições objetivas (§ Sample Data Export + Find Clipping do
/// Audacity). `clip_threshold` em [0,1]; 0.997 ≈ fundo de escala.
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
    }
}

// ---------------------------------------------------------------------------
// Janelas

/// Janela de Hann de N pontos.
fn hann(n: usize) -> Vec<f32> {
    if n <= 1 {
        return vec![1.0; n.max(1)];
    }
    (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n as f32 - 1.0)).cos())
        .collect()
}

// ---------------------------------------------------------------------------
// R3 — Espectro (Welch: FFT janelada + média de blocos)

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

/// Plot Spectrum (Welch). `fft_size` potência de 2 (256..65536). Blocos com
/// 50% de sobreposição, janela de Hann, média de potência. Normaliza pelo
/// ganho coerente da janela para que uma senoide de fundo de escala leia ~0 dB.
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

// ---------------------------------------------------------------------------
// R8 — ENF (Electric Network Frequency)

#[derive(Debug, Clone, Serialize)]
pub struct EnfResult {
    /// Nominal da rede (50 ou 60 Hz).
    pub nominal_hz: f32,
    pub window_s: f32,
    pub step_s: f32,
    /// Tempo central de cada quadro (s) e frequência estimada (Hz).
    pub times_s: Vec<f32>,
    pub enf_hz: Vec<f32>,
    pub mean_hz: f32,
    pub std_hz: f32,
    /// Maior salto frame-a-frame (Hz) — descontinuidade = indício de edição.
    pub max_jump_hz: f32,
}

/// Extrai a curva ENF: por quadro longo (janela Hann), FFT com zero-pad,
/// pega o pico na banda [nominal−1, nominal+1] Hz com interpolação parabólica
/// (precisão sub-bin). Mede média/desvio e o maior salto (indicador de
/// splice). Reprodutível; o cruzamento com banco de dados de rede fica fora
/// de escopo (extração + continuidade são 100% locais e determinísticos).
pub fn enf(samples: &[f32], sr: u32, nominal_hz: f32, window_s: f32, step_s: f32) -> EnfResult {
    let nominal = if nominal_hz < 55.0 { 50.0 } else { 60.0 };
    let win_s = window_s.clamp(2.0, 30.0);
    let stp_s = step_s.clamp(0.5, win_s);
    let win_n = ((sr as f32 * win_s) as usize).max(2);
    let hop = ((sr as f32 * stp_s) as usize).max(1);
    let fft_n = win_n.next_power_of_two();
    let win = hann(win_n);

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(fft_n);
    let bin_hz = sr as f32 / fft_n as f32;
    let lo_bin = ((nominal - 1.0) / bin_hz).floor().max(1.0) as usize;
    let hi_bin = ((nominal + 1.0) / bin_hz).ceil() as usize;

    let mut times = Vec::new();
    let mut enf = Vec::new();
    if samples.len() >= win_n {
        let mut start = 0;
        while start + win_n <= samples.len() {
            let mut buf: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); fft_n];
            for i in 0..win_n {
                buf[i] = Complex::new(samples[start + i] * win[i], 0.0);
            }
            fft.process(&mut buf);
            // Pico na banda.
            let mut peak_k = lo_bin;
            let mut peak_m = 0.0f32;
            for k in lo_bin..=hi_bin.min(fft_n / 2) {
                let m = buf[k].norm();
                if m > peak_m {
                    peak_m = m;
                    peak_k = k;
                }
            }
            // Interpolação parabólica sub-bin.
            let freq = if peak_k >= 1 && peak_k + 1 <= fft_n / 2 {
                let a = buf[peak_k - 1].norm();
                let b = buf[peak_k].norm();
                let c = buf[peak_k + 1].norm();
                let denom = a - 2.0 * b + c;
                let delta = if denom.abs() > 1e-9 {
                    0.5 * (a - c) / denom
                } else {
                    0.0
                };
                (peak_k as f32 + delta) * bin_hz
            } else {
                peak_k as f32 * bin_hz
            };
            times.push((start as f32 + win_n as f32 / 2.0) / sr as f32);
            enf.push(freq);
            start += hop;
        }
    }

    let mean = if enf.is_empty() {
        nominal
    } else {
        enf.iter().sum::<f32>() / enf.len() as f32
    };
    let std = if enf.len() > 1 {
        (enf.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / enf.len() as f32).sqrt()
    } else {
        0.0
    };
    let mut max_jump = 0.0f32;
    for w in enf.windows(2) {
        let j = (w[1] - w[0]).abs();
        if j > max_jump {
            max_jump = j;
        }
    }

    EnfResult {
        nominal_hz: nominal,
        window_s: win_s,
        step_s: stp_s,
        times_s: times,
        enf_hz: enf,
        mean_hz: mean,
        std_hz: std,
        max_jump_hz: max_jump,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    /// Gera senoide de `freq` Hz, `amp` (0..1), `dur_s` segundos a `sr`.
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
            *v += 0.25; // injeta DC
        }
        let m = measure(&s, 48000, 1, 0.997);
        assert!((m.dc_offset - 0.25).abs() < 0.01, "dc={}", m.dc_offset);
    }

    #[test]
    fn measure_detects_clipping_runs() {
        // Sinal com 3 trechos saturados.
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
        // Pico do espectro deve cair perto de 1000 Hz (± resolução do bin).
        assert!(
            (sp.peak_freq_hz - 1000.0).abs() < 30.0,
            "pico em {} Hz",
            sp.peak_freq_hz
        );
    }

    #[test]
    fn enf_tracks_steady_60hz() {
        // "Rede" estável a 60 Hz → ENF média ~60, salto ~0.
        let s = sine(60.0, 0.2, 12.0, 1000); // sr baixo basta p/ 60 Hz
        let e = enf(&s, 1000, 60.0, 4.0, 2.0);
        assert!(!e.enf_hz.is_empty());
        assert!((e.mean_hz - 60.0).abs() < 0.5, "ENF média {}", e.mean_hz);
        assert!(e.max_jump_hz < 0.5, "salto inesperado {}", e.max_jump_hz);
    }

    #[test]
    fn enf_picks_50_or_60_band() {
        let e50 = enf(&sine(50.0, 0.2, 12.0, 1000), 1000, 50.0, 4.0, 2.0);
        assert!((e50.mean_hz - 50.0).abs() < 0.5, "ENF50 {}", e50.mean_hz);
        assert_eq!(e50.nominal_hz, 50.0);
    }
}
