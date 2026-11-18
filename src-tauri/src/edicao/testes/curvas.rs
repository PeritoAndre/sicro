//! Quadros-chave e curvas no render (Fase 4, P4.2; contratos-fase4.md D7 e (f)): posição, escala, rotação e opacidade
//! do sobreposto (texto livre preso a um clipe e imagem) e o volume (som do trecho e clipe de áudio) animados pelas
//! três interpolações, medidos no que o ffmpeg entrega contra `curva::amostrar`. Roda no ffmpeg de `SICRO_FFMPEG`
//! (os dois binários, um de cada vez). Dono: P4.2.
#![cfg(test)]

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde_json::{json, Value};

use super::aceite;
use super::executor;
use super::fontes;
use super::medir::{self, QuadroRgb};
use crate::edicao::camada::texto::bitmap_sprite;
use crate::edicao::camada::{titulo, Bitmap};
use crate::edicao::curva;
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::modelo::{Animavel, ClipeSobreposto, EstadoMidia, Racional, TamanhoPx};
use crate::edicao::motor::{self, Contexto, FonteInfo};
use crate::edicao::quadros::{self, apoio, TabelaQuadros};
use crate::edicao::tempo;

const W: u32 = 320;
const H: u32 = 180;
const FPS: Racional = Racional { num: 30, den: 1 };
const FUNDO: &str = "#404040";

fn gerar(cap: &Capacidades, pasta: &Path, nome: &str, args: &[&str]) -> std::path::PathBuf {
    let saida = pasta.join(nome);
    let s = Command::new(&cap.binario).args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y"]).args(args).arg(&saida).status().expect("ffmpeg");
    assert!(s.success(), "gerar {nome} ({})", cap.versao);
    saida
}

fn taus(n: u64) -> Vec<f64> {
    (0..n).map(|k| tempo::instante_do_slot(k, FPS)).collect()
}

fn animavel(v: &Value) -> Animavel<f64> {
    serde_json::from_value(v.clone()).unwrap()
}

// ------------------------------------------------------------------ medida no quadro

/// Peso de uma cor sobre o fundo cinza: o canal dela menos o maior dos outros (o cinza dá 0).
fn peso(p: &[u8], canal: usize) -> f64 {
    let outros = (0..3).filter(|&c| c != canal).map(|c| p[c]).max().unwrap();
    (p[canal] as f64 - outros as f64).max(0.0)
}

/// Momentos da mancha de uma cor: centro (px), lados de um retângulo de mesma variância (`√(12·λ)`), ângulo do lado
/// maior (graus, horário na tela) e o miolo: a luma (3 × 3 em volta do centro) acima da do fundo (canto de cima à
/// esquerda). A luma tem a resolução inteira; o peso da cor vem do croma, que é de meia resolução e de poucos níveis.
#[derive(Debug, Clone, Copy)]
struct Mancha {
    cx: f64,
    cy: f64,
    maior: f64,
    menor: f64,
    angulo: f64,
    miolo: f64,
}

fn momentos(px: impl Iterator<Item = (f64, f64, f64)>) -> (f64, f64, f64, f64, f64) {
    let (mut s, mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0.0f64, 0.0, 0.0, 0.0, 0.0, 0.0);
    for (a, b, w) in px {
        s += w;
        sx += w * a;
        sy += w * b;
        sxx += w * a * a;
        syy += w * b * b;
        sxy += w * a * b;
    }
    let (cx, cy) = (sx / s, sy / s);
    let (vxx, vyy, vxy) = (sxx / s - cx * cx, syy / s - cy * cy, sxy / s - cx * cy);
    let (m, d) = ((vxx + vyy) / 2.0, (((vxx - vyy) / 2.0).powi(2) + vxy * vxy).sqrt());
    (cx, cy, (12.0 * (m + d)).sqrt(), (12.0 * (m - d).max(0.0)).sqrt(), 0.5 * (2.0 * vxy).atan2(vxx - vyy).to_degrees())
}

