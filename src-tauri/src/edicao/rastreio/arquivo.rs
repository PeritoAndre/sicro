//! Denso imutável e endereçado pelo conteúdo (§6.1): `rastreios/<rid>/<sha256>.json` na pasta do projeto. Corrida,
//! "Apagar daqui em diante" e "Simplificar" gravam um arquivo NOVO e devolvem o ponteiro; quem troca o ponteiro é o
//! front (`alterar()`). Dono: P2.2.
//!
//! Linhas em ordem de pts, sem pts repetido. Buraco reencontrado dentro de uma corrida = linha ausente (B24).
//! "Simplificar" troca o denso por âncoras (o rastreio fica sem denso).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{laco, AmostraCorrida, ArquivoDenso, ArquivoSilhuetas, Corrida, ResultadoCorrida, Sentido, ESTADO_AUSENTE, ESTADO_INCERTO, ESTADO_OK, TIPO_ESTENDIDO, TIPO_MEDIDO};
use crate::edicao::armazem;
use crate::edicao::executar::{self, sha256_bytes, sha_valido};
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{Caixa, Chave, Denso, Interp, Racional, Rastreio, TamanhoPx};
use crate::error::{Result, SicroError};

/// Esquema do arquivo denso (§6.1). O 1 (fixtures da Fase 0) ainda é lido.
pub const ESQUEMA_DENSO: u32 = 2;
/// Teto do arquivo lido do disco (10 min a 30 qps ≈ 1 MB; 2 h a 60 qps cabe com folga).
pub const MAX_BYTES_DENSO: u64 = 256 << 20;
/// Piso do "Simplificar" (§5.11): 2 px.
pub const PISO_PX: f64 = 2.0;
/// Passo que separa dois trechos do denso no "Simplificar": maior que 1,5 × o passo mediano.
const SALTO_DE_TRECHO: f64 = 1.5;
/// Teto do índice da coluna `corrida` e de `corridas`: `juntar` completa `corridas` até o maior índice citado.
const MAX_CORRIDAS: u32 = 1 << 16;

/// Edição de um denso (`edicao_rastreio_editar`). Faixas por pts do quadro (o denso não guarda a base de tempo);
/// `ate_pts` None = até o fim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum OpRastreio {
    /// "Apagar daqui em diante": tira as amostras (e as âncoras) da faixa, inclusive as pontas.
    ApagarFaixa { de_pts: i64, ate_pts: Option<i64> },
    /// "Simplificar": denso → âncoras (Douglas-Peucker em x, y, w, h; `tolerancia` = fração do lado médio, mínimo 2 px).
    Simplificar { tolerancia: f64 },
}

/// Resultado de uma edição: o front grava `ancoras` quando vem e `denso` SEMPRE (null = rastreio sem denso).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Editado {
    pub ancoras: Option<Vec<Chave<Caixa>>>,
    pub denso: Option<Denso>,
}

fn invalido(detalhe: impl std::fmt::Display) -> SicroError {
    SicroError::Validation(format!("{}: {detalhe}", codigo::ITEM_INVALIDO))
}

fn ausente(rastreio: &str, detalhe: impl std::fmt::Display) -> SicroError {
    SicroError::Validation(format!("{}: {rastreio}: {detalhe}", codigo::RASTREIO_AUSENTE))
}

/// Id e SHA-256 que vêm da tela, conferidos antes de montar qualquer caminho.
fn exigir(rastreio: &str, sha256: Option<&str>) -> Result<()> {
    if !armazem::id_valido(rastreio) {
        return Err(invalido("id do rastreio"));
    }
    if sha256.is_some_and(|s| !sha_valido(s)) {
        return Err(invalido("sha256"));
    }
    Ok(())
}

/// `<pasta do projeto>/rastreios/<rid>/<sha256>.json`. `rastreio` e `sha256` chegam da tela (`edicao_rastreio_ler`) e do
/// documento: `ler`, `gravar` e `editar` recusam com `item_invalido` o id fora de `armazem::id_valido` e o SHA-256
/// que não seja 64 hex minúsculos, antes de montar o caminho.
pub fn caminho(pasta_projeto: &Path, rastreio: &str, sha256: &str) -> PathBuf {
    pasta_projeto.join("rastreios").join(rastreio).join(format!("{sha256}.json"))
}

/// Colunas do mesmo tamanho, pts crescentes sem repetição, números finitos, estado e tipo conhecidos.
/// `corrida` fora de `corridas` passa (é proveniência; arquivos da Fase 0 vêm com `corridas` vazio).
pub(crate) fn conferir(d: &ArquivoDenso) -> Result<()> {
    if d.esquema == 0 || d.esquema > ESQUEMA_DENSO {
        return Err(invalido(format!("esquema {}", d.esquema)));
    }
    let n = d.pts.len();
    let cols = [d.x.len(), d.y.len(), d.w.len(), d.h.len(), d.conf.len(), d.estado.len(), d.tipo.len(), d.corrida.len()];
    if cols.iter().any(|&c| c != n) {
        return Err(invalido("colunas de tamanhos diferentes"));
    }
    if d.pts.windows(2).any(|p| p[1] <= p[0]) {
        return Err(invalido("pts fora de ordem"));
    }
    if [&d.x, &d.y, &d.w, &d.h, &d.conf].iter().any(|c| c.iter().any(|v| !v.is_finite())) {
        return Err(invalido("número não finito"));
    }
    if d.estado.iter().any(|&e| e > ESTADO_AUSENTE) || d.tipo.iter().any(|&t| t > TIPO_ESTENDIDO) {
        return Err(invalido("estado ou tipo"));
    }
    if d.corridas.len() > MAX_CORRIDAS as usize || d.corrida.iter().any(|&c| c >= MAX_CORRIDAS) {
        return Err(invalido("índice de corrida"));
    }
    Ok(())
}

/// Lê e confere o SHA-256 dos bytes; ausente, diferente, grande demais ou fora do formato = `rastreio_ausente`.
pub fn ler(pasta_projeto: &Path, rastreio: &str, sha256: &str) -> Result<ArquivoDenso> {
    exigir(rastreio, Some(sha256))?;
    let arq = caminho(pasta_projeto, rastreio, sha256);
    let f = std::fs::File::open(&arq).map_err(|_| ausente(rastreio, "arquivo"))?;
    if f.metadata().map(|m| m.len()).unwrap_or(0) > MAX_BYTES_DENSO {
        return Err(ausente(rastreio, "grande demais"));
    }
    let mut b = Vec::new();
    f.take(MAX_BYTES_DENSO + 1).read_to_end(&mut b)?;
    if b.len() as u64 > MAX_BYTES_DENSO {
        return Err(ausente(rastreio, "grande demais"));
    }
    if sha256_bytes(&b) != sha256 {
        return Err(ausente(rastreio, "conteúdo diferente do nome"));
    }
    let d: ArquivoDenso = serde_json::from_slice(&b).map_err(|e| ausente(rastreio, e))?;
    conferir(&d).map_err(|e| ausente(rastreio, e))?;
    Ok(d)
}

fn tempo(pts: i64, tb: Racional) -> f64 {
    pts as f64 * tb.num as f64 / tb.den as f64
}

