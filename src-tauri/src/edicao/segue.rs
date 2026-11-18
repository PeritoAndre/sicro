//! Zoom de câmera que acompanha a pessoa (Fase 4, D9): o centro da região do `EfEnquadramento` segue um rastreio,
//! suavizado; o tamanho vem de `regiao` (fixa ou com chaves); a região nunca sai da imagem (desliza na borda).
//! A cauda do clipe (`motor/clipe.rs`) troca o efeito por um com chaves por slot ANTES do enquadramento e de `A(τ)`:
//! o resto do motor (perspective, camadas, lupa) não muda. Espelho: `src/modules/edicao/modelo/segue.ts`, pelas
//! fixtures `segue-*.json`. Dono: pacote do zoom que segue (contratos-fase4.md).
//!
//! Amostras = o centro da caixa do rastreio em cada quadro da mídia dele (tabela do decodificador), levado ao espaço
//! girado. Tudo depende só do instante e da mídia, nunca da janela renderizada: bloco, prévia e quadro final dão o
//! mesmo slot.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::edicao::curva;
use crate::edicao::geometria::{self, Afim};
use crate::edicao::modelo::{Animavel, BaseRastreio, Caixa, Chave, ClipePrincipal, EfEnquadramento, Efeito, Interp, Ponto, ProjetoEdicao, Rastreio, Tolerante};
use crate::edicao::motor::clipe::{erro, Rastreios};
use crate::edicao::motor::Contexto;
use crate::edicao::quadros::TabelaQuadros;
use crate::edicao::rastreio::avaliar::Instante;
use crate::error::Result;

/// Suavização que "Acompanhar rastreio" grava (s, na base do rastreio).
pub const SUAVIZAR_PADRAO_S: f64 = 1.0;
/// Maior `suavizar_s` aceito (s); acima, `item_invalido`, e a conta usa este.
pub const SUAVIZAR_MAX_S: f64 = 10.0;

const INTEIRA: Caixa = Caixa { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

/// Detalhe do `nao_suportado`; None = renderiza. Convertida: o motor faz o zoom que segue (a validação e o despacho
/// do motor continuam perguntando aqui).
pub fn recusa(q: &EfEnquadramento) -> Option<&'static str> {
    let _ = q;
    None
}

/// Problemas do parâmetro (código do catálogo, campo), além da conferência do rastreio seguido (existe, mídia,
/// espaço, confirmação: a do destaque, feita pela validação): `suavizar_s` fora de 0..10 e enquadramento que segue
/// fora de um trecho (no congelado, na imagem e no título não há o que acompanhar no tempo).
pub fn problemas(q: &EfEnquadramento, p: &ProjetoEdicao, midia_do_clipe: Option<&str>) -> Vec<(&'static str, &'static str)> {
    let Some(s) = &q.segue else { return Vec::new() };
    let suavizar_ok = s.suavizar_s.is_finite() && (0.0..=SUAVIZAR_MAX_S).contains(&s.suavizar_s);
    let no_trecho = midia_do_clipe.is_some()
        && p.principal.iter().any(|c| match c {
            Tolerante::Conhecido(ClipePrincipal::Trecho(t)) => t.base.efeitos.iter().any(|e| matches!(e, Tolerante::Conhecido(Efeito::Enquadramento(x)) if x.base.id == q.base.id)),
            _ => false,
        });
    if suavizar_ok && no_trecho {
        Vec::new()
    } else {
        vec![("item_invalido", "segue")]
    }
}

/// Centro da caixa do rastreio num instante da base dele (no espaço girado); None = sem caixa ali.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct AmostraSegue {
    pub t: f64,
    pub centro: Option<Ponto>,
}

/// Largura da janela da média (s): 0 sem `segue` ou inválida; limitada a `SUAVIZAR_MAX_S`.
fn suavizar(e: &EfEnquadramento) -> f64 {
    match e.segue.as_ref().map(|s| s.suavizar_s) {
        Some(s) if s.is_finite() && s > 0.0 => s.min(SUAVIZAR_MAX_S),
        _ => 0.0,
    }
}

/// Região gravada em `t`; lista de chaves vazia = a imagem inteira (como `geometria::regiao_em`).
fn gravada(e: &EfEnquadramento, t: f64) -> Caixa {
    match &e.regiao {
        Animavel::Chaves(ks) if ks.is_empty() => INTEIRA,
        r => curva::avaliar(r, t),
    }
}

/// Canto do lado `lado` centrado em `c`, dentro de [0, 1]; lado ≥ 1 (ou inválido) = 0.
fn dentro(c: f64, lado: f64) -> f64 {
    if !(lado < 1.0) {
        return 0.0;
    }
    let v = c - lado / 2.0;
    if v < 0.0 {
        0.0
    } else if v > 1.0 - lado {
        1.0 - lado
    } else {
        v
    }
}

/// Centro em `t`: média (na ordem das amostras) das que têm centro em [t − s/2, t + s/2]; sem nenhuma, a mais
/// próxima com centro (empate: a anterior). `amostras` em ordem de `t`.
fn centro_em(amostras: &[AmostraSegue], t: f64, s: f64) -> Option<Ponto> {
    let (de, ate) = (t - s / 2.0, t + s / 2.0);
    let lo = amostras.partition_point(|a| a.t < de);
    let hi = amostras.partition_point(|a| a.t <= ate).max(lo);
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0u32);
    for c in amostras[lo..hi].iter().filter_map(|a| a.centro) {
        sx += c.x;
        sy += c.y;
        n += 1;
    }
    if n > 0 {
        return Some(Ponto { x: sx / n as f64, y: sy / n as f64 });
    }
    let antes = amostras[..lo].iter().rev().find_map(|a| a.centro.map(|c| (a.t, c)));
    let depois = amostras[hi..].iter().find_map(|a| a.centro.map(|c| (a.t, c)));
    match (antes, depois) {
        (Some((ta, a)), Some((td, d))) => Some(if t - ta <= td - t { a } else { d }),
        (Some((_, a)), None) => Some(a),
        (None, d) => d.map(|(_, d)| d),
    }
}

