//! Referências dentro de `.sicrodoc` cujo arquivo não está no workspace.
//! Tolerante: JSON malformado vira um aviso por laudo, não derruba a verificação.

use std::path::Path;

use serde_json::Value;
use uuid::Uuid;

use crate::error::Result;
use crate::filesystem::{probe_workspace_relative, RelativeResolution};
use crate::models::{BrokenLaudoLink, EvidenceKind, EvidenceRegistryItem, IntegrityStatus};

pub fn detect_broken_laudo_links(
    workspace_root: &Path,
    registry: &[EvidenceRegistryItem],
) -> Result<Vec<BrokenLaudoLink>> {
    let mut out: Vec<BrokenLaudoLink> = Vec::new();

    for item in registry {
        if !matches!(item.kind, EvidenceKind::Laudo) {
            continue;
        }
        let laudo_id = parse_id_suffix(&item.id);
        let Some(laudo_id) = laudo_id else { continue };
        let title = item
            .title
            .clone()
            .unwrap_or_else(|| format!("laudo {}", laudo_id));
        let Some(rel) = item.relative_path.as_deref() else {
            continue;
        };
        // Laudo .docx: só confere que o arquivo existe. Figuras só são seguidas
        // no .sicrodoc (JSON) — ler um .docx como JSON dava alarme falso.
        let is_sicrodoc = rel.to_ascii_lowercase().ends_with(".sicrodoc");
        let file_kind = match Path::new(rel).extension().and_then(|e| e.to_str()) {
            Some(ext) => format!(".{}", ext.to_ascii_lowercase()),
            None => "arquivo".to_string(),
        };
        let abs = match probe_workspace_relative(workspace_root, Some(rel)) {
            RelativeResolution::Ok { absolute, .. } => absolute,
            RelativeResolution::Missing { .. } => {
                out.push(BrokenLaudoLink {
                    laudo_id,
                    laudo_title: title,
                    node_type: file_kind.clone(),
                    relative_path: Some(rel.to_string()),
                    status: IntegrityStatus::MissingFile,
                    detail: Some(format!("arquivo do laudo ({file_kind}) não encontrado")),
                });
                continue;
            }
            RelativeResolution::Unsafe { reason } => {
                out.push(BrokenLaudoLink {
                    laudo_id,
                    laudo_title: title,
                    node_type: file_kind.clone(),
                    relative_path: Some(rel.to_string()),
                    status: IntegrityStatus::UnsafePath,
                    detail: Some(reason),
                });
                continue;
            }
            RelativeResolution::Empty => continue,
        };
        if !is_sicrodoc {
            continue;
        }

        let bytes = match std::fs::read(&abs) {
            Ok(b) => b,
            Err(e) => {
                out.push(BrokenLaudoLink {
                    laudo_id,
                    laudo_title: title,
                    node_type: ".sicrodoc".to_string(),
                    relative_path: Some(rel.to_string()),
                    status: IntegrityStatus::BrokenLink,
                    detail: Some(format!("ilegível: {e}")),
                });
                continue;
            }
        };
        let parsed: serde_json::Result<Value> = serde_json::from_slice(&bytes);
        let envelope = match parsed {
            Ok(v) => v,
            Err(e) => {
                out.push(BrokenLaudoLink {
                    laudo_id,
                    laudo_title: title,
                    node_type: ".sicrodoc".to_string(),
                    relative_path: Some(rel.to_string()),
                    status: IntegrityStatus::BrokenLink,
                    detail: Some(format!(".sicrodoc inválido (json): {e}")),
                });
                continue;
            }
        };

        let mut refs: Vec<NodeRef> = Vec::new();
        if let Some(content) = envelope.get("content") {
            collect_refs(content, &mut refs);
        }

        for r in refs {
            let probe =
                probe_workspace_relative(workspace_root, Some(&r.relative_path));
            match probe {
                RelativeResolution::Ok { .. } => {}
                RelativeResolution::Missing { .. } => {
                    out.push(BrokenLaudoLink {
                        laudo_id,
                        laudo_title: title.clone(),
                        node_type: r.node_type.clone(),
                        relative_path: Some(r.relative_path.clone()),
                        status: IntegrityStatus::MissingFile,
                        detail: Some(
                            "arquivo referenciado não está no workspace".to_string(),
                        ),
                    });
                }
                RelativeResolution::Unsafe { reason } => {
                    out.push(BrokenLaudoLink {
                        laudo_id,
                        laudo_title: title.clone(),
                        node_type: r.node_type.clone(),
                        relative_path: Some(r.relative_path.clone()),
                        status: IntegrityStatus::UnsafePath,
                        detail: Some(reason),
                    });
                }
                RelativeResolution::Empty => {}
            }
        }
    }

    Ok(out)
}

#[derive(Debug, Clone)]
struct NodeRef {
    node_type: String,
    relative_path: String,
}

/// Percorre a árvore TipTap e coleta todo nó com `attrs.relative_path` não vazio.
fn collect_refs(node: &Value, out: &mut Vec<NodeRef>) {
    if let Some(rel) = node
        .get("attrs")
        .and_then(|a| a.get("relative_path"))
        .and_then(Value::as_str)
    {
        if !rel.is_empty() {
            let node_type = node
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            out.push(NodeRef {
                node_type,
                relative_path: rel.to_string(),
            });
        }
    }
    if let Some(children) = node.get("content").and_then(Value::as_array) {
        for c in children {
            collect_refs(c, out);
        }
    }
}

