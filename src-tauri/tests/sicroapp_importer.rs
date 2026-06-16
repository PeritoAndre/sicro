//! Integration tests for the `.sicroapp` importer (Spike D).
//!
//! These tests build *synthetic* `.sicroapp` fixtures in memory (a few JSONs +
//! a fake JPEG) and run them through `run_import`. They DO NOT replace
//! validation with a real package exported by the SICRO Operacional mobile —
//! that validation step is part of the spike approval checklist.
//!
//! Coverage:
//!   - Happy path: valid v0.6 package imports cleanly, photos are extracted,
//!     hashes verified, report is written.
//!   - Duplicate detection: the same package SHA-256 cannot be imported twice.
//!   - Photo declared but missing from ZIP: warns but does not crash.
//!   - Hash mismatch: warns but does not crash.
//!   - Manifest with unknown format: import refuses.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::json;
use sicro_desktop_lib::importer::{run_import, ImportRegistry};
use sicro_desktop_lib::models::{ImportSicroappInput, ImportStatus};

const FAKE_JPEG_BYTES: &[u8] = b"\xff\xd8\xff\xe0FAKE-JPEG-PAYLOAD-FOR-TESTING\xff\xd9";

/// Builds a synthetic `.sicroapp` at `target` with the given JSON files and
/// fixed-content fake photos. Returns the path so the caller can pass it to
/// the importer.
struct FixtureBuilder {
    files: Vec<(String, Vec<u8>)>,
}

impl FixtureBuilder {
    fn new() -> Self {
        Self { files: Vec::new() }
    }
    fn add_json(mut self, name: &str, value: serde_json::Value) -> Self {
        self.files
            .push((name.to_string(), serde_json::to_vec_pretty(&value).unwrap()));
        self
    }
    fn add_bytes(mut self, name: &str, bytes: &[u8]) -> Self {
        self.files.push((name.to_string(), bytes.to_vec()));
        self
    }
    fn write_zip(&self, target: &Path) {
        let f = std::fs::File::create(target).expect("create sicroapp");
        let mut w = zip::ZipWriter::new(f);
        for (name, bytes) in &self.files {
            let opts = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            w.start_file(name, opts).expect("start_file");
            w.write_all(bytes).expect("write bytes");
        }
        w.finish().expect("zip finish");
    }
}