/// Grava o arquivo (nome = SHA-256 do texto; já existente não é regravado) e devolve o ponteiro com as contagens.
/// `tb` = base da tabela do decodificador (para `de_s`/`ate_s`). A pasta é a de `d.rastreio`.
pub fn gravar(pasta_projeto: &Path, d: &ArquivoDenso, tb: Racional) -> Result<Denso> {
    exigir(&d.rastreio, None)?;
    if !sha_valido(&d.midia_sha256) || d.pixel.w == 0 || d.pixel.h == 0 || d.espaco.is_empty() || tb.num <= 0 || tb.den <= 0 {
        return Err(invalido("cabeçalho do denso"));
    }
    conferir(d)?;
    let (Some(&p0), Some(&p1)) = (d.pts.first(), d.pts.last()) else { return Err(invalido("denso vazio")) };
    let bytes = serde_json::to_vec(d)?;
    if bytes.len() as u64 > MAX_BYTES_DENSO {
        return Err(invalido("denso grande demais"));
    }
    let sha256 = sha256_bytes(&bytes);
    let destino = caminho(pasta_projeto, &d.rastreio, &sha256);
    if !destino.is_file() {
        armazem::gravar_atomico(&destino, &bytes)?;
    }
    let conta = |e: u8| d.estado.iter().filter(|&&x| x == e).count() as u32;
    Ok(Denso {
        sha256,
        n: d.pts.len() as u32,
        de_s: tempo(p0, tb),
        ate_s: tempo(p1, tb),
        ok: conta(ESTADO_OK),
        incertos: conta(ESTADO_INCERTO),
        ausentes: conta(ESTADO_AUSENTE),
    })
}

/// Uma linha do denso.
#[derive(Debug, Clone, PartialEq)]
struct Linha {
    pts: i64,
    caixa: [f64; 4],
    conf: f64,
    estado: u8,
    tipo: u8,
    corrida: u32,
}

fn linhas(d: &ArquivoDenso) -> Vec<Linha> {
    (0..d.pts.len())
        .map(|i| Linha { pts: d.pts[i], caixa: [d.x[i], d.y[i], d.w[i], d.h[i]], conf: d.conf[i], estado: d.estado[i], tipo: d.tipo[i], corrida: d.corrida[i] })
        .collect()
}

fn da_amostra(a: &AmostraCorrida, corrida: u32) -> Linha {
    let caixa = a.caixa.map(|c| [c.x, c.y, c.w, c.h]).unwrap_or([0.0; 4]);
    let estado = if a.caixa.is_some() { a.estado } else { ESTADO_AUSENTE };
    Linha { pts: a.pts, caixa, conf: a.conf, estado, tipo: TIPO_MEDIDO, corrida }
}

fn amostra(l: &Linha) -> AmostraCorrida {
    let caixa = (l.estado != ESTADO_AUSENTE).then(|| Caixa { x: l.caixa[0], y: l.caixa[1], w: l.caixa[2], h: l.caixa[3] });
    AmostraCorrida { pts: l.pts, caixa, conf: l.conf, estado: l.estado, silhueta: None }
}

/// Põe as linhas (em ordem de pts) no denso e tira as corridas que nenhuma linha cita (a ordem das outras fica).
fn montar(base: &ArquivoDenso, mut ls: Vec<Linha>, corridas: Vec<serde_json::Value>) -> ArquivoDenso {
    ls.sort_by_key(|l| l.pts);
    let citadas: BTreeSet<u32> = ls.iter().map(|l| l.corrida).collect();
    let mut novo_indice = BTreeMap::new();
    let mut ficam = Vec::new();
    for (i, c) in corridas.into_iter().enumerate() {
        if citadas.contains(&(i as u32)) {
            novo_indice.insert(i as u32, ficam.len() as u32);
            ficam.push(c);
        }
    }
    ArquivoDenso {
        esquema: ESQUEMA_DENSO,
        rastreio: base.rastreio.clone(),
        midia_sha256: base.midia_sha256.clone(),
        espaco: base.espaco.clone(),
        pixel: base.pixel,
        pts: ls.iter().map(|l| l.pts).collect(),
        x: ls.iter().map(|l| l.caixa[0]).collect(),
        y: ls.iter().map(|l| l.caixa[1]).collect(),
        w: ls.iter().map(|l| l.caixa[2]).collect(),
        h: ls.iter().map(|l| l.caixa[3]).collect(),
        conf: ls.iter().map(|l| l.conf).collect(),
        estado: ls.iter().map(|l| l.estado).collect(),
        tipo: ls.iter().map(|l| l.tipo).collect(),
        // corrida citada que não existe em `corridas` (arquivo da Fase 0) fica como estava
        corrida: ls.iter().map(|l| novo_indice.get(&l.corrida).copied().unwrap_or(l.corrida)).collect(),
        corridas: ficam,
    }
}

