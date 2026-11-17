//! Fila de fundo (§5.10): tabelas de quadros, cópias de prévia, proxies, intermediários, sonda. Dono: P0.7.
//! Do processo, 2 trabalhadores de baixa prioridade, chave única por trabalho. A tela nunca espera.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::executar::Limitador;
use crate::error::{Result, SicroError};

/// Tipo do trabalho de fundo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TipoFundo {
    Tabela,
    CopiaPrevia,
    Proxy,
    Origem,
    Onda,
    Tira,
    Sonda,
}

/// Situação de um trabalho de fundo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum EstadoFundo {
    NaFila,
    Rodando,
    Pronta,
    Falhou { mensagem: String },
    Cancelada,
}

/// `chave` = "<tipo>:<sha256 da mídia ou hash do pedido>" (ex.: "tabela:ab12…"; a sonda de GPU é "sonda:gpu");
/// `feito` em 0..1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TarefaFundo {
    pub id: String,
    pub tipo: TipoFundo,
    pub chave: String,
    pub feito: f32,
    pub estado: EstadoFundo,
}

/// Evento `edicao:fundo`: leva `Vec<TarefaFundo>` (as vivas e as terminadas há pouco) a cada mudança.
pub const EVENTO_FUNDO: &str = "edicao:fundo";

/// O trabalho: recebe o sinal de cancelamento e uma função de progresso (0..1).
pub type Trabalho = Box<dyn FnOnce(&AtomicBool, &mut dyn FnMut(f32)) -> Result<()> + Send + 'static>;

/// Trabalhadores do processo.
const TRABALHADORES: usize = 2;
/// Terminada há mais que isso sai da lista.
const MANTER_TERMINADA: Duration = Duration::from_secs(60);
/// Trabalho que falhou não é refeito por esse tempo para a mesma chave, salvo pedido "na frente" (a tela pede a
/// prévia a cada desenho do cartão: sem isso, um ffmpeg por pedido). Menor que `MANTER_TERMINADA`.
const ESPERA_FALHA: Duration = Duration::from_secs(10);

type Avisar = Arc<dyn Fn(&[TarefaFundo]) + Send + Sync>;

struct Item {
    tarefa: TarefaFundo,
    trabalho: Option<Trabalho>,
    cancelar: Arc<AtomicBool>,
    terminada: Option<Instant>,
}

#[derive(Default)]
struct Interno {
    itens: Vec<Item>,
    fim: bool,
    iniciados: bool,
}

/// A fila; o processo tem uma (`global()`), os testes criam as suas.
pub(crate) struct Fundo {
    est: Mutex<Interno>,
    cv: Condvar,
    avisar: Mutex<Option<Avisar>>,
    lim: Mutex<Limitador>,
    trabalhadores: Mutex<Vec<JoinHandle<()>>>,
    n: usize,
    espera_falha: Duration,
}

fn terminal(e: &EstadoFundo) -> bool {
    matches!(e, EstadoFundo::Pronta | EstadoFundo::Falhou { .. } | EstadoFundo::Cancelada)
}

/// Texto do erro sem o prefixo do tipo: o código fica na frente.
pub(crate) fn texto_do_erro(e: &SicroError) -> String {
    match e {
        SicroError::Validation(m) => m.clone(),
        outro => outro.to_string(),
    }
}

impl Fundo {
    pub(crate) fn nova(n: usize) -> Arc<Self> {
        Arc::new(Fundo {
            est: Mutex::new(Interno::default()),
            cv: Condvar::new(),
            avisar: Mutex::new(None),
            lim: Mutex::new(Limitador::default()),
            trabalhadores: Mutex::new(Vec::new()),
            n,
            espera_falha: ESPERA_FALHA,
        })
    }

    fn trava(&self) -> MutexGuard<'_, Interno> {
        self.est.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn podar(g: &mut Interno) {
        g.itens.retain(|i| i.terminada.is_none_or(|t| t.elapsed() < MANTER_TERMINADA));
    }

    fn lista(g: &Interno) -> Vec<TarefaFundo> {
        g.itens.iter().map(|i| i.tarefa.clone()).collect()
    }

