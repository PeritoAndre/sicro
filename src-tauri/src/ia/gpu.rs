//! Aceleração por GPU (NVIDIA): item do Gerenciador em `app_local_data_dir()/ai/gpu/` e autoteste em processo
//! filho (§6.5; `gpu-inferencia.md` §7). "Pronta" só depois do autoteste, refeito quando o driver muda. Dono: P2.1.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::pacote::{self, Arquivo, Fonte};
use super::sessao::{self, Modelo, Opcoes, Preferencia, Provedor};
use crate::commands::settings_commands::AiSettings;
use crate::edicao::mensagens::codigo;
use crate::error::{Result, SicroError};

/// Argumento do binário que roda o autoteste e imprime `Autoteste` em JSON (uma linha) no stdout.
pub const ARG_AUTOTESTE: &str = "--autoteste-gpu";
/// Subpastas de `ai/gpu/` (`gpu-inferencia.md` §7.2): `onnxruntime/lib/`, `cuda/`, `cudnn/` e o `estado.json`.
pub const PASTA_ONNXRUNTIME: &str = "onnxruntime";
pub const PASTA_CUDA: &str = "cuda";
pub const PASTA_CUDNN: &str = "cudnn";
pub const ESTADO: &str = "estado.json";
/// Requisitos (§6.5): `compute_cap` ≥ 7.5 e driver ≥ 580.
pub const COMPUTE_CAP_MIN: (u32, u32) = (7, 5);
pub const DRIVER_MIN: u32 = 580;
/// Conv 3×3 + ReLU de 1,1 kB (`p2/scripts/gerar_autoteste_onnx.py`): exercita CUDA e cuDNN sem modelo baixado.
pub const MODELO_AUTOTESTE: &[u8] = include_bytes!("autoteste_conv.onnx");
pub const FORMA_AUTOTESTE: [usize; 4] = [1, 3, 64, 64];
/// Saída da GPU igual à da CPU até aqui (autoteste).
pub const TOLERANCIA_AUTOTESTE: f32 = 1e-3;
/// O filho que não responde em 2 min é tratado como reprovado (e morto).
const TEMPO_AUTOTESTE: Duration = Duration::from_secs(120);

#[cfg(windows)]
const LIB_NOME: &str = "onnxruntime.dll";
#[cfg(not(windows))]
const LIB_NOME: &str = "libonnxruntime.so.1.31.1";

/// Bibliotecas de um pacote de GPU: a `libonnxruntime` de GPU e as pastas pré-carregadas (CUDA, depois cuDNN).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacoteGpu {
    pub lib: PathBuf,
    pub cuda: PathBuf,
    pub cudnn: PathBuf,
}

impl PacoteGpu {
    /// O pacote no leiaute de `ai/gpu/`; None = falta a lib ou uma das pastas.
    pub fn da_pasta(gpu_dir: &Path) -> Option<Self> {
        let p = Self {
            lib: gpu_dir.join(PASTA_ONNXRUNTIME).join("lib").join(LIB_NOME),
            cuda: gpu_dir.join(PASTA_CUDA),
            cudnn: gpu_dir.join(PASTA_CUDNN),
        };
        (p.lib.is_file() && p.cuda.is_dir() && p.cudnn.is_dir()).then_some(p)
    }
}

/// Placa pelo `nvidia-smi --query-gpu=name,driver_version,compute_cap,memory.total`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Placa {
    pub nome: String,
    pub driver: String,
    pub compute_cap: String,
    pub memoria_mb: u64,
}

/// Resultado do autoteste (no filho, impresso em JSON). `motivo` = código do catálogo (`gpu_*`); `detalhe` = texto
/// do onnxruntime (vai para o log, nunca para a tela).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Autoteste {
    pub pronta: bool,
    pub provedor: Provedor,
    pub motivo: Option<String>,
    pub detalhe: Option<String>,
    pub ms_sessao: f64,
    pub ms_primeira: f64,
    pub ms_segunda: f64,
    pub nos_gpu: u32,
    pub nos_total: u32,
    pub dif_max: Option<f32>,
}

impl Autoteste {
    fn reprovado(motivo: &str, detalhe: impl Into<String>) -> Self {
        Self {
            pronta: false,
            provedor: Provedor::Cpu,
            motivo: Some(motivo.to_string()),
            detalhe: Some(detalhe.into()),
            ms_sessao: 0.0,
            ms_primeira: 0.0,
            ms_segunda: 0.0,
            nos_gpu: 0,
            nos_total: 0,
            dif_max: None,
        }
    }
}

/// `ai/gpu/estado.json`: o que foi instalado e o último autoteste (com a placa e o driver de então).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstadoGpu {
    /// "cuda13"
    pub variante: String,
    /// "1.31.1"
    pub onnxruntime: String,
    pub placa: Option<Placa>,
    pub autoteste: Option<Autoteste>,
    pub testado_em: Option<String>,
}

// ---------------------------------------------------------------------------
// Placa

fn ler_placa(saida: &str) -> Option<Placa> {
    let l = saida.lines().map(str::trim).find(|l| !l.is_empty())?;
    let c: Vec<&str> = l.split(',').map(str::trim).collect();
    if c.len() < 4 || c[1].is_empty() {
        return None;
    }
    Some(Placa {
        nome: c[0].to_string(),
        driver: c[1].to_string(),
        compute_cap: c[2].to_string(),
        memoria_mb: c[3].trim_end_matches("MiB").trim().parse().unwrap_or(0),
    })
}

/// Saída de um programa curto, com teto de tempo (mata só o próprio filho).
fn saida_de(mut cmd: std::process::Command, teto: Duration) -> Option<String> {
    let mut f = cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).spawn().ok()?;
    let mut out = f.stdout.take()?;
    let leitor = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = (&mut out).take(1 << 16).read_to_string(&mut s);
        s
    });
    let t = Instant::now();
    loop {
        match f.try_wait() {
            Ok(Some(st)) => return st.success().then(|| leitor.join().ok()).flatten(),
            Ok(None) if t.elapsed() < teto => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = f.kill();
                let _ = f.wait();
                return None;
            }
        }
    }
}

/// Placa NVIDIA desta máquina; None = sem `nvidia-smi` ou sem placa. Uma consulta por processo.
pub fn placa() -> Option<Placa> {
    static P: OnceLock<Option<Placa>> = OnceLock::new();
    P.get_or_init(|| {
        let exe = which::which("nvidia-smi").ok()?;
        let mut cmd = crate::tools::command(exe);
        cmd.args(["--query-gpu=name,driver_version,compute_cap,memory.total", "--format=csv,noheader,nounits"]);
        ler_placa(&saida_de(cmd, Duration::from_secs(15))?)
    })
    .clone()
}

/// Código do catálogo quando a placa não atende (`gpu_placa_antiga`, `gpu_driver_antigo`); None = atende.
pub fn requisito(p: &Placa) -> Option<&'static str> {
    let mut cc = p.compute_cap.split('.').map(|x| x.trim().parse::<u32>().unwrap_or(0));
    let cap = (cc.next().unwrap_or(0), cc.next().unwrap_or(0));
    if cap < COMPUTE_CAP_MIN {
        return Some(codigo::GPU_PLACA_ANTIGA);
    }
    let driver = p.driver.split('.').next().and_then(|x| x.trim().parse::<u32>().ok()).unwrap_or(0);
    (driver < DRIVER_MIN).then_some(codigo::GPU_DRIVER_ANTIGO)
}

// ---------------------------------------------------------------------------
// Estado e "pronta"

/// Lê `estado.json`; None = ausente ou ilegível.
pub fn estado(gpu_dir: &Path) -> Option<EstadoGpu> {
    serde_json::from_slice(&std::fs::read(gpu_dir.join(ESTADO)).ok()?).ok()
}

