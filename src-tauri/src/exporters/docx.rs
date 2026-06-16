//! DOCX export — walks the TipTap JSON content and builds a docx-rs document.
//!
//! Spike C revision after first runtime feedback:
//!   - Paragraphs ALWAYS get at least one Run (Word silently drops paragraphs
//!     without runs).
//!   - Unknown nodes flow through `fallback_paragraph`, which recursively
//!     extracts any text in the subtree, so nothing in the .sicrodoc is lost.
//!   - `tableHeader` is now treated like `tableCell` (the TipTap table
//!     extension emits both kinds, and we don't want to drop the first row
//!     just because it's a header).
//!   - Block nodes nested inside a table cell (storyboard, figure, lists)
//!     are flattened into the cell as a sequence of paragraphs instead of
//!     being misrouted through the inline walker.
//!   - `render_list` no longer carries the dead code that was producing
//!     duplicate bullets.
//!
//! MVP 2 ajuste runtime 1.2:
//!   - Page margins are read from `envelope.layout.page.margins` (the same
//!     resolver used by the editor and the HTML/PDF renderer). When the
//!     envelope does not carry a per-laudo override, the institutional
//!     template default is applied. Values in cm are converted to twips
//!     (1 cm = 567 twips) for docx-rs `PageMargin`.
//!
//! MVP 4 — evidência:
//!   - Figures with `attrs.relative_path` are embedded as real PNG/JPEG via
//!     docx-rs `Pic`. The asset bytes are read from `<workspace>/<rel>` so
//!     `Word`/LibreOffice see actual images, not placeholders. When the file
//!     is missing, unreadable, or the workspace root wasn't provided (e.g.
//!     unit tests), we fall back to the original italic placeholder so the
//!     pipeline never crashes mid-export.
//!   - Storyboard items behave the same way — frames extracted by Spike F
//!     show as inline images.
//!   - The new `evidenceTable` node from the schema is rendered as a real
//!     DOCX table (title + thead + tbody) so checklist / vestígios /
//!     medições keep their structure.

use std::path::Path;

use base64::Engine as _;
use docx_rs::*;
use serde_json::Value;

use crate::error::{Result, SicroError};

/// Render the TipTap JSON `content` (the inner value of `.sicrodoc`'s
/// `content` field) into a DOCX file at `target`.
///
/// `workspace_root` is the absolute path of the `.sicro` folder. When
/// provided, figures and storyboard items with a `relative_path` attribute
/// are embedded as real images. Passing `None` is supported (used by the
/// unit tests) — in that mode every image degrades to the italic
/// placeholder.
/// Pré-resolve as pílulas `fieldPlaceholder` ({numero_laudo} etc.) usando os
/// valores que o FRONT resolveu (metadata + occurrence + catálogo) e injetou em
/// `envelope.__field_values` (objeto chave→string). Substitui cada pílula
/// resolvível por um nó de texto com o valor. NÃO toca em:
///   - `page`/`pages` → continuam pílula (o walker emite campo NATIVO do Word);
///   - chaves ausentes ou com valor vazio → continuam pílula `{chave}` (mostra
///     pendência, igual ao editor).
/// Mantém o resolver do front como ÚNICA fonte de verdade (sem duplicar o
/// catálogo no Rust). Resolve em content + header.content + footer.content.
fn resolve_field_placeholders_in_envelope(envelope: &Value) -> Value {
    let mut env = envelope.clone();
    let fields = env
        .get("__field_values")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if !fields.is_empty() {
        for path in [
            &["content"][..],
            &["header", "content"][..],
            &["footer", "content"][..],
        ] {
            if let Some(subtree) = get_mut_path(&mut env, path) {
                resolve_fields_in_node(subtree, &fields);
            }
        }
    }
    env
}

/// Navega `value` por uma sequência de chaves de objeto, devolvendo a referência
/// mutável ao nó final (ou None se o caminho não existir).
fn get_mut_path<'a>(value: &'a mut Value, path: &[&str]) -> Option<&'a mut Value> {
    let mut cur = value;
    for key in path {
        cur = cur.get_mut(*key)?;
    }
    Some(cur)
}

/// Percorre a árvore ProseMirror trocando cada `fieldPlaceholder` resolvível por
/// um nó de texto. Ver `resolve_field_placeholders_in_envelope`.
fn resolve_fields_in_node(node: &mut Value, fields: &serde_json::Map<String, Value>) {
    if node.get("type").and_then(Value::as_str) == Some("fieldPlaceholder") {
        if let Some(key) = node
            .get("attrs")
            .and_then(|a| a.get("field"))
            .and_then(Value::as_str)
        {
            // page/pages ficam como pílula → campo nativo do Word no walker.
            if key != "page" && key != "pages" {
                if let Some(val) = fields.get(key).and_then(Value::as_str) {
                    let trimmed = val.trim();
                    if !trimmed.is_empty() {
                        *node = serde_json::json!({ "type": "text", "text": trimmed });
                        return;
                    }
                }
            }
        }
    }
    if let Some(content) = node.get_mut("content").and_then(Value::as_array_mut) {
        for child in content.iter_mut() {
            resolve_fields_in_node(child, fields);
        }
    }
}

pub fn render_doc_to_docx(
    envelope: &Value,
    target: &Path,
    workspace_root: Option<&Path>,
) -> Result<()> {
    // Pré-resolve as pílulas de campo (`{numero_laudo}` etc.) usando os valores
    // que o FRONT resolveu (metadata + occurrence + catálogo) e injetou em
    // `envelope.__field_values`. Sem isto, o walker emitia o placeholder textual
    // `{chave}` para todo campo que não fosse page/pages — o .docx/PDF-Word saía
    // com `{numero_laudo}` literal, embora o editor mostrasse o valor.
    let resolved_envelope;
    let envelope = if envelope.get("__field_values").is_some() {
        resolved_envelope = resolve_field_placeholders_in_envelope(envelope);
        &resolved_envelope
    } else {
        envelope
    };

    let content_doc = envelope
        .get("content")
        .ok_or_else(|| SicroError::Validation("sicrodoc missing 'content' field".to_string()))?;
    let blocks = children(content_doc);

    // MVP 2: register institutional Header/Footer (best-effort).
    let template_id = envelope
        .get("layout")
        .and_then(|l| l.get("institutional_template"))
        .and_then(Value::as_str);
    let metadata = envelope
        .get("metadata")
        .and_then(Value::as_object)
        .cloned();

    // Padrão do documento = Arial 12pt (24 meios-pontos), igual ao
    // editor (--font-doc) e ao render HTML. SEM isto, o corpo (runs sem size
    // explícito, criados com base_size=None) saía no default do docx-rs
    // (~10,5pt) — MENOR que os 12pt da tela. Isso era um descompasso de WYSIWYG
    // (texto do arquivo assinado menor que o editado) e inflava a contagem de
    // linhas por página (o Word empacotava ~60 linhas onde o editor mostrava 48).
    let base = Docx::new()
        .default_size(24)
        .default_fonts(RunFonts::new().ascii("Arial").hi_ansi("Arial"));
    let mut docx = build_institutional_chrome(base, template_id, metadata.as_ref());

    // Contexto de render (workspace p/ resolver imagens). Criado ANTES do
    // header porque ele agora também embute figuras (brasões) do cabeçalho.
    let ctx = RenderCtx { workspace_root };

    // N11 — Header dinâmico Word-style. Se envelope.header existir,
    // estiver enabled e tiver conteúdo não-vazio, monta um Header DOCX
    // nativo iterando os blocos do ProseMirror. Agora COM ctx + largura útil,
    // para embutir figuras (brasões dimensionados pelo `width`) e renderizar
    // tabelas do cabeçalho — antes caíam num placeholder de texto.
    if let Some(header) =
        build_dynamic_header(envelope, &ctx, usable_content_width_cm(envelope))
    {
        docx = docx.header(header);
    }

    // Apply effective page margins. The resolution order matches the editor:
    //   envelope.layout.page.margins  >  institutional template default  >  SICRO default.
    docx = docx.page_margin(resolve_page_margin(envelope, template_id));

    // NÃO injetamos mais um título sintético aqui: o título (quando existe)
    // vem do PRÓPRIO conteúdo que o perito digitou, igual ao render HTML/PDF.
    // Antes, o DOCX prependia "Laudo Pericial (data)" que não estava no corpo.

    let mut has_toc = false;
    for node in &blocks {
        if type_of(node) == "dynamicSummary" {
            has_toc = true;
        }
        docx = render_top_level_block(docx, node, &ctx);
    }

    let file = std::fs::File::create(target).map_err(|e| {
        SicroError::Filesystem(format!(
            "could not create docx at {}: {}",
            target.display(),
            e
        ))
    })?;
    docx.build()
        .pack(file)
        .map_err(|e| SicroError::Workspace(format!("docx pack failed: {e}")))?;

    // Quando há sumário (TOC nativo), liga `<w:updateFields>` no settings.xml.
    // Sem isso, Word/LibreOffice não recalculam o índice (números de página
    // sairiam vazios). Best-effort: se falhar, o export segue (TOC só não
    // atualiza sozinho — o usuário pode dar F9 no Word).
    if has_toc {
        let _ = enable_update_fields(target);
    }

    // Marca lateral vertical: text_box rotacionado do cabeçalho (ex.: a faixa
    // "POLÍCIA CIENTÍFICA…" na margem esquerda). docx-rs não escreve textbox
    // (writer `unimplemented!`), então injetamos uma textbox VML vertical no
    // header do .docx (pós-processo do ZIP). Best-effort: se falhar, o .docx
    // segue válido (a faixa só não aparece).
    let lateral = collect_header_lateral_marks(envelope);
    if !lateral.is_empty() {
        let _ = inject_header_lateral_marks(target, &lateral);
    }
    Ok(())
}

/// Pós-processa o `.docx` (que é um ZIP) para inserir
/// `<w:updateFields w:val="true"/>` em `word/settings.xml`. É o mecanismo OOXML
/// padrão que faz Word E LibreOffice recalcularem todos os campos (incluindo o
/// TOC) ao abrir/converter o documento. docx-rs 0.4 não expõe isso, então
/// reescrevemos o pacote.
fn enable_update_fields(docx_path: &Path) -> Result<()> {
    use std::io::{Read, Write};

    let f = std::fs::File::open(docx_path)
        .map_err(|e| SicroError::Filesystem(format!("abrir docx: {e}")))?;
    let mut zip = zip::ZipArchive::new(f)
        .map_err(|e| SicroError::Validation(format!("docx inválido: {e}")))?;

    let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let mut e = zip
            .by_index(i)
            .map_err(|e| SicroError::Validation(format!("entrada zip: {e}")))?;
        let name = e.name().to_string();
        let mut buf = Vec::new();
        e.read_to_end(&mut buf).ok();
        entries.push((name, buf));
    }
    drop(zip);

    // Injeta o updateFields logo após a abertura de <w:settings ...>.
    let mut touched = false;
    for (name, data) in entries.iter_mut() {
        if name == "word/settings.xml" {
            let xml = String::from_utf8_lossy(data).into_owned();
            if xml.contains("w:updateFields") {
                touched = true;
                break;
            }
            if let Some(open) = xml.find("<w:settings") {
                if let Some(gt) = xml[open..].find('>') {
                    let pos = open + gt + 1;
                    let mut out = String::with_capacity(xml.len() + 40);
                    out.push_str(&xml[..pos]);
                    out.push_str("<w:updateFields w:val=\"true\"/>");
                    out.push_str(&xml[pos..]);
                    *data = out.into_bytes();
                    touched = true;
                }
            }
            break;
        }
    }
    // Se não havia settings.xml (raro no docx-rs), não há o que fazer com
    // segurança — abortar sem reescrever.
    if !touched {
        return Ok(());
    }

    // Reescreve o ZIP com as entradas (modificadas).
    let out = std::fs::File::create(docx_path)
        .map_err(|e| SicroError::Filesystem(format!("recriar docx: {e}")))?;
    let mut w = zip::ZipWriter::new(out);
    let opts: zip::write::FileOptions =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        w.start_file(&name, opts)
            .map_err(|e| SicroError::Workspace(format!("zip start: {e}")))?;
        w.write_all(&data)
            .map_err(|e| SicroError::Workspace(format!("zip write: {e}")))?;
    }
    w.finish()
        .map_err(|e| SicroError::Workspace(format!("zip finish: {e}")))?;
    Ok(())
}

// ===========================================================================
// Marca lateral (text_box rotacionado do cabeçalho) → textbox VML vertical.
//
// O docx-rs não escreve textbox (writer `unimplemented!`), então geramos a
// faixa vertical como uma textbox VML (`<w:pict><v:shape><v:textbox>`) e a
// injetamos no(s) `word/header*.xml` em pós-processamento. VML com
// `layout-flow:vertical` é o jeito clássico de texto vertical em margem que
// Word E LibreOffice renderizam. Os namespaces v:/o:/w10: já vêm no `<w:hdr>`
// do docx-rs, então não precisamos declará-los.

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn attr_f64(node: &Value, key: &str, default: f64) -> f64 {
    node.get("attrs")
        .and_then(|a| a.get(key))
        .and_then(Value::as_f64)
        .unwrap_or(default)
}

