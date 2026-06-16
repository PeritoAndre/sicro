//! Import orchestrator — `run_import` is the single public entry point.
//!
//! Side effects in order (each step is logged + best-effort audit):
//!   1. SHA-256 of the source `.sicroapp`.
//!   2. Open the ZIP, parse the manifest.
//!   3. Look for an existing import with the same package hash.
//!   4. Create the destination `.sicro` workspace.
//!   5. Copy the package to `imports/<id>/original_package.sicroapp`.
//!   6. Verify `hashes.json` against the staged ZIP.
//!   7. Read the structured JSONs (case/metadata/location/photos).
//!   8. Build a Desktop `Occurrence` (preserving raw payloads).
//!   9. Extract photos to `media/photos/`; insert `media_assets` +
//!      `evidence_items`.
//!  10. Persist the `Import` row + the rendered `import_report.json`.
//!  11. Return `ImportResult { import, occurrence, workspace_path, report }`.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::{
    evidence_item_repo, import_repo, media_asset_repo, occurrence_repo,
};
use crate::error::{Result, SicroError};
use crate::filesystem::sanitize_folder_name;
use crate::models::{
    EvidenceItem, HashMismatch, Import, ImportReport, ImportResult, ImportSicroappInput,
    ImportStatus, MediaAsset, MediaAssetType, Occurrence, OccurrenceStatus,
};
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};
use crate::workspace::paths::unique_workspace_path;

use super::manifest_parser::{self, ParsedManifest};
use super::package_reader::{
    package_sha256, parse_hashes_json, stage_package, PackageReader,
};
use super::registry::{GlobalImportRecord, ImportRegistry};

/// Top-level folders that the imported workspace must have on top of the
/// usual `.sicro` skeleton. Keeping them here (instead of in
/// `workspace::create::SUBDIRS`) makes it clear they're "owned" by the
/// importer and stops Spike A workspaces from creating empty folders.
const IMPORT_SUBDIRS: &[&str] = &[
    "dossie",
    "laudos",
    "laudos/assets",
    "imports",
    "media",
    "media/photos",
    "logs",
];

/// Files the importer ALWAYS attempts to read. Anything else found in the
/// ZIP is reported under `files_ignored` (not an error — just noise).
const KNOWN_JSONS: &[&str] = &[
    "manifest.json",
    "metadados.json",
    "caso.json",
    "localizacao.json",
    "gps_leituras.json",
    "estatisticas.json",
    "timeline.json",
    "checklist.json",
    "fotos.json",
    "veiculos.json",
    "vitimas.json",
    "vestigios.json",
    "medicoes.json",
    "observacoes.json",
    "operacional.json",
    "hashes.json",
];

