//! Texto pelo Typst (§5.5): sprites em `sprites/<hash>.png` e atlas do carimbo em `atlas/<hash>.png`.
//! Conteúdo, estilo e largura chegam como dado (`json("dados.json")`), nunca como código: o modelo é fixo.
//! Caminho do A16: Typst → SVG (`typst-svg`, glifos em curvas) → `resvg` → RGBA de alfa reto; 1 pt = 1 px.
//! Fontes embutidas: Inter (sans), Libertinus Serif (serifa), DejaVu Sans Mono (mono). Dono: P0.5.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

use super::formas::bitmap_reto;
use super::{Bitmap, VERSAO_RASTERIZADOR};
use crate::edicao::cor;
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{BlocoTexto, EstiloTexto, TamanhoPx};
use crate::edicao::motor::plano::ImagemCamada;
use crate::error::{Result, SicroError};

/// Pedido de sprite (espelho do TS): `altura_canvas` em px; `fundo` = desenhar a caixa de fundo dos blocos.
// CONTRATO: `largura_max` passa a ser fração da ALTURA do canvas (era "da largura"): o sprite só conhece
// `altura_canvas`, e assim depende só dela. Quem monta converte: `bloco.largura_max × largura / altura`.
/// `largura_max` em fração da altura do canvas. Os blocos ficam um sob o outro, cada um no seu alinhamento;
/// `lugar`, `largura_max` e `id` dos blocos não entram no sprite (posicionam e identificam).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PedidoSprite {
    pub blocos: Vec<BlocoTexto>,
    pub largura_max: f64,
    pub altura_canvas: u32,
    pub fundo: bool,
}

/// Sprite pronto: caminho relativo ao cache da Edição do caso e tamanho em px.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sprite {
    pub relativo: String,
    pub w: u32,
    pub h: u32,
}

/// Atlas do carimbo: `glifos` na ordem das células (cada uma `celula` px), em linha.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Atlas {
    pub relativo: String,
    pub celula: TamanhoPx,
    pub glifos: String,
}

/// De onde `formas::primitivas` tira o que não cabe no efeito: sprites, atlas e tamanho das imagens
/// (o cache real, ou um falso nos testes). No front são os três últimos métodos de `QuadroPalco`.
pub trait FonteSprites {
    /// Sprite do pedido.
    fn sprite(&self, pedido: &PedidoSprite) -> Result<Sprite>;
    /// Atlas do carimbo nesse estilo e altura de canvas (px).
    fn atlas(&self, estilo: &EstiloTexto, altura_canvas: u32) -> Result<Atlas>;
    /// Tamanho (px) da imagem com esse `MidiaRef.id`; None = não é imagem conhecida.
    fn imagem(&self, midia: &str) -> Option<TamanhoPx>;
}

/// `FonteSprites` sobre o cache em disco (`cache` = pasta da Edição do caso) e as imagens do pedido.
pub struct SpritesEmCache<'a> {
    pub cache: &'a Path,
    pub imagens: &'a [ImagemCamada],
}

impl FonteSprites for SpritesEmCache<'_> {
    fn sprite(&self, pedido: &PedidoSprite) -> Result<Sprite> {
        sprite(self.cache, pedido)
    }
    fn atlas(&self, estilo: &EstiloTexto, altura_canvas: u32) -> Result<Atlas> {
        atlas_carimbo(self.cache, estilo, altura_canvas)
    }
    fn imagem(&self, midia: &str) -> Option<TamanhoPx> {
        self.imagens.iter().find(|i| i.midia == midia).map(|i| i.tamanho)
    }
}

// CONTRATO: acréscimo.
/// Glifos do atlas: dígitos, separadores e as letras de "Original", "Câmera", "Quadro", "Vídeo".
pub const GLIFOS_CARIMBO: &str = "0123456789:,./- OrignalC\u{e2}meQudoV\u{ed}";

/// Caractere que nenhuma fonte embutida desenha vira este (como no PDF).
const SUBSTITUTO: char = '\u{25A1}';

/// Maior sprite ou atlas (px): acima disso é tamanho de letra absurdo, e alocar derrubaria o processo.
const LIMITE_PX: u64 = 1 << 26;
/// Maior texto de um sprite (caracteres, somados os blocos): o SVG intermediário cresce com cada glifo.
const LIMITE_TEXTO: usize = 20_000;

