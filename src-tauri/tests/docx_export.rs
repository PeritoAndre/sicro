//! Integration test for the DOCX exporter.
//!
//! Spike C runtime feedback (2026-05-24): the user reported the DOCX showed
//! only the title. Forensic inspection of the workspace revealed the exported
//! .sicrodoc was actually empty (one empty paragraph) — the walker did the
//! right thing on the input it received. This test pins that contract: given
//! a rich .sicrodoc envelope, every textual leaf must surface in
//! `word/document.xml` of the produced DOCX.

use std::io::Read;
use std::path::Path;

use serde_json::json;
use sicro_desktop_lib::exporters::docx::render_doc_to_docx;

fn extract_document_xml(docx_path: &Path) -> String {
    let file = std::fs::File::open(docx_path).expect("docx exists");
    let mut archive = zip::ZipArchive::new(file).expect("docx is a zip");
    let mut entry = archive
        .by_name("word/document.xml")
        .expect("docx has word/document.xml");
    let mut s = String::new();
    entry.read_to_string(&mut s).expect("document.xml is utf-8");
    s
}

fn assert_contains_all(xml: &str, expected: &[&str]) {
    for needle in expected {
        assert!(
            xml.contains(needle),
            "DOCX document.xml is missing {needle:?}; xml dump:\n{xml}",
        );
    }
}