/// Região (0..1 da origem já girada) em cada instante de `em` (base do clipe): centro = média das `amostras` em
/// [t − suavizar_s/2, t + suavizar_s/2] (sem amostra na janela: a mais próxima; nenhuma: o centro de `regiao`),
/// tamanho = `regiao` em t, deslocada para dentro de [0, 1]² (lado de 1 ou mais fica em 0).
pub fn regiao_seguida(e: &EfEnquadramento, amostras: &[AmostraSegue], em: &[f64]) -> Vec<Caixa> {
    let s = suavizar(e);
    em.iter()
        .map(|&t| {
            let r = gravada(e, t);
            let c = centro_em(amostras, t, s).unwrap_or(Ponto { x: r.x + r.w / 2.0, y: r.y + r.h / 2.0 });
            Caixa { x: dentro(c.x, r.w), y: dentro(c.y, r.h), w: r.w, h: r.h }
        })
        .collect()
}

/// Giro e espelhos dos `girar` ligados, na ordem (a `regiao` mora na origem girada; o rastreio, na de antes).
fn giro(efeitos: &[Tolerante<Efeito>]) -> Afim {
    efeitos.iter().fold(Afim::IDENTIDADE, |g, e| match e {
        Tolerante::Conhecido(Efeito::Girar(gi)) if gi.base.ligado => g.seguida_de(&geometria::afim_giro(gi.graus as i32, gi.espelhar_h, gi.espelhar_v)),
        _ => g,
    })
}

/// Centro da caixa levado ao espaço girado. Espelho: `centroNoGirado` (fixture `segue-centro.json`).
pub fn centro_no_girado(c: Caixa, efeitos: &[Tolerante<Efeito>]) -> Ponto {
    giro(efeitos).ponto(Ponto { x: c.x + c.w / 2.0, y: c.y + c.h / 2.0 })
}

/// Centros por quadro da tabela, guardados os da janela (as buscas para fora não guardam: andam uma vez cada).
struct Centros<'a> {
    r: &'a Rastreio,
    tab: &'a TabelaQuadros,
    giro: Afim,
    feitos: HashMap<usize, Option<Ponto>>,
}

impl Centros<'_> {
    fn medir(&self, rs: &mut Rastreios<'_>, j: usize) -> Result<Option<Ponto>> {
        if let Some(c) = self.feitos.get(&j) {
            return Ok(*c);
        }
        let a = rs.em(self.r, Instante { t: self.tab.tempo(j), pts: Some(self.tab.pts[j]) })?;
        Ok(a.map(|a| self.giro.ponto(Ponto { x: a.caixa.x + a.caixa.w / 2.0, y: a.caixa.y + a.caixa.h / 2.0 })))
    }

    fn em(&mut self, rs: &mut Rastreios<'_>, j: usize) -> Result<Option<Ponto>> {
        let c = self.medir(rs, j)?;
        self.feitos.insert(j, c);
        Ok(c)
    }
}

/// As amostras que dão, em cada instante de `instantes` (crescentes), o mesmo que todos os quadros da mídia: os
/// quadros de cada janela e, na janela sem centro, o quadro com centro mais próximo de cada lado.
fn amostras(r: &Rastreio, tab: &TabelaQuadros, giro: Afim, instantes: &[f64], s: f64, rs: &mut Rastreios<'_>) -> Result<Vec<AmostraSegue>> {
    let n = tab.pts.len();
    // a mesma conta de `TabelaQuadros::tempo`
    let tempo = |p: i64| p as f64 * tab.tb.num as f64 / tab.tb.den as f64;
    let janelas: Vec<(usize, usize)> = instantes
        .iter()
        .map(|&t| {
            let lo = tab.pts.partition_point(|&p| tempo(p) < t - s / 2.0);
            (lo, tab.pts.partition_point(|&p| tempo(p) <= t + s / 2.0).max(lo))
        })
        .collect();
    let mut c = Centros { r, tab, giro, feitos: HashMap::new() };
    let mut usados: Vec<usize> = Vec::new();
    let mut ate = 0usize;
    for &(lo, hi) in &janelas {
        for j in lo.max(ate)..hi {
            c.em(rs, j)?;
            usados.push(j);
        }
        ate = ate.max(hi);
    }
    let vazias: Vec<(usize, usize)> = janelas.iter().copied().filter(|&(lo, hi)| (lo..hi).all(|j| c.feitos.get(&j).copied().flatten().is_none())).collect();
    // para trás em ordem crescente e para a frente em ordem decrescente: cada quadro é lido no máximo uma vez
    let mut ant: Option<(usize, Option<usize>)> = None;
    for &(lo, _) in &vazias {
        let (desde, mut achado) = match ant {
            Some((x, a)) if x <= lo => (x, a),
            _ => (0, None),
        };
        for j in (desde..lo).rev() {
            if c.medir(rs, j)?.is_some() {
                achado = Some(j);
                break;
            }
        }
        ant = Some((lo, achado));
        usados.extend(achado);
    }
    let mut seg: Option<(usize, Option<usize>)> = None;
    for &(_, hi) in vazias.iter().rev() {
        let (ate, mut achado) = match seg {
            Some((y, a)) if y >= hi => (y, a),
            _ => (n, None),
        };
        for j in hi..ate {
            if c.medir(rs, j)?.is_some() {
                achado = Some(j);
                break;
            }
        }
        seg = Some((hi, achado));
        usados.extend(achado);
    }
    usados.sort_unstable();
    usados.dedup();
    usados.into_iter().map(|j| Ok(AmostraSegue { t: tab.tempo(j), centro: c.medir(rs, j)? })).collect()
}

/// Chave antes de todas com a região deslocada dentro da imagem, do mesmo tamanho: o enquadramento que acompanha vai
/// sempre pelo caminho animado (`perspective`), em qualquer janela. Sem ela, a janela que só visse o alvo parado iria
/// pelo `crop` e o mesmo slot sairia diferente no quadro final e na exportação. Mesmo tamanho = mesmo destino e mesma
/// tolerância da tabela. Região que não cabe deslocar (os dois lados ≥ 1) é sempre fixa: sem chave.
fn sentinela(ks: &[Chave<Caixa>]) -> Option<Chave<Caixa>> {
    let k = ks.first()?;
    let v = k.v;
    let outro = if v.w < 1.0 {
        Caixa { x: if v.x > 0.0 { 0.0 } else { 1.0 - v.w }, ..v }
    } else if v.h < 1.0 {
        Caixa { y: if v.y > 0.0 { 0.0 } else { 1.0 - v.h }, ..v }
    } else {
        return None;
    };
    Some(Chave { t: k.t - 1.0, pts: None, v: outro, interp: Interp::Degrau })
}

