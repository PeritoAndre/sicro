//! Realce para escuta (gera derivado; o WAV de análise não muda). Ordem fixa,
//! para ser reprodutível:
//!   1. FFmpeg: adeclip, adeclick, notch 50/60 Hz + harmônicos, graves < 80 Hz;
//!   2. Rust: spectral gating com perfil A–B de ruído e RNNoise (nnnoiseless, local);
//!   3. FFmpeg: afftdn, agudos > 8 kHz, banda de voz 300–3400 Hz, dynaudnorm.
//! A receita volta para ser gravada no caso.

use std::path::{Path, PathBuf};

use rustfft::{num_complex::Complex32, FftPlanner};
use serde_json::{json, Value};

use crate::error::{Result, SicroError};

/// Trecho só de ruído usado como perfil (segundos do áudio de origem).
#[derive(Debug, Clone, Copy)]
pub struct NoiseProfile {
    pub start_s: f64,
    pub end_s: f64,
}

const PRE_ORDER: &[&str] = &["declip", "declick", "notch_hum_50", "notch_hum_60", "highpass"];
const RUST_ORDER: &[&str] = &["noise_profile", "denoise_ai"];
const POST_ORDER: &[&str] = &["denoise", "lowpass", "bandpass_voice", "normalize"];

fn ffmpeg_filters(key: &str) -> &'static [&'static str] {
    match key {
        "declip" => &["adeclip"],
        "declick" => &["adeclick"],
        "notch_hum_50" => &[
            "bandreject=f=50:width_type=h:width=4",
            "bandreject=f=100:width_type=h:width=4",
            "bandreject=f=150:width_type=h:width=4",
        ],
        "notch_hum_60" => &[
            "bandreject=f=60:width_type=h:width=4",
            "bandreject=f=120:width_type=h:width=4",
            "bandreject=f=180:width_type=h:width=4",
        ],
        "highpass" => &["highpass=f=80"],
        "denoise" => &["afftdn"],
        "lowpass" => &["lowpass=f=8000"],
        "bandpass_voice" => &["highpass=f=300", "lowpass=f=3400"],
        "normalize" => &["dynaudnorm"],
        _ => &[],
    }
}

fn is_known(key: &str) -> bool {
    PRE_ORDER.contains(&key) || RUST_ORDER.contains(&key) || POST_ORDER.contains(&key)
}

/// Cadeia `-af` de uma etapa, na ordem fixa (não na ordem em que veio).
fn chain(keys: &[String], order: &[&str]) -> String {
    order
        .iter()
        .filter(|k| keys.iter().any(|x| x == *k))
        .flat_map(|k| ffmpeg_filters(k).iter().copied())
        .collect::<Vec<_>>()
        .join(",")
}

