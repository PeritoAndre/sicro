//! Mídia de prévia (§7.1, decisão O8): o `<video>` toca o original, a cópia por remux ou o proxy de CPU. Dono: P0.4.
//! Cache (§4): `previa/<sha256>.mp4 + .json` e `proxies/<sha256>.720.mp4 + .json`. O `.json` só aparece depois do
//! arquivo conferido: sem ele (ou com tamanho diferente) a prévia não está pronta.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use super::executar::{ERRO_CANCELADO, ERRO_LEITURA_DIVERGENTE};
use super::ffmpeg::exec::{self, Comando};
use super::ffmpeg::sonda::Capacidades;
use super::modelo::{Racional, TamanhoPx};
use super::motor::FonteInfo;
use super::quadros::{OrigemTabela, TabelaQuadros};
use crate::error::{Result, SicroError};

/// O que o `<video>` toca (`Origem` = intermediário de origem, Fase 1).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TipoPrevia {
    Original,
    Copia,
    Proxy,
    Origem,
}

/// O que o `<video>` toca. `relativo`: ao caso em `Original`; nos demais, ao cache da Edição do caso.
/// `desloc_s` = pts0 da fonte − pts0 do arquivo tocado; `de_s`/`ate_s` = faixa em tempo de fonte (da tabela);
/// `largura`/`altura` em px do arquivo tocado; `pronta` false = ainda sendo feita na fila de fundo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreviaMidia {
    pub relativo: String,
    pub tipo: TipoPrevia,
    pub desloc_s: f64,
    pub de_s: f64,
    pub ate_s: f64,
    pub largura: u32,
    pub altura: u32,
    pub pronta: bool,
}

/// Passo mínimo e máximo entre quadros, em segundos.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PassoS {
    pub min: f64,
    pub max: f64,
}

/// O que a tela sabe de uma mídia (`edicao_midia_info`). `id` = `video_media.id` do caso; `exibido` em px;
/// `de_s`/`ate_s` em tempo de fonte; `n` = quadros; `erros` None = passada de integridade ainda não feita;
/// `marcavel` false = a tabela ainda é a de pacotes num contêiner sem tempo confiável nos pacotes (H.264 e HEVC
/// crus, MPEG-PS, MPEG de vídeo): marcar entrada, saída ou quadro tiraria o pts de uma tabela que pode errar por 2×.
// CONTRATO: `marcavel` acrescentado (decisão (h)17; no TS: `marcavel: boolean`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MidiaInfo {
    pub id: String,
    pub sha256: String,
    pub nome: String,
    pub exibido: TamanhoPx,
    pub taxa: Racional,
    pub vfr: bool,
    pub passo: PassoS,
    pub de_s: f64,
    pub ate_s: f64,
    pub n: u32,
    pub tem_audio: bool,
    pub previa: PreviaMidia,
    pub tabela: OrigemTabela,
    pub erros: Option<u32>,
    pub marcavel: bool,
}

/// Parâmetros do proxy de CPU (O8): 720p, `libx264 veryfast crf 23`, GOP 12, sem quadros B, pts iguais aos da fonte.
pub const PROXY_ALTURA: u32 = 720;
pub const PROXY_CRF: u32 = 23;
pub const PROXY_GOP: u32 = 12;

const PASTA_COPIA: &str = "previa";
const PASTA_PROXY: &str = "proxies";
/// Versão do `.json`: mudar refaz as prévias de todas as máquinas.
const VERSAO_REGISTRO: u32 = 1;
/// Variação aceita do deslocamento entre quadros (M2: ≤ 1 µs).
const TOLERANCIA_S: f64 = 1e-6;
/// Áudio que cabe em MP4 e toca nos dois motores sem conversão; o resto vira AAC.
const AUDIO_COPIA: &[&str] = &["aac", "mp3"];
/// Fração do progresso que vai para o ffmpeg; o resto é a conferência.
const PARTE_FFMPEG: f32 = 0.95;

/// Tipo que serve a essa fonte: `Original` (MP4/MOV que começa em zero, H.264 8 bits 4:2:0), senão `Copia`;
/// `Proxy` quando o codec não é H.264 8 bits 4:2:0.
pub fn decidir(fonte: &FonteInfo) -> TipoPrevia {
    if !codec_serve(fonte) {
        TipoPrevia::Proxy
    } else if conteiner_mp4(&fonte.conteiner) && comeca_em_zero(fonte) {
        TipoPrevia::Original
    } else {
        TipoPrevia::Copia
    }
}

fn codec_serve(f: &FonteInfo) -> bool {
    f.codec == "h264" && matches!(f.pix_fmt.as_str(), "yuv420p" | "yuvj420p")
}

/// `format_name` da família ISO (o ffprobe dá o mesmo nome para MP4, MOV, 3GP).
fn conteiner_mp4(nome: &str) -> bool {
    nome.split(',').any(|n| n == "mp4" || n == "mov")
}

/// Contêiner e primeiro quadro em zero: o tempo do elemento é o da fonte, sem trecho vazio no começo (M1).
fn comeca_em_zero(f: &FonteInfo) -> bool {
    f.inicio_s == 0.0 && f.quadros.pts.first() == Some(&0)
}

/// Arquivo de áudio (E2), imagem ou vídeo sem quadro: o `<video>` não tem o que tocar. Sem esta guarda viraria uma
/// prévia pendente para sempre (`tabela_pendente` a tela lê como "esperar").
fn sem_video(f: &FonteInfo) -> Result<()> {
    if f.codec.is_empty() || f.exibido.0 == 0 || f.exibido.1 == 0 {
        return Err(SicroError::Validation("nao_suportado: fonte sem vídeo".into()));
    }
    // imagem parada (E2; mesma regra de `quadros`): o palco a desenha pelo sprite, nunca por `<video>`
    if f.conteiner == "image2" || f.conteiner.ends_with("_pipe") {
        return Err(SicroError::Validation("nao_suportado: imagem".into()));
    }
    // a tabela (de pacotes ou do decodificador) vazia não enche depois
    if f.quadros.pts.is_empty() {
        return Err(SicroError::Validation("fonte_danificada: nenhum quadro".into()));
    }
    Ok(())
}

/// A prévia que existe agora, sem fazer nada: `pronta` false quando a cópia ou o proxy ainda não está no cache.
/// `ws` = raiz do caso; `cache` = pasta da Edição do caso. Fonte sem vídeo: `Err` (`nao_suportado`).
pub fn consultar(ws: &Path, cache: &Path, fonte: &FonteInfo) -> Result<PreviaMidia> {
    sem_video(fonte)?;
    match decidir(fonte) {
        TipoPrevia::Original => original(ws, fonte),
        TipoPrevia::Copia => {
            // o proxy feito quando o remux falhou também serve
            if let Some(p) = pronta(cache, TipoPrevia::Copia, fonte)? {
                return Ok(p);
            }
            Ok(pronta(cache, TipoPrevia::Proxy, fonte)?.unwrap_or(pendente(TipoPrevia::Copia, fonte)?))
        }
        _ => Ok(pronta(cache, TipoPrevia::Proxy, fonte)?.unwrap_or(pendente(TipoPrevia::Proxy, fonte)?)),
    }
}

/// Cópia por remux em `previa/<sha256>.mp4` + `.json`, com o deslocamento medido. `progresso` recebe 0..1.
pub fn copiar(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, cancelar: &AtomicBool, progresso: &mut dyn FnMut(f32)) -> Result<PreviaMidia> {
    if !codec_serve(fonte) {
        return Err(SicroError::Validation(format!("nao_suportado: cópia de {} {}", fonte.codec, fonte.pix_fmt)));
    }
    fazer(cap, cache, fonte, TipoPrevia::Copia, None, cancelar, progresso)
}

/// Proxy de CPU em `proxies/<sha256>.720.mp4` + `.json`. `progresso` recebe 0..1.
pub fn proxy_cpu(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, cancelar: &AtomicBool, progresso: &mut dyn FnMut(f32)) -> Result<PreviaMidia> {
    let motivo = (!codec_serve(fonte)).then(|| format!("codec: {} {}", fonte.codec, fonte.pix_fmt));
    fazer(cap, cache, fonte, TipoPrevia::Proxy, motivo, cancelar, progresso)
}

/// Faz o que `decidir` manda; se o remux falhar, vai o proxy. Trabalho da fila de fundo.
/// `Original` não tem o que fazer (a resposta, relativa ao caso, sai de `consultar`): `Err`.
pub fn produzir(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, cancelar: &AtomicBool, progresso: &mut dyn FnMut(f32)) -> Result<PreviaMidia> {
    let tipo = decidir(fonte);
    if tipo == TipoPrevia::Copia && pronta(cache, TipoPrevia::Copia, fonte)?.is_none() {
        if let Some(p) = pronta(cache, TipoPrevia::Proxy, fonte)? {
            progresso(1.0);
            return Ok(p);
        }
    }
    produzir_com(
        tipo,
        cancelar,
        |p| copiar(cap, cache, fonte, cancelar, p),
        |motivo, p| match motivo {
            Some(m) => fazer(cap, cache, fonte, TipoPrevia::Proxy, Some(m), cancelar, p),
            None => proxy_cpu(cap, cache, fonte, cancelar, p),
        },
        progresso,
    )
}

/// Regra da queda: cópia que falha (menos por cancelamento) vira proxy, com o motivo no `.json`.
fn produzir_com(
    tipo: TipoPrevia,
    cancelar: &AtomicBool,
    copia: impl FnOnce(&mut dyn FnMut(f32)) -> Result<PreviaMidia>,
    proxy: impl FnOnce(Option<String>, &mut dyn FnMut(f32)) -> Result<PreviaMidia>,
    progresso: &mut dyn FnMut(f32),
) -> Result<PreviaMidia> {
    match tipo {
        TipoPrevia::Copia => match copia(progresso) {
            Ok(p) => Ok(p),
            Err(e) if cancelado(&e) || cancelar.load(Ordering::SeqCst) => Err(e),
            Err(e) => proxy(Some(format!("remux: {e}")), progresso),
        },
        TipoPrevia::Proxy => proxy(None, progresso),
        TipoPrevia::Original | TipoPrevia::Origem => Err(SicroError::Validation("original: nada a produzir".into())),
    }
}

fn cancelado(e: &SicroError) -> bool {
    matches!(e, SicroError::Validation(m) if m.starts_with(ERRO_CANCELADO))
}

/// `MidiaInfo` de uma fonte já preparada. `id` e `nome` vêm do registro do caso.
pub fn midia_info(id: &str, nome: &str, fonte: &FonteInfo, previa: PreviaMidia) -> MidiaInfo {
    midia_info_com(id, nome, fonte, previa, fonte.quadros.taxa_nominal())
}

fn midia_info_com(id: &str, nome: &str, fonte: &FonteInfo, previa: PreviaMidia, taxa: Racional) -> MidiaInfo {
    let t = &fonte.quadros;
    let (de_s, ate_s) = faixa(t);
    MidiaInfo {
        id: id.to_string(),
        sha256: fonte.sha256.clone(),
        nome: nome.to_string(),
        exibido: TamanhoPx { w: fonte.exibido.0, h: fonte.exibido.1 },
        taxa,
        vfr: t.vfr,
        passo: PassoS { min: segundos(t.passo.0, t.tb), max: segundos(t.passo.1, t.tb) },
        de_s,
        ate_s,
        n: t.pts.len() as u32,
        tem_audio: fonte.audio.is_some(),
        previa,
        tabela: t.origem,
        erros: (t.origem == OrigemTabela::Decodificador).then_some(t.erros_decodificacao),
        marcavel: marcavel(fonte),
    }
}

/// A regra única do (g)14: só a tabela de pacotes de contêiner sem tempo nos pacotes bloqueia a marcação.
pub(crate) fn marcavel(fonte: &FonteInfo) -> bool {
    !(fonte.quadros.origem == OrigemTabela::Pacotes && super::executar::sem_tempo_nos_pacotes(&fonte.conteiner))
}

// ---------------------------------------------------------------- cache

/// O que fica ao lado do arquivo tocado. `motivo`: por que é proxy (codec, ou o erro do remux).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Registro {
    versao: u32,
    tipo: TipoPrevia,
    relativo: String,
    desloc_s: f64,
    de_s: f64,
    ate_s: f64,
    largura: u32,
    altura: u32,
    tamanho: u64,
    quadros: u64,
    fonte_sha256: String,
    ffmpeg_sha256: String,
    ffmpeg_versao: String,
    argumentos: Vec<String>,
    motivo: Option<String>,
    feito_em: String,
}

struct Destino {
    pasta: PathBuf,
    midia: PathBuf,
    registro: PathBuf,
    relativo: String,
}

