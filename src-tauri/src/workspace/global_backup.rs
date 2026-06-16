//! Backup geral (todos os casos) — incremental, 1 arquivo por caso.
//!
//! Em vez de um único ZIP gigante de tudo (que pode passar de dezenas de
//! GB, recomprime tudo toda vez e, se corromper, leva tudo junto), o backup
//! geral mantém uma PASTA-ESPELHO num destino escolhido (ex.: HD externo):
//!
//!     <destino>/
//!         sicro-backup-index.json     ← índice do conjunto (+ fingerprints)
//!         backup_<label>_<id8>.sicrobackup   (1 por caso)
//!         ...
//!
//! Cada caso vira um `.sicrobackup` independente (reaproveita `create_backup`),
//! portanto continua verificável e restaurável isoladamente.
//!
//! INCREMENTAL: para cada caso calculamos um *fingerprint* barato (hash de
//! caminho+tamanho+mtime de todos os arquivos que entrariam no backup, com a
//! MESMA regra de skip do zip). Se o fingerprint bate com o do último backup
//! e o arquivo ainda existe no destino, o caso é PULADO — só recopiamos o que
//! mudou. A primeira rodada copia tudo (não tem mágica com vídeo/drone); as
//! seguintes só tocam no que mexeu.
//!
//! §13: o workspace original nunca é tocado; casos não encontrados
//! (movidos / HD desconectado) são reportados e o backup anterior deles é
//! PRESERVADO (nunca apagamos um backup só porque a origem sumiu).

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Result, SicroError};
use crate::workspace::backup::{create_backup, SKIP_DIRS};
use crate::workspace::manifest::{Manifest, APP_VERSION};

const INDEX_FILENAME: &str = "sicro-backup-index.json";
/// Subpasta onde ficam os `.sicrobackup` por caso (estrutura v2 do conjunto).
/// O índice fica na raiz; `config/` (snapshot do app-settings) é gravado pelo
/// comando. Conjunto auto-explicativo e fácil de restaurar.
const CASES_SUBDIR: &str = "casos";
const INDEX_FORMAT: &str = "sicro-global-backup";
const INDEX_FORMAT_VERSION: &str = "1.0";

/// Um caso a entrar no backup geral (vindo do índice de casos no front).
#[derive(Debug, Clone, Deserialize)]
pub struct GlobalCaseInput {
    pub workspace_path: String,
    /// Rótulo humano (BO/tipo/município) usado no nome do arquivo.
    #[serde(default)]
    pub label: String,
}

/// Evento de progresso emitido por caso (para a UI não congelar).
#[derive(Debug, Clone, Serialize)]
pub struct GlobalBackupProgress {
    pub index: u32,
    pub total: u32,
    pub label: String,
    /// "checking" | "backing_up" | "skipped" | "done" | "missing" | "error"
    pub phase: String,
}

/// Resultado por caso no relatório final.
#[derive(Debug, Clone, Serialize)]
pub struct CaseBackupResult {
    pub workspace_path: String,
    pub workspace_id: Option<String>,
    pub label: String,
    /// "backed_up" | "skipped_unchanged" | "missing" | "error"
    pub status: String,
    pub filename: Option<String>,
    pub size_bytes: u64,
    pub file_count: u32,
    pub error: Option<String>,
}

/// Relatório do backup geral.
#[derive(Debug, Clone, Serialize)]
pub struct GlobalBackupReport {
    pub destination: String,
    pub generated_at: DateTime<Utc>,
    pub total_cases: u32,
    pub backed_up: u32,
    pub skipped: u32,
    pub missing: u32,
    pub errors: u32,
    /// Tamanho de TODO o conjunto no destino (soma dos arquivos rastreados).
    pub total_size_bytes: u64,
    pub cases: Vec<CaseBackupResult>,
}

