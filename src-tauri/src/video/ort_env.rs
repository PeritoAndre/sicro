//! O onnxruntime só admite um ambiente por processo: Pessoas e Super-resolução dividem este.

use std::path::Path;
use std::sync::OnceLock;

use crate::error::{Result, SicroError};

static ENV: OnceLock<ort::environment::Environment> = OnceLock::new();

pub fn entorno(lib: &Path) -> Result<&'static ort::environment::Environment> {
    let err = |e: &dyn std::fmt::Display| SicroError::Validation(format!("onnxruntime: {e}"));
    if ENV.get().is_none() {
        let env = ort::init_from(lib).map_err(|e| err(&e))?.build().map_err(|e| err(&e))?;
        let _ = ENV.set(env);
    }
    ENV.get().ok_or_else(|| err(&"ambiente não iniciado"))
}
