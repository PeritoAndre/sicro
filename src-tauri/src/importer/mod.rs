//! `.sicroapp` importer (Spike D).
//!
//! High-level flow (orchestrated by `run_import`):
//!
//!   1. **Open the file** — validate the extension and that the file is a real
//!      ZIP (not a renamed PDF, not corrupt).
//!   2. **Read `manifest.json`** — pull `formato`, `versao`, `gerado_em`,
//!      `ocorrencia.*`, `contagens`, `avisos`; preserve the raw payload.
//!   3. **Detect duplicate** — compare `package_sha256` against existing
//!      `imports` rows and refuse if already present.
//!   4. **Create workspace** — fresh `.sicro` directory + SQLite (re-uses the
//!      Spike A pipeline) so the import lands somewhere isolated.
//!   5. **Stage the original package** — copy the `.sicroapp` to
//!      `imports/<id>/original_package.sicroapp`.
//!   6. **Verify hashes** — if `hashes.json` exists, compare to streamed
//!      SHA-256 of each listed entry.
//!   7. **Read structured JSONs** — `caso`, `metadados`, `localizacao`, etc.,
//!      tolerating absent files with warnings.
//!   8. **Build the Desktop Occurrence** — map fields from `caso.json` +
//!      `metadados.json` + `localizacao.json`; keep raw JSONs verbatim.
//!   9. **Extract media** — copy `fotos/<id>.<ext>` to `media/photos/`;
//!      record `media_assets` + `evidence_items`.
//!  10. **Write report** — build `ImportReport`, persist it to
//!      `imports/<id>/import_report.json`, return it to the UI.
//!
//! All path manipulation goes through `safe_zip::sanitize_zip_path` to refuse
//! traversal (`..`) and absolute entries.

pub mod docx_import;
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
