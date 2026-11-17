//! Folha de contato de um rastreio (§6.3 `edicao_rastreio_tira`, §6.4): recortes do alvo em `n` instantes, sempre
//! os trechos fracos (interpolados, estendidos, incertos) e os de menor confiança primeiro, o resto espalhado; o
//! perito vê cada sugestão antes de "Confirmar". JPEG no cache do caso (`tiras/<hash>.jpg`), nomeado pelo pedido.
//! Quadros pelo `LeitorPipe` (decodificação em CPU, busca do §5.3). Dono: P4.5.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use super::corrida::LeitorPipe;
use super::{arquivo, ArquivoDenso, LeitorQuadros, Sentido, ESTADO_AUSENTE, ESTADO_INCERTO, TIPO_MEDIDO};
use crate::edicao::executar::{self, sha256_bytes};
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::TamanhoPx;
use crate::edicao::quadros::{OrigemTabela, TabelaQuadros};
use crate::edicao::{armazem, ffmpeg::sonda};
use crate::error::{Result, SicroError};

/// Teto de recortes por folha, lado da célula (px) e colunas.
pub const MAX_CELULAS: usize = 48;
pub const LADO_CELULA: u32 = 160;
pub const COLUNAS: usize = 6;
/// Faixa lida de uma vez (quadros); acima disso, um leitor por instante (cada um busca o seu quadro-chave).
const FAIXA_SEGUIDA: usize = 3000;
/// Versão do desenho (entra no nome do arquivo).
const VERSAO: u32 = 1;

/// `relativo` ao cache do caso (`ProjetoAberto.raiz_cache`); `tempos` = tempo de fonte (s) de cada célula, na ordem
/// das células; `colunas` da grade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tira {
    pub relativo: String,
    pub colunas: u32,
    pub tempos: Vec<f64>,
}

fn invalido(d: impl std::fmt::Display) -> SicroError {
    SicroError::Validation(format!("{}: {d}", codigo::ITEM_INVALIDO))
}

