//! DOCX → SicroDoc importer (POC — passo 1: texto).
//!
//! Caminho INVERSO do `exporters/docx.rs`: lê um `.docx` (ZIP + OOXML) e
//! produz o `content` ProseMirror/TipTap do `.sicrodoc`, em MELHOR ESFORÇO.
//! NÃO é fidelidade pixel-a-pixel — é uma conversão para conteúdo EDITÁVEL
//! que o perito ajusta (ideal para reaproveitar laudos-modelo do Word).
//!
//! Cobertura do passo 1 (validado nos laudos reais do perito):
//!   - parágrafos + alinhamento (`w:jc`);
//!   - marcas inline: negrito, itálico, sublinhado, tachado, sob/sobrescrito,
//!     cor, tamanho de fonte (`w:sz`, meio-pontos), família (`w:rFonts`);
//!   - títulos por estilo (`Heading1..3` / `Ttulo1..3`) → heading level;
//!   - legendas (`Legenda`/`Caption`) → `laudoStyle: "legenda"`;
//!   - listas (`w:numPr`) agrupadas em bulletList/orderedList (numFmt do
//!     `numbering.xml`);
//!   - margens da página (`w:sectPr/w:pgMar`, twips → cm);
//!   - campos `SEQ` etc.: o código do campo (`w:instrText`) é ignorado, o
//!     texto visível é preservado.
//!
//! FORA do passo 1 (placeholder por enquanto):
//!   - IMAGENS (`w:drawing`): viram um parágrafo-placeholder; a extração real
//!     entra no passo 2.
//!
//! Tabelas (`w:tbl`): convertidas para o nó `table` do SICRO (corpo E
//! cabeçalho). Isto preserva o "bloco de registro" do cabeçalho (Registrado
//! em / LAUDO Nº / Folha), que no Word costuma ser uma tabela/retângulo com
//! 3 colunas. `gridSpan` → colspan; células de continuação de `vMerge` são
//! puladas (rowspan simplificado).
//!
//! Caixa de texto VERTICAL do cabeçalho (marca lateral, ex: "POLÍCIA
//! CIENTÍFICA…"): detectada via `bodyPr vert="vert270"` (DrawingML) ou
//! `mso-layout-flow-alt:bottom-to-top` (VML) e reconstruída como nó `text_box`
//! do SICRO com `text_orientation` vertical, sem borda/preenchimento. Posição
//! e altura são best-effort (ajustáveis arrastando) — o objetivo é trazer o
//! TEXTO com a ORIENTAÇÃO certa, não a geometria exata.

use std::collections::HashMap;
use std::io::{Read, Seek};
use std::path::Path;

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::error::{Result, SicroError};

/// 1 cm em twips (twentieths of a point). 1 in = 1440 twips = 2.54 cm.
const TWIPS_PER_CM: f64 = 1440.0 / 2.54;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct DocxMargins {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct ImportStats {
    pub blocks: usize,
    pub paragraphs: usize,
    pub headings: usize,
    pub list_items: usize,
    pub images: usize,
    pub tables: usize,
    /// Caixas de texto verticais reconstruídas no cabeçalho (marca lateral).
    pub textboxes: usize,
    pub header_paragraphs: usize,
    /// Blocos top-level extraídos do rodapé (parágrafos + tabelas + figuras).
    pub footer_paragraphs: usize,
    /// Fórmulas matemáticas (OMML) convertidas em nós math (inline + bloco).
    pub math: usize,
}

#[derive(Debug)]
pub struct ParsedDocx {
    /// ProseMirror doc JSON (`{ "type": "doc", "content": [...] }`).
    pub content: Value,
    /// Conteúdo do cabeçalho (`word/header*.xml`) como doc ProseMirror,
    /// quando existe e tem texto. `None` ⇒ documento sem cabeçalho.
    pub header_content: Option<Value>,
    /// Conteúdo do rodapé (`word/footer*.xml`), quando existe e tem conteúdo.
    /// `None` ⇒ documento sem rodapé. Traz o brasão da PC que vive no rodapé.
    pub footer_content: Option<Value>,
    /// Margens da página em cm, quando o `.docx` as define.
    pub margins: Option<DocxMargins>,
    /// Altura ESTIMADA (cm) do conteúdo do cabeçalho do `.docx`. Mesmo sem
    /// importar o header, o consumer usa isto pra RESERVAR a margem de topo —
    /// o Word usa `w:top` pequeno e conta com o conteúdo do header pra empurrar
    /// o corpo; ao largar o header, o corpo "subiria" sem essa reserva.
    pub header_reserve_cm: Option<f64>,
    /// Imagens extraídas do `.docx` (corpo + cabeçalho). Os nós `figure`
    /// referenciam cada imagem por `relative_path = "__docximport__/<name>"`;
    /// o comando de import grava os bytes no workspace e reescreve o
    /// `relative_path` pro caminho final. Só formatos web (png/jpg/gif/bmp).
    pub images: Vec<ExtractedImage>,
    pub stats: ImportStats,
}

/// Uma imagem extraída do ZIP do `.docx`, pronta pra ser gravada no workspace.
#[derive(Debug, Clone)]
pub struct ExtractedImage {
    /// Nome-chave estável (ex: "img1.png"), casado com o sufixo do
    /// `relative_path` temporário do `figure` (`__docximport__/img1.png`).
    pub name: String,
    pub bytes: Vec<u8>,
    pub ext: String,
}

/// Prefixo do `relative_path` temporário emitido nos `figure` durante o
/// parse — o comando de import o reescreve pro caminho final no workspace.
pub const DOCX_IMPORT_PREFIX: &str = "__docximport__/";

/// Imagem pendente coletada durante o parse do XML (resolvida via rels),
/// lida do ZIP só DEPOIS (quando o `&mut zip` está livre de novo).
struct PendingImage {
    name: String,
    zip_path: String,
    ext: String,
}

/// Parse a `.docx` from disk.
pub fn parse_docx_file(path: &Path) -> Result<ParsedDocx> {
    let bytes = std::fs::read(path)
        .map_err(|e| SicroError::Filesystem(format!("não consegui ler o .docx: {e}")))?;
    parse_docx_bytes(&bytes)
}

/// Parse a `.docx` from its raw bytes (testável sem filesystem).
pub fn parse_docx_bytes(bytes: &[u8]) -> Result<ParsedDocx> {
    let reader = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(reader)
        .map_err(|e| SicroError::Validation(format!("arquivo não é um .docx válido (ZIP): {e}")))?;

    let document_xml = read_entry_string(&mut zip, "word/document.xml")
        .ok_or_else(|| SicroError::Validation("docx sem word/document.xml".to_string()))?;
    let numbering_xml = read_entry_string(&mut zip, "word/numbering.xml");
    let rels_xml =
        read_entry_string(&mut zip, "word/_rels/document.xml.rels").unwrap_or_default();

    let numbering = parse_numbering(numbering_xml.as_deref());
    let rels = parse_rels(&rels_xml);

    // Remove BOM eventual antes do parser de XML.
    let xml = document_xml.trim_start_matches('\u{feff}');
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| SicroError::Validation(format!("document.xml inválido: {e}")))?;
    let root = doc.root_element();
    let body = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "body")
        .ok_or_else(|| SicroError::Validation("document.xml sem <body>".to_string()))?;

    let mut stats = ImportStats::default();
    let mut imgs: Vec<PendingImage> = Vec::new();
    let blocks = walk_body(body, &numbering, &rels, &mut imgs, &mut stats);
    stats.blocks = blocks.len();
    let margins = find_margins(body);
    // Mesmo com o cabeçalho DESLIGADO na importação, estimamos a altura do seu
    // conteúdo pra reservar a margem de topo (senão o corpo "sobe": o .docx
    // costuma ter `w:top` pequeno e contar com o header pra empurrar o corpo).
    let header_reserve_cm = estimate_header_reserve_cm(&mut zip, body, &rels);

    // ── Cabeçalho e rodapé: importação DESLIGADA por ora (pedido do perito —
    // o documento importado entra "limpo", SEM o timbre/brasão do .docx). O
    // parse fica atrás do flag (em vez de apagado) pra reativar ser de uma
    // linha: troque `IMPORT_CHROME` para `true`. Com `false`, nem o brasão é
    // extraído (sem imagem órfã). O consumer já trata `None` (header e footer
    // ficam desligados).
    const IMPORT_CHROME: bool = false;

    // Cabeçalho: resolve o header default via `sectPr/headerReference` → rels.
    // Pra resolver as imagens do cabeçalho (brasão), lê o rels PRÓPRIO do
    // header (`word/_rels/headerN.xml.rels`).
    let header_content = if IMPORT_CHROME {
        let header_target =
            find_header_target(body, &rels).unwrap_or_else(|| "word/header1.xml".to_string());
        let header_xml = read_entry_string(&mut zip, &header_target);
        let header_rels_xml =
            read_entry_string(&mut zip, &rels_path_for(&header_target)).unwrap_or_default();
        let header_rels = parse_rels(&header_rels_xml);
        header_xml.and_then(|hx| {
            parse_header_xml(&hx, &numbering, &header_rels, &mut imgs, &mut stats)
        })
    } else {
        None
    };

    // W — Rodapé (footer): mesma mecânica. Resolve `sectPr/footerReference`
    // → footerN.xml + o rels PRÓPRIO do footer (pro brasão da PC).
    let footer_content = if IMPORT_CHROME {
        find_footer_target(body, &rels).and_then(|ft| {
            let fx = read_entry_string(&mut zip, &ft)?;
            let frels_xml =
                read_entry_string(&mut zip, &rels_path_for(&ft)).unwrap_or_default();
            let frels = parse_rels(&frels_xml);
            parse_footer_xml(&fx, &numbering, &frels, &mut imgs, &mut stats)
        })
    } else {
        None
    };

    // Lê os bytes das mídias pendentes do ZIP (agora que o parse de XML
    // terminou e o `&mut zip` voltou a estar livre). Mídias não-encontradas
    // são ignoradas — o figure correspondente fica com o path placeholder.
    let mut images: Vec<ExtractedImage> = Vec::new();
    for pend in &imgs {
        if let Some(bytes) = read_entry_bytes(&mut zip, &pend.zip_path) {
            if !bytes.is_empty() {
                images.push(ExtractedImage {
                    name: pend.name.clone(),
                    bytes,
                    ext: pend.ext.clone(),
                });
            }
        }
    }

    Ok(ParsedDocx {
        content: json!({ "type": "doc", "content": blocks }),
        header_content,
        footer_content,
        margins,
        header_reserve_cm,
        images,
        stats,
    })
}

/// `"word/header1.xml"` → `"word/_rels/header1.xml.rels"`.
fn rels_path_for(part: &str) -> String {
    match part.rfind('/') {
        Some(i) => format!("{}/_rels/{}.rels", &part[..i], &part[i + 1..]),
        None => format!("_rels/{part}.rels"),
    }
}

// ---------------------------------------------------------------------------
// ZIP helpers

fn read_entry_string<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Option<String> {
    let mut f = zip.by_name(name).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

/// Lê uma entrada binária do ZIP (mídia: png/jpg/…).
fn read_entry_bytes<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name).ok()?;
    let mut v = Vec::new();
    f.read_to_end(&mut v).ok()?;
    Some(v)
}

// ---------------------------------------------------------------------------
// roxmltree helpers (namespace-agnostic: comparam o LOCAL name, já que
// `has_tag_name("p")` / `attribute("val")` falham com o namespace `w:`).

fn child<'a, 'i>(
    n: roxmltree::Node<'a, 'i>,
    local: &str,
) -> Option<roxmltree::Node<'a, 'i>> {
    n.children()
        .find(|c| c.is_element() && c.tag_name().name() == local)
}

fn attr<'a>(n: roxmltree::Node<'a, '_>, local: &str) -> Option<&'a str> {
    n.attributes()
        .find(|a| a.name() == local)
        .map(|a| a.value())
}

// ---------------------------------------------------------------------------
// Body walk + agrupamento de listas

enum Para {
    Block(Value),
    ListItem { node: Value },
}

fn walk_body(
    body: roxmltree::Node,
    numbering: &Numbering,
    rels: &HashMap<String, String>,
    imgs: &mut Vec<PendingImage>,
    stats: &mut ImportStats,
) -> Vec<Value> {
    let mut blocks: Vec<Value> = Vec::new();
    let mut pending: Vec<Value> = Vec::new();
    // Contadores da numeração multinível, em ordem de documento.
    let mut counters: Counters = HashMap::new();

    let flush = |blocks: &mut Vec<Value>, pending: &mut Vec<Value>| {
        if !pending.is_empty() {
            let items = std::mem::take(pending);
            blocks.push(json!({ "type": "bulletList", "content": items }));
        }
    };

    for node in body.children().filter(|n| n.is_element()) {
        match node.tag_name().name() {
            "p" => {
                match convert_paragraph(node, numbering, &mut counters, rels, imgs, stats)
                {
                    // Só BULLETS agrupam em bulletList; numerados viram Block
                    // ("baked" com o número como texto).
                    Para::ListItem { node } => pending.push(node),
                    Para::Block(node) => {
                        flush(&mut blocks, &mut pending);
                        blocks.push(node);
                    }
                }
            }
            // Tabela → nó `table` do SICRO (preserva estrutura de linhas/
            // colunas em vez de achatar). Ver `convert_table`.
            "tbl" => {
                flush(&mut blocks, &mut pending);
                blocks.push(convert_table(
                    node, numbering, &mut counters, rels, imgs, stats,
                ));
            }
            _ => {}
        }
    }
    flush(&mut blocks, &mut pending);

    let mut blocks = merge_legends_into_figures(blocks);
    if blocks.is_empty() {
        blocks.push(json!({ "type": "paragraph" }));
    }
    blocks
}

