//! Comandos Tauri do módulo Croqui (viário, corporal e planta). O schema do
//! `doc` vive no frontend; Rust trata o envelope como JSON opaco.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use chrono::Utc;
use uuid::Uuid;

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::{croqui_repo, occurrence_repo};
use crate::error::{Result, SicroError};
use crate::filesystem::atomic_write_bytes;
use crate::hashing::sha256::{sha256_bytes, sha256_file};
use crate::image_processing::lens_correction::{
    apply_radial_correction, coefficients_for_intensity, crop as crop_image, CropRect,
};
use crate::models::{Croqui, CroquiDoc, CroquiStatus, ExportCroquiPngInput, NewCroquiInput};
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};

const CROQUIS_SUBDIR: &str = "croquis";
const CROQUIS_EXPORT_SUBDIR: &str = "croquis/exports";
const CROQUIS_BACKGROUNDS_SUBDIR: &str = "croquis/backgrounds";
const CURRENT_SCHEMA_VERSION: &str = "0.1";

#[tauri::command]
pub async fn create_croqui(
    workspace_path: String,
    input: NewCroquiInput,
) -> Result<CroquiDoc> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let now = Utc::now();
    let id = Uuid::new_v4();

    // "corporal" = carta de lesões (.sicrocorpo); "planta" = planta baixa
    // (.sicroplanta); "viario" (default) = croqui de via (.sicrocroqui).
    let kind = match input.kind.as_deref() {
        Some("corporal") => "corporal",
        Some("planta") => "planta",
        _ => "viario",
    };
    let (relative_path, default_title) = match kind {
        "corporal" => (
            format!("{CROQUIS_SUBDIR}/corpo_{}.sicrocorpo", id),
            format!("Croqui corporal {}", &id.to_string()[..8]),
        ),
        "planta" => (
            format!("{CROQUIS_SUBDIR}/planta_{}.sicroplanta", id),
            format!("Croqui de planta {}", &id.to_string()[..8]),
        ),
        _ => (
            format!("{CROQUIS_SUBDIR}/croqui_{}.sicrocroqui", id),
            format!("Croqui {}", &id.to_string()[..8]),
        ),
    };

    let croqui = Croqui {
        id,
        occurrence_id: manifest.occurrence_id,
        title: if input.title.trim().is_empty() {
            default_title
        } else {
            input.title.trim().to_string()
        },
        relative_path: relative_path.clone(),
        status: CroquiStatus::Draft,
        schema_version: CURRENT_SCHEMA_VERSION.to_string(),
        last_export_relative_path: None,
        kind: kind.to_string(),
        created_at: now,
        updated_at: now,
    };

    croqui_repo::insert(&conn, &croqui)?;
    occurrence_repo::record_audit(
        &conn,
        Some(&croqui.occurrence_id),
        match kind {
            "corporal" => "croqui.corporal.created",
            "planta" => "croqui.planta.created",
            _ => "croqui.created",
        },
        Some("croqui"),
        Some("croqui"),
        Some(&croqui.id),
        None,
    )?;

    // Envelope vazio para read-after-create nunca falhar; o frontend preenche ao abrir.
    let envelope = match kind {
        "corporal" => empty_corpo_envelope(&croqui),
        "planta" => empty_planta_envelope(&croqui),
        _ => empty_envelope(&croqui),
    };
    let target = ws.join(&relative_path);
    write_doc(&target, &envelope)?;

    Ok(CroquiDoc {
        croqui,
        doc: envelope,
    })
}

#[tauri::command]
pub async fn list_croquis(workspace_path: String) -> Result<Vec<Croqui>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    croqui_repo::list_by_occurrence(&conn, &manifest.occurrence_id)
}

#[tauri::command]
pub async fn read_croqui(workspace_path: String, croqui_id: String) -> Result<CroquiDoc> {
    let ws = PathBuf::from(&workspace_path);
    let id = Uuid::parse_str(&croqui_id)
        .map_err(|e| SicroError::Validation(format!("invalid croqui id: {e}")))?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let croqui = croqui_repo::find_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("croqui {} not found", id)))?;

    let target = ws.join(&croqui.relative_path);
    let bytes = std::fs::read(&target).map_err(|e| {
        SicroError::Workspace(format!(
            "could not read croqui at {}: {}",
            target.display(),
            e
        ))
    })?;
    let doc: serde_json::Value = serde_json::from_slice(&bytes)?;

    Ok(CroquiDoc { croqui, doc })
}