/// Converte "11pt" / "14px" / "12" em meios-pontos do Word (sz). None → falha.
fn font_size_to_halfpoints(s: &str) -> Option<u32> {
    let s = s.trim();
    let pt = if let Some(v) = s.strip_suffix("pt") {
        v.trim().parse::<f64>().ok()?
    } else if let Some(v) = s.strip_suffix("px") {
        v.trim().parse::<f64>().ok()? * 0.75
    } else {
        s.parse::<f64>().ok()?
    };
    Some(((pt * 2.0).round() as i64).clamp(8, 200) as u32)
}

/// Extrai recursivamente o texto + marcas (itálico/negrito/cor/tamanho) de um
/// node (text_box). A faixa lateral é uma linha; concatenamos os text nodes.
fn collect_text_and_marks(
    node: &Value,
    text: &mut String,
    italic: &mut bool,
    bold: &mut bool,
    color: &mut Option<String>,
    sz_half: &mut Option<u32>,
) {
    if type_of(node) == "text" {
        if let Some(t) = node.get("text").and_then(Value::as_str) {
            text.push_str(t);
        }
        if let Some(marks) = node.get("marks").and_then(Value::as_array) {
            for m in marks {
                match m.get("type").and_then(Value::as_str) {
                    Some("italic") => *italic = true,
                    Some("bold") => *bold = true,
                    Some("textStyle") => {
                        if color.is_none() {
                            if let Some(c) = m
                                .get("attrs")
                                .and_then(|a| a.get("color"))
                                .and_then(Value::as_str)
                            {
                                *color = Some(c.to_string());
                            }
                        }
                        if sz_half.is_none() {
                            if let Some(s) = m
                                .get("attrs")
                                .and_then(|a| a.get("fontSize"))
                                .and_then(Value::as_str)
                            {
                                *sz_half = font_size_to_halfpoints(s);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    for c in node
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        collect_text_and_marks(c, text, italic, bold, color, sz_half);
    }
}

fn collect_text_box_nodes<'a>(node: &'a Value, out: &mut Vec<&'a Value>) {
    if type_of(node) == "text_box" {
        out.push(node);
    }
    for c in node
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        collect_text_box_nodes(c, out);
    }
}

/// Parágrafo interno (centrado) da textbox, preservando itálico/negrito/cor/
/// tamanho do text_box.
fn lateral_inner_paragraph_xml(
    text: &str,
    italic: bool,
    bold: bool,
    color: Option<&str>,
    sz_half: u32,
) -> String {
    let mut rpr = String::new();
    if bold {
        rpr.push_str("<w:b/>");
    }
    if italic {
        rpr.push_str("<w:i/>");
    }
    if let Some(c) = color {
        rpr.push_str(&format!(
            "<w:color w:val=\"{}\"/>",
            xml_escape(c.trim_start_matches('#'))
        ));
    }
    rpr.push_str(&format!("<w:sz w:val=\"{0}\"/><w:szCs w:val=\"{0}\"/>", sz_half));
    format!(
        "<w:p><w:pPr><w:jc w:val=\"center\"/><w:rPr>{rpr}</w:rPr></w:pPr><w:r><w:rPr>{rpr}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml_escape(text)
    )
}

/// Constrói a `<w:p>` com a textbox VML vertical para um text_box, ou None se
/// não tiver texto. Posiciona a faixa centrada na margem esquerda e centrada
/// verticalmente na página (A4). `idx==0` carrega a definição do shapetype.
/// Canto superior-esquerdo (cm, relativo à PÁGINA) da faixa lateral VML, a
/// partir da caixa girada ±90° do editor. A caixa gira em torno do CENTRO:
///   centro (área de conteúdo) = (wrap_x + W/2, wrap_y + H/2);
///   visualmente vira H(espessura) × W(comprimento) → canto visual:
///     vis_left = wrap_x + (W − H)/2 ;  vis_top = wrap_y + (H − W)/2
/// Página: X = margem_esquerda + vis_left (origem do editor = área de
/// conteúdo); Y = vis_top (cabeçalho começa no topo da folha). Clampado p/ não
/// sair da folha A4. Preserva o "espaço à esquerda" que o editor mostra (não
/// cola na borda).
fn lateral_mark_offsets_cm(
    left_margin_cm: f64,
    wrap_x: f64,
    wrap_y: f64,
    width_cm: f64,
    height_cm: f64,
) -> (f64, f64) {
    const PAGE_W_CM: f64 = 21.0;
    const PAGE_H_CM: f64 = 29.7;
    let thick = height_cm.clamp(0.5, 6.0);
    let len = width_cm.clamp(2.0, PAGE_H_CM);
    let vis_left = wrap_x + (width_cm - height_cm) / 2.0;
    let vis_top = wrap_y + (height_cm - width_cm) / 2.0;
    (
        (left_margin_cm + vis_left).clamp(0.0, (PAGE_W_CM - thick).max(0.0)),
        vis_top.clamp(0.0, (PAGE_H_CM - len).max(0.0)),
    )
}

fn text_box_to_vml(tb: &Value, idx: usize, left_margin_cm: f64) -> Option<String> {
    let mut text = String::new();
    let mut italic = false;
    let mut bold = false;
    let mut color = None;
    let mut sz_half = None;
    collect_text_and_marks(tb, &mut text, &mut italic, &mut bold, &mut color, &mut sz_half);
    if text.trim().is_empty() {
        return None;
    }

    const PT_PER_CM: f64 = 28.3465;
    const PAGE_H_CM: f64 = 29.7; // A4

    let width_cm = attr_f64(tb, "width_cm", 20.0); // comprimento da faixa
    let height_cm = attr_f64(tb, "height_cm", 1.5); // espessura da faixa
    let rotation = attr_f64(tb, "rotation", -90.0);

    let thick_cm = height_cm.clamp(0.5, 6.0);
    let len_cm = width_cm.clamp(2.0, PAGE_H_CM);

    // Honra a posição REAL da caixa (não centraliza colada na borda) —
    // preserva o "espaço à esquerda" que o editor mostra.
    let wrap_x = attr_f64(tb, "wrap_x_cm", 0.0);
    let wrap_y = attr_f64(tb, "wrap_y_cm", 0.0);
    let (left_cm, top_cm) =
        lateral_mark_offsets_cm(left_margin_cm, wrap_x, wrap_y, width_cm, height_cm);

    // rotação negativa (anti-horário, como no editor) → lê de baixo p/ cima.
    let flow = if rotation < 0.0 {
        "layout-flow:vertical;mso-layout-flow-alt:bottom-to-top"
    } else {
        "layout-flow:vertical"
    };
    let inner = lateral_inner_paragraph_xml(&text, italic, bold, color.as_deref(), sz_half.unwrap_or(22));
    let shapetype = if idx == 0 {
        "<v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\"><v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>"
    } else {
        ""
    };

    Some(format!(
        "<w:p><w:r><w:pict>{shapetype}<v:shape id=\"SicroLateral{idx}\" type=\"#_x0000_t202\" \
         style=\"position:absolute;margin-left:{left:.1}pt;margin-top:{top:.1}pt;width:{thick:.1}pt;height:{len:.1}pt;\
         mso-position-horizontal-relative:page;mso-position-vertical-relative:page\" filled=\"f\" stroked=\"f\">\
         <v:textbox style=\"{flow}\" inset=\"1pt,1pt,1pt,1pt\"><w:txbxContent>{inner}</w:txbxContent></v:textbox>\
         </v:shape></w:pict></w:r></w:p>",
        left = left_cm * PT_PER_CM,
        top = top_cm * PT_PER_CM,
        thick = thick_cm * PT_PER_CM,
        len = len_cm * PT_PER_CM,
    ))
}

/// Coleta as faixas laterais (VML pronto) dos text_box do cabeçalho.
fn collect_header_lateral_marks(envelope: &Value) -> Vec<String> {
    let header = match envelope.get("header") {
        Some(h) => h,
        None => return vec![],
    };
    if header.get("enabled").and_then(Value::as_bool) != Some(true) {
        return vec![];
    }
    let content = match header.get("content") {
        Some(c) => c,
        None => return vec![],
    };
    let left_margin_cm = envelope
        .get("layout")
        .and_then(|l| l.get("page"))
        .and_then(|p| p.get("margins"))
        .and_then(|m| m.get("left"))
        .and_then(Value::as_str)
        .and_then(parse_len_cm)
        .unwrap_or(3.0);

    let mut boxes = Vec::new();
    collect_text_box_nodes(content, &mut boxes);
    boxes
        .iter()
        .enumerate()
        .filter_map(|(i, tb)| text_box_to_vml(tb, i, left_margin_cm))
        .collect()
}

/// Injeta as faixas VML logo antes de `</w:hdr>` em cada `word/header*.xml`.
/// Best-effort: zip inválido / sem header → no-op.
fn inject_header_lateral_marks(docx_path: &Path, marks: &[String]) -> Result<()> {
    use std::io::{Read, Write};

    let joined: String = marks.concat();
    let f = std::fs::File::open(docx_path)
        .map_err(|e| SicroError::Filesystem(format!("abrir docx: {e}")))?;
    let mut zip = zip::ZipArchive::new(f)
        .map_err(|e| SicroError::Validation(format!("docx inválido: {e}")))?;

    let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let mut e = zip
            .by_index(i)
            .map_err(|e| SicroError::Validation(format!("entrada zip: {e}")))?;
        let name = e.name().to_string();
        let mut buf = Vec::new();
        e.read_to_end(&mut buf).ok();
        entries.push((name, buf));
    }
    drop(zip);

    let mut touched = false;
    for (name, data) in entries.iter_mut() {
        if name.starts_with("word/header") && name.ends_with(".xml") {
            let xml = String::from_utf8_lossy(data).into_owned();
            if let Some(pos) = xml.rfind("</w:hdr>") {
                let mut out = String::with_capacity(xml.len() + joined.len());
                out.push_str(&xml[..pos]);
                out.push_str(&joined);
                out.push_str(&xml[pos..]);
                *data = out.into_bytes();
                touched = true;
            }
        }
    }
    if !touched {
        return Ok(());
    }

    let out = std::fs::File::create(docx_path)
        .map_err(|e| SicroError::Filesystem(format!("recriar docx: {e}")))?;
    let mut w = zip::ZipWriter::new(out);
    let opts: zip::write::FileOptions =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        w.start_file(&name, opts)
            .map_err(|e| SicroError::Workspace(format!("zip start: {e}")))?;
        w.write_all(&data)
            .map_err(|e| SicroError::Workspace(format!("zip write: {e}")))?;
    }
    w.finish()
        .map_err(|e| SicroError::Workspace(format!("zip finish: {e}")))?;
    Ok(())
}

/// Carries the read-only state needed during the walk so each node can
/// resolve evidence assets without re-parsing the envelope.
#[derive(Clone, Copy)]
struct RenderCtx<'a> {
    workspace_root: Option<&'a Path>,
}

// ===========================================================================
// MVP 2 ajuste 1.2 — page margins

/// 1 cm in twentieths of a point (twips). Word page geometry is expressed in
/// twips, so this conversion is exact for the values we accept.
const TWIPS_PER_CM: f64 = 567.0;

fn cm_to_twips(cm: f64) -> i32 {
    (cm * TWIPS_PER_CM).round() as i32
}

/// Aplica os recuos estilo Word (atributos `first_line_indent_cm` e
/// `left_indent_cm` dos nós paragraph/heading) como `w:ind` no DOCX —
/// espelhando o que o editor e o HTML/PDF-Edge já renderizam via CSS
/// (text-indent / margin-left). Sem atributo (default 0) = sem recuo.
fn apply_paragraph_indent(p: Paragraph, node: &Value) -> Paragraph {
    let attrs = node.get("attrs");
    let first = attrs
        .and_then(|a| a.get("first_line_indent_cm"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let left = attrs
        .and_then(|a| a.get("left_indent_cm"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let special = if first.abs() > 0.001 {
        Some(SpecialIndentType::FirstLine(cm_to_twips(first)))
    } else {
        None
    };
    let left_tw = if left.abs() > 0.001 {
        Some(cm_to_twips(left))
    } else {
        None
    };
    if special.is_some() || left_tw.is_some() {
        p.indent(left_tw, special, None, None)
    } else {
        p
    }
}

/// Parse a CSS length string ("3cm", "2.5cm", "25mm", "30pt", "1in").
/// Defaults: bare numbers are interpreted as cm. Returns None for invalid input.
fn parse_length_cm(value: &str) -> Option<f64> {
    let trimmed = value.trim().replace(',', ".");
    if trimmed.is_empty() {
        return None;
    }
    // Find the unit suffix.
    let (num_part, unit) = trimmed
        .find(|c: char| c.is_alphabetic())
        .map(|idx| (&trimmed[..idx], &trimmed[idx..]))
        .unwrap_or((trimmed.as_str(), "cm"));
    let n: f64 = num_part.trim().parse().ok()?;
    let cm = match unit.trim().to_lowercase().as_str() {
        "cm" | "" => n,
        "mm" => n / 10.0,
        "pt" => n / 28.3464567,
        "in" => n * 2.54,
        "px" => n / 37.795275591, // 96 dpi
        _ => return None,
    };
    Some(cm)
}

/// Resolve effective margins from envelope.layout.page.margins or fall back to
/// the institutional template defaults. Keeps the DOCX consistent with the
/// editor and the HTML/PDF renderer.
fn resolve_page_margin(envelope: &Value, template_id: Option<&str>) -> PageMargin {
    // Template fallback (cm). Mirrors `institutional-templates.ts`.
    // Default quando ausente: "em_branco" (folha limpa), margens simétricas
    // de 2,5cm — iguais às do template BLANK_V1 do front, pra editor e DOCX
    // concordarem. Só o PCA legado usa as margens assimétricas antigas.
    let (mut top, mut right, mut bottom, mut left) = match template_id.unwrap_or("em_branco")
    {
        "pca_padrao_v1" => (3.0_f64, 2.0_f64, 2.5_f64, 3.5_f64),
        _ => (2.5_f64, 2.5_f64, 2.5_f64, 2.5_f64),
    };

    // Per-laudo override (envelope.layout.page.margins) takes precedence when
    // the four sides are present and parse cleanly.
    if let Some(m) = envelope
        .get("layout")
        .and_then(|l| l.get("page"))
        .and_then(|p| p.get("margins"))
    {
        let parsed = (
            m.get("top").and_then(Value::as_str).and_then(parse_length_cm),
            m.get("right").and_then(Value::as_str).and_then(parse_length_cm),
            m.get("bottom").and_then(Value::as_str).and_then(parse_length_cm),
            m.get("left").and_then(Value::as_str).and_then(parse_length_cm),
        );
        if let (Some(t), Some(r), Some(b), Some(l)) = parsed {
            top = t;
            right = r;
            bottom = b;
            left = l;
        }
    }

    PageMargin::new()
        .top(cm_to_twips(top))
        .right(cm_to_twips(right))
        .bottom(cm_to_twips(bottom))
        .left(cm_to_twips(left))
}

// ===========================================================================
// N — Institutional chrome (footer apenas; header agora é dinâmico)
//
// Header hardcoded REMOVIDO. Antes essa seção injetava 3 linhas fixas
// (GOVERNO DO ESTADO DO AMAPÁ / POLÍCIA CIENTÍFICA / DEPTO CRIMINALÍSTICA)
// + "Laudo nº" no Header nativo do DOCX. Em N11 será reintroduzida a
// injeção de Header LENDO `envelope.header.content` (ProseMirror JSON),
// percorrendo os nodes com a mesma máquina do walker do body, e só quando
// `envelope.header.enabled === true`.
//
// O Footer permanece como antes — N só refatora o header.
// Marca lateral continua intencionalmente fora do DOCX (texto rotacionado
// em margem é frágil entre Word desktop / LibreOffice / Office Mobile).

fn build_institutional_chrome(
    docx: Docx,
    template_id: Option<&str>,
    metadata: Option<&serde_json::Map<String, Value>>,
) -> Docx {
    let _ = metadata;

    // Documento em branco (padrão) ou sem template institucional: SEM rodapé,
    // como uma folha do Word. O rodapé boilerplate só é aplicado para
    // templates que explicitamente o definem — hoje, apenas o PCA legado.
    // O timbre "de verdade" será reconstruído nos modelos via o cabeçalho
    // dinâmico (`envelope.header.content`), não aqui.
    if template_id != Some("pca_padrao_v1") {
        return docx;
    }

    let footer = Footer::new().add_paragraph(
        Paragraph::new()
            .align(AlignmentType::Center)
            .add_run(
                Run::new()
                    .add_text("Documento gerado pelo SICRO 2.0 — versão preliminar (MVP 2).")
                    .size(16)
                    .italic(),
            ),
    );

    // N — Header NÃO é mais aplicado aqui. É construído por
    // `build_dynamic_header` (N11) e aplicado no caller a partir do
    // `envelope.header.content`.
    docx.footer(footer)
}

/// N11 — Constrói um Header DOCX nativo a partir do envelope.header.
///
/// Retorna `None` quando:
///   - envelope não tem campo `header`,
///   - `header.enabled === false`,
///   - `header.content` está vazio (só parágrafo vazio).
///
/// Caso contrário monta um `Header::new()` iterando os blocos
/// top-level do `header.content.content` e convertendo cada um em
/// `Paragraph` via as MESMAS funções usadas no walker do body
/// (`paragraph_from_inline`, `heading_paragraph`). Isso garante que
/// formatação inline (bold/italic/underline/cor/alinhamento) seja
/// preservada com a mesma fidelidade.
///
/// `figure` é embutido como imagem real (brasão), dimensionado pelo attr
/// `width` (% da largura útil). `table` vira tabela nativa. Nodes ainda sem
/// suporte no header (text_box rotacionado, storyboard, etc.) caem no
/// fallback de texto — o text_box da marca lateral, por ex., sai como texto
/// (rotação em margem é frágil entre Word/LibreOffice; decisão consciente).
fn build_dynamic_header(envelope: &Value, ctx: &RenderCtx, usable_cm: f64) -> Option<Header> {
    let header_node = envelope.get("header")?;
    let enabled = header_node
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !enabled {
        return None;
    }
    let content_doc = header_node.get("content")?;
    let blocks = children(content_doc);
    if blocks.is_empty() {
        return None;
    }
    // Detecta header "trivialmente vazio" (apenas 1 parágrafo sem texto).
    if blocks.len() == 1
        && type_of(&blocks[0]) == "paragraph"
        && children(&blocks[0]).is_empty()
    {
        return None;
    }

    // Margem esquerda (cm) — origem do X das figuras flutuantes do cabeçalho.
    let left_margin_cm = envelope
        .get("layout")
        .and_then(|l| l.get("page"))
        .and_then(|p| p.get("margins"))
        .and_then(|m| m.get("left"))
        .and_then(Value::as_str)
        .and_then(parse_len_cm)
        .unwrap_or(3.0);

    let mut header = Header::new();
    for node in &blocks {
        match type_of(node) {
            "paragraph" => {
                header = header.add_paragraph(paragraph_from_inline(node));
            }
            "heading" => {
                header = header.add_paragraph(heading_paragraph(node));
            }
            // Figura (brasão) → imagem real embutida, dimensionada pelo
            // `width` da figura. ANTES caía no fallback de texto → virava
            // "[bloco figure sem suporte na exportação DOCX]".
            "figure" => {
                header =
                    header.add_paragraph(header_figure_paragraph(ctx, node, usable_cm, left_margin_cm));
            }
            // Tabela do cabeçalho (ex.: "Laudo nº …") → tabela nativa.
            "table" => match build_table_object(node, ctx) {
                Some(table) => {
                    header = header.add_table(table);
                }
                None => {
                    header = header.add_paragraph(fallback_paragraph(node));
                }
            },
            // Marca lateral (text_box rotacionado, ex.: "POLÍCIA CIENTÍFICA…")
            // NÃO entra no fluxo horizontal do cabeçalho — é injetada como
            // textbox VML vertical na margem em pós-processamento
            // (`inject_header_lateral_marks`). Aqui só pulamos para não sair
            // como texto horizontal solto.
            "text_box" => {}
            // Fallback: trata qualquer outro top-level como parágrafo
            // (extrai texto recursivamente). Mantém o header útil mesmo
            // se aparecerem nodes inesperados de schemas futuros.
            _ => {
                header = header.add_paragraph(fallback_paragraph(node));
            }
        }
    }
    Some(header)
}

/// Offsets (cm) de uma figura FLUTUANTE do cabeçalho, relativos à PÁGINA e
/// SEMPRE >= 0:
///   - X = margem_esquerda + wrap_x (o editor mede wrap_x a partir da área de
///     conteúdo; um wrap_x negativo aproxima da borda, mas o offset final não
///     pode ser negativo — o docx-rs serializa `posOffset` como u32 e um valor
///     negativo estoura p/ ~4,29 bilhões, corrompendo o DOCX no Word);
///   - Y = wrap_y (cabeçalho começa no topo da folha).
/// Clampado à folha A4 (X: 0..19cm, Y: 0..28cm).
fn header_float_offsets_cm(left_margin_cm: f64, wrap_x_cm: f64, wrap_y_cm: f64) -> (f64, f64) {
    (
        (left_margin_cm + wrap_x_cm).clamp(0.0, 19.0),
        wrap_y_cm.clamp(0.0, 28.0),
    )
}

/// Imagem ANCORADA (flutuante) na página, espelhando o posicionamento do
/// editor (`position: absolute; left: wrap_x_cm; top: wrap_y_cm`):
///   - X relativo à MARGEM (origem = margem esquerda = canto da área de
///     conteúdo do editor; offset negativo entra na margem, como no editor);
///   - Y relativo à PÁGINA (o cabeçalho começa no topo da folha).
/// `behindDoc` não é exposto pelo docx-rs 0.4 (sempre "na frente"); como a
/// figura costuma ficar em canto/margem, não cobre texto na prática.
fn build_image_pic_floating(
    workspace_root: &Path,
    relative_path: &str,
    target_cm: f64,
    x_emu: i32,
    y_emu: i32,
) -> Option<Pic> {
    resolve_image_bytes(workspace_root, relative_path).and_then(|bytes| {
        let (w_emu, h_emu) = image_target_emu(&bytes, target_cm);
        Some(
            Pic::new(&bytes)
                .size(w_emu, h_emu)
                .floating()
                // Ambos relativos à PÁGINA com offset >= 0. NUNCA passar offset
                // negativo: o docx-rs serializa `posOffset` como u32, então um
                // valor negativo vira ~4,29 bilhões (estouro) → OOXML inválido
                // que o Word recusa (o LibreOffice tolera). Por isso o caller
                // converte X de "margem + wrap_x" e clampa em >= 0.
                .relative_from_h(RelativeFromHType::Page)
                .relative_from_v(RelativeFromVType::Page)
                .position_h(DrawingPosition::Offset(x_emu))
                .position_v(DrawingPosition::Offset(y_emu)),
        )
    })
}

/// Parágrafo de cabeçalho com a IMAGEM da figura (brasão) embutida. Figuras
/// `inline` saem centralizadas no fluxo; figuras flutuantes (`in_front`/
/// `behind`) saem ANCORADAS na posição (wrap_x_cm/wrap_y_cm), preservando a
/// "transformação livre" do editor. Dimensão pelo attr `width`. Fallback:
/// placeholder itálico se a imagem não resolver.
fn header_figure_paragraph(
    ctx: &RenderCtx,
    node: &Value,
    usable_cm: f64,
    left_margin_cm: f64,
) -> Paragraph {
    let attrs = node.get("attrs");
    let align = match attrs
        .and_then(|a| a.get("align"))
        .and_then(Value::as_str)
        .unwrap_or("center")
    {
        "left" => AlignmentType::Left,
        "right" => AlignmentType::Right,
        _ => AlignmentType::Center,
    };
    // `relative_path` é o caminho do arquivo no workspace; `src` pode ser o
    // mesmo path OU um data URI (cabeçalho-modelo PORTÁTIL, com o brasão
    // embutido — independe da pasta de um caso). Só usamos `src` como caminho
    // de arquivo quando NÃO é data URI.
    let relative_path = attrs
        .and_then(|a| a.get("relative_path"))
        .and_then(Value::as_str)
        .or_else(|| {
            attrs
                .and_then(|a| a.get("src"))
                .and_then(Value::as_str)
                .filter(|s| !s.starts_with("data:"))
        });
    // Brasão embutido como data URI (modelo global). Tem prioridade: não
    // depende do workspace, então funciona em qualquer laudo/máquina.
    let data_uri_bytes = attrs
        .and_then(|a| a.get("src"))
        .and_then(Value::as_str)
        .filter(|s| s.starts_with("data:"))
        .and_then(decode_png_data_uri);
    let target_cm = figure_target_width_cm(node, usable_cm, 3.0);

    let wrap_mode = attrs
        .and_then(|a| a.get("wrap_mode"))
        .and_then(Value::as_str)
        .unwrap_or("inline");
    let floating = wrap_mode == "in_front" || wrap_mode == "behind";

    // (1) Brasão embutido (data URI) — caminho portátil, sem workspace.
    if let Some(bytes) = data_uri_bytes {
        if floating {
            let wx = attr_f64(node, "wrap_x_cm", 0.0);
            let wy = attr_f64(node, "wrap_y_cm", 0.0);
            let (x_cm, y_cm) = header_float_offsets_cm(left_margin_cm, wx, wy);
            let x_emu = (x_cm * EMU_PER_CM).round() as i32;
            let y_emu = (y_cm * EMU_PER_CM).round() as i32;
            if let Some(pic) = build_pic_from_bytes_floating(&bytes, target_cm, x_emu, y_emu) {
                return Paragraph::new().add_run(Run::new().add_image(pic));
            }
        } else if let Some(pic) = build_pic_from_bytes_sized(&bytes, target_cm) {
            return Paragraph::new()
                .align(align)
                .add_run(Run::new().add_image(pic));
        }
    }

    // (2) Arquivo do workspace (relative_path) — fotos/brasões do próprio caso.
    if let (Some(ws), Some(rel)) = (ctx.workspace_root, relative_path) {
        if floating {
            // Editor: `left: wrap_x_cm` é relativo à ÁREA DE CONTEÚDO (origem =
            // margem esquerda). No DOCX ancoramos relativo à PÁGINA com offset
            // POSITIVO = margem_esquerda + wrap_x (mesma posição visual, mas
            // SEM negativo — offset negativo estoura o u32 do docx-rs e corrompe
            // o arquivo p/ o Word). Y é relativo ao topo da página (= wrap_y).
            let wx = attr_f64(node, "wrap_x_cm", 0.0);
            let wy = attr_f64(node, "wrap_y_cm", 0.0);
            let (x_cm, y_cm) = header_float_offsets_cm(left_margin_cm, wx, wy);
            let x_emu = (x_cm * EMU_PER_CM).round() as i32;
            let y_emu = (y_cm * EMU_PER_CM).round() as i32;
            if let Some(pic) = build_image_pic_floating(ws, rel, target_cm, x_emu, y_emu) {
                // Parágrafo só ANCORA a figura flutuante (a imagem se posiciona
                // sozinha); o parágrafo em si fica vazio.
                return Paragraph::new().add_run(Run::new().add_image(pic));
            }
        } else if let Some(pic) = build_image_pic_sized(ws, rel, target_cm) {
            return Paragraph::new()
                .align(align)
                .add_run(Run::new().add_image(pic));
        }
    }
    Paragraph::new()
        .align(align)
        .add_run(Run::new().add_text("[imagem do cabeçalho indisponível]").italic())
}

// ===========================================================================
// Block dispatch (top level)

fn render_top_level_block(docx: Docx, node: &Value, ctx: &RenderCtx) -> Docx {
    match type_of(node) {
        "paragraph" => docx.add_paragraph(paragraph_from_inline(node)),
        "heading" => docx.add_paragraph(heading_paragraph(node)),
        "bulletList" => render_list(docx, node, ListKind::Bullet),
        "orderedList" => render_list(docx, node, ListKind::Ordered),
        "table" => render_table(docx, node, ctx),
        "figure" => render_figure_top(docx, node, ctx),
        "storyboard" => render_storyboard_top(docx, node, ctx),
        // MVP 4 — checklist/vestígios/medições importadas do Dossiê.
        "evidenceTable" => render_evidence_table(docx, node),
        // MVP 2 — institutional blocks
        "quesitoList" => render_quesito_list(docx, node),
        "signature" => render_signature(docx, node),
        // Sumário inserível → TOC NATIVO do Word (índice por títulos
        // Heading1-3, com hyperlink). Os números de página são calculados pelo
        // Word/LibreOffice ao abrir/converter (graças ao updateFields ligado no
        // settings.xml — ver enable_update_fields). dynamicFigureList/
        // dynamicTableList ainda não têm TOC nativo (caem no fallback).
        "dynamicSummary" => docx
            .add_paragraph(
                // Título "SUMÁRIO" como parágrafo simples (NÃO heading) pra não
                // aparecer dentro do próprio índice.
                Paragraph::new().add_run(Run::new().add_text("SUMÁRIO").bold().size(28)),
            )
            .add_table_of_contents(
                TableOfContents::new().heading_styles_range(1, 3).hyperlink(),
            ),
        "horizontalRule" => docx.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text("────────────────────────────")),
        ),
        // Fórmula matemática de BLOCO → parágrafo centralizado com a imagem
        // renderizada (PNG embutido). Fallback: LaTeX itálico.
        "mathBlock" => render_math_block(docx, node),
        // Anything we don't recognise: do our best to surface any text inside.
        _ => docx.add_paragraph(fallback_paragraph(node)),
    }
}

// ===========================================================================
// Paragraph helpers

fn heading_paragraph(node: &Value) -> Paragraph {
    let level = node
        .get("attrs")
        .and_then(|a| a.get("level"))
        .and_then(Value::as_i64)
        .unwrap_or(1);
    let style = match level {
        1 => "Heading1",
        2 => "Heading2",
        _ => "Heading3",
    };
    let size = match level {
        1 => 36, // 18pt (half-points)
        2 => 28, // 14pt
        _ => 24, // 12pt
    };

    let mut p = Paragraph::new().style(style);
    if let Some(al) = paragraph_alignment(node) {
        p = p.align(al);
    }
    p = apply_paragraph_indent(p, node);

    let mut runs_added = 0;
    for inline in children(node) {
        for run in inline_to_runs(&inline, Some(size), true) {
            p = p.add_run(run);
            runs_added += 1;
        }
    }
    if runs_added == 0 {
        p = p.add_run(Run::new().add_text("").size(size).bold());
    }
    p
}

/// Build a paragraph from the inline children of `node`. ALWAYS emits at
/// least one Run — Word will drop a paragraph without any run.
fn paragraph_from_inline(node: &Value) -> Paragraph {
    let mut p = Paragraph::new();
    // Corpo do laudo é JUSTIFICADO por padrão (igual ao editor e ao HTML/PDF,
    // que justificam o corpo por CSS) — só sai disso se o nó tiver alinhamento
    // explícito (textAlign). + recuos de 1ª linha / esquerda estilo Word.
    p = p.align(paragraph_alignment(node).unwrap_or(AlignmentType::Both));
    p = apply_paragraph_indent(p, node);

    let mut runs_added = 0;
    for inline in children(node) {
        for run in inline_to_runs(&inline, None, false) {
            p = p.add_run(run);
            runs_added += 1;
        }
    }
    if runs_added == 0 {
        p = p.add_run(Run::new().add_text(""));
    }
    p
}

fn empty_paragraph() -> Paragraph {
    Paragraph::new().add_run(Run::new().add_text(""))
}

/// Recursively walk the node looking for `text` leaves. If anything is found,
/// emit it as one italic paragraph; otherwise emit a sentinel so the user can
/// tell something was there but wasn't supported.
fn fallback_paragraph(node: &Value) -> Paragraph {
    let mut text = String::new();
    collect_text(node, &mut text);

    if text.trim().is_empty() {
        text = format!(
            "[bloco \"{}\" sem suporte na exportação DOCX]",
            type_of(node)
        );
    }
    Paragraph::new().add_run(Run::new().add_text(text).italic())
}

fn collect_text(node: &Value, buf: &mut String) {
    if type_of(node) == "text" {
        if let Some(t) = node.get("text").and_then(Value::as_str) {
            buf.push_str(t);
        }
        return;
    }
    if type_of(node) == "systemData" {
        if let Some(v) = node
            .get("attrs")
            .and_then(|a| a.get("value"))
            .and_then(Value::as_str)
        {
            buf.push_str(v);
        }
        return;
    }
    for child in children(node) {
        collect_text(&child, buf);
        // Insert a separator between block-level children for readability.
        let kind = type_of(&child);
        if matches!(
            kind,
            "paragraph" | "heading" | "storyboardItem" | "figure" | "table"
        ) {
            buf.push(' ');
        }
    }
}

// ===========================================================================
// Lists

#[derive(Clone, Copy)]
enum ListKind {
    Bullet,
    Ordered,
}

fn render_list(mut docx: Docx, list_node: &Value, kind: ListKind) -> Docx {
    for p in list_to_paragraphs(list_node, kind) {
        docx = docx.add_paragraph(p);
    }
    docx
}

/// Each list item contains paragraphs; we emit one DOCX paragraph per item
/// with the bullet prefixed as the first run. This intentionally avoids the
/// docx-rs numbering machinery — it's simple and prints predictably.
fn list_to_paragraphs(list_node: &Value, kind: ListKind) -> Vec<Paragraph> {
    let items = children(list_node);
    let mut out = Vec::new();

    for (i, item) in items.iter().enumerate() {
        let bullet = match kind {
            ListKind::Bullet => "• ".to_string(),
            ListKind::Ordered => format!("{}. ", i + 1),
        };
        for inner in children(item) {
            let mut p = Paragraph::new().indent(Some(360), None, None, None);
            if let Some(al) = paragraph_alignment(&inner) {
                p = p.align(al);
            }
            p = p.add_run(Run::new().add_text(bullet.clone()));

            for child in children(&inner) {
                for run in inline_to_runs(&child, None, false) {
                    p = p.add_run(run);
                }
            }
            // Bullet itself already counts as a run — paragraph is safe.
            out.push(p);
        }
    }
    out
}

// ===========================================================================
// Tables

/// F1.2 — 1 px de layout (96dpi) em twips (DXA). 1 px = 1/96 in; 1 in = 1440
/// twips ⇒ 1 px = 15 twips. As `colwidth` do prosemirror-tables são px de
/// layout; convertemos pra DXA pro `set_grid`/`TableCell::width` do docx-rs.
fn px_to_twips(px: f64) -> usize {
    (px * 15.0).round().max(1.0) as usize
}

/// Lê o atributo `attrs.colwidth` (array de px) de uma célula, retornando a
/// soma em px (cobre colspan: prosemirror guarda 1 entrada por coluna).
fn cell_colwidth_px(cell_node: &Value) -> Option<f64> {
    let arr = cell_node
        .get("attrs")
        .and_then(|a| a.get("colwidth"))
        .and_then(Value::as_array)?;
    if arr.is_empty() {
        return None;
    }
    let mut sum = 0.0;
    let mut any = false;
    for v in arr {
        if let Some(n) = v.as_f64() {
            sum += n;
            any = true;
        }
    }
    if any {
        Some(sum)
    } else {
        None
    }
}

/// Constrói o grid de colunas (larguras em DXA) a partir da PRIMEIRA linha.
/// Cada coluna do grid corresponde a uma coluna lógica (expande colspans em
/// fatias iguais). Retorna vazio se nenhuma largura estiver disponível (a
/// tabela então sai em auto-layout, como antes).
fn table_grid_from_first_row(first_row: &Value) -> Vec<usize> {
    let mut grid: Vec<usize> = Vec::new();
    for cell in children(first_row) {
        let colspan = cell
            .get("attrs")
            .and_then(|a| a.get("colspan"))
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1) as usize;
        match cell_colwidth_px(&cell) {
            Some(total_px) => {
                // Reparte a largura total da célula igualmente pelas colunas
                // que ela ocupa (colspan).
                let per = total_px / colspan as f64;
                for _ in 0..colspan {
                    grid.push(px_to_twips(per));
                }
            }
            None => return Vec::new(), // sem larguras → deixa auto-layout
        }
    }
    grid
}

/// Constrói o objeto `Table` do docx-rs a partir do node ProseMirror
/// (linhas/células/grid/alinhamento/bordas). `None` se não houver linhas.
/// Extraído de `render_table` para ser reusável no corpo E no CABEÇALHO
/// (que precisa de um `Table`, não de um `Docx`).
fn build_table_object(table_node: &Value, ctx: &RenderCtx) -> Option<Table> {
    let rows_json = children(table_node);
    let attrs = table_node.get("attrs");
    let border_style = attrs
        .and_then(|a| a.get("borderStyle"))
        .and_then(Value::as_str)
        .unwrap_or("all");
    let table_align = attrs
        .and_then(|a| a.get("tableAlign"))
        .and_then(Value::as_str)
        .unwrap_or("left");
    let cell_padding_px = attrs
        .and_then(|a| a.get("cellPadding"))
        .and_then(Value::as_f64)
        .unwrap_or(5.0);

    let mut rows: Vec<TableRow> = Vec::new();
    for row_node in &rows_json {
        // We accept any row-shaped node (tableRow is the canonical kind, but
        // we don't gate on it — robustness over strictness for the spike).
        let cells_json = children(row_node);
        let mut cells: Vec<TableCell> = Vec::new();
        for cell_node in &cells_json {
            // Accept both `tableCell` and `tableHeader`; same DOCX cell. The
            // header style is currently not different; colspan/valign/width
            // ARE honoured now (F1.2/F4).
            cells.push(build_table_cell(cell_node, ctx, cell_padding_px));
        }
        if !cells.is_empty() {
            // F3 — altura de linha (data-height-cm ou attr rowHeight em cm).
            let mut row = TableRow::new(cells);
            if let Some(h_cm) = row_height_cm(row_node) {
                // docx-rs row_height espera twips (1cm = 567 twips).
                row = row
                    .row_height((h_cm * 567.0) as f32)
                    .height_rule(HeightRule::AtLeast);
            }
            rows.push(row);
        }
    }

    if rows.is_empty() {
        return None;
    }

    let mut table = Table::new(rows);
    // F1.2 — Larguras de coluna (grid) a partir da primeira linha.
    if let Some(first_row) = rows_json.first() {
        let grid = table_grid_from_first_row(first_row);
        if !grid.is_empty() {
            table = table.set_grid(grid);
        }
    }
    // F4 — Alinhamento da tabela.
    table = match table_align {
        "center" => table.align(TableAlignmentType::Center),
        "right" => table.align(TableAlignmentType::Right),
        _ => table.align(TableAlignmentType::Left),
    };
    // F4 — Bordas: "none" remove a grade interna (mantém o retângulo padrão
    // do docx-rs, que já é só externo quando sem bordas internas). docx-rs
    // por padrão desenha bordas; pra "none" limpamos as internas.
    if border_style == "none" {
        table = table
            .clear_border(TableBorderPosition::InsideH)
            .clear_border(TableBorderPosition::InsideV);
    }
    Some(table)
}

fn render_table(docx: Docx, table_node: &Value, ctx: &RenderCtx) -> Docx {
    let attrs = table_node.get("attrs");
    let border_style = attrs
        .and_then(|a| a.get("borderStyle"))
        .and_then(Value::as_str)
        .unwrap_or("all");

    let Some(table) = build_table_object(table_node, ctx) else {
        return docx;
    };

    let docx = docx.add_table(table);
    // F4 — Legenda da tabela (attr `caption`) como parágrafo itálico abaixo,
    // espelhando o figcaption do Figure no DOCX (texto do perito; o prefixo
    // "Tabela N —" é decoração viva do editor / numeração do render HTML).
    //
    // F4.1 — Mesmo gate do renderHTML do editor (captionAllowed): tabela de
    // LAYOUT (borderStyle "none") e legenda REMOVIDA (captionVisible=false)
    // não emitem legenda. O DOCX anda o .sicrodoc CRU (não passa pelo
    // numberFigures do TS), então sem este gate um caption órfão — ex.:
    // tabela legendada que depois virou layout pelo diálogo de propriedades,
    // ou HTML colado com data-caption-visible="false" — imprimiria aqui (e
    // no PDF via LibreOffice) mesmo invisível no editor/HTML.
    let caption_allowed = border_style != "none"
        && attrs
            .and_then(|a| a.get("captionVisible"))
            .and_then(Value::as_bool)
            != Some(false);
    let docx = if let Some(caption) = attrs
        .and_then(|a| a.get("caption"))
        .and_then(Value::as_str)
        .filter(|c| caption_allowed && !c.trim().is_empty())
    {
        docx.add_paragraph(
            Paragraph::new()
                .align(AlignmentType::Left)
                .add_run(Run::new().add_text(caption).italic().size(20)),
        )
    } else {
        docx
    };
    // Trailing empty paragraph so subsequent blocks don't collapse into the table.
    docx.add_paragraph(empty_paragraph())
}

/// F3 — Altura de linha em cm. Aceita o attr novo `rowHeight` (número, cm) ou
/// o legado `data-height-cm` (string) escrito pelo TablePropertiesDialog.
fn row_height_cm(row_node: &Value) -> Option<f64> {
    let attrs = row_node.get("attrs")?;
    if let Some(n) = attrs.get("rowHeight").and_then(Value::as_f64) {
        if n > 0.0 {
            return Some(n);
        }
    }
    if let Some(s) = attrs.get("data-height-cm").and_then(Value::as_str) {
        let cleaned = s.trim().trim_end_matches("cm").trim();
        if let Ok(n) = cleaned.parse::<f64>() {
            if n > 0.0 {
                return Some(n);
            }
        }
    }
    None
}

/// Convert the children of a table cell into DOCX paragraphs. Unlike the
/// previous version, we recognise block kinds (storyboard / figure / list)
/// and flatten them into the cell as a sequence of paragraphs rather than
/// routing them through `paragraph_from_inline` (which would drop their text).
fn build_table_cell(cell_node: &Value, ctx: &RenderCtx, _cell_padding_px: f64) -> TableCell {
    let mut cell = TableCell::new();
    let attrs = cell_node.get("attrs");

    // F4 — colspan → grid_span (antes era ignorado; tabelas com células
    // mescladas saíam desalinhadas no DOCX).
    if let Some(colspan) = attrs.and_then(|a| a.get("colspan")).and_then(Value::as_u64) {
        if colspan > 1 {
            cell = cell.grid_span(colspan as usize);
        }
    }
    // F1.2 — largura da célula (DXA) a partir da colwidth (px).
    if let Some(px) = cell_colwidth_px(cell_node) {
        cell = cell.width(px_to_twips(px), WidthType::Dxa);
    }
    // F4 — alinhamento vertical do conteúdo (data-valign do dialog).
    match attrs.and_then(|a| a.get("data-valign")).and_then(Value::as_str) {
        Some("middle") => cell = cell.vertical_align(VAlignType::Center),
        Some("bottom") => cell = cell.vertical_align(VAlignType::Bottom),
        _ => {}
    }
    // Cor de FUNDO da célula (attr `backgroundColor` — sombreado estilo Word).
    // Emite `<w:shd w:fill="RRGGBB">` (CellShading) só quando há cor válida;
    // "sem cor"/transparente NÃO emite nada (fundo padrão). Funciona pelo
    // attr no JSON — clone estático do cabeçalho/rodapé incluso —, não só pelo
    // NodeView, porque o walker lê direto o JSON ProseMirror salvo.
    if let Some(hex) = attrs
        .and_then(|a| a.get("backgroundColor"))
        .and_then(Value::as_str)
        .and_then(css_color_to_hex)
    {
        cell = cell.shading(Shading::new().shd_type(ShdType::Clear).color("auto").fill(hex));
    }

    let inner_children = children(cell_node);

    if inner_children.is_empty() {
        return cell.add_paragraph(empty_paragraph());
    }

    let mut any_added = false;
    for inner in inner_children {
        match type_of(&inner) {
            "paragraph" => {
                cell = cell.add_paragraph(paragraph_from_inline(&inner));
                any_added = true;
            }
            "heading" => {
                cell = cell.add_paragraph(heading_paragraph(&inner));
                any_added = true;
            }
            "bulletList" => {
                for p in list_to_paragraphs(&inner, ListKind::Bullet) {
                    cell = cell.add_paragraph(p);
                    any_added = true;
                }
            }
            "orderedList" => {
                for p in list_to_paragraphs(&inner, ListKind::Ordered) {
                    cell = cell.add_paragraph(p);
                    any_added = true;
                }
            }
            "storyboard" => {
                for p in storyboard_to_paragraphs(&inner, ctx) {
                    cell = cell.add_paragraph(p);
                    any_added = true;
                }
            }
            "figure" => {
                for p in figure_to_paragraphs(&inner, ctx) {
                    cell = cell.add_paragraph(p);
                    any_added = true;
                }
            }
            "quesitoList" => {
                for p in quesito_list_to_paragraphs(&inner) {
                    cell = cell.add_paragraph(p);
                    any_added = true;
                }
            }
            "signature" => {
                for p in signature_to_paragraphs(&inner) {
                    cell = cell.add_paragraph(p);
                    any_added = true;
                }
            }
            _ => {
                cell = cell.add_paragraph(fallback_paragraph(&inner));
                any_added = true;
            }
        }
    }

    if !any_added {
        cell = cell.add_paragraph(empty_paragraph());
    }
    cell
}

// ===========================================================================
// Figures

fn render_figure_top(docx: Docx, figure_node: &Value, ctx: &RenderCtx) -> Docx {
    let mut docx = docx;
    for p in figure_to_paragraphs(figure_node, ctx) {
        docx = docx.add_paragraph(p);
    }
    docx
}

/// Figures embed the real PNG/JPEG when possible (MVP 4) and fall back to
/// an italic placeholder otherwise. The placeholder mirrors Spike C
/// behaviour so older `.sicrodoc` files without `relative_path` keep
/// working.
fn figure_to_paragraphs(figure_node: &Value, ctx: &RenderCtx) -> Vec<Paragraph> {
    let attrs = figure_node.get("attrs");
    let kind = attrs
        .and_then(|a| a.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("image");
    let relative_path = attrs
        .and_then(|a| a.get("relative_path"))
        .and_then(Value::as_str);

    let mut out = Vec::new();

    // Try to embed the actual image, fall back to an italic placeholder.
    let mut embedded = false;
    if let (Some(ws), Some(rel)) = (ctx.workspace_root, relative_path) {
        if let Some(pic) = build_image_pic(ws, rel) {
            out.push(
                Paragraph::new()
                    .align(AlignmentType::Center)
                    .add_run(Run::new().add_image(pic)),
            );
            embedded = true;
        }
    }
    if !embedded {
        let placeholder_text = match kind {
            "croqui" => "[Croqui — imagem indisponível nesta exportação]",
            "video_frame" => "[Frame de vídeo — imagem indisponível nesta exportação]",
            _ => "[Figura — imagem indisponível nesta exportação]",
        };
        out.push(
            Paragraph::new()
                .align(AlignmentType::Center)
                .add_run(Run::new().add_text(placeholder_text).italic()),
        );
    }

    // Caption: the only child is figcaption.
    if let Some(figcaption) = figure_node
        .get("content")
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter()
                .find(|c| c.get("type").and_then(Value::as_str) == Some("figcaption"))
        })
    {
        let runs: Vec<Run> = children(figcaption)
            .iter()
            .flat_map(|i| inline_to_runs(i, Some(20), false))
            .collect();
        let mut p = Paragraph::new().align(AlignmentType::Center);
        let mut added = 0;
        for run in runs {
            p = p.add_run(run.italic());
            added += 1;
        }
        if added == 0 {
            p = p.add_run(Run::new().add_text("").italic());
        }
        out.push(p);
    }
    out
}

// ===========================================================================
// Storyboard

fn render_storyboard_top(mut docx: Docx, sb_node: &Value, ctx: &RenderCtx) -> Docx {
    // Caption above
    if let Some(caption) = sb_node
        .get("attrs")
        .and_then(|a| a.get("caption"))
        .and_then(Value::as_str)
    {
        docx = docx.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(caption).bold().size(22)),
        );
    }

    // One 2-col row per storyboard item: [meta] | [description]
    let items = children(sb_node);
    let mut rows: Vec<TableRow> = Vec::new();
    for item in items {
        rows.push(storyboard_item_to_row(&item, ctx));
    }

    if !rows.is_empty() {
        docx = docx.add_table(Table::new(rows));
        docx = docx.add_paragraph(empty_paragraph());
    }
    docx
}

