//! Medidores dos testes com ffmpeg: quadro de origem, marcas coloridas, cor, hashes por quadro. Medem identidade de
//! quadro, pixel e cor, nunca só a duração. Todo vídeo é decodificado em CPU, com o giro do contêiner aplicado.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::fontes;
use crate::edicao::ffmpeg::exec::{self, Comando};
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::error::{Result, SicroError};

/// Um quadro decodificado em RGB 8 bits, por linhas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuadroRgb {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

// CONTRATO: acréscimo.
/// Quadro que não traz número legível (corrompido, coberto, cortado).
pub const NAO_IDENTIFICADO: u32 = u32::MAX;

/// Quadro de origem (o número j que a fonte sintética desenhou) de cada quadro do vídeo, na ordem.
/// Lê as seis faixas da matriz (`fontes`) pela mediana da luma e calibra pelos níveis de referência; imagem sem faixas
/// e quase uniforme vale como a convenção de `cm/00-fontes.sh` (luma 16 + N).
pub fn quadros_de_origem(cap: &Capacidades, video: &Path) -> Result<Vec<u32>> {
    let bruto = decodificar(cap, video, "extractplanes=y", "yuv4mpegpipe", "quadros")?;
    let (w, h, quadros) = y4m(&bruto)?;
    Ok(quadros.iter().map(|y| identificar(y, w, h)).collect())
}

/// Número do quadro numa luma `w`×`h` (`NAO_IDENTIFICADO` se não der).
pub(crate) fn identificar(y: &[u8], w: usize, h: usize) -> u32 {
    let mut med = [0u8; 6];
    for (s, m) in med.iter_mut().enumerate() {
        // metade central da faixa, todas as linhas
        let (a, b) = (s * w / 6 + w / 24, (s + 1) * w / 6 - w / 24);
        let mut hist = [0u32; 256];
        for r in 0..h {
            for &v in &y[r * w + a..r * w + b.max(a + 1)] {
                hist[v as usize] += 1;
            }
        }
        *m = mediana(&hist);
    }
    let (baixo, alto) = (med[0] as f64, med[1] as f64);
    if alto - baixo < 60.0 {
        // sem faixas: luma 16 + N, se quase toda a imagem tem o mesmo valor
        let mut hist = [0u32; 256];
        for &v in y {
            hist[v as usize] += 1;
        }
        let m = mediana(&hist);
        let perto: u32 = hist[m.saturating_sub(1) as usize..=(m as usize + 1).min(255)].iter().sum();
        return if m >= 16 && perto as usize * 2 > y.len() { (m - 16) as u32 } else { NAO_IDENTIFICADO };
    }
    let mut n = 0u32;
    for &m in &med[2..] {
        let d = (m as f64 - baixo) / (alto - baixo) * 9.0;
        let r = d.round();
        if !(0.0..=9.0).contains(&r) || (d - r).abs() > 0.35 {
            return NAO_IDENTIFICADO;
        }
        n = n * 10 + r as u32;
    }
    n
}

fn mediana(hist: &[u32; 256]) -> u8 {
    let total: u32 = hist.iter().sum();
    let mut acc = 0;
    for (v, &c) in hist.iter().enumerate() {
        acc += c;
        if acc * 2 >= total {
            return v as u8;
        }
    }
    255
}

/// pts de cada quadro do vídeo pelo decodificador (`-copyts`, `showinfo`), na base do fluxo.
pub fn pts(cap: &Capacidades, video: &Path) -> Result<Vec<i64>> {
    let p = fontes::pasta_temporaria("medir-pts");
    let cmd = comando(
        cap,
        &p,
        "pts",
        vec![
            "-hide_banner".into(),
            "-nostdin".into(),
            "-loglevel".into(),
            "info".into(),
            "-copyts".into(),
            "-i".into(),
            video.to_string_lossy().into_owned(),
            "-map".into(),
            "0:v:0".into(),
            "-vf".into(),
            "showinfo@medir=checksum=0".into(),
            "-f".into(),
            "null".into(),
            "-".into(),
        ],
    );
    exigir_zero(&cmd, exec::rodar(&cmd, &AtomicBool::new(false))?)?;
    Ok(exec::pts_lidos(&cmd.stderr)?.remove("medir").unwrap_or_default())
}

