//! O onnxruntime só admite um ambiente por processo: ponte para `ia::sessao::ambiente` (Pessoas, super-resolução,
//! rastreio e testes dividem o mesmo).

use std::path::Path;

use crate::error::Result;

pub fn entorno(lib: &Path) -> Result<&'static ort::environment::Environment> {
    crate::ia::sessao::ambiente(lib)
}