/// Junta uma corrida a um denso (ou a nenhum): a corrida substitui só a faixa que ela percorreu ("Retomar");
/// o resto fica como estava. Entre duas âncoras com corridas dos dois lados (uma para a frente e outra para trás,
/// uma de frente para a outra), `fundir`, só com as linhas da outra que não podem ter vindo de fusão anterior.
/// A corrida nova vai para o fim de `corridas`; as que nenhuma linha cita saem.
/// Sem base, `rastreio`, `midia_sha256`, `espaco` e `pixel` saem vazios: quem chama preenche (`gravar` confere).
pub fn juntar(base: Option<&ArquivoDenso>, res: &ResultadoCorrida) -> Result<ArquivoDenso> {
    let vazio = ArquivoDenso {
        esquema: ESQUEMA_DENSO,
        rastreio: String::new(),
        midia_sha256: String::new(),
        espaco: String::new(),
        pixel: TamanhoPx { w: 0, h: 0 },
        pts: vec![],
        x: vec![],
        y: vec![],
        w: vec![],
        h: vec![],
        conf: vec![],
        estado: vec![],
        tipo: vec![],
        corrida: vec![],
        corridas: vec![],
    };
    let base = base.unwrap_or(&vazio);
    conferir(base)?;
    let c = &res.corrida;
    if res.amostras.is_empty() || res.amostras.iter().any(|a| a.pts < c.de_pts || a.pts > c.ate_pts) {
        return Err(invalido("corrida fora da própria faixa"));
    }
    let velhas = linhas(base);
    let mut corridas = base.corridas.clone();
    // linha que cita corrida que o arquivo não tem (fixtures da Fase 0): a nova não pode herdar o índice
    let citada = velhas.iter().map(|l| l.corrida as usize + 1).max().unwrap_or(0);
    corridas.resize(corridas.len().max(citada), serde_json::Value::Null);
    let nova = corridas.len() as u32;
    corridas.push(serde_json::to_value(c)?);
    let (lo, hi, x) = (c.de_pts, c.ate_pts, c.ancora_pts);

    let tipadas: Vec<(u32, Corrida)> = base.corridas.iter().enumerate().filter_map(|(i, v)| serde_json::from_value::<Corrida>(v.clone()).ok().map(|o| (i as u32, o))).collect();
    // a corrida do outro sentido cuja âncora está adiante, no fim da faixa percorrida (ou além), e que cobre parte dela
    let oposta = tipadas
        .iter()
        .cloned()
        .filter(|(_, o)| o.sentido != c.sentido)
        .filter(|(_, o)| match c.sentido {
            Sentido::Frente => o.ancora_pts >= hi && o.ancora_pts > x,
            Sentido::Tras => o.ancora_pts <= lo && o.ancora_pts < x,
        })
        .filter(|(i, _)| velhas.iter().any(|l| l.corrida == *i && l.pts >= lo && l.pts <= hi))
        .min_by_key(|(_, o)| (o.ancora_pts - x).abs())
        .map(|(i, o)| (i, o.ancora_pts));

    let novas: Vec<Linha> = res.amostras.iter().map(|a| da_amostra(a, nova)).collect();
    let ls = match oposta {
        None => {
            let mut ls: Vec<Linha> = velhas.into_iter().filter(|l| l.pts < lo || l.pts > hi).collect();
            ls.extend(novas);
            ls
        }
        Some((i, y)) => {
            let (a, b) = (x.min(y), x.max(y));
            // linha da oposta que já pode ter sido fundida com outra corrida do sentido da nova (a mistura não se desfaz):
            // não entra na fusão; dentro da faixa percorrida sai, fora dela fica como estava
            let fundida = |p: i64| {
                tipadas.iter().any(|(k, o)| {
                    *k != i
                        && o.sentido == c.sentido
                        && o.de_pts <= p
                        && p <= o.ate_pts
                        && match c.sentido {
                            Sentido::Frente => o.ancora_pts <= p,
                            Sentido::Tras => o.ancora_pts >= p,
                        }
                })
            };
            let pura = |l: &Linha| l.corrida == i && l.pts >= a && l.pts <= b && !fundida(l.pts);
            let delas: Vec<AmostraCorrida> = velhas.iter().filter(|l| pura(l)).map(amostra).collect();
            let (frente, tras, i_frente, i_tras) = match c.sentido {
                Sentido::Frente => (res.amostras.as_slice(), delas.as_slice(), nova, i),
                Sentido::Tras => (delas.as_slice(), res.amostras.as_slice(), i, nova),
            };
            let fundidas = super::fundir::fundir(frente, tras, a, b);
            let com_caixa = |v: &[AmostraCorrida]| -> BTreeSet<i64> { v.iter().filter(|x| x.caixa.is_some()).map(|x| x.pts).collect() };
            let (cf, ct) = (com_caixa(frente), com_caixa(tras));
            let mut ls: Vec<Linha> = velhas.iter().filter(|l| (l.pts < lo || l.pts > hi) && !pura(l)).cloned().collect();
            // a linha cita a corrida de onde veio a caixa; das duas, a da âncora mais perto (a que deu a silhueta)
            ls.extend(fundidas.iter().map(|f| {
                let perto = if f.pts - a <= b - f.pts { i_frente } else { i_tras };
                let de = match (cf.contains(&f.pts), ct.contains(&f.pts)) {
                    (true, false) => i_frente,
                    (false, true) => i_tras,
                    _ => perto,
                };
                da_amostra(f, de)
            }));
            ls
        }
    };
    Ok(montar(base, ls, corridas))
}

/// Silhuetas do denso juntado: as da corrida nova (a última de `d.corridas`) e, nos outros quadros, as da base.
/// None = nenhum quadro com silhueta.
pub(crate) fn silhuetas_juntadas(d: &ArquivoDenso, res: &ResultadoCorrida, base: Option<&ArquivoSilhuetas>, modelo: &str) -> Option<ArquivoSilhuetas> {
    let nova = d.corridas.len().checked_sub(1)? as u32;
    let da_corrida: BTreeMap<i64, &Vec<u8>> = res.amostras.iter().filter_map(|a| a.silhueta.as_ref().map(|s| (a.pts, s))).collect();
    let da_base: BTreeMap<i64, &Vec<u8>> = base.map(|b| b.pts.iter().copied().zip(b.mascaras.iter()).collect()).unwrap_or_default();
    let (mut pts, mut mascaras) = (Vec::new(), Vec::new());
    for i in 0..d.pts.len() {
        if d.estado[i] == ESTADO_AUSENTE {
            continue;
        }
        let m = if d.corrida[i] == nova { da_corrida.get(&d.pts[i]) } else { da_base.get(&d.pts[i]) };
        if let Some(m) = m {
            pts.push(d.pts[i]);
            mascaras.push((*m).clone());
        }
    }
    if pts.is_empty() {
        return None;
    }
    let modelo = base.filter(|b| !da_base.is_empty() && b.modelo != modelo).map(|b| format!("{}; {modelo}", b.modelo)).unwrap_or_else(|| modelo.to_string());
    Some(ArquivoSilhuetas { esquema: super::silhueta::ESQUEMA_SILHUETAS, pixel: d.pixel, n: pts.len() as u32, pts, modelo, mascaras })
}

/// As corridas tipadas do arquivo.
pub fn corridas(d: &ArquivoDenso) -> Result<Vec<Corrida>> {
    d.corridas.iter().map(|v| serde_json::from_value(v.clone()).map_err(|e| invalido(format!("corrida: {e}")))).collect()
}

/// Aplica `op` ao rastreio `rastreio_id` do documento da tela (D2): denso (lido de `denso.sha256`) e âncoras;
/// grava o arquivo novo quando há. A base de tempo vem da tabela do decodificador da mídia (`cache` do caso).
pub fn editar(ws: &Path, cache: &Path, projeto: &serde_json::Value, rastreio_id: &str, op: &OpRastreio) -> Result<Editado> {
    exigir(rastreio_id, None)?;
    let p = armazem::tipado(projeto)?;
    if !armazem::id_valido(&p.id) {
        return Err(invalido("id do projeto"));
    }
    let r = p.rastreios.iter().find(|r| r.id == rastreio_id).ok_or_else(|| SicroError::Validation(format!("{}: {rastreio_id}", codigo::RASTREIO_AUSENTE)))?;
    let midia = r.midia.as_deref().ok_or_else(|| invalido("rastreio sem mídia"))?;
    let m = p.midias.iter().find(|m| m.id == midia).ok_or_else(|| SicroError::Validation(format!("{}: {midia}", codigo::MIDIA_AUSENTE)))?;
    let pasta = armazem::pasta_projeto(ws, &p.id);
    if let Some(d) = &r.denso {
        exigir(rastreio_id, Some(&d.sha256))?;
    }
    let denso = r.denso.as_ref().map(|d| ler(&pasta, &r.id, &d.sha256)).transpose()?;
    let prep = executar::preparar(ws, &p, cache, false)?;
    let tb = prep.fontes.get(midia).map(|f| f.quadros.tb).ok_or_else(|| SicroError::Validation(format!("{}: {midia}", codigo::MIDIA_AUSENTE)))?;
    let (ancoras, novo) = aplicar(r, denso.as_ref(), op, tb)?;
    let denso = match novo {
        Some(mut d) => {
            d.rastreio = r.id.clone();
            d.midia_sha256 = m.sha256.clone();
            Some(gravar(&pasta, &d, tb)?)
        }
        None => None,
    };
    Ok(Editado { ancoras, denso })
}

