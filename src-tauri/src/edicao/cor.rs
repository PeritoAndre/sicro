//! Cor (§3.3): leitura de "#rrggbb[aa]", RGB ↔ YUV (BT.709 e BT.601, faixa limitada e cheia)
//! e as receitas de filtro. Dono: P0.3a.

use super::mensagens::codigo;
use super::modelo::{Faixa, Matriz};
use crate::error::{Result, SicroError};

/// sRGB, alfa reto, 8 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Lê "#rrggbb" (alfa ff) ou "#rrggbbaa". Erro `cor` fora do formato.
pub fn ler(cor: &str) -> Result<Rgba> {
    let erro = || SicroError::Validation(codigo::COR.into());
    let hex = cor.strip_prefix('#').ok_or_else(erro)?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(erro());
    }
    let par = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| erro());
    Ok(Rgba { r: par(0)?, g: par(2)?, b: par(4)?, a: if hex.len() == 8 { par(6)? } else { 255 } })
}

/// (Kr, Kb) da matriz.
fn coeficientes(m: Matriz) -> (f64, f64) {
    match m {
        Matriz::Bt709 => (0.2126, 0.0722),
        Matriz::Bt601 => (0.299, 0.114),
    }
}

fn oito_bits(v: f64, min: f64, max: f64) -> u8 {
    v.round().max(min).min(max) as u8
}

/// (Y, U, V) 8 bits. Limitada: Y 16..235, U e V 16..240; cheia: 0..255 com croma centrado em 128.
pub fn yuv(c: Rgba, matriz: Matriz, faixa: Faixa) -> (u8, u8, u8) {
    let (kr, kb) = coeficientes(matriz);
    let kg = 1.0 - kr - kb;
    let (r, g, b) = (c.r as f64 / 255.0, c.g as f64 / 255.0, c.b as f64 / 255.0);
    let y = kr * r + kg * g + kb * b;
    let pb = (b - y) / (2.0 * (1.0 - kb));
    let pr = (r - y) / (2.0 * (1.0 - kr));
    match faixa {
        Faixa::Tv => (oito_bits(16.0 + 219.0 * y, 16.0, 235.0), oito_bits(128.0 + 224.0 * pb, 16.0, 240.0), oito_bits(128.0 + 224.0 * pr, 16.0, 240.0)),
        Faixa::Pc => (oito_bits(255.0 * y, 0.0, 255.0), oito_bits(128.0 + 255.0 * pb, 0.0, 255.0), oito_bits(128.0 + 255.0 * pr, 0.0, 255.0)),
    }
}

/// RGB (alfa ff) de um (Y, U, V) 8 bits; o inverso de `yuv`, com erro de até 1/255.
pub fn rgb(yuv: (u8, u8, u8), matriz: Matriz, faixa: Faixa) -> Rgba {
    let (kr, kb) = coeficientes(matriz);
    let kg = 1.0 - kr - kb;
    let (y, u, v) = (yuv.0 as f64, yuv.1 as f64, yuv.2 as f64);
    let (y, pb, pr) = match faixa {
        Faixa::Tv => ((y - 16.0) / 219.0, (u - 128.0) / 224.0, (v - 128.0) / 224.0),
        Faixa::Pc => (y / 255.0, (u - 128.0) / 255.0, (v - 128.0) / 255.0),
    };
    let r = y + 2.0 * (1.0 - kr) * pr;
    let b = y + 2.0 * (1.0 - kb) * pb;
    let g = (y - kr * r - kb * b) / kg;
    let q = |x: f64| oito_bits(255.0 * x, 0.0, 255.0);
    Rgba { r: q(r), g: q(g), b: q(b), a: 255 }
}

/// (Y, U, V) em BT.709, faixa limitada, 8 bits.
pub fn yuv_bt709(c: Rgba) -> (u8, u8, u8) {
    yuv(c, Matriz::Bt709, Faixa::Tv)
}

/// Matriz presumida de fonte sem etiqueta: altura ≤ 576 → BT.601, senão BT.709.
pub fn matriz_presumida(altura: u32) -> Matriz {
    if altura <= 576 {
        Matriz::Bt601
    } else {
        Matriz::Bt709
    }
}

const ETIQUETA_709_TV: &str = "setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv";
const FLAGS: &str = "bicubic+accurate_rnd+bitexact";