/// Entrada persistida no `sicro-backup-index.json` (por caso).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexCaseEntry {
    workspace_id: String,
    occurrence_id: String,
    label: String,
    source_path: String,
    filename: String,
    fingerprint: String,
    hash_sha256: String,
    size_bytes: u64,
    file_count: u32,
    backed_up_at: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct IndexFile {
    #[serde(default)]
    format: String,
    #[serde(default)]
    format_version: String,
    #[serde(default)]
    software_version: String,
    #[serde(default)]
    updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    cases: Vec<IndexCaseEntry>,
}

/// Fingerprint barato do workspace: hash de (caminho|tamanho|mtime) de todos
/// os arquivos que entrariam no backup, ordenados. Não lê o conteúdo dos
/// arquivos, só metadados — rápido mesmo com muita mídia.
pub fn workspace_fingerprint(root: &Path) -> Result<String> {
    let mut entries: Vec<String> = Vec::new();
    collect_fingerprint(root, root, &mut entries)?;
    entries.sort();
    let mut hasher = Sha256::new();
    for e in &entries {
        hasher.update(e.as_bytes());
        hasher.update(b"\n");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_fingerprint(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    let rd = fs::read_dir(dir)
        .map_err(|e| SicroError::Filesystem(format!("cannot read {}: {}", dir.display(), e)))?;
    for entry in rd {
        let entry =
            entry.map_err(|e| SicroError::Filesystem(format!("dir entry error: {e}")))?;
        let path = entry.path();
        let ft = entry
            .file_type()
            .map_err(|e| SicroError::Filesystem(format!("file_type error: {e}")))?;

        // Skip ephemeral/heavy + a própria pasta de backups — só no nível raiz,
        // exatamente como o zip do backup faz.
        if ft.is_dir() && dir == root {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if SKIP_DIRS.iter().any(|d| *d == name) || name == "backups" {
                    continue;
                }
            }
        }

        if ft.is_dir() {
            collect_fingerprint(root, &path, out)?;
        } else if ft.is_file() {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let meta = entry.metadata().map_err(|e| {
                SicroError::Filesystem(format!("metadata {}: {}", path.display(), e))
            })?;
            let size = meta.len();
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis())
                .unwrap_or(0);
            out.push(format!("{rel}|{size}|{mtime}"));
        }
        // symlinks/outros: ignorados (consistente com o zip).
    }
    Ok(())
}

fn load_index(index_path: &Path) -> HashMap<String, IndexCaseEntry> {
    let mut map = HashMap::new();
    if let Ok(bytes) = fs::read(index_path) {
        if let Ok(file) = serde_json::from_slice::<IndexFile>(&bytes) {
            for e in file.cases {
                map.insert(e.workspace_id.clone(), e);
            }
        }
    }
    map
}

fn write_index(
    index_path: &Path,
    map: &HashMap<String, IndexCaseEntry>,
    now: DateTime<Utc>,
) -> Result<()> {
    let mut cases: Vec<IndexCaseEntry> = map.values().cloned().collect();
    cases.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    let file = IndexFile {
        format: INDEX_FORMAT.to_string(),
        format_version: INDEX_FORMAT_VERSION.to_string(),
        software_version: APP_VERSION.to_string(),
        updated_at: Some(now),
        cases,
    };
    let json = serde_json::to_string_pretty(&file)
        .map_err(|e| SicroError::Workspace(format!("serialize backup index: {e}")))?;
    fs::write(index_path, json).map_err(|e| {
        SicroError::Filesystem(format!("write {}: {}", index_path.display(), e))
    })?;
    Ok(())
}

