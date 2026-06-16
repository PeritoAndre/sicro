//! Tauri commands for the Dossiê Operacional module (MVP 3).
//!
//! All commands take `workspace_path` and resolve the active `occurrence_id`
//! through `workspace::open_workspace`. The frontend never has to know about
//! occurrence UUIDs to query the dossier.

use std::path::PathBuf;

use crate::database::connection::open_connection;
use crate::database::repositories::{
    dossie_repo, import_repo, media_asset_repo,
};
use crate::error::Result;
use crate::importer::rehydrate_workspace;
use crate::models::{
    ChecklistItem, DossieCounts, DossieSummary, Entity, FieldNote, Measurement, MediaAsset,
    OccurrenceStats, RehydrateOutcome, TimelineEvent, Trace,
};
use crate::workspace::manifest::SQLITE_FILENAME;
use crate::workspace::open_workspace;

#[tauri::command]
pub async fn get_dossie_summary(workspace_path: String) -> Result<DossieSummary> {
    let ws = PathBuf::from(&workspace_path);
    let opened = open_workspace(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;

    let imports = import_repo::list_all(&conn)?;
    let latest_import = imports.into_iter().next();
    let stats = dossie_repo::find_stats(&conn, &opened.occurrence.id)?;

    let photos = media_asset_repo::list_by_occurrence(&conn, &opened.occurrence.id)?;
    let entities = dossie_repo::list_entities(&conn, &opened.occurrence.id)?;
    let traces = dossie_repo::list_traces(&conn, &opened.occurrence.id)?;
    let measurements = dossie_repo::list_measurements(&conn, &opened.occurrence.id)?;
    let notes = dossie_repo::list_field_notes(&conn, &opened.occurrence.id)?;
    let timeline = dossie_repo::list_timeline(&conn, &opened.occurrence.id)?;
    let checklist = dossie_repo::list_checklist(&conn, &opened.occurrence.id)?;

    let vehicles = entities.iter().filter(|e| e.r#type == "vehicle").count() as u32;
    let victims = entities.iter().filter(|e| e.r#type == "victim").count() as u32;

    let counts = DossieCounts {
        photos: photos.len() as u32,
        vehicles,
        victims,
        traces: traces.len() as u32,
        measurements: measurements.len() as u32,
        notes: notes.len() as u32,
        timeline: timeline.len() as u32,
        checklist: dossie_repo::summarise_checklist(&checklist),
    };

    Ok(DossieSummary {
        occurrence: opened.occurrence,
        latest_import,
        stats,
        counts,
    })
}

#[tauri::command]
pub async fn list_dossie_photos(workspace_path: String) -> Result<Vec<MediaAsset>> {
    let ws = PathBuf::from(&workspace_path);
    let opened = open_workspace(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    media_asset_repo::list_by_occurrence(&conn, &opened.occurrence.id)
}

#[tauri::command]
pub async fn list_dossie_checklist(workspace_path: String) -> Result<Vec<ChecklistItem>> {
    let ws = PathBuf::from(&workspace_path);
    let opened = open_workspace(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    dossie_repo::list_checklist(&conn, &opened.occurrence.id)
}

#[tauri::command]
pub async fn list_dossie_entities(workspace_path: String) -> Result<Vec<Entity>> {
    let ws = PathBuf::from(&workspace_path);
    let opened = open_workspace(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    dossie_repo::list_entities(&conn, &opened.occurrence.id)
}

#[tauri::command]
pub async fn list_dossie_traces(workspace_path: String) -> Result<Vec<Trace>> {
    let ws = PathBuf::from(&workspace_path);
    let opened = open_workspace(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    dossie_repo::list_traces(&conn, &opened.occurrence.id)
}

#[tauri::command]
pub async fn list_dossie_measurements(workspace_path: String) -> Result<Vec<Measurement>> {
    let ws = PathBuf::from(&workspace_path);
    let opened = open_workspace(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    dossie_repo::list_measurements(&conn, &opened.occurrence.id)
}

#[tauri::command]
pub async fn list_dossie_notes(workspace_path: String) -> Result<Vec<FieldNote>> {
    let ws = PathBuf::from(&workspace_path);
    let opened = open_workspace(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    dossie_repo::list_field_notes(&conn, &opened.occurrence.id)
}

#[tauri::command]
pub async fn list_dossie_timeline(workspace_path: String) -> Result<Vec<TimelineEvent>> {
    let ws = PathBuf::from(&workspace_path);
    let opened = open_workspace(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    dossie_repo::list_timeline(&conn, &opened.occurrence.id)
}

#[tauri::command]
pub async fn get_dossie_stats(workspace_path: String) -> Result<Option<OccurrenceStats>> {
    let ws = PathBuf::from(&workspace_path);
    let opened = open_workspace(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    dossie_repo::find_stats(&conn, &opened.occurrence.id)
}

/// Manually re-extract every Dossiê table from the most recent staged
/// package. The frontend invokes this when the user clicks "Recarregar
/// dados do pacote" on the Import tab.
#[tauri::command]
pub async fn rehydrate_dossie(workspace_path: String) -> Result<RehydrateOutcome> {
    let ws = PathBuf::from(&workspace_path);
    let _ = open_workspace(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    rehydrate_workspace(&ws, &conn)
}
