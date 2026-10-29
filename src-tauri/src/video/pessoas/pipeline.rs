//! As três tarefas longas: rastrear o vídeo, gravar o MP4 com a sobreposição e
//! desenhar a sobreposição num quadro (PNG) já extraído.

use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::error::{Result, SicroError};
use crate::video::frame_export::detect_ffmpeg;

use super::bytetrack::{ByteTrack, Params};
use super::decode::{display_size, frame_dur, frame_times, FrameReader};
use super::render::{desenhar_pessoa, Opcoes, Tela};
use super::classes::mascara;
use super::yolox::{recortar_ao_quadro, scaled_size, Yolox};
use super::{cor_padrao, Amostra, Cadencia, Pessoa};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OpcoesRastreio {
    /// Confiança mínima para criar/seguir uma pessoa.
    pub limiar: f32,
    /// Analisar um quadro a cada `passo` (1 = todos).
    pub passo: u32,
    /// Classes COCO a detectar (padrão: só pessoa).
    #[serde(default = "classes_padrao")]
    pub classes: Vec<u16>,
}

fn classes_padrao() -> Vec<u16> {
    vec![0]
}

impl Default for OpcoesRastreio {
    fn default() -> Self {
        Self { limiar: 0.5, passo: 1, classes: classes_padrao() }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Andamento {
    pub feitos: u32,
    pub total: u32,
    pub pessoas: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Meta {
    pub largura: u32,
    pub altura: u32,
    pub frame_dur_s: f64,
    pub passo: u32,
    pub limiar: f32,
    pub quadros_total: u32,
    pub quadros_analisados: u32,
    pub tempo_s: f64,
    pub ms_por_quadro: f64,
}

pub struct Rastreio {
    pub pessoas: Vec<Pessoa>,
    pub meta: Meta,
}

fn cancelado() -> SicroError {
    SicroError::Validation("cancelado".into())
}

pub fn rastrear(
    video: &Path,
    media_hash: &str,
    lib: &Path,
    modelo: &Path,
    op: OpcoesRastreio,
    cancel: &AtomicBool,
    progresso: &mut dyn FnMut(Andamento),
) -> Result<Rastreio> {
    let passo = op.passo.max(1);
    let t0 = Instant::now();
    let times = frame_times(video)?;
    let (dw, dh) = display_size(video)?;
    let fdur = frame_dur(&times);
    let mut yolox = Yolox::load(lib, modelo)?;
    let (sw, sh) = scaled_size(dw, dh, yolox.side as u32);
    let (rx, ry) = (sw as f32 / dw as f32, sh as f32 / dh as f32);
    let mut reader = FrameReader::open(video, passo, sw, sh)?;
    let analisados_esperados = (times.len() as u32).div_ceil(passo);
    // 1 s de oclusão tolerada, em quadros analisados
    let max_lost = ((1.0 / (fdur * passo as f64)).round() as u32).clamp(4, 120);
    // um ByteTrack por classe (a associação por IoU não mistura carro com pessoa);
    // o número, depois, é único na cena inteira.
    let mask = mascara(&op.classes);
    let mut bts: Vec<(u16, ByteTrack)> = (0..super::classes::N_CLASSES as u16)
        .filter(|&c| mask[c as usize])
        .map(|c| (c, ByteTrack::new(Params { high: op.limiar, max_lost, ..Params::default() })))
        .collect();
    if bts.is_empty() {
        return Err(SicroError::Validation("escolha ao menos uma classe para detectar".into()));
    }

    let mut n = 0u32;
    let mut ativos = 0u32;
    while let Some(rgb) = reader.next()? {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelado());
        }
        if (n * passo) as usize >= times.len() {
            break;
        }
        let dets = yolox.detect(rgb, sw as usize, sh as usize, (rx + ry) / 2.0, 0.1, &mask)?;
        let dets = recortar_ao_quadro(dets, dw as f32, dh as f32);
        ativos = 0;
        for (c, bt) in &mut bts {
            let da_classe: Vec<_> = dets.iter().filter(|d| d.classe == *c).copied().collect();
            ativos += bt.update(&da_classe).len() as u32;
        }
        n += 1;
        if n % 4 == 0 {
            progresso(Andamento { feitos: n, total: analisados_esperados, pessoas: ativos });
        }
    }
    drop(reader);
    progresso(Andamento { feitos: n, total: analisados_esperados.max(n), pessoas: ativos });
    if n == 0 {
        return Err(SicroError::Validation("não foi possível decodificar quadros do vídeo".into()));
    }

    // Trilhas -> pessoas, numeradas pela ordem de entrada em cena; descarta as muito curtas.
    let min_amostras = if passo > 1 { 3 } else { 5 };
    let mut trilhas: Vec<(u16, Vec<_>)> = bts
        .into_iter()
        .flat_map(|(c, bt)| bt.history.into_values().map(move |s| (c, s)))
        .filter(|(_, s)| s.len() >= min_amostras)
        .collect();
    trilhas.sort_by_key(|(c, s)| (s[0].frame, *c));
    let (fw, fh) = (dw as f32, dh as f32);
    let pessoas = trilhas
        .into_iter()
        .enumerate()
        .map(|(i, (classe, s))| {
            let numero = i as i64 + 1;
            Pessoa {
                id: uuid::Uuid::new_v4().to_string(),
                media_hash: media_hash.to_string(),
                numero,
                nome: String::new(),
                cor: cor_padrao(numero),
                estado: "auto".into(),
                classe: classe as i32,
                origem: "auto".into(),
                amostras: s
                    .iter()
                    .filter_map(|p| {
                        let idx = ((p.frame - 1) * passo) as usize;
                        let t = *times.get(idx)?;
                        let (x0, y0) = (p.x.clamp(0.0, fw), p.y.clamp(0.0, fh));
                        let (x1, y1) = ((p.x + p.w).clamp(0.0, fw), (p.y + p.h).clamp(0.0, fh));
                        Some(Amostra { t, x: x0, y: y0, w: (x1 - x0).max(2.0), h: (y1 - y0).max(2.0) })
                    })
                    .collect(),
                ajustes: vec![],
            }
        })
        .collect();
    let tempo = t0.elapsed().as_secs_f64();
    Ok(Rastreio {
        pessoas,
        meta: Meta {
            largura: dw,
            altura: dh,
            frame_dur_s: fdur,
            passo,
            limiar: op.limiar,
            quadros_total: times.len() as u32,
            quadros_analisados: n,
            tempo_s: tempo,
            ms_por_quadro: tempo * 1000.0 / n as f64,
        },
    })
}

/// Desenha, em `rgb` (w×h), as pessoas visíveis no instante `t`.
pub fn desenhar_quadro(rgb: &mut [u8], w: i32, h: i32, t: f64, pessoas: &[Pessoa], cad: Cadencia, op: Opcoes) {
    let mut tela = Tela { buf: rgb, w, h };
    for p in pessoas {
        if let Some(c) = p.caixa_em(t, cad) {
            desenhar_pessoa(&mut tela, &c, p.numero, &p.cor, op, op.classe.then(|| p.rotulo()).as_deref());
        }
    }
}

/// Taxa de quadros a dar ao codificador: a nominal do arquivo quando bate com a
/// contagem de quadros; senão, a média real.
fn taxa(video: &Path, times: &[f64], fdur: f64) -> String {
    let media = times.len() as f64 / (times[times.len() - 1] - times[0] + fdur);
    if let Ok(ffprobe) = crate::video::probe::detect_ffprobe() {
        if let Ok(o) = crate::tools::command(&ffprobe)
            .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=r_frame_rate", "-of", "csv=p=0"])
            .arg(video)
            .output()
        {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if let Some(v) = crate::video::probe::parse_fraction(&s) {
                if (v - media).abs() / media < 0.01 && s.contains('/') {
                    return s;
                }
            }
        }
    }
    format!("{media:.6}")
}

/// MP4 com a sobreposição gravada: mesmo tamanho e mesma contagem de quadros.
pub fn exportar_video(
    video: &Path,
    saida: &Path,
    pessoas: &[Pessoa],
    cad: Cadencia,
    op: Opcoes,
    com_audio: bool,
    cancel: &AtomicBool,
    progresso: &mut dyn FnMut(Andamento),
) -> Result<u32> {
    let times = frame_times(video)?;
    let (w, h) = display_size(video)?;
    let fdur = frame_dur(&times);
    let ffmpeg = detect_ffmpeg()?;
    let mut reader = FrameReader::open(video, 1, w, h)?;
    let mut cmd = crate::tools::command(&ffmpeg);
    cmd.args(["-v", "error", "-nostdin", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24"])
        .args(["-s", &format!("{w}x{h}"), "-framerate", &taxa(video, &times, fdur), "-i", "-"]);
    if com_audio {
        cmd.arg("-i").arg(video);
    }
    cmd.args(["-map", "0:v:0"]);
    if com_audio {
        cmd.args(["-map", "1:a:0?", "-c:a", "aac", "-b:a", "160k", "-shortest"]);
    }
    cmd.args(["-c:v", "libx264", "-preset", "medium", "-crf", "17", "-pix_fmt", "yuv420p", "-movflags", "+faststart"])
        .arg(saida)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped());
    let mut enc = cmd.spawn().map_err(|e| SicroError::Workspace(format!("could not spawn ffmpeg: {e}")))?;
    let mut stdin = enc.stdin.take().ok_or_else(|| SicroError::Workspace("ffmpeg sem entrada".into()))?;
    let total = times.len() as u32;
    let mut buf = vec![0u8; (w * h * 3) as usize];
    let mut n = 0u32;
    let mut erro: Option<SicroError> = None;
    loop {
        if cancel.load(Ordering::Relaxed) {
            erro = Some(cancelado());
            break;
        }
        match reader.next() {
            Ok(Some(frame)) => {
                let Some(&t) = times.get(n as usize) else { break };
                buf.copy_from_slice(frame);
                desenhar_quadro(&mut buf, w as i32, h as i32, t, pessoas, cad, op);
                if stdin.write_all(&buf).is_err() {
                    break; // o ffmpeg fechou: o motivo vem do stderr
                }
                n += 1;
                if n % 8 == 0 {
                    progresso(Andamento { feitos: n, total, pessoas: 0 });
                }
            }
            Ok(None) => break,
            Err(e) => {
                erro = Some(e);
                break;
            }
        }
    }
    drop(stdin);
    drop(reader);
    if erro.is_some() {
        let _ = enc.kill();
    }
    let fim = enc.wait_with_output()?;
    if let Some(e) = erro {
        let _ = std::fs::remove_file(saida);
        return Err(e);
    }
    if !fim.status.success() || n == 0 {
        let _ = std::fs::remove_file(saida);
        return Err(SicroError::Workspace(format!(
            "o ffmpeg não gravou o vídeo: {}",
            String::from_utf8_lossy(&fim.stderr).trim()
        )));
    }
    progresso(Andamento { feitos: n, total, pessoas: 0 });
    Ok(n)
}

/// Desenha a sobreposição num PNG já extraído do vídeo (no instante `t`).
pub fn sobrepor_png(png: &Path, t: f64, pessoas: &[Pessoa], cad: Cadencia, op: Opcoes) -> Result<Vec<i64>> {
    let img = image::open(png)
        .map_err(|e| SicroError::Filesystem(format!("abrir quadro: {e}")))?
        .to_rgb8();
    let (w, h) = img.dimensions();
    let mut raw = img.into_raw();
    let visiveis: Vec<i64> = pessoas.iter().filter(|p| p.caixa_em(t, cad).is_some()).map(|p| p.numero).collect();
    desenhar_quadro(&mut raw, w as i32, h as i32, t, pessoas, cad, op);
    let out = image::RgbImage::from_raw(w, h, raw).ok_or_else(|| SicroError::Filesystem("quadro inválido".into()))?;
    out.save_with_format(png, image::ImageFormat::Png)
        .map_err(|e| SicroError::Filesystem(format!("gravar quadro: {e}")))?;
    Ok(visiveis)
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Teste manual com arquivos reais:
    /// PESSOAS_LIB=.../libonnxruntime.so PESSOAS_MODEL=yolox_s.onnx PESSOAS_VIDEO=v.webm
    /// [PESSOAS_SAIDA=pasta] cargo test --release pipeline -- --ignored --nocapture
    #[test]
    #[ignore]
    fn rastreio_real() {
        let (Ok(lib), Ok(modelo), Ok(video)) = (
            std::env::var("PESSOAS_LIB"),
            std::env::var("PESSOAS_MODEL"),
            std::env::var("PESSOAS_VIDEO"),
        ) else {
            return;
        };
        let cancel = AtomicBool::new(false);
        let r = rastrear(
            Path::new(&video),
            "teste",
            Path::new(&lib),
            Path::new(&modelo),
            OpcoesRastreio {
                classes: std::env::var("PESSOAS_CLASSES")
                    .map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect())
                    .unwrap_or_else(|_| vec![0]),
                ..OpcoesRastreio::default()
            },
            &cancel,
            &mut |a| eprintln!("{}/{} ({} ativas)", a.feitos, a.total, a.pessoas),
        )
        .unwrap();
        eprintln!("{:#?}", r.meta);
        for p in &r.pessoas {
            eprintln!(
                "#{} {} : {} amostras, {:.2}s -> {:.2}s",
                p.numero, p.rotulo(), p.amostras.len(), p.primeiro_s(), p.ultimo_s()
            );
        }
        if let Ok(dir) = std::env::var("PESSOAS_SAIDA") {
            std::fs::write(Path::new(&dir).join("pessoas.json"), serde_json::to_string(&r.pessoas).unwrap()).unwrap();
        }
    }

    /// Teste manual: lê PESSOAS_JSON (trilhas de `rastreio_real`) e grava em PESSOAS_SAIDA um MP4
    /// com a sobreposição e PNGs de alguns instantes.
    #[test]
    #[ignore]
    fn exportacao_real() {
        let (Ok(video), Ok(json), Ok(saida)) = (
            std::env::var("PESSOAS_VIDEO"),
            std::env::var("PESSOAS_JSON"),
            std::env::var("PESSOAS_SAIDA"),
        ) else {
            return;
        };
        let pessoas: Vec<Pessoa> = serde_json::from_str(&std::fs::read_to_string(json).unwrap()).unwrap();
        let cad = Cadencia { frame_dur: 1001.0 / 24000.0, passo: 1 };
        let op = Opcoes { classe: true, ..Opcoes::default() };
        let cancel = AtomicBool::new(false);
        let mp4 = Path::new(&saida).join("saida.mp4");
        let t0 = Instant::now();
        let n = exportar_video(Path::new(&video), &mp4, &pessoas, cad, op, true, &cancel, &mut |_| {}).unwrap();
        eprintln!("mp4: {n} quadros em {:.1}s", t0.elapsed().as_secs_f64());
        for t in [20.0f64, 35.0, 50.0] {
            let png = Path::new(&saida).join(format!("quadro_{t}.png"));
            let ff = crate::video::frame_export::detect_ffmpeg().unwrap();
            let ok = crate::tools::command(ff)
                .args(["-v", "error", "-y", "-ss", &t.to_string(), "-i", &video, "-frames:v", "1"])
                .arg(&png)
                .status()
                .unwrap();
            assert!(ok.success());
            let nums = sobrepor_png(&png, t, &pessoas, cad, op).unwrap();
            eprintln!("quadro {t}: {} pessoas", nums.len());
        }
    }

    /// Teste manual da mancha: PESSOAS_VIDEO, PESSOAS_JSON (trilhas de `rastreio_real`), PESSOAS_SAIDA.
    /// Escolhe um objeto que o detector vê por ≥ 90 quadros, marca a caixa dele num quadro, segue 60 quadros
    /// (e 60 para trás) e mede o erro contra as caixas do detector nos mesmos instantes.
    #[test]
    #[ignore]
    fn mancha_real() {
        let (Ok(video), Ok(json), Ok(saida)) = (
            std::env::var("PESSOAS_VIDEO"),
            std::env::var("PESSOAS_JSON"),
            std::env::var("PESSOAS_SAIDA"),
        ) else {
            return;
        };
        let mut pessoas: Vec<Pessoa> = serde_json::from_str(&std::fs::read_to_string(json).unwrap()).unwrap();
        let video = Path::new(&video);
        let times = frame_times(video).unwrap();
        let (dw, dh) = display_size(video).unwrap();
        let cancel = AtomicBool::new(false);
        let pular: usize = std::env::var("PESSOAS_ALVO").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        // candidatos: trilhas longas e de tamanho razoável, na ordem de numeração
        let candidatas: Vec<usize> = pessoas
            .iter()
            .enumerate()
            .filter(|(_, p)| p.amostras.len() >= 150 && p.amostras[75].w > 40.0 && p.amostras[75].h > 40.0)
            .map(|(i, _)| i)
            .collect();
        let alvo = candidatas[pular.min(candidatas.len() - 1)];
        let ref_p = pessoas[alvo].clone();
        let k0 = ref_p.amostras.len() / 2;
        let a0 = ref_p.amostras[k0];
        let idx0 = times.iter().position(|&t| (t - a0.t).abs() < 1e-6).unwrap();
        eprintln!(
            "alvo: #{} {} ({} amostras), quadro {idx0} t={:.2}s caixa {:.0},{:.0} {:.0}x{:.0}",
            ref_p.numero, ref_p.rotulo(), ref_p.amostras.len(), a0.t, a0.x, a0.y, a0.w, a0.h
        );
        let mut man = Vec::new();
        for frente in [true, false] {
            let t0 = Instant::now();
            let r = crate::video::pessoas::mancha::seguir(
                video, &times, dw, dh, idx0, (a0.x, a0.y, a0.w, a0.h), frente,
                crate::video::pessoas::mancha::OpcoesMancha { max_quadros: 60, ..Default::default() }, &cancel, &mut |_| {},
            )
            .unwrap();
            // erro contra o detector nos mesmos instantes
            let (mut soma_c, mut max_c, mut soma_iou, mut n) = (0.0f32, 0.0f32, 0.0f32, 0);
            for a in &r.amostras {
                if let Some(d) = ref_p.amostras.iter().find(|d| (d.t - a.t).abs() < 1e-6) {
                    let (cx, cy) = (a.x + a.w / 2.0 - (d.x + d.w / 2.0), a.y + a.h / 2.0 - (d.y + d.h / 2.0));
                    let e = cx.hypot(cy);
                    let iou = crate::video::pessoas::yolox::Det { x: a.x, y: a.y, w: a.w, h: a.h, score: 1.0, classe: 0 }
                        .iou(&crate::video::pessoas::yolox::Det { x: d.x, y: d.y, w: d.w, h: d.h, score: 1.0, classe: 0 });
                    if std::env::var("PESSOAS_DETALHE").is_ok() {
                        eprintln!("  t={:.2} tracker({:.0},{:.0}) detector({:.0},{:.0}) erro {:.0}", a.t, a.x, a.y, d.x, d.y, e);
                    }
                    soma_c += e;
                    max_c = max_c.max(e);
                    soma_iou += iou;
                    n += 1;
                }
            }
            eprintln!(
                "{}: {} quadros seguidos em {:.2}s, motivo {:?}, parou_em {:?}, confiança média {:.2} | vs detector: erro de centro médio {:.1}px, máx {:.1}px, IoU médio {:.2} (n={n}, altura da caixa {:.0}px)",
                if frente { "frente" } else { "trás" },
                r.amostras.len() - 1, t0.elapsed().as_secs_f64(), r.motivo, r.parou_em, r.conf_media,
                soma_c / n as f32, max_c, soma_iou / n as f32, a0.h
            );
            man.extend(r.amostras);
        }
        man.sort_by(|a, b| a.t.total_cmp(&b.t));
        man.dedup_by(|a, b| (a.t - b.t).abs() < 1e-6);
        // a mancha entra como trilha manual; o detector vira só contexto (sem o alvo)
        let numero = pessoas.iter().map(|p| p.numero).max().unwrap_or(0) + 1;
        pessoas.remove(alvo);
        pessoas.push(Pessoa {
            id: "manual".into(), media_hash: "teste".into(), numero, nome: "mancha".into(), cor: cor_padrao(numero),
            estado: "auto".into(), classe: crate::video::pessoas::classes::CLASSE_MANUAL, origem: "manual".into(), amostras: man.clone(), ajustes: vec![],
        });
        let cad = Cadencia { frame_dur: 1001.0 / 24000.0, passo: 1 };
        let op = Opcoes { classe: true, ..Opcoes::default() };
        let ff = crate::video::frame_export::detect_ffmpeg().unwrap();
        for (nome, t) in [("antes", man[0].t), ("marca", a0.t), ("depois", man[man.len() - 1].t)] {
            let png = Path::new(&saida).join(format!("mancha_{nome}.png"));
            let ok = crate::tools::command(&ff)
                .args(["-v", "error", "-y", "-ss", &t.to_string(), "-i"])
                .arg(video)
                .args(["-frames:v", "1"])
                .arg(&png)
                .status()
                .unwrap();
            assert!(ok.success());
            let nums = sobrepor_png(&png, t, &pessoas, cad, op).unwrap();
            eprintln!("quadro {nome} t={t:.2}: {nums:?}");
        }
    }
}
