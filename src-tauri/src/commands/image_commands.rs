//! Comandos Tauri do Editor de Imagens: análises (`.sicroimage`), exportação
//! de derivados, leitura de assets e processamento (pilha de operações).

use std::path::{Path, PathBuf};

use base64::Engine as _;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::{
    image_analysis_repo, occurrence_repo,
};
use crate::error::{Result, SicroError};
use crate::filesystem::{
    atomic_write_bytes, resolve_workspace_relative, sanitize_relative_path,
};
use crate::hashing::sha256::{sha256_bytes, sha256_file};
use crate::image_editor::{filters, mask, metadata, pipeline, processor, report};
use crate::models::{
    BackendAdjustments, BackendOperation, CreateImageAnalysisInput, ExportImageInput,
    ImageAnalysis, ImageAssetBytes, ImageExport, ImageHistogram, ImageMetadata,
    ImageOperationLog, ImageSourceKind, ImportLocalImageInput, MaskSpec,
};
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};

const ANALYSES_DIR: &str = "imagens/analises";
const ORIGINALS_DIR: &str = "imagens/originais";
const CAMADAS_DIR: &str = "imagens/camadas";

#[derive(Debug, Clone, Deserialize)]
pub struct SaveImageAnalysisInput {
    /// JSON inteiro do `.sicroimage`; o schema vive no frontend.
    pub doc: serde_json::Value,
    /// Novo `metadata_json` da linha; ausente = mantém o existente.
    pub metadata_json: Option<String>,
    pub title: Option<String>,
}

#[tauri::command]
pub async fn create_image_analysis_from_evidence(
    workspace_path: String,
    input: CreateImageAnalysisInput,
) -> Result<ImageAnalysis> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let abs_src = resolve_workspace_relative(&ws, &input.original_relative_path)?;
    if !abs_src.is_file() {
        return Err(SicroError::Filesystem(format!(
            "imagem original não encontrada em {}",
            abs_src.display()
        )));
    }
    let meta_metadata = metadata::read_metadata(&abs_src, false)?;

    let now = Utc::now();
    let id = Uuid::new_v4();
    let rel_doc = format!(
        "{}/imagem_{}.sicroimage",
        ANALYSES_DIR,
        &id.to_string()[..8],
    );
    write_initial_sicroimage(&ws, &rel_doc, &id, &manifest.occurrence_id, &input, &meta_metadata)?;

    let row = ImageAnalysis {
        id,
        occurrence_id: manifest.occurrence_id,
        title: input.title.clone(),
        source_kind: input.source_kind,
        source_id: input.source_id.clone(),
        original_relative_path: input.original_relative_path.clone(),
        original_hash_sha256: input.original_hash_sha256.clone(),
        analysis_relative_path: rel_doc.clone(),
        last_export_relative_path: None,
        status: "draft".to_string(),
        metadata_json: serde_json::to_string(&meta_metadata)
            .unwrap_or_else(|_| "{}".to_string()),
        created_at: now,
        updated_at: now,
    };
    image_analysis_repo::insert(&conn, &row)?;
    image_analysis_repo::insert_log(
        &conn,
        &manifest.occurrence_id,
        &id,
        "analysis.created",
        &json!({
            "source_kind": input.source_kind.as_str(),
            "source_id": input.source_id,
            "original_relative_path": input.original_relative_path,
        })
        .to_string(),
    )?;
    occurrence_repo::record_audit(
        &conn,
        Some(&manifest.occurrence_id),
        "image.analysis_created",
        Some("image_editor"),
        Some("image_analysis"),
        Some(&id),
        Some(input.source_kind.as_str()),
    )?;
    Ok(row)
}

