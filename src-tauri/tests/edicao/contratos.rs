//! Verificação das Fases 0 e 1, lente de contratos: o projeto que o FRONT grava (fixtures `verif-fase0-do-store.json`
//! e `verif-fase1-do-store.json`, geradas pelo store real em `src/modules/edicao/store/fase{0,1}DoStore.test.ts*`,
//! com todo recurso de cada fase) é o que o Rust lê: tipado sem item desconhecido, nenhum campo perdido na ida e
//! volta, pts gravados pelo front iguais aos que o Rust prende, validação sem erro e compilação e render nos três
//! alvos, nos dois binários. O da Fase 1 exporta até o pacote do júri e abre o PDF por fora.
//! Incluído por `edicao/mod.rs` (`#[path]`), como a ponta a ponta: usa `pub(crate)` e a matriz de `edicao::testes`.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde_json::{json, Value};

use crate::commands::edicao_commands as cmd;
use crate::database::repositories::video_speed_repo;
use crate::edicao::armazem;
use crate::edicao::executar;
use crate::edicao::fila::{self, Estado};
use crate::edicao::medidas;
use crate::edicao::modelo::{
    Caixa, ClipePrincipal, ClipeSobreposto, Efeito, Nivel, OndePrender, ProjetoEdicao, RetratoCalculo, RetratoCalibracao, TipoMidia, Tolerante,
};
use crate::edicao::pacote;
use crate::models::{VideoSpeedCalculation, VideoSpeedCalibration};
use crate::edicao::motor::{self, Alvo};
use crate::edicao::quadros::{AlvoQuadro, TabelaQuadros};
use crate::edicao::rastreio::avaliar::{self, Instante};
use crate::edicao::testes::fontes::{self, Fonte};
use crate::workspace::paths;

const FIXTURE: &str = include_str!("../fixtures/edicao/verif-fase0-do-store.json");
/// Caixa que "Adicionar" põe no rastreio novo (`CAIXA_CENTRO`, inspetor/novos.ts).
const CAIXA_CENTRO: Caixa = Caixa { x: 0.4, y: 0.4, w: 0.2, h: 0.2 };
/// Teto de espera de uma tarefa da fila (folga de 3× sobre o medido com a máquina ocupada).
const ESPERA_MAX_S: u64 = 300;

fn doc() -> Value {
    serde_json::from_str(FIXTURE).expect("fixture do store")
}

/// Projeto da Fase 1 pelas portas da tela (`fase1DoStore.test.tsx`).
const FIXTURE_1: &str = include_str!("../fixtures/edicao/verif-fase1-do-store.json");
/// Calibração e cálculo como a análise os lista (`list_speed_*`): as linhas de que o vitest tirou os retratos.
const ANALISE_1: &str = include_str!("../fixtures/edicao/verif-fase1-analise.json");

fn doc1() -> Value {
    serde_json::from_str(FIXTURE_1).expect("fixture do store (Fase 1)")
}

/// Chaves que só o Rust regrava (M2): `Chave.pts`/`interp` e `Intervalo.de`/`ate` ausentes no TS.
fn so_do_rust(caminho: &str, chave: &str) -> bool {
    matches!(chave, "pts" | "interp" | "de" | "ate") && (caminho.contains("ancoras") || caminho.contains("regiao") || caminho.contains("ativo") || caminho.contains("volume"))
}

/// Diferenças entre o que o front gravou (`a`) e o que o Rust regrava (`b`): campo perdido, valor mudado, campo novo.
fn diferencas(a: &Value, b: &Value, caminho: &str, saida: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, va) in x {
                match y.get(k) {
                    None => saida.push(format!("{caminho}.{k}: o Rust não lê (perdido ao regravar)")),
                    Some(vb) => diferencas(va, vb, &format!("{caminho}.{k}"), saida),
                }
            }
            for k in y.keys().filter(|k| !x.contains_key(*k)) {
                if !so_do_rust(caminho, k) {
                    saida.push(format!("{caminho}.{k}: o Rust exige e o front não grava"));
                }
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (va, vb)) in x.iter().zip(y).enumerate() {
                diferencas(va, vb, &format!("{caminho}[{i}]"), saida);
            }
        }
        (Value::Number(x), Value::Number(y)) if x.as_f64() == y.as_f64() => {}
        _ if a == b => {}
        _ => saida.push(format!("{caminho}: {a} ≠ {b}")),
    }
}

fn desconhecidos(p: &ProjetoEdicao) -> Vec<String> {
    let mut v = Vec::new();
    let efeitos = |l: &[Tolerante<Efeito>], onde: &str, v: &mut Vec<String>| {
        for (i, e) in l.iter().enumerate() {
            if let Tolerante::Desconhecido(x) = e {
                v.push(format!("{onde}.efeitos[{i}] = {x}"));
            }
        }
    };
    for (i, c) in p.principal.iter().enumerate() {
        match c {
            Tolerante::Conhecido(c) => efeitos(&base(c).efeitos, &format!("principal[{i}]"), &mut v),
            Tolerante::Desconhecido(x) => v.push(format!("principal[{i}] = {x}")),
        }
    }
    for t in &p.sobreposicoes {
        for (i, c) in t.clipes.iter().enumerate() {
            match c {
                Tolerante::Conhecido(ClipeSobreposto::Video(s)) => efeitos(&s.base.efeitos, "sobreposto", &mut v),
                Tolerante::Conhecido(ClipeSobreposto::Imagem(s)) => efeitos(&s.base.efeitos, "sobreposto", &mut v),
                Tolerante::Conhecido(ClipeSobreposto::Titulo(s)) => efeitos(&s.base.efeitos, "sobreposto", &mut v),
                Tolerante::Desconhecido(x) => v.push(format!("sobreposto[{i}] = {x}")),
            }
        }
    }
    v
}

