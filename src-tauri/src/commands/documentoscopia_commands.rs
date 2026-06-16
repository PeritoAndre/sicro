//! Comandos Tauri do módulo Documentoscopia.
//!
//! Determinístico e com cadeia de custódia: importação copia o arquivo para o
//! workspace e gera SHA-256 (o ORIGINAL nunca é alterado); cada operação
//! registra log + auditoria. O OCR/extração é abstraído por [`crate::ocr`]
//! (mock agora; sidecar real depois) e a UI nunca escolhe um motor concreto.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use chrono::Utc;
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::{documentoscopia_repo as repo, occurrence_repo};
use crate::error::{Result, SicroError};
use crate::hashing::sha256::sha256_file;
use crate::models::{
    ComparisonSession, DetectedField, DocumentCaseFile, DocumentLog, DocumentRegion, OcrRun,
    OcrTextBlock,
};
use crate::ocr::{engine_from_models_dir, OcrBlock};
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};

const DOC_SUBDIR: &str = "documentos";
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "tif", "tiff", "bmp", "webp"];

// ---------------------------------------------------------------------------
// Tipos de entrada/saída (snake_case — espelham o TS)

#[derive(Debug, Deserialize)]
pub struct TextBlockInput {
    pub page_number: i64,
    pub text: String,
    pub confidence: Option<f64>,
    pub bbox_x: f64,
    pub bbox_y: f64,
    pub bbox_w: f64,
    pub bbox_h: f64,
    #[serde(default)]
    pub block_type: String,
    #[serde(default)]
    pub reading_order: i64,
}

#[derive(Debug, Deserialize)]
pub struct OcrRunInput {
    pub page_number: Option<i64>,
    pub engine: String,
    #[serde(default)]
    pub engine_version: String,
    #[serde(default)]
    pub language: String,
    pub mode: String,
    #[serde(default)]
    pub parameters_json: String,
    pub blocks: Vec<TextBlockInput>,
}

#[derive(Debug, Deserialize)]
pub struct FieldInput {
    pub page_number: Option<i64>,
    pub field_name: String,
    pub field_value: String,
    pub confidence: Option<f64>,
    #[serde(default)]
    pub source: String,
    pub bbox_x: Option<f64>,
    pub bbox_y: Option<f64>,
    pub bbox_w: Option<f64>,
    pub bbox_h: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct RegionInput {
    pub page_number: i64,
    pub region_type: String,
    pub bbox_x: f64,
    pub bbox_y: f64,
    pub bbox_w: f64,
    pub bbox_h: f64,
    #[serde(default)]
    pub label: String,
    pub confidence: Option<f64>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Serialize)]
pub struct OcrRunResult {
    pub run: OcrRun,
    pub blocks: Vec<OcrTextBlock>,
}

// ---------------------------------------------------------------------------
// Importação

/// Importa um documento (PDF ou imagem) preservando o original: copia para o
/// workspace, gera hash, lê metadados básicos e persiste. Dedup por SHA-256.
#[tauri::command]
pub async fn import_document(
    workspace_path: String,
    file_path: String,
    doc_type: Option<String>,
    title: Option<String>,
) -> Result<DocumentCaseFile> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let src = PathBuf::from(&file_path);
    if !src.is_file() {
        return Err(SicroError::Filesystem(format!(
            "arquivo não encontrado: {}",
            src.display()
        )));
    }
    let original_filename = src
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("documento")
        .to_string();
    let extension = src
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    let file_type = if extension == "pdf" {
        "pdf"
    } else if IMAGE_EXTS.contains(&extension.as_str()) {
        "image"
    } else {
        return Err(SicroError::Validation(format!(
            "tipo de arquivo não suportado: .{extension} (use PDF ou imagem PNG/JPG/TIFF/BMP/WebP)."
        )));
    };

    let sha256 = sha256_file(&src)?;
    if let Some(existing) = repo::find_document_by_sha(&conn, &occurrence_id, &sha256)? {
        return Err(SicroError::Validation(format!(
            "este documento (conteúdo idêntico) já foi importado nesta ocorrência \
             como “{}”.",
            existing.title
        )));
    }

    let dir = ws.join(DOC_SUBDIR);
    create_dir(&dir)?;
    let stored_name = unique_name(&dir, &original_filename);
    let dest = dir.join(&stored_name);
    std::fs::copy(&src, &dest)
        .map_err(|e| SicroError::Filesystem(format!("falha ao copiar documento: {e}")))?;

    // Metadados básicos. Para imagens, dimensões via crate `image`. Para PDF, o
    // page_count/text-layer/metadados vêm depois do frontend (pdf.js).
    let (page_count, metadata_json) = if file_type == "image" {
        let (w, h) = image::image_dimensions(&dest).unwrap_or((0, 0));
        (
            1,
            serde_json::json!({
                "width": w,
                "height": h,
                "format": extension,
            })
            .to_string(),
        )
    } else {
        (0, "{}".to_string())
    };

    let size_bytes = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    let now = Utc::now();
    let title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            Path::new(&original_filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&original_filename)
                .to_string()
        });
    let doc_type = doc_type
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "outro".to_string());

    let doc = DocumentCaseFile {
        id: Uuid::new_v4(),
        occurrence_id,
        title,
        original_filename,
        relative_path: format!("{DOC_SUBDIR}/{stored_name}"),
        file_type: file_type.to_string(),
        extension,
        doc_type,
        sha256: sha256.clone(),
        size_bytes,
        page_count,
        has_text_layer: false,
        status: "importado".to_string(),
        metadata_json,
        notes: String::new(),
        imported_by: None,
        created_at: now,
        updated_at: now,
    };
    repo::insert_document(&conn, &doc)?;
    log_doc(
        &conn,
        &occurrence_id,
        Some(&doc.id),
        "documento.importado",
        &serde_json::json!({
            "filename": doc.original_filename,
            "file_type": doc.file_type,
            "size_bytes": doc.size_bytes,
        })
        .to_string(),
        "importado",
        None,
        Some(&sha256),
    )?;
    Ok(doc)
}

#[tauri::command]
pub async fn list_documents(workspace_path: String) -> Result<Vec<DocumentCaseFile>> {
    let (conn, occ) = open(&workspace_path)?;
    repo::list_documents(&conn, &occ)
}

#[tauri::command]
pub async fn get_document(
    workspace_path: String,
    document_id: String,
) -> Result<DocumentCaseFile> {
    let (conn, _occ) = open(&workspace_path)?;
    let id = parse_id(&document_id)?;
    repo::find_document_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation("documento não encontrado".into()))
}

#[tauri::command]
pub async fn delete_document(workspace_path: String, document_id: String) -> Result<()> {
    let ws = PathBuf::from(&workspace_path);
    let (conn, occ) = open(&workspace_path)?;
    let id = parse_id(&document_id)?;
    if let Some(doc) = repo::find_document_by_id(&conn, &id)? {
        let abs = ws.join(&doc.relative_path);
        let _ = std::fs::remove_file(&abs);
        repo::delete_document(&conn, &id)?;
        log_doc(
            &conn,
            &occ,
            None,
            "documento.removido",
            &serde_json::json!({ "document_id": id.to_string(), "title": doc.title }).to_string(),
            "removido",
            Some(&doc.sha256),
            None,
        )?;
    }
    Ok(())
}

#[tauri::command]
pub async fn update_document_meta(
    workspace_path: String,
    document_id: String,
    title: String,
    doc_type: String,
    notes: String,
) -> Result<DocumentCaseFile> {
    let (conn, _occ) = open(&workspace_path)?;
    let id = parse_id(&document_id)?;
    let now = Utc::now();
    repo::update_document_meta(&conn, &id, title.trim(), doc_type.trim(), &notes, &now)?;
    repo::find_document_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation("documento não encontrado".into()))
}