#[tauri::command]
pub async fn create_image_analysis_from_file(
    workspace_path: String,
    input: ImportLocalImageInput,
) -> Result<ImageAnalysis> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let src = PathBuf::from(&input.source_path);
    if !src.is_file() {
        return Err(SicroError::Filesystem(format!(
            "arquivo não existe: {}",
            src.display()
        )));
    }
    let extension = src
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| SicroError::Validation("arquivo sem extensão".into()))?;
    let allowed = ["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff"];
    if !allowed.contains(&extension.as_str()) {
        return Err(SicroError::Validation(format!(
            "extensão não suportada: {extension}"
        )));
    }
    let file_id = Uuid::new_v4();
    let basename = src
        .file_stem()
        .and_then(|s| s.to_str())
        .map(sanitize_slug)
        .unwrap_or_else(|| "imagem".into());
    let rel_dest = format!(
        "{}/{}__{}.{}",
        ORIGINALS_DIR,
        basename,
        &file_id.to_string()[..8],
        extension,
    );
    let abs_dest = resolve_workspace_relative(&ws, &rel_dest)?;
    if let Some(parent) = abs_dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            SicroError::Filesystem(format!("cannot create originals dir: {e}"))
        })?;
    }
    let bytes = std::fs::read(&src).map_err(|e| {
        SicroError::Filesystem(format!("cannot read {}: {}", src.display(), e))
    })?;
    atomic_write_bytes(&abs_dest, &bytes)?;
    let hash = sha256_file(&abs_dest).ok();
    let meta_metadata = metadata::read_metadata(&abs_dest, false)?;

    let title = input
        .title
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| basename.clone());

    let payload = CreateImageAnalysisInput {
        title,
        source_kind: ImageSourceKind::LocalImport,
        source_id: Some(file_id.to_string()),
        original_relative_path: rel_dest.clone(),
        original_hash_sha256: hash.clone(),
    };

    let now = Utc::now();
    let id = Uuid::new_v4();
    let rel_doc = format!(
        "{}/imagem_{}.sicroimage",
        ANALYSES_DIR,
        &id.to_string()[..8],
    );
    write_initial_sicroimage(
        &ws,
        &rel_doc,
        &id,
        &manifest.occurrence_id,
        &payload,
        &meta_metadata,
    )?;

    let row = ImageAnalysis {
        id,
        occurrence_id: manifest.occurrence_id,
        title: payload.title.clone(),
        source_kind: payload.source_kind,
        source_id: payload.source_id.clone(),
        original_relative_path: rel_dest.clone(),
        original_hash_sha256: hash,
        analysis_relative_path: rel_doc,
        last_export_relative_path: None,
        status: "draft".to_string(),
        metadata_json: serde_json::to_string(&meta_metadata)
            .unwrap_or_else(|_| "{}".to_string()),
        created_at: now,
        updated_at: now,
    };
    image_analysis_repo::insert(&conn, &row)?;
    image_analysis_repo::insert_log(
        &conn,
        &manifest.occurrence_id,
        &id,
        "analysis.created_from_file",
        &json!({
            "original_relative_path": rel_dest,
            "source_path": input.source_path,
        })
        .to_string(),
    )?;
    occurrence_repo::record_audit(
        &conn,
        Some(&manifest.occurrence_id),
        "image.analysis_created",
        Some("image_editor"),
        Some("image_analysis"),
        Some(&id),
        Some("local_import"),
    )?;
    Ok(row)
}

#[tauri::command]
pub async fn list_image_analyses(
    workspace_path: String,
) -> Result<Vec<ImageAnalysis>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    image_analysis_repo::list_by_occurrence(&conn, &manifest.occurrence_id)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImageAnalysisPayload {
    pub analysis: ImageAnalysis,
    pub doc: serde_json::Value,
}

#[tauri::command]
pub async fn read_image_analysis(
    workspace_path: String,
    analysis_id: String,
) -> Result<ImageAnalysisPayload> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let id = Uuid::parse_str(&analysis_id)
        .map_err(|e| SicroError::Validation(format!("analysis_id inválido: {e}")))?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    let analysis = image_analysis_repo::find_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("análise {} não encontrada", id)))?;
    let abs_doc = resolve_workspace_relative(&ws, &analysis.analysis_relative_path)?;
    let bytes = std::fs::read(&abs_doc).map_err(|e| {
        SicroError::Filesystem(format!(
            "cannot read {}: {}",
            abs_doc.display(),
            e
        ))
    })?;
    let doc: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(SicroError::from)?;
    Ok(ImageAnalysisPayload { analysis, doc })
}

