//! ENF (Electric Network Frequency): a frequência da rede elétrica que fica
//! gravada como zumbido. Extração robusta + comparação com uma referência.
//!
//!   1. Reduz a taxa para ~1 kHz em estágios (filtro anti-aliasing a cada um).
//!   2. Rede 50/60 Hz automática: relação sinal/ruído dos harmônicos.
//!   3. Por quadro (8 s, passo 1 s): espectros dos harmônicos 1–4 trazidos à
//!      escala da fundamental e somados com pesos pela qualidade de cada um;
//!      pico com interpolação → frequência; SNR do quadro → confiança.
//!   4. Variações bruscas (comparando uma janela antes com uma depois — numa
//!      emenda a curva vira rampa por causa da sobreposição) e trechos sem ENF.
//!   5. Comparação: a curva desliza sobre a de uma gravação de referência da
//!      rede; melhor encaixe = maior correlação, com diferença média e o 2º
//!      melhor encaixe (unicidade).
//!
//! Algoritmo escrito aqui, a partir da literatura (combinação de espectros de
//! harmônicos, Hajj-Ahmad et al.); nenhum código copiado. Tudo determinístico.

use rustfft::{num_complex::Complex32, FftPlanner};
use serde::Serialize;

pub const WINDOW_S: f32 = 8.0;
pub const STEP_S: f32 = 1.0;
const MAX_HARMONIC: usize = 4;
/// Faixa em torno de cada harmônico (Hz, na escala da fundamental).
const HALF_BAND: f32 = 0.5;
/// Quadro com SNR abaixo disto é tratado como "sem ENF".
const MIN_SNR_DB: f32 = 6.0;

#[derive(Debug, Clone, Serialize)]
pub struct Harmonic {
    pub k: u32,
    pub snr_db: f32,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnfResult {
    /// Rede usada (50 ou 60 Hz).
    pub nominal_hz: f32,
    /// Escolhida automaticamente (senão, informada pelo perito).
    pub auto: bool,
    /// Força do zumbido (melhor SNR de harmônico, dB) em 50 e em 60 Hz.
    pub score_50_db: f32,
    pub score_60_db: f32,
    pub harmonics: Vec<Harmonic>,
    pub window_s: f32,
    pub step_s: f32,
    pub times_s: Vec<f32>,
    pub enf_hz: Vec<f32>,
    /// SNR de cada quadro (dB) — abaixo de 6 dB a frequência não é confiável.
    pub snr_db: Vec<f32>,
    pub mean_hz: f32,
    pub std_hz: f32,
    /// Maior variação quadro a quadro (Hz), só entre quadros confiáveis.
    pub max_jump_hz: f32,
    /// Fração dos quadros com ENF confiável (0..1).
    pub confidence: f32,
    /// Variações bruscas: (instante s, variação Hz).
    pub jumps: Vec<(f32, f32)>,
    /// Trechos sem ENF confiável (≥ 3 s): (início s, fim s).
    pub gaps: Vec<(f32, f32)>,
}

// ---------------------------------------------------------------------------
// Redução de taxa

/// Lê o áudio para o ENF já em 2 kHz mono (ffmpeg): uma referência de horas a
/// 48 kHz não precisa caber inteira na memória em float.
pub fn load_for_enf(wav: &std::path::Path) -> crate::error::Result<(Vec<f32>, u32)> {
    const RATE: u32 = 2000;
    let ffmpeg = super::detect("ffmpeg")?;
    let w = wav.to_string_lossy();
    let out = crate::tools::command(&ffmpeg)
        .args(["-v", "error", "-i", w.as_ref(), "-ac", "1", "-ar", "2000", "-f", "f32le", "-"])
        .output()
        .map_err(|e| crate::error::SicroError::Validation(format!("falha ao executar ffmpeg: {e}")))?;
    if !out.status.success() {
        return Err(crate::error::SicroError::Validation(format!(
            "ffmpeg falhou ao ler o áudio para o ENF: {}",
            String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("")
        )));
    }
    let x = out.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    Ok((x, RATE))
}

/// FIR passa-baixa (janela de Blackman), corte `fc` (Hz), `taps` ímpar.
fn lowpass_fir(fc: f64, rate: f64, taps: usize) -> Vec<f32> {
    let m = (taps - 1) as f64;
    let wc = 2.0 * std::f64::consts::PI * fc / rate;
    let mut h: Vec<f64> = (0..taps)
        .map(|i| {
            let n = i as f64 - m / 2.0;
            let sinc = if n == 0.0 { wc / std::f64::consts::PI } else { (wc * n).sin() / (std::f64::consts::PI * n) };
            let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / m).cos()
                + 0.08 * (4.0 * std::f64::consts::PI * i as f64 / m).cos();
            sinc * w
        })
        .collect();
    let sum: f64 = h.iter().sum();
    h.iter_mut().for_each(|v| *v /= sum);
    h.into_iter().map(|v| v as f32).collect()
}