/// Atualiza page_count / camada textual / metadados após o frontend (pdf.js)
/// inspecionar o PDF.
#[tauri::command]
pub async fn set_document_pageinfo(
    workspace_path: String,
    document_id: String,
    page_count: i64,
    has_text_layer: bool,
    metadata_json: String,
) -> Result<DocumentCaseFile> {
    let (conn, _occ) = open(&workspace_path)?;
    let id = parse_id(&document_id)?;
    let now = Utc::now();
    let meta = if metadata_json.trim().is_empty() {
        "{}".to_string()
    } else {
        metadata_json
    };
    repo::update_document_pageinfo(&conn, &id, page_count.max(0), has_text_layer, &meta, &now)?;
    repo::find_document_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation("documento não encontrado".into()))
}

// ---------------------------------------------------------------------------
// OCR / extração de texto

/// Persiste uma execução de OCR/extração (qualquer motor, inclusive a camada
/// textual de PDF lida no frontend). Marca o documento como `ocr_concluido`.
#[tauri::command]
pub async fn save_ocr_run(
    workspace_path: String,
    document_id: String,
    run: OcrRunInput,
) -> Result<OcrRunResult> {
    let (conn, occ) = open(&workspace_path)?;
    let doc_id = parse_id(&document_id)?;
    let blocks: Vec<NewBlock> = run
        .blocks
        .into_iter()
        .enumerate()
        .map(|(i, b)| NewBlock {
            page_number: b.page_number,
            text: b.text,
            confidence: b.confidence,
            bbox: [b.bbox_x, b.bbox_y, b.bbox_w, b.bbox_h],
            block_type: norm_block_type(&b.block_type),
            reading_order: if b.reading_order > 0 {
                b.reading_order
            } else {
                i as i64
            },
        })
        .collect();
    let language = if run.language.trim().is_empty() {
        "pt".to_string()
    } else {
        run.language
    };
    let params = if run.parameters_json.trim().is_empty() {
        "{}".to_string()
    } else {
        run.parameters_json
    };
    let result = persist_run(
        &conn,
        &doc_id,
        run.page_number,
        &run.engine,
        &run.engine_version,
        &language,
        &run.mode,
        &params,
        blocks,
    )?;
    let now = Utc::now();
    repo::update_document_status(&conn, &doc_id, "ocr_concluido", &now)?;
    log_doc(
        &conn,
        &occ,
        Some(&doc_id),
        "ocr.executado",
        &serde_json::json!({
            "engine": result.run.engine,
            "mode": result.run.mode,
            "blocks": result.run.block_count,
        })
        .to_string(),
        "ocr_concluido",
        None,
        None,
    )?;
    Ok(result)
}

/// Executa o motor de OCR sobre o documento e persiste o resultado. Usa o
/// **RapidOCR (PP-OCRv5)** quando o pacote de modelos foi baixado; senão, cai
/// no rascunho mock rotulado. O diretório de modelos é resolvido a partir do
/// `app` (não-roaming), o mesmo onde o gerenciador baixa o pacote.
#[tauri::command]
pub async fn run_ocr(
    app: tauri::AppHandle,
    workspace_path: String,
    document_id: String,
    page_number: Option<i64>,
    language: Option<String>,
) -> Result<OcrRunResult> {
    let ws = PathBuf::from(&workspace_path);
    let (conn, occ) = open(&workspace_path)?;
    let doc_id = parse_id(&document_id)?;
    let doc = repo::find_document_by_id(&conn, &doc_id)?
        .ok_or_else(|| SicroError::Validation("documento não encontrado".into()))?;

    // O motor de OCR lê IMAGEM (pixels). PDF precisa ser rasterizado (página →
    // imagem) antes — recurso ainda em desenvolvimento. Mensagem honesta em vez
    // do erro críptico de decodificação.
    if doc.file_type != "image" {
        return Err(SicroError::Validation(
            "O OCR direto de PDF ainda está em desenvolvimento (a página precisa \
             ser convertida em imagem antes). Para testar agora, importe a página \
             como imagem (PNG/JPG) — o OCR de imagem já funciona."
                .into(),
        ));
    }

    let lang = language.unwrap_or_else(|| "por".to_string());

    let models_dir = crate::commands::ocr_commands::ocr_models_dir(&app)?;
    let engine = engine_from_models_dir(&models_dir);
    let abs = ws.join(&doc.relative_path);
    let outcome = engine.recognize(&abs, &lang)?;

    let blocks: Vec<NewBlock> = outcome
        .blocks
        .into_iter()
        .map(|b: OcrBlock| NewBlock {
            page_number: page_number.unwrap_or(1),
            text: b.text,
            confidence: Some(b.confidence),
            bbox: b.bbox,
            block_type: norm_block_type(&b.block_type),
            reading_order: b.reading_order,
        })
        .collect();
    let mode = if page_number.is_some() {
        "page"
    } else {
        "full_document"
    };
    let result = persist_run(
        &conn,
        &doc_id,
        page_number,
        &outcome.engine,
        &outcome.engine_version,
        &lang,
        mode,
        "{}",
        blocks,
    )?;
    let now = Utc::now();
    repo::update_document_status(&conn, &doc_id, "ocr_concluido", &now)?;
    log_doc(
        &conn,
        &occ,
        Some(&doc_id),
        "ocr.executado",
        &serde_json::json!({ "engine": result.run.engine, "mode": mode }).to_string(),
        "ocr_concluido",
        None,
        None,
    )?;
    Ok(result)
}

/// Executa o OCR sobre uma **página de PDF já rasterizada no frontend** (pdf.js).
/// Recebe a imagem PNG em base64, grava num arquivo temporário, roda o RapidOCR
/// e persiste o resultado vinculado ao documento — mesma lógica do [`run_ocr`],
/// porém a fonte é a página renderizada (PDF escaneado) em vez do arquivo de
/// imagem importado. O `parameters_json` registra `source = "pdf_raster"` para
/// o perito saber como o texto foi obtido (§13). O temporário é sempre apagado.
#[tauri::command]
pub async fn run_ocr_page_image(
    app: tauri::AppHandle,
    workspace_path: String,
    document_id: String,
    page_number: i64,
    image_base64: String,
    language: Option<String>,
) -> Result<OcrRunResult> {
    let (conn, occ) = open(&workspace_path)?;
    let doc_id = parse_id(&document_id)?;
    repo::find_document_by_id(&conn, &doc_id)?
        .ok_or_else(|| SicroError::Validation("documento não encontrado".into()))?;

    let lang = language.unwrap_or_else(|| "por".to_string());

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(image_base64.trim())
        .map_err(|e| SicroError::Validation(format!("imagem inválida (base64): {e}")))?;
    if bytes.is_empty() {
        return Err(SicroError::Validation(
            "a página renderizada veio vazia".into(),
        ));
    }

    // Arquivo temporário (apagado ao final, inclusive em erro de OCR).
    let tmp = std::env::temp_dir().join(format!("sicro-ocr-{}.png", Uuid::new_v4()));
    std::fs::write(&tmp, &bytes).map_err(|e| {
        SicroError::Validation(format!("falha ao gravar imagem temporária: {e}"))
    })?;

    let models_dir = crate::commands::ocr_commands::ocr_models_dir(&app)?;
    let engine = engine_from_models_dir(&models_dir);
    let outcome = engine.recognize(&tmp, &lang);
    let _ = std::fs::remove_file(&tmp); // limpeza best-effort
    let outcome = outcome?;

    let blocks: Vec<NewBlock> = outcome
        .blocks
        .into_iter()
        .map(|b: OcrBlock| NewBlock {
            page_number,
            text: b.text,
            confidence: Some(b.confidence),
            bbox: b.bbox,
            block_type: norm_block_type(&b.block_type),
            reading_order: b.reading_order,
        })
        .collect();
    let result = persist_run(
        &conn,
        &doc_id,
        Some(page_number),
        &outcome.engine,
        &outcome.engine_version,
        &lang,
        "page",
        "{\"source\":\"pdf_raster\"}",
        blocks,
    )?;
    let now = Utc::now();
    repo::update_document_status(&conn, &doc_id, "ocr_concluido", &now)?;
    log_doc(
        &conn,
        &occ,
        Some(&doc_id),
        "ocr.executado",
        &serde_json::json!({
            "engine": result.run.engine,
            "mode": "page",
            "page": page_number,
            "source": "pdf_raster",
        })
        .to_string(),
        "ocr_concluido",
        None,
        None,
    )?;
    Ok(result)
}