fn base(c: &ClipePrincipal) -> &crate::edicao::modelo::ClipeBase {
    match c {
        ClipePrincipal::Trecho(x) => &x.base,
        ClipePrincipal::Congelar(x) => &x.base,
        ClipePrincipal::Imagem(x) => &x.base,
        ClipePrincipal::Titulo(x) => &x.base,
        ClipePrincipal::Tela(x) => &x.base,
    }
}

/// O front grava, o Rust lê tipado sem `Desconhecido` e regrava sem perder nem inventar campo (fora o M2).
#[test]
fn projeto_da_fase0_do_store_le_tipado_e_volta_igual() {
    let v = doc();
    let p = armazem::tipado(&v).expect("tipado");
    assert_eq!(desconhecidos(&p), Vec::<String>::new());
    let volta = serde_json::to_value(&p).unwrap();
    let mut d = Vec::new();
    diferencas(&v, &volta, "$", &mut d);
    assert_eq!(d, Vec::<String>::new(), "front × Rust");
    // tudo o que a fixture promete está lá (o vitest confere o lado do front)
    let tipos: Vec<&str> = v["principal"].as_array().unwrap().iter().map(|c| c["tipo"].as_str().unwrap()).collect();
    assert_eq!(tipos, ["titulo", "trecho", "congelar", "trecho", "trecho", "imagem"]);
}

// ------------------------------------------------------------------ caso de verdade

struct Caso {
    _d: tempfile::TempDir,
    ws: PathBuf,
    app: PathBuf,
}

fn caso() -> Caso {
    let d = tempfile::Builder::new().prefix("p0v-contratos-").tempdir().expect("pasta temporária");
    let pai = d.path().join("Casos ç");
    std::fs::create_dir_all(&pai).unwrap();
    let criado = crate::workspace::create::create_workspace(
        crate::models::NewOccurrenceInput {
            titulo: Some("Contratos".into()),
            numero_bo: None,
            protocolo: None,
            oficio: None,
            tipo_pericia: None,
            municipio: None,
            peritos: vec!["Perito Contratos".into()],
            parent_directory: Some(pai.to_string_lossy().into()),
        },
        &pai,
    )
    .unwrap();
    let app = d.path().join("cache do app");
    Caso { _d: d, ws: criado.path, app }
}

fn ffmpeg(cap: &crate::edicao::ffmpeg::sonda::Capacidades, args: &[&str], saida: &Path) {
    let st = crate::tools::command(&cap.binario).args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error"]).args(args).arg(saida).status().unwrap();
    assert!(st.success(), "{args:?}");
}

/// Vídeo registrado como o app registra; tabela do decodificador guardada no cache (o que a fila de fundo faria).
fn registrar(cap: &crate::edicao::ffmpeg::sonda::Capacidades, c: &Caso, f: &Path) -> (crate::models::VideoMedia, TabelaQuadros) {
    let vm = tauri::async_runtime::block_on(crate::commands::video_commands::register_video_media(
        c.ws.to_string_lossy().into(),
        crate::models::RegisterVideoInput { source_path: f.to_string_lossy().into(), allow_duplicate: false },
    ))
    .expect("registrar vídeo");
    let caminho = paths::resolver_relativo(&c.ws, &vm.relative_path);
    let tab = TabelaQuadros::do_decodificador(cap, &caminho, &AtomicBool::new(false), &mut |_| {}).unwrap();
    tab.clone().guardar(&cmd::cache_do_caso(&c.app, &c.ws).unwrap(), &vm.sha256).unwrap();
    (vm, tab)
}

/// A fixture com as mídias de mentira trocadas pelas do caso (mesmo id local; ocultações e o resto como o front gravou).
fn montar(cap: &crate::edicao::ffmpeg::sonda::Capacidades, c: &Caso) -> (Value, TabelaQuadros) {
    let (v, t, _) = montar_de(cap, c, doc());
    (v, t)
}

