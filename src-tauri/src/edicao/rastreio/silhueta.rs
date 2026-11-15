//! Silhuetas por quadro (§6.1): `silhuetas/<rid>/<sha256>.sil`, um arquivo por denso, cabeçalho JSON (esquema,
//! `pixel`, `n`, `pts[]`, modelo) e, por quadro, a máscara binária em RLE por linha na resolução da fonte exibida.
//! A Fase 2 grava (EdgeTAM), a Fase 5 usa. Imutável e endereçado pelo conteúdo, como o denso. Dono: P2.2.
//!
//! Formato (B23): `SICROSIL` · u32 LE do tamanho do cabeçalho · cabeçalho JSON · para cada quadro, u32 LE do tamanho
//! do RLE · RLE. RLE: por linha, comprimentos alternados começando pelos zeros (o primeiro pode ser 0), em LEB128
//! sem sinal; cada linha soma `w`.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{ArquivoSilhuetas, Mascara};
use crate::edicao::executar::{sha256_bytes, sha_valido};
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{Silhuetas, TamanhoPx};
use crate::error::{Result, SicroError};

pub const ESQUEMA_SILHUETAS: u32 = 1;
/// Teto do arquivo lido (54 000 quadros 1080p com uma pessoa cabem em ~400 MB).
pub const MAX_BYTES_SILHUETAS: u64 = 1 << 30;
/// Teto do lado e da área de uma máscara (8K): o tamanho vem do arquivo.
pub const MAX_LADO: u32 = 8192;
pub const MAX_AREA: u64 = 8192 * 4320;

const MAGICA: &[u8; 8] = b"SICROSIL";

fn invalido(detalhe: impl std::fmt::Display) -> SicroError {
    SicroError::Validation(format!("{}: {detalhe}", codigo::ITEM_INVALIDO))
}

fn ausente(rastreio: &str) -> SicroError {
    SicroError::Validation(format!("{}: {rastreio}", codigo::RASTREIO_AUSENTE))
}

fn tamanho_ok(w: u32, h: u32) -> bool {
    w > 0 && h > 0 && w <= MAX_LADO && h <= MAX_LADO && w as u64 * h as u64 <= MAX_AREA
}

