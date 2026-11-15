//! Régua de escala e velocidade exibida no rasterizador (§5.4, §5.5): só primitivas (`caminho`, `sprite`).
//! A conta da homografia e do texto mora em `edicao/medidas.rs`. Espelho no front: `palco/desenho/medidas.ts`, pela
//! fixture `primitivas-medidas.json`. Dono: P1.3.
//!
//! Régua: um caminho só, com a linha pelas divisões e um tique em cada uma, perpendicular na imagem, com meia altura
//! proporcional ao passo local (a perspectiva encolhe os de longe) e nunca menor que um múltiplo da largura do traço;
//! rótulo ("1 m") depois da ponta. Mesmas regras de conta de `formas.rs` (o espelho repete na mesma ordem).

use super::formas::{Primitiva, QuadroCamada, Segmento, Traco2, COR_CONTORNO};
use super::texto::{FonteSprites, PedidoSprite};
use crate::edicao::geometria;
use crate::edicao::medidas;
use crate::edicao::modelo::{
    Alinhamento, AlinhamentoV, BlocoTexto, Caixa, ContornoTexto, EfRegua, EfVelocidadeExibida, EstiloTexto, Fonte, Lugar, OndeVelocidade,
    PapelTexto, Ponto, Traco,
};
use crate::error::Result;

/// Meia altura do tique numa divisão do meio: fração do passo × número de divisões.
const TIQUE_MEIO: f64 = 0.0125;
/// Nas pontas (o dobro).
const TIQUE_PONTA: f64 = 0.05;
/// Meia altura mínima do tique, em larguras do traço (régua curta na imagem: o tique não some dentro da linha).
const TIQUE_MIN_MEIO: f64 = 1.5;
const TIQUE_MIN_PONTA: f64 = 2.5;
/// Rótulo da régua: tamanho do texto (fração da altura do canvas, o "p" do v1).
const ROTULO_TAMANHO: f64 = 0.033;
/// Contorno do rótulo (fração da altura do canvas), quando o traço tem contorno.
const ROTULO_CONTORNO: f64 = 0.003;
/// Largura máxima dos textos (rótulo e velocidade): fração da largura do canvas.
const LARGURA_TEXTO: f64 = 0.5;

/// −0 → 0.
fn z(v: f64) -> f64 {
    v + 0.0
}

