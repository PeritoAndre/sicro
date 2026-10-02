//! Repopula as tabelas do Dossiê a partir do pacote copiado em
//! `imports/<id>/original_package.sicroapp` do import mais recente.

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

/// `outcome.rehydrated == false` = nada feito (sem imports ou sem pacote
/// copiado); não é erro.
pub fn rehydrate_workspace(
    workspace_path: &Path,
    conn: &Connection,
) -> Result<RehydrateOutcome> {
    let manifest = Manifest::read(workspace_path)?;
    let occurrence_id = manifest.occurrence_id;

    // `list_all` vem em ordem decrescente: o primeiro é o mais recente.
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

/// Mesmo caminho usado pelo orchestrator no primeiro import.
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