/// Aplica o realce `keys` de `src` em `out` (WAV PCM 16-bit). Devolve a receita.
pub fn run(src: &Path, out: &Path, keys: &[String], profile: Option<NoiseProfile>) -> Result<Value> {
    if let Some(k) = keys.iter().find(|k| !is_known(k)) {
        return Err(SicroError::Validation(format!("filtro de realce desconhecido: {k}")));
    }
    if keys.is_empty() {
        return Err(SicroError::Validation("selecione ao menos um filtro de realce".into()));
    }
    let wants = |k: &str| keys.iter().any(|x| x == k);
    if wants("notch_hum_50") && wants("notch_hum_60") {
        return Err(SicroError::Validation("escolha 50 Hz ou 60 Hz para o zumbido, não os dois".into()));
    }
    if wants("noise_profile") && profile.is_none() {
        return Err(SicroError::Validation(
            "marque no player o trecho A–B só com ruído (sem fala) para usar como perfil".into(),
        ));
    }

    let pre = chain(keys, PRE_ORDER);
    let post = chain(keys, POST_ORDER);
    let rust: Vec<&str> = RUST_ORDER.iter().copied().filter(|k| wants(k)).collect();

    if rust.is_empty() {
        let all = [pre.as_str(), post.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(",");
        super::run_ffmpeg(&["-y", "-i", &src.to_string_lossy(), "-af", &all, "-c:a", "pcm_s16le", &out.to_string_lossy()])?;
        return Ok(json!({ "ffmpeg": all, "rust": [] }));
    }

    // 1. FFmpeg (antes) → WAV 48 kHz em ponto flutuante (o RNNoise trabalha a 48 kHz).
    let a = temp_beside(out, "pre");
    let b = temp_beside(out, "rust");
    let result = (|| -> Result<Value> {
        let src_s = src.to_string_lossy();
        let a_s = a.to_string_lossy();
        let mut args: Vec<&str> = vec!["-y", "-i", &src_s];
        if !pre.is_empty() {
            args.extend(["-af", &pre]);
        }
        args.extend(["-ar", "48000", "-c:a", "pcm_f32le", &a_s]);
        super::run_ffmpeg(&args)?;

        // 2. Rust.
        let (mut chans, sr) = read_channels(&a)?;
        let mut steps: Vec<Value> = Vec::new();
        if let Some(p) = profile.filter(|_| wants("noise_profile")) {
            let params = GateParams::default();
            for ch in chans.iter_mut() {
                spectral_gate(ch, sr, p, params)?;
            }
            steps.push(json!({
                "etapa": "reducao_por_amostra",
                "perfil_s": [p.start_s, p.end_s],
                "fft": GATE_N, "salto": GATE_HOP,
                "sensibilidade_desvios": params.sensitivity,
                "reducao_db": params.reduction_db,
            }));
        }
        if wants("denoise_ai") {
            for ch in chans.iter_mut() {
                rnnoise(ch);
            }
            steps.push(json!({ "etapa": "rnnoise", "biblioteca": "nnnoiseless 0.5", "modelo": "padrão embutido" }));
        }
        write_wav_f32(&b, &chans, sr)?;

        // 3. FFmpeg (depois) → saída final PCM 16-bit.
        let b_s = b.to_string_lossy();
        let out_s = out.to_string_lossy();
        let mut args: Vec<&str> = vec!["-y", "-i", &b_s];
        if !post.is_empty() {
            args.extend(["-af", &post]);
        }
        args.extend(["-c:a", "pcm_s16le", &out_s]);
        super::run_ffmpeg(&args)?;
        Ok(json!({ "ffmpeg_antes": pre, "rust": steps, "ffmpeg_depois": post, "taxa_hz": 48000 }))
    })();
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
    result
}

fn temp_beside(out: &Path, tag: &str) -> PathBuf {
    let stem = out.file_stem().and_then(|s| s.to_str()).unwrap_or("realce");
    out.with_file_name(format!(".{stem}.{tag}.{}.wav", std::process::id()))
}

// ---- WAV por canal -------------------------------------------------------------

fn read_channels(path: &Path) -> Result<(Vec<Vec<f32>>, u32)> {
    let mut r = hound::WavReader::open(path)
        .map_err(|e| SicroError::Validation(format!("WAV ilegível: {e}")))?;
    let spec = r.spec();
    let nch = spec.channels.max(1) as usize;
    let inter: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().collect::<std::result::Result<_, _>>(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample.saturating_sub(1))) as f32;
            r.samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect()
        }
    }
    .map_err(|e| SicroError::Validation(format!("WAV ilegível: {e}")))?;
    let mut chans = vec![Vec::with_capacity(inter.len() / nch); nch];
    for (i, s) in inter.into_iter().enumerate() {
        chans[i % nch].push(s);
    }
    Ok((chans, spec.sample_rate))
}

fn write_wav_f32(path: &Path, chans: &[Vec<f32>], sr: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: chans.len() as u16,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec)
        .map_err(|e| SicroError::Filesystem(format!("não foi possível gravar o WAV: {e}")))?;
    let n = chans.iter().map(|c| c.len()).min().unwrap_or(0);
    for i in 0..n {
        for c in chans {
            w.write_sample(c[i])
                .map_err(|e| SicroError::Filesystem(format!("falha ao gravar o WAV: {e}")))?;
        }
    }
    w.finalize()
        .map_err(|e| SicroError::Filesystem(format!("falha ao fechar o WAV: {e}")))
}

// ---- RNNoise (nnnoiseless) -----------------------------------------------------

/// Atraso do RNNoise, em amostras: a saída de cada quadro corresponde à
/// entrada de um quadro antes. Compensado para o derivado ficar alinhado ao
/// original (marcadores e comparações batem).
const RNNOISE_DELAY: usize = nnnoiseless::DenoiseState::FRAME_SIZE;

