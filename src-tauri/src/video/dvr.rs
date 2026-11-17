//! Gravadores (DVR), P4.6 (`receitas-audio-saida.md` §9): formato pelos bytes do começo, cópia de trabalho em MKV
//! por remux (`-fflags +genpts -c copy -copyinkf`) e a prova de que ela traz os mesmos quadros: lista `framemd5`
//! igual à do original (vídeo e áudio) e tempos no mesmo passo. O original fica como chegou; a cópia entra no caso
//! como mídia derivada (`derivation_json.kind = "copia_dvr"`), com o `start_time` do original e as lacunas de pts.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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

/// `format.start_time` do original (s); None se o contêiner não tem.
fn inicio_do_conteiner(cap: &Capacidades, t: &Trabalho, video: &Path) -> Result<Option<f64>> {
    let j = ffprobe_json(cap, t, "inicio", textos(&["-v", "error", "-show_entries", "format=start_time", "-of", "json", &video.to_string_lossy()]))?;
    Ok(numero(&j["format"]["start_time"]))
}

/// Taxa do fluxo cru pela duração que o analisador dá ao primeiro pacote (sai do SPS quando ele traz a temporização;
/// sem ela, a padrão do demuxer, 25). Fora de 1 a 240 qps: 25.
fn taxa_do_fluxo(cap: &Capacidades, t: &Trabalho, video: &Path, f: Formato) -> Result<Racional> {
    let demuxer = f.demuxer().unwrap_or("h264");
    let j = ffprobe_json(
        cap,
        t,
        "taxa",
        textos(&[
            "-v", "error", "-f", demuxer, "-select_streams", "v:0", "-show_entries", "stream=time_base:packet=duration", "-read_intervals", "%+#2",
            "-of", "json", &video.to_string_lossy(),
        ]),
    )?;
    let tb = j["streams"][0]["time_base"].as_str().and_then(fracao);
    let dur = j["packets"].as_array().and_then(|p| p.iter().find_map(|x| numero(&x["duration"]))).filter(|d| *d >= 1.0).map(|d| d as i64);
    let taxa = match (tb, dur) {
        (Some(tb), Some(d)) => {
            let (num, den) = (tb.den, tb.num.saturating_mul(d));
            Some(Racional { num: num / mdc(num, den), den: den / mdc(num, den) })
        }
        _ => None,
    };
    Ok(taxa.filter(|r| r.num >= r.den && r.num <= 240 * r.den).unwrap_or(Racional { num: 25, den: 1 }))
}