#[tauri::command]
pub async fn save_croqui(
    workspace_path: String,
    croqui_id: String,
    doc: serde_json::Value,
) -> Result<Croqui> {
    let ws = PathBuf::from(&workspace_path);
    let id = Uuid::parse_str(&croqui_id)
        .map_err(|e| SicroError::Validation(format!("invalid croqui id: {e}")))?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let mut croqui = croqui_repo::find_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("croqui {} not found", id)))?;

    let target = ws.join(&croqui.relative_path);
    write_doc(&target, &doc)?;

    let now = Utc::now();
    croqui_repo::touch(&conn, &croqui.id, now, None, None)?;
    croqui.updated_at = now;

    occurrence_repo::record_audit(
        &conn,
        Some(&croqui.occurrence_id),
        "croqui.saved",
        Some("croqui"),
        Some("croqui"),
        Some(&croqui.id),
        None,
    )?;

    Ok(croqui)
}

/// Apaga a linha e o arquivo (NotFound é silencioso). O PNG exportado NÃO é
/// removido: continua em `croquis/exports/` como artefato pericial.
#[tauri::command]
pub async fn delete_croqui(
    workspace_path: String,
    croqui_id: String,
) -> Result<()> {
    let ws = PathBuf::from(&workspace_path);
    let id = Uuid::parse_str(&croqui_id)
        .map_err(|e| SicroError::Validation(format!("invalid croqui id: {e}")))?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let croqui = croqui_repo::find_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("croqui {} not found", id)))?;

    occurrence_repo::record_audit(
        &conn,
        Some(&croqui.occurrence_id),
        "croqui.deleted",
        Some("croqui"),
        Some("croqui"),
        Some(&croqui.id),
        Some(&croqui.relative_path),
    )?;

    croqui_repo::delete(&conn, &id)?;

    let target = ws.join(&croqui.relative_path);
    match std::fs::remove_file(&target) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(SicroError::Filesystem(format!(
            "could not delete croqui file at {}: {}",
            target.display(),
            e
        ))),
    }
}

/// Grava o PNG exportado (Konva `toDataURL()`, base64) em `croquis/exports/`
/// e atualiza a linha.
#[tauri::command]
pub async fn export_croqui_png(
    workspace_path: String,
    croqui_id: String,
    input: ExportCroquiPngInput,
) -> Result<String> {
    let ws = PathBuf::from(&workspace_path);
    let id = Uuid::parse_str(&croqui_id)
        .map_err(|e| SicroError::Validation(format!("invalid croqui id: {e}")))?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let croqui = croqui_repo::find_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("croqui {} not found", id)))?;

    // Aceita base64 cru ou com prefixo `data:image/png;base64,`.
    let cleaned = input
        .png_base64
        .split(',')
        .last()
        .unwrap_or(&input.png_base64);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cleaned.trim())
        .map_err(|e| SicroError::Validation(format!("invalid base64 PNG: {e}")))?;
    if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(SicroError::Validation(
            "payload does not start with PNG magic bytes".to_string(),
        ));
    }

    let exports_dir = ws.join(CROQUIS_EXPORT_SUBDIR);
    std::fs::create_dir_all(&exports_dir).map_err(|e| {
        SicroError::Filesystem(format!(
            "cannot create exports dir {}: {}",
            exports_dir.display(),
            e
        ))
    })?;

    let ts = Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("croqui_{}_{}.png", id, ts);
    let target = exports_dir.join(&filename);
    atomic_write_bytes(&target, &bytes)?;

    let relative_path = format!("{CROQUIS_EXPORT_SUBDIR}/{filename}");
    let now = Utc::now();
    croqui_repo::touch(
        &conn,
        &croqui.id,
        now,
        Some(&relative_path),
        Some(CroquiStatus::Ready),
    )?;

    occurrence_repo::record_audit(
        &conn,
        Some(&croqui.occurrence_id),
        "croqui.exported.png",
        Some("croqui"),
        Some("croqui"),
        Some(&croqui.id),
        Some(&relative_path),
    )?;

    Ok(relative_path)
}