const FONTES: &[(&str, &[u8])] = &[
    ("Inter-Regular.ttf", include_bytes!("fontes/Inter-Regular.ttf")),
    ("Inter-Bold.ttf", include_bytes!("fontes/Inter-Bold.ttf")),
    ("Inter-Italic.ttf", include_bytes!("fontes/Inter-Italic.ttf")),
    ("Inter-BoldItalic.ttf", include_bytes!("fontes/Inter-BoldItalic.ttf")),
    ("LibertinusSerif-Regular.otf", include_bytes!("fontes/LibertinusSerif-Regular.otf")),
    ("LibertinusSerif-Bold.otf", include_bytes!("fontes/LibertinusSerif-Bold.otf")),
    ("LibertinusSerif-Italic.otf", include_bytes!("fontes/LibertinusSerif-Italic.otf")),
    ("LibertinusSerif-BoldItalic.otf", include_bytes!("fontes/LibertinusSerif-BoldItalic.otf")),
    ("DejaVuSansMono.ttf", include_bytes!("fontes/DejaVuSansMono.ttf")),
    ("DejaVuSansMono-Bold.ttf", include_bytes!("fontes/DejaVuSansMono-Bold.ttf")),
];

/// Comum aos dois modelos: família, peso, estilo, tamanho em px do canvas e o contorno por baixo do texto.
const COMUM: &str = r##"
#let d = json("dados.json")
#set page(width: auto, height: auto, margin: 0pt, fill: none)
#set text(lang: "pt", region: "BR", top-edge: "ascender", bottom-edge: "descender", hyphenate: false)
#set par(justify: false, leading: 0.25em)
#let familia = (sans: "Inter", serifa: "Libertinus Serif", mono: "DejaVu Sans Mono")
#let alinha = (esq: left, centro: center, dir: right)
#let letra(e, cor, traco, corpo) = text(
  font: familia.at(e.fonte), weight: if e.negrito { "bold" } else { "regular" },
  style: if e.italico { "italic" } else { "normal" }, size: e.tamanho * d.altura * 1pt,
  fill: cor, stroke: traco, corpo)
#let folga(e) = if e.contorno == none { 0pt } else { e.contorno.largura * d.altura * 1pt }
#let com-contorno(e, caixa, corpo) = if e.contorno == none { caixa(letra(e, rgb(e.cor), none, corpo)) } else {
  let t = folga(e)
  let cor = rgb(e.contorno.cor)
  pad(rest: t, { place(top + left, caixa(letra(e, cor, (paint: cor, thickness: 2 * t, join: "round"), corpo))); caixa(letra(e, rgb(e.cor), none, corpo)) })
}
"##;

/// Sprite: cada bloco com largura natural limitada a `d.largura`, caixa de fundo opcional; blocos empilhados;
/// tamanho final arredondado para cima ao px inteiro.
const MODELO_SPRITE: &str = r##"
#let bloco(b) = context {
  let e = b.estilo
  let largura = calc.min(measure(letra(e, black, none, b.conteudo)).width, d.largura * 1pt)
  let corpo = com-contorno(e, c => block(width: largura, align(alinha.at(e.alinhamento), c)), b.conteudo)
  if d.fundo and e.caixa != none {
    block(fill: rgb(e.caixa.cor), inset: e.caixa.margem * d.altura * 1pt, radius: e.caixa.raio * d.altura * 1pt, corpo)
  } else { corpo }
}
#context {
  let partes = d.blocos.map(bloco)
  let largura = calc.max(..partes.map(p => measure(p).width))
  let tudo = stack(dir: ttb, ..partes.zip(d.blocos).map(((p, b)) => block(width: largura, align(alinha.at(b.estilo.alinhamento), p))))
  let s = measure(tudo)
  box(width: calc.ceil(s.width / 1pt) * 1pt, height: calc.ceil(s.height / 1pt) * 1pt, tudo)
}
"##;

/// Atlas: uma célula por glifo, todas do mesmo tamanho inteiro (o maior avanço, a altura da linha), com o fundo.
const MODELO_ATLAS: &str = r##"
#context {
  let e = d.estilo
  let t = folga(e) / 1pt
  let gs = d.glifos.clusters()
  let w = calc.ceil(calc.max(..gs.map(g => measure(letra(e, black, none, g)).width / 1pt)) + 2 * t)
  let h = calc.ceil(measure(letra(e, black, none, "0")).height / 1pt + 2 * t)
  let m = if e.caixa == none { 0 } else { calc.round(e.caixa.margem * d.altura) }
  let fundo = if e.caixa == none { none } else { rgb(e.caixa.cor) }
  stack(dir: ltr, ..gs.map(g => box(width: w * 1pt, height: (h + 2 * m) * 1pt, fill: fundo,
    align(center + horizon, com-contorno(e, c => c, g)))))
}
"##;

struct Base {
    fontes: Vec<Font>,
    livro: LazyHash<FontBook>,
    biblioteca: LazyHash<Library>,
    hashes: Vec<(String, String)>,
    /// SHA-256 da lista de hashes: entra na chave de sprites, atlas e camadas.
    impressao: String,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}

