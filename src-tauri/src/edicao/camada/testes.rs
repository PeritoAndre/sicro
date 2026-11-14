//! Testes do rasterizador de ponta a ponta: mesmos bytes (duas execuções, 1 e N threads), cache, cancelamento,
//! tempo por quadro e, com ffmpeg (os dois binários: `SICRO_FFMPEG`), lista por slot em 29,97, máscara por quadro
//! lido sem sobra (o E1 do `a2/` com a cadeia montada à mão), cor e PNG lido de volta.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use serde_json::{json, Value};

use super::formas::{Primitiva, QuadroCamada, Segmento};
use super::*;
use crate::edicao::cor;
use crate::edicao::ffmpeg::args;
use crate::edicao::ffmpeg::exec::{self, Comando};
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::geometria::{self, Afim};
use crate::edicao::modelo::{Caixa, Efeito, Faixa, Racional};
use crate::edicao::motor::plano::{ImagemCamada, QuadroItem};
use crate::edicao::testes::{fontes, medir};

/// Até 1 ms por quadro numa camada recortada de 304×224 (A13: 0,47 ms). Medido com folga enquanto outros
/// agentes usam a máquina; o orquestrador roda de novo com a máquina livre e `FOLGA` = 1.
const LIMITE_MS_POR_QUADRO: f64 = 1.0;
const FOLGA: f64 = 3.0;

fn efeito(v: Value) -> Efeito {
    serde_json::from_value(v).expect("efeito")
}

fn base(tipo: &str, id: &str) -> Value {
    json!({ "tipo": tipo, "id": id, "ligado": true, "nome": "", "ativo": null })
}

fn com(mut v: Value, extra: Value) -> Value {
    for (k, x) in extra.as_object().unwrap() {
        v[k] = x.clone();
    }
    v
}

fn estilo(fonte: &str, caixa: bool) -> Value {
    json!({ "fonte": fonte, "negrito": true, "italico": false, "tamanho": 0.05, "cor": "#ffffff", "alinhamento": "centro",
            "caixa": if caixa { json!({ "cor": "#0000008c", "margem": 0.01, "raio": 0.005 }) } else { Value::Null }, "contorno": null })
}

fn quadro(q: u32, canvas: TamanhoPx, rastreios: &[(&str, Caixa)]) -> QuadroCamada {
    QuadroCamada {
        q: q as u64,
        t: q as f64 / 30.0,
        pts: Some(q as i64 * 512),
        a: Afim::IDENTIDADE,
        canvas,
        rastreios: rastreios.iter().map(|(k, c)| (k.to_string(), *c)).collect(),
        indice: Some(q as u64),
        programa_s: q as f64 / 30.0,
    }
}

fn item(desenho: Desenho, de: u32, quadros: Vec<QuadroCamada>) -> ItemCamada {
    ItemCamada { desenho, de, quadros: quadros.into_iter().map(|quadro| QuadroItem { quadro, transformacao: None }).collect() }
}

fn pedido(tipo: TipoCamada, caixa_px: [u32; 4], n: u32, itens: Vec<ItemCamada>, imagens: Vec<ImagemCamada>) -> PedidoCamada {
    let mut p = PedidoCamada { id: "c000_anot".into(), tipo, caixa_px, n, hash: String::new(), itens, imagens };
    p.hash = hash_do_pedido(&p);
    p
}

/// Caixa em px → 0..1.
fn caixa_px(x: f64, y: f64, w: f64, h: f64, t: TamanhoPx) -> Caixa {
    Caixa { x: x / t.w as f64, y: y / t.h as f64, w: w / t.w as f64, h: h / t.h as f64 }
}

/// Imagem RGBA 40×30 em degradê, gravada em `pasta`.
fn imagem_de_teste(pasta: &Path) -> ImagemCamada {
    let mut px = Vec::new();
    for y in 0..30u32 {
        for x in 0..40u32 {
            px.extend_from_slice(&[(x * 6) as u8, (y * 8) as u8, 200, if x < 5 { 0 } else { 255 }]);
        }
    }
    let png = Bitmap { w: 40, h: 30, canais: 4, px }.png().unwrap();
    let caminho = pasta.join("logo.png");
    std::fs::write(&caminho, &png).unwrap();
    ImagemCamada { midia: "logo".into(), sha256: hex(&Sha256::digest(&png)), caminho, tamanho: TamanhoPx { w: 40, h: 30 } }
}

/// Anotação com tudo da Fase 0 (destaque com rótulo, texto, carimbo que muda a cada slot, imagem sobreposta
/// girada) e máscara por quadro lido com borda suave, em 30 quadros.
fn pedidos_mistos(pasta_img: &Path) -> (PedidoCamada, PedidoCamada) {
    let canvas = TamanhoPx { w: 640, h: 360 };
    let n = 30;
    let r1 = |q: u32| caixa_px(100.0 + 4.0 * q.min(19) as f64, 120.0, 80.0, 60.0, canvas);
    let destaque = efeito(com(base("destaque", "d1"), json!({ "forma": "circulo", "rastreio": "r1",
        "traco": { "cor": "#ffcc00", "espessura": 0.008, "contorno": true }, "tamanho": 0.12, "angulo_graus": 45.0, "margem": 0.05,
        "rotulo": { "texto": "Veículo 1", "estilo": estilo("sans", true), "lado": "cima" } })));
    let texto = efeito(com(base("texto", "t1"), json!({ "conteudo": "Câmera 2 — saída", "estilo": estilo("serifa", true),
        "lugar": { "x": 0.5, "y": 0.95, "h": "centro", "v": "base" }, "largura_max": 0.8 })));
    let carimbo = efeito(com(base("carimbo", "k1"), json!({ "mostra": "tempo_fonte", "formato": "hms_ms", "relogio": null,
        "estilo": estilo("mono", true), "lugar": { "x": 0.98, "y": 0.03, "h": "dir", "v": "topo" } })));
    let img = imagem_de_teste(pasta_img);
    let q = |i: u32| quadro(i, canvas, &[("r1", r1(i))]);
    let mut imagem = item(Desenho::Imagem { midia: "logo".into() }, 0, (0..n).map(q).collect());
    for (k, qi) in imagem.quadros.iter_mut().enumerate() {
        qi.transformacao = Some(TransformacaoAvaliada { x: 0.8, y: 0.3, escala: 0.15, rotacao: 15.0 + k as f64, opacidade: 0.7 });
    }
    let anot = pedido(
        TipoCamada::Anotacao,
        [0, 0, 640, 360],
        n,
        vec![
            item(Desenho::Efeito { efeito: destaque }, 0, (0..n).map(q).collect()),
            item(Desenho::Efeito { efeito: texto }, 5, (5..25).map(q).collect()),
            item(Desenho::Efeito { efeito: carimbo }, 0, (0..n).map(q).collect()),
            imagem,
        ],
        vec![img],
    );
    let fonte = TamanhoPx { w: 320, h: 180 };
    let ocultar = efeito(com(base("ocultar", "o1"), json!({ "modo": "solido", "alvo": { "rastreio": "r1", "forma": "elipse", "vertices": null,
        "margem": 0.1, "borda_suave": 0.2, "inverter": false }, "intensidade": 1.0, "cor": "#000000ff", "conteudo": "rosto", "motivo": "" })));
    let qf: Vec<QuadroCamada> = (0..n).map(|i| quadro(i, fonte, &[("r1", caixa_px(50.0 + 2.0 * i as f64, 60.0, 40.0, 30.0, fonte))])).collect();
    let prims: Vec<Vec<Primitiva>> = qf.iter().map(|q| formas::primitivas(&ocultar, q, &formas_falsas()).unwrap()).collect();
    let caixa = mascara::uniao_px(&prims, fonte).unwrap();
    let masc = pedido(TipoCamada::MascaraFonte, caixa, n, vec![item(Desenho::Efeito { efeito: ocultar }, 0, qf)], Vec::new());
    (anot, masc)
}

/// Fonte de sprites que não deve ser chamada (efeitos sem texto).
struct SemSprites;
impl texto::FonteSprites for SemSprites {
    fn sprite(&self, _: &texto::PedidoSprite) -> Result<texto::Sprite> {
        unreachable!()
    }
    fn atlas(&self, _: &crate::edicao::modelo::EstiloTexto, _: u32) -> Result<texto::Atlas> {
        unreachable!()
    }
    fn imagem(&self, _: &str) -> Option<TamanhoPx> {
        None
    }
}

fn formas_falsas() -> SemSprites {
    SemSprites
}

