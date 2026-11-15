//! Testes da corrida (P2.2): leitor por pipe nos dois ffmpeg (`SICRO_FFMPEG`; `SICRO_EXIGE_FFMPEG=1` não deixa
//! dispensar), cancelamento, eventos e o fluxo inteiro com a mancha (marcar → seguir → denso → ocultar).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::*;
use crate::edicao::ffmpeg::sonda::{self, Capacidades};
use crate::edicao::modelo::Interp;
use crate::edicao::quadros::{self, apoio};
use crate::edicao::rastreio::{ESTADO_AUSENTE, ESTADO_OK};
use crate::edicao::testes::aceite;
use crate::edicao::testes::fontes::{self, MATRIZ};

/// Limites reais; os testes medem com folga de 3× enquanto houver outros agentes na máquina (O1).
const LIMITE_CANCELAR_S: f64 = 0.5;
const FOLGA: f64 = 3.0;

/// Uma corrida por processo: os testes que pedem corrida passam um de cada vez.
static VEZ_TESTE: Mutex<()> = Mutex::new(());

fn vez() -> std::sync::MutexGuard<'static, ()> {
    VEZ_TESTE.lock().unwrap_or_else(|e| e.into_inner())
}

// ------------------------------------------------------------------ vídeo de teste

const BITS: u32 = 16;
const BLOCO: (u32, u32) = (48, 72);

fn ruido(x: u32, y: u32, sem: u32) -> u8 {
    let mut h = x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263) ^ sem.wrapping_mul(2246822519);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    ((h ^ (h >> 16)) & 255) as u8
}

/// Vai e volta entre 0 e `amp`.
fn tri(k: u32, amp: u32) -> u32 {
    let p = k % (2 * amp);
    if p < amp { p } else { 2 * amp - p }
}

/// Canto do bloco texturizado no quadro k (None = sumiu): anda 2 px por quadro, indo e voltando.
fn bloco_em(k: u32, w: u32, ate: u32) -> Option<(u32, u32)> {
    (k < ate).then(|| (20 + tri(2 * k, w - BLOCO.0 - 40), 40 + tri(k, 16)))
}