/// O mesmo para qualquer fixture do store; devolve também os dois vídeos registrados (m1, m2).
fn montar_de(cap: &crate::edicao::ffmpeg::sonda::Capacidades, c: &Caso, mut v: Value) -> (Value, TabelaQuadros, [crate::models::VideoMedia; 2]) {
    let id = v["id"].as_str().unwrap().to_string();
    let pasta = fontes::pasta_temporaria("p0v-contratos");
    let f1 = fontes::gerar(Fonte::Mp4).unwrap();
    // segunda câmera: a mesma tabela de tempo (o espelho do front a imita), outra imagem e outro SHA-256
    let f2 = pasta.join("cam2.mp4");
    let f1s = f1.to_string_lossy().to_string();
    ffmpeg(cap, &["-i", &f1s, "-vf", "negate", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-bf", "0", "-g", "30", "-video_track_timescale", "15360", "-c:a", "copy"], &f2);
    let (v1, t1) = registrar(cap, c, &f1);
    let (v2, t2) = registrar(cap, c, &f2);
    // a tabela que o front imita (`TABELA` do vitest): pts 512·k, 450 quadros
    let esperado: Vec<i64> = (0..450).map(|k| 512 * k).collect();
    assert_eq!((t1.tb.num, t1.tb.den, &t1.pts), (1, 15360, &esperado), "{}", cap.versao);
    assert_eq!(t2.pts, t1.pts, "{}", cap.versao);
    let png = pasta.join("brasão.png");
    image::RgbaImage::from_pixel(64, 32, image::Rgba([200, 30, 30, 255])).save(&png).unwrap();
    let wav = pasta.join("narração.wav");
    ffmpeg(cap, &["-f", "lavfi", "-i", "sine=frequency=1000:sample_rate=48000:duration=2", "-c:a", "pcm_s16le"], &wav);
    let img = serde_json::to_value(armazem::importar_midia(&c.ws, &id, &png, TipoMidia::Imagem).unwrap()).unwrap();
    let som = serde_json::to_value(armazem::importar_midia(&c.ws, &id, &wav, TipoMidia::Audio).unwrap()).unwrap();
    // música (só a fixture da Fase 1 a tem): outro som, outro SHA-256
    let musica = v["midias"].as_array().unwrap().iter().any(|m| m["id"] == "m5").then(|| {
        let wav = pasta.join("música.wav");
        ffmpeg(cap, &["-f", "lavfi", "-i", "sine=frequency=330:sample_rate=48000:duration=2", "-c:a", "pcm_s16le"], &wav);
        serde_json::to_value(armazem::importar_midia(&c.ws, &id, &wav, TipoMidia::Audio).unwrap()).unwrap()
    });
    for m in v["midias"].as_array_mut().unwrap() {
        let (vm, propria) = match m["id"].as_str().unwrap() {
            "m1" => (Some(&v1), None),
            "m2" => (Some(&v2), None),
            "m3" => (None, Some(&img)),
            "m4" => (None, Some(&som)),
            "m5" => (None, musica.as_ref()),
            outro => panic!("mídia {outro}"),
        };
        if let Some(vm) = vm {
            m["sha256"] = json!(vm.sha256);
            m["media_id"] = json!(vm.id.to_string());
            m["nome"] = json!(vm.filename);
            m["relativo"] = json!(vm.relative_path);
            m["duracao_s"] = json!(vm.duration_s);
        }
        if let Some(p) = propria {
            let oc = m["ocultacoes"].clone();
            let id_local = m["id"].clone();
            *m = p.clone();
            m["id"] = id_local;
            m["ocultacoes"] = oc;
        }
    }
    (v, t1, [v1, v2])
}

fn esperar(id: &str) -> Estado {
    let (tx, rx) = std::sync::mpsc::channel();
    let id = id.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(fila::esperar(&id));
    });
    rx.recv_timeout(std::time::Duration::from_secs(ESPERA_MAX_S)).expect("tarefa presa").expect("esperar")
}

/// `Analise.prender` aplicado como a store aplica (edicaoStore.ts, `aplicarAnalise`: dado do backend, sem passo).
fn aplicar_prender(doc: &mut Value, p: &crate::edicao::modelo::Prender) {
    let clipe = |doc: &Value, id: &str| doc["principal"].as_array().unwrap().iter().position(|c| c["id"] == id).expect("clipe");
    let (alvo, t, pts) = match &p.onde {
        OndePrender::Ancora { rastreio, indice } => {
            let r = doc["rastreios"].as_array().unwrap().iter().position(|r| r["id"] == rastreio.as_str()).expect("rastreio");
            (&mut doc["rastreios"][r]["ancoras"][*indice as usize], "t", "pts")
        }
        OndePrender::Inicio { clipe: id } => {
            let i = clipe(doc, id);
            (&mut doc["principal"][i], "inicio_s", "inicio_pts")
        }
        OndePrender::Fim { clipe: id } => {
            let i = clipe(doc, id);
            (&mut doc["principal"][i], "fim_s", "fim_pts")
        }
        OndePrender::Instante { clipe: id } => {
            let i = clipe(doc, id);
            (&mut doc["principal"][i], "instante_s", "instante_pts")
        }
        OndePrender::AnaliseDe { clipe: id, efeito } | OndePrender::AnaliseAte { clipe: id, efeito } => {
            let i = clipe(doc, id);
            let e = doc["principal"][i]["efeitos"].as_array().unwrap().iter().position(|e| e["id"] == efeito.as_str()).expect("efeito");
            let de = matches!(p.onde, OndePrender::AnaliseDe { .. });
            (&mut doc["principal"][i]["efeitos"][e]["analise"], if de { "de_s" } else { "ate_s" }, if de { "de_pts" } else { "ate_pts" })
        }
    };
    alvo[t] = json!(p.t);
    alvo[pts] = json!(p.pts);
}

/// `edicao_analisar` como a tela o usa: a resposta traz `prender`, a store grava e pede de novo.
fn analisar_como_a_tela(c: &Caso, mut doc: Value) -> (Value, crate::edicao::modelo::Analise) {
    for _ in 0..3 {
        let a = cmd::analisar(&c.ws, &doc, &c.app).expect("edicao_analisar");
        if a.prender.is_empty() {
            return (doc, a);
        }
        for p in &a.prender {
            aplicar_prender(&mut doc, p);
        }
    }
    panic!("prender não converge");
}

/// Erro `intervalo` no `ativo` [t, t] da ocultação criada no congelado (contratos (h) 23 × validar.rs).
fn erro_do_instante(doc: &Value, p: &crate::edicao::modelo::Problema) -> bool {
    p.codigo == "intervalo"
        && p.campo.as_deref() == Some("ativo")
        && doc["midias"].as_array().unwrap().iter().flat_map(|m| m["ocultacoes"].as_array().unwrap()).any(|o| {
            Some(o["id"].as_str().unwrap()) == p.efeito.as_deref() && o["ativo"]["de"].as_f64().is_some() && o["ativo"]["de"] == o["ativo"]["ate"]
        })
}

