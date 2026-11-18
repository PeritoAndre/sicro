//! Gravadores (DVR), P4.6 (`receitas-audio-saida.md` §9): formato pelos bytes do começo, cópia de trabalho em MKV
//! por remux (`-fflags +genpts -c copy -copyinkf`) e a prova de que ela traz os mesmos quadros: lista `framemd5`
//! igual à do original (vídeo e áudio), tempos no mesmo passo e o som no mesmo lugar em relação à imagem. O original
//! fica como chegou; a cópia entra no caso como mídia derivada (`derivation_json.kind = "copia_dvr"`), com o
//! `start_time` do original e as lacunas de pts.
//!
//! Importação (P4.6b): `preparar` faz e prova a cópia a partir do arquivo escolhido, com avanço (`Fase`, fração pelo
//! pts lido) e cancelamento, antes de o original entrar no caso; `registrar_pendente` registra a cópia depois que o
//! original foi registrado. Cancelada, nada fica no caso.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::database::repositories::{occurrence_repo, video_repo};
use crate::edicao::ffmpeg::exec::{self, Comando};
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::Racional;
use crate::error::{Result, SicroError};
use crate::hashing::sha256::sha256_file;
use crate::models::VideoMedia;
use crate::workspace::paths::resolver_relativo;

/// Pasta das cópias de trabalho, relativa ao caso.
pub const REL_COPIAS_DVR: &str = "Sistema/videos/copias-dvr";
/// Teto de quadros de uma lista `framemd5` (pts guardados): 2^25 = 12 dias a 30 qps.
pub const MAX_QUADROS: usize = 1 << 25;
/// `start_time` entre 2000 e 2100 é data do relógio do gravador (s desde 1970), não contador.
const EPOCA: std::ops::Range<f64> = 946_684_800.0..4_102_444_800.0;
/// Pedaços do topo do AVI percorridos à procura do índice.
const MAX_PEDACOS: usize = 1 << 20;
/// Bytes do cabeçalho `hdrl` do AVI lidos à procura do `indx` (OpenDML).
const MAX_HDRL: u64 = 1 << 20;
const IFV: [u8; 17] = [0x11, 0xd2, 0xd3, 0xab, 0xba, 0xa9, 0xcf, 0x11, 0x8e, 0xe6, 0x00, 0xc0, 0x0c, 0x20, 0x53, 0x65, 0x44];

/// Formato de gravador reconhecido.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Formato {
    /// Dahua e OEM (Intelbras): "DHAV" ou "DAHUA".
    Dahua,
    /// Hikvision atual: "IMKH" + MPEG-PS; chega como `.mp4`.
    HikvisionImkh,
    /// Hikvision antigo: "4HKH"; o ffmpeg não abre.
    HikvisionAntigo,
    Ifv,
    Lvf,
    /// H.264 sem contêiner: o arquivo não tem tempos.
    H264Cru,
    HevcCru,
    /// AVI sem `idx1` (gravação interrompida).
    AviSemIndice,
}

impl Formato {
    pub fn rotulo(self) -> &'static str {
        match self {
            Formato::Dahua => "Dahua (DAV)",
            Formato::HikvisionImkh => "Hikvision (IMKH)",
            Formato::HikvisionAntigo => "Hikvision antigo (4HKH)",
            Formato::Ifv => "IFV",
            Formato::Lvf => "LVF",
            Formato::H264Cru => "H.264 sem contêiner",
            Formato::HevcCru => "HEVC sem contêiner",
            Formato::AviSemIndice => "AVI sem índice",
        }
    }

    /// Demuxer do ffmpeg que lê o formato; None = nenhum.
    pub fn demuxer(self) -> Option<&'static str> {
        match self {
            Formato::Dahua => Some("dhav"),
            Formato::HikvisionImkh => Some("mpeg"),
            Formato::HikvisionAntigo => None,
            Formato::Ifv => Some("ifv"),
            Formato::Lvf => Some("lvf"),
            Formato::H264Cru => Some("h264"),
            Formato::HevcCru => Some("hevc"),
            Formato::AviSemIndice => Some("avi"),
        }
    }

    pub fn cru(self) -> bool {
        matches!(self, Formato::H264Cru | Formato::HevcCru)
    }
}

fn validacao(codigo: &str, detalhe: Option<&str>) -> SicroError {
    SicroError::Validation(match detalhe {
        Some(d) => format!("{codigo}: {d}"),
        None => codigo.to_string(),
    })
}

/// Recusa o que o ffmpeg não abre: `gravador_nao_suportado: <rótulo>`.
pub fn exigir_suportado(f: Formato) -> Result<()> {
    match f.demuxer() {
        Some(_) => Ok(()),
        None => Err(validacao(codigo::GRAVADOR_NAO_SUPORTADO, Some(f.rotulo()))),
    }
}

// ---------------------------------------------------------------- reconhecer

fn ler_ate(f: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// Formato de gravador do arquivo; None = vídeo comum (ou outro arquivo).
pub fn reconhecer(caminho: &Path) -> Result<Option<Formato>> {
    let mut f = File::open(caminho).map_err(|e| SicroError::Filesystem(format!("{}: {e}", caminho.display())))?;
    let tamanho = f.metadata()?.len();
    let mut cabeca = [0u8; 64];
    let n = ler_ate(&mut f, &mut cabeca)?;
    let c = &cabeca[..n];
    if let Some(x) = pela_cabeca(c) {
        return Ok(Some(x));
    }
    if c.len() >= 12 && &c[0..4] == b"RIFF" && &c[8..12] == b"AVI " && avi_sem_indice(&mut f, tamanho)? {
        return Ok(Some(Formato::AviSemIndice));
    }
    Ok(None)
}

/// Assinaturas de `libavformat/{dhav,mpeg,ifv,lvfdec}.c` e o fluxo cru.
fn pela_cabeca(c: &[u8]) -> Option<Formato> {
    if c.starts_with(b"DAHUA") || (c.starts_with(b"DHAV") && c.get(4).is_some_and(|b| matches!(b, 0xF0 | 0xF1 | 0xFC | 0xFD))) {
        return Some(Formato::Dahua);
    }
    if c.starts_with(b"IMKH") {
        return Some(Formato::HikvisionImkh);
    }
    if c.starts_with(b"4HKH") {
        return Some(Formato::HikvisionAntigo);
    }
    if c.starts_with(&IFV) {
        return Some(Formato::Ifv);
    }
    if c.starts_with(b"LVFF") {
        return Some(Formato::Lvf);
    }
    fluxo_cru(c)
}

/// Código de início no byte 0 e uma unidade NAL que abre fluxo. HEVC: VPS, SPS, PPS, AUD ou SEI na camada 0 (nenhum
/// desses dois bytes é começo válido de H.264). H.264: SEI e AUD com `nal_ref_idc` 0; SPS, PPS e IDR com ele ≠ 0
/// (assim a caixa de 64 bits de um MP4, `00 00 00 01` + nome, não passa por fluxo cru).
fn fluxo_cru(c: &[u8]) -> Option<Formato> {
    let i = if c.starts_with(&[0, 0, 0, 1]) {
        4
    } else if c.starts_with(&[0, 0, 1]) {
        3
    } else {
        return None;
    };
    let (b0, b1) = (*c.get(i)?, *c.get(i + 1)?);
    if b0 & 0x80 != 0 {
        return None;
    }
    let tipo = (b0 >> 1) & 0x3F;
    let camada = ((b0 & 1) << 5) | (b1 >> 3);
    if matches!(tipo, 32..=35 | 39) && camada == 0 && b1 & 7 != 0 {
        return Some(Formato::HevcCru);
    }
    let referencia = b0 & 0x60 != 0;
    match (b0 & 0x1F, referencia) {
        (6 | 9, false) | (5 | 7 | 8, true) => Some(Formato::H264Cru),
        _ => None,
    }
}

/// AVI sem índice: nenhum `idx1` entre os pedaços do topo e nenhum `indx` (OpenDML) no `hdrl`. Arquivo que acaba no
/// meio de um pedaço (gravação interrompida) não chega ao `idx1`.
fn avi_sem_indice(f: &mut File, tamanho: u64) -> Result<bool> {
    let mut pos = 12u64;
    for _ in 0..MAX_PEDACOS {
        if pos + 8 > tamanho {
            return Ok(true);
        }
        f.seek(SeekFrom::Start(pos))?;
        let mut h = [0u8; 12];
        let n = ler_ate(f, &mut h)?;
        if n < 8 {
            return Ok(true);
        }
        let tam = u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as u64;
        match &h[0..4] {
            b"idx1" => return Ok(false),
            b"LIST" if n == 12 && &h[8..12] == b"hdrl" => {
                let mut corpo = vec![0u8; tam.saturating_sub(4).min(MAX_HDRL) as usize];
                let k = ler_ate(f, &mut corpo)?;
                if corpo[..k].windows(4).any(|w| w == b"indx") {
                    return Ok(false);
                }
            }
            _ => {}
        }
        pos += 8 + tam + (tam & 1);
    }
    // pedaços demais: fica com o ffmpeg, como AVI comum
    Ok(false)
}

// ---------------------------------------------------------------- ffmpeg

/// Pasta de trabalho de uma cópia (stderr dos processos), apagada ao sair.
struct Trabalho(PathBuf);

impl Trabalho {
    fn nova() -> Result<Self> {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!("sicro-dvr-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&p)?;
        Ok(Trabalho(p))
    }

    fn comando(&self, nome: &str, programa: &Path, args: Vec<String>) -> Comando {
        Comando {
            nome: nome.into(),
            programa: programa.to_path_buf(),
            args,
            pasta: self.0.clone(),
            ambiente: Vec::new(),
            sem_ambiente: Vec::new(),
            stderr: self.0.join(format!("{nome}.stderr")),
        }
    }
}

impl Drop for Trabalho {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn textos(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn mdc(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

fn fracao(texto: &str) -> Option<Racional> {
    let (a, b) = texto.split_once('/')?;
    let (a, b): (i64, i64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    (a > 0 && b > 0).then(|| Racional { num: a / mdc(a, b), den: b / mdc(a, b) })
}

fn numero(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).filter(|x: &f64| x.is_finite())
}

fn ffprobe_json(cap: &Capacidades, t: &Trabalho, nome: &str, args: Vec<String>) -> Result<Value> {
    let cmd = t.comando(nome, &cap.ffprobe, args);
    let (fim, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false))?;
    exec::exigir_sucesso(&cmd, &fim)?;
    Ok(serde_json::from_slice(&out)?)
}

/// `format.start_time` e `format.duration` (s); None no que o contêiner não traz.
fn inicio_e_duracao(cap: &Capacidades, t: &Trabalho, video: &Path) -> Result<(Option<f64>, Option<f64>)> {
    let j = ffprobe_json(cap, t, "inicio", textos(&["-v", "error", "-show_entries", "format=start_time,duration", "-of", "json", &video.to_string_lossy()]))?;
    Ok((numero(&j["format"]["start_time"]), numero(&j["format"]["duration"]).filter(|d| *d > 0.0)))
}

/// Taxa padrão do demuxer de fluxo cru (`-framerate`), a que vale quando o SPS não traz temporização.
const TAXA_PADRAO: Racional = Racional { num: 25, den: 1 };

/// Taxa do fluxo cru pela duração que o analisador dá ao primeiro pacote com `-framerate` posto. Medida com duas taxas
/// postas: a temporização do SPS vence o `-framerate`; sem ela, a duração segue o que foi posto (25 ≠ 7) e a taxa é
/// suposta (a padrão, 25). Fora de 1 a 240 qps: 25, suposta. Volta (taxa, suposta).
fn taxa_do_fluxo(cap: &Capacidades, t: &Trabalho, video: &Path, f: Formato) -> Result<(Racional, bool)> {
    let demuxer = f.demuxer().unwrap_or("h264");
    let medir = |posta: &str| -> Result<Option<Racional>> {
        let j = ffprobe_json(
            cap,
            t,
            "taxa",
            textos(&[
                "-v", "error", "-framerate", posta, "-f", demuxer, "-select_streams", "v:0", "-show_entries", "stream=time_base:packet=duration",
                "-read_intervals", "%+#2", "-of", "json", &video.to_string_lossy(),
            ]),
        )?;
        let tb = j["streams"][0]["time_base"].as_str().and_then(fracao);
        let dur = j["packets"].as_array().and_then(|p| p.iter().find_map(|x| numero(&x["duration"]))).filter(|d| *d >= 1.0).map(|d| d as i64);
        Ok(match (tb, dur) {
            (Some(tb), Some(d)) => {
                let (num, den) = (tb.den, tb.num.saturating_mul(d));
                Some(Racional { num: num / mdc(num, den), den: den / mdc(num, den) })
            }
            _ => None,
        })
    };
    let (a, b) = (medir("25/1")?, medir("7/1")?);
    match a.filter(|r| r.num >= r.den && r.num <= 240 * r.den) {
        Some(r) if a == b => Ok((r, false)),
        _ => Ok((TAXA_PADRAO, true)),
    }
}

/// Opções de entrada (antes do `-i`): fluxo cru vai pelo demuxer com a taxa posta.
fn entrada(f: Formato, taxa: Option<Racional>) -> Vec<String> {
    match (f.cru(), taxa) {
        (true, Some(r)) => vec!["-framerate".into(), format!("{}/{}", r.num, r.den), "-f".into(), f.demuxer().unwrap_or("h264").into()],
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------- avanço

/// Fase da cópia que a tela mostra.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Fase {
    /// Gravando o MKV (remux; FFV1 no fluxo cru com quadros B).
    Copia,
    /// Decodificando original e cópia para as listas `framemd5`.
    Prova,
}

/// Recebe a fase e a fração feita (0 a 1; None = sem como medir).
pub type AoAvanco<'a> = &'a (dyn Fn(Fase, Option<f64>) + Sync);

/// Intervalo do relator: avanço e repasse do pedido de cancelamento aos processos.
const RELATOR: Duration = Duration::from_millis(50);
/// Menor mudança de fração que vai para a tela.
const PASSO_AVANCO: f64 = 0.005;

/// f64 ≥ 0 num átomo; nos não negativos a ordem dos bits é a dos números (`fetch_max`).
struct Numero(AtomicU64);

impl Numero {
    fn novo() -> Self {
        Numero(AtomicU64::new(0))
    }

    fn ler(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }

    fn por(&self, x: f64) {
        self.0.store(if x.is_finite() && x > 0.0 { x } else { 0.0 }.to_bits(), Ordering::Relaxed);
    }

    fn subir(&self, x: f64) {
        if x.is_finite() && x > 0.0 {
            self.0.fetch_max(x.to_bits(), Ordering::Relaxed);
        }
    }
}

/// O que o relator lê enquanto a cópia anda.
struct Andamento {
    prova: AtomicBool,
    /// Fração da gravação; `copia_medida` false = sem como medir.
    copia: Numero,
    copia_medida: AtomicBool,
    /// Duração da cópia gravada (s); 0 = ainda não se sabe.
    duracao: Numero,
    /// Vídeo já decodificado (s), do original e da cópia.
    original: Numero,
    lida: Numero,
    /// O que os processos olham: o pedido de quem chamou (repassado pelo relator) ou a falha de um dos lados.
    parar: AtomicBool,
    fim: AtomicBool,
}

impl Andamento {
    fn novo() -> Self {
        Andamento {
            prova: AtomicBool::new(false),
            copia: Numero::novo(),
            copia_medida: AtomicBool::new(false),
            duracao: Numero::novo(),
            original: Numero::novo(),
            lida: Numero::novo(),
            parar: AtomicBool::new(false),
            fim: AtomicBool::new(false),
        }
    }

    fn agora(&self) -> (Fase, Option<f64>) {
        if self.prova.load(Ordering::SeqCst) {
            (Fase::Prova, fracao_da_prova(self.original.ler(), self.lida.ler(), self.duracao.ler()))
        } else {
            (Fase::Copia, self.copia_medida.load(Ordering::SeqCst).then(|| self.copia.ler().min(1.0)))
        }
    }
}

/// Fração da prova: o lado mais atrasado (original ou cópia) sobre a duração da cópia.
fn fracao_da_prova(original_s: f64, copia_s: f64, duracao_s: f64) -> Option<f64> {
    (duracao_s > 0.0).then(|| (original_s.min(copia_s) / duracao_s).clamp(0.0, 1.0))
}

/// Laço do relator até o fim do trabalho: repassa o cancelamento e manda o avanço quando muda.
fn relatar(a: &Andamento, cancelar: &AtomicBool, avanco: AoAvanco) {
    let mut ultimo: Option<(Fase, Option<f64>)> = None;
    loop {
        if cancelar.load(Ordering::SeqCst) {
            a.parar.store(true, Ordering::SeqCst);
        }
        let fim = a.fim.load(Ordering::SeqCst);
        let atual = a.agora();
        let mudou = match ultimo {
            None => true,
            Some((f, x)) => {
                f != atual.0
                    || match (x, atual.1) {
                        (Some(x), Some(y)) => (y - x).abs() >= PASSO_AVANCO,
                        (x, y) => x.is_some() != y.is_some(),
                    }
            }
        };
        if mudou && !a.parar.load(Ordering::SeqCst) {
            avanco(atual.0, atual.1);
            ultimo = Some(atual);
        }
        if fim {
            return;
        }
        std::thread::park_timeout(RELATOR);
    }
}

/// Marca o fim ao sair, inclusive em pânico, e acorda a thread que espera por ele.
struct AoSair<'a>(&'a AtomicBool, std::thread::Thread);

impl Drop for AoSair<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
        self.1.unpark();
    }
}

fn cancelado() -> SicroError {
    SicroError::Validation("cancelado".into())
}

/// SHA-256 em blocos, parando quando `parar` vira true (o arquivo de um gravador passa de gigabytes).
fn sha256_parando(caminho: &Path, parar: &AtomicBool) -> Result<String> {
    let mut f = File::open(caminho).map_err(|e| SicroError::Filesystem(format!("{}: {e}", caminho.display())))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if parar.load(Ordering::Relaxed) {
            return Err(cancelado());
        }
        match f.read(&mut buf) {
            Ok(0) => return Ok(format!("{:x}", h.finalize())),
            Ok(n) => h.update(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(SicroError::Filesystem(format!("{}: {e}", caminho.display()))),
        }
    }
}

/// Resumo de um fluxo da lista `framemd5`: contagem e SHA-256 das linhas (tamanho, md5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Resumo {
    pub quadros: u64,
    pub sha256: String,
}

/// Lista `framemd5` de um arquivo: vídeo (fluxo 0), áudio (fluxo 1, quando há) e os pts do vídeo na base `tb`.
#[derive(Debug, Clone)]
pub struct Lista {
    pub video: Resumo,
    pub audio: Option<Resumo>,
    pub tb: Option<Racional>,
    /// Vazio quando algum quadro veio sem pts.
    pub pts: Vec<i64>,
    /// pts do primeiro quadro de som, na base `tb_audio`: onde o som está em relação à imagem.
    pub audio_pts0: Option<i64>,
    pub tb_audio: Option<Racional>,
}

/// Leitura em fluxo das linhas do `framemd5` (o arquivo de um dia de gravação não cabe inteiro na memória como texto).
/// `lido` recebe o trecho de vídeo já decodificado, em s (maior pts − primeiro): é o avanço da prova.
struct Leitor<'a> {
    h: [Sha256; 2],
    n: [u64; 2],
    tb: Option<Racional>,
    tb_audio: Option<Racional>,
    pts: Vec<i64>,
    pts_ok: bool,
    audio_pts0: Option<i64>,
    resto: Vec<u8>,
    pts0: Option<i64>,
    lido: Option<&'a Numero>,
}