#[tauri::command]
pub async fn save_image_analysis(
    workspace_path: String,
    analysis_id: String,
    input: SaveImageAnalysisInput,
) -> Result<ImageAnalysis> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let id = Uuid::parse_str(&analysis_id)
        .map_err(|e| SicroError::Validation(format!("analysis_id inválido: {e}")))?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    let mut analysis = image_analysis_repo::find_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("análise {} não encontrada", id)))?;

    let abs_doc = resolve_workspace_relative(&ws, &analysis.analysis_relative_path)?;
    if let Some(parent) = abs_doc.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let bytes = serde_json::to_vec_pretty(&input.doc)?;
    atomic_write_bytes(&abs_doc, &bytes)?;

    let now = Utc::now();
    if let Some(title) = input.title.as_deref() {
        if title.trim() != analysis.title {
            conn.execute(
                "UPDATE image_analyses SET title = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![title.trim(), now.to_rfc3339(), id.to_string()],
            )?;
            analysis.title = title.trim().to_string();
        }
    }
    if let Some(metadata_json) = input.metadata_json.as_deref() {
        if metadata_json != analysis.metadata_json {
            conn.execute(
                "UPDATE image_analyses SET metadata_json = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![metadata_json, now.to_rfc3339(), id.to_string()],
            )?;
            analysis.metadata_json = metadata_json.to_string();
        }
    }
    image_analysis_repo::touch_updated(&conn, &id, now, None)?;
    analysis.updated_at = now;

    image_analysis_repo::insert_log(
        &conn,
        &analysis.occurrence_id,
        &id,
        "analysis.saved",
        "{}",
    )?;
    Ok(analysis)
}

#[tauri::command]
pub async fn export_image_derivative(
    workspace_path: String,
    analysis_id: String,
    input: ExportImageInput,
) -> Result<ImageExport> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let id = Uuid::parse_str(&analysis_id)
        .map_err(|e| SicroError::Validation(format!("analysis_id inválido: {e}")))?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let analysis = image_analysis_repo::find_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("análise {} não encontrada", id)))?;

    let artifact = pipeline::run_export(&ws, &analysis, &input)?;

    let now = Utc::now();
    let export = ImageExport {
        id: Uuid::new_v4(),
        occurrence_id: analysis.occurrence_id,
        image_analysis_id: analysis.id,
        output_relative_path: artifact.output_relative_path.clone(),
        sidecar_relative_path: Some(artifact.sidecar_relative_path.clone()),
        hash_sha256: Some(artifact.hash_sha256.clone()),
        width: Some(artifact.width as i32),
        height: Some(artifact.height as i32),
        format: artifact.format.clone(),
        created_at: now,
        operation_summary_json: input
            .operation_summary_json
            .clone()
            .unwrap_or_else(|| "{}".to_string()),
    };
    image_analysis_repo::insert_export(&conn, &export)?;
    image_analysis_repo::touch_updated(
        &conn,
        &analysis.id,
        now,
        Some(&artifact.output_relative_path),
    )?;
    image_analysis_repo::insert_log(
        &conn,
        &analysis.occurrence_id,
        &analysis.id,
        "analysis.exported",
        &json!({
            "format": artifact.format,
            "hash_sha256": artifact.hash_sha256,
            "output_relative_path": artifact.output_relative_path,
            "sidecar_relative_path": artifact.sidecar_relative_path,
            "size_bytes": artifact.size_bytes,
        })
        .to_string(),
    )?;
    occurrence_repo::record_audit(
        &conn,
        Some(&analysis.occurrence_id),
        "image.exported",
        Some("image_editor"),
        Some("image_export"),
        Some(&export.id),
        Some(&artifact.format),
    )?;
    Ok(export)
}

