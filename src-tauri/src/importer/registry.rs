//! Índice global (`imports_index.json` no diretório de config) dos pacotes já
//! importados: cada workspace só conhece seus próprios imports.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;
use crate::filesystem::atomic_write_bytes;

const REGISTRY_FILENAME: &str = "imports_index.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalImportRecord {
    pub package_sha256: String,
    pub workspace_id: Uuid,
    pub workspace_path: String,
    pub import_id: Uuid,
    pub original_filename: Option<String>,
    pub mobile_occurrence_id: Option<String>,
    pub imported_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RegistryFile {
    #[serde(default)]
    imports: Vec<GlobalImportRecord>,
}

/// Abstração sobre o arquivo para testar com tempdir.
pub struct ImportRegistry {
    path: PathBuf,
}

impl ImportRegistry {
    pub fn open(config_dir: &Path) -> Self {
        Self {
            path: config_dir.join(REGISTRY_FILENAME),
        }
    }

    pub fn find_by_sha256(&self, sha256: &str) -> Result<Option<GlobalImportRecord>> {
        let file = self.read()?;
        Ok(file
            .imports
            .into_iter()
            .find(|r| r.package_sha256.eq_ignore_ascii_case(sha256)))
    }

    /// Insere no topo; não deduplica (o caller já recusou duplicatas).
    pub fn record(&self, entry: GlobalImportRecord) -> Result<()> {
        let mut file = self.read()?;
        file.imports.insert(0, entry);
        self.write(&file)
    }

    fn read(&self) -> Result<RegistryFile> {
        if !self.path.exists() {
            return Ok(RegistryFile::default());
        }
        let bytes = std::fs::read(&self.path).map_err(|e| {
            crate::error::SicroError::Filesystem(format!(
                "cannot read import registry {}: {}",
                self.path.display(),
                e
            ))
        })?;
        Ok(serde_json::from_slice::<RegistryFile>(&bytes).unwrap_or_default())
    }

    fn write(&self, file: &RegistryFile) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(file)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        atomic_write_bytes(&self.path, &bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = ImportRegistry::open(tmp.path());
        assert!(reg.find_by_sha256("abc").unwrap().is_none());

        reg.record(GlobalImportRecord {
            package_sha256: "abc".into(),
            workspace_id: Uuid::new_v4(),
            workspace_path: "/tmp/ws".into(),
            import_id: Uuid::new_v4(),
            original_filename: Some("toy.sicroapp".into()),
            mobile_occurrence_id: Some("occ_1".into()),
            imported_at: Utc::now(),
        })
        .unwrap();

        let found = reg.find_by_sha256("ABC").unwrap();
        assert!(found.is_some());
    }
}