pub fn run_import(
    input: ImportSicroappInput,
    default_parent: &Path,
    registry: &ImportRegistry,
) -> Result<ImportResult> {
    let package_path = PathBuf::from(&input.package_path);
    let original_filename = package_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string);

    // 1. Validate extension + presence on disk.
    validate_extension(&package_path)?;
    if !package_path.is_file() {
        return Err(SicroError::Filesystem(format!(
            "package not found: {}",
            package_path.display()
        )));
    }

    // 2. Hash the source file before staging — so callers can compare even
    //    if the staging copy fails later.
    let pkg_sha256 = package_sha256(&package_path)?;
    let pkg_size = fs::metadata(&package_path).map(|m| m.len()).unwrap_or(0);

    // 2.1 Cross-workspace duplicate check. Importing the same `.sicroapp`
    //     twice would create two divergent workspaces from the same source,
    //     which the doc §8 explicitly forbids ("não duplicar silenciosamente").
    if let Some(existing) = registry.find_by_sha256(&pkg_sha256)? {
        return Err(SicroError::Validation(format!(
            "package already imported on {} (workspace {}, import_id {})",
            existing.imported_at, existing.workspace_path, existing.import_id,
        )));
    }

    // 3. Open the ZIP and parse the manifest.
    let mut probe_reader = PackageReader::open(&package_path)?;
    let raw_manifest = probe_reader
        .read_to_bytes("manifest.json")?
        .ok_or_else(|| {
            SicroError::Validation("package missing manifest.json".to_string())
        })?;
    let manifest = manifest_parser::parse(&raw_manifest)?;
    drop(probe_reader); // re-open after staging

    // 4. Resolve where the destination workspace will live.
    let parent: PathBuf = match &input.parent_directory {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => default_parent.to_path_buf(),
    };
    if !parent.exists() {
        fs::create_dir_all(&parent).map_err(|e| {
            SicroError::Filesystem(format!(
                "cannot create parent {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    let workspace_id = Uuid::new_v4();
    let occurrence_id = workspace_id; // 1 workspace = 1 occurrence (Spike A convention).
    let base_name = pick_workspace_folder_name(&manifest, &workspace_id);
    let workspace_path = unique_workspace_path(&parent, &base_name)?;

    // 5. Create the directory tree + SQLite.
    fs::create_dir_all(&workspace_path)?;
    for sub in IMPORT_SUBDIRS {
        fs::create_dir_all(workspace_path.join(sub))?;
    }
    let db_path = workspace_path.join(SQLITE_FILENAME);
    let mut conn = open_connection(&db_path)?;
    run_migrations(&mut conn)?;

    // 6. Defensive within-workspace duplicate check. The cross-workspace
    //    check at step 2.1 catches the common case; this guards against a
    //    user manually pointing two imports at the same fresh workspace
    //    via `parent_directory`.
    if let Some(existing) = import_repo::find_by_package_sha256(&conn, &pkg_sha256)? {
        drop(conn);
        let _ = fs::remove_dir_all(&workspace_path);
        return Err(SicroError::Validation(format!(
            "package already imported as {} on {}",
            existing.id, existing.imported_at
        )));
    }

    // 7. Stage the original package under imports/<id>/.
    let import_id = Uuid::new_v4();
    let import_dir = workspace_path
        .join("imports")
        .join(import_id.to_string());
    fs::create_dir_all(&import_dir)?;
    let staged_pkg = import_dir.join("original_package.sicroapp");
    stage_package(&package_path, &staged_pkg)?;

    let now: DateTime<Utc> = Utc::now();
    let mut report = ImportReport {
        package_original_filename: original_filename.clone(),
        package_sha256: Some(pkg_sha256.clone()),
        package_size_bytes: pkg_size,
        format: Some(manifest.format.clone()),
        schema_version: Some(manifest.schema_version.clone()),
        app_name: manifest.app_name.clone(),
        app_version: manifest.app_version.clone(),
        mobile_occurrence_id: manifest.occurrence_id.clone(),
        generated_at: manifest.exported_at.clone(),
        exported_at: manifest.exported_at.clone(),
        tipo_pericia: manifest.tipo_pericia.clone(),
        natureza: manifest.natureza.clone(),
        resultado: manifest.resultado.clone(),
        manifest_counts: manifest.counts.clone(),
        imported_at: Some(now),
        ..Default::default()
    };
    // Carry the manifest's own warnings into our report.
    report.warnings.extend(manifest.manifest_warnings.iter().cloned());

    // 8. Insert the `imports` row FIRST so subsequent `occurrences` and
    //    `media_assets` foreign-key references are satisfied.  warnings/
    //    errors/status get patched at the very end via
    //    `update_status_and_warnings`.
    let initial_import = Import {
        id: import_id,
        package_relative_path: format!("imports/{}/original_package.sicroapp", import_id),
        original_filename: original_filename.clone(),
        package_sha256: pkg_sha256.clone(),
        format: manifest.format.clone(),
        schema_version: manifest.schema_version.clone(),
        app_name: manifest.app_name.clone(),
        app_version: manifest.app_version.clone(),
        mobile_occurrence_id: manifest.occurrence_id.clone(),
        // Placeholder — finalised below.
        status: ImportStatus::Imported,
        warnings_json: "[]".to_string(),
        errors_json: "[]".to_string(),
        raw_manifest_json: manifest.raw_json.clone(),
        imported_at: now,
    };
    import_repo::insert(&conn, &initial_import)?;

    // 9. Re-open against the staged package (so further extraction reads
    //    from imports/<id>/original_package.sicroapp, not the user path).
    let mut reader = PackageReader::open(&staged_pkg)?;

    // Catalogue every file in the ZIP into either `jsons_read`, `files_ignored`,
    // and synthesize `jsons_missing` from KNOWN_JSONS.
    classify_zip_entries(&mut report, &reader);

    // 10. Verify hashes (best effort).
    if let Some(hash_bytes) = reader.read_to_bytes("hashes.json")? {
        report.hashes_present = true;
        match parse_hashes_json(&hash_bytes) {
            Ok(pairs) => verify_hashes(&mut reader, &pairs, &mut report),
            Err(e) => report.warnings.push(format!("hashes.json invalid: {e}")),
        }
    } else {
        report
            .warnings
            .push("hashes.json missing — integrity check skipped".to_string());
    }

    // 11. Read structured JSONs.
    let case_json = reader.read_to_bytes("caso.json")?;
    let metadata_json = reader.read_to_bytes("metadados.json")?;
    let location_json = reader.read_to_bytes("localizacao.json")?;
    let photos_json = reader.read_to_bytes("fotos.json")?;

    let caso = case_json
        .as_ref()
        .and_then(|b| serde_json::from_slice::<Value>(b).ok());
    let metadados = metadata_json
        .as_ref()
        .and_then(|b| serde_json::from_slice::<Value>(b).ok());
    let localizacao = location_json
        .as_ref()
        .and_then(|b| serde_json::from_slice::<Value>(b).ok());

    // 11. Build the Desktop Occurrence.
    let occurrence = build_occurrence(
        occurrence_id,
        import_id,
        &manifest,
        caso.as_ref(),
        metadados.as_ref(),
        localizacao.as_ref(),
        case_json.as_ref(),
        metadata_json.as_ref(),
        location_json.as_ref(),
        now,
    );
    occurrence_repo::insert(&conn, &occurrence)?;
    occurrence_repo::record_audit(
        &conn,
        Some(&occurrence.id),
        "occurrence.imported",
        Some("importer"),
        Some("occurrence"),
        Some(&occurrence.id),
        Some(&pkg_sha256),
    )?;

    // Mirror the occurrence summary into the report.
    report.bo = occurrence.numero_bo.clone();
    report.protocolo = occurrence.protocolo.clone();
    report.municipio = occurrence.municipio.clone();
    report.bairro = occurrence.bairro.clone();
    report.logradouro = occurrence.logradouro.clone();
    report.tipo_pericia = occurrence
        .tipo_pericia
        .clone()
        .or(report.tipo_pericia.take());
    report.natureza = occurrence.natureza.clone().or(report.natureza.take());
    report.resultado = occurrence.resultado.clone().or(report.resultado.take());

    // 12. Import photos.
    let photo_index = photos_json
        .as_ref()
        .and_then(|b| serde_json::from_slice::<Value>(b).ok())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();

    import_photos(
        &mut reader,
        &photo_index,
        &workspace_path,
        &conn,
        occurrence_id,
        import_id,
        now,
        &mut report,
    )?;

    // 13.5 MVP 3 — populate the Dossiê tables (checklist, entities, traces,
    //      measurements, notes, timeline, stats) from the remaining JSONs.
    //      Best-effort: an error here is reported but doesn't abort the
    //      import (the pacote already produced a valid Occurrence + media).
    match super::rehydrator::load_from_reader(&conn, occurrence_id, import_id, &mut reader) {
        Ok(counts) => {
            tracing::info!(
                "dossiê populated: checklist={} entities={} traces={} measurements={} notes={} timeline={} stats={}",
                counts.checklist,
                counts.entities,
                counts.traces,
                counts.measurements,
                counts.notes,
                counts.timeline,
                counts.stats_loaded,
            );
            report.warnings.extend(counts.warnings);
        }
        Err(e) => {
            report.warnings.push(format!(
                "dossiê population failed (non-fatal): {e}"
            ));
        }
    }

    // 14. Decide final status.
    let status = if report.errors.is_empty() && report.warnings.is_empty() {
        ImportStatus::Imported
    } else if report.errors.is_empty() {
        ImportStatus::ImportedWithWarnings
    } else {
        // Errors so far are non-fatal (we'd have bubbled them up). Mark with warnings.
        ImportStatus::ImportedWithWarnings
    };
    report.status = Some(status);

    // 15. Finalise the Import row in place.
    let warnings_json = serde_json::to_string(&report.warnings).unwrap_or_else(|_| "[]".into());
    let errors_json = serde_json::to_string(&report.errors).unwrap_or_else(|_| "[]".into());
    import_repo::update_status_and_warnings(
        &conn,
        &import_id,
        status,
        &warnings_json,
        &errors_json,
    )?;
    let import = Import {
        warnings_json: warnings_json.clone(),
        errors_json: errors_json.clone(),
        status,
        ..initial_import
    };

    // 16. Write the workspace manifest.json so the workspace is openable by
    //     the Spike A `open_occurrence` command without a special case.
    let mut ws_manifest = Manifest::new(workspace_id, occurrence_id);
    ws_manifest.touch();
    ws_manifest.write(&workspace_path)?;

    // 16. Fill in the workspace_path + import_id on the report and persist
    //     it to disk.
    let workspace_path_str = workspace_path
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| SicroError::Filesystem(format!("non-UTF8 path {}", workspace_path.display())))?;
    report.workspace_path = Some(workspace_path_str.clone());
    report.import_id = Some(import_id);
    report.occurrence_id = Some(occurrence_id);

    let report_path = import_dir.join("import_report.json");
    let report_json = serde_json::to_vec_pretty(&report)?;
    crate::filesystem::atomic_write_bytes(&report_path, &report_json)?;

    // Record in the global cross-workspace registry so the same package
    // can never be imported twice (even into a different parent_directory).
    registry.record(GlobalImportRecord {
        package_sha256: import.package_sha256.clone(),
        workspace_id,
        workspace_path: workspace_path_str.clone(),
        import_id,
        original_filename: original_filename.clone(),
        mobile_occurrence_id: manifest.occurrence_id.clone(),
        imported_at: now,
    })?;

    drop(reader);
    drop(conn);

    Ok(ImportResult {
        import,
        occurrence,
        workspace_path: workspace_path_str,
        report,
    })
}

// ===========================================================================
// Helpers

fn validate_extension(path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "sicroapp" && ext != "sicrocampo" {
        return Err(SicroError::Validation(format!(
            "expected a .sicroapp file, got {:?} (extension {:?})",
            path.display(),
            ext
        )));
    }
    Ok(())
}

fn pick_workspace_folder_name(manifest: &ParsedManifest, workspace_id: &Uuid) -> String {
    let short_id = &workspace_id.to_string()[..8];
    let mut parts: Vec<String> = Vec::new();
    if let Some(tipo) = &manifest.tipo_pericia {
        parts.push(format!("import_{tipo}"));
    } else {
        parts.push("import".to_string());
    }
    if let Some(mob_id) = &manifest.occurrence_id {
        // mobile IDs look like "occ_<ms>" — keep the last 6 chars as a hint.
        let tail = mob_id
            .chars()
            .rev()
            .take(6)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        if !tail.is_empty() {
            parts.push(tail);
        }
    }
    parts.push(short_id.to_string());
    sanitize_folder_name(&parts.join("_"))
}

fn classify_zip_entries(report: &mut ImportReport, reader: &PackageReader) {
    use std::collections::HashSet;
    let mut known_seen: HashSet<&'static str> = HashSet::new();
    for entry in reader.list_files() {
        if entry.starts_with("fotos/") {
            // Counted under photos_*; nothing to do here.
            continue;
        }
        match KNOWN_JSONS.iter().find(|k| **k == entry.as_str()) {
            Some(found) => {
                known_seen.insert(found);
                report.jsons_read.push(entry.clone());
            }
            None => {
                report.files_ignored.push(entry.clone());
            }
        }
    }
    for k in KNOWN_JSONS {
        if !known_seen.contains(k) {
            // hashes.json is the only one whose absence is its own warning
            // (handled in the orchestrator). Track the rest here.
            if *k == "hashes.json" {
                continue;
            }
            report.jsons_missing.push((*k).to_string());
        }
    }
}

fn verify_hashes(
    reader: &mut PackageReader,
    pairs: &[(String, String)],
    report: &mut ImportReport,
) {
    for (path, expected) in pairs {
        if !reader.contains(path) {
            report.files_missing_from_hashes.push(path.clone());
            continue;
        }
        match reader.sha256(path) {
            Ok(actual) => {
                if actual.eq_ignore_ascii_case(expected) {
                    report.hashes_verified_ok += 1;
                } else {
                    report.hashes_mismatched.push(HashMismatch {
                        path: path.clone(),
                        expected: expected.clone(),
                        actual,
                    });
                }
            }
            Err(e) => {
                report.warnings.push(format!(
                    "could not hash {path:?} for verification: {e}"
                ));
            }
        }
    }
    if !report.hashes_mismatched.is_empty() {
        report.warnings.push(format!(
            "{} hash mismatch(es) detected — see hashes_mismatched",
            report.hashes_mismatched.len()
        ));
    }
    if !report.files_missing_from_hashes.is_empty() {
        report.warnings.push(format!(
            "{} file(s) listed in hashes.json missing from ZIP",
            report.files_missing_from_hashes.len()
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn build_occurrence(
    id: Uuid,
    import_id: Uuid,
    manifest: &ParsedManifest,
    caso: Option<&Value>,
    metadados: Option<&Value>,
    localizacao: Option<&Value>,
    raw_case: Option<&Vec<u8>>,
    raw_metadata: Option<&Vec<u8>>,
    raw_location: Option<&Vec<u8>>,
    now: DateTime<Utc>,
) -> Occurrence {
    let bo = caso.and_then(|v| v.get("bo")).and_then(Value::as_str).map(str::to_string);
    let protocolo = caso
        .and_then(|v| v.get("protocolo"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let requisicao = caso
        .and_then(|v| v.get("requisicao"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let delegacia = caso
        .and_then(|v| v.get("delegacia"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let municipio = caso
        .and_then(|v| v.get("municipio"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let bairro = caso
        .and_then(|v| v.get("bairro"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let logradouro = caso
        .and_then(|v| v.get("logradouro"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let referencia = caso
        .and_then(|v| v.get("referencia"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let data_acionamento = caso
        .and_then(|v| v.get("acionamento_em"))
        .and_then(Value::as_str)
        .and_then(parse_iso8601);
    let data_chegada = caso
        .and_then(|v| v.get("chegada_em"))
        .and_then(Value::as_str)
        .and_then(parse_iso8601);
    let data_encerramento = caso
        .and_then(|v| v.get("encerramento_em"))
        .and_then(Value::as_str)
        .and_then(parse_iso8601);

    let peritos = caso
        .and_then(|v| v.get("peritos"))
        .and_then(Value::as_str)
        .map(|s| split_peritos(s))
        .unwrap_or_default();

    let tipo_pericia = metadados
        .and_then(|v| v.get("tipo_pericia"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| manifest.tipo_pericia.clone());
    let natureza = metadados
        .and_then(|v| v.get("natureza"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| manifest.natureza.clone());
    let resultado = metadados
        .and_then(|v| v.get("resultado"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| manifest.resultado.clone());

    let latitude = localizacao
        .and_then(|v| v.get("latitude"))
        .and_then(Value::as_f64);
    let longitude = localizacao
        .and_then(|v| v.get("longitude"))
        .and_then(Value::as_f64);
    let accuracy = localizacao
        .and_then(|v| v.get("precisao_m"))
        .and_then(Value::as_f64);

    Occurrence {
        id,
        numero_bo: bo,
        protocolo,
        requisicao,
        oficio: None,
        delegacia,
        tipo_pericia,
        natureza,
        municipio,
        bairro,
        logradouro,
        referencia,
        latitude,
        longitude,
        data_fato: None,
        data_acionamento,
        data_chegada,
        data_encerramento,
        peritos,
        status: OccurrenceStatus::EmAndamento,
        created_at: now,
        updated_at: now,
        import_id: Some(import_id),
        original_mobile_id: manifest.occurrence_id.clone(),
        primary_accuracy_m: accuracy,
        resultado,
        raw_case_json: raw_case.map(|b| String::from_utf8_lossy(b).to_string()),
        raw_metadata_json: raw_metadata.map(|b| String::from_utf8_lossy(b).to_string()),
        raw_location_json: raw_location.map(|b| String::from_utf8_lossy(b).to_string()),
    }
}

fn parse_iso8601(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .or_else(|| {
            // mobile sometimes omits the timezone offset (Dart toIso8601String).
            // Treat as UTC for now — Spike D doesn't model timezones beyond
            // ISO-8601-ish parsing.
            DateTime::parse_from_str(&format!("{s}Z"), "%Y-%m-%dT%H:%M:%S%.f%#z").ok()
        })
        .map(|d| d.with_timezone(&Utc))
}

fn split_peritos(s: &str) -> Vec<String> {
    s.split(|c| c == ',' || c == ';' || c == '\n')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn import_photos(
    reader: &mut PackageReader,
    photo_index: &[Value],
    workspace_path: &Path,
    conn: &rusqlite::Connection,
    occurrence_id: Uuid,
    import_id: Uuid,
    now: DateTime<Utc>,
    report: &mut ImportReport,
) -> Result<()> {
    let media_dir = workspace_path.join("media").join("photos");
    fs::create_dir_all(&media_dir)?;

    report.photos_declared = photo_index.len() as u32;

    for entry in photo_index {
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let pkg_path = entry
            .get("arquivo")
            .and_then(Value::as_str)
            .unwrap_or("");
        let categoria = entry
            .get("categoria")
            .and_then(Value::as_str)
            .map(str::to_string);
        let caption = entry
            .get("legenda")
            .and_then(Value::as_str)
            .map(str::to_string);
        let mobile_sha = entry
            .get("sha256")
            .and_then(Value::as_str)
            .map(str::to_string);
        let captured_at = entry
            .get("capturada_em")
            .and_then(Value::as_str)
            .and_then(parse_iso8601);
        let raw_json = serde_json::to_string(entry).unwrap_or_else(|_| "{}".to_string());

        let arquivo_disponivel = entry
            .get("arquivo_disponivel")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        if pkg_path.is_empty() {
            report.warnings.push(format!(
                "foto '{id}' has empty 'arquivo' field — skipped"
            ));
            report.photos_missing += 1;
            continue;
        }

        // sanitize the in-zip path against traversal.
        let sanitised = match super::safe_zip::sanitize_zip_path(pkg_path) {
            Ok(p) => p.to_str().unwrap_or_default().replace('\\', "/"),
            Err(e) => {
                report.warnings.push(format!(
                    "foto '{id}' has unsafe arquivo {pkg_path:?}: {e}"
                ));
                report.photos_missing += 1;
                continue;
            }
        };

        if !arquivo_disponivel {
            report.warnings.push(format!(
                "foto '{id}' marked arquivo_disponivel=false — metadata only"
            ));
            report.photos_missing += 1;
            continue;
        }

        if !reader.contains(&sanitised) {
            report
                .warnings
                .push(format!("foto '{id}' declared but {sanitised:?} not in ZIP"));
            report.photos_missing += 1;
            continue;
        }

        // Target filename uses the original_id when available, else a UUID.
        let target_filename = controlled_filename(&id, &sanitised);
        let target_path = media_dir.join(&target_filename);
        let (size, actual_sha) = match reader.extract_to(&sanitised, &target_path) {
            Ok(t) => t,
            Err(e) => {
                report
                    .warnings
                    .push(format!("foto '{id}' extraction failed: {e}"));
                continue;
            }
        };

        // If mobile published a SHA, compare it against the one we just
        // computed — divergence here is an actual integrity flag (separate
        // from hashes.json which is at the ZIP level).
        if let Some(mob_sha) = mobile_sha.as_deref() {
            if !mob_sha.eq_ignore_ascii_case(&actual_sha) {
                report.warnings.push(format!(
                    "foto '{id}' sha256 mismatch: mobile={mob_sha} actual={actual_sha}"
                ));
            }
        }

        let mime = guess_mime_from_path(&sanitised);
        let asset = MediaAsset {
            id: Uuid::new_v4(),
            import_id,
            occurrence_id,
            original_id: if id.is_empty() { None } else { Some(id.clone()) },
            r#type: MediaAssetType::Photo,
            relative_path: format!("media/photos/{target_filename}"),
            original_package_path: Some(sanitised.clone()),
            original_filename: PathBuf::from(&sanitised)
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_string),
            mime_type: mime,
            size_bytes: size,
            sha256: Some(actual_sha),
            captured_at,
            imported_at: now,
            category: categoria.clone(),
            caption: caption.clone(),
            raw_json,
        };
        media_asset_repo::insert(conn, &asset)?;

        let evidence = EvidenceItem {
            id: Uuid::new_v4(),
            occurrence_id,
            media_asset_id: Some(asset.id),
            r#type: "photo".to_string(),
            title: caption.clone(),
            description: None,
            source_module: Some("photos".to_string()),
            captured_at,
            metadata_json: serde_json::json!({
                "categoria": categoria,
                "original_id": id,
            })
            .to_string(),
            created_at: now,
        };
        evidence_item_repo::insert(conn, &evidence)?;

        report.photos_imported += 1;
    }

    if report.photos_declared == 0 {
        report
            .warnings
            .push("fotos.json missing or empty — no photos imported".to_string());
    }

    Ok(())
}

/// Pick a filename inside `media/photos/` that:
///   - keeps the mobile `id` as the stem when present (so the relation in
///     `media_assets.original_id` is grep-able on disk);
///   - uses the original extension (.jpg / .png / .webp);
///   - falls back to a UUID when the mobile id is empty.
fn controlled_filename(mobile_id: &str, source_path: &str) -> String {
    let ext = PathBuf::from(source_path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "bin".to_string());
    let stem = if mobile_id.trim().is_empty() {
        Uuid::new_v4().to_string()
    } else {
        sanitize_folder_name(mobile_id)
    };
    format!("{stem}.{ext}")
}

fn guess_mime_from_path(path: &str) -> Option<String> {
    let ext = PathBuf::from(path)
        .extension()
        .and_then(|s| s.to_str())?
        .to_ascii_lowercase();
    Some(
        match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "heic" => "image/heic",
            _ => return None,
        }
        .to_string(),
    )
}