/// Pós-processo: uma LEGENDA (`laudoStyle: "legenda"`) logo APÓS uma figura vira
/// o `figcaption` daquela figura — senão fica a legenda AUTO do SICRO
/// ("Figura N —") + a legenda original do Word ("Figura 1 - …") = legenda
/// DUPLA. Remove também o prefixo redundante "Figura N -" do texto (o SICRO já
/// numera). Best-effort: se não casar o padrão, mantém o texto como veio.
fn merge_legends_into_figures(blocks: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(blocks.len());
    let mut i = 0;
    while i < blocks.len() {
        let is_figure =
            blocks[i].get("type").and_then(|t| t.as_str()) == Some("figure");
        if is_figure {
            if let Some(next) = blocks.get(i + 1) {
                if is_legend_paragraph(next) {
                    let caption = strip_figure_label_prefix(&plain_text_of(next));
                    let mut fig = blocks[i].clone();
                    set_figcaption_text(&mut fig, &caption);
                    out.push(fig);
                    i += 2; // consumiu figura + legenda
                    continue;
                }
            }
        }
        out.push(blocks[i].clone());
        i += 1;
    }
    out
}

/// `paragraph` com `attrs.laudoStyle == "legenda"`.
fn is_legend_paragraph(v: &Value) -> bool {
    v.get("type").and_then(|t| t.as_str()) == Some("paragraph")
        && v.get("attrs")
            .and_then(|a| a.get("laudoStyle"))
            .and_then(|s| s.as_str())
            == Some("legenda")
}

/// Concatena o texto dos nós `text` diretos de um bloco.
fn plain_text_of(v: &Value) -> String {
    let mut s = String::new();
    if let Some(content) = v.get("content").and_then(|c| c.as_array()) {
        for node in content {
            if node.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(t) = node.get("text").and_then(|t| t.as_str()) {
                    s.push_str(t);
                }
            }
        }
    }
    s
}

/// Remove um prefixo "Figura N - " / "Fig. N — " (etc.) do início. Se não
/// houver número após "Figura"/"Fig", ou se sobrar vazio, devolve o texto
/// original (não arrisca perder a legenda).
fn strip_figure_label_prefix(s: &str) -> String {
    let t = s.trim();
    let lower = t.to_ascii_lowercase();
    let idx = if lower.starts_with("figura") {
        6
    } else if lower.starts_with("fig.") {
        4
    } else if lower.starts_with("fig") {
        3
    } else {
        return t.to_string();
    };
    let rest = t[idx..].trim_start();
    let no_digits = rest.trim_start_matches(|c: char| c.is_ascii_digit());
    if no_digits.len() == rest.len() {
        return t.to_string(); // sem número após "Figura" → não é rótulo
    }
    let after_sep = no_digits
        .trim_start()
        .trim_start_matches(|c: char| matches!(c, '-' | '–' | '—' | ':' | '.'))
        .trim_start();
    if after_sep.is_empty() {
        t.to_string() // só "Figura N" sem descrição → mantém
    } else {
        after_sep.to_string()
    }
}

/// Define o texto do `figcaption` da figura (texto vazio = figcaption sem
/// conteúdo, como o schema exige no mínimo).
fn set_figcaption_text(fig: &mut Value, text: &str) {
    let figcaption = if text.is_empty() {
        json!({ "type": "figcaption" })
    } else {
        json!({ "type": "figcaption", "content": [{ "type": "text", "text": text }] })
    };
    if let Some(obj) = fig.as_object_mut() {
        obj.insert("content".into(), json!([figcaption]));
    }
}

/// Converte um `<w:tbl>` num nó `table` do SICRO (TipTap). Cada `<w:tr>` →
/// `tableRow`; cada `<w:tc>` → `tableCell` com os parágrafos da célula
/// (TipTap exige `block+`, então células vazias ganham um parágrafo vazio).
///   - `w:gridSpan w:val=N`  → `colspan: N`.
///   - `w:vMerge` (sem val ou `="continue"`) → célula de continuação: pulada
///     (rowspan simplificado — passo 1 não reconstrói merges verticais).
/// Bullets dentro de célula são "desembrulhados" pro parágrafo interno
/// (listItem não pode ser filho direto de tableCell).
fn convert_table(
    tbl: roxmltree::Node,
    numbering: &Numbering,
    counters: &mut Counters,
    rels: &HashMap<String, String>,
    imgs: &mut Vec<PendingImage>,
    stats: &mut ImportStats,
) -> Value {
    let mut rows: Vec<Value> = Vec::new();
    for tr in tbl
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "tr")
    {
        let mut cells: Vec<Value> = Vec::new();
        for tc in tr
            .children()
            .filter(|c| c.is_element() && c.tag_name().name() == "tc")
        {
            let tcpr = child(tc, "tcPr");

            // vMerge "continue" → célula coberta pela de cima; não emite.
            if let Some(vm) = tcpr.and_then(|p| child(p, "vMerge")) {
                if attr(vm, "val").unwrap_or("continue") == "continue" {
                    continue;
                }
            }

            let colspan = tcpr
                .and_then(|p| child(p, "gridSpan"))
                .and_then(|g| attr(g, "val"))
                .and_then(|v| v.parse::<i64>().ok())
                .filter(|n| *n > 0)
                .unwrap_or(1);

            let mut content: Vec<Value> = Vec::new();
            for p in tc
                .children()
                .filter(|c| c.is_element() && c.tag_name().name() == "p")
            {
                match convert_paragraph(p, numbering, counters, rels, imgs, stats) {
                    Para::Block(b) => content.push(b),
                    // Bullet em célula: usa o(s) parágrafo(s) internos do
                    // listItem (listItem não é válido direto na célula).
                    Para::ListItem { node } => {
                        if let Some(inner) =
                            node.get("content").and_then(|c| c.as_array())
                        {
                            content.extend(inner.iter().cloned());
                        }
                    }
                }
            }
            if content.is_empty() {
                content.push(json!({ "type": "paragraph" }));
            }

            cells.push(json!({
                "type": "tableCell",
                "attrs": { "colspan": colspan, "rowspan": 1, "colwidth": Value::Null },
                "content": content,
            }));
        }
        if !cells.is_empty() {
            rows.push(json!({ "type": "tableRow", "content": cells }));
        }
    }

    if rows.is_empty() {
        return json!({ "type": "paragraph" });
    }
    stats.tables += 1;
    json!({ "type": "table", "content": rows })
}

fn convert_paragraph(
    p: roxmltree::Node,
    numbering: &Numbering,
    counters: &mut Counters,
    rels: &HashMap<String, String>,
    imgs: &mut Vec<PendingImage>,
    stats: &mut ImportStats,
) -> Para {
    let ppr = child(p, "pPr");
    let style = ppr.and_then(|pp| child(pp, "pStyle")).and_then(|s| attr(s, "val"));
    let align = ppr
        .and_then(|pp| child(pp, "jc"))
        .and_then(|j| attr(j, "val"))
        .map(map_align);
    let numpr = ppr.and_then(|pp| child(pp, "numPr"));
    let (own_left_cm, own_first_cm) = para_indents(ppr);
    let line_h = line_height_mult(ppr);

    // FÓRMULA DE BLOCO: `<m:oMathPara>` (math display do Word) direto no
    // parágrafo → nó mathBlock. (Math INLINE — `<m:oMath>` solto entre runs —
    // é tratado em convert_runs, preservando o texto ao redor.)
    if let Some(opara) = child(p, "oMathPara") {
        if let Some(latex) = omml_to_latex_opt(opara) {
            stats.math += 1;
            return Para::Block(math_block_node(&latex));
        }
    }

    // IMAGEM: se o parágrafo tem um desenho com FIGURA (a:blip), extrai e
    // emite um `figure`; senão (desenho sem figura, ou formato vetorial
    // EMF/WMF que o navegador não exibe) mantém um placeholder discreto.
    let has_drawing = p
        .descendants()
        .any(|d| d.is_element() && d.tag_name().name() == "drawing");
    if has_drawing {
        if let Some(fig) = picture_figure(p, rels, imgs) {
            stats.images += 1;
            return Para::Block(fig);
        }
        // Sem figura web. Distinção honesta (§13):
        if blip_and_owner(p).is_some() {
            // HÁ imagem, mas em formato não-web (EMF/WMF): placeholder visível.
            stats.images += 1;
            return Para::Block(image_placeholder());
        }
        // Desenho SEM imagem = caixa de texto / elipse / forma FLUTUANTE
        // (anotação sobreposta à foto, ex.: "Ponto de Impacto" + elipse). NÃO
        // importamos a anotação (o perito re-anota no SICRO Imagem) e NÃO
        // poluímos com "[imagem não importada]". Parágrafo vazio — fiel, pois
        // no Word a âncora costuma ser um parágrafo sem texto visível.
        return Para::Block(json!({ "type": "paragraph" }));
    }

    let inline = convert_runs(p);

    // Contabiliza fórmulas inline emitidas por convert_runs.
    stats.math += inline
        .iter()
        .filter(|n| n.get("type").and_then(Value::as_str) == Some("mathInline"))
        .count();

    // LISTA.
    if let Some(np) = numpr {
        let num_id = child(np, "numId").and_then(|x| attr(x, "val")).unwrap_or("");
        let ilvl: u8 = child(np, "ilvl")
            .and_then(|x| attr(x, "val"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        match resolve_list(numbering, counters, num_id, ilvl) {
            // Bullet → item de bulletList nativo do SICRO.
            Some(ListKind::Bullet) => {
                stats.list_items += 1;
                let mut para = Map::new();
                para.insert("type".into(), json!("paragraph"));
                // Preserva alinhamento + entrelinhas NO parágrafo do item (o
                // recuo vem da própria lista). Sem isto, o item de lista vinha
                // alinhado à esquerda e com entrelinha simples, destoando do
                // corpo justificado em 1,5.
                let mut pattrs = Map::new();
                if let Some(a) = &align {
                    pattrs.insert("textAlign".into(), json!(a));
                }
                if line_h > 0.0 {
                    pattrs.insert("line_height".into(), json!(line_h));
                }
                if !pattrs.is_empty() {
                    para.insert("attrs".into(), Value::Object(pattrs));
                }
                if !inline.is_empty() {
                    para.insert("content".into(), json!(inline));
                }
                return Para::ListItem {
                    node: json!({
                        "type": "listItem",
                        "content": [Value::Object(para)],
                    }),
                };
            }
            // Numerado → "baked": o número multinível do Word ("3.1") vira
            // TEXTO no início, e o item vira parágrafo normal com recuo
            // pendente (left + 1ª linha negativa = hanging), igual ao Word.
            Some(ListKind::Numbered { label, left_cm, hanging_cm }) => {
                stats.list_items += 1;
                let mut content: Vec<Value> = Vec::new();
                content.push(json!({ "type": "text", "text": format!("{label} ") }));
                content.extend(inline);
                let mut attrs = Map::new();
                if let Some(a) = &align {
                    attrs.insert("textAlign".into(), json!(a));
                }
                if left_cm > 0.0 {
                    attrs.insert("left_indent_cm".into(), json!(round2(left_cm)));
                }
                if hanging_cm > 0.0 {
                    attrs.insert(
                        "first_line_indent_cm".into(),
                        json!(round2(-hanging_cm)),
                    );
                }
                if line_h > 0.0 {
                    attrs.insert("line_height".into(), json!(line_h));
                }
                let mut node = Map::new();
                node.insert("type".into(), json!("paragraph"));
                if !attrs.is_empty() {
                    node.insert("attrs".into(), Value::Object(attrs));
                }
                node.insert("content".into(), json!(content));
                return Para::Block(Value::Object(node));
            }
            // Numeração não resolvida → cai como parágrafo normal abaixo.
            None => {}
        }
    }

    // TÍTULO.
    if let Some(level) = style.and_then(heading_level) {
        stats.headings += 1;
        let mut attrs = Map::new();
        attrs.insert("level".into(), json!(level));
        if let Some(a) = &align {
            attrs.insert("textAlign".into(), json!(a));
        }
        if own_left_cm > 0.0 {
            attrs.insert("left_indent_cm".into(), json!(round2(own_left_cm)));
        }
        if own_first_cm != 0.0 {
            attrs.insert("first_line_indent_cm".into(), json!(round2(own_first_cm)));
        }
        if line_h > 0.0 {
            attrs.insert("line_height".into(), json!(line_h));
        }
        let mut node = Map::new();
        node.insert("type".into(), json!("heading"));
        node.insert("attrs".into(), Value::Object(attrs));
        if !inline.is_empty() {
            node.insert("content".into(), json!(inline));
        }
        return Para::Block(Value::Object(node));
    }

    // PARÁGRAFO normal (com legenda quando o estilo for Legenda/Caption).
    stats.paragraphs += 1;
    let mut attrs = Map::new();
    if let Some(a) = &align {
        attrs.insert("textAlign".into(), json!(a));
    }
    if let Some(ls) = style.and_then(laudo_style_for) {
        attrs.insert("laudoStyle".into(), json!(ls));
    }
    if own_left_cm > 0.0 {
        attrs.insert("left_indent_cm".into(), json!(round2(own_left_cm)));
    }
    if own_first_cm != 0.0 {
        attrs.insert("first_line_indent_cm".into(), json!(round2(own_first_cm)));
    }
    if line_h > 0.0 {
        attrs.insert("line_height".into(), json!(line_h));
    }
    let mut node = Map::new();
    node.insert("type".into(), json!("paragraph"));
    if !attrs.is_empty() {
        node.insert("attrs".into(), Value::Object(attrs));
    }
    if !inline.is_empty() {
        node.insert("content".into(), json!(inline));
    }
    Para::Block(Value::Object(node))
}

// ---------------------------------------------------------------------------
// Imagens (w:drawing / w:pict → figure)

/// Extensões que o WebView2 exibe direto num `<img>`. EMF/WMF (vetor do
/// Office) ficam DE FORA — viram placeholder.
fn is_web_image_ext(ext: &str) -> bool {
    matches!(ext, "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp")
}

/// Acha a FIGURA (`<a:blip r:embed>` DrawingML ou `<v:imagedata r:id>` VML)
/// dentro de `node` e devolve `(rId, drawing_dono)`. O "dono" é o ancestral
/// `<w:drawing>`/`<w:pict>` que CONTÉM o blip — usado pra ler geometria/largura
/// SÓ desse desenho, e NÃO de um anchor IRMÃO (ex.: caixa de texto/elipse
/// flutuante no mesmo parágrafo, como o "Ponto de Impacto"). Sem isso, a foto
/// inline herdaria a posição flutuante do anchor irmão e iria parar no topo da
/// página (bug das fotos de ponto de impacto).
fn blip_and_owner<'a, 'i>(
    node: roxmltree::Node<'a, 'i>,
) -> Option<(String, roxmltree::Node<'a, 'i>)> {
    // DrawingML: <a:blip r:embed|r:link>.
    if let Some(b) = node
        .descendants()
        .find(|d| d.is_element() && d.tag_name().name() == "blip")
    {
        if let Some(rid) = attr(b, "embed").or_else(|| attr(b, "link")) {
            return Some((rid.to_string(), drawing_owner(b).unwrap_or(node)));
        }
    }
    // VML: <v:imagedata r:id>.
    if let Some(im) = node
        .descendants()
        .find(|d| d.is_element() && d.tag_name().name() == "imagedata")
    {
        if let Some(rid) = attr(im, "id") {
            return Some((rid.to_string(), drawing_owner(im).unwrap_or(node)));
        }
    }
    None
}

/// Ancestral `<w:drawing>`/`<w:pict>` mais próximo de `node` (o "desenho" dono).
fn drawing_owner<'a, 'i>(node: roxmltree::Node<'a, 'i>) -> Option<roxmltree::Node<'a, 'i>> {
    node.ancestors()
        .find(|a| a.is_element() && matches!(a.tag_name().name(), "drawing" | "pict"))
}

/// Largura em cm a partir do `<wp:extent cx>` (360000 EMU/cm), clamped a
/// [1; 17] cm. `None` se ausente.
fn drawing_extent_width_cm(node: roxmltree::Node) -> Option<f64> {
    node.descendants()
        .filter(|d| d.is_element() && d.tag_name().name() == "extent")
        .find_map(|e| attr(e, "cx"))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|emu| (emu / 360000.0).clamp(1.0, 17.0))
}

const EMU_PER_CM: f64 = 360000.0;

/// Geometria de uma figura FLUTUANTE (posição + rotação + modo de quebra). O
/// Word só guarda isso para imagens ancoradas; imagem inline (`<wp:inline>`)
/// → `floating=false` e offsets `None`.
struct FloatGeom {
    floating: bool,
    x_cm: Option<f64>,
    y_cm: Option<f64>,
    /// Graus, horário, normalizado para [0, 360).
    rotation_deg: f64,
    /// `true` quando a imagem fica ATRÁS do texto (behindDoc / z-index < 0).
    behind: bool,
}

impl FloatGeom {
    fn inline() -> Self {
        FloatGeom {
            floating: false,
            x_cm: None,
            y_cm: None,
            rotation_deg: 0.0,
            behind: false,
        }
    }
}

/// Lê posição + rotação + modo de quebra de um `w:drawing`/`w:pict`. Onde o
/// Word guarda cada coisa:
///   - **DrawingML** flutuante: `<wp:anchor behindDoc="…">` +
///     `<wp:positionH/V><wp:posOffset>` (EMU) + `<a:xfrm rot="…">`
///     (60000-avos de grau).
///   - **VML** (legado): `<v:shape style="position:absolute;left:…;top:…;
///     rotation:…;z-index:…">`.
///   - **Inline** (`<wp:inline>` / sem âncora): sem posição → `floating=false`.
///
/// Mapeamento é **best-effort** (§13): o referencial do Word (página/margem/
/// coluna) é aproximado para o offset relativo ao card da página do SICRO; o
/// perito ajusta arrastando. A rotação é exata.
fn drawing_float_geom(node: roxmltree::Node) -> FloatGeom {
    // DrawingML: <wp:anchor>.
    if let Some(anchor) = first_desc(node, "anchor") {
        let behind = attr(anchor, "behindDoc")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        return FloatGeom {
            floating: true,
            x_cm: position_offset_cm(anchor, "positionH"),
            y_cm: position_offset_cm(anchor, "positionV"),
            rotation_deg: xfrm_rotation_deg(node),
            behind,
        };
    }
    // VML: <v:shape style="position:absolute;…">.
    if let Some(shape) = first_desc(node, "shape") {
        if let Some(style) = attr(shape, "style") {
            if style.contains("position:absolute") {
                let behind = vml_style_value(style, "z-index")
                    .and_then(|v| v.trim().parse::<f64>().ok())
                    .map(|z| z < 0.0)
                    .unwrap_or(false);
                return FloatGeom {
                    floating: true,
                    x_cm: vml_style_value(style, "left").and_then(|v| parse_css_len_cm(&v)),
                    y_cm: vml_style_value(style, "top").and_then(|v| parse_css_len_cm(&v)),
                    rotation_deg: vml_style_value(style, "rotation")
                        .and_then(|v| v.trim().trim_end_matches("deg").trim().parse::<f64>().ok())
                        .map(normalize_deg)
                        .unwrap_or(0.0),
                    behind,
                };
            }
        }
    }
    FloatGeom::inline()
}

/// Primeiro descendente (qualquer profundidade) com o nome local dado.
fn first_desc<'a, 'i>(node: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    node.descendants()
        .find(|d| d.is_element() && d.tag_name().name() == name)
}

/// `<wp:posOffset>` (EMU) dentro de `<wp:positionH|V>` → cm, clamped [-50; 50].
fn position_offset_cm(anchor: roxmltree::Node, pos_tag: &str) -> Option<f64> {
    let pos = first_desc(anchor, pos_tag)?;
    let off = first_desc(pos, "posOffset")?;
    let emu: f64 = off.text()?.trim().parse().ok()?;
    Some((emu / EMU_PER_CM).clamp(-50.0, 50.0))
}

/// `rot` do primeiro `<a:xfrm>` (60000-avos de grau) → graus [0; 360).
fn xfrm_rotation_deg(node: roxmltree::Node) -> f64 {
    node.descendants()
        .filter(|d| d.is_element() && d.tag_name().name() == "xfrm")
        .find_map(|x| attr(x, "rot"))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|r| normalize_deg(r / 60000.0))
        .unwrap_or(0.0)
}