fn storyboard_item_to_row(item: &Value, ctx: &RenderCtx) -> TableRow {
    let attrs = item.get("attrs");
    let timestamp = attrs
        .and_then(|a| a.get("timestamp"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let frame_label = attrs
        .and_then(|a| a.get("frame_label"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let relative_path = attrs
        .and_then(|a| a.get("relative_path"))
        .and_then(Value::as_str);

    // Image cell (left) — embed the actual frame if we have it, otherwise
    // fall back to a placeholder. Storyboard frames target a smaller width
    // (about 7 cm) so two columns still fit comfortably on the page.
    let mut meta_cell = TableCell::new();
    let mut image_paragraph_added = false;
    if let (Some(ws), Some(rel)) = (ctx.workspace_root, relative_path) {
        if let Some(pic) = build_storyboard_pic(ws, rel) {
            meta_cell = meta_cell.add_paragraph(
                Paragraph::new()
                    .align(AlignmentType::Center)
                    .add_run(Run::new().add_image(pic)),
            );
            image_paragraph_added = true;
        }
    }
    if !image_paragraph_added {
        meta_cell = meta_cell.add_paragraph(
            Paragraph::new().add_run(Run::new().add_text("[Frame indisponível]").italic()),
        );
    }
    meta_cell = meta_cell
        .add_paragraph(Paragraph::new().add_run(Run::new().add_text(timestamp).bold()))
        .add_paragraph(Paragraph::new().add_run(Run::new().add_text(frame_label)));

    let mut desc_cell = TableCell::new();
    let inner = children(item);
    if inner.is_empty() {
        desc_cell = desc_cell.add_paragraph(empty_paragraph());
    } else {
        for p_node in inner {
            desc_cell = desc_cell.add_paragraph(paragraph_from_inline(&p_node));
        }
    }

    TableRow::new(vec![meta_cell, desc_cell])
}

// ===========================================================================
// MVP 4 — evidenceTable (checklist / vestígios / medições)

fn render_evidence_table(mut docx: Docx, node: &Value) -> Docx {
    let attrs = node.get("attrs");
    let title = attrs
        .and_then(|a| a.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let columns: Vec<(String, String)> = attrs
        .and_then(|a| a.get("columns"))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let key = c.get("key").and_then(Value::as_str)?.to_string();
                    let label = c
                        .get("label")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    Some((key, label))
                })
                .collect()
        })
        .unwrap_or_default();
    let rows: Vec<&Value> = attrs
        .and_then(|a| a.get("rows"))
        .and_then(Value::as_array)
        .map(|v| v.iter().collect())
        .unwrap_or_default();

    if columns.is_empty() {
        return docx;
    }

    if !title.trim().is_empty() {
        docx = docx.add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text(title.clone()).bold().size(22)),
        );
    }

    // Header row
    let header_cells: Vec<TableCell> = columns
        .iter()
        .map(|(_, label)| {
            TableCell::new().add_paragraph(
                Paragraph::new()
                    .add_run(Run::new().add_text(label.clone()).bold()),
            )
        })
        .collect();

    let mut table_rows: Vec<TableRow> = Vec::new();
    table_rows.push(TableRow::new(header_cells));

    for row in rows {
        let cells: Vec<TableCell> = columns
            .iter()
            .map(|(key, _)| {
                let text = match row.get(key) {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Number(n)) => n.to_string(),
                    Some(Value::Bool(b)) => b.to_string(),
                    Some(Value::Null) | None => "—".to_string(),
                    Some(other) => other.to_string(),
                };
                TableCell::new()
                    .add_paragraph(Paragraph::new().add_run(Run::new().add_text(text)))
            })
            .collect();
        table_rows.push(TableRow::new(cells));
    }

    docx = docx.add_table(Table::new(table_rows));
    docx.add_paragraph(empty_paragraph())
}