/// Leva o sinal a ~1–2 kHz em estágios de até 8×; o que interessa (até o 4º
/// harmônico de 60 Hz, < 300 Hz) passa intacto. Devolve (sinal, taxa).
pub fn decimate_for_enf(samples: &[f32], sr: u32) -> (Vec<f32>, f64) {
    const PASS: f64 = 300.0;
    let mut x = samples.to_vec();
    let mut rate = sr as f64;
    while rate >= 2000.0 {
        let d = ((rate / 1000.0).floor() as usize).clamp(2, 8);
        let new_rate = rate / d as f64;
        // Nada que dobre para dentro de [0, PASS] pode passar.
        let stop = new_rate - PASS;
        let taps = ((5.5 * rate / (stop - PASS)).ceil() as usize) | 1;
        let h = lowpass_fir((PASS + stop) / 2.0, rate, taps);
        let half = taps / 2;
        let out_len = x.len() / d;
        let y: Vec<f32> = (0..out_len)
            .map(|o| {
                let c = o * d;
                let mut acc = 0.0f32;
                for (j, hv) in h.iter().enumerate() {
                    let idx = c as isize + j as isize - half as isize;
                    if idx >= 0 && (idx as usize) < x.len() {
                        acc += hv * x[idx as usize];
                    }
                }
                acc
            })
            .collect();
        x = y;
        rate = new_rate;
    }
    (x, rate)
}

// ---------------------------------------------------------------------------
// Espectros

struct Frames {
    /// Potência por bin de cada quadro (só até ~300 Hz).
    power: Vec<Vec<f32>>,
    bin_hz: f32,
    times: Vec<f32>,
}

fn frames(x: &[f32], rate: f64, window_s: f32, step_s: f32) -> Frames {
    let win_n = ((rate * window_s as f64) as usize).max(16);
    let hop = ((rate * step_s as f64) as usize).max(1);
    // Zero-padding: bin de ~0,015 Hz (a interpolação faz o resto).
    let n = (win_n * 4).next_power_of_two().max(1 << 14);
    let bin_hz = (rate / n as f64) as f32;
    let keep = ((310.0 / bin_hz) as usize).min(n / 2);
    let w: Vec<f32> = (0..win_n).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / win_n as f32).cos()).collect();
    let fft = FftPlanner::<f32>::new().plan_fft_forward(n);
    let mut buf = vec![Complex32::new(0.0, 0.0); n];
    let mut power = Vec::new();
    let mut times = Vec::new();
    let mut start = 0;
    while start + win_n <= x.len() {
        for (i, b) in buf.iter_mut().enumerate() {
            *b = if i < win_n { Complex32::new(x[start + i] * w[i], 0.0) } else { Complex32::new(0.0, 0.0) };
        }
        fft.process(&mut buf);
        power.push(buf[..keep].iter().map(|c| c.norm_sqr()).collect());
        times.push(((start + win_n / 2) as f64 / rate) as f32);
        start += hop;
    }
    Frames { power, bin_hz, times }
}

/// Magnitude do espectro em `f` (Hz) por interpolação cúbica (Catmull-Rom)
/// entre bins. Com zero-padding 4× o lóbulo principal é liso e a cúbica acha
/// o pico ENTRE bins — interpolação linear da potência prenderia a estimativa
/// no bin (degraus de ~30 mHz).
fn at(p: &[f32], f: f32, bin_hz: f32) -> f32 {
    let x = f / bin_hz;
    let i = x.floor() as usize;
    if i < 1 || i + 2 >= p.len() {
        return 0.0;
    }
    let t = x - i as f32;
    let (p0, p1, p2, p3) = (p[i - 1].sqrt(), p[i].sqrt(), p[i + 1].sqrt(), p[i + 2].sqrt());
    (0.5 * (2.0 * p1
        + (p2 - p0) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t * t * t))
        .max(0.0)
}