/// Executa o backup geral incremental. `on_progress` é chamado por caso.
pub fn run_global_backup<F: FnMut(GlobalBackupProgress)>(
    cases: &[GlobalCaseInput],
    destination: &Path,
    mut on_progress: F,
) -> Result<GlobalBackupReport> {
    fs::create_dir_all(destination).map_err(|e| {
        SicroError::Filesystem(format!(
            "cannot create backup dir {}: {}",
            destination.display(),
            e
        ))
    })?;
    // Os .sicrobackup por caso ficam em <destino>/casos/.
    let casos_dir = destination.join(CASES_SUBDIR);
    fs::create_dir_all(&casos_dir).map_err(|e| {
        SicroError::Filesystem(format!(
            "cannot create {}: {}",
            casos_dir.display(),
            e
        ))
    })?;
    let index_path = destination.join(INDEX_FILENAME);
    let mut index_map = load_index(&index_path);

    let now = Utc::now();
    let total = cases.len() as u32;
    let mut results: Vec<CaseBackupResult> = Vec::with_capacity(cases.len());
    let (mut n_backed, mut n_skip, mut n_missing, mut n_err) = (0u32, 0u32, 0u32, 0u32);

    for (i, case) in cases.iter().enumerate() {
        let idx = i as u32;
        let label = case.label.trim().to_string();
        on_progress(GlobalBackupProgress {
            index: idx,
            total,
            label: label.clone(),
            phase: "checking".into(),
        });

        let ws = PathBuf::from(&case.workspace_path);

        // Manifesto → ids. Falhou = caso ausente/movido → preserva backup antigo.
        let manifest = match Manifest::read(&ws) {
            Ok(m) => m,
            Err(_) => {
                n_missing += 1;
                results.push(CaseBackupResult {
                    workspace_path: case.workspace_path.clone(),
                    workspace_id: None,
                    label: label.clone(),
                    status: "missing".into(),
                    filename: None,
                    size_bytes: 0,
                    file_count: 0,
                    error: Some("workspace não encontrado ou manifesto ilegível".into()),
                });
                on_progress(GlobalBackupProgress {
                    index: idx,
                    total,
                    label,
                    phase: "missing".into(),
                });
                continue;
            }
        };
        let wid = manifest.workspace_id.to_string();
        let id8: String = wid.chars().take(8).collect();

        let fp = match workspace_fingerprint(&ws) {
            Ok(f) => f,
            Err(e) => {
                n_err += 1;
                results.push(CaseBackupResult {
                    workspace_path: case.workspace_path.clone(),
                    workspace_id: Some(wid.clone()),
                    label: label.clone(),
                    status: "error".into(),
                    filename: None,
                    size_bytes: 0,
                    file_count: 0,
                    error: Some(e.to_string()),
                });
                on_progress(GlobalBackupProgress {
                    index: idx,
                    total,
                    label,
                    phase: "error".into(),
                });
                continue;
            }
        };

        // Incremental: pula se fingerprint bate e o arquivo ainda existe.
        if let Some(prev) = index_map.get(&wid).cloned() {
            if prev.fingerprint == fp && casos_dir.join(&prev.filename).exists() {
                n_skip += 1;
                let updated = IndexCaseEntry {
                    label: label.clone(),
                    source_path: case.workspace_path.clone(),
                    ..prev.clone()
                };
                let fname = updated.filename.clone();
                let sz = updated.size_bytes;
                let fc = updated.file_count;
                index_map.insert(wid.clone(), updated);
                results.push(CaseBackupResult {
                    workspace_path: case.workspace_path.clone(),
                    workspace_id: Some(wid),
                    label: label.clone(),
                    status: "skipped_unchanged".into(),
                    filename: Some(fname),
                    size_bytes: sz,
                    file_count: fc,
                    error: None,
                });
                on_progress(GlobalBackupProgress {
                    index: idx,
                    total,
                    label,
                    phase: "skipped".into(),
                });
                continue;
            }
        }

        // Mudou (ou é novo): gera o .sicrobackup no destino.
        on_progress(GlobalBackupProgress {
            index: idx,
            total,
            label: label.clone(),
            phase: "backing_up".into(),
        });
        let slug = if label.is_empty() { "ocorrencia".to_string() } else { label.clone() };
        let bo_hint = format!("{slug}_{id8}");
        match create_backup(&ws, Some(&casos_dir), Some(&bo_hint)) {
            Ok(art) => {
                // Substituiu um arquivo de nome diferente do mesmo caso? Remove o antigo.
                if let Some(prev) = index_map.get(&wid) {
                    if prev.filename != art.filename {
                        let _ = fs::remove_file(casos_dir.join(&prev.filename));
                    }
                }
                n_backed += 1;
                index_map.insert(
                    wid.clone(),
                    IndexCaseEntry {
                        workspace_id: wid.clone(),
                        occurrence_id: manifest.occurrence_id.to_string(),
                        label: label.clone(),
                        source_path: case.workspace_path.clone(),
                        filename: art.filename.clone(),
                        fingerprint: fp,
                        hash_sha256: art.hash_sha256.clone(),
                        size_bytes: art.size_bytes,
                        file_count: art.file_count,
                        backed_up_at: now,
                    },
                );
                results.push(CaseBackupResult {
                    workspace_path: case.workspace_path.clone(),
                    workspace_id: Some(wid),
                    label: label.clone(),
                    status: "backed_up".into(),
                    filename: Some(art.filename),
                    size_bytes: art.size_bytes,
                    file_count: art.file_count,
                    error: None,
                });
                on_progress(GlobalBackupProgress {
                    index: idx,
                    total,
                    label,
                    phase: "done".into(),
                });
            }
            Err(e) => {
                n_err += 1;
                results.push(CaseBackupResult {
                    workspace_path: case.workspace_path.clone(),
                    workspace_id: Some(wid),
                    label: label.clone(),
                    status: "error".into(),
                    filename: None,
                    size_bytes: 0,
                    file_count: 0,
                    error: Some(e.to_string()),
                });
                on_progress(GlobalBackupProgress {
                    index: idx,
                    total,
                    label,
                    phase: "error".into(),
                });
            }
        }
    }

    let total_size: u64 = index_map.values().map(|e| e.size_bytes).sum();
    write_index(&index_path, &index_map, now)?;

    Ok(GlobalBackupReport {
        destination: destination.to_string_lossy().into_owned(),
        generated_at: now,
        total_cases: total,
        backed_up: n_backed,
        skipped: n_skip,
        missing: n_missing,
        errors: n_err,
        total_size_bytes: total_size,
        cases: results,
    })
}