#[test]
fn renders_paragraphs_headings_marks_table_figure_storyboard() {
    let envelope = json!({
        "title": "Laudo de Teste — André",
        "content": {
            "type": "doc",
            "content": [
                { "type": "heading", "attrs": { "level": 1 }, "content": [
                    { "type": "text", "text": "Introducao" }
                ]},
                { "type": "heading", "attrs": { "level": 2 }, "content": [
                    { "type": "text", "text": "Secao A" }
                ]},
                { "type": "paragraph", "attrs": { "textAlign": "justify" }, "content": [
                    { "type": "text", "text": "Esteve" }
                ]},
                { "type": "paragraph", "content": [
                    { "type": "text", "text": "TextoBold", "marks": [{ "type": "bold" }] }
                ]},
                { "type": "paragraph", "content": [
                    { "type": "text", "text": "TextoItalic", "marks": [{ "type": "italic" }] }
                ]},
                { "type": "paragraph", "content": [
                    { "type": "text", "text": "TextoUnderline", "marks": [{ "type": "underline" }] }
                ]},
                { "type": "paragraph", "content": [
                    { "type": "text", "text": "Aqui" },
                    { "type": "systemData", "attrs": { "source": "occurrence", "field": "municipio", "value": "MunicipioInline", "review_status": "pending" }}
                ]},
                { "type": "bulletList", "content": [
                    { "type": "listItem", "content": [
                        { "type": "paragraph", "content": [{ "type": "text", "text": "BulletAlpha" }] }
                    ]},
                    { "type": "listItem", "content": [
                        { "type": "paragraph", "content": [{ "type": "text", "text": "BulletBeta" }] }
                    ]},
                ]},
                { "type": "orderedList", "content": [
                    { "type": "listItem", "content": [
                        { "type": "paragraph", "content": [{ "type": "text", "text": "OrderedAlpha" }] }
                    ]},
                    { "type": "listItem", "content": [
                        { "type": "paragraph", "content": [{ "type": "text", "text": "OrderedBeta" }] }
                    ]},
                ]},
                {
                    "type": "table",
                    "content": [
                        { "type": "tableRow", "content": [
                            { "type": "tableHeader", "content": [
                                { "type": "paragraph", "content": [{ "type": "text", "text": "HeaderCell" }] }
                            ]},
                            { "type": "tableHeader", "content": [
                                { "type": "paragraph", "content": [{ "type": "text", "text": "HeaderTwo" }] }
                            ]},
                        ]},
                        { "type": "tableRow", "content": [
                            { "type": "tableCell", "content": [
                                { "type": "paragraph", "content": [{ "type": "text", "text": "CellA1" }] }
                            ]},
                            { "type": "tableCell", "content": [
                                { "type": "paragraph", "content": [{ "type": "text", "text": "CellB1" }] }
                            ]},
                        ]},
                    ]
                },
                {
                    "type": "figure",
                    "attrs": { "kind": "image", "src": "data:image/svg+xml;utf8,...", "alt": "" },
                    "content": [
                        { "type": "figcaption", "content": [{ "type": "text", "text": "LegendaFigura" }] }
                    ]
                },
                {
                    "type": "storyboard",
                    "attrs": { "caption": "CaptionStoryboard" },
                    "content": [
                        {
                            "type": "storyboardItem",
                            "attrs": { "timestamp": "00:00:12.500", "frame_label": "Frame: 300" },
                            "content": [
                                { "type": "paragraph", "content": [{ "type": "text", "text": "DescricaoEvento1" }] }
                            ]
                        }
                    ]
                },
                // Nested case the real .sicrodoc had: a storyboard *inside* a tableHeader.
                {
                    "type": "table",
                    "content": [
                        { "type": "tableRow", "content": [
                            { "type": "tableHeader", "content": [
                                {
                                    "type": "storyboard",
                                    "attrs": { "caption": "CaptionNested" },
                                    "content": [
                                        { "type": "storyboardItem",
                                          "attrs": { "timestamp": "00:00:00.000", "frame_label": "Frame: 0" },
                                          "content": [
                                              { "type": "paragraph", "content": [{ "type": "text", "text": "NestedInsideCell" }] }
                                          ]
                                        }
                                    ]
                                }
                            ]}
                        ]}
                    ]
                }
            ]
        }
    });

    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("rich.docx");
    render_doc_to_docx(&envelope, &path, None).expect("render_doc_to_docx ok");

    let xml = extract_document_xml(&path);

    assert_contains_all(
        &xml,
        &[
            // Title
            "Laudo de Teste",
            // Headings
            "Introducao",
            "Secao A",
            // Paragraphs + marks
            "Esteve",
            "TextoBold",
            "TextoItalic",
            "TextoUnderline",
            "Aqui",
            "MunicipioInline",
            // Lists
            "BulletAlpha",
            "BulletBeta",
            "OrderedAlpha",
            "OrderedBeta",
            // Table (header + cells)
            "HeaderCell",
            "HeaderTwo",
            "CellA1",
            "CellB1",
            // Figure
            "LegendaFigura",
            // Storyboard top-level
            "CaptionStoryboard",
            "DescricaoEvento1",
            // Storyboard nested in tableHeader (real-world case from the
            // user's .sicrodoc — was being lost before the fix)
            "CaptionNested",
            "NestedInsideCell",
        ],
    );

    // Spot-check formatting marks were not dropped.
    assert!(
        xml.contains("<w:b /><w:bCs />") || xml.contains("<w:b/>"),
        "expected at least one bold run; xml:\n{xml}"
    );
    assert!(
        xml.contains("<w:i /><w:iCs />") || xml.contains("<w:i/>"),
        "expected at least one italic run; xml:\n{xml}"
    );
    assert!(
        xml.contains("w:u "),
        "expected at least one underline run; xml:\n{xml}"
    );
}

