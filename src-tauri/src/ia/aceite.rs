//! Aceites do P2.1 que pedem processo próprio (um onnxruntime por processo) ou dados reais do dono (só cópias).

use std::path::{Path, PathBuf};

use crate::commands::settings_commands::AiSettings;
use crate::edicao::mensagens::codigo;
use crate::ia::testes as t;

/// Roda `<teste>` deste binário num processo filho com as variáveis dadas; devolve o stdout.
fn filho(teste: &str, env: &[(&str, &str)], sem: &[&str]) -> String {
    let exe = std::env::current_exe().unwrap();
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["--exact", teste, "--nocapture", "--test-threads", "1"]);
    for (k, v) in env {
        cmd.env(k, v);
    }
    for k in sem {
        cmd.env_remove(k);
    }
    let o = cmd.output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(o.status.success(), "filho {teste}: {:?}\n{out}\n{}", o.status, String::from_utf8_lossy(&o.stderr));
    out
}

/// O que vem depois de `marca` numa linha (o libtest põe "test … ... " antes do que o filho imprime).
fn depois<'a>(out: &'a str, marca: &str) -> Option<&'a str> {
    out.lines().find_map(|l| l.find(marca).map(|i| &l[i + marca.len()..]))
}

/// onnxruntime mapeado neste processo?
fn lib_mapeada() -> bool {
    std::fs::read_to_string("/proc/self/maps").unwrap_or_default().contains("libonnxruntime.so")
}

fn mensagem<T>(r: crate::error::Result<T>) -> String {
    match r {
        Ok(_) => "ok".into(),
        Err(crate::error::SicroError::Validation(m)) => m,
        Err(e) => e.to_string(),
    }
}

/// No filho: com o interruptor desligado nas Configurações em vigor, Pessoas (YOLOX), super-resolução e a fábrica
/// recusam com `ia_desligada` sem carregar o onnxruntime; religado, o mesmo YOLOX abre em CPU.
#[test]
fn filho_interruptor() {
    if std::env::var("SICRO_IA_FILHO").as_deref() != Ok("interruptor") {
        return;
    }
    let lib = t::ort_lib().unwrap();
    let yolox = Path::new(t::DOSSIE).join("p2/modelos/yolox_s.onnx");
    let sr = Path::new(t::DOSSIE).join("p2/modelos/realesr-general-x4v3.onnx");
    let desligada = AiSettings { modelos_desligados: true, ort_lib_path: lib.to_string_lossy().into_owned(), ..Default::default() };
    crate::ia::sessao::lembrar(&desligada);
    let r = [
        mensagem(crate::video::pessoas::yolox::Yolox::load(&lib, &yolox)),
        mensagem(crate::video::realce::superres::SuperRes::load(&lib, &sr)),
        mensagem(crate::ia::criar_sessao(&desligada, &yolox, &crate::ia::Opcoes::default())),
        mensagem(crate::ia::criar_sessao_de_bytes(&desligada, crate::ia::gpu::MODELO_AUTOTESTE, &crate::ia::Opcoes::so_cpu())),
    ];
    let mapeada_desligada = lib_mapeada();
    crate::ia::sessao::lembrar(&AiSettings { modelos_desligados: false, ..desligada });
    let (ligada, provedor) = match crate::video::pessoas::yolox::Yolox::load(&lib, &yolox) {
        Ok(y) => ("ok".to_string(), Some(y.provedor.fio())),
        Err(e) => (e.to_string(), None),
    };
    println!("INTERRUPTOR {}", serde_json::json!({ "desligada": r, "mapeada_desligada": mapeada_desligada, "ligada": ligada, "provedor": provedor, "mapeada_ligada": lib_mapeada() }));
}

/// Aceite: "interruptor desliga todos os usos" do lado do backend (o da transcrição e dos locutores está em
/// `commands::audio_commands::testes`): processo novo, nada de onnxruntime carregado enquanto desligado.
#[test]
fn interruptor_desliga_os_modelos_sem_carregar_o_onnxruntime() {
    let Some(_) = t::ou_dispensa(t::ort_lib(), "interruptor_desliga_os_modelos_sem_carregar_o_onnxruntime", "a lib dos testes") else { return };
    let yolox = Path::new(t::DOSSIE).join("p2/modelos/yolox_s.onnx");
    let Some(_) = t::ou_dispensa(yolox.is_file().then_some(()), "interruptor_desliga_os_modelos_sem_carregar_o_onnxruntime", "yolox_s.onnx") else { return };
    let out = filho("ia::aceite::filho_interruptor", &[("SICRO_IA_FILHO", "interruptor"), ("ORT_DISABLE_TELEMETRY", "1")], &[]);
    let linha = depois(&out, "INTERRUPTOR ").unwrap_or_else(|| panic!("{out}"));
    let v: serde_json::Value = serde_json::from_str(linha).unwrap();
    for m in v["desligada"].as_array().unwrap() {
        assert_eq!(m.as_str(), Some(codigo::IA_DESLIGADA), "{v}");
    }
    assert_eq!(v["mapeada_desligada"], false, "{v}");
    assert_eq!((v["ligada"].as_str(), v["provedor"].as_str(), v["mapeada_ligada"].as_bool()), (Some("ok"), Some("cpu"), Some(true)), "{v}");
}

