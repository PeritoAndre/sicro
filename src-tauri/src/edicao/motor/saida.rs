//! Fecho do bloco e predefinições de codificação (§5.7): júri e prévia do render; WhatsApp, preservação e
//! personalizada no P1.8. Dono: P1.8.

use super::grafo::Grafo;
use super::plano::Canvas;
use super::Alvo;
use crate::edicao::ffmpeg::args;
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::modelo::{Codec, EscolhaCodificador, Exportacao, Predefinicao};
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
fn nvenc(canvas: &Canvas, cap: &Capacidades) -> Option<Vec<String>> {
    let g = cap.gpu.as_ref()?;
    (g.codificador == "h264_nvenc" && canvas.w <= NVENC_LADO_MAX && canvas.h <= NVENC_LADO_MAX).then(|| g.args.clone())
}

/// Detalhe do `nao_suportado` da predefinição que o motor ainda não codifica; None = codifica. Uma regra só:
/// `validar` (fase_0) e `codificacao` perguntam aqui.
pub fn recusa(exportacao: &Exportacao) -> Option<&'static str> {
    (exportacao.predefinicao != Predefinicao::Juri).then_some("predefinicao")
}

/// Codificação do alvo: predefinição + sonda (GPU aprovada no tamanho do projeto, senão `libx264`).
/// Pura: usa o que `cap.gpu` já traz; quem monta o `Contexto` roda antes `sonda::gpu_serve` no tamanho do projeto
/// e tira a GPU das capacidades se ela não serve.
pub fn codificacao(alvo: &Alvo, canvas: &Canvas, cap: &Capacidades) -> Result<Codificacao> {
    let comuns = |mut v: Vec<String>, g: &str| {
        v.extend(textos(&["-g", g, "-pix_fmt", "yuv420p", "-fps_mode", "passthrough"]));
        v.extend(textos(&ETIQUETAS));
        v
    };
    match alvo {
        Alvo::Final { exportacao } => {
            if recusa(exportacao).is_some() {
                let nome = serde_json::to_value(exportacao.predefinicao).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
                return Err(SicroError::Validation(format!("nao_suportado: predefinicao {nome}")));
            }
            let g = gop(canvas, 2);
            let gpu = if exportacao.codificador == EscolhaCodificador::Auto { nvenc(canvas, cap) } else { None };
            let (nome, video) = match gpu {
                Some(a) => {
                    let mut v = textos(&["-c:v", "h264_nvenc"]);
                    v.extend(a);
                    ("h264_nvenc", comuns(v, &g))
                }
                None => {
                    let v = textos(&["-c:v", "libx264", "-preset", "medium", "-crf", "18", "-threads", "8", "-profile:v", "high"]);
                    let mut v = comuns(v, &g);
                    v.extend(textos(&["-filter_complex_threads", "4"]));
                    ("libx264", v)
                }
            };
            Ok(Codificacao { nome: nome.into(), gpu: nome == "h264_nvenc", video, audio: textos(&["-c:a", "aac", "-b:a", "192k"]), extensao: "mp4".into(), timescale: timescale(canvas) })
        }
        Alvo::Previa { .. } => {
            let g = gop(canvas, 1);
            let (nome, video) = match nvenc(canvas, cap) {
                Some(_) => ("h264_nvenc", comuns(textos(&["-c:v", "h264_nvenc", "-preset", "p1", "-rc", "vbr", "-cq", "26", "-b:v", "0", "-bf", "0"]), &g)),
                None => ("libx264", comuns(textos(&["-c:v", "libx264", "-preset", "veryfast", "-crf", "24"]), &g)),
            };
            Ok(Codificacao { nome: nome.into(), gpu: nome == "h264_nvenc", video, audio: textos(&["-c:a", "aac", "-b:a", "128k"]), extensao: "mp4".into(), timescale: timescale(canvas) })
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

    #[test]
    fn previa_quadro_e_predefinicao_de_outra_fase() {
        let p = codificacao(&Alvo::Previa { altura: 540, de_q: 0, ate_q: 90 }, &canvas(960, 540, (25, 1)), &com_nvenc()).unwrap();
        assert!(p.video.join(" ").starts_with("-c:v h264_nvenc -preset p1 -rc vbr -cq 26 -b:v 0 -bf 0 -g 25"));
        let p = codificacao(&Alvo::Previa { altura: 540, de_q: 0, ate_q: 90 }, &canvas(960, 540, (30000, 1001)), &cap_falsa()).unwrap();
        assert!(p.video.join(" ").starts_with("-c:v libx264 -preset veryfast -crf 24 -g 30"));
        let q = codificacao(&Alvo::Quadro { q: 3, largura: 640 }, &canvas(640, 360, (30, 1)), &cap_falsa()).unwrap();
        assert_eq!((q.extensao.as_str(), q.gpu), ("bmp", false));
        for nome in ["whatsapp", "preservacao", "personalizada"] {
            let e = codificacao(&Alvo::Final { exportacao: exportacao(nome, "auto") }, &canvas(640, 360, (30, 1)), &cap_falsa()).unwrap_err().to_string();
            assert!(e.contains("nao_suportado") && e.contains(nome), "{e}");
        }
        assert!(com_perdas(&Alvo::Final { exportacao: exportacao("juri", "auto") }));
        assert!(!com_perdas(&Alvo::Final { exportacao: exportacao("preservacao", "auto") }));
    }

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
