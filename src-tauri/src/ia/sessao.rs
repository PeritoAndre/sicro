//! Fábrica única de sessão (§6.5; protótipo `p2/proto/src/sessao_ia.rs`). Um onnxruntime por processo: a lib de
//! GPU quando `gpu::pronta`, senão a de CPU (Pessoas ou super-resolução). CUDA com `error_on_failure`,
//! `use_tf32 = 0`, busca heurística e aquecimento; qualquer erro refaz em CPU com o motivo. Dono: P2.1.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use ort::environment::Environment;
use ort::ep::{self, ExecutionProvider};
use ort::session::builder::SessionBuilder;
use ort::session::Session;
use serde::{Deserialize, Serialize};

use super::gpu::{self, PacoteGpu};
use crate::commands::settings_commands::AiSettings;
use crate::edicao::mensagens::codigo;
use crate::error::{Result, SicroError};

/// Versão das libs que o Gerenciador instala (CPU e GPU). O nome da DLL do Windows não traz a versão.
pub const VERSAO_ONNXRUNTIME: &str = "1.31.1";

/// Onde a sessão rodou. No fio e nos registros (`Corrida.provedor`, `ProvIa.provedor`): `cpu` | `cuda`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provedor {
    Cpu,
    Cuda,
}

impl Provedor {
    /// Texto gravado nas corridas e no `tecnico.json`.
    pub fn fio(self) -> &'static str {
        match self {
            Provedor::Cpu => "cpu",
            Provedor::Cuda => "cuda",
        }
    }
}

/// `Auto`: GPU se pronta, senão CPU com o motivo em `SessaoIa::queda`. `Cpu`: nunca tenta GPU (VitTrack).
/// `Gpu`: erro se a GPU não servir (autoteste; os grafos seguintes do EdgeTAM depois que o primeiro ficou na GPU).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preferencia {
    Auto,
    Cpu,
    Gpu,
}

#[derive(Debug, Clone)]
pub struct Opcoes {
    pub preferencia: Preferencia,
    pub threads_cpu: usize,
    /// Forma NCHW f32 da primeira entrada para a inferência de aquecimento na GPU (a falta do cuDNN e da memória de
    /// vídeo só aparece nela); None = sem aquecimento (grafo de várias entradas: aquecer o primeiro).
    pub aquecer: Option<[usize; 4]>,
    /// Teto da arena CUDA em MiB; None = padrão do provedor.
    pub vram_max_mb: Option<usize>,
}

impl Default for Opcoes {
    fn default() -> Self {
        Self {
            preferencia: Preferencia::Auto,
            threads_cpu: std::thread::available_parallelism().map_or(4, |n| n.get()).min(8),
            aquecer: None,
            vram_max_mb: None,
        }
    }
}

impl Opcoes {
    /// "Só CPU" (VitTrack, §6.5).
    pub fn so_cpu() -> Self {
        Self { preferencia: Preferencia::Cpu, ..Self::default() }
    }
}

pub struct SessaoIa {
    pub session: ort::session::Session,
    pub provedor: Provedor,
    /// Código do catálogo (`gpu_*`) de por que não ficou na GPU, quando `Auto` caiu para CPU.
    pub queda: Option<String>,
}