impl<'a> Leitor<'a> {
    fn novo(lido: Option<&'a Numero>) -> Self {
        Leitor {
            h: [Sha256::new(), Sha256::new()],
            n: [0, 0],
            tb: None,
            tb_audio: None,
            pts: Vec::new(),
            pts_ok: true,
            audio_pts0: None,
            resto: Vec::new(),
            pts0: None,
            lido,
        }
    }

    fn pedaco(&mut self, b: &[u8]) -> Result<()> {
        self.resto.extend_from_slice(b);
        let Some(fim) = self.resto.iter().rposition(|&c| c == b'\n') else { return Ok(()) };
        let prontas: Vec<u8> = self.resto.drain(..=fim).collect();
        for l in String::from_utf8_lossy(&prontas).lines() {
            self.linha(l)?;
        }
        Ok(())
    }

    fn linha(&mut self, l: &str) -> Result<()> {
        if let Some(x) = l.strip_prefix("#tb 0:") {
            self.tb = fracao(x.trim());
            return Ok(());
        }
        if let Some(x) = l.strip_prefix("#tb 1:") {
            self.tb_audio = fracao(x.trim());
            return Ok(());
        }
        if l.starts_with('#') || l.trim().is_empty() {
            return Ok(());
        }
        let c: Vec<&str> = l.split(',').map(str::trim).collect();
        let (Some(fluxo), Some(pts), Some(tamanho), Some(md5)) = (c.first(), c.get(2), c.get(4), c.get(5)) else {
            return Err(SicroError::Validation(format!("framemd5 ilegível: {l}")));
        };
        let k = match *fluxo {
            "0" => 0,
            "1" => 1,
            _ => return Ok(()),
        };
        self.h[k].update(format!("{tamanho},{md5}\n").as_bytes());
        self.n[k] += 1;
        if k == 1 && self.n[1] == 1 {
            self.audio_pts0 = pts.parse().ok();
        }
        if k == 0 {
            if self.pts.len() >= MAX_QUADROS {
                return Err(SicroError::Validation(codigo::TABELA_GRANDE.into()));
            }
            match pts.parse::<i64>() {
                Ok(p) => {
                    self.pts.push(p);
                    let p0 = *self.pts0.get_or_insert(p);
                    if let (Some(l), Some(tb)) = (self.lido, self.tb) {
                        l.subir((p - p0) as f64 * tb.num as f64 / tb.den as f64);
                    }
                }
                Err(_) => self.pts_ok = false,
            }
        }
        Ok(())
    }

    fn fim(mut self, com_audio: bool) -> Result<Lista> {
        if !self.resto.is_empty() {
            let r = std::mem::take(&mut self.resto);
            for l in String::from_utf8_lossy(&r).lines() {
                self.linha(l)?;
            }
        }
        let [hv, ha] = self.h;
        let resumo = |h: Sha256, n: u64| Resumo { quadros: n, sha256: format!("{:x}", h.finalize()) };
        Ok(Lista {
            video: resumo(hv, self.n[0]),
            audio: com_audio.then(|| resumo(ha, self.n[1])),
            tb: self.tb,
            pts: if self.pts_ok { self.pts } else { Vec::new() },
            audio_pts0: if com_audio { self.audio_pts0 } else { None },
            tb_audio: if com_audio { self.tb_audio } else { None },
        })
    }
}

/// Tem fluxo de áudio? `entrada` leva o `-f` do fluxo cru.
fn tem_audio(cap: &Capacidades, t: &Trabalho, video: &Path, entrada: &[String]) -> Result<bool> {
    let mut a = textos(&["-v", "error"]);
    if let Some(i) = entrada.iter().position(|x| x == "-f") {
        a.extend(entrada.iter().skip(i).take(2).cloned());
    }
    a.extend(textos(&["-select_streams", "a", "-show_entries", "stream=index", "-of", "json", &video.to_string_lossy()]));
    let j = ffprobe_json(cap, t, "audio", a)?;
    Ok(j["streams"].as_array().is_some_and(|s| !s.is_empty()))
}

/// Lista `framemd5` decodificando em uma thread (o decodificador com várias threads entrega outros quadros em fluxo
/// danificado: LVF truncado), pts sem arredondar (`-enc_time_base demux`), `-copyts`. Áudio que não decodifica: só o vídeo.
pub fn lista_framemd5(cap: &Capacidades, video: &Path, entrada: &[String], cancelar: &AtomicBool) -> Result<Lista> {
    let audio = tem_audio(cap, &Trabalho::nova()?, video, entrada)?;
    lista(cap, video, entrada, audio, cancelar, None)
}

/// `lista_framemd5` com o áudio já sondado e o avanço em `lido` (s de vídeo decodificados).
fn lista(cap: &Capacidades, video: &Path, entrada: &[String], audio: bool, cancelar: &AtomicBool, lido: Option<&Numero>) -> Result<Lista> {
    let t = Trabalho::nova()?;
    let rodar = |com_audio: bool| -> Result<Option<Lista>> {
        let mut a = textos(&["-hide_banner", "-nostdin", "-nostats", "-loglevel", "error", "-threads", "1"]);
        a.extend(entrada.iter().cloned());
        a.extend(textos(&["-copyts", "-i", &video.to_string_lossy(), "-map", "0:v:0"]));
        if com_audio {
            a.extend(textos(&["-map", "0:a:0"]));
        }
        a.extend(textos(&["-fps_mode", "passthrough", "-enc_time_base", "demux", "-f", "framemd5", "-"]));
        let cmd = t.comando(if com_audio { "framemd5-va" } else { "framemd5-v" }, &cap.binario, a);
        let mut leitor = Leitor::novo(lido);
        let mut falha = None;
        let fim = exec::rodar_com_fluxo(&cmd, cancelar, &mut |b| match leitor.pedaco(b) {
            Ok(()) => Ok(true),
            Err(e) => {
                falha = Some(e);
                Ok(false)
            }
        })?;
        if let Some(e) = falha {
            return Err(e);
        }
        if fim.cancelado {
            return Err(SicroError::Validation("cancelado".into()));
        }
        // 69 = o ffmpeg desistiu por excesso de erros de decodificação: a lista vale até onde ele foi
        match fim.codigo {
            Some(0 | 69) => Ok(Some(leitor.fim(com_audio)?)),
            _ if com_audio => Ok(None),
            _ => exec::exigir_sucesso(&cmd, &fim).map(|_| None),
        }
    };
    if audio {
        if let Some(l) = rodar(true)? {
            return Ok(l);
        }
    }
    rodar(false)?.ok_or_else(|| SicroError::Validation(codigo::OUTRA.into()))
}

/// Intervalo entre dois quadros vizinhos acima de 1,5× o passo mediano (§8 das receitas), em tempo do original.
/// `quadros` = estimativa dos que faltam: round(intervalo/mediano) − 1, ao menos 1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Lacuna {
    pub de_s: f64,
    pub ate_s: f64,
    pub quadros: u64,
}

/// Passo mediano (ticks) e lacunas dos pts (em ordem de exibição).
pub fn lacunas(pts: &[i64], tb: Racional) -> (Option<i64>, Vec<Lacuna>) {
    let passos: Vec<i64> = pts.windows(2).map(|p| p[1] - p[0]).collect();
    if passos.is_empty() {
        return (None, Vec::new());
    }
    let mut ord = passos.clone();
    ord.sort_unstable();
    let med = ord[ord.len() / 2].max(1);
    let s = |p: i64| p as f64 * tb.num as f64 / tb.den as f64;
    let v = pts
        .windows(2)
        .filter(|p| 2 * (p[1] - p[0]) > 3 * med)
        .map(|p| {
            let d = p[1] - p[0];
            Lacuna { de_s: s(p[0]), ate_s: s(p[1]), quadros: (((2 * d + med) / (2 * med)) - 1).max(1) as u64 }
        })
        .collect();
    (Some(med), v)
}

/// Deslocamento constante entre os pts do original e os da cópia, em s (t_original = t_cópia + desloc), com folga de
/// um tique da base da CÓPIA (o MKV arredonda a 1 ms; o pts do original é exato). A folga não é a base mais grossa:
/// no AVI (1/25) ela deixaria um quadro inteiro fora do passo (AVI com quadros B e `+genpts`: 80, 120, 160, 200 ms
/// viram 40, 120, 160, 160). None = algum quadro saiu do passo.
fn deslocamento(o: &Lista, c: &Lista) -> Option<f64> {
    let (to, tc) = (o.tb?, c.tb?);
    if o.pts.len() != c.pts.len() || o.pts.is_empty() {
        return None;
    }
    // em unidades de 1/(den_o·den_c)
    let d = |j: usize| o.pts[j] as i128 * to.num as i128 * tc.den as i128 - c.pts[j] as i128 * tc.num as i128 * to.den as i128;
    let folga = tc.num as i128 * to.den as i128;
    let d0 = d(0);
    (0..o.pts.len()).all(|j| (d(j) - d0).abs() <= folga).then(|| d0 as f64 / (to.den as f64 * tc.den as f64))
}

/// O som no mesmo lugar em relação à imagem: o primeiro quadro de som desloca o mesmo que o vídeo (`desloc_s`), com
/// folga de um tique de cada base da cópia (os dois arredondados). Sem pts de som nos dois: nada a comparar.
fn som_no_lugar(o: &Lista, c: &Lista, desloc_s: f64) -> bool {
    let (Some(ao), Some(ac), Some(tao), Some(tac)) = (o.audio_pts0, c.audio_pts0, o.tb_audio, c.tb_audio) else { return true };
    let s = |p: i64, tb: Racional| p as f64 * tb.num as f64 / tb.den as f64;
    let folga = c.tb.map_or(0.0, |tb| s(1, tb)) + s(1, tac);
    // + 1 µs: em época o f64 tem passo de 2,4e-7 s
    ((s(ao, tao) - s(ac, tac)) - desloc_s).abs() <= folga + 1e-6
}

/// Prova da cópia: os mesmos quadros (vídeo e som), os pts do vídeo no mesmo passo e o som no mesmo lugar em
/// relação à imagem. Ok = o deslocamento (None no fluxo cru e no original com quadro sem pts, em que o `+genpts`
/// inventa o tempo); Err = o detalhe do `copia_divergente`.
fn provar(formato: Formato, o: &Lista, c: Option<&Lista>) -> std::result::Result<Option<f64>, String> {
    let c = match c {
        Some(c) if mesmo(o, c) => c,
        outra => return Err(format!("{} × {} quadros", o.video.quadros, outra.map_or(0, |c| c.video.quadros))),
    };
    if formato.cru() || o.pts.is_empty() {
        return Ok(None);
    }
    let d = deslocamento(o, c).ok_or("tempos")?;
    if !som_no_lugar(o, c, d) {
        return Err("tempos do som".into());
    }
    Ok(Some(d))
}

/// Como a cópia é gravada; a primeira é a da receita, a outra só entra se a prova da primeira falha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modo {
    Remux,
    /// AVI sem quadros B: o `+genpts` põe o vídeo um quadro depois do som (o original decodifica os dois em 0).
    RemuxSemGenpts,
    /// Fluxo cru com quadros B: o remux não grava pacote sem tempo (código 234).
    Ffv1,
}

/// A cópia feita e a prova.
#[derive(Debug, Clone, Serialize)]
pub struct Copia {
    pub formato: Formato,
    /// Programa e argumentos, como rodou.
    pub comando: Vec<String>,
    /// Fluxo cru com quadros B: FFV1 sem perdas (o remux não grava pacote sem tempo).
    pub recodificado: bool,
    /// Fluxo cru: a taxa posta no `-framerate`.
    pub taxa: Option<Racional>,
    /// Fluxo cru sem temporização no SPS: `taxa` é a padrão do demuxer (25), não do arquivo.
    pub taxa_suposta: bool,
    pub quadros: u64,
    pub framemd5_sha256: String,
    pub audio: Option<Resumo>,
    /// `format.start_time` do original.
    pub inicio_s: Option<f64>,
    /// O mesmo, quando é data do relógio do gravador.
    pub inicio_epoca_s: Option<f64>,
    /// t_original = t_cópia + `desloc_s`; None no fluxo cru (o original não tem tempos).
    pub desloc_s: Option<f64>,
    pub passo_mediano_s: Option<f64>,
    pub lacunas: Vec<Lacuna>,
}