fn mancha(q: &QuadroRgb, canal: usize) -> Mancha {
    let maximo = q.px.chunks_exact(3).map(|p| peso(p, canal)).fold(0.0, f64::max);
    assert!(maximo > 20.0, "sem a cor {canal} no quadro");
    // ruído da conversão de cor fora da mancha não pesa
    let corte = maximo * 0.03;
    let (cx, cy, maior, menor, angulo) = momentos(q.px.chunks_exact(3).enumerate().filter_map(|(i, p)| {
        let w = peso(p, canal);
        (w > corte).then(|| ((i as u32 % q.w) as f64 + 0.5, (i as u32 / q.w) as f64 + 0.5, w))
    }));
    let luma = |x: i64, y: i64| {
        let i = (y as u32 * q.w + x as u32) as usize * 3;
        0.2126 * q.px[i] as f64 + 0.7152 * q.px[i + 1] as f64 + 0.0722 * q.px[i + 2] as f64
    };
    let media = |x0: i64, y0: i64| (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (dx, dy))).map(|(dx, dy)| luma(x0 + dx, y0 + dy)).sum::<f64>() / 9.0;
    let miolo = media(cx.floor() as i64, cy.floor() as i64) - media(4, 4);
    Mancha { cx, cy, maior, menor, angulo, miolo }
}

/// Diferença de ângulos de eixo (mod 180°).
fn dif_angulo(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(180.0);
    d.min(180.0 - d)
}

// ------------------------------------------------------------------ sobreposto

/// Texto livre verde sobre a caixa verde (um retângulo cheio; cores sem saturar, que a volta pelo YUV não corta), preso ao clipe "B" 0,2 s depois do começo dele, sem
/// entrada nem saída (corte): só as curvas mexem. Lineares, suaves e degraus misturados em cada propriedade.
fn texto_animado() -> Value {
    json!({ "tipo": "titulo", "id": "tl", "rotulo": "", "pos": { "ref": "clipe", "clipe": "B", "desloc": 0.2 }, "duracao_s": 1.8,
        "transformacao": {
            "x": [{ "t": 0.0, "v": 0.25, "interp": "linear" }, { "t": 0.8, "v": 0.4, "interp": "suave" }, { "t": 1.6, "v": 0.3 }],
            "y": [{ "t": 0.0, "v": 0.3, "interp": "suave" }, { "t": 1.2, "v": 0.7 }],
            "escala": [{ "t": 0.0, "v": 0.8, "interp": "linear" }, { "t": 0.6, "v": 1.25, "interp": "degrau" }, { "t": 1.2, "v": 1.0 }],
            "rotacao": [{ "t": 0.0, "v": -25.0, "interp": "suave" }, { "t": 1.5, "v": 30.0 }],
            "opacidade": [{ "t": 0.0, "v": 0.4, "interp": "linear" }, { "t": 0.9, "v": 1.0, "interp": "degrau" }, { "t": 1.4, "v": 0.6 }]
        },
        "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [], "animacao": { "entrada": "corte", "saida": "corte", "lado": "baixo" },
        "titulo": { "modelo": "livre", "fundo": { "tipo": "transparente" }, "blocos": [{ "id": "b1", "papel": "corpo", "conteudo": "Texto",
            "estilo": { "fonte": "sans", "negrito": true, "italico": false, "tamanho": 0.09, "cor": "#30c030", "alinhamento": "centro",
                        "caixa": { "cor": "#30c030", "margem": 0.03, "raio": 0.0 }, "contorno": null },
            "lugar": { "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" }, "largura_max": 0.6 }] } })
}

