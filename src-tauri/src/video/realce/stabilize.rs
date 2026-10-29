//! Cópia estabilizada (ffmpeg vidstabdetect + vidstabtransform, duas passadas; `deshake` quando
//! o build do ffmpeg não traz o vidstab) e cópia com a lente corrigida (`lenscorrection`).
//! O original nunca é tocado: sai um arquivo novo, recompresso (H.264, CRF 16).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::error::{Result, SicroError};
use crate::video::frame_export::detect_ffmpeg;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Borda {
    /// Amplia a imagem até a borda preta sumir (perde os cantos).
    Cortar,
    /// Mantém o enquadramento e preenche o que faltou com o quadro vizinho.
    Preencher,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct OpcoesEstabilizar {
    /// Quadros de cada lado usados para suavizar o movimento (1 a 60).
    pub suavizar: u32,
    pub borda: Borda,
}

#[derive(Debug, Clone, Serialize)]
pub struct Estabilizado {
    pub motor: &'static str,
    pub comandos: Vec<String>,
}

/// Pasta temporária própria (apagada ao sair).
struct PastaTemp(PathBuf);

impl PastaTemp {
    fn nova() -> Result<Self> {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let p = std::env::temp_dir().join(format!("sicro-vidstab-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&p).map_err(|e| SicroError::Filesystem(format!("pasta temporária: {e}")))?;
        Ok(Self(p))
    }
}

impl Drop for PastaTemp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// O ffmpeg traz o filtro `nome`?
pub fn tem_filtro(nome: &str) -> bool {
    let Ok(ff) = detect_ffmpeg() else { return false };
    let Ok(out) = crate::tools::command(ff).args(["-hide_banner", "-filters"]).output() else { return false };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.split_whitespace().nth(1) == Some(nome))
}

const CODEC: [&str; 10] = ["-c:v", "libx264", "-preset", "medium", "-crf", "16", "-pix_fmt", "yuv420p", "-fps_mode", "passthrough"];

/// Roda o ffmpeg lendo `-progress` (fração 0..1 pelo tempo de saída). `cancelar` mata o processo.
fn rodar(
    args: &[String],
    cwd: Option<&Path>,
    total_s: f64,
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(f64),
) -> Result<()> {
    let ff = detect_ffmpeg()?;
    let mut cmd = crate::tools::command(&ff);
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-nostats", "-progress", "pipe:1"])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let mut child = cmd.spawn().map_err(|e| SicroError::Workspace(format!("could not spawn ffmpeg: {e}")))?;
    let stderr = child.stderr.take();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = stderr {
            let _ = std::io::Read::read_to_string(&mut e, &mut s);
        }
        s
    });
    if let Some(out) = child.stdout.take() {
        for linha in BufReader::new(out).lines().map_while(|l| l.ok()) {
            if cancelar.load(Ordering::Relaxed) {
                let _ = child.kill();
                break;
            }
            if let Some(v) = linha.strip_prefix("out_time_us=").or_else(|| linha.strip_prefix("out_time_ms=")) {
                if let Ok(us) = v.trim().parse::<f64>() {
                    if total_s > 0.0 {
                        progresso((us / 1e6 / total_s).clamp(0.0, 1.0));
                    }
                }
            }
        }
    }
    let status = child.wait().map_err(|e| SicroError::Workspace(format!("ffmpeg: {e}")))?;
    let stderr = err_thread.join().unwrap_or_default();
    if cancelar.load(Ordering::Relaxed) {
        return Err(SicroError::Validation("cancelado".into()));
    }
    if !status.success() {
        return Err(SicroError::Workspace(format!("ffmpeg falhou: {}", stderr.trim())));
    }
    Ok(())
}