#[tauri::command]
pub async fn read_image_asset(
    workspace_path: String,
    relative_path: String,
) -> Result<ImageAssetBytes> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let safe = sanitize_relative_path(&relative_path)?;
    let abs = ws.join(&safe);
    if !abs.is_file() {
        return Err(SicroError::Filesystem(format!(
            "asset não encontrado em {}",
            abs.display()
        )));
    }
    let bytes = std::fs::read(&abs)?;
    let size_bytes = bytes.len() as u64;
    let mime_type = metadata::guess_mime_for_path(&safe)
        .unwrap_or("application/octet-stream")
        .to_string();
    let base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(ImageAssetBytes {
        relative_path,
        mime_type,
        base64,
        size_bytes,
    })
}

#[tauri::command]
pub async fn read_all_image_metadata(
    workspace_path: String,
    relative_path: String,
) -> Result<crate::image_editor::full_metadata::FullMetadata> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let abs = resolve_workspace_relative(&ws, &relative_path)?;
    if !abs.is_file() {
        return Err(SicroError::Filesystem(format!("asset não encontrado em {}", abs.display())));
    }
    Ok(crate::image_editor::full_metadata::read_all(&abs))
}

/// Metadados resumidos (dimensões, formato, hashes, EXIF principal).
#[tauri::command]
pub async fn get_image_metadata(
    workspace_path: String,
    relative_path: String,
    compute_hash: Option<bool>,
) -> Result<ImageMetadata> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let abs = resolve_workspace_relative(&ws, &relative_path)?;
    if !abs.is_file() {
        return Err(SicroError::Filesystem(format!(
            "asset não encontrado em {}",
            abs.display()
        )));
    }
    metadata::read_metadata(&abs, compute_hash.unwrap_or(false))
}

#[tauri::command]
pub async fn list_image_operation_logs(
    workspace_path: String,
    analysis_id: String,
    limit: Option<i64>,
) -> Result<Vec<ImageOperationLog>> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let id = Uuid::parse_str(&analysis_id)
        .map_err(|e| SicroError::Validation(format!("analysis_id inválido: {e}")))?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    image_analysis_repo::list_logs_for_analysis(&conn, &id, limit.unwrap_or(200))
}

// ---------------------------------------------------------------------------
// Helpers

fn write_initial_sicroimage(
    workspace_root: &Path,
    rel_doc: &str,
    id: &Uuid,
    occurrence_id: &Uuid,
    input: &CreateImageAnalysisInput,
    meta: &ImageMetadata,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let envelope = json!({
        "schema_version": "0.1",
        "image_analysis_id": id.to_string(),
        "occurrence_id": occurrence_id.to_string(),
        "title": input.title,
        "source": {
            "kind": input.source_kind.as_str(),
            "source_id": input.source_id,
            "original_relative_path": input.original_relative_path,
            "original_hash_sha256": input.original_hash_sha256,
            "mime_type": meta.mime_type,
            "width": meta.width,
            "height": meta.height,
            "size_bytes": meta.size_bytes,
        },
        "canvas": {
            "zoom": 1.0,
            "pan_x": 0.0,
            "pan_y": 0.0,
            "rotation": 0.0,
            "background_color": "#1f2933",
        },
        "view_adjustments": {
            "brightness": 0.0,
            "contrast": 0.0,
            "exposure": 0.0,
            "gamma": 1.0,
            "saturation": 0.0,
            "grayscale": false,
            "invert": false,
        },
        "processing_stack": [],
        "layers": [
            {
                "id": "layer_base",
                "name": "Imagem base",
                "kind": "image_base",
                "visible": true,
                "locked": true,
                "opacity": 1.0,
            },
            {
                "id": "layer_annotations",
                "name": "Anotações",
                "kind": "annotations",
                "visible": true,
                "locked": false,
                "opacity": 1.0,
            },
        ],
        "annotations": [],
        "measurements": [],
        "scale": null,
        "exports": [],
        "created_at": now,
        "updated_at": now,
    });
    let abs = resolve_workspace_relative(workspace_root, rel_doc)?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            SicroError::Filesystem(format!("cannot create analises dir: {e}"))
        })?;
    }
    let bytes = serde_json::to_vec_pretty(&envelope)?;
    atomic_write_bytes(&abs, &bytes)?;
    Ok(())
}

