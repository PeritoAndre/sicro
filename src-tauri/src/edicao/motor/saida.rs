//! Fecho do bloco e predefinições de codificação (§5.7): júri, WhatsApp, preservação, personalizada e a prévia do
//! render. Dono: P1.8.

use super::grafo::Grafo;
use super::plano::{Canvas, Plano};
use super::Alvo;
use crate::edicao::ffmpeg::args;
use crate::edicao::ffmpeg::sonda::{Capacidades, Gpu};
use crate::edicao::modelo::{Codec, Conteiner, EscolhaCodificador, Exportacao, Personalizada, Predefinicao, ProjetoEdicao, Racional};
use crate::error::{Result, SicroError};

/// Maior lado que o H.264 do NVENC aceita (§5.6).
const NVENC_LADO_MAX: u32 = 4096;
/// Etiquetas de cor da saída (as do grafo, postas também no fluxo).
const ETIQUETAS: [&str; 8] = ["-color_range", "tv", "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709"];

/// Codificação escolhida para a exportação inteira (um codificador só). `nome` = "h264_nvenc" | "libx264" | "ffv1"…;
/// `video` = argumentos do codificador de vídeo; `audio` = codec e taxa do áudio na junção; `extensao` = do bloco e da saída;
/// `gpu` = usa codificador de GPU; `timescale` = `-video_track_timescale`.
#[derive(Debug, Clone, PartialEq)]
pub struct Codificacao {
    pub nome: String,
    pub video: Vec<String>,
    pub audio: Vec<String>,
    pub extensao: String,
    pub gpu: bool,
    pub timescale: i64,
}

fn textos(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// Escala de tempo da trilha: o numerador da taxa, dobrado até passar de 10 000 (30 → 15360, 30000/1001 → 30000).
fn timescale(canvas: &Canvas) -> i64 {
    let mut t = canvas.fps.num.max(1);
    while t < 10_000 {
        t *= 2;
    }
    t
}

/// Quadros entre quadros-chave para `segundos` de vídeo.
fn gop(canvas: &Canvas, segundos: i64) -> String {
    let (n, d) = (canvas.fps.num.max(1), canvas.fps.den.max(1));
    ((segundos * n + d - 1) / d).max(1).to_string()
}

/// O codificador de GPU aprovado serve a este canvas? Só o NVENC na Fase 0 (os outros pedem `hwupload` no grafo).
fn nvenc<'a>(canvas: &Canvas, cap: &'a Capacidades) -> Option<&'a Gpu> {
    let g = cap.gpu.as_ref()?;
    (g.codificador == "h264_nvenc" && canvas.w <= NVENC_LADO_MAX && canvas.h <= NVENC_LADO_MAX).then_some(g)
}

/// Detalhe da recusa da predefinição que o motor não codifica; None = codifica. Uma regra só: `validar`
/// e `codificacao` perguntam aqui. Desde o P1.8 as quatro predefinições codificam.
pub fn recusa(exportacao: &Exportacao) -> Option<&'static str> {
    let _ = exportacao;
    None
}

fn erro(t: String) -> SicroError {
    SicroError::Validation(t)
}

/// Codificador que não é obrigatório na sonda (FFV1, FLAC, HEVC, AV1): sem ele, `filtro_ausente`.
fn exigir(cap: &Capacidades, nome: &str) -> Result<()> {
    if cap.codificadores.contains(nome) {
        return Ok(());
    }
    Err(erro(format!("filtro_ausente: {nome}")))
}

/// Limites do vídeo que toca na conversa do WhatsApp (`receitas-audio-saida.md` §6.3): cabe em 1280×720 (deitado ou
/// em pé), 64 MB; abaixo de `WHATSAPP_KBPS_480` de teto, 854×480.
const WHATSAPP_LADOS: (u32, u32) = (1280, 720);
const WHATSAPP_LADOS_480: (u32, u32) = (854, 480);
const WHATSAPP_MB: f64 = 64.0;
const WHATSAPP_KBPS_MAX: u32 = 3500;
const WHATSAPP_KBPS_480: u32 = 900;
/// Piso do teto: abaixo disso o x264 não fecha a taxa (o arquivo passa dos 64 MB; fica dito no relato do pacote).
const WHATSAPP_KBPS_MIN: u32 = 100;
const WHATSAPP_AUDIO_KBPS: u32 = 128;
/// Menor lado que a validação aceita (§2.3 `saida`).
const LADO_MIN: u32 = 64;
/// Maior taxa do WhatsApp (§5.7: "até 1280×720 e 30 qps").
const WHATSAPP_FPS_MAX: i128 = 30;

/// Teto de taxa do vídeo do WhatsApp para a duração: `min(3500, 64·8·1024·0,97/duração − 128)` kb/s.
pub(crate) fn teto_whatsapp_kbps(duracao_s: f64) -> u32 {
    if !(duracao_s > 0.0) {
        return WHATSAPP_KBPS_MAX;
    }
    let v = WHATSAPP_MB * 8.0 * 1024.0 * 0.97 / duracao_s - WHATSAPP_AUDIO_KBPS as f64;
    (v.floor().max(WHATSAPP_KBPS_MIN as f64) as u32).min(WHATSAPP_KBPS_MAX)
}

/// Reduz (nunca amplia) para caber em `maior` × `menor`, deitado ou em pé, com lados pares.
fn caber(w: u32, h: u32, (maior, menor): (u32, u32)) -> (u32, u32) {
    let (lm, ln) = (w.max(h) as f64, w.min(h) as f64);
    let f = (maior as f64 / lm).min(menor as f64 / ln).min(1.0);
    let par = |v: u32| (((v as f64 * f / 2.0).round() as u32) * 2).max(LADO_MIN);
    if f >= 1.0 { (w, h) } else { (par(w), par(h)) }
}

/// Tamanho em que a predefinição codifica um projeto de `w`×`h` e `duracao_s` (WhatsApp: até 1280×720, ou 854×480
/// quando o teto de taxa cai abaixo de 900 kb/s; personalizada: `altura_max`). O do projeto quando já cabe.
pub(crate) fn tamanho(exportacao: &Exportacao, w: u32, h: u32, duracao_s: f64) -> (u32, u32) {
    match (exportacao.predefinicao, &exportacao.personalizada) {
        (Predefinicao::Whatsapp, _) => {
            let lados = if teto_whatsapp_kbps(duracao_s) < WHATSAPP_KBPS_480 { WHATSAPP_LADOS_480 } else { WHATSAPP_LADOS };
            caber(w, h, lados)
        }
        (Predefinicao::Personalizada, Some(x)) => match x.altura_max {
            Some(m) if m > 0 && m < h => {
                let par = |v: f64| (((v / 2.0).round() as u32) * 2).max(LADO_MIN);
                (par(w as f64 * m as f64 / h as f64), par(m as f64))
            }
            _ => (w, h),
        },
        _ => (w, h),
    }
}

