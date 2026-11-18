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
/// Folga (s) nas pontas da janela da média: com a ponta caindo num quadro (s × fps par), o arredondamento de
/// `t ± s/2` punha e tirava esse quadro de um slot para o outro e o zoom tremia. Espelho: `FOLGA_S` em segue.ts.
const FOLGA_S: f64 = 1e-9;

/// Slots de um bloco do clipe que acompanha (`motor/blocos.rs::dividir`): o `perspective` relê (analisa de novo) a
/// tabela da região a cada quadro, e o custo cresce com os slots do bloco. Medido em `desempenho_trecho_longo` (2 min
/// a 60 qps, 720p): num bloco só, 3,3× (9.0.1) e 5,6× (7.0.2) o tempo do zoom com chaves; em blocos de 300 slots,
/// 0,75× a 0,84×, com o mesmo composto quadro a quadro.
pub(crate) const BLOCO_Q: u64 = 300;

/// O clipe `id` da principal tem o enquadramento que vale (o primeiro ligado) acompanhando um rastreio?
pub(crate) fn clipe_segue(p: &ProjetoEdicao, id: &str) -> bool {
    p.principal.iter().any(|c| match c {
        Tolerante::Conhecido(ClipePrincipal::Trecho(t)) if t.base.id == id => {
            t.base.efeitos.iter().find_map(|e| match e {
                Tolerante::Conhecido(Efeito::Enquadramento(q)) if q.base.ligado => Some(q.segue.is_some()),
                _ => None,
            }) == Some(true)
        }
        _ => false,
    })
}

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

/// Para cada amostra, o índice da última com centro até ela e o da primeira com centro a partir dela.
struct Vizinhos {
    antes: Vec<Option<usize>>,
    depois: Vec<Option<usize>>,
}

impl Vizinhos {
    fn de(amostras: &[AmostraSegue]) -> Self {
        let n = amostras.len();
        let (mut antes, mut depois) = (vec![None; n], vec![None; n]);
        let mut u = None;
        for (i, a) in amostras.iter().enumerate() {
            if a.centro.is_some() {
                u = Some(i);
            }
            antes[i] = u;
        }
        u = None;
        for i in (0..n).rev() {
            if amostras[i].centro.is_some() {
                u = Some(i);
            }
            depois[i] = u;
        }
        Self { antes, depois }
    }

    /// O centro com amostra mais próxima de `t` entre a anterior `a` e a seguinte `d` (empate: a anterior).
    fn mais_proxima(amostras: &[AmostraSegue], a: Option<usize>, d: Option<usize>, t: f64) -> Option<Ponto> {
        match (a, d) {
            (Some(a), Some(d)) => amostras[if t - amostras[a].t <= amostras[d].t - t { a } else { d }].centro,
            (Some(i), None) | (None, Some(i)) => amostras[i].centro,
            (None, None) => None,
        }
    }
}

/// Centro em `t`: média, na ordem das amostras, de TODAS as da janela [t − s/2, t + s/2]; a que não tem centro vale
/// o da amostra com centro mais próxima no tempo (empate: a anterior), e assim uma lacuna do rastreio é atravessada
/// em `s` segundos em vez de saltar num quadro (com s = 0, corte). Janela vazia: a mais próxima de `t`. Nenhum
/// centro em lugar nenhum: None. `amostras` em ordem de `t`.
fn centro_em(amostras: &[AmostraSegue], viz: &Vizinhos, t: f64, s: f64) -> Option<Ponto> {
    let (de, ate) = (t - s / 2.0 - FOLGA_S, t + s / 2.0 + FOLGA_S);
    let lo = amostras.partition_point(|a| a.t < de);
    let hi = amostras.partition_point(|a| a.t <= ate).max(lo);
    if hi == lo {
        return Vizinhos::mais_proxima(amostras, lo.checked_sub(1).and_then(|i| viz.antes[i]), viz.depois.get(hi).copied().flatten(), t);
    }
    let (mut sx, mut sy) = (0.0, 0.0);
    for i in lo..hi {
        let c = match amostras[i].centro {
            Some(c) => c,
            None => Vizinhos::mais_proxima(amostras, viz.antes[i], viz.depois[i], amostras[i].t)?,
        };
        sx += c.x;
        sy += c.y;
    }
    let n = (hi - lo) as f64;
    Some(Ponto { x: sx / n, y: sy / n })
}

