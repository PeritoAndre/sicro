//! Matriz de fontes sintéticas (§11; receitas de `cm/00-fontes.sh`, `cm/07c`, `cm/08`, `cm/09-fontes-retomada.sh`
//! e `cm/17-diversos.py`). O conteúdo é desenhado aqui, igual para os dois binários, no espaço exibido:
//! seis faixas verticais cinza `[baixo][alto][milhar][centena][dezena][unidade]` com os dígitos do número do
//! quadro j em degraus de 1/9 entre o nível baixo e o alto, e um quadrado vermelho que anda (`marca_esperada`).
//! `medir::quadros_de_origem` lê as faixas pela mediana e calibra pelos dois níveis: resiste a escala, perda,
//! faixa cheia e marca por cima. Geradas uma vez por binário em `temp_dir()/sicro-edicao-fontes/<sha>/`.
//! Quadro-chave a cada `Descricao::gop` quadros codificados, sem corte de cena (`-sc_threshold 0`): as faixas
//! mudam a cada 10 quadros e o x264 poria quadros-chave fora do passo.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use crate::edicao::ffmpeg::exec::{self, Comando};
use crate::edicao::ffmpeg::sonda::{self, Capacidades};
use crate::error::{Result, SicroError};

/// As fontes da matriz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fonte {
    /// MP4 30 qps, base 1/15360.
    Mp4,
    /// MP4 de escala 1000.
    Mp4Escala1000,
    /// MP4 de escala 30.
    Mp4Escala30,
    /// MKV 30 qps (base 1/1000).
    Mkv30,
    /// MKV 23,976 qps.
    Mkv23976,
    /// AVI 25 qps (sem pts nos pacotes).
    Avi25,
    /// TS 59,94 qps com início ≠ 0.
    Ts5994,
    /// MP4 com quadros B.
    Mp4QuadrosB,
    /// MP4 com lista de edição.
    Mp4ListaDeEdicao,
    /// TS começando no meio do GOP.
    TsMeioDoGop,
    MpegPs,
    TaxaVariavel,
    /// Oscilação de ±4 ms nos pts.
    Oscilacao,
    FaixaCheia,
    Entrelacado,
    /// Pixel 10:11.
    PixelNaoQuadrado,
    Giro90,
}

/// Todas, na ordem da matriz.
pub const MATRIZ: &[Fonte] = &[
    Fonte::Mp4,
    Fonte::Mp4Escala1000,
    Fonte::Mp4Escala30,
    Fonte::Mkv30,
    Fonte::Mkv23976,
    Fonte::Avi25,
    Fonte::Ts5994,
    Fonte::Mp4QuadrosB,
    Fonte::Mp4ListaDeEdicao,
    Fonte::TsMeioDoGop,
    Fonte::MpegPs,
    Fonte::TaxaVariavel,
    Fonte::Oscilacao,
    Fonte::FaixaCheia,
    Fonte::Entrelacado,
    Fonte::PixelNaoQuadrado,
    Fonte::Giro90,
];

// CONTRATO: acréscimo. O que cada fonte é, para os testes conferirem sem refazer a conta.
/// `largura`×`altura`: o quadro decodificado (giro aplicado; pixel 10:11 em pixels codificados).
/// `n_quadros`: quadros desenhados antes de corte e seleção (o quadro j traz o número j). Áudio: cliques
/// em x,5 s sobre seno de 220 Hz (`cm/19`). `gop`: passo dos quadros-chave, em quadros codificados (na taxa
/// variável, os mantidos; na lista de edição e no TS cortado, os da fonte inteira).
#[derive(Debug, Clone, PartialEq)]
pub struct Descricao {
    pub arquivo: &'static str,
    pub largura: u32,
    pub altura: u32,
    pub fps: (i64, i64),
    pub n_quadros: u32,
    pub gop: u32,
    pub audio: bool,
    pub entrelacado: bool,
    pub faixa_cheia: bool,
    pub sar: (u32, u32),
    pub giro: i32,
}

/// Versão do desenho e das receitas: mudar invalida o cache de todas as máquinas.
const VERSAO_MATRIZ: u32 = 4;
/// Início do TS 59,94 (s), somado ao atraso padrão do muxer.
const INICIO_TS: u32 = 3600;
/// Corte da lista de edição: `-ss 1.5 … -t 10` sobre a fonte com quadros B (`cm/08`).
const CORTE_SS: f64 = 1.5;
const CORTE_T: f64 = 10.0;

// CONTRATO: acréscimo.
/// Descrição da fonte.
pub fn descricao(f: Fonte) -> Descricao {
    let base = |arquivo, fps, n, audio| Descricao {
        arquivo,
        largura: 320,
        altura: 180,
        fps,
        n_quadros: n,
        gop: 30,
        audio,
        entrelacado: false,
        faixa_cheia: false,
        sar: (1, 1),
        giro: 0,
    };
    match f {
        Fonte::Mp4 => base("mp4-15360.mp4", (30, 1), 450, true),
        Fonte::Mp4Escala1000 => base("mp4-escala1000.mp4", (30, 1), 450, true),
        Fonte::Mp4Escala30 => base("mp4-escala30.mp4", (30, 1), 450, true),
        Fonte::Mkv30 => base("mkv-30.mkv", (30, 1), 450, true),
        Fonte::Mkv23976 => Descricao { gop: 24, ..base("mkv-23976.mkv", (24000, 1001), 360, false) },
        Fonte::Avi25 => Descricao { gop: 25, ..base("avi-25.avi", (25, 1), 375, true) },
        Fonte::Ts5994 => Descricao { gop: 60, ..base("ts-5994.ts", (60000, 1001), 899, true) },
        Fonte::Mp4QuadrosB => base("mp4-quadros-b.mp4", (30, 1), 450, true),
        Fonte::Mp4ListaDeEdicao => base("mp4-lista-de-edicao.mp4", (30, 1), 450, true),
        Fonte::TsMeioDoGop => Descricao { gop: 60, ..base("ts-meio-do-gop.ts", (30, 1), 450, true) },
        Fonte::MpegPs => base("mpeg-ps.mpg", (30, 1), 450, true),
        Fonte::TaxaVariavel => base("taxa-variavel.mp4", (30, 1), 450, true),
        Fonte::Oscilacao => base("oscilacao-4ms.mkv", (30, 1), 450, true),
        Fonte::FaixaCheia => Descricao { faixa_cheia: true, ..base("faixa-cheia.mp4", (30, 1), 150, false) },
        Fonte::Entrelacado => Descricao {
            largura: 720,
            altura: 480,
            entrelacado: true,
            ..base("entrelacado.mp4", (30000, 1001), 180, false)
        },
        Fonte::PixelNaoQuadrado => Descricao {
            largura: 720,
            altura: 480,
            sar: (10, 11),
            ..base("pixel-10-11.mp4", (30000, 1001), 180, false)
        },
        Fonte::Giro90 => Descricao { largura: 180, altura: 320, giro: 90, ..base("giro-90.mp4", (30, 1), 450, false) },
    }
}