fn sanitize_slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let ok = c.is_ascii_alphanumeric() || c == '-' || c == '_';
        out.push(if ok { c } else { '_' });
    }
    let trimmed: String = out.trim_matches('_').chars().take(40).collect();
    if trimmed.is_empty() {
        "imagem".to_string()
    } else {
        trimmed
    }
}

// ---------------------------------------------------------------------------
// Processamento: histograma, pilha de operações, camadas e relatório

/// Histograma + média/desvio por canal de uma imagem do workspace.
#[tauri::command]
pub async fn compute_image_histogram(
    workspace_path: String,
    relative_path: String,
) -> Result<ImageHistogram> {
    let ws = PathBuf::from(&workspace_path);
    let abs = resolve_workspace_relative(&ws, &relative_path)?;
    let img = image::open(&abs)
        .map_err(|e| {
            SicroError::Filesystem(format!(
                "não consegui abrir {}: {}",
                abs.display(),
                e
            ))
        })?
        .to_rgba8();
    Ok(filters::histogram::compute(&img))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ApplyOperationPreviewResult {
    pub image_base64: String,
    pub width: u32,
    pub height: u32,
}

/// Preview da pilha sobre um bitmap já reduzido pelo cliente (não abre o original):
/// filtrar dezenas de megapixels e trafegar o PNG por base64 a cada filtro custava
/// minutos. O export continua em resolução cheia sobre o original (o fiel).
#[derive(Debug, Clone, Deserialize)]
pub struct ApplyOperationStackPreviewInput {
    /// PNG/JPEG em base64, sem prefixo `data:image/`.
    pub image_base64: String,
    #[serde(default)]
    pub adjustments: Option<BackendAdjustments>,
    #[serde(default)]
    pub operations: Vec<BackendOperation>,
}

#[derive(Debug, Deserialize)]
pub struct FilterThumbnailsInput {
    /// Imagem pequena (PNG/JPEG em base64): o original reduzido.
    pub image_base64: String,
    #[serde(default)]
    pub adjustments: Option<BackendAdjustments>,
    /// Pilha atual, aplicada uma vez antes dos candidatos.
    #[serde(default)]
    pub operations: Vec<BackendOperation>,
    /// Um filtro por miniatura, na ordem da galeria.
    pub candidates: Vec<BackendOperation>,
}

/// Miniaturas da galeria: a pilha atual + cada candidato, em JPEG base64 (vazio se o filtro falhar).
#[tauri::command]
pub async fn filter_thumbnails(input: FilterThumbnailsInput) -> Result<Vec<String>> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<String>> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&input.image_base64)
            .map_err(|e| SicroError::Validation(format!("base64 inválido: {e}")))?;
        let mut base = image::load_from_memory(&bytes)
            .map_err(|e| SicroError::Validation(format!("imagem inválida: {e}")))?
            .to_rgba8();
        if let Some(adj) = input.adjustments.as_ref() {
            processor::apply_adjustments(&mut base, adj);
        }
        for op in &input.operations {
            base = processor::apply_operation(base, op);
        }
        let encode = |img: image::RgbaImage| -> String {
            let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
            let mut buf = Vec::new();
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 80);
            if enc.encode_image(&rgb).is_err() {
                return String::new();
            }
            base64::engine::general_purpose::STANDARD.encode(&buf)
        };
        // Filtros independentes: em paralelo, um pedaço por núcleo; pânico vira miniatura vazia.
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
        let chunk = input.candidates.len().div_ceil(n).max(1);
        let base = &base;
        let out: Vec<String> = std::thread::scope(|sc| {
            let handles: Vec<_> = input
                .candidates
                .chunks(chunk)
                .map(|ops| {
                    sc.spawn(move || {
                        ops.iter()
                            .map(|op| {
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    encode(processor::apply_operation(base.clone(), op))
                                }))
                                .unwrap_or_default()
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        });
        Ok(out)
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("falha nas miniaturas: {e}")))?
}