fn base() -> &'static Base {
    static B: OnceLock<Base> = OnceLock::new();
    B.get_or_init(|| {
        let fontes: Vec<Font> = FONTES.iter().flat_map(|(_, b)| Font::iter(Bytes::new(*b))).collect();
        let hashes: Vec<(String, String)> =
            FONTES.iter().map(|(n, b)| (n.to_string(), hex(&Sha256::digest(b)))).collect();
        let mut h = Sha256::new();
        for (n, s) in &hashes {
            h.update(format!("{n} {s}\n").as_bytes());
        }
        Base {
            livro: LazyHash::new(FontBook::from_fonts(&fontes)),
            fontes,
            biblioteca: LazyHash::new(Library::default()),
            hashes,
            impressao: hex(&h.finalize()),
        }
    })
}

struct Mundo<'a> {
    base: &'a Base,
    principal: Source,
    dados: (FileId, Bytes),
}

fn id(caminho: &str) -> Result<FileId> {
    let v = VirtualPath::new(caminho).map_err(|e| SicroError::Validation(format!("caminho do texto ({caminho}): {e}")))?;
    Ok(RootedPath::new(VirtualRoot::Project, v).intern())
}

impl World for Mundo<'_> {
    fn library(&self) -> &LazyHash<Library> {
        &self.base.biblioteca
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.base.livro
    }
    fn main(&self) -> FileId {
        self.principal.id()
    }
    fn source(&self, i: FileId) -> FileResult<Source> {
        if i == self.principal.id() {
            Ok(self.principal.clone())
        } else {
            Err(FileError::NotSource)
        }
    }
    fn file(&self, i: FileId) -> FileResult<Bytes> {
        if i == self.dados.0 {
            Ok(self.dados.1.clone())
        } else {
            Err(FileError::NotFound(i.vpath().get_without_slash().into()))
        }
    }
    fn font(&self, i: usize) -> Option<Font> {
        self.base.fontes.get(i).cloned()
    }
    fn today(&self, _: Option<Duration>) -> Option<Datetime> {
        Datetime::from_ymd(2000, 1, 1)
    }
}

fn erros(lista: &[SourceDiagnostic]) -> String {
    lista.iter().map(|d| d.message.to_string()).collect::<Vec<_>>().join(" | ")
}

/// Troca o que nenhuma fonte embutida desenha (emoji, ideogramas) por □. Devolve os trocados.
fn sanear(s: &str) -> (String, Vec<char>) {
    let fontes = &base().fontes;
    let falta = |c: char| !c.is_whitespace() && !fontes.iter().any(|f| f.info().coverage.contains(c as u32));
    let mut trocados = Vec::new();
    let s = s
        .chars()
        .map(|c| {
            if falta(c) {
                trocados.push(c);
                SUBSTITUTO
            } else {
                c
            }
        })
        .collect();
    (s, trocados)
}

/// Compila `modelo` (depois de `COMUM`) com `dados` e rasteriza a primeira página: RGBA de alfa reto.
fn rasterizar_modelo(modelo: &str, dados: &serde_json::Value) -> Result<Bitmap> {
    let b = base();
    let mundo = Mundo {
        base: b,
        principal: Source::new(id("modelo.typ")?, format!("{COMUM}{modelo}")),
        dados: (id("dados.json")?, Bytes::new(serde_json::to_vec(dados)?)),
    };
    let r = typst::compile::<PagedDocument>(&mundo);
    // o Typst memoriza num cache global: sem despejo a memória só cresce
    typst::comemo::evict(10);
    let doc = r.output.map_err(|e| SicroError::Workspace(format!("texto: {}", erros(&e))))?;
    let pagina = doc.pages().first().ok_or_else(|| SicroError::Workspace("texto: sem página".into()))?;
    let svg = typst_svg::svg(pagina, &typst_svg::SvgOptions::default());
    // 72 dpi: o pt do SVG vira 1 px
    let opcoes = resvg::usvg::Options { dpi: 72.0, ..Default::default() };
    let arvore =
        resvg::usvg::Tree::from_str(&svg, &opcoes).map_err(|e| SicroError::Workspace(format!("texto (svg): {e}")))?;
    let tam = arvore.size();
    let (w, h) = ((tam.width().ceil() as u32).max(1), (tam.height().ceil() as u32).max(1));
    if w as u64 * h as u64 > LIMITE_PX {
        return Err(SicroError::Validation(format!("{}: texto de {w}×{h} px", codigo::ITEM_INVALIDO)));
    }
    let mut pm = tiny_skia::Pixmap::new(w, h).ok_or_else(|| SicroError::Workspace("texto: tamanho".into()))?;
    resvg::render(&arvore, tiny_skia::Transform::identity(), &mut pm.as_mut());
    Ok(bitmap_reto(pm))
}