#[test]
fn renders_table_with_caption_widths_align_and_rowheight() {
    // Overhaul tabelas (F1.2/F3/F4): colwidths → grid <w:gridCol>, alinhamento
    // → <w:jc>, legenda → parágrafo abaixo, altura de linha → <w:trHeight>,
    // colspan → <w:gridSpan>.
    let envelope = json!({
        "title": "Tabela rica",
        "content": {
            "type": "doc",
            "content": [
                {
                    "type": "table",
                    "attrs": {
                        "id": "tbl-1",
                        "caption": "Tabela 1 — Dados do local",
                        "tableAlign": "center",
                        "borderStyle": "all",
                        "cellPadding": 5
                    },
                    "content": [
                        { "type": "tableRow", "attrs": { "rowHeight": 1.5 }, "content": [
                            { "type": "tableHeader", "attrs": { "colspan": 2, "rowspan": 1, "colwidth": [200, 200] }, "content": [
                                { "type": "paragraph", "content": [{ "type": "text", "text": "Cabecalho2col" }] }
                            ]},
                        ]},
                        { "type": "tableRow", "content": [
                            { "type": "tableCell", "attrs": { "colspan": 1, "rowspan": 1, "colwidth": [200], "data-valign": "middle" }, "content": [
                                { "type": "paragraph", "content": [{ "type": "text", "text": "CelEsq" }] }
                            ]},
                            { "type": "tableCell", "attrs": { "colspan": 1, "rowspan": 1, "colwidth": [200] }, "content": [
                                { "type": "paragraph", "content": [{ "type": "text", "text": "CelDir" }] }
                            ]},
                        ]},
                    ]
                }
            ]
        }
    });

    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("table-rich.docx");
    render_doc_to_docx(&envelope, &path, None).expect("render_doc_to_docx ok");
    let xml = extract_document_xml(&path);

    assert_contains_all(
        &xml,
        &[
            "Cabecalho2col",
            "CelEsq",
            "CelDir",
            // Legenda como parágrafo abaixo da tabela.
            "Tabela 1 \u{2014} Dados do local",
            // Grid de colunas (larguras) — docx-rs emite <w:gridCol w:w="..."/>.
            "<w:gridCol",
            // Alinhamento central da tabela.
            "<w:jc w:val=\"center\"",
            // colspan → gridSpan.
            "<w:gridSpan",
            // altura de linha → trHeight.
            "<w:trHeight",
        ],
    );
}

#[test]
fn renders_borderless_registration_block_without_caption() {
    // O bloco de registro (borderStyle "none", sem caption) NÃO deve gerar
    // legenda nem numeração — é tabela de layout.
    let envelope = json!({
        "title": "Bloco de registro",
        "content": {
            "type": "doc",
            "content": [
                {
                    "type": "table",
                    "attrs": { "id": "tbl-reg", "borderStyle": "none", "caption": "" },
                    "content": [
                        { "type": "tableRow", "content": [
                            { "type": "tableCell", "attrs": { "colspan": 1, "rowspan": 1, "colwidth": [300] }, "content": [
                                { "type": "paragraph", "content": [{ "type": "text", "text": "RegistradoEm" }] }
                            ]},
                        ]},
                    ]
                }
            ]
        }
    });

    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("reg-block.docx");
    render_doc_to_docx(&envelope, &path, None).expect("render_doc_to_docx ok");
    let xml = extract_document_xml(&path);

    assert!(xml.contains("RegistradoEm"), "missing cell text; xml:\n{xml}");
    // Sem rótulo "Tabela N" auto.
    assert!(
        !xml.contains("Tabela 1"),
        "registration block must not be numbered; xml:\n{xml}"
    );
}

#[test]
fn renders_empty_document_without_crashing() {
    // Reproduces the user's actual exported .sicrodoc: just the envelope
    // around a doc with a single empty paragraph. The walker should emit the
    // title + one empty paragraph and pack cleanly.
    let envelope = json!({
        "title": "Laudo vazio",
        "content": {
            "type": "doc",
            "content": [
                { "type": "paragraph" }
            ]
        }
    });

    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("empty.docx");
    render_doc_to_docx(&envelope, &path, None).expect("render_doc_to_docx ok");

    let xml = extract_document_xml(&path);
    assert!(xml.contains("Laudo vazio"), "missing title; xml:\n{xml}");
}