/// Quadros da fonte selecionados pela taxa variável (os outros são descartados com os pts originais; `p1a/00-fontes.sh`).
fn mantido_na_taxa_variavel(n: u32) -> bool {
    !(n % 7 == 3 || n % 11 == 5 || (100..=112).contains(&n))
}

// CONTRATO: acréscimo.
/// Números dos quadros que o decodificador entrega, na ordem; None = depende do muxer e do decodificador:
/// TS cortado no pacote do quadro 150 (entrega do quadro-chave seguinte, o 180, em diante) e lista de edição
/// (começa no quadro de 1,5 s, o 45; no fim, a cópia corta quadros B e deixa um P adiante: `…, 344, 345, 348`).
pub fn quadros_esperados(f: Fonte) -> Option<Vec<u32>> {
    let d = descricao(f);
    match f {
        Fonte::TsMeioDoGop | Fonte::Mp4ListaDeEdicao => None,
        Fonte::TaxaVariavel => Some((0..d.n_quadros).filter(|&n| mantido_na_taxa_variavel(n)).collect()),
        _ => Some((0..d.n_quadros).collect()),
    }
}

// ---------------------------------------------------------------- desenho

#[derive(Debug, Clone, Copy)]
struct Leiaute {
    /// Tamanho codificado.
    wc: u32,
    hc: u32,
    giro: bool,
    sar: (u32, u32),
    cheia: bool,
    entrelacado: bool,
    fps: (i64, i64),
}

impl Leiaute {
    fn de(d: &Descricao) -> Self {
        let giro = d.giro == 90;
        let (wc, hc) = if giro { (d.altura, d.largura) } else { (d.largura, d.altura) };
        Leiaute { wc, hc, giro, sar: d.sar, cheia: d.faixa_cheia, entrelacado: d.entrelacado, fps: d.fps }
    }

    /// Grade com o giro aplicado (a do quadro decodificado).
    fn orientado(&self) -> (u32, u32) {
        if self.giro {
            (self.hc, self.wc)
        } else {
            (self.wc, self.hc)
        }
    }
}

// CONTRATO: acréscimo. Níveis de luma das faixas, para quem desenhar uma fonte própria.
/// Nível de luma do dígito `d` (0..9): de 16 a 235 (faixa limitada) ou de 0 a 255 (faixa cheia).
pub fn nivel_do_digito(d: u32, faixa_cheia: bool) -> u8 {
    let (a, b) = if faixa_cheia { (0.0, 255.0) } else { (16.0, 235.0) };
    (a + d as f64 * (b - a) / 9.0).round() as u8
}

/// Vermelho #ff0000 em YUV BT.601 (fonte sem etiqueta, altura ≤ 576: §3.3).
fn vermelho(cheia: bool) -> [u8; 3] {
    if cheia {
        [76, 85, 255]
    } else {
        [81, 90, 240]
    }
}

fn par(v: f64) -> u32 {
    ((v / 2.0).round().max(1.0) * 2.0) as u32
}

/// Caixa do quadrado vermelho no instante `tau` (s), em px da grade orientada, com bordas pares (4:2:0 sem mistura).
/// Lado = 1/6 da altura exibida; vai e volta na horizontal a cada 4 s, centro em y = 0,72.
fn caixa_marca(lei: &Leiaute, tau: f64) -> [u32; 4] {
    let (wo, ho) = lei.orientado();
    // entrelaçado: múltiplos de 4 na vertical (cada linha de croma de um campo cobre 2 linhas dele)
    let pv = |v: f64| if lei.entrelacado { ((v / 4.0).round().max(1.0) * 4.0) as u32 } else { par(v) };
    let h = pv(ho as f64 / 6.0);
    let w = par(h as f64 * lei.sar.1 as f64 / lei.sar.0 as f64);
    let fase = (tau / 4.0).fract();
    let tri = 1.0 - (2.0 * fase - 1.0).abs();
    let uc = 0.15 + 0.7 * tri;
    let x = par(uc * wo as f64 - w as f64 / 2.0).min(wo - w);
    let y = pv(0.72 * ho as f64 - h as f64 / 2.0).min(ho - h);
    [x, y, w, h]
}

fn tau(lei: &Leiaute, j: u32, campo_de_baixo: bool) -> f64 {
    (j as f64 + if campo_de_baixo { 0.5 } else { 0.0 }) * lei.fps.1 as f64 / lei.fps.0 as f64
}

// CONTRATO: acréscimo.
/// Caixa do quadrado vermelho no quadro j, em fração do quadro decodificado `[x, y, w, h]` (entrelaçado: campo de cima).
pub fn marca_esperada(f: Fonte, j: u32) -> [f64; 4] {
    marca_esperada_no_campo(f, j, false)
}

// CONTRATO: acréscimo.
/// Como `marca_esperada`, no campo de cima ou no de baixo (meio quadro depois) da fonte entrelaçada.
pub fn marca_esperada_no_campo(f: Fonte, j: u32, de_baixo: bool) -> [f64; 4] {
    let lei = Leiaute::de(&descricao(f));
    let (wo, ho) = lei.orientado();
    let c = caixa_marca(&lei, tau(&lei, j, de_baixo && lei.entrelacado));
    [c[0] as f64 / wo as f64, c[1] as f64 / ho as f64, c[2] as f64 / wo as f64, c[3] as f64 / ho as f64]
}