/// Todos os arquivos (caminho relativo → bytes) de uma pasta, recursivo.
fn arquivos(raiz: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut v = BTreeMap::new();
    let mut pilha = vec![raiz.to_path_buf()];
    while let Some(d) = pilha.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                pilha.push(p);
            } else {
                v.insert(p.strip_prefix(raiz).unwrap().to_path_buf(), std::fs::read(&p).unwrap());
            }
        }
    }
    v
}

#[test]
fn mesmos_bytes_em_duas_execucoes_e_com_1_e_n_threads() {
    let img = fontes::pasta_temporaria("p05-img");
    let (anot, masc) = pedidos_mistos(&img);
    let nao = AtomicBool::new(false);
    let rodar = |cache: &Path| {
        let a = rasterizar(&anot, cache, &nao).unwrap();
        let m = rasterizar(&masc, cache, &nao).unwrap();
        (a, m)
    };
    let c1 = fontes::pasta_temporaria("p05-det-a");
    let c2 = fontes::pasta_temporaria("p05-det-b");
    let c3 = fontes::pasta_temporaria("p05-det-c");
    let (a1, m1) = rodar(&c1);
    let (a2, m2) = rodar(&c2);
    let um = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
    let (a3, m3) = um.install(|| rodar(&c3));
    assert!(rayon::current_num_threads() > 1, "a máquina tem de rodar o caso de N threads");
    for (x, y) in [(&a1, &a2), (&a1, &a3), (&m1, &m2), (&m1, &m3)] {
        assert_eq!(x.arquivos, y.arquivos);
        assert_eq!((x.lista.clone(), x.fixa), (y.lista.clone(), y.fixa));
    }
    let (f1, f2, f3) = (arquivos(&c1), arquivos(&c2), arquivos(&c3));
    assert_eq!(f1, f2);
    assert_eq!(f1, f3);
    // sprites, atlas, camada e máscara
    for pasta in ["sprites", "atlas", "camadas", "mascaras"] {
        assert!(f1.keys().any(|k| k.starts_with(pasta)), "{pasta} vazia");
    }
    // de novo no mesmo cache: só lê (nenhum arquivo regravado)
    let datas = |p: &Path| -> Vec<std::time::SystemTime> {
        let mut v: Vec<_> = std::fs::read_dir(p).unwrap().flatten().map(|e| (e.file_name(), e.metadata().unwrap().modified().unwrap())).collect();
        v.sort();
        v.into_iter().map(|x| x.1).collect()
    };
    let antes = datas(&a1.pasta);
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert_eq!(rasterizar(&anot, &c1, &nao).unwrap(), a1);
    assert_eq!(datas(&a1.pasta), antes);
    // o carimbo muda a cada slot: um PNG por slot; a máscara anda com o rastreio
    assert_eq!(a1.arquivos.len(), 30);
    assert_eq!(m1.arquivos.len(), 30);
    let lista = std::fs::read_to_string(a1.pasta.join(LISTA)).unwrap();
    assert_eq!(lista.lines().count(), 31);
    for c in [c1, c2, c3, img] {
        let _ = std::fs::remove_dir_all(c);
    }
}

#[test]
fn slots_iguais_dividem_o_png_e_camada_que_nao_muda_e_fixa() {
    let canvas = TamanhoPx { w: 320, h: 180 };
    let d = efeito(com(base("destaque", "d1"), json!({ "forma": "retangulo", "rastreio": "r1",
        "traco": { "cor": "#ff0000", "espessura": 0.02, "contorno": false }, "tamanho": 0.1, "angulo_graus": 0.0, "margem": 0.0, "rotulo": null })));
    let r = |q: u32| caixa_px(if q < 5 { 40.0 } else { 80.0 }, 40.0, 40.0, 40.0, canvas);
    // slots 0..4 iguais, 5..9 iguais, 10..11 sem nada
    let p = pedido(
        TipoCamada::Anotacao,
        [0, 0, 320, 180],
        12,
        vec![item(Desenho::Efeito { efeito: d.clone() }, 0, (0..10).map(|q| quadro(q, canvas, &[("r1", r(q))])).collect())],
        Vec::new(),
    );
    let cache = fontes::pasta_temporaria("p05-iguais");
    let c = rasterizar(&p, &cache, &AtomicBool::new(false)).unwrap();
    assert_eq!(c.arquivos.iter().map(|a| a.arquivo.as_str()).collect::<Vec<_>>(), ["000000.png", "000005.png", "000010.png"]);
    let lista = std::fs::read_to_string(c.pasta.join(LISTA)).unwrap();
    let nomes: Vec<&str> = lista.lines().skip(1).collect();
    assert_eq!(nomes.len(), 12);
    assert_eq!(nomes[4], "file '000000.png'");
    assert_eq!(nomes[9], "file '000005.png'");
    assert_eq!(nomes[11], "file '000010.png'");
    assert!(!lista.contains("duration"));
    // um desenho só: fixa, sem lista
    let p = pedido(
        TipoCamada::Anotacao,
        [0, 0, 320, 180],
        8,
        vec![item(Desenho::Efeito { efeito: d }, 0, (0..8).map(|q| quadro(q, canvas, &[("r1", r(0))])).collect())],
        Vec::new(),
    );
    let c = rasterizar(&p, &cache, &AtomicBool::new(false)).unwrap();
    assert!(c.fixa && c.lista.is_none() && c.arquivos.len() == 1);
    let _ = std::fs::remove_dir_all(cache);
}

#[test]
fn hash_ignora_rotulo_e_caminho_e_segue_o_conteudo() {
    let img = fontes::pasta_temporaria("p05-hash");
    let (a, m) = pedidos_mistos(&img);
    let mut b = a.clone();
    b.id = "c999_anot".into();
    b.hash = "outro".into();
    b.imagens[0].caminho = PathBuf::from("/outro/lugar.png");
    assert_eq!(hash_do_pedido(&a), hash_do_pedido(&b));
    assert_eq!(a.hash.len(), 64);
    // qualquer entrada que muda pixel muda o hash
    type Muda = fn(&mut PedidoCamada);
    let mudancas: [(&str, Muda); 16] = [
        ("caixa_px", |p| p.caixa_px[0] += 2),
        ("n", |p| p.n += 1),
        ("tipo", |p| p.tipo = TipoCamada::MascaraCanvas),
        ("sha256 da imagem", |p| p.imagens[0].sha256 = "0".repeat(64)),
        ("tamanho da imagem", |p| p.imagens[0].tamanho.w += 1),
        ("altura do canvas", |p| p.itens[1].quadros[3].quadro.canvas.h += 2),
        ("largura do canvas", |p| p.itens[1].quadros[3].quadro.canvas.w += 2),
        ("rastreio", |p| p.itens[0].quadros[7].quadro.rastreios.get_mut("r1").unwrap().x += 1e-9),
        ("A", |p| p.itens[0].quadros[7].quadro.a.0[4] += 1e-9),
        ("t (carimbo)", |p| p.itens[2].quadros[2].quadro.t += 0.001),
        ("de", |p| p.itens[1].de += 1),
        ("um quadro a menos", |p| drop(p.itens[1].quadros.pop())),
        ("ordem dos itens", |p| p.itens.swap(0, 1)),
        ("transformacao", |p| p.itens[3].quadros[0].transformacao.as_mut().unwrap().opacidade = 0.5),
        ("efeito", |p| {
            let Desenho::Efeito { efeito: Efeito::Texto(t) } = &mut p.itens[1].desenho else { panic!() };
            t.conteudo.push('.');
        }),
        ("estilo do efeito", |p| {
            let Desenho::Efeito { efeito: Efeito::Texto(t) } = &mut p.itens[1].desenho else { panic!() };
            t.estilo.fonte = crate::edicao::modelo::Fonte::Mono;
        }),
    ];
    let mut vistos = std::collections::BTreeSet::from([hash_do_pedido(&a)]);
    for (nome, muda) in mudancas {
        let mut c = a.clone();
        muda(&mut c);
        assert!(vistos.insert(hash_do_pedido(&c)), "{nome}: hash repetido");
    }
    // versão do rasterizador, fontes e modelos de texto
    let (f, t) = (texto::impressao_das_fontes(), texto::impressao_dos_modelos());
    assert_eq!(hash_com(&a, VERSAO_RASTERIZADOR, f, t), hash_do_pedido(&a));
    assert!(vistos.insert(hash_com(&a, VERSAO_RASTERIZADOR + 1, f, t)), "versão");
    assert!(vistos.insert(hash_com(&a, VERSAO_RASTERIZADOR, &"0".repeat(64), t)), "fontes");
    assert!(vistos.insert(hash_com(&a, VERSAO_RASTERIZADOR, f, &"0".repeat(64))), "modelos");
    assert!(vistos.insert(hash_do_pedido(&m)));
    // a ordem das imagens não muda o hash
    let mut d = a.clone();
    d.imagens.push(ImagemCamada { midia: "a".into(), sha256: "1".repeat(64), caminho: PathBuf::from("x.png"), tamanho: TamanhoPx { w: 2, h: 2 } });
    let mut e = d.clone();
    e.imagens.reverse();
    assert_eq!(hash_do_pedido(&d), hash_do_pedido(&e));
    let _ = std::fs::remove_dir_all(img);
}