fn sha256_hex(texto: &[u8]) -> String {
    hex(&Sha256::digest(texto))
}

/// Grava com nome temporário e troca (dois escritores ao mesmo tempo não deixam arquivo pela metade).
pub(crate) fn gravar_atomico(destino: &Path, bytes: &[u8]) -> Result<()> {
    static N: AtomicU64 = AtomicU64::new(0);
    if let Some(p) = destino.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = destino.with_extension(format!("tmp-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    std::fs::write(&tmp, bytes)?;
    if let Err(e) = std::fs::rename(&tmp, destino) {
        let _ = std::fs::remove_file(&tmp);
        // no Windows a troca falha com o destino aberto por outro leitor: se já tem estes bytes, está feito
        if std::fs::read(destino).is_ok_and(|b| b == bytes) {
            return Ok(());
        }
        return Err(e.into());
    }
    Ok(())
}

/// Chave do sprite: modelo, versão, fontes e o pedido sem o que não muda pixel (id, lugar e largura dos blocos).
fn chave_sprite(pedido: &PedidoSprite) -> Result<String> {
    let blocos: Vec<serde_json::Value> = pedido
        .blocos
        .iter()
        .map(|b| serde_json::json!({ "conteudo": b.conteudo, "estilo": b.estilo }))
        .collect();
    let canonico = serde_json::json!({
        "versao": VERSAO_RASTERIZADOR, "fontes": base().impressao, "modelo": sha256_hex(MODELO_SPRITE.as_bytes()),
        "comum": sha256_hex(COMUM.as_bytes()), "blocos": blocos, "largura_max": pedido.largura_max,
        "altura_canvas": pedido.altura_canvas, "fundo": pedido.fundo,
    });
    Ok(sha256_hex(&serde_json::to_vec(&canonico)?))
}

fn memoria() -> &'static Mutex<HashMap<PathBuf, (u32, u32)>> {
    static M: OnceLock<Mutex<HashMap<PathBuf, (u32, u32)>>> = OnceLock::new();
    M.get_or_init(Default::default)
}

/// Tamanho do PNG já no cache. Na primeira vez do processo o arquivo é decodificado inteiro: PNG cortado
/// (queda de energia, disco) não serve e é refeito.
fn no_cache(arq: &Path) -> Option<(u32, u32)> {
    if !arq.is_file() {
        return None;
    }
    if let Some(t) = memoria().lock().unwrap_or_else(|e| e.into_inner()).get(arq) {
        return Some(*t);
    }
    let img = image::open(arq).ok()?;
    let t = (img.width(), img.height());
    memoria().lock().unwrap_or_else(|e| e.into_inner()).insert(arq.to_path_buf(), t);
    Some(t)
}

fn guardar(arq: &Path, b: &Bitmap) -> Result<()> {
    gravar_atomico(arq, &b.png()?)?;
    memoria().lock().unwrap_or_else(|e| e.into_inner()).insert(arq.to_path_buf(), (b.w, b.h));
    Ok(())
}

/// As cores do estilo têm de ser as do modelo (`#rrggbb[aa]`): o Typst aceita outras formas e recusa com texto dele.
fn conferir_cores(e: &EstiloTexto) -> Result<()> {
    cor::ler(&e.cor)?;
    if let Some(c) = &e.caixa {
        cor::ler(&c.cor)?;
    }
    if let Some(c) = &e.contorno {
        cor::ler(&c.cor)?;
    }
    Ok(())
}

/// Bitmap do sprite (sem cache): o que `sprite` grava.
pub(crate) fn bitmap_sprite(pedido: &PedidoSprite) -> Result<(Bitmap, Vec<char>)> {
    let altura = pedido.altura_canvas as f64;
    let largura = pedido.largura_max * altura;
    if pedido.altura_canvas == 0 || largura.is_nan() || largura <= 0.0 || !largura.is_finite() {
        return Err(SicroError::Validation(format!("{}: largura_max", codigo::ITEM_INVALIDO)));
    }
    for b in &pedido.blocos {
        conferir_cores(&b.estilo)?;
    }
    if pedido.blocos.iter().map(|b| b.conteudo.chars().count()).sum::<usize>() > LIMITE_TEXTO {
        return Err(SicroError::Validation(format!("{}: texto longo demais", codigo::ITEM_INVALIDO)));
    }
    if pedido.blocos.is_empty() {
        return Ok((Bitmap { w: 1, h: 1, canais: 4, px: vec![0; 4] }, Vec::new()));
    }
    let mut trocados = Vec::new();
    let blocos: Vec<serde_json::Value> = pedido
        .blocos
        .iter()
        .map(|b| {
            let (c, t) = sanear(&b.conteudo);
            trocados.extend(t);
            serde_json::json!({ "conteudo": c, "estilo": b.estilo })
        })
        .collect();
    let dados = serde_json::json!({ "altura": altura, "largura": largura, "fundo": pedido.fundo, "blocos": blocos });
    Ok((rasterizar_modelo(MODELO_SPRITE, &dados)?, trocados))
}

/// Rasteriza (ou acha no cache) o sprite do pedido. A chave inclui a altura do canvas.
pub fn sprite(cache: &Path, pedido: &PedidoSprite) -> Result<Sprite> {
    let relativo = format!("sprites/{}.png", chave_sprite(pedido)?);
    let arq = cache.join(&relativo);
    if let Some((w, h)) = no_cache(&arq) {
        return Ok(Sprite { relativo, w, h });
    }
    let (b, _) = bitmap_sprite(pedido)?;
    guardar(&arq, &b)?;
    Ok(Sprite { relativo, w: b.w, h: b.h })
}

/// Bitmap do atlas (sem cache) e o tamanho da célula.
pub(crate) fn bitmap_atlas(estilo: &EstiloTexto, altura_canvas: u32) -> Result<(Bitmap, TamanhoPx)> {
    if altura_canvas == 0 {
        return Err(SicroError::Validation(format!("{}: altura_canvas", codigo::ITEM_INVALIDO)));
    }
    conferir_cores(estilo)?;
    let dados = serde_json::json!({ "altura": altura_canvas as f64, "glifos": GLIFOS_CARIMBO, "estilo": estilo });
    let b = rasterizar_modelo(MODELO_ATLAS, &dados)?;
    let n = GLIFOS_CARIMBO.chars().count() as u32;
    if b.w % n != 0 {
        return Err(SicroError::Workspace(format!("atlas: largura {} não divide em {n} células", b.w)));
    }
    let celula = TamanhoPx { w: b.w / n, h: b.h };
    Ok((b, celula))
}

/// Atlas de glifos do carimbo: dígitos, separadores e os prefixos "Original", "Câmera", "Quadro", "Vídeo".
pub fn atlas_carimbo(cache: &Path, estilo: &EstiloTexto, altura_canvas: u32) -> Result<Atlas> {
    let canonico = serde_json::json!({
        "versao": VERSAO_RASTERIZADOR, "fontes": base().impressao, "modelo": sha256_hex(MODELO_ATLAS.as_bytes()),
        "comum": sha256_hex(COMUM.as_bytes()), "glifos": GLIFOS_CARIMBO, "estilo": estilo, "altura_canvas": altura_canvas,
    });
    let relativo = format!("atlas/{}.png", sha256_hex(&serde_json::to_vec(&canonico)?));
    let arq = cache.join(&relativo);
    let n = GLIFOS_CARIMBO.chars().count() as u32;
    if let Some((w, h)) = no_cache(&arq).filter(|(w, _)| w % n == 0) {
        return Ok(Atlas { relativo, celula: TamanhoPx { w: w / n, h }, glifos: GLIFOS_CARIMBO.into() });
    }
    let (b, celula) = bitmap_atlas(estilo, altura_canvas)?;
    guardar(&arq, &b)?;
    Ok(Atlas { relativo, celula, glifos: GLIFOS_CARIMBO.into() })
}

/// SHA-256 de cada fonte embutida: (nome, sha256). Entra no hash dos pedidos e em `execucao.fontes`.
pub fn fontes() -> Vec<(String, String)> {
    base().hashes.clone()
}

/// SHA-256 da lista de fontes (uma linha por fonte): o que entra nas chaves.
pub(crate) fn impressao_das_fontes() -> &'static str {
    &base().impressao
}

