//! Fila de render (§5.10) e registro das saídas (§4, decisão O7). Dono: P0.7.
//! A fila é do processo (não é campo de `AppState`): uma `Exportar`/`Refazer` por vez, em ordem;
//! `Previa` e `QuadroFinal` em faixa própria (a nova cancela a anterior do mesmo projeto).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::armazem;
use super::executar::{self, abrir_banco, codigo_de, sha256_bytes, sha_valido, Ambiente, Autor, Ciencia, Limitador, Progresso, Resultado};
use super::fundo::{self, texto_do_erro, TipoFundo};
use super::mensagens::codigo;
use super::modelo::{EscolhaCodificador, Predefinicao};
use super::motor::plano::Plano;
use super::motor::Alvo;
use super::relatorio::tecnico::{ConferenciaOcultacoes, Fecho};
use crate::database::repositories::{occurrence_repo, video_edit_output_repo, video_repo};
use crate::error::{Result, SicroError};
use crate::models::VideoEditOutput;
use crate::workspace::paths;

/// O que a tarefa produz.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TipoTarefa {
    Exportar,
    Previa,
    QuadroFinal,
    Refazer,
}

/// `Blocos`: blocos prontos e totais ("Vídeo 3/7").
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Fase {
    Previas,
    Camadas,
    Blocos { feitos: u32, total: u32 },
    Audio,
    Juncao,
    Conferindo,
    Relatorio,
    Pacote,
}

/// `Rodando`: slots feitos e totais, quadros por segundo e segundos que faltam.
/// `AConferir` e `Concluida`: exportação final registrada. `Pronta`: fim de `Previa` e `QuadroFinal`, com o arquivo
/// (relativo ao cache da Edição do caso). `Falhou`: mensagem curta classificada e o registro (relativo ao mesmo cache).
// CONTRATO: `Pronta` não está no §5.10; sem ele a prévia do render (Fase 0) não tinha como chegar à tela.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Estado {
    NaFila,
    Preparando,
    Rodando { fase: Fase, feito_q: u64, total_q: u64, qps: f32, restante_s: u32 },
    AConferir { saida_id: String },
    Concluida { saida_id: String },
    Pronta { relativo: String },
    Falhou { mensagem: String, log_rel: String },
    Cancelada,
}

impl Estado {
    /// Acabou de rodar (inclusive `AConferir`, que só espera o perito).
    pub(crate) fn acabou(&self) -> bool {
        !matches!(self, Estado::NaFila | Estado::Preparando | Estado::Rodando { .. })
    }

    /// Sai da lista no "limpar".
    fn terminada(&self) -> bool {
        matches!(self, Estado::Concluida { .. } | Estado::Pronta { .. } | Estado::Falhou { .. } | Estado::Cancelada)
    }
}

/// `caso` = id do caso; `criado_em` em RFC 3339.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tarefa {
    pub id: String,
    pub tipo: TipoTarefa,
    pub caso: String,
    pub projeto_id: String,
    pub titulo: String,
    pub estado: Estado,
    pub criado_em: String,
}

/// Evento `edicao:progresso`: `{ tarefa_id, fase, feito_q, total_q, qps, restante_s }`, até 5 por segundo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventoProgresso {
    pub tarefa_id: String,
    #[serde(flatten)]
    pub progresso: Progresso,
}

/// Nomes dos eventos. `edicao:fila` leva `Vec<Tarefa>` a cada mudança.
pub const EVENTO_FILA: &str = "edicao:fila";
pub const EVENTO_PROGRESSO: &str = "edicao:progresso";

/// Pedido de render. `projeto` = o documento da tela (vira retrato em `render/<tarefa>/projeto/`);
/// `ws` = raiz do caso; `cache` = pasta da Edição do caso; `ciencias` = códigos periciais de que o perito deu ciência.
#[derive(Debug, Clone, PartialEq)]
pub struct PedidoRender {
    pub tipo: TipoTarefa,
    pub ws: PathBuf,
    pub cache: PathBuf,
    pub projeto: Value,
    pub alvo: Alvo,
    pub ciencias: Vec<String>,
}

/// Quem recebe os eventos da fila (o integrador liga ao `AppHandle`; os testes usam um falso).
pub trait Ouvinte: Send + Sync {
    /// Lista resumida, a cada mudança.
    fn fila(&self, tarefas: &[Tarefa]);
    /// Avanço da tarefa que roda.
    fn progresso(&self, evento: &EventoProgresso);
}

/// Retrato do projeto de cada tarefa: `render/<tarefa>/projeto.json` (é onde `armazem::excluir` procura).
pub(crate) const RETRATO_TAREFA: &str = "projeto.json";
/// Registro da falha na pasta da tarefa.
const FALHA: &str = "falha.log";

/// Quem roda uma tarefa: o real compila e executa; os testes usam um falso. Devolve o estado final
/// (`AConferir`, `Concluida` ou `Pronta`); `Err` vira `Falhou` (ou `Cancelada`, se pediram).
pub(crate) trait Motor: Send + Sync {
    fn rodar(
        &self,
        tarefa: &Tarefa,
        pedido: &PedidoRender,
        trabalho: &Path,
        cancelar: &AtomicBool,
        progresso: &mut dyn FnMut(Progresso),
    ) -> Result<Estado>;
}

/// Exportação e refazer, uma por vez; prévia do render e quadro final fora dela, cada um na sua (o quadro final
/// não espera uma prévia longa).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Faixa {
    Exportacao,
    Previa,
    Quadro,
}

fn faixa(t: TipoTarefa) -> Faixa {
    match t {
        TipoTarefa::Exportar | TipoTarefa::Refazer => Faixa::Exportacao,
        TipoTarefa::Previa => Faixa::Previa,
        TipoTarefa::QuadroFinal => Faixa::Quadro,
    }
}

struct Item {
    tarefa: Tarefa,
    pedido: PedidoRender,
    cancelar: Arc<AtomicBool>,
    trabalho: PathBuf,
    terminada: Option<Instant>,
}

impl Item {
    /// Estado final: o documento (pode ser grande) não fica na memória até o "limpar".
    fn fechar(&mut self, e: Estado) {
        self.tarefa.estado = e;
        self.terminada = Some(Instant::now());
        self.pedido.projeto = Value::Null;
    }
}

/// Prévia ou quadro final terminado há mais que isso sai da lista quando chega outro do mesmo projeto
/// (a tela pede um quadro final a cada pausa: sem isso a lista e os eventos crescem a sessão inteira).
const MANTER_PRONTA: Duration = Duration::from_secs(60);

#[derive(Default)]
struct Interno {
    itens: Vec<Item>,
    fim: bool,
    iniciados: bool,
}

/// A fila de render; o processo tem uma (`global()`), os testes criam as suas com um `Motor` falso.
pub(crate) struct Fila {
    est: Mutex<Interno>,
    cv: Condvar,
    motor: Arc<dyn Motor>,
    ouvinte: Mutex<Option<Arc<dyn Ouvinte>>>,
    lim: Mutex<Limitador>,
    trabalhadores: Mutex<Vec<JoinHandle<()>>>,
    manter: Duration,
}

fn agora() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn erro(t: impl Into<String>) -> SicroError {
    SicroError::Validation(t.into())
}

impl Fila {
    pub(crate) fn nova(motor: Arc<dyn Motor>) -> Arc<Self> {
        Self::nova_com(motor, MANTER_PRONTA)
    }

    pub(crate) fn nova_com(motor: Arc<dyn Motor>, manter: Duration) -> Arc<Self> {
        Arc::new(Fila {
            est: Mutex::new(Interno::default()),
            cv: Condvar::new(),
            motor,
            ouvinte: Mutex::new(None),
            lim: Mutex::new(Limitador::default()),
            trabalhadores: Mutex::new(Vec::new()),
            manter,
        })
    }

    fn trava(&self) -> MutexGuard<'_, Interno> {
        self.est.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn lista(g: &Interno) -> Vec<Tarefa> {
        g.itens.iter().map(|i| i.tarefa.clone()).collect()
    }

    fn ouvinte(&self) -> Option<Arc<dyn Ouvinte>> {
        self.ouvinte.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Avisa fora da trava (quem ouve pode chamar `listar`).
    fn emitir(&self, lista: Vec<Tarefa>) {
        if let Some(o) = self.ouvinte() {
            o.fila(&lista);
        }
    }

    pub(crate) fn ouvir(&self, o: Arc<dyn Ouvinte>) {
        *self.ouvinte.lock().unwrap_or_else(|e| e.into_inner()) = Some(o);
    }

    fn iniciar(self: &Arc<Self>, g: &mut Interno) {
        if g.iniciados {
            return;
        }
        g.iniciados = true;
        g.fim = false;
        let mut hs = self.trabalhadores.lock().unwrap_or_else(|e| e.into_inner());
        for (nome, f) in [("edicao-render", Faixa::Exportacao), ("edicao-previa", Faixa::Previa), ("edicao-quadro", Faixa::Quadro)] {
            let eu = self.clone();
            match std::thread::Builder::new().name(nome.into()).spawn(move || eu.trabalhar(f)) {
                Ok(h) => hs.push(h),
                Err(e) => tracing::error!("edição: trabalhador da fila: {e}"),
            }
        }
    }

    pub(crate) fn enfileirar(self: &Arc<Self>, pedido: PedidoRender) -> Result<String> {
        // o id vira pasta (projeto, `.trf`, retrato): texto da tela não passa sem conferir
        if !pedido.projeto.get("id").and_then(Value::as_str).is_some_and(armazem::id_valido) {
            return Err(erro(codigo::ESQUEMA));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let trabalho = pedido.cache.join("render").join(&id);
        fs::create_dir_all(&trabalho)?;
        // retrato: editar depois não muda a tarefa (e `armazem::excluir` acha o cache do projeto por ele)
        fs::write(trabalho.join(RETRATO_TAREFA), serde_json::to_vec_pretty(&pedido.projeto)?)?;
        let texto = |k: &str| pedido.projeto.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let tarefa = Tarefa {
            id: id.clone(),
            tipo: pedido.tipo,
            caso: pedido.cache.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            projeto_id: texto("id"),
            titulo: texto("titulo"),
            estado: Estado::NaFila,
            criado_em: agora(),
        };
        let mut g = self.trava();
        let mut sem_rodar: Vec<(PathBuf, String)> = Vec::new();
        // a última vence: prévia (ou quadro final) nova cancela a anterior do mesmo tipo e projeto
        if faixa(pedido.tipo) != Faixa::Exportacao {
            let mesma = |i: &Item| i.tarefa.tipo == tarefa.tipo && i.tarefa.projeto_id == tarefa.projeto_id && i.tarefa.caso == tarefa.caso;
            let manter = self.manter;
            g.itens.retain(|i| !(mesma(i) && i.tarefa.estado.terminada() && i.terminada.is_some_and(|t| t.elapsed() >= manter)));
            for i in g.itens.iter_mut().filter(|i| mesma(i) && !i.tarefa.estado.acabou()) {
                i.cancelar.store(true, Ordering::SeqCst);
                if i.tarefa.estado == Estado::NaFila {
                    i.fechar(Estado::Cancelada);
                    sem_rodar.push((i.trabalho.clone(), i.tarefa.projeto_id.clone()));
                }
            }
        }
        g.itens.push(Item { tarefa, pedido, cancelar: Arc::new(AtomicBool::new(false)), trabalho, terminada: None });
        self.iniciar(&mut g);
        let lista = Self::lista(&g);
        drop(g);
        sem_rodar.iter().for_each(|(t, p)| enxugar(t, p));
        self.cv.notify_all();
        self.emitir(lista);
        Ok(id)
    }

    pub(crate) fn cancelar(&self, id: &str) -> Result<()> {
        let mut g = self.trava();
        let it = g.itens.iter_mut().find(|i| i.tarefa.id == id).ok_or_else(|| erro(format!("tarefa_ausente: {id}")))?;
        if it.tarefa.estado.acabou() {
            return Ok(());
        }
        it.cancelar.store(true, Ordering::SeqCst);
        let mut sem_rodar = None;
        if it.tarefa.estado == Estado::NaFila {
            it.fechar(Estado::Cancelada);
            sem_rodar = Some((it.trabalho.clone(), it.tarefa.projeto_id.clone()));
        }
        let lista = Self::lista(&g);
        drop(g);
        if let Some((t, p)) = sem_rodar {
            enxugar(&t, &p);
        }
        self.cv.notify_all();
        self.emitir(lista);
        Ok(())
    }

    pub(crate) fn listar(&self) -> Vec<Tarefa> {
        Self::lista(&self.trava())
    }

    pub(crate) fn limpar(&self) -> Vec<Tarefa> {
        let mut g = self.trava();
        g.itens.retain(|i| !i.tarefa.estado.terminada());
        let lista = Self::lista(&g);
        drop(g);
        self.emitir(lista.clone());
        lista
    }

    pub(crate) fn esperar(&self, id: &str) -> Result<Estado> {
        let mut g = self.trava();
        loop {
            let e = g.itens.iter().find(|i| i.tarefa.id == id).map(|i| i.tarefa.estado.clone());
            match e {
                None => return Err(erro(format!("tarefa_ausente: {id}"))),
                Some(e) if e.acabou() => return Ok(e),
                Some(_) => g = self.cv.wait(g).unwrap_or_else(|e| e.into_inner()),
            }
        }
    }

    /// Há tarefa que ainda vai rodar (ou roda) desse projeto?
    pub(crate) fn projeto_ocupado(&self, caso: &str, projeto_id: &str) -> bool {
        self.trava().itens.iter().any(|i| i.tarefa.caso == caso && i.tarefa.projeto_id == projeto_id && !i.tarefa.estado.acabou())
    }

    /// A saída ainda está na fila (`AConferir`)?
    pub(crate) fn saida_na_fila(&self, saida_id: &str) -> bool {
        self.trava().itens.iter().any(|i| matches!(&i.tarefa.estado, Estado::AConferir { saida_id: s } if s == saida_id))
    }

    /// `AConferir` → `Concluida` depois do `edicao_conferir`.
    pub(crate) fn conferida(&self, saida_id: &str) {
        let mut g = self.trava();
        let mut mudou = false;
        for i in g.itens.iter_mut() {
            if matches!(&i.tarefa.estado, Estado::AConferir { saida_id: s } if s == saida_id) {
                i.tarefa.estado = Estado::Concluida { saida_id: saida_id.to_string() };
                mudou = true;
            }
        }
        let lista = Self::lista(&g);
        drop(g);
        if mudou {
            self.cv.notify_all();
            self.emitir(lista);
        }
    }

    pub(crate) fn encerrar(&self) {
        let lista = {
            let mut g = self.trava();
            g.fim = true;
            for i in g.itens.iter_mut().filter(|i| !i.tarefa.estado.acabou()) {
                i.cancelar.store(true, Ordering::SeqCst);
                if i.tarefa.estado == Estado::NaFila {
                    i.fechar(Estado::Cancelada);
                }
            }
            Self::lista(&g)
        };
        self.cv.notify_all();
        let hs: Vec<_> = std::mem::take(&mut *self.trabalhadores.lock().unwrap_or_else(|e| e.into_inner()));
        for h in hs {
            let _ = h.join();
        }
        let mut g = self.trava();
        g.iniciados = false;
        g.fim = false;
        drop(g);
        self.cv.notify_all();
        self.emitir(lista);
    }

    fn mudar(&self, id: &str, e: Estado) -> Vec<Tarefa> {
        let mut g = self.trava();
        if let Some(i) = g.itens.iter_mut().find(|i| i.tarefa.id == id) {
            if e.acabou() {
                i.fechar(e);
            } else {
                i.tarefa.estado = e;
            }
        }
        Self::lista(&g)
    }

    fn trabalhar(self: Arc<Self>, minha: Faixa) {
        loop {
            let (tarefa, pedido, cancelar, trabalho, lista) = {
                let mut g = self.trava();
                loop {
                    if g.fim {
                        return;
                    }
                    if let Some(i) = g.itens.iter_mut().find(|i| faixa(i.tarefa.tipo) == minha && i.tarefa.estado == Estado::NaFila) {
                        i.tarefa.estado = Estado::Preparando;
                        let t = (i.tarefa.clone(), i.pedido.clone(), i.cancelar.clone(), i.trabalho.clone());
                        let lista = Self::lista(&g);
                        break (t.0, t.1, t.2, t.3, lista);
                    }
                    g = self.cv.wait(g).unwrap_or_else(|e| e.into_inner());
                }
            };
            self.emitir(lista);
            let mut fase_anterior: Option<std::mem::Discriminant<Fase>> = None;
            let eu = &self;
            let id = tarefa.id.clone();
            let mut progresso = |p: Progresso| {
                let d = std::mem::discriminant(&p.fase);
                let lista = eu.mudar(
                    &id,
                    Estado::Rodando { fase: p.fase, feito_q: p.feito_q, total_q: p.total_q, qps: p.qps, restante_s: p.restante_s },
                );
                // a lista vai quando a fase muda; o avanço, no máximo 5 vezes por segundo
                if fase_anterior != Some(d) {
                    fase_anterior = Some(d);
                    eu.emitir(lista);
                }
                if eu.lim.lock().unwrap_or_else(|e| e.into_inner()).passa() {
                    if let Some(o) = eu.ouvinte() {
                        o.progresso(&EventoProgresso { tarefa_id: id.clone(), progresso: p });
                    }
                }
            };
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.motor.rodar(&tarefa, &pedido, &trabalho, &cancelar, &mut progresso)
            }));
            let final_ = match r {
                Ok(Ok(e)) if e.acabou() && !matches!(e, Estado::Falhou { .. } | Estado::Cancelada) => e,
                Ok(Ok(e)) => falhou(&trabalho, &pedido.cache, &format!("estado final inválido: {e:?}")),
                Ok(Err(_)) if cancelar.load(Ordering::SeqCst) => Estado::Cancelada,
                Ok(Err(e)) if codigo_de(&e) == Some(executar::ERRO_CANCELADO) => Estado::Cancelada,
                Ok(Err(e)) => falhou(&trabalho, &pedido.cache, &texto_do_erro(&e)),
                Err(_) => falhou(&trabalho, &pedido.cache, "panico"),
            };
            if final_ == Estado::Cancelada || (tarefa.tipo == TipoTarefa::QuadroFinal && matches!(final_, Estado::Pronta { .. })) {
                enxugar(&trabalho, &tarefa.projeto_id);
            }
            let lista = self.mudar(&tarefa.id, final_);
            self.cv.notify_all();
            self.emitir(lista);
        }
    }
}