#[test]
fn cache_estragado_e_refeito_e_caixa_impar_e_recusada() {
    let img = fontes::pasta_temporaria("p05-estragado-img");
    let (anot, masc) = pedidos_mistos(&img);
    let nao = AtomicBool::new(false);
    let cache = fontes::pasta_temporaria("p05-estragado");
    let a = rasterizar(&anot, &cache, &nao).unwrap();
    let m = rasterizar(&masc, &cache, &nao).unwrap();
    let bons = (arquivos(&a.pasta), arquivos(&m.pasta));
    type Estrago = fn(&Path);
    let estragos: [(&str, Estrago); 6] = [
        ("PNG de 0 byte", |p| std::fs::write(p.join("000003.png"), b"").unwrap()),
        ("PNG cortado", |p| {
            let b = std::fs::read(p.join("000004.png")).unwrap();
            std::fs::write(p.join("000004.png"), &b[..b.len() / 2]).unwrap();
        }),
        ("PNG apagado", |p| std::fs::remove_file(p.join("000005.png")).unwrap()),
        ("lista cortada", |p| {
            let l = std::fs::read_to_string(p.join(LISTA)).unwrap();
            std::fs::write(p.join(LISTA), &l[..l.len() - 18]).unwrap();
        }),
        ("camada.json apagado", |p| std::fs::remove_file(p.join(PRONTA)).unwrap()),
        ("camada.json ilegível", |p| std::fs::write(p.join(PRONTA), b"{").unwrap()),
    ];
    for (nome, estraga) in estragos {
        for (pedido, pronta, bom) in [(&anot, &a, &bons.0), (&masc, &m, &bons.1)] {
            estraga(&pronta.pasta);
            let de_novo = rasterizar(pedido, &cache, &nao).unwrap_or_else(|e| panic!("{nome}: {e}"));
            assert_eq!(&de_novo, pronta, "{nome}");
            assert_eq!(&arquivos(&pronta.pasta), bom, "{nome}: a pasta não voltou aos mesmos bytes");
        }
    }
    // nada de sobra de pasta temporária
    for d in ["camadas", "mascaras"] {
        assert_eq!(std::fs::read_dir(cache.join(d)).unwrap().count(), 1, "{d}");
    }
    // camada ou contagem absurda: recusada antes de alocar
    for (caixa, n) in [([0, 0, 1 << 14, 1 << 14], 30), ([0, 0, 640, 360], u32::MAX), ([0, 0, u32::MAX - 1, u32::MAX - 1], 30)] {
        let mut g = anot.clone();
        g.caixa_px = caixa;
        g.n = n;
        let e = rasterizar(&g, &cache, &nao).unwrap_err().to_string();
        assert!(e.contains("item_invalido"), "{e}");
    }
    // posição ou lado ímpar: recusado (deslocaria o croma no overlay)
    for k in 0..4 {
        let mut imp = anot.clone();
        imp.caixa_px = [2, 2, 100, 100];
        imp.caixa_px[k] += 1;
        let e = rasterizar(&imp, &cache, &nao).unwrap_err().to_string();
        assert!(e.contains("item_invalido"), "{e}");
    }
    for c in [cache, img] {
        let _ = std::fs::remove_dir_all(c);
    }
}

#[test]
fn cancelar_nao_deixa_pasta_e_todos_em_ordem() {
    let img = fontes::pasta_temporaria("p05-todos-img");
    let (anot, masc) = pedidos_mistos(&img);
    let cache = fontes::pasta_temporaria("p05-cancelar");
    let e = rasterizar(&anot, &cache, &AtomicBool::new(true)).unwrap_err();
    assert!(e.to_string().contains(crate::edicao::executar::ERRO_CANCELADO));
    let sobras: Vec<_> = ["camadas", "mascaras"]
        .iter()
        .filter_map(|d| std::fs::read_dir(cache.join(d)).ok())
        .flatten()
        .flatten()
        .collect();
    assert!(sobras.is_empty(), "{sobras:?}");
    let mut contagem = Vec::new();
    let v = rasterizar_todos(&[masc.clone(), anot.clone(), masc.clone()], &cache, &AtomicBool::new(false), &mut |n| contagem.push(n)).unwrap();
    assert_eq!(contagem, [1, 2, 3]);
    assert_eq!(v.len(), 3);
    assert_eq!(v[0], v[2]);
    assert_eq!(v[1].pasta.parent().unwrap().file_name().unwrap(), "camadas");
    assert_eq!(v[0].pasta.parent().unwrap().file_name().unwrap(), "mascaras");
    for c in [cache, img] {
        let _ = std::fs::remove_dir_all(c);
    }
}

#[test]
fn ate_1_ms_por_quadro_em_camada_recortada_de_304x224() {
    // o A13: elipse com traço e contorno, cada quadro diferente, numa camada de 304×224
    let canvas = TamanhoPx { w: 1920, h: 1080 };
    let n = 240u32;
    let d = efeito(com(base("destaque", "d1"), json!({ "forma": "elipse", "rastreio": "r1",
        "traco": { "cor": "#ff2d2d", "espessura": 0.006, "contorno": true }, "tamanho": 0.1, "angulo_graus": 0.0, "margem": 0.0, "rotulo": null })));
    // semieixos √2/2 × lado: caixa de 190×135 vira elipse de ~269×191; 240 posições diferentes dentro de 304×224
    let quadros: Vec<QuadroCamada> =
        (0..n).map(|q| quadro(q, canvas, &[("r1", caixa_px(500.0 + (q % 16) as f64, 400.0 + (q / 16) as f64, 190.0, 135.0, canvas))])).collect();
    let envs: Vec<Caixa> = quadros.iter().filter_map(|q| formas::envolvente(&formas::primitivas(&d, q, &SemSprites).unwrap(), canvas)).collect();
    let u = envs.iter().fold(envs[0], |a, b| {
        let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
        Caixa { x: x0, y: y0, w: (a.x + a.w).max(b.x + b.w) - x0, h: (a.y + a.h).max(b.y + b.h) - y0 }
    });
    let caixa = geometria::caixa_px_par(u, canvas);
    assert!(caixa[2] <= 304 && caixa[3] <= 224, "{caixa:?}");
    let caixa = [caixa[0], caixa[1], 304, 224];
    let p = pedido(TipoCamada::Anotacao, caixa, n, vec![item(Desenho::Efeito { efeito: d }, 0, quadros)], Vec::new());
    let medir_com = |threads: usize| {
        let cache = fontes::pasta_temporaria("p05-tempo");
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        let t = Instant::now();
        let c = pool.install(|| rasterizar(&p, &cache, &AtomicBool::new(false))).unwrap();
        let ms = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
        assert_eq!(c.arquivos.len(), n as usize);
        let _ = std::fs::remove_dir_all(cache);
        ms
    };
    let um = medir_com(1);
    let todos = medir_com(rayon::current_num_threads());
    eprintln!("camada 304×224: {um:.3} ms/quadro em 1 thread; {todos:.3} ms/quadro com {} threads", rayon::current_num_threads());
    // o limite é o do binário otimizado: no perfil de teste (sem otimização) tiny-skia, png e sha2 rodam ~40× mais devagar
    if cfg!(debug_assertions) {
        eprintln!("perfil sem otimização: limite de {LIMITE_MS_POR_QUADRO} ms não conferido");
        return;
    }
    assert!(um <= LIMITE_MS_POR_QUADRO * FOLGA, "{um:.3} ms por quadro (limite {LIMITE_MS_POR_QUADRO} × folga {FOLGA})");
}

