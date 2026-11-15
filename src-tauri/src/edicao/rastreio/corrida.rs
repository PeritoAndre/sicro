//! Corrida pedida pela tela (§6.2, §6.3): uma por vez no processo; quadros por pipe `rawvideo rgb24` (decodificação
//! em CPU) do intermediário de origem quando o espaço não é "original", senão da fonte; busca pela regra do §5.3;
//! pts de cada quadro pelo `showinfo` lido em paralelo, casado por ordem e conferido com a tabela do decodificador;
//! para trás, blocos de até 100 quadros. Eventos `edicao:rastreio`, até 5 por segundo. Dono: P2.2.
//!
//! Uma corrida para na âncora seguinte do perito no seu sentido (`Motivo::Ancora`); `ambos` = para a frente e depois
//! para trás, da mesma âncora. O denso novo junta a corrida ao que o rastreio já tinha (`arquivo::juntar`).

use std::collections::VecDeque;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::edgetam::EdgeTam;
use super::mancha::Mancha;
use super::vittrack::VitTrack;
use super::{arquivo, laco, silhueta, ArquivoDenso, ArquivoSilhuetas, Avanco, EscolhaMotor, Imagem, LeitorQuadros, Motivo, Parou, PedidoCorrida, Rastreador, Sentido, SentidoPedido, MAX_QUADROS};
use crate::commands::settings_commands::AiSettings;
use crate::edicao::executar::{self, ERRO_CANCELADO, ERRO_LEITURA_DIVERGENTE};
use crate::edicao::ffmpeg::args;
use crate::edicao::ffmpeg::exec::{self, Comando};
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::mensagens::{self, codigo};
use crate::edicao::modelo::{Caixa, Chave, ClipePrincipal, ClipeTrecho, Denso, Faixa, Matriz, OrdemCampos, ProjetoEdicao, Racional, Rastreio, Silhuetas, TamanhoPx, Tolerante};
use crate::edicao::motor::FonteInfo;
use crate::edicao::origem::{self, EstadoOrigem};
use crate::edicao::quadros::{Busca as BuscaTabela, OrigemTabela, TabelaQuadros};
use crate::edicao::{armazem, cor, geometria, tempo};
use crate::error::{Result, SicroError};

/// Nome do evento (§6.3).
pub const EVENTO_RASTREIO: &str = "edicao:rastreio";
/// Lado maior da miniatura do evento (px).
pub const MINIATURA_MAX: u32 = 320;
/// Eventos de andamento por segundo, no máximo.
pub const AVANCOS_POR_S: u32 = 5;
// CONTRATO: acréscimo. Tamanho do bloco para trás e o teto de memória dele (o bloco inteiro fica na memória).
/// Quadros por bloco para trás (menos em quadro grande: `MEMORIA_BLOCO`).
pub const BLOCO_TRAS: usize = 100;
/// Memória de um bloco para trás.
pub const MEMORIA_BLOCO: usize = 512 << 20;
/// Teto do quadro entregue (8K).
const LADO_MAX: u32 = 8192;
/// Quadros esperando entre o ffmpeg e o laço.
const FILA: usize = 2;
/// Espera máxima pela linha do `showinfo` de um quadro que já chegou.
const ESPERA_SHOWINFO: Duration = Duration::from_secs(10);
/// Intervalo com que `proximo` olha o cancelamento enquanto espera um quadro.
const OLHAR_CANCELAR: Duration = Duration::from_millis(20);
const QUALIDADE_JPEG: u8 = 70;
/// Passo dos quadros-chave do intermediário de origem (`-g 12` de `origem.rs`).
const GOP_INTERMEDIARIO: usize = 12;

/// O que `edicao_rastreio_seguir` recebe (§6.3). `ate_s` em tempo de fonte (None = até a borda da mídia).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PedidoSeguir {
    pub rastreio_id: String,
    pub clipe_id: String,
    pub ancora: Chave<Caixa>,
    pub sentido: SentidoPedido,
    pub ate_s: Option<f64>,
    pub motor: EscolhaMotor,
}

/// Andamento de uma corrida. `caixa` em 0..1 (None = sem o alvo nesse quadro); `estado` como no denso;
/// `miniatura` = JPEG de até 320 px do quadro com a caixa, em `data:image/jpeg;base64,…`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvancoRastreio {
    pub tarefa_id: String,
    pub rastreio_id: String,
    pub sentido: Sentido,
    pub pts: i64,
    pub caixa: Option<Caixa>,
    pub conf: f64,
    pub estado: u8,
    pub feitos: u32,
    pub miniatura: Option<String>,
}

/// Fim de uma corrida pedida (as duas, em `ambos`): o front aplica `denso` e `silhuetas` com `alterar()`.
/// `denso` None = nada novo (cancelada antes do primeiro quadro seguido).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FimRastreio {
    pub tarefa_id: String,
    pub rastreio_id: String,
    pub parou: Vec<ParouSentido>,
    pub denso: Option<Denso>,
    pub silhuetas: Option<Silhuetas>,
}

/// Onde parou cada sentido (`Parou` com o sentido).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ParouSentido {
    pub sentido: Sentido,
    pub motivo: super::Motivo,
    pub pts: Option<i64>,
}

impl ParouSentido {
    pub fn de(sentido: Sentido, p: Parou) -> Self {
        Self { sentido, motivo: p.motivo, pts: p.pts }
    }
}

/// Corrida que falhou: `mensagem` = `"<código>: <frase>"` do catálogo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FalhaRastreio {
    pub tarefa_id: String,
    pub rastreio_id: String,
    pub mensagem: String,
}

/// Carga do evento `edicao:rastreio`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum EventoRastreio {
    Avanco(AvancoRastreio),
    Fim(FimRastreio),
    Falhou(FalhaRastreio),
}

