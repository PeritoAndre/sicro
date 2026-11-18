//! Sonda de capacidades e escolha do binário (§5.6). Dono: P0.4.
//! Protótipos: `p1c/scripts/sonda_gpu.py` e `sonda_valida_saida.py`; regras em `receitas-gpu-ffmpeg.md` §1 e §9.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::exec::{self, Comando, Termino};
use crate::error::{Result, SicroError};

/// De onde veio o binário: `SICRO_FFMPEG`, o embutido ou o PATH.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Origem {
    Configurado,
    Embutido,
    Path,
}

/// Como o binário recebe o grafo em arquivo: `-/filter_complex` ou `-filter_complex_script`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GrafoArquivo {
    BarraFilterComplex,
    FilterComplexScript,
}

/// Codificador de GPU aprovado: `degrau` = nome do degrau de parâmetros que passou; `args` = os dele.
/// NVENC: `nome` e `driver` = placa e versão do driver (`nvidia-smi`). VAAPI: `nome` = nó de render
/// (o render põe `-vaapi_device <nome>` antes das entradas e `format=nv12,hwupload` no fim do grafo),
/// `driver` = driver do kernel. QSV pede `-pix_fmt nv12`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Gpu {
    pub nome: String,
    pub driver: String,
    pub codificador: String,
    pub degrau: String,
    pub args: Vec<String>,
    pub nvdec: bool,
    pub scale_cuda: bool,
}

/// Por que um codificador de GPU foi reprovado (classificado pelo stderr).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClasseReprovacao {
    SemGpu,
    SemDriver,
    DriverAntigo,
    LimiteDeSessoes,
    RecursoNaoSuportado,
    FfmpegSemCodificador,
    /// Código 0 e vídeo destruído (conferência da saída, PSNR < 30 dB).
    SaidaInvalida,
    Outra,
}

/// Codificador reprovado e por quê. Os três últimos só em `driver_antigo`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Reprovado {
    pub codificador: String,
    pub classe: ClasseReprovacao,
    pub dica: String,
    pub driver_minimo: Option<String>,
    pub api_exigida: Option<String>,
    pub api_do_driver: Option<String>,
}

/// O que o binário escolhido sabe fazer. `faltam` = obrigatórios ausentes (filtros, e também `ffprobe`,
/// codificadores, muxers, decodificadores e `autoteste` quando falham); `gpu` None até a sonda de GPU acabar
/// (ou sem codificador aprovado).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Capacidades {
    pub binario: PathBuf,
    /// Da mesma pasta do binário.
    pub ffprobe: PathBuf,
    pub origem: Origem,
    pub versao: String,
    pub configuracao: String,
    pub sha256: String,
    pub filtros: BTreeSet<String>,
    pub codificadores: BTreeSet<String>,
    pub decodificadores: BTreeSet<String>,
    pub hwaccels: BTreeSet<String>,
    pub muxers: BTreeSet<String>,
    pub grafo_em_arquivo: GrafoArquivo,
    pub faltam: Vec<String>,
    pub gpu: Option<Gpu>,
    pub reprovados: Vec<Reprovado>,
}

/// Resumo para a tela (`edicao_capacidades`). `codificador` = o que a exportação "auto" usaria agora;
/// `sondando` = sonda de GPU em andamento.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumoCapacidades {
    pub versao: String,
    pub origem: Origem,
    pub sha256: String,
    pub faltam: Vec<String>,
    pub opcionais_ausentes: Vec<String>,
    pub codificador: String,
    pub gpu: Option<Gpu>,
    pub reprovados: Vec<Reprovado>,
    pub sondando: bool,
}

/// Filtros obrigatórios: os do autoteste, os do §5.6 e os que toda cadeia usa (§3.3, §5.3, §5.9).
const FILTROS_OBRIGATORIOS: &[&str] = &[
    "trim", "settb", "setpts", "fps", "tpad", "alphamerge", "overlay", "xfade", "concat", "scale", "crop", "pad", "boxblur",
    "lutyuv", "extractplanes", "mergeplanes", "eq", "showinfo", "atrim", "asplit", "atempo", "amix", "afade", "apad",
    "aresample", "loudnorm", "alimiter", "format", "split", "setsar", "setparams", "asetpts", "asetnsamples", "volume", "pan",
    "aevalsrc", "aformat", "asetrate", "adelay", "anullsrc", "transpose", "hflip", "vflip", "fade", "color",
    // a janela sem slot de lupa ativo termina a cópia nele (P1.2)
    "nullsink",
];
const FILTROS_OPCIONAIS: &[&str] = &["vidstabdetect", "vidstabtransform", "lenscorrection", "bwdif", "tmix", "perspective"];
const CODIFICADORES_OBRIGATORIOS: &[&str] = &["libx264", "aac", "pcm_s16le"];
const DECODIFICADORES_OBRIGATORIOS: &[&str] = &["h264", "png"];
const MUXERS_OBRIGATORIOS: &[&str] = &["framehash", "hash", "mp4", "matroska", "null", "wav"];

/// Conferência da saída: abaixo disso o codificador mentiu (`hevc_vaapi` da AMD: rc 0 e 5 dB).
const PSNR_MINIMO: f64 = 30.0;
/// Tempo máximo de cada tentativa da sonda (o de `sonda_gpu.py`).
const LIMITE_TENTATIVA: Duration = Duration::from_secs(20);
/// Tempo máximo do `nvidia-smi` (responde em ~50 ms; 2 s a frio).
const LIMITE_SMI: Duration = Duration::from_secs(10);
/// Quadros da conferência da saída: todos têm de voltar da decodificação.
const QUADROS_CONFERENCIA: u32 = 30;
/// Fonte da sonda de GPU: 1080p, a mesma do protótipo.
const ENTRADA_GPU: &str = "testsrc2=s=1920x1080:r=30";

/// Degraus de parâmetros por codificador, do mais completo ao mais simples (`receitas-gpu-ffmpeg.md` §1.4).
fn degraus(cod: &str) -> &'static [(&'static str, &'static [&'static str])] {
    match cod {
        "h264_nvenc" => &[
            ("completo", &["-preset", "p6", "-tune", "hq", "-rc", "vbr", "-cq", "21", "-b:v", "0", "-profile:v", "high"]),
            ("basico", &["-preset", "p4", "-rc", "vbr", "-cq", "21", "-b:v", "0", "-bf", "0", "-profile:v", "high"]),
            ("minimo", &["-rc", "constqp", "-qp", "21"]),
        ],
        "h264_qsv" => &[
            (
                "completo",
                &[
                    "-preset", "slow", "-global_quality", "20", "-look_ahead", "1", "-look_ahead_depth", "40", "-extbrc", "1", "-bf",
                    "3", "-profile:v", "high",
                ],
            ),
            ("basico", &["-preset", "medium", "-global_quality", "20", "-profile:v", "high"]),
        ],
        "h264_amf" => &[
            (
                "completo",
                &[
                    "-usage", "transcoding", "-quality", "quality", "-rc", "qvbr", "-qvbr_quality_level", "20", "-preanalysis", "1",
                    "-vbaq", "1", "-bf", "2", "-profile:v", "high",
                ],
            ),
            (
                "basico",
                &[
                    "-usage", "transcoding", "-quality", "quality", "-rc", "cqp", "-qp_i", "18", "-qp_p", "20", "-qp_b", "22",
                    "-profile:v", "high",
                ],
            ),
        ],
        "h264_vaapi" => &[
            ("icq", &["-rc_mode", "ICQ", "-global_quality", "20", "-profile:v", "high"]),
            ("cqp_b", &["-rc_mode", "CQP", "-qp", "18", "-bf", "2", "-profile:v", "high"]),
            ("basico", &["-rc_mode", "CQP", "-qp", "18", "-bf", "0", "-profile:v", "high"]),
        ],
        "h264_mf" => &[("basico", &["-hw_encoding", "1", "-rate_control", "quality", "-quality", "80"])],
        _ => &[],
    }
}

/// Ordem de tentativa por sistema (§5.6); `libx264` é a queda e não entra.
fn ordem_padrao() -> &'static [&'static str] {
    if cfg!(windows) {
        &["h264_nvenc", "h264_qsv", "h264_amf", "h264_mf"]
    } else {
        &["h264_nvenc", "h264_qsv", "h264_vaapi"]
    }
}

// ---------------------------------------------------------------- estado do processo

#[derive(Default)]
struct Estado {
    cache: Option<PathBuf>,
    por_binario: HashMap<PathBuf, Entrada>,
    /// Sondas de GPU em andamento ou esperando a vez, por SHA-256 do binário.
    sondando: HashMap<String, u32>,
}

struct Entrada {
    tamanho: u64,
    mtime: Option<SystemTime>,
    cap: Arc<Capacidades>,
}

fn estado() -> std::sync::MutexGuard<'static, Estado> {
    static E: OnceLock<Mutex<Estado>> = OnceLock::new();
    E.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// Uma sonda de listas por vez (duas chamadas juntas não sondam o mesmo binário duas vezes).
static SONDA_LISTAS: Mutex<()> = Mutex::new(());
/// Uma sonda de GPU por vez: a de fundo e o "Testar de novo" disputariam as sessões da placa.
static SONDA_GPU: Mutex<()> = Mutex::new(());

/// Conta uma sonda de GPU de `sha` enquanto vive: `sondando` só cai quando a última acaba.
struct Sondando(String);

impl Sondando {
    fn novo(sha: &str) -> Self {
        *estado().sondando.entry(sha.to_string()).or_insert(0) += 1;
        Sondando(sha.to_string())
    }
}

impl Drop for Sondando {
    fn drop(&mut self) {
        let mut e = estado();
        if let Some(n) = e.sondando.get_mut(&self.0) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                e.sondando.remove(&self.0);
            }
        }
    }
}

/// Pasta do cache das sondas (`app_cache_dir()/edicao`). Chamada uma vez na partida; sem ela não há cache em disco
/// nem sonda de GPU em segundo plano.
pub fn configurar(cache_edicao: &Path) {
    let mut e = estado();
    e.cache = Some(cache_edicao.to_path_buf());
    // o que foi sondado antes (sem cache, sem GPU) não vale mais
    e.por_binario.clear();
}

#[cfg(test)]
thread_local! {
    /// Testes: capacidades no lugar das do binário escolhido, só nesta thread.
    pub(crate) static FORCADAS: std::cell::RefCell<Option<Arc<Capacidades>>> = const { std::cell::RefCell::new(None) };
}

/// Capacidades do binário escolhido: memória → `sonda-<sha256>.json` → sonda. Erro se `SICRO_FFMPEG` tem `faltam`.
/// Com o cache configurado, a GPU vem de `sonda-gpu.json` (chave igual) ou da sonda lançada em segundo plano.
pub fn capacidades() -> Result<Arc<Capacidades>> {
    #[cfg(test)]
    if let Some(c) = FORCADAS.with(|f| f.borrow().clone()) {
        return Ok(c);
    }
    let (bin, origem) = escolher()?;
    let cache = estado().cache.clone();
    let cap = capacidades_de(&bin, origem, cache.as_deref(), cache.is_some())?;
    exigir(&cap)?;
    Ok(cap)
}

/// `SICRO_FFMPEG` sem um obrigatório não cai para outro binário: é erro, com a lista.
fn exigir(cap: &Capacidades) -> Result<()> {
    if cap.origem == Origem::Configurado && !cap.faltam.is_empty() {
        return Err(SicroError::Validation(format!("filtro_ausente: {}", cap.faltam.join(", "))));
    }
    Ok(())
}

/// Sonda reprovada (de qualquer origem: o do PATH, como no .deb, também): a Edição abre e edita, mas nada que
/// renderiza pelo motor entra na fila (§12 item 2).
pub(crate) fn exigir_minimo(cap: &Capacidades) -> Result<()> {
    if cap.faltam.is_empty() {
        return Ok(());
    }
    Err(SicroError::Validation(format!("{}: {}", crate::edicao::mensagens::codigo::FFMPEG_ABAIXO_DO_MINIMO, cap.faltam.join(", "))))
}