#[test]
fn carimbo_desenha_cada_caractere_com_a_celula_dele() {
    let canvas = TamanhoPx { w: 640, h: 360 };
    let cache = fontes::pasta_temporaria("p05-carimbo");
    let sprites = texto::SpritesEmCache { cache: &cache, imagens: &[] };
    for (mostra, caixa) in [("tempo_fonte", true), ("quadro", false), ("tempo_programa", true)] {
        let ef = efeito(com(base("carimbo", "k1"), json!({ "mostra": mostra, "formato": "hms_ms", "relogio": null,
            "estilo": estilo("mono", caixa), "lugar": { "x": 0.97, "y": 0.05, "h": "dir", "v": "topo" } })));
        let mut q = quadro(7, canvas, &[]);
        q.t = 5025.6789;
        q.programa_s = 754.321;
        q.indice = Some(3_120_459);
        let p = formas::primitivas(&ef, &q, &sprites).unwrap();
        let Primitiva::Atlas { src, celula, glifos, texto, lugar, altura } = &p[0] else { panic!("{p:?}") };
        let esperado = match mostra {
            "tempo_fonte" => " Original 01:23:45,679 ",
            "quadro" => "Quadro 3120459",
            _ => " Vídeo 00:12:34,321 ",
        };
        assert_eq!(texto, esperado);
        let b = formas::desenhar(&p, canvas, [0, 0, 640, 360], &cache, &[]).unwrap();
        let atlas = image::open(cache.join(src)).unwrap().to_rgba8();
        let (x, y, w, h, e) = formas::caixa_atlas(*celula, texto, *lugar, *altura, canvas);
        assert_eq!(e, 1.0);
        assert_eq!((w, h), ((texto.chars().count() as u32 * celula.w) as f64, celula.h as f64));
        // encostado à direita em 0,97 × 640
        assert_eq!(x + w, (0.97f64 * 640.0 + 0.5).floor());
        let (x, y) = (x as u32, y as u32);
        let mut com_tinta = 0;
        for py in 0..360u32 {
            for px in 0..640u32 {
                let d = &b.px[((py * 640 + px) * 4) as usize..][..4];
                if px < x || px >= x + w as u32 || py < y || py >= y + h as u32 {
                    assert_eq!(d[3], 0, "({px}, {py}) fora do carimbo");
                    continue;
                }
                let k = ((px - x) / celula.w) as usize;
                let i = glifos.chars().position(|g| g == texto.chars().nth(k).unwrap()).unwrap() as u32;
                let a = atlas.get_pixel(i * celula.w + (px - x) % celula.w, py - y).0;
                assert_eq!(d[3], a[3], "({px}, {py}) caractere {k}");
                if a[3] > 0 {
                    let tol = if a[3] == 255 { 0 } else { 255u32.div_ceil(a[3] as u32) };
                    assert!(d.iter().zip(&a).all(|(u, v)| u.abs_diff(*v) as u32 <= tol), "({px}, {py}): {d:?} × {a:?}");
                    com_tinta += (a[0] > 200 && a[3] > 200) as u32;
                }
            }
        }
        assert!(com_tinta > 300, "{mostra}: {com_tinta}");
    }
    let _ = std::fs::remove_dir_all(cache);
}

