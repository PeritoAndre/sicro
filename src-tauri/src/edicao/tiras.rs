//! Tiras de miniaturas da linha do tempo (`receitas-audio-saida.md` §10, receita B): só quadros-chave
//! (`-skip_frame nokey`), um por janela de tempo de fonte, reduzidos e juntados numa imagem só (`tile`) em JPEG, com
//! o mapa miniatura → célula. Uma imagem e não um JPEG por miniatura: foi o que a §10 mediu mais rápido (B, 2,4 s
//! para 60 miniaturas de 10 min em 1080p, contra 3,7 a 9,0 s do pipe de JPEGs), e a tela busca um arquivo só.
//! Cache `tiras/<sha256 da mídia>/<chave>.{jpg,json}`; feita na fila de fundo (`TipoFundo::Tira`). Dono: P4.3.
//!
//! Regra das miniaturas (n janelas iguais de `[de_s, ate_s)`, tempo de fonte, `-copyts`):
//! - miniatura 0: o quadro-chave exibido em `de_s` (o último com t ≤ `de_s`, pela tabela); sem ele, o primeiro da janela;
//! - miniatura k ≥ 1: o primeiro quadro-chave da janela k;
//! - janela sem quadro-chave repete a célula anterior (as do começo, a primeira).
//!
//! Dois modos. **Tempo** (contêiner com tempo nos pacotes): o `select` escolhe pelo `t` do quadro que o decodificador
//! entrega; vale o quadro-chave do decodificador (o IFV não marca nenhum nos pacotes) e a busca da regra do §5.3.
//! **Ordem** (MPEG-PS, H.264/HEVC cru): com `-skip_frame nokey` esses chegam com o pts errado, mas na ordem e iguais
//! aos quadros-chave da tabela do decodificador (soma de cada quadro conferida no MPEG-PS da matriz, Hikvision IMKH,
//! IFV e Dahua, nos dois binários): a escolha é feita na tabela e o `select` pega pelo número de ordem, sem busca.
//! Sem a tabela do decodificador esses dão `tabela_pendente`.
//! **Atalho do modo Ordem** (faixa longe do começo): a leitura começa perto da faixa e cada pacote ganha o pts que a
//! tabela deu ao quadro dele (`setts` na entrada); a escolha volta a ser pelo tempo, conferida célula a célula com a
//! tabela. MPEG-PS: `-ss`, pts que falta = o anterior + duração (é a conta da tabela). Fluxo cru: o ffmpeg ignora o pts
//! do pacote, então entra por uma lista do `concat` que pula até o byte do pacote (`skip_initial_bytes`, posições
//! medidas uma vez por mídia) e conta pts = pts₀ + N·passo. Não bateu: o modo Ordem desde o começo.
//! Giro do contêiner e pixel não quadrado: a célula tem a proporção exibida e o giro automático do ffmpeg, como o
//! reprodutor mostra.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::edicao::executar::{sem_tempo_nos_pacotes, sha256_bytes, sha_valido, ERRO_CANCELADO};
use crate::edicao::ffmpeg::exec;
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::fundo::{self, TipoFundo};
use crate::edicao::modelo::{Faixa, Matriz, Racional};
use crate::edicao::motor::FonteInfo;
use crate::edicao::previa;
use crate::edicao::quadros::{conteiner_de_imagem, OrigemTabela, TabelaQuadros};
use crate::error::{Result, SicroError};

/// Versão da receita: mudar invalida as tiras em cache.
pub const VERSAO: u32 = 1;
/// Altura de cada miniatura (px).
pub const MIN_ALTURA: u32 = 16;
pub const MAX_ALTURA: u32 = 360;
pub const MAX_MINIATURAS: u32 = 256;
/// Largura máxima da imagem (n × largura da célula, px).
pub const MAX_LARGURA: u32 = 16384;
const PASTA: &str = "tiras";
const FORMATO: &str = "sicro.tira/1";
/// `-q:v` do JPEG (§10).
const QUALIDADE: &str = "4";
/// Folga abaixo do quadro-chave da miniatura 0 (s): o `t` do filtro e o da tabela podem diferir no último bit.
const FOLGA_T0_S: f64 = 1e-4;
/// Folga na fronteira das janelas (fração de janela): quadro-chave EM cima da fronteira fica na janela que começa
/// nele, também quando a base de tempo não é exata em binário (10 s em 1/15360 dá 9,999…).
const FOLGA_JANELA: f64 = 1e-6;
/// Modo Ordem: abaixo de tantos quadros-chave antes da faixa, ler desde o começo custa menos que o atalho.
const PERTO: usize = 16;
/// Recuo do `-ss` do atalho antes do quadro-chave (s), como a regra do §5.3.
const RECUO_S: f64 = 0.5;
/// pts que falta no MPEG-PS: o do pacote anterior + duração; antes do primeiro pts lido, nenhum.
const PTS_DO_ANTERIOR: &str =
    r"setts=pts=if(eq(PTS\,NOPTS)\,if(eq(PREV_OUTPTS\,NOPTS)\,NOPTS\,PREV_OUTPTS+DURATION)\,PTS)";
const FORMATO_POSICOES: &str = "sicro.tira.posicoes/1";

/// Pedido da tira. Faixa `[de_s, ate_s)` em tempo de fonte; `n` miniaturas ou `largura` da faixa na tela (px; n =
/// largura ÷ largura da célula, preso aos tetos); `altura` de cada miniatura (px). `canal` = quem pede (o clipe na linha
/// do tempo): o pedido novo do mesmo canal cancela o anterior que ainda roda. Vazio = sem cancelamento.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PedidoTira {
    pub sha256: String,
    pub de_s: f64,
    pub ate_s: f64,
    pub n: Option<u32>,
    pub largura: Option<u32>,
    pub altura: u32,
    pub canal: String,
}

/// Célula da imagem: o quadro-chave (pts na base da tabela da mídia; `t` em s de fonte).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CelulaTira {
    pub pts: i64,
    pub t: f64,
}

/// Tira pronta. `relativo` = JPEG no cache do caso, `n`·`w` por `h`: a célula c em x ∈ [c·w, (c+1)·w), as além de
/// `celulas.len()` pretas. `miniaturas[k]` = célula da janela k (`n` itens). Nenhum quadro-chave na faixa: `relativo`
/// nulo e listas vazias.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TiraMiniaturas {
    pub relativo: Option<String>,
    pub w: u32,
    pub h: u32,
    pub n: u32,
    pub de_s: f64,
    pub ate_s: f64,
    pub celulas: Vec<CelulaTira>,
    pub miniaturas: Vec<u32>,
}

/// Resposta de `edicao_tira`. `tira` nula = pedida à fila de fundo: a tela pede de novo quando `edicao:fundo` der
/// `pronta` para `chave` (ou `cancelada`, se ainda quiser essa tira).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RespostaTira {
    pub chave: String,
    pub tira: Option<TiraMiniaturas>,
}

#[derive(Debug, Clone, PartialEq)]
enum Modo {
    Tempo,
    /// Número de ordem (entre os quadros-chave, desde o começo do arquivo) e célula de cada escolhido.
    Ordem { ordinais: Vec<usize>, celulas: Vec<CelulaTira> },
}

/// O pedido resolvido contra a mídia: janelas, célula, busca, modo e a chave do cache.
#[derive(Debug, Clone, PartialEq)]
struct Plano {
    de: f64,
    ate: f64,
    /// Início da miniatura 0: o quadro-chave exibido em `de` (≤ `de`), ou `de`.
    t0: f64,
    passo: f64,
    n: u32,
    w: u32,
    h: u32,
    /// `-ss` (s, relativo ao início do arquivo) pela regra do §5.3; None = sem busca.
    busca: Option<f64>,
    modo: Modo,
    /// Com a tabela do decodificador (modo Tempo ou Ordem): os quadros-chave que a regra escolhe nela (a leitura por
    /// tempo tem de dar esses).
    esperado: Option<Vec<usize>>,
    hash: String,
}

fn erro(t: impl Into<String>) -> SicroError {
    SicroError::Validation(t.into())
}

fn par(v: f64) -> u32 {
    ((v / 2.0).round().max(1.0) * 2.0) as u32
}

/// Janela do instante `t` (a conta do `select`, em `f64`): a de antes de `de` é a 0.
fn janela(t: f64, de: f64, passo: f64) -> f64 {
    ((t - de) / passo + FOLGA_JANELA).floor().max(0.0)
}

/// A regra das miniaturas sobre os quadros-chave da tabela (modo Ordem): índices escolhidos, em ordem.
fn escolher(tab: &TabelaQuadros, k0: Option<usize>, de: f64, ate: f64, passo: f64, n: u32) -> Vec<usize> {
    let mut sel: Vec<(f64, usize)> = k0.map(|j| (0.0, j)).into_iter().collect();
    for j in (0..tab.pts.len()).filter(|&j| tab.chave[j] && k0.is_none_or(|k| j > k)) {
        let t = tab.tempo(j);
        let k = janela(t, de, passo);
        if t >= de && t < ate && k < n as f64 && sel.last().is_none_or(|&(kk, _)| k > kk) {
            sel.push((k, j));
        }
    }
    sel.into_iter().map(|(_, j)| j).collect()
}

fn planejar(cap_sha256: &str, fonte: &FonteInfo, p: &PedidoTira) -> Result<Plano> {
    if !sha_valido(&p.sha256) {
        return Err(erro(format!("sha256_invalido: {}", p.sha256)));
    }
    if !(p.de_s.is_finite() && p.ate_s.is_finite() && p.ate_s > p.de_s) {
        return Err(erro(format!("item_invalido: faixa {} a {}", p.de_s, p.ate_s)));
    }
    if !(MIN_ALTURA..=MAX_ALTURA).contains(&p.altura) {
        return Err(erro(format!("item_invalido: altura {}", p.altura)));
    }
    let (ew, eh) = fonte.exibido;
    if ew == 0 || eh == 0 || conteiner_de_imagem(&fonte.conteiner) || fonte.quadros.pts.is_empty() {
        return Err(erro("nao_suportado: tira sem vídeo"));
    }
    // nunca amplia a fonte
    let h = par(p.altura.min(eh) as f64);
    let w = par(h as f64 * ew as f64 / eh as f64);
    let n = match (p.n, p.largura) {
        (Some(n), _) => n,
        // clipe largo na tela (zoom): menos miniaturas, não erro
        (None, Some(l)) => ((l as f64 / w as f64).round() as u32).clamp(1, MAX_MINIATURAS.min(MAX_LARGURA / w)),
        (None, None) => return Err(erro("item_invalido: n ou largura")),
    };
    if n == 0 || n > MAX_MINIATURAS {
        return Err(erro(format!("item_invalido: {n} miniaturas")));
    }
    if n as u64 * w as u64 > MAX_LARGURA as u64 {
        return Err(erro(format!("nao_suportado: tira de {} px", n as u64 * w as u64)));
    }
    // tabela de pacotes de contêiner sem tempo nos pacotes: tempos inventados, quadros-chave incertos
    if !previa::marcavel(fonte) {
        return Err(erro("tabela_pendente: tira"));
    }
    let tab = &fonte.quadros;
    let (de, ate) = (p.de_s, p.ate_s);
    let passo = (ate - de) / n as f64;
    let k0 = tab.chave_ate(tab.exibido(de)).filter(|&k| tab.tempo(k) <= de);
    let t0 = k0.map_or(de, |k| tab.tempo(k));
    let (modo, busca, esperado) = if sem_tempo_nos_pacotes(&fonte.conteiner) {
        let escolhidos = escolher(tab, k0, de, ate, passo, n);
        // os escolhidos vêm em ordem: uma passada só (gravação de 24 h tem milhões de quadros, a cada pedido da tela)
        let (mut ordinais, mut conta, mut j0) = (Vec::with_capacity(escolhidos.len()), 0, 0);
        for &j in &escolhidos {
            conta += tab.chave[j0..j].iter().filter(|&&k| k).count();
            j0 = j;
            ordinais.push(conta);
        }
        let celulas = escolhidos.iter().map(|&j| CelulaTira { pts: tab.pts[j], t: tab.tempo(j) }).collect();
        (Modo::Ordem { ordinais, celulas }, None, Some(escolhidos))
    } else {
        let esperado = (tab.origem == OrigemTabela::Decodificador).then(|| escolher(tab, k0, de, ate, passo, n));
        (Modo::Tempo, k0.and_then(|k| tab.busca_em(fonte.inicio_s, k)), esperado)
    };
    let letra = if modo == Modo::Tempo { "tempo" } else { "ordem" };
    let hash = sha256_bytes(format!("{FORMATO}\n{VERSAO}\n{}\n{cap_sha256}\n{letra}\n{t0}\n{de}\n{ate}\n{n}\n{w}x{h}\n", fonte.sha256).as_bytes());
    Ok(Plano { de, ate, t0, passo, n, w, h, busca, modo, esperado, hash })
}

/// O grafo da receita B. Tempo: `trim` acaba a leitura no fim da faixa e o `select` guarda em `ld(0)` a próxima
/// janela ainda sem célula (as variáveis do `st`/`ld` duram entre os quadros). Ordem: o `select` pega os números de
/// ordem escolhidos e o `tile` fecha no último (o `-frames:v 1` acaba a leitura). `showinfo@ti` conta as células.
/// `por_tempo` = o grafo do modo Tempo também no modo Ordem (o atalho).
fn grafo(fonte: &FonteInfo, p: &Plano, por_tempo: bool) -> String {
    let m = match fonte.matriz {
        Matriz::Bt601 => "bt601",
        Matriz::Bt709 => "bt709",
    };
    let r = match fonte.faixa {
        Faixa::Tv => "tv",
        Faixa::Pc => "pc",
    };
    let (de, ate, passo, n, w, h) = (p.de, p.ate, p.passo, p.n, p.w, p.h);
    let reduzir = format!(
        "showinfo@ti=checksum=0,scale={w}:{h}:flags=fast_bilinear:in_color_matrix={m}:in_range={r}:out_color_matrix=bt601:out_range=pc,setsar=1,format=yuvj420p"
    );
    match &p.modo {
        Modo::Ordem { ordinais, .. } if !por_tempo => {
            let quais: Vec<String> = ordinais.iter().map(|o| format!("eq(n,{o})")).collect();
            let largura = n * w;
            format!("[0:v:0]select='{}',{reduzir},tile={}x1,pad={largura}:{h}:0:0:black[v]", quais.join("+"), ordinais.len())
        }
        _ => {
            let t0 = p.t0 - FOLGA_T0_S;
            format!(
                "[0:v:0]trim=end={ate},\
                 select='if(gte(t,{t0})*lt(t,{ate}),if(gte(st(1,max(0,floor((t-({de}))/({passo})+{FOLGA_JANELA}))),ld(0))*lt(ld(1),{n}),st(0,ld(1)+1),0),0)',\
                 {reduzir},tile={n}x1[v]"
            )
        }
    }
}

