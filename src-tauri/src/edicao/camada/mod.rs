//! Rasterizador do SICRO (§5.5): de um `PedidoCamada` para PNGs + lista. Fase 1: `mod.rs`, `formas.rs`, `lista.rs`,
//! `texto.rs` do integrador; `lupa.rs` do P1.2, `medidas.rs` do P1.3, `selos.rs` do P1.R, `titulo.rs` e `modelos/`
//! do P1.7, `mascara.rs` do P2.6.
//! Anotação: PNG RGBA 8 bits, alfa reto. Máscara: PNG cinza 8 bits (branco = aplica).
//!
//! Um PNG por quadro DIFERENTE (o nome é o do primeiro quadro que o usa); a lista tem uma entrada por slot ou por
//! quadro lido e repete o nome nos iguais. Um PNG só = camada fixa, sem lista. A pasta nasce com nome temporário e
//! é renomeada no fim: no cache só existe camada inteira.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tiny_skia::{FilterQuality, PixmapPaint};

use super::executar::ERRO_CANCELADO;
use super::modelo::TamanhoPx;
use super::motor::plano::{Desenho, ItemCamada, PedidoCamada, TipoCamada, TransformacaoAvaliada};
use crate::error::{Result, SicroError};
use formas::{Primitiva, Recursos};
use texto::SpritesEmCache;

pub mod formas;
pub mod lista;
pub mod lupa;
pub mod mascara;
pub mod medidas;
pub mod selos;
pub mod texto;
pub mod titulo;

#[cfg(test)]
mod testes;

/// Entra no `hash` de todo `PedidoCamada`: sobe quando o desenho muda um pixel.
/// 2: mono itálico com as oblíquas da DejaVu Sans Mono (antes saía reto).
pub const VERSAO_RASTERIZADOR: u32 = 2;

// CONTRATO: acréscimo.
/// Nome da lista na pasta da camada.
pub const LISTA: &str = "lista.ffconcat";
const PRONTA: &str = "camada.json";
/// Maior camada (px por quadro) e maior número de quadros: acima disso é erro de quem montou o pedido, e alocar
/// derrubaria o processo.
const MAX_PX: u64 = 1 << 26;
const MAX_QUADROS: u32 = 1 << 24;

/// Pixels crus: `canais` = 1 (cinza) ou 4 (RGBA, alfa reto); `px` por linhas, sem preenchimento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub w: u32,
    pub h: u32,
    pub canais: u8,
    pub px: Vec<u8>,
}

impl Bitmap {
    /// PNG determinístico (mesma entrada, mesmos bytes): sem metadados, compressão rápida, filtro `Sub` (o adaptativo
    /// custa 5× mais para 6 % a menos).
    pub fn png(&self) -> Result<Vec<u8>> {
        let tipo = match self.canais {
            1 => ExtendedColorType::L8,
            4 => ExtendedColorType::Rgba8,
            c => return Err(SicroError::Validation(format!("png: {c} canais"))),
        };
        if self.w == 0 || self.h == 0 || self.px.len() != self.w as usize * self.h as usize * self.canais as usize {
            return Err(SicroError::Validation("png: tamanho".into()));
        }
        let mut out = Vec::with_capacity(self.px.len() / 4 + 64);
        PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Sub)
            .write_image(&self.px, self.w, self.h, tipo)
            .map_err(|e| SicroError::Workspace(format!("png: {e}")))?;
        Ok(out)
    }
}

/// Um PNG da camada: nome simples na pasta da camada e SHA-256 (vai para `execucao.camadas`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArquivoCamada {
    pub arquivo: String,
    pub sha256: String,
}

/// Camada rasterizada em `camadas/<hash>/` (ou `mascaras/<hash>/`). `lista` = `.ffconcat` com uma entrada por
/// slot ou por quadro lido; `fixa` = um PNG só (entra com `-loop 1`), sem lista.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CamadaPronta {
    pub id: String,
    pub pasta: PathBuf,
    pub lista: Option<String>,
    pub fixa: bool,
    pub arquivos: Vec<ArquivoCamada>,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}

/// `hash` de um pedido: SHA-256 do pedido canônico (sem o próprio campo) + `VERSAO_RASTERIZADOR` + SHA-256 das fontes
/// e dos modelos de texto. Ficam de fora o `id` (rótulo do plano) e o caminho das imagens (vale o SHA-256 delas).
pub fn hash_do_pedido(pedido: &PedidoCamada) -> String {
    hash_com(pedido, VERSAO_RASTERIZADOR, texto::impressao_das_fontes(), texto::impressao_dos_modelos())
}