/// De onde vem o grafo.
#[derive(Debug, Clone, Copy)]
pub enum Modelo<'a> {
    Arquivo(&'a Path),
    /// Grafo embutido no binário (VitTrack, modelo do autoteste).
    Bytes(&'a [u8]),
}

// ---------------------------------------------------------------------------
// Configurações em vigor (análise: `Yolox::load` e `SuperRes::load` não recebem `AiSettings`)

static VIGENTES: RwLock<Option<AiSettings>> = RwLock::new(None);

/// Gravadas pelas Configurações a cada leitura e gravação (`settings_commands`).
pub fn lembrar(ai: &AiSettings) {
    *VIGENTES.write().unwrap_or_else(|e| e.into_inner()) = Some(ai.clone());
}

/// As em vigor, com a lib de CPU pedida por quem chama (o interruptor e a GPU seguem as Configurações).
pub fn vigentes_com_lib(lib: &Path) -> AiSettings {
    com_lib(VIGENTES.read().unwrap_or_else(|e| e.into_inner()).clone(), lib)
}

fn com_lib(base: Option<AiSettings>, lib: &Path) -> AiSettings {
    AiSettings { ort_lib_path: lib.to_string_lossy().into_owned(), sr_ort_lib_path: String::new(), ..base.unwrap_or_default() }
}

// ---------------------------------------------------------------------------
// Ambiente (um por processo)

struct Carga {
    lib: PathBuf,
}

static ENV: OnceLock<Environment> = OnceLock::new();
static CARGA: OnceLock<Carga> = OnceLock::new();

fn erro(e: impl std::fmt::Display) -> SicroError {
    SicroError::Validation(format!("onnxruntime: {e}"))
}

/// A variável vem do `main.rs` (no app) ou de `ia::testes::ort_lib()` (no `cargo test`); sem ela a lib não carrega:
/// `with_telemetry(false)` sozinho não impede o envio (`gpu-inferencia.md` §8).
fn telemetria_desligada() -> bool {
    std::env::var("ORT_DISABLE_TELEMETRY").is_ok_and(|v| v == "1")
}

/// Ambiente único do processo (o `ort` só admite um; telemetria desligada). `video/ort_env.rs` vira ponte para cá.
pub fn ambiente(lib: &Path) -> Result<&'static Environment> {
    if let Some(e) = ENV.get() {
        return Ok(e);
    }
    static INICIO: Mutex<()> = Mutex::new(());
    let _g = INICIO.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(e) = ENV.get() {
        return Ok(e);
    }
    if !telemetria_desligada() {
        return Err(erro("ORT_DISABLE_TELEMETRY=1 ausente: a biblioteca não é carregada"));
    }
    if !lib.is_file() {
        return Err(SicroError::Validation(format!("{}: onnxruntime", codigo::MODELO_AUSENTE)));
    }
    let b = ort::init_from(lib).map_err(erro)?;
    // a lib já está no processo, mesmo se o ambiente falhar adiante
    let _ = CARGA.set(Carga { lib: lib.to_path_buf() });
    let env = b
        .with_telemetry(false)
        .with_logger(Arc::new(|nivel: ort::logging::LogLevel, _cat: &str, _id: &str, onde: &str, msg: &str| {
            if nivel >= ort::logging::LogLevel::Warning {
                tracing::warn!(target: "onnxruntime", "{onde}: {msg}");
            }
        }))
        .build()
        .map_err(erro)?;
    let _ = ENV.set(env);
    ENV.get().ok_or_else(|| erro("ambiente não iniciado"))
}

/// Biblioteca já carregada neste processo (None = nenhuma). Outra pedida depois = reabrir o SICRO (`reabrir`).
pub fn carregada() -> Option<PathBuf> {
    CARGA.get().map(|c| c.lib.clone())
}

/// A lib carregada traz o provedor CUDA (lib "de GPU")?
fn lib_tem_cuda() -> bool {
    ENV.get().is_some() && ep::CUDA::is_available().unwrap_or(false)
}

/// Que lib está no processo: `cuda` = a de GPU (também roda em CPU); None = nenhuma.
pub fn provedor_carregado() -> Option<Provedor> {
    ENV.get()?;
    Some(if lib_tem_cuda() { Provedor::Cuda } else { Provedor::Cpu })
}

/// "onnxruntime 1.31.1": versão da lib carregada, para `Corrida.runtime` e `ProvIa.runtime`.
pub fn versao_runtime() -> String {
    let Some(c) = CARGA.get() else { return String::new() };
    format!("onnxruntime {}", versao_da_lib(&c.lib))
}

