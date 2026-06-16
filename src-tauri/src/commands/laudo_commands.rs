//! Tauri commands for the Laudo module.
//!
//! Spike B exposes the minimum surface required to validate the Document
//! Engine end-to-end:
//!   - create a laudo (row + empty .sicrodoc on disk);
//!   - list laudos of a workspace;
//!   - read a laudo's full document JSON;
//!   - save (overwrite) a laudo's document JSON.
//!
//! The schema of `doc` (TipTap-based) is owned by the front-end Document
//! Engine. The Rust side treats it as opaque JSON.

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::{laudo_repo, occurrence_repo};
use crate::error::{Result, SicroError};
use crate::filesystem::{atomic_write_bytes, resolve_workspace_relative};
use crate::hashing::sha256::sha256_file;
use crate::models::{Laudo, LaudoDoc, LaudoStatus, NewLaudoInput};
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};

const LAUDOS_SUBDIR: &str = "laudos";

#[tauri::command]
pub async fn create_laudo(
    workspace_path: String,
    input: NewLaudoInput,
) -> Result<LaudoDoc> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let now = Utc::now();
    let id = Uuid::new_v4();
    let relative_path = format!("{LAUDOS_SUBDIR}/laudo_{}.sicrodoc", id);

    let laudo = Laudo {
        id,
        occurrence_id: manifest.occurrence_id,
        title: if input.title.trim().is_empty() {
            "Laudo sem título".to_string()
        } else {
            input.title.trim().to_string()
        },
        template_id: if input.template_id.trim().is_empty() {
            "documento_em_branco".to_string()
        } else {
            input.template_id.trim().to_string()
        },
        relative_path: relative_path.clone(),
        status: LaudoStatus::Rascunho,
        created_at: now,
        updated_at: now,
        last_export_pdf: None,
        last_export_docx: None,
        signature_type: None,
    };

    laudo_repo::insert(&conn, &laudo)?;
    occurrence_repo::record_audit(
        &conn,
        Some(&laudo.occurrence_id),
        "laudo.created",
        Some("laudo"),
        Some("laudo"),
        Some(&laudo.id),
        None,
    )?;

    // Write the initial .sicrodoc envelope. The front-end will overwrite it
    // with a real TipTap document on first save; until then we ship a valid
    // empty document so a read-after-create never fails.
    let envelope = empty_envelope(&laudo);
    let target = ws.join(&relative_path);
    write_doc(&target, &envelope)?;

    Ok(LaudoDoc {
        laudo,
        doc: envelope,
        opened_with_newer_version: None,
    })
}

/// POC — Importa um `.docx` do Word como um novo laudo (mão única,
/// melhor-esforço). Cria a linha do laudo na ocorrência do workspace e
/// grava o `.sicrodoc` com o conteúdo convertido (texto, marcas, títulos,
/// listas, legendas) + as margens da página do documento de origem.
///
/// Passo 1 do POC: imagens viram placeholder (extração real no passo 2).
#[tauri::command]
pub async fn import_docx_as_laudo(
    workspace_path: String,
    source_path: String,
    title: Option<String>,
) -> Result<LaudoDoc> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;

    let src = PathBuf::from(&source_path);
    if !src.is_file() {
        return Err(SicroError::Filesystem(format!(
            ".docx não encontrado: {}",
            src.display()
        )));
    }

    // Conversão OOXML → ProseMirror (melhor-esforço).
    let parsed = crate::importer::docx_import::parse_docx_file(&src)?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let now = Utc::now();
    let id = Uuid::new_v4();
    let relative_path = format!("{LAUDOS_SUBDIR}/laudo_{}.sicrodoc", id);

    // Título: o informado, senão o nome do arquivo (sem extensão).
    let resolved_title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .or_else(|| {
            src.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "Laudo importado".to_string());

    let laudo = Laudo {
        id,
        occurrence_id: manifest.occurrence_id,
        title: resolved_title,
        template_id: "documento_em_branco".to_string(),
        relative_path: relative_path.clone(),
        status: LaudoStatus::Rascunho,
        created_at: now,
        updated_at: now,
        last_export_pdf: None,
        last_export_docx: None,
        signature_type: None,
    };

    laudo_repo::insert(&conn, &laudo)?;
    occurrence_repo::record_audit(
        &conn,
        Some(&laudo.occurrence_id),
        "laudo.imported_docx",
        Some("laudo"),
        Some("laudo"),
        Some(&laudo.id),
        Some(&source_path),
    )?;

    let mut envelope = import_envelope(&laudo, &parsed);

    // Passo 2 — grava as imagens extraídas do .docx (brasão do cabeçalho +
    // figuras do corpo) no workspace e reescreve os `relative_path`
    // temporários dos `figure` (corpo + cabeçalho) pro caminho final.
    let img_map = write_imported_images(&ws, &laudo.id.to_string(), &parsed.images);
    rewrite_figure_paths(&mut envelope, &img_map);

    let target = ws.join(&relative_path);
    write_doc(&target, &envelope)?;

    Ok(LaudoDoc {
        laudo,
        doc: envelope,
        opened_with_newer_version: None,
    })
}