/// Tarefa cancelada, ou quadro final pronto (o BMP mora em `quadro/`): da pasta de trabalho ficam só o id do
/// projeto e as chaves do cache, que é por onde a limpeza por projeto a acha. A tela pede um quadro final a cada
/// pausa: sem isso cada um deixava o documento inteiro e os registros no cache por 7 dias.
fn enxugar(trabalho: &Path, projeto_id: &str) {
    for e in fs::read_dir(trabalho).into_iter().flatten().flatten() {
        let n = e.file_name();
        if n == RETRATO_TAREFA || n == executar::CHAVES {
            continue;
        }
        let p = e.path();
        let _ = if p.is_dir() { fs::remove_dir_all(&p) } else { fs::remove_file(&p) };
    }
    if trabalho.is_dir() {
        let _ = fs::write(trabalho.join(RETRATO_TAREFA), serde_json::json!({ "id": projeto_id }).to_string());
    }
}

/// `Falhou` com a mensagem (código na frente) e o registro em `render/<tarefa>/falha.log`.
fn falhou(trabalho: &Path, cache: &Path, mensagem: &str) -> Estado {
    let log = trabalho.join(FALHA);
    let _ = fs::write(&log, format!("{mensagem}\n"));
    let log_rel = log.strip_prefix(cache).unwrap_or(&log).to_string_lossy().replace('\\', "/");
    Estado::Falhou { mensagem: mensagem.to_string(), log_rel }
}

fn global() -> &'static Arc<Fila> {
    static F: OnceLock<Arc<Fila>> = OnceLock::new();
    F.get_or_init(|| Fila::nova(Arc::new(MotorReal)))
}

/// Liga o ouvinte dos eventos (uma vez, na partida).
pub fn ouvir(ouvinte: Box<dyn Ouvinte>) {
    global().ouvir(Arc::from(ouvinte));
}

/// Enfileira e devolve o id da tarefa. Grava o retrato do projeto antes de voltar.
pub fn enfileirar(pedido: PedidoRender) -> Result<String> {
    global().enfileirar(pedido)
}

/// Cancela a tarefa (na fila ou rodando): mata os processos dela e apaga a saída parcial.
pub fn cancelar(tarefa_id: &str) -> Result<()> {
    global().cancelar(tarefa_id)
}

/// Tarefas atuais, na ordem de entrada.
pub fn listar() -> Vec<Tarefa> {
    global().listar()
}

/// Tira da lista as tarefas terminadas (concluída, falhou, cancelada) e devolve as que ficam.
pub fn limpar() -> Vec<Tarefa> {
    global().limpar()
}

/// Espera a tarefa terminar e devolve o estado final (quadro final e testes).
pub fn esperar(tarefa_id: &str) -> Result<Estado> {
    global().esperar(tarefa_id)
}

/// Cancela tudo e mata os filhos (chamada no `ExitRequested`).
pub fn encerrar() {
    // o sinal antes dos filhos: filho morto com o sinal já de pé é cancelamento, não falha
    global().encerrar();
    super::ffmpeg::exec::matar_todos();
}

/// Há tarefa por rodar (ou rodando) desse projeto? (`edicao_excluir` recusa.)
pub(crate) fn projeto_ocupado(caso: &str, projeto_id: &str) -> bool {
    global().projeto_ocupado(caso, projeto_id)
}

/// A saída ainda está na fila? (`edicao_saida_excluir` recusa.)
pub(crate) fn saida_na_fila(saida_id: &str) -> bool {
    global().saida_na_fila(saida_id)
}

// ---------------------------------------------------------------------------
// O motor de verdade: preparar, validar, compilar, executar e registrar

struct MotorReal;

impl Motor for MotorReal {
    fn rodar(
        &self,
        tarefa: &Tarefa,
        pedido: &PedidoRender,
        trabalho: &Path,
        cancelar: &AtomicBool,
        progresso: &mut dyn FnMut(Progresso),
    ) -> Result<Estado> {
        let p = armazem::tipado(&pedido.projeto)?;
        let final_ = matches!(pedido.alvo, Alvo::Final { .. });
        let prep = executar::preparar_com(&pedido.ws, &p, &pedido.cache, final_, final_.then_some(cancelar))?;
        let ctx = prep.contexto();
        if final_ {
            let linha = super::tempo::resolver(&p, &ctx);
            let problemas = super::validar::validar(&p, &linha, &ctx, Some(&pedido.alvo));
            let faltam = super::validar::sem_ciencia(&problemas, &pedido.ciencias);
            if !faltam.is_empty() {
                return Err(erro(format!("ciencia: {}", faltam.join(", "))));
            }
        }
        let auto = matches!(&pedido.alvo, Alvo::Final { exportacao } if exportacao.codificador == EscolhaCodificador::Auto);
        let amb = Ambiente {
            cap: prep.cap.clone(),
            trabalho: trabalho.to_path_buf(),
            cache: pedido.cache.clone(),
            caso: pedido.ws.clone(),
            pasta_projeto: prep.pasta_projeto.clone(),
            paralelos: executar::paralelos(auto && prep.cap.gpu.is_some()),
            identidade: identidade(&pedido.ws, &prep.cap),
            ciencias: pedido.ciencias.iter().map(|c| Ciencia { codigo: c.clone(), ciente_em: tarefa.criado_em.clone() }).collect(),
        };
        let compilar = |cpu: bool| {
            let mut alvo = pedido.alvo.clone();
            if let (true, Alvo::Final { exportacao }) = (cpu, &mut alvo) {
                exportacao.codificador = EscolhaCodificador::Cpu;
            }
            super::motor::compilar(&p, &ctx, &alvo)
        };
        let (plano, res) = executar::executar_com_queda(&compilar, &amb, cancelar, progresso)?;
        if res.queda_para_cpu.is_some() {
            // §5.6: a sonda de GPU é refeita depois de falha de render por GPU
            fundo::pedir(TipoFundo::Sonda, &fundo::chave(TipoFundo::Sonda, "gpu"), Box::new(|_, _| super::ffmpeg::sonda::testar_de_novo().map(|_| ())));
        }
        let rel = |a: &Path| a.strip_prefix(&pedido.cache).unwrap_or(a).to_string_lossy().replace('\\', "/");
        match &plano.alvo {
            Alvo::Final { .. } => {
                let s = publicar(&Publicar {
                    ws: &pedido.ws,
                    projeto: &pedido.projeto,
                    plano: &plano,
                    res: &res,
                    amb: &amb,
                    quando: chrono::Local::now(),
                    tecnico: &TecnicoReal { plano: &plano, res: &res, amb: &amb },
                })?;
                if let Some(raiz) = pedido.cache.parent() {
                    if !plano.proveniencia.ocultacoes.is_empty() {
                        executar::limpar_do_plano(&pedido.cache, &plano);
                        executar::limpar_do_projeto(&pedido.cache, &tarefa.projeto_id, &tarefa.id);
                    }
                    let vivas: Vec<String> = listar().into_iter().map(|t| t.id).collect();
                    executar::limpar_cache(raiz, executar::TETO_CACHE, &vivas);
                }
                Ok(match s.estado {
                    EstadoSaida::AConferir => Estado::AConferir { saida_id: s.id },
                    EstadoSaida::Concluida => Estado::Concluida { saida_id: s.id },
                })
            }
            Alvo::Previa { .. } | Alvo::Quadro { .. } => Ok(Estado::Pronta { relativo: rel(&res.arquivo) }),
        }
    }
}

/// Quem, onde e com quê (`execucao.autor`, `caso`, `sicro`, `sistema`), lido do caso e da máquina.
pub(crate) fn identidade(ws: &Path, cap: &super::ffmpeg::sonda::Capacidades) -> executar::Identidade {
    let occ = abrir_banco(ws).ok().and_then(|(c, o)| occurrence_repo::find_by_id(&c, &o).ok().flatten());
    let (autor, caso) = match occ {
        Some(o) => (
            Autor { nome: o.peritos.first().cloned().unwrap_or_default(), ..Default::default() },
            executar::RetratoCaso {
                numero: o.protocolo.clone().or(o.numero_bo.clone()).unwrap_or_default(),
                referencia: o.titulo.clone().unwrap_or_default(),
                orgao: o.delegacia.clone().unwrap_or_default(),
                requisitante: o.oficio.clone().or(o.requisicao.clone()).unwrap_or_default(),
            },
        ),
        None => Default::default(),
    };
    executar::Identidade {
        autor,
        caso,
        sicro: executar::VersaoSicro {
            versao: crate::workspace::manifest::APP_VERSION.to_string(),
            commit: option_env!("SICRO_COMMIT").unwrap_or("").to_string(),
            alvo: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
        },
        sistema: executar::Sistema {
            so: std::env::consts::OS.to_string(),
            cpu: executar::Cpu { modelo: modelo_da_cpu(), instrucoes: instrucoes_da_cpu() },
            gpu: cap.gpu.as_ref().map(|g| g.nome.clone()),
            driver: cap.gpu.as_ref().map(|g| g.driver.clone()),
        },
    }
}