// ===========================================================================
// MVP 4 — Image embedding helpers

/// Default body content width when fitting an embedded figure. ~14 cm fits
/// inside the PCA Padrão page geometry (A4, 3 cm top, 3.5 cm left, 2 cm
/// right ⇒ ~15.5 cm of usable width) with a small visual breathing room.
const FIGURE_TARGET_WIDTH_CM: f64 = 14.0;
const STORYBOARD_TARGET_WIDTH_CM: f64 = 7.0;
/// 1 cm in English Metric Units (docx-rs measures images in EMU).
const EMU_PER_CM: f64 = 360_000.0;

fn build_image_pic(workspace_root: &Path, relative_path: &str) -> Option<Pic> {
    build_image_pic_sized(workspace_root, relative_path, FIGURE_TARGET_WIDTH_CM)
}

/// Como `build_image_pic`, mas com largura-alvo explícita (cm). Usado pelo
/// cabeçalho, onde o brasão deve sair pequeno (dimensionado pelo `width` da
/// figura), não na largura cheia do corpo.
fn build_image_pic_sized(
    workspace_root: &Path,
    relative_path: &str,
    target_cm: f64,
) -> Option<Pic> {
    resolve_image_bytes(workspace_root, relative_path).and_then(|bytes| {
        let (w_emu, h_emu) = image_target_emu(&bytes, target_cm);
        Some(Pic::new(&bytes).size(w_emu, h_emu))
    })
}