#[tauri::command]
pub async fn list_ocr_runs(
    workspace_path: String,
    document_id: String,
) -> Result<Vec<OcrRun>> {
    let (conn, _occ) = open(&workspace_path)?;
    let id = parse_id(&document_id)?;
    repo::list_ocr_runs(&conn, &id)
}

#[tauri::command]
pub async fn get_run_blocks(
    workspace_path: String,
    run_id: String,
) -> Result<Vec<OcrTextBlock>> {
    let (conn, _occ) = open(&workspace_path)?;
    let id = parse_id(&run_id)?;
    repo::list_blocks_for_run(&conn, &id)
}

#[tauri::command]
pub async fn review_text_block(
    workspace_path: String,
    block_id: String,
    corrected_text: Option<String>,
    reviewed: bool,
) -> Result<()> {
    let (conn, _occ) = open(&workspace_path)?;
    let id = parse_id(&block_id)?;
    repo::set_block_review(&conn, &id, corrected_text.as_deref(), reviewed)
}

/// Adiciona um bloco de texto **manual** (criado pelo perito) onde o OCR não
/// detectou nada. O bloco entra na execução mais recente; se ainda não houver
/// nenhuma, cria uma execução "manual" para hospedá-lo. `bbox` normalizado
/// (0..1). Marcado como `reviewed = true` e `block_type = "manual"`.
#[tauri::command]
pub async fn add_manual_block(
    workspace_path: String,
    document_id: String,
    page_number: i64,
    text: String,
    bbox_x: f64,
    bbox_y: f64,
    bbox_w: f64,
    bbox_h: f64,
) -> Result<OcrTextBlock> {
    let (conn, occ) = open(&workspace_path)?;
    let doc_id = parse_id(&document_id)?;
    let now = Utc::now();

    // Pendura na execução mais recente; se não houver, cria uma "manual".
    let run_id = match repo::list_ocr_runs(&conn, &doc_id)?.first() {
        Some(r) => r.id,
        None => {
            let run = OcrRun {
                id: Uuid::new_v4(),
                document_id: doc_id,
                page_number: Some(page_number),
                engine: "manual".to_string(),
                engine_version: String::new(),
                language: "pt".to_string(),
                mode: "manual".to_string(),
                status: "concluido".to_string(),
                avg_confidence: None,
                block_count: 0,
                parameters_json: "{}".to_string(),
                created_at: now,
            };
            repo::insert_ocr_run(&conn, &run)?;
            run.id
        }
    };

    let block = OcrTextBlock {
        id: Uuid::new_v4(),
        ocr_run_id: run_id,
        document_id: doc_id,
        page_number,
        text,
        confidence: None,
        bbox_x: bbox_x.clamp(0.0, 1.0),
        bbox_y: bbox_y.clamp(0.0, 1.0),
        bbox_w: bbox_w.clamp(0.0, 1.0),
        bbox_h: bbox_h.clamp(0.0, 1.0),
        block_type: "manual".to_string(),
        // ordena por posição vertical e depois dos blocos do OCR
        reading_order: (bbox_y.clamp(0.0, 1.0) * 100_000.0) as i64,
        corrected_text: None,
        reviewed: true,
        created_at: now,
    };
    repo::insert_text_block(&conn, &block)?;
    log_doc(
        &conn,
        &occ,
        Some(&doc_id),
        "ocr.bloco_manual",
        &serde_json::json!({ "page": page_number, "chars": block.text.len() }).to_string(),
        "ok",
        None,
        None,
    )?;
    Ok(block)
}

#[tauri::command]
pub async fn delete_text_block(workspace_path: String, block_id: String) -> Result<()> {
    let (conn, _occ) = open(&workspace_path)?;
    let id = parse_id(&block_id)?;
    repo::delete_text_block(&conn, &id)
}

/// Atualiza a posição/tamanho (bbox normalizado 0..1) de um bloco — usado quando
/// o perito move/redimensiona a caixa sobre o documento para alinhar melhor a
/// camada de texto do PDF pesquisável.
#[tauri::command]
pub async fn set_block_bbox(
    workspace_path: String,
    block_id: String,
    bbox_x: f64,
    bbox_y: f64,
    bbox_w: f64,
    bbox_h: f64,
) -> Result<()> {
    let (conn, _occ) = open(&workspace_path)?;
    let id = parse_id(&block_id)?;
    repo::set_block_bbox(
        &conn,
        &id,
        bbox_x.clamp(0.0, 1.0),
        bbox_y.clamp(0.0, 1.0),
        bbox_w.clamp(0.0, 1.0),
        bbox_h.clamp(0.0, 1.0),
    )
}

// ---------------------------------------------------------------------------
// Exportação: PDF pesquisável (imagem + camada de texto invisível)

#[derive(Debug, Deserialize)]
pub struct SearchPdfBlock {
    pub text: String,
    pub bbox_x: f64,
    pub bbox_y: f64,
    pub bbox_w: f64,
    pub bbox_h: f64,
}

#[derive(Debug, Deserialize)]
pub struct SearchPdfPage {
    /// Data URL (`data:image/...`) ou base64 puro da imagem da página.
    pub image_base64: String,
    pub width: f64,
    pub height: f64,
    pub blocks: Vec<SearchPdfBlock>,
}

fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Gera um **PDF pesquisável**: cada página recebe a imagem (intacta) e, por
/// cima, o texto dos blocos em **tinta invisível** posicionado pelas bboxes —
/// o documento continua visualmente idêntico, mas o texto fica selecionável e
/// pesquisável. Usa o mesmo pipeline Edge headless do laudo (offline). O texto
/// vem da camada já curada pelo perito (OCR + correções + blocos manuais).
#[tauri::command]
pub async fn export_searchable_pdf(
    app: tauri::AppHandle,
    workspace_path: String,
    document_id: String,
    pages: Vec<SearchPdfPage>,
) -> Result<String> {
    let _ = app;
    let ws = PathBuf::from(&workspace_path);
    let (conn, occ) = open(&workspace_path)?;
    let doc_id = parse_id(&document_id)?;
    repo::find_document_by_id(&conn, &doc_id)?
        .ok_or_else(|| SicroError::Validation("documento não encontrado".into()))?;
    if pages.is_empty() {
        return Err(SicroError::Validation(
            "nada para exportar (nenhuma página)".into(),
        ));
    }

    // @page assume o tamanho da 1ª página (documentos costumam ser uniformes).
    let pw = pages[0].width.max(1.0);
    let ph = pages[0].height.max(1.0);

    let mut body = String::new();
    for pg in &pages {
        let w = pg.width.max(1.0);
        let h = pg.height.max(1.0);
        let src = if pg.image_base64.starts_with("data:") {
            pg.image_base64.clone()
        } else {
            format!("data:image/png;base64,{}", pg.image_base64)
        };
        body.push_str(&format!(
            "<div class=\"pg\" style=\"width:{w:.0}px;height:{h:.0}px\">\
             <img src=\"{src}\" style=\"width:{w:.0}px;height:{h:.0}px\"/>"
        ));
        for b in &pg.blocks {
            let t = b.text.trim();
            if t.is_empty() {
                continue;
            }
            // Posição/tamanho em px (a página tem exatamente w×h px).
            let left = b.bbox_x * w;
            let top = b.bbox_y * h;
            let bw_px = (b.bbox_w * w).max(1.0);
            let fs = (b.bbox_h * h).max(4.0);
            // Largura natural estimada do texto em Helvetica (~0.55·fonte por
            // caractere). Escalamos em X para o texto caber EXATAMENTE na
            // largura da caixa — assim nada é cortado e cada linha ocupa só a
            // sua região (sem sobrepor a de baixo).
            let n = t.chars().count().max(1) as f64;
            let est = (0.55 * fs * n).max(1.0);
            let sx = (bw_px / est).clamp(0.05, 20.0);
            body.push_str(&format!(
                "<span class=\"t\" style=\"left:{left:.2}px;top:{top:.2}px;\
                 font-size:{fs:.2}px;transform:scaleX({sx:.4})\">{}</span>",
                esc_html(t)
            ));
        }
        body.push_str("</div>");
    }

    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>\
         @page{{margin:0;size:{pw:.0}px {ph:.0}px}}\
         html,body{{margin:0;padding:0}}\
         .pg{{position:relative;page-break-after:always;overflow:hidden}}\
         .pg>img{{position:absolute;top:0;left:0;display:block}}\
         .t{{position:absolute;color:transparent;white-space:pre;line-height:1;\
         transform-origin:left top;font-family:Helvetica,Arial,sans-serif}}\
         </style></head><body>{body}</body></html>"
    );

    let cache_dir = ws.join("cache");
    std::fs::create_dir_all(&cache_dir).ok();
    let rel = format!("{DOC_SUBDIR}/{doc_id}-pesquisavel.pdf");
    let out = ws.join(&rel);
    crate::exporters::pdf::render_html_to_pdf(&html, &cache_dir, &out, None)?;

    log_doc(
        &conn,
        &occ,
        Some(&doc_id),
        "ocr.pdf_pesquisavel",
        &serde_json::json!({ "pages": pages.len() }).to_string(),
        "ok",
        None,
        None,
    )?;
    Ok(rel)
}

// ---------------------------------------------------------------------------
// Fase 4 — Pré-processamento de imagem; Fase 3 — Detecção de layout

fn decode_b64_image(s: &str) -> Result<RgbaImage> {
    let raw = s.rsplit(',').next().unwrap_or(s).trim();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|e| SicroError::Validation(format!("imagem inválida (base64): {e}")))?;
    let img = image::load_from_memory(&bytes)
        .map_err(|e| SicroError::Validation(format!("falha ao decodificar imagem: {e}")))?;
    Ok(img.to_rgba8())
}

fn encode_b64_png(img: &RgbaImage) -> Result<String> {
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| SicroError::Validation(format!("falha ao codificar PNG: {e}")))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(buf.into_inner()))
}

/// Aplica um pipeline de pré-processamento (ids: "cinza","endireitar","clahe",
/// "niveis","otsu","inverter") e devolve a imagem processada em base64 (PNG, sem
/// prefixo data:). Função pura — não altera o original nem o banco (§13).
#[tauri::command]
pub async fn preprocess_image(image_base64: String, ops: Vec<String>) -> Result<String> {
    let img = decode_b64_image(&image_base64)?;
    let parsed = crate::docanalysis::parse_ops(&ops);
    let out = crate::docanalysis::apply_ops(&img, &parsed);
    encode_b64_png(&out)
}