fn hash_com(pedido: &PedidoCamada, versao: u32, fontes: &str, modelos: &str) -> String {
    #[derive(Serialize, Debug)]
    struct Canonico<'a> {
        versao: u32,
        fontes: &'a str,
        modelos: &'a str,
        tipo: TipoCamada,
        caixa_px: [u32; 4],
        n: u32,
        itens: &'a [ItemCamada],
        imagens: Vec<(&'a str, &'a str, TamanhoPx)>,
    }
    let mut imagens: Vec<(&str, &str, TamanhoPx)> =
        pedido.imagens.iter().map(|i| (i.midia.as_str(), i.sha256.as_str(), i.tamanho)).collect();
    imagens.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    let c = Canonico { versao, fontes, modelos, tipo: pedido.tipo, caixa_px: pedido.caixa_px, n: pedido.n, itens: &pedido.itens, imagens };
    let mut h = Sha256::new();
    h.update(b"sicro.edicao.camada/1\n");
    // serializar estruturas do próprio modelo não falha; se falhar, o texto de depuração ainda separa os pedidos
    h.update(serde_json::to_vec(&c).unwrap_or_else(|_| format!("{c:?}").into_bytes()));
    hex(&h.finalize())
}

/// O que vai num quadro da camada, na ordem dos itens; dois quadros com a mesma pintura dividem o PNG.
#[derive(Serialize)]
enum Pintura {
    Primitivas { p: Vec<Primitiva>, canvas: TamanhoPx, transformacao: Option<TransformacaoAvaliada> },
    Imagem { midia: String, canvas: TamanhoPx, transformacao: TransformacaoAvaliada },
}

fn invalido(detalhe: &str) -> SicroError {
    SicroError::Validation(format!("{}: {detalhe}", super::mensagens::codigo::ITEM_INVALIDO))
}

fn cancelado() -> SicroError {
    SicroError::Validation(ERRO_CANCELADO.into())
}

fn pinturas(pedido: &PedidoCamada, i: u32, sprites: &SpritesEmCache) -> Result<Vec<Pintura>> {
    let mut v = Vec::new();
    for item in &pedido.itens {
        let Some(qi) = i.checked_sub(item.de).and_then(|k| item.quadros.get(k as usize)) else { continue };
        let q = &qi.quadro;
        match &item.desenho {
            Desenho::Efeito { efeito } => {
                let p = formas::primitivas(efeito, q, sprites)?;
                if !p.is_empty() {
                    v.push(Pintura::Primitivas { p, canvas: q.canvas, transformacao: qi.transformacao });
                }
            }
            Desenho::Titulo { titulo } => {
                let p = titulo::primitivas(titulo, q, sprites)?;
                if !p.is_empty() {
                    v.push(Pintura::Primitivas { p, canvas: q.canvas, transformacao: qi.transformacao });
                }
            }
            Desenho::Imagem { midia } => {
                let t = qi.transformacao.ok_or_else(|| invalido("imagem sem transformacao"))?;
                v.push(Pintura::Imagem { midia: midia.clone(), canvas: q.canvas, transformacao: t });
            }
        }
    }
    Ok(v)
}