/// Quem recebe os eventos (o comando injeta o `emit` do Tauri; os testes, um coletor).
pub type Emissor = Box<dyn Fn(EventoRastreio) + Send + Sync>;

fn erro(cod: &str, detalhe: impl std::fmt::Display) -> SicroError {
    SicroError::Validation(format!("{cod}: {detalhe}"))
}

// ---------------------------------------------------------------- uma corrida por vez

struct EmCurso {
    tarefa: String,
    cancelar: Arc<AtomicBool>,
}

static EM_CURSO: Mutex<Option<EmCurso>> = Mutex::new(None);

/// Solta a vez quando a corrida acaba (também em pânico).
struct Vez(String);

impl Drop for Vez {
    fn drop(&mut self) {
        let mut g = EM_CURSO.lock().unwrap_or_else(|e| e.into_inner());
        if g.as_ref().is_some_and(|e| e.tarefa == self.0) {
            *g = None;
        }
    }
}

/// O cancelamento visível da corrida em andamento (teste de "menos de 0,5 s").
pub fn em_curso() -> Option<(String, Arc<AtomicBool>)> {
    EM_CURSO.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|e| (e.tarefa.clone(), e.cancelar.clone()))
}

/// Cancela a corrida (menos de 0,5 s até o fim dela); tarefa que já acabou = `tarefa_ausente`.
pub fn cancelar(tarefa_id: &str) -> Result<()> {
    match EM_CURSO.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        Some(e) if e.tarefa == tarefa_id => {
            e.cancelar.store(true, Ordering::SeqCst);
            Ok(())
        }
        _ => Err(erro(codigo::TAREFA_AUSENTE, tarefa_id)),
    }
}

// ---------------------------------------------------------------- motor

/// O motor da corrida pela regra do `auto` (§6.2). Pedido explícito sem o item = `modelo_ausente: <motor>`;
/// interruptor desligado = `ia_desligada` (a mancha segue sem os modelos). No `auto`, motor neural que não abre cai
/// para o seguinte.
pub fn escolher(ai: &AiSettings, pedido: EscolhaMotor) -> Result<Box<dyn Rastreador>> {
    let ligada = crate::ia::ligada(ai);
    let desligada = || SicroError::Validation(codigo::IA_DESLIGADA.into());
    match pedido {
        EscolhaMotor::Mancha => Ok(Box::new(Mancha::nova()?)),
        EscolhaMotor::Edgetam if !ligada => Err(desligada()),
        EscolhaMotor::Edgetam if !EdgeTam::instalado(ai) => Err(erro(codigo::MODELO_AUSENTE, "edgetam")),
        EscolhaMotor::Edgetam => Ok(Box::new(EdgeTam::novo(ai, &crate::ia::Opcoes::default())?)),
        EscolhaMotor::Vittrack if !ligada => Err(desligada()),
        EscolhaMotor::Vittrack => Ok(Box::new(VitTrack::novo(ai)?)),
        EscolhaMotor::Auto => {
            if ligada {
                if crate::ia::gpu::pronta(ai).is_some() && EdgeTam::instalado(ai) {
                    match EdgeTam::novo(ai, &crate::ia::Opcoes::default()) {
                        Ok(m) => return Ok(Box::new(m)),
                        Err(e) => tracing::warn!("rastreio: edgetam não abriu ({e}); vittrack"),
                    }
                }
                if crate::ia::sessao::biblioteca(ai).is_some() {
                    match VitTrack::novo(ai) {
                        Ok(m) => return Ok(Box::new(m)),
                        Err(e) => tracing::warn!("rastreio: vittrack não abriu ({e}); mancha"),
                    }
                }
            }
            Ok(Box::new(Mancha::nova()?))
        }
    }
}

// ---------------------------------------------------------------- leitor por pipe

/// Pasta de trabalho de um processo (grafo e stderr), apagada no fim.
struct Pasta(PathBuf);

impl Pasta {
    fn nova() -> Result<Self> {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!("sicro-rastreio-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&p)?;
        Ok(Pasta(p))
    }
}

impl Drop for Pasta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// pts de uma linha `[showinfo@rq @ 0x…] n: … pts: … ` (None = outra linha).
fn pts_do_showinfo(linha: &str) -> Option<std::result::Result<i64, ()>> {
    let resto = linha.trim_start_matches(|c: char| c == '\r').strip_prefix("[showinfo@rq @ ")?;
    let resto = resto[resto.find(']')? + 1..].trim_start();
    if !resto.starts_with("n:") {
        return None;
    }
    let v = resto[resto.find(" pts:")? + 5..].split_whitespace().next()?;
    Some(v.parse().map_err(|_| ()))
}

/// O stderr do ffmpeg lido enquanto ele escreve: os pts do `showinfo`, em ordem.
struct Cauda {
    caminho: PathBuf,
    f: Option<std::fs::File>,
    resto: Vec<u8>,
    fila: VecDeque<i64>,
}

impl Cauda {
    fn nova(caminho: PathBuf) -> Self {
        Self { caminho, f: None, resto: Vec::new(), fila: VecDeque::new() }
    }

    fn ler(&mut self) -> Result<()> {
        if self.f.is_none() {
            self.f = std::fs::File::open(&self.caminho).ok();
        }
        let Some(f) = self.f.as_mut() else { return Ok(()) };
        let mut b = [0u8; 1 << 16];
        loop {
            let n = f.read(&mut b)?;
            if n == 0 {
                break;
            }
            self.resto.extend_from_slice(&b[..n]);
        }
        while let Some(i) = self.resto.iter().position(|&c| c == b'\n') {
            let linha: Vec<u8> = self.resto.drain(..=i).collect();
            match pts_do_showinfo(&String::from_utf8_lossy(&linha)) {
                Some(Ok(p)) => self.fila.push_back(p),
                Some(Err(())) => return Err(erro(ERRO_LEITURA_DIVERGENTE, "quadro sem pts")),
                None => {}
            }
        }
        Ok(())
    }

