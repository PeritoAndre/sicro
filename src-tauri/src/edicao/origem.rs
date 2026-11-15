//! Intermediário de origem (§7.5): clipe com efeito de origem materializado em cache (`origem/<hash>.mp4` + `.json`),
//! com os pts da fonte, até 1080p, pela receita de proxy. Palco e rastreador leem dele; o render nunca (§7.6).
//! Faz também o `.trf` da estabilização quando o efeito ainda não tem (contratos-fase1 (b)). Dono: P1.4.
//!
//! A imagem sai da MESMA cadeia do motor (`realce::aplicar`), sem a ocultação (que é marcada no espaço original).
//! O arquivo começa em zero (o `<video>` só toca MP4 que começa em zero, D23): pts = pts da tabela do clipe menos o
//! do primeiro quadro, na mesma base; `desloc_s` leva a diferença.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::edicao::armazem;
use crate::edicao::executar::{self, TrfNovo, ERRO_CANCELADO, ERRO_LEITURA_DIVERGENTE};
use crate::edicao::ffmpeg::args;
use crate::edicao::ffmpeg::exec::{self, Comando};
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::fundo::{self, EstadoFundo, TipoFundo};
use crate::edicao::modelo::{BordaEstab, ClipePrincipal, ClipeTrecho, EfEstabilizar, Efeito, ProjetoEdicao, Racional, Tolerante};
use crate::edicao::motor::clipe::Montador;
use crate::edicao::motor::efeitos::{self, realce, Familia};
use crate::edicao::motor::grafo::{scale, Grafo};
use crate::edicao::motor::plano::{ArquivoTexto, Canvas, ImagemCamada, ItemCamada, Previa, ProvEstabilizacao, TipoCamada};
use crate::edicao::motor::FonteInfo;
use crate::edicao::previa::{PreviaMidia, TipoPrevia};
use crate::edicao::quadros::{OrigemTabela, TabelaQuadros};
use crate::edicao::tempo;
use crate::error::{Result, SicroError};

/// Situação do intermediário. `Pronto.trf`: `.trf` feito agora (já em `vidstab/` do projeto); o front grava
/// `trf_sha256` e `zoom_pct` no efeito por `alterar()` (um escritor só, D2).
// CONTRATO: o §7.5 diz `Pronto(PreviaMidia)`; `trf` leva à tela o `.trf` novo (contratos (c) 16).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum EstadoOrigem {
    Fazendo { tarefa: String, feito: f32 },
    Pronto { previa: PreviaMidia, trf: Option<TrfNovo> },
}

const PASTA: &str = "origem";
/// Versão da receita e do `.json`: mudar refaz os intermediários.
const VERSAO: u32 = 1;
/// Altura máxima do intermediário (§7.5).
const ALTURA_MAX: u32 = 1080;
const SAIDA: &str = "saida.mp4";
/// Fração do progresso que vai para a análise do movimento, quando há.
const PARTE_ANALISE: f32 = 0.3;

fn erro(codigo: &str, detalhe: impl std::fmt::Display) -> SicroError {
    SicroError::Validation(format!("{codigo}: {detalhe}"))
}

fn cancelado() -> SicroError {
    erro(ERRO_CANCELADO, "intermediário")
}

/// Efeitos de origem ligados, na ordem da lista, sem o nome (que não muda a imagem).
fn de_origem(clipe: &ClipeTrecho) -> Vec<Efeito> {
    efeitos::ligados(&clipe.base.efeitos, Familia::Origem).into_iter().cloned().collect()
}

/// Quadros `[a, b)` do clipe na tabela dele (a do mapa: `realce::tabela`).
fn faixa(clipe: &ClipeTrecho, tab: &TabelaQuadros) -> Result<(usize, usize)> {
    tempo::faixa_do_trecho(tab, clipe.inicio_s, clipe.inicio_pts, clipe.fim_s, clipe.fim_pts)
}

/// Chave do intermediário: SHA-256 de (mídia, faixa lida, efeitos de origem ligados com os parâmetros, ffmpeg).
pub fn chave(clipe: &ClipeTrecho, fonte: &FonteInfo, cap: &Capacidades) -> String {
    let tab = realce::tabela(&clipe.base.efeitos, &fonte.quadros);
    // a faixa pelos quadros (aparar por segundos que caem no mesmo quadro não refaz); sem tabela, pelos tempos
    let lida = match faixa(clipe, &tab) {
        Ok((a, b)) => json!([tab.pts[a], tab.pts.get(b)]),
        Err(_) => json!([clipe.inicio_s, clipe.inicio_pts, clipe.fim_s, clipe.fim_pts]),
    };
    let efeitos: Vec<Value> = de_origem(clipe)
        .iter()
        .map(|e| {
            let mut v = serde_json::to_value(e).unwrap_or(Value::Null);
            if let Some(o) = v.as_object_mut() {
                o.remove("nome");
            }
            v
        })
        .collect();
    let texto = json!({
        "versao": VERSAO, "midia": fonte.sha256, "faixa": lida, "efeitos": efeitos,
        "cor": [format!("{:?}", fonte.matriz), format!("{:?}", fonte.faixa), fonte.pix_fmt], "exibido": [fonte.exibido.0, fonte.exibido.1],
        "campos": format!("{:?}", fonte.ordem_campos),
        "ffmpeg": cap.sha256,
    });
    format!("{:x}", Sha256::digest(texto.to_string().as_bytes()))
}

/// O que fica ao lado do arquivo. `relativo` pode ser o de outra chave (o mesmo clipe antes de a tela gravar o `.trf`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Registro {
    versao: u32,
    relativo: String,
    desloc_s: f64,
    de_s: f64,
    ate_s: f64,
    largura: u32,
    altura: u32,
    tamanho: u64,
    quadros: u64,
    fonte_sha256: String,
    ffmpeg_sha256: String,
    ffmpeg_versao: String,
    argumentos: Vec<String>,
    trf: Option<TrfNovo>,
    feito_em: String,
}

fn caminho_do_registro(cache: &Path, chave: &str) -> PathBuf {
    cache.join(PASTA).join(format!("{chave}.json"))
}

/// O intermediário do cache, se o `.json` confere com a fonte e o arquivo tem o tamanho gravado.
fn pronto(cache: &Path, chave: &str, fonte: &FonteInfo) -> Option<EstadoOrigem> {
    let r: Registro = serde_json::from_slice(&std::fs::read(caminho_do_registro(cache, chave)).ok()?).ok()?;
    // `relativo` vem de arquivo do cache: só o nome que este módulo grava
    let nome = r.relativo.strip_prefix(&format!("{PASTA}/"))?;
    if nome.contains(['/', '\\']) || !nome.ends_with(".mp4") {
        return None;
    }
    let tamanho = std::fs::metadata(cache.join(PASTA).join(nome)).ok()?.len();
    if r.versao != VERSAO || r.fonte_sha256 != fonte.sha256 || tamanho != r.tamanho {
        return None;
    }
    let previa = PreviaMidia { relativo: r.relativo, tipo: TipoPrevia::Origem, desloc_s: r.desloc_s, de_s: r.de_s, ate_s: r.ate_s, largura: r.largura, altura: r.altura, pronta: true };
    Some(EstadoOrigem::Pronto { previa, trf: r.trf })
}

/// Pede o intermediário do clipe: o pronto do cache, ou o trabalho na fila de fundo (`TipoFundo::Origem`, chave
/// `origem:<chave>`). `ws` = caso; `cache` = cache da Edição do caso; `pasta_projeto` para o `.trf`.
pub fn pedir(ws: &Path, cache: &Path, pasta_projeto: &Path, cap: &Capacidades, clipe: &ClipeTrecho, fonte: &FonteInfo) -> Result<EstadoOrigem> {
    let _ = ws;
    if !realce::ativos(&clipe.base.efeitos, true) {
        return Err(erro("nao_suportado", format!("clipe {} sem efeito de origem", clipe.base.id)));
    }
    let k = chave(clipe, fonte, cap);
    if let Some(p) = pronto(cache, &k, fonte) {
        return Ok(p);
    }
    let chave_fundo = fundo::chave(TipoFundo::Origem, &k);
    let trabalho = {
        let (cache, pasta_projeto, cap, clipe, fonte) = (cache.to_path_buf(), pasta_projeto.to_path_buf(), Arc::new(cap.clone()), clipe.clone(), fonte.clone());
        Box::new(move |cancelar: &AtomicBool, progresso: &mut dyn FnMut(f32)| produzir(&cache, &pasta_projeto, &cap, &clipe, &fonte, cancelar, progresso).map(|_| ()))
    };
    let tarefa = fundo::pedir(TipoFundo::Origem, &chave_fundo, trabalho);
    match fundo::estado(&chave_fundo) {
        // falhou há pouco e a fila não refaz já: a tela recebe o motivo
        Some(t) if t.id == tarefa => match t.estado {
            EstadoFundo::Falhou { mensagem } => Err(SicroError::Validation(mensagem)),
            EstadoFundo::Pronta => pronto(cache, &k, fonte).ok_or_else(|| erro(ERRO_LEITURA_DIVERGENTE, "intermediário sumiu do cache")),
            _ => Ok(EstadoOrigem::Fazendo { tarefa, feito: t.feito }),
        },
        _ => Ok(EstadoOrigem::Fazendo { tarefa, feito: 0.0 }),
    }
}