/// Célula de cada miniatura: a última célula com janela ≤ k (janela vazia repete a anterior); antes da primeira, a
/// primeira. `ts` = instante de cada célula pela conta do `select`.
fn mapa(ts: &[f64], de: f64, passo: f64, n: u32) -> Vec<u32> {
    if ts.is_empty() {
        return Vec::new();
    }
    let janelas: Vec<f64> = ts.iter().map(|&t| janela(t, de, passo)).collect();
    (0..n).map(|k| janelas.iter().rposition(|&j| j <= k as f64).unwrap_or(0) as u32).collect()
}

/// Onde o atalho do modo Ordem começa a ler.
#[derive(Debug, Clone, PartialEq)]
enum Partida {
    /// MPEG-PS: `-ss` (s, relativo ao início do arquivo).
    Busca(f64),
    /// Fluxo cru, pela lista do `concat` que começa no byte do pacote do quadro j: pts = pts[j] + N·`passo`.
    Byte { pts0: i64, passo: i64 },
}

/// Lista do `concat` que abre a mídia a partir do byte `pos` (o `concat` faz o ffmpeg usar o pts dos pacotes).
const LISTA: &str = "partida.ffconcat";

fn lista_do_concat(video: &Path, pos: u64) -> String {
    // aspas simples: nada escapa lá dentro; a aspa do nome sai e volta escapada
    let nome = video.to_string_lossy().replace('\'', r"'\''");
    format!("ffconcat version 1.0\nfile '{nome}'\noption skip_initial_bytes {pos}\n")
}

fn comando(cap: &Capacidades, fonte: &FonteInfo, p: &Plano, pasta: &Path, partida: Option<&Partida>) -> exec::Comando {
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-nostats", "-y", "-loglevel", "info", "-copyts", "-skip_frame", "nokey"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let video = fonte.caminho.to_string_lossy().into_owned();
    match partida {
        None => {
            if let Some(alvo) = p.busca {
                a.extend(["-ss".to_string(), format!("{alvo:.6}")]);
            }
            a.extend(["-i".to_string(), video]);
        }
        Some(Partida::Busca(alvo)) => a.extend(["-ss".into(), format!("{alvo:.6}"), "-bsf:v".into(), PTS_DO_ANTERIOR.into(), "-i".into(), video]),
        Some(Partida::Byte { pts0, passo }) => {
            let setts = format!("setts=pts={pts0}+N*{passo}:dts={pts0}+N*{passo}");
            a.extend(["-f", "concat", "-safe", "0", "-bsf:v", &setts, "-i", LISTA].iter().map(|s| s.to_string()));
        }
    }
    a.extend(["-filter_complex".to_string(), grafo(fonte, p, partida.is_some())]);
    a.extend(["-map", "[v]", "-frames:v", "1", "-update", "1", "-c:v", "mjpeg", "-q:v", QUALIDADE, "-f", "image2", "tira.jpg"].iter().map(|s| s.to_string()));
    let nome = if partida.is_some() { "tira-atalho" } else { "tira" };
    exec::Comando {
        nome: nome.into(),
        programa: cap.binario.clone(),
        args: a,
        pasta: pasta.to_path_buf(),
        ambiente: Vec::new(),
        sem_ambiente: Vec::new(),
        stderr: pasta.join(format!("{nome}.stderr")),
    }
}

/// Base de tempo do `showinfo@ti` ("config in time_base: a/b").
fn base_do_showinfo(stderr: &Path) -> Option<Racional> {
    let texto = std::fs::read_to_string(stderr).ok()?;
    texto.lines().find_map(|l| {
        let resto = l.strip_prefix("[showinfo@ti @ ")?.split_once("] ")?.1;
        let (num, den) = resto.strip_prefix("config in time_base:")?.split(',').next()?.trim().split_once('/')?;
        let (num, den) = (num.parse().ok()?, den.parse().ok()?);
        (num > 0 && den > 0).then_some(Racional { num, den })
    })
}

fn arquivos(cache: &Path, sha256: &str, hash: &str) -> (PathBuf, PathBuf, String) {
    let rel = format!("{PASTA}/{sha256}/{hash}.jpg");
    let pasta = cache.join(PASTA).join(sha256);
    (pasta.join(format!("{hash}.jpg")), pasta.join(format!("{hash}.json")), rel)
}

#[derive(Serialize, Deserialize)]
struct Arquivo {
    formato: String,
    midia: String,
    chave: String,
    tira: TiraMiniaturas,
}

/// Tira no cache: o `.json` (gravado por último) e o JPEG que ele cita.
fn consultar(cache: &Path, sha256: &str, hash: &str) -> Option<TiraMiniaturas> {
    if !sha_valido(sha256) || !sha_valido(hash) {
        return None;
    }
    let (jpg, json, rel) = arquivos(cache, sha256, hash);
    let a: Arquivo = serde_json::from_slice(&std::fs::read(json).ok()?).ok()?;
    let certo = a.formato == FORMATO && a.midia == sha256 && a.chave == hash;
    let imagem = match &a.tira.relativo {
        Some(r) => *r == rel && jpg.is_file(),
        None => a.tira.celulas.is_empty(),
    };
    (certo && imagem).then_some(a.tira)
}

/// Pasta de trabalho `tiras/<chave>.tmp-<pid>-<n>` (a limpeza do cache tira as de processo morto), apagada no fim.
struct Trabalho(PathBuf);

impl Trabalho {
    fn nova(cache: &Path, hash: &str) -> Result<Self> {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = cache.join(PASTA).join(format!("{hash}.tmp-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p)?;
        Ok(Trabalho(p))
    }
}

impl Drop for Trabalho {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Por onde a tira saiu.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Caminho {
    Cache,
    Tempo,
    Ordem,
    /// Atalho do modo Ordem: MPEG-PS pelo `-ss`.
    Busca,
    /// Atalho do modo Ordem: fluxo cru pelo byte.
    Byte,
}

fn cancelada() -> SicroError {
    erro(format!("{ERRO_CANCELADO}: tira"))
}

/// Roda a tira e devolve o pts de cada célula como o `showinfo@ti` o viu. Nenhuma célula: lista vazia.
fn rodar_tira(cmd: &exec::Comando, cancelar: &AtomicBool, imagem: &Path) -> Result<Vec<i64>> {
    let fim = exec::rodar(cmd, cancelar)?;
    if fim.cancelado || cancelar.load(Ordering::SeqCst) {
        return Err(cancelada());
    }
    let pts = exec::pts_lidos(&cmd.stderr)?.remove("ti").unwrap_or_default();
    if pts.is_empty() {
        // nenhum quadro-chave na faixa: o 9.x sai com erro por não gravar nada, o 7.0 com 0
        let nada = exec::ultimas_linhas(&cmd.stderr, 5).iter().any(|l| l.contains("Nothing was written") || l.contains("Output file is empty"));
        if !(fim.codigo == Some(0) || nada) {
            exec::exigir_sucesso(cmd, &fim)?;
        }
    } else {
        exec::exigir_sucesso(cmd, &fim)?;
        if !std::fs::read(imagem).is_ok_and(|b| b.starts_with(&[0xFF, 0xD8, 0xFF])) {
            return Err(erro("outra: tira sem imagem"));
        }
    }
    Ok(pts)
}

/// Leitura por tempo: no máximo uma célula por janela, em ordem; com a tabela do decodificador, as células são as que a
/// regra escolhe nela (pts que o `-skip_frame` entregue errado vira `leitura_divergente`, não miniatura faltando ou fora
/// do lugar). Devolve as células e o instante de cada uma como o `select` o viu (pts × base).
fn conferir_tempo(fonte: &FonteInfo, p: &Plano, stderr: &Path, pts: &[i64]) -> Result<(Vec<CelulaTira>, Vec<f64>)> {
    if pts.len() > p.n as usize || pts.contains(&i64::MIN) || pts.windows(2).any(|w| w[1] <= w[0]) {
        return Err(erro(format!("leitura_divergente: tira com {} células para {} janelas", pts.len(), p.n)));
    }
    let tb = base_do_showinfo(stderr).unwrap_or(fonte.quadros.tb);
    let ts: Vec<f64> = pts.iter().map(|&x| x as f64 * tb.num as f64 / tb.den as f64).collect();
    if let Some(js) = &p.esperado {
        let tab = &fonte.quadros;
        let meio_tique = 0.5 * tab.tb.num as f64 / tab.tb.den as f64;
        if js.len() != ts.len() || js.iter().zip(&ts).any(|(&j, &t)| (tab.tempo(j) - t).abs() > meio_tique) {
            return Err(erro(format!("leitura_divergente: tira com {} células, a tabela pede {}", ts.len(), js.len())));
        }
    }
    Ok((pts.iter().zip(&ts).map(|(&x, &t)| CelulaTira { pts: x, t }).collect(), ts))
}

/// Faz e guarda a tira (bloqueia a thread). A do cache volta sem ffmpeg.
fn produzir(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, p: &Plano, cancelar: &AtomicBool) -> Result<TiraMiniaturas> {
    produzir_por(cap, cache, fonte, p, cancelar, PERTO).map(|(t, _)| t)
}

/// `produzir`, dizendo por onde saiu; `perto` = o limite do atalho do modo Ordem (os testes o forçam com 0).
fn produzir_por(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, p: &Plano, cancelar: &AtomicBool, perto: usize) -> Result<(TiraMiniaturas, Caminho)> {
    if let Some(t) = consultar(cache, &fonte.sha256, &p.hash) {
        return Ok((t, Caminho::Cache));
    }
    if cancelar.load(Ordering::SeqCst) {
        return Err(cancelada());
    }
    let trab = Trabalho::nova(cache, &p.hash)?;
    let imagem = trab.0.join("tira.jpg");
    let (celulas, ts, caminho) = match &p.modo {
        // nada escolhido na tabela: nada a decodificar
        Modo::Ordem { ordinais, .. } if ordinais.is_empty() => (Vec::new(), Vec::new(), Caminho::Ordem),
        Modo::Tempo => {
            let cmd = comando(cap, fonte, p, &trab.0, None);
            let pts = rodar_tira(&cmd, cancelar, &imagem)?;
            let (c, t) = conferir_tempo(fonte, p, &cmd.stderr, &pts)?;
            (c, t, Caminho::Tempo)
        }
        Modo::Ordem { ordinais, celulas } => match atalho(cap, cache, fonte, p, &trab.0, cancelar, perto)? {
            Some(r) => r,
            None => {
                let _ = std::fs::remove_file(&imagem);
                let cmd = comando(cap, fonte, p, &trab.0, None);
                let pts = rodar_tira(&cmd, cancelar, &imagem)?;
                if pts.len() != ordinais.len() {
                    return Err(erro(format!("leitura_divergente: tira com {} de {} quadros-chave", pts.len(), ordinais.len())));
                }
                (celulas.clone(), celulas.iter().map(|c| c.t).collect(), Caminho::Ordem)
            }
        },
    };
    let (jpg, json, rel) = arquivos(cache, &fonte.sha256, &p.hash);
    let tira = TiraMiniaturas {
        relativo: (!celulas.is_empty()).then_some(rel),
        w: p.w,
        h: p.h,
        n: p.n,
        de_s: p.de,
        ate_s: p.ate,
        miniaturas: mapa(&ts, p.de, p.passo, p.n),
        celulas,
    };
    let a = Arquivo { formato: FORMATO.into(), midia: fonte.sha256.clone(), chave: p.hash.clone(), tira };
    // a imagem antes do `.json`: o `.json` no lugar é a marca de tira inteira
    std::fs::create_dir_all(jpg.parent().expect("pasta da tira"))?;
    std::fs::write(trab.0.join("tira.json"), serde_json::to_vec(&a)?)?;
    if a.tira.relativo.is_some() {
        std::fs::rename(&imagem, &jpg)?;
    }
    std::fs::rename(trab.0.join("tira.json"), &json)?;
    Ok((a.tira, caminho))
}

/// Mídias (sha256 da mídia e do ffmpeg) em que o atalho não bateu com a tabela: vão direto ao modo Ordem.
fn divergentes() -> &'static Mutex<HashSet<String>> {
    static D: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    D.get_or_init(Default::default)
}

/// Passo dos pts quando a tabela é uma progressão (fluxo cru: o ffmpeg conta os quadros saídos); senão None.
fn passo_constante(tab: &TabelaQuadros) -> Option<i64> {
    let d = tab.pts.get(1)? - tab.pts[0];
    (d > 0 && tab.pts.windows(2).all(|w| w[1] - w[0] == d)).then_some(d)
}

/// Células, o instante de cada uma e por onde saíram.
type Lida = (Vec<CelulaTira>, Vec<f64>, Caminho);

/// Atalho do modo Ordem (ver o começo do arquivo). None = não vale aqui (faixa perto do começo, mídia sem posições,
/// tabela que não é progressão, mídia que já divergiu) ou a leitura não bateu com a tabela (a mídia fica marcada).
fn atalho(
    cap: &Capacidades,
    cache: &Path,
    fonte: &FonteInfo,
    p: &Plano,
    pasta: &Path,
    cancelar: &AtomicBool,
    perto: usize,
) -> Result<Option<Lida>> {
    let (Modo::Ordem { ordinais, .. }, Some(js)) = (&p.modo, &p.esperado) else { return Ok(None) };
    let (Some(&o0), Some(&j0)) = (ordinais.first(), js.first()) else { return Ok(None) };
    let marca = format!("{}:{}", fonte.sha256, cap.sha256);
    if o0 < perto || divergentes().lock().unwrap_or_else(|e| e.into_inner()).contains(&marca) {
        return Ok(None);
    }
    let tab = &fonte.quadros;
    let (partida, caminho) = if fonte.conteiner.split(',').any(|c| c == "mpeg") {
        let alvo = tab.tempo(j0) - fonte.inicio_s - RECUO_S;
        if alvo <= 0.0 {
            return Ok(None);
        }
        (Partida::Busca(alvo), Caminho::Busca)
    } else {
        let Some(passo) = passo_constante(tab) else { return Ok(None) };
        let Some(pos) = posicoes(cap, cache, fonte, cancelar)? else { return Ok(None) };
        // o último quadro-chave até o primeiro da tira
        let Some(&(j, byte)) = pos.ancoras.partition_point(|&(j, _)| j <= j0).checked_sub(1).map(|k| &pos.ancoras[k]) else { return Ok(None) };
        std::fs::write(pasta.join(LISTA), lista_do_concat(&fonte.caminho, byte))?;
        (Partida::Byte { pts0: tab.pts[j], passo }, Caminho::Byte)
    };
    let cmd = comando(cap, fonte, p, pasta, Some(&partida));
    let lido = rodar_tira(&cmd, cancelar, &pasta.join("tira.jpg")).and_then(|pts| conferir_tempo(fonte, p, &cmd.stderr, &pts));
    match lido {
        Ok((c, t)) => Ok(Some((c, t, caminho))),
        Err(e) if cancelar.load(Ordering::SeqCst) => Err(e),
        Err(e) => {
            tracing::warn!("tira: atalho {partida:?} não bateu ({e}); modo Ordem desde o começo");
            divergentes().lock().unwrap_or_else(|e| e.into_inner()).insert(marca);
            Ok(None)
        }
    }
}

/// Posições dos pacotes de vídeo de um fluxo cru (`ffprobe`, uma vez por mídia e ffmpeg; `tiras/<sha256>/`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Posicoes {
    formato: String,
    midia: String,
    ffmpeg: String,
    /// Pacotes lidos sem os de descarte.
    pacotes: usize,
    /// Cada pacote começa onde o anterior acaba (`pos` = soma dos tamanhos): o byte de cada um é o do índice dele.
    contiguo: bool,
    /// Índices dos pacotes de quadro-chave.
    chaves: Vec<usize>,
    /// (índice, byte) de cada pacote de quadro-chave, em ordem.
    ancoras: Vec<(usize, u64)>,
}

impl Posicoes {
    /// Valem para esta tabela: mesmo número de pacotes e quadros, quadros-chave nos mesmos índices, bytes contíguos.
    fn valem(&self, tab: &TabelaQuadros) -> bool {
        self.contiguo
            && self.pacotes == tab.pts.len()
            && self.chaves.len() == self.ancoras.len()
            && self.chaves.iter().copied().eq((0..tab.chave.len()).filter(|&j| tab.chave[j]))
    }
}

enum Medicao {
    Medindo,
    Prontas(Arc<Posicoes>),
    Sem,
}

fn medicoes() -> &'static (Mutex<HashMap<String, Medicao>>, Condvar) {
    static M: OnceLock<(Mutex<HashMap<String, Medicao>>, Condvar)> = OnceLock::new();
    M.get_or_init(Default::default)
}

