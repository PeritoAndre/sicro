//! System health report (MVP 8 — Consolidação Alpha).
//!
//! Reúne em uma única estrutura o que o operador precisa para decidir
//! se o SICRO está saudável para uso clínico:
//!
//!   - versão do app;
//!   - status do workspace ativo;
//!   - contagens por módulo (laudos / croquis / vídeos / imagens /
//!     evidências / exports);
//!   - resumo de integridade (reaproveita o registry do MVP 5);
//!   - presença das dependências externas (`ffmpeg`, `ffprobe`).
//!
//! Renderiza tanto JSON (para a UI) quanto HTML auto-suficiente
//! (gravado em `reports/system_health_*.html` — formato adotado pelo
//! relatório de integridade do MVP 5).

use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::{
    audio_repo, croqui_repo, evidence_link_repo, export_repo, image_analysis_repo,
    laudo_repo, media_asset_repo, video_repo,
};
use crate::error::Result;
use crate::filesystem::atomic_write_bytes;
use crate::models::VerifyOptions;
use crate::registry;
use crate::workspace::manifest::{Manifest, APP_VERSION, SQLITE_FILENAME};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyStatus {
    pub name: String,
    pub found: bool,
    pub path: Option<String>,
    pub version_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCounters {
    pub photos: u32,
    pub audios: u32,
    pub laudos: u32,
    pub croquis: u32,
    pub croqui_exports: u32,
    pub videos: u32,
    pub storyboard_frames: u32,
    pub image_analyses: u32,
    pub image_exports: u32,
    pub laudo_exports: u32,
    pub evidence_links: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceHealth {
    pub workspace_path: String,
    pub workspace_id: Uuid,
    pub occurrence_id: Uuid,
    pub workspace_size_bytes: u64,
    pub counters: WorkspaceCounters,
    pub integrity_overall_status: String,
    pub files_ok: u32,
    pub files_missing: u32,
    pub broken_links: u32,
    pub unsafe_paths: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthSnapshot {
    pub generated_at: DateTime<Utc>,
    pub app_version: String,
    pub schema_migrations_applied: u32,
    pub dependencies: Vec<DependencyStatus>,
    pub workspace: Option<WorkspaceHealth>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReportArtifact {
    pub relative_path: String,
    pub generated_at: DateTime<Utc>,
    pub overall_status: String,
}

/// Build the snapshot. When `workspace_path` is `Some(...)`, drills
/// into the workspace and adds counters + integrity. When `None`,
/// returns only the global section (versão, deps).
pub fn build_snapshot(workspace_path: Option<&Path>) -> Result<SystemHealthSnapshot> {
    let mut warnings: Vec<String> = Vec::new();
    let dependencies = probe_dependencies(&mut warnings);

    let workspace_health = if let Some(ws) = workspace_path {
        match build_workspace_health(ws) {
            Ok(h) => Some(h),
            Err(e) => {
                warnings.push(format!(
                    "não foi possível inspecionar o workspace {}: {}",
                    ws.display(),
                    e
                ));
                None
            }
        }
    } else {
        None
    };

    let schema_migrations_applied = workspace_path
        .and_then(|ws| count_applied_migrations(ws).ok())
        .unwrap_or(0);

    Ok(SystemHealthSnapshot {
        generated_at: Utc::now(),
        app_version: APP_VERSION.to_string(),
        schema_migrations_applied,
        dependencies,
        workspace: workspace_health,
        warnings,
    })
}

/// Render the snapshot to a standalone HTML file under
/// `<workspace>/reports/system_health_<TS>.html`. When no workspace
/// is given, falls back to the system temp dir.
pub fn render_and_save(
    workspace_path: Option<&Path>,
    snapshot: &SystemHealthSnapshot,
) -> Result<HealthReportArtifact> {
    let html = render_html(snapshot);
    let stamp = snapshot.generated_at.format("%Y%m%d_%H%M%S");
    let rel = format!("reports/system_health_{}.html", stamp);
    let dest_root: PathBuf = workspace_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(std::env::temp_dir);
    let abs = dest_root.join(&rel);
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    atomic_write_bytes(&abs, html.as_bytes())?;

    let overall_status = snapshot
        .workspace
        .as_ref()
        .map(|w| w.integrity_overall_status.clone())
        .unwrap_or_else(|| "n/a".to_string());

    Ok(HealthReportArtifact {
        relative_path: rel,
        generated_at: snapshot.generated_at,
        overall_status,
    })
}

// ---------------------------------------------------------------------------
// Workspace inspection

fn build_workspace_health(workspace_root: &Path) -> Result<WorkspaceHealth> {
    let manifest = Manifest::read(workspace_root)?;
    let mut conn = open_connection(&workspace_root.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let counters = count_entities(&conn, &manifest.occurrence_id)?;

    // Integrity via the MVP 5 verifier (light pass).
    let report = registry::verify_workspace(
        &conn,
        workspace_root,
        &manifest.occurrence_id,
        &VerifyOptions { deep: false },
    )?;

    let workspace_size_bytes = directory_size(workspace_root).unwrap_or(0);

    Ok(WorkspaceHealth {
        workspace_path: workspace_root.to_string_lossy().into_owned(),
        workspace_id: manifest.workspace_id,
        occurrence_id: manifest.occurrence_id,
        workspace_size_bytes,
        counters,
        integrity_overall_status: report.summary.overall_status,
        files_ok: report.summary.files_ok,
        files_missing: report.summary.files_missing,
        broken_links: report.summary.broken_links,
        unsafe_paths: report.summary.unsafe_paths,
    })
}

/// Conta as entidades de uma ocorrência (laudos, croquis, fotos, vídeos…) a
/// partir de uma conexão já aberta. Determinístico, sem varrer disco — só
/// consulta o banco. Reutilizado pelo health snapshot e pelo índice de casos.
pub fn count_entities(conn: &Connection, occurrence_id: &Uuid) -> Result<WorkspaceCounters> {
    Ok(WorkspaceCounters {
        photos: media_asset_repo::list_by_occurrence(conn, occurrence_id)?.len() as u32,
        audios: audio_repo::list_for_occurrence(conn, occurrence_id)?.len() as u32,
        laudos: laudo_repo::list_by_occurrence(conn, occurrence_id)?.len() as u32,
        croquis: croqui_repo::list_by_occurrence(conn, occurrence_id)?
            .iter()
            .filter(|c| c.relative_path.ends_with(".sicrocroqui") || !c.relative_path.is_empty())
            .count() as u32,
        croqui_exports: croqui_repo::list_by_occurrence(conn, occurrence_id)?
            .iter()
            .filter(|c| c.last_export_relative_path.is_some())
            .count() as u32,
        videos: video_repo::list_media_for_occurrence(conn, occurrence_id)?.len() as u32,
        storyboard_frames: {
            let videos = video_repo::list_media_for_occurrence(conn, occurrence_id)?;
            let mut total = 0u32;
            for v in videos {
                let frames =
                    video_repo::list_storyboard_for_media(conn, occurrence_id, &v.sha256)
                        .unwrap_or_default();
                total = total.saturating_add(frames.len() as u32);
            }
            total
        },
        image_analyses: image_analysis_repo::list_by_occurrence(conn, occurrence_id)?.len()
            as u32,
        image_exports: image_analysis_repo::list_exports_by_occurrence(conn, occurrence_id)?
            .len() as u32,
        laudo_exports: export_repo::list_by_occurrence(conn, occurrence_id)?.len() as u32,
        evidence_links: evidence_link_repo::list_for_occurrence(conn, occurrence_id)?.len()
            as u32,
    })
}

/// Versão "stand-alone" para um comando leve: abre o banco do workspace,
/// garante migrações e devolve só as contagens (sem integridade, sem tamanho de
/// disco, sem sondar dependências). Barato o suficiente para rodar toda vez que
/// um caso fica ativo.
pub fn count_occurrence_entities(workspace_root: &Path) -> Result<WorkspaceCounters> {
    let manifest = Manifest::read(workspace_root)?;
    let mut conn = open_connection(&workspace_root.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    count_entities(&conn, &manifest.occurrence_id)
}

fn count_applied_migrations(workspace_root: &Path) -> Result<u32> {
    let conn = open_connection(&workspace_root.join(SQLITE_FILENAME))?;
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
        .unwrap_or(0);
    Ok(count as u32)
}

fn directory_size(path: &Path) -> std::io::Result<u64> {
    let mut total: u64 = 0;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            // Skip cache/logs to keep the number meaningful (matches backup).
            if let Some(name) = entry.path().file_name().and_then(|s| s.to_str()) {
                if matches!(name, "cache" | "logs") {
                    continue;
                }
            }
            total = total.saturating_add(directory_size(&entry.path())?);
        } else if ty.is_file() {
            total = total.saturating_add(entry.metadata()?.len());
        }
    }
    Ok(total)
}

// ---------------------------------------------------------------------------
// External dependencies probe

fn probe_dependencies(warnings: &mut Vec<String>) -> Vec<DependencyStatus> {
    let mut deps: Vec<DependencyStatus> = Vec::new();
    for tool in &["ffmpeg", "ffprobe"] {
        let result = which::which(tool).ok();
        let found = result.is_some();
        let path = result.as_ref().map(|p| p.to_string_lossy().into_owned());
        if !found {
            warnings.push(format!(
                "{} não encontrado no PATH — módulo Vídeo precisa dele",
                tool
            ));
        }
        let version_hint = if found { probe_version(tool) } else { None };
        deps.push(DependencyStatus {
            name: tool.to_string(),
            found,
            path,
            version_hint,
        });
    }
    deps
}

fn probe_version(tool: &str) -> Option<String> {
    let out = Command::new(tool).arg("-version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    // First line is usually "ffmpeg version 6.1.1 ..." — keep ≤ 80 chars.
    Some(s.lines().next().unwrap_or("").chars().take(80).collect())
}

// ---------------------------------------------------------------------------
// HTML rendering

fn render_html(s: &SystemHealthSnapshot) -> String {
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html lang=\"pt-BR\"><head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<title>SICRO — Relatório de saúde do sistema</title>\n");
    html.push_str(STYLE);
    html.push_str("</head><body>\n");
    html.push_str("<h1>Relatório de saúde do sistema (Alpha)</h1>\n");
    html.push_str(&format!(
        "<p class=\"muted\">Gerado em <strong>{}</strong> · SICRO {}</p>\n",
        s.generated_at.format("%Y-%m-%d %H:%M:%S UTC"),
        html_escape(&s.app_version),
    ));

    // Workspace
    if let Some(ws) = &s.workspace {
        html.push_str("<h2>Workspace ativo ");
        let pill_cls = match ws.integrity_overall_status.as_str() {
            "ok" => "pill-ok",
            "warning" => "pill-warn",
            "critical" => "pill-crit",
            _ => "pill-info",
        };
        html.push_str(&format!(
            "<span class=\"pill {pill_cls}\">{}</span></h2>\n",
            html_escape(&ws.integrity_overall_status),
        ));
        html.push_str("<dl class=\"meta\">\n");
        html.push_str(&format!("<dt>Caminho</dt><dd><code>{}</code></dd>", html_escape(&ws.workspace_path)));
        html.push_str(&format!("<dt>workspace_id</dt><dd><code>{}</code></dd>", ws.workspace_id));
        html.push_str(&format!("<dt>occurrence_id</dt><dd><code>{}</code></dd>", ws.occurrence_id));
        html.push_str(&format!("<dt>Tamanho</dt><dd>{}</dd>", pretty_bytes(ws.workspace_size_bytes)));
        html.push_str(&format!("<dt>Migrações aplicadas</dt><dd>{}</dd>", s.schema_migrations_applied));
        html.push_str("</dl>\n");

        html.push_str("<h3>Contadores</h3>\n<div class=\"summary\">\n");
        let stats: &[(u32, &str)] = &[
            (ws.counters.photos, "Fotos"),
            (ws.counters.laudos, "Laudos"),
            (ws.counters.croquis, "Croquis"),
            (ws.counters.croqui_exports, "Croquis PNG"),
            (ws.counters.videos, "Vídeos"),
            (ws.counters.storyboard_frames, "Frames"),
            (ws.counters.image_analyses, "Análises de imagem"),
            (ws.counters.image_exports, "Imagens derivadas"),
            (ws.counters.laudo_exports, "Exports laudo"),
            (ws.counters.evidence_links, "evidence_links"),
            (ws.files_ok, "Arquivos OK"),
            (ws.files_missing, "Arquivos ausentes"),
            (ws.broken_links, "Links quebrados"),
            (ws.unsafe_paths, "Path inseguro"),
        ];
        for (n, label) in stats {
            html.push_str(&format!(
                "<div class=\"stat\"><div class=\"n\">{n}</div><div class=\"l\">{}</div></div>\n",
                html_escape(label),
            ));
        }
        html.push_str("</div>\n");
    } else {
        html.push_str("<p class=\"muted\">Nenhum workspace ativo — apenas seção global.</p>\n");
    }

    // Dependencies
    html.push_str("<h2>Dependências externas</h2>\n");
    html.push_str("<table><thead><tr><th>Ferramenta</th><th>Encontrada?</th><th>Caminho</th><th>Versão</th></tr></thead><tbody>\n");
    for d in &s.dependencies {
        let badge = if d.found {
            "<span class=\"pill pill-ok\">ok</span>"
        } else {
            "<span class=\"pill pill-warn\">ausente</span>"
        };
        html.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td><code>{}</code></td><td>{}</td></tr>\n",
            html_escape(&d.name),
            badge,
            html_escape(&d.path.clone().unwrap_or_else(|| "—".to_string())),
            html_escape(&d.version_hint.clone().unwrap_or_else(|| "—".to_string())),
        ));
    }
    html.push_str("</tbody></table>\n");

    // Warnings
    if !s.warnings.is_empty() {
        html.push_str("<h2>Alertas</h2>\n<ul>\n");
        for w in &s.warnings {
            html.push_str(&format!("<li>{}</li>\n", html_escape(w)));
        }
        html.push_str("</ul>\n");
    }

    html.push_str(&format!(
        "<footer><p class=\"muted\">SICRO Desktop {} · {}</p></footer>\n",
        html_escape(&s.app_version),
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
    ));
    html.push_str("</body></html>\n");
    html
}

const STYLE: &str = r#"<style>
:root {
  --fg: #1f2933; --muted: #6b7785; --bg: #fff; --surface: #f6f7fa;
  --border: #d8dee5; --ok: #198754; --warn: #b58105; --crit: #b3261e; --info: #2c5fa7;
}
* { box-sizing: border-box; }
body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
  color: var(--fg); background: var(--bg);
  max-width: 1100px; margin: 24px auto; padding: 0 16px; line-height: 1.4;
}
h1, h2, h3 { margin: 24px 0 8px; }
h2 { font-size: 18px; } h3 { font-size: 15px; }
.muted { color: var(--muted); font-size: 13px; }
code { font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  background: var(--surface); padding: 1px 5px; border-radius: 3px; font-size: 12px; }
table { width: 100%; border-collapse: collapse; margin: 12px 0 24px; font-size: 13px; }
th, td { padding: 6px 8px; border-bottom: 1px solid var(--border); text-align: left; }
th { background: var(--surface); font-weight: 600; font-size: 11px;
  text-transform: uppercase; letter-spacing: 0.04em; }
.pill { display: inline-block; padding: 1px 8px; border-radius: 10px;
  font-size: 11px; font-family: ui-monospace, monospace; margin-left: 8px; }
.pill-ok    { background: #e3f5ec; color: var(--ok);   }
.pill-warn  { background: #fff4d6; color: var(--warn); }
.pill-crit  { background: #fce4e1; color: var(--crit); }
.pill-info  { background: #e3eef9; color: var(--info); }
.meta { display: grid; grid-template-columns: 200px 1fr; gap: 4px 12px; font-size: 13px; }
.meta dt { color: var(--muted); }
.summary { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 8px; margin: 12px 0 24px; }
.stat { background: var(--surface); border: 1px solid var(--border);
  border-radius: 6px; padding: 8px 10px; }
.stat .n { font-size: 22px; font-family: ui-monospace, monospace; font-weight: 600; }
.stat .l { font-size: 11px; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; }
footer { margin-top: 32px; border-top: 1px solid var(--border); padding-top: 12px; }
</style>"#;

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn pretty_bytes(n: u64) -> String {
    if n < 1024 {
        format!("{} B", n)
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else if n < 1024 * 1024 * 1024 {
        format!("{:.1} MB", n as f64 / 1024.0 / 1024.0)
    } else {
        format!("{:.2} GB", n as f64 / 1024.0 / 1024.0 / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_html_without_workspace() {
        let snap = SystemHealthSnapshot {
            generated_at: Utc::now(),
            app_version: "test".to_string(),
            schema_migrations_applied: 0,
            dependencies: vec![DependencyStatus {
                name: "ffmpeg".to_string(),
                found: false,
                path: None,
                version_hint: None,
            }],
            workspace: None,
            warnings: vec!["ffmpeg não encontrado".to_string()],
        };
        let html = render_html(&snap);
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("Relatório de saúde do sistema"));
        assert!(html.contains("ffmpeg"));
        assert!(html.contains("ausente"));
        assert!(html.contains("ffmpeg não encontrado"));
    }

    #[test]
    fn pretty_bytes_scales() {
        assert_eq!(pretty_bytes(500), "500 B");
        assert!(pretty_bytes(2_500).ends_with(" KB"));
        assert!(pretty_bytes(2_500_000).ends_with(" MB"));
        assert!(pretty_bytes(2_500_000_000).ends_with(" GB"));
    }

    #[test]
    fn html_escape_handles_dangerous_chars() {
        assert_eq!(
            html_escape("<a href=\"&\">"),
            "&lt;a href=&quot;&amp;&quot;&gt;",
        );
    }
}