/// Trechos contínuos das linhas: passo maior que `SALTO_DE_TRECHO` × o passo mediano separa.
fn trechos(d: &ArquivoDenso) -> Vec<std::ops::Range<usize>> {
    let n = d.pts.len();
    if n == 0 {
        return vec![];
    }
    let mut passos: Vec<i64> = d.pts.windows(2).map(|p| p[1] - p[0]).collect();
    passos.sort_unstable();
    let mediano = passos.get(passos.len() / 2).copied().unwrap_or(1) as f64;
    let mut v = Vec::new();
    let mut ini = 0;
    for i in 1..=n {
        if i == n || (d.pts[i] - d.pts[i - 1]) as f64 > SALTO_DE_TRECHO * mediano {
            v.push(ini..i);
            ini = i;
        }
    }
    v
}

// CONTRATO: `aplicar` ganha `tb` (contratos-fase2 (a) dizia `aplicar(r, denso, op)`): as âncoras do "Simplificar"
// guardam `t` (s) ao lado do pts, e só a base de tempo da tabela dá esse `t`. Continua pura (`editar` passa a tb).
/// O núcleo de `editar`, sem disco: âncoras e denso novos de `r` (None = sem mudança nas âncoras; denso None =
/// rastreio sem denso). `tb` = base da tabela do decodificador da mídia.
pub fn aplicar(r: &Rastreio, denso: Option<&ArquivoDenso>, op: &OpRastreio, tb: Racional) -> Result<(Option<Vec<Chave<Caixa>>>, Option<ArquivoDenso>)> {
    if tb.num <= 0 || tb.den <= 0 {
        return Err(invalido("base de tempo"));
    }
    if let Some(d) = denso {
        conferir(d)?;
    }
    match *op {
        OpRastreio::ApagarFaixa { de_pts, ate_pts } => {
            if ate_pts.is_some_and(|a| a < de_pts) {
                return Err(invalido("faixa invertida"));
            }
            let dentro = |pts: i64| pts >= de_pts && ate_pts.is_none_or(|a| pts <= a);
            // âncora sem pts: pelo tempo, com meio tique de folga
            let meio = tempo(1, tb) / 2.0;
            let dentro_t = |t: f64| t >= tempo(de_pts, tb) - meio && ate_pts.is_none_or(|a| t <= tempo(a, tb) + meio);
            let ficam: Vec<Chave<Caixa>> = r.ancoras.iter().filter(|k| !k.pts.map_or(dentro_t(k.t), dentro)).cloned().collect();
            let ancoras = (ficam.len() != r.ancoras.len()).then_some(ficam);
            let denso = denso.and_then(|d| {
                let ls: Vec<Linha> = linhas(d).into_iter().filter(|l| !dentro(l.pts)).collect();
                (!ls.is_empty()).then(|| montar(d, ls, d.corridas.clone()))
            });
            Ok((ancoras, denso))
        }
        OpRastreio::Simplificar { tolerancia } => {
            if !(tolerancia.is_finite() && tolerancia > 0.0 && tolerancia <= 1.0) {
                return Err(invalido("tolerância"));
            }
            let Some(d) = denso else { return Ok((None, None)) };
            let ls = linhas(d);
            let mut novas: Vec<Chave<Caixa>> = Vec::new();
            for tr in trechos(d) {
                let am: Vec<AmostraCorrida> = ls[tr.clone()].iter().map(amostra).collect();
                for k in laco::quadros_chave(&am, d.pixel, tolerancia, PISO_PX) {
                    let a = &am[k];
                    novas.push(Chave { t: tempo(a.pts, tb), pts: Some(a.pts), v: a.caixa.expect("chave com caixa"), interp: Interp::Linear });
                }
            }
            // a âncora do perito vence no mesmo quadro
            let tem = |k: &Chave<Caixa>| r.ancoras.iter().any(|a| a.pts == k.pts || (a.pts.is_none() && a.t == k.t));
            let mut ancoras: Vec<Chave<Caixa>> = r.ancoras.clone();
            ancoras.extend(novas.into_iter().filter(|k| !tem(k)));
            ancoras.sort_by(|a, b| a.t.total_cmp(&b.t).then(a.pts.cmp(&b.pts)));
            Ok((Some(ancoras), None))
        }
    }
}


#[cfg(test)]
pub(crate) mod testes {
    use super::*;
    use crate::edicao::modelo::OrigemRastreio;
    use crate::edicao::rastreio::avaliar::testes::{ch, denso_teste, em, rastreio};
    use crate::edicao::rastreio::avaliar::{avaliar, Como, Instante};
    use crate::edicao::rastreio::{Limiares, ModeloCorrida, Motivo, MotorCorrida, MotorRastreio, Parou};
    use serde_json::json;

    const TB: Racional = Racional { num: 1, den: 15360 };

    pub(crate) fn corrida(sentido: Sentido, ancora: i64, de: i64, ate: i64) -> Corrida {
        Corrida {
            motor: MotorCorrida { motor: MotorRastreio::Mancha, modelo: ModeloCorrida { nome: "mancha".into(), origem: String::new(), commit: String::new(), arquivos: vec![], desvio: String::new() }, runtime: String::new(), provedor: "cpu".into() },
            sentido,
            ancora_pts: ancora,
            de_pts: de,
            ate_pts: ate,
            limiares: Limiares { ok: 0.7, perdido: 0.55, paciencia: 1 },
            parou: Motivo::Fim,
            feito_em: format!("2026-11-15T00:00:{:02}Z", (ancora / 512) % 60),
        }
    }

    /// Corrida de quadros k (pts 512·k): `de..=ate` lidos; amostras `am` (k → x); a âncora é `ancora`.
    pub(crate) fn res(sentido: Sentido, ancora: i64, de: i64, ate: i64, am: &[(i64, Option<f64>)]) -> ResultadoCorrida {
        let mut amostras: Vec<AmostraCorrida> = am
            .iter()
            .map(|&(k, x)| AmostraCorrida { pts: 512 * k, caixa: x.map(|x| Caixa { x, y: 0.25, w: 0.1, h: 0.2 }), conf: 0.9, estado: if x.is_some() { ESTADO_OK } else { ESTADO_AUSENTE }, silhueta: None })
            .collect();
        if sentido == Sentido::Tras {
            amostras.reverse();
        }
        ResultadoCorrida { amostras, parou: Parou { motivo: Motivo::Fim, pts: None }, corrida: corrida(sentido, 512 * ancora, 512 * de, 512 * ate) }
    }

    fn com_cabecalho(mut d: ArquivoDenso) -> ArquivoDenso {
        d.rastreio = "r1".into();
        d.midia_sha256 = "a".repeat(64);
        d.espaco = "original".into();
        d.pixel = TamanhoPx { w: 640, h: 360 };
        d
    }

    fn x_em(d: &ArquivoDenso, k: i64) -> Option<f64> {
        let j = d.pts.binary_search(&(512 * k)).ok()?;
        (d.estado[j] != ESTADO_AUSENTE).then_some(d.x[j])
    }

    fn corrida_em(d: &ArquivoDenso, k: i64) -> u32 {
        d.corrida[d.pts.binary_search(&(512 * k)).unwrap()]
    }