// ---------------------------------------------------------------------------
// Importação de foto de drone: correção radial de lente + recorte → PNG em
// `croquis/backgrounds/` com sidecar JSON (hashes + parâmetros). O original
// nunca é alterado.

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CropRectInput {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DroneImportInput {
    /// Caminho absoluto da imagem de origem.
    pub source_absolute_path: String,
    /// 0.0..=1.0; 0 desliga a correção.
    pub intensity: f32,
    /// Recorte aplicado à SAÍDA da correção (mesmas dimensões da entrada).
    pub crop: CropRectInput,
    /// Opcionais; vão para o sidecar (auditoria).
    pub croqui_id: Option<String>,
    pub occurrence_id: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DroneImportResult {
    /// PNG corrigido + recortado, relativo ao workspace (vira fundo do croqui).
    pub output_relative_path: String,
    /// Sidecar JSON com o pipeline (reproduzível/auditável).
    pub sidecar_relative_path: String,
    pub output_width: u32,
    pub output_height: u32,
    pub output_hash_sha256: String,
}

#[tauri::command]
pub async fn import_drone_image(
    workspace_path: String,
    input: DroneImportInput,
) -> Result<DroneImportResult> {
    let ws = PathBuf::from(&workspace_path);
    let source = PathBuf::from(&input.source_absolute_path);
    if !source.exists() {
        return Err(SicroError::Validation(format!(
            "drone source file does not exist: {}",
            source.display()
        )));
    }

    // Hash do original antes de qualquer coisa: vai para o sidecar (custódia).
    let original_hash = sha256_file(&source)?;

    let dyn_img = image::open(&source).map_err(|e| {
        SicroError::Filesystem(format!(
            "failed to decode drone image {}: {}",
            source.display(),
            e
        ))
    })?;
    let coeffs = coefficients_for_intensity(input.intensity);
    let corrected = apply_radial_correction(&dyn_img, coeffs);

    let crop_rect = CropRect {
        x: input.crop.x,
        y: input.crop.y,
        width: input.crop.width,
        height: input.crop.height,
    };
    let final_img = crop_image(corrected, crop_rect).ok_or_else(|| {
        SicroError::Validation(
            "crop rectangle produced an empty image — adjust the crop and try again"
                .to_string(),
        )
    })?;
    let (out_w, out_h) = final_img.dimensions();

    // PNG em memória: grava e faz hash sem reler.
    let mut png_bytes: Vec<u8> = Vec::new();
    {
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        final_img
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| {
                SicroError::Filesystem(format!("failed to encode PNG: {e}"))
            })?;
    }

    // Mesmo timestamp no PNG e no sidecar: prefixo comum.
    let ts = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let png_filename = format!("drone_corrigido_{ts}.png");
    let sidecar_filename = format!("drone_corrigido_{ts}.sidecar.json");

    let backgrounds_dir = ws.join(CROQUIS_BACKGROUNDS_SUBDIR);
    std::fs::create_dir_all(&backgrounds_dir).map_err(|e| {
        SicroError::Filesystem(format!(
            "cannot create backgrounds dir {}: {}",
            backgrounds_dir.display(),
            e
        ))
    })?;
    let png_path = backgrounds_dir.join(&png_filename);
    atomic_write_bytes(&png_path, &png_bytes)?;

    let output_hash = sha256_bytes(&png_bytes);

    // `original_relative_path` é best-effort: só quando a origem já está dentro
    // do workspace; o caminho absoluto vai sempre.
    let original_relative = source
        .strip_prefix(&ws)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"));
    let sidecar = serde_json::json!({
        "software": "SICRO Desktop — Croqui Drone Import",
        "schema_version": "1",
        "created_at": Utc::now().to_rfc3339(),
        "original_absolute_path": source.to_string_lossy(),
        "original_relative_path": original_relative,
        "original_hash_sha256": original_hash,
        "output_relative_path": format!(
            "{CROQUIS_BACKGROUNDS_SUBDIR}/{png_filename}"
        ),
        "output_hash_sha256": output_hash,
        "output_width": out_w,
        "output_height": out_h,
        "lens_correction": {
            "enabled": !coeffs.is_identity(),
            "intensity": input.intensity,
            "k1": coeffs.k1,
            "k2": coeffs.k2,
            "k3": coeffs.k3,
        },
        "crop": {
            "x": input.crop.x,
            "y": input.crop.y,
            "width": input.crop.width,
            "height": input.crop.height,
        },
        "croqui_id": input.croqui_id,
        "occurrence_id": input.occurrence_id,
    });
    let sidecar_path = backgrounds_dir.join(&sidecar_filename);
    let sidecar_bytes = serde_json::to_vec_pretty(&sidecar)?;
    atomic_write_bytes(&sidecar_path, &sidecar_bytes)?;

    if let Some(occ_id) = &input.occurrence_id {
        if let Ok(occ_uuid) = Uuid::parse_str(occ_id) {
            let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
            occurrence_repo::record_audit(
                &conn,
                Some(&occ_uuid),
                "croqui.background.drone_imported",
                Some("croqui"),
                Some("background"),
                None,
                Some(&png_filename),
            )?;
        }
    }

    Ok(DroneImportResult {
        output_relative_path: format!(
            "{CROQUIS_BACKGROUNDS_SUBDIR}/{png_filename}"
        ),
        sidecar_relative_path: format!(
            "{CROQUIS_BACKGROUNDS_SUBDIR}/{sidecar_filename}"
        ),
        output_width: out_w,
        output_height: out_h,
        output_hash_sha256: output_hash,
    })
}

// ---------------------------------------------------------------------------
// Helpers

fn write_doc(target: &Path, doc: &serde_json::Value) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(doc)?;
    atomic_write_bytes(target, &bytes)?;
    Ok(())
}

