//! Forma de onda (`receitas-audio-saida.md` §10): `-ac 1 -ar 8000 -f s16le` por pipe, mínimo e máximo por balde de
//! 10 ms, em cache por mídia (`ondas/<sha256>.pk`), feita na fila de fundo (`TipoFundo::Onda`). Dono: P1.9.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::edicao::executar::ERRO_CANCELADO;
use crate::edicao::ffmpeg::exec;
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::fundo::{self, TipoFundo};
use crate::edicao::motor::FonteInfo;
use crate::edicao::previa;
use crate::error::{Result, SicroError};

/// Baldes por segundo da onda.
pub const POR_S: u32 = 100;

/// Picos do som da mídia: `min[i]`/`max[i]` = amostras extremas (s16, mono) do balde `i`, que começa em
/// `inicio_s + i / por_s` (tempo de fonte). Vai até o fim do vídeo (mais 1 s): som além dele não entra.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Onda {
    pub inicio_s: f64,
    pub por_s: u32,
    pub min: Vec<i16>,
    pub max: Vec<i16>,
}

/// Taxa do PCM lido pelo pipe.
const TAXA: u32 = 8000;
const POR_BALDE: u32 = TAXA / POR_S;
/// Teto: 48 h de som (17,28 M baldes, 69 MB no cache).
const MAX_BALDES: usize = 48 * 3600 * POR_S as usize;
/// Baldes além do fim do vídeo (1 s): depois disso a leitura para (a pista não mostra).
const MARGEM_BALDES: usize = POR_S as usize;
const PASTA: &str = "ondas";
/// Cabeçalho do `.pk`: marca, SHA-256 da mídia (hex), `inicio_s` (f64), `por_s` (u32) e `n` (u32), tudo little-endian;
/// depois `min[n]` e `max[n]` em i16.
const MARCA: &[u8; 8] = b"SICROPK\x01";
const CABECALHO: usize = 8 + 64 + 8 + 4 + 4;