    #[test]
    fn imutavel_e_enderecado() {
        let pasta = tempfile::tempdir().unwrap();
        let mut d = denso_teste(10, 40, |k| if k == 20 { ESTADO_INCERTO } else { ESTADO_OK }, |_| TIPO_MEDIDO);
        // números que só voltam iguais com `float_roundtrip`
        d.x[3] = 0.1 + 0.2;
        d.y[4] = 1.0 / 3.0;
        d.conf[5] = 0.123456789012345678;
        d.corridas = vec![serde_json::to_value(corrida(Sentido::Frente, 5120, 5120, 20480)).unwrap()];
        let p = gravar(pasta.path(), &d, TB).unwrap();
        let arq = caminho(pasta.path(), "r1", &p.sha256);
        let bytes = std::fs::read(&arq).unwrap();
        assert_eq!(sha256_bytes(&bytes), p.sha256, "nome = SHA-256 do texto");
        assert_eq!(p, Denso { sha256: p.sha256.clone(), n: 31, de_s: 5120.0 / 15360.0, ate_s: 20480.0 / 15360.0, ok: 30, incertos: 1, ausentes: 0 });
        let lido = ler(pasta.path(), "r1", &p.sha256).unwrap();
        assert_eq!(lido, d);
        assert_eq!(corridas(&lido).unwrap()[0].ancora_pts, 5120);
        // de novo: mesmo nome, nada regravado
        let antes = std::fs::metadata(&arq).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(gravar(pasta.path(), &d, TB).unwrap(), p);
        assert_eq!(std::fs::metadata(&arq).unwrap().modified().unwrap(), antes);
        // um número diferente = outro arquivo; o primeiro continua lá
        let mut d2 = d.clone();
        d2.x[0] += 1e-12;
        let p2 = gravar(pasta.path(), &d2, TB).unwrap();
        assert_ne!(p2.sha256, p.sha256);
        assert_eq!(ler(pasta.path(), "r1", &p.sha256).unwrap(), d);

        // conteúdo que não bate com o nome; caminho para fora; SHA fora da regra
        std::fs::write(caminho(pasta.path(), "r1", &"b".repeat(64)), &bytes).unwrap();
        let e = ler(pasta.path(), "r1", &"b".repeat(64)).unwrap_err().to_string();
        assert!(e.contains(codigo::RASTREIO_AUSENTE), "{e}");
        for (rid, sha) in [("../r1", p.sha256.clone()), ("r1/..", p.sha256.clone()), ("r1", format!("../{}", &p.sha256[3..])), ("r1", p.sha256.to_uppercase()), ("", p.sha256.clone())] {
            let e = ler(pasta.path(), rid, &sha).unwrap_err().to_string();
            assert!(e.contains(codigo::ITEM_INVALIDO), "{rid} {sha}: {e}");
        }
        let mut fora = d.clone();
        fora.rastreio = "../fora".into();
        assert!(gravar(pasta.path(), &fora, TB).unwrap_err().to_string().contains(codigo::ITEM_INVALIDO));
        assert!(!pasta.path().join("fora").exists());
        // teto: o tamanho é lido antes (arquivo esparso, nada é lido para a memória)
        let grande = caminho(pasta.path(), "r1", &"c".repeat(64));
        std::fs::File::create(&grande).unwrap().set_len(MAX_BYTES_DENSO + 1).unwrap();
        let t = std::time::Instant::now();
        assert!(ler(pasta.path(), "r1", &"c".repeat(64)).unwrap_err().to_string().contains("grande demais"));
        assert!(t.elapsed().as_secs_f64() < 1.0);
        // esquema 1 (fixtures da Fase 0, leitor do executor) lê; esquema futuro e colunas tortas não
        for (nome, mudar, lê) in [
            ("esquema 1", json!({ "esquema": 1 }), true),
            ("esquema 3", json!({ "esquema": 3 }), false),
            ("coluna curta", json!({ "x": [0.1] }), false),
            ("pts fora de ordem", json!({ "pts": (0..31).rev().collect::<Vec<i64>>() }), false),
            ("estado 3", json!({ "estado": vec![3; 31] }), false),
        ] {
            let mut v = serde_json::to_value(&d).unwrap();
            for (k, x) in mudar.as_object().unwrap() {
                v[k] = x.clone();
            }
            let b = serde_json::to_vec(&v).unwrap();
            let sha = sha256_bytes(&b);
            std::fs::write(caminho(pasta.path(), "r1", &sha), &b).unwrap();
            assert_eq!(ler(pasta.path(), "r1", &sha).is_ok(), lê, "{nome}");
            let dv: ArquivoDenso = serde_json::from_value(v).unwrap();
            if !lê {
                assert!(gravar(pasta.path(), &dv, TB).unwrap_err().to_string().contains(codigo::ITEM_INVALIDO), "{nome}");
            }
        }
        // cabeçalho que `juntar(None, …)` deixa vazio não é gravado
        let r1 = res(Sentido::Frente, 0, 0, 3, &[(0, Some(0.1)), (1, Some(0.1))]);
        assert!(gravar(pasta.path(), &juntar(None, &r1).unwrap(), TB).unwrap_err().to_string().contains(codigo::ITEM_INVALIDO));
    }

