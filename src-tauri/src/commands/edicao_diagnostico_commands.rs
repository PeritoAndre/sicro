//! Diagnóstico de vídeo (P0.9, arquitetura §7.7): mídias de teste das medições do webview (`tempo3`, `pool8`,
//! `compose`, `switch`) e gravação do resultado em `app_cache_dir()/edicao/diagnostico-<data>.json`.
//! As mídias saem do ffmpeg do app (`SICRO_FFMPEG` vence; senão o embutido; senão o PATH) por `Command` direto,
//! sem `drawtext` (o 7.0.2 do AppImage não tem).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::{Result, SicroError};
use crate::workspace::paths;

/// Progresso do preparo das mídias: `[feitas, total]`.
pub const EVENTO_MIDIAS: &str = "edicao:diagnostico_midias";
/// Sobe quando a receita muda: o cache antigo é refeito.
pub const VERSAO_MIDIAS: u32 = 2;
const PASTA_MIDIAS: &str = "diagnostico";
const MANIFESTO: &str = "pronto.json";
/// Duração dos vídeos grandes (compose, pool8, switch), igual à bancada do P5.
pub const DURACAO_PADRAO_S: u32 = 30;

/// Variáveis do ambiente que mudam a composição do webview (§7.7).
const VARIAVEIS: &[&str] = &[
    "WEBKIT_DISABLE_DMABUF_RENDERER",
    "WEBKIT_DMABUF_RENDERER_FORCE_SHM",
    "WEBKIT_DISABLE_COMPOSITING_MODE",
    "GDK_BACKEND",
    "XDG_SESSION_TYPE",
    "LIBGL_ALWAYS_SOFTWARE",
    "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
    "SICRO_FFMPEG",
];

static PREPARO: Mutex<()> = Mutex::new(());

/// Uma mídia de teste. `p0_s` = pts do primeiro quadro de vídeo, lido pelo ffprobe (alvo do modo "previsto" do
/// `tempo3`); `inicio_s` = início do contêiner (o WebKitGTK rebaseia o Matroska por ele; muda com a versão do ffmpeg).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MidiaDiagnostico {
    pub nome: String,
    pub caminho: String,
    pub p0_s: f64,
    pub inicio_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Manifesto {
    versao: u32,
    ffmpeg: String,
    duracao_s: u32,
    midias: Vec<MidiaDiagnostico>,
}

/// Sistema, webview e variáveis de ambiente que o JS não enxerga.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AmbienteDiagnostico {
    pub so: String,
    pub arch: String,
    pub webview: Option<String>,
    pub sicro: String,
    pub variaveis: BTreeMap<String, Option<String>>,
}

/// Resposta de `edicao_diagnostico_midias`. `ffmpeg` = primeira linha do `-version` do binário que gerou.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MidiasDiagnostico {
    pub pasta: String,
    pub ffmpeg: String,
    pub midias: Vec<MidiaDiagnostico>,
    pub ambiente: AmbienteDiagnostico,
}

/// Prepara (ou reaproveita) as mídias de teste em `app_cache_dir()/edicao/diagnostico/`.
#[tauri::command]
pub async fn edicao_diagnostico_midias(app: AppHandle) -> Result<MidiasDiagnostico> {
    let pasta = cache_edicao(&app)?.join(PASTA_MIDIAS);
    let ffmpeg = escolher_ffmpeg(std::env::var_os("SICRO_FFMPEG"), || crate::tools::find_ffmpeg_tool("ffmpeg"))?;
    let app2 = app.clone();
    let m = tauri::async_runtime::spawn_blocking(move || {
        preparar(&pasta, &ffmpeg, DURACAO_PADRAO_S, &mut |feitas, total| {
            let _ = app2.emit(EVENTO_MIDIAS, [feitas, total]);
        })
        .map(|m| (pasta, m))
    })
    .await
    .map_err(|e| SicroError::Filesystem(format!("diagnóstico: {e}")))??;
    let (pasta, m) = m;
    Ok(MidiasDiagnostico {
        pasta: pasta.to_string_lossy().into_owned(),
        ffmpeg: m.ffmpeg,
        midias: m.midias,
        ambiente: ambiente(&app),
    })
}