/// Build a minimum-viable v0.6 .sicroapp at `target_dir/transito_v06.sicroapp`.
/// Returns the path.
fn write_v06_transito_fixture(target_dir: &Path, with_hashes: bool) -> PathBuf {
    let manifest = json!({
        "formato": "sicroapp",
        "versao": "0.6",
        "gerado_em": "2026-05-25T14:30:10.000",
        "ocorrencia": {
            "id": "occ_test_123",
            "status": "exportada",
            "tipo_pericia": "transito",
            "natureza": "colisao",
            "resultado": "vitima_lesionada"
        },
        "contagens": { "fotos": 2 },
        "arquivos": [
            "manifest.json", "caso.json", "metadados.json", "localizacao.json",
            "fotos.json", "fotos/foto_001.jpg", "fotos/foto_002.jpg",
            "checklist.json", "observacoes.json", "hashes.json"
        ],
        "avisos": []
    });
    let caso = json!({
        "bo": "123/2026",
        "protocolo": "P-987",
        "requisicao": null,
        "delegacia": "DPCA Centro",
        "municipio": "Macapá",
        "bairro": "Trem",
        "logradouro": "Av. FAB",
        "referencia": "em frente ao posto",
        "peritos": "André Barroso; Maria Silva",
        "acionamento_em": "2026-05-25T13:10:00.000Z",
        "chegada_em": "2026-05-25T13:30:00.000Z",
        "encerramento_em": "2026-05-25T14:25:00.000Z"
    });
    let metadados = json!({
        "tipo_pericia": "transito",
        "natureza": "colisao",
        "envolvidos": ["motorista", "pedestre"],
        "resultado": "vitima_lesionada",
        "resumo": "Colisão lateral com vítima lesionada."
    });
    let localizacao = json!({
        "latitude": -0.0354,
        "longitude": -51.0666,
        "precisao_m": 3.5,
        "altitude_m": 12.0,
        "capturado_em": "2026-05-25T13:35:00.000Z",
        "origem": "gps",
        "observacao": ""
    });
    let foto1_sha = sha256_hex(FAKE_JPEG_BYTES);
    let foto2_sha = sha256_hex(FAKE_JPEG_BYTES);
    let fotos = json!([
        {
            "id": "foto_001",
            "arquivo": "fotos/foto_001.jpg",
            "categoria": "visao_geral",
            "capturada_em": "2026-05-25T13:36:00.000Z",
            "legenda": "Vista geral",
            "sha256": foto1_sha,
            "arquivo_disponivel": true
        },
        {
            "id": "foto_002",
            "arquivo": "fotos/foto_002.jpg",
            "categoria": "veiculo",
            "capturada_em": "2026-05-25T13:38:00.000Z",
            "legenda": "Veículo 1",
            "sha256": foto2_sha,
            "arquivo_disponivel": true
        }
    ]);
    let checklist = json!([]);
    let observacoes = json!([]);

    let mut fx = FixtureBuilder::new()
        .add_json("manifest.json", manifest.clone())
        .add_json("caso.json", caso)
        .add_json("metadados.json", metadados)
        .add_json("localizacao.json", localizacao)
        .add_json("fotos.json", fotos)
        .add_json("checklist.json", checklist)
        .add_json("observacoes.json", observacoes)
        .add_bytes("fotos/foto_001.jpg", FAKE_JPEG_BYTES)
        .add_bytes("fotos/foto_002.jpg", FAKE_JPEG_BYTES);

    if with_hashes {
        let hashes = json!({
            "algoritmo": "SHA-256",
            "arquivos": [
                { "caminho": "manifest.json", "sha256": sha256_hex_json(&manifest) },
                { "caminho": "fotos/foto_001.jpg", "sha256": foto1_sha },
                { "caminho": "fotos/foto_002.jpg", "sha256": foto2_sha }
            ]
        });
        fx = fx.add_json("hashes.json", hashes);
    }

    let pkg = target_dir.join("transito_v06.sicroapp");
    fx.write_zip(&pkg);
    pkg
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex_json(value: &serde_json::Value) -> String {
    sha256_hex(&serde_json::to_vec_pretty(value).unwrap())
}

#[test]
fn imports_v0_6_transito_package_happy_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let pkg = write_v06_transito_fixture(tmp.path(), true);
    let parent = tmp.path().join("dest");
    fs::create_dir_all(&parent).unwrap();

    let registry_dir = tmp.path().join("config");
    fs::create_dir_all(&registry_dir).unwrap();
    let registry = ImportRegistry::open(&registry_dir);
    let result = run_import(
        ImportSicroappInput {
            package_path: pkg.to_str().unwrap().to_string(),
            parent_directory: Some(parent.to_str().unwrap().to_string()),
        },
        &parent,
        &registry,
    )
    .expect("import succeeds");

    // Occurrence basics.
    assert_eq!(result.occurrence.numero_bo.as_deref(), Some("123/2026"));
    assert_eq!(result.occurrence.municipio.as_deref(), Some("Macapá"));
    assert_eq!(result.occurrence.tipo_pericia.as_deref(), Some("transito"));
    assert_eq!(result.occurrence.natureza.as_deref(), Some("colisao"));
    assert_eq!(
        result.occurrence.original_mobile_id.as_deref(),
        Some("occ_test_123")
    );
    assert!(result.occurrence.raw_case_json.is_some());
    assert!(result.occurrence.raw_metadata_json.is_some());
    assert!(result.occurrence.raw_location_json.is_some());
    assert!(result.occurrence.peritos.len() >= 2);

    // Import row.
    assert_eq!(result.import.format, "sicroapp");
    assert_eq!(result.import.schema_version, "0.6");

    // Report.
    let r = &result.report;
    assert_eq!(r.photos_declared, 2);
    assert_eq!(r.photos_imported, 2);
    assert_eq!(r.photos_missing, 0);
    assert!(r.hashes_present);
    assert!(r.hashes_verified_ok >= 3); // manifest + 2 photos
    assert!(r.hashes_mismatched.is_empty());
    assert!(matches!(
        r.status,
        Some(ImportStatus::Imported) | Some(ImportStatus::ImportedWithWarnings)
    ));

    // Files on disk.
    let ws = PathBuf::from(&result.workspace_path);
    assert!(ws.join("sicro.sqlite").is_file());
    assert!(ws
        .join("imports")
        .join(result.import.id.to_string())
        .join("original_package.sicroapp")
        .is_file());
    assert!(ws
        .join("imports")
        .join(result.import.id.to_string())
        .join("import_report.json")
        .is_file());

    // Both photos extracted.
    let media_dir = ws.join("media").join("photos");
    let mut extracted: Vec<_> = fs::read_dir(&media_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    extracted.sort();
    assert_eq!(extracted.len(), 2);
}

#[test]
fn refuses_duplicate_package_by_hash() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let pkg = write_v06_transito_fixture(tmp.path(), false);
    let parent = tmp.path().join("dest");
    fs::create_dir_all(&parent).unwrap();

    let registry_dir = tmp.path().join("config");
    fs::create_dir_all(&registry_dir).unwrap();
    let registry = ImportRegistry::open(&registry_dir);

    // First import succeeds.
    let _ = run_import(
        ImportSicroappInput {
            package_path: pkg.to_str().unwrap().to_string(),
            parent_directory: Some(parent.to_str().unwrap().to_string()),
        },
        &parent,
        &registry,
    )
    .expect("first import ok");

    // Second attempt with same .sicroapp must fail — registry detects the
    // package even though the second workspace is fresh.
    let again = run_import(
        ImportSicroappInput {
            package_path: pkg.to_str().unwrap().to_string(),
            parent_directory: Some(parent.to_str().unwrap().to_string()),
        },
        &parent,
        &registry,
    );
    assert!(again.is_err(), "expected duplicate import to be rejected");
    let msg = format!("{}", again.unwrap_err());
    assert!(
        msg.contains("already imported"),
        "expected 'already imported' in error, got: {msg}"
    );
}