    #[test]
    fn juntar_substitui_so_a_faixa() {
        // corrida 1: para a frente de 0 até 150
        let c1: Vec<_> = (0..=150).map(|k| (k, Some(0.1 + k as f64 / 1000.0))).collect();
        let d1 = com_cabecalho(juntar(None, &res(Sentido::Frente, 0, 0, 150, &c1)).unwrap());
        assert_eq!((d1.pts.len(), d1.corridas.len()), (151, 1));
        // "Retomar" em 60: leu até 140, perdeu em 121 (cauda cortada)
        let c2: Vec<_> = (60..=120).map(|k| (k, Some(0.5 + k as f64 / 1000.0))).collect();
        let d2 = juntar(Some(&d1), &res(Sentido::Frente, 60, 60, 140, &c2)).unwrap();
        assert_eq!((d2.rastreio.as_str(), d2.pixel), ("r1", d1.pixel));
        for k in 0..=150 {
            let esperado = match k {
                0..=59 | 141..=150 => d1.x[k as usize],
                60..=120 => 0.5 + k as f64 / 1000.0,
                _ => f64::NAN,
            };
            match x_em(&d2, k) {
                Some(x) => assert_eq!(x, esperado, "quadro {k}"),
                None => assert!((121..=140).contains(&k), "quadro {k}"),
            }
        }
        // linhas de fora da faixa: as mesmas, coluna a coluna; a corrida citada, a mesma
        let antes: Vec<usize> = (0..60).chain(141..151).collect();
        for &k in &antes {
            let j = d2.pts.binary_search(&(512 * k as i64)).unwrap();
            assert_eq!((d2.y[j], d2.w[j], d2.h[j], d2.conf[j], d2.estado[j], d2.tipo[j]), (d1.y[k], d1.w[k], d1.h[k], d1.conf[k], d1.estado[k], d1.tipo[k]));
            assert_eq!(corridas(&d2).unwrap()[d2.corrida[j] as usize], corridas(&d1).unwrap()[0]);
        }
        assert_eq!((d2.corridas.len(), corrida_em(&d2, 70)), (2, 1));
        // para trás de 100 até 30: substitui 30..=100; a corrida 1 (para a frente desde 0) está de frente para ela e
        // cobre 30..=59: ali as duas se fundem, com o peso pela distância às âncoras 0 e 100
        let c3: Vec<_> = (30..=100).map(|k| (k, Some(0.9))).collect();
        let d3 = juntar(Some(&d2), &res(Sentido::Tras, 100, 30, 100, &c3)).unwrap();
        let mistura = |k: usize| (1.0 - k as f64 / 100.0) * d1.x[k] + k as f64 / 100.0 * 0.9;
        assert_eq!((x_em(&d3, 29), x_em(&d3, 30), x_em(&d3, 59)), (Some(d1.x[29]), Some(mistura(30)), Some(mistura(59))));
        assert_eq!((x_em(&d3, 60), x_em(&d3, 100), x_em(&d3, 101)), (Some(0.9), Some(0.9), Some(0.5 + 0.101)));
        // uma corrida que cobre tudo: as outras saem de `corridas`
        let c4: Vec<_> = (0..=200).map(|k| (k, Some(0.3))).collect();
        let d4 = juntar(Some(&d3), &res(Sentido::Frente, 0, 0, 200, &c4)).unwrap();
        assert_eq!((d4.corridas.len(), d4.pts.len()), (1, 201));
        assert!(d4.corrida.iter().all(|&c| c == 0));
        // buraco reencontrado dentro da corrida: linha ausente
        let c5: Vec<_> = (0..=20).map(|k| (k, (!(8..=11).contains(&k)).then_some(0.2))).collect();
        let d5 = juntar(None, &res(Sentido::Frente, 0, 0, 20, &c5)).unwrap();
        assert_eq!(d5.estado.iter().filter(|&&e| e == ESTADO_AUSENTE).count(), 4);
        assert_eq!(d5.x[9], 0.0);
        // amostra fora da faixa declarada
        assert!(juntar(None, &res(Sentido::Frente, 0, 0, 5, &[(0, Some(0.1)), (9, Some(0.1))])).is_err());
        // denso da Fase 0 (`corridas` vazio, linhas citando a 0): a corrida nova não herda o índice das velhas
        let mut velho = com_cabecalho(denso_teste(0, 9, |_| ESTADO_OK, |_| TIPO_MEDIDO));
        velho.esquema = 1;
        let d6 = juntar(Some(&velho), &res(Sentido::Frente, 5, 5, 9, &[(5, Some(0.7)), (6, Some(0.7))])).unwrap();
        assert_eq!((corrida_em(&d6, 2), corrida_em(&d6, 5), d6.corridas.len()), (0, 1, 2));
        assert!(d6.corridas[0].is_null() && corridas(&d6).is_err());
    }

    #[test]
    fn juntar_funde_entre_duas_ancoras() {
        // base: para trás da âncora 100, até 30
        let t: Vec<_> = (30..=100).map(|k| (k, Some(0.8))).collect();
        let base = com_cabecalho(juntar(None, &res(Sentido::Tras, 100, 30, 100, &t)).unwrap());
        // nova: para a frente da âncora 0, parou na âncora 100; buraco em 40..=42
        let f: Vec<_> = (0..=100).map(|k| (k, (!(40..=42).contains(&k)).then_some(0.2))).collect();
        let d = juntar(Some(&base), &res(Sentido::Frente, 0, 0, 100, &f)).unwrap();
        assert_eq!((d.pts.len(), d.corridas.len()), (101, 2));
        for k in 0..=100i64 {
            let x = x_em(&d, k).unwrap();
            if k < 30 {
                assert_eq!(x, 0.2, "{k}");
            } else if (40..=42).contains(&k) {
                // só a de trás tem caixa: fica com ela, e a linha cita a de trás
                assert_eq!((x, corrida_em(&d, k)), (0.8, 0), "{k}");
                continue;
            } else {
                let wt = k as f64 / 100.0;
                assert_eq!(x, (1.0 - wt) * 0.2 + wt * 0.8, "{k}");
            }
            // a corrida citada é a da âncora mais perto
            assert_eq!(corrida_em(&d, k), if k <= 50 { 1 } else { 0 }, "{k}");
        }
        // ao contrário: a base é para a frente (perdeu em 61), a nova para trás desde 100 até a âncora 0
        let f: Vec<_> = (0..=60).map(|k| (k, Some(0.2))).collect();
        let base = com_cabecalho(juntar(None, &res(Sentido::Frente, 0, 0, 70, &f)).unwrap());
        let t: Vec<_> = (0..=100).map(|k| (k, Some(0.8))).collect();
        let d = juntar(Some(&base), &res(Sentido::Tras, 100, 0, 100, &t)).unwrap();
        assert_eq!(x_em(&d, 61), Some(0.8));
        assert_eq!(x_em(&d, 40), Some(0.6 * 0.2 + 0.4 * 0.8));
        // corrida do outro sentido com a âncora para trás da nova (não estão uma de frente para a outra): substitui
        let t: Vec<_> = (10..=50).map(|k| (k, Some(0.8))).collect();
        let base = com_cabecalho(juntar(None, &res(Sentido::Tras, 50, 10, 50, &t)).unwrap());
        let f: Vec<_> = (40..=90).map(|k| (k, Some(0.2))).collect();
        let d = juntar(Some(&base), &res(Sentido::Frente, 40, 40, 90, &f)).unwrap();
        assert_eq!((x_em(&d, 39), x_em(&d, 45), x_em(&d, 50)), (Some(0.8), Some(0.2), Some(0.2)));
    }

    /// "Retomar" depois de uma fusão: a corrida substituída não sobra misturada no trecho novo; a seguinte do outro
    /// lado volta a fundir com a nova, sem a velha.
    #[test]
    fn retomar_depois_da_fusao_nao_guarda_a_velha() {
        // âncoras em 0 e 100: para a frente de 0 (x = 0,1) e para trás de 100 (x = 0,9), fundidas
        let f0: Vec<_> = (0..=100).map(|k| (k, Some(0.1))).collect();
        let d0 = com_cabecalho(juntar(None, &res(Sentido::Frente, 0, 0, 100, &f0)).unwrap());
        let t1: Vec<_> = (0..=100).map(|k| (k, Some(0.9))).collect();
        let d1 = juntar(Some(&d0), &res(Sentido::Tras, 100, 0, 100, &t1)).unwrap();
        assert_eq!(x_em(&d1, 60), Some(0.4 * 0.1 + 0.6 * 0.9));
        // âncora nova em 30 e "Retomar" para a frente até 100 (x = 0,5): de 30 a 100 só a nova
        let f2: Vec<_> = (30..=100).map(|k| (k, Some(0.5))).collect();
        let d2 = juntar(Some(&d1), &res(Sentido::Frente, 30, 30, 100, &f2)).unwrap();
        for k in 0..=100 {
            let esperado = if k < 30 { x_em(&d1, k) } else { Some(0.5) };
            assert_eq!(x_em(&d2, k), esperado, "quadro {k}");
        }
        // para trás de 100 de novo (x = 0,7) até a âncora 30: funde com a de 30, sem a de 0
        let t3: Vec<_> = (30..=100).map(|k| (k, Some(0.7))).collect();
        let d3 = juntar(Some(&d2), &res(Sentido::Tras, 100, 30, 100, &t3)).unwrap();
        for k in 30..=100i64 {
            let wt = (k - 30) as f64 / 70.0;
            assert_eq!(x_em(&d3, k), Some((1.0 - wt) * 0.5 + wt * 0.7), "quadro {k}");
        }
        assert_eq!((0..30).map(|k| x_em(&d3, k)).collect::<Vec<_>>(), (0..30).map(|k| x_em(&d1, k)).collect::<Vec<_>>());
        // perdida antes da âncora de trás: a mistura velha depois da faixa percorrida fica como estava
        let f4: Vec<_> = (30..=60).map(|k| (k, Some(0.3))).collect();
        let d4 = juntar(Some(&d1), &res(Sentido::Frente, 30, 30, 70, &f4)).unwrap();
        assert_eq!((x_em(&d4, 60), x_em(&d4, 65), x_em(&d4, 80)), (Some(0.3), None, x_em(&d1, 80)));
    }