/// Pelo nome do arquivo (`libonnxruntime.so.1.31.1`, também atrás de atalho); senão a versão que o Gerenciador instala.
fn versao_da_lib(lib: &Path) -> String {
    let real = std::fs::canonicalize(lib).unwrap_or_else(|_| lib.to_path_buf());
    for p in [lib, real.as_path()] {
        let nome = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if let Some(v) = nome.strip_prefix("libonnxruntime.so.") {
            if v.split('.').count() == 3 && v.split('.').all(|x| x.parse::<u32>().is_ok()) {
                return v.to_string();
            }
        }
    }
    VERSAO_ONNXRUNTIME.to_string()
}

// ---------------------------------------------------------------------------
// Pré-carga das bibliotecas da NVIDIA (dependência antes; `gpu-inferencia.md` §3)

#[cfg(not(windows))]
const CUDA_LIBS: &[(&str, bool)] = &[
    ("libcudart.so.", true),
    ("libnvJitLink.so.", false),
    ("libnvrtc.so.", false),
    ("libcublasLt.so.", true),
    ("libcublas.so.", true),
    ("libcurand.so.", true),
    ("libcufft.so.", false),
];
#[cfg(windows)]
const CUDA_LIBS: &[(&str, bool)] = &[
    ("cudart64_", true),
    ("nvjitlink_", false),
    ("nvrtc64_", false),
    ("cublaslt64_", true),
    ("cublas64_", true),
    // no Windows o provedor não importa o curand
    ("curand64_", false),
    ("cufft64_", false),
];

/// No Linux basta o `libcudnn.so.9` (abre as irmãs sozinho); no Windows entram todas, dependência primeiro.
#[cfg(not(windows))]
const CUDNN_LIBS: &[(&str, bool)] = &[("libcudnn.so.", true)];
#[cfg(windows)]
const CUDNN_LIBS: &[(&str, bool)] = &[
    ("cudnn_graph64_", true),
    ("cudnn_ops64_", true),
    ("cudnn_heuristic64_", false),
    ("cudnn_adv64_", false),
    ("cudnn_cnn64_", true),
    ("cudnn_engines_precompiled64_", true),
    ("cudnn_engines_runtime_compiled64_", false),
    ("cudnn_engines_tensor_ir64_", false),
    ("cudnn_ext64_", false),
    ("cudnn64_", true),
];

fn achar(dir: &Path, prefixo: &str) -> Option<PathBuf> {
    let mut achados: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if cfg!(windows) {
                let n = n.to_lowercase();
                n.starts_with(prefixo) && n.ends_with(".dll")
            } else {
                n.starts_with(prefixo)
            }
        })
        .collect();
    achados.sort();
    achados.into_iter().next()
}

/// `rotulo` (`cuda` | `cudnn`) entra na mensagem: é o que separa `gpu_sem_cudnn` de `gpu_sem_bibliotecas`.
fn precarregar(rotulo: &str, dir: &Path, lista: &[(&str, bool)]) -> std::result::Result<(), String> {
    for (prefixo, obrigatoria) in lista {
        match achar(dir, prefixo) {
            Some(p) => match ort::util::preload_dylib(&p) {
                Ok(()) => {}
                Err(e) if *obrigatoria => return Err(format!("{rotulo}: {}: {e}", p.display())),
                Err(_) => {}
            },
            None if *obrigatoria => return Err(format!("{rotulo}: {prefixo}* não está em {}", dir.display())),
            None => {}
        }
    }
    Ok(())
}

static PRECARGA: OnceLock<(PacoteGpu, std::result::Result<(), String>)> = OnceLock::new();

