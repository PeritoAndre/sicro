//! Integration test for the MVP 3 Dossiê tables.
//!
//! Builds a synthetic .sicroapp v0.6 fixture with full Dossiê payload
//! (checklist + veículos + vítimas + vestígios + medições + observações
//! + timeline + estatísticas) and verifies that `run_import` populates
//! every Dossiê table via the new orchestrator step.
//!
//! Also exercises the rehydrator: deletes one of the structured tables
//! and confirms `rehydrate_workspace` rebuilds it from
//! `imports/<id>/original_package.sicroapp`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::json;
use sicro_desktop_lib::database::connection::open_connection;
use sicro_desktop_lib::database::repositories::dossie_repo;
use sicro_desktop_lib::importer::{rehydrate_workspace, run_import, ImportRegistry};
use sicro_desktop_lib::models::ImportSicroappInput;
use sicro_desktop_lib::workspace::manifest::SQLITE_FILENAME;

const FAKE_JPEG: &[u8] = b"\xff\xd8\xff\xe0FAKE\xff\xd9";

fn build_fixture(target: &Path) -> PathBuf {
    let pkg = target.join("transito_full.sicroapp");

    let manifest = json!({
        "formato": "sicroapp",
        "versao": "0.6",
        "gerado_em": "2026-05-25T14:30:10.000Z",
        "ocorrencia": {
            "id": "occ_full_42",
            "tipo_pericia": "transito",
            "natureza": "colisao",
            "resultado": "vitima_lesionada"
        },
        "contagens": { "fotos": 1, "vestigios": 2 },
        "arquivos": []
    });
    let caso = json!({
        "bo": "42/2026",
        "municipio": "Macapá",
        "logradouro": "Av. FAB"
    });
    let metadados = json!({
        "tipo_pericia": "transito",
        "natureza": "colisao",
        "resultado": "vitima_lesionada"
    });
    let localizacao = json!({
        "latitude": -0.035,
        "longitude": -51.066,
        "precisao_m": 4.0,
        "capturado_em": "2026-05-25T13:35:00.000Z"
    });
    let fotos = json!([
        {
            "id": "foto_001",
            "arquivo": "fotos/foto_001.jpg",
            "categoria": "veiculo",
            "capturada_em": "2026-05-25T13:36:00.000Z",
            "legenda": "Veículo principal",
            "arquivo_disponivel": true
        }
    ]);
    let checklist = json!([
        {
            "id": "ck_1",
            "categoria": "Documental",
            "pergunta": "Existe BO físico?",
            "obrigatorio": true,
            "resposta": "sim",
            "observacao": "OK",
            "origem": "base"
        },
        {
            "id": "ck_2",
            "categoria": "Cena",
            "pergunta": "Local foi isolado?",
            "obrigatorio": true,
            "resposta": "nao_verificado",
            "origem": "base"
        },
        {
            "id": "ck_3",
            "categoria": "Cena",
            "pergunta": "Há testemunhas no local?",
            "obrigatorio": false,
            "resposta": "nao",
            "origem": "adicionado"
        }
    ]);
    let veiculos = json!([
        {
            "id": "veh_1",
            "identifier": "V1",
            "placa": "ABC1D23",
            "modelo": "Civic",
            "cor": "Preto",
            "ponto_impacto": "dianteira esquerda",
            "fotos": ["foto_001"]
        }
    ]);
    let vitimas = json!([
        {
            "id": "vit_1",
            "identifier": "P1",
            "nome": "Não identificada",
            "condicao": "lesionada"
        }
    ]);
    let vestigios = json!([
        {
            "id": "tr_1",
            "identifier": "E1",
            "tipo": "frenagem",
            "descricao": "Marca de frenagem no asfalto",
            "comprimento": 12.5,
            "unidade": "m",
            "direcao": "bairro-centro",
            "fotos": ["foto_001"]
        },
        {
            "id": "tr_2",
            "identifier": "E2",
            "tipo": "fragmento",
            "descricao": "Fragmento de plástico",
            "unidade": "cm"
        }
    ]);
    let medicoes = json!([
        {
            "id": "m_1",
            "rotulo": "Frenagem V1",
            "valor": 12.5,
            "unidade": "m",
            "ponto_a": "Início frenagem",
            "ponto_b": "Posição final V1"
        }
    ]);
    let observacoes = json!([
        {
            "id": "n_1",
            "texto": "Local com chuva fraca durante perícia.",
            "categoria": "local",
            "prioridade": "normal",
            "criado_em": "2026-05-25T13:40:00.000Z"
        }
    ]);
    let timeline = json!([
        { "id": "t_1", "tipo": "ocorrencia_criada", "titulo": "Ocorrência criada",
          "ocorrido_em": "2026-05-25T13:10:00.000Z" },
        { "id": "t_2", "tipo": "primeira_foto", "titulo": "Primeira foto",
          "ocorrido_em": "2026-05-25T13:36:00.000Z" }
    ]);
    let estatisticas = json!({
        "ocorrencia_id": "occ_full_42",
        "duracao_segundos": 4200,
        "total_fotos": 1,
        "total_veiculos": 1,
        "total_vitimas": 1,
        "total_vestigios": 2,
        "total_medicoes": 1,
        "total_observacoes": 1,
        "total_checklist": 3,
        "checklist_respondidos": 2,
        "melhor_precisao_gps_m": 4.0,
        "leituras_gps": 12
    });

    let f = fs::File::create(&pkg).unwrap();
    let mut w = zip::ZipWriter::new(f);
    let opts = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);

    let mut add_json = |name: &str, v: &serde_json::Value| {
        w.start_file(name, opts).unwrap();
        w.write_all(&serde_json::to_vec_pretty(v).unwrap()).unwrap();
    };
    add_json("manifest.json", &manifest);
    add_json("caso.json", &caso);
    add_json("metadados.json", &metadados);
    add_json("localizacao.json", &localizacao);
    add_json("fotos.json", &fotos);
    add_json("checklist.json", &checklist);
    add_json("veiculos.json", &veiculos);
    add_json("vitimas.json", &vitimas);
    add_json("vestigios.json", &vestigios);
    add_json("medicoes.json", &medicoes);
    add_json("observacoes.json", &observacoes);
    add_json("timeline.json", &timeline);
    add_json("estatisticas.json", &estatisticas);

    w.start_file("fotos/foto_001.jpg", opts).unwrap();
    w.write_all(FAKE_JPEG).unwrap();
    w.finish().unwrap();

    pkg
}