/// Redutor de ruído de fala por rede neural, num canal a 48 kHz, in place.
fn rnnoise(samples: &mut [f32]) {
    let n = nnnoiseless::DenoiseState::FRAME_SIZE;
    let mut st = nnnoiseless::DenoiseState::new();
    let total = samples.len();
    let mut out: Vec<f32> = Vec::with_capacity(total + RNNOISE_DELAY + n);
    let mut inb = vec![0.0f32; n];
    let mut ob = vec![0.0f32; n];
    // Alimenta o sinal e mais um quadro de silêncio para "esvaziar" o atraso.
    let mut pos = 0;
    while pos < total + RNNOISE_DELAY {
        for (j, v) in inb.iter_mut().enumerate() {
            *v = samples.get(pos + j).copied().unwrap_or(0.0) * 32767.0;
        }
        st.process_frame(&mut ob, &inb);
        out.extend(ob.iter().map(|v| v / 32767.0));
        pos += n;
    }
    for (i, s) in samples.iter_mut().enumerate() {
        *s = out.get(i + RNNOISE_DELAY).copied().unwrap_or(0.0);
    }
}

// ---- redução de ruído por amostra (spectral gating) ----------------------------

const GATE_N: usize = 2048;
const GATE_HOP: usize = 512;

#[derive(Debug, Clone, Copy)]
struct GateParams {
    /// Limiar por faixa = média do ruído + `sensitivity` desvios-padrão (dB).
    sensitivity: f32,
    /// Quanto atenuar o que fica abaixo do limiar (dB).
    reduction_db: f32,
}

impl Default for GateParams {
    fn default() -> Self {
        Self { sensitivity: 1.5, reduction_db: 12.0 }
    }
}