/// Taxa em que a predefinição codifica um projeto de taxa `fps`. WhatsApp: a do projeto dividida pelo menor inteiro
/// k que a põe em até 30 qps, k = ⌈fps / 30⌉ (60 → 30, 59,94 → 29,97, 50 → 25, 48 → 24, 120 → 30); até 30, a mesma.
/// Divisor inteiro: um slot da saída a cada k do projeto, sem cadência irregular.
pub(crate) fn taxa(exportacao: &Exportacao, fps: Racional) -> Racional {
    let (n, d) = (fps.num as i128, fps.den as i128);
    if exportacao.predefinicao != Predefinicao::Whatsapp || n <= 0 || d <= 0 || n <= WHATSAPP_FPS_MAX * d {
        return fps;
    }
    let k = (n + WHATSAPP_FPS_MAX * d - 1) / (WHATSAPP_FPS_MAX * d);
    let (mut a, mut b) = (n, d * k);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    match (i64::try_from(n / a), i64::try_from(d * k / a)) {
        (Ok(num), Ok(den)) => Racional { num, den },
        _ => fps,
    }
}

/// O projeto como a predefinição o codifica: canvas reduzido (as medidas do modelo são frações do canvas, então o
/// desenho acompanha) e, no WhatsApp, a taxa de `taxa` (a linha do tempo, o mapa de quadros, a validação e o
/// relatório saem dela). None = o próprio. `total_q` = slots do programa (a duração decide o teto do WhatsApp).
pub(crate) fn projeto_do_alvo(p: &ProjetoEdicao, alvo: &Alvo, total_q: u64) -> Option<ProjetoEdicao> {
    let Alvo::Final { exportacao } = alvo else { return None };
    let fps = p.saida.fps;
    let duracao_s = total_q as f64 * fps.den.max(1) as f64 / fps.num.max(1) as f64;
    let (w, h) = tamanho(exportacao, p.saida.largura, p.saida.altura, duracao_s);
    let fps = taxa(exportacao, p.saida.fps);
    if (w, h, fps) == (p.saida.largura, p.saida.altura, p.saida.fps) {
        return None;
    }
    let mut r = p.clone();
    (r.saida.largura, r.saida.altura, r.saida.fps) = (w, h, fps);
    Some(r)
}

/// O projeto que a exportação codifica (`projeto_do_alvo` no WhatsApp e na personalizada; o próprio no resto): quem
/// valida e quem compila a exportação olham este.
pub(crate) fn projeto_da_exportacao<'a>(p: &'a ProjetoEdicao, alvo: &Alvo, ctx: &super::Contexto) -> std::borrow::Cow<'a, ProjetoEdicao> {
    let reduz = matches!(alvo, Alvo::Final { exportacao } if matches!(exportacao.predefinicao, Predefinicao::Whatsapp | Predefinicao::Personalizada));
    match reduz.then(|| projeto_do_alvo(p, alvo, crate::edicao::tempo::resolver(p, ctx).total_q)).flatten() {
        Some(r) => std::borrow::Cow::Owned(r),
        None => std::borrow::Cow::Borrowed(p),
    }
}

/// Depois de compilar: o teto de taxa do WhatsApp entra nos blocos (`-maxrate`/`-bufsize`, pela duração do programa)
/// e a chave de cada bloco muda junto (bloco de outro teto não é reaproveitado).
pub(crate) fn acabar(plano: &mut Plano) {
    let Alvo::Final { exportacao } = &plano.alvo else { return };
    if exportacao.predefinicao != Predefinicao::Whatsapp {
        return;
    }
    let fps = plano.canvas.fps;
    let v = teto_whatsapp_kbps(plano.total_q as f64 * fps.den.max(1) as f64 / fps.num.max(1) as f64);
    let teto = [format!("-maxrate"), format!("{v}k"), "-bufsize".to_string(), format!("{}k", 2 * v)];
    for b in &mut plano.blocos {
        if b.codificador.iter().any(|a| a == "-maxrate") {
            continue;
        }
        // logo depois do `-crf <n>`
        let onde = b.codificador.iter().position(|a| a == "-crf").map_or(b.codificador.len(), |i| i + 2);
        b.codificador.splice(onde..onde, teto.iter().cloned());
        b.chave = crate::edicao::executar::sha256_bytes(format!("{} maxrate {v}", b.chave).as_bytes());
    }
}

/// Parâmetros da prévia no NVENC pelo degrau que a sonda aprovou: sem `-preset` nem `vbr` quando o degrau não os tem
/// (o "mínimo" é `constqp`).
fn previa_nvenc(g: &Gpu) -> Vec<String> {
    let tem = |x: &str| g.args.iter().any(|a| a == x);
    let mut v = textos(&["-c:v", "h264_nvenc"]);
    if tem("-preset") {
        v.extend(textos(&["-preset", "p1"]));
    }
    if tem("constqp") {
        v.extend(textos(&["-rc", "constqp", "-qp", "26"]));
    } else {
        v.extend(textos(&["-rc", "vbr", "-cq", "26", "-b:v", "0"]));
    }
    v.extend(textos(&["-bf", "0"]));
    v
}

const FFV1: [&str; 12] = ["-c:v", "ffv1", "-level", "3", "-coder", "1", "-context", "1", "-slices", "16", "-slicecrc", "1"];

fn extensao(c: Conteiner) -> &'static str {
    match c {
        Conteiner::Mp4 => "mp4",
        Conteiner::Mkv => "mkv",
        Conteiner::Mov => "mov",
    }
}