/// Decodifica o quadro de índice `n` do vídeo.
pub fn quadro(cap: &Capacidades, video: &Path, n: u32) -> Result<QuadroRgb> {
    let filtro = format!("select='eq(n,{n})',{RGB}");
    let out = decodificar(cap, video, &filtro, "ppm", "quadro")?;
    ppms(&out)?.into_iter().next().ok_or_else(|| SicroError::Validation(format!("sem quadro {n} em {}", video.display())))
}

// CONTRATO: acréscimo.
/// Os dois campos (de cima, de baixo) do quadro de índice `n`, cada um com metade da altura.
pub fn campos(cap: &Capacidades, video: &Path, n: u32) -> Result<(QuadroRgb, QuadroRgb)> {
    let filtro = format!("select='eq(n,{n})',separatefields,{RGB}");
    let mut c = ppms(&decodificar(cap, video, &filtro, "ppm", "campos")?)?.into_iter();
    match (c.next(), c.next()) {
        (Some(a), Some(b)) => Ok((a, b)),
        _ => Err(SicroError::Validation(format!("sem os campos do quadro {n} em {}", video.display()))),
    }
}

// CONTRATO: acréscimo.
/// Todos os quadros do vídeo em RGB, na ordem (para vídeos curtos de teste).
pub fn quadros(cap: &Capacidades, video: &Path) -> Result<Vec<QuadroRgb>> {
    ppms(&decodificar(cap, video, RGB, "ppm", "quadros-rgb")?)
}

/// Conversão para RGB: matriz e faixa pelas etiquetas do quadro (sem etiqueta: BT.601), croma interpolado.
const RGB: &str = "scale=flags=bicubic+accurate_rnd+bitexact+full_chroma_int,format=rgb24";

/// Caixa [x, y, w, h] em px dos pixels a até `tolerancia` (0..255 por canal) da cor `rgb`; None = nenhum.
pub fn marca(q: &QuadroRgb, rgb: [u8; 3], tolerancia: u8) -> Option<[u32; 4]> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    let mut achou = false;
    for (i, p) in q.px.as_chunks::<3>().0.iter().enumerate() {
        if perto(p, rgb, tolerancia) {
            let (x, y) = (i as u32 % q.w, i as u32 / q.w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            achou = true;
        }
    }
    achou.then(|| [x0, y0, x1 - x0 + 1, y1 - y0 + 1])
}

// CONTRATO: acréscimo.
/// Centro (x, y) em px dos pixels da cor, pela média (centro do pixel = +0,5); None = nenhum.
pub fn centro(q: &QuadroRgb, rgb: [u8; 3], tolerancia: u8) -> Option<(f64, f64)> {
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0u64);
    for (i, p) in q.px.as_chunks::<3>().0.iter().enumerate() {
        if perto(p, rgb, tolerancia) {
            sx += (i as u32 % q.w) as f64 + 0.5;
            sy += (i as u32 / q.w) as f64 + 0.5;
            n += 1;
        }
    }
    (n > 0).then(|| (sx / n as f64, sy / n as f64))
}

fn perto(p: &[u8; 3], rgb: [u8; 3], tol: u8) -> bool {
    p.iter().zip(rgb).all(|(&a, b)| a.abs_diff(b) <= tol)
}

/// Cor média (RGB) da região [x, y, w, h] em px.
pub fn cor_media(q: &QuadroRgb, regiao: [u32; 4]) -> [f64; 3] {
    let [x, y, w, h] = regiao;
    let mut s = [0.0; 3];
    let mut n = 0.0;
    for r in y..(y + h).min(q.h) {
        for c in x..(x + w).min(q.w) {
            let i = ((r * q.w + c) * 3) as usize;
            for (k, v) in s.iter_mut().enumerate() {
                *v += q.px[i + k] as f64;
            }
            n += 1.0;
        }
    }
    s.map(|v| if n > 0.0 { v / n } else { 0.0 })
}

/// Pixels diferentes entre dois quadros do mesmo tamanho (qualquer canal além de `tolerancia`).
pub fn pixels_diferentes(a: &QuadroRgb, b: &QuadroRgb, tolerancia: u8) -> u64 {
    assert_eq!((a.w, a.h), (b.w, b.h), "quadros de tamanhos diferentes");
    a.px.as_chunks::<3>().0.iter().zip(b.px.as_chunks::<3>().0).filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > tolerancia)).count() as u64
}