/// Passo 2 do import — grava as imagens extraídas do `.docx` em
/// `laudos/<id>/evidencias/imported/<name>` e devolve o mapa
/// `name → relative_path` (pra reescrever os `figure`).
fn write_imported_images(
    ws: &Path,
    laudo_id: &str,
    images: &[crate::importer::docx_import::ExtractedImage],
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for img in images {
        let rel = format!("laudos/{laudo_id}/evidencias/imported/{}", img.name);
        let Ok(abs) = resolve_workspace_relative(ws, &rel) else {
            continue;
        };
        if let Some(parent) = abs.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                continue;
            }
        }
        if atomic_write_bytes(&abs, &img.bytes).is_ok() {
            map.insert(img.name.clone(), rel);
        }
    }
    map
}

/// Reescreve os `relative_path` temporários (`__docximport__/<name>`) dos
/// nós `figure` no envelope (corpo + `header.content`). Mídia gravada →
/// caminho final; mídia ausente → `null` (figure mostra o placeholder).
fn rewrite_figure_paths(
    envelope: &mut serde_json::Value,
    map: &std::collections::HashMap<String, String>,
) {
    if let Some(content) = envelope.get_mut("content") {
        rewrite_figures_in(content, map);
    }
    if let Some(hc) = envelope.get_mut("header").and_then(|h| h.get_mut("content")) {
        rewrite_figures_in(hc, map);
    }
    if let Some(fc) = envelope.get_mut("footer").and_then(|f| f.get_mut("content")) {
        rewrite_figures_in(fc, map);
    }
}

fn rewrite_figures_in(
    node: &mut serde_json::Value,
    map: &std::collections::HashMap<String, String>,
) {
    if let Some(arr) = node.as_array_mut() {
        for item in arr.iter_mut() {
            rewrite_figures_in(item, map);
        }
        return;
    }
    let Some(obj) = node.as_object_mut() else {
        return;
    };
    if obj.get("type").and_then(|t| t.as_str()) == Some("figure") {
        let temp = obj
            .get("attrs")
            .and_then(|a| a.get("relative_path"))
            .and_then(|p| p.as_str())
            .and_then(|rel| {
                rel.strip_prefix(crate::importer::docx_import::DOCX_IMPORT_PREFIX)
                    .map(|s| s.to_string())
            });
        if let Some(name) = temp {
            let new_val = match map.get(&name) {
                Some(final_rel) => serde_json::json!(final_rel),
                None => serde_json::Value::Null,
            };
            if let Some(attrs) = obj.get_mut("attrs").and_then(|a| a.as_object_mut()) {
                attrs.insert("relative_path".into(), new_val);
            }
        }
    }
    if let Some(content) = obj.get_mut("content") {
        rewrite_figures_in(content, map);
    }
}