/// Como `build_image_pic_sized`, mas a partir de BYTES já em memória (ex.: um
/// brasão embutido como data URI no cabeçalho-modelo portátil) — não depende do
/// workspace. `bytes` deve ser uma imagem válida (PNG/JPG).
fn build_pic_from_bytes_sized(bytes: &[u8], target_cm: f64) -> Option<Pic> {
    if bytes.is_empty() {
        return None;
    }
    let (w_emu, h_emu) = image_target_emu(bytes, target_cm);
    Some(Pic::new(bytes).size(w_emu, h_emu))
}

/// Versão flutuante de `build_pic_from_bytes_sized`: ancora a imagem relativa à
/// PÁGINA com offset POSITIVO (mesma convenção de `build_image_pic_floating`,
/// que evita o estouro de u32 do docx-rs).
fn build_pic_from_bytes_floating(
    bytes: &[u8],
    target_cm: f64,
    x_emu: i32,
    y_emu: i32,
) -> Option<Pic> {
    if bytes.is_empty() {
        return None;
    }
    let (w_emu, h_emu) = image_target_emu(bytes, target_cm);
    Some(
        Pic::new(bytes)
            .size(w_emu, h_emu)
            .floating()
            .relative_from_h(RelativeFromHType::Page)
            .relative_from_v(RelativeFromVType::Page)
            .position_h(DrawingPosition::Offset(x_emu))
            .position_v(DrawingPosition::Offset(y_emu)),
    )
}