/// Amostras do PCM s16 estéreo 48 kHz do arquivo.
pub fn amostras(cap: &Capacidades, arquivo: &Path) -> Result<u64> {
    let p = fontes::pasta_temporaria("medir-amostras");
    let cmd = comando(
        cap,
        &p,
        "amostras",
        [
            "-hide_banner",
            "-nostdin",
            "-loglevel",
            "error",
            "-i",
            &arquivo.to_string_lossy(),
            "-map",
            "0:a:0",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-f",
            "s16le",
            "-",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    );
    let (t, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false))?;
    exigir_zero(&cmd, t)?;
    Ok(out.len() as u64 / 4)
}

// CONTRATO: acréscimo.
/// SHA-256 de cada quadro decodificado (`-f framehash -hash sha256`), na ordem.
pub fn framehash(cap: &Capacidades, video: &Path) -> Result<Vec<String>> {
    let p = fontes::pasta_temporaria("medir-framehash");
    let cmd = comando(
        cap,
        &p,
        "framehash",
        [
            "-hide_banner",
            "-nostdin",
            "-loglevel",
            "error",
            "-i",
            &video.to_string_lossy(),
            "-map",
            "0:v:0",
            "-fps_mode",
            "passthrough",
            "-f",
            "framehash",
            "-hash",
            "sha256",
            "-",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    );
    let (t, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false))?;
    exigir_zero(&cmd, t)?;
    Ok(coluna_de_hash(&String::from_utf8_lossy(&out)))
}

// CONTRATO: acréscimo.
/// Coluna de hash de um texto `framehash` ou `framemd5` (último campo de cada linha que não é comentário).
pub fn coluna_de_hash(texto: &str) -> Vec<String> {
    texto
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| l.rsplit(',').next().map(|h| h.trim().to_string()))
        .collect()
}

fn comando(cap: &Capacidades, pasta: &Path, nome: &str, args: Vec<String>) -> Comando {
    Comando {
        nome: nome.into(),
        programa: cap.binario.clone(),
        args,
        pasta: pasta.to_path_buf(),
        ambiente: Vec::new(),
        sem_ambiente: Vec::new(),
        stderr: pasta.join(format!("{nome}.stderr")),
    }
}

fn exigir_zero(cmd: &Comando, t: exec::Termino) -> Result<()> {
    exec::exigir_sucesso(cmd, &t)
}

/// Decodifica para memória por pipe: `-vf <filtro>` e saída `yuv4mpegpipe` (com `-strict -1`) ou `ppm`.
fn decodificar(cap: &Capacidades, video: &Path, filtro: &str, saida: &str, nome: &str) -> Result<Vec<u8>> {
    let p = fontes::pasta_temporaria("medir");
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-i"].iter().map(|s| s.to_string()).collect();
    a.push(video.to_string_lossy().into_owned());
    a.extend(["-map", "0:v:0", "-fps_mode", "passthrough", "-vf", filtro].iter().map(|s| s.to_string()));
    if saida == "ppm" {
        a.extend(["-f", "image2pipe", "-c:v", "ppm", "-"].iter().map(|s| s.to_string()));
    } else {
        a.extend(["-f", saida, "-strict", "-1", "-"].iter().map(|s| s.to_string()));
    }
    let cmd = comando(cap, &p, nome, a);
    let (t, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false))?;
    exigir_zero(&cmd, t)?;
    Ok(out)
}

/// `YUV4MPEG2 W.. H.. … Cmono` + quadros `FRAME…\n` com a luma.
fn y4m(b: &[u8]) -> Result<(usize, usize, Vec<&[u8]>)> {
    let erro = || SicroError::Validation("y4m inválido".into());
    let fim = b.iter().position(|&c| c == b'\n').ok_or_else(erro)?;
    let cab = String::from_utf8_lossy(&b[..fim]);
    let num = |p: char| cab.split_whitespace().find_map(|t| t.strip_prefix(p).and_then(|v| v.parse::<usize>().ok()));
    let (w, h) = (num('W').ok_or_else(erro)?, num('H').ok_or_else(erro)?);
    if !cab.contains("Cmono") {
        return Err(SicroError::Validation(format!("y4m sem Cmono: {cab}")));
    }
    let mut i = fim + 1;
    let mut q = Vec::new();
    while i < b.len() {
        let f = i + b[i..].iter().position(|&c| c == b'\n').ok_or_else(erro)? + 1;
        if b.len() < f + w * h {
            return Err(erro());
        }
        q.push(&b[f..f + w * h]);
        i = f + w * h;
    }
    Ok((w, h, q))
}