pub(crate) fn gravar_estado(gpu_dir: &Path, e: &EstadoGpu) -> Result<()> {
    let b = serde_json::to_vec_pretty(e).map_err(|e| SicroError::Workspace(e.to_string()))?;
    crate::filesystem::atomic_write_bytes(&gpu_dir.join(ESTADO), &b)
}

/// Aprovado com a placa e o driver de agora.
pub(crate) fn pronta_com(e: &EstadoGpu, placa: &Placa) -> bool {
    let aprovado = e.autoteste.as_ref().is_some_and(|a| a.pronta && a.provedor == Provedor::Cuda);
    let mesma = e.placa.as_ref().is_some_and(|p| p.driver == placa.driver && p.nome == placa.nome);
    aprovado && mesma
}

/// O driver (ou a placa) mudou desde o último autoteste.
pub(crate) fn mudou(e: &EstadoGpu, placa: &Placa) -> bool {
    e.placa.as_ref().is_none_or(|p| p.driver != placa.driver || p.nome != placa.nome)
}

/// Como chamar o autoteste: o próprio binário (no `cargo test`, o de teste no teste filho).
pub(crate) fn chamada_do_autoteste() -> Result<(PathBuf, Vec<String>)> {
    let exe = std::env::current_exe().map_err(|e| SicroError::Filesystem(format!("executável: {e}")))?;
    #[cfg(test)]
    let args = testes::args_do_filho();
    #[cfg(not(test))]
    let args = Vec::new();
    Ok((exe, args))
}

/// Roda o autoteste no filho e grava o resultado com a placa de agora.
pub(crate) fn testar_e_gravar(gpu_dir: &Path, placa: Option<Placa>) -> Result<EstadoGpu> {
    let (exe, args) = chamada_do_autoteste()?;
    let a = autoteste_em_processo(&exe, &args, gpu_dir)?;
    let mut e = estado(gpu_dir).unwrap_or(EstadoGpu {
        variante: VARIANTE.into(),
        onnxruntime: sessao::VERSAO_ONNXRUNTIME.into(),
        placa: None,
        autoteste: None,
        testado_em: None,
    });
    e.placa = placa;
    e.autoteste = Some(a);
    e.testado_em = Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    gravar_estado(gpu_dir, &e)?;
    Ok(e)
}

/// Pacote pronto para `criar_sessao`: instalado, autoteste aprovado e o driver de agora igual ao do autoteste.
/// Driver novo: refaz o autoteste no filho antes de responder (§6.5).
pub fn pronta(ai: &AiSettings) -> Option<PacoteGpu> {
    if ai.gpu_dir.is_empty() {
        return None;
    }
    pronta_com_placa(ai, placa().as_ref())
}

/// `pronta` com a placa dada (a tela já a tem).
pub(crate) fn pronta_com_placa(ai: &AiSettings, placa: Option<&Placa>) -> Option<PacoteGpu> {
    let dir = Some(PathBuf::from(&ai.gpu_dir)).filter(|d| !d.as_os_str().is_empty())?;
    let pacote = PacoteGpu::da_pasta(&dir)?;
    let placa = placa?;
    let mut e = estado(&dir)?;
    if mudou(&e, placa) {
        static REFAZ: Mutex<()> = Mutex::new(());
        let _g = REFAZ.lock().unwrap_or_else(|e| e.into_inner());
        e = estado(&dir)?;
        if mudou(&e, placa) {
            tracing::info!(target: "ia", "driver da NVIDIA mudou: refazendo o autoteste");
            e = testar_e_gravar(&dir, Some(placa.clone())).ok()?;
        }
    }
    pronta_com(&e, placa).then_some(pacote)
}

// ---------------------------------------------------------------------------
// Instalação (`gpu-inferencia.md` §7.1: URL, bytes e SHA-256 fixos por sistema)

pub(crate) const VARIANTE: &str = "cuda13";

/// Para onde vai cada arquivo do item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Destino {
    Onnxruntime,
    Cuda,
    Cudnn,
}