    /// Avisa fora da trava (quem ouve pode chamar `listar`).
    fn emitir(&self, lista: Vec<TarefaFundo>) {
        let a = self.avisar.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(a) = a {
            a(&lista);
        }
    }

    fn iniciar(self: &Arc<Self>, g: &mut Interno) {
        if g.iniciados {
            return;
        }
        g.iniciados = true;
        g.fim = false;
        let mut hs = self.trabalhadores.lock().unwrap_or_else(|e| e.into_inner());
        for k in 0..self.n {
            let f = self.clone();
            let h = std::thread::Builder::new().name(format!("edicao-fundo-{k}")).spawn(move || f.trabalhar());
            match h {
                Ok(h) => hs.push(h),
                Err(e) => tracing::error!("edição: trabalhador de fundo: {e}"),
            }
        }
    }

    pub(crate) fn pedir(self: &Arc<Self>, tipo: TipoFundo, chave: &str, trabalho: Trabalho, frente: bool) -> String {
        self.pedir_com(tipo, chave, trabalho, frente, frente)
    }

    /// `frente` = passa à frente das que esperam; `refazer` = roda de novo o que falhou há pouco.
    pub(crate) fn pedir_com(self: &Arc<Self>, tipo: TipoFundo, chave: &str, trabalho: Trabalho, frente: bool, refazer: bool) -> String {
        let mut g = self.trava();
        Self::podar(&mut g);
        if let Some(i) = g.itens.iter().position(|i| i.tarefa.chave == chave && !terminal(&i.tarefa.estado)) {
            let id = g.itens[i].tarefa.id.clone();
            // pedido repetido na frente: a que espera passa à frente
            if frente && g.itens[i].tarefa.estado == EstadoFundo::NaFila {
                let it = g.itens.remove(i);
                g.itens.insert(0, it);
            }
            return id;
        }
        let falhou_ha_pouco = |i: &Item| {
            matches!(i.tarefa.estado, EstadoFundo::Falhou { .. }) && i.terminada.is_some_and(|t| t.elapsed() < self.espera_falha)
        };
        if let Some(i) = g.itens.iter().find(|i| i.tarefa.chave == chave && falhou_ha_pouco(i)).filter(|_| !refazer) {
            return i.tarefa.id.clone();
        }
        g.itens.retain(|i| i.tarefa.chave != chave);
        let id = uuid::Uuid::new_v4().to_string();
        let item = Item {
            tarefa: TarefaFundo { id: id.clone(), tipo, chave: chave.to_string(), feito: 0.0, estado: EstadoFundo::NaFila },
            trabalho: Some(trabalho),
            cancelar: Arc::new(AtomicBool::new(false)),
            terminada: None,
        };
        if frente {
            g.itens.insert(0, item);
        } else {
            g.itens.push(item);
        }
        self.iniciar(&mut g);
        let lista = Self::lista(&g);
        drop(g);
        self.cv.notify_all();
        self.emitir(lista);
        id
    }

    pub(crate) fn estado(&self, chave: &str) -> Option<TarefaFundo> {
        self.trava().itens.iter().find(|i| i.tarefa.chave == chave).map(|i| i.tarefa.clone())
    }

    /// Cancela a tarefa viva dessa chave: na fila sai já `cancelada`; rodando, o sinal sobe e ela acaba `cancelada`.
    /// Devolve se havia tarefa viva.
    pub(crate) fn cancelar(&self, chave: &str) -> bool {
        let lista = {
            let mut g = self.trava();
            let Some(i) = g.itens.iter_mut().find(|i| i.tarefa.chave == chave && !terminal(&i.tarefa.estado)) else { return false };
            i.cancelar.store(true, Ordering::SeqCst);
            if i.tarefa.estado == EstadoFundo::NaFila {
                i.tarefa.estado = EstadoFundo::Cancelada;
                i.trabalho = None;
                i.terminada = Some(Instant::now());
            }
            Self::lista(&g)
        };
        self.emitir(lista);
        true
    }