/// Monta o envelope `.sicrodoc` para um laudo importado: conteúdo
/// convertido + margens da origem + cabeçalho desligado (documento limpo).
fn import_envelope(
    laudo: &Laudo,
    parsed: &crate::importer::docx_import::ParsedDocx,
) -> serde_json::Value {
    // CABEÇALHO: por decisão de produto, o cabeçalho do Word NÃO é importado.
    // O corpo entra LIMPO e o perito aplica depois o "Cabeçalho oficial" do
    // SICRO (brasões corretos, padronizado) — assim não vem timbre de terceiro
    // junto. O parser ainda extrai o header (stats/diagnóstico e mecânica do
    // rodapé), mas o conteúdo é descartado aqui.
    let header_h = 2.5; // DEFAULT_HEADER_HEIGHT_CM (header desligado: valor só default)
    let top_min = 1.5; // sem header pra acomodar; só não cola o texto na borda

    let mut layout = serde_json::json!({
        "page_size": "A4",
        "orientation": "portrait",
        "header_height_cm": header_h,
    });

    // Saneamento das margens. A margem de TOPO reserva o espaço que o cabeçalho
    // do `.docx` ocupava (estimado em `header_reserve_cm`): o Word usa `w:top`
    // pequeno e conta com o conteúdo do header pra empurrar o corpo — sem isso,
    // ao NÃO importar o header, o corpo "sobe" e cola no topo da página.
    let header_reserve = parsed.header_reserve_cm.unwrap_or(0.0);
    let margins_obj = parsed.margins.map(|m| {
        serde_json::json!({
            "top": format!("{:.2}cm", m.top.max(header_reserve).clamp(top_min, 7.0)),
            "right": format!("{:.2}cm", m.right.clamp(1.5, 5.0)),
            "bottom": format!("{:.2}cm", m.bottom.clamp(1.5, 5.0)),
            "left": format!("{:.2}cm", m.left.clamp(1.5, 5.0)),
        })
    });
    if let Some(mo) = margins_obj {
        layout["page"] = serde_json::json!({ "margins": mo });
    }

    // Cabeçalho sempre DESLIGADO e vazio na importação (documento limpo).
    let header = serde_json::json!({
        "enabled": false,
        "content": { "type": "doc", "content": [ { "type": "paragraph" } ] }
    });

    // W — Rodapé: ligado quando o `.docx` tinha conteúdo no footer (ex: o
    // brasão da Polícia Científica); senão, desligado e vazio.
    let footer = match &parsed.footer_content {
        Some(fc) => serde_json::json!({ "enabled": true, "content": fc.clone() }),
        None => serde_json::json!({
            "enabled": false,
            "content": { "type": "doc", "content": [ { "type": "paragraph" } ] }
        }),
    };
    // Altura do rodapé dimensionada pelo nº de blocos (≈0,6cm/bloco + folga),
    // limitada a [1,5cm; 4cm]. Só quando há rodapé.
    if parsed.footer_content.is_some() {
        let n = parsed.stats.footer_paragraphs.max(1) as f64;
        let footer_h = (0.6 * n + 0.5).clamp(1.5, 4.0);
        layout["footer_height_cm"] = serde_json::json!((footer_h * 100.0).round() / 100.0);
    }

    serde_json::json!({
        "schema_version": "1.2.0",
        "document_id": laudo.id.to_string(),
        "occurrence_id": laudo.occurrence_id.to_string(),
        "type": "laudo",
        "title": laudo.title,
        "template_id": laudo.template_id,
        "created_at": laudo.created_at.to_rfc3339(),
        "updated_at": laudo.updated_at.to_rfc3339(),
        "metadata": {},
        "layout": layout,
        "header": header,
        "footer": footer,
        "content": parsed.content.clone(),
    })
}

#[tauri::command]
pub async fn list_laudos(workspace_path: String) -> Result<Vec<Laudo>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let mut list = laudo_repo::list_by_occurrence(&conn, &manifest.occurrence_id)?;

    // H — best-effort: lê cada `.sicrodoc` e extrai `finalization.signature.type`.
    // Falhas são silenciosas (laudo continua sem badge na lista).
    for laudo in list.iter_mut() {
        if let Some(sig_type) = read_signature_type(&ws, &laudo.relative_path) {
            laudo.signature_type = Some(sig_type);
        }
    }

    Ok(list)
}