/// Região (0..1 da origem já girada) em cada instante de `em` (base do clipe): centro = média das `amostras` em
/// [t − suavizar_s/2, t + suavizar_s/2], cada uma sem centro valendo a mais próxima com centro (janela vazia: a mais
/// próxima; nenhum centro: o centro de `regiao`), tamanho = `regiao` em t, deslocada para dentro de [0, 1]² (lado de
/// 1 ou mais fica em 0).
pub fn regiao_seguida(e: &EfEnquadramento, amostras: &[AmostraSegue], em: &[f64]) -> Vec<Caixa> {
    let s = suavizar(e);
    let viz = Vizinhos::de(amostras);
    em.iter()
        .map(|&t| {
            let r = gravada(e, t);
            let c = centro_em(amostras, &viz, t, s).unwrap_or(Ponto { x: r.x + r.w / 2.0, y: r.y + r.h / 2.0 });
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
/// quadros de cada janela e, na janela com quadro sem centro, o quadro com centro mais próximo de cada lado dela.
fn amostras(r: &Rastreio, tab: &TabelaQuadros, giro: Afim, instantes: &[f64], s: f64, rs: &mut Rastreios<'_>) -> Result<Vec<AmostraSegue>> {
    let n = tab.pts.len();
    // a mesma conta de `TabelaQuadros::tempo`
    let tempo = |p: i64| p as f64 * tab.tb.num as f64 / tab.tb.den as f64;
    let janelas: Vec<(usize, usize)> = instantes
        .iter()
        .map(|&t| {
            let lo = tab.pts.partition_point(|&p| tempo(p) < t - s / 2.0 - FOLGA_S);
            (lo, tab.pts.partition_point(|&p| tempo(p) <= t + s / 2.0 + FOLGA_S).max(lo))
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
    // janela com quadro sem centro (ou vazia): o com centro mais próximo de cada lado dela entra (é o que vale no lugar)
    let vazias: Vec<(usize, usize)> = janelas.iter().copied().filter(|&(lo, hi)| hi == lo || (lo..hi).any(|j| c.feitos.get(&j).copied().flatten().is_none())).collect();
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
        let mut lista: Vec<Value> = casos
            .into_iter()
            .map(|(nome, e, a)| {
                json!({ "nome": nome, "entrada": { "efeito": serde_json::to_value(&e).unwrap(), "amostras": a, "em": instantes },
                        "saida": regiao_seguida(&e, &a, &instantes) })
            })
            .collect();
        // lacuna de 5 s (rastreio seguido que perdeu a pessoa): com média atravessa em `s`; sem média, corte no meio
        let lacuna: Vec<AmostraSegue> = (0..360).map(|j| am(j as f64 / 30.0, (!(120..270).contains(&j)).then(|| (0.15 + j as f64 * 0.002, 0.4 + j as f64 * 0.0005)))).collect();
        let na_lacuna: Vec<f64> = (100..300).step_by(3).map(|j| j as f64 / 30.0).collect();
        let tarde: Vec<AmostraSegue> = (0..120).map(|j| am(j as f64 / 30.0, (j >= 60).then(|| (0.3 + j as f64 * 0.003, 0.5)))).collect();
        let no_comeco: Vec<f64> = (0..90).step_by(2).map(|j| j as f64 / 30.0).collect();
        for (nome, e, a, em) in [
            ("lacuna de 5 s, média de 1 s: atravessa em 1 s", com(fixa.clone(), 1.0), &lacuna, &na_lacuna),
            ("lacuna de 5 s, média de 3 s", com(fixa.clone(), 3.0), &lacuna, &na_lacuna),
            ("lacuna de 5 s, sem média: corte no meio", com(fixa.clone(), 0.0), &lacuna, &na_lacuna),
            ("rastreio que começa depois, média de 1 s", com(fixa.clone(), 1.0), &tarde, &no_comeco),
        ] {
            lista.push(json!({ "nome": nome, "entrada": { "efeito": serde_json::to_value(&e).unwrap(), "amostras": a, "em": em }, "saida": regiao_seguida(&e, a, em) }));
        }
        // ponta da janela caindo em quadro (tempos de pts 512·j em 1/15360): o arredondamento não pode tirar o quadro
        let (pts_t, e) = (|j: usize| (512 * j) as f64 / 15360.0, com(fixa.clone(), 3.0));
        let a: Vec<AmostraSegue> = (40..176).map(|j| am(pts_t(j), Some((0.2 + j as f64 * 0.002, 0.5)))).collect();
        let em: Vec<f64> = (100..112).map(pts_t).collect();
        lista.push(json!({ "nome": "ponta da janela num quadro (3 s a 30 qps)", "entrada": { "efeito": serde_json::to_value(&e).unwrap(), "amostras": a, "em": em },
                           "saida": regiao_seguida(&e, &a, &em) }));
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

    /// Alvo em velocidade constante: o centro suavizado anda em passos iguais. Com a ponta da janela caindo em quadro
    /// (s × fps par) e sem folga, o arredondamento de `t ± s/2` punha e tirava o quadro da ponta e o centro tremia
    /// (revisão ZS: até 5 px por quadro a 2×).
    #[test]
    fn ponta_da_janela_nao_faz_tremer() {
        for (num, den, fps) in [(1i64, 15360i64, 30.0), (1, 90000, 30.0), (1001, 60000, 60000.0 / 1001.0), (1, 1000, 30.0)] {
            let pts = |j: usize| (j as f64 / fps * den as f64 / num as f64).round() as i64;
            let t = |j: usize| pts(j) as f64 * num as f64 / den as f64;
            let a: Vec<AmostraSegue> = (0..900).map(|j| am(t(j), Some((0.2 + j as f64 * 0.0005, 0.5)))).collect();
            for s in [0.5, 1.0, 2.0, 3.0, 4.0] {
                let em: Vec<f64> = (300..600).map(t).collect();
                let xs: Vec<f64> = regiao_seguida(&com(json!({ "x": 0.0, "y": 0.0, "w": 0.5, "h": 0.5 }), s), &a, &em).iter().map(|r| r.x).collect();
                let d2 = xs.windows(3).map(|p| (p[2] - 2.0 * p[1] + p[0]).abs()).fold(0.0, f64::max);
                // MKV 1/1000: pts arredondado ao ms, o passo do alvo varia um pouco; o resto é exato
                let teto = if den == 1000 { 1e-4 } else { 1e-12 };
                assert!(d2 <= teto, "base {num}/{den}, suavizar {s}: aceleração {d2:e}");
            }
        }
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
        // rastreio seguido com lacuna de 4,3 s (âncoras longe: nada no meio): a janela que pega a borda da lacuna
        // precisa do quadro com centro de cada lado dela, mesmo fora da janela do bloco
        let mut com_lacuna = r1_andando(&t, 300);
        com_lacuna["origem"] = json!({ "tipo": "assistido" });
        let ks: Vec<Value> = com_lacuna["ancoras"].as_array().unwrap().iter().filter(|k| !(3.0..7.4).contains(&k["t"].as_f64().unwrap())).cloned().collect();
        com_lacuna["ancoras"] = Value::Array(ks);
        let fixa = || json!({ "x": 0.3, "y": 0.3, "w": 0.4, "h": 0.4 });
        for (regiao, s, r) in [(fixa(), 1.0, r1_andando(&t, 300)), (chaves, 0.5, r1_andando(&t, 300)), (fixa(), 0.0, r1_andando(&t, 300)), (fixa(), 1.0, com_lacuna.clone()), (fixa(), 3.0, com_lacuna)] {
            let p = projeto(320, 180, (30, 1), midias.clone(), vec![trecho("c1", "m1", &t, 0, 300, 1.0, json!([enq_segue(regiao.clone(), s, "cobrir")]))], json!({ "rastreios": [r] }));
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
            for (a, b) in [(0usize, 16usize), (37, 38), (100, 116), (5, 299), (288, 300), (250, 251), (80, 96), (215, 231), (219, 220), (95, 96)] {
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
        // janela fechada: o quadro na ponta conta (sem folga, o arredondamento o punha e tirava)
        let na_janela: Vec<usize> = (0..tab.pts.len()).filter(|&i| (tab.tempo(i) - t).abs() <= s / 2.0 + 1e-9).collect();
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

    // ------------------------------------------------------------------ revisão: o caso do dono e as bordas (os dois ffmpeg)

    /// Fundo verde puro: qualquer pixel dele no quadro é barra ou borda, não imagem (as fontes são cinza e vermelho).
    const FUNDO_VERDE: &str = "#00ff00";

    fn verde(q: &QuadroRgb) -> u64 {
        q.px.chunks_exact(3).filter(|p| p[1] as i32 - p[0] as i32 > 150 && p[1] as i32 - p[2] as i32 > 150).count() as u64
    }

    /// Caixa [x0, y0, x1, y1] (px, inclusiva) dos pixels de `cor` dentro de `area`.
    fn caixa_de(q: &QuadroRgb, area: [u32; 4], cor: &dyn Fn(&[u8]) -> bool) -> Option<[u32; 4]> {
        let mut c: Option<[u32; 4]> = None;
        for y in area[1]..area[3].min(q.h) {
            for x in area[0]..area[2].min(q.w) {
                let i = ((y * q.w + x) * 3) as usize;
                if cor(&q.px[i..i + 3]) {
                    c = Some(match c {
                        None => [x, y, x, y],
                        Some(b) => [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)],
                    });
                }
            }
        }
        c
    }

    fn eh_vermelho(p: &[u8]) -> bool {
        p[0] as i32 - p[1] as i32 > 90 && p[0] as i32 - p[2] as i32 > 90
    }

    fn eh_azul(p: &[u8]) -> bool {
        p[2] as i32 - p[0] as i32 > 90 && p[2] as i32 - p[1] as i32 > 90
    }

    /// Centro (px) dos pixels vermelhos dentro de `area`.
    fn centro_vermelho_em(q: &QuadroRgb, area: [u32; 4]) -> Option<(f64, f64)> {
        let (mut sx, mut sy, mut n) = (0.0, 0.0, 0u64);
        for y in area[1]..area[3].min(q.h) {
            for x in area[0]..area[2].min(q.w) {
                let i = ((y * q.w + x) * 3) as usize;
                if eh_vermelho(&q.px[i..i + 3]) {
                    sx += x as f64 + 0.5;
                    sy += y as f64 + 0.5;
                    n += 1;
                }
            }
        }
        (n > 20).then(|| (sx / n as f64, sy / n as f64))
    }

    /// Caso do dono (PINHO, 17/11: dividiu o trecho para aproximar e afastar), agora num trecho só: cena inteira →
    /// aproxima → segura → afasta à cena inteira, nos tempos dele em escala, com a região dele (no alto, 5,3×) e com
    /// uma do meio (onde a marca passa). Sai nos dois ffmpeg sem erro nem problema; nenhum quadro com barra ou
    /// borda; a marca cai, em todo slot em que aparece inteira, onde a interpolação linear das chaves (refeita aqui)
    /// manda: centro a até 1,5 px e lado a até 2 px, sem salto entre chaves.
    #[test]
    fn aproxima_segura_afasta_num_trecho_so() {
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let tab = &m.info.quadros;
        let mundo = Mundo::novo(&cap).com("m1", &m.info);
        let inteira = [0.0, 0.0, 1.0, 1.0];
        let js = [30usize, 120, 210, 330];
        for (nome, r) in [("dono", [0.17837, 0.0, 0.18853, 0.18853]), ("meio", [0.4, 0.5, 0.4, 0.4])] {
            let vs = [inteira, r, r, inteira];
            let chaves: Vec<Value> = js.iter().zip(&vs).map(|(&j, v)| json!({ "t": tab.tempo(j), "pts": tab.pts[j], "v": { "x": v[0], "y": v[1], "w": v[2], "h": v[3] } })).collect();
            let e = efeito("enquadramento", "q1", json!({ "regiao": chaves, "ajuste": "cobrir", "interpolacao": "bilinear" }));
            let mut p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, 0, 450, 1.0, json!([e]))], json!({}));
            p.saida.cor_fundo = FUNDO_VERDE.into();
            let erros: Vec<String> = problemas_de(&p, &mundo).into_iter().filter(|x| x.nivel == Nivel::Erro).map(|x| format!("{} {:?}", x.codigo, x.campo)).collect();
            assert!(erros.is_empty(), "{nome}: {erros:?}");
            let plano = mundo.compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
            let quadros = renderizar(&cap, &plano, &format!("zs-dono-{nome}"));
            let regiao = |t: f64| -> [f64; 4] {
                let ts: Vec<f64> = js.iter().map(|&j| tab.tempo(j)).collect();
                if t <= ts[0] {
                    return vs[0];
                }
                for i in 0..3 {
                    if t <= ts[i + 1] {
                        let u = (t - ts[i]) / (ts[i + 1] - ts[i]);
                        return [0, 1, 2, 3].map(|k| vs[i][k] + (vs[i + 1][k] - vs[i][k]) * u);
                    }
                }
                vs[3]
            };
            let (mut medidos, mut pior, mut pior_lado) = (0, 0.0f64, 0.0f64);
            for (k, q) in quadros.iter().enumerate() {
                assert_eq!(verde(q), 0, "{nome} slot {k} ({}): barra ou borda", cap.versao);
                let r = regiao(tab.tempo(k));
                let c = fontes::marca_esperada(Fonte::Mp4, m.numeros[k]);
                let (x0, x1) = ((c[0] - r[0]) / r[2] * 320.0, (c[0] + c[2] - r[0]) / r[2] * 320.0);
                let (y0, y1) = ((c[1] - r[1]) / r[3] * 180.0, (c[1] + c[3] - r[1]) / r[3] * 180.0);
                if x0 < 3.0 || y0 < 3.0 || x1 > 317.0 || y1 > 177.0 {
                    continue;
                }
                let b = caixa_de(q, [0, 0, 320, 180], &eh_vermelho).unwrap_or_else(|| panic!("{nome} slot {k}: sem a marca"));
                let (mx, my) = centro_vermelho(q).unwrap();
                let d = (mx - (x0 + x1) / 2.0).abs().max((my - (y0 + y1) / 2.0).abs());
                let dl = ((b[2] + 1 - b[0]) as f64 - (x1 - x0)).abs().max(((b[3] + 1 - b[1]) as f64 - (y1 - y0)).abs());
                assert!(d <= 1.5 && dl <= 3.0, "{nome} slot {k} ({}): marca em ({mx:.2}, {my:.2}) lado {:?}, prevista ({:.2}, {:.2}) lado ({:.2}, {:.2})", cap.versao, b, (x0 + x1) / 2.0, (y0 + y1) / 2.0, x1 - x0, y1 - y0);
                pior = pior.max(d);
                pior_lado = pior_lado.max(dl);
                medidos += 1;
            }
            if nome == "meio" {
                assert!(medidos > 150, "{nome}: poucos slots com a marca inteira: {medidos}");
            }
            eprintln!("dono/{nome} ({}): {} slots sem borda, {medidos} medidos, pior centro {pior:.2} px, pior lado {pior_lado:.2} px", cap.versao, quadros.len());
        }
    }

    /// Regra D9 (com a emenda da rodada 2) refeita aqui, sem o código do motor: média de TODOS os quadros da mídia com
    /// tempo em [t − s/2, t + s/2] (quadro na ponta conta), o sem centro valendo o do quadro com centro mais próximo no
    /// tempo (empate: o anterior); sem quadro na janela, o mais próximo de t.
    fn centro_pela_regra(tab: &TabelaQuadros, centros: &[Option<(f64, f64)>], t: f64, s: f64) -> Option<(f64, f64)> {
        let perto = |t0: f64| -> Option<(f64, f64)> {
            let mut melhor: Option<(f64, (f64, f64))> = None;
            for (i, c) in centros.iter().enumerate() {
                let Some(c) = c else { continue };
                let d = (tab.tempo(i) - t0).abs();
                if melhor.is_none_or(|(m, _)| d < m) {
                    melhor = Some((d, *c));
                }
            }
            melhor.map(|(_, c)| c)
        };
        let na: Vec<usize> = (0..centros.len()).filter(|&i| (tab.tempo(i) - t).abs() <= s / 2.0 + 1e-9).collect();
        if na.is_empty() {
            return perto(t);
        }
        let cs: Vec<(f64, f64)> = na.iter().map(|&i| centros[i].or_else(|| perto(tab.tempo(i)))).collect::<Option<_>>()?;
        let n = cs.len() as f64;
        Some((cs.iter().map(|c| c.0).sum::<f64>() / n, cs.iter().map(|c| c.1).sum::<f64>() / n))
    }

    /// Por slot: marca medida e prevista (px do canvas 320×180) e o centro da região que o render usou e o previsto
    /// (px do canvas, deduzido da marca: a marca verdadeira é conhecida).
    #[derive(Debug, Clone, Copy)]
    struct Slot {
        medida: Option<(f64, f64)>,
        prevista: (f64, f64),
        regiao_usada: (f64, f64),
        regiao_prevista: (f64, f64),
        /// A marca prevista cabe inteira no canvas (o centro dos pixels dela só vale assim).
        inteira: bool,
    }

    struct CasoSegue<'a> {
        nome: &'a str,
        fonte: Fonte,
        fps: (i64, i64),
        girar90: bool,
        trecho: (usize, usize),
        /// Altura da região (fração da origem girada); a largura sai da proporção da saída.
        altura: f64,
        suavizar_s: f64,
        /// Centro do rastreio por quadro da mídia (espaço de origem); None = sem caixa.
        centros: Vec<Option<(f64, f64)>>,
        origem: Value,
        extra: Vec<Value>,
    }

    /// Renderiza o zoom que segue (região de tamanho fixo na proporção da saída, 2× ou o maior que cabe) e mede
    /// cada slot contra a regra refeita aqui; a marca (verdadeira) e o rastreio (`centros`) podem diferir.
    fn rodar_segue(cap: &Arc<Capacidades>, c: &CasoSegue) -> (Vec<QuadroRgb>, Vec<Slot>) {
        let m = apoio::medida(cap, c.fonte);
        let tab = &m.info.quadros;
        let (we, he) = (m.info.exibido.0 as f64, m.info.exibido.1 as f64);
        let (wg, hg) = if c.girar90 { (he, we) } else { (we, he) };
        // 90° horário: (x, y) da origem → (1 − y, x) da girada
        let girar = |p: (f64, f64)| if c.girar90 { (1.0 - p.1, p.0) } else { p };
        let prop = (320.0 / 180.0) / (wg / hg);
        let (mut w, mut h) = (c.altura * prop, c.altura);
        if w > 1.0 {
            h /= w;
            w = 1.0;
        }
        let ancoras: Vec<Value> = c
            .centros
            .iter()
            .enumerate()
            .filter_map(|(j, x)| x.map(|(cx, cy)| (j, cx, cy)))
            .map(|(j, cx, cy)| {
                let mk = fontes::marca_esperada(c.fonte, m.numeros[j]);
                ancora(tab.tempo(j), Some(tab.pts[j]), [cx - mk[2] / 2.0, cy - mk[3] / 2.0, mk[2], mk[3]])
            })
            .collect();
        let mut r = rastreio("r1", Some("m1"), "fonte", ancoras);
        r["origem"] = c.origem.clone();
        let mut efs = Vec::new();
        if c.girar90 {
            efs.push(efeito("girar", "g1", json!({ "graus": 90, "espelhar_h": false, "espelhar_v": false })));
        }
        efs.push(enq_segue(json!({ "x": 0.0, "y": 0.0, "w": w, "h": h }), c.suavizar_s, "cobrir"));
        efs.extend(c.extra.iter().cloned());
        let mut p = projeto(320, 180, c.fps, vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, c.trecho.0, c.trecho.1, 1.0, Value::Array(efs))], json!({ "rastreios": [r] }));
        p.saida.cor_fundo = FUNDO_VERDE.into();
        let mundo = Mundo::novo(cap).com("m1", &m.info);
        let plano = mundo.compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{} ({}): {e}", c.nome, cap.versao));
        let quadros = renderizar(cap, &plano, &format!("zs-rev-{}", c.nome));
        let centros_g: Vec<Option<(f64, f64)>> = c.centros.iter().map(|x| x.map(girar)).collect();
        let slots = quadros
            .iter()
            .enumerate()
            .map(|(k, q)| {
                let j = c.trecho.0 + k;
                let t = tab.tempo(j);
                let ce = centro_pela_regra(tab, &centros_g, t, c.suavizar_s).expect("centro");
                let x = (ce.0 - w / 2.0).clamp(0.0, (1.0 - w).max(0.0));
                let y = (ce.1 - h / 2.0).clamp(0.0, (1.0 - h).max(0.0));
                let mk = fontes::marca_esperada(c.fonte, m.numeros[j]);
                let verdade = girar((mk[0] + mk[2] / 2.0, mk[1] + mk[3] / 2.0));
                let prevista = ((verdade.0 - x) / w * 320.0, (verdade.1 - y) / h * 180.0);
                let lado = if c.girar90 { (mk[3] / w * 320.0, mk[2] / h * 180.0) } else { (mk[2] / w * 320.0, mk[3] / h * 180.0) };
                let inteira = prevista.0 - lado.0 / 2.0 >= 2.0 && prevista.0 + lado.0 / 2.0 <= 318.0 && prevista.1 - lado.1 / 2.0 >= 2.0 && prevista.1 + lado.1 / 2.0 <= 178.0;
                let medida = centro_vermelho_em(q, [0, 0, 320, 180]);
                assert_eq!(verde(q), 0, "{} slot {k} ({}): barra ou borda", c.nome, cap.versao);
                let inteira = inteira && medida.is_some();
                let m = medida.unwrap_or(prevista);
                let usada = ((verdade.0 - m.0 / 320.0 * w + w / 2.0) / w * 320.0, (verdade.1 - m.1 / 180.0 * h + h / 2.0) / h * 180.0);
                Slot { medida, prevista, regiao_usada: usada, regiao_prevista: ((x + w / 2.0) / w * 320.0, (y + h / 2.0) / h * 180.0), inteira }
            })
            .collect();
        (quadros, slots)
    }

    /// Pior desvio medido × previsto (px), nos slots com a marca inteira no canvas.
    fn pior_desvio(s: &[Slot]) -> f64 {
        assert!(s.iter().filter(|x| x.inteira).count() * 3 > s.len(), "poucos slots com a marca inteira");
        s.iter().filter(|x| x.inteira).map(|x| x.medida.map_or(0.0, |m| (m.0 - x.prevista.0).abs().max((m.1 - x.prevista.1).abs()))).fold(0.0, f64::max)
    }

    /// Maior passo e maior aceleração (px do canvas por quadro) do centro da região, num eixo.
    fn passos(v: &[f64]) -> (f64, f64, f64) {
        let d: Vec<f64> = v.windows(2).map(|p| p[1] - p[0]).collect();
        let d2: Vec<f64> = d.windows(2).map(|p| p[1] - p[0]).collect();
        let rms = (d2.iter().map(|x| x * x).sum::<f64>() / d2.len().max(1) as f64).sqrt();
        (d.iter().map(|x| x.abs()).fold(0.0, f64::max), d2.iter().map(|x| x.abs()).fold(0.0, f64::max), rms)
    }

    /// Gerador congruencial: o mesmo ruído sempre.
    fn ruido(semente: u64) -> impl FnMut() -> f64 {
        let mut x = semente;
        move || {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
        }
    }

    fn centros_da_marca(cap: &Capacidades, f: Fonte, quadros: std::ops::Range<usize>, tremor: f64) -> Vec<Option<(f64, f64)>> {
        let m = apoio::medida(cap, f);
        let mut r = ruido(0x5eed);
        (0..m.info.quadros.pts.len())
            .map(|j| {
                let mk = fontes::marca_esperada(f, m.numeros[j]);
                let (a, b) = (r(), r());
                quadros.contains(&j).then(|| (mk[0] + mk[2] / 2.0 + a * tremor, mk[1] + mk[3] / 2.0 + b * tremor))
            })
            .collect()
    }

    /// Rastreio que treme (±1,2 % por quadro, o que um seguidor real faz): com suavizar 0 o zoom treme junto; com 1 s
    /// e 3 s a média tira o tremor. O render bate com a regra (até 2 px) e a aceleração do centro da região cai.
    #[test]
    fn tremor_com_suavizar_0_1_e_3() {
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let mut acel = Vec::new();
        for s in [0.0, 1.0, 3.0] {
            let centros = centros_da_marca(&cap, Fonte::Mp4, 0..450, 0.012);
            let caso = CasoSegue { nome: &format!("tremor-{s}"), fonte: Fonte::Mp4, fps: (30, 1), girar90: false, trecho: (90, 300), altura: 0.5, suavizar_s: s, centros, origem: json!({ "tipo": "manual" }), extra: vec![] };
            let (_, slots) = rodar_segue(&cap, &caso);
            let pior = pior_desvio(&slots);
            assert!(pior <= 2.0, "suavizar {s} ({}): render × regra {pior:.2} px", cap.versao);
            let (px, ax, rx) = passos(&slots.iter().map(|x| x.regiao_prevista.0).collect::<Vec<_>>());
            let (_, ay, ry) = passos(&slots.iter().map(|x| x.regiao_prevista.1).collect::<Vec<_>>());
            // no render: aceleração do centro da região deduzido da marca, só com a marca inteira nos três slots
            let am: Vec<f64> = slots.windows(3).filter(|w| w.iter().all(|x| x.inteira)).map(|w| (w[2].regiao_usada.0 - 2.0 * w[1].regiao_usada.0 + w[0].regiao_usada.0).abs()).collect();
            let rm = (am.iter().map(|x| x * x).sum::<f64>() / am.len().max(1) as f64).sqrt();
            eprintln!("tremor s={s} ({}): render×regra {pior:.2} px; regra: passo {px:.2} acel {ax:.2} (y {ay:.2}) rms {rx:.3} (y {ry:.3}); render: acel rms {rm:.3} em {} trincas", cap.versao, am.len());
            acel.push((rx.max(ry), ax.max(ay), rm));
        }
        // tremor de ±1,2 % a 2× é ±7,7 px por quadro: sem média passa inteiro; 1 s (31 quadros) e 3 s o tiram. A
        // aceleração máxima com 1 s é a da região parando na borda da imagem (regra do contrato), não tremor.
        assert!(acel[0].0 > 5.0 && acel[0].2 > 5.0, "{acel:?}");
        assert!(acel[1].0 < 1.0 && acel[1].2 < 1.5, "{acel:?}");
        assert!(acel[2].0 < acel[1].0 && acel[2].1 < 1.0 && acel[2].2 < 1.0, "{acel:?}");
    }

    /// Pessoa some 5 s (rastreio seguido: âncoras longe, sem caixa no meio) e volta noutro lugar; rastreio que começa
    /// depois do início do trecho. O render bate com a regra onde a marca aparece inteira, sem borda em nenhum slot.
    /// Emenda D9 (rodada 2): na lacuna o zoom atravessa em `s` (1 s), sem salto: o maior passo do centro da região,
    /// previsto e medido no render, não passa da maior velocidade da marca (px do canvas por quadro) mais 1 px.
    #[test]
    fn lacuna_longa_e_rastreio_que_comeca_depois() {
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let mut centros = centros_da_marca(&cap, Fonte::Mp4, 0..450, 0.0);
        for c in &mut centros[120..270] {
            *c = None;
        }
        let caso = CasoSegue { nome: "lacuna", fonte: Fonte::Mp4, fps: (30, 1), girar90: false, trecho: (0, 450), altura: 0.5, suavizar_s: 1.0, centros, origem: json!({ "tipo": "assistido" }), extra: vec![] };
        let (_, slots) = rodar_segue(&cap, &caso);
        let pior = pior_desvio(&slots);
        let xs: Vec<f64> = slots.iter().map(|x| x.regiao_prevista.0).collect();
        let saltos: Vec<(usize, f64)> = xs.windows(2).enumerate().map(|(k, p)| (k + 1, p[1] - p[0])).filter(|(_, d)| d.abs() > 10.0).collect();
        // maior velocidade da marca no canvas (região de 0,5: 2×)
        let m = apoio::medida(&cap, Fonte::Mp4);
        let vel = (1..450).map(|j| (fontes::marca_esperada(Fonte::Mp4, m.numeros[j])[0] - fontes::marca_esperada(Fonte::Mp4, m.numeros[j - 1])[0]).abs() / 0.5 * 320.0).fold(0.0, f64::max);
        let na_lacuna = |x: &[Slot]| -> f64 { x[100..300].windows(2).map(|p| (p[1].regiao_prevista.0 - p[0].regiao_prevista.0).abs()).fold(0.0, f64::max) };
        let medido = slots[100..300].windows(2).filter(|p| p[0].inteira && p[1].inteira).map(|p| (p[1].regiao_usada.0 - p[0].regiao_usada.0).abs()).fold(0.0, f64::max);
        eprintln!("lacuna ({}): render×regra {pior:.2} px; maior passo na lacuna: regra {:.2} px, render {medido:.2} px; marca {vel:.2} px/quadro; saltos > 10 px: {saltos:?}", cap.versao, na_lacuna(&slots));
        assert!(pior <= 2.0, "lacuna ({}): {pior:.2}", cap.versao);
        assert!(saltos.is_empty() && na_lacuna(&slots) <= vel + 1.0 && medido <= vel + 1.0 + 2.0, "lacuna ({}): salto na lacuna", cap.versao);
        let centros = centros_da_marca(&cap, Fonte::Mp4, 150..450, 0.0);
        let caso = CasoSegue { nome: "depois", fonte: Fonte::Mp4, fps: (30, 1), girar90: false, trecho: (30, 300), altura: 0.5, suavizar_s: 1.0, centros, origem: json!({ "tipo": "manual" }), extra: vec![] };
        let (_, slots) = rodar_segue(&cap, &caso);
        let pior = pior_desvio(&slots);
        let xs: Vec<f64> = slots.iter().map(|x| x.regiao_prevista.0).collect();
        eprintln!("começa depois ({}): render×regra {pior:.2} px; passo máx {:.2}", cap.versao, passos(&xs).0);
        assert!(pior <= 2.0, "começa depois ({}): {pior:.2}", cap.versao);
        assert!(passos(&xs).0 < 10.0, "começa depois: salto");
    }

    /// Giro de 90° (efeito e contêiner), 59,94 com início ≠ 0, 29,97 de pixel 10:11: a marca no lugar da regra.
    #[test]
    fn segue_em_giro_taxa_e_inicio() {
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        for (nome, f, fps, girar90, tr) in [
            ("girar-90", Fonte::Mp4, (30, 1), true, (60usize, 240usize)),
            ("conteiner-90", Fonte::Giro90, (30, 1), false, (60, 240)),
            ("ts-5994", Fonte::Ts5994, (60000, 1001), false, (100, 400)),
            ("pixel-2997", Fonte::PixelNaoQuadrado, (30000, 1001), false, (20, 170)),
        ] {
            let m = apoio::medida(&cap, f);
            let centros = centros_da_marca(&cap, f, 0..m.info.quadros.pts.len(), 0.0);
            let caso = CasoSegue { nome, fonte: f, fps, girar90, trecho: tr, altura: 0.5, suavizar_s: 1.0, centros, origem: json!({ "tipo": "manual" }), extra: vec![] };
            let (_, slots) = rodar_segue(&cap, &caso);
            let pior = pior_desvio(&slots);
            let (pa, aa, _) = passos(&slots.iter().map(|x| if girar90 { x.regiao_usada.1 } else { x.regiao_usada.0 }).collect::<Vec<_>>());
            eprintln!("{nome} ({}): {} slots, início {:.3} s, render×regra {pior:.2} px, passo {pa:.2} acel {aa:.2}", cap.versao, slots.len(), m.info.quadros.tempo(0));
            assert!(pior <= 2.0, "{nome} ({}): {pior:.2} px", cap.versao);
        }
    }

    /// Destaque e lupa do mesmo rastreio dentro do zoom que segue: o retângulo em volta da marca e a lupa com a
    /// marca no centro dela, em todo slot.
    #[test]
    fn destaque_e_lupa_no_lugar_dentro_do_zoom_que_segue() {
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let destaque = efeito("destaque", "d1", json!({ "forma": "retangulo", "rastreio": "r1", "traco": { "cor": "#0000ff", "espessura": 0.006, "contorno": false }, "tamanho": 0.1, "angulo_graus": 0.0, "margem": 0.1, "rotulo": null }));
        // janela 45 px da origem, 56 px no canvas (1,25×), lupa de 84 px no alto à esquerda: longe da marca (y ≥ 98)
        let lupa = efeito("lupa", "L1", json!({ "rastreio": "r1", "janela": 0.25, "fator": 1.5, "forma": "retangulo", "interpolacao": "bilinear", "onde": { "modo": "fixa", "x": 0.15, "y": 0.2 }, "moldura": { "cor": "#00ffff", "espessura": 0.012, "contorno": false }, "ligacao": false }));
        let centros = centros_da_marca(&cap, Fonte::Mp4, 0..450, 0.0);
        let caso = CasoSegue { nome: "destaque", fonte: Fonte::Mp4, fps: (30, 1), girar90: false, trecho: (60, 240), altura: 0.8, suavizar_s: 1.0, centros, origem: json!({ "tipo": "manual" }), extra: vec![destaque, lupa] };
        let (quadros, slots) = rodar_segue(&cap, &caso);
        let (mut pior_d, mut pior_l, mut n) = (0.0f64, 0.0f64, 0);
        for (k, (q, s)) in quadros.iter().zip(&slots).enumerate() {
            let marca = centro_vermelho_em(q, [0, 88, 320, 180]).unwrap_or_else(|| panic!("slot {k}: sem a marca"));
            let dm = (marca.0 - s.prevista.0).abs().max((marca.1 - s.prevista.1).abs());
            let b = caixa_de(q, [0, 88, 320, 180], &eh_azul).unwrap_or_else(|| panic!("slot {k} ({}): sem o destaque", cap.versao));
            let d = ((b[0] + b[2] + 1) as f64 / 2.0 - marca.0).abs().max(((b[1] + b[3] + 1) as f64 / 2.0 - marca.1).abs());
            assert!(d <= 2.0 && dm <= 2.0, "slot {k} ({}): destaque {b:?} × marca {marca:?} (prevista {:?})", cap.versao, s.prevista);
            pior_d = pior_d.max(d);
            // a lupa encosta no alto do canvas: o centro dela é o da moldura (ciano)
            let mo = caixa_de(q, [0, 0, 110, 88], &|p: &[u8]| p[0] < 100 && p[1] > 150 && p[2] > 150).unwrap_or_else(|| panic!("slot {k}: sem a moldura da lupa"));
            let l = centro_vermelho_em(q, [mo[0] + 2, mo[1] + 2, mo[2] - 1, mo[3] - 1]).unwrap_or_else(|| panic!("slot {k} ({}): lupa sem a marca", cap.versao));
            let dl = (l.0 - (mo[0] + mo[2] + 1) as f64 / 2.0).abs().max((l.1 - (mo[1] + mo[3] + 1) as f64 / 2.0).abs());
            pior_l = pior_l.max(dl);
            assert!(dl <= 2.0, "slot {k} ({}): marca na lupa em {l:?}", cap.versao);
            n += 1;
        }
        eprintln!("destaque e lupa ({}): {n} slots, destaque × marca {pior_d:.2} px, lupa × centro {pior_l:.2} px", cap.versao);
    }

    // ------------------------------------------------------------------ blocos curtos (rodada 2)

    /// O clipe que acompanha vai em blocos de até `BLOCO_Q` slots; o resto do programa divide como antes.
    #[test]
    fn clipe_que_segue_em_blocos_curtos() {
        let mut cap = apoio::cap_falsa();
        cap.filtros.insert("perspective".into());
        let cap = Arc::new(cap);
        let fonte = apoio::fonte(apoio::tabela((1, 30), (0..2000).collect()), (320, 180));
        let mundo = Mundo::novo(&cap).com("m1", &fonte);
        let t = fonte.quadros.clone();
        let r = rastreio("r1", Some("m1"), "fonte", vec![ancora(0.0, Some(0), [0.4, 0.4, 0.1, 0.1])]);
        let fixo = efeito("enquadramento", "q9", json!({ "regiao": { "x": 0.3, "y": 0.3, "w": 0.4, "h": 0.4 }, "ajuste": "cobrir", "interpolacao": "bilinear" }));
        let principal = |segue: bool| {
            let e = if segue { enq_segue(json!({ "x": 0.3, "y": 0.3, "w": 0.4, "h": 0.4 }), 1.0, "cobrir") } else { fixo.clone() };
            vec![trecho("c0", "m1", &t, 0, 100, 1.0, json!([])), trecho("c1", "m1", &t, 100, 1100, 1.0, json!([e])), trecho("c2", "m1", &t, 1100, 1300, 1.0, json!([]))]
        };
        let janelas = |segue: bool| -> Vec<(u64, u32)> {
            let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], principal(segue), json!({ "rastreios": [r.clone()] }));
            mundo.compilar(&p, &alvo_final("cpu")).unwrap().blocos.iter().map(|b| (b.de_q, b.n_q)).collect()
        };
        assert_eq!(janelas(false), [(0, 1300)]);
        // c1 = [100, 1100): 300 slots por bloco a partir do começo dele; o primeiro leva c0 junto
        assert_eq!(janelas(true), [(0, 400), (400, 300), (700, 300), (1000, 300)]);
        assert!(!clipe_segue(&projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], principal(false), json!({})), "c1"));
    }

    /// Bloco curto no clipe que acompanha não muda nenhum quadro: o composto (hash por quadro) dividido é o do bloco
    /// único (o de antes da rodada 2 para este trecho), nos dois ffmpeg.
    #[test]
    fn blocos_curtos_dao_o_mesmo_composto() {
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let tab = &m.info.quadros;
        let mut ruido_ = ruido(0x11);
        let ancoras: Vec<Value> = (0..450)
            .map(|j| {
                let c = fontes::marca_esperada(Fonte::Mp4, m.numeros[j]);
                ancora(tab.tempo(j), Some(tab.pts[j]), [c[0] + 0.01 * ruido_(), c[1] + 0.01 * ruido_(), c[2], c[3]])
            })
            .collect();
        let k = |j: usize, w: f64| json!({ "t": tab.tempo(j), "pts": tab.pts[j], "v": { "x": 0.1, "y": 0.1, "w": w, "h": w }, "interp": "suave" });
        let mundo = Mundo::novo(&cap).com("m1", &m.info);
        for s in [1.0, 0.0] {
            let e = enq_segue(json!([k(0, 0.8), k(200, 0.4), k(400, 0.6)]), s, "cobrir");
            let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, 0, 450, 1.0, json!([e]))], json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras.clone())] }));
            let dividido = mundo.compilar(&p, &alvo_final("cpu")).unwrap();
            assert_eq!(dividido.blocos.len(), 2);
            crate::edicao::motor::blocos::UM_BLOCO.with(|c| c.set(true));
            let inteiro = mundo.compilar(&p, &alvo_final("cpu"));
            crate::edicao::motor::blocos::UM_BLOCO.with(|c| c.set(false));
            let inteiro = inteiro.unwrap();
            assert_eq!(inteiro.blocos.len(), 1);
            let a = executor::rodar(&cap, &dividido, &fontes::pasta_temporaria("zs-blocos-div"), &Opcoes::default()).unwrap().compostos;
            let b = executor::rodar(&cap, &inteiro, &fontes::pasta_temporaria("zs-blocos-um"), &Opcoes::default()).unwrap().compostos;
            assert_eq!(a.len(), 450);
            let dif = a.iter().zip(&b).filter(|(x, y)| x != y).count();
            assert_eq!(dif, 0, "suavizar {s} ({}): {dif} quadros diferentes (primeiro {:?})", cap.versao, a.iter().zip(&b).position(|(x, y)| x != y));
            eprintln!("blocos curtos (s = {s}, {}): 450 quadros iguais ao bloco único; maior grafo {} × {} bytes", cap.versao,
                      dividido.blocos.iter().map(|b| b.grafo.len()).max().unwrap(), inteiro.blocos[0].grafo.len());
        }
    }

    // ------------------------------------------------------------------ desempenho (rodada 2)

    /// Trecho de 2 min a 60 qps (720p): zoom só com chaves × zoom que segue (s = 1 e s = 0 com rastreio que treme).
    /// Mede o tempo da exportação e o tamanho do grafo; `SICRO_ZS_COMPOSTOS=<pasta>` grava (1ª vez) ou confere
    /// (depois) o hash do composto por quadro de cada caso, por versão do ffmpeg. Longo: só à mão.
    #[test]
    #[ignore = "medida longa (rodada 2 do ZS): rodar à mão"]
    fn desempenho_trecho_longo() {
        use std::sync::atomic::AtomicBool;
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let pasta = std::env::temp_dir().join("sicro-zs-desempenho").join(&cap.sha256[..16]);
        std::fs::create_dir_all(&pasta).unwrap();
        let video = pasta.join("fonte-720p60-2min.mp4");
        if !video.exists() {
            let tmp = pasta.join("fonte.tmp.mp4");
            let st = std::process::Command::new(&cap.binario)
                .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=60", "-t", "120", "-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-g", "60", "-pix_fmt", "yuv420p"])
                .arg(&tmp)
                .status()
                .unwrap();
            assert!(st.success());
            std::fs::rename(&tmp, &video).unwrap();
        }
        let tab = TabelaQuadros::do_decodificador(&cap, &video, &AtomicBool::new(false), &mut |_| {}).unwrap();
        let info = crate::edicao::quadros::fonte_info(&cap, &video, &"0".repeat(64), Arc::new(tab), None).unwrap();
        let tab = info.quadros.clone();
        let n = tab.pts.len();
        assert_eq!(n, 7200);
        let mut r = ruido(0x2e10);
        let ancoras: Vec<Value> = (0..n)
            .map(|j| {
                let t = tab.tempo(j);
                let (cx, cy) = (0.5 + 0.25 * (t * std::f64::consts::TAU / 20.0).sin() + 0.012 * r(), 0.5 + 0.1 * (t * std::f64::consts::TAU / 13.0).sin() + 0.012 * r());
                ancora(t, Some(tab.pts[j]), [cx - 0.04, cy - 0.04, 0.08, 0.08])
            })
            .collect();
        let k = |j: usize, w: f64| json!({ "t": tab.tempo(j), "pts": tab.pts[j], "v": { "x": (1.0 - w) / 2.0, "y": (1.0 - w) / 2.0, "w": w, "h": w } });
        let chaves = json!([k(0, 0.8), k(1800, 0.4), k(3600, 0.4), k(5400, 0.8)]);
        let mundo = Mundo::novo(&cap).com("m1", &info);
        let relato = std::env::var("SICRO_ZS_COMPOSTOS").ok().map(std::path::PathBuf::from);
        let mut tempos = Vec::new();
        for (nome, segue) in [("chaves", None), ("segue-1", Some(1.0)), ("segue-0", Some(0.0))] {
            let mut e = efeito("enquadramento", "q1", json!({ "regiao": chaves.clone(), "ajuste": "cobrir", "interpolacao": "bilinear" }));
            if let Some(s) = segue {
                e["segue"] = json!({ "rastreio": "r1", "suavizar_s": s });
            }
            let p = projeto(1280, 720, (60, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", &tab, 0, n, 1.0, json!([e]))],
                            json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras.clone())] }));
            let inicio = std::time::Instant::now();
            let plano = mundo.compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{nome}: {e}"));
            let compilar = inicio.elapsed().as_secs_f64();
            let maior = plano.blocos.iter().map(|b| b.grafo.len()).max().unwrap_or(0);
            let saida = fontes::pasta_temporaria(&format!("zs-desempenho-{nome}"));
            let rodado = executor::rodar(&cap, &plano, &saida, &Opcoes::default()).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
            let total = inicio.elapsed().as_secs_f64();
            let _ = std::fs::remove_dir_all(&saida);
            assert_eq!(rodado.compostos.len(), n, "{nome}");
            eprintln!("desempenho {nome} ({}): {total:.1} s (compilar {compilar:.1} s), {} blocos, maior grafo {:.2} MB, carga {}", cap.versao, plano.blocos.len(), maior as f64 / 1e6,
                      std::fs::read_to_string("/proc/loadavg").unwrap_or_default().trim());
            tempos.push(total);
            if let Some(dir) = &relato {
                std::fs::create_dir_all(dir).unwrap();
                let arq = dir.join(format!("{}-{nome}.txt", cap.versao));
                let texto = rodado.compostos.join("\n");
                if arq.exists() {
                    let antes = std::fs::read_to_string(&arq).unwrap();
                    let (a, b): (Vec<&str>, Vec<&str>) = (antes.lines().collect(), texto.lines().collect());
                    let dif = a.iter().zip(&b).filter(|(x, y)| x != y).count();
                    assert!(dif == 0 && a.len() == b.len(), "{nome} ({}): {dif} quadros com outro composto (primeiro: {:?})", cap.versao, a.iter().zip(&b).position(|(x, y)| x != y));
                    eprintln!("desempenho {nome} ({}): composto igual ao gravado ({} quadros)", cap.versao, a.len());
                } else {
                    std::fs::write(&arq, texto).unwrap();
                }
            }
        }
        eprintln!("desempenho ({}): segue-1 {:.2}×, segue-0 {:.2}× o das chaves", cap.versao, tempos[1] / tempos[0], tempos[2] / tempos[0]);
    }
}