    /// Índice de corrida absurdo (arquivo montado à mão): recusado antes de `juntar` completar `corridas` até ele.
    #[test]
    fn indice_de_corrida_com_teto() {
        let pasta = tempfile::tempdir().unwrap();
        let mut d = com_cabecalho(denso_teste(0, 3, |_| ESTADO_OK, |_| TIPO_MEDIDO));
        d.corrida[2] = u32::MAX;
        let b = serde_json::to_vec(&d).unwrap();
        let sha = sha256_bytes(&b);
        std::fs::create_dir_all(caminho(pasta.path(), "r1", &sha).parent().unwrap()).unwrap();
        std::fs::write(caminho(pasta.path(), "r1", &sha), &b).unwrap();
        assert!(ler(pasta.path(), "r1", &sha).unwrap_err().to_string().contains(codigo::RASTREIO_AUSENTE));
        let r = res(Sentido::Frente, 1, 1, 2, &[(1, Some(0.1)), (2, Some(0.1))]);
        assert!(juntar(Some(&d), &r).unwrap_err().to_string().contains(codigo::ITEM_INVALIDO));
        // no teto ainda passa
        d.corrida[2] = MAX_CORRIDAS - 1;
        assert!(juntar(Some(&d), &r).is_ok());
    }

    #[test]
    fn apagar_daqui_em_diante() {
        let mut r = rastreio(vec![ch(0, Caixa { x: 0.1, y: 0.1, w: 0.1, h: 0.1 }, Interp::Linear), ch(50, Caixa { x: 0.2, y: 0.1, w: 0.1, h: 0.1 }, Interp::Linear), ch(120, Caixa { x: 0.3, y: 0.1, w: 0.1, h: 0.1 }, Interp::Linear)], OrigemRastreio::Assistido);
        // âncora sem pts (gravada pelo tempo) também conta
        r.ancoras.push(Chave { t: em(130).t, pts: None, v: Caixa { x: 0.4, y: 0.1, w: 0.1, h: 0.1 }, interp: Interp::Linear });
        let a: Vec<_> = (0..=80).map(|k| (k, Some(0.1))).collect();
        let b: Vec<_> = (81..=150).map(|k| (k, Some(0.2))).collect();
        let d = com_cabecalho(juntar(Some(&com_cabecalho(juntar(None, &res(Sentido::Frente, 0, 0, 80, &a)).unwrap())), &res(Sentido::Frente, 81, 81, 150, &b)).unwrap());
        assert_eq!(d.corridas.len(), 2);
        let (anc, novo) = aplicar(&r, Some(&d), &OpRastreio::ApagarFaixa { de_pts: 512 * 80, ate_pts: None }, TB).unwrap();
        assert_eq!(anc.unwrap().iter().map(|k| k.pts).collect::<Vec<_>>(), vec![Some(0), Some(512 * 50)]);
        let novo = novo.unwrap();
        assert_eq!((novo.pts.first(), novo.pts.last(), novo.corridas.len()), (Some(&0), Some(&(512 * 79)), 1));
        assert_eq!(novo.corridas[0], d.corridas[0]);
        // faixa no meio: âncoras não mudam (None), o resto fica
        let (anc, novo) = aplicar(&r, Some(&d), &OpRastreio::ApagarFaixa { de_pts: 512 * 10, ate_pts: Some(512 * 20) }, TB).unwrap();
        assert!(anc.is_none());
        assert_eq!(novo.unwrap().pts.len(), 151 - 11);
        // tudo: sem denso
        let (anc, novo) = aplicar(&r, Some(&d), &OpRastreio::ApagarFaixa { de_pts: 0, ate_pts: None }, TB).unwrap();
        assert_eq!((anc.map(|a| a.len()), novo), (Some(0), None));
        assert!(aplicar(&r, Some(&d), &OpRastreio::ApagarFaixa { de_pts: 10, ate_pts: Some(5) }, TB).unwrap_err().to_string().contains(codigo::ITEM_INVALIDO));
    }