fn nome_matriz(m: Matriz) -> &'static str {
    match m {
        Matriz::Bt709 => "bt709",
        Matriz::Bt601 => "bt601",
    }
}

/// Cadeia de filtros que normaliza a fonte para yuv420p BT.709 limitada (texto de grafo, sem vírgula nas pontas).
/// Faixa cheia passa antes por tabela (planos soltos + `lutyuv`), igual nas duas versões do ffmpeg (§3.3);
/// essa parte supõe croma 4:2:0 (`yuvj420p` ou `yuv420p` com etiqueta `pc`). Outro croma: `filtro_fonte_de`.
pub fn filtro_fonte(matriz: Matriz, faixa: Faixa) -> String {
    receita_fonte(matriz, faixa, "yuv420p")
}

/// `filtro_fonte` pelo formato de pixel da fonte (`FonteInfo.pix_fmt`). Na faixa cheia os planos são
/// remontados no croma da fonte: com `yuv420p` fixo o grafo de um MJPEG `yuvj422p` não configura.
/// None = faixa cheia fora do YUV planar de 8 bits (4:2:0, 4:2:2, 4:4:4): o motor converte antes ou recusa.
pub fn filtro_fonte_de(matriz: Matriz, faixa: Faixa, pix_fmt: &str) -> Option<String> {
    let planos = match pix_fmt {
        "yuv420p" | "yuvj420p" => "yuv420p",
        "yuv422p" | "yuvj422p" => "yuv422p",
        "yuv444p" | "yuvj444p" => "yuv444p",
        _ if faixa == Faixa::Tv => "yuv420p",
        _ => return None,
    };
    Some(receita_fonte(matriz, faixa, planos))
}

/// Entrada da fonte no grafo com o tamanho exibido explícito (`geometria::exibido`, o de `FonteInfo.exibido`):
/// `scale=iw*sar:ih` dá largura ímpar em PAL 12:11 e outro tamanho quando o contêiner gira pixel não quadrado,
/// e a máscara por quadro lido tem de ter o tamanho do quadro. É a que o motor usa.
/// None = formato de pixel fora do que o motor aceita (a validação acusa `pixel_nao_suportado`).
// CONTRATO: acréscimo do integrador depois da leva 2.
pub fn cadeia_fonte(matriz: Matriz, faixa: Faixa, pix_fmt: &str, exibido: (u32, u32)) -> Option<String> {
    let (w, h) = exibido;
    filtro_fonte_de(matriz, faixa, pix_fmt).map(|f| f.replacen("scale=iw*sar:ih:", &format!("scale={w}:{h}:"), 1))
}

fn receita_fonte(matriz: Matriz, faixa: Faixa, planos: &str) -> String {
    let escala = format!(
        "scale=iw*sar:ih:flags={FLAGS}:in_color_matrix={m}:in_range=tv:out_color_matrix=bt709:out_range=tv,setsar=1,format=yuv420p,{ETIQUETA_709_TV}",
        m = nome_matriz(matriz)
    );
    match faixa {
        Faixa::Tv => escala,
        Faixa::Pc => {
            let croma = "'clip(round(128+(val-128)*224/255),16,240)'";
            format!(
                "extractplanes=y+u+v,mergeplanes=0x001020:{planos},lutyuv=y='clip(round(16+val*219/255),16,235)':u={croma}:v={croma},setparams=range=tv,{escala}"
            )
        }
    }
}

/// Cadeia de filtros de uma camada RGBA para yuva420p BT.709 limitada.
/// O `scale` explícito é o que faz a conversão sair por BT.709 (sem ele sai por BT.601).
pub fn filtro_camada() -> String {
    format!("scale=flags={FLAGS}:out_color_matrix=bt709:out_range=tv,format=yuva420p,{ETIQUETA_709_TV}")
}