#[test]
fn renders_mvp2_quesitos_and_signature() {
    let envelope = json!({
        "title": "Laudo MVP2 — Sinistro",
        "layout": {
            "page_size": "A4",
            "orientation": "portrait",
            "institutional_template": "pca_padrao_v1"
        },
        "metadata": { "numero_laudo": "12345/2026" },
        "content": {
            "type": "doc",
            "content": [
                { "type": "heading", "attrs": { "level": 2 }, "content": [
                    { "type": "text", "text": "5 – DOS QUESITOS" }
                ]},
                {
                    "type": "quesitoList",
                    "content": [
                        {
                            "type": "quesitoItem",
                            "content": [
                                { "type": "quesitoQuestion", "content": [
                                    { "type": "text", "text": "Houve sinistro?" }
                                ]},
                                { "type": "quesitoAnswer", "content": [
                                    { "type": "text", "text": "RespostaQuesito1" }
                                ]}
                            ]
                        },
                        {
                            "type": "quesitoItem",
                            "content": [
                                { "type": "quesitoQuestion", "content": [
                                    { "type": "text", "text": "Quais vestigios?" }
                                ]},
                                { "type": "quesitoAnswer", "content": [
                                    { "type": "text", "text": "RespostaQuesito2" }
                                ]}
                            ]
                        }
                    ]
                },
                {
                    "type": "signature",
                    "attrs": {
                        "city": "Macapa",
                        "uf": "AP",
                        "date": "2026-05-24",
                        "name": "Perito Andre",
                        "role": "Perito Criminal"
                    }
                }
            ]
        }
    });

    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("mvp2.docx");
    render_doc_to_docx(&envelope, &path, None).expect("render_doc_to_docx ok");

    let xml = extract_document_xml(&path);
    assert_contains_all(
        &xml,
        &[
            "Laudo MVP2",
            "5 – DOS QUESITOS",
            "Quesito 1:",
            "Houve sinistro?",
            "Resposta:",
            "RespostaQuesito1",
            "Quesito 2:",
            "Quais vestigios?",
            "RespostaQuesito2",
            "Macapa - AP, 24/05/2026.",
            "Perito Andre",
            "Perito Criminal",
        ],
    );

    // N — Header hardcoded REMOVIDO. O DOCX agora NÃO carrega header
    // institucional fixo. Esta asserção atua como regressão: garante
    // que ninguém reintroduza, por engano, o header antigo enquanto N11
    // não tiver implementado a injeção dinâmica de `envelope.header.content`.
    //
    // Quando N11 entrar em produção, esta seção será reescrita para:
    //   1. construir envelope com `header: { enabled: true, content: {...} }`
    //   2. assertar que `word/header*` existe E reflete o content dinâmico
    //   3. assertar que envelope SEM header NÃO produz `word/header*`
    let file = std::fs::File::open(&path).expect("docx exists");
    let mut archive = zip::ZipArchive::new(file).expect("zip");
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .collect();
    let has_header = names.iter().any(|n| n.starts_with("word/header"));
    let has_footer = names.iter().any(|n| n.starts_with("word/footer"));
    assert!(
        !has_header,
        "N — DOCX não deve mais carregar header hardcoded; entries: {names:?}"
    );
    assert!(has_footer, "DOCX should have a footer part; entries: {names:?}");
}

#[test]
fn renders_envelope_with_missing_optional_fields() {
    // Defensive case: an envelope that's missing `title` and where a paragraph
    // has no content array at all. Should still produce a valid DOCX with
    // a fallback title.
    let envelope = json!({
        "content": {
            "type": "doc",
            "content": [
                { "type": "paragraph", "content": [{ "type": "text", "text": "MinimalText" }] }
            ]
        }
    });
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("minimal.docx");
    render_doc_to_docx(&envelope, &path, None).expect("render_doc_to_docx ok");

    let xml = extract_document_xml(&path);
    assert!(
        xml.contains("MinimalText"),
        "missing inline text; xml:\n{xml}"
    );
}