#[tauri::command]
pub async fn apply_operation_stack_preview(
    input: ApplyOperationStackPreviewInput,
) -> Result<ApplyOperationPreviewResult> {
    // CPU-bound: pool de bloqueantes (ver `apply_operation_stack`).
    let out = tauri::async_runtime::spawn_blocking(
        move || -> Result<ApplyOperationPreviewResult> {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&input.image_base64)
                .map_err(|e| SicroError::Validation(format!("base64 inválido: {e}")))?;
            let mut img = image::load_from_memory(&bytes)
                .map_err(|e| SicroError::Validation(format!("imagem inválida: {e}")))?
                .to_rgba8();
            if let Some(adj) = input.adjustments.as_ref() {
                processor::apply_adjustments(&mut img, adj);
            }
            for op in &input.operations {
                img = processor::apply_operation(img, op);
            }
            let (w, h) = (img.width(), img.height());

            let mut buf = Vec::new();
            {
                use std::io::Cursor;
                let mut cursor = Cursor::new(&mut buf);
                img.write_to(&mut cursor, image::ImageFormat::Png)
                    .map_err(|e| {
                        SicroError::Workspace(format!("falha ao codificar PNG: {e}"))
                    })?;
            }
            let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
            Ok(ApplyOperationPreviewResult {
                image_base64: b64,
                width: w,
                height: h,
            })
        },
    )
    .await
    .map_err(|e| {
        SicroError::Workspace(format!("falha ao processar preview da pilha: {e}"))
    })??;
    Ok(out)
}

/// Recorta a seleção (máscara normalizada [0,1]; fora = transparente) para um PNG.
/// `apply_processing=false` recorta do ORIGINAL; `true`, do resultado processado.
#[derive(Debug, Clone, Deserialize)]
pub struct CopyRegionInput {
    /// Imagem original (relativa ao workspace).
    pub relative_path: String,
    /// Geometria da seleção (normalizada).
    pub mask: MaskSpec,
    /// Stem do arquivo de saída (uuid da camada, gerado no front).
    pub layer_id: String,
    #[serde(default)]
    pub apply_processing: bool,
    #[serde(default)]
    pub adjustments: Option<BackendAdjustments>,
    #[serde(default)]
    pub operations: Vec<BackendOperation>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CopyRegionResult {
    /// Caminho relativo do PNG gravado (`imagens/camadas/{layer_id}.png`).
    pub relative_path: String,
    /// Offset da bbox na imagem original (px) — posiciona a camada.
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub hash_sha256: String,
    /// PNG base64 (sem prefixo) para exibição imediata, sem reler o disco.
    pub base64: String,
    pub mime: String,
}

#[tauri::command]
pub async fn copy_region_to_layer(
    workspace_path: String,
    input: CopyRegionInput,
) -> Result<CopyRegionResult> {
    let ws = PathBuf::from(&workspace_path);
    let abs = resolve_workspace_relative(&ws, &input.relative_path)?;
    let stem: String = input
        .layer_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = if stem.is_empty() {
        "camada".to_string()
    } else {
        stem
    };
    let rel_out = format!("{CAMADAS_DIR}/{stem}.png");
    let abs_out = resolve_workspace_relative(&ws, &rel_out)?;

    // CPU-bound: pool de bloqueantes (ver `apply_operation_stack`).
    let out = tauri::async_runtime::spawn_blocking(
        move || -> Result<CopyRegionResult> {
            let mut img = image::open(&abs)
                .map_err(|e| {
                    SicroError::Filesystem(format!(
                        "não consegui abrir {}: {}",
                        abs.display(),
                        e
                    ))
                })?
                .to_rgba8();
            if input.apply_processing {
                if let Some(adj) = input.adjustments.as_ref() {
                    processor::apply_adjustments(&mut img, adj);
                }
                for op in &input.operations {
                    img = processor::apply_operation(img, op);
                }
            }
            let (crop, x, y) = mask::crop_masked(&img, &input.mask);
            let (width, height) = (crop.width(), crop.height());

            let mut buf = Vec::new();
            {
                use std::io::Cursor;
                let mut cursor = Cursor::new(&mut buf);
                crop.write_to(&mut cursor, image::ImageFormat::Png)
                    .map_err(|e| {
                        SicroError::Workspace(format!(
                            "falha ao codificar PNG da camada: {e}"
                        ))
                    })?;
            }
            if let Some(parent) = abs_out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    SicroError::Filesystem(format!(
                        "cannot create camadas dir: {e}"
                    ))
                })?;
            }
            atomic_write_bytes(&abs_out, &buf)?;
            let hash = sha256_bytes(&buf);
            let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
            Ok(CopyRegionResult {
                relative_path: rel_out,
                x,
                y,
                width,
                height,
                hash_sha256: hash,
                base64: b64,
                mime: "image/png".to_string(),
            })
        },
    )
    .await
    .map_err(|e| SicroError::Workspace(format!("falha ao recortar camada: {e}")))??;
    Ok(out)
}