    /// pts do próximo quadro: a linha sai antes do quadro chegar ao pipe; espera um pouco se ainda não está no arquivo.
    fn proximo(&mut self, parar: &AtomicBool) -> Result<i64> {
        let inicio = Instant::now();
        loop {
            if let Some(p) = self.fila.pop_front() {
                return Ok(p);
            }
            self.ler()?;
            if self.fila.is_empty() {
                if parar.load(Ordering::SeqCst) {
                    return Err(erro(ERRO_CANCELADO, "rastreio"));
                }
                if inicio.elapsed() > ESPERA_SHOWINFO {
                    return Err(erro(ERRO_LEITURA_DIVERGENTE, "quadro sem showinfo"));
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

/// Lê os quadros `ja..=jb` da tabela de `fonte`, em ordem, e entrega cada um (pts, RGB) a `entregar` (false = basta);
/// de um grupo de pts repetido, só o último. `ja` e `jb` não partem grupo. Devolve quantos leu. Leitura que diverge da tabela no primeiro quadro com busca é refeita sem ela (§5.3).
fn ler_faixa(
    cap: &Capacidades,
    fonte: &FonteInfo,
    cadeia_cor: &str,
    ja: usize,
    jb: usize,
    pixel: TamanhoPx,
    parar: &AtomicBool,
    processos: &AtomicUsize,
    entregar: &mut dyn FnMut(i64, Vec<u8>) -> bool,
) -> Result<usize> {
    let busca = fonte.quadros.busca_em(fonte.inicio_s, ja);
    let mut entregues = 0;
    let r = ler_uma(cap, fonte, cadeia_cor, ja, jb, pixel, parar, busca, processos, &mut entregues, entregar);
    match r {
        Err(e) if busca.is_some() && entregues == 0 && executar::codigo_de(&e) == Some(ERRO_LEITURA_DIVERGENTE) && !parar.load(Ordering::SeqCst) => {
            tracing::warn!("rastreio: leitura com busca não conferiu ({e}); refeita sem busca");
            ler_uma(cap, fonte, cadeia_cor, ja, jb, pixel, parar, None, processos, &mut entregues, entregar)?;
            Ok(entregues)
        }
        r => r.map(|_| entregues),
    }
}

#[allow(clippy::too_many_arguments)]
fn ler_uma(
    cap: &Capacidades,
    fonte: &FonteInfo,
    cadeia_cor: &str,
    ja: usize,
    jb: usize,
    pixel: TamanhoPx,
    parar: &AtomicBool,
    busca: Option<f64>,
    processos: &AtomicUsize,
    entregues: &mut usize,
    entregar: &mut dyn FnMut(i64, Vec<u8>) -> bool,
) -> Result<()> {
    let tab = &fonte.quadros;
    let esperados = &tab.pts[ja..=jb];
    let trab = Pasta::nova()?;
    let corte = match tab.pts.get(jb + 1) {
        Some(fim) => format!("trim=start_pts={}:end_pts={fim}", tab.pts[ja]),
        None => format!("trim=start_pts={}", tab.pts[ja]),
    };
    let rgb = format!("scale={}:{}:flags=bicubic+accurate_rnd+bitexact:in_color_matrix=bt709:in_range=tv,format=rgb24", pixel.w, pixel.h);
    args::gravar_texto(&trab.0, "g.txt", &format!("[0:v:0]{corte},showinfo@rq=checksum=0,{cadeia_cor},{rgb}[v]"))?;
    let mut a = args::globais();
    if let Some(alvo) = busca {
        let fim = if jb + 1 < tab.pts.len() { tab.tempo(jb + 1) } else { tab.fim_s() } - fonte.inicio_s;
        a.extend(["-ss".to_string(), format!("{alvo:.6}"), "-t".to_string(), format!("{:.6}", fim - alvo + 1.0)]);
    }
    a.extend(["-i".to_string(), fonte.caminho.to_string_lossy().into_owned()]);
    a.extend(args::grafo_em_arquivo(cap, "g.txt"));
    a.extend(["-map", "[v]", "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"].iter().map(|s| s.to_string()));
    let cmd = Comando {
        nome: "rastreio".into(),
        programa: cap.binario.clone(),
        args: a,
        pasta: trab.0.clone(),
        ambiente: Vec::new(),
        sem_ambiente: Vec::new(),
        stderr: trab.0.join("rastreio.stderr"),
    };
    processos.fetch_add(1, Ordering::SeqCst);
    let tamanho = pixel.w as usize * pixel.h as usize * 3;
    let mut cauda = Cauda::nova(cmd.stderr.clone());
    let mut buf: Vec<u8> = Vec::with_capacity(tamanho);
    let mut basta = false;
    let t = exec::rodar_com_fluxo(&cmd, parar, &mut |mut pedaco: &[u8]| {
        while !pedaco.is_empty() {
            let n = (tamanho - buf.len()).min(pedaco.len());
            buf.extend_from_slice(&pedaco[..n]);
            pedaco = &pedaco[n..];
            if buf.len() < tamanho {
                break;
            }
            let pts = cauda.proximo(parar)?;
            let k = *entregues;
            let Some(&previsto) = esperados.get(k) else { return Ok(false) };
            if pts != previsto {
                return Err(erro(ERRO_LEITURA_DIVERGENTE, format!("quadro {} lido com pts {pts}, tabela {previsto}", ja + k)));
            }
            // pts repetido (MPEG-PS): um quadro por pts, o último, que é o exibido (§3.1)
            if esperados.get(k + 1) != Some(&pts) {
                let q = std::mem::replace(&mut buf, Vec::with_capacity(tamanho));
                if !entregar(pts, q) {
                    basta = true;
                    return Ok(false);
                }
            } else {
                buf.clear();
            }
            *entregues += 1;
            if *entregues == esperados.len() {
                return Ok(false);
            }
        }
        Ok(true)
    })?;
    if basta || *entregues == esperados.len() || parar.load(Ordering::SeqCst) {
        return Ok(());
    }
    exec::exigir_sucesso(&cmd, &t)?;
    Err(erro(ERRO_LEITURA_DIVERGENTE, format!("lidos {} de {} quadros", *entregues, esperados.len())))
}

/// Leitor por pipe (§6.2). `pixel` = tamanho entregue (o exibido da fonte; o do intermediário no espaço de origem).
/// Um processo do ffmpeg numa thread própria; os quadros chegam por uma fila curta. Fechar o leitor mata o processo.
pub struct LeitorPipe {
    rx: Option<Receiver<Result<(i64, Vec<u8>)>>>,
    parar: Arc<AtomicBool>,
    externo: Option<Arc<AtomicBool>>,
    fio: Option<std::thread::JoinHandle<()>>,
    atual: Vec<u8>,
    pixel: TamanhoPx,
    processos: Arc<AtomicUsize>,
}

impl LeitorPipe {
    /// Quadros de `fonte` a partir do quadro de `de_pts`, no `sentido`, até `ate_pts` (incluso; None = borda).
    pub fn abrir(cap: &Capacidades, fonte: &FonteInfo, de_pts: i64, ate_pts: Option<i64>, sentido: Sentido, pixel: TamanhoPx) -> Result<Self> {
        Self::abrir_com_cancelar(cap, fonte, de_pts, ate_pts, sentido, pixel, None)
    }

    // CONTRATO: acréscimo. Cancelamento de quem lê (a corrida): `proximo` para de esperar e o ffmpeg morre.
    /// `abrir` com o sinal de cancelamento de quem lê: com ele aceso, `proximo` devolve None em até 20 ms e o
    /// processo é morto.
    pub fn abrir_com_cancelar(
        cap: &Capacidades,
        fonte: &FonteInfo,
        de_pts: i64,
        ate_pts: Option<i64>,
        sentido: Sentido,
        pixel: TamanhoPx,
        cancelar: Option<Arc<AtomicBool>>,
    ) -> Result<Self> {
        if pixel.w == 0 || pixel.h == 0 || pixel.w > LADO_MAX || pixel.h > LADO_MAX {
            return Err(erro(codigo::ITEM_INVALIDO, format!("quadro {}x{}", pixel.w, pixel.h)));
        }
        let tab = fonte.quadros.clone();
        let n = tab.pts.len();
        // faixas nunca partem um grupo de pts repetido (o `trim` por pts pega o grupo inteiro)
        let j0 = tab.indice_do_pts(de_pts).ok_or_else(|| erro(codigo::INTERVALO, format!("pts {de_pts}")))?;
        let j0 = match sentido {
            Sentido::Frente => j0,
            Sentido::Tras => tab.pts.partition_point(|&p| p <= de_pts) - 1,
        };
        let j1 = match (sentido, ate_pts) {
            (Sentido::Frente, None) => n - 1,
            (Sentido::Tras, None) => 0,
            (Sentido::Frente, Some(a)) => tab.pts.partition_point(|&p| p <= a).checked_sub(1).ok_or_else(|| erro(codigo::INTERVALO, a))?,
            (Sentido::Tras, Some(a)) => tab.pts.partition_point(|&p| p < a),
        };
        let invertido = match sentido {
            Sentido::Frente => j1 < j0,
            Sentido::Tras => j1 > j0,
        };
        if invertido || j1 >= n {
            return Err(erro(codigo::INTERVALO, format!("{de_pts}..{ate_pts:?}")));
        }
        let cadeia_cor = cor::cadeia_fonte(fonte.matriz, fonte.faixa, &fonte.pix_fmt, fonte.exibido).ok_or_else(|| erro(codigo::PIXEL_NAO_SUPORTADO, &fonte.pix_fmt))?;
        let (tx, rx) = sync_channel::<Result<(i64, Vec<u8>)>>(FILA);
        let parar = Arc::new(AtomicBool::new(false));
        let processos = Arc::new(AtomicUsize::new(0));
        let (cap, fonte, p2, pr2) = (cap.clone(), fonte.clone(), parar.clone(), processos.clone());
        let fio = std::thread::Builder::new()
            .name("sicro-rastreio-leitor".into())
            .spawn(move || {
                let r = match sentido {
                    Sentido::Frente => ler_faixa(&cap, &fonte, &cadeia_cor, j0, j1, pixel, &p2, &pr2, &mut |pts, q| tx.send(Ok((pts, q))).is_ok()).map(|_| ()),
                    Sentido::Tras => (|| -> Result<()> {
                        let por_memoria = (MEMORIA_BLOCO / (pixel.w as usize * pixel.h as usize * 3)).max(1);
                        let bloco = BLOCO_TRAS.min(por_memoria);
                        let mut topo = j0;
                        loop {
                            let mut ja = topo.saturating_sub(bloco - 1).max(j1);
                            while ja > j1 && fonte.quadros.pts[ja - 1] == fonte.quadros.pts[ja] {
                                ja -= 1;
                            }
                            let mut lidos = Vec::with_capacity(topo - ja + 1);
                            ler_faixa(&cap, &fonte, &cadeia_cor, ja, topo, pixel, &p2, &pr2, &mut |pts, q| {
                                lidos.push((pts, q));
                                true
                            })?;
                            if p2.load(Ordering::SeqCst) {
                                return Ok(());
                            }
                            for x in lidos.into_iter().rev() {
                                if tx.send(Ok(x)).is_err() {
                                    return Ok(());
                                }
                            }
                            if ja == j1 {
                                return Ok(());
                            }
                            topo = ja - 1;
                        }
                    })(),
                };
                if let Err(e) = r {
                    if !p2.load(Ordering::SeqCst) {
                        let _ = tx.send(Err(e));
                    }
                }
            })
            .map_err(SicroError::Io)?;
        Ok(Self { rx: Some(rx), parar, externo: cancelar, fio: Some(fio), atual: Vec::new(), pixel, processos })
    }

    /// Processos do ffmpeg abertos até agora (um por bloco para trás).
    #[cfg(test)]
    pub(crate) fn processos(&self) -> usize {
        self.processos.load(Ordering::SeqCst)
    }
}

impl LeitorQuadros for LeitorPipe {
    fn proximo(&mut self) -> Result<Option<(i64, Imagem<'_>)>> {
        let Some(rx) = self.rx.as_ref() else { return Ok(None) };
        loop {
            if self.externo.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
                self.parar.store(true, Ordering::SeqCst);
                return Ok(None);
            }
            match rx.recv_timeout(OLHAR_CANCELAR) {
                Ok(Ok((pts, q))) => {
                    self.atual = q;
                    return Ok(Some((pts, Imagem { rgb: &self.atual, w: self.pixel.w, h: self.pixel.h })));
                }
                Ok(Err(e)) => return Err(e),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    // a thread que lê morreu em pânico: não é o fim da faixa
                    if self.fio.take().is_some_and(|f| f.join().is_err()) {
                        return Err(erro(ERRO_LEITURA_DIVERGENTE, "leitor em pânico"));
                    }
                    return Ok(None);
                }
            }
        }
    }
}

impl Drop for LeitorPipe {
    fn drop(&mut self) {
        tracing::debug!("rastreio: leitor fechado depois de {} processo(s) do ffmpeg", self.processos.load(Ordering::SeqCst));
        self.parar.store(true, Ordering::SeqCst);
        // sem quem receba, a thread que espera para mandar acorda e sai
        drop(self.rx.take());
        if let Some(f) = self.fio.take() {
            let _ = f.join();
        }
    }
}

/// Leitor que soma `desloc` ao pts (intermediário de origem: pts do arquivo + pts do primeiro quadro do clipe).
struct Deslocado {
    leitor: LeitorPipe,
    desloc: i64,
}

impl LeitorQuadros for Deslocado {
    fn proximo(&mut self) -> Result<Option<(i64, Imagem<'_>)>> {
        let d = self.desloc;
        Ok(self.leitor.proximo()?.map(|(p, q)| (p + d, q)))
    }
}

// ---------------------------------------------------------------- miniatura e eventos

/// JPEG de até `MINIATURA_MAX` px no lado maior, com a caixa (2 px, na cor do rastreio), em data URL.
pub(crate) fn miniatura(q: &Imagem, caixa: Option<Caixa>, cor: [u8; 3]) -> Option<String> {
    let lado = q.w.max(q.h).max(1) as u64;
    let red = |v: u32| if lado <= MINIATURA_MAX as u64 { v } else { ((v as u64 * MINIATURA_MAX as u64 + lado / 2) / lado).max(1) as u32 };
    let (w, h) = (red(q.w), red(q.h));
    let mut px = vec![0u8; w as usize * h as usize * 3];
    for y in 0..h as usize {
        let sy = ((2 * y + 1) * q.h as usize / (2 * h as usize)).min(q.h as usize - 1);
        for x in 0..w as usize {
            let sx = ((2 * x + 1) * q.w as usize / (2 * w as usize)).min(q.w as usize - 1);
            let o = (sy * q.w as usize + sx) * 3;
            px[(y * w as usize + x) * 3..][..3].copy_from_slice(q.rgb.get(o..o + 3)?);
        }
    }
    if let Some(c) = caixa {
        let lim = |v: f64, m: u32| (v.round().max(0.0) as u32).min(m.saturating_sub(1));
        let (x0, y0) = (lim(c.x * w as f64, w), lim(c.y * h as f64, h));
        let (x1, y1) = (lim((c.x + c.w) * w as f64, w), lim((c.y + c.h) * h as f64, h));
        let mut pinta = |x: u32, y: u32| px[(y as usize * w as usize + x as usize) * 3..][..3].copy_from_slice(&cor);
        for e in 0..2u32 {
            for x in x0..=x1 {
                pinta(x, (y0 + e).min(y1));
                pinta(x, y1.saturating_sub(e).max(y0));
            }
            for y in y0..=y1 {
                pinta((x0 + e).min(x1), y);
                pinta(x1.saturating_sub(e).max(x0), y);
            }
        }
    }
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, QUALIDADE_JPEG).encode(&px, w, h, image::ExtendedColorType::Rgb8).ok()?;
    Some(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(out)))
}

/// Andamento limitado a `AVANCOS_POR_S` (a miniatura só é feita quando o evento sai).
struct Limitador<'a> {
    emitir: &'a Emissor,
    tarefa: String,
    rastreio: String,
    cor: [u8; 3],
    ultimo: Option<Instant>,
}

impl Limitador<'_> {
    fn avanco(&mut self, sentido: Sentido, a: Avanco) {
        let agora = Instant::now();
        let intervalo = Duration::from_millis(1000 / AVANCOS_POR_S as u64);
        if self.ultimo.is_some_and(|u| agora.duration_since(u) < intervalo) {
            return;
        }
        let miniatura = miniatura(a.quadro, a.caixa, self.cor);
        (self.emitir)(EventoRastreio::Avanco(AvancoRastreio {
            tarefa_id: self.tarefa.clone(),
            rastreio_id: self.rastreio.clone(),
            sentido,
            pts: a.pts,
            caixa: a.caixa,
            conf: a.conf,
            estado: a.estado,
            feitos: a.feitos,
            miniatura,
        }));
        // contado da saída do evento: a miniatura demora o que demora e não aproxima dois eventos
        self.ultimo = Some(Instant::now());
    }
}