fn mesmo(o: &Lista, c: &Lista) -> bool {
    o.video == c.video && o.video.quadros > 0 && o.audio == c.audio
}

/// Erro que apaga o destino antes de subir.
fn apagando<T>(destino: &Path, r: Result<T>) -> Result<T> {
    if r.is_err() {
        let _ = std::fs::remove_file(destino);
    }
    r
}

/// Faz a cópia de trabalho de `origem` em `destino` (MKV) e prova pelos quadros e pelos tempos (`provar`). Prova
/// que falha apaga o destino e devolve `copia_divergente`. Fluxo cru: remux com a taxa do fluxo e, se não prova,
/// FFV1 sem perdas; contêiner: remux da receita e, se não prova, o mesmo sem `+genpts` (AVI sem índice com som: nesta
/// ordem invertida). `cancelar` mata os processos; cancelada, o destino some e o erro é `cancelado`.
pub fn copiar(cap: &Capacidades, origem: &Path, formato: Formato, destino: &Path, cancelar: &AtomicBool) -> Result<Copia> {
    copiar_com(cap, origem, formato, destino, cancelar, &|_, _| {})
}

/// `copiar` mandando o avanço a `avanco`: fase Cópia (pts gravado sobre a duração do original; sem ela, bytes) e fase
/// Prova (pts lido do original e da cópia sobre a duração da cópia, o mais atrasado).
pub fn copiar_com(cap: &Capacidades, origem: &Path, formato: Formato, destino: &Path, cancelar: &AtomicBool, avanco: AoAvanco) -> Result<Copia> {
    exigir_suportado(formato)?;
    let a = Andamento::novo();
    std::thread::scope(|s| {
        let relator = s.spawn(|| relatar(&a, cancelar, avanco));
        let _fim = AoSair(&a.fim, relator.thread().clone());
        let r = copiar_andando(cap, origem, formato, destino, &a);
        // cancelada: o erro é o do pedido, qualquer que seja o processo que ele derrubou
        if cancelar.load(Ordering::SeqCst) {
            return apagando(destino, Err(cancelado()));
        }
        r
    })
}

fn copiar_andando(cap: &Capacidades, origem: &Path, formato: Formato, destino: &Path, a: &Andamento) -> Result<Copia> {
    let cancelar = &a.parar;
    let t = Trabalho::nova()?;
    let (taxa, taxa_suposta) = match formato.cru() {
        true => taxa_do_fluxo(cap, &t, origem, formato).map(|(r, s)| (Some(r), s))?,
        false => (None, false),
    };
    let ent = entrada(formato, taxa);
    let (inicio, duracao) = if formato.cru() { (None, None) } else { inicio_e_duracao(cap, &t, origem)? };
    let audio = tem_audio(cap, &t, origem, &ent)?;
    let tamanho = std::fs::metadata(origem).map(|m| m.len()).unwrap_or(0).max(1);
    // AVI sem índice com som: o `+genpts` põe o vídeo um quadro depois do som e a receita nunca prova
    let modos = match (formato, audio) {
        (f, _) if f.cru() => [Modo::Remux, Modo::Ffv1],
        (Formato::AviSemIndice, true) => [Modo::RemuxSemGenpts, Modo::Remux],
        _ => [Modo::Remux, Modo::RemuxSemGenpts],
    };
    // o que um modo gravou: comando como rodou, a lista da cópia (None = não gravou ou não se lê) e o erro do
    // processo quando ele não terminou bem
    struct Feita {
        modo: Modo,
        comando: Vec<String>,
        lista: Option<Lista>,
        falha: Option<SicroError>,
    }
    // `quadros` = os do original, quando já contados: avanço do FFV1, que não tem duração nem bytes comparáveis
    let fazer = |modo: Modo, quadros: Option<u64>| -> Result<Feita> {
        a.prova.store(false, Ordering::SeqCst);
        a.copia.por(0.0);
        a.lida.por(0.0);
        let mut v = textos(&["-hide_banner", "-nostdin", "-nostats", "-loglevel", "error", "-y"]);
        v.extend(ent.iter().cloned());
        if modo != Modo::RemuxSemGenpts {
            v.extend(textos(&["-fflags", "+genpts"]));
        }
        v.extend(textos(&["-i", &origem.to_string_lossy(), "-map", "0:v", "-map", "0:a?"]));
        if modo == Modo::Ffv1 {
            v.extend(textos(&["-c:v", "ffv1", "-level", "3", "-g", "1", "-c:a", "copy"]));
        } else {
            v.extend(textos(&["-c", "copy", "-copyinkf"]));
        }
        // `+bitexact`: sem os identificadores aleatórios do MKV, a mesma cópia dá os mesmos bytes
        v.extend(textos(&["-map_metadata", "0", "-fflags", "+bitexact", "-f", "matroska", &destino.to_string_lossy()]));
        let nome = match modo {
            Modo::Remux => "remux",
            Modo::RemuxSemGenpts => "remux-sem-genpts",
            Modo::Ffv1 => "ffv1",
        };
        let cmd = t.comando(nome, &cap.binario, v);
        let mut gravado_us = 0i64;
        let fim = exec::rodar_com_progresso(&cmd, cancelar, &mut |p| {
            gravado_us = gravado_us.max(p.tempo_us.unwrap_or(0));
            let f = match (p.tempo_us, duracao, quadros) {
                _ if p.fim => Some(1.0),
                (Some(us), Some(d), _) => Some(us as f64 / 1e6 / d),
                (_, _, Some(n)) if n > 0 => Some(p.quadro as f64 / n as f64),
                _ if modo != Modo::Ffv1 => std::fs::metadata(destino).ok().map(|m| m.len() as f64 / tamanho as f64),
                _ => None,
            };
            a.copia.por(f.unwrap_or(0.0).min(1.0));
            a.copia_medida.store(f.is_some(), Ordering::SeqCst);
        })?;
        if fim.cancelado {
            return Err(cancelado());
        }
        let mut comando = vec![cap.binario.to_string_lossy().into_owned()];
        comando.extend(cmd.args.iter().cloned());
        if let Err(falha) = exec::exigir_sucesso(&cmd, &fim) {
            return Ok(Feita { modo, comando, lista: None, falha: Some(falha) });
        }
        // denominador da prova: o tempo gravado (sem ele, a duração que o MKV declara)
        let gravado = (gravado_us > 0).then(|| gravado_us as f64 / 1e6);
        a.duracao.por(gravado.or_else(|| inicio_e_duracao(cap, &t, destino).ok().and_then(|x| x.1)).unwrap_or(0.0));
        a.prova.store(true, Ordering::SeqCst);
        // cópia que nem se lê (IFV sem `-copyinkf`: 0 pacotes de vídeo, código 0) é cópia divergente
        let lista = match tem_audio(cap, &t, destino, &[]).and_then(|au| lista(cap, destino, &[], au, cancelar, Some(&a.lida))) {
            Ok(l) => Some(l),
            Err(e) if cancelar.load(Ordering::Relaxed) => return Err(e),
            Err(_) => None,
        };
        Ok(Feita { modo, comando, lista, falha: None })
    };
    // a lista do original sai enquanto a cópia é feita; a cópia que não sai (erro, não divergência) para o original
    let (original, primeira) = std::thread::scope(|s| {
        let o = s.spawn(|| lista(cap, origem, &ent, audio, cancelar, Some(&a.original)));
        let c = fazer(modos[0], None);
        if c.is_err() {
            a.parar.store(true, Ordering::SeqCst);
        }
        (o.join().unwrap_or_else(|_| Err(SicroError::Validation(codigo::PANICO.into()))), c)
    });
    let primeira = apagando(destino, primeira)?;
    let original = apagando(destino, original)?;
    // o original já foi inteiro: o avanço da prova é o da cópia
    a.original.por(f64::MAX);
    let (feita, desloc_s) = match provar(formato, &original, primeira.lista.as_ref()) {
        Ok(d) => (primeira, d),
        Err(detalhe1) => {
            let segunda = apagando(destino, fazer(modos[1], Some(original.video.quadros)))?;
            match provar(formato, &original, segunda.lista.as_ref()) {
                Ok(d) => (segunda, d),
                Err(detalhe2) => {
                    // nenhuma gravou: o erro do processo; a segunda não gravou: o que a primeira mostrou
                    let e = match (&primeira.lista, segunda.falha) {
                        (None, Some(f)) => f,
                        (Some(_), Some(_)) => validacao(codigo::COPIA_DIVERGENTE, Some(&detalhe1)),
                        (_, None) => validacao(codigo::COPIA_DIVERGENTE, Some(&detalhe2)),
                    };
                    return apagando(destino, Err(e));
                }
            }
        }
    };
    let (comando, recodificado) = (feita.comando, feita.modo == Modo::Ffv1);
    let (med, lac) = match (formato.cru(), original.tb) {
        (false, Some(tb)) => {
            let (m, l) = lacunas(&original.pts, tb);
            (m.map(|m| m as f64 * tb.num as f64 / tb.den as f64), l)
        }
        _ => (None, Vec::new()),
    };
    Ok(Copia {
        formato,
        comando,
        recodificado,
        taxa,
        taxa_suposta,
        quadros: original.video.quadros,
        framemd5_sha256: original.video.sha256.clone(),
        audio: original.audio.clone(),
        inicio_s: inicio,
        inicio_epoca_s: inicio.filter(|s| EPOCA.contains(s)),
        desloc_s,
        passo_mediano_s: med,
        lacunas: lac,
    })
}

// ---------------------------------------------------------------- caso

/// A cópia registrada no caso (o que a tela de importação recebe e abre).
#[derive(Debug, Clone, Serialize)]
pub struct CopiaRegistrada {
    pub media: VideoMedia,
    pub formato: Formato,
    pub lacunas: Vec<Lacuna>,
    pub inicio_epoca_s: Option<f64>,
    /// A cópia já estava no caso (anexo repetido): nada foi feito.
    pub ja_existia: bool,
}

/// Cópia de trabalho já registrada para o original de SHA-256 `sha`, com o arquivo presente.
fn copia_existente(conn: &Connection, ws: &Path, occ: &Uuid, sha: &str) -> Result<Option<VideoMedia>> {
    Ok(video_repo::list_media_for_occurrence(conn, occ)?.into_iter().find(|m| {
        m.derived_from_hash.as_deref() == Some(sha)
            && m.derivation_json.as_deref().and_then(|j| serde_json::from_str::<Value>(j).ok()).is_some_and(|j| j["kind"] == "copia_dvr")
            && resolver_relativo(ws, &m.relative_path).is_file()
    }))
}

fn de_derivacao(m: &VideoMedia) -> (Vec<Lacuna>, Option<f64>) {
    let j: Value = m.derivation_json.as_deref().and_then(|j| serde_json::from_str(j).ok()).unwrap_or(Value::Null);
    (serde_json::from_value(j["lacunas"].clone()).unwrap_or_default(), numero(&j["inicio_epoca_s"]))
}

/// `<nome>.mkv` livre na pasta (`nome (2).mkv`… se já houver).
fn nome_livre(pasta: &Path, base: &str) -> String {
    let base: String = base.chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')).collect();
    let base = if base.trim().is_empty() { "copia".to_string() } else { base };
    (1..)
        .map(|k| if k == 1 { format!("{base}.mkv") } else { format!("{base} ({k}).mkv") })
        .find(|n| !pasta.join(n).exists())
        .unwrap_or_else(|| format!("{base}.mkv"))
}

/// "2017-09-18 19:24:59.000": o relógio do gravador como ele gravou (sem fuso).
fn relogio(s: f64) -> Option<String> {
    let ms = (s * 1000.0).round() as i64;
    chrono::DateTime::from_timestamp_millis(ms).map(|d| d.naive_utc().format("%Y-%m-%d %H:%M:%S%.3f").to_string())
}


/// O original do caso: banco, ocorrência, mídia, arquivo e formato.
fn original_do_caso(ws: &Path, media_id: &str) -> Result<(Connection, Uuid, VideoMedia, PathBuf, Formato)> {
    let id = Uuid::parse_str(media_id.trim()).map_err(|_| validacao(codigo::ITEM_INVALIDO, Some("media_id")))?;
    let (conn, occ) = crate::edicao::executar::abrir_banco(ws)?;
    let orig = video_repo::find_media_by_id(&conn, &id)?
        .filter(|m| m.occurrence_id == occ)
        .ok_or_else(|| validacao(codigo::MIDIA_AUSENTE, None))?;
    let caminho = resolver_relativo(ws, &orig.relative_path);
    if !caminho.is_file() {
        return Err(validacao(codigo::ARQUIVO_AUSENTE, None));
    }
    let formato = reconhecer(&caminho)?.ok_or_else(|| validacao(codigo::GRAVADOR_NAO_SUPORTADO, None))?;
    exigir_suportado(formato)?;
    Ok((conn, occ, orig, caminho, formato))
}

fn ja_registrada(m: VideoMedia, formato: Formato) -> CopiaRegistrada {
    let (lacunas, inicio_epoca_s) = de_derivacao(&m);
    CopiaRegistrada { media: m, formato, lacunas, inicio_epoca_s, ja_existia: true }
}

/// `.<uuid>.parcial` na pasta das cópias: grava com nome oculto e só ganha o nome de cópia depois de provada
/// (processo morto no meio não deixa um `<nome>.mkv` pela metade com cara de cópia).
fn novo_parcial(ws: &Path) -> Result<PathBuf> {
    let pasta = ws.join(REL_COPIAS_DVR);
    std::fs::create_dir_all(&pasta).map_err(|e| SicroError::Filesystem(format!("{}: {e}", pasta.display())))?;
    Ok(pasta.join(format!(".{}.parcial", Uuid::new_v4())))
}

/// Faz (ou acha) a cópia de trabalho do vídeo `media_id` do caso e a registra como mídia derivada, com log e auditoria.
pub fn copia_no_caso(cap: &Capacidades, ws: &Path, media_id: &str, cancelar: &AtomicBool) -> Result<CopiaRegistrada> {
    copia_no_caso_com(cap, ws, media_id, cancelar, &|_, _| {})
}

/// `copia_no_caso` com o avanço da cópia (`copiar_com`).
pub fn copia_no_caso_com(cap: &Capacidades, ws: &Path, media_id: &str, cancelar: &AtomicBool, avanco: AoAvanco) -> Result<CopiaRegistrada> {
    let (conn, occ, orig, caminho, formato) = original_do_caso(ws, media_id)?;
    if let Some(m) = copia_existente(&conn, ws, &occ, &orig.sha256)? {
        return Ok(ja_registrada(m, formato));
    }
    let parcial = novo_parcial(ws)?;
    let c = copiar_com(cap, &caminho, formato, &parcial, cancelar, avanco)?;
    registrar_copia(cap, ws, &conn, &occ, &orig, &caminho, &caminho, &parcial, c)
}