fn modelo_da_cpu() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|t| t.lines().find(|l| l.starts_with("model name")).and_then(|l| l.split_once(':')).map(|(_, v)| v.trim().to_string()))
        .unwrap_or_default()
}

/// Conjuntos que mudam os bytes do x264 e do AAC: definem a classe de máquina (§5.7).
fn instrucoes_da_cpu() -> Vec<String> {
    #[allow(unused_mut)]
    let mut v: Vec<String> = Vec::new();
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let todas: [(&str, bool); 8] = [
            ("sse4.2", std::arch::is_x86_feature_detected!("sse4.2")),
            ("avx", std::arch::is_x86_feature_detected!("avx")),
            ("avx2", std::arch::is_x86_feature_detected!("avx2")),
            ("fma", std::arch::is_x86_feature_detected!("fma")),
            ("avx512f", std::arch::is_x86_feature_detected!("avx512f")),
            ("avx512bw", std::arch::is_x86_feature_detected!("avx512bw")),
            ("avx512vl", std::arch::is_x86_feature_detected!("avx512vl")),
            ("avx512dq", std::arch::is_x86_feature_detected!("avx512dq")),
        ];
        v.extend(todas.iter().filter(|(_, s)| *s).map(|(n, _)| n.to_string()));
    }
    v
}

// ---------------------------------------------------------------------------
// Saídas registradas e retrato da exportação

/// Coluna `estado` de `video_edit_outputs` (um tipo só, o do banco: os literais serde são os mesmos).
// CONTRATO: era um enum próprio; reexportado de `models` pela regra da junção da leva 1 (contratos (e)).
pub use crate::models::EstadoSaida;

/// Linha de `video_edit_outputs` (migração 027), como a tela a recebe. Caminhos (`pasta`, `arquivo`, `tecnico_rel`,
/// `retrato_rel`) relativos ao caso; `tamanho` em bytes; datas em RFC 3339.
/// `decodificado_sha256`, `audio_sha256` e `tecnico_rel` None enquanto não existem (`tecnico_rel` só sai depois de `edicao_conferir`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SaidaRegistrada {
    pub id: String,
    pub projeto_id: String,
    pub titulo: String,
    pub pasta: String,
    pub arquivo: String,
    pub sha256: String,
    pub composto_sha256: String,
    pub decodificado_sha256: Option<String>,
    pub audio_sha256: Option<String>,
    pub tamanho: u64,
    pub duracao_s: f64,
    pub n_quadros: u64,
    pub predefinicao: Predefinicao,
    pub codificador: String,
    pub tecnico_rel: Option<String>,
    pub retrato_rel: String,
    pub projeto_sha256: String,
    pub autor: Autor,
    pub refeita_de: Option<String>,
    pub estado: EstadoSaida,
    /// Tem ocultação ou silêncio. Concluída com `exige_conferencia` e sem `conferida_por` = não conferida.
    pub exige_conferencia: bool,
    pub conferida_por: Option<String>,
    pub conferida_em: Option<String>,
    pub criado_em: String,
}

impl From<VideoEditOutput> for SaidaRegistrada {
    fn from(s: VideoEditOutput) -> Self {
        SaidaRegistrada {
            id: s.id.to_string(),
            projeto_id: s.projeto_id,
            titulo: s.titulo,
            pasta: s.pasta,
            arquivo: s.arquivo,
            sha256: s.sha256,
            composto_sha256: s.composto_sha256.unwrap_or_default(),
            decodificado_sha256: s.decodificado_sha256,
            audio_sha256: s.audio_sha256,
            tamanho: s.tamanho,
            duracao_s: s.duracao_s,
            n_quadros: s.n_quadros.max(0) as u64,
            // literal desconhecido (versão futura) não derruba a lista
            predefinicao: serde_json::from_value(Value::String(s.predefinicao)).unwrap_or(Predefinicao::Personalizada),
            codificador: s.codificador,
            tecnico_rel: s.tecnico_rel,
            retrato_rel: s.retrato_rel,
            projeto_sha256: s.projeto_sha256,
            autor: serde_json::from_value(s.autor).unwrap_or_default(),
            refeita_de: s.refeita_de.map(|u| u.to_string()),
            estado: s.estado,
            exige_conferencia: s.exige_conferencia,
            conferida_por: s.conferida_por,
            conferida_em: s.conferida_em.map(|d| d.to_rfc3339()),
            criado_em: s.criado_em.to_rfc3339(),
        }
    }
}

/// O que é o arquivo guardado no retrato.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PapelRetrato {
    Denso,
    Silhuetas,
    Trf,
    Midia,
}

/// Um arquivo que o retrato cita: guardado em `dados/<sha256>.<ext>` (ligação física, ou cópia quando o sistema
/// de arquivos não deixa). `de` = caminho de origem relativo à pasta do projeto; `tamanho` em bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemManifesto {
    pub papel: PapelRetrato,
    pub sha256: String,
    pub arquivo: String,
    pub de: String,
    pub tamanho: u64,
}

/// `manifesto.json` do retrato autossuficiente (O7): `exportacoes/<saida_id>/` tem `projeto.json`, este manifesto,
/// `grafos/` e `dados/`. O "refazer" lê só daqui. `alvo` = `Plano.alvo` da exportação (o "refazer" recompila com ele).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifesto {
    pub esquema: u32,
    pub saida_id: String,
    pub projeto_id: String,
    pub projeto_sha256: String,
    pub alvo: Alvo,
    pub itens: Vec<ItemManifesto>,
}

const ESQUEMA_MANIFESTO: u32 = 1;
const EXPORTACOES: &str = "exportacoes";
/// Arquivos do retrato além do projeto e do manifesto.
pub(crate) const COMPOSTOS: &str = "quadros.compostos.sha256";
pub(crate) const PENDENTE: &str = "tecnico.pendente.json";

fn pasta_retrato(ws: &Path, projeto_id: &str, saida_id: &str) -> Result<PathBuf> {
    if !armazem::id_valido(projeto_id) || !armazem::id_valido(saida_id) {
        return Err(erro(codigo::ESQUEMA));
    }
    Ok(armazem::pasta_projeto(ws, projeto_id).join(EXPORTACOES).join(saida_id))
}

/// Liga (ou copia) `origem` em `destino` sem nunca sobrescrever e devolve o SHA-256 e o tamanho do que ficou.
fn ligar_ou_copiar(origem: &Path, destino: &Path) -> Result<(String, u64)> {
    if fs::hard_link(origem, destino).is_err() {
        let mut de = fs::File::open(origem)?;
        let mut para = fs::OpenOptions::new().write(true).create_new(true).open(destino)?;
        std::io::copy(&mut de, &mut para)?;
        para.sync_all()?;
    }
    let sha = crate::hashing::sha256::sha256_file(destino)?;
    Ok((sha, fs::metadata(destino)?.len()))
}

/// O que o projeto cita (densos, silhuetas, `.trf`, mídia própria), lido do JSON cru: item de versão futura também vale.
/// (papel, sha256, origem absoluta, `de` relativo à pasta do projeto).
fn citados(ws: &Path, projeto_id: &str, projeto: &Value) -> Vec<(PapelRetrato, String, PathBuf, String)> {
    let pasta = armazem::pasta_projeto(ws, projeto_id);
    let mut v: Vec<(PapelRetrato, String, PathBuf, String)> = Vec::new();
    let mut por = |papel, sha: &str, de: String, origem: PathBuf| {
        if !v.iter().any(|(p, s, _, _)| *p == papel && s == sha) {
            v.push((papel, sha.to_string(), origem, de));
        }
    };
    for r in projeto.get("rastreios").and_then(Value::as_array).into_iter().flatten() {
        let Some(id) = r.get("id").and_then(Value::as_str).filter(|i| armazem::id_valido(i)) else { continue };
        for (campo, sub, ext, papel) in [("denso", "rastreios", "json", PapelRetrato::Denso), ("silhuetas", "silhuetas", "sil", PapelRetrato::Silhuetas)] {
            if let Some(sha) = r.get(campo).and_then(|d| d.get("sha256")).and_then(Value::as_str).filter(|s| sha_valido(s)) {
                let de = format!("{sub}/{id}/{sha}.{ext}");
                por(papel, sha, de.clone(), pasta.join(&de));
            }
        }
    }
    let mut trfs = Vec::new();
    trf_em(projeto, &mut trfs);
    for sha in trfs {
        let de = format!("vidstab/{sha}.trf");
        por(PapelRetrato::Trf, &sha, de.clone(), pasta.join(&de));
    }
    let prefixo = format!("{}/{projeto_id}/", paths::REL_EDICAO);
    for m in projeto.get("midias").and_then(Value::as_array).into_iter().flatten() {
        if m.get("origem").and_then(Value::as_str) != Some("projeto") {
            continue;
        }
        let rel = m.get("relativo").and_then(Value::as_str).filter(|r| executar::relativo_seguro(r));
        let (Some(rel), Some(sha)) = (rel, m.get("sha256").and_then(Value::as_str).filter(|s| sha_valido(s))) else {
            continue;
        };
        let de = rel.strip_prefix(&prefixo).unwrap_or(rel).to_string();
        por(PapelRetrato::Midia, sha, de, paths::resolver_relativo(ws, rel));
    }
    v
}

/// `trf_sha256` de todo `estabilizar` do documento, em qualquer profundidade.
fn trf_em(v: &Value, saida: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            if o.get("tipo").and_then(Value::as_str) == Some("estabilizar") {
                if let Some(s) = o.get("trf_sha256").and_then(Value::as_str).filter(|s| sha_valido(s)) {
                    if !saida.iter().any(|x| x == s) {
                        saida.push(s.to_string());
                    }
                }
            }
            o.values().for_each(|x| trf_em(x, saida));
        }
        Value::Array(a) => a.iter().for_each(|x| trf_em(x, saida)),
        _ => {}
    }
}

/// Grava o retrato em `Sistema/videos/edicao/<projeto_id>/exportacoes/<saida_id>/`: o projeto como chegou,
/// os grafos e listas do plano e tudo que o projeto cita (densos, silhuetas, `.trf`, mídia própria).
pub fn gravar_retrato(ws: &Path, projeto_id: &str, saida_id: &str, projeto: &Value, plano: &Plano) -> Result<Manifesto> {
    gravar_retrato_com(ws, projeto_id, saida_id, projeto, plano, &[])
}

/// `gravar_retrato` com `.trf` feitos neste render (o documento ainda não os cita).
pub(crate) fn gravar_retrato_com(ws: &Path, projeto_id: &str, saida_id: &str, projeto: &Value, plano: &Plano, trf_novos: &[String]) -> Result<Manifesto> {
    let dir = pasta_retrato(ws, projeto_id, saida_id)?;
    if dir.exists() {
        return Err(erro(format!("retrato_existe: {saida_id}")));
    }
    fs::create_dir_all(dir.join("dados"))?;
    let r = (|| -> Result<Manifesto> {
        let bytes = serde_json::to_vec_pretty(projeto)?;
        let projeto_sha256 = sha256_bytes(&bytes);
        fs::write(dir.join("projeto.json"), &bytes)?;
        let mut lista = citados(ws, projeto_id, projeto);
        let pasta = armazem::pasta_projeto(ws, projeto_id);
        for sha in trf_novos.iter().filter(|s| sha_valido(s)) {
            if !lista.iter().any(|(p, s, _, _)| *p == PapelRetrato::Trf && s == sha) {
                let de = format!("vidstab/{sha}.trf");
                lista.push((PapelRetrato::Trf, sha.clone(), pasta.join(&de), de));
            }
        }
        let mut itens = Vec::with_capacity(lista.len());
        for (papel, sha, origem, de) in lista {
            if !origem.is_file() {
                let c = match papel {
                    PapelRetrato::Denso | PapelRetrato::Silhuetas => codigo::RASTREIO_AUSENTE,
                    _ => codigo::MIDIA_AUSENTE,
                };
                return Err(erro(format!("{c}: {de}")));
            }
            let ext = origem.extension().and_then(|e| e.to_str()).filter(|e| e.chars().all(|c| c.is_ascii_alphanumeric())).unwrap_or("bin").to_ascii_lowercase();
            let arquivo = format!("dados/{sha}.{ext}");
            let destino = dir.join(&arquivo);
            let (lido, tamanho) = if destino.is_file() {
                (crate::hashing::sha256::sha256_file(&destino)?, fs::metadata(&destino)?.len())
            } else {
                ligar_ou_copiar(&origem, &destino)?
            };
            if lido != sha {
                return Err(erro(format!("{}: {de}", codigo::MIDIA_TROCADA)));
            }
            itens.push(ItemManifesto { papel, sha256: sha, arquivo, de, tamanho });
        }
        gravar_grafos(&dir.join("grafos"), plano)?;
        let m = Manifesto {
            esquema: ESQUEMA_MANIFESTO,
            saida_id: saida_id.to_string(),
            projeto_id: projeto_id.to_string(),
            projeto_sha256,
            alvo: plano.alvo.clone(),
            itens,
        };
        fs::write(dir.join("manifesto.json"), serde_json::to_vec_pretty(&m)?)?;
        Ok(m)
    })();
    if r.is_err() {
        let _ = fs::remove_dir_all(&dir);
    }
    r
}