/// Normaliza graus para [0; 360).
fn normalize_deg(d: f64) -> f64 {
    ((d % 360.0) + 360.0) % 360.0
}

/// Extrai `key:value` de um `style` CSS-like (declarações separadas por `;`).
fn vml_style_value(style: &str, key: &str) -> Option<String> {
    for decl in style.split(';') {
        let mut it = decl.splitn(2, ':');
        let k = it.next()?.trim();
        if k.eq_ignore_ascii_case(key) {
            return it.next().map(|s| s.trim().to_string());
        }
    }
    None
}

/// Converte um comprimento CSS (`10pt`, `1.5cm`, `96px`…) em cm. Default pt
/// (unidade padrão do VML do Office). Clamp [-50; 50].
fn parse_css_len_cm(raw: &str) -> Option<f64> {
    let s = raw.trim();
    let idx = s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len());
    let n: f64 = s[..idx].trim().parse().ok()?;
    let cm = match s[idx..].trim() {
        "cm" => n,
        "mm" => n / 10.0,
        "in" => n * 2.54,
        "pt" | "" => n * 2.54 / 72.0,
        "pc" => n * 12.0 * 2.54 / 72.0,
        "px" => n * 2.54 / 96.0,
        _ => return None,
    };
    Some(cm.clamp(-50.0, 50.0))
}

/// Resolve um `w:drawing`/`w:pict` (dentro de `node`) numa figura WEB e
/// registra a mídia pendente. Retorna o nó `figure` (com `relative_path`
/// temporário `__docximport__/<name>`) ou `None` se não houver figura web
/// (sem blip, rId não resolvido, ou formato vetorial EMF/WMF).
fn picture_figure(
    node: roxmltree::Node,
    rels: &HashMap<String, String>,
    imgs: &mut Vec<PendingImage>,
) -> Option<Value> {
    let (rid, owner) = blip_and_owner(node)?;
    let target = rels.get(&rid)?; // ex: "media/image1.png"
    let ext = std::path::Path::new(target)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())?;
    if !is_web_image_ext(&ext) {
        return None;
    }
    let name = format!("img{}.{}", imgs.len() + 1, ext);
    let zip_path = format!("word/{}", target.trim_start_matches('/'));
    imgs.push(PendingImage {
        name: name.clone(),
        zip_path,
        ext,
    });
    // Geometria/largura SÓ do desenho dono do blip (`owner`), nunca do
    // parágrafo inteiro: assim a foto inline não herda a posição flutuante de
    // uma caixa de texto/elipse irmã ancorada no mesmo `w:p`.
    Some(figure_node(
        &name,
        drawing_extent_width_cm(owner),
        &drawing_float_geom(owner),
    ))
}

/// Monta um `figure` do SICRO (com `figcaption` vazio — exigido pelo schema).
/// `relative_path` é o marcador temporário (reescrito no comando de import);
/// `width` vem do extent do Word, senão "70%".
///
/// W (fase 2c) — quando a imagem era FLUTUANTE no Word (`geom.floating`), o
/// figure entra em modo `in_front`/`behind` ancorado em (`wrap_x_cm`,
/// `wrap_y_cm`); a rotação (ex.: brasão lateralizado 270°) vai pro attr
/// `rotation`. Imagem inline → fluxo normal centralizado (como antes).
fn figure_node(name: &str, width_cm: Option<f64>, geom: &FloatGeom) -> Value {
    let width = match width_cm {
        Some(cm) => format!("{cm:.2}cm"),
        None => "70%".to_string(),
    };
    let mut figure = json!({
        "type": "figure",
        "attrs": {
            "src": null,
            "relative_path": format!("{DOCX_IMPORT_PREFIX}{name}"),
            "kind": "image",
            "width": width,
            "align": "center",
        },
        "content": [{ "type": "figcaption" }],
    });
    // Attrs aditivos: só emitidos quando há algo a preservar (mantém o output
    // enxuto e os testes de imagem inline intactos).
    let attrs = figure["attrs"].as_object_mut().expect("attrs é objeto");
    if geom.rotation_deg.abs() > 0.01 {
        attrs.insert("rotation".into(), json!(round2(geom.rotation_deg)));
    }
    if geom.floating {
        attrs.insert(
            "wrap_mode".into(),
            json!(if geom.behind { "behind" } else { "in_front" }),
        );
        if let Some(x) = geom.x_cm {
            attrs.insert("wrap_x_cm".into(), json!(round2(x)));
        }
        if let Some(y) = geom.y_cm {
            attrs.insert("wrap_y_cm".into(), json!(round2(y)));
        }
    }
    figure
}

/// Ajuste de coordenada para figuras de CABEÇALHO/RODAPÉ. A banda do
/// header/footer é curta (~2 cm) e, no editor, a figura flutuante é
/// posicionada RELATIVA à região (`.region` é `position:absolute`) — não à
/// página. Mas o Word costuma ancorar a imagem RELATIVA À PÁGINA (ex.: Y ≈
/// 27 cm para um brasão no rodapé do A4), o que jogaria a figura para muito
/// abaixo da banda. Quando o Y capturado é grande demais para uma banda
/// (> 8 cm), reposiciona no topo da banda (0,2 cm) — visível e arrastável.
/// O X (lateral esquerda) e a rotação são preservados; o perito ajusta o
/// resto arrastando (§13: best-effort, honesto).
fn clamp_chrome_figure_y(fig: &mut Value) {
    let Some(attrs) = fig.get_mut("attrs").and_then(|a| a.as_object_mut()) else {
        return;
    };
    if let Some(y) = attrs.get("wrap_y_cm").and_then(|v| v.as_f64()) {
        if !(0.0..=8.0).contains(&y) {
            attrs.insert("wrap_y_cm".into(), json!(0.2));
        }
    }
}

/// Placeholder pra desenhos sem figura web (EMF/WMF, formas sem blip).
fn image_placeholder() -> Value {
    json!({
        "type": "paragraph",
        "attrs": { "laudoStyle": "legenda" },
        "content": [{
            "type": "text",
            "marks": [{ "type": "italic" }],
            "text": "[imagem não importada — formato não suportado; reinsira manualmente]",
        }],
    })
}

// ---------------------------------------------------------------------------
// Inline (runs)

fn convert_runs(p: roxmltree::Node) -> Vec<Value> {
    let mut inline: Vec<Value> = Vec::new();
    for c in p.children().filter(|n| n.is_element()) {
        match c.tag_name().name() {
            "r" => collect_run(c, &mut inline),
            // Hyperlink / campo simples: extrai os runs internos como texto.
            "hyperlink" | "fldSimple" => {
                for r in c
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "r")
                {
                    collect_run(r, &mut inline);
                }
            }
            // Fórmula matemática INLINE (`<m:oMath>` solto entre os runs do
            // parágrafo) → nó mathInline (só LaTeX; o front gera o PNG ao abrir).
            "oMath" => {
                if let Some(latex) = omml_to_latex_opt(c) {
                    inline.push(math_inline_node(&latex));
                }
            }
            _ => {}
        }
    }
    inline
}

fn collect_run(r: roxmltree::Node, inline: &mut Vec<Value>) {
    let marks = build_marks(child(r, "rPr"));
    for c in r.children().filter(|n| n.is_element()) {
        match c.tag_name().name() {
            "t" => {
                let txt = c.text().unwrap_or("");
                if !txt.is_empty() {
                    inline.push(text_node(txt, &marks));
                }
            }
            "tab" => inline.push(text_node("\t", &marks)),
            "br" => {
                // Quebra de linha vira hardBreak; quebra de PÁGINA é ignorada
                // (a paginação do SICRO é automática).
                if attr(c, "type") != Some("page") {
                    inline.push(json!({ "type": "hardBreak" }));
                }
            }
            // `w:instrText` (código de campo, ex: " SEQ Figura ") é ignorado.
            _ => {}
        }
    }
}