fn parse_id_suffix(synthetic: &str) -> Option<Uuid> {
    let (_, suffix) = synthetic.split_once(':')?;
    Uuid::parse_str(suffix).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EvidenceKind;
    use chrono::Utc;
    use std::fs;
    use tempfile::TempDir;

    fn fixture_laudo_item(
        rel_doc: &str,
        title: &str,
    ) -> (Uuid, EvidenceRegistryItem) {
        let id = Uuid::new_v4();
        let item = EvidenceRegistryItem {
            id: format!("laudo:{id}"),
            occurrence_id: Uuid::nil(),
            kind: EvidenceKind::Laudo,
            subtype: None,
            title: Some(title.to_string()),
            description: None,
            source_module: "laudo".to_string(),
            original_id: None,
            relative_path: Some(rel_doc.to_string()),
            sidecar_relative_path: None,
            hash_sha256: None,
            size_bytes: None,
            mime_type: None,
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            status: None,
            integrity_status: IntegrityStatus::Unknown,
            integrity_detail: None,
            linked_laudos_count: 0,
            metadata_json: "{}".to_string(),
        };
        (id, item)
    }

    fn write_laudo(workspace: &Path, rel_doc: &str, content_json: &str) {
        let abs = workspace.join(rel_doc);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        let envelope = format!(r#"{{"content":{content_json}}}"#);
        fs::write(abs, envelope).unwrap();
    }

    #[test]
    fn empty_when_no_laudos() {
        let tmp = TempDir::new().unwrap();
        let r = detect_broken_laudo_links(tmp.path(), &[]).unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn reports_missing_sicrodoc_file() {
        let tmp = TempDir::new().unwrap();
        let (_id, item) = fixture_laudo_item(
            "laudos/missing.sicrodoc",
            "Laudo Fantasma",
        );
        let r = detect_broken_laudo_links(tmp.path(), &[item]).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].status, IntegrityStatus::MissingFile);
    }

    #[test]
    fn detects_figure_pointing_to_missing_photo() {
        let tmp = TempDir::new().unwrap();
        let (_id, item) =
            fixture_laudo_item("laudos/laudo-1.sicrodoc", "Laudo A");
        write_laudo(
            tmp.path(),
            "laudos/laudo-1.sicrodoc",
            r#"{"type":"doc","content":[
                {"type":"figure","attrs":{"relative_path":"imports/photos/MISS.jpg"}},
                {"type":"paragraph","content":[{"type":"text","text":"foo"}]}
            ]}"#,
        );
        let r = detect_broken_laudo_links(tmp.path(), &[item]).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].node_type, "figure");
        assert_eq!(r[0].status, IntegrityStatus::MissingFile);
    }

    #[test]
    fn ignores_figures_with_valid_targets() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("imports").join("photos");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("OK.jpg"), b"x").unwrap();

        let (_id, item) =
            fixture_laudo_item("laudos/laudo-1.sicrodoc", "Laudo A");
        write_laudo(
            tmp.path(),
            "laudos/laudo-1.sicrodoc",
            r#"{"type":"doc","content":[
                {"type":"figure","attrs":{"relative_path":"imports/photos/OK.jpg"}}
            ]}"#,
        );
        let r = detect_broken_laudo_links(tmp.path(), &[item]).unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn flags_unsafe_relative_paths_inside_doc() {
        let tmp = TempDir::new().unwrap();
        let (_id, item) =
            fixture_laudo_item("laudos/laudo-1.sicrodoc", "Laudo A");
        write_laudo(
            tmp.path(),
            "laudos/laudo-1.sicrodoc",
            r#"{"type":"doc","content":[
                {"type":"figure","attrs":{"relative_path":"../etc/passwd"}}
            ]}"#,
        );
        let r = detect_broken_laudo_links(tmp.path(), &[item]).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].status, IntegrityStatus::UnsafePath);
    }

    #[test]
    fn docx_laudo_present_is_not_flagged() {
        // Um .docx (ZIP) não é JSON e não pode virar "link quebrado".
        let tmp = TempDir::new().unwrap();
        let (_id, item) = fixture_laudo_item("laudos/laudo_1.docx", "Laudo 3.x");
        let abs = tmp.path().join("laudos").join("laudo_1.docx");
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, b"PK\x03\x04 zip do word").unwrap();
        let r = detect_broken_laudo_links(tmp.path(), &[item]).unwrap();
        assert!(r.is_empty(), "{r:?}");
    }

    #[test]
    fn docx_laudo_missing_is_reported() {
        let tmp = TempDir::new().unwrap();
        let (_id, item) = fixture_laudo_item("laudos/sumiu.docx", "Laudo 3.x");
        let r = detect_broken_laudo_links(tmp.path(), &[item]).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].status, IntegrityStatus::MissingFile);
        assert_eq!(r[0].node_type, ".docx");
    }

    #[test]
    fn warns_on_malformed_sicrodoc_json() {
        let tmp = TempDir::new().unwrap();
        let (_id, item) =
            fixture_laudo_item("laudos/laudo-1.sicrodoc", "Laudo A");
        let abs = tmp.path().join("laudos").join("laudo-1.sicrodoc");
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, b"not-a-json{").unwrap();
        let r = detect_broken_laudo_links(tmp.path(), &[item]).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].status, IntegrityStatus::BrokenLink);
    }
}