/// Grafos, listas e argumentos de cada passo (sem caminho absoluto: mídia vira `midia:<id>`).
fn gravar_grafos(dir: &Path, plano: &Plano) -> Result<()> {
    use super::motor::plano::{Juncao, Papel};
    use super::ffmpeg::args::gravar_texto;
    for b in &plano.blocos {
        let d = dir.join(format!("b{:03}", b.indice));
        let base = b.saida.rsplit_once('.').map(|(a, _)| a).unwrap_or("bloco");
        gravar_texto(&d, &format!("{base}.txt"), &b.grafo)?;
        for a in &b.auxiliares {
            gravar_texto(&d, &a.nome, &a.conteudo)?;
        }
    }
    if let Juncao::Concat { lista, .. } = &plano.juncao {
        gravar_texto(&dir.join("juncao"), &lista.nome, &lista.conteudo)?;
    }
    if let Some(a) = &plano.audio {
        let d = dir.join("audio");
        gravar_texto(&d, "audio.txt", &a.grafo)?;
        for x in &a.auxiliares {
            gravar_texto(&d, &x.nome, &x.conteudo)?;
        }
    }
    let entrada = |e: &super::motor::plano::Entrada| {
        let fonte = match &e.papel {
            Papel::Midia { id, .. } => format!("midia:{id}"),
            Papel::Camada { id } => format!("camada:{id}"),
            Papel::Imagem => "imagem".into(),
            Papel::Gerada => e.caminho.to_string_lossy().into_owned(),
        };
        serde_json::json!({ "antes": e.antes, "antes_sem_busca": e.antes_sem_busca, "fonte": fonte })
    };
    let argumentos = serde_json::json!({
        "blocos": plano.blocos.iter().map(|b| serde_json::json!({
            "indice": b.indice, "de_q": b.de_q, "n_q": b.n_q, "chave": b.chave, "saida": b.saida, "framehash": b.framehash,
            "entradas": b.entradas.iter().map(entrada).collect::<Vec<_>>(), "codificador": b.codificador,
        })).collect::<Vec<_>>(),
        "juncao": plano.juncao,
        "audio": plano.audio.as_ref().map(|a| serde_json::json!({ "globais": a.globais, "entradas": a.entradas.iter().map(entrada).collect::<Vec<_>>() })),
        "previas": plano.previas,
    });
    fs::write(dir.join("argumentos.json"), serde_json::to_vec_pretty(&argumentos)?)?;
    Ok(())
}

/// Lê o retrato e confere o SHA-256 de cada item.
pub fn ler_retrato(ws: &Path, projeto_id: &str, saida_id: &str) -> Result<(Value, Manifesto)> {
    let dir = pasta_retrato(ws, projeto_id, saida_id)?;
    let m: Manifesto = serde_json::from_slice(&fs::read(dir.join("manifesto.json"))?)?;
    let bytes = fs::read(dir.join("projeto.json"))?;
    if sha256_bytes(&bytes) != m.projeto_sha256 || m.saida_id != saida_id || m.projeto_id != projeto_id {
        return Err(erro(format!("{}: projeto.json do retrato", codigo::MIDIA_TROCADA)));
    }
    for it in &m.itens {
        let nome = it.arquivo.strip_prefix("dados/").filter(|n| n.starts_with(&it.sha256) && !n.contains(['/', '\\']) && !n.contains(".."));
        let Some(nome) = nome else { return Err(erro(format!("{}: {}", codigo::ITEM_INVALIDO, it.arquivo))) };
        let arq = dir.join("dados").join(nome);
        let ok = arq.is_file()
            && fs::metadata(&arq)?.len() == it.tamanho
            && crate::hashing::sha256::sha256_file(&arq)? == it.sha256;
        if !ok {
            return Err(erro(format!("{}: {}", codigo::MIDIA_TROCADA, it.de)));
        }
    }
    Ok((serde_json::from_slice(&bytes)?, m))
}

/// Saídas registradas do caso (as de um projeto quando `projeto_id` é Some), da mais nova para a mais antiga.
pub fn saidas(ws: &Path, projeto_id: Option<&str>) -> Result<Vec<SaidaRegistrada>> {
    let (conn, occ) = abrir_banco(ws)?;
    let linhas = match projeto_id {
        Some(p) => video_edit_output_repo::listar_do_projeto(&conn, &occ, p)?,
        None => video_edit_output_repo::listar_do_caso(&conn, &occ)?,
    };
    Ok(linhas.into_iter().map(SaidaRegistrada::from).collect())
}

/// Fecha uma saída em "A conferir": grava `conferida_por`/`conferida_em` (ou "não conferida"), gera o `tecnico.json`
/// e marca concluída. `por` = nome do perito.
pub fn conferir_saida(ws: &Path, saida_id: &str, conferida: bool, por: &str) -> Result<SaidaRegistrada> {
    let s = concluir_conferencia(ws, saida_id, conferida, por, &GravarReal)?;
    global().conferida(saida_id);
    Ok(s)
}

// ---------------------------------------------------------------------------
// Publicação da exportação final

/// O relatório técnico visto pela publicação: o real é o do P0.R (`relatorio::tecnico`); os testes usam um falso.
pub(crate) trait Tecnico {
    fn montar(&self, fecho: &Fecho) -> Result<Value>;
    fn gravar(&self, pasta: &Path, titulo: &str, tecnico: &Value) -> Result<String>;
    fn bytes(&self, tecnico: &Value) -> Vec<u8>;
}

struct TecnicoReal<'a> {
    plano: &'a Plano,
    res: &'a Resultado,
    amb: &'a Ambiente,
}

impl Tecnico for TecnicoReal<'_> {
    fn montar(&self, fecho: &Fecho) -> Result<Value> {
        super::relatorio::tecnico::montar(self.plano, self.res, self.amb, fecho)
    }
    fn gravar(&self, pasta: &Path, titulo: &str, tecnico: &Value) -> Result<String> {
        super::relatorio::tecnico::gravar(pasta, titulo, tecnico)
    }
    fn bytes(&self, tecnico: &Value) -> Vec<u8> {
        super::relatorio::tecnico::bytes(tecnico)
    }
}

/// Só grava (a conferência completa um JSON já montado).
pub(crate) trait Gravar {
    fn gravar(&self, pasta: &Path, titulo: &str, tecnico: &Value) -> Result<String>;
}

struct GravarReal;

impl Gravar for GravarReal {
    fn gravar(&self, pasta: &Path, titulo: &str, tecnico: &Value) -> Result<String> {
        super::relatorio::tecnico::gravar(pasta, titulo, tecnico)
    }
}

pub(crate) struct Publicar<'a> {
    pub ws: &'a Path,
    pub projeto: &'a Value,
    pub plano: &'a Plano,
    pub res: &'a Resultado,
    pub amb: &'a Ambiente,
    /// Hora do nome da pasta (local).
    pub quando: chrono::DateTime<chrono::Local>,
    pub tecnico: &'a dyn Tecnico,
}

/// Nome dos arquivos da saída: o título saneado (`<Título>.mp4`, `<Título>.tecnico.json`).
fn nome_base(titulo: &str) -> String {
    let t = titulo.trim().trim_matches('.');
    if t.is_empty() {
        return "Edição".into();
    }
    // corta aqui, numa fronteira de caractere: o `truncate(100)` do `sanitize_folder_name` entra em pânico
    // quando o byte 100 cai no meio de uma letra acentuada
    let mut fim = t.len().min(TITULO_MAX);
    while !t.is_char_boundary(fim) {
        fim -= 1;
    }
    let base = crate::filesystem::safe_paths::sanitize_folder_name(&t[..fim]);
    // pasta e arquivo não terminam em espaço nem ponto (o Windows os descarta)
    let base = base.trim_end_matches([' ', '.']);
    if base.is_empty() { "Edição".into() } else { base.to_string() }
}

/// Bytes do título no nome da pasta e dos arquivos da saída.
const TITULO_MAX: usize = 100;