/// Quadro j em yuv420p (ou yuvj420p), planos em sequência, na grade codificada.
fn desenhar(lei: &Leiaute, j: u32) -> Vec<u8> {
    assert!(j < 10_000);
    let (wo, ho) = lei.orientado();
    let (wo, ho) = (wo as usize, ho as usize);
    let digitos = [0, 9, j / 1000 % 10, j / 100 % 10, j / 10 % 10, j % 10];
    let linha: Vec<u8> = (0..wo).map(|x| nivel_do_digito(digitos[x * 6 / wo], lei.cheia)).collect();
    let mut y = Vec::with_capacity(wo * ho);
    for _ in 0..ho {
        y.extend_from_slice(&linha);
    }
    let cor = vermelho(lei.cheia);
    let cima = caixa_marca(lei, tau(lei, j, false));
    let baixo = if lei.entrelacado { caixa_marca(lei, tau(lei, j, true)) } else { cima };
    for (r, row) in y.chunks_mut(wo).enumerate() {
        let c = if r % 2 == 1 { baixo } else { cima };
        if (c[1] as usize..(c[1] + c[3]) as usize).contains(&r) {
            row[c[0] as usize..(c[0] + c[2]) as usize].fill(cor[0]);
        }
    }
    let (cw, ch) = (wo / 2, ho / 2);
    let mut u = vec![128u8; cw * ch];
    let mut v = vec![128u8; cw * ch];
    for cy in (cima[1] / 2) as usize..((cima[1] + cima[3]) / 2) as usize {
        // 4:2:0 entrelaçado: linha de croma par é do campo de cima, ímpar do de baixo
        let c = if cy % 2 == 1 { baixo } else { cima };
        for cx in (c[0] / 2) as usize..((c[0] + c[2]) / 2) as usize {
            u[cy * cw + cx] = cor[1];
            v[cy * cw + cx] = cor[2];
        }
    }
    if !lei.giro {
        y.extend(u);
        y.extend(v);
        return y;
    }
    // giro de 90° no contêiner (`-display_rotation 90`): exibido (xo, yo) = codificado (yo', ...) com
    // codificado (x, y) → exibido (y, wc−1−x); medido nos dois binários
    let girar = |p: &[u8], wc: usize, hc: usize| -> Vec<u8> {
        let wo = hc;
        let mut o = vec![0u8; wc * hc];
        for yc in 0..hc {
            for xc in 0..wc {
                o[yc * wc + xc] = p[(wc - 1 - xc) * wo + yc];
            }
        }
        o
    };
    let (wc, hc) = (lei.wc as usize, lei.hc as usize);
    let mut q = girar(&y, wc, hc);
    q.extend(girar(&u, wc / 2, hc / 2));
    q.extend(girar(&v, wc / 2, hc / 2));
    q
}

// ---------------------------------------------------------------- ffmpeg dos testes

/// ffmpeg dos testes (`SICRO_FFMPEG` ou o do sistema). None = não há; com `SICRO_EXIGE_FFMPEG=1` é pânico.
/// É o `ffmpeg_dos_testes()` do §11: teste com ffmpeg começa por `let Some(cap) = fontes::cap() else { return };`.
pub fn cap() -> Option<Arc<Capacidades>> {
    static C: OnceLock<Option<Arc<Capacidades>>> = OnceLock::new();
    let c = C
        .get_or_init(|| {
            let r = sonda::escolher().and_then(|(bin, origem)| sonda::capacidades_de(&bin, origem, None, false));
            // binário sem um obrigatório (ou que não é ffmpeg) não serve aos testes
            let r = r.and_then(|c| match c.faltam.is_empty() {
                true => Ok(c),
                false => Err(SicroError::Validation(format!("{}: faltam {}", c.binario.display(), c.faltam.join(", ")))),
            });
            match r {
                Ok(c) => {
                    eprintln!("ffmpeg dos testes: {} ({}, {:?})", c.versao, c.binario.display(), c.origem);
                    Some(c)
                }
                Err(e) => {
                    eprintln!("ffmpeg dos testes: {e}");
                    None
                }
            }
        })
        .clone();
    if abaixo_do_minimo() {
        assert!(c.is_none(), "SICRO_FFMPEG_ABAIXO_DO_MINIMO=1 e a sonda aprovou o ffmpeg");
        eprintln!("aviso: teste dispensado, ffmpeg abaixo do mínimo");
        return None;
    }
    decidir(c, std::env::var("SICRO_EXIGE_FFMPEG").is_ok_and(|v| v == "1"))
}

// CONTRATO: acréscimo.
/// `SICRO_FFMPEG_ABAIXO_DO_MINIMO=1` (CI da Ubuntu 22.04, ffmpeg 4.4): a sonda TEM de reprovar o ffmpeg dos testes,
/// e os testes que precisam de um utilizável ficam dispensados.
pub fn abaixo_do_minimo() -> bool {
    std::env::var("SICRO_FFMPEG_ABAIXO_DO_MINIMO").is_ok_and(|v| v == "1")
}

// CONTRATO: acréscimo.
/// O outro ffmpeg da máquina (o do sistema ou o 7.0.2 do AppImage). Falta dele com `SICRO_EXIGE_FFMPEG=1` é falha
/// só no Linux: o instalador do Windows leva um ffmpeg só.
pub fn outro_ffmpeg(cap: &Capacidades) -> Option<Arc<Capacidades>> {
    let candidatos = [Path::new(env!("CARGO_MANIFEST_DIR")).join("ffmpeg-linux/ffmpeg"), PathBuf::from("/usr/bin/ffmpeg")];
    let mesmo = |p: &Path| std::fs::canonicalize(p).ok() == std::fs::canonicalize(&cap.binario).ok();
    let achado = candidatos.iter().find(|p| p.exists() && !mesmo(p)).and_then(|p| sonda::capacidades_de(p, sonda::Origem::Configurado, None, false).ok());
    let achado = achado.filter(|c| c.faltam.is_empty() && c.sha256 != cap.sha256);
    if achado.is_none() {
        let exige = cfg!(target_os = "linux") && std::env::var("SICRO_EXIGE_FFMPEG").is_ok_and(|v| v == "1");
        assert!(!exige, "SICRO_EXIGE_FFMPEG=1 e o segundo ffmpeg não foi achado");
        eprintln!("aviso: comparação entre binários dispensada, sem o segundo ffmpeg");
    }
    achado
}

fn decidir(c: Option<Arc<Capacidades>>, exige: bool) -> Option<Arc<Capacidades>> {
    if c.is_none() {
        assert!(!exige, "SICRO_EXIGE_FFMPEG=1 e nenhum ffmpeg utilizável");
        eprintln!("aviso: teste dispensado, sem ffmpeg (SICRO_EXIGE_FFMPEG=1 faz falhar)");
    }
    c
}