/// Lê o `.sicrodoc` no disco e retorna o `type` da assinatura digital
/// (`finalization.signature.type`), se houver. Falhas silenciosas.
fn read_signature_type(ws: &Path, relative_path: &str) -> Option<String> {
    let abs = ws.join(relative_path);
    let bytes = std::fs::read(&abs).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("finalization")
        .and_then(|f| f.get("signature"))
        .and_then(|s| s.get("type"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
}

#[tauri::command]
pub async fn read_laudo(workspace_path: String, laudo_id: String) -> Result<LaudoDoc> {
    let ws = PathBuf::from(&workspace_path);
    let id = Uuid::parse_str(&laudo_id)
        .map_err(|e| SicroError::Validation(format!("invalid laudo id: {e}")))?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let laudo = laudo_repo::find_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("laudo {} not found", id)))?;

    let target = ws.join(&laudo.relative_path);
    let doc: serde_json::Value = match std::fs::read(&target) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        // Auto-cura: a linha do laudo existe no banco, mas o `.sicrodoc` não
        // está no disco (escrita interrompida, ou pasta sincronizada — ex.:
        // OneDrive — que não persistiu o arquivo). Em vez de travar o "abrir",
        // devolve um envelope vazio para o perito editar e salvar (o próximo
        // save regrava o arquivo). NÃO escrevemos aqui: leitura não muta o
        // disco e, como o arquivo não existe (NotFound), não há conteúdo a
        // sobrescrever/perder.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => empty_envelope(&laudo),
        Err(e) => {
            return Err(SicroError::Workspace(format!(
                "could not read laudo at {}: {}",
                target.display(),
                e
            )));
        }
    };

    // Assinatura de versão: se o arquivo foi salvo numa versão do SICRO mais
    // nova que a deste app, sinaliza pro front avisar (não bloqueia).
    let opened_with_newer_version = doc
        .get("sicro_app_version")
        .and_then(|v| v.as_str())
        .filter(|fv| version_is_newer(fv, crate::workspace::manifest::APP_VERSION))
        .map(|fv| fv.to_string());

    Ok(LaudoDoc {
        laudo,
        doc,
        opened_with_newer_version,
    })
}

/// Remove o laudo do workspace: apaga a linha do SQLite e remove o
/// arquivo `.sicrodoc` em disco. Idempotente em relação ao arquivo —
/// se ele já não existe, a operação segue (o usuário não deve ser
/// penalizado por arquivo já apagado externamente).
///
/// Audit: grava `laudo.deleted` em `occurrence_audit` antes de remover
/// a linha do laudo para preservar a rastreabilidade.
#[tauri::command]
pub async fn delete_laudo(
    workspace_path: String,
    laudo_id: String,
) -> Result<()> {
    let ws = PathBuf::from(&workspace_path);
    let id = Uuid::parse_str(&laudo_id)
        .map_err(|e| SicroError::Validation(format!("invalid laudo id: {e}")))?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let laudo = laudo_repo::find_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("laudo {} not found", id)))?;

    // Audit primeiro (enquanto o laudo ainda existe na tabela), depois
    // remove a linha. O record_audit referencia o id do laudo, então
    // se a ordem inverter o FK não vai estourar — mas mantemos a
    // sequência clara para futuro hardening do schema.
    occurrence_repo::record_audit(
        &conn,
        Some(&laudo.occurrence_id),
        "laudo.deleted",
        Some("laudo"),
        Some("laudo"),
        Some(&laudo.id),
        Some(&laudo.relative_path),
    )?;

    laudo_repo::delete(&conn, &id)?;

    // Apaga o .sicrodoc do disco. NotFound é silencioso para tornar
    // a operação idempotente (se o usuário deletou o arquivo
    // manualmente, o command ainda completa com sucesso).
    let target = ws.join(&laudo.relative_path);
    match std::fs::remove_file(&target) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(SicroError::Filesystem(format!(
            "could not delete laudo file at {}: {}",
            target.display(),
            e
        ))),
    }
}

#[tauri::command]
pub async fn save_laudo(
    workspace_path: String,
    laudo_id: String,
    doc: serde_json::Value,
) -> Result<Laudo> {
    let ws = PathBuf::from(&workspace_path);
    let id = Uuid::parse_str(&laudo_id)
        .map_err(|e| SicroError::Validation(format!("invalid laudo id: {e}")))?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let mut laudo = laudo_repo::find_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("laudo {} not found", id)))?;

    let target = ws.join(&laudo.relative_path);
    write_doc(&target, &doc)?;

    let now = Utc::now();
    laudo_repo::touch_updated_at(&conn, &laudo.id, now)?;
    laudo.updated_at = now;

    occurrence_repo::record_audit(
        &conn,
        Some(&laudo.occurrence_id),
        "laudo.saved",
        Some("laudo"),
        Some("laudo"),
        Some(&laudo.id),
        None,
    )?;

    Ok(laudo)
}

