//! G12.21 — Gerador de relatório de análise pericial.
//!
//! Produz um HTML auto-contido com:
//!   - Cabeçalho institucional + identificador do laudo.
//!   - Thumbnail da imagem analisada (embutida como data URI).
//!   - Painel EXIF.
//!   - Tabela de hashes (MD5/SHA-1/SHA-256/SHA-3-256).
//!   - Lista de operações aplicadas (pipeline).
//!   - Lista de anotações com coordenadas + medidas.
//!   - Escala (se calibrada) + densidade de pixels.
//!   - Chain of custody (logs de operações com timestamp).
//!   - Footer com timestamp de geração + versão SICRO.
//!
//! O HTML pode ser convertido em PDF posteriormente pelo pipeline
//! Edge headless (igual ao laudo).

use base64::Engine as _;
use chrono::Utc;
use serde_json::Value;

use crate::models::ImageAnalysis;
use crate::workspace::manifest::APP_VERSION;

pub struct ReportInput<'a> {
    pub analysis: &'a ImageAnalysis,
    pub doc_json: &'a Value,
    pub exif_json: Option<&'a Value>,
    pub hashes_json: Option<&'a Value>,
    pub operation_logs: &'a [Value],
    pub thumbnail_data_uri: Option<String>,
}

pub fn render_html(input: &ReportInput<'_>) -> String {
    let title = escape_html(&input.analysis.title);
    let analysis_id = input.analysis.id.to_string();
    let occurrence_id = input.analysis.occurrence_id.to_string();
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();

    let source_block = render_source_block(input);
    let exif_block = render_exif_block(input.exif_json);
    let hashes_block = render_hashes_block(input.hashes_json);
    let processing_block = render_processing_stack(input.doc_json);
    let annotations_block = render_annotations(input.doc_json);
    let scale_block = render_scale(input.doc_json);
    let logs_block = render_logs(input.operation_logs);
    let thumb = match &input.thumbnail_data_uri {
        Some(uri) => format!(
            r#"<img class="thumb" src="{}" alt="Thumbnail da imagem analisada" />"#,
            uri
        ),
        None => String::from(
            r#"<div class="thumb-placeholder">[thumbnail não disponível]</div>"#,
        ),
    };

    format!(
        r#"<!doctype html>
<html lang="pt-BR">
<head>
<meta charset="utf-8" />
<title>Relatório de Análise — {title}</title>
<style>{styles}</style>
</head>
<body>
  <header class="institutional">
    <div class="brand">POLÍCIA CIENTÍFICA DO AMAPÁ</div>
    <div class="subtitle">Relatório de Análise Pericial de Imagem</div>
  </header>
  <main>
    <section class="meta">
      <h1>{title}</h1>
      <dl>
        <dt>ID da análise</dt><dd><code>{analysis_id}</code></dd>
        <dt>ID da ocorrência</dt><dd><code>{occurrence_id}</code></dd>
        <dt>Gerado em</dt><dd>{now}</dd>
        <dt>SICRO Desktop</dt><dd>v{version}</dd>
      </dl>
    </section>
    <section class="thumb-wrap">
      <h2>Imagem analisada</h2>
      {thumb}
    </section>
    {source_block}
    {hashes_block}
    {exif_block}
    {processing_block}
    {scale_block}
    {annotations_block}
    {logs_block}
    <footer>
      <p>
        Este relatório é gerado automaticamente pelo SICRO Desktop e
        descreve o pipeline de processamento aplicado à imagem original.
        Os hashes pertencem ao arquivo de origem; as operações listadas
        são reproduzíveis a partir do `.sicroimage`.
      </p>
    </footer>
  </main>
</body>
</html>"#,
        title = title,
        analysis_id = analysis_id,
        occurrence_id = occurrence_id,
        now = now,
        version = APP_VERSION,
        styles = REPORT_STYLES,
        thumb = thumb,
        source_block = source_block,
        exif_block = exif_block,
        hashes_block = hashes_block,
        processing_block = processing_block,
        annotations_block = annotations_block,
        scale_block = scale_block,
        logs_block = logs_block,
    )
}