/// Caminho da fonte, gerada uma vez por binário em `temp_dir()/sicro-edicao-fontes/<sha>/v<versão>/`
/// (fora do repositório; reaproveitada entre execuções e entre agentes, com trava por arquivo).
pub fn gerar(fonte: Fonte) -> Result<PathBuf> {
    let cap = cap().ok_or_else(|| SicroError::Validation("ffmpeg_ausente".into()))?;
    gerar_em(&pasta_da_matriz(&cap), &cap, fonte)
}

fn pasta_da_matriz(cap: &Capacidades) -> PathBuf {
    std::env::temp_dir()
        .join("sicro-edicao-fontes")
        .join(&cap.sha256[..16])
        .join(format!("v{VERSAO_MATRIZ}"))
}

/// Gera (ou reaproveita) a fonte em `pasta`. A trava (`<arquivo>.trava`, `File::lock`) serializa threads e processos;
/// o arquivo só aparece pronto (renomeado da pasta de trabalho) e o `<arquivo>.ok` marca o fim.
pub(crate) fn gerar_em(pasta: &Path, cap: &Capacidades, fonte: Fonte) -> Result<PathBuf> {
    let d = descricao(fonte);
    let destino = pasta.join(d.arquivo);
    let ok = pasta.join(format!("{}.ok", d.arquivo));
    if ok.exists() && destino.exists() {
        return Ok(destino);
    }
    std::fs::create_dir_all(pasta)?;
    let base = match fonte {
        Fonte::Mp4Escala1000 | Fonte::Mp4Escala30 | Fonte::Mkv30 => Some(gerar_em(pasta, cap, Fonte::Mp4)?),
        Fonte::Oscilacao => Some(gerar_em(pasta, cap, Fonte::Mkv30)?),
        Fonte::Mp4ListaDeEdicao => Some(gerar_em(pasta, cap, Fonte::Mp4QuadrosB)?),
        _ => None,
    };
    let trava = File::create(pasta.join(format!("{}.trava", d.arquivo)))?;
    trava.lock()?;
    if ok.exists() && destino.exists() {
        return Ok(destino);
    }
    static N: AtomicU64 = AtomicU64::new(0);
    let trabalho = pasta.join(format!("tmp-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&trabalho);
    std::fs::create_dir_all(&trabalho)?;
    let r = produzir(cap, &trabalho, fonte, &d, base.as_deref());
    if r.is_ok() {
        std::fs::rename(trabalho.join(d.arquivo), &destino)?;
        std::fs::write(&ok, VERSAO_MATRIZ.to_string())?;
    }
    let _ = std::fs::remove_dir_all(&trabalho);
    r.map(|_| destino)
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// Cliques em x,5 s sobre seno de 220 Hz (`cm/19-audio-buraco.sh`).
fn audio_lavfi(taxa: u32, canais: &str, dur: f64) -> String {
    format!("aevalsrc=exprs=0.9*lt(mod(t+0.5\\,1)\\,0.001)+0.02*sin(2*PI*220*t):s={taxa}:c={canais}:d={dur}")
}

/// Codifica os quadros desenhados (rawvideo pelo stdin) com o áudio e os argumentos de saída.
fn codificar(cap: &Capacidades, pasta: &Path, d: &Descricao, audio: Option<(u32, &str, &[&str])>, saida: &[&str], arquivo: &str) -> Result<()> {
    let lei = Leiaute::de(d);
    let dur = d.n_quadros as f64 * d.fps.1 as f64 / d.fps.0 as f64;
    let mut a = s(&[
        "-hide_banner",
        "-nostdin",
        "-y",
        "-loglevel",
        "error",
        "-f",
        "rawvideo",
        "-pix_fmt",
        if d.faixa_cheia { "yuvj420p" } else { "yuv420p" },
        "-s",
        &format!("{}x{}", lei.wc, lei.hc),
        "-framerate",
        &format!("{}/{}", d.fps.0, d.fps.1),
        "-i",
        "-",
    ]);
    if let Some((taxa, canais, _)) = audio {
        a.extend(s(&["-f", "lavfi", "-i", &audio_lavfi(taxa, canais, dur)]));
        a.extend(s(&["-map", "0:v", "-map", "1:a"]));
    }
    a.extend(s(saida));
    if let Some((_, _, cod)) = audio {
        a.extend(s(cod));
    }
    a.push(arquivo.into());
    let erro = pasta.join(format!("{arquivo}.stderr"));
    let mut filho = crate::tools::command(&cap.binario)
        .args(&a)
        .current_dir(pasta)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(File::create(&erro)?)
        .spawn()?;
    let mut entrada = filho.stdin.take().expect("stdin");
    let mut escrita = Ok(());
    for j in 0..d.n_quadros {
        if let Err(e) = entrada.write_all(&desenhar(&lei, j)) {
            escrita = Err(e);
            break;
        }
    }
    drop(entrada);
    let st = filho.wait()?;
    if !st.success() || escrita.is_err() {
        return Err(SicroError::Validation(format!(
            "fonte {arquivo} ({}): {:?} {}",
            cap.versao,
            escrita.err(),
            exec::ultimas_linhas(&erro, 5).join(" | ")
        )));
    }
    Ok(())
}

/// Roda o ffmpeg (ou o ffprobe) dos testes na pasta e exige código 0; devolve o stdout.
fn rodar(cap: &Capacidades, programa: &Path, pasta: &Path, nome: &str, args: Vec<String>) -> Result<Vec<u8>> {
    let cmd = Comando {
        nome: nome.into(),
        programa: programa.to_path_buf(),
        args,
        pasta: pasta.to_path_buf(),
        ambiente: Vec::new(),
        sem_ambiente: Vec::new(),
        stderr: pasta.join(format!("{nome}.stderr")),
    };
    let (t, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false))?;
    exec::exigir_sucesso(&cmd, &t).map_err(|e| SicroError::Validation(format!("{e} ({})", cap.versao)))?;
    Ok(out)
}

fn copiar(cap: &Capacidades, pasta: &Path, base: &Path, antes: &[&str], extra: &[&str], arquivo: &str) -> Result<()> {
    let mut a = s(&["-hide_banner", "-nostdin", "-y", "-loglevel", "error"]);
    a.extend(s(antes));
    a.extend(s(&["-i", &base.to_string_lossy(), "-map", "0", "-c", "copy"]));
    a.extend(s(extra));
    a.push(arquivo.into());
    rodar(cap, &cap.binario, pasta, arquivo, a).map(|_| ())
}

const X264: &[&str] = &["-c:v", "libx264", "-preset", "veryfast", "-qp", "0", "-pix_fmt", "yuv420p", "-sc_threshold", "0"];

fn produzir(cap: &Capacidades, p: &Path, fonte: Fonte, d: &Descricao, base: Option<&Path>) -> Result<()> {
    let aac: (u32, &str, &[&str]) = (48000, "stereo", &["-c:a", "aac", "-b:a", "128k"]);
    let mp2: (u32, &str, &[&str]) = (48000, "stereo", &["-c:a", "mp2", "-b:a", "192k"]);
    let alaw: (u32, &str, &[&str]) = (8000, "mono", &["-c:a", "pcm_alaw"]);
    fn x264<'a>(extra: &[&'a str]) -> Vec<&'a str> {
        X264.iter().copied().chain(extra.iter().copied()).collect()
    }
    let a = d.arquivo;
    match fonte {
        Fonte::Mp4 => codificar(cap, p, d, Some(aac), &x264(&["-bf", "0", "-g", "30", "-video_track_timescale", "15360"]), a),
        Fonte::Mp4Escala1000 => copiar(cap, p, base.unwrap(), &[], &["-video_track_timescale", "1000"], a),
        Fonte::Mp4Escala30 => copiar(cap, p, base.unwrap(), &[], &["-video_track_timescale", "30"], a),
        Fonte::Mkv30 => copiar(cap, p, base.unwrap(), &[], &[], a),
        Fonte::Mkv23976 => codificar(cap, p, d, None, &x264(&["-bf", "0", "-g", "24"]), a),
        Fonte::Avi25 => codificar(cap, p, d, Some(alaw), &x264(&["-bf", "0", "-g", "25"]), a),
        Fonte::Ts5994 => {
            let off = INICIO_TS.to_string();
            codificar(cap, p, d, Some(mp2), &x264(&["-bf", "0", "-g", "60", "-output_ts_offset", &off, "-f", "mpegts"]), a)
        }
        // o x264 sem perda (qp 0) não faz quadro B: qp 1
        Fonte::Mp4QuadrosB => codificar(
            cap,
            p,
            d,
            Some(aac),
            &["-c:v", "libx264", "-preset", "veryfast", "-qp", "1", "-pix_fmt", "yuv420p", "-sc_threshold", "0", "-bf", "2", "-x264-params", "b-adapt=0", "-g", "30", "-video_track_timescale", "15360"],
            a,
        ),
        Fonte::Mp4ListaDeEdicao => {
            let (ss, t) = (CORTE_SS.to_string(), CORTE_T.to_string());
            copiar(cap, p, base.unwrap(), &["-ss", &ss], &["-t", &t], a)
        }
        Fonte::TsMeioDoGop => {
            codificar(cap, p, d, Some(mp2), &x264(&["-bf", "0", "-g", "60", "-f", "mpegts"]), "inteiro.ts")?;
            // corta no pacote do quadro 150 (meio do GOP 120..179), alinhado aos 188 bytes do TS (`cm/09`)
            let out = rodar(
                cap,
                &cap.ffprobe,
                p,
                "pacotes",
                s(&["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pos", "-of", "csv=p=0", "inteiro.ts"]),
            )?;
            let pos: Vec<u64> = String::from_utf8_lossy(&out).lines().filter_map(|l| l.trim().trim_end_matches(',').parse().ok()).collect();
            let corte = pos.get(150).copied().ok_or_else(|| SicroError::Validation("ts sem 150 pacotes".into()))? / 188 * 188;
            let b = std::fs::read(p.join("inteiro.ts"))?;
            std::fs::write(p.join(a), &b[corte as usize..])?;
            Ok(())
        }
        Fonte::MpegPs => codificar(cap, p, d, Some(mp2), &x264(&["-bf", "0", "-g", "30", "-f", "vob"]), a),
        Fonte::TaxaVariavel => codificar(
            cap,
            p,
            d,
            Some(aac),
            &x264(&[
                "-bf",
                "0",
                "-g",
                "30",
                "-vf",
                "select='not(eq(mod(n,7),3)+eq(mod(n,11),5)+between(n,100,112))'",
                "-fps_mode",
                "vfr",
                "-video_track_timescale",
                "15360",
            ]),
            a,
        ),
        Fonte::Oscilacao => copiar(cap, p, base.unwrap(), &[], &["-bsf:v", "setts=ts=PTS+round(4*sin(N*2.3))"], a),
        Fonte::FaixaCheia => codificar(
            cap,
            p,
            d,
            None,
            &["-c:v", "libx264", "-preset", "veryfast", "-qp", "0", "-pix_fmt", "yuvj420p", "-color_range", "pc", "-sc_threshold", "0", "-bf", "0", "-g", "30"],
            a,
        ),
        Fonte::Entrelacado => codificar(
            cap,
            p,
            d,
            None,
            &x264(&["-bf", "0", "-g", "30", "-vf", "setfield=tff", "-flags", "+ildct+ilme", "-x264-params", "tff=1"]),
            a,
        ),
        Fonte::PixelNaoQuadrado => codificar(cap, p, d, None, &x264(&["-bf", "0", "-g", "30", "-vf", "setsar=10/11"]), a),
        Fonte::Giro90 => {
            codificar(cap, p, d, None, &x264(&["-bf", "0", "-g", "30", "-video_track_timescale", "15360"]), "sem-giro.mp4")?;
            copiar(cap, p, &p.join("sem-giro.mp4"), &["-display_rotation", "90"], &[], a)
        }
    }
}

