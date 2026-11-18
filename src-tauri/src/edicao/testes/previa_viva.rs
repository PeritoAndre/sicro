//! Prévia viva (P3.3) × motor: o quadro final (`Alvo::Quadro`, o mesmo do `edicao_quadro_final`) de 5 projetos de
//! formas sobre fontes sintéticas, e o tempo do quadro final em 1080p com GOP de 2 s. Com `SICRO_PREVIA_SAIDA=<pasta>`
//! grava as fontes, os projetos, as tabelas e os quadros (PNG): a bancada da prévia pinta os mesmos slots pelo palco
//! (`palco/motor`) e confere as formas a até 2 px (contratos-fase4 (f)). Nada do palco entra no render (D10).
#![cfg(test)]

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde_json::{json, Value};

use super::aceite::{ancora, efeito, midia, ocultar, projeto, rastreio, trecho, Mundo};
use super::executor::{self, Opcoes};
use super::fontes;
use super::medir::{self, QuadroRgb};
use crate::edicao::ffmpeg::exec::{self, Comando};
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::modelo::ProjetoEdicao;
use crate::edicao::motor::{Alvo, FonteInfo};
use crate::edicao::quadros::TabelaQuadros;

/// Quadro final do pedido ao arquivo em 1080p com GOP ≤ 2 s (§11 Fase 3, P3.3) e a folga com a máquina ocupada (O1).
pub const QUADRO_FINAL_1080_S: f64 = 0.8;
const FOLGA: f64 = 3.0;
const W: u32 = 640;
const H: u32 = 360;
/// Cores das formas (puras, sobre fonte cinza): a bancada acha cada uma pela cor.
const VERMELHO: [u8; 3] = [255, 0, 0];
const AZUL: [u8; 3] = [0, 0, 255];
const VERDE: [u8; 3] = [0, 255, 0];

fn gerar(cap: &Capacidades, pasta: &Path, nome: &str, lavfi: &str, codificar: &[&str]) -> PathBuf {
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i", lavfi].map(String::from).into();
    a.extend(codificar.iter().map(|s| s.to_string()));
    a.push(nome.to_string());
    let cmd = Comando { nome: "gerar".into(), programa: cap.binario.clone(), args: a, pasta: pasta.to_path_buf(), ambiente: Vec::new(), sem_ambiente: Vec::new(), stderr: pasta.join("gerar.stderr") };
    let fim = exec::rodar(&cmd, &AtomicBool::new(false)).expect("ffmpeg");
    exec::exigir_sucesso(&cmd, &fim).expect("fonte de teste");
    pasta.join(nome)
}

fn fonte_de(cap: &Capacidades, video: &Path) -> FonteInfo {
    let tab = TabelaQuadros::do_decodificador(cap, video, &AtomicBool::new(false), &mut |_| {}).expect("tabela do decodificador");
    crate::edicao::quadros::fonte_info(cap, video, &"0".repeat(64), Arc::new(tab), None).expect("fonte_info")
}

fn destaque(id: &str, forma: &str, r: &str, cor: &str) -> Value {
    efeito("destaque", id, json!({ "forma": forma, "rastreio": r, "traco": { "cor": cor, "espessura": 0.008, "contorno": false }, "tamanho": 0.12, "angulo_graus": 45.0, "margem": 0.05, "rotulo": null }))
}

fn com_transicao(mut c: Value, tipo: &str, s: f64) -> Value {
    c["transicao_entrada"] = json!({ "tipo": tipo, "duracao_s": s, "audio": "acompanhar" });
    c
}

/// Rastreios que andam: r1 do canto de cima à esquerda até o centro de baixo, r2 perto do centro.
fn rastreios(t: &TabelaQuadros) -> Value {
    let n = t.pts.len() - 1;
    json!([
        rastreio("r1", Some("m1"), "fonte", vec![ancora(t.tempo(0), Some(t.pts[0]), [0.10, 0.20, 0.15, 0.20]), ancora(t.tempo(n), Some(t.pts[n]), [0.70, 0.55, 0.15, 0.20])]),
        rastreio("r2", Some("m1"), "fonte", vec![ancora(t.tempo(0), Some(t.pts[0]), [0.40, 0.40, 0.10, 0.12]), ancora(t.tempo(n), Some(t.pts[n]), [0.55, 0.50, 0.10, 0.12])]),
    ])
}

/// Slots medidos e as cores das formas em cada um.
type Slots = Vec<(u64, Vec<[u8; 3]>)>;

fn em(qs: &[u64], cores: &[[u8; 3]]) -> Slots {
    qs.iter().map(|&q| (q, cores.to_vec())).collect()
}