/// O projeto do store no caso: os pts que o front gravou são os que o Rust prende, a análise não tem erro, e os
/// três alvos compilam e renderizam (exportação até "A conferir", prévia e quadro final prontos).
#[test]
fn projeto_da_fase0_do_store_valida_compila_e_renderiza_nos_tres_alvos() {
    let Some(cap) = fontes::cap() else { return };
    let c = caso();
    let (v, tab) = montar(&cap, &c);
    armazem::salvar(&c.ws, &v).expect("edicao_salvar");
    let id = v["id"].as_str().unwrap();
    let aberto = cmd::abrir(&c.ws, id, &c.app, &fila::projeto_ocupado).expect("edicao_abrir");
    assert!(!aberto.so_leitura && aberto.avisos.is_empty(), "{:?}", aberto.avisos);

    // pts do front = quadro exibido que o Rust acha (`edicao_quadro_em`) em todo trecho e congelado
    for cl in aberto.projeto["principal"].as_array().unwrap() {
        let par = |t: &str, p: &str| (cl[t].as_f64(), cl[p].as_i64());
        for (t, pts) in [par("inicio_s", "inicio_pts"), par("fim_s", "fim_pts"), par("instante_s", "instante_pts")] {
            if let (Some(t), Some(pts)) = (t, pts) {
                assert_eq!(tab.quadro_em(AlvoQuadro::T { t }).unwrap().pts, pts, "{}", cl["id"]);
            }
        }
    }
    // o front prendeu início, fim e instante; só as âncoras padrão (sem pts) ficam para o Rust prender
    let primeira = cmd::analisar(&c.ws, &aberto.projeto, &c.app).unwrap();
    let presos: Vec<_> = primeira.prender.iter().filter(|p| !matches!(p.onde, OndePrender::Ancora { .. })).collect();
    assert!(presos.is_empty(), "{presos:?}");

    let (doc, analise) = analisar_como_a_tela(&c, aberto.projeto);
    for p in &analise.problemas {
        eprintln!("problema ({}): {:?} {} {:?} {:?} {:?}", cap.versao, p.nivel, p.codigo, p.clipe, p.efeito, p.campo);
    }
    let erros: Vec<_> = analise.problemas.iter().filter(|p| p.nivel == Nivel::Erro).collect();
    assert!(erros.is_empty(), "{erros:?}");
    let total = analise.linha.total_q;
    assert_eq!(total, 465, "título 60 + trecho a 0,5× 120 + congelado 45 + 90 + 120 + imagem 30");

    let p = armazem::tipado(&doc).unwrap();
    let cache = cmd::cache_do_caso(&c.app, &c.ws).unwrap();
    let prep = executar::preparar(&c.ws, &p, &cache, true).unwrap();
    let alvos = [
        Alvo::Final { exportacao: p.saida.exportacao.clone() },
        Alvo::Previa { altura: 180, de_q: 0, ate_q: total },
        Alvo::Quadro { q: 200, largura: 320 },
    ];
    for alvo in &alvos {
        let plano = motor::compilar(&p, &prep.contexto(), alvo).unwrap_or_else(|e| panic!("compilar {alvo:?}: {e}"));
        assert!(!plano.blocos.is_empty(), "{alvo:?}");
    }
    drop(prep);

    // render: os três pela fila, como a tela pede (a gravação vem antes, como o `descarregar` da tela)
    armazem::salvar(&c.ws, &doc).unwrap();
    let ciencias: Vec<String> = analise.problemas.iter().filter(|p| p.nivel == Nivel::Pericial).map(|p| p.codigo.to_string()).collect();
    for alvo in alvos {
        let tarefa = fila::enfileirar(cmd::pedido(&c.ws, doc.clone(), alvo.clone(), ciencias.clone(), &c.app).unwrap()).unwrap();
        match (&alvo, esperar(&tarefa)) {
            (Alvo::Final { .. }, Estado::AConferir { saida_id }) => {
                let s = fila::saidas(&c.ws, Some(id)).unwrap().into_iter().find(|s| s.id == saida_id).expect("saída registrada");
                assert_eq!((s.n_quadros, s.exige_conferencia), (total, true), "{}", cap.versao);
                assert!(paths::resolver_relativo(&c.ws, &s.arquivo).is_file(), "{}", s.arquivo);
            }
            (Alvo::Previa { .. } | Alvo::Quadro { .. }, Estado::Pronta { relativo }) => {
                assert!(cache.join(&relativo).is_file(), "{relativo}");
            }
            (a, e) => panic!("{a:?} terminou em {e:?} ({})", cap.versao),
        }
    }
}

// ------------------------------------------------------------------ Fase 1

/// O projeto da Fase 1 que o front grava é lido tipado, sem `Desconhecido`, e volta sem perder nem inventar campo.
#[test]
fn projeto_da_fase1_do_store_le_tipado_e_volta_igual() {
    let v = doc1();
    let p = armazem::tipado(&v).expect("tipado");
    assert_eq!(desconhecidos(&p), Vec::<String>::new());
    let volta = serde_json::to_value(&p).unwrap();
    let mut d = Vec::new();
    diferencas(&v, &volta, "$", &mut d);
    assert_eq!(d, Vec::<String>::new(), "front × Rust");
    let tipos: Vec<&str> = v["principal"].as_array().unwrap().iter().map(|c| c["tipo"].as_str().unwrap()).collect();
    assert_eq!(tipos, ["titulo", "trecho", "trecho", "titulo", "trecho", "congelar", "trecho", "imagem", "titulo"]);
}