pub(crate) struct ArquivoGpu {
    pub nome: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub destino: Destino,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const ARQUIVOS_GPU: Option<&[ArquivoGpu]> = Some(&[
    ArquivoGpu { nome: "onnxruntime-linux-x64-gpu_cuda13-1.31.1.tgz", url: "https://github.com/microsoft/onnxruntime/releases/download/v1.31.1/onnxruntime-linux-x64-gpu_cuda13-1.31.1.tgz", bytes: 244_573_709, sha256: "b2146862e691f7e338f5aff9c7a9a5567c2ca57cf01442a1adb3a13bf62d9095", destino: Destino::Onnxruntime },
    ArquivoGpu { nome: "nvidia_cuda_runtime-13.5.37-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl", url: "https://files.pythonhosted.org/packages/0b/7c/14b8f5518f8951e9dca57ea80f44f8ea8bcc8671812e6cc38009f3205e57/nvidia_cuda_runtime-13.5.37-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.whl", bytes: 2_504_024, sha256: "df2b79a8852f10d1fef8c848489c49eebb5754051bf870202a862831412b179d", destino: Destino::Cuda },
    ArquivoGpu { nome: "nvidia_cublas-13.10.0.20-py3-none-manylinux1_x86_64.manylinux_2_5_x86_64.whl", url: "https://files.pythonhosted.org/packages/b4/12/16628161ed8c109c8c73b38167df50cbc630a8a6e6548e2534d461526c3c/nvidia_cublas-13.10.0.20-py3-none-manylinux1_x86_64.manylinux_2_5_x86_64.whl", bytes: 444_289_903, sha256: "dcc6d63884c53d07e267ce7e6feac17b2538cebaafec5db865736c3e28e4dc84", destino: Destino::Cuda },
    ArquivoGpu { nome: "nvidia_curand-10.4.5.25-py3-none-manylinux_2_27_x86_64.whl", url: "https://files.pythonhosted.org/packages/12/63/636e5dc9afb2959bfc0042a357d5567358df829b66c96d9fc36d77dc0596/nvidia_curand-10.4.5.25-py3-none-manylinux_2_27_x86_64.whl", bytes: 77_113_015, sha256: "22bb226a857e5888fe6b54ae465fc4371296bcff13a35f79e19691bcf454f529", destino: Destino::Cuda },
    ArquivoGpu { nome: "nvidia_cuda_nvrtc-13.5.37-py3-none-manylinux2010_x86_64.manylinux_2_12_x86_64.whl", url: "https://files.pythonhosted.org/packages/a9/54/db4e7f650746265f61c05cf1f8f556dd8c6739b58c823fbc9a7d6fda3247/nvidia_cuda_nvrtc-13.5.37-py3-none-manylinux2010_x86_64.manylinux_2_12_x86_64.whl", bytes: 58_185_930, sha256: "86b4d68058807988b7d9af5b44917a0601e9c337f35ccd657bfb3c759ff8d8bc", destino: Destino::Cuda },
    ArquivoGpu { nome: "nvidia_cudnn_cu13-9.28.1.4-py3-none-manylinux_2_27_x86_64.whl", url: "https://files.pythonhosted.org/packages/ef/6f/94535c450d44a68ef0b43d7a5e54c0e5ecda2ca3db7daec5d54c8d94e0d8/nvidia_cudnn_cu13-9.28.1.4-py3-none-manylinux_2_27_x86_64.whl", bytes: 516_817_937, sha256: "a54f205b1db1f2895d99f398754db1369816f0569dd459731f925635cf7b65ec", destino: Destino::Cudnn },
]);

#[cfg(all(windows, target_arch = "x86_64"))]
const ARQUIVOS_GPU: Option<&[ArquivoGpu]> = Some(&[
    ArquivoGpu { nome: "onnxruntime-win-x64-gpu_cuda13-1.31.1.zip", url: "https://github.com/microsoft/onnxruntime/releases/download/v1.31.1/onnxruntime-win-x64-gpu_cuda13-1.31.1.zip", bytes: 309_101_542, sha256: "6c64508de9c04f9d938297287e9d81fa47a2fbbb30cc04e6fac0db88f6cb68c1", destino: Destino::Onnxruntime },
    ArquivoGpu { nome: "nvidia_cuda_runtime-13.5.37-py3-none-win_amd64.whl", url: "https://files.pythonhosted.org/packages/35/99/e07dd9565d2e0317c1a62ff39ae9fc0542c048f74bcfc777c8be97710347/nvidia_cuda_runtime-13.5.37-py3-none-win_amd64.whl", bytes: 2_780_254, sha256: "f225d3fa5ea6bfbb58fc4979ef72375cbe661d9107f0d8a4ff22cc151525e4a9", destino: Destino::Cuda },
    ArquivoGpu { nome: "nvidia_cublas-13.10.0.20-py3-none-win_amd64.whl", url: "https://files.pythonhosted.org/packages/12/39/e1b1b4adb8b171894d9a89035df0596d448cf73dc403802d24dbe30cc0be/nvidia_cublas-13.10.0.20-py3-none-win_amd64.whl", bytes: 427_284_863, sha256: "c310b749eea855b88fafb906de08d6819b87216c6711640771764fea5dcd8546", destino: Destino::Cuda },
    ArquivoGpu { nome: "nvidia_curand-10.4.5.25-py3-none-win_amd64.whl", url: "https://files.pythonhosted.org/packages/ee/b6/5b38490991b6875ae60c4f41e190ac91cadd1e6a1324b152999cc4a3f580/nvidia_curand-10.4.5.25-py3-none-win_amd64.whl", bytes: 77_330_709, sha256: "f6a02a70f5b9715235df31a164a4bf3237a45be1600a5f435f5e4a0d8682eb22", destino: Destino::Cuda },
    ArquivoGpu { nome: "nvidia_cuda_nvrtc-13.5.37-py3-none-win_amd64.whl", url: "https://files.pythonhosted.org/packages/9f/6c/241c5155199a6de55bad8862213ed64b487dc9526aae817e4af31afea1a4/nvidia_cuda_nvrtc-13.5.37-py3-none-win_amd64.whl", bytes: 49_248_991, sha256: "e8e6c10b251385707ae3abd7b4ee118de6736fd7678d0f0f4eab4d759f679c21", destino: Destino::Cuda },
    ArquivoGpu { nome: "nvidia_cudnn_cu13-9.28.1.4-py3-none-win_amd64.whl", url: "https://files.pythonhosted.org/packages/48/9c/26e6bdd31c97c389aa565b49c916be0dec17dd61dad6cd33fee286d1b946/nvidia_cudnn_cu13-9.28.1.4-py3-none-win_amd64.whl", bytes: 407_065_099, sha256: "73d874c6bd8a1c75ca040228f77c4e163efe9bab3b65bd85914e2b7334ac68ee", destino: Destino::Cudnn },
]);

#[cfg(not(any(all(target_os = "linux", target_arch = "x86_64"), all(windows, target_arch = "x86_64"))))]
const ARQUIVOS_GPU: Option<&[ArquivoGpu]> = None;

/// Os arquivos do item neste sistema; None = sem pacote (fora de Linux e Windows x64).
pub(crate) fn arquivos() -> Option<&'static [ArquivoGpu]> {
    ARQUIVOS_GPU
}

/// Download do item em MB (para a tela).
pub(crate) fn mb() -> u32 {
    arquivos().map_or(0, |a| (a.iter().map(|x| x.bytes).sum::<u64>() / 1_000_000) as u32)
}

/// Instala de `fonte` numa pasta nova ao lado e troca no fim: tudo ou nada. Cada pacote é conferido, extraído e
/// apagado antes do próximo (pico de disco menor).
pub(crate) fn instalar_de(itens: &[ArquivoGpu], fonte: &Fonte, gpu_dir: &Path, cancelar: &AtomicBool, progresso: &mut dyn FnMut(u64, u64)) -> Result<()> {
    let _t = pacote::trava();
    let total: u64 = itens.iter().map(|a| a.bytes).sum();
    let novo = pacote::irma(gpu_dir, "novo");
    let _ = std::fs::remove_dir_all(&novo);
    let r = (|| -> Result<()> {
        let baixa = novo.join("pacotes");
        std::fs::create_dir_all(&baixa).map_err(|e| SicroError::Filesystem(e.to_string()))?;
        let mut feito = 0u64;
        for a in itens {
            let arq = Arquivo { nome: a.nome.into(), url: a.url.into(), bytes: a.bytes, sha256: a.sha256.into() };
            let local = baixa.join(a.nome);
            pacote::obter(&arq, fonte, &local, cancelar, &mut |k| {
                feito += k;
                progresso(feito, total);
            })?;
            match a.destino {
                Destino::Onnxruntime => {
                    let n = pacote::extrair_lib_onnxruntime(&local, &novo.join(PASTA_ONNXRUNTIME).join("lib"))?;
                    if !n.iter().any(|x| x == LIB_NOME) {
                        return Err(SicroError::Validation(format!("{LIB_NOME} não está no pacote")));
                    }
                }
                Destino::Cuda => drop(pacote::extrair_roda(&local, &novo.join(PASTA_CUDA))?),
                Destino::Cudnn => drop(pacote::extrair_roda(&local, &novo.join(PASTA_CUDNN))?),
            }
            let _ = std::fs::remove_file(&local);
        }
        let _ = std::fs::remove_dir_all(&baixa);
        if PacoteGpu::da_pasta(&novo).is_none() {
            return Err(SicroError::Validation(codigo::GPU_SEM_BIBLIOTECAS.into()));
        }
        gravar_estado(&novo, &EstadoGpu { variante: VARIANTE.into(), onnxruntime: sessao::VERSAO_ONNXRUNTIME.into(), placa: None, autoteste: None, testado_em: None })
    })();
    if let Err(e) = r {
        let _ = std::fs::remove_dir_all(&novo);
        return Err(e);
    }
    pacote::trocar(&novo, gpu_dir)
}

/// Baixa e extrai o item (URLs, bytes e SHA-256 fixos por sistema; `gpu-inferencia.md` §7.1) em `gpu_dir`.
/// `progresso(recebidos, total)`. Não roda o autoteste.
pub fn instalar(gpu_dir: &Path, cancelar: &AtomicBool, progresso: &mut dyn FnMut(u64, u64)) -> Result<()> {
    let itens = arquivos().ok_or_else(|| SicroError::Validation(format!("{}: gpu", codigo::NAO_SUPORTADO)))?;
    instalar_de(itens, &Fonte::Rede, gpu_dir, cancelar, progresso)
}

pub fn remover(gpu_dir: &Path) -> Result<()> {
    if !gpu_dir.exists() {
        return Ok(());
    }
    // a lib deste processo pode estar nela: no Windows não sai (reabrir)
    std::fs::remove_dir_all(gpu_dir).map_err(|e| SicroError::Validation(format!("{}: {e}", codigo::REABRIR)))
}

// ---------------------------------------------------------------------------
// Autoteste

/// Entrada fixa (congruencial): a mesma em qualquer máquina.
pub(crate) fn entrada_de_teste(n: usize) -> Vec<f32> {
    let mut x = 0x2545_F491u32;
    (0..n)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 8) as f32 / (1u32 << 24) as f32
        })
        .collect()
}

