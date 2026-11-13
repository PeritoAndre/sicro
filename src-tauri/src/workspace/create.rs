//! Workspace creation.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Local, Utc};
use uuid::Uuid;

use crate::database::{connection::open_connection, migrations::run_migrations};
use crate::database::repositories::occurrence_repo;
use crate::error::{Result, SicroError};
use crate::models::{NewOccurrenceInput, Occurrence, OccurrenceStatus};
use crate::workspace::manifest::Manifest;
use crate::workspace::paths::{PASTAS_DO_CASO, derive_workspace_name, sqlite_path, unique_workspace_path};

pub struct CreatedWorkspace {
    pub path: PathBuf,
    pub manifest: Manifest,
    pub occurrence: Occurrence,
}

pub fn create_workspace(
    input: NewOccurrenceInput,
    default_parent: &Path,
) -> Result<CreatedWorkspace> {
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

    let workspace_id = Uuid::new_v4();
    let occurrence_id = workspace_id; // 1 workspace = 1 ocorrência: mesmo UUID.
    // Só a pasta ganha o nome do caso; o id interno segue sendo o UUID.
    let base_name = derive_workspace_name(input.titulo.as_deref(), Local::now().date_naive());
    let workspace_path = unique_workspace_path(&parent, &base_name)?;

    fs::create_dir_all(&workspace_path)?;
    for sub in PASTAS_DO_CASO {
        fs::create_dir_all(workspace_path.join(sub))?;
    }

    let db_path = sqlite_path(&workspace_path);
    let mut conn = open_connection(&db_path)?;
    run_migrations(&mut conn)?;

    let now = Utc::now();
    let occurrence = Occurrence {
        id: occurrence_id,
        titulo: input
            .titulo
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
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
        // Não veio de .sicroapp.
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

    let manifest = Manifest::new(workspace_id, occurrence_id);
    manifest.write(&workspace_path)?;

    Ok(CreatedWorkspace {
        path: workspace_path,
        manifest,
        occurrence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entrada(titulo: Option<&str>) -> NewOccurrenceInput {
        NewOccurrenceInput {
            titulo: titulo.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn pasta_leva_o_nome_do_caso_e_o_layout_novo() {
        let tmp = tempfile::tempdir().unwrap();
        let a = create_workspace(entrada(Some("Acidente BR-156")), tmp.path()).unwrap();
        let b = create_workspace(entrada(Some("Acidente BR-156")), tmp.path()).unwrap();
        let c = create_workspace(entrada(None), tmp.path()).unwrap();
        let nome = |p: &Path| p.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(nome(&a.path), "Acidente BR-156.sicro");
        assert_eq!(nome(&b.path), "Acidente BR-156.2.sicro");
        assert!(nome(&c.path).starts_with("Caso de "), "{}", nome(&c.path));
        assert!(a.path.join("Sistema/manifest.json").is_file());
        assert!(a.path.join("Sistema/sicro.sqlite").is_file());
        assert!(a.path.join("Exportações/Croquis").is_dir());
        assert!(a.path.join("Mídias/Vídeos").is_dir());
        assert!(!a.path.join("manifest.json").exists());
    }
}