/// As linhas da análise que o vitest mostra nas listas, gravadas no banco do caso na mídia m1; devolve os retratos
/// como `edicao_retrato_calibracao`/`edicao_retrato_calculo` os entregam à tela.
fn analise_no_banco(c: &Caso, sha_m1: &str) -> (RetratoCalibracao, RetratoCalculo) {
    let occ = crate::workspace::manifest::Manifest::read(&c.ws).unwrap().occurrence_id;
    let mut a: Value = serde_json::from_str(ANALISE_1).unwrap();
    for k in ["calibracao", "calculo"] {
        a[k]["occurrence_id"] = json!(occ.to_string());
        a[k]["media_hash"] = json!(sha_m1);
    }
    let cal: VideoSpeedCalibration = serde_json::from_value(a["calibracao"].clone()).expect("calibração do front lida pelo Rust");
    let calc: VideoSpeedCalculation = serde_json::from_value(a["calculo"].clone()).expect("cálculo do front lido pelo Rust");
    let conn = crate::database::connection::open_connection(&paths::sqlite_path(&c.ws)).unwrap();
    video_speed_repo::insert_calibration(&conn, &cal).unwrap();
    video_speed_repo::insert_calculation(&conn, &calc).unwrap();
    (medidas::retrato_calibracao(&c.ws, &cal.id.to_string()).unwrap(), medidas::retrato_calculo(&c.ws, &calc.id.to_string()).unwrap())
}

/// Troca os retratos que o vitest pôs (respostas falsas dos comandos) pelos que o Rust dá das mesmas linhas; a tela grava
/// a resposta inteira. Fora `pontos_sha256` (o vitest inventa) e a mídia (de mentira), têm de ser iguais.
fn retratos_do_rust(doc: &mut Value, cal: &RetratoCalibracao, calc: &RetratoCalculo) -> usize {
    let (cal, calc) = (serde_json::to_value(cal).unwrap(), serde_json::to_value(calc).unwrap());
    let sem = |v: &Value, fora: &[&str]| {
        let mut v = v.clone();
        for k in fora {
            v.as_object_mut().unwrap().remove(*k);
        }
        v
    };
    let mut n = 0;
    for c in doc["principal"].as_array_mut().unwrap() {
        for e in c["efeitos"].as_array_mut().unwrap() {
            let (campo, rust, fora): (&str, &Value, &[&str]) = match e["tipo"].as_str().unwrap() {
                "regua" => ("calibracao", &cal, &["pontos_sha256", "midia_sha256"]),
                "velocidade_exibida" => ("calculo", &calc, &["midia_sha256"]),
                _ => continue,
            };
            let mut d = Vec::new();
            diferencas(&sem(&e[campo], fora), &sem(rust, fora), campo, &mut d);
            assert_eq!(d, Vec::<String>::new(), "o que o vitest imitou × o que o Rust devolve");
            e[campo] = rust.clone();
            n += 1;
        }
    }
    n
}

/// Uma ferramenta de fora sobre o PDF; None = ausente (com `SICRO_EXIGE_FFMPEG=1`, falha: nada medido).
fn por_fora(prog: &str, args: &[&str], pdf: &Path) -> Option<(bool, String)> {
    match std::process::Command::new(prog).args(args).arg(pdf).output() {
        Ok(o) => Some((o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))),
        Err(e) => {
            assert!(std::env::var_os("SICRO_EXIGE_FFMPEG").is_none(), "{prog} ausente: {e}");
            eprintln!("dispensado: sem {prog}");
            None
        }
    }
}

/// O PDF do pacote aberto por fora: estrutura (`qpdf --check`), toda fonte embutida (`pdffonts`), metadados e marcação
/// (`pdfinfo`), PDF/A-3a + PDF/UA-1 declarados no XMP (`pdfinfo -meta`) e o projeto anexado (`pdfdetach`). A
/// conformidade de fato ao PDF/A só o veraPDF diz: aqui é a declaração.
fn conferir_pdf_por_fora(pdf: &Path) {
    if let Some((ok, s)) = por_fora("qpdf", &["--check"], pdf) {
        assert!(ok && s.contains("No syntax or stream encoding errors found"), "qpdf --check: {s}");
    }
    if let Some((ok, s)) = por_fora("pdffonts", &[], pdf) {
        assert!(ok, "{s}");
        let fontes: Vec<&str> = s.lines().skip(2).filter(|l| !l.trim().is_empty()).collect();
        assert!(!fontes.is_empty(), "PDF sem fonte: {s}");
        for l in &fontes {
            let t: Vec<&str> = l.split_whitespace().collect();
            assert!(t.len() >= 7 && t[t.len() - 5] == "yes", "fonte não embutida: {l}");
        }
    }
    if let Some((ok, s)) = por_fora("pdfinfo", &[], pdf) {
        assert!(ok, "{s}");
        let campo = |k: &str| s.lines().find_map(|l| l.strip_prefix(k).map(|v| v.trim().to_string())).unwrap_or_default();
        assert!(campo("Creator:").starts_with("SICRO "), "{s}");
        assert!(!campo("Title:").is_empty(), "PDF sem título: {s}");
        assert_eq!(campo("Tagged:"), "yes", "PDF/UA pede marcação: {s}");
        assert!(campo("Pages:").parse::<u32>().unwrap_or(0) >= 6, "{s}");
    }
    if let Some((ok, s)) = por_fora("pdfinfo", &["-meta"], pdf) {
        assert!(ok, "{s}");
        let liso: String = s.split_whitespace().collect();
        for marca in ["<pdfaid:part>3</pdfaid:part>", "<pdfaid:conformance>A</pdfaid:conformance>", "<pdfuaid:part>1</pdfuaid:part>"] {
            assert!(liso.contains(marca), "XMP sem {marca}: {s}");
        }
    }
    if let Some((ok, s)) = por_fora("pdfdetach", &["-list"], pdf) {
        assert!(ok && !s.trim_start().starts_with("0 "), "PDF/A-3 sem anexo: {s}");
    }
}

