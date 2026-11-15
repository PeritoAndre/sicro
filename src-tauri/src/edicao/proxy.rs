//! Proxy de prévia (§7.6): GPU (`h264_nvenc` com `scale_cuda`) com queda para a CPU (`previa::proxy_cpu`, O8),
//! 720p, GOP 12 sem quadros B, pts iguais aos da fonte (`-fps_mode passthrough -enc_time_base demux`). Nunca entra em
//! hash, relatório nem render. A decisão "cópia ou proxy" fica em `previa.rs` (também do P1.9). Dono: P1.9.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::modelo::{Faixa, Racional};
use crate::edicao::motor::FonteInfo;
use crate::edicao::previa::{self, PreviaMidia, PROXY_GOP, SAIDA};
use crate::error::Result;

// CONTRATO: acréscimo (P1.9): limites de `precisa`, para o palco e os testes.
/// Lado menor acima disso pede proxy (4K por desempenho; 1080×1920 em pé toca como 1080p).
pub const LADO_MAX: u32 = 1080;
/// Quadros por segundo acima disso pedem proxy (com 1 % de folga: 60 em base de 1 ms dá passos de 16 e 17).
pub const QPS_MAX: f64 = 60.0;
const FOLGA_QPS: f64 = 1.01;
/// Busca pausada medida (p50 de 3, ms) acima disso pede proxy (M9: 480p com GOP de 20 s fica em 107; 1080p com 10 s, 238).
pub const BUSCA_P50_MAX_MS: f64 = 120.0;
/// `-cq` do NVENC (`receitas-gpu-ffmpeg.md` §5).
const CQ_GPU: u32 = 24;

/// A mídia toca pelo proxy? Codec ≠ H.264 8 bits 4:2:0, lado menor > 1080, taxa variável, mais de 60 qps, ou busca lenta
/// medida (`busca_p50_ms` > 120: 3 buscas pausadas quando a mídia entra na Edição). Rampa, ré e abaixo de 0,25× são do
/// clipe: o palco pede o proxy nesses casos.
pub fn precisa(fonte: &FonteInfo, busca_p50_ms: Option<f64>) -> bool {
    motivo(fonte, busca_p50_ms).is_some()
}

/// Por que o proxy (vai para o `.json` dele); None = não precisa.
pub(crate) fn motivo(fonte: &FonteInfo, busca_p50_ms: Option<f64>) -> Option<String> {
    if !previa::codec_serve(fonte) {
        return Some(format!("codec: {} {}", fonte.codec, fonte.pix_fmt));
    }
    let (w, h) = fonte.exibido;
    if w.min(h) > LADO_MAX {
        return Some(format!("tamanho: {w}x{h}"));
    }
    if fonte.quadros.vfr {
        return Some("taxa variável".into());
    }
    if let Some(q) = qps(fonte).filter(|&q| q > QPS_MAX * FOLGA_QPS) {
        return Some(format!("taxa: {q:.3} qps"));
    }
    busca_p50_ms.filter(|&ms| ms > BUSCA_P50_MAX_MS).map(|ms| format!("busca: {ms:.0} ms"))
}

/// Quadros por segundo médios da tabela (do primeiro ao último quadro); None com menos de 2 quadros.
fn qps(fonte: &FonteInfo) -> Option<f64> {
    let t = &fonte.quadros;
    let (a, b) = (*t.pts.first()?, *t.pts.last()?);
    let dur = (b - a) as f64 * t.tb.num as f64 / t.tb.den as f64;
    (t.pts.len() > 1 && dur > 0.0).then(|| (t.pts.len() - 1) as f64 / dur)
}

/// Faz o proxy (bloqueia a thread): GPU aprovada na sonda primeiro; código ≠ 0 refaz em CPU. Lista de pts igual à da
/// fonte. `progresso` em 0..1.
pub fn produzir(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, cancelar: &AtomicBool, progresso: &mut dyn FnMut(f32)) -> Result<PreviaMidia> {
    produzir_por(cap, cache, fonte, motivo(fonte, None), cancelar, progresso)
}

/// `produzir` com o motivo dado (a queda do remux grava o erro dele).
pub(crate) fn produzir_por(
    cap: &Capacidades,
    cache: &Path,
    fonte: &FonteInfo,
    motivo: Option<String>,
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(f32),
) -> Result<PreviaMidia> {
    queda(
        gpu_serve(cap, fonte),
        cancelar,
        motivo,
        |m, p| previa::fazer_proxy(cap, cache, fonte, true, m, cancelar, p),
        |m, p| previa::fazer_proxy(cap, cache, fonte, false, m, cancelar, p),
        progresso,
    )
}

/// Regra da queda: GPU que falha (menos por cancelamento) refaz em CPU, com o erro dela no motivo.
fn queda(
    gpu: bool,
    cancelar: &AtomicBool,
    motivo: Option<String>,
    na_gpu: impl FnOnce(Option<String>, &mut dyn FnMut(f32)) -> Result<PreviaMidia>,
    na_cpu: impl FnOnce(Option<String>, &mut dyn FnMut(f32)) -> Result<PreviaMidia>,
    progresso: &mut dyn FnMut(f32),
) -> Result<PreviaMidia> {
    if !gpu {
        return na_cpu(motivo, progresso);
    }
    match na_gpu(motivo.clone(), progresso) {
        Ok(p) => Ok(p),
        Err(e) if previa::cancelado(&e) || cancelar.load(Ordering::SeqCst) => Err(e),
        Err(e) => {
            let e = crate::edicao::fundo::texto_do_erro(&e);
            // a classe `gpu` do executor já começa por "gpu: "
            let e = e.strip_prefix("gpu: ").unwrap_or(&e);
            let m = match motivo {
                Some(m) => format!("{m}; gpu: {e}"),
                None => format!("gpu: {e}"),
            };
            na_cpu(Some(m), progresso)
        }
    }
}

