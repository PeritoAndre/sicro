//! HTML integrity report writer (MVP 5).
//!
//! Produces a self-contained, single-file HTML report from a
//! `WorkspaceIntegrityReport`. The HTML is intentionally static (no JS,
//! no external CSS) so it can be opened by any browser, archived as
//! evidence and printed if needed.
//!
//! Caller pipeline:
//!   `verify_workspace` → `render_html_report` → `atomic_write_bytes`.
//!
//! Filename convention: `reports/workspace_integrity_YYYYMMDD_HHMMSS.html`.

use chrono::{DateTime, SecondsFormat, Utc};

use crate::models::{
    BrokenLaudoLink, EvidenceRegistryItem, IntegrityStatus, RegistrySummary,
    WorkspaceIntegrityReport,
};

pub fn report_filename(generated_at: &DateTime<Utc>) -> String {
    format!(
        "reports/workspace_integrity_{}.html",
        generated_at.format("%Y%m%d_%H%M%S")
    )
}

pub fn render_html_report(report: &WorkspaceIntegrityReport) -> String {
    let mut out = String::new();
    out.push_str("<!DOCTYPE html>\n");
    out.push_str("<html lang=\"pt-BR\"><head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str(
        "<title>SICRO — Relatório de Integridade do Workspace</title>\n",
    );
    out.push_str(&style_block());
    out.push_str("</head><body>\n");

    out.push_str(&format!(
        "<h1>Relatório de integridade do workspace</h1>\n\
         <p class=\"muted\">Gerado em <strong>{}</strong> · ocorrência <code>{}</code> · SICRO {} · {}</p>\n",
        report
            .generated_at
            .to_rfc3339_opts(SecondsFormat::Secs, true),
        report.occurrence_id,
        html_escape(&report.app_version),
        if report.deep_check_executed {
            "verificação profunda"
        } else {
            "verificação leve"
        },
    ));
    out.push_str(&format!(
        "<p class=\"muted\">Workspace: <code>{}</code></p>\n",
        html_escape(&report.workspace_path)
    ));

    out.push_str(&summary_section(&report.summary));
    out.push_str(&warnings_section(&report.warnings));
    out.push_str(&items_table(&report.items));
    out.push_str(&broken_links_table(&report.broken_laudo_links));

    out.push_str(&format!(
        "<footer><p class=\"muted\">SICRO Desktop {} · {}</p></footer>\n",
        html_escape(&report.app_version),
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
    ));
    out.push_str("</body></html>\n");
    out
}

