//! Denso imutável e endereçado pelo conteúdo (§6.1): `rastreios/<rid>/<sha256>.json` na pasta do projeto. Corrida,
//! "Apagar daqui em diante" e "Simplificar" gravam um arquivo NOVO e devolvem o ponteiro; quem troca o ponteiro é o
//! front (`alterar()`). Dono: P2.2.
//!
//! Linhas em ordem de pts, sem pts repetido. Buraco reencontrado dentro de uma corrida = linha ausente (B24).
//! "Simplificar" troca o denso por âncoras (o rastreio fica sem denso); as corridas dele ficam no projeto
//! (`Rastreio.corridas_do_programa`, regra em `corridas_do_programa`): o denso vira órfão e sai ao fechar.
//!
//! Quem contribui para cada linha, sem campo novo no formato: toda corrida substitui a faixa inteira que percorreu,
//! então a linha que existe foi escrita pela corrida mais nova de `corridas` cuja faixa cobre o pts (a ESCRITORA). A
//! linha pura cita a escritora; a linha fundida cita a outra corrida da fusão (a oposta, mais antiga). `corridas[]` =
//! exatamente as corridas que contribuem para alguma linha (citada ou escritora).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{laco, AmostraCorrida, ArquivoDenso, ArquivoSilhuetas, Corrida, ResultadoCorrida, Sentido, ESTADO_AUSENTE, ESTADO_INCERTO, ESTADO_OK, TIPO_ESTENDIDO, TIPO_MEDIDO};
use crate::edicao::armazem;
use crate::edicao::executar::{self, sha256_bytes, sha_valido};
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{Caixa, Chave, Denso, Interp, Racional, Rastreio, TamanhoPx};
use crate::edicao::quadros::TabelaQuadros;
use crate::error::{Result, SicroError};

/// Esquema do arquivo denso (§6.1). O 1 (fixtures da Fase 0) ainda é lido.
pub const ESQUEMA_DENSO: u32 = 2;
/// Teto do arquivo lido do disco (10 min a 30 qps ≈ 1 MB; 2 h a 60 qps cabe com folga).
pub const MAX_BYTES_DENSO: u64 = 256 << 20;
/// "Simplificar" sem denso: caixa do programa a até 2 s de uma correção do perito e com lado (raiz da área) 1,5×
/// maior ou menor que a dela é descartada.
const PERTO_DO_PERITO_S: f64 = 2.0;
const TAMANHO_FORA: f64 = 1.5;
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
    /// `suavizar_s` (rastreio só de anotação): média móvel centrada nessa janela antes de escolher as âncoras; sem ela
    /// o tremido do motor vira uma âncora por quadro.
    // CONTRATO: `suavizar_s` acrescentado no teste do dono (16/11), opcional; espelho em `src/types/edicao.ts`.
    Simplificar {
        tolerancia: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        suavizar_s: Option<f64>,
    },
}

/// Resultado de uma edição: o front grava `ancoras` quando vem e `denso` SEMPRE (null = rastreio sem denso).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Editado {
    pub ancoras: Option<Vec<Chave<Caixa>>>,
    pub denso: Option<Denso>,
    /// `Rastreio.corridas_do_programa` novo (vem com `ancoras`; vazio = nenhuma); None = fica como está.
    // CONTRATO: acréscimo (17/11, o "Simplificar" apagava o programa usado); espelho em `src/types/edicao.ts`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corridas_do_programa: Option<Vec<serde_json::Value>>,
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

/// Sem denso (um "Simplificar" de antes já o trocou por âncoras, uma por quadro quando o motor tremia): as âncoras do
/// programa são suavizadas e reduzidas; as do perito ficam como estão. None = menos de 3 do programa.
fn simplificar_ancoras(r: &Rastreio, tolerancia: f64, janela_s: f64, tb: Racional) -> Option<Vec<Chave<Caixa>>> {
    let programa: BTreeSet<u64> = r.ancoras_do_programa.iter().map(|t| t.to_bits()).collect();
    let do_programa = |k: &Chave<Caixa>| programa.contains(&k.t.to_bits());
    if r.ancoras.iter().filter(|k| do_programa(k)).count() < 3 {
        return None;
    }
    let mut ks: Vec<&Chave<Caixa>> = r.ancoras.iter().collect();
    ks.sort_by(|a, b| a.t.total_cmp(&b.t));
    // perto de uma correção do perito, a caixa do programa com outro tamanho é o motor que cresceu (rosto → corpo): sai
    let perito: Vec<&Chave<Caixa>> = ks.iter().copied().filter(|k| !do_programa(k)).collect();
    let fora_do_tamanho = |k: &Chave<Caixa>| -> bool {
        let i = perito.partition_point(|p| p.t <= k.t);
        let perto: Vec<&Chave<Caixa>> = [i.checked_sub(1).map(|j| perito[j]), perito.get(i).copied()].into_iter().flatten().filter(|p| (p.t - k.t).abs() <= PERTO_DO_PERITO_S).collect();
        let Some(&p) = perto.iter().min_by(|a, b| (a.t - k.t).abs().total_cmp(&(b.t - k.t).abs())) else { return false };
        let razao = (k.v.w * k.v.h / (p.v.w * p.v.h).max(1e-12)).sqrt();
        !(1.0 / TAMANHO_FORA..=TAMANHO_FORA).contains(&razao)
    };
    ks.retain(|k| !do_programa(k) || !fora_do_tamanho(k));
    let pts = |k: &Chave<Caixa>| k.pts.unwrap_or_else(|| (k.t * tb.den as f64 / tb.num as f64).round() as i64);
    let mut am: Vec<AmostraCorrida> = ks.iter().map(|k| AmostraCorrida { pts: pts(k), caixa: Some(k.v), conf: 1.0, estado: ESTADO_OK, silhueta: None }).collect();
    suavizar(&mut am, janela_s, tb);
    // as do perito não mudam (entram na média dos vizinhos, mas ficam onde ele pôs)
    for (a, k) in am.iter_mut().zip(&ks) {
        if !do_programa(k) {
            a.caixa = Some(k.v);
        }
    }
    // o denso não existe mais: o tamanho do quadro só pesa no piso de 2 px
    let pixel = TamanhoPx { w: 1920, h: 1080 };
    let escolhidas: BTreeSet<usize> = com_teto(&am, laco::quadros_chave(&am, pixel, tolerancia, PISO_PX), tb).into_iter().collect();
    let mut out: Vec<Chave<Caixa>> = ks
        .iter()
        .zip(&am)
        .enumerate()
        .filter(|(i, (k, _))| !do_programa(k) || escolhidas.contains(i))
        .map(|(_, (k, a))| if do_programa(k) { Chave { t: k.t, pts: k.pts, v: a.caixa.expect("com caixa"), interp: Interp::Linear } } else { (*k).clone() })
        .collect();
    out.sort_by(|a, b| a.t.total_cmp(&b.t).then(a.pts.cmp(&b.pts)));
    Some(out)
}

