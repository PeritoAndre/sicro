//! Mapa de quadros em inteiros (§3.1), linha resolvida e conversões fonte ↔ programa. Dono: P0.3b.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::modelo::{
    BaseRastreio, ClipePrincipal, ClipeResolvido, ClipeSobreposto, Efeito, FaixaFonte, LinhaResolvida, LivreResolvido, OndePrender, Posicao,
    Prender, ProjetoEdicao, Racional, Tolerante, Velocidade,
};
use super::motor::Contexto;
use super::quadros::TabelaQuadros;
use crate::error::{Result, SicroError};

/// Taxa do áudio do programa (Hz).
pub const TAXA_AUDIO: u32 = 48_000;

/// O `setpts` calcula em `double`: inteiro só é exato abaixo de 2^53.
const LIMITE_DOUBLE: i128 = 1 << 53;
/// Teto de slots de um trecho (`quadro_do_slot` tem um `u32` por slot): 2^24 = 77 h de programa a 60 qps.
/// Documento estragado (taxa de saída de milhões) dá `mapa_grande` em vez de pedir gigabytes.
// CONTRATO: acréscimo (revisão). O espelho do front aplica o mesmo teto.
pub const MAX_SLOTS: u32 = 1 << 24;

/// Mapa de um trecho (§3.1). `j_in`/`j_out` = índices na tabela; `c`, `r`, `m`, `kn`, `kd`, `e` = constantes
/// inteiras da fórmula; `p0` = P_jin; `quadro_do_slot[q]` = índice na tabela do quadro mostrado no slot q do clipe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapaQuadros {
    pub j_in: usize,
    pub j_out: usize,
    pub c: i64,
    pub r: i64,
    pub m: i64,
    pub kn: i64,
    pub kd: i64,
    pub e: i64,
    pub p0: i64,
    pub n_q: u32,
    pub quadro_do_slot: Vec<u32>,
    pub repetidos: u32,
    pub descartados: u32,
}

fn erro(codigo: &str) -> SicroError {
    SicroError::Validation(codigo.into())
}