/// Caminho de GPU: NVENC aprovado com `scale_cuda` (NVDEC, escala e codificação na placa). Giro do contêiner e faixa
/// cheia ficam na CPU: o `scale_cuda` não gira nem converte a faixa. A escala que ele zera (`escala_serve`) é
/// recusada em `previa::fazer`, que conhece o tamanho decodificado.
fn gpu_serve(cap: &Capacidades, fonte: &FonteInfo) -> bool {
    cap.gpu.as_ref().is_some_and(|g| g.codificador == "h264_nvenc" && g.scale_cuda) && fonte.giro == 0 && fonte.faixa == Faixa::Tv
}

/// A escala da GPU serve de `de` para `para` (px decodificados → px do proxy)? O `scale_cuda` do 9.0.1 devolve quadros
/// zerados (tela verde), sem erro, quando reduz um lado e mantém o outro (720×480 com pixel 10:11 → 654×480; medido
/// com bilinear, bicúbico e lanczos): esses vão para a CPU.
pub(crate) fn escala_serve(de: (u32, u32), para: (u32, u32)) -> bool {
    let so_um = |a: u32, b: u32, c: u32, d: u32| b < a && d == c;
    !(so_um(de.0, para.0, de.1, para.1) || so_um(de.1, para.1, de.0, para.0))
}