/// Âncoras do "Simplificar" a no máximo `ESPACO_MAX_S` uma da outra quando há amostra no meio (pessoa parada dá
/// pontas longe; mais que `BURACO_S` viraria buraco). Buraco de verdade (sem amostra) fica como está.
const ESPACO_MAX_S: f64 = super::avaliar::BURACO_S / 2.0;
fn com_teto(am: &[AmostraCorrida], chaves: Vec<usize>, tb: Racional) -> Vec<usize> {
    let t = |i: usize| tempo(am[i].pts, tb);
    let mut out = Vec::with_capacity(chaves.len());
    for w in chaves.windows(2) {
        let (a, b) = (w[0], w[1]);
        out.push(a);
        let partes = ((t(b) - t(a)) / ESPACO_MAX_S).ceil() as usize;
        let mut ultimo = a;
        for m in 1..partes {
            let alvo = t(a) + (t(b) - t(a)) * m as f64 / partes as f64;
            // a amostra com caixa mais perto do alvo, depois da última posta
            let melhor = (ultimo + 1..b).filter(|&i| am[i].caixa.is_some()).min_by(|&x, &y| (t(x) - alvo).abs().total_cmp(&(t(y) - alvo).abs()));
            if let Some(i) = melhor {
                out.push(i);
                ultimo = i;
            }
        }
    }
    if let Some(&z) = chaves.last() {
        out.push(z);
    }
    out.dedup();
    out
}

/// Média móvel centrada (janela `janela_s`, pelo tempo de cada quadro) de x, y, w e h das amostras com caixa.
fn suavizar(am: &mut [AmostraCorrida], janela_s: f64, tb: Racional) {
    let com: Vec<(f64, Caixa)> = am.iter().filter_map(|a| a.caixa.map(|c| (tempo(a.pts, tb), c))).collect();
    if com.len() < 3 {
        return;
    }
    let meia = janela_s / 2.0;
    let (mut i0, mut i1, mut soma, mut n) = (0usize, 0usize, [0f64; 4], 0f64);
    let mut lisas = Vec::with_capacity(com.len());
    for &(t, _) in &com {
        while i1 < com.len() && com[i1].0 <= t + meia {
            let c = com[i1].1;
            (soma[0], soma[1], soma[2], soma[3], n) = (soma[0] + c.x, soma[1] + c.y, soma[2] + c.w, soma[3] + c.h, n + 1.0);
            i1 += 1;
        }
        while com[i0].0 < t - meia {
            let c = com[i0].1;
            (soma[0], soma[1], soma[2], soma[3], n) = (soma[0] - c.x, soma[1] - c.y, soma[2] - c.w, soma[3] - c.h, n - 1.0);
            i0 += 1;
        }
        lisas.push(Caixa { x: soma[0] / n, y: soma[1] / n, w: soma[2] / n, h: soma[3] / n });
    }
    let mut it = lisas.into_iter();
    for a in am.iter_mut().filter(|a| a.caixa.is_some()) {
        a.caixa = it.next();
    }
}

fn amostra(l: &Linha) -> AmostraCorrida {
    let caixa = (l.estado != ESTADO_AUSENTE).then(|| Caixa { x: l.caixa[0], y: l.caixa[1], w: l.caixa[2], h: l.caixa[3] });
    AmostraCorrida { pts: l.pts, caixa, conf: l.conf, estado: l.estado, silhueta: None }
}

/// A `Corrida` de cada posição de `corridas` (None = entrada que não é corrida: arquivo da Fase 0, trilha de Pessoas).
fn tipadas(corridas: &[serde_json::Value]) -> Vec<Option<Corrida>> {
    corridas.iter().map(|v| serde_json::from_value::<Corrida>(v.clone()).ok()).collect()
}

/// Escritora de cada linha (pts em ordem): a corrida de maior índice cuja faixa percorrida cobre o pts; None = nenhuma.
fn escritoras(pts: &[i64], tipadas: &[Option<Corrida>]) -> Vec<Option<u32>> {
    let n = pts.len();
    let mut esc = vec![None; n];
    // próxima linha ainda sem escritora (com atalho), para não repassar o que a mais nova já marcou
    let mut prox: Vec<usize> = (0..=n).collect();
    let achar = |prox: &mut Vec<usize>, mut i: usize| -> usize {
        let mut r = i;
        while prox[r] != r {
            r = prox[r];
        }
        while prox[i] != r {
            let seguinte = prox[i];
            prox[i] = r;
            i = seguinte;
        }
        r
    };
    for (k, c) in tipadas.iter().enumerate().rev() {
        let Some(c) = c else { continue };
        let (a, b) = (pts.partition_point(|&p| p < c.de_pts), pts.partition_point(|&p| p <= c.ate_pts));
        let mut i = achar(&mut prox, a);
        while i < b {
            esc[i] = Some(k as u32);
            prox[i] = i + 1;
            i = achar(&mut prox, i + 1);
        }
    }
    esc
}