#[test]
fn applies_page_margins_from_envelope() {
    // MVP 2 ajuste runtime 1.2: when the envelope carries
    // layout.page.margins, the DOCX must encode those values in twips
    // via <w:pgMar/>. The conversion is 1 cm = 567 twips.
    //   top=3cm    → 1701
    //   right=2cm  → 1134
    //   bottom=2.5cm → 1418
    //   left=3.5cm → 1985
    let envelope = json!({
        "title": "Margens",
        "layout": {
            "institutional_template": "pca_padrao_v1",
            "page": {
                "margins": {
                    "top": "3cm",
                    "right": "2cm",
                    "bottom": "2.5cm",
                    "left": "3.5cm"
                }
            }
        },
        "content": {
            "type": "doc",
            "content": [
                { "type": "paragraph", "content": [{ "type": "text", "text": "Conteudo" }] }
            ]
        }
    });
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("margens.docx");
    render_doc_to_docx(&envelope, &path, None).expect("render_doc_to_docx ok");

    let xml = extract_document_xml(&path);
    assert!(xml.contains("Conteudo"), "missing body text; xml:\n{xml}");
    assert!(
        xml.contains("w:top=\"1701\""),
        "expected w:top=\"1701\" in pgMar; xml:\n{xml}"
    );
    assert!(
        xml.contains("w:right=\"1134\""),
        "expected w:right=\"1134\" in pgMar; xml:\n{xml}"
    );
    assert!(
        xml.contains("w:bottom=\"1418\""),
        "expected w:bottom=\"1418\" in pgMar; xml:\n{xml}"
    );
    assert!(
        xml.contains("w:left=\"1985\""),
        "expected w:left=\"1985\" in pgMar; xml:\n{xml}"
    );
}

#[test]
fn falls_back_to_template_margins_when_envelope_has_no_override() {
    // No `layout.page.margins` in the envelope → DOCX should use the
    // institutional template (pca_padrao_v1) defaults:
    //   top=3cm    → 1701
    //   right=2cm  → 1134
    //   bottom=2.5cm → 1418
    //   left=3.5cm → 1985
    let envelope = json!({
        "title": "Defaults",
        "layout": { "institutional_template": "pca_padrao_v1" },
        "content": {
            "type": "doc",
            "content": [
                { "type": "paragraph", "content": [{ "type": "text", "text": "Sem override" }] }
            ]
        }
    });
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("defaults.docx");
    render_doc_to_docx(&envelope, &path, None).expect("render_doc_to_docx ok");

    let xml = extract_document_xml(&path);
    assert!(xml.contains("w:top=\"1701\""), "expected template top; xml:\n{xml}");
    assert!(xml.contains("w:right=\"1134\""), "expected template right; xml:\n{xml}");
    assert!(xml.contains("w:bottom=\"1418\""), "expected template bottom; xml:\n{xml}");
    assert!(xml.contains("w:left=\"1985\""), "expected template left; xml:\n{xml}");
}

