//! Relatório de autenticidade de áudio: estrutura (contêiner, codec, pacotes,
//! erros de decodificação) e conteúdo (corte de banda, cliques, silêncio
//! digital, salto do ruído de fundo). Só indícios com posição; a conclusão é
//! do perito.

use std::collections::BTreeMap;
use std::path::Path;

use rustfft::{num_complex::Complex32, FftPlanner};
use serde::Serialize;

use crate::error::{Result, SicroError};

#[derive(Debug, Clone, Default, Serialize)]
pub struct Structure {
    pub format: String,
    pub format_long: String,
    pub duration_s: Option<f64>,
    pub bit_rate: Option<i64>,
    pub format_tags: BTreeMap<String, String>,
    pub codec: String,
    pub codec_long: String,
    pub profile: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub channel_layout: Option<String>,
    pub bits_per_sample: Option<u32>,
    pub stream_bit_rate: Option<i64>,
    pub start_time_s: Option<f64>,
    pub stream_tags: BTreeMap<String, String>,
    /// Codec sem perdas (PCM, FLAC, ALAC…).
    pub lossless: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Packets {
    pub count: usize,
    /// Buracos na linha de tempo dos pacotes: (início s, duração s).
    pub gaps: Vec<(f64, f64)>,
    /// Pacotes que começam antes do fim do anterior.
    pub overlaps: usize,
    pub size_min: u64,
    pub size_max: u64,
    /// "constante" | "variável" (tamanho dos pacotes).
    pub size_mode: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Bandwidth {
    pub nyquist_hz: f32,
    /// Frequência em que a energia termina (None = vai até o limite do arquivo).
    pub cutoff_hz: Option<f32>,
    /// Queda logo acima do corte (dB) — corte "em degrau" é típico de codec.
    pub drop_db: f32,
    pub steep: bool,
    /// Banda ao longo do arquivo (blocos de 2 s com som, agrupados): muda no
    /// meio = trechos de fontes/codificações diferentes.
    pub segments: Vec<BandSegment>,
    pub varies: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BandSegment {
    pub start_s: f64,
    pub end_s: f64,
    /// None = banda cheia (sem corte em degrau).
    pub cutoff_hz: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// O que foi examinado: "original importado" | "vídeo de origem" | "derivado do SICRO".
    pub source_kind: String,
    pub source_file: String,
    pub structure: Option<Structure>,
    pub packets: Option<Packets>,
    pub decode_errors: usize,
    pub decode_error_samples: Vec<String>,
    pub bandwidth: Bandwidth,
    /// Cliques/impulsos isolados (s).
    pub clicks: Vec<f64>,
    pub clicks_total: usize,
    /// Silêncio digital absoluto no meio da gravação: (início s, duração s) —
    /// os 100 primeiros; o total em `digital_silences_total`.
    pub digital_silences: Vec<(f64, f64)>,
    pub digital_silences_total: usize,
    /// Saltos no ruído de fundo.
    pub noise_jumps: Vec<NoiseJump>,
    /// Observações objetivas para o perito conferir.
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Estrutura (ffprobe / ffmpeg)

fn tags(v: Option<&serde_json::Value>) -> BTreeMap<String, String> {
    v.and_then(|t| t.as_object())
        .map(|o| {
            o.iter()
                .map(|(k, v)| (k.clone(), v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// Lê contêiner e a 1ª trilha de áudio do JSON do ffprobe (-show_format -show_streams).
pub fn parse_structure(json: &str) -> Option<Structure> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let fmt = v.get("format")?;
    let st = v
        .get("streams")?
        .as_array()?
        .iter()
        .find(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("audio"))?;
    let s = |o: &serde_json::Value, k: &str| o.get(k).and_then(|x| x.as_str()).map(str::to_string);
    let num = |o: &serde_json::Value, k: &str| {
        o.get(k).and_then(|x| x.as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| x.as_f64()))
    };
    let codec = s(st, "codec_name").unwrap_or_default();
    let lossless = codec.starts_with("pcm_")
        || ["flac", "alac", "wavpack", "ape", "tta", "mlp", "truehd"].contains(&codec.as_str());
    let bits = num(st, "bits_per_raw_sample")
        .filter(|b| *b > 0.0)
        .or_else(|| num(st, "bits_per_sample").filter(|b| *b > 0.0))
        .map(|b| b as u32);
    Some(Structure {
        format: s(fmt, "format_name").unwrap_or_default(),
        format_long: s(fmt, "format_long_name").unwrap_or_default(),
        duration_s: num(fmt, "duration"),
        bit_rate: num(fmt, "bit_rate").map(|b| b as i64),
        format_tags: tags(fmt.get("tags")),
        codec,
        codec_long: s(st, "codec_long_name").unwrap_or_default(),
        profile: s(st, "profile"),
        sample_rate: num(st, "sample_rate").map(|r| r as u32),
        channels: num(st, "channels").map(|c| c as u32),
        channel_layout: s(st, "channel_layout"),
        bits_per_sample: bits,
        stream_bit_rate: num(st, "bit_rate").map(|b| b as i64),
        start_time_s: num(st, "start_time"),
        stream_tags: tags(st.get("tags")),
        lossless,
    })
}

/// Pacotes da 1ª trilha de áudio (`pts_time,duration_time,size` em CSV).
pub fn parse_packets(csv: &str) -> Packets {
    let mut p = Packets { size_min: u64::MAX, ..Default::default() };
    let mut prev_end: Option<f64> = None;
    for line in csv.lines() {
        let f: Vec<&str> = line.trim().split(',').collect();
        if f.len() < 3 {
            continue;
        }
        let (Ok(pts), Ok(dur), Ok(size)) = (f[0].parse::<f64>(), f[1].parse::<f64>(), f[2].parse::<u64>()) else {
            continue;
        };
        p.count += 1;
        p.size_min = p.size_min.min(size);
        p.size_max = p.size_max.max(size);
        if let Some(end) = prev_end {
            let d = pts - end;
            let tol = (0.5 * dur).max(0.002);
            if d > tol {
                p.gaps.push((end, d));
            } else if d < -tol {
                p.overlaps += 1;
            }
        }
        prev_end = Some(pts + dur);
    }
    if p.count == 0 {
        p.size_min = 0;
    }
    // O último pacote costuma ser menor: "variável" só com diferença real.
    p.size_mode = if p.count > 2 && p.size_max > p.size_min + p.size_max / 50 + 1 {
        "variável".into()
    } else {
        "constante".into()
    };
    p
}

pub fn probe(file: &Path) -> Result<(Option<Structure>, Option<Packets>)> {
    let ffprobe = super::detect("ffprobe")?;
    let f = file.to_string_lossy();
    let out = crate::tools::command(&ffprobe)
        .args(["-v", "quiet", "-print_format", "json", "-show_format", "-show_streams", f.as_ref()])
        .output()
        .map_err(|e| SicroError::Validation(format!("falha ao executar ffprobe: {e}")))?;
    let structure = parse_structure(&String::from_utf8_lossy(&out.stdout));
    let out = crate::tools::command(&ffprobe)
        .args([
            "-v", "error", "-select_streams", "a:0", "-show_entries", "packet=pts_time,duration_time,size",
            "-of", "csv=p=0", f.as_ref(),
        ])
        .output()
        .map_err(|e| SicroError::Validation(format!("falha ao executar ffprobe: {e}")))?;
    let packets = Some(parse_packets(&String::from_utf8_lossy(&out.stdout))).filter(|p| p.count > 0);
    Ok((structure, packets))
}

/// Decodifica a trilha inteira e conta os erros que o FFmpeg relata.
pub fn decode_errors(file: &Path) -> Result<(usize, Vec<String>)> {
    let ffmpeg = super::detect("ffmpeg")?;
    let f = file.to_string_lossy();
    let out = crate::tools::command(&ffmpeg)
        .args(["-hide_banner", "-nostats", "-v", "error", "-i", f.as_ref(), "-map", "0:a:0", "-f", "null", "-"])
        .output()
        .map_err(|e| SicroError::Validation(format!("falha ao executar ffmpeg: {e}")))?;
    let lines: Vec<String> = String::from_utf8_lossy(&out.stderr)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    Ok((lines.len(), lines.into_iter().take(5).collect()))
}

// ---------------------------------------------------------------------------
// Conteúdo (amostras mono)

fn db(p: f32) -> f32 {
    10.0 * (p + 1e-20).log10()
}

fn median(v: &mut [f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let m = v.len() / 2;
    *v.select_nth_unstable_by(m, |a, b| a.total_cmp(b)).1
}

/// Onde a energia termina num espectro médio (soma de potências de `used`
/// quadros). O "chão" é o fim da banda; o corte é a frequência mais alta ainda
/// 12 dB acima dele — e só conta se for DEGRAU (≥ 20 dB em ~700 Hz): declive
/// natural (grave mais forte que agudo) não é corte. Devolve (corte, queda).
fn cutoff_of(acc: &[f32], used: usize, nyq: f32) -> (Option<f32>, f32) {
    let bins = acc.len();
    let bin_hz = nyq / (bins - 1) as f32;
    let spec: Vec<f32> = acc.iter().map(|p| db(p / used as f32)).collect();
    // Suaviza (mediana de ±4 bins) para não tropeçar em picos.
    let smooth: Vec<f32> = (0..bins)
        .map(|k| {
            let mut w: Vec<f32> = spec[k.saturating_sub(4)..(k + 5).min(bins)].to_vec();
            median(&mut w)
        })
        .collect();
    let top = (bins as f32 * 0.97) as usize;
    let mut tail: Vec<f32> = smooth[top..bins - 1].to_vec();
    // Chão = fim da banda, mas nunca mais de 70 dB abaixo da banda de passagem
    // (sinal limpo demais — sem o ruído de 16 bits — deixaria o vazamento da
    // janela parecer energia).
    let pass_lo = (500.0 / bin_hz) as usize;
    let pass_hi = ((4000.0f32.min(0.4 * nyq)) / bin_hz) as usize;
    let mut pass: Vec<f32> = smooth[pass_lo..pass_hi.max(pass_lo + 1)].to_vec();
    let floor = median(&mut tail).max(median(&mut pass) - 70.0);
    // Do alto para baixo: primeiro ponto 12 dB acima do chão que se mantém por 8 bins.
    let Some(kc) = (9..bins - 1).rev().find(|&k| (0..8).all(|j| smooth[k - j] > floor + 12.0)) else {
        return (None, 0.0);
    };
    let fc = kc as f32 * bin_hz;
    if fc >= 0.95 * nyq {
        return (None, 0.0); // vai até o limite: sem corte
    }
    let below = smooth[kc.saturating_sub((400.0 / bin_hz) as usize).max(1)];
    let above = smooth[(kc + (300.0 / bin_hz).ceil() as usize).min(bins - 1)];
    let drop = below - above;
    // Corte de banda pressupõe banda PREENCHIDA abaixo dele (fala, ruído,
    // música). Um tom puro "termina" logo acima de si mesmo — não é codec.
    let k200 = ((200.0 / bin_hz) as usize).min(kc);
    let filled = (k200..kc).filter(|&k| smooth[k] > floor + 12.0).count() as f32 / (kc - k200).max(1) as f32;
    (Some(fc).filter(|_| drop >= 20.0 && filled >= 0.7), drop)
}

/// Banda do arquivo inteiro e ao longo do tempo (blocos de 2 s). Corte em
/// degrau bem abaixo do limite do arquivo é a marca de codificador com
/// perdas; corte que MUDA no meio indica trechos de origens diferentes.
pub fn bandwidth(samples: &[f32], sr: u32) -> Bandwidth {
    const N: usize = 4096;
    const BLOCK_S: f64 = 2.0;
    let nyq = sr as f32 / 2.0;
    let mut out = Bandwidth { nyquist_hz: nyq, ..Default::default() };
    if samples.len() < N * 2 {
        return out;
    }
    let win: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N as f32).cos()).collect();
    let fft = FftPlanner::<f32>::new().plan_fft_forward(N);
    let mut acc = vec![0f32; N / 2 + 1];
    let mut used = 0usize;
    let block_len = (BLOCK_S * sr as f64) as usize;
    let mut blocks: Vec<(Vec<f32>, usize)> = vec![(vec![0f32; N / 2 + 1], 0); samples.len() / block_len + 1];
    let mut buf = vec![Complex32::new(0.0, 0.0); N];
    let mut start = 0;
    while start + N <= samples.len() {
        let fr = &samples[start..start + N];
        let rms = (fr.iter().map(|x| x * x).sum::<f32>() / N as f32).sqrt();
        if rms > 1e-3 {
            for (i, b) in buf.iter_mut().enumerate() {
                *b = Complex32::new(fr[i] * win[i], 0.0);
            }
            fft.process(&mut buf);
            let blk = &mut blocks[(start + N / 2) / block_len];
            for (k, a) in acc.iter_mut().enumerate() {
                let p = buf[k].norm_sqr();
                *a += p;
                blk.0[k] += p;
            }
            used += 1;
            blk.1 += 1;
        }
        start += N / 2;
    }
    if used < 4 {
        return out;
    }
    let (fc, drop) = cutoff_of(&acc, used, nyq);
    out.cutoff_hz = fc;
    out.drop_db = drop;
    out.steep = fc.is_some();

    // Ao longo do tempo: blocos com som suficiente, agrupados quando o corte é
    // "o mesmo" (ambos cheios, ou cortes a menos de 8% um do outro).
    let same = |a: Option<f32>, b: Option<f32>| match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => (x - y).abs() <= 0.08 * x.max(y),
        _ => false,
    };
    let min_frames = (BLOCK_S * sr as f64 / (N / 2) as f64 * 0.4) as usize;
    for (i, (b_acc, b_used)) in blocks.iter().enumerate() {
        if *b_used < min_frames.max(4) {
            continue;
        }
        let (c, _) = cutoff_of(b_acc, *b_used, nyq);
        let (t0, t1) = (i as f64 * BLOCK_S, ((i + 1) as f64 * BLOCK_S).min(samples.len() as f64 / sr as f64));
        match out.segments.last_mut() {
            Some(seg) if same(seg.cutoff_hz, c) => seg.end_s = t1,
            _ => out.segments.push(BandSegment { start_s: t0, end_s: t1, cutoff_hz: c }),
        }
    }
    // Varia se há ao menos dois trechos diferentes de ≥ 4 s cada.
    let long: Vec<&BandSegment> = out.segments.iter().filter(|g| g.end_s - g.start_s >= 2.0 * BLOCK_S - 1e-6).collect();
    out.varies = long.windows(2).any(|w| !same(w[0].cutoff_hz, w[1].cutoff_hz))
        || (long.len() >= 2 && !same(long[0].cutoff_hz, long[long.len() - 1].cutoff_hz));
    out
}

/// Impulsos isolados: 2ª diferença do sinal muito acima da escala local
/// (mediana do desvio absoluto em blocos de 256 amostras) e curtos (≤ 3 ms).
/// Devolve os instantes (s), um por evento (eventos a < 20 ms se juntam).
pub fn clicks(samples: &[f32], sr: u32) -> Vec<f64> {
    const BLOCK: usize = 256;
    const K: f32 = 25.0;
    if samples.len() < BLOCK * 4 {
        return Vec::new();
    }
    let e: Vec<f32> = (0..samples.len())
        .map(|n| if n < 2 { 0.0 } else { (samples[n] - 2.0 * samples[n - 1] + samples[n - 2]).abs() })
        .collect();
    let nb = e.len() / BLOCK;
    let block_med: Vec<f32> = (0..nb)
        .map(|b| {
            let mut v = e[b * BLOCK..(b + 1) * BLOCK].to_vec();
            median(&mut v)
        })
        .collect();
    let merge = (0.020 * sr as f64) as usize;
    let max_len = (0.003 * sr as f64).max(2.0) as usize;
    let mut events: Vec<(usize, usize)> = Vec::new();
    for (n, &v) in e.iter().enumerate() {
        if v < 0.02 {
            continue;
        }
        let b = (n / BLOCK).min(nb - 1);
        // Escala local = maior mediana entre o bloco e os vizinhos (não dispara
        // na borda de um trecho alto).
        let scale = block_med[b.saturating_sub(2)..(b + 3).min(nb)].iter().copied().fold(0.0f32, f32::max);
        if v > K * scale.max(1e-4) {
            match events.last_mut() {
                Some((_, end)) if n - *end <= merge => *end = n,
                _ => events.push((n, n)),
            }
        }
    }
    events
        .into_iter()
        .filter(|(a, b)| b - a <= max_len)
        .map(|(a, _)| a as f64 / sr as f64)
        .collect()
}

/// Trechos de amostras EXATAMENTE zero (≥ 10 ms) no meio da gravação, com som
/// em volta — gravação contínua quase nunca tem zero digital puro.
pub fn digital_silences(samples: &[f32], sr: u32) -> Vec<(f64, f64)> {
    let min_len = (0.010 * sr as f64) as usize;
    let edge = (0.1 * sr as f64) as usize;
    let ctx = (0.2 * sr as f64) as usize;
    let rms = |s: &[f32]| if s.is_empty() { 0.0 } else { (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt() };
    let mut out = Vec::new();
    let mut n = 0;
    while n < samples.len() {
        if samples[n] != 0.0 {
            n += 1;
            continue;
        }
        let a = n;
        while n < samples.len() && samples[n] == 0.0 {
            n += 1;
        }
        let b = n;
        if b - a >= min_len && a > edge && b + edge < samples.len() {
            let before = rms(&samples[a.saturating_sub(ctx)..a]);
            let after = rms(&samples[b..(b + ctx).min(samples.len())]);
            if before > 1e-3 || after > 1e-3 {
                out.push((a as f64 / sr as f64, (b - a) as f64 / sr as f64));
            }
        }
    }
    out
}

/// Saltos no ruído de fundo, em quatro faixas (grave, médio, agudo, muito
/// agudo): o "chão" (10º percentil do nível em quadros de 50 ms) nos 3 s antes
/// contra os 3 s depois. Mudança ≥ 10 dB de uma vez (ambiente trocado, fonte
/// diferente) é indício clássico de emenda — ou de algo que ligou perto.
/// Níveis abaixo de −100 dB contam como −100 (inaudível não "salta").
pub fn noise_jumps(samples: &[f32], sr: u32) -> Vec<NoiseJump> {
    const SIDE_S: f64 = 3.0;
    const MIN_DB: f32 = 10.0;
    let nyq = sr as f32 / 2.0;
    let hop = (0.025 * sr as f64) as usize;
    let frame = (0.050 * sr as f64) as usize;
    if hop == 0 || samples.len() < frame * 4 {
        return Vec::new();
    }
    let n = frame.next_power_of_two();
    let edges = [50.0f32, 500.0, 2000.0, 6000.0, nyq];
    let bands: Vec<(usize, usize, &str)> = [(0, "grave"), (1, "médio"), (2, "agudo"), (3, "muito agudo")]
        .iter()
        .filter(|(i, _)| edges[*i] < edges[i + 1] && edges[*i] < 0.9 * nyq)
        .map(|&(i, name)| {
            let k = |f: f32| ((f / nyq) * (n / 2) as f32) as usize;
            (k(edges[i]).max(1), k(edges[i + 1]).min(n / 2), name)
        })
        .collect();
    let win: Vec<f32> = (0..frame).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / frame as f32).cos()).collect();
    let wsum: f32 = win.iter().map(|w| w * w).sum();
    let fft = FftPlanner::<f32>::new().plan_fft_forward(n);
    let mut buf = vec![Complex32::new(0.0, 0.0); n];
    let n_frames = (samples.len() - frame) / hop;
    let mut levels: Vec<Vec<f32>> = vec![Vec::with_capacity(n_frames); bands.len()];
    for f in 0..n_frames {
        let s0 = f * hop;
        for (i, b) in buf.iter_mut().enumerate() {
            *b = if i < frame { Complex32::new(samples[s0 + i] * win[i], 0.0) } else { Complex32::new(0.0, 0.0) };
        }
        fft.process(&mut buf);
        for (bi, &(lo, hi, _)) in bands.iter().enumerate() {
            let p: f32 = buf[lo..hi].iter().map(|c| c.norm_sqr()).sum::<f32>() * 2.0 / (wsum * frame as f32);
            levels[bi].push(db(p).max(-100.0));
        }
    }
    let fps = sr as f64 / hop as f64;
    let side = (SIDE_S * fps) as usize;
    let p10 = |v: &[f32]| {
        let mut w = v.to_vec();
        let k = w.len() / 10;
        *w.select_nth_unstable_by(k, |a, b| a.total_cmp(b)).1
    };
    let step = (0.25 * fps).max(1.0) as usize;
    // Candidatos: o maior salto entre as faixas em cada ponto da grade.
    let mut cand: Vec<(usize, f32, usize)> = Vec::new();
    let mut i = side;
    while i + side <= n_frames {
        let best = (0..bands.len())
            .map(|b| (p10(&levels[b][i..i + side]) - p10(&levels[b][i - side..i]), b))
            .max_by(|x, y| x.0.abs().total_cmp(&y.0.abs()));
        if let Some((d, b)) = best.filter(|(d, _)| d.abs() >= MIN_DB) {
            cand.push((i, d, b));
        }
        i += step;
    }
    // Um evento por região: o maior salto entre candidatos a < 3 s.
    let mut out: Vec<(usize, f32, usize)> = Vec::new();
    for c in cand {
        match out.last_mut() {
            Some(last) if c.0 - last.0 <= side => {
                if c.1.abs() > last.1.abs() {
                    *last = c;
                }
            }
            _ => out.push(c),
        }
    }
    out.into_iter()
        .map(|(i, d, b)| NoiseJump { t_s: i as f64 / fps, delta_db: d, band: bands[b].2.to_string() })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct NoiseJump {
    pub t_s: f64,
    /// Variação do chão (dB; + = sobe).
    pub delta_db: f32,
    /// Faixa onde o salto é maior.
    pub band: String,
}

// ---------------------------------------------------------------------------
// Observações

pub fn notes(r: &Report) -> Vec<String> {
    let mut n = Vec::new();
    if let Some(s) = &r.structure {
        let enc: Vec<&String> = s
            .format_tags
            .iter()
            .chain(s.stream_tags.iter())
            .filter(|(k, _)| k.eq_ignore_ascii_case("encoder") || k.eq_ignore_ascii_case("encoded_by"))
            .map(|(_, v)| v)
            .collect();
        if enc.iter().any(|v| v.contains("Lavf") || v.contains("Lavc")) {
            n.push(
                "O metadado de codificador indica FFmpeg (Lavf/Lavc): o arquivo foi gerado ou convertido por \
                 programa — câmeras e celulares costumam gravar a própria identificação. Conferir a cadeia de \
                 obtenção."
                    .into(),
            );
        }
        if let (true, Some(fc), true) = (s.lossless, r.bandwidth.cutoff_hz, r.bandwidth.steep) {
            n.push(format!(
                "O arquivo é sem perdas ({}), mas a energia termina em degrau em {:.1} kHz: o conteúdo já passou \
                 por compressão com perdas antes (p. ex. convertido de MP3/AAC/Opus ou gravado por app que comprime).",
                s.codec,
                fc / 1000.0
            ));
        }
    }
    if r.bandwidth.varies {
        let desc: Vec<String> = r
            .bandwidth
            .segments
            .iter()
            .map(|g| {
                let band = g.cutoff_hz.map_or("banda cheia".to_string(), |c| format!("até {:.1} kHz", c / 1000.0));
                format!("{}–{} s: {band}", g.start_s.round(), g.end_s.round())
            })
            .collect();
        n.push(format!(
            "A banda do sinal MUDA ao longo do arquivo ({}): trechos com codificação/origem diferentes — indício \
             forte de montagem; conferir as transições.",
            desc.join("; ")
        ));
    }
    if let (Some(fc), true) = (r.bandwidth.cutoff_hz, r.bandwidth.steep) {
        if (3000.0..=4200.0).contains(&fc) {
            n.push(format!(
                "Banda até {:.1} kHz: compatível com áudio de telefonia (banda estreita).",
                fc / 1000.0
            ));
        }
    }
    if let Some(p) = &r.packets {
        if !p.gaps.is_empty() {
            n.push(format!(
                "{} buraco(s) na linha de tempo dos pacotes — o tempo pula sem áudio entre eles. Pode ser perda na \
                 gravação/transmissão ou corte; conferir nos instantes listados.",
                p.gaps.len()
            ));
        }
        if p.overlaps > 0 {
            n.push(format!("{} pacote(s) começam antes do fim do anterior (tempo sobreposto).", p.overlaps));
        }
    }
    if r.decode_errors > 0 {
        n.push(format!(
            "O FFmpeg relatou {} erro(s) ao decodificar: trechos corrompidos ou emendados sem cuidado. Ouvir nos pontos \
             indicados pelos detectores.",
            r.decode_errors
        ));
    }
    if r.digital_silences_total > 0 {
        n.push(format!(
            "{} trecho(s) de silêncio digital absoluto (amostras exatamente zero) no meio de som: comum em cortes, \
             emendas, falhas de gravação ou supressor de ruído (noise gate) de app; raro em captação contínua sem \
             processamento.",
            r.digital_silences_total
        ));
    }
    if !r.noise_jumps.is_empty() {
        n.push(format!(
            "{} mudança(s) brusca(s) (≥ 10 dB) no ruído de fundo: possível troca de ambiente/emenda — ou algo que \
             ligou/desligou perto. Ouvir os pontos.",
            r.noise_jumps.len()
        ));
    }
    if r.clicks_total > 0 {
        n.push(format!(
            "{} impulso(s) isolado(s) (cliques). Podem ser emendas sem transição, mas também toques no microfone ou \
             interferência — ouvir.",
            r.clicks_total
        ));
    }
    if n.is_empty() {
        n.push("Nenhum indício encontrado pelos detectores. Isso não prova integridade: só que estes testes não acharam marca.".into());
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ruído branco determinístico (LCG), amplitude ±a.
    fn noise(n: usize, a: f32, seed: u64) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((s >> 33) as f32 / (1u64 << 31) as f32 * 2.0 - 1.0) * a
            })
            .collect()
    }

    fn lowpass_brickwall(x: &[f32], sr: u32, fc: f32) -> Vec<f32> {
        let n = x.len();
        let mut p = FftPlanner::<f32>::new();
        let mut buf: Vec<Complex32> = x.iter().map(|v| Complex32::new(*v, 0.0)).collect();
        p.plan_fft_forward(n).process(&mut buf);
        let kc = (fc / sr as f32 * n as f32) as usize;
        for (k, b) in buf.iter_mut().enumerate() {
            if k > kc && k < n - kc {
                *b = Complex32::new(0.0, 0.0);
            }
        }
        p.plan_fft_inverse(n).process(&mut buf);
        buf.iter().map(|c| c.re / n as f32).collect()
    }

    #[test]
    fn corte_de_codec_aparece_e_banda_cheia_nao() {
        let sr = 48000;
        let x = noise(1 << 17, 0.3, 7);
        assert_eq!(bandwidth(&x, sr).cutoff_hz, None);
        // Declive suave (grave forte, como fala ou ruído rosa) não é corte.
        // Tom puro não tem "corte de codec".
        let tom: Vec<f32> = (0..1 << 17)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin())
            .zip(noise(1 << 17, 0.0005, 2))
            .map(|(a, b)| a + b)
            .collect();
        assert_eq!(bandwidth(&tom, sr).cutoff_hz, None);
        let mut inclinado = x.clone();
        for i in 1..inclinado.len() {
            inclinado[i] = 0.9 * inclinado[i - 1] + 0.1 * x[i];
        }
        assert_eq!(bandwidth(&inclinado, sr).cutoff_hz, None);
        let y = lowpass_brickwall(&x, sr, 16000.0);
        let b = bandwidth(&y, sr);
        let fc = b.cutoff_hz.expect("corte esperado");
        assert!((fc - 16000.0).abs() < 300.0, "corte em {fc}");
        assert!(b.steep, "queda {}", b.drop_db);
        assert!(!b.varies);
        // 11 s: metade cortada + metade cheia — a banda muda no meio.
        let longo = noise(1 << 19, 0.3, 8);
        let cortado = lowpass_brickwall(&longo, sr, 16000.0);
        let mut z = cortado[..cortado.len() / 2].to_vec();
        z.extend_from_slice(&longo[longo.len() / 2..]);
        let bz = bandwidth(&z, sr);
        assert!(bz.varies, "{:?}", bz.segments);
    }

    #[test]
    fn clique_isolado_detectado_e_seno_limpo_nao() {
        let sr = 48000u32;
        let mut x: Vec<f32> = (0..sr as usize * 2)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin())
            .collect();
        let ruido = noise(x.len(), 0.005, 3);
        for (a, b) in x.iter_mut().zip(ruido) {
            *a += b;
        }
        assert!(clicks(&x, sr).is_empty());
        x[sr as usize] += 0.3;
        let c = clicks(&x, sr);
        assert_eq!(c.len(), 1);
        assert!((c[0] - 1.0).abs() < 0.002);
    }

    #[test]
    fn silencio_digital_no_meio_detectado() {
        let sr = 48000u32;
        let mut x = noise(sr as usize * 3, 0.1, 11);
        for v in &mut x[sr as usize..sr as usize + 2400] {
            *v = 0.0;
        }
        let s = digital_silences(&x, sr);
        assert_eq!(s.len(), 1);
        assert!((s[0].0 - 1.0).abs() < 1e-3 && (s[0].1 - 0.05).abs() < 1e-3);
        // Zero no começo (antes do som) não conta.
        let mut y = noise(sr as usize * 3, 0.1, 12);
        for v in &mut y[..2400] {
            *v = 0.0;
        }
        assert!(digital_silences(&y, sr).is_empty());
    }

    #[test]
    fn salto_no_ruido_de_fundo_detectado() {
        let sr = 16000u32;
        let mut x = noise(sr as usize * 6, 0.003, 5);
        x.extend(noise(sr as usize * 6, 0.03, 6)); // +20 dB aos 6 s
        let j = noise_jumps(&x, sr);
        assert_eq!(j.len(), 1, "{j:?}");
        assert!((j[0].t_s - 6.0).abs() < 0.5 && j[0].delta_db > 15.0, "{j:?}");
        assert!(noise_jumps(&noise(sr as usize * 12, 0.01, 9), sr).is_empty());
        // 48 kHz (quadro de 50 ms maior que 2048): ruído só no muito agudo a partir dos 6 s.
        let sr = 48000u32;
        let grave: Vec<f32> = (0..sr as usize * 12)
            .map(|i| 0.1 * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / sr as f32).sin())
            .collect();
        let chiado = lowpass_brickwall(&noise(1 << 19, 0.05, 4), sr, 23000.0);
        let mut y = grave.clone();
        for (i, v) in y.iter_mut().enumerate().skip(sr as usize * 6) {
            // agudo: diferença do ruído (passa-alta grosseiro)
            let k = i % chiado.len();
            *v += chiado[k] - chiado[k.saturating_sub(1)];
        }
        let j = noise_jumps(&y, sr);
        assert!(j.iter().any(|e| (e.t_s - 6.0).abs() < 0.5 && e.delta_db > 15.0), "{j:?}");
    }