// CONTRATO: acréscimo (fecho da Fase 1, `edicao_origem_estado`): a consulta que não põe nada na fila.
/// Situação do intermediário do clipe sem pedir: o pronto do cache, o que está na fila de fundo, ou None (nem um
/// nem outro; clipe sem efeito de origem também). O que falhou há pouco devolve o motivo.
pub fn estado(cache: &Path, cap: &Capacidades, clipe: &ClipeTrecho, fonte: &FonteInfo) -> Result<Option<EstadoOrigem>> {
    if !realce::ativos(&clipe.base.efeitos, true) {
        return Ok(None);
    }
    let k = chave(clipe, fonte, cap);
    if let Some(p) = pronto(cache, &k, fonte) {
        return Ok(Some(p));
    }
    match fundo::estado(&fundo::chave(TipoFundo::Origem, &k)) {
        Some(t) => match t.estado {
            EstadoFundo::NaFila | EstadoFundo::Rodando => Ok(Some(EstadoOrigem::Fazendo { tarefa: t.id, feito: t.feito })),
            EstadoFundo::Falhou { mensagem } => Err(SicroError::Validation(mensagem)),
            EstadoFundo::Pronta | EstadoFundo::Cancelada => Ok(None),
        },
        None => Ok(None),
    }
}

/// O trecho `clipe_id` da principal do `projeto` (JSON da tela). Outro tipo de clipe: `Ok(None)` com `tolerante`,
/// senão `nao_suportado`.
fn trecho_do_projeto<'a>(p: &'a ProjetoEdicao, clipe_id: &str, tolerante: bool) -> Result<Option<&'a ClipeTrecho>> {
    let outro = |o_que: &str| if tolerante { Ok(None) } else { Err(erro("nao_suportado", format!("intermediário de {o_que}"))) };
    for c in &p.principal {
        let Tolerante::Conhecido(c) = c else { continue };
        match c {
            ClipePrincipal::Trecho(t) if t.base.id == clipe_id => return Ok(Some(t)),
            ClipePrincipal::Congelar(c) if c.base.id == clipe_id => return outro("quadro congelado"),
            ClipePrincipal::Imagem(c) if c.base.id == clipe_id => return outro("imagem"),
            ClipePrincipal::Titulo(c) if c.base.id == clipe_id => return outro("título"),
            ClipePrincipal::Tela(c) if c.base.id == clipe_id => return outro("tela dividida"),
            _ => {}
        }
    }
    Err(erro("item_invalido", format!("clipe {clipe_id}")))
}

fn projeto_da_tela(projeto: &Value) -> Result<ProjetoEdicao> {
    let p = armazem::tipado(projeto)?;
    if !armazem::id_valido(&p.id) {
        return Err(erro("item_invalido", "id do projeto"));
    }
    Ok(p)
}

/// `edicao_origem_pedir`: acha o clipe `clipe_id` no `projeto` (JSON da tela), prepara a fonte e chama `pedir`.
/// Clipe sem efeito de origem: `nao_suportado`.
pub fn pedir_do_projeto(ws: &Path, cache: &Path, projeto: &Value, clipe_id: &str) -> Result<EstadoOrigem> {
    let p = projeto_da_tela(projeto)?;
    let clipe = trecho_do_projeto(&p, clipe_id, false)?.ok_or_else(|| erro("item_invalido", format!("clipe {clipe_id}")))?;
    let prep = executar::preparar(ws, &p, cache, false)?;
    let fonte = prep.fontes.get(&clipe.midia).ok_or_else(|| erro("midia_ausente", &clipe.midia))?;
    // o corte é por pts do decodificador: com a tabela de pacotes a tela pede de novo depois
    if fonte.quadros.origem != OrigemTabela::Decodificador {
        return Err(erro("tabela_pendente", &clipe.midia));
    }
    pedir(ws, cache, &prep.pasta_projeto, &prep.cap, clipe, fonte)
}

// CONTRATO: acréscimo (fecho da Fase 1). Corpo de `edicao_origem_estado`.
/// `estado` do clipe `clipe_id` do `projeto` da tela. Clipe que não é trecho, sem efeito de origem ou com a tabela do
/// decodificador pendente: None (não há intermediário a esperar).
pub fn estado_do_projeto(ws: &Path, cache: &Path, projeto: &Value, clipe_id: &str) -> Result<Option<EstadoOrigem>> {
    let p = projeto_da_tela(projeto)?;
    let Some(clipe) = trecho_do_projeto(&p, clipe_id, true)? else { return Ok(None) };
    if !realce::ativos(&clipe.base.efeitos, true) {
        return Ok(None);
    }
    let prep = executar::preparar(ws, &p, cache, false)?;
    let fonte = prep.fontes.get(&clipe.midia).ok_or_else(|| erro("midia_ausente", &clipe.midia))?;
    if fonte.quadros.origem != OrigemTabela::Decodificador {
        return Ok(None);
    }
    estado(cache, &prep.cap, clipe, fonte)
}

// ---------------------------------------------------------------- produção

/// Pasta de trabalho ao lado do destino (mesmo disco: o `rename` é atômico), apagada no fim.
struct Temporaria(PathBuf);

