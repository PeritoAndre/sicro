//! Fotos do caso (as que vêm do pacote `.sicroapp` do SICRO Operacional) — a
//! origem "Fotos do caso" das Imagens.

use std::path::PathBuf;

use crate::database::connection::open_connection;
use crate::database::repositories::media_asset_repo;
use crate::error::Result;
use crate::models::MediaAsset;
use crate::workspace::manifest::SQLITE_FILENAME;
use crate::workspace::open_workspace;

#[tauri::command]
pub async fn list_dossie_photos(workspace_path: String) -> Result<Vec<MediaAsset>> {
    let ws = PathBuf::from(&workspace_path);
    let opened = open_workspace(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    media_asset_repo::list_by_occurrence(&conn, &opened.occurrence.id)
}