// ===========================================================================
// OMML (Office Math) → LaTeX — Rodada 2 (importação de fórmulas do Word).
//
// O Word guarda equações em OOXML Math (`<m:oMath>` inline, `<m:oMathPara>`
// display). Convertemos pra LaTeX (best-effort, §13) e emitimos um nó math só
// com `latex` — o front gera o PNG de exibição ao abrir o laudo (o Rust não roda
// MathLive). Cobrimos as construções comuns dos laudos: fração, raiz, sub/
// sobrescrito, n-ário (Σ/∏/∫), delimitadores, função, barra. Texto unicode
// (gregas, ×, ≈, √…) passa direto — KaTeX/MathLive renderizam. Construções raras
// degradam pro texto interno (nunca perdem o conteúdo silenciosamente).

/// Nó math inline (só `latex`; `render_png` é hidratado no front ao carregar).
fn math_inline_node(latex: &str) -> Value {
    json!({ "type": "mathInline", "attrs": { "latex": latex } })
}

/// Nó math de bloco (display), idem.
fn math_block_node(latex: &str) -> Value {
    json!({ "type": "mathBlock", "attrs": { "latex": latex } })
}

/// LaTeX de um nó OMML; `None` se vazio/só espaço.
fn omml_to_latex_opt(node: roxmltree::Node) -> Option<String> {
    let s = omml_to_latex(node);
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// Conversão recursiva de um elemento OMML em LaTeX.
fn omml_to_latex(node: roxmltree::Node) -> String {
    match node.tag_name().name() {
        "t" => escape_math_text(node.text().unwrap_or("")),
        "r" => omml_children_latex(node), // math run → pega os <m:t> internos
        "f" => {
            let num = child(node, "num").map(omml_to_latex).unwrap_or_default();
            let den = child(node, "den").map(omml_to_latex).unwrap_or_default();
            format!("\\frac{{{}}}{{{}}}", num.trim(), den.trim())
        }
        "sSup" => {
            let base = child(node, "e").map(omml_to_latex).unwrap_or_default();
            let sup = child(node, "sup").map(omml_to_latex).unwrap_or_default();
            format!("{{{}}}^{{{}}}", base.trim(), sup.trim())
        }
        "sSub" => {
            let base = child(node, "e").map(omml_to_latex).unwrap_or_default();
            let sub = child(node, "sub").map(omml_to_latex).unwrap_or_default();
            format!("{{{}}}_{{{}}}", base.trim(), sub.trim())
        }
        "sSubSup" => {
            let base = child(node, "e").map(omml_to_latex).unwrap_or_default();
            let sub = child(node, "sub").map(omml_to_latex).unwrap_or_default();
            let sup = child(node, "sup").map(omml_to_latex).unwrap_or_default();
            format!("{{{}}}_{{{}}}^{{{}}}", base.trim(), sub.trim(), sup.trim())
        }
        "rad" => {
            let e = child(node, "e").map(omml_to_latex).unwrap_or_default();
            let deg = child(node, "deg").map(omml_to_latex).unwrap_or_default();
            if deg.trim().is_empty() {
                format!("\\sqrt{{{}}}", e.trim())
            } else {
                format!("\\sqrt[{}]{{{}}}", deg.trim(), e.trim())
            }
        }
        "nary" => omml_nary_latex(node),
        "d" => omml_delim_latex(node),
        "func" => {
            let name = child(node, "fName").map(omml_to_latex).unwrap_or_default();
            let e = child(node, "e").map(omml_to_latex).unwrap_or_default();
            format!("{}({})", name.trim(), e.trim())
        }
        "bar" => {
            let e = child(node, "e").map(omml_to_latex).unwrap_or_default();
            format!("\\overline{{{}}}", e.trim())
        }
        // accent / group: best-effort — preserva a base (não perde conteúdo).
        "acc" | "groupChr" => child(node, "e").map(omml_to_latex).unwrap_or_default(),
        // Containers (oMath, oMathPara, e, num, den, sub, sup, deg, fName, lim…)
        // e desconhecidos: recursa nos filhos.
        _ => omml_children_latex(node),
    }
}

/// Concatena o LaTeX dos filhos-elemento, pulando propriedades (`*Pr`).
fn omml_children_latex(node: roxmltree::Node) -> String {
    let mut out = String::new();
    for c in node.children().filter(|n| n.is_element()) {
        if c.tag_name().name().ends_with("Pr") {
            continue; // fPr, sSupPr, naryPr, radPr, dPr, rPr, ctrlPr, mPr…
        }
        out.push_str(&omml_to_latex(c));
    }
    out
}

/// n-ário (Σ/∏/∫…) com sub/sobre-índice e corpo. `naryPr/chr@val` = operador.
fn omml_nary_latex(node: roxmltree::Node) -> String {
    let chr = child(node, "naryPr")
        .and_then(|pr| child(pr, "chr"))
        .and_then(|c| attr(c, "val"))
        .unwrap_or("∫"); // default do Word é integral
    let op = match chr {
        "∑" => "\\sum".to_string(),
        "∏" => "\\prod".to_string(),
        "∐" => "\\coprod".to_string(),
        "∫" => "\\int".to_string(),
        "∬" => "\\iint".to_string(),
        "∭" => "\\iiint".to_string(),
        "∮" => "\\oint".to_string(),
        "⋃" => "\\bigcup".to_string(),
        "⋂" => "\\bigcap".to_string(),
        other => format!("{} ", escape_math_text(other)),
    };
    let sub = child(node, "sub").map(omml_to_latex).unwrap_or_default();
    let sup = child(node, "sup").map(omml_to_latex).unwrap_or_default();
    let e = child(node, "e").map(omml_to_latex).unwrap_or_default();
    let mut s = op;
    if !sub.trim().is_empty() {
        s.push_str(&format!("_{{{}}}", sub.trim()));
    }
    if !sup.trim().is_empty() {
        s.push_str(&format!("^{{{}}}", sup.trim()));
    }
    s.push(' ');
    s.push_str(e.trim());
    s.trim().to_string()
}

/// Delimitador: begChr/endChr (default parênteses) com `\left…\right`.
fn omml_delim_latex(node: roxmltree::Node) -> String {
    let pr = child(node, "dPr");
    let beg = pr
        .and_then(|p| child(p, "begChr"))
        .and_then(|c| attr(c, "val"))
        .unwrap_or("(");
    let end = pr
        .and_then(|p| child(p, "endChr"))
        .and_then(|c| attr(c, "val"))
        .unwrap_or(")");
    let inner: String = node
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "e")
        .map(omml_to_latex)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "\\left{} {} \\right{}",
        latex_delim(beg),
        inner.trim(),
        latex_delim(end)
    )
}

fn latex_delim(c: &str) -> String {
    match c {
        "(" | ")" | "[" | "]" | "|" => c.to_string(),
        "{" => "\\{".to_string(),
        "}" => "\\}".to_string(),
        "" => ".".to_string(), // delimitador vazio em LaTeX
        other => other.to_string(),
    }
}

/// Escapa os caracteres de controle do LaTeX em texto literal de `<m:t>`.
/// Letras/dígitos/operadores/unicode (gregas, ×, ≈) passam direto.
fn escape_math_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\backslash "),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '$' => out.push_str("\\$"),
            '&' => out.push_str("\\&"),
            '#' => out.push_str("\\#"),
            '%' => out.push_str("\\%"),
            '_' => out.push_str("\\_"),
            '^' => out.push_str("\\char94 "),
            '~' => out.push_str("\\sim "),
            _ => out.push(ch),
        }
    }
    out
}

fn text_node(text: &str, marks: &[Value]) -> Value {
    let mut o = Map::new();
    o.insert("type".into(), json!("text"));
    o.insert("text".into(), json!(text));
    if !marks.is_empty() {
        o.insert("marks".into(), json!(marks));
    }
    Value::Object(o)
}

fn build_marks(rpr: Option<roxmltree::Node>) -> Vec<Value> {
    let mut marks: Vec<Value> = Vec::new();
    let Some(rpr) = rpr else {
        return marks;
    };
    if bool_prop(rpr, "b") {
        marks.push(json!({ "type": "bold" }));
    }
    if bool_prop(rpr, "i") {
        marks.push(json!({ "type": "italic" }));
    }
    if let Some(u) = child(rpr, "u") {
        if attr(u, "val").map(|v| v != "none").unwrap_or(true) {
            marks.push(json!({ "type": "underline" }));
        }
    }
    if bool_prop(rpr, "strike") {
        marks.push(json!({ "type": "strike" }));
    }
    if let Some(va) = child(rpr, "vertAlign") {
        match attr(va, "val") {
            Some("superscript") => marks.push(json!({ "type": "superscript" })),
            Some("subscript") => marks.push(json!({ "type": "subscript" })),
            _ => {}
        }
    }

    // color + fontSize + fontFamily compartilham a MESMA marca `textStyle`
    // (não podem ser marcas separadas do mesmo tipo).
    let mut ts = Map::new();
    if let Some(v) = child(rpr, "color").and_then(|c| attr(c, "val")) {
        if v != "auto" && !v.is_empty() {
            ts.insert("color".into(), json!(format!("#{}", v.to_lowercase())));
        }
    }
    if let Some(v) = child(rpr, "sz").and_then(|s| attr(s, "val")) {
        if let Ok(half) = v.parse::<f64>() {
            let pt = half / 2.0;
            let s = if pt.fract() == 0.0 {
                format!("{}pt", pt as i64)
            } else {
                format!("{}pt", pt)
            };
            ts.insert("fontSize".into(), json!(s));
        }
    }
    if let Some(v) = child(rpr, "rFonts").and_then(|f| attr(f, "ascii")) {
        if !v.is_empty() {
            ts.insert("fontFamily".into(), json!(v));
        }
    }
    if !ts.is_empty() {
        marks.push(json!({ "type": "textStyle", "attrs": Value::Object(ts) }));
    }

    marks
}

/// `<w:b/>` = ligado; `<w:b w:val="0"/>` = desligado.
fn bool_prop(rpr: roxmltree::Node, name: &str) -> bool {
    match child(rpr, name) {
        None => false,
        Some(el) => match attr(el, "val") {
            Some(v) => !matches!(v, "0" | "false" | "off"),
            None => true,
        },
    }
}

// ---------------------------------------------------------------------------
// Mapeamentos

fn map_align(jc: &str) -> String {
    match jc {
        "center" => "center",
        "right" | "end" => "right",
        "both" | "distribute" => "justify",
        _ => "left",
    }
    .to_string()
}

/// `Heading1..3` / `Ttulo1..3` / `Título1..3` → nível 1..3 (clampa em 3).
fn heading_level(style: &str) -> Option<u8> {
    let lower = style.to_lowercase();
    let stripped = lower
        .strip_prefix("heading")
        .or_else(|| lower.strip_prefix("título"))
        .or_else(|| lower.strip_prefix("titulo"))
        .or_else(|| lower.strip_prefix("ttulo"))?;
    let digits: String = stripped.trim().chars().filter(|c| c.is_ascii_digit()).collect();
    let n: u8 = digits.parse().ok()?;
    if n == 0 {
        None
    } else {
        Some(n.min(3))
    }
}

/// Estilos de legenda → o estilo documental `legenda` do SICRO.
fn laudo_style_for(style: &str) -> Option<&'static str> {
    let lower = style.to_lowercase();
    if lower == "legenda" || lower == "caption" {
        Some("legenda")
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Margens (sectPr / pgMar)

fn find_margins(body: roxmltree::Node) -> Option<DocxMargins> {
    let sect = child(body, "sectPr")?;
    let pg = child(sect, "pgMar")?;
    let cm = |name: &str| -> Option<f64> {
        attr(pg, name)
            .and_then(|v| v.parse::<f64>().ok())
            .map(|tw| tw / TWIPS_PER_CM)
    };
    Some(DocxMargins {
        top: cm("top")?,
        right: cm("right")?,
        bottom: cm("bottom")?,
        left: cm("left")?,
    })
}

/// Estima a ALTURA (cm) do conteúdo do cabeçalho do `.docx`. Mesmo SEM importar
/// o header, isto é usado pra RESERVAR a margem de topo — o Word costuma usar
/// `w:top` pequeno e contar com o conteúdo do header (brasão + timbre + caixa de
/// registro) pra empurrar o corpo. Heurística (best-effort, o perito afina na
/// régua): maior imagem (brasão, via `cy` em EMU) + linhas de texto fora de
/// tabela + linhas de tabela. Clampa em faixa razoável. `None` se não há header.
fn estimate_header_reserve_cm<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    body: roxmltree::Node,
    rels: &HashMap<String, String>,
) -> Option<f64> {
    let target = find_header_target(body, rels)?;
    let xml = read_entry_string(zip, &target)?;
    let xml = xml.trim_start_matches('\u{feff}');
    let doc = roxmltree::Document::parse(xml).ok()?;
    let root = doc.root_element();

    let mut img_cm = 0.0_f64; // maior imagem (brasão), em cm
    let mut text_lines = 0usize; // <w:p> com texto FORA de tabela
    let mut table_rows = 0usize; // linhas de tabela (caixa de registro)
    for n in root.descendants() {
        if !n.is_element() {
            continue;
        }
        match n.tag_name().name() {
            // wp:extent / a:ext → `cy` em EMU (1 cm = 360000 EMU).
            "extent" | "ext" => {
                if let Some(cy) = attr(n, "cy").and_then(|v| v.parse::<f64>().ok()) {
                    img_cm = img_cm.max(cy / 360_000.0);
                }
            }
            "tr" => table_rows += 1,
            "p" => {
                let in_table = n.ancestors().any(|a| a.tag_name().name() == "tbl");
                let has_text = n.descendants().any(|t| {
                    t.tag_name().name() == "t"
                        && t.text().map(|s| !s.trim().is_empty()).unwrap_or(false)
                });
                if has_text && !in_table {
                    text_lines += 1;
                }
            }
            _ => {}
        }
    }

    // Brasão, timbre e caixa empilham na vertical no cabeçalho institucional.
    // Soma + uma folga; clampa pra não exagerar (o perito ajusta na régua).
    let reserve =
        img_cm + (text_lines as f64) * 0.45 + (table_rows as f64) * 0.5 + 0.3;
    Some((reserve * 100.0).round() / 100.0)
        .map(|v: f64| v.clamp(2.5, 6.0))
}