/// O projeto da Fase 1 do store num caso de verdade: retratos e degravação ligados às mídias do caso, nenhum erro na
/// análise, os três alvos compilam, e a exportação do júri com pacote, originais e legendas nas duas formas vai até a
/// conferência; o pacote confere pelas somas e o PDF abre por fora com o que o front gravou (proveniência, revisor,
/// sobrepostos e legendas).
#[test]
fn projeto_da_fase1_do_store_valida_compila_e_exporta_com_pacote() {
    let Some(cap) = fontes::cap() else { return };
    let ver = cap.versao.clone();
    let c = caso();
    let (mut v, tab, [v1, _]) = montar_de(&cap, &c, doc1());
    let (cal, calc) = analise_no_banco(&c, &v1.sha256);
    assert_eq!(retratos_do_rust(&mut v, &cal, &calc), 2);
    // degravação da câmera 1 (a origem que a tela grava ao trazer as falas)
    v["legendas"]["origem"]["audio_sha256"] = json!(v1.sha256);
    armazem::salvar(&c.ws, &v).expect("edicao_salvar");
    let id = v["id"].as_str().unwrap().to_string();
    let aberto = cmd::abrir(&c.ws, &id, &c.app, &fila::projeto_ocupado).expect("edicao_abrir");
    assert!(!aberto.so_leitura && aberto.avisos.is_empty(), "{:?}", aberto.avisos);
    for cl in aberto.projeto["principal"].as_array().unwrap() {
        let par = |t: &str, p: &str| (cl[t].as_f64(), cl[p].as_i64());
        for (t, pts) in [par("inicio_s", "inicio_pts"), par("fim_s", "fim_pts"), par("instante_s", "instante_pts")] {
            if let (Some(t), Some(pts)) = (t, pts) {
                assert_eq!(tab.quadro_em(AlvoQuadro::T { t }).unwrap().pts, pts, "{}", cl["id"]);
            }
        }
    }
    let primeira = cmd::analisar(&c.ws, &aberto.projeto, &c.app).unwrap();
    let presos: Vec<_> = primeira.prender.iter().filter(|p| !matches!(p.onde, OndePrender::Ancora { .. })).collect();
    assert!(presos.is_empty(), "o front prendeu tudo o que grava com pts: {presos:?}");

    let (doc, analise) = analisar_como_a_tela(&c, aberto.projeto);
    for p in &analise.problemas {
        eprintln!("problema ({ver}): {:?} {} {:?} {:?} {:?}", p.nivel, p.codigo, p.clipe, p.efeito, p.campo);
    }
    let erros: Vec<_> = analise.problemas.iter().filter(|p| p.nivel == Nivel::Erro).collect();
    assert!(erros.is_empty(), "{ver}: {erros:?}");
    assert!(!analise.problemas.iter().any(|p| p.codigo == crate::edicao::mensagens::codigo::RETRATO_DESATUALIZADO), "{ver}: retratos do banco = os gravados");
    let total = analise.linha.total_q;
    // título 30 + 60 + 60 − 15 (fundir de 0,5 s) + cartela 30 + 15 + congelado 30 + 15 + imagem 30 + encerramento 30
    assert_eq!(total, 285, "{ver}");

    let p = armazem::tipado(&doc).unwrap();
    let cache = cmd::cache_do_caso(&c.app, &c.ws).unwrap();
    let prep = executar::preparar(&c.ws, &p, &cache, true).unwrap();
    let alvos = [Alvo::Final { exportacao: p.saida.exportacao.clone() }, Alvo::Previa { altura: 180, de_q: 0, ate_q: total }, Alvo::Quadro { q: 100, largura: 320 }];
    for alvo in &alvos {
        let plano = motor::compilar(&p, &prep.contexto(), alvo).unwrap_or_else(|e| panic!("{ver}: compilar {alvo:?}: {e}"));
        assert!(!plano.blocos.is_empty(), "{alvo:?}");
        if matches!(alvo, Alvo::Final { .. }) {
            assert_eq!(plano.total_q, total, "{ver}: o fecho não muda o total");
        }
    }
    drop(prep);

    armazem::salvar(&c.ws, &doc).unwrap();
    let mut ciencias: Vec<String> = analise.problemas.iter().filter(|p| p.nivel == Nivel::Pericial).map(|p| p.codigo.to_string()).collect();
    ciencias.dedup();
    for alvo in alvos {
        let tarefa = fila::enfileirar(cmd::pedido(&c.ws, doc.clone(), alvo.clone(), ciencias.clone(), &c.app).unwrap()).unwrap();
        match (&alvo, esperar(&tarefa)) {
            (Alvo::Final { .. }, Estado::AConferir { saida_id }) => {
                // `ocultar` na câmera 2: para em "A conferir"; o pacote sai na conferência
                let s = cmd::conferir(&c.ws, &saida_id, true).expect("edicao_conferir");
                assert_eq!((s.n_quadros, s.exige_conferencia), (total, true), "{ver}");
                let pasta = paths::resolver_relativo(&c.ws, &s.pasta);
                let listados = pacote::ponta::conferir_somas(&pasta);
                let nome = Path::new(&s.arquivo).file_stem().unwrap().to_string_lossy().into_owned();
                let base = nome.strip_suffix(" (ocultado)").expect("com `ocultar`, o nome diz").to_string();
                for esperado in [format!("{base} (ocultado).mp4"), format!("{base}.tecnico.json"), format!("{base}.srt"), format!("{base}.vtt"), "mapa.csv".into(), "audio.flac".into(), "projeto/projeto.json".into()] {
                    assert!(listados.contains(&esperado), "{ver}: falta {esperado} em {listados:?}");
                }
                // as câmeras em `originais/` e a mídia própria (imagem, narração, música) em `projeto/`, pelo SHA-256 que o
                // projeto cita
                let sha_em = |prefixo: &str| -> Vec<String> {
                    listados.iter().filter(|a| a.starts_with(prefixo)).map(|a| crate::hashing::sha256::sha256_file(&pasta.join(a)).unwrap()).collect()
                };
                let (originais, proprias) = (sha_em("originais/"), sha_em("projeto/midia/"));
                for m in doc["midias"].as_array().unwrap() {
                    let onde = if m["origem"] == "caso" { &originais } else { &proprias };
                    assert!(onde.contains(&m["sha256"].as_str().unwrap().to_string()), "{ver}: mídia {} fora do pacote", m["id"]);
                }
                assert_eq!((originais.len(), proprias.len()), (2, 3), "{ver}");
                // a "Cor" escolhida no Inspetor (O4) e a proveniência da análise (P1.10) chegam ao `feito`
                let tec: Value = serde_json::from_str(&std::fs::read_to_string(pasta.join(format!("{base}.tecnico.json"))).unwrap()).unwrap();
                let midia = |id: &str| tec["feito"]["midias"].as_array().unwrap().iter().find(|m| m["id"] == id).cloned().unwrap_or_else(|| panic!("{ver}: {id} fora do feito"));
                assert_eq!(midia("m2")["cor"], json!({ "matriz": "bt709", "faixa": "tv" }), "{ver}");
                let m1 = midia("m1");
                assert_eq!((&m1["recebido"], &m1["relogio_desvio_s"], &m1["gravador"]), (&json!({ "nativo": true, "obs": "Pendrive do condomínio" }), &json!(-2.5), &json!("DVR Intelbras")), "{ver}");
                assert_eq!(tec["feito"]["revisado_por"], json!("Dra. Lia"), "{ver}");
                let srt = std::fs::read_to_string(pasta.join(format!("{base}.srt"))).unwrap();
                for fala in ["O portão está fechado", "Certo", "Mãos para cima!", "Abertura do exame"] {
                    assert!(srt.contains(fala), "{ver}: {fala:?} fora do .srt: {srt}");
                }
                let pdf = pasta.join(pacote::PDF);
                // conferência à mão (veraPDF, leitor de tela): SICRO_GUARDAR_PDF=<pasta>
                if let Some(d) = std::env::var_os("SICRO_GUARDAR_PDF") {
                    std::fs::copy(&pdf, Path::new(&d).join(format!("verif-fase1-{ver}.pdf"))).unwrap();
                }
                conferir_pdf_por_fora(&pdf);
                let Some(texto) = pacote::ponta::texto_do_pdf(&pdf) else {
                    assert!(std::env::var_os("SICRO_EXIGE_FFMPEG").is_none(), "pdftotext ausente");
                    continue;
                };
                for (o_que, trecho) in [
                    ("revisor", "Dra. Lia"),
                    ("proveniência: como chegou", "Pendrive do condomínio"),
                    ("proveniência: gravador", "DVR Intelbras"),
                    ("terço inferior", "Rua das Flores, 120"),
                    ("título preso", "Chegada do veículo"),
                    ("legendas", "Legendas (4)"),
                    ("conferência", "Perito Contratos"),
                ] {
                    assert!(texto.contains(&trecho.split_whitespace().collect::<String>()), "{ver}: PDF sem {o_que} ({trecho:?})");
                }
                assert!(texto.contains(&s.sha256), "{ver}: SHA-256 do vídeo fora do PDF");
            }
            (Alvo::Previa { .. } | Alvo::Quadro { .. }, Estado::Pronta { relativo }) => assert!(cache.join(&relativo).is_file(), "{relativo}"),
            (a, e) => panic!("{a:?} terminou em {e:?} ({ver})"),
        }
    }
}

