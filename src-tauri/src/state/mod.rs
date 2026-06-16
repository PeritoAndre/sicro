//! Cross-workspace state managed by the app process itself.
//!
//! Currently this is just the global "recents" list, stored as JSON under
//! the user's config directory. Future cross-cutting state (user profile,
//! per-machine settings) goes here too — workspaces stay self-contained.

use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Utc;
use directories::{ProjectDirs, UserDirs};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Result, SicroError};
use crate::filesystem::atomic_write_bytes;
use crate::models::{Occurrence, RecentOccurrence};

const QUALIFIER: &str = "br.org";
const ORGANIZATION: &str = "SICRO";
const APPLICATION: &str = "SICRO";
const RECENT_FILENAME: &str = "recent.json";
const MAX_RECENTS: usize = 25;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RecentsFile {
    #[serde(default)]
    recents: Vec<RecentOccurrence>,
}

/// Global state owned by Tauri's `App::manage()`.
pub struct AppState {
    config_dir: PathBuf,
    default_workspace_parent: PathBuf,
    recents_path: PathBuf,
    inner: Mutex<RecentsFile>,
}

impl AppState {
    pub fn init() -> Result<Self> {
        let project_dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION).ok_or_else(
            || {
                SicroError::Filesystem(
                    "could not resolve the application's config directory".to_string(),
                )
            },
        )?;
        let config_dir = project_dirs.config_dir().to_path_buf();
        std::fs::create_dir_all(&config_dir)?;

        let recents_path = config_dir.join(RECENT_FILENAME);
        let recents = if recents_path.is_file() {
            match std::fs::read(&recents_path) {
                Ok(bytes) => serde_json::from_slice::<RecentsFile>(&bytes).unwrap_or_default(),
                Err(_) => RecentsFile::default(),
            }
        } else {
            RecentsFile::default()
        };

        // Pasta padrão dos casos: LOCAL e fora de sync (ex.: C:\Users\<user>\
        // SICRO\Casos), NÃO em Documentos — que costuma estar redirecionado pro
        // OneDrive. Um SQLite "vivo" dentro de pasta sincronizada corrompe (o
        // sync sobe o .sqlite sem o -wal, ou no meio de uma transação). A
        // redundância em nuvem é feita via BACKUP (.sicrobackup estático), não
        // sincronizando o banco vivo. O perito pode trocar em Configurações.
        let default_workspace_parent = UserDirs::new()
            .map(|d| d.home_dir().join("SICRO").join("Casos"))
            .unwrap_or_else(|| config_dir.join("workspaces"));

        std::fs::create_dir_all(&default_workspace_parent).ok();

        Ok(Self {
            config_dir,
            default_workspace_parent,
            recents_path,
            inner: Mutex::new(recents),
        })
    }

    pub fn config_dir(&self) -> &std::path::Path {
        &self.config_dir
    }

    pub fn default_workspace_parent(&self) -> &std::path::Path {
        &self.default_workspace_parent
    }

    pub fn list_recents(&self) -> Vec<RecentOccurrence> {
        let guard = self.inner.lock().expect("recents mutex poisoned");
        guard.recents.clone()
    }

    pub fn upsert_recent(
        &self,
        occurrence: &Occurrence,
        workspace_path: &str,
        workspace_id: Uuid,
    ) -> Result<()> {
        let mut guard = self.inner.lock().expect("recents mutex poisoned");

        // Drop any prior entry with the same workspace_id OR the same path
        // (paths can move; ids are stable).
        guard
            .recents
            .retain(|r| r.workspace_id != workspace_id && r.workspace_path != workspace_path);

        let mut entry =
            RecentOccurrence::from_occurrence(occurrence, workspace_path, workspace_id);
        entry.last_opened_at = Utc::now();
        guard.recents.insert(0, entry);

        if guard.recents.len() > MAX_RECENTS {
            guard.recents.truncate(MAX_RECENTS);
        }

        write_recents(&self.recents_path, &guard)?;
        Ok(())
    }

    pub fn forget_recent(&self, workspace_id: Uuid) -> Result<()> {
        let mut guard = self.inner.lock().expect("recents mutex poisoned");
        guard.recents.retain(|r| r.workspace_id != workspace_id);
        write_recents(&self.recents_path, &guard)?;
        Ok(())
    }
}

fn write_recents(path: &std::path::Path, file: &RecentsFile) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(file)?;
    atomic_write_bytes(path, &bytes)?;
    Ok(())
}