    #[test]
    fn simplificar_vira_ancoras() {
        // caixa em px de 640×360: reta até 60, curva depois; buraco em 70..=74
        let px = |k: i64| -> Option<[f64; 4]> {
            if (70..=74).contains(&k) {
                return None;
            }
            let t = k as f64;
            let x = if k <= 60 { 100.0 + 2.0 * t } else { 220.0 + 0.05 * (t - 60.0).powi(2) };
            Some([x, 80.0 + 0.5 * t, 40.0 + 0.1 * t, 90.0])
        };
        let mut d = com_cabecalho(denso_teste(0, 140, |k| if (70..=74).contains(&k) { ESTADO_AUSENTE } else { ESTADO_OK }, |_| TIPO_MEDIDO));
        for (j, k) in (0..=140).enumerate() {
            let c = px(k).unwrap_or([0.0; 4]);
            (d.x[j], d.y[j], d.w[j], d.h[j]) = (c[0] / 640.0, c[1] / 360.0, c[2] / 640.0, c[3] / 360.0);
        }
        let perito = Caixa { x: 0.5, y: 0.5, w: 0.1, h: 0.1 };
        let r = rastreio(vec![ch(30, perito, Interp::Linear)], OrigemRastreio::Assistido);
        let (anc, novo) = aplicar(&r, Some(&d), &OpRastreio::Simplificar { tolerancia: 0.04 }, TB).unwrap();
        assert!(novo.is_none(), "o denso vira âncoras");
        let anc = anc.unwrap();
        let pts: Vec<i64> = anc.iter().map(|k| k.pts.unwrap() / 512).collect();
        // a reta vira as duas pontas; o buraco separa (69 e 75 são chaves); a âncora do perito vence em 30
        assert!(pts.contains(&0) && pts.contains(&69) && pts.contains(&75) && pts.contains(&140), "{pts:?}");
        assert!(anc.len() < 20, "{}", anc.len());
        assert_eq!(anc.iter().find(|k| k.pts == Some(512 * 30)).unwrap().v, perito);
        assert!(anc.windows(2).all(|w| w[0].t < w[1].t) && anc.iter().all(|k| k.t == k.pts.unwrap() as f64 / 15360.0));
        // as âncoras reproduzem cada amostra dentro da tolerância (4 % do lado médio, mínimo 2 px)
        let mut sem_perito = anc.clone();
        sem_perito.retain(|k| k.pts != Some(512 * 30));
        sem_perito.push(Chave { t: em(30).t, pts: Some(512 * 30), v: Caixa { x: d.x[30], y: d.y[30], w: d.w[30], h: d.h[30] }, interp: Interp::Linear });
        sem_perito.sort_by(|a, b| a.t.total_cmp(&b.t));
        let r2 = rastreio(sem_perito, OrigemRastreio::Assistido);
        for k in (0..=140).filter(|k| !(70..=74).contains(k)) {
            let a = avaliar(&r2, None, em(k)).unwrap();
            let c = px(k).unwrap();
            let v = [a.caixa.x * 640.0, a.caixa.y * 360.0, a.caixa.w * 640.0, a.caixa.h * 360.0];
            let lim = (0.04 * 0.5 * (c[2] + c[3])).max(2.0) + 1e-6;
            let e = (v[0] - c[0]).abs().max((v[1] - c[1]).abs()).max((v[0] + v[2] - c[0] - c[2]).abs()).max((v[1] + v[3] - c[1] - c[3]).abs());
            assert!(e <= lim, "quadro {k}: erro {e} > {lim}");
            assert!(matches!(a.como, Como::Ancora | Como::Interpolado));
        }
        assert!(aplicar(&r, Some(&d), &OpRastreio::Simplificar { tolerancia: 0.0 }, TB).is_err());
        assert!(aplicar(&r, Some(&d), &OpRastreio::Simplificar { tolerancia: f64::NAN }, TB).is_err());
        assert_eq!(aplicar(&r, None, &OpRastreio::Simplificar { tolerancia: 0.04 }, TB).unwrap(), (None, None));
    }

    /// Taxa variável (passo de 256 tiques até o quadro 70, depois 768): a fração entre duas chaves é pelo tempo, como
    /// na avaliação. A caixa anda 2 px por QUADRO: reta por índice, quebrada no tempo; as âncoras reproduzem cada amostra.
    #[test]
    fn simplificar_pelo_tempo_em_taxa_variavel() {
        let pts = |k: i64| if k <= 70 { 256 * k } else { 256 * 70 + 768 * (k - 70) };
        let mut d = com_cabecalho(denso_teste(0, 140, |_| ESTADO_OK, |_| TIPO_MEDIDO));
        for k in 0..=140i64 {
            let j = k as usize;
            d.pts[j] = pts(k);
            (d.x[j], d.y[j], d.w[j], d.h[j]) = ((100.0 + 2.0 * k as f64) / 640.0, 0.2, 0.1, 0.2);
        }
        let r = rastreio(vec![], OrigemRastreio::Assistido);
        let (anc, _) = aplicar(&r, Some(&d), &OpRastreio::Simplificar { tolerancia: 0.04 }, TB).unwrap();
        let anc = anc.unwrap();
        assert!(anc.len() >= 3 && anc.len() < 10, "{}", anc.len());
        let r2 = rastreio(anc, OrigemRastreio::Assistido);
        for k in 0..=140i64 {
            let e = Instante { t: pts(k) as f64 / 15360.0, pts: Some(pts(k)) };
            let a = avaliar(&r2, None, e).unwrap();
            let erro = (a.caixa.x * 640.0 - (100.0 + 2.0 * k as f64)).abs();
            assert!(erro <= 2.0 + 1e-6, "quadro {k}: {erro:.2} px");
        }
    }

    /// Denso imutável com o histórico da tela: cada corrida e cada edição dão um arquivo novo; desfazer e refazer só
    /// trocam o ponteiro (todo ponteiro do histórico continua legível); ao fechar, sai só o que nenhum estado cita.
    #[test]
    fn orfao_sai_ao_fechar() {
        use crate::edicao::testes::aceite;
        let ws = tempfile::tempdir().unwrap();
        let p0 = aceite::projeto(640, 360, (30, 1), vec![aceite::midia("m1", "video", json!([]))], vec![], json!({ "rastreios": [aceite::rastreio("r1", Some("m1"), "fonte", vec![aceite::ancora(0.0, Some(0), [0.1, 0.1, 0.1, 0.1])])] }));
        let pasta = armazem::pasta_projeto(ws.path(), &p0.id);
        let doc = |p: &crate::edicao::modelo::ProjetoEdicao| serde_json::to_value(p).unwrap();
        armazem::salvar(ws.path(), &doc(&p0)).unwrap();
        // corrida (A), "Retomar" (B), "Simplificar" pelo comando (C não existe: vira âncoras)
        let a: Vec<_> = (0..=40).map(|k| (k, Some(0.1))).collect();
        let mut da = juntar(None, &res(Sentido::Frente, 0, 0, 40, &a)).unwrap();
        (da.rastreio, da.midia_sha256, da.espaco, da.pixel) = ("r1".into(), "0".repeat(64), "original".into(), TamanhoPx { w: 640, h: 360 });
        let pa = gravar(&pasta, &da, TB).unwrap();
        let b: Vec<_> = (20..=60).map(|k| (k, Some(0.3))).collect();
        let pb = gravar(&pasta, &juntar(Some(&da), &res(Sentido::Frente, 20, 20, 60, &b)).unwrap(), TB).unwrap();
        let estado = |d: Option<&Denso>| {
            let mut p = p0.clone();
            p.rastreios[0].denso = d.cloned();
            p
        };
        let (s1, s2) = (estado(Some(&pa)), estado(Some(&pb)));
        // Ctrl+Z, Ctrl+Y, Ctrl+Z: cada estado lê o seu denso
        for s in [&s1, &s2, &s1, &s2, &s1] {
            armazem::salvar(ws.path(), &doc(s)).unwrap();
            let d = s.rastreios[0].denso.as_ref().unwrap();
            assert_eq!(ler(&pasta, "r1", &d.sha256).unwrap().pts.len() as u32, d.n);
        }
        // fechar com B ainda no histórico da tela: fica
        assert_eq!(armazem::limpar_orfaos(ws.path(), &s1, &[pb.sha256.clone()]).unwrap(), 0);
        assert!(ler(&pasta, "r1", &pb.sha256).is_ok());
        // fechar com o histórico vazio: B (que só o refazer citava) sai, A fica
        assert_eq!(armazem::limpar_orfaos(ws.path(), &s1, &[]).unwrap(), 1);
        assert!(ler(&pasta, "r1", &pa.sha256).is_ok());
        assert!(ler(&pasta, "r1", &pb.sha256).unwrap_err().to_string().contains(codigo::RASTREIO_AUSENTE));
        let restantes: Vec<_> = std::fs::read_dir(pasta.join("rastreios/r1")).unwrap().flatten().map(|e| e.file_name().into_string().unwrap()).collect();
        assert_eq!(restantes, vec![format!("{}.json", pa.sha256)]);
    }
}