/// O binário: `SICRO_FFMPEG` vence sempre; senão o campo "ffmpeg" das Configurações; senão o embutido; senão o PATH.
/// Os dois primeiros, definidos e ausentes, são erro (nunca cai calado para outro).
pub(crate) fn escolher() -> Result<(PathBuf, Origem)> {
    escolher_com(
        std::env::var_os("SICRO_FFMPEG"),
        crate::commands::settings_commands::ffmpeg_configurado(),
        crate::tools::bundled_ffmpeg_tool("ffmpeg"),
        which::which("ffmpeg").ok(),
    )
}

fn escolher_com(var: Option<OsString>, campo: Option<PathBuf>, embutido: Option<PathBuf>, path: Option<PathBuf>) -> Result<(PathBuf, Origem)> {
    if let Some(v) = var.filter(|v| !v.is_empty()) {
        let p = std::path::absolute(PathBuf::from(&v)).unwrap_or_else(|_| PathBuf::from(&v));
        if !p.is_file() {
            return Err(SicroError::Validation(format!("ffmpeg_ausente: SICRO_FFMPEG={}", p.display())));
        }
        return Ok((p, Origem::Configurado));
    }
    if let Some(c) = campo.filter(|c| !c.as_os_str().is_empty()) {
        let p = std::path::absolute(&c).unwrap_or(c);
        if !p.is_file() {
            return Err(SicroError::Validation(format!("ffmpeg_ausente: {}", p.display())));
        }
        return Ok((p, Origem::Configurado));
    }
    if let Some(p) = embutido {
        return Ok((p, Origem::Embutido));
    }
    if let Some(p) = path {
        return Ok((p, Origem::Path));
    }
    Err(SicroError::Validation("ffmpeg_ausente".into()))
}

fn origem_de(binario: &Path) -> Origem {
    match escolher() {
        Ok((p, o)) if p == binario => o,
        _ if crate::tools::bundled_ffmpeg_tool("ffmpeg").as_deref() == Some(binario) => Origem::Embutido,
        _ => Origem::Path,
    }
}

/// Capacidades de um binário com o cache em memória e, se houver pasta, em disco. `gpu_em_fundo` lança a sonda
/// de GPU numa thread quando o cache de GPU não serve.
pub(crate) fn capacidades_de(bin: &Path, origem: Origem, cache: Option<&Path>, gpu_em_fundo: bool) -> Result<Arc<Capacidades>> {
    let meta = std::fs::metadata(bin)?;
    let (tamanho, mtime) = (meta.len(), meta.modified().ok());
    let da_memoria = || {
        estado()
            .por_binario
            .get(bin)
            .filter(|e| e.tamanho == tamanho && e.mtime == mtime && e.cap.origem == origem)
            .map(|e| e.cap.clone())
    };
    if let Some(c) = da_memoria() {
        return Ok(c);
    }
    let _serie = SONDA_LISTAS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(c) = da_memoria() {
        return Ok(c);
    }
    let sha = sha256_arquivo(bin)?;
    let do_disco = cache
        .and_then(|d| ler_json::<Capacidades>(&d.join(format!("sonda-{sha}.json"))))
        .filter(|c| c.sha256 == sha);
    let mut cap = match do_disco {
        Some(mut c) => {
            c.binario = bin.to_path_buf();
            c.ffprobe = ffprobe_de(bin);
            c.origem = origem;
            // refeito: a lista de obrigatórios muda com a versão do SICRO e o ffprobe pode ter aparecido
            c.faltam = faltam_das_listas(&c);
            c
        }
        None => {
            let c = sondar_listas(bin, origem, &sha)?;
            // autoteste falho pode ser passageiro (disco, máquina ocupada): fica só na memória
            if let (Some(d), false) = (cache, c.faltam.iter().any(|f| f == "autoteste")) {
                gravar_json(&d.join(format!("sonda-{sha}.json")), &c)?;
            }
            c
        }
    };
    cap.gpu = None;
    cap.reprovados.clear();
    let mut lancar = false;
    if let Some(d) = cache {
        match ler_gpu(d, &chave_gpu(&sha)) {
            Some(e) => {
                cap.gpu = e.gpu;
                cap.reprovados = e.reprovados;
            }
            None => lancar = gpu_em_fundo,
        }
    }
    let cap = Arc::new(cap);
    estado().por_binario.insert(bin.to_path_buf(), Entrada { tamanho, mtime, cap: cap.clone() });
    if lancar {
        lancar_gpu(cap.clone(), cache.map(Path::to_path_buf));
    }
    Ok(cap)
}

fn lancar_gpu(cap: Arc<Capacidades>, cache: Option<PathBuf>) {
    let vez = {
        let mut e = estado();
        if e.sondando.get(&cap.sha256).is_some_and(|n| *n > 0) {
            return;
        }
        e.sondando.insert(cap.sha256.clone(), 1);
        Sondando(cap.sha256.clone())
    };
    std::thread::spawn(move || {
        let _vez = vez;
        let _serie = SONDA_GPU.lock().unwrap_or_else(|e| e.into_inner());
        let r = sondar_gpu(&cap, ordem_padrao(), &[]);
        guardar_gpu(&cap, r, cache.as_deref());
    });
}

/// Grava a sonda de GPU (salvo passageiro) e troca a entrada da memória pela nova. Sonda cortada pela saída do app
/// (os filhos mortos por `matar_todos` saem como falha do codificador) não vai para o disco: gravada, a placa ficava
/// reprovada, ou num degrau pior, até o perito mandar testar de novo.
fn guardar_gpu(cap: &Capacidades, r: ResultadoGpu, cache: Option<&Path>) -> Arc<Capacidades> {
    if let (Some(d), false) = (cache, r.passageiro || exec::encerrando()) {
        let mut todas: BTreeMap<String, EntradaGpu> = ler_json(&d.join("sonda-gpu.json")).unwrap_or_default();
        todas.insert(
            cap.sha256.clone(),
            EntradaGpu { chave: chave_gpu(&cap.sha256), gpu: r.gpu.clone(), reprovados: r.reprovados.clone() },
        );
        if let Err(e) = gravar_json(&d.join("sonda-gpu.json"), &todas) {
            tracing::warn!("sonda-gpu.json: {e}");
        }
    }
    let nova = Arc::new(Capacidades { gpu: r.gpu, reprovados: r.reprovados, ..cap.clone() });
    let mut e = estado();
    if let Some(ent) = e.por_binario.get_mut(&cap.binario) {
        if ent.cap.sha256 == cap.sha256 {
            ent.cap = nova.clone();
        }
    }
    nova
}

/// Sonda um binário específico, sem cache (testes nos dois ffmpeg): listas, autoteste e GPU.
pub fn sondar(binario: &Path) -> Result<Capacidades> {
    let sha = sha256_arquivo(binario)?;
    let mut cap = sondar_listas(binario, origem_de(binario), &sha)?;
    let r = sondar_gpu(&cap, ordem_padrao(), &[]);
    cap.gpu = r.gpu;
    cap.reprovados = r.reprovados;
    Ok(cap)
}

/// Refaz a sonda de GPU ("Testar de novo", ou depois de falha de render por GPU) e devolve o novo estado.
pub fn testar_de_novo() -> Result<Arc<Capacidades>> {
    let (bin, origem) = escolher()?;
    let cache = estado().cache.clone();
    testar_de_novo_de(&bin, origem, cache.as_deref())
}

fn testar_de_novo_de(bin: &Path, origem: Origem, cache: Option<&Path>) -> Result<Arc<Capacidades>> {
    let cap = capacidades_de(bin, origem, cache, false)?;
    exigir(&cap)?;
    let _vez = Sondando::novo(&cap.sha256);
    let _serie = SONDA_GPU.lock().unwrap_or_else(|e| e.into_inner());
    let r = sondar_gpu(&cap, ordem_padrao(), &[]);
    Ok(guardar_gpu(&cap, r, cache))
}

/// Codificação real de 8 quadros no tamanho e na taxa do projeto com o codificador de GPU aprovado; false = usar CPU.
pub fn gpu_serve(cap: &Capacidades, largura: u32, altura: u32, fps: crate::edicao::modelo::Racional) -> bool {
    let Some(g) = &cap.gpu else { return false };
    let Ok(pasta) = PastaTemp::nova("gpu-serve") else { return false };
    let entrada = format!("testsrc2=s={largura}x{altura}:r={}/{}", fps.num, fps.den);
    let no = (g.codificador == "h264_vaapi").then_some(g.nome.as_str());
    let args = linha_codificar(&g.codificador, &g.args, no, &entrada, 8, Some("null"), "-");
    matches!(rodar_limitado(&cmd_sonda(cap, &pasta.0, "gpu-serve", args, &[])), Ok((t, false)) if t.codigo == Some(0))
}

/// Resumo para a tela.
pub fn resumo(cap: &Capacidades) -> ResumoCapacidades {
    ResumoCapacidades {
        versao: cap.versao.clone(),
        origem: cap.origem,
        sha256: cap.sha256.clone(),
        faltam: cap.faltam.clone(),
        opcionais_ausentes: FILTROS_OPCIONAIS.iter().filter(|f| !cap.filtros.contains(**f)).map(|f| f.to_string()).collect(),
        codificador: cap.gpu.as_ref().map(|g| g.codificador.clone()).unwrap_or_else(|| "libx264".into()),
        gpu: cap.gpu.clone(),
        reprovados: cap.reprovados.clone(),
        sondando: estado().sondando.get(&cap.sha256).is_some_and(|n| *n > 0),
    }
}

// ---------------------------------------------------------------- listas e autoteste

