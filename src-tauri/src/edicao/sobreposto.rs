//! Entrada e saída dos clipes sobrepostos (título, texto livre, imagem): a `Transformacao` avaliada num instante
//! local, com a opacidade de `entrada_s`/`saida_s` e (Fase 4) a `animacao` (corte, aparecer, deslizar).
//! Espelho: `src/modules/edicao/modelo/sobreposto.ts`, pela fixture `sobreposto-transformacao.json`.
//! Dono: pacote do texto livre (contratos-fase4.md); a camada global (`motor/sobrepor.rs`) só chama.

use crate::edicao::curva;
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{Lado, SobBase, Surgir};
use crate::edicao::motor::plano::TransformacaoAvaliada;
use crate::error::{Result, SicroError};

/// Deslocamento do "deslizar" no começo da entrada (e no fim da saída): fração do canvas no eixo do `lado`.
pub const DESLIZE: f64 = 0.08;

/// Detalhe do `nao_suportado` da `animacao`; None = renderiza. Regra única do pacote: a validação e a camada global
/// perguntam aqui (regra 2 de contratos-fase4.md). O motor faz corte, aparecer e deslizar nos dois lados: nada fica
/// recusado.
pub fn recusa(b: &SobBase) -> Option<&'static str> {
    let _ = b;
    None
}

/// A `recusa` como erro (`<código>: <detalhe> <id>`), para a camada global.
pub fn conferir(b: &SobBase) -> Result<()> {
    match recusa(b) {
        Some(d) => Err(SicroError::Validation(format!("{}: {d} {}", codigo::NAO_SUPORTADO, b.id))),
        None => Ok(()),
    }
}