// ---------------------------------------------------------------------------
// Recuo da primeira linha

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Entrelinhas do parágrafo a partir do `w:pPr/w:spacing` (Word). Devolve o
/// multiplicador (1.5, 2.0…) quando `lineRule="auto"` e o valor é claramente
/// maior que "simples" (~1,0). `0.0` = usar o default do editor (≈1,15).
/// `lineRule` exact/atLeast (twips absolutos) é ignorado por ora.
fn line_height_mult(ppr: Option<roxmltree::Node>) -> f64 {
    let Some(sp) = ppr.and_then(|pp| child(pp, "spacing")) else {
        return 0.0;
    };
    // O Word trata `lineRule` ausente como "auto".
    if attr(sp, "lineRule").unwrap_or("auto") != "auto" {
        return 0.0;
    }
    let Some(line) = attr(sp, "line").and_then(|v| v.parse::<f64>().ok()) else {
        return 0.0;
    };
    let mult = line / 240.0; // 240 = simples; 360 = 1,5; 480 = duplo.
    // "Simples" (~1,0) → mantém o default do editor (≈1,15). Só seta quando é
    // visivelmente 1,5 / 2,0 / etc.
    if mult <= 1.2 {
        return 0.0;
    }
    round2(mult)
}

/// `w:pPr/w:ind w:firstLine="708"` (twips) → cm. 0 quando ausente/≤0.
fn first_line_indent_cm(ppr: Option<roxmltree::Node>) -> f64 {
    ppr.and_then(|pp| child(pp, "ind"))
        .and_then(|ind| attr(ind, "firstLine"))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|tw| tw / TWIPS_PER_CM)
        .filter(|cm| *cm > 0.0)
        .unwrap_or(0.0)
}

/// Recuos do parágrafo a partir do seu próprio `w:ind`:
///   - left  = `w:left`/`w:start` em cm (>0; ignora negativos).
///   - first = `w:firstLine` (positivo) OU `-w:hanging` (recuo pendente).
fn para_indents(ppr: Option<roxmltree::Node>) -> (f64, f64) {
    let Some(ind) = ppr.and_then(|pp| child(pp, "ind")) else {
        return (0.0, 0.0);
    };
    let tw = |name: &str| attr(ind, name).and_then(|v| v.parse::<f64>().ok());
    let left = tw("left")
        .or_else(|| tw("start"))
        .map(|t| t / TWIPS_PER_CM)
        .filter(|c| *c > 0.0)
        .unwrap_or(0.0);
    let first = if let Some(fl) = tw("firstLine") {
        (fl / TWIPS_PER_CM).max(0.0)
    } else if let Some(h) = tw("hanging") {
        -(h / TWIPS_PER_CM)
    } else {
        0.0
    };
    (left, first)
}

// ---------------------------------------------------------------------------
// Cabeçalho (word/header*.xml)

/// `word/_rels/document.xml.rels`: Id → Target (ex: rId7 → "header2.xml").
fn parse_rels(xml: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let xml = xml.trim_start_matches('\u{feff}');
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return out;
    };
    for r in doc
        .root_element()
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "Relationship")
    {
        if let (Some(id), Some(target)) = (attr(r, "Id"), attr(r, "Target")) {
            out.insert(id.to_string(), target.to_string());
        }
    }
    out
}

/// Resolve o caminho do header default via `sectPr/headerReference` (prefere
/// "default", depois "first", depois o primeiro) → rels → "word/headerN.xml".
fn find_header_target(
    body: roxmltree::Node,
    rels: &HashMap<String, String>,
) -> Option<String> {
    let sect = child(body, "sectPr")?;
    let refs: Vec<roxmltree::Node> = sect
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "headerReference")
        .collect();
    let pick = refs
        .iter()
        .find(|r| attr(**r, "type") == Some("default"))
        .or_else(|| refs.iter().find(|r| attr(**r, "type") == Some("first")))
        .or_else(|| refs.first())?;
    let rid = attr(*pick, "id")?;
    let target = rels.get(rid)?;
    Some(format!("word/{}", target.trim_start_matches('/')))
}

/// W — Resolve o caminho do rodapé default via `sectPr/footerReference`
/// (mesma lógica do header: prefere "default", depois "first", depois o 1º).
fn find_footer_target(
    body: roxmltree::Node,
    rels: &HashMap<String, String>,
) -> Option<String> {
    let sect = child(body, "sectPr")?;
    let refs: Vec<roxmltree::Node> = sect
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "footerReference")
        .collect();
    let pick = refs
        .iter()
        .find(|r| attr(**r, "type") == Some("default"))
        .or_else(|| refs.iter().find(|r| attr(**r, "type") == Some("first")))
        .or_else(|| refs.first())?;
    let rid = attr(*pick, "id")?;
    let target = rels.get(rid)?;
    Some(format!("word/{}", target.trim_start_matches('/')))
}

/// Converte `word/header*.xml` (`<w:hdr>`) num doc ProseMirror, em ordem de
/// documento: parágrafos viram parágrafos; `w:tbl` (ex: o bloco de registro
/// "Registrado em / LAUDO Nº / Folha") vira TABELA do SICRO. Imagens (brasão)
/// são ignoradas no passo 1. Retorna `None` quando o cabeçalho não tem texto.
fn parse_header_xml(
    xml: &str,
    numbering: &Numbering,
    rels: &HashMap<String, String>,
    imgs: &mut Vec<PendingImage>,
    stats: &mut ImportStats,
) -> Option<Value> {
    let xml = xml.trim_start_matches('\u{feff}');
    let doc = roxmltree::Document::parse(xml).ok()?;
    let hdr = doc.root_element();

    let mut blocks: Vec<Value> = Vec::new();
    let mut counters: Counters = HashMap::new();
    collect_header_blocks(
        hdr, numbering, &mut counters, rels, imgs, stats, &mut blocks,
    );

    // Só ativa o cabeçalho se houver CONTEÚDO real: texto OU figura (brasão) /
    // caixa de texto / tabela. Evita header totalmente vazio.
    if !blocks.iter().any(header_block_has_content) {
        return None;
    }
    // Diagnóstico: nº de blocos top-level do cabeçalho (parágrafos + tabelas).
    stats.header_paragraphs = blocks.len();
    Some(json!({ "type": "doc", "content": blocks }))
}

/// Conteúdo "real" num bloco do cabeçalho: texto não-vazio OU um nó visual
/// (figura/brasão, caixa de texto, tabela).
fn header_block_has_content(v: &Value) -> bool {
    let t = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    matches!(t, "figure" | "text_box" | "table") || value_has_text(v)
}

/// W — Converte `word/footer*.xml` (`<w:ftr>`) num doc ProseMirror, reusando
/// a MESMA máquina do cabeçalho (`collect_header_blocks`): parágrafos, tabelas,
/// figuras (brasão da PC) e caixas verticais. Retorna `None` se vazio.
fn parse_footer_xml(
    xml: &str,
    numbering: &Numbering,
    rels: &HashMap<String, String>,
    imgs: &mut Vec<PendingImage>,
    stats: &mut ImportStats,
) -> Option<Value> {
    let xml = xml.trim_start_matches('\u{feff}');
    let doc = roxmltree::Document::parse(xml).ok()?;
    let ftr = doc.root_element();
    let mut blocks: Vec<Value> = Vec::new();
    let mut counters: Counters = HashMap::new();
    collect_header_blocks(ftr, numbering, &mut counters, rels, imgs, stats, &mut blocks);
    if !blocks.iter().any(header_block_has_content) {
        return None;
    }
    stats.footer_paragraphs = blocks.len();
    Some(json!({ "type": "doc", "content": blocks }))
}

/// Caminha o cabeçalho em ordem de documento coletando blocos:
///   - `w:p`   → parágrafo (pulando vazios — espaçadores do timbre);
///   - `w:tbl` → tabela (via `convert_table`), SEM descer nela de novo;
///   - `mc:AlternateContent` → processa SÓ um ramo (Choice, ou Fallback se
///     não houver Choice) pra NÃO duplicar o conteúdo do textbox/brasão —
///     essa duplicação era a causa da marca lateral aparecer 2× na importação;
///   - demais containers (txbxContent, sdtContent, drawing, etc.) → recursa
///     pra alcançar parágrafos/tabelas dentro de caixas de texto do timbre.
fn collect_header_blocks(
    node: roxmltree::Node,
    numbering: &Numbering,
    counters: &mut Counters,
    rels: &HashMap<String, String>,
    imgs: &mut Vec<PendingImage>,
    stats: &mut ImportStats,
    out: &mut Vec<Value>,
) {
    for c in node.children().filter(|n| n.is_element()) {
        match c.tag_name().name() {
            "p" => {
                // Texto PRÓPRIO do parágrafo (runs diretos). `convert_runs`
                // lê só `w:r > w:t` diretos, então não captura texto de
                // caixas aninhadas — por isso recursamos logo abaixo.
                if let Some(b) = header_paragraph(c) {
                    out.push(b);
                }
                // Caixas de texto / figuras do timbre ficam ANCORADAS dentro
                // de um `w:p` (run → drawing → txbxContent → parágrafos).
                // Recursa pra alcançá-las. Sem duplicar o texto do próprio
                // parágrafo (já lido acima e ausente do subtree do textbox).
                collect_header_blocks(c, numbering, counters, rels, imgs, stats, out);
            }
            "tbl" => {
                out.push(convert_table(c, numbering, counters, rels, imgs, stats))
            }
            // Caixa de texto do timbre. Se for VERTICAL (marca lateral),
            // reconstrói como `text_box` vertical do SICRO; senão, recursa
            // (achata os parágrafos como antes).
            "txbxContent" => match textbox_orientation(c) {
                Some(orient) => out.push(build_vertical_textbox(c, orient, stats)),
                None => {
                    collect_header_blocks(c, numbering, counters, rels, imgs, stats, out)
                }
            },
            // Desenho/imagem: se tem FIGURA (a:blip / v:imagedata) → brasão;
            // senão recursa (pode ser uma caixa de texto = shape sem blip).
            "drawing" | "pict" => {
                if let Some(mut fig) = picture_figure(c, rels, imgs) {
                    // Header/rodapé: corrige o Y page-relative do Word para a
                    // banda curta da região (vide `clamp_chrome_figure_y`).
                    clamp_chrome_figure_y(&mut fig);
                    stats.images += 1;
                    out.push(fig);
                } else {
                    collect_header_blocks(c, numbering, counters, rels, imgs, stats, out);
                }
            }
            "AlternateContent" => {
                if let Some(branch) =
                    child(c, "Choice").or_else(|| child(c, "Fallback"))
                {
                    collect_header_blocks(
                        branch, numbering, counters, rels, imgs, stats, out,
                    );
                }
            }
            _ => collect_header_blocks(c, numbering, counters, rels, imgs, stats, out),
        }
    }
}

/// Converte um `<w:p>` do cabeçalho em parágrafo ProseMirror. `None` quando
/// não há texto (espaçadores do timbre em volta do brasão).
fn header_paragraph(p: roxmltree::Node) -> Option<Value> {
    let ppr = child(p, "pPr");
    let align = ppr
        .and_then(|pp| child(pp, "jc"))
        .and_then(|j| attr(j, "val"))
        .map(map_align);
    let indent_cm = first_line_indent_cm(ppr);
    let inline = convert_runs(p); // ignora drawings (imagens)

    if !inline.iter().any(value_has_text) {
        return None;
    }
    let mut attrs = Map::new();
    if let Some(a) = &align {
        attrs.insert("textAlign".into(), json!(a));
    }
    if indent_cm > 0.0 {
        attrs.insert("first_line_indent_cm".into(), json!(round2(indent_cm)));
    }
    let mut node = Map::new();
    node.insert("type".into(), json!("paragraph"));
    if !attrs.is_empty() {
        node.insert("attrs".into(), Value::Object(attrs));
    }
    node.insert("content".into(), json!(inline));
    Some(Value::Object(node))
}

/// Detecta a direção do texto de uma caixa (`<w:txbxContent>`) subindo pelos
/// ancestrais (até 5 níveis):
///   - DrawingML: `<wps:bodyPr vert="vert270">` (irmão do `txbx`, sob o `wsp`).
///     `vert270` → ascendente (baixo→cima); outros valores → descendente.
///   - VML: `<v:textbox style="…mso-layout-flow-alt:bottom-to-top">`.
/// Retorna o `text_orientation` do SICRO, ou `None` quando é horizontal.
fn textbox_orientation(txbx_content: roxmltree::Node) -> Option<&'static str> {
    let mut cur = txbx_content.parent();
    for _ in 0..5 {
        let n = cur?;
        // VML: o pai costuma ser <v:textbox style="...">.
        if n.tag_name().name() == "textbox" {
            let s = attr(n, "style").unwrap_or("").to_ascii_lowercase();
            if s.contains("bottom-to-top") {
                return Some("vertical_up");
            }
            if s.contains("layout-flow:vertical") || s.contains("vertical-ideographic")
            {
                return Some("vertical_down");
            }
        }
        // DrawingML: bodyPr[@vert] em algum descendente deste ancestral.
        if let Some(v) = n
            .descendants()
            .find(|d| {
                d.is_element()
                    && d.tag_name().name() == "bodyPr"
                    && attr(*d, "vert").is_some()
            })
            .and_then(|d| attr(d, "vert"))
        {
            return Some(if v == "vert270" {
                "vertical_up"
            } else {
                "vertical_down"
            });
        }
        cur = n.parent();
    }
    None
}

