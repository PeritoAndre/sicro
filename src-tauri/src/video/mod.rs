//! Video Engine (Spike F).
//!
//! Two responsibilities, both shell-outs to standard tools:
//!   - `probe`        — runs `ffprobe -of json` and parses the result into
//!                       a `ParsedProbe` consumable by the orchestrator.
//!   - `frame_export` — runs `ffmpeg -ss <ts> -i <video> -frames:v 1
//!                       -update 1 -y <out.png>` to extract a single frame
//!                       and writes a sidecar JSON next to the PNG.
//!
//! The Python lab proved (`SICRO_VIDEO_LAB_RELATORIO.md` §6) that the
//! visual player should not be the source of technical truth — that's
//! ffprobe/ffmpeg. Spike F honours that decision: HTMLVideoElement is the
//! viewer, this module is the perito's technical eye.

pub mod frame_export;
pub mod measure;
pub mod probe;
pub mod speed;

pub use frame_export::{extract_frame, ExtractFrameOptions, ExtractedFrame};
pub use probe::{detect_ffprobe, probe_media, ParsedProbe};