fn arquivo_posicoes(cache: &Path, sha256: &str, ffmpeg: &str) -> PathBuf {
    cache.join(PASTA).join(sha256).join(format!("posicoes-{ffmpeg}.json"))
}

fn ler_posicoes(arquivo: &Path) -> Option<Posicoes> {
    serde_json::from_slice::<Posicoes>(&std::fs::read(arquivo).ok()?).ok().filter(|p| p.formato == FORMATO_POSICOES)
}

fn gravar_posicoes(arquivo: &Path, p: &Posicoes) -> Result<()> {
    std::fs::create_dir_all(arquivo.parent().expect("pasta das posições"))?;
    let tmp = arquivo.with_extension(format!("json.tmp-{}", std::process::id()));
    std::fs::write(&tmp, serde_json::to_vec(p)?)?;
    std::fs::rename(&tmp, arquivo).inspect_err(|_| drop(std::fs::remove_file(&tmp)))?;
    Ok(())
}

/// Lê as linhas `size,pos,flags` do `ffprobe` aos pedaços.
struct LeitorPosicoes {
    resto: Vec<u8>,
    pacotes: usize,
    /// Onde começa o próximo pacote (byte), pela soma dos tamanhos.
    proximo: Option<u64>,
    contiguo: bool,
    chaves: Vec<usize>,
    ancoras: Vec<(usize, u64)>,
}

impl Default for LeitorPosicoes {
    fn default() -> Self {
        LeitorPosicoes { resto: Vec::new(), pacotes: 0, proximo: None, contiguo: true, chaves: Vec::new(), ancoras: Vec::new() }
    }
}

impl LeitorPosicoes {
    fn ler(&mut self, b: &[u8]) {
        self.resto.extend_from_slice(b);
        let mut ini = 0;
        while let Some(i) = self.resto[ini..].iter().position(|&c| c == b'\n') {
            let linha = String::from_utf8_lossy(&self.resto[ini..ini + i]).into_owned();
            self.linha(&linha);
            ini += i + 1;
        }
        self.resto.drain(..ini);
    }

    fn linha(&mut self, l: &str) {
        let mut c = l.trim().split(',');
        let (Some(tamanho), Some(pos), Some(flags)) = (c.next(), c.next(), c.next()) else { return };
        let Ok(tamanho) = tamanho.trim().parse::<u64>() else {
            self.contiguo = false;
            return;
        };
        let pos = pos.trim().parse::<u64>().ok();
        if pos.is_some() && self.proximo.is_some() && pos != self.proximo {
            self.contiguo = false;
        }
        let inicio = pos.or(self.proximo);
        self.proximo = inicio.map(|i| i + tamanho);
        // o pacote de descarte ocupa bytes mas não vira quadro (como na tabela de pacotes)
        if flags.contains('D') {
            return;
        }
        if flags.contains('K') {
            self.chaves.push(self.pacotes);
            match inicio {
                Some(i) => self.ancoras.push((self.pacotes, i)),
                None => self.contiguo = false,
            }
        }
        self.pacotes += 1;
    }

    fn posicoes(self, midia: &str, ffmpeg: &str) -> Posicoes {
        Posicoes {
            formato: FORMATO_POSICOES.into(),
            midia: midia.into(),
            ffmpeg: ffmpeg.into(),
            pacotes: self.pacotes,
            contiguo: self.contiguo,
            chaves: self.chaves,
            ancoras: self.ancoras,
        }
    }
}

/// Mede as posições do vídeo com o `ffprobe` (só demultiplexa: lê o arquivo, não decodifica).
fn medir_posicoes(ffprobe: &Path, video: &Path, cache: &Path, midia: &str, ffmpeg: &str) -> Result<Posicoes> {
    // só o fim do programa mata esta leitura: a tira que a pediu pode ser cancelada no meio
    static NUNCA: AtomicBool = AtomicBool::new(false);
    let trab = Trabalho::nova(cache, &format!("posicoes-{midia}"))?;
    let args = ["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=size,pos,flags", "-of", "csv=p=0"];
    let cmd = exec::Comando {
        nome: "tira-posicoes".into(),
        programa: ffprobe.to_path_buf(),
        args: args.iter().map(|s| s.to_string()).chain([video.to_string_lossy().into_owned()]).collect(),
        pasta: trab.0.clone(),
        ambiente: Vec::new(),
        sem_ambiente: Vec::new(),
        stderr: trab.0.join("posicoes.stderr"),
    };
    let mut leitor = LeitorPosicoes::default();
    let fim = exec::rodar_com_fluxo(&cmd, &NUNCA, &mut |b| {
        leitor.ler(b);
        Ok(true)
    })?;
    exec::exigir_sucesso(&cmd, &fim)?;
    let resto = String::from_utf8_lossy(&std::mem::take(&mut leitor.resto)).into_owned();
    leitor.linha(&resto);
    Ok(leitor.posicoes(midia, ffmpeg))
}

/// Posições do fluxo cru: da memória, do cache, ou medidas numa thread própria (a tira cancelada a cada zoom não perde
/// a medição; a seguinte espera por ela). Espera enquanto a tira não for cancelada. None = não valem para a tabela.
fn posicoes(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, cancelar: &AtomicBool) -> Result<Option<Arc<Posicoes>>> {
    if !sha_valido(&fonte.sha256) || !sha_valido(&cap.sha256) {
        return Ok(None);
    }
    let valem = |p: &Posicoes| p.midia == fonte.sha256 && p.ffmpeg == cap.sha256 && p.valem(&fonte.quadros);
    let chave = format!("{}:{}", fonte.sha256, cap.sha256);
    let (m, cv) = medicoes();
    let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        match g.get(&chave) {
            Some(Medicao::Prontas(p)) => return Ok(valem(p).then(|| p.clone())),
            Some(Medicao::Sem) => return Ok(None),
            Some(Medicao::Medindo) => {}
            None => {
                let arquivo = arquivo_posicoes(cache, &fonte.sha256, &cap.sha256);
                if let Some(p) = ler_posicoes(&arquivo) {
                    g.insert(chave.clone(), Medicao::Prontas(Arc::new(p)));
                    continue;
                }
                g.insert(chave.clone(), Medicao::Medindo);
                let (ffprobe, video, cache, sha, ffmpeg, ch) =
                    (cap.ffprobe.clone(), fonte.caminho.clone(), cache.to_path_buf(), fonte.sha256.clone(), cap.sha256.clone(), chave.clone());
                let medir = move || {
                    let r = std::panic::catch_unwind(|| medir_posicoes(&ffprobe, &video, &cache, &sha, &ffmpeg));
                    let p = match r {
                        Ok(Ok(p)) => {
                            if let Err(e) = gravar_posicoes(&arquivo, &p) {
                                tracing::warn!("tira: posições não gravadas: {e}");
                            }
                            Medicao::Prontas(Arc::new(p))
                        }
                        Ok(Err(e)) => {
                            tracing::warn!("tira: posições: {e}");
                            Medicao::Sem
                        }
                        Err(_) => Medicao::Sem,
                    };
                    let (m, cv) = medicoes();
                    m.lock().unwrap_or_else(|e| e.into_inner()).insert(ch, p);
                    cv.notify_all();
                };
                if std::thread::Builder::new().name("tira-posicoes".into()).spawn(medir).is_err() {
                    g.insert(chave.clone(), Medicao::Sem);
                    return Ok(None);
                }
            }
        }
        if cancelar.load(Ordering::SeqCst) {
            return Err(cancelada());
        }
        g = cv.wait_timeout(g, Duration::from_millis(20)).unwrap_or_else(|e| e.into_inner()).0;
    }
}

/// Chave que cada canal (o clipe na tela) ainda espera da fila de fundo.
fn canais() -> &'static Mutex<HashMap<String, String>> {
    static C: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// Troca o que o canal espera (`None` = nada: tira do cache ou pedido recusado) e cancela o que ele esperava antes, se
/// nenhum outro canal ainda o quer.
fn trocar(m: &mut HashMap<String, String>, canal: &str, chave: Option<&str>) {
    if canal.is_empty() {
        return;
    }
    let velha = match chave {
        Some(c) => m.insert(canal.to_string(), c.to_string()),
        None => m.remove(canal),
    };
    if let Some(v) = velha.filter(|v| Some(v.as_str()) != chave && !m.values().any(|x| x == v)) {
        fundo::cancelar(&v);
    }
}

fn esquecer(chave: &str) {
    canais().lock().unwrap_or_else(|e| e.into_inner()).retain(|_, v| v != chave);
}

/// `edicao_tira`: a tira do cache, ou nula depois de pedir à fila de fundo (`tira:<chave>`, na frente das que esperam).
pub fn pedir(ws: &Path, cache: &Path, pedido: &PedidoTira) -> Result<RespostaTira> {
    if !sha_valido(&pedido.sha256) {
        return Err(erro(format!("sha256_invalido: {}", pedido.sha256)));
    }
    let (_, fonte, cap) = crate::commands::edicao_commands::fonte_do_caso(ws, &pedido.sha256, cache)?;
    pedir_com(cap, cache, fonte, pedido)
}

