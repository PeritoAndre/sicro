//! Ambiente dos testes de IA e de rastreio (contratos-fase2.md (d)): onde estão os modelos, o onnxruntime, as
//! bibliotecas da NVIDIA e as 38 sequências, e a regra da dispensa. Usado por P2.1, P2.2, P2.3 e P2.5.
//! Um onnxruntime por processo: todo teste do `cargo test` usa `ort_lib()`; cenário com outra lib (CPU pura,
//! sem cuDNN) roda em processo filho.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::gpu::PacoteGpu;
use crate::commands::settings_commands::AiSettings;

/// Dossiê da Edição profissional (padrão das variáveis abaixo).
pub const DOSSIE: &str = "/home/andre/Work/sicro-bench/edicao-pro";

fn var_ou(var: &str, padrao: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| Path::new(DOSSIE).join(padrao))
}

fn existe(p: PathBuf) -> Option<PathBuf> {
    p.exists().then_some(p)
}

/// `SICRO_EXIGE_MODELOS=1`: recurso ausente faz o teste FALHAR em vez de se dispensar.
pub fn exige() -> bool {
    std::env::var("SICRO_EXIGE_MODELOS").is_ok_and(|v| v == "1")
}

/// Sem o recurso: com `SICRO_EXIGE_MODELOS=1` o teste falha; sem ela registra a dispensa no stderr e devolve None.
pub fn ou_dispensa<T>(o: Option<T>, teste: &str, falta: &str) -> Option<T> {
    if o.is_none() {
        assert!(!exige(), "{teste}: falta {falta} (SICRO_EXIGE_MODELOS=1)");
        eprintln!("teste dispensado: {teste}: falta {falta}");
    }
    o
}

/// `SICRO_MODELOS` ou `<dossiê>/modelos` (rastreio/, segmentacao/, redacao/ e SHA256SUMS).
pub fn modelos() -> Option<PathBuf> {
    existe(var_ou("SICRO_MODELOS", "modelos"))
}

/// Um arquivo de `modelos()` (caminho relativo, ex. `rastreio/edgetam_video/vision_encoder.onnx`) com o SHA-256
/// conferido; None = ausente ou diferente.
pub fn modelo(relativo: &str, sha256: &str) -> Option<PathBuf> {
    let p = modelos()?.join(relativo);
    conferir_sha256(&p, sha256).then_some(p)
}

/// `SICRO_EXIGE_GPU=1`: sem placa NVIDIA (ou com o autoteste reprovado) o teste de GPU FALHA em vez de se dispensar.
pub fn exige_gpu() -> bool {
    std::env::var("SICRO_EXIGE_GPU").is_ok_and(|v| v == "1")
}

/// `ou_dispensa` do que precisa de placa: falha com `SICRO_EXIGE_GPU=1`, senão registra a dispensa.
pub fn ou_dispensa_gpu<T>(o: Option<T>, teste: &str, falta: &str) -> Option<T> {
    if o.is_none() {
        assert!(!exige_gpu(), "{teste}: falta {falta} (SICRO_EXIGE_GPU=1)");
        eprintln!("teste dispensado: {teste}: falta {falta}");
    }
    o
}

/// Uma sessão de GPU por vez no processo de teste (EdgeTAM ≈ 1,5 GB de VRAM, placa dividida): todo teste de GPU, de
/// qualquer módulo (P2.1, P2.3), segura esta trava enquanto a sessão vive.
pub fn trava_gpu() -> std::sync::MutexGuard<'static, ()> {
    static GPU: std::sync::Mutex<()> = std::sync::Mutex::new(());
    GPU.lock().unwrap_or_else(|e| e.into_inner())
}

/// `SICRO_ORT_LIB` ou a lib de GPU do protótipo (também roda em CPU): a única do processo de teste.
/// Põe `ORT_DISABLE_TELEMETRY=1` antes: o `main.rs` não roda no `cargo test` e `with_telemetry(false)` não basta
/// (`gpu-inferencia.md` §8).
pub fn ort_lib() -> Option<PathBuf> {
    static TELEMETRIA: std::sync::Once = std::sync::Once::new();
    TELEMETRIA.call_once(|| std::env::set_var("ORT_DISABLE_TELEMETRY", "1"));
    existe(var_ou("SICRO_ORT_LIB", "p2/ort-cuda13/onnxruntime-linux-x64-gpu_cuda13-1.31.1/lib/libonnxruntime.so.1.31.1"))
}