/// Vídeo, áudio, extensão e nome do codificador da predefinição personalizada (sempre em CPU).
fn personalizada(x: &Personalizada, cap: &Capacidades, g2: &str) -> Result<(&'static str, Vec<String>, Vec<String>, &'static str)> {
    let invalido = |campo: &str| erro(format!("item_invalido: personalizada.{campo}"));
    let q = x.qualidade;
    let faixa = |max: f64| if q.is_finite() && (0.0..=max).contains(&q) { Ok(format!("{q}")) } else { Err(invalido("qualidade")) };
    if !(32..=512).contains(&x.audio_kbps) {
        return Err(invalido("audio_kbps"));
    }
    let audio = vec!["-c:a".to_string(), "aac".to_string(), "-b:a".to_string(), format!("{}k", x.audio_kbps)];
    let ext = extensao(x.conteiner);
    let (nome, video, g) = match x.codec {
        Codec::H264 => ("libx264", [textos(&["-c:v", "libx264", "-preset", "medium", "-crf"]), vec![faixa(51.0)?], textos(&["-threads", "8", "-profile:v", "high"])].concat(), g2),
        Codec::Hevc => {
            exigir(cap, "libx265")?;
            ("libx265", [textos(&["-c:v", "libx265", "-preset", "medium", "-crf"]), vec![faixa(51.0)?], textos(&["-tag:v", "hvc1", "-x265-params", "log-level=error"])].concat(), g2)
        }
        Codec::Av1 => {
            exigir(cap, "libaom-av1")?;
            ("libaom-av1", [textos(&["-c:v", "libaom-av1", "-crf"]), vec![faixa(63.0)?], textos(&["-b:v", "0", "-cpu-used", "6", "-row-mt", "1"])].concat(), g2)
        }
        Codec::Ffv1 => {
            // FFV1 não cabe em MP4 nem em MOV (D10)
            if x.conteiner != Conteiner::Mkv {
                return Err(invalido("conteiner"));
            }
            exigir(cap, "ffv1")?;
            ("ffv1", textos(&FFV1), "1")
        }
    };
    let mut v = video;
    v.extend(textos(&["-g", g, "-pix_fmt", "yuv420p", "-fps_mode", "passthrough"]));
    v.extend(textos(&ETIQUETAS));
    if x.codec != Codec::Ffv1 {
        v.extend(textos(&["-filter_complex_threads", "4"]));
    }
    Ok((nome, v, audio, ext))
}

/// Codificação do alvo: predefinição + sonda (GPU aprovada no tamanho do projeto, senão `libx264`).
/// Pura: usa o que `cap.gpu` já traz; quem compila (`fila::compilar_alvo`) roda antes `sonda::gpu_serve` no tamanho
/// do projeto e tira a GPU das capacidades se ela não serve.
pub fn codificacao(alvo: &Alvo, canvas: &Canvas, cap: &Capacidades) -> Result<Codificacao> {
    let comuns = |mut v: Vec<String>, g: &str| {
        v.extend(textos(&["-g", g, "-pix_fmt", "yuv420p", "-fps_mode", "passthrough"]));
        v.extend(textos(&ETIQUETAS));
        v
    };
    let pronta = |nome: &str, video: Vec<String>, audio: Vec<String>, ext: &str| Codificacao {
        nome: nome.into(),
        gpu: nome == "h264_nvenc",
        video,
        audio,
        extensao: ext.into(),
        timescale: timescale(canvas),
    };
    match alvo {
        Alvo::Final { exportacao } => {
            let g = gop(canvas, 2);
            match exportacao.predefinicao {
                Predefinicao::Juri => {
                    let gpu = if exportacao.codificador == EscolhaCodificador::Auto { nvenc(canvas, cap) } else { None };
                    let (nome, video) = match gpu {
                        Some(gp) => {
                            let mut v = textos(&["-c:v", "h264_nvenc"]);
                            v.extend(gp.args.iter().cloned());
                            ("h264_nvenc", comuns(v, &g))
                        }
                        None => {
                            let v = textos(&["-c:v", "libx264", "-preset", "medium", "-crf", "18", "-threads", "8", "-profile:v", "high"]);
                            let mut v = comuns(v, &g);
                            v.extend(textos(&["-filter_complex_threads", "4"]));
                            ("libx264", v)
                        }
                    };
                    Ok(pronta(nome, video, textos(&["-c:a", "aac", "-b:a", "192k"]), "mp4"))
                }
                // toca na conversa: Main, sem quadros B; o teto de taxa entra em `acabar` (depende da duração)
                Predefinicao::Whatsapp => {
                    let v = textos(&["-c:v", "libx264", "-preset", "slow", "-crf", "22", "-threads", "8", "-profile:v", "main", "-bf", "0"]);
                    let mut v = comuns(v, &g);
                    v.extend(textos(&["-filter_complex_threads", "4"]));
                    Ok(pronta("libx264", v, textos(&["-c:a", "aac", "-b:a", "128k"]), "mp4"))
                }
                // sem perda do vídeo montado: todo quadro é chave, 16 fatias com CRC; bloco e saída em MKV
                Predefinicao::Preservacao => {
                    exigir(cap, "ffv1")?;
                    exigir(cap, "flac")?;
                    Ok(pronta("ffv1", comuns(textos(&FFV1), "1"), textos(&["-c:a", "flac", "-compression_level", "8"]), "mkv"))
                }
                Predefinicao::Personalizada => {
                    let x = exportacao.personalizada.as_ref().ok_or_else(|| erro("saida: exportacao".into()))?;
                    let (nome, video, audio, ext) = personalizada(x, cap, &g)?;
                    Ok(pronta(nome, video, audio, ext))
                }
            }
        }
        Alvo::Previa { .. } => {
            let g = gop(canvas, 1);
            let (nome, video) = match nvenc(canvas, cap) {
                Some(gp) => ("h264_nvenc", comuns(previa_nvenc(gp), &g)),
                None => ("libx264", comuns(textos(&["-c:v", "libx264", "-preset", "veryfast", "-crf", "24"]), &g)),
            };
            Ok(pronta(nome, video, textos(&["-c:a", "aac", "-b:a", "128k"]), "mp4"))
        }
        Alvo::Quadro { .. } => Ok(Codificacao {
            nome: "bmp".into(),
            video: textos(&["-frames:v", "1", "-update", "1", "-c:v", "bmp", "-pix_fmt", "bgr24"]),
            audio: Vec::new(),
            extensao: "bmp".into(),
            gpu: false,
            timescale: timescale(canvas),
        }),
    }
}

/// Argumentos de vídeo do bloco: os do codificador mais a reprodutibilidade e a escala de tempo (contêiner de vídeo).
pub(crate) fn argumentos_do_bloco(cod: &Codificacao) -> Vec<String> {
    let mut v = cod.video.clone();
    if cod.extensao != "bmp" {
        v.extend(args::bitexact());
        v.extend(["-video_track_timescale".to_string(), cod.timescale.to_string()]);
    }
    v
}

/// Fecha o vídeo do bloco: `format`, etiquetas de cor e `split` em `[v]` e `[h]` (o do `framehash`; só `[v]` sem ele).
pub fn fecho(g: &mut Grafo, video: &str, com_framehash: bool) {
    let mut f = vec!["format=yuv420p".to_string(), "setsar=1".to_string(), "setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv".to_string()];
    if com_framehash {
        f.push("split".to_string());
        g.cadeia(&[video], &f, &["v", "h"]);
    } else {
        g.cadeia(&[video], &f, &["v"]);
    }
}