/// SNR (dB) do harmônico k de `f0` num espectro: pico em k·(f0 ± 0,3) contra o
/// percentil 95 das laterais k·f0 ± (1..3) Hz (contra a mediana, ruído puro já
/// daria ~8 dB — o máximo de muitos bins é sempre maior que a mediana).
fn harmonic_snr(p: &[f32], bin_hz: f32, f0: f32, k: usize) -> f32 {
    let kf = k as f32;
    let span = |a: f32, b: f32| -> Vec<f32> {
        let (i, j) = ((a / bin_hz).floor().max(0.0) as usize, ((b / bin_hz).ceil() as usize).min(p.len() - 1));
        if i >= j { Vec::new() } else { p[i..=j].to_vec() }
    };
    let peak = span(kf * (f0 - 0.3), kf * (f0 + 0.3)).into_iter().fold(0.0f32, f32::max);
    let mut side = span(kf * f0 - 3.0, kf * f0 - 1.0);
    side.extend(span(kf * f0 + 1.0, kf * f0 + 3.0));
    if side.is_empty() || peak <= 0.0 {
        return 0.0;
    }
    let m = (side.len() * 95 / 100).min(side.len() - 1);
    let noise = *side.select_nth_unstable_by(m, |a, b| a.total_cmp(b)).1;
    10.0 * (peak / noise.max(1e-30)).log10()
}

// ---------------------------------------------------------------------------
// Extração

pub fn extract(samples: &[f32], sr: u32, nominal: Option<f32>) -> EnfResult {
    let (x, rate) = decimate_for_enf(samples, sr);
    let fr = frames(&x, rate, WINDOW_S, STEP_S);
    let mut out = EnfResult {
        nominal_hz: nominal.map(|n| if n < 55.0 { 50.0 } else { 60.0 }).unwrap_or(60.0),
        auto: nominal.is_none(),
        score_50_db: 0.0,
        score_60_db: 0.0,
        harmonics: Vec::new(),
        window_s: WINDOW_S,
        step_s: STEP_S,
        times_s: Vec::new(),
        enf_hz: Vec::new(),
        snr_db: Vec::new(),
        mean_hz: 0.0,
        std_hz: 0.0,
        max_jump_hz: 0.0,
        confidence: 0.0,
        jumps: Vec::new(),
        gaps: Vec::new(),
    };
    if fr.power.is_empty() {
        out.mean_hz = out.nominal_hz;
        return out;
    }
    let nyq = (rate / 2.0) as f32;
    // Espectro médio → força de cada rede e de cada harmônico.
    let mut avg = vec![0f32; fr.power[0].len()];
    for p in &fr.power {
        for (a, v) in avg.iter_mut().zip(p) {
            *a += v;
        }
    }
    let ks = |f0: f32| (1..=MAX_HARMONIC).filter(move |k| (*k as f32) * (f0 + 1.5) < (nyq - 5.0).min(305.0));
    let score = |f0: f32| ks(f0).map(|k| harmonic_snr(&avg, fr.bin_hz, f0, k)).fold(f32::MIN, f32::max);
    out.score_50_db = score(50.0);
    out.score_60_db = score(60.0);
    if out.auto {
        out.nominal_hz = if out.score_50_db > out.score_60_db { 50.0 } else { 60.0 };
    }
    let f0 = out.nominal_hz;
    // Pesos: harmônicos com SNR ≥ 3 dB, proporcionais ao SNR linear − 1.
    let mut hs: Vec<Harmonic> = ks(f0)
        .map(|k| {
            let snr = harmonic_snr(&avg, fr.bin_hz, f0, k);
            Harmonic { k: k as u32, snr_db: snr, weight: (10f32.powf(snr / 10.0) - 1.0).max(0.0) }
        })
        .collect();
    hs.retain(|h| h.snr_db >= 3.0);
    if hs.is_empty() {
        hs.push(Harmonic { k: 1, snr_db: harmonic_snr(&avg, fr.bin_hz, f0, 1), weight: 1.0 });
    }
    let wsum: f32 = hs.iter().map(|h| h.weight).sum::<f32>().max(1e-12);
    hs.iter_mut().for_each(|h| h.weight /= wsum);

    // Grade comum na escala da fundamental: f0 ± 0,5 Hz em passos de 2 mHz.
    let grid: Vec<f32> = (0..=500).map(|i| f0 - HALF_BAND + i as f32 * 0.002).collect();
    for (fi, p) in fr.power.iter().enumerate() {
        let mut comb = vec![0f32; grid.len()];
        let mut snr_lin = 0.0f32;
        for h in &hs {
            let k = h.k as f32;
            let band: Vec<f32> = grid.iter().map(|g| at(p, k * g, fr.bin_hz)).collect();
            let peak = band.iter().copied().fold(0.0f32, f32::max).max(1e-30);
            for (c, b) in comb.iter_mut().zip(&band) {
                *c += h.weight * b / peak;
            }
            snr_lin += h.weight * 10f32.powf(harmonic_snr(p, fr.bin_hz, f0, h.k as usize) / 10.0);
        }
        let (imax, _) = comb.iter().enumerate().fold((0, f32::MIN), |acc, (i, v)| if *v > acc.1 { (i, *v) } else { acc });
        let delta = if imax > 0 && imax + 1 < comb.len() {
            let (a, b, c) = (comb[imax - 1], comb[imax], comb[imax + 1]);
            let den = a - 2.0 * b + c;
            if den.abs() > 1e-12 { (0.5 * (a - c) / den).clamp(-0.5, 0.5) } else { 0.0 }
        } else {
            0.0
        };
        out.times_s.push(fr.times[fi]);
        out.enf_hz.push(f0 - HALF_BAND + (imax as f32 + delta) * 0.002);
        out.snr_db.push(10.0 * snr_lin.max(1e-12).log10());
    }
    out.harmonics = hs;
    summarize(&mut out);
    out
}

