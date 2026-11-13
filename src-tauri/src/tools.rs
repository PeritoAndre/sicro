//! Programas externos (ffmpeg, ffprobe, whisper…). O instalador do Windows e o AppImage
//! trazem FFmpeg e ExifTool embutidos (o .deb/.rpm usam os da distro); o embutido tem
//! prioridade. No Windows todo processo precisa de `CREATE_NO_WINDOW` para não piscar console.

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

/// Pastas onde os programas embutidos podem estar: ao lado do executável (Windows)
/// e nos recursos do pacote Linux (`usr/lib/SICRO/` no AppImage; o nome do binário por garantia).
fn bundled_dirs() -> Vec<PathBuf> {
    let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from)) else {
        return Vec::new();
    };
    let mut dirs = vec![exe_dir.clone()];
    if cfg!(target_os = "linux") {
        for nome in ["SICRO", env!("CARGO_PKG_NAME")] {
            dirs.push(exe_dir.join("../lib").join(nome));
        }
    }
    dirs
}

/// Primeiro `<pasta>/<sub>/<arquivo>` que exista entre as pastas embutidas.
fn bundled_file(sub: &str, file: &str) -> Option<PathBuf> {
    bundled_dirs().into_iter().map(|d| d.join(sub).join(file)).find(|p| p.is_file())
}

/// FFmpeg que vem junto com o SICRO: `ffmpeg/<nome>[.exe]` (Windows e AppImage).
pub fn bundled_ffmpeg_tool(name: &str) -> Option<PathBuf> {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    bundled_file("ffmpeg", &file)
}

/// `ffmpeg` / `ffprobe`: primeiro o que vem com o SICRO, depois o PATH.
pub fn find_ffmpeg_tool(name: &str) -> Option<PathBuf> {
    bundled_ffmpeg_tool(name).or_else(|| which::which(name).ok())
}

/// `exiftool`: o que vem com o SICRO (`exiftool/`, Windows e AppImage), depois o PATH.
pub fn find_exiftool() -> Option<PathBuf> {
    bundled_file("exiftool", if cfg!(windows) { "exiftool.exe" } else { "exiftool" })
        .or_else(|| which::which("exiftool").ok())
}
