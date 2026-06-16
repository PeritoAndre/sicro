//! Evidence Registry (MVP 5) — consolidated read of every piece of
//! evidence, derivative and artefact owned by a `.sicro` workspace.
//!
//! The registry is *not* a new persisted table — it is a projection
//! built on demand from the existing module tables. This keeps the
//! schema migration risk near zero (the new module only reads) and
//! lets the perito see everything in one place without changing how
//! each module persists its own data.
//!
//! Public entry points:
//!   - `aggregator::build_registry`  → `Vec<EvidenceRegistryItem>`
//!   - `aggregator::build_summary`   → `RegistrySummary`
//!   - `integrity::verify_workspace` → `WorkspaceIntegrityReport`
//!   - `report::render_html_report`  → string ready to atomic-write
//!
//! Each submodule is a leaf — they do not depend on each other, so the
//! commands can mix and match cheap calls (`build_summary` alone) and
//! expensive ones (`verify_workspace` with deep hashing).

pub mod aggregator;
pub mod broken_links;
pub mod integrity;
pub mod report;

pub use aggregator::{build_registry, build_summary};
pub use broken_links::detect_broken_laudo_links;
pub use integrity::verify_workspace;
pub use report::render_html_report;
