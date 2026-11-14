//! Proxy de prévia (§7.6): GPU (`h264_nvenc` com `scale_cuda`) com queda para a CPU (`previa::proxy_cpu`, O8),
//! 720p, GOP 12 sem quadros B, pts iguais aos da fonte (`-fps_mode passthrough -enc_time_base demux`). Nunca entra em
//! hash, relatório nem render. A decisão "cópia ou proxy" fica em `previa.rs` (também do P1.9). Dono: P1.9.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::motor::FonteInfo;
use crate::edicao::previa::PreviaMidia;
use crate::error::Result;

/// A mídia toca pelo proxy? Altura > 1080, codec ≠ H.264 8 bits 4:2:0, taxa variável, mais de 60 qps, ou busca lenta
/// medida (`busca_p50_ms` > 120: 3 buscas pausadas quando a mídia entra na Edição). Rampa, ré e abaixo de 0,25× são do
/// clipe: o palco pede o proxy nesses casos.
pub fn precisa(fonte: &FonteInfo, busca_p50_ms: Option<f64>) -> bool {
    let _ = (fonte, busca_p50_ms);
    todo!("P1.9")
}

/// Faz o proxy (bloqueia a thread): GPU aprovada na sonda primeiro; código ≠ 0 refaz em CPU. Lista de pts igual à da
/// fonte. `progresso` em 0..1.
pub fn produzir(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, cancelar: &AtomicBool, progresso: &mut dyn FnMut(f32)) -> Result<PreviaMidia> {
    let _ = (cap, cache, fonte, cancelar, progresso);
    todo!("P1.9")
}