/// Vídeo `w`×`h` a 30 qps: fundo texturizado fixo, bloco que anda (até o quadro `some`) e, nas 16 linhas de cima,
/// o número do quadro em 16 faixas pretas e brancas. `extra` entra antes da saída (`-output_ts_offset 3`, …).
fn video(nome: &str, n: u32, w: u32, h: u32, some: u32, extra: &[&str]) -> Option<(Arc<Capacidades>, PathBuf)> {
    let cap = fontes::cap()?;
    let pasta = fontes::pasta_temporaria("p22-video");
    let saida = pasta.join(nome);
    let mut fundo = vec![0u8; (w * h * 3) as usize];
    for y in 0..h {
        for x in 0..w {
            let c = 50 + ruido(x / 6, y / 6, 1) / 6;
            fundo[((y * w + x) * 3) as usize..][..3].copy_from_slice(&[c, c, c / 2]);
        }
    }
    let textura: Vec<u8> = (0..BLOCO.1).flat_map(|y| (0..BLOCO.0).flat_map(move |x| {
        let c = ruido(x, y, 7);
        [c, 255 - c, c / 3]
    })).collect();
    let mut c = std::process::Command::new(&cap.binario);
    c.args(["-hide_banner", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{w}x{h}"), "-r", "30", "-i", "-"])
        .args(["-c:v", "libx264", "-preset", "ultrafast", "-qp", "0", "-pix_fmt", "yuv420p", "-g", "30", "-sc_threshold", "0", "-bf", "0"])
        .args(extra)
        .arg("-y")
        .arg(&saida)
        .stdin(Stdio::piped());
    let mut filho = c.spawn().expect("ffmpeg");
    let mut entrada = filho.stdin.take().unwrap();
    let mut q = fundo.clone();
    for k in 0..n {
        q.copy_from_slice(&fundo);
        if let Some((bx, by)) = bloco_em(k, w, some) {
            for y in 0..BLOCO.1 {
                let o = (((by + y) * w + bx) * 3) as usize;
                q[o..o + (BLOCO.0 * 3) as usize].copy_from_slice(&textura[(y * BLOCO.0 * 3) as usize..((y + 1) * BLOCO.0 * 3) as usize]);
            }
        }
        for y in 0..16 {
            for b in 0..BITS {
                let v = if k >> b & 1 == 1 { 255 } else { 0 };
                let (x0, x1) = (b * w / BITS, (b + 1) * w / BITS);
                q[((y * w + x0) * 3) as usize..((y * w + x1) * 3) as usize].fill(v);
            }
        }
        entrada.write_all(&q).unwrap();
    }
    drop(entrada);
    assert!(filho.wait().unwrap().success(), "gerar {nome}");
    Some((cap, saida))
}

/// Número do quadro lido das 16 faixas de cima.
fn numero(q: &[u8], w: u32) -> u32 {
    (0..BITS)
        .map(|b| {
            let x = b * w / BITS + w / (2 * BITS);
            let p = &q[((8 * w + x) * 3) as usize..][..3];
            u32::from((p[0] as u32 + p[1] as u32 + p[2] as u32) / 3 > 128) << b
        })
        .sum()
}

/// Luma (0..255) de um quadro RGB, para o `identificar` das fontes da matriz.
fn luma(q: &[u8]) -> Vec<u8> {
    q.chunks_exact(3).map(|p| ((p[0] as u32 * 54 + p[1] as u32 * 183 + p[2] as u32 * 19) >> 8) as u8).collect()
}

fn fonte_de(cap: &Capacidades, video: &Path) -> FonteInfo {
    let tab = TabelaQuadros::do_decodificador(cap, video, &AtomicBool::new(false), &mut |_| {}).expect("tabela");
    quadros::fonte_info(cap, video, &"0".repeat(64), Arc::new(tab), None).expect("fonte_info")
}

fn pixel(f: &FonteInfo) -> TamanhoPx {
    TamanhoPx { w: f.exibido.0, h: f.exibido.1 }
}

/// Todos os quadros de um leitor: (pts, RGB).
fn ler_todos(l: &mut LeitorPipe) -> Vec<(i64, Vec<u8>)> {
    let mut v = Vec::new();
    while let Some((p, q)) = l.proximo().expect("quadro") {
        v.push((p, q.rgb.to_vec()));
    }
    v
}

// ------------------------------------------------------------------ leitor por pipe

#[test]
fn showinfo_e_miniatura() {
    assert_eq!(pts_do_showinfo("[showinfo@rq @ 0x5581] n:   3 pts:  90090 pts_time:1.001 duration:3003"), Some(Ok(90090)));
    assert_eq!(pts_do_showinfo("[showinfo@rq @ 0x5581] n:   3 pts:NOPTS"), Some(Err(())));
    assert_eq!(pts_do_showinfo("[showinfo@rq @ 0x5581] n:   3 pts: NOPTS pts_time:"), Some(Err(())));
    assert_eq!(pts_do_showinfo("[showinfo@rq @ 0x5581] color_range:tv"), None);
    assert_eq!(pts_do_showinfo("[showinfo@outro @ 0x5581] n: 0 pts: 5"), None);
    assert_eq!(pts_do_showinfo("Stream #0:0: Video: n: 0 pts: 5"), None);
    // 1920×1080 vira 320×180, com a caixa na cor do rastreio
    let (w, h) = (1920u32, 1080u32);
    let rgb = vec![30u8; (w * h * 3) as usize];
    let m = miniatura(&Imagem { rgb: &rgb, w, h }, Some(Caixa { x: 0.25, y: 0.25, w: 0.5, h: 0.5 }), [255, 0, 0]).unwrap();
    let b = base64::engine::general_purpose::STANDARD.decode(m.strip_prefix("data:image/jpeg;base64,").unwrap()).unwrap();
    let img = image::load_from_memory_with_format(&b, image::ImageFormat::Jpeg).unwrap().to_rgb8();
    assert_eq!(img.dimensions(), (320, 180));
    let borda = img.get_pixel(160, 45);
    let dentro = img.get_pixel(160, 90);
    assert!(borda[0] > 180 && borda[1] < 90, "{borda:?}");
    assert!(dentro[0] < 60, "{dentro:?}");
    // pequena fica do tamanho dela; retrato também respeita o lado maior
    let rgb = vec![0u8; 100 * 50 * 3];
    let b = miniatura(&Imagem { rgb: &rgb, w: 100, h: 50 }, None, [0, 0, 0]).unwrap();
    let b = base64::engine::general_purpose::STANDARD.decode(b.split(',').nth(1).unwrap()).unwrap();
    assert_eq!(image::load_from_memory(&b).unwrap().to_rgb8().dimensions(), (100, 50));
    let rgb = vec![0u8; 720 * 1280 * 3];
    let b = miniatura(&Imagem { rgb: &rgb, w: 720, h: 1280 }, None, [0, 0, 0]).unwrap();
    let b = base64::engine::general_purpose::STANDARD.decode(b.split(',').nth(1).unwrap()).unwrap();
    assert_eq!(image::load_from_memory(&b).unwrap().to_rgb8().dimensions(), (180, 320));
}

/// Arquivo que começa em 3 s (MP4 e TS): o leitor parte do quadro pedido, com e sem busca, e lê os pts da tabela.
/// Na matriz de fontes, o mesmo em todas (início ≠ 0, TS no meio do GOP, PS, lista de edição, quadros B, taxa
/// variável, giro, pixel 10:11…): pts lidos = pts da tabela e o número desenhado = o do quadro.
#[test]
fn inicio_em_3_s_parte_do_quadro_certo() {
    for (nome, extra) in [("tres.mp4", vec!["-output_ts_offset", "3"]), ("tres.ts", vec!["-output_ts_offset", "3", "-muxdelay", "0", "-muxpreload", "0", "-f", "mpegts"])] {
        let Some((cap, v)) = video(nome, 600, 320, 240, 600, &extra) else { return };
        let f = fonte_de(&cap, &v);
        let tab = &f.quadros;
        assert!((tab.tempo(0) - 3.0).abs() < 0.05 && f.inicio_s > 2.9, "{nome}: início {} / {}", tab.tempo(0), f.inicio_s);
        let n = tab.pts.len();
        assert_eq!(n, 600, "{nome}");
        let mut com_busca = 0;
        for j in [0usize, 1, 57, 89, 328, 330, 401, 595] {
            com_busca += usize::from(tab.busca_em(f.inicio_s, j).is_some());
            let fim = (j + 4).min(n - 1);
            let mut l = LeitorPipe::abrir(&cap, &f, tab.pts[j], Some(tab.pts[fim]), Sentido::Frente, pixel(&f)).unwrap();
            let lidos = ler_todos(&mut l);
            assert_eq!(lidos.iter().map(|x| x.0).collect::<Vec<_>>(), tab.pts[j..=fim].to_vec(), "{nome} quadro {j} ({})", cap.versao);
            assert_eq!(lidos.iter().map(|x| numero(&x.1, 320)).collect::<Vec<_>>(), (j as u32..=fim as u32).collect::<Vec<_>>(), "{nome} quadro {j} ({})", cap.versao);
        }
        assert!(com_busca >= 3, "{nome}: a busca do §5.3 tem de entrar ({com_busca})");
    }
    let Some(cap) = fontes::cap() else { return };
    let mut repetidos = 0;
    for &f in MATRIZ {
        let m = apoio::medida(&cap, f);
        let tab = &m.info.quadros;
        let n = m.limite.min(tab.pts.len());
        // perto do começo, 2 antes de um quadro-chave depois dos 10 s (com busca), meio e fim
        let chave = (0..n).rev().find(|&j| tab.chave[j] && tab.busca_em(m.info.inicio_s, j).is_some()).unwrap_or(n / 2);
        for j in [2usize, chave.saturating_sub(2), n / 2, n - 3] {
            let fim = j + 2;
            // pts repetido: o leitor dá um quadro por pts, o último do grupo (o exibido)
            let ultimo = tab.pts.partition_point(|&p| p <= tab.pts[fim]) - 1;
            let esperados: Vec<usize> = (j..=ultimo).filter(|&k| k == ultimo || tab.pts[k + 1] != tab.pts[k]).collect();
            let mut l = LeitorPipe::abrir(&cap, &m.info, tab.pts[j], Some(tab.pts[fim]), Sentido::Frente, pixel(&m.info)).unwrap();
            let lidos = ler_todos(&mut l);
            assert_eq!(lidos.iter().map(|x| x.0).collect::<Vec<_>>(), esperados.iter().map(|&k| tab.pts[k]).collect::<Vec<_>>(), "{f:?} quadro {j} ({})", cap.versao);
            let (w, h) = m.info.exibido;
            let nums: Vec<u32> = lidos.iter().map(|x| crate::edicao::testes::medir::identificar(&luma(&x.1), w as usize, h as usize)).collect();
            assert_eq!(nums, esperados.iter().map(|&k| m.numeros[k]).collect::<Vec<_>>(), "{f:?} quadro {j} ({})", cap.versao);
            repetidos += usize::from(esperados.len() < ultimo + 1 - j);
            // para trás, o mesmo trecho ao contrário
            let mut l = LeitorPipe::abrir(&cap, &m.info, tab.pts[ultimo], Some(tab.pts[j]), Sentido::Tras, pixel(&m.info)).unwrap();
            let tras: Vec<i64> = ler_todos(&mut l).iter().map(|x| x.0).collect();
            assert_eq!(tras, esperados.iter().rev().map(|&k| tab.pts[k]).collect::<Vec<_>>(), "{f:?} para trás {j} ({})", cap.versao);
        }
    }
    eprintln!("trechos com pts repetido: {repetidos} ({})", cap.versao);
}

/// Amostras reais de gravador (Dahua, Hikvision, IFV, LVF): o quadro lido com a busca (e dos 3 primeiros segundos)
/// é o mesmo, byte a byte, do lido em sequência desde o começo; pts = tabela.
#[test]
fn gravador_parte_do_quadro_certo() {
    let Some(cap) = fontes::cap() else { return };
    let amostras = fontes::amostras_de_gravador();
    if amostras.is_empty() {
        assert!(std::env::var("SICRO_EXIGE_FFMPEG").map_or(true, |v| v != "1"), "amostras de gravador ausentes");
        eprintln!("aviso: amostras de gravador ausentes; teste dispensado");
        return;
    }
    let mut testadas = 0;
    for v in amostras {
        let Ok(m) = apoio::medir_por_conteudo(&cap, &v) else {
            eprintln!("{}: não decodifica com {}", v.display(), cap.versao);
            continue;
        };
        let tab = &m.info.quadros;
        let n = m.limite.min(tab.pts.len());
        if n < 20 {
            continue;
        }
        // quadro reduzido (o mesmo nas duas leituras): o pipe de 2592×1520 em sequência levaria minutos
        let (w0, h0) = m.info.exibido;
        let w = w0.min(480) / 2 * 2;
        let px = TamanhoPx { w, h: ((h0 as f64 * w as f64 / w0 as f64 / 2.0).round() as u32 * 2).max(2) };
        // quadros a conferir: dos 3 primeiros segundos, 2 antes de um quadro-chave com busca, meio
        let teto = n.min(900);
        let chave = (0..teto).rev().find(|&j| tab.chave[j] && tab.busca_em(m.info.inicio_s, j).is_some());
        let mut alvos = vec![2usize, teto / 2];
        alvos.extend(chave.map(|c| c - 2));
        let ate = alvos.iter().max().unwrap() + 2;
        let mut referencia = std::collections::HashMap::new();
        let mut l = LeitorPipe::abrir(&cap, &m.info, tab.pts[0], Some(tab.pts[ate]), Sentido::Frente, px).unwrap();
        let mut j = 0usize;
        while let Some((p, q)) = l.proximo().unwrap() {
            assert_eq!(p, tab.pts[j], "{} sequencial quadro {j}", v.display());
            if alvos.iter().any(|&a| j >= a && j <= a + 2) {
                referencia.insert(j, q.rgb.to_vec());
            }
            j += 1;
        }
        assert_eq!(j, ate + 1, "{}", v.display());
        for &a in &alvos {
            let mut l = LeitorPipe::abrir(&cap, &m.info, tab.pts[a], Some(tab.pts[a + 2]), Sentido::Frente, px).unwrap();
            let lidos = ler_todos(&mut l);
            assert_eq!(lidos.iter().map(|x| x.0).collect::<Vec<_>>(), tab.pts[a..=a + 2].to_vec(), "{} quadro {a} ({})", v.display(), cap.versao);
            for (k, (_, q)) in lidos.iter().enumerate() {
                assert!(q == &referencia[&(a + k)], "{} quadro {} difere do sequencial ({})", v.display(), a + k, cap.versao);
            }
        }
        testadas += 1;
    }
    assert!(testadas >= 3, "{testadas} amostras");
}

#[test]
fn para_tras_em_blocos() {
    let Some((cap, v)) = video("tras.mp4", 300, 320, 240, 300, &[]) else { return };
    let f = fonte_de(&cap, &v);
    let tab = &f.quadros;
    let mut l = LeitorPipe::abrir(&cap, &f, tab.pts[280], Some(tab.pts[31]), Sentido::Tras, pixel(&f)).unwrap();
    let lidos = ler_todos(&mut l);
    assert_eq!(lidos.iter().map(|x| x.0).collect::<Vec<_>>(), (31..=280).rev().map(|j| tab.pts[j]).collect::<Vec<_>>(), "{}", cap.versao);
    assert_eq!(lidos.iter().map(|x| numero(&x.1, 320)).collect::<Vec<_>>(), (31..=280).rev().collect::<Vec<u32>>());
    assert_eq!(l.processos(), 3, "250 quadros em blocos de 100");
    // até a borda, e um quadro só
    let mut l = LeitorPipe::abrir(&cap, &f, tab.pts[5], None, Sentido::Tras, pixel(&f)).unwrap();
    assert_eq!(ler_todos(&mut l).iter().map(|x| numero(&x.1, 320)).collect::<Vec<_>>(), (0..=5).rev().collect::<Vec<u32>>());
    let mut l = LeitorPipe::abrir(&cap, &f, tab.pts[7], Some(tab.pts[7]), Sentido::Tras, pixel(&f)).unwrap();
    assert_eq!(ler_todos(&mut l).len(), 1);
    // memória: em 4K o bloco cai para 20 quadros
    assert_eq!(MEMORIA_BLOCO / (3840 * 2160 * 3), 21);
    // pedidos fora da tabela ou invertidos
    assert!(LeitorPipe::abrir(&cap, &f, tab.pts[5] + 1, None, Sentido::Frente, pixel(&f)).is_err());
    assert!(LeitorPipe::abrir(&cap, &f, tab.pts[5], Some(tab.pts[9]), Sentido::Tras, pixel(&f)).is_err());
    assert!(LeitorPipe::abrir(&cap, &f, tab.pts[9], Some(tab.pts[5]), Sentido::Frente, pixel(&f)).is_err());
    assert!(LeitorPipe::abrir(&cap, &f, tab.pts[5], None, Sentido::Frente, TamanhoPx { w: 0, h: 10 }).is_err());
    // fechar no meio mata o ffmpeg e a thread acaba
    let mut l = LeitorPipe::abrir(&cap, &f, tab.pts[0], None, Sentido::Frente, pixel(&f)).unwrap();
    let _ = l.proximo().unwrap();
    let t = Instant::now();
    drop(l);
    assert!(t.elapsed().as_secs_f64() < LIMITE_CANCELAR_S * FOLGA, "{:?}", t.elapsed());
}

// ------------------------------------------------------------------ corrida pela tela

/// Caso com o vídeo em `Midias/`, o projeto com o rastreio `r1` (âncora no quadro `j0`, na caixa do bloco) e o
/// trecho `c1` inteiro.
struct Caso {
    ws: tempfile::TempDir,
    cache: tempfile::TempDir,
    cap: Arc<Capacidades>,
    fonte: FonteInfo,
    projeto: Value,
    w: u32,
    h: u32,
}

fn caso(n: u32, w: u32, h: u32, some: u32, j0: usize) -> Option<Caso> {
    let (cap, v) = video("caso.mp4", n, w, h, some, &[])?;
    // o ffmpeg das capacidades globais (o que `preparar` usa) é o mesmo dos testes
    assert_eq!(sonda::capacidades().unwrap().sha256, cap.sha256);
    let ws = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(ws.path().join("Midias")).unwrap();
    std::fs::copy(&v, ws.path().join("Midias/v.mp4")).unwrap();
    let sha = sonda::sha256_arquivo(&v).unwrap();
    let fonte = fonte_de(&cap, &v);
    let tab = &fonte.quadros;
    let (bx, by) = bloco_em(j0 as u32, w, some).unwrap();
    let ancora = aceite::ancora(tab.tempo(j0), Some(tab.pts[j0]), [bx as f64 / w as f64, by as f64 / h as f64, BLOCO.0 as f64 / w as f64, BLOCO.1 as f64 / h as f64]);
    let mut m = aceite::midia("m1", "video", json!([]));
    (m["sha256"], m["relativo"]) = (json!(sha), json!("Midias/v.mp4"));
    let p = aceite::projeto(w, h, (30, 1), vec![m], vec![aceite::trecho("c1", "m1", tab, 0, tab.pts.len(), 1.0, json!([]))], json!({ "rastreios": [aceite::rastreio("r1", Some("m1"), "fonte", vec![ancora])] }));
    let projeto = serde_json::to_value(&p).unwrap();
    Some(Caso { ws, cache: tempfile::tempdir().unwrap(), cap, fonte, projeto, w, h })
}

impl Caso {
    fn pedido(&self, sentido: SentidoPedido) -> PedidoSeguir {
        let a: Chave<Caixa> = serde_json::from_value(self.projeto["rastreios"][0]["ancoras"][0].clone()).unwrap();
        PedidoSeguir { rastreio_id: "r1".into(), clipe_id: "c1".into(), ancora: a, sentido, ate_s: None, motor: EscolhaMotor::Auto }
    }

    fn pasta(&self) -> PathBuf {
        armazem::pasta_projeto(self.ws.path(), "p-teste")
    }

    /// Âncora (como o front grava) no quadro j, na caixa do bloco.
    fn ancora_em(&self, j: usize, some: u32) -> Value {
        let tab = &self.fonte.quadros;
        let (bx, by) = bloco_em(j as u32, self.w, some).unwrap();
        aceite::ancora(tab.tempo(j), Some(tab.pts[j]), [bx as f64 / self.w as f64, by as f64 / self.h as f64, BLOCO.0 as f64 / self.w as f64, BLOCO.1 as f64 / self.h as f64])
    }

    /// Pede e espera o fim (falha = pânico).
    fn seguir(&self, ancora: &Value, sentido: SentidoPedido, ate_s: Option<f64>) -> FimRastreio {
        let (ev, emitir) = coletor();
        let p = PedidoSeguir { ancora: serde_json::from_value(ancora.clone()).unwrap(), ate_s, ..self.pedido(sentido) };
        pedir(self.ws.path(), self.cache.path(), &ai_padrao(), &self.projeto, &p, emitir).unwrap();
        match esperar_fim(&ev, 600).1 {
            EventoRastreio::Fim(f) => f,
            e => panic!("{e:?}"),
        }
    }
}

type Eventos = Arc<Mutex<Vec<(Instant, EventoRastreio)>>>;

fn coletor() -> (Eventos, Emissor) {
    let ev: Eventos = Arc::new(Mutex::new(Vec::new()));
    let e2 = ev.clone();
    (ev, Box::new(move |e| e2.lock().unwrap().push((Instant::now(), e))))
}

/// Espera o evento final (fim ou falha) por até `s` segundos.
fn esperar_fim(ev: &Eventos, s: u64) -> (Instant, EventoRastreio) {
    let t = Instant::now();
    loop {
        if let Some(x) = ev.lock().unwrap().iter().find(|(_, e)| !matches!(e, EventoRastreio::Avanco(_))) {
            return x.clone();
        }
        assert!(t.elapsed() < Duration::from_secs(s), "a corrida não acabou em {s} s");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn ai_padrao() -> AiSettings {
    AiSettings::default()
}

#[test]
fn escolher_pela_regra_do_auto() {
    let mut ai = ai_padrao();
    // sem GPU pronta nem onnxruntime configurado: a mancha
    assert_eq!(escolher(&ai, EscolhaMotor::Auto).unwrap().corrida().motor, crate::edicao::rastreio::MotorRastreio::Mancha);
    assert!(escolher(&ai, EscolhaMotor::Edgetam).err().unwrap().to_string().contains("modelo_ausente: edgetam"));
    ai.modelos_desligados = true;
    for m in [EscolhaMotor::Edgetam, EscolhaMotor::Vittrack] {
        assert!(escolher(&ai, m).err().unwrap().to_string().contains(codigo::IA_DESLIGADA), "{m:?}");
    }
    // a mancha segue com o interruptor desligado, pedida ou pelo auto
    for m in [EscolhaMotor::Mancha, EscolhaMotor::Auto] {
        let r = escolher(&ai, m).unwrap();
        assert_eq!((r.laco(), r.corrida().motor), (crate::edicao::rastreio::Laco::Simples, crate::edicao::rastreio::MotorRastreio::Mancha));
    }
}

/// Cancelar em menos de 0,5 s (do pedido ao evento final, com o ffmpeg morto); uma corrida por vez; o que foi seguido
/// até ali vira denso.
#[test]
fn cancelar_em_menos_de_meio_segundo() {
    let _v = vez();
    let Some(c) = caso(1200, 320, 240, 1200, 3) else { return };
    let (ev, emitir) = coletor();
    let tarefa = pedir(c.ws.path(), c.cache.path(), &ai_padrao(), &c.projeto, &c.pedido(SentidoPedido::Frente), emitir).unwrap();
    assert_eq!(em_curso().map(|x| x.0), Some(tarefa.clone()));
    // a segunda é recusada enquanto a primeira corre
    let (_, outro) = coletor();
    let e = pedir(c.ws.path(), c.cache.path(), &ai_padrao(), &c.projeto, &c.pedido(SentidoPedido::Frente), outro).unwrap_err();
    assert!(e.to_string().contains(codigo::RASTREIO_EM_CURSO), "{e}");
    // espera a corrida andar
    let t = Instant::now();
    while ev.lock().unwrap().iter().filter(|(_, e)| matches!(e, EventoRastreio::Avanco(a) if a.feitos >= 10)).count() == 0 {
        assert!(t.elapsed() < Duration::from_secs(120), "a corrida não andou");
        assert!(!ev.lock().unwrap().iter().any(|(_, e)| matches!(e, EventoRastreio::Falhou(_))), "{:?}", ev.lock().unwrap().last());
        std::thread::sleep(Duration::from_millis(5));
    }
    let pedido_em = Instant::now();
    cancelar(&tarefa).unwrap();
    let (fim_em, fim) = esperar_fim(&ev, 30);
    let gasto = fim_em.duration_since(pedido_em).as_secs_f64();
    assert!(gasto < LIMITE_CANCELAR_S * FOLGA, "cancelar levou {gasto:.3} s");
    eprintln!("cancelar: {gasto:.3} s ({})", c.cap.versao);
    assert!(em_curso().is_none());
    let EventoRastreio::Fim(fim) = fim else { panic!("{fim:?}") };
    assert_eq!((fim.parou.len(), fim.parou[0].motivo), (1, Motivo::Cancelado));
    let d = arquivo::ler(&c.pasta(), "r1", &fim.denso.as_ref().expect("o que foi seguido vira denso").sha256).unwrap();
    assert!(d.pts.len() > 10 && d.pts.len() < 1200, "{}", d.pts.len());
    assert!(cancelar(&tarefa).unwrap_err().to_string().contains(codigo::TAREFA_AUSENTE));
    // cancelada logo ao pedir (ainda preparando ou antes do quadro da âncora): fim sem nada novo
    let (ev, emitir) = coletor();
    let tarefa = pedir(c.ws.path(), c.cache.path(), &ai_padrao(), &c.projeto, &c.pedido(SentidoPedido::Ambos), emitir).unwrap();
    cancelar(&tarefa).unwrap();
    let (_, fim) = esperar_fim(&ev, 30);
    let EventoRastreio::Fim(fim) = fim else { panic!("{fim:?}") };
    assert_eq!(fim.parou.iter().map(|p| (p.sentido, p.motivo)).collect::<Vec<_>>(), vec![(Sentido::Frente, Motivo::Cancelado), (Sentido::Tras, Motivo::Cancelado)]);
    if fim.denso.is_none() {
        assert!(fim.silhuetas.is_none());
    }
    assert!(em_curso().is_none());
    // pedido inválido: na hora, sem tarefa
    let mut ruim = c.pedido(SentidoPedido::Frente);
    ruim.rastreio_id = "../r1".into();
    assert!(pedir(c.ws.path(), c.cache.path(), &ai_padrao(), &c.projeto, &ruim, coletor().1).unwrap_err().to_string().contains(codigo::ITEM_INVALIDO));
    ruim.rastreio_id = "r9".into();
    assert!(pedir(c.ws.path(), c.cache.path(), &ai_padrao(), &c.projeto, &ruim, coletor().1).unwrap_err().to_string().contains(codigo::RASTREIO_AUSENTE));
    // âncora com número não finito (pedido montado no Rust: o JSON não traz NaN)
    for (x, w) in [(f64::NAN, 0.1), (0.1, f64::INFINITY), (f64::NEG_INFINITY, 0.1)] {
        let mut ruim = c.pedido(SentidoPedido::Frente);
        (ruim.ancora.v.x, ruim.ancora.v.w) = (x, w);
        assert!(pedir(c.ws.path(), c.cache.path(), &ai_padrao(), &c.projeto, &ruim, coletor().1).unwrap_err().to_string().contains(codigo::REGIAO), "{x} {w}");
    }
    assert!(em_curso().is_none());
}

/// Caixa (px) do denso no quadro j.
fn caixa_do_denso(d: &ArquivoDenso, tab: &TabelaQuadros, j: usize, w: u32, h: u32) -> Option<[f64; 4]> {
    let i = d.pts.binary_search(&tab.pts[j]).ok()?;
    (d.estado[i] != ESTADO_AUSENTE).then(|| [d.x[i] * w as f64, d.y[i] * h as f64, d.w[i] * w as f64, d.h[i] * h as f64])
}

fn iou(a: [f64; 4], b: [f64; 4]) -> f64 {
    let ix = ((a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0])).max(0.0);
    let iy = ((a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1])).max(0.0);
    let i = ix * iy;
    i / (a[2] * a[3] + b[2] * b[3] - i)
}

/// O fluxo inteiro sem modelo nenhum (a mancha): marcar → seguir para a frente e para trás (`ambos`) → denso →
/// avaliação → validação (sem cobertura depois que o bloco some; a ausência aceita não bloqueia) → ocultar no render,
/// com 0 pixel do bloco à vista. Eventos até 5 por segundo, com a miniatura de até 320 px.
#[test]
fn ponta_a_ponta_com_a_mancha() {
    let _v = vez();
    let (n, some, j0) = (240u32, 200u32, 100usize);
    let Some(mut c) = caso(n, 640, 360, some, j0) else { return };
    let (ev, emitir) = coletor();
    let inicio = Instant::now();
    pedir(c.ws.path(), c.cache.path(), &ai_padrao(), &c.projeto, &c.pedido(SentidoPedido::Ambos), emitir).unwrap();
    let (_, fim) = esperar_fim(&ev, 600);
    let duracao = inicio.elapsed();
    let EventoRastreio::Fim(fim) = fim else { panic!("{fim:?}") };
    let tab = c.fonte.quadros.clone();
    // para a frente perdeu onde o bloco some; para trás foi até o começo
    assert_eq!(fim.parou, vec![
        ParouSentido { sentido: Sentido::Frente, motivo: Motivo::Perdido, pts: Some(tab.pts[some as usize]) },
        ParouSentido { sentido: Sentido::Tras, motivo: Motivo::Fim, pts: Some(tab.pts[0]) },
    ]);
    let ptr = fim.denso.clone().unwrap();
    assert!(fim.silhuetas.is_none(), "a mancha não dá silhueta");
    let d = arquivo::ler(&c.pasta(), "r1", &ptr.sha256).unwrap();
    assert_eq!((ptr.n, ptr.ok + ptr.incertos, ptr.ausentes), (some, some, 0));
    assert_eq!(arquivo::corridas(&d).unwrap().iter().map(|x| (x.sentido, x.parou)).collect::<Vec<_>>(), vec![(Sentido::Frente, Motivo::Perdido), (Sentido::Tras, Motivo::Fim)]);
    for j in 0..some as usize {
        let (bx, by) = bloco_em(j as u32, c.w, some).unwrap();
        let caixa = caixa_do_denso(&d, &tab, j, c.w, c.h).unwrap_or_else(|| panic!("quadro {j} sem caixa"));
        let i = iou(caixa, [bx as f64, by as f64, BLOCO.0 as f64, BLOCO.1 as f64]);
        assert!(i > 0.9, "quadro {j}: IoU {i:.3} ({caixa:?})");
    }
    // eventos: até 5 em qualquer janela de 1 s, com a miniatura do quadro
    let ev = ev.lock().unwrap().clone();
    let avancos: Vec<(Instant, AvancoRastreio)> = ev.iter().filter_map(|(t, e)| match e {
        EventoRastreio::Avanco(a) => Some((*t, a.clone())),
        _ => None,
    }).collect();
    assert!(!avancos.is_empty());
    for (i, (t, _)) in avancos.iter().enumerate() {
        let na_janela = avancos[i..].iter().take_while(|(u, _)| u.duration_since(*t) < Duration::from_secs(1)).count();
        assert!(na_janela <= AVANCOS_POR_S as usize, "{na_janela} avanços em 1 s");
    }
    if duracao > Duration::from_secs(2) {
        assert!(avancos.len() >= 2, "{} avanços em {duracao:?}", avancos.len());
    }
    for (_, a) in &avancos {
        let b = base64::engine::general_purpose::STANDARD.decode(a.miniatura.as_ref().unwrap().strip_prefix("data:image/jpeg;base64,").unwrap()).unwrap();
        let img = image::load_from_memory_with_format(&b, image::ImageFormat::Jpeg).unwrap();
        assert_eq!((img.width(), img.height()), (320, 180));
        assert_eq!(a.rastreio_id, "r1");
    }

    // a tela aplica o ponteiro (um escritor só) e liga a ocultação da mídia
    c.projeto["rastreios"][0]["denso"] = serde_json::to_value(&ptr).unwrap();
    c.projeto["rastreios"][0]["origem"] = json!({ "tipo": "assistido" });
    // "Apagar daqui em diante" e "Simplificar" pelo comando: arquivo novo, o de antes intacto
    let ed = arquivo::editar(c.ws.path(), c.cache.path(), &c.projeto, "r1", &arquivo::OpRastreio::ApagarFaixa { de_pts: tab.pts[150], ate_pts: None }).unwrap();
    assert!(ed.ancoras.is_none(), "a âncora (quadro 100) fica");
    let apagado = arquivo::ler(&c.pasta(), "r1", &ed.denso.as_ref().unwrap().sha256).unwrap();
    assert_eq!((apagado.pts.first(), apagado.pts.last()), (Some(&tab.pts[0]), Some(&tab.pts[149])));
    assert_eq!(ed.denso.as_ref().unwrap().ate_s, tab.tempo(149));
    assert_eq!(arquivo::ler(&c.pasta(), "r1", &ptr.sha256).unwrap(), d);
    let ed = arquivo::editar(c.ws.path(), c.cache.path(), &c.projeto, "r1", &arquivo::OpRastreio::Simplificar { tolerancia: 0.04 }).unwrap();
    let ks = ed.ancoras.unwrap();
    assert!(ed.denso.is_none() && ks.len() >= 2 && ks.len() < 60, "{}", ks.len());
    assert!(ks.iter().all(|k| k.pts.is_some_and(|p| tab.indice_do_pts(p).is_some()) && (k.t - tab.tempo(tab.indice_do_pts(k.pts.unwrap()).unwrap())).abs() < 1e-12));
    assert!(arquivo::editar(c.ws.path(), c.cache.path(), &c.projeto, "../r1", &arquivo::OpRastreio::Simplificar { tolerancia: 0.04 }).unwrap_err().to_string().contains(codigo::ITEM_INVALIDO));
    let ocultar = |ate: f64| json!([aceite::ocultar("o1", "r1", "solido", json!({ "ativo": { "de": tab.tempo(0), "ate": ate }, "alvo": { "rastreio": "r1", "forma": "retangulo", "vertices": null, "margem": 0.02, "borda_suave": 0.0, "inverter": false } }))]);
    let analisar = |projeto: &Value| {
        let p = armazem::tipado(projeto).unwrap();
        let prep = executar::preparar(c.ws.path(), &p, c.cache.path(), false).unwrap();
        crate::edicao::validar::analisar(&p, &prep.contexto(), None).problemas
    };
    let sem_cobertura = |pr: &[crate::edicao::modelo::Problema]| pr.iter().filter(|x| x.codigo == codigo::SEM_COBERTURA).map(|x| x.n.unwrap_or(1)).sum::<u64>();
    // ocultação até o último quadro com o bloco: coberta inteira
    c.projeto["midias"][0]["ocultacoes"] = ocultar(tab.tempo(some as usize - 1));
    assert_eq!(sem_cobertura(&analisar(&c.projeto)), 0);
    // 10 quadros a mais, depois que o bloco some: sem cobertura
    c.projeto["midias"][0]["ocultacoes"] = ocultar(tab.tempo(some as usize + 9));
    assert_eq!(sem_cobertura(&analisar(&c.projeto)), 10);
    // a ausência aceita pelo perito não bloqueia
    c.projeto["rastreios"][0]["ausencias"] = json!([{ "de_s": tab.tempo(some as usize), "ate_s": tab.tempo(some as usize + 9), "motivo": "fora_de_cena", "aceita_em": "2026-11-15T00:00:00Z", "por": "perito" }]);
    assert_eq!(sem_cobertura(&analisar(&c.projeto)), 0);

    // render: o trecho do bloco, com a ocultação; 0 pixel do bloco à vista (o controle sem ela mostra o bloco)
    c.projeto["midias"][0]["ocultacoes"] = ocultar(tab.tempo(some as usize - 1));
    c.projeto["principal"] = json!([aceite::trecho("c1", "m1", &tab, 0, some as usize, 1.0, json!([]))]);
    let visivel = |projeto: &Value, nome: &str| -> Vec<u64> {
        let p = armazem::tipado(projeto).unwrap();
        let prep = executar::preparar(c.ws.path(), &p, c.cache.path(), false).unwrap();
        let ctx = prep.contexto();
        let linha = tempo::resolver(&p, &ctx);
        let plano = crate::edicao::motor::compilar_video(&p, &linha, &ctx, &aceite::alvo_final("cpu")).expect("compila");
        let r = crate::edicao::testes::executor::rodar(&c.cap, &plano, &fontes::pasta_temporaria(nome), &crate::edicao::testes::executor::Opcoes { sem_perdas: true, ..Default::default() }).expect("render");
        let mut qs = Vec::new();
        for b in &r.blocos {
            qs.extend(crate::edicao::testes::medir::quadros(&c.cap, b).unwrap());
        }
        assert_eq!(qs.len(), some as usize);
        // pixels claros (a textura do bloco) dentro da caixa verdadeira do bloco
        qs.iter().enumerate().map(|(j, q)| {
            let (bx, by) = bloco_em(j as u32, c.w, some).unwrap();
            let mut claros = 0;
            for y in by..by + BLOCO.1 {
                for x in bx..bx + BLOCO.0 {
                    let p = &q.px[((y * q.w + x) * 3) as usize..][..3];
                    claros += u64::from(p.iter().map(|&v| v as u32).sum::<u32>() > 3 * 24);
                }
            }
            claros
        }).collect()
    };
    let com = visivel(&c.projeto, "p22-ocultar");
    assert!(com.iter().all(|&v| v == 0), "pixel do bloco à vista: {:?} ({})", com.iter().enumerate().filter(|x| *x.1 > 0).take(5).collect::<Vec<_>>(), c.cap.versao);
    c.projeto["midias"][0]["ocultacoes"] = json!([]);
    let sem = visivel(&c.projeto, "p22-controle");
    assert!(sem.iter().all(|&v| v > (BLOCO.0 * BLOCO.1 / 2) as u64), "controle: {:?}", &sem[..5]);
    let _ = (Interp::Linear, ESTADO_OK);
}

/// Âncoras do perito em 20 e 120: a corrida para na seguinte (`ancora`), e no "até" pedido; "Retomar" de uma âncora
/// nova em 60 substitui só 60..=120 (a corrida seguinte para em 120): as linhas de fora ficam iguais, bit a bit.
#[test]
fn para_na_ancora_e_retomar_so_o_trecho_seguinte() {
    let _v = vez();
    let (n, some) = (200u32, 200u32);
    let Some(mut c) = caso(n, 320, 240, some, 20) else { return };
    let tab = c.fonte.quadros.clone();
    let (a20, a120) = (c.ancora_em(20, some), c.ancora_em(120, some));
    c.projeto["rastreios"][0]["ancoras"] = json!([a20, a120]);
    let fim = c.seguir(&a20, SentidoPedido::Frente, None);
    assert_eq!(fim.parou, vec![ParouSentido { sentido: Sentido::Frente, motivo: Motivo::Ancora, pts: Some(tab.pts[120]) }]);
    let d1 = arquivo::ler(&c.pasta(), "r1", &fim.denso.as_ref().unwrap().sha256).unwrap();
    assert_eq!(d1.pts, tab.pts[20..=120].to_vec());
    assert_eq!(arquivo::corridas(&d1).unwrap()[0].parou, Motivo::Ancora);
    c.projeto["rastreios"][0]["denso"] = serde_json::to_value(fim.denso.as_ref().unwrap()).unwrap();
    // da âncora 120 até 5 s (quadro 150): para no "até"
    let fim = c.seguir(&a120, SentidoPedido::Frente, Some(tab.tempo(150)));
    assert_eq!(fim.parou, vec![ParouSentido { sentido: Sentido::Frente, motivo: Motivo::Fim, pts: Some(tab.pts[150]) }]);
    let d2 = arquivo::ler(&c.pasta(), "r1", &fim.denso.as_ref().unwrap().sha256).unwrap();
    assert_eq!(d2.pts, tab.pts[20..=150].to_vec());
    c.projeto["rastreios"][0]["denso"] = serde_json::to_value(fim.denso.as_ref().unwrap()).unwrap();
    // "Retomar" de uma âncora nova em 60
    let a60 = c.ancora_em(60, some);
    c.projeto["rastreios"][0]["ancoras"] = json!([a20, a60, a120]);
    let fim = c.seguir(&a60, SentidoPedido::Frente, None);
    assert_eq!(fim.parou[0].motivo, Motivo::Ancora);
    let d3 = arquivo::ler(&c.pasta(), "r1", &fim.denso.as_ref().unwrap().sha256).unwrap();
    assert_eq!(d3.pts, d2.pts);
    let nova = (d3.corridas.len() - 1) as u32;
    for (i, &p) in d3.pts.iter().enumerate() {
        let j = tab.indice_do_pts(p).unwrap();
        if (60..=120).contains(&j) {
            assert_eq!(d3.corrida[i], nova, "quadro {j}");
        } else {
            let k = d2.pts.binary_search(&p).unwrap();
            assert_ne!(d3.corrida[i], nova, "quadro {j}");
            let (a, b) = ([d3.x[i], d3.y[i], d3.w[i], d3.h[i], d3.conf[i]], [d2.x[k], d2.y[k], d2.w[k], d2.h[k], d2.conf[k]]);
            assert!(a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()) && d3.estado[i] == d2.estado[k], "quadro {j}");
            assert_eq!(arquivo::corridas(&d3).unwrap()[d3.corrida[i] as usize], arquivo::corridas(&d2).unwrap()[d2.corrida[k] as usize], "quadro {j}");
        }
    }
    let r = arquivo::corridas(&d3).unwrap();
    assert_eq!((r[nova as usize].ancora_pts, r[nova as usize].de_pts, r[nova as usize].ate_pts), (tab.pts[60], tab.pts[60], tab.pts[120]));
    for j in 20..=150 {
        let (bx, by) = bloco_em(j as u32, c.w, some).unwrap();
        let i = iou(caixa_do_denso(&d3, &tab, j, c.w, c.h).unwrap(), [bx as f64, by as f64, BLOCO.0 as f64, BLOCO.1 as f64]);
        assert!(i > 0.9, "quadro {j}: IoU {i:.3}");
    }
}

/// Espaço de origem com clipe de 29 s: o intermediário é lido com a busca do §5.3 (quadros-chave do GOP fixo), para a
/// frente e para trás, com os pts da tabela e o quadro certo, um processo por bloco (sem leitura refeita do começo);
/// a corrida para trás desde o fim do clipe acerta.
#[test]
fn origem_longa_le_com_busca() {
    let _v = vez();
    let (n, some) = (900u32, 900u32);
    let (j_in, j_out, j0, ate) = (10usize, 890usize, 880usize, 730usize);
    let Some(mut c) = caso(n, 320, 240, some, j0) else { return };
    let tab = c.fonte.quadros.clone();
    let lente = aceite::efeito("lente", "l1", json!({ "k1": 0.0, "k2": 0.0, "cx": 0.5, "cy": 0.5, "perfil": "" }));
    c.projeto["principal"] = json!([aceite::trecho("c1", "m1", &tab, j_in, j_out, 1.0, json!([lente]))]);
    let p = armazem::tipado(&c.projeto).unwrap();
    let Tolerante::Conhecido(ClipePrincipal::Trecho(clipe)) = &p.principal[0] else { panic!() };
    let espaco_id = geometria::espaco_de_origem(&p.midias[0].sha256, &clipe.base.efeitos);
    c.projeto["rastreios"][0]["espaco"] = json!(espaco_id);
    let ancora: Value = c.projeto["rastreios"][0]["ancoras"][0].clone();
    let fim = c.seguir(&ancora, SentidoPedido::Tras, Some(tab.tempo(ate)));
    assert_eq!(fim.parou, vec![ParouSentido { sentido: Sentido::Tras, motivo: Motivo::Fim, pts: Some(tab.pts[ate]) }]);
    let d = arquivo::ler(&c.pasta(), "r1", &fim.denso.unwrap().sha256).unwrap();
    assert_eq!(d.pts, tab.pts[ate..=j0].to_vec());
    for j in ate..=j0 {
        let (bx, by) = bloco_em(j as u32, c.w, some).unwrap();
        let i = iou(caixa_do_denso(&d, &tab, j, c.w, c.h).unwrap(), [bx as f64, by as f64, BLOCO.0 as f64, BLOCO.1 as f64]);
        assert!(i > 0.9, "quadro {j}: IoU {i:.3}");
    }
    // o intermediário (já pronto) com busca: quadros dos 3 primeiros segundos, depois dos 10 s e o último
    let p = armazem::tipado(&c.projeto).unwrap();
    let r = p.rastreios[0].clone();
    let t = Trabalho {
        ws: c.ws.path().to_path_buf(),
        cache: c.cache.path().to_path_buf(),
        ai: ai_padrao(),
        clipe: trecho(&p, "c1"),
        p,
        r,
        pedido: c.pedido(SentidoPedido::Tras),
        tarefa: "t".into(),
        cancelar: Arc::new(AtomicBool::new(false)),
    };
    let mut fonte = c.fonte.clone();
    fonte.sha256 = t.p.midias[0].sha256.clone();
    let esp = espaco(&t, &c.cap, &c.pasta(), &fonte).unwrap();
    let ti = esp.leitura.quadros.clone();
    assert_eq!(ti.pts.len(), j_out - j_in);
    let px = TamanhoPx { w: esp.leitura.exibido.0, h: esp.leitura.exibido.1 };
    for (de, ate, s, processos) in [(2usize, 6usize, Sentido::Frente, 1usize), (700, 710, Sentido::Frente, 1), (869, 640, Sentido::Tras, 3)] {
        let mut l = LeitorPipe::abrir(&c.cap, &esp.leitura, ti.pts[de], Some(ti.pts[ate]), s, px).unwrap();
        let lidos = ler_todos(&mut l);
        assert_eq!(l.processos(), processos, "{de}..{ate}");
        let esperados: Vec<usize> = if de <= ate { (de..=ate).collect() } else { (ate..=de).rev().collect() };
        assert_eq!(lidos.iter().map(|x| x.0).collect::<Vec<_>>(), esperados.iter().map(|&j| ti.pts[j]).collect::<Vec<_>>(), "{de}..{ate} ({})", c.cap.versao);
        assert_eq!(lidos.iter().map(|x| numero(&x.1, px.w)).collect::<Vec<_>>(), esperados.iter().map(|&j| (j + j_in) as u32).collect::<Vec<_>>(), "{de}..{ate} ({})", c.cap.versao);
    }
    assert!(ti.busca_em(0.0, 700).is_some() && ti.busca_em(0.0, 640).is_some() && ti.busca_em(0.0, 2).is_none());
}

/// Caminho com espaço e acento lê igual; arquivo que encolheu (TS cortado) ou ficou vazio depois da tabela: erro
/// na leitura, nunca uma faixa mais curta tomada por inteira.
#[test]
fn caminho_com_acento_e_arquivo_que_encolheu() {
    let Some((cap, v)) = video("acento.ts", 120, 320, 240, 120, &["-f", "mpegts"]) else { return };
    let pasta = fontes::pasta_temporaria("p22 caminho com espaço");
    let dentro = pasta.join("vídeo ç ã.ts");
    std::fs::copy(&v, &dentro).unwrap();
    let mut f = fonte_de(&cap, &dentro);
    let tab = f.quadros.clone();
    let mut l = LeitorPipe::abrir(&cap, &f, tab.pts[100], Some(tab.pts[104]), Sentido::Frente, pixel(&f)).unwrap();
    assert_eq!(ler_todos(&mut l).iter().map(|x| numero(&x.1, 320)).collect::<Vec<_>>(), (100..=104).collect::<Vec<u32>>());
    // metade do arquivo: a tabela pede 120 quadros
    let b = std::fs::read(&dentro).unwrap();
    let cortado = pasta.join("cortado.ts");
    std::fs::write(&cortado, &b[..b.len() / 2 / 188 * 188]).unwrap();
    f.caminho = cortado.clone();
    let mut l = LeitorPipe::abrir(&cap, &f, tab.pts[0], None, Sentido::Frente, pixel(&f)).unwrap();
    let mut lidos = 0;
    let e = loop {
        match l.proximo() {
            Ok(Some(_)) => lidos += 1,
            Ok(None) => panic!("faixa curta tomada por inteira: {lidos} quadros"),
            Err(e) => break e,
        }
    };
    assert!(lidos > 10 && lidos < 120 && e.to_string().contains(ERRO_LEITURA_DIVERGENTE), "{lidos}: {e}");
    // vazio
    std::fs::write(&cortado, b"").unwrap();
    let mut l = LeitorPipe::abrir(&cap, &f, tab.pts[0], None, Sentido::Frente, pixel(&f)).unwrap();
    assert!(l.proximo().is_err());
    let mut l = LeitorPipe::abrir(&cap, &f, tab.pts[100], Some(tab.pts[0]), Sentido::Tras, pixel(&f)).unwrap();
    assert!(l.proximo().is_err());
}

/// Thread do leitor que morre em pânico: erro, não fim da faixa.
#[test]
fn leitor_em_panico_nao_e_fim() {
    let (tx, rx) = sync_channel::<Result<(i64, Vec<u8>)>>(FILA);
    let fio = std::thread::spawn(move || {
        tx.send(Ok((7, vec![1, 2, 3]))).unwrap();
        panic!("leitor de teste");
    });
    let mut l = LeitorPipe { rx: Some(rx), parar: Arc::new(AtomicBool::new(false)), externo: None, fio: Some(fio), atual: Vec::new(), pixel: TamanhoPx { w: 1, h: 1 }, processos: Arc::new(AtomicUsize::new(0)) };
    assert_eq!(l.proximo().unwrap().map(|(p, q)| (p, q.rgb.to_vec())), Some((7, vec![1, 2, 3])));
    assert!(l.proximo().unwrap_err().to_string().contains(ERRO_LEITURA_DIVERGENTE));
}

/// Espaço de origem: a corrida lê o intermediário do clipe (lente de coeficientes zero: a imagem é a mesma, o espaço
/// é `origem:<hash>`), e o denso sai nos pts da fonte (os do intermediário + o do primeiro quadro do clipe).
#[test]
fn espaco_de_origem_le_o_intermediario() {
    let _v = vez();
    let (j_in, j_out, j0) = (10usize, 70usize, 20usize);
    let Some(mut c) = caso(90, 640, 360, 90, j0) else { return };
    let tab = c.fonte.quadros.clone();
    let lente = aceite::efeito("lente", "l1", json!({ "k1": 0.0, "k2": 0.0, "cx": 0.5, "cy": 0.5, "perfil": "" }));
    c.projeto["principal"] = json!([aceite::trecho("c1", "m1", &tab, j_in, j_out, 1.0, json!([lente]))]);
    let p = armazem::tipado(&c.projeto).unwrap();
    let Tolerante::Conhecido(ClipePrincipal::Trecho(clipe)) = &p.principal[0] else { panic!() };
    let espaco = geometria::espaco_de_origem(&p.midias[0].sha256, &clipe.base.efeitos);
    assert!(espaco.starts_with("origem:"));
    c.projeto["rastreios"][0]["espaco"] = json!(espaco);
    let (ev, emitir) = coletor();
    pedir(c.ws.path(), c.cache.path(), &ai_padrao(), &c.projeto, &c.pedido(SentidoPedido::Frente), emitir).unwrap();
    let (_, fim) = esperar_fim(&ev, 600);
    let EventoRastreio::Fim(fim) = fim else { panic!("{fim:?}") };
    // até a borda do clipe, não da mídia
    assert_eq!(fim.parou, vec![ParouSentido { sentido: Sentido::Frente, motivo: Motivo::Fim, pts: Some(tab.pts[j_out - 1]) }]);
    let d = arquivo::ler(&c.pasta(), "r1", &fim.denso.unwrap().sha256).unwrap();
    assert_eq!((d.espaco.as_str(), d.pixel), (espaco.as_str(), TamanhoPx { w: 640, h: 360 }));
    assert_eq!(d.pts, tab.pts[j0..j_out].to_vec());
    for j in j0..j_out {
        let (bx, by) = bloco_em(j as u32, c.w, 90).unwrap();
        let i = iou(caixa_do_denso(&d, &tab, j, c.w, c.h).unwrap(), [bx as f64, by as f64, BLOCO.0 as f64, BLOCO.1 as f64]);
        assert!(i > 0.9, "quadro {j}: IoU {i:.3}");
    }
    // o intermediário existe no cache e foi ele o lido (a fonte é outro arquivo)
    assert!(std::fs::read_dir(c.cache.path().join("origem")).unwrap().flatten().any(|e| e.path().extension().is_some_and(|x| x == "mp4")));
    // espaço que não é o do clipe: recusado sem ler nada
    c.projeto["rastreios"][0]["espaco"] = json!(format!("origem:{}", "0".repeat(64)));
    let (ev, emitir) = coletor();
    pedir(c.ws.path(), c.cache.path(), &ai_padrao(), &c.projeto, &c.pedido(SentidoPedido::Frente), emitir).unwrap();
    let (_, e) = esperar_fim(&ev, 120);
    assert!(matches!(&e, EventoRastreio::Falhou(f) if f.mensagem.starts_with(codigo::RASTREIO_ESPACO)), "{e:?}");
}
