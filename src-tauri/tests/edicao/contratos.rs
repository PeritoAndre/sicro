//! Verificação da Fase 0, lente de contratos: o projeto que o FRONT grava (fixture `verif-fase0-do-store.json`,
//! gerada pelo store real em `src/modules/edicao/store/fase0DoStore.test.ts`, com todo recurso da Fase 0) é o que
//! o Rust lê: tipado sem item desconhecido, nenhum campo perdido na ida e volta, pts gravados pelo front iguais aos
//! que o Rust prende, validação sem erro e compilação e render nos três alvos, nos dois binários.
//! Incluído por `edicao/mod.rs` (`#[path]`), como a ponta a ponta: usa `pub(crate)` e a matriz de `edicao::testes`.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde_json::{json, Value};

use crate::commands::edicao_commands as cmd;
use crate::edicao::armazem;
use crate::edicao::executar;
use crate::edicao::fila::{self, Estado};
use crate::edicao::modelo::{Caixa, ClipePrincipal, ClipeSobreposto, Efeito, Nivel, OndePrender, ProjetoEdicao, TipoMidia, Tolerante};
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
    let mut v = doc();
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
    for m in v["midias"].as_array_mut().unwrap() {
        let (vm, propria) = match m["id"].as_str().unwrap() {
            "m1" => (Some(&v1), None),
            "m2" => (Some(&v2), None),
            "m3" => (None, Some(&img)),
            "m4" => (None, Some(&som)),
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
    (v, t1)
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
        outro => panic!("prender {outro:?} sem estabilizar na Fase 0"),
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

    let (mut doc, mut analise) = analisar_como_a_tela(&c, aberto.projeto);
    for p in &analise.problemas {
        eprintln!("problema ({}): {:?} {} {:?} {:?} {:?}", cap.versao, p.nivel, p.codigo, p.clipe, p.efeito, p.campo);
    }
    let (instante, outros): (Vec<_>, Vec<_>) = analise.problemas.iter().filter(|p| p.nivel == Nivel::Erro).partition(|p| erro_do_instante(&doc, p));
    assert!(outros.is_empty(), "{outros:?}");
    if !instante.is_empty() {
        // DEFEITO aberto (teste `ocultar_do_congelado_valida`): até o validar.rs aceitar [t, t], a cópia daqui usa
        // [t, t + meio quadro], que cobre os mesmos quadros; corrigido, este ramo não roda
        let ids: Vec<String> = instante.iter().filter_map(|p| p.efeito.clone()).collect();
        for o in doc["midias"].as_array_mut().unwrap().iter_mut().flat_map(|m| m["ocultacoes"].as_array_mut().unwrap().iter_mut()) {
            if ids.iter().any(|i| o["id"] == i.as_str()) {
                o["ativo"]["ate"] = json!(o["ativo"]["de"].as_f64().unwrap() + 1.0 / 60.0);
            }
        }
        analise = cmd::analisar(&c.ws, &doc, &c.app).unwrap();
        let erros: Vec<_> = analise.problemas.iter().filter(|p| p.nivel == Nivel::Erro).collect();
        assert!(erros.is_empty(), "{erros:?}");
    }
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

/// DEFEITO entre pacotes (contratos (h) 23 × validar.rs:945 e :1271): a ocultação criada no quadro congelado vai
/// para a mídia com `ativo` = [t, t] no instante do quadro (intervalo fechado, (h) 3), e a validação recusa
/// `ate <= de` com `intervalo` ("Trecho inválido"): a exportação fica barrada. `should_panic` enquanto o defeito
/// existir; corrigido o validar.rs (`ate < de`), tirar o `should_panic` e o ramo de contorno do teste acima.
#[test]
fn ocultar_do_congelado_valida() {
    let Some(cap) = fontes::cap() else { panic!("sem ffmpeg: nada medido") };
    let c = caso();
    let (v, _) = montar(&cap, &c);
    armazem::salvar(&c.ws, &v).unwrap();
    let (doc, a) = analisar_como_a_tela(&c, v);
    let erros: Vec<_> = a.problemas.iter().filter(|p| erro_do_instante(&doc, p)).collect();
    assert!(erros.is_empty(), "DEFEITO: ocultação do congelado com ativo [t, t] recusada: {erros:?}");
}

/// DEFEITO entre pacotes (inspetor/acoes.ts × palco): "Adicionar" põe a âncora padrão no tempo do CURSOR (meio do
/// quadro, sem pts); "Marcar" no mesmo quadro grava o par do palco (início do quadro, com pts) e `porChave` não
/// troca uma pela outra. Ficam duas âncoras no mesmo quadro e, do quadro seguinte em diante, o rastreio segura a
/// caixa padrão do meio da imagem: destaque e ocultação fora do lugar marcado. `should_panic` enquanto o defeito
/// existir; corrigido o acoes.ts e regravada a fixture, tirar o `should_panic`.
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
        assert_eq!(a.caixa, marcada, "DEFEITO: rastreio {} segura {:?} (âncora padrão) no lugar de {:?} (marcada); âncoras {:?}", r.id, a.caixa, marcada, r.ancoras);
    }
}

/// DEFEITO entre pacotes (fila.rs:580 × mensagens.rs:168 × ponta_a_ponta.rs:259): a falha `ciencia: <códigos>` da
/// exportação passa pelo catálogo, que não guarda o detalhe de `ciencia`; os códigos que faltam somem da mensagem e
/// quem pede de novo com a ciência (o ajudante da ponta a ponta, que lê os códigos dela) não tem o que ler. Medido:
/// `amostras_de_gravador_exportam` (ignorado) falha no `test.lvf` com "ciencia: Aviso sem ciência".
#[test]
fn falha_de_ciencia_diz_quais_avisos_faltam() {
    let e = crate::edicao::mensagens::erro_de_comando(crate::error::SicroError::Validation("ciencia: fonte_danificada, taxa_variavel".into()));
    let texto = e.to_string();
    assert!(texto.contains("fonte_danificada") && texto.contains("taxa_variavel"), "DEFEITO: {texto}");
}