fn pedir_com(cap: Arc<Capacidades>, cache: &Path, fonte: FonteInfo, pedido: &PedidoTira) -> Result<RespostaTira> {
    let plano = planejar(&cap.sha256, &fonte, pedido);
    // troca e fila sob a mesma trava: a tela manda o pedido seguinte do canal sem esperar a resposta, e o que entrasse
    // na fila depois da troca do outro ficava rodando sem dono
    let mut m = canais().lock().unwrap_or_else(|e| e.into_inner());
    let p = match plano {
        Ok(p) => p,
        Err(e) => {
            trocar(&mut m, &pedido.canal, None);
            return Err(e);
        }
    };
    let chave = fundo::chave(TipoFundo::Tira, &p.hash);
    if let Some(t) = consultar(cache, &fonte.sha256, &p.hash) {
        trocar(&mut m, &pedido.canal, None);
        return Ok(RespostaTira { chave, tira: Some(t) });
    }
    trocar(&mut m, &pedido.canal, Some(&chave));
    let (cache, ch) = (cache.to_path_buf(), chave.clone());
    fundo::pedir_com_prioridade(
        TipoFundo::Tira,
        &chave,
        Box::new(move |c, _| {
            let r = produzir(&cap, &cache, &fonte, &p, c).map(|_| ());
            esquecer(&ch);
            r
        }),
    );
    Ok(RespostaTira { chave, tira: None })
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::fs::File;
    use std::time::{Duration, Instant};

    use crate::edicao::ffmpeg::sonda;
    use crate::edicao::fundo::EstadoFundo;
    use crate::edicao::quadros::{apoio, fonte_info, Busca, OrigemTabela, TabelaQuadros};
    use crate::edicao::testes::fontes::{self, Fonte, MATRIZ};
    use crate::edicao::testes::medir;

    /// Limites reais (s): 60 miniaturas de 10 min em 1080p (§10, receita B: 2,4 s no 9.0.1, 3,0 s no 7.0.2) e a morte do
    /// pedido velho. Os testes medem com folga de 3× enquanto houver outros agentes na máquina (O1).
    const SESSENTA_S: f64 = 2.4;
    const CANCELAR_S: f64 = 0.5;
    const FOLGA: f64 = 3.0;

    fn pedido(sha256: &str, de_s: f64, ate_s: f64, n: u32, altura: u32) -> PedidoTira {
        PedidoTira { sha256: sha256.into(), de_s, ate_s, n: Some(n), largura: None, altura, canal: String::new() }
    }

    fn codigo<T: std::fmt::Debug>(r: Result<T>) -> String {
        match r {
            Err(SicroError::Validation(m)) => m.split(':').next().unwrap_or_default().to_string(),
            outro => panic!("esperava erro: {outro:?}"),
        }
    }

    // ------------------------------------------------------------ sem ffmpeg

    #[test]
    fn plano_celula_quantidade_inicio_busca_e_chave() {
        let sha = "ab".repeat(32);
        let mut tab = apoio::tabela((1, 15360), apoio::pts_regulares(900, (1, 30), (1, 15360), 0));
        tab.busca = Busca::Confiavel;
        let f = FonteInfo { sha256: sha.clone(), ..apoio::fonte(tab.clone(), (1920, 1080)) };
        let p = planejar("f", &f, &pedido(&sha, 0.0, 30.0, 60, 90)).unwrap();
        assert_eq!((p.w, p.h, p.n, p.passo), (160, 90, 60, 0.5));
        // pela largura: 9600 px de 160 = 60; `n` vence a largura
        let por_largura = PedidoTira { n: None, largura: Some(9600), ..pedido(&sha, 0.0, 30.0, 1, 90) };
        assert_eq!(planejar("f", &f, &por_largura).unwrap().n, 60);
        assert_eq!(planejar("f", &f, &PedidoTira { n: Some(7), ..por_largura.clone() }).unwrap().n, 7);
        // clipe largo na tela (zoom): a largura dá menos miniaturas, nunca erro; `n` pedido acima do teto é recusado
        for l in [40_000, u32::MAX] {
            let p = planejar("f", &f, &PedidoTira { largura: Some(l), ..por_largura.clone() }).unwrap();
            assert_eq!((p.n, p.n * p.w), (102, 16_320), "{l}");
        }
        let baixa = planejar("f", &f, &PedidoTira { largura: Some(u32::MAX), altura: 16, ..por_largura.clone() }).unwrap();
        assert_eq!((baixa.w, baixa.n), (28, MAX_MINIATURAS));
        // nunca amplia; vertical e pixel não quadrado pela proporção exibida
        let pequena = FonteInfo { exibido: (320, 180), ..f.clone() };
        assert_eq!(planejar("f", &pequena, &pedido(&sha, 0.0, 30.0, 10, 360)).map(|p| (p.w, p.h)).unwrap(), (320, 180));
        let vertical = FonteInfo { exibido: (180, 320), ..f.clone() };
        assert_eq!(planejar("f", &vertical, &pedido(&sha, 0.0, 30.0, 10, 90)).map(|p| (p.w, p.h)).unwrap(), (50, 90));
        let pal = FonteInfo { exibido: (654, 480), ..f.clone() };
        assert_eq!(planejar("f", &pal, &pedido(&sha, 0.0, 30.0, 10, 90)).map(|p| (p.w, p.h)).unwrap(), (122, 90));
        // miniatura 0 parte do quadro-chave exibido em `de` (a cada 30 quadros = 1 s); busca só a 10 s do início
        let p = planejar("f", &f, &pedido(&sha, 12.4, 20.0, 10, 90)).unwrap();
        assert_eq!((p.t0, p.busca), (12.0, Some(11.5)));
        let p = planejar("f", &f, &pedido(&sha, 5.4, 20.0, 10, 90)).unwrap();
        assert_eq!((p.t0, p.busca), (5.0, None));
        assert_eq!(p.modo, Modo::Tempo);
        // contêiner sem tempo nos pacotes: pela tabela do decodificador, por ordem e sem busca; com a de pacotes, espera
        let ps = FonteInfo { conteiner: "mpeg".into(), ..f.clone() };
        let p = planejar("f", &ps, &pedido(&sha, 12.4, 20.0, 4, 90)).unwrap();
        // janelas de 1,9 s a partir de 12,4: 12 (miniatura 0) e o primeiro de cada janela seguinte: 15, 17, 19
        let celulas = [12.0, 15.0, 17.0, 19.0].map(|t| CelulaTira { pts: (t * 15360.0) as i64, t }).to_vec();
        assert_eq!((p.busca, p.modo), (None, Modo::Ordem { ordinais: vec![12, 15, 17, 19], celulas }));
        let mut pacotes = tab.clone();
        pacotes.origem = OrigemTabela::Pacotes;
        let ps_pacotes = FonteInfo { quadros: Arc::new(pacotes), ..ps.clone() };
        assert_eq!(codigo(planejar("f", &ps_pacotes, &pedido(&sha, 12.4, 20.0, 10, 90))), "tabela_pendente");
        // chave: muda com tudo o que muda a imagem (mídia, ffmpeg, faixa, n, altura, início, modo), não com o canal
        let base = planejar("f", &f, &pedido(&sha, 12.4, 20.0, 10, 90)).unwrap().hash;
        assert!(sha_valido(&base));
        assert_eq!(planejar("f", &f, &PedidoTira { canal: "c".into(), ..pedido(&sha, 12.4, 20.0, 10, 90) }).unwrap().hash, base);
        let outra = FonteInfo { sha256: "cd".repeat(32), ..f.clone() };
        for h in [
            planejar("e", &f, &pedido(&sha, 12.4, 20.0, 10, 90)),
            planejar("f", &outra, &pedido(&"cd".repeat(32), 12.4, 20.0, 10, 90)),
            planejar("f", &f, &pedido(&sha, 12.5, 20.0, 10, 90)),
            planejar("f", &f, &pedido(&sha, 12.4, 21.0, 10, 90)),
            planejar("f", &f, &pedido(&sha, 12.4, 20.0, 11, 90)),
            planejar("f", &f, &pedido(&sha, 12.4, 20.0, 10, 92)),
            planejar("f", &ps, &pedido(&sha, 12.4, 20.0, 10, 90)),
        ] {
            assert_ne!(h.unwrap().hash, base);
        }
        // recusas
        assert_eq!(codigo(planejar("f", &f, &pedido("../x", 0.0, 1.0, 10, 90))), "sha256_invalido");
        for (de, ate) in [(1.0, 1.0), (2.0, 1.0), (f64::NAN, 1.0), (0.0, f64::INFINITY)] {
            assert_eq!(codigo(planejar("f", &f, &pedido(&sha, de, ate, 10, 90))), "item_invalido", "{de} {ate}");
        }
        for (n, altura) in [(0, 90), (MAX_MINIATURAS + 1, 90), (10, MIN_ALTURA - 1), (10, MAX_ALTURA + 1)] {
            assert_eq!(codigo(planejar("f", &f, &pedido(&sha, 0.0, 1.0, n, altura))), "item_invalido", "{n} {altura}");
        }
        assert_eq!(codigo(planejar("f", &f, &PedidoTira { n: None, ..pedido(&sha, 0.0, 1.0, 1, 90) })), "item_invalido");
        assert_eq!(codigo(planejar("f", &f, &pedido(&sha, 0.0, 1.0, 103, 90))), "nao_suportado");
        let foto = FonteInfo { conteiner: "image2".into(), ..f.clone() };
        assert_eq!(codigo(planejar("f", &foto, &pedido(&sha, 0.0, 1.0, 10, 90))), "nao_suportado");
        let som = FonteInfo { exibido: (0, 0), ..f.clone() };
        assert_eq!(codigo(planejar("f", &som, &pedido(&sha, 0.0, 1.0, 10, 90))), "nao_suportado");
    }

    #[test]
    fn mapa_das_miniaturas() {
        // janelas de 1 s a partir de 10 s: células em 9,5 (miniatura 0), 12,2, 13,9 e 17,0
        assert_eq!(mapa(&[9.5, 12.2, 13.9, 17.0], 10.0, 1.0, 9), vec![0, 0, 1, 2, 2, 2, 2, 3, 3]);
        // janelas do começo vazias: a primeira célula
        assert_eq!(mapa(&[12.2, 13.9], 10.0, 1.0, 5), vec![0, 0, 0, 1, 1]);
        assert!(mapa(&[], 10.0, 1.0, 5).is_empty());
        // quadro-chave em cima da fronteira, numa conta que não é exata em binário (janelas de 0,1 s, base 1/1000, como
        // o `select` calcula: pts × (1/1000)): fica na janela que começa nele
        let ts: Vec<f64> = (0..60).map(|k| (k * 100) as f64 * (1.0 / 1000.0)).collect();
        assert!(ts.iter().enumerate().any(|(k, t)| (t / 0.1).floor() < k as f64), "conta exata: o caso não prova nada");
        assert_eq!(mapa(&ts, 0.0, 0.1, 60), (0..60).collect::<Vec<u32>>());
        // a mesma regra na escolha pela tabela (modo Ordem)
        let tab = apoio::tabela((1, 15360), apoio::pts_regulares(18000, (1, 30), (1, 15360), 0));
        let ordem = |n| escolher(&tab, Some(0), 0.0, 600.0, 600.0 / n as f64, n);
        assert_eq!(ordem(60), (0..60).map(|k| k * 300).collect::<Vec<usize>>());
        // GOP de 1 s e janelas de 0,25 s: só as janelas que começam num segundo inteiro têm célula
        let ts: Vec<f64> = ordem(2400).iter().map(|&j| tab.tempo(j)).collect();
        assert_eq!(ts.len(), 600);
        assert_eq!(mapa(&ts, 0.0, 0.25, 8), vec![0, 0, 0, 0, 1, 1, 1, 1]);
    }

    #[test]
    fn cache_so_vale_inteiro_e_da_mesma_chave() {
        let cache = fontes::pasta_temporaria("tiras-cache");
        let (sha, hash) = ("ab".repeat(32), "cd".repeat(32));
        let (jpg, json, rel) = arquivos(&cache, &sha, &hash);
        std::fs::create_dir_all(jpg.parent().unwrap()).unwrap();
        let tira = TiraMiniaturas {
            relativo: Some(rel.clone()),
            w: 160,
            h: 90,
            n: 2,
            de_s: 0.0,
            ate_s: 1.0,
            celulas: vec![CelulaTira { pts: 0, t: 0.0 }],
            miniaturas: vec![0, 0],
        };
        let grava = |formato: &str, midia: &str, chave: &str, t: &TiraMiniaturas| {
            let a = Arquivo { formato: formato.into(), midia: midia.into(), chave: chave.into(), tira: t.clone() };
            std::fs::write(&json, serde_json::to_vec(&a).unwrap()).unwrap();
        };
        grava(FORMATO, &sha, &hash, &tira);
        assert_eq!(consultar(&cache, &sha, &hash), None, "sem o JPEG");
        std::fs::write(&jpg, [0xFF, 0xD8, 0xFF]).unwrap();
        assert_eq!(consultar(&cache, &sha, &hash), Some(tira.clone()));
        assert_eq!(rel, format!("tiras/{sha}/{hash}.jpg"));
        grava("sicro.tira/0", &sha, &hash, &tira);
        assert_eq!(consultar(&cache, &sha, &hash), None);
        grava(FORMATO, &"ef".repeat(32), &hash, &tira);
        assert_eq!(consultar(&cache, &sha, &hash), None);
        grava(FORMATO, &sha, &"ef".repeat(32), &tira);
        assert_eq!(consultar(&cache, &sha, &hash), None);
        grava(FORMATO, &sha, &hash, &TiraMiniaturas { relativo: Some("../../x.jpg".into()), ..tira.clone() });
        assert_eq!(consultar(&cache, &sha, &hash), None);
        std::fs::write(&json, b"{").unwrap();
        assert_eq!(consultar(&cache, &sha, &hash), None);
        assert_eq!(consultar(&cache, "../..", &hash), None);
    }

    // ------------------------------------------------------------ com ffmpeg (os dois binários)

    fn comando_de_teste(cap: &Capacidades, nome: &str, args: &[&str]) -> exec::Comando {
        let p = fontes::pasta_temporaria(&format!("tiras-{nome}"));
        exec::Comando {
            nome: nome.into(),
            programa: cap.binario.clone(),
            args: args.iter().map(|s| s.to_string()).collect(),
            pasta: p.clone(),
            ambiente: Vec::new(),
            sem_ambiente: Vec::new(),
            stderr: p.join(format!("{nome}.stderr")),
        }
    }

    /// Luma de cada célula da tira (o Y do JPEG, sem conversão), na ordem.
    fn lumas(cap: &Capacidades, cache: &Path, t: &TiraMiniaturas) -> Vec<Vec<u8>> {
        let Some(rel) = &t.relativo else { return Vec::new() };
        let jpg = cache.join(rel).to_string_lossy().into_owned();
        let cmd = comando_de_teste(cap, "ler", &["-hide_banner", "-nostdin", "-loglevel", "error", "-i", &jpg, "-vf", "extractplanes=y", "-f", "rawvideo", "-pix_fmt", "gray", "-"]);
        let (fim, y) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).unwrap();
        exec::exigir_sucesso(&cmd, &fim).unwrap();
        let (w, h) = (t.w as usize, t.h as usize);
        assert_eq!(y.len(), t.n as usize * w * h, "imagem de {} bytes para {}×{w}×{h} ({})", y.len(), t.n, cap.versao);
        let largura = t.n as usize * w;
        (0..t.celulas.len()).map(|c| (0..h).flat_map(|r| y[r * largura + c * w..r * largura + (c + 1) * w].to_vec()).collect()).collect()
    }

    /// Células (índices na tabela) e mapa esperados, contados à parte pela tabela (quadro-chave = `tab.chave`).
    fn esperado(tab: &TabelaQuadros, de: f64, ate: f64, n: u32, com_k0: bool) -> (Vec<usize>, Vec<u32>) {
        let p = (ate - de) / n as f64;
        let chaves: Vec<usize> = (0..tab.pts.len()).filter(|&j| tab.chave[j]).collect();
        let k0 = if com_k0 { chaves.iter().copied().rfind(|&j| tab.tempo(j) <= de) } else { None };
        let mut sel: Vec<(u32, usize)> = k0.map(|j| (0, j)).into_iter().collect();
        for &j in &chaves {
            let t = tab.tempo(j);
            if t < de || t >= ate || k0.is_some_and(|k| j <= k) {
                continue;
            }
            let k = ((t - de) / p).floor() as u32;
            if sel.last().is_none_or(|&(kk, _)| k > kk) {
                sel.push((k, j));
            }
        }
        let mapa = (0..n).map(|k| sel.iter().rposition(|&(kk, _)| kk <= k).unwrap_or(0) as u32).collect();
        (sel.into_iter().map(|(_, j)| j).collect(), mapa)
    }

    /// Nenhum quadro-chave a menos de 2 ms de uma fronteira de janela, nenhum quadro a menos de 2 ms de `de`: a conta
    /// do teste e a do filtro não divergem no último bit.
    fn longe_das_fronteiras(tab: &TabelaQuadros, de: f64, ate: f64, n: u32) -> bool {
        let p = (ate - de) / n as f64;
        (0..tab.pts.len()).all(|j| {
            let t = tab.tempo(j);
            let x = (t - de) / p;
            let na_fronteira = tab.chave[j] && t >= de && t < ate && (x - x.round()).abs() * p < 0.002;
            (t - de).abs() >= 0.002 && !na_fronteira
        })
    }

    /// Faixa de até `dur` s a partir do meio do quadro `j`, afastada das fronteiras.
    fn faixa(tab: &TabelaQuadros, j: usize, dur: f64, n: u32) -> (f64, f64) {
        let fim = tab.tempo(tab.pts.len() - 2);
        let mut de = tab.tempo(j) + 0.4 * tab.duracao(j);
        for _ in 0..50 {
            let ate = (de + dur).min(fim);
            if longe_das_fronteiras(tab, de, ate, n) {
                return (de, ate);
            }
            de += 0.0037;
        }
        panic!("sem faixa longe das fronteiras a partir do quadro {j}");
    }

    /// Roda a tira e confere: as células são os quadros-chave esperados (pelo pts e pelo número desenhado), o mapa é o
    /// esperado e a célula tem a proporção exibida.
    #[allow(clippy::too_many_arguments)]
    fn conferir(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, numeros: &[u32], de: f64, ate: f64, n: u32, altura: u32, nome: &str) -> TiraMiniaturas {
        conferir_por(cap, cache, fonte, numeros, (de, ate, n, altura), nome, PERTO).0
    }

    /// `conferir` com o limite do atalho do modo Ordem; devolve também por onde a tira saiu.
    fn conferir_por(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, numeros: &[u32], (de, ate, n, altura): (f64, f64, u32, u32), nome: &str, perto: usize) -> (TiraMiniaturas, Caminho) {
        let p = planejar(&cap.sha256, fonte, &pedido(&fonte.sha256, de, ate, n, altura)).unwrap();
        let (t, caminho) = produzir_por(cap, cache, fonte, &p, &AtomicBool::new(false), perto).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
        let tab = &fonte.quadros;
        let (js, mapa) = esperado(tab, de, ate, n, true);
        let lidos: Vec<u32> = lumas(cap, cache, &t).iter().map(|y| medir::identificar(y, t.w as usize, t.h as usize)).collect();
        let msg = format!("{nome} ({}): de {de} ate {ate} n {n}", cap.versao);
        assert_eq!(t.celulas.iter().map(|c| c.pts).collect::<Vec<_>>(), js.iter().map(|&j| tab.pts[j]).collect::<Vec<_>>(), "pts das células: {msg}");
        assert_eq!(lidos, js.iter().map(|&j| numeros[j]).collect::<Vec<_>>(), "números nas células: {msg}");
        assert_eq!(t.miniaturas, mapa, "mapa: {msg}");
        let (ew, eh) = fonte.exibido;
        assert_eq!((t.w, t.h), (par(t.h as f64 * ew as f64 / eh as f64), par(altura.min(eh) as f64)), "célula: {msg}");
        assert!(t.celulas.iter().zip(&js).all(|(c, &j)| (c.t - tab.tempo(j)).abs() < 1e-9), "tempo das células: {msg}");
        (t, caminho)
    }

    /// O aceite: em toda a matriz de fontes (início ≠ 0, taxa variável, oscilação, giro de 90°, pixel 10:11, faixa
    /// cheia, quadros B, lista de edição, TS cortado, MPEG-PS, entrelaçado), a miniatura k mostra o quadro-chave certo,
    /// lido pelo número desenhado no quadro: uma faixa longa (20 miniaturas) e uma curta e densa (60 em 4 s, janelas
    /// vazias repetindo a anterior), as duas começando no meio de um GOP.
    #[test]
    fn miniatura_k_mostra_o_quadro_chave_certo() {
        let Some(cap) = fontes::cap() else { return };
        let cache = fontes::pasta_temporaria("tiras-matriz");
        for &f in MATRIZ {
            let m = apoio::medida(&cap, f);
            let video = fontes::gerar(f).unwrap();
            let fonte = FonteInfo { sha256: sonda::sha256_arquivo(&video).unwrap(), ..m.info.clone() };
            let tab = &fonte.quadros;
            let chaves: Vec<usize> = (0..tab.pts.len()).filter(|&j| tab.chave[j]).collect();
            assert!(chaves.len() >= 3, "{f:?}: {} quadros-chave", chaves.len());
            // vertical: célula mais alta (as faixas ficariam estreitas)
            let altura = if fonte.exibido.1 > fonte.exibido.0 { 180 } else { 90 };
            let (de, ate) = faixa(tab, chaves[1] + 7, 1e9, 20);
            let t = conferir(&cap, &cache, &fonte, &m.numeros, de, ate, 20, altura, &format!("{f:?} longa"));
            assert!(t.celulas.len() >= 3, "{f:?}: {} células", t.celulas.len());
            let (de, ate) = faixa(tab, chaves[2] - 3, 4.0, 60);
            let t = conferir(&cap, &cache, &fonte, &m.numeros, de, ate, 60, altura, &format!("{f:?} densa"));
            assert!(t.miniaturas.windows(2).any(|w| w[0] == w[1]), "{f:?}: densa sem janela vazia");
        }
    }

    /// Busca (`-ss` da regra do §5.3) no meio do MP4: as mesmas células que sem busca.
    #[test]
    fn com_busca_as_mesmas_celulas() {
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let video = fontes::gerar(Fonte::Mp4).unwrap();
        let fonte = FonteInfo { sha256: sonda::sha256_arquivo(&video).unwrap(), ..m.info.clone() };
        assert_eq!(fonte.quadros.busca, Busca::Confiavel, "{}", cap.versao);
        let (de, ate) = faixa(&fonte.quadros, 11 * 30 + 12, 3.5, 12);
        assert!(planejar(&cap.sha256, &fonte, &pedido(&fonte.sha256, de, ate, 12, 90)).unwrap().busca.is_some());
        let com = conferir(&cap, &fontes::pasta_temporaria("tiras-busca"), &fonte, &m.numeros, de, ate, 12, 90, "Mp4 com busca");
        let sem_busca = FonteInfo { quadros: Arc::new(TabelaQuadros { busca: Busca::SemBusca, ..(*fonte.quadros).clone() }), ..fonte.clone() };
        assert!(planejar(&cap.sha256, &sem_busca, &pedido(&fonte.sha256, de, ate, 12, 90)).unwrap().busca.is_none());
        let sem = conferir(&cap, &fontes::pasta_temporaria("tiras-busca"), &sem_busca, &m.numeros, de, ate, 12, 90, "Mp4 sem busca");
        assert_eq!((com.celulas, com.miniaturas), (sem.celulas, sem.miniaturas));
    }

    /// Luma dos quadros desses pts, decodificados com todos os quadros (sem `-skip_frame`) e reduzidos como na tira.
    fn quadros_reduzidos(cap: &Capacidades, fonte: &FonteInfo, p: &Plano, pts: &[i64]) -> Vec<Vec<u8>> {
        let g = grafo(fonte, p, false);
        let reducao = g.split("showinfo@ti=checksum=0,").nth(1).unwrap().split(",tile=").next().unwrap();
        let quais: Vec<String> = pts.iter().map(|x| format!("eq(pts,{x})")).collect();
        let filtro = format!("select='{}',{reducao},extractplanes=y", quais.join("+"));
        let video = fonte.caminho.to_string_lossy().into_owned();
        let cmd = comando_de_teste(
            cap,
            "direto",
            &["-hide_banner", "-nostdin", "-loglevel", "error", "-copyts", "-i", &video, "-map", "0:v:0", "-vf", &filtro, "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "gray", "-"],
        );
        let (fim, y) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).unwrap();
        exec::exigir_sucesso(&cmd, &fim).unwrap();
        let tam = (p.w * p.h) as usize;
        assert_eq!(y.len(), tam * pts.len(), "quadros de pts {pts:?}");
        y.chunks(tam).map(|c| c.to_vec()).collect()
    }

    /// Gravador sem índice: AVI cortado antes do `idx1` (pelo número desenhado), IFV real (pacotes sem marca de
    /// quadro-chave: a tabela de pacotes não tem nenhum, vale o decodificador), Dahua real (relógio do gravador em
    /// 1 505 762 699 s) e Hikvision IMKH real (MPEG-PS: modo Ordem): as células são os quadros-chave do decodificador
    /// pela regra das janelas, e a imagem de cada uma é o quadro desse pts reduzido do mesmo jeito. Sem a tabela do
    /// decodificador, o Hikvision dá `tabela_pendente`.
    #[test]
    fn gravador_sem_indice_e_inicio_no_relogio() {
        let Some(cap) = fontes::cap() else { return };
        let cache = fontes::pasta_temporaria("tiras-gravador");
        let avi = fontes::gerar(Fonte::Avi25).unwrap();
        let b = std::fs::read(&avi).unwrap();
        let corte = b.windows(4).rposition(|w| w == b"idx1").expect("idx1");
        let sem_indice = fontes::pasta_temporaria("tiras-avi").join("sem índice.avi");
        std::fs::write(&sem_indice, &b[..corte]).unwrap();
        let tab = TabelaQuadros::do_decodificador(&cap, &sem_indice, &AtomicBool::new(false), &mut |_| {}).unwrap();
        let numeros = medir::quadros_de_origem(&cap, &sem_indice).unwrap();
        assert_eq!(numeros.len(), tab.pts.len());
        let sha = sonda::sha256_arquivo(&sem_indice).unwrap();
        let fonte = fonte_info(&cap, &sem_indice, &sha, Arc::new(tab), None).unwrap();
        let chaves: Vec<usize> = (0..fonte.quadros.pts.len()).filter(|&j| fonte.quadros.chave[j]).collect();
        let (de, ate) = faixa(&fonte.quadros, chaves[1] + 4, 1e9, 30);
        conferir(&cap, &cache, &fonte, &numeros, de, ate, 30, 90, "AVI sem índice");

        // H.264 cru (modo Ordem), pelo número desenhado
        let cru = h264_cru(&cap);
        let tab = TabelaQuadros::do_decodificador(&cap, &cru, &AtomicBool::new(false), &mut |_| {}).unwrap();
        let numeros = medir::quadros_de_origem(&cap, &cru).unwrap();
        assert_eq!(numeros.len(), tab.pts.len());
        let fonte = fonte_info(&cap, &cru, &sonda::sha256_arquivo(&cru).unwrap(), Arc::new(tab), None).unwrap();
        let chaves: Vec<usize> = (0..fonte.quadros.pts.len()).filter(|&j| fonte.quadros.chave[j]).collect();
        let (de, ate) = faixa(&fonte.quadros, chaves[1] + 4, 1e9, 20);
        assert!(matches!(planejar(&cap.sha256, &fonte, &pedido(&fonte.sha256, de, ate, 20, 90)).unwrap().modo, Modo::Ordem { .. }));
        let t = conferir(&cap, &cache, &fonte, &numeros, de, ate, 20, 90, "H.264 cru");
        assert!(t.celulas.len() >= 5, "{}", t.celulas.len());

        let amostras = fontes::amostras_de_gravador();
        for prefixo in ["ifv", "dahua", "hik_nvr", "test.lvf"] {
            let Some(arq) = amostras.iter().find(|a| a.file_name().is_some_and(|n| n.to_string_lossy().starts_with(prefixo))) else {
                eprintln!("aviso: amostra {prefixo} ausente (p1b/dl/dvr/amostras): dispensada");
                continue;
            };
            let dec = Arc::new(TabelaQuadros::do_decodificador(&cap, arq, &AtomicBool::new(false), &mut |_| {}).unwrap());
            let sha = sonda::sha256_arquivo(arq).unwrap();
            // a tela usa a melhor tabela que houver: no IFV ainda a de pacotes (sem quadro-chave), no Dahua a do decodificador
            let (tab, com_k0) = if prefixo == "ifv" {
                let pac = Arc::new(TabelaQuadros::de_pacotes(&cap, arq).unwrap());
                assert!(pac.chave.iter().all(|k| !k), "IFV com quadro-chave nos pacotes ({})", cap.versao);
                (pac, false)
            } else {
                (dec.clone(), true)
            };
            let fonte = fonte_info(&cap, arq, &sha, tab, None).unwrap();
            if prefixo == "dahua" {
                assert!(fonte.inicio_s > 1.5e9, "{}", fonte.inicio_s);
            }
            if prefixo == "hik_nvr" {
                let pac = fonte_info(&cap, arq, &sha, Arc::new(TabelaQuadros::de_pacotes(&cap, arq).unwrap()), None).unwrap();
                assert_eq!(codigo(planejar(&cap.sha256, &pac, &pedido(&sha, 0.0, 9.0, 8, 90))), "tabela_pendente", "{}", cap.versao);
            }
            let chaves: Vec<usize> = (0..dec.pts.len()).filter(|&j| dec.chave[j]).collect();
            let (de, ate) = faixa(&dec, chaves[1] + 3, 1e9, 24);
            let p = planejar(&cap.sha256, &fonte, &pedido(&sha, de, ate, 24, 90)).unwrap();
            assert_eq!(matches!(p.modo, Modo::Ordem { .. }), prefixo == "hik_nvr", "{} ({})", fonte.conteiner, cap.versao);
            let t = produzir(&cap, &cache, &fonte, &p, &AtomicBool::new(false)).unwrap_or_else(|e| panic!("{prefixo} ({}): {e}", cap.versao));
            let (js, mapa) = esperado(&dec, de, ate, 24, com_k0);
            let msg = format!("{prefixo} ({}): de {de} ate {ate}", cap.versao);
            assert!(js.len() >= 5, "{msg}: {} células esperadas", js.len());
            assert_eq!(t.celulas.iter().map(|c| c.pts).collect::<Vec<_>>(), js.iter().map(|&j| dec.pts[j]).collect::<Vec<_>>(), "{msg}");
            assert_eq!(t.miniaturas, mapa, "{msg}");
            let diretos = quadros_reduzidos(&cap, &fonte, &p, &t.celulas.iter().map(|c| c.pts).collect::<Vec<_>>());
            for (c, (y, direto)) in lumas(&cap, &cache, &t).iter().zip(&diretos).enumerate() {
                let dif = y.iter().zip(direto).map(|(a, b)| a.abs_diff(*b) as f64).sum::<f64>() / y.len() as f64;
                assert!(dif < 4.0, "{msg}: célula {c} difere {dif:.2} do quadro de pts {}", t.celulas[c].pts);
                let (mn, mx) = (y.iter().min().unwrap(), y.iter().max().unwrap());
                assert!(mx - mn > 40, "{msg}: célula {c} lisa");
            }
        }
    }

    /// H.264 cru (Anexo B) tirado do MP4 da matriz: mesmos quadros e números desenhados, sem tempo nos pacotes.
    fn h264_cru(cap: &Capacidades) -> PathBuf {
        let mp4 = fontes::gerar(Fonte::Mp4).unwrap().to_string_lossy().into_owned();
        let cmd = comando_de_teste(cap, "cru", &["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-i", &mp4, "-map", "0:v:0", "-c", "copy", "-bsf:v", "h264_mp4toannexb", "-f", "h264", "cru.h264"]);
        let fim = exec::rodar(&cmd, &AtomicBool::new(false)).unwrap();
        exec::exigir_sucesso(&cmd, &fim).unwrap();
        cmd.pasta.join("cru.h264")
    }

    /// Mídia sem tempo nos pacotes lida como se tivesse (modo Tempo): o `-skip_frame` entrega pts errados, e a tira,
    /// conferida com a tabela do decodificador, dá `leitura_divergente` em vez de miniatura faltando ou fora do lugar.
    #[test]
    fn pts_errado_do_skip_frame_vira_erro() {
        let Some(cap) = fontes::cap() else { return };
        let cache = fontes::pasta_temporaria("tiras-pts-errado");
        for video in [fontes::gerar(Fonte::MpegPs).unwrap(), h264_cru(&cap)] {
            let tab = Arc::new(TabelaQuadros::do_decodificador(&cap, &video, &AtomicBool::new(false), &mut |_| {}).unwrap());
            let fonte = fonte_info(&cap, &video, &sonda::sha256_arquivo(&video).unwrap(), tab, None).unwrap();
            assert!(sem_tempo_nos_pacotes(&fonte.conteiner), "{}", fonte.conteiner);
            let como_tempo = FonteInfo { conteiner: "mpegts".into(), ..fonte.clone() };
            let chaves: Vec<usize> = (0..fonte.quadros.pts.len()).filter(|&j| fonte.quadros.chave[j]).collect();
            let (de, ate) = faixa(&fonte.quadros, chaves[1] + 3, 1e9, 12);
            let p = planejar(&cap.sha256, &como_tempo, &pedido(&fonte.sha256, de, ate, 12, 90)).unwrap();
            assert!(p.modo == Modo::Tempo && p.esperado.as_ref().is_some_and(|e| e.len() >= 5), "{p:?}");
            let r = produzir(&cap, &cache, &como_tempo, &p, &AtomicBool::new(false));
            assert_eq!(codigo(r), "leitura_divergente", "{} ({})", fonte.conteiner, cap.versao);
        }
    }

    // ------------------------------------------------------------ atalho do modo Ordem, sem ffmpeg

    #[test]
    fn posicoes_pela_soma_dos_tamanhos() {
        // `size,pos,flags` em pedaços que cortam as linhas; pacote sem posição começa onde o anterior acaba; o de
        // descarte ocupa bytes mas não conta como quadro
        let texto = "100,0,K__\n50,100,___\n30,N/A,___\n70,180,K__\n20,250,_D_\n40,270,K__\n10,N/A,___";
        let mut l = LeitorPosicoes::default();
        for pedaco in texto.as_bytes().chunks(7) {
            l.ler(pedaco);
        }
        let resto = String::from_utf8(std::mem::take(&mut l.resto)).unwrap();
        l.linha(&resto);
        let p = l.posicoes("m", "f");
        assert_eq!((p.pacotes, p.contiguo, &p.chaves, &p.ancoras), (6, true, &vec![0, 3, 4], &vec![(0, 0), (3, 180), (4, 270)]));
        let tab = |n: usize, chaves: &[usize]| {
            let mut t = apoio::tabela((1, 30), (0..n as i64).collect());
            t.chave = (0..n).map(|j| chaves.contains(&j)).collect();
            t
        };
        assert!(p.valem(&tab(6, &[0, 3, 4])));
        // outro número de quadros, quadro-chave em outro índice: não valem
        assert!(!p.valem(&tab(7, &[0, 3, 4])));
        assert!(!p.valem(&tab(6, &[0, 3])));
        assert!(!p.valem(&tab(6, &[0, 2, 4])));
        // posição fora da soma (byte de outro pacote), quadro-chave sem posição nenhuma, tamanho ilegível: não valem
        for texto in ["100,0,K__\n50,120,K__\n", "100,N/A,K__\n50,100,___\n", "100,0,K__\nN/A,100,K__\n"] {
            let mut l = LeitorPosicoes::default();
            l.ler(texto.as_bytes());
            let p = l.posicoes("m", "f");
            assert!(!p.contiguo && !p.valem(&tab(p.pacotes, &p.chaves)), "{texto:?}");
        }
    }

    #[test]
    fn lista_do_concat_com_aspa() {
        assert_eq!(
            lista_do_concat(Path::new("/x/gravação d'água.h264"), 7),
            "ffconcat version 1.0\nfile '/x/gravação d'\\''água.h264'\noption skip_initial_bytes 7\n"
        );
    }

    /// Fluxo cru (Anexo B) do vídeo de `video` em `destino`: os mesmos quadros, sem tempo nos pacotes.
    fn cru_de(cap: &Capacidades, video: &Path, destino: &Path) {
        let (v, d) = (video.to_string_lossy().into_owned(), destino.to_string_lossy().into_owned());
        let cmd = comando_de_teste(cap, "cru-de", &["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-i", &v, "-map", "0:v:0", "-c", "copy", "-bsf:v", "h264_mp4toannexb", "-f", "h264", &d]);
        let fim = exec::rodar(&cmd, &AtomicBool::new(false)).unwrap();
        exec::exigir_sucesso(&cmd, &fim).unwrap();
    }

    fn fonte_do_decodificador(cap: &Capacidades, video: &Path) -> FonteInfo {
        let tab = TabelaQuadros::do_decodificador(cap, video, &AtomicBool::new(false), &mut |_| {}).unwrap();
        fonte_info(cap, video, &sonda::sha256_arquivo(video).unwrap(), Arc::new(tab), None).unwrap()
    }

    fn chaves_de(tab: &TabelaQuadros) -> Vec<usize> {
        (0..tab.pts.len()).filter(|&j| tab.chave[j]).collect()
    }

    /// Índices dos pacotes de quadro-chave sem pts (pelo `ffprobe`).
    fn chaves_sem_pts(cap: &Capacidades, video: &Path) -> Vec<usize> {
        let v = video.to_string_lossy().into_owned();
        let args = ["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts,flags", "-of", "csv=p=0", &v];
        let cmd = exec::Comando { programa: cap.ffprobe.clone(), ..comando_de_teste(cap, "pts-pacotes", &args) };
        let (fim, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).unwrap();
        exec::exigir_sucesso(&cmd, &fim).unwrap();
        let texto = String::from_utf8_lossy(&out);
        texto.lines().enumerate().filter(|(_, l)| l.starts_with("N/A") && l.contains('K')).map(|(j, _)| j).collect()
    }

    /// A mesma tira pelo atalho (`perto` = 0) e pelo modo Ordem desde o começo, em caches separados: o atalho sai pelo
    /// caminho `quer`, com as mesmas células, o mesmo mapa e a mesma luma em cada célula. Devolve a do atalho e os
    /// segundos de cada uma.
    fn igual_ao_ordem(cap: &Capacidades, fonte: &FonteInfo, (de, ate, n): (f64, f64, u32), quer: Caminho, nome: &str) -> (TiraMiniaturas, f64, f64) {
        let p = planejar(&cap.sha256, fonte, &pedido(&fonte.sha256, de, ate, n, 90)).unwrap();
        let msg = format!("{nome} ({}): de {de} ate {ate} n {n}", cap.versao);
        assert!(matches!(p.modo, Modo::Ordem { .. }), "{msg}");
        let (c1, c2) = (fontes::pasta_temporaria("tiras-atalho"), fontes::pasta_temporaria("tiras-ordem"));
        let t0 = Instant::now();
        let (a, por) = produzir_por(cap, &c1, fonte, &p, &AtomicBool::new(false), 0).unwrap_or_else(|e| panic!("{msg}: {e}"));
        let t1 = Instant::now();
        let (o, por_o) = produzir_por(cap, &c2, fonte, &p, &AtomicBool::new(false), usize::MAX).unwrap_or_else(|e| panic!("{msg}: {e}"));
        let t2 = Instant::now();
        assert_eq!((por, por_o), (quer, Caminho::Ordem), "{msg}");
        assert_eq!((&a.celulas, &a.miniaturas), (&o.celulas, &o.miniaturas), "{msg}");
        assert!(a.celulas.len() >= 3, "{msg}: {} células", a.celulas.len());
        assert_eq!(lumas(cap, &c1, &a), lumas(cap, &c2, &o), "imagem: {msg}");
        (a, (t1 - t0).as_secs_f64(), (t2 - t1).as_secs_f64())
    }

    /// Atalho do modo Ordem, longe do começo: MPEG-PS da matriz (pacotes sem pts: o `setts` repõe o da tabela) pelo
    /// `-ss`, e H.264 cru num caminho com espaço, acento e aspa (a lista do `concat`) pelo byte. A miniatura k mostra o
    /// quadro-chave certo (número desenhado) e a tira é a do modo Ordem desde o começo.
    #[test]
    fn atalho_do_modo_ordem_mostra_o_quadro_chave_certo() {
        let Some(cap) = fontes::cap() else { return };
        let cache = fontes::pasta_temporaria("tiras-atalho-matriz");
        let m = apoio::medida(&cap, Fonte::MpegPs);
        let ps = FonteInfo { sha256: sonda::sha256_arquivo(&fontes::gerar(Fonte::MpegPs).unwrap()).unwrap(), ..m.info.clone() };
        // o caso do atalho: quadro-chave sem pts no pacote (o `-skip_frame` sozinho o entrega com o pts errado)
        let k = chaves_de(&ps.quadros);
        let (de, ate) = faixa(&ps.quadros, k[6] + 3, 1e9, 12);
        let (t, por) = conferir_por(&cap, &cache, &ps, &m.numeros, (de, ate, 12, 90), "MpegPs pelo -ss", 0);
        assert_eq!((por, t.celulas.len() >= 5), (Caminho::Busca, true), "{} ({})", t.celulas.len(), cap.versao);
        let sem_pts = chaves_sem_pts(&cap, &ps.caminho);
        assert!(t.celulas.iter().any(|c| ps.quadros.indice_do_pts(c.pts).is_some_and(|j| sem_pts.contains(&j))), "nenhuma célula sem pts no pacote: {sem_pts:?}");
        igual_ao_ordem(&cap, &ps, (de, ate, 12), Caminho::Busca, "MpegPs");
        // densa, começando no meio de um GOP: janelas vazias repetem a anterior
        let (de, ate) = faixa(&ps.quadros, k[8] - 3, 4.0, 40);
        let (t, por) = conferir_por(&cap, &cache, &ps, &m.numeros, (de, ate, 40, 90), "MpegPs densa", 0);
        assert!(por == Caminho::Busca && t.miniaturas.windows(2).any(|w| w[0] == w[1]), "{por:?} ({})", cap.versao);

        let pasta = fontes::pasta_temporaria("tiras-cru com espaço");
        let cru = pasta.join("gravação d'água.h264");
        std::fs::copy(h264_cru(&cap), &cru).unwrap();
        let fonte = fonte_do_decodificador(&cap, &cru);
        let numeros = medir::quadros_de_origem(&cap, &cru).unwrap();
        let k = chaves_de(&fonte.quadros);
        let (de, ate) = faixa(&fonte.quadros, k[7] + 4, 1e9, 10);
        let (t, por) = conferir_por(&cap, &cache, &fonte, &numeros, (de, ate, 10, 90), "H.264 cru pelo byte", 0);
        assert_eq!((por, t.celulas.len() >= 5), (Caminho::Byte, true), "{} ({})", t.celulas.len(), cap.versao);
        igual_ao_ordem(&cap, &fonte, (de, ate, 10), Caminho::Byte, "H.264 cru");
        // as posições ficaram no cache da mídia: o pacote do quadro-chave começa num código de início
        let pos = ler_posicoes(&arquivo_posicoes(&cache, &fonte.sha256, &cap.sha256)).expect("posições no cache");
        let b = std::fs::read(&cru).unwrap();
        assert!(pos.valem(&fonte.quadros), "{}", cap.versao);
        assert_eq!(pos.ancoras.iter().map(|a| a.0).collect::<Vec<_>>(), k);
        assert!(pos.ancoras.iter().all(|&(_, p)| b[p as usize..].starts_with(&[0, 0, 1]) || b[p as usize..].starts_with(&[0, 0, 0, 1])));
    }

    /// Amostras de gravador: Hikvision IMKH (MPEG-PS) pelo `-ss`; o H.264 dele e o do Dahua, tirados em fluxo cru, pelo
    /// byte. No começo, no meio e no fim: as células do atalho são as do modo Ordem desde o começo, e a imagem de cada
    /// uma é o quadro desse pts decodificado inteiro.
    #[test]
    fn atalho_nas_amostras_de_gravador() {
        let Some(cap) = fontes::cap() else { return };
        let amostras = fontes::amostras_de_gravador();
        let pasta = fontes::pasta_temporaria("tiras-amostras-cru");
        for (prefixo, cru) in [("hik_nvr", false), ("hik_nvr", true), ("dahua", true)] {
            let Some(arq) = amostras.iter().find(|a| a.file_name().is_some_and(|n| n.to_string_lossy().starts_with(prefixo))) else {
                eprintln!("aviso: amostra {prefixo} ausente (p1b/dl/dvr/amostras): dispensada");
                continue;
            };
            let video = if cru {
                let v = pasta.join(format!("{prefixo}.h264"));
                cru_de(&cap, arq, &v);
                v
            } else {
                arq.clone()
            };
            let fonte = fonte_do_decodificador(&cap, &video);
            let quer = if cru { Caminho::Byte } else { Caminho::Busca };
            let k = chaves_de(&fonte.quadros);
            for (onde, i) in [("começo", 2), ("meio", k.len() / 2), ("fim", k.len() - 6)] {
                let (de, ate) = faixa(&fonte.quadros, k[i] + 3, 1e9, 16);
                let nome = format!("{prefixo}{} {onde}", if cru { " cru" } else { "" });
                let (t, s_atalho, s_ordem) = igual_ao_ordem(&cap, &fonte, (de, ate, 16), quer, &nome);
                eprintln!("{nome} ({}): atalho {:.0} ms, modo Ordem desde o começo {:.0} ms", cap.versao, s_atalho * 1e3, s_ordem * 1e3);
                let p = planejar(&cap.sha256, &fonte, &pedido(&fonte.sha256, de, ate, 16, 90)).unwrap();
                let diretos = quadros_reduzidos(&cap, &fonte, &p, &t.celulas.iter().map(|c| c.pts).collect::<Vec<_>>());
                let c = fontes::pasta_temporaria("tiras-amostras");
                let (t2, _) = produzir_por(&cap, &c, &fonte, &p, &AtomicBool::new(false), 0).unwrap();
                for (j, (y, direto)) in lumas(&cap, &c, &t2).iter().zip(&diretos).enumerate() {
                    let dif = y.iter().zip(direto).map(|(a, b)| a.abs_diff(*b) as f64).sum::<f64>() / y.len() as f64;
                    assert!(dif < 4.0, "{nome} ({}): célula {j} difere {dif:.2} do quadro de pts {}", cap.versao, t2.celulas[j].pts);
                }
            }
        }
    }

    /// Atalho que lê o quadro errado (posição plantada no quadro antes do que a conta supõe) não chega à tela: a
    /// conferência com a tabela recusa, a tira sai do modo Ordem desde o começo com os quadros certos, e a mídia vai
    /// direto ao modo Ordem dali em diante.
    #[test]
    fn atalho_que_nao_bate_cai_no_modo_ordem() {
        let Some(cap) = fontes::cap() else { return };
        let cru = h264_cru(&cap);
        let numeros = medir::quadros_de_origem(&cap, &cru).unwrap();
        let fonte = FonteInfo { sha256: "e1".repeat(32), ..fonte_do_decodificador(&cap, &cru) };
        let cache = fontes::pasta_temporaria("tiras-atalho-errado");
        let certas = medir_posicoes(&cap.ffprobe, &cru, &cache, &fonte.sha256, &cap.sha256).unwrap();
        assert!(certas.valem(&fonte.quadros), "{}", cap.versao);
        // o byte de cada quadro-chave dado ao quadro seguinte: a leitura começa um quadro antes do que a conta supõe
        let erradas = Posicoes { ancoras: certas.ancoras.iter().map(|&(j, b)| (j + 1, b)).collect(), ..certas.clone() };
        assert!(erradas.valem(&fonte.quadros));
        gravar_posicoes(&arquivo_posicoes(&cache, &fonte.sha256, &cap.sha256), &erradas).unwrap();
        let k = chaves_de(&fonte.quadros);
        let (de, ate) = faixa(&fonte.quadros, k[5] + 4, 1e9, 10);
        let (_, por) = conferir_por(&cap, &cache, &fonte, &numeros, (de, ate, 10, 90), "posições erradas", 0);
        assert_eq!(por, Caminho::Ordem, "{}", cap.versao);
        assert!(divergentes().lock().unwrap().contains(&format!("{}:{}", fonte.sha256, cap.sha256)));
        let (de, ate) = faixa(&fonte.quadros, k[6] + 2, 1e9, 9);
        assert_eq!(conferir_por(&cap, &cache, &fonte, &numeros, (de, ate, 9, 90), "depois", 0).1, Caminho::Ordem);
    }

    /// A tira que espera as posições (medidas pela primeira vez, `ffprobe` preso na entrada) volta logo quando é
    /// cancelada; a medição segue sozinha e acaba quando a entrada some, sem posições.
    #[test]
    fn tira_esperando_as_posicoes_cancela_logo() {
        let Some(cap) = fontes::cap() else { return };
        let cru = h264_cru(&cap);
        let (escuta, entrada) = entrada_presa();
        let fonte = FonteInfo { caminho: entrada, sha256: "e2".repeat(32), ..fonte_do_decodificador(&cap, &cru) };
        let k = chaves_de(&fonte.quadros);
        let (de, ate) = faixa(&fonte.quadros, k[6] + 2, 1e9, 9);
        let p = planejar(&cap.sha256, &fonte, &pedido(&fonte.sha256, de, ate, 9, 90)).unwrap();
        let cache = fontes::pasta_temporaria("tiras-espera");
        let cancelar = Arc::new(AtomicBool::new(false));
        let (c, cap2, fonte2, cache2) = (cancelar.clone(), cap.clone(), fonte.clone(), cache.clone());
        let h = std::thread::spawn(move || (produzir_por(&cap2, &cache2, &fonte2, &p, &c, 0), Instant::now()));
        let chave = format!("{}:{}", fonte.sha256, cap.sha256);
        let medindo = || matches!(medicoes().0.lock().unwrap().get(&chave), Some(Medicao::Medindo));
        let t0 = Instant::now();
        while !medindo() {
            assert!(t0.elapsed() < Duration::from_secs(30), "medição não começou");
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(300));
        let pedido_em = Instant::now();
        cancelar.store(true, Ordering::SeqCst);
        let (r, voltou) = h.join().unwrap();
        assert_eq!(codigo(r), ERRO_CANCELADO);
        let gasto = voltou.duration_since(pedido_em).as_secs_f64();
        assert!(gasto < CANCELAR_S * FOLGA, "{gasto:.3} s ({})", cap.versao);
        assert!(medindo(), "a medição morreu com a tira");
        drop(escuta);
        let t0 = Instant::now();
        while medindo() {
            assert!(t0.elapsed() < Duration::from_secs(30), "medição presa");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(medicoes().0.lock().unwrap().get(&chave), Some(Medicao::Sem)));
    }

    /// Faixa sem nenhum quadro-chave (antes do primeiro quadro): tira vazia, sem imagem, nos dois binários (o 9.x sai
    /// com erro quando não grava nada), e a segunda vez vem do cache. No modo Ordem nem roda o ffmpeg. Depois do fim
    /// vale a regra: a miniatura 0 é o último quadro-chave. Faixa que começa antes do primeiro quadro: as células certas.
    #[test]
    fn faixa_sem_quadro_chave_da_tira_vazia() {
        let Some(cap) = fontes::cap() else { return };
        let cache = fontes::pasta_temporaria("tiras-vazia");
        let sem_ffmpeg = Capacidades { binario: PathBuf::from("/sem/ffmpeg"), ..(*cap).clone() };
        for (f, c) in [(Fonte::Mp4, &*cap), (Fonte::MpegPs, &sem_ffmpeg)] {
            let fonte = FonteInfo { sha256: sonda::sha256_arquivo(&fontes::gerar(f).unwrap()).unwrap(), ..apoio::medida(&cap, f).info.clone() };
            let ini = fonte.quadros.tempo(0);
            let p = planejar(&cap.sha256, &fonte, &pedido(&fonte.sha256, ini - 5.0, ini - 1.0, 8, 90)).unwrap();
            assert_eq!(p.modo == Modo::Tempo, f == Fonte::Mp4);
            let t = produzir(c, &cache, &fonte, &p, &AtomicBool::new(false)).unwrap_or_else(|e| panic!("{f:?}: {e} ({})", cap.versao));
            assert_eq!((t.relativo.as_deref(), t.celulas.len(), t.miniaturas.len(), t.n), (None, 0, 0, 8), "{f:?}");
            assert_eq!(consultar(&cache, &fonte.sha256, &p.hash), Some(t));
            let fim = fonte.quadros.fim_s();
            let numeros = &apoio::medida(&cap, f).numeros;
            let ultima = (0..fonte.quadros.pts.len()).rev().find(|&j| fonte.quadros.chave[j]).unwrap();
            let t = conferir(&cap, &cache, &fonte, numeros, fim + 1.0, fim + 9.0, 8, 90, &format!("{f:?} depois do fim"));
            assert_eq!((t.celulas.len(), t.celulas[0].pts), (1, fonte.quadros.pts[ultima]));
            // faixa que começa antes do primeiro quadro (número negativo no filtro)
            let t = conferir(&cap, &cache, &fonte, numeros, ini - 0.6, ini + 4.4, 10, 90, &format!("{f:?} antes do início"));
            assert_eq!(t.celulas.len(), 5, "{f:?}");
        }
    }

    /// Cache: a segunda chamada devolve a mesma tira sem rodar o ffmpeg (binário inexistente com o mesmo SHA-256) e
    /// sem regravar; outro ffmpeg (outro SHA-256) é outra chave. A pasta de trabalho some.
    #[test]
    fn segunda_chamada_vem_do_cache() {
        let Some(cap) = fontes::cap() else { return };
        let fonte = FonteInfo { sha256: "c2".repeat(32), ..apoio::medida(&cap, Fonte::Mkv30).info.clone() };
        let cache = fontes::pasta_temporaria("tiras-cache2");
        let p = planejar(&cap.sha256, &fonte, &pedido(&fonte.sha256, 0.5, 14.5, 28, 90)).unwrap();
        let t = produzir(&cap, &cache, &fonte, &p, &AtomicBool::new(false)).unwrap();
        assert!(t.relativo.is_some() && t.celulas.len() >= 10, "{t:?}");
        let (jpg, json, _) = arquivos(&cache, &fonte.sha256, &p.hash);
        let quando = || (std::fs::metadata(&jpg).unwrap().modified().unwrap(), std::fs::metadata(&json).unwrap().modified().unwrap());
        let antes = quando();
        let sem_ffmpeg = Capacidades { binario: PathBuf::from("/sem/ffmpeg"), ..(*cap).clone() };
        assert_eq!(produzir(&sem_ffmpeg, &cache, &fonte, &p, &AtomicBool::new(false)).unwrap(), t);
        assert_eq!(quando(), antes);
        let resto: Vec<_> = std::fs::read_dir(cache.join(PASTA)).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(resto.len(), 1, "{resto:?}");
        let outro = planejar(&"9".repeat(64), &fonte, &pedido(&fonte.sha256, 0.5, 14.5, 28, 90)).unwrap();
        assert_ne!(outro.hash, p.hash);
        assert!(produzir(&sem_ffmpeg, &cache, &fonte, &outro, &AtomicBool::new(false)).is_err());
    }

    /// `edicao_tira` num caso de verdade: a primeira chamada pede à fila de fundo (`tira:<chave>`) e volta sem tira;
    /// pronta a tarefa, a segunda vem do cache, sem tarefa nova; cache com espaço e acento; SHA-256 fora do caso.
    #[test]
    fn pedir_pela_fila_de_fundo_e_depois_do_cache() {
        let Some(cap) = fontes::cap() else { return };
        let (d, ws) = crate::edicao::executar::apoio::caso();
        let video = fontes::gerar(Fonte::Mp4).unwrap();
        let vm = tauri::async_runtime::block_on(crate::commands::video_commands::register_video_media(
            ws.to_string_lossy().into(),
            crate::models::RegisterVideoInput { source_path: video.to_string_lossy().into(), allow_duplicate: false },
        ))
        .unwrap();
        let cache = d.path().join("cache da edição");
        let ped = PedidoTira { canal: "clipe-fila".into(), ..pedido(&vm.sha256, 1.3, 14.2, 30, 90) };
        let r = pedir(&ws, &cache, &ped).unwrap();
        assert!(r.tira.is_none() && r.chave.starts_with("tira:"), "{r:?}");
        let t0 = Instant::now();
        let id = loop {
            let e = fundo::estado(&r.chave).expect("tarefa da tira");
            match e.estado {
                EstadoFundo::Pronta => break e.id,
                EstadoFundo::Falhou { mensagem } => panic!("{mensagem} ({})", cap.versao),
                EstadoFundo::Cancelada => panic!("cancelada"),
                _ => assert!(t0.elapsed() < Duration::from_secs(120), "tira presa na fila"),
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let r2 = pedir(&ws, &cache, &ped).unwrap();
        assert_eq!(r2.chave, r.chave);
        let t = r2.tira.expect("do cache");
        assert_eq!(fundo::estado(&r.chave).map(|e| (e.id, e.estado)), Some((id, EstadoFundo::Pronta)), "tarefa nova para tira em cache");
        // tira do cache: o canal não fica esperando nada (registro velho impedia cancelar a tira de outro canal)
        assert!(!canais().lock().unwrap().contains_key("clipe-fila"));
        // quadros-chave do MP4 (a cada 30), lidos na imagem, no tempo da célula
        let lidos: Vec<u32> = lumas(&cap, &cache, &t).iter().map(|y| medir::identificar(y, t.w as usize, t.h as usize)).collect();
        assert!(lidos.len() >= 10 && lidos.iter().zip(&t.celulas).all(|(&n, c)| n % 30 == 0 && n as f64 == (c.t * 30.0).round()), "{lidos:?}");
        assert!(pedir(&ws, &cache, &pedido(&"0".repeat(64), 0.0, 1.0, 4, 90)).is_err());
        assert_eq!(codigo(pedir(&ws, &cache, &pedido("../x", 0.0, 1.0, 4, 90))), "sha256_invalido");
        // a tabela pedida ao fundo grava neste cache: espera antes de a pasta sair
        let chave = fundo::chave(TipoFundo::Tabela, &vm.sha256);
        while fundo::estado(&chave).is_some_and(|t| matches!(t.estado, EstadoFundo::NaFila | EstadoFundo::Rodando)) {
            assert!(t0.elapsed().as_secs() < 240, "tabela presa");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn ate_o_estado(chave: &str, quer: impl Fn(&EstadoFundo) -> bool, teto: Duration) -> EstadoFundo {
        let t0 = Instant::now();
        loop {
            if let Some(e) = fundo::estado(chave).map(|t| t.estado).filter(|e| quer(e)) {
                return e;
            }
            assert!(t0.elapsed() < teto, "{chave}: {:?}", fundo::estado(chave));
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Entrada que prende o ffmpeg em qualquer sistema (o FIFO é só Unix): conexão TCP local que nunca manda nada.
    fn entrada_presa() -> (std::net::TcpListener, PathBuf) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("tcp://127.0.0.1:{}", l.local_addr().unwrap().port());
        (l, PathBuf::from(url))
    }

    fn vivo(e: &EstadoFundo) -> bool {
        matches!(e, EstadoFundo::NaFila | EstadoFundo::Rodando)
    }

    /// O front pede outra faixa (zoom): o pedido velho do mesmo canal morre em menos de 0,5 s, com o ffmpeg de verdade
    /// preso lendo a entrada. Pedido que outro canal ainda quer não morre. Roda também no Windows (CI).
    #[test]
    fn pedido_novo_do_canal_cancela_o_velho() {
        let Some(cap) = fontes::cap() else { return };
        let (_escuta, entrada) = entrada_presa();
        let pasta = fontes::pasta_temporaria("tiras-presa");
        let fonte = FonteInfo { caminho: entrada, sha256: "f1".repeat(32), ..apoio::medida(&cap, Fonte::Mp4).info.clone() };
        let cache = pasta.join("cache");
        let ped = |canal: &str, de: f64| PedidoTira { canal: canal.into(), ..pedido(&fonte.sha256, de, 14.0, 10, 90) };
        let a = pedir_com(cap.clone(), &cache, fonte.clone(), &ped("clipe-a", 1.1)).unwrap();
        let b = pedir_com(cap.clone(), &cache, fonte.clone(), &ped("clipe-b", 1.1)).unwrap();
        assert!(a.tira.is_none() && a.chave == b.chave);
        ate_o_estado(&a.chave, |e| *e == EstadoFundo::Rodando, Duration::from_secs(120));
        // o ffmpeg nasce e fica preso lendo a entrada
        std::thread::sleep(Duration::from_millis(300));
        // "clipe-a" muda de faixa; "clipe-b" ainda quer a velha
        let a2 = pedir_com(cap.clone(), &cache, fonte.clone(), &ped("clipe-a", 2.1)).unwrap();
        assert_ne!(a2.chave, a.chave);
        std::thread::sleep(Duration::from_millis(300));
        assert!(vivo(&fundo::estado(&a.chave).unwrap().estado), "cancelada com outro canal querendo");
        // "clipe-b" muda também: a velha morre
        let t0 = Instant::now();
        let b2 = pedir_com(cap.clone(), &cache, fonte.clone(), &ped("clipe-b", 3.1)).unwrap();
        let fim = ate_o_estado(&a.chave, |e| !vivo(e), Duration::from_secs(10));
        let gasto = t0.elapsed().as_secs_f64();
        assert_eq!(fim, EstadoFundo::Cancelada, "{}", cap.versao);
        eprintln!("pedido velho morto em {:.0} ms ({})", gasto * 1000.0, cap.versao);
        assert!(gasto < CANCELAR_S * FOLGA, "{gasto:.3} s ({})", cap.versao);
        // pedido recusado (faixa vazia) também larga o que o canal esperava
        assert_eq!(codigo(pedir_com(cap.clone(), &cache, fonte.clone(), &ped("clipe-a", 14.0))), "item_invalido");
        assert_eq!(ate_o_estado(&a2.chave, |e| !vivo(e), Duration::from_secs(10)), EstadoFundo::Cancelada);
        fundo::cancelar(&b2.chave);
        assert_eq!(ate_o_estado(&b2.chave, |e| !vivo(e), Duration::from_secs(10)), EstadoFundo::Cancelada);
        // nada no cache nem pasta de trabalho das canceladas
        let resto: Vec<_> = std::fs::read_dir(cache.join(PASTA)).map(|r| r.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
        assert!(resto.is_empty(), "{resto:?}");
    }

    /// Rajada da linha do tempo (zoom e rolagem): 8 linhas de pedidos ao mesmo tempo, 12 cada, sem esperar resposta,
    /// duas a duas no mesmo canal (4 canais). No fim cada canal tem viva só uma tira; todas as outras acabam `cancelada`
    /// (nenhuma órfã rodando), cada pedido volta logo e, canceladas as últimas, não sobra nada na fila nem no cache.
    #[test]
    fn rajada_deixa_viva_uma_tira_por_canal() {
        let Some(cap) = fontes::cap() else { return };
        let (_escuta, entrada) = entrada_presa();
        let fonte = FonteInfo { caminho: entrada, sha256: "f3".repeat(32), ..apoio::medida(&cap, Fonte::Mp4).info.clone() };
        let cache = fontes::pasta_temporaria("tiras-rajada").join("cache");
        let (linhas, pedidos, canais_n) = (8usize, 12usize, 4usize);
        let pedidas: Vec<(usize, String)> = std::thread::scope(|s| {
            let hs: Vec<_> = (0..linhas)
                .map(|l| {
                    let (cap, fonte, cache) = (cap.clone(), fonte.clone(), cache.clone());
                    s.spawn(move || {
                        (0..pedidos)
                            .map(|k| {
                                let de = 0.5 + 0.01 * (l * pedidos + k) as f64;
                                let ped = PedidoTira { canal: format!("rajada-{}", l % canais_n), ..pedido(&fonte.sha256, de, 14.0, 10, 90) };
                                let t0 = Instant::now();
                                let r = pedir_com(cap.clone(), &cache, fonte.clone(), &ped).unwrap();
                                assert!(t0.elapsed() < Duration::from_secs(1), "pedido em {:?}", t0.elapsed());
                                (l % canais_n, r.chave)
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
        });
        let t0 = Instant::now();
        let vivas = loop {
            let vivas: Vec<(usize, String)> = pedidas.iter().filter(|(_, c)| vivo(&fundo::estado(c).unwrap().estado)).cloned().collect();
            if vivas.len() <= canais_n || t0.elapsed() > Duration::from_secs(10) {
                break vivas;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let mut por_canal: Vec<usize> = vivas.iter().map(|(c, _)| *c).collect();
        por_canal.sort();
        assert_eq!(por_canal, (0..canais_n).collect::<Vec<_>>(), "vivas: {vivas:?}");
        for (_, c) in &vivas {
            fundo::cancelar(c);
        }
        for (_, c) in &pedidas {
            assert_eq!(ate_o_estado(c, |e| !vivo(e), Duration::from_secs(10)), EstadoFundo::Cancelada, "{c}");
        }
        let resto: Vec<_> = std::fs::read_dir(cache.join(PASTA)).map(|r| r.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
        assert!(resto.is_empty(), "{resto:?}");
    }

    /// 1080p30 de 10 min, GOP de 2 s (o `longo10min.mp4` do `p1b`, aqui com x264), gerado uma vez por máquina.
    fn dez_minutos(cap: &Capacidades) -> PathBuf {
        let pasta = std::env::temp_dir().join("sicro-edicao-fontes").join("tiras-v1");
        std::fs::create_dir_all(&pasta).unwrap();
        let (destino, ok) = (pasta.join("longo-10min-1080p.mp4"), pasta.join("longo-10min-1080p.mp4.ok"));
        let trava = File::create(pasta.join("longo.trava")).unwrap();
        trava.lock().unwrap();
        if !(ok.exists() && destino.exists()) {
            let tmp = pasta.join(format!("tmp-{}.mp4", std::process::id()));
            let st = crate::tools::command(&cap.binario)
                .args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=s=1920x1080:r=30:d=600"])
                .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "28", "-g", "60", "-bf", "2", "-pix_fmt", "yuv420p"])
                .arg(&tmp)
                .status()
                .unwrap();
            assert!(st.success(), "gerar 10 min ({})", cap.versao);
            std::fs::rename(&tmp, &destino).unwrap();
            std::fs::write(&ok, b"1").unwrap();
        }
        destino
    }

    /// O tempo do aceite: 60 miniaturas de 10 min em 1080p (300 quadros-chave decodificados) em cerca de 2,4 s; as
    /// células são os quadros-chave de 10 em 10 s (o da fronteira fica na janela dele). E cancelar no meio de uma de
    /// verdade: a volta em menos de 0,5 s.
    #[test]
    #[ignore = "completo: vídeo 1080p de 10 min (gerado uma vez por máquina)"]
    fn sessenta_miniaturas_em_2_4_s() {
        let Some(cap) = fontes::cap() else { return };
        let video = dez_minutos(&cap);
        let tab = Arc::new(TabelaQuadros::de_pacotes(&cap, &video).unwrap());
        let fonte = fonte_info(&cap, &video, &"d1".repeat(32), tab, None).unwrap();
        let cache = fontes::pasta_temporaria("tiras-10min");
        let p = planejar(&cap.sha256, &fonte, &pedido(&fonte.sha256, 0.0, 600.0, 60, 90)).unwrap();
        assert_eq!((p.w, p.h), (160, 90));
        let t0 = Instant::now();
        let t = produzir(&cap, &cache, &fonte, &p, &AtomicBool::new(false)).unwrap();
        let gasto = t0.elapsed().as_secs_f64();
        eprintln!("60 miniaturas de 10 min ({}): {gasto:.2} s, carga {}", cap.versao, std::fs::read_to_string("/proc/loadavg").unwrap_or_default().trim());
        assert_eq!(t.celulas.len(), 60);
        assert!(t.celulas.iter().enumerate().all(|(k, c)| (c.t - 10.0 * k as f64).abs() < 1e-6), "{:?}", t.celulas);
        assert_eq!(t.miniaturas, (0..60).collect::<Vec<u32>>());
        assert!(lumas(&cap, &cache, &t).windows(2).all(|w| w[0] != w[1]), "células repetidas");
        assert!(gasto < SESSENTA_S * FOLGA, "{gasto:.2} s ({})", cap.versao);
        // cancelar no meio (outra chave)
        let p2 = planejar(&cap.sha256, &fonte, &pedido(&fonte.sha256, 0.0, 600.0, 59, 90)).unwrap();
        let cancelar = Arc::new(AtomicBool::new(false));
        let c = cancelar.clone();
        let (cap2, cache2, fonte2, p2b) = (cap.clone(), cache.clone(), fonte.clone(), p2.clone());
        let h = std::thread::spawn(move || {
            let r = produzir(&cap2, &cache2, &fonte2, &p2b, &c);
            (r, Instant::now())
        });
        std::thread::sleep(Duration::from_millis(400));
        let pedido_em = Instant::now();
        cancelar.store(true, Ordering::SeqCst);
        let (r, voltou) = h.join().unwrap();
        let gasto = voltou.duration_since(pedido_em).as_secs_f64();
        assert!(r.unwrap_err().to_string().contains(ERRO_CANCELADO));
        eprintln!("cancelada no meio em {:.0} ms ({})", gasto * 1000.0, cap.versao);
        assert!(gasto < CANCELAR_S * FOLGA, "{gasto:.3} s ({})", cap.versao);
        assert_eq!(consultar(&cache, &fonte.sha256, &p2.hash), None);
    }

    /// Uma hora 1080p30 com quadro-chave a cada segundo (3600), gerada uma vez por máquina: `ps` = MPEG-PS com pts em
    /// todo pacote; `parada` = MPEG-PS de cena quase parada (quadros pequenos: parte dos quadros-chave sem pts no
    /// pacote); `cru` = o H.264 do `ps` em fluxo cru. A tabela do decodificador fica no cache ao lado.
    fn uma_hora(cap: &Capacidades, nome: &str) -> FonteInfo {
        let pasta = std::env::temp_dir().join("sicro-edicao-fontes").join("tiras-v1");
        std::fs::create_dir_all(&pasta).unwrap();
        let trava = File::create(pasta.join("hora.trava")).unwrap();
        trava.lock().unwrap();
        let x264 = ["-c:v", "libx264", "-preset", "ultrafast", "-crf", "40", "-g", "30", "-bf", "0", "-pix_fmt", "yuv420p", "-f"];
        let gerar = |destino: &Path, args: &[&str]| {
            let ok = destino.with_extension("ok");
            if ok.exists() && destino.exists() {
                return;
            }
            let _ = std::fs::remove_dir_all(destino.with_extension("cache"));
            let tmp = pasta.join(format!("tmp-{}", std::process::id()));
            let st = crate::tools::command(&cap.binario).args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error"]).args(args).arg(&tmp).status().unwrap();
            assert!(st.success(), "gerar {} ({})", destino.display(), cap.versao);
            std::fs::rename(&tmp, destino).unwrap();
            std::fs::write(ok, b"1").unwrap();
        };
        let ps = pasta.join("hora-ps.mpg");
        let mut a = vec!["-f", "lavfi", "-i", "testsrc2=s=1920x1080:r=30:d=3600"];
        a.extend(x264);
        a.push("vob");
        gerar(&ps, &a);
        let video = match nome {
            "ps" => ps,
            "parada" => {
                let v = pasta.join("hora-parada.mpg");
                let mut a = vec!["-f", "lavfi", "-i", "smptehdbars=s=1920x1080:r=30:d=3600[a];testsrc2=s=192x108:r=30:d=3600[b];[a][b]overlay=x='mod(t*97,1700)':y=400"];
                a.extend(x264);
                a.push("vob");
                gerar(&v, &a);
                v
            }
            _ => {
                let v = pasta.join("hora-cru.h264");
                let p = ps.to_string_lossy().into_owned();
                gerar(&v, &["-i", &p, "-map", "0:v:0", "-c", "copy", "-f", "h264"]);
                v
            }
        };
        let sha = sha256_bytes(format!("hora-{nome}").as_bytes());
        let cache = video.with_extension("cache");
        let mut tab = TabelaQuadros::obter_com(cap, &cache, &sha, &video).unwrap();
        if tab.origem != OrigemTabela::Decodificador {
            let t0 = Instant::now();
            tab = TabelaQuadros::do_decodificador(cap, &video, &AtomicBool::new(false), &mut |_| {}).unwrap().guardar(&cache, &sha).unwrap();
            eprintln!("tabela do decodificador de {nome} ({}): {:.1} s", cap.versao, t0.elapsed().as_secs_f64());
        }
        fonte_info(cap, &video, &sha, tab, None).unwrap()
    }

    /// O aceite do P4.3b: numa hora de gravação, a tira de 20 miniaturas de um minuto no fim (e no meio) leva o tempo da
    /// do começo, nos três casos do modo Ordem; antes, a do fim decodificava os 3600 quadros-chave. A do fim é a mesma
    /// do modo Ordem desde o começo (células, mapa e imagem). O fluxo cru mede as posições uma vez (vai no primeiro
    /// pedido longe do começo, medido à parte).
    #[test]
    #[ignore = "completo: uma hora 1080p (gerada uma vez por máquina, ~3 GB)"]
    fn uma_hora_tira_no_fim_como_no_comeco() {
        let Some(cap) = fontes::cap() else { return };
        for (nome, quer) in [("ps", Caminho::Busca), ("parada", Caminho::Busca), ("cru", Caminho::Byte)] {
            let fonte = uma_hora(&cap, nome);
            let tab = &fonte.quadros;
            assert_eq!(chaves_de(tab).len(), 3600, "{nome}");
            if nome == "parada" {
                let sem_pts = chaves_sem_pts(&cap, &fonte.caminho).len();
                eprintln!("parada ({}): {sem_pts} quadros-chave sem pts no pacote", cap.versao);
                assert!(sem_pts > 0, "a cena parada não prova o caso");
            }
            let ini = tab.tempo(0);
            let pedido_em = |de: f64| pedido(&fonte.sha256, ini + de, ini + de + 60.0, 20, 90);
            let medir_em = |de: f64, perto: usize| {
                let p = planejar(&cap.sha256, &fonte, &pedido_em(de)).unwrap();
                let cache = fontes::pasta_temporaria("tiras-hora");
                let t0 = Instant::now();
                let (t, por) = produzir_por(&cap, &cache, &fonte, &p, &AtomicBool::new(false), perto).unwrap_or_else(|e| panic!("{nome} {de} ({}): {e}", cap.versao));
                (t0.elapsed().as_secs_f64(), t, por, cache)
            };
            if nome == "cru" {
                let (s, _, por, _) = medir_em(1200.3, PERTO);
                eprintln!("cru ({}): primeiro pedido longe do começo, com a medição das posições: {s:.2} s ({por:?})", cap.versao);
            }
            let (comeco, _, por_c, _) = medir_em(0.3, PERTO);
            let (meio, _, por_m, _) = medir_em(1800.3, PERTO);
            let (fim, t_fim, por_f, c_fim) = medir_em(3530.3, PERTO);
            let (antes, t_ordem, por_o, c_ordem) = medir_em(3530.3, usize::MAX);
            let carga = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
            eprintln!(
                "{nome} ({}): começo {comeco:.2} s ({por_c:?}), meio {meio:.2} s ({por_m:?}), fim {fim:.2} s ({por_f:?}); fim no modo Ordem desde o começo {antes:.2} s; carga {}",
                cap.versao,
                carga.trim()
            );
            assert_eq!((por_c, por_m, por_f, por_o), (Caminho::Ordem, quer, quer, Caminho::Ordem), "{nome} ({})", cap.versao);
            assert_eq!((&t_fim.celulas, &t_fim.miniaturas), (&t_ordem.celulas, &t_ordem.miniaturas), "{nome} ({})", cap.versao);
            assert_eq!(t_fim.celulas.len(), 20, "{nome}");
            assert_eq!(lumas(&cap, &c_fim, &t_fim), lumas(&cap, &c_ordem, &t_ordem), "{nome} ({})", cap.versao);
            // "o mesmo tempo": com folga de 3× (O1) e piso de 0,3 s para o começo
            let teto = comeco.max(0.3) * FOLGA;
            assert!(meio < teto && fim < teto, "{nome} ({}): começo {comeco:.2} s, meio {meio:.2} s, fim {fim:.2} s", cap.versao);
        }
    }
}
