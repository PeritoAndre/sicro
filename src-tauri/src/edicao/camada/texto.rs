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
use crate::edicao::modelo::{BlocoTexto, EstiloTexto, SombraTexto, TamanhoPx};
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
    ("DejaVuSansMono-Oblique.ttf", include_bytes!("fontes/DejaVuSansMono-Oblique.ttf")),
    ("DejaVuSansMono-BoldOblique.ttf", include_bytes!("fontes/DejaVuSansMono-BoldOblique.ttf")),
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

/// Sprite: cada bloco com largura natural limitada a `d.largura` (faixa: a largura inteira), caixa de fundo opcional;
/// blocos empilhados; tamanho final arredondado para cima ao px inteiro. `b.texto`/`b.caixa` falsos escondem o texto
/// ou a cor da caixa sem mudar o arranjo (as passadas da sombra: caixa, sombra e texto compostos à parte).
/// Palavra mais larga que a caixa quebra em qualquer letra (antes saía cortada na borda do sprite); sem palavra assim
/// o conteúdo vai igual (mesmos bytes de antes).
const MODELO_SPRITE: &str = r##"
#let quebrar(e, s, w) = s.split("\n").map(l => l.split(" ").map(p => if p != "" and measure(letra(e, black, none, p)).width > w { p.clusters().join("\u{200B}") } else { p }).join(" ")).join("\n")
#let bloco(b) = context {
  let e = b.estilo
  let largura = if b.faixa { d.largura * 1pt } else { calc.min(measure(letra(e, black, none, b.conteudo)).width, d.largura * 1pt) }
  let corpo = com-contorno(e, c => block(width: largura, align(alinha.at(e.alinhamento), c)), quebrar(e, b.conteudo, largura))
  let corpo = if b.texto { corpo } else { hide(corpo) }
  if d.fundo and e.caixa != none {
    block(fill: if b.caixa { rgb(e.caixa.cor) } else { none }, inset: e.caixa.margem * d.altura * 1pt, radius: e.caixa.raio * d.altura * 1pt, corpo)
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
    Ok(bitmap_reto(rasterizar_pixmap(modelo, dados)?))
}

/// O mesmo, em RGBA pré-multiplicado (as passadas da sombra são compostas antes de voltar ao alfa reto).
fn rasterizar_pixmap(modelo: &str, dados: &serde_json::Value) -> Result<tiny_skia::Pixmap> {
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
    Ok(pm)
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
    if let Some(s) = &e.sombra {
        cor::ler(&s.cor)?;
    }
    Ok(())
}

// CONTRATO: acréscimo da abertura da Fase 4 (contratos-fase4.md B2): regra única do pacote do texto livre.
/// Detalhe do `nao_suportado` do estilo no sprite; None = desenha. Desde o pacote do texto livre o sprite desenha
/// sombra e faixa: nada mais é recusado aqui (o atlas do carimbo tem a regra dele, `recusa_do_atlas`).
pub fn recusa_estilo(e: &EstiloTexto) -> Option<&'static str> {
    let _ = e;
    None
}

// CONTRATO: acréscimo do pacote do texto livre (Fase 4): `recusa_do_atlas`, `SOMBRA_MAX`, `sombra_ok` (para a
// validação do P4.R) e `folga_da_sombra` (espelho em `modelo/sobreposto.ts`, para as alças do palco).
/// O atlas do carimbo é uma célula por glifo: não tem largura de bloco (faixa) nem lugar para o desfoque da sombra.
pub fn recusa_do_atlas(e: &EstiloTexto) -> Option<&'static str> {
    if e.sombra.is_some() {
        return Some("sombra");
    }
    if e.caixa.as_ref().is_some_and(|c| c.faixa) {
        return Some("faixa");
    }
    None
}

/// Maior deslocamento e maior desfoque da sombra, em fração da altura do canvas.
pub const SOMBRA_MAX: f64 = 0.05;

/// Sombra dentro da faixa que o sprite desenha (deslocamento e desfoque finitos, até `SOMBRA_MAX`; desfoque ≥ 0).
pub fn sombra_ok(s: &SombraTexto) -> bool {
    let ok = |v: f64| v.is_finite() && v.abs() <= SOMBRA_MAX;
    ok(s.dx) && ok(s.dy) && ok(s.desfoque) && s.desfoque >= 0.0
}

/// Arredondamento do Rust (meio para longe do zero), o que o espelho repete.
fn arredondar(v: f64) -> i64 {
    v.round() as i64
}

/// Raios (px) das 3 passadas de caixa que aproximam a gaussiana de desvio `sigma` px ("boxes for gauss"):
/// larguras ímpares `wl` e `wl + 2`, as `m` primeiras na menor.
pub(crate) fn raios_do_desfoque(sigma: f64) -> [u32; 3] {
    if !(sigma > 0.0) || !sigma.is_finite() {
        return [0; 3];
    }
    let ideal = (4.0 * sigma * sigma + 1.0).sqrt();
    let mut wl = ideal.floor() as i64;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m = arredondar((12.0 * sigma * sigma - 3.0 * (wl * wl) as f64 - 12.0 * wl as f64 - 9.0) / (-4.0 * wl as f64 - 4.0));
    let raio = |i: i64| ((if i < m { wl } else { wu }) - 1).max(0) as u32 / 2;
    [raio(0), raio(1), raio(2)]
}