fn render_source_block(input: &ReportInput<'_>) -> String {
    let a = input.analysis;
    format!(
        r#"<section>
  <h2>Origem</h2>
  <dl class="grid">
    <dt>Tipo</dt><dd>{kind}</dd>
    <dt>Caminho relativo</dt><dd><code>{path}</code></dd>
    <dt>Hash SHA-256 declarado</dt><dd><code>{hash}</code></dd>
    <dt>Status</dt><dd>{status}</dd>
    <dt>Criado em</dt><dd>{created}</dd>
    <dt>Atualizado em</dt><dd>{updated}</dd>
  </dl>
</section>"#,
        kind = escape_html(a.source_kind.as_str()),
        path = escape_html(&a.original_relative_path),
        hash = a.original_hash_sha256.as_deref().unwrap_or("—"),
        status = escape_html(&a.status),
        created = a.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
        updated = a.updated_at.format("%Y-%m-%d %H:%M:%S UTC"),
    )
}

fn render_exif_block(exif: Option<&Value>) -> String {
    let value = match exif {
        Some(v) => v,
        None => {
            return String::from(
                r#"<section><h2>Metadados EXIF</h2><p class="empty">Nenhum EXIF disponível para esta imagem.</p></section>"#,
            );
        }
    };

    let summary = value.get("summary");
    let summary_html = if let Some(s) = summary {
        let mut rows = String::new();
        if let Some(dt) = s.get("datetime").and_then(|v| v.as_str()) {
            rows.push_str(&format!(
                "<dt>Data/Hora</dt><dd>{}</dd>",
                escape_html(dt)
            ));
        }
        if let Some(cam) = s.get("camera") {
            if let Some(make) = cam.get("make").and_then(|v| v.as_str()) {
                rows.push_str(&format!(
                    "<dt>Fabricante</dt><dd>{}</dd>",
                    escape_html(make)
                ));
            }
            if let Some(model) = cam.get("model").and_then(|v| v.as_str()) {
                rows.push_str(&format!(
                    "<dt>Modelo</dt><dd>{}</dd>",
                    escape_html(model)
                ));
            }
        }
        if let Some(sw) = s.get("software").and_then(|v| v.as_str()) {
            rows.push_str(&format!(
                "<dt>Software</dt><dd>{}</dd>",
                escape_html(sw)
            ));
        }
        if let Some(iso) = s.get("iso").and_then(|v| v.as_u64()) {
            rows.push_str(&format!("<dt>ISO</dt><dd>{}</dd>", iso));
        }
        if let Some(et) = s.get("exposure_time").and_then(|v| v.as_str()) {
            rows.push_str(&format!(
                "<dt>Exposição</dt><dd>{}</dd>",
                escape_html(et)
            ));
        }
        if let Some(fn_) = s.get("f_number").and_then(|v| v.as_f64()) {
            rows.push_str(&format!("<dt>f/</dt><dd>{:.1}</dd>", fn_));
        }
        if let Some(fl) = s.get("focal_length_mm").and_then(|v| v.as_f64()) {
            rows.push_str(&format!(
                "<dt>Distância focal</dt><dd>{:.1} mm</dd>",
                fl
            ));
        }
        if let Some(gps) = s.get("gps") {
            if let (Some(lat), Some(lon)) = (
                gps.get("lat").and_then(|v| v.as_f64()),
                gps.get("lon").and_then(|v| v.as_f64()),
            ) {
                rows.push_str(&format!(
                    "<dt>GPS</dt><dd>{:.6}, {:.6}</dd>",
                    lat, lon
                ));
            }
        }
        format!("<dl class=\"grid\">{}</dl>", rows)
    } else {
        String::new()
    };

    format!(
        r#"<section>
  <h2>Metadados EXIF</h2>
  {}
</section>"#,
        summary_html
    )
}

fn render_hashes_block(hashes: Option<&Value>) -> String {
    let v = match hashes {
        Some(h) => h,
        None => return String::new(),
    };
    let md5 = v.get("md5").and_then(|s| s.as_str()).unwrap_or("—");
    let sha1 = v.get("sha1").and_then(|s| s.as_str()).unwrap_or("—");
    let sha256 = v.get("sha256").and_then(|s| s.as_str()).unwrap_or("—");
    let sha3_256 = v.get("sha3_256").and_then(|s| s.as_str()).unwrap_or("—");
    format!(
        r#"<section>
  <h2>Cadeia de custódia — hashes</h2>
  <table class="hash-table">
    <thead><tr><th>Algoritmo</th><th>Valor (hex)</th></tr></thead>
    <tbody>
      <tr><td>MD5</td><td><code>{md5}</code></td></tr>
      <tr><td>SHA-1</td><td><code>{sha1}</code></td></tr>
      <tr><td>SHA-256</td><td><code>{sha256}</code></td></tr>
      <tr><td>SHA-3-256</td><td><code>{sha3_256}</code></td></tr>
    </tbody>
  </table>
</section>"#,
    )
}

