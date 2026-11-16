//! "Conferir ocultações" (§5.10, §10): os intervalos ocultados no ARQUIVO exportado, em tempo do arquivo, com os
//! trechos fracos (quadros `incerto`, interpolados e os sem região numa ausência aceita, fora da invertida) primeiro.
//! Lê só o retrato da saída (O7). Dono: P2.6.
//!
//! Do retrato: o `projeto.json` (ocultações, rastreios, densos em `dados/`) e o `feito` do relatório técnico (mapa de
//! quadros de cada clipe, em inteiros, e o slot na saída). O retrato não guarda a tabela de quadros: com passo
//! constante (o comum) os quadros do clipe são `pts_primeiro + k·passo` e o resultado é exato; com passo variável,
//! exato com a tabela do decodificador do cache do caso (`trechos_com_cache`); sem ela, os quadros são aproximados
//! (pontas das lacunas, pts conhecidos do denso e das âncoras) e cada faixa ganha 1 slot de cada lado.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::executar::{self, sha256_bytes, sha_valido};
use super::ffmpeg::sonda;
use super::fila::{self, PENDENTE};
use super::mensagens::codigo;
use super::modelo::{ClipePrincipal, ConteudoOculto, Efeito, EfOcultar, ProjetoEdicao, Racional, Rastreio, Tolerante};
use super::motor::clipe::{efeitos_do_congelado, ocultacoes_de};
use super::motor::efeitos::{base_ativa, realce};
use super::quadros::{OrigemTabela, TabelaQuadros};
use super::motor::plano::{ProvMapa, ProvNaSaida};
use super::motor::LeitorRastreios;
use super::rastreio::avaliar::{self, Como, Instante};
use super::rastreio::arquivo::{self, MAX_BYTES_DENSO};
use super::rastreio::{ArquivoDenso, ArquivoSilhuetas};
use crate::database::repositories::video_edit_output_repo;
use crate::error::{Result, SicroError};
use crate::workspace::paths;

/// Como o quadro fraco foi coberto; `Ausencia` = não foi: o rastreio não dá região e o perito aceitou a ausência do
/// alvo (se ele aceitou por engano, o alvo está à vista no arquivo). Na invertida o quadro sem região sai todo coberto
/// e não é fraco.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComoFraco {
    Incerto,
    Interpolado,
    // CONTRATO: acréscimo do fecho da Fase 2 (espelho em `src/types/edicao.ts`).
    Ausencia,
}

/// Faixa de quadros fracos, em segundos do arquivo exportado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaixaFraca {
    pub de_s: f64,
    pub ate_s: f64,
    pub como: ComoFraco,
}

/// Um intervalo a conferir de uma ocultação no arquivo (segundos do arquivo, `[de_s, ate_s)`): onde ela aparece e onde
/// uma ausência aceita a deixa sem região (faixa fraca `Ausencia`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrechoConferencia {
    pub efeito: String,
    pub conteudo: ConteudoOculto,
    pub de_s: f64,
    pub ate_s: f64,
    pub fracos: Vec<FaixaFraca>,
}

fn erro(m: impl std::fmt::Display) -> SicroError {
    SicroError::Validation(m.to_string())
}

/// Trechos da saída `saida_id` (estado "a conferir" ou concluída), ordenados: com fracos primeiro, depois por
/// tempo. Saída sem ocultação = lista vazia.
pub fn trechos(ws: &Path, saida_id: &str) -> Result<Vec<TrechoConferencia>> {
    trechos_com_cache(ws, saida_id, None)
}

// CONTRATO: acréscimo. O retrato não guarda a tabela de quadros: com passo variável os quadros do clipe saem
// aproximados. Com o cache do caso (`app_cache_dir()/edicao/<caso>`), a tabela do decodificador que a exportação
// usou dá os quadros exatos; o comando passa o cache (para o integrador).
/// `trechos` com o cache do caso; a tabela só é lida do cache (nada é medido aqui).
pub(crate) fn trechos_com_cache(ws: &Path, saida_id: &str, cache: Option<&Path>) -> Result<Vec<TrechoConferencia>> {
    let ausente = || erro(format!("{}: {saida_id}", codigo::SAIDA_AUSENTE));
    let id = uuid::Uuid::parse_str(saida_id).map_err(|_| ausente())?;
    let (conn, occ) = executar::abrir_banco(ws)?;
    let s = video_edit_output_repo::obter(&conn, &occ, &id)?.ok_or_else(ausente)?;
    // confere o SHA-256 de cada item do retrato (projeto, densos, pendente) antes de ler
    let (projeto, _) = fila::ler_retrato(ws, &s.projeto_id, saida_id)?;
    let retrato = fila::pasta_retrato(ws, &s.projeto_id, saida_id)?;
    let pendente = retrato.join(PENDENTE);
    let tecnico = if pendente.is_file() {
        std::fs::read(&pendente)?
    } else {
        // concluída: o relatório final, na pasta da saída
        let rel = s.tecnico_rel.as_deref().ok_or_else(|| erro(format!("{}: tecnico.json", codigo::ARQUIVO_AUSENTE)))?;
        std::fs::read(paths::resolver_relativo(ws, rel)).map_err(|_| erro(format!("{}: tecnico.json", codigo::ARQUIVO_AUSENTE)))?
    };
    let tecnico: Value = serde_json::from_slice(&tecnico)?;
    let p: ProjetoEdicao = serde_json::from_value(projeto).map_err(|e| erro(format!("{}: {e}", codigo::ESQUEMA)))?;
    let cap = std::cell::OnceCell::new();
    let tabelas = |sha: &str| -> Option<Arc<TabelaQuadros>> { tabela_do_cache(cache?, cap.get_or_init(|| sonda::capacidades().ok()).as_deref()?, sha) };
    calcular_com(&p, &tecnico, &DoRetrato { pasta: retrato }, &tabelas)
}

/// Tabela do decodificador da mídia `sha` que já está no cache (ou na memória); None sem ela. A de pacotes não serve.
fn tabela_do_cache(cache: &Path, cap: &sonda::Capacidades, sha: &str) -> Option<Arc<TabelaQuadros>> {
    if !sha_valido(sha) {
        return None;
    }
    // caminho vazio: sem a tabela no cache, `obter_com` não acha o vídeo e falha, sem medir nada
    TabelaQuadros::obter_com(cap, cache, sha, Path::new("")).ok().filter(|t| t.origem == OrigemTabela::Decodificador)
}

/// Densos do retrato: `dados/<sha256>.json`, conferidos pelo nome.
struct DoRetrato {
    pasta: PathBuf,
}

impl LeitorRastreios for DoRetrato {
    fn denso(&self, rastreio: &str, sha256: &str) -> Result<Arc<ArquivoDenso>> {
        let ausente = || erro(format!("{}: {rastreio}", codigo::RASTREIO_AUSENTE));
        if !sha_valido(sha256) {
            return Err(ausente());
        }
        // a regra do `arquivo::ler` (teto, SHA-256, colunas), sobre o `dados/` do retrato
        let f = std::fs::File::open(self.pasta.join("dados").join(format!("{sha256}.json"))).map_err(|_| ausente())?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(f, MAX_BYTES_DENSO + 1), &mut bytes).map_err(|_| ausente())?;
        if bytes.len() as u64 > MAX_BYTES_DENSO || sha256_bytes(&bytes) != sha256 {
            return Err(ausente());
        }
        let d: ArquivoDenso = serde_json::from_slice(&bytes).map_err(|e| erro(format!("{}: {e}", codigo::RASTREIO_AUSENTE)))?;
        arquivo::conferir(&d).map_err(|e| erro(format!("{}: {rastreio}: {e}", codigo::RASTREIO_AUSENTE)))?;
        Ok(Arc::new(d))
    }

    fn silhuetas(&self, _: &str, _: &str) -> Result<Arc<ArquivoSilhuetas>> {
        Err(erro(format!("{}: silhuetas", codigo::NAO_SUPORTADO)))
    }
}

// ---------------------------------------------------------------- o que se lê do `feito`

#[derive(Deserialize)]
struct Feito {
    saida: SaidaF,
    #[serde(default)]
    midias: Vec<MidiaF>,
    #[serde(default)]
    roteiro: Vec<ItemF>,
}

#[derive(Deserialize)]
struct SaidaF {
    fps: Racional,
}

#[derive(Deserialize)]
struct MidiaF {
    id: String,
    medido: MedidoF,
}

#[derive(Deserialize)]
struct MedidoF {
    tb: Racional,
    passo_min: i64,
    passo_max: i64,
    #[serde(default)]
    lacunas: Vec<(i64, i64)>,
}

#[derive(Deserialize)]
struct ItemF {
    clipe: String,
    na_saida: ProvNaSaida,
    origem: Option<OrigemF>,
}

#[derive(Deserialize)]
struct OrigemF {
    midia: String,
    pts_primeiro: i64,
    pts_ultimo: i64,
    lidos: u32,
    mapa: ProvMapa,
    #[serde(default)]
    campos: bool,
}

