//! Workspace creation.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use uuid::Uuid;

use crate::database::{connection::open_connection, migrations::run_migrations};
use crate::database::repositories::occurrence_repo;
use crate::error::{Result, SicroError};
use crate::models::{NewOccurrenceInput, Occurrence, OccurrenceStatus};
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};
use crate::workspace::paths::{derive_workspace_name, unique_workspace_path};

/// Top-level folders that every workspace must have, per doc 02 §9.
const SUBDIRS: &[&str] = &[
    "dossie",
    "laudos",
    "laudos/assets",
    "croquis",
    "croquis/exports",
    "videos",
    "videos/media",
    "videos/storyboards",
    "videos/frames",
    "imagens",
    "imagens/originais",
    "imagens/tratadas",
    "imagens/anotadas",
    "midias",
    "midias/anexos",
    "exports",
    "exports/docx",
    "exports/pdf",
    "exports/png",
    "exports/pacotes",
    "logs",
    "cache",
    "cache/thumbnails",
    "cache/previews",
];

pub struct CreatedWorkspace {
    pub path: PathBuf,
    pub manifest: Manifest,
    pub occurrence: Occurrence,
}

pub fn create_workspace(
    input: NewOccurrenceInput,
    default_parent: &Path,
) -> Result<CreatedWorkspace> {
    // 1. Resolve where the workspace will live on disk.
    let parent: PathBuf = match &input.parent_directory {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => default_parent.to_path_buf(),
    };

    if !parent.exists() {
        fs::create_dir_all(&parent).map_err(|e| {
            SicroError::Filesystem(format!(
                "could not create parent directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }
    if !parent.is_dir() {
        return Err(SicroError::Validation(format!(
            "parent path is not a directory: {}",
            parent.display()
        )));
    }

    // 2. Generate ids and pick a unique folder name.
    let workspace_id = Uuid::new_v4();
    let occurrence_id = workspace_id; // Spike A: 1 workspace = 1 occurrence.
    let short_id = workspace_id.to_string()[..8].to_string();
    let base_name = derive_workspace_name(
        input.numero_bo.as_deref(),
        input.municipio.as_deref(),
        &short_id,
    );
    let workspace_path = unique_workspace_path(&parent, &base_name)?;

    // 3. Build the directory tree.
    fs::create_dir_all(&workspace_path)?;
    for sub in SUBDIRS {
        fs::create_dir_all(workspace_path.join(sub))?;
    }

    // 4. Initialize SQLite and run migrations.
    let db_path = workspace_path.join(SQLITE_FILENAME);
    let mut conn = open_connection(&db_path)?;
    run_migrations(&mut conn)?;

    // 5. Insert the initial occurrence row.
    let now = Utc::now();
    let occurrence = Occurrence {
        id: occurrence_id,
        numero_bo: input.numero_bo.filter(|s| !s.trim().is_empty()),
        protocolo: input.protocolo.filter(|s| !s.trim().is_empty()),
        requisicao: None,
        oficio: input.oficio.filter(|s| !s.trim().is_empty()),
        delegacia: None,
        tipo_pericia: input.tipo_pericia.filter(|s| !s.trim().is_empty()),
        natureza: None,
        municipio: input.municipio.filter(|s| !s.trim().is_empty()),
        bairro: None,
        logradouro: None,
        referencia: None,
        latitude: None,
        longitude: None,
        data_fato: None,
        data_acionamento: None,
        data_chegada: None,
        data_encerramento: None,
        peritos: input
            .peritos
            .into_iter()
            .filter(|p| !p.trim().is_empty())
            .collect(),
        status: OccurrenceStatus::Aberta,
        created_at: now,
        updated_at: now,
        // Spike A occurrence — not imported from a .sicroapp.
        import_id: None,
        original_mobile_id: None,
        primary_accuracy_m: None,
        resultado: None,
        raw_case_json: None,
        raw_metadata_json: None,
        raw_location_json: None,
    };
    occurrence_repo::insert(&conn, &occurrence)?;

    occurrence_repo::record_audit(
        &conn,
        Some(&occurrence.id),
        "occurrence.created",
        Some("workspace"),
        Some("occurrence"),
        Some(&occurrence.id),
        None,
    )?;

    // 6. Write the manifest.
    let manifest = Manifest::new(workspace_id, occurrence_id);
    manifest.write(&workspace_path)?;

    Ok(CreatedWorkspace {
        path: workspace_path,
        manifest,
        occurrence,
    })
}