/// Amostras reais de gravador (Dahua, Hikvision, LVF, IFV) presentes em `p1b/dl/dvr/amostras`; vazio se não houver.
/// Pasta: `SICRO_AMOSTRAS_DVR`, senão a do dossiê (`dossie()`).
pub fn amostras_de_gravador() -> Vec<PathBuf> {
    let pasta = std::env::var_os("SICRO_AMOSTRAS_DVR")
        .map(PathBuf::from)
        .or_else(|| dossie().map(|d| d.join("p1b/dl/dvr/amostras")));
    let Some(Ok(rd)) = pasta.map(std::fs::read_dir) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "dav" | "mp4" | "ifv" | "lvf" | "264" | "h264" | "avi" | "ts"))
        })
        .collect();
    v.sort();
    v
}

// CONTRATO: acréscimo.
/// Pasta do dossiê da Edição (`sicro-bench/edicao-pro`): `SICRO_DOSSIE`, ou ao lado do repositório, ou em `~/Work`.
pub fn dossie() -> Option<PathBuf> {
    let mut c: Vec<PathBuf> = std::env::var_os("SICRO_DOSSIE").map(PathBuf::from).into_iter().collect();
    let manifesto = Path::new(env!("CARGO_MANIFEST_DIR"));
    c.push(manifesto.join("../../sicro-bench/edicao-pro"));
    c.push(manifesto.join("../../../sicro-bench/edicao-pro"));
    if let Some(h) = std::env::var_os("HOME") {
        c.push(PathBuf::from(h).join("Work/sicro-bench/edicao-pro"));
    }
    c.into_iter().find(|p| p.is_dir())
}