// ---------------------------------------------------------------------------
// Helpers

fn write_doc(target: &Path, doc: &serde_json::Value) -> Result<()> {
    // Assinatura de versão: carimba a versão do SICRO que está gravando. Na
    // leitura, se o arquivo vier de uma versão MAIS NOVA, avisamos o perito
    // (pode não abrir corretamente → recomendar atualizar o software).
    let mut stamped = doc.clone();
    if let Some(obj) = stamped.as_object_mut() {
        obj.insert(
            "sicro_app_version".to_string(),
            serde_json::Value::String(crate::workspace::manifest::APP_VERSION.to_string()),
        );
    }
    let bytes = serde_json::to_vec_pretty(&stamped)?;
    atomic_write_bytes(target, &bytes)?;
    Ok(())
}

/// `true` se a versão `a` é estritamente mais NOVA que `b`. Semver simplão
/// (major.minor.patch[-pré]); pré-lançamento conta como mais antigo que o
/// release de mesmo núcleo (2.0.0 > 2.0.0-beta.1 > 2.0.0-beta.0). Carimbo
/// ausente/ilegível nunca é "mais novo" (não avisa à toa).
fn version_is_newer(a: &str, b: &str) -> bool {
    fn parse(v: &str) -> ([u64; 3], String) {
        let v = v.trim();
        let (core, pre) = match v.split_once('-') {
            Some((c, p)) => (c, p.to_string()),
            None => (v, String::new()),
        };
        let mut nums = [0u64; 3];
        for (i, part) in core.split('.').take(3).enumerate() {
            nums[i] = part.trim().parse().unwrap_or(0);
        }
        (nums, pre)
    }
    let (an, ap) = parse(a);
    let (bn, bp) = parse(b);
    if an != bn {
        return an > bn;
    }
    match (ap.is_empty(), bp.is_empty()) {
        (true, true) => false,
        (true, false) => true,  // a = release, b = pré → a mais novo
        (false, true) => false, // a = pré, b = release → a mais antigo
        (false, false) => ap > bp,
    }
}

#[cfg(test)]
mod version_tests {
    use super::version_is_newer;

    #[test]
    fn detects_newer_only_when_strictly_ahead() {
        assert!(version_is_newer("2.1.0", "2.0.0"));
        assert!(version_is_newer("2.0.1", "2.0.0"));
        assert!(version_is_newer("3.0.0", "2.9.9"));
        assert!(version_is_newer("2.0.0", "2.0.0-beta.0"));
        assert!(version_is_newer("2.0.0-beta.1", "2.0.0-beta.0"));
        assert!(!version_is_newer("2.0.0", "2.0.0"));
        assert!(!version_is_newer("2.0.0-beta.0", "2.0.0"));
        assert!(!version_is_newer("1.9.9", "2.0.0"));
        assert!(!version_is_newer("", "2.0.0-beta.0"));
    }
}

/// Build a minimal `.sicrodoc` envelope for a freshly-created laudo.
// ---------------------------------------------------------------------------
// H — Fluxo gov.br externo: importação do PDF assinado de volta para o workspace.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ImportSignedPdfInput {
    /// ID do laudo (UUID).
    pub laudo_id: String,
    /// Caminho ABSOLUTO no SO do PDF assinado escolhido pelo perito.
    /// Vindo do `<input type=file>` ou do file picker do Tauri.
    pub source_absolute_path: String,
    /// Nome de arquivo desejado (sem path). Se vazio, o backend gera um.
    pub preferred_filename: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportSignedPdfResult {
    /// Caminho relativo ao workspace onde o PDF foi gravado.
    pub relative_path: String,
    /// SHA-256 do PDF assinado (hex).
    pub sha256: String,
    /// Tamanho em bytes.
    pub size_bytes: u64,
}