/// Nós por provedor no perfil do onnxruntime (`"provider":"…"` de cada nó executado).
pub(crate) fn nos_por_provedor(perfil: &str) -> Vec<(String, u32)> {
    let mut cont: Vec<(String, u32)> = Vec::new();
    let chave = "\"provider\"";
    let mut resto = perfil;
    while let Some(i) = resto.find(chave) {
        resto = &resto[i + chave.len()..];
        let Some(a) = resto.find('"') else { break };
        let depois = &resto[a + 1..];
        let Some(b) = depois.find('"') else { break };
        let nome = &depois[..b];
        match cont.iter_mut().find(|(n, _)| n == nome) {
            Some(c) => c.1 += 1,
            None => cont.push((nome.to_string(), 1)),
        }
        resto = &depois[b..];
    }
    cont
}

/// O autoteste neste processo (é o corpo do filho): CPU × GPU com o modelo embutido. Pronta = sessão no CUDA, duas
/// inferências sem erro, maioria dos nós na GPU (perfil) e saída igual à da CPU até `TOLERANCIA_AUTOTESTE`.
pub fn autoteste_aqui(pacote: &PacoteGpu) -> Autoteste {
    let n: usize = FORMA_AUTOTESTE.iter().product();
    let dados = entrada_de_teste(n);
    let referencia = match super::criar_sessao_com(&pacote.lib, None, Modelo::Bytes(MODELO_AUTOTESTE), &Opcoes::so_cpu())
        .map_err(|e| e.to_string())
        .and_then(|mut s| sessao::rodar(&mut s.session, FORMA_AUTOTESTE, dados.clone()))
    {
        Ok(v) => v,
        Err(m) => return Autoteste::reprovado(codigo::GPU_FALHOU, format!("CPU: {m}")),
    };
    let pasta = std::env::temp_dir().join(format!("sicro-autoteste-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&pasta);
    let r = autoteste_gpu(pacote, &dados, &referencia, &pasta.join("perfil"));
    let _ = std::fs::remove_dir_all(&pasta);
    r
}

fn autoteste_gpu(pacote: &PacoteGpu, dados: &[f32], referencia: &[f32], perfil: &Path) -> Autoteste {
    let op = Opcoes { preferencia: Preferencia::Gpu, aquecer: None, ..Opcoes::default() };
    let t = Instant::now();
    let mut s = match sessao::sessao_gpu(&pacote.lib, Some(pacote), Modelo::Bytes(MODELO_AUTOTESTE), &op, Some(perfil)) {
        Ok(s) => s,
        Err(q) => return Autoteste::reprovado(q.codigo, q.detalhe),
    };
    let mut r = Autoteste::reprovado(codigo::GPU_FALHOU, "");
    r.ms_sessao = t.elapsed().as_secs_f64() * 1e3;
    let mut saida = Vec::new();
    for i in 0..2 {
        let t = Instant::now();
        match sessao::rodar(&mut s, FORMA_AUTOTESTE, dados.to_vec()) {
            Ok(v) => saida = v,
            Err(m) => {
                let q = sessao::Queda::de(&m);
                r.motivo = Some(q.codigo.into());
                r.detalhe = Some(q.detalhe);
                return r;
            }
        }
        let ms = t.elapsed().as_secs_f64() * 1e3;
        if i == 0 {
            r.ms_primeira = ms;
        } else {
            r.ms_segunda = ms;
        }
    }
    match s.end_profiling() {
        Ok(arq) => {
            if let Ok(txt) = std::fs::read_to_string(&arq) {
                for (nome, c) in nos_por_provedor(&txt) {
                    r.nos_total += c;
                    if nome != "CPUExecutionProvider" {
                        r.nos_gpu += c;
                    }
                }
            }
            let _ = std::fs::remove_file(&arq);
        }
        Err(e) => r.detalhe = Some(format!("perfil: {e}")),
    }
    let dif = if saida.len() == referencia.len() {
        saida.iter().zip(referencia).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max)
    } else {
        f32::INFINITY
    };
    r.dif_max = Some(dif);
    if r.nos_total == 0 || r.nos_gpu * 2 <= r.nos_total {
        r.detalhe = Some(format!("só {} de {} nós na GPU", r.nos_gpu, r.nos_total));
    } else if !(dif <= TOLERANCIA_AUTOTESTE) {
        r.detalhe = Some(format!("saída da GPU difere da CPU em {dif}"));
    } else {
        r.pronta = true;
        r.provedor = Provedor::Cuda;
        r.motivo = None;
        r.detalhe = None;
    }
    r
}

/// Roda `exe <args…> --autoteste-gpu <gpu_dir>` e lê o JSON. Em produção `exe` = `current_exe()`; nos testes, o
/// binário de teste com o filtro do teste filho (`ia::testes`). Falha de driver que derruba o filho = não pronta.
pub fn autoteste_em_processo(exe: &Path, args: &[String], gpu_dir: &Path) -> Result<Autoteste> {
    let (status, out, err) = rodar_filho(exe, args, gpu_dir, &[])?;
    Ok(ler_autoteste(status, &out, &err))
}

/// O filho com as variáveis a mais (testes): status (None = morto por tempo), stdout e stderr (até 1 MiB cada).
fn rodar_filho(exe: &Path, args: &[String], gpu_dir: &Path, env: &[(&str, &str)]) -> Result<(Option<std::process::ExitStatus>, String, String)> {
    let mut cmd = crate::tools::command(exe);
    cmd.args(args)
        .arg(ARG_AUTOTESTE)
        .arg(gpu_dir)
        .env("ORT_DISABLE_TELEMETRY", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut f = cmd.spawn().map_err(|e| SicroError::Filesystem(format!("autoteste: {e}")))?;
    // teto de memória: 1 MiB de cada saída; o resto é descartado (o filho não trava no cano cheio)
    let ler = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = (&mut r).take(1 << 20).read_to_end(&mut b);
            let _ = std::io::copy(&mut r, &mut std::io::sink());
            String::from_utf8_lossy(&b).into_owned()
        })
    };
    let out = ler(Box::new(f.stdout.take().expect("stdout")));
    let err = ler(Box::new(f.stderr.take().expect("stderr")));
    let t = Instant::now();
    let status = loop {
        match f.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) if t.elapsed() < TEMPO_AUTOTESTE => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = f.kill();
                let _ = f.wait();
                break None;
            }
        }
    };
    Ok((status, out.join().unwrap_or_default(), err.join().unwrap_or_default()))
}

/// A última linha do stdout que é um `Autoteste`; sem ela (filho caído, morto por tempo) = reprovado, `gpu_falhou`.
fn ler_autoteste(status: Option<std::process::ExitStatus>, out: &str, err: &str) -> Autoteste {
    let json = |l: &str| l.find('{').and_then(|i| serde_json::from_str::<Autoteste>(l[i..].trim()).ok());
    match out.lines().rev().find_map(json) {
        Some(a) => {
            if let Some(d) = &a.detalhe {
                tracing::warn!(target: "ia", "autoteste da GPU: {} {d}", a.motivo.as_deref().unwrap_or(""));
            }
            a
        }
        None => {
            let fim: String = err.chars().rev().take(2000).collect::<Vec<_>>().into_iter().rev().collect();
            let como = match status {
                Some(s) => format!("{s}"),
                None => format!("sem resposta em {} s", TEMPO_AUTOTESTE.as_secs()),
            };
            tracing::warn!(target: "ia", "autoteste da GPU sem resultado ({como}): {fim}");
            Autoteste::reprovado(codigo::GPU_FALHOU, format!("o processo do autoteste terminou sem resultado ({como}): {fim}"))
        }
    }
}