#[test]
fn flags_photo_declared_but_missing_in_zip() {
    let tmp = tempfile::tempdir().expect("tempdir");

    // Manifest claims a photo that is NOT included in the ZIP.
    let manifest = json!({
        "formato": "sicroapp",
        "versao": "0.6",
        "gerado_em": "2026-05-25T14:30:10.000",
        "ocorrencia": { "id": "occ_missing", "tipo_pericia": "transito" },
        "contagens": { "fotos": 1 },
        "arquivos": ["manifest.json","fotos.json","caso.json"]
    });
    let fotos = json!([
        {
            "id": "foto_missing",
            "arquivo": "fotos/foto_missing.jpg",
            "categoria": "visao_geral",
            "capturada_em": "2026-05-25T13:36:00.000Z",
            "arquivo_disponivel": true
        }
    ]);
    let caso = json!({ "municipio": "Macapá" });

    let pkg = tmp.path().join("missing.sicroapp");
    FixtureBuilder::new()
        .add_json("manifest.json", manifest)
        .add_json("fotos.json", fotos)
        .add_json("caso.json", caso)
        .write_zip(&pkg);

    let parent = tmp.path().join("dest");
    fs::create_dir_all(&parent).unwrap();
    let registry_dir = tmp.path().join("config");
    fs::create_dir_all(&registry_dir).unwrap();
    let registry = ImportRegistry::open(&registry_dir);
    let result = run_import(
        ImportSicroappInput {
            package_path: pkg.to_str().unwrap().to_string(),
            parent_directory: Some(parent.to_str().unwrap().to_string()),
        },
        &parent,
        &registry,
    )
    .expect("import should not abort because of a missing photo");

    assert_eq!(result.report.photos_declared, 1);
    assert_eq!(result.report.photos_imported, 0);
    assert_eq!(result.report.photos_missing, 1);
    assert!(
        result
            .report
            .warnings
            .iter()
            .any(|w| w.contains("foto_missing")),
        "expected warning for missing photo"
    );
}

#[test]
fn detects_hash_mismatch_in_hashes_json() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let pkg = tmp.path().join("badhash.sicroapp");

    let manifest = json!({
        "formato": "sicroapp",
        "versao": "0.6",
        "ocorrencia": { "id": "occ_badhash", "tipo_pericia": "transito" }
    });
    let caso = json!({ "municipio": "Macapá" });
    let fotos = json!([]);

    let hashes = json!({
        "algoritmo": "SHA-256",
        "arquivos": [
            { "caminho": "caso.json", "sha256": "deadbeef" } // wrong
        ]
    });

    FixtureBuilder::new()
        .add_json("manifest.json", manifest)
        .add_json("caso.json", caso)
        .add_json("fotos.json", fotos)
        .add_json("hashes.json", hashes)
        .write_zip(&pkg);

    let parent = tmp.path().join("dest");
    fs::create_dir_all(&parent).unwrap();
    let registry_dir = tmp.path().join("config");
    fs::create_dir_all(&registry_dir).unwrap();
    let registry = ImportRegistry::open(&registry_dir);
    let result = run_import(
        ImportSicroappInput {
            package_path: pkg.to_str().unwrap().to_string(),
            parent_directory: Some(parent.to_str().unwrap().to_string()),
        },
        &parent,
        &registry,
    )
    .expect("import should not abort on hash mismatch");

    assert!(result.report.hashes_present);
    assert!(!result.report.hashes_mismatched.is_empty());
    assert!(result
        .report
        .warnings
        .iter()
        .any(|w| w.contains("hash mismatch")));
}

#[test]
fn rejects_unknown_manifest_format() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let pkg = tmp.path().join("evil.sicroapp");

    let manifest = json!({ "formato": "evil-zip", "versao": "0.6" });
    FixtureBuilder::new()
        .add_json("manifest.json", manifest)
        .write_zip(&pkg);

    let parent = tmp.path().join("dest");
    fs::create_dir_all(&parent).unwrap();
    let registry_dir = tmp.path().join("config");
    fs::create_dir_all(&registry_dir).unwrap();
    let registry = ImportRegistry::open(&registry_dir);
    let result = run_import(
        ImportSicroappInput {
            package_path: pkg.to_str().unwrap().to_string(),
            parent_directory: Some(parent.to_str().unwrap().to_string()),
        },
        &parent,
        &registry,
    );
    assert!(result.is_err(), "expected unknown format to be refused");
}