/// Regressão do defeito da Fase 0 (contratos (h) 23, (i) 1): a ocultação criada no quadro congelado vai para a mídia
/// com `ativo` = [t, t] no instante do quadro e a validação a aceita (intervalo fechado).
#[test]
fn ocultar_do_congelado_valida() {
    let Some(cap) = fontes::cap() else { return };
    let c = caso();
    let (v, _) = montar(&cap, &c);
    armazem::salvar(&c.ws, &v).unwrap();
    let (doc, a) = analisar_como_a_tela(&c, v);
    assert!(doc["midias"].as_array().unwrap().iter().flat_map(|m| m["ocultacoes"].as_array().unwrap()).any(|o| o["ativo"]["de"].is_number() && o["ativo"]["de"] == o["ativo"]["ate"]), "a fixture tem a ocultação [t, t]");
    let erros: Vec<_> = a.problemas.iter().filter(|p| erro_do_instante(&doc, p)).collect();
    assert!(erros.is_empty(), "ocultação do congelado com ativo [t, t] recusada: {erros:?}");
}

/// Regressão do defeito da Fase 0 ((i) 2): a âncora padrão de "Adicionar" nasce no quadro mostrado, com pts, e a
/// marcação no mesmo quadro a substitui; do quadro seguinte em diante o rastreio segura a caixa marcada.
#[test]
fn ancora_marcada_vale_depois_do_quadro_em_que_foi_marcada() {
    let p = armazem::tipado(&doc()).unwrap();
    // marcados uma vez só, no quadro 60 (colchetes e ocultação da mídia do trecho 1)
    let marcados: Vec<_> = p
        .rastreios
        .iter()
        .filter(|r| r.ancoras.first().is_some_and(|k| k.pts == Some(60 * 512) && k.v != CAIXA_CENTRO) && r.ancoras.iter().filter(|k| k.pts.is_some()).count() == 1)
        .collect();
    assert_eq!(marcados.len(), 2, "rastreios marcados no quadro 60");
    for r in marcados {
        let marcada = r.ancoras[0].v;
        // quadro 61: depois da única marcação, o rastreio manual segura a caixa marcada
        let k = 61i64;
        let a = avaliar::avaliar(r, None, Instante { t: (512 * k) as f64 / 15360.0, pts: Some(512 * k) }).expect("caixa");
        assert_eq!(a.caixa, marcada, "rastreio {} segura {:?} (âncora padrão) no lugar de {:?} (marcada); âncoras {:?}", r.id, a.caixa, marcada, r.ancoras);
    }
}