/// H — Importa um PDF assinado pelo gov.br de volta para o workspace.
///
/// Validações:
///   - O arquivo source existe e é PDF (header `%PDF-`).
///   - O laudo existe na ocorrência atual do workspace.
///   - O caminho final fica dentro de `laudos/<id>/assinados/`.
///
/// O command NÃO mexe no `.sicrodoc` — quem grava o `finalization.signature`
/// com os metadados retornados é o frontend (via `setStatus` do store).
/// Isso preserva a separação: backend cuida do arquivo, frontend cuida
/// do envelope JSON.
#[tauri::command]
pub async fn import_signed_pdf(
    workspace_path: String,
    input: ImportSignedPdfInput,
) -> Result<ImportSignedPdfResult> {
    let ws = PathBuf::from(&workspace_path);
    let _manifest = Manifest::read(&ws)?;

    let laudo_uuid = Uuid::parse_str(&input.laudo_id)
        .map_err(|e| SicroError::Validation(format!("UUID inválido: {e}")))?;

    let src_abs = PathBuf::from(&input.source_absolute_path);
    if !src_abs.is_file() {
        return Err(SicroError::Filesystem(format!(
            "PDF assinado não encontrado: {}",
            src_abs.display()
        )));
    }

    // Validação: cabeçalho `%PDF-` (primeiros 5 bytes).
    let bytes = std::fs::read(&src_abs).map_err(|e| {
        SicroError::Filesystem(format!("não consegui ler o PDF: {e}"))
    })?;
    if bytes.len() < 5 || &bytes[..5] != b"%PDF-" {
        return Err(SicroError::Validation(
            "arquivo não é um PDF válido (header %PDF- ausente)".to_string(),
        ));
    }

    // Verifica que o laudo existe no banco para evitar pastas órfãs.
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let exists = laudo_repo::find_by_id(&conn, &laudo_uuid)?;
    if exists.is_none() {
        return Err(SicroError::Validation(format!(
            "laudo {} não encontrado neste workspace",
            laudo_uuid
        )));
    }

    // Sanitiza nome de arquivo + monta caminho relativo.
    let filename = input
        .preferred_filename
        .as_deref()
        .map(sanitize_pdf_filename)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let stamp = Utc::now().format("%Y%m%d_%H%M%S");
            format!("laudo_assinado_govbr_{stamp}.pdf")
        });

    let rel = format!("laudos/{}/assinados/{}", laudo_uuid, filename);
    let dst_abs = resolve_workspace_relative(&ws, &rel)?;
    if let Some(parent) = dst_abs.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            SicroError::Filesystem(format!(
                "cannot create signed dir: {e}"
            ))
        })?;
    }

    // Grava de forma atômica.
    atomic_write_bytes(&dst_abs, &bytes)?;

    // Hash + size do arquivo finalizado (== source, mas re-hash garante).
    let sha = sha256_file(&dst_abs)?;
    let size_bytes = std::fs::metadata(&dst_abs)
        .map(|m| m.len())
        .unwrap_or(bytes.len() as u64);

    Ok(ImportSignedPdfResult {
        relative_path: rel,
        sha256: sha,
        size_bytes,
    })
}

fn sanitize_pdf_filename(raw: &str) -> String {
    let trimmed = raw.trim();
    let stem = trimmed
        .strip_suffix(".pdf")
        .or_else(|| trimmed.strip_suffix(".PDF"))
        .unwrap_or(trimmed);
    let mut out = String::with_capacity(stem.len() + 4);
    for c in stem.chars() {
        let ok = c.is_ascii_alphanumeric()
            || c == '-'
            || c == '_'
            || c == '.';
        out.push(if ok { c } else { '_' });
    }
    let truncated: String = out.trim_matches('_').chars().take(80).collect();
    if truncated.is_empty() {
        "laudo_assinado.pdf".to_string()
    } else {
        format!("{truncated}.pdf")
    }
}

// ---------------------------------------------------------------------------

/// The front-end may overwrite it immediately, but it MUST be valid JSON the
/// front-end can parse — otherwise the first `read_laudo` would explode.
fn empty_envelope(laudo: &Laudo) -> serde_json::Value {
    serde_json::json!({
        "schema_version": "1.0.0",
        "document_id": laudo.id.to_string(),
        "occurrence_id": laudo.occurrence_id.to_string(),
        "type": "laudo",
        "title": laudo.title,
        "template_id": laudo.template_id,
        "created_at": laudo.created_at.to_rfc3339(),
        "updated_at": laudo.updated_at.to_rfc3339(),
        "metadata": {},
        "layout": {
            "page_size": "A4",
            "orientation": "portrait"
        },
        // ProseMirror/TipTap empty doc: a single empty paragraph.
        "content": {
            "type": "doc",
            "content": [
                { "type": "paragraph" }
            ]
        }
    })
}