/// Converte uma medida CSS ("2.5cm", "40mm", "1in", "96px", ou número puro
/// = cm) em centímetros. `None` se não parsear.
fn parse_len_cm(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some(v) = s.strip_suffix("cm") {
        v.trim().parse::<f64>().ok()
    } else if let Some(v) = s.strip_suffix("mm") {
        v.trim().parse::<f64>().ok().map(|x| x / 10.0)
    } else if let Some(v) = s.strip_suffix("in") {
        v.trim().parse::<f64>().ok().map(|x| x * 2.54)
    } else if let Some(v) = s.strip_suffix("px") {
        v.trim().parse::<f64>().ok().map(|x| x * 2.54 / 96.0)
    } else {
        s.parse::<f64>().ok()
    }
}

/// Largura útil de conteúdo (cm) = largura A4 (21cm) − margens esquerda/direita
/// do `envelope.layout.page.margins`. É a MESMA base que o editor usa pra
/// resolver larguras de figura em `%`. Fallback p/ margens ausentes: 3/2 cm.
fn usable_content_width_cm(envelope: &Value) -> f64 {
    const A4_WIDTH_CM: f64 = 21.0;
    let margins = envelope
        .get("layout")
        .and_then(|l| l.get("page"))
        .and_then(|p| p.get("margins"));
    let left = margins
        .and_then(|m| m.get("left"))
        .and_then(Value::as_str)
        .and_then(parse_len_cm)
        .unwrap_or(3.0);
    let right = margins
        .and_then(|m| m.get("right"))
        .and_then(Value::as_str)
        .and_then(parse_len_cm)
        .unwrap_or(2.0);
    (A4_WIDTH_CM - left - right).clamp(5.0, 20.0)
}

/// Largura-alvo (cm) de uma figura a partir do attr `width` ("13.4%", "3cm",
/// "40mm", "120px"). Porcentagem é relativa à largura útil. Fallback =
/// `default_cm`. Clampada a [0.8, usable].
fn figure_target_width_cm(node: &Value, usable_cm: f64, default_cm: f64) -> f64 {
    let raw = node
        .get("attrs")
        .and_then(|a| a.get("width"))
        .and_then(Value::as_str);
    let cm = match raw {
        Some(s) => {
            let s = s.trim();
            if let Some(p) = s.strip_suffix('%') {
                p.trim()
                    .parse::<f64>()
                    .ok()
                    .map(|v| (v / 100.0) * usable_cm)
            } else {
                parse_len_cm(s)
            }
        }
        None => None,
    };
    cm.unwrap_or(default_cm).clamp(0.8, usable_cm)
}

fn build_storyboard_pic(workspace_root: &Path, relative_path: &str) -> Option<Pic> {
    resolve_image_bytes(workspace_root, relative_path).and_then(|bytes| {
        let (w_emu, h_emu) = image_target_emu(&bytes, STORYBOARD_TARGET_WIDTH_CM);
        Some(Pic::new(&bytes).size(w_emu, h_emu))
    })
}

// ===========================================================================
// Fórmula matemática — embed do PNG (render_png, data URI) no DOCX.

/// Decodifica um data URI `data:image/png;base64,...` em bytes PNG.
fn decode_png_data_uri(s: &str) -> Option<Vec<u8>> {
    let comma = s.find(',')?;
    if !s[..comma].contains("base64") {
        return None;
    }
    base64::engine::general_purpose::STANDARD
        .decode(s[comma + 1..].trim().as_bytes())
        .ok()
}

/// `Pic` de uma fórmula, dimensionada pelo tamanho natural (render_w_cm/h_cm),
/// preservando o aspecto (re-escala se exceder a largura útil).
fn math_pic(node: &Value) -> Option<Pic> {
    let attrs = node.get("attrs")?;
    let bytes = decode_png_data_uri(attrs.get("render_png").and_then(Value::as_str)?)?;
    if !is_supported_image(&bytes) {
        return None;
    }
    let mut w_cm = attrs.get("render_w_cm").and_then(Value::as_f64).unwrap_or(0.0);
    let mut h_cm = attrs.get("render_h_cm").and_then(Value::as_f64).unwrap_or(0.0);
    if w_cm > 0.1 && h_cm > 0.1 {
        if w_cm > 16.0 {
            let scale = 16.0 / w_cm;
            w_cm *= scale;
            h_cm *= scale;
        }
        let w = (w_cm.clamp(0.3, 16.0) * EMU_PER_CM).round() as u32;
        let h = (h_cm.clamp(0.2, 24.0) * EMU_PER_CM).round() as u32;
        Some(Pic::new(&bytes).size(w, h))
    } else {
        let (w, h) = image_target_emu(&bytes, 6.0);
        Some(Pic::new(&bytes).size(w, h))
    }
}

fn math_latex(node: &Value) -> &str {
    node.get("attrs")
        .and_then(|a| a.get("latex"))
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// Fórmula de bloco → parágrafo centralizado com a imagem (ou LaTeX itálico).
fn render_math_block(docx: Docx, node: &Value) -> Docx {
    let p = Paragraph::new().align(AlignmentType::Center);
    let p = match math_pic(node) {
        Some(pic) => p.add_run(Run::new().add_image(pic)),
        None => p.add_run(Run::new().add_text(math_latex(node)).italic()),
    };
    docx.add_paragraph(p)
}

/// Fórmula inline → run com a imagem (ou LaTeX itálico no fallback).
fn math_runs(node: &Value) -> Vec<Run> {
    match math_pic(node) {
        Some(pic) => vec![Run::new().add_image(pic)],
        None => vec![Run::new().add_text(math_latex(node)).italic()],
    }
}

/// Read the bytes of an evidence asset, refusing path traversal. Returns
/// `None` for missing / unreadable / non-image files — the caller is
/// expected to fall back to a placeholder.
fn resolve_image_bytes(workspace_root: &Path, rel: &str) -> Option<Vec<u8>> {
    let safe = sanitize_relative_path(rel)?;
    let abs = workspace_root.join(&safe);
    let bytes = std::fs::read(&abs).ok()?;
    if !is_supported_image(&bytes) {
        return None;
    }
    Some(bytes)
}

fn sanitize_relative_path(raw: &str) -> Option<std::path::PathBuf> {
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with('/') || raw.starts_with('\\') {
        return None;
    }
    if let Some(c) = raw.chars().next() {
        if c.is_alphabetic() && raw[1..].starts_with(':') {
            return None;
        }
    }
    let mut out = std::path::PathBuf::new();
    for part in raw.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return None;
        }
        out.push(part);
    }
    Some(out)
}

fn is_supported_image(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
        || bytes.starts_with(&[0xFF, 0xD8, 0xFF])
}

/// Decide the docx-rs `size(width, height)` parameters for an image. The
/// width is fixed (`target_cm`); the height is derived from the file's
/// intrinsic aspect ratio so figures stay un-stretched. When the file
/// dimensions can't be parsed we fall back to a 4:3 aspect.
fn image_target_emu(bytes: &[u8], target_cm: f64) -> (u32, u32) {
    let (width_px, height_px) =
        image_dimensions(bytes).unwrap_or((4, 3)); // fallback ratio
    let aspect = if width_px == 0 {
        0.75
    } else {
        height_px as f64 / width_px as f64
    };
    let width_cm = target_cm.max(1.0);
    let height_cm = (width_cm * aspect).clamp(1.0, 18.0);
    let w = (width_cm * EMU_PER_CM).round() as u32;
    let h = (height_cm * EMU_PER_CM).round() as u32;
    (w, h)
}