/// No filho sem `ORT_DISABLE_TELEMETRY`: o ambiente recusa antes de abrir a lib (caminho falso: nada carrega nem
/// com o código errado).
#[test]
fn filho_sem_telemetria() {
    if std::env::var("SICRO_IA_FILHO").as_deref() != Ok("telemetria") {
        return;
    }
    let r = crate::ia::sessao::ambiente(Path::new("/nao/existe/libonnxruntime.so.1.31.1"));
    println!("TELEMETRIA {}", serde_json::json!({ "erro": mensagem(r.map(|_| ())), "carregada": crate::ia::sessao::carregada().is_some() }));
}

/// Telemetria desligada em todo processo que carrega onnxruntime: sem a variável o SICRO não abre a lib.
#[test]
fn sem_a_variavel_da_telemetria_nao_carrega() {
    let out = filho("ia::aceite::filho_sem_telemetria", &[("SICRO_IA_FILHO", "telemetria")], &["ORT_DISABLE_TELEMETRY"]);
    let linha = depois(&out, "TELEMETRIA ").unwrap_or_else(|| panic!("{out}"));
    let v: serde_json::Value = serde_json::from_str(linha).unwrap();
    assert!(v["erro"].as_str().unwrap().contains("ORT_DISABLE_TELEMETRY"), "{v}");
    assert_eq!(v["carregada"], false);
}

/// Casos do dono com corrida de Pessoas (`video_people_runs`), lidos do índice; cada um copiado (manifesto e banco)
/// para `destino`. Nunca abre o original.
fn copias_dos_casos_com_analise(destino: &Path) -> Vec<(PathBuf, Vec<(String, u32, u32)>)> {
    let Some(home) = std::env::var_os("HOME") else { return Vec::new() };
    let indice = PathBuf::from(home).join(".config/br.org.sicro.desktop/case-index.json");
    let Ok(txt) = std::fs::read_to_string(&indice) else { return Vec::new() };
    let v: serde_json::Value = serde_json::from_str(&txt).unwrap_or_default();
    let mut casos = Vec::new();
    for (i, e) in v["entries"].as_array().into_iter().flatten().enumerate() {
        let Some(ws) = e["workspace_path"].as_str().map(PathBuf::from) else { continue };
        if !ws.join("sicro.sqlite").is_file() || !ws.join("manifest.json").is_file() {
            continue;
        }
        let copia = destino.join(format!("caso{i}"));
        std::fs::create_dir_all(&copia).unwrap();
        for n in ["manifest.json", "sicro.sqlite", "sicro.sqlite-wal", "sicro.sqlite-shm"] {
            if ws.join(n).is_file() {
                std::fs::copy(ws.join(n), copia.join(n)).unwrap();
            }
        }
        let conn = rusqlite::Connection::open(copia.join("sicro.sqlite")).unwrap();
        let runs: Vec<(String, u32, u32)> = conn
            .prepare("select media_hash, width, height from video_people_runs order by media_hash")
            .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect())
            .unwrap_or_default();
        drop(conn);
        if runs.is_empty() {
            let _ = std::fs::remove_dir_all(&copia);
        } else {
            casos.push((copia, runs));
        }
    }
    casos
}

/// Aceite: "as 3 análises reais continuam abrindo" (`mapa-analise.md` 11.9): cópias dos casos do dono abertas por
/// `pessoas_listar` com o código desta fase; a corrida e as trilhas (9, com caixas) voltam com o tamanho gravado.
/// As Configurações reais do dono (só leitura) abrem com os campos novos no padrão.
#[test]
fn as_tres_analises_reais_continuam_abrindo() {
    let d = std::env::temp_dir().join(format!("p21-analises-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let casos = copias_dos_casos_com_analise(&d);
    let n_runs: usize = casos.iter().map(|(_, r)| r.len()).sum();
    if t::ou_dispensa((n_runs >= 3).then_some(()), "as_tres_analises_reais_continuam_abrindo", "as 3 análises reais (casos do dono)").is_none() {
        let _ = std::fs::remove_dir_all(&d);
        return;
    }
    let mut trilhas = 0;
    for (copia, runs) in &casos {
        for (hash, w, h) in runs {
            let e = tauri::async_runtime::block_on(crate::commands::pessoas_commands::pessoas_listar(copia.to_string_lossy().into_owned(), hash.clone())).unwrap();
            let run = e.run.as_ref().expect("corrida");
            assert_eq!((run.width, run.height), (*w, *h));
            assert!(run.frames_analyzed > 0 && run.model.contains("YOLOX"));
            assert!(!e.pessoas.is_empty());
            for p in &e.pessoas {
                assert!(!p.caixas.is_empty() && p.caixas.len() >= p.quadros, "{}", p.id);
                assert!(p.caixas.iter().all(|c| c.iter().all(|v| v.is_finite())));
            }
            trilhas += e.pessoas.len();
        }
    }
    eprintln!("análises reais: {n_runs} corridas, {trilhas} trilhas");
    assert!(trilhas >= 9);
    let _ = std::fs::remove_dir_all(&d);

    let cfg = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/br.org.sicro.desktop/app-settings.json"));
    if let Some(b) = cfg.and_then(|c| std::fs::read(c).ok()) {
        let s: crate::commands::settings_commands::AppSettings = serde_json::from_slice(&b).expect("app-settings.json do dono abre");
        let bruto: serde_json::Value = serde_json::from_slice(&b).unwrap();
        assert_eq!(s.ai.ort_lib_path, bruto["ai"]["ort_lib_path"].as_str().unwrap_or(""));
        assert_eq!(s.ai.modelos_desligados, bruto["ai"]["modelos_desligados"].as_bool().unwrap_or(false));
    }
}
