//! Processo do ffmpeg (§5.10): vigia de cancelamento, stderr em arquivo, filhos presos ao pai. Dono: P0.4.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::args;
use crate::error::{Result, SicroError};

/// Um processo a rodar. `pasta` = `current_dir`; `ambiente` acrescenta e `sem_ambiente` remove variáveis;
/// `stderr` = arquivo que recebe o stderr inteiro; `nome` identifica o passo no relatório.
#[derive(Debug, Clone, PartialEq)]
pub struct Comando {
    pub nome: String,
    pub programa: PathBuf,
    pub args: Vec<String>,
    pub pasta: PathBuf,
    pub ambiente: Vec<(String, String)>,
    pub sem_ambiente: Vec<String>,
    pub stderr: PathBuf,
}

/// Como o processo acabou. `codigo` None = morto por sinal; `inicio` e `fim` em RFC 3339.
/// `cancelado` = morto pela vigia ou por `matar_todos`. É o item de `execucao.passos` do `tecnico.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Termino {
    pub nome: String,
    pub argumentos: Vec<String>,
    pub inicio: String,
    pub fim: String,
    pub codigo: Option<i32>,
    pub cancelado: bool,
}

/// Classe curta de falha, para a mensagem da fila. `LeituraDivergente` não sai do stderr: é a conferência do executor.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClasseFalha {
    FiltroAusente,
    ArquivoAusente,
    DiscoCheio,
    Gpu,
    LeituraDivergente,
    Outra,
}

// CONTRATO: acréscimo. Uma leitura do `-progress pipe:1`; `rodar` não tem retorno de chamada e o executor
// (P0.7) e a tabela do decodificador (P0.3b) precisam do avanço.
/// Avanço do ffmpeg: quadros já saídos, tempo de saída (µs), velocidade (× tempo real); `fim` na última leitura.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProgressoFfmpeg {
    pub quadro: u64,
    pub tempo_us: Option<i64>,
    pub velocidade: Option<f32>,
    pub fim: bool,
}

/// Intervalo da vigia: tempo máximo entre o pedido de cancelamento e o `kill`.
const VIGIA: Duration = Duration::from_millis(10);

struct Vivo {
    filho: Mutex<Child>,
    morto: AtomicBool,
}

fn vivos() -> &'static Mutex<HashMap<u64, Arc<Vivo>>> {
    static VIVOS: OnceLock<Mutex<HashMap<u64, Arc<Vivo>>>> = OnceLock::new();
    VIVOS.get_or_init(Default::default)
}

static PROXIMO: AtomicU64 = AtomicU64::new(1);

enum Saida<'a> {
    Nada,
    Captura,
    Progresso(&'a mut dyn FnMut(&ProgressoFfmpeg)),
}

/// Roda até o fim ou até `cancelar` virar true (a vigia mata sem depender da saída do processo).
/// Código ≠ 0 não é `Err`: vem em `Termino`. `Err` = não deu para começar (programa ausente, linha grande, E/S).
pub fn rodar(cmd: &Comando, cancelar: &AtomicBool) -> Result<Termino> {
    rodar_interno(cmd, cancelar, Saida::Nada).map(|(t, _)| t)
}

/// Como `rodar`, devolvendo também o stdout inteiro (ffprobe, `-f framehash -`).
pub fn rodar_com_saida(cmd: &Comando, cancelar: &AtomicBool) -> Result<(Termino, Vec<u8>)> {
    rodar_interno(cmd, cancelar, Saida::Captura)
}

// CONTRATO: acréscimo (ver `ProgressoFfmpeg`).
/// Como `rodar`, com `-progress pipe:1` posto na frente dos argumentos (se faltar) e `progresso` chamado a cada leitura.
/// O cancelamento não depende de chegar progresso.
pub fn rodar_com_progresso(cmd: &Comando, cancelar: &AtomicBool, progresso: &mut dyn FnMut(&ProgressoFfmpeg)) -> Result<Termino> {
    rodar_interno(cmd, cancelar, Saida::Progresso(progresso)).map(|(t, _)| t)
}