/// Parse PNG / JPEG image dimensions. Returns `None` for unsupported or
/// truncated files. We deliberately stay free of `image`/`imagesize`
/// dependencies — the PNG IHDR and JPEG SOF segments are tiny.
fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        // PNG: 8-byte signature + 4 length + 4 type "IHDR" + 4 width + 4 height
        if bytes.len() < 24 {
            return None;
        }
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return Some((w, h));
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        // Walk JPEG markers until we hit an SOFn (start of frame) segment.
        let mut i = 2usize;
        while i + 9 < bytes.len() {
            // Marker must start with 0xFF; skip padding 0xFFs.
            if bytes[i] != 0xFF {
                return None;
            }
            while i < bytes.len() && bytes[i] == 0xFF {
                i += 1;
            }
            if i >= bytes.len() {
                return None;
            }
            let marker = bytes[i];
            i += 1;
            // Standalone markers (no segment body).
            if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) {
                continue;
            }
            if i + 1 >= bytes.len() {
                return None;
            }
            let seg_len = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as usize;
            if seg_len < 2 || i + seg_len > bytes.len() {
                return None;
            }
            // SOFn = 0xC0..=0xCF excluding 0xC4 (DHT), 0xC8 (JPG), 0xCC (DAC).
            if (0xC0..=0xCF).contains(&marker)
                && marker != 0xC4
                && marker != 0xC8
                && marker != 0xCC
            {
                if i + 7 >= bytes.len() {
                    return None;
                }
                // i points to seg_len (2 bytes); after that is precision (1),
                // then height (2 BE), then width (2 BE).
                let h = u16::from_be_bytes([bytes[i + 3], bytes[i + 4]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                return Some((w, h));
            }
            i += seg_len;
        }
        return None;
    }
    None
}

// ===========================================================================
// Quesito (MVP 2)

fn render_quesito_list(mut docx: Docx, list_node: &Value) -> Docx {
    for p in quesito_list_to_paragraphs(list_node) {
        docx = docx.add_paragraph(p);
    }
    docx
}

fn quesito_list_to_paragraphs(list_node: &Value) -> Vec<Paragraph> {
    let mut out = Vec::new();
    for (idx, item) in children(list_node).into_iter().enumerate() {
        let mut question = String::new();
        let mut answer = String::new();
        for child in children(&item) {
            match type_of(&child) {
                "quesitoQuestion" => collect_text(&child, &mut question),
                "quesitoAnswer" => collect_text(&child, &mut answer),
                _ => {}
            }
        }

        // "Quesito N: <pergunta>"
        let mut q = Paragraph::new().align(AlignmentType::Both);
        q = q.add_run(
            Run::new()
                .add_text(format!("Quesito {}: ", idx + 1))
                .bold(),
        );
        if question.trim().is_empty() {
            q = q.add_run(Run::new().add_text("(sem pergunta)"));
        } else {
            q = q.add_run(Run::new().add_text(question));
        }
        out.push(q);

        // "Resposta: <resposta>"
        let mut a = Paragraph::new()
            .align(AlignmentType::Both)
            .indent(Some(360), None, None, None);
        a = a.add_run(Run::new().add_text("Resposta: ").bold());
        if answer.trim().is_empty() {
            a = a.add_run(Run::new().add_text("(a preencher)").italic());
        } else {
            a = a.add_run(Run::new().add_text(answer));
        }
        out.push(a);

        // Spacer
        out.push(empty_paragraph());
    }
    out
}

// ===========================================================================
// Signature (MVP 2)

fn render_signature(mut docx: Docx, sig_node: &Value) -> Docx {
    for p in signature_to_paragraphs(sig_node) {
        docx = docx.add_paragraph(p);
    }
    docx
}