fn ffprobe_de(bin: &Path) -> PathBuf {
    bin.with_file_name(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" })
}

pub(crate) fn sha256_arquivo(p: &Path) -> Result<String> {
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

/// Pasta de trabalho temporária da sonda, apagada no fim.
struct PastaTemp(PathBuf);

impl PastaTemp {
    fn nova(nome: &str) -> Result<Self> {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "sicro-sonda-{nome}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p)?;
        Ok(Self(p))
    }
}

impl Drop for PastaTemp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cmd_em(programa: &Path, pasta: &Path, nome: &str, args: Vec<String>, ambiente: &[(String, String)]) -> Comando {
    Comando {
        nome: nome.to_string(),
        programa: programa.to_path_buf(),
        args,
        pasta: pasta.to_path_buf(),
        ambiente: ambiente.to_vec(),
        // LIBVA_DRIVER_NAME=nvidia força o driver errado no nó da AMD (receitas §1.4)
        sem_ambiente: vec!["LIBVA_DRIVER_NAME".into()],
        stderr: pasta.join(format!("{nome}.stderr")),
    }
}

fn cmd_sonda(cap: &Capacidades, pasta: &Path, nome: &str, args: Vec<String>, ambiente: &[(String, String)]) -> Comando {
    cmd_em(&cap.binario, pasta, nome, args, ambiente)
}

/// Roda com tempo máximo; o segundo valor diz se esgotou.
fn rodar_limitado(cmd: &Comando) -> Result<(Termino, bool)> {
    limitado(LIMITE_TENTATIVA, |cancelar| exec::rodar(cmd, cancelar))
}

/// Como `rodar_limitado`, com o stdout (listas, `nvidia-smi`): programa que não responde não trava a partida.
fn saida_limitada(cmd: &Comando, limite: Duration) -> Result<(Termino, Vec<u8>, bool)> {
    limitado(limite, |cancelar| exec::rodar_com_saida(cmd, cancelar)).map(|((t, out), esgotou)| (t, out, esgotou))
}

fn limitado<T>(limite: Duration, f: impl FnOnce(&AtomicBool) -> Result<T>) -> Result<(T, bool)> {
    let cancelar = AtomicBool::new(false);
    let pronto = AtomicBool::new(false);
    let esgotou = AtomicBool::new(false);
    let t = std::thread::scope(|s| {
        s.spawn(|| {
            let t0 = Instant::now();
            while !pronto.load(Ordering::SeqCst) {
                if t0.elapsed() > limite {
                    esgotou.store(true, Ordering::SeqCst);
                    cancelar.store(true, Ordering::SeqCst);
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let r = f(&cancelar);
        pronto.store(true, Ordering::SeqCst);
        r
    })?;
    Ok((t, esgotou.load(Ordering::SeqCst)))
}

fn saida_texto(bin: &Path, pasta: &Path, nome: &str, args: &[&str]) -> Result<String> {
    let cmd = cmd_em(bin, pasta, nome, args.iter().map(|s| s.to_string()).collect(), &[]);
    let (t, out, esgotou) = saida_limitada(&cmd, LIMITE_TENTATIVA)?;
    if esgotou {
        return Err(SicroError::Validation(format!("ffmpeg_invalido: {} não respondeu a {nome}", bin.display())));
    }
    exec::exigir_sucesso(&cmd, &t).map_err(|e| SicroError::Validation(format!("ffmpeg_invalido: {} {e}", bin.display())))?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Listas, obrigatórios e autoteste (sem GPU).
pub(crate) fn sondar_listas(bin: &Path, origem: Origem, sha: &str) -> Result<Capacidades> {
    let pasta = PastaTemp::nova("listas")?;
    let p = &pasta.0;
    let v = saida_texto(bin, p, "version", &["-version"])?;
    let versao = v.lines().next().and_then(|l| l.split_whitespace().nth(2)).unwrap_or("").to_string();
    let configuracao = ler_buildconf(&saida_texto(bin, p, "buildconf", &["-hide_banner", "-buildconf"])?);
    let filtros = ler_filtros(&saida_texto(bin, p, "filters", &["-hide_banner", "-filters"])?);
    let codificadores = ler_codecs(&saida_texto(bin, p, "encoders", &["-hide_banner", "-encoders"])?);
    let decodificadores = ler_codecs(&saida_texto(bin, p, "decoders", &["-hide_banner", "-decoders"])?);
    let hwaccels = ler_hwaccels(&saida_texto(bin, p, "hwaccels", &["-hide_banner", "-hwaccels"])?);
    let muxers = ler_muxers(&saida_texto(bin, p, "muxers", &["-hide_banner", "-muxers"])?);
    let mut cap = Capacidades {
        binario: bin.to_path_buf(),
        ffprobe: ffprobe_de(bin),
        origem,
        versao,
        configuracao,
        sha256: sha.to_string(),
        filtros,
        codificadores,
        decodificadores,
        hwaccels,
        muxers,
        grafo_em_arquivo: GrafoArquivo::BarraFilterComplex,
        faltam: Vec::new(),
        gpu: None,
        reprovados: Vec::new(),
    };
    cap.faltam = faltam_das_listas(&cap);
    match autoteste(bin, p) {
        Ok(g) => cap.grafo_em_arquivo = g,
        Err(motivo) => {
            tracing::warn!("autoteste do ffmpeg {}: {motivo}", bin.display());
            cap.faltam.push("autoteste".into());
        }
    }
    Ok(cap)
}

/// Obrigatórios ausentes das listas, mais `ffprobe` se não está ao lado do binário.
fn faltam_das_listas(c: &Capacidades) -> Vec<String> {
    let mut faltam: Vec<String> = Vec::new();
    for (lista, tem) in [
        (FILTROS_OBRIGATORIOS, &c.filtros),
        (CODIFICADORES_OBRIGATORIOS, &c.codificadores),
        (DECODIFICADORES_OBRIGATORIOS, &c.decodificadores),
        (MUXERS_OBRIGATORIOS, &c.muxers),
    ] {
        faltam.extend(lista.iter().filter(|x| !tem.contains(**x)).map(|x| x.to_string()));
    }
    if !c.ffprobe.is_file() {
        faltam.push("ffprobe".into());
    }
    faltam
}

fn ler_buildconf(t: &str) -> String {
    t.lines()
        .map(str::trim)
        .skip_while(|l| !l.starts_with("configuration:"))
        .skip(1)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// ` TSC nome  V->V  descrição` (o 7.0.2 tem três colunas de marca, o 9.0.1 duas).
fn ler_filtros(t: &str) -> BTreeSet<String> {
    t.lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            (c.len() >= 3 && c[0].chars().all(|x| "TSC.".contains(x)) && c[2].contains("->")).then(|| c[1].to_string())
        })
        .collect()
}

/// ` V....D nome  descrição`.
fn ler_codecs(t: &str) -> BTreeSet<String> {
    t.lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            (c.len() >= 2
                && c[0].len() == 6
                && c[0].starts_with(['V', 'A', 'S', 'D', 'T'])
                && c[0].chars().all(|x| "VASDTFXBI.".contains(x))
                && c[1] != "=")
                .then(|| c[1].to_string())
        })
        .collect()
}

fn ler_hwaccels(t: &str) -> BTreeSet<String> {
    t.lines()
        .skip_while(|l| !l.starts_with("Hardware acceleration methods"))
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// `  E  nome  descrição` depois da linha `---`; nomes separados por vírgula valem um a um.
fn ler_muxers(t: &str) -> BTreeSet<String> {
    t.lines()
        .skip_while(|l| !l.trim_start().starts_with("--"))
        .skip(1)
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            (c.len() >= 2 && c[0].chars().all(|x| "DEd".contains(x))).then(|| c[1])
        })
        .flat_map(|n| n.split(',').map(String::from).collect::<Vec<_>>())
        .collect()
}

/// Autoteste de ~1 s (§5.6): grafo em arquivo com `trim` por pts, `settb`, `setpts`, `fps`, `tpad`, `alphamerge`,
/// `overlay`, `xfade`, `concat`, entrada `.ffconcat`, giro do contêiner, `-fps_mode` e saída `framehash`. Devolve a
/// forma do grafo em arquivo que este binário aceita.
fn autoteste(bin: &Path, pasta: &Path) -> std::result::Result<GrafoArquivo, String> {
    let rodar = |nome: &str, args: &[&str]| -> std::result::Result<(Termino, PathBuf), String> {
        let cmd = cmd_em(bin, pasta, nome, args.iter().map(|s| s.to_string()).collect(), &[]);
        let (t, _) = rodar_limitado(&cmd).map_err(|e| e.to_string())?;
        Ok((t, cmd.stderr))
    };
    let ok = |(t, e): (Termino, PathBuf)| -> std::result::Result<(), String> {
        if t.codigo == Some(0) {
            Ok(())
        } else {
            Err(format!("{}: {}", t.nome, exec::ultimas_linhas(&e, 3).join(" | ")))
        }
    };
    // 64x48 codificado, 48x64 exibido: o alphamerge com a máscara 48x64 só abre se o giro foi aplicado
    ok(rodar(
        "fonte",
        &[
            "-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=gray:s=64x48:r=30:d=0.4", "-c:v",
            "libx264", "-qp", "0", "-bf", "0", "-pix_fmt", "yuv420p", "-video_track_timescale", "15360", "a0.mp4",
        ],
    )?)?;
    let giro = rodar(
        "giro",
        &["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-display_rotation", "90", "-i", "a0.mp4", "-c", "copy", "a.mp4"],
    )?;
    if giro.0.codigo != Some(0) {
        // ffmpeg anterior ao 7: metadado rotate
        ok(rodar(
            "giro-antigo",
            &["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-i", "a0.mp4", "-c", "copy", "-metadata:s:v:0", "rotate=90", "a.mp4"],
        )?)?;
    }
    for (k, v) in [(0u8, 255u8), (1, 0)] {
        image::GrayImage::from_pixel(48, 64, image::Luma([v]))
            .save(pasta.join(format!("m{k}.png")))
            .map_err(|e| e.to_string())?;
    }
    let mut lista = String::from("ffconcat version 1.0\n");
    for k in 0..12 {
        lista.push_str(&format!("file 'm{}.png'\n", k % 2));
    }
    super::args::gravar_texto(pasta, "lista.ffconcat", &lista).map_err(|e| e.to_string())?;
    let grafo = "[0:v]trim=start_pts=512:end_pts=5632,settb=1/30,setpts=N,fps@t0=30:start_time=0,\
                 tpad=stop_mode=clone:stop=2,format=yuv420p,split=4[a][b][c][d];\
                 [1:v]settb=1/30,setpts=N,format=gray,trim=end_frame=12[m];\
                 [b][m]alphamerge[am];\
                 [a][am]overlay=format=yuv420:eof_action=pass,format=yuv420p[o];\
                 [c][d]xfade=transition=fade:duration=0.1:offset=0.2[x];\
                 [o][x]concat=n=2:v=1:a=0,format=yuv420p[v]";
    super::args::gravar_texto(pasta, "g.txt", grafo).map_err(|e| e.to_string())?;
    let mut ultimo = String::new();
    for (forma, opcao) in [(GrafoArquivo::BarraFilterComplex, "-/filter_complex"), (GrafoArquivo::FilterComplexScript, "-filter_complex_script")] {
        let _ = std::fs::remove_file(pasta.join("s.framehash"));
        let (t, err) = rodar(
            "autoteste",
            &[
                "-hide_banner", "-nostdin", "-y", "-nostats", "-loglevel", "verbose", "-i", "a.mp4", "-f", "concat", "-safe", "0", "-i",
                "lista.ffconcat", opcao, "g.txt", "-map", "[v]", "-fps_mode", "passthrough", "-f", "framehash", "-hash", "sha256", "s.framehash",
            ],
        )?;
        if t.codigo != Some(0) {
            let l = exec::ultimas_linhas(&err, 5);
            ultimo = format!("{opcao}: {}", l.join(" | "));
            if l.iter().any(|x| x.contains("Unrecognized option") || x.contains("Option not found")) {
                continue;
            }
            return Err(ultimo);
        }
        let fh = std::fs::read_to_string(pasta.join("s.framehash")).map_err(|e| e.to_string())?;
        let quadros = fh.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).count();
        let dimensoes = fh.lines().any(|l| l.starts_with("#dimensions") && l.trim_end().ends_with("48x64"));
        let lidos = exec::quadros_lidos(&err).map_err(|e| e.to_string())?.get("t0").copied();
        // 12 da sobreposição + 18 do xfade (6 + 12)
        if quadros != 30 || !dimensoes || lidos != Some(10) {
            return Err(format!("autoteste: {quadros} quadros, 48x64 {dimensoes}, fps@t0 {lidos:?}"));
        }
        return Ok(forma);
    }
    Err(ultimo)
}

// ---------------------------------------------------------------- GPU

struct ResultadoGpu {
    gpu: Option<Gpu>,
    reprovados: Vec<Reprovado>,
    /// Falha passageira (sessões, VRAM): não vai para o cache.
    passageiro: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ChaveGpu {
    ffmpeg_sha256: String,
    nvidia_smi: String,
    nos: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntradaGpu {
    chave: ChaveGpu,
    gpu: Option<Gpu>,
    reprovados: Vec<Reprovado>,
}

fn ler_gpu(cache: &Path, chave: &ChaveGpu) -> Option<EntradaGpu> {
    let todas: BTreeMap<String, EntradaGpu> = ler_json(&cache.join("sonda-gpu.json"))?;
    todas.get(&chave.ffmpeg_sha256).filter(|e| &e.chave == chave).cloned()
}

fn chave_gpu(sha: &str) -> ChaveGpu {
    ChaveGpu {
        ffmpeg_sha256: sha.to_string(),
        nvidia_smi: nvidia_smi(),
        nos: nos_de_render().into_iter().map(|(n, d)| format!("{} {d}", n.display())).collect(),
    }
}

/// `driver_version, name` da primeira placa NVIDIA; vazio sem `nvidia-smi`.
fn nvidia_smi() -> String {
    let Ok(prog) = which::which("nvidia-smi") else { return String::new() };
    let Ok(pasta) = PastaTemp::nova("smi") else { return String::new() };
    let cmd = cmd_em(&prog, &pasta.0, "nvidia-smi", vec!["--query-gpu=driver_version,name".into(), "--format=csv,noheader".into()], &[]);
    // o nvidia-smi trava quando a placa cai do barramento
    match saida_limitada(&cmd, LIMITE_SMI) {
        Ok((t, out, false)) if t.codigo == Some(0) => String::from_utf8_lossy(&out).lines().next().unwrap_or("").trim().to_string(),
        _ => String::new(),
    }
}

/// Linux: nós `/dev/dri/renderD*` com o driver do kernel.
fn nos_de_render() -> Vec<(PathBuf, String)> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir("/dev/dri") {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if !n.starts_with("renderD") {
                continue;
            }
            let d = std::fs::read_link(format!("/sys/class/drm/{n}/device/driver"))
                .ok()
                .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "?".into());
            v.push((e.path(), d));
        }
    }
    v.sort();
    v
}

/// Argumentos de uma codificação de teste: `-f lavfi -i <entrada> -frames:v n`, conversão por codificador, `-c:v`.
fn linha_codificar(cod: &str, params: &[impl AsRef<str>], no: Option<&str>, entrada: &str, n: u32, formato: Option<&str>, saida: &str) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-loglevel", "warning"].iter().map(|s| s.to_string()).collect();
    if let Some(no) = no {
        a.extend(["-vaapi_device".into(), no.into()]);
    }
    a.extend(["-f".into(), "lavfi".into(), "-i".into(), entrada.into(), "-frames:v".into(), n.to_string()]);
    if no.is_some() {
        a.extend(["-vf".into(), "format=nv12,hwupload".into()]);
    } else {
        a.extend(["-pix_fmt".into(), if cod.ends_with("_qsv") { "nv12" } else { "yuv420p" }.into()]);
    }
    a.extend(["-c:v".into(), cod.into()]);
    a.extend(params.iter().map(|p| p.as_ref().to_string()));
    if let Some(f) = formato {
        a.extend(["-f".into(), f.into()]);
    }
    a.push(saida.into());
    a
}