/// Carrega CUDA e depois cuDNN das pastas do pacote, uma vez por processo. Outro pacote depois: `reabrir`.
fn preparar_gpu(p: &PacoteGpu) -> std::result::Result<(), Queda> {
    let (feito, r) = PRECARGA.get_or_init(|| {
        #[cfg(windows)]
        {
            // o carregador do Windows resolve pelo PATH o que as DLLs pedirem depois
            let mut path = std::env::var_os("PATH").unwrap_or_default();
            for d in [&p.cudnn, &p.cuda] {
                let mut novo = d.clone().into_os_string();
                novo.push(";");
                novo.push(&path);
                path = novo;
            }
            std::env::set_var("PATH", path);
        }
        let r = precarregar("cuda", &p.cuda, CUDA_LIBS).and_then(|_| precarregar("cudnn", &p.cudnn, CUDNN_LIBS));
        (p.clone(), r)
    });
    if feito.cuda != p.cuda || feito.cudnn != p.cudnn {
        return Err(Queda::com(codigo::REABRIR, "outras bibliotecas da NVIDIA já carregadas neste processo"));
    }
    r.clone().map_err(|m| Queda::de(&m))
}

// ---------------------------------------------------------------------------
// Sessão

/// Por que a GPU não serviu: código do catálogo (tela) e o texto do onnxruntime (log).
#[derive(Debug, Clone)]
pub(crate) struct Queda {
    pub codigo: &'static str,
    pub detalhe: String,
}

impl Queda {
    fn com(codigo: &'static str, detalhe: &str) -> Self {
        Self { codigo, detalhe: detalhe.to_string() }
    }
    pub(crate) fn de(detalhe: &str) -> Self {
        Self { codigo: gpu::motivo(detalhe), detalhe: detalhe.to_string() }
    }
}

fn base(env: &Environment, op: &Opcoes, perfil: Option<&Path>) -> std::result::Result<SessionBuilder, String> {
    let mut b = Session::builder(env).map_err(|e| e.to_string())?.with_intra_threads(op.threads_cpu.max(1)).map_err(|e| e.to_string())?;
    if let Some(p) = perfil {
        b = b.with_profiling(p).map_err(|e| e.to_string())?;
    }
    Ok(b)
}

fn ep_cuda(op: &Opcoes) -> ep::ExecutionProviderDispatch {
    let mut c = ep::CUDA::default()
        .with_device_id(0)
        .with_conv_algorithm_search(ep::cuda::ConvAlgorithmSearch::Heuristic)
        // TF32 afasta a saída da CPU (0,015 contra 0,00004 no YOLOX)
        .with_tf32(false);
    if let Some(mb) = op.vram_max_mb {
        c = c.with_memory_limit(mb << 20).with_arena_extend_strategy(ep::ArenaExtendStrategy::SameAsRequested);
    }
    c.build().error_on_failure()
}

fn commit(b: &mut SessionBuilder, modelo: Modelo<'_>) -> std::result::Result<Session, String> {
    match modelo {
        Modelo::Arquivo(p) => b.commit_from_file(p),
        Modelo::Bytes(m) => b.commit_from_memory(m),
    }
    .map_err(|e| e.to_string())
}

/// Roda a primeira entrada com zeros: a falta do cuDNN e da memória de vídeo só aparecem aqui.
fn aquecer(s: &mut Session, forma: [usize; 4]) -> std::result::Result<(), String> {
    let nome = s.inputs().first().ok_or("modelo sem entrada")?.name().to_string();
    let t = ort::value::Tensor::from_array((forma, vec![0.0f32; forma.iter().product()])).map_err(|e| e.to_string())?;
    s.run(ort::inputs![nome.as_str() => t]).map(|_| ()).map_err(|e| e.to_string())
}