impl Temporaria {
    fn nova(pasta: &Path, chave: &str) -> Result<Self> {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = pasta.join(format!("{chave}.tmp-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p)?;
        Ok(Temporaria(p))
    }
}

impl Drop for Temporaria {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// O que `realce::aplicar` pede: só o grafo. A análise do movimento roda aqui (`estabilizacao`), não como prévia.
struct Montagem {
    g: Grafo,
    canvas: Canvas,
}

impl Montador for Montagem {
    fn grafo(&mut self) -> &mut Grafo {
        &mut self.g
    }
    fn camada(&mut self, _: char, _: TipoCamada, _: [u32; 4], _: u32, _: Vec<ItemCamada>, _: Vec<ImagemCamada>) -> String {
        String::new()
    }
    fn previa(&mut self, _: Previa) {}
    fn auxiliar(&mut self, _: ArquivoTexto) {}
    fn canvas(&self) -> &Canvas {
        &self.canvas
    }
    fn rotulo(&self) -> &str {
        "o"
    }
    fn exportacao_final(&self) -> bool {
        false
    }
}

fn comando(programa: &Path, pasta: &Path, nome: &str, args: Vec<String>) -> Comando {
    Comando { nome: nome.into(), programa: programa.to_path_buf(), args, pasta: pasta.to_path_buf(), ambiente: Vec::new(), sem_ambiente: Vec::new(), stderr: pasta.join(format!("{nome}.stderr")) }
}

/// Roda e devolve quantos quadros saíram (o último `frame=` do `-progress`).
fn rodar(cmd: &Comando, cancelar: &AtomicBool, progresso: &mut dyn FnMut(f32), dur_s: f64) -> Result<u64> {
    let mut quadros = 0;
    let t = exec::rodar_com_progresso(cmd, cancelar, &mut |p| {
        quadros = quadros.max(p.quadro);
        if let Some(us) = p.tempo_us {
            progresso((us as f64 / 1e6 / dur_s.max(1e-3)).clamp(0.0, 1.0) as f32);
        }
    })?;
    if t.cancelado || cancelar.load(Ordering::SeqCst) {
        return Err(cancelado());
    }
    exec::exigir_sucesso(cmd, &t)?;
    Ok(quadros)
}

/// Só para os testes: soma `desvio_s` ao `-ss` (a busca que abre adiante do previsto). Devolve se há busca.
fn desviar(a: &mut [String], desvio_s: f64) -> bool {
    let Some(i) = a.iter().position(|x| x == "-ss") else { return false };
    if desvio_s != 0.0 {
        if let Some(v) = a.get(i + 1).and_then(|v| v.parse::<f64>().ok()) {
            a[i + 1] = format!("{:.6}", v + desvio_s);
        }
    }
    true
}

/// Tamanho do intermediário: o exibido, reduzido até `ALTURA_MAX`, lados pares.
fn tamanho(exibido: (u32, u32)) -> (u32, u32) {
    let (w, h) = exibido;
    if h <= ALTURA_MAX {
        return ((w / 2 * 2).max(2), (h / 2 * 2).max(2));
    }
    (((w as f64 * ALTURA_MAX as f64 / h as f64 / 2.0).round() as u32 * 2).max(2), ALTURA_MAX)
}

/// Põe o `.trf` do efeito na pasta de trabalho com o nome que o grafo cita: o guardado no projeto, ou o analisado
/// agora (que vai para `vidstab/<sha256>.trf` do projeto). Devolve o SHA-256 do novo.
fn estabilizacao(
    cap: &Capacidades,
    trabalho: &Path,
    pasta_projeto: &Path,
    clipe: &ClipeTrecho,
    e: &EfEstabilizar,
    fonte: &FonteInfo,
    desvio_s: f64,
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(f32),
) -> Result<Option<String>> {
    let nome = realce::nome_do_trf(&clipe.base.efeitos, e, fonte);
    if e.trf_sha256.is_some() {
        let guardado = ProvEstabilizacao { efeito: e.base.id.clone(), trf_sha256: e.trf_sha256.clone(), analise: e.analise, zoom_pct: e.zoom_pct };
        realce::trazer_trf(&[guardado], pasta_projeto, trabalho)?;
        return Ok(None);
    }
    // a análise não passa pela conferência do bloco: confere aqui os quadros que ela viu e refaz sem busca
    let esperado = realce::quadros_da_analise(&clipe.base.efeitos, e, &fonte.quadros);
    let mut analisar = |com_busca: bool| -> Result<(bool, bool)> {
        let mut a = realce::deteccao(&clipe.base.efeitos, e, fonte, &nome, com_busca)?;
        let buscou = desviar(&mut a, desvio_s);
        let mut cmd = comando(&cap.binario, trabalho, if com_busca { "analise" } else { "analise-sem-busca" }, a);
        // sem isso 3 execuções dão 3 `.trf` (§5.3)
        cmd.ambiente.push(("OMP_NUM_THREADS".into(), "1".into()));
        let quadros = rodar(&cmd, cancelar, progresso, (e.analise.ate_s - e.analise.de_s).max(1e-3))?;
        Ok((buscou, quadros == esperado))
    };
    let (buscou, certa) = analisar(true)?;
    if !certa && !(buscou && analisar(false)?.1) {
        return Err(erro(ERRO_LEITURA_DIVERGENTE, "análise do movimento"));
    }
    Ok(Some(executar::guardar_trf(pasta_projeto, &std::fs::read(trabalho.join(&nome))?)?))
}

/// Último `Final zoom: <x>` do registro do `vidstabtransform`.
fn zoom_final(stderr: &Path) -> Option<f64> {
    let log = String::from_utf8_lossy(&std::fs::read(stderr).ok()?).into_owned();
    log.lines().filter_map(|l| l.split("Final zoom:").nth(1)).filter_map(|v| v.trim().parse::<f64>().ok()).next_back()
}

/// Base de tempo e pts de cada pacote de vídeo do arquivo, em ordem crescente. Pacote sem duração é erro: o MP4
/// corta pela lista de edição o último quadro que não a tem.
fn pacotes(cap: &Capacidades, video: &Path, pasta: &Path, cancelar: &AtomicBool) -> Result<(Racional, Vec<i64>)> {
    let a = ["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=time_base:packet=pts,duration", "-of", "csv"].iter().map(|s| s.to_string()).chain([video.to_string_lossy().into_owned()]).collect();
    let cmd = comando(&cap.ffprobe, pasta, "pacotes", a);
    let (t, out) = exec::rodar_com_saida(&cmd, cancelar)?;
    exec::exigir_sucesso(&cmd, &t)?;
    let (mut tb, mut pts) = (None, Vec::new());
    for l in String::from_utf8_lossy(&out).lines() {
        let mut c = l.trim().trim_end_matches(',').split(',');
        match c.next() {
            Some("packet") => {
                let (p, d) = (c.next().and_then(|v| v.parse::<i64>().ok()), c.next().and_then(|v| v.parse::<i64>().ok()));
                if !d.is_some_and(|d| d > 0) {
                    return Err(erro(ERRO_LEITURA_DIVERGENTE, "quadro do intermediário sem duração"));
                }
                pts.extend(p);
            }
            Some("stream") if tb.is_none() => {
                tb = c.next().and_then(|t| t.split_once('/')).and_then(|(n, d)| Some(Racional { num: n.parse().ok()?, den: d.parse().ok()? })).filter(|r| r.num > 0 && r.den > 0)
            }
            _ => {}
        }
    }
    pts.sort_unstable();
    Ok((tb.ok_or_else(|| erro(ERRO_LEITURA_DIVERGENTE, "intermediário sem base de tempo"))?, pts))
}

/// Faz o intermediário do clipe (trabalho da fila de fundo) e devolve a prévia e o `.trf` novo, se houve.
pub(crate) fn produzir(
    cache: &Path,
    pasta_projeto: &Path,
    cap: &Capacidades,
    clipe: &ClipeTrecho,
    fonte: &FonteInfo,
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(f32),
) -> Result<(PreviaMidia, Option<TrfNovo>)> {
    produzir_com(cache, pasta_projeto, cap, clipe, fonte, 0.0, cancelar, progresso)
}

/// `produzir`; `desvio_s` ≠ 0 só nos testes (ver `desviar`).
#[allow(clippy::too_many_arguments)]
fn produzir_com(
    cache: &Path,
    pasta_projeto: &Path,
    cap: &Capacidades,
    clipe: &ClipeTrecho,
    fonte: &FonteInfo,
    desvio_s: f64,
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(f32),
) -> Result<(PreviaMidia, Option<TrfNovo>)> {
    let k = chave(clipe, fonte, cap);
    if let Some(EstadoOrigem::Pronto { previa, trf }) = pronto(cache, &k, fonte) {
        progresso(1.0);
        return Ok((previa, trf));
    }
    let efeitos_do_clipe = &clipe.base.efeitos;
    let tab = &fonte.quadros;
    let tab_clipe = realce::tabela(efeitos_do_clipe, tab);
    let (fa, fb) = faixa(clipe, &tab_clipe)?;
    let leitura = realce::leitura(efeitos_do_clipe, tab, &tab_clipe, (fa, fb), (fa, fb))?;
    let (ja, jb) = leitura.ler;
    if ja >= jb || jb > tab.pts.len() {
        return Err(erro("intervalo", &clipe.base.id));
    }
    let pasta = cache.join(PASTA);
    std::fs::create_dir_all(&pasta)?;
    let tmp = Temporaria::nova(&pasta, &k)?;

    // análise do movimento (ou o `.trf` guardado) antes do grafo que o lê
    let est = de_origem(clipe).into_iter().find_map(|e| match e {
        Efeito::Estabilizar(e) => Some(e),
        _ => None,
    });
    let mut parte = 0.0;
    let mut trf_sha = None;
    if let Some(e) = &est {
        if e.trf_sha256.is_none() {
            parte = PARTE_ANALISE;
        }
        trf_sha = estabilizacao(cap, &tmp.0, pasta_projeto, clipe, e, fonte, desvio_s, cancelar, &mut |f| progresso(f * PARTE_ANALISE))?;
    }

    let (w, h) = tamanho(fonte.exibido);
    let cor = realce::cor_da_fonte(efeitos_do_clipe, fonte).ok_or_else(|| erro("pixel_nao_suportado", &clipe.midia))?;
    let corte = match tab.pts.get(jb) {
        Some(fim) => format!("trim=start_pts={}:end_pts={fim}", tab.pts[ja]),
        None => format!("trim=start_pts={}", tab.pts[ja]),
    };
    let mut m = Montagem { g: Grafo::novo(), canvas: Canvas { w, h, fps: tab.taxa_nominal(), cor_fundo: "#000000".into() } };
    let na_cor = m.g.rotulo("f");
    m.g.cadeia(&["0:v:0"], &[corte, cor], &[&na_cor]);
    let realcado = realce::aplicar(&mut m, &na_cor, efeitos_do_clipe, fonte, &leitura)?;
    // começa em zero, na base da tabela do clipe
    let p0 = tab_clipe.pts[fa];
    let menos = if p0 < 0 { format!("+{}", p0.unsigned_abs()) } else { format!("-{p0}") };
    let mut fim = vec![format!("settb={}/{}", tab.tb.num, tab.tb.den), format!("setpts=PTS{menos}")];
    if (w, h) != fonte.exibido {
        fim.push(scale(w, h, "bicubic"));
    }
    fim.extend(["setsar=1".to_string(), "format=yuv420p".to_string()]);
    m.g.cadeia(&[&realcado], &fim, &["v"]);
    let (de_s, ate_s) = (tab_clipe.tempo(fa), if fb < tab_clipe.pts.len() { tab_clipe.tempo(fb) } else { tab_clipe.fim_s() });
    if fonte.audio.is_some() {
        // o som do mesmo intervalo, com o zero no primeiro quadro
        m.g.cadeia(&["0:a:0"], &[format!("atrim=start={de_s:.6}:end={ate_s:.6}"), format!("asetpts=PTS-{de_s:.6}/TB"), "aresample=48000".to_string()], &["a"]);
    }
    args::gravar_texto(&tmp.0, "g.txt", &m.g.texto())?;

    let mut depois = args::grafo_em_arquivo(cap, "g.txt");
    let a = &mut depois;
    // receita de CPU do proxy (§7.6), com o pts imposto pelo grafo passado adiante
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    a.extend(s(&["-map", "[v]", "-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-profile:v", "high", "-g", "12", "-bf", "0", "-sc_threshold", "0", "-fps_mode", "passthrough", "-enc_time_base"]));
    a.push(format!("{}:{}", tab.tb.num, tab.tb.den));
    a.extend(["-video_track_timescale".to_string(), tab.tb.den.to_string()]);
    // duração de cada quadro pelo pts do seguinte; a do último, pela tabela: o 7.0.2 a deixa em zero depois do
    // `setpts` e a lista de edição do MP4 corta o último quadro
    let ultimo = match tab_clipe.pts.get(fb) {
        Some(fim) => fim - tab_clipe.pts[fb - 1],
        None => tab_clipe.passo_mediano,
    };
    a.extend(["-bsf:v".to_string(), format!("setts=duration=if(eq(NEXT_PTS\\,NOPTS)\\,{}\\,NEXT_PTS-PTS)", ultimo.max(1))]);
    if fonte.audio.is_some() {
        a.extend(s(&["-map", "[a]", "-c:a", "aac", "-b:a", "128k", "-ac", "2"]));
    }
    a.extend(s(&["-movflags", "+faststart", SAIDA]));
    let saida = tmp.0.join(SAIDA);
    let busca = tab.busca_em(fonte.inicio_s, ja).map(|alvo| {
        let t = if jb < tab.pts.len() { tab.tempo(jb) } else { tab.fim_s() } - fonte.inicio_s;
        ["-ss".to_string(), format!("{alvo:.6}"), "-t".to_string(), format!("{:.6}", t - alvo + 1.0)]
    });
    let mut tentar = |com_busca: bool| -> Result<(Comando, Vec<i64>)> {
        let mut a = args::globais();
        a.extend(busca.iter().filter(|_| com_busca).flatten().cloned());
        desviar(&mut a, desvio_s);
        a.extend(["-i".to_string(), fonte.caminho.to_string_lossy().into_owned()]);
        a.extend(depois.iter().cloned());
        let cmd = comando(&cap.binario, &tmp.0, if com_busca { "origem" } else { "origem-sem-busca" }, a);
        rodar(&cmd, cancelar, &mut |f| progresso(parte + f * (1.0 - parte) * 0.97), ate_s - de_s)?;
        // os pts do arquivo são os da tabela do clipe (menos o do primeiro), um por quadro
        let (tb, pts) = pacotes(cap, &saida, &tmp.0, cancelar)?;
        let iguais = pts.len() == fb - fa
            && pts.iter().zip(&tab_clipe.pts[fa..fb]).all(|(&p, &q)| p as i128 * tb.num as i128 * tab.tb.den as i128 == (q - p0) as i128 * tab.tb.num as i128 * tb.den as i128);
        if !iguais {
            return Err(erro(ERRO_LEITURA_DIVERGENTE, format!("intermediário com {} quadros para {} da tabela, ou pts diferentes", pts.len(), fb - fa)));
        }
        Ok((cmd, pts))
    };
    // leitura que não confere com a busca é refeita sem ela, como o bloco do render (§5.3)
    let (cmd, pts) = match tentar(true) {
        Err(e) if busca.is_some() && !cancelar.load(Ordering::SeqCst) => {
            tracing::warn!("edição: intermediário de {} não conferiu ({e}); refeito sem busca", clipe.base.id);
            tentar(false)?
        }
        r => r?,
    };

    // o que a tela tem de gravar no efeito: o `.trf` analisado agora e o zoom que o `ampliar` calculou
    let trf = match &est {
        Some(e) if trf_sha.is_some() || realce::zoom_a_calcular(e) => {
            let zoom_pct = match e.borda {
                BordaEstab::Ampliar => zoom_final(&cmd.stderr).ok_or_else(|| erro(ERRO_LEITURA_DIVERGENTE, "zoom da estabilização"))?,
                BordaEstab::Preta => 0.0,
            };
            trf_sha.or_else(|| e.trf_sha256.clone()).map(|sha256| TrfNovo { efeito: e.base.id.clone(), sha256, zoom_pct })
        }
        _ => None,
    };
    let relativo = format!("{PASTA}/{k}.mp4");
    let registro = Registro {
        versao: VERSAO,
        relativo: relativo.clone(),
        desloc_s: de_s,
        de_s,
        ate_s,
        largura: w,
        altura: h,
        tamanho: std::fs::metadata(&saida)?.len(),
        quadros: pts.len() as u64,
        fonte_sha256: fonte.sha256.clone(),
        ffmpeg_sha256: cap.sha256.clone(),
        ffmpeg_versao: cap.versao.clone(),
        argumentos: cmd.args.clone(),
        trf: trf.clone(),
        feito_em: chrono::Utc::now().to_rfc3339(),
    };
    if cancelar.load(Ordering::SeqCst) {
        return Err(cancelado());
    }
    std::fs::OpenOptions::new().write(true).open(&saida)?.sync_all()?;
    std::fs::rename(&saida, pasta.join(format!("{k}.mp4")))?;
    gravar_registro(&tmp.0, &caminho_do_registro(cache, &k), &registro)?;
    // depois que a tela gravar o `.trf` e o zoom no efeito a chave muda e a imagem é a mesma: a chave nova aponta para cá
    if let Some(t) = &trf {
        let mut depois = clipe.clone();
        for e in &mut depois.base.efeitos {
            if let Tolerante::Conhecido(Efeito::Estabilizar(x)) = e {
                if x.base.id == t.efeito {
                    (x.trf_sha256, x.zoom_pct) = (Some(t.sha256.clone()), t.zoom_pct);
                }
            }
        }
        let k2 = chave(&depois, fonte, cap);
        gravar_registro(&tmp.0, &caminho_do_registro(cache, &k2), &Registro { trf: None, ..registro.clone() })?;
    }
    progresso(1.0);
    let previa = PreviaMidia { relativo, tipo: TipoPrevia::Origem, desloc_s: de_s, de_s, ate_s, largura: w, altura: h, pronta: true };
    Ok((previa, trf))
}

/// Grava o `.json` inteiro ou nada (temporário na pasta de trabalho, mesmo disco, e `rename`).
fn gravar_registro(trabalho: &Path, destino: &Path, r: &Registro) -> Result<()> {
    let tmp = trabalho.join("registro.json");
    std::fs::write(&tmp, serde_json::to_vec_pretty(r)?)?;
    std::fs::rename(&tmp, destino)?;
    Ok(())
}

#[cfg(test)]
mod testes {
    use std::time::{Duration, Instant};

    use serde_json::json;

    use super::*;
    use crate::edicao::motor::efeitos::realce::testes::{
        ajustes, caixa_da_cor, croma_do_outro_campo, desentrelacar, estabilizar, fonte_bff, fonte_de, gerar, magenta, media, plano_tremido, projeto_bff, renderizar, rodar_previas, sha256_do_arquivo,
        tremido, verde, TREMIDO,
    };
    use crate::edicao::quadros::apoio;
    use crate::edicao::testes::aceite::{alvo_final, ancora, efeito, midia, projeto, rastreio, trecho};
    use crate::edicao::testes::fontes::{self, Fonte};
    use crate::edicao::testes::medir;

    /// Trecho `[j_in, j_out)` da tabela da fonte com os efeitos dados.
    fn clipe(tab: &TabelaQuadros, j_in: usize, j_out: usize, efeitos: Value) -> ClipeTrecho {
        serde_json::from_value(trecho("c1", "m1", tab, j_in, j_out, 1.0, efeitos)).expect("trecho")
    }

    fn fazer(cap: &Capacidades, raiz: &Path, c: &ClipeTrecho, fonte: &FonteInfo) -> (PreviaMidia, Option<TrfNovo>, PathBuf) {
        fazer_com(cap, raiz, c, fonte, 0.0)
    }

    /// `fazer` com a busca desviada em `desvio_s` segundos.
    fn fazer_com(cap: &Capacidades, raiz: &Path, c: &ClipeTrecho, fonte: &FonteInfo, desvio_s: f64) -> (PreviaMidia, Option<TrfNovo>, PathBuf) {
        let (cache, projeto) = (raiz.join("cache"), raiz.join("projeto"));
        let (p, trf) = produzir_com(&cache, &projeto, cap, c, fonte, desvio_s, &AtomicBool::new(false), &mut |_| {}).unwrap_or_else(|e| panic!("intermediário ({}): {e}", cap.versao));
        let arquivo = cache.join(&p.relativo);
        (p, trf, arquivo)
    }

    /// `estado` não pede: sem nada na fila nem no cache, None; com o trabalho na fila ou rodando, "fazendo" com a
    /// tarefa e o progresso; o que falhou devolve o motivo; clipe sem efeito de origem, None sem olhar a fila.
    #[test]
    fn estado_acompanha_a_fila_sem_pedir() {
        use std::sync::atomic::Ordering;
        let Some(cap) = fontes::cap() else { return };
        let tab = apoio::tabela((1, 15360), apoio::pts_regulares(120, (1, 30), (1, 15360), 0));
        let mut fonte = apoio::fonte(tab.clone(), (640, 360));
        // chave só deste teste (a fila de fundo é do processo)
        fonte.sha256 = "e57ad0".repeat(11)[..64].to_string();
        let cache = fontes::pasta_temporaria("origem-estado");
        let c = clipe(&tab, 10, 70, json!([ajustes(0.1, 1.0)]));
        let chave_fundo = fundo::chave(TipoFundo::Origem, &chave(&c, &fonte, &cap));
        assert_eq!(estado(&cache, &cap, &c, &fonte).unwrap(), None);
        assert!(fundo::estado(&chave_fundo).is_none(), "perguntar não põe na fila");
        // um trabalho com essa chave (o que `pedir` enfileira), preso até o teste soltar, e que falha
        let soltar = Arc::new(AtomicBool::new(false));
        let trabalho = {
            let soltar = soltar.clone();
            Box::new(move |_: &AtomicBool, progresso: &mut dyn FnMut(f32)| {
                progresso(0.25);
                let t0 = Instant::now();
                while !soltar.load(Ordering::SeqCst) && t0.elapsed() < Duration::from_secs(120) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(erro("fonte_danificada", "quadro 12"))
            })
        };
        let tarefa = fundo::pedir(TipoFundo::Origem, &chave_fundo, trabalho);
        match estado(&cache, &cap, &c, &fonte).unwrap() {
            Some(EstadoOrigem::Fazendo { tarefa: t, feito }) => assert!(t == tarefa && (0.0..=1.0).contains(&feito), "{t} {feito}"),
            outro => panic!("na fila ou rodando: {outro:?}"),
        }
        soltar.store(true, Ordering::SeqCst);
        let t0 = Instant::now();
        while fundo::estado(&chave_fundo).is_some_and(|t| matches!(t.estado, EstadoFundo::NaFila | EstadoFundo::Rodando)) {
            assert!(t0.elapsed() < Duration::from_secs(180), "tarefa de fundo presa");
            std::thread::sleep(Duration::from_millis(5));
        }
        let e = estado(&cache, &cap, &c, &fonte).unwrap_err().to_string();
        assert!(e.contains("fonte_danificada") && e.contains("quadro 12"), "{e}");
        // sem efeito de origem não há o que esperar, nem com a fila dizendo outra coisa
        assert_eq!(estado(&cache, &cap, &clipe(&tab, 10, 70, json!([])), &fonte).unwrap(), None);
        assert!(!cache.join("origem").exists(), "nada foi feito");
    }

    /// Busca que abre adiante do previsto (injetada: 1,5 s depois, o decodificador entrega do quadro-chave seguinte,
    /// com código 0): a análise do movimento e o intermediário são refeitos sem busca e saem como sem o defeito.
    /// Com leitura alargada (desentrelace) a falta é de UM quadro e a contagem fecharia com o quadro vizinho.
    #[test]
    fn busca_errada_e_refeita_sem_busca() {
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let tab = &m.info.quadros;
        // quadro-chave a cada 30; o trecho começa um quadro depois do 390
        assert!(tab.chave[360] && tab.chave[390] && tab.chave[420] && !tab.chave[389] && !tab.chave[391]);
        let (j_in, j_out) = (391usize, 421usize);
        let registro = |raiz: &Path, c: &ClipeTrecho| -> Registro { serde_json::from_slice(&std::fs::read(caminho_do_registro(&raiz.join("cache"), &chave(c, &m.info, &cap))).unwrap()).unwrap() };
        for (nome, efeitos, numerado) in [
            ("ajustes", json!([ajustes(0.05, 1.1)]), true),
            ("desentrelace", json!([desentrelacar("quadro")]), true),
            ("estabilizar", json!([estabilizar(tab, 385, 430, "preta", 0.0, None)]), false),
        ] {
            let c = clipe(tab, j_in, j_out, efeitos);
            let (certa, errada) = (fontes::pasta_temporaria("origem-busca-certa"), fontes::pasta_temporaria("origem-busca-errada"));
            let (p, trf, arquivo) = fazer(&cap, &certa, &c, &m.info);
            assert!(registro(&certa, &c).argumentos.iter().any(|a| a == "-ss"), "{nome}: o caso tem de usar busca");
            let (p2, trf2, arquivo2) = fazer_com(&cap, &errada, &c, &m.info, 1.5);
            assert!(!registro(&errada, &c).argumentos.iter().any(|a| a == "-ss"), "{nome} ({}): não foi refeito sem busca", cap.versao);
            assert_eq!((&p2, &trf2), (&p, &trf), "{nome} ({}): prévia e `.trf`", cap.versao);
            assert_eq!(trf.is_some(), nome == "estabilizar");
            conferir_pts(&cap, &p2, &arquivo2, &c, &m.info, nome);
            if numerado {
                assert_eq!(medir::quadros_de_origem(&cap, &arquivo2).unwrap(), m.numeros[j_in..j_out], "{nome} ({}): quadro de cada posição", cap.versao);
            }
            let (a, b) = (medir::quadros(&cap, &arquivo).unwrap(), medir::quadros(&cap, &arquivo2).unwrap());
            let n = a.iter().zip(&b).filter(|(x, y)| medir::pixels_diferentes(x, y, 0) > 0).count();
            assert_eq!((a.len(), b.len(), n), (j_out - j_in, j_out - j_in, 0), "{nome} ({}): quadros diferentes do intermediário sem o defeito", cap.versao);
        }
    }

    /// Confere o arquivo com a tabela do clipe: um quadro por entrada, pts = o da tabela menos o do primeiro, na base
    /// dela; `desloc_s` e a faixa em tempo de fonte.
    fn conferir_pts(cap: &Capacidades, p: &PreviaMidia, arquivo: &Path, c: &ClipeTrecho, fonte: &FonteInfo, rotulo: &str) -> (usize, usize) {
        let tab = realce::tabela(&c.base.efeitos, &fonte.quadros);
        let (fa, fb) = faixa(c, &tab).unwrap();
        let (tb, pts) = pacotes(cap, arquivo, arquivo.parent().unwrap(), &AtomicBool::new(false)).unwrap();
        assert_eq!(tb, tab.tb, "{rotulo}: base de tempo");
        let esperado: Vec<i64> = tab.pts[fa..fb].iter().map(|q| q - tab.pts[fa]).collect();
        assert_eq!(pts, esperado, "{rotulo}: pts do arquivo × tabela do clipe");
        // o decodificador entrega os mesmos (nenhum quadro escondido nem reordenado)
        assert_eq!(medir::pts(cap, arquivo).unwrap(), esperado, "{rotulo}: pts decodificados");
        let fim = if fb < tab.pts.len() { tab.tempo(fb) } else { tab.fim_s() };
        assert_eq!((p.tipo, p.pronta, p.desloc_s, p.de_s, p.ate_s), (TipoPrevia::Origem, true, tab.tempo(fa), tab.tempo(fa), fim), "{rotulo}");
        assert!(p.relativo.starts_with("origem/") && p.relativo.ends_with(".mp4"), "{rotulo}: {}", p.relativo);
        (fa, fb)
    }

    fn trilhas(cap: &Capacidades, arquivo: &Path, tipo: &str) -> Vec<String> {
        let a = ["-v", "error", "-select_streams", tipo, "-show_entries", "stream=codec_name,start_time,duration", "-of", "csv=p=0"].iter().map(|s| s.to_string()).chain([arquivo.to_string_lossy().into_owned()]).collect();
        let cmd = comando(&cap.ffprobe, arquivo.parent().unwrap(), "trilhas", a);
        let (_, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).unwrap();
        String::from_utf8_lossy(&out).lines().map(String::from).filter(|l| !l.trim().is_empty()).collect()
    }

    /// Intermediário com os pts da fonte: MP4, MKV com oscilação, entrelaçado por campo e estabilizado; o quadro de
    /// cada posição é o da tabela.
    #[test]
    fn pts_iguais_aos_da_fonte() {
        let Some(cap) = fontes::cap() else { return };
        let raiz = fontes::pasta_temporaria("origem-pts");
        // (fonte, faixa, efeitos, identificável pelo número desenhado)
        let casos: [(Fonte, usize, usize, Value, bool); 5] = [
            (Fonte::Mkv30, 40, 100, json!([ajustes(0.05, 1.1)]), true),
            (Fonte::Mp4, 0, 30, json!([ajustes(-0.05, 1.0), efeito("nitidez", "n1", json!({ "quantidade": 0.8 }))]), true),
            (Fonte::Oscilacao, 35, 95, json!([media(3)]), false),
            (Fonte::Entrelacado, 20, 40, json!([desentrelacar("campo")]), true),
            (Fonte::Entrelacado, 150, 180, json!([desentrelacar("quadro")]), true),
        ];
        for (f, j_in, j_out, efeitos, identificavel) in casos {
            let m = apoio::medida(&cap, f);
            let rotulo = format!("{f:?} {j_in}..{j_out} ({})", cap.versao);
            let c = clipe(&m.info.quadros, j_in, j_out, efeitos);
            let (p, trf, arquivo) = fazer(&cap, &raiz, &c, &m.info);
            assert!(trf.is_none(), "{rotulo}");
            let (fa, fb) = conferir_pts(&cap, &p, &arquivo, &c, &m.info, &rotulo);
            let campos = fb - fa != j_out - j_in;
            assert_eq!(fb - fa, if campos { 2 * (j_out - j_in) } else { j_out - j_in }, "{rotulo}");
            assert_eq!((p.largura, p.altura), m.info.exibido, "{rotulo}");
            if identificavel {
                let numeros = medir::quadros_de_origem(&cap, &arquivo).unwrap();
                let esperado: Vec<u32> = (fa..fb).map(|k| m.numeros[if campos { k / 2 } else { k }]).collect();
                assert_eq!(numeros, esperado, "{rotulo}: quadro de cada posição");
            }
            // som do mesmo intervalo, começando em zero
            let som = trilhas(&cap, &arquivo, "a:0");
            assert_eq!(som.len(), usize::from(m.info.audio.is_some()), "{rotulo}: trilha de som {som:?}");
            if let Some(l) = som.first() {
                let c: Vec<&str> = l.split(',').collect();
                let (inicio, dur): (f64, f64) = (c[1].parse().unwrap(), c[2].parse().unwrap());
                assert!(c[0] == "aac" && (0.0..0.06).contains(&inicio) && (dur - (p.ate_s - p.de_s)).abs() < 0.08, "{rotulo}: som {l} para {} s", p.ate_s - p.de_s);
            }
            // de novo: o do cache, sem refazer
            let antes = std::fs::metadata(&arquivo).unwrap().modified().unwrap();
            let (p2, _, arquivo2) = fazer(&cap, &raiz, &c, &m.info);
            assert_eq!((p2, arquivo2.clone()), (p, arquivo.clone()), "{rotulo}");
            assert_eq!(std::fs::metadata(&arquivo2).unwrap().modified().unwrap(), antes, "{rotulo}: refez o que estava pronto");
            eprintln!("intermediário {rotulo}: {} quadros com os pts da tabela", fb - fa);
        }
        // nenhuma pasta de trabalho sobra no cache
        let sobras: Vec<String> = std::fs::read_dir(raiz.join("cache").join(PASTA)).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.contains(".tmp")).collect();
        assert!(sobras.is_empty(), "{sobras:?}");
    }

    /// A matriz de fontes inteira (início ≠ 0, 29,97 e 59,94, base grossa, quadros B, lista de edição, taxa variável,
    /// faixa cheia, pixel não quadrado, giro): pts da tabela, um quadro por entrada e o quadro certo em cada posição,
    /// com efeito só de quadro, com leitura alargada (desentrelace por quadro) e até o fim da mídia.
    #[test]
    fn matriz_de_fontes() {
        let Some(cap) = fontes::cap() else { return };
        let raiz = fontes::pasta_temporaria("origem-matriz");
        let (mut falhas, mut feitos) = (Vec::new(), 0);
        for &f in fontes::MATRIZ {
            let m = apoio::medida(&cap, f);
            let tab = &m.info.quadros;
            let n = m.limite.min(tab.pts.len());
            // a medida de teste não traz o SHA-256 do arquivo (que entra na chave): um cache por fonte
            let raiz = raiz.join(format!("{f:?}"));
            for (nome, efeitos) in [("ajustes", json!([ajustes(0.03, 1.0)])), ("desentrelace", json!([desentrelacar("quadro")]))] {
                for (j_in, j_out) in [(8, 28), (n - 14, n)] {
                    let rotulo = format!("{f:?} {nome} {j_in}..{j_out} ({})", cap.versao);
                    let c = clipe(tab, j_in, j_out, efeitos.clone());
                    let (p, arquivo) = match produzir(&raiz.join("cache"), &raiz.join("projeto"), &cap, &c, &m.info, &AtomicBool::new(false), &mut |_| {}) {
                        Ok((p, _)) => (p.clone(), raiz.join("cache").join(&p.relativo)),
                        Err(e) => {
                            falhas.push(format!("{rotulo}: {e}"));
                            continue;
                        }
                    };
                    feitos += 1;
                    let esperado: Vec<i128> = tab.pts[j_in..j_out].iter().map(|q| (q - tab.pts[j_in]) as i128 * tab.tb.num as i128).collect();
                    let (tb, pts) = pacotes(&cap, &arquivo, arquivo.parent().unwrap(), &AtomicBool::new(false)).unwrap();
                    // na base do arquivo, em cruz com a da tabela
                    let do_arquivo: Vec<i128> = pts.iter().map(|&x| x as i128 * tb.num as i128 * tab.tb.den as i128).collect();
                    if do_arquivo != esperado.iter().map(|x| x * tb.den as i128).collect::<Vec<_>>() {
                        falhas.push(format!("{rotulo}: pts do arquivo {:?}… (base {}/{}) × tabela {:?}…", &pts[..pts.len().min(4)], tb.num, tb.den, &tab.pts[j_in..j_in + 4]));
                    }
                    match medir::pts(&cap, &arquivo) {
                        Ok(d) if d == pts => {}
                        outro => falhas.push(format!("{rotulo}: pts decodificados {:?}", outro.map(|d| d.len()))),
                    }
                    match medir::quadros_de_origem(&cap, &arquivo) {
                        Ok(v) if v == m.numeros[j_in..j_out] => {}
                        outro => falhas.push(format!("{rotulo}: quadro de cada posição {:?} × {:?}", outro.map(|v| v.into_iter().take(4).collect::<Vec<_>>()), &m.numeros[j_in..j_in + 4])),
                    }
                    if (p.largura, p.altura) != m.info.exibido || p.desloc_s != tab.tempo(j_in) {
                        falhas.push(format!("{rotulo}: {}×{} com deslocamento {}", p.largura, p.altura, p.desloc_s));
                    }
                }
            }
        }
        eprintln!("intermediário na matriz ({}): {feitos} feitos, {} falhas", cap.versao, falhas.len());
        assert!(falhas.is_empty(), "{}", falhas.join("\n"));
    }

    /// Intermediário por campo de fonte com o campo de baixo primeiro: um campo por quadro, na ordem do tempo, sem o
    /// croma do outro campo (`realce::cor_da_fonte`).
    #[test]
    fn por_campo_sem_croma_do_outro_campo() {
        let Some(cap) = fontes::cap() else { return };
        let raiz = fontes::pasta_temporaria("origem-bff");
        let f = fonte_bff(&cap, &raiz);
        let p = projeto_bff(&f, "auto");
        let Tolerante::Conhecido(ClipePrincipal::Trecho(c)) = &p.principal[0] else { panic!("trecho") };
        let (previa, _, arquivo) = fazer(&cap, &raiz, c, &f);
        let (fa, fb) = conferir_pts(&cap, &previa, &arquivo, c, &f, "bff por campo");
        assert_eq!((fa, fb), (20, 80));
        let v = croma_do_outro_campo(&cap, &arquivo, "null");
        // H.264 com perdas: folga sobre o fundo (128); misturado ficaria perto de 157
        assert!(v < 140.0, "({}) croma do outro campo no intermediário: V {v:.1}", cap.versao);
        // um campo por quadro, na ordem: a marca anda 4 px a cada um
        let q = medir::quadros(&cap, &arquivo).unwrap();
        let x: Vec<f64> = q.iter().map(|q| caixa_da_cor(q, |p| p[0] as i32 - p[1] as i32 > 90 && p[0] as i32 - p[2] as i32 > 90).expect("marca").1 .0).collect();
        for (k, x) in x.iter().enumerate() {
            let previsto = 40.0 + 4.0 * (20 + k) as f64 + 24.0;
            assert!((x - previsto).abs() <= 1.0, "campo {k} ({}): marca em {x:.1} × {previsto:.1}", cap.versao);
        }
    }

    /// Estabilizar por campo: a análise vê dois campos por quadro (a conferência dela conta em campos) e o
    /// intermediário sai com um quadro por campo; com o `.trf` gravado, o mesmo arquivo.
    #[test]
    fn estabilizar_por_campo() {
        let Some(cap) = fontes::cap() else { return };
        let raiz = fontes::pasta_temporaria("origem-estab-campo");
        let f = tremido(&cap, &raiz);
        let tab = &f.quadros;
        let c = clipe(tab, 20, 60, json!([desentrelacar("campo"), estabilizar(tab, 10, 109, "preta", 0.0, None)]));
        let (p, trf, arquivo) = fazer(&cap, &raiz, &c, &f);
        let trf = trf.expect("`.trf` novo");
        assert_eq!(conferir_pts(&cap, &p, &arquivo, &c, &f, "estabilizar por campo"), (40, 120));
        // estabilizou: o alvo quase não anda de um campo ao seguinte (na fonte, mais de 5 px por quadro)
        let centros: Vec<(f64, f64)> = medir::quadros(&cap, &arquivo).unwrap().iter().map(|q| caixa_da_cor(q, magenta).expect("alvo").1).collect();
        let passo = centros.windows(2).map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1)).sum::<f64>() / (centros.len() - 1) as f64;
        assert!(passo < 1.0, "({}) o alvo anda {passo:.2} px por campo no intermediário", cap.versao);
        let gravado = clipe(tab, 20, 60, json!([desentrelacar("campo"), estabilizar(tab, 10, 109, "preta", 0.0, Some(&trf.sha256))]));
        let (p2, trf2, _) = fazer(&cap, &raiz, &gravado, &f);
        assert!(trf2.is_none() && p2 == p, "({}) com o `.trf` gravado", cap.versao);
    }