fn rodar_interno(cmd: &Comando, cancelar: &AtomicBool, mut saida: Saida<'_>) -> Result<(Termino, Vec<u8>)> {
    let mut argumentos = cmd.args.clone();
    if matches!(saida, Saida::Progresso(_)) && !argumentos.iter().any(|a| a == "-progress") {
        argumentos.splice(0..0, ["-progress".to_string(), "pipe:1".to_string()]);
    }
    if args::tamanho_da_linha(&cmd.programa, &argumentos) >= args::LIMITE_LINHA {
        return Err(SicroError::Validation(format!("linha_grande: {}", cmd.nome)));
    }
    if let Some(p) = cmd.stderr.parent() {
        std::fs::create_dir_all(p)?;
    }
    let erro = File::create(&cmd.stderr)?;

    let mut c = crate::tools::command(&cmd.programa);
    c.args(&argumentos)
        .current_dir(&cmd.pasta)
        .stdin(Stdio::null())
        .stdout(if matches!(saida, Saida::Nada) { Stdio::null() } else { Stdio::piped() })
        .stderr(Stdio::from(erro));
    for (k, v) in &cmd.ambiente {
        c.env(k, v);
    }
    for k in &cmd.sem_ambiente {
        c.env_remove(k);
    }
    preso_ao_pai(&mut c);

    let inicio = agora();
    let mut filho = c.spawn().map_err(|e| {
        SicroError::Filesystem(format!("não foi possível rodar {} ({}): {e}", cmd.programa.display(), cmd.nome))
    })?;
    #[cfg(windows)]
    job::prender(&filho);
    let stdout = filho.stdout.take();
    let vivo = Arc::new(Vivo { filho: Mutex::new(filho), morto: AtomicBool::new(false) });
    let id = PROXIMO.fetch_add(1, Ordering::Relaxed);
    vivos().lock().unwrap_or_else(|e| e.into_inner()).insert(id, vivo.clone());

    let mut capturado = Vec::new();
    let (estado, leitura) = std::thread::scope(|s| {
        let vigia = s.spawn(|| vigiar(&vivo, cancelar));
        let leitura = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(mut out) = stdout {
                match &mut saida {
                    Saida::Nada => {}
                    Saida::Captura => {
                        let _ = out.read_to_end(&mut capturado);
                    }
                    Saida::Progresso(f) => ler_progresso(out, &mut **f),
                }
            }
        }));
        // pânico no retorno de chamada: o filho não fica rodando sozinho até o fim
        if leitura.is_err() {
            vivo.morto.store(true, Ordering::SeqCst);
            let _ = vivo.filho.lock().unwrap_or_else(|e| e.into_inner()).kill();
        }
        (vigia.join(), leitura)
    });
    vivos().lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
    if let Err(p) = leitura {
        std::panic::resume_unwind(p);
    }
    let estado = estado.map_err(|_| SicroError::Validation(format!("vigia_falhou: {}", cmd.nome)))??;

    Ok((
        Termino {
            nome: cmd.nome.clone(),
            argumentos,
            inicio,
            fim: agora(),
            codigo: estado.code(),
            cancelado: vivo.morto.load(Ordering::SeqCst),
        },
        capturado,
    ))
}

fn vigiar(vivo: &Vivo, cancelar: &AtomicBool) -> std::io::Result<ExitStatus> {
    loop {
        if cancelar.load(Ordering::SeqCst) && !vivo.morto.swap(true, Ordering::SeqCst) {
            let _ = vivo.filho.lock().unwrap_or_else(|e| e.into_inner()).kill();
        }
        if let Some(st) = vivo.filho.lock().unwrap_or_else(|e| e.into_inner()).try_wait()? {
            return Ok(st);
        }
        std::thread::sleep(VIGIA);
    }
}