fn destino(cache: &Path, tipo: TipoPrevia, sha: &str) -> Result<Destino> {
    if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(SicroError::Validation(format!("item_invalido: sha256 {sha}")));
    }
    let (sub, base) = match tipo {
        TipoPrevia::Copia => (PASTA_COPIA, sha.to_string()),
        TipoPrevia::Proxy => (PASTA_PROXY, format!("{sha}.{PROXY_ALTURA}")),
        _ => return Err(SicroError::Validation(format!("item_invalido: prévia {tipo:?} sem cache"))),
    };
    let pasta = cache.join(sub);
    Ok(Destino {
        midia: pasta.join(format!("{base}.mp4")),
        registro: pasta.join(format!("{base}.json")),
        relativo: format!("{sub}/{base}.mp4"),
        pasta,
    })
}

/// A prévia do cache, se o `.json` confere com a fonte e o arquivo tem o tamanho gravado. Registro ilegível = não pronta.
fn pronta(cache: &Path, tipo: TipoPrevia, fonte: &FonteInfo) -> Result<Option<PreviaMidia>> {
    let d = destino(cache, tipo, &fonte.sha256)?;
    let Ok(texto) = std::fs::read(&d.registro) else { return Ok(None) };
    let Ok(r) = serde_json::from_slice::<Registro>(&texto) else { return Ok(None) };
    let tamanho = std::fs::metadata(&d.midia).map(|m| m.len()).ok();
    if r.versao != VERSAO_REGISTRO || r.tipo != tipo || r.fonte_sha256 != fonte.sha256 || tamanho != Some(r.tamanho) {
        return Ok(None);
    }
    let (de_s, ate_s) = faixa(&fonte.quadros);
    Ok(Some(PreviaMidia { relativo: d.relativo, tipo, desloc_s: r.desloc_s, de_s, ate_s, largura: r.largura, altura: r.altura, pronta: true }))
}

fn pendente(tipo: TipoPrevia, fonte: &FonteInfo) -> Result<PreviaMidia> {
    let d = destino(Path::new(""), tipo, &fonte.sha256)?;
    let (de_s, ate_s) = faixa(&fonte.quadros);
    let (largura, altura) = if tipo == TipoPrevia::Proxy { tamanho_proxy(fonte.exibido) } else { fonte.exibido };
    Ok(PreviaMidia { relativo: d.relativo, tipo, desloc_s: 0.0, de_s, ate_s, largura, altura, pronta: false })
}

fn original(ws: &Path, fonte: &FonteInfo) -> Result<PreviaMidia> {
    let rel = fonte
        .caminho
        .strip_prefix(ws)
        .map(Path::to_path_buf)
        .or_else(|_| {
            let (c, w) = (fonte.caminho.canonicalize()?, ws.canonicalize()?);
            c.strip_prefix(&w).map(Path::to_path_buf).map_err(|_| std::io::Error::other("fora"))
        })
        .map_err(|_| SicroError::Validation(format!("midia_ausente: {} fora do caso", fonte.caminho.display())))?;
    let relativo = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
    let (de_s, ate_s) = faixa(&fonte.quadros);
    Ok(PreviaMidia {
        relativo,
        tipo: TipoPrevia::Original,
        desloc_s: 0.0,
        de_s,
        ate_s,
        largura: fonte.exibido.0,
        altura: fonte.exibido.1,
        pronta: true,
    })
}

/// Pasta de trabalho ao lado do destino (mesmo disco: o `rename` é atômico), apagada no fim, com sucesso ou não.
struct Temporaria(PathBuf);

