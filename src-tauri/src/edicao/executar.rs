//! Roda um `Plano` (§5.2, §5.8): prévias, camadas, blocos, áudio, junção e conferência. Dono: P0.7.
//! `preparar` é o único lugar com E/S antes do render (banco, tabelas, sonda, leitor de rastreios).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::armazem;
use super::camada::{self, ArquivoCamada, CamadaPronta};
use super::conferir::{self, ConferenciaSaida};
use super::ffmpeg::args;
use super::ffmpeg::exec::{self, Comando, Termino};
use super::ffmpeg::sonda::{self, Capacidades};
use super::fila::Fase;
use super::fundo::{self, EstadoFundo, TipoFundo};
use super::mensagens::codigo;
use super::modelo::{EstadoMidia, MidiaRef, OrigemMidia, ProjetoEdicao, SituacaoMidia};
use super::motor::audio::{self, Loudnorm};
use super::motor::plano::{Bloco, Entrada, Juncao, Papel, Plano, Previa, MARCA_GANHO};
use super::motor::{Alvo, Contexto, FonteInfo, LeitorRastreios};
use super::quadros::{self, OrigemTabela, TabelaQuadros};
use super::rastreio::{ArquivoDenso, ArquivoSilhuetas};
use crate::error::{Result, SicroError};
use crate::workspace::paths;

/// Prefixos de `SicroError::Validation` com que `executar` classifica a falha.
/// `ERRO_GPU`: a fila recompila com `EscolhaCodificador::Cpu` e refaz tudo (um codificador por exportação).
pub const ERRO_GPU: &str = "falha_gpu";
pub const ERRO_CANCELADO: &str = "cancelado";
pub const ERRO_LEITURA_DIVERGENTE: &str = "leitura_divergente";

/// Retrato do perito (`execucao.autor`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Autor {
    pub nome: String,
    pub matricula: String,
    pub cargo: String,
}

/// Retrato do caso (`execucao.caso`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RetratoCaso {
    pub numero: String,
    pub referencia: String,
    pub orgao: String,
    pub requisitante: String,
}

/// `execucao.sicro`: versão, commit e alvo de compilação.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VersaoSicro {
    pub versao: String,
    pub commit: String,
    pub alvo: String,
}

/// `instrucoes` = conjuntos de instruções da CPU (avx2, avx512f…): definem a classe de máquina.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Cpu {
    pub modelo: String,
    pub instrucoes: Vec<String>,
}

/// `execucao.sistema`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Sistema {
    pub so: String,
    pub cpu: Cpu,
    pub gpu: Option<String>,
    pub driver: Option<String>,
}

/// Quem, onde e com quê: a parte de `execucao` que não depende do render.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Identidade {
    pub autor: Autor,
    pub caso: RetratoCaso,
    pub sicro: VersaoSicro,
    pub sistema: Sistema,
}

/// Ciência do perito num aviso pericial: código do §2.3 e quando (RFC 3339).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ciencia {
    pub codigo: String,
    pub ciente_em: String,
}

/// Onde e com quê o plano roda. `trabalho` = `render/<tarefa>/` (é o `current_dir` dos processos);
/// `cache` = pasta da Edição do caso no cache; `caso` = raiz do caso; `pasta_projeto` = `Sistema/videos/edicao/<id>/`
/// (destino dos `.trf`); `paralelos` = blocos ao mesmo tempo.
#[derive(Debug, Clone)]
pub struct Ambiente {
    pub cap: Arc<Capacidades>,
    pub trabalho: PathBuf,
    pub cache: PathBuf,
    pub caso: PathBuf,
    pub pasta_projeto: PathBuf,
    pub paralelos: u32,
    pub identidade: Identidade,
    pub ciencias: Vec<Ciencia>,
}

/// Avanço do render: fase, slots feitos e totais, quadros por segundo e segundos que faltam.
/// É o evento `edicao:progresso` sem o `tarefa_id` (`fila::EventoProgresso`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Progresso {
    pub fase: Fase,
    pub feito_q: u64,
    pub total_q: u64,
    pub qps: f32,
    pub restante_s: u32,
}

/// Bloco rodado (`execucao.blocos`): `reaproveitado` = veio do cache; `sha256` = do arquivo do bloco.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlocoFeito {
    pub indice: usize,
    pub de_q: u64,
    pub n_q: u32,
    pub chave: String,
    pub reaproveitado: bool,
    pub sha256: String,
}

/// pts lidos de um clipe na exportação final (do `showinfo`), na ordem: alimentam o `mapa.csv`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LidosClipe {
    pub clipe: String,
    pub pts: Vec<i64>,
}

/// `.trf` gerado por uma prévia `Vidstab`: já copiado para `vidstab/<sha256>.trf` do projeto.
/// O front grava `trf_sha256` e `zoom_pct` no efeito por `alterar()`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrfNovo {
    pub efeito: String,
    pub sha256: String,
    pub zoom_pct: f64,
}

/// O que o render produziu (a parte `execucao` do `tecnico.json` que varia, §10.2).
/// Caminhos absolutos, na pasta de trabalho: a fila leva à pasta da exportação.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Resultado {
    /// Vídeo final (ou o BMP do quadro final).
    pub arquivo: PathBuf,
    pub sha256: String,
    /// Bytes.
    pub tamanho: u64,
    /// Nome do codificador de vídeo usado e os argumentos dele.
    pub codificador: String,
    pub argumentos: Vec<String>,
    /// Motivo da queda de GPU para CPU; None = não houve.
    pub queda_para_cpu: Option<String>,
    /// Hash do composto (coluna de hash dos `framehash`, na ordem dos slots); None sem framehash.
    pub composto_sha256: Option<String>,
    /// Arquivo `quadros.compostos.sha256` (um hash por slot); None sem framehash.
    pub compostos: Option<PathBuf>,
    /// SHA-256 do PCM (`audio.wav`); None sem passo de áudio.
    pub audio_sha256: Option<String>,
    /// Medição do volume e ganho aplicado (dB).
    pub loudnorm: Option<Loudnorm>,
    pub ganho_db: Option<f64>,
    pub blocos: Vec<BlocoFeito>,
    /// `arquivo` relativo ao cache da Edição do caso (`camadas/<hash>/000000.png`): os nomes se repetem entre camadas.
    pub camadas: Vec<ArquivoCamada>,
    pub passos: Vec<Termino>,
    /// Só na exportação final.
    pub conferencia: Option<ConferenciaSaida>,
    pub lidos: Vec<LidosClipe>,
    /// Arquivos de legenda gravados.
    pub legendas: Vec<PathBuf>,
    pub trf: Vec<TrfNovo>,
    /// RFC 3339.
    pub renderizado_em: String,
}

/// Dados lidos do disco para compilar e analisar; `contexto()` empresta tudo ao compilador e à validação.
/// `midias` = situação de cada `MidiaRef` (só as `Ok` entram em `fontes`, com a chave `MidiaRef.id`).
pub struct Preparado {
    pub fontes: HashMap<String, FonteInfo>,
    pub cap: Arc<Capacidades>,
    pub rastreios: Box<dyn LeitorRastreios + Send + Sync>,
    pub pasta_projeto: PathBuf,
    pub midias: Vec<EstadoMidia>,
}

impl Preparado {
    /// Contexto do compilador sobre estes dados.
    pub fn contexto(&self) -> Contexto<'_> {
        Contexto {
            fontes: &self.fontes,
            cap: &self.cap,
            rastreios: self.rastreios.as_ref(),
            pasta_projeto: &self.pasta_projeto,
            midias: &self.midias,
        }
    }
}

fn erro(texto: impl Into<String>) -> SicroError {
    SicroError::Validation(texto.into())
}

fn agora() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Código de um `Validation("<código>[: detalhe]")`.
pub(crate) fn codigo_de(e: &SicroError) -> Option<&str> {
    match e {
        SicroError::Validation(m) => Some(m.split(':').next().unwrap_or("").trim()),
        _ => None,
    }
}

pub(crate) fn sha_valido(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

pub(crate) fn sha256_bytes(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}

/// Guarda a análise da estabilização em `vidstab/<sha256>.trf` do projeto (se ainda não está) e devolve o SHA-256.
/// Temporário próprio de cada gravação: a exportação e o intermediário (ou dois intermediários do mesmo trecho)
/// gravam o mesmo `.trf` ao mesmo tempo, e com um nome só um trocava o arquivo do outro e o outro falhava.
pub(crate) fn guardar_trf(pasta_projeto: &Path, bytes: &[u8]) -> Result<String> {
    static N: AtomicU64 = AtomicU64::new(0);
    let sha256 = sha256_bytes(bytes);
    let destino = pasta_projeto.join("vidstab").join(format!("{sha256}.trf"));
    if !destino.is_file() {
        fs::create_dir_all(pasta_projeto.join("vidstab"))?;
        let tmp = destino.with_extension(format!("trf.{}-{}.tmp", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let r = fs::write(&tmp, bytes).and_then(|_| fs::rename(&tmp, &destino));
        if r.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        r?;
    }
    Ok(sha256)
}

// ---------------------------------------------------------------------------------------------- preparar

/// Lê banco, tabelas de quadros, sonda e rastreios do projeto. `ws` = raiz do caso; `cache` = pasta da Edição do caso.
/// `conferir_sha256` = recalcula o SHA-256 de cada mídia (sempre na exportação final; uma vez por sessão no resto).
pub fn preparar(ws: &Path, p: &ProjetoEdicao, cache: &Path, conferir_sha256: bool) -> Result<Preparado> {
    preparar_com(ws, p, cache, conferir_sha256, None)
}

/// `preparar`; com `esperar` (exportação final), espera a tabela do decodificador de cada mídia que só tem a de
/// pacotes (sem ela a validação devolve `tabela_pendente`). `esperar` = sinal de cancelamento.
pub(crate) fn preparar_com(
    ws: &Path,
    p: &ProjetoEdicao,
    cache: &Path,
    conferir_sha256: bool,
    esperar: Option<&AtomicBool>,
) -> Result<Preparado> {
    let cap = sonda::capacidades()?;
    let pasta_projeto = armazem::pasta_projeto(ws, &p.id);
    let mut banco = BancoMidias::Fechado;
    let mut fontes = HashMap::new();
    let mut midias = Vec::with_capacity(p.midias.len());
    for m in &p.midias {
        let (estado, fonte) = preparar_midia(ws, m, cache, &cap, conferir_sha256, esperar, &mut banco)?;
        if let Some(f) = fonte {
            fontes.insert(m.id.clone(), f);
        }
        midias.push(estado);
    }
    Ok(Preparado {
        fontes,
        cap,
        rastreios: Box::new(LeitorDisco { pasta: pasta_projeto.clone(), densos: Mutex::new(HashMap::new()) }),
        pasta_projeto,
        midias,
    })
}

fn preparar_midia(
    ws: &Path,
    m: &MidiaRef,
    cache: &Path,
    cap: &Arc<Capacidades>,
    conferir_sha256: bool,
    esperar: Option<&AtomicBool>,
    banco: &mut BancoMidias,
) -> Result<(EstadoMidia, Option<FonteInfo>)> {
    let estado = |s| EstadoMidia { id: m.id.clone(), estado: s, tabela: None, busca: None, erros: None };
    if !sha_valido(&m.sha256) {
        return Ok((estado(SituacaoMidia::Ausente), None));
    }
    let Some(caminho) = localizar(ws, m, banco) else { return Ok((estado(SituacaoMidia::Ausente), None)) };
    match sha_da_sessao(&caminho, conferir_sha256, esperar) {
        Ok(sha) if sha == m.sha256 => {}
        Ok(_) => return Ok((estado(SituacaoMidia::Trocada), None)),
        Err(e) if codigo_de(&e) == Some(ERRO_CANCELADO) => return Err(e),
        Err(_) => return Ok((estado(SituacaoMidia::Ausente), None)),
    }
    let mut tab = match TabelaQuadros::obter_com(cap, cache, &m.sha256, &caminho) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("edição: tabela de {}: {e}", m.sha256);
            return Ok((estado(SituacaoMidia::Ausente), None));
        }
    };
    let mut info = match fonte_info_memo(cap, &caminho, &m.sha256, tab.clone(), m.cor.as_ref()) {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!("edição: ffprobe de {}: {e}", m.sha256);
            return Ok((estado(SituacaoMidia::Ausente), None));
        }
    };
    if info.quadros.origem == OrigemTabela::Pacotes {
        // a exportação espera por ela: não fica atrás dos proxies
        let chave = pedir_tabela(cap, cache, &m.sha256, &caminho, &info.conteiner, esperar.is_some());
        if let Some(cancelar) = esperar {
            esperar_fundo(&chave, cancelar)?;
            tab = TabelaQuadros::obter_com(cap, cache, &m.sha256, &caminho)?;
            info = fonte_info_memo(cap, &caminho, &m.sha256, tab.clone(), m.cor.as_ref())?;
        }
    }
    let t = &info.quadros;
    let e = EstadoMidia {
        id: m.id.clone(),
        estado: SituacaoMidia::Ok,
        tabela: Some(t.origem),
        busca: Some(t.busca),
        erros: (t.origem == OrigemTabela::Decodificador).then_some(t.erros_decodificacao),
    };
    Ok((e, Some(info)))
}

/// `quadros::fonte_info` (a única leitura do ffprobe) guardada por arquivo, SHA-256, cor e tabela: a análise roda
/// a cada mudança do documento e o `preparar` não chama o ffprobe de novo para cada mídia. Tabela nova = leitura nova.
pub(crate) fn fonte_info_memo(
    cap: &Capacidades,
    caminho: &Path,
    sha256: &str,
    tab: Arc<TabelaQuadros>,
    cor: Option<&super::modelo::CorMidia>,
) -> Result<FonteInfo> {
    type Chave = (PathBuf, String, String, String);
    type Memo = Mutex<HashMap<Chave, (Arc<TabelaQuadros>, FonteInfo)>>;
    static MEMO: OnceLock<Memo> = OnceLock::new();
    let memo = MEMO.get_or_init(Default::default);
    let cor_txt = cor.and_then(|c| serde_json::to_string(c).ok()).unwrap_or_default();
    let chave = (caminho.to_path_buf(), sha256.to_string(), cor_txt, cap.sha256.clone());
    let guardada = memo.lock().unwrap_or_else(|e| e.into_inner()).get(&chave).filter(|(t, _)| Arc::ptr_eq(t, &tab)).map(|(_, f)| f.clone());
    if let Some(f) = guardada {
        return Ok(f);
    }
    let f = quadros::fonte_info(cap, caminho, sha256, tab.clone(), cor)?;
    memo.lock().unwrap_or_else(|e| e.into_inner()).insert(chave, (tab, f.clone()));
    Ok(f)
}

/// Mídia sem tempo nos pacotes (H.264 cru, MPEG-PS): a tabela de pacotes inventa os pts; a do decodificador vai na
/// frente da fila de fundo (decisão (g)14).
pub(crate) fn sem_tempo_nos_pacotes(conteiner: &str) -> bool {
    conteiner.split(',').any(|c| matches!(c, "h264" | "hevc" | "mpeg" | "mpegvideo"))
}

/// Pede a tabela do decodificador à fila de fundo (chave única) e devolve a chave. `na_frente` = a exportação, que
/// espera por ela (refaz a que falhou há pouco).
pub(crate) fn pedir_tabela(cap: &Arc<Capacidades>, cache: &Path, sha256: &str, video: &Path, conteiner: &str, na_frente: bool) -> String {
    let chave = fundo::chave(TipoFundo::Tabela, sha256);
    let (cap, cache, sha, video) = (cap.clone(), cache.to_path_buf(), sha256.to_string(), video.to_path_buf());
    let trabalho: fundo::Trabalho = Box::new(move |c, prog| {
        let t = TabelaQuadros::do_decodificador(&cap, &video, c, prog)?;
        t.guardar(&cache, &sha).map(|_| ())
    });
    if na_frente {
        fundo::pedir_na_frente(TipoFundo::Tabela, &chave, trabalho);
    } else if sem_tempo_nos_pacotes(conteiner) {
        // a tela pede a cada quadro: passa à frente, mas não refaz o que acabou de falhar (decisão (h)14)
        fundo::pedir_com_prioridade(TipoFundo::Tabela, &chave, trabalho);
    } else {
        fundo::pedir(TipoFundo::Tabela, &chave, trabalho);
    }
    chave
}