fn sha_valido(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn arquivo(cache: &Path, sha256: &str) -> PathBuf {
    cache.join(PASTA).join(format!("{sha256}.pk"))
}

/// Onda já no cache; None = falta (quem chama pede à fila de fundo).
pub fn consultar(cache: &Path, sha256: &str) -> Option<Onda> {
    if !sha_valido(sha256) {
        return None;
    }
    let b = std::fs::read(arquivo(cache, sha256)).ok()?;
    ler(&b, sha256)
}

fn ler(b: &[u8], sha256: &str) -> Option<Onda> {
    if b.len() < CABECALHO || &b[..8] != MARCA || &b[8..72] != sha256.as_bytes() {
        return None;
    }
    let inicio_s = f64::from_le_bytes(b[72..80].try_into().ok()?);
    let por_s = u32::from_le_bytes(b[80..84].try_into().ok()?);
    let n = u32::from_le_bytes(b[84..88].try_into().ok()?) as usize;
    // o tamanho confere antes de alocar: arquivo truncado ou cabeçalho estragado não vira vetor gigante
    if por_s != POR_S || n > MAX_BALDES || b.len() != CABECALHO + 4 * n || !inicio_s.is_finite() {
        return None;
    }
    let i16s = |de: usize| b[de..de + 2 * n].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
    Some(Onda { inicio_s, por_s, min: i16s(CABECALHO), max: i16s(CABECALHO + 2 * n) })
}

fn bytes(o: &Onda, sha256: &str) -> Vec<u8> {
    let mut b = Vec::with_capacity(CABECALHO + 4 * o.min.len());
    b.extend_from_slice(MARCA);
    b.extend_from_slice(sha256.as_bytes());
    b.extend_from_slice(&o.inicio_s.to_le_bytes());
    b.extend_from_slice(&o.por_s.to_le_bytes());
    b.extend_from_slice(&(o.min.len() as u32).to_le_bytes());
    for v in o.min.iter().chain(&o.max) {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b
}

/// Grava por temporário + `rename`: arquivo pela metade nunca fica com o nome final.
fn guardar(cache: &Path, sha256: &str, o: &Onda) -> Result<()> {
    static N: AtomicU64 = AtomicU64::new(0);
    let destino = arquivo(cache, sha256);
    let pasta = destino.parent().expect("pasta das ondas");
    std::fs::create_dir_all(pasta)?;
    // nome próprio: a pasta de trabalho (`<sha>.tmp-…`) pode estar ao lado
    let tmp = pasta.join(format!("{sha256}.pk.tmp-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    std::fs::write(&tmp, bytes(o, sha256))?;
    let r = std::fs::rename(&tmp, &destino);
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(r?)
}

/// Onda de mídia sem som: vazia, sem ffmpeg.
fn vazia(fonte: &FonteInfo) -> Onda {
    Onda { inicio_s: fonte.inicio_s, por_s: POR_S, min: Vec::new(), max: Vec::new() }
}

/// Calcula e guarda a onda (bloqueia a thread). `progresso` em 0..1.
pub fn produzir(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, cancelar: &AtomicBool, progresso: &mut dyn FnMut(f32)) -> Result<Onda> {
    if !sha_valido(&fonte.sha256) {
        return Err(SicroError::Validation(format!("sha256_invalido: {}", fonte.sha256)));
    }
    if let Some(o) = consultar(cache, &fonte.sha256) {
        progresso(1.0);
        return Ok(o);
    }
    let Some(audio) = &fonte.audio else {
        progresso(1.0);
        return Ok(vazia(fonte));
    };
    // som até o fim do vídeo (pela tabela): mede o progresso e corta a leitura. Salto no tempo do som (o `async=1`
    // o enche de silêncio) não vira horas de onda num vídeo de minutos. Tabela de pacotes sem tempo confiável
    // (MPEG-PS, H.264 cru) pode errar a duração: aí não corta
    let dur = fonte.quadros.pts.last().map(|_| (fonte.quadros.fim_s() - audio.inicio_s).max(0.0));
    let esperadas = dur.map(|d| (d * TAXA as f64) as u64).filter(|&n| n > 0);
    let corte = dur.filter(|_| previa::marcavel(fonte)).map(|d| ((d * POR_S as f64).ceil() as usize).saturating_add(MARGEM_BALDES));
    let trabalho = Trabalho::nova(cache, &fonte.sha256)?;
    let (min, max) = picos_por_pipe(cap, &fonte.caminho, &trabalho.0, cancelar, esperadas, corte, progresso)?;
    let o = Onda { inicio_s: audio.inicio_s, por_s: POR_S, min, max };
    guardar(cache, &fonte.sha256, &o)?;
    progresso(1.0);
    Ok(o)
}

/// Pasta de trabalho (stderr do ffmpeg) dentro de `ondas/`, apagada no fim.
struct Trabalho(PathBuf);

impl Trabalho {
    fn nova(cache: &Path, sha256: &str) -> Result<Self> {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = cache.join(PASTA).join(format!("{sha256}.tmp-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
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

/// Mínimo e máximo por balde, somados aos pedaços que o pipe entrega (um byte pode ficar entre dois pedaços).
struct Picos {
    min: Vec<i16>,
    max: Vec<i16>,
    n: u32,
    mn: i16,
    mx: i16,
    sobra: Option<u8>,
    amostras: u64,
    teto: usize,
    /// Baldes que bastam (fim do vídeo); `cheio` = chegou lá, o resto do som não entra.
    corte: Option<usize>,
    cheio: bool,
}

impl Picos {
    fn novo(teto: usize, corte: Option<usize>) -> Self {
        Picos { min: Vec::new(), max: Vec::new(), n: 0, mn: i16::MAX, mx: i16::MIN, sobra: None, amostras: 0, teto, corte, cheio: corte == Some(0) }
    }

    fn amostra(&mut self, v: i16) -> Result<()> {
        if self.cheio {
            return Ok(());
        }
        self.mn = self.mn.min(v);
        self.mx = self.mx.max(v);
        self.n += 1;
        self.amostras += 1;
        if self.n == POR_BALDE {
            self.fechar()?;
        }
        Ok(())
    }

    fn fechar(&mut self) -> Result<()> {
        if self.min.len() >= self.teto {
            return Err(SicroError::Validation(format!("nao_suportado: som de mais de {} h", MAX_BALDES / (3600 * POR_S as usize))));
        }
        self.min.push(self.mn);
        self.max.push(self.mx);
        (self.n, self.mn, self.mx) = (0, i16::MAX, i16::MIN);
        self.cheio = self.corte.is_some_and(|c| self.min.len() >= c);
        Ok(())
    }

    fn somar(&mut self, mut b: &[u8]) -> Result<()> {
        if let (Some(lo), Some(&hi)) = (self.sobra, b.first()) {
            self.sobra = None;
            self.amostra(i16::from_le_bytes([lo, hi]))?;
            b = &b[1..];
        }
        let pares = b.chunks_exact(2);
        self.sobra = pares.remainder().first().copied();
        for c in pares {
            self.amostra(i16::from_le_bytes([c[0], c[1]]))?;
        }
        Ok(())
    }

    /// O último balde pode ter menos de `POR_BALDE` amostras.
    fn fim(mut self) -> Result<(Vec<i16>, Vec<i16>)> {
        if self.n > 0 {
            self.fechar()?;
        }
        Ok((self.min, self.max))
    }
}

/// `ffmpeg … -map 0:a:0 -af aresample=8000:async=1 -ac 1 -f s16le -` lido em fluxo (sem guardar o PCM). O
/// `async=1` preenche buraco de som com silêncio: a onda não escorrega depois de uma falha do gravador.
/// Pelo `exec`: o filho fica preso ao SICRO (morre com ele e no `matar_todos` da saída).
fn picos_por_pipe(
    cap: &Capacidades,
    arquivo: &Path,
    pasta: &Path,
    cancelar: &AtomicBool,
    esperadas: Option<u64>,
    corte: Option<usize>,
    progresso: &mut dyn FnMut(f32),
) -> Result<(Vec<i16>, Vec<i16>)> {
    let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-nostats", "-loglevel", "error", "-vn", "-sn", "-dn", "-i"].iter().map(|s| s.to_string()).collect();
    args.push(arquivo.to_string_lossy().into_owned());
    args.extend(["-map", "0:a:0", "-af", "aresample=8000:async=1", "-ac", "1", "-f", "s16le", "-"].iter().map(|s| s.to_string()));
    let cmd = exec::Comando {
        nome: "onda".into(),
        programa: cap.binario.clone(),
        args,
        pasta: pasta.to_path_buf(),
        ambiente: Vec::new(),
        sem_ambiente: Vec::new(),
        stderr: pasta.join("onda.stderr"),
    };
    let mut picos = Picos::novo(MAX_BALDES, corte);
    let mut ultimo = 0.0f32;
    // onda já inteira (fim do vídeo): o resto do som não interessa
    let t = exec::rodar_com_fluxo(&cmd, cancelar, &mut |b| {
        picos.somar(b)?;
        if let Some(t) = esperadas {
            let f = (picos.amostras as f64 / t as f64).min(0.99) as f32;
            if f - ultimo >= 0.01 {
                ultimo = f;
                progresso(f);
            }
        }
        Ok(!picos.cheio)
    })?;
    if t.cancelado || cancelar.load(Ordering::SeqCst) {
        return Err(SicroError::Validation(format!("{ERRO_CANCELADO}: onda")));
    }
    // cortada: o processo foi morto aqui, o código dele não diz nada
    if t.codigo != Some(0) && !picos.cheio {
        return Err(SicroError::Validation(format!("outra: onda (código {:?}): {}", t.codigo, exec::ultimas_linhas(&cmd.stderr, 3).join(" | "))));
    }
    picos.fim()
}

/// `edicao_onda`: a onda do cache, ou None depois de pedir à fila de fundo (chave `onda:<sha256>`; o fim chega pelo
/// evento `edicao:fundo` e a tela pede de novo). Vídeo sem som: a onda vazia, sem fila.
pub fn pedir(ws: &Path, cache: &Path, sha256: &str) -> Result<Option<Onda>> {
    if let Some(o) = consultar(cache, sha256) {
        return Ok(Some(o));
    }
    let (_, fonte, cap) = crate::commands::edicao_commands::fonte_do_caso(ws, sha256, cache)?;
    if fonte.audio.is_none() {
        return Ok(Some(vazia(&fonte)));
    }
    let cache = cache.to_path_buf();
    let chave = fundo::chave(TipoFundo::Onda, sha256);
    fundo::pedir(TipoFundo::Onda, &chave, Box::new(move |c, p| produzir(&cap, &cache, &fonte, c, p).map(|_| ())));
    Ok(None)
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::fs::File;
    use std::time::Duration;
    use std::time::Instant;

    use crate::edicao::previa::testes::{extra, fonte_de_teste, fonte_falsa, sem_progresso, MP4};
    use crate::edicao::testes::fontes::{self, Fonte};

    /// Amostras pseudoaleatórias de faixa inteira, com os dois extremos de s16 no meio.
    fn sorteadas(n: usize, semente: u32) -> Vec<i16> {
        let mut x = semente;
        let mut v: Vec<i16> = (0..n)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (x >> 16) as u16 as i16
            })
            .collect();
        if n > 500 {
            (v[123], v[456]) = (i16::MIN, i16::MAX);
        }
        v
    }

    /// Mínimo e máximo por balde de `POR_BALDE` amostras, contados à parte (o último pode ser parcial).
    fn esperado(a: &[i16]) -> (Vec<i16>, Vec<i16>) {
        let c = a.chunks(POR_BALDE as usize);
        (c.clone().map(|b| *b.iter().min().unwrap()).collect(), c.map(|b| *b.iter().max().unwrap()).collect())
    }

    fn le(a: &[i16]) -> Vec<u8> {
        a.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// Fonte falsa com a tabela do vídeo cobrindo `segundos` (a onda vai até o fim do vídeo).
    fn com_duracao(mut f: FonteInfo, segundos: i64) -> FonteInfo {
        let mut t = (*f.quadros).clone();
        t.pts = vec![0, segundos * 15360];
        f.quadros = std::sync::Arc::new(t);
        f
    }

    #[test]
    fn picos_por_balde_com_pedacos_que_partem_a_amostra() {
        let a = sorteadas(1000, 7);
        let (mn, mx) = esperado(&a);
        assert_eq!(mn.len(), 13);
        let b = le(&a);
        for passo in [1, 3, 7, 160, 999, b.len()] {
            let mut p = Picos::novo(MAX_BALDES, None);
            for c in b.chunks(passo) {
                p.somar(c).unwrap();
            }
            assert_eq!(p.amostras, 1000, "{passo}");
            assert_eq!(p.fim().unwrap(), (mn.clone(), mx.clone()), "{passo}");
        }
        // byte solto no fim (amostra incompleta) não vira amostra
        let mut p = Picos::novo(MAX_BALDES, None);
        p.somar(&b[..161]).unwrap();
        assert_eq!(p.fim().unwrap(), esperado(&a[..80]));
        // teto: o balde que passa dele é erro, nunca vetor sem fim
        let mut p = Picos::novo(2, None);
        let e = p.somar(&b[..2 * 240]).unwrap_err();
        assert!(e.to_string().contains("nao_suportado"), "{e}");
        // corte (fim do vídeo): para nos baldes que bastam, sem erro, e o resto não entra
        let mut p = Picos::novo(MAX_BALDES, Some(3));
        p.somar(&b[..2 * 239]).unwrap();
        assert!(!p.cheio);
        p.somar(&b[2 * 239..]).unwrap();
        assert!(p.cheio);
        assert_eq!(p.fim().unwrap(), esperado(&a[..240]));
        // corte antes do teto vale; corte zero não guarda nada
        let mut p = Picos::novo(4, Some(2));
        p.somar(&b).unwrap();
        assert_eq!(p.fim().unwrap(), esperado(&a[..160]));
        let mut p = Picos::novo(MAX_BALDES, Some(0));
        p.somar(&b).unwrap();
        assert_eq!(p.fim().unwrap(), (vec![], vec![]));
    }

    #[test]
    fn cache_so_vale_inteiro_e_da_mesma_midia() {
        let cache = fontes::pasta_temporaria("ondas-cache");
        let sha = "ab".repeat(32);
        let a = sorteadas(8000, 3);
        let (min, max) = esperado(&a);
        let o = Onda { inicio_s: 1505762699.125, por_s: POR_S, min, max };
        assert_eq!(consultar(&cache, &sha), None);
        guardar(&cache, &sha, &o).unwrap();
        assert_eq!(consultar(&cache, &sha), Some(o.clone()));
        let resto: Vec<_> = std::fs::read_dir(cache.join(PASTA)).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(resto.len(), 1, "{resto:?}");
        let arq = arquivo(&cache, &sha);
        let b = std::fs::read(&arq).unwrap();
        assert_eq!(b.len(), CABECALHO + 4 * 100);
        // de outra mídia, truncado, com marca errada, `n` que não bate (sem alocar), `por_s` de outra versão
        assert_eq!(ler(&b, &"cd".repeat(32)), None);
        assert_eq!(ler(&b[..b.len() - 1], &sha), None);
        let mut x = b.clone();
        x[0] = b'X';
        assert_eq!(ler(&x, &sha), None);
        let mut x = b.clone();
        x[84..88].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(ler(&x, &sha), None);
        let mut x = b.clone();
        x[80..84].copy_from_slice(&50u32.to_le_bytes());
        assert_eq!(ler(&x, &sha), None);
        // sha que não é hex não vira caminho
        assert_eq!(consultar(&cache, "../../x"), None);
        // onda vazia (mídia sem som) também vai e volta
        let v = Onda { inicio_s: 0.0, por_s: POR_S, min: vec![], max: vec![] };
        assert_eq!(ler(&bytes(&v, &sha), &sha), Some(v));
    }

    /// Contrato com o front (`ondas-picos.json`, lido por `transporte.test.tsx`): amostras s16 a 8 kHz → a `Onda` que
    /// `edicao_onda` devolve.
    #[test]
    fn fixture_ondas_picos() {
        let a = sorteadas(837, 5);
        let mut p = Picos::novo(MAX_BALDES, None);
        p.somar(&le(&a)).unwrap();
        let (min, max) = p.fim().unwrap();
        let o = Onda { inicio_s: 1.25, por_s: POR_S, min, max };
        assert_eq!(o.min.len(), 11);
        crate::edicao::curva::fixtures::conferir(
            "ondas-picos.json",
            serde_json::json!([{ "nome": "837 amostras a 8 kHz, som em 1,25 s", "entrada": { "taxa": TAXA, "amostras": a }, "saida": o }]),
        );
    }

    // ------------------------------------------------------------ com ffmpeg (os dois binários)

    /// WAV PCM s16 mono a 8 kHz com essas amostras.
    fn wav(destino: &Path, a: &[i16]) {
        let dados = le(a);
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + dados.len() as u32).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&TAXA.to_le_bytes());
        b.extend_from_slice(&(2 * TAXA).to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(dados.len() as u32).to_le_bytes());
        b.extend_from_slice(&dados);
        std::fs::write(destino, b).unwrap();
    }

    fn produzir_de(cap: &Capacidades, f: &FonteInfo) -> (PathBuf, Onda) {
        let cache = fontes::pasta_temporaria("ondas-pipe");
        let mut visto = Vec::new();
        let o = produzir(cap, &cache, f, &AtomicBool::new(false), &mut |x| visto.push(x)).unwrap_or_else(|e| panic!("{e} ({})", cap.versao));
        assert_eq!(visto.last(), Some(&1.0));
        assert!(visto.windows(2).all(|w| w[0] <= w[1]), "{visto:?}");
        // do cache, igual; a pasta de trabalho some
        assert_eq!(consultar(&cache, &f.sha256).as_ref(), Some(&o));
        let resto: Vec<_> = std::fs::read_dir(cache.join(PASTA)).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(resto.len(), 1, "{resto:?}");
        (cache, o)
    }

    /// Cliques da matriz (0,9 em x,5 s do som, sobre seno de 220 Hz a 0,02): o balde do clique (±1) passa de 0,7 da
    /// escala e os baldes a mais de 50 ms de qualquer clique ficam abaixo de 0,03: a onda começa no som (`inicio_s`).
    fn cliques_no_lugar(o: &Onda, segundos: u32, nome: &str) {
        let n = o.min.len();
        assert!((n as i64 - (segundos * POR_S) as i64).abs() <= 3, "{nome}: {n} baldes");
        let pico = |i: usize| o.max[i].max(o.min[i].saturating_abs());
        for k in 0..segundos as usize {
            let i = 50 + 100 * k;
            if i + 1 >= n {
                break;
            }
            let p = (i - 1..=i + 1).map(pico).max().unwrap();
            assert!(p as f64 >= 0.7 * 32767.0, "{nome}: clique de {k},5 s com pico {p}");
        }
        for i in 0..n {
            let ate_clique = ((i % 100) as i64 - 50).abs();
            if ate_clique > 5 {
                assert!(pico(i) as f64 <= 0.03 * 32767.0, "{nome}: balde {i} com pico {} longe dos cliques", pico(i));
            }
        }
    }

    /// Onda por pipe `s16le` a 8 kHz (§11): PCM conhecido a 8 kHz num MKV com o som começando em 1,25 s sai exato
    /// (amostra a amostra, `inicio_s` = 1,25); AAC estéreo a 48 kHz do MP4 e MP2 do TS em 3600 s põem os cliques nos
    /// baldes certos; vídeo sem som dá onda vazia sem ffmpeg.
    #[test]
    fn picos_por_pipe() {
        let Some(cap) = fontes::cap() else { return };
        // exato
        let pasta = fontes::pasta_temporaria("ondas-exato");
        let a = sorteadas(3 * TAXA as usize + 37, 11);
        wav(&pasta.join("som.wav"), &a);
        let wav_s = pasta.join("som.wav").to_string_lossy().into_owned();
        let mkv = extra(
            &cap,
            "ondas-som-em-1250ms.mkv",
            &["-f", "lavfi", "-i", "testsrc2=s=160x90:r=25:d=5", "-itsoffset", "1.25", "-i", &wav_s, "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "copy"],
        );
        let f = fonte_de_teste(&cap, &mkv, true);
        let (_, o) = produzir_de(&cap, &f);
        assert!((o.inicio_s - 1.25).abs() < 1e-9, "{} ({})", o.inicio_s, cap.versao);
        let (mn, mx) = esperado(&a);
        assert_eq!(o.min.len(), mn.len(), "{}", cap.versao);
        assert_eq!((o.min, o.max), (mn, mx), "amostras ({})", cap.versao);
        // AAC 48 kHz estéreo (MP4) e MP2 (TS com início em 3600 s)
        for (fonte, segundos) in [(Fonte::Mp4, 15), (Fonte::Ts5994, 15)] {
            let f = fonte_de_teste(&cap, &fontes::gerar(fonte).unwrap(), true);
            let (_, o) = produzir_de(&cap, &f);
            assert_eq!(o.inicio_s, f.audio.as_ref().unwrap().inicio_s);
            eprintln!("{fonte:?} ({}): início {} s, {} baldes", cap.versao, o.inicio_s, o.min.len());
            cliques_no_lugar(&o, segundos, &format!("{fonte:?} ({})", cap.versao));
        }
        // sem som: vazia, sem ffmpeg e sem cache
        let f = fonte_de_teste(&cap, &fontes::gerar(Fonte::Mkv23976).unwrap(), true);
        let cache = fontes::pasta_temporaria("ondas-mudo");
        let sem_ffmpeg = Capacidades { binario: PathBuf::from("/sem/ffmpeg"), ..(*cap).clone() };
        let o = produzir(&sem_ffmpeg, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        assert!(o.min.is_empty() && o.max.is_empty() && o.por_s == POR_S);
    }

    /// Buraco no som (falha do gravador): 1 s sem pacotes depois de 2 s vira 100 baldes mudos no lugar certo, e o som
    /// de depois fica no tempo dele (sem o `async=1` tudo escorrega 1 s para trás).
    #[test]
    fn buraco_no_som_vira_silencio_no_lugar() {
        let Some(cap) = fontes::cap() else { return };
        let mkv = extra(
            &cap,
            "ondas-buraco.mkv",
            &[
                "-f", "lavfi", "-i", "testsrc2=s=160x90:r=25:d=6", "-f", "lavfi", "-i", "aevalsrc=exprs='0.5*sin(2*PI*440*t)':s=8000:c=mono:d=4", "-map", "0:v", "-map", "1:a",
                "-c:v", "libx264", "-preset", "ultrafast", "-af", "asetnsamples=n=80:p=0,asetpts='if(gte(T\\,2)\\,PTS+8000\\,PTS)'", "-c:a", "pcm_s16le",
            ],
        );
        let f = fonte_de_teste(&cap, &mkv, true);
        let (_, o) = produzir_de(&cap, &f);
        assert_eq!(o.min.len(), 500, "{}", cap.versao);
        let pico = |i: usize| o.max[i].max(o.min[i].saturating_abs());
        for i in 0..500 {
            if (200..300).contains(&i) {
                assert_eq!((o.min[i], o.max[i]), (0, 0), "balde {i} ({})", cap.versao);
            } else {
                assert!(pico(i) as f64 >= 0.45 * 32767.0, "balde {i}: {} ({})", pico(i), cap.versao);
            }
        }
    }

    /// Salto no tempo do som (10 h depois de 2 s, num vídeo de 6 s): o `async=1` o enche de silêncio, e a onda para
    /// no fim do vídeo (mais 1 s) em vez de levar 3,6 milhões de baldes à tela. Som mais longo que o vídeo: o mesmo.
    #[test]
    fn salto_no_tempo_do_som_para_no_fim_do_video() {
        let Some(cap) = fontes::cap() else { return };
        let mkv = extra(
            &cap,
            "ondas-salto-10h.mkv",
            &[
                "-f", "lavfi", "-i", "testsrc2=s=160x90:r=25:d=6", "-f", "lavfi", "-i", "aevalsrc=exprs='0.5*sin(2*PI*440*t)':s=8000:c=mono:d=4", "-map", "0:v", "-map", "1:a",
                "-c:v", "libx264", "-preset", "ultrafast", "-af", "asetnsamples=n=80:p=0,asetpts='if(gte(T\\,2)\\,PTS+8000*36000\\,PTS)'", "-c:a", "pcm_s16le",
            ],
        );
        let f = fonte_de_teste(&cap, &mkv, true);
        let (_, o) = produzir_de(&cap, &f);
        assert_eq!(o.min.len(), 600 + MARGEM_BALDES, "{}", cap.versao);
        let pico = |i: usize| o.max[i].max(o.min[i].saturating_abs());
        assert!((0..200).all(|i| pico(i) as f64 >= 0.45 * 32767.0) && (200..o.min.len()).all(|i| pico(i) == 0), "{}", cap.versao);
        // som de 15 s num vídeo de 5 s
        let longo = extra(
            &cap,
            "ondas-som-longo.mkv",
            &[
                "-f", "lavfi", "-i", "testsrc2=s=160x90:r=25:d=5", "-f", "lavfi", "-i", "aevalsrc=exprs='0.5*sin(2*PI*440*t)':s=8000:c=mono:d=15", "-map", "0:v", "-map", "1:a",
                "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "pcm_s16le",
            ],
        );
        let f = fonte_de_teste(&cap, &longo, true);
        let (_, o) = produzir_de(&cap, &f);
        assert_eq!(o.min.len(), 500 + MARGEM_BALDES, "{}", cap.versao);
        assert!((0..o.min.len()).all(|i| o.max[i] as f64 >= 0.45 * 32767.0), "{}", cap.versao);
    }

    /// Dahua real (G.711 a 8 kHz, relógio do gravador em 1 505 762 699 s, som 1 s antes da imagem, pacotes de 40 ms
    /// com tempos que oscilam): a onda começa no som e cobre os 50,08 s dos pacotes a até 100 ms (a folga do `async`).
    #[test]
    fn onda_do_dahua_real() {
        let Some(cap) = fontes::cap() else { return };
        let Some(dav) = fontes::amostras_de_gravador().into_iter().find(|a| a.file_name().is_some_and(|n| n.to_string_lossy().starts_with("dahua"))) else {
            eprintln!("aviso: amostra Dahua ausente (p1b/dl/dvr/amostras): dispensada");
            return;
        };
        let f = fonte_de_teste(&cap, &dav, false);
        let (_, o) = produzir_de(&cap, &f);
        let video = f.quadros.pts[0] as f64 * f.quadros.tb.num as f64 / f.quadros.tb.den as f64;
        assert_eq!((o.inicio_s, video), (1_505_762_699.0, 1_505_762_700.0), "{}", cap.versao);
        assert!((5000..=5020).contains(&o.min.len()), "{} baldes ({})", o.min.len(), cap.versao);
        let com_som = (0..o.min.len()).filter(|&i| o.max[i] as i32 - o.min[i] as i32 > 64).count();
        assert!(com_som > o.min.len() / 2, "{com_som} baldes com som ({})", cap.versao);
    }

    /// Cancelar no meio: erro `cancelado`, nada no cache, nenhuma pasta de trabalho sobrando, o ffmpeg morto.
    #[test]
    fn cancelar_no_meio_nao_deixa_nada() {
        let Some(cap) = fontes::cap() else { return };
        let m4a = extra(&cap, "ondas-10min.m4a", &["-f", "lavfi", "-i", "sine=frequency=330:sample_rate=48000:duration=600", "-ac", "2", "-c:a", "aac", "-b:a", "96k"]);
        let mut f = com_duracao(fonte_falsa(MP4, "", "", 0.0, 0), 600);
        f.caminho = m4a;
        f.sha256 = "c".repeat(64);
        f.audio = Some(crate::edicao::motor::AudioInfo { inicio_s: 0.0, taxa: 48000, canais: 2 });
        let cache = fontes::pasta_temporaria("ondas-cancelar");
        let cancelar = AtomicBool::new(false);
        let mut visto = Vec::new();
        let e = produzir(&cap, &cache, &f, &cancelar, &mut |x| {
            visto.push(x);
            if x > 0.0 {
                cancelar.store(true, Ordering::SeqCst);
            }
        })
        .unwrap_err();
        assert!(e.to_string().contains(ERRO_CANCELADO), "{e} ({})", cap.versao);
        assert!(visto.iter().all(|&x| x < 1.0), "{visto:?}");
        let resto: Vec<_> = std::fs::read_dir(cache.join(PASTA)).map(|r| r.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
        assert!(resto.is_empty(), "{resto:?}");
        assert_eq!(consultar(&cache, &f.sha256), None);
        // inteira depois, com 60 000 baldes
        let o = produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        assert!((o.min.len() as i64 - 60_000).abs() <= 3, "{}", o.min.len());
    }

    /// Limite real (s) entre o cancelamento e a volta (a vigia olha a cada 10 ms) e a folga com a máquina dividida.
    const CANCELAR_S: f64 = 0.2;

    /// Processo que entrega um pouco de PCM e trava (sem fechar o pipe): só a vigia o derruba no cancelamento.
    #[cfg(unix)]
    #[test]
    fn cancelar_mata_o_processo_preso() {
        use std::os::unix::fs::PermissionsExt;
        let pasta = fontes::pasta_temporaria("ondas-preso");
        let falso = pasta.join("ffmpeg-preso.sh");
        std::fs::write(&falso, "#!/bin/sh\nhead -c 1600 /dev/zero\nexec sleep 60\n").unwrap();
        std::fs::set_permissions(&falso, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut cap = crate::edicao::quadros::apoio::cap_falsa();
        cap.binario = falso;
        let mut f = fonte_falsa(MP4, "", "", 0.0, 0);
        f.audio = Some(crate::edicao::motor::AudioInfo { inicio_s: 0.0, taxa: 8000, canais: 1 });
        let cache = pasta.join("cache");
        let cancelar = AtomicBool::new(false);
        let mut quando = None;
        let e = produzir(&cap, &cache, &f, &cancelar, &mut |_| {
            if quando.is_none() {
                quando = Some(Instant::now());
                cancelar.store(true, Ordering::SeqCst);
            }
        })
        .unwrap_err();
        let gasto = quando.expect("progresso antes do cancelamento").elapsed().as_secs_f64();
        assert!(e.to_string().contains(ERRO_CANCELADO), "{e}");
        assert!(gasto < CANCELAR_S * FOLGA, "{gasto:.2} s");
        assert_eq!(consultar(&cache, &f.sha256), None);
    }

    /// O corte no fim do vídeo só vale com tabela confiável: processo que entrega 3 s de PCM para um vídeo de 0,1 s
    /// dá 0,1 s + 1 s de onda; com a tabela de pacotes de um MPEG-PS (duração que pode errar), os 3 s inteiros.
    #[cfg(unix)]
    #[test]
    fn corte_no_fim_do_video_so_com_tabela_confiavel() {
        use std::os::unix::fs::PermissionsExt;
        let pasta = fontes::pasta_temporaria("ondas-corte");
        let falso = pasta.join("ffmpeg-3s.sh");
        std::fs::write(&falso, "#!/bin/sh\nhead -c 48000 /dev/zero\n").unwrap();
        std::fs::set_permissions(&falso, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut cap = crate::edicao::quadros::apoio::cap_falsa();
        cap.binario = falso;
        let som = Some(crate::edicao::motor::AudioInfo { inicio_s: 0.0, taxa: 8000, canais: 1 });
        let mut f = fonte_falsa(MP4, "", "", 0.0, 0);
        f.audio = som.clone();
        let o = produzir(&cap, &pasta.join("cache-mp4"), &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        assert_eq!(o.min.len(), 10 + MARGEM_BALDES);
        let mut ps = fonte_falsa("mpeg", "", "", 0.0, 0);
        ps.audio = som;
        std::sync::Arc::make_mut(&mut ps.quadros).origem = crate::edicao::quadros::OrigemTabela::Pacotes;
        assert!(!previa::marcavel(&ps));
        let o = produzir(&cap, &pasta.join("cache-ps"), &ps, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        assert_eq!(o.min.len(), 300);
    }

    /// `edicao_onda`: a primeira chamada pede à fila de fundo (`onda:<sha256>`) e devolve None; pronta a tarefa, a
    /// onda vem do cache. Vídeo sem som: a onda vazia na hora, sem fila.
    #[test]
    fn pedir_pela_fila_de_fundo() {
        let Some(cap) = fontes::cap() else { return };
        let (d, ws) = crate::edicao::executar::apoio::caso();
        let registrar = |f: &Path| {
            tauri::async_runtime::block_on(crate::commands::video_commands::register_video_media(
                ws.to_string_lossy().into(),
                crate::models::RegisterVideoInput { source_path: f.to_string_lossy().into(), allow_duplicate: false },
            ))
            .unwrap()
        };
        // o mudo é próprio: a fila de fundo guarda a tabela do decodificador na memória do processo, e o
        // `quadros::testes::obter_prefere_a_do_decodificador_e_nao_a_pede` conta com a do `Mkv23976` fora dela
        let mudo = extra(&cap, "ondas-mudo.mkv", &["-f", "lavfi", "-i", "testsrc2=s=160x90:r=25:d=2", "-c:v", "libx264", "-preset", "ultrafast"]);
        let (vm, mudo) = (registrar(&fontes::gerar(Fonte::Mp4).unwrap()), registrar(&mudo));
        let cache = d.path().join("cache");
        assert_eq!(pedir(&ws, &cache, &vm.sha256).unwrap(), None);
        let chave = fundo::chave(TipoFundo::Onda, &vm.sha256);
        let t0 = Instant::now();
        loop {
            match fundo::estado(&chave).map(|t| t.estado) {
                Some(fundo::EstadoFundo::Pronta) => break,
                Some(fundo::EstadoFundo::Falhou { mensagem }) => panic!("{mensagem} ({})", cap.versao),
                _ => assert!(t0.elapsed().as_secs() < 120, "onda presa na fila"),
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let o = pedir(&ws, &cache, &vm.sha256).unwrap().expect("pronta");
        cliques_no_lugar(&o, 15, &format!("pela fila ({})", cap.versao));
        let v = pedir(&ws, &cache, &mudo.sha256).unwrap().expect("vazia");
        assert!(v.min.is_empty());
        assert!(fundo::estado(&fundo::chave(TipoFundo::Onda, &mudo.sha256)).is_none());
        assert!(pedir(&ws, &cache, "../x").is_err());
        // a tabela pedida ao fundo grava neste cache: espera antes de a pasta sair
        for sha in [&vm.sha256, &mudo.sha256] {
            let chave = fundo::chave(TipoFundo::Tabela, sha);
            while fundo::estado(&chave).is_some_and(|t| matches!(t.estado, fundo::EstadoFundo::NaFila | fundo::EstadoFundo::Rodando)) {
                assert!(t0.elapsed().as_secs() < 240, "tabela presa");
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    /// ffmpeg que morre no meio, depois de entregar PCM (código ≠ 0 ou sinal): erro com o fim do stderr, nada no
    /// cache e nenhuma pasta de trabalho. Onda pela metade nunca vale como a da mídia.
    #[cfg(unix)]
    #[test]
    fn processo_que_morre_no_meio_nao_deixa_onda() {
        use std::os::unix::fs::PermissionsExt;
        let pasta = fontes::pasta_temporaria("ondas-morre");
        for (nome, fim, no_erro) in [("sai", "echo 'Invalid data found when processing input' >&2\nexit 183", "Invalid data found"), ("sinal", "kill -9 $$", "outra: onda")] {
            let falso = pasta.join(format!("ffmpeg-{nome}.sh"));
            std::fs::write(&falso, format!("#!/bin/sh\nhead -c 32000 /dev/zero\n{fim}\n")).unwrap();
            std::fs::set_permissions(&falso, std::fs::Permissions::from_mode(0o755)).unwrap();
            let mut cap = crate::edicao::quadros::apoio::cap_falsa();
            cap.binario = falso;
            // vídeo de 60 s: os 2 s de PCM do processo falso ficam antes do corte
            let mut f = com_duracao(fonte_falsa(MP4, "", "", 0.0, 0), 60);
            f.audio = Some(crate::edicao::motor::AudioInfo { inicio_s: 0.0, taxa: 8000, canais: 1 });
            let cache = pasta.join(format!("cache-{nome}"));
            let e = produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap_err().to_string();
            assert!(e.starts_with("outra: onda") || e.contains("outra: onda"), "{nome}: {e}");
            assert!(e.contains(no_erro) && !e.contains(ERRO_CANCELADO), "{nome}: {e}");
            assert_eq!(consultar(&cache, &f.sha256), None, "{nome}");
            let resto: Vec<_> = std::fs::read_dir(cache.join(PASTA)).map(|r| r.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
            assert!(resto.is_empty(), "{nome}: {resto:?}");
        }
    }

    /// Caminho da mídia e do cache com espaço e acento; arquivo de 0 bytes dá erro e não deixa nada.
    #[test]
    fn caminho_com_espaco_e_acento_e_arquivo_vazio() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("ondas-acento").join("Ocorrência nº 12 (cópia)");
        std::fs::create_dir_all(&pasta).unwrap();
        let video = pasta.join("vídeo da câmera 1.mp4");
        std::fs::copy(fontes::gerar(Fonte::Mp4).unwrap(), &video).unwrap();
        let f = fonte_de_teste(&cap, &video, true);
        let cache = pasta.join("cache da edição");
        let o = produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap_or_else(|e| panic!("{e} ({})", cap.versao));
        cliques_no_lugar(&o, 15, &format!("acento ({})", cap.versao));
        assert_eq!(consultar(&cache, &f.sha256), Some(o));
        let vazio = pasta.join("vazio nº 2.mp4");
        std::fs::write(&vazio, b"").unwrap();
        let mut v = fonte_falsa(MP4, "", "", 0.0, 0);
        v.caminho = vazio;
        v.sha256 = "e".repeat(64);
        v.audio = Some(crate::edicao::motor::AudioInfo { inicio_s: 0.0, taxa: 8000, canais: 1 });
        let e = produzir(&cap, &cache, &v, &AtomicBool::new(false), &mut sem_progresso()).unwrap_err().to_string();
        assert!(e.contains("outra: onda"), "{e} ({})", cap.versao);
        assert_eq!(consultar(&cache, &v.sha256), None);
        let resto: Vec<_> = std::fs::read_dir(cache.join(PASTA)).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(resto.len(), 1, "{resto:?}");
    }

    /// Uma hora de som (o `audio1h.m4a` do `p1b`: ruído rosa, AAC estéreo 48 kHz 128k), gerada uma vez por máquina.
    fn uma_hora(cap: &Capacidades) -> PathBuf {
        let pasta = std::env::temp_dir().join("sicro-edicao-fontes").join("ondas-v1");
        std::fs::create_dir_all(&pasta).unwrap();
        let (destino, ok) = (pasta.join("audio-1h.m4a"), pasta.join("audio-1h.m4a.ok"));
        let trava = File::create(pasta.join("audio-1h.trava")).unwrap();
        trava.lock().unwrap();
        if !(ok.exists() && destino.exists()) {
            let tmp = pasta.join(format!("tmp-{}.m4a", std::process::id()));
            let st = crate::tools::command(&cap.binario)
                .args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "anoisesrc=c=pink:r=48000:a=0.2:d=3600:seed=5"])
                .args(["-af", "volume='0.3+0.7*abs(sin(t/7))':eval=frame", "-ac", "2", "-c:a", "aac", "-b:a", "128k"])
                .arg(&tmp)
                .status()
                .unwrap();
            assert!(st.success(), "gerar 1 h ({})", cap.versao);
            std::fs::rename(&tmp, &destino).unwrap();
            std::fs::write(&ok, b"1").unwrap();
        }
        destino
    }

    /// Limite real (s) para 1 h de som (`receitas-audio-saida.md` §10: 8,4 a 9,2 s) e a folga com a máquina dividida.
    const UMA_HORA_S: f64 = 9.0;
    const FOLGA: f64 = 3.0;

    #[test]
    #[ignore = "completo: 1 h de som (gerada uma vez por máquina)"]
    fn uma_hora_em_9_s() {
        let Some(cap) = fontes::cap() else { return };
        let m4a = uma_hora(&cap);
        let mut f = com_duracao(fonte_falsa(MP4, "", "", 0.0, 0), 3600);
        f.caminho = m4a;
        f.sha256 = "d".repeat(64);
        f.audio = Some(crate::edicao::motor::AudioInfo { inicio_s: 0.0, taxa: 48000, canais: 2 });
        let cache = fontes::pasta_temporaria("ondas-1h");
        let t = Instant::now();
        let o = produzir(&cap, &cache, &f, &AtomicBool::new(false), &mut sem_progresso()).unwrap();
        let gasto = t.elapsed().as_secs_f64();
        eprintln!("1 h de som ({}): {gasto:.1} s, {} baldes, carga {}", cap.versao, o.min.len(), std::fs::read_to_string("/proc/loadavg").unwrap_or_default().trim());
        assert!((o.min.len() as i64 - 360_000).abs() <= 3, "{}", o.min.len());
        // o envelope do gerador (0,3 a 1,0, período de 7π s) aparece: picos por balde de 700 a 2000 em 80 % deles
        // (medido nos dois binários: p10 709, p50 1347, p90 2015)
        let pico = |i: usize| o.max[i].max(o.min[i].saturating_abs()) as f64;
        let (fortes, fracos) = (0..o.min.len()).fold((0, 0), |(a, b), i| (a + (pico(i) > 1800.0) as u32, b + (pico(i) < 900.0) as u32));
        assert!(fortes > 20_000 && fracos > 20_000, "{fortes} {fracos}");
        assert!(gasto < UMA_HORA_S * FOLGA, "{gasto:.1} s ({})", cap.versao);
    }
}