fn open_workspace_db(workspace_path: &Path) -> Connection {
    open_connection(&workspace_path.join(SQLITE_FILENAME)).expect("open sqlite")
}

#[test]
fn import_populates_every_dossie_table() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = build_fixture(tmp.path());
    let parent = tmp.path().join("dest");
    fs::create_dir_all(&parent).unwrap();
    let config = tmp.path().join("config");
    fs::create_dir_all(&config).unwrap();
    let registry = ImportRegistry::open(&config);

    let result = run_import(
        ImportSicroappInput {
            package_path: pkg.to_str().unwrap().to_string(),
            parent_directory: Some(parent.to_str().unwrap().to_string()),
        },
        &parent,
        &registry,
    )
    .expect("import succeeds");

    let workspace_path = PathBuf::from(&result.workspace_path);
    let conn = open_workspace_db(&workspace_path);
    let occ_id = result.occurrence.id;

    // Checklist
    let cl = dossie_repo::list_checklist(&conn, &occ_id).unwrap();
    assert_eq!(cl.len(), 3, "expected 3 checklist items");
    let summary = dossie_repo::summarise_checklist(&cl);
    assert_eq!(summary.total, 3);
    assert_eq!(summary.answered, 2, "sim + nao should count as answered");
    assert_eq!(summary.not_verified, 1);
    assert_eq!(summary.required_total, 2);
    assert_eq!(summary.required_pending, 1, "ck_2 is required + nao_verificado");

    // Entities
    let entities = dossie_repo::list_entities(&conn, &occ_id).unwrap();
    let vehicles = entities.iter().filter(|e| e.r#type == "vehicle").count();
    let victims = entities.iter().filter(|e| e.r#type == "victim").count();
    assert_eq!(vehicles, 1);
    assert_eq!(victims, 1);
    let vehicle = entities.iter().find(|e| e.r#type == "vehicle").unwrap();
    assert!(vehicle.label.as_deref().unwrap_or("").contains("V1"));
    assert!(vehicle.label.as_deref().unwrap_or("").contains("ABC1D23"));
    assert!(vehicle.photo_ids_json.contains("foto_001"));

    // Traces
    let traces = dossie_repo::list_traces(&conn, &occ_id).unwrap();
    assert_eq!(traces.len(), 2);
    let frenagem = traces.iter().find(|t| t.identifier.as_deref() == Some("E1")).unwrap();
    assert_eq!(frenagem.length, Some(12.5));
    assert_eq!(frenagem.unit.as_deref(), Some("m"));

    // Measurements
    let meas = dossie_repo::list_measurements(&conn, &occ_id).unwrap();
    assert_eq!(meas.len(), 1);
    assert_eq!(meas[0].value, Some(12.5));

    // Notes
    let notes = dossie_repo::list_field_notes(&conn, &occ_id).unwrap();
    assert_eq!(notes.len(), 1);
    assert!(notes[0].text.as_deref().unwrap_or("").contains("chuva"));

    // Timeline
    let timeline = dossie_repo::list_timeline(&conn, &occ_id).unwrap();
    assert_eq!(timeline.len(), 2);

    // Stats
    let stats = dossie_repo::find_stats(&conn, &occ_id).unwrap().expect("stats");
    assert_eq!(stats.duration_seconds, Some(4200));
    assert_eq!(stats.traces_count, Some(2));
    assert_eq!(stats.photos_count, Some(1));
}

#[test]
fn rehydrate_repopulates_dossie_after_deletion() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = build_fixture(tmp.path());
    let parent = tmp.path().join("dest");
    fs::create_dir_all(&parent).unwrap();
    let config = tmp.path().join("config");
    fs::create_dir_all(&config).unwrap();
    let registry = ImportRegistry::open(&config);

    let result = run_import(
        ImportSicroappInput {
            package_path: pkg.to_str().unwrap().to_string(),
            parent_directory: Some(parent.to_str().unwrap().to_string()),
        },
        &parent,
        &registry,
    )
    .expect("import");

    let workspace_path = PathBuf::from(&result.workspace_path);
    let occ_id = result.occurrence.id;

    // Wipe a couple of tables to simulate an old Spike D workspace.
    {
        let conn = open_workspace_db(&workspace_path);
        dossie_repo::delete_checklist_for_occurrence(&conn, &occ_id).unwrap();
        dossie_repo::delete_traces_for_occurrence(&conn, &occ_id).unwrap();
        assert_eq!(dossie_repo::list_checklist(&conn, &occ_id).unwrap().len(), 0);
        assert_eq!(dossie_repo::list_traces(&conn, &occ_id).unwrap().len(), 0);
    }

    // Rehydrate.
    let outcome = {
        let conn = open_workspace_db(&workspace_path);
        rehydrate_workspace(&workspace_path, &conn).unwrap()
    };
    assert!(outcome.rehydrated, "rehydrate should run");
    assert_eq!(outcome.checklist_loaded, 3);
    assert_eq!(outcome.traces_loaded, 2);
    assert!(outcome.stats_loaded);

    // Confirm the tables are back.
    let conn = open_workspace_db(&workspace_path);
    assert_eq!(dossie_repo::list_checklist(&conn, &occ_id).unwrap().len(), 3);
    assert_eq!(dossie_repo::list_traces(&conn, &occ_id).unwrap().len(), 2);
}