/// Registra a cópia provada em `parcial` (lida de `lido_de`) como derivada do original `orig` do caso (`caminho`).
#[allow(clippy::too_many_arguments)]
fn registrar_copia(
    cap: &Capacidades,
    ws: &Path,
    conn: &Connection,
    occ: &Uuid,
    orig: &VideoMedia,
    caminho: &Path,
    lido_de: &Path,
    parcial: &Path,
    mut c: Copia,
) -> Result<CopiaRegistrada> {
    let formato = c.formato;
    let pasta = ws.join(REL_COPIAS_DVR);
    let base = Path::new(&orig.filename).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let sha256 = apagando(parcial, sha256_file(parcial))?;
    // a mesma cópia deste original já registrada (o arquivo sumiu da pasta, por exemplo): volta para o lugar dele.
    // A de outro original com os mesmos bytes não serve: a derivação apontaria para o outro.
    let ja = video_repo::list_media_for_occurrence(conn, occ)?
        .into_iter()
        .find(|m| m.sha256 == sha256 && m.derived_from_hash.as_deref() == Some(orig.sha256.as_str()));
    if let Some(m) = ja {
        let lugar = resolver_relativo(ws, &m.relative_path);
        if lugar.is_file() {
            let _ = std::fs::remove_file(parcial);
        } else {
            if let Some(p) = lugar.parent() {
                apagando(parcial, std::fs::create_dir_all(p).map_err(SicroError::from))?;
            }
            apagando(parcial, std::fs::rename(parcial, &lugar).map_err(SicroError::from))?;
        }
        return Ok(ja_registrada(m, formato));
    }
    let nome = nome_livre(&pasta, &base);
    let destino = pasta.join(&nome);
    apagando(parcial, std::fs::rename(parcial, &destino).map_err(SicroError::from))?;
    // o comando registrado lê o original do caso e grava no nome final (os bytes são os mesmos: SHA-256 conferido)
    let (p, de) = (parcial.to_string_lossy().into_owned(), lido_de.to_string_lossy().into_owned());
    for a in c.comando.iter_mut() {
        if *a == p {
            *a = destino.to_string_lossy().into_owned();
        } else if *a == de {
            *a = caminho.to_string_lossy().into_owned();
        }
    }
    let agora = Utc::now();
    let derivacao = json!({
        "kind": "copia_dvr",
        "formato": formato,
        "rotulo": formato.rotulo(),
        "source": { "sha256": orig.sha256, "filename": orig.filename, "relative_path": orig.relative_path },
        "ffmpeg_command": c.comando,
        "ffmpeg": { "versao": cap.versao, "sha256": cap.sha256 },
        "recodificado": c.recodificado,
        "taxa": c.taxa.map(|r| format!("{}/{}", r.num, r.den)),
        "taxa_suposta": c.taxa.map(|_| c.taxa_suposta),
        "quadros": c.quadros,
        "framemd5_sha256": c.framemd5_sha256,
        "audio": c.audio,
        "inicio_s": c.inicio_s,
        "inicio_epoca_s": c.inicio_epoca_s,
        "relogio": c.inicio_epoca_s.and_then(relogio),
        "desloc_s": c.desloc_s,
        "passo_mediano_s": c.passo_mediano_s,
        "lacunas": c.lacunas,
        "created_at": agora.to_rfc3339(),
    });
    let probe = crate::video::probe_media(&destino).ok();
    let media = VideoMedia {
        id: Uuid::new_v4(),
        occurrence_id: *occ,
        original_path: None,
        relative_path: format!("{REL_COPIAS_DVR}/{nome}"),
        filename: nome,
        sha256,
        size_bytes: std::fs::metadata(&destino).map(|m| m.len()).unwrap_or(0),
        duration_s: probe.as_ref().and_then(|p| p.duration_s),
        codec: probe.as_ref().and_then(|p| p.video_codec.clone()),
        width: probe.as_ref().and_then(|p| p.width),
        height: probe.as_ref().and_then(|p| p.height),
        pixel_format: probe.as_ref().and_then(|p| p.pixel_format.clone()),
        fps_declared: probe.as_ref().and_then(|p| p.fps_declared),
        avg_frame_rate: probe.as_ref().and_then(|p| p.avg_frame_rate.clone()),
        r_frame_rate: probe.as_ref().and_then(|p| p.r_frame_rate.clone()),
        time_base: probe.as_ref().and_then(|p| p.time_base.clone()),
        frame_count: probe.as_ref().and_then(|p| p.frame_count),
        bitrate: probe.as_ref().and_then(|p| p.bitrate),
        raw_probe_json: probe.as_ref().map(|p| p.raw_json.clone()).unwrap_or_else(|| "{}".into()),
        warnings_json: serde_json::to_string(&probe.as_ref().map(|p| p.warnings.clone()).unwrap_or_default()).unwrap_or_else(|_| "[]".into()),
        created_at: agora,
        updated_at: agora,
        derived_from_hash: Some(orig.sha256.clone()),
        derivation_json: Some(derivacao.to_string()),
    };
    // mídia, log e auditoria juntos; sem registro, o arquivo não fica
    let gravar = || -> Result<()> {
        let tx = conn.unchecked_transaction()?;
        video_repo::insert_media(&tx, &media)?;
        let detalhe = json!({ "media_id": media.id.to_string(), "sha256": media.sha256, "filename": media.filename, "derivation": derivacao });
        video_repo::insert_log(&tx, occ, Some(&orig.sha256), "dvr.copia", &detalhe.to_string())?;
        occurrence_repo::record_audit(&tx, Some(occ), "video.dvr.copia", Some("video"), Some("video_media"), Some(&media.id), Some(&media.sha256))?;
        Ok(tx.commit()?)
    };
    apagando(&destino, gravar())?;
    Ok(CopiaRegistrada { media, formato, lacunas: c.lacunas, inicio_epoca_s: c.inicio_epoca_s, ja_existia: false })
}

// ---------------------------------------------------------------- importação (P4.6b)

/// Cópia feita e provada à espera do registro do original (`registrar_pendente`).
struct Pendente {
    chave: String,
    ws: PathBuf,
    formato: Formato,
    /// SHA-256 do arquivo de que a cópia saiu: o original registrado tem de ser ele.
    sha256: String,
    origem: PathBuf,
    /// O `.parcial` e a prova; None = a cópia deste arquivo já estava no caso.
    feita: Option<(PathBuf, Copia)>,
}

fn pendentes() -> std::sync::MutexGuard<'static, Vec<Pendente>> {
    static P: Mutex<Vec<Pendente>> = Mutex::new(Vec::new());
    P.lock().unwrap_or_else(|e| e.into_inner())
}

/// O que a tela recebe de `preparar`.
#[derive(Debug, Clone, Serialize)]
pub struct Preparo {
    pub chave: String,
    pub formato: Formato,
    /// A cópia deste arquivo já estava no caso: nada foi gravado.
    pub ja_existia: bool,
}

/// Esquece as pendentes do caso e apaga os `.parcial` que sobraram (importação abandonada, processo morto).
fn descartar_pendentes(ws: &Path) {
    pendentes().retain(|p| p.ws != ws);
    let Ok(rd) = std::fs::read_dir(ws.join(REL_COPIAS_DVR)) else { return };
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with('.') && n.ends_with(".parcial") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Faz e prova a cópia de trabalho do arquivo escolhido (`origem`, ainda fora do caso) num `.parcial` da pasta das
/// cópias, com o SHA-256 dele em paralelo. Nada entra no banco: o registro é `registrar_pendente`, depois do original.
/// Cancelada ou com erro, o `.parcial` some. Cópia deste arquivo já no caso: para assim que o SHA-256 sai.
pub fn preparar(cap: &Capacidades, ws: &Path, origem: &Path, cancelar: &AtomicBool, avanco: AoAvanco) -> Result<Preparo> {
    let formato = reconhecer(origem)?.ok_or_else(|| validacao(codigo::GRAVADOR_NAO_SUPORTADO, None))?;
    exigir_suportado(formato)?;
    // o caso abre antes de gravar nele
    crate::edicao::executar::abrir_banco(ws)?;
    descartar_pendentes(ws);
    let parcial = novo_parcial(ws)?;
    let (parar, fim, existente) = (AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false));
    let (copia, sha256) = std::thread::scope(|s| {
        let h = s.spawn(|| -> Result<String> {
            let sha = sha256_parando(origem, &parar)?;
            let (conn, occ) = crate::edicao::executar::abrir_banco(ws)?;
            if copia_existente(&conn, ws, &occ, &sha)?.is_some() {
                existente.store(true, Ordering::SeqCst);
                parar.store(true, Ordering::SeqCst);
            }
            Ok(sha)
        });
        // o pedido de quem chamou chega aqui por `parar`, que o SHA-256 achando a cópia no caso também liga
        let espelho = s.spawn(|| {
            while !fim.load(Ordering::SeqCst) {
                if cancelar.load(Ordering::SeqCst) {
                    parar.store(true, Ordering::SeqCst);
                }
                std::thread::park_timeout(Duration::from_millis(10));
            }
        });
        let _fim = AoSair(&fim, espelho.thread().clone());
        let c = copiar_com(cap, origem, formato, &parcial, &parar, avanco);
        // sem cópia, o SHA-256 não serve mais
        if c.is_err() {
            parar.store(true, Ordering::SeqCst);
        }
        (c, h.join().unwrap_or_else(|_| Err(SicroError::Validation(codigo::PANICO.into()))))
    });
    let chave = Uuid::new_v4().to_string();
    let pendente = |sha256: String, feita| Pendente { chave: chave.clone(), ws: ws.to_path_buf(), formato, sha256, origem: origem.to_path_buf(), feita };
    if existente.load(Ordering::SeqCst) {
        let _ = std::fs::remove_file(&parcial);
        pendentes().push(pendente(sha256?, None));
        return Ok(Preparo { chave, formato, ja_existia: true });
    }
    if cancelar.load(Ordering::SeqCst) {
        let _ = std::fs::remove_file(&parcial);
        return Err(cancelado());
    }
    let copia = copia?;
    let sha256 = apagando(&parcial, sha256)?;
    pendentes().push(pendente(sha256, Some((parcial, copia))));
    Ok(Preparo { chave, formato, ja_existia: false })
}

/// Registra a cópia que `preparar` deixou (`chave`) como derivada do original `media_id`, já registrado pela
/// importação: o SHA-256 dele tem de ser o do arquivo de que a cópia saiu. A cópia que já estava no caso volta igual.
pub fn registrar_pendente(cap: &Capacidades, ws: &Path, media_id: &str, chave: &str) -> Result<CopiaRegistrada> {
    let p = {
        let mut v = pendentes();
        let i = v.iter().position(|p| p.chave == chave && p.ws == ws).ok_or_else(|| validacao(codigo::ITEM_INVALIDO, Some("chave")))?;
        v.swap_remove(i)
    };
    let parcial = p.feita.as_ref().map(|f| f.0.clone());
    let r = (|| {
        let (conn, occ, orig, caminho, _) = original_do_caso(ws, media_id)?;
        if let Some(m) = copia_existente(&conn, ws, &occ, &orig.sha256)? {
            return Ok(ja_registrada(m, p.formato));
        }
        let Some((parcial, copia)) = p.feita else { return Err(validacao(codigo::ARQUIVO_AUSENTE, None)) };
        if orig.sha256 != p.sha256 {
            return Err(validacao(codigo::COPIA_DIVERGENTE, Some("original")));
        }
        registrar_copia(cap, ws, &conn, &occ, &orig, &caminho, &p.origem, &parcial, copia)
    })();
    // registrada, o `.parcial` já mudou de nome; nos outros casos ele sobra
    if let Some(x) = parcial {
        let _ = std::fs::remove_file(x);
    }
    r
}