/// Arredonda ao pixel (meio sobe), igual a `Math.floor(v + 0.5)`.
fn ao_pixel(v: f64) -> f64 {
    (v + 0.5).floor()
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

/// Pedido de um texto de uma linha, sem lugar (o lugar só posiciona; o sprite fica o mesmo onde quer que vá).
fn pedido(id: &str, conteudo: String, estilo: EstiloTexto, quadro: &QuadroCamada) -> PedidoSprite {
    let (w, h) = (quadro.canvas.w as f64, quadro.canvas.h as f64);
    PedidoSprite {
        blocos: vec![BlocoTexto {
            id: id.to_string(),
            papel: PapelTexto::Corpo,
            conteudo,
            estilo,
            lugar: Lugar { x: 0.0, y: 0.0, h: Alinhamento::Esq, v: AlinhamentoV::Topo },
            largura_max: LARGURA_TEXTO,
        }],
        largura_max: LARGURA_TEXTO * w / h,
        altura_canvas: quadro.canvas.h,
        fundo: true,
    }
}

/// Estilo do rótulo da régua: sem serifa, na cor do traço, contorno preto se o traço tem.
fn estilo_do_rotulo(t: &Traco) -> EstiloTexto {
    EstiloTexto {
        fonte: Fonte::Sans,
        negrito: false,
        italico: false,
        tamanho: ROTULO_TAMANHO,
        cor: t.cor.clone(),
        alinhamento: Alinhamento::Centro,
        caixa: None,
        contorno: t.contorno.then(|| ContornoTexto { cor: "#000000".into(), largura: ROTULO_CONTORNO }),
    }
}

/// Régua no quadro: linha das pontas, divisões e rótulo ("1 m"). Pontas por `medidas::pontas_conferidas` (erro com a
/// régua fora da região calibrada) passadas por `quadro.a`.
pub fn regua(e: &EfRegua, quadro: &QuadroCamada, sprites: &dyn FonteSprites) -> Result<Vec<Primitiva>> {
    let pontos = medidas::pontas_conferidas(e)?;
    let (w, h) = (quadro.canvas.w as f64, quadro.canvas.h as f64);
    let c: Vec<(f64, f64)> = pontos
        .iter()
        .map(|p| {
            let q = quadro.a.ponto(*p);
            (q.x * w, q.y * h)
        })
        .collect();
    let n = c.len() - 1;
    let m = |x: f64, y: f64| Segmento::M(z(x / w), z(y / h));
    let l = |x: f64, y: f64| Segmento::L(z(x / w), z(y / h));
    let mut d = Vec::with_capacity(3 * c.len());
    d.push(m(c[0].0, c[0].1));
    for &(x, y) in &c[1..] {
        d.push(l(x, y));
    }
    let largura = e.traco.espessura * h;
    for k in 0..=n {
        let (a, b, f, minimo) = if k == 0 {
            (c[0], c[1], TIQUE_PONTA, TIQUE_MIN_PONTA)
        } else if k == n {
            (c[n - 1], c[n], TIQUE_PONTA, TIQUE_MIN_PONTA)
        } else {
            (c[k - 1], c[k + 1], TIQUE_MEIO, TIQUE_MIN_MEIO)
        };
        let f = f * n as f64;
        let (mut nx, mut ny) = (-(b.1 - a.1) * f, (b.0 - a.0) * f);
        let (meia, minimo) = ((nx * nx + ny * ny).sqrt(), minimo * largura);
        if meia > 0.0 && meia < minimo {
            let aumento = minimo / meia;
            nx *= aumento;
            ny *= aumento;
        }
        let (x, y) = c[k];
        d.push(m(x - nx, y - ny));
        d.push(l(x + nx, y + ny));
    }
    let traco = Traco2 { cor: e.traco.cor.clone(), largura: e.traco.espessura, contorno: e.traco.contorno.then(|| COR_CONTORNO.to_string()) };
    let mut v = vec![Primitiva::Caminho { d, traco: Some(traco), preenche: None, regra: None }];
    if e.rotulo {
        let s = sprites.sprite(&pedido(&e.base.id, medidas::texto_do_comprimento(e.comprimento_m), estilo_do_rotulo(&e.traco), quadro))?;
        let (sw, sh) = (s.w as f64, s.h as f64);
        let (tx, ty) = c[n];
        let (vx, vy) = (tx - c[0].0, ty - c[0].1);
        let g = e.traco.espessura * h * 2.0;
        // depois da ponta, no sentido da régua
        let (x, y) = if vx.abs() >= vy.abs() {
            (if vx >= 0.0 { tx + g } else { tx - g - sw }, ty - sh * 0.5)
        } else {
            (tx - sw * 0.5, if vy >= 0.0 { ty + g } else { ty - g - sh })
        };
        v.push(Primitiva::Sprite { src: s.relativo, caixa: caixa(ao_pixel(x), ao_pixel(y), sw, sh, w, h), opacidade: 1.0 });
    }
    Ok(v)
}

fn caixa(x: f64, y: f64, sw: f64, sh: f64, w: f64, h: f64) -> Caixa {
    Caixa { x: z(x / w), y: z(y / h), w: z(sw / w), h: z(sh / h) }
}

/// Velocidade no quadro: sprite do texto de `medidas::texto_da_velocidade` no lugar fixo ou com o centro deslocado
/// (`dx`, `dy`, fração do canvas) do centro do rastreio no canvas. Rastreio sem caixa no quadro: nada.
pub fn velocidade(e: &EfVelocidadeExibida, quadro: &QuadroCamada, sprites: &dyn FonteSprites) -> Result<Vec<Primitiva>> {
    let (w, h) = (quadro.canvas.w as f64, quadro.canvas.h as f64);
    let centro = match &e.onde {
        OndeVelocidade::Fixa { .. } => None,
        OndeVelocidade::Segue { rastreio, dx, dy } => {
            let Some(r) = quadro.rastreios.get(rastreio) else { return Ok(Vec::new()) };
            let ponto = (r.w.is_nan() || r.w <= 0.0) && (r.h.is_nan() || r.h <= 0.0);
            let (cx, cy) = if ponto {
                let p = quadro.a.ponto(Ponto { x: r.x, y: r.y });
                (p.x * w, p.y * h)
            } else {
                let c = geometria::caixa_no_canvas(&quadro.a, *r);
                let (x0, y0, bw, bh) = (c.x * w, c.y * h, c.w * w, c.h * h);
                (x0 + bw * 0.5, y0 + bh * 0.5)
            };
            Some((cx + dx * w, cy + dy * h))
        }
    };
    let s = sprites.sprite(&pedido(&e.base.id, medidas::texto_da_velocidade(&e.calculo), e.estilo.clone(), quadro))?;
    let (sw, sh) = (s.w as f64, s.h as f64);
    let (x0, y0) = match (&e.onde, centro) {
        (_, Some((cx, cy))) => (ao_pixel(cx - sw * 0.5), ao_pixel(cy - sh * 0.5)),
        (OndeVelocidade::Fixa { lugar }, None) => (ao_pixel(lugar.x * w - sw * fator_h(lugar.h)), ao_pixel(lugar.y * h - sh * fator_v(lugar.v))),
        (OndeVelocidade::Segue { .. }, None) => return Ok(Vec::new()),
    };
    Ok(vec![Primitiva::Sprite { src: s.relativo, caixa: caixa(x0, y0, sw, sh, w, h), opacidade: 1.0 }])
}

#[cfg(test)]
mod testes {
    use serde_json::{json, Value};

    use crate::edicao::camada::formas::testes::{base, com, estilo, fixture, quadro, rodar_caso, CAIXA, CANVAS};
    use crate::edicao::geometria::{a_de, Afim};
    use crate::edicao::medidas::testes::{camera, em01, fila, linha, marca, plano, LINHA};
    use crate::edicao::modelo::{Efeito, TamanhoPx, Tolerante};

    fn a_43() -> Afim {
        a_de(&[], 0.0, TamanhoPx { w: 1440, h: 1080 }, CANVAS)
    }

    fn a_girado() -> Afim {
        let girar: Efeito = serde_json::from_value(json!({ "tipo": "girar", "id": "g", "ligado": true, "nome": "", "ativo": null,
            "graus": 90, "espelhar_h": false, "espelhar_v": false })).unwrap();
        a_de(&[Tolerante::Conhecido(girar)], 0.0, TamanhoPx { w: 1280, h: 720 }, CANVAS)
    }

    #[allow(clippy::too_many_arguments)]
    fn regua(id: &str, cal: Value, ancora: (f64, f64), dir: f64, m: f64, div: u32, traco: Value, rotulo: bool) -> Value {
        com(base("regua", id), json!({ "calibracao": cal, "ancora": em01(ancora), "direcao_graus": dir, "comprimento_m": m, "divisoes": div,
            "traco": traco, "rotulo": rotulo }))
    }

    fn velocidade(id: &str, kmh: f64, faixa: Value, onde: Value) -> Value {
        com(base("velocidade_exibida", id), json!({ "calculo": { "id": "c", "kmh": kmh, "faixa": faixa, "de_s": 0.0, "ate_s": 1.0, "metodo": "monte_carlo" },
            "onde": onde, "estilo": estilo("sans", true, false) }))
    }

    /// Fixture do espelho (`palco/desenho/medidas.ts`): `SICRO_GRAVAR_FIXTURES=1` regrava.
    #[test]
    fn fixture_primitivas_medidas() {
        let q = quadro(CANVAS, Afim::IDENTIDADE, &[("r1", CAIXA)]);
        let regua0 = com(base("regua", "g1"), json!({ "calibracao": { "id": "c", "metodo": "plane", "h": [1, 0, 0, 0, 1, 0, 0, 0, 1], "espaco": { "w": 1920, "h": 1080 },
            "pontos_sha256": "", "residuo_m": 0.01, "regiao": [] }, "ancora": { "x": 0.5, "y": 0.5 }, "direcao_graus": 0.0, "comprimento_m": 1.0,
            "divisoes": 10, "traco": { "cor": "#ffffff", "espessura": 0.004, "contorno": true }, "rotulo": true }));
        let velocidade0 = com(base("velocidade_exibida", "v1"), json!({ "calculo": { "id": "c", "kmh": 42.0, "faixa": [39.0, 45.0], "de_s": 0.0, "ate_s": 1.0, "metodo": "x" },
            "onde": { "modo": "fixa", "lugar": { "x": 0.5, "y": 0.9, "h": "centro", "v": "base" } }, "estilo": estilo("sans", false, false) }));
        let (plano, linha, fila) = (serde_json::to_value(plano()).unwrap(), serde_json::to_value(linha()).unwrap(), serde_json::to_value(fila()).unwrap());
        let amarelo = json!({ "cor": "#ffd84a", "espessura": 0.006, "contorno": true });
        let vermelho = json!({ "cor": "#ff000080", "espessura": 0.003, "contorno": false });
        let q43 = quadro(CANVAS, a_43(), &[("r1", CAIXA), ("p1", [0.3, 0.6, 0.0, 0.0])]);
        let qgir = quadro(CANVAS, a_girado(), &[]);
        let ((x1, y1), _, _) = LINHA;
        let mut desligada = regua("g9", plano.clone(), camera(2.0, 3.0), 0.0, 1.0, 4, amarelo.clone(), true);
        desligada["ligado"] = json!(false);
        let mut curta = regua("g8", plano.clone(), camera(2.0, 3.0), 0.0, 1.0, 4, amarelo.clone(), true);
        curta["calibracao"]["h"] = json!([1, 0, 0, 0, 1, 0, 0, 0]);
        let fixa = |lugar: Value| json!({ "modo": "fixa", "lugar": lugar });
        let segue = |r: &str, dx: f64, dy: f64| json!({ "modo": "segue", "rastreio": r, "dx": dx, "dy": dy });
        let casos: Vec<(&str, Value, &crate::edicao::camada::formas::QuadroCamada)> = vec![
            ("regua", regua0, &q),
            ("velocidade_exibida", velocidade0, &q),
            ("régua plane em perspectiva, quadro 4:3, sem contorno", regua("g2", plano.clone(), camera(2.0, 3.0), 30.0, 2.0, 10, vermelho.clone(), true), &q43),
            ("régua plane para cima e para a esquerda", regua("g3", plano.clone(), camera(6.0, 4.0), 135.0, 1.5, 3, amarelo.clone(), true), &q),
            ("régua plane para baixo (rótulo embaixo)", regua("g4", plano.clone(), camera(3.0, 6.0), 270.0, 2.0, 4, amarelo.clone(), true), &q),
            ("régua plane sem divisões e sem rótulo", regua("g5", plano.clone(), camera(4.0, 5.0), -20.0, 1.0, 0, amarelo.clone(), false), &q),
            ("régua line no quadro girado", regua("g6", linha.clone(), (x1, y1), 0.0, 1.0, 5, amarelo.clone(), true), &qgir),
            ("régua cross_ratio para trás", regua("g7", fila.clone(), marca(9.0, 0.0), 180.0, 2.0, 4, amarelo.clone(), true), &q),
            ("régua atrás da câmera", regua("g10", plano.clone(), camera(4.0, 5.0), 270.0, 20.0, 2, amarelo.clone(), true), &q),
            ("régua com divisões acima do teto", regua("g11", plano.clone(), camera(4.0, 5.0), 0.0, 1.0, 101, amarelo.clone(), true), &q),
            ("régua plane com a ponta fora da região", regua("g12", plano.clone(), camera(6.0, 5.0), 0.0, 3.0, 2, amarelo.clone(), true), &q),
            ("régua plane com a âncora fora da região", regua("g13", plano.clone(), camera(-1.0, 5.0), 0.0, 1.0, 2, amarelo.clone(), true), &q),
            ("régua plane encostada na borda da região", regua("g14", plano, camera(6.0, 5.0), 0.0, 2.0, 2, amarelo.clone(), false), &q),
            ("régua line além do segmento", regua("g15", linha, (x1, y1), 180.0, 1.0, 4, amarelo.clone(), true), &q),
            ("régua cross_ratio com a âncora antes da primeira marca", regua("g16", fila.clone(), marca(-0.5, 0.0), 0.0, 1.0, 5, amarelo.clone(), true), &q),
            ("régua cross_ratio além da última marca", regua("g17", fila.clone(), marca(11.5, 0.0), 0.0, 1.0, 5, amarelo.clone(), true), &q),
            ("régua cross_ratio até a última marca", regua("g18", fila, marca(11.0, 0.0), 0.0, 1.0, 2, amarelo, false), &q),
            ("régua homografia com 8 números", curta, &q),
            ("régua desligada", desligada, &q),
            ("velocidade sem faixa acompanhando a caixa, 4:3", velocidade("v2", 30.5, Value::Null, segue("r1", 0.0, -0.08)), &q43),
            ("velocidade acompanhando um ponto", velocidade("v3", 61.2, json!([58.9, 63.4]), segue("p1", 0.05, 0.0)), &q43),
            ("velocidade com o rastreio fora do quadro", velocidade("v4", 42.0, Value::Null, segue("r9", 0.0, 0.0)), &q43),
            ("velocidade baixa, uma casa, no canto", velocidade("v5", 7.25, json!([6.04, 8.51]), fixa(json!({ "x": 0.98, "y": 0.02, "h": "dir", "v": "topo" }))), &q),
        ];
        fixture("primitivas-medidas.json", casos.into_iter().map(|(n, e, q)| (n.to_string(), e, q.clone())).collect());
    }

    /// A régua no canvas: as pontas passam por A (4:3 com barras); o caminho começa na âncora e termina na ponta.
    #[test]
    fn regua_no_canvas_passa_por_a() {
        let a = a_43();
        let e = regua("g2", serde_json::to_value(plano()).unwrap(), camera(2.0, 3.0), 30.0, 2.0, 10, json!({ "cor": "#ffffff", "espessura": 0.004, "contorno": false }), false);
        let saida = rodar_caso(&e, &quadro(CANVAS, a, &[]));
        let d = &saida["saida"]["primitivas"][0]["d"];
        let ef: crate::edicao::modelo::EfRegua = serde_json::from_value(e.clone()).unwrap();
        let v = crate::edicao::medidas::pontas_da_regua(&ef).unwrap();
        let px = |p: &Value| (p[1].as_f64().unwrap() * 1920.0, p[2].as_f64().unwrap() * 1080.0);
        for (i, p) in [(0usize, v[0]), (10, v[10])] {
            let c = a.ponto(p);
            let (x, y) = px(&d[i]);
            assert!((x - c.x * 1920.0).abs() < 1e-6 && (y - c.y * 1080.0).abs() < 1e-6, "{i}: ({x}, {y}) × {c:?}");
        }
        // 4:3 em 16:9: tudo dentro da faixa da imagem (240..1680 px)
        for s in d.as_array().unwrap() {
            let (x, _) = px(s);
            assert!((240.0..=1680.0).contains(&x), "{s}");
        }
    }

    /// A velocidade que acompanha: o centro do texto fica no centro da caixa do rastreio no canvas mais o deslocamento.
    #[test]
    fn velocidade_acompanha_o_rastreio() {
        let a = a_43();
        let q = quadro(CANVAS, a, &[("r1", CAIXA)]);
        let saida = rodar_caso(&velocidade("v2", 30.5, Value::Null, json!({ "modo": "segue", "rastreio": "r1", "dx": 0.1, "dy": -0.08 })), &q);
        let c = &saida["saida"]["primitivas"][0]["caixa"];
        let (x, y, w, h) = (c["x"].as_f64().unwrap(), c["y"].as_f64().unwrap(), c["w"].as_f64().unwrap(), c["h"].as_f64().unwrap());
        let r = crate::edicao::geometria::caixa_no_canvas(&a, crate::edicao::modelo::Caixa { x: CAIXA[0], y: CAIXA[1], w: CAIXA[2], h: CAIXA[3] });
        let (cx, cy) = ((r.x + r.w * 0.5 + 0.1) * 1920.0, (r.y + r.h * 0.5 - 0.08) * 1080.0);
        // ao pixel: até meio pixel
        assert!(((x + w * 0.5) * 1920.0 - cx).abs() <= 0.5 + 1e-9 && ((y + h * 0.5) * 1080.0 - cy).abs() <= 0.5 + 1e-9, "{c} × ({cx}, {cy})");
        assert_eq!(saida["entrada"]["sprites"][0]["pedido"]["blocos"][0]["conteudo"], "≈ 31 km/h");
    }
}