/// Entrada do `main.rs`: Some(código de saída) quando os argumentos pedem o autoteste (imprime o JSON); None = não.
pub fn principal_do_autoteste(args: &[String]) -> Option<i32> {
    let i = args.iter().position(|a| a == ARG_AUTOTESTE)?;
    let r = match args.get(i + 1).map(PathBuf::from) {
        Some(d) => match PacoteGpu::da_pasta(&d) {
            Some(p) => autoteste_aqui(&p),
            None => Autoteste::reprovado(codigo::GPU_NAO_INSTALADA, format!("sem o pacote em {}", d.display())),
        },
        None => Autoteste::reprovado(codigo::GPU_NAO_INSTALADA, "falta a pasta"),
    };
    let mut so = std::io::stdout().lock();
    let _ = writeln!(so, "{}", serde_json::to_string(&r).unwrap_or_default());
    let _ = so.flush();
    Some(if r.pronta { 0 } else { 3 })
}

/// Erro do onnxruntime/CUDA → código do catálogo (`gpu_sem_bibliotecas`, `gpu_sem_cudnn`, `gpu_sem_placa`,
/// `gpu_driver_antigo`, `gpu_sem_memoria`, `gpu_nao_instalada`, `gpu_falhou`); molde em `explicar_queda`.
pub fn motivo(erro: &str) -> &'static str {
    let e = erro.to_lowercase();
    let tem = |s: &str| e.contains(s);
    if tem("no cuda-capable device") || tem("cuda failure 100") || tem("cudaerrornodevice") {
        codigo::GPU_SEM_PLACA
    } else if (tem("insufficient") && tem("driver")) || tem("cuda failure 35") || tem("cudaerrorinsufficientdriver") {
        codigo::GPU_DRIVER_ANTIGO
    } else if tem("out of memory") || tem("cuda failure 2:") || tem("available memory of") || tem("failed to allocate memory") {
        codigo::GPU_SEM_MEMORIA
    } else if tem("cudnn") {
        // antes da regra das bibliotecas: a falta do cuDNN também traz "cannot open shared object file"
        codigo::GPU_SEM_CUDNN
    } else if tem("failed to load library") || tem("cannot open shared object file") || tem("não está em") || tem("error 126") {
        codigo::GPU_SEM_BIBLIOTECAS
    } else if tem("biblioteca do onnxruntime") && tem("cpu") {
        codigo::GPU_NAO_INSTALADA
    } else {
        codigo::GPU_FALHOU
    }
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;
    use crate::ia::testes as t;

    /// O teste filho: o binário de teste chamado como `<filtro> -- --autoteste-gpu <pasta>`.
    pub(crate) const FILHO: &str = "ia::gpu::testes::filho_do_autoteste";

    pub(crate) fn args_do_filho() -> Vec<String> {
        ["--exact", FILHO, "--nocapture", "--test-threads", "1", "--"].map(String::from).to_vec()
    }

    /// No `cargo test` comum não faz nada; chamado como filho, roda o autoteste (e, com `SICRO_IA_AUTO=1`, a queda
    /// da sessão `Auto` com e sem aquecimento) e sai.
    #[test]
    fn filho_do_autoteste() {
        let args: Vec<String> = std::env::args().collect();
        if !args.iter().any(|a| a == ARG_AUTOTESTE) {
            return;
        }
        if std::env::var("SICRO_IA_CENARIO").as_deref() == Ok("derruba") {
            std::process::abort();
        }
        let codigo = principal_do_autoteste(&args).unwrap();
        if std::env::var("SICRO_IA_AUTO").as_deref() == Ok("1") {
            let i = args.iter().position(|a| a == ARG_AUTOTESTE).unwrap();
            if let Some(p) = PacoteGpu::da_pasta(Path::new(&args[i + 1])) {
                println!("AUTO {}", serde_json::to_string(&auto_no_filho(&p)).unwrap());
            }
        }
        let _ = std::io::stdout().flush();
        std::process::exit(codigo);
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub(crate) struct ComAquecimento {
        pub provedor: Provedor,
        pub queda: Option<String>,
        pub dif: Option<f32>,
    }
    #[derive(Debug, Serialize, Deserialize)]
    pub(crate) struct SemAquecimento {
        pub provedor: Provedor,
        pub erro: Option<String>,
    }
    #[derive(Debug, Serialize, Deserialize)]
    pub(crate) struct Auto {
        pub com_aquecimento: ComAquecimento,
        pub sem_aquecimento: SemAquecimento,
    }

    /// No filho: sessão `Auto` do modelo do autoteste com e sem aquecimento; saída comparada à da CPU.
    fn auto_no_filho(p: &PacoteGpu) -> Auto {
        let n: usize = FORMA_AUTOTESTE.iter().product();
        let dados = entrada_de_teste(n);
        let cpu = crate::ia::criar_sessao_com(&p.lib, None, Modelo::Bytes(MODELO_AUTOTESTE), &Opcoes::so_cpu())
            .ok()
            .and_then(|mut s| sessao::rodar(&mut s.session, FORMA_AUTOTESTE, dados.clone()).ok());
        let com = match crate::ia::criar_sessao_com(&p.lib, Some(p), Modelo::Bytes(MODELO_AUTOTESTE), &Opcoes { aquecer: Some(FORMA_AUTOTESTE), ..Opcoes::default() }) {
            Ok(mut s) => {
                let v = sessao::rodar(&mut s.session, FORMA_AUTOTESTE, dados.clone()).ok();
                let dif = v.zip(cpu.clone()).map(|(a, b)| a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max));
                ComAquecimento { provedor: s.provedor, queda: s.queda, dif }
            }
            Err(e) => ComAquecimento { provedor: Provedor::Cpu, queda: Some(e.to_string()), dif: None },
        };
        let sem = match crate::ia::criar_sessao_com(&p.lib, Some(p), Modelo::Bytes(MODELO_AUTOTESTE), &Opcoes::default()) {
            Ok(mut s) => {
                let erro = sessao::rodar(&mut s.session, FORMA_AUTOTESTE, dados).err().map(|m| motivo(&m).to_string());
                SemAquecimento { provedor: s.provedor, erro }
            }
            Err(e) => SemAquecimento { provedor: Provedor::Cpu, erro: Some(e.to_string()) },
        };
        Auto { com_aquecimento: com, sem_aquecimento: sem }
    }

    /// O que o filho devolveu: o autoteste (pelo caminho de produção) e a linha `AUTO`.
    #[derive(Debug)]
    pub(crate) struct Cenario {
        pub autoteste: Autoteste,
        pub auto: Option<Auto>,
    }

    fn ligar(de: &Path, para: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(de, para).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(de, para).unwrap();
    }

    /// Pasta `ai/gpu` de teste, montada com atalhos para as libs do dossiê, alterada pelo cenário, e as variáveis
    /// só do filho. None = dispensa.
    pub(crate) fn pasta_gpu(cenario: &str, teste: &str) -> Option<(PathBuf, Vec<(&'static str, &'static str)>)> {
        let p = t::ou_dispensa(t::pacote_gpu(), teste, "a lib e as bibliotecas da NVIDIA do dossiê")?;
        let d = std::env::temp_dir().join(format!("p21-gpu-{cenario}-{}-{}", std::process::id(), teste));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join(PASTA_ONNXRUNTIME)).unwrap();
        let lib_dir = p.lib.parent().unwrap();
        let mut env = Vec::new();
        match cenario {
            "lib_de_cpu" => {
                let cpu = t::ou_dispensa(t::ort_cpu_lib(), teste, "a lib de CPU (SICRO_ORT_CPU_LIB)")?;
                std::fs::create_dir_all(d.join(PASTA_ONNXRUNTIME).join("lib")).unwrap();
                ligar(&cpu, &d.join(PASTA_ONNXRUNTIME).join("lib").join(LIB_NOME));
            }
            "sem_provedor" => {
                // a pré-carga passa e o provedor CUDA não carrega no registro (o 17b): sem `error_on_failure`
                // a sessão ficaria na CPU dizendo CUDA
                let l = d.join(PASTA_ONNXRUNTIME).join("lib");
                std::fs::create_dir_all(&l).unwrap();
                for n in [LIB_NOME, "libonnxruntime_providers_shared.so"] {
                    ligar(&lib_dir.join(n), &l.join(n));
                }
            }
            _ => ligar(lib_dir, &d.join(PASTA_ONNXRUNTIME).join("lib")),
        }
        match cenario {
            "sem_bibliotecas" => std::fs::create_dir_all(d.join(PASTA_CUDA)).unwrap(),
            _ => ligar(&p.cuda, &d.join(PASTA_CUDA)),
        }
        match cenario {
            "sem_cudnn" => std::fs::create_dir_all(d.join(PASTA_CUDNN)).unwrap(),
            "cudnn_na_inferencia" => {
                // passa na pré-carga (é o cudart com outro nome) e o provedor não acha o cuDNN na 1ª convolução
                std::fs::create_dir_all(d.join(PASTA_CUDNN)).unwrap();
                ligar(&p.cuda.join("libcudart.so.13"), &d.join(PASTA_CUDNN).join("libcudnn.so.9"));
            }
            _ => ligar(&p.cudnn, &d.join(PASTA_CUDNN)),
        }
        if cenario == "sem_placa" {
            env.push(("CUDA_VISIBLE_DEVICES", ""));
        }
        if cenario == "derruba" {
            env.push(("SICRO_IA_CENARIO", "derruba"));
        }
        Some((d, env))
    }

    /// Roda o cenário num processo filho, lido pelo caminho de produção (`rodar_filho` + `ler_autoteste`, o par de
    /// `autoteste_em_processo`), com `SICRO_IA_AUTO=1`. None = dispensa (sem libs, ou sem placa NVIDIA).
    pub(crate) fn cenario(cenario: &str, teste: &str) -> Option<Cenario> {
        let (d, mut env) = pasta_gpu(cenario, teste)?;
        if cenario != "lib_de_cpu" {
            t::ou_dispensa_gpu(placa(), teste, "placa NVIDIA")?;
        }
        let _trava = t::trava_gpu();
        let (exe, args) = chamada_do_autoteste().unwrap();
        env.push(("SICRO_IA_AUTO", "1"));
        let (status, out, err) = rodar_filho(&exe, &args, &d, &env).unwrap();
        let autoteste = ler_autoteste(status, &out, &err);
        let auto = out.lines().find_map(|l| l.find("AUTO ").map(|i| &l[i + 5..])).map(|j| serde_json::from_str::<Auto>(j).unwrap());
        let _ = std::fs::remove_dir_all(&d);
        eprintln!("cenário {cenario}: {autoteste:?} | {auto:?} | {status:?}");
        Some(Cenario { autoteste, auto })
    }

    pub(crate) fn yolox() -> Option<PathBuf> {
        let p = Path::new(t::DOSSIE).join("p2/modelos/yolox_s.onnx");
        p.is_file().then_some(p)
    }

    /// `p2/modelos/quadro_640x640.rgb` em BGR 0..255, CHW (o preparo do `yolox.rs`).
    pub(crate) fn quadro_yolox() -> Vec<f32> {
        let rgb = std::fs::read(Path::new(t::DOSSIE).join("p2/modelos/quadro_640x640.rgb")).expect("quadro_640x640.rgb");
        crate::video::pessoas::yolox::letterbox_bgr_chw(&rgb, 640, 640, 640)
    }

    #[test]
    fn autoteste_passa_em_gpu() {
        let Some(r) = cenario("ok", "autoteste_passa_em_gpu") else { return };
        let a = &r.autoteste;
        assert!(a.pronta, "{a:?}");
        assert_eq!(a.provedor, Provedor::Cuda);
        assert_eq!((a.motivo.as_deref(), a.detalhe.as_deref()), (None, None));
        assert!(a.nos_total > 0 && a.nos_gpu * 2 > a.nos_total, "{a:?}");
        assert!(a.dif_max.is_some_and(|d| d <= TOLERANCIA_AUTOTESTE), "{a:?}");
        let auto = r.auto.expect("AUTO");
        assert_eq!((auto.com_aquecimento.provedor, auto.com_aquecimento.queda.as_deref()), (Provedor::Cuda, None));
        assert!(auto.com_aquecimento.dif.is_some_and(|d| d <= TOLERANCIA_AUTOTESTE));
    }

    fn cai_para_cpu(cen: &str, teste: &str, esperado: &str) {
        let Some(r) = cenario(cen, teste) else { return };
        let a = &r.autoteste;
        assert!(!a.pronta, "{a:?}");
        assert_eq!(a.provedor, Provedor::Cpu);
        assert_eq!(a.motivo.as_deref(), Some(esperado), "{a:?}");
        assert!(a.detalhe.as_deref().is_some_and(|d| !d.is_empty()));
        let auto = r.auto.expect("AUTO");
        // a sessão Auto cai para a CPU com o motivo e dá a saída exata da CPU
        assert_eq!(auto.com_aquecimento.provedor, Provedor::Cpu, "{auto:?}");
        assert_eq!(auto.com_aquecimento.queda.as_deref(), Some(esperado), "{auto:?}");
        assert_eq!(auto.com_aquecimento.dif, Some(0.0), "{auto:?}");
    }

    #[test]
    fn cai_para_cpu_sem_cudnn() {
        cai_para_cpu("sem_cudnn", "cai_para_cpu_sem_cudnn", codigo::GPU_SEM_CUDNN);
        cai_para_cpu("cudnn_na_inferencia", "cai_para_cpu_sem_cudnn", codigo::GPU_SEM_CUDNN);
    }

    #[test]
    fn cai_para_cpu_sem_bibliotecas() {
        cai_para_cpu("sem_bibliotecas", "cai_para_cpu_sem_bibliotecas", codigo::GPU_SEM_BIBLIOTECAS);
        cai_para_cpu("sem_provedor", "cai_para_cpu_sem_bibliotecas", codigo::GPU_SEM_BIBLIOTECAS);
    }

    #[test]
    fn cai_para_cpu_sem_placa() {
        cai_para_cpu("sem_placa", "cai_para_cpu_sem_placa", codigo::GPU_SEM_PLACA);
    }

    #[test]
    fn lib_de_cpu_nao_fica_pronta() {
        cai_para_cpu("lib_de_cpu", "lib_de_cpu_nao_fica_pronta", codigo::GPU_NAO_INSTALADA);
    }

    /// Filho que cai (falha de driver derruba o processo sem erro capturável) = não pronta, `gpu_falhou`.
    #[test]
    fn filho_que_cai_nao_fica_pronta() {
        let Some((d, env)) = pasta_gpu("derruba", "filho_que_cai_nao_fica_pronta") else { return };
        let (exe, args) = chamada_do_autoteste().unwrap();
        let (status, out, err) = rodar_filho(&exe, &args, &d, &env).unwrap();
        let a = ler_autoteste(status, &out, &err);
        let _ = std::fs::remove_dir_all(&d);
        assert!(!a.pronta, "{a:?}");
        assert_eq!(a.motivo.as_deref(), Some(codigo::GPU_FALHOU));
        assert!(a.detalhe.as_deref().unwrap_or("").contains("sem resultado"), "{a:?}");
    }

    /// Os textos que o onnxruntime 1.31.1 devolveu nos cenários do `p2/out` (copiados aqui e, com o dossiê, lidos
    /// dos arquivos) caem no código certo; o que não se reconhece é `gpu_falhou`.
    #[test]
    fn motivo_dos_textos_do_onnxruntime() {
        let casos = [
            ("Failed to load library /x/libonnxruntime_providers_cuda.so with error: libcublasLt.so.12: cannot open shared object file: No such file or directory", codigo::GPU_SEM_BIBLIOTECAS),
            ("cuda: libcudart.so.* não está em /x/cuda", codigo::GPU_SEM_BIBLIOTECAS),
            ("Parse Failed to parse provider option \"device_id\": CUDA failure 100: no CUDA-capable device is detected ; GPU=-1", codigo::GPU_SEM_PLACA),
            ("NOT_IMPLEMENTED : cuDNN is unavailable or disabled for CUDA Execution Provider: dlopen failed for libcudnn.so: libcudnn.so: cannot open shared object file", codigo::GPU_SEM_CUDNN),
            ("cudnn: libcudnn.so.* não está em /x/cudnn", codigo::GPU_SEM_CUDNN),
            ("CUDNN_STATUS_SUBLIBRARY_LOADING_FAILED", codigo::GPU_SEM_CUDNN),
            ("bfc_arena.cc:360 Available memory of 968960 is smaller than requested bytes of 18036480", codigo::GPU_SEM_MEMORIA),
            ("CUDA failure 35: CUDA driver version is insufficient for CUDA runtime version", codigo::GPU_DRIVER_ANTIGO),
            ("a biblioteca do onnxruntime é a de CPU", codigo::GPU_NAO_INSTALADA),
            ("qualquer outra coisa", codigo::GPU_FALHOU),
        ];
        for (txt, cod) in casos {
            assert_eq!(motivo(txt), cod, "{txt}");
        }
        // as saídas medidas do protótipo
        let out = Path::new(t::DOSSIE).join("p2/out");
        let Some(_) = t::ou_dispensa(out.is_dir().then_some(()), "motivo_dos_textos_do_onnxruntime", "p2/out") else { return };
        let ler = |n: &str| std::fs::read_to_string(out.join(n)).unwrap();
        let motivo_da_linha = |txt: &str, chave: &str| -> &'static str {
            let l = txt.lines().find(|l| l.contains(chave)).unwrap_or_else(|| panic!("{chave}"));
            motivo(l)
        };
        assert_eq!(motivo_da_linha(&ler("17-autoteste-embutido-sem-cudnn.txt"), "motivo="), codigo::GPU_SEM_CUDNN);
        assert_eq!(motivo_da_linha(&ler("17-autoteste-sem-cudnn.txt"), "motivo="), codigo::GPU_SEM_CUDNN);
        assert_eq!(motivo_da_linha(&ler("17b-autoteste-embutido-cuda12-sem-libs.txt"), "motivo="), codigo::GPU_SEM_BIBLIOTECAS);
        assert_eq!(motivo_da_linha(&ler("17c-autoteste-embutido-lib-cpu.txt"), "motivo="), codigo::GPU_NAO_INSTALADA);
        for (arq, chave, cod) in [
            ("11-auto-cuda12-sem-libs.txt", "Failed to load library", codigo::GPU_SEM_BIBLIOTECAS),
            ("12-auto-sem-placa.txt", "CUDA failure 100", codigo::GPU_SEM_PLACA),
            ("19-auto-vram-64.txt", "Available memory of", codigo::GPU_SEM_MEMORIA),
            ("15-gpu-cuda12-sem-runtime-compiled.txt", "SUBLIBRARY_LOADING_FAILED", codigo::GPU_SEM_CUDNN),
        ] {
            assert_eq!(motivo_da_linha(&ler(arq), chave), cod, "{arq}");
        }
    }

    fn placa_de(driver: &str) -> Placa {
        Placa { nome: "NVIDIA GeForce RTX 4060 Ti".into(), driver: driver.into(), compute_cap: "8.9".into(), memoria_mb: 16380 }
    }

    #[test]
    fn placa_e_requisito() {
        let p = ler_placa("NVIDIA GeForce RTX 4060 Ti, 610.57.04, 8.9, 16380\n").unwrap();
        assert_eq!(p, placa_de("610.57.04"));
        assert_eq!(ler_placa("NVIDIA A, 610.57.04, 8.9, 8188 MiB\nNVIDIA B, 610.57.04, 7.5, 4096").unwrap().memoria_mb, 8188);
        assert_eq!(ler_placa(""), None);
        assert_eq!(requisito(&p), None);
        assert_eq!(requisito(&Placa { compute_cap: "6.1".into(), ..p.clone() }), Some(codigo::GPU_PLACA_ANTIGA));
        assert_eq!(requisito(&Placa { compute_cap: "7.5".into(), ..p.clone() }), None);
        assert_eq!(requisito(&Placa { driver: "579.99".into(), ..p.clone() }), Some(codigo::GPU_DRIVER_ANTIGO));
        assert_eq!(requisito(&Placa { driver: "580.00".into(), ..p.clone() }), None);
    }

    /// "Pronta" exige o driver do autoteste; driver novo refaz o autoteste no filho e grava a placa de agora.
    #[test]
    fn driver_novo_refaz() {
        let aprovado = Autoteste { pronta: true, provedor: Provedor::Cuda, motivo: None, detalhe: None, ms_sessao: 1.0, ms_primeira: 1.0, ms_segunda: 1.0, nos_gpu: 4, nos_total: 4, dif_max: Some(0.0) };
        let e = EstadoGpu { variante: "cuda13".into(), onnxruntime: "1.31.1".into(), placa: Some(placa_de("610.57.04")), autoteste: Some(aprovado.clone()), testado_em: None };
        assert!(pronta_com(&e, &placa_de("610.57.04")));
        assert!(!pronta_com(&e, &placa_de("611.20")));
        assert!(mudou(&e, &placa_de("611.20")) && !mudou(&e, &placa_de("610.57.04")));
        assert!(!pronta_com(&EstadoGpu { autoteste: Some(Autoteste { pronta: false, ..aprovado.clone() }), ..e.clone() }, &placa_de("610.57.04")));

        // de verdade: estado com outro driver → `pronta` refaz no filho e grava o driver de agora
        let Some((d, _)) = pasta_gpu("ok", "driver_novo_refaz") else { return };
        let Some(agora) = t::ou_dispensa_gpu(placa(), "driver_novo_refaz", "placa NVIDIA") else { return };
        let velho = Placa { driver: "1.0".into(), ..agora.clone() };
        gravar_estado(&d, &EstadoGpu { placa: Some(velho), ..e }).unwrap();
        let ai = AiSettings { gpu_dir: d.to_string_lossy().into_owned(), ..Default::default() };
        let p = {
            let _trava = t::trava_gpu();
            pronta(&ai)
        };
        let novo = estado(&d).unwrap();
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(novo.placa.as_ref().map(|p| p.driver.as_str()), Some(agora.driver.as_str()));
        assert!(novo.autoteste.as_ref().is_some_and(|a| a.pronta && a.ms_sessao > 0.0), "{novo:?}");
        assert!(novo.testado_em.is_some());
        assert_eq!(p.map(|p| p.lib.file_name().unwrap().to_owned()), Some(LIB_NOME.into()));
    }

    #[test]
    fn principal_so_responde_ao_argumento() {
        assert_eq!(principal_do_autoteste(&["sicro".into()]), None);
        assert_eq!(principal_do_autoteste(&["sicro".into(), "--outro".into()]), None);
        assert_eq!(nos_por_provedor(r#"[{"args":{"provider":"CUDAExecutionProvider"}},{"args":{"provider" : "CPUExecutionProvider"}},{"args":{"provider":"CUDAExecutionProvider"}}]"#), vec![("CUDAExecutionProvider".into(), 2), ("CPUExecutionProvider".into(), 1)]);
    }

    /// Instala a aceleração a partir dos pacotes já baixados no dossiê (os mesmos bytes e SHA-256 da tabela fixa,
    /// sem rede): a pasta sai no leiaute de `ai/gpu`, com as libs iguais às extraídas no protótipo, e o autoteste
    /// no filho a aprova.
    #[test]
    fn instala_dos_pacotes_baixados_e_aprova() {
        let Some(itens) = arquivos() else { return };
        let dl = Path::new(t::DOSSIE).join("p2/dl");
        let tem = itens.iter().all(|a| dl.join(a.nome).is_file() || dl.join("wheels").join(a.nome).is_file());
        if t::ou_dispensa(tem.then_some(()), "instala_dos_pacotes_baixados_e_aprova", "os pacotes em p2/dl").is_none() {
            return;
        }
        let d = std::env::temp_dir().join(format!("p21-instala-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let gpu_dir = d.join("gpu");
        let mut ultimo = (0, 0);
        let nao = AtomicBool::new(false);
        let t0 = Instant::now();
        instalar_de(itens, &Fonte::Pastas(vec![dl.clone(), dl.join("wheels")]), &gpu_dir, &nao, &mut |r, tot| ultimo = (r, tot)).unwrap();
        eprintln!("instalação a partir de p2/dl: {:.1} s", t0.elapsed().as_secs_f64());
        let total: u64 = itens.iter().map(|a| a.bytes).sum();
        assert_eq!(ultimo, (total, total));
        assert_eq!(mb(), (total / 1_000_000) as u32);
        let p = PacoteGpu::da_pasta(&gpu_dir).expect("leiaute de ai/gpu");
        let nomes = |sub: &str| {
            let mut v: Vec<String> = std::fs::read_dir(gpu_dir.join(sub)).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            v.sort();
            v
        };
        assert_eq!(nomes("onnxruntime/lib"), ["libonnxruntime.so.1.31.1", "libonnxruntime_providers_cuda.so", "libonnxruntime_providers_shared.so"]);
        for n in ["libcudart.so.13", "libcublas.so.13", "libcublasLt.so.13", "libcurand.so.10", "libnvrtc.so.13", "LICENSE-nvidia_cublas.txt"] {
            assert!(nomes("cuda").iter().any(|x| x == n), "{n}: {:?}", nomes("cuda"));
        }
        assert_eq!(nomes("cudnn").iter().filter(|n| n.starts_with("libcudnn")).count(), 10);
        assert!(nomes("cuda").iter().all(|n| !n.contains("nvblas")));
        assert!(!pacote::irma(&gpu_dir, "novo").exists());
        // bytes iguais aos que o protótipo extraiu e mediu
        let igual = |a: &Path, b: &Path| std::fs::read(a).unwrap() == std::fs::read(b).unwrap();
        let pg = t::pacote_gpu().unwrap();
        assert!(igual(&p.lib, &pg.lib));
        assert!(igual(&p.cudnn.join("libcudnn_engines_runtime_compiled.so.9"), &pg.cudnn.join("libcudnn_engines_runtime_compiled.so.9")));
        assert!(igual(&p.cuda.join("libcublasLt.so.13"), &pg.cuda.join("libcublasLt.so.13")));
        let e = estado(&gpu_dir).unwrap();
        assert_eq!((e.variante.as_str(), e.onnxruntime.as_str(), e.autoteste.is_none()), ("cuda13", "1.31.1", true));
        // a pasta instalada passa no autoteste do filho
        if let Some(agora) = t::ou_dispensa_gpu(placa(), "instala_dos_pacotes_baixados_e_aprova", "placa NVIDIA") {
            let _trava = t::trava_gpu();
            let e = testar_e_gravar(&gpu_dir, Some(agora.clone())).unwrap();
            assert!(e.autoteste.as_ref().is_some_and(|a| a.pronta), "{e:?}");
            let ai = AiSettings { gpu_dir: gpu_dir.to_string_lossy().into_owned(), ..Default::default() };
            assert_eq!(pronta(&ai), Some(p));
        }
        remover(&gpu_dir).unwrap();
        assert!(!gpu_dir.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Download do item pelo servidor falso: o caminho de rede de `instalar_de` monta o leiaute; um pacote errado
    /// não deixa nada (nem a pasta antiga some).
    #[test]
    fn instala_pela_rede_falsa() {
        use sha2::{Digest, Sha256};
        let d = std::env::temp_dir().join(format!("p21-rede-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let tgz = {
            let mut v = Vec::new();
            {
                let enc = flate2::write::GzEncoder::new(&mut v, flate2::Compression::fast());
                let mut b = tar::Builder::new(enc);
                for nome in [format!("ort/lib/{LIB_NOME}"), "ort/lib/libonnxruntime_providers_shared.so".into()] {
                    let mut h = tar::Header::new_gnu();
                    h.set_size(3);
                    h.set_mode(0o644);
                    h.set_cksum();
                    b.append_data(&mut h, nome, &b"lib"[..]).unwrap();
                }
                b.into_inner().unwrap().finish().unwrap();
            }
            v
        };
        let roda = |nome: &str| {
            let mut c = std::io::Cursor::new(Vec::new());
            {
                let mut z = zip::ZipWriter::new(&mut c);
                z.start_file(format!("nvidia/x/lib/{nome}"), zip::write::FileOptions::default()).unwrap();
                z.write_all(b"so").unwrap();
                z.finish().unwrap();
            }
            c.into_inner()
        };
        let arquivos = vec![("ort.tgz".to_string(), tgz), ("cuda.whl".into(), roda("libcudart.so.13")), ("cudnn.whl".into(), roda("libcudnn.so.9"))];
        let sha = |b: &[u8]| format!("{:x}", Sha256::digest(b));
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        let (base, h) = pacote::testes::servidor(arquivos.clone(), 3);
        let itens: Vec<ArquivoGpu> = arquivos
            .iter()
            .zip([Destino::Onnxruntime, Destino::Cuda, Destino::Cudnn])
            .map(|((n, b), destino)| ArquivoGpu { nome: leak(n.clone()), url: leak(format!("{base}/{n}")), bytes: b.len() as u64, sha256: leak(sha(b)), destino })
            .collect();
        let gpu_dir = d.join("gpu");
        let nao = AtomicBool::new(false);
        instalar_de(&itens, &Fonte::Rede, &gpu_dir, &nao, &mut |_, _| {}).unwrap();
        h.join().unwrap();
        let p = PacoteGpu::da_pasta(&gpu_dir).expect("leiaute");
        assert_eq!(std::fs::read(&p.lib).unwrap(), b"lib");
        assert_eq!(std::fs::read(p.cudnn.join("libcudnn.so.9")).unwrap(), b"so");
        // pacote adulterado: erro, a pasta instalada fica como estava
        let (base2, h2) = pacote::testes::servidor(arquivos.clone(), 1);
        let mut ruins: Vec<ArquivoGpu> = itens.into_iter().take(1).collect();
        ruins[0].url = leak(format!("{base2}/ort.tgz"));
        ruins[0].sha256 = leak(sha(b"outro"));
        assert!(instalar_de(&ruins, &Fonte::Rede, &gpu_dir, &nao, &mut |_, _| {}).is_err());
        h2.join().unwrap();
        assert_eq!(std::fs::read(&p.lib).unwrap(), b"lib");
        assert!(!pacote::irma(&gpu_dir, "novo").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