// ---------------------------------------------------------------- pedido

/// O que a corrida lê: o arquivo (`leitura`, com a tabela dos pts dele), a tabela dos pts do denso, a diferença entre
/// as duas (pts do denso = pts lido + `desloc`) e a faixa de quadros que pode percorrer.
struct Espaco {
    leitura: FonteInfo,
    tab: Arc<TabelaQuadros>,
    desloc: i64,
    faixa: (usize, usize),
}

fn espaco(t: &Trabalho, cap: &Capacidades, pasta_projeto: &Path, fonte: &FonteInfo) -> Result<Espaco> {
    if t.r.espaco == "original" {
        let n = fonte.quadros.pts.len();
        return Ok(Espaco { leitura: fonte.clone(), tab: fonte.quadros.clone(), desloc: 0, faixa: (0, n) });
    }
    // espaço de origem: o intermediário do clipe, que tem de ser o deste espaço
    let clipe = t.clipe.as_ref().ok_or_else(|| erro(codigo::RASTREIO_ESPACO, &t.r.id))?;
    if geometria::espaco_de_origem(&fonte.sha256, &clipe.base.efeitos) != t.r.espaco {
        return Err(erro(codigo::RASTREIO_ESPACO, &t.r.id));
    }
    let previa = loop {
        if t.cancelar.load(Ordering::SeqCst) {
            return Err(erro(ERRO_CANCELADO, "intermediário"));
        }
        match origem::pedir(&t.ws, &t.cache, pasta_projeto, cap, clipe, fonte)? {
            EstadoOrigem::Pronto { previa, .. } => break previa,
            EstadoOrigem::Fazendo { .. } => std::thread::sleep(Duration::from_millis(100)),
        }
    };
    let tab_clipe = crate::edicao::motor::efeitos::realce::tabela(&clipe.base.efeitos, &fonte.quadros).into_owned();
    let (fa, fb) = tempo::faixa_do_trecho(&tab_clipe, clipe.inicio_s, clipe.inicio_pts, clipe.fim_s, clipe.fim_pts)?;
    let p0 = tab_clipe.pts[fa];
    let pts: Vec<i64> = tab_clipe.pts[fa..fb].iter().map(|p| p - p0).collect();
    // o intermediário começa em zero, na base da tabela do clipe, GOP fixo (origem.rs); busca do §5.3 como na fonte
    // (o MP4 volta ao quadro-chave real ≤ alvo, e leitura que não confere é refeita sem busca)
    let tab_int = TabelaQuadros {
        tb: tab_clipe.tb,
        chave: (0..pts.len()).map(|j| j % GOP_INTERMEDIARIO == 0).collect(),
        pts,
        passo_mediano: tab_clipe.passo_mediano,
        passo: tab_clipe.passo,
        vfr: tab_clipe.vfr,
        origem: OrigemTabela::Decodificador,
        ffmpeg_sha256: cap.sha256.clone(),
        erros_decodificacao: 0,
        busca: BuscaTabela::Confiavel,
        declarado: tab_clipe.declarado.clone(),
        pacotes: (fb - fa) as u64,
        lacunas: Vec::new(),
    };
    let leitura = FonteInfo {
        caminho: t.cache.join(&previa.relativo),
        sha256: String::new(),
        inicio_s: 0.0,
        exibido: (previa.largura, previa.altura),
        sar: Racional { num: 1, den: 1 },
        giro: 0,
        entrelacado: false,
        ordem_campos: OrdemCampos::Auto,
        pix_fmt: "yuv420p".into(),
        matriz: Matriz::Bt709,
        faixa: Faixa::Tv,
        quadros: Arc::new(tab_int),
        audio: None,
        conteiner: "mov,mp4,m4a,3gp,3g2,mj2".into(),
        codec: "h264".into(),
        tamanho: 0,
    };
    Ok(Espaco { leitura, tab: Arc::new(tab_clipe), desloc: p0, faixa: (fa, fb) })
}