/// Grava o JSON das medições em `app_cache_dir()/edicao/diagnostico-<data>.json` e devolve o caminho absoluto.
#[tauri::command]
pub async fn edicao_diagnostico_salvar(app: AppHandle, json: Value) -> Result<String> {
    let pasta = cache_edicao(&app)?;
    let p = salvar_em(&pasta, &json, chrono::Local::now().naive_local())?;
    Ok(p.to_string_lossy().into_owned())
}

fn cache_edicao(app: &AppHandle) -> Result<PathBuf> {
    let c = app.path().app_cache_dir().map_err(|e| SicroError::Filesystem(format!("cache dir: {e}")))?;
    Ok(paths::dir_edicao_cache_raiz(&c))
}

fn ambiente(app: &AppHandle) -> AmbienteDiagnostico {
    AmbienteDiagnostico {
        so: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        webview: tauri::webview_version().ok(),
        sicro: app.package_info().version.to_string(),
        variaveis: VARIAVEIS.iter().map(|k| (k.to_string(), std::env::var(k).ok())).collect(),
    }
}

/// `SICRO_FFMPEG` definido vence sempre (D13); inexistente é erro, nunca queda calada.
fn escolher_ffmpeg(configurado: Option<OsString>, achar: impl FnOnce() -> Option<PathBuf>) -> Result<PathBuf> {
    if let Some(c) = configurado.filter(|c| !c.is_empty()) {
        let p = PathBuf::from(c);
        return if p.is_file() {
            Ok(p)
        } else {
            Err(SicroError::Validation(format!("SICRO_FFMPEG não existe: {}", p.display())))
        };
    }
    achar().ok_or_else(|| SicroError::Validation("ffmpeg_ausente".into()))
}

/// ffprobe da mesma pasta do ffmpeg (D13); senão o que o app acharia.
fn ffprobe_de(ffmpeg: &Path) -> Option<PathBuf> {
    let nome = if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" };
    ffmpeg
        .parent()
        .map(|d| d.join(nome))
        .filter(|p| p.is_file())
        .or_else(|| crate::tools::find_ffmpeg_tool("ffprobe"))
}

fn versao(ffmpeg: &Path) -> Result<String> {
    let out = crate::tools::command(ffmpeg)
        .args(["-hide_banner", "-version"])
        .output()
        .map_err(|e| SicroError::Filesystem(format!("{}: {e}", ffmpeg.display())))?;
    Ok(String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or_default().trim().to_string())
}

/// Uma saída e os argumentos do ffmpeg entre `-y` e o arquivo de saída.
struct Receita {
    nome: &'static str,
    args: Vec<String>,
}