/// Quanto o sprite cresce de cada lado (px) para caber a sombra: deslocamento arredondado + os 3 raios do desfoque.
/// Espelho em `modelo/sobreposto.ts::folgaDaSombra` (fixture `texto-folga-sombra.json`).
pub fn folga_da_sombra(s: &SombraTexto, altura_canvas: u32) -> (u32, u32) {
    let h = altura_canvas as f64;
    let r: u32 = raios_do_desfoque(s.desfoque * h).iter().sum();
    (arredondar(s.dx * h).unsigned_abs() as u32 + r, arredondar(s.dy * h).unsigned_abs() as u32 + r)
}

/// Uma passada de caixa de raio `r` sobre `n` valores a passo `passo` a partir de `inicio` (fora = 0), arredondada.
fn caixa_1d(v: &mut [u32], tmp: &mut Vec<u32>, inicio: usize, n: usize, passo: usize, r: usize) {
    if r == 0 || n == 0 {
        return;
    }
    tmp.clear();
    tmp.extend((0..n).map(|i| v[inicio + i * passo]));
    let d = (2 * r + 1) as u64;
    // soma da janela [i − r, i + r] com zeros fora
    let mut soma: u64 = tmp.iter().take(r.min(n)).map(|&x| x as u64).sum();
    for i in 0..n {
        if i + r < n {
            soma += tmp[i + r] as u64;
        }
        v[inicio + i * passo] = ((soma + d / 2) / d) as u32;
        if i >= r {
            soma -= tmp[i - r] as u64;
        }
    }
}

/// Alfa (pré-multiplicado) de `silhueta` posto em (x0, y0) de uma tela `w`×`h`, desfocado pelas 3 caixas (uma
/// horizontal e uma vertical por passada). Inteiros com 8 bits de fração: o mesmo resultado em toda máquina.
fn alfa_desfocado(silhueta: &tiny_skia::Pixmap, x0: u32, y0: u32, w: u32, h: u32, raios: [u32; 3]) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let mut v = vec![0u32; w * h];
    let sw = silhueta.width() as usize;
    for (i, p) in silhueta.pixels().iter().enumerate() {
        let (x, y) = (x0 as usize + i % sw, y0 as usize + i / sw);
        v[y * w + x] = (p.alpha() as u32) << 8;
    }
    let mut tmp = Vec::new();
    for r in raios {
        for y in 0..h {
            caixa_1d(&mut v, &mut tmp, y * w, w, 1, r as usize);
        }
        for x in 0..w {
            caixa_1d(&mut v, &mut tmp, x, h, w, r as usize);
        }
    }
    v.into_iter().map(|a| ((a + 128) >> 8).min(255) as u8).collect()
}

/// Dados do modelo com o que cada passada mostra de cada bloco (`texto`, `caixa`; `faixa` vem do estilo).
fn dados_do_sprite(pedido: &PedidoSprite, conteudos: &[String], mostra: impl Fn(usize) -> (bool, bool)) -> serde_json::Value {
    let altura = pedido.altura_canvas as f64;
    let blocos: Vec<serde_json::Value> = pedido
        .blocos
        .iter()
        .zip(conteudos)
        .enumerate()
        .map(|(i, (b, c))| {
            let (texto, caixa) = mostra(i);
            let faixa = b.estilo.caixa.as_ref().is_some_and(|c| c.faixa);
            serde_json::json!({ "conteudo": c, "estilo": b.estilo, "faixa": faixa, "texto": texto, "caixa": caixa })
        })
        .collect();
    serde_json::json!({ "altura": altura, "largura": pedido.largura_max * altura, "fundo": pedido.fundo, "blocos": blocos })
}