/// Aplica a pilha de operações na imagem original e devolve PNG/JPEG base64.
/// Não persiste nada; quem persiste é `export_image_derivative`.
#[derive(Debug, Clone, Deserialize)]
pub struct ApplyOperationStackInput {
    pub relative_path: String,
    #[serde(default)]
    pub adjustments: Option<BackendAdjustments>,
    #[serde(default)]
    pub operations: Vec<BackendOperation>,
    /// true = JPEG; false (default) = PNG.
    #[serde(default)]
    pub as_jpeg: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ApplyOperationStackResult {
    pub image_base64: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
}

#[tauri::command]
pub async fn apply_operation_stack(
    workspace_path: String,
    input: ApplyOperationStackInput,
) -> Result<ApplyOperationStackResult> {
    let ws = PathBuf::from(&workspace_path);
    let abs = resolve_workspace_relative(&ws, &input.relative_path)?;

    // CPU-bound e pesado (filtros O(w·h·k²)). Rodar direto no `async fn` ocupava
    // um worker do runtime sem ceder; com vários filtros em sequência os workers
    // esgotavam e o preview travava em "Aplicando filtros…". Daí o spawn_blocking.
    let out = tauri::async_runtime::spawn_blocking(
        move || -> Result<ApplyOperationStackResult> {
            let mut img = image::open(&abs)
                .map_err(|e| {
                    SicroError::Filesystem(format!(
                        "não consegui abrir {}: {}",
                        abs.display(),
                        e
                    ))
                })?
                .to_rgba8();

            if let Some(adj) = input.adjustments.as_ref() {
                processor::apply_adjustments(&mut img, adj);
            }
            for op in &input.operations {
                img = processor::apply_operation(img, op);
            }
            let (w, h) = (img.width(), img.height());

            let (fmt, mime) = if input.as_jpeg {
                (image::ImageFormat::Jpeg, "image/jpeg")
            } else {
                (image::ImageFormat::Png, "image/png")
            };
            let mut buf = Vec::new();
            {
                use std::io::Cursor;
                let mut cursor = Cursor::new(&mut buf);
                img.write_to(&mut cursor, fmt).map_err(|e| {
                    SicroError::Workspace(format!("falha ao codificar: {e}"))
                })?;
            }
            let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
            Ok(ApplyOperationStackResult {
                image_base64: b64,
                mime: mime.to_string(),
                width: w,
                height: h,
            })
        },
    )
    .await
    .map_err(|e| {
        SicroError::Workspace(format!("falha ao processar pilha de operações: {e}"))
    })??;
    Ok(out)
}

/// Relatório HTML auto-contido da análise (EXIF, hashes, pilha de operações,
/// logs, thumbnail). O front mostra num iframe ou manda para o pipeline PDF.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImageAnalysisReportArtifact {
    pub html: String,
    pub output_relative_path: String,
}