/// Corrige perspectiva a partir de 4 cantos normalizados (0..1), em ordem
/// horária a partir do superior-esquerdo. Devolve a imagem retificada (base64
/// PNG). Função pura.
#[tauri::command]
pub async fn perspective_image(
    image_base64: String,
    points: Vec<[f64; 2]>,
) -> Result<String> {
    if points.len() != 4 {
        return Err(SicroError::Validation(
            "a correção de perspectiva exige exatamente 4 pontos".into(),
        ));
    }
    let img = decode_b64_image(&image_base64)?;
    let w = img.width() as f32;
    let h = img.height() as f32;
    let src: [[f32; 2]; 4] = [
        [points[0][0] as f32 * w, points[0][1] as f32 * h],
        [points[1][0] as f32 * w, points[1][1] as f32 * h],
        [points[2][0] as f32 * w, points[2][1] as f32 * h],
        [points[3][0] as f32 * w, points[3][1] as f32 * h],
    ];
    let dist =
        |a: [f32; 2], b: [f32; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    let ow = (((dist(src[0], src[1]) + dist(src[3], src[2])) / 2.0).round()).max(8.0) as u32;
    let oh = (((dist(src[0], src[3]) + dist(src[1], src[2])) / 2.0).round()).max(8.0) as u32;
    let dst: [[f32; 2]; 4] = [
        [0.0, 0.0],
        [ow as f32, 0.0],
        [ow as f32, oh as f32],
        [0.0, oh as f32],
    ];
    let out =
        crate::image_editor::filters::geometric::perspective_correct(&img, &src, &dst, ow, oh);
    encode_b64_png(&out)
}

/// Detecta elementos VERIFICÁVEIS na imagem da página (QR/código de barras
/// decodificados; candidato a tabela por linhas) e **persiste** como regiões.
/// Determinístico; nada conclui (§13).
#[tauri::command]
pub async fn detect_layout(
    workspace_path: String,
    document_id: String,
    page_number: i64,
    image_base64: String,
) -> Result<Vec<DocumentRegion>> {
    let (conn, occ) = open(&workspace_path)?;
    let doc_id = parse_id(&document_id)?;
    let img = decode_b64_image(&image_base64)?;
    let found = crate::docanalysis::detect_layout(&img);
    let now = Utc::now();
    let mut saved = Vec::with_capacity(found.len());
    for d in found {
        let r = DocumentRegion {
            id: Uuid::new_v4(),
            document_id: doc_id,
            page_number,
            region_type: d.region_type,
            bbox_x: d.bbox[0],
            bbox_y: d.bbox[1],
            bbox_w: d.bbox[2],
            bbox_h: d.bbox[3],
            label: d.label,
            confidence: d.confidence,
            notes: "detecção automática".to_string(),
            created_at: now,
        };
        repo::insert_region(&conn, &r)?;
        saved.push(r);
    }
    log_doc(
        &conn,
        &occ,
        Some(&doc_id),
        "layout.detectado",
        &serde_json::json!({ "page": page_number, "regioes": saved.len() }).to_string(),
        "ok",
        None,
        None,
    )?;
    Ok(saved)
}

#[derive(Debug, Serialize)]
pub struct DecodedCode {
    pub region_type: String,
    pub label: String,
}

/// Tenta **decodificar** um QR/código de barras dentro de uma região (bbox
/// 0..1): isola o recorte e amplia, decodificando fotos/escaneados que a
/// detecção na página inteira não pega. Determinístico; não toca no original.
#[tauri::command]
pub async fn decode_region(
    image_base64: String,
    bbox_x: f64,
    bbox_y: f64,
    bbox_w: f64,
    bbox_h: f64,
) -> Result<Option<DecodedCode>> {
    let img = decode_b64_image(&image_base64)?;
    let w = img.width();
    let h = img.height();
    if w == 0 || h == 0 {
        return Ok(None);
    }
    // Recorte com pequena folga (4% de cada lado), limitado à imagem.
    let pad_x = (bbox_w * 0.04 * w as f64).max(2.0);
    let pad_y = (bbox_h * 0.04 * h as f64).max(2.0);
    let x0 = (bbox_x * w as f64 - pad_x).max(0.0) as u32;
    let y0 = (bbox_y * h as f64 - pad_y).max(0.0) as u32;
    let x1 = (((bbox_x + bbox_w) * w as f64) + pad_x).min(w as f64) as u32;
    let y1 = (((bbox_y + bbox_h) * h as f64) + pad_y).min(h as f64) as u32;
    let cw = x1.saturating_sub(x0).max(1);
    let ch = y1.saturating_sub(y0).max(1);
    let crop = image::imageops::crop_imm(&img, x0, y0, cw, ch).to_image();
    Ok(
        crate::docanalysis::decode_crop(&crop).map(|(region_type, label)| DecodedCode {
            region_type,
            label,
        }),
    )
}

// ---------------------------------------------------------------------------
// Campos detectados

/// Salva campos detectados. `replace_source` (ex.: "heuristica") apaga apenas os
/// campos daquela origem antes de inserir — preserva os campos manuais.
#[tauri::command]
pub async fn save_fields(
    workspace_path: String,
    document_id: String,
    fields: Vec<FieldInput>,
    replace_source: Option<String>,
) -> Result<Vec<DetectedField>> {
    let (conn, _occ) = open(&workspace_path)?;
    let doc_id = parse_id(&document_id)?;
    if let Some(src) = replace_source.as_deref() {
        repo::delete_fields_by_source(&conn, &doc_id, src)?;
    }
    let now = Utc::now();
    for f in fields {
        let field = DetectedField {
            id: Uuid::new_v4(),
            document_id: doc_id,
            page_number: f.page_number,
            field_name: f.field_name,
            field_value: f.field_value,
            confidence: f.confidence,
            source: if f.source.trim().is_empty() {
                "heuristica".to_string()
            } else {
                f.source
            },
            bbox_x: f.bbox_x,
            bbox_y: f.bbox_y,
            bbox_w: f.bbox_w,
            bbox_h: f.bbox_h,
            reviewed: false,
            corrected_value: None,
            created_at: now,
        };
        repo::insert_field(&conn, &field)?;
    }
    repo::list_fields(&conn, &doc_id)
}

#[tauri::command]
pub async fn list_fields(
    workspace_path: String,
    document_id: String,
) -> Result<Vec<DetectedField>> {
    let (conn, _occ) = open(&workspace_path)?;
    let id = parse_id(&document_id)?;
    repo::list_fields(&conn, &id)
}

#[tauri::command]
pub async fn review_field(
    workspace_path: String,
    field_id: String,
    corrected_value: Option<String>,
    reviewed: bool,
) -> Result<()> {
    let (conn, _occ) = open(&workspace_path)?;
    let id = parse_id(&field_id)?;
    repo::set_field_review(&conn, &id, corrected_value.as_deref(), reviewed)
}

// ---------------------------------------------------------------------------
// Regiões

#[tauri::command]
pub async fn save_region(
    workspace_path: String,
    document_id: String,
    region: RegionInput,
) -> Result<DocumentRegion> {
    let (conn, _occ) = open(&workspace_path)?;
    let doc_id = parse_id(&document_id)?;
    let r = DocumentRegion {
        id: Uuid::new_v4(),
        document_id: doc_id,
        page_number: region.page_number,
        region_type: region.region_type,
        bbox_x: region.bbox_x,
        bbox_y: region.bbox_y,
        bbox_w: region.bbox_w,
        bbox_h: region.bbox_h,
        label: region.label,
        confidence: region.confidence,
        notes: region.notes,
        created_at: Utc::now(),
    };
    repo::insert_region(&conn, &r)?;
    Ok(r)
}

#[tauri::command]
pub async fn list_regions(
    workspace_path: String,
    document_id: String,
) -> Result<Vec<DocumentRegion>> {
    let (conn, _occ) = open(&workspace_path)?;
    let id = parse_id(&document_id)?;
    repo::list_regions(&conn, &id)
}

#[tauri::command]
pub async fn delete_region(workspace_path: String, region_id: String) -> Result<()> {
    let (conn, _occ) = open(&workspace_path)?;
    let id = parse_id(&region_id)?;
    repo::delete_region(&conn, &id)
}

// ---------------------------------------------------------------------------
// Comparações + histórico

#[tauri::command]
pub async fn save_comparison(
    workspace_path: String,
    questioned_document_id: String,
    reference_document_id: String,
    comparison_type: String,
    results_json: String,
    summary: String,
) -> Result<ComparisonSession> {
    let (conn, occ) = open(&workspace_path)?;
    let c = ComparisonSession {
        id: Uuid::new_v4(),
        occurrence_id: occ,
        questioned_document_id: parse_id(&questioned_document_id)?,
        reference_document_id: parse_id(&reference_document_id)?,
        comparison_type,
        results_json: if results_json.trim().is_empty() {
            "{}".to_string()
        } else {
            results_json
        },
        summary,
        created_at: Utc::now(),
    };
    repo::insert_comparison(&conn, &c)?;
    Ok(c)
}

#[tauri::command]
pub async fn list_comparisons(workspace_path: String) -> Result<Vec<ComparisonSession>> {
    let (conn, occ) = open(&workspace_path)?;
    repo::list_comparisons(&conn, &occ)
}

const CONFRONTO_SUBDIR: &str = "documentoscopia/confrontos";

/// Salva a imagem composta de um confronto (PNG em base64, gerada no frontend)
/// dentro do workspace, em `documentoscopia/confrontos/`. Retorna o caminho
/// relativo (para revelar na pasta ou anexar ao laudo). É um DERIVADO — os
/// documentos originais nunca são alterados (§13).
#[tauri::command]
pub async fn save_confronto_image(
    workspace_path: String,
    png_base64: String,
    file_name: String,
) -> Result<String> {
    let ws = PathBuf::from(&workspace_path);
    let raw = png_base64
        .split_once(',')
        .map(|(_, b)| b)
        .unwrap_or(&png_base64);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .map_err(|e| SicroError::Validation(format!("imagem inválida (base64): {e}")))?;
    if bytes.is_empty() {
        return Err(SicroError::Validation("imagem vazia".into()));
    }
    let dir = ws.join(CONFRONTO_SUBDIR);
    create_dir(&dir)?;
    let safe: String = file_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = if safe.is_empty() {
        "confronto.png".to_string()
    } else {
        safe
    };
    let dest = dir.join(&name);
    std::fs::write(&dest, &bytes)
        .map_err(|e| SicroError::Filesystem(format!("falha ao salvar confronto: {e}")))?;
    Ok(format!("{CONFRONTO_SUBDIR}/{name}"))
}

/// ELA (Error Level Analysis) sobre uma imagem (base64). Retorna o heatmap
/// PNG em base64. §13: indício, não conclusão — falsos-positivos em
/// bordas/alto contraste e em regiões já recomprimidas.
#[tauri::command]
pub async fn doc_ela(
    image_base64: String,
    quality: Option<u8>,
    gain: Option<u32>,
) -> Result<String> {
    let img = decode_b64_image(&image_base64)?;
    let out = crate::docforensics::ela(&img, quality.unwrap_or(90), gain.unwrap_or(15));
    encode_b64_png(&out)
}

/// Mapa de ruído (energia local de alta frequência) sobre uma imagem (base64).
/// Retorna PNG em base64. §13: indício, não conclusão — o ruído varia
/// naturalmente com iluminação, foco e compressão.
#[tauri::command]
pub async fn doc_noise_map(image_base64: String, window: Option<u32>) -> Result<String> {
    let img = decode_b64_image(&image_base64)?;
    let out = crate::docforensics::noise_map(&img, window.unwrap_or(4));
    encode_b64_png(&out)
}

/// Copy-move (regiões clonadas na mesma imagem). Heatmap PNG base64. §13:
/// indício, não conclusão — texturas repetitivas geram falso-positivo.
#[tauri::command]
pub async fn doc_copy_move(
    image_base64: String,
    block: Option<u32>,
    step: Option<u32>,
) -> Result<String> {
    let img = decode_b64_image(&image_base64)?;
    let b = block.unwrap_or(16);
    let out = crate::docforensics::copy_move(&img, b, step.unwrap_or(b / 2));
    encode_b64_png(&out)
}

/// Extrai o maior JPEG embutido (filtro DCTDecode) da página `page` (1-based)
/// de um PDF. O stream DCTDecode JÁ é um arquivo JPEG — devolvemos seus bytes
/// crus, preservando o histórico de compressão original (essencial p/ ELA).
/// `None` se a página não tiver imagem JPEG (ex.: texto vetorial).
fn pdf_page_jpeg(path: &Path, page: u32) -> Result<Option<Vec<u8>>> {
    use lopdf::{Document, Object};
    let doc = Document::load(path)
        .map_err(|e| SicroError::Validation(format!("PDF inválido: {e}")))?;
    let pages = doc.get_pages();
    let page_id = match pages.get(&page) {
        Some(id) => *id,
        None => return Ok(None),
    };
    let (res_opt, inherited) = doc
        .get_page_resources(page_id)
        .map_err(|e| SicroError::Validation(format!("recursos da página: {e}")))?;
    let mut resources: Vec<&lopdf::Dictionary> = Vec::new();
    if let Some(d) = res_opt {
        resources.push(d);
    }
    for rid in inherited {
        if let Ok(d) = doc.get_dictionary(rid) {
            resources.push(d);
        }
    }
    let mut best: Option<Vec<u8>> = None;
    for res in resources {
        let xobj = match res.get_deref(b"XObject", &doc).and_then(|o| o.as_dict()) {
            Ok(d) => d,
            Err(_) => continue,
        };
        for (_name, val) in xobj.iter() {
            let stream = match val {
                Object::Reference(rid) => match doc.get_object(*rid).and_then(|o| o.as_stream()) {
                    Ok(s) => s,
                    Err(_) => continue,
                },
                Object::Stream(s) => s,
                _ => continue,
            };
            let d = &stream.dict;
            let is_image = d
                .get(b"Subtype")
                .and_then(|o| o.as_name())
                .map(|n| n == b"Image")
                .unwrap_or(false);
            if !is_image {
                continue;
            }
            let is_jpeg = match d.get(b"Filter") {
                Ok(Object::Name(n)) => n == b"DCTDecode",
                Ok(Object::Array(arr)) => arr
                    .iter()
                    .any(|o| o.as_name().map(|n| n == b"DCTDecode").unwrap_or(false)),
                _ => false,
            };
            if !is_jpeg {
                continue;
            }
            if best.as_ref().map(|b| stream.content.len() > b.len()).unwrap_or(true) {
                best = Some(stream.content.clone());
            }
        }
    }
    Ok(best)
}

/// Comando: devolve o JPEG embutido da página do PDF em base64 (ou `None`).
#[tauri::command]
pub async fn extract_pdf_jpeg(
    workspace_path: String,
    relative_path: String,
    page: u32,
) -> Result<Option<String>> {
    let path = PathBuf::from(&workspace_path).join(&relative_path);
    let bytes = pdf_page_jpeg(&path, page)?;
    Ok(bytes.map(|b| base64::engine::general_purpose::STANDARD.encode(b)))
}

/// Gera uma amostra-teste de ELA (caso-controle positivo) no workspace e
/// devolve o caminho ABSOLUTO do .jpg, para o front importá-la como documento.
#[tauri::command]
pub async fn generate_ela_test_sample(workspace_path: String) -> Result<String> {
    let ws = PathBuf::from(&workspace_path);
    let dir = ws.join("documentoscopia/amostras");
    create_dir(&dir)?;
    let img = crate::docforensics::synth_ela_sample();
    let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 92);
        enc.encode_image(&rgb)
            .map_err(|e| SicroError::Filesystem(format!("falha ao codificar amostra: {e}")))?;
    }
    let dest = dir.join("amostra_ela_teste.jpg");
    std::fs::write(&dest, &buf)
        .map_err(|e| SicroError::Filesystem(format!("falha ao salvar amostra: {e}")))?;
    Ok(dest.to_string_lossy().to_string())
}