fn leb(v: u32, out: &mut Vec<u8>) {
    let mut v = v;
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn ler_leb(b: &[u8], i: &mut usize) -> Option<u32> {
    let mut v: u64 = 0;
    for k in 0..5 {
        let x = *b.get(*i)?;
        *i += 1;
        v |= ((x & 0x7f) as u64) << (7 * k);
        if x & 0x80 == 0 {
            return u32::try_from(v).ok();
        }
    }
    None
}

/// RLE por linha de uma máscara (qualquer byte ≠ 0 conta como 1).
pub fn rle(m: &Mascara) -> Vec<u8> {
    let (w, h) = (m.w as usize, m.h as usize);
    let mut out = Vec::new();
    for y in 0..h {
        let linha = &m.bits[y * w..(y + 1) * w];
        let mut atual = false;
        let mut n = 0u32;
        for &b in linha {
            let v = b != 0;
            if v != atual {
                leb(n, &mut out);
                atual = v;
                n = 0;
            }
            n += 1;
        }
        leb(n, &mut out);
    }
    out
}

/// Volta do RLE; tamanho que não fecha com `w`×`h` = `item_invalido`.
pub fn mascara(rle: &[u8], w: u32, h: u32) -> Result<Mascara> {
    if !tamanho_ok(w, h) {
        return Err(invalido(format!("máscara {w}x{h}")));
    }
    let mut bits = vec![0u8; w as usize * h as usize];
    let mut i = 0;
    for y in 0..h as usize {
        let linha = &mut bits[y * w as usize..(y + 1) * w as usize];
        let (mut x, mut v) = (0usize, 0u8);
        while x < linha.len() {
            let n = ler_leb(rle, &mut i).ok_or_else(|| invalido("RLE curto"))? as usize;
            if n > linha.len() - x {
                return Err(invalido("RLE passa da linha"));
            }
            linha[x..x + n].fill(v);
            x += n;
            v ^= 1;
        }
    }
    if i != rle.len() {
        return Err(invalido("RLE com sobra"));
    }
    Ok(Mascara { w, h, bits })
}

/// `<pasta do projeto>/silhuetas/<rid>/<sha256>.sil`. Mesma regra do `arquivo::caminho` (id e SHA-256 conferidos antes).
pub fn caminho(pasta_projeto: &Path, rastreio: &str, sha256: &str) -> PathBuf {
    pasta_projeto.join("silhuetas").join(rastreio).join(format!("{sha256}.sil"))
}

#[derive(Serialize, Deserialize)]
struct Cabecalho {
    esquema: u32,
    pixel: TamanhoPx,
    n: u32,
    pts: Vec<i64>,
    modelo: String,
}

fn exigir(rastreio: &str, sha256: Option<&str>) -> Result<()> {
    if !crate::edicao::armazem::id_valido(rastreio) {
        return Err(invalido("id do rastreio"));
    }
    if sha256.is_some_and(|s| !sha_valido(s)) {
        return Err(invalido("sha256"));
    }
    Ok(())
}

fn conferir(s: &ArquivoSilhuetas) -> Result<()> {
    if s.esquema == 0 || s.esquema > ESQUEMA_SILHUETAS {
        return Err(invalido(format!("esquema {}", s.esquema)));
    }
    if !tamanho_ok(s.pixel.w, s.pixel.h) || s.n as usize != s.pts.len() || s.pts.len() != s.mascaras.len() {
        return Err(invalido("silhuetas"));
    }
    if s.pts.windows(2).any(|p| p[1] <= p[0]) {
        return Err(invalido("pts fora de ordem"));
    }
    Ok(())
}

/// Bytes do arquivo (o SHA-256 deles é o nome).
fn bytes(s: &ArquivoSilhuetas) -> Result<Vec<u8>> {
    let cab = serde_json::to_vec(&Cabecalho { esquema: s.esquema, pixel: s.pixel, n: s.n, pts: s.pts.clone(), modelo: s.modelo.clone() })?;
    let mut out = Vec::with_capacity(16 + cab.len() + s.mascaras.iter().map(|m| m.len() + 4).sum::<usize>());
    out.extend_from_slice(MAGICA);
    out.extend_from_slice(&u32::try_from(cab.len()).map_err(|_| invalido("cabeçalho"))?.to_le_bytes());
    out.extend_from_slice(&cab);
    for m in &s.mascaras {
        out.extend_from_slice(&u32::try_from(m.len()).map_err(|_| invalido("máscara"))?.to_le_bytes());
        out.extend_from_slice(m);
    }
    Ok(out)
}

/// Grava (nome = SHA-256 dos bytes; existente não é regravado) e devolve o ponteiro. Cada RLE é conferido.
pub fn gravar(pasta_projeto: &Path, rastreio: &str, s: &ArquivoSilhuetas) -> Result<Silhuetas> {
    exigir(rastreio, None)?;
    conferir(s)?;
    for m in &s.mascaras {
        mascara(m, s.pixel.w, s.pixel.h)?;
    }
    let b = bytes(s)?;
    if b.len() as u64 > MAX_BYTES_SILHUETAS {
        return Err(invalido("silhuetas grandes demais"));
    }
    let sha256 = sha256_bytes(&b);
    let destino = caminho(pasta_projeto, rastreio, &sha256);
    if !destino.is_file() {
        crate::edicao::armazem::gravar_atomico(&destino, &b)?;
    }
    Ok(Silhuetas { sha256, n: s.n, modelo: s.modelo.clone() })
}

/// Lê e confere o SHA-256 (o `LeitorRastreios::silhuetas` do executor passa a chamar daqui na Fase 5).
pub fn ler(pasta_projeto: &Path, rastreio: &str, sha256: &str) -> Result<ArquivoSilhuetas> {
    exigir(rastreio, Some(sha256))?;
    let arq = caminho(pasta_projeto, rastreio, sha256);
    let f = std::fs::File::open(&arq).map_err(|_| ausente(rastreio))?;
    let mut b = Vec::new();
    f.take(MAX_BYTES_SILHUETAS + 1).read_to_end(&mut b)?;
    if b.len() as u64 > MAX_BYTES_SILHUETAS || sha256_bytes(&b) != sha256 {
        return Err(ausente(rastreio));
    }
    if b.len() < 12 || &b[..8] != MAGICA {
        return Err(invalido("arquivo de silhuetas"));
    }
    let tam = u32::from_le_bytes([b[8], b[9], b[10], b[11]]) as usize;
    let cab: Cabecalho = serde_json::from_slice(b.get(12..12 + tam).ok_or_else(|| invalido("cabeçalho"))?).map_err(|e| invalido(e))?;
    let mut mascaras = Vec::with_capacity(cab.pts.len().min(b.len() / 4));
    let mut i = 12 + tam;
    while i < b.len() {
        let n = u32::from_le_bytes(b.get(i..i + 4).ok_or_else(|| invalido("quadro"))?.try_into().unwrap_or_default()) as usize;
        i += 4;
        mascaras.push(b.get(i..i + n).ok_or_else(|| invalido("quadro"))?.to_vec());
        i += n;
    }
    let s = ArquivoSilhuetas { esquema: cab.esquema, pixel: cab.pixel, n: cab.n, pts: cab.pts, modelo: cab.modelo, mascaras };
    conferir(&s)?;
    Ok(s)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn masc(w: u32, h: u32, f: impl Fn(u32, u32) -> bool) -> Mascara {
        let bits = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| u8::from(f(x, y))).collect();
        Mascara { w, h, bits }
    }

    #[test]
    fn rle_ida_e_volta() {
        let casos = [
            masc(1, 1, |_, _| false),
            masc(1, 1, |_, _| true),
            masc(7, 3, |x, _| x % 2 == 0),
            masc(300, 200, |x, y| (x as i32 - 150).pow(2) + (y as i32 - 100).pow(2) < 70 * 70),
            masc(200, 5, |x, _| x >= 199),
            masc(1000, 2, |_, _| true),
            // corrida longa (> 127: LEB128 de 2 bytes)
            masc(MAX_LADO, 1, |x, _| x >= 300),
        ];
        for m in casos {
            let r = rle(&m);
            assert_eq!(mascara(&r, m.w, m.h).unwrap(), m, "{}x{}", m.w, m.h);
        }
        // tamanho medido: uma linha vazia é 1 byte; a elipse cabe em ~4 bytes por linha
        assert_eq!(rle(&masc(100, 10, |_, _| false)).len(), 10);
        let e = masc(300, 200, |x, y| (x as i32 - 150).pow(2) + (y as i32 - 100).pow(2) < 70 * 70);
        assert!(rle(&e).len() <= 4 * 200, "{}", rle(&e).len());
        // valores ≠ 0 e ≠ 1 contam como 1; a volta dá 0/1
        let m = Mascara { w: 3, h: 1, bits: vec![0, 255, 7] };
        assert_eq!(mascara(&rle(&m), 3, 1).unwrap().bits, vec![0, 1, 1]);
    }

    #[test]
    fn rle_que_nao_fecha_e_recusado() {
        let m = masc(10, 2, |x, _| x > 4);
        let r = rle(&m);
        for (nome, w, h, b) in [
            ("curto", 10, 2, r[..r.len() - 1].to_vec()),
            ("sobra", 10, 2, [r.clone(), vec![0]].concat()),
            ("linha maior", 9, 2, r.clone()),
            ("mais linhas", 10, 3, r.clone()),
            ("varint aberto", 10, 1, vec![0x80, 0x80]),
            ("tamanho grande", MAX_LADO + 1, 1, r.clone()),
            ("área grande", MAX_LADO, MAX_LADO, r.clone()),
        ] {
            let e = mascara(&b, w, h).expect_err(nome).to_string();
            assert!(e.contains(codigo::ITEM_INVALIDO), "{nome}: {e}");
        }
    }

    #[test]
    fn arquivo_imutavel_e_conferido() {
        let d = tempfile::tempdir().unwrap();
        let ms: Vec<Mascara> = (0..5).map(|k| masc(64, 48, move |x, y| x >= 10 + k && x < 30 + k && y >= 5 && y < 40)).collect();
        let s = ArquivoSilhuetas { esquema: 1, pixel: TamanhoPx { w: 64, h: 48 }, n: 5, pts: vec![0, 512, 1024, 1536, 2048], modelo: "EdgeTAM vídeo".into(), mascaras: ms.iter().map(rle).collect() };
        let p = gravar(d.path(), "r1", &s).unwrap();
        assert_eq!((p.n, p.modelo.as_str()), (5, "EdgeTAM vídeo"));
        let arq = caminho(d.path(), "r1", &p.sha256);
        assert_eq!(sha256_bytes(&std::fs::read(&arq).unwrap()), p.sha256);
        let lido = ler(d.path(), "r1", &p.sha256).unwrap();
        assert_eq!(lido, s);
        for (r, m) in lido.mascaras.iter().zip(&ms) {
            assert_eq!(&mascara(r, 64, 48).unwrap(), m);
        }
        // gravar de novo: mesmo nome, arquivo não regravado
        let antes = std::fs::metadata(&arq).unwrap().modified().unwrap();
        assert_eq!(gravar(d.path(), "r1", &s).unwrap(), p);
        assert_eq!(std::fs::metadata(&arq).unwrap().modified().unwrap(), antes);
        // conteúdo trocado: ausente; caminho para fora: recusado antes de abrir
        std::fs::write(&arq, b"SICROSIL").unwrap();
        assert!(ler(d.path(), "r1", &p.sha256).unwrap_err().to_string().contains(codigo::RASTREIO_AUSENTE));
        for (rid, sha) in [("../r1", p.sha256.as_str()), ("r1", "../x"), ("r1", &p.sha256.to_uppercase())] {
            assert!(ler(d.path(), rid, sha).unwrap_err().to_string().contains(codigo::ITEM_INVALIDO), "{rid} {sha}");
        }
        assert!(gravar(d.path(), "../fora", &s).unwrap_err().to_string().contains(codigo::ITEM_INVALIDO));
        assert!(!d.path().join("fora").exists() && !d.path().join("silhuetas/../fora").exists());
        // RLE que não fecha não é gravado
        let mut ruim = s.clone();
        ruim.mascaras[2].pop();
        assert!(gravar(d.path(), "r1", &ruim).unwrap_err().to_string().contains(codigo::ITEM_INVALIDO));
    }
}
