//! Tabela de quadros (§5.2): a de pacotes (rápida, serve à tela) e a do decodificador
//! (a que vale para o render), sonda de busca, contagem de erros e cache. Dono: P0.3b.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::cor;
use super::ffmpeg::exec::{self, Comando};
use super::ffmpeg::sonda::{self, Capacidades};
use super::geometria;
use super::modelo::{CorMidia, Faixa, Matriz, Racional, TamanhoPx};
use super::motor::{AudioInfo, FonteInfo};
use super::tempo;
use crate::error::{Result, SicroError};

/// Margem do `-ss` antes do quadro-chave (s) e distância mínima do alvo ao início da mídia (s): a regra do §5.3.
// CONTRATO: acréscimo. A sonda de busca e o motor usam os mesmos números.
pub const BUSCA_MARGEM_S: f64 = 0.5;
pub const BUSCA_GUARDA_S: f64 = 10.0;
/// Trechos da sonda de busca e quadros comparados em cada um.
const SONDAS: usize = 6;
const QUADROS_POR_SONDA: usize = 8;
/// Taxas padrão de `taxa_nominal`, da menor para a maior.
const TAXAS: &[(i64, i64)] =
    &[(10, 1), (12, 1), (25, 2), (15, 1), (20, 1), (24000, 1001), (24, 1), (25, 1), (30000, 1001), (30, 1), (50, 1), (60000, 1001), (60, 1)];
/// Marca do arquivo de cache `quadros/<sha256>.<ffmpeg>.q2`.
const FORMATO: &str = "sicro.quadros/2";

/// De onde veio a tabela: pacotes (ffprobe) ou decodificador (a que vale para o render).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrigemTabela {
    Pacotes,
    Decodificador,
}

/// `SemBusca`: o motor nunca usa `-ss` nessa mídia.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Busca {
    Confiavel,
    SemBusca,
}

/// O que o contêiner declara (vai para o relatório ao lado do medido).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Declarado {
    pub r_frame_rate: String,
    pub avg_frame_rate: String,
    pub nb_frames: Option<u64>,
    /// Segundos.
    pub duracao: Option<f64>,
}

/// Quadros do vídeo em ordem de exibição. `pts` e passos na base `tb`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabelaQuadros {
    pub tb: Racional,
    pub pts: Vec<i64>,
    pub chave: Vec<bool>,
    pub passo_mediano: i64,
    /// (mínimo, máximo).
    pub passo: (i64, i64),
    pub vfr: bool,
    pub origem: OrigemTabela,
    pub ffmpeg_sha256: String,
    pub erros_decodificacao: u32,
    pub busca: Busca,
    pub declarado: Declarado,
    pub pacotes: u64,
    /// Saltos anormais de pts: (pts antes, pts depois).
    pub lacunas: Vec<(i64, i64)>,
}

/// Tabela compacta para o front (`edicao_tabela_quadros`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabelaCompacta {
    pub tb: Racional,
    /// pts do quadro 0.
    pub pts0: i64,
    /// passos[i] = pts[i+1] − pts[i], em ticks de `tb` (n − 1 itens).
    pub passos: Vec<u32>,
    /// `TabelaQuadros.passo_mediano` (ticks): duração do último quadro; vai pronto para o espelho não divergir na mediana.
    // CONTRATO: não está no §5.12; o fim de trecho no fim da mídia e o meio do último quadro dependem dele.
    pub passo_mediano: i64,
    /// Índices dos quadros-chave.
    pub chaves: Vec<u32>,
    pub vfr: bool,
    pub origem: OrigemTabela,
}

/// Alvo de `edicao_quadro_em`: `{ t }` (segundos de fonte), `{ indice }` ou `{ pts }`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum AlvoQuadro {
    T { t: f64 },
    Indice { indice: u32 },
    Pts { pts: i64 },
}

/// Quadro achado: índice na tabela, pts, tempo de fonte (s) e duração do quadro (s).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct QuadroEm {
    pub indice: u32,
    pub pts: i64,
    pub pts_s: f64,
    pub dur_s: f64,
}