/// Sequência de PPM (P6, 8 bits) do `image2pipe`.
fn ppms(b: &[u8]) -> Result<Vec<QuadroRgb>> {
    let erro = || SicroError::Validation("ppm inválido".into());
    let mut v = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let mut campos = Vec::new();
        while campos.len() < 4 {
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            let ini = i;
            while i < b.len() && !b[i].is_ascii_whitespace() {
                i += 1;
            }
            if ini == i {
                return Err(erro());
            }
            campos.push(String::from_utf8_lossy(&b[ini..i]).into_owned());
        }
        i += 1;
        if campos[0] != "P6" || campos[3] != "255" {
            return Err(erro());
        }
        let (w, h): (u32, u32) = (campos[1].parse().map_err(|_| erro())?, campos[2].parse().map_err(|_| erro())?);
        let n = (w * h * 3) as usize;
        if b.len() < i + n {
            return Err(erro());
        }
        v.push(QuadroRgb { w, h, px: b[i..i + n].to_vec() });
        i += n;
    }
    Ok(v)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::testes::fontes::{self, Fonte};

    fn faixas(n: u32, w: usize, h: usize, cheia: bool, ganho: f64, desvio: f64) -> Vec<u8> {
        let d = [0, 9, n / 1000 % 10, n / 100 % 10, n / 10 % 10, n % 10];
        let mut y = Vec::with_capacity(w * h);
        for r in 0..h {
            for x in 0..w {
                let v = fontes::nivel_do_digito(d[x * 6 / w], cheia) as f64 * ganho + desvio + ((x * 7 + r * 13) % 9) as f64 - 4.0;
                y.push(v.clamp(0.0, 255.0) as u8);
            }
        }
        y
    }

    #[test]
    fn identifica_pelas_faixas_com_ruido_e_faixa_trocada() {
        for n in [0, 7, 45, 449, 898, 9999] {
            assert_eq!(identificar(&faixas(n, 320, 180, false, 1.0, 0.0), 320, 180), n);
            // faixa cheia lida depois da tabela do §3.3 (16 + v·219/255) e com escala
            assert_eq!(identificar(&faixas(n, 160, 90, true, 219.0 / 255.0, 16.0), 160, 90), n);
        }
        // marca cobrindo parte das faixas não muda a mediana
        let mut y = faixas(321, 320, 180, false, 1.0, 0.0);
        for r in 100..150 {
            y[r * 320..r * 320 + 200].fill(81);
        }
        assert_eq!(identificar(&y, 320, 180), 321);
        // convenção antiga: luma 16 + N
        assert_eq!(identificar(&vec![16 + 37; 64 * 48], 64, 48), 37);
        assert_eq!(identificar(&(0..64 * 48).map(|i| (i % 251) as u8).collect::<Vec<_>>(), 64, 48), NAO_IDENTIFICADO);
    }

    #[test]
    fn marca_centro_cor_e_diferencas() {
        let mut q = QuadroRgb { w: 10, h: 8, px: vec![0; 10 * 8 * 3] };
        for y in 2..5 {
            for x in 3..7 {
                let i = (y * 10 + x) * 3;
                q.px[i..i + 3].copy_from_slice(&[250, 5, 3]);
            }
        }
        assert_eq!(marca(&q, [255, 0, 0], 10), Some([3, 2, 4, 3]));
        assert_eq!(centro(&q, [255, 0, 0], 10), Some((5.0, 3.5)));
        assert_eq!(marca(&q, [0, 255, 0], 10), None);
        assert_eq!(cor_media(&q, [3, 2, 4, 3]), [250.0, 5.0, 3.0]);
        let mut r = q.clone();
        r.px[0] = 9;
        r.px[5] = 1;
        assert_eq!(pixels_diferentes(&q, &r, 0), 2);
        assert_eq!(pixels_diferentes(&q, &r, 8), 1);
        assert_eq!(coluna_de_hash("#tb 0: 1/30\n0,          0,          0,        1,    86400, ab12\n0, 1, 1, 1, 86400, cd34\n"), ["ab12", "cd34"]);
    }

    /// O número do quadro sobrevive a escala e a perda forte (proxy, prévia): 160x90, crf 35, com quadros B.
    #[test]
    fn identifica_depois_de_escala_e_perda() {
        let Some(cap) = fontes::cap() else { return };
        let fonte = fontes::gerar(Fonte::Ts5994).unwrap();
        let d = fontes::pasta_temporaria("medir-perda");
        let args = [
            "-hide_banner", "-nostdin", "-loglevel", "error", "-i", &fonte.to_string_lossy(), "-frames:v", "200", "-vf", "scale=160:90", "-an",
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "35", "-fps_mode", "passthrough", "p.mp4",
        ];
        let c = comando(&cap, &d, "perda", args.iter().map(|s| s.to_string()).collect());
        exigir_zero(&c, exec::rodar(&c, &AtomicBool::new(false)).unwrap()).unwrap();
        assert_eq!(quadros_de_origem(&cap, &d.join("p.mp4")).unwrap(), (0..200).collect::<Vec<u32>>(), "{}", cap.versao);
        // a marca continua no lugar (em px do quadro reduzido)
        let q = quadro(&cap, &d.join("p.mp4"), 137).unwrap();
        let e = fontes::marca_esperada(Fonte::Ts5994, 137);
        let (cx, cy) = centro(&q, [255, 0, 0], 90).unwrap();
        assert!((cx - (e[0] + e[2] / 2.0) * 160.0).abs() <= 2.0 && (cy - (e[1] + e[3] / 2.0) * 90.0).abs() <= 2.0, "{cx} {cy} {e:?}");
    }

    /// Arquivo vazio, truncado ou ausente é erro com o stderr do ffmpeg, nunca lista vazia nem pânico.
    #[test]
    fn arquivo_vazio_ou_truncado_e_erro() {
        let Some(cap) = fontes::cap() else { return };
        let d = fontes::pasta_temporaria("medir vazio d'água");
        let vazio = d.join("vazio é.mp4");
        std::fs::write(&vazio, b"").unwrap();
        let inteiro = std::fs::read(fontes::gerar(Fonte::Mp4).unwrap()).unwrap();
        let truncado = d.join("truncado.mp4");
        // sem o moov, que fica no fim
        std::fs::write(&truncado, &inteiro[..inteiro.len() / 2]).unwrap();
        for v in [&vazio, &truncado, &d.join("ausente.mp4")] {
            assert!(quadros_de_origem(&cap, v).is_err(), "{}", v.display());
            assert!(pts(&cap, v).is_err());
            assert!(quadro(&cap, v, 0).is_err());
            assert!(framehash(&cap, v).is_err());
        }
        // caminho com espaço, acento e apóstrofo
        let copia = d.join("cópia d'água.mp4");
        std::fs::write(&copia, &inteiro).unwrap();
        assert_eq!(quadros_de_origem(&cap, &copia).unwrap().len(), 450);
        assert!(quadro(&cap, &copia, 450).is_err());
    }

    #[test]
    fn hashes_amostras_e_quadros_da_fonte() {
        let Some(cap) = fontes::cap() else { return };
        let v = fontes::gerar(Fonte::Mp4).unwrap();
        let h = framehash(&cap, &v).unwrap();
        assert_eq!(h.len(), 450);
        assert_eq!(h[0].len(), 64);
        // quadros sem perda: o mesmo desenho dá o mesmo hash na cópia por remux
        let k = fontes::gerar(Fonte::Mkv30).unwrap();
        assert_eq!(framehash(&cap, &k).unwrap(), h, "{}", cap.versao);
        let a = amostras(&cap, &v).unwrap();
        assert!((a as i64 - 15 * 48000).abs() <= 2048, "{a} amostras ({})", cap.versao);
        // um quadro inteiro em memória: faixas e marca
        let q = quadro(&cap, &v, 10).unwrap();
        assert_eq!((q.w, q.h), (320, 180));
        let todos = quadros(&cap, &fontes::gerar(Fonte::FaixaCheia).unwrap()).unwrap();
        assert_eq!(todos.len(), 150);
        // faixa cheia: nível alto (255) chega branco
        let branco = cor_media(&todos[0], [320 / 6 + 10, 10, 20, 20]);
        assert!(branco.iter().all(|c| *c > 250.0), "{branco:?}");
    }
}
