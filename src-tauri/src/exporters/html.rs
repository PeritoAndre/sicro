//! HTML export — trivial pass-through.
//!
//! The front-end Document Engine already renders the full HTML (head + style
//! + body). Rust only writes the bytes atomically. Keeping the function in a
//! dedicated module mirrors the pdf/docx siblings and gives a single home for
//! any future HTML post-processing (asset rewriting, inlining, etc.).

use std::path::Path;

use crate::error::Result;
use crate::filesystem::atomic_write_bytes;

pub fn write_html(target: &Path, html: &str) -> Result<()> {
    atomic_write_bytes(target, html.as_bytes())?;
    Ok(())
}