/// Imagem azul clara 2:1 no programa (0,1 s), com as cinco propriedades animadas.
fn imagem_animada() -> Value {
    json!({ "tipo": "imagem", "id": "im", "rotulo": "", "pos": { "ref": "programa", "t": 0.1 }, "duracao_s": 2.2, "midia": "azul",
        "transformacao": {
            "x": [{ "t": 0.0, "v": 0.7, "interp": "suave" }, { "t": 2.0, "v": 0.82 }],
            "y": [{ "t": 0.0, "v": 0.65, "interp": "linear" }, { "t": 1.0, "v": 0.35, "interp": "linear" }, { "t": 2.0, "v": 0.5 }],
            "escala": [{ "t": 0.0, "v": 0.15, "interp": "suave" }, { "t": 1.5, "v": 0.25 }],
            "rotacao": [{ "t": 0.0, "v": 10.0, "interp": "degrau" }, { "t": 1.0, "v": -15.0 }],
            "opacidade": [{ "t": 0.0, "v": 1.0, "interp": "linear" }, { "t": 2.0, "v": 0.45 }]
        },
        "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [] })
}

const IMG: (u32, u32) = (48, 24);

fn imagem_azul(pasta: &Path) -> FonteInfo {
    let px = [96u8, 160, 255, 255].repeat((IMG.0 * IMG.1) as usize);
    let png = Bitmap { w: IMG.0, h: IMG.1, canais: 4, px }.png().unwrap();
    let caminho = pasta.join("azul.png");
    std::fs::write(&caminho, png).unwrap();
    let mut info = apoio::fonte(apoio::tabela((1, 1), vec![0]), IMG);
    info.caminho = caminho;
    info.pix_fmt = "rgba".into();
    info
}

/// Valores das cinco propriedades amostrados pelo Rust nos instantes locais.
struct Amostras {
    x: Vec<f64>,
    y: Vec<f64>,
    escala: Vec<f64>,
    rotacao: Vec<f64>,
    opacidade: Vec<f64>,
}

fn amostrar(c: &Value, tempos: &[f64]) -> Amostras {
    let t = &c["transformacao"];
    let a = |k: &str| curva::amostrar(&animavel(&t[k]), tempos);
    Amostras { x: a("x"), y: a("y"), escala: a("escala"), rotacao: a("rotacao"), opacidade: a("opacidade") }
}

/// O que o motor desenha, conferido contra a amostragem do Rust nos dois sobrepostos, slot a slot: centro a até 1 px,
/// ângulo a até 1°, lados a até 1,5 px (escala) e o miolo pela opacidade. As contas das curvas são conferidas antes à
/// mão (linear, suave e degrau), para a amostragem não ser a única referência.
#[test]
fn posicao_escala_rotacao_opacidade_no_render() {
    let Some(cap) = fontes::cap() else { return };
    let pasta = fontes::pasta_temporaria("curvas-sobreposto");
    let cartela = |id: &str, dur: f64| {
        json!({ "tipo": "titulo", "id": id, "rotulo": "", "duracao_s": dur, "transicao_entrada": null,
                "titulo": { "modelo": "cartela", "fundo": { "tipo": "cor", "cor": FUNDO }, "blocos": [] } })
    };
    let (texto, imagem) = (texto_animado(), imagem_animada());
    let p = aceite::projeto(W, H, (FPS.num, FPS.den), vec![aceite::midia("azul", "imagem", json!([]))], vec![cartela("A", 0.5), cartela("B", 2.0)],
        json!({ "sobreposicoes": [{ "id": "s1", "nome": "", "oculta": false, "clipes": [imagem.clone(), texto.clone()] }] }));
    let plano = aceite::Mundo::novo(&cap).com("azul", &imagem_azul(&pasta)).compilar(&p, &aceite::alvo_final("cpu")).unwrap_or_else(|e| panic!("{e} ({})", cap.versao));
    let r = executor::rodar(&cap, &plano, &pasta.join("render"), &executor::Opcoes { sem_perdas: true, ..Default::default() }).unwrap_or_else(|e| panic!("{e} ({})", cap.versao));
    let q: Vec<QuadroRgb> = r.blocos.iter().flat_map(|b| medir::quadros(&cap, b).unwrap()).collect();
    assert_eq!(q.len(), 75, "{}", cap.versao);

    // texto: slots [21, 75) (clipe B no slot 15, + 6); imagem: [3, 69)
    let (ti, tn, ii, in_) = (21usize, 54usize, 3usize, 66usize);
    let (at, ai) = (amostrar(&texto, &taus(tn as u64)), amostrar(&imagem, &taus(in_ as u64)));
    // as curvas à mão, nos instantes dos slots: linear no meio, suave a ¼ e a ¾, degrau segurando
    let perto = |a: f64, b: f64| (a - b).abs() < 1e-12;
    assert!(perto(at.x[12], 0.325), "linear: {}", at.x[12]);
    assert!(perto(at.x[30], 0.4 + (0.3 - 0.4) * 0.15625), "suave a ¼: {}", at.x[30]);
    assert!(perto(at.x[42], 0.4 + (0.3 - 0.4) * 0.84375), "suave a ¾: {}", at.x[42]);
    assert!(perto(at.escala[17], 0.8 + 0.45 * (17.0 / 18.0)) && at.escala[18] == 1.25 && at.escala[35] == 1.25 && at.escala[36] == 1.0, "degrau");
    assert!(at.opacidade[26] < 1.0 && at.opacidade[27] == 1.0 && at.opacidade[41] == 1.0 && at.opacidade[42] == 0.6, "degrau da opacidade");
    assert!(ai.rotacao[29] == 10.0 && ai.rotacao[30] == -15.0, "degrau do giro");

    // sprite do texto: tamanho, centro e lados (momentos do alfa), posto no meio do canvas como `camada::titulo`
    let ClipeSobreposto::Titulo(x) = serde_json::from_value::<ClipeSobreposto>(texto.clone()).unwrap() else { unreachable!() };
    let (s, _) = bitmap_sprite(&titulo::pedido(&x.titulo, 0, TamanhoPx { w: W, h: H })).unwrap();
    let (scx, scy, smaior, smenor, _) =
        momentos(s.px.chunks_exact(4).enumerate().map(|(i, p)| ((i as u32 % s.w) as f64 + 0.5, (i as u32 / s.w) as f64 + 0.5, p[3] as f64)));
    let (x0, y0) = ((W as f64 * 0.5 - s.w as f64 * 0.5 + 0.5).floor(), (H as f64 * 0.5 - s.h as f64 * 0.5 + 0.5).floor());
    let (dx, dy) = (x0 + scx - W as f64 * 0.5, y0 + scy - H as f64 * 0.5);

    // miolo opaco de referência: texto com opacidade 1 (degrau de 0,9 a 1,4 s), imagem no começo (1)
    let (ref_t, ref_i) = (mancha(&q[ti + 33], 1).miolo, mancha(&q[ii], 2).miolo);
    // #30c030 e #60a0ff bem acima do fundo #404040 na luma (a escala da medida)
    assert!(ref_t > 60.0 && ref_i > 60.0, "verde {ref_t}, azul {ref_i} ({})", cap.versao);

    let (mut pior_px, mut pior_grau, mut pior_lado, mut pior_op) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut conferir = |nome: &str, k: usize, m: Mancha, e: (f64, f64, f64, f64, f64, f64)| {
        let (ex, ey, emaior, emenor, eang, eop) = e;
        let dp = (m.cx - ex).abs().max((m.cy - ey).abs());
        let da = dif_angulo(m.angulo, eang);
        let dl = (m.maior - emaior).abs().max((m.menor - emenor).abs());
        let dop = (m.miolo - eop).abs();
        assert!(dp <= 1.0, "{nome} ({}) slot {k}: centro ({:.2}, {:.2}) × ({ex:.2}, {ey:.2})", cap.versao, m.cx, m.cy);
        assert!(da <= 1.0, "{nome} ({}) slot {k}: ângulo {:.2}° × {eang:.2}°", cap.versao, m.angulo);
        assert!(dl <= 1.5, "{nome} ({}) slot {k}: lados {:.2} × {:.2} contra {emaior:.2} × {emenor:.2}", cap.versao, m.maior, m.menor);
        assert!(dop <= 0.03, "{nome} ({}) slot {k}: opacidade {:.3} × {eop:.3}", cap.versao, m.miolo);
        (pior_px, pior_grau, pior_lado, pior_op) = (pior_px.max(dp), pior_grau.max(da), pior_lado.max(dl), pior_op.max(dop));
    };
    for (k, quadro) in q.iter().enumerate() {
        if k >= ti && k < ti + tn {
            let j = k - ti;
            let (sc, th) = (at.escala[j], at.rotacao[j].to_radians());
            let (co, sn) = (th.cos(), th.sin());
            let e = (at.x[j] * W as f64 + sc * (co * dx - sn * dy), at.y[j] * H as f64 + sc * (sn * dx + co * dy), smaior * sc, smenor * sc, at.rotacao[j], at.opacidade[j]);
            let mut m = mancha(quadro, 1);
            m.miolo /= ref_t;
            conferir("texto", k, m, e);
        }
        if k >= ii && k < ii + in_ {
            let j = k - ii;
            let lado = ai.escala[j] * W as f64;
            let e = (ai.x[j] * W as f64, ai.y[j] * H as f64, lado, lado * IMG.1 as f64 / IMG.0 as f64, ai.rotacao[j], ai.opacidade[j]);
            let mut m = mancha(quadro, 2);
            m.miolo /= ref_i;
            conferir("imagem", k, m, e);
        }
    }
    eprintln!("curvas no render ({}): pior centro {pior_px:.2} px, ângulo {pior_grau:.2}°, lado {pior_lado:.2} px, opacidade {pior_op:.3}", cap.versao);
    // as curvas andaram de fato (a medida pegaria um motor que ignorasse qualquer uma delas)
    let faixa = |v: &[f64]| v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min);
    assert!(faixa(&at.x) * W as f64 > 40.0 && faixa(&at.y) * H as f64 > 60.0 && faixa(&at.rotacao) > 50.0 && faixa(&at.escala) > 0.4 && faixa(&at.opacidade) > 0.5);
    assert!(faixa(&ai.x) * W as f64 > 30.0 && faixa(&ai.y) * H as f64 > 50.0 && faixa(&ai.rotacao) > 20.0 && faixa(&ai.escala) * W as f64 > 30.0 && faixa(&ai.opacidade) > 0.5);
    let _ = std::fs::remove_dir_all(pasta);
}

// ------------------------------------------------------------------ volume

/// WAV PCM s16 estéreo 48 kHz: o canal esquerdo.
fn esquerdo(wav: &[u8]) -> Vec<i16> {
    assert_eq!((&wav[0..4], &wav[8..12]), (&b"RIFF"[..], &b"WAVE"[..]));
    let mut i = 12;
    while i + 8 <= wav.len() {
        let n = u32::from_le_bytes(wav[i + 4..i + 8].try_into().unwrap()) as usize;
        if &wav[i..i + 4] == b"fmt " {
            let (canais, taxa, bits) = (u16::from_le_bytes([wav[i + 10], wav[i + 11]]), u32::from_le_bytes(wav[i + 12..i + 16].try_into().unwrap()), u16::from_le_bytes([wav[i + 22], wav[i + 23]]));
            assert_eq!((canais, taxa, bits), (2, 48_000, 16));
        }
        if &wav[i..i + 4] == b"data" {
            return wav[i + 8..i + 8 + n].chunks_exact(4).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        }
        i += 8 + n + (n & 1);
    }
    panic!("WAV sem data")
}

/// Nível s16 de um nível 0..1 do `aevalsrc` (o ffmpeg arredonda ×32768).
fn s16(v: f64) -> f64 {
    (v * 32_768.0).round()
}

/// Som do trecho (fonte a 0,4, chaves em tempo de FONTE) e narração presa ao trecho (fonte a 0,2, chaves em tempo LOCAL)
/// tocando juntos, pela exportação inteira: cada amostra do PCM = soma das duas fontes × o ganho que `curva::amostrar`
/// dá no começo do quadro de 256 amostras dela (o `volume` com `eval=frame`), a até 2 de 32768.
#[test]
fn volume_animado_no_pcm() {
    let Some(cap) = fontes::cap() else { return };
    let pasta = fontes::pasta_temporaria("curvas-volume");
    let video = gerar(&cap, &pasta, "v.mkv", &["-f", "lavfi", "-i", "testsrc2=s=160x90:r=30:d=4", "-f", "lavfi", "-i", "aevalsrc=exprs=0.4:s=48000:c=mono:d=4",
        "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "ultrafast", "-g", "30", "-pix_fmt", "yuv420p", "-c:a", "pcm_s16le"]);
    let narr = gerar(&cap, &pasta, "n.wav", &["-f", "lavfi", "-i", "aevalsrc=exprs=0.2:s=48000:c=mono:d=3", "-c:a", "pcm_s16le"]);
    let tab = TabelaQuadros::do_decodificador(&cap, &video, &AtomicBool::new(false), &mut |_| {}).unwrap();
    let fv = quadros::fonte_info(&cap, &video, &"0".repeat(64), Arc::new(tab.clone()), None).unwrap();
    let fn_ = quadros::fonte_info(&cap, &narr, &"0".repeat(64), Arc::new(TabelaQuadros::de_pacotes(&cap, &narr).unwrap()), None).unwrap();
    assert!(fv.audio.is_some() && fn_.audio.is_some());

    let vol_trecho = json!([{ "t": 1.0, "v": 1.0, "interp": "linear" }, { "t": 1.8, "v": 0.25, "interp": "suave" }, { "t": 2.6, "v": 1.5, "interp": "degrau" }, { "t": 3.1, "v": 0.6 }]);
    let vol_narr = json!([{ "t": 0.0, "v": 0.0, "interp": "suave" }, { "t": 0.6, "v": 1.0, "interp": "linear" }, { "t": 1.2, "v": 0.3 }]);
    // trecho de 1,0 s a 3,5 s da fonte (75 slots); narração 0,4 s depois do começo dele, 1,5 s do arquivo (de 0,3 s)
    let mut c = aceite::trecho("c", "v", &tab, 30, 105, 1.0, json!([]));
    c["audio"]["volume"] = vol_trecho.clone();
    let narracao = json!({ "id": "n", "nome": "", "midia": "nm", "pos": { "ref": "clipe", "clipe": "c", "desloc": 0.4 }, "de_s": 0.3, "ate_s": 1.8,
                           "volume": vol_narr.clone(), "entrada_s": 0.0, "saida_s": 0.0 });
    let mut p = aceite::projeto(W, H, (FPS.num, FPS.den), vec![aceite::midia("v", "video", json!([])), aceite::midia("nm", "audio", json!([]))], vec![c],
        json!({ "audios": [{ "id": "a1", "nome": "", "papel": "narracao", "volume": 1.0, "mudo": false, "abaixa_original": false, "clipes": [narracao] }] }));
    p.saida.audio = true;
    let fontes_ = HashMap::from([("v".to_string(), fv), ("nm".to_string(), fn_)]);
    let midias: Vec<EstadoMidia> = Vec::new();
    let ctx = Contexto { fontes: &fontes_, cap: &cap, rastreios: &apoio::SemRastreios, pasta_projeto: Path::new("/nao/existe/projeto"), midias: &midias };
    let plano = motor::compilar(&p, &ctx, &aceite::alvo_final("cpu")).unwrap_or_else(|e| panic!("{e} ({})", cap.versao));
    let passo = plano.audio.clone().expect("passo de áudio");
    let destino = pasta.join("render");
    let (_, res) = executor::exportar(&cap, &plano, &destino, "2026-11-18T00:00:00Z").unwrap_or_else(|e| panic!("{e} ({})", cap.versao));
    let wav = std::fs::read(destino.join(&passo.saida)).unwrap();
    let l = esquerdo(&wav);
    assert_eq!(l.len(), 120_000, "{}", cap.versao);
    assert!(res.audio_sha256.is_some());

    // ganho de cada quadro de 256 amostras no começo dele: trecho com o zero em 1,0 s de fonte, narração em 0 do recorte
    let quadro = |i: usize| (i / 256 * 256) as f64 / 48_000.0;
    let (vt, vn) = (animavel(&vol_trecho), animavel(&vol_narr));
    let (ini_n, fim_n) = (19_200usize, 19_200 + 72_000);
    let tempos_t: Vec<f64> = (0..l.len()).map(|i| 1.0 + quadro(i)).collect();
    let tempos_n: Vec<f64> = (ini_n..fim_n).map(|i| quadro(i - ini_n)).collect();
    let (gt, gn) = (curva::amostrar(&vt, &tempos_t), curva::amostrar(&vn, &tempos_n));
    // à mão: linear no meio, suave a ¼, degrau segurando, depois da última
    let perto = |a: f64, b: f64| (a - b).abs() < 1e-12;
    assert!(perto(curva::avaliar(&vt, 1.4), 0.625) && perto(curva::avaliar(&vt, 2.0), 0.25 + 1.25 * 0.15625) && curva::avaliar(&vt, 3.0) == 1.5 && curva::avaliar(&vt, 3.4) == 0.6);
    assert!(perto(curva::avaliar(&vn, 0.15), 0.15625) && perto(curva::avaliar(&vn, 0.9), 0.65));
    let mut pior = 0.0f64;
    for (i, &v) in l.iter().enumerate() {
        let mut e = s16(0.4) * gt[i];
        if (ini_n..fim_n).contains(&i) {
            e += s16(0.2) * gn[i - ini_n];
        }
        let d = (v as f64 - e).abs();
        assert!(d <= 2.0, "({}) amostra {i}: {v} × {e:.1}", cap.versao);
        pior = pior.max(d);
    }
    eprintln!("volume animado no PCM ({}): pior diferença {pior:.1} de 32768", cap.versao);
    // os degraus e as pontas no PCM: antes da narração, durante (suave subindo), no degrau do trecho (3,0 s de fonte)
    let em = |t: f64| (t * 48_000.0) as usize;
    assert!((l[em(0.4)] as f64 - s16(0.4) * 0.625).abs() <= 2.0);
    assert!((l[em(2.1)] as f64 - s16(0.4) * 1.5).abs() <= 2.0, "degrau do trecho: {}", l[em(2.1)]);
    assert!(l[ini_n - 1] as f64 - s16(0.4) * curva::avaliar(&vt, 1.0 + quadro(ini_n - 1)) <= 2.0 && l[fim_n] as f64 - s16(0.4) * curva::avaliar(&vt, 1.0 + quadro(fim_n)) <= 2.0);
    let _ = std::fs::remove_dir_all(pasta);
}