fn style_block() -> &'static str {
    r#"<style>
      :root {
        --fg: #1f2933;
        --muted: #6b7785;
        --bg: #fff;
        --surface: #f6f7fa;
        --border: #d8dee5;
        --ok: #198754;
        --warn: #b58105;
        --crit: #b3261e;
        --info: #2c5fa7;
      }
      * { box-sizing: border-box; }
      body {
        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
        color: var(--fg); background: var(--bg);
        max-width: 1100px; margin: 24px auto; padding: 0 16px; line-height: 1.4;
      }
      h1, h2 { margin: 24px 0 8px; }
      h2 { font-size: 18px; }
      .muted { color: var(--muted); font-size: 13px; }
      code {
        font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
        background: var(--surface); padding: 1px 5px; border-radius: 3px;
        font-size: 12px;
      }
      table {
        width: 100%; border-collapse: collapse; margin: 12px 0 24px;
        font-size: 13px;
      }
      th, td {
        padding: 6px 8px; border-bottom: 1px solid var(--border);
        text-align: left; vertical-align: top;
      }
      th {
        background: var(--surface); font-weight: 600; font-size: 11px;
        text-transform: uppercase; letter-spacing: 0.04em;
      }
      tr:hover td { background: var(--surface); }
      .pill {
        display: inline-block; padding: 1px 8px; border-radius: 10px;
        font-size: 11px; font-family: ui-monospace, monospace;
      }
      .pill-ok    { background: #e3f5ec; color: var(--ok);   }
      .pill-warn  { background: #fff4d6; color: var(--warn); }
      .pill-crit  { background: #fce4e1; color: var(--crit); }
      .pill-info  { background: #e3eef9; color: var(--info); }
      .summary {
        display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
        gap: 8px; margin: 12px 0 24px;
      }
      .stat {
        background: var(--surface); border: 1px solid var(--border);
        border-radius: 6px; padding: 8px 10px;
      }
      .stat .n { font-size: 22px; font-family: ui-monospace, monospace; font-weight: 600; }
      .stat .l { font-size: 11px; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; }
      .stat.alert { background: #fff4d6; border-color: #efc775; }
      .stat.critical { background: #fce4e1; border-color: #e7a59f; }
      footer { margin-top: 32px; border-top: 1px solid var(--border); padding-top: 12px; }
    </style>
    "#
}

fn summary_section(s: &RegistrySummary) -> String {
    let status_pill = match s.overall_status.as_str() {
        "ok" => "<span class=\"pill pill-ok\">íntegro</span>",
        "warning" => "<span class=\"pill pill-warn\">atenção</span>",
        "critical" => "<span class=\"pill pill-crit\">crítico</span>",
        _ => "<span class=\"pill pill-info\">indefinido</span>",
    };
    let mut html = format!(
        "<h2>Resumo {}</h2>\n<div class=\"summary\">\n",
        status_pill
    );
    let stats: &[(u32, &str, bool, bool)] = &[
        (s.photos, "Fotos", false, false),
        (s.croquis, "Croquis", false, false),
        (s.croqui_exports, "Croquis PNG", false, false),
        (s.videos, "Vídeos", false, false),
        (s.storyboard_frames, "Frames", false, false),
        (s.laudos, "Laudos", false, false),
        (s.laudo_exports, "Exports", false, false),
        (s.imported_packages, "Pacotes importados", false, false),
        (s.linked_in_laudos, "Inseridos em laudo", false, false),
        (s.files_ok, "Arquivos OK", false, false),
        (s.files_missing, "Ausentes", s.files_missing > 0, false),
        (s.broken_links, "Links quebrados", s.broken_links > 0, false),
        (s.hash_mismatches, "Hash divergente", false, s.hash_mismatches > 0),
        (s.unsafe_paths, "Path inseguro", false, s.unsafe_paths > 0),
    ];
    for (n, label, warn, crit) in stats {
        let cls = if *crit {
            " critical"
        } else if *warn {
            " alert"
        } else {
            ""
        };
        html.push_str(&format!(
            "<div class=\"stat{cls}\"><div class=\"n\">{n}</div><div class=\"l\">{}</div></div>\n",
            html_escape(label)
        ));
    }
    html.push_str("</div>\n");
    html
}

fn warnings_section(warnings: &[String]) -> String {
    if warnings.is_empty() {
        return String::new();
    }
    let mut html = String::from("<h2>Alertas</h2>\n<ul>\n");
    for w in warnings {
        html.push_str(&format!("<li>{}</li>\n", html_escape(w)));
    }
    html.push_str("</ul>\n");
    html
}

fn items_table(items: &[EvidenceRegistryItem]) -> String {
    let mut html = String::from(
        "<h2>Itens (");
    html.push_str(&items.len().to_string());
    html.push_str(")</h2>\n<table><thead><tr>\n\
         <th>Tipo</th><th>Título</th><th>Origem</th><th>Caminho</th>\
         <th>Status</th><th>Inserções em laudo</th>\
         </tr></thead><tbody>\n");
    for i in items {
        html.push_str(&format!(
            "<tr><td>{kind}</td><td>{title}</td><td>{src}</td><td><code>{path}</code></td><td>{status}</td><td>{links}</td></tr>\n",
            kind = html_escape(i.kind.as_str()),
            title = html_escape(&i.title.clone().unwrap_or_default()),
            src = html_escape(&i.source_module),
            path = html_escape(&i.relative_path.clone().unwrap_or_default()),
            status = status_pill(&i.integrity_status, &i.integrity_detail),
            links = i.linked_laudos_count,
        ));
    }
    html.push_str("</tbody></table>\n");
    html
}

fn broken_links_table(links: &[BrokenLaudoLink]) -> String {
    let mut html = format!("<h2>Links quebrados em laudos ({})</h2>\n", links.len());
    if links.is_empty() {
        html.push_str(
            "<p class=\"muted\">Nenhum bloco do laudo aponta para um arquivo ausente ou caminho inseguro.</p>\n",
        );
        return html;
    }
    html.push_str(
        "<table><thead><tr>\
            <th>Laudo</th><th>Bloco</th><th>Caminho</th><th>Status</th><th>Detalhe</th>\
        </tr></thead><tbody>\n",
    );
    for b in links {
        html.push_str(&format!(
            "<tr><td>{title}</td><td>{nty}</td><td><code>{path}</code></td><td>{status}</td><td>{det}</td></tr>\n",
            title = html_escape(&b.laudo_title),
            nty = html_escape(&b.node_type),
            path = html_escape(&b.relative_path.clone().unwrap_or_default()),
            status = status_pill(&b.status, &b.detail),
            det = html_escape(&b.detail.clone().unwrap_or_default()),
        ));
    }
    html.push_str("</tbody></table>\n");
    html
}

fn status_pill(status: &IntegrityStatus, detail: &Option<String>) -> String {
    let label = match status {
        IntegrityStatus::Ok => "ok",
        IntegrityStatus::MissingFile => "ausente",
        IntegrityStatus::HashMismatch => "hash divergente",
        IntegrityStatus::MissingSidecar => "sidecar ausente",
        IntegrityStatus::BrokenLink => "link quebrado",
        IntegrityStatus::UnsafePath => "path inseguro",
        IntegrityStatus::Unknown => "—",
    };
    let cls = match status {
        IntegrityStatus::Ok => "pill-ok",
        IntegrityStatus::Unknown => "pill-info",
        IntegrityStatus::MissingFile
        | IntegrityStatus::MissingSidecar
        | IntegrityStatus::BrokenLink => "pill-warn",
        IntegrityStatus::HashMismatch | IntegrityStatus::UnsafePath => "pill-crit",
    };
    let title = detail
        .as_deref()
        .map(html_escape)
        .unwrap_or_default();
    format!(
        "<span class=\"pill {cls}\" title=\"{title}\">{label}</span>"
    )
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{EvidenceKind, EvidenceRegistryItem};
    use chrono::Utc;
    use uuid::Uuid;

    fn fixture_report() -> WorkspaceIntegrityReport {
        let mut summary = RegistrySummary::default();
        summary.photos = 2;
        summary.laudos = 1;
        summary.files_ok = 2;
        summary.total_items = 3;
        summary.overall_status = "ok".to_string();

        let item = EvidenceRegistryItem {
            id: "photo:abc".to_string(),
            occurrence_id: Uuid::nil(),
            kind: EvidenceKind::Photo,
            subtype: Some("image/jpeg".to_string()),
            title: Some("foto-001".to_string()),
            description: None,
            source_module: "importer".to_string(),
            original_id: Some("IMG-001".to_string()),
            relative_path: Some("imports/photos/IMG-001.jpg".to_string()),
            sidecar_relative_path: None,
            hash_sha256: None,
            size_bytes: Some(1024),
            mime_type: Some("image/jpeg".to_string()),
            created_at: Some(Utc::now()),
            updated_at: None,
            status: Some("imported".to_string()),
            integrity_status: IntegrityStatus::Ok,
            integrity_detail: None,
            linked_laudos_count: 1,
            metadata_json: "{}".to_string(),
        };

        WorkspaceIntegrityReport {
            occurrence_id: Uuid::nil(),
            workspace_path: "C:\\tmp\\fake.sicro".to_string(),
            generated_at: Utc::now(),
            app_version: "2.0.0-alpha.0".to_string(),
            summary,
            items: vec![item],
            broken_laudo_links: vec![],
            warnings: vec![],
            deep_check_executed: false,
        }
    }

    #[test]
    fn renders_doctype_html_and_title() {
        let html = render_html_report(&fixture_report());
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>SICRO"));
    }

    #[test]
    fn renders_summary_pill_when_status_ok() {
        let html = render_html_report(&fixture_report());
        assert!(html.contains("pill-ok"));
        assert!(html.contains("íntegro"));
    }

    #[test]
    fn escapes_workspace_path_with_backslashes() {
        let html = render_html_report(&fixture_report());
        // Backslashes don't need escaping per se, but quotes inside paths
        // would; sanity-check that the path appears.
        assert!(html.contains("C:\\tmp\\fake.sicro"));
    }

    #[test]
    fn includes_item_row() {
        let html = render_html_report(&fixture_report());
        assert!(html.contains("imports/photos/IMG-001.jpg"));
        assert!(html.contains("foto-001"));
    }

    #[test]
    fn includes_empty_broken_links_block() {
        let html = render_html_report(&fixture_report());
        assert!(html.contains("Links quebrados em laudos (0)"));
    }

    #[test]
    fn filename_uses_timestamp_pattern() {
        let dt = chrono::TimeZone::with_ymd_and_hms(
            &chrono::Utc, 2026, 5, 25, 14, 30, 12,
        )
        .unwrap();
        assert_eq!(
            report_filename(&dt),
            "reports/workspace_integrity_20260525_143012.html",
        );
    }
}