impl Temporaria {
    fn nova(pasta: &Path, sha: &str) -> Result<Self> {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = pasta.join(format!("{sha}.tmp-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p)?;
        Ok(Temporaria(p))
    }
}

impl Drop for Temporaria {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------- produção

fn fazer(
    cap: &Capacidades,
    cache: &Path,
    fonte: &FonteInfo,
    tipo: TipoPrevia,
    motivo: Option<String>,
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(f32),
) -> Result<PreviaMidia> {
    sem_video(fonte)?;
    let tab = &fonte.quadros;
    let d = destino(cache, tipo, &fonte.sha256)?;
    if let Some(p) = pronta(cache, tipo, fonte)? {
        progresso(1.0);
        return Ok(p);
    }
    if cancelar.load(Ordering::SeqCst) {
        return Err(SicroError::Validation(format!("{ERRO_CANCELADO}: prévia")));
    }
    let (largura, altura) = match tipo {
        TipoPrevia::Proxy => tamanho_proxy(fonte.exibido),
        _ => fonte.exibido,
    };
    if largura == 0 || altura == 0 {
        return Err(SicroError::Validation("item_invalido: tamanho exibido".into()));
    }
    std::fs::create_dir_all(&d.pasta)?;
    let tmp = Temporaria::nova(&d.pasta, &fonte.sha256)?;
    if tipo == TipoPrevia::Copia && conteiner_mp4(&fonte.conteiner) {
        let n = escondidos(cap, &fonte.caminho, &tmp.0, cancelar)?;
        if n > 0 {
            return Err(SicroError::Validation(format!("{ERRO_LEITURA_DIVERGENTE}: {n} pacotes escondidos pela lista de edição")));
        }
    }
    let audio = codec_do_audio(cap, &fonte.caminho, &tmp.0, cancelar)?;
    let args = match tipo {
        TipoPrevia::Proxy => args_proxy(&fonte.caminho, audio.as_deref(), tab.tb, (largura, altura)),
        _ => args_copia(&fonte.caminho, audio.as_deref()),
    };
    let cmd = comando(&cap.binario, &tmp.0, "previa", args);
    let (de_s, ate_s) = faixa(tab);
    let dur = (ate_s - de_s).max(1e-3);
    let t = exec::rodar_com_progresso(&cmd, cancelar, &mut |p| {
        if let Some(us) = p.tempo_us {
            progresso(((us as f64 / 1e6 / dur) as f32).clamp(0.0, 1.0) * PARTE_FFMPEG);
        }
    })?;
    exec::exigir_sucesso(&cmd, &t)?;

    let saida = tmp.0.join(SAIDA);
    let desloc_s = deslocamento(cap, &fonte.caminho, &saida, &tmp.0, tipo == TipoPrevia::Copia, cancelar)?;
    let (tb, pts) = pacotes(cap, &saida, &tmp.0, cancelar)?;
    conferir(tipo, tab, desloc_s, tb, &pts)?;

    let tamanho = std::fs::metadata(&saida)?.len();
    let r = Registro {
        versao: VERSAO_REGISTRO,
        tipo,
        relativo: d.relativo.clone(),
        desloc_s,
        de_s,
        ate_s,
        largura,
        altura,
        tamanho,
        quadros: pts.len() as u64,
        fonte_sha256: fonte.sha256.clone(),
        ffmpeg_sha256: cap.sha256.clone(),
        ffmpeg_versao: cap.versao.clone(),
        argumentos: cmd.args.clone(),
        motivo,
        feito_em: chrono::Utc::now().to_rfc3339(),
    };
    // outra chamada para a mesma mídia terminou antes: vale a dela (no Windows ela pode estar aberta no `<video>`)
    if let Some(p) = pronta(cache, tipo, fonte)? {
        progresso(1.0);
        return Ok(p);
    }
    duravel(&saida)?;
    std::fs::rename(&saida, &d.midia)?;
    let reg = tmp.0.join("registro.json");
    std::fs::write(&reg, serde_json::to_vec_pretty(&r)?)?;
    duravel(&reg)?;
    std::fs::rename(&reg, &d.registro)?;
    progresso(1.0);
    Ok(PreviaMidia { relativo: d.relativo, tipo, desloc_s, de_s, ate_s, largura, altura, pronta: true })
}

/// Nome do arquivo de saída dentro da pasta de trabalho.
const SAIDA: &str = "saida.mp4";

/// No disco antes do `rename`: queda de energia não deixa registro valendo para arquivo com tamanho certo e
/// conteúdo faltando (NTFS). Escrita: o `FlushFileBuffers` do Windows recusa arquivo aberto só para leitura.
fn duravel(p: &Path) -> Result<()> {
    std::fs::OpenOptions::new().write(true).open(p)?.sync_all()?;
    Ok(())
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn args_audio(audio: Option<&str>) -> Vec<String> {
    match audio {
        None => s(&["-an"]),
        Some(c) if AUDIO_COPIA.contains(&c) => s(&["-map", "0:a:0", "-c:a", "copy"]),
        // G.711 dos gravadores e o resto: AAC 48 kHz estéreo
        Some(_) => s(&["-map", "0:a:0", "-c:a", "aac", "-b:a", "128k", "-ac", "2", "-ar", "48000"]),
    }
}

/// Remux do §7.1: vídeo copiado (com os quadros antes do primeiro quadro-chave), início em zero.
fn args_copia(fonte: &Path, audio: Option<&str>) -> Vec<String> {
    let mut a = s(&["-hide_banner", "-nostdin", "-y", "-nostats", "-loglevel", "error", "-i"]);
    a.push(fonte.to_string_lossy().into_owned());
    a.extend(s(&["-map", "0:v:0", "-c:v", "copy", "-copyinkf"]));
    a.extend(args_audio(audio));
    a.extend(s(&["-avoid_negative_ts", "make_zero", "-movflags", "+faststart", SAIDA]));
    a
}

/// Receita de CPU do §7.6 (O8): pts da fonte passados adiante na mesma base; tamanho exibido explícito com pixel quadrado
/// (o `scale=-2:720` da receita deixaria pixel não quadrado, que os dois motores medem diferente, M1).
fn args_proxy(fonte: &Path, audio: Option<&str>, tb: Racional, tamanho: (u32, u32)) -> Vec<String> {
    let (w, h) = tamanho;
    let mut a = s(&["-hide_banner", "-nostdin", "-y", "-nostats", "-loglevel", "error", "-i"]);
    a.push(fonte.to_string_lossy().into_owned());
    a.extend(s(&["-map", "0:v:0", "-vf"]));
    a.push(format!("scale={w}:{h}:flags=bicubic,setsar=1,format=yuv420p"));
    a.extend(s(&["-c:v", "libx264", "-preset", "veryfast", "-crf"]));
    a.push(PROXY_CRF.to_string());
    a.extend(s(&["-profile:v", "high", "-g"]));
    a.push(PROXY_GOP.to_string());
    a.extend(s(&["-bf", "0", "-sc_threshold", "0", "-fps_mode", "passthrough", "-enc_time_base", "demux", "-video_track_timescale"]));
    a.push(tb.den.to_string());
    a.extend(args_audio(audio));
    a.extend(s(&["-movflags", "+faststart", SAIDA]));
    a
}

/// Tamanho do proxy: altura até `PROXY_ALTURA` (sem ampliar), proporção do exibido, lados pares.
fn tamanho_proxy(exibido: (u32, u32)) -> (u32, u32) {
    let (w, h) = exibido;
    if w == 0 || h == 0 {
        return (0, 0);
    }
    let alt = (h.min(PROXY_ALTURA) / 2 * 2).max(2);
    let larg = ((w as f64 * alt as f64 / h as f64 / 2.0).round() as u32 * 2).max(2);
    (larg, alt)
}

fn comando(programa: &Path, pasta: &Path, nome: &str, args: Vec<String>) -> Comando {
    Comando {
        nome: nome.into(),
        programa: programa.to_path_buf(),
        args,
        pasta: pasta.to_path_buf(),
        ambiente: Vec::new(),
        sem_ambiente: Vec::new(),
        stderr: pasta.join(format!("{nome}.stderr")),
    }
}

/// `codec_name` da primeira trilha de áudio; None = sem áudio.
fn codec_do_audio(cap: &Capacidades, fonte: &Path, pasta: &Path, cancelar: &AtomicBool) -> Result<Option<String>> {
    let mut a = s(&["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=codec_name", "-of", "csv=p=0"]);
    a.push(fonte.to_string_lossy().into_owned());
    let cmd = comando(&cap.ffprobe, pasta, "audio", a);
    let (t, out) = exec::rodar_com_saida(&cmd, cancelar)?;
    exec::exigir_sucesso(&cmd, &t)?;
    // TS repete a trilha por programa
    Ok(String::from_utf8_lossy(&out).lines().map(|l| l.trim().trim_end_matches(',')).find(|l| !l.is_empty()).map(String::from))
}

/// Pacotes de vídeo marcados `D` (escondidos pela lista de edição do MP4/MOV). Todo remux os mostra.
fn escondidos(cap: &Capacidades, fonte: &Path, pasta: &Path, cancelar: &AtomicBool) -> Result<usize> {
    let mut a = s(&["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=flags", "-of", "csv=p=0"]);
    a.push(fonte.to_string_lossy().into_owned());
    let cmd = comando(&cap.ffprobe, pasta, "escondidos", a);
    let (t, out) = exec::rodar_com_saida(&cmd, cancelar)?;
    exec::exigir_sucesso(&cmd, &t)?;
    Ok(String::from_utf8_lossy(&out).lines().filter(|l| l.contains('D')).count())
}

/// pts0 da fonte − pts0 do arquivo tocado (s), pelo primeiro quadro que o decodificador entrega em cada um.
/// `mesmo_quadro` (cópia): o primeiro quadro tem de ser o mesmo nos dois; sem isso a conta mede o par errado.
fn deslocamento(cap: &Capacidades, fonte: &Path, tocado: &Path, pasta: &Path, mesmo_quadro: bool, cancelar: &AtomicBool) -> Result<f64> {
    let (pf, tf, cf) = primeiro_quadro(cap, fonte, pasta, "primeiro-fonte", cancelar)?;
    let (pt, tt, ct) = primeiro_quadro(cap, tocado, pasta, "primeiro-tocado", cancelar)?;
    if mesmo_quadro && cf != ct {
        return Err(SicroError::Validation(format!("{ERRO_LEITURA_DIVERGENTE}: primeiro quadro da cópia ({ct}) ≠ da fonte ({cf})")));
    }
    // diferença exata em racional antes de virar f64 (fonte de 12 h: 43 200 s)
    let num = pf as i128 * tf.num as i128 * tt.den as i128 - pt as i128 * tt.num as i128 * tf.den as i128;
    Ok(num as f64 / (tf.den as i128 * tt.den as i128) as f64)
}

/// pts, base de tempo e checksum (`showinfo`) do primeiro quadro decodificado (`-copyts`).
fn primeiro_quadro(cap: &Capacidades, video: &Path, pasta: &Path, nome: &str, cancelar: &AtomicBool) -> Result<(i64, Racional, String)> {
    let mut a = s(&["-hide_banner", "-nostdin", "-nostats", "-loglevel", "verbose", "-copyts", "-i"]);
    a.push(video.to_string_lossy().into_owned());
    a.extend(s(&["-map", "0:v:0", "-frames:v", "1", "-vf", "showinfo@p", "-f", "null", "-"]));
    let cmd = comando(&cap.binario, pasta, nome, a);
    let t = exec::rodar(&cmd, cancelar)?;
    exec::exigir_sucesso(&cmd, &t)?;
    let sem = || SicroError::Validation(format!("{ERRO_LEITURA_DIVERGENTE}: nenhum quadro em {}", video.display()));
    let pts = exec::pts_lidos(&cmd.stderr)?.remove("p").and_then(|v| v.first().copied()).filter(|&p| p != i64::MIN).ok_or_else(sem)?;
    let texto = String::from_utf8_lossy(&std::fs::read(&cmd.stderr)?).into_owned();
    let linhas = || texto.lines().filter(|l| l.contains("[showinfo@p "));
    let tb = linhas().find_map(|l| l.split("config in time_base: ").nth(1)).and_then(|r| racional(r.split(',').next()?.trim())).ok_or_else(sem)?;
    let soma = linhas()
        .find(|l| l.contains("] n:"))
        .and_then(|l| l.split(" checksum:").nth(1))
        .and_then(|r| r.split_whitespace().next())
        .map(String::from)
        .ok_or_else(sem)?;
    Ok((pts, tb, soma))
}

/// Base de tempo e pts (dts quando falta) de cada pacote de vídeo do arquivo tocado, em ordem crescente.
fn pacotes(cap: &Capacidades, video: &Path, pasta: &Path, cancelar: &AtomicBool) -> Result<(Racional, Vec<i64>)> {
    let mut a = s(&["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=time_base:packet=pts,dts", "-of", "csv"]);
    a.push(video.to_string_lossy().into_owned());
    let cmd = comando(&cap.ffprobe, pasta, "pacotes", a);
    let (t, out) = exec::rodar_com_saida(&cmd, cancelar)?;
    exec::exigir_sucesso(&cmd, &t)?;
    let (mut tb, mut pts) = (None, Vec::new());
    for l in String::from_utf8_lossy(&out).lines() {
        let mut c = l.trim().trim_end_matches(',').split(',');
        match c.next() {
            Some("packet") => {
                let (p, d) = (c.next().and_then(|v| v.parse().ok()), c.next().and_then(|v| v.parse().ok()));
                if let Some(v) = p.or(d) {
                    pts.push(v);
                }
            }
            Some("stream") if tb.is_none() => tb = c.next().and_then(racional),
            _ => {}
        }
    }
    let tb = tb.ok_or_else(|| SicroError::Validation(format!("{ERRO_LEITURA_DIVERGENTE}: sem base de tempo em {}", video.display())))?;
    pts.sort_unstable();
    Ok((tb, pts))
}

fn racional(t: &str) -> Option<Racional> {
    let (n, d) = t.split_once('/')?;
    let r = Racional { num: n.trim().parse().ok()?, den: d.trim().parse().ok()? };
    (r.num > 0 && r.den > 0).then_some(r)
}

/// Confere o arquivo tocado com a tabela da fonte, quadro a quadro, com o deslocamento medido:
/// cópia = todo quadro da tabela está nela (pode ter a mais os quadros antes do primeiro quadro-chave);
/// proxy = todo quadro dele está na tabela e, com a tabela do decodificador, são os mesmos.
/// pts repetido na fonte sai 1 passo adiante no MP4 (o muxer não aceita dois iguais): folga de 1 passo por repetição.
fn conferir(tipo: TipoPrevia, tab: &TabelaQuadros, desloc_s: f64, tb: Racional, pts: &[i64]) -> Result<()> {
    let esperado: Vec<f64> = tab.pts.iter().map(|&p| segundos(p, tab.tb) - desloc_s).collect();
    let tocado: Vec<f64> = pts.iter().map(|&p| segundos(p, tb)).collect();
    let passo = tb.num as f64 / tb.den as f64;
    let mut repetido = vec![0u32; tab.pts.len()];
    for j in 1..tab.pts.len() {
        if tab.pts[j] <= tab.pts[j - 1] {
            repetido[j] = repetido[j - 1] + 1;
        }
    }
    let folga = |j: usize| TOLERANCIA_S + repetido[j] as f64 * passo;
    let erro = |o: &str| SicroError::Validation(format!("{ERRO_LEITURA_DIVERGENTE}: {o}"));
    let mut k = 0;
    match tipo {
        TipoPrevia::Proxy => {
            for (i, &t) in tocado.iter().enumerate() {
                while k < esperado.len() && esperado[k] < t - folga(k) {
                    k += 1;
                }
                if k >= esperado.len() || (esperado[k] - t).abs() > folga(k) {
                    return Err(erro(&format!("quadro {i} do proxy em {t:.6} s sem par na fonte")));
                }
                k += 1;
            }
            if tab.origem == OrigemTabela::Decodificador && tocado.len() != esperado.len() {
                return Err(erro(&format!("proxy com {} quadros, fonte com {}", tocado.len(), esperado.len())));
            }
        }
        _ => {
            for (j, &e) in esperado.iter().enumerate() {
                while k < tocado.len() && tocado[k] < e - folga(j) {
                    k += 1;
                }
                if k >= tocado.len() || (tocado[k] - e).abs() > folga(j) {
                    return Err(erro(&format!("quadro {j} da fonte sem par na cópia ({e:.6} s)")));
                }
                k += 1;
            }
        }
    }
    Ok(())
}

fn segundos(pts: i64, tb: Racional) -> f64 {
    (pts as i128 * tb.num as i128) as f64 / tb.den as f64
}

/// Faixa em tempo de fonte: do primeiro quadro ao fim do último (passo mediano, §3.1).
fn faixa(t: &TabelaQuadros) -> (f64, f64) {
    match (t.pts.first(), t.pts.last()) {
        (Some(&a), Some(&b)) => (segundos(a, t.tb), segundos(b + t.passo_mediano.max(0), t.tb)),
        _ => (0.0, 0.0),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::Instant;

    use serde_json::Value;

    use crate::edicao::ffmpeg::sonda;
    use crate::edicao::geometria;
    use crate::edicao::modelo::{Faixa, Matriz};
    use crate::edicao::motor::AudioInfo;
    use crate::edicao::quadros::{Busca, Declarado};
    use crate::edicao::testes::fontes::{self, Fonte};
    use crate::edicao::testes::medir;

    // ------------------------------------------------------------ ajudantes só de teste

    fn rodar(cap: &Capacidades, programa: &Path, pasta: &Path, nome: &str, args: Vec<String>) -> Vec<u8> {
        let cmd = comando(programa, pasta, nome, args);
        let (t, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).unwrap();
        exec::exigir_sucesso(&cmd, &t).unwrap_or_else(|e| panic!("{e} ({})", cap.versao));
        out
    }

    fn sonda_json(cap: &Capacidades, video: &Path) -> Value {
        let p = fontes::pasta_temporaria("previa-sonda");
        let mut a = s(&["-v", "error", "-show_format", "-show_streams", "-of", "json"]);
        a.push(video.to_string_lossy().into_owned());
        serde_json::from_slice(&rodar(cap, &cap.ffprobe, &p, "sonda", a)).unwrap()
    }

    fn fluxo<'a>(j: &'a Value, tipo: &str) -> Option<&'a Value> {
        j["streams"].as_array()?.iter().find(|s| s["codec_type"] == tipo)
    }

    fn num(v: &Value) -> Option<f64> {
        v.as_str().and_then(|t| t.parse().ok()).or_else(|| v.as_f64())
    }

    /// Tabela à mão: pts do decodificador (`medir::pts`) ou dos pacotes (ffprobe, sem decodificar; arquivo grande).
    fn tabela(cap: &Capacidades, video: &Path, tb: Racional, decodificar: bool) -> TabelaQuadros {
        let pts = if decodificar {
            medir::pts(cap, video).unwrap()
        } else {
            let p = fontes::pasta_temporaria("previa-pacotes");
            pacotes(cap, video, &p, &AtomicBool::new(false)).unwrap().1
        };
        let passos: Vec<i64> = pts.windows(2).map(|w| w[1] - w[0]).collect();
        let mut ord = passos.clone();
        ord.sort_unstable();
        let mediano = ord.get(ord.len() / 2).copied().unwrap_or(1);
        let passo = (ord.first().copied().unwrap_or(mediano), ord.last().copied().unwrap_or(mediano));
        TabelaQuadros {
            tb,
            chave: (0..pts.len()).map(|i| i == 0).collect(),
            passo_mediano: mediano,
            passo,
            vfr: passo.0 != passo.1,
            origem: if decodificar { OrigemTabela::Decodificador } else { OrigemTabela::Pacotes },
            ffmpeg_sha256: cap.sha256.clone(),
            erros_decodificacao: 0,
            busca: Busca::Confiavel,
            declarado: Declarado { r_frame_rate: String::new(), avg_frame_rate: String::new(), nb_frames: None, duracao: None },
            pacotes: pts.len() as u64,
            lacunas: Vec::new(),
            pts,
        }
    }

    /// `FonteInfo` montado do ffprobe (`quadros::fonte_info` é do P0.3b; giro horário = −rotation).
    fn fonte_de_teste(cap: &Capacidades, video: &Path, decodificar: bool) -> FonteInfo {
        let j = sonda_json(cap, video);
        let v = fluxo(&j, "video").expect("sem vídeo");
        let tb = racional(v["time_base"].as_str().unwrap()).unwrap();
        let sar = v["sample_aspect_ratio"]
            .as_str()
            .and_then(|t| t.split_once(':'))
            .and_then(|(a, b)| Some(Racional { num: a.parse().ok()?, den: b.parse().ok()? }))
            .filter(|r| r.num > 0 && r.den > 0)
            .unwrap_or(Racional { num: 1, den: 1 });
        let rotacao = v["side_data_list"]
            .as_array()
            .and_then(|l| l.iter().find_map(|d| d["rotation"].as_f64()))
            .unwrap_or(0.0) as i32;
        let giro = (-rotacao).rem_euclid(360);
        let (w, h) = (v["width"].as_u64().unwrap() as u32, v["height"].as_u64().unwrap() as u32);
        let exibido = geometria::exibido(TamanhoPx { w, h }, sar, giro);
        let pix_fmt = v["pix_fmt"].as_str().unwrap_or("").to_string();
        let faixa = if pix_fmt.starts_with("yuvj") || v["color_range"] == "pc" { Faixa::Pc } else { Faixa::Tv };
        let matriz = match v["color_space"].as_str() {
            Some("bt709") => Matriz::Bt709,
            Some("bt470bg" | "smpte170m") => Matriz::Bt601,
            _ => crate::edicao::cor::matriz_presumida(exibido.h),
        };
        let audio = fluxo(&j, "audio").map(|a| AudioInfo {
            inicio_s: num(&a["start_time"]).unwrap_or(0.0),
            taxa: num(&a["sample_rate"]).unwrap_or(0.0) as u32,
            canais: a["channels"].as_u64().unwrap_or(0) as u32,
        });
        FonteInfo {
            caminho: video.to_path_buf(),
            sha256: sonda::sha256_arquivo(video).unwrap(),
            inicio_s: num(&j["format"]["start_time"]).unwrap_or(0.0),
            exibido: (exibido.w, exibido.h),
            sar,
            giro,
            entrelacado: !matches!(v["field_order"].as_str(), None | Some("progressive" | "unknown")),
            pix_fmt,
            matriz,
            faixa,
            quadros: Arc::new(tabela(cap, video, tb, decodificar)),
            audio,
            conteiner: j["format"]["format_name"].as_str().unwrap_or("").to_string(),
            codec: v["codec_name"].as_str().unwrap_or("").to_string(),
        }
    }

    /// Fontes que a matriz não tem, feitas uma vez por processo (o binário é um só por processo).
    fn extra(cap: &Capacidades, nome: &str, args: &[&str]) -> PathBuf {
        static PASTA: OnceLock<PathBuf> = OnceLock::new();
        static TRAVA: Mutex<()> = Mutex::new(());
        let pasta = PASTA.get_or_init(|| fontes::pasta_temporaria("previa-fontes"));
        let _g = TRAVA.lock().unwrap_or_else(|e| e.into_inner());
        let destino = pasta.join(nome);
        if !destino.exists() {
            let mut a = s(&["-hide_banner", "-nostdin", "-y", "-loglevel", "error"]);
            a.extend(s(args));
            a.push(format!("tmp-{nome}"));
            rodar(cap, &cap.binario, pasta, nome, a);
            std::fs::rename(pasta.join(format!("tmp-{nome}")), &destino).unwrap();
        }
        destino
    }

    fn mp4_matriz() -> String {
        fontes::gerar(Fonte::Mp4).unwrap().to_string_lossy().into_owned()
    }

    /// MP4 da matriz com o vídeo começando em 3 s (lista de edição com espera, como `ce/wk/media/r3.mp4`).
    fn mp4_inicio_3s(cap: &Capacidades) -> PathBuf {
        let m = mp4_matriz();
        extra(cap, "mp4-inicio-3s.mp4", &["-i", &m, "-map", "0", "-c", "copy", "-output_ts_offset", "3"])
    }

    fn hevc(cap: &Capacidades) -> PathBuf {
        let m = mp4_matriz();
        extra(
            cap,
            "hevc.mp4",
            &["-i", &m, "-map", "0:v", "-t", "5", "-c:v", "libx265", "-preset", "ultrafast", "-crf", "12", "-x265-params", "log-level=error", "-pix_fmt", "yuv420p", "-tag:v", "hvc1"],
        )
    }

    fn dez_bits(cap: &Capacidades) -> PathBuf {
        let m = mp4_matriz();
        extra(
            cap,
            "h264-10bits.mp4",
            &["-i", &m, "-map", "0:v", "-map", "0:a", "-t", "5", "-c:v", "libx264", "-preset", "veryfast", "-crf", "10", "-pix_fmt", "yuv420p10le", "-g", "30", "-c:a", "copy"],
        )
    }

    /// 1080p de 45 s: o proxy reduz e dura o bastante para cancelar no meio.
    fn grande(cap: &Capacidades) -> PathBuf {
        extra(
            cap,
            "grande-1080.mp4",
            &[
                "-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=30:duration=45", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=45",
                "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "ultrafast", "-crf", "30", "-g", "60", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "64k",
            ],
        )
    }

    fn sem_progresso() -> impl FnMut(f32) {
        |_| {}
    }

    /// Base de tempo do vídeo do arquivo (a dos pts que `medir::pts` lê).
    fn tb_do_video(cap: &Capacidades, video: &Path) -> Racional {
        racional(fluxo(&sonda_json(cap, video), "video").unwrap()["time_base"].as_str().unwrap()).unwrap()
    }

    /// Mede o arquivo tocado contra a fonte pelo decodificador: mesmo número de quadros; deslocamento = `desloc_s`
    /// no primeiro, no do meio e no último a até 1 µs; nos demais, a até 1 µs salvo pts repetido na fonte.
    fn conferir_pts(cap: &Capacidades, fonte: &FonteInfo, tocado: &Path, p: &PreviaMidia) {
        let (pf, pt) = (medir::pts(cap, &fonte.caminho).unwrap(), medir::pts(cap, tocado).unwrap());
        let (tf, tt) = (tb_do_video(cap, &fonte.caminho), tb_do_video(cap, tocado));
        let nome = fonte.caminho.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(pf.len(), pt.len(), "{nome}: quadros da fonte × tocado ({})", cap.versao);
        let d = |i: usize| segundos(pf[i], tf) - segundos(pt[i], tt);
        let n = pf.len();
        let tres = [d(0), d(n / 2), d(n - 1)];
        for (i, v) in tres.iter().enumerate() {
            assert!((v - p.desloc_s).abs() <= TOLERANCIA_S, "{nome}: desloc {} no ponto {i} × {} ({})", v, p.desloc_s, cap.versao);
        }
        let (mn, mx) = tres.iter().fold((f64::MAX, f64::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        assert!(mx - mn <= TOLERANCIA_S, "{nome}: variação {} s ({})", mx - mn, cap.versao);
        let passo = tt.num as f64 / tt.den as f64;
        let mut rep = 0u32;
        for i in 0..n {
            rep = if i > 0 && pf[i] <= pf[i - 1] { rep + 1 } else { 0 };
            let folga = TOLERANCIA_S + rep as f64 * passo;
            assert!((d(i) - p.desloc_s).abs() <= folga, "{nome}: quadro {i} desloc {} × {} ({})", d(i), p.desloc_s, cap.versao);
        }
    }

    fn audio_do(cap: &Capacidades, video: &Path) -> Option<(String, u64)> {
        let j = sonda_json(cap, video);
        fluxo(&j, "audio").map(|a| (a["codec_name"].as_str().unwrap().to_string(), num(&a["sample_rate"]).unwrap() as u64))
    }

    /// SHA-256 de cada quadro decodificado com UMA thread: com várias, o último quadro de arquivo truncado
    /// (`test.lvf`, amostra cortada em 5 MiB) sai diferente em 1 de 10 decodificações do mesmo arquivo, nos dois binários.
    fn hashes_por_quadro(cap: &Capacidades, video: &Path) -> Vec<String> {
        let p = fontes::pasta_temporaria("previa-framehash");
        let mut a = s(&["-hide_banner", "-nostdin", "-loglevel", "error", "-threads", "1", "-i"]);
        a.push(video.to_string_lossy().into_owned());
        a.extend(s(&["-map", "0:v:0", "-fps_mode", "passthrough", "-f", "framehash", "-hash", "sha256", "-"]));
        let h = medir::coluna_de_hash(&String::from_utf8_lossy(&rodar(cap, &cap.binario, &p, "framehash", a)));
        assert!(!h.is_empty(), "{}: nenhum quadro", video.display());
        h
    }

    /// Cópia de uma fonte: tipo, arquivo, pts, quadro a quadro (hash), áudio e a segunda chamada.
    fn conferir_copia(cap: &Capacidades, fonte: &FonteInfo) -> PreviaMidia {
        let nome = fonte.caminho.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(decidir(fonte), TipoPrevia::Copia, "{nome}");
        let cache = fontes::pasta_temporaria("previa-cache");
        let ws = fonte.caminho.parent().unwrap();
        assert!(!consultar(ws, &cache, fonte).unwrap().pronta);
        let mut ultimo = 0.0;
        let p = produzir(cap, &cache, fonte, &AtomicBool::new(false), &mut |f| ultimo = f).unwrap();
        assert_eq!((p.tipo, p.pronta, ultimo), (TipoPrevia::Copia, true, 1.0), "{nome}");
        assert_eq!(p.relativo, format!("previa/{}.mp4", fonte.sha256));
        assert_eq!((p.largura, p.altura), fonte.exibido);
        assert_eq!(consultar(ws, &cache, fonte).unwrap(), p, "{nome}: consultar depois de pronta");
        let tocado = cache.join(&p.relativo);
        eprintln!("{nome}: desloc_s {:.6} ({})", p.desloc_s, cap.versao);
        conferir_pts(cap, fonte, &tocado, &p);
        assert_eq!(hashes_por_quadro(cap, &fonte.caminho), hashes_por_quadro(cap, &tocado), "{nome}: quadros ({})", cap.versao);
        match (audio_do(cap, &fonte.caminho), audio_do(cap, &tocado)) {
            (None, None) => {}
            (Some((c, _)), Some((ct, _))) if AUDIO_COPIA.contains(&c.as_str()) => assert_eq!(ct, c, "{nome}"),
            (Some(_), Some(at)) => assert_eq!(at, ("aac".to_string(), 48000), "{nome}"),
            (a, b) => panic!("{nome}: áudio {a:?} → {b:?}"),
        }
        // o contêiner da cópia leva o giro e o pixel da fonte: o `<video>` mostra o mesmo exibido que `largura`/`altura`
        let c = fonte_de_teste(cap, &tocado, false);
        assert_eq!((c.giro, c.sar, c.exibido), (fonte.giro, fonte.sar, fonte.exibido), "{nome}: geometria ({})", cap.versao);
        // sem resto da pasta de trabalho
        let resto: Vec<_> = std::fs::read_dir(cache.join("previa")).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(resto.len(), 2, "{nome}: {resto:?}");
        p
    }

    fn fonte_falsa(conteiner: &str, codec: &str, pix_fmt: &str, inicio_s: f64, pts0: i64) -> FonteInfo {
        FonteInfo {
            caminho: PathBuf::from("/caso/Mídias/v.mp4"),
            sha256: "ab".repeat(32),
            inicio_s,
            exibido: (1280, 720),
            sar: Racional { num: 1, den: 1 },
            giro: 0,
            entrelacado: false,
            pix_fmt: pix_fmt.into(),
            matriz: Matriz::Bt709,
            faixa: Faixa::Tv,
            quadros: Arc::new(TabelaQuadros {
                tb: Racional { num: 1, den: 15360 },
                pts: vec![pts0, pts0 + 512, pts0 + 1024],
                chave: vec![true, false, false],
                passo_mediano: 512,
                passo: (512, 512),
                vfr: false,
                origem: OrigemTabela::Decodificador,
                ffmpeg_sha256: String::new(),
                erros_decodificacao: 2,
                busca: Busca::Confiavel,
                declarado: Declarado { r_frame_rate: "30/1".into(), avg_frame_rate: "30/1".into(), nb_frames: Some(3), duracao: Some(0.1) },
                pacotes: 3,
                lacunas: Vec::new(),
            }),
            audio: None,
            conteiner: conteiner.into(),
            codec: codec.into(),
        }
    }

    const MP4: &str = "mov,mp4,m4a,3gp,3g2,mj2";
    /// Diferença de cor aceita no proxy (crf 23 e escala), por canal, em 0..255 (medido: até 1,4).
    const COR_PROXY: f64 = 4.0;
    /// Segunda chamada com a prévia pronta (só lê o `.json`): limite real e folga com a máquina dividida.
    const SEGUNDA_CHAMADA_S: f64 = 0.05;
    const FOLGA: f64 = 3.0;

    // ------------------------------------------------------------ sem ffmpeg

    #[test]
    fn decide_pelo_conteiner_pelo_inicio_e_pelo_codec() {
        use TipoPrevia::*;
        let casos = [
            (MP4, "h264", "yuv420p", 0.0, 0, Original),
            (MP4, "h264", "yuvj420p", 0.0, 0, Original),
            (MP4, "h264", "yuv420p", 3.0, 46080, Copia),
            (MP4, "h264", "yuv420p", 0.0, 512, Copia),
            (MP4, "h264", "yuv420p", -0.021, 0, Copia),
            ("matroska,webm", "h264", "yuv420p", 0.0, 0, Copia),
            ("mpegts", "h264", "yuv420p", 1.4, 0, Copia),
            ("avi", "h264", "yuv420p", 0.0, 0, Copia),
            ("mpeg", "h264", "yuv420p", 0.5, 0, Copia),
            (MP4, "hevc", "yuv420p", 0.0, 0, Proxy),
            (MP4, "h264", "yuv420p10le", 0.0, 0, Proxy),
            (MP4, "h264", "yuv422p", 0.0, 0, Proxy),
            ("matroska,webm", "hevc", "yuv420p10le", 3.0, 0, Proxy),
            ("avi", "mpeg4", "yuv420p", 0.0, 0, Proxy),
        ];
        for (c, codec, px, ini, pts0, esperado) in casos {
            assert_eq!(decidir(&fonte_falsa(c, codec, px, ini, pts0)), esperado, "{c} {codec} {px} {ini} {pts0}");
        }
    }

    #[test]
    fn tamanho_do_proxy_sem_ampliar_e_com_lados_pares() {
        assert_eq!(tamanho_proxy((1920, 1080)), (1280, 720));
        assert_eq!(tamanho_proxy((3840, 2160)), (1280, 720));
        assert_eq!(tamanho_proxy((1080, 1920)), (406, 720));
        assert_eq!(tamanho_proxy((320, 180)), (320, 180));
        assert_eq!(tamanho_proxy((654, 480)), (654, 480));
        assert_eq!(tamanho_proxy((784, 576)), (784, 576));
        assert_eq!(tamanho_proxy((0, 0)), (0, 0));
    }

    #[test]
    fn queda_para_o_proxy_so_quando_a_copia_falha() {
        let p = |tipo| PreviaMidia { relativo: String::new(), tipo, desloc_s: 0.0, de_s: 0.0, ate_s: 0.0, largura: 2, altura: 2, pronta: true };
        let nao = AtomicBool::new(false);
        let mut prog = |_: f32| {};
        // cópia boa: não chama o proxy
        let r = produzir_com(TipoPrevia::Copia, &nao, |_| Ok(p(TipoPrevia::Copia)), |_, _| panic!("proxy"), &mut prog).unwrap();
        assert_eq!(r.tipo, TipoPrevia::Copia);
        // remux falhou: proxy com o motivo
        let mut motivo = None;
        let r = produzir_com(
            TipoPrevia::Copia,
            &nao,
            |_| Err(SicroError::Validation("outra: previa (código Some(1)): x".into())),
            |m, _| {
                motivo = m;
                Ok(p(TipoPrevia::Proxy))
            },
            &mut prog,
        )
        .unwrap();
        assert_eq!(r.tipo, TipoPrevia::Proxy);
        assert!(motivo.unwrap().starts_with("remux: "));
        // cancelada: não cai para o proxy
        let e = produzir_com(TipoPrevia::Copia, &nao, |_| Err(SicroError::Validation("cancelado: previa".into())), |_, _| panic!("proxy"), &mut prog).unwrap_err();
        assert!(cancelado(&e));
        let sim = AtomicBool::new(true);
        assert!(produzir_com(TipoPrevia::Copia, &sim, |_| Err(SicroError::Validation("outra: x".into())), |_, _| panic!("proxy"), &mut prog).is_err());
        // codec que não serve: proxy direto, sem motivo de remux
        let r = produzir_com(
            TipoPrevia::Proxy,
            &nao,
            |_| panic!("cópia"),
            |m, _| {
                assert!(m.is_none());
                Ok(p(TipoPrevia::Proxy))
            },
            &mut prog,
        )
        .unwrap();
        assert_eq!(r.tipo, TipoPrevia::Proxy);
        assert!(produzir_com(TipoPrevia::Original, &nao, |_| panic!("cópia"), |_, _| panic!("proxy"), &mut prog).is_err());
    }

    #[test]
    fn registro_so_vale_com_o_arquivo_inteiro() {
        let cache = fontes::pasta_temporaria("previa-registro");
        let f = fonte_falsa("matroska,webm", "h264", "yuv420p", 0.0, 0);
        let pend = consultar(Path::new("/caso"), &cache, &f).unwrap();
        assert_eq!(
            pend,
            PreviaMidia { relativo: format!("previa/{}.mp4", f.sha256), tipo: TipoPrevia::Copia, desloc_s: 0.0, de_s: 0.0, ate_s: 1536.0 / 15360.0, largura: 1280, altura: 720, pronta: false }
        );
        let d = destino(&cache, TipoPrevia::Copia, &f.sha256).unwrap();
        std::fs::create_dir_all(&d.pasta).unwrap();
        std::fs::write(&d.midia, vec![7u8; 1000]).unwrap();
        let r = Registro {
            versao: VERSAO_REGISTRO,
            tipo: TipoPrevia::Copia,
            relativo: d.relativo.clone(),
            desloc_s: -0.021,
            de_s: 0.0,
            ate_s: 0.1,
            largura: 1280,
            altura: 720,
            tamanho: 1000,
            quadros: 3,
            fonte_sha256: f.sha256.clone(),
            ffmpeg_sha256: String::new(),
            ffmpeg_versao: String::new(),
            argumentos: Vec::new(),
            motivo: None,
            feito_em: String::new(),
        };
        let gravar = |r: &Registro| std::fs::write(&d.registro, serde_json::to_vec(r).unwrap()).unwrap();
        gravar(&r);
        let ok = consultar(Path::new("/caso"), &cache, &f).unwrap();
        assert!(ok.pronta && ok.desloc_s == -0.021);
        // arquivo parcial (tamanho diferente), registro de outra fonte, de outra versão, ilegível ou ausente: não pronta
        std::fs::write(&d.midia, vec![7u8; 999]).unwrap();
        assert!(!consultar(Path::new("/caso"), &cache, &f).unwrap().pronta);
        std::fs::write(&d.midia, vec![7u8; 1000]).unwrap();
        gravar(&Registro { fonte_sha256: "cd".repeat(32), ..r.clone() });
        assert!(!consultar(Path::new("/caso"), &cache, &f).unwrap().pronta);
        gravar(&Registro { versao: VERSAO_REGISTRO + 1, ..r.clone() });
        assert!(!consultar(Path::new("/caso"), &cache, &f).unwrap().pronta);
        std::fs::write(&d.registro, b"{").unwrap();
        assert!(!consultar(Path::new("/caso"), &cache, &f).unwrap().pronta);
        std::fs::remove_file(&d.registro).unwrap();
        assert!(!consultar(Path::new("/caso"), &cache, &f).unwrap().pronta);
        // o proxy feito na queda serve a uma fonte de cópia
        let dp = destino(&cache, TipoPrevia::Proxy, &f.sha256).unwrap();
        std::fs::create_dir_all(&dp.pasta).unwrap();
        std::fs::write(&dp.midia, vec![1u8; 10]).unwrap();
        std::fs::write(&dp.registro, serde_json::to_vec(&Registro { tipo: TipoPrevia::Proxy, tamanho: 10, ..r.clone() }).unwrap()).unwrap();
        let px = consultar(Path::new("/caso"), &cache, &f).unwrap();
        assert_eq!((px.tipo, px.pronta, px.relativo.as_str()), (TipoPrevia::Proxy, true, dp.relativo.as_str()));
        // sha que não é hex não vira caminho
        assert!(destino(&cache, TipoPrevia::Copia, "../../x").is_err());
    }

    #[test]
    fn original_relativo_ao_caso() {
        let f = fonte_falsa(MP4, "h264", "yuv420p", 0.0, 0);
        let p = consultar(Path::new("/caso"), Path::new("/cache"), &f).unwrap();
        assert_eq!(
            p,
            PreviaMidia { relativo: "Mídias/v.mp4".into(), tipo: TipoPrevia::Original, desloc_s: 0.0, de_s: 0.0, ate_s: 0.1, largura: 1280, altura: 720, pronta: true }
        );
        assert!(consultar(Path::new("/outro"), Path::new("/cache"), &f).is_err());
    }

    #[test]
    fn midia_info_sai_da_tabela() {
        let mut f = fonte_falsa("matroska,webm", "h264", "yuv420p", 0.0, 1536);
        f.audio = Some(AudioInfo { inicio_s: 0.0, taxa: 48000, canais: 2 });
        let pv = pendente(TipoPrevia::Copia, &f).unwrap();
        let taxa = Racional { num: 30, den: 1 };
        let m = midia_info_com("m1", "v.mkv", &f, pv.clone(), taxa);
        assert_eq!(
            m,
            MidiaInfo {
                id: "m1".into(),
                sha256: f.sha256.clone(),
                nome: "v.mkv".into(),
                exibido: TamanhoPx { w: 1280, h: 720 },
                taxa,
                vfr: false,
                passo: PassoS { min: 512.0 / 15360.0, max: 512.0 / 15360.0 },
                de_s: 0.1,
                ate_s: 0.2,
                n: 3,
                tem_audio: true,
                previa: pv,
                tabela: OrigemTabela::Decodificador,
                erros: Some(2),
                marcavel: true,
            }
        );
    }

    /// Marcação bloqueada só com a tabela de pacotes num contêiner sem tempo nos pacotes.
    #[test]
    fn marcavel_pela_tabela_e_pelo_conteiner() {
        for (conteiner, pacotes, esperado) in [
            ("h264", true, false),
            ("hevc", true, false),
            ("mpeg", true, false),
            ("mpegvideo", true, false),
            ("h264", false, true),
            ("mpeg", false, true),
            ("mpegts", true, true),
            ("mov,mp4,m4a,3gp,3g2,mj2", true, true),
            ("matroska,webm", true, true),
        ] {
            let mut f = fonte_falsa(conteiner, "h264", "yuv420p", 0.0, 0);
            let mut t = (*f.quadros).clone();
            t.origem = if pacotes { OrigemTabela::Pacotes } else { OrigemTabela::Decodificador };
            f.quadros = std::sync::Arc::new(t);
            let pv = pendente(TipoPrevia::Copia, &f).unwrap();
            assert_eq!(midia_info("m", "x", &f, pv).marcavel, esperado, "{conteiner} pacotes={pacotes}");
        }
    }

    // ------------------------------------------------------------ com ffmpeg (os dois binários)

    #[test]
    fn copia_do_mp4_que_comeca_em_3s() {
        let Some(cap) = fontes::cap() else { return };
        let f = fonte_de_teste(&cap, &mp4_inicio_3s(&cap), true);
        assert!((f.inicio_s - 3.0).abs() < 0.03, "{} ({})", f.inicio_s, cap.versao);
        let p = conferir_copia(&cap, &f);
        assert!((p.de_s - 3.0).abs() < 1e-9, "{}", p.de_s);
        // segunda chamada não refaz: mesmo arquivo (inode e data), resposta igual e sem ffmpeg
        let cache = fontes::pasta_temporaria("previa-idem");
        let nao = AtomicBool::new(false);
        let a = copiar(&cap, &cache, &f, &nao, &mut sem_progresso()).unwrap();
        // data e tamanho em todo sistema; o inode só existe no Unix (o teste também compila no Windows, P0.10)
        let md = |p: &Path| {
            let m = std::fs::metadata(p).unwrap();
            #[cfg(unix)]
            let ino = std::os::unix::fs::MetadataExt::ino(&m);
            #[cfg(not(unix))]
            let ino = 0u64;
            (ino, m.modified().unwrap(), m.len())
        };
        let (m1, r1) = (md(&cache.join(&a.relativo)), md(&cache.join("previa").join(format!("{}.json", f.sha256))));
        // sem ffmpeg nem ffprobe: refazer qualquer passo falharia
        let sem_ffmpeg = Capacidades { binario: PathBuf::from("/sem/ffmpeg"), ffprobe: PathBuf::from("/sem/ffprobe"), ..(*cap).clone() };
        let t = Instant::now();
        let b = produzir(&sem_ffmpeg, &cache, &f, &nao, &mut sem_progresso()).unwrap();
        let gasto = t.elapsed();
        assert_eq!(a, b);
        assert_eq!(copiar(&sem_ffmpeg, &cache, &f, &nao, &mut sem_progresso()).unwrap(), a);
        assert_eq!((m1, r1), (md(&cache.join(&a.relativo)), md(&cache.join("previa").join(format!("{}.json", f.sha256)))));
        assert!(gasto.as_secs_f64() < SEGUNDA_CHAMADA_S * FOLGA, "{gasto:?}");
    }

    #[test]
    fn copia_do_mkv_30() {
        let Some(cap) = fontes::cap() else { return };
        conferir_copia(&cap, &fonte_de_teste(&cap, &fontes::gerar(Fonte::Mkv30).unwrap(), true));
    }

    #[test]
    fn copia_do_mkv_com_oscilacao_de_4ms() {
        let Some(cap) = fontes::cap() else { return };
        conferir_copia(&cap, &fonte_de_teste(&cap, &fontes::gerar(Fonte::Oscilacao).unwrap(), true));
    }

    #[test]
    fn copia_do_ts_5994() {
        let Some(cap) = fontes::cap() else { return };
        let f = fonte_de_teste(&cap, &fontes::gerar(Fonte::Ts5994).unwrap(), true);
        let p = conferir_copia(&cap, &f);
        assert!(p.desloc_s > 3600.0, "{}", p.desloc_s);
    }

    #[test]
    fn copia_do_ts_que_comeca_no_meio_do_gop() {
        let Some(cap) = fontes::cap() else { return };
        let f = fonte_de_teste(&cap, &fontes::gerar(Fonte::TsMeioDoGop).unwrap(), true);
        let p = conferir_copia(&cap, &f);
        // a cópia leva os quadros antes do primeiro quadro-chave; os dois decodificadores entregam do 180 ao 449
        let cache_tocado = fontes::pasta_temporaria("previa-gop");
        let q = copiar(&cap, &cache_tocado, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        assert_eq!(q.desloc_s, p.desloc_s);
        let esperado: Vec<u32> = (180..450).collect();
        assert_eq!(medir::quadros_de_origem(&cap, &f.caminho).unwrap(), esperado, "fonte ({})", cap.versao);
        assert_eq!(medir::quadros_de_origem(&cap, &cache_tocado.join(&q.relativo)).unwrap(), esperado, "cópia ({})", cap.versao);
    }

    #[test]
    fn copia_do_avi_25_com_g711_vira_aac() {
        let Some(cap) = fontes::cap() else { return };
        let f = fonte_de_teste(&cap, &fontes::gerar(Fonte::Avi25).unwrap(), true);
        assert_eq!(audio_do(&cap, &f.caminho).unwrap().0, "pcm_alaw");
        conferir_copia(&cap, &f);
    }

    #[test]
    fn copia_do_mpeg_ps() {
        let Some(cap) = fontes::cap() else { return };
        conferir_copia(&cap, &fonte_de_teste(&cap, &fontes::gerar(Fonte::MpegPs).unwrap(), true));
    }

    #[test]
    fn copia_sem_audio_sai_sem_audio() {
        let Some(cap) = fontes::cap() else { return };
        let f = fonte_de_teste(&cap, &fontes::gerar(Fonte::Mkv23976).unwrap(), true);
        assert!(f.audio.is_none());
        conferir_copia(&cap, &f);
    }

    /// Proxy de uma fonte: tipo, codec e tamanho do arquivo, mesma lista de pts (a menos do deslocamento), mesmos quadros.
    /// `origem`: números de quadro esperados no proxy; None = os que `medir` lê na fonte.
    fn conferir_proxy(cap: &Capacidades, fonte: &FonteInfo, p: &PreviaMidia, cache: &Path, identificar: bool, origem: Option<Vec<u32>>) {
        let nome = fonte.caminho.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!((p.tipo, p.pronta), (TipoPrevia::Proxy, true), "{nome}");
        assert_eq!(p.relativo, format!("proxies/{}.720.mp4", fonte.sha256));
        let tocado = cache.join(&p.relativo);
        let j = sonda_json(cap, &tocado);
        let v = fluxo(&j, "video").unwrap();
        assert_eq!((v["codec_name"].as_str(), v["pix_fmt"].as_str()), (Some("h264"), Some("yuv420p")), "{nome}");
        assert_eq!((v["width"].as_u64().unwrap() as u32, v["height"].as_u64().unwrap() as u32), (p.largura, p.altura), "{nome}");
        assert!(matches!(v["sample_aspect_ratio"].as_str(), None | Some("1:1")), "{nome}: {:?}", v["sample_aspect_ratio"]);
        assert!(v["side_data_list"].as_array().is_none_or(|l| l.iter().all(|d| d["rotation"].is_null())), "{nome}: giro no proxy");
        assert_eq!(v["has_b_frames"].as_u64(), Some(0), "{nome}");
        conferir_pts(cap, fonte, &tocado, p);
        if identificar {
            let esperado = origem.unwrap_or_else(|| medir::quadros_de_origem(cap, &fonte.caminho).unwrap());
            assert_eq!(medir::quadros_de_origem(cap, &tocado).unwrap(), esperado, "{nome} ({})", cap.versao);
            // cor: o quadrado vermelho do quadro do meio, por dentro da borda, igual a até COR_PROXY
            let n = esperado.len() as u32 / 2;
            let dentro = |q: &medir::QuadroRgb| {
                let m = medir::marca(q, [255, 0, 0], 60).expect("marca");
                medir::cor_media(q, [m[0] + 3, m[1] + 3, m[2].saturating_sub(6), m[3].saturating_sub(6)])
            };
            let (a, b) = (dentro(&medir::quadro(cap, &fonte.caminho, n).unwrap()), dentro(&medir::quadro(cap, &tocado, n).unwrap()));
            assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() <= COR_PROXY), "{nome}: cor {a:?} × {b:?} ({})", cap.versao);
        }
        // GOP 12: quadro-chave a cada 12 quadros
        let pacotes = String::from_utf8_lossy(&rodar(
            cap,
            &cap.ffprobe,
            &fontes::pasta_temporaria("previa-gop12"),
            "chaves",
            vec!["-v".into(), "error".into(), "-select_streams".into(), "v:0".into(), "-show_entries".into(), "packet=flags".into(), "-of".into(), "csv=p=0".into(), tocado.to_string_lossy().into_owned()],
        ))
        .into_owned();
        let chaves: Vec<usize> = pacotes.lines().enumerate().filter(|(_, l)| l.starts_with('K')).map(|(i, _)| i).collect();
        assert!(chaves.iter().enumerate().all(|(i, &k)| k == i * PROXY_GOP as usize), "{nome}: chaves {chaves:?}");
        match (audio_do(cap, &fonte.caminho), audio_do(cap, &tocado)) {
            (None, None) => {}
            (Some((c, _)), Some((ct, _))) if AUDIO_COPIA.contains(&c.as_str()) => assert_eq!(ct, c, "{nome}"),
            (Some(_), Some(at)) => assert_eq!(at, ("aac".to_string(), 48000), "{nome}"),
            (a, b) => panic!("{nome}: áudio {a:?} → {b:?}"),
        }
        let r: Registro = serde_json::from_slice(&std::fs::read(cache.join(format!("proxies/{}.720.json", fonte.sha256))).unwrap()).unwrap();
        assert_eq!((r.fonte_sha256.as_str(), r.ffmpeg_sha256.as_str()), (fonte.sha256.as_str(), cap.sha256.as_str()));
        assert_eq!(r.tamanho, std::fs::metadata(&tocado).unwrap().len());
    }

    #[test]
    fn hevc_vai_para_o_proxy() {
        let Some(cap) = fontes::cap() else { return };
        let f = fonte_de_teste(&cap, &hevc(&cap), true);
        assert_eq!((f.codec.as_str(), decidir(&f)), ("hevc", TipoPrevia::Proxy));
        let cache = fontes::pasta_temporaria("previa-hevc");
        assert!(copiar(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).is_err());
        let p = produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        assert_eq!((p.largura, p.altura), (320, 180));
        assert_eq!(consultar(f.caminho.parent().unwrap(), &cache, &f).unwrap(), p);
        conferir_proxy(&cap, &f, &p, &cache, true, None);
        let r: Registro = serde_json::from_slice(&std::fs::read(cache.join(format!("proxies/{}.720.json", f.sha256))).unwrap()).unwrap();
        assert_eq!(r.motivo.as_deref(), Some("codec: hevc yuv420p"));
    }

    #[test]
    fn dez_bits_vai_para_o_proxy() {
        let Some(cap) = fontes::cap() else { return };
        let f = fonte_de_teste(&cap, &dez_bits(&cap), true);
        assert_eq!((f.pix_fmt.as_str(), decidir(&f)), ("yuv420p10le", TipoPrevia::Proxy));
        let cache = fontes::pasta_temporaria("previa-10bits");
        let p = produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        // `medir` só lê 8 bits: a fonte são os 150 primeiros quadros da matriz
        conferir_proxy(&cap, &f, &p, &cache, true, Some((0..150).collect()));
    }

    #[test]
    fn proxy_de_ts_avi_e_ps_mantem_os_pts() {
        let Some(cap) = fontes::cap() else { return };
        for fonte in [Fonte::Ts5994, Fonte::Avi25, Fonte::MpegPs, Fonte::Oscilacao, Fonte::TaxaVariavel] {
            let f = fonte_de_teste(&cap, &fontes::gerar(fonte).unwrap(), true);
            let cache = fontes::pasta_temporaria("previa-proxy-matriz");
            let p = proxy_cpu(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
            conferir_proxy(&cap, &f, &p, &cache, true, None);
        }
    }

    #[test]
    fn proxy_de_pixel_nao_quadrado_e_girado_sai_com_pixel_quadrado() {
        let Some(cap) = fontes::cap() else { return };
        for (fonte, tamanho) in [(Fonte::PixelNaoQuadrado, (654, 480)), (Fonte::Giro90, (180, 320))] {
            let f = fonte_de_teste(&cap, &fontes::gerar(fonte).unwrap(), true);
            assert_eq!(decidir(&f), TipoPrevia::Original);
            let cache = fontes::pasta_temporaria("previa-proxy-geom");
            let p = proxy_cpu(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
            assert_eq!((p.largura, p.altura), tamanho, "{fonte:?}");
            conferir_proxy(&cap, &f, &p, &cache, true, None);
            // quadrado vermelho no mesmo lugar, em fração do quadro
            let n = f.quadros.pts.len() as u32 / 2;
            let q = medir::quadro(&cap, &cache.join(&p.relativo), n).unwrap();
            let m = medir::marca(&q, [255, 0, 0], 60).expect("marca");
            let e = fontes::marca_esperada(fonte, n);
            let (w, h) = (q.w as f64, q.h as f64);
            for (a, b, l) in [(m[0] as f64 / w, e[0], w), (m[1] as f64 / h, e[1], h), (m[2] as f64 / w, e[2], w), (m[3] as f64 / h, e[3], h)] {
                assert!(((a - b) * l).abs() <= 3.0, "{fonte:?}: marca {m:?} × {e:?} ({})", cap.versao);
            }
        }
    }

    #[test]
    fn proxy_reduz_para_720_e_cancelar_no_meio_nao_deixa_pronto() {
        let Some(cap) = fontes::cap() else { return };
        let f = fonte_de_teste(&cap, &grande(&cap), true);
        let cache = fontes::pasta_temporaria("previa-grande");
        let ws = f.caminho.parent().unwrap();
        // cancelar no primeiro avanço do meio
        let cancelar = AtomicBool::new(false);
        let mut visto = Vec::new();
        let e = proxy_cpu(&cap, &cache, &f, &cancelar, &mut |p| {
            visto.push(p);
            if p > 0.0 && p < PARTE_FFMPEG {
                cancelar.store(true, Ordering::SeqCst);
            }
        })
        .unwrap_err();
        assert!(cancelado(&e), "{e}");
        assert!(visto.iter().any(|&p| p > 0.0 && p < PARTE_FFMPEG), "{visto:?}");
        assert_eq!(decidir(&f), TipoPrevia::Original);
        assert!(pronta(&cache, TipoPrevia::Proxy, &f).unwrap().is_none());
        let resto: Vec<_> = std::fs::read_dir(cache.join("proxies")).map(|r| r.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
        assert!(resto.is_empty(), "{resto:?}");
        // cópia cancelada antes de começar: nada no cache
        let e = copiar(&cap, &cache, &f, &AtomicBool::new(true), &mut sem_progresso()).unwrap_err();
        assert!(cancelado(&e), "{e}");
        assert!(!cache.join("previa").exists() || std::fs::read_dir(cache.join("previa")).unwrap().next().is_none());
        // inteiro: 1920×1080 → 1280×720
        let p = proxy_cpu(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        assert_eq!((p.largura, p.altura), (1280, 720));
        conferir_proxy(&cap, &f, &p, &cache, false, None);
        assert_eq!(consultar(ws, &cache, &f).unwrap().tipo, TipoPrevia::Original);
    }

    /// Tipo esperado das fontes de `ce/wk/media` (M1, M2): o resto da pasta, pelo ffprobe.
    const DOSSIE_ORIGINAL: &[&str] = &[
        "cam1.mp4", "cam2.mp4", "cam3.mp4", "cam4.mp4", "cam1_p720.mp4", "cam2_p720.mp4", "cam3_p720.mp4", "cam4_p720.mp4", "h0.mp4", "hcut.mp4", "idx30.mp4",
        "idx30_rot90.mp4", "r0.mp4", "rb.mp4", "sar_10_11.mp4", "v1080_g30.mp4", "v480_g60.mp4", "v480_g300.mp4", "v480_g600.mp4", "v4k_g300.mp4",
    ];
    const DOSSIE_COPIA: &[&str] = &[
        "cp_r007_mkv.mp4", "cp_r0_ts.mp4", "cp_r12h_mkv.mp4", "cp_r3d_mkv.mp4", "cp_r3_mkv.mp4", "cp_r3_mp4.mp4", "h007.mkv", "h007.mp4", "h3.mkv", "h3.mp4", "h3v.mp4",
        "hcopyts.mp4", "idx30_st3.mkv", "idx30_st3.mov", "idx30_st3.mp4", "r007.mkv", "r007.mp4", "r0.mkv", "r0.ts", "r12h.mkv", "r12h.mp4", "r3d.mkv", "r3.mkv", "r3.mov",
        "r3.mp4", "r3v.mkv", "rb_dvr.mkv", "rb.mkv",
    ];
    /// Acima disso a tabela sai dos pacotes (só os originais grandes; sem decodificar 4K no teste).
    const DOSSIE_DECODIFICAR_ATE: u64 = 5 << 20;

    #[test]
    fn fontes_do_dossie() {
        let Some(cap) = fontes::cap() else { return };
        let Some(pasta) = fontes::dossie().map(|d| d.join("ce/wk/media")).filter(|p| p.is_dir()) else {
            eprintln!("aviso: sem ce/wk/media no dossiê; teste dispensado");
            return;
        };
        let mut nomes: Vec<String> = std::fs::read_dir(&pasta).unwrap().flatten().filter(|e| e.path().is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        nomes.sort();
        let mut contagem: HashMap<&str, usize> = HashMap::new();
        for nome in &nomes {
            let caminho = pasta.join(nome);
            let grande = std::fs::metadata(&caminho).unwrap().len() > DOSSIE_DECODIFICAR_ATE;
            let f = fonte_de_teste(&cap, &caminho, !grande);
            let esperado = if DOSSIE_ORIGINAL.contains(&nome.as_str()) {
                TipoPrevia::Original
            } else if DOSSIE_COPIA.contains(&nome.as_str()) {
                TipoPrevia::Copia
            } else {
                eprintln!("aviso: {nome} fora da lista; tipo pelo ffprobe");
                if !codec_serve(&f) {
                    TipoPrevia::Proxy
                } else if conteiner_mp4(&f.conteiner) && f.inicio_s == 0.0 && f.quadros.pts.first() == Some(&0) {
                    TipoPrevia::Original
                } else {
                    TipoPrevia::Copia
                }
            };
            assert_eq!(decidir(&f), esperado, "{nome} ({})", cap.versao);
            match esperado {
                TipoPrevia::Original => {
                    let p = consultar(&pasta, Path::new("/sem-cache"), &f).unwrap();
                    assert_eq!((p.relativo.as_str(), p.desloc_s, p.pronta, (p.largura, p.altura)), (nome.as_str(), 0.0, true, f.exibido), "{nome}");
                    assert_eq!(p.de_s, 0.0, "{nome}");
                }
                TipoPrevia::Copia => {
                    conferir_copia(&cap, &f);
                }
                _ => {
                    let cache = fontes::pasta_temporaria("previa-dossie-proxy");
                    let p = produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
                    conferir_proxy(&cap, &f, &p, &cache, false, None);
                }
            }
            *contagem.entry(match esperado {
                TipoPrevia::Original => "original",
                TipoPrevia::Copia => "copia",
                _ => "proxy",
            })
            .or_default() += 1;
        }
        eprintln!("ce/wk/media ({}): {contagem:?}", cap.versao);
        assert!(contagem.get("copia").copied().unwrap_or(0) >= DOSSIE_COPIA.len(), "{contagem:?}");
    }

    // ------------------------------------------------------------ revisão adversa

    /// Toda a matriz. MP4 com quadros B e com lista de edição (corte real, `media_time` > 0) começam em zero e ficam
    /// `Original`: a M1/M2 não mediu corte real no `<video>` (a `hcut.mp4` do dossiê tem `media_time` 0).
    #[test]
    fn tipo_de_cada_fonte_da_matriz() {
        let Some(cap) = fontes::cap() else { return };
        for &fonte in fontes::MATRIZ {
            let f = fonte_de_teste(&cap, &fontes::gerar(fonte).unwrap(), true);
            let esperado = match fonte {
                Fonte::Mkv30 | Fonte::Mkv23976 | Fonte::Avi25 | Fonte::Ts5994 | Fonte::TsMeioDoGop | Fonte::MpegPs | Fonte::Oscilacao => TipoPrevia::Copia,
                _ => TipoPrevia::Original,
            };
            assert_eq!(decidir(&f), esperado, "{fonte:?} ({})", cap.versao);
        }
    }

    /// Tabela de pacotes (a que `obter` dá antes da do decodificador): o TS cortado tem 30 pacotes antes do primeiro
    /// quadro-chave. A cópia leva todos (`-copyinkf`), com o mesmo deslocamento; sem eles a conferência cairia no proxy.
    #[test]
    fn copia_com_tabela_de_pacotes_leva_os_quadros_antes_do_quadro_chave() {
        let Some(cap) = fontes::cap() else { return };
        let f = fonte_de_teste(&cap, &fontes::gerar(Fonte::TsMeioDoGop).unwrap(), false);
        assert_eq!(f.quadros.origem, OrigemTabela::Pacotes);
        let decodificados = medir::pts(&cap, &f.caminho).unwrap();
        assert_eq!(f.quadros.pts.len(), decodificados.len() + 30, "pacotes × decodificador ({})", cap.versao);
        let cache = fontes::pasta_temporaria("previa-pacotes-ts");
        let p = produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        assert_eq!((p.tipo, p.pronta), (TipoPrevia::Copia, true), "{}", cap.versao);
        let tocado = cache.join(&p.relativo);
        let (tb, pc) = pacotes(&cap, &tocado, &cache, &AtomicBool::new(false)).unwrap();
        assert_eq!(pc.len(), f.quadros.pts.len(), "pacotes da cópia ({})", cap.versao);
        for (j, (&a, &b)) in f.quadros.pts.iter().zip(&pc).enumerate() {
            let d = segundos(a, f.quadros.tb) - segundos(b, tb);
            assert!((d - p.desloc_s).abs() <= TOLERANCIA_S, "pacote {j}: {d} × {} ({})", p.desloc_s, cap.versao);
        }
        conferir_pts(&cap, &f, &tocado, &p);
        assert_eq!(hashes_por_quadro(&cap, &f.caminho), hashes_por_quadro(&cap, &tocado), "quadros ({})", cap.versao);
    }

    /// Duas chamadas ao mesmo tempo para a mesma mídia: as duas terminam com a mesma prévia, o `.json` vale para o
    /// arquivo que ficou e nenhuma pasta de trabalho sobra.
    #[test]
    fn duas_chamadas_ao_mesmo_tempo_para_a_mesma_midia() {
        let Some(cap) = fontes::cap() else { return };
        for fonte in [Fonte::Mkv30, Fonte::Avi25] {
            let f = fonte_de_teste(&cap, &fontes::gerar(fonte).unwrap(), true);
            let cache = fontes::pasta_temporaria("previa-duas");
            let barreira = std::sync::Barrier::new(2);
            let uma = || {
                barreira.wait();
                produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso())
            };
            let (a, b) = std::thread::scope(|s| {
                let (h1, h2) = (s.spawn(uma), s.spawn(uma));
                (h1.join().unwrap(), h2.join().unwrap())
            });
            let (a, b) = (a.unwrap(), b.unwrap());
            assert_eq!(a, b, "{fonte:?}");
            assert_eq!((a.tipo, a.pronta), (TipoPrevia::Copia, true), "{fonte:?}");
            assert_eq!(consultar(f.caminho.parent().unwrap(), &cache, &f).unwrap(), a, "{fonte:?}");
            let tocado = cache.join(&a.relativo);
            let r: Registro = serde_json::from_slice(&std::fs::read(cache.join(format!("previa/{}.json", f.sha256))).unwrap()).unwrap();
            assert_eq!(r.tamanho, std::fs::metadata(&tocado).unwrap().len(), "{fonte:?}");
            let resto: Vec<_> = std::fs::read_dir(cache.join("previa")).unwrap().flatten().map(|e| e.file_name()).collect();
            assert_eq!(resto.len(), 2, "{fonte:?}: {resto:?}");
            conferir_pts(&cap, &f, &tocado, &a);
            assert_eq!(hashes_por_quadro(&cap, &f.caminho), hashes_por_quadro(&cap, &tocado), "{fonte:?} ({})", cap.versao);
        }
    }

    /// Giro de 90° e pixel 10:11 num MP4 que começa em 3 s: a cópia guarda os dois (o `<video>` gira e estica igual à fonte).
    #[test]
    fn copia_de_video_girado_e_de_pixel_nao_quadrado() {
        let Some(cap) = fontes::cap() else { return };
        for (fonte, nome) in [(Fonte::Giro90, "giro-90-inicio-3s.mp4"), (Fonte::PixelNaoQuadrado, "pixel-10-11-inicio-3s.mp4")] {
            let m = fontes::gerar(fonte).unwrap().to_string_lossy().into_owned();
            let v = extra(&cap, nome, &["-i", &m, "-map", "0", "-c", "copy", "-output_ts_offset", "3"]);
            let f = fonte_de_teste(&cap, &v, true);
            let d = fontes::descricao(fonte);
            // `Descricao.giro` é o do `-display_rotation` (anti-horário); `FonteInfo.giro` é horário
            let giro = (360 - d.giro).rem_euclid(360);
            assert_eq!((f.giro, f.sar), (giro, Racional { num: d.sar.0 as i64, den: d.sar.1 as i64 }), "{nome} ({})", cap.versao);
            conferir_copia(&cap, &f);
        }
    }

    /// Põe uma espera (edição vazia) de `espera_s` antes da 1ª edição do `elst` do vídeo; `moov` no fim do arquivo.
    /// É o recorte exportado com início ≠ 0: corte real (pré-rolagem escondida) e o vídeo começando depois.
    fn com_espera(origem: &Path, destino: &Path, espera_s: f64) {
        let mut b = std::fs::read(origem).unwrap();
        let u32_em = |b: &[u8], i: usize| u32::from_be_bytes(b[i..i + 4].try_into().unwrap()) as usize;
        let caixas = |b: &[u8], ini: usize, fim: usize| {
            let mut v = Vec::new();
            let mut i = ini;
            while i + 8 <= fim {
                let t = u32_em(b, i);
                v.push((i, t, b[i + 4..i + 8].to_vec()));
                i += t.max(8);
            }
            v
        };
        let (mi, mt, _) = caixas(&b, 0, b.len()).into_iter().find(|c| c.2 == b"moov").unwrap();
        assert_eq!(mi + mt, b.len(), "moov fora do fim");
        let mvhd = caixas(&b, mi + 8, mi + mt).into_iter().find(|c| c.2 == b"mvhd").unwrap().0;
        let escala = if b[mvhd + 8] == 0 { u32_em(&b, mvhd + 20) } else { u32_em(&b, mvhd + 28) };
        let mut achado = None;
        for (ti, tt, tipo) in caixas(&b, mi + 8, mi + mt) {
            if tipo != b"trak" {
                continue;
            }
            let filhos = caixas(&b, ti + 8, ti + tt);
            let video = filhos.iter().filter(|c| c.2 == b"mdia").any(|m| caixas(&b, m.0 + 8, m.0 + m.1).iter().any(|h| h.2 == b"hdlr" && &b[h.0 + 16..h.0 + 20] == b"vide"));
            if let (true, Some(e)) = (video, filhos.iter().find(|c| c.2 == b"edts")) {
                achado = Some((ti, e.0, e.1));
                break;
            }
        }
        let (ti, ei, et) = achado.expect("vídeo sem edts");
        let (li, lt, _) = caixas(&b, ei + 8, ei + et).into_iter().find(|c| c.2 == b"elst").unwrap();
        assert_eq!(b[li + 8], 0, "elst v1");
        let n = u32_em(&b, li + 12) as u32;
        let mut novo = Vec::new();
        novo.extend(((lt + 12) as u32).to_be_bytes());
        novo.extend(b"elst\0\0\0\0");
        novo.extend((n + 1).to_be_bytes());
        novo.extend(((espera_s * escala as f64).round() as u32).to_be_bytes());
        novo.extend((-1i32).to_be_bytes());
        novo.extend([0, 1, 0, 0]);
        novo.extend_from_slice(&b[li + 16..li + lt]);
        b.splice(li..li + lt, novo);
        for pos in [ei, ti, mi] {
            let t = u32_em(&b, pos) + 12;
            b[pos..pos + 4].copy_from_slice(&(t as u32).to_be_bytes());
        }
        std::fs::write(destino, b).unwrap();
    }

    /// MP4 com corte real (pré-rolagem 30..44 escondida pela lista de edição) começando em 3 s: é `Copia`, mas todo
    /// remux mostra a pré-rolagem (317 quadros contra 302). A cópia é recusada e vai o proxy, com os quadros 45.. da fonte.
    #[test]
    fn lista_de_edicao_que_esconde_quadros_vai_para_o_proxy() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("previa-espera");
        let v = pasta.join("corte-espera-3s.mp4");
        com_espera(&fontes::gerar(Fonte::Mp4ListaDeEdicao).unwrap(), &v, 3.0);
        let f = fonte_de_teste(&cap, &v, true);
        assert_eq!(decidir(&f), TipoPrevia::Copia, "{}", cap.versao);
        assert!((segundos(f.quadros.pts[0], f.quadros.tb) - 3.0).abs() < 1e-9, "{:?} ({})", f.quadros.pts.first(), cap.versao);
        let origem = medir::quadros_de_origem(&cap, &v).unwrap();
        assert_eq!(origem.first(), Some(&45), "fonte ({})", cap.versao);
        let cache = pasta.join("cache");
        let e = copiar(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap_err();
        assert!(matches!(&e, SicroError::Validation(m) if m.starts_with(ERRO_LEITURA_DIVERGENTE)), "{e} ({})", cap.versao);
        assert!(pronta(&cache, TipoPrevia::Copia, &f).unwrap().is_none());
        let p = produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        conferir_proxy(&cap, &f, &p, &cache, true, Some(origem));
        let r: Registro = serde_json::from_slice(&std::fs::read(cache.join(format!("proxies/{}.720.json", f.sha256))).unwrap()).unwrap();
        assert!(r.motivo.as_deref().is_some_and(|m| m.starts_with("remux: ")), "{:?}", r.motivo);
    }

    /// Fonte com quadros B fora do MP4 (MKV; TS começando em 10 s): a cópia mantém os quadros e o deslocamento.
    #[test]
    fn copia_de_fonte_com_quadros_b() {
        let Some(cap) = fontes::cap() else { return };
        let qb = fontes::gerar(Fonte::Mp4QuadrosB).unwrap().to_string_lossy().into_owned();
        let mkv = extra(&cap, "quadros-b.mkv", &["-i", &qb, "-map", "0", "-c", "copy"]);
        let ts = extra(&cap, "quadros-b.ts", &["-i", &qb, "-map", "0", "-c", "copy", "-output_ts_offset", "10", "-f", "mpegts"]);
        for v in [mkv, ts] {
            let f = fonte_de_teste(&cap, &v, true);
            let p = conferir_copia(&cap, &f);
            assert_eq!(medir::quadros_de_origem(&cap, &v).unwrap(), (0..450).collect::<Vec<u32>>(), "{} ({})", v.display(), cap.versao);
            eprintln!("{}: desloc_s {:.6} ({})", v.display(), p.desloc_s, cap.versao);
        }
    }

    /// Fonte e cache em pastas com espaço, acento e pontuação (nome de caso real; no Windows o cache fica sob o usuário).
    #[test]
    fn caminho_com_espaco_e_acento() {
        let Some(cap) = fontes::cap() else { return };
        let base = fontes::pasta_temporaria("previa-caminho");
        let caso = base.join("Caso nº 12 — São João");
        let pasta = caso.join("Mídias (cópia) #1");
        std::fs::create_dir_all(&pasta).unwrap();
        let video = pasta.join("câmera 1, entrada; 'portão' %d.mkv");
        std::fs::copy(fontes::gerar(Fonte::Mkv30).unwrap(), &video).unwrap();
        let f = fonte_de_teste(&cap, &video, true);
        let cache = base.join("cache ção/edição x");
        let p = produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        assert_eq!((p.tipo, p.pronta), (TipoPrevia::Copia, true), "{}", cap.versao);
        let tocado = cache.join(&p.relativo);
        conferir_pts(&cap, &f, &tocado, &p);
        assert_eq!(hashes_por_quadro(&cap, &f.caminho), hashes_por_quadro(&cap, &tocado), "{}", cap.versao);
        // original: relativo ao caso, com '/'
        let mp4 = pasta.join("câmera 2 ação.mp4");
        std::fs::copy(fontes::gerar(Fonte::Mp4).unwrap(), &mp4).unwrap();
        let o = consultar(&caso, &cache, &fonte_de_teste(&cap, &mp4, true)).unwrap();
        assert_eq!((o.tipo, o.relativo.as_str()), (TipoPrevia::Original, "Mídias (cópia) #1/câmera 2 ação.mp4"));
    }

    /// Arquivo de áudio (E2: tabela vazia, sem vídeo): erro explícito, nunca uma prévia pendente que não fica pronta.
    #[test]
    fn fonte_so_de_audio_nao_tem_previa() {
        let Some(cap) = fontes::cap() else { return };
        let mut f = fonte_falsa("wav", "", "", 0.0, 0);
        f.exibido = (0, 0);
        f.quadros = Arc::new(TabelaQuadros { pts: Vec::new(), chave: Vec::new(), pacotes: 0, ..(*f.quadros).clone() });
        f.audio = Some(AudioInfo { inicio_s: 0.0, taxa: 8000, canais: 1 });
        let nao_suportado = |e: SicroError| matches!(&e, SicroError::Validation(m) if m.starts_with("nao_suportado"));
        assert!(nao_suportado(consultar(Path::new("/caso"), Path::new("/cache"), &f).unwrap_err()));
        let cache = fontes::pasta_temporaria("previa-audio");
        assert!(nao_suportado(produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap_err()));
        assert!(nao_suportado(proxy_cpu(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap_err()));
        assert!(std::fs::read_dir(&cache).unwrap().next().is_none());
        // vídeo sem nenhum quadro: erro final, nunca `tabela_pendente` (a tela esperaria para sempre)
        let mut v = fonte_falsa("matroska,webm", "h264", "yuv420p", 0.0, 0);
        v.quadros = Arc::new(TabelaQuadros { pts: Vec::new(), chave: Vec::new(), pacotes: 0, ..(*v.quadros).clone() });
        let danificada = |e: SicroError| matches!(&e, SicroError::Validation(m) if m.starts_with("fonte_danificada"));
        assert!(danificada(consultar(Path::new("/caso"), &cache, &v).unwrap_err()));
        assert!(danificada(produzir(&cap, &cache, &v, &AtomicBool::new(false), &mut sem_progresso()).unwrap_err()));
        assert!(std::fs::read_dir(&cache).unwrap().next().is_none());
    }

    /// Imagem parada (PNG e JPEG, como o `ffprobe` as dá): sem prévia de `<video>` e nada no cache.
    #[test]
    fn imagem_nao_tem_previa() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("previa-imagem");
        let cache = pasta.join("cache");
        for nome in ["logo.png", "foto.jpg"] {
            let img = pasta.join(nome);
            rodar(&cap, &cap.binario, &pasta, nome, s(&["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=size=320x180", "-frames:v", "1", nome]));
            let f = fonte_de_teste(&cap, &img, true);
            assert!(!f.codec.is_empty() && f.exibido == (320, 180), "{nome}: {} {:?} ({})", f.codec, f.exibido, cap.versao);
            let nao_suportado = |e: SicroError| matches!(&e, SicroError::Validation(m) if m.starts_with("nao_suportado"));
            assert!(nao_suportado(consultar(&pasta, &cache, &f).unwrap_err()), "{nome} ({}): {}", cap.versao, f.conteiner);
            assert!(nao_suportado(produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap_err()), "{nome}");
        }
        assert!(!cache.exists());
    }

    /// Arquivo de 0 bytes com tabela antiga: cópia e proxy falham, nada fica dado como pronto, nenhuma pasta de trabalho sobra.
    #[test]
    fn arquivo_vazio_falha_sem_deixar_nada() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("previa-vazio");
        let mut f = fonte_falsa("matroska,webm", "h264", "yuv420p", 0.0, 0);
        f.caminho = pasta.join("vazio.mkv");
        std::fs::write(&f.caminho, b"").unwrap();
        let cache = pasta.join("cache");
        assert!(produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).is_err());
        assert!(!consultar(&pasta, &cache, &f).unwrap().pronta);
        for sub in [PASTA_COPIA, PASTA_PROXY] {
            let resto: Vec<_> = std::fs::read_dir(cache.join(sub)).map(|r| r.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
            assert!(resto.is_empty(), "{sub}: {resto:?}");
        }
    }

    /// `true` se o ffprobe abre o arquivo e acha vídeo (MP4 de gravador sem `moov` não abre).
    fn abre(cap: &Capacidades, video: &Path) -> bool {
        let p = fontes::pasta_temporaria("previa-abre");
        let mut a = s(&["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=codec_type", "-of", "csv=p=0"]);
        a.push(video.to_string_lossy().into_owned());
        let cmd = comando(&cap.ffprobe, &p, "abre", a);
        let (t, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).unwrap();
        t.codigo == Some(0) && String::from_utf8_lossy(&out).contains("video")
    }

    /// Amostras reais de gravador (Dahua com áudio no índice 0 e relógio em 1,5·10⁹ s, Hikvision PS com nome .mp4,
    /// IFV, LVF), quando presentes: cópia com os mesmos quadros e o deslocamento medido em todos.
    #[test]
    fn copia_das_amostras_de_gravador() {
        let Some(cap) = fontes::cap() else { return };
        let amostras = fontes::amostras_de_gravador();
        if amostras.is_empty() {
            eprintln!("aviso: sem amostras de gravador; teste dispensado");
            return;
        }
        let mut feitas = Vec::new();
        for a in &amostras {
            let nome = a.file_name().unwrap().to_string_lossy().into_owned();
            if !abre(&cap, a) {
                eprintln!("{nome}: não abre ({})", cap.versao);
                continue;
            }
            let f = fonte_de_teste(&cap, a, true);
            let p = conferir_copia(&cap, &f);
            eprintln!("{nome}: desloc_s {:.6} ({})", p.desloc_s, cap.versao);
            feitas.push(nome);
        }
        assert!(!feitas.is_empty(), "{amostras:?}");
    }

    /// Proxy das mesmas amostras: relógio de gravador (ms, com oscilação, início em 1,5·10⁹ s) passa com a mesma
    /// lista de pts e o mesmo número de quadros; 2592×1520 cai para 720 de altura.
    #[test]
    fn proxy_das_amostras_de_gravador() {
        let Some(cap) = fontes::cap() else { return };
        let mut feitas = 0;
        for a in fontes::amostras_de_gravador().iter().filter(|a| abre(&cap, a)) {
            let f = fonte_de_teste(&cap, a, true);
            let cache = fontes::pasta_temporaria("previa-proxy-gravador");
            let p = proxy_cpu(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
            assert!(p.altura <= PROXY_ALTURA && p.altura <= f.exibido.1, "{}: {p:?}", a.display());
            conferir_proxy(&cap, &f, &p, &cache, false, None);
            feitas += 1;
        }
        eprintln!("proxy de {feitas} amostras de gravador ({})", cap.versao);
    }

    /// Processo auxiliar de `processo_morto_no_meio_nao_deixa_pronto`: faz o proxy e avisa quando o ffmpeg já andou.
    #[test]
    #[ignore]
    fn auxiliar_previa_que_morre() {
        let (Ok(cache), Ok(video)) = (std::env::var("SICRO_TESTE_PREVIA_CACHE"), std::env::var("SICRO_TESTE_PREVIA_VIDEO")) else { return };
        let Some(cap) = fontes::cap() else { return };
        let (cache, video) = (PathBuf::from(cache), PathBuf::from(video));
        let f = fonte_de_teste(&cap, &video, false);
        let aviso = cache.join("andou");
        let _ = proxy_cpu(&cap, &cache, &f, &AtomicBool::new(false), &mut |p| {
            if p > 0.1 && !aviso.exists() {
                std::fs::write(&aviso, b"").unwrap();
            }
        });
    }

    /// Morte do ffmpeg depois da do SICRO: limite real (a folga de 3× vale enquanto houver outros agentes).
    const MORTE_S: f64 = 1.0;

    /// O SICRO morre (SIGKILL) com o proxy no meio: nada dado como pronto, o ffmpeg morre junto e a pasta de
    /// trabalho que sobra (a limpeza é do P0.7) não atrapalha a vez seguinte.
    #[cfg(target_os = "linux")]
    #[test]
    fn processo_morto_no_meio_nao_deixa_pronto() {
        let Some(cap) = fontes::cap() else { return };
        let video = grande(&cap);
        let cache = fontes::pasta_temporaria("previa-morre");
        let mut aux = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "edicao::previa::testes::auxiliar_previa_que_morre", "--ignored", "--nocapture"])
            .env("SICRO_TESTE_PREVIA_CACHE", &cache)
            .env("SICRO_TESTE_PREVIA_VIDEO", &video)
            // as pastas temporárias do auxiliar morto ficam dentro da deste teste e somem com ela
            .env("TMPDIR", fontes::pasta_temporaria("previa-morre-tmp"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let t0 = Instant::now();
        while !cache.join("andou").exists() {
            assert!(t0.elapsed().as_secs() < 120 && aux.try_wait().unwrap().is_none(), "o auxiliar não chegou ao meio ({})", cap.versao);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        aux.kill().unwrap();
        let _ = aux.wait();
        let morto = Instant::now();
        // ffmpeg da prévia = processo vivo com a pasta de trabalho dentro do cache
        let no_cache = || -> Vec<u32> {
            std::fs::read_dir("/proc")
                .unwrap()
                .flatten()
                .filter_map(|e| e.file_name().to_string_lossy().parse::<u32>().ok())
                .filter(|p| std::fs::read_link(format!("/proc/{p}/cwd")).is_ok_and(|c| c.starts_with(&cache)))
                .collect()
        };
        while !no_cache().is_empty() {
            assert!(morto.elapsed().as_secs_f64() < MORTE_S * FOLGA, "ffmpeg vivo depois da morte do SICRO: {:?} ({})", no_cache(), cap.versao);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let f = fonte_de_teste(&cap, &video, false);
        assert!(pronta(&cache, TipoPrevia::Proxy, &f).unwrap().is_none());
        let sobra: Vec<String> = std::fs::read_dir(cache.join(PASTA_PROXY)).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(!sobra.is_empty() && sobra.iter().all(|n| n.starts_with(&format!("{}.tmp-", f.sha256))), "{sobra:?}");
        let p = proxy_cpu(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        assert_eq!(pronta(&cache, TipoPrevia::Proxy, &f).unwrap(), Some(p));
    }
}