fn desenhar_quadro(tipo: TipoCamada, caixa_px: [u32; 4], ps: &[Pintura], r: &Recursos) -> Result<Bitmap> {
    match tipo {
        TipoCamada::Anotacao => {
            let mut pm = formas::pixmap_vazio(caixa_px)?;
            for p in ps {
                match p {
                    Pintura::Primitivas { p, canvas, transformacao } => {
                        let (extra, op) = match transformacao {
                            Some(t) => (
                                Some(formas::transformacao(t.x, t.y, t.escala, t.rotacao, canvas.w as f64, canvas.h as f64, *canvas)),
                                t.opacidade,
                            ),
                            None => (None, 1.0),
                        };
                        formas::pintar(&mut pm, p, *canvas, caixa_px, r, extra, op)?;
                    }
                    Pintura::Imagem { midia, canvas, transformacao: t } => {
                        let src = format!("midia:{midia}");
                        let img = r.pixmap(&src)?;
                        let (iw, ih) = (img.width() as f64, img.height() as f64);
                        // muito reduzida: desenha a que já foi reduzida por blocos de k px
                        let (img, k) = r.pixmap_para(&src, if iw > 0.0 { t.escala * canvas.w as f64 / iw } else { 1.0 })?;
                        let ts = formas::transformacao(t.x, t.y, t.escala, t.rotacao, iw, ih, *canvas)
                            .pre_scale(k as f32, k as f32)
                            .post_translate(-(caixa_px[0] as f32), -(caixa_px[1] as f32));
                        let paint = PixmapPaint {
                            opacity: t.opacidade.clamp(0.0, 1.0) as f32,
                            quality: FilterQuality::Bilinear,
                            ..PixmapPaint::default()
                        };
                        pm.draw_pixmap(0, 0, tiny_skia::Pixmap::as_ref(&img), &paint, ts, None);
                    }
                }
            }
            Ok(formas::bitmap_reto(pm))
        }
        TipoCamada::MascaraCanvas | TipoCamada::MascaraFonte => {
            let (mut regioes, mut lupas) = (Vec::new(), Vec::new());
            let mut espaco = None;
            for p in ps {
                if let Pintura::Primitivas { p, canvas, .. } = p {
                    espaco.get_or_insert(*canvas);
                    regioes.extend(p.iter().filter(|x| matches!(x, Primitiva::Regiao { .. })).cloned());
                    lupas.extend(p.iter().filter(|x| matches!(x, Primitiva::Lupa { .. })).cloned());
                }
            }
            match espaco {
                // a forma da lupa para o `alphamerge` dela (P1.2): camada de máscara pedida por `motor/efeitos/lupa.rs`
                Some(e) if !lupas.is_empty() => lupa::mascara(&lupas, e, caixa_px),
                Some(e) => mascara::mascara(&regioes, e, caixa_px),
                None => Ok(Bitmap { w: caixa_px[2], h: caixa_px[3], canais: 1, px: vec![0; caixa_px[2] as usize * caixa_px[3] as usize] }),
            }
        }
    }
}

/// O que fica em `camada.json`: a camada e o SHA-256 da lista.
#[derive(Serialize, Deserialize)]
struct Selo {
    #[serde(flatten)]
    camada: CamadaPronta,
    lista_sha256: Option<String>,
}

fn sha256_do_arquivo(p: &Path) -> Option<String> {
    std::fs::read(p).ok().map(|b| hex(&Sha256::digest(&b)))
}

/// Camada do cache, só se estiver inteira: todo PNG e a lista com o SHA-256 gravado (arquivo de 0 byte, cortado
/// ou trocado não passa: o SHA-256 de cada camada vai para o relatório).
fn ler_pronta(pasta: &Path, id: &str) -> Option<CamadaPronta> {
    let Selo { camada: mut p, lista_sha256 } = serde_json::from_slice(&std::fs::read(pasta.join(PRONTA)).ok()?).ok()?;
    let nome_simples = |n: &str| !n.is_empty() && Path::new(n).file_name().is_some_and(|f| f == n);
    if p.arquivos.is_empty() || !p.arquivos.iter().all(|a| nome_simples(&a.arquivo)) || p.fixa != p.lista.is_none() {
        return None;
    }
    let lista_ok = match (&p.lista, &lista_sha256) {
        (None, None) => true,
        (Some(l), Some(s)) => nome_simples(l) && sha256_do_arquivo(&pasta.join(l)).as_ref() == Some(s),
        _ => false,
    };
    let completa = lista_ok && p.arquivos.par_iter().all(|a| sha256_do_arquivo(&pasta.join(&a.arquivo)).as_ref() == Some(&a.sha256));
    completa.then(|| {
        p.id = id.to_string();
        p.pasta = pasta.to_path_buf();
        p
    })
}

