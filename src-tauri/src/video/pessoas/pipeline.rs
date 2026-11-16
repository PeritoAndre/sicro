//! As três tarefas longas: rastrear o vídeo, gravar o MP4 com a sobreposição e
//! desenhar a sobreposição num quadro (PNG) já extraído.

use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::edicao::ffmpeg::sonda;
use crate::edicao::quadros::TabelaQuadros;
use crate::error::{Result, SicroError};
use crate::video::frame_export::detect_ffmpeg;

use super::bytetrack::{ByteTrack, Params};
use super::decode::{display_size, frame_dur, LeitorDatado};
use super::render::{desenhar_pessoa, Opcoes, Tela};
use super::classes::{mascara, N_CLASSES};
use super::yolox::{recortar_ao_quadro, scaled_size, Det, Yolox};
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
    /// Onde o detector rodou (`cpu` | `cuda`): a saída da GPU não é bit a bit a da CPU (§6.6).
    pub provedor: String,
}

pub struct Rastreio {
    pub pessoas: Vec<Pessoa>,
    pub meta: Meta,
}

fn cancelado() -> SicroError {
    SicroError::Validation("cancelado".into())
}

/// Detector de um quadro reduzido: (rgb, largura, altura, reduzido/original, classes) → detecções em px do original.
pub(crate) type Detector<'a> = dyn FnMut(&[u8], usize, usize, f32, &[bool; N_CLASSES]) -> Result<Vec<Det>> + 'a;

pub fn rastrear(
    video: &Path,
    media_hash: &str,
    lib: &Path,
    modelo: &Path,
    op: OpcoesRastreio,
    cancel: &AtomicBool,
    progresso: &mut dyn FnMut(Andamento),
) -> Result<Rastreio> {
    let mut yolox = Yolox::load(lib, modelo)?;
    let (side, provedor) = (yolox.side as u32, yolox.provedor.fio());
    let mut r = rastrear_com(video, media_hash, op, side, cancel, progresso, &mut |rgb, sw, sh, ratio, mask| yolox.detect(rgb, sw, sh, ratio, 0.1, mask))?;
    r.meta.provedor = provedor.into();
    Ok(r)
}

