//! Pipeline de export: carrega a imagem (original ou PNG composto do front),
//! aplica ajustes e operações, grava em `imagens/exports/` com sidecar JSON
//! (origem, operações, hashes, versão). O caller grava a linha em `image_exports`.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use chrono::Utc;
use image::{ImageFormat, RgbaImage};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{Result, SicroError};
use crate::filesystem::{atomic_write_bytes, resolve_workspace_relative};
use crate::hashing::sha256::sha256_bytes;
use crate::models::{BackendOperation, ExportImageInput, ImageAnalysis};
use crate::workspace::manifest::APP_VERSION;

use super::processor::{apply_adjustments, apply_operation};

/// Saída de `run_export`: arquivo já em disco + dados para a linha em `image_exports`.
#[derive(Debug, Clone)]
pub struct ExportArtifact {
    pub output_relative_path: String,
    pub sidecar_relative_path: String,
    pub width: u32,
    pub height: u32,
    pub hash_sha256: String,
    pub format: String,
    pub size_bytes: u64,
}

pub fn run_export(
    workspace_root: &Path,
    analysis: &ImageAnalysis,
    input: &ExportImageInput,
) -> Result<ExportArtifact> {
    let format_str = input
        .format
        .as_deref()
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "png".to_string());
    let format = match format_str.as_str() {
        "png" => ImageFormat::Png,
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        other => {
            return Err(SicroError::Validation(format!(
                "formato de exportação não suportado: {other}"
            )));
        }
    };

    // PNG composto pelo front (anotações já gravadas) ou original em disco.
    let mut img: RgbaImage = if let Some(b64) = input.composed_png_base64.as_deref() {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| SicroError::Validation(format!("base64 inválido: {e}")))?;
        image::load_from_memory(&bytes)
            .map_err(|e| SicroError::Validation(format!("PNG composto inválido: {e}")))?
            .to_rgba8()
    } else {
        let abs = resolve_workspace_relative(
            workspace_root,
            &analysis.original_relative_path,
        )?;
        image::open(&abs)
            .map_err(|e| {
                SicroError::Filesystem(format!(
                    "não consegui abrir {}: {}",
                    abs.display(),
                    e
                ))
            })?
            .to_rgba8()
    };

    // Ajustes só quando pedido; o padrão é confiar no PNG composto.
    if input.apply_backend_adjustments {
        if let Some(adj) = input.adjustments.as_ref() {
            apply_adjustments(&mut img, adj);
        }
    }

    // Operações sempre: podem ter sido pedidas depois de compor o canvas.
    for op in &input.operations {
        img = apply_operation(img, op);
    }

    // Anotações e camadas de pixels por cima (camada transparente do tamanho do resultado).
    if let Some(b64) = input.overlay_png_base64.as_deref() {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| SicroError::Validation(format!("base64 inválido: {e}")))?;
        let mut over = image::load_from_memory(&bytes)
            .map_err(|e| SicroError::Validation(format!("camada de anotações inválida: {e}")))?
            .to_rgba8();
        if over.dimensions() != img.dimensions() {
            over = image::imageops::resize(&over, img.width(), img.height(), image::imageops::FilterType::Triangle);
        }
        image::imageops::overlay(&mut img, &over, 0, 0);
    }

    // Tarjas por último: desfocam/pixelizam o que ficou embaixo, inclusive anotações.
    for r in &input.redactions {
        super::redact::apply(&mut img, r);
    }

    let (width, height) = (img.width(), img.height());

    let mut buf: Vec<u8> = Vec::new();
    {
        use std::io::Cursor;
        let mut cursor = Cursor::new(&mut buf);
        img.write_to(&mut cursor, format).map_err(|e| {
            SicroError::Workspace(format!("falha ao codificar imagem: {e}"))
        })?;
    }
    let hash = sha256_bytes(&buf);
    let size_bytes = buf.len() as u64;

    let ext = match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        _ => "bin",
    };
    let stamp = Utc::now().format("%Y%m%d_%H%M%S");
    let export_id = Uuid::new_v4();
    let rel_out = format!(
        "imagens/exports/{}_{}_{}.{ext}",
        sanitize_slug(&analysis.title),
        stamp,
        &export_id.to_string()[..8],
    );
    let abs_out: PathBuf = resolve_workspace_relative(workspace_root, &rel_out)?;
    if let Some(parent) = abs_out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            SicroError::Filesystem(format!(
                "cannot create exports dir: {e}"
            ))
        })?;
    }
    atomic_write_bytes(&abs_out, &buf)?;

    let rel_sidecar = rel_out.replace(
        &format!(".{ext}"),
        "_sidecar.json",
    );
    let sidecar = build_sidecar(analysis, input, &hash, width, height, &format_str);
    let sidecar_bytes = serde_json::to_vec_pretty(&sidecar)?;
    let abs_sidecar =
        resolve_workspace_relative(workspace_root, &rel_sidecar)?;
    atomic_write_bytes(&abs_sidecar, &sidecar_bytes)?;

    Ok(ExportArtifact {
        output_relative_path: rel_out,
        sidecar_relative_path: rel_sidecar,
        width,
        height,
        hash_sha256: hash,
        format: format_str,
        size_bytes,
    })
}

fn build_sidecar(
    analysis: &ImageAnalysis,
    input: &ExportImageInput,
    hash: &str,
    width: u32,
    height: u32,
    format: &str,
) -> Value {
    let summary: Value = input
        .operation_summary_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| json!({}));

    let ops: Vec<Value> = input.operations.iter().map(operation_to_value).collect();

    json!({
        "software": "SICRO Desktop",
        "software_version": APP_VERSION,
        "generated_at": Utc::now().to_rfc3339(),
        "image_analysis_id": analysis.id.to_string(),
        "occurrence_id": analysis.occurrence_id.to_string(),
        "source": {
            "kind": analysis.source_kind.as_str(),
            "source_id": analysis.source_id,
            "original_relative_path": analysis.original_relative_path,
            "original_hash_sha256": analysis.original_hash_sha256,
        },
        "derivative": {
            "format": format,
            "hash_sha256": hash,
            "width": width,
            "height": height,
        },
        "operations": ops,
        "adjustments": input.adjustments.clone().unwrap_or_default(),
        "composed_from_frontend": input.composed_png_base64.is_some(),
        "backend_adjustments_applied": input.apply_backend_adjustments,
        "summary": summary,
    })
}

fn operation_to_value(op: &BackendOperation) -> Value {
    serde_json::to_value(op).unwrap_or_else(|_| json!({"kind": "unknown"}))
}

fn sanitize_slug(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for c in title.chars() {
        let ok = match c {
            c if c.is_ascii_alphanumeric() => true,
            '-' | '_' => true,
            _ => false,
        };
        out.push(if ok { c } else { '_' });
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "analise".to_string()
    } else {
        let truncated: String = trimmed.chars().take(40).collect();
        truncated
    }
}