#[tauri::command]
pub async fn generate_image_analysis_report(
    workspace_path: String,
    analysis_id: String,
) -> Result<ImageAnalysisReportArtifact> {
    let ws = PathBuf::from(&workspace_path);
    let _manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let analysis_uuid = Uuid::parse_str(&analysis_id)
        .map_err(|e| SicroError::Validation(format!("UUID inválido: {e}")))?;
    let analysis = image_analysis_repo::find_by_id(&conn, &analysis_uuid)?
        .ok_or_else(|| SicroError::Validation("análise não encontrada".to_string()))?;

    let doc_abs =
        resolve_workspace_relative(&ws, &analysis.analysis_relative_path)?;
    let doc_bytes = std::fs::read(&doc_abs).map_err(|e| {
        SicroError::Filesystem(format!("cannot read .sicroimage: {e}"))
    })?;
    let doc_json: serde_json::Value = serde_json::from_slice(&doc_bytes)
        .map_err(|e| SicroError::Validation(format!(".sicroimage inválido: {e}")))?;

    let orig_abs =
        resolve_workspace_relative(&ws, &analysis.original_relative_path)?;
    let hashes = crate::image_editor::hashes::compute_all_hashes(&orig_abs).ok();
    let hashes_json = hashes.as_ref().and_then(|h| serde_json::to_value(h).ok());

    let exif_value =
        crate::image_editor::exif::read_exif_value(&orig_abs);

    let logs_rows =
        image_analysis_repo::list_logs_for_analysis(&conn, &analysis_uuid, 100)?;
    let logs: Vec<serde_json::Value> = logs_rows
        .iter()
        .map(|l| {
            json!({
                "created_at": l.created_at.to_rfc3339(),
                "action": l.action,
                "details_json": l.details_json,
            })
        })
        .collect();

    let thumb_data_uri = match image::open(&orig_abs) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let max_w: u32 = 800;
            let (w, h) = (rgba.width(), rgba.height());
            let (tw, th) = if w > max_w {
                let scale = max_w as f32 / w as f32;
                (max_w, (h as f32 * scale).max(1.0) as u32)
            } else {
                (w, h)
            };
            let resized = image::imageops::resize(
                &rgba,
                tw,
                th,
                image::imageops::FilterType::Lanczos3,
            );
            let mut buf = Vec::new();
            {
                use std::io::Cursor;
                let mut cursor = Cursor::new(&mut buf);
                if resized
                    .write_to(&mut cursor, image::ImageFormat::Png)
                    .is_ok()
                {
                    Some(report::encode_thumbnail_data_uri(&buf, "image/png"))
                } else {
                    None
                }
            }
        }
        Err(_) => None,
    };

    let report_input = report::ReportInput {
        analysis: &analysis,
        doc_json: &doc_json,
        exif_json: exif_value.as_ref(),
        hashes_json: hashes_json.as_ref(),
        operation_logs: &logs,
        thumbnail_data_uri: thumb_data_uri,
    };
    let html = report::render_html(&report_input);

    let stamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let rel = format!(
        "imagens/relatorios/{}_{}.html",
        sanitize_slug(&analysis.title),
        stamp
    );
    let abs = resolve_workspace_relative(&ws, &rel)?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            SicroError::Filesystem(format!("cannot create reports dir: {e}"))
        })?;
    }
    atomic_write_bytes(&abs, html.as_bytes())?;

    Ok(ImageAnalysisReportArtifact {
        html,
        output_relative_path: rel,
    })
}
