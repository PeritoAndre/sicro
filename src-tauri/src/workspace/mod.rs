//! Workspace = a single `.sicro` directory on disk.
//!
//! Layout (doc 02 §9):
//!     <name>.sicro/
//!         manifest.json
//!         sicro.sqlite
//!         dossie/  laudos/  croquis/  videos/
//!         imagens/  midias/  exports/  logs/  cache/

pub mod backup;
pub mod create;
pub mod global_backup;
pub mod health;
pub mod manifest;
pub mod open;
pub mod paths;
pub mod validation;

pub use backup::{create_backup, BackupArtifact};
pub use global_backup::{
    run_global_backup, run_restore, CaseBackupResult, GlobalBackupProgress, GlobalBackupReport,
    GlobalCaseInput, RestoreProgress, RestoreReport, RestoredCase,
};
pub use create::create_workspace;
pub use health::{
    build_snapshot, count_occurrence_entities, render_and_save, HealthReportArtifact,
    SystemHealthSnapshot, WorkspaceCounters,
};
pub use manifest::{Manifest, MANIFEST_FILENAME, SQLITE_FILENAME};
pub use open::open_workspace;