/// Cria `Exportações/Vídeos/Edições/<Título> <AAAA-MM-DD HHhMMmSS>[ (n)]/`: nunca reaproveita pasta.
fn criar_pasta_da_saida(ws: &Path, base: &str, quando: &chrono::DateTime<chrono::Local>) -> Result<(PathBuf, String)> {
    let raiz = paths::dir_edicao_saida(ws);
    fs::create_dir_all(&raiz)?;
    let nome = format!("{base} {}", quando.format("%Y-%m-%d %Hh%Mm%S"));
    for n in 1..=999u32 {
        let pasta = if n == 1 { nome.clone() } else { format!("{nome} ({n})") };
        match fs::create_dir(raiz.join(&pasta)) {
            Ok(()) => return Ok((raiz.join(&pasta), format!("{}/{pasta}", paths::REL_EDICAO_SAIDA))),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(erro(format!("pasta_existe: {nome}")))
}

fn somente_leitura(p: &Path) -> Result<()> {
    let mut perm = fs::metadata(p)?.permissions();
    perm.set_readonly(true);
    fs::set_permissions(p, perm)?;
    Ok(())
}

/// Tira o somente leitura de tudo na pasta e apaga a pasta.
fn apagar_pasta_da_saida(p: &Path) -> Result<()> {
    if !p.exists() {
        return Ok(());
    }
    gravavel(p);
    fs::remove_dir_all(p)?;
    Ok(())
}

/// Tira o somente leitura de `p` e, se for pasta, do que há dentro (o pacote do júri tem subpastas).
fn gravavel(p: &Path) {
    if let Ok(m) = fs::symlink_metadata(p) {
        if m.is_dir() {
            fs::read_dir(p).into_iter().flatten().flatten().for_each(|e| gravavel(&e.path()));
        }
        let mut perm = m.permissions();
        if perm.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            let _ = fs::set_permissions(p, perm);
        }
    }
}

/// Pasta de uma saída pelo `pasta` do banco: só vale DENTRO de `Exportações/Vídeos/Edições/` (um valor vazio ou
/// com `..` resolveria para o caso inteiro, e excluir a saída apagaria o caso).
fn pasta_da_saida(ws: &Path, pasta_rel: &str) -> Result<PathBuf> {
    let dentro = pasta_rel.strip_prefix(paths::REL_EDICAO_SAIDA).and_then(|r| r.strip_prefix('/')).filter(|r| {
        !r.is_empty() && !r.contains(['/', '\\']) && *r != "." && *r != ".."
    });
    match dentro {
        Some(nome) => Ok(paths::dir_edicao_saida(ws).join(nome)),
        None => Err(erro(format!("{}: pasta da saída", codigo::ITEM_INVALIDO))),
    }
}

/// Exportação final pronta: pasta nova da saída (`create_new`), retrato autossuficiente, `tecnico.json`
/// (ou `tecnico.pendente.json` no retrato quando há ocultação ou silêncio a conferir), somente leitura depois do
/// hash, linha em `video_edit_outputs`, log por mídia usada e auditoria.
pub(crate) fn publicar(p: &Publicar) -> Result<SaidaRegistrada> {
    let projeto_id = p.projeto.get("id").and_then(Value::as_str).filter(|i| armazem::id_valido(i)).ok_or_else(|| erro(codigo::ESQUEMA))?;
    let titulo = p.projeto.get("titulo").and_then(Value::as_str).unwrap_or("").to_string();
    let Alvo::Final { exportacao } = &p.plano.alvo else { return Err(erro(format!("{}: alvo", codigo::ITEM_INVALIDO))) };
    let prov = &p.plano.proveniencia;
    let exige_conferencia = !prov.ocultacoes.is_empty() || !prov.audio.silencios.is_empty();
    let (conn, occ) = abrir_banco(p.ws)?;
    let saida = uuid::Uuid::new_v4();
    let saida_id = saida.to_string();
    let base = nome_base(&titulo);
    let ext = p.res.arquivo.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let (pasta, pasta_rel) = criar_pasta_da_saida(p.ws, &base, &p.quando)?;
    let retrato = pasta_retrato(p.ws, projeto_id, &saida_id)?;
    let r = (|| -> Result<SaidaRegistrada> {
        let nome_video = if exige_conferencia { format!("{base} (ocultado).{ext}") } else { format!("{base}.{ext}") };
        let video = pasta.join(&nome_video);
        let (sha, tamanho) = ligar_ou_copiar(&p.res.arquivo, &video)?;
        if sha != p.res.sha256 {
            return Err(erro(format!("{}: cópia da saída", codigo::LEITURA_DIVERGENTE)));
        }
        // a ligação que fica na pasta de trabalho viraria somente leitura junto (no Windows a limpeza não a apagaria)
        if p.res.arquivo.starts_with(&p.amb.trabalho) {
            let _ = fs::remove_file(&p.res.arquivo);
        }
        somente_leitura(&video)?;
        for l in &p.res.legendas {
            let ext = l.extension().and_then(|e| e.to_str()).unwrap_or("srt");
            let destino = pasta.join(format!("{base}.{ext}"));
            ligar_ou_copiar(l, &destino)?;
            if l.starts_with(&p.amb.trabalho) {
                let _ = fs::remove_file(l);
            }
            somente_leitura(&destino)?;
        }
        let trf: Vec<String> = p.res.trf.iter().map(|t| t.sha256.clone()).collect();
        let m = gravar_retrato_com(p.ws, projeto_id, &saida_id, p.projeto, p.plano, &trf)?;
        if let Some(c) = &p.res.compostos {
            fs::copy(c, retrato.join(COMPOSTOS))?;
        }
        let arquivo_rel = format!("{pasta_rel}/{nome_video}");
        let fecho = Fecho {
            projeto_sha256: m.projeto_sha256.clone(),
            arquivo_rel: arquivo_rel.clone(),
            ocultacoes: exige_conferencia.then(|| ConferenciaOcultacoes { conferida: false, por: String::new(), em: String::new() }),
        };
        let tecnico = p.tecnico.montar(&fecho)?;
        let (estado, tecnico_rel) = if exige_conferencia {
            // sobrevive ao reinício: `edicao_conferir` completa a conferência e grava o final
            fs::write(retrato.join(PENDENTE), p.tecnico.bytes(&tecnico))?;
            (EstadoSaida::AConferir, None)
        } else {
            p.tecnico.gravar(&pasta, &base, &tecnico)?;
            let arq = pasta.join(format!("{base}.tecnico.json"));
            if arq.is_file() {
                somente_leitura(&arq)?;
            }
            (EstadoSaida::Concluida, Some(format!("{pasta_rel}/{base}.tecnico.json")))
        };
        let fps = p.plano.canvas.fps;
        let linha = VideoEditOutput {
            id: saida,
            occurrence_id: occ,
            projeto_id: projeto_id.to_string(),
            titulo: titulo.clone(),
            pasta: pasta_rel.clone(),
            arquivo: arquivo_rel.clone(),
            sha256: sha.clone(),
            composto_sha256: p.res.composto_sha256.clone(),
            decodificado_sha256: p.res.conferencia.as_ref().map(|c| c.decodificado_sha256.clone()),
            audio_sha256: p.res.audio_sha256.clone(),
            tamanho,
            duracao_s: p.plano.total_q as f64 * fps.den as f64 / fps.num.max(1) as f64,
            n_quadros: p.plano.total_q as i64,
            predefinicao: serde_json::to_value(exportacao.predefinicao)?.as_str().unwrap_or("").to_string(),
            codificador: p.res.codificador.clone(),
            tecnico_rel,
            retrato_rel: format!("{}/{projeto_id}/{EXPORTACOES}/{saida_id}", paths::REL_EDICAO),
            projeto_sha256: m.projeto_sha256.clone(),
            autor: serde_json::to_value(&p.amb.identidade.autor)?,
            refeita_de: None,
            estado,
            exige_conferencia,
            conferida_por: None,
            conferida_em: None,
            criado_em: chrono::Utc::now(),
        };
        // linha, logs e auditoria entram juntos ou não entram: falha no meio não deixa linha apontando para pasta apagada
        let tx = conn.unchecked_transaction()?;
        video_edit_output_repo::inserir(&tx, &linha)?;
        for md in &prov.midias {
            let (de, ate) = (md.usado.first().map_or(0.0, |u| u.de_s), md.usado.last().map_or(0.0, |u| u.ate_s));
            let d = serde_json::json!({ "saida_id": saida_id, "de_s": de, "ate_s": ate, "usado": md.usado });
            video_repo::insert_log(&tx, &occ, Some(&md.sha256), "edicao.uso", &d.to_string())?;
        }
        let d = serde_json::json!({
            "projeto_id": projeto_id, "titulo": titulo, "arquivo": arquivo_rel, "sha256": sha,
            "codificador": p.res.codificador, "queda_para_cpu": p.res.queda_para_cpu, "exige_conferencia": exige_conferencia,
        });
        occurrence_repo::record_audit(&tx, Some(&occ), "video.edicao.render", Some("video"), Some("edicao_saida"), Some(&saida), Some(&d.to_string()))?;
        tx.commit()?;
        // o PCM do programa (centenas de MB por hora) não fica 7 dias na pasta de trabalho
        if let Some(a) = p.plano.audio.as_ref().filter(|a| !a.saida.contains(['/', '\\']) && a.saida != "..") {
            let _ = fs::remove_file(p.amb.trabalho.join(&a.saida));
        }
        Ok(SaidaRegistrada::from(linha))
    })();
    if r.is_err() {
        let _ = apagar_pasta_da_saida(&pasta);
        let _ = fs::remove_dir_all(&retrato);
    }
    r
}

/// `edicao_conferir`: completa a conferência no `tecnico.pendente.json` do retrato, grava o `tecnico.json` final na
/// pasta da saída, marca concluída e apaga o pendente.
pub(crate) fn concluir_conferencia(ws: &Path, saida_id: &str, conferida: bool, por: &str, g: &dyn Gravar) -> Result<SaidaRegistrada> {
    // uma conferência por vez: duas juntas gravariam dois relatórios com horas diferentes
    static VEZ: Mutex<()> = Mutex::new(());
    let _vez = VEZ.lock().unwrap_or_else(|e| e.into_inner());
    let (conn, occ) = abrir_banco(ws)?;
    let id = uuid::Uuid::parse_str(saida_id).map_err(|_| erro(format!("saida_ausente: {saida_id}")))?;
    let s = video_edit_output_repo::obter(&conn, &occ, &id)?.ok_or_else(|| erro(format!("saida_ausente: {saida_id}")))?;
    if s.estado != EstadoSaida::AConferir {
        return Err(erro(format!("saida_concluida: {saida_id}")));
    }
    let retrato = paths::resolver_relativo(ws, &s.retrato_rel);
    let pendente = retrato.join(PENDENTE);
    let mut tecnico: Value = serde_json::from_slice(&fs::read(&pendente)?)?;
    let em = chrono::Utc::now();
    let c = serde_json::json!({
        "ocultacoes": if conferida { "conferida" } else { "nao_conferida" },
        "por": conferida.then_some(por),
        "em": conferida.then(|| em.to_rfc3339()),
    });
    let execucao = tecnico.as_object_mut().and_then(|o| o.get_mut("execucao")).and_then(Value::as_object_mut);
    let Some(execucao) = execucao else { return Err(erro(format!("{}: tecnico.pendente.json", codigo::ESQUEMA))) };
    let conf = execucao.entry("conferencia").or_insert_with(|| Value::Object(Default::default()));
    if !conf.is_object() {
        *conf = Value::Object(Default::default());
    }
    for (k, v) in c.as_object().into_iter().flatten() {
        conf[k] = v.clone();
    }
    let pasta = pasta_da_saida(ws, &s.pasta)?;
    let base = nome_base(&s.titulo);
    let arq = pasta.join(format!("{base}.tecnico.json"));
    // a saída ainda está "a conferir": relatório que já exista é sobra de tentativa que falhou depois de gravar
    // (somente leitura, impediria o `create_new` para sempre)
    if arq.is_file() {
        gravavel(&arq);
        fs::remove_file(&arq)?;
    }
    g.gravar(&pasta, &base, &tecnico)?;
    if arq.is_file() {
        somente_leitura(&arq)?;
    }
    let tecnico_rel = format!("{}/{base}.tecnico.json", s.pasta);
    if !video_edit_output_repo::concluir(&conn, &occ, &id, conferida.then_some((por, em)), Some(&tecnico_rel))? {
        return Err(erro(format!("saida_concluida: {saida_id}")));
    }
    let _ = fs::remove_file(&pendente);
    let d = serde_json::json!({ "conferida": conferida, "por": por });
    occurrence_repo::record_audit(&conn, Some(&occ), "video.edicao.conferida", Some("video"), Some("edicao_saida"), Some(&id), Some(&d.to_string()))?;
    let s = video_edit_output_repo::obter(&conn, &occ, &id)?.ok_or_else(|| erro(format!("saida_ausente: {saida_id}")))?;
    Ok(s.into())
}

/// `edicao_saida_excluir`: linha do banco, retrato e pasta da saída (somente leitura incluída), com auditoria.
/// Se era a última exportação de um projeto já excluído, a pasta do projeto some (O7).
pub(crate) fn excluir_saida(ws: &Path, saida_id: &str) -> Result<()> {
    let (conn, occ) = abrir_banco(ws)?;
    let id = uuid::Uuid::parse_str(saida_id).map_err(|_| erro(format!("saida_ausente: {saida_id}")))?;
    let s = video_edit_output_repo::obter(&conn, &occ, &id)?.ok_or_else(|| erro(format!("saida_ausente: {saida_id}")))?;
    let pasta = pasta_da_saida(ws, &s.pasta)?;
    let d = serde_json::json!({ "projeto_id": s.projeto_id, "titulo": s.titulo, "arquivo": s.arquivo, "sha256": s.sha256 });
    occurrence_repo::record_audit(&conn, Some(&occ), "video.edicao.saida_excluida", Some("video"), Some("edicao_saida"), Some(&id), Some(&d.to_string()))?;
    apagar_pasta_da_saida(&pasta)?;
    if armazem::id_valido(&s.projeto_id) {
        let proj = armazem::pasta_projeto(ws, &s.projeto_id);
        let dir = proj.join(EXPORTACOES).join(saida_id);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        let vazia = fs::read_dir(proj.join(EXPORTACOES)).map(|mut r| r.next().is_none()).unwrap_or(true);
        if vazia && !proj.join("projeto.json").is_file() && proj.is_dir() {
            fs::remove_dir_all(&proj)?;
        }
    }
    video_edit_output_repo::excluir(&conn, &occ, &id)?;
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde::de::DeserializeOwned;
    use serde_json::json;

    fn ida_e_volta<T: Serialize + DeserializeOwned>(j: Value) {
        let lido: T = serde_json::from_value(j.clone()).expect("lê");
        assert_eq!(serde_json::to_value(&lido).unwrap(), j);
    }

    #[test]
    fn fase_tag_tipo() {
        ida_e_volta::<Fase>(json!({ "tipo": "previas" }));
        ida_e_volta::<Fase>(json!({ "tipo": "blocos", "feitos": 3, "total": 7 }));
        ida_e_volta::<Fase>(json!({ "tipo": "conferindo" }));
    }

    #[test]
    fn estado_tag_tipo() {
        ida_e_volta::<Estado>(json!({ "tipo": "na_fila" }));
        ida_e_volta::<Estado>(json!({ "tipo": "preparando" }));
        ida_e_volta::<Estado>(json!({ "tipo": "rodando", "fase": { "tipo": "blocos", "feitos": 1, "total": 2 },
                                       "feito_q": 300, "total_q": 1419, "qps": 112.5, "restante_s": 10 }));
        ida_e_volta::<Estado>(json!({ "tipo": "a_conferir", "saida_id": "s1" }));
        ida_e_volta::<Estado>(json!({ "tipo": "concluida", "saida_id": "s1" }));
        ida_e_volta::<Estado>(json!({ "tipo": "pronta", "relativo": "render/t1/previa.mp4" }));
        ida_e_volta::<Estado>(json!({ "tipo": "falhou", "mensagem": "disco cheio", "log_rel": "render/t1/b000.log" }));
        ida_e_volta::<Estado>(json!({ "tipo": "cancelada" }));
        ida_e_volta::<Tarefa>(json!({ "id": "t1", "tipo": "quadro_final", "caso": "c1", "projeto_id": "p1", "titulo": "Teste",
                                       "estado": { "tipo": "na_fila" }, "criado_em": "2026-11-14T03:00:00Z" }));
    }

    #[test]
    fn evento_de_progresso_achatado() {
        ida_e_volta::<EventoProgresso>(json!({ "tarefa_id": "t1", "fase": { "tipo": "audio" }, "feito_q": 10, "total_q": 20, "qps": 30.0, "restante_s": 1 }));
    }

    use crate::edicao::executar::apoio as ap;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    /// Motor falso: cada tarefa espera a sua liberação (por título), avisa o progresso e devolve o estado pedido.
    #[derive(Default)]
    struct Falso {
        liberadas: Mutex<HashMap<String, Estado>>,
        erros: Mutex<HashMap<String, String>>,
        rodando: Mutex<Vec<String>>,
        ordem: Mutex<Vec<String>>,
        juntos_max: Mutex<usize>,
        progresso_ms: Mutex<Option<u64>>,
    }

    impl Falso {
        fn liberar(&self, titulo: &str, e: Estado) {
            self.liberadas.lock().unwrap().insert(titulo.into(), e);
        }
    }

    impl Motor for Falso {
        fn rodar(&self, t: &Tarefa, _: &PedidoRender, _: &Path, cancelar: &AtomicBool, progresso: &mut dyn FnMut(Progresso)) -> Result<Estado> {
            {
                let mut r = self.rodando.lock().unwrap();
                r.push(t.titulo.clone());
                let mut m = self.juntos_max.lock().unwrap();
                *m = (*m).max(r.len());
            }
            self.ordem.lock().unwrap().push(t.titulo.clone());
            let sair = |eu: &Self| eu.rodando.lock().unwrap().retain(|x| x != &t.titulo);
            if t.titulo == "pânico" {
                sair(self);
                panic!("de propósito");
            }
            let t0 = Instant::now();
            let mut k = 0u64;
            loop {
                if cancelar.load(Ordering::SeqCst) {
                    sair(self);
                    return Err(SicroError::Validation("cancelado: b000".into()));
                }
                if let Some(e) = self.erros.lock().unwrap().get(&t.titulo) {
                    sair(self);
                    return Err(SicroError::Validation(e.clone()));
                }
                if let Some(e) = self.liberadas.lock().unwrap().get(&t.titulo) {
                    sair(self);
                    return Ok(e.clone());
                }
                let passo = *self.progresso_ms.lock().unwrap();
                if let Some(ms) = passo {
                    k += 1;
                    let fase = if t0.elapsed() < Duration::from_millis(700) { Fase::Camadas } else { Fase::Blocos { feitos: 0, total: 3 } };
                    progresso(Progresso { fase, feito_q: k, total_q: 1_000_000, qps: 1.0, restante_s: 1 });
                    std::thread::sleep(Duration::from_millis(ms));
                } else {
                    std::thread::sleep(Duration::from_millis(2));
                }
                assert!(t0.elapsed() < Duration::from_secs(60), "tarefa {} presa", t.titulo);
            }
        }
    }

    #[derive(Default)]
    struct Escuta {
        filas: Mutex<Vec<Vec<Tarefa>>>,
        progressos: Mutex<Vec<(Instant, EventoProgresso)>>,
    }

    impl Ouvinte for Arc<Escuta> {
        fn fila(&self, t: &[Tarefa]) {
            self.filas.lock().unwrap().push(t.to_vec());
        }
        fn progresso(&self, e: &EventoProgresso) {
            self.progressos.lock().unwrap().push((Instant::now(), e.clone()));
        }
    }

    fn pedido(cache: &Path, tipo: TipoTarefa, projeto: &str, titulo: &str) -> PedidoRender {
        let alvo = match tipo {
            TipoTarefa::Previa => Alvo::Previa { altura: 360, de_q: 0, ate_q: 30 },
            TipoTarefa::QuadroFinal => Alvo::Quadro { q: 0, largura: 320 },
            _ => ap::alvo_final(EscolhaCodificador::Cpu),
        };
        PedidoRender {
            tipo,
            ws: cache.join("caso"),
            cache: cache.join("cache-do-caso"),
            projeto: json!({ "esquema": 2, "id": projeto, "titulo": titulo }),
            alvo,
            ciencias: vec![],
        }
    }

    fn estado(f: &Fila, id: &str) -> Estado {
        f.listar().into_iter().find(|t| t.id == id).expect("tarefa").estado
    }

    /// Espera o motor falso começar a tarefa.
    fn comecou(m: &Falso, titulo: &str) {
        let t0 = Instant::now();
        while !m.rodando.lock().unwrap().iter().any(|x| x == titulo) {
            assert!(t0.elapsed() < Duration::from_secs(30), "{titulo} não começou");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Exportações uma por vez e em ordem; prévia em faixa própria (roda junto); retrato da tarefa gravado ao enfileirar.
    #[test]
    fn exportacoes_em_ordem_uma_por_vez() {
        let d = ap::pasta("fila-ordem");
        let m = Arc::new(Falso::default());
        let f = Fila::nova(m.clone());
        let e1 = f.enfileirar(pedido(d.path(), TipoTarefa::Exportar, "p1", "e1")).unwrap();
        let e2 = f.enfileirar(pedido(d.path(), TipoTarefa::Exportar, "p1", "e2")).unwrap();
        let e3 = f.enfileirar(pedido(d.path(), TipoTarefa::Exportar, "p2", "e3")).unwrap();
        let p1 = f.enfileirar(pedido(d.path(), TipoTarefa::Previa, "p1", "p1")).unwrap();
        comecou(&m, "e1");
        comecou(&m, "p1");
        assert_eq!((estado(&f, &e2), estado(&f, &e3)), (Estado::NaFila, Estado::NaFila));
        let retrato: Value = serde_json::from_slice(&fs::read(d.path().join("cache-do-caso/render").join(&e2).join(RETRATO_TAREFA)).unwrap()).unwrap();
        assert_eq!(retrato["titulo"], "e2");
        for (t, id) in [("e1", &e1), ("e2", &e2), ("e3", &e3)] {
            m.liberar(t, Estado::Concluida { saida_id: format!("s-{t}") });
            assert_eq!(f.esperar(id).unwrap(), Estado::Concluida { saida_id: format!("s-{t}") });
        }
        m.liberar("p1", Estado::Pronta { relativo: "render/x/saida.mp4".into() });
        assert_eq!(f.esperar(&p1).unwrap(), Estado::Pronta { relativo: "render/x/saida.mp4".into() });
        let ordem = m.ordem.lock().unwrap().clone();
        let exportacoes: Vec<&String> = ordem.iter().filter(|t| t.starts_with('e')).collect();
        assert_eq!(exportacoes, ["e1", "e2", "e3"]);
        assert_eq!(*m.juntos_max.lock().unwrap(), 2, "uma exportação e uma prévia ao mesmo tempo");
        let lista = f.listar();
        assert_eq!(lista.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), [e1.as_str(), &e2, &e3, &p1], "ordem de entrada");
        assert!(lista.iter().all(|t| t.caso == "cache-do-caso" && !t.criado_em.is_empty()));
        assert!(f.limpar().is_empty());
        f.encerrar();
    }

    /// Prévia e quadro final: a nova cancela a anterior do mesmo tipo e projeto; outro projeto ou outro tipo não.
    #[test]
    fn a_ultima_previa_vence() {
        let d = ap::pasta("fila-ultima");
        let m = Arc::new(Falso::default());
        let f = Fila::nova(m.clone());
        let a1 = f.enfileirar(pedido(d.path(), TipoTarefa::Previa, "pa", "a1")).unwrap();
        comecou(&m, "a1");
        let q1 = f.enfileirar(pedido(d.path(), TipoTarefa::QuadroFinal, "pa", "q1")).unwrap();
        let b1 = f.enfileirar(pedido(d.path(), TipoTarefa::Previa, "pb", "b1")).unwrap();
        let q2 = f.enfileirar(pedido(d.path(), TipoTarefa::QuadroFinal, "pa", "q2")).unwrap();
        let a2 = f.enfileirar(pedido(d.path(), TipoTarefa::Previa, "pa", "a2")).unwrap();
        // a1 e q1 rodavam (faixas próprias): canceladas pelo motor
        assert_eq!(f.esperar(&a1).unwrap(), Estado::Cancelada);
        assert_eq!(f.esperar(&q1).unwrap(), Estado::Cancelada);
        for (t, id) in [("b1", &b1), ("q2", &q2), ("a2", &a2)] {
            m.liberar(t, Estado::Pronta { relativo: format!("quadro/{t}.bmp") });
            assert_eq!(f.esperar(id).unwrap(), Estado::Pronta { relativo: format!("quadro/{t}.bmp") }, "{t}");
        }
        // b1 (outro projeto) esperou a2? não: a faixa da prévia roda uma por vez, na ordem que sobrou
        let ordem = m.ordem.lock().unwrap().clone();
        let pos = |t: &str| ordem.iter().position(|x| x == t).unwrap();
        assert!(pos("b1") < pos("a2"), "{ordem:?}");
        f.encerrar();
    }

    /// Cancelar na fila (na hora) e rodando (o motor para); falha vira `falhou` com o registro; pânico também.
    #[test]
    fn cancelar_falhar_e_panico() {
        let d = ap::pasta("fila-cancelar");
        let m = Arc::new(Falso::default());
        let f = Fila::nova(m.clone());
        let r = f.enfileirar(pedido(d.path(), TipoTarefa::Exportar, "p", "roda")).unwrap();
        let espera = f.enfileirar(pedido(d.path(), TipoTarefa::Exportar, "p", "espera")).unwrap();
        comecou(&m, "roda");
        let render = d.path().join("cache-do-caso/render");
        fs::write(render.join(&r).join("audio.wav"), vec![0u8; 4096]).unwrap();
        fs::write(render.join(&r).join(executar::CHAVES), b"{}").unwrap();
        f.cancelar(&espera).unwrap();
        assert_eq!(estado(&f, &espera), Estado::Cancelada);
        f.cancelar(&r).unwrap();
        assert_eq!(f.esperar(&r).unwrap(), Estado::Cancelada);
        // cancelada (na fila ou rodando): ficam só o id do projeto e as chaves do cache
        for id in [&r, &espera] {
            assert_eq!(fs::read_to_string(render.join(id).join(RETRATO_TAREFA)).unwrap(), r#"{"id":"p"}"#);
            assert!(!render.join(id).join("audio.wav").exists());
        }
        assert!(render.join(&r).join(executar::CHAVES).is_file());
        assert!(!m.ordem.lock().unwrap().contains(&"espera".to_string()));
        assert!(f.cancelar("nao-existe").is_err());

        m.erros.lock().unwrap().insert("falha".into(), "filtro_ausente: b001 (código Some(1)): No such filter".into());
        let x = f.enfileirar(pedido(d.path(), TipoTarefa::Exportar, "p", "falha")).unwrap();
        let Estado::Falhou { mensagem, log_rel } = f.esperar(&x).unwrap() else { panic!("devia falhar") };
        assert!(mensagem.starts_with("filtro_ausente: "), "{mensagem}");
        assert_eq!(log_rel, format!("render/{x}/falha.log"));
        assert!(fs::read_to_string(d.path().join("cache-do-caso").join(&log_rel)).unwrap().contains("No such filter"));

        let p = f.enfileirar(pedido(d.path(), TipoTarefa::Exportar, "p", "pânico")).unwrap();
        assert!(matches!(f.esperar(&p).unwrap(), Estado::Falhou { mensagem, .. } if mensagem == "panico"));
        // a fila segue depois do pânico
        let depois = f.enfileirar(pedido(d.path(), TipoTarefa::Exportar, "p", "depois")).unwrap();
        m.liberar("depois", Estado::Concluida { saida_id: "s".into() });
        assert!(matches!(f.esperar(&depois).unwrap(), Estado::Concluida { .. }));
        assert!(!f.projeto_ocupado("cache-do-caso", "p"));
        f.encerrar();
    }

    /// `AConferir` fica na lista até a conferência; reiniciar dá fila vazia.
    #[test]
    fn a_conferir_e_reinicio() {
        let d = ap::pasta("fila-conferir");
        let m = Arc::new(Falso::default());
        let f = Fila::nova(m.clone());
        let ocupa = f.enfileirar(pedido(d.path(), TipoTarefa::Exportar, "p", "x")).unwrap();
        comecou(&m, "x");
        assert!(f.projeto_ocupado("cache-do-caso", "p") && !f.projeto_ocupado("cache-do-caso", "outro"));
        m.liberar("x", Estado::AConferir { saida_id: "s1".into() });
        assert_eq!(f.esperar(&ocupa).unwrap(), Estado::AConferir { saida_id: "s1".into() });
        assert!(!f.projeto_ocupado("cache-do-caso", "p"));
        assert!(f.saida_na_fila("s1") && !f.saida_na_fila("s2"));
        assert_eq!(f.limpar().len(), 1, "a conferir não sai no limpar");
        f.conferida("s1");
        assert_eq!(estado(&f, &ocupa), Estado::Concluida { saida_id: "s1".into() });
        assert!(f.limpar().is_empty());
        f.encerrar();
        // reinício: outra fila, vazia
        assert!(Fila::nova(m).listar().is_empty());
    }

    /// Progresso a cada 1 ms: no máximo 5 eventos de progresso por segundo; a lista vai só quando muda.
    #[test]
    fn no_maximo_5_eventos_de_progresso_por_segundo() {
        let d = ap::pasta("fila-progresso");
        let m = Arc::new(Falso::default());
        *m.progresso_ms.lock().unwrap() = Some(1);
        let f = Fila::nova(m.clone());
        let escuta = Arc::new(Escuta::default());
        f.ouvir(Arc::new(escuta.clone()));
        let id = f.enfileirar(pedido(d.path(), TipoTarefa::Exportar, "p", "x")).unwrap();
        comecou(&m, "x");
        std::thread::sleep(Duration::from_millis(1500));
        m.liberar("x", Estado::Concluida { saida_id: "s".into() });
        f.esperar(&id).unwrap();
        let ev = escuta.progressos.lock().unwrap().clone();
        assert!(ev.len() >= 4, "{}", ev.len());
        assert!(ev.iter().all(|(_, e)| e.tarefa_id == id));
        ap::no_maximo_5_por_segundo(&ev.iter().map(|(t, _)| *t).collect::<Vec<_>>());
        // lista: entrar, preparar, as duas fases e o fim (não uma por avanço)
        let filas = escuta.filas.lock().unwrap().len();
        assert!((4..=6).contains(&filas), "{filas} avisos da lista");
        assert!(matches!(escuta.filas.lock().unwrap().last().unwrap()[0].estado, Estado::Concluida { .. }));
        f.encerrar();
    }

    /// Quadro final a cada pausa: o terminado há mais que `manter` sai quando chega outro do mesmo projeto, e
    /// nenhuma tarefa terminada segura o documento na memória. Id de projeto que não serve de pasta é recusado.
    #[test]
    fn quadros_finais_antigos_saem_e_o_documento_nao_fica() {
        let d = ap::pasta("fila-poda");
        let m = Arc::new(Falso::default());
        let f = Fila::nova_com(m.clone(), Duration::from_millis(50));
        let rodar = |tipo, projeto: &str, t: &str| {
            let id = f.enfileirar(pedido(d.path(), tipo, projeto, t)).unwrap();
            m.liberar(t, Estado::Pronta { relativo: format!("quadro/{t}.bmp") });
            f.esperar(&id).unwrap();
            id
        };
        let q1 = rodar(TipoTarefa::QuadroFinal, "pa", "q1");
        let outro = rodar(TipoTarefa::QuadroFinal, "pb", "o1");
        let previa = rodar(TipoTarefa::Previa, "pa", "v1");
        let exportada = f.enfileirar(pedido(d.path(), TipoTarefa::Exportar, "pa", "e1")).unwrap();
        m.liberar("e1", Estado::Concluida { saida_id: "s".into() });
        f.esperar(&exportada).unwrap();
        assert!(f.trava().itens.iter().all(|i| i.pedido.projeto.is_null()), "documento de tarefa terminada na memória");
        std::thread::sleep(Duration::from_millis(80));
        let q2 = rodar(TipoTarefa::QuadroFinal, "pa", "q2");
        let q3 = rodar(TipoTarefa::QuadroFinal, "pa", "q3");
        // quadro final pronto não deixa o documento na pasta de trabalho; a prévia (o arquivo dela mora lá) deixa
        let render = d.path().join("cache-do-caso/render");
        assert_eq!(fs::read_to_string(render.join(&q2).join(RETRATO_TAREFA)).unwrap(), r#"{"id":"pa"}"#);
        assert!(fs::read_to_string(render.join(&previa).join(RETRATO_TAREFA)).unwrap().contains("titulo"));
        let ids: Vec<String> = f.listar().into_iter().map(|t| t.id).collect();
        // q1 saiu; q2 (recém-terminado) fica; outro projeto, prévia e exportação não são tocados
        assert_eq!(ids, [outro, previa, exportada, q2, q3]);
        assert!(!ids.contains(&q1));

        for ruim in ["../fora", "", "a/b"] {
            let e = f.enfileirar(pedido(d.path(), TipoTarefa::QuadroFinal, ruim, "x")).unwrap_err();
            assert!(e.to_string().contains("esquema"), "{ruim}: {e}");
        }
        let mut sem_id = pedido(d.path(), TipoTarefa::Exportar, "p", "x");
        sem_id.projeto = json!({ "esquema": 2, "titulo": "x" });
        assert!(f.enfileirar(sem_id).is_err());
        f.encerrar();
    }

    #[test]
    fn encerrar_cancela_tudo() {
        let d = ap::pasta("fila-encerrar");
        let m = Arc::new(Falso::default());
        let f = Fila::nova(m.clone());
        let a = f.enfileirar(pedido(d.path(), TipoTarefa::Exportar, "p", "a")).unwrap();
        let b = f.enfileirar(pedido(d.path(), TipoTarefa::Exportar, "p", "b")).unwrap();
        comecou(&m, "a");
        f.encerrar();
        assert_eq!((estado(&f, &a), estado(&f, &b)), (Estado::Cancelada, Estado::Cancelada));
        assert!(m.rodando.lock().unwrap().is_empty());
    }

    // ------------------------------------------------------------------ publicação, retrato e conferência

    use ap::caso;

    /// Relatório falso: o `montar` do P0.R é outro pacote.
    struct TecFalso;

    impl Tecnico for TecFalso {
        fn montar(&self, fecho: &Fecho) -> Result<Value> {
            Ok(json!({ "esquema": "teste", "feito": { "projeto": { "sha256": fecho.projeto_sha256 } },
                       "execucao": { "arquivo": fecho.arquivo_rel, "conferencia": { "quadros": 90, "erros": 0 } } }))
        }
        fn gravar(&self, pasta: &Path, titulo: &str, v: &Value) -> Result<String> {
            use std::io::Write;
            let b = self.bytes(v);
            fs::OpenOptions::new().write(true).create_new(true).open(pasta.join(format!("{titulo}.tecnico.json")))?.write_all(&b)?;
            Ok(sha256_bytes(&b))
        }
        fn bytes(&self, v: &Value) -> Vec<u8> {
            serde_json::to_vec_pretty(v).unwrap()
        }
    }

    impl Gravar for TecFalso {
        fn gravar(&self, pasta: &Path, titulo: &str, v: &Value) -> Result<String> {
            Tecnico::gravar(self, pasta, titulo, v)
        }
    }

    const SHA_MIDIA: &str = "5f3a9c0e2b7d41e6a8c9b0d1e2f3a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5";

    fn prov_midia() -> super::super::motor::plano::ProvMidia {
        serde_json::from_value(json!({
            "id": "m1", "nome": "cam.mp4", "sha256": SHA_MIDIA, "derivacao": null, "recebido": { "nativo": null, "obs": "" },
            "relogio_desvio_s": null, "gravador": null, "cor": { "matriz": "bt709", "faixa": "tv" }, "conteiner": "mp4", "codec": "h264",
            "pix_fmt": "yuv420p", "sar": { "num": 1, "den": 1 }, "giro": 0, "inicio_s": 0.0,
            "declarado": { "r_frame_rate": "30/1", "avg_frame_rate": "30/1", "nb_frames": 450, "duracao": 15.0 },
            "medido": { "tb": { "num": 1, "den": 15360 }, "de_s": 0.0, "ate_s": 15.0, "pacotes": 450, "quadros": 450, "passo_min": 512,
                        "passo_med": 512, "passo_max": 512, "lacunas": [] },
            "discrepancias": [], "erros_decodificacao": 0, "usado": [{ "de_s": 1.0, "ate_s": 4.0 }], "usado_total_s": 3.0
        }))
        .unwrap()
    }

    fn ocultacao() -> super::super::motor::plano::ProvOcultacao {
        serde_json::from_value(json!({
            "midia": "m1", "clipes": ["k0"], "de_s": 1.0, "ate_s": 2.0, "modo": "solido", "forma": "retangulo", "intensidade": 1.0,
            "conteudo": "placa", "motivo": "placa", "quadros": { "ancora": 1, "medido": 0, "interpolado": 0, "incerto": 0 }
        }))
        .unwrap()
    }

    /// Plano e resultado falsos (o arquivo "renderizado" são bytes conhecidos na pasta de trabalho).
    fn render_falso(d: &Path, tarefa: &str, ocultar: bool) -> (Plano, Resultado, Ambiente) {
        let cap = Arc::new(crate::edicao::quadros::apoio::cap_falsa());
        let mut plano = ap::plano(&cap, Path::new("/caminho/da/fonte.mp4"), &[0; 120], &ap::cpu(), ap::alvo_final(EscolhaCodificador::Cpu));
        plano.proveniencia.midias = vec![prov_midia()];
        if ocultar {
            plano.proveniencia.ocultacoes = vec![ocultacao()];
        }
        let mut amb = ap::ambiente(cap, d, tarefa);
        amb.identidade.autor.nome = "Perito Teste".into();
        fs::create_dir_all(&amb.trabalho).unwrap();
        let arquivo = amb.trabalho.join("saida.mp4");
        fs::write(&arquivo, format!("vídeo de {tarefa}")).unwrap();
        let compostos = amb.trabalho.join(COMPOSTOS);
        fs::write(&compostos, "aa\nbb\n").unwrap();
        let res = Resultado {
            sha256: crate::hashing::sha256::sha256_file(&arquivo).unwrap(),
            tamanho: fs::metadata(&arquivo).unwrap().len(),
            arquivo,
            codificador: "libx264".into(),
            argumentos: ap::cpu(),
            queda_para_cpu: None,
            composto_sha256: Some("c".repeat(64)),
            compostos: Some(compostos),
            audio_sha256: Some("a".repeat(64)),
            loudnorm: None,
            ganho_db: None,
            blocos: vec![],
            camadas: vec![],
            passos: vec![],
            conferencia: Some(super::super::conferir::ConferenciaSaida {
                quadros: 90,
                erros: 0,
                decodificado_sha256: "d".repeat(64),
                decodificados: PathBuf::new(),
            }),
            lidos: vec![],
            legendas: vec![],
            trf: vec![],
            renderizado_em: agora(),
        };
        (plano, res, amb)
    }

    fn quando() -> chrono::DateTime<chrono::Local> {
        use chrono::TimeZone;
        chrono::Local.with_ymd_and_hms(2026, 11, 14, 15, 4, 5).unwrap()
    }

    fn contar(ws: &Path, sql: &str) -> i64 {
        let (c, _) = abrir_banco(ws).unwrap();
        c.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    /// Duas exportações no mesmo segundo: duas pastas; arquivos somente leitura; nada sobrescrito; banco, log por
    /// mídia e auditoria; retrato íntegro; excluir a saída tira tudo, e a última leva a pasta do projeto já excluído.
    #[test]
    fn publicar_registra_e_nunca_sobrescreve() {
        let (d, ws) = caso();
        let pid = "6f1c2a52-0d5e-4c1b-9a43-2f7d0c1e9b10";
        let projeto = json!({ "esquema": 2, "id": pid, "titulo": "Placa: a/b", "midias": [] });
        let mut saidas_feitas = Vec::new();
        for t in ["t1", "t2", "t3"] {
            let (plano, res, amb) = render_falso(d.path(), t, false);
            fs::write(amb.trabalho.join("audio.wav"), b"RIFF").unwrap();
            if t == "t3" {
                // pasta com o nome da próxima já existe: não é reaproveitada
                fs::create_dir_all(paths::dir_edicao_saida(&ws).join("Placa_ a_b 2026-11-14 15h04m05 (3)")).unwrap();
            }
            let s = publicar(&Publicar { ws: &ws, projeto: &projeto, plano: &plano, res: &res, amb: &amb, quando: quando(), tecnico: &TecFalso }).unwrap();
            saidas_feitas.push((s, res));
        }
        let pastas: Vec<&str> = saidas_feitas.iter().map(|(s, _)| s.pasta.as_str()).collect();
        assert_eq!(
            pastas,
            [
                "Exportações/Vídeos/Edições/Placa_ a_b 2026-11-14 15h04m05",
                "Exportações/Vídeos/Edições/Placa_ a_b 2026-11-14 15h04m05 (2)",
                "Exportações/Vídeos/Edições/Placa_ a_b 2026-11-14 15h04m05 (4)",
            ]
        );
        for (s, res) in &saidas_feitas {
            assert_eq!((s.estado, s.exige_conferencia, s.codificador.as_str()), (EstadoSaida::Concluida, false, "libx264"));
            assert_eq!(s.arquivo, format!("{}/Placa_ a_b.mp4", s.pasta));
            assert_eq!(s.tecnico_rel.as_deref(), Some(format!("{}/Placa_ a_b.tecnico.json", s.pasta).as_str()));
            assert_eq!((s.n_quadros, s.duracao_s, s.sha256.as_str()), (90, 3.0, res.sha256.as_str()));
            assert_eq!(s.autor.nome, "Perito Teste");
            let video = ws.join(&s.arquivo);
            assert_eq!(crate::hashing::sha256::sha256_file(&video).unwrap(), res.sha256);
            assert!(!res.arquivo.exists(), "a ligação da pasta de trabalho sai");
            assert!(!res.arquivo.with_file_name("audio.wav").exists(), "o PCM do programa sai da pasta de trabalho");
            assert!(fs::metadata(&video).unwrap().permissions().readonly());
            assert!(fs::metadata(ws.join(s.tecnico_rel.as_ref().unwrap())).unwrap().permissions().readonly());
            assert!(fs::OpenOptions::new().write(true).open(&video).is_err(), "somente leitura");
            // retrato íntegro, com a lista por slot e os grafos sem caminho absoluto
            let (v, m) = ler_retrato(&ws, pid, &s.id).unwrap();
            assert_eq!((v, m.projeto_sha256.as_str(), m.alvo.clone()), (projeto.clone(), s.projeto_sha256.as_str(), ap::alvo_final(EscolhaCodificador::Cpu)));
            let r = ws.join(&s.retrato_rel);
            assert_eq!(fs::read_to_string(r.join(COMPOSTOS)).unwrap(), "aa\nbb\n");
            assert!(r.join("grafos/b000/b000.txt").is_file());
            let argumentos = fs::read_to_string(r.join("grafos/argumentos.json")).unwrap();
            assert!(argumentos.contains("midia:m1") && !argumentos.contains("/caminho/da/fonte.mp4"), "{argumentos}");
        }
        let ids: std::collections::HashSet<&str> = saidas_feitas.iter().map(|(s, _)| s.id.as_str()).collect();
        assert_eq!(ids.len(), 3);
        // banco, log por mídia e auditoria
        let lista = saidas(&ws, Some(pid)).unwrap();
        assert_eq!(lista.len(), 3);
        assert_eq!(saidas(&ws, None).unwrap(), lista);
        assert!(saidas(&ws, Some("outro")).unwrap().is_empty());
        let (conn, occ) = abrir_banco(&ws).unwrap();
        let logs = video_repo::list_logs_for_media(&conn, &occ, SHA_MIDIA, 50).unwrap();
        assert_eq!(logs.iter().filter(|l| l.action == "edicao.uso").count(), 3);
        let d0: Value = serde_json::from_str(&logs[0].details_json).unwrap();
        assert_eq!((d0["de_s"].as_f64(), d0["ate_s"].as_f64()), (Some(1.0), Some(4.0)));
        assert!(ids.contains(d0["saida_id"].as_str().unwrap()));
        assert_eq!(contar(&ws, "SELECT COUNT(*) FROM audit_logs WHERE action = 'video.edicao.render'"), 3);

        // excluir: somente leitura incluído; a pasta do projeto (sem projeto.json) some com a última
        let proj = armazem::pasta_projeto(&ws, pid);
        for (i, (s, _)) in saidas_feitas.iter().enumerate() {
            excluir_saida(&ws, &s.id).unwrap();
            assert!(!ws.join(&s.pasta).exists() && !ws.join(&s.retrato_rel).exists());
            assert_eq!(proj.exists(), i < 2, "pasta do projeto depois de {} exclusões", i + 1);
        }
        assert!(saidas(&ws, None).unwrap().is_empty());
        assert_eq!(contar(&ws, "SELECT COUNT(*) FROM audit_logs WHERE action = 'video.edicao.saida_excluida'"), 3);
        assert!(excluir_saida(&ws, &saidas_feitas[0].0.id).is_err());
    }

    /// Com ocultação: "(ocultado)" no nome, `a_conferir`, pendente no retrato; a conferência (sem fila, como depois de
    /// reiniciar) completa o relatório; "não conferida" também conclui.
    #[test]
    fn ocultacao_fica_a_conferir_e_sobrevive_ao_reinicio() {
        let (d, ws) = caso();
        let pid = "1a2b3c4d-0d5e-4c1b-9a43-2f7d0c1e9b10";
        let projeto = json!({ "esquema": 2, "id": pid, "titulo": "Oculto", "midias": [] });
        let mut feitas = Vec::new();
        for t in ["t1", "t2"] {
            let (plano, res, amb) = render_falso(d.path(), t, true);
            let s = publicar(&Publicar { ws: &ws, projeto: &projeto, plano: &plano, res: &res, amb: &amb, quando: quando(), tecnico: &TecFalso }).unwrap();
            assert_eq!((s.estado, s.exige_conferencia, s.tecnico_rel.clone()), (EstadoSaida::AConferir, true, None));
            assert!(s.arquivo.ends_with("/Oculto (ocultado).mp4"), "{}", s.arquivo);
            assert!(ws.join(&s.retrato_rel).join(PENDENTE).is_file());
            assert!(!ws.join(&s.pasta).join("Oculto.tecnico.json").exists());
            feitas.push(s);
        }
        // reinício: a fila nova não sabe de nada; a conferência vem do banco e do retrato
        assert!(Fila::nova(Arc::new(MotorSemUso)).listar().is_empty());
        let a = concluir_conferencia(&ws, &feitas[0].id, true, "Perito Teste", &TecFalso).unwrap();
        assert_eq!((a.estado, a.conferida_por.as_deref()), (EstadoSaida::Concluida, Some("Perito Teste")));
        assert!(a.conferida_em.is_some());
        let t: Value = serde_json::from_slice(&fs::read(ws.join(a.tecnico_rel.as_ref().unwrap())).unwrap()).unwrap();
        assert_eq!(t["execucao"]["conferencia"], json!({ "quadros": 90, "erros": 0, "ocultacoes": "conferida", "por": "Perito Teste", "em": a.conferida_em }));
        assert!(!ws.join(&a.retrato_rel).join(PENDENTE).exists());
        assert!(fs::metadata(ws.join(a.tecnico_rel.as_ref().unwrap())).unwrap().permissions().readonly());
        assert!(concluir_conferencia(&ws, &feitas[0].id, true, "x", &TecFalso).unwrap_err().to_string().contains("saida_concluida"));

        let b = concluir_conferencia(&ws, &feitas[1].id, false, "Perito Teste", &TecFalso).unwrap();
        assert_eq!((b.estado, b.conferida_por.clone(), b.exige_conferencia), (EstadoSaida::Concluida, None, true));
        let t: Value = serde_json::from_slice(&fs::read(ws.join(b.tecnico_rel.as_ref().unwrap())).unwrap()).unwrap();
        assert_eq!(t["execucao"]["conferencia"]["ocultacoes"], "nao_conferida");
        assert_eq!(t["execucao"]["conferencia"]["por"], Value::Null);
        assert_eq!(contar(&ws, "SELECT COUNT(*) FROM audit_logs WHERE action = 'video.edicao.conferida'"), 2);
    }

    /// Título comprido com acento no byte 100: o nome sai cortado numa letra inteira (antes: pânico no fim do render).
    #[test]
    fn titulo_comprido_com_acento_vira_nome() {
        for n in 95..=101 {
            let titulo = format!("{}ção do local", "a".repeat(n));
            let base = nome_base(&titulo);
            assert!(base.len() <= TITULO_MAX && titulo.starts_with(&base), "{n}: {base}");
            assert!(!base.ends_with(' ') && !base.ends_with('.'));
        }
        assert_eq!(nome_base(&format!("{} final", "a".repeat(99))), "a".repeat(99), "não termina em espaço");
        assert_eq!(nome_base("  ..  "), "Edição");
        assert_eq!(nome_base("Placa: a/b"), "Placa_ a_b");
    }

    /// Falha depois de a linha entrar no banco (auditoria que não grava): nada fica, nem linha, nem log, nem pasta.
    #[test]
    fn publicar_que_falha_no_registro_nao_deixa_nada() {
        let (d, ws) = caso();
        let pid = "7a1c2a52-0d5e-4c1b-9a43-2f7d0c1e9b10";
        let projeto = json!({ "esquema": 2, "id": pid, "titulo": "Falha", "midias": [] });
        let (plano, res, amb) = render_falso(d.path(), "t1", false);
        {
            let (c, _) = abrir_banco(&ws).unwrap();
            c.execute_batch("ALTER TABLE audit_logs RENAME TO audit_logs_fora").unwrap();
        }
        let e = publicar(&Publicar { ws: &ws, projeto: &projeto, plano: &plano, res: &res, amb: &amb, quando: quando(), tecnico: &TecFalso });
        assert!(e.is_err());
        assert_eq!(contar(&ws, "SELECT COUNT(*) FROM video_edit_outputs"), 0, "linha sem arquivo");
        assert_eq!(contar(&ws, "SELECT COUNT(*) FROM video_operation_logs WHERE action = 'edicao.uso'"), 0);
        let sobras: Vec<_> = fs::read_dir(paths::dir_edicao_saida(&ws)).into_iter().flatten().flatten().map(|e| e.file_name()).collect();
        assert!(sobras.is_empty(), "{sobras:?}");
        assert!(!armazem::pasta_projeto(&ws, pid).join("exportacoes").exists() || fs::read_dir(armazem::pasta_projeto(&ws, pid).join("exportacoes")).unwrap().next().is_none());
    }

    /// Conferência que falhou depois de gravar o relatório (somente leitura): a tentativa seguinte conclui.
    /// `pasta` do banco fora de `Exportações/Vídeos/Edições/` (vazia, com `..`): nada é apagado.
    #[test]
    fn conferencia_repete_e_excluir_nao_sai_da_pasta_das_edicoes() {
        let (d, ws) = caso();
        let pid = "8b2c3a52-0d5e-4c1b-9a43-2f7d0c1e9b10";
        let projeto = json!({ "esquema": 2, "id": pid, "titulo": "Oculto", "midias": [] });
        let (plano, res, amb) = render_falso(d.path(), "t1", true);
        let s = publicar(&Publicar { ws: &ws, projeto: &projeto, plano: &plano, res: &res, amb: &amb, quando: quando(), tecnico: &TecFalso }).unwrap();
        // sobra da tentativa anterior
        let sobra = ws.join(&s.pasta).join("Oculto.tecnico.json");
        fs::write(&sobra, b"{}").unwrap();
        somente_leitura(&sobra).unwrap();
        let a = concluir_conferencia(&ws, &s.id, true, "Perito Teste", &TecFalso).unwrap();
        assert_eq!(a.estado, EstadoSaida::Concluida);
        let t: Value = serde_json::from_slice(&fs::read(&sobra).unwrap()).unwrap();
        assert_eq!(t["execucao"]["conferencia"]["ocultacoes"], "conferida");

        for ruim in ["", ".", "Exportações/Vídeos/Edições", "Exportações/Vídeos/Edições/", "Exportações/Vídeos/Edições/..", "Exportações/Vídeos/Edições/../..", "Sistema"] {
            {
                let (c, _) = abrir_banco(&ws).unwrap();
                c.execute("UPDATE video_edit_outputs SET pasta = ?1", [ruim]).unwrap();
            }
            assert!(excluir_saida(&ws, &s.id).is_err(), "{ruim:?}");
            assert!(ws.join("Sistema").is_dir() && sobra.is_file(), "{ruim:?}: apagou fora da pasta da saída");
            assert_eq!(contar(&ws, "SELECT COUNT(*) FROM video_edit_outputs"), 1);
        }
    }

    struct MotorSemUso;
    impl Motor for MotorSemUso {
        fn rodar(&self, _: &Tarefa, _: &PedidoRender, _: &Path, _: &AtomicBool, _: &mut dyn FnMut(Progresso)) -> Result<Estado> {
            unreachable!()
        }
    }

    /// Retrato autossuficiente: apagar o projeto (menos `exportacoes/`) não estraga o retrato; todo hash confere;
    /// adulterado, ausente ou trocado é recusado; o mesmo retrato não é regravado.
    #[test]
    fn retrato_autossuficiente() {
        let (d, ws) = caso();
        let pid = "2b3c4d5e-0d5e-4c1b-9a43-2f7d0c1e9b10";
        let proj = armazem::pasta_projeto(&ws, pid);
        let escrever = |rel: &str, b: &[u8]| {
            let p = proj.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, b).unwrap();
        };
        let (denso, sil, trf, wav) = (b"denso".to_vec(), b"silhuetas".to_vec(), b"trf".to_vec(), b"RIFF narra".to_vec());
        let (s1, s2, s3, s4) = (sha256_bytes(&denso), sha256_bytes(&sil), sha256_bytes(&trf), sha256_bytes(&wav));
        escrever(&format!("rastreios/r1/{s1}.json"), &denso);
        escrever(&format!("silhuetas/r1/{s2}.sil"), &sil);
        escrever(&format!("vidstab/{s3}.trf"), &trf);
        escrever("midia/n1.wav", &wav);
        let projeto = json!({
            "esquema": 2, "id": pid, "titulo": "Retrato", "created_at": "", "updated_at": "",
            "rastreios": [{ "id": "r1", "denso": { "sha256": s1 }, "silhuetas": { "sha256": s2 } }],
            "principal": [{ "tipo": "trecho", "efeitos": [{ "tipo": "estabilizar", "trf_sha256": s3 }] }, { "tipo": "futuro", "x": [{ "tipo": "estabilizar", "trf_sha256": s3 }] }],
            "midias": [{ "id": "m2", "origem": "projeto", "relativo": format!("Sistema/videos/edicao/{pid}/midia/n1.wav"), "sha256": s4 },
                       { "id": "m3", "origem": "caso", "relativo": "Mídias/Vídeos/x.mp4", "sha256": SHA_MIDIA }]
        });
        fs::write(proj.join("projeto.json"), serde_json::to_vec(&projeto).unwrap()).unwrap();
        let (plano, _, _) = render_falso(d.path(), "t1", false);
        let sid = "3c4d5e6f-0d5e-4c1b-9a43-2f7d0c1e9b10";
        let m = gravar_retrato(&ws, pid, sid, &projeto, &plano).unwrap();
        let mut papeis: Vec<(PapelRetrato, &str)> = m.itens.iter().map(|i| (i.papel, i.de.as_str())).collect();
        papeis.sort_by_key(|p| p.1.to_string());
        assert_eq!(
            papeis,
            [
                (PapelRetrato::Midia, "midia/n1.wav"),
                (PapelRetrato::Denso, format!("rastreios/r1/{s1}.json").as_str()),
                (PapelRetrato::Silhuetas, format!("silhuetas/r1/{s2}.sil").as_str()),
                (PapelRetrato::Trf, format!("vidstab/{s3}.trf").as_str()),
            ]
        );
        assert!(gravar_retrato(&ws, pid, sid, &projeto, &plano).unwrap_err().to_string().contains("retrato_existe"));

        armazem::excluir(&ws, pid, None).unwrap();
        assert!(!proj.join("rastreios").exists() && !proj.join("projeto.json").exists() && proj.join("exportacoes").is_dir());
        let (v, m2) = ler_retrato(&ws, pid, sid).unwrap();
        assert_eq!((v, &m2), (projeto.clone(), &m));
        for (it, bytes) in m.itens.iter().map(|i| (i, match i.papel {
            PapelRetrato::Denso => &denso,
            PapelRetrato::Silhuetas => &sil,
            PapelRetrato::Trf => &trf,
            PapelRetrato::Midia => &wav,
        })) {
            assert_eq!(&fs::read(ws.join(paths::REL_EDICAO).join(pid).join("exportacoes").join(sid).join(&it.arquivo)).unwrap(), bytes);
        }
        // adulterado (mesmo tamanho, só o hash acusa; e outro tamanho): recusa
        let dados = armazem::pasta_projeto(&ws, pid).join("exportacoes").join(sid).join(&m.itens[0].arquivo);
        let original = fs::read(&dados).unwrap();
        let mut mexido = original.clone();
        mexido[0] ^= 0x20;
        fs::write(&dados, &mexido).unwrap();
        assert!(ler_retrato(&ws, pid, sid).unwrap_err().to_string().contains("midia_trocada"));
        fs::write(&dados, b"outra coisa").unwrap();
        assert!(ler_retrato(&ws, pid, sid).is_err());
        fs::write(&dados, &original).unwrap();
        assert!(ler_retrato(&ws, pid, sid).is_ok());
        let pj = armazem::pasta_projeto(&ws, pid).join("exportacoes").join(sid).join("projeto.json");
        let bom = fs::read(&pj).unwrap();
        fs::write(&pj, String::from_utf8(bom.clone()).unwrap().replace("Retrato", "Retrata")).unwrap();
        assert!(ler_retrato(&ws, pid, sid).is_err(), "projeto.json do retrato mexido");
        fs::write(&pj, &bom).unwrap();

        // citado que não existe, ou com outro conteúdo: recusa e não deixa pasta
        let sid2 = "4d5e6f70-0d5e-4c1b-9a43-2f7d0c1e9b10";
        let e = gravar_retrato(&ws, pid, sid2, &projeto, &plano).unwrap_err();
        assert!(e.to_string().contains("rastreio_ausente"), "{e}");
        assert!(!armazem::pasta_projeto(&ws, pid).join("exportacoes").join(sid2).exists());
        escrever("midia/n1.wav", b"trocado");
        let so_midia = json!({ "esquema": 2, "id": pid, "midias": projeto["midias"] });
        let e = gravar_retrato(&ws, pid, sid2, &so_midia, &plano).unwrap_err();
        assert!(e.to_string().contains("midia_trocada"), "{e}");
        assert!(!armazem::pasta_projeto(&ws, pid).join("exportacoes").join(sid2).exists());
        assert!(gravar_retrato(&ws, "../fora", sid2, &so_midia, &plano).is_err());
        // mídia "do projeto" que aponta para fora do caso (com o SHA-256 certo): não entra no retrato
        let fora = ws.parent().unwrap().join("fora.wav");
        fs::write(&fora, b"de fora").unwrap();
        let sha_fora = sha256_bytes(b"de fora");
        for rel in ["../fora.wav".to_string(), fora.to_string_lossy().into_owned()] {
            let p = json!({ "esquema": 2, "id": pid, "midias": [{ "id": "m9", "origem": "projeto", "relativo": rel, "sha256": sha_fora }] });
            let sid3 = uuid::Uuid::new_v4().to_string();
            let m = gravar_retrato(&ws, pid, &sid3, &p, &plano).unwrap();
            assert!(m.itens.is_empty(), "{rel}: {:?}", m.itens);
        }
    }

    #[test]
    fn saida_registrada_literais() {
        assert_eq!(serde_json::to_value(EstadoSaida::AConferir).unwrap(), json!("a_conferir"));
        assert_eq!(serde_json::to_value(TipoTarefa::QuadroFinal).unwrap(), json!("quadro_final"));
        assert_eq!(serde_json::to_value(PapelRetrato::Trf).unwrap(), json!("trf"));
    }
}