fn erro(texto: impl Into<String>) -> SicroError {
    SicroError::Validation(texto.into())
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

/// "a/b" ou "a:b" → fração reduzida; None se faltar parte ou algum lado não for positivo.
fn fracao(texto: &str) -> Option<Racional> {
    let (a, b) = texto.split_once(['/', ':'])?;
    let (a, b): (i64, i64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    (a > 0 && b > 0).then(|| Racional { num: a / mdc(a, b), den: b / mdc(a, b) })
}

/// Pasta de trabalho de uma passada (stderr dos processos), apagada ao sair.
struct Trabalho(PathBuf);

impl Trabalho {
    fn nova() -> Result<Self> {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!("sicro-quadros-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
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

// ---------------------------------------------------------------- ffprobe

/// Fluxo de vídeo como o ffprobe mostra.
struct FluxoVideo {
    codec: String,
    w: u32,
    h: u32,
    pix_fmt: String,
    sar: Racional,
    /// Graus no sentido horário.
    giro: i32,
    entrelacado: bool,
    faixa_etiqueta: String,
    matriz_etiqueta: String,
    tb: Option<Racional>,
    r_frame_rate: String,
    avg_frame_rate: String,
    nb_frames: Option<u64>,
    duracao: Option<f64>,
}

/// O que uma chamada do ffprobe diz do arquivo. `video` = `v:0` quando não é capa de áudio.
struct Info {
    conteiner: String,
    inicio: Option<f64>,
    duracao: Option<f64>,
    video: Option<FluxoVideo>,
    audio: Option<AudioInfo>,
    imagem: bool,
}

fn numero(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).filter(|x: &f64| x.is_finite())
}

fn sondar(cap: &Capacidades, video: &Path) -> Result<Info> {
    let t = Trabalho::nova()?;
    let cmd = t.comando(
        "ffprobe-info",
        &cap.ffprobe,
        textos(&["-v", "error", "-show_format", "-show_streams", "-of", "json", &video.to_string_lossy()]),
    );
    let (fim, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false))?;
    exec::exigir_sucesso(&cmd, &fim)?;
    let j: Value = serde_json::from_slice(&out)?;
    let texto = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let conteiner = texto(&j["format"]["format_name"]);
    let fluxos = j["streams"].as_array().cloned().unwrap_or_default();
    let do_tipo = |tipo: &str| fluxos.iter().find(|s| s["codec_type"] == tipo);
    let video = do_tipo("video").filter(|s| numero(&s["disposition"]["attached_pic"]).unwrap_or(0.0) == 0.0).map(|s| {
        // `rotation` (matriz de exibição) é anti-horário; a etiqueta `rotate` já é horária
        let giro = s["side_data_list"]
            .as_array()
            .and_then(|l| l.iter().find_map(|d| numero(&d["rotation"])))
            .map(|r| -r)
            .or_else(|| numero(&s["tags"]["rotate"]))
            .unwrap_or(0.0);
        FluxoVideo {
            codec: texto(&s["codec_name"]),
            w: numero(&s["width"]).unwrap_or(0.0) as u32,
            h: numero(&s["height"]).unwrap_or(0.0) as u32,
            pix_fmt: texto(&s["pix_fmt"]),
            sar: s["sample_aspect_ratio"].as_str().and_then(fracao).unwrap_or(Racional { num: 1, den: 1 }),
            giro: (giro.round() as i64).rem_euclid(360) as i32,
            entrelacado: matches!(s["field_order"].as_str(), Some("tt" | "bb" | "tb" | "bt")),
            faixa_etiqueta: texto(&s["color_range"]),
            matriz_etiqueta: texto(&s["color_space"]),
            tb: s["time_base"].as_str().and_then(fracao),
            r_frame_rate: texto(&s["r_frame_rate"]),
            avg_frame_rate: texto(&s["avg_frame_rate"]),
            nb_frames: numero(&s["nb_frames"]).map(|n| n as u64),
            duracao: numero(&s["duration"]),
        }
    });
    let inicio = numero(&j["format"]["start_time"]);
    let audio = do_tipo("audio").map(|s| AudioInfo {
        inicio_s: numero(&s["start_time"]).or(inicio).unwrap_or(0.0),
        taxa: numero(&s["sample_rate"]).unwrap_or(0.0) as u32,
        canais: numero(&s["channels"]).unwrap_or(0.0) as u32,
    });
    let imagem = video.is_some() && (conteiner == "image2" || conteiner.ends_with("_pipe"));
    Ok(Info { conteiner, inicio, duracao: numero(&j["format"]["duration"]), video, audio, imagem })
}

/// Um pacote de vídeo: tempo (pts, ou dts quando ele falta) e se é quadro-chave. Sem os de descarte.
struct Pacotes {
    total: u64,
    tempos: Vec<(Option<i64>, bool)>,
}

fn ler_pacotes(cap: &Capacidades, video: &Path, cancelar: &AtomicBool) -> Result<Pacotes> {
    let t = Trabalho::nova()?;
    let cmd = t.comando(
        "ffprobe-pacotes",
        &cap.ffprobe,
        textos(&["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts,dts,flags", "-of", "csv=p=0", &video.to_string_lossy()]),
    );
    let (fim, out) = exec::rodar_com_saida(&cmd, cancelar)?;
    exec::exigir_sucesso(&cmd, &fim)?;
    let mut p = Pacotes { total: 0, tempos: Vec::new() };
    for linha in String::from_utf8_lossy(&out).lines() {
        // pts,dts,flags (pode vir mais coisa depois)
        let mut c = linha.split(',');
        let (Some(pts), Some(dts), Some(flags)) = (c.next(), c.next(), c.next()) else { continue };
        p.total += 1;
        if flags.contains('D') {
            continue;
        }
        let tempo = pts.trim().parse::<i64>().ok().or_else(|| dts.trim().parse::<i64>().ok());
        p.tempos.push((tempo, flags.contains('K')));
    }
    Ok(p)
}

/// Passo de um quadro em ticks de `tb` pela taxa declarada ("30000/1001"); None se não der.
fn passo_declarado(tb: Racional, taxa: &str) -> Option<i64> {
    let r = fracao(taxa)?;
    let p = (tb.den as i128 * r.den as i128 * 2 + tb.num as i128 * r.num as i128) / (2 * tb.num as i128 * r.num as i128);
    i64::try_from(p).ok().filter(|p| *p > 0)
}

/// Tempos em ordem de exibição. Pacote sem tempo nenhum (MPEG-PS, fluxo cru) ganha um por interpolação
/// entre os vizinhos, na ordem do arquivo: vale só para a tela, até chegar a tabela do decodificador.
fn tempos_dos_pacotes(mut t: Vec<(Option<i64>, bool)>, passo: i64) -> (Vec<i64>, Vec<bool>) {
    let conhecidos: Vec<usize> = t.iter().enumerate().filter(|(_, x)| x.0.is_some()).map(|(i, _)| i).collect();
    match (conhecidos.first().copied(), conhecidos.last().copied()) {
        (Some(a), Some(z)) => {
            let (ta, tz) = (t[a].0.unwrap_or(0), t[z].0.unwrap_or(0));
            for (i, x) in t.iter_mut().enumerate().take(a) {
                x.0 = Some(ta - (a - i) as i64 * passo);
            }
            for (i, x) in t.iter_mut().enumerate().skip(z + 1) {
                x.0 = Some(tz + (i - z) as i64 * passo);
            }
            for par in conhecidos.windows(2) {
                let (i, k) = (par[0], par[1]);
                let (ti, tk) = (t[i].0.unwrap_or(0) as i128, t[k].0.unwrap_or(0) as i128);
                for (m, x) in t.iter_mut().enumerate().take(k).skip(i + 1) {
                    x.0 = Some((ti + (tk - ti) * (m - i) as i128 / (k - i) as i128) as i64);
                }
            }
        }
        _ => {
            for (i, x) in t.iter_mut().enumerate() {
                x.0 = Some(i as i64 * passo);
            }
        }
    }
    let mut v: Vec<(i64, bool)> = t.into_iter().map(|(p, k)| (p.unwrap_or(0), k)).collect();
    v.sort_by_key(|x| x.0);
    v.into_iter().unzip()
}

// ---------------------------------------------------------------- tabela

/// (passo mediano, (mínimo, máximo), taxa variável, lacunas).
type Passos = (i64, (i64, i64), bool, Vec<(i64, i64)>);

/// Passo mediano, (mínimo, máximo), taxa variável e lacunas dos pts (em ordem).
/// Classes de `receitas-audio-saida.md` §8: variável = mais de 0,5 % dos passos além de ±25 % da mediana
/// (ou de 1,5 ms, o que for maior); lacuna = passo acima de 1,5× a mediana.
fn medir_passos(tb: Racional, pts: &[i64], declarado: Option<i64>) -> Passos {
    let passos: Vec<i64> = pts.windows(2).map(|p| p[1] - p[0]).collect();
    if passos.is_empty() {
        let p = declarado.unwrap_or(1).max(1);
        return (p, (p, p), false, Vec::new());
    }
    let mut ordenados = passos.clone();
    ordenados.sort_unstable();
    let mediano = ordenados[ordenados.len() / 2].max(1);
    let tol = (0.25 * mediano as f64).max(0.0015 * tb.den as f64 / tb.num as f64).max(1.0);
    let fora = passos.iter().filter(|&&d| (d - mediano).abs() as f64 > tol).count();
    let lacunas = pts.windows(2).filter(|p| (p[1] - p[0]) as f64 > 1.5 * mediano as f64).map(|p| (p[0], p[1])).collect();
    (mediano, (ordenados[0], ordenados[ordenados.len() - 1]), fora * 200 > passos.len(), lacunas)
}

/// Tabelas já lidas neste processo, por (SHA-256 da mídia, SHA-256 do ffmpeg).
type Memoria = Mutex<HashMap<(String, String), Arc<TabelaQuadros>>>;

fn memoria() -> &'static Memoria {
    static M: OnceLock<Memoria> = OnceLock::new();
    M.get_or_init(Default::default)
}

fn guardar_na_memoria(sha256: &str, tab: Arc<TabelaQuadros>) {
    memoria().lock().unwrap_or_else(|e| e.into_inner()).insert((sha256.to_string(), tab.ffmpeg_sha256.clone()), tab);
}

/// `<cache>/quadros/<sha256 da mídia>.<16 dígitos do SHA-256 do ffmpeg>.q2` (o SHA-256 inteiro vai dentro do arquivo).
fn arquivo_do_cache(cache: &Path, sha256: &str, ffmpeg_sha256: &str) -> Result<PathBuf> {
    let hex = |s: &str| !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
    if !hex(sha256) || !hex(ffmpeg_sha256) {
        return Err(erro(format!("sha256_invalido: {sha256}")));
    }
    Ok(cache.join("quadros").join(format!("{sha256}.{}.q2", &ffmpeg_sha256[..ffmpeg_sha256.len().min(16)])))
}

#[derive(Serialize, Deserialize)]
struct ArquivoQ2 {
    formato: String,
    midia: String,
    tabela: TabelaQuadros,
}

fn ler_do_cache(arquivo: &Path, sha256: &str, ffmpeg_sha256: &str) -> Option<TabelaQuadros> {
    let a: ArquivoQ2 = serde_json::from_slice(&std::fs::read(arquivo).ok()?).ok()?;
    let t = a.tabela;
    let inteira = t.pts.len() == t.chave.len() && t.pts.windows(2).all(|p| p[0] <= p[1]) && t.tb.num > 0 && t.tb.den > 0;
    (a.formato == FORMATO && a.midia == sha256 && t.ffmpeg_sha256 == ffmpeg_sha256 && t.origem == OrigemTabela::Decodificador && inteira).then_some(t)
}

impl TabelaQuadros {
    /// A melhor tabela que houver: a do decodificador (memória, depois o cache `quadros/<sha256>.<ffmpeg>.q2`);
    /// senão a de pacotes, feita na hora. Não conhece a fila: quem recebe `origem = Pacotes` pede a outra.
    pub fn obter(cache: &Path, sha256: &str, video: &Path) -> Result<Arc<Self>> {
        Self::obter_com(&*sonda::capacidades()?, cache, sha256, video)
    }

    /// `obter` com o binário já escolhido.
    pub(crate) fn obter_com(cap: &Capacidades, cache: &Path, sha256: &str, video: &Path) -> Result<Arc<Self>> {
        let arquivo = arquivo_do_cache(cache, sha256, &cap.sha256)?;
        let chave = (sha256.to_string(), cap.sha256.clone());
        let da_memoria = memoria().lock().unwrap_or_else(|e| e.into_inner()).get(&chave).cloned();
        if let Some(t) = da_memoria.as_ref().filter(|t| t.origem == OrigemTabela::Decodificador) {
            return Ok(t.clone());
        }
        if arquivo.is_file() {
            match ler_do_cache(&arquivo, sha256, &cap.sha256) {
                Some(t) => {
                    let t = Arc::new(t);
                    guardar_na_memoria(sha256, t.clone());
                    return Ok(t);
                }
                // arquivo de outro formato ou estragado: refaz
                None => drop(std::fs::remove_file(&arquivo)),
            }
        }
        if let Some(t) = da_memoria {
            return Ok(t);
        }
        let t = Arc::new(Self::de_pacotes(cap, video)?);
        guardar_na_memoria(sha256, t.clone());
        Ok(t)
    }

    /// Grava a tabela do decodificador no cache (arquivo novo + troca de nome) e a deixa na memória:
    /// o próximo `obter` devolve esta.
    // CONTRATO: acréscimo. `do_decodificador` não recebe o cache; quem o roda (fila de fundo, P0.7) guarda por aqui.
    pub fn guardar(self, cache: &Path, sha256: &str) -> Result<Arc<Self>> {
        if self.origem != OrigemTabela::Decodificador {
            return Err(erro("tabela_pendente"));
        }
        let arquivo = arquivo_do_cache(cache, sha256, &self.ffmpeg_sha256)?;
        if let Some(p) = arquivo.parent() {
            std::fs::create_dir_all(p)?;
        }
        let a = ArquivoQ2 { formato: FORMATO.into(), midia: sha256.into(), tabela: self };
        static N: AtomicU64 = AtomicU64::new(0);
        let tmp = arquivo.with_extension(format!("q2.{}-{}.tmp", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::fs::write(&tmp, serde_json::to_vec(&a)?)?;
        if let Err(e) = std::fs::rename(&tmp, &arquivo) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.into());
        }
        let t = Arc::new(a.tabela);
        guardar_na_memoria(sha256, t.clone());
        Ok(t)
    }

    /// Tabela pronta a partir dos pts em ordem (mede passos, taxa variável e lacunas).
    fn montar(cap: &Capacidades, tb: Racional, pts: Vec<i64>, chave: Vec<bool>, origem: OrigemTabela, declarado: Declarado, pacotes: u64) -> Self {
        let (passo_mediano, passo, vfr, lacunas) = medir_passos(tb, &pts, passo_declarado(tb, &declarado.r_frame_rate));
        TabelaQuadros {
            tb,
            pts,
            chave,
            passo_mediano,
            passo,
            vfr,
            origem,
            ffmpeg_sha256: cap.sha256.clone(),
            erros_decodificacao: 0,
            busca: Busca::SemBusca,
            declarado,
            pacotes,
            lacunas,
        }
    }

    /// Imagem (1 quadro, pts 0, base 1/1) ou arquivo sem vídeo (vazia): exatas por definição.
    fn sem_video(cap: &Capacidades, info: &Info) -> Self {
        let n = usize::from(info.imagem);
        let declarado = Declarado { r_frame_rate: String::new(), avg_frame_rate: String::new(), nb_frames: None, duracao: info.duracao };
        Self::montar(cap, Racional { num: 1, den: 1 }, vec![0; n], vec![true; n], OrigemTabela::Decodificador, declarado, n as u64)
    }

    fn declarado(info: &Info, v: &FluxoVideo) -> Declarado {
        Declarado {
            r_frame_rate: v.r_frame_rate.clone(),
            avg_frame_rate: v.avg_frame_rate.clone(),
            nb_frames: v.nb_frames,
            duracao: v.duracao.or(info.duracao),
        }
    }

    /// Tabela de pacotes pelo ffprobe (dts quando falta pts; sem os pacotes de descarte). `busca` = `SemBusca`
    /// (não sondada). Imagem e arquivo sem vídeo: a tabela exata, com `origem = Decodificador`.
    pub fn de_pacotes(cap: &Capacidades, video: &Path) -> Result<Self> {
        let info = sondar(cap, video)?;
        let Some(v) = info.video.as_ref().filter(|_| !info.imagem) else { return Ok(Self::sem_video(cap, &info)) };
        let tb = v.tb.ok_or_else(|| erro("pts_invalido: sem base de tempo"))?;
        let p = ler_pacotes(cap, video, &AtomicBool::new(false))?;
        let passo = passo_declarado(tb, &v.r_frame_rate).unwrap_or(1);
        let (pts, chave) = tempos_dos_pacotes(p.tempos, passo);
        Ok(Self::montar(cap, tb, pts, chave, OrigemTabela::Pacotes, Self::declarado(&info, v), p.total))
    }

    /// Tabela do decodificador (`showinfo`), com erros de decodificação e sonda de busca.
    /// `progresso` recebe 0..1. Trabalho da fila de fundo.
    pub fn do_decodificador(
        cap: &Capacidades,
        video: &Path,
        cancelar: &AtomicBool,
        progresso: &mut dyn FnMut(f32),
    ) -> Result<Self> {
        let info = sondar(cap, video)?;
        let Some(v) = info.video.as_ref().filter(|_| !info.imagem) else {
            progresso(1.0);
            return Ok(Self::sem_video(cap, &info));
        };
        let pacotes = ler_pacotes(cap, video, cancelar)?.total;
        let trabalho = Trabalho::nova()?;
        let caminho = video.to_string_lossy().into_owned();
        // `level+info`: cada linha traz o nível, e as de erro são contadas
        let cmd = trabalho.comando(
            "quadros",
            &cap.binario,
            textos(&[
                "-hide_banner", "-nostdin", "-nostats", "-loglevel", "level+info", "-copyts", "-i", &caminho, "-map", "0:v:0", "-vf",
                "showinfo@tq=checksum=0", "-f", "null", "-",
            ]),
        );
        let total = pacotes.max(1) as f32;
        let fim = exec::rodar_com_progresso(&cmd, cancelar, &mut |p| progresso((p.quadro as f32 / total).min(1.0) * 0.9))?;
        let lido = ler_showinfo(&cmd.stderr, "tq")?;
        // 69 = o ffmpeg desistiu por excesso de erros de decodificação: a tabela vale até onde ele foi
        if fim.cancelado || !(fim.codigo == Some(0) || (fim.codigo == Some(69) && !lido.pts.is_empty())) {
            exec::exigir_sucesso(&cmd, &fim)?;
        }
        let tb = lido.tb.or(v.tb).ok_or_else(|| erro("pts_invalido: sem base de tempo"))?;
        if let Some(j) = lido.pts.iter().position(|p| *p == i64::MIN) {
            return Err(erro(format!("pts_invalido: quadro {j} sem pts")));
        }
        if let Some(j) = lido.pts.windows(2).position(|p| p[1] < p[0]) {
            return Err(erro(format!("pts_invalido: fora de ordem no quadro {}", j + 1)));
        }
        let mut tab = Self::montar(cap, tb, lido.pts, lido.chave, OrigemTabela::Decodificador, Self::declarado(&info, v), pacotes);
        tab.erros_decodificacao = lido.erros;
        // sem `start_time` o ffmpeg não desloca o `-ss`: o mesmo 0 de `fonte_info`
        let inicio = info.inicio.unwrap_or(0.0);
        tab.busca = sondar_busca(cap, &trabalho, &caminho, &tab, inicio, cancelar, &mut |f| progresso(0.9 + 0.1 * f))?;
        progresso(1.0);
        Ok(tab)
    }

    /// Índice do quadro exibido no tempo de fonte `t` (s): maior j com pts_j·tb ≤ t + tb/2.
    pub fn exibido(&self, t: f64) -> usize {
        // em ticks: maior j com pts_j ≤ chão(t/tb + 1/2)
        let alvo = (t * self.tb.den as f64 / self.tb.num as f64 + 0.5).floor();
        self.pts.partition_point(|&p| p as f64 <= alvo).saturating_sub(1)
    }

    /// Índice do quadro com esse pts exato (o primeiro, se o pts se repete).
    pub fn indice_do_pts(&self, pts: i64) -> Option<usize> {
        let j = self.pts.partition_point(|&p| p < pts);
        (self.pts.get(j) == Some(&pts)).then_some(j)
    }

    /// Índice do último quadro-chave ≤ j.
    pub fn chave_ate(&self, j: usize) -> Option<usize> {
        let ate = j.min(self.chave.len().checked_sub(1)?);
        (0..=ate).rev().find(|&i| self.chave[i])
    }

    /// Menor taxa padrão em que o arquivo inteiro a 1× não descarta quadro, até 60. Entre X·1000/1001 e X vale
    /// a que deixa os quadros mais perto dos slots: um vídeo curto de 30 qps cabe em 29,97 sem descarte, só que
    /// mais lento. Quadro de pts repetido não conta (nenhuma taxa o mostra). Menos de 2 quadros: 30.
    pub fn taxa_nominal(&self) -> Racional {
        if self.pts.len() < 2 {
            return Racional { num: 30, den: 1 };
        }
        let repetidos = self.pts.windows(2).filter(|p| p[0] == p[1]).count() as u64;
        let cabe = |num: i64, den: i64| tempo::descartados_a_1x(self, Racional { num, den }).is_ok_and(|d| d == repetidos);
        let Some(&(num, den)) = TAXAS.iter().find(|&&(num, den)| cabe(num, den)) else { return Racional { num: 60, den: 1 } };
        if den == 1001 && TAXAS.contains(&(num / 1000, 1)) && cabe(num / 1000, 1) {
            // desvios em frações de denominadores diferentes: compara em cruz
            let a = tempo::desvio_a_1x(self, Racional { num, den });
            let b = tempo::desvio_a_1x(self, Racional { num: num / 1000, den: 1 });
            if let (Ok((an, ad)), Ok((bn, bd))) = (a, b) {
                if bn * ad < an * bd {
                    return Racional { num: num / 1000, den: 1 };
                }
            }
        }
        Racional { num, den }
    }

    /// Tempo de fonte (s) do quadro j: pts_j·tb.
    pub fn tempo(&self, j: usize) -> f64 {
        self.pts[j] as f64 * self.tb.num as f64 / self.tb.den as f64
    }

    /// Duração (s) do quadro j; no último, o passo mediano.
    pub fn duracao(&self, j: usize) -> f64 {
        let passo = if j + 1 < self.pts.len() { self.pts[j + 1] - self.pts[j] } else { self.passo_mediano };
        passo as f64 * self.tb.num as f64 / self.tb.den as f64
    }

    /// Tempo de fonte (s) do fim da mídia: fim do último quadro. Tabela vazia: 0.
    // CONTRATO: acréscimo.
    pub fn fim_s(&self) -> f64 {
        match self.pts.last() {
            Some(&p) => (p + self.passo_mediano) as f64 * self.tb.num as f64 / self.tb.den as f64,
            None => 0.0,
        }
    }

    /// Quadro de um tempo preso (segundos, pts): o do pts quando ele está na tabela (é a identidade, D4);
    /// senão o exibido em `t`. Com pts repetido, o primeiro quadro desse pts (o `trim` por pts pega todos).
    // CONTRATO: acréscimo.
    pub fn do_par(&self, t: f64, pts: Option<i64>) -> usize {
        pts.and_then(|p| self.indice_do_pts(p)).unwrap_or_else(|| self.primeiro_do_pts(self.exibido(t)))
    }

    /// Primeiro quadro com o pts do quadro j.
    fn primeiro_do_pts(&self, j: usize) -> usize {
        self.pts.get(j).and_then(|&p| self.indice_do_pts(p)).unwrap_or(j)
    }

    /// Fim de trecho (índice exclusivo) de um par (segundos, pts): `n` quando `t` alcança o fim da mídia.
    // CONTRATO: acréscimo.
    pub fn fim_do_par(&self, t: f64, pts: Option<i64>) -> usize {
        if let Some(j) = pts.and_then(|p| self.indice_do_pts(p)) {
            return j;
        }
        let n = self.pts.len();
        let alvo = (t * self.tb.den as f64 / self.tb.num as f64 + 0.5).floor();
        match self.pts.last() {
            Some(&p) if alvo >= (p + self.passo_mediano) as f64 => n,
            _ => self.primeiro_do_pts(self.exibido(t)),
        }
    }

    /// Alvo do `-ss` (s, relativo ao início do arquivo) de uma leitura que começa no quadro `j_in`, pela regra
    /// do §5.3: tempo do último quadro-chave ≤ `j_in` menos 0,5 s. None = sem busca (mídia `SemBusca`, sem
    /// quadro-chave conhecido ou alvo a menos de 10 s do início). `inicio_s` = `FonteInfo.inicio_s`.
    // CONTRATO: acréscimo. É a mesma conta da sonda de busca; `motor::clipe::busca` deve chamar esta.
    pub fn busca_em(&self, inicio_s: f64, j_in: usize) -> Option<f64> {
        if self.busca == Busca::SemBusca {
            return None;
        }
        self.alvo_da_busca(inicio_s, j_in)
    }

    fn alvo_da_busca(&self, inicio_s: f64, j_in: usize) -> Option<f64> {
        let alvo = self.tempo(self.chave_ate(j_in)?) - inicio_s - BUSCA_MARGEM_S;
        (alvo >= BUSCA_GUARDA_S).then_some(alvo)
    }

    /// Resolve o alvo para um quadro da tabela. `{ pts }` que a tabela não tem: o quadro exibido nesse pts.
    /// Erro `intervalo`: tabela vazia ou índice fora dela.
    pub fn quadro_em(&self, alvo: AlvoQuadro) -> Result<QuadroEm> {
        let n = self.pts.len();
        let j = match alvo {
            _ if n == 0 => return Err(erro("intervalo")),
            AlvoQuadro::T { t } => self.exibido(t),
            AlvoQuadro::Indice { indice } if (indice as usize) < n => indice as usize,
            AlvoQuadro::Indice { .. } => return Err(erro("intervalo")),
            AlvoQuadro::Pts { pts } => self.indice_do_pts(pts).unwrap_or_else(|| self.pts.partition_point(|&p| p <= pts).saturating_sub(1)),
        };
        Ok(QuadroEm { indice: j as u32, pts: self.pts[j], pts_s: self.tempo(j), dur_s: self.duracao(j) })
    }

    /// Forma compacta para o front. Erro `tabela_grande`: passo que não cabe em 32 bits, ou pts fora de ±(2^53 − 1),
    /// que o número do JSON não leva inteiro ao front.
    // CONTRATO: devolve `Result` (resposta (e) 8 do orquestrador: lacuna acima de 2³² ticks é erro, nunca valor truncado).
    pub fn compacta(&self) -> Result<TabelaCompacta> {
        if self.pts.first().into_iter().chain(self.pts.last()).any(|p| p.unsigned_abs() >= 1 << 53) {
            return Err(erro("tabela_grande"));
        }
        let passos = self.pts.windows(2).map(|p| u32::try_from(p[1] - p[0])).collect::<std::result::Result<Vec<u32>, _>>();
        Ok(TabelaCompacta {
            tb: self.tb,
            pts0: self.pts.first().copied().unwrap_or(0),
            passos: passos.map_err(|_| erro("tabela_grande"))?,
            passo_mediano: self.passo_mediano,
            chaves: self.chave.iter().enumerate().filter(|(_, k)| **k).map(|(i, _)| i as u32).collect(),
            vfr: self.vfr,
            origem: self.origem,
        })
    }
}

/// O que o `showinfo@<rótulo>` e o decodificador deixaram no stderr.
struct Lido {
    tb: Option<Racional>,
    /// `i64::MIN` = quadro sem pts.
    pts: Vec<i64>,
    chave: Vec<bool>,
    /// Linhas de nível erro (ou pior) e avisos de pacote ou quadro corrompido, depois de aberta a entrada e fora
    /// do lado da saída: as da sondagem do contêiner não são do decodificador (TS cortado no meio do GOP dá
    /// dezenas e nenhum quadro reconstruído).
    erros: u32,
    /// Quadros já lidos quando apareceu a primeira dessas linhas.
    primeiro_erro: Option<usize>,
}

/// Contextos de log do lado da saída (muxer nulo, codificador, filtros): erro deles não é da mídia.
const SAIDA: &[&str] = &["[null @", "[out#", "[vost#", "[enc", "[mux", "[fc#", "[Parsed_", "[showinfo@"];
/// Avisos do ffmpeg para pacote e quadro corrompidos do fluxo mapeado: saem em nível aviso, sem linha de erro.
/// TS sem 40 pacotes no meio: 15 quadros a menos e só estes dois avisos; LVF truncado: o último quadro sai
/// reconstruído e muda de uma execução para outra.
const CORROMPIDO: &[&str] = &["corrupt input packet", "corrupt decoded frame"];
const NIVEIS: &[&str] = &["panic", "fatal", "error", "warning", "info", "verbose", "debug", "trace"];

/// Nível e mensagem de uma linha de `-loglevel level+info`: o nível vem depois dos colchetes de contexto
/// (`[h264 @ 0x55] [error] …`). Metadado do arquivo impresso no log (`[info]     title : [error] …`) fica na mensagem.
fn nivel(linha: &str) -> Option<(&str, &str)> {
    let mut resto = linha;
    while let Some(dentro) = resto.strip_prefix('[') {
        let (grupo, depois) = dentro.split_once(']')?;
        resto = depois.trim_start();
        if NIVEIS.contains(&grupo) {
            return Some((grupo, resto));
        }
    }
    None
}

fn ler_showinfo(stderr: &Path, rotulo: &str) -> Result<Lido> {
    let marca = format!("[showinfo@{rotulo} @ ");
    let mut l = Lido { tb: None, pts: Vec::new(), chave: Vec::new(), erros: 0, primeiro_erro: None };
    let mut aberta = false;
    let mut f = BufReader::new(std::fs::File::open(stderr)?);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if f.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        let linha = String::from_utf8_lossy(&buf);
        // só a linha que COMEÇA pelo rótulo: um metadado do arquivo com o mesmo texto não vira quadro
        let Some(depois) = linha.strip_prefix(marca.as_str()) else {
            aberta |= linha.contains("Input #0");
            let da_saida = SAIDA.iter().any(|c| linha.starts_with(c));
            let erro = match nivel(&linha) {
                Some(("error" | "fatal" | "panic", _)) => true,
                Some(("warning", msg)) => CORROMPIDO.iter().any(|c| msg.starts_with(c)),
                _ => false,
            };
            if aberta && !da_saida && erro {
                l.erros = l.erros.saturating_add(1);
                l.primeiro_erro = l.primeiro_erro.or(Some(l.pts.len()));
            }
            continue;
        };
        let Some(resto) = depois.split_once("] ").map(|x| x.1.trim_start()) else { continue };
        let resto = resto.strip_prefix("[info]").unwrap_or(resto).trim_start();
        if let Some(tb) = resto.strip_prefix("config in time_base:") {
            l.tb = l.tb.or_else(|| fracao(tb.split(',').next().unwrap_or_default()));
        } else if resto.starts_with("n:") {
            let Some(pts) = resto.split_once(" pts:").and_then(|x| x.1.split_whitespace().next()) else { continue };
            l.pts.push(if pts == "NOPTS" { i64::MIN } else { pts.parse().map_err(|_| erro(format!("showinfo: {}", linha.trim_end())))? });
            l.chave.push(resto.contains("iskey:1"));
        }
    }
    Ok(l)
}

/// Sonda de busca (§5.2): até 6 trechos lidos com o `-ss` da regra do §5.3; os pts que chegam têm de ser os da
/// tabela. Metade dos trechos começa num quadro-chave e metade 2 quadros antes do quadro-chave seguinte.
/// `SemBusca`: algum trecho divergiu, ou a regra não permite busca em ponto nenhum da mídia (nada sondado).
fn sondar_busca(
    cap: &Capacidades,
    trabalho: &Trabalho,
    caminho: &str,
    tab: &TabelaQuadros,
    inicio_s: f64,
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(f32),
) -> Result<Busca> {
    let n = tab.pts.len();
    let chaves: Vec<usize> = (0..n).filter(|&j| tab.chave[j] && tab.alvo_da_busca(inicio_s, j).is_some()).collect();
    let na_chave: Vec<usize> = chaves.clone();
    let antes_da_seguinte: Vec<usize> = (0..n)
        .filter(|&j| tab.chave[j])
        .filter_map(|k| k.checked_sub(2))
        .filter(|&j| !tab.chave[j] && !tab.chave[j + 1] && tab.alvo_da_busca(inicio_s, j).is_some())
        .collect();
    // espalhados pela mídia
    let escolher = |v: &[usize], quantos: usize| -> Vec<usize> {
        let quantos = quantos.min(v.len());
        (0..quantos).map(|i| v[if quantos > 1 { i * (v.len() - 1) / (quantos - 1) } else { v.len() / 2 }]).collect()
    };
    let mut trechos = escolher(&antes_da_seguinte, SONDAS / 2);
    trechos.extend(escolher(&na_chave, SONDAS - trechos.len()));
    trechos.sort_unstable();
    trechos.dedup();
    if trechos.is_empty() {
        return Ok(Busca::SemBusca);
    }
    for (i, &j_in) in trechos.iter().enumerate() {
        let Some(alvo) = tab.alvo_da_busca(inicio_s, j_in) else { return Ok(Busca::SemBusca) };
        let j_out = (j_in + QUADROS_POR_SONDA).min(n);
        let fim_s = if j_out < n { tab.tempo(j_out) } else { tab.fim_s() } - inicio_s;
        let corte = match tab.pts.get(j_out) {
            Some(p) => format!("trim=start_pts={}:end_pts={p}", tab.pts[j_in]),
            None => format!("trim=start_pts={}", tab.pts[j_in]),
        };
        let nome = format!("busca{i}");
        let cmd = trabalho.comando(
            &nome,
            &cap.binario,
            textos(&[
                "-hide_banner", "-nostdin", "-nostats", "-loglevel", "level+info", "-copyts", "-ss", &format!("{alvo:.6}"), "-t",
                &format!("{:.6}", fim_s - alvo + 1.0), "-i", caminho, "-map", "0:v:0", "-vf", &format!("{corte},showinfo@sb=checksum=0"), "-f",
                "null", "-",
            ]),
        );
        let fim = exec::rodar(&cmd, cancelar)?;
        if fim.cancelado {
            exec::exigir_sucesso(&cmd, &fim)?;
        }
        if fim.codigo != Some(0) || ler_showinfo(&cmd.stderr, "sb")?.pts != tab.pts[j_in..j_out] {
            return Ok(Busca::SemBusca);
        }
        progresso((i + 1) as f32 / trechos.len() as f32);
    }
    Ok(Busca::Confiavel)
}

/// A ÚNICA leitura `ffprobe` → `FonteInfo` (o `preparar`, a prévia e os testes usam esta), juntada à tabela já obtida.
/// `exibido` = `geometria::exibido` (a regra de `decode.rs::display_size`); `giro` em graus no sentido horário
/// (o `rotation` do ffprobe vem com o sinal contrário); `inicio_s` = `start_time` do contêiner como está (pode ser negativo);
/// `matriz` e `faixa`: `cor` do `MidiaRef` quando houver, senão a etiqueta do fluxo, senão a presunção
/// (`cor::matriz_presumida` pela altura codificada, faixa limitada; `yuvj*` = cheia). Imagem e arquivo só de áudio: ver `FonteInfo`.
// CONTRATO: acréscimo do integrador depois da leva 2 (dono: P0.3b).
pub fn fonte_info(cap: &Capacidades, video: &Path, sha256: &str, quadros: Arc<TabelaQuadros>, cor: Option<&CorMidia>) -> Result<FonteInfo> {
    let info = sondar(cap, video)?;
    let v = info.video.as_ref();
    let um = Racional { num: 1, den: 1 };
    let exibido = v.map_or((0, 0), |v| {
        let e = geometria::exibido(TamanhoPx { w: v.w, h: v.h }, v.sar, v.giro);
        (e.w, e.h)
    });
    let matriz = cor.map(|c| c.matriz).unwrap_or_else(|| match v.map(|v| v.matriz_etiqueta.as_str()) {
        Some("bt709") => Matriz::Bt709,
        Some("smpte170m" | "bt470bg") => Matriz::Bt601,
        _ => cor::matriz_presumida(v.map_or(0, |v| v.h)),
    });
    let cheia = v.is_some_and(|v| v.faixa_etiqueta == "pc" || v.pix_fmt.starts_with("yuvj"));
    let faixa = cor.map(|c| c.faixa).unwrap_or(if cheia { Faixa::Pc } else { Faixa::Tv });
    // imagem e arquivo sem vídeo têm tabela própria, qualquer que seja a recebida
    let quadros = if v.is_none() || info.imagem { Arc::new(TabelaQuadros::sem_video(cap, &info)) } else { quadros };
    Ok(FonteInfo {
        caminho: video.to_path_buf(),
        sha256: sha256.to_string(),
        inicio_s: info.inicio.unwrap_or(0.0),
        exibido,
        sar: v.map_or(um, |v| v.sar),
        giro: v.map_or(0, |v| v.giro),
        entrelacado: v.is_some_and(|v| v.entrelacado),
        pix_fmt: v.map(|v| v.pix_fmt.clone()).unwrap_or_default(),
        matriz,
        faixa,
        quadros,
        audio: info.audio,
        conteiner: info.conteiner,
        codec: v.map(|v| v.codec.clone()).unwrap_or_default(),
    })
}

/// Apoio dos testes de `quadros` e de `tempo`: tabelas sintéticas, contexto sem disco e as tabelas da matriz de fontes.
#[cfg(test)]
pub(crate) mod apoio {
    use super::*;
    use crate::edicao::ffmpeg::sonda::{GrafoArquivo, Origem};
    use crate::edicao::motor::LeitorRastreios;
    use crate::edicao::rastreio::{ArquivoDenso, ArquivoSilhuetas};
    use crate::edicao::testes::fontes::{self, Fonte};

    /// Capacidades sem binário (testes que não rodam o ffmpeg).
    pub fn cap_falsa() -> Capacidades {
        Capacidades {
            binario: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            origem: Origem::Path,
            versao: "teste".into(),
            configuracao: String::new(),
            sha256: "f".repeat(64),
            filtros: Default::default(),
            codificadores: Default::default(),
            decodificadores: Default::default(),
            hwaccels: Default::default(),
            muxers: Default::default(),
            grafo_em_arquivo: GrafoArquivo::BarraFilterComplex,
            faltam: Vec::new(),
            gpu: None,
            reprovados: Vec::new(),
        }
    }

    /// Tabela "do decodificador" com esses pts; quadro-chave a cada 30.
    pub fn tabela(tb: (i64, i64), pts: Vec<i64>) -> TabelaQuadros {
        let chave = (0..pts.len()).map(|j| j % 30 == 0).collect();
        let declarado = Declarado { r_frame_rate: String::new(), avg_frame_rate: String::new(), nb_frames: None, duracao: None };
        let n = pts.len() as u64;
        let mut t = TabelaQuadros::montar(&cap_falsa(), Racional { num: tb.0, den: tb.1 }, pts, chave, OrigemTabela::Decodificador, declarado, n);
        t.busca = Busca::Confiavel;
        t
    }

    /// pts de `n` quadros de passo `passo` (s, fração) arredondados para a base `tb`, a partir de `inicio` ticks.
    pub fn pts_regulares(n: usize, passo: (i64, i64), tb: (i64, i64), inicio: i64) -> Vec<i64> {
        // round(i·passo/tb), metade para cima
        let (num, den) = (passo.0 as i128 * tb.1 as i128, passo.1 as i128 * tb.0 as i128);
        (0..n as i128).map(|i| inicio + ((2 * i * num + den) / (2 * den)) as i64).collect()
    }

    /// `FonteInfo` de uma tabela, sem arquivo.
    pub fn fonte(tab: TabelaQuadros, exibido: (u32, u32)) -> FonteInfo {
        FonteInfo {
            caminho: PathBuf::new(),
            sha256: "0".repeat(64),
            inicio_s: 0.0,
            exibido,
            sar: Racional { num: 1, den: 1 },
            giro: 0,
            entrelacado: false,
            pix_fmt: "yuv420p".into(),
            matriz: Matriz::Bt709,
            faixa: Faixa::Tv,
            quadros: Arc::new(tab),
            audio: None,
            conteiner: "mov,mp4,m4a,3gp,3g2,mj2".into(),
            codec: "h264".into(),
        }
    }

    pub struct SemRastreios;

    impl LeitorRastreios for SemRastreios {
        fn denso(&self, _: &str, _: &str) -> Result<Arc<ArquivoDenso>> {
            Err(erro("rastreio_ausente"))
        }
        fn silhuetas(&self, _: &str, _: &str) -> Result<Arc<ArquivoSilhuetas>> {
            Err(erro("rastreio_ausente"))
        }
    }

    /// Fonte da matriz pronta para medir: arquivo, `FonteInfo` com a tabela do decodificador e o número
    /// desenhado em cada quadro que o decodificador entrega (medido uma vez por processo).
    pub struct Medida {
        pub info: FonteInfo,
        pub numeros: Vec<u32>,
        /// Fonte sem número desenhado (gravador real): identidade pelo conteúdo do quadro reduzido
        /// (`REDUZIDO`); `numeros[j]` = primeiro quadro com o mesmo conteúdo.
        pub por_conteudo: Option<HashMap<u64, u32>>,
        /// Quadros `[0, limite)` comparáveis. Mídia com erro de decodificação: até o quadro-chave do GOP anterior ao
        /// do primeiro erro, porque dali em diante o decodificador não repete o que entrega (LVF truncado: some um
        /// quadro entre os 30 últimos ou o último muda, nos dois binários). Íntegra: todos.
        pub limite: usize,
    }

    /// Redução dos quadros identificados pelo conteúdo: filtro e tamanho.
    pub const REDUZIDO: (&str, usize, usize) = ("scale=160:90:flags=neighbor,format=yuv420p", 160, 90);

    pub fn conteudo(y: &[u8]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        y.hash(&mut h);
        h.finish()
    }

    /// Como `medir_arquivo`, para vídeo qualquer: decodifica inteiro pela cadeia de cor e guarda o conteúdo de cada quadro.
    pub fn medir_por_conteudo(cap: &Capacidades, video: &Path) -> Result<Medida> {
        let tab = TabelaQuadros::do_decodificador(cap, video, &AtomicBool::new(false), &mut |_| {})?;
        let info = fonte_info(cap, video, &"0".repeat(64), Arc::new(tab), None)?;
        let cor = cor::cadeia_fonte(info.matriz, info.faixa, &info.pix_fmt, info.exibido).ok_or_else(|| erro("pixel_nao_suportado"))?;
        let t = Trabalho::nova()?;
        let filtro = format!("{cor},{},extractplanes=y", REDUZIDO.0);
        let cmd = t.comando(
            "conteudo",
            &cap.binario,
            textos(&[
                "-hide_banner", "-nostdin", "-loglevel", "error", "-copyts", "-i", &video.to_string_lossy(), "-map", "0:v:0", "-vf", &filtro,
                "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "gray", "-",
            ]),
        );
        let (fim, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false))?;
        exec::exigir_sucesso(&cmd, &fim)?;
        let mut por_conteudo = HashMap::new();
        let quadros = out.as_chunks::<{ REDUZIDO.1 * REDUZIDO.2 }>().0;
        let numeros: Vec<u32> = quadros.iter().enumerate().map(|(j, y)| *por_conteudo.entry(conteudo(y)).or_insert(j as u32)).collect();
        if numeros.len() != info.quadros.pts.len() {
            return Err(erro(format!("{} quadros decodificados × {} na tabela", numeros.len(), info.quadros.pts.len())));
        }
        let limite = match info.quadros.erros_decodificacao {
            0 => numeros.len(),
            _ => {
                let cmd = t.comando(
                    "estrago",
                    &cap.binario,
                    textos(&["-hide_banner", "-nostdin", "-nostats", "-loglevel", "level+info", "-copyts", "-i", &video.to_string_lossy(), "-map", "0:v:0", "-vf", "showinfo@ee=checksum=0", "-f", "null", "-"]),
                );
                let fim = exec::rodar(&cmd, &AtomicBool::new(false))?;
                if fim.codigo != Some(0) && fim.codigo != Some(69) {
                    exec::exigir_sucesso(&cmd, &fim)?;
                }
                let tab = &info.quadros;
                let primeiro = ler_showinfo(&cmd.stderr, "ee")?.primeiro_erro.unwrap_or(0);
                tab.chave_ate(primeiro).and_then(|k| k.checked_sub(1)).and_then(|j| tab.chave_ate(j)).unwrap_or(0)
            }
        };
        Ok(Medida { info, numeros, por_conteudo: Some(por_conteudo), limite })
    }

    pub fn medida(cap: &Capacidades, f: Fonte) -> Arc<Medida> {
        static M: OnceLock<Mutex<HashMap<Fonte, Arc<Medida>>>> = OnceLock::new();
        let m = M.get_or_init(Default::default);
        if let Some(x) = m.lock().unwrap_or_else(|e| e.into_inner()).get(&f) {
            return x.clone();
        }
        let video = fontes::gerar(f).expect("fonte");
        let x = Arc::new(medir_arquivo(cap, &video));
        m.lock().unwrap_or_else(|e| e.into_inner()).entry(f).or_insert(x).clone()
    }

    pub fn medir_arquivo(cap: &Capacidades, video: &Path) -> Medida {
        let tab = TabelaQuadros::do_decodificador(cap, video, &AtomicBool::new(false), &mut |_| {}).expect("tabela do decodificador");
        let info = fonte_info(cap, video, &"0".repeat(64), Arc::new(tab), None).expect("fonte_info");
        let numeros = crate::edicao::testes::medir::quadros_de_origem(cap, video).expect("quadros de origem");
        assert_eq!(numeros.len(), info.quadros.pts.len(), "{}: quadros decodificados × tabela", video.display());
        let limite = numeros.len();
        Medida { info, numeros, por_conteudo: None, limite }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::testes::fontes::{self, Fonte, MATRIZ};
    use crate::edicao::testes::medir;
    use serde_json::json;

    fn r(num: i64, den: i64) -> Racional {
        Racional { num, den }
    }
    fn codigo<T: std::fmt::Debug>(x: Result<T>) -> String {
        match x {
            Err(SicroError::Validation(c)) => c,
            outro => panic!("esperava erro de validação: {outro:?}"),
        }
    }
    fn sem_cancelar() -> AtomicBool {
        AtomicBool::new(false)
    }

    type Par = (i64, i64);

    /// Bases de tempo dos testes sem ffmpeg: (tb, passo em s, pts inicial).
    const BASES: &[(Par, Par, i64)] = &[
        ((1, 15360), (1, 30), 0),
        ((1, 15360), (1, 60), 0),
        ((1, 12800), (1, 25), 0),
        ((1, 1000), (1, 30), 0),
        ((1, 1000), (1, 15), 0),
        ((1, 1000), (1001, 24000), 0),
        ((1, 30), (1, 30), 0),
        ((1, 25), (1, 25), 0),
        ((1, 600), (1, 15), 0),
        ((1, 30000), (1001, 30000), 0),
        ((1001, 30000), (1001, 30000), 0),
        ((1, 90000), (1001, 60000), 324_126_000),
        ((1, 90000), (2, 25), 135_518_643_900_000),
        ((1, 1000), (1, 15), 1_505_762_710_000),
    ];

    #[test]
    fn alvo_quadro_pelo_nome_do_campo() {
        assert_eq!(serde_json::from_value::<AlvoQuadro>(json!({ "t": 12.5 })).unwrap(), AlvoQuadro::T { t: 12.5 });
        assert_eq!(serde_json::from_value::<AlvoQuadro>(json!({ "indice": 312 })).unwrap(), AlvoQuadro::Indice { indice: 312 });
        assert_eq!(serde_json::from_value::<AlvoQuadro>(json!({ "pts": -90 })).unwrap(), AlvoQuadro::Pts { pts: -90 });
        assert_eq!(serde_json::to_value(AlvoQuadro::Pts { pts: 7 }).unwrap(), json!({ "pts": 7 }));
    }

    // ---------------------------------------------------------------- sem ffmpeg

    #[test]
    fn exibido_tem_folga_de_meio_tick() {
        let t = apoio::tabela((1, 1000), vec![100, 133, 167, 200]);
        // antes do primeiro vale o primeiro; meio tick antes do pts já é o quadro
        assert_eq!((t.exibido(-5.0), t.exibido(0.0), t.exibido(0.0994), t.exibido(0.0996), t.exibido(0.1)), (0, 0, 0, 0, 0));
        assert_eq!((t.exibido(0.1324), t.exibido(0.1326), t.exibido(0.133), t.exibido(0.1664), t.exibido(0.1666)), (0, 1, 1, 1, 2));
        assert_eq!((t.exibido(0.2), t.exibido(99.0), t.exibido(f64::NAN)), (3, 3, 0));
        assert_eq!(apoio::tabela((1, 1000), Vec::new()).exibido(1.0), 0);
        // pts repetido: o maior j
        let rep = apoio::tabela((1, 25), vec![0, 1, 1, 2]);
        assert_eq!((rep.exibido(0.04), rep.indice_do_pts(1), rep.indice_do_pts(3)), (2, Some(1), None));
    }

    #[test]
    fn pts_gravado_e_relido_acha_o_mesmo_quadro() {
        let mut conferidos = 0;
        for &(tb, passo, inicio) in BASES {
            let t = apoio::tabela(tb, apoio::pts_regulares(3000, passo, tb, inicio));
            for j in 0..t.pts.len() {
                let s = t.tempo(j);
                // o par como vai para o projeto.json: segundos em texto e pts inteiro
                let texto = serde_json::to_string(&json!({ "t": s, "pts": t.pts[j] })).unwrap();
                let lido: serde_json::Value = serde_json::from_str(&texto).unwrap();
                let (s2, pts2) = (lido["t"].as_f64().unwrap(), lido["pts"].as_i64().unwrap());
                assert_eq!(s2.to_bits(), s.to_bits());
                assert_eq!((t.exibido(s2), t.indice_do_pts(pts2), t.do_par(s2, Some(pts2))), (j, Some(j), j), "tb {tb:?} quadro {j}");
                // só os segundos, com 6 casas (o erro do `e9_saida.txt`)
                let seis: f64 = format!("{s:.6}").parse().unwrap();
                assert_eq!(t.exibido(seis), j, "tb {tb:?} quadro {j} com 6 casas");
                // o pts manda mesmo com segundos de outro quadro
                assert_eq!(t.do_par(s + 5.0, Some(t.pts[j])), j);
                conferidos += 1;
            }
        }
        assert_eq!(conferidos, BASES.len() * 3000);
    }

    #[test]
    fn indices_chaves_e_duracoes() {
        let t = apoio::tabela((1, 1000), apoio::pts_regulares(100, (1, 30), (1, 1000), 500));
        assert_eq!((t.indice_do_pts(500), t.indice_do_pts(533), t.indice_do_pts(534), t.indice_do_pts(0)), (Some(0), Some(1), None, None));
        assert_eq!((t.chave_ate(0), t.chave_ate(29), t.chave_ate(30), t.chave_ate(99), t.chave_ate(5000)), (Some(0), Some(0), Some(30), Some(90), Some(90)));
        assert_eq!((t.passo_mediano, t.passo, t.vfr, t.lacunas.len()), (33, (33, 34), false, 0));
        assert_eq!((t.tempo(1), t.duracao(1), t.duracao(99), t.fim_s()), (0.533, 0.034, 0.033, (t.pts[99] + 33) as f64 / 1000.0));
        // fim de trecho: o par manda; sem pts, quem alcança o fim da mídia é `n`
        let fim = t.fim_s();
        assert_eq!((t.fim_do_par(9.0, Some(t.pts[40])), t.fim_do_par(t.tempo(40), None), t.fim_do_par(fim, None), t.fim_do_par(fim - 0.002, None)), (40, 40, 100, 99));
        let sem_chave = TabelaQuadros { chave: vec![false; 100], ..t.clone() };
        assert_eq!(sem_chave.chave_ate(50), None);
        let vazia = apoio::tabela((1, 1000), Vec::new());
        assert_eq!((vazia.chave_ate(0), vazia.fim_s(), vazia.fim_do_par(1.0, None), vazia.do_par(1.0, Some(3))), (None, 0.0, 0, 0));
    }

    #[test]
    fn quadro_em_por_tempo_indice_e_pts() {
        let t = apoio::tabela((1, 15360), apoio::pts_regulares(90, (1, 30), (1, 15360), 0));
        let q = |alvo| t.quadro_em(alvo).unwrap();
        assert_eq!(q(AlvoQuadro::T { t: 1.0 }), QuadroEm { indice: 30, pts: 15360, pts_s: 1.0, dur_s: 512.0 / 15360.0 });
        assert_eq!((q(AlvoQuadro::T { t: 1.02 }).indice, q(AlvoQuadro::T { t: 1.04 }).indice), (30, 31));
        assert_eq!(q(AlvoQuadro::Indice { indice: 89 }).pts, 89 * 512);
        assert_eq!(q(AlvoQuadro::Pts { pts: 1024 }).indice, 2);
        // pts que a tabela não tem: o quadro exibido nele
        assert_eq!((q(AlvoQuadro::Pts { pts: 1500 }).indice, q(AlvoQuadro::Pts { pts: -7 }).indice, q(AlvoQuadro::Pts { pts: i64::MAX }).indice), (2, 0, 89));
        assert_eq!(codigo(t.quadro_em(AlvoQuadro::Indice { indice: 90 })), "intervalo");
        assert_eq!(codigo(apoio::tabela((1, 1000), Vec::new()).quadro_em(AlvoQuadro::T { t: 0.0 })), "intervalo");
    }

    #[test]
    fn compacta_e_recusa_passo_que_nao_cabe() {
        let t = apoio::tabela((1, 1000), vec![500, 533, 567, 600]);
        let c = t.compacta().unwrap();
        assert_eq!((c.pts0, c.passos.clone(), c.passo_mediano, c.chaves.clone(), c.vfr, c.origem), (500, vec![33, 34, 33], 33, vec![0], false, OrigemTabela::Decodificador));
        assert_eq!(
            serde_json::to_value(&c).unwrap(),
            json!({ "tb": { "num": 1, "den": 1000 }, "pts0": 500, "passos": [33, 34, 33], "passo_mediano": 33, "chaves": [0], "vfr": false, "origem": "decodificador" })
        );
        // lacuna de mais de 2³² ticks (72 min em base de 1 µs): erro, nunca valor truncado
        let lacuna = apoio::tabela((1, 1_000_000), vec![0, 33_333, 4_294_967_296 + 33_333, 4_295_033_962]);
        assert_eq!(codigo(lacuna.compacta()), "tabela_grande");
        let no_limite = apoio::tabela((1, 1_000_000), vec![0, 4_294_967_295]);
        assert_eq!(no_limite.compacta().unwrap().passos, [u32::MAX]);
        let vazia = apoio::tabela((1, 1000), Vec::new()).compacta().unwrap();
        assert_eq!((vazia.pts0, vazia.passos.len()), (0, 0));
        // pts que o JSON não leva inteiro ao front (época em base de 0,1 µs): erro, nunca valor arredondado no espelho
        let limite = (1i64 << 53) - 1;
        assert_eq!(apoio::tabela((1, 10_000_000), vec![limite - 333_333, limite]).compacta().unwrap().pts0, limite - 333_333);
        assert_eq!(codigo(apoio::tabela((1, 10_000_000), vec![limite - 333_333, limite + 1]).compacta()), "tabela_grande");
        assert_eq!(codigo(apoio::tabela((1, 10_000_000), vec![-limite - 1, -limite + 333_333]).compacta()), "tabela_grande");
    }

    #[test]
    fn taxa_nominal_e_a_menor_sem_descarte() {
        let nominal = |tb: (i64, i64), passo: (i64, i64), n: usize| apoio::tabela(tb, apoio::pts_regulares(n, passo, tb, 0)).taxa_nominal();
        // 1 ms a 12, 15 e 30; 12,5 de gravador; 25 e 50
        assert_eq!((nominal((1, 1000), (1, 12), 96), nominal((1, 1000), (1, 15), 120), nominal((1, 1000), (1, 30), 240)), (r(12, 1), r(15, 1), r(30, 1)));
        assert_eq!((nominal((1, 90000), (2, 25), 420), nominal((1, 12800), (1, 25), 300), nominal((1, 15360), (1, 50), 300)), (r(25, 2), r(25, 1), r(50, 1)));
        // 29,97 × 30, 23,976 × 24 e 59,94 × 60, em vídeo curto e longo, em base fina e de 1 ms
        for n in [40, 300, 3000] {
            assert_eq!((nominal((1, 15360), (1, 30), n), nominal((1, 30), (1, 30), n), nominal((1, 1000), (1, 30), n)), (r(30, 1), r(30, 1), r(30, 1)), "{n}");
            assert_eq!((nominal((1, 30000), (1001, 30000), n), nominal((1, 1000), (1001, 30000), n)), (r(30000, 1001), r(30000, 1001)), "{n}");
            assert_eq!((nominal((1, 1000), (1001, 24000), n), nominal((1, 1000), (1, 24), n)), (r(24000, 1001), r(24, 1)), "{n}");
            assert_eq!((nominal((1, 90000), (1001, 60000), n), nominal((1, 15360), (1, 60), n)), (r(60000, 1001), r(60, 1)), "{n}");
        }
        // taxas baixas de gravador: a seguinte da lista em que nada some; acima de 60 fica 60
        assert_eq!((nominal((1, 1000), (1, 6), 60), nominal((1, 1000), (1, 8), 60), nominal((1, 1000), (1, 10), 60), nominal((1, 1000), (1, 20), 60)), (r(10, 1), r(10, 1), r(10, 1), r(20, 1)));
        assert_eq!(nominal((1, 90000), (1, 120), 600), r(60, 1));
        // 30 qps com quadros faltando continua 30
        let falhado: Vec<i64> = apoio::pts_regulares(450, (1, 30), (1, 15360), 0).into_iter().enumerate().filter(|(i, _)| i % 7 != 3 && i % 11 != 5).map(|x| x.1).collect();
        assert_eq!(apoio::tabela((1, 15360), falhado).taxa_nominal(), r(30, 1));
        assert_eq!((apoio::tabela((1, 1), vec![0]).taxa_nominal(), apoio::tabela((1, 1), Vec::new()).taxa_nominal()), (r(30, 1), r(30, 1)));
        // pts repetido (MPEG-PS): o quadro de baixo some em qualquer taxa; a nominal continua a do vídeo
        let mut rep = apoio::pts_regulares(450, (1, 30), (1, 90000), 45000);
        for j in [100, 250, 400] {
            rep[j] = rep[j - 1];
        }
        assert_eq!(apoio::tabela((1, 90000), rep).taxa_nominal(), r(30, 1));
    }

    #[test]
    fn passos_taxa_variavel_e_lacunas() {
        let medir = |pts: &[i64]| medir_passos(r(1, 1000), pts, None);
        // arredondamento de 1 ms: constante
        assert_eq!(medir(&apoio::pts_regulares(300, (1, 30), (1, 1000), 0)), (33, (33, 34), false, vec![]));
        // oscilação de relógio dentro de ±25 %: não é variável
        let osc: Vec<i64> = apoio::pts_regulares(300, (1, 15), (1, 1000), 0).into_iter().enumerate().map(|(i, p)| p + (i as i64 * 7) % 9 - 4).collect();
        let (med, (min, max), vfr, lac) = medir(&osc);
        assert_eq!((med, vfr, lac.len()), (65, false, 0));
        assert!(min >= 58 && max <= 75);
        // uma lacuna num vídeo constante: lacuna registrada, taxa constante
        let mut um = apoio::pts_regulares(300, (1, 30), (1, 1000), 0);
        for p in &mut um[150..] {
            *p += 800;
        }
        assert_eq!(medir(&um), (33, (33, 833), false, vec![(um[149], um[150])]));
        // 15 % dos quadros faltando: variável, cada buraco é uma lacuna
        let falhado: Vec<i64> = apoio::pts_regulares(300, (1, 30), (1, 1000), 0).into_iter().enumerate().filter(|(i, _)| i % 7 != 3).map(|x| x.1).collect();
        let (_, _, vfr, lac) = medir(&falhado);
        assert_eq!((vfr, lac.len()), (true, 43));
        // menos de 2 quadros: o passo declarado
        assert_eq!(medir_passos(r(1, 15360), &[0], Some(512)), (512, (512, 512), false, vec![]));
        assert_eq!(medir_passos(r(1, 15360), &[], None), (1, (1, 1), false, vec![]));
        assert_eq!((passo_declarado(r(1, 15360), "30/1"), passo_declarado(r(1, 90000), "30000/1001"), passo_declarado(r(1, 1000), "0/0")), (Some(512), Some(3003), None));
    }

    #[test]
    fn pacote_sem_tempo_e_interpolado() {
        // MPEG-PS: um pts a cada tantos pacotes
        let t = vec![(None, true), (Some(3000), false), (None, false), (None, false), (Some(12000), false), (None, true), (None, false)];
        assert_eq!(tempos_dos_pacotes(t, 3000), (vec![0, 3000, 6000, 9000, 12000, 15000, 18000], vec![true, false, false, false, false, true, false]));
        // fluxo cru: nenhum tempo, só a taxa declarada
        assert_eq!(tempos_dos_pacotes(vec![(None, true), (None, false), (None, false)], 40).0, [0, 40, 80]);
        // quadros B: vêm fora de ordem e saem em ordem de exibição
        let b = vec![(Some(0), true), (Some(3), false), (Some(1), false), (Some(2), false)];
        assert_eq!(tempos_dos_pacotes(b, 1), (vec![0, 1, 2, 3], vec![true, false, false, false]));
        assert_eq!(tempos_dos_pacotes(Vec::new(), 1).0.len(), 0);
    }

    #[test]
    fn fracoes_do_ffprobe() {
        assert_eq!((fracao("1/15360"), fracao("10:11"), fracao("60000/2002"), fracao("N/A"), fracao("0:1"), fracao("0/0")), (Some(r(1, 15360)), Some(r(10, 11)), Some(r(30000, 1001)), None, None, None));
    }

    #[test]
    fn showinfo_e_erros_lidos_do_stderr() {
        let pasta = fontes::pasta_temporaria("quadros-showinfo");
        let arq = pasta.join("e.txt");
        // 9.0.1 e 7.0.2 com `level+info`; a sondagem do contêiner reclama antes de `Input #0` (TS cortado);
        // metadado do arquivo que imita linha de log não conta como erro nem como quadro
        let texto = "\
[h264 @ 0x55] [error] non-existing PPS 0 referenced
[h264 @ 0x55] [error] no frame!
[info] Input #0, mpegts, from 'x.ts':
[info]     title           : [error] corrupt decoded frame
[info]                     : [showinfo@tq @ 0x1] [info] n:   0 pts: 999 iskey:1
[showinfo@tq @ 0x7f] [info] config in time_base: 1/90000, frame_rate: 30/1
[showinfo@tq @ 0x7f] [info] config out time_base: 0/0, frame_rate: 0/0
[showinfo@tq @ 0x7f] [info] n:   0 pts: 666902 pts_time:7.410022 duration:   3000 duration_time:0.0333333 fmt:yuv420p cl:left sar:0/1 s:320x180 i:P iskey:1 type:I 
[showinfo@tq @ 0x7f] [info] color_range:unknown color_space:unknown color_primaries:unknown color_trc:unknown
[h264 @ 0x56] [error] error while decoding MB 10 3, bytestream -7
[showinfo@tq @ 0x7f] [info] n:   1 pts:1362902 pts_time:15.143356 duration:   3000 duration_time:0.0333333 fmt:yuv420p cl:left sar:0/1 s:320x180 i:P iskey:0 type:P 
[null @ 0x57] [error] Application provided invalid, non monotonically increasing dts to muxer in stream 0: 131 >= 131
[vist#0:0/h264 @ 0x58] [error] Error submitting packet to decoder: Invalid data found when processing input
[showinfo@tq @ 0x7f] [info] n:   2 pts: -3000 pts_time:-0.03 duration:   3000 duration_time:0.0333333 fmt:yuv420p cl:left sar:0/1 s:320x180 i:P iskey:0 type:P 
[in#0/mpegts @ 0x5a] [warning] Packet corrupt (stream = 0, dts = 1362902).
[in#0/mpegts @ 0x5a] [warning] corrupt input packet in stream 0
[vist#0:0/h264 @ 0x58] [dec:h264 @ 0x5b] [warning] corrupt decoded frame
[h264 @ 0x56] [warning] Increasing reorder buffer to 1
[showinfo@outro @ 0x7f] [info] n:   0 pts: 1 pts_time:0 duration: 1 fmt:yuv420p iskey:1 type:I 
[out#0/null @ 0x59] [info] video:112KiB audio:0KiB
";
        std::fs::write(&arq, texto).unwrap();
        let l = ler_showinfo(&arq, "tq").unwrap();
        assert_eq!((l.tb, l.pts, l.chave, l.erros), (Some(r(1, 90000)), vec![666902, 1362902, -3000], vec![true, false, false], 4));
        // sem o nível na linha (loglevel simples) e quadro sem pts
        std::fs::write(&arq, "Input #0, avi, from 'x':\n[showinfo@sb @ 0x1] config in time_base: 1/25, frame_rate: 25/1\n[showinfo@sb @ 0x1] n:   0 pts: NOPTS pts_time:NOPTS iskey:1\n").unwrap();
        let l = ler_showinfo(&arq, "sb").unwrap();
        assert_eq!((l.tb, l.pts, l.erros), (Some(r(1, 25)), vec![i64::MIN], 0));
    }

    #[test]
    fn regra_da_busca() {
        // 30 qps, quadro-chave a cada 30, mídia que começa em 100 s
        let mut t = apoio::tabela((1, 1000), apoio::pts_regulares(900, (1, 30), (1, 1000), 100_000));
        let ini = 100.0;
        // último quadro-chave ≤ j_in, menos 0,5 s; a menos de 10 s do início, sem busca
        assert_eq!((t.busca_em(ini, 0), t.busca_em(ini, 329), t.busca_em(ini, 330), t.busca_em(ini, 359)), (None, None, Some(10.5), Some(10.5)));
        assert_eq!((t.busca_em(ini, 360), t.busca_em(ini, 899)), (Some(11.5), Some(28.5)));
        // o início que conta é o do contêiner (o som pode começar antes da imagem)
        assert_eq!(t.busca_em(ini - 1.0, 300), Some(10.5));
        t.busca = Busca::SemBusca;
        assert_eq!(t.busca_em(ini, 899), None);
        t.busca = Busca::Confiavel;
        t.chave = vec![false; 900];
        assert_eq!(t.busca_em(ini, 899), None);
    }

    #[test]
    fn cache_em_arquivo_pela_midia_e_pelo_ffmpeg() {
        let cache = fontes::pasta_temporaria("quadros-cache");
        let cap = apoio::cap_falsa();
        let sha = "ab".repeat(32);
        let tab = apoio::tabela((1, 15360), apoio::pts_regulares(300, (1, 30), (1, 15360), 0));
        // tabela de pacotes não vai para o cache
        let pacotes = TabelaQuadros { origem: OrigemTabela::Pacotes, ..tab.clone() };
        assert_eq!(codigo(pacotes.guardar(&cache, &sha)), "tabela_pendente");
        let guardada = tab.clone().guardar(&cache, &sha).unwrap();
        let arquivo = cache.join("quadros").join(format!("{sha}.{}.q2", "f".repeat(16)));
        assert!(arquivo.is_file(), "{}", arquivo.display());
        assert_eq!(std::fs::read_dir(cache.join("quadros")).unwrap().count(), 1, "sobra de arquivo temporário");
        // da memória, sem tocar no vídeo (o caminho nem existe)
        let nada = Path::new("/nao/existe.mp4");
        assert!(Arc::ptr_eq(&TabelaQuadros::obter_com(&cap, &cache, &sha, nada).unwrap(), &guardada));
        // do arquivo
        memoria().lock().unwrap().remove(&(sha.clone(), cap.sha256.clone()));
        assert_eq!(*TabelaQuadros::obter_com(&cap, &cache, &sha, nada).unwrap(), tab);
        // outro ffmpeg: outra chave; a mídia nem é procurada no cache dele
        let outro = Capacidades { sha256: "e".repeat(64), ..apoio::cap_falsa() };
        assert!(TabelaQuadros::obter_com(&outro, &cache, &sha, nada).is_err());
        // arquivo de outra mídia com o nome desta, ou estragado: descartado
        memoria().lock().unwrap().remove(&(sha.clone(), cap.sha256.clone()));
        let de_outra = cache.join("quadros").join(format!("{}.{}.q2", "cd".repeat(32), "f".repeat(16)));
        std::fs::copy(&arquivo, &de_outra).unwrap();
        assert!(TabelaQuadros::obter_com(&cap, &cache, &"cd".repeat(32), nada).is_err());
        assert!(!de_outra.exists());
        // o nome guarda só 16 dígitos do ffmpeg: o de outro binário com os mesmos 16 é descartado pelo SHA-256 de dentro
        let prefixo = Capacidades { sha256: format!("{}{}", "f".repeat(16), "0".repeat(48)), ..apoio::cap_falsa() };
        tab.clone().guardar(&cache, &sha).unwrap();
        assert!(TabelaQuadros::obter_com(&prefixo, &cache, &sha, nada).is_err());
        assert!(!arquivo.exists());
        // formato de outra versão: descartado
        tab.clone().guardar(&cache, &sha).unwrap();
        memoria().lock().unwrap().remove(&(sha.clone(), cap.sha256.clone()));
        let velho = std::fs::read_to_string(&arquivo).unwrap().replacen(FORMATO, "sicro.quadros/1", 1);
        std::fs::write(&arquivo, velho).unwrap();
        assert!(TabelaQuadros::obter_com(&cap, &cache, &sha, nada).is_err());
        assert!(!arquivo.exists());
        std::fs::write(&arquivo, b"{ lixo").unwrap();
        assert!(TabelaQuadros::obter_com(&cap, &cache, &sha, nada).is_err());
        assert!(!arquivo.exists());
        assert_eq!(codigo(TabelaQuadros::obter_com(&cap, &cache, "../fora", nada)), "sha256_invalido: ../fora");
    }

    // ---------------------------------------------------------------- com ffmpeg

    fn rodar_ffmpeg(cap: &Capacidades, pasta: &Path, nome: &str, args: &[&str]) -> Vec<u8> {
        let mut a = textos(&["-hide_banner", "-nostdin", "-y", "-loglevel", "error"]);
        a.extend(textos(args));
        let cmd = Comando {
            nome: nome.into(),
            programa: cap.binario.clone(),
            args: a,
            pasta: pasta.to_path_buf(),
            ambiente: Vec::new(),
            sem_ambiente: Vec::new(),
            stderr: pasta.join(format!("{nome}.stderr")),
        };
        let (fim, out) = exec::rodar_com_saida(&cmd, &sem_cancelar()).expect("ffmpeg");
        exec::exigir_sucesso(&cmd, &fim).unwrap_or_else(|e| panic!("{e} ({})", cap.versao));
        out
    }

    fn do_decodificador(cap: &Capacidades, video: &Path) -> TabelaQuadros {
        TabelaQuadros::do_decodificador(cap, video, &sem_cancelar(), &mut |_| {}).unwrap_or_else(|e| panic!("{}: {e}", video.display()))
    }

    #[test]
    fn tabela_do_decodificador_na_matriz_inteira() {
        let Some(cap) = fontes::cap() else { return };
        let mut quadros = 0;
        for &f in MATRIZ {
            let m = apoio::medida(&cap, f);
            let (tab, d) = (&m.info.quadros, fontes::descricao(f));
            let video = fontes::gerar(f).unwrap();
            // os pts que outro `showinfo` lê e o número desenhado em cada quadro
            assert_eq!(tab.pts, medir::pts(&cap, &video).unwrap(), "{f:?} ({})", cap.versao);
            if let Some(esperados) = fontes::quadros_esperados(f) {
                assert_eq!(m.numeros, esperados, "{f:?}");
            }
            assert!(tab.pts.windows(2).all(|p| p[0] <= p[1]), "{f:?}: pts fora de ordem");
            assert_eq!((tab.origem, tab.ffmpeg_sha256.as_str(), tab.erros_decodificacao), (OrigemTabela::Decodificador, cap.sha256.as_str(), 0), "{f:?}");
            // quadro-chave a cada `gop` quadros codificados
            let chaves: Vec<u32> = (0..tab.pts.len()).filter(|&j| tab.chave[j]).map(|j| m.numeros[j]).collect();
            if !matches!(f, Fonte::TaxaVariavel | Fonte::Mp4ListaDeEdicao) {
                assert!(chaves.iter().all(|n| n % d.gop == 0) && chaves.len() == tab.pts.len().div_ceil(d.gop as usize), "{f:?}: chaves {chaves:?}");
            }
            assert_eq!(tab.taxa_nominal(), r(d.fps.0, d.fps.1), "{f:?}");
            assert_eq!(tab.vfr, matches!(f, Fonte::TaxaVariavel | Fonte::MpegPs | Fonte::Oscilacao), "{f:?}");
            assert_eq!(tab.compacta().unwrap().passos.len() + 1, tab.pts.len());
            quadros += tab.pts.len();
        }
        eprintln!("tabela do decodificador ({}): {} fontes, {quadros} quadros conferidos", cap.versao, MATRIZ.len());
    }

    #[test]
    fn decodificador_x_pacotes_em_avi_lista_de_edicao_e_ts_cortado() {
        let Some(cap) = fontes::cap() else { return };
        let par = |f: Fonte| {
            let video = fontes::gerar(f).unwrap();
            (apoio::medida(&cap, f), TabelaQuadros::de_pacotes(&cap, &video).unwrap())
        };
        // AVI: nenhum pacote traz pts; pelo dts a tabela de pacotes é a do decodificador (base 1/25)
        let (m, p) = par(Fonte::Avi25);
        let d = &m.info.quadros;
        assert_eq!((d.tb, d.pts.len(), d.pacotes, p.origem, p.busca), (r(1, 25), 375, 375, OrigemTabela::Pacotes, Busca::SemBusca));
        assert_eq!((&p.pts, &p.chave, p.tb), (&d.pts, &d.chave, d.tb));
        assert_eq!(d.pts, (0..375).collect::<Vec<i64>>());
        // lista de edição (corte por cópia): há mais pacotes que quadros; sem os de descarte, bate
        let (m, p) = par(Fonte::Mp4ListaDeEdicao);
        let d = &m.info.quadros;
        assert_eq!((d.pacotes, d.pts.len(), p.pacotes, p.pts.len()), (317, 302, 317, 302), "pacotes × quadros");
        assert_eq!((&p.pts, &p.chave), (&d.pts, &d.chave));
        assert_eq!((m.numeros[0], m.numeros[301], d.lacunas.len()), (45, 348, 1), "começa no quadro de 1,5 s; no fim falta o 346 e o 347");
        assert_eq!(d.declarado.nb_frames, Some(317), "o contêiner declara os pacotes, não os quadros");
        // TS cortado no meio do GOP: os 30 primeiros pacotes não viram quadro
        let (m, p) = par(Fonte::TsMeioDoGop);
        let d = &m.info.quadros;
        assert_eq!((d.pacotes, d.pts.len(), p.pts.len()), (300, 270, 300));
        assert_eq!((&p.pts[30..], &p.chave[30..], p.chave[..30].iter().any(|k| *k)), (&d.pts[..], &d.chave[..], false));
        assert_eq!((m.numeros[0], m.numeros[269], d.chave[0]), (180, 449, true));
        // MPEG-PS: a maioria dos pacotes vem sem tempo; a de pacotes interpola e a do decodificador é a que vale
        let (m, p) = par(Fonte::MpegPs);
        assert_eq!((m.info.quadros.pts.len(), p.pts.len(), p.pacotes), (450, 450, 450));
        assert_eq!(p.chave, m.info.quadros.chave);
        // as demais fontes de pts inteiros nos pacotes: iguais
        for f in [Fonte::Mp4, Fonte::Mp4Escala1000, Fonte::Mp4Escala30, Fonte::Mkv30, Fonte::Mkv23976, Fonte::Ts5994, Fonte::Mp4QuadrosB, Fonte::TaxaVariavel, Fonte::Oscilacao, Fonte::Giro90] {
            let (m, p) = par(f);
            assert_eq!((&p.pts, &p.chave, p.tb, p.pacotes), (&m.info.quadros.pts, &m.info.quadros.chave, m.info.quadros.tb, m.info.quadros.pts.len() as u64), "{f:?}");
        }
    }

    #[test]
    fn declarado_ao_lado_do_medido() {
        let Some(cap) = fontes::cap() else { return };
        let t = &apoio::medida(&cap, Fonte::TaxaVariavel).info.quadros;
        // o contêiner declara 30 qps; a média, menos
        assert_eq!((t.declarado.r_frame_rate.as_str(), t.declarado.nb_frames, t.pts.len(), t.pacotes), ("30/1", Some(341), 341, 341));
        assert_ne!(t.declarado.avg_frame_rate, "30/1");
        assert!((t.declarado.duracao.unwrap() - 15.0).abs() < 0.1);
        assert_eq!((t.passo, t.passo_mediano, t.lacunas.len()), ((512, 7168), 512, 85));
        assert_eq!(t.lacunas[0], (2 * 512, 4 * 512), "falta o quadro 3");
        let t = &apoio::medida(&cap, Fonte::Mkv23976).info.quadros;
        assert_eq!((t.declarado.r_frame_rate.as_str(), t.declarado.nb_frames, t.passo), ("24000/1001", None, (41, 42)));
    }

    #[test]
    fn erros_de_decodificacao_so_na_fonte_danificada() {
        let Some(cap) = fontes::cap() else { return };
        // íntegras: zero em toda a matriz (conferido em `tabela_do_decodificador_na_matriz_inteira`)
        let pasta = fontes::pasta_temporaria("quadros-danificada");
        let mut bytes = std::fs::read(fontes::gerar(Fonte::Mp4).unwrap()).unwrap();
        let n = bytes.len();
        // três trechos de 600 bytes zerados no meio dos dados (`critica-pericia` E4b)
        for parte in [3, 5, 7] {
            bytes[n * parte / 10..n * parte / 10 + 600].fill(0);
        }
        let danificada = pasta.join("danificada.mp4");
        std::fs::write(&danificada, &bytes).unwrap();
        let t = do_decodificador(&cap, &danificada);
        assert!(t.erros_decodificacao > 0, "{}", cap.versao);
        assert_eq!(t.pacotes, 450);
        assert!((300..=450).contains(&t.pts.len()), "{} quadros", t.pts.len());
        eprintln!("fonte danificada ({}): {} linhas de erro, {} quadros de 450 pacotes", cap.versao, t.erros_decodificacao, t.pts.len());
        // TS sem 40 pacotes de 188 bytes no meio: o ffmpeg perde 15 quadros e só avisa (nenhuma linha de erro)
        let ts = std::fs::read(fontes::gerar(Fonte::Ts5994).unwrap()).unwrap();
        let meio = ts.len() / 188 / 2 * 188;
        let buraco = pasta.join("buraco.ts");
        std::fs::write(&buraco, [&ts[..meio], &ts[meio + 40 * 188..]].concat()).unwrap();
        let t = do_decodificador(&cap, &buraco);
        assert!(t.pts.len() < 899 && !t.lacunas.is_empty(), "{} quadros", t.pts.len());
        assert!(t.erros_decodificacao > 0, "TS sem pacotes no meio passou por íntegro ({})", cap.versao);
        eprintln!("TS sem 40 pacotes ({}): {} linhas, {} quadros de 899", cap.versao, t.erros_decodificacao, t.pts.len());
        // estrago que faz o 9.0.1 desistir com o código 69 (o 7.0.2 sai com 0): a tabela vale até onde ele foi
        let mut b = std::fs::read(fontes::gerar(Fonte::Mp4).unwrap()).unwrap();
        let (a, z) = (b.len() * 15 / 100, b.len() * 70 / 100);
        let mut i = a;
        while i + 400 < a + z {
            b[i..i + 300].fill(0);
            i += 400;
        }
        std::fs::write(pasta.join("desiste.mp4"), &b).unwrap();
        let t = do_decodificador(&cap, &pasta.join("desiste.mp4"));
        assert!(t.erros_decodificacao > 0 && (50..450).contains(&t.pts.len()), "{} quadros ({})", t.pts.len(), cap.versao);
        // íntegro com metadado que imita linha de log (erro e quadro do `showinfo`): nenhum erro, só os quadros dele
        let titulo = "title=[error] corrupt decoded frame\n[showinfo@tq @ 0x1] [info] n:   0 pts: 999 iskey:1";
        rodar_ffmpeg(&cap, &pasta, "metadado", &["-f", "lavfi", "-i", "testsrc2=s=160x90:r=25:d=0.4", "-c:v", "libx264", "-preset", "ultrafast", "-metadata", titulo, "metadado.mp4"]);
        let t = do_decodificador(&cap, &pasta.join("metadado.mp4"));
        assert_eq!((t.erros_decodificacao, t.pts), (0, (0..10).map(|j| j * 512).collect::<Vec<i64>>()), "{}", cap.versao);
    }

    #[test]
    fn sonda_de_busca_reprova_onde_a_busca_falha() {
        let Some(cap) = fontes::cap() else { return };
        // MPEG-PS: depois da busca os quadros sem pts no pacote não chegam (o trecho que começa no quadro-chave vem vazio)
        let ps = apoio::medida(&cap, Fonte::MpegPs);
        assert_eq!(ps.info.quadros.busca, Busca::SemBusca, "{}", cap.versao);
        assert_eq!(ps.info.quadros.busca_em(ps.info.inicio_s, 400), None);
        // a mesma leitura feita à mão: o trecho do quadro 330 (quadro-chave) com o -ss da regra perde quadros
        let (tab, video) = (&ps.info.quadros, fontes::gerar(Fonte::MpegPs).unwrap());
        let alvo = tab.alvo_da_busca(ps.info.inicio_s, 330).expect("a regra permite a busca");
        let pasta = fontes::pasta_temporaria("quadros-busca");
        let ler = |antes: &[&str]| -> Vec<u32> {
            let filtro = format!("trim=start_pts={}:end_pts={},extractplanes=y", tab.pts[330], tab.pts[338]);
            let mut a = vec!["-copyts"];
            a.extend_from_slice(antes);
            let caminho = video.to_string_lossy();
            a.extend_from_slice(&["-i", &caminho, "-map", "0:v:0", "-vf", &filtro, "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "gray", "-"]);
            rodar_ffmpeg(&cap, &pasta, "ps", &a).as_chunks::<{ 320 * 180 }>().0.iter().map(|y| medir::identificar(y, 320, 180)).collect()
        };
        assert_eq!(ler(&[]), (330..338).collect::<Vec<u32>>(), "sem busca o trecho vem inteiro");
        assert_ne!(ler(&["-ss", &format!("{alvo:.6}")]), (330..338).collect::<Vec<u32>>(), "com a busca da regra o trecho vem errado");
        // aprovadas: toda fonte longa o bastante para a regra permitir busca
        for f in [Fonte::Mp4, Fonte::Mp4Escala1000, Fonte::Mp4Escala30, Fonte::Mkv30, Fonte::Mkv23976, Fonte::Avi25, Fonte::Ts5994, Fonte::Mp4QuadrosB, Fonte::TaxaVariavel, Fonte::Oscilacao, Fonte::Giro90] {
            let m = apoio::medida(&cap, f);
            assert_eq!(m.info.quadros.busca, Busca::Confiavel, "{f:?} ({})", cap.versao);
            let n = m.info.quadros.pts.len();
            assert!(m.info.quadros.busca_em(m.info.inicio_s, n - 1).is_some_and(|a| a >= BUSCA_GUARDA_S), "{f:?}");
            assert_eq!(m.info.quadros.busca_em(m.info.inicio_s, 0), None);
        }
        // curtas (menos de 10,5 s até o último quadro-chave): nada a sondar, nunca se busca
        for f in [Fonte::Mp4ListaDeEdicao, Fonte::TsMeioDoGop, Fonte::FaixaCheia, Fonte::Entrelacado, Fonte::PixelNaoQuadrado] {
            let m = apoio::medida(&cap, f);
            assert_eq!(m.info.quadros.busca, Busca::SemBusca, "{f:?}");
            assert!((0..m.info.quadros.pts.len()).all(|j| m.info.quadros.alvo_da_busca(m.info.inicio_s, j).is_none()), "{f:?}");
        }
    }

    #[test]
    fn pts_relido_acha_o_mesmo_quadro_na_matriz() {
        let Some(cap) = fontes::cap() else { return };
        let (mut conferidos, mut repetidos) = (0, 0);
        for &f in MATRIZ {
            let t = apoio::medida(&cap, f).info.quadros.clone();
            for j in 0..t.pts.len() {
                // pts repetido (MPEG-PS): o par não separa os dois quadros
                if t.pts.get(j + 1) == Some(&t.pts[j]) || (j > 0 && t.pts[j - 1] == t.pts[j]) {
                    repetidos += 1;
                    continue;
                }
                let lido: serde_json::Value = serde_json::from_str(&json!({ "t": t.tempo(j), "pts": t.pts[j] }).to_string()).unwrap();
                let (s, pts) = (lido["t"].as_f64().unwrap(), lido["pts"].as_i64().unwrap());
                let seis: f64 = format!("{s:.6}").parse().unwrap();
                assert_eq!((t.indice_do_pts(pts), t.exibido(s), t.exibido(seis), t.quadro_em(AlvoQuadro::Pts { pts }).unwrap().indice), (Some(j), j, j, j as u32), "{f:?} quadro {j}");
                conferidos += 1;
            }
        }
        eprintln!("pts gravado e relido ({}): {conferidos} quadros, 100 %; {repetidos} de pts repetido fora da conta", cap.versao);
        assert!(conferidos > 6000 && repetidos <= 12);
    }

    #[test]
    fn obter_prefere_a_do_decodificador_e_nao_a_pede() {
        let Some(cap) = fontes::cap() else { return };
        let cache = fontes::pasta_temporaria("quadros-obter");
        let video = fontes::gerar(Fonte::Mkv23976).unwrap();
        let sha = sonda::sha256_arquivo(&video).unwrap();
        // sem cache: a de pacotes, feita na hora e guardada só na memória
        let a = TabelaQuadros::obter(&cache, &sha, &video).unwrap();
        assert_eq!((a.origem, a.busca, a.pts.len()), (OrigemTabela::Pacotes, Busca::SemBusca, 360));
        assert!(Arc::ptr_eq(&a, &TabelaQuadros::obter(&cache, &sha, &video).unwrap()));
        assert!(!cache.join("quadros").exists(), "obter não grava nem pede a tabela do decodificador");
        // quem roda a passada guarda; daí em diante vale a do decodificador
        let b = do_decodificador(&cap, &video).guardar(&cache, &sha).unwrap();
        let c = TabelaQuadros::obter(&cache, &sha, &video).unwrap();
        assert!(Arc::ptr_eq(&b, &c));
        assert_eq!((c.origem, c.busca, &c.pts), (OrigemTabela::Decodificador, Busca::Confiavel, &a.pts));
        // outro processo (memória vazia): vem do arquivo
        memoria().lock().unwrap().remove(&(sha.clone(), cap.sha256.clone()));
        assert_eq!(*TabelaQuadros::obter(&cache, &sha, &video).unwrap(), *b);
    }

    #[test]
    fn progresso_e_cancelamento() {
        let Some(cap) = fontes::cap() else { return };
        let video = fontes::gerar(Fonte::Ts5994).unwrap();
        let mut vistos = Vec::new();
        TabelaQuadros::do_decodificador(&cap, &video, &sem_cancelar(), &mut |f| vistos.push(f)).unwrap();
        assert!(vistos.windows(2).all(|p| p[0] <= p[1]) && vistos.iter().all(|f| (0.0..=1.0).contains(f)), "{vistos:?}");
        assert_eq!(vistos.last(), Some(&1.0));
        assert!(vistos.iter().any(|f| *f > 0.9 && *f < 1.0), "a sonda de busca também avança: {vistos:?}");
        let e = codigo(TabelaQuadros::do_decodificador(&cap, &video, &AtomicBool::new(true), &mut |_| {}));
        assert!(e.starts_with("cancelado"), "{e}");
        // arquivo que não é mídia: erro, não tabela vazia
        let lixo = fontes::pasta_temporaria("quadros-lixo").join("lixo.mp4");
        std::fs::write(&lixo, b"isto nao e um video").unwrap();
        assert!(TabelaQuadros::do_decodificador(&cap, &lixo, &sem_cancelar(), &mut |_| {}).is_err());
        assert!(TabelaQuadros::de_pacotes(&cap, &lixo).is_err());
    }

    // ---------------------------------------------------------------- fonte_info

    fn info_de(cap: &Capacidades, video: &Path, cor: Option<&CorMidia>) -> FonteInfo {
        let tab = TabelaQuadros::de_pacotes(cap, video).unwrap();
        fonte_info(cap, video, &"1".repeat(64), Arc::new(tab), cor).unwrap()
    }

    /// Primeiro quadro em RGB, com os argumentos de entrada dados (`-noautorotate`).
    fn primeiro_quadro(cap: &Capacidades, pasta: &Path, video: &Path, antes: &[&str], w: u32, h: u32) -> medir::QuadroRgb {
        let caminho = video.to_string_lossy();
        let mut a = antes.to_vec();
        a.extend_from_slice(&["-i", &caminho, "-map", "0:v:0", "-frames:v", "1", "-vf", "scale=flags=bicubic+accurate_rnd+bitexact+full_chroma_int,format=rgb24", "-f", "rawvideo", "-"]);
        let px = rodar_ffmpeg(cap, pasta, "quadro", &a);
        assert_eq!(px.len(), (w * h * 3) as usize, "tamanho do quadro decodificado");
        medir::QuadroRgb { w, h, px }
    }

    #[test]
    fn fonte_info_pixel_10_11_e_giro_do_conteiner() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("quadros-info");
        // pixel 10:11: 720×480 codificado, 654×480 exibido
        let video = fontes::gerar(Fonte::PixelNaoQuadrado).unwrap();
        let i = info_de(&cap, &video, None);
        assert_eq!((i.exibido, i.sar, i.giro, i.entrelacado, i.pix_fmt.as_str()), ((654, 480), r(10, 11), 0, false, "yuv420p"));
        // a cadeia de cor do motor entrega exatamente esse tamanho
        let cadeia = cor::cadeia_fonte(i.matriz, i.faixa, &i.pix_fmt, i.exibido).unwrap();
        let px = rodar_ffmpeg(&cap, &pasta, "exibido", &["-i", &video.to_string_lossy(), "-frames:v", "1", "-vf", &cadeia, "-f", "rawvideo", "-pix_fmt", "gray", "-"]);
        assert_eq!(px.len(), 654 * 480);
        // giro de 90 no contêiner: lados trocados e `giro` no sentido horário (o `rotation` do ffprobe é o contrário)
        let video = fontes::gerar(Fonte::Giro90).unwrap();
        let i = info_de(&cap, &video, None);
        assert_eq!((i.exibido, i.sar, i.giro), ((180, 320), r(1, 1), 270));
        // pixel: a marca vermelha do quadro codificado, levada pelo giro, cai onde o decodificador a mostra
        let vermelho = [255, 0, 0];
        let codificado = primeiro_quadro(&cap, &pasta, &video, &["-noautorotate"], 320, 180);
        let exibido = primeiro_quadro(&cap, &pasta, &video, &[], 180, 320);
        let (cx, cy) = medir::centro(&codificado, vermelho, 60).expect("marca no quadro codificado");
        let (ex, ey) = medir::centro(&exibido, vermelho, 60).expect("marca no quadro exibido");
        let p = geometria::afim_giro(i.giro, false, false).ponto(crate::edicao::modelo::Ponto { x: cx / 320.0, y: cy / 180.0 });
        assert!((p.x * 180.0 - ex).abs() <= 1.0 && (p.y * 320.0 - ey).abs() <= 1.0, "giro {}: previsto ({}, {}), medido ({ex}, {ey})", i.giro, p.x * 180.0, p.y * 320.0);
        let m = fontes::marca_esperada(Fonte::Giro90, 0);
        assert!(((m[0] + m[2] / 2.0) * 180.0 - ex).abs() <= 2.0 && ((m[1] + m[3] / 2.0) * 320.0 - ey).abs() <= 2.0);
        // entrelaçado
        let i = info_de(&cap, &fontes::gerar(Fonte::Entrelacado).unwrap(), None);
        assert_eq!((i.exibido, i.entrelacado, i.sar), ((720, 480), true, r(1, 1)));
    }

    #[test]
    fn fonte_info_inicio_fora_e_antes_do_zero() {
        let Some(cap) = fontes::cap() else { return };
        // TS que começa em 3600 s: o início é o do contêiner (o som), como está
        let video = fontes::gerar(Fonte::Ts5994).unwrap();
        let i = info_de(&cap, &video, None);
        assert!((i.inicio_s - 3601.389978).abs() < 1e-6, "{}", i.inicio_s);
        assert_eq!((i.conteiner.as_str(), i.codec.as_str()), ("mpegts", "h264"));
        let a = i.audio.clone().unwrap();
        assert_eq!((a.taxa, a.canais), (48000, 2));
        assert!((a.inicio_s - i.inicio_s).abs() < 1e-6 && i.quadros.tempo(0) > i.inicio_s, "a imagem começa depois do som");
        // MKV com o som antes do zero: início negativo, sem zerar
        let pasta = fontes::pasta_temporaria("quadros-negativo");
        let base = fontes::gerar(Fonte::Mp4).unwrap();
        rodar_ffmpeg(&cap, &pasta, "negativo", &["-i", &base.to_string_lossy(), "-t", "3", "-c", "copy", "-output_ts_offset", "-1", "-avoid_negative_ts", "disabled", "negativo.mkv"]);
        let i = info_de(&cap, &pasta.join("negativo.mkv"), None);
        assert!(i.inicio_s < -0.9 && i.inicio_s > -1.1, "{}", i.inicio_s);
        assert_eq!(i.audio.as_ref().map(|a| a.inicio_s), Some(i.inicio_s));
        assert_eq!(i.conteiner, "matroska,webm");
    }

    #[test]
    fn fonte_info_cor_etiqueta_presuncao_e_escolha_do_perito() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("quadros-cor");
        let gerar = |nome: &str, tamanho: &str, extra: &[&str]| -> PathBuf {
            let fonte = format!("testsrc2=s={tamanho}:r=30:d=0.2");
            let mut a = vec!["-f", "lavfi", "-i", fonte.as_str(), "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"];
            a.extend_from_slice(extra);
            a.push(nome);
            rodar_ffmpeg(&cap, &pasta, nome, &a);
            pasta.join(nome)
        };
        let cor_de = |video: &Path| {
            let i = info_de(&cap, video, None);
            (i.matriz, i.faixa)
        };
        // sem etiqueta: pela altura
        assert_eq!(cor_de(&gerar("sd.mp4", "320x180", &[])), (Matriz::Bt601, Faixa::Tv));
        assert_eq!(cor_de(&gerar("sd576.mp4", "720x576", &[])), (Matriz::Bt601, Faixa::Tv));
        assert_eq!(cor_de(&gerar("hd.mp4", "1280x720", &[])), (Matriz::Bt709, Faixa::Tv));
        // com etiqueta: vale a etiqueta, contra a altura
        let bt709: &[&str] = &["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709"];
        assert_eq!(cor_de(&gerar("sd709.mp4", "320x180", bt709)), (Matriz::Bt709, Faixa::Tv));
        assert_eq!(cor_de(&gerar("hd601.mp4", "1280x720", &["-colorspace", "smpte170m"])), (Matriz::Bt601, Faixa::Tv));
        assert_eq!(cor_de(&gerar("hd470.mp4", "1280x720", &["-colorspace", "bt470bg"])), (Matriz::Bt601, Faixa::Tv));
        // faixa cheia: pela etiqueta `pc` e pelo formato `yuvj420p`
        assert_eq!(cor_de(&gerar("pc.mp4", "1280x720", &["-color_range", "pc"])), (Matriz::Bt709, Faixa::Pc));
        let cheia = info_de(&cap, &fontes::gerar(Fonte::FaixaCheia).unwrap(), None);
        assert_eq!((cheia.pix_fmt.as_str(), cheia.matriz, cheia.faixa), ("yuvj420p", Matriz::Bt601, Faixa::Pc));
        // `MidiaRef.cor` manda na matriz e na faixa
        let video = fontes::gerar(Fonte::Mp4).unwrap();
        assert_eq!(cor_de(&video), (Matriz::Bt601, Faixa::Tv));
        for (matriz, faixa) in [(Matriz::Bt709, Faixa::Pc), (Matriz::Bt709, Faixa::Tv), (Matriz::Bt601, Faixa::Pc)] {
            let i = info_de(&cap, &video, Some(&CorMidia { matriz, faixa }));
            assert_eq!((i.matriz, i.faixa), (matriz, faixa));
        }
        let i = info_de(&cap, &fontes::gerar(Fonte::FaixaCheia).unwrap(), Some(&CorMidia { matriz: Matriz::Bt709, faixa: Faixa::Tv }));
        assert_eq!((i.matriz, i.faixa), (Matriz::Bt709, Faixa::Tv));
    }

    #[test]
    fn fonte_info_audio_imagem_e_arquivo_so_de_audio() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("quadros-midias");
        // áudio: início, taxa e canais; e sem áudio
        let i = info_de(&cap, &fontes::gerar(Fonte::Mp4).unwrap(), None);
        assert_eq!(i.audio, Some(AudioInfo { inicio_s: 0.0, taxa: 48000, canais: 2 }));
        assert_eq!((i.sha256.as_str(), i.inicio_s, i.exibido, i.conteiner.as_str(), i.codec.as_str(), i.quadros.pts.len()), ("1".repeat(64).as_str(), 0.0, (320, 180), "mov,mp4,m4a,3gp,3g2,mj2", "h264", 450));
        let i = info_de(&cap, &fontes::gerar(Fonte::Avi25).unwrap(), None);
        assert_eq!((i.audio, i.conteiner.as_str()), (Some(AudioInfo { inicio_s: 0.0, taxa: 8000, canais: 1 }), "avi"));
        assert_eq!(info_de(&cap, &fontes::gerar(Fonte::Mkv23976).unwrap(), None).audio, None);
        // imagem PNG: tabela de 1 quadro (pts 0, base 1/1), lados pares, pronta para a exportação
        rodar_ffmpeg(&cap, &pasta, "png", &["-f", "lavfi", "-i", "testsrc2=s=321x241:d=1", "-frames:v", "1", "foto.png"]);
        let foto = pasta.join("foto.png");
        for tab in [TabelaQuadros::de_pacotes(&cap, &foto).unwrap(), do_decodificador(&cap, &foto), apoio::tabela((1, 25), vec![0, 1, 2])] {
            let i = fonte_info(&cap, &foto, "00", Arc::new(tab), None).unwrap();
            let t = &i.quadros;
            assert_eq!((t.tb, &t.pts, &t.chave, t.origem, t.pacotes, t.passo_mediano), (r(1, 1), &vec![0], &vec![true], OrigemTabela::Decodificador, 1, 1));
            assert_eq!((i.exibido, i.inicio_s, i.audio.clone(), i.codec.as_str(), i.giro, i.sar), ((320, 240), 0.0, None, "png", 0, r(1, 1)));
            assert_eq!(t.quadro_em(AlvoQuadro::T { t: 3.0 }).unwrap().indice, 0);
        }
        // arquivo só de áudio: tabela vazia, exibido (0, 0)
        rodar_ffmpeg(&cap, &pasta, "wav", &["-f", "lavfi", "-i", "sine=f=440:d=1", "-ar", "44100", "-ac", "1", "-c:a", "pcm_s16le", "som.wav"]);
        let som = pasta.join("som.wav");
        for tab in [TabelaQuadros::de_pacotes(&cap, &som).unwrap(), do_decodificador(&cap, &som)] {
            let i = fonte_info(&cap, &som, "00", Arc::new(tab), None).unwrap();
            assert_eq!((i.quadros.pts.len(), i.quadros.origem, i.exibido, i.codec.as_str(), i.pix_fmt.as_str()), (0, OrigemTabela::Decodificador, (0, 0), "", ""));
            assert_eq!((i.audio, i.conteiner.as_str(), i.inicio_s), (Some(AudioInfo { inicio_s: 0.0, taxa: 44100, canais: 1 }), "wav", 0.0));
        }
        // áudio com capa: a capa não é vídeo
        rodar_ffmpeg(&cap, &pasta, "capa", &["-i", "som.wav", "-i", "foto.png", "-map", "0:a", "-map", "1:v", "-c:a", "aac", "-c:v", "png", "-disposition:v", "attached_pic", "capa.m4a"]);
        let i = info_de(&cap, &pasta.join("capa.m4a"), None);
        assert_eq!((i.quadros.pts.len(), i.exibido, i.audio.map(|a| (a.taxa, a.canais))), (0, (0, 0), Some((44100, 1))));
    }
}