/// Sonda de GPU: codificação real de 8 quadros descendo os degraus, classificação do stderr, conferência da saída
/// por PSNR, extras só com NVENC. Para no primeiro aprovado. `ambiente` vai só para os filhos.
fn sondar_gpu(cap: &Capacidades, ordem: &[&str], ambiente: &[(String, String)]) -> ResultadoGpu {
    let mut r = ResultadoGpu { gpu: None, reprovados: Vec::new(), passageiro: false };
    let Ok(pasta) = PastaTemp::nova("gpu") else { return r };
    let p = &pasta.0;
    let reprova = |cod: &str, classe: ClasseReprovacao, dica: String| Reprovado {
        codificador: cod.to_string(),
        classe,
        dica,
        driver_minimo: None,
        api_exigida: None,
        api_do_driver: None,
    };
    for &cod in ordem {
        if !cap.codificadores.contains(cod) {
            r.reprovados.push(reprova(cod, ClasseReprovacao::FfmpegSemCodificador, dica(ClasseReprovacao::FfmpegSemCodificador)));
            continue;
        }
        let alvos: Vec<Option<(PathBuf, String)>> = if cod == "h264_vaapi" {
            nos_de_render()
                .into_iter()
                .filter(|(_, d)| matches!(d.as_str(), "i915" | "xe" | "amdgpu" | "radeon"))
                .map(Some)
                .collect()
        } else {
            vec![None]
        };
        if alvos.is_empty() {
            r.reprovados.push(reprova(cod, ClasseReprovacao::SemGpu, dica(ClasseReprovacao::SemGpu)));
            continue;
        }
        let mut ultima: Option<Reprovado> = None;
        'alvos: for alvo in &alvos {
            let no = alvo.as_ref().map(|(n, _)| n.to_string_lossy().into_owned());
            for (degrau, params) in degraus(cod) {
                let args = linha_codificar(cod, params, no.as_deref(), ENTRADA_GPU, 8, Some("null"), "-");
                let cmd = cmd_sonda(cap, p, &format!("{cod}-{degrau}"), args, ambiente);
                let (t, esgotou) = match rodar_limitado(&cmd) {
                    Ok(x) => x,
                    Err(e) => {
                        ultima = Some(reprova(cod, ClasseReprovacao::Outra, e.to_string()));
                        break 'alvos;
                    }
                };
                if t.codigo == Some(0) && !esgotou {
                    match conferir_saida(cap, p, cod, params, no.as_deref(), ambiente) {
                        Ok(Conferencia::Psnr(psnr)) if psnr >= PSNR_MINIMO => {
                            let (nome, driver) = match (cod, alvo) {
                                ("h264_vaapi", Some((n, d))) => (n.to_string_lossy().into_owned(), d.clone()),
                                ("h264_nvenc", _) => {
                                    let s = nvidia_smi();
                                    let (d, n) = s.split_once(',').unwrap_or(("", ""));
                                    (n.trim().to_string(), d.trim().to_string())
                                }
                                _ => (String::new(), String::new()),
                            };
                            let (nvdec, scale_cuda) = if cod == "h264_nvenc" { extras_nvenc(cap, p, ambiente) } else { (false, false) };
                            r.gpu = Some(Gpu {
                                nome,
                                driver,
                                codificador: cod.to_string(),
                                degrau: degrau.to_string(),
                                args: params.iter().map(|s| s.to_string()).collect(),
                                nvdec,
                                scale_cuda,
                            });
                            return r;
                        }
                        Ok(Conferencia::Psnr(psnr)) => {
                            ultima = Some(reprova(cod, ClasseReprovacao::SaidaInvalida, format!("saída inválida ({psnr:.1} dB)")));
                        }
                        // os 8 quadros passaram e os 30 não: sessão tomada no intervalo, VRAM, tempo
                        Ok(Conferencia::NaoCodificou { stderr, esgotou }) => {
                            let mut rep = classificar_gpu(cod, &stderr);
                            if esgotou {
                                rep.classe = ClasseReprovacao::Outra;
                                rep.dica = "tempo esgotado".into();
                            }
                            r.passageiro |= esgotou || passageiro(&rep, &stderr);
                            ultima = Some(rep);
                        }
                        Err(e) => ultima = Some(reprova(cod, ClasseReprovacao::SaidaInvalida, e)),
                    }
                    // este nó não serve; o seguinte (outra placa) pode servir
                    continue 'alvos;
                }
                let texto = std::fs::read_to_string(&cmd.stderr).unwrap_or_default();
                let mut rep = classificar_gpu(cod, &texto);
                if esgotou {
                    rep.classe = ClasseReprovacao::Outra;
                    rep.dica = "tempo esgotado".into();
                }
                r.passageiro |= esgotou || passageiro(&rep, &texto);
                let parar = esgotou
                    || matches!(
                        rep.classe,
                        ClasseReprovacao::FfmpegSemCodificador
                            | ClasseReprovacao::SemDriver
                            | ClasseReprovacao::DriverAntigo
                            | ClasseReprovacao::SemGpu
                            | ClasseReprovacao::LimiteDeSessoes
                    );
                ultima = Some(rep);
                if parar {
                    break;
                }
            }
        }
        if let Some(u) = ultima {
            r.reprovados.push(u);
        }
    }
    r
}

fn passageiro(rep: &Reprovado, stderr: &str) -> bool {
    rep.classe == ClasseReprovacao::LimiteDeSessoes || stderr.contains("CUDA_ERROR_OUT_OF_MEMORY")
}

enum Conferencia {
    /// PSNR mínimo (dB) dos 30 quadros.
    Psnr(f64),
    /// A codificação dos 30 quadros falhou: o stderr vai para `classificar_gpu`.
    NaoCodificou { stderr: String, esgotou: bool },
}

/// 30 quadros codificados num arquivo, decodificados em CPU e comparados com a fonte: PSNR mínimo (dB).
/// `Err` = saída inválida: não decodifica, tamanho errado, quadros de menos ou de mais.
fn conferir_saida(
    cap: &Capacidades,
    pasta: &Path,
    cod: &str,
    params: &[impl AsRef<str>],
    no: Option<&str>,
    ambiente: &[(String, String)],
) -> std::result::Result<Conferencia, String> {
    let arq = format!("conf-{cod}.mp4");
    let estat = format!("conf-{cod}.psnr");
    let _ = std::fs::remove_file(pasta.join(&arq));
    let _ = std::fs::remove_file(pasta.join(&estat));
    let args = linha_codificar(cod, params, no, ENTRADA_GPU, QUADROS_CONFERENCIA, None, &arq);
    let cmd = cmd_sonda(cap, pasta, &format!("{cod}-conf"), args, ambiente);
    match rodar_limitado(&cmd) {
        Ok((t, false)) if t.codigo == Some(0) => {}
        Ok((_, esgotou)) => {
            return Ok(Conferencia::NaoCodificou { stderr: std::fs::read_to_string(&cmd.stderr).unwrap_or_default(), esgotou });
        }
        Err(e) => return Err(e.to_string()),
    }
    // +1 quadro na referência: quadro a mais na saída também aparece na contagem
    let referencia = format!("testsrc2=s=1920x1080:r=30,trim=end_frame={}", QUADROS_CONFERENCIA + 1);
    let grafo = format!("[0:v]crop=1920:1080:0:0,format=yuv420p[a];[1:v]format=yuv420p[b];[a][b]psnr=shortest=1:stats_file={estat}");
    let args: Vec<String> = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "info",
        "-i",
        &arq,
        "-f",
        "lavfi",
        "-i",
        &referencia,
        "-lavfi",
        &grafo,
        "-f",
        "null",
        "-",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let cmd = cmd_sonda(cap, pasta, &format!("{cod}-psnr"), args, ambiente);
    if !matches!(rodar_limitado(&cmd), Ok((t, false)) if t.codigo == Some(0)) {
        return Err(format!("saída não decodifica: {}", exec::ultimas_linhas(&cmd.stderr, 2).join(" | ")));
    }
    // uma linha `n:<k> …` por quadro comparado
    let comparados = std::fs::read_to_string(pasta.join(&estat)).unwrap_or_default().lines().filter(|l| l.starts_with("n:")).count();
    if comparados != QUADROS_CONFERENCIA as usize {
        return Err(format!("saída com {comparados} quadros de {QUADROS_CONFERENCIA}"));
    }
    let texto = std::fs::read_to_string(&cmd.stderr).unwrap_or_default();
    psnr_minimo(&texto).map(Conferencia::Psnr).ok_or_else(|| "sem PSNR".to_string())
}

/// `PSNR y:… average:… min:<x> max:…` → x (`inf` = idêntico).
fn psnr_minimo(stderr: &str) -> Option<f64> {
    let l = stderr.lines().rev().find(|l| l.contains("PSNR y:"))?;
    let v = l.split("min:").nth(1)?.split_whitespace().next()?;
    if v.starts_with("inf") {
        Some(f64::INFINITY)
    } else {
        v.parse().ok()
    }
}

/// NVDEC e `scale_cuda` + NVENC, cada um com código 0 e stderr vazio (o `-hwaccel cuda` cai para CPU calado).
fn extras_nvenc(cap: &Capacidades, pasta: &Path, ambiente: &[(String, String)]) -> (bool, bool) {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let amostra = s(&[
        "-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i", ENTRADA_GPU, "-frames:v", "30", "-c:v", "libx264",
        "-preset", "ultrafast", "-pix_fmt", "yuv420p", "amostra.mp4",
    ]);
    if !matches!(rodar_limitado(&cmd_sonda(cap, pasta, "amostra", amostra, ambiente)), Ok((t, false)) if t.codigo == Some(0)) {
        return (false, false);
    }
    let limpo = |nome: &str, args: Vec<String>| {
        let cmd = cmd_sonda(cap, pasta, nome, args, ambiente);
        matches!(rodar_limitado(&cmd), Ok((t, false)) if t.codigo == Some(0))
            && std::fs::read_to_string(&cmd.stderr).map(|e| e.trim().is_empty()).unwrap_or(false)
    };
    let nvdec = cap.hwaccels.contains("cuda")
        && limpo("nvdec", s(&["-hide_banner", "-nostdin", "-loglevel", "error", "-hwaccel", "cuda", "-i", "amostra.mp4", "-f", "null", "-"]));
    let scale_cuda = nvdec
        && cap.filtros.contains("scale_cuda")
        && limpo(
            "scale_cuda",
            s(&[
                "-hide_banner", "-nostdin", "-loglevel", "error", "-hwaccel", "cuda", "-hwaccel_output_format", "cuda", "-i", "amostra.mp4",
                "-vf", "scale_cuda=1280:720", "-c:v", "h264_nvenc", "-f", "null", "-",
            ]),
        );
    (nvdec, scale_cuda)
}

fn dica(c: ClasseReprovacao) -> String {
    match c {
        ClasseReprovacao::SemGpu => "nenhuma placa compatível",
        ClasseReprovacao::SemDriver => "driver do fabricante ausente",
        ClasseReprovacao::DriverAntigo => "driver mais antigo que o exigido",
        ClasseReprovacao::LimiteDeSessoes => "sessões do codificador ocupadas",
        ClasseReprovacao::RecursoNaoSuportado => "a placa recusou os parâmetros",
        ClasseReprovacao::FfmpegSemCodificador => "ffmpeg sem este codificador",
        ClasseReprovacao::SaidaInvalida => "saída inválida",
        ClasseReprovacao::Outra => "falha não classificada",
    }
    .to_string()
}