fn mdc(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Teto de a/b, com b > 0.
fn teto(a: i128, b: i128) -> i128 {
    a.div_euclid(b) + i128::from(a.rem_euclid(b) != 0)
}

/// Constantes do §3.1 para um trecho, todas em `i128`.
struct Conta {
    c: i128,
    r: i128,
    m: i128,
    kn: i128,
    kd: i128,
    e: i128,
    p0: i128,
    pfim: i128,
}

impl Conta {
    fn nova(tab: &TabelaQuadros, j_in: usize, j_out: usize, v: Racional, fps: Racional) -> Result<Self> {
        let n = tab.pts.len();
        let (tn, td) = (tab.tb.num as i128, tab.tb.den as i128);
        let (fnum, fden) = (fps.num as i128, fps.den as i128);
        let (vn, vd) = (v.num as i128, v.den as i128);
        if j_in >= j_out || j_out > n || tn <= 0 || td <= 0 || fnum <= 0 || fden <= 0 {
            return Err(erro("intervalo"));
        }
        // o `trim` corta por pts: a ponta do trecho tem de ser o primeiro quadro do pts dela
        let repete = |j: usize| j > 0 && j < n && tab.pts[j - 1] == tab.pts[j];
        if repete(j_in) || repete(j_out) {
            return Err(erro("intervalo"));
        }
        if vn <= 0 || vd <= 0 {
            return Err(erro("velocidade"));
        }
        // taxa ou base de tempo absurda (documento ou tabela estragados) estoura até o i128: `mapa_grande`, nunca pânico
        let mul = |a: i128, b: i128| a.checked_mul(b).ok_or_else(|| erro("mapa_grande"));
        let c = mul(td / mdc(td, fnum), fnum)?;
        let r = mul(tn, c)? / td;
        let m = mul(c, fden)? / fnum;
        let (kn, kd) = (mul(fnum, vd)?, mul(mul(fden, vn)?, c)?);
        let g = mdc(kn, kd);
        let (kn, kd) = (kn / g, kd / g);
        let p = |j: usize| mul(tab.pts[j] as i128, r);
        let p0 = p(j_in)?;
        // fim da mídia: o último quadro dura o passo mediano
        let pfim = if j_out < n { p(j_out)? } else { p(n - 1)?.checked_add(mul(tab.passo_mediano.max(1) as i128, r)?).ok_or_else(|| erro("mapa_grande"))? };
        if pfim <= p0 {
            return Err(erro("intervalo"));
        }
        if p0.abs() >= LIMITE_DOUBLE || pfim.abs() >= LIMITE_DOUBLE || mul(pfim - p0, kn)? >= LIMITE_DOUBLE {
            return Err(erro("mapa_grande"));
        }
        // menor passo positivo do trecho (pts repetido não zera a folga)
        let mut pmin: Option<i128> = None;
        for j in j_in..j_out {
            let d = if j + 1 < j_out { p(j + 1)? } else { pfim } - p(j)?;
            if d < 0 {
                return Err(erro("intervalo"));
            }
            if d > 0 {
                pmin = Some(pmin.map_or(d, |x| x.min(d)));
            }
        }
        let e = match pmin {
            Some(pm) => kd.min(pm * kn) / 2,
            None => kd / 2,
        };
        Ok(Conta { c, r, m, kn, kd, e, p0, pfim })
    }

    fn slot(&self, pts: i64) -> i128 {
        teto((pts as i128 * self.r - self.p0) * self.kn - self.e, self.kd)
    }

    fn n_q(&self) -> i128 {
        teto((self.pfim - self.p0) * self.kn - self.e, self.kd).max(1)
    }
}

/// Mapa dos quadros `j_in..j_out` para slots na taxa `fps`, em i128. Erro `mapa_grande` se passar de 2^53 ou de `MAX_SLOTS`;
/// `intervalo` se o trecho é vazio, sai da tabela ou tem ponta no meio de um pts repetido (`faixa_do_trecho` não devolve essas).
pub fn mapa_trecho(tab: &TabelaQuadros, j_in: usize, j_out: usize, v: &Velocidade, fps: Racional) -> Result<MapaQuadros> {
    let v = match v {
        Velocidade::Constante { fator } => fator_racional(*fator)?,
        // Fase 3 (P3.4)
        Velocidade::Rampa { .. } => return Err(erro("nao_suportado: rampa")),
    };
    mapa_racional(tab, j_in, j_out, v, fps)
}

/// `mapa_trecho` com a velocidade já em fração.
pub(crate) fn mapa_racional(tab: &TabelaQuadros, j_in: usize, j_out: usize, v: Racional, fps: Racional) -> Result<MapaQuadros> {
    let k = Conta::nova(tab, j_in, j_out, v, fps)?;
    let n_q = k.n_q();
    if n_q > MAX_SLOTS as i128 || j_out > u32::MAX as usize {
        return Err(erro("mapa_grande"));
    }
    let n_q = n_q as usize;
    let mut quadro_do_slot = vec![0u32; n_q];
    let mut distintos = 0u32;
    let mut s = 0usize;
    for j in j_in..j_out {
        // o quadro j fica do slot dele até o slot do seguinte
        let depois = if j + 1 < j_out { (k.slot(tab.pts[j + 1]).max(0) as usize).min(n_q) } else { n_q };
        if depois > s {
            quadro_do_slot[s..depois].fill(j as u32);
            distintos += 1;
            s = depois;
        }
    }
    let i = |v: i128| i64::try_from(v).map_err(|_| erro("mapa_grande"));
    Ok(MapaQuadros {
        j_in,
        j_out,
        c: i(k.c)?,
        r: i(k.r)?,
        m: i(k.m)?,
        kn: i(k.kn)?,
        kd: i(k.kd)?,
        e: i(k.e)?,
        p0: i(k.p0)?,
        n_q: n_q as u32,
        quadro_do_slot,
        repetidos: n_q as u32 - distintos,
        descartados: (j_out - j_in) as u32 - distintos,
    })
}

/// Quadros descartados pelo arquivo inteiro a 1× na taxa `fps` (sem montar o mapa).
pub(crate) fn descartados_a_1x(tab: &TabelaQuadros, fps: Racional) -> Result<u64> {
    let n = tab.pts.len();
    let k = Conta::nova(tab, 0, n, Racional { num: 1, den: 1 }, fps)?;
    let n_q = k.n_q();
    let (mut s, mut distintos) = (0i128, 0u64);
    for j in 0..n {
        let depois = if j + 1 < n { k.slot(tab.pts[j + 1]).clamp(0, n_q) } else { n_q };
        if depois > s {
            distintos += 1;
            s = depois;
        }
    }
    Ok(n as u64 - distintos)
}

/// Maior distância entre um quadro e o slot dele, no arquivo inteiro a 1× na taxa `fps`: fração (numerador,
/// denominador) em segundos.
pub(crate) fn desvio_a_1x(tab: &TabelaQuadros, fps: Racional) -> Result<(i128, i128)> {
    let k = Conta::nova(tab, 0, tab.pts.len(), Racional { num: 1, den: 1 }, fps)?;
    // slot·M e P_j − P_0 estão na mesma base 1/c
    let maior = tab.pts.iter().map(|&p| (k.slot(p) * k.m - (p as i128 * k.r - k.p0)).abs()).max().unwrap_or(0);
    Ok((maior, k.c))
}

/// Quadros `[j_in, j_out)` de um trecho pelo par (segundos, pts) de cada ponta; o pts é a identidade (D4)
/// e os segundos só valem quando ele falta ou não está na tabela. `j_out = n` = fim da mídia. Erro `intervalo`.
// CONTRATO: acréscimo. `resolver`, `validar` e o motor acham a faixa pela mesma conta.
pub fn faixa_do_trecho(tab: &TabelaQuadros, inicio_s: f64, inicio_pts: Option<i64>, fim_s: f64, fim_pts: Option<i64>) -> Result<(usize, usize)> {
    if tab.pts.is_empty() {
        return Err(erro("intervalo"));
    }
    let (j_in, j_out) = (tab.do_par(inicio_s, inicio_pts), tab.fim_do_par(fim_s, fim_pts));
    if j_out <= j_in {
        return Err(erro("intervalo"));
    }
    Ok((j_in, j_out))
}

fn ligado(c: &ClipePrincipal) -> bool {
    !base(c).desligado
}

fn base(c: &ClipePrincipal) -> &super::modelo::ClipeBase {
    match c {
        ClipePrincipal::Trecho(x) => &x.base,
        ClipePrincipal::Congelar(x) => &x.base,
        ClipePrincipal::Imagem(x) => &x.base,
        ClipePrincipal::Titulo(x) => &x.base,
        ClipePrincipal::Tela(x) => &x.base,
    }
}

/// Clipes ligados e conhecidos da principal, na ordem.
fn ligados(p: &ProjetoEdicao) -> impl Iterator<Item = &ClipePrincipal> {
    p.principal.iter().filter_map(|c| match c {
        Tolerante::Conhecido(c) if ligado(c) => Some(c),
        _ => None,
    })
}

/// Tempo preso a quadro: (segundos, pts).
type Par = (f64, Option<i64>);

/// Os tempos de fonte de um clipe com trecho: (mídia, início, fim, velocidade).
fn trecho_de(c: &ClipePrincipal) -> Option<(&str, Par, Par, &Velocidade)> {
    match c {
        ClipePrincipal::Trecho(t) => Some((&t.midia, (t.inicio_s, t.inicio_pts), (t.fim_s, t.fim_pts), &t.velocidade)),
        // a tela segue a célula de referência
        ClipePrincipal::Tela(t) => {
            let cel = t.celulas.get(t.referencia)?;
            Some((&cel.midia, (t.inicio_s, t.inicio_pts), (t.fim_s, t.fim_pts), &t.velocidade))
        }
        _ => None,
    }
}

/// Slots e faixa de fonte de um clipe da principal. Sem mídia, sem tabela ou com faixa inválida:
/// slots estimados pelos segundos e `fonte` None (a validação acusa).
fn medir(c: &ClipePrincipal, ctx: &Contexto, fps: Racional) -> (u32, Option<FaixaFonte>) {
    if let Some((midia, ini, fim, v)) = trecho_de(c) {
        let estimado = || {
            let fator = match v {
                Velocidade::Constante { fator } if fator.is_finite() && *fator > 0.0 => *fator,
                _ => 1.0,
            };
            n_q_sem_fonte((fim.0 - ini.0) / fator, fps)
        };
        let Some(tab) = ctx.fontes.get(midia).map(|f| &f.quadros) else { return (estimado(), None) };
        let mapa = faixa_do_trecho(tab, ini.0, ini.1, fim.0, fim.1).and_then(|(a, b)| mapa_trecho(tab, a, b, v, fps));
        return match mapa {
            Ok(m) => {
                let fonte = FaixaFonte {
                    midia: midia.to_string(),
                    j_in: m.j_in as u32,
                    j_out: m.j_out as u32,
                    pts_in: tab.pts[m.j_in],
                    pts_out: tab.pts.get(m.j_out).copied(),
                    lidos: (m.j_out - m.j_in) as u32,
                    repetidos: m.repetidos,
                    descartados: m.descartados,
                    tabela: tab.origem,
                };
                (m.n_q, Some(fonte))
            }
            Err(_) => (estimado(), None),
        };
    }
    match c {
        ClipePrincipal::Congelar(g) => {
            let n_q = n_q_sem_fonte(g.duracao_s, fps);
            let fonte = ctx.fontes.get(&g.midia).map(|f| &f.quadros).filter(|t| !t.pts.is_empty()).map(|tab| {
                let j = tab.do_par(g.instante_s, g.instante_pts);
                FaixaFonte {
                    midia: g.midia.clone(),
                    j_in: j as u32,
                    j_out: j as u32 + 1,
                    pts_in: tab.pts[j],
                    pts_out: tab.pts.get(j + 1).copied(),
                    lidos: 1,
                    repetidos: n_q - 1,
                    descartados: 0,
                    tabela: tab.origem,
                }
            });
            (n_q, fonte)
        }
        ClipePrincipal::Imagem(i) => (n_q_sem_fonte(i.duracao_s, fps), None),
        ClipePrincipal::Titulo(t) => (n_q_sem_fonte(t.duracao_s, fps), None),
        ClipePrincipal::Trecho(_) | ClipePrincipal::Tela(_) => (1, None),
    }
}

/// Linha do tempo em slots: clipes da principal (com a faixa de fonte) e clipes livres.
/// Entram só os clipes ligados e conhecidos; clipe livre preso a clipe que não está na linha fica de fora.
pub fn resolver(p: &ProjetoEdicao, ctx: &Contexto) -> LinhaResolvida {
    let fps = p.saida.fps;
    let mut linha = LinhaResolvida { fps, total_q: 0, clipes: Vec::new(), livres: Vec::new() };
    if fps.num <= 0 || fps.den <= 0 {
        return linha;
    }
    let (mut fim, mut n_anterior) = (0u64, 0u32);
    for c in ligados(p) {
        let (n_q, fonte) = medir(c, ctx, fps);
        let b = base(c);
        // a transição nunca passa do clipe dela nem do anterior (o excesso é erro `transicao` na validação)
        let sobrepoe_q = match &b.transicao_entrada {
            Some(t) if !linha.clipes.is_empty() && transicao_sobrepoe(&t.tipo) => {
                slots(t.duracao_s, fps).min(n_q as u64).min(n_anterior as u64) as u32
            }
            _ => 0,
        };
        let inicio_q = fim - sobrepoe_q as u64;
        linha.clipes.push(ClipeResolvido { id: b.id.clone(), inicio_q, n_q, sobrepoe_q, fonte });
        fim = inicio_q + n_q as u64;
        n_anterior = n_q;
    }
    linha.total_q = fim;

    let por_id: HashMap<&str, (u64, u32)> = linha.clipes.iter().map(|c| (c.id.as_str(), (c.inicio_q, c.n_q))).collect();
    // (início, fim natural): do programa, ou do clipe-âncora
    let lugar = |pos: &Posicao| -> Option<(u64, u64)> {
        match pos {
            Posicao::Programa { t } => Some((slots(*t, fps), linha.total_q)),
            Posicao::Clipe { clipe, desloc } => {
                let (a, n) = *por_id.get(clipe.as_str())?;
                let d = (desloc * fps.num as f64 / fps.den as f64).round();
                let inicio = if d.is_finite() { (a as f64 + d).max(0.0) as u64 } else { a };
                Some((inicio, a + n as u64))
            }
        }
    };
    let livre = |id: &str, trilha: &str, pos: &Posicao, duracao_s: Option<f64>| -> Option<LivreResolvido> {
        let (inicio_q, fim) = lugar(pos)?;
        let n_q = match duracao_s {
            Some(d) => slots(d, fps).clamp(1, u32::MAX as u64),
            None => fim.saturating_sub(inicio_q).min(u32::MAX as u64),
        } as u32;
        Some(LivreResolvido { id: id.to_string(), trilha: trilha.to_string(), inicio_q, n_q })
    };
    let mut livres = Vec::new();
    for t in &p.sobreposicoes {
        for c in &t.clipes {
            let Tolerante::Conhecido(c) = c else { continue };
            let b = match c {
                ClipeSobreposto::Video(v) => &v.base,
                ClipeSobreposto::Imagem(i) => &i.base,
                ClipeSobreposto::Titulo(x) => &x.base,
            };
            livres.extend(livre(&b.id, &t.id, &b.pos, b.duracao_s));
        }
    }
    for t in &p.audios {
        for c in &t.clipes {
            // sem `ate_s`: até o fim do arquivo (duração do retrato da mídia)
            let fim_arquivo = || p.midias.iter().find(|m| m.id == c.midia).and_then(|m| m.duracao_s);
            let duracao = c.ate_s.or_else(fim_arquivo).map(|ate| ate - c.de_s);
            livres.extend(livre(&c.id, &t.id, &c.pos, duracao));
        }
    }
    linha.livres = livres;
    linha
}

/// A transição sobrepõe os dois clipes (encurta o programa)? Só `escurecer` não sobrepõe.
pub fn transicao_sobrepoe(tipo: &str) -> bool {
    tipo != "escurecer"
}

/// Fator decimal (até 3 casas) como fração exata por milésimos. Erro `velocidade` fora de 0,05..16.
pub fn fator_racional(fator: f64) -> Result<Racional> {
    let mil = (fator * 1000.0).round();
    if !fator.is_finite() || (fator * 1000.0 - mil).abs() > 1e-6 || !(50.0..=16000.0).contains(&mil) {
        return Err(erro("velocidade"));
    }
    let mil = mil as i128;
    let g = mdc(mil, 1000);
    Ok(Racional { num: (mil / g) as i64, den: (1000 / g) as i64 })
}

/// round(duracao_s·fps), na ordem (d·num)/den; não finito ou negativo = 0.
fn arredondar(duracao_s: f64, fps: Racional) -> f64 {
    let x = (duracao_s * fps.num as f64 / fps.den as f64).round();
    if x.is_finite() && x > 0.0 {
        x
    } else {
        0.0
    }
}

/// Slots de um clipe sem fonte: max(1, round(duracao_s·fps)).
pub fn n_q_sem_fonte(duracao_s: f64, fps: Racional) -> u32 {
    arredondar(duracao_s, fps).clamp(1.0, u32::MAX as f64) as u32
}

/// Slots de uma duração: round(duracao_s·fps) (transições, deslocamentos).
pub fn slots(duracao_s: f64, fps: Racional) -> u64 {
    arredondar(duracao_s, fps) as u64
}

/// Instante do slot q em segundos: q·den/num.
pub fn instante_do_slot(q: u64, fps: Racional) -> f64 {
    q as f64 * fps.den as f64 / fps.num as f64
}

/// Fronteira de amostra do slot q a 48 kHz: round(q·48000·den/num).
pub fn amostra_do_slot(q: u64, fps: Racional) -> u64 {
    let (num, den) = (fps.num.max(1) as u128, fps.den.max(1) as u128);
    ((2 * q as u128 * TAXA_AUDIO as u128 * den + num) / (2 * num)) as u64
}

/// Slots do clipe em que o quadro j aparece: (primeiro slot local, quantos); None = descartado ou fora.
pub fn slots_do_quadro(mapa: &MapaQuadros, j: usize) -> Option<(u32, u32)> {
    let j = u32::try_from(j).ok()?;
    let a = mapa.quadro_do_slot.partition_point(|&x| x < j);
    let b = mapa.quadro_do_slot.partition_point(|&x| x <= j);
    (b > a).then_some((a as u32, (b - a) as u32))
}

/// Clipe da principal que ocupa o slot `q` do programa (na sobreposição de uma transição, o que entra).
// CONTRATO: acréscimo (conversão programa → fonte).
pub fn clipe_no_slot(linha: &LinhaResolvida, q: u64) -> Option<usize> {
    linha.clipes.iter().rposition(|c| c.inicio_q <= q && q < c.inicio_q + c.n_q as u64)
}

/// Quadro da fonte (índice na tabela) mostrado no slot `q` do programa por um clipe com mapa.
// CONTRATO: acréscimo (conversão programa → fonte).
pub fn quadro_no_slot(clipe: &ClipeResolvido, mapa: &MapaQuadros, q: u64) -> Option<usize> {
    let local = usize::try_from(q.checked_sub(clipe.inicio_q)?).ok()?;
    mapa.quadro_do_slot.get(local).map(|&j| j as usize)
}

/// Slots do programa em que o clipe mostra o quadro j da fonte: (primeiro, quantos).
// CONTRATO: acréscimo (conversão fonte → programa).
pub fn slots_no_programa(clipe: &ClipeResolvido, mapa: &MapaQuadros, j: usize) -> Option<(u64, u32)> {
    slots_do_quadro(mapa, j).map(|(a, n)| (clipe.inicio_q + a as u64, n))
}

/// `Prender` quando o par gravado (t, pts) não é o do quadro `j` da tabela.
fn conferir_par(v: &mut Vec<Prender>, onde: OndePrender, tab: &TabelaQuadros, j: usize, t: f64, pts: Option<i64>) {
    let (certo_pts, certo_t) = if j < tab.pts.len() { (Some(tab.pts[j]), tab.tempo(j)) } else { (None, tab.fim_s()) };
    let mesmo_t = (t - certo_t).abs() <= 1e-9;
    if pts != certo_pts || !mesmo_t {
        v.push(Prender { onde, pts: certo_pts, t: certo_t });
    }
}

/// Tempos presos a quadro que faltam ou não batem com a tabela: o front grava cada um por `alterar()`.
pub fn prender(p: &ProjetoEdicao, ctx: &Contexto) -> Vec<Prender> {
    let mut v = Vec::new();
    let tabela = |midia: &str| ctx.fontes.get(midia).filter(|f| crate::edicao::previa::marcavel(f)).map(|f| &*f.quadros).filter(|t| !t.pts.is_empty());
    let analise = |v: &mut Vec<Prender>, clipe: &str, efeitos: &[Tolerante<Efeito>], tab: &TabelaQuadros| {
        for e in efeitos {
            let Tolerante::Conhecido(Efeito::Estabilizar(e)) = e else { continue };
            let a = &e.analise;
            let (de, ate) = (tab.do_par(a.de_s, Some(a.de_pts)), tab.do_par(a.ate_s, Some(a.ate_pts)));
            conferir_par(v, OndePrender::AnaliseDe { clipe: clipe.into(), efeito: e.base.id.clone() }, tab, de, a.de_s, Some(a.de_pts));
            conferir_par(v, OndePrender::AnaliseAte { clipe: clipe.into(), efeito: e.base.id.clone() }, tab, ate, a.ate_s, Some(a.ate_pts));
        }
    };
    for c in &p.principal {
        let Tolerante::Conhecido(c) = c else { continue };
        let id = &base(c).id;
        if let Some((midia, ini, fim, _)) = trecho_de(c) {
            let Some(tab) = tabela(midia) else { continue };
            conferir_par(&mut v, OndePrender::Inicio { clipe: id.clone() }, tab, tab.do_par(ini.0, ini.1), ini.0, ini.1);
            conferir_par(&mut v, OndePrender::Fim { clipe: id.clone() }, tab, tab.fim_do_par(fim.0, fim.1), fim.0, fim.1);
            analise(&mut v, id, &base(c).efeitos, tab);
        } else if let ClipePrincipal::Congelar(g) = c {
            let Some(tab) = tabela(&g.midia) else { continue };
            conferir_par(&mut v, OndePrender::Instante { clipe: id.clone() }, tab, tab.do_par(g.instante_s, g.instante_pts), g.instante_s, g.instante_pts);
        }
    }
    for t in &p.sobreposicoes {
        for c in &t.clipes {
            let Tolerante::Conhecido(ClipeSobreposto::Video(s)) = c else { continue };
            let Some(tab) = tabela(&s.midia) else { continue };
            conferir_par(&mut v, OndePrender::Inicio { clipe: s.base.id.clone() }, tab, tab.do_par(s.inicio_s, s.inicio_pts), s.inicio_s, s.inicio_pts);
            analise(&mut v, &s.base.id, &s.base.efeitos, tab);
        }
    }
    for r in &p.rastreios {
        let (BaseRastreio::Fonte, Some(midia)) = (r.base, &r.midia) else { continue };
        let Some(tab) = tabela(midia) else { continue };
        for (i, a) in r.ancoras.iter().enumerate() {
            conferir_par(&mut v, OndePrender::Ancora { rastreio: r.id.clone(), indice: i as u32 }, tab, tab.do_par(a.t, a.pts), a.t, a.pts);
        }
    }
    v
}

/// Maior altura da saída automática (px).
const ALTURA_AUTO: u32 = 1080;

/// (largura, altura, fps) da saída automática (§8.1): proporção da mídia com mais tempo no programa, maior altura
/// até 1080 e a maior `taxa_nominal` até 60. None quando `saida.auto` é false ou não há mídia.
pub fn saida_auto(p: &ProjetoEdicao, ctx: &Contexto) -> Option<(u32, u32, Racional)> {
    if !p.saida.auto {
        return None;
    }
    let linha = resolver(p, ctx);
    // slots de cada mídia com imagem, na ordem em que aparecem
    let mut tempo: Vec<(&str, u64)> = Vec::new();
    let mut taxa: Option<Racional> = None;
    let mut altura = 0u32;
    for (c, r) in ligados(p).zip(&linha.clipes) {
        let (midia, video) = match c {
            ClipePrincipal::Trecho(_) | ClipePrincipal::Tela(_) => (trecho_de(c).map(|t| t.0), true),
            ClipePrincipal::Congelar(g) => (Some(g.midia.as_str()), true),
            ClipePrincipal::Imagem(i) => (Some(i.midia.as_str()), false),
            ClipePrincipal::Titulo(_) => (None, false),
        };
        let Some(f) = midia.and_then(|m| ctx.fontes.get(m)).filter(|f| f.exibido.0 > 0 && f.exibido.1 > 0) else { continue };
        let midia = midia.unwrap_or_default();
        match tempo.iter_mut().find(|(m, _)| *m == midia) {
            Some((_, n)) => *n += r.n_q as u64,
            None => tempo.push((midia, r.n_q as u64)),
        }
        altura = altura.max(f.exibido.1);
        if video && f.quadros.pts.len() > 1 {
            let t = f.quadros.taxa_nominal();
            if taxa.is_none_or(|a| (t.num as i128 * a.den as i128) > (a.num as i128 * t.den as i128)) {
                taxa = Some(t);
            }
        }
    }
    // a primeira entre as de maior tempo
    let mut dona: Option<(&str, u64)> = None;
    for &(m, n) in &tempo {
        if dona.is_none_or(|(_, maior)| n > maior) {
            dona = Some((m, n));
        }
    }
    let (w, h) = ctx.fontes.get(dona?.0)?.exibido;
    let par = |v: f64| ((v / 2.0).round() as u32 * 2).max(64);
    let altura = par(altura.min(ALTURA_AUTO) as f64);
    let largura = par(altura as f64 * w as f64 / h as f64);
    Some((largura, altura, taxa.unwrap_or(p.saida.fps)))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::curva::fixtures;
    use crate::edicao::ffmpeg::sonda::Capacidades;
    use crate::edicao::motor::FonteInfo;
    use crate::edicao::quadros::apoio::{self, SemRastreios};
    use serde_json::{json, Value};
    use std::path::Path;

    fn r(num: i64, den: i64) -> Racional {
        Racional { num, den }
    }
    fn cte(fator: f64) -> Velocidade {
        Velocidade::Constante { fator }
    }
    fn codigo<T: std::fmt::Debug>(x: Result<T>) -> String {
        match x {
            Err(SicroError::Validation(c)) => c,
            outro => panic!("esperava erro de validação: {outro:?}"),
        }
    }

    /// Sorteio com semente fixa.
    struct Sorteio(u64);
    impl Sorteio {
        fn prox(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn ate(&mut self, n: usize) -> usize {
            (self.prox() % n.max(1) as u64) as usize
        }
    }

    const VELOCIDADES: &[f64] = &[0.05, 0.1, 0.25, 0.5, 0.75, 1.0, 1.25, 2.0, 4.0, 8.0, 16.0];
    const TAXAS: &[(i64, i64)] = &[(25, 2), (15, 1), (24000, 1001), (25, 1), (30000, 1001), (30, 1), (50, 1), (60000, 1001), (60, 1)];

    // ---------------------------------------------------------------- a3/t9-mapa.py

    /// round(p/q) como o do Python: metade para o par.
    fn arredonda_par(p: i128, q: i128) -> i64 {
        let (f, resto) = (p.div_euclid(q), p.rem_euclid(q));
        (f + i128::from(2 * resto > q || (2 * resto == q && f % 2 != 0))) as i64
    }

    type Fracao = (i64, i64);

    /// Os 7 casos de `a3/t9-mapa.py`: (nome, pts, tb, fps). `ruido` em milésimos de segundo.
    fn casos_t9() -> Vec<(&'static str, Vec<i64>, Fracao, Fracao)> {
        let fonte = |n: i128, passo: (i128, i128), tb: (i128, i128), ruido: fn(i128) -> i128| -> Vec<i64> {
            (0..=n).map(|i| arredonda_par((i * passo.0 * 1000 + ruido(i) * passo.1) * tb.1, passo.1 * 1000 * tb.0)).collect()
        };
        let zero: fn(i128) -> i128 = |_| 0;
        vec![
            ("MKV 1 ms a 30 qps", fonte(90, (1, 30), (1, 1000), zero), (1, 1000), (30, 1)),
            ("MP4 escala 1000 a 15 qps", fonte(90, (1, 15), (1, 1000), zero), (1, 1000), (15, 1)),
            ("MKV 1 ms a 23,976", fonte(90, (1001, 24000), (1, 1000), zero), (1, 1000), (24000, 1001)),
            ("MP4 1/15360 a 30 qps", fonte(90, (1, 30), (1, 15360), zero), (1, 15360), (30, 1)),
            ("MKV 1 ms com oscilacao de 4 ms", fonte(90, (1, 30), (1, 1000), |i| (i * 7) % 9 - 4), (1, 1000), (30, 1)),
            ("AVI 1/25 em projeto de 30", fonte(50, (1, 25), (1, 25), zero), (1, 25), (30, 1)),
            ("MP4 1/15360 30 qps em projeto 30000/1001", fonte(90, (1, 30), (1, 15360), zero), (1, 15360), (30000, 1001)),
        ]
    }

    const V_T9: &[(&str, f64)] = &[("1", 1.0), ("1/2", 0.5), ("1/4", 0.25), ("4", 4.0)];

    /// As linhas de `a3/out/t9-mapa.log`, refeitas pelo Rust.
    fn linhas_t9() -> Vec<String> {
        let mut l = Vec::new();
        for (nome, pts, tb, fps) in casos_t9() {
            let lidos = pts.len() - 1;
            let tab = apoio::tabela(tb, pts);
            for &(txt, v) in V_T9 {
                let m = mapa_trecho(&tab, 0, lidos, &cte(v), r(fps.0, fps.1)).unwrap();
                let obs = if v < 1.0 && !nome.contains("projeto") {
                    let mut vezes: Vec<u32> = (0..lidos).map(|j| slots_do_quadro(&m, j).map_or(0, |s| s.1)).collect();
                    vezes.sort_unstable();
                    vezes.dedup();
                    format!("slots por quadro: {vezes:?}")
                } else if v == 4.0 && !nome.contains("projeto") {
                    format!("primeiros quadros: {:?}", &m.quadro_do_slot[..4])
                } else {
                    String::new()
                };
                let slot0 = if m.quadro_do_slot[0] == 0 { "True" } else { "False" };
                l.push(format!("{nome:42} {txt:>5} {lidos:5} {:4} {:4} {:4}  {slot0:10} {obs}", m.n_q, m.repetidos, m.descartados));
            }
        }
        l
    }

    #[test]
    fn mapa_igual_ao_do_t9() {
        let linhas = linhas_t9();
        // os números do log, para o teste valer também sem o dossiê
        let resumo: Vec<String> = linhas.iter().map(|l| l[42..].split_whitespace().take(5).collect::<Vec<_>>().join(" ")).collect();
        let comum = ["1 90 90 0 0", "1/2 90 180 90 0", "1/4 90 360 270 0", "4 90 23 0 67"];
        let avi = ["1 50 60 10 0", "1/2 50 120 70 0", "1/4 50 240 190 0", "4 50 15 0 35"];
        for (i, bloco) in resumo.chunks(4).enumerate() {
            assert_eq!(bloco, if i == 5 { avi } else { comum }, "caso {i}");
        }
        assert!(linhas[1].ends_with("slots por quadro: [2]") && linhas[2].ends_with("slots por quadro: [4]"));
        assert!(linhas[18].ends_with("slots por quadro: [3, 4, 5]") && linhas[3].ends_with("primeiros quadros: [0, 4, 8, 12]"));
        let Some(dossie) = crate::edicao::testes::fontes::dossie() else {
            eprintln!("aviso: sem o dossiê, t9-mapa.log não conferido");
            return;
        };
        let log = std::fs::read_to_string(dossie.join("a3/out/t9-mapa.log")).expect("t9-mapa.log");
        let do_log: Vec<&str> = log.lines().skip(1).filter(|l| !l.starts_with("maior produto")).map(str::trim_end).collect();
        assert_eq!(do_log, linhas.iter().map(|l| l.trim_end()).collect::<Vec<_>>());
    }

    /// Roda o `mapa()` do próprio `a3/t9-mapa.py` e compara o mapa inteiro (constantes e quadro de cada slot).
    #[test]
    fn mapa_igual_ao_do_t9_rodando_o_python() {
        let script = crate::edicao::testes::fontes::dossie().map(|d| d.join("a3/t9-mapa.py")).filter(|p| p.is_file());
        let (Some(script), Ok(python)) = (script, which::which("python3")) else {
            eprintln!("aviso: sem python3 ou sem o dossiê, t9-mapa.py não rodou");
            return;
        };
        let condutor = r#"
import json, sys
src = open(sys.argv[1]).read()
exec(src[:src.index("print(f\"{'caso'")])
saida = []
for nome, pts, tb, fps in casos:
    for v in (F(1), F(1, 2), F(1, 4), F(4)):
        slot, n_q, quadro, rep, desc, k = mapa(pts, tb, fps, v, 0, len(pts) - 1)
        saida.append(dict(nome=nome, pts=pts, n_q=n_q, quadro=quadro, rep=rep, desc=desc, k=list(k)))
print(json.dumps(saida))
"#;
        let out = crate::tools::command(python).args(["-I", "-c", condutor]).arg(&script).current_dir(std::env::temp_dir()).output().expect("python3");
        assert!(out.status.success(), "t9-mapa.py: {}", String::from_utf8_lossy(&out.stderr));
        let do_python: Vec<Value> = serde_json::from_slice(&out.stdout).expect("json do t9");
        let mut i = 0;
        for (nome, pts, tb, fps) in casos_t9() {
            let lidos = pts.len() - 1;
            let tab = apoio::tabela(tb, pts.clone());
            for &(_, v) in V_T9 {
                let m = mapa_trecho(&tab, 0, lidos, &cte(v), r(fps.0, fps.1)).unwrap();
                let p = &do_python[i];
                assert_eq!(p["nome"], nome);
                assert_eq!(p["pts"], json!(pts), "{nome}: pts");
                assert_eq!(p["k"], json!([m.c, m.r, m.m, m.kn, m.kd, m.e]), "{nome} {v}: c, R, M, Kn, Kd, E");
                assert_eq!(p["n_q"], json!(m.n_q), "{nome} {v}");
                assert_eq!(p["quadro"], json!(m.quadro_do_slot), "{nome} {v}: quadro de cada slot");
                assert_eq!((p["rep"].clone(), p["desc"].clone()), (json!(m.repetidos), json!(m.descartados)), "{nome} {v}");
                i += 1;
            }
        }
        assert_eq!(i, do_python.len());
    }

    // ---------------------------------------------------------------- propriedades do mapa

    #[test]
    fn passo_de_1_ms_na_propria_taxa_e_em_camera_lenta() {
        for fps in [12, 15, 30] {
            let tab = apoio::tabela((1, 1000), apoio::pts_regulares(200, (1, fps), (1, 1000), 0));
            // todo começo de trecho: a fase do arredondamento muda com o corte
            for j_in in 0..40 {
                let j_out = j_in + 120;
                let m = mapa_trecho(&tab, j_in, j_out, &cte(1.0), r(fps, 1)).unwrap();
                assert_eq!((m.n_q, m.repetidos, m.descartados), (120, 0, 0), "{fps} qps, corte em {j_in}");
                for (v, vezes) in [(0.5, 2), (0.25, 4)] {
                    let m = mapa_trecho(&tab, j_in, j_out, &cte(v), r(fps, 1)).unwrap();
                    assert_eq!(m.descartados, 0);
                    for j in j_in..j_out {
                        assert_eq!(slots_do_quadro(&m, j).map(|s| s.1), Some(vezes), "{fps} qps a {v}×, corte em {j_in}, quadro {j}");
                    }
                }
            }
        }
    }

    /// slot(j) do §3.1 escrito por extenso, para conferir o mapa pela definição.
    fn slot_pela_definicao(tab: &TabelaQuadros, j_in: usize, j_out: usize, v: Racional, fps: Racional) -> (Vec<i128>, i128) {
        let mmc = |a: i128, b: i128| a / mdc(a, b) * b;
        let (tn, td, fnum, fden) = (tab.tb.num as i128, tab.tb.den as i128, fps.num as i128, fps.den as i128);
        let c = mmc(td, fnum);
        let rr = tn * c / td;
        let (kn, kd) = (fnum * v.den as i128, fden * v.num as i128 * c);
        let (kn, kd) = (kn / mdc(kn, kd), kd / mdc(kn, kd));
        let n = tab.pts.len();
        let mut p: Vec<i128> = tab.pts[j_in..j_out.min(n)].iter().map(|&x| x as i128 * rr).collect();
        p.push(if j_out < n { tab.pts[j_out] as i128 * rr } else { (tab.pts[n - 1] + tab.passo_mediano) as i128 * rr });
        let pmin = p.windows(2).map(|w| w[1] - w[0]).filter(|d| *d > 0).min().unwrap();
        let e = kd.min(pmin * kn) / 2;
        let teto = |a: i128| -((-a).div_euclid(kd));
        let slots = p[..p.len() - 1].iter().map(|x| teto((x - p[0]) * kn - e)).collect();
        (slots, teto((p[p.len() - 1] - p[0]) * kn - e).max(1))
    }

    #[test]
    fn mapa_pela_definicao_em_trechos_sorteados() {
        let mut s = Sorteio(20261114);
        let bases: &[(Fracao, Fracao)] = &[
            ((1, 15360), (1, 30)),
            ((1, 1000), (1, 30)),
            ((1, 1000), (1001, 24000)),
            ((1, 30), (1, 30)),
            ((1, 25), (1, 25)),
            ((1, 90000), (1001, 60000)),
            ((1, 600), (1, 15)),
            ((1001, 30000), (1001, 30000)),
            ((1, 12800), (1, 25)),
            ((1, 1000), (2, 25)),
        ];
        let mut casos = 0;
        for &(tb, passo) in bases {
            // regular, com oscilação e com quadros faltando; uma começa em época (1,5e9 s)
            for tipo in 0..4 {
                let inicio = if tipo == 3 { 1_500_000_000 * tb.1 / tb.0 } else { 0 };
                let mut pts = apoio::pts_regulares(400, passo, tb, inicio);
                let amplitude = (tb.1 / (250 * tb.0)).max(1);
                if tipo == 1 && 4 * amplitude < passo.0 * tb.1 / (passo.1 * tb.0) {
                    for (i, p) in pts.iter_mut().enumerate().skip(1) {
                        *p += (i as i64 * 7) % (2 * amplitude + 1) - amplitude;
                    }
                }
                if tipo == 2 {
                    pts = pts.into_iter().enumerate().filter(|(i, _)| i % 7 != 3 && i % 11 != 5 && !(100..=112).contains(i)).map(|x| x.1).collect();
                }
                let tab = apoio::tabela(tb, pts);
                let n = tab.pts.len();
                for _ in 0..60 {
                    let v = fator_racional(VELOCIDADES[s.ate(VELOCIDADES.len())]).unwrap();
                    let fps = TAXAS[s.ate(TAXAS.len())];
                    let fps = r(fps.0, fps.1);
                    let lidos = 1 + s.ate(150);
                    let j_in = s.ate(n - lidos + 1);
                    let j_out = j_in + lidos;
                    let m = mapa_racional(&tab, j_in, j_out, v, fps).unwrap();
                    let (slots, n_q) = slot_pela_definicao(&tab, j_in, j_out, v, fps);
                    assert_eq!(m.n_q as i128, n_q);
                    assert_eq!(slots[0], 0);
                    assert_eq!(m.quadro_do_slot[0] as usize, j_in, "o slot 0 mostra j_in");
                    for (q, &j) in m.quadro_do_slot.iter().enumerate() {
                        let maior = (0..lidos).rev().find(|&k| slots[k] <= q as i128).unwrap() + j_in;
                        assert_eq!(j as usize, maior, "tb {tb:?} tipo {tipo} v {v:?} fps {fps:?} [{j_in}, {j_out}) slot {q}");
                    }
                    let distintos = slots.iter().enumerate().filter(|&(k, &a)| a < n_q && slots.get(k + 1).is_none_or(|&b| b > a)).count() as u32;
                    assert_eq!((m.repetidos, m.descartados), (m.n_q - distintos, lidos as u32 - distintos));
                    assert_eq!(descartados_a_1x(&tab, fps).unwrap(), mapa_racional(&tab, 0, n, r(1, 1), fps).unwrap().descartados as u64);
                    casos += 1;
                }
            }
        }
        assert_eq!(casos, 2400);
    }

    #[test]
    fn aceleracao_mostra_o_quadro_exibido() {
        let tab = apoio::tabela((1, 1000), apoio::pts_regulares(200, (1, 30), (1, 1000), 0));
        let m = mapa_trecho(&tab, 0, 120, &cte(4.0), r(30, 1)).unwrap();
        assert_eq!(&m.quadro_do_slot[..5], &[0, 4, 8, 12, 16]);
        assert_eq!((m.n_q, m.repetidos, m.descartados), (30, 0, 90));
        // 60 → 30: abre no quadro de entrada, não no seguinte
        let tab = apoio::tabela((1, 15360), apoio::pts_regulares(200, (1, 60), (1, 15360), 0));
        let m = mapa_trecho(&tab, 7, 127, &cte(1.0), r(30, 1)).unwrap();
        assert_eq!(&m.quadro_do_slot[..4], &[7, 9, 11, 13]);
        assert_eq!((m.c, m.kn, m.kd, m.e, m.n_q, m.descartados), (15360, 1, 512, 128, 60, 60));
    }

    #[test]
    fn constantes_dos_ensaios_do_motor() {
        // cm/out2/10-mapa-inteiro.log: c, Kn/Kd e E medidos nos dois ffmpeg
        let casos: &[(Fracao, Fracao, f64, Fracao, [i64; 4])] = &[
            ((1, 15360), (1, 30), 1.0, (30, 1), [15360, 1, 512, 256]),
            ((1, 15360), (1, 30), 4.0, (30, 1), [15360, 1, 2048, 256]),
            ((1, 15360), (1, 30), 1.5, (30000, 1001), [1920000, 1, 96096, 32000]),
            ((1, 15360), (1, 30), 0.37, (30, 1), [15360, 25, 4736, 2368]),
            ((1, 1000), (1, 30), 1.0, (30, 1), [3000, 1, 100, 49]),
            ((1, 1000), (1, 30), 0.05, (30, 1), [3000, 1, 5, 2]),
            ((1, 30), (1, 30), 1.0, (25, 1), [150, 1, 6, 2]),
            ((1, 30), (1, 30), 0.5, (30, 1), [30, 2, 1, 0]),
            ((1, 25), (1, 25), 1.0, (30000, 1001), [30000, 1, 1001, 500]),
            ((1, 25), (1, 25), 0.25, (30, 1), [150, 4, 5, 2]),
        ];
        for &(tb, passo, v, fps, [c, kn, kd, e]) in casos {
            let tab = apoio::tabela(tb, apoio::pts_regulares(200, passo, tb, 0));
            let m = mapa_trecho(&tab, 30, 90, &cte(v), r(fps.0, fps.1)).unwrap();
            assert_eq!((m.c, m.kn, m.kd, m.e), (c, kn, kd, e), "tb {tb:?} v {v} fps {fps:?}");
        }
    }

    #[test]
    fn fim_da_midia_usa_o_passo_mediano() {
        let tab = apoio::tabela((1, 15360), apoio::pts_regulares(90, (1, 30), (1, 15360), 0));
        let m = mapa_trecho(&tab, 60, 90, &cte(1.0), r(30, 1)).unwrap();
        assert_eq!((m.n_q, m.repetidos, m.descartados, m.quadro_do_slot[29]), (30, 0, 0, 89));
        // vizinhos [a, b) e [b, c) não repetem nem pulam
        let a = mapa_trecho(&tab, 10, 40, &cte(1.0), r(30, 1)).unwrap();
        let b = mapa_trecho(&tab, 40, 70, &cte(1.0), r(30, 1)).unwrap();
        assert_eq!((a.quadro_do_slot.last(), b.quadro_do_slot.first()), (Some(&39), Some(&40)));
    }

    #[test]
    fn pts_repetido_nao_zera_a_folga() {
        let mut pts = apoio::pts_regulares(120, (1, 30), (1, 1000), 0);
        pts[50] = pts[49];
        let tab = apoio::tabela((1, 1000), pts);
        let m = mapa_trecho(&tab, 0, 100, &cte(1.0), r(30, 1)).unwrap();
        // some só o primeiro do par repetido
        assert_eq!((m.e, m.descartados, m.repetidos), (49, 1, 1));
        assert_eq!(slots_do_quadro(&m, 49), None);
        // ponta de trecho no segundo quadro do pts repetido: o `trim` por pts leria os dois
        assert_eq!(codigo(mapa_trecho(&tab, 50, 100, &cte(1.0), r(30, 1))), "intervalo");
        assert_eq!(codigo(mapa_trecho(&tab, 0, 50, &cte(1.0), r(30, 1))), "intervalo");
        // pelo par (segundos, pts) a ponta cai no primeiro
        assert_eq!(faixa_do_trecho(&tab, tab.tempo(50), None, tab.tempo(60), None).unwrap(), (49, 60));
        assert_eq!(faixa_do_trecho(&tab, 0.0, None, tab.tempo(50), Some(tab.pts[50])).unwrap(), (0, 49));
    }

    #[test]
    fn erros_do_mapa() {
        let tab = apoio::tabela((1, 1000), apoio::pts_regulares(100, (1, 30), (1, 1000), 0));
        assert_eq!(codigo(mapa_trecho(&tab, 10, 10, &cte(1.0), r(30, 1))), "intervalo");
        assert_eq!(codigo(mapa_trecho(&tab, 10, 101, &cte(1.0), r(30, 1))), "intervalo");
        assert_eq!(codigo(mapa_trecho(&tab, 0, 10, &cte(1.0), r(0, 1))), "intervalo");
        assert_eq!(codigo(mapa_trecho(&tab, 0, 10, &cte(0.01), r(30, 1))), "velocidade");
        assert_eq!(codigo(mapa_trecho(&tab, 0, 10, &cte(1.2345), r(30, 1))), "velocidade");
        assert_eq!(codigo(mapa_trecho(&tab, 0, 10, &Velocidade::Rampa { pontos: Vec::new() }, r(30, 1))), "nao_suportado: rampa");
        let vazia = apoio::tabela((1, 1000), Vec::new());
        assert_eq!(codigo(mapa_trecho(&vazia, 0, 1, &cte(1.0), r(30, 1))), "intervalo");
        // produto acima de 2^53: base de tempo que não casa com a taxa
        let grossa = apoio::tabela((1, 1_000_000_007), apoio::pts_regulares(100, (60, 1), (1, 1_000_000_007), 0));
        assert_eq!(codigo(mapa_trecho(&grossa, 0, 90, &cte(1.0), r(60000, 1001))), "mapa_grande");
        // pts em época numa base de 0,1 µs: P passa de 2^53 antes de subtrair
        let epoca = apoio::tabela((1, 10_000_000), apoio::pts_regulares(100, (1, 30), (1, 10_000_000), 17_000_000_000_000_000));
        assert_eq!(codigo(mapa_trecho(&epoca, 0, 90, &cte(1.0), r(30, 1))), "mapa_grande");
        // taxa absurda (documento estragado): erro, sem estouro do i128
        let tab30 = apoio::tabela((1, 1000), apoio::pts_regulares(100, (1, 30), (1, 1000), 0));
        for fps in [r(4_611_686_018_427_387_905, 4_611_686_018_427_387_903), r(i64::MAX, 1), r(1, i64::MAX)] {
            assert!(matches!(codigo(mapa_trecho(&tab30, 0, 90, &cte(16.0), fps)).as_str(), "mapa_grande" | "intervalo"), "{fps:?}");
            assert!(matches!(codigo(mapa_trecho(&tab, 0, 90, &cte(0.05), fps)).as_str(), "mapa_grande" | "intervalo"), "{fps:?}");
        }
        let enorme = apoio::tabela((1, 1000), vec![0, i64::MAX / 2, i64::MAX - 1]);
        assert_eq!(codigo(mapa_trecho(&enorme, 0, 3, &cte(1.0), r(30000, 1001))), "mapa_grande");
        // taxa de saída de milhões numa hora de vídeo: 3,6·10⁹ slots cabem em u32 e em 2^53, mas seriam 14 GB
        let hora = apoio::tabela((1, 1000), apoio::pts_regulares(108_000, (1, 30), (1, 1000), 0));
        assert_eq!(codigo(mapa_trecho(&hora, 0, 108_000, &cte(1.0), r(1_000_000, 1))), "mapa_grande");
        // no teto: 2^24 slots passam, um a mais não (1 ms a 1000 qps, 1 slot por tick)
        let ms = apoio::tabela((1, 1000), vec![0, 16_777_216, 16_777_217]);
        assert_eq!(mapa_trecho(&ms, 0, 1, &cte(1.0), r(1000, 1)).unwrap().n_q, 16_777_216);
        assert_eq!(codigo(mapa_trecho(&ms, 0, 2, &cte(1.0), r(1000, 1))), "mapa_grande");
        // época em 1/90000 (gravador): cabe
        let dav = apoio::tabela((1, 90000), apoio::pts_regulares(100, (1, 15), (1, 90000), 1_505_762_710 * 90000));
        let m = mapa_trecho(&dav, 0, 90, &cte(1.0), r(15, 1)).unwrap();
        assert_eq!((m.n_q, m.repetidos, m.descartados), (90, 0, 0));
    }

    #[test]
    fn velocidade_por_milesimos() {
        for (f, num, den) in [(1.0, 1, 1), (0.5, 1, 2), (0.37, 37, 100), (0.05, 1, 20), (16.0, 16, 1), (1.25, 5, 4), (0.333, 333, 1000), (1.5, 3, 2)] {
            assert_eq!(fator_racional(f).unwrap(), r(num, den), "{f}");
        }
        for f in [0.049, 16.001, 0.3333, f64::NAN, f64::INFINITY, 0.0, -1.0] {
            assert_eq!(codigo(fator_racional(f)), "velocidade", "{f}");
        }
    }

    #[test]
    fn contas_de_slot() {
        let ntsc = r(30000, 1001);
        assert_eq!((n_q_sem_fonte(0.0, ntsc), n_q_sem_fonte(5.0, ntsc), n_q_sem_fonte(-3.0, ntsc), n_q_sem_fonte(f64::NAN, ntsc)), (1, 150, 1, 1));
        assert_eq!((slots(0.5, r(30, 1)), slots(0.0, ntsc), slots(-1.0, ntsc), slots(1.0, r(25, 2))), (15, 0, 0, 13));
        assert_eq!(instante_do_slot(30, ntsc), 1.001);
        assert_eq!((amostra_do_slot(0, ntsc), amostra_do_slot(1, ntsc), amostra_do_slot(2, ntsc), amostra_do_slot(5, ntsc)), (0, 1602, 3203, 8008));
        assert_eq!((amostra_do_slot(30000, ntsc), amostra_do_slot(1, r(30, 1)), amostra_do_slot(3, r(25, 2))), (48_048_000, 1600, 11520));
        assert!(transicao_sobrepoe("dissolver") && !transicao_sobrepoe("escurecer"));
    }

    #[test]
    fn slots_de_um_quadro() {
        let tab = apoio::tabela((1, 15360), apoio::pts_regulares(100, (1, 30), (1, 15360), 0));
        let lento = mapa_trecho(&tab, 10, 20, &cte(0.5), r(30, 1)).unwrap();
        assert_eq!((slots_do_quadro(&lento, 10), slots_do_quadro(&lento, 13), slots_do_quadro(&lento, 19)), (Some((0, 2)), Some((6, 2)), Some((18, 2))));
        assert_eq!((slots_do_quadro(&lento, 9), slots_do_quadro(&lento, 20)), (None, None));
        let rapido = mapa_trecho(&tab, 10, 50, &cte(4.0), r(30, 1)).unwrap();
        assert_eq!((slots_do_quadro(&rapido, 14), slots_do_quadro(&rapido, 15)), (Some((1, 1)), None));
    }

    // ---------------------------------------------------------------- linha resolvida

    fn projeto(extra: Value) -> ProjetoEdicao {
        let mut p = json!({
            "esquema": 2, "id": "p", "titulo": "Teste", "created_at": "2026-11-14T03:00:00.000Z", "updated_at": "2026-11-14T03:00:00.000Z",
            "saida": { "largura": 1920, "altura": 1080, "fps": { "num": 30, "den": 1 }, "auto": false, "cor_fundo": "#000000", "audio": true,
                       "selos": { "velocidade": true, "ampliacao": true, "realce": true },
                       "exportacao": { "predefinicao": "juri", "codificador": "auto", "legendas": "nenhuma", "pdf": "a3", "revisado_por": "" } }
        });
        for (k, v) in extra.as_object().expect("objeto") {
            if k == "fps" || k == "auto" {
                p["saida"][k] = v.clone();
            } else {
                p[k] = v.clone();
            }
        }
        serde_json::from_value(p).expect("projeto de teste")
    }

    fn trecho(id: &str, midia: &str, de: (f64, Option<i64>), ate: (f64, Option<i64>), fator: f64, transicao: Value) -> Value {
        json!({ "tipo": "trecho", "id": id, "midia": midia, "inicio_s": de.0, "inicio_pts": de.1, "fim_s": ate.0, "fim_pts": ate.1,
                "velocidade": { "tipo": "constante", "fator": fator }, "transicao_entrada": transicao })
    }
    fn titulo(id: &str, duracao_s: f64, transicao: Value) -> Value {
        json!({ "tipo": "titulo", "id": id, "duracao_s": duracao_s, "transicao_entrada": transicao,
                "titulo": { "modelo": "cartela", "fundo": { "tipo": "cor", "cor": "#000000" }, "blocos": [] } })
    }
    fn transicao(tipo: &str, duracao_s: f64) -> Value {
        json!({ "tipo": tipo, "duracao_s": duracao_s, "audio": "acompanhar" })
    }
    fn sob_titulo(id: &str, pos: Value, duracao_s: Option<f64>) -> Value {
        json!({ "tipo": "titulo", "id": id, "rotulo": "", "pos": pos, "duracao_s": duracao_s, "entrada_s": 0.0, "saida_s": 0.0,
                "transformacao": { "x": 0.5, "y": 0.5, "escala": 1.0, "rotacao": 0.0, "opacidade": 1.0 },
                "titulo": { "modelo": "cartela", "fundo": { "tipo": "transparente" }, "blocos": [] } })
    }

    /// Mídias dos testes da linha: "a" 30 qps em 1/15360 (300 quadros, 16:9) e "b" 15 qps em 1 ms (150 quadros, 4:3).
    fn midias() -> HashMap<String, FonteInfo> {
        let a = apoio::tabela((1, 15360), apoio::pts_regulares(300, (1, 30), (1, 15360), 0));
        let b = apoio::tabela((1, 1000), apoio::pts_regulares(150, (1, 15), (1, 1000), 2000));
        HashMap::from([("a".to_string(), apoio::fonte(a, (1280, 720))), ("b".to_string(), apoio::fonte(b, (640, 480)))])
    }

    fn com_contexto<T>(fontes: &HashMap<String, FonteInfo>, cap: &Capacidades, f: impl FnOnce(&Contexto) -> T) -> T {
        f(&Contexto { fontes, cap, rastreios: &SemRastreios, pasta_projeto: Path::new(""), midias: &[] })
    }

    fn linha_de(p: &ProjetoEdicao, fontes: &HashMap<String, FonteInfo>) -> LinhaResolvida {
        com_contexto(fontes, &apoio::cap_falsa(), |ctx| resolver(p, ctx))
    }

    #[test]
    fn linha_da_principal_com_transicoes() {
        let m = midias();
        let p = projeto(json!({ "principal": [
            titulo("t", 2.0, json!(null)),
            // [quadro 30, quadro 90): 60 quadros, 60 slots
            trecho("c1", "a", (1.0, Some(15360)), (3.0, Some(46080)), 1.0, transicao("dissolver", 0.5)),
            // 15 qps em projeto de 30, a 0,5×: 30 quadros, 120 slots
            trecho("c2", "b", (4.0, None), (6.0, None), 0.5, transicao("escurecer", 1.0)),
            { "tipo": "congelar", "id": "g", "midia": "a", "instante_s": 2.0, "instante_pts": 30720, "duracao_s": 1.5, "transicao_entrada": transicao("dissolver", 1.0) },
            { "tipo": "imagem", "id": "i", "midia": "foto", "duracao_s": 0.01 },
        ] }));
        let l = linha_de(&p, &m);
        let resumo: Vec<(&str, u64, u32, u32)> = l.clipes.iter().map(|c| (c.id.as_str(), c.inicio_q, c.n_q, c.sobrepoe_q)).collect();
        // a entrada do primeiro e o `escurecer` não sobrepõem; o `dissolver` puxa o clipe para dentro do anterior
        assert_eq!(resumo, [("t", 0, 60, 0), ("c1", 45, 60, 15), ("c2", 105, 120, 0), ("g", 195, 45, 30), ("i", 240, 1, 0)]);
        assert_eq!(l.total_q, 241);
        let f = l.clipes[1].fonte.as_ref().unwrap();
        assert_eq!((f.midia.as_str(), f.j_in, f.j_out, f.pts_in, f.pts_out, f.lidos, f.repetidos, f.descartados), ("a", 30, 90, 15360, Some(46080), 60, 0, 0));
        let f = l.clipes[2].fonte.as_ref().unwrap();
        assert_eq!((f.j_in, f.j_out, f.pts_in, f.pts_out, f.lidos, f.repetidos, f.descartados), (30, 60, 4000, Some(6000), 30, 90, 0));
        let f = l.clipes[3].fonte.as_ref().unwrap();
        assert_eq!((f.j_in, f.j_out, f.pts_in, f.lidos, f.repetidos), (60, 61, 30720, 1, 44));
        assert!(l.clipes[0].fonte.is_none() && l.clipes[4].fonte.is_none());
        // conversões programa ↔ fonte
        assert_eq!((clipe_no_slot(&l, 44), clipe_no_slot(&l, 50), clipe_no_slot(&l, 240), clipe_no_slot(&l, 241)), (Some(0), Some(1), Some(4), None));
        let mapa = mapa_trecho(&m["b"].quadros, 30, 60, &cte(0.5), r(30, 1)).unwrap();
        assert_eq!((quadro_no_slot(&l.clipes[2], &mapa, 105), quadro_no_slot(&l.clipes[2], &mapa, 110), quadro_no_slot(&l.clipes[2], &mapa, 104)), (Some(30), Some(31), None));
        assert_eq!(slots_no_programa(&l.clipes[2], &mapa, 31), Some((109, 4)));
    }

    #[test]
    fn linha_fim_da_midia_desligado_e_clipe_sem_tabela() {
        let m = midias();
        let p = projeto(json!({ "principal": [
            // fim_pts nulo com fim_s no fim da mídia = até o último quadro
            trecho("c1", "a", (9.0, Some(138240)), (10.0, None), 1.0, json!(null)),
            { "tipo": "titulo", "id": "fora", "desligado": true, "duracao_s": 5.0, "titulo": { "modelo": "cartela", "fundo": { "tipo": "transparente" }, "blocos": [] } },
            // mídia sem tabela: slots pelos segundos, sem faixa
            trecho("c2", "nao_ha", (0.0, None), (2.0, None), 0.5, json!(null)),
            // faixa invertida: 1 slot, sem faixa
            trecho("c3", "a", (3.0, None), (1.0, None), 1.0, json!(null)),
            { "tipo": "do_futuro", "id": "x" },
        ] }));
        let l = linha_de(&p, &m);
        let resumo: Vec<(&str, u64, u32, bool)> = l.clipes.iter().map(|c| (c.id.as_str(), c.inicio_q, c.n_q, c.fonte.is_some())).collect();
        assert_eq!(resumo, [("c1", 0, 30, true), ("c2", 30, 120, false), ("c3", 150, 1, false)]);
        let f = l.clipes[0].fonte.as_ref().unwrap();
        assert_eq!((f.j_in, f.j_out, f.pts_out), (270, 300, None));
        assert_eq!(l.total_q, 151);
    }

    #[test]
    fn clipes_livres_presos_a_clipe_e_ao_programa() {
        let m = midias();
        let principal = |ordem: [&str; 2]| -> Vec<Value> {
            ordem.iter().map(|&id| if id == "t" { titulo("t", 2.0, json!(null)) } else { trecho("c1", "a", (1.0, None), (3.0, None), 1.0, json!(null)) }).collect()
        };
        let resto = json!({
            "midias": [{ "id": "n", "tipo": "audio", "origem": "projeto", "media_id": null, "sha256": "00", "nome": "n", "relativo": "n.wav", "duracao_s": 4.0,
                         "inicio_s": 0.0, "largura": null, "altura": null, "taxa": null, "vfr": false, "tem_audio": true, "cor": null, "derivacao": null,
                         "recebido": { "nativo": null, "obs": "" }, "relogio_desvio_s": null, "gravador": null, "decodificacao": null }],
            "sobreposicoes": [{ "id": "s1", "nome": "S1", "oculta": false, "clipes": [
                sob_titulo("preso", json!({ "ref": "clipe", "clipe": "c1", "desloc": 0.5 }), Some(1.0)),
                sob_titulo("ate_o_fim", json!({ "ref": "clipe", "clipe": "c1", "desloc": 0.5 }), None),
                sob_titulo("antes", json!({ "ref": "clipe", "clipe": "c1", "desloc": -5.0 }), Some(0.0)),
                sob_titulo("fixo", json!({ "ref": "programa", "t": 1.0 }), None),
                sob_titulo("orfao", json!({ "ref": "clipe", "clipe": "nao_ha", "desloc": 0.0 }), Some(1.0)),
            ] }],
            "audios": [{ "id": "a1", "nome": "Narração", "papel": "narracao", "volume": 1.0, "mudo": false, "abaixa_original": true, "clipes": [
                { "id": "n1", "nome": "", "midia": "n", "pos": { "ref": "clipe", "clipe": "c1", "desloc": 1.0 }, "de_s": 0.5, "ate_s": 2.0, "volume": 1.0, "entrada_s": 0.0, "saida_s": 0.0 },
                { "id": "n2", "nome": "", "midia": "n", "pos": { "ref": "programa", "t": 0.0 }, "de_s": 1.0, "ate_s": null, "volume": 1.0, "entrada_s": 0.0, "saida_s": 0.0 },
            ] }],
        });
        let linha = |ordem: [&str; 2]| {
            let mut extra = resto.clone();
            extra["principal"] = json!(principal(ordem));
            let l = linha_de(&projeto(extra), &m);
            l.livres.iter().map(|x| (x.id.clone(), x.trilha.clone(), x.inicio_q, x.n_q)).collect::<Vec<_>>()
        };
        let s = |id: &str, trilha: &str, q: u64, n: u32| (id.to_string(), trilha.to_string(), q, n);
        // título (60 slots) antes do trecho (60 slots)
        assert_eq!(
            linha(["t", "c1"]),
            [s("preso", "s1", 75, 30), s("ate_o_fim", "s1", 75, 45), s("antes", "s1", 0, 1), s("fixo", "s1", 30, 90), s("n1", "a1", 90, 45), s("n2", "a1", 0, 90)]
        );
        // reordenar a principal leva junto o que está preso ao clipe e deixa o resto
        assert_eq!(
            linha(["c1", "t"]),
            [s("preso", "s1", 15, 30), s("ate_o_fim", "s1", 15, 45), s("antes", "s1", 0, 1), s("fixo", "s1", 30, 90), s("n1", "a1", 30, 45), s("n2", "a1", 0, 90)]
        );
    }

    #[test]
    fn linha_a_29_97_e_taxa_invalida() {
        let m = midias();
        let p = projeto(json!({ "fps": { "num": 30000, "den": 1001 }, "principal": [trecho("c1", "a", (0.0, None), (10.0, None), 1.0, json!(null)), titulo("t", 5.0, json!(null))] }));
        let l = linha_de(&p, &m);
        // 300 quadros de 30 qps em 29,97: 300 slots (10 s não acumulam meio quadro de atraso)
        assert_eq!((l.clipes[0].n_q, l.clipes[1].n_q, l.total_q), (300, 150, 450));
        let p = projeto(json!({ "fps": { "num": 0, "den": 1 }, "principal": [titulo("t", 5.0, json!(null))] }));
        assert_eq!((linha_de(&p, &m).total_q, linha_de(&p, &m).clipes.len()), (0, 0));
    }

    #[test]
    fn prender_acha_o_que_falta_e_o_que_nao_bate() {
        let m = midias();
        let p = projeto(json!({
            "principal": [
                // início sem pts; fim com o par certo
                trecho("c1", "a", (1.0, None), (3.0, Some(46080)), 1.0, json!(null)),
                // pts que a tabela não tem (era de outra tabela): vale o quadro exibido em t; fim no fim da mídia
                trecho("c2", "b", (4.0, Some(4001)), (13.0, None), 1.0, json!(null)),
                // pts certo com segundos de outro quadro: o pts é a identidade, os segundos são corrigidos
                { "tipo": "congelar", "id": "g", "midia": "a", "instante_s": 7.0, "instante_pts": 30720, "duracao_s": 1.0 },
                // tudo certo: nada a gravar
                trecho("c3", "a", (2.0, Some(30720)), (4.0, Some(61440)), 1.0, json!(null)),
            ],
            "rastreios": [{ "id": "r1", "nome": "", "cor": "#ff0000", "midia": "a", "base": "fonte", "tipo": "caixa", "espaco": "original",
                            "ancoras": [{ "t": 1.0, "pts": 15360, "v": { "x": 0.1, "y": 0.1, "w": 0.2, "h": 0.2 } }, { "t": 1.51, "v": { "x": 0.1, "y": 0.1, "w": 0.2, "h": 0.2 } }],
                            "denso": null, "silhuetas": null, "origem": { "tipo": "manual" }, "confirmacao": null }],
        }));
        let v = com_contexto(&m, &apoio::cap_falsa(), |ctx| prender(&p, ctx));
        let fim_b = m["b"].quadros.fim_s();
        assert_eq!(
            v,
            [
                Prender { onde: OndePrender::Inicio { clipe: "c1".into() }, pts: Some(15360), t: 1.0 },
                Prender { onde: OndePrender::Inicio { clipe: "c2".into() }, pts: Some(4000), t: 4.0 },
                Prender { onde: OndePrender::Fim { clipe: "c2".into() }, pts: None, t: fim_b },
                Prender { onde: OndePrender::Instante { clipe: "g".into() }, pts: Some(30720), t: 2.0 },
                // 1,51 s: o quadro exibido é o 45 (1,5 s)
                Prender { onde: OndePrender::Ancora { rastreio: "r1".into(), indice: 1 }, pts: Some(23040), t: 1.5 },
            ]
        );
        // gravado o que foi pedido, não há mais nada a prender
        let p = projeto(json!({ "principal": [
            trecho("c1", "a", (1.0, Some(15360)), (3.0, Some(46080)), 1.0, json!(null)),
            trecho("c2", "b", (4.0, Some(4000)), (fim_b, None), 1.0, json!(null)),
        ] }));
        assert!(com_contexto(&m, &apoio::cap_falsa(), |ctx| prender(&p, ctx)).is_empty());
        // mídia não marcável (tabela de pacotes de h264 cru): nada é preso por ela (contratos (h) 17)
        let p = projeto(json!({ "principal": [trecho("c1", "a", (1.0, None), (3.0, None), 1.0, json!(null))] }));
        assert!(!com_contexto(&m, &apoio::cap_falsa(), |ctx| prender(&p, ctx)).is_empty());
        let mut m = m;
        let a = m.get_mut("a").unwrap();
        a.conteiner = "h264".into();
        let mut q = (*a.quadros).clone();
        q.origem = crate::edicao::quadros::OrigemTabela::Pacotes;
        a.quadros = q.into();
        assert!(com_contexto(&m, &apoio::cap_falsa(), |ctx| prender(&p, ctx)).is_empty());
    }

    #[test]
    fn saida_automatica() {
        let mut m = midias();
        let quatro_k = apoio::tabela((1, 90000), apoio::pts_regulares(200, (1001, 60000), (1, 90000), 0));
        m.insert("c".into(), apoio::fonte(quatro_k, (3840, 2160)));
        let auto = |principal: Value, ligada: bool| com_contexto(&m, &apoio::cap_falsa(), |ctx| saida_auto(&projeto(json!({ "auto": ligada, "principal": principal })), ctx));
        // o gravador 4:3 a 15 qps ocupa mais tempo: proporção dele, altura e taxa da outra
        let gravador_e_celular = json!([trecho("c1", "b", (2.0, None), (8.0, None), 1.0, json!(null)), trecho("c2", "a", (0.0, None), (2.0, None), 1.0, json!(null))]);
        assert_eq!(auto(gravador_e_celular.clone(), true), Some((960, 720, r(30, 1))));
        assert_eq!(auto(gravador_e_celular, false), None);
        // altura até 1080; 59,94 fica 59,94
        assert_eq!(auto(json!([trecho("c1", "c", (0.0, None), (3.0, None), 1.0, json!(null))]), true), Some((1920, 1080, r(60000, 1001))));
        // só título: sem mídia
        assert_eq!(auto(json!([titulo("t", 2.0, json!(null))]), true), None);
    }

    // ---------------------------------------------------------------- fixtures do espelho do front

    /// Entrada de `mapaTrecho` no front: a tabela compacta, o trecho, a velocidade do modelo e a taxa.
    fn caso_mapa(nome: &str, tab: &TabelaQuadros, j_in: usize, j_out: usize, v: &Velocidade, fps: Racional) -> Value {
        let saida = match mapa_trecho(tab, j_in, j_out, v, fps) {
            Ok(m) => json!(m),
            Err(SicroError::Validation(c)) => json!({ "erro": c }),
            Err(e) => panic!("{e}"),
        };
        json!({ "nome": nome, "entrada": { "tabela": tab.compacta().unwrap(), "j_in": j_in, "j_out": j_out, "velocidade": v, "fps": fps }, "saida": saida })
    }

    /// Bases de tempo do aceite: (nome, tb, passo em s, pts inicial).
    const BASES: &[(&str, Fracao, Fracao, i64)] = &[
        ("mp4 1/15360 30 qps", (1, 15360), (1, 30), 0),
        ("mkv 1 ms 30 qps", (1, 1000), (1, 30), 0),
        ("mkv 1 ms 23,976", (1, 1000), (1001, 24000), 0),
        ("1 ms 15 qps", (1, 1000), (1, 15), 0),
        ("1 ms 12 qps", (1, 1000), (1, 12), 0),
        ("mp4 escala 30", (1, 30), (1, 30), 0),
        ("avi 1/25", (1, 25), (1, 25), 0),
        ("ts 1/90000 59,94 início 3600 s", (1, 90000), (1001, 60000), 324_126_000),
        ("mov 1/600 15 qps", (1, 600), (1, 15), 0),
        ("mp4 1001/30000", (1001, 30000), (1001, 30000), 0),
        ("gravador 1/90000 12,5 qps em época", (1, 90000), (2, 25), 135_518_643_900_000),
        // som antes da imagem: pts negativos (o espelho não pode truncar a divisão para o zero)
        ("mkv 1 ms 30 qps começando em −1 s", (1, 1000), (1, 30), -1000),
        ("ts 1/90000 29,97 começando em −0,5 s", (1, 90000), (1001, 30000), -45000),
    ];

    #[test]
    fn fixture_tempo_mapa() {
        let mut casos = Vec::new();
        let mut s = Sorteio(7);
        for &(nome, tb, passo, inicio) in BASES {
            let tab = apoio::tabela(tb, apoio::pts_regulares(120, passo, tb, inicio));
            // na própria taxa, câmera lenta e aceleração; depois velocidades e taxas sorteadas
            let propria = r(passo.1, passo.0);
            casos.push(caso_mapa(&format!("{nome}: própria taxa"), &tab, 20, 80, &cte(1.0), propria));
            casos.push(caso_mapa(&format!("{nome}: 0,5× na própria taxa"), &tab, 21, 51, &cte(0.5), propria));
            casos.push(caso_mapa(&format!("{nome}: 0,25× na própria taxa"), &tab, 22, 37, &cte(0.25), propria));
            casos.push(caso_mapa(&format!("{nome}: até o fim da mídia a 30"), &tab, 100, 120, &cte(1.0), r(30, 1)));
            for _ in 0..4 {
                let v = VELOCIDADES[s.ate(VELOCIDADES.len())];
                let fps = TAXAS[s.ate(TAXAS.len())];
                let lidos = ((30.0 * v) as usize).clamp(2, 100);
                let j_in = s.ate(120 - lidos);
                casos.push(caso_mapa(&format!("{nome}: {v}× a {}/{}, quadros {j_in} a {}", fps.0, fps.1, j_in + lidos), &tab, j_in, j_in + lidos, &cte(v), r(fps.0, fps.1)));
            }
        }
        // toda velocidade do aceite e toda taxa, numa base fina e numa de 1 ms
        for &(nome, tb, passo, inicio) in &BASES[..2] {
            let tab = apoio::tabela(tb, apoio::pts_regulares(120, passo, tb, inicio));
            for (i, &v) in VELOCIDADES.iter().enumerate() {
                let fps = TAXAS[i % TAXAS.len()];
                let lidos = ((20.0 * v) as usize).clamp(2, 110);
                casos.push(caso_mapa(&format!("{nome}: {v}× a {}/{}", fps.0, fps.1), &tab, 5, 5 + lidos, &cte(v), r(fps.0, fps.1)));
            }
        }
        // passos irregulares: oscilação de ±4 ms, quadros faltando, pts repetido
        let mut osc = apoio::pts_regulares(120, (1, 30), (1, 1000), 0);
        for (i, p) in osc.iter_mut().enumerate().skip(1) {
            *p += (i as i64 * 7) % 9 - 4;
        }
        let osc = apoio::tabela((1, 1000), osc);
        casos.push(caso_mapa("oscilação de 4 ms: própria taxa", &osc, 10, 100, &cte(1.0), r(30, 1)));
        casos.push(caso_mapa("oscilação de 4 ms: 0,25×", &osc, 10, 40, &cte(0.25), r(30, 1)));
        let vfr: Vec<i64> = apoio::pts_regulares(160, (1, 30), (1, 15360), 0).into_iter().enumerate().filter(|(i, _)| i % 7 != 3 && i % 11 != 5).map(|x| x.1).collect();
        let vfr = apoio::tabela((1, 15360), vfr);
        casos.push(caso_mapa("taxa variável a 30", &vfr, 0, 100, &cte(1.0), r(30, 1)));
        casos.push(caso_mapa("taxa variável a 25, 2×", &vfr, 13, 113, &cte(2.0), r(25, 1)));
        let mut rep = apoio::pts_regulares(60, (1, 30), (1, 1000), 0);
        rep[30] = rep[29];
        casos.push(caso_mapa("pts repetido", &apoio::tabela((1, 1000), rep), 0, 50, &cte(1.0), r(30, 1)));
        // erros
        let tab = apoio::tabela((1, 1000), apoio::pts_regulares(60, (1, 30), (1, 1000), 0));
        casos.push(caso_mapa("erro: trecho vazio", &tab, 10, 10, &cte(1.0), r(30, 1)));
        casos.push(caso_mapa("erro: fora da mídia", &tab, 10, 61, &cte(1.0), r(30, 1)));
        casos.push(caso_mapa("erro: velocidade abaixo de 0,05", &tab, 0, 10, &cte(0.04), r(30, 1)));
        casos.push(caso_mapa("erro: velocidade com 4 casas", &tab, 0, 10, &cte(0.3333), r(30, 1)));
        casos.push(caso_mapa("erro: rampa", &tab, 0, 10, &Velocidade::Rampa { pontos: Vec::new() }, r(30, 1)));
        // passos de 4 s ainda cabem em 32 bits na tabela compacta
        let grossa = apoio::tabela((1, 1_000_000_007), apoio::pts_regulares(60, (4, 1), (1, 1_000_000_007), 0));
        casos.push(caso_mapa("erro: mapa grande", &grossa, 0, 55, &cte(1.0), r(60000, 1001)));
        // trecho de 1 quadro: no começo, no meio de pts negativos e no fim da mídia, nas pontas das velocidades
        let neg = apoio::tabela((1, 1000), apoio::pts_regulares(60, (1, 30), (1, 1000), -1000));
        for (v, fps) in [(1.0, r(30, 1)), (0.05, r(60000, 1001)), (16.0, r(25, 2))] {
            for (a, b) in [(0, 1), (17, 18), (59, 60)] {
                casos.push(caso_mapa(&format!("1 quadro [{a}, {b}) com pts negativo a {v}× em {}/{}", fps.num, fps.den), &neg, a, b, &cte(v), fps));
            }
        }
        casos.push(caso_mapa("pts negativo até o fim da mídia a 0,05× em 59,94", &neg, 50, 60, &cte(0.05), r(60000, 1001)));
        fixtures::conferir("tempo-mapa.json", Value::Array(casos));
    }

    #[test]
    fn fixture_tempo_quadro() {
        use crate::edicao::quadros::AlvoQuadro;
        let mut casos = Vec::new();
        for &(nome, tb, passo, inicio) in BASES {
            let tab = apoio::tabela(tb, apoio::pts_regulares(40, passo, tb, inicio));
            let compacta = json!(tab.compacta().unwrap());
            let mut caso = |rotulo: String, alvo: AlvoQuadro| {
                let saida = match tab.quadro_em(alvo) {
                    Ok(q) => json!(q),
                    Err(SicroError::Validation(c)) => json!({ "erro": c }),
                    Err(e) => panic!("{e}"),
                };
                casos.push(json!({ "nome": format!("{nome}: {rotulo}"), "entrada": { "tabela": compacta, "alvo": alvo }, "saida": saida }));
            };
            let (t7, d7) = (tab.tempo(7), tab.duracao(7));
            caso("no pts do quadro 7".into(), AlvoQuadro::T { t: t7 });
            caso("no meio do quadro 7".into(), AlvoQuadro::T { t: t7 + d7 / 2.0 });
            caso("um pouco antes do quadro 8".into(), AlvoQuadro::T { t: t7 + d7 * 0.9 });
            caso("antes do primeiro".into(), AlvoQuadro::T { t: tab.tempo(0) - 1.0 });
            caso("depois do último".into(), AlvoQuadro::T { t: tab.fim_s() + 1.0 });
            caso("índice 39".into(), AlvoQuadro::Indice { indice: 39 });
            caso("índice fora".into(), AlvoQuadro::Indice { indice: 40 });
            caso("pts do quadro 12".into(), AlvoQuadro::Pts { pts: tab.pts[12] });
            if tab.pts[13] - tab.pts[12] > 1 {
                caso("pts entre o 12 e o 13".into(), AlvoQuadro::Pts { pts: tab.pts[12] + 1 });
            }
        }
        fixtures::conferir("tempo-quadro.json", Value::Array(casos));
    }

    #[test]
    fn fixture_tempo_contas() {
        let mut casos = Vec::new();
        for &(num, den) in TAXAS {
            let fps = r(num, den);
            for d in [0.0, 0.01, 0.5, 1.0, 2.5, 3.337, 10.0, 59.94, 3600.0, -1.0] {
                casos.push(json!({ "nome": format!("slots de {d} s a {num}/{den}"), "entrada": { "conta": "slots", "duracao_s": d, "fps": fps }, "saida": slots(d, fps) }));
                casos.push(json!({ "nome": format!("clipe sem fonte de {d} s a {num}/{den}"), "entrada": { "conta": "n_q_sem_fonte", "duracao_s": d, "fps": fps }, "saida": n_q_sem_fonte(d, fps) }));
            }
            for q in [0u64, 1, 2, 5, 29, 30, 1001, 30000, 107_892] {
                casos.push(json!({ "nome": format!("instante do slot {q} a {num}/{den}"), "entrada": { "conta": "instante_do_slot", "q": q, "fps": fps }, "saida": instante_do_slot(q, fps) }));
                casos.push(json!({ "nome": format!("amostra do slot {q} a {num}/{den}"), "entrada": { "conta": "amostra_do_slot", "q": q, "fps": fps }, "saida": amostra_do_slot(q, fps) }));
            }
        }
        for f in [0.05, 0.1, 0.25, 0.333, 0.37, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 4.0, 8.0, 16.0, 0.049, 16.001, 0.3333, 0.0] {
            let saida = match fator_racional(f) {
                Ok(x) => json!(x),
                Err(_) => json!({ "erro": "velocidade" }),
            };
            casos.push(json!({ "nome": format!("fator {f}"), "entrada": { "conta": "fator_racional", "fator": f }, "saida": saida }));
        }
        fixtures::conferir("tempo-contas.json", Value::Array(casos));
    }

    #[test]
    fn fixture_tempo_resolver() {
        let m = midias();
        let tabelas: serde_json::Map<String, Value> = m.iter().map(|(k, f)| (k.clone(), json!(f.quadros.compacta().unwrap()))).collect();
        let mut nomes: Vec<&String> = tabelas.keys().collect();
        nomes.sort();
        assert_eq!(nomes, ["a", "b"]);
        let caso = |nome: &str, extra: Value| -> Value {
            let p = projeto(extra);
            json!({ "nome": nome, "entrada": { "projeto": p, "tabelas": tabelas }, "saida": linha_de(&p, &m) })
        };
        let narracao = json!([{ "id": "n", "tipo": "audio", "origem": "projeto", "media_id": null, "sha256": "00", "nome": "n", "relativo": "n.wav", "duracao_s": 4.0,
                               "inicio_s": 0.0, "largura": null, "altura": null, "taxa": null, "vfr": false, "tem_audio": true, "cor": null, "derivacao": null,
                               "recebido": { "nativo": null, "obs": "" }, "relogio_desvio_s": null, "gravador": null, "decodificacao": null }]);
        let livres = json!([{ "id": "s1", "nome": "S1", "oculta": false, "clipes": [
            sob_titulo("preso", json!({ "ref": "clipe", "clipe": "c1", "desloc": 0.5 }), Some(1.0)),
            sob_titulo("ate_o_fim", json!({ "ref": "clipe", "clipe": "c1", "desloc": 0.5 }), None),
            sob_titulo("antes", json!({ "ref": "clipe", "clipe": "c1", "desloc": -5.0 }), Some(0.0)),
            sob_titulo("fixo", json!({ "ref": "programa", "t": 1.0 }), None),
            sob_titulo("orfao", json!({ "ref": "clipe", "clipe": "nao_ha", "desloc": 0.0 }), Some(1.0)),
        ] }]);
        let audios = json!([{ "id": "a1", "nome": "Narração", "papel": "narracao", "volume": 1.0, "mudo": false, "abaixa_original": true, "clipes": [
            { "id": "n1", "nome": "", "midia": "n", "pos": { "ref": "clipe", "clipe": "c1", "desloc": 1.0 }, "de_s": 0.5, "ate_s": 2.0, "volume": 1.0, "entrada_s": 0.0, "saida_s": 0.0 },
            { "id": "n2", "nome": "", "midia": "n", "pos": { "ref": "programa", "t": 0.0 }, "de_s": 1.0, "ate_s": null, "volume": 1.0, "entrada_s": 0.0, "saida_s": 0.0 },
        ] }]);
        let principal = json!([
            titulo("t", 2.0, json!(null)),
            trecho("c1", "a", (1.0, Some(15360)), (3.0, Some(46080)), 1.0, transicao("dissolver", 0.5)),
            trecho("c2", "b", (4.0, None), (6.0, None), 0.5, transicao("escurecer", 1.0)),
            { "tipo": "congelar", "id": "g", "midia": "a", "instante_s": 2.0, "instante_pts": 30720, "duracao_s": 1.5, "transicao_entrada": transicao("dissolver", 1.0) },
            { "tipo": "imagem", "id": "i", "midia": "foto", "duracao_s": 0.01 },
        ]);
        let casos = vec![
            caso("vazio", json!({})),
            caso("principal com transições, a 30", json!({ "principal": principal })),
            caso("o mesmo a 29,97", json!({ "fps": { "num": 30000, "den": 1001 }, "principal": principal })),
            caso("o mesmo a 12,5", json!({ "fps": { "num": 25, "den": 2 }, "principal": principal })),
            caso("o mesmo a 60, com livres e áudio", json!({ "fps": { "num": 60, "den": 1 }, "principal": principal, "midias": narracao, "sobreposicoes": livres, "audios": audios })),
            caso("livres e áudio, título antes do trecho", json!({
                "principal": [titulo("t", 2.0, json!(null)), trecho("c1", "a", (1.0, None), (3.0, None), 1.0, json!(null))],
                "midias": narracao, "sobreposicoes": livres, "audios": audios })),
            caso("livres e áudio, trecho antes do título", json!({
                "principal": [trecho("c1", "a", (1.0, None), (3.0, None), 1.0, json!(null)), titulo("t", 2.0, json!(null))],
                "midias": narracao, "sobreposicoes": livres, "audios": audios })),
            caso("fim da mídia, desligado, sem tabela, faixa invertida, desconhecido", json!({ "principal": [
                trecho("c1", "a", (9.0, Some(138240)), (10.0, None), 1.0, json!(null)),
                { "tipo": "titulo", "id": "fora", "desligado": true, "duracao_s": 5.0, "titulo": { "modelo": "cartela", "fundo": { "tipo": "transparente" }, "blocos": [] } },
                trecho("c2", "nao_ha", (0.0, None), (2.0, None), 0.5, json!(null)),
                trecho("c3", "a", (3.0, None), (1.0, None), 1.0, json!(null)),
                { "tipo": "do_futuro", "id": "x" },
            ] })),
            caso("velocidades de 0,05× a 16×", json!({ "principal": [
                trecho("v1", "a", (0.0, None), (0.5, None), 0.05, json!(null)),
                trecho("v2", "a", (1.0, None), (9.0, None), 16.0, json!(null)),
                trecho("v3", "b", (2.0, None), (5.0, None), 0.37, json!(null)),
                trecho("v4", "b", (2.0, Some(2000)), (12.0, None), 4.0, transicao("dissolver", 0.2)),
                trecho("v5", "a", (0.0, None), (2.0, None), 1.2345, json!(null)),
            ] })),
            caso("transição maior que os clipes", json!({ "principal": [titulo("t1", 1.0, json!(null)), titulo("t2", 0.5, transicao("dissolver", 3.0)), titulo("t3", 2.0, transicao("dissolver", 3.0))] })),
            // −7,5 slots: o Rust arredonda para longe do zero (−8); `Math.round` daria −7
            caso("livre preso com deslocamento negativo de meio slot", json!({
                "principal": [titulo("t", 2.0, json!(null)), trecho("c1", "a", (1.0, None), (3.0, None), 1.0, json!(null))],
                "sobreposicoes": [{ "id": "s1", "nome": "S1", "oculta": false, "clipes": [
                    sob_titulo("meio_negativo", json!({ "ref": "clipe", "clipe": "c1", "desloc": -0.25 }), Some(0.25)),
                    sob_titulo("meio_positivo", json!({ "ref": "clipe", "clipe": "c1", "desloc": 0.25 }), Some(0.25)),
                ] }] })),
        ];
        fixtures::conferir("tempo-resolver.json", Value::Array(casos));
    }

    // ---------------------------------------------------------------- medido no ffmpeg (cadeia do §5.3)

    use crate::edicao::ffmpeg::args;
    use crate::edicao::ffmpeg::exec::{self, Comando};
    use crate::edicao::quadros::apoio::Medida;
    use crate::edicao::testes::fontes::{self, Fonte, MATRIZ};
    use crate::edicao::testes::medir;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Um trecho a renderizar: quadros `[j_in, j_out)` da fonte a `v`× na taxa `fps`.
    #[derive(Debug, Clone, Copy)]
    struct Caso {
        fonte: usize,
        v: f64,
        fps: (i64, i64),
        j_in: usize,
        j_out: usize,
    }

    /// O que saiu de um render: o número desenhado em cada slot, os `frames in` do `fps@` e se houve `-ss`.
    struct Saida {
        numeros: Vec<u32>,
        lidos: u64,
        com_busca: bool,
    }

    /// Renderiza o trecho pela cadeia do §5.3, montada à mão: `-copyts`, busca pela regra, `trim` por pts, cor,
    /// `settb` + `setpts` em inteiros, `fps@` com `start_time=0`, `tpad` e `trim=end_frame`; identifica cada quadro que sai.
    fn renderizar(cap: &Capacidades, m: &Medida, mapa: &MapaQuadros, fps: Racional, pasta: &Path, n: usize) -> Result<Saida> {
        let (info, tab) = (&m.info, &m.info.quadros);
        let corte = match tab.pts.get(mapa.j_out) {
            Some(fim) => format!("trim=start_pts={}:end_pts={fim}", tab.pts[mapa.j_in]),
            None => format!("trim=start_pts={}", tab.pts[mapa.j_in]),
        };
        let mut cor = crate::edicao::cor::cadeia_fonte(info.matriz, info.faixa, &info.pix_fmt, info.exibido).expect("formato de pixel da matriz");
        // regra fixa do §5.3: depois de filtro de várias entradas (o `mergeplanes` da faixa cheia) vai um quadro
        // clonado, senão o 7.0.2 perde o último no `fps`. O mesmo no trecho que vai até o fim da mídia quando o
        // decodificador não dá a duração do quadro (Dahua `.dav`). O `fps@` conta esse quadro a mais nos dois binários.
        let clonado = u64::from(cor.contains("mergeplanes") || mapa.j_out == tab.pts.len());
        if clonado == 1 {
            cor.push_str(",tpad=stop_mode=clone:stop=1");
        }
        let grafo = format!(
            "[0:v]{corte},{cor},settb=1/{c},setpts='ceil(((PTS-{p0})*{kn}-{e})/{kd})*{m}',fps@c000={num}/{den}:start_time=0,\
             tpad=stop_mode=clone:stop=-1,trim=end_frame={n_q}{reduzir},extractplanes=y[v]",
            c = mapa.c, p0 = mapa.p0, kn = mapa.kn, e = mapa.e, kd = mapa.kd, m = mapa.m, num = fps.num, den = fps.den, n_q = mapa.n_q,
            reduzir = if m.por_conteudo.is_some() { format!(",{}", apoio::REDUZIDO.0) } else { String::new() }
        );
        let nome = format!("g{n}.txt");
        args::gravar_texto(pasta, &nome, &grafo)?;
        let mut a = args::globais();
        let busca = tab.busca_em(info.inicio_s, mapa.j_in);
        if let Some(alvo) = busca {
            let fim = if mapa.j_out < tab.pts.len() { tab.tempo(mapa.j_out) } else { tab.fim_s() } - info.inicio_s;
            a.extend(["-ss".to_string(), format!("{alvo:.6}"), "-t".to_string(), format!("{:.6}", fim - alvo + 1.0)]);
        }
        a.extend(["-i".to_string(), info.caminho.to_string_lossy().into_owned()]);
        a.extend(args::grafo_em_arquivo(cap, &nome));
        a.extend(["-map", "[v]", "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "gray", "-"].map(String::from));
        let cmd = Comando {
            nome: format!("c{n}"),
            programa: cap.binario.clone(),
            args: a,
            pasta: pasta.to_path_buf(),
            ambiente: Vec::new(),
            sem_ambiente: Vec::new(),
            stderr: pasta.join(format!("c{n}.stderr")),
        };
        let (fim, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false))?;
        exec::exigir_sucesso(&cmd, &fim)?;
        let (w, h) = match m.por_conteudo {
            Some(_) => (apoio::REDUZIDO.1, apoio::REDUZIDO.2),
            None => (info.exibido.0 as usize, info.exibido.1 as usize),
        };
        if out.len() % (w * h) != 0 {
            return Err(erro("saída com quadro partido"));
        }
        let identificar = |y: &[u8]| match &m.por_conteudo {
            Some(t) => t.get(&apoio::conteudo(y)).copied().unwrap_or(medir::NAO_IDENTIFICADO),
            None => medir::identificar(y, w, h),
        };
        let lidos = exec::quadros_lidos(&cmd.stderr)?.get("c000").copied().unwrap_or(u64::MAX).saturating_sub(clonado);
        let _ = (std::fs::remove_file(pasta.join(&nome)), std::fs::remove_file(&cmd.stderr));
        Ok(Saida { numeros: out.chunks_exact(w * h).map(identificar).collect(), lidos, com_busca: busca.is_some() })
    }

    /// Previsto em inteiros × medido: devolve os slots conferidos, ou o que divergiu.
    fn conferir_caso(cap: &Capacidades, nome: &str, m: &Medida, caso: &Caso, pasta: &Path, n: usize) -> std::result::Result<(u64, bool), String> {
        let fps = r(caso.fps.0, caso.fps.1);
        let rotulo = format!("{nome} quadros [{}, {}) a {}× em {}/{} ({})", caso.j_in, caso.j_out, caso.v, fps.num, fps.den, cap.versao);
        let mapa = mapa_trecho(&m.info.quadros, caso.j_in, caso.j_out, &cte(caso.v), fps).map_err(|e| format!("{rotulo}: {e}"))?;
        let s = renderizar(cap, m, &mapa, fps, pasta, n).map_err(|e| format!("{rotulo}: {e}"))?;
        let previsto: Vec<u32> = mapa.quadro_do_slot.iter().map(|&j| m.numeros[j as usize]).collect();
        if s.numeros != previsto {
            let q = s.numeros.iter().zip(&previsto).position(|(a, b)| a != b).unwrap_or(s.numeros.len().min(previsto.len()));
            return Err(format!(
                "{rotulo}: {} slots medidos, {} previstos; primeiro slot diferente {q}: medido {:?}, previsto {:?}",
                s.numeros.len(), previsto.len(), s.numeros.get(q), previsto.get(q)
            ));
        }
        if s.lidos != (caso.j_out - caso.j_in) as u64 {
            return Err(format!("{rotulo}: frames in {} × lidos {}", s.lidos, caso.j_out - caso.j_in));
        }
        // o slot 0 mostra o quadro de entrada (com pts repetido na entrada, o último desse pts)
        let pts = &m.info.quadros.pts;
        let entrada = pts.partition_point(|&p| p <= pts[caso.j_in]) - 1;
        if previsto[0] != m.numeros[entrada] {
            return Err(format!("{rotulo}: o slot 0 não mostra o quadro de entrada"));
        }
        Ok((mapa.n_q as u64, s.com_busca))
    }

    /// Contagens de uma rodada: casos, slots conferidos, casos com `-ss` e falhas.
    #[derive(Default)]
    struct Placar {
        casos: usize,
        slots: u64,
        com_busca: usize,
        falhas: Vec<String>,
    }

    /// Roda os casos em poucas threads (outros agentes usam a máquina).
    fn rodar_casos(cap: &Capacidades, casos: &[Caso]) -> Placar {
        let medidas: Vec<_> = MATRIZ.iter().map(|&f| (format!("{f:?}"), apoio::medida(cap, f))).collect();
        rodar_casos_em(cap, &medidas, casos)
    }

    fn rodar_casos_em(cap: &Capacidades, medidas: &[(String, std::sync::Arc<Medida>)], casos: &[Caso]) -> Placar {
        let (proximo, placar) = (AtomicUsize::new(0), Mutex::new(Placar::default()));
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get() / 3).clamp(2, 6);
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| {
                    let pasta = fontes::pasta_temporaria("tempo-matriz");
                    loop {
                        let n = proximo.fetch_add(1, Ordering::Relaxed);
                        let Some(caso) = casos.get(n) else { break };
                        let (nome, m) = &medidas[caso.fonte];
                        let res = conferir_caso(cap, nome, m, caso, &pasta, n);
                        let mut p = placar.lock().unwrap();
                        p.casos += 1;
                        match res {
                            Ok((slots, com_busca)) => {
                                p.slots += slots;
                                p.com_busca += usize::from(com_busca);
                            }
                            Err(e) => p.falhas.push(e),
                        }
                    }
                });
            }
        });
        placar.into_inner().unwrap()
    }

    /// Trecho de cerca de 90 slots (até 300 quadros lidos), sorteado; de vez em quando do começo ou até o fim da mídia.
    fn trecho_sorteado(s: &mut Sorteio, m: &Medida, v: f64, fps: (i64, i64)) -> (usize, usize) {
        let n = m.limite;
        let nominal = m.info.quadros.taxa_nominal();
        let taxa_fonte = nominal.num as f64 / nominal.den as f64;
        let lidos = ((90.0 * v * taxa_fonte * fps.1 as f64 / fps.0 as f64).round() as usize).clamp(2, 300).min(n - 1);
        let (j_in, j_out) = match s.ate(7) {
            0 => (0, lidos),
            1 => (n - lidos, n),
            _ => {
                let j_in = s.ate(n - lidos);
                (j_in, j_in + lidos)
            }
        };
        // como a linha resolvida faz: ponta no primeiro quadro do pts (o MPEG-PS repete pts)
        let primeiro = |j: usize| m.info.quadros.pts.get(j).and_then(|&p| m.info.quadros.indice_do_pts(p)).unwrap_or(j);
        (primeiro(j_in), primeiro(j_out))
    }

    /// Casos de busca do §11 para uma fonte: 2 quadros antes de um quadro-chave do fim e nos 3 primeiros segundos.
    fn casos_de_busca(fonte: usize, m: &Medida) -> Vec<Caso> {
        let tab = &m.info.quadros;
        let n = m.limite;
        let nominal = tab.taxa_nominal();
        let fps = (nominal.num, nominal.den);
        let primeiro = |j: usize| tab.indice_do_pts(tab.pts[j]).unwrap_or(j);
        let chaves: Vec<usize> = (0..n).filter(|&j| tab.chave[j]).collect();
        let mut v = Vec::new();
        if let Some(&k) = chaves.iter().rev().find(|&&k| k >= 2 && k + 21 <= n) {
            v.push(Caso { fonte, v: 1.0, fps, j_in: primeiro(k - 2), j_out: primeiro(k + 20) });
            v.push(Caso { fonte, v: 1.0, fps, j_in: primeiro(k), j_out: primeiro(k + 20) });
        }
        let cedo = (2.0 * nominal.num as f64 / nominal.den as f64) as usize;
        if cedo + 31 <= n {
            v.push(Caso { fonte, v: 1.0, fps, j_in: primeiro(cedo), j_out: primeiro(cedo + 30) });
        }
        v
    }

    fn fechar(cap: &Capacidades, nome: &str, p: Placar) {
        eprintln!("{nome} ({}): {} casos, {} slots conferidos, {} com busca (-ss), {} falhas", cap.versao, p.casos, p.slots, p.com_busca, p.falhas.len());
        for f in p.falhas.iter().take(25) {
            eprintln!("  {f}");
        }
        assert!(p.falhas.is_empty(), "{} casos divergem do previsto", p.falhas.len());
    }

    /// Subconjunto rápido: toda fonte na própria taxa, em câmera lenta, acelerada e nos casos de busca.
    #[test]
    fn matriz_rapida_previsto_igual_ao_medido() {
        let Some(cap) = fontes::cap() else { return };
        let mut s = Sorteio(3);
        let mut casos = Vec::new();
        for (i, &f) in MATRIZ.iter().enumerate() {
            let m = apoio::medida(&cap, f);
            let d = fontes::descricao(f);
            for (v, fps) in [(1.0, d.fps), (0.5, (30, 1)), (4.0, (25, 1))] {
                let (j_in, j_out) = trecho_sorteado(&mut s, &m, v, fps);
                casos.push(Caso { fonte: i, v, fps, j_in, j_out });
            }
            casos.extend(casos_de_busca(i, &m));
        }
        // as pontas das velocidades e das taxas
        casos.push(Caso { fonte: 3, v: 0.05, fps: (60, 1), j_in: 200, j_out: 203 });
        casos.push(Caso { fonte: 0, v: 16.0, fps: (25, 2), j_in: 20, j_out: 420 });
        // trecho de 1 quadro: em base grossa a 0,05×, no fim da mídia a 16× e a 0,25×, e com -ss no TS de início ≠ 0
        let n = |i: usize| apoio::medida(&cap, MATRIZ[i]).info.quadros.pts.len();
        casos.push(Caso { fonte: 5, v: 0.05, fps: (60000, 1001), j_in: 200, j_out: 201 });
        casos.push(Caso { fonte: 2, v: 16.0, fps: (25, 2), j_in: n(2) - 1, j_out: n(2) });
        casos.push(Caso { fonte: 3, v: 0.25, fps: (30, 1), j_in: n(3) - 1, j_out: n(3) });
        casos.push(Caso { fonte: 6, v: 1.0, fps: (30000, 1001), j_in: 800, j_out: 801 });
        let p = rodar_casos(&cap, &casos);
        assert!(p.casos >= 100 && p.com_busca >= 20, "{} casos, {} com busca", p.casos, p.com_busca);
        fechar(&cap, "matriz rápida", p);
    }

    /// Matriz completa: fontes × velocidades de 0,05× a 16× × taxas de 12,5 a 60. Leva minutos.
    #[test]
    #[ignore]
    fn matriz_completa_previsto_igual_ao_medido() {
        let Some(cap) = fontes::cap() else { return };
        let mut s = Sorteio(20261114);
        let mut casos = Vec::new();
        for (i, &f) in MATRIZ.iter().enumerate() {
            let m = apoio::medida(&cap, f);
            for &v in VELOCIDADES {
                for &fps in TAXAS {
                    let (j_in, j_out) = trecho_sorteado(&mut s, &m, v, fps);
                    casos.push(Caso { fonte: i, v, fps, j_in, j_out });
                }
            }
            casos.extend(casos_de_busca(i, &m));
        }
        let p = rodar_casos(&cap, &casos);
        assert_eq!(p.casos, casos.len());
        fechar(&cap, "matriz completa", p);
    }

    /// As amostras reais de gravador (Dahua, Hikvision, LVF, IFV), quando o dossiê está presente: mesmo aceite da
    /// matriz, com o quadro identificado pelo conteúdo. Arquivo que o ffmpeg não abre fica de fora, com aviso.
    #[test]
    #[ignore]
    fn amostras_de_gravador_previsto_igual_ao_medido() {
        let Some(cap) = fontes::cap() else { return };
        let mut medidas = Vec::new();
        for video in fontes::amostras_de_gravador() {
            let nome = video.file_name().unwrap_or_default().to_string_lossy().into_owned();
            match apoio::medir_por_conteudo(&cap, &video) {
                Ok(m) => {
                    let (tab, distintos) = (&m.info.quadros, m.por_conteudo.as_ref().map_or(0, |t| t.len()));
                    eprintln!(
                        "{nome}: tb {}/{}, {} quadros ({distintos} conteúdos), {} pacotes, início {:.3} s, passo {:?}, nominal {}/{}, {:?}, {} erros, comparados [0, {})",
                        tab.tb.num, tab.tb.den, tab.pts.len(), tab.pacotes, m.info.inicio_s, tab.passo, tab.taxa_nominal().num, tab.taxa_nominal().den, tab.busca, tab.erros_decodificacao, m.limite
                    );
                    if m.limite < 60 {
                        eprintln!("aviso: {nome} fora do teste: erro de decodificação no começo");
                        continue;
                    }
                    medidas.push((nome, std::sync::Arc::new(m)));
                }
                Err(e) => eprintln!("aviso: {nome} fora do teste: {e}"),
            }
        }
        if medidas.is_empty() {
            eprintln!("aviso: sem amostras de gravador, nada conferido");
            return;
        }
        let mut s = Sorteio(11);
        let mut casos = Vec::new();
        for (i, (_, m)) in medidas.iter().enumerate() {
            for &v in &[0.25, 0.5, 1.0, 2.0, 8.0] {
                for &fps in &[(25, 2), (15, 1), (25, 1), (30000, 1001), (30, 1), (60, 1)] {
                    let (j_in, j_out) = trecho_sorteado(&mut s, m, v, fps);
                    casos.push(Caso { fonte: i, v, fps, j_in, j_out });
                }
            }
            casos.extend(casos_de_busca(i, m));
        }
        let p = rodar_casos_em(&cap, &medidas, &casos);
        fechar(&cap, "amostras de gravador", p);
    }

    /// Fonte de passo 1 ms a 12, 15 e 30 qps: na própria taxa nada some nem repete; a 0,5× e 0,25× todo quadro
    /// fica 2 e 4 slots. Medido na saída do ffmpeg. As de 12 e 15 qps são a de 30 com os tempos esticados (MKV).
    #[test]
    fn passo_de_1_ms_medido_na_propria_taxa() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("tempo-1ms");
        let base = fontes::gerar(Fonte::Mp4).unwrap();
        let mut medidas = vec![(30, apoio::medida(&cap, Fonte::Mkv30)), (30, apoio::medida(&cap, Fonte::Mp4Escala1000))];
        for (taxa, escala) in [(15, "2"), (12, "2.5")] {
            let nome = format!("ms{taxa}.mkv");
            let cmd = Comando {
                nome: nome.clone(),
                programa: cap.binario.clone(),
                args: ["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-itsscale", escala, "-i", &base.to_string_lossy(), "-an", "-c", "copy", &nome].map(String::from).to_vec(),
                pasta: pasta.clone(),
                ambiente: Vec::new(),
                sem_ambiente: Vec::new(),
                stderr: pasta.join(format!("{nome}.stderr")),
            };
            exec::exigir_sucesso(&cmd, &exec::rodar(&cmd, &AtomicBool::new(false)).unwrap()).unwrap();
            medidas.push((taxa, std::sync::Arc::new(apoio::medir_arquivo(&cap, &pasta.join(&nome)))));
        }
        let (mut n, mut slots) = (0, 0);
        for (taxa, m) in &medidas {
            let tab = &m.info.quadros;
            assert_eq!((tab.tb, tab.pts.len(), tab.taxa_nominal()), (r(1, 1000), 450, r(*taxa, 1)), "{taxa} qps");
            assert!(tab.passo.1 - tab.passo.0 == 1, "passos de 1 ms arredondado: {:?}", tab.passo);
            // cortes em fases diferentes do arredondamento
            for j_in in [0, 100, 101, 102, 331] {
                for (v, vezes, lidos) in [(1.0, 1, 100), (0.5, 2, 60), (0.25, 4, 30)] {
                    let mapa = mapa_trecho(tab, j_in, j_in + lidos, &cte(v), r(*taxa, 1)).unwrap();
                    assert_eq!((mapa.descartados, mapa.n_q), (0, (lidos * vezes) as u32));
                    let s = renderizar(&cap, m, &mapa, r(*taxa, 1), &pasta, n).unwrap();
                    n += 1;
                    // cada quadro lido aparece `vezes` slots seguidos, na ordem, sem faltar nenhum
                    let esperado: Vec<u32> = (j_in..j_in + lidos).flat_map(|j| std::iter::repeat_n(m.numeros[j], vezes)).collect();
                    assert_eq!(s.numeros, esperado, "{taxa} qps a {v}×, corte em {j_in} ({})", cap.versao);
                    assert_eq!(s.lidos, lidos as u64);
                    slots += s.numeros.len();
                }
            }
        }
        eprintln!("passo de 1 ms ({}): {n} renders, {slots} slots medidos, 0 descartados e 0 repetidos fora do previsto", cap.versao);
    }

    /// `critica-pericia/e11_videos_reais.py` refeito sobre o mapa novo: os vídeos registrados nos casos do dono,
    /// SÓ LEITURA (banco aberto como imutável; nada é gravado nos casos) e só números na saída.
    /// A 1× na `r_frame_rate` de cada vídeo: no máximo 2 quadros descartados no total (os de pts quase repetido).
    #[test]
    #[ignore]
    fn videos_reais_do_dono_na_propria_taxa() {
        use rusqlite::{Connection, OpenFlags};
        let Some(cap) = fontes::cap() else { return };
        let indice = std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config/br.org.sicro.desktop/case-index.json"));
        let Some(indice) = indice.and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else {
            eprintln!("aviso: sem o índice de casos, nada conferido");
            return;
        };
        let mut videos = Vec::new();
        for e in indice["entries"].as_array().cloned().unwrap_or_default() {
            let ws = std::path::PathBuf::from(e["workspace_path"].as_str().unwrap_or_default());
            let Some(db) = ["Sistema/sicro.sqlite", "sicro.sqlite"].iter().map(|c| ws.join(c)).find(|p| p.is_file()) else { continue };
            // `immutable=1`: sem trava e sem arquivo -wal ou -shm novo na pasta do caso
            let uri = format!("file:{}?mode=ro&immutable=1", db.to_string_lossy().replace('%', "%25").replace('?', "%3f").replace('#', "%23"));
            let Ok(con) = Connection::open_with_flags(uri, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI) else { continue };
            let Ok(mut st) = con.prepare("select relative_path from video_media") else { continue };
            let relativos: Vec<String> = st.query_map([], |l| l.get(0)).map(|it| it.flatten().collect()).unwrap_or_default();
            for rel in relativos {
                let nome = Path::new(&rel).file_name().map(std::path::PathBuf::from).unwrap_or_default();
                let candidatos = [ws.join(&rel), ws.join("Mídias/Vídeos").join(&nome), ws.join("videos/originais").join(&nome)];
                videos.extend(candidatos.into_iter().find(|p| p.is_file()));
            }
        }
        eprintln!("vídeos dos casos ({}): {} arquivos", cap.versao, videos.len());
        eprintln!(" #  quadros        tb  passo mín/med/máx  lac  r_frame_rate  desc. (pacotes)  desc. (decodificador)  nominal  desc. na nominal");
        let (mut lidos, mut total, mut total_pacotes, mut total_nominal, mut usados) = (0u64, 0u64, 0u64, 0u64, 0);
        for (i, v) in videos.iter().enumerate() {
            let (Ok(pacotes), Ok(tab)) = (TabelaQuadros::de_pacotes(&cap, v), TabelaQuadros::do_decodificador(&cap, v, &AtomicBool::new(false), &mut |_| {})) else {
                eprintln!("{:2}  ilegível", i + 1);
                continue;
            };
            if tab.pts.len() < 3 {
                continue;
            }
            // "30/1", "30000/1001"
            let partes: Vec<i64> = tab.declarado.r_frame_rate.split('/').filter_map(|x| x.parse().ok()).collect();
            let [num, den] = partes[..] else { continue };
            if num < den || num > 120 * den {
                continue;
            }
            let (d, dp) = (descartados_a_1x(&tab, r(num, den)).unwrap(), descartados_a_1x(&pacotes, r(num, den)).unwrap());
            let nominal = tab.taxa_nominal();
            let dn = descartados_a_1x(&tab, nominal).unwrap();
            eprintln!(
                "{:2} {:8} {:>9} {:>18} {:4} {:>13} {:16} {:22} {:>8} {:17}",
                i + 1, tab.pts.len(), format!("{}/{}", tab.tb.num, tab.tb.den), format!("{}/{}/{}", tab.passo.0, tab.passo_mediano, tab.passo.1),
                tab.lacunas.len(), tab.declarado.r_frame_rate, dp, d, format!("{}/{}", nominal.num, nominal.den), dn
            );
            lidos += tab.pts.len() as u64;
            total += d;
            total_pacotes += dp;
            total_nominal += dn;
            usados += 1;
        }
        eprintln!("total: {usados} vídeos, {lidos} quadros; descartados a 1× na r_frame_rate: {total} (decodificador), {total_pacotes} (pacotes); na taxa nominal: {total_nominal}");
        assert!(total <= 2 && total_pacotes <= 2, "descartados na própria taxa: {total} e {total_pacotes}");
    }
}