/// Receita de GPU do §7.6 com o tamanho exibido explícito e pixel quadrado (o mesmo da CPU).
pub(crate) fn args_gpu(fonte: &Path, audio: &[String], tb: Racional, (w, h): (u32, u32)) -> Vec<String> {
    let mut a = previa::s(&["-hide_banner", "-nostdin", "-y", "-nostats", "-loglevel", "error", "-hwaccel", "cuda", "-hwaccel_output_format", "cuda", "-i"]);
    a.push(fonte.to_string_lossy().into_owned());
    a.extend(previa::s(&["-map", "0:v:0", "-vf"]));
    a.push(format!("scale_cuda={w}:{h}:format=nv12,setsar=1"));
    a.extend(previa::s(&["-c:v", "h264_nvenc", "-preset", "p1", "-rc", "vbr", "-b:v", "0", "-cq"]));
    a.push(CQ_GPU.to_string());
    a.extend(previa::s(&["-profile:v", "high", "-g"]));
    a.push(PROXY_GOP.to_string());
    a.extend(previa::s(&["-bf", "0", "-no-scenecut", "1", "-fps_mode", "passthrough", "-enc_time_base", "demux", "-video_track_timescale"]));
    a.push(tb.den.to_string());
    a.extend(audio.iter().cloned());
    a.extend(previa::s(&["-movflags", "+faststart", SAIDA]));
    a
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::{Arc, OnceLock};
    use std::time::{Duration, Instant};

    use serde_json::{json, Value};

    use crate::edicao::ffmpeg::sonda::{self, Gpu};
    use crate::edicao::modelo::Racional;
    use crate::edicao::previa::testes::{conferir_proxy, extra, fonte_de_teste, fonte_falsa, hashes_por_quadro, registro_do_proxy, sem_progresso, tb_do_video, MP4};
    use crate::edicao::previa::TipoPrevia;
    use crate::edicao::testes::fontes::{self, Fonte};
    use crate::edicao::testes::medir;
    use crate::error::SicroError;

    fn com_gpu(cap: &Capacidades) -> Capacidades {
        Capacidades {
            gpu: Some(Gpu {
                nome: "placa".into(),
                driver: "610".into(),
                codificador: "h264_nvenc".into(),
                degrau: "completo".into(),
                args: Vec::new(),
                nvdec: true,
                scale_cuda: true,
            }),
            ..cap.clone()
        }
    }

    // ------------------------------------------------------------ sem ffmpeg

    #[test]
    fn precisa_pelos_criterios_da_fonte_e_pela_busca_medida() {
        let base = || fonte_falsa(MP4, "h264", "yuv420p", 0.0, 0);
        let f = base();
        assert!(!precisa(&f, None));
        // busca medida: p50 acima de 120 ms
        for (ms, esperado) in [(60.0, false), (120.0, false), (120.5, true), (238.0, true)] {
            assert_eq!(precisa(&f, Some(ms)), esperado, "{ms}");
        }
        assert_eq!(motivo(&f, Some(238.0)).as_deref(), Some("busca: 238 ms"));
        // codec
        for (codec, px) in [("hevc", "yuv420p"), ("h264", "yuv420p10le"), ("h264", "yuv422p"), ("mpeg4", "yuv420p")] {
            let f = fonte_falsa(MP4, codec, px, 0.0, 0);
            assert_eq!(motivo(&f, None), Some(format!("codec: {codec} {px}")));
        }
        assert!(!precisa(&fonte_falsa(MP4, "h264", "yuvj420p", 0.0, 0), None));
        // tamanho: o lado menor (1080p deitado ou em pé toca)
        for (exibido, esperado) in [((1920, 1080), false), ((1080, 1920), false), ((2560, 1440), true), ((1440, 2560), true), ((3840, 2160), true), ((2592, 1520), true)] {
            let mut f = base();
            f.exibido = exibido;
            assert_eq!(precisa(&f, None), esperado, "{exibido:?}");
        }
        // taxa variável e mais de 60 qps (60 exatos e 59,94 tocam; 60 em base de 1 ms também)
        let mut f = base();
        Arc::make_mut(&mut f.quadros).vfr = true;
        assert_eq!(motivo(&f, None).as_deref(), Some("taxa variável"));
        let com_pts = |tb: i64, pts: Vec<i64>| {
            let mut f = base();
            let t = Arc::make_mut(&mut f.quadros);
            t.tb = Racional { num: 1, den: tb };
            t.pts = pts;
            f
        };
        let passos = |inicio: i64, p: &[i64], n: usize| (0..n).scan(inicio, |a, i| Some(std::mem::replace(a, *a + p[i % p.len()]))).collect::<Vec<i64>>();
        for (f, esperado) in [
            (com_pts(15360, passos(0, &[256], 120)), false),
            (com_pts(60000, passos(0, &[1001], 120)), false),
            (com_pts(1000, passos(0, &[17, 17, 16], 120)), false),
            (com_pts(15360, passos(0, &[128], 240)), true),
            (com_pts(1000, passos(5, &[8], 100)), true),
            (com_pts(15360, vec![0]), false),
        ] {
            assert_eq!(precisa(&f, None), esperado, "{:?}", &f.quadros.pts[..f.quadros.pts.len().min(4)]);
        }
        assert_eq!(motivo(&com_pts(15360, passos(0, &[128], 240)), None).as_deref(), Some("taxa: 120.000 qps"));
    }

    #[test]
    fn queda_da_gpu_para_a_cpu_so_quando_a_gpu_falha() {
        let p = |tipo| PreviaMidia { relativo: String::new(), tipo, desloc_s: 0.0, de_s: 0.0, ate_s: 0.0, largura: 2, altura: 2, pronta: true };
        let nao = AtomicBool::new(false);
        let mut prog = |_: f32| {};
        // sem GPU: CPU direto, com o motivo dado
        let mut visto = None;
        queda(false, &nao, Some("codec: x".into()), |_, _| panic!("gpu"), |m, _| { visto = m; Ok(p(TipoPrevia::Proxy)) }, &mut prog).unwrap();
        assert_eq!(visto.as_deref(), Some("codec: x"));
        // GPU boa: a CPU não roda
        queda(true, &nao, None, |m, _| { assert!(m.is_none()); Ok(p(TipoPrevia::Proxy)) }, |_, _| panic!("cpu"), &mut prog).unwrap();
        // GPU falha: CPU, com o erro dela no motivo
        let mut visto = None;
        queda(
            true,
            &nao,
            Some("tamanho: 3840x2160".into()),
            |_, _| Err(SicroError::Validation("outra: previa (código Some(218)): scale_cuda".into())),
            |m, _| { visto = m; Ok(p(TipoPrevia::Proxy)) },
            &mut prog,
        )
        .unwrap();
        assert_eq!(visto.as_deref(), Some("tamanho: 3840x2160; gpu: outra: previa (código Some(218)): scale_cuda"));
        // erro já da classe `gpu`: um prefixo só
        let mut visto = None;
        queda(true, &nao, None, |_, _| Err(SicroError::Validation("gpu: previa (código Some(218)): nvenc".into())), |m, _| { visto = m; Ok(p(TipoPrevia::Proxy)) }, &mut prog).unwrap();
        assert_eq!(visto.as_deref(), Some("gpu: previa (código Some(218)): nvenc"));
        // cancelada (pelo erro ou pelo sinal): não refaz
        let e = queda(true, &nao, None, |_, _| Err(SicroError::Validation("cancelado: prévia".into())), |_, _| panic!("cpu"), &mut prog).unwrap_err();
        assert!(e.to_string().contains("cancelado"));
        let sim = AtomicBool::new(true);
        assert!(queda(true, &sim, None, |_, _| Err(SicroError::Validation("outra: x".into())), |_, _| panic!("cpu"), &mut prog).is_err());
    }

    #[test]
    fn gpu_so_com_nvenc_e_scale_cuda_sem_giro_e_em_faixa_limitada() {
        let cap = crate::edicao::quadros::apoio::cap_falsa();
        let f = fonte_falsa(MP4, "hevc", "yuv420p10le", 0.0, 0);
        assert!(!gpu_serve(&cap, &f));
        let g = com_gpu(&cap);
        assert!(gpu_serve(&g, &f));
        let mut girada = f.clone();
        girada.giro = 90;
        let mut cheia = f.clone();
        cheia.faixa = Faixa::Pc;
        assert!(!gpu_serve(&g, &girada) && !gpu_serve(&g, &cheia));
        for (cod, scale) in [("h264_nvenc", false), ("h264_vaapi", true), ("hevc_nvenc", true)] {
            let mut c = com_gpu(&cap);
            let gp = c.gpu.as_mut().unwrap();
            (gp.codificador, gp.scale_cuda) = (cod.into(), scale);
            assert!(!gpu_serve(&c, &f), "{cod} {scale}");
        }
        let a = args_gpu(Path::new("/m/v.mkv"), &previa::args_audio(Some("pcm_alaw")), Racional { num: 1, den: 1000 }, (1228, 720)).join(" ");
        assert!(a.contains("-hwaccel cuda -hwaccel_output_format cuda -i /m/v.mkv -map 0:v:0 -vf scale_cuda=1228:720:format=nv12,setsar=1 -c:v h264_nvenc"), "{a}");
        assert!(a.contains("-g 12 -bf 0 -no-scenecut 1 -fps_mode passthrough -enc_time_base demux -video_track_timescale 1000 -map 0:a:0 -c:a aac"), "{a}");
        assert!(a.ends_with("-movflags +faststart saida.mp4"), "{a}");
    }

    #[test]
    fn escala_da_gpu_nao_reduz_um_lado_so() {
        // medido no 9.0.1 (`scale_cuda`, RTX 4060 Ti): um lado menor e o outro igual sai zerado
        for (de, para, serve) in [
            ((720, 480), (654, 480), false),
            ((720, 480), (640, 480), false),
            ((1280, 722), (1280, 720), false),
            ((1920, 1080), (1280, 1080), false),
            ((720, 480), (720, 480), true),
            ((1920, 1080), (1280, 720), true),
            ((2592, 1520), (1228, 720), true),
            ((720, 576), (768, 576), true),
            ((960, 1080), (1280, 720), true),
            ((720, 480), (654, 478), true),
        ] {
            assert_eq!(escala_serve(de, para), serve, "{de:?} → {para:?}");
        }
    }

    // ------------------------------------------------------------ com ffmpeg (os dois binários)

    /// Capacidades com a GPU de verdade (sonda completa, uma vez por processo); None = sem NVENC com `scale_cuda`.
    fn gpu_real(cap: &Capacidades) -> Option<Arc<Capacidades>> {
        static G: OnceLock<Option<Arc<Capacidades>>> = OnceLock::new();
        G.get_or_init(|| {
            let c = sonda::sondar(&cap.binario).ok()?;
            eprintln!("GPU do proxy em {}: {:?}", c.versao, c.gpu.as_ref().map(|g| (&g.codificador, g.scale_cuda)));
            c.gpu.as_ref().is_some_and(|g| g.codificador == "h264_nvenc" && g.scale_cuda).then(|| Arc::new(c))
        })
        .clone()
    }

    /// Base de tempo do fluxo e pts do decodificador a partir do primeiro quadro.
    fn pts_relativos(cap: &Capacidades, video: &Path) -> (Racional, Vec<i64>) {
        let pts = medir::pts(cap, video).unwrap();
        let p0 = pts[0];
        (tb_do_video(cap, video), pts.iter().map(|p| p - p0).collect())
    }

    /// Lista de pts do proxy = a da fonte, número a número: mesma base (`-video_track_timescale`) e os mesmos ticks
    /// depois do primeiro quadro (o deslocamento do primeiro é o `desloc_s`, conferido em `conferir_proxy`).
    fn pts_iguais(cap: &Capacidades, fonte: &FonteInfo, p: &PreviaMidia, cache: &Path, nome: &str) {
        let (tf, pf) = pts_relativos(cap, &fonte.caminho);
        let (tp, pp) = pts_relativos(cap, &cache.join(&p.relativo));
        assert_eq!((tf, pf.len()), (tp, pp.len()), "{nome}: base e quadros ({})", cap.versao);
        if let Some(j) = (0..pf.len()).find(|&j| pf[j] != pp[j]) {
            panic!("{nome}: quadro {j} com pts {} na fonte e {} no proxy ({})", pf[j], pp[j], cap.versao);
        }
    }

    /// As fontes do aceite: MP4, MKV de 30 qps, MKV com oscilação de ±4 ms e o Dahua real (quando presente).
    /// `bool` = a régua de quadros da matriz serve (o Dahua não tem).
    fn fontes_do_aceite(cap: &Capacidades) -> Vec<(String, FonteInfo, bool)> {
        let mut v: Vec<(String, FonteInfo, bool)> =
            [Fonte::Mp4, Fonte::Mkv30, Fonte::Oscilacao].iter().map(|&f| (format!("{f:?}"), fonte_de_teste(cap, &fontes::gerar(f).unwrap(), true), true)).collect();
        let dahua = fontes::amostras_de_gravador().into_iter().find(|a| a.file_name().is_some_and(|n| n.to_string_lossy().starts_with("dahua")));
        match dahua {
            Some(d) => v.push(("dahua.dav".into(), fonte_de_teste(cap, &d, true), false)),
            None => eprintln!("aviso: amostra Dahua ausente (p1b/dl/dvr/amostras): dispensada"),
        }
        v
    }

    /// Lista de pts igual à da fonte (§11) pela CPU e, com NVENC aprovado, pela GPU.
    #[test]
    fn pts_iguais_aos_da_fonte() {
        let Some(cap) = fontes::cap() else { return };
        let gpu = gpu_real(&cap);
        if gpu.is_none() {
            eprintln!("aviso: sem NVENC com scale_cuda em {}: só a CPU", cap.versao);
        }
        for (nome, f, regua) in fontes_do_aceite(&cap) {
            let vias: Vec<(&str, Capacidades)> = std::iter::once(("cpu", (*cap).clone())).chain(gpu.as_ref().map(|g| ("gpu", (**g).clone()))).collect();
            for (via, c) in vias {
                let cache = fontes::pasta_temporaria("proxy-pts");
                let t = Instant::now();
                let p = produzir(&c, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap_or_else(|e| panic!("{nome} {via}: {e} ({})", cap.versao));
                let (args, motivo) = registro_do_proxy(&cache, &f.sha256);
                let cod = if via == "gpu" { "h264_nvenc" } else { "libx264" };
                assert!(args.iter().any(|a| a == cod), "{nome} {via}: {args:?} {motivo:?}");
                assert!(!motivo.as_deref().unwrap_or("").contains("gpu:"), "{nome} {via}: {motivo:?}");
                conferir_proxy(&cap, &f, &p, &cache, regua, None);
                pts_iguais(&cap, &f, &p, &cache, &format!("{nome} {via}"));
                eprintln!("{nome} {via} ({}): {} quadros, {}x{}, {:.1} s, motivo {motivo:?}", cap.versao, f.quadros.pts.len(), p.largura, p.altura, t.elapsed().as_secs_f64());
            }
        }
    }

    /// GPU que falha refaz em CPU, com a mesma lista de pts e o erro dela no motivo: H.264 4:4:4 com a GPU anunciada
    /// (o 7.0.2 não tem CUDA; o NVDEC desta placa não lê 4:4:4). Com a GPU de verdade, o 4:2:0 sai da placa.
    #[test]
    fn gpu_cai_para_cpu() {
        let Some(cap) = fontes::cap() else { return };
        let m = fontes::gerar(Fonte::Mp4).unwrap().to_string_lossy().into_owned();
        let v444 = extra(
            &cap,
            "h264-444.mp4",
            &["-i", &m, "-map", "0:v", "-map", "0:a", "-t", "5", "-c:v", "libx264", "-preset", "veryfast", "-crf", "10", "-pix_fmt", "yuv444p", "-g", "30", "-c:a", "copy"],
        );
        let f444 = fonte_de_teste(&cap, &v444, true);
        assert_eq!(motivo(&f444, None).as_deref(), Some("codec: h264 yuv444p"));
        let queda_em = |c: &Capacidades, nome: &str| {
            assert!(gpu_serve(c, &f444), "{nome}");
            let cache = fontes::pasta_temporaria("proxy-queda");
            let mut visto = Vec::new();
            let p = produzir(c, &cache, &f444, &AtomicBool::new(false), &mut |x| visto.push(x)).unwrap_or_else(|e| panic!("{nome}: {e} ({})", cap.versao));
            let (args, motivo) = registro_do_proxy(&cache, &f444.sha256);
            assert!(args.iter().any(|a| a == "libx264") && !args.iter().any(|a| a == "h264_nvenc"), "{nome}: {args:?}");
            assert!(motivo.as_deref().is_some_and(|m| m.starts_with("codec: h264 yuv444p; gpu: ")), "{nome}: {motivo:?} ({})", cap.versao);
            assert_eq!(visto.last(), Some(&1.0), "{nome}");
            conferir_proxy(&cap, &f444, &p, &cache, true, None);
            pts_iguais(&cap, &f444, &p, &cache, nome);
            // a tentativa da GPU não deixa nada: só o proxy e o `.json`
            let resto: Vec<_> = std::fs::read_dir(cache.join("proxies")).unwrap().flatten().map(|e| e.file_name()).collect();
            assert_eq!(resto.len(), 2, "{nome}: {resto:?}");
            eprintln!("{nome} ({}): {}", cap.versao, motivo.unwrap_or_default());
        };
        queda_em(&com_gpu(&cap), "GPU anunciada");
        let Some(g) = gpu_real(&cap) else {
            eprintln!("aviso: sem NVENC com scale_cuda em {}: a GPU de verdade fica dispensada", cap.versao);
            return;
        };
        queda_em(&g, "GPU de verdade, 4:4:4");
        let f = fonte_de_teste(&cap, &fontes::gerar(Fonte::Mkv30).unwrap(), true);
        let cache = fontes::pasta_temporaria("proxy-gpu");
        let p = produzir(&g, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        let (args, motivo) = registro_do_proxy(&cache, &f.sha256);
        assert!(args.iter().any(|a| a == "h264_nvenc") && motivo.is_none(), "{args:?} {motivo:?}");
        conferir_proxy(&cap, &f, &p, &cache, true, None);
        pts_iguais(&cap, &f, &p, &cache, "GPU de verdade, 4:2:0");
    }

    /// Pixel não quadrado (720×480 a 10:11 → 654×480) com a GPU anunciada: a escala que o `scale_cuda` zera fica na
    /// CPU e a imagem é a da fonte (régua de quadros e cor). Sem a regra, a placa entrega o proxy inteiro verde.
    #[test]
    fn pixel_nao_quadrado_nao_passa_pela_escala_da_gpu() {
        let Some(cap) = fontes::cap() else { return };
        let f = fonte_de_teste(&cap, &fontes::gerar(Fonte::PixelNaoQuadrado).unwrap(), true);
        let anunciada = com_gpu(&cap);
        for (nome, c) in std::iter::once(("GPU anunciada", &anunciada)).chain(gpu_real(&cap).as_deref().map(|g| ("GPU de verdade", g))) {
            assert!(gpu_serve(c, &f), "{nome}");
            let cache = fontes::pasta_temporaria("proxy-pixel");
            let p = produzir(c, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap_or_else(|e| panic!("{nome}: {e} ({})", cap.versao));
            assert_eq!((p.largura, p.altura), (654, 480), "{nome}");
            let (args, motivo) = registro_do_proxy(&cache, &f.sha256);
            assert!(args.iter().any(|a| a == "libx264"), "{nome}: {args:?}");
            assert!(motivo.as_deref().is_some_and(|m| m.contains("gpu: nao_suportado: scale_cuda de 720x480 para 654x480")), "{nome}: {motivo:?}");
            conferir_proxy(&cap, &f, &p, &cache, true, None);
        }
    }

    /// Placa que entrega outra imagem sem erro nenhum (aqui um "ffmpeg" que troca a saída do NVENC por um quadro de
    /// cor lisa): a conferência da imagem recusa o proxy de GPU e a CPU refaz, com a imagem da fonte.
    #[cfg(unix)]
    #[test]
    fn gpu_que_entrega_outra_imagem_cai_para_a_cpu() {
        use std::os::unix::fs::PermissionsExt;
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("proxy-outra-imagem");
        let falso = pasta.join("ffmpeg-falso.sh");
        let real = cap.binario.to_string_lossy().replace('\'', "'\\''");
        let texto = format!(
            "#!/bin/sh\ncase \" $* \" in\n*\" h264_nvenc \"*) exec '{real}' -hide_banner -nostdin -y -loglevel error -f lavfi -i color=c=magenta:s=320x180:r=30:d=0.2 -c:v libx264 -pix_fmt yuv420p saida.mp4 ;;\n*) exec '{real}' \"$@\" ;;\nesac\n"
        );
        std::fs::write(&falso, texto).unwrap();
        std::fs::set_permissions(&falso, std::fs::Permissions::from_mode(0o755)).unwrap();
        let c = Capacidades { binario: falso, ..com_gpu(&cap) };
        let f = fonte_de_teste(&cap, &fontes::gerar(Fonte::Mp4).unwrap(), true);
        assert!(gpu_serve(&c, &f));
        let cache = pasta.join("cache");
        let p = produzir(&c, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap_or_else(|e| panic!("{e} ({})", cap.versao));
        let (args, motivo) = registro_do_proxy(&cache, &f.sha256);
        assert!(args.iter().any(|a| a == "libx264") && !args.iter().any(|a| a == "h264_nvenc"), "{args:?}");
        assert!(motivo.as_deref().is_some_and(|m| m.starts_with("gpu: leitura_divergente: imagem do proxy")), "{motivo:?} ({})", cap.versao);
        conferir_proxy(&cap, &f, &p, &cache, true, None);
    }

    /// O caso de uso do caminho de GPU (§7.6): HEVC de 8 e de 10 bits lidos pelo NVDEC, com a imagem da fonte (régua
    /// e cor) e a mesma lista de pts. H.264 de 10 bits o NVDEC não lê: cai para a CPU.
    #[test]
    fn gpu_de_verdade_le_hevc_de_8_e_de_10_bits() {
        let Some(cap) = fontes::cap() else { return };
        let Some(g) = gpu_real(&cap) else {
            eprintln!("aviso: sem NVENC com scale_cuda em {}: dispensado", cap.versao);
            return;
        };
        let m = fontes::gerar(Fonte::Mp4).unwrap().to_string_lossy().into_owned();
        let x265 = |nome: &str, px: &str| {
            extra(&cap, nome, &["-i", &m, "-map", "0:v", "-t", "5", "-c:v", "libx265", "-preset", "ultrafast", "-crf", "12", "-x265-params", "log-level=error", "-pix_fmt", px, "-tag:v", "hvc1"])
        };
        let h10 = extra(&cap, "proxy-h264-10.mp4", &["-i", &m, "-map", "0:v", "-t", "5", "-c:v", "libx264", "-preset", "veryfast", "-crf", "10", "-pix_fmt", "yuv420p10le", "-g", "30"]);
        for (arq, cod) in [(x265("proxy-hevc-8.mp4", "yuv420p"), "h264_nvenc"), (x265("proxy-hevc-10.mp4", "yuv420p10le"), "h264_nvenc"), (h10, "libx264")] {
            let nome = arq.file_name().unwrap().to_string_lossy().into_owned();
            let f = fonte_de_teste(&cap, &arq, true);
            let cache = fontes::pasta_temporaria("proxy-hevc");
            let p = produzir(&g, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap_or_else(|e| panic!("{nome}: {e}"));
            let (args, motivo) = registro_do_proxy(&cache, &f.sha256);
            assert!(args.iter().any(|a| a == cod), "{nome}: {args:?} {motivo:?}");
            // `medir` só lê 8 bits: a fonte são os 150 primeiros quadros da matriz
            conferir_proxy(&cap, &f, &p, &cache, true, Some((0..150).collect()));
            pts_iguais(&cap, &f, &p, &cache, &nome);
        }
    }

    /// A matriz inteira pela GPU de verdade: o proxy que sai (da placa ou, na queda, da CPU) tem os quadros, a cor e a
    /// lista de pts da fonte. Giro, faixa cheia e pixel não quadrado nunca vão à placa.
    #[test]
    fn matriz_inteira_com_a_gpu_de_verdade() {
        let Some(cap) = fontes::cap() else { return };
        let Some(g) = gpu_real(&cap) else {
            eprintln!("aviso: sem NVENC com scale_cuda em {}: dispensado", cap.versao);
            return;
        };
        for &fonte in fontes::MATRIZ {
            let f = fonte_de_teste(&cap, &fontes::gerar(fonte).unwrap(), true);
            let cache = fontes::pasta_temporaria("proxy-matriz-gpu");
            let p = produzir(&g, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap_or_else(|e| panic!("{fonte:?}: {e}"));
            let (args, motivo) = registro_do_proxy(&cache, &f.sha256);
            let na_gpu = args.iter().any(|a| a == "h264_nvenc");
            eprintln!("{fonte:?}: {} ({motivo:?})", if na_gpu { "gpu" } else { "cpu" });
            if matches!(fonte, Fonte::Giro90 | Fonte::FaixaCheia | Fonte::PixelNaoQuadrado) {
                assert!(!na_gpu, "{fonte:?}: {args:?}");
            }
            if fonte == Fonte::FaixaCheia {
                // o proxy de CPU guarda a faixa cheia (`yuvj420p`), que `conferir_proxy` não espera
                let tocado = cache.join(&p.relativo);
                assert_eq!(medir::quadros_de_origem(&cap, &tocado).unwrap(), medir::quadros_de_origem(&cap, &f.caminho).unwrap(), "{fonte:?}");
            } else {
                conferir_proxy(&cap, &f, &p, &cache, true, None);
            }
            // pts repetido na fonte (MPEG-PS) sai 1 passo adiante no MP4: aí vale a folga de `conferir_proxy`
            if f.quadros.pts.windows(2).all(|w| w[0] < w[1]) {
                pts_iguais(&cap, &f, &p, &cache, &format!("{fonte:?}"));
            }
        }
    }

    /// O render lê só o original (§7.6): com cópia, proxy e um intermediário de origem prontos no cache da Edição, o
    /// plano da exportação final (`preparar` + `compilar`, como a fila) não cita nenhum deles e os quadros compostos
    /// são os do original, iguais aos de um cache vazio. O mesmo plano apontado para o proxy daria outros quadros: o
    /// teste enxerga a troca.
    #[test]
    fn render_nunca_le_proxy_nem_origem() {
        use crate::edicao::executar::{self, apoio};
        use crate::edicao::motor::{self, plano::Papel};
        use crate::edicao::testes::aceite;
        use crate::edicao::testes::executor::{self, Opcoes};
        let Some(cap) = fontes::cap() else { return };
        let (d, ws) = apoio::caso();
        let vm = tauri::async_runtime::block_on(crate::commands::video_commands::register_video_media(
            ws.to_string_lossy().into(),
            crate::models::RegisterVideoInput { source_path: fontes::gerar(Fonte::Mp4).unwrap().to_string_lossy().into(), allow_duplicate: false },
        ))
        .unwrap();
        let original = crate::workspace::paths::resolver_relativo(&ws, &vm.relative_path);
        let nao = AtomicBool::new(false);
        let f = fonte_de_teste(&cap, &original, true);
        assert_eq!(f.sha256, vm.sha256);
        // cache com cópia, proxy e intermediário de origem
        let cache = d.path().join("cache da edição");
        let copia = previa::copiar(&cap, &cache, &f, &nao, &mut sem_progresso()).unwrap();
        let px = produzir(&cap, &cache, &f, &nao, &mut sem_progresso()).unwrap();
        let px_arquivo = cache.join(&px.relativo);
        let origem = cache.join("origem").join(format!("{}.mp4", "e".repeat(64)));
        std::fs::create_dir_all(origem.parent().unwrap()).unwrap();
        std::fs::copy(&px_arquivo, &origem).unwrap();
        // crf 23 sobre a fonte sem perda: quadros diferentes (ler do proxy apareceria nos compostos)
        assert_ne!(hashes_por_quadro(&cap, &original), hashes_por_quadro(&cap, &px_arquivo), "{}", cap.versao);
        let mut m = aceite::midia("m1", "video", json!([]));
        m["sha256"] = json!(vm.sha256);
        m["relativo"] = json!(vm.relative_path);
        m["media_id"] = json!(vm.id.to_string());
        m["nome"] = json!(vm.filename);
        let p = aceite::projeto(320, 180, (30, 1), vec![m], vec![aceite::trecho("c1", "m1", &f.quadros, 30, 90, 1.0, json!([]))], json!({}));
        let plano_com = |cache: &Path| {
            let prep = executar::preparar_com(&ws, &p, cache, true, Some(&nao)).unwrap();
            assert_eq!(prep.contexto().fontes["m1"].caminho, original, "{}", cap.versao);
            motor::compilar(&p, &prep.contexto(), &aceite::alvo_final("cpu")).unwrap()
        };
        let plano = plano_com(&cache);
        let texto = serde_json::to_string(&plano).unwrap();
        for proibido in ["cache da edição", "proxies/", "origem/", copia.relativo.as_str(), px.relativo.as_str()] {
            assert!(!texto.contains(proibido), "o plano cita {proibido}");
        }
        let midias: Vec<&PathBuf> = plano.blocos.iter().flat_map(|b| &b.entradas).filter(|e| matches!(e.papel, Papel::Midia { .. })).map(|e| &e.caminho).collect();
        assert!(!midias.is_empty() && midias.iter().all(|c| **c == original), "{midias:?}");
        let rodar = |pl: &crate::edicao::motor::plano::Plano, nome: &str| {
            executor::rodar(&cap, pl, &fontes::pasta_temporaria(nome), &Opcoes { sem_perdas: true, ..Opcoes::default() }).unwrap_or_else(|e| panic!("{nome}: {e} ({})", cap.versao))
        };
        let r = rodar(&plano, "proxy-render");
        let lidos: Vec<u32> = r.blocos.iter().flat_map(|b| medir::quadros_de_origem(&cap, b).unwrap()).collect();
        assert_eq!(lidos, (30..90).collect::<Vec<u32>>(), "{}", cap.versao);
        // igual ao render com o cache vazio
        let vazio = d.path().join("cache vazio");
        assert_eq!(rodar(&plano_com(&vazio), "proxy-render-vazio").compostos, r.compostos, "{}", cap.versao);
        // o mesmo plano lendo o proxy: outros quadros
        let mut errado = plano.clone();
        for e in errado.blocos.iter_mut().flat_map(|b| b.entradas.iter_mut()).filter(|e| matches!(e.papel, Papel::Midia { .. })) {
            e.caminho = px_arquivo.clone();
        }
        assert_ne!(rodar(&errado, "proxy-render-errado").compostos, r.compostos, "{}", cap.versao);
    }

    // ------------------------------------------------------------ bancada WebKitGTK

    /// Processo da bancada: morto e esperado ao sair do escopo (só os PIDs deste teste).
    struct Filho(std::process::Child);

    impl Drop for Filho {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn copiar_pasta(de: &Path, para: &Path) {
        std::fs::create_dir_all(para).unwrap();
        for e in std::fs::read_dir(de).unwrap().flatten() {
            if e.path().is_file() {
                std::fs::copy(e.path(), para.join(e.file_name())).unwrap();
            }
        }
    }

    /// Tempo máximo da bancada inteira.
    const BANCADA_S: u64 = 300;

    /// Bancada do §11 (M9 da crítica do editor): 864×480 com GOP de 20 s (`ce/wk/media/v480_g600.mp4`), o proxy feito
    /// por `produzir` e o guiado do P5 (`t=seekplay`: vídeo pausado, um seek por quadro, 4 s a 1× e a −1×) no WebKitGTK
    /// de verdade, num `broadwayd` próprio (nenhuma janela na área de trabalho). Proxy: 30 de 30 quadros por segundo nos
    /// dois sentidos. O original, na mesma rodada, é a medida de controle (M9: 18,8 e 13,8). Com a máquina carregada,
    /// rodar como o M9 (`p5/scripts/prio.sh`: `systemd-run --user --scope -p CPUWeight=10000`).
    #[test]
    #[ignore = "bancada: WebKitGTK sem janela (broadwayd, python3-gi, WebKit2 4.1)"]
    fn guiado_com_proxy_30_de_30_nos_dois_sentidos() {
        let Some(cap) = fontes::cap() else { return };
        let Some(wk) = fontes::dossie().map(|d| d.join("ce/wk")).filter(|p| p.join("media/v480_g600.mp4").is_file()) else {
            eprintln!("dispensado: sem ce/wk/media/v480_g600.mp4 no dossiê");
            return;
        };
        let raiz = fontes::pasta_temporaria("proxy-bancada").join("wk");
        for sub in ["scripts", "page"] {
            copiar_pasta(&wk.join(sub), &raiz.join(sub));
        }
        for sub in ["media", "out"] {
            std::fs::create_dir_all(raiz.join(sub)).unwrap();
        }
        let fonte = wk.join("media/v480_g600.mp4");
        let f = fonte_de_teste(&cap, &fonte, true);
        let cache = fontes::pasta_temporaria("proxy-bancada-cache");
        let p = produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        pts_iguais(&cap, &f, &p, &cache, "v480_g600");
        std::fs::copy(cache.join(&p.relativo), raiz.join("media/proxy.mp4")).unwrap();
        std::fs::copy(&fonte, raiz.join("media/original.mp4")).unwrap();
        let lista = raiz.join("lista.json");
        std::fs::write(&lista, json!(["t=seekplay&f=proxy.mp4&vels=1,-1", "t=seekplay&f=original.mp4&vels=1,-1"]).to_string()).unwrap();
        // tela própria: porta e soquete livres
        let soquetes = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let n = (40u16..100)
            .find(|n| std::net::TcpListener::bind(("127.0.0.1", 8900 + n)).is_ok() && !soquetes.join(format!("broadway{n}.socket")).exists())
            .expect("tela livre");
        let Ok(tela) = Command::new("broadwayd")
            .args(["--address", "127.0.0.1", "--port", &(8900 + n).to_string(), &format!(":{n}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            eprintln!("dispensado: sem broadwayd");
            return;
        };
        let _tela = Filho(tela);
        std::thread::sleep(Duration::from_millis(800));
        let mut pagina = Filho(
            Command::new("python3")
                .arg("-I")
                .arg(raiz.join("scripts/run-webkit.py"))
                .arg("p19")
                .arg(&lista)
                .arg("--janela")
                .env("GDK_BACKEND", "broadway")
                .env("BROADWAY_DISPLAY", format!(":{n}"))
                .env_remove("WAYLAND_DISPLAY")
                .env_remove("DISPLAY")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("python3"),
        );
        let t0 = Instant::now();
        let st = loop {
            if let Some(st) = pagina.0.try_wait().unwrap() {
                break st;
            }
            assert!(t0.elapsed().as_secs() < BANCADA_S, "bancada sem fim");
            std::thread::sleep(Duration::from_millis(100));
        };
        assert!(st.success(), "run-webkit.py: {st:?}");
        let saida: Value = serde_json::from_slice(&std::fs::read(raiz.join("out/webkit-p19.json")).unwrap()).unwrap();
        eprintln!("WebKitGTK {} ({}), carga {} → {}", saida["webkit"], cap.versao, saida["carga_inicio"], saida["carga_fim"]);
        let mut medido = std::collections::HashMap::new();
        for t in saida["testes"].as_array().unwrap() {
            let arq = t["res"]["arquivo"].as_str().unwrap_or_else(|| panic!("{t}")).to_string();
            for vel in ["1", "-1"] {
                let m = &t["res"]["modos"][vel];
                let (vistos, ideal) = (m["quadros_distintos"].as_u64().unwrap(), m["ideal"].as_u64().unwrap());
                eprintln!("{arq} {vel}×: {vistos} de {ideal} quadros ({} por s)", m["distintos_por_s"]);
                medido.insert((arq.clone(), vel), (vistos, ideal));
            }
        }
        for vel in ["1", "-1"] {
            let (vistos, ideal) = medido[&("proxy.mp4".to_string(), vel)];
            assert!(ideal == 120 && vistos >= ideal, "proxy a {vel}×: {vistos} de {ideal}");
        }
    }
}