/// Classe da falha de um codificador de GPU pelo stderr (`sonda_gpu.py`, `FALHAS`; a primeira que casa vence).
fn classificar_gpu(cod: &str, stderr: &str) -> Reprovado {
    let tem = |ps: &[&str]| ps.iter().any(|p| stderr.contains(p));
    let mut detalhe = (None, None, None);
    let (classe, dica_txt) = if tem(&["Unknown encoder", "Encoder not found"]) {
        (ClasseReprovacao::FfmpegSemCodificador, None)
    } else if tem(&["Unrecognized option", "Option not found", "Unable to parse option value", "Error setting option"]) {
        (ClasseReprovacao::Outra, Some("ffmpeg não reconhece uma opção"))
    } else if tem(&["Driver does not support the required nvenc API version"]) {
        detalhe = (
            depois_de(stderr, "minimum required Nvidia driver for nvenc is "),
            depois_de(stderr, "Required: "),
            depois_de(stderr, "Found: "),
        );
        (ClasseReprovacao::DriverAntigo, None)
    } else if tem(&[
        "Cannot load libnvidia-encode.so.1",
        "Cannot load nvEncodeAPI64.dll",
        "Cannot load nvEncodeAPI.dll",
        "Cannot load libcuda.so.1",
        "Cannot load nvcuda.dll",
        "Error creating a MFX session",
        "Error initializing an MFX session",
        "Failed to initialise VAAPI connection",
        "No VA display found",
        "Cannot open a VA display",
        "Cannot open VA display",
        "Failed to open the DRM device",
    ]) || (stderr.contains("DLL ") && stderr.contains("amfrt") && stderr.contains("failed to open"))
    {
        (ClasseReprovacao::SemDriver, None)
    } else if tem(&[
        "OpenEncodeSessionEx failed: out of memory",
        "OpenEncodeSessionEx failed: incompatible client key",
        "No free encoding sessions",
        "out of memory (10)",
    ]) {
        (ClasseReprovacao::LimiteDeSessoes, None)
    } else if tem(&["CUDA_ERROR_NO_DEVICE", "No CUDA capable devices found", "No NVENC capable devices found"])
        || (stderr.contains("Requested GPU ") && stderr.contains(", but only"))
    {
        (ClasseReprovacao::SemGpu, None)
    } else if tem(&[
        "No capable devices found",
        "not supported",
        "InitializeEncoder failed",
        "No usable encoding entrypoint",
        "No usable encoding profile",
        "Driver does not support",
        "No supported RC mode",
        "Function not implemented",
    ]) || excede_numero(stderr)
    {
        (ClasseReprovacao::RecursoNaoSuportado, None)
    } else {
        (ClasseReprovacao::Outra, None)
    };
    Reprovado {
        codificador: cod.to_string(),
        classe,
        dica: dica_txt.map(String::from).unwrap_or_else(|| dica(classe)),
        driver_minimo: detalhe.0,
        api_exigida: detalhe.1,
        api_do_driver: detalhe.2,
    }
}

/// `exceed 7`, `exceeds 4096`.
fn excede_numero(s: &str) -> bool {
    s.match_indices("exceed").any(|(i, _)| {
        let r = &s[i + 6..];
        let r = r.strip_prefix('s').unwrap_or(r);
        r.strip_prefix(' ').and_then(|x| x.chars().next()).is_some_and(|c| c.is_ascii_digit())
    })
}

fn depois_de(s: &str, marca: &str) -> Option<String> {
    let i = s.find(marca)?;
    s[i + marca.len()..].split_whitespace().next().map(String::from)
}

// ---------------------------------------------------------------- JSON

fn ler_json<T: serde::de::DeserializeOwned>(p: &Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(p).ok()?).ok()
}

/// Grava por arquivo temporário + renomear: duas instâncias não deixam JSON pela metade.
fn gravar_json<T: Serialize>(p: &Path, v: &T) -> Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, serde_json::to_vec_pretty(v)?)?;
    std::fs::rename(&tmp, p)?;
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::testes::fontes;

    #[test]
    fn sicro_ffmpeg_vence_a_ordem() {
        let d = tempfile::tempdir().unwrap();
        let (a, b, c) = (d.path().join("a"), d.path().join("b"), d.path().join("c"));
        for p in [&a, &b, &c] {
            std::fs::write(p, "x").unwrap();
        }
        let var = Some(OsString::from(&a));
        assert_eq!(escolher_com(var.clone(), None, Some(b.clone()), Some(c.clone())).unwrap(), (a.clone(), Origem::Configurado));
        assert_eq!(escolher_com(None, None, Some(b.clone()), Some(c.clone())).unwrap(), (b.clone(), Origem::Embutido));
        assert_eq!(escolher_com(Some(OsString::new()), None, None, Some(c.clone())).unwrap(), (c.clone(), Origem::Path));
        // definido e ausente: erro, nunca cai para o embutido nem para o PATH
        assert!(escolher_com(Some(d.path().join("nao").into()), None, Some(b.clone()), Some(c.clone())).is_err());
        assert!(escolher_com(None, None, None, None).is_err());

        // o campo das Configurações: depois de `SICRO_FFMPEG`, antes do embutido; ausente é erro com o caminho
        let campo = d.path().join("campo");
        std::fs::write(&campo, "x").unwrap();
        assert_eq!(escolher_com(None, Some(campo.clone()), Some(b.clone()), Some(c.clone())).unwrap(), (campo.clone(), Origem::Configurado));
        assert_eq!(escolher_com(var.clone(), Some(campo.clone()), Some(b.clone()), None).unwrap(), (a.clone(), Origem::Configurado));
        assert_eq!(escolher_com(None, Some(PathBuf::new()), Some(b.clone()), None).unwrap(), (b.clone(), Origem::Embutido));
        let sumido = d.path().join("sumido");
        match escolher_com(None, Some(sumido.clone()), Some(b.clone()), Some(c.clone())) {
            Err(SicroError::Validation(m)) => assert_eq!(m, format!("ffmpeg_ausente: {}", sumido.display())),
            outro => panic!("{outro:?}"),
        }
        // `SICRO_FFMPEG` ausente vence o campo presente (erro, não o campo)
        assert!(escolher_com(Some(d.path().join("nao").into()), Some(campo), Some(b), Some(c)).is_err());
    }

    #[test]
    fn capacidades_usa_o_binario_dos_testes() {
        let Some(cap) = fontes::cap() else { return };
        let c = capacidades().unwrap();
        assert_eq!((c.binario.clone(), c.origem, c.versao.clone()), (cap.binario.clone(), cap.origem, cap.versao.clone()));
        let r = resumo(&c);
        assert_eq!(r.versao, cap.versao);
        assert!(r.faltam.is_empty() && r.opcionais_ausentes.is_empty(), "{r:?}");
    }

    #[test]
    fn le_as_listas() {
        let f = " TS aap               AA->A      Apply Affine.\n .. abench            A->A       Bench.\n  T.. = Timeline support\n TSC alphamerge  VV->V  Copy.\n | = Source\n";
        assert_eq!(ler_filtros(f), ["aap", "abench", "alphamerge"].map(String::from).into());
        let e = " V..... = Video\n ------\n V....D libx264  H.264\n A....D aac   AAC\n";
        assert_eq!(ler_codecs(e), ["libx264", "aac"].map(String::from).into());
        let h = "Hardware acceleration methods:\nvdpau\ncuda\n\n";
        assert_eq!(ler_hwaccels(h), ["vdpau", "cuda"].map(String::from).into());
        let m = "Formats:\n D.. = Demuxing supported\n ---\n  E  3g2   3GP2\n  E  matroska,webm  MKV\n Ed alsa  ALSA\n";
        assert_eq!(ler_muxers(m), ["3g2", "matroska", "webm", "alsa"].map(String::from).into());
        assert_eq!(ler_buildconf("\n  configuration:\n    --a\n    --b\n"), "--a --b");
        assert_eq!(psnr_minimo("[Parsed_psnr_2 @ 0x1] PSNR y:49.9 u:51 v:52 average:50.12 min:48.90 max:51.2"), Some(48.9));
        assert_eq!(psnr_minimo("PSNR y:inf u:inf v:inf average:inf min:inf max:inf"), Some(f64::INFINITY));
    }

    /// Listas reais, obrigatórios, autoteste (giro, `.ffconcat`, `fps@` em verbose) e o que distingue os dois binários.
    #[test]
    fn listas_e_autoteste_do_binario() {
        let Some(cap) = fontes::cap() else { return };
        let t0 = Instant::now();
        let c = sondar_listas(&cap.binario, cap.origem, &cap.sha256).unwrap();
        eprintln!("sonda de listas e autoteste ({}): {:.2} s", c.versao, t0.elapsed().as_secs_f64());
        assert!(c.faltam.is_empty(), "{:?} ({})", c.faltam, c.versao);
        // medido (CI do P0.10, 6.1 da Ubuntu 24.04): antes do 7 só há `-filter_complex_script`
        let maior: Option<u32> = c.versao.trim_start_matches('n').split('.').next().and_then(|v| v.parse().ok());
        if let Some(m) = maior {
            let g = if m >= 7 { GrafoArquivo::BarraFilterComplex } else { GrafoArquivo::FilterComplexScript };
            assert_eq!(c.grafo_em_arquivo, g, "{}", c.versao);
        }
        for f in FILTROS_OBRIGATORIOS.iter().chain(FILTROS_OPCIONAIS) {
            assert!(c.filtros.contains(*f), "{f} ({})", c.versao);
        }
        assert!(c.configuracao.contains("--enable-libx264"));
        assert!(c.ffprobe.is_file());
        match c.versao.as_str() {
            // mapa-infra.md, achados 1 e 2; p1c/out/sonda-appimage-7.0.2.txt
            "7.0.2-static" => {
                assert!(!c.filtros.contains("drawtext"));
                assert_eq!(c.hwaccels, ["vdpau".to_string()].into());
                for e in ["h264_nvenc", "h264_qsv", "h264_vaapi"] {
                    assert!(!c.codificadores.contains(e));
                }
            }
            "n9.0.1" => {
                assert!(c.filtros.contains("drawtext") && c.filtros.contains("scale_cuda"));
                assert!(c.hwaccels.contains("cuda") && c.hwaccels.contains("vaapi"));
                for e in ["h264_nvenc", "h264_qsv", "h264_vaapi"] {
                    assert!(c.codificadores.contains(e));
                }
            }
            v => eprintln!("versão sem referência medida: {v}"),
        }
    }

    /// T2 (`a3/out/t2-grafo-em-arquivo.log`): `-/filter_complex` nos dois; `-filter_complex_script` só no 7.0.2;
    /// BOM e comentário quebram o grafo.
    #[test]
    fn forma_do_grafo_em_arquivo_t2() {
        let Some(cap) = fontes::cap() else { return };
        let p = fontes::pasta_temporaria("t2");
        let g = "color=c=red:s=16x16:d=0.1,format=yuv420p[v]";
        std::fs::write(p.join("g.txt"), g).unwrap();
        std::fs::write(p.join("bom.txt"), format!("\u{feff}{g}")).unwrap();
        std::fs::write(p.join("com.txt"), format!("# comentario\n{g}")).unwrap();
        let roda = |opcao: &str, arq: &str| {
            let a = ["-hide_banner", "-nostdin", opcao, arq, "-map", "[v]", "-f", "null", "-"].iter().map(|s| s.to_string()).collect();
            exec::rodar(&cmd_em(&cap.binario, &p, "t2", a, &[]), &AtomicBool::new(false)).unwrap().codigo
        };
        let forma = &grafo_em_arquivo_args(&cap);
        assert_eq!(roda(forma, "g.txt"), Some(0));
        assert_ne!(roda(forma, "bom.txt"), Some(0));
        assert_ne!(roda(forma, "com.txt"), Some(0));
        let script = roda("-filter_complex_script", "g.txt");
        match cap.versao.as_str() {
            "7.0.2-static" => assert_eq!(script, Some(0)),
            "n9.0.1" => assert_ne!(script, Some(0)),
            _ => {}
        }
    }

    /// Imita o ffmpeg da distro sobre o binário dos testes: recusa `-/filter_complex` como o 6.1 (Ubuntu 24.04) e, com
    /// `sem_fps_mode`, também `-fps_mode` como o 4.4 (22.04). O `-filter_complex_script` chega ao binário de verdade
    /// como `-/filter_complex` (o 9.0.1 não tem mais a forma antiga).
    #[cfg(unix)]
    fn distro(nome: &str, cap: &Capacidades, sem_fps_mode: bool) -> PathBuf {
        let recusa = if sem_fps_mode { "-/filter_complex|-fps_mode" } else { "-/filter_complex" };
        let corpo = format!(
            r#"a=()
for x in "$@"; do
  case "$x" in
    {recusa}) echo "Unrecognized option '${{x#-}}'." >&2; echo "Error splitting the argument list: Option not found" >&2; exit 8;;
    -filter_complex_script) a+=("-/filter_complex");;
    *) a+=("$x");;
  esac
