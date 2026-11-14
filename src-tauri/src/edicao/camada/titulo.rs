//! Títulos como primitivas (§5.5): fundo (cor, imagem ou nada) e um sprite por bloco, em 0..1 do canvas.
//! Espelho no front: `modelo/titulos.ts`, pela fixture `primitivas-titulo.json`. Dono: P0.5t (o acabamento dos
//! modelos em Typst é do P1.7).
//!
//! `modelo` é só rótulo: todo bloco traz o próprio `lugar` e é posto como o `EfTexto` (canto arredondado ao pixel).
//! Fundo de cor = retângulo do canvas inteiro; de imagem = a imagem cobrindo o canvas sem distorcer, centrada, com o
//! que sobra cortado. Contas como em `formas` (o espelho repete na mesma ordem): em px do canvas, só `+ − × ÷`,
//! `floor` e `max`, e a saída passa por `+ 0`.

use super::formas::{Primitiva, QuadroCamada, Segmento};
use super::texto::{FonteSprites, PedidoSprite};
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{Alinhamento, AlinhamentoV, Caixa, Fundo, TamanhoPx, Titulo};
use crate::error::{Result, SicroError};

/// −0 → 0.
fn z(v: f64) -> f64 {
    v + 0.0
}

fn fator_h(h: Alinhamento) -> f64 {
    match h {
        Alinhamento::Esq => 0.0,
        Alinhamento::Centro => 0.5,
        Alinhamento::Dir => 1.0,
    }
}

fn fator_v(v: AlinhamentoV) -> f64 {
    match v {
        AlinhamentoV::Topo => 0.0,
        AlinhamentoV::Meio => 0.5,
        AlinhamentoV::Base => 1.0,
    }
}

