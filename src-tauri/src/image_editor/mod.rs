//! Editor de imagem pericial: processamento, pipeline de export, metadados,
//! filtros, EXIF, hashes e relatório. Os comandos Tauri ficam em
//! `crate::commands::image_commands`.

pub mod exif;
pub mod filters;
pub mod hashes;
pub mod mask;
pub mod metadata;
pub mod pipeline;
pub mod processor;
pub mod report;