/// `rastrear` com o detector dado (`side` = lado do modelo).
pub(crate) fn rastrear_com(
    video: &Path,
    media_hash: &str,
    op: OpcoesRastreio,
    side: u32,
    cancel: &AtomicBool,
    progresso: &mut dyn FnMut(Andamento),
    detectar: &mut Detector,
) -> Result<Rastreio> {
    let passo = op.passo.max(1);
    let t0 = Instant::now();
    let cap = sonda::capacidades()?;
    // os pacotes só dão a barra e a paciência do ByteTrack; o tempo de cada quadro vem do decodificador
    let pacotes = TabelaQuadros::de_pacotes(&cap, video)?;
    let fdur = Some(pacotes.passo_mediano as f64 * pacotes.tb.num as f64 / pacotes.tb.den as f64).filter(|d| *d > 0.0).unwrap_or(1.0 / 30.0);
    let (dw, dh) = display_size(video)?;
    let (sw, sh) = scaled_size(dw, dh, side);
    let (rx, ry) = (sw as f32 / dw as f32, sh as f32 / dh as f32);
    let mut reader = LeitorDatado::abrir(&cap.binario, video, passo, sw, sh)?;
    let analisados_esperados = (pacotes.pts.len() as u32).div_ceil(passo);
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
    // tempo de cada quadro analisado (`Sample.frame` − 1)
    let mut tempos: Vec<f64> = Vec::new();
    while let Some((rgb, t)) = reader.proximo()? {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelado());
        }
        tempos.push(t);
        let dets = detectar(rgb, sw as usize, sh as usize, (rx + ry) / 2.0, &mask)?;
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
    if n == 0 {
        return Err(SicroError::Validation("não foi possível decodificar quadros do vídeo".into()));
    }
    let decodificados = reader.fim()?.tempos;
    progresso(Andamento { feitos: n, total: analisados_esperados.max(n), pessoas: ativos });

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
                        let t = *tempos.get(p.frame as usize - 1)?;
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
            frame_dur_s: if decodificados.len() > 1 { frame_dur(&decodificados) } else { fdur },
            passo,
            limiar: op.limiar,
            quadros_total: decodificados.len() as u32,
            quadros_analisados: n,
            tempo_s: tempo,
            ms_por_quadro: tempo * 1000.0 / n as f64,
            provedor: String::new(),
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
    let cap = sonda::capacidades()?;
    // os pacotes dão a taxa do codificador e a barra; o tempo de cada quadro (o que se desenha) vem do decodificador
    let pacotes = TabelaQuadros::de_pacotes(&cap, video)?;
    let times: Vec<f64> = (0..pacotes.pts.len()).map(|j| pacotes.tempo(j)).collect();
    if times.is_empty() {
        return Err(SicroError::Validation("nenhum quadro de vídeo encontrado".into()));
    }
    let (w, h) = display_size(video)?;
    let fdur = frame_dur(&times);
    let ffmpeg = detect_ffmpeg()?;
    let mut reader = LeitorDatado::abrir(&cap.binario, video, 1, w, h)?;
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
    let mut acabou = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            erro = Some(cancelado());
            break;
        }
        match reader.proximo() {
            Ok(Some((frame, t))) => {
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
            Ok(None) => {
                acabou = true;
                break;
            }
            Err(e) => {
                erro = Some(e);
                break;
            }
        }
    }
    drop(stdin);
    // decodificador que morreu no meio não vira vídeo cortado (sem quadro nenhum, o erro de sempre, abaixo)
    if acabou && n > 0 {
        if let Err(e) = reader.fim() {
            erro = Some(e);
        }
    } else {
        drop(reader);
    }
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
    use crate::edicao::testes::fontes::{self, Fonte};
    use crate::video::pessoas::decode::frame_times;
    use crate::edicao::testes::medir::{self, NAO_IDENTIFICADO};

    /// Número desenhado (faixas da matriz) num quadro RGB, pelo canal verde (o vermelho da marca não pesa).
    fn numero(rgb: &[u8], w: usize, h: usize) -> u32 {
        let g: Vec<u8> = rgb.chunks_exact(3).map(|p| p[1]).collect();
        medir::identificar(&g, w, h)
    }

    /// Detector falso: uma caixa fixa por quadro e o número do quadro que ele recebeu.
    fn analisar(video: &Path, passo: u32) -> Result<(Rastreio, Vec<u32>)> {
        let mut vistos = Vec::new();
        let op = OpcoesRastreio { limiar: 0.5, passo, classes: vec![0] };
        let r = rastrear_com(video, "h", op, 320, &AtomicBool::new(false), &mut |_| {}, &mut |rgb, w, h, _, _| {
            vistos.push(numero(rgb, w, h));
            Ok(vec![Det { x: 8.0, y: 8.0, w: 40.0, h: 40.0, score: 0.95, classe: 0 }])
        })?;
        Ok((r, vistos))
    }

    /// Item do fecho (P2.5, `pipeline.rs:155`): cada quadro analisado leva o tempo do decodificador (pts do quadro que
    /// o detector recebeu, identificado pelo número desenhado), nunca o do pacote de mesmo índice. Nas fontes da
    /// matriz, nos dois ffmpeg, com passo 1 e 3. Imprime quantos quadros a regra antiga (pacote n-ésimo) datava errado.
    #[test]
    fn quadro_analisado_leva_o_tempo_do_decodificador() {
        let Some(cap) = fontes::cap() else { return };
        let (mut resumo, mut falhas) = (Vec::new(), Vec::new());
        for &f in fontes::MATRIZ {
            let video = fontes::gerar(f).expect("fonte");
            let tab = TabelaQuadros::do_decodificador(&cap, &video, &AtomicBool::new(false), &mut |_| {}).unwrap();
            let numeros = medir::quadros_de_origem(&cap, &video).unwrap();
            assert_eq!(numeros.len(), tab.pts.len(), "{f:?}");
            let pacotes = frame_times(&video).unwrap_or_default();
            for passo in [1u32, 3] {
                let (r, vistos) = match analisar(&video, passo) {
                    Ok(x) => x,
                    Err(e) => {
                        falhas.push(format!("{f:?} passo {passo}: {e}"));
                        continue;
                    }
                };
                assert_eq!(r.pessoas.len(), 1, "{f:?} passo {passo}");
                let am = &r.pessoas[0].amostras;
                assert_eq!(am.len(), vistos.len(), "{f:?} passo {passo}: uma amostra por quadro analisado");
                if vistos.len() != tab.pts.len().div_ceil(passo as usize) {
                    falhas.push(format!("{f:?} passo {passo}: {} quadros analisados, {} decodificados", vistos.len(), tab.pts.len()));
                }
                let (mut conferidos, mut errados, mut antigos_errados) = (0, 0, 0);
                for (k, (a, &num)) in am.iter().zip(&vistos).enumerate() {
                    if num == NAO_IDENTIFICADO {
                        continue;
                    }
                    let j = numeros.iter().position(|&x| x == num).unwrap_or_else(|| panic!("{f:?}: quadro {num} fora do decodificado"));
                    assert_eq!(numeros.iter().filter(|&&x| x == num).count(), 1, "{f:?}: número {num} repetido");
                    let esperado = tab.tempo(j);
                    if (a.t - esperado).abs() >= 1e-6 {
                        if errados == 0 {
                            falhas.push(format!("{f:?} passo {passo}, analisado {k} (quadro {num}): t {} ≠ {esperado}", a.t));
                        }
                        errados += 1;
                    }
                    conferidos += 1;
                    let antigo = pacotes.get(k * passo as usize).copied().unwrap_or(f64::NAN);
                    if !((antigo - esperado).abs() < 0.5 * tab.duracao(j)) {
                        antigos_errados += 1;
                    }
                }
                assert!(conferidos * 10 >= am.len() * 9, "{f:?} passo {passo}: só {conferidos} de {} identificados", am.len());
                if r.meta.quadros_total as usize != tab.pts.len() {
                    falhas.push(format!("{f:?} passo {passo}: quadros_total {} ≠ {}", r.meta.quadros_total, tab.pts.len()));
                }
                if passo == 1 {
                    resumo.push(format!(
                        "{f:?}: pacotes {}, decodificados {}, datados errado: agora {errados}, pela regra antiga {antigos_errados}, de {conferidos}",
                        pacotes.len(),
                        tab.pts.len()
                    ));
                }
            }
        }
        eprintln!("datas da análise ({}):\n{}", cap.versao, resumo.join("\n"));
        assert!(falhas.is_empty(), "{}:\n{}", cap.versao, falhas.join("\n"));
        let ts = resumo.iter().find(|l| l.starts_with("TsMeioDoGop")).unwrap();
        assert!(!ts.contains("antiga 0,"), "a fonte TsMeioDoGop deixou de exercitar o defeito: {ts}");
    }

    /// O MP4 com a sobreposição desenha a caixa no quadro do tempo dela (pts do decodificador): uma amostra no tempo
    /// do quadro 200 aparece nos quadros 200 e 201 (folga de 1,5 quadro) e em nenhum outro. Pela regra antiga
    /// (n-ésimo pacote) ela caía 30 quadros depois no TS cortado e 15 na lista de edição.
    #[test]
    fn video_exportado_desenha_no_quadro_do_tempo() {
        let Some(cap) = fontes::cap() else { return };
        for f in [Fonte::TsMeioDoGop, Fonte::Mp4ListaDeEdicao] {
            let video = fontes::gerar(f).expect("fonte");
            let tab = TabelaQuadros::do_decodificador(&cap, &video, &AtomicBool::new(false), &mut |_| {}).unwrap();
            let numeros = medir::quadros_de_origem(&cap, &video).unwrap();
            let j = numeros.iter().position(|&n| n == 200).expect("quadro 200");
            let p = Pessoa {
                id: "p".into(),
                media_hash: "h".into(),
                numero: 1,
                nome: String::new(),
                cor: "#00ff00".into(),
                estado: "auto".into(),
                classe: 0,
                origem: "auto".into(),
                amostras: vec![Amostra { t: tab.tempo(j), x: 120.0, y: 60.0, w: 60.0, h: 60.0 }],
                ajustes: vec![],
            };
            let saida = fontes::pasta_temporaria("pessoas-exportar").join("saida.mp4");
            let cad = Cadencia { frame_dur: tab.duracao(j), passo: 1 };
            let n = exportar_video(&video, &saida, &[p], cad, Opcoes::default(), false, &AtomicBool::new(false), &mut |_| {}).unwrap();
            assert_eq!(n as usize, tab.pts.len(), "{f:?}: um quadro de saída por quadro decodificado");
            let quadros = medir::quadros(&cap, &saida).unwrap();
            let nums = medir::quadros_de_origem(&cap, &saida).unwrap();
            assert_eq!((quadros.len(), nums.len()), (tab.pts.len(), tab.pts.len()), "{f:?}");
            let com_caixa: Vec<u32> = quadros
                .iter()
                .zip(&nums)
                .filter(|(q, _)| q.px.chunks_exact(3).filter(|c| c[1] as i32 - c[0] as i32 > 120 && c[1] as i32 - c[2] as i32 > 120).count() > 40)
                .map(|(_, &n)| n)
                .collect();
            assert_eq!(com_caixa, vec![200, 201], "{f:?} ({})", cap.versao);
        }
    }

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