fn summarize(e: &mut EnfResult) {
    let good: Vec<usize> = (0..e.enf_hz.len()).filter(|&i| e.snr_db[i] >= MIN_SNR_DB).collect();
    e.confidence = good.len() as f32 / e.enf_hz.len().max(1) as f32;
    let vals: Vec<f32> = if good.is_empty() { e.enf_hz.clone() } else { good.iter().map(|&i| e.enf_hz[i]).collect() };
    e.mean_hz = if vals.is_empty() { e.nominal_hz } else { vals.iter().sum::<f32>() / vals.len() as f32 };
    e.std_hz = if vals.len() > 1 {
        (vals.iter().map(|v| (v - e.mean_hz).powi(2)).sum::<f32>() / vals.len() as f32).sqrt()
    } else {
        0.0
    };
    e.max_jump_hz = good
        .windows(2)
        .filter(|w| w[1] == w[0] + 1)
        .map(|w| (e.enf_hz[w[1]] - e.enf_hz[w[0]]).abs())
        .fold(0.0, f32::max);

    // Variações bruscas: mediana de uma janela antes x uma depois (a janela de
    // análise espalha a mudança por ~8 quadros). Limiar robusto: 6 × desvio
    // típico dessas diferenças, nunca abaixo de 15 mHz.
    let w = (e.window_s / e.step_s).round() as usize;
    let n = e.enf_hz.len();
    if n > 2 * w + 2 {
        let med = |v: &mut Vec<f32>| {
            let m = v.len() / 2;
            *v.select_nth_unstable_by(m, |a, b| a.total_cmp(b)).1
        };
        let side = |a: usize, b: usize| -> Option<f32> {
            let mut v: Vec<f32> = (a..b).filter(|&i| e.snr_db[i] >= MIN_SNR_DB).map(|i| e.enf_hz[i]).collect();
            (v.len() * 2 >= b - a).then(|| med(&mut v))
        };
        let diffs: Vec<(usize, f32)> = (w..n - w).filter_map(|i| Some((i, side(i, i + w)? - side(i - w, i)?))).collect();
        if diffs.len() > 4 {
            let mut abs: Vec<f32> = diffs.iter().map(|d| d.1.abs()).collect();
            let mad = med(&mut abs) * 1.4826;
            let thr = (6.0 * mad).max(0.015);
            // Grupos de pontos acima do limiar (a mudança aparece num platô de
            // ~1 janela); a posição é o centro do grupo, a variação a maior.
            let mut ev: Vec<(usize, usize, f32)> = Vec::new();
            for &(i, d) in &diffs {
                if d.abs() < thr {
                    continue;
                }
                match ev.last_mut() {
                    Some(g) if i - g.1 <= w / 2 => {
                        g.1 = i;
                        if d.abs() > g.2.abs() {
                            g.2 = d;
                        }
                    }
                    _ => ev.push((i, i, d)),
                }
            }
            e.jumps = ev.into_iter().map(|(a, b, d)| (e.times_s[(a + b) / 2], d)).collect();
        }
    }
    // Trechos sem ENF confiável (≥ 3 quadros seguidos).
    let mut i = 0;
    while i < n {
        if e.snr_db[i] >= MIN_SNR_DB {
            i += 1;
            continue;
        }
        let a = i;
        while i < n && e.snr_db[i] < MIN_SNR_DB {
            i += 1;
        }
        if i - a >= 3 {
            e.gaps.push((e.times_s[a], e.times_s[i - 1]));
        }
    }
}

