//! Rehydrate a workspace's Dossiê tables from the staged `.sicroapp`.
//!
//! Used in two scenarios:
//!   1. A workspace was created by the Spike D importer (no MVP 3 tables);
//!      opening the Dossiê triggers `rehydrate_workspace` automatically.
//!   2. The user clicks "Recarregar dados do pacote" in the dossiê UI.
//!
//! Both paths share this function. It reads `imports/<id>/original_package.sicroapp`
//! of the most-recent successful import, parses the relevant JSONs, and
//! calls `dossie_mapper::persist_all` to (re)populate every table.

use std::path::Path;

use rusqlite::Connection;
use serde_json::Value;
use uuid::Uuid;

use crate::database::repositories::import_repo;
use crate::error::{Result, SicroError};
use crate::models::RehydrateOutcome;
use crate::workspace::manifest::Manifest;

use super::dossie_mapper::{self, DossieLoadCounts};
use super::package_reader::PackageReader;

/// Re-extract the Dossiê tables for the given workspace.
///
/// Returns `Ok(outcome)` even if the workspace has no imports — the caller
/// can decide whether absence is a problem. `outcome.rehydrated == false`
/// means "nothing was done" (no imports / no staged package).
pub fn rehydrate_workspace(
    workspace_path: &Path,
    conn: &Connection,
) -> Result<RehydrateOutcome> {
    let manifest = Manifest::read(workspace_path)?;
    let occurrence_id = manifest.occurrence_id;

    // Pick the most recent successful import. `list_all` is ordered DESC.
    let imports = import_repo::list_all(conn)?;
    let import = match imports.into_iter().next() {
        Some(i) => i,
        None => return Ok(RehydrateOutcome::default()),
    };

    let staged_pkg = workspace_path
        .join("imports")
        .join(import.id.to_string())
        .join("original_package.sicroapp");
    if !staged_pkg.is_file() {
        return Ok(RehydrateOutcome {
            rehydrated: false,
            warnings: vec![format!(
                "staged package not found at {}",
                staged_pkg.display()
            )],
            ..Default::default()
        });
    }

    let mut reader = PackageReader::open(&staged_pkg)?;
    let counts = load_from_reader(conn, occurrence_id, import.id, &mut reader)?;

    Ok(RehydrateOutcome {
        rehydrated: true,
        from_package_path: staged_pkg
            .to_str()
            .map(|s| s.to_string())
            .or_else(|| Some(staged_pkg.display().to_string())),
        checklist_loaded: counts.checklist,
        entities_loaded: counts.entities,
        traces_loaded: counts.traces,
        measurements_loaded: counts.measurements,
        notes_loaded: counts.notes,
        timeline_loaded: counts.timeline,
        stats_loaded: counts.stats_loaded,
        warnings: counts.warnings,
    })
}

/// Same logic used by the orchestrator at first-import time. Pulled into
/// its own helper so the orchestrator stays small.
pub fn load_from_reader(
    conn: &Connection,
    occurrence_id: Uuid,
    import_id: Uuid,
    reader: &mut PackageReader,
) -> Result<DossieLoadCounts> {
    let checklist = read_json(reader, "checklist.json")?;
    let veiculos = read_json(reader, "veiculos.json")?;
    let vitimas = read_json(reader, "vitimas.json")?;
    let vestigios = read_json(reader, "vestigios.json")?;
    let medicoes = read_json(reader, "medicoes.json")?;
    let observacoes = read_json(reader, "observacoes.json")?;
    let timeline = read_json(reader, "timeline.json")?;
    let estatisticas = read_json(reader, "estatisticas.json")?;

    dossie_mapper::persist_all(
        conn,
        occurrence_id,
        import_id,
        checklist.as_ref(),
        veiculos.as_ref(),
        vitimas.as_ref(),
        vestigios.as_ref(),
        medicoes.as_ref(),
        observacoes.as_ref(),
        timeline.as_ref(),
        estatisticas.as_ref(),
    )
}

fn read_json(reader: &mut PackageReader, name: &str) -> Result<Option<Value>> {
    let bytes = match reader.read_to_bytes(name)? {
        Some(b) => b,
        None => return Ok(None),
    };
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| {
        SicroError::Validation(format!("{name} invalid JSON: {e}"))
    })?;
    Ok(Some(value))
}
