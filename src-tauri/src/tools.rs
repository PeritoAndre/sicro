//! Programas externos (ffmpeg, ffprobe, whisper, navegador…): onde achar e
//! como rodar.
//!
//! - O instalador do Windows traz o FFmpeg em `<pasta do SICRO>\ffmpeg\`
//!   (quase nenhum Windows tem ffmpeg no PATH). Ele tem prioridade; sem ele,
//!   vale o do PATH (Linux: o do sistema).
//! - O SICRO é um app de janela: no Windows, cada programa de console aberto
//!   sem `CREATE_NO_WINDOW` pisca uma janela preta na tela.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

/// `Command` que não abre janela de console no Windows.
pub fn command<S: AsRef<OsStr>>(program: S) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// FFmpeg que vem junto com o SICRO: `<pasta do executável>/ffmpeg/<nome>[.exe]`.
pub fn bundled_ffmpeg_tool(name: &str) -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.join("ffmpeg");
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let p = dir.join(file);
    p.is_file().then_some(p)
}

/// `ffmpeg` / `ffprobe`: primeiro o que vem com o SICRO, depois o PATH.
pub fn find_ffmpeg_tool(name: &str) -> Option<PathBuf> {
    bundled_ffmpeg_tool(name).or_else(|| which::which(name).ok())
}