/// Tudo o que a thread da corrida leva.
struct Trabalho {
    ws: PathBuf,
    cache: PathBuf,
    ai: AiSettings,
    p: ProjetoEdicao,
    r: Rastreio,
    clipe: Option<ClipeTrecho>,
    pedido: PedidoSeguir,
    tarefa: String,
    cancelar: Arc<AtomicBool>,
}

fn trecho(p: &ProjetoEdicao, clipe_id: &str) -> Option<ClipeTrecho> {
    p.principal.iter().find_map(|c| match c {
        Tolerante::Conhecido(ClipePrincipal::Trecho(t)) if t.base.id == clipe_id => Some(t.clone()),
        _ => None,
    })
}

/// Começa a corrida em segundo plano e devolve o `tarefa_id`. Outra corrida em andamento = `rastreio_em_curso`.
/// Não grava nada no projeto: o arquivo denso novo (e as silhuetas) vão para a pasta do projeto e o ponteiro sai no
/// evento final. `projeto` é o documento da tela (D2).
pub fn pedir(ws: &Path, cache: &Path, ai: &AiSettings, projeto: &Value, pedido: &PedidoSeguir, emitir: Emissor) -> Result<String> {
    let p = armazem::tipado(projeto)?;
    if !armazem::id_valido(&p.id) || !armazem::id_valido(&pedido.rastreio_id) {
        return Err(erro(codigo::ITEM_INVALIDO, "id"));
    }
    let r = p.rastreios.iter().find(|r| r.id == pedido.rastreio_id).cloned().ok_or_else(|| erro(codigo::RASTREIO_AUSENTE, &pedido.rastreio_id))?;
    let midia = r.midia.clone().ok_or_else(|| erro(codigo::RASTREIO_BASE, &r.id))?;
    let clipe = trecho(&p, &pedido.clipe_id);
    if clipe.as_ref().is_some_and(|c| c.midia != midia) || (clipe.is_none() && r.espaco != "original") {
        return Err(erro(codigo::ITEM_INVALIDO, format!("clipe {}", pedido.clipe_id)));
    }
    let a = &pedido.ancora;
    let finitos = [a.v.x, a.v.y, a.v.w, a.v.h, a.t].iter().all(|v| v.is_finite());
    if !(finitos && a.v.w > 0.0 && a.v.h > 0.0) || pedido.ate_s.is_some_and(|t| !t.is_finite()) {
        return Err(erro(codigo::REGIAO, &r.id));
    }
    let ocupado = || SicroError::Validation(codigo::RASTREIO_EM_CURSO.into());
    if EM_CURSO.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
        return Err(ocupado());
    }
    // o motor é escolhido antes: falta de modelo ou interruptor desligado voltam para a tela na hora
    let motor = escolher(ai, pedido.motor)?;
    let tarefa = uuid::Uuid::new_v4().to_string();
    let cancelar = Arc::new(AtomicBool::new(false));
    {
        let mut g = EM_CURSO.lock().unwrap_or_else(|e| e.into_inner());
        if g.is_some() {
            return Err(ocupado());
        }
        *g = Some(EmCurso { tarefa: tarefa.clone(), cancelar: cancelar.clone() });
    }
    let vez = Vez(tarefa.clone());
    let t = Trabalho { ws: ws.to_path_buf(), cache: cache.to_path_buf(), ai: ai.clone(), p, r, clipe, pedido: pedido.clone(), tarefa: tarefa.clone(), cancelar };
    std::thread::Builder::new()
        .name("sicro-rastreio".into())
        .spawn(move || {
            let _vez = vez;
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| trabalhar(&t, motor, &emitir)));
            let ev = match r {
                Ok(Ok(fim)) => EventoRastreio::Fim(fim),
                // cancelada antes da primeira corrida (tabela, intermediário): fim sem nada novo
                Ok(Err(e)) if executar::codigo_de(&e) == Some(ERRO_CANCELADO) => EventoRastreio::Fim(FimRastreio {
                    tarefa_id: t.tarefa.clone(),
                    rastreio_id: t.r.id.clone(),
                    parou: sentidos(t.pedido.sentido).iter().map(|&s| ParouSentido { sentido: s, motivo: Motivo::Cancelado, pts: None }).collect(),
                    denso: None,
                    silhuetas: None,
                }),
                Ok(Err(e)) => falha(&t, e),
                Err(_) => falha(&t, erro(codigo::PANICO, "rastreio")),
            };
            // a vez sai antes do evento: quem recebe o fim pode pedir outra corrida
            drop(_vez);
            emitir(ev);
        })
        .map_err(SicroError::Io)?;
    Ok(tarefa)
}