/// SHA-256 dos modelos Typst, dos glifos do atlas e do substituto: mudou o modelo, mudou o hash das camadas.
pub(crate) fn impressao_dos_modelos() -> &'static str {
    static M: OnceLock<String> = OnceLock::new();
    M.get_or_init(|| sha256_hex(format!("{COMUM}\0{MODELO_SPRITE}\0{MODELO_ATLAS}\0{GLIFOS_CARIMBO}\0{SUBSTITUTO}").as_bytes()))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::modelo::{Alinhamento, AlinhamentoV, CaixaTexto, ContornoTexto, Fonte, Lugar, PapelTexto};

    fn estilo(fonte: Fonte, caixa: Option<&str>, contorno: Option<&str>) -> EstiloTexto {
        EstiloTexto {
            fonte,
            negrito: false,
            italico: false,
            tamanho: 0.05,
            cor: "#ffffff".into(),
            alinhamento: Alinhamento::Centro,
            caixa: caixa.map(|c| CaixaTexto { cor: c.into(), margem: 0.01, raio: 0.005 }),
            contorno: contorno.map(|c| ContornoTexto { cor: c.into(), largura: 0.003 }),
        }
    }

    fn pedido(conteudo: &str, e: EstiloTexto, largura_max: f64) -> PedidoSprite {
        PedidoSprite {
            blocos: vec![BlocoTexto {
                id: "b1".into(),
                papel: PapelTexto::Corpo,
                conteudo: conteudo.into(),
                estilo: e,
                lugar: Lugar { x: 0.5, y: 0.5, h: Alinhamento::Centro, v: AlinhamentoV::Meio },
                largura_max: 0.5,
            }],
            largura_max,
            altura_canvas: 1080,
            fundo: true,
        }
    }

    fn sprite_de(conteudo: &str, e: EstiloTexto) -> Bitmap {
        let (b, trocados) = bitmap_sprite(&pedido(conteudo, e, 1.5)).unwrap();
        assert!(trocados.is_empty(), "{conteudo}: {trocados:?}");
        b
    }

    fn alfa(b: &Bitmap, x: u32, y: u32) -> u8 {
        b.px[((y * b.w + x) * 4 + 3) as usize]
    }

    fn soma_alfa(b: &Bitmap) -> u64 {
        b.px.as_chunks::<4>().0.iter().map(|p| p[3] as u64).sum()
    }

    #[test]
    fn sprite_com_alfa_e_acentos() {
        for fonte in [Fonte::Sans, Fonte::Serifa, Fonte::Mono] {
            let com = sprite_de("ção ã é Ç", estilo(fonte, None, None));
            let sem = sprite_de("cao a e C", estilo(fonte, None, None));
            assert_ne!(com, sem);
            assert!(soma_alfa(&com) > soma_alfa(&sem), "{fonte:?}: os acentos têm de aparecer");
            assert_eq!(alfa(&com, 0, 0), 0);
            assert!(com.px.as_chunks::<4>().0.iter().any(|p| p[3] == 255) && com.px.as_chunks::<4>().0.iter().any(|p| p[3] > 0 && p[3] < 255));
            // texto branco em alfa reto: todo pixel visível é branco
            assert!(com.px.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).all(|p| p[..3] == [255, 255, 255]));
        }
        // caixa de fundo meio transparente: alfa 128 na borda esquerda, cor preta
        let b = sprite_de("Texto", estilo(Fonte::Sans, Some("#00000080"), None));
        let i = ((b.h / 2 * b.w + 1) * 4) as usize;
        assert_eq!(&b.px[i..i + 4], &[0, 0, 0, 128]);
    }

    #[test]
    fn texto_que_parece_codigo_sai_literal() {
        let e = || estilo(Fonte::Sans, None, None);
        for (codigo, simples) in [("*a*", "a"), ("$x$", "x"), ("#import \"x.typ\": *", "import \"x.typ\": "), ("_b_", "b")] {
            let a = sprite_de(codigo, e());
            let b = sprite_de(simples, e());
            assert!(a.w > b.w, "{codigo} ({}) devia ser mais largo que {simples} ({})", a.w, b.w);
        }
        for c in ["#panic(\"x\")", "#let x = 1 #x", "@d <e> [f] \\ // `g` ]", "#{", "\"", "#set text(size: 1000pt)"] {
            sprite_de(c, e());
        }
        // `#set` como texto não muda o tamanho do resto
        assert_eq!(sprite_de("#set text(size: 1000pt)", e()).h, sprite_de("abc", e()).h);
    }

    #[test]
    fn quebra_de_linha_e_largura() {
        let e = || estilo(Fonte::Serifa, None, None);
        let uma = sprite_de("linha um", e());
        let duas = sprite_de("linha um\nlinha dois", e());
        assert!(duas.h as f64 > uma.h as f64 * 1.6, "{} × {}", duas.h, uma.h);
        let longo = "o veículo entra no cruzamento e reduz antes da faixa de pedestres";
        let (estreito, _) = bitmap_sprite(&pedido(longo, e(), 0.3)).unwrap();
        assert!(estreito.w as f64 <= 0.3 * 1080.0 + 1.0, "{}", estreito.w);
        assert!(estreito.h > 2 * uma.h);
    }

    #[test]
    fn contorno_por_baixo_do_texto() {
        let b = sprite_de("I", estilo(Fonte::Sans, None, Some("#ff0000")));
        // meio da haste: cor do texto; algum pixel só do contorno
        let i = ((b.h / 2 * b.w + b.w / 2) * 4) as usize;
        assert_eq!(&b.px[i..i + 4], &[255, 255, 255, 255]);
        assert!(b.px.as_chunks::<4>().0.contains(&[255, 0, 0, 255]));
    }

    #[test]
    fn caractere_sem_glifo_vira_quadrado() {
        let (_, trocados) = bitmap_sprite(&pedido("emoji 😀 e 中", estilo(Fonte::Sans, None, None), 1.5)).unwrap();
        assert_eq!(trocados, vec!['😀', '中']);
    }

    #[test]
    fn mesmos_bytes_e_cache() {
        let p = pedido("Câmera 2 — 14/03/2026", estilo(Fonte::Sans, Some("#000000a0"), Some("#000000")), 1.5);
        let a = bitmap_sprite(&p).unwrap().0.png().unwrap();
        typst::comemo::evict(0);
        let b = bitmap_sprite(&p).unwrap().0.png().unwrap();
        assert_eq!(a, b);
        let cache = crate::edicao::testes::fontes::pasta_temporaria("p05-sprite");
        let s = sprite(&cache, &p).unwrap();
        assert_eq!(std::fs::read(cache.join(&s.relativo)).unwrap(), a);
        assert_eq!(sprite(&cache, &p).unwrap(), s);
        // lugar, largura e id do bloco não mudam o sprite; a altura do canvas muda
        let mut q = p.clone();
        q.blocos[0].lugar.x = 0.1;
        q.blocos[0].largura_max = 0.9;
        q.blocos[0].id = "outro".into();
        assert_eq!(sprite(&cache, &q).unwrap().relativo, s.relativo);
        q.altura_canvas = 720;
        let s720 = sprite(&cache, &q).unwrap();
        assert_ne!(s720.relativo, s.relativo);
        assert!(s720.h < s.h);
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn atlas_do_carimbo() {
        let cache = crate::edicao::testes::fontes::pasta_temporaria("p05-atlas");
        let n = GLIFOS_CARIMBO.chars().count() as u32;
        for (fonte, caixa) in [(Fonte::Mono, None), (Fonte::Sans, Some("#00000080"))] {
            let e = estilo(fonte, caixa, None);
            let a = atlas_carimbo(&cache, &e, 1080).unwrap();
            let img = image::open(cache.join(&a.relativo)).unwrap().to_rgba8();
            assert_eq!((img.width(), img.height()), (a.celula.w * n, a.celula.h));
            assert!(a.celula.w > 10 && a.celula.h > 40, "{:?}", a.celula);
            // cada célula de letra ou dígito tem tinta; o espaço só tem o fundo
            for (i, g) in GLIFOS_CARIMBO.chars().enumerate() {
                let x0 = i as u32 * a.celula.w;
                let tinta = (x0..x0 + a.celula.w)
                    .flat_map(|x| (0..a.celula.h).map(move |y| (x, y)))
                    .filter(|&(x, y)| img.get_pixel(x, y)[3] > 200 && img.get_pixel(x, y)[0] > 200)
                    .count();
                assert_eq!(tinta == 0, g == ' ', "glifo {g:?}: {tinta}");
            }
            if caixa.is_some() {
                assert_eq!(img.get_pixel(0, 0).0, [0, 0, 0, 128]);
            }
            assert_eq!(atlas_carimbo(&cache, &e, 1080).unwrap(), a);
        }
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn fontes_embutidas() {
        let f = fontes();
        assert_eq!(f.len(), 10);
        assert!(f.iter().all(|(_, h)| h.len() == 64));
        let familias: Vec<String> = base().fontes.iter().map(|f| f.info().family.to_string()).collect();
        for nome in ["Inter", "Libertinus Serif", "DejaVu Sans Mono"] {
            assert!(familias.iter().any(|f| f == nome), "{nome} em {familias:?}");
        }
    }
    #[test]
    fn estilo_como_dado_cor_invalida_e_tamanho_absurdo() {
        let codigo_de = |e: SicroError| match e {
            SicroError::Validation(m) => m.split(':').next().unwrap_or_default().to_string(),
            outro => panic!("erro fora do catálogo: {outro}"),
        };
        // cor fora de #rrggbb[aa] (inclusive a que o Typst aceitaria, e uma que tenta fechar a chamada): código `cor`
        for cor in ["red", "#fff", "#ffffff\"); #panic(\"x", ""] {
            let mut e = estilo(Fonte::Sans, None, None);
            e.cor = cor.into();
            assert_eq!(codigo_de(bitmap_sprite(&pedido("abc", e.clone(), 1.5)).unwrap_err()), codigo::COR, "{cor:?}");
            assert_eq!(codigo_de(bitmap_atlas(&e, 1080).unwrap_err()), codigo::COR, "{cor:?}");
        }
        assert_eq!(codigo_de(bitmap_sprite(&pedido("abc", estilo(Fonte::Sans, Some("x"), None), 1.5)).unwrap_err()), codigo::COR);
        assert_eq!(codigo_de(bitmap_sprite(&pedido("abc", estilo(Fonte::Sans, None, Some("x")), 1.5)).unwrap_err()), codigo::COR);
        // letra de 30× a altura do canvas: recusado antes de alocar
        let mut e = estilo(Fonte::Sans, None, None);
        e.tamanho = 30.0;
        assert_eq!(codigo_de(bitmap_sprite(&pedido("abc abc abc", e, 100.0)).unwrap_err()), codigo::ITEM_INVALIDO);
        // texto de tamanho absurdo: recusado antes de compor
        let longo = "a".repeat(LIMITE_TEXTO + 1);
        assert_eq!(codigo_de(bitmap_sprite(&pedido(&longo, estilo(Fonte::Sans, None, None), 1.5)).unwrap_err()), codigo::ITEM_INVALIDO);
        // nada disto derruba: vazio, só espaço, só quebra, tamanho zero ou negativo, largura mínima, margens negativas
        for (c, tam, larg) in [(" ", 0.05, 1.5), ("\n", 0.05, 1.5), ("\t\r\n", 0.05, 1.5), ("abc", 0.0, 1.5), ("abc", -0.05, 1.5), ("abc def", 0.05, 1e-6)] {
            let mut e = estilo(Fonte::Serifa, Some("#00000080"), Some("#000000"));
            e.tamanho = tam;
            e.caixa.as_mut().unwrap().margem = -0.01;
            e.contorno.as_mut().unwrap().largura = -0.01;
            let (b, _) = bitmap_sprite(&pedido(c, e, larg)).unwrap_or_else(|e| panic!("{c:?} {tam}: {e}"));
            assert!(b.w >= 1 && b.h >= 1);
        }
    }

    #[test]
    fn sprite_cortado_no_cache_e_refeito() {
        let p = pedido("refeito", estilo(Fonte::Mono, None, None), 1.5);
        let bom = bitmap_sprite(&p).unwrap().0.png().unwrap();
        for (nome, lixo) in [("0 byte", Vec::new()), ("cortado", bom[..bom.len() * 2 / 3].to_vec()), ("outra coisa", b"nao e png".to_vec())] {
            let cache = crate::edicao::testes::fontes::pasta_temporaria("p05-cortado");
            let relativo = format!("sprites/{}.png", chave_sprite(&p).unwrap());
            std::fs::create_dir_all(cache.join("sprites")).unwrap();
            std::fs::write(cache.join(&relativo), &lixo).unwrap();
            let s = sprite(&cache, &p).unwrap();
            assert_eq!(s.relativo, relativo);
            assert_eq!(std::fs::read(cache.join(&relativo)).unwrap(), bom, "{nome}");
            let _ = std::fs::remove_dir_all(&cache);
        }
    }
    #[test]
    fn atlas_cada_celula_tem_o_seu_glifo() {
        // tinta (soma do alfa) da célula i = tinta do glifo i desenhado sozinho; troca de ordem ou deslocamento de uma
        // célula não passa
        for (fonte, negrito) in [(Fonte::Mono, false), (Fonte::Sans, true), (Fonte::Serifa, false)] {
            let mut e = estilo(fonte, None, None);
            e.negrito = negrito;
            let (a, celula) = bitmap_atlas(&e, 1080).unwrap();
            let mut tintas = Vec::new();
            for (i, g) in GLIFOS_CARIMBO.chars().enumerate() {
                let x0 = i as u32 * celula.w;
                let na_celula: u64 = (0..celula.h).flat_map(|y| (x0..x0 + celula.w).map(move |x| (x, y))).map(|(x, y)| alfa(&a, x, y) as u64).sum();
                let sozinho = if g == ' ' { 0 } else { soma_alfa(&bitmap_sprite(&pedido(&g.to_string(), e.clone(), 1.5)).unwrap().0) };
                // a suavização não conserva a tinta ao décimo de pixel: 8 %
                let folga = (sozinho / 12).max(600);
                assert!(na_celula.abs_diff(sozinho) <= folga, "{fonte:?} glifo {g:?} (célula {i}): {na_celula} × {sozinho}");
                tintas.push(sozinho);
            }
            // a medida separa os glifos: ao menos 20 tintas diferentes entre os 31
            tintas.sort_unstable();
            tintas.dedup();
            assert!(tintas.len() >= 20, "{fonte:?}: {}", tintas.len());
        }
    }
}