    /// Fonte acima de 1080: o intermediário desce para 1080 de altura, com os mesmos pts.
    #[test]
    fn acima_de_1080_reduz() {
        let Some(cap) = fontes::cap() else { return };
        let raiz = fontes::pasta_temporaria("origem-4k");
        let v = gerar(&cap, &raiz, "grande.mp4", "testsrc2=s=2560x1440:r=30:d=0.5", &["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-bf", "0", "-video_track_timescale", "15360"]);
        let f = fonte_de(&cap, &v);
        let c = clipe(&f.quadros, 2, 12, json!([ajustes(0.02, 1.0)]));
        let (p, _, arquivo) = fazer(&cap, &raiz, &c, &f);
        conferir_pts(&cap, &p, &arquivo, &c, &f, "2560×1440");
        assert_eq!((p.largura, p.altura), (1920, 1080));
        let q = medir::quadro(&cap, &arquivo, 0).unwrap();
        assert_eq!((q.w, q.h), (1920, 1080));
    }

    /// A chave muda com o que muda a imagem, e só com isso.
    #[test]
    fn chave_do_intermediario() {
        let cap = apoio::cap_falsa();
        let tab = apoio::tabela((1, 15360), apoio::pts_regulares(120, (1, 30), (1, 15360), 0));
        let f = apoio::fonte(tab.clone(), (640, 360));
        let base = clipe(&tab, 20, 80, json!([ajustes(0.1, 1.2), efeito("destaque", "d1", json!({ "forma": "retangulo", "rastreio": "r1", "traco": { "cor": "#00ff00", "espessura": 0.006, "contorno": true }, "tamanho": 0.1, "angulo_graus": 0.0, "margem": 0.0, "rotulo": null }))]));
        let k = chave(&base, &f, &cap);
        assert_eq!(k.len(), 64);
        // igual: outro id e rótulo do clipe, velocidade, anotação diferente, nome do efeito, efeito de origem desligado
        let mut c = clipe(&tab, 20, 80, json!([ajustes(0.1, 1.2), { "tipo": "nitidez", "id": "n", "ligado": false, "nome": "", "ativo": null, "quantidade": 1.0 }]));
        c.base.id = "outro".into();
        c.velocidade = crate::edicao::modelo::Velocidade::Constante { fator: 0.5 };
        if let Tolerante::Conhecido(Efeito::Ajustes(a)) = &mut c.base.efeitos[0] {
            a.base.nome = "Mais claro".into();
        }
        assert_eq!(chave(&c, &f, &cap), k);
        // diferente: parâmetro, faixa, mídia, cor da mídia, ffmpeg
        assert_ne!(chave(&clipe(&tab, 20, 80, json!([ajustes(0.1, 1.3)])), &f, &cap), k);
        assert_ne!(chave(&clipe(&tab, 20, 81, json!([ajustes(0.1, 1.2)])), &f, &cap), k);
        assert_ne!(chave(&clipe(&tab, 21, 80, json!([ajustes(0.1, 1.2)])), &f, &cap), k);
        assert_ne!(chave(&base, &FonteInfo { sha256: "1".repeat(64), ..f.clone() }, &cap), k);
        assert_ne!(chave(&base, &FonteInfo { matriz: crate::edicao::modelo::Matriz::Bt601, ..f.clone() }, &cap), k);
        assert_ne!(chave(&base, &f, &Capacidades { sha256: "e".repeat(64), ..cap.clone() }), k);
        // estabilizar: `.trf` e zoom entram
        let e = |trf: Option<&str>, zoom: f64| chave(&clipe(&tab, 20, 80, json!([estabilizar(&tab, 10, 109, "ampliar", zoom, trf)])), &f, &cap);
        let sha = "a".repeat(64);
        assert!(e(None, 0.0) != e(Some(&sha), 0.0) && e(Some(&sha), 0.0) != e(Some(&sha), 5.0));
    }