    pub(crate) fn listar(&self) -> Vec<TarefaFundo> {
        let mut g = self.trava();
        Self::podar(&mut g);
        Self::lista(&g)
    }

    pub(crate) fn ouvir(&self, f: Avisar) {
        *self.avisar.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
    }

    /// Cancela tudo e espera os trabalhadores; depois disso um pedido novo liga trabalhadores novos.
    pub(crate) fn encerrar(&self) {
        let lista = {
            let mut g = self.trava();
            g.fim = true;
            for i in g.itens.iter_mut().filter(|i| !terminal(&i.tarefa.estado)) {
                i.cancelar.store(true, Ordering::SeqCst);
                if i.tarefa.estado == EstadoFundo::NaFila {
                    i.tarefa.estado = EstadoFundo::Cancelada;
                    i.trabalho = None;
                    i.terminada = Some(Instant::now());
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
        self.emitir(lista);
    }

    fn trabalhar(self: Arc<Self>) {
        baixa_prioridade();
        loop {
            let (id, trabalho, cancelar, lista) = {
                let mut g = self.trava();
                loop {
                    if g.fim {
                        return;
                    }
                    if let Some(i) = g.itens.iter().position(|i| i.tarefa.estado == EstadoFundo::NaFila && i.trabalho.is_some()) {
                        let it = &mut g.itens[i];
                        it.tarefa.estado = EstadoFundo::Rodando;
                        let t = (it.tarefa.id.clone(), it.trabalho.take(), it.cancelar.clone());
                        let lista = Self::lista(&g);
                        break (t.0, t.1, t.2, lista);
                    }
                    g = self.cv.wait(g).unwrap_or_else(|e| e.into_inner());
                }
            };
            self.emitir(lista);
            let Some(trabalho) = trabalho else { continue };
            let eu = self.clone();
            let id2 = id.clone();
            let mut progresso = move |f: f32| {
                // o aviso sai com o limitador preso (dois trabalhadores dividem o mesmo): o seguinte passa depois
                let mut lim = eu.lim.lock().unwrap_or_else(|e| e.into_inner());
                let lista = {
                    let mut g = eu.trava();
                    if let Some(i) = g.itens.iter_mut().find(|i| i.tarefa.id == id2) {
                        i.tarefa.feito = f.clamp(0.0, 1.0);
                    }
                    lim.passa().then(|| Self::lista(&g))
                };
                if let Some(l) = lista {
                    eu.emitir(l);
                }
                drop(lim);
            };
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| trabalho(&cancelar, &mut progresso)));
            let estado = match r {
                Ok(Ok(())) => EstadoFundo::Pronta,
                Ok(Err(_)) if cancelar.load(Ordering::SeqCst) => EstadoFundo::Cancelada,
                Ok(Err(e)) => EstadoFundo::Falhou { mensagem: texto_do_erro(&e) },
                Err(_) => EstadoFundo::Falhou { mensagem: "panico".into() },
            };
            let lista = {
                let mut g = self.trava();
                if let Some(i) = g.itens.iter_mut().find(|i| i.tarefa.id == id) {
                    if estado == EstadoFundo::Pronta {
                        i.tarefa.feito = 1.0;
                    }
                    i.tarefa.estado = estado;
                    i.terminada = Some(Instant::now());
                }
                Self::podar(&mut g);
                Self::lista(&g)
            };
            self.cv.notify_all();
            self.emitir(lista);
        }
    }
}

/// Trabalhador de fundo cede a CPU ao render e à tela (os filhos herdam).
#[cfg(target_os = "linux")]
fn baixa_prioridade() {
    extern "C" {
        fn setpriority(qual: std::ffi::c_int, quem: std::ffi::c_uint, prio: std::ffi::c_int) -> std::ffi::c_int;
    }
    // no Linux a prioridade é por thread: quem = 0 é esta thread
    unsafe {
        setpriority(0, 0, 10);
    }
}

#[cfg(not(target_os = "linux"))]
fn baixa_prioridade() {}

fn global() -> &'static Arc<Fundo> {
    static F: OnceLock<Arc<Fundo>> = OnceLock::new();
    F.get_or_init(|| Fundo::nova(TRABALHADORES))
}