/// Constrói um nó `text_box` do SICRO a partir de uma caixa de texto VERTICAL
/// do Word (marca lateral do timbre). Sem borda/preenchimento; posição e
/// altura são best-effort na margem esquerda (ajustáveis arrastando) — o foco
/// é trazer o TEXTO com a ORIENTAÇÃO certa.
fn build_vertical_textbox(
    txbx_content: roxmltree::Node,
    orientation: &str,
    stats: &mut ImportStats,
) -> Value {
    let mut content: Vec<Value> = Vec::new();
    for p in txbx_content
        .descendants()
        .filter(|d| d.is_element() && d.tag_name().name() == "p")
    {
        if let Some(b) = header_paragraph(p) {
            content.push(b);
        }
    }
    if content.is_empty() {
        content.push(json!({ "type": "paragraph" }));
    }
    stats.textboxes += 1;
    json!({
        "type": "text_box",
        "attrs": {
            "id": format!("tbx-imp-{}", stats.textboxes),
            "width_cm": 1.2,
            "height_cm": 12.0,
            "wrap_mode": "in_front",
            "wrap_x_cm": 0.2,
            "wrap_y_cm": 0.3,
            "rotation": 0,
            "border_enabled": false,
            "border_color": "#1f2937",
            "border_width": 1,
            "border_style": "solid",
            "fill_enabled": false,
            "fill_color": "#ffffff",
            "padding_cm": 0.1,
            "text_orientation": orientation,
        },
        "content": content,
    })
}