done
set -- "${{a[@]}}""#
        );
        falso(nome, cap, &corpo)
    }

    /// Sem `-/filter_complex` (6.1), a sonda escolhe o `-filter_complex_script` e o comando montado por `args` roda e
    /// dá os quadros do binário de verdade; a forma nova, nesse binário, é recusada. Sem `-fps_mode` (4.4), que toda
    /// exportação usa, o autoteste reprova.
    #[cfg(unix)]
    #[test]
    fn ffmpeg_da_distro_usa_o_script_ou_e_reprovado() {
        let Some(cap) = fontes::cap() else { return };
        if cap.grafo_em_arquivo != GrafoArquivo::BarraFilterComplex {
            eprintln!("dispensado ({}): o binário já é anterior ao 7 (ver listas_e_autoteste_do_binario)", cap.versao);
            return;
        }
        let f = distro("distro-6", &cap, false);
        let seis = sondar_listas(&f, Origem::Configurado, &sha256_arquivo(&f).unwrap()).unwrap();
        assert!(seis.faltam.is_empty(), "{:?}", seis.faltam);
        assert_eq!(seis.grafo_em_arquivo, GrafoArquivo::FilterComplexScript);
        let p = fontes::pasta_temporaria("distro");
        std::fs::write(p.join("g.txt"), "testsrc2=s=64x48:r=30:d=0.5,format=yuv420p[v]").unwrap();
        let rodar = |c: &Capacidades| {
            let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error"].map(String::from).into();
            a.extend(super::super::args::grafo_em_arquivo(c, "g.txt"));
            a.extend(["-map", "[v]", "-fps_mode", "passthrough", "-f", "framehash", "-hash", "sha256", "-"].map(String::from));
            let (t, out) = exec::rodar_com_saida(&cmd_em(&c.binario, &p, "distro", a, &[]), &AtomicBool::new(false)).unwrap();
            (t.codigo, String::from_utf8_lossy(&out).lines().filter(|l| !l.starts_with('#')).map(String::from).collect::<Vec<_>>())
        };
        let (no_seis, de_verdade) = (rodar(&seis), rodar(&cap));
        assert_eq!((no_seis.0, de_verdade.0), (Some(0), Some(0)));
        assert_eq!((no_seis.1.len(), &no_seis.1), (15, &de_verdade.1));
        assert_ne!(rodar(&Capacidades { binario: f, ..(*cap).clone() }).0, Some(0), "o falso tem de recusar a forma nova");
        let f = distro("distro-4", &cap, true);
        assert_eq!(sondar_listas(&f, Origem::Configurado, &sha256_arquivo(&f).unwrap()).unwrap().faltam, ["autoteste"]);
    }

    /// CI da Ubuntu 22.04 (`SICRO_FFMPEG_ABAIXO_DO_MINIMO=1`): o 4.4 da distro não tem o que a exportação usa
    /// (`-fps_mode`, o `setts`, `loudnorm` com `LRA` 50) e a sonda o reprova pelo autoteste.
    #[test]
    fn ffmpeg_abaixo_do_minimo_e_reprovado() {
        if !fontes::abaixo_do_minimo() {
            return;
        }
        let (bin, origem) = escolher().unwrap();
        let c = sondar_listas(&bin, origem, &sha256_arquivo(&bin).unwrap()).unwrap();
        eprintln!("reprovado ({}): faltam {:?}", c.versao, c.faltam);
        assert_eq!(c.faltam, ["autoteste"], "{}", c.versao);
    }

    fn grafo_em_arquivo_args(cap: &Capacidades) -> String {
        super::super::args::grafo_em_arquivo(cap, "g.txt").remove(0)
    }

    /// Script que finge ser o ffmpeg: `corpo` roda antes do `exec` do verdadeiro; `ffprobe` ao lado.
    #[cfg(unix)]
    fn falso(nome: &str, cap: &Capacidades, corpo: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let d = fontes::pasta_temporaria(nome);
        let f = d.join("ffmpeg");
        // caminho com apóstrofo: fecha, escapa e reabre as aspas do bash
        let real = cap.binario.to_string_lossy().replace('\'', "'\\''");
        std::fs::write(&f, format!("#!/bin/bash\nREAL='{real}'\n{corpo}\nexec \"$REAL\" \"$@\"\n")).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(&cap.ffprobe, d.join("ffprobe")).unwrap();
        f
    }

    #[cfg(unix)]
    #[test]
    fn binario_sem_filtro_obrigatorio_da_erro_com_a_lista() {
        let Some(cap) = fontes::cap() else { return };
        let f = falso(
            "sem-alphamerge",
            &cap,
            r#"for a in "$@"; do if [ "$a" = "-filters" ]; then "$REAL" "$@" | grep -v ' alphamerge '; exit ${PIPESTATUS[0]}; fi; done"#,
        );
        let c = capacidades_de(&f, Origem::Configurado, None, false).unwrap();
        assert_eq!(c.faltam, ["alphamerge"], "{}", cap.versao);
        let e = exigir(&c).unwrap_err().to_string();
        assert!(e.contains("filtro_ausente") && e.contains("alphamerge"), "{e}");
        // do PATH com falta: devolve as capacidades (a validação acusa `filtro_ausente` por efeito)
        let c = capacidades_de(&f, Origem::Path, None, false).unwrap();
        assert!(exigir(&c).is_ok());
        estado().por_binario.remove(&f);
    }

    /// O codificador "de GPU" do falso devolve ruído com código 0: a conferência por PSNR reprova.
    #[cfg(unix)]
    #[test]
    fn codificador_que_devolve_lixo_com_codigo_zero_e_reprovado() {
        let Some(cap) = fontes::cap() else { return };
        let f = falso(
            "lixo",
            &cap,
            r#"lixo=0; n=8; ant=""
for a in "$@"; do [ "$a" = "h264_nvenc" ] && lixo=1; [ "$ant" = "-frames:v" ] && n="$a"; ant="$a"; done
for a in "$@"; do if [ "$a" = "-encoders" ]; then "$REAL" "$@" | grep -v ' h264_nvenc '; echo " V....D h264_nvenc           NVIDIA NVENC H.264 encoder (codec h264)"; exit 0; fi; done
if [ $lixo = 1 ]; then
  out="${@: -1}"
  [ "$out" = "-" ] && exit 0
  exec "$REAL" -hide_banner -nostdin -loglevel error -y -f lavfi -i "nullsrc=s=1920x1080:r=30,geq=lum='random(1)*255':cb=128:cr=128" -frames:v "$n" -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$out"
fi"#,
        );
        let c = capacidades_de(&f, Origem::Path, None, false).unwrap();
        assert!(c.codificadores.contains("h264_nvenc"));
        let r = sondar_gpu(&c, &["h264_nvenc"], &[]);
        assert!(r.gpu.is_none());
        assert_eq!(r.reprovados.len(), 1);
        assert_eq!((r.reprovados[0].codificador.as_str(), r.reprovados[0].classe), ("h264_nvenc", ClasseReprovacao::SaidaInvalida));
        eprintln!("{} ({})", r.reprovados[0].dica, cap.versao);
        // controle: o libx264 de verdade passa na mesma conferência
        let p = PastaTemp::nova("controle").unwrap();
        let Ok(Conferencia::Psnr(psnr)) = conferir_saida(&cap, &p.0, "libx264", &["-preset", "medium", "-crf", "18"], None, &[]) else {
            panic!("o libx264 não passou na conferência ({})", cap.versao)
        };
        assert!(psnr >= PSNR_MINIMO, "{psnr}");
        estado().por_binario.remove(&f);
    }

    /// Falso cujo `h264_nvenc` passa nos 8 quadros (`-f null -`) e, com saída em arquivo, roda `gerar` (bash; `$out`, `$n`, `$REAL`).
    #[cfg(unix)]
    fn falso_nvenc(nome: &str, cap: &Capacidades, gerar: &str) -> PathBuf {
        let corpo = format!(
            r#"lixo=0; n=8; ant=""
for a in "$@"; do [ "$a" = "h264_nvenc" ] && lixo=1; [ "$ant" = "-frames:v" ] && n="$a"; ant="$a"; done
for a in "$@"; do if [ "$a" = "-encoders" ]; then "$REAL" "$@" | grep -v ' h264_nvenc '; echo " V....D h264_nvenc           NVIDIA NVENC H.264 encoder (codec h264)"; exit 0; fi; done
if [ $lixo = 1 ]; then
  out="${{@: -1}}"
  [ "$out" = "-" ] && exit 0
  {gerar}
fi"#
        );
        falso(nome, cap, &corpo)
    }

    /// Outras formas de lixo com código 0: quadros de menos, de mais, arquivo vazio, bytes soltos, tamanho errado.
    #[cfg(unix)]
    #[test]
    fn saida_truncada_vazia_ou_de_outro_tamanho_e_reprovada() {
        let Some(cap) = fontes::cap() else { return };
        let x264 = |quadros: &str, tamanho: &str| {
            format!(
                r#"exec "$REAL" -hide_banner -nostdin -loglevel error -y -f lavfi -i "testsrc2=s={tamanho}:r=30" -frames:v {quadros} -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$out""#
            )
        };
        let casos = [
            ("dois-quadros", x264("2", "1920x1080"), "2 quadros"),
            ("um-a-mais", x264("31", "1920x1080"), "31 quadros"),
            ("vazio", r#": > "$out"; exit 0"#.to_string(), "não decodifica"),
            ("bytes", r#"head -c 200000 /dev/urandom > "$out"; exit 0"#.to_string(), "não decodifica"),
            ("menor", x264("$n", "1280x720"), "não decodifica"),
        ];
        for (nome, gerar, motivo) in casos {
            let f = falso_nvenc(&format!("lixo-{nome}"), &cap, &gerar);
            let c = capacidades_de(&f, Origem::Path, None, false).unwrap();
            let r = sondar_gpu(&c, &["h264_nvenc"], &[]);
            estado().por_binario.remove(&f);
            assert!(r.gpu.is_none(), "{nome} ({})", cap.versao);
            let rep = &r.reprovados[0];
            assert_eq!(rep.classe, ClasseReprovacao::SaidaInvalida, "{nome}: {rep:?}");
            assert!(rep.dica.contains(motivo), "{nome}: {} ({})", rep.dica, cap.versao);
            assert!(!r.passageiro, "{nome}");
        }
        // controle: o mesmo falso devolvendo os 30 quadros certos é aprovado
        let f = falso_nvenc("lixo-controle", &cap, &x264("$n", "1920x1080"));
        let c = capacidades_de(&f, Origem::Path, None, false).unwrap();
        let r = sondar_gpu(&c, &["h264_nvenc"], &[]);
        estado().por_binario.remove(&f);
        // (os extras do NVENC rodam no ffmpeg de verdade e não entram na conta)
        assert_eq!(r.gpu.as_ref().map(|g| g.degrau.as_str()), Some("completo"), "{:?} ({})", r.reprovados, cap.versao);
    }

    /// A sonda cortada pela saída do app, num processo auxiliar (o `matar_todos` mataria o ffmpeg dos vizinhos): antes
    /// dele a reprovação vai para o cache (controle); depois, não.
    #[test]
    #[ignore]
    fn auxiliar_sonda_cortada_na_saida() {
        let Ok(pasta) = std::env::var("SICRO_TESTE_SONDA_SAIDA") else { return };
        let Some(cap) = fontes::cap() else { return };
        let cache = PathBuf::from(pasta);
        let reprovada = || ResultadoGpu {
            gpu: None,
            reprovados: vec![Reprovado { codificador: "h264_nvenc".into(), classe: ClasseReprovacao::Outra, dica: String::new(), driver_minimo: None, api_exigida: None, api_do_driver: None }],
            passageiro: false,
        };
        let arquivo = cache.join("sonda-gpu.json");
        guardar_gpu(&cap, reprovada(), Some(&cache));
        assert!(arquivo.exists(), "controle: fora da saída a sonda grava");
        std::fs::remove_file(&arquivo).unwrap();
        assert!(!exec::encerrando());
        exec::matar_todos();
        assert!(exec::encerrando());
        guardar_gpu(&cap, reprovada(), Some(&cache));
        assert!(!arquivo.exists(), "sonda cortada pela saída gravou no cache");
        std::fs::write(cache.join("ok"), "ok").unwrap();
    }

    #[test]
    fn sonda_cortada_pela_saida_do_app_nao_vai_para_o_cache() {
        let Some(_cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("sonda-saida");
        let st = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "edicao::ffmpeg::sonda::testes::auxiliar_sonda_cortada_na_saida", "--ignored", "--nocapture"])
            .env("SICRO_TESTE_SONDA_SAIDA", &d)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(st.success());
        assert!(d.join("ok").exists());
    }

    /// Sessão tomada entre os 8 quadros e os 30: classe do stderr, passageiro, nada no cache.
    #[cfg(unix)]
    #[test]
    fn falha_passageira_na_conferencia_nao_vai_para_o_cache() {
        let Some(cap) = fontes::cap() else { return };
        let f = falso_nvenc(
            "sessoes",
            &cap,
            r#"echo "[h264_nvenc @ 0x5] OpenEncodeSessionEx failed: out of memory (10): (no details)" >&2; exit 187"#,
        );
        let cache = fontes::pasta_temporaria("cache-passageiro");
        let c = capacidades_de(&f, Origem::Path, Some(&cache), false).unwrap();
        let r = sondar_gpu(&c, &["h264_nvenc"], &[]);
        assert!(r.gpu.is_none() && r.passageiro);
        assert_eq!(r.reprovados[0].classe, ClasseReprovacao::LimiteDeSessoes, "{:?}", r.reprovados);
        let nova = guardar_gpu(&c, r, Some(&cache));
        assert_eq!(nova.reprovados.len(), 1);
        assert!(!cache.join("sonda-gpu.json").exists());
        estado().por_binario.remove(&f);
    }

    /// Binário trocado no mesmo caminho: a memória não serve (tamanho, data) e o disco também não (sha256).
    #[cfg(unix)]
    #[test]
    fn cache_invalidado_quando_o_binario_muda() {
        let Some(cap) = fontes::cap() else { return };
        let sem = r#"for a in "$@"; do [ "$a" = "s.framehash" ] && exit 1; done
for a in "$@"; do if [ "$a" = "-filters" ]; then "$REAL" "$@" | grep -v ' xfade '; exit ${PIPESTATUS[0]}; fi; done"#;
        let f = falso("troca", &cap, sem);
        let cache = fontes::pasta_temporaria("cache-troca");
        let c1 = capacidades_de(&f, Origem::Path, Some(&cache), false).unwrap();
        // autoteste falho: fica na memória, não no disco
        assert_eq!(c1.faltam, ["xfade", "autoteste"], "{}", cap.versao);
        assert!(!cache.join(format!("sonda-{}.json", c1.sha256)).exists());

        // mesmo caminho, mesmo tamanho, outro conteúdo
        let texto = std::fs::read_to_string(&f).unwrap();
        std::fs::write(&f, texto.replace("' xfade '", "' xfadX '").replace("s.framehash", "s.framehasX")).unwrap();
        let c2 = capacidades_de(&f, Origem::Path, Some(&cache), false).unwrap();
        assert_ne!(c1.sha256, c2.sha256);
        assert!(c2.faltam.is_empty(), "{:?}", c2.faltam);
        assert!(cache.join(format!("sonda-{}.json", c2.sha256)).is_file());

        // `faltam` gravado no disco não manda: é refeito das listas e do ffprobe ao ler
        let arq = cache.join(format!("sonda-{}.json", c2.sha256));
        let mut j: serde_json::Value = serde_json::from_slice(&std::fs::read(&arq).unwrap()).unwrap();
        j["faltam"] = serde_json::json!(["ffprobe", "autoteste"]);
        j["filtros"].as_array_mut().unwrap().retain(|x| x != "tpad");
        std::fs::write(&arq, serde_json::to_vec(&j).unwrap()).unwrap();
        estado().por_binario.remove(&f);
        let c3 = capacidades_de(&f, Origem::Path, Some(&cache), false).unwrap();
        assert_eq!(c3.faltam, ["tpad"]);
        std::fs::remove_file(f.with_file_name("ffprobe")).unwrap();
        estado().por_binario.remove(&f);
        let c4 = capacidades_de(&f, Origem::Path, Some(&cache), false).unwrap();
        assert_eq!(c4.faltam, ["tpad", "ffprobe"]);
        estado().por_binario.remove(&f);
    }

    /// Programa que não responde não trava a sonda: tempo máximo e erro.
    #[cfg(unix)]
    #[test]
    fn programa_que_nao_responde_tem_tempo_maximo() {
        let d = fontes::pasta_temporaria("mudo");
        let cmd = cmd_em(Path::new("/bin/sleep"), &d, "mudo", vec!["600".into()], &[]);
        let t0 = Instant::now();
        let (t, out, esgotou) = saida_limitada(&cmd, Duration::from_millis(300)).unwrap();
        assert!(esgotou && t.cancelado && out.is_empty());
        assert!(t0.elapsed() < Duration::from_secs(5), "{:?}", t0.elapsed());
    }

    /// Primeiras linhas reais de `receitas-gpu-ffmpeg.md` §1.3 (driver antigo: texto de `nvenc.c:306-312`, não provocável aqui).
    const STDERR_REAIS: &[(&str, ClasseReprovacao)] = &[
        ("[h264_nvenc @ 0x55b5f851e540] dl_fn->cuda_dl->cuInit(0) failed -> CUDA_ERROR_NO_DEVICE: no CUDA-capable device is detected", ClasseReprovacao::SemGpu),
        ("[h264_nvenc @ 0x5] No capable devices found", ClasseReprovacao::RecursoNaoSuportado),
        ("[h264_nvenc @ 0x5] Cannot load libnvidia-encode.so.1\n[h264_nvenc @ 0x5] The minimum required Nvidia driver for nvenc is 610.00 or newer", ClasseReprovacao::SemDriver),
        ("[h264_nvenc @ 0x5] Cannot load libcuda.so.1", ClasseReprovacao::SemDriver),
        ("[h264_nvenc @ 0x5] Max B-frames 8 exceed 7\n[h264_nvenc @ 0x5] No capable devices found", ClasseReprovacao::RecursoNaoSuportado),
        ("[h264_nvenc @ 0x5] Width 7680 exceeds 4096\n[h264_nvenc @ 0x5] No capable devices found", ClasseReprovacao::RecursoNaoSuportado),
        ("[h264_nvenc @ 0x5] OpenEncodeSessionEx failed: incompatible client key (21): (no details)\n[h264_nvenc @ 0x5] No capable devices found", ClasseReprovacao::LimiteDeSessoes),
        ("[h264_amf @ 0x5] DLL libamfrt64.so.1 failed to open", ClasseReprovacao::SemDriver),
        ("[h264_qsv @ 0x557ce7309580] Error creating a MFX session: -9.", ClasseReprovacao::SemDriver),
        ("[h264_vaapi @ 0x5] No usable encoding entrypoint found for profile VAProfileH264High (7).", ClasseReprovacao::RecursoNaoSuportado),
        ("[h264_vaapi @ 0x5] Driver does not support ICQ RC mode (supported modes: CQP, CBR, VBR, QVBR).", ClasseReprovacao::RecursoNaoSuportado),
        ("[h264_nvenc @ 0x5] InitializeEncoder failed: invalid param (8): Frame Dimension less than the minimum supported value.", ClasseReprovacao::RecursoNaoSuportado),
        ("[vost#0:0 @ 0x5] Unknown encoder 'h264_nvenc'", ClasseReprovacao::FfmpegSemCodificador),
    ];

    fn classe_do_prototipo(c: &str) -> ClasseReprovacao {
        serde_json::from_value(serde_json::Value::String(c.into())).unwrap_or(ClasseReprovacao::Outra)
    }

    #[test]
    fn classifica_stderr_real_de_gpu() {
        for (e, c) in STDERR_REAIS {
            assert_eq!(classificar_gpu("x", e).classe, *c, "{e}");
        }
        let r = classificar_gpu(
            "h264_nvenc",
            "[h264_nvenc @ 0x5] Driver does not support the required nvenc API version. Required: 13.1 Found: 13.0\n\
             [h264_nvenc @ 0x5] The minimum required Nvidia driver for nvenc is 610.00 or newer",
        );
        assert_eq!(r.classe, ClasseReprovacao::DriverAntigo);
        assert_eq!((r.driver_minimo.as_deref(), r.api_exigida.as_deref(), r.api_do_driver.as_deref()), (Some("610.00"), Some("13.1"), Some("13.0")));
        assert!(passageiro(&classificar_gpu("x", STDERR_REAIS[6].0), ""));

        // stderr guardados em p1c/: linhas `[degrau] classe: stderr` das sondas e os erros de sessão das medições
        let Some(d) = fontes::dossie() else {
            eprintln!("aviso: dossiê ausente; só as linhas embutidas");
            return;
        };
        let mut n = 0;
        for arq in ["sonda-normal.txt", "sonda-sem-gpu.txt", "sonda-sem-gpu-retomada.txt", "sonda-todos-retomada.txt"] {
            let Ok(t) = std::fs::read_to_string(d.join("p1c/out").join(arq)) else { continue };
            for l in t.lines().map(str::trim).filter(|l| l.starts_with('[')) {
                let Some((_, resto)) = l.split_once("] ") else { continue };
                let Some((classe, stderr)) = resto.split_once(": ") else { continue };
                assert_eq!(classificar_gpu("x", stderr).classe, classe_do_prototipo(classe), "{arq}: {l}");
                n += 1;
            }
        }
        for arq in ["sessoes.jsonl", "sessoes-livre.jsonl", "codificar.jsonl", "codificar-livre.jsonl"] {
            let Ok(t) = std::fs::read_to_string(d.join("p1c/out").join(arq)) else { continue };
            for l in t.lines() {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(l) else { continue };
                let erros: Vec<String> = match (&v["erros"], &v["erro"]) {
                    (serde_json::Value::Object(m), _) => m.keys().cloned().collect(),
                    (_, serde_json::Value::String(e)) if !e.is_empty() => vec![e.clone()],
                    _ => continue,
                };
                for e in erros.iter().filter(|e| e.contains("OpenEncodeSessionEx")) {
                    assert_eq!(classificar_gpu("h264_nvenc", e).classe, ClasseReprovacao::LimiteDeSessoes, "{arq}: {e}");
                    n += 1;
                }
            }
        }
        eprintln!("stderr reais do p1c classificados: {n}");
        assert!(n >= 10, "{n}");
    }

    /// Resultado gravado de `sonda_gpu.py`: por codificador, `Ok(degrau)` ou `Err(classe)`.
    /// Por codificador: `Ok(degrau)` ou `Err(classe)`.
    type LinhasSonda = Vec<(String, std::result::Result<String, String>)>;

    fn ler_sonda_txt(t: &str) -> (String, LinhasSonda) {
        let versao = t.lines().next().and_then(|l| l.split_whitespace().nth(2)).unwrap_or("").to_string();
        let mut v = Vec::new();
        for l in t.lines().skip(1) {
            let c: Vec<&str> = l.split_whitespace().collect();
            if c.len() >= 3 && (c[0].starts_with("h264_") || c[0] == "libx264") {
                match c[1] {
                    "OK" => v.push((c[0].to_string(), Ok(c[2].trim_start_matches("degrau=").to_string()))),
                    "FALHA" => v.push((c[0].to_string(), Err(c[2].to_string()))),
                    _ => {}
                }
            }
        }
        (versao, v)
    }

    /// Embutidas para a conferência rodar sem o dossiê (mesmas linhas de `p1c/out/`).
    const SONDA_7: &str = "ffmpeg version 7.0.2-static\n  h264_nvenc  FALHA ffmpeg_sem_codificador\n  h264_qsv    FALHA ffmpeg_sem_codificador\n  h264_vaapi  FALHA ffmpeg_sem_codificador\n  libx264     OK    degrau=cpu 274 ms\n";
    const SONDA_9: &str = "ffmpeg version n9.0.1\n  h264_nvenc  OK    degrau=completo 326 ms\n  h264_qsv    FALHA sem_driver\n  h264_vaapi  OK    degrau=cqp_b 111 ms\n  libx264     OK    degrau=cpu 106 ms\n";
    const SONDA_9_SEM_GPU: &str = "ffmpeg version n9.0.1\n  h264_nvenc  FALHA sem_gpu\n  h264_qsv    FALHA sem_driver\n  h264_vaapi  OK    degrau=cqp_b 110 ms\n";

    fn referencia(arquivo: &str, embutida: &str) -> String {
        fontes::dossie()
            .and_then(|d| std::fs::read_to_string(d.join("p1c/out").join(arquivo)).ok())
            .unwrap_or_else(|| {
                eprintln!("aviso: {arquivo} ausente; referência embutida");
                embutida.to_string()
            })
    }

    /// O que a sonda devolve × o que `sonda_gpu.py` gravou: o primeiro aprovado na ordem, com o degrau, e as classes dos anteriores.
    fn confere(gpu: &Option<Gpu>, reprovados: &[Reprovado], txt: &str, versao: &str) {
        let (v, linhas) = ler_sonda_txt(txt);
        assert_eq!(v, versao);
        if reprovados.iter().any(|x| x.classe == ClasseReprovacao::LimiteDeSessoes) {
            eprintln!("aviso: sessões do NVENC ocupadas por outro programa; conferência inconclusiva ({versao})");
            return;
        }
        let mut esperados = Vec::new();
        let mut aprovado = None;
        for cod in ordem_padrao() {
            match linhas.iter().find(|(c, _)| c == cod).map(|(_, r)| r) {
                Some(Ok(degrau)) => {
                    aprovado = Some((cod.to_string(), degrau.clone()));
                    break;
                }
                Some(Err(classe)) => esperados.push((cod.to_string(), classe_do_prototipo(classe))),
                None => {}
            }
        }
        let obtido: Vec<(String, ClasseReprovacao)> = reprovados.iter().map(|x| (x.codificador.clone(), x.classe)).collect();
        assert_eq!(obtido, esperados, "{versao}");
        assert_eq!(gpu.as_ref().map(|g| (g.codificador.clone(), g.degrau.clone())), aprovado, "{versao}");
    }

    /// `sondar` inteira (listas, autoteste, GPU) × `p1c/out/sonda-*.txt`; sem placa visível só nos filhos.
    #[test]
    fn sonda_confere_com_p1c() {
        let Some(cap) = fontes::cap() else { return };
        let t0 = Instant::now();
        let c = sondar(&cap.binario).unwrap();
        eprintln!(
            "sonda inteira ({}): {:?} em {:.2} s; reprovados {:?}",
            c.versao,
            c.gpu,
            t0.elapsed().as_secs_f64(),
            c.reprovados.iter().map(|x| (&x.codificador, x.classe)).collect::<Vec<_>>()
        );
        assert_eq!(c.versao, cap.versao);
        assert!(c.faltam.is_empty(), "{:?}", c.faltam);
        assert_eq!(c.grafo_em_arquivo, cap.grafo_em_arquivo);
        match cap.versao.as_str() {
            "7.0.2-static" => confere(&c.gpu, &c.reprovados, &referencia("sonda-appimage-7.0.2.txt", SONDA_7), "7.0.2-static"),
            "n9.0.1" => {
                confere(&c.gpu, &c.reprovados, &referencia("sonda-todos-retomada.txt", SONDA_9), "n9.0.1");
                if let Some(g) = &c.gpu {
                    assert!(g.nvdec && g.scale_cuda, "{g:?}");
                    assert!(!g.nome.is_empty() && !g.driver.is_empty(), "{g:?}");
                }
                let sem = sondar_gpu(&cap, ordem_padrao(), &[("CUDA_VISIBLE_DEVICES".into(), String::new())]);
                confere(&sem.gpu, &sem.reprovados, &referencia("sonda-sem-gpu-retomada.txt", SONDA_9_SEM_GPU), "n9.0.1");
            }
            v => eprintln!("versão sem referência medida: {v}"),
        }
    }

    /// Com o cache configurado, a sonda de GPU roda numa thread, grava `sonda-gpu.json` e troca a entrada da memória.
    #[cfg(unix)]
    #[test]
    fn sonda_de_gpu_em_fundo_grava_o_cache() {
        let Some(cap) = fontes::cap() else { return };
        let bin = falso("fundo", &cap, "");
        let cache = fontes::pasta_temporaria("cache-fundo");
        let c = capacidades_de(&bin, Origem::Path, Some(&cache), true).unwrap();
        assert!(c.gpu.is_none() && c.reprovados.is_empty());
        let t0 = Instant::now();
        while resumo(&c).sondando {
            assert!(t0.elapsed() < Duration::from_secs(120));
            std::thread::sleep(Duration::from_millis(50));
        }
        let nova = capacidades_de(&bin, Origem::Path, Some(&cache), true).unwrap();
        assert!(!Arc::ptr_eq(&c, &nova));
        match ler_json::<BTreeMap<String, EntradaGpu>>(&cache.join("sonda-gpu.json")) {
            Some(g) => {
                let e = &g[&c.sha256];
                assert_eq!((&nova.gpu, &nova.reprovados), (&e.gpu, &e.reprovados));
                assert_eq!(e.chave, chave_gpu(&c.sha256));
            }
            None => assert!(nova.reprovados.iter().any(|r| r.classe == ClasseReprovacao::LimiteDeSessoes), "{:?}", nova.reprovados),
        }
        estado().por_binario.remove(&bin);
    }

    /// "Testar de novo" durante a sonda de fundo: as duas não rodam juntas e `sondando` vale até a última acabar
    /// (qualquer uma pode pegar a vez primeiro). Cada codificação do falso segura uma trava por 0,3 s e deixa uma
    /// marca enquanto roda; duas sondas ao mesmo tempo se encontram na trava.
    #[cfg(unix)]
    #[test]
    fn testar_de_novo_espera_a_sonda_de_fundo() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("trava-gpu");
        let (trava, log) = (d.join("trava"), d.join("juntos.log"));
        let corpo = format!(
            r#"for a in "$@"; do if [ "$a" = "-c:v" ]; then touch '{d}/ativo.'$$; if mkdir '{t}' 2>/dev/null; then sleep 0.3; rmdir '{t}'; else echo junto >> '{l}'; fi; "$REAL" "$@"; r=$?; rm -f '{d}/ativo.'$$; exit $r; fi; done"#,
            d = d.display(),
            t = trava.display(),
            l = log.display()
        );
        let bin = falso("trava-gpu", &cap, &corpo);
        let ativos = || std::fs::read_dir(&d).into_iter().flatten().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("ativo.")).count();
        let cache = fontes::pasta_temporaria("cache-trava-gpu");
        let c = capacidades_de(&bin, Origem::Path, Some(&cache), true).unwrap();
        assert!(resumo(&c).sondando, "a sonda de fundo não começou");
        let fim = Arc::new(AtomicBool::new(false));
        let h = {
            let (bin, cache, fim) = (bin.clone(), cache.clone(), fim.clone());
            std::thread::spawn(move || {
                let r = testar_de_novo_de(&bin, Origem::Path, Some(&cache));
                fim.store(true, Ordering::SeqCst);
                r
            })
        };
        let t0 = Instant::now();
        while !fim.load(Ordering::SeqCst) {
            assert!(resumo(&c).sondando || fim.load(Ordering::SeqCst), "`sondando` caiu antes da última sonda acabar");
            assert!(t0.elapsed() < Duration::from_secs(300));
            std::thread::sleep(Duration::from_millis(10));
        }
        h.join().unwrap().unwrap();
        // a de fundo pode ter ficado para depois: `sondando` só cai quando ela acaba, e então nada mais roda
        while resumo(&c).sondando {
            assert!(t0.elapsed() < Duration::from_secs(300));
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(ativos(), 0, "`sondando` caiu com codificação rodando");
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(ativos(), 0);
        assert!(!log.exists(), "duas sondas de GPU ao mesmo tempo ({})", cap.versao);
        estado().por_binario.remove(&bin);
    }

    #[test]
    fn testar_de_novo_troca_a_entrada_da_memoria() {
        let Some(cap) = fontes::cap() else { return };
        let c = testar_de_novo().unwrap();
        assert_eq!(c.versao, cap.versao);
        assert!(!resumo(&c).sondando);
        assert!(Arc::ptr_eq(&c, &capacidades().unwrap()));
        if cap.versao == "7.0.2-static" {
            assert!(c.gpu.is_none() && c.reprovados.len() == 3);
        }
    }

    #[test]
    fn gpu_serve_no_tamanho_do_projeto() {
        let Some(cap) = fontes::cap() else { return };
        let fps = crate::edicao::modelo::Racional { num: 30, den: 1 };
        let r = sondar_gpu(&cap, ordem_padrao(), &[]);
        let c = Capacidades { gpu: r.gpu.clone(), ..(*cap).clone() };
        match &r.gpu {
            None => assert!(!gpu_serve(&c, 1920, 1080, fps), "{}", cap.versao),
            Some(g) => {
                assert!(gpu_serve(&c, 1920, 1080, fps), "{g:?}");
                if g.codificador == "h264_nvenc" {
                    // o NVENC recusa H.264 acima de 4096 de lado
                    assert!(!gpu_serve(&c, 7680, 4320, fps));
                }
            }
        }
    }

    /// Memória → `sonda-<sha>.json` → sonda; GPU de `sonda-gpu.json` só com a chave igual.
    #[cfg(unix)]
    #[test]
    fn cache_em_memoria_e_em_disco() {
        let Some(cap) = fontes::cap() else { return };
        let bin = falso("cache", &cap, "");
        let cache = fontes::pasta_temporaria("cache-sonda");
        let c1 = capacidades_de(&bin, Origem::Path, Some(&cache), false).unwrap();
        let arq = cache.join(format!("sonda-{}.json", c1.sha256));
        assert!(arq.is_file());
        let c2 = capacidades_de(&bin, Origem::Path, Some(&cache), false).unwrap();
        assert!(Arc::ptr_eq(&c1, &c2));

        let mut j: serde_json::Value = serde_json::from_slice(&std::fs::read(&arq).unwrap()).unwrap();
        j["configuracao"] = "lido do disco".into();
        std::fs::write(&arq, serde_json::to_vec(&j).unwrap()).unwrap();
        let gpu = Gpu {
            nome: "placa".into(),
            driver: "1".into(),
            codificador: "h264_teste".into(),
            degrau: "completo".into(),
            args: vec![],
            nvdec: false,
            scale_cuda: false,
        };
        let mut todas = BTreeMap::new();
        todas.insert(c1.sha256.clone(), EntradaGpu { chave: chave_gpu(&c1.sha256), gpu: Some(gpu.clone()), reprovados: vec![] });
        gravar_json(&cache.join("sonda-gpu.json"), &todas).unwrap();
        estado().por_binario.remove(&bin);
        let c3 = capacidades_de(&bin, Origem::Path, Some(&cache), false).unwrap();
        assert_eq!(c3.configuracao, "lido do disco");
        assert_eq!(c3.gpu.as_ref(), Some(&gpu));
        assert_eq!(resumo(&c3).codificador, "h264_teste");

        // chave diferente (outro driver): a GPU guardada não vale
        let mut outra = chave_gpu(&c1.sha256);
        outra.nvidia_smi = "000.00, outra placa".into();
        todas.insert(c1.sha256.clone(), EntradaGpu { chave: outra, gpu: Some(gpu), reprovados: vec![] });
        gravar_json(&cache.join("sonda-gpu.json"), &todas).unwrap();
        estado().por_binario.remove(&bin);
        let c4 = capacidades_de(&bin, Origem::Path, Some(&cache), false).unwrap();
        assert!(c4.gpu.is_none());
        assert_eq!(resumo(&c4).codificador, "libx264");
        estado().por_binario.remove(&bin);
    }
}