#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::ffmpeg::args;
    use crate::edicao::modelo::Velocidade;
    use crate::edicao::quadros::apoio::{self, Medida};
    use crate::edicao::tempo::{mapa_trecho, MapaQuadros};
    use crate::edicao::testes::fontes::{self, Fonte};

    fn amostra(nome: &str) -> Option<PathBuf> {
        fontes::amostras_de_gravador().into_iter().find(|p| p.file_name().is_some_and(|n| n == nome))
    }

    /// As amostras reais (`p1b/dl/dvr/amostras`) com o formato e o que a medição do dossiê achou (receitas §8 e §9):
    /// quadros e lacunas (None = não medido lá). IFV: o dossiê conta 253 porque soma `pts_time` em ponto flutuante e
    /// 19 passos de exatamente 1,5× a mediana (60 ms) passam; em inteiros (como `quadros::medir_passos`) são 234.
    const REAIS: &[(&str, Formato, u64, Option<usize>)] = &[
        ("dahua.dav", Formato::Dahua, 726, Some(1)),
        ("hik_nvr.mp4", Formato::HikvisionImkh, 420, Some(0)),
        ("ifv7.ifv", Formato::Ifv, 6000, Some(234)),
        ("test.lvf", Formato::Lvf, 631, None),
    ];

    fn ffmpeg(cap: &Capacidades, pasta: &Path, a: &[&str]) {
        let mut v = textos(&["-hide_banner", "-nostdin", "-y", "-loglevel", "error"]);
        v.extend(textos(a));
        let cmd = Comando {
            nome: "gerar".into(),
            programa: cap.binario.clone(),
            args: v,
            pasta: pasta.to_path_buf(),
            ambiente: Vec::new(),
            sem_ambiente: Vec::new(),
            stderr: pasta.join("gerar.stderr"),
        };
        let fim = exec::rodar(&cmd, &AtomicBool::new(false)).unwrap();
        exec::exigir_sucesso(&cmd, &fim).unwrap();
    }

    /// Sem amostra real: fluxo cru H.264 sem e com quadros B, HEVC com quadros B (15 qps, 20 s) e AVI de 12 s com
    /// G.711 cortado antes do `idx1`. Conteúdo do `testsrc2`: todo quadro diferente.
    fn sinteticos(cap: &Capacidades, pasta: &Path) -> Vec<(PathBuf, Formato, bool)> {
        let src = ["-f", "lavfi", "-i", "testsrc2=size=320x240:rate=15", "-t", "20", "-pix_fmt", "yuv420p"];
        let x264 = |bf: &str, nome: &str| {
            let mut a = src.to_vec();
            a.extend(["-c:v", "libx264", "-preset", "veryfast", "-bf", bf, "-g", "30", "-f", "h264", nome]);
            ffmpeg(cap, pasta, &a);
        };
        x264("0", "semb.264");
        x264("2", "comb.264");
        let mut a = src.to_vec();
        a.extend(["-c:v", "libx265", "-preset", "veryfast", "-x265-params", "log-level=error", "-g", "30", "-f", "hevc", "comb.h265"]);
        ffmpeg(cap, pasta, &a);
        ffmpeg(
            cap,
            pasta,
            &[
                "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25", "-f", "lavfi", "-i", "sine=f=440:r=8000", "-t", "12", "-c:v", "libx264",
                "-preset", "veryfast", "-bf", "0", "-g", "25", "-pix_fmt", "yuv420p", "-c:a", "pcm_mulaw", "-ar", "8000", "inteiro.avi",
            ],
        );
        let b = std::fs::read(pasta.join("inteiro.avi")).unwrap();
        let k = b.windows(4).rposition(|w| w == b"idx1").expect("AVI com idx1");
        std::fs::write(pasta.join("cortado.avi"), &b[..k - 5000]).unwrap();
        vec![
            (pasta.join("semb.264"), Formato::H264Cru, false),
            (pasta.join("comb.264"), Formato::H264Cru, true),
            (pasta.join("comb.h265"), Formato::HevcCru, true),
            (pasta.join("cortado.avi"), Formato::AviSemIndice, false),
        ]
    }

    // ------------------------------------------------------------ reconhecer

    #[test]
    fn reconhece_pelos_bytes_do_comeco() {
        let mut ifv = IFV.to_vec();
        ifv.extend(b"\0\0\0This is h.264 file");
        let casos: &[(&[u8], Option<Formato>)] = &[
            (b"DHAV\xf0\0\0\0\x0c\xe1", Some(Formato::Dahua)),
            (b"DHAV\xfd\0\0\0", Some(Formato::Dahua)),
            (b"DAHUA\0\0\0", Some(Formato::Dahua)),
            (b"DHAV\x00\0\0\0", None),
            (b"IMKH\x01\x01\0\0\x02\0\0\x01", Some(Formato::HikvisionImkh)),
            (b"4HKH\xfe\xb3\xd0\xd6", Some(Formato::HikvisionAntigo)),
            (&ifv, Some(Formato::Ifv)),
            (&IFV[..16], None),
            (b"LVFF\x1c\0\0\0", Some(Formato::Lvf)),
            // H.264: SPS, AUD, SEI, IDR; HEVC: VPS, AUD, SPS
            (b"\0\0\0\x01\x67\x64\0\x0c", Some(Formato::H264Cru)),
            (b"\0\0\x01\x09\xf0\0", Some(Formato::H264Cru)),
            (b"\0\0\0\x01\x06\x05", Some(Formato::H264Cru)),
            (b"\0\0\0\x01\x65\x88", Some(Formato::H264Cru)),
            (b"\0\0\0\x01\x40\x01\x0c\x01", Some(Formato::HevcCru)),
            (b"\0\0\0\x01\x46\x01\x50", Some(Formato::HevcCru)),
            (b"\0\0\x01\x42\x01\x01", Some(Formato::HevcCru)),
            // MP4 (caixa comum e de 64 bits), MPEG-PS, MPEG-TS, MKV: vídeo comum
            (b"\0\0\0\x20ftypisom", None),
            (b"\0\0\0\x01ftypisom", None),
            (b"\0\0\0\x01mdat\0\0", None),
            (b"\0\0\x01\xba\x44\0", None),
            (b"\x47\x40\x11\x10", None),
            (b"\x1a\x45\xdf\xa3", None),
            // fatia sem SPS (fluxo cortado) e VOL do MPEG-4 parte 2: não
            (b"\0\0\0\x01\x41\x9a", None),
            (b"\0\0\x01\x21\x00", None),
            (b"", None),
        ];
        for (bytes, esperado) in casos {
            assert_eq!(pela_cabeca(bytes), *esperado, "{bytes:02x?}");
        }
    }

    /// AVI montado à mão: com `idx1`, com `indx` (OpenDML) e sem o `idx1` (fim normal e cortado no meio do `movi`).
    #[test]
    fn avi_sem_indice_so_quando_falta_o_indice() {
        let pedaco = |id: &[u8], corpo: &[u8]| -> Vec<u8> {
            let mut v = id.to_vec();
            v.extend((corpo.len() as u32).to_le_bytes());
            v.extend(corpo);
            if corpo.len() % 2 == 1 {
                v.push(0);
            }
            v
        };
        let lista = |tipo: &[u8], corpo: &[u8]| -> Vec<u8> {
            let mut c = tipo.to_vec();
            c.extend(corpo);
            pedaco(b"LIST", &c)
        };
        let riff = |corpo: Vec<u8>| -> Vec<u8> {
            let mut c = b"AVI ".to_vec();
            c.extend(corpo);
            pedaco(b"RIFF", &c)
        };
        let hdrl = lista(b"hdrl", &pedaco(b"avih", &[0; 56]));
        let hdrl_odml = lista(b"hdrl", &[pedaco(b"avih", &[0; 56]), lista(b"strl", &pedaco(b"indx", &[0; 24]))].concat());
        let movi = lista(b"movi", &[pedaco(b"00dc", &[1; 301]), pedaco(b"00dc", &[2; 200])].concat());
        let inteiro = riff([hdrl.clone(), movi.clone(), pedaco(b"idx1", &[0; 32])].concat());
        let d = fontes::pasta_temporaria("dvr-avi");
        let casos = [
            ("com-idx1.avi", inteiro.clone(), None),
            ("odml.avi", riff([hdrl_odml, movi.clone()].concat()), None),
            ("sem-idx1.avi", riff([hdrl.clone(), movi.clone()].concat()), Some(Formato::AviSemIndice)),
            ("cortado.avi", inteiro[..12 + hdrl.len() + 300].to_vec(), Some(Formato::AviSemIndice)),
        ];
        for (nome, bytes, esperado) in casos {
            std::fs::write(d.join(nome), &bytes).unwrap();
            assert_eq!(reconhecer(&d.join(nome)).unwrap(), esperado, "{nome}");
        }
    }

    #[test]
    fn reconhece_as_amostras_reais_e_os_sinteticos() {
        let mut vistas = 0;
        let antigos = [("hik_ch01.mp4", Formato::HikvisionAntigo, 0, None), ("hik_test000.mp4", Formato::HikvisionAntigo, 0, None)];
        for &(nome, f, _, _) in REAIS.iter().chain(antigos.iter()) {
            match amostra(nome) {
                Some(p) => {
                    assert_eq!(reconhecer(&p).unwrap(), Some(f), "{nome}");
                    vistas += 1;
                }
                None => eprintln!("aviso: amostra {nome} ausente, fora do teste"),
            }
        }
        eprintln!("amostras reais reconhecidas: {vistas}");
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("dvr-sinteticos");
        for (p, f, _) in sinteticos(&cap, &d) {
            assert_eq!(reconhecer(&p).unwrap(), Some(f), "{}", p.display());
        }
        // vídeo comum, inclusive o AVI com índice e o MPEG-PS sem "IMKH"
        assert_eq!(reconhecer(&d.join("inteiro.avi")).unwrap(), None);
        for f in [Fonte::Mp4, Fonte::Mkv30, Fonte::Avi25, Fonte::Ts5994, Fonte::MpegPs, Fonte::Mp4QuadrosB] {
            assert_eq!(reconhecer(&fontes::gerar(f).unwrap()).unwrap(), None, "{f:?}");
        }
    }

    // ------------------------------------------------------------ recusa

    /// O Hikvision antigo volta com a frase do catálogo, antes de entrar no caso e sem rodar o ffmpeg.
    #[test]
    fn recusa_o_hikvision_antigo_com_a_frase_do_catalogo() {
        let frase = "gravador_nao_suportado: Formato de gravador não suportado: Hikvision antigo (4HKH)";
        let msg = |e: SicroError| match crate::edicao::mensagens::erro_de_comando(e) {
            SicroError::Validation(m) => m,
            outro => panic!("{outro:?}"),
        };
        let d = fontes::pasta_temporaria("dvr-4hkh");
        // só o cabeçalho da amostra pública
        let falso = d.join("ch01.mp4");
        std::fs::write(&falso, b"4HKH\xfe\xb3\xd0\xd6\x08\x03\x04\x20\0\0\0\0\x03\x10\x01\x10").unwrap();
        let mut arquivos = vec![falso];
        arquivos.extend(["hik_ch01.mp4", "hik_test000.mp4"].iter().filter_map(|n| amostra(n)));
        for a in &arquivos {
            assert_eq!(msg(crate::commands::video_dvr_commands::conferir(a).unwrap_err()), frase, "{}", a.display());
            // `copiar` recusa antes de qualquer processo (capacidades falsas: o binário não existe)
            let e = copiar(&apoio::cap_falsa(), a, Formato::HikvisionAntigo, &d.join("x.mkv"), &AtomicBool::new(false)).unwrap_err();
            assert_eq!(msg(e), frase);
            assert!(!d.join("x.mkv").exists());
        }
        eprintln!("4HKH recusados: {}", arquivos.len());
        // vídeo comum passa sem formato
        std::fs::write(d.join("comum.mp4"), b"\0\0\0\x20ftypisom\0\0\x02\0").unwrap();
        assert_eq!(crate::commands::video_dvr_commands::conferir(&d.join("comum.mp4")).unwrap(), None);
    }

    // ------------------------------------------------------------ cópia de trabalho

    /// Lista `framemd5` por um caminho independente do `Leitor`: arquivo em disco, (tamanho, md5) por quadro de vídeo.
    fn lista_crua(cap: &Capacidades, video: &Path, pasta: &Path, nome: &str) -> Vec<(String, String)> {
        let saida = pasta.join(format!("{nome}.framemd5"));
        let v = video.to_string_lossy();
        ffmpeg(cap, pasta, &["-threads", "1", "-i", &v, "-map", "0:v:0", "-fps_mode", "passthrough", "-f", "framemd5", &saida.to_string_lossy()]);
        std::fs::read_to_string(&saida)
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(|l| {
                let c: Vec<&str> = l.split(',').map(str::trim).collect();
                (c[4].to_string(), c[5].to_string())
            })
            .collect()
    }

    /// pts do primeiro som menos o do primeiro quadro, em s, por um caminho independente do `Leitor` (arquivo em disco).
    fn som_menos_video(cap: &Capacidades, video: &Path, pasta: &Path, nome: &str) -> f64 {
        let saida = pasta.join(format!("{nome}.av.framemd5"));
        let v = video.to_string_lossy();
        ffmpeg(cap, pasta, &["-threads", "1", "-copyts", "-i", &v, "-map", "0:v:0", "-map", "0:a:0", "-fps_mode", "passthrough", "-enc_time_base", "demux", "-f", "framemd5", &saida.to_string_lossy()]);
        let (mut tb, mut primeiro) = ([0.0f64; 2], [None::<f64>; 2]);
        for l in std::fs::read_to_string(&saida).unwrap().lines() {
            if let Some(x) = l.strip_prefix("#tb ") {
                let (k, f) = x.split_once(':').unwrap();
                let (a, b) = f.trim().split_once('/').unwrap();
                tb[k.parse::<usize>().unwrap()] = a.parse::<f64>().unwrap() / b.parse::<f64>().unwrap();
            } else if !l.starts_with('#') {
                let c: Vec<&str> = l.split(',').map(str::trim).collect();
                let k: usize = c[0].parse().unwrap();
                primeiro[k].get_or_insert(c[2].parse::<f64>().unwrap() * tb[k]);
            }
        }
        primeiro[1].unwrap() - primeiro[0].unwrap()
    }

    /// (contêiner, codec do vídeo) pelo ffprobe.
    fn conteiner(cap: &Capacidades, video: &Path) -> (String, String) {
        let t = Trabalho::nova().unwrap();
        let a = textos(&["-v", "error", "-select_streams", "v:0", "-show_entries", "format=format_name:stream=codec_name", "-of", "json", &video.to_string_lossy()]);
        let j = ffprobe_json(cap, &t, "c", a).unwrap();
        (j["format"]["format_name"].as_str().unwrap_or("").into(), j["streams"][0]["codec_name"].as_str().unwrap_or("").into())
    }

    /// Cópia conferida por fora: MKV, mesma lista (tamanho, md5) e mesma contagem, original intacto.
    fn conferir_copia(cap: &Capacidades, origem: &Path, destino: &Path, c: &Copia, pasta: &Path) {
        let o = lista_crua(cap, origem, pasta, "o");
        let k = lista_crua(cap, destino, pasta, "k");
        assert_eq!((o.len() as u64, k.len() as u64), (c.quadros, c.quadros), "{}", origem.display());
        let diferente = o.iter().zip(&k).position(|(a, b)| a != b);
        assert_eq!(diferente, None, "{}: quadro {diferente:?} diferente ({})", origem.display(), cap.versao);
        assert_eq!(conteiner(cap, destino).0, "matroska,webm");
        assert_eq!(c.comando[0], cap.binario.to_string_lossy());
    }

    #[test]
    fn copia_das_amostras_reais_com_a_mesma_lista_framemd5() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("dvr-copia-reais");
        let mut feitas = 0;
        for &(nome, f, quadros, lacunas) in REAIS {
            let Some(origem) = amostra(nome) else {
                eprintln!("aviso: amostra {nome} ausente, fora do teste");
                continue;
            };
            let antes = sha256_file(&origem).unwrap();
            let destino = d.join(format!("{nome}.mkv"));
            let c = copiar(&cap, &origem, f, &destino, &AtomicBool::new(false)).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
            eprintln!(
                "{nome} ({}): {} quadros, áudio {:?}, início {:?}, época {:?}, desloc {:?}, passo {:?}, {} lacunas {:?}",
                cap.versao, c.quadros, c.audio.as_ref().map(|a| a.quadros), c.inicio_s, c.inicio_epoca_s, c.desloc_s, c.passo_mediano_s,
                c.lacunas.len(), c.lacunas.iter().take(2).collect::<Vec<_>>()
            );
            assert_eq!(sha256_file(&origem).unwrap(), antes, "original alterado");
            assert_eq!((c.formato, c.quadros, c.recodificado, c.taxa), (f, quadros, false, None), "{nome}");
            assert!(c.comando.iter().any(|a| a == "-copyinkf") && c.comando.windows(2).any(|w| w == ["-fflags", "+genpts"]), "{:?}", c.comando);
            conferir_copia(&cap, &origem, &destino, &c, &d);
            if let Some(n) = lacunas {
                assert_eq!(c.lacunas.len(), n, "{nome}: lacunas");
            }
            // o deslocamento é o `start_time` do original: a cópia começa em zero
            assert!((c.desloc_s.unwrap() - c.inicio_s.unwrap()).abs() < 1e-6, "{nome}: {:?} × {:?}", c.desloc_s, c.inicio_s);
            match f {
                // relógio do DVR, 2017-09-18 19:24:59 (o nome do arquivo); 800 ms sem quadro já vêm do gravador
                Formato::Dahua => {
                    assert_eq!(c.inicio_epoca_s, Some(1_505_762_699.0));
                    assert_eq!(relogio(1_505_762_699.0).as_deref(), Some("2017-09-18 19:24:59.000"));
                    let l = &c.lacunas[0];
                    // em época o f64 tem passo de 2,4e-7 s
                    assert!((l.ate_s - l.de_s - 0.8).abs() < 1e-6 && l.quadros >= 9 && l.quadros <= 11, "{l:?}");
                    assert_eq!(c.audio.as_ref().map(|a| a.quadros), Some(1245));
                }
                // contador do MPEG-PS (s), não data
                _ => assert_eq!(c.inicio_epoca_s, None, "{nome}"),
            }
            feitas += 1;
        }
        eprintln!("cópias de amostras reais conferidas ({}): {feitas}", cap.versao);
    }

    #[test]
    fn copia_dos_sinteticos_cru_com_quadros_b_vai_sem_perdas() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("dvr-copia-sint");
        for (origem, f, quadros_b) in sinteticos(&cap, &d) {
            let destino = d.join(format!("{}.mkv", origem.file_name().unwrap().to_string_lossy()));
            let c = copiar(&cap, &origem, f, &destino, &AtomicBool::new(false)).unwrap_or_else(|e| panic!("{} ({}): {e}", origem.display(), cap.versao));
            eprintln!("{} ({}): {} quadros, recodificado {}, taxa {:?}", origem.display(), cap.versao, c.quadros, c.recodificado, c.taxa);
            conferir_copia(&cap, &origem, &destino, &c, &d);
            // remux quando dá; FFV1 só no fluxo cru com quadros B
            assert_eq!(c.recodificado, quadros_b, "{}", origem.display());
            assert_eq!(conteiner(&cap, &destino).1, if quadros_b { "ffv1" } else if f == Formato::HevcCru { "hevc" } else { "h264" });
            if f.cru() {
                // a taxa vem do fluxo (15 qps), não a padrão do demuxer
                assert_eq!(c.taxa, Some(Racional { num: 15, den: 1 }), "{}", origem.display());
                assert_eq!((c.quadros, c.desloc_s, c.lacunas.len()), (300, None, 0));
                assert!(c.comando.windows(2).any(|w| w == ["-framerate", "15/1"]));
            } else {
                // AVI cortado: os quadros que restam, iguais; o G.711 vai junto
                assert!(c.quadros > 250 && c.quadros < 300, "{}", c.quadros);
                assert!(c.audio.as_ref().is_some_and(|a| a.quadros > 0));
                assert!(c.lacunas.is_empty());
                // o som no mesmo lugar em relação à imagem (com `+genpts` o vídeo ia um quadro, 40 ms, depois)
                let (so, sc) = (som_menos_video(&cap, &origem, &d, "avi-o"), som_menos_video(&cap, &destino, &d, "avi-c"));
                assert!((so - sc).abs() <= 0.002, "som − vídeo: original {so} × cópia {sc}");
                assert!(!c.comando.iter().any(|a| a == "+genpts"), "{:?}", c.comando);
            }
        }
    }

    // ------------------------------------------------------------ cópia que dá errado

    /// ffmpeg de mentira: chama o verdadeiro e planta `defeito` só no comando que grava o MKV (`$real`, `${args[@]}`
    /// sem a saída, `$fim` = a saída). O resto (listas, sondas) passa direto.
    #[cfg(unix)]
    fn ffmpeg_com_defeito(cap: &Capacidades, pasta: &Path, nome: &str, defeito: &str) -> Capacidades {
        use std::os::unix::fs::PermissionsExt;
        let script = pasta.join(nome);
        let texto = format!(
            "#!/bin/bash\nreal='{}'\nfor a in \"$@\"; do [ \"$a\" = matroska ] && eh=1; done\n[ -z \"$eh\" ] && exec \"$real\" \"$@\"\n\
             args=(\"$@\"); fim=\"${{args[-1]}}\"; unset 'args[-1]'\n{defeito}\n",
            cap.binario.display()
        );
        std::fs::write(&script, texto).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        Capacidades { binario: script, ..cap.clone() }
    }

    /// Remux que sai errado ou morre no meio: erro do catálogo, nada no destino, original com o mesmo SHA-256; pelo
    /// caso, nenhuma mídia nova e nenhum arquivo na pasta das cópias.
    #[cfg(unix)]
    #[test]
    fn copia_que_da_errado_apaga_o_destino_e_nao_toca_no_original() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("dvr-defeito");
        let avi = sinteticos(&cap, &d).into_iter().find(|x| x.1 == Formato::AviSemIndice).unwrap().0;
        let mut origens = vec![(avi, Formato::AviSemIndice)];
        origens.extend(amostra("dahua.dav").map(|p| (p, Formato::Dahua)));
        let defeitos = [
            ("quadros", r#"exec "$real" "${args[@]}" -frames:v 40 "$fim""#, "copia_divergente"),
            ("tempos", r#"exec "$real" "${args[@]}" -bsf:v setts=pts=2*PTS:dts=2*DTS "$fim""#, "copia_divergente"),
            ("som", r#"exec "$real" "${args[@]}" -bsf:a setts=ts=TS+400 "$fim""#, "copia_divergente"),
            // gravou um pedaço e morreu
            ("morre", r#""$real" "${args[@]}" -frames:v 40 "$fim"; exit 137"#, "remux"),
        ];
        // AVI sem índice com quadros B (sem som, para valer só a prova do vídeo): o `+genpts` grava 40, 120, 160, 160…
        // onde o original tem 80, 120, 160, 200 (o remux sem ele nem grava): a cópia não passa pelos tempos
        ffmpeg(
            &cap,
            &d,
            &["-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25", "-t", "12", "-c:v", "libx264", "-preset", "veryfast", "-bf", "2", "-g", "25", "-pix_fmt", "yuv420p", "b.avi"],
        );
        let b = std::fs::read(d.join("b.avi")).unwrap();
        let k = b.windows(4).rposition(|w| w == b"idx1").unwrap();
        std::fs::write(d.join("b-cortado.avi"), &b[..k - 5000]).unwrap();
        let antes = sha256_file(&d.join("b-cortado.avi")).unwrap();
        let e = copiar(&cap, &d.join("b-cortado.avi"), Formato::AviSemIndice, &d.join("b.mkv"), &AtomicBool::new(false)).unwrap_err().to_string();
        assert!(e.ends_with("copia_divergente: tempos"), "AVI com quadros B ({}): {e}", cap.versao);
        assert!(!d.join("b.mkv").exists());
        assert_eq!(sha256_file(&d.join("b-cortado.avi")).unwrap(), antes);
        for (origem, f) in &origens {
            let antes = sha256_file(origem).unwrap();
            for (i, (nome, defeito, esperado)) in defeitos.iter().enumerate() {
                let falsa = ffmpeg_com_defeito(&cap, &d, &format!("ffmpeg-{nome}"), defeito);
                let destino = d.join(format!("c{i}.mkv"));
                let e = copiar(&falsa, origem, *f, &destino, &AtomicBool::new(false)).expect_err(nome).to_string();
                let rotulo = format!("{} {nome} ({})", origem.display(), cap.versao);
                assert!(e.contains(esperado), "{rotulo}: {e}");
                assert!(!destino.exists(), "{rotulo}: destino ficou");
                assert_eq!(sha256_file(origem).unwrap(), antes, "{rotulo}: original alterado");
                eprintln!("{rotulo}: {e}");
            }
        }
        // pelo caso: o original registrado fica, nada mais
        let (origem, _) = origens.last().unwrap();
        let (_d, ws, orig) = caso_com(origem);
        let falsa = ffmpeg_com_defeito(&cap, &d, "ffmpeg-caso", defeitos[3].1);
        let (conn, occ) = crate::edicao::executar::abrir_banco(&ws).unwrap();
        let n = video_repo::list_media_for_occurrence(&conn, &occ).unwrap().len();
        assert!(copia_no_caso(&falsa, &ws, &orig.id.to_string(), &AtomicBool::new(false)).is_err());
        assert_eq!(video_repo::list_media_for_occurrence(&conn, &occ).unwrap().len(), n);
        assert_eq!(std::fs::read_dir(ws.join(REL_COPIAS_DVR)).unwrap().count(), 0, "arquivo na pasta das cópias");
        assert_eq!(sha256_file(&resolver_relativo(&ws, &orig.relative_path)).unwrap(), orig.sha256, "original do caso alterado");
        // enquanto grava, o MKV tem nome oculto (`.….parcial`): o ffmpeg de mentira recusa qualquer outro
        let vigia = ffmpeg_com_defeito(&cap, &d, "ffmpeg-oculto", r#"case "$(basename "$fim")" in .*.parcial) exec "$real" "$@";; *) exit 99;; esac"#);
        let r = copia_no_caso(&vigia, &ws, &orig.id.to_string(), &AtomicBool::new(false)).unwrap();
        let nomes: Vec<String> = std::fs::read_dir(ws.join(REL_COPIAS_DVR)).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(nomes, vec![r.media.filename.clone()]);
        assert!(!r.media.filename.starts_with('.'));
        assert_eq!(sha256_file(&resolver_relativo(&ws, &orig.relative_path)).unwrap(), orig.sha256, "original do caso alterado");
    }

    /// Dois originais diferentes (um byte do cabeçalho que o demuxer não usa) dão cópias com os mesmos bytes: cada um
    /// fica com a sua, derivada dele, e não com a do outro.
    #[test]
    fn copia_igual_de_outro_original_nao_serve() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("dvr-outro");
        let a = sinteticos(&cap, &d).into_iter().find(|x| x.1 == Formato::AviSemIndice).unwrap().0;
        let mut b = std::fs::read(&a).unwrap();
        // `dwMaxBytesPerSec` do `avih`
        let k = b.windows(4).position(|w| w == b"avih").unwrap() + 12;
        b[k] ^= 0x5a;
        std::fs::write(d.join("outra.avi"), &b).unwrap();
        let (_t, ws, orig_a) = caso_com(&a);
        let input = crate::models::RegisterVideoInput { source_path: d.join("outra.avi").to_string_lossy().into(), allow_duplicate: false };
        let orig_b = tauri::async_runtime::block_on(crate::commands::video_commands::register_video_media(ws.to_string_lossy().into(), input)).unwrap();
        assert_ne!(orig_a.sha256, orig_b.sha256);
        let ra = copia_no_caso(&cap, &ws, &orig_a.id.to_string(), &AtomicBool::new(false)).unwrap();
        let rb = copia_no_caso(&cap, &ws, &orig_b.id.to_string(), &AtomicBool::new(false)).unwrap();
        // os mesmos bytes (senão o caso não é este)
        assert_eq!(ra.media.sha256, rb.media.sha256);
        assert_eq!((rb.ja_existia, rb.media.derived_from_hash.as_deref()), (false, Some(orig_b.sha256.as_str())));
        assert_ne!(rb.media.id, ra.media.id);
        assert!(resolver_relativo(&ws, &rb.media.relative_path).is_file() && resolver_relativo(&ws, &ra.media.relative_path).is_file());
        // e cada uma volta para o seu original no anexo repetido
        let rb2 = copia_no_caso(&cap, &ws, &orig_b.id.to_string(), &AtomicBool::new(false)).unwrap();
        assert_eq!((rb2.ja_existia, rb2.media.id), (true, rb.media.id));
    }

    // ------------------------------------------------------------ busca e mapa

    /// Um trecho lido pela cadeia do §5.3 (a de `edicao/tempo.rs`): `-copyts`, `-ss` da regra (`busca_em`), `trim` por
    /// pts, cor, `settb` + `setpts` do mapa em inteiros e `fps@`; cada slot identificado pelo conteúdo do quadro.
    /// Devolve os números dos quadros, os `frames in` e se houve busca.
    fn ler_trecho(cap: &Capacidades, m: &Medida, mapa: &MapaQuadros, fps: Racional, pasta: &Path, n: usize) -> (Vec<u32>, u64, bool) {
        let (info, tab) = (&m.info, &m.info.quadros);
        let corte = match tab.pts.get(mapa.j_out) {
            Some(fim) => format!("trim=start_pts={}:end_pts={fim}", tab.pts[mapa.j_in]),
            None => format!("trim=start_pts={}", tab.pts[mapa.j_in]),
        };
        let mut cor = crate::edicao::cor::cadeia_fonte(info.matriz, info.faixa, &info.pix_fmt, info.exibido).expect("formato de pixel");
        // regra do §5.3: quadro clonado no fim da mídia (o decodificador não dá a duração do último)
        let clonado = u64::from(cor.contains("mergeplanes") || mapa.j_out == tab.pts.len());
        if clonado == 1 {
            cor.push_str(",tpad=stop_mode=clone:stop=1");
        }
        let grafo = format!(
            "[0:v]{corte},{cor},settb=1/{c},setpts='ceil(((PTS-{p0})*{kn}-{e})/{kd})*{m}',fps@c000={num}/{den}:start_time=0,\
             tpad=stop_mode=clone:stop=-1,trim=end_frame={n_q},{red},extractplanes=y[v]",
            c = mapa.c, p0 = mapa.p0, kn = mapa.kn, e = mapa.e, kd = mapa.kd, m = mapa.m, num = fps.num, den = fps.den, n_q = mapa.n_q,
            red = apoio::REDUZIDO.0
        );
        let nome = format!("g{n}.txt");
        args::gravar_texto(pasta, &nome, &grafo).unwrap();
        let mut a = args::globais();
        let busca = tab.busca_em(info.inicio_s, mapa.j_in);
        if let Some(alvo) = busca {
            let fim = if mapa.j_out < tab.pts.len() { tab.tempo(mapa.j_out) } else { tab.fim_s() } - info.inicio_s;
            a.extend(["-ss".to_string(), format!("{alvo:.6}"), "-t".to_string(), format!("{:.6}", fim - alvo + 1.0)]);
        }
        a.extend(["-i".to_string(), info.caminho.to_string_lossy().into_owned()]);
        a.extend(args::grafo_em_arquivo(cap, &nome));
        a.extend(textos(&["-map", "[v]", "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "gray", "-"]));
        let cmd = Comando {
            nome: format!("c{n}"),
            programa: cap.binario.clone(),
            args: a,
            pasta: pasta.to_path_buf(),
            ambiente: Vec::new(),
            sem_ambiente: Vec::new(),
            stderr: pasta.join(format!("c{n}.stderr")),
        };
        let (fim, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).unwrap();
        exec::exigir_sucesso(&cmd, &fim).unwrap();
        let tam = apoio::REDUZIDO.1 * apoio::REDUZIDO.2;
        assert_eq!(out.len() % tam, 0, "quadro partido");
        let t = m.por_conteudo.as_ref().unwrap();
        let ids = out.chunks_exact(tam).map(|y| t.get(&apoio::conteudo(y)).copied().unwrap_or(u32::MAX)).collect();
        let lidos = exec::quadros_lidos(&cmd.stderr).unwrap().get("c000").copied().unwrap_or(u64::MAX).saturating_sub(clonado);
        (ids, lidos, busca.is_some())
    }

    /// Trechos `[j_in, j_out)` a `v`× em `fps`: os dois da busca do §11 (2 quadros antes de um quadro-chave do fim e no
    /// próprio), um nos 3 primeiros segundos, câmera lenta e acelerado no meio. Pontas no primeiro quadro do pts.
    fn trechos(m: &Medida, n: usize) -> Vec<(usize, usize, f64, Racional)> {
        let tab = &m.info.quadros;
        let nominal = tab.taxa_nominal();
        let primeiro = |j: usize| tab.indice_do_pts(tab.pts[j]).unwrap_or(j);
        let chaves: Vec<usize> = (0..n).filter(|&j| tab.chave[j]).collect();
        let mut v = Vec::new();
        if let Some(&k) = chaves.iter().rev().find(|&&k| k >= 2 && k + 21 <= n) {
            v.push((primeiro(k - 2), primeiro(k + 20), 1.0, nominal));
            v.push((primeiro(k), primeiro(k + 20), 1.0, nominal));
        }
        let cedo = (2.0 * nominal.num as f64 / nominal.den as f64) as usize;
        if cedo + 31 <= n {
            v.push((primeiro(cedo), primeiro(cedo + 30), 1.0, nominal));
        }
        let meio = n / 2;
        if meio + 41 <= n {
            v.push((primeiro(meio), primeiro(meio + 20), 0.5, Racional { num: 25, den: 1 }));
            v.push((primeiro(meio), primeiro(meio + 40), 4.0, Racional { num: 30, den: 1 }));
        }
        v
    }

    /// Para cada par (original, cópia): o mesmo conteúdo em cada índice; o mesmo mapa nos dois (os mesmos quadros
    /// em cada slot); e cada trecho lido pela regra do motor, no original e na cópia, mostra os quadros previstos.
    /// Original sem tempos (fluxo cru) entra só pela cópia. Devolve (trechos, slots, trechos com busca).
    fn conferir_par(cap: &Capacidades, nome: &str, original: Option<&Path>, copia: &Path, lacunas: Option<usize>, pasta: &Path) -> (usize, u64, usize) {
        let k = apoio::medir_por_conteudo(cap, copia).unwrap_or_else(|e| panic!("{nome}: cópia: {e}"));
        let o = original.map(|p| apoio::medir_por_conteudo(cap, p).unwrap_or_else(|e| panic!("{nome}: original: {e}")));
        let n = o.as_ref().map_or(k.limite, |o| o.limite.min(k.limite));
        assert!(n >= 60, "{nome}: {n} quadros comparáveis");
        if let Some(o) = &o {
            assert_eq!(o.info.quadros.pts.len(), k.info.quadros.pts.len(), "{nome}: quadros na tabela");
            assert_eq!(o.numeros[..n], k.numeros[..n], "{nome}: conteúdo por índice");
            // as lacunas da tabela do decodificador (o que a Edição mostra) são as registradas na cópia
            if let Some(l) = lacunas {
                assert_eq!(o.info.quadros.lacunas.len(), l, "{nome}: lacunas da tabela");
            }
        }
        let (mut casos, mut slots, mut com_busca) = (0, 0u64, 0);
        for (i, &(j_in, j_out, v, fps)) in trechos(o.as_ref().unwrap_or(&k), n).iter().enumerate() {
            let vel = Velocidade::Constante { fator: v };
            let mk = mapa_trecho(&k.info.quadros, j_in, j_out, &vel, fps).unwrap();
            let rotulo = format!("{nome} [{j_in}, {j_out}) a {v}× em {}/{} ({})", fps.num, fps.den, cap.versao);
            let mut lidas = vec![("cópia", &k, mk.clone())];
            if let Some(o) = &o {
                let mo = mapa_trecho(&o.info.quadros, j_in, j_out, &vel, fps).unwrap();
                assert_eq!(mo.quadro_do_slot, mk.quadro_do_slot, "{rotulo}: mapa do original × da cópia");
                lidas.push(("original", o, mo));
            }
            for (qual, m, mapa) in lidas {
                let (ids, lidos, busca) = ler_trecho(cap, m, &mapa, fps, pasta, i * 2 + usize::from(qual == "original"));
                let previsto: Vec<u32> = mapa.quadro_do_slot.iter().map(|&j| m.numeros[j as usize]).collect();
                let q = ids.iter().zip(&previsto).position(|(a, b)| a != b);
                assert_eq!((ids.len(), q), (previsto.len(), None), "{rotulo}, {qual}: slots lidos × previstos (busca {busca})");
                assert_eq!(lidos, (j_out - j_in) as u64, "{rotulo}, {qual}: frames in");
                casos += 1;
                slots += mapa.n_q as u64;
                com_busca += usize::from(busca);
            }
        }
        eprintln!("{nome} ({}): {n} quadros comparáveis, {casos} trechos, {com_busca} com busca", cap.versao);
        (casos, slots, com_busca)
    }

    #[test]
    fn busca_e_mapa_certos_no_original_e_na_copia() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("dvr-busca");
        // (nome, original lido pela Edição?, original, formato, lacunas)
        let mut pares: Vec<(String, bool, PathBuf, Formato, Option<usize>)> = Vec::new();
        for &(nome, f, _, l) in REAIS {
            match amostra(nome) {
                Some(p) => pares.push((nome.into(), true, p, f, l)),
                None => eprintln!("aviso: amostra {nome} ausente, fora do teste"),
            }
        }
        for (p, f, _) in sinteticos(&cap, &d) {
            if p.ends_with("cortado.avi") || p.ends_with("comb.264") {
                pares.push((p.file_name().unwrap().to_string_lossy().into_owned(), !f.cru(), p, f, if f.cru() { None } else { Some(0) }));
            }
        }
        let total = std::sync::Mutex::new((0, 0u64, 0));
        std::thread::scope(|s| {
            for (i, (nome, com_original, origem, f, lacunas)) in pares.iter().enumerate() {
                let (cap, d, total) = (&cap, &d, &total);
                s.spawn(move || {
                    let pasta = d.join(format!("p{i}"));
                    std::fs::create_dir_all(&pasta).unwrap();
                    let copia = pasta.join("copia.mkv");
                    let c = copiar(cap, origem, *f, &copia, &AtomicBool::new(false)).unwrap();
                    if let Some(l) = lacunas {
                        assert_eq!(c.lacunas.len(), *l, "{nome}: lacunas registradas");
                    }
                    let r = conferir_par(cap, nome, com_original.then_some(origem.as_path()), &copia, *lacunas, &pasta);
                    let mut t = total.lock().unwrap();
                    *t = (t.0 + r.0, t.1 + r.1, t.2 + r.2);
                });
            }
        });
        let (casos, slots, com_busca) = total.into_inner().unwrap();
        eprintln!("busca e mapa ({}): {} pares, {casos} trechos, {slots} slots, {com_busca} com busca", cap.versao, pares.len());
        assert!(casos >= 10 && com_busca >= 2, "{casos} trechos, {com_busca} com busca");
    }

    // ------------------------------------------------------------ importação: avanço, cancelar, taxa suposta (P4.6b)

    /// Do pedido de cancelamento à volta de `preparar` (vigia de 10 ms, relator de 50 ms, espera dos processos): o
    /// medido fica abaixo de 0,3 s; 3× de folga mais a carga da máquina.
    const CANCELAR_MAX_S: f64 = 2.0;

    /// Avanço da prova pelo pts lido: maior pts − primeiro, em s, só do vídeo e só de linha inteira; a fração é a do
    /// lado mais atrasado sobre a duração da cópia.
    #[test]
    fn avanco_pelo_pts_lido() {
        let lido = Numero::novo();
        let mut l = Leitor::novo(Some(&lido));
        l.pedaco(b"#tb 0: 1/90000\n#tb 1: 1/8000\n0, 900000, 900000, 3600, 100, aaaa\n1, 8000, 8000, 160, 20, bbbb\n").unwrap();
        assert_eq!(lido.ler(), 0.0);
        // ordem de exibição com quadro B: o maior vale
        l.pedaco(b"0, 907200, 907200, 3600, 100, cccc\n0, 903600, 903600, 3600, 100, dddd\n1, 80000, 80000, 160, 20, eeee\n").unwrap();
        assert!((lido.ler() - 0.08).abs() < 1e-12, "{}", lido.ler());
        l.pedaco(b"0, 990000, 990000, 3600, 100, ff").unwrap();
        assert!((lido.ler() - 0.08).abs() < 1e-12, "linha partida contou");
        l.pedaco(b"ff\n").unwrap();
        assert!((lido.ler() - 1.0).abs() < 1e-12, "{}", lido.ler());
        assert_eq!(l.fim(true).unwrap().video.quadros, 4);
        assert_eq!(fracao_da_prova(1.0, 0.5, 2.0), Some(0.25));
        assert_eq!(fracao_da_prova(0.5, 1.0, 2.0), Some(0.25));
        assert_eq!(fracao_da_prova(3.0, 3.0, 2.0), Some(1.0));
        assert_eq!(fracao_da_prova(1.0, 1.0, 0.0), None);
        // fase Cópia: sem medida, sem fração
        let a = Andamento::novo();
        a.copia.por(0.4);
        assert_eq!(a.agora(), (Fase::Copia, None));
        a.copia_medida.store(true, Ordering::SeqCst);
        assert_eq!(a.agora(), (Fase::Copia, Some(0.4)));
        a.prova.store(true, Ordering::SeqCst);
        a.duracao.por(10.0);
        a.original.por(f64::MAX);
        a.lida.por(2.5);
        assert_eq!(a.agora(), (Fase::Prova, Some(0.25)));
    }

    /// Caso vazio (o original ainda não entrou) dentro de `pasta`, com acento, espaço e % no caminho.
    fn caso_vazio(pasta: &Path) -> PathBuf {
        let pai = pasta.join("Casos ç 100%");
        std::fs::create_dir_all(&pai).unwrap();
        crate::workspace::create::create_workspace(crate::models::NewOccurrenceInput::default(), &pai).unwrap().path
    }

    fn registrar_original(ws: &Path, origem: &Path, duplicado: bool) -> VideoMedia {
        let input = crate::models::RegisterVideoInput { source_path: origem.to_string_lossy().into(), allow_duplicate: duplicado };
        tauri::async_runtime::block_on(crate::commands::video_commands::register_video_media(ws.to_string_lossy().into(), input)).unwrap()
    }

    /// Fonte que leva segundos para provar: o Dahua real (4 MP) ou, sem ele, um AVI sem índice de 40 s em 720p com som.
    fn fonte_longa(cap: &Capacidades, pasta: &Path) -> PathBuf {
        let (fonte, ext) = match amostra("dahua.dav") {
            Some(p) => (p, "dav"),
            None => {
                ffmpeg(
                    cap,
                    pasta,
                    &[
                        "-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=25", "-f", "lavfi", "-i", "sine=f=440:r=8000", "-t", "40", "-c:v", "libx264",
                        "-preset", "ultrafast", "-bf", "0", "-pix_fmt", "yuv420p", "-c:a", "pcm_mulaw", "-ar", "8000", "longo.avi",
                    ],
                );
                let b = std::fs::read(pasta.join("longo.avi")).unwrap();
                let k = b.windows(4).rposition(|w| w == b"idx1").unwrap();
                std::fs::write(pasta.join("longo-cortado.avi"), &b[..k - 5000]).unwrap();
                (pasta.join("longo-cortado.avi"), "avi")
            }
        };
        let origem = pasta.join(format!("câm 1 %d.{ext}"));
        std::fs::copy(fonte, &origem).unwrap();
        origem
    }

    /// Processos vivos filhos deste com `marca` na linha de comando.
    #[cfg(target_os = "linux")]
    fn filhos_com(marca: &Path) -> Vec<String> {
        let (eu, marca) = (std::process::id(), marca.to_string_lossy().into_owned());
        std::fs::read_dir("/proc")
            .unwrap()
            .flatten()
            .filter_map(|e| {
                let p: u32 = e.file_name().to_string_lossy().parse().ok()?;
                let stat = std::fs::read_to_string(format!("/proc/{p}/stat")).ok()?;
                let mut c = stat[stat.rfind(')')? + 2..].split_whitespace();
                let (estado, pai) = (c.next()?, c.next()?.parse::<u32>().ok()?);
                if pai != eu || estado == "Z" {
                    return None;
                }
                let cmd = String::from_utf8_lossy(&std::fs::read(format!("/proc/{p}/cmdline")).ok()?).replace('\0', " ");
                cmd.contains(&marca).then_some(cmd)
            })
            .collect()
    }

    fn arquivos_das_copias(ws: &Path) -> Vec<String> {
        match std::fs::read_dir(ws.join(REL_COPIAS_DVR)) {
            Ok(rd) => rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Cancelar na fase Cópia e no meio da prova: volta logo com `cancelado`, nenhum ffmpeg vivo, nenhum arquivo na
    /// pasta das cópias, nenhuma mídia nem registro no caso e o arquivo escolhido com o mesmo SHA-256.
    #[cfg(target_os = "linux")]
    #[test]
    fn cancelar_mata_os_processos_e_nao_deixa_nada_no_caso() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("dvr-cancelar");
        let ws = caso_vazio(&d);
        let origem = fonte_longa(&cap, &d);
        let antes = sha256_file(&origem).unwrap();
        type Quando = fn(Fase, Option<f64>) -> bool;
        let pontos: [(&str, Quando); 2] = [("cópia", |f, _| f == Fase::Copia), ("prova", |f, x| f == Fase::Prova && x.is_some_and(|x| x >= 0.2))];
        for (rotulo, quando) in pontos {
            let cancelar = AtomicBool::new(false);
            let pedido = Mutex::new(None::<(std::time::Instant, usize)>);
            let avanco = |f: Fase, x: Option<f64>| {
                if quando(f, x) && !cancelar.swap(true, Ordering::SeqCst) {
                    *pedido.lock().unwrap() = Some((std::time::Instant::now(), filhos_com(&d).len()));
                }
            };
            let r = preparar(&cap, &ws, &origem, &cancelar, &avanco);
            let (quando_pediu, vivos_no_pedido) = pedido.lock().unwrap().expect("o ponto de cancelar não chegou");
            let espera = quando_pediu.elapsed().as_secs_f64();
            let e = r.map(|_| ()).unwrap_err().to_string();
            eprintln!("cancelar na {rotulo} ({}): {espera:.3} s, {vivos_no_pedido} processos no pedido: {e}", cap.versao);
            assert!(e.contains("cancelado"), "{rotulo}: {e}");
            assert!(espera < CANCELAR_MAX_S, "{rotulo}: {espera} s do pedido à volta");
            if rotulo == "prova" {
                assert!(vivos_no_pedido >= 2, "{rotulo}: {vivos_no_pedido} (original e cópia decodificando)");
            }
            assert_eq!(filhos_com(&d), Vec::<String>::new(), "{rotulo}: ffmpeg vivo");
            assert_eq!(arquivos_das_copias(&ws), Vec::<String>::new(), "{rotulo}: sobrou arquivo");
            let (conn, occ) = crate::edicao::executar::abrir_banco(&ws).unwrap();
            assert!(video_repo::list_media_for_occurrence(&conn, &occ).unwrap().is_empty(), "{rotulo}: mídia no caso");
            let contar = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
            assert_eq!(contar("SELECT COUNT(*) FROM video_operation_logs"), 0, "{rotulo}: registro no caso");
            assert_eq!(contar("SELECT COUNT(*) FROM audit_logs WHERE module = 'video'"), 0, "{rotulo}: auditoria no caso");
            assert!(pendentes().iter().all(|p| p.ws != ws), "{rotulo}: pendente");
            assert_eq!(sha256_file(&origem).unwrap(), antes, "{rotulo}: arquivo escolhido alterado");
        }
    }

    /// A importação: a cópia sai do arquivo escolhido antes de o original entrar no caso (nada no banco até lá), com
    /// avanço real (Cópia, depois Prova, sem voltar, com passos no meio); registrado o original, a cópia entra derivada
    /// dele e o comando gravado lê o original do caso. Reimportar o mesmo arquivo para logo e devolve a mesma cópia;
    /// original que não é o arquivo de que a cópia saiu é recusado.
    #[test]
    fn importacao_prepara_antes_e_registra_depois_do_original() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("dvr-importar");
        let ws = caso_vazio(&d);
        let origem = fonte_longa(&cap, &d);
        let antes = sha256_file(&origem).unwrap();
        let eventos = Mutex::new(Vec::<(Fase, Option<f64>)>::new());
        let p = preparar(&cap, &ws, &origem, &AtomicBool::new(false), &|f, x| eventos.lock().unwrap().push((f, x))).unwrap();
        assert!(!p.ja_existia);
        let (conn, occ) = crate::edicao::executar::abrir_banco(&ws).unwrap();
        assert!(video_repo::list_media_for_occurrence(&conn, &occ).unwrap().is_empty(), "mídia antes do original");
        let nomes = arquivos_das_copias(&ws);
        assert!(nomes.len() == 1 && nomes[0].starts_with('.') && nomes[0].ends_with(".parcial"), "{nomes:?}");
        // avanço
        let ev = eventos.into_inner().unwrap();
        eprintln!("avanço ({}): {} eventos, {:?}", cap.versao, ev.len(), ev.iter().map(|(f, x)| format!("{f:?} {x:.3?}")).collect::<Vec<_>>());
        let k = ev.iter().position(|e| e.0 == Fase::Prova).expect("sem fase Prova");
        assert!(k > 0 && ev[k..].iter().all(|e| e.0 == Fase::Prova), "Cópia depois de Prova");
        for fase in [&ev[..k], &ev[k..]] {
            let x: Vec<f64> = fase.iter().filter_map(|e| e.1).collect();
            assert!(x.windows(2).all(|w| w[1] >= w[0]), "fração voltou: {x:?}");
        }
        let prova: Vec<f64> = ev[k..].iter().filter_map(|e| e.1).collect();
        assert!(prova.iter().filter(|x| **x > 0.05 && **x < 0.95).count() >= 3, "prova sem passos no meio: {prova:?}");
        assert!(prova.last().is_some_and(|x| *x >= 0.9), "prova não chegou ao fim: {prova:?}");
        // registro
        let orig = registrar_original(&ws, &origem, false);
        let r = registrar_pendente(&cap, &ws, &orig.id.to_string(), &p.chave).unwrap();
        assert!(!r.ja_existia);
        assert_eq!(r.media.derived_from_hash.as_deref(), Some(orig.sha256.as_str()));
        let arquivo = resolver_relativo(&ws, &r.media.relative_path);
        assert_eq!(sha256_file(&arquivo).unwrap(), r.media.sha256);
        assert_eq!(arquivos_das_copias(&ws), vec![r.media.filename.clone()]);
        let j: Value = serde_json::from_str(r.media.derivation_json.as_deref().unwrap()).unwrap();
        let cmd: Vec<&str> = j["ffmpeg_command"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        let no_caso = resolver_relativo(&ws, &orig.relative_path);
        assert!(cmd.windows(2).any(|w| w[0] == "-i" && w[1] == no_caso.to_string_lossy()), "{cmd:?}");
        assert_eq!(cmd.last().copied(), Some(arquivo.to_string_lossy().as_ref()));
        assert!(!cmd.iter().any(|a| *a == origem.to_string_lossy()), "{cmd:?}");
        assert_eq!(sha256_file(&origem).unwrap(), antes, "arquivo escolhido alterado");
        // a chave vale uma vez
        let e = registrar_pendente(&cap, &ws, &orig.id.to_string(), &p.chave).unwrap_err().to_string();
        assert!(e.contains(codigo::ITEM_INVALIDO), "{e}");
        // reimportar: para assim que o SHA-256 sai, nada gravado, a mesma cópia
        let eventos = Mutex::new(Vec::<(Fase, Option<f64>)>::new());
        let p2 = preparar(&cap, &ws, &origem, &AtomicBool::new(false), &|f, x| eventos.lock().unwrap().push((f, x))).unwrap();
        assert!(p2.ja_existia);
        let ev = eventos.into_inner().unwrap();
        assert!(!ev.iter().any(|e| e.0 == Fase::Prova && e.1.is_some_and(|x| x >= 0.5)), "reimportação provou de novo: {ev:?}");
        assert_eq!(arquivos_das_copias(&ws), vec![r.media.filename.clone()]);
        let n = video_repo::list_media_for_occurrence(&conn, &occ).unwrap().len();
        let r2 = registrar_pendente(&cap, &ws, &orig.id.to_string(), &p2.chave).unwrap();
        assert_eq!((r2.ja_existia, r2.media.id), (true, r.media.id));
        assert_eq!(video_repo::list_media_for_occurrence(&conn, &occ).unwrap().len(), n);
        // original registrado que não é o arquivo de que a cópia saiu (um byte a mais no fim): recusado
        let outro = d.join(format!("outro.{}", origem.extension().unwrap().to_string_lossy()));
        let mut b = std::fs::read(&origem).unwrap();
        b.push(0);
        std::fs::write(&outro, b).unwrap();
        std::fs::remove_file(&arquivo).unwrap();
        let p3 = preparar(&cap, &ws, &origem, &AtomicBool::new(false), &|_, _| {}).unwrap();
        assert!(!p3.ja_existia);
        let orig_outro = registrar_original(&ws, &outro, false);
        let e = registrar_pendente(&cap, &ws, &orig_outro.id.to_string(), &p3.chave).unwrap_err().to_string();
        assert!(e.contains(codigo::COPIA_DIVERGENTE) && e.ends_with("original"), "{e}");
        assert_eq!(arquivos_das_copias(&ws), Vec::<String>::new(), "sobrou o .parcial");
        assert_eq!(video_repo::list_media_for_occurrence(&conn, &occ).unwrap().len(), n + 1, "só o outro original entrou");
    }

    /// AVI sem índice com som grava uma cópia só (a sem `+genpts` primeiro); o Dahua e o AVI sem som, a da receita.
    #[cfg(unix)]
    #[test]
    fn uma_gravacao_quando_a_primeira_prova() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("dvr-uma");
        let conta = ffmpeg_com_defeito(&cap, &d, "ffmpeg-conta", r#"echo x >> "$(dirname "$fim")/gravacoes"; exec "$real" "$@""#);
        let com_som = sinteticos(&cap, &d).into_iter().find(|x| x.1 == Formato::AviSemIndice).unwrap().0;
        ffmpeg(&cap, &d, &["-i", &com_som.to_string_lossy(), "-map", "0:v", "-c", "copy", "mudo.avi"]);
        let b = std::fs::read(d.join("mudo.avi")).unwrap();
        let k = b.windows(4).rposition(|w| w == b"idx1").unwrap();
        std::fs::write(d.join("mudo-cortado.avi"), &b[..k - 5000]).unwrap();
        let mut casos = vec![(com_som, Formato::AviSemIndice, false), (d.join("mudo-cortado.avi"), Formato::AviSemIndice, true)];
        casos.extend(amostra("dahua.dav").map(|p| (p, Formato::Dahua, true)));
        for (i, (origem, f, genpts)) in casos.iter().enumerate() {
            let pasta = d.join(format!("u{i}"));
            std::fs::create_dir_all(&pasta).unwrap();
            let c = copiar(&conta, origem, *f, &pasta.join("c.mkv"), &AtomicBool::new(false)).unwrap();
            let n = std::fs::read_to_string(pasta.join("gravacoes")).unwrap().lines().count();
            assert_eq!((n, c.comando.iter().any(|a| a == "+genpts")), (1, *genpts), "{} ({})", origem.display(), cap.versao);
        }
    }

    /// Fluxo cru sem temporização no SPS (x265 sem VUI de tempo): a taxa é a padrão (25), marcada como suposta na cópia,
    /// no `derivation_json` e na `Derivacao` da Edição; com a temporização (x264, 15 qps), não.
    #[test]
    fn fluxo_cru_sem_temporizacao_marca_a_taxa_suposta() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("dvr-taxa");
        let src = ["-f", "lavfi", "-i", "testsrc2=size=320x240:rate=15", "-t", "6", "-pix_fmt", "yuv420p"];
        let mut a = src.to_vec();
        a.extend(["-c:v", "libx265", "-preset", "veryfast", "-x265-params", "log-level=error:bframes=0:vui-timing-info=0:hrd=0", "-f", "hevc", "sem.h265"]);
        ffmpeg(&cap, &d, &a);
        let mut a = src.to_vec();
        a.extend(["-c:v", "libx264", "-preset", "veryfast", "-bf", "0", "-f", "h264", "com.264"]);
        ffmpeg(&cap, &d, &a);
        let t = Trabalho::nova().unwrap();
        let quinze = Racional { num: 15, den: 1 };
        assert_eq!(taxa_do_fluxo(&cap, &t, &d.join("sem.h265"), Formato::HevcCru).unwrap(), (TAXA_PADRAO, true), "{}", cap.versao);
        assert_eq!(taxa_do_fluxo(&cap, &t, &d.join("com.264"), Formato::H264Cru).unwrap(), (quinze, false), "{}", cap.versao);
        for (nome, f, taxa, suposta) in [("sem.h265", Formato::HevcCru, TAXA_PADRAO, true), ("com.264", Formato::H264Cru, quinze, false)] {
            let origem = d.join(nome);
            let destino = d.join(format!("{nome}.mkv"));
            let c = copiar(&cap, &origem, f, &destino, &AtomicBool::new(false)).unwrap();
            assert_eq!((c.taxa, c.taxa_suposta, c.quadros), (Some(taxa), suposta, 90), "{nome}");
            assert!(c.comando.windows(2).any(|w| w[0] == "-framerate" && w[1] == format!("{}/{}", taxa.num, taxa.den)), "{:?}", c.comando);
            conferir_copia(&cap, &origem, &destino, &c, &d);
            let (_t, ws, orig) = caso_com(&origem);
            let r = copia_no_caso(&cap, &ws, &orig.id.to_string(), &AtomicBool::new(false)).unwrap();
            let j: Value = serde_json::from_str(r.media.derivation_json.as_deref().unwrap()).unwrap();
            assert_eq!((&j["taxa_suposta"], j["taxa"].as_str()), (&json!(suposta), Some(format!("{}/{}", taxa.num, taxa.den).as_str())), "{nome}");
            let (conn, _) = crate::edicao::executar::abrir_banco(&ws).unwrap();
            let der = crate::edicao::importar_v1::derivacao(&conn, &r.media).unwrap().unwrap();
            assert_eq!(der.taxa_suposta, suposta.then_some(taxa), "{nome}");
            // no fio só quando supôs
            assert_eq!(serde_json::to_value(&der).unwrap().get("taxa_suposta").is_some(), suposta, "{nome}");
        }
    }

    // ------------------------------------------------------------ bancada

    /// Tempo da cópia provada (fora da suíte): o Dahua real e o mesmo conteúdo em AVI sem índice, com som (a primeira
    /// cópia não passa pelos tempos e entra a segunda). `SICRO_BANCADA_VEZES` repetições (3).
    #[test]
    #[ignore = "bancada"]
    fn bancada_tempo_da_copia() {
        let Some(cap) = fontes::cap() else { return };
        let Some(dav) = amostra("dahua.dav") else { return };
        let d = fontes::pasta_temporaria("dvr-bancada");
        ffmpeg(&cap, &d, &["-i", &dav.to_string_lossy(), "-map", "0:v", "-map", "0:a", "-c", "copy", "dahua.avi"]);
        let b = std::fs::read(d.join("dahua.avi")).unwrap();
        let k = b.windows(4).rposition(|w| w == b"idx1").unwrap();
        std::fs::write(d.join("dahua-cortado.avi"), &b[..k - 5000]).unwrap();
        let vezes: usize = std::env::var("SICRO_BANCADA_VEZES").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
        for (origem, f) in [(dav, Formato::Dahua), (d.join("dahua-cortado.avi"), Formato::AviSemIndice)] {
            for i in 0..vezes {
                let destino = d.join(format!("b{i}.mkv"));
                let t0 = std::time::Instant::now();
                let c = copiar(&cap, &origem, f, &destino, &AtomicBool::new(false)).unwrap();
                let s = t0.elapsed().as_secs_f64();
                let genpts = c.comando.iter().any(|a| a == "+genpts");
                eprintln!("bancada {} ({}): {s:.2} s, {} quadros, genpts {genpts}", origem.display(), cap.versao, c.quadros);
                std::fs::remove_file(&destino).unwrap();
            }
        }
    }

    // ------------------------------------------------------------ registro no caso

    /// Caso de verdade (acento, espaço e % no caminho e no nome) com o original registrado pelo comando da importação.
    fn caso_com(origem: &Path) -> (tempfile::TempDir, PathBuf, VideoMedia) {
        let d = tempfile::Builder::new().prefix("p4.6-dvr-").tempdir().unwrap();
        let pai = d.path().join("Casos ç 100%");
        std::fs::create_dir_all(&pai).unwrap();
        let ws = crate::workspace::create::create_workspace(crate::models::NewOccurrenceInput::default(), &pai).unwrap().path;
        let fonte = d.path().join(format!("câm 1 %d.{}", origem.extension().unwrap_or_default().to_string_lossy()));
        std::fs::copy(origem, &fonte).unwrap();
        let input = crate::models::RegisterVideoInput { source_path: fonte.to_string_lossy().into(), allow_duplicate: false };
        let m = tauri::async_runtime::block_on(crate::commands::video_commands::register_video_media(ws.to_string_lossy().into(), input)).unwrap();
        (d, ws, m)
    }

    /// Registro da cópia: mídia derivada no caso (arquivo, SHA-256, log, auditoria), `derivation_json` completo, a
    /// `Derivacao` que a Edição tira dele (tipo, cadeia, sem perdas, `inicio_epoca_s`) e o anexo repetido.
    fn conferir_registro(cap: &Capacidades, origem: &Path, formato: Formato, epoca: Option<f64>, n_lacunas: usize) {
        let (_d, ws, orig) = caso_com(origem);
        let r = copia_no_caso(cap, &ws, &orig.id.to_string(), &AtomicBool::new(false)).unwrap();
        let m = &r.media;
        assert_eq!((r.formato, r.ja_existia, r.inicio_epoca_s, r.lacunas.len()), (formato, false, epoca, n_lacunas));
        assert_eq!(m.derived_from_hash.as_deref(), Some(orig.sha256.as_str()));
        assert!(m.relative_path.starts_with(&format!("{REL_COPIAS_DVR}/")) && m.filename.ends_with(".mkv"), "{}", m.relative_path);
        assert_eq!(m.filename, format!("{}.mkv", Path::new(&orig.filename).file_stem().unwrap().to_string_lossy()));
        assert!(orig.filename.contains("câm 1 %d"), "{}", orig.filename);
        let arquivo = resolver_relativo(&ws, &m.relative_path);
        assert_eq!(sha256_file(&arquivo).unwrap(), m.sha256);
        assert_eq!(sha256_file(&resolver_relativo(&ws, &orig.relative_path)).unwrap(), orig.sha256, "original do caso alterado");
        let j: Value = serde_json::from_str(m.derivation_json.as_deref().unwrap()).unwrap();
        assert_eq!((j["kind"].as_str(), &j["formato"], j["source"]["sha256"].as_str()), (Some("copia_dvr"), &json!(formato), Some(orig.sha256.as_str())));
        assert_eq!(j["framemd5_sha256"].as_str().map(str::len), Some(64));
        assert_eq!(j["inicio_epoca_s"].as_f64(), epoca);
        assert_eq!(j["lacunas"].as_array().map(Vec::len), Some(n_lacunas));
        assert_eq!(j["ffmpeg"]["versao"], json!(cap.versao));
        // a Edição: `MidiaRef.derivacao` de uma mídia do caso
        let (conn, occ) = crate::edicao::executar::abrir_banco(&ws).unwrap();
        let der = crate::edicao::importar_v1::derivacao(&conn, m).unwrap().unwrap();
        assert_eq!(
            (der.tipo, der.de_sha256.as_str(), der.raiz_sha256.as_str(), der.com_perdas, der.inicio_epoca_s),
            (crate::edicao::modelo::TipoDerivacao::CopiaDvr, orig.sha256.as_str(), orig.sha256.as_str(), false, epoca)
        );
        assert!(der.comando.contains("-c copy -copyinkf") || der.comando.contains("-c:v ffv1"), "{}", der.comando);
        // contêiner com tempos: nada suposto
        assert!(j["taxa_suposta"].is_null() && der.taxa_suposta.is_none(), "{}", j["taxa_suposta"]);
        // o comando registrado lê o original do caso e grava no nome final da cópia
        let cmd: Vec<&str> = j["ffmpeg_command"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert_eq!(cmd.last().copied(), Some(arquivo.to_string_lossy().as_ref()));
        assert!(cmd.windows(2).any(|w| w[0] == "-i" && w[1] == resolver_relativo(&ws, &orig.relative_path).to_string_lossy()), "{cmd:?}");
        // no fio: o campo só vai quando há data; sem ele, lê como None
        let fio = serde_json::to_value(&der).unwrap();
        assert_eq!(fio.get("inicio_epoca_s").and_then(Value::as_f64), epoca);
        let mut velho = fio.clone();
        velho.as_object_mut().unwrap().remove("inicio_epoca_s");
        assert_eq!(serde_json::from_value::<crate::edicao::modelo::Derivacao>(velho).unwrap().inicio_epoca_s, None);
        let logs = video_repo::list_logs_for_media(&conn, &occ, &orig.sha256, 20).unwrap();
        assert!(logs.iter().any(|l| l.action == "dvr.copia" && l.details_json.contains(&m.sha256)));
        // anexo repetido: a mesma cópia, nada novo no caso
        let n = video_repo::list_media_for_occurrence(&conn, &occ).unwrap().len();
        let r2 = copia_no_caso(cap, &ws, &orig.id.to_string(), &AtomicBool::new(false)).unwrap();
        assert_eq!((r2.ja_existia, r2.media.id, r2.lacunas.len(), r2.inicio_epoca_s), (true, m.id, n_lacunas, epoca));
        assert_eq!(video_repo::list_media_for_occurrence(&conn, &occ).unwrap().len(), n);
        assert_eq!(std::fs::read_dir(ws.join(REL_COPIAS_DVR)).unwrap().count(), 1);
        // arquivo da cópia apagado da pasta: refeita, dá os mesmos bytes e volta ao registro que já havia
        std::fs::remove_file(&arquivo).unwrap();
        let r3 = copia_no_caso(cap, &ws, &orig.id.to_string(), &AtomicBool::new(false)).unwrap();
        assert_eq!((r3.ja_existia, r3.media.id), (true, m.id));
        assert_eq!(sha256_file(&arquivo).unwrap(), m.sha256);
        assert_eq!(video_repo::list_media_for_occurrence(&conn, &occ).unwrap().len(), n);
        assert_eq!(std::fs::read_dir(ws.join(REL_COPIAS_DVR)).unwrap().count(), 1);
        eprintln!("registro ({}): {} → {} ({:?})", cap.versao, orig.filename, m.relative_path, epoca);
    }

    #[test]
    fn registra_a_copia_e_a_derivacao() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("dvr-registro");
        let avi = sinteticos(&cap, &d).into_iter().find(|x| x.1 == Formato::AviSemIndice).unwrap().0;
        conferir_registro(&cap, &avi, Formato::AviSemIndice, None, 0);
        match amostra("dahua.dav") {
            // relógio do DVR: 2017-09-18 19:24:59
            Some(p) => conferir_registro(&cap, &p, Formato::Dahua, Some(1_505_762_699.0), 1),
            None => eprintln!("aviso: dahua.dav ausente, registro com época fora do teste"),
        }
        // vídeo comum e mídia de outro caso: recusados sem cópia
        let (_d, ws, comum) = caso_com(&fontes::gerar(Fonte::Mp4).unwrap());
        let e = copia_no_caso(&cap, &ws, &comum.id.to_string(), &AtomicBool::new(false)).unwrap_err().to_string();
        assert!(e.contains(codigo::GRAVADOR_NAO_SUPORTADO), "{e}");
        let e = copia_no_caso(&cap, &ws, &Uuid::new_v4().to_string(), &AtomicBool::new(false)).unwrap_err().to_string();
        assert!(e.contains(codigo::MIDIA_AUSENTE), "{e}");
        assert!(!ws.join(REL_COPIAS_DVR).exists());
    }
}