// ---------------------------------------------------------------------------
// Restauração — lê um conjunto de backup (estrutura v2) e devolve os casos.
// GENÉRICO quanto à origem (HD externo, pendrive, nuvem, rede): é só uma pasta.
// §13: a origem NUNCA é modificada; não sobrescreve casos existentes (a menos
// que `overwrite`), preservando o que já está no disco do perito.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct RestoreProgress {
    pub index: u32,
    pub total: u32,
    pub label: String,
    pub phase: String,
}

#[derive(Debug, Serialize)]
pub struct RestoredCase {
    pub workspace_id: Option<String>,
    pub label: String,
    pub source_filename: String,
    /// "restored" | "skipped_exists" | "error"
    pub status: String,
    pub restored_path: Option<String>,
    pub file_count: u32,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RestoreReport {
    pub source: String,
    pub cases_parent: String,
    pub restored: u32,
    pub skipped: u32,
    pub errors: u32,
    pub cases: Vec<RestoredCase>,
}

/// Nome de pasta seguro a partir do caminho original do workspace (último
/// componente). Fallback: `restaurado_<id8>.sicro`.
fn restore_folder_name(source_path: &str, id8: &str) -> String {
    let trimmed = source_path.trim_end_matches(['/', '\\']);
    let base = trimmed.rsplit(['/', '\\']).next().unwrap_or("");
    if base.is_empty() {
        format!("restaurado_{id8}.sicro")
    } else {
        base.to_string()
    }
}

/// Nome de entrada de zip é seguro (sem zip-slip)?
fn safe_zip_name(name: &str) -> bool {
    !name.is_empty()
        && !name.split(['/', '\\']).any(|c| c == "..")
        && !name.starts_with('/')
        && !name.starts_with('\\')
        && !name.contains(':')
}

/// Lê o `_sicro_backup_manifest.json` de dentro do zip → (workspace_id, source_path).
fn read_inner_manifest(zip_path: &Path) -> Option<(String, String)> {
    let file = fs::File::open(zip_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name("_sicro_backup_manifest.json").ok()?;
    let mut s = String::new();
    entry.read_to_string(&mut s).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    let wid = v
        .get("workspace_id")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let src = v
        .get("source_workspace_path")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    Some((wid, src))
}

/// Extrai um `.sicrobackup` (zip) para `target_dir`, exceto o manifesto interno.
/// Devolve a contagem de arquivos extraídos.
fn extract_sicrobackup(zip_path: &Path, target_dir: &Path) -> Result<u32> {
    let file = fs::File::open(zip_path)
        .map_err(|e| SicroError::Filesystem(format!("abrir {}: {}", zip_path.display(), e)))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| SicroError::Workspace(format!("ler zip {}: {}", zip_path.display(), e)))?;
    let mut count = 0u32;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| SicroError::Workspace(format!("zip entry {i}: {e}")))?;
        let name = entry.name().to_string();
        if name == "_sicro_backup_manifest.json" {
            continue;
        }
        if !safe_zip_name(&name) {
            return Err(SicroError::Workspace(format!(
                "entrada de backup insegura: {name}"
            )));
        }
        let out_path = target_dir.join(&name);
        if name.ends_with('/') {
            fs::create_dir_all(&out_path).ok();
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SicroError::Filesystem(format!("criar {}: {}", parent.display(), e))
            })?;
        }
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| SicroError::Workspace(format!("ler {name} do zip: {e}")))?;
        fs::write(&out_path, &buf).map_err(|e| {
            SicroError::Filesystem(format!("gravar {}: {}", out_path.display(), e))
        })?;
        count += 1;
    }
    Ok(count)
}