/// Arredonda ao pixel (meio sobe), igual a `Math.floor(v + 0.5)`.
fn ao_pixel(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// Canto de bloco mais longe que isto do canvas (px, 2^30): recusado. A cópia do sprite estoura o `i32` do tiny-skia
/// (pânico) e nada visível fica tão longe.
const MAX_LUGAR_PX: f64 = 1_073_741_824.0;

fn longe(v: f64) -> bool {
    !v.is_finite() || v.abs() > MAX_LUGAR_PX
}

/// Caixa em px do canvas `cw`×`ch` → 0..1.
fn caixa(x: f64, y: f64, w: f64, h: f64, cw: f64, ch: f64) -> Caixa {
    Caixa { x: z(x / cw), y: z(y / ch), w: z(w / cw), h: z(h / ch) }
}

/// Primitivas do título no quadro: fundo (cor ou imagem) e um sprite por bloco, em 0..1 do canvas.
/// Bloco de conteúdo vazio não desenha. Imagem de fundo que `sprites` não conhece: erro `midia_ausente`.
/// Bloco com o canto a mais de `MAX_LUGAR_PX` do canvas (ou não finito): erro `item_invalido`.
pub fn primitivas(titulo: &Titulo, quadro: &QuadroCamada, sprites: &dyn FonteSprites) -> Result<Vec<Primitiva>> {
    let (cw, ch) = (quadro.canvas.w as f64, quadro.canvas.h as f64);
    let mut v = Vec::with_capacity(titulo.blocos.len() + 1);
    match &titulo.fundo {
        Fundo::Transparente => {}
        Fundo::Cor { cor } => v.push(Primitiva::Caminho {
            d: vec![Segmento::M(0.0, 0.0), Segmento::L(1.0, 0.0), Segmento::L(1.0, 1.0), Segmento::L(0.0, 1.0), Segmento::Z],
            traco: None,
            preenche: Some(cor.clone()),
            regra: None,
        }),
        Fundo::Imagem { midia } => {
            let tam = sprites
                .imagem(midia)
                .filter(|s| s.w > 0 && s.h > 0)
                .ok_or_else(|| SicroError::Validation(format!("{}: {midia}", codigo::MIDIA_AUSENTE)))?;
            let (iw, ih) = (tam.w as f64, tam.h as f64);
            // a maior das duas escalas cobre o canvas; a mesma nos dois eixos não distorce
            let e = (cw / iw).max(ch / ih);
            let (sw, sh) = (iw * e, ih * e);
            v.push(Primitiva::Sprite {
                src: format!("midia:{midia}"),
                caixa: caixa((cw - sw) * 0.5, (ch - sh) * 0.5, sw, sh, cw, ch),
                opacidade: 1.0,
            });
        }
    }
    for (i, b) in titulo.blocos.iter().enumerate() {
        if b.conteudo.is_empty() {
            continue;
        }
        let s = sprites.sprite(&pedido(titulo, i, quadro.canvas))?;
        let (sw, sh) = (s.w as f64, s.h as f64);
        let x0 = ao_pixel(b.lugar.x * cw - sw * fator_h(b.lugar.h));
        let y0 = ao_pixel(b.lugar.y * ch - sh * fator_v(b.lugar.v));
        if longe(x0) || longe(y0) {
            return Err(SicroError::Validation(format!("{}: lugar do bloco {}", codigo::ITEM_INVALIDO, b.id)));
        }
        v.push(Primitiva::Sprite { src: s.relativo, caixa: caixa(x0, y0, sw, sh, cw, ch), opacidade: 1.0 });
    }
    Ok(v)
}

/// Pedido de sprite de um bloco do título para esse canvas: o bloco sozinho, com `largura_max` convertida de
/// fração da largura (bloco) para fração da ALTURA (pedido). Índice fora da lista: pedido sem bloco e de largura 0,
/// que o rasterizador de texto recusa com `item_invalido`.
pub fn pedido(titulo: &Titulo, bloco: usize, canvas: TamanhoPx) -> PedidoSprite {
    let (cw, ch) = (canvas.w as f64, canvas.h as f64);
    match titulo.blocos.get(bloco) {
        Some(b) => PedidoSprite { blocos: vec![b.clone()], largura_max: b.largura_max * cw / ch, altura_canvas: canvas.h, fundo: true },
        None => PedidoSprite { blocos: Vec::new(), largura_max: 0.0, altura_canvas: canvas.h, fundo: true },
    }
}

#[cfg(test)]
mod testes {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::edicao::camada::texto::{self, Atlas, Sprite, SpritesEmCache};
    use crate::edicao::camada::{self, Bitmap};
    use crate::edicao::cor;
    use crate::edicao::curva::fixtures;
    use crate::edicao::ffmpeg::args;
    use crate::edicao::ffmpeg::exec::{self, Comando};
    use crate::edicao::ffmpeg::sonda::Capacidades;
    use crate::edicao::geometria::Afim;
    use crate::edicao::importar_v1::{self, FontesV1};
    use crate::edicao::modelo::{ClipePrincipal, EstiloTexto, Faixa, Matriz, MidiaRef, ModeloTitulo, Tolerante};
    use crate::edicao::motor::plano::{Desenho, ImagemCamada, ItemCamada, PedidoCamada, QuadroItem, TipoCamada};
    use crate::edicao::testes::fontes;

    const MODELOS: [&str; 6] = ["cartela", "abertura", "evento", "terco_inferior", "encerramento", "livre"];

    /// Fonte de sprites falsa e determinística; guarda pedidos e respostas (vão para a fixture: o espelho TS repete).
    struct Falso {
        imagens: BTreeMap<String, TamanhoPx>,
        sprites: RefCell<Vec<Value>>,
    }

    impl Falso {
        fn novo(imagens: &BTreeMap<String, TamanhoPx>) -> Self {
            Falso { imagens: imagens.clone(), sprites: RefCell::new(Vec::new()) }
        }
    }

    impl FonteSprites for Falso {
        fn sprite(&self, p: &PedidoSprite) -> Result<Sprite> {
            let conteudo: String = p.blocos.iter().map(|b| b.conteudo.as_str()).collect();
            let tam = p.blocos.first().map(|b| b.estilo.tamanho).unwrap_or(0.05) * p.altura_canvas as f64;
            let linhas = conteudo.split('\n').count() as u32;
            let mais_longa = conteudo.split('\n').map(|l| l.chars().count()).max().unwrap_or(0);
            let w = ((mais_longa as f64 * tam * 0.55).round() as u32).min((p.largura_max * p.altura_canvas as f64) as u32).max(1);
            let h = ((tam * 1.25).round() as u32 * linhas).max(1);
            let s = Sprite { relativo: format!("sprites/falso-{w}x{h}.png"), w, h };
            self.sprites.borrow_mut().push(json!({ "pedido": p, "sprite": s }));
            Ok(s)
        }
        fn atlas(&self, _: &EstiloTexto, _: u32) -> Result<Atlas> {
            Err(SicroError::Validation("titulo nao usa atlas".into()))
        }
        fn imagem(&self, midia: &str) -> Option<TamanhoPx> {
            self.imagens.get(midia).copied()
        }
    }

    fn quadro(canvas: TamanhoPx) -> QuadroCamada {
        QuadroCamada { q: 0, t: 0.0, pts: None, a: Afim::IDENTIDADE, canvas, rastreios: BTreeMap::new(), indice: None, programa_s: 0.0 }
    }

    fn estilo(cor: &str, tamanho: f64, negrito: bool, caixa: Option<&str>, contorno: Option<&str>) -> Value {
        json!({ "fonte": "sans", "negrito": negrito, "italico": false, "tamanho": tamanho, "cor": cor, "alinhamento": "centro",
                "caixa": caixa.map(|c| json!({ "cor": c, "margem": 0.012, "raio": 0.004 })),
                "contorno": contorno.map(|c| json!({ "cor": c, "largura": 0.002 })) })
    }

    fn bloco(id: &str, papel: &str, conteudo: &str, e: Value, lugar: (f64, f64, &str, &str), largura: f64) -> Value {
        json!({ "id": id, "papel": papel, "conteudo": conteudo, "estilo": e,
                "lugar": { "x": lugar.0, "y": lugar.1, "h": lugar.2, "v": lugar.3 }, "largura_max": largura })
    }

    fn titulo(modelo: &str, fundo: Value, blocos: Vec<Value>) -> Titulo {
        serde_json::from_value(json!({ "modelo": modelo, "fundo": fundo, "blocos": blocos })).expect("título")
    }

    fn cor_fundo(c: &str) -> Value {
        json!({ "tipo": "cor", "cor": c })
    }

    fn transparente() -> Value {
        json!({ "tipo": "transparente" })
    }

    fn fundo_imagem(m: &str) -> Value {
        json!({ "tipo": "imagem", "midia": m })
    }

    const HD: TamanhoPx = TamanhoPx { w: 1920, h: 1080 };

    /// Três blocos em cantos e alinhamentos diferentes.
    fn tres_blocos() -> Vec<Value> {
        vec![
            bloco("b1", "titulo", "Laudo 1234/2026", estilo("#ffffff", 0.078, true, None, None), (0.5, 0.42, "centro", "base"), 0.9),
            bloco("b2", "subtitulo", "Câmera 2 — saída", estilo("#e0e0e0", 0.052, false, Some("#0000008c"), None), (0.5, 0.45, "centro", "topo"), 0.8),
            bloco("b3", "rodape", "Instituto de Criminalística", estilo("#c0c0c0", 0.033, false, None, Some("#000000")), (0.95, 0.94, "dir", "base"), 0.5),
        ]
    }

    /// Casos da fixture: (nome, título, canvas, imagens conhecidas).
    fn casos() -> Vec<(String, Titulo, TamanhoPx, BTreeMap<String, TamanhoPx>)> {
        let sem = BTreeMap::new();
        let img = |pares: &[(&str, u32, u32)]| pares.iter().map(|(k, w, h)| (k.to_string(), TamanhoPx { w: *w, h: *h })).collect::<BTreeMap<_, _>>();
        let corpo = |c: &str| bloco("c1", "corpo", c, estilo("#fafafa", 0.033, false, None, None), (0.5, 0.5, "centro", "meio"), 0.9);
        let longe_em = |x: f64, y: f64| bloco("g", "corpo", "x", estilo("#ffffff", 0.05, false, None, None), (x, y, "esq", "topo"), 0.5);
        const K1: TamanhoPx = TamanhoPx { w: 1024, h: 576 };
        let mut v = vec![(
            "cartela da v1".to_string(),
            titulo("cartela", cor_fundo("#101010"), vec![corpo("Fim do trecho")]),
            HD,
            sem.clone(),
        )];
        for m in MODELOS {
            v.push((format!("modelo {m}"), titulo(m, cor_fundo("#000000c0"), tres_blocos()), HD, sem.clone()));
        }
        v.extend([
            ("transparente tres blocos 720p".into(), titulo("terco_inferior", transparente(), tres_blocos()), TamanhoPx { w: 1280, h: 720 }, sem.clone()),
            ("transparente sem blocos".into(), titulo("livre", transparente(), vec![]), HD, sem.clone()),
            ("cor sem blocos".into(), titulo("cartela", cor_fundo("#000000"), vec![]), HD, sem.clone()),
            (
                "bloco vazio no meio".into(),
                titulo("abertura", cor_fundo("#000000"), vec![corpo("Antes"), bloco("v", "corpo", "", estilo("#ffffff", 0.05, false, Some("#ff0000"), None), (0.5, 0.6, "centro", "meio"), 0.9), corpo("Depois")]),
                HD,
                sem.clone(),
            ),
            ("imagem 4:3 em 16:9".into(), titulo("abertura", fundo_imagem("fundo"), tres_blocos()), HD, img(&[("fundo", 400, 300)])),
            ("imagem 21:9 em 16:9".into(), titulo("abertura", fundo_imagem("fundo"), vec![]), HD, img(&[("fundo", 2520, 1080)])),
            ("imagem na proporcao do canvas".into(), titulo("encerramento", fundo_imagem("fundo"), vec![corpo("Fim")]), HD, img(&[("fundo", 3840, 2160)])),
            ("imagem em canvas vertical".into(), titulo("evento", fundo_imagem("fundo"), vec![corpo("Evento 1")]), TamanhoPx { w: 1080, h: 1920 }, img(&[("fundo", 400, 300)])),
            ("imagem ausente".into(), titulo("abertura", fundo_imagem("outra"), tres_blocos()), HD, img(&[("fundo", 400, 300)])),
            ("imagem de tamanho zero".into(), titulo("abertura", fundo_imagem("fundo"), tres_blocos()), HD, img(&[("fundo", 0, 300)])),
            (
                "bloco fora do canvas".into(),
                titulo("livre", transparente(), vec![
                    bloco("f1", "corpo", "Fora à direita", estilo("#ffffff", 0.05, false, None, None), (1.2, 0.5, "esq", "meio"), 0.5),
                    bloco("f2", "corpo", "Meio fora no alto", estilo("#ffffff", 0.05, false, None, None), (0.1, -0.02, "esq", "meio"), 0.5),
                    bloco("f3", "corpo", "Antes da esquerda", estilo("#ffffff", 0.05, false, None, None), (-0.3, 0.7, "dir", "topo"), 0.5),
                ]),
                HD,
                sem.clone(),
            ),
            (
                "quebra de linha e acento".into(),
                titulo("cartela", cor_fundo("#202020"), vec![bloco("q", "corpo", "Perícia nº 7/2026\nSão José — ação", estilo("#ffffff", 0.06, false, None, None), (0.5, 0.5, "centro", "meio"), 0.9)]),
                HD,
                sem.clone(),
            ),
            // canvas ímpar: o canto cai em meio pixel e sobe
            ("canvas impar".into(), titulo("cartela", cor_fundo("#000000"), vec![corpo("abc"), corpo("abcd")]), TamanhoPx { w: 641, h: 361 }, sem.clone()),
            // x·w = 0,49999999999999994: floor(v + 0,5) dá 1, o `Math.round` daria 0
            (
                "arredondamento no limite do meio pixel".into(),
                titulo("livre", transparente(), vec![bloco("r", "corpo", "x", estilo("#ffffff", 0.05, false, None, None), ((0.5 - f64::EPSILON / 4.0) / 1024.0, 0.5, "esq", "meio"), 0.5)]),
                TamanhoPx { w: 1024, h: 576 },
                sem.clone(),
            ),
            (
                "texto que parece codigo".into(),
                titulo("livre", cor_fundo("#000000"), vec![corpo("#set text(size: 100pt) *x* $y$ #panic(\"z\")")]),
                HD,
                sem.clone(),
            ),
            ("cor de fundo fora do formato".into(), titulo("cartela", cor_fundo("vermelho"), vec![corpo("x")]), HD, sem.clone()),
            // 2^30 px é o limite: no limite passa; 1 px além, e o que estoura para infinito, é recusado
            ("lugar no limite".into(), titulo("livre", transparente(), vec![longe_em(1048576.0, 0.5), longe_em(0.5, -1864135.0)]), K1, sem.clone()),
            ("lugar 1 px alem do limite a direita".into(), titulo("livre", transparente(), vec![corpo("antes"), longe_em(1048576.0009765625, 0.5)]), K1, sem.clone()),
            ("lugar alem do limite acima".into(), titulo("livre", cor_fundo("#000000"), vec![longe_em(0.5, -1864136.0)]), K1, sem.clone()),
            ("lugar que estoura para infinito".into(), titulo("livre", transparente(), vec![longe_em(-1e308, 1e308)]), K1, sem.clone()),
            (
                "largura do bloco em canvas vertical".into(),
                titulo("terco_inferior", transparente(), vec![bloco("l", "titulo", "Uma frase comprida que quebra em várias linhas", estilo("#ffffff", 0.04, true, None, None), (0.05, 0.85, "esq", "base"), 0.6)]),
                TamanhoPx { w: 1080, h: 1920 },
                sem.clone(),
            ),
        ]);
        v
    }

    /// Código do erro de validação (antes do ":").
    fn codigo_do_erro(e: &SicroError) -> String {
        match e {
            SicroError::Validation(m) => m.split(':').next().unwrap_or_default().trim().to_string(),
            outro => panic!("erro inesperado: {outro}"),
        }
    }

    fn rodar_caso(t: &Titulo, canvas: TamanhoPx, imagens: &BTreeMap<String, TamanhoPx>) -> (Value, Value) {
        let f = Falso::novo(imagens);
        let q = quadro(canvas);
        let mut saida = match primitivas(t, &q, &f) {
            Ok(p) => json!({ "primitivas": p }),
            Err(e) => json!({ "erro": codigo_do_erro(&e) }),
        };
        // um pedido por bloco e um fora da lista
        saida["pedidos"] = json!((0..=t.blocos.len()).map(|i| pedido(t, i, canvas)).collect::<Vec<_>>());
        (json!({ "titulo": t, "quadro": q, "imagens": imagens, "sprites": f.sprites.take() }), saida)
    }

    #[test]
    fn fixture_primitivas_titulo() {
        let lista: Vec<Value> = casos()
            .into_iter()
            .map(|(nome, t, canvas, imagens)| {
                let (entrada, saida) = rodar_caso(&t, canvas, &imagens);
                json!({ "nome": nome, "entrada": entrada, "saida": saida })
            })
            .collect();
        fixtures::conferir("primitivas-titulo.json", Value::Array(lista));
    }

    fn do_caso(nome: &str) -> Vec<Primitiva> {
        let (_, t, canvas, imagens) = casos().into_iter().find(|c| c.0 == nome).unwrap();
        primitivas(&t, &quadro(canvas), &Falso::novo(&imagens)).unwrap()
    }

    fn caixa_px(p: &Primitiva, canvas: TamanhoPx) -> [f64; 4] {
        let Primitiva::Sprite { caixa: c, .. } = p else { panic!("{p:?}") };
        [c.x * canvas.w as f64, c.y * canvas.h as f64, c.w * canvas.w as f64, c.h * canvas.h as f64]
    }

    #[test]
    fn seis_modelos_sem_panico_e_o_lugar_do_bloco_vale() {
        let base = do_caso("modelo cartela");
        assert_eq!(base.len(), 4);
        for m in MODELOS {
            assert_eq!(do_caso(&format!("modelo {m}")), base, "{m}");
        }
        // o modelo é lido do texto
        for m in MODELOS {
            let t = titulo(m, transparente(), vec![]);
            assert_eq!(serde_json::to_value(t.modelo).unwrap(), json!(m));
        }
        // fundo de cor: o canvas inteiro, por baixo dos blocos; blocos na ordem
        let Primitiva::Caminho { d, preenche, traco: None, regra: None } = &base[0] else { panic!() };
        assert_eq!(preenche.as_deref(), Some("#000000c0"));
        assert_eq!(d, &[Segmento::M(0.0, 0.0), Segmento::L(1.0, 0.0), Segmento::L(1.0, 1.0), Segmento::L(0.0, 1.0), Segmento::Z]);
        // b1 centro/base em (0,5; 0,42); b3 dir/base em (0,95; 0,94): o ponto do lugar a meio px
        let [x, y, w, h] = caixa_px(&base[1], HD);
        assert!((x + w * 0.5 - 960.0).abs() <= 0.5 && (y + h - 0.42 * 1080.0).abs() <= 0.5, "{x} {y} {w} {h}");
        let [x, y, w, h] = caixa_px(&base[3], HD);
        assert!((x + w - 0.95 * 1920.0).abs() <= 0.5 && (y + h - 0.94 * 1080.0).abs() <= 0.5);
        // canto em px inteiro
        for p in &base[1..] {
            let [x, y, ..] = caixa_px(p, HD);
            assert!((x.round() - x).abs() < 1e-9 && (y.round() - y).abs() < 1e-9, "{x} {y}");
        }
    }

    #[test]
    fn fundo_de_imagem_cobre_sem_distorcer_e_vazios_nao_desenham() {
        for (nome, canvas, img) in [
            ("imagem 4:3 em 16:9", HD, (400.0, 300.0)),
            ("imagem 21:9 em 16:9", HD, (2520.0, 1080.0)),
            ("imagem na proporcao do canvas", HD, (3840.0, 2160.0)),
            ("imagem em canvas vertical", TamanhoPx { w: 1080, h: 1920 }, (400.0, 300.0)),
        ] {
            let p = do_caso(nome);
            let Primitiva::Sprite { src, opacidade, .. } = &p[0] else { panic!() };
            assert_eq!((src.as_str(), *opacidade), ("midia:fundo", 1.0));
            let [x, y, w, h] = caixa_px(&p[0], canvas);
            let (cw, ch) = (canvas.w as f64, canvas.h as f64);
            // mesma escala nos dois eixos, cobre o canvas e fica centrada
            assert!((w / img.0 - h / img.1).abs() < 1e-9, "{nome}: {w}×{h}");
            assert!(x <= 1e-9 && y <= 1e-9 && x + w >= cw - 1e-9 && y + h >= ch - 1e-9, "{nome}");
            assert!((x + w * 0.5 - cw * 0.5).abs() < 1e-9 && (y + h * 0.5 - ch * 0.5).abs() < 1e-9, "{nome}");
            assert!((w - cw).abs() < 1e-9 || (h - ch).abs() < 1e-9, "{nome}: um lado exato");
        }
        let f = Falso::novo(&BTreeMap::new());
        let erro = |t: &Titulo| codigo_do_erro(&primitivas(t, &quadro(HD), &f).unwrap_err());
        assert_eq!(erro(&titulo("abertura", fundo_imagem("nada"), vec![])), codigo::MIDIA_AUSENTE);
        // vazios
        assert!(do_caso("transparente sem blocos").is_empty());
        assert_eq!(do_caso("cor sem blocos").len(), 1);
        let p = do_caso("bloco vazio no meio");
        assert_eq!(p.len(), 3, "o bloco vazio não desenha nem a caixa de fundo");
    }

    #[test]
    fn pedido_em_fracao_da_altura() {
        let t = titulo("cartela", transparente(), tres_blocos());
        let p = pedido(&t, 1, HD);
        assert_eq!(p.blocos, vec![t.blocos[1].clone()]);
        assert_eq!((p.largura_max, p.altura_canvas, p.fundo), (0.8 * 1920.0 / 1080.0, 1080, true));
        let v = pedido(&t, 1, TamanhoPx { w: 1080, h: 1920 });
        assert_eq!(v.largura_max, 0.8 * 1080.0 / 1920.0);
        // fora da lista: sem bloco, largura 0
        for i in [3, usize::MAX] {
            assert_eq!(pedido(&t, i, HD), PedidoSprite { blocos: vec![], largura_max: 0.0, altura_canvas: 1080, fundo: true });
        }
        let vazio = titulo("livre", transparente(), vec![]);
        assert_eq!(pedido(&vazio, 0, HD).blocos.len(), 0);
        // canvas sem altura: não derruba
        let _ = pedido(&t, 0, TamanhoPx { w: 0, h: 0 });
        // com o rasterizador de texto de verdade: a largura do sprite respeita a do bloco
        let cache = fontes::pasta_temporaria("p05t-pedido");
        let longo = "o veículo entra no cruzamento e reduz antes da faixa de pedestres ".repeat(3);
        for canvas in [HD, TamanhoPx { w: 1080, h: 1920 }] {
            let t = titulo("livre", transparente(), vec![bloco("l", "corpo", &longo, estilo("#ffffff", 0.04, false, None, None), (0.5, 0.5, "centro", "meio"), 0.6)]);
            let s = texto::sprite(&cache, &pedido(&t, 0, canvas)).unwrap();
            assert!(s.w as f64 <= 0.6 * canvas.w as f64 + 1.0 && s.w as f64 > 0.5 * canvas.w as f64, "{canvas:?}: {}", s.w);
        }
        let erro = texto::sprite(&cache, &pedido(&t, 7, HD)).unwrap_err();
        assert_eq!(codigo_do_erro(&erro), codigo::ITEM_INVALIDO);
        let _ = std::fs::remove_dir_all(cache);
    }

    // ------------------------------------------------------------ rasterizado de verdade

    fn quadros(canvas: TamanhoPx, n: u32) -> Vec<QuadroItem> {
        (0..n)
            .map(|q| {
                let mut k = quadro(canvas);
                k.q = q as u64;
                k.t = q as f64 / 30.0;
                k.programa_s = k.t;
                QuadroItem { quadro: k, transformacao: None }
            })
            .collect()
    }

    /// Camada de anotação do canvas inteiro com o título em `n` quadros.
    fn pedido_camada(t: &Titulo, canvas: TamanhoPx, n: u32, imagens: Vec<ImagemCamada>) -> PedidoCamada {
        let item = ItemCamada { desenho: Desenho::Titulo { titulo: t.clone() }, de: 0, quadros: quadros(canvas, n) };
        let mut p = PedidoCamada {
            id: "c000_titulo".into(),
            tipo: TipoCamada::Anotacao,
            caixa_px: [0, 0, canvas.w, canvas.h],
            n,
            hash: String::new(),
            itens: vec![item],
            imagens,
        };
        p.hash = camada::hash_do_pedido(&p);
        p
    }

    fn png_da_camada(c: &camada::CamadaPronta) -> image::RgbaImage {
        image::open(c.pasta.join(&c.arquivos[0].arquivo)).expect("png da camada").to_rgba8()
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

    /// Imagem `w`×`h` azul com um quadrado vermelho no meio (lado = um quarto do menor lado), gravada em `pasta`.
    fn imagem_quadrado(pasta: &Path, midia: &str, w: u32, h: u32) -> (ImagemCamada, f64) {
        let lado = w.min(h) / 4;
        let (x0, y0) = ((w - lado) / 2, (h - lado) / 2);
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let dentro = (x0..x0 + lado).contains(&x) && (y0..y0 + lado).contains(&y);
                px.extend_from_slice(if dentro { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
            }
        }
        let png = Bitmap { w, h, canais: 4, px }.png().unwrap();
        let caminho = pasta.join(format!("{midia}.png"));
        std::fs::write(&caminho, &png).unwrap();
        let sha256 = Sha256::digest(&png).iter().map(|b| format!("{b:02x}")).collect();
        (ImagemCamada { midia: midia.into(), sha256, caminho, tamanho: TamanhoPx { w, h } }, lado as f64)
    }

    struct SemBanco;
    impl FontesV1 for SemBanco {
        fn video(&self, _: &str) -> Option<MidiaRef> {
            None
        }
        fn narracao(&self, _: &str) -> Option<MidiaRef> {
            None
        }
        fn pts(&self, _: &str, _: f64) -> Option<i64> {
            None
        }
        fn tamanho(&self, _: &str) -> Option<(u32, u32)> {
            None
        }
    }

    /// Centro (px, centro do pixel = +0,5) ponderado por `peso` em cada pixel.
    fn centro_ponderado(img: &image::RgbaImage, peso: impl Fn(&image::Rgba<u8>) -> f64) -> (f64, f64) {
        let (mut sx, mut sy, mut s) = (0.0, 0.0, 0.0);
        for (x, y, p) in img.enumerate_pixels() {
            let k = peso(p);
            sx += k * (x as f64 + 0.5);
            sy += k * (y as f64 + 0.5);
            s += k;
        }
        assert!(s > 0.0, "nada desenhado");
        (sx / s, sy / s)
    }

    #[test]
    fn cartela_da_v1_rasteriza_com_o_texto_no_centro() {
        let nao = AtomicBool::new(false);
        // toda cartela de toda fixture `v1-*.json`
        let mut v1s: Vec<PathBuf> = std::fs::read_dir(fixtures::caminho(""))
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("v1-") && n.ends_with(".json")))
            .collect();
        v1s.sort();
        let (mut com_texto, mut sem_texto) = (Vec::new(), Vec::new());
        for caminho in &v1s {
            let arquivo = caminho.file_name().unwrap().to_string_lossy().into_owned();
            let v1: Value = serde_json::from_str(&std::fs::read_to_string(caminho).unwrap()).unwrap();
            let p = importar_v1::converter(&v1, &SemBanco, "2026-11-14T12:00:00.000Z", "teste").unwrap_or_else(|e| panic!("{arquivo}: {e}"));
            let canvas = TamanhoPx { w: p.saida.largura, h: p.saida.altura };
            for c in &p.principal {
                let Tolerante::Conhecido(ClipePrincipal::Titulo(k)) = c else { continue };
                let id = format!("{arquivo} {}", k.base.id);
                assert_eq!(k.titulo.modelo, ModeloTitulo::Cartela, "{id}");
                let Fundo::Cor { cor: fundo } = &k.titulo.fundo else { panic!("{id}") };
                let fundo = cor::ler(fundo).unwrap();
                let cache = fontes::pasta_temporaria("p05t-cartela");
                let c = camada::rasterizar(&pedido_camada(&k.titulo, canvas, 3, Vec::new()), &cache, &nao).unwrap();
                assert!(c.fixa && c.arquivos.len() == 1, "{id}");
                let png = png_da_camada(&c);
                assert_eq!(png.dimensions(), (canvas.w, canvas.h), "{id}");
                // cartela sem texto (a v1 guardava só espaços): o fundo, opaco, no canvas inteiro
                if k.titulo.blocos.is_empty() {
                    assert!(png.pixels().all(|p| p.0 == [fundo.r, fundo.g, fundo.b, 255]), "{id}: só o fundo");
                    sem_texto.push(id);
                    let _ = std::fs::remove_dir_all(cache);
                    continue;
                }
                assert_eq!(k.titulo.blocos.len(), 1, "{id}");
                let letra = cor::ler(&k.titulo.blocos[0].estilo.cor).unwrap();
                // o sprite que o título pediu, lido do cache
                let s = texto::sprite(&cache, &pedido(&k.titulo, 0, canvas)).unwrap();
                let sprite = image::open(cache.join(&s.relativo)).unwrap().to_rgba8();
                // tinta na camada: quanto o pixel andou do fundo para a cor do texto (o canal de maior contraste)
                let canal = (0..3).max_by_key(|&i| [letra.r, letra.g, letra.b][i].abs_diff([fundo.r, fundo.g, fundo.b][i])).unwrap();
                let (f, l) = ([fundo.r, fundo.g, fundo.b][canal] as f64, [letra.r, letra.g, letra.b][canal] as f64);
                let tinta = |p: &image::Rgba<u8>| ((p[canal] as f64 - f) / (l - f)).clamp(0.0, 1.0);
                let (lx, ly) = centro_ponderado(&png, tinta);
                let (sx, sy) = centro_ponderado(&sprite, |p| p[3] as f64 / 255.0);
                // onde o sprite caiu na camada (medido): a caixa do texto centrada no canvas a até 1 px
                let (cx, cy) = (lx - sx + s.w as f64 * 0.5, ly - sy + s.h as f64 * 0.5);
                let (mx, my) = (canvas.w as f64 * 0.5, canvas.h as f64 * 0.5);
                eprintln!("{id}: caixa do texto com centro em ({cx:.3}, {cy:.3}), tinta em ({lx:.3}, {ly:.3}); canvas ({mx}, {my})");
                assert!((cx - mx).abs() <= 1.0 && (cy - my).abs() <= 1.0, "{id}: centro ({cx}, {cy}) × ({mx}, {my})");
                // e a tinta não foi esticada nem cortada: a mesma soma do sprite
                let soma_camada: f64 = png.pixels().map(tinta).sum();
                let soma_sprite: f64 = sprite.pixels().map(|p| p[3] as f64 / 255.0).sum();
                assert!((soma_camada - soma_sprite).abs() <= soma_sprite * 0.02 + 2.0, "{id}: {soma_camada} × {soma_sprite}");
                // fora da caixa do texto, só o fundo, opaco
                let (x0, y0) = ((cx - s.w as f64 * 0.5).round() as u32, (cy - s.h as f64 * 0.5).round() as u32);
                let fora = png
                    .enumerate_pixels()
                    .filter(|(x, y, _)| !((x0..x0 + s.w).contains(x) && (y0..y0 + s.h).contains(y)))
                    .filter(|(_, _, p)| p.0 != [fundo.r, fundo.g, fundo.b, 255])
                    .count();
                assert_eq!(fora, 0, "{id}: pixel fora da caixa do texto diferente do fundo");
                com_texto.push(id);
                let _ = std::fs::remove_dir_all(cache);
            }
        }
        // as três cartelas das fixtures de hoje; fixture nova com cartela entra sozinha
        for id in ["v1-completo.json f2", "v1-velocidade.json a2"] {
            assert!(com_texto.iter().any(|c| c == id), "{id} em {com_texto:?}");
        }
        assert!(sem_texto.iter().any(|c| c == "v1-completo.json f6"), "{sem_texto:?}");
    }

    #[test]
    fn fundo_de_imagem_sem_distorcao_na_camada() {
        let nao = AtomicBool::new(false);
        let pasta = fontes::pasta_temporaria("p05t-imagem");
        for (iw, ih, canvas) in [
            (160, 120, TamanhoPx { w: 640, h: 360 }),
            (120, 160, TamanhoPx { w: 640, h: 360 }),
            (160, 120, TamanhoPx { w: 360, h: 640 }),
            // muito reduzida: passa pela redução por blocos do rasterizador
            (1600, 1200, TamanhoPx { w: 640, h: 360 }),
            (640, 360, TamanhoPx { w: 640, h: 360 }),
        ] {
            let (img, lado) = imagem_quadrado(&pasta, &format!("q{iw}x{ih}"), iw, ih);
            let t = titulo("abertura", fundo_imagem(&img.midia), vec![]);
            let c = camada::rasterizar(&pedido_camada(&t, canvas, 1, vec![img]), &pasta, &nao).unwrap();
            let png = png_da_camada(&c);
            // cobre o canvas inteiro
            assert!(png.pixels().all(|p| p[3] == 255), "{iw}×{ih} em {canvas:?}: pixel sem fundo");
            // o quadrado continua quadrado, no tamanho da escala e no meio
            let vermelho: Vec<(u32, u32)> = png.enumerate_pixels().filter(|(_, _, p)| p[0] > 128 && p[2] < 128).map(|(x, y, _)| (x, y)).collect();
            let (x0, x1) = (vermelho.iter().map(|p| p.0).min().unwrap(), vermelho.iter().map(|p| p.0).max().unwrap());
            let (y0, y1) = (vermelho.iter().map(|p| p.1).min().unwrap(), vermelho.iter().map(|p| p.1).max().unwrap());
            let (w, h) = ((x1 - x0 + 1) as f64, (y1 - y0 + 1) as f64);
            let e = (canvas.w as f64 / iw as f64).max(canvas.h as f64 / ih as f64);
            eprintln!("{iw}×{ih} em {}×{}: quadrado {w}×{h} (esperado {:.2})", canvas.w, canvas.h, lado * e);
            assert!((w - h).abs() <= 1.0, "{iw}×{ih} em {canvas:?}: distorcido {w}×{h}");
            assert!((w - lado * e).abs() <= 1.5 && (h - lado * e).abs() <= 1.5, "{iw}×{ih}: lado {w}×{h} × {}", lado * e);
            let (cx, cy) = ((x0 + x1 + 1) as f64 * 0.5, (y0 + y1 + 1) as f64 * 0.5);
            assert!((cx - canvas.w as f64 * 0.5).abs() <= 1.0 && (cy - canvas.h as f64 * 0.5).abs() <= 1.0, "{cx} {cy}");
        }
        let _ = std::fs::remove_dir_all(pasta);
    }

    #[test]
    fn vazios_texto_de_20000_e_bloco_fora_do_canvas_nao_derrubam() {
        let nao = AtomicBool::new(false);
        let canvas = TamanhoPx { w: 640, h: 360 };
        let cache = fontes::pasta_temporaria("p05t-vazios");
        let sprites = SpritesEmCache { cache: &cache, imagens: &[] };
        let e = || estilo("#ffffff", 0.05, false, Some("#00000080"), Some("#000000"));
        let centro = (0.5, 0.5, "centro", "meio");
        // lista vazia, bloco vazio, só espaço: nada derruba; com cor, só o fundo
        for (t, n_prims) in [
            (titulo("livre", transparente(), vec![]), 0),
            (titulo("cartela", cor_fundo("#123456"), vec![]), 1),
            (titulo("abertura", transparente(), vec![bloco("v", "corpo", "", e(), centro, 0.9)]), 0),
            (titulo("abertura", cor_fundo("#000000"), vec![bloco("v", "corpo", "", e(), centro, 0.9), bloco("s", "corpo", "  \n ", e(), centro, 0.9)]), 2),
        ] {
            let p = primitivas(&t, &quadro(canvas), &sprites).unwrap();
            assert_eq!(p.len(), n_prims, "{t:?}");
            let c = camada::rasterizar(&pedido_camada(&t, canvas, 2, Vec::new()), &cache, &nao).unwrap();
            assert!(c.fixa);
        }
        // o pedido do bloco vazio também serve ao rasterizador de texto
        let t = titulo("abertura", transparente(), vec![bloco("v", "corpo", "", e(), centro, 0.9)]);
        texto::sprite(&cache, &pedido(&t, 0, canvas)).unwrap();
        // 20 000 caracteres: sprite de verdade, maior que o canvas (cortado na camada)
        let limite = "palavra ".repeat(2500);
        assert_eq!(limite.chars().count(), 20_000);
        let t = titulo("livre", transparente(), vec![bloco("l", "corpo", &limite, estilo("#ffffff", 0.02, false, None, None), centro, 0.9)]);
        let inicio = std::time::Instant::now();
        let p = primitivas(&t, &quadro(canvas), &sprites).unwrap();
        eprintln!("20 000 caracteres: {:?}, {:?}", inicio.elapsed(), caixa_px(&p[0], canvas));
        let [_, y, _, h] = caixa_px(&p[0], canvas);
        assert!(y < 0.0 && h > canvas.h as f64, "o texto longo passa do canvas");
        let c = camada::rasterizar(&pedido_camada(&t, canvas, 1, Vec::new()), &cache, &nao).unwrap();
        assert!(png_da_camada(&c).pixels().any(|p| p[3] == 255));
        // um a mais: recusado com código, sem pânico
        let demais = format!("{limite}a");
        let t = titulo("livre", transparente(), vec![bloco("l", "corpo", &demais, estilo("#ffffff", 0.02, false, None, None), centro, 0.9)]);
        assert_eq!(codigo_do_erro(&primitivas(&t, &quadro(canvas), &sprites).unwrap_err()), codigo::ITEM_INVALIDO);
        let e = camada::rasterizar(&pedido_camada(&t, canvas, 1, Vec::new()), &cache, &nao).unwrap_err();
        assert_eq!(codigo_do_erro(&e), codigo::ITEM_INVALIDO);
        // bloco todo fora e bloco cortado pela borda, num título de 1 quadro
        let t = titulo("livre", transparente(), vec![
            bloco("f", "corpo", "Fora", estilo("#ff0000", 0.08, true, None, None), (1.2, 0.5, "esq", "meio"), 0.5),
            bloco("m", "corpo", "Meio fora", estilo("#00ff00", 0.08, true, None, None), (0.0, 0.0, "centro", "meio"), 0.5),
        ]);
        let c = camada::rasterizar(&pedido_camada(&t, canvas, 1, Vec::new()), &cache, &nao).unwrap();
        assert!(c.fixa && c.lista.is_none() && c.arquivos.len() == 1);
        let png = png_da_camada(&c);
        assert!(png.pixels().all(|p| p[3] == 0 || p[0] == 0), "nada do bloco de fora aparece");
        let verde = png.enumerate_pixels().filter(|(_, _, p)| p[3] == 255 && p[1] == 255).map(|(x, y, _)| (x, y)).collect::<Vec<_>>();
        assert!(!verde.is_empty() && verde.iter().all(|&(x, y)| x < 160 && y < 40), "o bloco cortado aparece só no canto");
        let _ = std::fs::remove_dir_all(cache);
    }

    #[test]
    fn bloco_longe_demais_e_recusado_sem_panico() {
        let nao = AtomicBool::new(false);
        let canvas = TamanhoPx { w: 640, h: 360 };
        let cache = fontes::pasta_temporaria("p05t-longe");
        let em = |x: f64, y: f64| titulo("livre", transparente(), vec![bloco("f", "corpo", "Fora", estilo("#ff0000", 0.08, true, None, None), (x, y, "esq", "base"), 0.5)]);
        // recorte da camada inteiro e deslocado: a cópia do sprite desconta o canto do recorte
        let recortes = [[0, 0, 640, 360], [100, 100, 200, 200]];
        let rasterizar = |t: &Titulo, caixa_px: [u32; 4]| {
            let mut p = pedido_camada(t, canvas, 1, Vec::new());
            p.caixa_px = caixa_px;
            p.hash = camada::hash_do_pedido(&p);
            camada::rasterizar(&p, &cache, &nao)
        };
        let (lx, ly) = (MAX_LUGAR_PX / canvas.w as f64, MAX_LUGAR_PX / canvas.h as f64);
        let longe = [(4e6, 0.5), (-3.5e6, 0.5), (0.5, 1e7), (0.5, -1e7), (1e300, 0.5), (-1e300, 0.5), (0.5, -1e300), (f64::MAX, f64::MAX), (lx + 1.0, 0.5), (-lx - 1.0, 0.5), (0.5, ly + 1.0)];
        for (x, y) in longe {
            for caixa_px in recortes {
                assert_eq!(codigo_do_erro(&rasterizar(&em(x, y), caixa_px).unwrap_err()), codigo::ITEM_INVALIDO, "({x}, {y})");
            }
        }
        // no limite, e longe mas dentro dele: desenha (nada aparece)
        for (x, y) in [(lx, 0.5), (-lx, 0.5), (0.5, ly), (1e5, -1e5), (-1e5, 1e5)] {
            for caixa_px in recortes {
                let c = rasterizar(&em(x, y), caixa_px).unwrap();
                assert!(png_da_camada(&c).pixels().all(|p| p[3] == 0), "({x}, {y})");
            }
        }
        let _ = std::fs::remove_dir_all(cache);
    }

    #[test]
    fn mesmos_bytes_em_duas_execucoes() {
        let nao = AtomicBool::new(false);
        let canvas = TamanhoPx { w: 640, h: 360 };
        let img = fontes::pasta_temporaria("p05t-bytes-img");
        let (imagem, _) = imagem_quadrado(&img, "fundo", 200, 120);
        let t = titulo("abertura", fundo_imagem("fundo"), vec![
            bloco("a", "titulo", "Perícia nº 7/2026\nSão José — ação", estilo("#ffcc00", 0.07, true, Some("#0000008c"), Some("#000000")), (0.5, 0.45, "centro", "base"), 0.9),
            bloco("b", "rodape", "Câmera 2", estilo("#ffffff", 0.04, false, None, None), (0.97, 0.95, "dir", "base"), 0.4),
        ]);
        let rodar = |cache: &Path| {
            let c = camada::rasterizar(&pedido_camada(&t, canvas, 2, vec![imagem.clone()]), cache, &nao).unwrap();
            (c.arquivos.clone(), arquivos(cache))
        };
        let (c1, c2) = (fontes::pasta_temporaria("p05t-bytes-a"), fontes::pasta_temporaria("p05t-bytes-b"));
        let (a1, f1) = rodar(&c1);
        typst::comemo::evict(0);
        let (a2, f2) = rodar(&c2);
        assert_eq!(a1, a2);
        assert_eq!(f1, f2);
        assert!(f1.keys().any(|k| k.starts_with("sprites")) && f1.keys().any(|k| k.starts_with("camadas")));
        for p in [c1, c2, img] {
            let _ = std::fs::remove_dir_all(p);
        }
    }

    // ------------------------------------------------------------ com ffmpeg

    fn ffmpeg(cap: &Capacidades, pasta: &Path, nome: &str, entradas: &[String], grafo: &str) -> Vec<u8> {
        args::gravar_texto(pasta, &format!("{nome}.txt"), grafo).unwrap();
        let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-v", "error"].map(String::from).into();
        a.extend_from_slice(entradas);
        a.extend(args::grafo_em_arquivo(cap, &format!("{nome}.txt")));
        a.extend(["-map", "[v]", "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "yuv420p", "-"].map(String::from));
        let cmd = Comando {
            nome: nome.into(),
            programa: cap.binario.clone(),
            args: a,
            pasta: pasta.to_path_buf(),
            ambiente: Vec::new(),
            sem_ambiente: Vec::new(),
            stderr: pasta.join(format!("{nome}.stderr")),
        };
        let (t, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).expect("ffmpeg roda");
        assert_eq!(t.codigo, Some(0), "{nome} ({}): {:?}", cap.versao, exec::ultimas_linhas(&cmd.stderr, 8));
        out
    }

    #[test]
    fn ffmpeg_titulo_transparente_sobre_video() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {} ({})", cap.versao, cap.binario.display());
        let (w, h) = (640u32, 360u32);
        let canvas = TamanhoPx { w, h };
        let cores = [("#ff0000", [255u8, 0, 0]), ("#00ff00", [0, 255, 0]), ("#0000ff", [0, 0, 255])];
        let t = titulo("terco_inferior", transparente(), vec![
            bloco("b1", "titulo", "Câmera 2", estilo(cores[0].0, 0.1, true, None, None), (0.05, 0.6, "esq", "base"), 0.9),
            bloco("b2", "subtitulo", "Saída 12h", estilo(cores[1].0, 0.16, true, Some("#000000a0"), None), (0.05, 0.62, "esq", "topo"), 0.9),
            bloco("b3", "rodape", "Laudo nº 123", estilo(cores[2].0, 0.1, true, None, None), (0.95, 0.3, "dir", "base"), 0.9),
        ]);
        let n = 6u32;
        let pasta = fontes::pasta_temporaria("p05t-video");
        let cache = pasta.join("cachê do caso");
        let c = camada::rasterizar(&pedido_camada(&t, canvas, n, Vec::new()), &cache, &AtomicBool::new(false)).unwrap();
        assert!(c.fixa, "título que não muda: um PNG só");
        let png = c.pasta.join(&c.arquivos[0].arquivo);
        let camada = image::open(&png).unwrap().to_rgba8();
        let caixas: Vec<[f64; 4]> = primitivas(&t, &quadro(canvas), &SpritesEmCache { cache: &cache, imagens: &[] })
            .unwrap()
            .iter()
            .map(|p| caixa_px(p, canvas))
            .collect();
        assert_eq!(caixas.len(), 3);
        let video: Vec<String> = ["-f", "lavfi", "-i", &format!("testsrc2=s={w}x{h}:r=30:d=1")].map(String::from).into();
        let base = format!("[0:v]trim=end_frame={n},format=yuv420p,setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv");
        let sem = ffmpeg(&cap, &pasta, "sem", &video, &format!("{base}[v]"));
        let mut entradas = video.clone();
        entradas.extend(["-loop", "1", "-framerate", "30", "-t", "1", "-i"].map(String::from));
        entradas.push(png.to_string_lossy().into_owned());
        let g = format!("{base}[b];[1:v]trim=end_frame={n},{}[an];[b][an]overlay=0:0:format=yuv420:eof_action=pass[v]", cor::filtro_camada());
        let com = ffmpeg(&cap, &pasta, "com", &entradas, &g);
        let (ny, nc) = ((w * h) as usize, (w * h / 4) as usize);
        let tam = ny + 2 * nc;
        assert_eq!((sem.len(), com.len()), (tam * n as usize, tam * n as usize));
        let alfa = |x: u32, y: u32| camada.get_pixel(x, y)[3];
        let dentro = |x: u32, y: u32| caixas.iter().any(|c| (x as f64) >= c[0] && (x as f64) < c[0] + c[2] && (y as f64) >= c[1] && (y as f64) < c[1] + c[3]);
        let bloco_croma = |cx: u32, cy: u32| [(2 * cx, 2 * cy), (2 * cx + 1, 2 * cy), (2 * cx, 2 * cy + 1), (2 * cx + 1, 2 * cy + 1)];
        let (mut fora, mut mudou_fora, mut mudou_dentro) = (0u64, 0u64, 0u64);
        let mut pior = [0.0f64; 3];
        let mut medidos = [0u64; 3];
        for k in 0..n as usize {
            let (s, m) = (&sem[k * tam..(k + 1) * tam], &com[k * tam..(k + 1) * tam]);
            // luma: alfa 0 ou fora das caixas dos blocos = o pixel do vídeo, sem erro
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) as usize;
                    if alfa(x, y) == 0 || !dentro(x, y) {
                        fora += 1;
                        mudou_fora += (s[i] != m[i]) as u64;
                    } else {
                        mudou_dentro += (s[i] != m[i]) as u64;
                    }
                }
            }
            for cy in 0..h / 2 {
                for cx in 0..w / 2 {
                    let i = (cy * w / 2 + cx) as usize;
                    let px = bloco_croma(cx, cy);
                    let (u, v) = (ny + i, ny + nc + i);
                    if px.iter().all(|&(x, y)| alfa(x, y) == 0 || !dentro(x, y)) {
                        fora += 2;
                        mudou_fora += (s[u] != m[u]) as u64 + (s[v] != m[v]) as u64;
                    }
                    // cor do texto: bloco 2×2 opaco na cor de um bloco, e o alcance do filtro de croma da camada
                    // (3,5 px além do bloco) todo nessa cor; perto da caixa de fundo o 4:2:0 mistura as duas
                    for (j, (_, rgb)) in cores.iter().enumerate() {
                        let alcance = ((2 * cx).saturating_sub(3)..(2 * cx + 5).min(w))
                            .flat_map(|x| ((2 * cy).saturating_sub(3)..(2 * cy + 5).min(h)).map(move |y| (x, y)));
                        let cheio = px.iter().all(|&(x, y)| camada.get_pixel(x, y)[3] == 255);
                        if cheio && alcance.into_iter().all(|(x, y)| camada.get_pixel(x, y).0[..3] == rgb[..]) {
                            for &(x, y) in &px {
                                let c = cor::rgb((m[(y * w + x) as usize], m[u], m[v]), Matriz::Bt709, Faixa::Tv);
                                for (p, (a, b)) in pior.iter_mut().zip([c.r, c.g, c.b].iter().zip(rgb)) {
                                    *p = p.max(a.abs_diff(*b) as f64);
                                }
                                medidos[j] += 1;
                            }
                        }
                    }
                }
            }
        }
        eprintln!("{}: {fora} amostras fora dos blocos, {mudou_fora} mudaram; {mudou_dentro} mudaram dentro; erro de cor {pior:?} em {medidos:?} px", cap.versao);
        assert_eq!(mudou_fora, 0, "{} ({}): o vídeo mudou fora dos blocos", cap.versao, cap.binario.display());
        assert!(fora as f64 > 0.8 * (tam * n as usize) as f64, "quase tudo é fora dos blocos");
        assert!(mudou_dentro > 1000, "o título não apareceu");
        assert!(medidos.iter().all(|&m| m >= 100), "poucos pixels de cor cheia: {medidos:?}");
        assert!(pior.iter().all(|&e| e <= 2.0), "{} ({}): erro de cor {pior:?}", cap.versao, cap.binario.display());
        let _ = std::fs::remove_dir_all(pasta);
    }
}