/// Sessão de CPU com `ort_lib()`, sem passar pelo corpo de `ia::sessao` (P2.1): os testes da onda (porte do VitTrack
/// e do EdgeTAM em CPU, P2.3) rodam sem esperar o P2.1. None = sem a lib (dispensa pela regra 7); lib presente que
/// não carrega = falha.
pub fn sessao_cpu(modelo: super::Modelo<'_>) -> Option<super::SessaoIa> {
    let lib = ort_lib()?;
    let env = crate::video::ort_env::entorno(&lib).expect("onnxruntime dos testes");
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
    let mut b = ort::session::Session::builder(env)
        .map_err(|e| e.to_string())
        .and_then(|b| b.with_intra_threads(threads).map_err(|e| e.to_string()))
        .expect("sessão de teste");
    let session = match modelo {
        super::Modelo::Arquivo(p) => b.commit_from_file(p),
        super::Modelo::Bytes(m) => b.commit_from_memory(m),
    }
    .map_err(|e| e.to_string())
    .expect("grafo de teste");
    Some(super::SessaoIa { session, provedor: super::Provedor::Cpu, queda: None })
}

/// Pacote de GPU dos testes: `ort_lib()` + `SICRO_CUDA_DIR` e `SICRO_CUDNN_DIR` (padrão: `p2/gestor/cuda13` e
/// `p2/gestor/cudnn9-cu13`). Não diz se há placa: isso é o autoteste.
pub fn pacote_gpu() -> Option<PacoteGpu> {
    Some(PacoteGpu {
        lib: ort_lib()?,
        cuda: existe(var_ou("SICRO_CUDA_DIR", "p2/gestor/cuda13"))?,
        cudnn: existe(var_ou("SICRO_CUDNN_DIR", "p2/gestor/cudnn9-cu13"))?,
    })
}

/// `SICRO_ORT_CPU_LIB` ou a lib de CPU 1.31.1 da venv do dossiê (nunca a do dono): só no filho do cenário
/// "lib de CPU" (um onnxruntime por processo).
pub fn ort_cpu_lib() -> Option<PathBuf> {
    existe(var_ou("SICRO_ORT_CPU_LIB", "p3a/.venv-cv4/lib/python3.13/site-packages/onnxruntime/capi/libonnxruntime.so.1.31.1"))
}

/// `SICRO_SEQUENCIAS` ou `<dossiê>/p3a`: `sint/<nome>.{mp4,json}` (16) e `real/gt/<nome>.json` (22, com o vídeo
/// em `video`), verdade por quadro em `gt[]` (`inteira`, `fracao`); referência do laço em `res_laco/`.
pub fn sequencias() -> Option<PathBuf> {
    existe(var_ou("SICRO_SEQUENCIAS", "p3a"))
}

/// `AiSettings` dos testes: interruptor ligado, `ort_lib_path = ort_lib()`, EdgeTAM em `modelos()/rastreio/
/// edgetam_video`. `gpu_dir` vazio: GPU só por `criar_sessao_com` com `pacote_gpu()`.
pub fn ambiente() -> Option<AiSettings> {
    let lib = ort_lib()?;
    let mut ai = AiSettings { ort_lib_path: lib.to_string_lossy().into_owned(), ..Default::default() };
    if let Some(m) = modelos() {
        ai.rastreio_dir = m.join("rastreio/edgetam_video").to_string_lossy().into_owned();
    }
    Some(ai)
}

/// SHA-256 do arquivo igual ao esperado (minúsculas).
pub fn conferir_sha256(arquivo: &Path, esperado: &str) -> bool {
    let Ok(mut f) = std::fs::File::open(arquivo) else { return false };
    let mut h = Sha256::new();
    if std::io::copy(&mut f, &mut h).is_err() {
        return false;
    }
    format!("{:x}", h.finalize()) == esperado
}