/// True se o nó ProseMirror (Value) contém algum text node não-vazio.
fn value_has_text(v: &Value) -> bool {
    if v.get("type").and_then(|t| t.as_str()) == Some("text") {
        return v
            .get("text")
            .and_then(|t| t.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
    }
    v.get("content")
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().any(value_has_text))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn heading_level_pt_and_en() {
        assert_eq!(heading_level("Heading1"), Some(1));
        assert_eq!(heading_level("Ttulo2"), Some(2));
        assert_eq!(heading_level("Título3"), Some(3));
        assert_eq!(heading_level("Heading9"), Some(3)); // clampa
        assert_eq!(heading_level("Legenda"), None);
    }

    #[test]
    fn align_mapping() {
        assert_eq!(map_align("both"), "justify");
        assert_eq!(map_align("center"), "center");
        assert_eq!(map_align("end"), "right");
        assert_eq!(map_align("qualquer"), "left");
    }

    #[test]
    fn counter_formats() {
        assert_eq!(format_counter(1, "decimal"), "1");
        assert_eq!(format_counter(3, "lowerLetter"), "c");
        assert_eq!(format_counter(27, "lowerLetter"), "aa");
        assert_eq!(format_counter(2, "upperLetter"), "B");
        assert_eq!(format_counter(4, "lowerRoman"), "iv");
        assert_eq!(format_counter(9, "upperRoman"), "IX");
    }

    #[test]
    fn multilevel_label_3_1() {
        let xml = r#"<?xml version="1.0"?>
        <w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:abstractNum w:abstractNumId="0">
            <w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl>
            <w:lvl w:ilvl="1"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1.%2"/></w:lvl>
          </w:abstractNum>
          <w:num w:numId="5"><w:abstractNumId w:val="0"/></w:num>
        </w:numbering>"#;
        let numbering = parse_numbering(Some(xml));
        let mut c: Counters = HashMap::new();
        let lbl = |k: &mut Counters, lv: u8| match resolve_list(&numbering, k, "5", lv) {
            Some(ListKind::Numbered { label, .. }) => label,
            _ => panic!("esperava Numbered"),
        };
        assert_eq!(lbl(&mut c, 0), "1."); // 1º item nível 0
        assert_eq!(lbl(&mut c, 1), "1.1"); // subitem
        assert_eq!(lbl(&mut c, 1), "1.2"); // próximo subitem
        assert_eq!(lbl(&mut c, 0), "2."); // volta ao nível 0 (reseta o 1)
        assert_eq!(lbl(&mut c, 1), "2.1"); // subitem recomeça
    }

    #[test]
    fn multilevel_with_start_3() {
        // Igual ao laudo real #2: nível 0 começa em 3 (w:start=3); um item
        // no nível 1 (sem item de nível 0 antes) deve dar "3.1".
        let xml = r#"<?xml version="1.0"?>
        <w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:abstractNum w:abstractNumId="10">
            <w:lvl w:ilvl="0"><w:start w:val="3"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1"/></w:lvl>
            <w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1.%2"/></w:lvl>
          </w:abstractNum>
          <w:num w:numId="43"><w:abstractNumId w:val="10"/></w:num>
        </w:numbering>"#;
        let numbering = parse_numbering(Some(xml));
        let mut c: Counters = HashMap::new();
        let lbl = |k: &mut Counters| match resolve_list(&numbering, k, "43", 1) {
            Some(ListKind::Numbered { label, .. }) => label,
            _ => panic!("esperava Numbered"),
        };
        assert_eq!(lbl(&mut c), "3.1");
        assert_eq!(lbl(&mut c), "3.2");
    }

    // -- V1: importação de tabelas -----------------------------------------

    #[test]
    fn table_converts_to_table_node() {
        let xml = r#"<?xml version="1.0"?>
        <w:tbl xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:tr>
            <w:tc><w:p><w:r><w:t>Registrado em</w:t></w:r></w:p></w:tc>
            <w:tc><w:p><w:r><w:t>LAUDO 123</w:t></w:r></w:p></w:tc>
            <w:tc><w:p><w:r><w:t>Folha 1</w:t></w:r></w:p></w:tc>
          </w:tr>
        </w:tbl>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let numbering = parse_numbering(None);
        let mut counters: Counters = HashMap::new();
        let mut stats = ImportStats::default();
        let table = convert_table(
            doc.root_element(),
            &numbering,
            &mut counters,
            &HashMap::new(),
            &mut Vec::new(),
            &mut stats,
        );

        assert_eq!(table["type"], "table");
        let rows = table["content"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        let cells = rows[0]["content"].as_array().unwrap();
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0]["type"], "tableCell");
        assert_eq!(cells[0]["attrs"]["colspan"], 1);
        // 1º parágrafo da 1ª célula carrega o texto.
        assert_eq!(
            cells[0]["content"][0]["content"][0]["text"]
                .as_str()
                .unwrap(),
            "Registrado em"
        );
        assert_eq!(stats.tables, 1);
    }

    #[test]
    fn table_cell_gridspan_becomes_colspan_and_empty_gets_paragraph() {
        let xml = r#"<?xml version="1.0"?>
        <w:tbl xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:tr>
            <w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Largo</w:t></w:r></w:p></w:tc>
            <w:tc><w:p/></w:tc>
          </w:tr>
        </w:tbl>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let numbering = parse_numbering(None);
        let mut counters: Counters = HashMap::new();
        let mut stats = ImportStats::default();
        let table = convert_table(
            doc.root_element(),
            &numbering,
            &mut counters,
            &HashMap::new(),
            &mut Vec::new(),
            &mut stats,
        );
        let cells = table["content"][0]["content"].as_array().unwrap();
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0]["attrs"]["colspan"], 2);
        // Célula vazia recebe ≥1 parágrafo (TipTap exige block+).
        assert_eq!(cells[1]["content"].as_array().unwrap().len(), 1);
        assert_eq!(cells[1]["content"][0]["type"], "paragraph");
    }

    #[test]
    fn table_vmerge_continuation_is_skipped() {
        let xml = r#"<?xml version="1.0"?>
        <w:tbl xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:tr>
            <w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>Topo</w:t></w:r></w:p></w:tc>
            <w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc>
          </w:tr>
          <w:tr>
            <w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>
            <w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc>
          </w:tr>
        </w:tbl>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let numbering = parse_numbering(None);
        let mut counters: Counters = HashMap::new();
        let mut stats = ImportStats::default();
        let table = convert_table(
            doc.root_element(),
            &numbering,
            &mut counters,
            &HashMap::new(),
            &mut Vec::new(),
            &mut stats,
        );
        let rows = table["content"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        // 2ª linha: a célula de continuação (vMerge sem restart) some →
        // sobra só a célula "B".
        let r2 = rows[1]["content"].as_array().unwrap();
        assert_eq!(r2.len(), 1);
        assert_eq!(r2[0]["content"][0]["content"][0]["text"], "B");
    }

    #[test]
    fn header_imports_table_and_dedups_alternate_content() {
        // Simula o cabeçalho real: caixa de texto (marca lateral) ANCORADA
        // dentro de um w:p via mc:AlternateContent (Choice + Fallback
        // idênticos), seguida do bloco de registro como tabela.
        let xml = r#"<?xml version="1.0"?>
        <w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
               xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006">
          <w:p>
            <w:r>
              <mc:AlternateContent>
                <mc:Choice Requires="wps"><w:p><w:r><w:t>MARCA LATERAL</w:t></w:r></w:p></mc:Choice>
                <mc:Fallback><w:p><w:r><w:t>MARCA LATERAL</w:t></w:r></w:p></mc:Fallback>
              </mc:AlternateContent>
            </w:r>
          </w:p>
          <w:tbl>
            <w:tr>
              <w:tc><w:p><w:r><w:t>Registrado</w:t></w:r></w:p></w:tc>
              <w:tc><w:p><w:r><w:t>Laudo 9</w:t></w:r></w:p></w:tc>
            </w:tr>
          </w:tbl>
        </w:hdr>"#;
        let numbering = parse_numbering(None);
        let mut stats = ImportStats::default();
        let doc =
            parse_header_xml(xml, &numbering, &HashMap::new(), &mut Vec::new(), &mut stats)
                .expect("header com texto");
        let serialized = serde_json::to_string(&doc).unwrap();

        // mc:AlternateContent NÃO pode duplicar (Choice + Fallback contavam 2×).
        assert_eq!(
            serialized.matches("MARCA LATERAL").count(),
            1,
            "AlternateContent duplicou o conteúdo"
        );
        // O bloco de registro virou UMA tabela top-level.
        let blocks = doc["content"].as_array().unwrap();
        let n_tables = blocks.iter().filter(|b| b["type"] == "table").count();
        assert_eq!(n_tables, 1);
        assert_eq!(stats.tables, 1);
    }

    // -- V2: marca lateral vertical (caixa de texto) -----------------------

    #[test]
    fn vertical_textbox_drawingml_detected() {
        // wps:wsp com bodyPr vert="vert270" → caixa vertical ascendente.
        let xml = r#"<?xml version="1.0"?>
        <w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
               xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
          <w:p>
            <wps:wsp>
              <wps:txbx>
                <w:txbxContent>
                  <w:p><w:r><w:t>MARCA LATERAL</w:t></w:r></w:p>
                </w:txbxContent>
              </wps:txbx>
              <wps:bodyPr vert="vert270"/>
            </wps:wsp>
          </w:p>
        </w:hdr>"#;
        let numbering = parse_numbering(None);
        let mut stats = ImportStats::default();
        let doc =
            parse_header_xml(xml, &numbering, &HashMap::new(), &mut Vec::new(), &mut stats)
                .expect("header com texto");
        let blocks = doc["content"].as_array().unwrap();
        let tb = blocks
            .iter()
            .find(|b| b["type"] == "text_box")
            .expect("esperava um text_box vertical");
        assert_eq!(tb["attrs"]["text_orientation"], "vertical_up");
        assert!(tb["attrs"]["border_enabled"] == false);
        let serialized = serde_json::to_string(&doc).unwrap();
        assert!(serialized.contains("MARCA LATERAL"));
        assert_eq!(stats.textboxes, 1);
    }

    #[test]
    fn vertical_textbox_vml_detected() {
        // VML <v:textbox style="...bottom-to-top"> → caixa vertical ascendente.
        let xml = r#"<?xml version="1.0"?>
        <w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
               xmlns:v="urn:schemas-microsoft-com:vml">
          <w:p>
            <w:pict>
              <v:shape>
                <v:textbox style="layout-flow:vertical;mso-layout-flow-alt:bottom-to-top">
                  <w:txbxContent>
                    <w:p><w:r><w:t>MARCA VML</w:t></w:r></w:p>
                  </w:txbxContent>
                </v:textbox>
              </v:shape>
            </w:pict>
          </w:p>
        </w:hdr>"#;
        let numbering = parse_numbering(None);
        let mut stats = ImportStats::default();
        let doc =
            parse_header_xml(xml, &numbering, &HashMap::new(), &mut Vec::new(), &mut stats)
                .expect("header com texto");
        let tb = doc["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["type"] == "text_box")
            .expect("esperava text_box VML");
        assert_eq!(tb["attrs"]["text_orientation"], "vertical_up");
        assert_eq!(stats.textboxes, 1);
    }

    #[test]
    fn horizontal_textbox_flattens_not_textbox() {
        // bodyPr SEM vert → caixa horizontal: achata em parágrafo, sem text_box.
        let xml = r#"<?xml version="1.0"?>
        <w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
               xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
          <w:p>
            <wps:wsp>
              <wps:txbx>
                <w:txbxContent>
                  <w:p><w:r><w:t>CAIXA HORIZONTAL</w:t></w:r></w:p>
                </w:txbxContent>
              </wps:txbx>
              <wps:bodyPr/>
            </wps:wsp>
          </w:p>
        </w:hdr>"#;
        let numbering = parse_numbering(None);
        let mut stats = ImportStats::default();
        let doc =
            parse_header_xml(xml, &numbering, &HashMap::new(), &mut Vec::new(), &mut stats)
                .expect("header com texto");
        let blocks = doc["content"].as_array().unwrap();
        assert!(
            blocks.iter().all(|b| b["type"] != "text_box"),
            "caixa horizontal não deve virar text_box"
        );
        assert_eq!(stats.textboxes, 0);
        let serialized = serde_json::to_string(&doc).unwrap();
        assert!(serialized.contains("CAIXA HORIZONTAL"));
    }

    // -- V3: extração de imagens (w:drawing → figure) ----------------------

    #[test]
    fn web_image_ext_filter() {
        assert!(is_web_image_ext("png"));
        assert!(is_web_image_ext("jpeg"));
        assert!(!is_web_image_ext("emf"));
        assert!(!is_web_image_ext("wmf"));
    }

    #[test]
    fn rels_path_derivation() {
        assert_eq!(rels_path_for("word/header1.xml"), "word/_rels/header1.xml.rels");
        assert_eq!(
            rels_path_for("word/document.xml"),
            "word/_rels/document.xml.rels"
        );
    }

    #[test]
    fn picture_figure_png_emits_figure_and_pending() {
        let xml = r#"<?xml version="1.0"?>
        <w:drawing xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                   xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
                   xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
                   xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <wp:inline>
            <wp:extent cx="3600000" cy="1200000"/>
            <a:graphic><a:graphicData><a:blip r:embed="rId1"/></a:graphicData></a:graphic>
          </wp:inline>
        </w:drawing>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let mut rels = HashMap::new();
        rels.insert("rId1".to_string(), "media/image1.png".to_string());
        let mut imgs: Vec<PendingImage> = Vec::new();
        let fig =
            picture_figure(doc.root_element(), &rels, &mut imgs).expect("esperava figure");
        assert_eq!(fig["type"], "figure");
        assert_eq!(fig["attrs"]["relative_path"], "__docximport__/img1.png");
        assert_eq!(fig["attrs"]["width"], "10.00cm"); // 3.600.000 EMU / 360.000
        assert_eq!(fig["content"][0]["type"], "figcaption"); // schema exige
        assert_eq!(imgs.len(), 1);
        assert_eq!(imgs[0].zip_path, "word/media/image1.png");
    }

    #[test]
    fn picture_figure_emf_is_unsupported() {
        let xml = r#"<?xml version="1.0"?>
        <w:drawing xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                   xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
                   xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <a:blip r:embed="rId9"/>
        </w:drawing>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let mut rels = HashMap::new();
        rels.insert("rId9".to_string(), "media/image9.emf".to_string());
        let mut imgs: Vec<PendingImage> = Vec::new();
        assert!(picture_figure(doc.root_element(), &rels, &mut imgs).is_none());
        assert!(imgs.is_empty());
    }

    #[test]
    fn picture_figure_inline_has_no_float_attrs() {
        // Imagem inline NÃO deve ganhar wrap_mode/rotation (regressão).
        let xml = r#"<?xml version="1.0"?>
        <w:drawing xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                   xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
                   xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
                   xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <wp:inline>
            <wp:extent cx="3600000" cy="1200000"/>
            <a:graphic><a:graphicData><a:blip r:embed="rId1"/></a:graphicData></a:graphic>
          </wp:inline>
        </w:drawing>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let mut rels = HashMap::new();
        rels.insert("rId1".to_string(), "media/image1.png".to_string());
        let mut imgs: Vec<PendingImage> = Vec::new();
        let fig = picture_figure(doc.root_element(), &rels, &mut imgs).unwrap();
        assert!(fig["attrs"].get("wrap_mode").is_none());
        assert!(fig["attrs"].get("rotation").is_none());
        assert!(fig["attrs"].get("wrap_x_cm").is_none());
    }

    #[test]
    fn picture_figure_anchored_drawing_extracts_position_and_rotation() {
        // Brasão FLUTUANTE: âncora com posOffset (EMU) + rotação (60000-avos)
        // + behindDoc → figure em modo `behind` com posição/rotação reais.
        // 540000 EMU / 360000 = 1.5cm ; 9720000 / 360000 = 27cm ;
        // rot 16200000 / 60000 = 270°.
        let xml = r#"<?xml version="1.0"?>
        <w:drawing xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                   xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
                   xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
                   xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"
                   xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <wp:anchor behindDoc="1">
            <wp:positionH relativeFrom="page"><wp:posOffset>540000</wp:posOffset></wp:positionH>
            <wp:positionV relativeFrom="page"><wp:posOffset>9720000</wp:posOffset></wp:positionV>
            <wp:extent cx="1080000" cy="3600000"/>
            <wp:wrapNone/>
            <a:graphic><a:graphicData>
              <pic:pic><pic:spPr>
                <a:xfrm rot="16200000"><a:off x="0" y="0"/><a:ext cx="1080000" cy="3600000"/></a:xfrm>
              </pic:spPr></pic:pic>
              <a:blip r:embed="rId5"/>
            </a:graphicData></a:graphic>
          </wp:anchor>
        </w:drawing>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let mut rels = HashMap::new();
        rels.insert("rId5".to_string(), "media/image5.png".to_string());
        let mut imgs: Vec<PendingImage> = Vec::new();
        let fig = picture_figure(doc.root_element(), &rels, &mut imgs).unwrap();
        assert_eq!(fig["attrs"]["wrap_mode"], "behind");
        assert_eq!(fig["attrs"]["wrap_x_cm"], 1.5);
        assert_eq!(fig["attrs"]["wrap_y_cm"], 27.0);
        assert_eq!(fig["attrs"]["rotation"], 270.0);
    }

    #[test]
    fn picture_figure_vml_shape_extracts_position_and_rotation() {
        // VML (legado): <v:shape style="position:absolute;left/top;rotation;z-index">.
        // 1.5cm/27cm direto; rotation 90; z-index<0 → behind.
        let xml = r#"<?xml version="1.0"?>
        <w:pict xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                xmlns:v="urn:schemas-microsoft-com:vml"
                xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <v:shape style="position:absolute;left:1.5cm;top:27cm;width:1cm;height:10cm;rotation:90;z-index:-5">
            <v:imagedata r:id="rId7"/>
          </v:shape>
        </w:pict>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let mut rels = HashMap::new();
        rels.insert("rId7".to_string(), "media/image7.png".to_string());
        let mut imgs: Vec<PendingImage> = Vec::new();
        let fig = picture_figure(doc.root_element(), &rels, &mut imgs).unwrap();
        assert_eq!(fig["attrs"]["wrap_mode"], "behind");
        assert_eq!(fig["attrs"]["wrap_x_cm"], 1.5);
        assert_eq!(fig["attrs"]["wrap_y_cm"], 27.0);
        assert_eq!(fig["attrs"]["rotation"], 90.0);
    }

    #[test]
    fn inline_photo_with_floating_textbox_stays_inline() {
        // Caso "Ponto de Impacto": FOTO inline (wp:inline + blip) E uma CAIXA DE
        // TEXTO flutuante (wp:anchor, posV relativeFrom paragraph) no MESMO w:p.
        // A figura da foto NÃO pode herdar a geometria flutuante do anchor irmão
        // (senão vira flutuante e vai parar no topo/cabeçalho).
        let xml = r#"<?xml version="1.0"?>
        <w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
             xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
             xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
             xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"
             xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <w:r><w:drawing><wp:inline>
            <wp:extent cx="3600000" cy="2700000"/>
            <a:graphic><a:graphicData><a:blip r:embed="rId5"/></a:graphicData></a:graphic>
          </wp:inline></w:drawing></w:r>
          <w:r><w:drawing><wp:anchor behindDoc="0">
            <wp:positionH relativeFrom="column"><wp:posOffset>1563011</wp:posOffset></wp:positionH>
            <wp:positionV relativeFrom="paragraph"><wp:posOffset>3107911</wp:posOffset></wp:positionV>
            <a:graphic><a:graphicData><wps:wsp><wps:txbx><w:txbxContent>
              <w:p><w:r><w:t>Ponto de Impacto</w:t></w:r></w:p>
            </w:txbxContent></wps:txbx></wps:wsp></a:graphicData></a:graphic>
          </wp:anchor></w:drawing></w:r>
        </w:p>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let mut rels = HashMap::new();
        rels.insert("rId5".to_string(), "media/image5.png".to_string());
        let mut imgs: Vec<PendingImage> = Vec::new();
        let fig = picture_figure(doc.root_element(), &rels, &mut imgs).expect("acha a foto");
        assert!(
            fig["attrs"].get("wrap_mode").is_none(),
            "foto inline NÃO pode virar flutuante por causa da caixa irmã"
        );
        assert!(fig["attrs"].get("wrap_y_cm").is_none(), "sem wrap_y_cm");
        assert_eq!(imgs.len(), 1, "exatamente uma imagem pendente (a foto)");
    }

    #[test]
    fn floating_annotation_without_image_has_no_blip() {
        // Parágrafo só com caixa de texto/elipse flutuante (sem foto): não há
        // blip → blip_and_owner None → convert_paragraph emite parágrafo vazio
        // (não o placeholder "[imagem não importada]").
        let xml = r#"<?xml version="1.0"?>
        <w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
             xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
             xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
             xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
          <w:r><w:drawing><wp:anchor behindDoc="0">
            <a:graphic><a:graphicData><wps:wsp>
              <wps:spPr><a:prstGeom prst="ellipse"/></wps:spPr>
            </wps:wsp></a:graphicData></a:graphic>
          </wp:anchor></w:drawing></w:r>
        </w:p>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        assert!(
            blip_and_owner(doc.root_element()).is_none(),
            "anotação flutuante sem imagem não tem blip"
        );
    }

    #[test]
    fn clamp_chrome_figure_y_resets_page_relative_but_keeps_band() {
        // Y page-relative (27cm) é grande demais pra banda → vai pro topo (0.2).
        let mut big = json!({ "type": "figure", "attrs": { "wrap_y_cm": 27.0 } });
        clamp_chrome_figure_y(&mut big);
        assert_eq!(big["attrs"]["wrap_y_cm"], 0.2);
        // Y já compatível com a banda (1.0cm) é preservado.
        let mut small = json!({ "type": "figure", "attrs": { "wrap_y_cm": 1.0 } });
        clamp_chrome_figure_y(&mut small);
        assert_eq!(small["attrs"]["wrap_y_cm"], 1.0);
        // Sem wrap_y_cm → no-op (não cria o attr).
        let mut none = json!({ "type": "figure", "attrs": { "width": "5cm" } });
        clamp_chrome_figure_y(&mut none);
        assert!(none["attrs"].get("wrap_y_cm").is_none());
    }

    #[test]
    fn parse_docx_extracts_body_image() {
        use std::io::Write;
        let doc_xml = r#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                    xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
                    xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
                    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <w:body>
            <w:p><w:r><w:drawing><wp:inline>
              <wp:extent cx="3600000" cy="1200000"/>
              <a:graphic><a:graphicData><a:blip r:embed="rId1"/></a:graphicData></a:graphic>
            </wp:inline></w:drawing></w:r></w:p>
          </w:body>
        </w:document>"#;
        let rels_xml = r#"<?xml version="1.0"?>
        <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
          <Relationship Id="rId1" Type="img" Target="media/image1.png"/>
        </Relationships>"#;
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            zw.start_file("word/document.xml", zip::write::FileOptions::default())
                .unwrap();
            zw.write_all(doc_xml.as_bytes()).unwrap();
            zw.start_file(
                "word/_rels/document.xml.rels",
                zip::write::FileOptions::default(),
            )
            .unwrap();
            zw.write_all(rels_xml.as_bytes()).unwrap();
            zw.start_file("word/media/image1.png", zip::write::FileOptions::default())
                .unwrap();
            zw.write_all(b"\x89PNG\r\n\x1a\nFAKEPNGDATA").unwrap();
            zw.finish().unwrap();
        }
        let parsed = parse_docx_bytes(&buf).expect("parse falhou");
        assert_eq!(parsed.images.len(), 1, "deveria coletar 1 imagem");
        assert_eq!(parsed.images[0].name, "img1.png");
        assert!(!parsed.images[0].bytes.is_empty());
        let blocks = parsed.content["content"].as_array().unwrap();
        let fig = blocks
            .iter()
            .find(|b| b["type"] == "figure")
            .expect("esperava um figure no corpo");
        assert_eq!(fig["attrs"]["relative_path"], "__docximport__/img1.png");
    }

    #[test]
    fn parse_docx_ignores_chrome_by_default() {
        use std::io::Write;
        // Importação de cabeçalho/rodapé está DESLIGADA por ora
        // (IMPORT_CHROME = false). Mesmo com um footer1.xml contendo o brasão
        // da PC, o importador deve IGNORAR o rodapé (footer_content None) e NÃO
        // extrair a imagem do rodapé — só o corpo entra.
        let doc_xml = r#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <w:body>
            <w:p><w:r><w:t>corpo</w:t></w:r></w:p>
            <w:sectPr>
              <w:footerReference w:type="default" r:id="rIdF"/>
            </w:sectPr>
          </w:body>
        </w:document>"#;
        let doc_rels = r#"<?xml version="1.0"?>
        <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
          <Relationship Id="rIdF" Type="footer" Target="footer1.xml"/>
        </Relationships>"#;
        let footer_xml = r#"<?xml version="1.0"?>
        <w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
               xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
               xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
               xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <w:p><w:r><w:drawing><wp:inline>
            <wp:extent cx="900000" cy="900000"/>
            <a:graphic><a:graphicData><a:blip r:embed="rIdImg"/></a:graphicData></a:graphic>
          </wp:inline></w:drawing></w:r></w:p>
        </w:ftr>"#;
        let footer_rels = r#"<?xml version="1.0"?>
        <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
          <Relationship Id="rIdImg" Type="img" Target="media/pc.png"/>
        </Relationships>"#;
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let put = |zw: &mut zip::ZipWriter<_>, name: &str, data: &[u8]| {
                zw.start_file(name, zip::write::FileOptions::default()).unwrap();
                zw.write_all(data).unwrap();
            };
            put(&mut zw, "word/document.xml", doc_xml.as_bytes());
            put(&mut zw, "word/_rels/document.xml.rels", doc_rels.as_bytes());
            put(&mut zw, "word/footer1.xml", footer_xml.as_bytes());
            put(&mut zw, "word/_rels/footer1.xml.rels", footer_rels.as_bytes());
            put(&mut zw, "word/media/pc.png", b"\x89PNG\r\n\x1a\nFAKE");
            zw.finish().unwrap();
        }
        let parsed = parse_docx_bytes(&buf).expect("parse falhou");
        // Chrome (header/footer) desligado: rodapé e cabeçalho NÃO importados.
        assert!(
            parsed.footer_content.is_none(),
            "rodapé NÃO deve ser importado (IMPORT_CHROME=false)"
        );
        assert!(
            parsed.header_content.is_none(),
            "cabeçalho NÃO deve ser importado (IMPORT_CHROME=false)"
        );
        // A imagem do rodapé (brasão) NÃO deve ser extraída — sem órfã.
        assert!(
            !parsed.images.iter().any(|i| i.name.ends_with(".png")),
            "imagem do rodapé NÃO deve ser coletada quando o rodapé é ignorado"
        );
        // O CORPO continua importado normalmente.
        let body = parsed.content["content"].as_array().unwrap();
        assert!(
            !body.is_empty(),
            "o corpo do documento deve continuar sendo importado"
        );
    }

    #[test]
    fn parse_docx_reads_line_spacing() {
        use std::io::Write;
        // 1º parágrafo: 1,5 linhas (w:line=360, auto). 2º: simples (240).
        let doc_xml = r#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p><w:pPr><w:spacing w:line="360" w:lineRule="auto"/></w:pPr>
              <w:r><w:t>um e meio</w:t></w:r></w:p>
            <w:p><w:pPr><w:spacing w:line="240" w:lineRule="auto"/></w:pPr>
              <w:r><w:t>simples</w:t></w:r></w:p>
          </w:body>
        </w:document>"#;
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            zw.start_file("word/document.xml", zip::write::FileOptions::default())
                .unwrap();
            zw.write_all(doc_xml.as_bytes()).unwrap();
            zw.finish().unwrap();
        }
        let parsed = parse_docx_bytes(&buf).expect("parse falhou");
        let blocks = parsed.content["content"].as_array().unwrap();
        // 1,5 → line_height 1.5; simples → SEM line_height (usa default ≈1,15).
        assert_eq!(
            blocks[0]["attrs"]["line_height"],
            serde_json::json!(1.5),
            "parágrafo 1,5 deve importar line_height=1.5"
        );
        assert!(
            blocks[1]
                .get("attrs")
                .and_then(|a| a.get("line_height"))
                .is_none(),
            "parágrafo simples NÃO deve setar line_height"
        );
    }

    #[test]
    fn omml_frac_sup_rad_to_latex() {
        const NS: &str = r#"xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math""#;
        let frac = format!(
            r#"<m:oMath {NS}><m:f><m:num><m:r><m:t>L</m:t></m:r></m:num><m:den><m:r><m:t>P</m:t></m:r></m:den></m:f></m:oMath>"#
        );
        let d = roxmltree::Document::parse(&frac).unwrap();
        assert_eq!(omml_to_latex(d.root_element()).trim(), r"\frac{L}{P}");

        let sup = format!(
            r#"<m:oMath {NS}><m:sSup><m:e><m:r><m:t>x</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:oMath>"#
        );
        let d2 = roxmltree::Document::parse(&sup).unwrap();
        assert_eq!(omml_to_latex(d2.root_element()).trim(), r"{x}^{2}");

        let rad = format!(
            r#"<m:oMath {NS}><m:rad><m:deg/><m:e><m:r><m:t>2</m:t></m:r></m:e></m:rad></m:oMath>"#
        );
        let d3 = roxmltree::Document::parse(&rad).unwrap();
        assert_eq!(omml_to_latex(d3.root_element()).trim(), r"\sqrt{2}");
    }

    #[test]
    fn omml_nary_sum_to_latex() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:nary><m:naryPr><m:chr m:val="∑"/></m:naryPr><m:sub><m:r><m:t>i</m:t></m:r></m:sub><m:sup><m:r><m:t>n</m:t></m:r></m:sup><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>"#;
        let d = roxmltree::Document::parse(xml).unwrap();
        let latex = omml_to_latex(d.root_element());
        assert!(latex.contains("\\sum"), "operador soma: {latex}");
        assert!(latex.contains("_{i}"), "índice inferior: {latex}");
        assert!(latex.contains("^{n}"), "índice superior: {latex}");
    }

    #[test]
    fn parse_docx_imports_math_block_and_inline() {
        use std::io::Write;
        // 1º parágrafo: display math (oMathPara) → mathBlock.
        // 2º parágrafo: texto + oMath inline + texto → mathInline preservando o texto.
        let doc_xml = r#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                    xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">
          <w:body>
            <w:p><m:oMathPara><m:oMath><m:f>
              <m:num><m:r><m:t>L</m:t></m:r></m:num>
              <m:den><m:r><m:t>P</m:t></m:r></m:den>
            </m:f></m:oMath></m:oMathPara></w:p>
            <w:p>
              <w:r><w:t>valor </w:t></w:r>
              <m:oMath><m:rad><m:deg/><m:e><m:r><m:t>2</m:t></m:r></m:e></m:rad></m:oMath>
              <w:r><w:t> px</w:t></w:r>
            </w:p>
          </w:body>
        </w:document>"#;
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            zw.start_file("word/document.xml", zip::write::FileOptions::default())
                .unwrap();
            zw.write_all(doc_xml.as_bytes()).unwrap();
            zw.finish().unwrap();
        }
        let parsed = parse_docx_bytes(&buf).expect("parse falhou");
        let blocks = parsed.content["content"].as_array().unwrap();
        // Bloco 0 = mathBlock (display) com o LaTeX da fração.
        assert_eq!(blocks[0]["type"], "mathBlock");
        assert_eq!(
            blocks[0]["attrs"]["latex"],
            serde_json::json!(r"\frac{L}{P}")
        );
        // Bloco 1 = paragraph com texto + mathInline (√2) + texto.
        assert_eq!(blocks[1]["type"], "paragraph");
        let content = blocks[1]["content"].as_array().unwrap();
        let has_inline = content.iter().any(|n| {
            n["type"] == "mathInline" && n["attrs"]["latex"] == serde_json::json!(r"\sqrt{2}")
        });
        assert!(has_inline, "esperava mathInline √2 no parágrafo: {content:?}");
        // O texto ao redor da fórmula inline é preservado.
        let has_text = content
            .iter()
            .any(|n| n["type"] == "text" && n["text"] == serde_json::json!("valor "));
        assert!(has_text, "texto ao redor da fórmula deve ser preservado");
    }

    #[test]
    fn legend_merges_into_figure_figcaption() {
        // Figura + legenda "Figura 1 - …" logo abaixo → uma figura só, com a
        // legenda no figcaption e SEM o prefixo "Figura 1 -" (o SICRO numera).
        let blocks = vec![
            json!({
                "type": "figure",
                "attrs": { "kind": "image" },
                "content": [{ "type": "figcaption" }],
            }),
            json!({
                "type": "paragraph",
                "attrs": { "laudoStyle": "legenda" },
                "content": [{
                    "type": "text",
                    "text": "Figura 1 - Visão anterior esquerda de V1.",
                }],
            }),
        ];
        let out = merge_legends_into_figures(blocks);
        assert_eq!(out.len(), 1, "a legenda deve ser absorvida pela figura");
        assert_eq!(out[0]["type"], "figure");
        assert_eq!(
            out[0]["content"][0]["content"][0]["text"],
            serde_json::json!("Visão anterior esquerda de V1."),
            "prefixo 'Figura 1 -' removido e texto vira figcaption"
        );
    }

    #[test]
    fn strip_figure_label_prefix_variants() {
        assert_eq!(strip_figure_label_prefix("Figura 1 - Vista A"), "Vista A");
        assert_eq!(strip_figure_label_prefix("Figura 12 — Vista B"), "Vista B");
        assert_eq!(strip_figure_label_prefix("Fig. 3: Vista C"), "Vista C");
        // Sem número → não mexe.
        assert_eq!(strip_figure_label_prefix("Figura geral"), "Figura geral");
        // Texto comum → intacto.
        assert_eq!(strip_figure_label_prefix("Vista lateral"), "Vista lateral");
    }
}