/// Estado da ocultação num quadro: None = fora dela; Some(None) = cobre; Some(Some(k)) = fraco (cobre, ou sem região
/// numa ausência aceita).
type Estado = Option<Option<ComoFraco>>;

/// Faixas de slots do arquivo `[de, ate)` em que cada ocultação aparece, e as fracas.
#[derive(Default)]
struct Faixas {
    conteudo: Option<ConteudoOculto>,
    cobertas: Vec<(u64, u64)>,
    fracas: Vec<(u64, u64, ComoFraco)>,
}

/// Os trechos a partir do projeto do retrato e do relatório técnico (`feito`), sem tabela de quadros.
#[cfg(test)]
pub(crate) fn calcular(p: &ProjetoEdicao, tecnico: &Value, densos: &dyn LeitorRastreios) -> Result<Vec<TrechoConferencia>> {
    calcular_com(p, tecnico, densos, &|_| None)
}

/// Tabela de quadros do decodificador pelo SHA-256 da mídia; None = não há (os quadros saem do retrato).
pub(crate) type Tabelas<'a> = &'a dyn Fn(&str) -> Option<Arc<TabelaQuadros>>;

/// pts dos `n` quadros do clipe pela tabela, se ela é a da exportação (mesmos passos e lacunas, mesmo primeiro e
/// último quadro); com desentrelace por campo, a tabela de campos do clipe.
fn quadros_da_tabela(t: &TabelaQuadros, m: &MedidoF, o: &OrigemF, efeitos: &[Tolerante<Efeito>]) -> Option<Vec<i64>> {
    if t.tb != m.tb || t.passo != (m.passo_min, m.passo_max) || t.lacunas != m.lacunas {
        return None;
    }
    let tab = if o.campos { realce::tabela(efeitos, t) } else { Cow::Borrowed(t) };
    let n = o.lidos as usize;
    let j = tab.pts.binary_search(&o.pts_primeiro).ok()?;
    let q = tab.pts.get(j..j.checked_add(n)?)?;
    (n > 0 && q[n - 1] == o.pts_ultimo).then(|| q.to_vec())
}

/// Os trechos a partir do projeto do retrato e do relatório técnico (`feito`). Puro (fora os densos e as tabelas
/// pedidos).
pub(crate) fn calcular_com(p: &ProjetoEdicao, tecnico: &Value, densos: &dyn LeitorRastreios, tabelas: Tabelas) -> Result<Vec<TrechoConferencia>> {
    let feito: Feito = serde_json::from_value(tecnico.get("feito").cloned().unwrap_or(Value::Null)).map_err(|e| erro(format!("{}: feito: {e}", codigo::ESQUEMA)))?;
    let fps = feito.saida.fps;
    if fps.num <= 0 || fps.den <= 0 {
        return Err(erro(format!("{}: fps", codigo::ESQUEMA)));
    }
    let mut tabs: HashMap<String, Option<Arc<TabelaQuadros>>> = HashMap::new();
    let mut lidos: HashMap<(String, String), Arc<ArquivoDenso>> = HashMap::new();
    let mut por_efeito: BTreeMap<String, Faixas> = BTreeMap::new();
    for it in &feito.roteiro {
        let Some(o) = &it.origem else { continue };
        let clipe = p.principal.iter().find_map(|c| match c {
            Tolerante::Conhecido(c @ (ClipePrincipal::Trecho(_) | ClipePrincipal::Congelar(_))) if id_do_clipe(c) == it.clipe => Some(c),
            _ => None,
        });
        let Some(clipe) = clipe else { return Err(erro(format!("{}: clipe {} do relatório fora do projeto", codigo::ESQUEMA, it.clipe))) };
        let medido = &feito.midias.iter().find(|m| m.id == o.midia).ok_or_else(|| erro(format!("{}: mídia {}", codigo::ESQUEMA, o.midia)))?.medido;
        let midia = p.midias.iter().find(|m| m.id == o.midia);
        let efeitos: Cow<[Tolerante<Efeito>]> = match clipe {
            ClipePrincipal::Congelar(g) => Cow::Owned(efeitos_do_congelado(&g.base.efeitos, it.na_saida.n_q, fps)?),
            ClipePrincipal::Trecho(t) => Cow::Borrowed(&t.base.efeitos[..]),
            _ => continue,
        };
        let congelado = matches!(clipe, ClipePrincipal::Congelar(_));
        let ocultacoes = ocultacoes_de(midia, &efeitos);
        let quadros = match (midia, ocultacoes.is_empty() || congelado) {
            (Some(md), false) => tabs.entry(md.sha256.clone()).or_insert_with(|| tabelas(&md.sha256)).as_deref().and_then(|t| quadros_da_tabela(t, medido, o, &efeitos)),
            _ => None,
        };
        for oc in ocultacoes {
            let r = p.rastreios.iter().find(|r| r.id == oc.alvo.rastreio).ok_or_else(|| erro(format!("{}: {}", codigo::RASTREIO_AUSENTE, oc.alvo.rastreio)))?;
            let denso = match &r.denso {
                Some(d) => {
                    let chave = (r.id.clone(), d.sha256.clone());
                    if !lidos.contains_key(&chave) {
                        lidos.insert(chave.clone(), densos.denso(&r.id, &d.sha256)?);
                    }
                    lidos.get(&chave).cloned()
                }
                None => None,
            };
            let segmentos = segmentos_do_clipe(oc, r, denso.as_deref(), o, it.na_saida.n_q, medido, congelado, quadros.as_deref())?;
            let f = por_efeito.entry(oc.base.id.clone()).or_default();
            f.conteudo.get_or_insert(oc.conteudo);
            let de = it.na_saida.de_q;
            for (a, b, e) in segmentos {
                match e {
                    None => {}
                    Some(k) => {
                        f.cobertas.push((de + a, de + b));
                        if let Some(k) = k {
                            f.fracas.push((de + a, de + b, k));
                        }
                    }
                }
            }
        }
    }
    let seg = |q: u64| q as f64 * fps.den as f64 / fps.num as f64;
    let mut v = Vec::new();
    for (efeito, f) in por_efeito {
        let Some(conteudo) = f.conteudo else { continue };
        for (a, b) in unir(f.cobertas) {
            let mut fracos: Vec<FaixaFraca> = Vec::new();
            for k in [ComoFraco::Incerto, ComoFraco::Interpolado, ComoFraco::Ausencia] {
                let dela: Vec<(u64, u64)> = f.fracas.iter().filter(|x| x.2 == k).map(|x| (x.0.max(a), x.1.min(b))).filter(|x| x.0 < x.1).collect();
                fracos.extend(unir(dela).into_iter().map(|(x, y)| FaixaFraca { de_s: seg(x), ate_s: seg(y), como: k }));
            }
            fracos.sort_by(|x, y| x.de_s.total_cmp(&y.de_s));
            v.push(TrechoConferencia { efeito: efeito.clone(), conteudo, de_s: seg(a), ate_s: seg(b), fracos });
        }
    }
    v.sort_by(|x, y| x.fracos.is_empty().cmp(&y.fracos.is_empty()).then(x.de_s.total_cmp(&y.de_s)).then(x.efeito.cmp(&y.efeito)));
    Ok(v)
}

fn id_do_clipe(c: &ClipePrincipal) -> &str {
    match c {
        ClipePrincipal::Trecho(t) => &t.base.id,
        ClipePrincipal::Congelar(g) => &g.base.id,
        _ => "",
    }
}

/// Faixas ordenadas que se tocam ou se cruzam viram uma.
fn unir(mut v: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    v.retain(|x| x.0 < x.1);
    v.sort_unstable();
    let mut u: Vec<(u64, u64)> = Vec::with_capacity(v.len());
    for (a, b) in v {
        match u.last_mut() {
            Some(x) if a <= x.1 => x.1 = x.1.max(b),
            _ => u.push((a, b)),
        }
    }
    u
}

fn teto(a: i128, b: i128) -> i128 {
    let q = a / b;
    if a % b != 0 && ((a < 0) == (b < 0)) {
        q + 1
    } else {
        q
    }
}

/// pts aproximados dos quadros do clipe sem a tabela (passo variável). As pontas de cada lacuna da mídia são quadros
/// reais: cada pedaço entre elas ganha quadros no passo médio fora das lacunas, e cada quadro se prende a um pts
/// conhecido (âncora, denso) a menos de meio passo; nunca voltam no tempo. Sem a lacuna, o quadro mostrado durante
/// ela (o último antes) saía da conferência.
fn quadros_aproximados(o: &OrigemF, m: &MedidoF, conhecidos: &[i64]) -> Vec<i64> {
    let (a, b) = (o.pts_primeiro, o.pts_ultimo);
    let mut pedacos: Vec<(i64, i64)> = Vec::new();
    let mut ini = a;
    for &(x, y) in m.lacunas.iter().filter(|l| l.0 >= a && l.1 <= b && l.0 < l.1) {
        if x >= ini {
            pedacos.push((ini, x));
            ini = y;
        }
    }
    pedacos.push((ini, b));
    let duracao: i64 = pedacos.iter().map(|p| p.1 - p.0).sum();
    let passos = (o.lidos as usize).saturating_sub(pedacos.len()).max(1);
    let medio = if duracao > 0 { duracao as f64 / passos as f64 } else { 1.0 };
    let mut v: Vec<i64> = Vec::with_capacity(o.lidos as usize + pedacos.len());
    for (s, e) in pedacos {
        let n = if e > s { ((e - s) as f64 / medio).round().max(1.0) as i64 } else { 0 };
        let passo = if n > 0 { (e - s) as f64 / n as f64 } else { 1.0 };
        for k in 0..=n {
            let g = s + ((e - s) as f64 * k as f64 / n.max(1) as f64).round() as i64;
            let i = conhecidos.partition_point(|&p| p < g);
            let perto = [i.checked_sub(1), Some(i)].into_iter().flatten().filter_map(|i| conhecidos.get(i).copied()).min_by_key(|p| (p - g).abs());
            let q = match perto {
                Some(p) if ((p - g).abs() as f64) * 2.0 < passo => p,
                _ => g,
            };
            v.push(q.max(v.last().copied().unwrap_or(i64::MIN)));
        }
    }
    v
}