fn signature_to_paragraphs(sig_node: &Value) -> Vec<Paragraph> {
    let attrs = sig_node.get("attrs");
    let city = attrs
        .and_then(|a| a.get("city"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let uf = attrs
        .and_then(|a| a.get("uf"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let date = attrs
        .and_then(|a| a.get("date"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let name = attrs
        .and_then(|a| a.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let role = attrs
        .and_then(|a| a.get("role"))
        .and_then(Value::as_str)
        .unwrap_or("Perito Criminal");

    let place_line = if !city.is_empty() || !uf.is_empty() {
        format!("{} - {}, {}.", city, uf, format_pt_br_date(date))
    } else {
        format_pt_br_date(date)
    };

    vec![
        // empty spacer
        empty_paragraph(),
        // Place + date — right-aligned per institutional convention
        Paragraph::new()
            .align(AlignmentType::Right)
            .add_run(Run::new().add_text(place_line)),
        // Blank line for the signature
        empty_paragraph(),
        // Rule
        Paragraph::new()
            .align(AlignmentType::Center)
            .add_run(Run::new().add_text("_______________________________")),
        // Name (bold, centered)
        Paragraph::new()
            .align(AlignmentType::Center)
            .add_run(Run::new().add_text(if name.is_empty() {
                "(nome do perito)".to_string()
            } else {
                name.to_string()
            }).bold()),
        // Role (italic, centered)
        Paragraph::new()
            .align(AlignmentType::Center)
            .add_run(Run::new().add_text(role).italic()),
    ]
}

fn format_pt_br_date(iso: &str) -> String {
    // Accept "YYYY-MM-DD" or full ISO; output "DD/MM/YYYY". Empty input
    // returns a blank placeholder.
    if iso.is_empty() {
        return "______ / ______ / ______".to_string();
    }
    if iso.len() < 10 {
        return iso.to_string();
    }
    let date_part = &iso[..10];
    let parts: Vec<&str> = date_part.split('-').collect();
    if parts.len() != 3 {
        return date_part.to_string();
    }
    format!("{}/{}/{}", parts[2], parts[1], parts[0])
}

/// Flatten a storyboard into a sequence of paragraphs (used when the
/// storyboard appears inside a cell — nesting a 2-col table inside a
/// cell makes Word lay things out unpredictably).
fn storyboard_to_paragraphs(sb_node: &Value, ctx: &RenderCtx) -> Vec<Paragraph> {
    let mut out = Vec::new();
    if let Some(caption) = sb_node
        .get("attrs")
        .and_then(|a| a.get("caption"))
        .and_then(Value::as_str)
    {
        out.push(Paragraph::new().add_run(Run::new().add_text(caption).bold()));
    }

    for (idx, item) in children(sb_node).into_iter().enumerate() {
        let attrs = item.get("attrs");
        let timestamp = attrs
            .and_then(|a| a.get("timestamp"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let frame_label = attrs
            .and_then(|a| a.get("frame_label"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let relative_path = attrs
            .and_then(|a| a.get("relative_path"))
            .and_then(Value::as_str);

        out.push(
            Paragraph::new().add_run(
                Run::new()
                    .add_text(format!(
                        "Item {} — {} | {}",
                        idx + 1,
                        timestamp,
                        frame_label
                    ))
                    .bold(),
            ),
        );

        // Best-effort: embed the actual frame even when nested in a cell.
        if let (Some(ws), Some(rel)) = (ctx.workspace_root, relative_path) {
            if let Some(pic) = build_storyboard_pic(ws, rel) {
                out.push(
                    Paragraph::new()
                        .align(AlignmentType::Center)
                        .add_run(Run::new().add_image(pic)),
                );
            }
        }

        for inner in children(&item) {
            out.push(paragraph_from_inline(&inner));
        }
    }
    out
}

// ===========================================================================
// Inline rendering

fn inline_to_runs(node: &Value, base_size: Option<usize>, bold_default: bool) -> Vec<Run> {
    match type_of(node) {
        "text" => vec![text_to_run(node, base_size, bold_default)],
        "hardBreak" => vec![Run::new().add_break(BreakType::TextWrapping)],
        // Fórmula matemática INLINE → run com a imagem renderizada (ou LaTeX
        // itálico no fallback).
        "mathInline" => math_runs(node),
        "systemData" => {
            let value = node
                .get("attrs")
                .and_then(|a| a.get("value"))
                .and_then(Value::as_str)
                .unwrap_or("");
            // Distinguish system data with a muted color (gray).
            vec![Run::new().add_text(value).color("808080")]
        }
        // Pílula de campo automático (`{numero_laudo}` etc.). Os campos com
        // valor JÁ foram pré-resolvidos em `resolve_field_placeholders_in_envelope`
        // (viraram nós de texto) usando os valores que o front resolveu. O que
        // CHEGA aqui como pílula é só: (a) page/pages → campo NATIVO do Word, ou
        // (b) campo sem valor → placeholder textual `{<key>}` em laranja (mostra
        // pendência). Sem este branch, o walker perdia a pílula (`_ => Vec::new()`).
        "fieldPlaceholder" => {
            let key = node
                .get("attrs")
                .and_then(|a| a.get("field"))
                .and_then(Value::as_str)
                .unwrap_or("");
            match key {
                "" => Vec::new(),
                // page/pages → campos NATIVOS do Word (PAGE / NUMPAGES). O Word
                // calcula o número real (no rodapé, repete por página). É a
                // forma fiel de "Folha X de Y" no .docx.
                "page" => vec![word_page_field(InstrText::PAGE(InstrPAGE {}))],
                "pages" => {
                    vec![word_page_field(InstrText::NUMPAGES(InstrNUMPAGES {}))]
                }
                // Demais campos: o walker do backend não tem o resolver
                // (occurrence vive no front), então emite o placeholder textual
                // `{<key>}` em laranja — o usuário vê a chave e pode editar.
                _ => vec![Run::new()
                    .add_text(&format!("{{{key}}}"))
                    .color("B86F08")],
            }
        }
        _ => Vec::new(),
    }
}

/// Run com um campo nativo do Word (PAGE/NUMPAGES): begin → instr → separate →
/// valor em cache ("1") → end. Word recalcula ao abrir/atualizar campos.
fn word_page_field(instr: InstrText) -> Run {
    Run::new()
        .add_field_char(FieldCharType::Begin, false)
        .add_instr_text(instr)
        .add_field_char(FieldCharType::Separate, false)
        .add_text("1")
        .add_field_char(FieldCharType::End, false)
}

fn text_to_run(node: &Value, base_size: Option<usize>, bold_default: bool) -> Run {
    let text = node.get("text").and_then(Value::as_str).unwrap_or("");
    let mut run = Run::new().add_text(text);
    if let Some(size) = base_size {
        run = run.size(size);
    }
    if bold_default {
        run = run.bold();
    }

    let marks = node
        .get("marks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for mark in marks {
        match mark.get("type").and_then(Value::as_str).unwrap_or("") {
            "bold" => run = run.bold(),
            "italic" => run = run.italic(),
            "underline" => run = run.underline("single"),
            "strike" => run = run.strike(),
            "code" => run = run.fonts(RunFonts::new().east_asia("Consolas").ascii("Consolas")),
            // TextStyle carrega tamanho/fonte/cor inline (definidos pelo perito
            // na toolbar). Aplicar como propriedades EXPLÍCITAS do run, que
            // SOBRESCREVEM o default do documento (TNR 12pt). É isto que torna o
            // 12pt apenas um padrão — não uma trava: mudar na tela reflete no
            // DOCX/PDF-LibreOffice, igual já reflete no editor e no HTML/PDF-Edge.
            "textStyle" => {
                if let Some(attrs) = mark.get("attrs") {
                    if let Some(hp) = attrs
                        .get("fontSize")
                        .and_then(Value::as_str)
                        .and_then(css_size_to_half_points)
                    {
                        run = run.size(hp);
                    }
                    if let Some(fam) = attrs
                        .get("fontFamily")
                        .and_then(Value::as_str)
                        .and_then(css_first_family)
                    {
                        run = run.fonts(RunFonts::new().ascii(&fam).hi_ansi(&fam));
                    }
                    if let Some(col) = attrs.get("color").and_then(Value::as_str) {
                        let hex = col.trim().trim_start_matches('#');
                        if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                            run = run.color(hex);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    run
}

/// Converte um `font-size` CSS ("14pt", "16px", "1.5em") em meios-pontos (a
/// unidade do docx-rs/OOXML). Base de `em` = 12pt. Devolve None se inválido.
fn css_size_to_half_points(v: &str) -> Option<usize> {
    let s = v.trim();
    let num: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let n: f64 = num.parse().ok()?;
    if n <= 0.0 {
        return None;
    }
    let pt = if s.ends_with("px") {
        n * 72.0 / 96.0
    } else if s.ends_with("em") {
        n * 12.0
    } else {
        n // "pt" ou sem unidade → assume pt
    };
    let hp = (pt * 2.0).round() as i64;
    if hp > 0 {
        Some(hp as usize)
    } else {
        None
    }
}

/// Extrai a PRIMEIRA família de uma `font-family` CSS ("Arial, sans-serif" →
/// "Arial"), sem aspas. Devolve None se vazio.
fn css_first_family(v: &str) -> Option<String> {
    let first = v
        .split(',')
        .next()?
        .trim()
        .trim_matches(|c| c == '\'' || c == '"')
        .trim();
    if first.is_empty() {
        None
    } else {
        Some(first.to_string())
    }
}

/// Converte uma cor CSS (`#RGB`, `#RRGGBB`, `rgb()/rgba()`) em hex `RRGGBB`
/// (UPPERCASE, sem `#`) — o formato que o OOXML `<w:shd w:fill="…">` espera.
///
/// Devolve `None` para "sem cor" (vazio, `transparent`, `none`) ou entrada
/// inválida — nesses casos a célula NÃO recebe sombreado (fundo padrão do
/// Word), preservando a regra "default = transparente" dos docs antigos.
/// `rgba(...)` com alpha 0 também é tratado como "sem cor".
fn css_color_to_hex(value: &str) -> Option<String> {
    let v = value.trim();
    if v.is_empty() || v.eq_ignore_ascii_case("transparent") || v.eq_ignore_ascii_case("none") {
        return None;
    }
    if let Some(hex) = v.strip_prefix('#') {
        let hex = hex.trim();
        match hex.len() {
            6 if hex.chars().all(|c| c.is_ascii_hexdigit()) => {
                return Some(hex.to_uppercase());
            }
            3 if hex.chars().all(|c| c.is_ascii_hexdigit()) => {
                // #RGB → #RRGGBB
                let mut out = String::with_capacity(6);
                for c in hex.chars() {
                    out.push(c);
                    out.push(c);
                }
                return Some(out.to_uppercase());
            }
            _ => return None,
        }
    }
    // rgb()/rgba()
    let lower = v.to_ascii_lowercase();
    if let Some(inner) = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))
        .and_then(|s| s.strip_suffix(')'))
    {
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() < 3 {
            return None;
        }
        // alpha 0 → "sem cor" (não emite shading).
        if parts.len() >= 4 {
            if let Ok(a) = parts[3].parse::<f64>() {
                if a <= 0.0 {
                    return None;
                }
            }
        }
        let r = parts[0].parse::<u32>().ok()?.min(255);
        let g = parts[1].parse::<u32>().ok()?.min(255);
        let b = parts[2].parse::<u32>().ok()?.min(255);
        return Some(format!("{r:02X}{g:02X}{b:02X}"));
    }
    None
}

// ===========================================================================
// Generic helpers

fn type_of(node: &Value) -> &str {
    node.get("type").and_then(Value::as_str).unwrap_or("")
}

fn children(node: &Value) -> Vec<Value> {
    node.get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn paragraph_alignment(node: &Value) -> Option<AlignmentType> {
    let attr = node
        .get("attrs")
        .and_then(|a| a.get("textAlign"))
        .and_then(Value::as_str)?;
    match attr {
        "center" => Some(AlignmentType::Center),
        "right" => Some(AlignmentType::Right),
        "justify" => Some(AlignmentType::Both),
        _ => Some(AlignmentType::Left),
    }
}

// ===========================================================================
// Testes — cor de fundo de célula (CSS → hex p/ <w:shd>).

#[cfg(test)]
mod cell_bg_tests {
    use super::css_color_to_hex;

    #[test]
    fn hex_6_digits_uppercased() {
        assert_eq!(css_color_to_hex("#ffcc00").as_deref(), Some("FFCC00"));
        assert_eq!(css_color_to_hex("#1A2B3C").as_deref(), Some("1A2B3C"));
    }

    #[test]
    fn hex_3_digits_expanded() {
        assert_eq!(css_color_to_hex("#fc0").as_deref(), Some("FFCC00"));
        assert_eq!(css_color_to_hex("#abc").as_deref(), Some("AABBCC"));
    }

    #[test]
    fn rgb_and_rgba_converted() {
        assert_eq!(css_color_to_hex("rgb(255, 204, 0)").as_deref(), Some("FFCC00"));
        assert_eq!(
            css_color_to_hex("rgba(26, 43, 60, 1)").as_deref(),
            Some("1A2B3C")
        );
    }

    #[test]
    fn no_color_inputs_return_none() {
        // Default (sem cor) NÃO deve emitir shading.
        assert!(css_color_to_hex("").is_none());
        assert!(css_color_to_hex("   ").is_none());
        assert!(css_color_to_hex("transparent").is_none());
        assert!(css_color_to_hex("none").is_none());
        // rgba com alpha 0 = transparente.
        assert!(css_color_to_hex("rgba(255,0,0,0)").is_none());
    }

    #[test]
    fn invalid_inputs_return_none() {
        assert!(css_color_to_hex("#12").is_none());
        assert!(css_color_to_hex("#xyzxyz").is_none());
        assert!(css_color_to_hex("notacolor").is_none());
        assert!(css_color_to_hex("rgb(1,2)").is_none());
    }
}

// ===========================================================================
// Testes — fórmula matemática (decode do data URI + Pic).

#[cfg(test)]
mod math_tests {
    use super::*;
    use serde_json::json;

    // PNG 1×1 transparente (válido, magic bytes corretos).
    const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

    #[test]
    fn decode_png_data_uri_ok() {
        let uri = format!("data:image/png;base64,{PNG_1X1}");
        let bytes = decode_png_data_uri(&uri).expect("deve decodificar");
        assert!(is_supported_image(&bytes), "bytes devem ser PNG válido");
    }

    #[test]
    fn decode_png_data_uri_rejects_bad_input() {
        assert!(decode_png_data_uri("sem virgula").is_none());
        assert!(decode_png_data_uri("data:image/png,NOTBASE64").is_none());
    }

    #[test]
    fn math_pic_some_when_png_present() {
        // PNG 2×2 VÁLIDO gerado pelo crate `image` — o docx-rs decodifica a
        // imagem de fato (valida CRC), então não dá pra usar um PNG forjado.
        let img = image::DynamicImage::new_rgba8(2, 2);
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png)
            .expect("encode png");
        let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
        let node = json!({
            "type": "mathBlock",
            "attrs": {
                "latex": r"k = \frac{L}{P}",
                "render_png": format!("data:image/png;base64,{b64}"),
                "render_w_cm": 3.0,
                "render_h_cm": 1.2,
            }
        });
        assert!(math_pic(&node).is_some(), "com render_png válido → Pic embutido");
    }

    #[test]
    fn math_pic_none_without_png_falls_back_to_latex() {
        let node = json!({
            "type": "mathBlock",
            "attrs": { "latex": "E = mc^2" }
        });
        assert!(math_pic(&node).is_none(), "sem render_png → sem Pic");
        assert_eq!(math_latex(&node), "E = mc^2", "fallback usa o LaTeX");
    }

    // ----- Dimensionamento de figura/cabeçalho (fix do brasão no header) -----

    #[test]
    fn parse_len_cm_handles_units() {
        assert_eq!(parse_len_cm("2.5cm"), Some(2.5));
        assert_eq!(parse_len_cm("40mm"), Some(4.0));
        assert_eq!(parse_len_cm("1in"), Some(2.54));
        assert!((parse_len_cm("96px").unwrap() - 2.54).abs() < 1e-6);
        assert_eq!(parse_len_cm("3"), Some(3.0)); // número puro = cm
        assert_eq!(parse_len_cm("abc"), None);
    }

    #[test]
    fn usable_width_from_envelope_margins() {
        let env = json!({ "layout": { "page": { "margins": {
            "left": "2.5cm", "right": "2.5cm", "top": "8.44cm", "bottom": "2.5cm"
        }}}});
        // A4 21cm − 2.5 − 2.5 = 16cm (mesma base do editor p/ larguras em %).
        assert!((usable_content_width_cm(&env) - 16.0).abs() < 1e-6);
    }

    #[test]
    fn header_figure_width_percent_is_relative_to_usable() {
        // Brasão do cabeçalho: width "13.4%" de 16cm ≈ 2.14cm (pequeno),
        // NÃO os 14cm do corpo — regressão do "brasão gigante no header".
        let fig = json!({ "type": "figure", "attrs": { "width": "13.4%" } });
        let w = figure_target_width_cm(&fig, 16.0, 3.0);
        assert!((w - 2.144).abs() < 0.01, "esperava ~2.14cm, veio {w}");
        // cm explícito passa direto; sem width → default.
        let fig_cm = json!({ "type": "figure", "attrs": { "width": "5cm" } });
        assert!((figure_target_width_cm(&fig_cm, 16.0, 3.0) - 5.0).abs() < 1e-6);
        let fig_none = json!({ "type": "figure", "attrs": {} });
        assert!((figure_target_width_cm(&fig_none, 16.0, 3.0) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn header_float_offsets_never_negative() {
        // Badge "fora" (wrap_x = -1.85, margem 2.5) → X = 0.65cm (>= 0, SEM o
        // estouro de u32 que corrompia o DOCX no Word); Y preserva wrap_y.
        let (x, y) = header_float_offsets_cm(2.5, -1.85, 24.18);
        assert!(x >= 0.0 && (x - 0.65).abs() < 1e-6, "X = {x}");
        assert!((y - 24.18).abs() < 1e-6, "Y = {y}");
        // wrap_x absurdamente negativo → clampa em 0 (nunca negativo).
        let (x2, _) = header_float_offsets_cm(2.5, -99.0, 5.0);
        assert_eq!(x2, 0.0);
        // Y fora da folha → clampa em 28cm.
        let (_, y2) = header_float_offsets_cm(2.5, 0.0, 99.0);
        assert_eq!(y2, 28.0);
    }

    #[test]
    fn lateral_mark_preserves_editor_left_space() {
        // Caixa real do laudo: wrap_x=-12.58, wrap_y=13.26, 24.04×2cm, girada
        // -90° na margem 2.5. O editor mostra a faixa com ESPAÇO à esquerda
        // (canto ~0.94cm da borda), NÃO colada (o bug antigo dava ~0.25cm).
        let (l, t) = lateral_mark_offsets_cm(2.5, -12.58, 13.26, 24.04, 2.0);
        assert!((l - 0.94).abs() < 0.05, "left esperado ~0.94cm, veio {l}");
        assert!((t - 2.24).abs() < 0.05, "top esperado ~2.24cm, veio {t}");
        assert!(l >= 0.0 && t >= 0.0, "offsets nunca negativos");
        // Faixa que sairia da folha pela esquerda → clampa em 0 (não negativo).
        let (l2, _) = lateral_mark_offsets_cm(0.5, -50.0, 13.0, 24.0, 2.0);
        assert_eq!(l2, 0.0);
        // Comprimento que estouraria a folha → top recuado p/ caber.
        let (_, t2) = lateral_mark_offsets_cm(2.5, 0.0, 50.0, 28.0, 2.0);
        assert!(t2 <= 29.7 - 28.0 + 1e-9, "top deve recuar p/ caber, veio {t2}");
    }

    #[test]
    fn field_placeholders_resolve_from_injected_values() {
        // Envelope com pílulas no cabeçalho: numero_laudo (com valor),
        // numero_requisicao (sem valor) e page (contador). Só numero_laudo deve
        // virar texto; page e o campo vazio continuam pílula.
        let envelope = serde_json::json!({
            "__field_values": { "numero_laudo": "38916/2026" },
            "header": { "content": { "type": "doc", "content": [
                { "type": "paragraph", "content": [
                    { "type": "text", "text": "Laudo nº " },
                    { "type": "fieldPlaceholder", "attrs": { "field": "numero_laudo" } },
                    { "type": "fieldPlaceholder", "attrs": { "field": "numero_requisicao" } },
                    { "type": "fieldPlaceholder", "attrs": { "field": "page" } }
                ] }
            ] } },
            "content": { "type": "doc", "content": [] }
        });
        let resolved = resolve_field_placeholders_in_envelope(&envelope);
        let runs = &resolved["header"]["content"]["content"][0]["content"];
        // numero_laudo → nó de texto com o valor.
        assert_eq!(runs[1]["type"], "text");
        assert_eq!(runs[1]["text"], "38916/2026");
        // numero_requisicao (sem valor) → continua pílula.
        assert_eq!(runs[2]["type"], "fieldPlaceholder");
        // page (contador) → continua pílula → campo nativo do Word no walker.
        assert_eq!(runs[3]["type"], "fieldPlaceholder");
        assert_eq!(runs[3]["attrs"]["field"], "page");
    }

    #[test]
    fn header_data_uri_pic_builds_from_bytes() {
        // Brasão embutido como data URI (cabeçalho-modelo PORTÁTIL) → bytes →
        // Pic dimensionada/flutuante, sem depender do workspace. PNG 2×2 VÁLIDO
        // gerado pelo crate `image` (o docx-rs decodifica de fato — PNG forjado
        // dá CRC mismatch).
        let img = image::DynamicImage::new_rgba8(2, 2);
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png)
            .expect("encode png");
        let bytes = buf.into_inner();
        assert!(build_pic_from_bytes_sized(&bytes, 3.0).is_some());
        assert!(build_pic_from_bytes_floating(&bytes, 3.0, 100, 200).is_some());
        // Bytes vazios → None (sem panic).
        assert!(build_pic_from_bytes_sized(&[], 3.0).is_none());
        assert!(build_pic_from_bytes_floating(&[], 3.0, 0, 0).is_none());
    }
}