/// Espera o trabalho de fundo `chave` terminar (pronto ou não).
fn esperar_fundo(chave: &str, cancelar: &AtomicBool) -> Result<()> {
    loop {
        if cancelar.load(Ordering::SeqCst) {
            return Err(erro(format!("{ERRO_CANCELADO}: tabela")));
        }
        match fundo::estado(chave).map(|t| t.estado) {
            None | Some(EstadoFundo::Pronta | EstadoFundo::Falhou { .. } | EstadoFundo::Cancelada) => return Ok(()),
            _ => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

enum BancoMidias {
    Fechado,
    Aberto(rusqlite::Connection, uuid::Uuid),
    Sem,
}

/// `relativo` que fica dentro do caso: sem raiz, sem unidade e sem `..` (o texto vem do documento da tela).
pub(crate) fn relativo_seguro(rel: &str) -> bool {
    use std::path::Component;
    !rel.is_empty() && Path::new(rel).components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Arquivo da mídia: o `relativo` gravado; mídia do caso sem ele (ou movida) pelo SHA-256 no banco.
fn localizar(ws: &Path, m: &MidiaRef, banco: &mut BancoMidias) -> Option<PathBuf> {
    if relativo_seguro(&m.relativo) {
        let p = paths::resolver_relativo(ws, &m.relativo);
        if p.is_file() {
            return Some(p);
        }
    }
    if m.origem != OrigemMidia::Caso {
        return None;
    }
    if matches!(banco, BancoMidias::Fechado) {
        *banco = abrir_banco(ws).map(|(c, o)| BancoMidias::Aberto(c, o)).unwrap_or(BancoMidias::Sem);
    }
    let BancoMidias::Aberto(conn, occ) = banco else { return None };
    let v = crate::database::repositories::video_repo::find_media_by_sha256(conn, occ, &m.sha256).ok()??;
    let p = paths::resolver_relativo(ws, &v.relative_path);
    p.is_file().then_some(p)
}

pub(crate) fn abrir_banco(ws: &Path) -> Result<(rusqlite::Connection, uuid::Uuid)> {
    let occ = crate::workspace::manifest::Manifest::read(ws)?.occurrence_id;
    let banco = paths::sqlite_path(ws);
    if !banco.is_file() {
        return Err(SicroError::Database("banco do caso ausente".into()));
    }
    let mut conn = crate::database::connection::open_connection(&banco)?;
    crate::database::migrations::run_migrations(&mut conn)?;
    Ok((conn, occ))
}

/// SHA-256 do arquivo; fora da exportação final, uma vez por sessão (mesmo tamanho e mesma data = mesmo conteúdo).
/// `cancelar` = sinal da exportação (fonte de vários GB): olhado a cada pedaço.
fn sha_da_sessao(caminho: &Path, sempre: bool, cancelar: Option<&AtomicBool>) -> Result<String> {
    type Memo = Mutex<HashMap<PathBuf, (u64, Option<SystemTime>, String)>>;
    static MEMO: OnceLock<Memo> = OnceLock::new();
    let meta = fs::metadata(caminho)?;
    let (tam, data) = (meta.len(), meta.modified().ok());
    let memo = MEMO.get_or_init(Default::default);
    if !sempre {
        if let Some((t, d, s)) = memo.lock().unwrap_or_else(|e| e.into_inner()).get(caminho) {
            if *t == tam && *d == data {
                return Ok(s.clone());
            }
        }
    }
    let sha = sha256_em_pedacos(caminho, cancelar.unwrap_or(&AtomicBool::new(false)), &mut |_, _| {})?;
    memo.lock().unwrap_or_else(|e| e.into_inner()).insert(caminho.to_path_buf(), (tam, data, sha.clone()));
    Ok(sha)
}

/// Pedaço do SHA-256 da sessão: o cancelamento é olhado entre dois pedaços.
const PEDACO_SHA: usize = 4 << 20;

/// SHA-256 do arquivo lido em pedaços, olhando o cancelamento; `progresso(lidos, total)` a cada pedaço.
pub(crate) fn sha256_em_pedacos(caminho: &Path, cancelar: &AtomicBool, progresso: &mut dyn FnMut(u64, u64)) -> Result<String> {
    let mut f = fs::File::open(caminho)?;
    let total = f.metadata()?.len();
    let mut h = Sha256::new();
    let mut buf = vec![0u8; PEDACO_SHA];
    let mut lidos = 0u64;
    loop {
        if cancelar.load(Ordering::SeqCst) {
            return Err(erro(format!("{ERRO_CANCELADO}: sha256")));
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        lidos += n as u64;
        progresso(lidos, total);
    }
    Ok(format!("{:x}", h.finalize()))
}

/// Rastreios do disco: `rastreios/<id>/<sha256>.json`, conferido pelo SHA-256 e guardado em memória.
struct LeitorDisco {
    pasta: PathBuf,
    densos: Mutex<HashMap<(String, String), Arc<ArquivoDenso>>>,
}

impl LeitorRastreios for LeitorDisco {
    fn denso(&self, rastreio: &str, sha256: &str) -> Result<Arc<ArquivoDenso>> {
        let ausente = || erro(format!("{}: {rastreio}", codigo::RASTREIO_AUSENTE));
        if !armazem::id_valido(rastreio) || !sha_valido(sha256) {
            return Err(ausente());
        }
        let chave = (rastreio.to_string(), sha256.to_string());
        if let Some(d) = self.densos.lock().unwrap_or_else(|e| e.into_inner()).get(&chave) {
            return Ok(d.clone());
        }
        let bytes = fs::read(self.pasta.join("rastreios").join(rastreio).join(format!("{sha256}.json"))).map_err(|_| ausente())?;
        if sha256_bytes(&bytes) != sha256 {
            return Err(ausente());
        }
        let d: ArquivoDenso = serde_json::from_slice(&bytes).map_err(|e| erro(format!("{}: {e}", codigo::RASTREIO_AUSENTE)))?;
        let d = Arc::new(d);
        self.densos.lock().unwrap_or_else(|e| e.into_inner()).insert(chave, d.clone());
        Ok(d)
    }

    fn silhuetas(&self, _rastreio: &str, _sha256: &str) -> Result<Arc<ArquivoSilhuetas>> {
        // o formato do arquivo fecha na Fase 2
        Err(erro(format!("{}: silhuetas", codigo::NAO_SUPORTADO)))
    }
}

// ---------------------------------------------------------------------------------------------- executar

/// Entre dois avisos de progresso. Quem recebe vê o aviso entre a passagem dele e a do seguinte (o aviso sai com o
/// limitador preso, ou na mesma thread): 6 avisos vistos ocupam ao menos 4 intervalos, e 4 × 250 ms = 1 s.
pub(crate) const INTERVALO_PROGRESSO: Duration = Duration::from_millis(250);

/// Deixa passar um aviso a cada `INTERVALO_PROGRESSO`.
#[derive(Default)]
pub(crate) struct Limitador {
    ultimo: Option<Instant>,
}

impl Limitador {
    pub(crate) fn passa(&mut self) -> bool {
        let agora = Instant::now();
        if self.ultimo.is_some_and(|u| agora.duration_since(u) < INTERVALO_PROGRESSO) {
            return false;
        }
        self.ultimo = Some(agora);
        true
    }
}

/// Codificador de vídeo nos argumentos (`-c:v <nome>`).
pub(crate) fn codificador_de(args: &[String]) -> Option<&str> {
    args.windows(2).find(|w| matches!(w[0].as_str(), "-c:v" | "-codec:v" | "-vcodec")).map(|w| w[1].as_str())
}

/// Codificador de GPU (pelo nome).
pub(crate) fn e_gpu(nome: &str) -> bool {
    ["_nvenc", "_qsv", "_vaapi", "_amf", "_mf", "_videotoolbox", "_v4l2m2m"].iter().any(|s| nome.ends_with(s))
}

fn bloco_gpu(b: &Bloco) -> bool {
    codificador_de(&b.codificador).is_some_and(e_gpu)
}

/// A falha derruba a exportação para a CPU? Cancelamento (inclusive o `matar_todos` da saída do app) e leitura
/// divergente (a decodificação é sempre em CPU) não são falha de GPU.
fn falha_de_gpu(e: &SicroError, gpu: bool) -> bool {
    gpu && !matches!(codigo_de(e), Some(ERRO_CANCELADO) | Some(ERRO_LEITURA_DIVERGENTE))
}

/// Blocos ao mesmo tempo (§5.8): GPU, 2 sessões; CPU, 1 a cada 8 núcleos, até 2.
pub(crate) fn paralelos(gpu: bool) -> u32 {
    if gpu {
        return 2;
    }
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as u32;
    (n / 8).clamp(1, 2)
}

fn nome_simples(n: &str) -> bool {
    !n.is_empty()
        && !n.starts_with('-')
        && !n.starts_with('.')
        && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// "b003.mp4" → ("b003", "mp4").
fn partes(nome: &str) -> Result<(&str, &str)> {
    match nome.rsplit_once('.') {
        Some((a, b)) if nome_simples(nome) && !a.is_empty() && !b.is_empty() => Ok((a, b)),
        _ => Err(erro(format!("{}: saída {nome}", codigo::ITEM_INVALIDO))),
    }
}

struct Relogio<'p> {
    cb: &'p mut dyn FnMut(Progresso),
    lim: Limitador,
    total_q: u64,
}

impl Relogio<'_> {
    fn avisar(&mut self, fase: Fase, feito_q: u64, qps: f32) {
        if !self.lim.passa() {
            return;
        }
        let falta = self.total_q.saturating_sub(feito_q) as f32;
        let restante_s = if qps > 0.0 { (falta / qps).ceil().min(u32::MAX as f32) as u32 } else { 0 };
        (self.cb)(Progresso { fase, feito_q: feito_q.min(self.total_q), total_q: self.total_q, qps, restante_s });
    }
}

/// Extensão dos pts lidos de um bloco no cache (`blocos/<chave>.lidos.json`).
const LIDOS: &str = "lidos.json";

/// O que um bloco rodado devolve.
struct Rodado {
    feito: BlocoFeito,
    passos: Vec<Termino>,
    lidos: Vec<(String, Vec<i64>)>,
}

struct Execucao<'a> {
    plano: &'a Plano,
    amb: &'a Ambiente,
    cancelar: &'a AtomicBool,
    dir_blocos: PathBuf,
    passos: Vec<Termino>,
}

/// Roda o plano até o arquivo final na pasta de trabalho. Cancelar não deixa saída nem bloco truncado no cache.
/// `progresso` é chamado até 5 vezes por segundo.
pub fn executar(plano: &Plano, amb: &Ambiente, cancelar: &AtomicBool, progresso: &mut dyn FnMut(Progresso)) -> Result<Resultado> {
    let dir_blocos = amb.cache.join(if matches!(plano.alvo, Alvo::Quadro { .. }) { "quadro" } else { "blocos" });
    let mut ex = Execucao { plano, amb, cancelar, dir_blocos, passos: Vec::new() };
    let mut relogio = Relogio { cb: progresso, lim: Limitador::default(), total_q: plano.total_q };
    let r = ex.rodar(&mut relogio);
    if r.is_err() {
        ex.limpar_parcial();
    }
    r
}

/// `executar` com a queda de GPU (§5.7): qualquer falha num bloco de GPU refaz a exportação INTEIRA pelo plano
/// de CPU (`compilar(true)`), com um codificador só, e grava o motivo em `queda_para_cpu`.
/// Devolve o plano que de fato gerou o arquivo.
pub(crate) fn executar_com_queda(
    compilar: &dyn Fn(bool) -> Result<Plano>,
    amb: &Ambiente,
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(Progresso),
) -> Result<(Plano, Resultado)> {
    let plano = compilar(false)?;
    match executar(&plano, amb, cancelar, progresso) {
        Err(e) if codigo_de(&e) == Some(ERRO_GPU) => {
            let motivo = match &e {
                SicroError::Validation(m) => m.split_once(": ").map(|(_, d)| d).unwrap_or(m).to_string(),
                _ => String::new(),
            };
            tracing::warn!("edição: queda de GPU para CPU: {motivo}");
            let cpu = compilar(true)?;
            if cpu.blocos.iter().any(bloco_gpu) {
                return Err(erro(format!("{}: plano de CPU com codificador de GPU", codigo::ITEM_INVALIDO)));
            }
            let mut r = executar(&cpu, amb, cancelar, progresso)?;
            r.queda_para_cpu = Some(motivo);
            Ok((cpu, r))
        }
        r => r.map(|res| (plano, res)),
    }
}

impl Execucao<'_> {
    fn trabalho(&self) -> &Path {
        &self.amb.trabalho
    }

    fn cancelado(&self) -> bool {
        self.cancelar.load(Ordering::SeqCst)
    }

    fn checar_cancelado(&self) -> Result<()> {
        if self.cancelado() {
            return Err(erro(ERRO_CANCELADO));
        }
        Ok(())
    }

    fn comando(&self, nome: &str, args: Vec<String>, log: &str) -> Comando {
        Comando {
            nome: nome.to_string(),
            programa: self.amb.cap.binario.clone(),
            args,
            pasta: self.trabalho().to_path_buf(),
            ambiente: Vec::new(),
            sem_ambiente: Vec::new(),
            stderr: self.trabalho().join(log),
        }
    }

    /// Roda um passo sequencial e exige código 0.
    fn passo(&mut self, cmd: &Comando) -> Result<Termino> {
        let t = exec::rodar(cmd, self.cancelar)?;
        self.passos.push(t.clone());
        exigir(cmd, &t)?;
        Ok(t)
    }

    /// `passo` que avisa a fase enquanto roda: o aviso único do começo quase sempre cai no intervalo do limitador
    /// e a tela ficava em "Blocos" durante o áudio inteiro.
    fn passo_avisando(&mut self, cmd: &Comando, rel: &mut Relogio, fase: Fase) -> Result<Termino> {
        let total = self.plano.total_q;
        let t = exec::rodar_com_progresso(cmd, self.cancelar, &mut |_| rel.avisar(fase, total, 0.0))?;
        self.passos.push(t.clone());
        exigir(cmd, &t)?;
        Ok(t)
    }

    fn arquivos_do_bloco(&self, b: &Bloco) -> Result<(PathBuf, Option<PathBuf>)> {
        let (_, ext) = partes(&b.saida)?;
        if !sha_valido(&b.chave) {
            return Err(erro(format!("{}: chave do bloco {}", codigo::ITEM_INVALIDO, b.indice)));
        }
        let v = self.dir_blocos.join(format!("{}.{ext}", b.chave));
        let h = (!b.framehash.is_empty()).then(|| self.dir_blocos.join(format!("{}.framehash", b.chave)));
        Ok((v, h))
    }

    /// O bloco do cache só vale inteiro: vídeo não vazio, `framehash` com `n_q` quadros e, na exportação final,
    /// os pts lidos guardados com ele.
    fn em_cache(&self, b: &Bloco) -> Result<bool> {
        let (v, h) = self.arquivos_do_bloco(b)?;
        if !fs::metadata(&v).is_ok_and(|m| m.is_file() && m.len() > 0) {
            return Ok(false);
        }
        if let Some(h) = h {
            let n = fs::read_to_string(&h).map(|t| conferir::coluna_de_hash(&t).len()).unwrap_or(usize::MAX);
            if n != b.n_q as usize {
                return Ok(false);
            }
        }
        Ok(!self.guarda_lidos(b) || self.lidos_do_cache(b).is_some())
    }

    /// Exportação final com pts a conferir: os pts lidos vão para o cache com o bloco (alimentam o `mapa.csv`
    /// também quando o bloco é reaproveitado).
    fn guarda_lidos(&self, b: &Bloco) -> bool {
        matches!(self.plano.alvo, Alvo::Final { .. }) && b.conferir.iter().any(|c| !c.pts.is_empty())
    }

    fn arquivo_dos_lidos(&self, b: &Bloco) -> PathBuf {
        self.dir_blocos.join(format!("{}.{LIDOS}", b.chave))
    }

    /// pts lidos quando o bloco foi renderizado; None se faltam ou não são os que este plano espera.
    fn lidos_do_cache(&self, b: &Bloco) -> Option<Vec<(String, Vec<i64>)>> {
        let lidos: Vec<(String, Vec<i64>)> = serde_json::from_slice(&fs::read(self.arquivo_dos_lidos(b)).ok()?).ok()?;
        let esperados = b.conferir.iter().filter(|c| !c.pts.is_empty());
        esperados.map(|c| (&c.clipe, &c.pts)).eq(lidos.iter().map(|(c, p)| (c, p))).then_some(lidos)
    }

    fn rodar(&mut self, rel: &mut Relogio) -> Result<Resultado> {
        let plano = self.plano;
        if plano.blocos.is_empty() || plano.total_q == 0 {
            return Err(erro(format!("{}: plano sem blocos", codigo::ITEM_INVALIDO)));
        }
        let nomes: HashSet<&str> = plano.blocos.iter().map(|b| codificador_de(&b.codificador).unwrap_or("")).collect();
        if nomes.len() > 1 {
            return Err(erro(format!("{}: mais de um codificador na exportação", codigo::ITEM_INVALIDO)));
        }
        fs::create_dir_all(self.trabalho())?;
        fs::create_dir_all(&self.dir_blocos)?;
        // o que esta tarefa põe no cache: a limpeza por projeto (exportação com ocultar) acha por aqui
        let chaves = serde_json::json!({
            "dir": self.dir_blocos.file_name().map(|n| n.to_string_lossy().into_owned()),
            "blocos": plano.blocos.iter().map(|b| &b.chave).collect::<Vec<_>>(),
            "camadas": plano.camadas.iter().map(camada::hash_do_pedido).collect::<Vec<_>>(),
        });
        {
            // com a trava da limpeza: ou as chaves desta tarefa já estão no disco quando ela limpa (e ficam), ou a
            // limpeza acabou antes e o que saiu é refeito
            let _g = trava_da_limpeza();
            fs::write(self.trabalho().join(CHAVES), serde_json::to_vec(&chaves)?)?;
        }

        // prévias (o áudio cita os auxiliares dele já na medição)
        rel.avisar(Fase::Previas, 0, 0.0);
        if let Some(a) = &plano.audio {
            for aux in &a.auxiliares {
                args::gravar_texto(self.trabalho(), &aux.nome, &aux.conteudo)?;
            }
        }
        let (loudnorm, trf) = self.previas()?;
        let ganho = match (&loudnorm, &plano.audio) {
            (Some(m), Some(a)) => audio::ganho_db(m, a.normalizar.as_ref()),
            _ => None,
        };

        // camadas: só as dos blocos que não estão no cache
        self.checar_cancelado()?;
        rel.avisar(Fase::Camadas, 0, 0.0);
        let mut pendentes = Vec::new();
        for b in &plano.blocos {
            if !self.em_cache(b)? {
                pendentes.push(b);
            }
        }
        let prontas = self.camadas(&pendentes, rel)?;

        // blocos
        let (feitos, lidos) = self.blocos(&pendentes, &prontas, rel)?;

        // áudio
        self.checar_cancelado()?;
        let mut audio_sha256 = None;
        if let Some(a) = &plano.audio {
            rel.avisar(Fase::Audio, plano.total_q, 0.0);
            audio_sha256 = Some(self.audio(a, ganho, rel)?);
        }

        // junção
        self.checar_cancelado()?;
        let arquivo = match &plano.juncao {
            Juncao::Direta => {
                if plano.blocos.len() != 1 {
                    return Err(erro(format!("{}: junção direta com {} blocos", codigo::ITEM_INVALIDO, plano.blocos.len())));
                }
                self.arquivos_do_bloco(&plano.blocos[0])?.0
            }
            Juncao::Concat { lista, args: a, extensao } => {
                rel.avisar(Fase::Juncao, plano.total_q, 0.0);
                self.juntar(lista, a, extensao, rel)?
            }
        };
        let sha256 = crate::hashing::sha256::sha256_file(&arquivo)?;
        let tamanho = fs::metadata(&arquivo)?.len();

        // conferência do arquivo entregue
        let mut conferencia = None;
        if matches!(plano.alvo, Alvo::Final { .. }) {
            self.checar_cancelado()?;
            rel.avisar(Fase::Conferindo, 0, 0.0);
            let t0 = Instant::now();
            let r = conferir::conferir(&self.amb.cap, &arquivo, plano.total_q, self.trabalho(), self.cancelar, &mut |q| {
                rel.avisar(Fase::Conferindo, q, q as f32 / t0.elapsed().as_secs_f32().max(1e-3))
            });
            match r {
                Ok(c) => conferencia = Some(c),
                Err(e) if !self.cancelado() && falha_de_gpu(&e, plano.blocos.iter().any(bloco_gpu)) => {
                    self.apagar_blocos_gpu();
                    return Err(erro(format!("{ERRO_GPU}: {}", fundo::texto_do_erro(&e))));
                }
                Err(e) => return Err(e),
            }
        }

        // composto: coluna de hash dos framehash dos blocos, na ordem
        let (mut composto_sha256, mut compostos) = (None, None);
        if plano.blocos.iter().all(|b| !b.framehash.is_empty()) {
            let fhs = plano.blocos.iter().map(|b| self.arquivos_do_bloco(b).map(|(_, h)| h.unwrap_or_default())).collect::<Result<Vec<_>>>()?;
            let destino = self.trabalho().join("quadros.compostos.sha256");
            composto_sha256 = Some(conferir::hash_do_composto(&fhs, &destino)?);
            compostos = Some(destino);
        }

        let mut legendas = Vec::new();
        for l in &plano.legendas {
            args::gravar_texto(self.trabalho(), &l.nome, &l.conteudo)?;
            legendas.push(self.trabalho().join(&l.nome));
        }

        let mut camadas: Vec<ArquivoCamada> = prontas
            .values()
            .flat_map(|c| {
                let base = c.pasta.strip_prefix(&self.amb.cache).unwrap_or(&c.pasta).to_string_lossy().replace('\\', "/");
                c.arquivos.iter().map(move |a| ArquivoCamada { arquivo: format!("{base}/{}", a.arquivo), sha256: a.sha256.clone() })
            })
            .collect();
        camadas.sort_by(|a, b| a.arquivo.cmp(&b.arquivo));
        let cod = &plano.blocos[0].codificador;
        Ok(Resultado {
            arquivo,
            sha256,
            tamanho,
            codificador: codificador_de(cod).unwrap_or("").to_string(),
            argumentos: cod.clone(),
            queda_para_cpu: None,
            composto_sha256,
            compostos,
            audio_sha256,
            loudnorm,
            ganho_db: ganho,
            blocos: feitos,
            camadas,
            passos: std::mem::take(&mut self.passos),
            conferencia,
            lidos,
            legendas,
            trf,
            renderizado_em: agora(),
        })
    }

    fn previas(&mut self) -> Result<(Option<Loudnorm>, Vec<TrfNovo>)> {
        let mut loudnorm = None;
        let mut trf = Vec::new();
        // `.trf` guardado no projeto (ou no retrato, no "refazer"): o grafo do bloco cita `<sha256>.trf` na pasta de trabalho
        super::motor::efeitos::realce::trazer_trf(&self.plano.proveniencia.estabilizacoes, &self.amb.pasta_projeto, self.trabalho())?;
        for (i, p) in self.plano.previas.iter().enumerate() {
            self.checar_cancelado()?;
            match p {
                Previa::Loudnorm { args } => {
                    let cmd = self.comando("loudnorm", args.clone(), &format!("previa{i:03}.log"));
                    self.passo(&cmd)?;
                    loudnorm = Some(audio::ler_loudnorm(&String::from_utf8_lossy(&fs::read(&cmd.stderr)?))?);
                }
                Previa::Vidstab { efeito, args, trf: nome } => {
                    if !nome_simples(nome) {
                        return Err(erro(format!("{}: trf {nome}", codigo::ITEM_INVALIDO)));
                    }
                    let mut cmd = self.comando("vidstab", args.clone(), &format!("previa{i:03}.log"));
                    // sem isso 3 execuções dão 3 `.trf` (§5.3)
                    cmd.ambiente.push(("OMP_NUM_THREADS".into(), "1".into()));
                    self.passo(&cmd)?;
                    let sha256 = guardar_trf(&self.amb.pasta_projeto, &fs::read(self.trabalho().join(nome))?)?;
                    let log = String::from_utf8_lossy(&fs::read(&cmd.stderr)?).into_owned();
                    trf.push(TrfNovo { efeito: efeito.clone(), sha256, zoom_pct: zoom_final(&log) });
                }
            }
        }
        Ok((loudnorm, trf))
    }

    fn camadas(&mut self, pendentes: &[&Bloco], rel: &mut Relogio) -> Result<HashMap<String, CamadaPronta>> {
        let mut ids = HashSet::new();
        for b in pendentes {
            for e in &b.entradas {
                if let Papel::Camada { id } = &e.papel {
                    if !self.plano.camadas.iter().any(|c| &c.id == id) {
                        return Err(erro(format!("{}: camada {id} fora do plano", codigo::ITEM_INVALIDO)));
                    }
                    ids.insert(id.as_str());
                }
            }
        }
        let pedidos: Vec<_> = self.plano.camadas.iter().filter(|c| ids.contains(c.id.as_str())).cloned().collect();
        if pedidos.is_empty() {
            return Ok(HashMap::new());
        }
        let prontas = camada::rasterizar_todos(&pedidos, &self.amb.cache, self.cancelar, &mut |_| rel.avisar(Fase::Camadas, 0, 0.0))
        .map_err(|e| if self.cancelado() { erro(ERRO_CANCELADO) } else { e })?;
        Ok(prontas.into_iter().map(|c| (c.id.clone(), c)).collect())
    }

    /// Roda os blocos que não estão no cache, até `paralelos` ao mesmo tempo, e devolve todos em ordem.
    fn blocos(
        &mut self,
        pendentes: &[&Bloco],
        prontas: &HashMap<String, CamadaPronta>,
        rel: &mut Relogio,
    ) -> Result<(Vec<BlocoFeito>, Vec<LidosClipe>)> {
        let plano = self.plano;
        let total = plano.blocos.len() as u32;
        let pend: HashSet<usize> = pendentes.iter().map(|b| b.indice).collect();
        let feitos_q_cache: u64 = plano.blocos.iter().filter(|b| !pend.contains(&b.indice)).map(|b| b.n_q as u64).sum();
        let parar = AtomicBool::new(false);
        let proximo = AtomicUsize::new(0);
        let quadros: Vec<AtomicU64> = pendentes.iter().map(|_| AtomicU64::new(0)).collect();
        let prontos = AtomicUsize::new(0);
        let n_trab = (self.amb.paralelos.clamp(1, 2) as usize).min(pendentes.len());
        let vivos = AtomicUsize::new(n_trab);
        let resultados: Mutex<Vec<Option<Result<Rodado>>>> = Mutex::new((0..pendentes.len()).map(|_| None).collect());
        let t0 = Instant::now();
        let este = &*self;
        std::thread::scope(|s| {
            for _ in 0..n_trab {
                s.spawn(|| {
                    // conta a saída mesmo em pânico: a vigia espera `vivos` chegar a zero
                    struct Saiu<'v>(&'v AtomicUsize);
                    impl Drop for Saiu<'_> {
                        fn drop(&mut self) {
                            self.0.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                    let _saiu = Saiu(&vivos);
                    loop {
                        let i = proximo.fetch_add(1, Ordering::SeqCst);
                        if i >= pendentes.len() || parar.load(Ordering::SeqCst) {
                            break;
                        }
                        let r = este.rodar_bloco(pendentes[i], prontas, &parar, &quadros[i]);
                        if r.is_err() {
                            parar.store(true, Ordering::SeqCst);
                        }
                        resultados.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(r);
                        prontos.fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
            // vigia: cancelamento de fora e progresso, só nesta thread
            loop {
                if self.cancelar.load(Ordering::SeqCst) {
                    parar.store(true, Ordering::SeqCst);
                }
                let feitos_agora = prontos.load(Ordering::SeqCst);
                let rodados: u64 =
                    pendentes.iter().zip(&quadros).map(|(b, q)| q.load(Ordering::Relaxed).min(b.n_q as u64)).sum();
                let qps = rodados as f32 / t0.elapsed().as_secs_f32().max(1e-3);
                let fase = Fase::Blocos { feitos: total - pendentes.len() as u32 + feitos_agora as u32, total };
                rel.avisar(fase, feitos_q_cache + rodados, qps);
                if vivos.load(Ordering::SeqCst) == 0 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });

        let resultados = resultados.into_inner().unwrap_or_else(|e| e.into_inner());
        let mut rodados: HashMap<usize, Rodado> = HashMap::new();
        let mut falha: Option<(usize, SicroError)> = None;
        for (i, r) in resultados.into_iter().enumerate() {
            match r {
                Some(Ok(rd)) => {
                    self.passos.extend(rd.passos.iter().cloned());
                    rodados.insert(pendentes[i].indice, rd);
                }
                Some(Err(e)) => {
                    // o bloco morto porque outro falhou não é a causa
                    let induzido = codigo_de(&e) == Some(ERRO_CANCELADO);
                    if falha.is_none() || (!induzido && falha.as_ref().is_some_and(|(_, f)| codigo_de(f) == Some(ERRO_CANCELADO))) {
                        falha = Some((i, e));
                    }
                }
                None => {}
            }
        }
        if self.cancelado() {
            return Err(erro(ERRO_CANCELADO));
        }
        if let Some((i, e)) = falha {
            if falha_de_gpu(&e, bloco_gpu(pendentes[i])) {
                self.apagar_blocos_gpu();
                return Err(erro(format!("{ERRO_GPU}: {}", fundo::texto_do_erro(&e))));
            }
            return Err(e);
        }

        let mut feitos = Vec::with_capacity(plano.blocos.len());
        let mut lidos: Vec<LidosClipe> = Vec::new();
        for b in &plano.blocos {
            match rodados.remove(&b.indice) {
                Some(rd) => {
                    for (clipe, pts) in rd.lidos {
                        match lidos.iter_mut().find(|l| l.clipe == clipe) {
                            Some(l) => l.pts.extend(pts),
                            None => lidos.push(LidosClipe { clipe, pts }),
                        }
                    }
                    feitos.push(rd.feito);
                }
                None => {
                    let (v, _) = self.arquivos_do_bloco(b)?;
                    if self.guarda_lidos(b) {
                        let guardados = self.lidos_do_cache(b).ok_or_else(|| erro(format!("{}: pts lidos do bloco {}", codigo::ITEM_INVALIDO, b.indice)))?;
                        for (clipe, pts) in guardados {
                            match lidos.iter_mut().find(|l| l.clipe == clipe) {
                                Some(l) => l.pts.extend(pts),
                                None => lidos.push(LidosClipe { clipe, pts }),
                            }
                        }
                    }
                    feitos.push(BlocoFeito {
                        indice: b.indice,
                        de_q: b.de_q,
                        n_q: b.n_q,
                        chave: b.chave.clone(),
                        reaproveitado: true,
                        sha256: crate::hashing::sha256::sha256_file(&v)?,
                    });
                }
            }
        }
        Ok((feitos, lidos))
    }

    /// Um bloco: grafo e auxiliares na pasta de trabalho, ffmpeg, conferência por execução (refeito sem `-ss` se
    /// divergir) e troca de nome para o cache só no fim.
    fn rodar_bloco(&self, b: &Bloco, prontas: &HashMap<String, CamadaPronta>, parar: &AtomicBool, quadros: &AtomicU64) -> Result<Rodado> {
        let (base, ext) = partes(&b.saida)?;
        let (destino, destino_fh) = self.arquivos_do_bloco(b)?;
        let grafo = format!("{base}.txt");
        args::gravar_texto(self.trabalho(), &grafo, &b.grafo)?;
        for aux in &b.auxiliares {
            args::gravar_texto(self.trabalho(), &aux.nome, &aux.conteudo)?;
        }
        let tmp = format!("{base}.tmp.{ext}");
        let tmp_fh = format!("{base}.tmp.framehash");
        let final_ = matches!(self.plano.alvo, Alvo::Final { .. });
        let gpu_vaapi = codificador_de(&b.codificador).is_some_and(|c| c.ends_with("_vaapi"));

        let tentar = |sem_busca: bool| -> Result<(Termino, Option<String>)> {
            let mut a = args::globais();
            for (k, e) in b.entradas.iter().enumerate() {
                let antes = match (&e.antes_sem_busca, sem_busca) {
                    (Some(s), true) => s,
                    _ => &e.antes,
                };
                a.extend(antes.iter().cloned());
                a.push("-i".into());
                a.push(self.caminho_da_entrada(e, k, b, prontas)?.to_string_lossy().into_owned());
            }
            a.extend(args::grafo_em_arquivo(&self.amb.cap, &grafo));
            a.extend(["-map".to_string(), "[v]".to_string()]);
            a.extend(b.codificador.iter().cloned());
            a.push(tmp.clone());
            if !b.framehash.is_empty() {
                a.extend(["-map", "[h]", "-f", "framehash", "-hash", "sha256"].iter().map(|s| s.to_string()));
                a.push(tmp_fh.clone());
            }
            let (nome, log) = if sem_busca { (format!("{base} sem busca"), format!("{base}.sem_busca.log")) } else { (base.to_string(), format!("{base}.log")) };
            let mut cmd = self.comando(&nome, a, &log);
            if gpu_vaapi {
                cmd.sem_ambiente.push("LIBVA_DRIVER_NAME".into());
            }
            for f in [&tmp, &tmp_fh] {
                let _ = fs::remove_file(self.trabalho().join(f));
            }
            quadros.store(0, Ordering::Relaxed);
            let t = exec::rodar_com_progresso(&cmd, parar, &mut |p| quadros.store(p.quadro, Ordering::Relaxed))?;
            exigir(&cmd, &t)?;
            let fh = (!b.framehash.is_empty()).then(|| self.trabalho().join(&tmp_fh));
            let d = divergencia(b, &cmd.stderr, fh.as_deref())?;
            Ok((t, d))
        };

        let mut passos = Vec::new();
        let (t, mut div) = tentar(false)?;
        passos.push(t);
        let mut log = self.trabalho().join(format!("{base}.log"));
        if div.is_some() && b.entradas.iter().any(|e| e.antes_sem_busca.is_some()) {
            tracing::warn!("edição: bloco {} divergiu ({}); refeito sem busca", b.indice, div.as_deref().unwrap_or(""));
            let (t, d) = tentar(true)?;
            passos.push(t);
            div = d;
            log = self.trabalho().join(format!("{base}.sem_busca.log"));
        }
        if let Some(d) = div {
            return Err(erro(format!("{ERRO_LEITURA_DIVERGENTE}: bloco {}: {d}", b.indice)));
        }
        let lidos: Vec<(String, Vec<i64>)> = if final_ {
            let pts = exec::pts_lidos(&log)?;
            b.conferir
                .iter()
                .filter(|c| !c.pts.is_empty())
                .map(|c| (c.clipe.clone(), pts.get(&c.rotulo).cloned().unwrap_or_default()))
                .collect()
        } else {
            Vec::new()
        };
        // pts lidos e framehash antes do vídeo: o vídeo no cache sempre tem os dois ao lado
        if self.guarda_lidos(b) {
            let tmp_l = self.trabalho().join(format!("{base}.tmp.{LIDOS}"));
            fs::write(&tmp_l, serde_json::to_vec(&lidos)?)?;
            fs::rename(&tmp_l, self.arquivo_dos_lidos(b))?;
        }
        if let Some(h) = &destino_fh {
            fs::rename(self.trabalho().join(&tmp_fh), h)?;
        }
        let tmp_abs = self.trabalho().join(&tmp);
        let sha256 = crate::hashing::sha256::sha256_file(&tmp_abs)?;
        fs::rename(&tmp_abs, &destino)?;
        Ok(Rodado {
            feito: BlocoFeito { indice: b.indice, de_q: b.de_q, n_q: b.n_q, chave: b.chave.clone(), reaproveitado: false, sha256 },
            passos,
            lidos,
        })
    }

    /// Arquivo do cache pelo caminho relativo à pasta de trabalho (a do processo). O concat resolve as entradas da lista
    /// contra o caminho dela como URL: `#` ou `?` numa pasta acima (nome de usuário) cortava o caminho ali.
    fn relativo_ao_trabalho(&self, alvo: &Path) -> PathBuf {
        let (Ok(t), Ok(a)) = (self.trabalho().strip_prefix(&self.amb.cache), alvo.strip_prefix(&self.amb.cache)) else { return alvo.to_path_buf() };
        if !t.components().chain(a.components()).all(|c| matches!(c, std::path::Component::Normal(_))) {
            return alvo.to_path_buf();
        }
        t.components().map(|_| std::path::Component::ParentDir).collect::<PathBuf>().join(a)
    }

    /// Caminho de cada `-i`. Camada: a lista rasterizada; camada fixa (um PNG só) lida por `-f concat` ganha uma
    /// lista própria com uma entrada por quadro; com `-loop` vai o PNG.
    fn caminho_da_entrada(&self, e: &Entrada, k: usize, b: &Bloco, prontas: &HashMap<String, CamadaPronta>) -> Result<PathBuf> {
        let Papel::Camada { id } = &e.papel else { return Ok(e.caminho.clone()) };
        let c = prontas.get(id).ok_or_else(|| erro(format!("{}: camada {id} não rasterizada", codigo::ITEM_INVALIDO)))?;
        if let Some(l) = &c.lista {
            return Ok(self.relativo_ao_trabalho(&c.pasta.join(l)));
        }
        let png = c.pasta.join(&c.arquivos.first().ok_or_else(|| erro(format!("{}: camada {id} vazia", codigo::ITEM_INVALIDO)))?.arquivo);
        if !e.antes.iter().any(|a| a == "concat") {
            return Ok(png);
        }
        let n = self.plano.camadas.iter().find(|p| &p.id == id).map(|p| p.n).unwrap_or(1);
        let nome = format!("b{:03}_e{k:03}_fixa.ffconcat", b.indice);
        let caminho = self.trabalho().join(&nome);
        let mut w = BufWriter::new(fs::File::create(&caminho)?);
        w.write_all(b"ffconcat version 1.0\n")?;
        let linha = format!("file '{}'\n", args::escapar_concat(&png.to_string_lossy()));
        for _ in 0..n {
            w.write_all(linha.as_bytes())?;
        }
        w.flush()?;
        Ok(caminho)
    }

    fn audio(&mut self, a: &super::motor::plano::PassoAudio, ganho: Option<f64>, rel: &mut Relogio) -> Result<String> {
        for n in [&a.saida, &a.hash] {
            if !nome_simples(n) {
                return Err(erro(format!("{}: áudio {n}", codigo::ITEM_INVALIDO)));
            }
        }
        // sem marca não há o que trocar
        let grafo = if a.grafo.contains(MARCA_GANHO) { audio::grafo_com_ganho(a, ganho) } else { a.grafo.clone() };
        args::gravar_texto(self.trabalho(), "audio.txt", &grafo)?;
        let mut v = a.globais.clone();
        for e in &a.entradas {
            v.extend(e.antes.iter().cloned());
            v.push("-i".into());
            v.push(e.caminho.to_string_lossy().into_owned());
        }
        v.extend(args::grafo_em_arquivo(&self.amb.cap, "audio.txt"));
        v.extend(["-map", "[a]", "-c:a", "pcm_s16le"].iter().map(|s| s.to_string()));
        v.push(a.saida.clone());
        v.extend(["-map", "[ah]", "-f", "hash", "-hash", "sha256"].iter().map(|s| s.to_string()));
        v.push(a.hash.clone());
        for n in [&a.saida, &a.hash] {
            let _ = fs::remove_file(self.trabalho().join(n));
        }
        let cmd = self.comando("audio", v, "audio.log");
        self.passo_avisando(&cmd, rel, Fase::Audio)?;
        let amostras = amostras_wav(&self.trabalho().join(&a.saida))?;
        if amostras != a.amostras {
            return Err(erro(format!("{ERRO_LEITURA_DIVERGENTE}: áudio com {amostras} amostras de {}", a.amostras)));
        }
        let texto = fs::read_to_string(self.trabalho().join(&a.hash))?;
        texto
            .trim()
            .split_once('=')
            .map(|(_, h)| h.trim().to_lowercase())
            .filter(|h| sha_valido(h))
            .ok_or_else(|| erro(format!("{}: hash do áudio", codigo::AUDIO)))
    }

    fn juntar(&mut self, lista: &super::motor::plano::ArquivoTexto, extra: &[String], extensao: &str, rel: &mut Relogio) -> Result<PathBuf> {
        // os blocos vão para a pasta de trabalho com o nome que a lista cita
        for b in &self.plano.blocos {
            let (origem, _) = self.arquivos_do_bloco(b)?;
            let alvo = self.trabalho().join(&b.saida);
            let _ = fs::remove_file(&alvo);
            if fs::hard_link(&origem, &alvo).is_err() {
                fs::copy(&origem, &alvo)?;
            }
        }
        args::gravar_texto(self.trabalho(), &lista.nome, &lista.conteudo)?;
        let saida = format!("saida.{extensao}");
        if !nome_simples(&saida) {
            return Err(erro(format!("{}: extensão {extensao}", codigo::ITEM_INVALIDO)));
        }
        let mut v: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-nostats", "-loglevel", "verbose", "-f", "concat", "-safe", "0", "-i"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        v.push(lista.nome.clone());
        if let Some(a) = &self.plano.audio {
            v.push("-i".into());
            v.push(a.saida.clone());
        }
        v.extend(extra.iter().cloned());
        v.push(saida.clone());
        let cmd = self.comando("juncao", v, "juncao.log");
        self.passo_avisando(&cmd, rel, Fase::Juncao)?;
        // as cópias ligadas dos blocos seguram o espaço que a limpeza do cache libera
        for b in &self.plano.blocos {
            let _ = fs::remove_file(self.trabalho().join(&b.saida));
        }
        Ok(self.trabalho().join(saida))
    }

    fn apagar_blocos_gpu(&self) {
        for b in self.plano.blocos.iter().filter(|b| bloco_gpu(b)) {
            if let Ok((v, h)) = self.arquivos_do_bloco(b) {
                let _ = fs::remove_file(self.arquivo_dos_lidos(b));
                let _ = fs::remove_file(v);
                if let Some(h) = h {
                    let _ = fs::remove_file(h);
                }
            }
        }
    }

    /// Saída parcial: temporários dos blocos e a saída da junção (blocos prontos ficam no cache).
    fn limpar_parcial(&self) {
        let Ok(rd) = fs::read_dir(self.trabalho()) else { return };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.contains(".tmp.") || n.starts_with("saida.") {
                let _ = fs::remove_file(e.path());
            }
        }
    }
}

/// Palavras das linhas de erro do ffmpeg: com `-loglevel verbose` as últimas linhas são estatística, não o motivo.
const SINAIS_DE_ERRO: &[&str] = &[
    "rror", "Requested", "Invalid", "invalid", "Unknown", "Unrecognized", "not found", "No such", "ailed", "Cannot", "Could not",
    "nsupported", "No capable", "out of memory", "No space", "Conversion", "Impossible",
];

/// `Ok` com código 0; senão o erro classificado com as linhas de erro do stderr (as últimas 3), ou `cancelado`.
fn exigir(cmd: &Comando, t: &Termino) -> Result<()> {
    if t.cancelado {
        return Err(erro(format!("{ERRO_CANCELADO}: {}", cmd.nome)));
    }
    if t.codigo == Some(0) {
        return Ok(());
    }
    let todas = exec::ultimas_linhas(&cmd.stderr, 400);
    let mut linhas: Vec<String> = todas.iter().filter(|l| SINAIS_DE_ERRO.iter().any(|s| l.contains(s))).cloned().collect();
    if linhas.is_empty() {
        linhas = todas;
    }
    let linhas = linhas[linhas.len().saturating_sub(3)..].to_vec();
    let classe = serde_json::to_value(exec::classificar(&linhas)).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    Err(erro(format!("{classe}: {} (código {:?}): {}", cmd.nome, t.codigo, linhas.join(" | "))))
}

/// Conferência por execução de um bloco (§5.3): `frames in` de cada `fps@` (ÚLTIMA linha) = `lidos` + 1 (o clone do
/// último quadro que a cadeia do clipe põe sempre, contrato de `ConferirClipe`); pts do `showinfo@` = os esperados
/// quando o plano os traz; e o `framehash` com `n_q` quadros. None = confere.
fn divergencia(b: &Bloco, stderr: &Path, framehash: Option<&Path>) -> Result<Option<String>> {
    let lidos = exec::quadros_lidos(stderr)?;
    for c in &b.conferir {
        let n = lidos.get(&c.rotulo).copied();
        if n != Some(c.lidos as u64 + 1) {
            let lido = n.map_or("nenhum".into(), |n| n.to_string());
            return Ok(Some(format!("{}: frames in {lido}, esperados {} + 1", c.rotulo, c.lidos)));
        }
    }
    if b.conferir.iter().any(|c| !c.pts.is_empty()) {
        let pts = exec::pts_lidos(stderr)?;
        for c in b.conferir.iter().filter(|c| !c.pts.is_empty()) {
            let lido = pts.get(&c.rotulo).map(Vec::as_slice).unwrap_or(&[]);
            if lido != c.pts.as_slice() {
                let i = lido.iter().zip(&c.pts).position(|(a, b)| a != b).unwrap_or(lido.len().min(c.pts.len()));
                return Ok(Some(format!("{}: pts lidos diferem no quadro {i} ({} lidos, {} esperados)", c.rotulo, lido.len(), c.pts.len())));
            }
        }
    }
    if let Some(fh) = framehash {
        let n = conferir::coluna_de_hash(&fs::read_to_string(fh)?).len();
        if n != b.n_q as usize {
            return Ok(Some(format!("{n} quadros compostos, esperados {}", b.n_q)));
        }
    }
    Ok(None)
}

/// Último "Final zoom: <x>" do log do vidstab (0 sem ele).
fn zoom_final(log: &str) -> f64 {
    log.lines()
        .rev()
        .filter_map(|l| l.split_once("Final zoom:").map(|(_, r)| r.trim()))
        .find_map(|r| r.split_whitespace().next().and_then(|v| v.trim_end_matches('%').parse::<f64>().ok()))
        .unwrap_or(0.0)
}

/// Amostras (por canal) do `data` de um WAV PCM s16.
fn amostras_wav(arquivo: &Path) -> Result<u64> {
    let mut f = fs::File::open(arquivo)?;
    let mut cab = [0u8; 12];
    f.read_exact(&mut cab)?;
    if &cab[0..4] != b"RIFF" || &cab[8..12] != b"WAVE" {
        return Err(erro(format!("{}: wav inválido", codigo::AUDIO)));
    }
    let mut canais = 0u64;
    let mut bits = 0u64;
    // poucos pedaços antes do `data` (fmt, LIST); o teto evita laço em arquivo estranho
    for _ in 0..64 {
        let mut c = [0u8; 8];
        f.read_exact(&mut c)?;
        let tam = u32::from_le_bytes([c[4], c[5], c[6], c[7]]) as u64;
        match &c[0..4] {
            b"fmt " => {
                let mut fmt = vec![0u8; tam.min(64) as usize];
                f.read_exact(&mut fmt)?;
                if fmt.len() < 16 {
                    break;
                }
                canais = u16::from_le_bytes([fmt[2], fmt[3]]) as u64;
                bits = u16::from_le_bytes([fmt[14], fmt[15]]) as u64;
                let resto = tam.saturating_sub(fmt.len() as u64) + (tam & 1);
                std::io::copy(&mut (&mut f).take(resto), &mut std::io::sink())?;
            }
            b"data" => {
                let bloco = canais * bits / 8;
                if bloco == 0 {
                    break;
                }
                return Ok(tam / bloco);
            }
            _ => {
                std::io::copy(&mut (&mut f).take(tam + (tam & 1)), &mut std::io::sink())?;
            }
        }
    }
    Err(erro(format!("{}: wav sem dados", codigo::AUDIO)))
}

// ---------------------------------------------------------------------------------------------- limpeza do cache

/// Pastas do cache de um caso que entram na conta do teto (as tabelas de quadros ficam: são pequenas e caras).
const PASTAS_DO_TETO: &[&str] = &["blocos", "quadro", "camadas", "mascaras", "previa", "proxies", "origem", "sprites", "atlas"];
/// `render/<tarefa>` mais velho que isso sai (§4).
pub(crate) const IDADE_RENDER: Duration = Duration::from_secs(7 * 24 * 3600);
/// Temporário de processo morto (`<hash>.tmp-<pid>-<n>`) mais velho que isso sai.
const IDADE_TMP: Duration = Duration::from_secs(6 * 3600);
/// Teto padrão do cache da Edição (§4).
pub(crate) const TETO_CACHE: u64 = 20 * 1024 * 1024 * 1024;

/// Pasta das marcas de instância do app dentro do cache da Edição.
const INSTANCIAS: &str = "instancias";
/// Pastas do cache de um caso com temporários `<nome>.tmp-<pid>-<n>` de processo morto.
const PASTAS_COM_TMP: &[&str] = &["previa", "proxies", "camadas", "mascaras", "origem", "ondas"];

/// Há outra instância viva do app neste cache? Cada processo segura uma marca presa por trava de arquivo em
/// `<raiz>/instancias/` até acabar; a marca que se deixa prender é de processo morto e sai. Sem trava no sistema de
/// arquivos a resposta é "há": na dúvida, nada de outra instância é apagado.
fn outra_instancia(raiz: &Path) -> bool {
    static MINHAS: Mutex<Vec<(PathBuf, PathBuf, fs::File)>> = Mutex::new(Vec::new());
    let dir = raiz.join(INSTANCIAS);
    if fs::create_dir_all(&dir).is_err() {
        return true;
    }
    let mut minhas = MINHAS.lock().unwrap_or_else(|e| e.into_inner());
    if !minhas.iter().any(|(r, _, _)| r == raiz) {
        // a marca só aparece com o nome final já presa: ninguém a vê solta enquanto nasce
        let nome = format!("{}-{}", std::process::id(), uuid::Uuid::new_v4());
        let (tmp, fim) = (dir.join(format!("{nome}.nascendo")), dir.join(format!("{nome}.trava")));
        let presa = fs::OpenOptions::new().read(true).write(true).create_new(true).open(&tmp).ok().filter(|a| a.try_lock().is_ok());
        match presa {
            Some(a) if fs::rename(&tmp, &fim).is_ok() => minhas.push((raiz.to_path_buf(), fim, a)),
            _ => {
                let _ = fs::remove_file(&tmp);
                return true;
            }
        }
    }
    let minha = minhas.iter().find(|(r, _, _)| r == raiz).map(|(_, m, _)| m.clone());
    let mut outra = false;
    for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let c = e.path();
        if Some(&c) == minha.as_ref() || c.extension().and_then(|x| x.to_str()) != Some("trava") {
            continue;
        }
        match fs::OpenOptions::new().read(true).write(true).open(&c) {
            Ok(a) if a.try_lock().is_ok() => {
                drop(a);
                let _ = fs::remove_file(&c);
            }
            _ => outra = true,
        }
    }
    outra
}

/// `render/<tarefa>` de exportação interrompida (processo morto): saem os temporários grandes (saída, PCM, blocos
/// ligados, registros) e ficam o id do projeto e as chaves do cache, por onde a limpeza por projeto acha os blocos.
/// Devolve os bytes liberados.
fn enxugar_render(d: &Path) -> u64 {
    let mut liberado = 0;
    for e in fs::read_dir(d).into_iter().flatten().flatten() {
        let n = e.file_name();
        if n == super::fila::RETRATO_TAREFA || n == CHAVES {
            continue;
        }
        liberado += tamanho_de(&e.path());
        remover(&e.path());
    }
    let retrato = d.join(super::fila::RETRATO_TAREFA);
    let id = fs::read(&retrato).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()).and_then(|v| v.get("id").and_then(|i| i.as_str()).map(String::from));
    if let (Some(id), Ok(m)) = (id, fs::metadata(&retrato)) {
        let curto = serde_json::json!({ "id": id }).to_string();
        if m.len() > curto.len() as u64 && fs::write(&retrato, &curto).is_ok() {
            liberado += m.len() - curto.len() as u64;
        }
    }
    liberado
}

/// Limpeza do cache da Edição (`raiz` = `app_cache_dir()/edicao`): temporários de processo morto, `render/<tarefa>`
/// com mais de 7 dias (salvo `vivas`), o de exportação interrompida (enxugado) e, acima do `teto`, os arquivos mais
/// antigos. Devolve os bytes liberados.
pub(crate) fn limpar_cache(raiz: &Path, teto: u64, vivas: &[String]) -> u64 {
    // o que é mais novo que este processo pode ser dele (tarefa posta na fila enquanto a limpeza da partida roda)
    static PARTIDA: OnceLock<SystemTime> = OnceLock::new();
    limpar_cache_em(raiz, teto, vivas, *PARTIDA.get_or_init(SystemTime::now))
}

/// `limpar_cache` com o instante de partida do processo dado (testes). Com outra instância do app viva no mesmo
/// cache, o `render/<tarefa>` que não é desta só sai pela idade, e o teto não tira arquivo mais novo que `IDADE_TMP`
/// (pode ser bloco que a outra acabou de fazer e ainda vai juntar).
pub(crate) fn limpar_cache_em(raiz: &Path, teto: u64, vivas: &[String], partida: SystemTime) -> u64 {
    let agora = SystemTime::now();
    let outra = outra_instancia(raiz);
    let velho = |p: &Path, idade: Duration| {
        fs::metadata(p).and_then(|m| m.modified()).is_ok_and(|d| agora.duration_since(d).is_ok_and(|i| i > idade))
    };
    let mut liberado = 0u64;
    let mut candidatos: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let mut total = 0u64;
    let Ok(casos) = fs::read_dir(raiz) else { return 0 };
    for caso in casos.flatten().filter(|e| e.path().is_dir() && e.file_name() != INSTANCIAS) {
        let caso = caso.path();
        for sub in PASTAS_COM_TMP {
            for e in fs::read_dir(caso.join(sub)).into_iter().flatten().flatten() {
                let nome = e.file_name().to_string_lossy().into_owned();
                // `<hash>.tmp-<pid>-<n>`: o deste processo pode estar em uso
                let deste = nome.split_once(".tmp-").and_then(|(_, r)| r.split('-').next()) == Some(&std::process::id().to_string());
                if nome.contains(".tmp-") && !deste && velho(&e.path(), IDADE_TMP) {
                    liberado += tamanho_de(&e.path());
                    remover(&e.path());
                }
            }
        }
        for e in fs::read_dir(caso.join("render")).into_iter().flatten().flatten() {
            let nome = e.file_name().to_string_lossy().into_owned();
            if vivas.contains(&nome) {
                continue;
            }
            if velho(&e.path(), IDADE_RENDER) {
                liberado += tamanho_de(&e.path());
                remover(&e.path());
            } else if !outra && mais_novo(&e.path()).is_some_and(|d| d < partida) {
                // nenhuma outra instância e parada desde antes deste processo: ninguém mais a usa
                liberado += enxugar_render(&e.path());
            }
        }
        for sub in PASTAS_DO_TETO {
            for e in fs::read_dir(caso.join(sub)).into_iter().flatten().flatten() {
                if e.file_name().to_string_lossy().contains(".tmp") {
                    continue;
                }
                let t = tamanho_de(&e.path());
                let d = e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
                total += t;
                if outra && agora.duration_since(d).is_ok_and(|i| i < IDADE_TMP) {
                    continue;
                }
                candidatos.push((d, t, e.path()));
            }
        }
    }
    candidatos.sort();
    for (_, t, p) in candidatos {
        if total <= teto {
            break;
        }
        remover(&p);
        total -= t;
        liberado += t;
    }
    liberado
}

/// Pastas de trabalho que a Edição cria na pasta temporária do sistema (tabela do decodificador, sonda), com o pid
/// no nome: `sicro-quadros-<pid>-<n>`, `sicro-sonda-<nome>-<pid>-<n>`.
const TEMPORARIAS_DO_SISTEMA: &[&str] = &["sicro-quadros-", "sicro-sonda-"];

/// Na partida: pastas de trabalho de processo morto (kill, queda de energia) na pasta temporária do sistema. O stderr
/// da tabela do decodificador tem ~300 B por quadro (centenas de MB numa gravação de 24 h, em `/tmp` na memória).
/// Sai a de outro pid parada há mais de `IDADE_TMP` (o arquivo mais novo dentro dela decide: outra instância do app
/// pode estar escrevendo). Devolve os bytes liberados.
pub(crate) fn limpar_temporarias(temp: &Path) -> u64 {
    let meu = std::process::id().to_string();
    let agora = SystemTime::now();
    let mut liberado = 0;
    for e in fs::read_dir(temp).into_iter().flatten().flatten() {
        let nome = e.file_name().to_string_lossy().into_owned();
        let Some(resto) = TEMPORARIAS_DO_SISTEMA.iter().find_map(|p| nome.strip_prefix(p)) else { continue };
        // o pid é o penúltimo pedaço do nome
        let pid = resto.rsplit('-').nth(1);
        if pid.is_none_or(|p| p == meu || p.parse::<u32>().is_err()) || !e.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let parada = mais_novo(&e.path()).and_then(|d| agora.duration_since(d).ok()).is_some_and(|i| i > IDADE_TMP);
        if parada {
            liberado += tamanho_de(&e.path());
            remover(&e.path());
        }
    }
    liberado
}

/// Data de modificação mais nova da pasta e do que há nela (um nível: as pastas de trabalho são planas).
fn mais_novo(p: &Path) -> Option<SystemTime> {
    let da_pasta = fs::metadata(p).and_then(|m| m.modified()).ok();
    fs::read_dir(p).into_iter().flatten().flatten().filter_map(|e| e.metadata().and_then(|m| m.modified()).ok()).chain(da_pasta).max()
}

/// Chaves que uma tarefa pôs no cache, em `render/<tarefa>/`.
pub(crate) const CHAVES: &str = "chaves.json";

/// Ao concluir exportação com `ocultar` (§4): saem os blocos, camadas, quadros finais e prévias do render de todas
/// as tarefas do projeto (o que foi renderizado antes da ocultação não fica no cache). `exportacao` = a que limpa (sai
/// o cache, fica a pasta); `intocadas` = tarefas vivas que entraram depois da parada das prévias.
pub(crate) fn limpar_do_projeto(cache: &Path, projeto_id: &str, exportacao: &str, intocadas: &[String]) {
    let _g = trava_da_limpeza();
    // o que as vivas usam fica, mesmo quando uma tarefa antiga do projeto usou a mesma chave
    let vivas: Vec<String> = intocadas.iter().filter(|t| *t != exportacao).cloned().collect();
    let (blocos_vivos, camadas_vivas) = chaves_das_vivas(cache, &vivas);
    for e in fs::read_dir(cache.join("render")).into_iter().flatten().flatten() {
        let d = e.path();
        let deste = [d.join(super::fila::RETRATO_TAREFA), d.join("projeto").join(super::fila::RETRATO_TAREFA)]
            .iter()
            .filter_map(|a| fs::read(a).ok())
            .filter_map(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .any(|v| v.get("id").and_then(serde_json::Value::as_str) == Some(projeto_id));
        let nome = e.file_name().to_string_lossy().into_owned();
        if !deste || (nome != exportacao && intocadas.contains(&nome)) {
            continue;
        }
        if let Some(c) = fs::read(d.join(CHAVES)).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) {
            let lista = |k: &str| -> Vec<String> {
                c.get(k).and_then(|v| v.as_array()).into_iter().flatten().filter_map(|x| x.as_str()).filter(|x| sha_valido(x)).map(String::from).collect()
            };
            let dir = match c.get("dir").and_then(|v| v.as_str()) {
                Some("quadro") => "quadro",
                _ => "blocos",
            };
            for chave in lista("blocos").into_iter().filter(|c| !blocos_vivos.contains(c)) {
                for ext in fs::read_dir(cache.join(dir)).into_iter().flatten().flatten() {
                    if ext.file_name().to_string_lossy().split('.').next() == Some(chave.as_str()) {
                        remover(&ext.path());
                    }
                }
            }
            for h in lista("camadas").into_iter().filter(|h| !camadas_vivas.contains(h)) {
                for sub in ["camadas", "mascaras"] {
                    remover(&cache.join(sub).join(&h));
                }
            }
        }
        if nome != exportacao {
            remover(&d);
        }
    }
}

/// Uma limpeza por plano de cada vez, e nunca no meio da gravação das chaves de uma tarefa.
fn trava_da_limpeza() -> std::sync::MutexGuard<'static, ()> {
    static T: Mutex<()> = Mutex::new(());
    T.lock().unwrap_or_else(|e| e.into_inner())
}

/// Blocos e camadas que as tarefas `vivas` puseram (ou vão usar) no cache: o `chaves.json` de `render/<tarefa>/`.
fn chaves_das_vivas(cache: &Path, vivas: &[String]) -> (HashSet<String>, HashSet<String>) {
    let (mut blocos, mut camadas) = (HashSet::new(), HashSet::new());
    for t in vivas {
        let Some(c) = fs::read(cache.join("render").join(t).join(CHAVES)).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) else { continue };
        let lista = |k: &str| c.get(k).and_then(|v| v.as_array()).into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect::<Vec<_>>();
        blocos.extend(lista("blocos"));
        camadas.extend(lista("camadas"));
    }
    (blocos, camadas)
}

/// Ao concluir exportação com `ocultar`: saem os blocos e as camadas daquele plano (renderizados antes da
/// ocultação, não podem ficar no cache). `vivas` = tarefas que entraram depois da parada das prévias: o que elas
/// usam fica (a prévia nova, já com a ocultação, falhava por arquivo sumido no meio do render).
// CONTRATO: 3º parâmetro acrescentado no fecho da Fase 1 (contratos-fase1 (g) L12).
pub(crate) fn limpar_do_plano(cache: &Path, plano: &Plano, vivas: &[String]) {
    let _g = trava_da_limpeza();
    let (blocos_vivos, camadas_vivas) = chaves_das_vivas(cache, vivas);
    for b in &plano.blocos {
        if !sha_valido(&b.chave) || blocos_vivos.contains(&b.chave) {
            continue;
        }
        for dir in ["blocos", "quadro"] {
            for e in fs::read_dir(cache.join(dir)).into_iter().flatten().flatten() {
                if e.file_name().to_string_lossy().starts_with(&b.chave) {
                    remover(&e.path());
                }
            }
        }
    }
    for c in &plano.camadas {
        let h = camada::hash_do_pedido(c);
        if camadas_vivas.contains(&h) {
            continue;
        }
        for dir in ["camadas", "mascaras"] {
            remover(&cache.join(dir).join(&h));
        }
    }
}

fn tamanho_de(p: &Path) -> u64 {
    match fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => fs::read_dir(p).into_iter().flatten().flatten().map(|e| tamanho_de(&e.path())).sum(),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

fn remover(p: &Path) {
    let _ = if p.is_dir() { fs::remove_dir_all(p) } else { fs::remove_file(p) };
}


/// Planos montados à mão para os testes do executor e da fila (o compilador é de outro pacote).
#[cfg(test)]
pub(crate) mod apoio {
    use super::*;
    use crate::edicao::ffmpeg::sonda::Origem;
    use crate::edicao::modelo::{EscolhaCodificador, Exportacao, ModoLegendas, ModoPdf, Predefinicao, Racional};
    use crate::edicao::motor::plano::{
        ArquivoTexto, Canvas, ConferirClipe, PassoAudio, ProvAudio, ProvFfmpeg, ProvLegendas, ProvProjeto, ProvSaida, Proveniencia,
    };

    fn textos(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// "Arquivo reprodutível": x264 com threads fixas e sem nada que mude entre execuções.
    pub fn cpu() -> Vec<String> {
        textos(&[
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-g", "30", "-bf", "0", "-threads", "2", "-fflags", "+bitexact",
            "-flags:v", "+bitexact", "-map_metadata", "-1", "-video_track_timescale", "30",
        ])
    }

    /// Codificador de GPU que sempre falha: placa inexistente no 9.0.1, codificador ausente no 7.0.2.
    pub fn gpu_que_falha() -> Vec<String> {
        textos(&["-c:v", "h264_nvenc", "-gpu", "57", "-fflags", "+bitexact", "-map_metadata", "-1", "-video_track_timescale", "30"])
    }

    pub fn alvo_final(codificador: EscolhaCodificador) -> Alvo {
        Alvo::Final {
            exportacao: Exportacao {
                predefinicao: Predefinicao::Juri,
                codificador,
                legendas: ModoLegendas::Nenhuma,
                pacote: None,
                pdf: ModoPdf::A3,
                revisado_por: String::new(),
                personalizada: None,
            },
        }
    }

    pub fn prov(cap: &Capacidades, total_q: u64) -> Proveniencia {
        Proveniencia {
            projeto: ProvProjeto { id: "p1".into(), titulo: "Teste".into(), esquema: 2, esquema_menor: 0 },
            saida: ProvSaida {
                largura: 320,
                altura: 180,
                fps: Racional { num: 30, den: 1 },
                duracao_s: total_q as f64 / 30.0,
                n_quadros: total_q,
                predefinicao: Some(Predefinicao::Juri),
                com_perdas: true,
            },
            midias: vec![],
            roteiro: vec![],
            ocultacoes: vec![],
            audio: ProvAudio { silencios: vec![], tolerancia_atempo_ms: 0.0, mixagem: None, trilhas: vec![] },
            legendas: ProvLegendas { origem: None, n: 0, conferidas: 0, modo: ModoLegendas::Nenhuma, no_projeto: 0, ia: 0, ia_conferidas: 0 },
            sincronias: vec![],
            rastreios: vec![],
            ia: vec![],
            estabilizacoes: vec![],
            avisos: vec![],
            selos_desligados: vec![],
            revisado_por: String::new(),
            ffmpeg: ProvFfmpeg { versao: cap.versao.clone(), configuracao: String::new(), origem: Origem::Path, sha256: cap.sha256.clone() },
            fecho: None,
            sobrepostos: vec![],
        }
    }

    /// Bloco `i`: quadros `j_in..j_in+n` da fonte (pts da tabela), um slot por quadro, `showinfo@` e `fps@` com rótulo,
    /// e o clone do último quadro antes do `settb` (regra única da cadeia do clipe, contrato de `ConferirClipe`).
    /// `antes` = argumentos antes do `-i` (busca); `antes_sem_busca` Some = o executor pode refazer sem ela.
    #[allow(clippy::too_many_arguments)]
    pub fn bloco(
        cap: &Capacidades,
        fonte: &Path,
        pts: &[i64],
        i: usize,
        j_in: usize,
        n: usize,
        cod: &[String],
        antes: Vec<String>,
        antes_sem_busca: Option<Vec<String>>,
    ) -> Bloco {
        let rot = format!("c{i:03}");
        let fim = pts.get(j_in + n).map(|p| format!(":end_pts={p}")).unwrap_or_default();
        let grafo = format!(
            "[0:v]trim=start_pts={}{fim},showinfo@{rot}=checksum=0,setpts=PTS-STARTPTS,tpad=stop_mode=clone:stop=1,settb=1/30,setpts=N,\
             fps@{rot}=30:start_time=0,trim=end_frame={n},format=yuv420p,split[v][h]",
            pts[j_in]
        );
        let chave = sha256_bytes(format!("{grafo}\n{cod:?}\n{antes:?}\n{}\n{}", cap.sha256, fonte.display()).as_bytes());
        Bloco {
            indice: i,
            de_q: (i * n) as u64,
            n_q: n as u32,
            chave,
            entradas: vec![Entrada { antes, antes_sem_busca, caminho: fonte.to_path_buf(), papel: Papel::Midia { id: "m1".into(), clipe: format!("k{i}") } }],
            grafo,
            auxiliares: vec![],
            codificador: cod.to_vec(),
            saida: format!("b{i:03}.mp4"),
            framehash: format!("b{i:03}.framehash"),
            conferir: vec![ConferirClipe { rotulo: rot, clipe: format!("k{i}"), lidos: n as u32, pts: pts[j_in..j_in + n].to_vec() }],
        }
    }

    pub fn juncao(blocos: &[Bloco], com_audio: bool) -> Juncao {
        let mut lista = String::from("ffconcat version 1.0\n");
        for b in blocos {
            lista.push_str(&format!("file '{}'\n", b.saida));
        }
        let mut args = textos(&["-map", "0:v", "-c:v", "copy"]);
        if com_audio {
            args.extend(textos(&["-map", "1:a", "-c:a", "aac", "-b:a", "192k", "-flags:a", "+bitexact"]));
        }
        args.extend(textos(&["-movflags", "+faststart", "-fflags", "+bitexact", "-map_metadata", "-1", "-video_track_timescale", "30"]));
        Juncao::Concat { lista: ArquivoTexto { nome: "blocos.ffconcat".into(), conteudo: lista }, args, extensao: "mp4".into() }
    }

    /// Áudio do programa: tom de 440 Hz, `amostras` a 48 kHz, estéreo.
    pub fn audio(amostras: u64) -> PassoAudio {
        PassoAudio {
            globais: textos(&["-cpuflags", "0", "-hide_banner", "-nostdin", "-y", "-nostats", "-loglevel", "verbose"]),
            entradas: vec![Entrada {
                antes: textos(&["-f", "lavfi"]),
                antes_sem_busca: None,
                caminho: PathBuf::from(format!("sine=frequency=440:sample_rate=48000:duration={}", amostras / 48_000 + 1)),
                papel: Papel::Gerada,
            }],
            grafo: format!("[0:a]aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo,atrim=end_sample={amostras},asplit[a][ah]"),
            auxiliares: vec![],
            saida: "audio.wav".into(),
            hash: "audio.sha256".into(),
            amostras,
            normalizar: None,
            tolerancia_atempo_ms: 0.0,
        }
    }

    /// Três blocos de 30 quadros (0..90 da fonte), áudio e junção, exportação final.
    pub fn plano(cap: &Capacidades, fonte: &Path, pts: &[i64], cod: &[String], alvo: Alvo) -> Plano {
        let blocos: Vec<Bloco> = (0..3).map(|i| bloco(cap, fonte, pts, i, i * 30, 30, cod, vec![], None)).collect();
        Plano {
            alvo,
            canvas: Canvas { w: 320, h: 180, fps: Racional { num: 30, den: 1 }, cor_fundo: "#000000".into() },
            total_q: 90,
            previas: vec![],
            camadas: vec![],
            juncao: juncao(&blocos, true),
            blocos,
            audio: Some(audio(144_000)),
            legendas: vec![],
            proveniencia: prov(cap, 90),
        }
    }

    pub fn ambiente(cap: Arc<Capacidades>, raiz: &Path, tarefa: &str) -> Ambiente {
        Ambiente {
            cap,
            trabalho: raiz.join("cache").join("render").join(tarefa),
            cache: raiz.join("cache"),
            caso: raiz.join("caso"),
            pasta_projeto: raiz.join("caso").join("projeto"),
            paralelos: 2,
            identidade: Identidade::default(),
            ciencias: vec![],
        }
    }

    /// Caso de verdade (manifesto e banco) numa pasta temporária, com espaço e acento no caminho.
    pub fn caso() -> (tempfile::TempDir, PathBuf) {
        let d = pasta("caso");
        let pai = d.path().join("Casos de teste ç");
        fs::create_dir_all(&pai).unwrap();
        let criado = crate::workspace::create::create_workspace(
            crate::models::NewOccurrenceInput {
                titulo: Some("Caso da Edição".into()),
                numero_bo: None,
                protocolo: Some("123/26".into()),
                oficio: None,
                tipo_pericia: None,
                municipio: None,
                peritos: vec!["Perito Teste".into()],
                parent_directory: Some(pai.to_string_lossy().into()),
            },
            &pai,
        )
        .unwrap();
        (d, criado.path)
    }

    pub fn pasta(nome: &str) -> tempfile::TempDir {
        tempfile::Builder::new().prefix(&format!("p07-{nome}-")).tempdir().expect("pasta temporária")
    }

    /// Nenhuma janela de 1 s com mais de 5 instantes.
    pub fn no_maximo_5_por_segundo(v: &[Instant]) {
        for (i, t) in v.iter().enumerate() {
            let n = v[i..].iter().filter(|u| u.duration_since(*t) < Duration::from_secs(1)).count();
            assert!(n <= 5, "{n} avisos de progresso em 1 s");
        }
    }
}

#[cfg(test)]
mod testes {
    use super::apoio::*;
    use super::*;
    use crate::edicao::modelo::EscolhaCodificador;
    use crate::edicao::testes::fontes::{self, Fonte};
    use crate::edicao::testes::medir;

    /// Fonte da matriz (MP4 30 qps, base 1/15360) e os pts do decodificador.
    fn fonte(cap: &Capacidades) -> (PathBuf, Vec<i64>) {
        let f = fontes::gerar(Fonte::Mp4).expect("fonte");
        let pts = medir::pts(cap, &f).expect("pts");
        assert!(pts.len() >= 120, "{}", pts.len());
        (f, pts)
    }

    fn arquivos(p: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    /// O mesmo `.trf` gravado ao mesmo tempo por várias tarefas (exportação e intermediários): toda gravação dá certo,
    /// o arquivo fica inteiro e não sobra temporário.
    #[test]
    fn mesmo_trf_gravado_ao_mesmo_tempo() {
        let pasta = fontes::pasta_temporaria("executar-trf");
        let bytes: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        for volta in 0..20 {
            let _ = fs::remove_dir_all(pasta.join("vidstab"));
            let erros: Vec<String> = std::thread::scope(|s| {
                let hs: Vec<_> = (0..8).map(|_| s.spawn(|| guardar_trf(&pasta, &bytes))).collect();
                hs.into_iter().filter_map(|h| h.join().unwrap().err().map(|e| format!("volta {volta}: {e}"))).collect()
            });
            assert!(erros.is_empty(), "{erros:?}");
            assert_eq!(arquivos(&pasta.join("vidstab")), vec![format!("{}.trf", sha256_bytes(&bytes))]);
            assert_eq!(fs::read(pasta.join("vidstab").join(format!("{}.trf", sha256_bytes(&bytes)))).unwrap(), bytes);
        }
    }

    /// Exportação de 3 blocos: contagem, quadro de origem em todos os slots, pixel e cor, composto = quadros da fonte,
    /// conferência do arquivo; repetir com outro cache dá o mesmo SHA-256; com o mesmo cache não roda bloco nenhum.
    #[test]
    fn tres_blocos_exportam_conferem_e_repetem() {
        let Some(cap) = fontes::cap() else { return };
        let (f, pts) = fonte(&cap);
        let plano = plano(&cap, &f, &pts, &cpu(), alvo_final(EscolhaCodificador::Cpu));
        let d = pasta("tres-blocos");
        let nao = AtomicBool::new(false);
        let amb1 = ambiente(cap.clone(), d.path(), "t1");
        let mut instantes = Vec::new();
        let r1 = executar(&plano, &amb1, &nao, &mut |_| instantes.push(Instant::now())).unwrap();
        no_maximo_5_por_segundo(&instantes);
        let v = cap.versao.as_str();

        assert_eq!(r1.codificador, "libx264");
        assert_eq!(r1.blocos.iter().map(|b| (b.indice, b.reaproveitado)).collect::<Vec<_>>(), [(0, false), (1, false), (2, false)]);
        assert_eq!(r1.sha256, crate::hashing::sha256::sha256_file(&r1.arquivo).unwrap());
        let c = r1.conferencia.as_ref().expect("conferência");
        assert_eq!((c.quadros, c.erros), (90, 0), "{v}");
        assert_eq!(c.decodificado_sha256, conferir::hash_de_conteudo(&medir::framehash(&cap, &r1.arquivo).unwrap()));
        // composto = os 90 primeiros quadros da fonte, sem conversão nenhuma
        let fonte_h = medir::framehash(&cap, &f).unwrap();
        assert_eq!(r1.composto_sha256.as_deref(), Some(conferir::hash_de_conteudo(&fonte_h[..90]).as_str()), "{v}");
        assert_eq!(fs::read_to_string(r1.compostos.as_ref().unwrap()).unwrap().lines().count(), 90);
        // quadro de origem de cada slot, e pixel e cor (marca vermelha) contra a fonte
        let origem = medir::quadros_de_origem(&cap, &r1.arquivo).unwrap();
        assert_eq!(origem, (0..90).collect::<Vec<u32>>(), "{v}");
        for q in [0u32, 29, 30, 45, 89] {
            let (a, b) = (medir::quadro(&cap, &f, q).unwrap(), medir::quadro(&cap, &r1.arquivo, q).unwrap());
            let dif = medir::pixels_diferentes(&a, &b, 24);
            assert!(dif * 100 < (a.w * a.h) as u64, "{v} quadro {q}: {dif} px diferentes");
            let (ma, mb) = (medir::marca(&a, [255, 0, 0], 80).unwrap(), medir::marca(&b, [255, 0, 0], 80).unwrap());
            assert!(ma.iter().zip(mb).all(|(x, y)| (*x as i64 - y as i64).abs() <= 2), "{v} quadro {q}: marca {ma:?} × {mb:?}");
        }
        // pts lidos por clipe = os da tabela
        assert_eq!(r1.lidos.len(), 3);
        for (i, l) in r1.lidos.iter().enumerate() {
            assert_eq!((l.clipe.as_str(), l.pts.as_slice()), (format!("k{i}").as_str(), &pts[i * 30..i * 30 + 30]));
        }
        assert_eq!(amostras_wav(&amb1.trabalho.join("audio.wav")).unwrap(), 144_000);
        assert!(r1.audio_sha256.as_ref().is_some_and(|h| sha_valido(h)));
        // o AAC arredonda para o quadro de 1024 amostras
        let a = medir::amostras(&cap, &r1.arquivo).unwrap();
        assert!(a.abs_diff(144_000) <= 1024, "{v}: {a} amostras no arquivo");

        // outro cache: tudo de novo, mesmo arquivo (mesma classe de máquina)
        let d2 = pasta("tres-blocos-2");
        let r2 = executar(&plano, &ambiente(cap.clone(), d2.path(), "t2"), &nao, &mut |_| {}).unwrap();
        assert!(r2.blocos.iter().all(|b| !b.reaproveitado));
        assert_eq!((&r2.sha256, &r2.composto_sha256, &r2.audio_sha256), (&r1.sha256, &r1.composto_sha256, &r1.audio_sha256), "{v}");
        assert_eq!(r2.conferencia.unwrap().decodificado_sha256, c.decodificado_sha256);

        // mesmo cache: nenhum ffmpeg de bloco roda
        let r3 = executar(&plano, &ambiente(cap.clone(), d.path(), "t3"), &nao, &mut |_| {}).unwrap();
        assert!(r3.blocos.iter().all(|b| b.reaproveitado), "{:?}", r3.blocos);
        assert!(r3.passos.iter().all(|t| !t.nome.starts_with('b')), "{:?}", r3.passos.iter().map(|t| &t.nome).collect::<Vec<_>>());
        assert_eq!(r3.blocos.iter().map(|b| &b.sha256).collect::<Vec<_>>(), r1.blocos.iter().map(|b| &b.sha256).collect::<Vec<_>>());
        assert_eq!((&r3.sha256, &r3.composto_sha256), (&r1.sha256, &r1.composto_sha256));
        // os pts lidos vêm do cache com o bloco: o relatório da exportação repetida não fica sem eles
        assert_eq!(r3.lidos, r1.lidos);

        // bloco do cache com o framehash truncado, ou sem os pts lidos, não vale: é refeito
        let fh = amb1.cache.join("blocos").join(format!("{}.framehash", plano.blocos[1].chave));
        let texto = fs::read_to_string(&fh).unwrap();
        fs::write(&fh, texto.trim_end().rsplit_once('\n').unwrap().0).unwrap();
        fs::remove_file(amb1.cache.join("blocos").join(format!("{}.{LIDOS}", plano.blocos[2].chave))).unwrap();
        let r4 = executar(&plano, &ambiente(cap.clone(), d.path(), "t4"), &nao, &mut |_| {}).unwrap();
        assert_eq!(r4.blocos.iter().map(|b| b.reaproveitado).collect::<Vec<_>>(), [true, false, false]);
        assert_eq!((&r4.sha256, &r4.composto_sha256, &r4.lidos), (&r1.sha256, &r1.composto_sha256, &r1.lidos));
        // vídeo de 0 bytes no cache também não vale
        fs::write(amb1.cache.join("blocos").join(format!("{}.mp4", plano.blocos[0].chave)), b"").unwrap();
        let r5 = executar(&plano, &ambiente(cap.clone(), d.path(), "t5"), &nao, &mut |_| {}).unwrap();
        assert_eq!(r5.blocos.iter().map(|b| b.reaproveitado).collect::<Vec<_>>(), [false, true, true]);
        assert_eq!(r5.sha256, r1.sha256);
        // o que sobra na pasta de trabalho: nada temporário
        assert!(arquivos(&amb1.trabalho).iter().all(|n| !n.contains(".tmp.")), "{:?}", arquivos(&amb1.trabalho));
    }

    /// `lidos` errado de propósito: o bloco é refeito sem busca e, divergindo de novo, vira erro; nada dele no cache.
    #[test]
    fn frames_in_divergente_faz_o_bloco_falhar() {
        let Some(cap) = fontes::cap() else { return };
        let (f, pts) = fonte(&cap);
        let mut plano = plano(&cap, &f, &pts, &cpu(), alvo_final(EscolhaCodificador::Cpu));
        plano.blocos[1].conferir[0].lidos = 29;
        plano.blocos[1].entradas[0].antes_sem_busca = Some(vec![]);
        let d = pasta("divergente");
        let amb = ambiente(cap.clone(), d.path(), "t1");
        let e = executar(&plano, &amb, &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert_eq!(codigo_de(&e), Some(ERRO_LEITURA_DIVERGENTE), "{e}");
        assert!(e.to_string().contains("frames in 31, esperados 29 + 1"), "{e}");
        let t = arquivos(&amb.trabalho);
        assert!(t.contains(&"b001.sem_busca.log".to_string()), "refeito sem busca: {t:?}");
        assert!(t.iter().all(|n| !n.contains(".tmp.") && !n.starts_with("saida.")), "{t:?}");
        let cache = arquivos(&amb.cache.join("blocos"));
        assert!(cache.iter().all(|n| !n.starts_with(&plano.blocos[1].chave)), "{cache:?}");

        // pts esperados diferentes dos lidos: também falha
        let mut p2 = super::apoio::plano(&cap, &f, &pts, &cpu(), alvo_final(EscolhaCodificador::Cpu));
        p2.blocos[2].conferir[0].pts[5] += 1;
        let e = executar(&p2, &ambiente(cap.clone(), pasta("pts").path(), "t1"), &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert!(e.to_string().contains("pts lidos diferem no quadro 5"), "{e}");
    }

    /// Busca que pula quadros (alvo do `-ss` depois do quadro-chave): `frames in` diverge, o bloco é refeito sem
    /// busca e o arquivo sai certo.
    #[test]
    fn busca_errada_e_refeita_sem_busca() {
        let Some(cap) = fontes::cap() else { return };
        let (f, pts) = fonte(&cap);
        let mut plano = plano(&cap, &f, &pts, &cpu(), alvo_final(EscolhaCodificador::Cpu));
        // quadros 60..90 (2,0 s); -ss 2,2 descarta 60..65
        let b = bloco(&cap, &f, &pts, 2, 60, 30, &cpu(), vec!["-ss".into(), "2.2".into()], Some(vec![]));
        plano.blocos[2] = b;
        let d = pasta("busca");
        let amb = ambiente(cap.clone(), d.path(), "t1");
        let r = executar(&plano, &amb, &AtomicBool::new(false), &mut |_| {}).unwrap();
        let nomes: Vec<&str> = r.passos.iter().map(|t| t.nome.as_str()).collect();
        assert!(nomes.contains(&"b002") && nomes.contains(&"b002 sem busca"), "{nomes:?}");
        let lidos = exec::quadros_lidos(&amb.trabalho.join("b002.log")).unwrap();
        assert!(lidos["c002"] < 30, "a busca errada tinha de perder quadros: {lidos:?}");
        assert_eq!(medir::quadros_de_origem(&cap, &r.arquivo).unwrap(), (0..90).collect::<Vec<u32>>(), "{}", cap.versao);
        assert_eq!(r.lidos[2].pts, pts[60..90]);
    }

    /// Bloco longo de `testsrc2`, para cancelar no meio.
    fn bloco_longo(i: usize) -> Bloco {
        let n = 6000;
        let grafo = format!("[0:v]trim=end_frame={n},format=yuv420p,split[v][h]");
        Bloco {
            indice: i,
            de_q: (i * n) as u64,
            n_q: n as u32,
            chave: sha256_bytes(format!("longo {i} {grafo}").as_bytes()),
            entradas: vec![Entrada {
                antes: vec!["-f".into(), "lavfi".into()],
                antes_sem_busca: None,
                caminho: PathBuf::from(format!("testsrc2=s=640x360:r=30:d={}", n / 30 + 1)),
                papel: Papel::Gerada,
            }],
            grafo,
            auxiliares: vec![],
            codificador: ["-c:v", "libx264", "-preset", "slow", "-threads", "1"].iter().map(|s| s.to_string()).collect(),
            saida: format!("b{i:03}.mp4"),
            framehash: format!("b{i:03}.framehash"),
            conferir: vec![],
        }
    }

    /// ffmpeg vivo com a pasta de trabalho como `cwd` (só os deste teste).
    #[cfg(target_os = "linux")]
    fn ffmpeg_vivos_em(pasta: &Path) -> usize {
        let alvo = fs::canonicalize(pasta).unwrap_or_else(|_| pasta.to_path_buf());
        fs::read_dir("/proc")
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
            .filter(|e| fs::read_link(e.path().join("cwd")).is_ok_and(|c| c == alvo))
            .count()
    }

    /// Cancelar no meio: erro `cancelado` em menos de 1 s (× 3 de folga), nenhum temporário, nenhum bloco truncado
    /// no cache e nenhum processo sobrando.
    #[test]
    fn cancelar_no_meio_nao_deixa_nada() {
        const LIMITE_S: f64 = 1.0;
        const FOLGA: f64 = 3.0;
        let Some(cap) = fontes::cap() else { return };
        let mut plano = plano(&cap, Path::new("/nao/usada"), &[0; 200], &cpu(), alvo_final(EscolhaCodificador::Cpu));
        plano.blocos = (0..3).map(bloco_longo).collect();
        plano.total_q = 18_000;
        plano.audio = None;
        plano.juncao = juncao(&plano.blocos, false);
        let d = pasta("cancelar");
        let amb = ambiente(cap.clone(), d.path(), "t1");
        let cancelar = AtomicBool::new(false);
        let pedido = std::sync::Mutex::new(None::<Instant>);
        let r = std::thread::scope(|s| {
            s.spawn(|| {
                // espera o ffmpeg dos blocos aparecer
                let t0 = Instant::now();
                while !amb.trabalho.join("b000.log").is_file() && t0.elapsed() < Duration::from_secs(60) {
                    std::thread::sleep(Duration::from_millis(20));
                }
                std::thread::sleep(Duration::from_millis(1000));
                *pedido.lock().unwrap() = Some(Instant::now());
                cancelar.store(true, Ordering::SeqCst);
            });
            executar(&plano, &amb, &cancelar, &mut |_| {})
        });
        let levou = pedido.lock().unwrap().unwrap().elapsed().as_secs_f64();
        let e = r.unwrap_err();
        assert_eq!(codigo_de(&e), Some(ERRO_CANCELADO), "{e}");
        assert!(levou < LIMITE_S * FOLGA, "cancelar levou {levou:.3} s");
        eprintln!("cancelar: {levou:.3} s ({})", cap.versao);
        let t = arquivos(&amb.trabalho);
        assert!(t.iter().all(|n| !n.contains(".tmp.") && !n.starts_with("saida.")), "{t:?}");
        for n in arquivos(&amb.cache.join("blocos")) {
            // bloco no cache só inteiro, com o framehash ao lado
            if let Some(chave) = n.strip_suffix(".mp4") {
                let fh = fs::read_to_string(amb.cache.join("blocos").join(format!("{chave}.framehash"))).unwrap();
                assert_eq!(conferir::coluna_de_hash(&fh).len(), 6000);
            }
        }
        #[cfg(target_os = "linux")]
        assert_eq!(ffmpeg_vivos_em(&amb.trabalho), 0, "processo sobrando");
    }

    /// Falha de GPU forçada: a exportação inteira é refeita pelo plano de CPU (um codificador só) e a queda fica
    /// registrada; nenhum bloco de GPU fica no cache.
    #[test]
    fn falha_de_gpu_refaz_tudo_em_cpu() {
        let Some(cap) = fontes::cap() else { return };
        let (f, pts) = fonte(&cap);
        let d = pasta("gpu");
        let amb = ambiente(cap.clone(), d.path(), "t1");
        let gpu = plano(&cap, &f, &pts, &gpu_que_falha(), alvo_final(EscolhaCodificador::Auto));
        let cpu_ = plano(&cap, &f, &pts, &cpu(), alvo_final(EscolhaCodificador::Cpu));
        let chamadas = std::sync::Mutex::new(Vec::new());
        let compilar = |cpu: bool| {
            chamadas.lock().unwrap().push(cpu);
            Ok(if cpu { cpu_.clone() } else { gpu.clone() })
        };
        // um bloco de GPU já no cache (de uma exportação anterior que deu certo): tem de sair na queda
        let dir = amb.cache.join("blocos");
        fs::create_dir_all(&dir).unwrap();
        let b0 = &gpu.blocos[0];
        fs::write(dir.join(format!("{}.mp4", b0.chave)), b"bloco de gpu").unwrap();
        fs::write(dir.join(format!("{}.framehash", b0.chave)), "0, 0, 0, 1, 1, aa\n".repeat(30)).unwrap();
        fs::write(dir.join(format!("{}.{LIDOS}", b0.chave)), serde_json::to_vec(&[("k0", &pts[..30])]).unwrap()).unwrap();
        let (usado, r) = executar_com_queda(&compilar, &amb, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(*chamadas.lock().unwrap(), [false, true]);
        assert!(!amb.trabalho.join("b000.log").is_file() || !fs::read_to_string(amb.trabalho.join("b000.log")).unwrap().contains("h264_nvenc"), "o bloco 0 de GPU vinha do cache");
        assert_eq!(usado, cpu_);
        assert_eq!(r.codificador, "libx264");
        let motivo = r.queda_para_cpu.as_deref().expect("queda registrada");
        assert!(motivo.contains("b00"), "{motivo}");
        eprintln!("queda ({}): {motivo}", cap.versao);
        let cache = arquivos(&amb.cache.join("blocos"));
        assert!(gpu.blocos.iter().all(|b| cache.iter().all(|n| !n.starts_with(&b.chave))), "{cache:?}");
        assert_eq!(cache.len(), 9, "3 blocos de CPU, os framehash e os pts lidos: {cache:?}");
        assert_eq!(medir::quadros_de_origem(&cap, &r.arquivo).unwrap(), (0..90).collect::<Vec<u32>>());
        // o arquivo é o mesmo de uma exportação direto em CPU
        let direto = executar(&cpu_, &ambiente(cap.clone(), pasta("gpu-direto").path(), "t1"), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(direto.sha256, r.sha256);

        // plano de CPU que ainda traz GPU: recusado
        let compilar_ruim = |_: bool| Ok(gpu.clone());
        let e = executar_com_queda(&compilar_ruim, &ambiente(cap.clone(), pasta("gpu-ruim").path(), "t1"), &AtomicBool::new(false), &mut |_| {})
            .unwrap_err();
        assert!(e.to_string().contains("plano de CPU com codificador de GPU"), "{e}");
    }

    #[test]
    fn dois_codificadores_numa_exportacao_sao_recusados() {
        let Some(cap) = fontes::cap() else { return };
        let mut plano = plano(&cap, Path::new("/x"), &[0; 120], &cpu(), alvo_final(EscolhaCodificador::Cpu));
        plano.blocos[1].codificador = gpu_que_falha();
        let d = pasta("dois");
        let e = executar(&plano, &ambiente(cap.clone(), d.path(), "t1"), &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert!(e.to_string().contains("mais de um codificador"), "{e}");
    }

    #[test]
    fn camada_fixa_lida_por_concat_ganha_lista_propria() {
        let Some(cap) = fontes::cap() else { return };
        let d = pasta("fixa");
        let mut plano = plano(&cap, Path::new("/x"), &[0; 120], &cpu(), alvo_final(EscolhaCodificador::Cpu));
        let pedido = crate::edicao::motor::plano::PedidoCamada {
            id: "k001".into(),
            tipo: crate::edicao::motor::plano::TipoCamada::Anotacao,
            caixa_px: [0, 0, 2, 2],
            n: 4,
            hash: String::new(),
            itens: vec![],
            imagens: vec![],
        };
        plano.camadas = vec![pedido];
        let amb = ambiente(cap.clone(), d.path(), "t1");
        fs::create_dir_all(&amb.trabalho).unwrap();
        let ex = Execucao { plano: &plano, amb: &amb, cancelar: &AtomicBool::new(false), dir_blocos: amb.cache.join("blocos"), passos: vec![] };
        let png = d.path().join("cam d'a").join("000000.png");
        let pronta = CamadaPronta { id: "k001".into(), pasta: png.parent().unwrap().to_path_buf(), lista: None, fixa: true, arquivos: vec![ArquivoCamada { arquivo: "000000.png".into(), sha256: String::new() }] };
        let prontas: HashMap<String, CamadaPronta> = [("k001".to_string(), pronta.clone())].into();
        let concat = Entrada { antes: vec!["-f".into(), "concat".into(), "-safe".into(), "0".into()], antes_sem_busca: None, caminho: PathBuf::new(), papel: Papel::Camada { id: "k001".into() } };
        let lista = ex.caminho_da_entrada(&concat, 1, &plano.blocos[0], &prontas).unwrap();
        let texto = fs::read_to_string(&lista).unwrap();
        assert!(texto.starts_with("ffconcat version 1.0\n"));
        assert_eq!(texto.lines().filter(|l| l.starts_with("file ")).count(), 4);
        assert!(texto.contains(&args::escapar_concat(&png.to_string_lossy())));
        let laco = Entrada { antes: vec!["-loop".into(), "1".into()], ..concat.clone() };
        assert_eq!(ex.caminho_da_entrada(&laco, 1, &plano.blocos[0], &prontas).unwrap(), png);
        let com_lista = CamadaPronta { lista: Some(camada::LISTA.into()), fixa: false, ..pronta.clone() };
        let prontas: HashMap<String, CamadaPronta> = [("k001".to_string(), com_lista)].into();
        assert_eq!(ex.caminho_da_entrada(&concat, 1, &plano.blocos[0], &prontas).unwrap(), png.parent().unwrap().join(camada::LISTA));
        // camada no cache: a lista vai relativa à pasta de trabalho (nenhuma pasta de cima entra no caminho)
        let no_cache = CamadaPronta { pasta: amb.cache.join("camadas").join("abc"), lista: Some(camada::LISTA.into()), fixa: false, ..pronta.clone() };
        let prontas: HashMap<String, CamadaPronta> = [("k001".to_string(), no_cache)].into();
        let rel = ex.caminho_da_entrada(&concat, 1, &plano.blocos[0], &prontas).unwrap();
        assert!(rel.is_relative() && rel.ends_with(Path::new("camadas/abc").join(camada::LISTA)), "{}", rel.display());
        assert_eq!(fs::canonicalize(&amb.cache).unwrap().join("camadas/abc").join(camada::LISTA), {
            fs::create_dir_all(amb.cache.join("camadas/abc")).unwrap();
            fs::write(amb.cache.join("camadas/abc").join(camada::LISTA), "x").unwrap();
            fs::canonicalize(amb.trabalho.join(&rel)).unwrap()
        });
    }

    #[test]
    fn auxiliares_do_executor() {
        assert_eq!(codificador_de(&cpu()), Some("libx264"));
        assert!(e_gpu("h264_nvenc") && e_gpu("hevc_vaapi") && e_gpu("h264_qsv") && e_gpu("h264_amf") && e_gpu("h264_mf"));
        assert!(!e_gpu("libx264") && !e_gpu("ffv1") && !e_gpu("bmp"));
        assert_eq!(zoom_final("x\n[vidstabtransform @ 0x1] Final zoom: 3.25\nFinal zoom: 4.5\n"), 4.5);
        assert_eq!(zoom_final("nada"), 0.0);
        assert!(sha_valido(&"a".repeat(64)) && !sha_valido(&"A".repeat(64)) && !sha_valido("../x"));
        assert_eq!(partes("b003.mp4").unwrap(), ("b003", "mp4"));
        assert!(partes("../b.mp4").is_err() && partes("-x.mp4").is_err() && partes("semext").is_err());
        let mut l = Limitador::default();
        assert!(l.passa());
        assert!(!l.passa());
        assert!(paralelos(true) == 2 && (1..=2).contains(&paralelos(false)));
        // cancelamento (inclusive o da saída do app) e leitura divergente não derrubam para a CPU
        assert!(falha_de_gpu(&erro("gpu: b000 (código Some(187))"), true) && falha_de_gpu(&SicroError::Filesystem("x".into()), true));
        assert!(!falha_de_gpu(&erro(format!("{ERRO_CANCELADO}: b000")), true) && !falha_de_gpu(&erro(format!("{ERRO_LEITURA_DIVERGENTE}: bloco 0")), true));
        assert!(!falha_de_gpu(&erro("gpu: b000"), false));
        assert!(relativo_seguro("Mídias/Vídeos/cam 12:30.mp4") && relativo_seguro("./a/b.mp4"));
        assert!(!relativo_seguro("") && !relativo_seguro("../fora.mp4") && !relativo_seguro("a/../../b") && !relativo_seguro("/etc/passwd"));
        assert!(sem_tempo_nos_pacotes("h264") && sem_tempo_nos_pacotes("mpeg") && !sem_tempo_nos_pacotes("mov,mp4,m4a,3gp,3g2,mj2"));
    }

    #[test]
    fn amostras_de_um_wav_com_pedaco_extra() {
        let d = pasta("wav");
        let mut b = Vec::new();
        let dados = 4 * 1000u32;
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(4 + 8 + 16 + 8 + 5 + 1 + 8 + dados).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&[1, 0, 2, 0]);
        b.extend_from_slice(&48_000u32.to_le_bytes());
        b.extend_from_slice(&(48_000u32 * 4).to_le_bytes());
        b.extend_from_slice(&[4, 0, 16, 0]);
        b.extend_from_slice(b"LIST");
        b.extend_from_slice(&5u32.to_le_bytes());
        b.extend_from_slice(b"abcde\0");
        b.extend_from_slice(b"data");
        b.extend_from_slice(&dados.to_le_bytes());
        b.extend(std::iter::repeat_n(0u8, dados as usize));
        let p = d.path().join("a.wav");
        fs::write(&p, &b).unwrap();
        assert_eq!(amostras_wav(&p).unwrap(), 1000);
        fs::write(&p, b"RIFF....WAVE").unwrap();
        assert!(amostras_wav(&p).is_err());
    }

    /// Prévia `Vidstab`: o `.trf` vai para `vidstab/<sha256>.trf` do projeto e sai igual em duas execuções
    /// (`OMP_NUM_THREADS=1`).
    #[test]
    fn previa_vidstab_grava_o_trf_no_projeto() {
        let Some(cap) = fontes::cap() else { return };
        let (f, pts) = fonte(&cap);
        let mut plano = plano(&cap, &f, &pts, &cpu(), alvo_final(EscolhaCodificador::Cpu));
        let args: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-i", &f.to_string_lossy(), "-frames:v", "90", "-vf", "format=yuv420p,vidstabdetect=result=e001.trf", "-f", "null", "-"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        plano.previas = vec![Previa::Vidstab { efeito: "e1".into(), args, trf: "e001.trf".into() }];
        let mut shas = Vec::new();
        for t in ["t1", "t2"] {
            let d = pasta("vidstab");
            let amb = ambiente(cap.clone(), d.path(), t);
            let r = executar(&plano, &amb, &AtomicBool::new(false), &mut |_| {}).unwrap();
            assert_eq!(r.trf.len(), 1);
            let n = &r.trf[0];
            assert_eq!(n.efeito, "e1");
            let guardado = amb.pasta_projeto.join("vidstab").join(format!("{}.trf", n.sha256));
            assert_eq!(fs::read(&guardado).unwrap(), fs::read(amb.trabalho.join("e001.trf")).unwrap());
            assert_eq!(sha256_bytes(&fs::read(&guardado).unwrap()), n.sha256);
            assert_eq!(r.passos[0].nome, "vidstab", "prévia antes dos blocos");
            shas.push(n.sha256.clone());
        }
        assert_eq!(shas[0], shas[1], "{}", cap.versao);
    }

    /// Clipe estabilizado com o `.trf` já gravado (`trf_sha256`): o executor leva `vidstab/<sha256>.trf` do projeto à
    /// pasta de trabalho, onde o grafo do bloco o procura. Sem o arquivo no projeto: `arquivo_ausente`, antes do bloco.
    #[test]
    fn trf_guardado_chega_a_pasta_de_trabalho_do_bloco() {
        use crate::edicao::modelo::FaixaAnalise;
        use crate::edicao::motor::plano::ProvEstabilizacao;
        let Some(cap) = fontes::cap() else { return };
        let (f, pts) = fonte(&cap);
        // o `.trf` de verdade, feito pela prévia numa exportação anterior
        let d = pasta("trf-guardado");
        let amb = ambiente(cap.clone(), d.path(), "t0");
        let mut primeiro = plano(&cap, &f, &pts, &cpu(), alvo_final(EscolhaCodificador::Cpu));
        let args: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-i", &f.to_string_lossy(), "-frames:v", "90", "-vf", "format=yuv420p,vidstabdetect=result=e001.trf", "-f", "null", "-"].iter().map(|s| s.to_string()).collect();
        primeiro.previas = vec![Previa::Vidstab { efeito: "e1".into(), args, trf: "e001.trf".into() }];
        let sha = executar(&primeiro, &amb, &AtomicBool::new(false), &mut |_| {}).unwrap().trf[0].sha256.clone();
        // agora o projeto cita o `.trf`: o grafo usa `<sha256>.trf` e não há prévia
        let com_trf = |tarefa: &str, raiz: &Path| {
            let mut p = plano(&cap, &f, &pts, &cpu(), alvo_final(EscolhaCodificador::Cpu));
            for b in &mut p.blocos {
                b.grafo = b.grafo.replace("format=yuv420p,split", &format!("format=yuv420p,vidstabtransform=input={sha}.trf:optzoom=0:zoom=0:crop=black:smoothing=5:interpol=bilinear,split"));
                assert!(b.grafo.contains(".trf"));
                b.chave = sha256_bytes(format!("{}{}", b.chave, b.grafo).as_bytes());
            }
            p.proveniencia.estabilizacoes = vec![ProvEstabilizacao { efeito: "e1".into(), trf_sha256: Some(sha.clone()), analise: FaixaAnalise { de_s: 0.0, ate_s: 3.0, de_pts: 0, ate_pts: pts[89] }, zoom_pct: 0.0 }];
            (p, ambiente(cap.clone(), raiz, tarefa))
        };
        let (p, amb1) = com_trf("t1", d.path());
        let r = executar(&p, &amb1, &AtomicBool::new(false), &mut |_| {}).unwrap_or_else(|e| panic!("{}: {e}", cap.versao));
        assert!(r.trf.is_empty(), "nada analisado de novo");
        assert_eq!(sha256_bytes(&fs::read(amb1.trabalho.join(format!("{sha}.trf"))).unwrap()), sha);
        assert_eq!(r.conferencia.as_ref().map(|c| c.quadros), Some(90), "{}", cap.versao);
        // projeto sem o arquivo: falha dizendo qual, sem rodar bloco nenhum
        let vazio = pasta("trf-ausente");
        let (p, amb2) = com_trf("t1", vazio.path());
        let e = executar(&p, &amb2, &AtomicBool::new(false), &mut |_| {}).unwrap_err().to_string();
        assert!(e.contains("arquivo_ausente") && e.contains(&format!("vidstab/{sha}.trf")), "{e}");
        // com outro conteúdo no lugar: recusado, nunca usado
        let troca = amb2.pasta_projeto.join("vidstab");
        fs::create_dir_all(&troca).unwrap();
        fs::write(troca.join(format!("{sha}.trf")), b"VID.STAB 1\nlixo").unwrap();
        let e = executar(&p, &amb2, &AtomicBool::new(false), &mut |_| {}).unwrap_err().to_string();
        assert!(e.contains("leitura_divergente"), "{e}");
    }

    /// `preparar` num caso: mídia achada pelo relativo ou, sem ele, pelo SHA-256 no banco; trocada; ausente;
    /// na exportação final espera a tabela do decodificador da fila de fundo. Leitor de rastreios confere o SHA-256.
    #[test]
    fn preparar_le_o_caso() {
        let Some(cap) = fontes::cap() else { return };
        let (_d, ws) = caso();
        let f = fontes::gerar(Fonte::Mp4).unwrap();
        let vm = tauri::async_runtime::block_on(crate::commands::video_commands::register_video_media(
            ws.to_string_lossy().into(),
            crate::models::RegisterVideoInput { source_path: f.to_string_lossy().into(), allow_duplicate: false },
        ))
        .unwrap();
        let midia = |id: &str, sha: &str, rel: &str| {
            serde_json::json!({ "id": id, "tipo": "video", "origem": "caso", "media_id": null, "sha256": sha, "nome": "x.mp4", "relativo": rel,
                "duracao_s": null, "inicio_s": null, "largura": null, "altura": null, "taxa": null, "vfr": false, "tem_audio": true,
                "cor": null, "derivacao": null, "recebido": { "nativo": null, "obs": "" }, "relogio_desvio_s": null, "gravador": null,
                "decodificacao": null })
        };
        let fora = ws.parent().unwrap().join("fora do caso.mp4");
        fs::copy(&f, &fora).unwrap();
        let mut v: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/edicao/projeto-minimo.json")).unwrap();
        v["midias"] = serde_json::json!([
            midia("m1", &vm.sha256, &vm.relative_path),
            midia("m2", &vm.sha256, ""),
            midia("m3", &"0".repeat(64), &vm.relative_path),
            midia("m4", &"1".repeat(64), ""),
            midia("m5", "../fora", ""),
            midia("m6", &vm.sha256, "../fora do caso.mp4"),
            midia("m7", &vm.sha256, &fora.to_string_lossy())
        ]);
        // m6 e m7 apontam, com o SHA-256 certo, para um arquivo FORA do caso: não são lidas (são do projeto: sem banco)
        v["midias"][5]["origem"] = "projeto".into();
        v["midias"][6]["origem"] = "projeto".into();
        let p: ProjetoEdicao = serde_json::from_value(v).unwrap();
        let cache = pasta("preparar-cache");
        let prep = preparar(&ws, &p, cache.path(), true).unwrap();
        let estados: Vec<(&str, SituacaoMidia)> = prep.midias.iter().map(|m| (m.id.as_str(), m.estado)).collect();
        use SituacaoMidia::*;
        assert_eq!(estados, [("m1", Ok), ("m2", Ok), ("m3", Trocada), ("m4", Ausente), ("m5", Ausente), ("m6", Ausente), ("m7", Ausente)]);
        let mut chaves: Vec<&String> = prep.fontes.keys().collect();
        chaves.sort();
        assert_eq!(chaves, ["m1", "m2"]);
        let m1 = &prep.fontes["m1"];
        assert_eq!((m1.exibido, m1.sha256.as_str()), ((320, 180), vm.sha256.as_str()));
        assert_eq!(prep.midias[0].tabela, Some(m1.quadros.origem));
        assert_eq!(prep.contexto().midias.len(), 7);

        // a segunda leitura da mesma mídia com a mesma tabela não chama o ffprobe (aqui o arquivo nem existe mais)
        let copia = cache.path().join("copia.mp4");
        fs::copy(&f, &copia).unwrap();
        let tab = TabelaQuadros::obter_com(&cap, cache.path(), &vm.sha256, &copia).unwrap();
        let a = fonte_info_memo(&cap, &copia, &vm.sha256, tab.clone(), None).unwrap();
        fs::remove_file(&copia).unwrap();
        assert!(quadros::fonte_info(&cap, &copia, &vm.sha256, tab.clone(), None).is_err());
        let b = fonte_info_memo(&cap, &copia, &vm.sha256, tab.clone(), None).unwrap();
        assert_eq!((a.exibido, &a.conteiner, &a.codec), (b.exibido, &b.conteiner, &b.codec));
        // outra tabela (a do decodificador chegou): lê de novo
        let outra = Arc::new((*tab).clone());
        assert!(fonte_info_memo(&cap, &copia, &vm.sha256, outra, None).is_err());

        // exportação final: espera a do decodificador
        let cancelar = AtomicBool::new(false);
        let prep = preparar_com(&ws, &p, cache.path(), true, Some(&cancelar)).unwrap();
        assert_eq!(prep.fontes["m1"].quadros.origem, OrigemTabela::Decodificador, "{}", cap.versao);
        assert_eq!(prep.midias[0].erros, Some(0));
        assert_eq!(prep.fontes["m1"].quadros.pts.len(), 450);
        // exportação cancelada durante o SHA-256 da sessão (a tabela já está pronta): para ali, não analisa
        let cancelado = AtomicBool::new(true);
        let e = preparar_com(&ws, &p, cache.path(), true, Some(&cancelado)).err().expect("cancelada");
        assert_eq!(codigo_de(&e), Some(ERRO_CANCELADO), "{e}");

        // rastreio denso pelo SHA-256 do conteúdo
        let d = pasta("leitor");
        let denso = serde_json::json!({ "esquema": 1, "rastreio": "r0", "midia_sha256": vm.sha256, "espaco": "original",
            "pixel": { "w": 320, "h": 180 }, "pts": [0], "x": [0.1], "y": [0.1], "w": [0.2], "h": [0.2], "conf": [1.0],
            "estado": [0], "tipo": [0], "corrida": [0], "corridas": [] });
        let bytes = serde_json::to_vec(&denso).unwrap();
        let sha = sha256_bytes(&bytes);
        fs::create_dir_all(d.path().join("rastreios/r1")).unwrap();
        fs::write(d.path().join("rastreios/r1").join(format!("{sha}.json")), &bytes).unwrap();
        let leitor = LeitorDisco { pasta: d.path().to_path_buf(), densos: Mutex::new(HashMap::new()) };
        assert_eq!(leitor.denso("r1", &sha).unwrap().rastreio, "r0");
        assert!(leitor.denso("r1", &"e".repeat(64)).is_err());
        assert!(leitor.denso("../r1", &sha).is_err());
        fs::write(d.path().join("rastreios/r1").join(format!("{}.json", "f".repeat(64))), &bytes).unwrap();
        assert!(leitor.denso("r1", &"f".repeat(64)).is_err(), "conteúdo que não bate com o nome");
        assert!(leitor.silhuetas("r1", &sha).unwrap_err().to_string().contains("nao_suportado"));
    }

    /// Tabela de mídia sem tempo nos pacotes pedida pela tela (a cada quadro): a que falhou há pouco não roda de novo a
    /// cada pedido (decisões (g)14 e (h)14); a exportação, que espera por ela, refaz.
    #[test]
    fn tabela_que_falhou_nao_e_refeita_a_cada_pedido_da_tela() {
        let Some(cap) = fontes::cap() else { return };
        let d = pasta("tabela-falha");
        let sha = sha256_bytes(d.path().to_string_lossy().as_bytes());
        let falta = d.path().join("não existe.h264");
        let nao = AtomicBool::new(false);
        let chave = pedir_tabela(&cap, d.path(), &sha, &falta, "h264", false);
        esperar_fundo(&chave, &nao).unwrap();
        let falhou = fundo::estado(&chave).expect("na lista");
        assert!(matches!(falhou.estado, EstadoFundo::Falhou { .. }), "{falhou:?}");
        for _ in 0..3 {
            pedir_tabela(&cap, d.path(), &sha, &falta, "h264", false);
        }
        assert_eq!(fundo::estado(&chave), Some(falhou.clone()), "refeita a cada pedido da tela");
        pedir_tabela(&cap, d.path(), &sha, &falta, "h264", true);
        assert_ne!(fundo::estado(&chave).map(|t| t.id), Some(falhou.id), "a exportação refaz");
        esperar_fundo(&chave, &nao).unwrap();
    }

    /// SHA-256 da sessão em pedaços: igual ao de `hashing`, um aviso por pedaço, e o cancelamento vale no pedaço seguinte.
    #[test]
    fn sha256_em_pedacos_olha_o_cancelamento() {
        let d = pasta("sha-pedacos");
        let a = d.path().join("grande.bin");
        let n = PEDACO_SHA * 2 + 123;
        fs::write(&a, (0..n).map(|i| (i % 251) as u8).collect::<Vec<u8>>()).unwrap();
        let mut avisos = Vec::new();
        let h = sha256_em_pedacos(&a, &AtomicBool::new(false), &mut |l, t| avisos.push((l, t))).unwrap();
        assert_eq!(h, crate::hashing::sha256::sha256_file(&a).unwrap());
        assert_eq!(avisos.len(), 3);
        assert_eq!(avisos.last(), Some(&(n as u64, n as u64)));
        let c = AtomicBool::new(false);
        let mut k = 0;
        let e = sha256_em_pedacos(&a, &c, &mut |_, _| {
            k += 1;
            c.store(true, Ordering::SeqCst);
        })
        .unwrap_err();
        assert_eq!((codigo_de(&e), k), (Some(ERRO_CANCELADO), 1));
    }

    /// Exportação com ocultar: saem os blocos, camadas e prévias das tarefas do projeto; os de outro projeto ficam.
    #[test]
    fn limpeza_do_projeto_com_ocultar() {
        let d = pasta("limpeza-projeto");
        let cache = d.path();
        let (a, b, q, k) = ("a".repeat(64), "b".repeat(64), "c".repeat(64), "d".repeat(64));
        let tarefa = |t: &str, pid: &str, dir: &str, blocos: &[&str], camadas: &[&str]| {
            let r = cache.join("render").join(t);
            fs::create_dir_all(&r).unwrap();
            fs::write(r.join(super::super::fila::RETRATO_TAREFA), serde_json::json!({ "id": pid }).to_string()).unwrap();
            fs::write(r.join(CHAVES), serde_json::json!({ "dir": dir, "blocos": blocos, "camadas": camadas }).to_string()).unwrap();
        };
        tarefa("t1", "p1", "blocos", &[&a], &[&k]);
        tarefa("t2", "p2", "blocos", &[&b], &[]);
        tarefa("t3", "p1", "quadro", &[&q], &[]);
        tarefa("t4", "p1", "blocos", &[], &[]);
        for (dir, n) in [("blocos", format!("{a}.mp4")), ("blocos", format!("{a}.framehash")), ("blocos", format!("{b}.mp4")), ("quadro", format!("{q}.bmp"))] {
            fs::create_dir_all(cache.join(dir)).unwrap();
            fs::write(cache.join(dir).join(n), b"x").unwrap();
        }
        fs::create_dir_all(cache.join("camadas").join(&k)).unwrap();
        let z = "f".repeat(64);
        tarefa("t5", "p1", "blocos", &[&z], &[]);
        fs::write(cache.join("blocos").join(format!("{z}.mp4")), b"x").unwrap();
        limpar_do_projeto(cache, "p1", "t4", &["t5".to_string()]);
        assert_eq!(arquivos(&cache.join("blocos")), [format!("{b}.mp4"), format!("{z}.mp4")]);
        assert!(arquivos(&cache.join("quadro")).is_empty() && !cache.join("camadas").join(&k).exists());
        assert_eq!(arquivos(&cache.join("render")), ["t2", "t4", "t5"], "a viva que entrou depois fica inteira");
    }

    /// Exportação com `ocultar` que termina com uma prévia NOVA do mesmo projeto já na fila (entrou depois da parada):
    /// o bloco e a camada que a nova usa ficam, ainda que o plano da exportação e uma prévia antiga usem as mesmas
    /// chaves; o resto do plano e da antiga sai. Antes, a nova falhava por arquivo sumido no meio do render.
    #[test]
    fn limpeza_nao_tira_o_que_a_previa_nova_usa() {
        let Some(cap) = fontes::cap() else { return };
        let (f, pts) = fonte(&cap);
        let d = pasta("limpeza-viva");
        let cache = d.path();
        let plano = plano(&cap, &f, &pts, &cpu(), alvo_final(EscolhaCodificador::Cpu));
        let (b0, b1) = (plano.blocos[0].chave.clone(), plano.blocos[1].chave.clone());
        // camadas: uma só do plano antigo, outra que a nova também usa
        let (k_velha, k_viva) = ("1".repeat(64), "2".repeat(64));
        let tarefa = |t: &str, blocos: &[&str], camadas: &[&str]| {
            let r = cache.join("render").join(t);
            fs::create_dir_all(&r).unwrap();
            fs::write(r.join(super::super::fila::RETRATO_TAREFA), serde_json::json!({ "id": "p1" }).to_string()).unwrap();
            fs::write(r.join(CHAVES), serde_json::json!({ "dir": "blocos", "blocos": blocos, "camadas": camadas }).to_string()).unwrap();
        };
        tarefa("exportacao", &[&b0, &b1], &[&k_velha, &k_viva]);
        tarefa("antiga", &[&b0], &[&k_velha, &k_viva]);
        tarefa("nova", &[&b1], &[&k_viva]);
        fs::create_dir_all(cache.join("blocos")).unwrap();
        for b in [&b0, &b1] {
            fs::write(cache.join("blocos").join(format!("{b}.mp4")), b"x").unwrap();
            fs::write(cache.join("blocos").join(format!("{b}.framehash")), b"x").unwrap();
        }
        for k in [&k_velha, &k_viva] {
            fs::create_dir_all(cache.join("camadas").join(k)).unwrap();
        }
        let vivas = vec!["nova".to_string()];
        limpar_do_plano(cache, &plano, &vivas);
        assert_eq!(arquivos(&cache.join("blocos")), [format!("{b1}.framehash"), format!("{b1}.mp4")], "o bloco da nova fica; o outro do plano sai");
        // a limpeza do projeto (que roda em seguida, com a exportação e a nova na lista das vivas) também respeita
        limpar_do_projeto(cache, "p1", "exportacao", &["exportacao".to_string(), "nova".to_string()]);
        assert_eq!(arquivos(&cache.join("blocos")), [format!("{b1}.framehash"), format!("{b1}.mp4")]);
        assert_eq!(arquivos(&cache.join("camadas")), [k_viva.clone()], "a camada da nova fica; a só das antigas sai");
        assert_eq!(arquivos(&cache.join("render")), ["exportacao", "nova"]);
        // sem tarefa viva, tudo do plano sai
        limpar_do_plano(cache, &plano, &[]);
        assert!(arquivos(&cache.join("blocos")).is_empty());
    }

    /// (Só Unix: abre pasta para mudar a data, o que o Windows não deixa sem `FILE_FLAG_BACKUP_SEMANTICS`.)
    #[cfg(unix)]
    #[test]
    fn limpeza_do_cache() {
        let d = pasta("limpeza");
        let raiz = d.path();
        let caso = raiz.join("caso1");
        let velho = SystemTime::now() - Duration::from_secs(8 * 24 * 3600);
        let escrever = |rel: &str, n: usize, quando: Option<SystemTime>| {
            let p = caso.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, vec![0u8; n]).unwrap();
            if let Some(q) = quando {
                fs::File::options().write(true).open(&p).unwrap().set_modified(q).unwrap();
            }
            p
        };
        let tmp = escrever("previa/aa.tmp-1-0/x.mp4", 10, None);
        fs::File::open(tmp.parent().unwrap()).unwrap().set_modified(velho).unwrap();
        let render_velho = escrever("render/t-velho/projeto.json", 10, None);
        fs::File::open(render_velho.parent().unwrap()).unwrap().set_modified(velho).unwrap();
        let render_vivo = escrever("render/t-vivo/projeto.json", 10, None);
        fs::File::open(render_vivo.parent().unwrap()).unwrap().set_modified(velho).unwrap();
        let antigo = escrever("blocos/a.mp4", 1000, Some(velho));
        let novo = escrever("blocos/b.mp4", 1000, None);
        let tabela = escrever("quadros/x.q2", 5000, Some(velho));
        let liberado = limpar_cache(raiz, 1500, &["t-vivo".to_string()]);
        assert!(!tmp.parent().unwrap().exists() && !render_velho.parent().unwrap().exists());
        assert!(render_vivo.exists(), "tarefa viva fica");
        assert!(!antigo.exists() && novo.exists(), "acima do teto sai o mais antigo");
        assert!(tabela.exists(), "tabelas de quadros não entram no teto");
        assert_eq!(liberado, 10 + 10 + 1000);
    }

    /// Pendência (e) do P1.8: `render/<tarefa>` de exportação interrompida (processo morto) perde os temporários grandes e
    /// guarda só o id do projeto e as chaves; com outra instância do app viva no mesmo cache nada alheio é tocado, nem
    /// pelo teto; a tarefa viva desta instância e a mais nova que a partida do processo ficam; temporários de `origem/`
    /// e `ondas/` de processo morto saem. (Só Unix: muda a data de pasta.)
    #[cfg(unix)]
    #[test]
    fn render_interrompido_e_enxugado_so_sem_outra_instancia() {
        let d = pasta("instancias");
        let raiz = d.path();
        let caso = raiz.join("caso1");
        let tarefa = |nome: &str| {
            let t = caso.join("render").join(nome);
            fs::create_dir_all(t.join("miniaturas")).unwrap();
            fs::write(t.join(super::super::fila::RETRATO_TAREFA), serde_json::json!({ "id": "p1", "titulo": "x".repeat(500) }).to_string()).unwrap();
            fs::write(t.join(CHAVES), r#"{"dir":"blocos","blocos":[],"camadas":[]}"#).unwrap();
            fs::write(t.join("saida.mp4"), vec![0u8; 4000]).unwrap();
            fs::write(t.join("audio.wav"), vec![0u8; 6000]).unwrap();
            fs::write(t.join("miniaturas").join("b000.bmp"), vec![0u8; 300]).unwrap();
            t
        };
        let (morta, viva) = (tarefa("t-morta"), tarefa("t-viva"));
        let velho = SystemTime::now() - IDADE_TMP - Duration::from_secs(600);
        for sub in ["origem/aa.tmp-1-0", "ondas/bb.pk.tmp-1-0"] {
            fs::create_dir_all(caso.join(sub)).unwrap();
            fs::write(caso.join(sub).join("x"), vec![0u8; 50]).unwrap();
            fs::File::open(caso.join(sub)).unwrap().set_modified(velho).unwrap();
        }
        let bloco = caso.join("blocos").join("a.mp4");
        fs::create_dir_all(bloco.parent().unwrap()).unwrap();
        fs::write(&bloco, vec![0u8; 1000]).unwrap();
        let vivas = ["t-viva".to_string()];
        let depois = SystemTime::now() + Duration::from_secs(5);
        // tarefa mais nova que a partida do processo pode ser dele: fica inteira
        assert_eq!(limpar_cache_em(raiz, u64::MAX, &vivas, SystemTime::now() - Duration::from_secs(3600)), 100, "só os temporários");
        assert!(morta.join("saida.mp4").exists() && !caso.join("origem/aa.tmp-1-0").exists() && !caso.join("ondas/bb.pk.tmp-1-0").exists());
        // outra instância viva (marca presa por outro descritor): nem o render parado nem o bloco recente saem
        let outra = raiz.join(INSTANCIAS).join("99999-de-outro-processo.trava");
        let presa = fs::File::create(&outra).unwrap();
        presa.try_lock().expect("trava de arquivo");
        assert_eq!(limpar_cache_em(raiz, 500, &vivas, depois), 0);
        assert!(morta.join("saida.mp4").exists() && bloco.exists() && outra.exists());
        // a outra instância morre: a marca solta sai, o render interrompido é enxugado e o teto volta a valer
        drop(presa);
        let liberado = limpar_cache_em(raiz, 500, &vivas, depois);
        assert!(!outra.exists(), "marca de processo morto");
        assert!(!morta.join("saida.mp4").exists() && !morta.join("audio.wav").exists() && !morta.join("miniaturas").exists());
        assert_eq!(fs::read_to_string(morta.join(super::super::fila::RETRATO_TAREFA)).unwrap(), r#"{"id":"p1"}"#);
        assert!(morta.join(CHAVES).is_file(), "as chaves ficam: a limpeza por projeto acha os blocos por elas");
        assert!(viva.join("saida.mp4").exists() && viva.join("audio.wav").exists(), "tarefa viva desta instância fica");
        assert!(!bloco.exists(), "acima do teto");
        assert!(liberado >= 4000 + 6000 + 300 + 1000, "{liberado}");
        // de novo: nada mais a tirar, e a marca desta instância segue de pé
        assert_eq!(limpar_cache_em(raiz, 500, &vivas, depois), 0);
        assert_eq!(fs::read_dir(raiz.join(INSTANCIAS)).unwrap().count(), 1);
    }

    /// Partida depois de processo morto: a pasta de trabalho da tabela do decodificador (stderr de ~300 B por quadro)
    /// e a da sonda, de outro pid e paradas há mais de 6 h, saem da pasta temporária do sistema; a deste processo, a de
    /// outro processo que ainda escreve, a recente e o que não é da Edição ficam. (Só Unix: muda a data de pasta.)
    #[cfg(unix)]
    #[test]
    fn temporarias_de_processo_morto_saem_na_partida() {
        let d = pasta("temporarias");
        let t = d.path();
        let velho = SystemTime::now() - IDADE_TMP - Duration::from_secs(600);
        let meu = std::process::id();
        let criar = |nome: &str, arquivo_velho: bool| {
            let p = t.join(nome);
            fs::create_dir_all(&p).unwrap();
            let f = p.join("quadros.stderr");
            fs::write(&f, vec![0u8; 300]).unwrap();
            if arquivo_velho {
                fs::File::options().write(true).open(&f).unwrap().set_modified(velho).unwrap();
            }
            fs::File::open(&p).unwrap().set_modified(velho).unwrap();
            p
        };
        let morta = criar("sicro-quadros-4000000-0", true);
        let sonda_morta = criar("sicro-sonda-gpu-serve-4000001-3", true);
        let minha = criar(&format!("sicro-quadros-{meu}-0"), true);
        let escrevendo = criar("sicro-quadros-4000002-0", false);
        let alheia = criar("outro-4000003-0", true);
        let recente = t.join("sicro-quadros-4000004-0");
        fs::create_dir_all(&recente).unwrap();
        assert_eq!(limpar_temporarias(t), 600);
        assert!(!morta.exists() && !sonda_morta.exists());
        assert!(minha.exists() && escrevendo.exists() && alheia.exists() && recente.exists());
    }
}