fn ler_progresso(out: impl Read, f: &mut dyn FnMut(&ProgressoFfmpeg)) {
    let mut p = ProgressoFfmpeg::default();
    for linha in BufReader::new(out).lines() {
        let Ok(linha) = linha else { break };
        let Some((k, v)) = linha.split_once('=') else { continue };
        let v = v.trim();
        match k.trim() {
            "frame" => p.quadro = v.parse().unwrap_or(p.quadro),
            "out_time_us" => p.tempo_us = v.parse().ok(),
            "speed" => p.velocidade = v.trim_end_matches('x').trim().parse().ok(),
            "progress" => {
                p.fim = v == "end";
                f(&p);
            }
            _ => {}
        }
    }
}

fn agora() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(target_os = "linux")]
fn preso_ao_pai(c: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    extern "C" {
        fn prctl(opcao: std::ffi::c_int, ...) -> std::ffi::c_int;
    }
    const PR_SET_PDEATHSIG: std::ffi::c_int = 1;
    const SIGKILL: std::ffi::c_ulong = 9;
    let pai = std::process::id();
    // Vale enquanto a thread que criou o filho viver: `rodar` só volta depois que ele acaba.
    unsafe {
        c.pre_exec(move || {
            if prctl(PR_SET_PDEATHSIG, SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            // o pai morreu entre o fork e o prctl
            if std::os::unix::process::parent_id() != pai {
                return Err(std::io::Error::from(std::io::ErrorKind::Interrupted));
            }
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
fn preso_ao_pai(_c: &mut std::process::Command) {}

/// Windows: todo filho entra num Job Object com `KILL_ON_JOB_CLOSE`; o handle só fecha quando o SICRO acaba.
#[cfg(windows)]
mod job {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;

    type Handle = *mut c_void;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimit {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimit {
        basic: BasicLimit,
        io: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(atributos: *mut c_void, nome: *const u16) -> Handle;
        fn SetInformationJobObject(job: Handle, classe: i32, info: *mut c_void, tamanho: u32) -> i32;
        fn AssignProcessToJobObject(job: Handle, processo: Handle) -> i32;
    }

    fn job() -> Option<usize> {
        static JOB: OnceLock<Option<usize>> = OnceLock::new();
        *JOB.get_or_init(|| unsafe {
            let h = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
            if h.is_null() {
                return None;
            }
            let mut info = ExtendedLimit::default();
            info.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                h,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                &mut info as *mut ExtendedLimit as *mut c_void,
                std::mem::size_of::<ExtendedLimit>() as u32,
            );
            if ok == 0 {
                return None;
            }
            Some(h as usize)
        })
    }

    pub(super) fn prender(filho: &std::process::Child) {
        if let Some(j) = job() {
            unsafe {
                AssignProcessToJobObject(j as Handle, filho.as_raw_handle() as Handle);
            }
        }
    }
}

/// `frames in` de cada `fps@<rótulo>` no stderr gravado: vale a ÚLTIMA linha de cada rótulo
/// (a primeira é `0 frames in`, do grafo de sondagem; T1).
pub fn quadros_lidos(stderr: &Path) -> Result<HashMap<String, u64>> {
    let mut m = HashMap::new();
    for linha in linhas(stderr)? {
        let Some((rotulo, resto)) = rotulo_de(&linha, "fps@") else { continue };
        let Some(i) = resto.find(" frames in") else { continue };
        if let Some(n) = resto[..i].split_whitespace().last().and_then(|n| n.parse().ok()) {
            m.insert(rotulo, n);
        }
    }
    Ok(m)
}

/// pts impressos por cada `showinfo@<rótulo>` no stderr gravado, na ordem. `pts: NOPTS` vira `i64::MIN`.
pub fn pts_lidos(stderr: &Path) -> Result<HashMap<String, Vec<i64>>> {
    let mut m: HashMap<String, Vec<i64>> = HashMap::new();
    for linha in linhas(stderr)? {
        let Some((rotulo, resto)) = rotulo_de(&linha, "showinfo@") else { continue };
        let resto = resto.trim_start();
        if !resto.starts_with("n:") {
            continue;
        }
        let Some(i) = resto.find(" pts:") else { continue };
        let Some(v) = resto[i + 5..].split_whitespace().next() else { continue };
        let pts = if v == "NOPTS" { i64::MIN } else { v.parse().map_err(|_| SicroError::Validation(format!("showinfo: {linha}")))? };
        m.entry(rotulo).or_default().push(pts);
    }
    Ok(m)
}

/// `[nome@rotulo @ 0x…] resto` → (rotulo, resto).
fn rotulo_de(linha: &str, prefixo: &str) -> Option<(String, String)> {
    // só no começo da linha: metadado do arquivo com o mesmo texto não vira quadro lido
    // (sem `-nostats`, a linha de progresso do ffmpeg termina em `\r` e fica colada na frente)
    let linha = match linha.rsplit_once('\r') {
        Some((antes, depois)) if antes.starts_with("frame=") || antes.starts_with("size=") => depois,
        _ => linha,
    };
    let resto = linha.strip_prefix('[')?.strip_prefix(prefixo)?;
    let fim = resto.find(" @ ")?;
    let rotulo = &resto[..fim];
    let depois = &resto[fim..];
    let j = depois.find(']')?;
    Some((rotulo.to_string(), depois[j + 1..].to_string()))
}

fn linhas(arquivo: &Path) -> Result<Vec<String>> {
    let mut f = BufReader::new(File::open(arquivo)?);
    let mut v = Vec::new();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if f.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        v.push(String::from_utf8_lossy(&buf).trim_end_matches(['\n', '\r']).to_string());
    }
    Ok(v)
}

/// Últimas `n` linhas do stderr gravado (lê só o fim do arquivo; `\r` também separa).
pub fn ultimas_linhas(stderr: &Path, n: usize) -> Vec<String> {
    const FIM: u64 = 256 * 1024;
    let Ok(mut f) = File::open(stderr) else { return Vec::new() };
    let tam = f.metadata().map(|m| m.len()).unwrap_or(0);
    let cortado = tam > FIM;
    if cortado && f.seek(SeekFrom::Start(tam - FIM)).is_err() {
        return Vec::new();
    }
    let mut b = Vec::new();
    if f.read_to_end(&mut b).is_err() {
        return Vec::new();
    }
    let texto = String::from_utf8_lossy(&b);
    let mut l: Vec<&str> = texto.split(['\n', '\r']).collect();
    if cortado && !l.is_empty() {
        l.remove(0);
    }
    let l: Vec<String> = l.into_iter().map(str::trim_end).filter(|s| !s.is_empty()).map(String::from).collect();
    l[l.len().saturating_sub(n)..].to_vec()
}

/// Classifica a falha pelas últimas linhas do stderr.
pub fn classificar(linhas: &[String]) -> ClasseFalha {
    let t = linhas.join("\n").to_lowercase();
    let tem = |ps: &[&str]| ps.iter().any(|p| t.contains(p));
    if tem(&["no space left on device", "not enough space on the disk", "disk full"]) {
        ClasseFalha::DiscoCheio
    } else if tem(&["no such filter", "filter not found", "unknown filter", "unknown encoder", "encoder not found"]) {
        ClasseFalha::FiltroAusente
    } else if tem(&[
        "nvenc",
        "cuda",
        "cuinit",
        "vaapi",
        "mfx session",
        "_qsv",
        "amfrt",
        "no capable devices",
        "openencodesessionex",
        "device creation failed",
        "hardware device setup failed",
    ]) {
        ClasseFalha::Gpu
    } else if tem(&["no such file or directory", "does not exist", "cannot find the file", "cannot find the path"]) {
        ClasseFalha::ArquivoAusente
    } else {
        ClasseFalha::Outra
    }
}

// CONTRATO: acréscimo. O erro de passo que todo chamador montava à mão.
/// `Ok` com código 0; senão `"<classe>: <nome> (código <c>): <últimas 5 linhas do stderr>"` (classe de `classificar`,
/// em snake_case), ou `"cancelado: <nome>"`.
pub fn exigir_sucesso(cmd: &Comando, t: &Termino) -> Result<()> {
    if t.cancelado {
        return Err(SicroError::Validation(format!("cancelado: {}", cmd.nome)));
    }
    if t.codigo == Some(0) {
        return Ok(());
    }
    let linhas = ultimas_linhas(&cmd.stderr, 5);
    let classe = serde_json::to_value(classificar(&linhas)).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    Err(SicroError::Validation(format!("{classe}: {} (código {:?}): {}", cmd.nome, t.codigo, linhas.join(" | "))))
}

/// Posto por `matar_todos`: o app está saindo.
static ENCERRANDO: AtomicBool = AtomicBool::new(false);

// CONTRATO: acréscimo (revisão do fecho da Fase 1, contratos-fase1 (g) R4).
/// `matar_todos` já rodou (o app está saindo)? O que um filho morto por ele "mediu" não vale: quem grava resultado
/// de medição (a sonda de GPU) pergunta aqui antes.
pub(crate) fn encerrando() -> bool {
    ENCERRANDO.load(Ordering::SeqCst)
}

/// Mata todos os filhos vivos (chamada no `ExitRequested`); quem esperava por eles recebe `cancelado`.
pub fn matar_todos() {
    ENCERRANDO.store(true, Ordering::SeqCst);
    let vivos: Vec<Arc<Vivo>> = vivos().lock().unwrap_or_else(|e| e.into_inner()).values().cloned().collect();
    for v in vivos {
        v.morto.store(true, Ordering::SeqCst);
        let _ = v.filho.lock().unwrap_or_else(|e| e.into_inner()).kill();
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::testes::fontes;
    use std::time::Instant;

    // Limites reais; os testes medem com folga de 3× enquanto houver outros agentes na máquina (O1).
    const CANCELAR_S: f64 = 1.0;
    const PAI_MORTO_S: f64 = 1.0;
    const FOLGA: f64 = 3.0;

    fn cmd(pasta: &Path, programa: &Path, args: &[&str]) -> Comando {
        Comando {
            nome: "teste".into(),
            programa: programa.to_path_buf(),
            args: args.iter().map(|s| s.to_string()).collect(),
            pasta: pasta.to_path_buf(),
            ambiente: Vec::new(),
            sem_ambiente: Vec::new(),
            stderr: pasta.join("stderr.txt"),
        }
    }

    #[test]
    fn le_fps_e_showinfo_do_stderr() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("e.txt");
        std::fs::write(
            &p,
            "[fps@c003 @ 0x55] 0 frames in, 0 frames out; 0 frames dropped, 0 frames duplicated.\n\
             [showinfo@c003 @ 0x56] config in time_base: 1/30, frame_rate: 30/1\n\
             [showinfo@c003 @ 0x56] n:   0 pts:      3 pts_time:0.1     duration:      1\n\
             [showinfo@c003 @ 0x56] color_range:unknown color_space:unknown\n\
             [showinfo@c003 @ 0x56] n:   1 pts:  -7680 pts_time:-0.5\n\
             [showinfo@x @ 0x57] n:   0 pts:  NOPTS pts_time:NOPTS\n\
             [fps@c003 @ 0x55] 60 frames in, 60 frames out; 0 frames dropped, 0 frames duplicated.\n\
             [fps@c012 @ 0x58] 7 frames in, 9 frames out; 0 frames dropped, 2 frames duplicated.\r\n",
        )
        .unwrap();
        let q = quadros_lidos(&p).unwrap();
        assert_eq!(q["c003"], 60);
        assert_eq!(q["c012"], 7);
        let pts = pts_lidos(&p).unwrap();
        assert_eq!(pts["c003"], vec![3, -7680]);
        assert_eq!(pts["x"], vec![i64::MIN]);
        assert_eq!(ultimas_linhas(&p, 2)[1], "[fps@c012 @ 0x58] 7 frames in, 9 frames out; 0 frames dropped, 2 frames duplicated.");
    }

    #[test]
    fn classifica_falhas() {
        let c = |s: &str| classificar(&[s.to_string()]);
        assert_eq!(c("[AVFilterGraph @ 0x1] No such filter: 'drawtext'"), ClasseFalha::FiltroAusente);
        assert_eq!(c("a.mp4: No such file or directory"), ClasseFalha::ArquivoAusente);
        assert_eq!(c("av_interleaved_write_frame(): No space left on device"), ClasseFalha::DiscoCheio);
        assert_eq!(c("[h264_nvenc @ 0x1] OpenEncodeSessionEx failed: incompatible client key (21)"), ClasseFalha::Gpu);
        assert_eq!(c("Device creation failed: -12."), ClasseFalha::Gpu);
        assert_eq!(c("Invalid data found when processing input"), ClasseFalha::Outra);
    }

    #[test]
    fn codigo_e_stderr_em_arquivo() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("exec-codigo");
        let c = cmd(&d, &cap.binario, &["-hide_banner", "-nostdin", "-i", "nao-existe.mp4", "-f", "null", "-"]);
        let t = rodar(&c, &AtomicBool::new(false)).unwrap();
        assert_ne!(t.codigo, Some(0));
        assert!(!t.cancelado);
        let l = ultimas_linhas(&c.stderr, 20);
        assert_eq!(classificar(&l), ClasseFalha::ArquivoAusente, "{l:?}");
        let e = exigir_sucesso(&c, &t).unwrap_err().to_string();
        assert!(e.contains("arquivo_ausente: teste (código Some(") && e.contains("nao-existe.mp4"), "{e}");

        // current_dir: nome simples resolvido na pasta de trabalho; stdout capturado
        std::fs::write(d.join("g.txt"), "color=c=red:s=16x16:d=0.2,format=gray[v]").unwrap();
        let c = cmd(&d, &cap.binario, &["-hide_banner", "-nostdin", "-/filter_complex", "g.txt", "-map", "[v]", "-f", "rawvideo", "-"]);
        let (t, out) = rodar_com_saida(&c, &AtomicBool::new(false)).unwrap();
        assert_eq!(t.codigo, Some(0), "{:?}", ultimas_linhas(&c.stderr, 5));
        assert_eq!(out.len(), 16 * 16 * 5, "{} ({})", out.len(), cap.versao);

        // progresso chega e termina com fim
        let c = cmd(&d, &cap.binario, &["-hide_banner", "-nostdin", "-f", "lavfi", "-i", "testsrc2=s=64x48:r=30:d=1", "-f", "null", "-"]);
        let mut ultimo = ProgressoFfmpeg::default();
        let t = rodar_com_progresso(&c, &AtomicBool::new(false), &mut |p| ultimo = *p).unwrap();
        assert_eq!(t.codigo, Some(0));
        assert!(ultimo.fim);
        assert_eq!(ultimo.quadro, 30);
        assert_eq!(&t.argumentos[..2], ["-progress", "pipe:1"]);
    }

    /// stderr de megabytes vai inteiro para o arquivo sem segurar o filho (não há pipe para encher).
    #[test]
    fn stderr_grande_nao_trava() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("exec-stderr");
        let c = cmd(
            &d,
            &cap.binario,
            &["-hide_banner", "-nostdin", "-f", "lavfi", "-i", "testsrc2=s=64x48:r=30:d=200", "-vf", "showinfo@s", "-f", "null", "-"],
        );
        let t = rodar(&c, &AtomicBool::new(false)).unwrap();
        assert_eq!(t.codigo, Some(0));
        assert!(std::fs::metadata(&c.stderr).unwrap().len() > 1 << 20);
        assert_eq!(pts_lidos(&c.stderr).unwrap()["s"], (0..6000).collect::<Vec<i64>>());
        // só o fim é lido
        assert!(ultimas_linhas(&c.stderr, 3).len() == 3);
    }

    #[test]
    fn linha_grande_e_recusada() {
        let d = tempfile::tempdir().unwrap();
        let grande = "x".repeat(args::LIMITE_LINHA);
        let c = cmd(d.path(), Path::new("ffmpeg"), &[&grande]);
        assert!(matches!(rodar(&c, &AtomicBool::new(false)), Err(SicroError::Validation(m)) if m.starts_with("linha_grande")));
    }

    /// 5 000 filtros com conversão de formato: o ffmpeg fica segundos calado abrindo o grafo (medido: ~3 s nos dois).
    #[test]
    fn cancelar_na_abertura_de_grafo_de_5000_filtros() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("exec-cancelar");
        let g = vec!["format=yuv444p,format=yuv420p"; 2500].join(",");
        args::gravar_texto(&d, "g.txt", &format!("[0:v]{g}[v]")).unwrap();
        let mut a = vec!["-hide_banner", "-nostdin", "-filter_complex_threads", "1", "-f", "lavfi", "-i", "testsrc2=s=64x48:r=30:d=60"];
        let ga = args::grafo_em_arquivo(&cap, "g.txt");
        a.extend(ga.iter().map(String::as_str));
        a.extend(["-map", "[v]", "-f", "null", "-"]);
        let c = cmd(&d, &cap.binario, &a);
        let cancelar = AtomicBool::new(false);
        let pedido: Mutex<Option<Instant>> = Mutex::new(None);
        let mut progressos = 0u32;
        let (t, fim) = std::thread::scope(|s| {
            s.spawn(|| {
                std::thread::sleep(Duration::from_millis(400));
                *pedido.lock().unwrap() = Some(Instant::now());
                cancelar.store(true, Ordering::SeqCst);
            });
            let r = rodar_com_progresso(&c, &cancelar, &mut |_| progressos += 1);
            (r, Instant::now())
        });
        let t = t.unwrap();
        assert!(t.cancelado, "{t:?}");
        #[cfg(unix)]
        assert_eq!(t.codigo, None);
        assert_eq!(progressos, 0, "o ffmpeg já tinha aberto o grafo ({})", cap.versao);
        let depois = fim.duration_since(pedido.lock().unwrap().unwrap()).as_secs_f64();
        eprintln!("cancelar ({}): {depois:.3} s depois do pedido", cap.versao);
        assert!(depois < CANCELAR_S * FOLGA, "{depois} s ({})", cap.versao);
    }

    /// Pânico no retorno de chamada do progresso: o ffmpeg morre, a lista de vivos esvazia e o pânico segue.
    #[cfg(target_os = "linux")]
    #[test]
    fn panico_no_progresso_mata_o_filho() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("exec-panico");
        // -re: sem a morte, rodaria 10 min; d=601 distingue este ffmpeg dos vizinhos
        let c = cmd(&d, &cap.binario, &["-hide_banner", "-nostdin", "-re", "-f", "lavfi", "-i", "testsrc2=s=64x48:r=30:d=601", "-f", "null", "-"]);
        let meus = || -> Vec<u32> {
            filhos_de(std::process::id())
                .into_iter()
                .filter(|p| vivo(*p) && std::fs::read(format!("/proc/{p}/cmdline")).is_ok_and(|c| String::from_utf8_lossy(&c).contains("r=30:d=601")))
                .collect()
        };
        let mut pid = 0;
        let t0 = Instant::now();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rodar_com_progresso(&c, &AtomicBool::new(false), &mut |_| {
                pid = meus()[0];
                panic!("retorno de chamada")
            })
        }));
        assert!(r.is_err() && pid != 0);
        assert!(t0.elapsed().as_secs_f64() < CANCELAR_S * FOLGA + 2.0, "{:?}", t0.elapsed());
        assert!(meus().is_empty(), "ffmpeg vivo depois do pânico ({})", cap.versao);
        let preso = vivos().lock().unwrap().values().any(|v| v.filho.lock().unwrap().id() == pid);
        assert!(!preso, "o filho ficou na lista de vivos");
    }

    /// Processo auxiliar: roda um ffmpeg longo por `rodar` e espera. Só age quando chamado por `matar_o_pai_mata_os_filhos`.
    #[test]
    #[ignore]
    fn auxiliar_pai_de_ffmpeg() {
        let Ok(pasta) = std::env::var("SICRO_TESTE_PAI") else { return };
        let Some(cap) = fontes::cap() else { return };
        let pasta = PathBuf::from(pasta);
        let c = cmd(&pasta, &cap.binario, &["-hide_banner", "-nostdin", "-re", "-f", "lavfi", "-i", "testsrc2=s=64x48:r=30:d=600", "-f", "null", "-"]);
        let _ = rodar(&c, &AtomicBool::new(false));
    }

    #[cfg(target_os = "linux")]
    fn filhos_de(pid: u32) -> Vec<u32> {
        let mut v = Vec::new();
        for e in std::fs::read_dir("/proc").unwrap().flatten() {
            let Ok(p) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{p}/stat")) else { continue };
            // pid (comm) estado ppid …
            let Some(depois) = stat.rfind(')').map(|i| &stat[i + 2..]) else { continue };
            let campos: Vec<&str> = depois.split_whitespace().collect();
            if campos.get(1).and_then(|x| x.parse::<u32>().ok()) == Some(pid) {
                v.push(p);
            }
        }
        v
    }

    #[cfg(target_os = "linux")]
    fn vivo(pid: u32) -> bool {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|s| s.rfind(')').map(|i| !s[i + 2..].starts_with('Z')).unwrap_or(false))
            .unwrap_or(false)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn matar_o_pai_mata_os_filhos() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("exec-pai");
        let mut aux = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "edicao::ffmpeg::exec::testes::auxiliar_pai_de_ffmpeg", "--ignored", "--nocapture"])
            .env("SICRO_TESTE_PAI", &d)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        // o ffmpeg é neto do teste: filho do auxiliar
        let t0 = Instant::now();
        let ffmpeg = loop {
            let f: Vec<u32> = filhos_de(aux.id())
                .into_iter()
                // o ffmpeg longo, não um da sonda que o auxiliar roda antes
                .filter(|p| std::fs::read(format!("/proc/{p}/cmdline")).is_ok_and(|c| String::from_utf8_lossy(&c).contains("r=30:d=600")))
                .collect();
            if let Some(p) = f.first() {
                break *p;
            }
            assert!(t0.elapsed() < Duration::from_secs(60), "o auxiliar não abriu o ffmpeg");
            std::thread::sleep(Duration::from_millis(20));
        };
        aux.kill().unwrap();
        let morto = Instant::now();
        let _ = aux.wait();
        while vivo(ffmpeg) {
            assert!(morto.elapsed().as_secs_f64() < PAI_MORTO_S * FOLGA, "ffmpeg {ffmpeg} vivo depois da morte do pai ({})", cap.versao);
            std::thread::sleep(Duration::from_millis(10));
        }
        eprintln!("filho morto {:.3} s depois do pai ({})", morto.elapsed().as_secs_f64(), cap.versao);
    }

    /// `matar_todos` num processo auxiliar (no processo dos testes mataria o ffmpeg dos vizinhos).
    #[test]
    #[ignore]
    fn auxiliar_matar_todos() {
        let Ok(pasta) = std::env::var("SICRO_TESTE_MATAR") else { return };
        let Some(cap) = fontes::cap() else { return };
        let pasta = PathBuf::from(pasta);
        let c = cmd(&pasta, &cap.binario, &["-hide_banner", "-nostdin", "-re", "-f", "lavfi", "-i", "testsrc2=s=64x48:r=30:d=600", "-f", "null", "-"]);
        std::thread::spawn(|| {
            let t0 = Instant::now();
            while vivos().lock().unwrap().is_empty() {
                if t0.elapsed() > Duration::from_secs(60) {
                    std::process::exit(2);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            std::thread::sleep(Duration::from_millis(200));
            matar_todos();
        });
        let t0 = Instant::now();
        let t = rodar(&c, &AtomicBool::new(false)).unwrap();
        assert!(t.cancelado);
        assert!(t0.elapsed() < Duration::from_secs(10));
        std::fs::write(pasta.join("ok"), "ok").unwrap();
    }

    #[test]
    fn matar_todos_encerra_o_filho() {
        let Some(_cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("exec-matar");
        let st = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "edicao::ffmpeg::exec::testes::auxiliar_matar_todos", "--ignored", "--nocapture"])
            .env("SICRO_TESTE_MATAR", &d)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(st.success());
        assert!(d.join("ok").exists());
    }
}