/// `Transformacao` no tempo local `tau` de um clipe de `dur` s, com a entrada e a saída (contratos-fase4.md D4).
/// Fração da entrada `ke = tau / entrada_s` e da saída `ks = (dur − tau) / saida_s`, em 0..1 ("corte", duração não
/// positiva ou não finita = 1). Aparecer e deslizar multiplicam a opacidade pela fração; deslizar soma
/// `(1 − suave(k)) × DESLIZE` no eixo do `lado` (cima −y, baixo +y, esq −x, dir +x; a saída vai para o mesmo lado).
/// Sem `animacao` = aparecer nos dois lados: a rampa de opacidade de antes da Fase 4, bit a bit.
pub fn transformacao_em(b: &SobBase, tau: f64, dur: f64) -> TransformacaoAvaliada {
    let t = &b.transformacao;
    let (entrada, saida) = b.animacao.map_or((Surgir::Aparecer, Surgir::Aparecer), |a| (a.entrada, a.saida));
    let fracao = |s: Surgir, v: f64, d: f64| if s != Surgir::Corte && d > 0.0 && d.is_finite() { (v / d).clamp(0.0, 1.0) } else { 1.0 };
    let (ke, ks) = (fracao(entrada, tau, b.entrada_s), fracao(saida, dur - tau, b.saida_s));
    let opacidade = curva::avaliar(&t.opacidade, tau).clamp(0.0, 1.0) * ke * ks;
    let (mut x, mut y) = (curva::avaliar(&t.x, tau), curva::avaliar(&t.y, tau));
    let mut desloc = 0.0;
    for (s, k) in [(entrada, ke), (saida, ks)] {
        if s == Surgir::Deslizar {
            desloc += (1.0 - curva::suave(k)) * DESLIZE;
        }
    }
    // só com deslocamento: sem ele x e y saem como avaliados (−0 inclusive)
    if desloc != 0.0 {
        match b.animacao.map_or(Lado::Baixo, |a| a.lado) {
            Lado::Cima => y -= desloc,
            Lado::Baixo => y += desloc,
            Lado::Esq => x -= desloc,
            Lado::Dir => x += desloc,
        }
    }
    TransformacaoAvaliada { x, y, escala: curva::avaliar(&t.escala, tau), rotacao: curva::avaliar(&t.rotacao, tau), opacidade }
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::{json, Value};

    fn base(extra: Value) -> SobBase {
        let mut v = json!({ "id": "s1", "rotulo": "", "pos": { "ref": "programa", "t": 0.0 }, "duracao_s": 2.0,
            "transformacao": { "x": 0.25, "y": [{ "t": 0.0, "v": 0.2 }, { "t": 2.0, "v": 0.8, "interp": "suave" }], "escala": 1.0, "rotacao": 30.0,
                               "opacidade": [{ "t": 0.0, "v": 1.0 }, { "t": 1.0, "v": 0.5 }] },
            "entrada_s": 0.5, "saida_s": 0.25, "efeitos": [] });
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        serde_json::from_value(v).unwrap()
    }

    fn anima(entrada: &str, saida: &str, lado: &str) -> Value {
        json!({ "animacao": { "entrada": entrada, "saida": saida, "lado": lado } })
    }

    /// Os casos do espelho (`modelo/sobreposto.ts`): os 27 da abertura (sem `animacao`, "aparecer" nos dois lados e
    /// sem rampa, que dão o mesmo) e os do texto livre: corte e deslizar em cada lado, entrada e saída que se
    /// cruzam num clipe curto, e o −0 que só o deslocamento muda.
    #[test]
    fn fixture_transformacao() {
        let mut casos = Vec::new();
        let taus = [0.0, 0.1, 0.25, 0.5, 1.0, 1.8, 1.9, 2.0, 2.5];
        let mut caso = |nome: String, b: &SobBase, tau: f64, dur: f64| {
            casos.push(json!({ "nome": nome, "entrada": { "base": serde_json::to_value(b).unwrap(), "tau": tau, "dur": dur },
                               "saida": serde_json::to_value(transformacao_em(b, tau, dur)).unwrap() }));
        };
        for (nome, extra) in [
            ("sem animação", json!({})),
            ("aparecer nos dois lados", anima("aparecer", "aparecer", "baixo")),
            ("sem rampa", json!({ "entrada_s": 0.0, "saida_s": -1.0 })),
        ] {
            let b = base(extra);
            for tau in taus {
                caso(format!("{nome} em {tau}"), &b, tau, 2.0);
            }
        }
        for (e, s) in [("corte", "corte"), ("corte", "deslizar"), ("deslizar", "corte"), ("deslizar", "deslizar"), ("deslizar", "aparecer"), ("aparecer", "corte")] {
            for lado in ["cima", "baixo", "esq", "dir"] {
                let b = base(anima(e, s, lado));
                for tau in taus {
                    caso(format!("{e} e {s} pelo lado {lado} em {tau}"), &b, tau, 2.0);
                }
            }
        }
        // clipe de 0,6 s com entrada de 0,5 s e saída de 0,25 s: as duas valem juntas no meio
        let b = base(anima("deslizar", "deslizar", "dir"));
        for tau in [0.0, 0.2, 0.35, 0.4, 0.5, 0.6] {
            caso(format!("deslizar cruzado em {tau}"), &b, tau, 0.6);
        }
        let b = base(json!({ "entrada_s": 0.0, "saida_s": -1.0, "animacao": { "entrada": "deslizar", "saida": "deslizar", "lado": "cima" } }));
        for tau in [0.0, 2.0] {
            caso(format!("deslizar sem duração em {tau}"), &b, tau, 2.0);
        }
        // −0 no x: sai −0 sem deslocamento e +0 quando a entrada desliza no eixo x
        for (nome, extra) in [("sem animação", json!({})), ("deslizar", anima("deslizar", "corte", "dir")), ("deslizar acabado", anima("deslizar", "aparecer", "esq"))] {
            let mut b = base(extra);
            b.transformacao.x = crate::edicao::modelo::Animavel::Fixo(-0.0);
            for tau in [0.25, 1.0] {
                caso(format!("x −0 {nome} em {tau}"), &b, tau, 2.0);
            }
        }
        crate::edicao::curva::fixtures::conferir("sobreposto-transformacao.json", Value::Array(casos));
    }

    #[test]
    fn nada_e_recusado() {
        assert_eq!(recusa(&base(json!({}))), None);
        for (e, s) in [("aparecer", "aparecer"), ("corte", "aparecer"), ("aparecer", "deslizar"), ("deslizar", "corte")] {
            let b = base(anima(e, s, "dir"));
            assert_eq!(recusa(&b), None, "{e} {s}");
            assert!(conferir(&b).is_ok());
        }
    }

    /// As contas do D4 conferidas por fora: sem animação = a rampa de antes, bit a bit; corte = opacidade cheia nas
    /// pontas; deslizar no meio da entrada = metade do deslocamento (suave(½) = ½) com metade da opacidade.
    #[test]
    fn contas_do_deslizar_e_do_corte() {
        let antes = |b: &SobBase, tau: f64, dur: f64| {
            let rampa = |v: f64, d: f64| if d > 0.0 && d.is_finite() { (v / d).clamp(0.0, 1.0) } else { 1.0 };
            curva::avaliar(&b.transformacao.opacidade, tau).clamp(0.0, 1.0) * rampa(tau, b.entrada_s) * rampa(dur - tau, b.saida_s)
        };
        for extra in [json!({}), anima("aparecer", "aparecer", "cima")] {
            let b = base(extra);
            for i in 0..=40 {
                let tau = i as f64 * 0.0625 - 0.25;
                let t = transformacao_em(&b, tau, 2.0);
                assert_eq!(t.opacidade.to_bits(), antes(&b, tau, 2.0).to_bits(), "{tau}");
                assert_eq!(t.x.to_bits(), curva::avaliar(&b.transformacao.x, tau).to_bits());
                assert_eq!(t.y.to_bits(), curva::avaliar(&b.transformacao.y, tau).to_bits());
            }
        }
        let b = base(anima("corte", "corte", "baixo"));
        assert_eq!(transformacao_em(&b, 0.0, 2.0).opacidade, 1.0);
        assert_eq!(transformacao_em(&b, 2.0, 2.0).opacidade, 0.5, "só a curva de opacidade (0,5 depois de 1 s)");
        for (lado, dx, dy) in [("cima", 0.0, -1.0), ("baixo", 0.0, 1.0), ("esq", -1.0, 0.0), ("dir", 1.0, 0.0)] {
            let b = base(anima("deslizar", "deslizar", lado));
            let parado = transformacao_em(&base(json!({})), 0.25, 2.0);
            let meio = transformacao_em(&b, 0.25, 2.0);
            assert!((meio.x - (parado.x + dx * DESLIZE / 2.0)).abs() < 1e-12 && (meio.y - (parado.y + dy * DESLIZE / 2.0)).abs() < 1e-12, "{lado}: {meio:?}");
            assert_eq!(meio.opacidade, parado.opacidade, "{lado}");
            let comeco = transformacao_em(&b, 0.0, 2.0);
            assert!((comeco.x - (0.25 + dx * DESLIZE)).abs() < 1e-12 && (comeco.y - (0.2 + dy * DESLIZE)).abs() < 1e-12, "{lado}: {comeco:?}");
            assert_eq!(comeco.opacidade, 0.0, "{lado}");
            // saída: no fim, o deslocamento inteiro para o mesmo lado
            let fim = transformacao_em(&b, 2.0, 2.0);
            let y_fim = curva::avaliar(&b.transformacao.y, 2.0);
            assert!((fim.x - (0.25 + dx * DESLIZE)).abs() < 1e-12 && (fim.y - (y_fim + dy * DESLIZE)).abs() < 1e-12, "{lado}: {fim:?}");
        }
    }

    // ------------------------------------------------------------------ no render (dois ffmpeg)

    use crate::edicao::camada::texto::bitmap_sprite;
    use crate::edicao::camada::titulo;
    use crate::edicao::modelo::{ClipeSobreposto, TamanhoPx};
    use crate::edicao::testes::{aceite, executor, fontes, medir};

    const W: u32 = 320;
    const H: u32 = 180;
    const FPS: f64 = 30.0;

    /// Texto livre de 1 s no começo do programa, verde sobre a caixa verde (um retângulo cheio: o centro da tinta
    /// é o centro do bloco), no centro (0,3; 0,6) do canvas.
    fn texto_livre(animacao: Value) -> Value {
        let mut c = json!({ "tipo": "titulo", "id": "tl", "rotulo": "", "pos": { "ref": "programa", "t": 0.0 }, "duracao_s": 1.0,
            "transformacao": { "x": 0.3, "y": 0.6, "escala": 1.0, "rotacao": 0.0, "opacidade": 1.0 }, "entrada_s": 0.3, "saida_s": 0.3, "efeitos": [],
            "titulo": { "modelo": "livre", "fundo": { "tipo": "transparente" }, "blocos": [{ "id": "b1", "papel": "corpo", "conteudo": "Texto",
                "estilo": { "fonte": "sans", "negrito": true, "italico": false, "tamanho": 0.09, "cor": "#00e000", "alinhamento": "centro",
                            "caixa": { "cor": "#00e000", "margem": 0.03, "raio": 0.0 }, "contorno": null },
                "lugar": { "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" }, "largura_max": 0.6 }] } });
        if !animacao.is_null() {
            c["animacao"] = animacao;
        }
        c
    }

    /// Peso de "verde" do pixel (o fundo cinza dá 0; mistura com o fundo dá a fração do verde).
    fn verde(p: &[u8]) -> f64 {
        (p[1] as f64 - (p[0].max(p[2])) as f64).max(0.0)
    }

    /// Centro (px) e massa do verde no quadro.
    fn centro_do_verde(q: &medir::QuadroRgb) -> (f64, f64, f64) {
        let (mut sx, mut sy, mut s) = (0.0, 0.0, 0.0);
        for (i, p) in q.px.chunks_exact(3).enumerate() {
            let v = verde(p);
            sx += v * ((i as u32 % q.w) as f64 + 0.5);
            sy += v * ((i as u32 / q.w) as f64 + 0.5);
            s += v;
        }
        (sx / s.max(1e-9), sy / s.max(1e-9), s)
    }

    fn renderizar(cap: &std::sync::Arc<crate::edicao::ffmpeg::sonda::Capacidades>, livre: Value, nome: &str) -> Vec<medir::QuadroRgb> {
        let fundo = json!({ "tipo": "titulo", "id": "A", "rotulo": "", "duracao_s": 1.0, "transicao_entrada": null,
                            "titulo": { "modelo": "cartela", "fundo": { "tipo": "cor", "cor": "#202020" }, "blocos": [] } });
        let p = aceite::projeto(W, H, (30, 1), vec![], vec![fundo], json!({ "sobreposicoes": [{ "id": "s1", "nome": "", "oculta": false, "clipes": [livre] }] }));
        let plano = aceite::Mundo::novo(cap).compilar(&p, &aceite::alvo_final("cpu")).unwrap_or_else(|e| panic!("{nome}: {e}"));
        let pasta = fontes::pasta_temporaria(nome);
        let r = executor::rodar(cap, &plano, &pasta, &executor::Opcoes { sem_perdas: true, ..Default::default() }).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
        let q: Vec<_> = r.blocos.iter().flat_map(|b| medir::quadros(cap, b).unwrap()).collect();
        let _ = std::fs::remove_dir_all(pasta);
        assert_eq!(q.len(), 30, "{nome}");
        q
    }

    /// Centro esperado (px do canvas) do bloco no slot: o sprite posto no meio do canvas (arredondado ao pixel, como
    /// `camada::titulo`) e levado pela transformação avaliada.
    fn centro_esperado(livre: &Value, tau: f64) -> (f64, f64, f64) {
        let c: ClipeSobreposto = serde_json::from_value(livre.clone()).unwrap();
        let ClipeSobreposto::Titulo(x) = c else { unreachable!() };
        let (s, _) = bitmap_sprite(&titulo::pedido(&x.titulo, 0, TamanhoPx { w: W, h: H })).unwrap();
        let (sw, sh) = (s.w as f64, s.h as f64);
        let (x0, y0) = ((W as f64 * 0.5 - sw * 0.5 + 0.5).floor(), (H as f64 * 0.5 - sh * 0.5 + 0.5).floor());
        let t = transformacao_em(&x.base, tau, 1.0);
        (x0 + sw * 0.5 + t.x * W as f64 - W as f64 * 0.5, y0 + sh * 0.5 + t.y * H as f64 - H as f64 * 0.5, t.opacidade)
    }

    /// Deslizar (pelos lados baixo e esq) e corte no render, nos dois ffmpeg: em todo slot com o texto visível (opacidade
    /// ≥ 0,2) o centro da tinta fica a até 1 px do previsto por `transformacao_em` e a massa acompanha a opacidade;
    /// com corte, o primeiro e o último slot já saem cheios; com aparecer (sem animação), o primeiro sai vazio.
    #[test]
    fn deslizar_e_corte_no_render() {
        let Some(cap) = fontes::cap() else { return };
        // massa do texto parado (opacidade 1), do caso sem animação no meio do clipe
        let parado = texto_livre(Value::Null);
        let q0 = renderizar(&cap, parado.clone(), "tl-render-parado");
        let cheia = centro_do_verde(&q0[15]).2;
        assert!(cheia > 1000.0, "texto parado sem tinta: {cheia}");
        assert!(centro_do_verde(&q0[0]).2 < cheia * 0.02, "aparecer: o primeiro slot sai vazio");
        let mut deslocados = 0;
        for (nome, animacao) in [
            ("deslizar-baixo", json!({ "entrada": "deslizar", "saida": "deslizar", "lado": "baixo" })),
            ("deslizar-esq", json!({ "entrada": "deslizar", "saida": "aparecer", "lado": "esq" })),
            ("corte-dir", json!({ "entrada": "corte", "saida": "corte", "lado": "dir" })),
        ] {
            let livre = texto_livre(animacao.clone());
            let q = renderizar(&cap, livre.clone(), &format!("tl-render-{nome}"));
            for (k, quadro) in q.iter().enumerate() {
                let tau = k as f64 / FPS;
                let (ex, ey, op) = centro_esperado(&livre, tau);
                let (mx, my, massa) = centro_do_verde(quadro);
                assert!((massa / cheia - op).abs() < 0.06, "{nome} ({}) slot {k}: massa {:.3} × opacidade {op:.3}", cap.versao, massa / cheia);
                if op < 0.2 {
                    continue;
                }
                assert!((mx - ex).abs() <= 1.0 && (my - ey).abs() <= 1.0, "{nome} ({}) slot {k}: centro ({mx:.2}, {my:.2}) × ({ex:.2}, {ey:.2})", cap.versao);
                let (px, py, _) = centro_esperado(&parado, 0.5);
                if (ex - px).abs() > 1.5 || (ey - py).abs() > 1.5 {
                    deslocados += 1;
                }
            }
            if animacao["entrada"] == "corte" {
                assert!((centro_do_verde(&q[0]).2 / cheia - 1.0).abs() < 0.03 && (centro_do_verde(&q[29]).2 / cheia - 1.0).abs() < 0.03, "{nome}: corte sem rampa");
            }
        }
        // o deslize de fato andou em vários slots medidos (entrada e saída do baixo, entrada do esq)
        assert!(deslocados >= 12, "{deslocados} slots deslocados");
    }

    /// Amostras para olhar (não mede): três textos livres com os padrões da tela sobre um vídeo 1280×720, renderizados
    /// pelo motor; grava PNG de quadros do deslize e do texto parado em `SICRO_TL_AMOSTRAS`.
    #[test]
    #[ignore = "amostras visuais: SICRO_TL_AMOSTRAS=<pasta>"]
    fn amostras_visuais() {
        let Some(cap) = fontes::cap() else { return };
        let Some(destino) = std::env::var_os("SICRO_TL_AMOSTRAS").map(std::path::PathBuf::from) else { return };
        std::fs::create_dir_all(&destino).unwrap();
        let pasta = fontes::pasta_temporaria("tl-amostras");
        let video = pasta.join("cena.mp4");
        let s = std::process::Command::new(&cap.binario)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i",
                   "gradients=s=1280x720:c0=0x24405f:c1=0xb7a37e:c2=0x55663f:c3=0x8fa3b5:nb_colors=4:speed=0.015:r=30:d=3",
                   "-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "30"])
            .arg(&video)
            .status()
            .unwrap();
        assert!(s.success());
        let m = crate::edicao::quadros::apoio::medir_por_conteudo(&cap, &video).unwrap();
        let tab = &m.info.quadros;
        let estilo = |cor: &str, caixa: Value, contorno: bool, sombra: bool| {
            let mut e = json!({ "fonte": "sans", "negrito": false, "italico": false, "tamanho": 0.052, "cor": cor, "alinhamento": "centro", "caixa": caixa,
                                "contorno": if contorno { json!({ "cor": "#000000", "largura": 0.003 }) } else { Value::Null } });
            if sombra {
                e["sombra"] = json!({ "cor": "#000000b3", "dx": 0.0, "dy": 0.003, "desfoque": 0.004 });
            }
            e
        };
        let livre = |id: &str, texto: &str, x: f64, y: f64, rot: f64, largura: f64, e: Value, animacao: Value| {
            let mut c = json!({ "tipo": "titulo", "id": id, "rotulo": "", "pos": { "ref": "programa", "t": 0.0 }, "duracao_s": 3.0,
                "transformacao": { "x": x, "y": y, "escala": 1.0, "rotacao": rot, "opacidade": 1.0 }, "entrada_s": 0.3, "saida_s": 0.3, "efeitos": [],
                "titulo": { "modelo": "livre", "fundo": { "tipo": "transparente" }, "blocos": [{ "id": format!("{id}b"), "papel": "corpo", "conteudo": texto,
                    "estilo": e, "lugar": { "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" }, "largura_max": largura }] } });
            if !animacao.is_null() {
                c["animacao"] = animacao;
            }
            c
        };
        let faixa = json!({ "cor": "#000000a0", "margem": 0.012, "raio": 0.0, "faixa": true });
        let caixa = json!({ "cor": "#000000", "margem": 0.012, "raio": 0.006 });
        let clipes = vec![
            livre("a", "Policiais chegam ao estabelecimento", 0.5, 0.86, 0.0, 0.6, estilo("#ffffff", faixa, false, false), json!({ "entrada": "deslizar", "saida": "aparecer", "lado": "baixo" })),
            livre("b", "Câmera 2 — 14/03/2026 22:41:07", 0.27, 0.09, 0.0, 0.5, estilo("#ffffff", Value::Null, true, true), Value::Null),
            livre("c", "Veículo branco", 0.73, 0.42, -6.0, 0.3, estilo("#ffd84a", caixa, false, true), json!({ "entrada": "deslizar", "saida": "corte", "lado": "dir" })),
        ];
        let p = aceite::projeto(1280, 720, (30, 1), vec![aceite::midia("m1", "video", json!([]))], vec![aceite::trecho("c1", "m1", tab, 0, 90, 1.0, json!([]))],
            json!({ "sobreposicoes": [{ "id": "s1", "nome": "", "oculta": false, "clipes": clipes }] }));
        let plano = aceite::Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &aceite::alvo_final("cpu")).unwrap();
        let r = executor::rodar(&cap, &plano, &pasta.join("render"), &executor::Opcoes { sem_perdas: true, ..Default::default() }).unwrap();
        let q: Vec<_> = r.blocos.iter().flat_map(|b| medir::quadros(&cap, b).unwrap()).collect();
        for k in [2usize, 5, 45] {
            let x = &q[k];
            let img = image::RgbImage::from_raw(x.w, x.h, x.px.clone()).unwrap();
            img.save(destino.join(format!("texto-livre-{}-q{k:02}.png", cap.versao.replace(['/', ' '], "_")))).unwrap();
        }
        let _ = std::fs::remove_dir_all(pasta);
    }
}
