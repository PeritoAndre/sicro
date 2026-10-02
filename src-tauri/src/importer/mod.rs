//! Importador de pacotes `.sicroapp` (e `.sicrocampo`, nome antigo). Todo caminho
//! vindo do ZIP passa por `safe_zip::sanitize_zip_path`.

pub mod dossie_mapper;
pub mod manifest_parser;
pub mod orchestrator;
pub mod package_reader;
pub mod registry;
pub mod rehydrator;
pub mod safe_zip;

pub use orchestrator::run_import;
pub use registry::{GlobalImportRecord, ImportRegistry};
pub use rehydrator::rehydrate_workspace;
