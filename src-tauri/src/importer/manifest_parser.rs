//! Tolerant parser for the `manifest.json` of a `.sicroapp`.
//!
//! The mobile contract (`docs/SICROAPP_COMPATIBILITY_POLICY.md`) is
//! "additive only — never rename, move or remove". So this parser only reads
//! what it understands and **preserves the raw payload verbatim** so future
//! Desktop versions can mine fields we don't model today.
//!
//! Mobile v0.6 emits Portuguese keys. The auditoria report also proposes
//! English aliases (`format`, `schema_version`, `exported_at`, `app_name`,
//! `app_version`, `operator`, ...) for forward compatibility. The parser
//! accepts both — Portuguese is the source of truth today, English is the
//! upgrade path.

use serde_json::Value;

use crate::error::{Result, SicroError};

#[derive(Debug, Clone)]
pub struct ParsedManifest {
    pub format: String,
    pub schema_version: String,
    pub exported_at: Option<String>,
    pub app_name: Option<String>,
    pub app_version: Option<String>,

    pub occurrence_id: Option<String>,
    pub tipo_pericia: Option<String>,
    pub natureza: Option<String>,
    pub resultado: Option<String>,

    pub manifest_warnings: Vec<String>,
    pub counts: Option<Value>,
    pub declared_files: Vec<String>,

    /// Raw JSON of the manifest, preserved for audit/forward-compat.
    pub raw_json: String,
}

pub fn parse(raw_bytes: &[u8]) -> Result<ParsedManifest> {
    let parsed: Value = serde_json::from_slice(raw_bytes).map_err(|e| {
        SicroError::Validation(format!("manifest.json is not valid JSON: {e}"))
    })?;
    let obj = parsed
        .as_object()
        .ok_or_else(|| SicroError::Validation("manifest.json must be an object".to_string()))?;

    let format = first_string(obj, &["formato", "format"])
        .ok_or_else(|| SicroError::Validation("manifest is missing 'formato'".to_string()))?;
    // Accept both 'sicroapp' (current) and 'sicrocampo' (legacy).
    if format != "sicroapp" && format != "sicrocampo" {
        return Err(SicroError::Validation(format!(
            "unsupported manifest 'formato': {format:?}"
        )));
    }

    let schema_version = first_string(obj, &["versao", "schema_version"])
        .ok_or_else(|| SicroError::Validation("manifest is missing 'versao'".to_string()))?;

    let exported_at = first_string(obj, &["gerado_em", "exported_at"]);
    let app_name = first_string(obj, &["app_name"]);
    let app_version = first_string(obj, &["app_version"]);

    // Nested ocorrencia block (mobile) — flatten the fields we care about.
    let occurrence_block = obj.get("ocorrencia").and_then(Value::as_object);
    let occurrence_id = occurrence_block
        .and_then(|o| o.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let tipo_pericia = occurrence_block
        .and_then(|o| o.get("tipo_pericia"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let natureza = occurrence_block
        .and_then(|o| o.get("natureza"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let resultado = occurrence_block
        .and_then(|o| o.get("resultado"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let manifest_warnings = obj
        .get("avisos")
        .or_else(|| obj.get("notes"))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let declared_files = obj
        .get("arquivos")
        .or_else(|| obj.get("files"))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let counts = obj.get("contagens").cloned();

    let raw_json = String::from_utf8(raw_bytes.to_vec())
        .unwrap_or_else(|_| serde_json::to_string(&parsed).unwrap_or_default());

    Ok(ParsedManifest {
        format,
        schema_version,
        exported_at,
        app_name,
        app_version,
        occurrence_id,
        tipo_pericia,
        natureza,
        resultado,
        manifest_warnings,
        counts,
        declared_files,
        raw_json,
    })
}

fn first_string(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = obj.get(*k).and_then(Value::as_str) {
            return Some(s.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v0_6_portuguese_manifest() {
        let raw = br#"{
          "formato": "sicroapp",
          "versao": "0.6",
          "gerado_em": "2026-05-25T14:30:10.000",
          "ocorrencia": {
            "id": "occ_123",
            "tipo_pericia": "transito",
            "natureza": "colisao",
            "resultado": "vitima_lesionada",
            "status_operacional": "exportada"
          },
          "contagens": { "fotos": 18 },
          "arquivos": ["manifest.json","caso.json"],
          "avisos": []
        }"#;
        let m = parse(raw).expect("parse");
        assert_eq!(m.format, "sicroapp");
        assert_eq!(m.schema_version, "0.6");
        assert_eq!(m.occurrence_id.as_deref(), Some("occ_123"));
        assert_eq!(m.tipo_pericia.as_deref(), Some("transito"));
        assert_eq!(m.declared_files.len(), 2);
    }

    #[test]
    fn accepts_legacy_sicrocampo_format() {
        let raw = br#"{ "formato": "sicrocampo", "versao": "0.4" }"#;
        let m = parse(raw).expect("parse");
        assert_eq!(m.format, "sicrocampo");
        assert_eq!(m.schema_version, "0.4");
    }

    #[test]
    fn rejects_missing_format() {
        let raw = br#"{ "versao": "0.6" }"#;
        assert!(parse(raw).is_err());
    }

    #[test]
    fn rejects_unknown_format() {
        let raw = br#"{ "formato": "evil-zip", "versao": "0.6" }"#;
        assert!(parse(raw).is_err());
    }

    #[test]
    fn rejects_non_object_root() {
        assert!(parse(b"[]").is_err());
        assert!(parse(b"not json").is_err());
    }
}