fn a(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// Código binário do quadro: 12 blocos de lado `b` no canto superior esquerdo (bit i = bloco i branco),
/// o mesmo que `readFrameNo` da bancada lê.
fn barras(b: u32) -> String {
    let mut f = format!("drawbox=x=0:y=0:w={}:h={b}:color=black:t=fill", 12 * b);
    for i in 0..12 {
        f.push_str(&format!(
            ",drawbox=x={}:y=0:w={b}:h={b}:color=white:t=fill:enable='eq(mod(floor(n/pow(2\\,{i}))\\,2)\\,1)'",
            i * b
        ));
    }
    f
}

fn grande(padrao: &str, hz: u32, gop: u32, taxa: &str, dur: u32) -> Vec<String> {
    let mut v = a(&["-f", "lavfi", "-i"]);
    v.push(format!("{padrao}=size=1920x1080:rate=30:duration={dur}"));
    v.extend(a(&["-f", "lavfi", "-i"]));
    v.push(format!("sine=frequency={hz}:sample_rate=48000:duration={dur}"));
    v.push("-vf".into());
    v.push(format!("noise=alls=12:allf=t,{},format=yuv420p", barras(40)));
    v.extend(a(&["-c:v", "libx264", "-preset", "veryfast", "-profile:v", "high"]));
    v.extend(["-b:v", taxa, "-maxrate", taxa, "-bufsize", taxa].map(String::from));
    let g = gop.to_string();
    v.extend(["-g", &g, "-keyint_min", &g].map(String::from));
    v.extend(a(&["-sc_threshold", "0", "-bf", "2", "-af", "volume=-30dB", "-c:a", "aac", "-b:a", "96k", "-movflags", "+faststart"]));
    v
}

fn proxy720(fonte: &Path) -> Vec<String> {
    let mut v = vec!["-i".into(), fonte.to_string_lossy().into_owned()];
    v.extend(a(&[
        "-vf", "scale=-2:720", "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-g", "15", "-keyint_min", "15",
        "-sc_threshold", "0", "-bf", "0", "-c:a", "aac", "-b:a", "96k", "-movflags", "+faststart",
    ]));
    v
}

/// r*: 640×360, 30 qps, 180 quadros; metade esquerda = 20+13·⌊N/16⌋, direita = 20+13·(N mod 16) (página `tempo3`).
fn base_r(bf: u32) -> Vec<String> {
    let mut v = a(&[
        "-f", "lavfi", "-i", "color=c=black:s=640x360:r=30:d=6",
        "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=6",
        "-vf", "format=yuv420p,geq=lum='if(lt(X,W/2),20+13*floor(N/16),20+13*mod(N,16))':cb=128:cr=128",
        "-c:v", "libx264", "-preset", "veryfast", "-profile:v", "high", "-crf", "16", "-g", "30", "-bf",
    ]);
    v.push(bf.to_string());
    v.extend(a(&["-ac", "1", "-c:a", "aac", "-b:a", "64k", "-movflags", "+faststart"]));
    v
}

fn copia(fonte: &Path, extra: &[&str]) -> Vec<String> {
    let mut v = vec!["-i".into(), fonte.to_string_lossy().into_owned(), "-c".into(), "copy".into()];
    v.extend(a(extra));
    v
}

/// Remux para Matroska com o vídeo no início da fonte. O AAC começa 1024 amostras antes; sem `disabled` o 7.0.2
/// empurra tudo 21 ms (o 9.0.1 grava o áudio negativo, como as mídias medidas no dossiê).
fn copia_mkv(fonte: &Path, extra: &[&str]) -> Vec<String> {
    let mut v = copia(fonte, extra);
    v.extend(a(&["-avoid_negative_ts", "disabled"]));
    v
}

/// Cópia de prévia por remux (§7.1).
fn copia_previa(fonte: &Path) -> Vec<String> {
    let mut v = vec!["-i".into(), fonte.to_string_lossy().into_owned()];
    v.extend(a(&[
        "-map", "0:v:0", "-map", "0:a:0?", "-c:v", "copy", "-copyinkf", "-c:a", "copy", "-avoid_negative_ts", "make_zero",
        "-movflags", "+faststart",
    ]));
    v
}

fn receitas(p: &Path, dur: u32) -> Vec<Receita> {
    let r0 = p.join("r0.mp4");
    let rb = p.join("rb.mp4");
    let j = |n: &str| p.join(n);
    let mut v = vec![
        Receita { nome: "r0.mp4", args: base_r(0) },
        Receita { nome: "rb.mp4", args: base_r(2) },
        Receita { nome: "r0.mkv", args: copia_mkv(&r0, &[]) },
        Receita { nome: "rb.mkv", args: copia_mkv(&rb, &[]) },
        Receita { nome: "r3.mp4", args: copia(&r0, &["-output_ts_offset", "3", "-movflags", "+faststart"]) },
        Receita { nome: "r3.mov", args: copia(&r0, &["-output_ts_offset", "3", "-movflags", "+faststart"]) },
        Receita { nome: "r3.mkv", args: copia_mkv(&r0, &["-output_ts_offset", "3"]) },
        Receita { nome: "r3v.mkv", args: copia_mkv(&r0, &["-an", "-output_ts_offset", "3"]) },
        Receita {
            nome: "r3d.mkv",
            args: {
                let s = r0.to_string_lossy().into_owned();
                let mut v = a(&["-itsoffset", "3", "-i"]);
                v.push(s.clone());
                v.extend(a(&["-itsoffset", "2.5", "-i"]));
                v.push(s);
                v.extend(a(&["-map", "0:v", "-map", "1:a", "-c", "copy"]));
                v
            },
        },
        Receita { nome: "r007.mp4", args: copia(&r0, &["-output_ts_offset", "0.0666667", "-movflags", "+faststart"]) },
        Receita { nome: "r007.mkv", args: copia_mkv(&r0, &["-output_ts_offset", "0.0666667"]) },
        Receita { nome: "r12h.mp4", args: copia(&r0, &["-output_ts_offset", "43200", "-movflags", "+faststart"]) },
        Receita { nome: "r12h.mkv", args: copia_mkv(&r0, &["-output_ts_offset", "43200"]) },
        Receita { nome: "r0.ts", args: copia(&r0, &["-f", "mpegts"]) },
    ];
    for (nome, fonte) in [
        ("cp_r3_mkv.mp4", "r3.mkv"),
        ("cp_r3d_mkv.mp4", "r3d.mkv"),
        ("cp_r007_mkv.mp4", "r007.mkv"),
        ("cp_r0_ts.mp4", "r0.ts"),
        ("cp_r3_mp4.mp4", "r3.mp4"),
        ("cp_r12h_mkv.mp4", "r12h.mkv"),
    ] {
        v.push(Receita { nome, args: copia_previa(&j(fonte)) });
    }
    v.push(Receita { nome: "v1080_g30.mp4", args: grande("testsrc2", 440, 30, "8M", dur) });
    for (nome, padrao, hz) in [
        ("cam1.mp4", "testsrc2", 330),
        ("cam2.mp4", "smptehdbars", 440),
        ("cam3.mp4", "testsrc", 550),
        ("cam4.mp4", "pal75bars", 660),
    ] {
        v.push(Receita { nome, args: grande(padrao, hz, 60, "6M", dur) });
    }
    for (nome, fonte) in [
        ("cam1_p720.mp4", "cam1.mp4"),
        ("cam2_p720.mp4", "cam2.mp4"),
        ("cam3_p720.mp4", "cam3.mp4"),
        ("cam4_p720.mp4", "cam4.mp4"),
    ] {
        v.push(Receita { nome, args: proxy720(&j(fonte)) });
    }
    v
}

fn rodar(ffmpeg: &Path, args: &[String], saida: &Path) -> Result<()> {
    let out = crate::tools::command(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y"])
        .args(args)
        .arg(saida)
        .output()
        .map_err(|e| SicroError::Filesystem(format!("{}: {e}", ffmpeg.display())))?;
    if !out.status.success() || !saida.is_file() {
        let err = String::from_utf8_lossy(&out.stderr);
        let fim: Vec<&str> = err.lines().rev().take(4).collect();
        return Err(SicroError::Filesystem(format!(
            "ffmpeg {}: {}",
            saida.file_name().unwrap_or_default().to_string_lossy(),
            fim.into_iter().rev().collect::<Vec<_>>().join(" | ")
        )));
    }
    Ok(())
}

/// pts (s) do primeiro quadro na ordem de exibição: o menor dos primeiros quadros decodificados.
fn p0(ffprobe: &Path, arquivo: &Path) -> Result<f64> {
    let out = crate::tools::command(ffprobe)
        .args(["-v", "error", "-select_streams", "v:0", "-read_intervals", "%+#8", "-show_entries", "frame=pts_time", "-of", "csv=p=0"])
        .arg(arquivo)
        .output()
        .map_err(|e| SicroError::Filesystem(format!("{}: {e}", ffprobe.display())))?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split(',').next().and_then(|x| x.trim().parse::<f64>().ok()))
        .reduce(f64::min)
        .ok_or_else(|| SicroError::Filesystem(format!("ffprobe sem quadro: {}", arquivo.display())))
}

/// `start_time` do contêiner.
fn inicio(ffprobe: &Path, arquivo: &Path) -> Result<f64> {
    let out = crate::tools::command(ffprobe)
        .args(["-v", "error", "-show_entries", "format=start_time", "-of", "csv=p=0"])
        .arg(arquivo)
        .output()
        .map_err(|e| SicroError::Filesystem(format!("{}: {e}", ffprobe.display())))?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .trim_end_matches(',')
        .parse::<f64>()
        .map_err(|_| SicroError::Filesystem(format!("ffprobe sem início: {}", arquivo.display())))
}

fn manifesto_valido(pasta: &Path, dur: u32) -> Option<Manifesto> {
    let m: Manifesto = serde_json::from_slice(&std::fs::read(pasta.join(MANIFESTO)).ok()?).ok()?;
    let ok = m.versao == VERSAO_MIDIAS
        && m.duracao_s == dur
        && m.midias.len() == receitas(pasta, dur).len()
        && m.midias.iter().all(|x| std::fs::metadata(&x.caminho).is_ok_and(|f| f.is_file() && f.len() > 0));
    ok.then_some(m)
}

/// Reaproveita as mídias se o manifesto vale; senão gera todas e grava o manifesto por último.
fn preparar(pasta: &Path, ffmpeg: &Path, dur: u32, progresso: &mut dyn FnMut(usize, usize)) -> Result<Manifesto> {
    let _g = PREPARO.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(m) = manifesto_valido(pasta, dur) {
        return Ok(m);
    }
    let rs = receitas(pasta, dur);
    // só o que esta função grava: a pasta pode vir de fora (bancada)
    for nome in std::iter::once(MANIFESTO).chain(rs.iter().map(|r| r.nome)) {
        let _ = std::fs::remove_file(pasta.join(nome));
    }
    std::fs::create_dir_all(pasta).map_err(|e| SicroError::Filesystem(format!("criar {}: {e}", pasta.display())))?;
    let ffprobe = ffprobe_de(ffmpeg).ok_or_else(|| SicroError::Validation("ffprobe_ausente".into()))?;
    let total = rs.len();
    progresso(0, total);
    let mut midias = Vec::with_capacity(total);
    for (i, r) in rs.iter().enumerate() {
        let saida = pasta.join(r.nome);
        rodar(ffmpeg, &r.args, &saida)?;
        midias.push(MidiaDiagnostico {
            nome: r.nome.to_string(),
            caminho: saida.to_string_lossy().into_owned(),
            p0_s: p0(&ffprobe, &saida)?,
            inicio_s: inicio(&ffprobe, &saida)?,
        });
        progresso(i + 1, total);
    }
    let m = Manifesto { versao: VERSAO_MIDIAS, ffmpeg: versao(ffmpeg)?, duracao_s: dur, midias };
    std::fs::write(pasta.join(MANIFESTO), serde_json::to_vec_pretty(&m)?)?;
    Ok(m)
}

/// `diagnostico-AAAA-MM-DD-HHMMSS.json`; no mesmo segundo ganha `-2`, `-3`…
fn salvar_em(pasta: &Path, json: &Value, quando: chrono::NaiveDateTime) -> Result<PathBuf> {
    if !json.is_object() {
        return Err(SicroError::Validation("diagnostico_invalido".into()));
    }
    std::fs::create_dir_all(pasta).map_err(|e| SicroError::Filesystem(format!("criar {}: {e}", pasta.display())))?;
    let base = format!("diagnostico-{}", quando.format("%Y-%m-%d-%H%M%S"));
    let bytes = serde_json::to_vec_pretty(json)?;
    for n in 1..1000 {
        let nome = if n == 1 { format!("{base}.json") } else { format!("{base}-{n}.json") };
        let p = pasta.join(nome);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(mut f) => {
                use std::io::Write;
                f.write_all(&bytes)?;
                return Ok(p);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(SicroError::Filesystem(format!("{base}: nomes esgotados")))
}

/// Gera as mídias numa pasta dada, fora do app (bancada do dossiê):
/// `SICRO_DIAG_PASTA=<pasta> cargo test --lib edicao_diagnostico_commands::bancada -- --ignored --nocapture`.
#[cfg(test)]
mod bancada {
    #[test]
    #[ignore]
    fn gerar_para_bancada() {
        let pasta = std::path::PathBuf::from(std::env::var("SICRO_DIAG_PASTA").expect("SICRO_DIAG_PASTA"));
        let ff = super::escolher_ffmpeg(std::env::var_os("SICRO_FFMPEG"), || crate::tools::find_ffmpeg_tool("ffmpeg")).unwrap();
        let m = super::preparar(&pasta, &ff, super::DURACAO_PADRAO_S, &mut |a, b| eprintln!("{a}/{b}")).unwrap();
        println!("{}", serde_json::to_string(&m).unwrap());
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::process::Command;

    #[test]
    fn comandos_registraveis() {
        let _h: Box<dyn Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync> =
            Box::new(tauri::generate_handler![edicao_diagnostico_midias, edicao_diagnostico_salvar]);
    }

    #[test]
    fn salvar_grava_com_nome_de_data_e_nao_sobrescreve() {
        let d = tempfile::tempdir().unwrap();
        let pasta = d.path().join("edicao");
        let quando = chrono::NaiveDate::from_ymd_opt(2026, 11, 14).unwrap().and_hms_opt(3, 7, 9).unwrap();
        let j = serde_json::json!({ "tempo3": { "erros": 0 }, "pool8": [], "compose": [], "switch": [], "data": "x", "n": 0.1 + 0.2 });
        let p1 = salvar_em(&pasta, &j, quando).unwrap();
        let p2 = salvar_em(&pasta, &j, quando).unwrap();
        assert_eq!(p1.file_name().unwrap(), "diagnostico-2026-11-14-030709.json");
        assert_eq!(p2.file_name().unwrap(), "diagnostico-2026-11-14-030709-2.json");
        let lido: Value = serde_json::from_slice(&std::fs::read(&p1).unwrap()).unwrap();
        assert_eq!(lido, j);
        assert!(salvar_em(&pasta, &serde_json::json!([1, 2]), quando).is_err());
    }

    #[test]
    fn sicro_ffmpeg_vence_e_inexistente_e_erro() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("ffmpeg");
        std::fs::write(&f, b"").unwrap();
        let outro = || Some(PathBuf::from("/outro/ffmpeg"));
        assert_eq!(escolher_ffmpeg(Some(f.clone().into_os_string()), outro).unwrap(), f);
        assert!(escolher_ffmpeg(Some(d.path().join("nao").into_os_string()), outro).is_err());
        assert_eq!(escolher_ffmpeg(None, outro).unwrap(), PathBuf::from("/outro/ffmpeg"));
        assert_eq!(escolher_ffmpeg(Some(OsString::new()), outro).unwrap(), PathBuf::from("/outro/ffmpeg"));
        assert!(escolher_ffmpeg(None, || None).is_err());
    }

    /// O binário do teste: `SICRO_FFMPEG`, senão o do app. Com `SICRO_EXIGE_FFMPEG=1` a falta é falha.
    fn ffmpeg_do_teste() -> Option<PathBuf> {
        match escolher_ffmpeg(std::env::var_os("SICRO_FFMPEG"), || crate::tools::find_ffmpeg_tool("ffmpeg")) {
            Ok(p) => Some(p),
            Err(e) => {
                assert!(std::env::var("SICRO_EXIGE_FFMPEG").as_deref() != Ok("1"), "ffmpeg exigido e ausente: {e}");
                eprintln!("sem ffmpeg: teste dispensado");
                None
            }
        }
    }

    /// Quadros `ns` em yuv420p cru, na ordem de exibição.
    fn quadros_yuv(ff: &Path, arq: &Path, w: usize, h: usize, ns: &[u32]) -> Vec<Vec<u8>> {
        let sel: Vec<String> = ns.iter().map(|n| format!("eq(n\\,{n})")).collect();
        let out = Command::new(ff)
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
            .arg(arq)
            .args(["-map", "0:v:0", "-vf"])
            .arg(format!("select='{}',format=yuv420p", sel.join("+")))
            .args(["-fps_mode", "passthrough", "-f", "rawvideo", "-"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let t = w * h * 3 / 2;
        assert_eq!(out.stdout.len(), t * ns.len(), "{}: quadros lidos", arq.display());
        out.stdout.chunks(t).map(|c| c.to_vec()).collect()
    }

    fn y_u_v(q: &[u8], w: usize, h: usize, x: usize, y: usize) -> (u8, u8, u8) {
        let uo = w * h;
        let vo = uo + (w / 2) * (h / 2);
        (q[y * w + x], q[uo + (y / 2) * (w / 2) + x / 2], q[vo + (y / 2) * (w / 2) + x / 2])
    }

    fn pacotes(ffprobe: &Path, arq: &Path) -> u32 {
        let out = Command::new(ffprobe)
            .args(["-v", "error", "-select_streams", "v:0", "-count_packets", "-show_entries", "stream=nb_read_packets", "-of", "csv=p=0"])
            .arg(arq)
            .output()
            .unwrap();
        // TS repete a seção do programa: vale a primeira linha
        let s = String::from_utf8_lossy(&out.stdout);
        s.lines().find(|l| !l.trim().is_empty()).unwrap_or_default().trim().trim_end_matches(',').parse().unwrap()
    }

    fn primeiro_audio(ffprobe: &Path, arq: &Path) -> f64 {
        let out = Command::new(ffprobe)
            .args(["-v", "error", "-select_streams", "a:0", "-read_intervals", "%+#1", "-show_entries", "packet=pts_time", "-of", "csv=p=0"])
            .arg(arq)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or_default().trim().trim_end_matches(',').parse().unwrap()
    }

    /// Lê o código de barras do quadro (bloco de lado `b`; bit = Y no centro do bloco acima de 128) e confere que
    /// os blocos são neutros (branco e preto, sem cor).
    fn codigo(q: &[u8], w: usize, h: usize, b: f64) -> u32 {
        let mut n = 0;
        for i in 0..12 {
            let x = (i as f64 * b + b / 2.0) as usize;
            let (y, u, v) = y_u_v(q, w, h, x, (b / 2.0) as usize);
            assert!(u.abs_diff(128) <= 3 && v.abs_diff(128) <= 3, "bloco {i} com cor: u={u} v={v}");
            assert!(!(40..=200).contains(&y), "bloco {i} ambíguo: y={y}");
            if y > 128 {
                n |= 1 << i;
            }
        }
        n
    }

    /// Mídias geradas pelo ffmpeg escolhido: início (pts do 1º quadro), número de quadros, quadro certo (código e
    /// níveis de cinza lidos no pixel) e cor neutra. Roda nos dois binários (`SICRO_FFMPEG` escolhe).
    #[test]
    fn midias_conferidas_no_pixel() {
        let Some(ff) = ffmpeg_do_teste() else { return };
        let ver = versao(&ff).unwrap();
        eprintln!("ffmpeg: {} ({ver})", ff.display());
        let d = tempfile::tempdir().unwrap();
        let pasta = d.path().join("cache com espaço/diagnóstico");
        let dur = 3;
        let mut passos = Vec::new();
        let m = preparar(&pasta, &ff, dur, &mut |a, b| passos.push((a, b))).unwrap();
        assert_eq!(m.ffmpeg, ver, "gerado pelo binário escolhido");
        let total = receitas(&pasta, dur).len();
        assert_eq!(m.midias.len(), total);
        assert_eq!(passos.first(), Some(&(0, total)));
        assert_eq!(passos.last(), Some(&(total, total)));
        let por = |n: &str| m.midias.iter().find(|x| x.nome == n).unwrap_or_else(|| panic!("falta {n}"));
        for x in &m.midias {
            eprintln!("  {} p0 {:.6} início {:.6}", x.nome, x.p0_s, x.inicio_s);
        }

        // início do vídeo em cada contêiner (o que a página compara com o tempo do elemento)
        for (n, p0) in [
            ("r0.mp4", 0.0), ("r0.mkv", 0.0), ("rb.mp4", 0.0), ("r3.mp4", 3.0), ("r3.mov", 3.0), ("r3.mkv", 3.0),
            ("r3v.mkv", 3.0), ("r3d.mkv", 3.0), ("r007.mp4", 0.0667), ("r007.mkv", 0.067), ("r12h.mkv", 43200.0),
            ("r12h.mp4", 43200.0),
        ] {
            let d = por(n).p0_s - p0;
            assert!(d.abs() < 1.5e-3, "{n}: p0 {} ≠ {p0}", por(n).p0_s);
        }
        assert!(por("r0.ts").p0_s > 0.5, "TS começa depois de zero");
        // início do contêiner: nunca depois do vídeo; r3d começa no áudio (2,5 s); o AAC sem `CodecDelay` do 7.0.2
        // adianta o Matroska 21 ms (2,979 em vez de 3)
        for x in &m.midias {
            assert!(x.inicio_s <= x.p0_s + 1e-3, "{}: início {} depois do vídeo {}", x.nome, x.inicio_s, x.p0_s);
        }
        assert!((2.47..2.51).contains(&por("r3d.mkv").inicio_s), "r3d.mkv: início {}", por("r3d.mkv").inicio_s);
        assert!((2.97..3.001).contains(&por("r3.mkv").inicio_s), "r3.mkv: início {}", por("r3.mkv").inicio_s);
        assert!(por("r0.mp4").inicio_s.abs() < 1e-3);
        let fp = ffprobe_de(&ff).unwrap();
        let a0 = primeiro_audio(&fp, Path::new(&por("r3d.mkv").caminho));
        assert!((2.47..2.51).contains(&a0), "r3d.mkv: áudio em {a0}");
        for n in ["cp_r3_mkv.mp4", "cp_r3d_mkv.mp4", "cp_r007_mkv.mp4", "cp_r0_ts.mp4", "cp_r3_mp4.mp4", "cp_r12h_mkv.mp4"] {
            assert!(por(n).p0_s < 0.6, "{n}: cópia começa perto de zero ({})", por(n).p0_s);
        }

        // r*: 180 quadros; níveis de cinza do quadro N nas duas metades; cópia de prévia = mesmos quadros
        let (w, h) = (640, 360);
        for n in ["r0.mp4", "r3.mkv", "r3d.mkv", "r12h.mp4", "r0.ts", "rb.mkv", "cp_r3_mkv.mp4", "cp_r0_ts.mp4", "cp_r12h_mkv.mp4"] {
            let arq = Path::new(&por(n).caminho);
            assert_eq!(pacotes(&fp, arq), 180, "{n}: quadros");
            let ns = [0u32, 1, 15, 16, 77, 179];
            for (q, k) in quadros_yuv(&ff, arq, w, h, &ns).iter().zip(ns) {
                let (ye, ue, ve) = y_u_v(q, w, h, w / 4, h / 2);
                let (yd, ud, vd) = y_u_v(q, w, h, 3 * w / 4, h / 2);
                let (esq, dir) = (20 + 13 * (k / 16), 20 + 13 * (k % 16));
                assert!(u32::from(ye).abs_diff(esq) <= 2 && u32::from(yd).abs_diff(dir) <= 2, "{n} quadro {k}: {ye}/{yd} ≠ {esq}/{dir}");
                for c in [ue, ve, ud, vd] {
                    assert!(c.abs_diff(128) <= 2, "{n} quadro {k}: croma {c}");
                }
            }
        }

        // vídeos grandes: código do quadro lido no pixel, 1080p e proxy 720p
        let ns = [0u32, 1, 2, 5, 37, 64, 89];
        for (n, w, h) in [("v1080_g30.mp4", 1920, 1080), ("cam2.mp4", 1920, 1080), ("cam4.mp4", 1920, 1080), ("cam1_p720.mp4", 1280, 720), ("cam3_p720.mp4", 1280, 720)] {
            let arq = Path::new(&por(n).caminho);
            assert_eq!(pacotes(&fp, arq), dur * 30, "{n}: quadros");
            for (q, k) in quadros_yuv(&ff, arq, w, h, &ns).iter().zip(ns) {
                assert_eq!(codigo(q, w, h, w as f64 / 48.0), k, "{n}: código do quadro {k}");
            }
        }

        // segunda chamada reaproveita (manifesto válido, nada regravado)
        let antes = std::fs::metadata(pasta.join("v1080_g30.mp4")).unwrap().modified().unwrap();
        let mut chamou = false;
        let m2 = preparar(&pasta, &ff, dur, &mut |_, _| chamou = true).unwrap();
        assert_eq!(m2, m);
        assert!(!chamou);
        assert_eq!(std::fs::metadata(pasta.join("v1080_g30.mp4")).unwrap().modified().unwrap(), antes);
        // arquivo zerado ou apagado: refaz
        std::fs::write(pasta.join("cam3.mp4"), b"").unwrap();
        assert!(manifesto_valido(&pasta, dur).is_none());
        std::fs::remove_file(pasta.join("cam4.mp4")).unwrap();
        assert!(manifesto_valido(&pasta, dur).is_none());
    }
}