/// Gancho da cauda do clipe: os efeitos com o enquadramento que segue trocado por um de chaves (degrau) nos instantes
/// `taus` (os slots da cauda, na base do clipe) e nos das chaves gravadas (a maior ampliação do clipe, que dá a
/// tolerância da tabela, não depende da janela). None = o enquadramento que vale não segue (todo projeto antigo).
pub(crate) fn na_cauda(efeitos: &[Tolerante<Efeito>], taus: &[f64], rastreios: &mut Rastreios<'_>, ctx: &Contexto<'_>) -> Result<Option<Vec<Tolerante<Efeito>>>> {
    // vale o primeiro ligado (geometria e motor)
    let Some((i, q)) = efeitos.iter().enumerate().find_map(|(i, e)| match e {
        Tolerante::Conhecido(Efeito::Enquadramento(q)) if q.base.ligado => Some((i, q)),
        _ => None,
    }) else {
        return Ok(None);
    };
    let Some(s) = &q.segue else { return Ok(None) };
    let r = rastreios.achar(&s.rastreio)?;
    let midia = match (r.base, r.midia.as_deref()) {
        (BaseRastreio::Fonte, Some(m)) => m,
        _ => return Err(erro("rastreio_base", &r.id)),
    };
    let tab = &ctx.fontes.get(midia).ok_or_else(|| erro("midia_ausente", midia))?.quadros;
    let mut instantes: Vec<f64> = taus.to_vec();
    if let Animavel::Chaves(ks) = &q.regiao {
        instantes.extend(ks.iter().map(|k| k.t));
    }
    instantes.retain(|t| t.is_finite());
    instantes.sort_by(f64::total_cmp);
    instantes.dedup();
    if instantes.is_empty() {
        return Ok(None);
    }
    let am = amostras(r, tab, giro(efeitos), &instantes, suavizar(q), rastreios)?;
    let mut chaves: Vec<Chave<Caixa>> = instantes.iter().zip(regiao_seguida(q, &am, &instantes)).map(|(&t, v)| Chave { t, pts: None, v, interp: Interp::Degrau }).collect();
    if let Some(k) = sentinela(&chaves) {
        chaves.insert(0, k);
    }
    let mut novo = q.clone();
    novo.regiao = Animavel::Chaves(chaves);
    novo.segue = None;
    let mut v = efeitos.to_vec();
    v[i] = Tolerante::Conhecido(Efeito::Enquadramento(novo));
    Ok(Some(v))
}

#[cfg(test)]
mod testes {
    use std::path::Path;
    use std::sync::Arc;

    use serde_json::{json, Value};

    use super::*;
    use crate::edicao::ffmpeg::sonda::Capacidades;
    use crate::edicao::modelo::{Nivel, SegueRastreio, TamanhoPx};
    use crate::edicao::motor::efeitos::enquadramento;
    use crate::edicao::motor::plano::Plano;
    use crate::edicao::motor::Alvo;
    use crate::edicao::quadros::apoio::{self, SemRastreios};
    use crate::edicao::testes::aceite::{self, alvo_final, ancora, ancoras_da_marca, efeito, midia, projeto, rastreio, trecho, Mundo};
    use crate::edicao::testes::executor::{self, Opcoes};
    use crate::edicao::testes::fontes::{self, Fonte};
    use crate::edicao::testes::medir::{self, QuadroRgb};
    use crate::edicao::{tempo, validar};

    fn enq(segue: Value) -> EfEnquadramento {
        let mut v = json!({ "id": "q1", "ligado": true, "nome": "", "ativo": null, "regiao": { "x": 0.25, "y": 0.25, "w": 0.5, "h": 0.5 },
                            "ajuste": "cobrir", "interpolacao": "bilinear" });
        if !segue.is_null() {
            v["segue"] = segue;
        }
        serde_json::from_value(v).unwrap()
    }

    fn com(regiao: Value, suavizar_s: f64) -> EfEnquadramento {
        EfEnquadramento { regiao: serde_json::from_value(regiao).unwrap(), segue: Some(SegueRastreio { rastreio: "r1".into(), suavizar_s }), ..enq(Value::Null) }
    }

    fn am(t: f64, c: Option<(f64, f64)>) -> AmostraSegue {
        AmostraSegue { t, centro: c.map(|(x, y)| Ponto { x, y }) }
    }

    #[test]
    fn recusa_convertida() {
        assert_eq!(recusa(&enq(Value::Null)), None);
        assert_eq!(recusa(&enq(json!({ "rastreio": "r1", "suavizar_s": SUAVIZAR_PADRAO_S }))), None);
    }