/// Sprite com sombra: a caixa, depois a sombra de cada grupo de blocos de mesma sombra (a silhueta do texto com o
/// contorno, na cor da sombra, deslocada e desfocada), depois o texto; a tela cresce a mesma folga dos dois lados
/// (o centro do texto não muda).
fn sprite_com_sombra(pedido: &PedidoSprite, conteudos: &[String]) -> Result<Bitmap> {
    let n = pedido.blocos.len();
    let com_caixa = pedido.fundo && pedido.blocos.iter().any(|b| b.estilo.caixa.is_some());
    let texto = rasterizar_pixmap(MODELO_SPRITE, &dados_do_sprite(pedido, conteudos, |_| (true, false)))?;
    let (w, h) = (texto.width(), texto.height());
    // grupos de blocos com a mesma sombra, na ordem da primeira aparição
    let mut grupos: Vec<(&SombraTexto, Vec<usize>)> = Vec::new();
    for (i, b) in pedido.blocos.iter().enumerate() {
        if let Some(s) = &b.estilo.sombra {
            match grupos.iter_mut().find(|(x, _)| *x == s) {
                Some((_, v)) => v.push(i),
                None => grupos.push((s, vec![i])),
            }
        }
    }
    let (mut fx, mut fy) = (0u32, 0u32);
    for (s, _) in &grupos {
        let (x, y) = folga_da_sombra(s, pedido.altura_canvas);
        (fx, fy) = (fx.max(x), fy.max(y));
    }
    let (w2, h2) = (w as u64 + 2 * fx as u64, h as u64 + 2 * fy as u64);
    if w2 * h2 > LIMITE_PX {
        return Err(SicroError::Validation(format!("{}: texto de {w2}×{h2} px", codigo::ITEM_INVALIDO)));
    }
    let (w2, h2) = (w2 as u32, h2 as u32);
    let mut tela = tiny_skia::Pixmap::new(w2, h2).ok_or_else(|| SicroError::Workspace("texto: tamanho".into()))?;
    let copia = tiny_skia::PixmapPaint::default();
    let id = tiny_skia::Transform::identity();
    if com_caixa {
        let caixa = rasterizar_pixmap(MODELO_SPRITE, &dados_do_sprite(pedido, conteudos, |_| (false, true)))?;
        tela.draw_pixmap(fx as i32, fy as i32, caixa.as_ref(), &copia, id, None);
    }
    for (s, blocos) in &grupos {
        let silhueta = if blocos.len() == n {
            None
        } else {
            Some(rasterizar_pixmap(MODELO_SPRITE, &dados_do_sprite(pedido, conteudos, |i| (blocos.contains(&i), false)))?)
        };
        let silhueta = silhueta.as_ref().unwrap_or(&texto);
        if (silhueta.width(), silhueta.height()) != (w, h) {
            return Err(SicroError::Workspace("texto: passadas da sombra de tamanhos diferentes".into()));
        }
        let altura = pedido.altura_canvas as f64;
        let (ox, oy) = (arredondar(s.dx * altura), arredondar(s.dy * altura));
        let x0 = (fx as i64 + ox) as u32;
        let y0 = (fy as i64 + oy) as u32;
        let alfa = alfa_desfocado(silhueta, x0, y0, w2, h2, raios_do_desfoque(s.desfoque * altura));
        let c = cor::ler(&s.cor)?;
        let mut sombra = tiny_skia::Pixmap::new(w2, h2).ok_or_else(|| SicroError::Workspace("texto: tamanho".into()))?;
        let mul = |v: u8, a: u32| ((v as u32 * a + 127) / 255) as u8;
        for (p, a) in sombra.pixels_mut().iter_mut().zip(alfa) {
            let a = (a as u32 * c.a as u32 + 127) / 255;
            if a > 0 {
                *p = tiny_skia::PremultipliedColorU8::from_rgba(mul(c.r, a), mul(c.g, a), mul(c.b, a), a as u8).unwrap_or(*p);
            }
        }
        tela.draw_pixmap(0, 0, sombra.as_ref(), &copia, id, None);
    }
    tela.draw_pixmap(fx as i32, fy as i32, texto.as_ref(), &copia, id, None);
    Ok(bitmap_reto(tela))
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
        if let Some(d) = recusa_estilo(&b.estilo) {
            return Err(SicroError::Validation(format!("{}: {d}", codigo::NAO_SUPORTADO)));
        }
        if b.estilo.sombra.as_ref().is_some_and(|s| !sombra_ok(s)) {
            return Err(SicroError::Validation(format!("{}: sombra", codigo::ITEM_INVALIDO)));
        }
    }
    if pedido.blocos.iter().map(|b| b.conteudo.chars().count()).sum::<usize>() > LIMITE_TEXTO {
        return Err(SicroError::Validation(format!("{}: texto longo demais", codigo::ITEM_INVALIDO)));
    }
    if pedido.blocos.is_empty() {
        return Ok((Bitmap { w: 1, h: 1, canais: 4, px: vec![0; 4] }, Vec::new()));
    }
    let mut trocados = Vec::new();
    let conteudos: Vec<String> = pedido
        .blocos
        .iter()
        .map(|b| {
            let (c, t) = sanear(&b.conteudo);
            trocados.extend(t);
            c
        })
        .collect();
    if pedido.blocos.iter().any(|b| b.estilo.sombra.is_some()) {
        return Ok((sprite_com_sombra(pedido, &conteudos)?, trocados));
    }
    // sem sombra: uma passada só (os mesmos bytes de antes da Fase 4 para quem não usa faixa)
    Ok((rasterizar_modelo(MODELO_SPRITE, &dados_do_sprite(pedido, &conteudos, |_| (true, true)))?, trocados))
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
    // recusa explícita: o atlas não desenharia a sombra nem a faixa (nunca ignorar calado)
    if let Some(d) = recusa_do_atlas(estilo) {
        return Err(SicroError::Validation(format!("{}: {d}", codigo::NAO_SUPORTADO)));
    }
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
            caixa: caixa.map(|c| CaixaTexto { cor: c.into(), margem: 0.01, raio: 0.005, faixa: false }),
            contorno: contorno.map(|c| ContornoTexto { cor: c.into(), largura: 0.003 }),
            sombra: None,
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
        assert_eq!(f.len(), 12);
        assert!(f.iter().all(|(_, h)| h.len() == 64));
        let familias: Vec<String> = base().fontes.iter().map(|f| f.info().family.to_string()).collect();
        for nome in ["Inter", "Libertinus Serif", "DejaVu Sans Mono"] {
            assert!(familias.iter().any(|f| f == nome), "{nome} em {familias:?}");
        }
    }
    /// Mono itálico sai inclinado, com e sem negrito (sem as oblíquas da DejaVu o Typst caía na reta).
    #[test]
    fn mono_italico_inclina() {
        // centro horizontal da tinta no terço de cima e no de baixo das linhas com tinta
        let inclinacao = |b: &Bitmap| {
            let alfa = |x: u32, y: u32| b.px[((y * b.w + x) * b.canais as u32 + b.canais as u32 - 1) as usize] as f64;
            let linhas: Vec<u32> = (0..b.h).filter(|&y| (0..b.w).any(|x| alfa(x, y) > 128.0)).collect();
            let (a, z) = (linhas[0], *linhas.last().unwrap());
            let terco = (z - a) / 3;
            let centro = |ys: std::ops::Range<u32>| {
                let (mut s, mut n) = (0.0, 0.0);
                for y in ys {
                    for x in 0..b.w {
                        let v = alfa(x, y);
                        (s, n) = (s + v * x as f64, n + v);
                    }
                }
                s / n
            };
            centro(a..a + terco) - centro(z - terco..z)
        };
        for negrito in [false, true] {
            let mut e = estilo(Fonte::Mono, None, None);
            e.negrito = negrito;
            let reto = sprite_de("IIII", e.clone());
            e.italico = true;
            let italico = sprite_de("IIII", e);
            assert_ne!(reto.px, italico.px, "negrito {negrito}");
            assert!(inclinacao(&reto).abs() < 0.5, "reto inclinado: {}", inclinacao(&reto));
            assert!(inclinacao(&italico) > 3.0, "itálico reto (negrito {negrito}): {}", inclinacao(&italico));
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

    // ---------------------------------------------------------------- Fase 4: sombra e faixa (texto livre)

    fn sombra(dx: f64, dy: f64, desfoque: f64) -> SombraTexto {
        SombraTexto { cor: "#000000".into(), dx, dy, desfoque }
    }

    fn rgba(b: &Bitmap, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * b.w + x) * 4) as usize;
        [b.px[i], b.px[i + 1], b.px[i + 2], b.px[i + 3]]
    }

    /// O modelo de antes da Fase 4, para provar que o de agora desenha igual quando não há sombra nem faixa.
    const MODELO_ANTES_DA_FASE4: &str = r##"
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

    /// Sem sombra e sem faixa, o modelo novo dá os MESMOS bytes do de antes (projeto antigo, composto igual):
    /// fontes, caixa, contorno, alinhamentos, vários blocos e quebra de linha.
    #[test]
    fn sem_sombra_nem_faixa_desenha_como_antes() {
        let mut casos = Vec::new();
        for fonte in [Fonte::Sans, Fonte::Serifa, Fonte::Mono] {
            casos.push(pedido("Câmera 2 — 14/03/2026", estilo(fonte, Some("#000000a0"), Some("#000000")), 1.5));
            casos.push(pedido("linha um\nlinha dois mais longa", estilo(fonte, None, Some("#ff0000")), 0.3));
        }
        let mut e = estilo(Fonte::Sans, Some("#00000080"), None);
        e.alinhamento = crate::edicao::modelo::Alinhamento::Dir;
        e.negrito = true;
        let mut dois = pedido("Título", e.clone(), 0.8);
        let mut b2 = dois.blocos[0].clone();
        b2.conteudo = "subtítulo bem mais comprido".into();
        b2.estilo.alinhamento = crate::edicao::modelo::Alinhamento::Esq;
        dois.blocos.push(b2);
        casos.push(dois);
        let mut sem_fundo = pedido("sem fundo", e, 1.5);
        sem_fundo.fundo = false;
        casos.push(sem_fundo);
        for p in casos {
            let antigo = {
                let blocos: Vec<serde_json::Value> = p.blocos.iter().map(|b| serde_json::json!({ "conteudo": b.conteudo, "estilo": b.estilo })).collect();
                let altura = p.altura_canvas as f64;
                let dados = serde_json::json!({ "altura": altura, "largura": p.largura_max * altura, "fundo": p.fundo, "blocos": blocos });
                rasterizar_modelo(MODELO_ANTES_DA_FASE4, &dados).unwrap()
            };
            assert_eq!(bitmap_sprite(&p).unwrap().0, antigo, "{:?}", p.blocos[0].conteudo);
        }
    }

    /// Sombra: o texto por cima (todo pixel opaco do texto sozinho sai igual, deslocado pela folga), a sombra embaixo,
    /// deslocada (dy) e desfocada (degradê de alfa, mesmo alcance dos dois lados no eixo sem deslocamento); a tela cresce
    /// a folga dos dois lados. Com caixa, a sombra fica entre a caixa e o texto.
    #[test]
    fn sombra_por_baixo_e_desfocada() {
        let mut e = estilo(Fonte::Sans, None, None);
        e.negrito = true;
        e.tamanho = 0.08;
        let sem = sprite_de("Teste", e.clone());
        let s = sombra(0.0, 0.004, 0.003);
        e.sombra = Some(s.clone());
        let com = sprite_de("Teste", e.clone());
        let (fx, fy) = folga_da_sombra(&s, 1080);
        let r: u32 = raios_do_desfoque(0.003 * 1080.0).iter().sum();
        assert_eq!((fx, fy), (r, 4 + r));
        assert!(r >= 6, "desfoque de 3,2 px de desvio: raios {r}");
        assert_eq!((com.w, com.h), (sem.w + 2 * fx, sem.h + 2 * fy));
        let mut opacos = 0;
        let mut cobertos = vec![false; (com.w * com.h) as usize];
        for y in 0..sem.h {
            for x in 0..sem.w {
                if alfa(&sem, x, y) > 0 {
                    cobertos[((y + fy) * com.w + x + fx) as usize] = true;
                }
                if alfa(&sem, x, y) == 255 {
                    opacos += 1;
                    assert_eq!(rgba(&com, x + fx, y + fy), [255, 255, 255, 255], "texto por baixo da sombra em ({x}, {y})");
                }
            }
        }
        assert!(opacos > 1000);
        // só sombra: fora do texto; preta, alfa em degradê
        let (mut alfas, mut linhas, mut colunas) = (std::collections::BTreeSet::new(), (u32::MAX, 0u32), (u32::MAX, 0u32));
        for y in 0..com.h {
            for x in 0..com.w {
                let p = rgba(&com, x, y);
                if p[3] > 0 && !cobertos[(y * com.w + x) as usize] {
                    assert_eq!(&p[..3], &[0, 0, 0], "sombra preta em ({x}, {y})");
                    alfas.insert(p[3]);
                    (linhas, colunas) = ((linhas.0.min(y), linhas.1.max(y)), (colunas.0.min(x), colunas.1.max(x)));
                }
            }
        }
        assert!(alfas.len() >= 40, "desfocada: {} níveis de alfa", alfas.len());
        // alcance da sombra além do texto: acima = r − dy, abaixo = r + dy; esquerda = direita = r (a 1 px)
        let tinta = |b: &Bitmap| {
            let (mut l, mut c) = ((u32::MAX, 0u32), (u32::MAX, 0u32));
            for y in 0..b.h {
                for x in 0..b.w {
                    if alfa(b, x, y) > 0 {
                        (l, c) = ((l.0.min(y), l.1.max(y)), (c.0.min(x), c.1.max(x)));
                    }
                }
            }
            (l, c)
        };
        let (lt, ct) = tinta(&sem);
        // (a cauda do desfoque some no arredondamento a 8 bits conforme a tinta da borda: folga de 2 px)
        let (acima, abaixo) = ((lt.0 + fy) as i64 - linhas.0 as i64, linhas.1 as i64 - (lt.1 + fy) as i64);
        let (esq, dir) = ((ct.0 + fx) as i64 - colunas.0 as i64, colunas.1 as i64 - (ct.1 + fx) as i64);
        assert!((abaixo - acima - 8).abs() <= 2, "deslocada 4 px para baixo: {acima} acima, {abaixo} abaixo");
        assert!((esq - dir).abs() <= 2, "sem deslocamento no x: {esq} × {dir}");
        assert!(esq <= r as i64 && esq >= r as i64 - 3, "alcance do desfoque {esq} × raios {r}");
        // com caixa azul: a caixa fica, a sombra escurece a caixa embaixo do texto, o texto fica por cima
        let mut e2 = e.clone();
        e2.caixa = Some(CaixaTexto { cor: "#0000ff".into(), margem: 0.02, raio: 0.0, faixa: false });
        let sem2 = sprite_de("Teste", EstiloTexto { sombra: None, ..e2.clone() });
        let com2 = sprite_de("Teste", e2);
        assert_eq!(rgba(&com2, fx + 1, fy + 1), [0, 0, 255, 255], "canto da caixa");
        let mut escurecidos = 0;
        for y in 0..sem2.h {
            for x in 0..sem2.w {
                let (a, b) = (rgba(&sem2, x, y), rgba(&com2, x + fx, y + fy));
                if a == [255, 255, 255, 255] {
                    assert_eq!(b, a, "texto em ({x}, {y})");
                } else if a == [0, 0, 255, 255] && b[3] == 255 && b[2] < 200 && b[0] == 0 && b[1] == 0 {
                    escurecidos += 1;
                }
            }
        }
        assert!(escurecidos > 200, "sombra sobre a caixa: {escurecidos}");
        // fora da caixa nada (a sombra é do texto e a caixa a cobre com folga)
        assert_eq!(rgba(&com2, 0, 0)[3], 0);
    }

    /// Faixa: a caixa vai de uma borda à outra do bloco (`largura_max`), com o texto curto ou longo; o alinhamento
    /// põe o texto dentro dela. Sem faixa, a caixa é justa ao texto.
    #[test]
    fn faixa_na_largura_do_bloco() {
        let faixa = |alinhamento: crate::edicao::modelo::Alinhamento| {
            let mut e = estilo(Fonte::Sans, Some("#000000a0"), None);
            e.caixa.as_mut().unwrap().faixa = true;
            e.caixa.as_mut().unwrap().raio = 0.0;
            e.alinhamento = alinhamento;
            e
        };
        use crate::edicao::modelo::Alinhamento::{Centro, Dir, Esq};
        // 0,6 da altura + 2 margens de 0,01: 669,6 → 670 px
        let esperada = (0.6f64 * 1080.0 + 2.0 * 0.01 * 1080.0).ceil() as u32;
        let curto = bitmap_sprite(&pedido("A", faixa(Centro), 0.6)).unwrap().0;
        let longo = bitmap_sprite(&pedido("o veículo entra no cruzamento", faixa(Centro), 0.6)).unwrap().0;
        assert_eq!((curto.w, longo.w), (esperada, esperada));
        let justa = bitmap_sprite(&pedido("A", estilo(Fonte::Sans, Some("#000000a0"), None), 0.6)).unwrap().0;
        assert!(justa.w < esperada / 4, "sem faixa a caixa é justa: {}", justa.w);
        // a caixa cobre a largura inteira na linha do meio (fora as colunas de borda, cobertas em parte)
        for b in [&curto, &longo] {
            let y = b.h / 2;
            for x in 1..b.w - 1 {
                let p = rgba(b, x, y);
                assert!(p[3] >= 0xa0, "coluna {x}: {p:?}");
            }
        }
        // o alinhamento põe a tinta do texto na esquerda, no meio ou na direita da faixa
        let centro_da_tinta = |b: &Bitmap| {
            let (mut s, mut n) = (0.0, 0.0);
            for y in 0..b.h {
                for x in 0..b.w {
                    let p = rgba(b, x, y);
                    if p[0] > 200 {
                        (s, n) = (s + x as f64, n + 1.0);
                    }
                }
            }
            s / n / b.w as f64
        };
        let (e, c, d) = (
            centro_da_tinta(&bitmap_sprite(&pedido("Texto", faixa(Esq), 0.6)).unwrap().0),
            centro_da_tinta(&curto),
            centro_da_tinta(&bitmap_sprite(&pedido("Texto", faixa(Dir), 0.6)).unwrap().0),
        );
        assert!(e < 0.2 && (c - 0.5).abs() < 0.05 && d > 0.8, "esq {e:.3}, centro {c:.3}, dir {d:.3}");
    }

    /// Palavra mais larga que a caixa: quebra dentro da caixa (a tinta fica dentro das margens), em vez de sair
    /// cortada na borda do sprite como no modelo de antes.
    #[test]
    fn palavra_longa_quebra_dentro_da_caixa() {
        let e = estilo(Fonte::Sans, Some("#000000"), None);
        let margem = (0.01f64 * 1080.0) as u32;
        let tinta_x = |b: &Bitmap| {
            let (mut a, mut z) = (u32::MAX, 0u32);
            for y in 0..b.h {
                for x in 0..b.w {
                    if rgba(b, x, y)[0] > 100 {
                        (a, z) = (a.min(x), z.max(x));
                    }
                }
            }
            (a, z)
        };
        let p = pedido("Inconstitucionalissimamente atravessou", e.clone(), 0.2);
        let novo = bitmap_sprite(&p).unwrap().0;
        let (a, z) = tinta_x(&novo);
        assert!(a >= margem - 1 && z <= novo.w - margem, "tinta {a}..{z} em {} px", novo.w);
        let antigo = {
            let blocos: Vec<serde_json::Value> = p.blocos.iter().map(|b| serde_json::json!({ "conteudo": b.conteudo, "estilo": b.estilo })).collect();
            let dados = serde_json::json!({ "altura": 1080.0, "largura": 0.2 * 1080.0, "fundo": true, "blocos": blocos });
            rasterizar_modelo(MODELO_ANTES_DA_FASE4, &dados).unwrap()
        };
        let (a0, z0) = tinta_x(&antigo);
        assert!(a0 < margem - 1 || z0 > antigo.w - margem, "o modelo de antes cortava: {a0}..{z0}");
        assert!(novo.h > antigo.h, "a palavra ocupa mais linhas: {} × {}", novo.h, antigo.h);
        // acentos e cedilha inteiros na quebra (graphemes)
        let (b, trocados) = bitmap_sprite(&pedido("ÇÃÉÍÕÚçãéíõúÇÃÉÍÕÚçãéíõú", e, 0.1)).unwrap();
        assert!(trocados.is_empty() && b.h > 0);
    }

    /// Mesmos bytes com sombra, faixa e contorno: duas vezes (cache do Typst limpo entre elas) e em 4 threads.
    #[test]
    fn mesmos_bytes() {
        let mut e = estilo(Fonte::Serifa, Some("#000000a0"), Some("#202020"));
        e.caixa.as_mut().unwrap().faixa = true;
        e.sombra = Some(SombraTexto { cor: "#000000b3".into(), dx: 0.002, dy: 0.003, desfoque: 0.004 });
        let p = pedido("Policiais chegam ao estabelecimento", e, 0.7);
        let a = bitmap_sprite(&p).unwrap().0.png().unwrap();
        typst::comemo::evict(0);
        assert_eq!(bitmap_sprite(&p).unwrap().0.png().unwrap(), a);
        let outros: Vec<Vec<u8>> = std::thread::scope(|s| {
            let h: Vec<_> = (0..4).map(|_| s.spawn(|| bitmap_sprite(&p).unwrap().0.png().unwrap())).collect();
            h.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert!(outros.iter().all(|o| *o == a));
        // a sombra entra na chave do cache: outro deslocamento, outro arquivo
        let mut q = p.clone();
        q.blocos[0].estilo.sombra.as_mut().unwrap().dy = 0.004;
        assert_ne!(chave_sprite(&q).unwrap(), chave_sprite(&p).unwrap());
        let mut sem_faixa = p.clone();
        sem_faixa.blocos[0].estilo.caixa.as_mut().unwrap().faixa = false;
        assert_ne!(chave_sprite(&sem_faixa).unwrap(), chave_sprite(&p).unwrap());
    }

    /// Desfoque de um quadrado de 5 px: simétrico nos dois eixos, alcance = meio lado + soma dos raios, massa
    /// conservada (a 2 %), pico no centro; a variância das 3 caixas fica perto de σ² e a medida bate com ela.
    #[test]
    fn desfoque_de_um_ponto() {
        let mut quadrado = tiny_skia::Pixmap::new(5, 5).unwrap();
        quadrado.pixels_mut().fill(tiny_skia::PremultipliedColorU8::from_rgba(255, 255, 255, 255).unwrap());
        for sigma in [1.0, 3.24, 6.0] {
            let raios = raios_do_desfoque(sigma);
            let r: u32 = raios.iter().sum();
            let n = 2 * r + 15;
            let a = alfa_desfocado(&quadrado, n / 2 - 2, n / 2 - 2, n, n, raios);
            let em = |x: u32, y: u32| a[(y * n + x) as usize];
            let soma: u32 = a.iter().map(|&v| v as u32).sum();
            assert!((soma as f64 - 25.0 * 255.0).abs() < 25.0 * 255.0 * 0.02, "σ {sigma}: massa {soma}");
            for y in 0..n {
                for x in 0..n {
                    assert_eq!(em(x, y), em(n - 1 - x, y), "σ {sigma}");
                    assert_eq!(em(x, y), em(y, x), "σ {sigma}");
                    if em(x, y) > 0 {
                        assert!(x.abs_diff(n / 2) <= r + 2 && y.abs_diff(n / 2) <= r + 2, "σ {sigma}: ({x}, {y}) além do alcance {r}");
                    }
                }
            }
            assert_eq!(a.iter().max(), Some(&em(n / 2, n / 2)));
            // variância das caixas ((2r+1)² − 1)/12 somadas: perto de σ²; a medida = ela + a do quadrado (2)
            let das_caixas: f64 = raios.iter().map(|&r| (((2 * r + 1) * (2 * r + 1)) as f64 - 1.0) / 12.0).sum();
            assert!((das_caixas.sqrt() - sigma).abs() < 0.2 * sigma, "σ {sigma}: caixas dão {}", das_caixas.sqrt());
            let var: f64 = (0..n).map(|x| (x as f64 - (n / 2) as f64).powi(2) * (0..n).map(|y| em(x, y) as f64).sum::<f64>()).sum::<f64>() / soma as f64;
            assert!((var - (das_caixas + 2.0)).abs() < 0.05 * (das_caixas + 2.0), "σ {sigma}: variância {var} × {}", das_caixas + 2.0);
        }
        assert_eq!(raios_do_desfoque(0.0), [0, 0, 0]);
        assert_eq!(raios_do_desfoque(f64::NAN), [0, 0, 0]);
    }

    /// Dois blocos com sombras diferentes e um sem: cada grupo com a sua cor, nada some.
    #[test]
    fn sombras_por_bloco() {
        let mut e = estilo(Fonte::Sans, None, None);
        e.negrito = true;
        let mut p = pedido("AAA", e.clone(), 1.5);
        let mut b2 = p.blocos[0].clone();
        b2.estilo.sombra = Some(SombraTexto { cor: "#ff0000".into(), dx: 0.004, dy: 0.0, desfoque: 0.0 });
        let mut b3 = p.blocos[0].clone();
        b3.estilo.sombra = Some(SombraTexto { cor: "#0000ff".into(), dx: 0.0, dy: 0.004, desfoque: 0.001 });
        p.blocos.push(b2);
        p.blocos.push(b3);
        let b = bitmap_sprite(&p).unwrap().0;
        let px: Vec<[u8; 4]> = (0..b.h).flat_map(|y| (0..b.w).map(move |x| (x, y))).map(|(x, y)| rgba(&b, x, y)).collect();
        assert!(px.iter().any(|p| p[3] == 255 && p[0] > 200 && p[1] < 40 && p[2] < 40), "sombra vermelha");
        assert!(px.iter().any(|p| p[3] > 100 && p[2] > 200 && p[0] < 40 && p[1] < 40), "sombra azul");
        // o bloco de cima, sem sombra, não ganhou nenhuma: só branco no terço de cima
        let terco = b.h / 3;
        assert!((0..terco).all(|y| (0..b.w).all(|x| { let p = rgba(&b, x, y); p[3] == 0 || p[..3] == [255, 255, 255] })));
    }

    /// Sombra fora da faixa (deslocamento ou desfoque absurdo, desfoque negativo): `item_invalido` antes de alocar;
    /// o atlas do carimbo recusa sombra e faixa (não desenharia); `recusa_estilo` não recusa mais nada.
    #[test]
    fn sombra_fora_da_faixa_e_atlas() {
        let codigo_de = |e: SicroError| e.to_string();
        for s in [sombra(0.06, 0.0, 0.0), sombra(0.0, -0.06, 0.0), sombra(0.0, 0.0, 0.051), sombra(0.0, 0.0, -0.001), sombra(f64::NAN, 0.0, 0.0)] {
            assert!(!sombra_ok(&s));
            let mut e = estilo(Fonte::Sans, None, None);
            e.sombra = Some(s.clone());
            assert!(recusa_estilo(&e).is_none());
            let m = codigo_de(bitmap_sprite(&pedido("abc", e, 1.5)).unwrap_err());
            assert!(m.contains(&format!("{}: sombra", codigo::ITEM_INVALIDO)), "{s:?}: {m}");
        }
        assert!(sombra_ok(&sombra(SOMBRA_MAX, -SOMBRA_MAX, SOMBRA_MAX)) && sombra_ok(&sombra(0.0, 0.0, 0.0)));
        let mut e = estilo(Fonte::Sans, None, None);
        e.sombra = Some(sombra(0.0, 0.003, 0.002));
        assert!(codigo_de(bitmap_atlas(&e, 1080).unwrap_err()).contains(&format!("{}: sombra", codigo::NAO_SUPORTADO)));
        let mut e = estilo(Fonte::Sans, Some("#000000"), None);
        e.caixa.as_mut().unwrap().faixa = true;
        assert!(recusa_estilo(&e).is_none());
        assert!(codigo_de(bitmap_atlas(&e, 1080).unwrap_err()).contains(&format!("{}: faixa", codigo::NAO_SUPORTADO)));
        // cor da sombra fora do modelo: código `cor`
        let mut e = estilo(Fonte::Sans, None, None);
        e.sombra = Some(SombraTexto { cor: "black".into(), dx: 0.0, dy: 0.0, desfoque: 0.0 });
        assert!(matches!(bitmap_sprite(&pedido("abc", e, 1.5)), Err(SicroError::Validation(m)) if m == codigo::COR));
    }

    /// Espelho da folga (o palco põe as alças na caixa visível, sem a folga da sombra): `texto-folga-sombra.json`.
    #[test]
    fn fixture_folga_da_sombra() {
        let mut casos = Vec::new();
        for (dx, dy, desfoque) in [(0.0, 0.003, 0.004), (0.0, 0.0, 0.0), (-0.002, 0.001, 0.0005), (0.05, -0.05, 0.05), (0.0013, 0.0021, 0.0017), (0.0, 0.0, 0.0001)] {
            for altura in [180u32, 360, 720, 1080, 2160] {
                let s = sombra(dx, dy, desfoque);
                let (x, y) = folga_da_sombra(&s, altura);
                casos.push(serde_json::json!({ "nome": format!("{dx} {dy} {desfoque} em {altura}"), "entrada": { "sombra": s, "altura_canvas": altura },
                    "saida": { "x": x, "y": y, "raios": raios_do_desfoque(desfoque * altura as f64) } }));
            }
        }
        // desvios na fronteira das larguras das caixas (o arredondamento de m)
        for sigma in [0.5, 0.9, 1.0, 1.5, 2.0, 2.5, 3.0, 4.32, 7.0, 10.0, 54.0] {
            casos.push(serde_json::json!({ "nome": format!("desvio {sigma}"), "entrada": { "sombra": sombra(0.0, 0.0, sigma / 1080.0), "altura_canvas": 1080 },
                "saida": { "x": folga_da_sombra(&sombra(0.0, 0.0, sigma / 1080.0), 1080).0, "y": folga_da_sombra(&sombra(0.0, 0.0, sigma / 1080.0), 1080).1,
                           "raios": raios_do_desfoque(sigma / 1080.0 * 1080.0) } }));
        }
        crate::edicao::curva::fixtures::conferir("texto-folga-sombra.json", serde_json::Value::Array(casos));
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