    fn esperar(mut pedir: impl FnMut() -> Result<EstadoOrigem>) -> (PreviaMidia, Option<TrfNovo>, u32) {
        let (t0, mut fazendo) = (Instant::now(), 0);
        loop {
            match pedir() {
                Ok(EstadoOrigem::Pronto { previa, trf }) => return (previa, trf, fazendo),
                Ok(EstadoOrigem::Fazendo { tarefa, feito }) => {
                    assert!(!tarefa.is_empty() && (0.0..=1.0).contains(&feito));
                    fazendo += 1;
                }
                // a tabela do decodificador ainda está na fila de fundo
                Err(e) if e.to_string().contains("tabela_pendente") => {}
                Err(e) => panic!("pedido do intermediário: {e}"),
            }
            assert!(t0.elapsed() < Duration::from_secs(ESPERA_S), "intermediário não ficou pronto em {ESPERA_S} s");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Teto de espera do teste da fila (a máquina dos testes roda outros ao mesmo tempo): 3× o limite real de 40 s.
    const ESPERA_S: u64 = 120;

    /// Pela fila de fundo: `Fazendo` com progresso, depois `Pronto`; o `.trf` feito ali vai para o projeto e chega no
    /// estado; depois que a tela grava `trf_sha256` e `zoom_pct`, o mesmo arquivo serve sem refazer.
    #[test]
    fn pedir_pela_fila_e_o_trf_chega_ao_estado() {
        let Some(cap) = fontes::cap() else { return };
        let raiz = fontes::pasta_temporaria("origem-fila");
        let f = tremido(&cap, &raiz);
        let (cache, pasta_projeto) = (raiz.join("cache"), raiz.join("projeto"));
        let tab = &f.quadros;
        // sem efeito de origem não há intermediário
        let e = pedir(&raiz, &cache, &pasta_projeto, &cap, &clipe(tab, 20, 90, json!([])), &f).unwrap_err().to_string();
        assert!(e.contains("nao_suportado"), "{e}");
        for borda in ["preta", "ampliar"] {
            let c = clipe(tab, 20, 90, json!([estabilizar(tab, 10, 109, borda, 0.0, None)]));
            let (previa, trf, fazendo) = esperar(|| pedir(&raiz, &cache, &pasta_projeto, &cap, &c, &f));
            assert!(fazendo >= 1, "{borda}: o primeiro pedido tem de voltar `Fazendo`");
            let trf = trf.unwrap_or_else(|| panic!("{borda}: `.trf` novo no estado"));
            assert_eq!(trf.efeito, "s1");
            let guardado = pasta_projeto.join("vidstab").join(format!("{}.trf", trf.sha256));
            assert_eq!(sha256_do_arquivo(&guardado), trf.sha256, "{borda}: `.trf` gravado no projeto pelo conteúdo");
            assert!(if borda == "ampliar" { trf.zoom_pct > 3.0 } else { trf.zoom_pct == 0.0 }, "{borda}: zoom {}", trf.zoom_pct);
            // a exportação que chegar antes do intermediário analisa igual: mesmo nome no grafo, mesmo arquivo
            let plano = plano_tremido(&cap, &f, 20, 90, json!([estabilizar(tab, 10, 109, borda, 0.0, None)]), json!({}), &alvo_final("cpu"));
            let (_, da_exportacao, _) = rodar_previas(&cap, &plano, &fontes::pasta_temporaria("fila-previa")).remove(0);
            assert_eq!(da_exportacao, trf.sha256, "{borda} ({}): `.trf` da prévia do render × o do intermediário", cap.versao);
            let arquivo = cache.join(&previa.relativo);
            conferir_pts(&cap, &previa, &arquivo, &c, &f, borda);
            // o pedido repetido devolve o mesmo, com o `.trf` (a tela pode não ter gravado ainda)
            assert_eq!(pedir(&raiz, &cache, &pasta_projeto, &cap, &c, &f).unwrap(), EstadoOrigem::Pronto { previa: previa.clone(), trf: Some(trf.clone()) });
            // a tela gravou: chave nova, mesmo arquivo, nada a gravar de novo, nada refeito
            let antes = std::fs::metadata(&arquivo).unwrap().modified().unwrap();
            let gravado = clipe(tab, 20, 90, json!([estabilizar(tab, 10, 109, borda, trf.zoom_pct, Some(&trf.sha256))]));
            assert_ne!(chave(&gravado, &f, &cap), chave(&c, &f, &cap));
            assert_eq!(pedir(&raiz, &cache, &pasta_projeto, &cap, &gravado, &f).unwrap(), EstadoOrigem::Pronto { previa: previa.clone(), trf: None }, "{borda}");
            assert_eq!(std::fs::metadata(&arquivo).unwrap().modified().unwrap(), antes, "{borda}: refez");
            // outro recorte do mesmo clipe usa o `.trf` guardado: nenhum `.trf` novo
            let aparado = clipe(tab, 40, 80, json!([estabilizar(tab, 10, 109, borda, trf.zoom_pct, Some(&trf.sha256))]));
            let (p2, trf2, _) = esperar(|| pedir(&raiz, &cache, &pasta_projeto, &cap, &aparado, &f));
            assert!(trf2.is_none() && p2.relativo != previa.relativo, "{borda}");
            assert_eq!(std::fs::read_dir(pasta_projeto.join("vidstab")).unwrap().count(), 1, "{borda}: a mesma análise, um `.trf` só");
        }
        // borda trocada para "ampliar" depois da análise (zoom zerado): o zoom calculado volta no estado, com o mesmo `.trf`
        let sha = std::fs::read_dir(pasta_projeto.join("vidstab")).unwrap().flatten().next().unwrap().file_name().to_string_lossy().trim_end_matches(".trf").to_string();
        let trocado = clipe(tab, 30, 70, json!([estabilizar(tab, 10, 109, "ampliar", 0.0, Some(&sha))]));
        let (_, trf, _) = esperar(|| pedir(&raiz, &cache, &pasta_projeto, &cap, &trocado, &f));
        let trf = trf.expect("zoom calculado");
        assert!(trf.sha256 == sha && trf.zoom_pct > 3.0, "{trf:?}");
        // `.trf` citado e ausente do projeto: erro com o código do catálogo, não imagem sem estabilizar
        let orfao = clipe(tab, 20, 90, json!([estabilizar(tab, 10, 109, "preta", 0.0, Some(&"b".repeat(64)))]));
        let e = produzir(&cache, &pasta_projeto, &cap, &orfao, &f, &AtomicBool::new(false), &mut |_| {}).unwrap_err().to_string();
        assert!(e.contains("arquivo_ausente"), "{e}");
        // `.trf` do projeto trocado por fora: o conteúdo não confere com o nome
        std::fs::write(pasta_projeto.join("vidstab").join(format!("{}.trf", "b".repeat(64))), b"VID.STAB 1\n").unwrap();
        let e = produzir(&cache, &pasta_projeto, &cap, &orfao, &f, &AtomicBool::new(false), &mut |_| {}).unwrap_err().to_string();
        assert!(e.contains("leitura_divergente"), "{e}");
    }

    /// `edicao_origem_pedir` num caso de verdade: acha o clipe no JSON da tela, espera a tabela do decodificador e faz.
    #[test]
    fn pedir_do_projeto_num_caso() {
        let Some(cap) = fontes::cap() else { return };
        let (_d, ws) = crate::edicao::executar::apoio::caso();
        let cache = ws.parent().unwrap().join("cache");
        let m = apoio::medida(&cap, Fonte::Mkv30);
        std::fs::create_dir_all(ws.join("Midias")).unwrap();
        std::fs::copy(&m.info.caminho, ws.join("Midias").join("m1.mkv")).unwrap();
        let mut midia = midia("m1", "video", json!([]));
        midia["sha256"] = json!(sha256_do_arquivo(&m.info.caminho));
        midia["relativo"] = json!("Midias/m1.mkv");
        let tab = &m.info.quadros;
        let congelado = json!({ "tipo": "congelar", "id": "g1", "rotulo": "", "midia": "m1", "instante_s": tab.tempo(5), "instante_pts": tab.pts[5], "duracao_s": 1.0, "efeitos": [media(4)] });
        let mut p = projeto(320, 180, (30, 1), vec![midia], vec![trecho("c1", "m1", tab, 40, 70, 1.0, json!([ajustes(0.05, 1.0)])), trecho("c2", "m1", tab, 80, 90, 1.0, json!([])), congelado], json!({}));
        p.id = uuid::Uuid::new_v4().to_string();
        let doc = serde_json::to_value(&p).unwrap();
        let (previa, trf, _) = esperar(|| pedir_do_projeto(&ws, &cache, &doc, "c1"));
        assert!(trf.is_none());
        let arquivo = cache.join(&previa.relativo);
        let c: ClipeTrecho = serde_json::from_value(doc["principal"][0].clone()).unwrap();
        conferir_pts(&cap, &previa, &arquivo, &c, &m.info, "caso");
        assert_eq!(medir::quadros_de_origem(&cap, &arquivo).unwrap(), m.numeros[40..70]);
        for (id, pedaco) in [("c2", "nao_suportado: clipe c2 sem efeito de origem"), ("g1", "nao_suportado: intermediário de quadro congelado"), ("nao-existe", "item_invalido: clipe nao-existe")] {
            let e = pedir_do_projeto(&ws, &cache, &doc, id).unwrap_err().to_string();
            assert!(e.contains(pedaco), "{id}: {e}");
        }
    }

    /// Centro (px) da moldura verde do destaque e do alvo magenta num quadro.
    fn centros(q: &medir::QuadroRgb) -> ((f64, f64), (f64, f64)) {
        let (v, _) = caixa_da_cor(q, verde).expect("moldura verde");
        let (_, m) = caixa_da_cor(q, magenta).expect("alvo magenta");
        (((v[0] + v[2]) as f64 / 2.0, (v[1] + v[3]) as f64 / 2.0), m)
    }

    /// Marcação feita sobre o intermediário cai a até 2 px no render: o alvo medido nos quadros do intermediário vira
    /// âncora de um destaque e o render (mesma cadeia de origem) desenha a moldura em volta dele.
    #[test]
    fn marcacao_no_intermediario_a_ate_2_px() {
        let Some(cap) = fontes::cap() else { return };
        let raiz = fontes::pasta_temporaria("origem-marca");
        let f = tremido(&cap, &raiz);
        let tab = &f.quadros;
        let (j_in, j_out) = (25usize, 85usize);
        let lente = efeito("lente", "l1", json!({ "k1": -0.12, "k2": 0.0, "cx": 0.5, "cy": 0.5, "perfil": "" }));
        let sem_trf = clipe(tab, j_in, j_out, json!([estabilizar(tab, 10, 109, "ampliar", 0.0, None), lente.clone()]));
        let (previa, trf, arquivo) = fazer(&cap, &raiz, &sem_trf, &f);
        let trf = trf.expect("`.trf` novo");
        assert_eq!((previa.largura, previa.altura), (TREMIDO.0, TREMIDO.1));
        // o perito marca o alvo em cada quadro do intermediário (0..1 da imagem que ele vê)
        let quadros = medir::quadros(&cap, &arquivo).unwrap();
        assert_eq!(quadros.len(), j_out - j_in);
        let marcar = |q: &medir::QuadroRgb| {
            let (c, _) = caixa_da_cor(q, magenta).expect("alvo no intermediário");
            [c[0] as f64 / q.w as f64, c[1] as f64 / q.h as f64, (c[2] - c[0]) as f64 / q.w as f64, (c[3] - c[1]) as f64 / q.h as f64]
        };
        let destaque = efeito("destaque", "d1", json!({ "forma": "retangulo", "rastreio": "r1", "traco": { "cor": "#00ff00", "espessura": 0.006, "contorno": false }, "tamanho": 0.1, "angulo_graus": 0.0, "margem": 0.02, "rotulo": null }));
        let efeitos = json!([estabilizar(tab, 10, 109, "ampliar", trf.zoom_pct, Some(&trf.sha256)), lente, destaque]);
        let guardado = raiz.join("projeto").join("vidstab").join(format!("{}.trf", trf.sha256));
        let render = |nome: &str, ancoras: Vec<Value>| {
            let plano = plano_tremido(&cap, &f, j_in, j_out, efeitos.clone(), json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras)] }), &alvo_final("cpu"));
            renderizar(&cap, &plano, nome, &[&guardado]).1
        };
        let do_intermediario: Vec<Value> = quadros.iter().zip(j_in..).map(|(q, j)| ancora(tab.tempo(j), Some(tab.pts[j]), marcar(q))).collect();
        let saida = render("marca", do_intermediario);
        let mut pior = 0.0f64;
        for (k, q) in saida.iter().enumerate() {
            let ((vx, vy), (mx, my)) = centros(q);
            let d = (vx - mx).abs().max((vy - my).abs());
            assert!(d <= 2.0, "quadro {k} ({}): moldura em ({vx:.1}, {vy:.1}), alvo em ({mx:.1}, {my:.1})", cap.versao);
            // e o alvo do render está onde o intermediário o mostrou
            let (_, (ix, iy)) = caixa_da_cor(&quadros[k], magenta).unwrap();
            assert!((ix - mx).abs() <= 1.0 && (iy - my).abs() <= 1.0, "quadro {k}: alvo em ({ix:.1}, {iy:.1}) no intermediário e ({mx:.1}, {my:.1}) no render");
            pior = pior.max(d);
        }
        // controle: marcado sobre a imagem crua (sem o intermediário) o destaque erra o alvo estabilizado
        let cru = medir::quadros(&cap, &f.caminho).unwrap();
        let da_fonte: Vec<Value> = (j_in..j_out).map(|j| ancora(tab.tempo(j), Some(tab.pts[j]), marcar(&cru[j]))).collect();
        let crua = render("marca-crua", da_fonte);
        let errados = crua
            .iter()
            .filter(|q| {
                let ((vx, vy), (mx, my)) = centros(q);
                (vx - mx).abs().max((vy - my).abs()) > 2.0
            })
            .count();
        assert!(errados > (j_out - j_in) / 2, "controle: só {errados} quadros erram com a marcação feita na imagem crua");
        eprintln!("marcação sobre o intermediário ({}): pior desvio {pior:.2} px em {} quadros; na imagem crua {errados} erram mais de 2 px", cap.versao, saida.len());
    }
}
