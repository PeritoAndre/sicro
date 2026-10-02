//! Superfície de comandos Tauri. Sem `pub use` dos comandos aqui: o
//! `generate_handler!` depende dos símbolos gerados (`__cmd__*`) ao lado da fn,
//! e um re-export quebraria o registro. Sempre `commands::<módulo>::<nome>`.

pub mod ai_commands;
pub mod alpha_commands;
pub mod audio_commands;
pub mod case_index_commands;
pub mod croqui_commands;
pub mod dossie_commands;
pub mod image_commands;
pub mod import_commands;
pub mod os_open;
pub mod registry_commands;
pub mod settings_commands;
pub mod video_commands;
pub mod video_distance_commands;
pub mod video_speed_commands;
pub mod workspace_commands;