// ---------------------------------------------------------------------------
// Numeração multinível (numbering.xml)

#[derive(Debug, Default, Clone)]
struct LevelDef {
    num_fmt: String,
    lvl_text: String,
    /// Valor inicial do contador (`w:start`, default 1). Muitos laudos
    /// começam a subnumeração em 3 ("3.1") via `<w:start w:val="3">`.
    start: i64,
    left_cm: f64,
    hanging_cm: f64,
}

#[derive(Debug, Default)]
struct Numbering {
    /// numId → abstractNumId.
    num_to_abs: HashMap<String, String>,
    /// abstractNumId → (ilvl → definição do nível).
    abs_levels: HashMap<String, HashMap<u8, LevelDef>>,
}

/// Contadores correntes por (abstractNumId → ilvl → valor), mutados durante
/// o walk em ordem de documento pra reproduzir a numeração do Word.
type Counters = HashMap<String, HashMap<u8, i64>>;

enum ListKind {
    Bullet,
    Numbered {
        label: String,
        left_cm: f64,
        hanging_cm: f64,
    },
}

/// `numbering.xml` → mapa de definições. Lê, por nível, `numFmt`, `lvlText`
/// e os recuos (`w:ind` left/hanging), além do mapa numId → abstractNumId.
fn parse_numbering(xml: Option<&str>) -> Numbering {
    let mut out = Numbering::default();
    let Some(xml) = xml else {
        return out;
    };
    let xml = xml.trim_start_matches('\u{feff}');
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return out;
    };
    let root = doc.root_element();

    for an in root
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "abstractNum")
    {
        let Some(aid) = attr(an, "abstractNumId") else {
            continue;
        };
        let mut levels: HashMap<u8, LevelDef> = HashMap::new();
        for lvl in an
            .children()
            .filter(|c| c.is_element() && c.tag_name().name() == "lvl")
        {
            let ilvl: u8 = attr(lvl, "ilvl").and_then(|v| v.parse().ok()).unwrap_or(0);
            let num_fmt = child(lvl, "numFmt")
                .and_then(|f| attr(f, "val"))
                .unwrap_or("decimal")
                .to_string();
            let lvl_text = child(lvl, "lvlText")
                .and_then(|t| attr(t, "val"))
                .unwrap_or("")
                .to_string();
            let start: i64 = child(lvl, "start")
                .and_then(|s| attr(s, "val"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            let (mut left_cm, mut hanging_cm) = (0.0, 0.0);
            if let Some(ind) = child(lvl, "ind") {
                left_cm = attr(ind, "left")
                    .or_else(|| attr(ind, "start"))
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|tw| tw / TWIPS_PER_CM)
                    .unwrap_or(0.0);
                hanging_cm = attr(ind, "hanging")
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|tw| tw / TWIPS_PER_CM)
                    .unwrap_or(0.0);
            }
            levels.insert(
                ilvl,
                LevelDef {
                    num_fmt,
                    lvl_text,
                    start,
                    left_cm,
                    hanging_cm,
                },
            );
        }
        out.abs_levels.insert(aid.to_string(), levels);
    }

    for num in root
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "num")
    {
        let Some(nid) = attr(num, "numId") else {
            continue;
        };
        if let Some(aid) = child(num, "abstractNumId").and_then(|a| attr(a, "val")) {
            out.num_to_abs.insert(nid.to_string(), aid.to_string());
        }
    }
    out
}

/// Resolve um item de lista (numId, ilvl) na forma de exibição. Numerados:
/// computa o rótulo multinível ("3.1", "a.", "ii") mantendo `counters` e
/// formatando via `lvlText`. Bullets → `Bullet` (SICRO usa bulletList nativo).
fn resolve_list(
    numbering: &Numbering,
    counters: &mut Counters,
    num_id: &str,
    ilvl: u8,
) -> Option<ListKind> {
    let abs = numbering.num_to_abs.get(num_id)?;
    let levels = numbering.abs_levels.get(abs)?;
    let level = levels.get(&ilvl)?;
    if level.num_fmt == "bullet" {
        return Some(ListKind::Bullet);
    }

    // Incrementa o contador deste nível e zera os mais profundos. Na
    // PRIMEIRA vez no nível, começa no `w:start` definido (ex.: 3).
    {
        let abs_counters = counters.entry(abs.clone()).or_default();
        let cur = match abs_counters.get(&ilvl) {
            Some(x) => x + 1,
            None => level.start,
        };
        abs_counters.insert(ilvl, cur);
        abs_counters.retain(|k, _| *k <= ilvl);
    }

    // Monta o rótulo substituindo %1..%9 pelos contadores formatados. Níveis
    // referenciados mas ainda não incrementados (ex.: o "3" do "3.1") usam o
    // `w:start` do próprio nível.
    let mut label = level.lvl_text.clone();
    for n in 1..=9u8 {
        let token = format!("%{n}");
        if !label.contains(&token) {
            continue;
        }
        let lvl_idx = n - 1;
        let start = levels.get(&lvl_idx).map(|l| l.start).unwrap_or(1);
        let val = counters
            .get(abs)
            .and_then(|m| m.get(&lvl_idx))
            .copied()
            .unwrap_or(start);
        let fmt = levels
            .get(&lvl_idx)
            .map(|l| l.num_fmt.as_str())
            .unwrap_or("decimal");
        label = label.replace(&token, &format_counter(val, fmt));
    }

    Some(ListKind::Numbered {
        label,
        left_cm: level.left_cm,
        hanging_cm: level.hanging_cm,
    })
}

fn format_counter(val: i64, fmt: &str) -> String {
    match fmt {
        "lowerLetter" => to_alpha(val, false),
        "upperLetter" => to_alpha(val, true),
        "lowerRoman" => to_roman(val, false),
        "upperRoman" => to_roman(val, true),
        // decimal / decimalZero / outros → número arábico.
        _ => val.to_string(),
    }
}

/// 1→a, 26→z, 27→aa (estilo planilha).
fn to_alpha(n: i64, upper: bool) -> String {
    if n <= 0 {
        return n.to_string();
    }
    let mut n = n;
    let mut s: Vec<u8> = Vec::new();
    while n > 0 {
        let r = ((n - 1) % 26) as u8;
        s.push(if upper { b'A' + r } else { b'a' + r });
        n = (n - 1) / 26;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

fn to_roman(n: i64, upper: bool) -> String {
    if n <= 0 || n > 3999 {
        return n.to_string();
    }
    let table = [
        (1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"),
        (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"),
        (5, "v"), (4, "iv"), (1, "i"),
    ];
    let mut n = n;
    let mut s = String::new();
    for (v, sym) in table {
        while n >= v {
            s.push_str(sym);
            n -= v;
        }
    }
    if upper {
        s.to_uppercase()
    } else {
        s
    }
}