fn falha(t: &Trabalho, e: SicroError) -> EventoRastreio {
    let mensagem = match mensagens::erro_de_comando(e) {
        SicroError::Validation(m) => m,
        outro => outro.to_string(),
    };
    tracing::warn!("rastreio {}: {mensagem}", t.r.id);
    EventoRastreio::Falhou(FalhaRastreio { tarefa_id: t.tarefa.clone(), rastreio_id: t.r.id.clone(), mensagem })
}

/// As corridas de um pedido, na ordem: `ambos` = para a frente e depois para trás.
fn sentidos(p: SentidoPedido) -> &'static [Sentido] {
    match p {
        SentidoPedido::Frente => &[Sentido::Frente],
        SentidoPedido::Tras => &[Sentido::Tras],
        SentidoPedido::Ambos => &[Sentido::Frente, Sentido::Tras],
    }
}

/// Quadro (índice em `tab`) da âncora do perito.
fn quadro_da_ancora(tab: &TabelaQuadros, k: &Chave<Caixa>) -> usize {
    tab.do_par(k.t, k.pts)
}

fn trabalhar(t: &Trabalho, primeiro: Box<dyn Rastreador>, emitir: &Emissor) -> Result<FimRastreio> {
    let prep = executar::preparar(&t.ws, &t.p, &t.cache, false)?;
    let cap = prep.cap.clone();
    let midia = t.r.midia.as_deref().unwrap_or_default();
    let m = t.p.midias.iter().find(|m| m.id == midia).ok_or_else(|| erro(codigo::MIDIA_AUSENTE, midia))?;
    let mut fonte = prep.fontes.get(midia).cloned().ok_or_else(|| erro(codigo::MIDIA_AUSENTE, midia))?;
    // os pts do denso são os do decodificador (§3.1): sem a tabela dele, faz agora
    if fonte.quadros.origem != OrigemTabela::Decodificador {
        let tab = TabelaQuadros::do_decodificador(&cap, &fonte.caminho, &t.cancelar, &mut |_| {})?.guardar(&t.cache, &m.sha256)?;
        fonte = executar::fonte_info_memo(&cap, &fonte.caminho, &m.sha256, tab, m.cor.as_ref())?;
    }
    let esp = espaco(t, &cap, &prep.pasta_projeto, &fonte)?;
    let tab = esp.tab.clone();
    let (fa, fb) = esp.faixa;
    if fb <= fa {
        return Err(erro(codigo::INTERVALO, "clipe vazio"));
    }
    let j0 = quadro_da_ancora(&tab, &t.pedido.ancora);
    if j0 < fa || j0 >= fb {
        return Err(erro(codigo::INTERVALO, "âncora fora do clipe"));
    }
    let anc_pts = tab.pts[j0];
    let pixel = TamanhoPx { w: esp.leitura.exibido.0, h: esp.leitura.exibido.1 };
    let fps = {
        let r = tab.taxa_nominal();
        r.num as f64 / r.den as f64
    };
    // quadros das outras âncoras do perito
    let outras: Vec<usize> = t.r.ancoras.iter().map(|k| quadro_da_ancora(&tab, k)).filter(|&j| j != j0).collect();
    let pedido_ate = t.pedido.ate_s.map(|s| tab.exibido(s));
    let pasta = prep.pasta_projeto.clone();
    let mut base: Option<ArquivoDenso> = match &t.r.denso {
        Some(d) => Some(arquivo::ler(&pasta, &t.r.id, &d.sha256)?),
        None => None,
    };
    let mut base_sil: Option<ArquivoSilhuetas> = t.r.silhuetas.as_ref().and_then(|s| match silhueta::ler(&pasta, &t.r.id, &s.sha256) {
        Ok(x) => Some(x),
        Err(e) => {
            tracing::warn!("rastreio {}: silhuetas antigas ilegíveis ({e})", t.r.id);
            None
        }
    });
    let cor_rgb = cor::ler(&t.r.cor).map(|c| [c.r, c.g, c.b]).unwrap_or([255, 204, 0]);
    let mut lim = Limitador { emitir, tarefa: t.tarefa.clone(), rastreio: t.r.id.clone(), cor: cor_rgb, ultimo: None };
    let mut motor = Some(primeiro);
    let mut parou = Vec::new();
    let mut novo = false;
    for &s in sentidos(t.pedido.sentido) {
        if t.cancelar.load(Ordering::SeqCst) {
            parou.push(ParouSentido { sentido: s, motivo: Motivo::Cancelado, pts: None });
            continue;
        }
        let mut r = match motor.take() {
            Some(m) => m,
            None => escolher(&t.ai, t.pedido.motor)?,
        };
        // até a âncora seguinte do perito, o pedido da tela ou a borda do clipe
        let (ancora_seguinte, fim) = match s {
            Sentido::Frente => {
                let a = outras.iter().copied().filter(|&j| j > j0).min();
                (a, [a, pedido_ate, Some(fb - 1)].into_iter().flatten().min().unwrap_or(fb - 1).clamp(j0, fb - 1))
            }
            Sentido::Tras => {
                let a = outras.iter().copied().filter(|&j| j < j0).max();
                (a, [a, pedido_ate, Some(fa)].into_iter().flatten().max().unwrap_or(fa).clamp(fa, j0))
            }
        };
        let leitor = LeitorPipe::abrir_com_cancelar(&cap, &esp.leitura, anc_pts - esp.desloc, Some(tab.pts[fim] - esp.desloc), s, pixel, Some(t.cancelar.clone()))?;
        let mut leitor = Deslocado { leitor, desloc: esp.desloc };
        let pc = PedidoCorrida {
            rastreio: t.r.id.clone(),
            midia_sha256: m.sha256.clone(),
            espaco: t.r.espaco.clone(),
            pixel,
            ancora: Chave { t: tab.tempo(j0), pts: Some(anc_pts), v: t.pedido.ancora.v, interp: t.pedido.ancora.interp },
            sentido: s,
            ate_pts: Some(tab.pts[fim]),
            max_quadros: MAX_QUADROS,
            fps,
        };
        let corrida = laco::correr(&pc, &mut leitor, r.as_mut(), &t.cancelar, &mut |a| lim.avanco(s, a));
        drop(leitor);
        let mut res = match corrida {
            Ok(r) => r,
            // cancelada antes do quadro da âncora: nada deste lado; o outro lado (já feito) fica
            Err(e) if executar::codigo_de(&e) == Some(ERRO_CANCELADO) => {
                parou.push(ParouSentido { sentido: s, motivo: Motivo::Cancelado, pts: None });
                continue;
            }
            Err(e) => return Err(e),
        };
        if res.parou.motivo == Motivo::Fim && ancora_seguinte.is_some_and(|a| Some(tab.pts[a]) == res.parou.pts) {
            res.parou.motivo = Motivo::Ancora;
            res.corrida.parou = Motivo::Ancora;
        }
        parou.push(ParouSentido::de(s, res.parou));
        // cancelada antes de seguir um quadro: nada novo deste lado
        if res.parou.motivo == Motivo::Cancelado && res.corrida.de_pts == res.corrida.ate_pts {
            continue;
        }
        let mut d = arquivo::juntar(base.as_ref(), &res)?;
        (d.rastreio, d.midia_sha256, d.espaco, d.pixel) = (t.r.id.clone(), m.sha256.clone(), t.r.espaco.clone(), pixel);
        base_sil = arquivo::silhuetas_juntadas(&d, &res, base_sil.as_ref(), &res.corrida.motor.modelo.nome);
        base = Some(d);
        novo = true;
    }
    let (denso, silhuetas) = match (novo, base) {
        (true, Some(d)) => (Some(arquivo::gravar(&pasta, &d, tab.tb)?), base_sil.map(|s| silhueta::gravar(&pasta, &t.r.id, &s)).transpose()?),
        _ => (None, None),
    };
    Ok(FimRastreio { tarefa_id: t.tarefa.clone(), rastreio_id: t.r.id.clone(), parou, denso, silhuetas })
}

#[cfg(test)]
mod testes;
