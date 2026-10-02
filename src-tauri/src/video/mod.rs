//! Vídeo: ffprobe/ffmpeg (shell-out) como fonte técnica da verdade — o player
//! é só visualização.

pub mod clip;
pub mod frame_export;
pub mod measure;
pub mod probe;
pub mod speed;
pub mod thumbnail;

pub use frame_export::{extract_frame, ExtractFrameOptions, ExtractedFrame};
pub use probe::{detect_ffprobe, probe_media, ParsedProbe};