// ---------------------------------------------------------------------------
// Comparação com uma gravação de referência da rede

#[derive(Debug, Clone, Serialize)]
pub struct EnfMatch {
    /// Onde o áudio questionado começa dentro da referência (s).
    pub offset_s: f32,
    /// Correlação de Pearson no melhor encaixe (−1..1).
    pub correlation: f32,
    /// Diferença média absoluta entre as curvas no encaixe (Hz).
    pub mean_abs_diff_hz: f32,
    /// Segundo melhor encaixe, longe (> 1 janela) do melhor — unicidade: num
    /// encaixe único, a diferença do 2º é bem maior que a do 1º.
    pub second_correlation: f32,
    pub second_mean_abs_diff_hz: f32,
    pub second_offset_s: f32,
    /// Quadros usados na comparação (os dois com ENF confiável).
    pub frames_used: usize,
    /// Correlação por deslocamento (para o gráfico; até ~1500 pontos).
    pub curve_offsets_s: Vec<f32>,
    pub curve_corr: Vec<f32>,
    /// Encaixe por partes de ~30 s: numa montagem, partes caem em pontos
    /// diferentes da referência.
    pub parts: Vec<EnfPart>,
    /// Partes confiáveis (r ≥ 0,8) encaixam em pontos diferentes (> 2 s).
    pub parts_disagree: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnfPart {
    /// Trecho do áudio questionado (s).
    pub start_s: f32,
    pub end_s: f32,
    /// Onde esse trecho encaixa na referência (s) e quão bem.
    pub offset_s: f32,
    pub correlation: f32,
}

/// Melhor encaixe de q[a..b] em r, com `rel` = posição relativa ao início de q
/// (o encaixe do trecho é comparável ao do áudio inteiro). (lag, r, |dif|, n).
fn best_lag(q: &EnfResult, r: &EnfResult, a: usize, b: usize) -> Option<(usize, f32, f32, usize)> {
    let len = b - a;
    if r.enf_hz.len() < len {
        return None;
    }
    let min_used = ((len as f32 * 0.6) as usize).max(5);
    let mut best: Option<(usize, f32, f32, usize)> = None;
    for lag in 0..=r.enf_hz.len() - len {
        let pairs: Vec<(f32, f32)> = (a..b)
            .filter(|&i| q.snr_db[i] >= MIN_SNR_DB && r.snr_db[lag + i - a] >= MIN_SNR_DB)
            .map(|i| (q.enf_hz[i], r.enf_hz[lag + i - a]))
            .collect();
        if pairs.len() < min_used {
            continue;
        }
        let (c, d) = pearson(&pairs);
        // Mesma rede: a frequência ABSOLUTA tem que bater (só a forma, um
        // pedaço curto de curva lenta "encaixa" em muitos lugares).
        if best.map_or(true, |bb| d < bb.2) {
            best = Some((lag, c, d, pairs.len()));
        }
    }
    best
}

/// (correlação de Pearson, diferença média absoluta).
fn pearson(pairs: &[(f32, f32)]) -> (f32, f32) {
    let n = pairs.len() as f32;
    let (ma, mb) = (pairs.iter().map(|p| p.0).sum::<f32>() / n, pairs.iter().map(|p| p.1).sum::<f32>() / n);
    let (mut sab, mut saa, mut sbb, mut sd) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for (a, b) in pairs {
        sab += (a - ma) * (b - mb);
        saa += (a - ma).powi(2);
        sbb += (b - mb).powi(2);
        sd += (a - b).abs();
    }
    (if saa > 0.0 && sbb > 0.0 { sab / (saa * sbb).sqrt() } else { 0.0 }, sd / n)
}

/// Desliza a curva `q` sobre `r` (mesmo passo) com sobreposição total; exige
/// 60% dos quadros de `q` confiáveis nos dois.
pub fn compare(q: &EnfResult, r: &EnfResult) -> Option<EnfMatch> {
    let step = q.step_s;
    let (nq, nr) = (q.enf_hz.len(), r.enf_hz.len());
    if nq < 10 || nr < nq {
        return None;
    }
    let ok_q: Vec<bool> = q.snr_db.iter().map(|s| *s >= MIN_SNR_DB).collect();
    let ok_r: Vec<bool> = r.snr_db.iter().map(|s| *s >= MIN_SNR_DB).collect();
    let min_used = (nq as f32 * 0.6) as usize;
    let mut corr = vec![f32::NAN; nr - nq + 1];
    let mut mad = vec![f32::NAN; nr - nq + 1];
    let mut used = vec![0usize; nr - nq + 1];
    for lag in 0..=nr - nq {
        let pairs: Vec<(f32, f32)> =
            (0..nq).filter(|&i| ok_q[i] && ok_r[lag + i]).map(|i| (q.enf_hz[i], r.enf_hz[lag + i])).collect();
        if pairs.len() < min_used.max(5) {
            continue;
        }
        let (c, d) = pearson(&pairs);
        corr[lag] = c;
        mad[lag] = d;
        used[lag] = pairs.len();
    }
    // Melhor encaixe = menor diferença média absoluta (mesma rede: o valor
    // tem que bater, não só a forma); a correlação vai junto como medida.
    let best = (0..mad.len()).filter(|&l| !mad[l].is_nan()).min_by(|&a, &b| mad[a].total_cmp(&mad[b]))?;
    let excl = (q.window_s / step).ceil() as usize;
    let second = (0..mad.len())
        .filter(|&l| !mad[l].is_nan() && l.abs_diff(best) > excl)
        .min_by(|&a, &b| mad[a].total_cmp(&mad[b]));
    let stride = (corr.len() / 1500).max(1);
    let (curve_offsets_s, curve_corr): (Vec<f32>, Vec<f32>) = (0..corr.len())
        .step_by(stride)
        .map(|l| {
            // Em cada faixa do gráfico, o maior valor (o pico não some).
            let m = (l..(l + stride).min(corr.len())).filter(|&j| !corr[j].is_nan()).map(|j| corr[j]).fold(f32::NAN, f32::max);
            (l as f32 * step, m)
        })
        .unzip();
    // Por partes de ~30 quadros (a última absorve a sobra curta).
    const PART: usize = 30;
    let shift = r.times_s[0] - q.times_s[0];
    let mut parts = Vec::new();
    if nq >= 2 * PART {
        let mut a = 0;
        while a < nq {
            let b = if nq - a < PART + PART / 2 { nq } else { a + PART };
            if let Some((lag, c, _, _)) = best_lag(q, r, a, b) {
                parts.push(EnfPart {
                    start_s: q.times_s[a] - q.window_s / 2.0,
                    end_s: q.times_s[b - 1] + q.window_s / 2.0,
                    // Onde o INÍCIO do áudio estaria, se este trecho estiver no lugar.
                    offset_s: (lag as f32 - a as f32) * step + shift,
                    correlation: c,
                });
            }
            a = b;
        }
    }
    let good: Vec<&EnfPart> = parts.iter().filter(|p| p.correlation >= 0.8).collect();
    let parts_disagree = good.windows(2).any(|w| (w[0].offset_s - w[1].offset_s).abs() > 2.0);
    Some(EnfMatch {
        parts,
        parts_disagree,
        offset_s: best as f32 * step + shift,
        correlation: corr[best],
        mean_abs_diff_hz: mad[best],
        second_correlation: second.map_or(f32::NAN, |s| corr[s]),
        second_mean_abs_diff_hz: second.map_or(f32::NAN, |s| mad[s]),
        second_offset_s: second.map_or(f32::NAN, |s| s as f32 * step + (r.times_s[0] - q.times_s[0])),
        frames_used: used[best],
        curve_offsets_s,
        curve_corr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gerador determinístico (LCG) em [−1, 1].
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
        }
    }

    /// Curva de rede "real": passeio aleatório suave em torno do nominal (Hz, 1 por s).
    fn grid_curve(nominal: f32, secs: usize, seed: u64) -> Vec<f32> {
        let mut r = Rng(seed);
        let mut f = nominal;
        let mut v = 0.0f32;
        (0..secs)
            .map(|_| {
                v = 0.9 * v + 0.004 * r.next();
                f = (f + v).clamp(nominal - 0.08, nominal + 0.08);
                f
            })
            .collect()
    }

    /// Zumbido com harmônicos seguindo `curve` (interpolada), mais ruído.
    fn hum(curve: &[f32], sr: u32, harm_amp: &[f32], noise: f32, seed: u64) -> Vec<f32> {
        let mut r = Rng(seed);
        let n = (curve.len() - 1) * sr as usize;
        let mut phase = 0.0f64;
        (0..n)
            .map(|i| {
                let t = i as f32 / sr as f32;
                let j = t.floor() as usize;
                let f = curve[j] + (curve[j + 1] - curve[j]) * (t - j as f32);
                phase += 2.0 * std::f64::consts::PI * f as f64 / sr as f64;
                let s: f32 = harm_amp.iter().enumerate().map(|(k, a)| a * ((k + 1) as f64 * phase).sin() as f32).sum();
                s + noise * r.next()
            })
            .collect()
    }

    fn curve_at(curve: &[f32], t: f32) -> f32 {
        let j = (t.max(0.0).floor() as usize).min(curve.len() - 2);
        curve[j] + (curve[j + 1] - curve[j]) * (t - j as f32)
    }

    #[test]
    fn segue_a_curva_da_rede_com_precisao() {
        let sr = 8000;
        let c = grid_curve(60.0, 90, 1);
        let x = hum(&c, sr, &[0.01, 0.005, 0.01], 0.05, 2);
        let e = extract(&x, sr, None);
        assert_eq!(e.nominal_hz, 60.0);
        assert!(e.auto && e.score_60_db > e.score_50_db + 10.0, "{} {}", e.score_60_db, e.score_50_db);
        assert!(e.confidence > 0.95, "confiança {}", e.confidence);
        // A janela de 8 s suaviza: compara com a média da curva ponderada
        // pela mesma janela (Hann).
        let (mut err, mut sq, mut bias) = (0.0f32, 0.0f32, 0.0f32);
        for (t, f) in e.times_s.iter().zip(&e.enf_hz) {
            let (mut sw, mut sv) = (0.0f32, 0.0f32);
            for i in 0..=160 {
                let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / 160.0).cos();
                sw += w;
                sv += w * curve_at(&c, t - 4.0 + i as f32 * 0.05);
            }
            err = err.max((f - sv / sw).abs());
            sq += (f - sv / sw).powi(2);
            bias += f - sv / sw;
        }
        let n = e.enf_hz.len() as f32;
        let (rms, bias) = ((sq / n).sqrt(), bias / n);
        // Com o zumbido 14 dB abaixo do ruído (por amostra): poucos mHz.
        assert!(rms < 0.003 && err < 0.01 && bias.abs() < 0.001, "rms {rms} máx {err} viés {bias}");
        assert!(e.jumps.is_empty(), "{:?}", e.jumps);
    }

