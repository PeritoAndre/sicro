//! Editor de Imagem Pericial — módulo central.
//!
//! MVP 7 (estrutura base):
//!   - `processor`  → operações reais sobre bytes (image crate);
//!   - `pipeline`   → orquestra leitura → ajustes → operações → escrita,
//!                    com sidecar JSON + hash final;
//!   - `metadata`   → leitura de metadados (dimensões, mime, hash) sem
//!                    decodificar a imagem inteira quando possível.
//!
//! G12 (Image Engine Pro — adições):
//!   - `filters`    → submódulos com filtros forenses (Sobel, CLAHE,
//!                    morfologia, perspectiva, etc.).
//!   - `exif`       → leitura de EXIF (kamadak-exif).
//!   - `hashes`     → MD5 / SHA-1 / SHA-256 / SHA-3-256 em paralelo
//!                    para chain of custody.
//!   - `report`     → relatório HTML/PDF da análise pericial.
//!
//! Nenhum commando Tauri vive aqui — esses ficam em
//! `crate::commands::image_commands`, que apenas orquestra
//! `image_analysis_repo` + chamada dos módulos.

pub mod exif;
pub mod filters;
pub mod hashes;
pub mod mask;
pub mod metadata;
pub mod pipeline;
pub mod processor;
pub mod report;
