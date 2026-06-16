//! Image processing — reusable, framework-agnostic operators.
//!
//! Lives under the library crate so other modules (croqui drone import,
//! image editor, future image-quality checks) can call the same code.
//! Nothing in here knows about Tauri or SQLite — the input is a path
//! or byte slice, the output is a corrected image (or bytes).

pub mod lens_correction;