/// Os 5 projetos (nome, projeto, slots medidos com as cores das formas).
fn casos(t: &TabelaQuadros) -> Vec<(&'static str, ProjetoEdicao, Slots)> {
    let m = || vec![midia("m1", "video", json!([]))];
    let extra = || json!({ "rastreios": rastreios(t) });
    let titulo = json!({ "tipo": "titulo", "id": "t1", "rotulo": "t1", "desligado": false, "transicao_entrada": null, "efeitos": [], "duracao_s": 2.0,
                         "titulo": { "modelo": "cartela", "fundo": { "tipo": "cor", "cor": "#000000" }, "blocos": [] } });
    let congelar = json!({ "tipo": "congelar", "id": "g1", "rotulo": "g1", "desligado": false, "transicao_entrada": null, "midia": "m1",
                           "instante_s": t.tempo(50), "instante_pts": t.pts[50], "duracao_s": 1.0, "efeitos": [destaque("e1", "colchetes", "r1", "#ff0000")] });
    let zoom = efeito("enquadramento", "z", json!({ "regiao": { "x": 0.25, "y": 0.25, "w": 0.5, "h": 0.5 }, "ajuste": "cobrir", "interpolacao": "bilinear" }));
    let girar = efeito("girar", "g", json!({ "graus": 90, "espelhar_h": false, "espelhar_v": false }));
    let v = vec![
        ("p1-destaque", projeto(W, H, (30, 1), m(), vec![trecho("c1", "m1", t, 15, 195, 1.0, json!([destaque("e1", "retangulo", "r1", "#ff0000")]))], extra()), em(&[0, 70, 179], &[VERMELHO])),
        (
            "p2-meia-velocidade-zoom",
            projeto(W, H, (30, 1), m(), vec![trecho("c1", "m1", t, 30, 120, 0.5, json!([zoom, destaque("e1", "elipse", "r2", "#ff0000")]))], extra()),
            em(&[10, 90, 170], &[VERMELHO]),
        ),
        (
            "p3-fundir-girar-seta",
            projeto(
                W,
                H,
                (30, 1),
                m(),
                vec![trecho("c1", "m1", t, 0, 60, 1.0, json!([])), com_transicao(trecho("c2", "m1", t, 100, 190, 1.0, json!([girar, destaque("e1", "seta", "r2", "#ff0000")])), "fundir", 0.5)],
                extra(),
            ),
            em(&[65, 100, 130], &[VERMELHO]),
        ),
        (
            "p4-titulo-congelado-ocultar",
            projeto(W, H, (30, 1), m(), vec![titulo, congelar, trecho("c3", "m1", t, 60, 150, 1.0, json!([ocultar("o1", "r2", "solido", json!({ "cor": "#0000ff" }))]))], extra()),
            // o colchete no congelado; o sólido azul no trecho
            [em(&[70], &[VERMELHO]), em(&[110, 170], &[AZUL])].concat(),
        ),
        (
            "p5-dobro-escurecer",
            projeto(
                W,
                H,
                (30, 1),
                m(),
                vec![
                    trecho("c1", "m1", t, 0, 60, 1.0, json!([])),
                    com_transicao(trecho("c2", "m1", t, 60, 240, 2.0, json!([destaque("e1", "retangulo", "r1", "#ff0000"), destaque("e2", "circulo", "r2", "#00ff00")])), "escurecer", 0.3),
                ],
                extra(),
            ),
            em(&[80, 100, 140], &[VERMELHO, VERDE]),
        ),
    ];
    // selos de edição desligados: o texto do canto não é forma (e o palco só o pinta com o sprite do Rust)
    v.into_iter()
        .map(|(n, mut p, q)| {
            p.saida.selos.velocidade = false;
            p.saida.selos.ampliacao = false;
            p.saida.selos.realce = false;
            (n, p, q)
        })
        .collect()
}

fn quadro_do_bmp(p: &Path) -> QuadroRgb {
    let img = image::open(p).expect("bmp do quadro final").to_rgb8();
    QuadroRgb { w: img.width(), h: img.height(), px: img.into_raw() }
}