/// Regressão do defeito da Fase 0 ((i) 7): a falha `ciencia: <códigos>` da exportação passa pelo catálogo sem perder
/// os códigos que faltam.
#[test]
fn falha_de_ciencia_diz_quais_avisos_faltam() {
    let e = crate::edicao::mensagens::erro_de_comando(crate::error::SicroError::Validation("ciencia: fonte_danificada, taxa_variavel".into()));
    let texto = e.to_string();
    assert!(texto.contains("fonte_danificada") && texto.contains("taxa_variavel"), "{texto}");
}

/// Regressão (§5.7): na predefinição WhatsApp o PDF diz que o código de verificação só confere no envio como documento.
#[test]
fn relatorio_do_whatsapp_diz_que_o_hash_so_vale_como_documento() {
    use crate::edicao::testes::aceite::trecho;
    let Some(caso) = pacote::ponta::caso("whatsapp") else { panic!("DEFEITO não medido: sem ffmpeg") };
    let (m, tab) = caso.midia("m1", &fontes::gerar(Fonte::Mp4).unwrap());
    let doc = pacote::ponta::projeto("Zap", true, vec![m], vec![trecho("c1", "m1", &tab, 30, 75, 1.0, json!([]))], json!({}));
    let (fim, _) = caso.exportar(&doc, pacote::ponta::exportacao("whatsapp", "cpu", json!({ "originais": false }), "a3"));
    let s = caso.saida(&pacote::ponta::id_da(&fim));
    assert_eq!(serde_json::to_value(&s.predefinicao).unwrap(), json!("whatsapp"));
    let pdf = caso.pasta(&s).join(pacote::PDF);
    let texto = pacote::ponta::texto_do_pdf(&pdf).unwrap_or_else(|| panic!("DEFEITO não medido: sem pdftotext"));
    assert!(texto.contains("WhatsApp"), "o PDF diz a predefinição");
    assert!(texto.contains("comodocumento"), "DEFEITO: PDF da predefinição WhatsApp sem a ressalva do envio como documento");
}

/// Regressão: cota de disco esgotada (EDQUOT, pasta de rede com cota) é disco cheio.
#[test]
fn cota_de_disco_esgotada_e_disco_cheio() {
    use crate::edicao::ffmpeg::exec::{classificar, ClasseFalha};
    let linha = "[vf#0:0 @ 0x55a398628e80] Task finished with error code: -122 (Disk quota exceeded)".to_string();
    assert_eq!(classificar(&[linha]), ClasseFalha::DiscoCheio, "DEFEITO: cota esgotada não é disco cheio");
}

/// Regressão: caso sem número não deixa título nem cabeçalho do PDF pendurados em "—" ou "·".
#[test]
fn titulo_do_pdf_de_caso_sem_numero_nao_fica_pendurado() {
    use crate::edicao::testes::aceite::trecho;
    let Some(_) = fontes::cap() else { panic!("DEFEITO não medido: sem ffmpeg") };
    let c = caso();
    let pc = pacote::ponta::Caso { ws: c.ws.clone(), app: c.app.clone(), cap: crate::edicao::ffmpeg::sonda::capacidades().unwrap() };
    let (m, tab) = pc.midia("m1", &fontes::gerar(Fonte::Mp4).unwrap());
    let doc = pacote::ponta::projeto("Sem número", true, vec![m], vec![trecho("c1", "m1", &tab, 30, 60, 1.0, json!([]))], json!({}));
    let (fim, _) = pc.exportar(&doc, pacote::ponta::exportacao("juri", "cpu", json!({ "originais": false }), "a3"));
    let s = pc.saida(&pacote::ponta::id_da(&fim));
    let Some((ok, info)) = por_fora("pdfinfo", &[], &pc.pasta(&s).join(pacote::PDF)) else { panic!("DEFEITO não medido: sem pdfinfo") };
    assert!(ok, "{info}");
    let titulo = info.lines().find_map(|l| l.strip_prefix("Title:")).map(str::trim).unwrap_or_default().to_string();
    assert!(titulo.starts_with("Relatório da edição demonstrativa de vídeo"), "{info}");
    assert!(!titulo.ends_with('—'), "DEFEITO: título do PDF pendurado: {titulo:?}");
}