/// Segmentos `[de, ate)` (slots do clipe, `0..n_q`) com o estado da ocultação, na ordem. `exatos` = pts dos quadros
/// do clipe pela tabela do decodificador; sem ela, passo constante é exato e o variável, aproximado.
#[allow(clippy::too_many_arguments)]
fn segmentos_do_clipe(oc: &EfOcultar, r: &Rastreio, denso: Option<&ArquivoDenso>, o: &OrigemF, n_q: u32, m: &MedidoF, congelado: bool, exatos: Option<&[i64]>) -> Result<Vec<(u64, u64, Estado)>> {
    let (tn, td) = (m.tb.num, m.tb.den);
    if tn <= 0 || td <= 0 {
        return Err(erro(format!("{}: base de tempo", codigo::ESQUEMA)));
    }
    let n_q = n_q as u64;
    let estado = |pts: i64| -> Estado {
        let t = pts as f64 * tn as f64 / td as f64;
        if !base_ativa(&oc.base, t) {
            return None;
        }
        match avaliar::avaliar(r, denso, Instante { t, pts: Some(pts) }) {
            Some(a) => Some(match (a.incerto, a.como) {
                (true, _) => Some(ComoFraco::Incerto),
                (false, Como::Interpolado) => Some(ComoFraco::Interpolado),
                _ => None,
            }),
            // sem região dentro do `ativo`: só numa ausência aceita (fora dela o motor recusa, `sem_cobertura`); a
            // invertida cobre ali o quadro inteiro e não é fraca
            None if avaliar::em_ausencia(r, t) => Some((!oc.alvo.inverter).then_some(ComoFraco::Ausencia)),
            None => None,
        }
    };
    let n = o.lidos as usize;
    // um quadro só (congelado, trecho de um quadro): vale o clipe inteiro
    if congelado || n <= 1 {
        return Ok(vec![(0, n_q, estado(o.pts_primeiro))]);
    }
    let mp = &o.mapa;
    let r_ = (tn as i128).checked_mul(mp.c as i128).filter(|v| mp.kd > 0 && v % td as i128 == 0).ok_or_else(|| erro(format!("{}: mapa", codigo::ESQUEMA)))? / td as i128;
    let p0 = o.pts_primeiro as i128 * r_;
    // primeiro slot do quadro de pts `pts` (§3.1: teto(((P − P_jin)·Kn − E) / Kd)), no clipe
    let slot = |pts: i64| -> u64 { teto((pts as i128 * r_ - p0) * mp.kn as i128 - mp.e as i128, mp.kd as i128).clamp(0, n_q as i128) as u64 };
    // por campo (desentrelace) os pts são de campo, metade do passo do quadro
    let passo = if o.campos { (m.passo_min % 2 == 0).then_some(m.passo_min / 2) } else { Some(m.passo_min) };
    let total = o.pts_ultimo as i128 - o.pts_primeiro as i128;
    let constante = m.passo_min == m.passo_max && m.lacunas.is_empty() && passo.is_some_and(|s| s > 0 && total == s as i128 * (n as i128 - 1));
    let exata = constante || exatos.is_some_and(|q| q.len() == n);
    // pts exatos conhecidos (âncoras e denso): o quadro aproximado se prende a eles
    let mut conhecidos: Vec<i64> = r.ancoras.iter().filter_map(|a| a.pts).chain(denso.map(|d| d.pts.clone()).unwrap_or_default()).filter(|p| (o.pts_primeiro..=o.pts_ultimo).contains(p)).collect();
    conhecidos.sort_unstable();
    conhecidos.dedup();
    let medio = total as f64 / (n - 1) as f64;
    // por campo vale a paridade do índice: um quadro por campo, passo uniforme
    let aproximados = if exata || o.campos { Vec::new() } else { quadros_aproximados(o, m, &conhecidos) };
    let quadro = |k: usize| -> i64 {
        if let Some(q) = exatos.filter(|q| q.len() == n) {
            return q[k];
        }
        if constante {
            return o.pts_primeiro + k as i64 * passo.unwrap_or(0);
        }
        if !o.campos {
            return aproximados[k];
        }
        let g = o.pts_primeiro + (k as f64 * medio).round() as i64;
        let i = conhecidos.partition_point(|&p| p < g);
        let perto = [i.checked_sub(1), Some(i)].into_iter().flatten().filter_map(|i| conhecidos.get(i).copied()).min_by_key(|p| (p - g).abs());
        match perto {
            Some(p) if ((p - g).abs() as f64) * 2.0 < medio => p,
            _ => g,
        }
    };
    let quantos = if exata || o.campos { n } else { aproximados.len() };
    let mut seg: Vec<(u64, u64, Estado)> = Vec::new();
    let mut anterior = o.pts_primeiro;
    for k in 0..quantos {
        let pts = quadro(k);
        // campo ímpar: segundo campo do quadro anterior (a ocultação é por quadro da fonte)
        let aval = if o.campos && k % 2 == 1 { anterior } else { pts };
        if !(o.campos && k % 2 == 1) {
            anterior = pts;
        }
        let e = estado(aval);
        let de = if k == 0 { 0 } else { slot(pts) };
        match seg.last_mut() {
            Some(s) if s.2 == e => {}
            Some(s) => {
                // pts aproximado que volta (preso a um conhecido) não encurta o segmento anterior
                let de = de.max(s.0);
                s.1 = de;
                seg.push((de, n_q, e));
            }
            None => seg.push((de, n_q, e)),
        }
    }
    if !exata {
        // quadros aproximados: a faixa coberta (e a fraca) ganha 1 slot de cada lado
        let mut largos = Vec::with_capacity(seg.len());
        for (a, b, e) in seg {
            if a < b {
                largos.push(if e.is_some() { (a.saturating_sub(1), (b + 1).min(n_q), e) } else { (a, b, e) });
            }
        }
        return Ok(largos);
    }
    Ok(seg.into_iter().filter(|s| s.0 < s.1).collect())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::executar::{apoio as ex, Ambiente, Resultado};
    use crate::edicao::motor::plano::Plano;
    use crate::edicao::quadros::apoio::{self, SemRastreios};
    use crate::edicao::relatorio::tecnico::{self, Fecho};
    use crate::edicao::testes::aceite::{alvo_final, ancora, midia, ocultar, projeto, rastreio, trecho, Mundo};
    use crate::edicao::testes::fontes::{self, Fonte};
    use crate::edicao::testes::{executor, medir};
    use serde_json::json;

    const TB: (i64, i64) = (1, 15360);

    /// `feito` do relatório técnico do plano, como a exportação grava.
    fn feito(plano: &Plano, cap: Arc<crate::edicao::ffmpeg::sonda::Capacidades>) -> Value {
        let d = tempfile::tempdir().unwrap();
        let amb = ex::ambiente(cap, d.path(), "t");
        let fecho = Fecho { projeto_sha256: "0".repeat(64), arquivo_rel: "saida.mp4".into(), ocultacoes: None };
        json!({ "feito": tecnico::feito(plano, &amb, &fecho).unwrap() })
    }

    fn congelar(id: &str, tab: &crate::edicao::quadros::TabelaQuadros, j: usize, duracao_s: f64) -> Value {
        json!({ "tipo": "congelar", "id": id, "rotulo": "", "midia": "m1", "instante_s": tab.tempo(j), "instante_pts": tab.pts[j], "duracao_s": duracao_s, "efeitos": [] })
    }

    /// Faixa em slots a 30 qps.
    fn em_slots(de_s: f64, ate_s: f64) -> (u64, u64) {
        ((de_s * 30.0).round() as u64, (ate_s * 30.0).round() as u64)
    }

    /// O projeto dos testes: ocultação da mídia `o1` com âncoras em `ancoras` e ativa em [40, 100]; trecho [30, 120) a
    /// 1×, repetição [50, 90) a 0,5× e o quadro 70 congelado (0,4 s); `o2` (rosto) a partir do quadro 110.
    fn projeto_do_teste(tab: &crate::edicao::quadros::TabelaQuadros, ancoras: &[usize], cor: &str) -> ProjetoEdicao {
        let caixa = |j: usize| fontes::marca_esperada(Fonte::Mp4, j as u32);
        let o1 = ocultar("o1", "r1", "solido", json!({ "cor": cor, "conteudo": "placa", "ativo": { "de": tab.tempo(40), "ate": tab.tempo(100) } }));
        let o2 = ocultar("o2", "r2", "mosaico", json!({ "conteudo": "rosto", "intensidade": 0.75, "ativo": { "de": tab.tempo(110), "ate": null } }));
        projeto(
            320,
            180,
            (30, 1),
            vec![midia("m1", "video", json!([o1, o2]))],
            vec![trecho("c1", "m1", tab, 30, 120, 1.0, json!([])), trecho("c2", "m1", tab, 50, 90, 0.5, json!([])), congelar("c3", tab, 70, 0.4)],
            json!({ "rastreios": [
                rastreio("r1", Some("m1"), "fonte", ancoras.iter().map(|&j| ancora(tab.tempo(j), Some(tab.pts[j]), caixa(j))).collect()),
                rastreio("r2", Some("m1"), "fonte", vec![ancora(tab.tempo(110), Some(tab.pts[110]), [0.1, 0.1, 0.2, 0.2])]),
            ] }),
        )
    }

    /// Âncoras de `o1`: a cada quadro de 40 a 60 e no 80 (61..79 interpolados).
    fn ancoras_o1() -> Vec<usize> {
        (40..=60).chain([80]).collect()
    }

    /// Faixas esperadas (slots do arquivo) de `o1` e `o2` no projeto do teste: c1 ocupa 0..90 (quadro j no slot j − 30),
    /// c2 90..170 (quadro j nos slots 90 + 2(j − 50) e o seguinte), c3 170..182.
    fn esperado() -> Vec<(&'static str, (u64, u64), Vec<(u64, u64)>)> {
        vec![("o1", (10, 71), vec![(31, 50)]), ("o1", (90, 182), vec![(112, 150), (170, 182)]), ("o2", (80, 90), vec![])]
    }

    fn conferir(v: &[TrechoConferencia], rotulo: &str) {
        let achado: Vec<(&str, (u64, u64), Vec<(u64, u64)>)> =
            v.iter().map(|t| (t.efeito.as_str(), em_slots(t.de_s, t.ate_s), t.fracos.iter().map(|f| em_slots(f.de_s, f.ate_s)).collect())).collect();
        assert_eq!(achado, esperado(), "{rotulo}");
        assert!(v[..2].iter().all(|t| t.fracos.iter().all(|f| f.como == ComoFraco::Interpolado)));
        assert_eq!((v[0].conteudo, v[2].conteudo), (ConteudoOculto::Placa, ConteudoOculto::Rosto));
    }

    /// Fracos primeiro, depois por tempo; a repetição e o congelado aparecem com a faixa deles; trechos vizinhos da
    /// mesma ocultação viram um. Pelo compilador de verdade (tabela sintética, sem ffmpeg).
    #[test]
    fn fracos_primeiro() {
        let tab = apoio::tabela(TB, apoio::pts_regulares(300, (1, 30), TB, 0));
        let cap = Arc::new(apoio::cap_falsa());
        let p = projeto_do_teste(&tab, &ancoras_o1(), "#00ff00ff");
        let plano = Mundo::novo(&cap).com("m1", &apoio::fonte(tab.clone(), (320, 180))).compilar(&p, &alvo_final("cpu")).unwrap();
        let v = calcular(&p, &feito(&plano, cap.clone()), &SemRastreios).unwrap();
        conferir(&v, "tabela de passo constante");
        assert_eq!((v[0].de_s, v[0].ate_s), (10.0 / 30.0, 71.0 / 30.0));
        // passo variável (±1 tique, MKV de 1 ms): faixas com 1 slot de folga de cada lado, nunca menos
        let tb_ms = (1, 1000);
        let pts: Vec<i64> = (0..300i64).map(|k| (k * 1000 + 15) / 30).collect();
        let tab = apoio::tabela(tb_ms, pts);
        let p = projeto_do_teste(&tab, &ancoras_o1(), "#00ff00ff");
        let plano = Mundo::novo(&cap).com("m1", &apoio::fonte(tab.clone(), (320, 180))).compilar(&p, &alvo_final("cpu")).unwrap();
        let v = calcular(&p, &feito(&plano, cap), &SemRastreios).unwrap();
        for ((efeito, (a, b), fr), t) in esperado().into_iter().zip(&v) {
            let (x, y) = em_slots(t.de_s, t.ate_s);
            assert!(t.efeito == efeito && x <= a && x + 1 >= a && y >= b && y <= b + 1, "{efeito}: {x}..{y} × {a}..{b}");
            for ((fa, fb), f) in fr.into_iter().zip(&t.fracos) {
                let (x, y) = em_slots(f.de_s, f.ate_s);
                assert!(x <= fa && x + 1 >= fa && y >= fb && y <= fb + 1, "{efeito} fraco: {x}..{y} × {fa}..{fb}");
            }
        }
        // sem ocultação, nada
        let mut p = p;
        p.midias[0].ocultacoes.clear();
        let plano = Mundo::novo(&Arc::new(apoio::cap_falsa())).com("m1", &apoio::fonte(tab, (320, 180))).compilar(&p, &alvo_final("cpu")).unwrap();
        assert!(calcular(&p, &feito(&plano, Arc::new(apoio::cap_falsa())), &SemRastreios).unwrap().is_empty());
    }

    /// Verdade por slot, do plano em memória (`quadro_do_slot`, que o retrato não guarda): por efeito, os slots do
    /// arquivo em que a ocultação aparece e os fracos.
    fn verdade(p: &ProjetoEdicao, plano: &Plano, tab: &crate::edicao::quadros::TabelaQuadros, densos: &dyn LeitorRastreios) -> BTreeMap<String, (Vec<u64>, Vec<u64>)> {
        let fps = plano.proveniencia.saida.fps;
        let mut v: BTreeMap<String, (Vec<u64>, Vec<u64>)> = BTreeMap::new();
        for it in &plano.proveniencia.roteiro {
            let Some(o) = &it.origem else { continue };
            let clipe = p.principal.iter().find_map(|c| match c {
                Tolerante::Conhecido(c) if id_do_clipe(c) == it.clipe => Some(c),
                _ => None,
            });
            let efeitos: Vec<Tolerante<Efeito>> = match clipe.unwrap() {
                ClipePrincipal::Congelar(g) => efeitos_do_congelado(&g.base.efeitos, it.na_saida.n_q, fps).unwrap(),
                ClipePrincipal::Trecho(t) => t.base.efeitos.clone(),
                _ => continue,
            };
            let midia = p.midias.iter().find(|m| m.id == o.midia);
            for oc in ocultacoes_de(midia, &efeitos) {
                let r = p.rastreios.iter().find(|r| r.id == oc.alvo.rastreio).unwrap();
                let d = r.denso.as_ref().map(|d| densos.denso(&r.id, &d.sha256).unwrap());
                let e = v.entry(oc.base.id.clone()).or_default();
                for (q, &j) in o.quadro_do_slot.iter().enumerate() {
                    let (pts, t) = (tab.pts[j as usize], tab.tempo(j as usize));
                    if !base_ativa(&oc.base, t) {
                        continue;
                    }
                    match avaliar::avaliar(r, d.as_deref(), Instante { t, pts: Some(pts) }) {
                        Some(a) => {
                            e.0.push(it.na_saida.de_q + q as u64);
                            if a.incerto || a.como == Como::Interpolado {
                                e.1.push(it.na_saida.de_q + q as u64);
                            }
                        }
                        // ausência aceita: a conferir; fraca, salvo na invertida (o quadro inteiro coberto)
                        None if avaliar::em_ausencia(r, t) => {
                            e.0.push(it.na_saida.de_q + q as u64);
                            if !oc.alvo.inverter {
                                e.1.push(it.na_saida.de_q + q as u64);
                            }
                        }
                        None => {}
                    }
                }
            }
        }
        v
    }

    /// Passo variável com uma lacuna de 1 s (gravador que perdeu quadros): os trechos cobrem TODO slot em que a
    /// ocultação aparece no arquivo (nunca a menos), com no máximo 1 slot a mais em cada ponta.
    #[test]
    fn passo_variavel_com_lacuna_nunca_a_menos() {
        let pts: Vec<i64> = (0..300i64).map(|k| k * 512 + if k > 60 { 15360 } else { 0 }).collect();
        let tab = apoio::tabela(TB, pts);
        let p = projeto_do_teste(&tab, &ancoras_o1(), "#00ff00ff");
        let mut fim_no_60 = p.clone();
        // o `ativo` acaba no último quadro antes da lacuna: ele fica na tela a lacuna inteira
        fim_no_60.midias[0].ocultacoes[0].base.ativo.as_mut().unwrap().ate = Some(tab.tempo(60));
        for (caso, p) in [("ativo até depois da lacuna", p), ("ativo até o quadro antes da lacuna", fim_no_60)] {
            comparar_com_a_verdade(caso, &p, &tab, &|_| None, 1);
        }
    }

    /// Passo variável sem lacuna (30 qps e depois 20 qps): sem a tabela os quadros saem aproximados; com a tabela do
    /// decodificador (`trechos_com_cache`) os trechos são exatos, slot a slot. Tabela de outra mídia não é usada.
    #[test]
    fn passo_variavel_exato_com_a_tabela() {
        let pts: Vec<i64> = (0..300i64).map(|k| if k <= 100 { k * 512 } else { 100 * 512 + (k - 100) * 768 }).collect();
        let tab = apoio::tabela(TB, pts);
        assert!(tab.lacunas.is_empty() && tab.passo.0 != tab.passo.1);
        let p = projeto_do_teste(&tab, &ancoras_o1(), "#00ff00ff");
        let t = Arc::new(tab.clone());
        let sha = p.midias[0].sha256.clone();
        comparar_com_a_verdade("com a tabela", &p, &tab, &|s| (s == sha).then(|| t.clone()), 0);
        // tabela que não é a da exportação (outros passos): ignorada, e a aproximação não fica igual à verdade
        let outra = Arc::new(apoio::tabela(TB, apoio::pts_regulares(300, (1, 30), TB, 0)));
        let cap = Arc::new(apoio::cap_falsa());
        let plano = Mundo::novo(&cap).com("m1", &apoio::fonte(tab.clone(), (320, 180))).compilar(&p, &alvo_final("cpu")).unwrap();
        let f = feito(&plano, cap);
        let sem = calcular(&p, &f, &SemRastreios).unwrap();
        assert_eq!(calcular_com(&p, &f, &SemRastreios, &|_| Some(outra.clone())).unwrap(), sem);
        assert_ne!(calcular_com(&p, &f, &SemRastreios, &|_| Some(t.clone())).unwrap(), sem);
    }

    /// A tabela vem só do que já está no cache do caso, e só a do decodificador; sem ela, None (nada é medido).
    #[test]
    fn tabela_do_cache_so_a_do_decodificador() {
        let d = tempfile::tempdir().unwrap();
        let cap = apoio::cap_falsa();
        // SHA-256 só deste teste (a memória das tabelas é do processo)
        let sha = "26".repeat(32);
        assert!(tabela_do_cache(d.path(), &cap, &sha).is_none());
        let t = apoio::tabela(TB, apoio::pts_regulares(10, (1, 30), TB, 0));
        let guardada = t.guardar(d.path(), &sha).unwrap();
        assert_eq!(tabela_do_cache(d.path(), &cap, &sha).unwrap().pts, guardada.pts);
        assert!(tabela_do_cache(d.path(), &cap, "../26").is_none());
    }

    /// Passo variável com lacuna e rastreio DENSO (o caso do CFTV): quadros incertos soltos não somem da
    /// conferência sem a tabela. Denso gravado à mão, uma linha por quadro real.
    #[test]
    fn passo_variavel_com_denso_nunca_a_menos() {
        use crate::edicao::motor::efeitos::regiao::testes::denso::{compilar, gravado};
        use crate::edicao::rastreio::{ESTADO_INCERTO, ESTADO_OK, TIPO_MEDIDO};
        let pts: Vec<i64> = (0..300i64).map(|k| k * 512 + if k > 60 { 15360 } else { 0 }).collect();
        let tab = apoio::tabela(TB, pts);
        let (a, b) = (40usize, 110usize);
        let d = ArquivoDenso {
            esquema: 2,
            rastreio: "r1".into(),
            midia_sha256: "0".repeat(64),
            espaco: "original".into(),
            pixel: crate::edicao::modelo::TamanhoPx { w: 320, h: 180 },
            pts: tab.pts[a..b].to_vec(),
            x: vec![0.4; b - a],
            y: vec![0.4; b - a],
            w: vec![0.2; b - a],
            h: vec![0.2; b - a],
            conf: vec![0.9; b - a],
            estado: (a..b).map(|j| if [47, 60, 77, 91].contains(&j) { ESTADO_INCERTO } else { ESTADO_OK }).collect(),
            tipo: vec![TIPO_MEDIDO; b - a],
            corrida: vec![0; b - a],
            corridas: vec![],
        };
        let (sha, leitor) = gravado(&d, tab.tb);
        let mut p = projeto_do_teste(&tab, &[a], "#00ff00ff");
        p.rastreios[0] = serde_json::from_value(json!({ "id": "r1", "nome": "r1", "cor": "#ff0000", "midia": "m1", "base": "fonte", "tipo": "caixa", "espaco": "original",
            "ancoras": [ancora(tab.tempo(a), Some(tab.pts[a]), [0.4, 0.4, 0.2, 0.2])],
            "denso": { "sha256": sha, "n": b - a, "de_s": tab.tempo(a), "ate_s": tab.tempo(b - 1), "ok": 66, "incertos": 4, "ausentes": 0 },
            "silhuetas": null, "ausencias": [], "origem": { "tipo": "assistido" }, "confirmacao": null }))
        .unwrap();
        p.midias[0].ocultacoes[0].base.ativo.as_mut().unwrap().ate = Some(tab.tempo(b - 1));
        let cap = Arc::new(apoio::cap_falsa());
        let plano = compilar(&cap, &apoio::fonte(tab.clone(), (320, 180)), &p, &leitor, &alvo_final("cpu")).unwrap();
        comparar_plano("denso com lacuna", &p, &plano, &tab, &leitor, &|_| None, 1);
    }

    /// Os trechos de `calcular_com` × a verdade por slot: nenhum slot coberto (ou fraco) de fora; no máximo `folga`
    /// slots a mais em cada ponta de faixa.
    fn comparar_com_a_verdade(caso: &str, p: &ProjetoEdicao, tab: &crate::edicao::quadros::TabelaQuadros, tabelas: Tabelas, folga: usize) {
        let cap = Arc::new(apoio::cap_falsa());
        let plano = Mundo::novo(&cap).com("m1", &apoio::fonte(tab.clone(), (320, 180))).compilar(p, &alvo_final("cpu")).unwrap();
        comparar_plano(caso, p, &plano, tab, &SemRastreios, tabelas, folga);
    }

    fn comparar_plano(caso: &str, p: &ProjetoEdicao, plano: &Plano, tab: &crate::edicao::quadros::TabelaQuadros, densos: &dyn LeitorRastreios, tabelas: Tabelas, folga: usize) {
        let cap = Arc::new(apoio::cap_falsa());
        // segundos do arquivo → slot, na taxa da saída do plano (29,97 e 25 também)
        let fps = plano.proveniencia.saida.fps;
        let q = |s: f64| (s * fps.num as f64 / fps.den as f64).round() as u64;
        let slots = |faixas: Vec<(f64, f64)>| -> Vec<u64> {
            let mut s: Vec<u64> = faixas.into_iter().flat_map(|(a, b)| q(a)..q(b)).collect();
            s.sort_unstable();
            s.dedup();
            s
        };
        let v = calcular_com(p, &feito(plano, cap.clone()), densos, tabelas).unwrap();
        let certos = verdade(p, plano, tab, densos);
        assert_eq!(certos.keys().collect::<Vec<_>>(), ["o1", "o2"], "{caso}");
        for (efeito, (mut cobertos, mut fracos)) in certos {
            for s in [&mut cobertos, &mut fracos] {
                s.sort_unstable();
                s.dedup();
            }
            let dele: Vec<&TrechoConferencia> = v.iter().filter(|t| t.efeito == efeito).collect();
            let achados = slots(dele.iter().map(|t| (t.de_s, t.ate_s)).collect());
            let fracos_achados = slots(dele.iter().flat_map(|t| t.fracos.iter().map(|f| (f.de_s, f.ate_s))).collect());
            assert!(!cobertos.is_empty(), "{caso}, {efeito}: a verdade não cobre nada");
            for (nome, certo, achado) in [("cobertos", &cobertos, &achados), ("fracos", &fracos, &fracos_achados)] {
                let faltam: Vec<u64> = certo.iter().filter(|q| achado.binary_search(q).is_err()).copied().collect();
                assert!(faltam.is_empty(), "{caso}, {efeito} {nome}: slots do arquivo fora da conferência: {faltam:?}");
                let sobram: Vec<u64> = achado.iter().filter(|q| certo.binary_search(q).is_err()).copied().collect();
                let pontas = 2 * folga * unir(certo.iter().map(|&q| (q, q + 1)).collect()).len();
                assert!(sobram.len() <= pontas, "{caso}, {efeito} {nome}: slots a mais: {sobram:?}");
            }
        }
    }

    /// Trechos fracos pelo denso: linhas `incerto` e de `tipo` interpolado viram faixas "incerto" e "interpolado"
    /// (B24), na ordem do tempo; o resto do denso cobre sem fraco. Denso gravado à mão, leitor falso.
    #[test]
    fn fracos_do_denso() {
        use crate::edicao::motor::efeitos::regiao::testes::denso::{compilar, gravado};
        use crate::edicao::rastreio::{ESTADO_INCERTO, ESTADO_OK, TIPO_INTERPOLADO, TIPO_MEDIDO};
        let tab = apoio::tabela(TB, apoio::pts_regulares(300, (1, 30), TB, 0));
        let cap = Arc::new(apoio::cap_falsa());
        let (a, b) = (40usize, 100usize);
        let d = ArquivoDenso {
            esquema: 2,
            rastreio: "r1".into(),
            midia_sha256: "0".repeat(64),
            espaco: "original".into(),
            pixel: crate::edicao::modelo::TamanhoPx { w: 320, h: 180 },
            pts: tab.pts[a..b].to_vec(),
            x: vec![0.4; b - a],
            y: vec![0.4; b - a],
            w: vec![0.2; b - a],
            h: vec![0.2; b - a],
            conf: vec![0.9; b - a],
            estado: (a..b).map(|j| if (60..65).contains(&j) { ESTADO_INCERTO } else { ESTADO_OK }).collect(),
            tipo: (a..b).map(|j| if (70..75).contains(&j) { TIPO_INTERPOLADO } else { TIPO_MEDIDO }).collect(),
            corrida: vec![0; b - a],
            corridas: vec![],
        };
        let (sha, leitor) = gravado(&d, tab.tb);
        let r = json!({ "id": "r1", "nome": "r1", "cor": "#ff0000", "midia": "m1", "base": "fonte", "tipo": "caixa", "espaco": "original",
            "ancoras": [ancora(tab.tempo(a), Some(tab.pts[a]), [0.4, 0.4, 0.2, 0.2])],
            "denso": { "sha256": sha, "n": b - a, "de_s": tab.tempo(a), "ate_s": tab.tempo(b - 1), "ok": 55, "incertos": 5, "ausentes": 0 },
            "silhuetas": null, "ausencias": [], "origem": { "tipo": "assistido" }, "confirmacao": null });
        let o1 = ocultar("o1", "r1", "solido", json!({ "conteudo": "rosto", "ativo": { "de": tab.tempo(a), "ate": tab.tempo(b - 1) } }));
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([o1]))], vec![trecho("c1", "m1", &tab, 30, 120, 1.0, json!([]))], json!({ "rastreios": [r] }));
        let plano = compilar(&cap, &apoio::fonte(tab, (320, 180)), &p, &leitor, &alvo_final("cpu")).unwrap();
        let v = calcular(&p, &feito(&plano, cap), &leitor).unwrap();
        let achado: Vec<(u64, u64, Vec<(u64, u64, ComoFraco)>)> = v
            .iter()
            .map(|t| {
                let (x, y) = em_slots(t.de_s, t.ate_s);
                (x, y, t.fracos.iter().map(|f| (em_slots(f.de_s, f.ate_s).0, em_slots(f.de_s, f.ate_s).1, f.como)).collect())
            })
            .collect();
        assert_eq!(achado, vec![(10, 70, vec![(30, 35, ComoFraco::Incerto), (40, 45, ComoFraco::Interpolado)])]);
    }

    /// Fecho da Fase 2: a conferência leva também às ausências aceitas (se o perito aceitou uma por engano, o alvo
    /// está à vista ali): os quadros sem região dentro do `ativo` e de uma ausência aceita viram faixa fraca
    /// `Ausencia`, no mesmo trecho; ausente fora de ausência não chega à exportação. Passo constante (exato) e com
    /// lacuna (nunca a menos), contra a verdade por slot.
    #[test]
    fn ausencia_aceita_e_faixa_fraca() {
        use crate::edicao::motor::efeitos::regiao::testes::denso::{compilar, gravado};
        use crate::edicao::rastreio::{ESTADO_AUSENTE, ESTADO_INCERTO, ESTADO_OK, TIPO_MEDIDO};
        for (caso, pts) in [
            ("passo constante", (0..300i64).map(|k| k * 512).collect::<Vec<i64>>()),
            ("lacuna de 1 s", (0..300i64).map(|k| k * 512 + if k > 60 { 15360 } else { 0 }).collect()),
        ] {
            let tab = apoio::tabela(TB, pts);
            let cap = Arc::new(apoio::cap_falsa());
            let (a, b) = (40usize, 100usize);
            // incerto 60..64; ausente 70..79 e 85..87, as duas aceitas
            let estado = |j: usize| match j {
                60..=64 => ESTADO_INCERTO,
                70..=79 | 85..=87 => ESTADO_AUSENTE,
                _ => ESTADO_OK,
            };
            let d = ArquivoDenso {
                esquema: 2,
                rastreio: "r1".into(),
                midia_sha256: "0".repeat(64),
                espaco: "original".into(),
                pixel: crate::edicao::modelo::TamanhoPx { w: 320, h: 180 },
                pts: tab.pts[a..b].to_vec(),
                x: vec![0.4; b - a],
                y: vec![0.4; b - a],
                w: vec![0.2; b - a],
                h: vec![0.2; b - a],
                conf: vec![0.9; b - a],
                estado: (a..b).map(estado).collect(),
                tipo: vec![TIPO_MEDIDO; b - a],
                corrida: vec![0; b - a],
                corridas: vec![],
            };
            let (sha, leitor) = gravado(&d, tab.tb);
            let aus = |x: usize, y: usize| json!({ "de_s": tab.tempo(x), "ate_s": tab.tempo(y), "motivo": "oculto", "aceita_em": "2026-11-15T10:00:00Z", "por": "Perito" });
            let r = json!({ "id": "r1", "nome": "r1", "cor": "#ff0000", "midia": "m1", "base": "fonte", "tipo": "caixa", "espaco": "original",
                "ancoras": [ancora(tab.tempo(a), Some(tab.pts[a]), [0.4, 0.4, 0.2, 0.2])],
                "denso": { "sha256": sha, "n": b - a, "de_s": tab.tempo(a), "ate_s": tab.tempo(b - 1), "ok": 42, "incertos": 5, "ausentes": 13 },
                "silhuetas": null, "ausencias": [aus(70, 79), aus(85, 87)], "origem": { "tipo": "assistido" }, "confirmacao": null });
            let o1 = ocultar("o1", "r1", "solido", json!({ "conteudo": "rosto", "ativo": { "de": tab.tempo(a), "ate": tab.tempo(b - 1) } }));
            let o2 = ocultar("o2", "r2", "solido", json!({ "conteudo": "placa", "ativo": { "de": tab.tempo(110), "ate": null } }));
            let r2 = rastreio("r2", Some("m1"), "fonte", vec![ancora(tab.tempo(110), Some(tab.pts[110]), [0.1, 0.1, 0.2, 0.2])]);
            let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([o1, o2]))], vec![trecho("c1", "m1", &tab, 30, 120, 1.0, json!([]))], json!({ "rastreios": [r, r2] }));
            let plano = compilar(&cap, &apoio::fonte(tab.clone(), (320, 180)), &p, &leitor, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{caso}: {e}"));
            let v = calcular(&p, &feito(&plano, cap.clone()), &leitor).unwrap();
            if caso == "passo constante" {
                // um trecho só (a ausência não o parte), com as faixas na ordem do tempo; slot = quadro − 30
                let achado: Vec<(u64, u64, Vec<(u64, u64, ComoFraco)>)> = v
                    .iter()
                    .filter(|t| t.efeito == "o1")
                    .map(|t| (em_slots(t.de_s, t.ate_s).0, em_slots(t.de_s, t.ate_s).1, t.fracos.iter().map(|f| (em_slots(f.de_s, f.ate_s).0, em_slots(f.de_s, f.ate_s).1, f.como)).collect()))
                    .collect();
                assert_eq!(achado, vec![(10, 70, vec![(30, 35, ComoFraco::Incerto), (40, 50, ComoFraco::Ausencia), (55, 58, ComoFraco::Ausencia)])]);
                // fracos primeiro
                assert_eq!(v[0].efeito, "o1");
            }
            comparar_plano(caso, &p, &plano, &tab, &leitor, &|_| None, 1);
            // invertida: sem região, o quadro sai todo coberto; o trecho é o mesmo, sem a faixa `Ausencia`
            let mut pi = p.clone();
            pi.midias[0].ocultacoes[0].alvo.inverter = true;
            let plano_i = compilar(&cap, &apoio::fonte(tab.clone(), (320, 180)), &pi, &leitor, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{caso}: {e}"));
            let vi = calcular(&pi, &feito(&plano_i, cap.clone()), &leitor).unwrap();
            let dele: Vec<(u64, u64, Vec<ComoFraco>)> =
                vi.iter().filter(|t| t.efeito == "o1").map(|t| (em_slots(t.de_s, t.ate_s).0, em_slots(t.de_s, t.ate_s).1, t.fracos.iter().map(|f| f.como).collect())).collect();
            assert!(!dele.is_empty() && dele.iter().all(|t| !t.2.contains(&ComoFraco::Ausencia)), "{caso}: {dele:?}");
            if caso == "passo constante" {
                assert_eq!(dele, vec![(10, 70, vec![ComoFraco::Incerto])]);
            }
            comparar_plano(&format!("{caso}, invertida"), &pi, &plano_i, &tab, &leitor, &|_| None, 1);
        }
        // a tela recebe o literal "ausencia"
        assert_eq!(serde_json::to_value(ComoFraco::Ausencia).unwrap(), json!("ausencia"));
    }

    /// Taxas de NTSC e PAL, fonte que não começa em zero e câmera lenta: cobertos e fracos (incerto, interpolado,
    /// ausência aceita) contra a verdade por slot do plano, exatos.
    #[test]
    fn taxas_29_97_e_25_contra_a_verdade() {
        use crate::edicao::motor::efeitos::regiao::testes::denso::{compilar, gravado};
        use crate::edicao::rastreio::{ESTADO_AUSENTE, ESTADO_INCERTO, ESTADO_OK, TIPO_INTERPOLADO, TIPO_MEDIDO};
        let ntsc = ((1, 30000), (1001, 30000));
        for (caso, (tb, passo), inicio, fps, v) in [
            ("29,97 → 29,97, início em 2 s", ntsc, 60060, (30000, 1001), 1.0),
            ("25 → 29,97", ((1, 90000), (1, 25)), 0, (30000, 1001), 1.0),
            ("29,97 → 25", ntsc, 0, (25, 1), 1.0),
            ("29,97 → 29,97 a 0,5×", ntsc, 60060, (30000, 1001), 0.5),
        ] {
            let tab = apoio::tabela(tb, apoio::pts_regulares(300, passo, tb, inicio));
            let cap = Arc::new(apoio::cap_falsa());
            let (a, b) = (40usize, 100usize);
            let estado = |j: usize| match j {
                60..=64 => ESTADO_INCERTO,
                70..=79 | 85..=87 => ESTADO_AUSENTE,
                _ => ESTADO_OK,
            };
            let d = ArquivoDenso {
                esquema: 2,
                rastreio: "r1".into(),
                midia_sha256: "0".repeat(64),
                espaco: "original".into(),
                pixel: crate::edicao::modelo::TamanhoPx { w: 320, h: 180 },
                pts: tab.pts[a..b].to_vec(),
                x: vec![0.4; b - a],
                y: vec![0.4; b - a],
                w: vec![0.2; b - a],
                h: vec![0.2; b - a],
                conf: vec![0.9; b - a],
                estado: (a..b).map(estado).collect(),
                tipo: (a..b).map(|j| if (66..=68).contains(&j) { TIPO_INTERPOLADO } else { TIPO_MEDIDO }).collect(),
                corrida: vec![0; b - a],
                corridas: vec![],
            };
            let (sha, leitor) = gravado(&d, tab.tb);
            let aus = |x: usize, y: usize| json!({ "de_s": tab.tempo(x), "ate_s": tab.tempo(y), "motivo": "oculto", "aceita_em": "2026-11-15T10:00:00Z", "por": "Perito" });
            let r = json!({ "id": "r1", "nome": "r1", "cor": "#ff0000", "midia": "m1", "base": "fonte", "tipo": "caixa", "espaco": "original",
                "ancoras": [ancora(tab.tempo(a), Some(tab.pts[a]), [0.4, 0.4, 0.2, 0.2])],
                "denso": { "sha256": sha, "n": b - a, "de_s": tab.tempo(a), "ate_s": tab.tempo(b - 1), "ok": 42, "incertos": 5, "ausentes": 13 },
                "silhuetas": null, "ausencias": [aus(70, 79), aus(85, 87)], "origem": { "tipo": "assistido" }, "confirmacao": null });
            let o1 = ocultar("o1", "r1", "solido", json!({ "conteudo": "rosto", "ativo": { "de": tab.tempo(a), "ate": tab.tempo(b - 1) } }));
            let o2 = ocultar("o2", "r2", "solido", json!({ "conteudo": "placa", "ativo": { "de": tab.tempo(110), "ate": null } }));
            let r2 = rastreio("r2", Some("m1"), "fonte", vec![ancora(tab.tempo(110), Some(tab.pts[110]), [0.1, 0.1, 0.2, 0.2])]);
            let p = projeto(320, 180, fps, vec![midia("m1", "video", json!([o1, o2]))], vec![trecho("c1", "m1", &tab, 30, 120, v, json!([]))], json!({ "rastreios": [r, r2] }));
            let plano = compilar(&cap, &apoio::fonte(tab.clone(), (320, 180)), &p, &leitor, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{caso}: {e}"));
            // passo constante: exato (folga 0)
            comparar_plano(caso, &p, &plano, &tab, &leitor, &|_| None, 0);
            let v = calcular(&p, &feito(&plano, cap), &leitor).unwrap();
            for k in [ComoFraco::Incerto, ComoFraco::Interpolado, ComoFraco::Ausencia] {
                assert!(v.iter().any(|t| t.fracos.iter().any(|f| f.como == k)), "{caso}: sem {k:?}");
            }
        }
    }

    /// O denso do retrato passa pela regra do `arquivo::ler` (SHA-256 do nome, colunas, índices): o malformado é
    /// `rastreio_ausente`, nunca lido calado.
    #[test]
    fn retrato_le_o_denso_pela_regra_do_arquivo() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("dados")).unwrap();
        let leitor = DoRetrato { pasta: d.path().to_path_buf() };
        let denso = |x: Vec<f64>| json!({ "esquema": 2, "rastreio": "r1", "midia_sha256": "0".repeat(64), "espaco": "original", "pixel": { "w": 320, "h": 180 },
            "pts": [0, 512], "x": x, "y": [0.1, 0.1], "w": [0.2, 0.2], "h": [0.2, 0.2], "conf": [0.9, 0.9], "estado": [0, 0], "tipo": [0, 0], "corrida": [0, 0], "corridas": [] });
        let grava = |v: Value| {
            let b = serde_json::to_vec(&v).unwrap();
            let sha = sha256_bytes(&b);
            std::fs::write(d.path().join("dados").join(format!("{sha}.json")), &b).unwrap();
            sha
        };
        let bom = grava(denso(vec![0.1, 0.1]));
        assert_eq!(leitor.denso("r1", &bom).unwrap().pts, [0, 512]);
        // uma coluna mais curta que as outras
        let ruim = grava(denso(vec![0.1]));
        assert!(leitor.denso("r1", &ruim).unwrap_err().to_string().contains(codigo::RASTREIO_AUSENTE));
        // conteúdo que não é o do nome
        std::fs::copy(d.path().join("dados").join(format!("{bom}.json")), d.path().join("dados").join(format!("{}.json", "e".repeat(64)))).unwrap();
        assert!(leitor.denso("r1", &"e".repeat(64)).is_err());
        assert!(leitor.denso("r1", "../x").is_err());
    }

    /// Bancada do diálogo "Conferir ocultações" (fecho da Fase 2, `sicro-bench/edicao-pro/bancada/fase2-conferir`): uma
    /// exportação de verdade (motor, blocos, publicação "a conferir", retrato) de uma fonte que começa em 2 s e grava o
    /// índice do quadro na luma (metade esquerda 20 + 13·⌊N/16⌋, direita 20 + 13·(N mod 16)). Trecho dos quadros
    /// 30..150; `o1` (sólido verde) ativo de 40 a 110, ausência aceita 40..49 (antes da 1ª âncora), âncoras do perito
    /// no 50 e no 90 (interpolado entre elas), segura a última até o 110. Grava em `$SICRO_BANCADA_CONFERIR`:
    /// `exportado.mp4`, `saida.json` (a `SaidaRegistrada`) e `trechos.json` (`trechos` do retrato), e confere no
    /// arquivo: índice de cada slot = 30 + slot; verde exatamente nos slots cobertos, ausente na ausência aceita.
    #[test]
    #[ignore = "bancada: SICRO_BANCADA_CONFERIR=<pasta>"]
    fn bancada_conferir_arquivo_exportado() {
        use crate::edicao::motor::efeitos::realce::testes::fonte_de;
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let destino = PathBuf::from(std::env::var("SICRO_BANCADA_CONFERIR").expect("SICRO_BANCADA_CONFERIR"));
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("f2c-bancada");
        let lum = "if(lt(X\\,W/2)\\,20+13*floor(N/16)\\,20+13*mod(N\\,16))";
        let a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i"]
            .iter()
            .map(|s| s.to_string())
            .chain([format!("color=c=black:s=320x180:r=30:d=6,format=yuv420p,geq=lum='{lum}':cb=128:cr=128")])
            .chain(["-c:v", "libx264", "-qp", "0", "-preset", "ultrafast", "-g", "15", "-bf", "0", "-pix_fmt", "yuv420p", "-video_track_timescale", "15360", "-output_ts_offset", "2", "fonte.mp4"].iter().map(|s| s.to_string()))
            .collect();
        let cmd = crate::edicao::ffmpeg::exec::Comando { nome: "fonte".into(), programa: cap.binario.clone(), args: a, pasta: pasta.clone(), ambiente: vec![], sem_ambiente: vec![], stderr: pasta.join("fonte.log") };
        let (fim, _) = crate::edicao::ffmpeg::exec::rodar_com_saida(&cmd, &std::sync::atomic::AtomicBool::new(false)).unwrap();
        crate::edicao::ffmpeg::exec::exigir_sucesso(&cmd, &fim).unwrap();
        let f = fonte_de(&cap, &pasta.join("fonte.mp4"));
        let tab = &f.quadros;
        assert!((tab.tempo(0) - 2.0).abs() < 1e-6, "a fonte começa em 2 s: {}", tab.tempo(0));
        let cx = [0.55, 0.6, 0.3, 0.3];
        let r = json!({ "id": "r1", "nome": "r1", "cor": "#ff0000", "midia": "m1", "base": "fonte", "tipo": "caixa", "espaco": "original",
            "ancoras": [ancora(tab.tempo(50), Some(tab.pts[50]), cx), ancora(tab.tempo(90), Some(tab.pts[90]), cx)],
            "denso": null, "silhuetas": null,
            "ausencias": [{ "de_s": tab.tempo(40), "ate_s": tab.tempo(49), "motivo": "fora_de_cena", "aceita_em": "2026-11-15T10:00:00Z", "por": "Perito Teste" }],
            "origem": { "tipo": "manual" }, "confirmacao": null });
        let o1 = ocultar("o1", "r1", "solido", json!({ "cor": "#00ff00ff", "conteudo": "placa", "ativo": { "de": tab.tempo(40), "ate": tab.tempo(110) } }));
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([o1]))], vec![trecho("c1", "m1", tab, 30, 150, 1.0, json!([]))], json!({ "rastreios": [r] }));
        let plano = Mundo::novo(&cap).com("m1", &f).compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{e}"));
        let (caso_tmp, ws) = ex::caso();
        let (_, res) = executor::exportar(&cap, &plano, &caso_tmp.path().join("render"), "2026-11-15T12:00:00Z").unwrap_or_else(|e| panic!("exportar ({}): {e}", cap.versao));
        let mut amb = ex::ambiente(cap.clone(), caso_tmp.path(), "t1");
        amb.caso = ws.clone();
        let doc = serde_json::to_value(&p).unwrap();
        let s = fila::publicar(&fila::Publicar { ws: &ws, projeto: &doc, plano: &plano, res: &res, amb: &amb, quando: chrono::Local::now(), tecnico: &Tec { plano: &plano, res: &res, amb: &amb } }).unwrap();
        let v = trechos(&ws, &s.id).unwrap();
        let fracos: Vec<(u64, u64, ComoFraco)> = v.iter().flat_map(|t| t.fracos.iter().map(|f| (em_slots(f.de_s, f.ate_s).0, em_slots(f.de_s, f.ate_s).1, f.como))).collect();
        assert_eq!(fracos, [(10, 20, ComoFraco::Ausencia), (21, 60, ComoFraco::Interpolado)], "{}", cap.versao);
        // o arquivo: índice na luma = 30 + slot; verde nos slots cobertos (fora da ausência), nada na ausência
        let arquivo = paths::resolver_relativo(&ws, &s.arquivo);
        let quadros = medir::quadros(&cap, &arquivo).unwrap();
        assert_eq!(quadros.len() as u64, plano.total_q);
        let luma = |q: &medir::QuadroRgb, x: u32| {
            let mut soma = 0u32;
            for y in 10..30 {
                for dx in 0..20 {
                    soma += q.px[((y * q.w + x + dx) * 3 + 1) as usize] as u32;
                }
            }
            soma as f64 / 400.0
        };
        // RGB de faixa cheia: (Y − 16)·255/219; nível = (Y − 20) / 13
        let nivel = |v: f64| ((v * 219.0 / 255.0 + 16.0 - 20.0) / 13.0).round() as i64;
        for (k, q) in quadros.iter().enumerate() {
            assert_eq!(nivel(luma(q, 60)) * 16 + nivel(luma(q, 240)), 30 + k as i64, "slot {k} ({})", cap.versao);
            let verde = q.px.chunks_exact(3).filter(|p| verde(p)).count() > 200;
            assert_eq!(verde, (20..=80).contains(&k), "slot {k} ({})", cap.versao);
        }
        std::fs::create_dir_all(&destino).unwrap();
        std::fs::copy(&arquivo, destino.join("exportado.mp4")).unwrap();
        std::fs::write(destino.join("saida.json"), serde_json::to_vec_pretty(&s).unwrap()).unwrap();
        std::fs::write(destino.join("trechos.json"), serde_json::to_vec_pretty(&v).unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(&pasta);
        eprintln!("bancada ({}): {} slots, trechos {:?}", cap.versao, quadros.len(), v.iter().map(|t| (t.de_s, t.ate_s)).collect::<Vec<_>>());
    }

    /// O relatório técnico do teste, pelo `tecnico.rs` de verdade.
    struct Tec<'a> {
        plano: &'a Plano,
        res: &'a Resultado,
        amb: &'a Ambiente,
    }

    impl fila::Tecnico for Tec<'_> {
        fn montar(&self, f: &Fecho) -> Result<Value> {
            tecnico::montar(self.plano, self.res, self.amb, f)
        }
        fn gravar(&self, pasta: &Path, titulo: &str, t: &Value) -> Result<String> {
            tecnico::gravar(pasta, titulo, t)
        }
        fn bytes(&self, t: &Value) -> Vec<u8> {
            tecnico::bytes(t)
        }
    }

    fn verde(p: &[u8]) -> bool {
        p[1] > 170 && p[0] < 90 && p[2] < 90
    }

    /// §11 (P2.6): a conferência percorre os intervalos no ARQUIVO exportado. Exportação de verdade (blocos, junção,
    /// conferência), publicação "a conferir" com o retrato e o relatório pendente; os trechos lidos do retrato batem,
    /// slot a slot, com onde o sólido verde aparece no arquivo decodificado. Nos dois binários (`SICRO_FFMPEG`).
    #[test]
    fn intervalos_no_arquivo() {
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let m = crate::edicao::quadros::apoio::medida(&cap, Fonte::Mp4);
        let p = projeto_do_teste(&m.info.quadros, &ancoras_o1(), "#00ff00ff");
        let plano = Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{e}"));
        let (caso_tmp, ws) = ex::caso();
        let trabalho = caso_tmp.path().join("render");
        let (_, res) = executor::exportar(&cap, &plano, &trabalho, "2026-11-15T12:00:00Z").unwrap_or_else(|e| panic!("exportar ({}): {e}", cap.versao));
        let mut amb = ex::ambiente(cap.clone(), caso_tmp.path(), "t1");
        amb.caso = ws.clone();
        let doc = serde_json::to_value(&p).unwrap();
        let s = fila::publicar(&fila::Publicar { ws: &ws, projeto: &doc, plano: &plano, res: &res, amb: &amb, quando: chrono::Local::now(), tecnico: &Tec { plano: &plano, res: &res, amb: &amb } })
            .unwrap_or_else(|e| panic!("publicar ({}): {e}", cap.versao));
        assert_eq!(s.estado, crate::models::EstadoSaida::AConferir);
        let v = trechos(&ws, &s.id).unwrap();
        conferir(&v, &cap.versao);
        // o arquivo exportado: o sólido verde está exatamente nos slots de `o1`
        let quadros = medir::quadros(&cap, &paths::resolver_relativo(&ws, &s.arquivo)).unwrap();
        assert_eq!(quadros.len() as u64, plano.total_q);
        let no_arquivo: Vec<bool> = quadros.iter().map(|q| q.px.chunks_exact(3).filter(|p| verde(p)).count() > 200).collect();
        let nos_trechos: Vec<bool> = (0..quadros.len() as u64).map(|q| v.iter().filter(|t| t.efeito == "o1").any(|t| (em_slots(t.de_s, t.ate_s).0..em_slots(t.de_s, t.ate_s).1).contains(&q))).collect();
        let diferentes: Vec<usize> = (0..quadros.len()).filter(|&q| no_arquivo[q] != nos_trechos[q]).collect();
        assert!(diferentes.is_empty(), "slots em que o arquivo e a conferência discordam ({}): {diferentes:?}", cap.versao);
        assert!(no_arquivo.iter().filter(|x| **x).count() > 100);
        // concluída, a conferência lê o relatório final (o pendente sai do retrato)
        fila::conferir_saida(&ws, &s.id, true, "Perito Teste").unwrap();
        assert_eq!(trechos(&ws, &s.id).unwrap(), v);
        // retrato mexido não é lido
        let pj = fila::pasta_retrato(&ws, &s.projeto_id, &s.id).unwrap().join("projeto.json");
        let mut perm = std::fs::metadata(&pj).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        std::fs::set_permissions(&pj, perm).unwrap();
        std::fs::write(&pj, std::fs::read_to_string(&pj).unwrap().replace("\"Teste\"", "\"Outro\"")).unwrap();
        assert!(trechos(&ws, &s.id).unwrap_err().to_string().contains(codigo::MIDIA_TROCADA));
        assert!(trechos(&ws, "nao-e-uuid").unwrap_err().to_string().contains(codigo::SAIDA_AUSENTE));
        assert!(trechos(&ws, &uuid::Uuid::new_v4().to_string()).unwrap_err().to_string().contains(codigo::SAIDA_AUSENTE));
        eprintln!("conferência no arquivo ({}): {} trechos, {} slots com a ocultação", cap.versao, v.len(), no_arquivo.iter().filter(|x| **x).count());
    }
}