/// Põe a pasta pronta no lugar. Outro escritor chegou antes: vale a dele, se estiver inteira; sobra estragada no
/// destino sai e a troca é tentada de novo (no Windows o rename falha enquanto um antivírus segura um PNG novo).
fn publicar(tmp: &Path, pasta: &Path, id: &str) -> Result<Option<CamadaPronta>> {
    for tentativa in 0..4 {
        if std::fs::rename(tmp, pasta).is_ok() {
            return Ok(None);
        }
        if let Some(p) = ler_pronta(pasta, id) {
            let _ = std::fs::remove_dir_all(tmp);
            return Ok(Some(p));
        }
        let _ = std::fs::remove_dir_all(pasta);
        if tentativa > 0 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    let _ = std::fs::remove_dir_all(tmp);
    Err(SicroError::Filesystem(format!("camada: não deu para gravar {}", pasta.display())))
}

/// Rasteriza o pedido no cache (`cache` = pasta da Edição do caso); pronto no cache = só devolve.
/// Os quadros diferentes são desenhados em paralelo (`rayon`); o resultado não depende do número de threads.
pub fn rasterizar(pedido: &PedidoCamada, cache: &Path, cancelar: &AtomicBool) -> Result<CamadaPronta> {
    let hash = hash_do_pedido(pedido);
    let raiz = cache.join(match pedido.tipo {
        TipoCamada::Anotacao => "camadas",
        TipoCamada::MascaraCanvas | TipoCamada::MascaraFonte => "mascaras",
    });
    let pasta = raiz.join(&hash);
    if let Some(p) = ler_pronta(&pasta, &pedido.id) {
        return Ok(p);
    }
    let [_, _, w, h] = pedido.caixa_px;
    if w == 0 || h == 0 || pedido.n == 0 {
        return Err(invalido("caixa_px ou n vazio"));
    }
    if w as u64 * h as u64 > MAX_PX || pedido.n > MAX_QUADROS {
        return Err(invalido("camada grande demais"));
    }
    // posição ou lado ímpar desloca o croma 4:2:0 no `overlay`
    if pedido.caixa_px.iter().any(|v| v % 2 != 0) {
        return Err(invalido("caixa_px ímpar"));
    }
    if pedido.itens.iter().any(|it| it.de as u64 + it.quadros.len() as u64 > pedido.n as u64) {
        return Err(invalido("item além de n"));
    }
    let sprites = SpritesEmCache { cache, imagens: &pedido.imagens };
    // pintura de cada quadro; iguais dividem o PNG
    let mut unicos: Vec<(u32, Vec<Pintura>)> = Vec::new();
    let mut indice: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut por_quadro: Vec<usize> = Vec::with_capacity(pedido.n as usize);
    for i in 0..pedido.n {
        if cancelar.load(Ordering::Relaxed) {
            return Err(cancelado());
        }
        let ps = pinturas(pedido, i, &sprites)?;
        let chave = serde_json::to_vec(&ps)?;
        let k = *indice.entry(chave).or_insert_with(|| {
            unicos.push((i, ps));
            unicos.len() - 1
        });
        por_quadro.push(k);
    }
    static N: AtomicU64 = AtomicU64::new(0);
    let tmp = raiz.join(format!("{hash}.tmp-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let feito = (|| -> Result<CamadaPronta> {
        let recursos = Recursos::novo(cache, &pedido.imagens);
        let arquivos = unicos
            .par_iter()
            .map(|(i0, ps)| {
                if cancelar.load(Ordering::Relaxed) {
                    return Err(cancelado());
                }
                let png = desenhar_quadro(pedido.tipo, pedido.caixa_px, ps, &recursos)?.png()?;
                let arquivo = format!("{i0:06}.png");
                std::fs::write(tmp.join(&arquivo), &png)?;
                Ok(ArquivoCamada { arquivo, sha256: hex(&Sha256::digest(&png)) })
            })
            .collect::<Result<Vec<_>>>()?;
        let fixa = arquivos.len() == 1;
        let (lista, lista_sha256) = if fixa {
            (None, None)
        } else {
            let nomes: Vec<String> = por_quadro.iter().map(|&k| arquivos[k].arquivo.clone()).collect();
            let texto = lista::ffconcat(&nomes);
            std::fs::write(tmp.join(LISTA), &texto)?;
            (Some(LISTA.to_string()), Some(hex(&Sha256::digest(texto.as_bytes()))))
        };
        let selo = Selo { camada: CamadaPronta { id: String::new(), pasta: PathBuf::new(), lista, fixa, arquivos }, lista_sha256 };
        std::fs::write(tmp.join(PRONTA), serde_json::to_vec_pretty(&selo)?)?;
        Ok(selo.camada)
    })();
    let mut p = match feito {
        Ok(p) => p,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
    };
    if let Some(dele) = publicar(&tmp, &pasta, &pedido.id)? {
        return Ok(dele);
    }
    p.id = pedido.id.clone();
    p.pasta = pasta;
    Ok(p)
}

/// Rasteriza vários pedidos em paralelo; devolve na ordem dos pedidos. `feito` recebe quantos acabaram.
pub fn rasterizar_todos(
    pedidos: &[PedidoCamada],
    cache: &Path,
    cancelar: &AtomicBool,
    feito: &mut dyn FnMut(usize),
) -> Result<Vec<CamadaPronta>> {
    let (tx, rx) = mpsc::channel::<()>();
    let resultado = std::thread::scope(|s| {
        let h = s.spawn(move || {
            pedidos
                .par_iter()
                .map(|p| {
                    let r = rasterizar(p, cache, cancelar);
                    let _ = tx.send(());
                    r
                })
                .collect::<Vec<_>>()
        });
        let mut n = 0;
        for () in rx {
            n += 1;
            feito(n);
        }
        h.join()
    });
    match resultado {
        Ok(v) => v.into_iter().collect(),
        Err(p) => std::panic::resume_unwind(p),
    }
}