/// Envelope `.sicrocroqui` vazio que o engine do frontend abre direto; o
/// serializer de lá sobrescreve no primeiro save.
fn empty_envelope(c: &Croqui) -> serde_json::Value {
    serde_json::json!({
        "schema_version": c.schema_version,
        "croqui_id": c.id.to_string(),
        "occurrence_id": c.occurrence_id.to_string(),
        "title": c.title,
        "created_at": c.created_at.to_rfc3339(),
        "updated_at": c.updated_at.to_rfc3339(),
        "canvas": {
            "width_px": 1600,
            "height_px": 1000,
            "background_color": "#ffffff",
            "grid": { "enabled": true, "size_px": 50 }
        },
        "scale": null,
        "background_image": null,
        "layers": [
            { "id": "layer_background", "name": "Imagem de fundo", "visible": true, "locked": true, "kind": "background" },
            { "id": "layer_objects", "name": "Objetos", "visible": true, "locked": false, "kind": "objects" }
        ],
        "objects": []
    })
}

/// Envelope `.sicrocorpo` vazio: campos mínimos de `coerceCorpoDoc`; o resto
/// é preenchido no frontend.
fn empty_corpo_envelope(c: &Croqui) -> serde_json::Value {
    serde_json::json!({
        "schema_version": c.schema_version,
        "corpo_id": c.id.to_string(),
        "occurrence_id": c.occurrence_id.to_string(),
        "title": c.title,
        "created_at": c.created_at.to_rfc3339(),
        "updated_at": c.updated_at.to_rfc3339(),
        "template_id": "corpo_completo",
        "canvas": { "width_px": 1040, "height_px": 700 },
        "markers": []
    })
}

/// Envelope `.sicroplanta` vazio: campos mínimos de `coercePlantaDoc`;
/// `floorplan` e marcadores são preenchidos no frontend.
fn empty_planta_envelope(c: &Croqui) -> serde_json::Value {
    serde_json::json!({
        "schema_version": c.schema_version,
        "planta_id": c.id.to_string(),
        "occurrence_id": c.occurrence_id.to_string(),
        "title": c.title,
        "created_at": c.created_at.to_rfc3339(),
        "updated_at": c.updated_at.to_rfc3339(),
        "px_per_m": 100,
        "floorplan": { "floors": [], "furnitureId": 0, "wallNodeId": 0 },
        "evidences": [],
        "compass_deg": 0
    })
}