#[test]
fn imagem_sobreposta_no_lugar_tamanho_giro_e_opacidade() {
    let pasta = fontes::pasta_temporaria("p05-sobreposta");
    // 40×30: metade esquerda vermelha, direita azul, canto de cima à esquerda (5×5) verde
    let mut px = Vec::new();
    for y in 0..30u32 {
        for x in 0..40u32 {
            px.extend_from_slice(if x < 5 && y < 5 { &[0, 255, 0, 255] } else if x < 20 { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
        }
    }
    let png = Bitmap { w: 40, h: 30, canais: 4, px }.png().unwrap();
    std::fs::write(pasta.join("i.png"), &png).unwrap();
    let img = ImagemCamada { midia: "i".into(), sha256: hex(&Sha256::digest(&png)), caminho: pasta.join("i.png"), tamanho: TamanhoPx { w: 40, h: 30 } };
    let canvas = TamanhoPx { w: 640, h: 360 };
    let rodar = |rotacao: f64, opacidade: f64, caixa: [u32; 4]| {
        let mut it = item(Desenho::Imagem { midia: "i".into() }, 0, vec![quadro(0, canvas, &[])]);
        it.quadros[0].transformacao = Some(TransformacaoAvaliada { x: 0.5, y: 0.5, escala: 0.25, rotacao, opacidade });
        let c = rasterizar(&pedido(TipoCamada::Anotacao, caixa, 1, vec![it], vec![img.clone()]), &pasta.join("cache"), &AtomicBool::new(false)).unwrap();
        assert!(c.fixa);
        let lido = image::open(c.pasta.join(&c.arquivos[0].arquivo)).unwrap().to_rgba8();
        assert_eq!((lido.width(), lido.height()), (caixa[2], caixa[3]));
        move |x: u32, y: u32| lido.get_pixel(x - caixa[0], y - caixa[1]).0
    };
    // sem giro: 160×120 px centrados → x 240..400, y 120..240
    let p = rodar(0.0, 1.0, [0, 0, 640, 360]);
    assert_eq!(p(250, 130), [0, 255, 0, 255]);
    assert_eq!(p(300, 200), [255, 0, 0, 255]);
    assert_eq!(p(380, 200), [0, 0, 255, 255]);
    assert_eq!(p(242, 236), [255, 0, 0, 255]);
    assert_eq!(p(398, 122), [0, 0, 255, 255]);
    for (x, y) in [(237, 180), (402, 180), (320, 117), (320, 242)] {
        assert_eq!(p(x, y)[3], 0, "({x}, {y}) fora da imagem");
    }
    // 90° no sentido horário: 120×160 px → x 260..380, y 100..260; o canto verde vai para cima à direita
    let p = rodar(90.0, 1.0, [200, 80, 240, 200]);
    assert_eq!(p(370, 110), [0, 255, 0, 255]);
    assert_eq!(p(320, 150), [255, 0, 0, 255]);
    assert_eq!(p(320, 230), [0, 0, 255, 255]);
    assert_eq!(p(264, 104), [255, 0, 0, 255]);
    for (x, y) in [(257, 180), (383, 180), (320, 97), (320, 263)] {
        assert_eq!(p(x, y)[3], 0, "({x}, {y}) fora da imagem girada");
    }
    // opacidade 0,5: alfa pela metade, cor inteira (alfa reto)
    let p = rodar(0.0, 0.5, [0, 0, 640, 360]);
    let m = p(300, 200);
    assert!(m[3].abs_diff(128) <= 1 && m[0] >= 253 && m[1] <= 2 && m[2] <= 2, "{m:?}");
    let _ = std::fs::remove_dir_all(pasta);
}

#[test]
fn imagem_muito_reduzida_sai_filtrada() {
    let pasta = fontes::pasta_temporaria("p05-reduzida");
    // listras verticais de 3 px (preto e branco) em 1280×960, mostradas com 200 px de largura (6,4× menor)
    let (w, h) = (1280u32, 960u32);
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..h {
        for x in 0..w {
            let v = if (x / 3) % 2 == 0 { 0 } else { 255 };
            px.extend_from_slice(&[v, v, v, 255]);
        }
    }
    let png = Bitmap { w, h, canais: 4, px }.png().unwrap();
    std::fs::write(pasta.join("l.png"), &png).unwrap();
    let img = ImagemCamada { midia: "l".into(), sha256: hex(&Sha256::digest(&png)), caminho: pasta.join("l.png"), tamanho: TamanhoPx { w, h } };
    let canvas = TamanhoPx { w: 640, h: 360 };
    let medir_faixa = |b: &Bitmap, x0: u32, x1: u32, y0: u32, y1: u32| {
        let (mut lo, mut hi) = (255u8, 0u8);
        for y in y0..y1 {
            for x in x0..x1 {
                let p = &b.px[((y * b.w + x) * 4) as usize..][..4];
                assert_eq!(p[3], 255, "({x}, {y})");
                lo = lo.min(p[0]);
                hi = hi.max(p[0]);
            }
        }
        (lo, hi)
    };
    // sobreposta (transformação) e como efeito `imagem` (sprite): miolo cinza médio, sem listras falsas
    let mut it = item(Desenho::Imagem { midia: "l".into() }, 0, vec![quadro(0, canvas, &[])]);
    it.quadros[0].transformacao = Some(TransformacaoAvaliada { x: 0.5, y: 0.5, escala: 200.0 / 640.0, rotacao: 0.0, opacidade: 1.0 });
    let c = rasterizar(&pedido(TipoCamada::Anotacao, [0, 0, 640, 360], 1, vec![it], vec![img.clone()]), &pasta.join("cache"), &AtomicBool::new(false)).unwrap();
    let lido = image::open(c.pasta.join(&c.arquivos[0].arquivo)).unwrap().to_rgba8();
    let b = Bitmap { w: 640, h: 360, canais: 4, px: lido.into_raw() };
    let (lo, hi) = medir_faixa(&b, 224, 416, 108, 252);
    eprintln!("sobreposta 6,4× menor: {lo}..{hi}");
    assert!(lo >= 96 && hi <= 160, "sobreposta: {lo}..{hi}");
    let ef = efeito(com(base("imagem", "i1"), json!({ "midia": "l", "lugar": { "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" }, "largura": 200.0 / 640.0, "opacidade": 1.0 })));
    let sprites = texto::SpritesEmCache { cache: &pasta, imagens: std::slice::from_ref(&img) };
    let p = formas::primitivas(&ef, &quadro(0, canvas, &[]), &sprites).unwrap();
    let b = formas::desenhar(&p, canvas, [0, 0, 640, 360], &pasta, std::slice::from_ref(&img)).unwrap();
    let (lo, hi) = medir_faixa(&b, 224, 416, 108, 252);
    eprintln!("efeito imagem 6,4× menor: {lo}..{hi}");
    assert!(lo >= 96 && hi <= 160, "efeito: {lo}..{hi}");
    // as bordas ficam no lugar: 200×150 centrados → x 220..420, y 105..255
    for (x, y) in [(218, 180), (422, 180), (320, 103), (320, 257)] {
        assert_eq!(b.px[((y * 640 + x) * 4 + 3) as usize], 0, "({x}, {y}) fora da imagem");
    }
    for (x, y) in [(221, 180), (418, 180), (320, 106), (320, 253)] {
        assert_eq!(b.px[((y * 640 + x) * 4 + 3) as usize], 255, "({x}, {y}) dentro da imagem");
    }
    let _ = std::fs::remove_dir_all(pasta);
}

#[test]
fn imagem_com_orientacao_do_exif_sai_em_pe_e_tamanho_errado_e_recusado() {
    let pasta = fontes::pasta_temporaria("p05-exif");
    // 40×24 deitada: metade esquerda vermelha, direita azul; EXIF 6 = girar 90° no sentido horário para exibir
    let mut rgb = Vec::new();
    for _ in 0..24u32 {
        for x in 0..40u32 {
            rgb.extend_from_slice(if x < 20 { &[255u8, 0, 0] } else { &[0u8, 0, 255] });
        }
    }
    let mut jpg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 95).encode(&rgb, 40, 24, image::ExtendedColorType::Rgb8).unwrap();
    assert_eq!(&jpg[..2], &[0xff, 0xd8]);
    // APP1 "Exif" com um IFD de uma entrada: Orientation (0x0112) = 6
    let exif: [u8; 36] = [
        0xff, 0xe1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 0x2a, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
    ];
    let mut com_exif = jpg[..2].to_vec();
    com_exif.extend_from_slice(&exif);
    com_exif.extend_from_slice(&jpg[2..]);
    // extensão errada de propósito: a leitura é pelo conteúdo
    let caminho = pasta.join("foto.png");
    std::fs::write(&caminho, &com_exif).unwrap();
    let canvas = TamanhoPx { w: 320, h: 180 };
    let img = |tamanho: TamanhoPx| ImagemCamada { midia: "f".into(), sha256: hex(&Sha256::digest(&com_exif)), caminho: caminho.clone(), tamanho };
    let ef = efeito(com(base("imagem", "i1"), json!({ "midia": "f", "lugar": { "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" }, "largura": 0.15, "opacidade": 1.0 })));
    // em pé: 24×40 → 48×80 px no canvas, x 136..184, y 50..130; o vermelho (esquerda da deitada) vai para cima
    let em_pe = img(TamanhoPx { w: 24, h: 40 });
    let sprites = texto::SpritesEmCache { cache: &pasta, imagens: std::slice::from_ref(&em_pe) };
    let p = formas::primitivas(&ef, &quadro(0, canvas, &[]), &sprites).unwrap();
    let b = formas::desenhar(&p, canvas, [0, 0, 320, 180], &pasta, std::slice::from_ref(&em_pe)).unwrap();
    let px = |x: u32, y: u32| &b.px[((y * 320 + x) * 4) as usize..][..4];
    let (cima, baixo) = (px(160, 62), px(160, 118));
    assert!(cima[0] > 200 && cima[2] < 60 && cima[3] == 255, "em cima: {cima:?}");
    assert!(baixo[2] > 200 && baixo[0] < 60 && baixo[3] == 255, "embaixo: {baixo:?}");
    for (x, y) in [(132, 90), (188, 90), (160, 46), (160, 134)] {
        assert_eq!(px(x, y)[3], 0, "({x}, {y}) fora da imagem");
    }
    // tamanho do pedido diferente do da imagem lida (aqui, o deitado): recusado, em vez de esticar
    let deitada = img(TamanhoPx { w: 40, h: 24 });
    let e = formas::desenhar(&p, canvas, [0, 0, 320, 180], &pasta, std::slice::from_ref(&deitada)).unwrap_err().to_string();
    assert!(e.contains("item_invalido"), "{e}");
    let _ = std::fs::remove_dir_all(pasta);
}

// ---------------------------------------------------------------- com ffmpeg

/// Saída em RGB pela BT.709 limitada, croma interpolado.
const PARA_RGB: &str =
    "format=yuv420p,scale=flags=bicubic+accurate_rnd+bitexact+full_chroma_int:in_color_matrix=bt709:in_range=tv:out_range=pc,format=rgb24";
const BT709_TV: &str = "setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv";

fn ffmpeg(cap: &Capacidades, programa: &Path, pasta: &Path, nome: &str, args: Vec<String>) -> Vec<u8> {
    let cmd = Comando {
        nome: nome.into(),
        programa: programa.to_path_buf(),
        args,
        pasta: pasta.to_path_buf(),
        ambiente: Vec::new(),
        sem_ambiente: Vec::new(),
        stderr: pasta.join(format!("{nome}.stderr")),
    };
    let (t, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).expect("ffmpeg roda");
    assert_eq!(t.codigo, Some(0), "{nome} ({}): {:?}", cap.versao, exec::ultimas_linhas(&cmd.stderr, 8));
    out
}

/// Roda `grafo` (saída `[v]`) e devolve os quadros em RGB.
fn grafo(cap: &Capacidades, pasta: &Path, nome: &str, entradas: &[String], g: &str, w: u32, h: u32) -> Vec<medir::QuadroRgb> {
    args::gravar_texto(pasta, &format!("{nome}.txt"), g).unwrap();
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-v", "error", "-copyts"].map(String::from).into();
    a.extend_from_slice(entradas);
    a.extend(args::grafo_em_arquivo(cap, &format!("{nome}.txt")));
    a.extend(["-map", "[v]", "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"].map(String::from));
    let bruto = ffmpeg(cap, &cap.binario, pasta, nome, a);
    let tam = (w * h * 3) as usize;
    assert_eq!(bruto.len() % tam, 0, "{nome}: saída de tamanho inesperado");
    bruto.chunks_exact(tam).map(|c| medir::QuadroRgb { w, h, px: c.to_vec() }).collect()
}

/// Cache dos testes com ffmpeg: caminho com acento, espaço e apóstrofo, como a pasta de um caso.
const PASTA_DIFICIL: &str = "cachê do caso d'água 1";

fn cap_dos_testes() -> Option<std::sync::Arc<Capacidades>> {
    let c = fontes::cap()?;
    assert!(!c.versao.is_empty());
    eprintln!("ffmpeg {} ({})", c.versao, c.binario.display());
    Some(c)
}

#[test]
fn ffmpeg_lista_por_slot_alinhada_em_2997() {
    let Some(cap) = cap_dos_testes() else { return };
    let canvas = TamanhoPx { w: 320, h: 180 };
    let n = 300u32;
    // posição p do quadrado: anda de 2 em 2 slots até o 100 (pares iguais dividem o PNG), depois a cada slot
    let p = |q: u32| if q < 100 { q / 2 } else { q - 50 };
    let canto = |p: u32| (10.0 + 2.0 * (p % 120) as f64, 30.0 + 50.0 * (p / 120) as f64);
    let d = efeito(com(base("destaque", "d1"), json!({ "forma": "retangulo", "rastreio": "r1",
        "traco": { "cor": "#ffff00", "espessura": 4.0 / 180.0, "contorno": false }, "tamanho": 0.1, "angulo_graus": 0.0, "margem": 0.0, "rotulo": null })));
    let quadros: Vec<QuadroCamada> = (0..n)
        .map(|q| {
            let (x, y) = canto(p(q));
            let mut k = quadro(q, canvas, &[("r1", caixa_px(x, y, 20.0, 20.0, canvas))]);
            k.t = q as f64 * 1001.0 / 30000.0;
            k
        })
        .collect();
    let prims: Vec<Vec<Primitiva>> = quadros.iter().map(|q| formas::primitivas(&d, q, &SemSprites).unwrap()).collect();
    let u = prims.iter().filter_map(|p| formas::envolvente(p, canvas)).reduce(|a, b| {
        let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
        Caixa { x: x0, y: y0, w: (a.x + a.w).max(b.x + b.w) - x0, h: (a.y + a.h).max(b.y + b.h) - y0 }
    });
    let caixa = geometria::caixa_px_par(u.unwrap(), canvas);
    let ped = pedido(TipoCamada::Anotacao, caixa, n, vec![item(Desenho::Efeito { efeito: d }, 0, quadros)], Vec::new());
    let pasta = fontes::pasta_temporaria("p05-a17");
    // a lista vai ao ffmpeg por caminho com acento, espaço e apóstrofo
    let c = rasterizar(&ped, &pasta.join(PASTA_DIFICIL), &AtomicBool::new(false)).unwrap();
    assert_eq!(c.arquivos.len(), 250);
    let lista = c.pasta.join(c.lista.as_ref().unwrap());
    let entradas: Vec<String> = [
        "-f", "lavfi", "-i", "color=c=0x404040:s=320x180:r=30000/1001:d=11", "-f", "concat", "-safe", "0", "-i", &lista.to_string_lossy(),
    ]
    .map(String::from)
    .into();
    let g = format!(
        "[0:v]trim=end_frame={n},format=yuv420p,{BT709_TV}[b];[1:v]settb=1001/30000,setpts=N,trim=end_frame={n},{camada}[an];\
         [b][an]overlay=x={x}:y={y}:format=yuv420:eof_action=pass,{PARA_RGB}[v]",
        camada = cor::filtro_camada(),
        x = caixa[0],
        y = caixa[1],
    );
    let saida = grafo(&cap, &pasta, "a17", &entradas, &g, 320, 180);
    assert_eq!(saida.len(), n as usize);
    let mut fora = Vec::new();
    for (q, f) in saida.iter().enumerate() {
        let (x, y) = canto(p(q as u32));
        let centro = medir::centro(f, [255, 255, 0], 60);
        match centro {
            // meio pixel: camada deslocada 1 px (recorte ou posição ímpar) não passa
            Some((cx, cy)) if (cx - (x + 10.0)).abs() <= 0.5 && (cy - (y + 10.0)).abs() <= 0.5 => {}
            outro => fora.push((q, outro, (x + 10.0, y + 10.0))),
        }
    }
    assert!(fora.is_empty(), "{} ({}): camada fora do slot em {} de {n}: {:?}", cap.versao, cap.binario.display(), fora.len(), &fora[..fora.len().min(5)]);
    let _ = std::fs::remove_dir_all(pasta);
}

/// Mapa de quadros em inteiros (§3.1), o de `cm/lib2.py::mapa_int`, para montar a cadeia à mão.
struct Mapa {
    c: i128,
    m: i128,
    kn: i128,
    kd: i128,
    e: i128,
    p0: i128,
    n_q: usize,
    esp: Vec<usize>,
}

fn mdc(a: i128, b: i128) -> i128 {
    if b == 0 {
        a.abs()
    } else {
        mdc(b, a % b)
    }
}

fn teto(a: i128, b: i128) -> i128 {
    -((-a).div_euclid(b))
}

fn mapa(pts: &[i64], tb: (i64, i64), j_in: usize, j_out: usize, v: (i64, i64), fps: (i64, i64)) -> Mapa {
    let (tbn, tbd) = (tb.0 as i128, tb.1 as i128);
    let (fnum, fden) = (fps.0 as i128, fps.1 as i128);
    let c = tbd / mdc(tbd, fnum) * fnum;
    let r = tbn * c / tbd;
    let m = c * fden / fnum;
    let (mut kn, mut kd) = (fnum * v.1 as i128, fden * v.0 as i128 * c);
    let g = mdc(kn, kd);
    kn /= g;
    kd /= g;
    let p: Vec<i128> = pts.iter().map(|&x| x as i128 * r).collect();
    let p0 = p[j_in];
    let pfim = if j_out < p.len() { p[j_out] } else { p[p.len() - 1] + (p[p.len() - 1] - p[p.len() - 2]) };
    let mut borda: Vec<i128> = p[j_in..j_out].to_vec();
    borda.push(pfim);
    let pmin = borda.windows(2).map(|w| w[1] - w[0]).filter(|d| *d > 0).min().unwrap();
    let e = kd.min(pmin * kn) / 2;
    let slot: Vec<i128> = p[j_in..j_out].iter().map(|x| teto((x - p0) * kn - e, kd)).collect();
    let n_q = teto((pfim - p0) * kn - e, kd).max(1) as usize;
    let mut esp = Vec::with_capacity(n_q);
    let mut k = 0;
    for q in 0..n_q as i128 {
        while k + 1 < slot.len() && slot[k + 1] <= q {
            k += 1;
        }
        esp.push(j_in + k);
    }
    Mapa { c, m, kn, kd, e, p0, n_q, esp }
}

fn arvore(v: &[i64], a: usize, b: usize) -> String {
    if b - a == 1 {
        return v[a].to_string();
    }
    let m = (a + b) / 2;
    format!("if(lt(N,{m}),{},{})", arvore(v, a, m), arvore(v, m, b))
}

fn base_de_tempo(cap: &Capacidades, pasta: &Path, video: &Path) -> (i64, i64) {
    let a: Vec<String> = ["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=time_base", "-of", "csv=p=0"]
        .map(String::from)
        .into_iter()
        .chain([video.to_string_lossy().into_owned()])
        .collect();
    let s = String::from_utf8(ffmpeg(cap, &cap.ffprobe, pasta, "tb", a)).unwrap();
    // no TS o ffprobe repete o fluxo (programa e raiz): vale a primeira linha
    let (n, d) = s.lines().next().unwrap_or_default().trim().trim_end_matches(',').split_once('/').expect("time_base");
    (n.parse().unwrap(), d.parse().unwrap())
}

#[test]
fn ffmpeg_mascara_por_quadro_lido_sem_sobra() {
    let Some(cap) = cap_dos_testes() else { return };
    let verde = cor::ler("#00ff00").unwrap();
    let mut resumo = Vec::new();
    // inclui início ≠ 0 em 59,94 (TS), base de tempo grossa (escala 30), 23,976 em base de 1 ms, 29,97 com pixel 10:11 e giro
    for fonte in [
        fontes::Fonte::Mp4,
        fontes::Fonte::TaxaVariavel,
        fontes::Fonte::Giro90,
        fontes::Fonte::PixelNaoQuadrado,
        fontes::Fonte::Ts5994,
        fontes::Fonte::Mp4Escala30,
        fontes::Fonte::Mkv23976,
    ] {
        let video = fontes::gerar(fonte).unwrap();
        let d = fontes::descricao(fonte);
        let pasta = fontes::pasta_temporaria("p05-e1");
        let codificado = if d.giro % 180 == 90 { TamanhoPx { w: d.altura, h: d.largura } } else { TamanhoPx { w: d.largura, h: d.altura } };
        let exibido = geometria::exibido(codificado, Racional { num: d.sar.0 as i64, den: d.sar.1 as i64 }, d.giro);
        let pts = medir::pts(&cap, &video).unwrap();
        let tb = base_de_tempo(&cap, &pasta, &video);
        let numeros = fontes::quadros_esperados(fonte).unwrap();
        assert_eq!(numeros.len(), pts.len(), "{fonte:?}");
        let (j_in, j_out) = (40usize, 130usize);
        for (borda, v) in [(0.0, (1, 1)), (0.0, (1, 2)), (0.1, (1, 1))] {
            let marca = |j: usize| {
                let m = fontes::marca_esperada(fonte, numeros[j]);
                Caixa { x: m[0], y: m[1], w: m[2], h: m[3] }
            };
            let ocultar = efeito(com(base("ocultar", "o1"), json!({ "modo": "solido", "alvo": { "rastreio": "r1", "forma": "retangulo",
                "vertices": null, "margem": 0.0, "borda_suave": borda, "inverter": false }, "intensidade": 1.0, "cor": "#00ff00ff",
                "conteudo": "placa", "motivo": "" })));
            let quadros: Vec<QuadroCamada> = (j_in..j_out)
                .map(|j| {
                    let mut q = quadro((j - j_in) as u32, exibido, &[("r1", marca(j))]);
                    q.pts = Some(pts[j]);
                    q
                })
                .collect();
            let prims: Vec<Vec<Primitiva>> = quadros.iter().map(|q| formas::primitivas(&ocultar, q, &SemSprites).unwrap()).collect();
            let caixa = mascara::uniao_px(&prims, exibido).unwrap();
            let ped = pedido(TipoCamada::MascaraFonte, caixa, (j_out - j_in) as u32, vec![item(Desenho::Efeito { efeito: ocultar }, 0, quadros)], Vec::new());
            let c = rasterizar(&ped, &pasta.join(PASTA_DIFICIL), &AtomicBool::new(false)).unwrap();
            // cadeia do §5.3 à mão: trim por pts → cor → ocultar por quadro lido → tpad → mapa → contagem
            let mp = mapa(&pts, tb, j_in, j_out, v, (30, 1));
            let cadeia = cor::cadeia_fonte(cor::matriz_presumida(d.altura), Faixa::Tv, "yuv420p", (exibido.w, exibido.h)).unwrap();
            let [x, y, w, h] = caixa;
            let fim = if j_out < pts.len() { format!(":end_pts={}", pts[j_out]) } else { String::new() };
            let g = format!(
                "[0:v]trim=start_pts={p_in}{fim},{cadeia},split[a][b];[b]crop={w}:{h}:{x}:{y}:exact=1,{solido}[var];\
                 [1:v]format=gray,settb={tbn}/{tbd},setpts='{arv}'[mk];[var][mk]alphamerge[fx];\
                 [a][fx]overlay=x={x}:y={y}:format=yuv420:eof_action=pass,tpad=stop_mode=clone:stop=1,\
                 settb=1/{cc},setpts='ceil(((PTS-{p0})*{kn}-{e})/{kd})*{m}',fps=30:start_time=0,tpad=stop_mode=clone:stop=-1,\
                 trim=end_frame={nq},{PARA_RGB}[v]",
                p_in = pts[j_in],
                solido = cor::filtro_cor_cheia(verde),
                tbn = tb.0,
                tbd = tb.1,
                arv = arvore(&pts[j_in..j_out], 0, j_out - j_in),
                cc = mp.c,
                p0 = mp.p0,
                kn = mp.kn,
                e = mp.e,
                kd = mp.kd,
                m = mp.m,
                nq = mp.n_q,
            );
            let lista = c.pasta.join(c.lista.as_ref().unwrap());
            let entradas: Vec<String> =
                ["-i", &video.to_string_lossy(), "-f", "concat", "-safe", "0", "-i", &lista.to_string_lossy()].map(String::from).into();
            let saida = grafo(&cap, &pasta, "e1", &entradas, &g, exibido.w, exibido.h);
            assert_eq!(saida.len(), mp.n_q, "{fonte:?} {v:?}");
            let (mut vermelho_max, mut erros) = (0u64, Vec::new());
            for (q, f) in saida.iter().enumerate() {
                let m = marca(mp.esp[q]);
                let (fx0, fy0) = (m.x * exibido.w as f64, m.y * exibido.h as f64);
                let (fx1, fy1) = (fx0 + m.w * exibido.w as f64, fy0 + m.h * exibido.h as f64);
                let (mut vermelho, mut verde_dentro, mut verde_fora) = (0u64, 0u64, 0u64);
                let (ix0, iy0, ix1, iy1) = (fx0.ceil() as u32, fy0.ceil() as u32, fx1.floor() as u32, fy1.floor() as u32);
                // limite dos blocos 2×2 que a forma toca (mais a borda suave)
                let folga = borda * (fx1 - fx0).min(fy1 - fy0) + 1.0;
                let (bx0, by0) = (((fx0 - folga).floor() as u32) & !1, ((fy0 - folga).floor() as u32) & !1);
                let (bx1, by1) = (((fx1 + folga).ceil() as u32 + 1) & !1, ((fy1 + folga).ceil() as u32 + 1) & !1);
                for (i, p) in f.px.as_chunks::<3>().0.iter().enumerate() {
                    let (px, py) = (i as u32 % f.w, i as u32 / f.w);
                    let (r, g, b) = (p[0] as i32, p[1] as i32, p[2] as i32);
                    vermelho += (r - g > 90 && r - b > 90) as u64;
                    let e_verde = g - r > 90 && g - b > 90;
                    let dentro = px >= ix0 && px < ix1 && py >= iy0 && py < iy1;
                    verde_dentro += (dentro && e_verde) as u64;
                    verde_fora += (e_verde && !(px >= bx0 && px < bx1 && py >= by0 && py < by1)) as u64;
                }
                vermelho_max = vermelho_max.max(vermelho);
                let area = ((ix1 - ix0) * (iy1 - iy0)) as u64;
                if vermelho > 0 || verde_dentro != area || verde_fora > 0 {
                    erros.push((q, mp.esp[q], vermelho, verde_dentro, area, verde_fora));
                }
            }
            resumo.push(format!("{fonte:?} borda {borda} {}/{}×: {} slots, vermelho máx {vermelho_max}, slots errados {}", v.0, v.1, saida.len(), erros.len()));
            assert!(erros.is_empty(), "{} ({}) {fonte:?} borda {borda} v {v:?}: (slot, quadro, vermelho, verde dentro, área, verde fora) {:?}",
                cap.versao, cap.binario.display(), &erros[..erros.len().min(6)]);
        }
        let _ = std::fs::remove_dir_all(pasta);
    }
    eprintln!("{}", resumo.join("\n"));
}

#[test]
fn ffmpeg_cor_chega_ao_video() {
    let Some(cap) = cap_dos_testes() else { return };
    let canvas = TamanhoPx { w: 320, h: 180 };
    let quadrado = |x: f64, cor: &str| Primitiva::Caminho {
        d: vec![
            Segmento::M(x / 320.0, 20.0 / 180.0),
            Segmento::L((x + 40.0) / 320.0, 20.0 / 180.0),
            Segmento::L((x + 40.0) / 320.0, 60.0 / 180.0),
            Segmento::L(x / 320.0, 60.0 / 180.0),
            Segmento::Z,
        ],
        traco: None,
        preenche: Some(cor.into()),
        regra: None,
    };
    let cores = [("#ff0000", [255, 0, 0]), ("#00ff00", [0, 255, 0]), ("#0000ff", [0, 0, 255]), ("#ffffff", [255, 255, 255]), ("#000000", [0, 0, 0])];
    let prims: Vec<Primitiva> = cores.iter().enumerate().map(|(i, (c, _))| quadrado(20.0 + 60.0 * i as f64, c)).collect();
    let b = formas::desenhar(&prims, canvas, [0, 0, 320, 180], Path::new("/nao/existe"), &[]).unwrap();
    let pasta = fontes::pasta_temporaria("p05-cor");
    std::fs::write(pasta.join("camada.png"), b.png().unwrap()).unwrap();
    let entradas: Vec<String> =
        ["-f", "lavfi", "-i", "color=c=0x808080:s=320x180:r=30:d=1", "-loop", "1", "-framerate", "30", "-t", "1", "-i", "camada.png"].map(String::from).into();
    let g = format!(
        "[0:v]trim=end_frame=1,format=yuv420p,{BT709_TV}[b];[1:v]trim=end_frame=1,{}[an];[b][an]overlay=0:0:format=yuv420:eof_action=pass,{PARA_RGB}[v]",
        cor::filtro_camada()
    );
    let f = &grafo(&cap, &pasta, "cor", &entradas, &g, 320, 180)[0];
    for (i, (nome, alvo)) in cores.iter().enumerate() {
        let m = medir::cor_media(f, [24 + 60 * i as u32, 24, 32, 32]);
        let erro = m.iter().zip(alvo).map(|(a, b)| (a - *b as f64).abs()).fold(0.0, f64::max);
        assert!(erro <= 2.0, "{} ({}): {nome} chegou como {m:?} (erro {erro:.2})", cap.versao, cap.binario.display());
    }
    // fora da camada o fundo não muda
    let fundo = medir::cor_media(f, [0, 100, 320, 80]);
    assert!(fundo.iter().all(|v| (v - 128.0).abs() <= 2.0), "{fundo:?}");
    let _ = std::fs::remove_dir_all(pasta);
}

#[test]
fn ffmpeg_le_o_png_de_volta_igual() {
    let Some(cap) = cap_dos_testes() else { return };
    let pasta = fontes::pasta_temporaria("p05-png");
    // RGBA sintético com todo tipo de alfa, um desenho suavizado e uma máscara cinza
    let mut px = Vec::new();
    for y in 0..32u32 {
        for x in 0..64u32 {
            px.extend_from_slice(&[(x * 4) as u8, (y * 8) as u8, ((x + y) * 2) as u8, [0, 1, 7, 128, 200, 254, 255][(x as usize + y as usize) % 7]]);
        }
    }
    let sintetico = Bitmap { w: 64, h: 32, canais: 4, px };
    let desenho = formas::desenhar(
        &[Primitiva::Elipse {
            cx: 0.5,
            cy: 0.5,
            rx: 0.35,
            ry: 0.3,
            traco: Some(formas::Traco2 { cor: "#33cc99c0".into(), largura: 0.04, contorno: Some(formas::COR_CONTORNO.into()) }),
            preenche: Some("#ff000040".into()),
        }],
        TamanhoPx { w: 200, h: 120 },
        [0, 0, 200, 120],
        Path::new("/nao/existe"),
        &[],
    )
    .unwrap();
    assert!(desenho.px.as_chunks::<4>().0.iter().any(|p| p[3] > 0 && p[3] < 255));
    let regiao = Primitiva::Regiao {
        modo: formas::ModoRegiao::Solido,
        forma: Box::new(Primitiva::Elipse { cx: 0.5, cy: 0.5, rx: 0.2, ry: 0.2, traco: None, preenche: None }),
        intensidade: 1.0,
        cor: None,
        borda_suave: 0.3,
        inverter: false,
        espaco: formas::EspacoRegiao::Fonte,
    };
    let cinza = mascara::mascara(&[regiao], TamanhoPx { w: 120, h: 80 }, [0, 0, 120, 80]).unwrap();
    for (nome, b, fmt) in [("sintetico", &sintetico, "rgba"), ("desenho", &desenho, "rgba"), ("mascara", &cinza, "gray")] {
        std::fs::write(pasta.join(format!("{nome}.png")), b.png().unwrap()).unwrap();
        let a: Vec<String> = ["-hide_banner", "-nostdin", "-v", "error", "-i", &format!("{nome}.png"), "-f", "rawvideo", "-pix_fmt", fmt, "-"]
            .map(String::from)
            .into();
        let lido = ffmpeg(&cap, &cap.binario, &pasta, nome, a);
        assert!(lido == b.px, "{} ({}): {nome} lido diferente", cap.versao, cap.binario.display());
    }
    let _ = std::fs::remove_dir_all(pasta);
}

/// Mistura de referência: camada (alfa reto) sobre fundo uniforme, em RGB, convertida por BT.709 limitada; croma
/// pela média do bloco 2×2. Devolve os planos Y, U, V (yuv420p) em f64.
fn referencia_420(b: &Bitmap, fundo: (u8, u8, u8)) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let (w, h) = (b.w as usize, b.h as usize);
    let (kr, kb) = (0.2126, 0.0722);
    let kg = 1.0 - kr - kb;
    let mut y = vec![0.0; w * h];
    let mut u = vec![0.0; w * h / 4];
    let mut v = vec![0.0; w * h / 4];
    for (i, p) in b.px.as_chunks::<4>().0.iter().enumerate() {
        let (r, g, bl, a) = (p[0] as f64 / 255.0, p[1] as f64 / 255.0, p[2] as f64 / 255.0, p[3] as f64 / 255.0);
        let yl = kr * r + kg * g + kb * bl;
        let (ul, vl) = (128.0 + 224.0 * (bl - yl) / (2.0 * (1.0 - kb)), 128.0 + 224.0 * (r - yl) / (2.0 * (1.0 - kr)));
        y[i] = a * (16.0 + 219.0 * yl) + (1.0 - a) * fundo.0 as f64;
        let c = (i / w / 2) * (w / 2) + (i % w) / 2;
        u[c] += 0.25 * (a * ul + (1.0 - a) * fundo.1 as f64);
        v[c] += 0.25 * (a * vl + (1.0 - a) * fundo.2 as f64);
    }
    (y, u, v)
}