/// Os quadros finais dos 5 projetos; cada forma aparece no slot medido. Com `SICRO_PREVIA_SAIDA`, grava para a bancada.
#[test]
fn quadros_finais_dos_projetos_da_paridade() {
    let Some(cap) = fontes::cap() else { return };
    let dir = fontes::pasta_temporaria("previa-paridade");
    // cinza com ruído, 640×360 a 30 qps, quadro-chave a cada 2 s e quadros B (como câmera)
    let video = gerar(&cap, &dir, "fonte.mp4", "color=c=0x505050:s=640x360:r=30:d=8,noise=alls=10:allf=t,format=yuv420p", &["-c:v", "libx264", "-preset", "veryfast", "-g", "60", "-bf", "2", "-pix_fmt", "yuv420p", "-movflags", "+faststart"]);
    let info = fonte_de(&cap, &video);
    let t = &info.quadros;
    assert_eq!(t.pts.len(), 240);
    let mundo = Mundo::novo(&cap).com("m1", &info);
    let saida = std::env::var_os("SICRO_PREVIA_SAIDA").map(|s| PathBuf::from(s).join(cap.versao.replace(['/', ' '], "_")));
    let mut indice = Vec::new();
    let hex = |c: &[u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
    for (nome, p, slots) in casos(t) {
        let mut quadros = Vec::new();
        for (q, cores) in &slots {
            let q = *q;
            let plano = mundo.compilar(&p, &Alvo::Quadro { q, largura: W }).unwrap_or_else(|e| panic!("{nome} q{q}: {e}"));
            let r = executor::rodar(&cap, &plano, &dir.join(format!("{nome}-q{q}")), &Opcoes::default()).unwrap_or_else(|e| panic!("{nome} q{q}: {e}"));
            let img = quadro_do_bmp(&r.blocos[0]);
            assert_eq!((img.w, img.h), (W, H), "{nome} q{q}");
            for c in cores {
                let caixa = medir::marca(&img, *c, 60);
                assert!(caixa.is_some_and(|b| b[2] >= 4 && b[3] >= 4), "{nome} q{q}: forma {c:?} ausente no quadro final ({})", cap.versao);
            }
            if let Some(s) = &saida {
                let pasta = s.join(nome);
                std::fs::create_dir_all(&pasta).unwrap();
                image::open(&r.blocos[0]).unwrap().save(pasta.join(format!("q{q}.png"))).unwrap();
            }
            quadros.push(json!({ "q": q, "png": format!("{nome}/q{q}.png"), "cores": cores.iter().map(hex).collect::<Vec<_>>() }));
        }
        indice.push(json!({ "nome": nome, "projeto": serde_json::to_value(&p).unwrap(), "quadros": quadros }));
    }
    if let Some(s) = &saida {
        std::fs::copy(&video, s.join("fonte.mp4")).unwrap();
        let midias = json!({ "m1": { "arquivo": "fonte.mp4", "tabela": t.compacta().unwrap(), "w": info.exibido.0, "h": info.exibido.1 } });
        std::fs::write(s.join("casos.json"), serde_json::to_vec_pretty(&json!({ "ffmpeg": cap.versao, "midias": midias, "casos": indice })).unwrap()).unwrap();
        eprintln!("paridade gravada em {}", s.display());
    }
}

/// Quadro final em 1080p com GOP de 2 s: do pedido (compilar) ao arquivo (BMP) em até 0,8 s (× folga com a máquina
/// ocupada); o quadro é o da fonte. Slots no fim de um GOP depois dos 10 s (a busca cai no quadro-chave anterior e
/// decodifica o GOP inteiro) e um antes dos 10 s (sem busca, D5).
#[test]
fn quadro_final_em_1080p_com_gop_de_2_s() {
    let Some(cap) = fontes::cap() else { return };
    let dir = fontes::pasta_temporaria("previa-quadro-final");
    let video = gerar(&cap, &dir, "f1080.mp4", "testsrc2=s=1920x1080:r=30:d=30", &["-c:v", "libx264", "-preset", "ultrafast", "-crf", "28", "-g", "60", "-keyint_min", "60", "-sc_threshold", "0", "-pix_fmt", "yuv420p"]);
    let info = fonte_de(&cap, &video);
    let tab = &info.quadros;
    assert_eq!(tab.pts.len(), 900);
    let p = projeto(1920, 1080, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, 0, 900, 1.0, json!([]))], json!({}));
    let mundo = Mundo::novo(&cap).com("m1", &info);
    let mut tempos = Vec::new();
    for q in [359u64, 659, 899, 299] {
        let t0 = std::time::Instant::now();
        let plano = mundo.compilar(&p, &Alvo::Quadro { q, largura: 1920 }).unwrap();
        let r = executor::rodar(&cap, &plano, &dir.join(format!("q{q}")), &Opcoes::default()).unwrap();
        let s = t0.elapsed().as_secs_f64();
        let img = quadro_do_bmp(&r.blocos[0]);
        assert_eq!((img.w, img.h), (1920, 1080));
        // o quadro certo: o motor leu um quadro só, o do slot
        assert_eq!(plano.blocos[0].conferir[0].lidos, 1, "q{q}");
        tempos.push((q, s));
    }
    eprintln!("quadro final 1080p GOP 2 s ({}): {}", cap.versao, tempos.iter().map(|(q, s)| format!("q{q} {s:.3} s")).collect::<Vec<_>>().join(", "));
    let pior = tempos.iter().map(|x| x.1).fold(0.0, f64::max);
    assert!(pior < QUADRO_FINAL_1080_S * FOLGA, "{pior:.3} s (limite {QUADRO_FINAL_1080_S} s × folga {FOLGA})");
}