    /// Os casos do espelho (`modelo/segue.ts::regiaoSeguida`).
    #[test]
    fn fixture_regiao() {
        let fixa = json!({ "x": 0.25, "y": 0.25, "w": 0.5, "h": 0.5 });
        let chaves = json!([
            { "t": 1.0, "v": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 }, "interp": "linear" },
            { "t": 2.0, "v": { "x": 0.3, "y": 0.3, "w": 0.4, "h": 0.4 }, "interp": "suave" },
            { "t": 3.0, "v": { "x": 0.3, "y": 0.3, "w": 0.4, "h": 0.4 }, "interp": "degrau" },
            { "t": 4.0, "v": { "x": 0.1, "y": 0.2, "w": 0.7, "h": 0.6 }, "interp": "linear" }
        ]);
        let andando: Vec<AmostraSegue> = (0..90).map(|j| am(j as f64 / 30.0, Some((0.1 + j as f64 * 0.009, 0.7 - j as f64 * 0.003)))).collect();
        let com_buraco: Vec<AmostraSegue> = (0..90).map(|j| am(j as f64 / 30.0, (j < 20 || j > 60).then(|| (0.2 + j as f64 * 0.007, 0.5)))).collect();
        let instantes: Vec<f64> = vec![-1.0, 0.0, 0.1, 1.0 / 3.0, 0.5, 1.0, 1.25, 1.5, 1.7, 2.0, 2.5, 2.9, 3.0, 3.5, 4.0, 5.0];
        let casos: Vec<(&str, EfEnquadramento, Vec<AmostraSegue>)> = vec![
            ("sem amostras: o centro da região", com(fixa.clone(), 1.0), vec![]),
            ("média de 1 s", com(fixa.clone(), 1.0), andando.clone()),
            ("sem média (0 s)", com(fixa.clone(), 0.0), andando.clone()),
            ("média de 0,2 s", com(fixa.clone(), 0.2), andando.clone()),
            ("acima do teto (usa 10 s)", com(fixa.clone(), 25.0), andando.clone()),
            ("suavizar negativo (sem média)", com(fixa.clone(), -2.0), andando.clone()),
            ("buraco: a mais próxima, empate na anterior", com(fixa.clone(), 0.2), com_buraco.clone()),
            ("só antes", com(fixa.clone(), 0.0), vec![am(0.0, Some((0.3, 0.4))), am(0.5, None)]),
            ("só depois", com(fixa.clone(), 0.0), vec![am(0.5, None), am(1.0, Some((0.6, 0.45)))]),
            ("empate exato", com(fixa.clone(), 0.0), vec![am(0.0, Some((0.2, 0.2))), am(1.0, Some((0.8, 0.8)))]),
            ("nenhuma com centro", com(fixa.clone(), 1.0), vec![am(0.0, None), am(1.0, None)]),
            ("borda: desliza para dentro", com(fixa.clone(), 0.0), vec![am(0.0, Some((0.01, 0.99))), am(1.0, Some((0.99, 0.02))), am(2.0, Some((-0.3, 1.4)))]),
            ("chaves de tamanho (aproximar, segurar, afastar)", com(chaves.clone(), 0.5), andando.clone()),
            ("lado de 1 ou mais", com(json!({ "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.2 }), 0.0), andando.clone()),
            ("região vazia: a imagem inteira", com(json!([]), 1.0), andando.clone()),
        ];
        let lista: Vec<Value> = casos
            .into_iter()
            .map(|(nome, e, a)| {
                json!({ "nome": nome, "entrada": { "efeito": serde_json::to_value(&e).unwrap(), "amostras": a, "em": instantes },
                        "saida": regiao_seguida(&e, &a, &instantes) })
            })
            .collect();
        curva::fixtures::conferir("segue-regiao.json", Value::Array(lista));
    }

    /// Centro da caixa no espaço girado (espelho `centroNoGirado`).
    #[test]
    fn fixture_centro() {
        let caixa = Caixa { x: 0.1, y: 0.2, w: 0.3, h: 0.1 };
        let girar = |g: u32, h: bool, v: bool| efeito("girar", "g", json!({ "graus": g, "espelhar_h": h, "espelhar_v": v }));
        let casos: Vec<(&str, Value)> = vec![
            ("sem giro", json!([])),
            ("90", json!([girar(90, false, false)])),
            ("180 espelhado", json!([girar(180, true, false)])),
            ("270 e espelho vertical", json!([girar(270, false, true)])),
            ("dois giros", json!([girar(90, true, false), girar(90, false, false)])),
            ("giro desligado", json!([{ "tipo": "girar", "id": "g", "ligado": false, "nome": "", "ativo": null, "graus": 90, "espelhar_h": false, "espelhar_v": false }])),
        ];
        let lista: Vec<Value> = casos
            .into_iter()
            .map(|(nome, ef)| {
                let efs: Vec<Tolerante<Efeito>> = serde_json::from_value(ef.clone()).unwrap();
                json!({ "nome": nome, "entrada": { "caixa": caixa, "efeitos": ef }, "saida": centro_no_girado(caixa, &efs) })
            })
            .collect();
        curva::fixtures::conferir("segue-centro.json", Value::Array(lista));
    }

    /// A região fica dentro da imagem em qualquer centro (até fora dela) e tamanho até 1; lado ≥ 1 fica em 0.
    #[test]
    fn nunca_sai_do_quadro() {
        // gerador congruencial: a mesma sequência sempre
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut sorteio = || {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (x >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut vistos = (0, 0);
        for _ in 0..20_000 {
            let w = 0.02 + sorteio() * 1.2;
            let h = 0.02 + sorteio() * 1.2;
            let (cx, cy) = (sorteio() * 1.6 - 0.3, sorteio() * 1.6 - 0.3);
            let e = com(json!({ "x": 0.1, "y": 0.1, "w": w, "h": h }), sorteio() * 3.0);
            let a: Vec<AmostraSegue> = (0..5).map(|k| am(k as f64 * 0.25, Some((cx + k as f64 * 0.01, cy)))).collect();
            for r in regiao_seguida(&e, &a, &[0.0, 0.5, 1.0]) {
                assert_eq!((r.w, r.h), (w, h));
                if w <= 1.0 {
                    assert!(r.x >= 0.0 && r.x + r.w <= 1.0 + 1e-12, "{r:?}");
                    vistos.0 += 1;
                } else {
                    assert_eq!(r.x, 0.0);
                    vistos.1 += 1;
                }
                if h <= 1.0 {
                    assert!(r.y >= 0.0 && r.y + r.h <= 1.0 + 1e-12, "{r:?}");
                }
            }
        }
        assert!(vistos.0 > 1000 && vistos.1 > 1000, "{vistos:?}");
    }

    #[test]
    fn suavizacao_e_janela_fechada() {
        // 31 amostras de 1/30 s: a janela de 1 s em t = 0,5 pega de 0 a 1 inteiras (as pontas contam)
        let a: Vec<AmostraSegue> = (0..=30).map(|j| am(j as f64 / 30.0 * 1.0, Some((j as f64 / 100.0, 0.5)))).collect();
        let r = regiao_seguida(&com(json!({ "x": 0.0, "y": 0.0, "w": 0.2, "h": 0.2 }), 1.0), &a, &[0.5])[0];
        let media = (0..=30).map(|j| j as f64 / 100.0).sum::<f64>() / 31.0;
        assert!((r.x + 0.1 - media).abs() < 1e-12, "{r:?}");
        // sem média, exatamente a do instante
        let r = regiao_seguida(&com(json!({ "x": 0.0, "y": 0.0, "w": 0.2, "h": 0.2 }), 0.0), &a, &[0.5])[0];
        assert!((r.x + 0.1 - 0.15).abs() < 1e-12, "{r:?}");
    }

    // ------------------------------------------------------------------ cauda e motor, sem ffmpeg

    /// Rastreio manual que anda: âncora a cada 10 quadros (interpolada entre elas).
    fn r1_andando(t: &TabelaQuadros, n: usize) -> Value {
        let ks: Vec<Value> = (0..n)
            .step_by(10)
            .map(|j| {
                let x = 0.1 + 0.6 * ((j as f64) * 0.05).sin().abs();
                ancora(t.tempo(j), Some(t.pts[j]), [x, 0.4, 0.1, 0.2])
            })
            .collect();
        rastreio("r1", Some("m1"), "fonte", ks)
    }

    fn enq_segue(regiao: Value, suavizar_s: f64, ajuste: &str) -> Value {
        efeito("enquadramento", "q9", json!({ "regiao": regiao, "ajuste": ajuste, "interpolacao": "bilinear", "segue": { "rastreio": "r1", "suavizar_s": suavizar_s } }))
    }

    fn efeitos_do(p: &ProjetoEdicao) -> Vec<Tolerante<Efeito>> {
        match &p.principal[0] {
            Tolerante::Conhecido(ClipePrincipal::Trecho(t)) => t.base.efeitos.clone(),
            _ => unreachable!(),
        }
    }

    /// Bloco, prévia e quadro final dão o mesmo slot: a região de cada instante não depende da janela, o efeito vai
    /// sempre pelo caminho animado e a maior ampliação (tolerância da tabela) é a do clipe inteiro.
    #[test]
    fn janela_nao_muda_o_slot() {
        let (mundo, midias) = aceite::mundo_das_recusas();
        let t = mundo.fontes["m1"].quadros.clone();
        let ctx = Contexto { fontes: &mundo.fontes, cap: &mundo.cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
        let chaves = json!([
            { "t": t.tempo(60), "v": { "x": 0.2, "y": 0.2, "w": 0.6, "h": 0.6 } },
            { "t": t.tempo(120), "v": { "x": 0.3, "y": 0.3, "w": 0.3, "h": 0.3 } },
            { "t": t.tempo(200), "v": { "x": 0.3, "y": 0.3, "w": 0.3, "h": 0.3 } },
            { "t": t.tempo(260), "v": { "x": 0.0, "y": 0.0, "w": 0.9, "h": 0.9 } }
        ]);
        for (regiao, s) in [(json!({ "x": 0.3, "y": 0.3, "w": 0.4, "h": 0.4 }), 1.0), (chaves, 0.5), (json!({ "x": 0.3, "y": 0.3, "w": 0.4, "h": 0.4 }), 0.0)] {
            let p = projeto(320, 180, (30, 1), midias.clone(), vec![trecho("c1", "m1", &t, 0, 300, 1.0, json!([enq_segue(regiao.clone(), s, "cobrir")]))], json!({ "rastreios": [r1_andando(&t, 300)] }));
            let efs = efeitos_do(&p);
            let taus = |a: usize, b: usize| -> Vec<f64> { (a..b).map(|j| t.tempo(j)).collect() };
            let mut rs = Rastreios::novo(&p, &ctx);
            let inteiro = na_cauda(&efs, &taus(0, 300), &mut rs, &ctx).unwrap().expect("segue");
            let zoom_maximo = |e: &[Tolerante<Efeito>]| match e.iter().find_map(|x| match x {
                Tolerante::Conhecido(Efeito::Enquadramento(q)) => Some(q.regiao.clone()),
                _ => None,
            }) {
                Some(Animavel::Chaves(ks)) => ks.iter().map(|k| 1.0 / k.v.w).fold(0.0, f64::max),
                _ => unreachable!(),
            };
            for (a, b) in [(0usize, 16usize), (37, 38), (100, 116), (5, 299), (288, 300), (250, 251)] {
                let mut rs = Rastreios::novo(&p, &ctx);
                let janela = na_cauda(&efs, &taus(a, b), &mut rs, &ctx).unwrap().expect("segue");
                assert!(enquadramento::animado(&janela), "janela [{a}, {b}) sem o caminho animado");
                assert_eq!(zoom_maximo(&janela), zoom_maximo(&inteiro), "janela [{a}, {b}): outra ampliação máxima");
                for j in a..b {
                    let (x, y) = (geometria::regiao_em(&inteiro, t.tempo(j)), geometria::regiao_em(&janela, t.tempo(j)));
                    assert_eq!(x, y, "janela [{a}, {b}) no quadro {j} (s = {s})");
                }
            }
            // com `cobrir` o destino não varia (a região fica dentro da imagem): a regra que recusa não dispara
            let tempos = taus(0, 300);
            assert!(!enquadramento::destino_variavel(&inteiro, TamanhoPx { w: 320, h: 180 }, TamanhoPx { w: 320, h: 180 }, &tempos));
            let canvas = crate::edicao::motor::plano::Canvas { w: 320, h: 180, fps: crate::edicao::modelo::Racional { num: 30, den: 1 }, cor_fundo: "#000000".into() };
            let f = enquadramento::filtros(&inteiro, TamanhoPx { w: 320, h: 180 }, &canvas, &tempos, 0, 0).unwrap();
            assert!(f.contains("perspective"), "{f}");
        }
        // `conter` de tamanho fixo também não varia
        let p = projeto(320, 180, (30, 1), midias.clone(), vec![trecho("c1", "m1", &t, 0, 300, 1.0, json!([enq_segue(json!({ "x": 0.1, "y": 0.1, "w": 0.4, "h": 0.4 }), 1.0, "conter")]))], json!({ "rastreios": [r1_andando(&t, 300)] }));
        let mut rs = Rastreios::novo(&p, &ctx);
        let tempos: Vec<f64> = (0..300).map(|j| t.tempo(j)).collect();
        let e = na_cauda(&efeitos_do(&p), &tempos, &mut rs, &ctx).unwrap().unwrap();
        assert!(!enquadramento::destino_variavel(&e, TamanhoPx { w: 320, h: 180 }, TamanhoPx { w: 320, h: 180 }, &tempos));
    }

    /// O centro do rastreio (espaço de origem) vai ao espaço girado, onde mora a região.
    #[test]
    fn giro_leva_o_centro_ao_espaco_girado() {
        let (mundo, midias) = aceite::mundo_das_recusas();
        let t = mundo.fontes["m1"].quadros.clone();
        let ctx = Contexto { fontes: &mundo.fontes, cap: &mundo.cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
        let parado = rastreio("r1", Some("m1"), "fonte", vec![ancora(t.tempo(0), Some(t.pts[0]), [0.15, 0.25, 0.1, 0.1])]);
        let girar = efeito("girar", "g1", json!({ "graus": 90, "espelhar_h": false, "espelhar_v": false }));
        let p = projeto(320, 180, (30, 1), midias, vec![trecho("c1", "m1", &t, 0, 30, 1.0, json!([girar, enq_segue(json!({ "x": 0.0, "y": 0.0, "w": 0.2, "h": 0.2 }), 0.0, "cobrir")]))], json!({ "rastreios": [parado] }));
        let mut rs = Rastreios::novo(&p, &ctx);
        let e = na_cauda(&efeitos_do(&p), &[t.tempo(10)], &mut rs, &ctx).unwrap().unwrap();
        // (x, y) → (1 − y, x): centro (0,2; 0,3) vai a (0,7; 0,2)
        let r = geometria::regiao_em(&e, t.tempo(10));
        assert!((r.x - 0.6).abs() < 1e-12 && (r.y - 0.1).abs() < 1e-12, "{r:?}");
    }

    /// Projeto antigo (sem `segue`) não passa pela troca; desligado também não.
    #[test]
    fn sem_segue_nada_muda() {
        let (mundo, midias) = aceite::mundo_das_recusas();
        let t = mundo.fontes["m1"].quadros.clone();
        let ctx = Contexto { fontes: &mundo.fontes, cap: &mundo.cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
        let sem = efeito("enquadramento", "q9", json!({ "regiao": { "x": 0.3, "y": 0.3, "w": 0.4, "h": 0.4 }, "ajuste": "cobrir", "interpolacao": "bilinear" }));
        let mut desligado = enq_segue(json!({ "x": 0.3, "y": 0.3, "w": 0.4, "h": 0.4 }), 1.0, "cobrir");
        desligado["ligado"] = json!(false);
        for e in [sem, desligado] {
            let p = projeto(320, 180, (30, 1), midias.clone(), vec![trecho("c1", "m1", &t, 0, 30, 1.0, json!([e]))], json!({ "rastreios": [r1_andando(&t, 300)] }));
            let mut rs = Rastreios::novo(&p, &ctx);
            assert!(na_cauda(&efeitos_do(&p), &[0.0, 0.1], &mut rs, &ctx).unwrap().is_none());
        }
    }

    /// A proveniência do processamento `enquadramento` leva o `segue` e o rastreio entra em `feito.rastreios`.
    #[test]
    fn proveniencia_diz_o_rastreio() {
        let (mundo, midias) = aceite::mundo_das_recusas();
        let t = mundo.fontes["m1"].quadros.clone();
        let montar = |e: Value| projeto(320, 180, (30, 1), midias.clone(), vec![trecho("c1", "m1", &t, 0, 60, 1.0, json!([e]))], json!({ "rastreios": [r1_andando(&t, 300)] }));
        let plano = mundo.compilar(&montar(enq_segue(json!({ "x": 0.3, "y": 0.3, "w": 0.4, "h": 0.4 }), 1.5, "cobrir")), &alvo_final("cpu")).unwrap();
        let pr = &plano.proveniencia;
        assert_eq!(pr.rastreios.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["r1"]);
        let q = pr.roteiro[0].processamentos.iter().find(|x| x.nome == "enquadramento").expect("processamento");
        assert_eq!(q.parametros["segue"], json!({ "rastreio": "r1", "suavizar_s": 1.5 }));
        assert!(plano.blocos[0].grafo.contains("perspective"), "o zoom que segue anda");
        // sem acompanhar, o rastreio não é usado
        let fixo = efeito("enquadramento", "q9", json!({ "regiao": { "x": 0.3, "y": 0.3, "w": 0.4, "h": 0.4 }, "ajuste": "cobrir", "interpolacao": "bilinear" }));
        let plano = mundo.compilar(&montar(fixo), &alvo_final("cpu")).unwrap();
        assert!(plano.proveniencia.rastreios.is_empty());
        assert!(!plano.blocos[0].grafo.contains("perspective"));
    }

    fn problemas_de(p: &ProjetoEdicao, mundo: &Mundo) -> Vec<crate::edicao::modelo::Problema> {
        let ctx = Contexto { fontes: &mundo.fontes, cap: &mundo.cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
        let linha = tempo::resolver(p, &ctx);
        validar::validar(p, &linha, &ctx, Some(&alvo_final("cpu")))
    }

    /// `suavizar_s` fora de 0..10 e enquadramento que segue fora do trecho: `item_invalido` no campo `segue`; o
    /// rastreio automático sem confirmação passa por `validar::confirmado` (regra da leva 1).
    #[test]
    fn problemas_do_parametro_e_do_rastreio() {
        let (mundo, midias) = aceite::mundo_das_recusas();
        let t = mundo.fontes["m1"].quadros.clone();
        let no_trecho = |e: Value, r: Value| projeto(320, 180, (30, 1), midias.clone(), vec![trecho("c1", "m1", &t, 0, 60, 1.0, json!([e]))], json!({ "rastreios": [r] }));
        let do_segue = |ps: &[crate::edicao::modelo::Problema]| -> Vec<(String, String)> {
            ps.iter().filter(|x| x.efeito.as_deref() == Some("q9") && x.nivel == Nivel::Erro).map(|x| (x.codigo.to_string(), x.campo.clone().unwrap_or_default())).collect()
        };
        let regiao = json!({ "x": 0.3, "y": 0.3, "w": 0.4, "h": 0.4 });
        assert_eq!(do_segue(&problemas_de(&no_trecho(enq_segue(regiao.clone(), 1.0, "cobrir"), r1_andando(&t, 300)), &mundo)), Vec::<(String, String)>::new());
        assert_eq!(do_segue(&problemas_de(&no_trecho(enq_segue(regiao.clone(), 0.0, "cobrir"), r1_andando(&t, 300)), &mundo)), Vec::<(String, String)>::new());
        assert_eq!(do_segue(&problemas_de(&no_trecho(enq_segue(regiao.clone(), 10.0, "cobrir"), r1_andando(&t, 300)), &mundo)), Vec::<(String, String)>::new());
        for s in [10.5, -0.1] {
            assert_eq!(do_segue(&problemas_de(&no_trecho(enq_segue(regiao.clone(), s, "cobrir"), r1_andando(&t, 300)), &mundo)), [("item_invalido".to_string(), "segue".to_string())], "suavizar {s}");
        }
        // rastreio que não existe
        assert_eq!(do_segue(&problemas_de(&no_trecho(enq_segue(regiao.clone(), 1.0, "cobrir"), rastreio("r2", Some("m1"), "fonte", vec![])), &mundo)), [("rastreio_ausente".to_string(), "rastreio".to_string())]);
        // detecção automática sem confirmação; confirmada cobrindo o uso, passa
        let mut auto = r1_andando(&t, 300);
        auto["origem"] = json!({ "tipo": "deteccao", "classe": "pessoa" });
        assert_eq!(do_segue(&problemas_de(&no_trecho(enq_segue(regiao.clone(), 1.0, "cobrir"), auto.clone()), &mundo)), [("rastreio_nao_confirmado".to_string(), "rastreio".to_string())]);
        auto["confirmacao"] = json!({ "por": "Perito", "em": "2026-11-17T00:00:00Z", "denso_sha256": null, "de_s": 0.0, "ate_s": 9.0 });
        assert_eq!(do_segue(&problemas_de(&no_trecho(enq_segue(regiao.clone(), 1.0, "cobrir"), auto), &mundo)), Vec::<(String, String)>::new());
        // no congelado: nada a acompanhar no tempo
        let cong = json!({ "tipo": "congelar", "id": "c3", "rotulo": "", "midia": "m1", "instante_s": t.tempo(30), "instante_pts": t.pts[30], "duracao_s": 1.0,
                           "efeitos": [enq_segue(regiao.clone(), 1.0, "cobrir")] });
        let p = projeto(320, 180, (30, 1), midias.clone(), vec![cong], json!({ "rastreios": [r1_andando(&t, 300)] }));
        assert!(do_segue(&problemas_de(&p, &mundo)).contains(&("item_invalido".to_string(), "segue".to_string())));
        // na imagem (sem fonte)
        let img = json!({ "tipo": "imagem", "id": "c4", "rotulo": "", "midia": "foto", "duracao_s": 1.0, "efeitos": [enq_segue(regiao, 1.0, "cobrir")] });
        let p = projeto(320, 180, (30, 1), midias.clone(), vec![img], json!({ "rastreios": [rastreio("r1", None, "local", vec![ancora(0.0, None, [0.1, 0.1, 0.2, 0.2])])] }));
        assert!(do_segue(&problemas_de(&p, &mundo)).contains(&("item_invalido".to_string(), "segue".to_string())));
    }

    // ------------------------------------------------------------------ no render (os dois ffmpeg)

    fn quadros_709(cap: &Capacidades, video: &Path, w: u32, h: u32) -> Vec<QuadroRgb> {
        let a: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-i", &video.to_string_lossy(), "-vf",
            "scale=flags=bicubic+accurate_rnd+bitexact+full_chroma_int:in_color_matrix=bt709:in_range=tv:out_range=pc,format=rgb24", "-f", "rawvideo", "-"].map(String::from).into();
        let pasta = video.parent().unwrap().to_path_buf();
        let cmd = crate::edicao::ffmpeg::exec::Comando { nome: "rgb709".into(), programa: cap.binario.clone(), args: a, pasta: pasta.clone(), ambiente: Vec::new(), sem_ambiente: Vec::new(), stderr: pasta.join("rgb709.stderr") };
        let (fim, out) = crate::edicao::ffmpeg::exec::rodar_com_saida(&cmd, &std::sync::atomic::AtomicBool::new(false)).expect("ffmpeg");
        crate::edicao::ffmpeg::exec::exigir_sucesso(&cmd, &fim).expect("decodificar");
        out.chunks_exact((w * h * 3) as usize).map(|px| QuadroRgb { w, h, px: px.to_vec() }).collect()
    }

    /// Centro (px) dos pixels vermelhos fortes (o quadrado da matriz, ampliado).
    fn centro_vermelho(q: &QuadroRgb) -> Option<(f64, f64)> {
        let (mut sx, mut sy, mut n) = (0.0, 0.0, 0u64);
        for (i, p) in q.px.chunks_exact(3).enumerate() {
            if p[0] as i32 - p[1] as i32 > 90 && p[0] as i32 - p[2] as i32 > 90 {
                sx += (i as u32 % q.w) as f64 + 0.5;
                sy += (i as u32 / q.w) as f64 + 0.5;
                n += 1;
            }
        }
        (n > 20).then(|| (sx / n as f64, sy / n as f64))
    }

    /// Previsão independente do motor: média dos centros da marca nos quadros com tempo em [t − s/2, t + s/2],
    /// tamanho pela região gravada em t, deslocada para dentro.
    fn previsto(m: &apoio::Medida, j: usize, s: f64, tamanho: &dyn Fn(f64) -> (f64, f64)) -> (f64, f64, Caixa) {
        let tab = &m.info.quadros;
        let centro = |i: usize| {
            let c = fontes::marca_esperada(Fonte::Mp4, m.numeros[i]);
            (c[0] + c[2] / 2.0, c[1] + c[3] / 2.0)
        };
        let t = tab.tempo(j);
        let na_janela: Vec<usize> = (0..tab.pts.len()).filter(|&i| tab.tempo(i) >= t - s / 2.0 && tab.tempo(i) <= t + s / 2.0).collect();
        let (mx, my) = na_janela.iter().fold((0.0, 0.0), |(a, b), &i| (a + centro(i).0, b + centro(i).1));
        let (mx, my) = (mx / na_janela.len() as f64, my / na_janela.len() as f64);
        let (w, h) = tamanho(t);
        let r = Caixa { x: (mx - w / 2.0).clamp(0.0, 1.0 - w), y: (my - h / 2.0).clamp(0.0, 1.0 - h), w, h };
        let (cx, cy) = centro(j);
        ((cx - r.x) / r.w * 320.0, (cy - r.y) / r.h * 180.0, r)
    }

    fn renderizar(cap: &Arc<Capacidades>, plano: &Plano, nome: &str) -> Vec<QuadroRgb> {
        let pasta = fontes::pasta_temporaria(nome);
        let r = executor::rodar(cap, plano, &pasta, &Opcoes { sem_perdas: true, ..Opcoes::default() }).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
        let q: Vec<QuadroRgb> = r.blocos.iter().flat_map(|b| quadros_709(cap, b, 320, 180)).collect();
        assert_eq!(q.len() as u64, plano.total_q, "{nome} ({}): quadros × slots", cap.versao);
        q
    }

    /// §(f) ZS: a marca sintética que anda (vai e volta a cada 4 s) é acompanhada; o centro dela no vídeo fica a até
    /// 2 px do previsto pela suavização (média de 1 s), com a região deslizando na borda; o tamanho anda pelas chaves
    /// (aproximar, segurar, afastar) sem dividir o trecho; o quadro final de slots soltos é o mesmo da exportação.
    #[test]
    fn segue_no_render() {
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let tab = &m.info.quadros;
        let (j_in, j_out) = (60usize, 180usize);
        let chaves = json!([
            { "t": tab.tempo(60), "v": { "x": 0.2, "y": 0.2, "w": 0.6, "h": 0.6 }, "interp": "linear" },
            { "t": tab.tempo(100), "v": { "x": 0.25, "y": 0.25, "w": 0.4, "h": 0.4 }, "interp": "linear" },
            { "t": tab.tempo(140), "v": { "x": 0.25, "y": 0.25, "w": 0.4, "h": 0.4 }, "interp": "linear" },
            { "t": tab.tempo(170), "v": { "x": 0.2, "y": 0.2, "w": 0.6, "h": 0.6 }, "interp": "linear" }
        ]);
        let tamanho = |t: f64| {
            let a: Animavel<Caixa> = serde_json::from_value(chaves.clone()).unwrap();
            let c = curva::avaliar(&a, t);
            (c.w, c.h)
        };
        let montar = |segue: bool| {
            let mut e = efeito("enquadramento", "q9", json!({ "regiao": chaves.clone(), "ajuste": "cobrir", "interpolacao": "bilinear" }));
            if segue {
                e["segue"] = json!({ "rastreio": "r1", "suavizar_s": 1.0 });
            }
            projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, j_in, j_out, 1.0, json!([e]))],
                    json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(Fonte::Mp4, &m, 0, 240))] }))
        };
        let p = montar(true);
        let mundo = Mundo::novo(&cap).com("m1", &m.info);
        let plano = mundo.compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{e}"));
        let quadros = renderizar(&cap, &plano, "zs-segue");
        let (mut pior, mut na_borda) = (0.0f64, 0);
        for (k, q) in quadros.iter().enumerate() {
            let j = j_in + k;
            let (ex, ey, r) = previsto(&m, j, 1.0, &tamanho);
            let (mx, my) = centro_vermelho(q).unwrap_or_else(|| panic!("slot {k}: marca fora do quadro ({})", cap.versao));
            let d = (mx - ex).abs().max((my - ey).abs());
            assert!(d <= 2.0, "slot {k} ({}): marca em ({mx:.2}, {my:.2}), prevista ({ex:.2}, {ey:.2}), região {r:?}", cap.versao);
            pior = pior.max(d);
            na_borda += usize::from(r.x == 0.0 || r.x + r.w >= 1.0 - 1e-12);
        }
        assert!(na_borda > 5, "o ensaio não passou pela borda: {na_borda}");
        // sem acompanhar, a marca sai do lugar previsto (o teste separa os dois)
        let parado = renderizar(&cap, &mundo.compilar(&montar(false), &alvo_final("cpu")).unwrap(), "zs-parado");
        let longe = parado
            .iter()
            .enumerate()
            .filter(|(k, q)| {
                let (ex, ey, _) = previsto(&m, j_in + k, 1.0, &tamanho);
                centro_vermelho(q).is_none_or(|(mx, my)| (mx - ex).abs().max((my - ey).abs()) > 10.0)
            })
            .count();
        assert!(longe > 30, "sem o segue a marca ficaria no lugar: {longe}");
        // quadro final de slots soltos (janela de outro tamanho) = exportação
        for q in [3u64, 17, 47, 90, 119] {
            let pq = mundo.compilar(&p, &Alvo::Quadro { q, largura: 320 }).unwrap();
            let rq = executor::rodar(&cap, &pq, &fontes::pasta_temporaria("zs-quadro"), &Opcoes::default()).unwrap_or_else(|e| panic!("quadro {q}: {e}"));
            let bmp = image::open(&rq.blocos[0]).expect("bmp").to_rgb8();
            let lido = QuadroRgb { w: bmp.width(), h: bmp.height(), px: bmp.into_raw() };
            assert_eq!(medir::pixels_diferentes(&quadros[q as usize], &lido, 1), 0, "quadro final do slot {q} ≠ exportação ({})", cap.versao);
        }
        // prévia de uma janela no meio: os mesmos quadros
        let pv = mundo.compilar(&p, &Alvo::Previa { altura: 180, de_q: 41, ate_q: 77 }).unwrap();
        let rv = executor::rodar(&cap, &pv, &fontes::pasta_temporaria("zs-previa"), &Opcoes { sem_perdas: true, ..Opcoes::default() }).unwrap();
        let qv = quadros_709(&cap, &rv.blocos[0], 320, 180);
        assert_eq!(qv.len(), 36);
        for (i, x) in qv.iter().enumerate() {
            assert_eq!(medir::pixels_diferentes(&quadros[41 + i], x, 1), 0, "prévia: slot {} ({})", 41 + i, cap.versao);
        }
        eprintln!("zoom que segue ({}): {} slots, pior desvio {pior:.2} px, {na_borda} na borda", cap.versao, quadros.len());
    }
}