/// Sessão no provedor CUDA, ou o porquê de não. `perfil` = prefixo do perfil do onnxruntime (autoteste).
pub(crate) fn sessao_gpu(lib: &Path, pacote: Option<&PacoteGpu>, modelo: Modelo<'_>, op: &Opcoes, perfil: Option<&Path>) -> std::result::Result<Session, Queda> {
    let env = ambiente(lib).map_err(|e| Queda::de(&e.to_string()))?;
    let Some(p) = pacote else { return Err(Queda::com(codigo::GPU_NAO_INSTALADA, "sem o pacote de GPU")) };
    if !lib_tem_cuda() {
        let outra = carregada().is_some_and(|l| l != p.lib);
        return Err(if outra {
            Queda::com(codigo::REABRIR, "a biblioteca do onnxruntime carregada é a de CPU")
        } else {
            Queda::com(codigo::GPU_NAO_INSTALADA, "a biblioteca do onnxruntime é a de CPU")
        });
    }
    preparar_gpu(p)?;
    let mut b = base(env, op, perfil).map_err(|m| Queda::de(&m))?;
    b = b.with_execution_providers([ep_cuda(op)]).map_err(|e| Queda::de(&e.to_string()))?;
    let mut s = commit(&mut b, modelo).map_err(|m| Queda::de(&m))?;
    if let Some(forma) = op.aquecer {
        aquecer(&mut s, forma).map_err(|m| Queda::de(&m))?;
    }
    Ok(s)
}

/// O texto do onnxruntime chega a vários kB (JSON do cuDNN): inteiro no log, cortado no erro.
fn cortar(s: &str) -> String {
    s.chars().take(300).collect()
}

/// Com a biblioteca explícita: autoteste no processo filho e testes (`ia::testes::ambiente`). Não olha o interruptor.
pub fn criar_sessao_com(lib: &Path, pacote: Option<&PacoteGpu>, modelo: Modelo<'_>, op: &Opcoes) -> Result<SessaoIa> {
    let env = ambiente(lib)?;
    let mut queda = None;
    if op.preferencia != Preferencia::Cpu && (pacote.is_some() || op.preferencia == Preferencia::Gpu) {
        match sessao_gpu(lib, pacote, modelo, op, None) {
            Ok(session) => return Ok(SessaoIa { session, provedor: Provedor::Cuda, queda: None }),
            Err(q) => {
                tracing::warn!(target: "ia", "GPU não usada ({}): {}", q.codigo, q.detalhe);
                if op.preferencia == Preferencia::Gpu {
                    return Err(SicroError::Validation(format!("{}: {}", q.codigo, cortar(&q.detalhe))));
                }
                queda = Some(q.codigo.to_string());
            }
        }
    }
    let mut b = base(env, op, None).map_err(erro)?;
    let session = commit(&mut b, modelo).map_err(erro)?;
    Ok(SessaoIa { session, provedor: Provedor::Cpu, queda })
}

/// Biblioteca e pacote de GPU que `criar_sessao` usa: a lib de GPU quando `gpu::pronta(ai)`, senão a de CPU
/// (`ort_lib_path`, depois `sr_ort_lib_path`). None = nenhum onnxruntime instalado (`modelo_ausente: onnxruntime`).
pub fn biblioteca(ai: &AiSettings) -> Option<(PathBuf, Option<PacoteGpu>)> {
    if let Some(p) = gpu::pronta(ai) {
        return Some((p.lib.clone(), Some(p)));
    }
    [&ai.ort_lib_path, &ai.sr_ort_lib_path]
        .iter()
        .map(|s| PathBuf::from(s.as_str()))
        .find(|p| !p.as_os_str().is_empty() && p.is_file())
        .map(|l| (l, None))
}

fn com_ai(ai: &AiSettings, modelo: Modelo<'_>, op: &Opcoes) -> Result<SessaoIa> {
    if !super::ligada(ai) {
        return Err(SicroError::Validation(codigo::IA_DESLIGADA.into()));
    }
    let (lib, pacote) = biblioteca(ai).ok_or_else(|| SicroError::Validation(format!("{}: onnxruntime", codigo::MODELO_AUSENTE)))?;
    criar_sessao_com(&lib, pacote.as_ref(), modelo, op)
}

/// A função única (§6.5). Interruptor desligado: `Validation("ia_desligada")`.
pub fn criar_sessao(ai: &AiSettings, modelo: &Path, op: &Opcoes) -> Result<SessaoIa> {
    com_ai(ai, Modelo::Arquivo(modelo), op)
}