/// Monta a chave de um trabalho.
pub fn chave(tipo: TipoFundo, alvo: &str) -> String {
    let t = serde_json::to_value(tipo).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    format!("{t}:{alvo}")
}

/// Pede um trabalho e devolve o id da tarefa. Chave repetida (na fila ou rodando, ou que falhou há menos de 10 s)
/// reaproveita a tarefa e descarta `trabalho`.
pub fn pedir(tipo: TipoFundo, chave: &str, trabalho: Trabalho) -> String {
    global().pedir(tipo, chave, trabalho, false)
}

/// `pedir`, passando à frente das que esperam e refazendo o que falhou há pouco (a exportação, que espera pela tabela).
pub(crate) fn pedir_na_frente(tipo: TipoFundo, chave: &str, trabalho: Trabalho) -> String {
    global().pedir(tipo, chave, trabalho, true)
}

/// Passa à frente das que esperam SEM refazer o que falhou há pouco (tabela de mídia sem tempo nos pacotes, decisão
/// (g)14: quem pede é a tela, a cada quadro; refazer a cada pedido seria um ffmpeg por pedido).
pub(crate) fn pedir_com_prioridade(tipo: TipoFundo, chave: &str, trabalho: Trabalho) -> String {
    global().pedir_com(tipo, chave, trabalho, true, false)
}

/// Tarefa dessa chave, se houver.
pub fn estado(chave: &str) -> Option<TarefaFundo> {
    global().estado(chave)
}

// CONTRATO: acréscimo (P4.3): a tira que a tela deixou de querer morre já, e o pedido seguinte da mesma chave roda
// de novo (`cancelada` não é falha: não espera os 10 s).
/// Cancela a tarefa viva dessa chave; devolve se havia.
pub fn cancelar(chave: &str) -> bool {
    global().cancelar(chave)
}

/// Tarefas atuais.
pub fn listar() -> Vec<TarefaFundo> {
    global().listar()
}

/// Liga quem recebe a lista a cada mudança (o integrador emite `edicao:fundo`).
#[allow(clippy::type_complexity)]
pub fn ouvir(ouvinte: Box<dyn Fn(&[TarefaFundo]) + Send + Sync>) {
    global().ouvir(Arc::from(ouvinte));
}