fn render_processing_stack(doc: &Value) -> String {
    let stack = doc
        .get("processing_stack")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if stack.is_empty() {
        return String::from(
            r#"<section><h2>Pipeline de processamento</h2><p class="empty">Nenhuma operação aplicada.</p></section>"#,
        );
    }
    let rows: String = stack
        .iter()
        .enumerate()
        .map(|(i, op)| {
            let kind = op.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let enabled = op.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
            let params = op
                .get("params")
                .map(|p| serde_json::to_string(p).unwrap_or_default())
                .unwrap_or_else(|| "{}".to_string());
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td><code>{}</code></td></tr>",
                i + 1,
                escape_html(kind),
                if enabled { "habilitada" } else { "desabilitada" },
                escape_html(&params),
            )
        })
        .collect();
    format!(
        r#"<section>
  <h2>Pipeline de processamento</h2>
  <table>
    <thead><tr><th>Ordem</th><th>Operação</th><th>Estado</th><th>Parâmetros</th></tr></thead>
    <tbody>{rows}</tbody>
  </table>
</section>"#,
    )
}

fn render_annotations(doc: &Value) -> String {
    let anns = doc
        .get("annotations")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if anns.is_empty() {
        return String::from(
            r#"<section><h2>Anotações</h2><p class="empty">Sem anotações.</p></section>"#,
        );
    }
    let rows: String = anns
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let kind = a.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let label = a.get("label").and_then(|v| v.as_str()).unwrap_or("");
            let notes = a.get("notes").and_then(|v| v.as_str()).unwrap_or("");
            let x = a.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = a.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            format!(
                "<tr><td>{}</td><td>{}</td><td>({:.0}, {:.0})</td><td>{}</td><td>{}</td></tr>",
                i + 1,
                escape_html(kind),
                x,
                y,
                escape_html(label),
                escape_html(notes),
            )
        })
        .collect();
    format!(
        r#"<section>
  <h2>Anotações ({n})</h2>
  <table>
    <thead><tr><th>#</th><th>Tipo</th><th>Posição (px)</th><th>Rótulo</th><th>Observação</th></tr></thead>
    <tbody>{rows}</tbody>
  </table>
</section>"#,
        n = anns.len(),
    )
}

fn render_scale(doc: &Value) -> String {
    let scale = doc.get("scale");
    match scale {
        Some(s) if !s.is_null() => {
            let px_per_unit = s
                .get("px_per_unit")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let unit = s.get("unit").and_then(|v| v.as_str()).unwrap_or("m");
            let calib_dist = s
                .get("calibration_real_distance")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            format!(
                r#"<section>
  <h2>Escala calibrada</h2>
  <dl class="grid">
    <dt>Pixels por unidade</dt><dd>{:.2} px / {}</dd>
    <dt>Distância de calibração</dt><dd>{:.2} {}</dd>
  </dl>
</section>"#,
                px_per_unit, unit, calib_dist, unit
            )
        }
        _ => String::new(),
    }
}

fn render_logs(logs: &[Value]) -> String {
    if logs.is_empty() {
        return String::from(
            r#"<section><h2>Histórico de operações</h2><p class="empty">Sem registros.</p></section>"#,
        );
    }
    let rows: String = logs
        .iter()
        .map(|log| {
            let at = log.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
            let action = log.get("action").and_then(|v| v.as_str()).unwrap_or("");
            let details = log
                .get("details_json")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!(
                "<tr><td>{}</td><td>{}</td><td><code>{}</code></td></tr>",
                escape_html(at),
                escape_html(action),
                escape_html(details),
            )
        })
        .collect();
    format!(
        r#"<section>
  <h2>Histórico de operações</h2>
  <table>
    <thead><tr><th>Quando</th><th>Ação</th><th>Detalhes</th></tr></thead>
    <tbody>{rows}</tbody>
  </table>
</section>"#,
    )
}

