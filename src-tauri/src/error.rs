//! Application-level error type.
//!
//! Errors that cross the Tauri boundary (i.e. propagate to JavaScript)
//! are serialized as a small JSON object `{ kind, message }`. The front-end
//! `toSicroError()` reads exactly that shape, so do NOT change it lightly.

use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SicroError {
    #[error("workspace error: {0}")]
    Workspace(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("filesystem error: {0}")]
    Filesystem(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl SicroError {
    pub fn kind(&self) -> &'static str {
        match self {
            SicroError::Workspace(_) => "workspace",
            SicroError::Database(_) | SicroError::Sqlite(_) => "database",
            SicroError::Filesystem(_) => "filesystem",
            SicroError::Validation(_) => "validation",
            SicroError::Io(_) => "io",
            SicroError::Serde(_) => "io",
        }
    }
}

/// Tauri serializes command errors with the type's `Serialize` impl.
/// We emit `{ kind, message }` so the front-end can rely on the shape.
///
/// NOTE: the return type is explicitly qualified as `std::result::Result`
/// because the `Result` alias declared at the bottom of this file shadows the
/// prelude one. Without the qualifier the compiler would resolve
/// `Result<S::Ok, S::Error>` to `std::result::Result<S::Ok, SicroError>` —
/// the wrong error type for `Serializer::serialize`.
impl Serialize for SicroError {
    fn serialize<S: Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("SicroError", 2)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

pub type Result<T> = std::result::Result<T, SicroError>;