fn texto(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Estabiliza `entrada` em `saida`. `progresso(fase, fração)`: fase 1 = análise, 2 = gravação.
pub fn estabilizar(
    entrada: &Path,
    saida: &Path,
    op: OpcoesEstabilizar,
    total_s: f64,
    tem_audio: bool,
    dimensoes: (u32, u32),
    usar_vidstab: bool,
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(u8, f64),
) -> Result<Estabilizado> {
    let suav = op.suavizar.clamp(1, 60);
    let tmp = PastaTemp::nova()?;
    let mut comandos = Vec::new();
    let audio: Vec<String> = if tem_audio {
        ["-map", "0:a?", "-c:a", "aac", "-b:a", "192k"].iter().map(|s| s.to_string()).collect()
    } else {
        vec![]
    };
    if usar_vidstab && tem_filtro("vidstabdetect") && tem_filtro("vidstabtransform") {
        // O arquivo de movimento fica na pasta temporária e é citado só pelo nome (o ":" de um
        // caminho do Windows atrapalha a sintaxe do filtro).
        let a1: Vec<String> = vec![
            "-i".into(), texto(entrada), "-map".into(), "0:v:0".into(), "-vf".into(),
            "vidstabdetect=shakiness=6:accuracy=15:result=movimento.trf".into(), "-f".into(), "null".into(), "-".into(),
        ];
        comandos.push(format!("ffmpeg {}", a1.join(" ")));
        rodar(&a1, Some(tmp.0.as_path()), total_s, cancelar, &mut |f| progresso(1, f))?;
        let ajuste = match op.borda {
            Borda::Cortar => "optzoom=1:zoom=0:crop=black",
            Borda::Preencher => "optzoom=0:zoom=0:crop=keep",
        };
        let vf = format!("vidstabtransform=input=movimento.trf:smoothing={suav}:{ajuste}:interpol=bicubic,unsharp=5:5:0.6:3:3:0.3");
        let mut a2: Vec<String> = vec!["-i".into(), texto(entrada), "-map".into(), "0:v:0".into(), "-vf".into(), vf];
        a2.extend(CODEC.iter().map(|s| s.to_string()));
        a2.extend(audio);
        a2.extend(["-movflags".into(), "+faststart".into(), "-y".into(), texto(saida)]);
        comandos.push(format!("ffmpeg {}", a2.join(" ")));
        rodar(&a2, Some(tmp.0.as_path()), total_s, cancelar, &mut |f| progresso(2, f))?;
        return Ok(Estabilizado { motor: "vidstab", comandos });
    }
    if !tem_filtro("deshake") {
        return Err(SicroError::Validation("este ffmpeg não tem o vidstab nem o deshake".into()));
    }
    // Sem vidstab: uma passada só, com o deshake (mais simples, só translação).
    // o deshake só aceita raio múltiplo de 16, de 16 a 64
    let raio = (suav / 2).max(1).div_ceil(16).clamp(1, 4) * 16;
    let vf = match op.borda {
        Borda::Preencher => format!("deshake=rx={raio}:ry={raio}:edge=mirror"),
        Borda::Cortar => {
            let (w, h) = dimensoes;
            format!(
                "deshake=rx={raio}:ry={raio}:edge=blank,crop=trunc(iw*0.92/2)*2:trunc(ih*0.92/2)*2,scale={}:{}",
                w / 2 * 2,
                h / 2 * 2
            )
        }
    };
    let mut a: Vec<String> = vec!["-i".into(), texto(entrada), "-map".into(), "0:v:0".into(), "-vf".into(), vf];
    a.extend(CODEC.iter().map(|s| s.to_string()));
    a.extend(audio);
    a.extend(["-movflags".into(), "+faststart".into(), "-y".into(), texto(saida)]);
    comandos.push(format!("ffmpeg {}", a.join(" ")));
    rodar(&a, Some(tmp.0.as_path()), total_s, cancelar, &mut |f| progresso(2, f))?;
    Ok(Estabilizado { motor: "deshake", comandos })
}

/// Cópia do vídeo com a lente corrigida pelo filtro `lenscorrection`.
pub fn corrigir_lente(
    entrada: &Path,
    saida: &Path,
    k1: f64,
    k2: f64,
    cx: f64,
    cy: f64,
    total_s: f64,
    tem_audio: bool,
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(f64),
) -> Result<String> {
    let vf = format!("lenscorrection=cx={cx:.5}:cy={cy:.5}:k1={k1:.6}:k2={k2:.6}:i=bilinear");
    let mut a: Vec<String> = vec!["-i".into(), texto(entrada), "-map".into(), "0:v:0".into(), "-vf".into(), vf];
    a.extend(CODEC.iter().map(|s| s.to_string()));
    if tem_audio {
        a.extend(["-map", "0:a?", "-c:a", "aac", "-b:a", "192k"].iter().map(|s| s.to_string()));
    }
    a.extend(["-movflags".into(), "+faststart".into(), "-y".into(), texto(saida)]);
    let cmd = format!("ffmpeg {}", a.join(" "));
    rodar(&a, None, total_s, cancelar, progresso)?;
    Ok(cmd)
}

/// Pasta e nome únicos dentro de `dir` para `base.ext`.
pub fn nome_unico(dir: &Path, base: &str, ext: &str) -> PathBuf {
    let mut p = dir.join(format!("{base}.{ext}"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{base}_{n}.{ext}"));
        n += 1;
    }
    p
}