/// Cancela tudo e espera os trabalhadores (chamada no `ExitRequested`).
pub fn encerrar() {
    global().encerrar();
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn estado_de_fundo_tag_tipo() {
        for j in [json!({ "tipo": "na_fila" }), json!({ "tipo": "rodando" }), json!({ "tipo": "pronta" }),
                  json!({ "tipo": "falhou", "mensagem": "x" }), json!({ "tipo": "cancelada" })] {
            let e: EstadoFundo = serde_json::from_value(j.clone()).expect("lê");
            assert_eq!(serde_json::to_value(&e).unwrap(), j);
        }
        let t = json!({ "id": "f1", "tipo": "copia_previa", "chave": "copia_previa:ab12", "feito": 0.5, "estado": { "tipo": "rodando" } });
        let lida: TarefaFundo = serde_json::from_value(t.clone()).unwrap();
        assert_eq!(serde_json::to_value(&lida).unwrap(), t);
    }

    #[test]
    fn chave_leva_o_literal_do_tipo() {
        assert_eq!(chave(TipoFundo::CopiaPrevia, "ab12"), "copia_previa:ab12");
        assert_eq!(chave(TipoFundo::Sonda, "gpu"), "sonda:gpu");
    }

    /// Espera uma condição sobre a fila (com teto).
    fn ate(f: &Fundo, cond: impl Fn(&[TarefaFundo]) -> bool) -> Vec<TarefaFundo> {
        let t0 = Instant::now();
        loop {
            let l = f.listar();
            if cond(&l) {
                return l;
            }
            assert!(t0.elapsed() < Duration::from_secs(30), "fila de fundo parada: {l:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn estado_de<'a>(l: &'a [TarefaFundo], chave: &str) -> &'a EstadoFundo {
        &l.iter().find(|t| t.chave == chave).expect("tarefa").estado
    }

    /// Trabalho que espera a liberação, contando quantos rodam ao mesmo tempo.
    fn trabalho_preso(liberar: Arc<AtomicBool>, juntos: Arc<AtomicUsize>, maximo: Arc<AtomicUsize>) -> Trabalho {
        Box::new(move |c, prog| {
            let n = juntos.fetch_add(1, Ordering::SeqCst) + 1;
            maximo.fetch_max(n, Ordering::SeqCst);
            prog(0.5);
            while !liberar.load(Ordering::SeqCst) {
                if c.load(Ordering::SeqCst) {
                    juntos.fetch_sub(1, Ordering::SeqCst);
                    return Err(SicroError::Validation("cancelado".into()));
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            juntos.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        })
    }

    #[test]
    fn dois_trabalhadores_chave_unica_frente_e_falha() {
        let f = Fundo::nova(2);
        let eventos = Arc::new(Mutex::new(Vec::<Vec<TarefaFundo>>::new()));
        let ev = eventos.clone();
        f.ouvir(Arc::new(move |l: &[TarefaFundo]| ev.lock().unwrap().push(l.to_vec())));
        let (liberar, juntos, maximo) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let preso = || trabalho_preso(liberar.clone(), juntos.clone(), maximo.clone());

        // `a` segura um trabalhador até o fim: `d` e `c` passam em série pelo outro, e a ordem de início não corre
        let liberar_a = Arc::new(AtomicBool::new(false));
        let a = f.pedir(TipoFundo::Tabela, "tabela:a", trabalho_preso(liberar_a.clone(), juntos.clone(), maximo.clone()), false);
        let b = f.pedir(TipoFundo::Tabela, "tabela:b", preso(), false);
        ate(&f, |l| l.iter().filter(|t| t.estado == EstadoFundo::Rodando).count() == 2);
        let c = f.pedir(TipoFundo::Proxy, "proxy:c", preso(), false);
        let d = f.pedir(TipoFundo::Tabela, "tabela:d", preso(), true);
        // chave repetida (rodando ou na fila) reaproveita: o trabalho novo é descartado
        let rodou_o_repetido = Arc::new(AtomicBool::new(false));
        let r = rodou_o_repetido.clone();
        assert_eq!(f.pedir(TipoFundo::Tabela, "tabela:a", Box::new(move |_, _| { r.store(true, Ordering::SeqCst); Ok(()) }), false), a);
        assert_eq!(f.pedir(TipoFundo::Proxy, "proxy:c", preso(), false), c);
        // "na frente" foi para o começo da lista e roda antes da que chegou antes
        let l = f.listar();
        assert_eq!(l.iter().position(|t| t.id == d), Some(0), "{l:?}");
        assert_eq!(*estado_de(&l, "proxy:c"), EstadoFundo::NaFila);
        assert_eq!(l.iter().find(|t| t.id == a).unwrap().feito, 0.5);

        liberar.store(true, Ordering::SeqCst);
        ate(&f, |l| l.iter().filter(|t| terminal(&t.estado)).count() == 3);
        liberar_a.store(true, Ordering::SeqCst);
        let l = ate(&f, |l| l.iter().all(|t| terminal(&t.estado)));
        assert!(l.iter().all(|t| t.estado == EstadoFundo::Pronta && t.feito == 1.0), "{l:?}");
        assert_eq!(maximo.load(Ordering::SeqCst), 2, "2 trabalhadores");
        assert!(!rodou_o_repetido.load(Ordering::SeqCst));
        assert_eq!([&a, &b, &c, &d].iter().collect::<std::collections::HashSet<_>>().len(), 4);

        // ordem de início: d (na frente) antes de c
        let ev = eventos.lock().unwrap().clone();
        let comecou = |id: &str| ev.iter().position(|l| l.iter().any(|t| t.id == id && t.estado != EstadoFundo::NaFila)).unwrap();
        assert!(comecou(&d) < comecou(&c), "a da frente começa antes");

        // falha e pânico viram `falhou`; o mesmo pedido de novo roda de novo
        f.pedir(TipoFundo::Sonda, "sonda:gpu", Box::new(|_, _| Err(SicroError::Validation("sem_gpu: x".into()))), false);
        f.pedir(TipoFundo::Onda, "onda:p", Box::new(|_, _| panic!("de propósito")), false);
        let l = ate(&f, |l| l.iter().filter(|t| matches!(t.estado, EstadoFundo::Falhou { .. })).count() == 2);
        assert_eq!(*estado_de(&l, "sonda:gpu"), EstadoFundo::Falhou { mensagem: "sem_gpu: x".into() });
        // falhou há menos de 10 s: o pedido comum não roda de novo; o da frente (exportação) roda
        let rodou = Arc::new(AtomicBool::new(false));
        let r = rodou.clone();
        let falha = estado_de(&f.listar(), "sonda:gpu").clone();
        let mesmo = f.pedir(TipoFundo::Sonda, "sonda:gpu", Box::new(move |_, _| { r.store(true, Ordering::SeqCst); Ok(()) }), false);
        std::thread::sleep(Duration::from_millis(50));
        assert!(!rodou.load(Ordering::SeqCst), "refeito a cada pedido");
        assert_eq!((estado_de(&f.listar(), "sonda:gpu"), f.listar().iter().find(|t| t.chave == "sonda:gpu").unwrap().id.clone()), (&falha, mesmo.clone()));
        // só prioridade (a tela, em mídia sem tempo nos pacotes): também não refaz
        let r = rodou.clone();
        let prioridade = f.pedir_com(TipoFundo::Sonda, "sonda:gpu", Box::new(move |_, _| { r.store(true, Ordering::SeqCst); Ok(()) }), true, false);
        std::thread::sleep(Duration::from_millis(50));
        assert!(!rodou.load(Ordering::SeqCst) && prioridade == mesmo, "prioridade refez o que falhou há pouco");
        let id = f.pedir(TipoFundo::Sonda, "sonda:gpu", Box::new(|_, _| Ok(())), true);
        ate(&f, |l| l.iter().any(|t| t.id == id && t.estado == EstadoFundo::Pronta));
        f.encerrar();
    }

    /// Depois dos 10 s (aqui encurtados) o pedido comum refaz o que falhou.
    #[test]
    fn falha_vence_a_espera() {
        // espera encurtada; o sono abaixo dá 3× de folga (máquina carregada: falhou uma vez com 1,5×)
        const ESPERA: Duration = Duration::from_millis(100);
        let mut f = Fundo::nova(1);
        Arc::get_mut(&mut f).unwrap().espera_falha = ESPERA;
        f.pedir(TipoFundo::Proxy, "proxy:x", Box::new(|_, _| Err(SicroError::Validation("fonte_danificada: x".into()))), false);
        ate(&f, |l| l.iter().any(|t| matches!(t.estado, EstadoFundo::Falhou { .. })));
        let antes = f.pedir(TipoFundo::Proxy, "proxy:x", Box::new(|_, _| Ok(())), false);
        assert!(matches!(estado_de(&f.listar(), "proxy:x"), EstadoFundo::Falhou { .. }));
        std::thread::sleep(ESPERA * 3);
        let depois = f.pedir(TipoFundo::Proxy, "proxy:x", Box::new(|_, _| Ok(())), false);
        assert_ne!(antes, depois);
        ate(&f, |l| l.iter().any(|t| t.id == depois && t.estado == EstadoFundo::Pronta));
        f.encerrar();
    }

    #[test]
    fn encerrar_cancela_o_que_roda_e_o_que_espera() {
        let f = Fundo::nova(1);
        let (liberar, juntos, maximo) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        f.pedir(TipoFundo::Proxy, "proxy:1", trabalho_preso(liberar.clone(), juntos.clone(), maximo.clone()), false);
        f.pedir(TipoFundo::Proxy, "proxy:2", trabalho_preso(liberar, juntos.clone(), maximo), false);
        ate(&f, |l| l.iter().any(|t| t.estado == EstadoFundo::Rodando));
        f.encerrar();
        let l = f.listar();
        assert!(l.iter().all(|t| t.estado == EstadoFundo::Cancelada), "{l:?}");
        assert_eq!(juntos.load(Ordering::SeqCst), 0);
        // depois de encerrar, um pedido novo liga trabalhadores de novo
        let id = f.pedir(TipoFundo::Proxy, "proxy:3", Box::new(|_, _| Ok(())), false);
        ate(&f, |l| l.iter().any(|t| t.id == id && t.estado == EstadoFundo::Pronta));
        f.encerrar();
    }

    /// Cancelar por chave (P4.3): a que roda acaba `cancelada` (não `falhou`), a da fila sai sem rodar, chave sem
    /// tarefa viva devolve false, e o mesmo pedido logo depois roda de novo (sem a espera de 10 s da falha).
    #[test]
    fn cancelar_por_chave() {
        let f = Fundo::nova(1);
        let (liberar, juntos, maximo) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        f.pedir(TipoFundo::Tira, "tira:a", trabalho_preso(liberar.clone(), juntos.clone(), maximo.clone()), false);
        let rodou_b = Arc::new(AtomicBool::new(false));
        let r = rodou_b.clone();
        f.pedir(TipoFundo::Tira, "tira:b", Box::new(move |_, _| { r.store(true, Ordering::SeqCst); Ok(()) }), false);
        ate(&f, |l| l.iter().any(|t| t.chave == "tira:a" && t.estado == EstadoFundo::Rodando));
        assert!(f.cancelar("tira:b"));
        assert_eq!(*estado_de(&f.listar(), "tira:b"), EstadoFundo::Cancelada);
        assert!(f.cancelar("tira:a"));
        ate(&f, |l| l.iter().all(|t| terminal(&t.estado)));
        assert_eq!(*estado_de(&f.listar(), "tira:a"), EstadoFundo::Cancelada);
        assert!(!rodou_b.load(Ordering::SeqCst), "a cancelada na fila rodou");
        assert!(!f.cancelar("tira:a") && !f.cancelar("tira:nenhuma"));
        let id = f.pedir(TipoFundo::Tira, "tira:a", Box::new(|_, _| Ok(())), false);
        ate(&f, |l| l.iter().any(|t| t.id == id && t.estado == EstadoFundo::Pronta));
        f.encerrar();
    }

    /// Progresso que muda a cada milissegundo: no máximo 5 avisos por segundo além das mudanças de estado.
    #[test]
    fn progresso_de_fundo_limitado() {
        let f = Fundo::nova(1);
        let quando = Arc::new(Mutex::new(Vec::<(Instant, Vec<TarefaFundo>)>::new()));
        let q = quando.clone();
        f.ouvir(Arc::new(move |l: &[TarefaFundo]| q.lock().unwrap().push((Instant::now(), l.to_vec()))));
        f.pedir(
            TipoFundo::Tabela,
            "tabela:p",
            Box::new(|_, prog| {
                let t0 = Instant::now();
                while t0.elapsed() < Duration::from_millis(1500) {
                    prog(t0.elapsed().as_secs_f32() / 1.5);
                    std::thread::sleep(Duration::from_millis(1));
                }
                Ok(())
            }),
            false,
        );
        ate(&f, |l| l.iter().all(|t| t.estado == EstadoFundo::Pronta));
        let ev = quando.lock().unwrap().clone();
        // o aviso da mudança de estado (feito 0) não conta
        let so_progresso: Vec<Instant> = ev
            .iter()
            .filter(|(_, l)| l.iter().all(|t| t.estado == EstadoFundo::Rodando && t.feito > 0.0))
            .map(|(t, _)| *t)
            .collect();
        assert!(so_progresso.len() >= 3, "{}", so_progresso.len());
        for (i, t) in so_progresso.iter().enumerate() {
            let na_janela = so_progresso[i..].iter().filter(|u| u.duration_since(*t) < Duration::from_secs(1)).count();
            assert!(na_janela <= 5, "{na_janela} avisos em 1 s");
        }
        f.encerrar();
    }
}