/// Opções de entrada (antes do `-i`): fluxo cru vai pelo demuxer com a taxa posta.
fn entrada(f: Formato, taxa: Option<Racional>) -> Vec<String> {
    match (f.cru(), taxa) {
        (true, Some(r)) => vec!["-framerate".into(), format!("{}/{}", r.num, r.den), "-f".into(), f.demuxer().unwrap_or("h264").into()],
        _ => Vec::new(),
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
}

/// Leitura em fluxo das linhas do `framemd5` (o arquivo de um dia de gravação não cabe inteiro na memória como texto).
struct Leitor {
    h: [Sha256; 2],
    n: [u64; 2],
    tb: Option<Racional>,
    pts: Vec<i64>,
    pts_ok: bool,
    resto: Vec<u8>,
}

impl Leitor {
    fn novo() -> Self {
        Leitor { h: [Sha256::new(), Sha256::new()], n: [0, 0], tb: None, pts: Vec::new(), pts_ok: true, resto: Vec::new() }
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
        if k == 0 {
            if self.pts.len() >= MAX_QUADROS {
                return Err(SicroError::Validation(codigo::TABELA_GRANDE.into()));
            }
            match pts.parse::<i64>() {
                Ok(p) => self.pts.push(p),
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
    let t = Trabalho::nova()?;
    let audio = tem_audio(cap, &t, video, entrada)?;
    let rodar = |com_audio: bool| -> Result<Option<Lista>> {
        let mut a = textos(&["-hide_banner", "-nostdin", "-nostats", "-loglevel", "error", "-threads", "1"]);
        a.extend(entrada.iter().cloned());
        a.extend(textos(&["-copyts", "-i", &video.to_string_lossy(), "-map", "0:v:0"]));
        if com_audio {
            a.extend(textos(&["-map", "0:a:0"]));
        }
        a.extend(textos(&["-fps_mode", "passthrough", "-enc_time_base", "demux", "-f", "framemd5", "-"]));
        let cmd = t.comando(if com_audio { "framemd5-va" } else { "framemd5-v" }, &cap.binario, a);
        let mut leitor = Leitor::novo();
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
/// um tique da base mais grossa (a do MKV é 1 ms). None = algum quadro saiu do passo.
fn deslocamento(o: &Lista, c: &Lista) -> Option<f64> {
    let (to, tc) = (o.tb?, c.tb?);
    if o.pts.len() != c.pts.len() || o.pts.is_empty() {
        return None;
    }
    // em unidades de 1/(den_o·den_c)
    let d = |j: usize| o.pts[j] as i128 * to.num as i128 * tc.den as i128 - c.pts[j] as i128 * tc.num as i128 * to.den as i128;
    let folga = (to.num as i128 * tc.den as i128).max(tc.num as i128 * to.den as i128);
    let d0 = d(0);
    (0..o.pts.len()).all(|j| (d(j) - d0).abs() <= folga).then(|| d0 as f64 / (to.den as f64 * tc.den as f64))
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

/// Faz a cópia de trabalho de `origem` em `destino` (MKV) e prova pelos quadros. Prova que falha apaga o destino e
/// devolve `copia_divergente`. Fluxo cru: remux com a taxa do fluxo; com quadros B (o remux não grava pacote sem
/// tempo, código 234), FFV1 sem perdas.
pub fn copiar(cap: &Capacidades, origem: &Path, formato: Formato, destino: &Path, cancelar: &AtomicBool) -> Result<Copia> {
    exigir_suportado(formato)?;
    let t = Trabalho::nova()?;
    let taxa = if formato.cru() { Some(taxa_do_fluxo(cap, &t, origem, formato)?) } else { None };
    let ent = entrada(formato, taxa);
    let inicio = if formato.cru() { None } else { inicio_do_conteiner(cap, &t, origem)? };
    // comando como rodou e a lista da cópia; None = o remux do fluxo cru não gravou, ou a cópia não se lê
    let fazer = |recodificar: bool| -> Result<(Vec<String>, Option<Lista>)> {
        let mut a = textos(&["-hide_banner", "-nostdin", "-nostats", "-loglevel", "error", "-y"]);
        a.extend(ent.iter().cloned());
        a.extend(textos(&["-fflags", "+genpts", "-i", &origem.to_string_lossy(), "-map", "0:v", "-map", "0:a?"]));
        if recodificar {
            a.extend(textos(&["-c:v", "ffv1", "-level", "3", "-g", "1", "-c:a", "copy"]));
        } else {
            a.extend(textos(&["-c", "copy", "-copyinkf"]));
        }
        // `+bitexact`: sem os identificadores aleatórios do MKV, a mesma cópia dá os mesmos bytes
        a.extend(textos(&["-map_metadata", "0", "-fflags", "+bitexact", "-f", "matroska", &destino.to_string_lossy()]));
        let cmd = t.comando(if recodificar { "ffv1" } else { "remux" }, &cap.binario, a);
        let fim = exec::rodar(&cmd, cancelar)?;
        if fim.cancelado {
            return Err(SicroError::Validation("cancelado".into()));
        }
        let mut comando = vec![cap.binario.to_string_lossy().into_owned()];
        comando.extend(cmd.args.iter().cloned());
        if fim.codigo != Some(0) {
            if formato.cru() && !recodificar {
                return Ok((comando, None));
            }
            exec::exigir_sucesso(&cmd, &fim)?;
        }
        // cópia que nem se lê (IFV sem `-copyinkf`: 0 pacotes de vídeo, código 0) é cópia divergente
        match lista_framemd5(cap, destino, &[], cancelar) {
            Ok(l) => Ok((comando, Some(l))),
            Err(e) if cancelar.load(Ordering::Relaxed) => Err(e),
            Err(_) => Ok((comando, None)),
        }
    };
    // a lista do original sai enquanto a cópia é feita
    let (original, primeira) = std::thread::scope(|s| {
        let o = s.spawn(|| lista_framemd5(cap, origem, &ent, cancelar));
        let c = fazer(false);
        (o.join().unwrap_or_else(|_| Err(SicroError::Validation(codigo::PANICO.into()))), c)
    });
    let original = apagando(destino, original)?;
    let (mut comando, mut copia) = apagando(destino, primeira)?;
    let mut recodificado = false;
    if formato.cru() && !copia.as_ref().is_some_and(|l| mesmo(&original, l)) {
        (comando, copia) = apagando(destino, fazer(true))?;
        recodificado = true;
    }
    let copia = match copia {
        Some(l) if mesmo(&original, &l) => l,
        outra => {
            let n = outra.map_or(0, |l| l.video.quadros);
            let detalhe = format!("{} × {n} quadros", original.video.quadros);
            return apagando(destino, Err(validacao(codigo::COPIA_DIVERGENTE, Some(&detalhe))));
        }
    };
    // original com quadro sem pts: o `+genpts` da cópia inventa esse tempo, não há o que comparar
    let desloc_s = match formato.cru() || original.pts.is_empty() {
        true => None,
        false => Some(apagando(destino, deslocamento(&original, &copia).ok_or_else(|| validacao(codigo::COPIA_DIVERGENTE, Some("tempos"))))?),
    };
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

/// Faz (ou acha) a cópia de trabalho do vídeo `media_id` do caso e a registra como mídia derivada, com log e auditoria.
pub fn copia_no_caso(cap: &Capacidades, ws: &Path, media_id: &str, cancelar: &AtomicBool) -> Result<CopiaRegistrada> {
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
    if let Some(m) = copia_existente(&conn, ws, &occ, &orig.sha256)? {
        let (lacunas, inicio_epoca_s) = de_derivacao(&m);
        return Ok(CopiaRegistrada { media: m, formato, lacunas, inicio_epoca_s, ja_existia: true });
    }
    let pasta = ws.join(REL_COPIAS_DVR);
    std::fs::create_dir_all(&pasta).map_err(|e| SicroError::Filesystem(format!("{}: {e}", pasta.display())))?;
    let base = Path::new(&orig.filename).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let nome = nome_livre(&pasta, &base);
    let destino = pasta.join(&nome);
    let c = copiar(cap, &caminho, formato, &destino, cancelar)?;
    let sha256 = apagando(&destino, sha256_file(&destino))?;
    // os mesmos bytes já registrados (o arquivo sumiu da pasta, por exemplo): volta para o lugar dele
    if let Some(m) = video_repo::find_media_by_sha256(&conn, &occ, &sha256)? {
        let lugar = resolver_relativo(ws, &m.relative_path);
        if lugar != destino {
            if lugar.is_file() {
                let _ = std::fs::remove_file(&destino);
            } else {
                if let Some(p) = lugar.parent() {
                    std::fs::create_dir_all(p)?;
                }
                apagando(&destino, std::fs::rename(&destino, &lugar).map_err(SicroError::from))?;
            }
        }
        let (lacunas, inicio_epoca_s) = de_derivacao(&m);
        return Ok(CopiaRegistrada { media: m, formato, lacunas, inicio_epoca_s, ja_existia: true });
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
        occurrence_id: occ,
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
    let registrar = || -> Result<()> {
        let tx = conn.unchecked_transaction()?;
        video_repo::insert_media(&tx, &media)?;
        let detalhe = json!({ "media_id": media.id.to_string(), "sha256": media.sha256, "filename": media.filename, "derivation": derivacao });
        video_repo::insert_log(&tx, &occ, Some(&orig.sha256), "dvr.copia", &detalhe.to_string())?;
        occurrence_repo::record_audit(&tx, Some(&occ), "video.dvr.copia", Some("video"), Some("video_media"), Some(&media.id), Some(&media.sha256))?;
        Ok(tx.commit()?)
    };
    apagando(&destino, registrar())?;
    Ok(CopiaRegistrada { media, formato, lacunas: c.lacunas, inicio_epoca_s: c.inicio_epoca_s, ja_existia: false })
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
            }
        }
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

    // ------------------------------------------------------------ registro no caso

    /// Caso de verdade (acento e espaço no caminho) com o original registrado pelo comando da importação.
    fn caso_com(origem: &Path) -> (tempfile::TempDir, PathBuf, VideoMedia) {
        let d = tempfile::Builder::new().prefix("p4.6-dvr-").tempdir().unwrap();
        let pai = d.path().join("Casos ç");
        std::fs::create_dir_all(&pai).unwrap();
        let ws = crate::workspace::create::create_workspace(crate::models::NewOccurrenceInput::default(), &pai).unwrap().path;
        let input = crate::models::RegisterVideoInput { source_path: origem.to_string_lossy().into(), allow_duplicate: false };
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