/// Embute uma imagem em data URI para o HTML do relatório.
pub fn encode_thumbnail_data_uri(bytes: &[u8], mime: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{};base64,{}", mime, b64)
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const REPORT_STYLES: &str = r#"
body { font-family: "Times New Roman", Cambria, serif; color: #111; margin: 24px; }
header.institutional {
  text-align: center; border-bottom: 1px solid #444; padding-bottom: 8px;
  margin-bottom: 18px;
}
header.institutional .brand { font-size: 14pt; font-weight: bold; }
header.institutional .subtitle { font-size: 11pt; color: #555; }
main h1 { font-size: 18pt; text-align: center; margin: 0 0 4px; }
main h2 { font-size: 12.5pt; margin: 18px 0 6px; border-bottom: 1px solid #ddd; padding-bottom: 2px; }
section { margin-bottom: 14px; page-break-inside: avoid; }
.thumb-wrap { text-align: center; }
.thumb { max-width: 70%; max-height: 14cm; border: 1px solid #aaa; }
.thumb-placeholder {
  font-style: italic; color: #888; padding: 20px;
  border: 1px dashed #bbb; background: #f7f7f7;
}
dl.grid { display: grid; grid-template-columns: 180px 1fr; gap: 4px 14px; font-size: 10.5pt; }
dl.grid dt { font-weight: 600; color: #444; }
dl.grid dd { margin: 0; word-break: break-word; }
table { width: 100%; border-collapse: collapse; font-size: 10pt; margin-top: 4px; }
table th, table td { border: 1px solid #aaa; padding: 4px 6px; vertical-align: top; }
table th { background: #ececec; text-align: left; }
table.hash-table td:first-child { width: 110px; font-weight: 600; }
code { font-family: "Consolas", "Courier New", monospace; font-size: 9.5pt; }
.empty { font-style: italic; color: #888; }
footer { margin-top: 24px; padding-top: 6px; border-top: 1px solid #aaa; font-size: 9.5pt; color: #555; }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn sample_analysis() -> ImageAnalysis {
        ImageAnalysis {
            id: Uuid::new_v4(),
            occurrence_id: Uuid::new_v4(),
            title: "Análise Teste".to_string(),
            source_kind: crate::models::ImageSourceKind::LocalImport,
            source_id: None,
            original_relative_path: "imagens/originais/teste.png".to_string(),
            original_hash_sha256: Some("abc".to_string()),
            analysis_relative_path: "imagens/analises/teste.sicroimage".to_string(),
            last_export_relative_path: None,
            status: "active".to_string(),
            metadata_json: "{}".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn report_renders_with_minimal_input() {
        let analysis = sample_analysis();
        let doc = json!({ "annotations": [], "processing_stack": [], "scale": null });
        let input = ReportInput {
            analysis: &analysis,
            doc_json: &doc,
            exif_json: None,
            hashes_json: None,
            operation_logs: &[],
            thumbnail_data_uri: None,
        };
        let html = render_html(&input);
        assert!(html.contains("Análise Teste"));
        assert!(html.contains("Relatório de Análise Pericial"));
        assert!(html.contains("Nenhuma operação aplicada"));
        assert!(html.contains("Sem anotações"));
    }

    #[test]
    fn report_includes_hashes_when_provided() {
        let analysis = sample_analysis();
        let doc = json!({});
        let hashes = json!({
            "md5": "900150983cd24fb0d6963f7d28e17f72",
            "sha1": "a9993e364706816aba3e25717850c26c9cd0d89d",
            "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "sha3_256": "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532",
        });
        let input = ReportInput {
            analysis: &analysis,
            doc_json: &doc,
            exif_json: None,
            hashes_json: Some(&hashes),
            operation_logs: &[],
            thumbnail_data_uri: None,
        };
        let html = render_html(&input);
        assert!(html.contains("900150983cd24fb0d6963f7d28e17f72"));
        assert!(html.contains("SHA-3-256"));
    }

    #[test]
    fn report_html_escapes_tags() {
        let mut analysis = sample_analysis();
        analysis.title = "<script>alert('x')</script>".to_string();
        let doc = json!({});
        let input = ReportInput {
            analysis: &analysis,
            doc_json: &doc,
            exif_json: None,
            hashes_json: None,
            operation_logs: &[],
            thumbnail_data_uri: None,
        };
        let html = render_html(&input);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn thumbnail_data_uri_encodes_bytes() {
        let uri = encode_thumbnail_data_uri(&[1, 2, 3], "image/png");
        assert!(uri.starts_with("data:image/png;base64,"));
    }
}