#[cfg(test)]
mod t {
    use super::*;

    #[test]
    fn dispensa_sem_o_recurso_e_falha_quando_exigido() {
        if exige() {
            let r = std::panic::catch_unwind(|| ou_dispensa::<u8>(None, "x", "y"));
            assert!(r.is_err());
        } else {
            assert_eq!(ou_dispensa::<u8>(None, "x", "y"), None);
        }
        assert_eq!(ou_dispensa(Some(3), "x", "y"), Some(3));
        assert!(!conferir_sha256(Path::new("/nao/existe"), "00"));
    }

    /// O ambiente da onda: a lib dos testes roda o VitTrack embutido em CPU neste processo (P2.3 testa o porte sem
    /// o corpo do P2.1) e a telemetria fica desligada antes de a lib carregar.
    #[test]
    fn sessao_cpu_roda_o_vittrack() {
        use crate::edicao::rastreio::vittrack;
        let Some(mut s) = ou_dispensa(sessao_cpu(super::super::Modelo::Bytes(vittrack::MODELO)), "sessao_cpu_roda_o_vittrack", "a lib dos testes")
        else {
            return;
        };
        assert_eq!(std::env::var("ORT_DISABLE_TELEMETRY").as_deref(), Ok("1"));
        assert_eq!(s.provedor, super::super::Provedor::Cpu);
        let t = ort::value::Tensor::from_array(([1usize, 3, 128, 128], vec![0.0f32; 3 * 128 * 128])).unwrap();
        let b = ort::value::Tensor::from_array(([1usize, 3, 256, 256], vec![0.0f32; 3 * 256 * 256])).unwrap();
        let saida = s.session.run(ort::inputs!["template" => t, "search" => b]).unwrap();
        let (forma, conf) = saida[0].try_extract_tensor::<f32>().unwrap();
        assert_eq!(forma.iter().copied().collect::<Vec<i64>>(), vec![1, 1, 16, 16]);
        assert!(conf.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
    }

    #[test]
    fn dispensa_de_gpu_falha_quando_exigida() {
        if exige_gpu() {
            assert!(std::panic::catch_unwind(|| ou_dispensa_gpu::<u8>(None, "x", "y")).is_err());
        } else {
            assert_eq!(ou_dispensa_gpu::<u8>(None, "x", "y"), None);
        }
        assert_eq!(ou_dispensa_gpu(Some(1), "x", "y"), Some(1));
    }

    fn sha(b: &[u8]) -> String {
        format!("{:x}", Sha256::digest(b))
    }

    /// Os dados do contrato (SHA-256 e bytes dos modelos embutidos e do item "Rastreador") batem com os arquivos.
    #[test]
    fn dados_dos_modelos_conferem() {
        use crate::edicao::rastreio::{edgetam, vittrack};
        assert_eq!(sha(vittrack::MODELO), vittrack::SHA256);
        assert_eq!(vittrack::MODELO.len(), 714_726);
        assert_eq!(sha(edgetam::CONSTANTES), edgetam::SHA256_CONSTANTES);
        assert_eq!(edgetam::CONSTANTES.len(), 2_816);
        assert_eq!(sha(super::super::gpu::MODELO_AUTOTESTE), "884669ea0383005236899be837f81f9968237fe3415d4261d8b0377b3c5f9e46");
        let Some(_) = ou_dispensa(modelos(), "dados_dos_modelos_conferem", "a pasta de modelos") else { return };
        let v = modelo("rastreio/vittrack/object_tracking_vittrack_2023sep.onnx", vittrack::SHA256);
        assert!(ou_dispensa(v, "dados_dos_modelos_conferem", "o VitTrack").is_some() || !exige());
        for a in edgetam::ARQUIVOS {
            let p = modelo(&format!("rastreio/edgetam_video/{}", a.nome), a.sha256);
            let Some(p) = ou_dispensa(p, "dados_dos_modelos_conferem", a.nome) else { continue };
            assert_eq!(std::fs::metadata(&p).unwrap().len(), a.bytes, "{}", a.nome);
        }
    }
}