/// Restaura os casos de um conjunto de backup para `cases_parent`.
pub fn run_restore<F: FnMut(RestoreProgress)>(
    source_dir: &Path,
    cases_parent: &Path,
    overwrite: bool,
    mut on_progress: F,
) -> Result<RestoreReport> {
    let casos_dir = source_dir.join(CASES_SUBDIR);
    if !casos_dir.is_dir() {
        return Err(SicroError::Workspace(format!(
            "pasta de backup inválida: '{}' não contém 'casos/'",
            source_dir.display()
        )));
    }
    fs::create_dir_all(cases_parent).map_err(|e| {
        SicroError::Filesystem(format!("criar {}: {}", cases_parent.display(), e))
    })?;

    // Índice (opcional) → rótulos por workspace_id.
    let index_map = load_index(&source_dir.join(INDEX_FILENAME));
    let label_by_wid: HashMap<String, String> = index_map
        .values()
        .map(|e| (e.workspace_id.clone(), e.label.clone()))
        .collect();

    // Lista os `.sicrobackup` em casos/.
    let mut backups: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = fs::read_dir(&casos_dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            if p.extension().and_then(|e| e.to_str()) == Some("sicrobackup") {
                backups.push(p);
            }
        }
    }
    backups.sort();

    let total = backups.len() as u32;
    let mut cases: Vec<RestoredCase> = Vec::with_capacity(backups.len());
    let (mut n_ok, mut n_skip, mut n_err) = (0u32, 0u32, 0u32);

    for (i, zip_path) in backups.iter().enumerate() {
        let idx = i as u32;
        let source_filename = zip_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (wid, src_path) = read_inner_manifest(zip_path).unwrap_or_default();
        let id8: String = wid.chars().take(8).collect();
        let label = label_by_wid
            .get(&wid)
            .cloned()
            .unwrap_or_else(|| source_filename.clone());

        on_progress(RestoreProgress {
            index: idx,
            total,
            label: label.clone(),
            phase: "restoring".into(),
        });

        let folder = restore_folder_name(&src_path, &id8);
        let target = cases_parent.join(&folder);
        let wid_opt = if wid.is_empty() { None } else { Some(wid.clone()) };

        if target.exists() && !overwrite {
            n_skip += 1;
            cases.push(RestoredCase {
                workspace_id: wid_opt,
                label: label.clone(),
                source_filename,
                status: "skipped_exists".into(),
                restored_path: Some(target.to_string_lossy().into_owned()),
                file_count: 0,
                error: None,
            });
            on_progress(RestoreProgress {
                index: idx,
                total,
                label,
                phase: "skipped".into(),
            });
            continue;
        }

        match extract_sicrobackup(zip_path, &target) {
            Ok(fc) => {
                n_ok += 1;
                cases.push(RestoredCase {
                    workspace_id: wid_opt,
                    label: label.clone(),
                    source_filename,
                    status: "restored".into(),
                    restored_path: Some(target.to_string_lossy().into_owned()),
                    file_count: fc,
                    error: None,
                });
                on_progress(RestoreProgress {
                    index: idx,
                    total,
                    label,
                    phase: "done".into(),
                });
            }
            Err(e) => {
                n_err += 1;
                cases.push(RestoredCase {
                    workspace_id: wid_opt,
                    label: label.clone(),
                    source_filename,
                    status: "error".into(),
                    restored_path: None,
                    file_count: 0,
                    error: Some(e.to_string()),
                });
                on_progress(RestoreProgress {
                    index: idx,
                    total,
                    label,
                    phase: "error".into(),
                });
            }
        }
    }

    Ok(RestoreReport {
        source: source_dir.to_string_lossy().into_owned(),
        cases_parent: cases_parent.to_string_lossy().into_owned(),
        restored: n_ok,
        skipped: n_skip,
        errors: n_err,
        cases,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn fake_workspace(dir: &Path, wid: &str, oid: &str) {
        fs::create_dir_all(dir.join("imagens").join("originais")).unwrap();
        fs::create_dir_all(dir.join("logs")).unwrap();
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "format": "sicro-workspace",
                "version": "2.0.0",
                "created_at": "2026-05-25T13:00:00Z",
                "updated_at": "2026-05-25T13:00:00Z",
                "workspace_id": wid,
                "occurrence_id": oid,
                "app_version": "test",
                "database": "sicro.sqlite",
                "integrity": {"strategy": "sha256", "manifest_hash": null},
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(dir.join("sicro.sqlite"), b"DB-BYTES").unwrap();
        fs::write(
            dir.join("imagens").join("originais").join("a.png"),
            &[1, 2, 3, 4],
        )
        .unwrap();
        fs::write(dir.join("logs").join("app.log"), b"ephemeral").unwrap();
    }

    fn no_progress(_p: GlobalBackupProgress) {}

    #[test]
    fn fingerprint_is_stable_and_ignores_logs() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("case.sicro");
        fake_workspace(&ws, "aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa", "bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb");
        let fp1 = workspace_fingerprint(&ws).unwrap();
        // Mexer só em logs/ não muda o fingerprint.
        fs::write(ws.join("logs").join("app.log"), b"different ephemeral content").unwrap();
        let fp2 = workspace_fingerprint(&ws).unwrap();
        assert_eq!(fp1, fp2, "logs/ não deve afetar o fingerprint");
        assert_eq!(fp1.len(), 64);
    }

    #[test]
    fn global_backup_creates_then_skips_unchanged() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("case.sicro");
        fake_workspace(&ws, "11111111-1111-4111-1111-111111111111", "22222222-2222-4222-2222-222222222222");
        let dest = tmp.path().join("backup-set");
        let cases = vec![GlobalCaseInput {
            workspace_path: ws.to_string_lossy().into_owned(),
            label: "BO 12/2026".into(),
        }];

        // 1ª rodada: copia.
        let r1 = run_global_backup(&cases, &dest, no_progress).unwrap();
        assert_eq!(r1.backed_up, 1);
        assert_eq!(r1.skipped, 0);
        assert!(dest.join(INDEX_FILENAME).is_file());
        let r1_file = r1.cases[0].filename.clone().unwrap();
        assert!(dest.join("casos").join(&r1_file).is_file());

        // 2ª rodada sem mudar nada: pula.
        let r2 = run_global_backup(&cases, &dest, no_progress).unwrap();
        assert_eq!(r2.backed_up, 0);
        assert_eq!(r2.skipped, 1, "caso inalterado deve ser pulado");

        // Mudar um arquivo de conteúdo → re-backup.
        fs::write(ws.join("imagens").join("originais").join("a.png"), &[9, 9, 9, 9, 9]).unwrap();
        let r3 = run_global_backup(&cases, &dest, no_progress).unwrap();
        assert_eq!(r3.backed_up, 1, "mudança real deve re-backupiar");
        assert_eq!(r3.skipped, 0);
    }

    #[test]
    fn global_backup_reports_missing_without_deleting_prior() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("case.sicro");
        fake_workspace(&ws, "33333333-3333-4333-3333-333333333333", "44444444-4444-4444-4444-444444444444");
        let dest = tmp.path().join("backup-set");
        let cases = vec![GlobalCaseInput {
            workspace_path: ws.to_string_lossy().into_owned(),
            label: "Caso X".into(),
        }];
        let r1 = run_global_backup(&cases, &dest, no_progress).unwrap();
        let file = dest.join("casos").join(r1.cases[0].filename.clone().unwrap());
        assert!(file.is_file());

        // Some com a origem (renomeia o .sicro) e roda de novo.
        fs::rename(&ws, tmp.path().join("moved.sicro")).unwrap();
        let r2 = run_global_backup(&cases, &dest, no_progress).unwrap();
        assert_eq!(r2.missing, 1);
        assert!(file.is_file(), "backup anterior deve ser preservado quando a origem some");
    }

    #[test]
    fn restore_roundtrip_rebuilds_case() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("BO-99_Macapa_abcd1234.sicro");
        fake_workspace(
            &ws,
            "55555555-5555-4555-5555-555555555555",
            "66666666-6666-4666-6666-666666666666",
        );
        let dest = tmp.path().join("backup-set");
        let cases = vec![GlobalCaseInput {
            workspace_path: ws.to_string_lossy().into_owned(),
            label: "BO 99".into(),
        }];
        let r = run_global_backup(&cases, &dest, no_progress).unwrap();
        assert_eq!(r.backed_up, 1);

        // Restaura para uma pasta nova (simula "outro computador").
        let restore_parent = tmp.path().join("restaurado");
        let rep = run_restore(&dest, &restore_parent, false, |_p| {}).unwrap();
        assert_eq!(rep.restored, 1);
        assert_eq!(rep.errors, 0);

        let restored = restore_parent.join("BO-99_Macapa_abcd1234.sicro");
        assert!(
            restored.join("manifest.json").is_file(),
            "manifesto restaurado"
        );
        assert!(
            restored
                .join("imagens")
                .join("originais")
                .join("a.png")
                .is_file(),
            "asset restaurado"
        );
        // logs/ é pulado no backup → não aparece no restaurado.
        assert!(
            !restored.join("logs").join("app.log").is_file(),
            "logs/ não entra no backup"
        );

        // Sem overwrite, restaurar de novo PULA (preserva o que já existe).
        let rep2 = run_restore(&dest, &restore_parent, false, |_p| {}).unwrap();
        assert_eq!(rep2.skipped, 1);
        assert_eq!(rep2.restored, 0);
    }
}