/// Spectral gating: mede o espectro do ruído em `profile` e atenua, por quadro
/// e por faixa, o que fica abaixo do limiar. Ganho suavizado em frequência e
/// no tempo (evita "ruído musical"); mantém comprimento e alinhamento.
fn spectral_gate(samples: &mut Vec<f32>, sr: u32, profile: NoiseProfile, p: GateParams) -> Result<()> {
    let n = GATE_N;
    let hop = GATE_HOP;
    let len = samples.len();
    if len < n {
        return Ok(());
    }
    let window: Vec<f32> = (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos())
        .collect();
    // Margem de meio quadro de cada lado: todo ponto do sinal cai em 4 quadros.
    let pad = n;
    let mut x = vec![0.0f32; pad];
    x.extend_from_slice(samples);
    x.extend(std::iter::repeat(0.0).take(pad + n));
    let frames = (x.len() - n) / hop + 1;

    let mut planner = FftPlanner::<f32>::new();
    let fwd = planner.plan_fft_forward(n);
    let inv = planner.plan_fft_inverse(n);
    let bins = n / 2 + 1;

    let mut spec: Vec<Vec<Complex32>> = Vec::with_capacity(frames);
    for f in 0..frames {
        let s = f * hop;
        let mut buf: Vec<Complex32> = (0..n).map(|i| Complex32::new(x[s + i] * window[i], 0.0)).collect();
        fwd.process(&mut buf);
        spec.push(buf);
    }

    // Perfil: quadros cujo centro cai dentro do trecho de ruído.
    let to_frame_center = |f: usize| ((f * hop + n / 2) as f64 - pad as f64) / sr as f64;
    let prof: Vec<usize> = (0..frames)
        .filter(|&f| {
            let t = to_frame_center(f);
            t >= profile.start_s && t <= profile.end_s
        })
        .collect();
    if prof.len() < 4 {
        return Err(SicroError::Validation(
            "o trecho de ruído (A–B) é curto demais — use pelo menos meio segundo".into(),
        ));
    }
    let db = |c: Complex32| 20.0 * (c.norm() + 1e-9).log10();
    let mut mean = vec![0.0f32; bins];
    let mut var = vec![0.0f32; bins];
    for &f in &prof {
        for k in 0..bins {
            mean[k] += db(spec[f][k]);
        }
    }
    mean.iter_mut().for_each(|m| *m /= prof.len() as f32);
    for &f in &prof {
        for k in 0..bins {
            let d = db(spec[f][k]) - mean[k];
            var[k] += d * d;
        }
    }
    let thr: Vec<f32> = (0..bins)
        .map(|k| mean[k] + p.sensitivity * (var[k] / prof.len() as f32).sqrt())
        .collect();

    let floor = 10f32.powf(-p.reduction_db / 20.0);
    let mut prev = vec![1.0f32; bins];
    let mut y = vec![0.0f32; x.len()];
    let mut raw = vec![0.0f32; bins];
    let mut gain = vec![0.0f32; bins];
    for (f, frame) in spec.iter_mut().enumerate() {
        for k in 0..bins {
            raw[k] = if db(frame[k]) >= thr[k] { 1.0 } else { floor };
        }
        // Suaviza em frequência (±2 faixas) e no tempo (sobe na hora, desce devagar).
        for k in 0..bins {
            let lo = k.saturating_sub(2);
            let hi = (k + 2).min(bins - 1);
            let m = raw[lo..=hi].iter().sum::<f32>() / (hi - lo + 1) as f32;
            gain[k] = if m >= prev[k] { m } else { 0.6 * prev[k] + 0.4 * m };
            prev[k] = gain[k];
        }
        for k in 0..bins {
            frame[k] *= gain[k];
            if k > 0 && k < n / 2 {
                frame[n - k] *= gain[k];
            }
        }
        inv.process(frame);
        let s = f * hop;
        for i in 0..n {
            // iFFT do rustfft não normaliza (÷n); janela de síntese Hann.
            y[s + i] += frame[i].re / n as f32 * window[i];
        }
    }
    // Hann² com 75% de sobreposição soma 1,5.
    for (i, v) in samples.iter_mut().enumerate() {
        *v = y[pad + i] / 1.5;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(sr: u32, secs: f32, hz: f32, amp: f32) -> Vec<f32> {
        (0..(sr as f32 * secs) as usize)
            .map(|i| amp * (2.0 * std::f32::consts::PI * hz * i as f32 / sr as f32).sin())
            .collect()
    }
    fn noise(len: usize, amp: f32, seed: u64) -> Vec<f32> {
        let mut s = seed;
        (0..len)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((s >> 33) as f32 / (1u64 << 31) as f32 - 0.5) * 2.0 * amp
            })
            .collect()
    }
    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn ordem_fixa_da_cadeia() {
        let keys: Vec<String> = ["normalize", "highpass", "declick"].iter().map(|s| s.to_string()).collect();
        assert_eq!(chain(&keys, PRE_ORDER), "adeclick,highpass=f=80");
        assert_eq!(chain(&keys, POST_ORDER), "dynaudnorm");
    }

    #[test]
    fn reducao_por_amostra_tira_o_ruido_e_preserva_o_tom() {
        let sr = 48000;
        // 1 s só de ruído, depois 2 s de ruído + tom de 1 kHz.
        let n = noise(sr as usize * 3, 0.05, 7);
        let t = tone(sr, 2.0, 1000.0, 0.3);
        let mut x: Vec<f32> = n.clone();
        for (i, v) in t.iter().enumerate() {
            x[sr as usize + i] += v;
        }
        let before_noise = rms(&x[..sr as usize]);
        let before_total = rms(&x[sr as usize..]);
        spectral_gate(&mut x, sr, NoiseProfile { start_s: 0.05, end_s: 0.95 }, GateParams::default()).unwrap();
        let after_noise = rms(&x[sr as usize / 10..sr as usize * 9 / 10]);
        let after_total = rms(&x[sr as usize + sr as usize / 10..sr as usize * 3 - sr as usize / 10]);
        assert!(20.0 * (before_noise / after_noise).log10() > 8.0, "ruído caiu pouco");
        // O tom (que domina o trecho final) fica praticamente igual.
        assert!((20.0 * (before_total / after_total).log10()).abs() < 1.5);
        assert_eq!(x.len(), sr as usize * 3);
    }

    #[test]
    fn rnnoise_mantem_comprimento_e_alinhamento() {
        let sr = 48000usize;
        // Sinal "de voz": harmônicos de 150 Hz com envelope silábico (4 Hz).
        let mut x: Vec<f32> = (0..sr * 2)
            .map(|i| {
                let t = i as f32 / sr as f32;
                let env = (0.5 - 0.5 * (2.0 * std::f32::consts::PI * 4.0 * t).cos()).powi(2);
                let f0 = 150.0;
                let v: f32 = (1..8)
                    .map(|h| (2.0 * std::f32::consts::PI * f0 * h as f32 * t).sin() / h as f32)
                    .sum();
                0.25 * env * v
            })
            .collect();
        let orig = x.clone();
        rnnoise(&mut x);
        assert_eq!(x.len(), orig.len());
        // Atraso = deslocamento de maior correlação entre entrada e saída.
        let corr = |lag: i32| -> f32 {
            (2000..orig.len() - 2000)
                .map(|i| orig[i] * x[(i as i32 + lag) as usize])
                .sum()
        };
        let best = (-960..=960).step_by(4).max_by(|a, b| corr(*a).total_cmp(&corr(*b))).unwrap();
        assert!(best.abs() <= 48, "atraso residual de {best} amostras");
    }
}
