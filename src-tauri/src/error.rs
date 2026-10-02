//! Erro da aplicação. Ao cruzar a fronteira Tauri vira `{ kind, message }`; o
//! `toSicroError()` do front lê exatamente esse formato — não mudar sem o front.

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

/// Tauri serializa erros de comando por este `Serialize`: `{ kind, message }`.
/// `std::result::Result` qualificado porque o alias `Result` no fim do arquivo
/// faz sombra ao da prelude.
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