/// Pasta temporária nova para um teste (apagada no fim do processo, também quando um teste falha).
pub fn pasta_temporaria(nome: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let raiz = raiz_temporaria();
    let p = raiz.join(format!("{nome}-{}", N.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("pasta temporária");
    p
}

fn raiz_temporaria() -> &'static Path {
    static R: OnceLock<PathBuf> = OnceLock::new();
    R.get_or_init(|| {
        let mae = std::env::temp_dir().join("sicro-edicao-testes");
        limpar_orfas(&mae);
        let r = mae.join(std::process::id().to_string());
        let _ = std::fs::remove_dir_all(&r);
        std::fs::create_dir_all(&r).expect("pasta temporária");
        apagar_no_fim();
        r
    })
}

#[cfg(unix)]
fn apagar_no_fim() {
    extern "C" fn apagar() {
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("sicro-edicao-testes").join(std::process::id().to_string()));
    }
    extern "C" {
        fn atexit(f: extern "C" fn()) -> std::ffi::c_int;
    }
    unsafe {
        atexit(apagar);
    }
}

// Windows: sem `atexit`; a pasta sai na limpeza por idade da próxima execução.
#[cfg(not(unix))]
fn apagar_no_fim() {}

/// Pastas de processos mortos (Linux, por `/proc`) ou com mais de 6 h (demais sistemas).
fn limpar_orfas(mae: &Path) {
    let Ok(rd) = std::fs::read_dir(mae) else { return };
    for e in rd.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
        let orfa = if cfg!(target_os = "linux") {
            !Path::new(&format!("/proc/{pid}")).exists()
        } else {
            e.metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| t.elapsed().is_ok_and(|d| d > std::time::Duration::from_secs(6 * 3600)))
        };
        if orfa && pid != std::process::id() {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::testes::medir;

    #[test]
    #[should_panic(expected = "SICRO_EXIGE_FFMPEG=1")]
    fn sem_ffmpeg_exigido_falha() {
        decidir(None, true);
    }

    #[test]
    fn sem_ffmpeg_sem_exigencia_dispensa() {
        assert!(decidir(None, false).is_none());
    }

    #[test]
    fn cap_respeita_sicro_ffmpeg() {
        let Some(cap) = cap() else { return };
        assert!(!cap.versao.is_empty());
        match std::env::var_os("SICRO_FFMPEG").filter(|v| !v.is_empty()) {
            Some(v) => {
                assert_eq!(cap.binario, std::path::absolute(PathBuf::from(v)).unwrap());
                assert_eq!(cap.origem, sonda::Origem::Configurado);
            }
            None => assert_ne!(cap.origem, sonda::Origem::Configurado),
        }
    }

    #[test]
    fn desenho_tem_o_tamanho_e_o_giro_certos() {
        for &f in MATRIZ {
            let d = descricao(f);
            let lei = Leiaute::de(&d);
            assert_eq!(desenhar(&lei, 0).len(), (lei.wc * lei.hc * 3 / 2) as usize, "{f:?}");
            let c = caixa_marca(&lei, 1.3);
            assert!(c.iter().all(|v| v % 2 == 0), "{f:?} {c:?}");
        }
        // no giro, o quadrado do exibido cai onde o ffmpeg o põe: codificado (x, y) → exibido (y, wc−1−x)
        let lei = Leiaute::de(&descricao(Fonte::Giro90));
        let q = desenhar(&lei, 7);
        let c = caixa_marca(&lei, tau(&lei, 7, false));
        let (xo, yo) = (c[0] as usize + 1, c[1] as usize + 1);
        let (xc, yc) = (lei.wc as usize - 1 - yo, xo);
        assert_eq!(q[yc * lei.wc as usize + xc], vermelho(false)[0]);
    }

    fn ffprobe(cap: &Capacidades, video: &Path) -> serde_json::Value {
        let p = pasta_temporaria("ffprobe");
        let out = rodar(
            cap,
            &cap.ffprobe,
            &p,
            "ffprobe",
            s(&[
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,time_base,start_time,sample_aspect_ratio,field_order,color_range,pix_fmt,has_b_frames,avg_frame_rate,r_frame_rate:stream_side_data=rotation:format=format_name,start_time",
                "-of",
                "json",
                &video.to_string_lossy(),
            ]),
        )
        .unwrap();
        serde_json::from_slice(&out).unwrap()
    }

    /// Linhas CSV do ffprobe para o fluxo de vídeo (`entradas` = `packet=flags`, `frame=pict_type`…).
    fn ffprobe_csv(cap: &Capacidades, video: &Path, antes: &[&str], entradas: &str) -> Vec<String> {
        let p = pasta_temporaria("ffprobe-csv");
        let mut a = s(&["-v", "error"]);
        a.extend(s(antes));
        a.extend(s(&["-select_streams", "v:0", "-show_entries", entradas, "-of", "csv=p=0", &video.to_string_lossy()]));
        // o TS cortado acusa o que não decodifica no começo: vale o stdout
        let cmd = Comando {
            nome: "ffprobe-csv".into(),
            programa: cap.ffprobe.clone(),
            args: a,
            pasta: p.clone(),
            ambiente: Vec::new(),
            sem_ambiente: Vec::new(),
            stderr: p.join("ffprobe.stderr"),
        };
        let (t, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).unwrap();
        assert_eq!(t.codigo, Some(0), "{:?}", exec::ultimas_linhas(&cmd.stderr, 3));
        String::from_utf8_lossy(&out).lines().map(|l| l.trim().trim_end_matches(',').to_string()).filter(|l| !l.is_empty()).collect()
    }

    /// Índices (ordem de decodificação) dos pacotes de vídeo que são quadro-chave.
    fn chaves(cap: &Capacidades, video: &Path) -> Vec<usize> {
        ffprobe_csv(cap, video, &[], "packet=flags").iter().enumerate().filter(|(_, f)| f.starts_with('K')).map(|(i, _)| i).collect()
    }

    /// Cada fonte é gerada, tem as propriedades que diz ter e todo quadro decodificado é identificado pelo conteúdo,
    /// com o quadrado vermelho no lugar (primeiro, do meio e último).
    #[test]
    fn matriz_inteira_identificavel() {
        let Some(cap) = cap() else { return };
        for &f in MATRIZ {
            let d = descricao(f);
            let v = gerar(f).unwrap_or_else(|e| panic!("{f:?} ({}): {e}", cap.versao));
            let j = ffprobe(&cap, &v);
            let video = j["streams"].as_array().unwrap().iter().find(|s| s["codec_type"] == "video").unwrap().clone();
            let tem_audio = j["streams"].as_array().unwrap().iter().any(|s| s["codec_type"] == "audio");
            assert_eq!(tem_audio, d.audio, "{f:?}");
            let campo = |k: &str| video[k].as_str().unwrap_or("").to_string();
            match f {
                Fonte::Mp4 | Fonte::Mp4QuadrosB | Fonte::Giro90 | Fonte::TaxaVariavel => assert_eq!(campo("time_base"), "1/15360", "{f:?}"),
                Fonte::Mp4Escala1000 | Fonte::Mkv30 | Fonte::Mkv23976 | Fonte::Oscilacao => assert_eq!(campo("time_base"), "1/1000", "{f:?}"),
                Fonte::Mp4Escala30 => assert_eq!(campo("time_base"), "1/30"),
                Fonte::Avi25 => assert_eq!(campo("time_base"), "1/25"),
                Fonte::Ts5994 => assert!(campo("start_time").parse::<f64>().unwrap() > INICIO_TS as f64),
                Fonte::MpegPs => assert!(j["format"]["format_name"].as_str().unwrap().contains("mpeg")),
                Fonte::FaixaCheia => assert_eq!((campo("pix_fmt").as_str(), campo("color_range").as_str()), ("yuvj420p", "pc")),
                Fonte::Entrelacado => assert_eq!(campo("field_order"), "tt"),
                Fonte::PixelNaoQuadrado => assert_eq!(campo("sample_aspect_ratio"), "10:11"),
                _ => {}
            }
            let taxa = match d.fps {
                (n, 1) => format!("{n}/1"),
                (n, m) => format!("{n}/{m}"),
            };
            assert_eq!(campo("r_frame_rate"), taxa, "{f:?}");
            // quadro-chave a cada `gop` quadros codificados, nunca fora do passo
            let k = chaves(&cap, &v);
            match f {
                // cópia cortada no quadro-chave do 30: os da fonte (30, 60, …) viram 0, 30, …
                Fonte::Mp4ListaDeEdicao => assert!(k.len() >= 10 && k.iter().all(|i| i % 30 == 0), "{f:?}: {k:?}"),
                // começa no meio do GOP: 30 pacotes sem chave (150..179), depois de 60 em 60
                Fonte::TsMeioDoGop => assert_eq!(k, [30, 90, 150, 210, 270], "{f:?}"),
                _ => {
                    let n = quadros_esperados(f).unwrap().len();
                    assert_eq!(k, (0..n).step_by(d.gop as usize).collect::<Vec<_>>(), "{f:?} ({})", cap.versao);
                }
            }
            let tipos = ffprobe_csv(&cap, &v, &[], "frame=pict_type,interlaced_frame,top_field_first");
            let n_b = tipos.iter().filter(|t| t.starts_with('B')).count();
            if f == Fonte::Mp4QuadrosB {
                assert!(video["has_b_frames"].as_i64().unwrap() > 0);
                // I B B P…: dois em cada três
                assert!(n_b >= 280, "{f:?}: {n_b} quadros B");
            } else if f != Fonte::Mp4ListaDeEdicao {
                assert_eq!(n_b, 0, "{f:?}");
            }
            assert!(tipos.iter().all(|t| t.ends_with(if d.entrelacado { ",1,1" } else { ",0,0" })), "{f:?}: {:?}", &tipos[..3]);
            if f == Fonte::Mp4ListaDeEdicao {
                // a lista de edição esconde o que vem antes do corte: sem ela o decodificador entrega do quadro-chave (30) em diante
                let com = ffprobe_csv(&cap, &v, &[], "frame=pict_type").len();
                let sem = ffprobe_csv(&cap, &v, &["-ignore_editlist", "1"], "frame=pict_type").len();
                assert!(n_b > 150 && sem >= com + 15, "{f:?}: {com} com a lista, {sem} sem, {n_b} B");
            }
            if f == Fonte::Giro90 {
                let rot = video["side_data_list"].as_array().and_then(|l| l.iter().find_map(|x| x["rotation"].as_i64()));
                assert_eq!(rot.map(i64::abs), Some(90), "{video}");
            }

            let codigos = medir::quadros_de_origem(&cap, &v).unwrap();
            let pts = medir::pts(&cap, &v).unwrap();
            assert_eq!(codigos.len(), pts.len(), "{f:?}");
            match quadros_esperados(f) {
                Some(e) => assert_eq!(codigos, e, "{f:?} ({})", cap.versao),
                None => {
                    // depois do que não se lê no começo, tudo identificado e em ordem
                    let bons: Vec<u32> = codigos.iter().copied().skip_while(|&c| c == medir::NAO_IDENTIFICADO).collect();
                    assert!(bons.len() >= 200, "{f:?}: {bons:?}");
                    assert!(bons.windows(2).all(|w| w[1] > w[0] && w[1] != medir::NAO_IDENTIFICADO), "{f:?}: {bons:?}");
                    if f == Fonte::TsMeioDoGop {
                        assert!(bons.windows(2).all(|w| w[1] == w[0] + 1), "{f:?}: {bons:?}");
                        assert_eq!(*bons.last().unwrap(), d.n_quadros - 1);
                    } else {
                        assert_eq!(codigos[0], (CORTE_SS * 30.0) as u32, "{f:?}");
                        assert!(bons.len() >= (CORTE_T * 30.0) as usize - 2, "{f:?}");
                    }
                }
            }
            match f {
                Fonte::Mp4 => assert_eq!(pts, (0..450).map(|k| k * 512).collect::<Vec<i64>>()),
                // os descartados deixam buraco: pts do quadro n = n·512
                Fonte::TaxaVariavel => assert_eq!(pts, codigos.iter().map(|&n| n as i64 * 512).collect::<Vec<i64>>()),
                Fonte::Oscilacao => {
                    let base = medir::pts(&cap, &gerar(Fonte::Mkv30).unwrap()).unwrap();
                    let desvio: Vec<i64> = pts.iter().zip(&base).map(|(a, b)| a - b).collect();
                    assert_eq!(desvio.len(), 450);
                    assert_eq!((*desvio.iter().min().unwrap(), *desvio.iter().max().unwrap()), (-4, 4), "{f:?}");
                    assert!(desvio.iter().filter(|d| **d != 0).count() > 300, "{f:?}");
                }
                Fonte::TsMeioDoGop => assert_eq!(codigos[0], 180, "{f:?} ({})", cap.versao),
                Fonte::Ts5994 | Fonte::MpegPs => assert!(pts[0] > 0, "{f:?}"),
                _ => {}
            }
            let n = codigos.len() as u32;
            for k in [0, n / 2, n - 1] {
                let q = medir::quadro(&cap, &v, k).unwrap();
                assert_eq!((q.w, q.h), (d.largura, d.altura), "{f:?}");
                let jq = codigos[k as usize];
                if jq == medir::NAO_IDENTIFICADO {
                    continue;
                }
                let e = marca_esperada(f, jq);
                let caixa = medir::marca(&q, [255, 0, 0], 60).unwrap_or_else(|| panic!("{f:?} quadro {k}: sem marca"));
                let esperado = [e[0] * q.w as f64, e[1] * q.h as f64, e[2] * q.w as f64, e[3] * q.h as f64];
                // entrelaçado: o quadro inteiro traz os dois campos, meio quadro (~4 px) um do outro
                let tol = if d.entrelacado { 8.0 } else { 2.0 };
                for i in 0..4 {
                    assert!(
                        (caixa[i] as f64 - esperado[i]).abs() <= tol,
                        "{f:?} quadro {k} (n.º {jq}): marca {caixa:?}, esperada {esperado:?} ({})",
                        cap.versao
                    );
                }
                if d.entrelacado {
                    // cada campo é uma imagem coerente (luma e croma) do seu instante
                    let (cima, baixo) = medir::campos(&cap, &v, k).unwrap();
                    assert_eq!((cima.w, cima.h), (d.largura, d.altura / 2));
                    let mut xs = Vec::new();
                    for (q, de_baixo) in [(&cima, false), (&baixo, true)] {
                        let e = marca_esperada_no_campo(f, jq, de_baixo);
                        let esperado = [e[0] * q.w as f64, e[1] * q.h as f64, e[2] * q.w as f64, e[3] * q.h as f64];
                        let caixa = medir::marca(q, [255, 0, 0], 60).unwrap();
                        for i in 0..4 {
                            assert!((caixa[i] as f64 - esperado[i]).abs() <= 2.0, "{f:?} quadro {k} campo de baixo {de_baixo}: {caixa:?}, esperada {esperado:?}");
                        }
                        xs.push(esperado[0]);
                    }
                    assert!((xs[0] - xs[1]).abs() >= 2.0, "{f:?}: campos no mesmo lugar {xs:?}");
                }
            }
            eprintln!("{f:?}: {} quadros, {} a {} ({})", codigos.len(), codigos[0], codigos[codigos.len() - 1], cap.versao);
        }
    }

    /// Várias threads e um segundo processo pedem a mesma fonte numa pasta nova: um gera, todos recebem o mesmo arquivo íntegro.
    #[test]
    fn geracao_concorrente_nao_corrompe() {
        let Some(cap) = cap() else { return };
        let pasta = pasta_temporaria("concorrente");
        let outro = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "edicao::testes::fontes::testes::auxiliar_gerar", "--ignored", "--nocapture"])
            .env("SICRO_TESTE_GERAR", &pasta)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let caminhos: Vec<PathBuf> = std::thread::scope(|s| {
            let h: Vec<_> = (0..6).map(|_| s.spawn(|| gerar_em(&pasta, &cap, Fonte::Mp4QuadrosB).unwrap())).collect();
            h.into_iter().map(|x| x.join().unwrap()).collect()
        });
        assert!(outro.wait_with_output().unwrap().status.success());
        assert!(caminhos.windows(2).all(|w| w[0] == w[1]));
        let codigos = medir::quadros_de_origem(&cap, &caminhos[0]).unwrap();
        assert_eq!(codigos, (0..450).collect::<Vec<u32>>());
        // nenhuma sobra de pasta de trabalho
        let sobras: Vec<_> = std::fs::read_dir(&pasta)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("tmp-"))
            .collect();
        assert!(sobras.is_empty(), "{sobras:?}");
    }

    #[test]
    #[ignore]
    fn auxiliar_gerar() {
        let Ok(pasta) = std::env::var("SICRO_TESTE_GERAR") else { return };
        let cap = cap().unwrap();
        let p = PathBuf::from(pasta);
        std::thread::scope(|s| {
            for _ in 0..3 {
                s.spawn(|| gerar_em(&p, &cap, Fonte::Mp4QuadrosB).unwrap());
            }
        });
    }

    #[test]
    fn amostras_de_gravador_quando_presentes() {
        let v = amostras_de_gravador();
        eprintln!("amostras de gravador: {}", v.len());
        assert!(v.iter().all(|p| p.is_file()));
        if dossie().is_some_and(|d| d.join("p1b/dl/dvr/amostras/dahua.dav").exists()) {
            assert!(v.iter().any(|p| p.ends_with("dahua.dav")));
            assert!(!v.iter().any(|p| p.extension().is_some_and(|e| e == "txt")));
        }
    }
}