    #[test]
    fn precisao_entre_bins_so_com_a_fundamental() {
        // Bin da FFT ≈ 30 mHz: tons entre bins não podem "grudar" no bin.
        let sr = 2000;
        for f in [60.0f32, 60.007, 60.013, 60.021, 59.989] {
            let c = vec![f; 31];
            let x = hum(&c, sr, &[0.02], 0.01, 21);
            let e = extract(&x, sr, Some(60.0));
            let mid = e.enf_hz[e.enf_hz.len() / 2];
            assert!((mid - f).abs() < 0.0015, "tom {f}: medido {mid}");
        }
    }

    #[test]
    fn rede_de_50_hz_e_harmonico_mais_forte_que_a_fundamental() {
        let sr = 4000;
        let c = grid_curve(50.0, 40, 3);
        // Fundamental fraca, 2º e 3º fortes (comum em fonte chaveada).
        let x = hum(&c, sr, &[0.001, 0.02, 0.015], 0.05, 4);
        let e = extract(&x, sr, None);
        assert_eq!(e.nominal_hz, 50.0);
        assert!(e.harmonics.iter().any(|h| h.k >= 2 && h.weight > 0.3), "{:?}", e.harmonics);
        let mid = e.enf_hz.len() / 2;
        assert!((e.enf_hz[mid] - curve_at(&c, e.times_s[mid])).abs() < 0.02);
    }