/// Regressão do bug "brasão do cabeçalho vira placeholder de texto": o header
/// dinâmico agora EMBUTE figura (ou cai num placeholder ESPECÍFICO) e
/// renderiza tabela como `<w:tbl>` — antes a figura caía no fallback genérico
/// "[bloco \"figure\" sem suporte na exportação DOCX]", que aparecia no DOCX
/// E no "PDF (estilo Word)" (que converte o mesmo DOCX via LibreOffice).
#[test]
fn header_figure_and_table_do_not_emit_unsupported_placeholder() {
    let envelope = json!({
        "title": "Laudo com cabeçalho",
        "header": {
            "enabled": true,
            "content": {
                "type": "doc",
                "content": [
                    { "type": "paragraph", "content": [
                        { "type": "text", "text": "GOVERNO DO ESTADO DO AMAPÁ" }
                    ]},
                    { "type": "figure",
                      "attrs": { "kind": "image", "width": "13.4%",
                                 "relative_path": "laudos/x/evidencias/photos/brasao.png" },
                      "content": [{ "type": "figcaption", "content": [] }] },
                    { "type": "table", "attrs": { "borderStyle": "all" }, "content": [
                        { "type": "tableRow", "content": [
                            { "type": "tableCell", "content": [
                                { "type": "paragraph", "content": [
                                    { "type": "text", "text": "Laudo nº 123" }
                                ]}
                            ]}
                        ]}
                    ]}
                ]
            }
        },
        "content": {
            "type": "doc",
            "content": [
                { "type": "paragraph", "content": [{ "type": "text", "text": "Corpo do laudo." }] }
            ]
        }
    });
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("com_header.docx");
    // workspace None → a imagem não resolve, mas DEVE cair no placeholder
    // específico do cabeçalho, nunca no genérico "sem suporte".
    render_doc_to_docx(&envelope, &path, None).expect("render_doc_to_docx ok");

    let file = std::fs::File::open(&path).expect("docx exists");
    let mut archive = zip::ZipArchive::new(file).expect("zip");
    let header_name = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .find(|n| n.starts_with("word/header"))
        .expect("DOCX deve ter uma parte word/header*");
    let mut header_xml = String::new();
    archive
        .by_name(&header_name)
        .unwrap()
        .read_to_string(&mut header_xml)
        .expect("header xml utf-8");

    assert!(
        !header_xml.contains("sem suporte na exporta"),
        "header não deve mais emitir o placeholder genérico; xml:\n{header_xml}"
    );
    assert!(
        header_xml.contains("indispon"),
        "figura sem workspace deve cair no placeholder do cabeçalho; xml:\n{header_xml}"
    );
    assert!(
        header_xml.contains("<w:tbl") && header_xml.contains("Laudo n"),
        "tabela do cabeçalho deve sair como <w:tbl> com o texto; xml:\n{header_xml}"
    );
    assert!(
        header_xml.contains("GOVERNO DO ESTADO"),
        "texto do cabeçalho preservado; xml:\n{header_xml}"
    );
}

/// A faixa lateral (text_box rotacionado do cabeçalho) sai como textbox VML
/// VERTICAL na margem — não como texto horizontal no fluxo do cabeçalho.
#[test]
fn header_text_box_becomes_vertical_vml_textbox() {
    let envelope = json!({
        "title": "Laudo",
        "header": { "enabled": true, "content": { "type": "doc", "content": [
            { "type": "paragraph", "content": [{ "type": "text", "text": "GOVERNO" }] },
            { "type": "text_box",
              "attrs": { "width_cm": 24.04, "height_cm": 2, "rotation": -90 },
              "content": [ { "type": "paragraph", "content": [
                  { "type": "text", "text": "POLÍCIA CIENTÍFICA DO ESTADO DO AMAPÁ",
                    "marks": [{ "type": "italic" }] }
              ]} ] }
        ]}},
        "content": { "type": "doc", "content": [
            { "type": "paragraph", "content": [{ "type": "text", "text": "Corpo." }] }
        ]}
    });
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("lateral.docx");
    render_doc_to_docx(&envelope, &path, None).expect("render_doc_to_docx ok");

    let file = std::fs::File::open(&path).expect("docx exists");
    let mut archive = zip::ZipArchive::new(file).expect("docx é zip válido");
    let header_name = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .find(|n| n.starts_with("word/header") && n.ends_with(".xml"))
        .expect("parte word/header*");
    let mut xml = String::new();
    archive
        .by_name(&header_name)
        .unwrap()
        .read_to_string(&mut xml)
        .expect("header xml utf-8");

    assert!(xml.contains("<v:textbox"), "deveria ter textbox VML; xml:\n{xml}");
    assert!(
        xml.contains("layout-flow:vertical"),
        "texto deveria ser vertical; xml:\n{xml}"
    );
    assert!(
        xml.contains("bottom-to-top"),
        "rotação -90 → leitura de baixo p/ cima; xml:\n{xml}"
    );
    assert!(
        xml.contains("POLÍCIA CIENTÍFICA DO ESTADO DO AMAP"),
        "texto da faixa presente; xml:\n{xml}"
    );
    assert!(xml.contains("<w:i/>"), "itálico preservado; xml:\n{xml}");
}