fn sha_valido(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Linhas do denso que entram na folha (índices, em ordem de pts): primeira e última; um representante (o de menor
/// confiança) de cada trecho fraco seguido, dos mais fracos para os menos; depois os de menor confiança; o resto
/// espalhado no tempo.
pub fn escolher(d: &ArquivoDenso, n: usize) -> Vec<usize> {
    let validas: Vec<usize> = (0..d.pts.len()).filter(|&i| d.estado[i] != ESTADO_AUSENTE).collect();
    let k = n.min(MAX_CELULAS).min(validas.len());
    if k == 0 {
        return Vec::new();
    }
    let mut fica: Vec<usize> = Vec::with_capacity(k);
    let por = |i: usize, fica: &mut Vec<usize>| {
        if fica.len() < k && !fica.contains(&i) {
            fica.push(i);
        }
    };
    por(validas[0], &mut fica);
    por(*validas.last().unwrap_or(&validas[0]), &mut fica);
    let fraca = |i: usize| d.tipo[i] != TIPO_MEDIDO || d.estado[i] == ESTADO_INCERTO;
    let mut trechos: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < validas.len() {
        if !fraca(validas[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        let mut melhor = validas[i];
        while j < validas.len() && fraca(validas[j]) && (j == i || validas[j] == validas[j - 1] + 1) {
            if d.conf[validas[j]] < d.conf[melhor] {
                melhor = validas[j];
            }
            j += 1;
        }
        trechos.push(melhor);
        i = j;
    }
    trechos.sort_by(|a, b| d.conf[*a].total_cmp(&d.conf[*b]).then(a.cmp(b)));
    for t in trechos {
        por(t, &mut fica);
    }
    // metade do que sobra: as de menor confiança
    let resto = (k - fica.len()) / 2;
    let mut fracos: Vec<usize> = validas.iter().copied().filter(|i| !fica.contains(i)).collect();
    fracos.sort_by(|a, b| d.conf[*a].total_cmp(&d.conf[*b]).then(a.cmp(b)));
    for &i in fracos.iter().take(resto) {
        por(i, &mut fica);
    }
    // o resto espalhado
    let falta = k - fica.len();
    for s in 0..falta {
        let alvo = (s + 1) * validas.len() / (falta + 1);
        if let Some(&i) = validas.iter().skip(alvo).find(|i| !fica.contains(i)).or_else(|| validas.iter().find(|i| !fica.contains(i))) {
            por(i, &mut fica);
        }
    }
    fica.sort_by_key(|&i| d.pts[i]);
    fica
}

/// Acha o denso pelo SHA-256 entre os rastreios do projeto (o conteúdo é endereçado: qualquer cópia serve).
fn achar_denso(pasta: &Path, sha: &str) -> Result<ArquivoDenso> {
    let raiz = pasta.join("rastreios");
    let mut ids: Vec<String> = std::fs::read_dir(&raiz)
        .map(|r| r.flatten().filter_map(|e| e.file_name().to_str().map(str::to_string)).filter(|n| armazem::id_valido(n)).collect())
        .unwrap_or_default();
    ids.sort();
    for id in ids {
        if raiz.join(&id).join(format!("{sha}.json")).is_file() {
            return arquivo::ler(pasta, &id, sha);
        }
    }
    Err(SicroError::Validation(format!("{}: {sha}", codigo::RASTREIO_AUSENTE)))
}

/// A folha de contato (§6.3): denso `denso_sha256` da mídia `midia_sha256` do projeto, `n` células.
pub fn folha(ws: &Path, cache: &Path, projeto_id: &str, midia_sha256: &str, denso_sha256: &str, n: u32) -> Result<Tira> {
    if !armazem::id_valido(projeto_id) || !sha_valido(midia_sha256) || !sha_valido(denso_sha256) || n == 0 {
        return Err(invalido("pedido da folha"));
    }
    let d = achar_denso(&armazem::pasta_projeto(ws, projeto_id), denso_sha256)?;
    if d.midia_sha256 != midia_sha256 {
        return Err(invalido("mídia do denso"));
    }
    let linhas = escolher(&d, n as usize);
    if linhas.is_empty() {
        return Err(SicroError::Validation(format!("{}: sem caixa", codigo::RASTREIO_AUSENTE)));
    }
    let nome = format!("{}.jpg", &sha256_bytes(format!("{denso_sha256}|{n}|{VERSAO}").as_bytes())[..32]);
    let relativo = format!("tiras/{nome}");
    let destino = cache.join("tiras").join(&nome);
    let (video, tab) = video_e_tabela(ws, cache, midia_sha256)?;
    let tempos: Vec<f64> = linhas.iter().map(|&i| d.pts[i] as f64 * tab.tb.num as f64 / tab.tb.den as f64).collect();
    let colunas = COLUNAS.min(linhas.len());
    if !destino.is_file() {
        let img = desenhar(&video, &tab, midia_sha256, &d, &linhas, colunas)?;
        std::fs::create_dir_all(destino.parent().unwrap_or(cache))?;
        let mut jpg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 85)
            .encode(&img.rgb, img.w, img.h, image::ExtendedColorType::Rgb8)
            .map_err(|e| SicroError::Validation(format!("{}: jpeg {e}", codigo::ITEM_INVALIDO)))?;
        armazem::gravar_atomico(&destino, &jpg)?;
    }
    Ok(Tira { relativo, colunas: colunas as u32, tempos })
}

/// O vídeo do caso com esse SHA-256 e a tabela do decodificador dele (do cache; feita agora se faltar).
fn video_e_tabela(ws: &Path, cache: &Path, sha: &str) -> Result<(PathBuf, std::sync::Arc<TabelaQuadros>)> {
    let (conn, occ) = executar::abrir_banco(ws)?;
    let m = crate::database::repositories::video_repo::find_media_by_sha256(&conn, &occ, sha)?
        .ok_or_else(|| SicroError::Validation(format!("{}: {sha}", codigo::MIDIA_AUSENTE)))?;
    let video = crate::workspace::paths::resolver_relativo(ws, &m.relative_path);
    if !video.is_file() {
        return Err(SicroError::Validation(format!("{}: {sha}", codigo::MIDIA_AUSENTE)));
    }
    let cap = sonda::capacidades()?;
    let mut tab = TabelaQuadros::obter_com(&cap, cache, sha, &video)?;
    if tab.origem != OrigemTabela::Decodificador {
        tab = TabelaQuadros::do_decodificador(&cap, &video, &AtomicBool::new(false), &mut |_| {})?.guardar(cache, sha)?;
    }
    Ok((video, tab))
}

struct Folha {
    rgb: Vec<u8>,
    w: u32,
    h: u32,
}

/// Cor da moldura do alvo na célula: amarela na detecção, laranja no quadro interpolado ou estendido, vermelha no
/// incerto (estado por cor, sem texto).
fn cor_da_linha(d: &ArquivoDenso, i: usize) -> [u8; 3] {
    if d.estado[i] == ESTADO_INCERTO {
        [235, 64, 52]
    } else if d.tipo[i] != TIPO_MEDIDO {
        [255, 152, 0]
    } else {
        [255, 216, 74]
    }
}

fn desenhar(video: &Path, tab: &std::sync::Arc<TabelaQuadros>, sha: &str, d: &ArquivoDenso, linhas: &[usize], colunas: usize) -> Result<Folha> {
    let cap = sonda::capacidades()?;
    let fonte = executar::fonte_info_memo(&cap, video, sha, tab.clone(), None)?;
    let pixel = TamanhoPx { w: fonte.exibido.0, h: fonte.exibido.1 };
    let (lw, lh) = (pixel.w as f64, pixel.h as f64);
    let fileiras = linhas.len().div_ceil(colunas);
    let (fw, fh) = (colunas as u32 * LADO_CELULA, fileiras as u32 * LADO_CELULA);
    let mut folha = Folha { rgb: vec![40u8; (fw * fh * 3) as usize], w: fw, h: fh };
    let mut celula = |k: usize, q: &super::Imagem| {
        let i = linhas[k];
        // o alvo com metade do lado de contexto em volta, no mínimo 32 px
        let (bx, by, bw, bh) = (d.x[i] * lw, d.y[i] * lh, d.w[i] * lw, d.h[i] * lh);
        let lado = (bw.max(bh) * 2.0).max(32.0);
        let (cx, cy) = (bx + bw / 2.0, by + bh / 2.0);
        let x0 = (cx - lado / 2.0).clamp(0.0, (lw - 1.0).max(0.0));
        let y0 = (cy - lado / 2.0).clamp(0.0, (lh - 1.0).max(0.0));
        let x1 = (cx + lado / 2.0).clamp(x0 + 1.0, lw);
        let y1 = (cy + lado / 2.0).clamp(y0 + 1.0, lh);
        let esc = (LADO_CELULA as f64 / (x1 - x0)).min(LADO_CELULA as f64 / (y1 - y0));
        let (cw, ch) = (((x1 - x0) * esc).round().max(1.0) as u32, ((y1 - y0) * esc).round().max(1.0) as u32);
        let (ox, oy) = ((k % colunas) as u32 * LADO_CELULA + (LADO_CELULA - cw.min(LADO_CELULA)) / 2, (k / colunas) as u32 * LADO_CELULA + (LADO_CELULA - ch.min(LADO_CELULA)) / 2);
        for y in 0..ch.min(LADO_CELULA) {
            let sy = ((y0 + (y as f64 + 0.5) / esc) as u32).min(q.h - 1);
            for x in 0..cw.min(LADO_CELULA) {
                let sx = ((x0 + (x as f64 + 0.5) / esc) as u32).min(q.w - 1);
                let s = ((sy * q.w + sx) * 3) as usize;
                let o = (((oy + y) * fw + ox + x) * 3) as usize;
                folha.rgb[o..o + 3].copy_from_slice(&q.rgb[s..s + 3]);
            }
        }
        // moldura do alvo (1 px), na posição dele dentro da célula
        let cor = cor_da_linha(d, i);
        let px = |v: f64, o: u32, m: u32| (o as f64 + v * esc).round().clamp(o as f64, (o + m - 1) as f64) as u32;
        let (ax0, ay0) = (px(bx - x0, ox, cw.min(LADO_CELULA)), px(by - y0, oy, ch.min(LADO_CELULA)));
        let (ax1, ay1) = (px(bx + bw - x0, ox, cw.min(LADO_CELULA)), px(by + bh - y0, oy, ch.min(LADO_CELULA)));
        for x in ax0..=ax1 {
            for y in [ay0, ay1] {
                let o = ((y * fw + x) * 3) as usize;
                folha.rgb[o..o + 3].copy_from_slice(&cor);
            }
        }
        for y in ay0..=ay1 {
            for x in [ax0, ax1] {
                let o = ((y * fw + x) * 3) as usize;
                folha.rgb[o..o + 3].copy_from_slice(&cor);
            }
        }
    };
    let pts: Vec<i64> = linhas.iter().map(|&i| d.pts[i]).collect();
    let (j0, j1) = (tab.indice_do_pts(pts[0]).unwrap_or(0), tab.indice_do_pts(pts[pts.len() - 1]).unwrap_or(0));
    let mut feitos = vec![false; linhas.len()];
    if j1.saturating_sub(j0) <= FAIXA_SEGUIDA {
        let mut l = LeitorPipe::abrir(&cap, &fonte, pts[0], Some(pts[pts.len() - 1]), Sentido::Frente, pixel)?;
        while let Some((p, q)) = l.proximo()? {
            for (k, &alvo) in pts.iter().enumerate() {
                if alvo == p && !feitos[k] {
                    celula(k, &q);
                    feitos[k] = true;
                }
            }
        }
    } else {
        for (k, &p) in pts.iter().enumerate() {
            let mut l = LeitorPipe::abrir(&cap, &fonte, p, Some(p), Sentido::Frente, pixel)?;
            if let Some((lido, q)) = l.proximo()? {
                if lido == p {
                    celula(k, &q);
                    feitos[k] = true;
                }
            }
        }
    }
    if let Some(k) = feitos.iter().position(|f| !f) {
        return Err(SicroError::Validation(format!("{}: quadro {} da folha", executar::ERRO_LEITURA_DIVERGENTE, pts[k])));
    }
    Ok(folha)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::rastreio::{ESTADO_OK, TIPO_ESTENDIDO, TIPO_INTERPOLADO};

    fn denso(n: usize, f: impl Fn(usize) -> (f64, u8, u8)) -> ArquivoDenso {
        let mut d = ArquivoDenso {
            esquema: 2,
            rastreio: "r".into(),
            midia_sha256: "a".repeat(64),
            espaco: "original".into(),
            pixel: TamanhoPx { w: 100, h: 100 },
            pts: vec![],
            x: vec![],
            y: vec![],
            w: vec![],
            h: vec![],
            conf: vec![],
            estado: vec![],
            tipo: vec![],
            corrida: vec![],
            corridas: vec![],
        };
        for i in 0..n {
            let (c, e, t) = f(i);
            d.pts.push(i as i64 * 512);
            for col in [&mut d.x, &mut d.y, &mut d.w, &mut d.h] {
                col.push(0.25);
            }
            d.conf.push(c);
            d.estado.push(e);
            d.tipo.push(t);
            d.corrida.push(0);
        }
        d
    }

    /// Primeira e última sempre; cada trecho fraco (interpolado, estendido, incerto) tem a sua célula mesmo com
    /// confiança alta (a interpolada herda a dos vizinhos); depois as de menor confiança; quadro sem caixa nunca
    /// entra; em ordem de pts.
    #[test]
    fn escolhe_fracos_e_espalha() {
        let d = denso(300, |i| match i {
            0..=4 => (0.95, ESTADO_OK, TIPO_ESTENDIDO),
            40..=49 => (0.95 - (i as f64 - 40.0) * 0.001, ESTADO_OK, TIPO_INTERPOLADO),
            120 => (0.95, ESTADO_INCERTO, TIPO_MEDIDO),
            200..=209 => (0.0, ESTADO_AUSENTE, TIPO_MEDIDO),
            _ => (0.5 + (i % 7) as f64 * 0.01, ESTADO_OK, TIPO_MEDIDO),
        });
        let e = escolher(&d, 12);
        assert_eq!(e.len(), 12);
        assert!(e.windows(2).all(|w| d.pts[w[0]] < d.pts[w[1]]), "{e:?}");
        assert!(e.contains(&0) && e.contains(&299));
        assert!(e.contains(&49), "o mais fraco do trecho interpolado: {e:?}");
        // depois dos trechos fracos, as de menor confiança (0,5: i múltiplo de 7)
        assert!(e.iter().filter(|&&i| i % 7 == 0 && !(200..210).contains(&i) && i != 0).count() >= 3, "{e:?}");
        assert!(e.contains(&120), "o incerto: {e:?}");
        assert!(!e.iter().any(|i| (200..210).contains(i)), "quadro sem caixa: {e:?}");
        // espalhado: nenhum terço do trecho fica sem célula
        for terco in 0..3 {
            assert!(e.iter().any(|&i| i / 100 == terco), "{e:?}");
        }
        // pedido maior que as linhas válidas e acima do teto
        assert_eq!(escolher(&denso(5, |_| (0.9, ESTADO_OK, TIPO_MEDIDO)), 20), vec![0, 1, 2, 3, 4]);
        assert_eq!(escolher(&denso(500, |_| (0.9, ESTADO_OK, TIPO_MEDIDO)), 100).len(), MAX_CELULAS);
        assert!(escolher(&denso(5, |_| (0.0, ESTADO_AUSENTE, TIPO_MEDIDO)), 4).is_empty());
    }

    #[test]
    fn pedido_invalido_recusa_antes_de_ler() {
        let ws = std::env::temp_dir();
        for (p, m, s) in [("../x", "a".repeat(64), "b".repeat(64)), ("p", "A".repeat(64), "b".repeat(64)), ("p", "a".repeat(64), "../b".into())] {
            let e = folha(&ws, &ws, p, &m, &s, 6).unwrap_err().to_string();
            assert!(e.contains(codigo::ITEM_INVALIDO), "{e}");
        }
    }
}