/// Mesma regra, com o grafo em memória.
pub fn criar_sessao_de_bytes(ai: &AiSettings, modelo: &[u8], op: &Opcoes) -> Result<SessaoIa> {
    com_ai(ai, Modelo::Bytes(modelo), op)
}

/// Roda a primeira entrada f32 NCHW e devolve a primeira saída achatada.
pub(crate) fn rodar(s: &mut Session, forma: [usize; 4], dados: Vec<f32>) -> std::result::Result<Vec<f32>, String> {
    let nome = s.inputs().first().ok_or("modelo sem entrada")?.name().to_string();
    let t = ort::value::Tensor::from_array((forma, dados)).map_err(|e| e.to_string())?;
    let saidas = s.run(ort::inputs![nome.as_str() => t]).map_err(|e| e.to_string())?;
    let (_, v) = saidas[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
    Ok(v.to_vec())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::ia::gpu::{testes as g, FORMA_AUTOTESTE, MODELO_AUTOTESTE, TOLERANCIA_AUTOTESTE};
    use crate::ia::testes as t;

    /// Desligado: as três portas recusam com `ia_desligada` antes de procurar a lib; ligado sem lib: `modelo_ausente`.
    #[test]
    fn desligada_recusa() {
        let desl = AiSettings { modelos_desligados: true, ort_lib_path: "/nao/existe.so".into(), ..Default::default() };
        let msg = |r: Result<SessaoIa>| match r {
            Err(SicroError::Validation(m)) => m,
            Err(e) => e.to_string(),
            Ok(_) => "sessão criada".into(),
        };
        assert_eq!(msg(criar_sessao(&desl, Path::new("/x.onnx"), &Opcoes::default())), codigo::IA_DESLIGADA);
        assert_eq!(msg(criar_sessao_de_bytes(&desl, MODELO_AUTOTESTE, &Opcoes::so_cpu())), codigo::IA_DESLIGADA);
        let ligada = AiSettings { modelos_desligados: false, ..desl.clone() };
        assert_eq!(msg(criar_sessao_de_bytes(&ligada, MODELO_AUTOTESTE, &Opcoes::so_cpu())), "modelo_ausente: onnxruntime");
        // a análise (sem `AiSettings`) segue o interruptor das Configurações em vigor
        let v = AiSettings { modelos_desligados: true, gpu_dir: "/g".into(), sr_ort_lib_path: "/sr.so".into(), ..Default::default() };
        let c = com_lib(Some(v), Path::new("/lib.so"));
        assert!(c.modelos_desligados && c.gpu_dir == "/g" && c.ort_lib_path == "/lib.so" && c.sr_ort_lib_path.is_empty());
        let c = com_lib(None, Path::new("/lib.so"));
        assert!(!c.modelos_desligados && c.gpu_dir.is_empty() && c.ort_lib_path == "/lib.so");
    }

    #[test]
    fn versao_pelo_nome_da_lib() {
        assert_eq!(versao_da_lib(Path::new("/a/libonnxruntime.so.1.31.1")), "1.31.1");
        assert_eq!(versao_da_lib(Path::new("/a/libonnxruntime.so.1.29.0")), "1.29.0");
        assert_eq!(versao_da_lib(Path::new("C:/a/onnxruntime.dll")), VERSAO_ONNXRUNTIME);
    }

    /// O autoteste e o YOLOX dão na GPU o mesmo que na CPU (sem TF32): até 1e-3 no autoteste e 1e-3 no YOLOX
    /// (o protótipo mediu 1,2e-7 e 3,5e-5); a sessão de GPU fica no CUDA e a de CPU no CPU.
    #[test]
    fn gpu_igual_a_cpu_no_autoteste() {
        let _trava = t::trava_gpu();
        let Some(p) = t::ou_dispensa(t::pacote_gpu(), "gpu_igual_a_cpu_no_autoteste", "a lib e as bibliotecas da NVIDIA") else { return };
        if t::ou_dispensa_gpu(gpu::placa(), "gpu_igual_a_cpu_no_autoteste", "placa NVIDIA").is_none() {
            return;
        }
        let entrada = gpu::entrada_de_teste(FORMA_AUTOTESTE.iter().product());
        let mut cpu = criar_sessao_com(&p.lib, None, Modelo::Bytes(MODELO_AUTOTESTE), &Opcoes::so_cpu()).expect("cpu");
        assert_eq!(cpu.provedor, Provedor::Cpu);
        let op = Opcoes { preferencia: Preferencia::Gpu, aquecer: Some(FORMA_AUTOTESTE), ..Default::default() };
        let mut gpu = criar_sessao_com(&p.lib, Some(&p), Modelo::Bytes(MODELO_AUTOTESTE), &op).expect("gpu");
        assert_eq!((gpu.provedor, gpu.queda.as_deref()), (Provedor::Cuda, None));
        let a = rodar(&mut cpu.session, FORMA_AUTOTESTE, entrada.clone()).unwrap();
        let b = rodar(&mut gpu.session, FORMA_AUTOTESTE, entrada).unwrap();
        assert_eq!(a.len(), b.len());
        let dif = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max);
        assert!(dif <= TOLERANCIA_AUTOTESTE, "autoteste: GPU difere da CPU em {dif}");
        assert!(a.iter().any(|v| *v != 0.0), "saída nula: a comparação não mede nada");
        drop(gpu);

        let Some(yolox) = t::ou_dispensa(g::yolox(), "gpu_igual_a_cpu_no_autoteste", "yolox_s.onnx") else { return };
        let forma = [1, 3, 640, 640];
        let q = g::quadro_yolox();
        let mut cpu = criar_sessao_com(&p.lib, None, Modelo::Arquivo(&yolox), &Opcoes::so_cpu()).unwrap();
        let mut gpu = criar_sessao_com(&p.lib, Some(&p), Modelo::Arquivo(&yolox), &Opcoes { aquecer: Some(forma), ..Default::default() }).unwrap();
        assert_eq!(gpu.provedor, Provedor::Cuda, "queda: {:?}", gpu.queda);
        let a = rodar(&mut cpu.session, forma, q.clone()).unwrap();
        let b = rodar(&mut gpu.session, forma, q).unwrap();
        let dif = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max);
        eprintln!("YOLOX GPU × CPU: diferença máxima {dif}");
        assert!(dif <= 1e-3, "YOLOX: GPU difere da CPU em {dif}");
    }

    /// Sem o cuDNN a sessão CUDA nasce e só a primeira inferência falha (`p2/out/17`): com aquecimento a fábrica cai
    /// para CPU com `gpu_sem_cudnn` e a saída igual à da CPU; sem aquecimento a sessão diz CUDA e quebra no uso.
    #[test]
    fn aquecimento_pega_falta_do_cudnn() {
        let Some(r) = g::cenario("cudnn_na_inferencia", "aquecimento_pega_falta_do_cudnn") else { return };
        let a = r.auto.expect("linha AUTO do filho");
        assert_eq!(a.com_aquecimento.provedor, Provedor::Cpu, "{a:?}");
        assert_eq!(a.com_aquecimento.queda.as_deref(), Some(codigo::GPU_SEM_CUDNN), "{a:?}");
        assert_eq!(a.com_aquecimento.dif, Some(0.0), "{a:?}");
        assert_eq!(a.sem_aquecimento.provedor, Provedor::Cuda, "{a:?}");
        assert_eq!(a.sem_aquecimento.erro.as_deref(), Some(codigo::GPU_SEM_CUDNN), "{a:?}");
        assert!(!r.autoteste.pronta);
        assert_eq!(r.autoteste.motivo.as_deref(), Some(codigo::GPU_SEM_CUDNN));
    }
}
