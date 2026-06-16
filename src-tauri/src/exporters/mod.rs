//! Export Engine — produces HTML / PDF / DOCX artefacts from a `.sicrodoc`.
//!
//! Architecture (Spike C):
//!   - `paths`  : where each artefact lives inside the workspace.
//!   - `html`   : writes the HTML the front-end already rendered (Document Engine).
//!   - `pdf`    : invokes Microsoft Edge in headless `--print-to-pdf` mode.
//!   - `docx`   : walks the TipTap JSON content and builds a docx-rs document.
//!
//! Why these choices?
//!   - HTML is the cheapest format; the Document Engine already knows how to
//!     render it. Rust just persists the bytes to disk.
//!   - PDF reuses the Chromium engine already installed on every Windows 11
//!     machine (Microsoft Edge). No extra binary shipped, no Rust dependency
//!     pulled in just to render CSS.
//!   - DOCX cannot be produced from HTML easily with pure Rust; converting the
//!     TipTap JSON directly is more honest. `docx-rs` is small and pure Rust.
//!
//! Each exporter is independent and may be replaced wholesale in a future spike
//! (e.g. swap headless Edge for a Rust PDF library, or DOCX for a template-based
//! approach) without touching the others.

pub mod docx;
pub mod html;
pub mod paths;
pub mod pdf;

pub use paths::resolve_export_target;