    #[test]
    fn emenda_aparece_como_variacao_brusca() {
        let sr = 4000;
        // Dois trechos de rede de momentos diferentes: degrau de ~0,05 Hz aos 40 s.
        let mut c = vec![60.0f32; 41];
        c.extend(vec![60.05f32; 41]);
        let x = hum(&c, sr, &[0.01, 0.0, 0.0], 0.02, 5);
        let e = extract(&x, sr, Some(60.0));
        assert_eq!(e.jumps.len(), 1, "{:?}", e.jumps);
        assert!((e.jumps[0].0 - 40.0).abs() < 3.0 && (e.jumps[0].1 - 0.05).abs() < 0.015, "{:?}", e.jumps);
    }

    #[test]
    fn sem_zumbido_nao_inventa_enf() {
        let sr = 4000;
        let mut r = Rng(9);
        let x: Vec<f32> = (0..sr as usize * 30).map(|_| 0.1 * r.next()).collect();
        let e = extract(&x, sr, None);
        assert!(e.confidence < 0.2, "confiança {}", e.confidence);
        assert!(!e.gaps.is_empty());
    }

    #[test]
    fn acha_o_trecho_certo_na_referencia() {
        let sr = 2000;
        let rede = grid_curve(60.0, 400, 11);
        let referencia = hum(&rede, sr, &[0.01, 0.003], 0.02, 12);
        // Áudio questionado = 120 s a partir dos 150 s, outro ruído e ganho.
        let trecho: Vec<f32> = rede[150..271].to_vec();
        let questionado: Vec<f32> = hum(&trecho, sr, &[0.004, 0.0], 0.03, 13);
        let er = extract(&referencia, sr, Some(60.0));
        let eq = extract(&questionado, sr, Some(60.0));
        let m = compare(&eq, &er).expect("comparação");
        assert!((m.offset_s - 150.0).abs() <= 1.0, "encaixe em {} s", m.offset_s);
        assert!(m.correlation > 0.9, "r = {}", m.correlation);
        assert!(m.second_mean_abs_diff_hz > 3.0 * m.mean_abs_diff_hz, "2º {}", m.second_mean_abs_diff_hz);
        assert!(m.mean_abs_diff_hz < 0.01);
        assert!(m.parts.len() >= 4 && !m.parts_disagree, "{:?}", m.parts);
        assert!(m.parts.iter().all(|p| (p.offset_s - 150.0).abs() <= 1.0), "{:?}", m.parts);

        // Montagem: 60 s dos 50 s + 60 s dos 300 s → partes discordam.
        let mut montagem_curva: Vec<f32> = rede[50..110].to_vec();
        montagem_curva.extend_from_slice(&rede[300..361]);
        let montagem = hum(&montagem_curva, sr, &[0.004, 0.0], 0.03, 14);
        let em = extract(&montagem, sr, Some(60.0));
        let mm = compare(&em, &er).expect("comparação");
        assert!(mm.parts_disagree, "{:?}", mm.parts);
        // Entre as partes confiáveis: uma no lugar da 1ª metade (50 s) e outra
        // no da 2ª (300 s da rede, mas começa aos 60 s do áudio → 240 s).
        let boas: Vec<&EnfPart> = mm.parts.iter().filter(|p| p.correlation >= 0.8).collect();
        assert!(boas.iter().any(|p| (p.offset_s - 50.0).abs() <= 2.0), "{:?}", mm.parts);
        assert!(boas.iter().any(|p| (p.offset_s - 240.0).abs() <= 2.0), "{:?}", mm.parts);
    }

    #[test]
    fn reducao_de_taxa_preserva_o_zumbido_e_corta_o_resto() {
        let sr = 48000;
        let x: Vec<f32> = (0..sr as usize * 2)
            .map(|i| {
                let t = i as f32 / sr as f32;
                (2.0 * std::f32::consts::PI * 120.0 * t).sin() + (2.0 * std::f32::consts::PI * 5900.0 * t).sin()
            })
            .collect();
        let (y, rate) = decimate_for_enf(&x, sr);
        assert!((1000.0..2000.0).contains(&rate), "taxa {rate}");
        // 120 Hz passa com amplitude ~1; 5,9 kHz (que dobraria para baixo) some.
        let mid = &y[y.len() / 4..3 * y.len() / 4];
        let rms = (mid.iter().map(|v| v * v).sum::<f32>() / mid.len() as f32).sqrt();
        assert!((rms - 0.7071).abs() < 0.02, "rms {rms}");
    }
}