#[test]
fn ffmpeg_cor_da_borda_e_do_semitransparente() {
    let Some(cap) = cap_dos_testes() else { return };
    let canvas = TamanhoPx { w: 160, h: 96 };
    let ret = |x: f64, y: f64, w: f64, h: f64, cor: &str| Primitiva::Caminho {
        d: vec![
            Segmento::M(x / 160.0, y / 96.0),
            Segmento::L((x + w) / 160.0, y / 96.0),
            Segmento::L((x + w) / 160.0, (y + h) / 96.0),
            Segmento::L(x / 160.0, (y + h) / 96.0),
            Segmento::Z,
        ],
        traco: None,
        preenche: Some(cor.into()),
        regra: None,
    };
    // cores de canal 0 ou 255 (o alfa reto delas é exato em qualquer cobertura); formas a 8 px ou mais umas das outras
    let prims = vec![
        ret(11.0, 11.0, 30.0, 30.0, "#ff0000"),   // opaco, bordas em px ímpar
        ret(52.0, 10.0, 30.0, 30.0, "#0000ff"),   // opaco, bordas em px par
        ret(95.0, 11.0, 30.0, 29.0, "#ff000080"), // meio transparente, ímpar
        ret(137.0, 9.0, 2.0, 32.0, "#00ffff"),    // linha de 2 px em x ímpar
        ret(10.5, 54.25, 30.0, 6.5, "#ffff00"),   // bordas no meio do pixel (suavizadas)
        Primitiva::Elipse {
            cx: 70.0 / 160.0,
            cy: 70.0 / 96.0,
            rx: 14.0 / 160.0,
            ry: 14.0 / 96.0,
            traco: Some(formas::Traco2 { cor: "#ffff00".into(), largura: 3.0 / 96.0, contorno: None }),
            preenche: None,
        },
        Primitiva::Elipse { cx: 120.0 / 160.0, cy: 70.0 / 96.0, rx: 15.3 / 160.0, ry: 11.7 / 96.0, traco: None, preenche: Some("#ff00ffc0".into()) },
    ];
    // a referência é o desenho no canvas inteiro; a camada vai inteira e recortada (posição e lados pares)
    let b = formas::desenhar(&prims, canvas, [0, 0, 160, 96], Path::new("/nao/existe"), &[]).unwrap();
    let pasta = fontes::pasta_temporaria("p05-borda");
    for (fundo, caixa) in [("#808080", [0, 0, 160, 96]), ("#206040", [8, 6, 134, 82]), ("#f0e010", [8, 6, 134, 82])] {
        let recorte = formas::desenhar(&prims, canvas, caixa, Path::new("/nao/existe"), &[]).unwrap();
        assert_eq!((recorte.w, recorte.h), (caixa[2], caixa[3]));
        std::fs::write(pasta.join("camada.png"), recorte.png().unwrap()).unwrap();
        let f = cor::yuv_bt709(cor::ler(fundo).unwrap());
        let g = format!(
            "[0:v]trim=end_frame=1,format=yuv420p,{},{BT709_TV}[b];[1:v]trim=end_frame=1,{}[an];[b][an]overlay={}:{}:format=yuv420:eof_action=pass[v]",
            cor::filtro_cor_cheia(cor::ler(fundo).unwrap()),
            cor::filtro_camada(),
            caixa[0],
            caixa[1],
        );
        args::gravar_texto(&pasta, "borda.txt", &g).unwrap();
        let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-v", "error", "-f", "lavfi", "-i", "color=c=black:s=160x96:r=30:d=1", "-loop", "1",
            "-framerate", "30", "-t", "1", "-i", "camada.png"].map(String::from).into();
        a.extend(args::grafo_em_arquivo(&cap, "borda.txt"));
        a.extend(["-map", "[v]", "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "yuv420p", "-"].map(String::from));
        let lido = ffmpeg(&cap, &cap.binario, &pasta, "borda", a);
        assert_eq!(lido.len(), 160 * 96 * 3 / 2);
        let (ry, ru, rv) = referencia_420(&b, f);
        let pior = |lido: &[u8], r: &[f64]| lido.iter().zip(r).map(|(a, b)| (*a as f64 - b).abs()).fold(0.0, f64::max);
        let (ey, eu, ev) = (pior(&lido[..160 * 96], &ry), pior(&lido[160 * 96..160 * 96 * 5 / 4], &ru), pior(&lido[160 * 96 * 5 / 4..], &rv));
        eprintln!("fundo {fundo}: erro máximo Y {ey:.2}, U {eu:.2}, V {ev:.2}");
        assert!(ey <= 2.0 && eu <= 2.0 && ev <= 2.0, "{} ({}) fundo {fundo}: erro máximo Y {ey:.2}, U {eu:.2}, V {ev:.2}", cap.versao, cap.binario.display());
    }
    let _ = std::fs::remove_dir_all(pasta);
}