/// Fecho do quadro final: o mesmo e a conversão explícita para RGB (BT.709, faixa limitada → cheia).
pub(crate) fn fecho_quadro(g: &mut Grafo, video: &str) {
    let f = vec![
        "format=yuv420p".to_string(),
        "setsar=1".to_string(),
        "setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv".to_string(),
        "scale=flags=bicubic+accurate_rnd+bitexact+full_chroma_int:in_color_matrix=bt709:in_range=tv:out_range=pc".to_string(),
        "format=bgr24".to_string(),
    ];
    g.cadeia(&[video], &f, &["v"]);
}

/// A predefinição perde informação na codificação? (júri e WhatsApp sim; preservação não)
pub fn com_perdas(alvo: &Alvo) -> bool {
    match alvo {
        Alvo::Final { exportacao } => match exportacao.predefinicao {
            Predefinicao::Juri | Predefinicao::Whatsapp => true,
            Predefinicao::Preservacao => false,
            Predefinicao::Personalizada => exportacao.personalizada.as_ref().is_none_or(|p| p.codec != Codec::Ffv1),
        },
        Alvo::Previa { .. } => true,
        // BMP sem compressão
        Alvo::Quadro { .. } => false,
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::ffmpeg::sonda::Gpu;
    use crate::edicao::modelo::{Exportacao, Racional};
    use crate::edicao::quadros::apoio::cap_falsa;

    fn canvas(w: u32, h: u32, fps: (i64, i64)) -> Canvas {
        Canvas { w, h, fps: Racional { num: fps.0, den: fps.1 }, cor_fundo: "#000000".into() }
    }
    fn exportacao(predefinicao: &str, codificador: &str) -> Exportacao {
        serde_json::from_value(serde_json::json!({ "predefinicao": predefinicao, "codificador": codificador, "legendas": "nenhuma", "pacote": null,
            "pdf": "a3", "revisado_por": "", "personalizada": null }))
        .unwrap()
    }
    fn com_nvenc() -> Capacidades {
        let mut c = cap_falsa();
        c.gpu = Some(Gpu {
            nome: "RTX".into(),
            driver: "610".into(),
            codificador: "h264_nvenc".into(),
            degrau: "completo".into(),
            args: textos(&["-preset", "p6", "-tune", "hq", "-rc", "vbr", "-cq", "21", "-b:v", "0", "-profile:v", "high"]),
            nvdec: true,
            scale_cuda: false,
        });
        c
    }

    #[test]
    fn juri_em_gpu_e_em_cpu_sem_nivel_fixo() {
        let alvo = Alvo::Final { exportacao: exportacao("juri", "auto") };
        let c = codificacao(&alvo, &canvas(1920, 1080, (30000, 1001)), &com_nvenc()).unwrap();
        assert_eq!((c.nome.as_str(), c.gpu, c.extensao.as_str(), c.timescale), ("h264_nvenc", true, "mp4", 30000));
        let linha = c.video.join(" ");
        assert!(linha.starts_with("-c:v h264_nvenc -preset p6 -tune hq -rc vbr -cq 21 -b:v 0 -profile:v high -g 60 -pix_fmt yuv420p"), "{linha}");
        assert_eq!(c.audio, textos(&["-c:a", "aac", "-b:a", "192k"]));
        // sem GPU aprovada, com `cpu` pedido, ou acima de 4096 de lado: libx264
        for (alvo, cap, cv) in [
            (alvo.clone(), cap_falsa(), canvas(1920, 1080, (30, 1))),
            (Alvo::Final { exportacao: exportacao("juri", "cpu") }, com_nvenc(), canvas(1920, 1080, (30, 1))),
            (alvo.clone(), com_nvenc(), canvas(7680, 4320, (30, 1))),
        ] {
            let c = codificacao(&alvo, &cv, &cap).unwrap();
            let linha = c.video.join(" ");
            assert!(linha.starts_with("-c:v libx264 -preset medium -crf 18 -threads 8 -profile:v high -g 60"), "{linha}");
            assert!(!c.gpu && c.timescale == 15360 && linha.contains("-filter_complex_threads 4"));
        }
        for c in [codificacao(&alvo, &canvas(3840, 2160, (60, 1)), &com_nvenc()).unwrap(), codificacao(&alvo, &canvas(1920, 1080, (60, 1)), &cap_falsa()).unwrap()] {
            let linha = argumentos_do_bloco(&c).join(" ");
            for proibido in ["-level", "-bf ", "aq"] {
                assert!(!linha.contains(proibido), "{proibido} em {linha}");
            }
            assert!(linha.contains("-fflags +bitexact") && linha.contains("-video_track_timescale 15360") && linha.contains("-g 120"));
        }
    }

    fn com_codificadores(mut c: Capacidades) -> Capacidades {
        c.codificadores = ["libx264", "ffv1", "flac", "libx265", "libaom-av1"].iter().map(|s| s.to_string()).collect();
        c
    }

    /// A prévia pelo NVENC segue o degrau que a sonda aprovou: o "mínimo" (`constqp`, sem preset) não ganha `-preset`
    /// nem `vbr`, que foram justamente o que a placa recusou.
    #[test]
    fn previa_segue_o_degrau_aprovado_e_quadro_final() {
        let previa = Alvo::Previa { altura: 540, de_q: 0, ate_q: 90 };
        let p = codificacao(&previa, &canvas(960, 540, (25, 1)), &com_nvenc()).unwrap();
        assert!(p.video.join(" ").starts_with("-c:v h264_nvenc -preset p1 -rc vbr -cq 26 -b:v 0 -bf 0 -g 25"), "{:?}", p.video);
        let mut basico = com_nvenc();
        basico.gpu.as_mut().unwrap().args = textos(&["-preset", "p4", "-rc", "vbr", "-cq", "21", "-b:v", "0", "-bf", "0", "-profile:v", "high"]);
        let p = codificacao(&previa, &canvas(960, 540, (25, 1)), &basico).unwrap();
        assert!(p.video.join(" ").starts_with("-c:v h264_nvenc -preset p1 -rc vbr -cq 26 -b:v 0 -bf 0 -g 25"), "{:?}", p.video);
        let mut minimo = com_nvenc();
        (minimo.gpu.as_mut().unwrap().degrau, minimo.gpu.as_mut().unwrap().args) = ("minimo".into(), textos(&["-rc", "constqp", "-qp", "21"]));
        let p = codificacao(&previa, &canvas(960, 540, (25, 1)), &minimo).unwrap();
        let linha = p.video.join(" ");
        assert!(linha.starts_with("-c:v h264_nvenc -rc constqp -qp 26 -bf 0 -g 25"), "{linha}");
        assert!(!linha.contains("-preset") && !linha.contains("vbr") && !linha.contains("-cq"), "{linha}");
        let p = codificacao(&previa, &canvas(960, 540, (30000, 1001)), &cap_falsa()).unwrap();
        assert!(p.video.join(" ").starts_with("-c:v libx264 -preset veryfast -crf 24 -g 30"));
        let q = codificacao(&Alvo::Quadro { q: 3, largura: 640 }, &canvas(640, 360, (30, 1)), &cap_falsa()).unwrap();
        assert_eq!((q.extensao.as_str(), q.gpu), ("bmp", false));
        assert!(com_perdas(&Alvo::Final { exportacao: exportacao("juri", "auto") }));
        assert!(!com_perdas(&Alvo::Final { exportacao: exportacao("preservacao", "auto") }));
    }

    /// As quatro predefinições codificam (nenhuma recusa); o que falta no binário ou está fora de faixa tem erro próprio.
    #[test]
    fn whatsapp_preservacao_e_personalizada() {
        let cap = com_codificadores(com_nvenc());
        let cv = canvas(1280, 720, (30, 1));
        for nome in ["juri", "whatsapp", "preservacao", "personalizada"] {
            assert_eq!(recusa(&exportacao(nome, "auto")), None, "{nome}");
        }
        // WhatsApp: Main, sem quadros B, CPU mesmo com GPU aprovada
        let w = codificacao(&Alvo::Final { exportacao: exportacao("whatsapp", "auto") }, &cv, &cap).unwrap();
        let linha = w.video.join(" ");
        assert!(linha.starts_with("-c:v libx264 -preset slow -crf 22 -threads 8 -profile:v main -bf 0 -g 60 -pix_fmt yuv420p"), "{linha}");
        assert_eq!((w.nome.as_str(), w.gpu, w.extensao.as_str(), w.audio.join(" ")), ("libx264", false, "mp4", "-c:a aac -b:a 128k".to_string()));
        assert!(!linha.contains("-level"));
        // Preservação: FFV1 nível 3 com todo quadro chave, FLAC, bloco e saída em MKV
        let f = codificacao(&Alvo::Final { exportacao: exportacao("preservacao", "auto") }, &cv, &cap).unwrap();
        assert!(f.video.join(" ").starts_with("-c:v ffv1 -level 3 -coder 1 -context 1 -slices 16 -slicecrc 1 -g 1 -pix_fmt yuv420p"), "{:?}", f.video);
        assert_eq!((f.nome.as_str(), f.gpu, f.extensao.as_str(), f.audio.join(" ")), ("ffv1", false, "mkv", "-c:a flac -compression_level 8".to_string()));
        let e = codificacao(&Alvo::Final { exportacao: exportacao("preservacao", "auto") }, &cv, &com_nvenc()).unwrap_err().to_string();
        assert!(e.contains("filtro_ausente: ffv1"), "{e}");
        // personalizada
        let pers = |codec: &str, conteiner: &str, q: f64, audio: u32| {
            let mut e = exportacao("personalizada", "auto");
            let mut x: Personalizada = serde_json::from_value(serde_json::json!({ "codec": codec, "conteiner": conteiner, "qualidade": 0.0, "altura_max": null, "audio_kbps": audio })).unwrap();
            x.qualidade = q;
            e.personalizada = Some(x);
            codificacao(&Alvo::Final { exportacao: e }, &cv, &cap)
        };
        let e = codificacao(&Alvo::Final { exportacao: exportacao("personalizada", "auto") }, &cv, &cap).unwrap_err().to_string();
        assert!(e.starts_with("Erro de validação: saida") || e.contains("saida: exportacao"), "{e}");
        let h = pers("hevc", "mkv", 20.0, 160).unwrap();
        assert!(h.video.join(" ").starts_with("-c:v libx265 -preset medium -crf 20 -tag:v hvc1"), "{:?}", h.video);
        assert_eq!((h.extensao.as_str(), h.audio.join(" "), h.gpu), ("mkv", "-c:a aac -b:a 160k".to_string(), false));
        let x = pers("h264", "mov", 23.5, 192).unwrap();
        assert!(x.video.join(" ").starts_with("-c:v libx264 -preset medium -crf 23.5 -threads 8 -profile:v high -g 60"), "{:?}", x.video);
        assert_eq!(x.extensao, "mov");
        assert!(pers("av1", "mp4", 30.0, 128).unwrap().video.join(" ").starts_with("-c:v libaom-av1 -crf 30 -b:v 0"));
        assert_eq!(pers("ffv1", "mkv", 0.0, 192).unwrap().nome, "ffv1");
        for (r, campo) in [(pers("ffv1", "mp4", 0.0, 192), "conteiner"), (pers("h264", "mp4", 52.0, 192), "qualidade"), (pers("h264", "mp4", f64::NAN, 192), "qualidade"), (pers("h264", "mp4", 20.0, 8), "audio_kbps")] {
            let e = r.unwrap_err().to_string();
            assert!(e.contains(&format!("item_invalido: personalizada.{campo}")), "{e}");
        }
    }

    /// WhatsApp cabe em 1280×720 deitado ou em pé, nunca amplia, e desce para 854×480 quando o teto de 64 MB deixa
    /// menos de 900 kb/s; o teto é o da receita.
    #[test]
    fn tamanho_e_teto_do_whatsapp() {
        let w = exportacao("whatsapp", "auto");
        assert_eq!(tamanho(&w, 1920, 1080, 20.0), (1280, 720));
        assert_eq!(tamanho(&w, 1080, 1920, 20.0), (720, 1280));
        assert_eq!(tamanho(&w, 3840, 2160, 30.0), (1280, 720));
        assert_eq!(tamanho(&w, 640, 360, 20.0), (640, 360));
        assert_eq!(tamanho(&w, 1440, 1080, 20.0), (960, 720));
        assert_eq!(tamanho(&w, 1920, 1080, 600.0), (854, 480));
        assert_eq!(tamanho(&exportacao("juri", "auto"), 3840, 2160, 600.0), (3840, 2160));
        let mut p = exportacao("personalizada", "auto");
        p.personalizada = Some(serde_json::from_value(serde_json::json!({ "codec": "h264", "conteiner": "mp4", "qualidade": 20.0, "altura_max": 720, "audio_kbps": 192 })).unwrap());
        assert_eq!(tamanho(&p, 1920, 1080, 20.0), (1280, 720));
        assert_eq!(tamanho(&p, 640, 360, 20.0), (640, 360));
        // min(3500, 64·8·1024·0,97/duração − 128)
        assert_eq!((teto_whatsapp_kbps(20.0), teto_whatsapp_kbps(600.0), teto_whatsapp_kbps(141.0)), (3500, 719, 3478));
        assert_eq!((teto_whatsapp_kbps(7200.0), teto_whatsapp_kbps(0.0)), (WHATSAPP_KBPS_MIN, WHATSAPP_KBPS_MAX));
        // 64 MB com a taxa do teto mais o som
        let total_kb = (teto_whatsapp_kbps(600.0) + WHATSAPP_AUDIO_KBPS) as f64 * 600.0;
        assert!(total_kb <= WHATSAPP_MB * 8.0 * 1024.0, "{total_kb}");
    }

    // ------------------------------------------------------------------ com o motor

    use crate::edicao::executar::Preparado;
    use crate::edicao::fila::compilar_alvo_com;
    use crate::edicao::motor::FonteInfo;
    use crate::edicao::quadros::apoio::{self, SemRastreios};
    use crate::edicao::testes::aceite::{midia, projeto, trecho};
    use crate::edicao::testes::fontes::{self, Fonte};
    use crate::edicao::testes::{executor, medir};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn preparado(cap: Capacidades, fontes: HashMap<String, FonteInfo>) -> Preparado {
        Preparado { fontes, cap: Arc::new(cap), rastreios: Box::new(SemRastreios), pasta_projeto: PathBuf::from("/nao/existe/projeto"), midias: Vec::new() }
    }

    fn alvo(predefinicao: &str, codificador: &str) -> Alvo {
        Alvo::Final { exportacao: exportacao(predefinicao, codificador) }
    }

    fn codificador_do(plano: &Plano) -> String {
        plano.blocos[0].codificador.iter().skip_while(|a| *a != "-c:v").nth(1).cloned().unwrap_or_default()
    }

    /// A codificação de prova no tamanho do alvo decide: GPU aprovada que não serve ao tamanho compila em CPU sem
    /// passar pela queda; a que serve fica; `cpu` e alvo sem GPU nem perguntam.
    #[test]
    fn prova_da_gpu_no_tamanho_do_alvo_decide_o_codificador() {
        let tab = apoio::tabela((1, 30), (0..60).collect());
        let fonte = apoio::fonte(tab.clone(), (320, 180));
        let p = projeto(640, 360, (30, 1), vec![midia("m1", "video", serde_json::json!([]))], vec![trecho("c1", "m1", &tab, 0, 30, 1.0, serde_json::json!([]))], serde_json::json!({}));
        let prep = preparado(com_nvenc(), HashMap::from([("m1".to_string(), fonte)]));
        let pedidos = std::cell::RefCell::new(Vec::new());
        let serve = |sim: bool| {
            let pedidos = &pedidos;
            move |_: &Capacidades, w: u32, h: u32, fps: Racional| {
                pedidos.borrow_mut().push((w, h, fps.num));
                sim
            }
        };
        let plano = compilar_alvo_com(&p, &prep, &alvo("juri", "auto"), false, &serve(true)).unwrap();
        assert_eq!(codificador_do(&plano), "h264_nvenc");
        let plano = compilar_alvo_com(&p, &prep, &alvo("juri", "auto"), false, &serve(false)).unwrap();
        assert_eq!(codificador_do(&plano), "libx264");
        assert!(matches!(&plano.alvo, Alvo::Final { exportacao } if exportacao.codificador == EscolhaCodificador::Auto), "a escolha do perito não muda");
        // a prévia pergunta no tamanho dela
        let previa = Alvo::Previa { altura: 180, de_q: 0, ate_q: 10 };
        assert_eq!(codificador_do(&compilar_alvo_com(&p, &prep, &previa, false, &serve(false)).unwrap()), "libx264");
        assert_eq!(*pedidos.borrow(), vec![(640, 360, 30), (640, 360, 30), (320, 180, 30)]);
        // sem pergunta: "Arquivo reprodutível", queda para CPU, WhatsApp e quadro final
        pedidos.borrow_mut().clear();
        for (a, cpu) in [(alvo("juri", "cpu"), false), (alvo("juri", "auto"), true), (alvo("whatsapp", "auto"), false), (Alvo::Quadro { q: 0, largura: 320 }, false)] {
            let plano = compilar_alvo_com(&p, &prep, &a, cpu, &serve(true)).unwrap();
            assert_ne!(codificador_do(&plano), "h264_nvenc", "{a:?}");
        }
        assert!(pedidos.borrow().is_empty(), "{:?}", pedidos.borrow());
    }

    /// WhatsApp pelo caminho da fila: o canvas sai reduzido (o desenho acompanha, são frações) e o teto de taxa da
    /// duração entra em todos os blocos, com a chave do bloco mudando junto.
    #[test]
    fn whatsapp_reduz_o_canvas_e_poe_o_teto_nos_blocos() {
        let tab = apoio::tabela((1, 30), (0..600).collect());
        let fonte = apoio::fonte(tab.clone(), (320, 180));
        let prep = preparado(com_codificadores(cap_falsa()), HashMap::from([("m1".to_string(), fonte)]));
        let nao = |_: &Capacidades, _: u32, _: u32, _: Racional| false;
        let p = projeto(1920, 1080, (30, 1), vec![midia("m1", "video", serde_json::json!([]))], vec![trecho("c1", "m1", &tab, 0, 600, 1.0, serde_json::json!([]))], serde_json::json!({}));
        let juri = compilar_alvo_com(&p, &prep, &alvo("juri", "cpu"), false, &nao).unwrap();
        assert_eq!((juri.canvas.w, juri.canvas.h, juri.proveniencia.saida.largura), (1920, 1080, 1920));
        assert!(!juri.blocos[0].codificador.iter().any(|a| a == "-maxrate"));
        let w = compilar_alvo_com(&p, &prep, &alvo("whatsapp", "cpu"), false, &nao).unwrap();
        assert_eq!((w.canvas.w, w.canvas.h, w.total_q), (1280, 720, 600));
        assert_eq!((w.proveniencia.saida.largura, w.proveniencia.saida.altura), (1280, 720), "o relatório diz o tamanho entregue");
        let linha = w.blocos[0].codificador.join(" ");
        // 20 s: 3500 kb/s
        assert!(linha.contains("-crf 22 -maxrate 3500k -bufsize 7000k -threads 8 -profile:v main -bf 0"), "{linha}");
        // a chave do bloco leva o teto
        let mut sem_teto = w.clone();
        for b in &mut sem_teto.blocos {
            b.codificador.retain(|a| !["-maxrate", "3500k", "-bufsize", "7000k"].contains(&a.as_str()));
        }
        let antes = sem_teto.blocos[0].chave.clone();
        acabar(&mut sem_teto);
        assert_eq!(sem_teto.blocos[0].codificador, w.blocos[0].codificador);
        assert_ne!(sem_teto.blocos[0].chave, antes);
        let de_novo = sem_teto.blocos[0].chave.clone();
        acabar(&mut sem_teto);
        assert_eq!(sem_teto.blocos[0].chave, de_novo, "uma vez só");
        // 10 min de programa: teto de 719 kb/s e 854×480
        let longo: Vec<_> = (0..30).map(|i| trecho(&format!("c{i}"), "m1", &tab, 0, 600, 1.0, serde_json::json!([]))).collect();
        let p = projeto(1920, 1080, (30, 1), vec![midia("m1", "video", serde_json::json!([]))], longo, serde_json::json!({}));
        let w = compilar_alvo_com(&p, &prep, &alvo("whatsapp", "cpu"), false, &nao).unwrap();
        assert_eq!((w.canvas.w, w.canvas.h, w.total_q), (854, 480, 18000));
        assert!(w.blocos.iter().all(|b| b.codificador.join(" ").contains("-maxrate 719k -bufsize 1438k")));
    }

    /// Defeito de 17/11: o WhatsApp saía na taxa do projeto (60 qps no caso real); o §5.7 manda até 30. A regra
    /// (`taxa`) e a fila inteira até o PDF, nos dois binários: projeto a 59,94 com a gravação de 59,94 sai a 29,97
    /// (lido do arquivo), cada slot com o quadro de origem do mapa (um a cada dois: metade descartada, com ciência
    /// pedida na análise), e o `tecnico.json` e o PDF dizem 29,97 ("Ritmo" e anexo D); projeto a 60 com gravação de
    /// 30 sai a 30, cada quadro uma vez.
    #[test]
    fn whatsapp_sai_a_no_maximo_30_qps() {
        use crate::edicao::pacote::{self, ponta};
        use crate::edicao::testes::aceite::trecho;
        let r = |num: i64, den: i64| Racional { num, den };
        let w = exportacao("whatsapp", "cpu");
        for (de, para) in [((60, 1), (30, 1)), ((60000, 1001), (30000, 1001)), ((50, 1), (25, 1)), ((48, 1), (24, 1)), ((120, 1), (30, 1)), ((90, 1), (30, 1)), ((100, 1), (25, 1)), ((120, 2), (30, 1)), ((30, 1), (30, 1)), ((30000, 1001), (30000, 1001)), ((25, 1), (25, 1)), ((24000, 1001), (24000, 1001))] {
            assert_eq!(taxa(&w, r(de.0, de.1)), r(para.0, para.1), "{de:?}");
        }
        for nome in ["juri", "preservacao", "personalizada"] {
            assert_eq!(taxa(&exportacao(nome, "cpu"), r(60, 1)), r(60, 1), "{nome}");
        }
        let Some(caso) = ponta::caso("whatsapp-30") else { return };
        let cap = caso.cap.clone();
        // (gravação, taxa do projeto, taxa da saída, quadros da gravação por slot)
        for (fonte, fps, saida, passo) in [(Fonte::Ts5994, (60000, 1001), (30000, 1001), 2usize), (Fonte::Mp4, (60, 1), (30, 1), 1)] {
            let onde = format!("{} {fonte:?}", cap.versao);
            let numeros = apoio::medida(&cap, fonte).numeros.clone();
            let (m, tab) = caso.midia("m1", &fontes::gerar(fonte).unwrap());
            let (j_in, n) = (30usize, 30usize);
            let mut doc = ponta::projeto("Zap", false, vec![m], vec![trecho("c1", "m1", &tab, j_in, j_in + n * passo, 1.0, serde_json::json!([]))], serde_json::json!({}));
            let exp = ponta::exportacao("whatsapp", "cpu", serde_json::json!({ "originais": false }), "a3");
            (doc["saida"]["fps"], doc["saida"]["exportacao"]) = (serde_json::json!({ "num": fps.0, "den": fps.1 }), exp.clone());
            // a análise da tela e a fila cobram a ciência dos descartados da taxa de saída (60 com gravação de 30: nada)
            let a = crate::commands::edicao_commands::analisar(&caso.ws, &doc, &caso.app).unwrap();
            let descartados = a.problemas.iter().find(|x| x.codigo == "quadros_descartados").map(|x| x.n);
            assert_eq!(descartados, (passo > 1).then_some(Some((n * (passo - 1)) as u64)), "{onde}: {:?}", a.problemas);
            assert_eq!(a.linha.fps, r(fps.0, fps.1), "{onde}: a linha da tela segue a do projeto");
            if passo > 1 {
                let falhou = format!("{:?}", caso.enfileirar(&doc, exp.clone(), &[]));
                assert!(falhou.contains("ciencia") && falhou.contains("quadros_descartados"), "{onde}: {falhou}");
            }
            let (fim, ciencias) = caso.exportar(&doc, exp);
            assert_eq!(ciencias.contains(&"quadros_descartados".to_string()), passo > 1, "{onde}: {ciencias:?}");
            let s = caso.saida(&ponta::id_da(&fim));
            let mp4 = caso.ws.join(&s.arquivo);
            // a taxa e os quadros do arquivo
            let o = crate::tools::command(&cap.ffprobe).args(["-v", "error", "-select_streams", "v", "-show_entries", "stream=r_frame_rate,avg_frame_rate", "-of", "json"]).arg(&mp4).output().unwrap();
            let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
            let taxa_lida = format!("{}/{}", saida.0, saida.1);
            assert_eq!((v["streams"][0]["r_frame_rate"].as_str(), v["streams"][0]["avg_frame_rate"].as_str()), (Some(taxa_lida.as_str()), Some(taxa_lida.as_str())), "{onde}: {v}");
            let esperados: Vec<u32> = (0..n).map(|k| numeros[j_in + passo * k]).collect();
            assert_eq!(medir::quadros_de_origem(&cap, &mp4).unwrap(), esperados, "{onde}: quadro de origem de cada slot");
            // o `tecnico.json`: a taxa da saída e o mapa (lidos, repetidos, descartados)
            let t: serde_json::Value = serde_json::from_slice(&std::fs::read(caso.ws.join(s.tecnico_rel.as_ref().unwrap())).unwrap()).unwrap();
            assert_eq!(t["feito"]["saida"]["fps"], serde_json::json!({ "num": saida.0, "den": saida.1 }), "{onde}");
            let origem = &t["feito"]["roteiro"][0]["origem"];
            assert_eq!((origem["lidos"].as_u64(), origem["repetidos"].as_u64(), origem["descartados"].as_u64()), (Some((n * passo) as u64), Some(0), Some((n * (passo - 1)) as u64)), "{onde}: {origem}");
            // o PDF: "Ritmo" do vídeo entregue e a taxa no anexo D
            let pdf = ponta::texto_do_pdf(&caso.pasta(&s).join(pacote::PDF)).unwrap_or_else(|| panic!("{onde}: sem pdftotext"));
            if passo > 1 {
                // a gravação aparece pelo passo medido (59,92)
                assert!(pdf.contains("Ritmo29,97imagensporsegundo") && pdf.contains("→29,97") && !pdf.contains("→59,94"), "{onde}: o PDF não diz 29,97");
            } else {
                assert!(pdf.contains("Ritmo30imagensporsegundo") && !pdf.contains("→60"), "{onde}: o PDF não diz 30");
            }
        }
    }

    fn sondar(cap: &Capacidades, arquivo: &Path) -> serde_json::Value {
        let o = crate::tools::command(&cap.ffprobe)
            .args(["-v", "error", "-show_entries", "stream=codec_type,codec_name,profile,width,height,pix_fmt,has_b_frames", "-of", "json"])
            .arg(arquivo)
            .output()
            .expect("ffprobe");
        serde_json::from_slice(&o.stdout).expect("json do ffprobe")
    }

    /// Média de cor dos quatro quadrantes (frações do próprio quadro: compara saídas de tamanhos diferentes).
    fn quadrantes(q: &medir::QuadroRgb) -> Vec<[f64; 3]> {
        let (w, h) = (q.w / 2, q.h / 2);
        [(0, 0), (w, 0), (0, h), (w, h)].iter().map(|&(x, y)| medir::cor_media(q, [x, y, w, h])).collect()
    }

    /// Aceite do §11: as três predefinições pelo motor inteiro, nos dois binários. 13 trechos = 2 blocos: o FFV1 sai em
    /// blocos MKV. Em cada saída: o quadro de origem de cada slot, o codec e o tamanho lidos do arquivo e a cor contra
    /// a saída sem perda; na preservação, os quadros decodificados são os compostos, bit a bit.
    #[test]
    fn tres_predefinicoes() {
        let Some(cap) = fontes::cap() else { return };
        for enc in ["ffv1", "flac"] {
            assert!(cap.codificadores.contains(enc), "{}: sem {enc}", cap.versao);
        }
        let m = apoio::medida(&cap, Fonte::Mp4);
        let tab = &m.info.quadros;
        let clipes: Vec<_> = (0..13).map(|i| trecho(&format!("c{i}"), "m1", tab, 10 + 3 * i, 12 + 3 * i, 1.0, serde_json::json!([]))).collect();
        let esperados: Vec<u32> = (0..13).flat_map(|i| [m.numeros[10 + 3 * i], m.numeros[11 + 3 * i]]).collect();
        let prep = preparado((*cap).clone(), HashMap::from([("m1".to_string(), m.info.clone())]));
        let nao = |_: &Capacidades, _: u32, _: u32, _: Racional| false;
        let mut referencia: Option<Vec<[f64; 3]>> = None;
        // a preservação primeiro: é a referência de cor das outras
        for (nome, tam, codec, perfil, ext, saida) in [
            ("preservacao", (640, 360), "ffv1", None, "mkv", (640, 360)),
            ("juri", (640, 360), "h264", Some("High"), "mp4", (640, 360)),
            ("whatsapp", (1920, 1080), "h264", Some("Main"), "mp4", (1280, 720)),
        ] {
            let p = projeto(tam.0, tam.1, (30, 1), vec![midia("m1", "video", serde_json::json!([]))], clipes.clone(), serde_json::json!({}));
            let plano = compilar_alvo_com(&p, &prep, &alvo(nome, "cpu"), false, &nao).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
            assert_eq!(plano.blocos.len(), 2, "{nome}: 13 entradas = 2 blocos");
            assert!(plano.blocos.iter().all(|b| b.saida.ends_with(&format!(".{ext}"))), "{nome}: bloco em {ext}");
            let pasta = fontes::pasta_temporaria(&format!("p18-predef-{nome}"));
            let (_, res) = executor::exportar(&cap, &plano, &pasta, "2026-11-15T03:00:00Z").unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
            assert_eq!(res.arquivo.extension().and_then(|e| e.to_str()), Some(ext), "{nome}");
            // o que o arquivo é
            let j = sondar(&cap, &res.arquivo);
            let v = j["streams"].as_array().and_then(|s| s.iter().find(|x| x["codec_type"] == "video")).cloned().unwrap_or_default();
            assert_eq!((v["codec_name"].as_str(), v["pix_fmt"].as_str()), (Some(codec), Some("yuv420p")), "{nome} ({}): {v}", cap.versao);
            assert_eq!((v["width"].as_u64(), v["height"].as_u64()), (Some(saida.0), Some(saida.1)), "{nome}: {v}");
            if let Some(perfil) = perfil {
                assert_eq!(v["profile"].as_str(), Some(perfil), "{nome}: {v}");
            }
            if nome == "whatsapp" {
                assert_eq!(v["has_b_frames"].as_u64(), Some(0), "sem quadros B: {v}");
                assert!(res.argumentos.join(" ").contains("-maxrate 3500k -bufsize 7000k"), "{:?}", res.argumentos);
            }
            // identidade de quadro: cada slot mostra o quadro de origem previsto, através dos dois blocos
            assert_eq!(medir::quadros_de_origem(&cap, &res.arquivo).unwrap(), esperados, "{nome} ({})", cap.versao);
            let conf = res.conferencia.as_ref().unwrap();
            assert_eq!(conf.quadros, 26, "{nome}");
            // pixel e cor
            let cores = quadrantes(&medir::quadro(&cap, &res.arquivo, 5).unwrap());
            match &referencia {
                None => {
                    // sem perda: o que se decodifica do arquivo é o que o grafo compôs
                    assert_eq!(Some(&conf.decodificado_sha256), res.composto_sha256.as_ref(), "{nome} ({}): FFV1 com perda", cap.versao);
                    assert!(!com_perdas(&plano.alvo));
                    referencia = Some(cores);
                }
                Some(r) => {
                    assert_ne!(Some(&conf.decodificado_sha256), res.composto_sha256.as_ref(), "{nome}: H.264 não é sem perda");
                    for (a, b) in cores.iter().zip(r) {
                        for k in 0..3 {
                            assert!((a[k] - b[k]).abs() <= COR_MAX, "{nome} ({}): cor {a:?} × {b:?}", cap.versao);
                        }
                    }
                }
            }
        }
    }

    /// Maior diferença da média de cor de um quadrante entre a saída com perda e a sem perda (0..255).
    const COR_MAX: f64 = 3.0;

    #[test]
    fn fecho_com_e_sem_framehash() {
        let mut g = Grafo::novo();
        fecho(&mut g, "j", true);
        assert_eq!(g.texto(), "[j]format=yuv420p,setsar=1,setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv,split[v][h]");
        let mut g = Grafo::novo();
        fecho(&mut g, "j", false);
        assert!(g.texto().ends_with("range=tv[v]"));
    }
}