    #[test]
    fn pacotes_com_buraco_e_sobreposicao() {
        let csv = "0.000000,0.021333,371\n0.021333,0.021333,380\n0.542667,0.021333,375\n0.550000,0.021333,377\n";
        let p = parse_packets(csv);
        assert_eq!(p.count, 4);
        assert_eq!(p.gaps.len(), 1);
        assert!((p.gaps[0].0 - 0.042666).abs() < 1e-4 && (p.gaps[0].1 - 0.5).abs() < 1e-3);
        assert_eq!(p.overlaps, 1);
        assert_eq!(p.size_mode, "variável");
    }

    #[test]
    fn estrutura_do_ffprobe() {
        let j = r#"{"streams":[{"codec_type":"video"},{"codec_type":"audio","codec_name":"pcm_s16le",
            "codec_long_name":"PCM signed 16-bit","sample_rate":"48000","channels":1,"bits_per_sample":16,
            "tags":{"encoder":"Lavc60.3.100 pcm_s16le"}}],
            "format":{"format_name":"wav","duration":"30.000000","bit_rate":"768000","tags":{"encoder":"Lavf60.3.100"}}}"#;
        let s = parse_structure(j).unwrap();
        assert!(s.lossless && s.codec == "pcm_s16le" && s.sample_rate == Some(48000));
        assert_eq!(s.bits_per_sample, Some(16));
        assert_eq!(s.format_tags.get("encoder").map(String::as_str), Some("Lavf60.3.100"));
    }
}