const INDICIO_SUBDIR: &str = "documentoscopia/indicios";

/// Um indício (heatmap ELA/ruído/copy-move) salvo na "bandeja" do workspace.
#[derive(Debug, Serialize)]
pub struct DocIndicio {
    pub relative_path: String,
    pub file_name: String,
    pub created_at: String,
}

fn sanitize_name(name: &str, fallback: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() {
        fallback.to_string()
    } else {
        safe
    }
}

/// Salva um heatmap de indício (PNG base64) na bandeja `documentoscopia/indicios`
/// do workspace. Retorna o caminho relativo — serve para exportar (revelar na
/// pasta) e para o laudo listar/inserir. É um DERIVADO; o original nunca muda.
#[tauri::command]
pub async fn save_doc_indicio(
    workspace_path: String,
    png_base64: String,
    file_name: String,
) -> Result<String> {
    let ws = PathBuf::from(&workspace_path);
    let raw = png_base64.split_once(',').map(|(_, b)| b).unwrap_or(&png_base64);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .map_err(|e| SicroError::Validation(format!("imagem inválida (base64): {e}")))?;
    if bytes.is_empty() {
        return Err(SicroError::Validation("imagem vazia".into()));
    }
    let dir = ws.join(INDICIO_SUBDIR);
    create_dir(&dir)?;
    let name = sanitize_name(&file_name, "indicio.png");
    let dest = dir.join(&name);
    std::fs::write(&dest, &bytes)
        .map_err(|e| SicroError::Filesystem(format!("falha ao salvar indício: {e}")))?;
    Ok(format!("{INDICIO_SUBDIR}/{name}"))
}

