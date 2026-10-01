//! Tauri command surface — anything callable from JavaScript lives here.
//!
//! We deliberately do NOT `pub use` the individual `#[tauri::command]` items
//! at this level. The `tauri::generate_handler!` macro relies on macro-generated
//! sibling symbols (`__cmd__*`) which live next to the function in their
//! module; a `pub use` would silently shadow that path and break the handler
//! registration. Always reference commands via their full module path:
//! `commands::<module>::<name>`.

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