/// Põe as linhas (em ordem de pts) no denso e deixa em `corridas` só as que contribuem para alguma linha (a citada e,
/// na linha fundida, a escritora), na ordem em que estavam.
fn montar(base: &ArquivoDenso, mut ls: Vec<Linha>, corridas: Vec<serde_json::Value>) -> ArquivoDenso {
    ls.sort_by_key(|l| l.pts);
    let esc = escritoras(&ls.iter().map(|l| l.pts).collect::<Vec<_>>(), &tipadas(&corridas));
    let contribuem: BTreeSet<u32> = ls.iter().zip(&esc).flat_map(|(l, w)| [Some(l.corrida), *w]).flatten().collect();
    let mut novo_indice = BTreeMap::new();
    let mut ficam = Vec::new();
    for (i, c) in corridas.into_iter().enumerate() {
        if contribuem.contains(&(i as u32)) {
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
/// uma de frente para a outra), `fundir`, só com as linhas PURAS da outra (a mistura de uma fusão anterior não se
/// desfaz e não entra). A linha em que a oposta entra cita a oposta; a nova é a escritora dela.
/// A corrida nova vai para o fim de `corridas`; as que não contribuem para nenhuma linha saem.
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

    let tipadas_base = tipadas(&base.corridas);
    let esc = escritoras(&base.pts, &tipadas_base);
    // linha pura da corrida i: cita i e foi i quem a escreveu
    let pura = |j: usize, i: u32| velhas[j].corrida == i && esc[j] == Some(i);
    let puras_na_faixa: BTreeSet<u32> = (0..velhas.len()).filter(|&j| velhas[j].pts >= lo && velhas[j].pts <= hi && pura(j, velhas[j].corrida)).map(|j| velhas[j].corrida).collect();
    // a corrida do outro sentido cuja âncora está adiante, no fim da faixa percorrida (ou além), com linha pura nela
    let oposta = tipadas_base
        .iter()
        .enumerate()
        .filter_map(|(i, o)| o.as_ref().map(|o| (i as u32, o.sentido, o.ancora_pts)))
        .filter(|&(_, sentido, _)| sentido != c.sentido)
        .filter(|&(_, _, y)| match c.sentido {
            Sentido::Frente => y >= hi && y > x,
            Sentido::Tras => y <= lo && y < x,
        })
        .filter(|&(i, _, _)| puras_na_faixa.contains(&i))
        .min_by_key(|&(_, _, y)| (y - x).abs())
        .map(|(i, _, y)| (i, y));

    let ls = match oposta {
        None => {
            let mut ls: Vec<Linha> = velhas.into_iter().filter(|l| l.pts < lo || l.pts > hi).collect();
            ls.extend(res.amostras.iter().map(|a| da_amostra(a, nova)));
            ls
        }
        Some((i, y)) => {
            let (a, b) = (x.min(y), x.max(y));
            let dela: Vec<bool> = (0..velhas.len()).map(|j| pura(j, i) && velhas[j].pts >= a && velhas[j].pts <= b).collect();
            let delas: Vec<AmostraCorrida> = velhas.iter().zip(&dela).filter(|(_, &e)| e).map(|(l, _)| amostra(l)).collect();
            let com_ela: BTreeSet<i64> = delas.iter().map(|d| d.pts).collect();
            let (frente, tras) = match c.sentido {
                Sentido::Frente => (res.amostras.as_slice(), delas.as_slice()),
                Sentido::Tras => (delas.as_slice(), res.amostras.as_slice()),
            };
            let fundidas = super::fundir::fundir(frente, tras, a, b);
            // as linhas puras da oposta na faixa da fusão voltam pela fusão; da faixa percorrida, nada velho fica
            let mut ls: Vec<Linha> = velhas.iter().zip(&dela).filter(|(l, &e)| (l.pts < lo || l.pts > hi) && !e).map(|(l, _)| l.clone()).collect();
            ls.extend(fundidas.iter().map(|f| da_amostra(f, if com_ela.contains(&f.pts) { i } else { nova })));
            ls
        }
    };
    Ok(montar(base, ls, corridas))
}

/// Silhuetas do denso juntado: as da corrida nova (a última de `d.corridas`) nas linhas que ela escreveu sozinha; na
/// linha fundida, a do lado da âncora mais perto (a outra, se esse lado não tem); nos outros quadros, as da base.
/// None = nenhum quadro com silhueta.
pub(crate) fn silhuetas_juntadas(d: &ArquivoDenso, res: &ResultadoCorrida, base: Option<&ArquivoSilhuetas>, modelo: &str) -> Option<ArquivoSilhuetas> {
    let nova = d.corridas.len().checked_sub(1)? as u32;
    let tip = tipadas(&d.corridas);
    let esc = escritoras(&d.pts, &tip);
    let da_corrida: BTreeMap<i64, &Vec<u8>> = res.amostras.iter().filter_map(|a| a.silhueta.as_ref().map(|s| (a.pts, s))).collect();
    let da_base: BTreeMap<i64, &Vec<u8>> = base.map(|b| b.pts.iter().copied().zip(b.mascaras.iter()).collect()).unwrap_or_default();
    let (mut pts, mut mascaras) = (Vec::new(), Vec::new());
    for i in 0..d.pts.len() {
        if d.estado[i] == ESTADO_AUSENTE {
            continue;
        }
        let p = d.pts[i];
        let m = if d.corrida[i] == nova {
            da_corrida.get(&p)
        } else if esc[i] == Some(nova) {
            let outra = tip.get(d.corrida[i] as usize).and_then(|o| o.as_ref()).map(|o| o.ancora_pts);
            let perto_da_nova = outra.is_none_or(|y| (p - res.corrida.ancora_pts).abs() <= (p - y).abs());
            let (um, outro) = if perto_da_nova { (da_corrida.get(&p), da_base.get(&p)) } else { (da_base.get(&p), da_corrida.get(&p)) };
            um.or(outro)
        } else {
            da_base.get(&p)
        };
        if let Some(m) = m {
            pts.push(p);
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
    // só a mídia do rastreio (o projeto inteiro calcularia o SHA-256 da sessão de todas)
    let (_, fonte) = executar::preparar_uma(ws, &p, midia, cache, &std::sync::atomic::AtomicBool::new(false))?;
    let tb = fonte.quadros.tb;
    let (ancoras, novo) = aplicar_com(r, denso.as_ref(), op, tb, Some(&fonte.quadros))?;
    let corridas_do_programa = corridas_do_programa(r, denso.as_ref(), op, ancoras.as_deref(), tb);
    let denso = match novo {
        Some(mut d) => {
            d.rastreio = r.id.clone();
            d.midia_sha256 = m.sha256.clone();
            Some(gravar(&pasta, &d, tb)?)
        }
        None => None,
    };
    Ok(Editado { ancoras, denso, corridas_do_programa })
}

/// Até onde da âncora do programa vão as amostras que a média do "Simplificar" mistura: metade da maior janela
/// (`suavizar_s` ≤ 10 s).
const ALCANCE_S: f64 = 5.0;

/// `Rastreio.corridas_do_programa` depois de `op`, que deixou as âncoras `novas` (None = âncoras sem mudança: fica).
/// O "Simplificar" com denso acrescenta as corridas dele, todas (`corridas[]` = as que deram alguma linha; o denso sem
/// nenhuma dá `null`); âncora do programa de antes sem registro vira `null`. Fica a corrida cuja faixa (mais
/// `ALCANCE_S`) cobre o instante de alguma âncora do programa que sobrou: TODAS as que cobrem, não só a última (a
/// regra de `juri::corridas_no_video`), que a âncora de um "Simplificar" anterior continua por cima da faixa de uma
/// corrida mais nova: nunca IA a menos. `null` fica enquanto sobrar âncora do programa.
pub(crate) fn corridas_do_programa(r: &Rastreio, denso: Option<&ArquivoDenso>, op: &OpRastreio, novas: Option<&[Chave<Caixa>]>, tb: Racional) -> Option<Vec<serde_json::Value>> {
    let novas = novas?;
    // a mesma tolerância do motor (`motor::do_programa`)
    let do_programa = |t: f64| r.ancoras_do_programa.iter().any(|x| (x - t).abs() <= 1e-6);
    let programa: Vec<&Chave<Caixa>> = match op {
        // o espelho de `ancorasDoSimplificar` (front): as devolvidas que não são âncora do perito (mesmo `t` e caixa)
        OpRastreio::Simplificar { .. } => {
            let perito: Vec<&Chave<Caixa>> = r.ancoras.iter().filter(|k| !do_programa(k.t)).collect();
            novas.iter().filter(|k| !perito.iter().any(|p| p.t == k.t && p.v == k.v)).collect()
        }
        OpRastreio::ApagarFaixa { .. } => novas.iter().filter(|k| do_programa(k.t)).collect(),
    };
    let mut lista = r.corridas_do_programa.clone();
    if lista.is_empty() && r.ancoras.iter().any(|k| do_programa(k.t)) {
        lista.push(serde_json::Value::Null);
    }
    if let (OpRastreio::Simplificar { .. }, Some(d)) = (op, denso) {
        if d.corridas.is_empty() {
            lista.push(serde_json::Value::Null);
        }
        lista.extend(d.corridas.iter().cloned());
    }
    let pts = |k: &Chave<Caixa>| k.pts.unwrap_or_else(|| (k.t * tb.den as f64 / tb.num as f64).round() as i64);
    let instantes: Vec<i64> = programa.iter().map(|k| pts(k)).collect();
    let alcance = (ALCANCE_S * tb.den as f64 / tb.num as f64).ceil() as i64;
    let mut fica: Vec<serde_json::Value> = Vec::new();
    for c in lista {
        let cobre = match serde_json::from_value::<Corrida>(c.clone()) {
            Ok(x) => instantes.iter().any(|&p| p >= x.de_pts.min(x.ate_pts).saturating_sub(alcance) && p <= x.de_pts.max(x.ate_pts).saturating_add(alcance)),
            Err(_) => !instantes.is_empty(),
        };
        if cobre && !fica.contains(&c) {
            fica.push(c);
        }
    }
    Some(fica)
}

/// Trechos contínuos das linhas. Com a tabela do decodificador que tem todos os pts do denso: duas linhas seguidas são
/// do mesmo trecho quando são quadros vizinhos na tabela (o buraco é quadro que falta, não passo longo: CFTV que cai
/// de taxa não vira uma âncora por quadro). Sem ela: passo maior que `SALTO_DE_TRECHO` × o passo mediano separa.
fn trechos(d: &ArquivoDenso, tab: Option<&TabelaQuadros>) -> Vec<std::ops::Range<usize>> {
    let n = d.pts.len();
    if n == 0 {
        return vec![];
    }
    if let Some(tab) = tab.filter(|t| d.pts.iter().all(|&p| t.pts.binary_search(&p).is_ok())) {
        // o pts seguinte ao de `p` na tabela (pts repetido do MPEG-PS: o grupo inteiro fica para trás)
        let seguinte = |p: i64| tab.pts.get(tab.pts.partition_point(|&x| x <= p)).copied();
        let mut v = Vec::new();
        let mut ini = 0;
        for i in 1..=n {
            if i == n || seguinte(d.pts[i - 1]) != Some(d.pts[i]) {
                v.push(ini..i);
                ini = i;
            }
        }
        return v;
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
    aplicar_com(r, denso, op, tb, None)
}

/// `aplicar` com a tabela de quadros da mídia (os trechos do "Simplificar" pelos quadros vizinhos dela).
pub(crate) fn aplicar_com(
    r: &Rastreio,
    denso: Option<&ArquivoDenso>,
    op: &OpRastreio,
    tb: Racional,
    tab: Option<&TabelaQuadros>,
) -> Result<(Option<Vec<Chave<Caixa>>>, Option<ArquivoDenso>)> {
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
        OpRastreio::Simplificar { tolerancia, suavizar_s } => {
            if !(tolerancia.is_finite() && tolerancia > 0.0 && tolerancia <= 1.0) {
                return Err(invalido("tolerância"));
            }
            if suavizar_s.is_some_and(|s| !(s.is_finite() && s > 0.0 && s <= 10.0)) {
                return Err(invalido("suavizar"));
            }
            let Some(d) = denso else {
                return Ok(match suavizar_s {
                    Some(s) => (simplificar_ancoras(r, tolerancia, s, tb), None),
                    None => (None, None),
                });
            };
            let ls = linhas(d);
            let mut novas: Vec<Chave<Caixa>> = Vec::new();
            for tr in trechos(d, tab) {
                let mut am: Vec<AmostraCorrida> = ls[tr.clone()].iter().map(amostra).collect();
                if let Some(s) = suavizar_s {
                    suavizar(&mut am, s, tb);
                }
                for k in com_teto(&am, laco::quadros_chave(&am, d.pixel, tolerancia, PISO_PX), tb) {
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
                // só a de trás tem caixa: fica com ela
                assert_eq!(x, 0.8, "{k}");
            } else {
                let wt = k as f64 / 100.0;
                assert_eq!(x, (1.0 - wt) * 0.2 + wt * 0.8, "{k}");
            }
            // onde a de trás entra, a linha cita a de trás (a oposta); a nova é a escritora. Antes de 30, só a nova
            assert_eq!(corrida_em(&d, k), if k < 30 { 1 } else { 0 }, "{k}");
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

    /// "Ambos" a partir de uma âncora no meio de [A, B] já fundido: a corrida para a frente não tira de `corridas` a de
    /// trás velha, que ainda entra nas linhas fundidas de [A, C); a para trás então não funde com essa mistura. Nada da
    /// corrida velha de trás (x = 0,9) sobra em [A, C]; no fim, só as duas novas em `corridas`.
    #[test]
    fn ambos_do_meio_de_uma_fusao_nao_guarda_a_velha() {
        let f0: Vec<_> = (0..=100).map(|k| (k, Some(0.1))).collect();
        let d0 = com_cabecalho(juntar(None, &res(Sentido::Frente, 0, 0, 100, &f0)).unwrap());
        let t1: Vec<_> = (0..=100).map(|k| (k, Some(0.9))).collect();
        let d1 = juntar(Some(&d0), &res(Sentido::Tras, 100, 0, 100, &t1)).unwrap();
        assert_eq!(x_em(&d1, 50), Some(0.5 * 0.1 + 0.5 * 0.9));
        // "ambos" da âncora 50: para a frente até a âncora 100, depois para trás até a âncora 0
        let f2: Vec<_> = (50..=100).map(|k| (k, Some(0.5))).collect();
        let d2 = juntar(Some(&d1), &res(Sentido::Frente, 50, 50, 100, &f2)).unwrap();
        // a de trás velha entra nas linhas fundidas de 0 a 49: fica em `corridas`
        assert_eq!(corridas(&d2).unwrap().iter().map(|c| (c.sentido, c.ancora_pts / 512)).collect::<Vec<_>>(), vec![(Sentido::Frente, 0), (Sentido::Tras, 100), (Sentido::Frente, 50)]);
        let t3: Vec<_> = (0..=50).map(|k| (k, Some(0.3))).collect();
        let d3 = juntar(Some(&d2), &res(Sentido::Tras, 50, 0, 50, &t3)).unwrap();
        for k in 0..=100 {
            assert_eq!(x_em(&d3, k), Some(if k <= 50 { 0.3 } else { 0.5 }), "quadro {k}");
        }
        assert_eq!(corridas(&d3).unwrap().iter().map(|c| (c.sentido, c.ancora_pts / 512)).collect::<Vec<_>>(), vec![(Sentido::Frente, 50), (Sentido::Tras, 50)]);
    }

    /// Modelo de referência do `juntar`: cada linha com o valor e o CONJUNTO das corridas que entram nela (o formato só
    /// guarda uma por linha). Corridas sorteadas (sentido, âncora, faixa, buracos, cauda perdida) e "Apagar daqui em
    /// diante": a cada passo, as linhas, os valores, quem contribui em cada linha (citada + escritora) e `corridas[]`
    /// (exatamente as que contribuem) iguais ao modelo.
    #[test]
    fn corridas_sao_exatamente_as_que_contribuem() {
        use std::collections::BTreeMap as Mapa;
        #[derive(Clone, Debug)]
        struct Linha {
            x: Option<f64>,
            quem: BTreeSet<usize>,
        }
        struct Modelo {
            linhas: Mapa<i64, Linha>,
            corridas: Vec<Corrida>,
            fusoes: usize,
        }
        impl Modelo {
            fn vivas(&self) -> BTreeSet<usize> {
                self.linhas.values().flat_map(|l| l.quem.iter().copied()).collect()
            }
            fn juntar(&mut self, r: &ResultadoCorrida) {
                let c = r.corrida.clone();
                let n = self.corridas.len();
                self.corridas.push(c.clone());
                let (lo, hi, x) = (c.de_pts / 512, c.ate_pts / 512, c.ancora_pts / 512);
                let vivas = self.vivas();
                let pura = |l: &Linha, o: usize| l.quem.len() == 1 && l.quem.contains(&o);
                let oposta = vivas
                    .iter()
                    .copied()
                    .filter(|&o| {
                        let k = &self.corridas[o];
                        let y = k.ancora_pts / 512;
                        k.sentido != c.sentido
                            && match c.sentido {
                                Sentido::Frente => y >= hi && y > x,
                                Sentido::Tras => y <= lo && y < x,
                            }
                            && self.linhas.range(512 * lo..=512 * hi).any(|(_, l)| pura(l, o))
                    })
                    .min_by_key(|&o| ((self.corridas[o].ancora_pts / 512) - x).abs());
                let novas: Mapa<i64, Option<f64>> = r.amostras.iter().map(|a| (a.pts, a.caixa.map(|b| b.x))).collect();
                match oposta {
                    None => {
                        self.linhas.retain(|&p, _| p < 512 * lo || p > 512 * hi);
                        for (&p, &v) in &novas {
                            self.linhas.insert(p, Linha { x: v, quem: [n].into() });
                        }
                    }
                    Some(o) => {
                        self.fusoes += 1;
                        let y = self.corridas[o].ancora_pts / 512;
                        let (a, b) = (x.min(y), x.max(y));
                        let delas: Mapa<i64, Option<f64>> = self.linhas.range(512 * a..=512 * b).filter(|(_, l)| pura(l, o)).map(|(&p, l)| (p, l.x)).collect();
                        self.linhas.retain(|&p, _| (p < 512 * lo || p > 512 * hi) && !delas.contains_key(&p));
                        let am = |m: &Mapa<i64, Option<f64>>| -> Vec<AmostraCorrida> {
                            m.iter().map(|(&p, &v)| AmostraCorrida { pts: p, caixa: v.map(|x| Caixa { x, y: 0.25, w: 0.1, h: 0.2 }), conf: 0.9, estado: if v.is_some() { ESTADO_OK } else { ESTADO_AUSENTE }, silhueta: None }).collect()
                        };
                        let (fr, tr) = match c.sentido {
                            Sentido::Frente => (am(&novas), am(&delas)),
                            Sentido::Tras => (am(&delas), am(&novas)),
                        };
                        for f in super::super::fundir::fundir(&fr, &tr, 512 * a.min(b), 512 * b) {
                            // a corrida nova leu o quadro (amostra ou cauda perdida): entra
                            let leu = f.pts >= 512 * lo && f.pts <= 512 * hi;
                            let quem: BTreeSet<usize> = match (delas.contains_key(&f.pts), leu) {
                                (true, true) => [o, n].into(),
                                (true, false) => [o].into(),
                                (false, _) => [n].into(),
                            };
                            self.linhas.insert(f.pts, Linha { x: f.caixa.map(|b| b.x), quem });
                        }
                    }
                }
            }
        }
        let mut sorteio = 0x5eed_1234_u64;
        let mut tirar = |n: u64| {
            sorteio ^= sorteio << 13;
            sorteio ^= sorteio >> 7;
            sorteio ^= sorteio << 17;
            sorteio % n
        };
        let (mut passos, mut fusoes, mut mistas, mut guardadas) = (0, 0, 0, 0);
        for rodada in 0..1000 {
            let mut m = Modelo { linhas: Mapa::new(), corridas: Vec::new(), fusoes: 0 };
            let mut d: Option<ArquivoDenso> = None;
            for passo in 0..(2 + tirar(7)) {
                let onde = format!("rodada {rodada}, passo {passo}");
                if d.is_some() && tirar(8) == 0 {
                    // "Apagar daqui em diante"
                    let de = tirar(60) as i64;
                    m.linhas.retain(|&p, _| p < 512 * de);
                    let r = rastreio(vec![], OrigemRastreio::Assistido);
                    let (_, novo) = aplicar(&r, d.as_ref(), &OpRastreio::ApagarFaixa { de_pts: 512 * de, ate_pts: None }, TB).unwrap();
                    d = novo.map(com_cabecalho);
                    if d.is_none() {
                        assert!(m.linhas.is_empty(), "{onde}");
                        m.corridas.clear();
                    }
                } else {
                    let frente = tirar(2) == 0;
                    // metade das âncoras numa grade (corridas de frente uma para a outra, como entre marcações do perito)
                    let grade = tirar(2) == 0;
                    let x = if grade { 15 * tirar(5) as i64 } else { tirar(61) as i64 };
                    let alcance = if grade { 15 * (1 + tirar(4) as i64) } else { 1 + tirar(60) as i64 };
                    let (lo, hi) = if frente { (x, (x + alcance).min(60)) } else { ((x - alcance).max(0), x) };
                    let (lo, hi) = (lo.min(hi), hi.max(lo));
                    // cauda perdida: as amostras acabam antes da faixa lida; buracos no meio
                    let corte = tirar(4) as i64;
                    let fim_am = if frente { (hi - corte).max(x) } else { (lo + corte).min(x) };
                    let valor = 0.05 + passo as f64 * 0.1 + rodada as f64 * 1e-4;
                    let ks: Vec<i64> = if frente { (x..=fim_am).collect() } else { (fim_am..=x).collect() };
                    let am: Vec<(i64, Option<f64>)> = ks.iter().map(|&k| (k, (k == x || tirar(6) != 0).then_some(valor + k as f64 * 1e-3))).collect();
                    let mut r = res(if frente { Sentido::Frente } else { Sentido::Tras }, x, lo, hi, &am);
                    r.corrida.feito_em = format!("{rodada}-{passo}");
                    m.juntar(&r);
                    d = Some(com_cabecalho(juntar(d.as_ref(), &r).unwrap()));
                }
                passos += 1;
                let Some(dd) = &d else { continue };
                // linhas e valores
                let linhas: Vec<(i64, Option<f64>)> = (0..dd.pts.len()).map(|i| (dd.pts[i], (dd.estado[i] != ESTADO_AUSENTE).then_some(dd.x[i]))).collect();
                let esperadas: Vec<(i64, Option<f64>)> = m.linhas.iter().map(|(&p, l)| (p, l.x)).collect();
                assert_eq!(linhas, esperadas, "{onde}: linhas");
                // quem contribui, linha a linha: a citada e a escritora
                let tip = tipadas(&dd.corridas);
                let esc = escritoras(&dd.pts, &tip);
                let id = |k: u32| tip[k as usize].as_ref().unwrap().feito_em.clone();
                for (i, (&p, l)) in m.linhas.iter().enumerate() {
                    let derivado: BTreeSet<String> = [Some(dd.corrida[i]), esc[i]].into_iter().flatten().map(id).collect();
                    let modelo: BTreeSet<String> = l.quem.iter().map(|&k| m.corridas[k].feito_em.clone()).collect();
                    assert_eq!(derivado, modelo, "{onde}: quadro {}", p / 512);
                }
                // `corridas[]` = exatamente as que contribuem, na ordem em que nasceram
                let vivas: Vec<String> = m.vivas().into_iter().map(|k| m.corridas[k].feito_em.clone()).collect();
                assert_eq!(dd.corridas.iter().map(|v| v["feito_em"].as_str().unwrap().to_string()).collect::<Vec<_>>(), vivas, "{onde}: corridas");
                mistas += m.linhas.values().filter(|l| l.quem.len() == 2).count();
                // corrida que só fica por entrar numa linha fundida (nenhuma linha a cita)
                guardadas += vivas.iter().filter(|v| !(0..dd.pts.len()).any(|i| id(dd.corrida[i]) == **v)).count();
            }
            fusoes += m.fusoes;
        }
        eprintln!("{passos} passos, {fusoes} fusões, {mistas} linhas fundidas, {guardadas} corridas guardadas só pela fusão");
        assert!(passos > 4000 && fusoes > 400 && mistas > 10000 && guardadas > 300, "{passos} {fusoes} {mistas} {guardadas}");
    }

    /// Silhuetas do denso juntado: a linha pura fica com a da sua corrida; a fundida, com a do lado da âncora mais perto.
    #[test]
    fn silhuetas_da_linha_fundida_pelo_lado_mais_perto() {
        let com_sil = |mut r: ResultadoCorrida, marca: u8| {
            for a in &mut r.amostras {
                a.silhueta = a.caixa.map(|_| vec![marca, (a.pts / 512) as u8]);
            }
            r
        };
        let t: Vec<_> = (30..=100).map(|k| (k, Some(0.8))).collect();
        let rt = com_sil(res(Sentido::Tras, 100, 30, 100, &t), 1);
        let base = com_cabecalho(juntar(None, &rt).unwrap());
        let sil_base = silhuetas_juntadas(&base, &rt, None, "m").unwrap();
        let f: Vec<_> = (0..=100).map(|k| (k, Some(0.2))).collect();
        let rf = com_sil(res(Sentido::Frente, 0, 0, 100, &f), 2);
        let d = com_cabecalho(juntar(Some(&base), &rf).unwrap());
        let s = silhuetas_juntadas(&d, &rf, Some(&sil_base), "m").unwrap();
        assert_eq!(s.pts, (0..=100).map(|k| 512 * k).collect::<Vec<i64>>());
        for (k, m) in s.mascaras.iter().enumerate() {
            // de 0 a 50 a âncora da frente (0) está mais perto; de 51 a 100, a de trás (100)
            assert_eq!(m, &vec![if k <= 50 { 2 } else { 1 }, k as u8], "quadro {k}");
        }
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
    fn simplificar_suavizado_alisa_o_tremido_do_motor() {
        // pessoa andando em reta (caixa ~80×160 px) com tremido de ±6 px quadro a quadro, como a moldura da máscara
        let verdade = |t: f64| [100.0 + 2.0 * t, 100.0 + 0.4 * t, 80.0, 160.0];
        let mut d = com_cabecalho(denso_teste(0, 150, |_| ESTADO_OK, |_| TIPO_MEDIDO));
        for (j, k) in (0..=150).enumerate() {
            let tr = if k % 2 == 0 { 6.0 } else { -6.0 };
            let v = verdade(k as f64);
            let c = [v[0] + tr, v[1] - tr, v[2] + tr, v[3] - tr];
            (d.x[j], d.y[j], d.w[j], d.h[j]) = (c[0] / 640.0, c[1] / 360.0, c[2] / 640.0, c[3] / 360.0);
        }
        let r = rastreio(vec![], OrigemRastreio::Assistido);
        let anc = |s: Option<f64>| aplicar(&r, Some(&d), &OpRastreio::Simplificar { tolerancia: 0.04, suavizar_s: s }, TB).unwrap().0.unwrap();
        // sem suavizar o tremido vira uma âncora por quadro (era o "não funcionou" do dono)
        assert!(anc(None).len() > 100, "{}", anc(None).len());
        let lisas = anc(Some(0.5));
        assert!(lisas.len() <= 6, "{}", lisas.len());
        // o caminho que sobra fica perto da verdade (o tremido some na média)
        let r2 = rastreio(lisas, OrigemRastreio::Assistido);
        for k in 10..=140 {
            let a = avaliar(&r2, None, em(k)).unwrap().caixa;
            let v = verdade(k as f64);
            let e = (a.x * 640.0 - v[0]).abs().max((a.y * 360.0 - v[1]).abs());
            assert!(e < 3.0, "quadro {k}: {e}");
        }
        assert!(aplicar(&r, Some(&d), &OpRastreio::Simplificar { tolerancia: 0.04, suavizar_s: Some(0.0) }, TB).is_err());
        assert!(aplicar(&r, Some(&d), &OpRastreio::Simplificar { tolerancia: 0.04, suavizar_s: Some(f64::NAN) }, TB).is_err());
    }

    #[test]
    fn simplificar_de_novo_sem_denso_alisa_as_ancoras_do_programa() {
        // o "Simplificar" de antes deixou uma âncora tremida por quadro (do programa); o perito corrigiu duas
        let verdade = |t: f64| [100.0 + 2.0 * t, 100.0 + 0.4 * t, 80.0, 160.0];
        let cx = |c: [f64; 4]| Caixa { x: c[0] / 640.0, y: c[1] / 360.0, w: c[2] / 640.0, h: c[3] / 360.0 };
        let mut ks: Vec<Chave<Caixa>> = (0..=150)
            .map(|k: i64| {
                let tr = if k % 2 == 0 { 6.0 } else { -6.0 };
                let v = verdade(k as f64);
                ch(k, cx([v[0] + tr, v[1] - tr, v[2] + tr, v[3] - tr]), Interp::Linear)
            })
            .collect();
        let mut r = rastreio(vec![], OrigemRastreio::Assistido);
        r.ancoras_do_programa = ks.iter().map(|k| k.t).filter(|t| *t != em(40).t && *t != em(90).t).collect();
        let (p40, p90) = (cx(verdade(40.0)), cx(verdade(90.0)));
        ks[40].v = p40;
        ks[90].v = p90;
        r.ancoras = ks;
        let op = |s: Option<f64>| OpRastreio::Simplificar { tolerancia: 0.04, suavizar_s: s };
        // sem suavizar e sem denso: nada (como antes)
        assert_eq!(aplicar(&r, None, &op(None), TB).unwrap(), (None, None));
        let (anc, novo) = aplicar(&r, None, &op(Some(0.5)), TB).unwrap();
        assert!(novo.is_none());
        let anc = anc.unwrap();
        assert!(anc.len() <= 10, "{}", anc.len());
        // as do perito ficam, iguais
        assert_eq!(anc.iter().find(|k| k.t == em(40).t).unwrap().v, p40);
        assert_eq!(anc.iter().find(|k| k.t == em(90).t).unwrap().v, p90);
        let r2 = rastreio(anc, OrigemRastreio::Assistido);
        for k in 10..=140 {
            let a = avaliar(&r2, None, em(k)).unwrap().caixa;
            let v = verdade(k as f64);
            let e = (a.x * 640.0 - v[0]).abs().max((a.y * 360.0 - v[1]).abs());
            assert!(e < 3.0, "quadro {k}: {e}");
        }
        // menos de 3 do programa: nada
        r.ancoras_do_programa.truncate(2);
        assert_eq!(aplicar(&r, None, &op(Some(0.5)), TB).unwrap(), (None, None));
    }

    #[test]
    fn simplificar_sem_denso_descarta_o_motor_que_cresceu_perto_da_correcao() {
        // rosto ~40×40 px; em 60..=80 o motor pegou o corpo (100×200); o perito corrigiu só o 70
        let rosto = |k: i64| Caixa { x: (100.0 + k as f64) / 640.0, y: 80.0 / 360.0, w: 40.0 / 640.0, h: 40.0 / 360.0 };
        let corpo = |k: i64| Caixa { x: (90.0 + k as f64) / 640.0, y: 70.0 / 360.0, w: 100.0 / 640.0, h: 200.0 / 360.0 };
        let mut r = rastreio((0..=140).map(|k| ch(k, if (60..=80).contains(&k) && k != 70 { corpo(k) } else { rosto(k) }, Interp::Linear)).collect(), OrigemRastreio::Assistido);
        r.ancoras_do_programa = r.ancoras.iter().map(|k| k.t).filter(|t| *t != em(70).t).collect();
        let anc = aplicar(&r, None, &OpRastreio::Simplificar { tolerancia: 0.04, suavizar_s: Some(0.5) }, TB).unwrap().0.unwrap();
        let r2 = rastreio(anc, OrigemRastreio::Assistido);
        for k in 0..=140 {
            let a = avaliar(&r2, None, em(k)).unwrap().caixa;
            assert!((a.w * 640.0 - 40.0).abs() < 4.0, "quadro {k}: largura {}", a.w * 640.0);
        }
    }

    #[test]
    fn simplificar_poe_ancoras_a_cada_2_s_e_buraco_de_4_s_fica_vazio() {
        // parada 10 s (300 quadros): sem o teto ficariam só as pontas, e o buraco de 4 s apagaria a pessoa
        let mut d = com_cabecalho(denso_teste(0, 300, |_| ESTADO_OK, |_| TIPO_MEDIDO));
        for j in 0..d.pts.len() {
            (d.x[j], d.y[j], d.w[j], d.h[j]) = (0.3, 0.3, 0.1, 0.2);
        }
        let r = rastreio(vec![], OrigemRastreio::Assistido);
        let anc = aplicar(&r, Some(&d), &OpRastreio::Simplificar { tolerancia: 0.04, suavizar_s: Some(0.5) }, TB).unwrap().0.unwrap();
        assert!(anc.windows(2).all(|w| w[1].t - w[0].t <= ESPACO_MAX_S + 1e-9), "{:?}", anc.iter().map(|k| k.t).collect::<Vec<_>>());
        let r2 = rastreio(anc, OrigemRastreio::Assistido);
        assert!((0..=300).all(|k| avaliar(&r2, None, em(k)).is_some()));
        // duas âncoras seguidas a mais de 4 s, sem nada no meio: vazio no meio (manual liga)
        let r3 = rastreio(vec![ch(0, Caixa { x: 0.1, y: 0.1, w: 0.1, h: 0.1 }, Interp::Linear), ch(200, Caixa { x: 0.6, y: 0.6, w: 0.1, h: 0.1 }, Interp::Linear)], OrigemRastreio::Assistido);
        assert!(avaliar(&r3, None, em(100)).is_none());
        let mut r4 = r3.clone();
        r4.origem = OrigemRastreio::Manual;
        assert!(avaliar(&r4, None, em(100)).is_some());
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
        let (anc, novo) = aplicar(&r, Some(&d), &OpRastreio::Simplificar { tolerancia: 0.04, suavizar_s: None }, TB).unwrap();
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
        assert!(aplicar(&r, Some(&d), &OpRastreio::Simplificar { tolerancia: 0.0, suavizar_s: None }, TB).is_err());
        assert!(aplicar(&r, Some(&d), &OpRastreio::Simplificar { tolerancia: f64::NAN, suavizar_s: None }, TB).is_err());
        assert_eq!(aplicar(&r, None, &OpRastreio::Simplificar { tolerancia: 0.04, suavizar_s: None }, TB).unwrap(), (None, None));
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
        let (anc, _) = aplicar(&r, Some(&d), &OpRastreio::Simplificar { tolerancia: 0.04, suavizar_s: None }, TB).unwrap();
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

    /// CFTV que cai de taxa (passo de 512 até o quadro 100, depois 1 024: mais que 1,5 × o mediano) com a caixa em reta:
    /// pela tabela do decodificador o denso é um trecho só e vira poucas âncoras; sem ela, cada quadro da parte lenta
    /// virava um trecho (uma âncora por quadro). Quadro que falta na tabela continua separando.
    #[test]
    fn simplificar_pela_tabela_quando_a_taxa_cai() {
        use crate::edicao::quadros::{Busca, Declarado, OrigemTabela};
        let pts = |k: i64| if k <= 100 { 512 * k } else { 512 * 100 + 1024 * (k - 100) };
        let mut d = com_cabecalho(denso_teste(0, 140, |_| ESTADO_OK, |_| TIPO_MEDIDO));
        for k in 0..=140i64 {
            let j = k as usize;
            d.pts[j] = pts(k);
            (d.x[j], d.y[j], d.w[j], d.h[j]) = ((100.0 + pts(k) as f64 / 512.0) / 640.0, 0.2, 0.1, 0.2);
        }
        let tab = |ks: &mut dyn Iterator<Item = i64>| {
            let v: Vec<i64> = ks.map(pts).collect();
            let n = v.len();
            TabelaQuadros {
                tb: TB,
                pts: v,
                chave: vec![true; n],
                passo_mediano: 512,
                passo: (512, 1024),
                vfr: true,
                origem: OrigemTabela::Decodificador,
                ffmpeg_sha256: String::new(),
                erros_decodificacao: 0,
                busca: Busca::SemBusca,
                declarado: Declarado { r_frame_rate: String::new(), avg_frame_rate: String::new(), nb_frames: None, duracao: None },
                pacotes: n as u64,
                lacunas: vec![],
            }
        };
        let r = rastreio(vec![], OrigemRastreio::Assistido);
        let op = OpRastreio::Simplificar { tolerancia: 0.04, suavizar_s: None };
        let sem = aplicar(&r, Some(&d), &op, TB).unwrap().0.unwrap();
        let t = tab(&mut (0..=140));
        let com = aplicar_com(&r, Some(&d), &op, TB, Some(&t)).unwrap().0.unwrap();
        // com a tabela: as pontas e as do teto de 2 s (6 s de reta)
        assert!(sem.len() > 35 && com.len() <= 4, "sem a tabela {}, com {}", sem.len(), com.len());
        let r2 = rastreio(com, OrigemRastreio::Assistido);
        for k in 0..=140i64 {
            let a = avaliar(&r2, None, Instante { t: pts(k) as f64 / 15360.0, pts: Some(pts(k)) }).unwrap();
            assert!((a.caixa.x * 640.0 - (100.0 + pts(k) as f64 / 512.0)).abs() <= 2.0 + 1e-6, "quadro {k}");
        }
        // quadro 120 sem linha no denso (perdido e reencontrado sem linha): na tabela ele existe, então separa
        let mut furado = d.clone();
        let fora = furado.pts.binary_search(&pts(120)).unwrap();
        for c in [&mut furado.x, &mut furado.y, &mut furado.w, &mut furado.h, &mut furado.conf] {
            c.remove(fora);
        }
        for c in [&mut furado.estado, &mut furado.tipo] {
            c.remove(fora);
        }
        furado.corrida.remove(fora);
        furado.pts.remove(fora);
        let ks: Vec<i64> = aplicar_com(&r, Some(&furado), &op, TB, Some(&t)).unwrap().0.unwrap().iter().map(|k| k.pts.unwrap()).collect();
        assert!(ks.contains(&pts(119)) && ks.contains(&pts(121)) && ks.len() <= 6, "{ks:?}");
        // tabela que não tem os pts do denso (outro espaço): cai na regra do passo mediano
        let outra = tab(&mut (0..=70));
        assert_eq!(aplicar_com(&r, Some(&d), &op, TB, Some(&outra)).unwrap().0.unwrap().len(), sem.len());
    }

    /// O rastreio como a tela grava o "Simplificar" de `d` (`ancorasDoSimplificar` e o registro das corridas).
    fn simplificado(r: &Rastreio, d: Option<&ArquivoDenso>, op: &OpRastreio) -> Rastreio {
        let anc = aplicar(r, d, op, TB).unwrap().0.unwrap();
        let reg = corridas_do_programa(r, d, op, Some(&anc), TB).unwrap();
        let perito: Vec<&Chave<Caixa>> = r.ancoras.iter().filter(|k| !r.ancoras_do_programa.contains(&k.t)).collect();
        let mut s = r.clone();
        s.ancoras_do_programa = anc.iter().filter(|k| !perito.iter().any(|p| p.t == k.t && p.v == k.v)).map(|k| k.t).collect();
        (s.ancoras, s.denso, s.corridas_do_programa) = (anc, None, reg);
        s
    }

    fn anda(k: i64) -> Option<f64> {
        Some(0.1 + k as f64 * 0.0005)
    }

    /// Defeito de 17/11 (caso PINHO, "sem registro do programa usado: 62"): o "Simplificar" trocava o denso por âncoras
    /// e o registro das corridas sumia com ele. Agora as corridas do denso simplificado (todas) ficam no rastreio; o
    /// "Simplificar" de novo sem denso não perde nenhuma; "Apagar daqui em diante" tira só a corrida sem âncora do
    /// programa a até 5 s da faixa dela; apagar tudo zera; faixa sem âncora não mexe no registro (None).
    #[test]
    fn simplificar_guarda_as_corridas_do_denso() {
        // A (VitTrack) nos quadros 0..=100 e, depois de um buraco, B (mancha) nos 400..=900
        let mut ra = res(Sentido::Frente, 0, 0, 100, &(0..=100).map(|k| (k, anda(k))).collect::<Vec<_>>());
        (ra.corrida.motor.motor, ra.corrida.motor.modelo.nome) = (MotorRastreio::Vittrack, "VitTrack".into());
        let rb = res(Sentido::Frente, 400, 400, 900, &(400..=900).map(|k| (k, anda(k))).collect::<Vec<_>>());
        let d = com_cabecalho(juntar(Some(&com_cabecalho(juntar(None, &ra).unwrap())), &rb).unwrap());
        assert_eq!(d.corridas.len(), 2);
        let perito = rastreio(vec![ch(0, Caixa { x: 0.1, y: 0.25, w: 0.1, h: 0.2 }, Interp::Linear)], OrigemRastreio::Assistido);
        let op = OpRastreio::Simplificar { tolerancia: 0.04, suavizar_s: Some(0.5) };
        let r = simplificado(&perito, Some(&d), &op);
        assert!(r.ancoras_do_programa.len() > 3);
        assert_eq!(r.corridas_do_programa, d.corridas, "o registro é o do denso, corrida a corrida");
        // de novo, sem denso: as duas ficam
        assert_eq!(simplificado(&r, None, &op).corridas_do_programa, d.corridas);
        let apagar = |de: i64| {
            let op = OpRastreio::ApagarFaixa { de_pts: 512 * de, ate_pts: None };
            corridas_do_programa(&r, None, &op, aplicar(&r, None, &op, TB).unwrap().0.as_deref(), TB)
        };
        // do 500: B ainda tem âncora do programa (400..499)
        assert_eq!(apagar(500), Some(d.corridas.clone()));
        // do 350: a âncora do programa mais perto de B é a do 100, 10 s antes dele: B sai, A fica
        assert_eq!(apagar(350), Some(vec![d.corridas[0].clone()]));
        assert_eq!(apagar(0), Some(vec![]));
        assert_eq!(apagar(2000), None, "nenhuma âncora sai: o registro fica como está");
    }

    /// O registro nunca perde programa (nunca IA a menos): a âncora de um "Simplificar" anterior que ficou por cima da
    /// faixa de uma corrida mais nova mantém a corrida dela (a regra "a última corrida que cobre o instante" a tirava);
    /// âncora do programa simplificada antes do registro (projeto de antes de 17/11) vira `null`, ao lado das corridas
    /// novas; denso sem corrida registrada (Fase 0) dá `null`.
    #[test]
    fn registro_do_simplificar_nunca_perde_programa() {
        let op = OpRastreio::Simplificar { tolerancia: 0.04, suavizar_s: Some(0.5) };
        let perito = rastreio(vec![ch(0, Caixa { x: 0.1, y: 0.25, w: 0.1, h: 0.2 }, Interp::Linear)], OrigemRastreio::Assistido);
        // primeira geração: A (VitTrack) de 0 a 300
        let mut ra = res(Sentido::Frente, 0, 0, 300, &(0..=300).map(|k| (k, anda(k))).collect::<Vec<_>>());
        (ra.corrida.motor.motor, ra.corrida.motor.modelo.nome) = (MotorRastreio::Vittrack, "VitTrack".into());
        let da = com_cabecalho(juntar(None, &ra).unwrap());
        let r1 = simplificado(&perito, Some(&da), &op);
        assert_eq!(r1.corridas_do_programa, da.corridas);
        // segunda geração: B (mancha) segue de novo de 0 a 300; as âncoras de A seguem valendo por cima do denso
        let rb = res(Sentido::Frente, 0, 0, 300, &(0..=300).map(|k| (k, anda(k).map(|x| x + 0.001))).collect::<Vec<_>>());
        let db = com_cabecalho(juntar(None, &rb).unwrap());
        let r2 = simplificado(&r1, Some(&db), &op);
        assert_eq!(r2.corridas_do_programa, [da.corridas.clone(), db.corridas.clone()].concat(), "A continua: as âncoras dela estão no vídeo");
        // projeto simplificado antes do registro: as âncoras do programa sem corrida viram `null`
        let mut velho = r1.clone();
        velho.corridas_do_programa.clear();
        assert_eq!(simplificado(&velho, Some(&db), &op).corridas_do_programa, [vec![serde_json::Value::Null], db.corridas.clone()].concat());
        assert_eq!(simplificado(&velho, None, &op).corridas_do_programa, vec![serde_json::Value::Null]);
        // denso da Fase 0 (sem `corridas`): `null`
        let mut fase0 = da.clone();
        (fase0.esquema, fase0.corridas) = (1, vec![]);
        assert_eq!(simplificado(&perito, Some(&fase0), &op).corridas_do_programa, vec![serde_json::Value::Null]);
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