/// Lista os indícios salvos na bandeja (para a aba Evidências do laudo).
#[tauri::command]
pub async fn list_doc_indicios(workspace_path: String) -> Result<Vec<DocIndicio>> {
    let dir = PathBuf::from(&workspace_path).join(INDICIO_SUBDIR);
    let mut out: Vec<DocIndicio> = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(out), // pasta ainda não existe
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_img = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg"))
            .unwrap_or(false);
        if !is_img {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_string();
        let created_at = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| chrono::DateTime::<Utc>::from(t).to_rfc3339())
            .unwrap_or_default();
        out.push(DocIndicio {
            relative_path: format!("{INDICIO_SUBDIR}/{file_name}"),
            file_name,
            created_at,
        });
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct DocReportArtifact {
    pub html_relative_path: String,
    pub pdf_relative_path: Option<String>,
}

fn fmt_bytes(n: u64) -> String {
    let n = n as f64;
    if n < 1024.0 {
        format!("{n:.0} B")
    } else if n < 1024.0 * 1024.0 {
        format!("{:.1} KB", n / 1024.0)
    } else {
        format!("{:.2} MB", n / 1024.0 / 1024.0)
    }
}

fn doc_type_label(v: &str) -> &str {
    match v {
        "cnh" => "CNH",
        "rg" => "RG",
        "crlv" => "CRLV",
        "contrato" => "Contrato",
        "recibo" => "Recibo",
        "declaracao" => "Declaração",
        "oficio" => "Ofício",
        "boletim" => "Boletim",
        "laudo" => "Laudo externo",
        "processo" => "Processo",
        _ => "Outro",
    }
}

/// Monta o HTML do relatório técnico de exame documentoscópico (anexo do laudo).
fn doc_report_html(
    doc: &DocumentCaseFile,
    occ: &Uuid,
    run_blocks: &[(OcrRun, Vec<OcrTextBlock>)],
    fields: &[DetectedField],
    regions: &[DocumentRegion],
    logs: &[DocumentLog],
) -> String {
    let dt = |d: &chrono::DateTime<Utc>| d.format("%d/%m/%Y %H:%M").to_string();
    let conf = |c: Option<f64>| {
        c.map(|v| format!("{:.0}%", v * 100.0))
            .unwrap_or_else(|| "—".to_string())
    };
    let total_blocks: usize = run_blocks.iter().map(|(_, b)| b.len()).sum();
    let reviewed_blocks: usize = run_blocks
        .iter()
        .flat_map(|(_, b)| b.iter())
        .filter(|b| b.reviewed)
        .count();

    let mut h = String::new();
    h.push_str(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
<title>Relatório técnico — exame documentoscópico</title><style>\
*{box-sizing:border-box}\
body{font-family:'Segoe UI',Arial,sans-serif;color:#1a1d23;margin:0;padding:32px 40px;font-size:12px;line-height:1.5}\
h1{font-size:18px;margin:0 0 2px}h2{font-size:13px;margin:22px 0 6px;border-bottom:2px solid #1d4ed8;padding-bottom:3px;color:#1e293b}\
.sub{color:#555;font-size:11px;margin:0 0 14px}\
.note{border:1px solid #f59e0b;background:#fff7ed;border-radius:6px;padding:10px 12px;font-size:11px;margin:12px 0}\
table{border-collapse:collapse;width:100%;margin:4px 0 10px;font-size:11px}\
th,td{border:1px solid #d4d7dd;padding:4px 7px;text-align:left;vertical-align:top}\
th{background:#f1f5f9;width:170px;font-weight:600}\
table.grid th{width:auto;background:#f1f5f9}\
.mono{font-family:'Consolas',monospace;font-size:10px;word-break:break-all}\
pre{white-space:pre-wrap;background:#f8fafc;border:1px solid #e2e8f0;border-radius:5px;padding:8px 10px;font-size:11px;font-family:'Consolas',monospace;margin:4px 0}\
.muted{color:#777}.tag{display:inline-block;padding:0 6px;border-radius:10px;background:#e0e7ff;color:#1e40af;font-size:10px}\
footer{margin-top:26px;border-top:1px solid #d4d7dd;padding-top:8px;color:#666;font-size:10px}\
@media print{body{padding:0}}\
</style></head><body>",
    );

    // Cabeçalho + §13
    h.push_str("<h1>Relatório técnico — exame documentoscópico</h1>");
    h.push_str(&format!(
        "<p class=\"sub\">SICRO 2.0 · Documentoscopia assistida · gerado em {} · ocorrência {}</p>",
        esc_html(&Utc::now().format("%d/%m/%Y %H:%M").to_string()),
        esc_html(&occ.to_string())
    ));
    h.push_str(
        "<div class=\"note\"><strong>Natureza deste documento (§13).</strong> Consolida o exame \
técnico-computacional de <strong>apoio</strong> realizado no SICRO. As ferramentas auxiliam a \
leitura, a extração e a análise; a <strong>interpretação e a conclusão são exclusivamente do \
perito</strong>. Nenhuma afirmação automática de autenticidade ou autoria é feita. O arquivo \
original é preservado e nunca alterado.</div>",
    );

    // Identificação
    h.push_str("<h2>1. Identificação do documento</h2><table>");
    let row = |k: &str, v: &str| format!("<tr><th>{}</th><td>{}</td></tr>", k, v);
    h.push_str(&row("Título", &esc_html(&doc.title)));
    h.push_str(&row("Arquivo original", &esc_html(&doc.original_filename)));
    h.push_str(&row(
        "Tipo",
        &format!(
            "{} · {}",
            esc_html(doc_type_label(&doc.doc_type)),
            esc_html(&doc.file_type)
        ),
    ));
    h.push_str(&row("Páginas", &doc.page_count.to_string()));
    h.push_str(&row("Tamanho", &fmt_bytes(doc.size_bytes)));
    h.push_str(&row(
        "SHA-256",
        &format!("<span class=\"mono\">{}</span>", esc_html(&doc.sha256)),
    ));
    h.push_str(&row("Importado em", &esc_html(&dt(&doc.created_at))));
    h.push_str(&row("Situação", &esc_html(&doc.status)));
    if !doc.notes.trim().is_empty() {
        h.push_str(&row("Observações", &esc_html(&doc.notes)));
    }
    h.push_str("</table>");

    // Resumo
    h.push_str(&format!(
        "<p class=\"muted\">Resumo: {} pág. · {} execuções de OCR · {} blocos de texto ({} revisados) · {} campos · {} regiões · {} eventos de histórico.</p>",
        doc.page_count, run_blocks.len(), total_blocks, reviewed_blocks, fields.len(), regions.len(), logs.len()
    ));

    // OCR
    h.push_str("<h2>2. Reconhecimento de texto (OCR)</h2>");
    if run_blocks.is_empty() {
        h.push_str("<p class=\"muted\">Nenhuma execução de OCR registrada.</p>");
    }
    for (r, blocks) in run_blocks {
        h.push_str(&format!(
            "<p><strong>Motor:</strong> {} {} · <strong>idioma:</strong> {} · <strong>modo:</strong> {} · <strong>blocos:</strong> {} · <strong>confiança média:</strong> {} · {}</p>",
            esc_html(&r.engine), esc_html(&r.engine_version), esc_html(&r.language),
            esc_html(&r.mode), blocks.len(), conf(r.avg_confidence), esc_html(&dt(&r.created_at))
        ));
        let text: String = blocks
            .iter()
            .map(|b| match b.corrected_text.as_deref() {
                Some(c) if !c.trim().is_empty() => c,
                _ => b.text.as_str(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !text.trim().is_empty() {
            h.push_str(&format!("<pre>{}</pre>", esc_html(&text)));
        }
    }
    h.push_str("<p class=\"muted\">Texto revisado pelo perito quando indicado; o OCR é apoio à transcrição, sujeito a conferência humana.</p>");

    // Campos
    h.push_str("<h2>3. Campos extraídos</h2>");
    if fields.is_empty() {
        h.push_str("<p class=\"muted\">Nenhum campo extraído.</p>");
    } else {
        h.push_str("<table class=\"grid\"><tr><th>Campo</th><th>Valor</th><th>Fonte</th><th>Revisado</th><th>Conf.</th></tr>");
        for f in fields {
            let value = match f.corrected_value.as_deref() {
                Some(c) if !c.trim().is_empty() => c,
                _ => f.field_value.as_str(),
            };
            h.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc_html(&f.field_name),
                esc_html(value),
                esc_html(&f.source),
                if f.reviewed { "sim" } else { "não" },
                conf(f.confidence)
            ));
        }
        h.push_str("</table>");
    }

    // Regiões
    h.push_str("<h2>4. Regiões marcadas / códigos</h2>");
    if regions.is_empty() {
        h.push_str("<p class=\"muted\">Nenhuma região marcada.</p>");
    } else {
        h.push_str("<table class=\"grid\"><tr><th>Tipo</th><th>Rótulo</th><th>Pág.</th><th>Observações</th><th>Conf.</th></tr>");
        for rg in regions {
            h.push_str(&format!(
                "<tr><td><span class=\"tag\">{}</span></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc_html(&rg.region_type),
                esc_html(&rg.label),
                rg.page_number,
                esc_html(&rg.notes),
                conf(rg.confidence)
            ));
        }
        h.push_str("</table>");
    }

    // Histórico
    h.push_str("<h2>5. Histórico / cadeia de custódia</h2>");
    if logs.is_empty() {
        h.push_str("<p class=\"muted\">Sem eventos registrados.</p>");
    } else {
        h.push_str("<table class=\"grid\"><tr><th>Data/hora</th><th>Ação</th><th>Resultado</th><th>Responsável</th></tr>");
        for l in logs {
            h.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc_html(&dt(&l.created_at)),
                esc_html(&l.action),
                esc_html(&l.result),
                esc_html(l.actor.as_deref().unwrap_or("—"))
            ));
        }
        h.push_str("</table>");
    }

    h.push_str(
        "<footer>Documento gerado pelo SICRO 2.0 (Documentoscopia assistida). Ferramenta de apoio \
técnico-computacional — não substitui o exame nem a conclusão do perito responsável (§13). \
Indícios de manipulação digital (ELA, ruído, copy-move) e confrontos, quando realizados, são \
anexados separadamente como peças indiciárias.</footer></body></html>",
    );
    h
}

/// Gera o relatório técnico do documento (HTML + PDF best-effort) e devolve os
/// caminhos relativos. É um anexo do laudo; o original nunca é alterado (§13).
#[tauri::command]
pub async fn generate_doc_report(
    workspace_path: String,
    document_id: String,
) -> Result<DocReportArtifact> {
    let ws = PathBuf::from(&workspace_path);
    let (conn, occ) = open(&workspace_path)?;
    let id = parse_id(&document_id)?;
    let doc = repo::find_document_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation("documento não encontrado".into()))?;
    let runs = repo::list_ocr_runs(&conn, &id)?;
    let mut run_blocks: Vec<(OcrRun, Vec<OcrTextBlock>)> = Vec::new();
    for r in runs {
        let blocks = repo::list_blocks_for_run(&conn, &r.id)?;
        run_blocks.push((r, blocks));
    }
    let fields = repo::list_fields(&conn, &id)?;
    let regions = repo::list_regions(&conn, &id)?;
    let logs = repo::list_logs(&conn, &id)?;
    let html = doc_report_html(&doc, &occ, &run_blocks, &fields, &regions, &logs);

    let dir = ws.join("documentoscopia/relatorios");
    create_dir(&dir)?;
    let stamp = Utc::now().format("%Y%m%d_%H%M%S");
    let slug = sanitize_name(&doc.title, "documento");
    let html_rel = format!("documentoscopia/relatorios/{slug}_{stamp}.html");
    std::fs::write(ws.join(&html_rel), html.as_bytes())
        .map_err(|e| SicroError::Filesystem(format!("falha ao gravar relatório: {e}")))?;

    // PDF (best-effort — usa o Edge headless; se indisponível, fica só o HTML).
    let cache = ws.join("cache");
    let _ = create_dir(&cache);
    let pdf_rel = format!("documentoscopia/relatorios/{slug}_{stamp}.pdf");
    let pdf_relative_path =
        match crate::exporters::pdf::render_html_to_pdf(&html, &cache, &ws.join(&pdf_rel), None) {
            Ok(()) => Some(pdf_rel),
            Err(_) => None,
        };

    Ok(DocReportArtifact {
        html_relative_path: html_rel,
        pdf_relative_path,
    })
}

#[tauri::command]
pub async fn list_document_log(
    workspace_path: String,
    document_id: String,
) -> Result<Vec<DocumentLog>> {
    let (conn, _occ) = open(&workspace_path)?;
    let id = parse_id(&document_id)?;
    repo::list_logs(&conn, &id)
}

// ---------------------------------------------------------------------------
// Helpers internos

struct NewBlock {
    page_number: i64,
    text: String,
    confidence: Option<f64>,
    bbox: [f64; 4],
    block_type: String,
    reading_order: i64,
}

#[allow(clippy::too_many_arguments)]
fn persist_run(
    conn: &rusqlite::Connection,
    document_id: &Uuid,
    page_number: Option<i64>,
    engine: &str,
    engine_version: &str,
    language: &str,
    mode: &str,
    parameters_json: &str,
    blocks: Vec<NewBlock>,
) -> Result<OcrRunResult> {
    let now = Utc::now();
    let confs: Vec<f64> = blocks.iter().filter_map(|b| b.confidence).collect();
    let avg = if confs.is_empty() {
        None
    } else {
        Some(confs.iter().sum::<f64>() / confs.len() as f64)
    };
    let run = OcrRun {
        id: Uuid::new_v4(),
        document_id: *document_id,
        page_number,
        engine: engine.to_string(),
        engine_version: engine_version.to_string(),
        language: language.to_string(),
        mode: mode.to_string(),
        status: "concluido".to_string(),
        avg_confidence: avg,
        block_count: blocks.len() as i64,
        parameters_json: parameters_json.to_string(),
        created_at: now,
    };
    repo::insert_ocr_run(conn, &run)?;
    let mut persisted = Vec::with_capacity(blocks.len());
    for b in blocks {
        let block = OcrTextBlock {
            id: Uuid::new_v4(),
            ocr_run_id: run.id,
            document_id: *document_id,
            page_number: b.page_number,
            text: b.text,
            confidence: b.confidence,
            bbox_x: b.bbox[0],
            bbox_y: b.bbox[1],
            bbox_w: b.bbox[2],
            bbox_h: b.bbox[3],
            block_type: b.block_type,
            reading_order: b.reading_order,
            corrected_text: None,
            reviewed: false,
            created_at: now,
        };
        repo::insert_text_block(conn, &block)?;
        persisted.push(block);
    }
    Ok(OcrRunResult {
        run,
        blocks: persisted,
    })
}

fn norm_block_type(t: &str) -> String {
    let t = t.trim();
    if t.is_empty() {
        "paragraph".to_string()
    } else {
        t.to_string()
    }
}

/// Abre a conexão do workspace e devolve `(conn, occurrence_id)`.
fn open(workspace_path: &str) -> Result<(rusqlite::Connection, Uuid)> {
    let ws = PathBuf::from(workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    Ok((conn, manifest.occurrence_id))
}

fn parse_id(id: &str) -> Result<Uuid> {
    Uuid::parse_str(id).map_err(|e| SicroError::Validation(format!("id inválido: {e}")))
}

fn create_dir(p: &Path) -> Result<()> {
    std::fs::create_dir_all(p)
        .map_err(|e| SicroError::Filesystem(format!("falha ao criar pasta: {e}")))
}

/// Nome de arquivo livre de colisão dentro de `dir` (anexa -1, -2… se preciso).
fn unique_name(dir: &Path, desired: &str) -> String {
    let p = Path::new(desired);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("documento");
    let ext = p.extension().and_then(|s| s.to_str());
    let make = |n: u32| match (n, ext) {
        (0, Some(e)) => format!("{stem}.{e}"),
        (0, None) => stem.to_string(),
        (n, Some(e)) => format!("{stem}-{n}.{e}"),
        (n, None) => format!("{stem}-{n}"),
    };
    let mut n = 0;
    loop {
        let candidate = make(n);
        if !dir.join(&candidate).exists() {
            return candidate;
        }
        n += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn log_doc(
    conn: &rusqlite::Connection,
    occurrence_id: &Uuid,
    document_id: Option<&Uuid>,
    action: &str,
    parameters_json: &str,
    result: &str,
    source_hash: Option<&str>,
    output_hash: Option<&str>,
) -> Result<()> {
    let log = DocumentLog {
        id: Uuid::new_v4(),
        document_id: document_id.copied(),
        occurrence_id: *occurrence_id,
        action: action.to_string(),
        parameters_json: parameters_json.to_string(),
        result: result.to_string(),
        source_hash: source_hash.map(|s| s.to_string()),
        output_hash: output_hash.map(|s| s.to_string()),
        actor: None,
        created_at: Utc::now(),
    };
    repo::insert_log(conn, &log)?;
    let _ = occurrence_repo::record_audit(
        conn,
        Some(occurrence_id),
        action,
        Some("documentoscopia"),
        Some("doc_documents"),
        document_id,
        source_hash.or(output_hash),
    );
    Ok(())
}
