//! G12 — Filtros forenses para o Image Engine Pro.
//!
//! Cada submódulo cobre uma família:
//!   - `edges`        — Sobel, Laplacian, Canny.
//!   - `blur`         — Gaussian, Median, Bilateral.
//!   - `morphology`   — Dilate, Erode, Open, Close.
//!   - `enhancement`  — CLAHE, Histogram EQ, Auto-Levels, White Balance.
//!   - `geometric`    — Perspective warp (4-point homography).
//!   - `misc`         — Unsharp mask, Threshold, Pixelize.
//!
//! Todas as funções recebem `&RgbaImage` ou consomem `RgbaImage` e
//! retornam novo `RgbaImage`. Não dependem de OpenCV nem de bindings de
//! sistema — só `image` crate + Rust puro.

pub mod blur;
pub mod channels;
pub mod compare;
pub mod convolution;
pub mod decorrelation;
pub mod edges;
pub mod enhancement;
pub mod geometric;
pub mod histogram;
pub mod misc;
pub mod morphology;
pub mod tone;