/// `lutyuv=y=<Y>:u=<U>:v=<V>` da cor cheia (sólido do `ocultar`, fundo).
pub fn filtro_cor_cheia(c: Rgba) -> String {
    let (y, u, v) = yuv_bt709(c);
    format!("lutyuv=y={y}:u={u}:v={v}")
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn c(hex: &str) -> Rgba {
        ler(hex).unwrap()
    }
    fn erro(a: Rgba, b: Rgba) -> u8 {
        [a.r.abs_diff(b.r), a.g.abs_diff(b.g), a.b.abs_diff(b.b)].into_iter().max().unwrap()
    }

    #[test]
    fn ler_formatos() {
        assert_eq!(c("#ff0000"), Rgba { r: 255, g: 0, b: 0, a: 255 });
        assert_eq!(c("#0000008c"), Rgba { r: 0, g: 0, b: 0, a: 140 });
        assert_eq!(c("#C89678"), Rgba { r: 200, g: 150, b: 120, a: 255 });
        for ruim in ["ff0000", "#ff000", "#ff00000", "#gg0000", "#ff0000ff0", "", "#", "#ff 000", "#+f0000"] {
            match ler(ruim) {
                Err(SicroError::Validation(m)) => assert_eq!(m, "cor"),
                outro => panic!("{ruim:?}: {outro:?}"),
            }
        }
    }

    /// Valores à mão por BT.709; os de vermelho e verde limitados são os que o `pad` do ffmpeg dá (`cm/out2/15d`).
    const CASOS: [(&str, (u8, u8, u8), (u8, u8, u8)); 6] = [
        ("#ff0000", (63, 102, 240), (54, 99, 255)),
        ("#00ff00", (173, 42, 26), (182, 30, 12)),
        ("#0000ff", (32, 240, 118), (18, 255, 116)),
        ("#ffffff", (235, 128, 128), (255, 128, 128)),
        ("#000000", (16, 128, 128), (0, 128, 128)),
        ("#808080", (126, 128, 128), (128, 128, 128)),
    ];

    #[test]
    fn bt709_nos_dois_sentidos_nas_duas_faixas() {
        for (hex, tv, pc) in CASOS {
            let cor = c(hex);
            assert_eq!(yuv(cor, Matriz::Bt709, Faixa::Tv), tv, "{hex} limitada");
            assert_eq!(yuv_bt709(cor), tv);
            assert_eq!(yuv(cor, Matriz::Bt709, Faixa::Pc), pc, "{hex} cheia");
            assert!(erro(rgb(tv, Matriz::Bt709, Faixa::Tv), cor) <= 1, "{hex} limitada de volta: {:?}", rgb(tv, Matriz::Bt709, Faixa::Tv));
            assert!(erro(rgb(pc, Matriz::Bt709, Faixa::Pc), cor) <= 1, "{hex} cheia de volta: {:?}", rgb(pc, Matriz::Bt709, Faixa::Pc));
        }
    }

    #[test]
    fn ida_e_volta_em_toda_a_grade() {
        // cubo RGB de 17 em 17 nas duas matrizes e faixas
        for m in [Matriz::Bt709, Matriz::Bt601] {
            for f in [Faixa::Tv, Faixa::Pc] {
                let mut pior = 0;
                for r in (0..=255).step_by(17) {
                    for g in (0..=255).step_by(17) {
                        for b in (0..=255).step_by(17) {
                            let cor = Rgba { r, g, b, a: 255 };
                            pior = pior.max(erro(rgb(yuv(cor, m, f), m, f), cor));
                        }
                    }
                }
                // a quantização do croma em 8 bits perde até 2 em cor saturada
                assert!(pior <= 2, "{m:?} {f:?}: {pior}");
            }
        }
    }

    #[test]
    fn presumida_e_receitas() {
        assert_eq!(matriz_presumida(480), Matriz::Bt601);
        assert_eq!(matriz_presumida(576), Matriz::Bt601);
        assert_eq!(matriz_presumida(720), Matriz::Bt709);
        assert_eq!(
            filtro_fonte(Matriz::Bt601, Faixa::Tv),
            "scale=iw*sar:ih:flags=bicubic+accurate_rnd+bitexact:in_color_matrix=bt601:in_range=tv:out_color_matrix=bt709:out_range=tv,setsar=1,format=yuv420p,setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv"
        );
        assert!(filtro_fonte(Matriz::Bt709, Faixa::Pc).starts_with("extractplanes=y+u+v,mergeplanes=0x001020:yuv420p,lutyuv=y='clip(round(16+val*219/255),16,235)':u='clip(round(128+(val-128)*224/255),16,240)':v='clip(round(128+(val-128)*224/255),16,240)',setparams=range=tv,scale=iw*sar:ih:"));
        assert_eq!(filtro_camada(), "scale=flags=bicubic+accurate_rnd+bitexact:out_color_matrix=bt709:out_range=tv,format=yuva420p,setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv");
        assert_eq!(filtro_cor_cheia(c("#00ff00")), "lutyuv=y=173:u=42:v=26");
        for f in [filtro_fonte(Matriz::Bt709, Faixa::Tv), filtro_fonte(Matriz::Bt709, Faixa::Pc), filtro_camada(), filtro_cor_cheia(c("#123456"))] {
            assert!(!f.starts_with(',') && !f.ends_with(','));
        }
        // pelo formato de pixel: 4:2:0 igual à receita fixa; faixa limitada não depende do formato
        for fmt in ["yuv420p", "yuvj420p"] {
            assert_eq!(filtro_fonte_de(Matriz::Bt601, Faixa::Pc, fmt), Some(filtro_fonte(Matriz::Bt601, Faixa::Pc)));
        }
        for fmt in ["yuv420p", "yuv422p10le", "nv12", "gray"] {
            assert_eq!(filtro_fonte_de(Matriz::Bt601, Faixa::Tv, fmt), Some(filtro_fonte(Matriz::Bt601, Faixa::Tv)));
        }
        assert!(filtro_fonte_de(Matriz::Bt601, Faixa::Pc, "yuvj422p").unwrap().starts_with("extractplanes=y+u+v,mergeplanes=0x001020:yuv422p,lutyuv="));
        assert!(filtro_fonte_de(Matriz::Bt601, Faixa::Pc, "yuvj444p").unwrap().starts_with("extractplanes=y+u+v,mergeplanes=0x001020:yuv444p,lutyuv="));
        for fmt in ["nv12", "yuv420p10le", "gray", "yuvj440p", ""] {
            assert_eq!(filtro_fonte_de(Matriz::Bt709, Faixa::Pc, fmt), None, "{fmt}");
        }
    }

    // ---- com ffmpeg: SICRO_FFMPEG escolhe o binário; SICRO_EXIGE_FFMPEG=1 falha sem ele ----

    struct Ff {
        bin: PathBuf,
        versao: String,
    }

    fn ffmpeg() -> Option<Ff> {
        let exige = std::env::var("SICRO_EXIGE_FFMPEG").map(|v| v == "1").unwrap_or(false);
        let bin = std::env::var_os("SICRO_FFMPEG").map(PathBuf::from).or_else(|| which::which("ffmpeg").ok());
        let achado = bin.and_then(|b| {
            let out = Command::new(&b).arg("-version").output().ok()?;
            let primeira = String::from_utf8_lossy(&out.stdout).lines().next()?.to_string();
            (out.status.success() && primeira.starts_with("ffmpeg version")).then_some(Ff { bin: b, versao: primeira })
        });
        match achado {
            Some(f) => {
                eprintln!("{}", f.versao);
                Some(f)
            }
            None if exige => panic!("SICRO_EXIGE_FFMPEG=1 e ffmpeg não encontrado"),
            None => {
                eprintln!("sem ffmpeg: teste dispensado");
                None
            }
        }
    }

    /// Um quadro `w`×`h` cru de `fmt_in` pela cadeia `vf`, devolvido cru em `fmt_out`.
    fn passar(ff: &Ff, pasta: &Path, dados: &[u8], fmt_in: &str, w: u32, h: u32, vf: &str, fmt_out: &str) -> Vec<u8> {
        let ent = pasta.join("entrada.raw");
        std::fs::write(&ent, dados).unwrap();
        let out = Command::new(&ff.bin)
            .args(["-hide_banner", "-nostdin", "-v", "error", "-f", "rawvideo", "-pix_fmt", fmt_in, "-s", &format!("{w}x{h}"), "-i"])
            .arg(&ent)
            .args(["-vf", vf, "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", fmt_out, "-"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{}: {}", ff.versao, String::from_utf8_lossy(&out.stderr));
        out.stdout
    }

    fn plano_420(w: usize, h: usize, y: u8, u: u8, v: u8) -> Vec<u8> {
        let mut d = vec![y; w * h];
        d.extend(std::iter::repeat(u).take(w * h / 4));
        d.extend(std::iter::repeat(v).take(w * h / 4));
        d
    }

    /// Todos os pixels de cada plano de um yuv420p (e do alfa, em yuva420p) iguais a `esperado`.
    fn uniforme(d: &[u8], w: usize, h: usize, esperado: &[u8]) -> std::result::Result<(), String> {
        let n = w * h;
        let planos = [(0, n), (n, n / 4), (n + n / 4, n / 4), (n + n / 2, n)];
        if d.len() != n * 3 / 2 + if esperado.len() == 4 { n } else { 0 } {
            return Err(format!("tamanho {}", d.len()));
        }
        for (i, &e) in esperado.iter().enumerate() {
            let (de, k) = planos[i];
            if let Some(x) = d[de..de + k].iter().find(|&&x| x != e) {
                return Err(format!("plano {i}: {x} ≠ {e}"));
            }
        }
        Ok(())
    }

    #[test]
    fn ffmpeg_cor_cheia_chega_exata() {
        let Some(ff) = ffmpeg() else { return };
        let pasta = tempfile::tempdir().unwrap();
        let (w, h) = (64, 32);
        for hex in ["#ff0000", "#00ff00", "#0000ff", "#c89678", "#ffffff", "#000000"] {
            let (y, u, v) = yuv_bt709(c(hex));
            let saida = passar(&ff, pasta.path(), &plano_420(w, h, 90, 140, 100), "yuv420p", w as u32, h as u32, &filtro_cor_cheia(c(hex)), "yuv420p");
            uniforme(&saida, w, h, &[y, u, v]).unwrap_or_else(|e| panic!("{} {hex}: {e}", ff.versao));
        }
    }

    #[test]
    fn ffmpeg_camada_rgba_chega_por_bt709() {
        let Some(ff) = ffmpeg() else { return };
        let pasta = tempfile::tempdir().unwrap();
        let (w, h) = (64, 32);
        for hex in ["#ff0000", "#00ff00", "#0000ff", "#ffffff", "#000000", "#808080", "#c89678"] {
            let cor = c(hex);
            let dados: Vec<u8> = std::iter::repeat([cor.r, cor.g, cor.b, 255]).take(w * h).flatten().collect();
            let saida = passar(&ff, pasta.path(), &dados, "rgba", w as u32, h as u32, &filtro_camada(), "yuva420p");
            let n = w * h;
            let medido = (saida[n / 2 + w / 2], saida[n + n / 8], saida[n + n / 4 + n / 8]);
            assert_eq!(saida[n + n / 2 + n / 2], 255, "{hex}: alfa");
            let (y, u, v) = yuv_bt709(cor);
            let dif = [medido.0.abs_diff(y), medido.1.abs_diff(u), medido.2.abs_diff(v)].into_iter().max().unwrap();
            assert!(dif <= 1, "{} {hex}: {medido:?} × {:?}", ff.versao, (y, u, v));
            // aceite das camadas (P0.5): até 2/255 no RGB
            assert!(erro(rgb(medido, Matriz::Bt709, Faixa::Tv), cor) <= 2, "{} {hex}: {medido:?}", ff.versao);
        }
    }

    /// Faixa cheia por tabela: cada valor de entrada sai pela conta da tabela, nas duas versões
    /// (BT.709 → BT.709: a escala que vem depois não mexe nos pixels).
    #[test]
    fn ffmpeg_fonte_faixa_cheia_pela_tabela() {
        let Some(ff) = ffmpeg() else { return };
        let pasta = tempfile::tempdir().unwrap();
        // 256 colunas de Y; croma em 128 colunas (uma por par de colunas de Y), 2 linhas
        let (w, h) = (256usize, 2usize);
        let mut d: Vec<u8> = (0..h).flat_map(|_| (0..=255u8).collect::<Vec<_>>()).collect();
        let croma_u: Vec<u8> = (0..128).map(|i| (i * 2) as u8).collect();
        let croma_v: Vec<u8> = (0..128).map(|i| 255 - (i * 2) as u8).collect();
        d.extend(&croma_u);
        d.extend(&croma_v);
        let saida = passar(&ff, pasta.path(), &d, "yuvj420p", w as u32, h as u32, &filtro_fonte(Matriz::Bt709, Faixa::Pc), "yuv420p");
        assert_eq!(saida.len(), w * h * 3 / 2, "{}", ff.versao);
        let ly = |v: u8| ((16.0 + v as f64 * 219.0 / 255.0).round()).clamp(16.0, 235.0) as u8;
        let lc = |v: u8| ((128.0 + (v as f64 - 128.0) * 224.0 / 255.0).round()).clamp(16.0, 240.0) as u8;
        for i in 0..w * h {
            assert_eq!(saida[i], ly(d[i]), "{} Y[{i}]", ff.versao);
        }
        for i in 0..128 {
            assert_eq!(saida[w * h + i], lc(croma_u[i]), "{} U[{i}]", ff.versao);
            assert_eq!(saida[w * h + 128 + i], lc(croma_v[i]), "{} V[{i}]", ff.versao);
        }
    }

    /// Faixa cheia em 4:2:2 e 4:4:4 (MJPEG de câmera e de DVR): o grafo configura, o Y sai pela tabela em
    /// cada um dos 256 valores, croma uniforme sai exato, e um quadro sorteado dá os MESMOS bytes nos dois
    /// binários (o SHA-256 abaixo é um só: o teste roda com cada ffmpeg).
    #[test]
    fn ffmpeg_fonte_faixa_cheia_422_e_444() {
        use sha2::{Digest, Sha256};
        let Some(ff) = ffmpeg() else { return };
        let pasta = tempfile::tempdir().unwrap();
        let ly = |v: u8| ((16.0 + v as f64 * 219.0 / 255.0).round()).clamp(16.0, 235.0) as u8;
        let lc = |v: u8| ((128.0 + (v as f64 - 128.0) * 224.0 / 255.0).round()).clamp(16.0, 240.0) as u8;
        for (fmt, croma_w, croma_h, sha) in [("yuvj422p", 2usize, 1usize, SHA_422), ("yuvj444p", 1, 1, SHA_444)] {
            let vf = filtro_fonte_de(Matriz::Bt709, Faixa::Pc, fmt).unwrap();
            // rampa de Y, croma uniforme
            let (w, h) = (256usize, 4usize);
            let mut d: Vec<u8> = (0..h).flat_map(|_| (0..=255u8).collect::<Vec<_>>()).collect();
            let n_croma = (w / croma_w) * (h / croma_h);
            d.extend(std::iter::repeat(200u8).take(n_croma));
            d.extend(std::iter::repeat(50u8).take(n_croma));
            let saida = passar(&ff, pasta.path(), &d, fmt, w as u32, h as u32, &vf, "yuv420p");
            assert_eq!(saida.len(), w * h * 3 / 2, "{} {fmt}", ff.versao);
            for i in 0..w * h {
                assert_eq!(saida[i], ly(d[i]), "{} {fmt} Y[{i}]", ff.versao);
            }
            assert!(saida[w * h..w * h + w * h / 4].iter().all(|&x| x == lc(200)), "{} {fmt} U", ff.versao);
            assert!(saida[w * h + w * h / 4..].iter().all(|&x| x == lc(50)), "{} {fmt} V", ff.versao);
            // quadro sorteado 64×48, BT.601 → BT.709 e croma reduzido pelo `scale`: mesmos bytes nas duas versões
            let (w, h) = (64usize, 48usize);
            let mut s: u64 = 0x51c0_2026;
            let n = w * h + 2 * (w / croma_w) * (h / croma_h);
            let d: Vec<u8> = (0..n)
                .map(|_| {
                    s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    (s >> 56) as u8
                })
                .collect();
            let vf = filtro_fonte_de(Matriz::Bt601, Faixa::Pc, fmt).unwrap();
            let saida = passar(&ff, pasta.path(), &d, fmt, w as u32, h as u32, &vf, "yuv420p");
            assert_eq!(saida.len(), w * h * 3 / 2, "{} {fmt}", ff.versao);
            let hex: String = Sha256::digest(&saida).iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hex, sha, "{} {fmt}: quadro sorteado mudou", ff.versao);
        }
    }
    const SHA_422: &str = "8e6dd782d6216eaf1d9216a686469db50b3ad4ee4dbdb8fca67cc18431e83fe2";
    const SHA_444: &str = "00f191aea648d0df03fcf2a6dfb5ace24c0019de3bf1d907d96f4eeaa9d8f960";

    /// Fonte BT.601 (limitada e cheia) e BT.709 cheia → BT.709 limitada: a cor pedida chega a até 2/255
    /// pela conta daqui, e a proporção do pixel é aplicada (720×480 10:11 → 654×480).
    #[test]
    fn ffmpeg_fonte_para_709_e_sar() {
        let Some(ff) = ffmpeg() else { return };
        let pasta = tempfile::tempdir().unwrap();
        let (w, h) = (64usize, 32usize);
        for (m, f, fmt) in [(Matriz::Bt601, Faixa::Tv, "yuv420p"), (Matriz::Bt601, Faixa::Pc, "yuvj420p"), (Matriz::Bt709, Faixa::Pc, "yuvj420p"), (Matriz::Bt709, Faixa::Tv, "yuv420p")] {
            for hex in ["#ff0000", "#00ff00", "#0000ff", "#c89678", "#808080", "#ffffff", "#000000"] {
                let cor = c(hex);
                let (y, u, v) = yuv(cor, m, f);
                let saida = passar(&ff, pasta.path(), &plano_420(w, h, y, u, v), fmt, w as u32, h as u32, &filtro_fonte(m, f), "yuv420p");
                let n = w * h;
                let medido = (saida[n / 2], saida[n + n / 8], saida[n + n / 4 + n / 8]);
                assert!(erro(rgb(medido, Matriz::Bt709, Faixa::Tv), cor) <= 2, "{} {m:?} {f:?} {hex}: {medido:?}", ff.versao);
            }
        }
        let (w, h) = (720usize, 480usize);
        let vf = format!("setsar=10/11,{}", filtro_fonte(Matriz::Bt601, Faixa::Tv));
        let saida = passar(&ff, pasta.path(), &plano_420(w, h, 100, 128, 128), "yuv420p", w as u32, h as u32, &vf, "yuv420p");
        assert_eq!(saida.len(), 654 * 480 * 3 / 2, "{}", ff.versao);
        let vf = format!("setsar=10/11,{}", filtro_fonte(Matriz::Bt709, Faixa::Pc));
        let saida = passar(&ff, pasta.path(), &plano_420(w, h, 100, 128, 128), "yuvj420p", w as u32, h as u32, &vf, "yuv420p");
        assert_eq!(saida.len(), 654 * 480 * 3 / 2, "{}: faixa cheia perde a proporção do pixel", ff.versao);
    }

    #[test]
    fn cadeia_fonte_leva_o_tamanho_exibido() {
        let f = cadeia_fonte(Matriz::Bt601, Faixa::Tv, "yuv420p", (784, 576)).unwrap();
        assert!(f.starts_with("scale=784:576:flags="), "{f}");
        assert!(!f.contains("iw*sar"));
        let f = cadeia_fonte(Matriz::Bt709, Faixa::Pc, "yuvj422p", (480, 654)).unwrap();
        assert!(f.contains(":yuv422p,") && f.contains(",scale=480:654:flags=") && !f.contains("iw*sar"), "{f}");
        assert_eq!(cadeia_fonte(Matriz::Bt709, Faixa::Pc, "nv12", (2, 2)), None);
    }

    /// PAL 720×576 com pixel 12:11: o grafo sai no tamanho de `geometria::exibido` (784×576), não em 785×576.
    #[test]
    fn ffmpeg_cadeia_fonte_sai_no_tamanho_exibido() {
        let Some(ff) = ffmpeg() else { return };
        let pasta = tempfile::tempdir().unwrap();
        let (w, h) = (720usize, 576usize);
        let ex = crate::edicao::geometria::exibido(
            crate::edicao::modelo::TamanhoPx { w: 720, h: 576 },
            crate::edicao::modelo::Racional { num: 12, den: 11 },
            0,
        );
        assert_eq!((ex.w, ex.h), (784, 576));
        for (m, f, fmt) in [(Matriz::Bt601, Faixa::Tv, "yuv420p"), (Matriz::Bt601, Faixa::Pc, "yuvj420p")] {
            let vf = format!("setsar=12/11,{}", cadeia_fonte(m, f, fmt, (ex.w, ex.h)).unwrap());
            let saida = passar(&ff, pasta.path(), &plano_420(w, h, 100, 128, 128), fmt, w as u32, h as u32, &vf, "yuv420p");
            assert_eq!(saida.len(), 784 * 576 * 3 / 2, "{} {f:?}", ff.versao);
        }
    }
}
