//! Ponta a ponta da Edição (fecho da Fase 0): caso temporário com banco e migrações, mídias da matriz registradas
//! como o app registra (`register_video_media`), projeto vindo do store (`fixtures/edicao/projeto-do-store.json`, com
//! as `MidiaRef` trocadas pelas fontes do caso) e as MESMAS funções que os comandos chamam: salvar, abrir, analisar,
//! enfileirar, esperar a fila, conferir. Nos dois binários (`SICRO_FFMPEG`); `SICRO_EXIGE_FFMPEG=1` faz falhar sem
//! ffmpeg. Incluído por `edicao/mod.rs` (`#[path]`): usa o que é `pub(crate)` e os medidores de `edicao::testes`.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::commands::edicao_commands as cmd;
use crate::edicao::executar::{self, abrir_banco, sha256_bytes};
use crate::edicao::ffmpeg::sonda::{self, Capacidades};
use crate::edicao::fila::{self, Estado, EstadoSaida, PapelRetrato, SaidaRegistrada};
use crate::edicao::modelo::{Analise, Nivel, Racional, TipoMidia};
use crate::edicao::motor::plano::Plano;
use crate::edicao::motor::{self, Alvo};
use crate::edicao::quadros::TabelaQuadros;
use crate::edicao::testes::fontes::{self, Fonte};
use crate::edicao::testes::medir::{self, QuadroRgb};
use crate::edicao::{armazem, conferir, mensagens};
use crate::models::VideoMedia;
use crate::workspace::paths;

const PERITO: &str = "Perito Ponta";
/// Canvas dos projetos de teste (o do store é 1920×1080; a fonte é 320×180).
const CANVAS: (u32, u32) = (640, 360);
/// Caixas `[x, y, w, h]` (0..1 da fonte) dos rastreios do store, postas onde a régua de pixel lê sem ambiguidade:
/// ocultação da mídia na faixa "alto" (branca), destaque no meio, borrão do clipe na borda entre "baixo" e "alto".
const OCULTA_MIDIA: [f64; 4] = [0.20, 0.06, 0.10, 0.14];
const DESTAQUE: [f64; 4] = [0.55, 0.30, 0.12, 0.20];
const BORRAO: [f64; 4] = [0.12, 0.05, 0.10, 0.15];
/// Trechos (índices da tabela do decodificador `[j_in, j_out)`) dos dois clipes do store; a ocultação da mídia começa
/// em `OCULTA_DESDE` (meio do clipe 1: o slot anterior fica descoberto).
const TRECHO_1: (usize, usize) = (30, 135);
const OCULTA_DESDE: usize = 60;
const TRECHO_2: (usize, usize) = (60, 360);
/// Limite que a arquitetura fixa para a saída de GPU decodificada contra a de CPU (§5.6: PSNR mínimo de 30 dB).
const PSNR_MINIMO_DB: f64 = 30.0;
/// Vizinho que difere do quadro j menos que isso abaixo do PSNR da saída não distingue (cena parada de gravador).
const MARGEM_VIZINHO_DB: f64 = 3.0;

// ------------------------------------------------------------------ caso, fontes e projeto

struct Caso {
    _d: tempfile::TempDir,
    ws: PathBuf,
    /// `app_cache_dir()` do teste.
    app: PathBuf,
}

impl Caso {
    fn novo(nome: &str) -> Self {
        let d = tempfile::Builder::new().prefix(&format!("p0f-{nome}-")).tempdir().expect("pasta temporária");
        let pai = d.path().join("Casos ç");
        std::fs::create_dir_all(&pai).unwrap();
        let criado = crate::workspace::create::create_workspace(
            crate::models::NewOccurrenceInput {
                titulo: Some("Ponta a ponta".into()),
                numero_bo: None,
                protocolo: Some("77/26".into()),
                oficio: None,
                tipo_pericia: None,
                municipio: None,
                peritos: vec![PERITO.into()],
                parent_directory: Some(pai.to_string_lossy().into()),
            },
            &pai,
        )
        .unwrap();
        let app = d.path().join("cache do app");
        Caso { _d: d, ws: criado.path, app }
    }

    fn cache(&self) -> PathBuf {
        cmd::cache_do_caso(&self.app, &self.ws).unwrap()
    }

    fn contar(&self, sql: &str) -> i64 {
        let (c, _) = abrir_banco(&self.ws).unwrap();
        c.query_row(sql, [], |r| r.get(0)).unwrap()
    }
}

/// Vídeo do caso: registro, a cópia no caso, a tabela do decodificador e o número desenhado em cada quadro.
struct Fonte1 {
    vm: VideoMedia,
    caminho: PathBuf,
    tab: TabelaQuadros,
    numeros: Vec<u32>,
}

fn registrar(cap: &Capacidades, ws: &Path, f: &Path) -> Fonte1 {
    let (vm, caminho, tab) = registrar_e_ler(cap, ws, f).expect("vídeo do caso");
    let numeros = medir::quadros_de_origem(cap, &caminho).expect("régua");
    assert_eq!(numeros.len(), tab.pts.len(), "{}", cap.versao);
    Fonte1 { vm, caminho, tab, numeros }
}

/// Registro como o app faz e a tabela do decodificador da cópia no caso.
fn registrar_e_ler(cap: &Capacidades, ws: &Path, f: &Path) -> crate::error::Result<(VideoMedia, PathBuf, TabelaQuadros)> {
    let vm = tauri::async_runtime::block_on(crate::commands::video_commands::register_video_media(
        ws.to_string_lossy().into(),
        crate::models::RegisterVideoInput { source_path: f.to_string_lossy().into(), allow_duplicate: false },
    ))?;
    let caminho = paths::resolver_relativo(ws, &vm.relative_path);
    let tab = TabelaQuadros::do_decodificador(cap, &caminho, &AtomicBool::new(false), &mut |_| {})?;
    Ok((vm, caminho, tab))
}

fn midia_ref(id: &str, f: &Fonte1, ocultacoes: Value) -> Value {
    json!({ "id": id, "tipo": "video", "origem": "caso", "media_id": f.vm.id.to_string(), "sha256": f.vm.sha256,
            "nome": f.vm.filename, "relativo": f.vm.relative_path, "duracao_s": f.vm.duration_s, "inicio_s": null,
            "largura": f.vm.width, "altura": f.vm.height, "taxa": null, "vfr": false, "tem_audio": true, "cor": null,
            "derivacao": null, "recebido": { "nativo": null, "obs": "" }, "relogio_desvio_s": null, "gravador": null,
            "decodificacao": null, "ocultacoes": ocultacoes })
}

fn caixa(c: [f64; 4]) -> Value {
    json!({ "x": c[0], "y": c[1], "w": c[2], "h": c[3] })
}

/// Clipe da principal pelo id.
fn clipe_mut<'a>(p: &'a mut Value, id: &str) -> &'a mut Value {
    p["principal"].as_array_mut().unwrap().iter_mut().find(|c| c["id"] == id).expect("clipe do store")
}

fn rastreio_mut<'a>(p: &'a mut Value, id: &str) -> &'a mut Value {
    p["rastreios"].as_array_mut().unwrap().iter_mut().find(|c| c["id"] == id).expect("rastreio do store")
}

fn efeito_mut<'a>(clipe: &'a mut Value, tipo: &str) -> &'a mut Value {
    clipe["efeitos"].as_array_mut().unwrap().iter_mut().find(|e| e["tipo"] == tipo).expect("efeito do store")
}

fn trecho_em(clipe: &mut Value, f: &Fonte1, (j_in, j_out): (usize, usize)) {
    clipe["inicio_s"] = json!(f.tab.tempo(j_in));
    clipe["fim_s"] = json!(f.tab.tempo(j_out));
    clipe["inicio_pts"] = json!(f.tab.pts[j_in]);
    clipe["fim_pts"] = json!(f.tab.pts[j_out]);
}

/// As fontes e o caso do projeto do store.
struct Montagem {
    caso: Caso,
    m1: Fonte1,
    m2: Fonte1,
}

fn montagem(nome: &str, cap: &Capacidades) -> Montagem {
    let caso = Caso::novo(nome);
    let m1 = registrar(cap, &caso.ws, &fontes::gerar(Fonte::Mp4).unwrap());
    let m2 = registrar(cap, &caso.ws, &fontes::gerar(Fonte::Ts5994).unwrap());
    Montagem { caso, m1, m2 }
}

/// O documento do store com as fontes do caso: id e título próprios, canvas menor, clipes nos trechos de teste, os
/// rastreios nas caixas da régua, a ocultação da mídia do meio do clipe 1 em diante e a narração importada pelo
/// comando (`edicao_midia_importar`). `ocultar` false = sem nenhuma ocultação (o cache dos blocos fica).
fn projeto_do_store(mt: &Montagem, cap: &Capacidades, ocultar: bool) -> Value {
    let mut p: Value = serde_json::from_str(include_str!("../fixtures/edicao/projeto-do-store.json")).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    p["id"] = json!(id);
    p["titulo"] = json!(if ocultar { "Ponta a ponta" } else { "Ponta a ponta sem ocultar" });
    p["saida"]["largura"] = json!(CANVAS.0);
    p["saida"]["altura"] = json!(CANVAS.1);
    let (t1, t2) = (&mt.m1.tab, &mt.m2.tab);
    // mídias do caso
    let mut oc = p["midias"][0]["ocultacoes"].clone();
    oc[0]["ativo"] = json!({ "de": t1.tempo(OCULTA_DESDE), "ate": t1.fim_s() });
    p["midias"][0] = midia_ref("m1", &mt.m1, if ocultar { oc } else { json!([]) });
    p["midias"][1] = midia_ref("m2", &mt.m2, json!([]));
    // narração: arquivo próprio do projeto, pelo comando de importar
    let wav = fontes::pasta_temporaria("narracao").join("narração.wav");
    gerar_wav(cap, &wav);
    let narra = armazem::importar_midia(&mt.caso.ws, &id, &wav, TipoMidia::Audio).expect("importar narração");
    let mut narra = serde_json::to_value(&narra).unwrap();
    narra["id"] = json!("m3");
    p["midias"][2] = narra;
    // clipe 1: o girar fica; o enquadramento do store vira a imagem inteira (a régua lê as faixas)
    let c1 = clipe_mut(&mut p, "00000000-0000-4000-8000-000000000002");
    trecho_em(c1, &mt.m1, TRECHO_1);
    efeito_mut(c1, "enquadramento")["regiao"] = caixa([0.0, 0.0, 1.0, 1.0]);
    // clipe 2: o zoom animado começa 1 s depois da entrada (a régua lê os primeiros slots depois do escurecer)
    let c2 = clipe_mut(&mut p, "00000000-0000-4000-8000-000000000003");
    trecho_em(c2, &mt.m2, TRECHO_2);
    let t0 = t2.tempo(TRECHO_2.0);
    efeito_mut(c2, "enquadramento")["regiao"] = json!([
        { "t": t0, "v": caixa([0.0, 0.0, 1.0, 1.0]) },
        { "t": t0 + 1.0, "v": caixa([0.0, 0.0, 1.0, 1.0]) },
        { "t": t0 + 4.5, "v": caixa([0.4, 0.3, 0.4, 0.4]) }
    ]);
    if ocultar {
        efeito_mut(c2, "ocultar")["ativo"] = json!({ "de": t0, "ate": t2.tempo(TRECHO_2.1) });
    } else {
        c2["efeitos"].as_array_mut().unwrap().retain(|e| e["tipo"] != "ocultar");
    }
    // rastreios: uma âncora no início do trecho, presa ao quadro
    let ancora = |t: &TabelaQuadros, j: usize, c: [f64; 4]| json!([{ "t": t.tempo(j), "pts": t.pts[j], "v": caixa(c) }]);
    rastreio_mut(&mut p, "00000000-0000-4000-8000-000000000007")["ancoras"] = ancora(t1, TRECHO_1.0, DESTAQUE);
    rastreio_mut(&mut p, "00000000-0000-4000-8000-000000000010")["ancoras"] = ancora(t1, TRECHO_1.0, OCULTA_MIDIA);
    rastreio_mut(&mut p, "00000000-0000-4000-8000-000000000012")["ancoras"] = ancora(t2, TRECHO_2.0, BORRAO);
    p
}

/// 2 s de seno de 1 kHz a 48 kHz, PCM.
fn gerar_wav(cap: &Capacidades, destino: &Path) {
    let st = crate::tools::command(&cap.binario)
        .args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=1000:sample_rate=48000:duration=2", "-c:a", "pcm_s16le"])
        .arg(destino)
        .status()
        .unwrap();
    assert!(st.success());
}

// ------------------------------------------------------------------ o caminho dos comandos

/// Configura a sonda uma vez no processo (cache próprio do teste, que vive até o fim do processo) e refaz a de GPU:
/// é o `iniciar` do app, sem o `AppHandle`. As exportações pegam `sonda::capacidades()`, como no app.
fn sonda_do_app() -> Arc<Capacidades> {
    static FEITA: OnceLock<()> = OnceLock::new();
    FEITA.get_or_init(|| {
        sonda::configurar(&fontes::pasta_temporaria("sonda-do-app"));
        let c = sonda::testar_de_novo().expect("sonda");
        eprintln!("sonda do app: {} gpu {:?}", c.versao, c.gpu.as_ref().map(|g| &g.codificador));
    });
    sonda::capacidades().unwrap()
}

/// O que a tela faz para exportar, pelas funções dos comandos: `edicao_salvar` → `edicao_abrir` → `edicao_analisar`
/// → `edicao_enfileirar` (ciência dos avisos periciais) → espera da fila. Devolve o documento aberto, a análise e o
/// estado final da tarefa.
fn exportar(c: &Caso, projeto: &Value, codificador: &str) -> (Value, Analise, Estado) {
    armazem::salvar(&c.ws, projeto).expect("edicao_salvar");
    let id = projeto["id"].as_str().unwrap();
    let aberto = cmd::abrir(&c.ws, id, &c.app, &fila::projeto_ocupado).expect("edicao_abrir");
    assert!(!aberto.so_leitura, "{:?}", aberto.avisos);
    assert_eq!(Path::new(&aberto.raiz_cache), c.cache());
    let doc = aberto.projeto;
    let (analise, fim) = exportar_aberto(c, &doc, codificador);
    (doc, analise, fim)
}

/// Exportar de novo o documento já aberto (sem mudança, a tela não grava): análise, ciência e fila. Aviso pericial que
/// só aparece com a tabela do decodificador (fonte danificada) volta como `ciencia: <códigos>`: o perito dá ciência e
/// a tela pede de novo, uma vez.
fn exportar_aberto(c: &Caso, doc: &Value, codificador: &str) -> (Analise, Estado) {
    let analise = cmd::analisar(&c.ws, doc, &c.app).expect("edicao_analisar");
    let mut ciencias: Vec<String> = analise.problemas.iter().filter(|p| p.nivel == Nivel::Pericial).map(|p| p.codigo.to_string()).collect();
    let mut exportacao = doc["saida"]["exportacao"].clone();
    exportacao["codificador"] = json!(codificador);
    let alvo: Alvo = serde_json::from_value(json!({ "tipo": "final", "exportacao": exportacao })).unwrap();
    for vez in 0..2 {
        let tarefa = fila::enfileirar(cmd::pedido(&c.ws, doc.clone(), alvo.clone(), ciencias.clone(), &c.app).unwrap()).expect("edicao_enfileirar");
        match fila::esperar(&tarefa).expect("esperar") {
            Estado::Falhou { mensagem, .. } if vez == 0 && mensagem.starts_with("ciencia: ") => {
                ciencias.extend(mensagem["ciencia: ".len()..].split(", ").map(String::from));
            }
            fim => return (analise, fim),
        }
    }
    unreachable!()
}

fn alvo_de(codificador: &str) -> Alvo {
    serde_json::from_value(json!({ "tipo": "final", "exportacao": {
        "predefinicao": "juri", "codificador": codificador, "legendas": "nenhuma", "pacote": null, "pdf": "a3", "revisado_por": "", "personalizada": null } }))
    .unwrap()
}

/// O plano que o motor prevê para o documento (mesma `preparar` e mesmo `compilar` da fila).
fn plano_de(c: &Caso, doc: &Value, codificador: &str) -> Plano {
    let p = armazem::tipado(doc).unwrap();
    let prep = executar::preparar_com(&c.ws, &p, &c.cache(), true, Some(&AtomicBool::new(false))).unwrap();
    motor::compilar(&p, &prep.contexto(), &alvo_de(codificador)).expect("compilar")
}

fn saida_unica(c: &Caso, id: &str) -> SaidaRegistrada {
    let v = fila::saidas(&c.ws, Some(id)).unwrap();
    assert_eq!(v.len(), 1, "{v:?}");
    v.into_iter().next().unwrap()
}

fn ler_json(p: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).unwrap()
}

/// `(q inicial, n_q, quadro_do_slot)` do item `n` do roteiro.
fn mapa_do_item(plano: &Plano, n: usize) -> (u64, u32, Vec<u32>) {
    let it = &plano.proveniencia.roteiro[n];
    let o = it.origem.as_ref().map(|o| o.quadro_do_slot.clone()).unwrap_or_default();
    (it.na_saida.de_q, it.na_saida.n_q, o)
}

/// Slot do §3.1 de um pts do trecho a 1×, contado direto dos pts da tabela em inteiros (sem o mapa do motor):
/// teto((P − P_jin)·fps − E), E = meio slot ou meio passo mínimo do trecho, o menor. Em frações de 1/(2·B) de slot.
fn slot_pela_tabela(tab: &TabelaQuadros, (j_in, j_out): (usize, usize), fps: Racional) -> impl Fn(i64) -> i128 {
    let (a, b) = (tab.tb.num as i128 * fps.num as i128, tab.tb.den as i128 * fps.den as i128);
    let ultimo = j_out.min(tab.pts.len() - 1);
    let pmin = tab.pts[j_in..=ultimo].windows(2).map(|w| (w[1] - w[0]) as i128).filter(|d| *d > 0).min().expect("trecho com passo");
    let (e2, p0) = (b.min(pmin * a), tab.pts[j_in] as i128);
    move |pts| -(-(2 * (pts as i128 - p0) * a - e2)).div_euclid(2 * b)
}

/// Quadro que o slot `k` do trecho mostra: maior j com slot(j) ≤ k. A previsão do plano é conferida contra esta:
/// sem isso um mapa deslocado desloca a régua junto e nada acusa.
fn quadro_pela_tabela(tab: &TabelaQuadros, trecho: (usize, usize), k: u32, fps: Racional) -> u32 {
    let slot = slot_pela_tabela(tab, trecho, fps);
    (trecho.0..trecho.1).rev().find(|&j| slot(tab.pts[j]) <= k as i128).expect("slot dentro do trecho") as u32
}

/// Slots do trecho (o fim é o pts de `j_out`, que existe nos trechos de teste).
fn slots_pela_tabela(tab: &TabelaQuadros, trecho: (usize, usize), fps: Racional) -> u32 {
    slot_pela_tabela(tab, trecho, fps)(tab.pts[trecho.1]).max(1) as u32
}

// ------------------------------------------------------------------ réguas

fn psnr(a: &QuadroRgb, b: &QuadroRgb) -> f64 {
    assert_eq!((a.w, a.h), (b.w, b.h));
    let mse = a.px.iter().zip(&b.px).map(|(x, y)| (*x as f64 - *y as f64).powi(2)).sum::<f64>() / a.px.len() as f64;
    if mse == 0.0 { f64::INFINITY } else { 10.0 * (255.0f64 * 255.0 / mse).log10() }
}

fn px(q: &QuadroRgb, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * q.w + x) * 3) as usize;
    [q.px[i], q.px[i + 1], q.px[i + 2]]
}

/// Largura da imagem da fonte 16:9 girada 90° e posta em `conter` no canvas.
fn largura_girada() -> f64 {
    CANVAS.1 as f64 * 180.0 / 320.0
}

/// Ponto `(x, y)` da fonte (0..1) no canvas do clipe 1: giro horário (`x' = 1 − y`, `y' = x`) e barras dos lados.
fn no_canvas_girado(x: f64, y: f64) -> (f64, f64) {
    let w = largura_girada();
    ((CANVAS.0 as f64 - w) / 2.0 + (1.0 - y) * w, x * CANVAS.1 as f64)
}

/// Número desenhado na fonte girada do clipe 1: a coluna da fonte é a linha do canvas; lê-se a faixa da imagem
/// (sem as barras, 3 px para dentro), transposta, pela régua de `medir`.
fn numero_girado(q: &QuadroRgb) -> u32 {
    let w = largura_girada();
    let x0 = ((CANVAS.0 as f64 - w) / 2.0).ceil() as u32 + 3;
    let x1 = ((CANVAS.0 as f64 + w) / 2.0).floor() as u32 - 3;
    let (y0, y1) = (3, q.h - 3);
    let (lw, lh) = ((y1 - y0) as usize, (x1 - x0) as usize);
    let mut luma = vec![0u8; lw * lh];
    for (r, x) in (x0..x1).enumerate() {
        for (k, y) in (y0..y1).enumerate() {
            let p = px(q, x, y);
            luma[r * lw + k] = ((p[0] as u32 + p[1] as u32 + p[2] as u32) / 3) as u8;
        }
    }
    medir::identificar(&luma, lw, lh)
}

/// Média RGB da caixa `[x0, y0, x1, y1]` em px.
fn media(q: &QuadroRgb, c: [f64; 4]) -> [f64; 3] {
    let [x0, y0, x1, y1] = c;
    medir::cor_media(q, [x0.ceil() as u32, y0.ceil() as u32, (x1 - x0).floor() as u32, (y1 - y0).floor() as u32])
}

/// Maior salto de vermelho entre pixels vizinhos na linha `y`, de `x0` a `x1`.
fn salto_max(q: &QuadroRgb, y: u32, x0: u32, x1: u32) -> u8 {
    (x0..x1).map(|x| px(q, x, y)[0].abs_diff(px(q, x + 1, y)[0])).max().unwrap_or(0)
}

fn eh_amarelo(p: [u8; 3]) -> bool {
    p[0] > 170 && p[1] > 140 && p[2] < 140 && p[0] as i32 - p[2] as i32 > 70
}

/// Caixa dos pixels amarelos na região `[x0, y0, x1, y1]` (px).
fn caixa_amarela(q: &QuadroRgb, r: [u32; 4]) -> Option<[u32; 4]> {
    let mut c: Option<[u32; 4]> = None;
    for y in r[1]..r[3].min(q.h) {
        for x in r[0]..r[2].min(q.w) {
            if eh_amarelo(px(q, x, y)) {
                c = Some(match c {
                    None => [x, y, x, y],
                    Some([a, b, cx, d]) => [a.min(x), b.min(y), cx.max(x), d.max(y)],
                });
            }
        }
    }
    c
}

/// ffmpeg vivo com `cwd` dentro de `pasta` (só os deste caso).
#[cfg(target_os = "linux")]
fn ffmpeg_vivos_em(pasta: &Path) -> usize {
    let alvo = std::fs::canonicalize(pasta).unwrap_or_else(|_| pasta.to_path_buf());
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
        .filter(|e| std::fs::read_link(e.path().join("cwd")).is_ok_and(|c| c.starts_with(&alvo)))
        .count()
}

// ------------------------------------------------------------------ os testes

/// O projeto do store, com ocultação da mídia e do clipe, exportado pela fila em CPU (júri) até conferido:
/// MP4 somente leitura que toca com `total_q` quadros; quadro de origem previsto em 3 slots de cada clipe; destaque,
/// texto e as duas ocultações no lugar; amostras de áudio; `tecnico.json` com hashes que conferem; retrato
/// autossuficiente; banco, log por mídia, auditoria e Integridade. A segunda exportação dá o mesmo arquivo e o mesmo
/// `feito`, e não reaproveita bloco (a ocultação limpa o cache do projeto). Excluir o projeto preserva o retrato;
/// excluir a saída apaga tudo dela.
#[test]
fn projeto_do_store_exporta_e_confere() {
    let Some(cap) = fontes::cap() else { return };
    let cap_app = sonda_do_app();
    assert_eq!(cap_app.sha256, cap.sha256, "o ffmpeg do app é o dos testes");
    let v = cap.versao.clone();
    let mt = montagem("store", &cap);
    let projeto = projeto_do_store(&mt, &cap, true);
    let c = &mt.caso;
    let id = projeto["id"].as_str().unwrap().to_string();

    let (doc, analise, fim) = exportar(c, &projeto, "cpu");
    let erros: Vec<_> = analise.problemas.iter().filter(|p| p.nivel == Nivel::Erro).collect();
    assert!(erros.iter().all(|p| p.codigo == mensagens::codigo::TABELA_PENDENTE), "{v}: {erros:?}");
    let Estado::AConferir { saida_id } = fim else { panic!("{v}: esperava a conferir, veio {fim:?}") };
    let plano = plano_de(c, &doc, "cpu");
    let total_q = plano.total_q;
    assert_eq!(total_q, analise.linha.total_q);

    // a conferir: banco e pendente no retrato; nada na pasta da saída até conferir
    let s = saida_unica(c, &id);
    assert_eq!((s.id.as_str(), s.estado, s.exige_conferencia, s.tecnico_rel.clone()), (saida_id.as_str(), EstadoSaida::AConferir, true, None));
    let retrato = c.ws.join(&s.retrato_rel);
    assert!(retrato.join(fila::PENDENTE).is_file());
    let s = fila::conferir_saida(&c.ws, &saida_id, true, &cmd::perito(&c.ws)).expect("edicao_conferir");
    assert_eq!((s.estado, s.conferida_por.as_deref()), (EstadoSaida::Concluida, Some(PERITO)));
    assert!(!retrato.join(fila::PENDENTE).exists());

    // o MP4: existe, somente leitura, "(ocultado)" no nome, toca com total_q quadros
    let mp4 = c.ws.join(&s.arquivo);
    assert!(s.arquivo.ends_with("Ponta a ponta (ocultado).mp4"), "{}", s.arquivo);
    assert!(std::fs::metadata(&mp4).unwrap().permissions().readonly());
    assert_eq!(crate::hashing::sha256::sha256_file(&mp4).unwrap(), s.sha256);
    let decodificados = medir::framehash(&cap, &mp4).unwrap();
    assert_eq!(decodificados.len() as u64, total_q, "{v}");
    assert_eq!((s.n_quadros, s.decodificado_sha256.clone()), (total_q, Some(conferir::hash_de_conteudo(&decodificados))));

    // quadro de origem por slot = previsto, em 3 slots de cada clipe (o 1 girado, longe do escurecer que entra no fim
    // dele; o 2 depois do escurecer e antes do zoom)
    let (q1, n1, mapa1) = mapa_do_item(&plano, 1);
    let (q2, n2, mapa2) = mapa_do_item(&plano, 2);
    assert_eq!((n1 as usize, n2 as usize), (mapa1.len(), mapa2.len()));
    assert_eq!(mapa1[0] as usize, TRECHO_1.0);
    // a previsão do plano não se confere com ela mesma: o quadro de cada slot medido sai dos pts da tabela (§3.1)
    let fps = plano.canvas.fps;
    assert_eq!((n1, n2), (slots_pela_tabela(&mt.m1.tab, TRECHO_1, fps), slots_pela_tabela(&mt.m2.tab, TRECHO_2, fps)), "{v}");
    let lidos_saida = medir::quadros_de_origem(&cap, &mp4).unwrap();
    for k in [0u32, n1 / 2, n1 - 20] {
        let j = quadro_pela_tabela(&mt.m1.tab, TRECHO_1, k, fps);
        assert_eq!(mapa1[k as usize], j, "{v}: clipe 1, slot {k}: previsão do plano");
        let q = medir::quadro(&cap, &mp4, (q1 + k as u64) as u32).unwrap();
        assert_eq!(numero_girado(&q), mt.m1.numeros[j as usize], "{v}: clipe 1, slot {k}");
    }
    for k in [15u32, 22, 29] {
        let j = quadro_pela_tabela(&mt.m2.tab, TRECHO_2, k, fps);
        assert_eq!(mapa2[k as usize], j, "{v}: clipe 2, slot {k}: previsão do plano");
        let q = (q2 + k as u64) as usize;
        assert_eq!(lidos_saida[q], mt.m2.numeros[j as usize], "{v}: clipe 2, slot {k}");
    }

    // destaque (amarelo) em volta do rastreio, girado com a imagem
    let k = 10u32;
    let q = medir::quadro(&cap, &mp4, (q1 + k as u64) as u32).unwrap();
    let (a, b) = (no_canvas_girado(DESTAQUE[0], DESTAQUE[1] + DESTAQUE[3]), no_canvas_girado(DESTAQUE[0] + DESTAQUE[2], DESTAQUE[1]));
    let esperado = [a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)];
    let achado = caixa_amarela(&q, [0, 0, CANVAS.0, CANVAS.1]).unwrap_or_else(|| panic!("{v}: sem destaque"));
    for i in 0..4 {
        assert!((achado[i] as f64 - esperado[i]).abs() <= 5.0, "{v}: destaque {achado:?}, esperado {esperado:?}");
    }
    // ocultação da mídia (sólido preto sobre a faixa branca): fora do `ativo` a faixa aparece; dentro, preto
    let (a, b) = (no_canvas_girado(OCULTA_MIDIA[0], OCULTA_MIDIA[1] + OCULTA_MIDIA[3]), no_canvas_girado(OCULTA_MIDIA[0] + OCULTA_MIDIA[2], OCULTA_MIDIA[1]));
    let dentro = [a.0.min(b.0) + 3.0, a.1.min(b.1) + 3.0, a.0.max(b.0) - 3.0, a.1.max(b.1) - 3.0];
    let desde = (OCULTA_DESDE - TRECHO_1.0) as u64;
    let antes = medir::quadro(&cap, &mp4, (q1 + desde - 1) as u32).unwrap();
    assert!(media(&antes, dentro).iter().all(|c| *c > 200.0), "{v}: slot antes do ativo: {:?}", media(&antes, dentro));
    for k in [desde, desde + 20, n1 as u64 - 1] {
        let q = medir::quadro(&cap, &mp4, (q1 + k) as u32).unwrap();
        assert!(media(&q, dentro).iter().all(|c| *c < 12.0), "{v}: slot {k} sem a ocultação: {:?}", media(&q, dentro));
    }
    // clipe 2: borrão do clipe (a borda baixo/alto some dentro da caixa, segue nítida fora) e o texto embaixo
    let k = 22u32;
    let q = medir::quadro(&cap, &mp4, (q2 + k as u64) as u32).unwrap();
    let (bx0, bx1) = ((BORRAO[0] * CANVAS.0 as f64) as u32, ((BORRAO[0] + BORRAO[2]) * CANVAS.0 as f64) as u32);
    let meio_borrao = ((BORRAO[1] + BORRAO[3] / 2.0) * CANVAS.1 as f64) as u32;
    let (dentro_b, fora_b) = (salto_max(&q, meio_borrao, bx0 + 4, bx1 - 4), salto_max(&q, (0.45 * CANVAS.1 as f64) as u32, bx0 + 4, bx1 - 4));
    assert!(fora_b > 100 && (dentro_b as u32) * 3 < fora_b as u32, "{v}: borda dentro {dentro_b}, fora {fora_b}");
    let j = mt.m2.numeros[mapa2[k as usize] as usize];
    assert!(j / 100 % 10 != 9 && j / 10 % 10 != 9, "faixa do texto branca no quadro {j}");
    let brancos = |q: &QuadroRgb| {
        let (x0, x1) = ((0.36 * q.w as f64) as u32, (0.64 * q.w as f64) as u32);
        let (y0, y1) = ((0.86 * q.h as f64) as u32, (0.96 * q.h as f64) as u32);
        (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| px(q, x, y).iter().all(|c| *c >= 245)).count()
    };
    assert!(brancos(&q) > 80, "{v}: texto ausente ({} px)", brancos(&q));
    let sem_texto = medir::quadro(&cap, &mt.m2.caminho, mapa2[k as usize]).unwrap();
    assert!(brancos(&sem_texto) < 5, "a fonte já tinha branco ali");

    // áudio: o passo de áudio exige o PCM com o número exato de amostras (a(total_q)); o AAC do MP4 arredonda ao quadro
    let esperadas = (total_q as u128 * 48_000 * plano.canvas.fps.den as u128 / plano.canvas.fps.num as u128) as u64;
    assert_eq!(plano.audio.as_ref().map(|a| a.amostras), Some(esperadas));
    let no_mp4 = medir::amostras(&cap, &mp4).unwrap();
    assert!(no_mp4.abs_diff(esperadas) <= 1024, "{v}: {no_mp4} amostras de {esperadas}");

    // tecnico.json: esquema, feito × execução, hashes que conferem com os arquivos
    let tecnico = ler_json(&c.ws.join(s.tecnico_rel.as_ref().unwrap()));
    assert!(std::fs::metadata(c.ws.join(s.tecnico_rel.as_ref().unwrap())).unwrap().permissions().readonly());
    assert_eq!(tecnico["esquema"], "sicro.edicao.tecnico/2");
    let ex = &tecnico["execucao"];
    assert_eq!((ex["saida"]["sha256"].as_str(), ex["saida"]["arquivo"].as_str()), (Some(s.sha256.as_str()), Some(s.arquivo.as_str())));
    assert_eq!(ex["conteudo"]["decodificado_sha256"].as_str(), s.decodificado_sha256.as_deref());
    assert_eq!(ex["conteudo"]["composto_sha256"].as_str(), Some(s.composto_sha256.as_str()));
    let compostos = std::fs::read_to_string(retrato.join(fila::COMPOSTOS)).unwrap();
    assert_eq!(compostos.lines().count() as u64, total_q);
    assert_eq!(format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(compostos.as_bytes())), s.composto_sha256);
    assert_eq!(ex["conferencia"]["ocultacoes"], "conferida");
    assert_eq!((ex["conferencia"]["por"].as_str(), ex["conferencia"]["quadros"].as_u64()), (Some(PERITO), Some(total_q)));
    let blocos_plano: Vec<&str> = plano.blocos.iter().map(|b| b.chave.as_str()).collect();
    let blocos_exec: Vec<&str> = ex["blocos"].as_array().unwrap().iter().map(|b| b["chave"].as_str().unwrap()).collect();
    assert_eq!(blocos_exec, blocos_plano, "o plano recompilado é o da exportação");
    let feito = &tecnico["feito"];
    assert_eq!(feito["projeto"]["sha256"].as_str(), Some(sha256_bytes(&std::fs::read(retrato.join("projeto.json")).unwrap()).as_str()));
    assert_eq!(feito["saida"]["n_quadros"].as_u64(), Some(total_q));
    assert_eq!(feito["ocultacoes"].as_array().map(Vec::len), Some(2), "mídia e clipe");
    assert!(!serde_json::to_string(feito).unwrap().contains(&c.ws.to_string_lossy().to_string()), "caminho absoluto no feito");

    // banco, log por mídia usada, auditoria e Integridade
    let (conn, occ) = abrir_banco(&c.ws).unwrap();
    for m in [&mt.m1, &mt.m2] {
        let logs = crate::database::repositories::video_repo::list_logs_for_media(&conn, &occ, &m.vm.sha256, 50).unwrap();
        let usos: Vec<Value> = logs.iter().filter(|l| l.action == "edicao.uso").map(|l| serde_json::from_str(&l.details_json).unwrap()).collect();
        assert_eq!(usos.len(), 1, "{usos:?}");
        assert_eq!(usos[0]["saida_id"], json!(saida_id));
    }
    assert_eq!(c.contar("SELECT COUNT(*) FROM audit_logs WHERE action = 'video.edicao.render'"), 1);
    assert_eq!(c.contar("SELECT COUNT(*) FROM audit_logs WHERE action = 'video.edicao.conferida'"), 1);
    let itens = crate::registry::aggregator::build_registry(&conn, &occ).unwrap();
    let item = itens.iter().find(|i| i.id == format!("video_edit:{saida_id}")).expect("saída na Integridade");
    assert_eq!((item.relative_path.as_deref(), item.hash_sha256.as_deref()), (Some(s.arquivo.as_str()), Some(s.sha256.as_str())));
    assert_eq!(crate::registry::integrity::verify_one(&c.ws, item, true).0, crate::models::IntegrityStatus::Ok);
    drop(conn);

    // a mesma exportação de novo: mesmo arquivo e mesmo `feito`; a ocultação limpou o cache do projeto (nada reaproveitado)
    let (_, fim2) = exportar_aberto(c, &doc, "cpu");
    let Estado::AConferir { saida_id: s2 } = fim2 else { panic!("{v}: {fim2:?}") };
    let s2 = fila::conferir_saida(&c.ws, &s2, true, PERITO).unwrap();
    assert_ne!(s2.pasta, s.pasta);
    assert_eq!((&s2.sha256, &s2.composto_sha256), (&s.sha256, &s.composto_sha256), "{v}: mesma classe de máquina, mesmo arquivo");
    let tecnico2 = ler_json(&c.ws.join(s2.tecnico_rel.as_ref().unwrap()));
    assert_eq!(tecnico2["feito"], tecnico["feito"], "feito determinístico");
    assert!(tecnico2["execucao"]["blocos"].as_array().unwrap().iter().all(|b| b["reaproveitado"] == false));

    // excluir o projeto: fica só `exportacoes/`, o retrato confere sozinho
    cmd::excluir(&c.ws, &id, &c.app, &fila::projeto_ocupado).expect("edicao_excluir");
    let pasta = armazem::pasta_projeto(&c.ws, &id);
    let sobrou: Vec<String> = std::fs::read_dir(&pasta).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(sobrou, ["exportacoes"]);
    for sid in [&s.id, &s2.id] {
        let (v_doc, m) = fila::ler_retrato(&c.ws, &id, sid).expect("retrato autossuficiente");
        assert_eq!(v_doc["id"], json!(id));
        let r = pasta.join("exportacoes").join(sid);
        for it in &m.itens {
            assert_eq!(crate::hashing::sha256::sha256_file(&r.join(&it.arquivo)).unwrap(), it.sha256, "{}", it.arquivo);
        }
        assert!(m.itens.iter().any(|i| i.papel == PapelRetrato::Midia && i.de.starts_with("midia/")), "narração no retrato");
        assert!(m.itens.iter().any(|i| i.papel == PapelRetrato::Compostos));
        assert!(m.itens.iter().any(|i| i.papel == PapelRetrato::Grafo));
    }
    // excluir as saídas: a pasta de cada uma, o retrato, a linha; a última leva a pasta do projeto
    fila::excluir_saida(&c.ws, &s.id).unwrap();
    assert!(!c.ws.join(&s.pasta).exists() && !pasta.join("exportacoes").join(&s.id).exists() && pasta.is_dir());
    fila::excluir_saida(&c.ws, &s2.id).unwrap();
    assert!(!c.ws.join(&s2.pasta).exists() && !pasta.exists());
    assert!(fila::saidas(&c.ws, None).unwrap().is_empty());
    assert_eq!(c.contar("SELECT COUNT(*) FROM audit_logs WHERE action = 'video.edicao.saida_excluida'"), 2);
    #[cfg(target_os = "linux")]
    assert_eq!(ffmpeg_vivos_em(&c.cache()), 0);
}

/// Sem ocultação: repetir a exportação reaproveita todos os blocos e dá o mesmo SHA-256 em CPU; o `mapa.csv` do plano
/// com o resultado é o dos pts lidos (os da tabela do decodificador); com `auto` nesta máquina vai NVENC (quando a
/// sonda aprova) com o mesmo composto da CPU e o decodificado a pelo menos 30 dB do de CPU.
#[test]
fn repetir_reaproveita_mapa_e_auto() {
    let Some(cap) = fontes::cap() else { return };
    let cap_app = sonda_do_app();
    let v = cap.versao.clone();
    let mt = montagem("repetir", &cap);
    let projeto = projeto_do_store(&mt, &cap, false);
    let c = &mt.caso;
    let id = projeto["id"].as_str().unwrap().to_string();

    let (doc, _, fim) = exportar(c, &projeto, "cpu");
    let Estado::Concluida { saida_id: a } = fim else { panic!("{v}: {fim:?}") };
    let (_, fim) = exportar_aberto(c, &doc, "cpu");
    let Estado::Concluida { saida_id: b } = fim else { panic!("{v}: {fim:?}") };
    let saidas = fila::saidas(&c.ws, Some(&id)).unwrap();
    let (sa, sb) = (saidas.iter().find(|s| s.id == a).unwrap(), saidas.iter().find(|s| s.id == b).unwrap());
    assert_eq!((sa.estado, sa.exige_conferencia, sa.conferida_por.clone()), (EstadoSaida::Concluida, false, None));
    assert!(!sa.arquivo.contains("(ocultado)"));
    assert_eq!((&sa.sha256, &sa.composto_sha256, &sa.decodificado_sha256), (&sb.sha256, &sb.composto_sha256, &sb.decodificado_sha256), "{v}");
    let tb = ler_json(&c.ws.join(sb.tecnico_rel.as_ref().unwrap()));
    let blocos = tb["execucao"]["blocos"].as_array().unwrap();
    assert!(!blocos.is_empty() && blocos.iter().all(|b| b["reaproveitado"] == true), "{v}: {blocos:?}");
    let ta = ler_json(&c.ws.join(sa.tecnico_rel.as_ref().unwrap()));
    assert_eq!(ta["feito"], tb["feito"]);

    // mapa.csv: pelo executor real com o mesmo plano (todos os blocos do cache), pts lidos = os da tabela
    let plano = plano_de(c, &doc, "cpu");
    let cache = c.cache();
    let p = armazem::tipado(&doc).unwrap();
    let prep = executar::preparar(&c.ws, &p, &cache, false).unwrap();
    let amb = executar::Ambiente {
        cap: prep.cap.clone(),
        trabalho: cache.join("render").join("teste-mapa"),
        cache: cache.clone(),
        caso: c.ws.clone(),
        pasta_projeto: prep.pasta_projeto.clone(),
        paralelos: 1,
        identidade: Default::default(),
        ciencias: vec![],
    };
    let res = executar::executar(&plano, &amb, &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert!(res.blocos.iter().all(|b| b.reaproveitado));
    assert_eq!(res.sha256, sa.sha256);
    let csv = crate::edicao::relatorio::mapa::mapa_csv(&plano, Some(&res)).expect("mapa.csv com os lidos");
    let tabelas = [("m1", &mt.m1), ("m2", &mt.m2)];
    // lidos de cada clipe = os de slot novo mais os descartados (59,94 → 30 descarta a metade), todos na tabela
    let mut por_clipe: std::collections::HashMap<String, Vec<i64>> = Default::default();
    let mut slots_com_fonte = 0;
    for l in csv.lines().skip(1) {
        let c: Vec<&str> = l.split(',').collect();
        if c[3].is_empty() {
            continue;
        }
        let (midia, pts, j): (&str, i64, usize) = (c[3], c[4].parse().unwrap(), c[5].parse().unwrap());
        let f = tabelas.iter().find(|(m, _)| *m == midia).unwrap().1;
        assert_eq!(pts, f.tab.pts[j], "{v}: {l}");
        if c[7] != "repetido" {
            por_clipe.entry(c[2].to_string()).or_default().push(pts);
        }
        slots_com_fonte += !c[0].is_empty() as u32;
    }
    assert_eq!(slots_com_fonte, plano.proveniencia.roteiro.iter().filter(|r| r.origem.is_some()).map(|r| r.na_saida.n_q).sum::<u32>());
    assert_eq!(res.lidos.len(), 2);
    for l in &res.lidos {
        let mut do_mapa = por_clipe.remove(&l.clipe).unwrap_or_default();
        do_mapa.sort();
        assert_eq!(do_mapa, l.pts, "{v}: pts lidos do clipe {}", l.clipe);
    }

    // auto: NVENC aprovado → composto igual ao da CPU e decodificado a ≥ 30 dB; sem GPU (7.0.2) é a CPU
    let (_, fim) = exportar_aberto(c, &doc, "auto");
    let Estado::Concluida { saida_id: g } = fim else { panic!("{v}: {fim:?}") };
    let sg = fila::saidas(&c.ws, Some(&id)).unwrap().into_iter().find(|s| s.id == g).unwrap();
    assert_eq!(sg.composto_sha256, sa.composto_sha256, "{v}: composto da GPU");
    match &cap_app.gpu {
        Some(gpu) if gpu.codificador == "h264_nvenc" => {
            assert_eq!(sg.codificador, "h264_nvenc", "{v}");
            let (mg, mc) = (c.ws.join(&sg.arquivo), c.ws.join(&sa.arquivo));
            assert_eq!(medir::framehash(&cap, &mg).unwrap().len() as u64, sg.n_quadros);
            for q in [0u32, (sg.n_quadros / 2) as u32, (sg.n_quadros - 1) as u32] {
                let d = psnr(&medir::quadro(&cap, &mg, q).unwrap(), &medir::quadro(&cap, &mc, q).unwrap());
                assert!(d >= PSNR_MINIMO_DB, "{v}: quadro {q} da GPU a {d:.1} dB do da CPU");
            }
            eprintln!("auto ({v}): h264_nvenc, composto igual ao da CPU");
        }
        outro => {
            assert_eq!((sg.codificador.as_str(), &sg.sha256), ("libx264", &sa.sha256), "{v}: sem GPU aprovada ({outro:?})");
            eprintln!("auto ({v}): sem NVENC aprovado, libx264");
        }
    }
}

/// Cancelar no meio dos blocos: a tarefa fica `Cancelada` e não deixa saída, linha no banco, bloco truncado no cache
/// nem processo.
#[test]
fn cancelar_no_meio_nao_deixa_nada() {
    let Some(cap) = fontes::cap() else { return };
    sonda_do_app();
    let v = cap.versao.clone();
    let mt = montagem("cancelar", &cap);
    let mut projeto = projeto_do_store(&mt, &cap, false);
    // 1080p: os blocos levam o bastante para o pedido cair no meio
    projeto["saida"]["largura"] = json!(1920);
    projeto["saida"]["altura"] = json!(1080);
    let c = &mt.caso;
    armazem::salvar(&c.ws, &projeto).unwrap();
    let doc = cmd::abrir(&c.ws, projeto["id"].as_str().unwrap(), &c.app, &fila::projeto_ocupado).unwrap().projeto;
    let analise = cmd::analisar(&c.ws, &doc, &c.app).unwrap();
    let ciencias: Vec<String> = analise.problemas.iter().filter(|p| p.nivel == Nivel::Pericial).map(|p| p.codigo.to_string()).collect();
    let tarefa = fila::enfileirar(cmd::pedido(&c.ws, doc, alvo_de("cpu"), ciencias, &c.app).unwrap()).unwrap();
    let t0 = Instant::now();
    loop {
        let e = fila::listar().into_iter().find(|t| t.id == tarefa).unwrap().estado;
        match e {
            Estado::Rodando { fase: fila::Fase::Blocos { .. }, feito_q, .. } if feito_q > 0 => break,
            e if e.acabou() => panic!("{v}: terminou antes do cancelamento: {e:?}"),
            _ => {}
        }
        assert!(t0.elapsed() < Duration::from_secs(600), "{v}: os blocos não começaram");
        std::thread::sleep(Duration::from_millis(20));
    }
    let pedido = Instant::now();
    fila::cancelar(&tarefa).unwrap();
    assert_eq!(fila::esperar(&tarefa).unwrap(), Estado::Cancelada);
    eprintln!("cancelar ({v}): {:.3} s", pedido.elapsed().as_secs_f64());
    assert!(fila::saidas(&c.ws, None).unwrap().is_empty());
    assert!(std::fs::read_dir(paths::dir_edicao_saida(&c.ws)).map(|r| r.count()).unwrap_or(0) == 0, "pasta de saída");
    let cache = c.cache();
    let trabalho = cache.join("render").join(&tarefa);
    let sobras: Vec<String> = std::fs::read_dir(&trabalho).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert!(sobras.iter().all(|n| n == "projeto.json" || n == executar::CHAVES), "{sobras:?}");
    for e in std::fs::read_dir(cache.join("blocos")).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        assert!(!n.contains(".tmp"), "temporário no cache: {n}");
        if let Some(chave) = n.strip_suffix(".mp4") {
            let fh = std::fs::read_to_string(cache.join("blocos").join(format!("{chave}.framehash"))).unwrap();
            assert_eq!(conferir::coluna_de_hash(&fh).len(), medir::framehash(&cap, &e.path()).unwrap().len(), "bloco truncado {n}");
        }
    }
    #[cfg(target_os = "linux")]
    assert_eq!(ffmpeg_vivos_em(&cache), 0, "processo sobrando");
}

/// Projeto da v1 (`v1-completo.json`, com as mídias e os tempos trocados pelos do caso) importado pelo `edicao_abrir`
/// exporta e confere a contagem.
#[test]
fn projeto_importado_da_v1_exporta() {
    let Some(cap) = fontes::cap() else { return };
    sonda_do_app();
    let v = cap.versao.clone();
    let c = Caso::novo("v1");
    let a = registrar(&cap, &c.ws, &fontes::gerar(Fonte::Mp4).unwrap());
    let b = registrar(&cap, &c.ws, &fontes::gerar(Fonte::Mkv30).unwrap());
    let d = registrar(&cap, &c.ws, &fontes::gerar(Fonte::Ts5994).unwrap());
    let mut v1: Value = serde_json::from_str(include_str!("../fixtures/edicao/v1-completo.json")).unwrap();
    let ids = [
        ("11111111-2222-4333-8444-555555555551", &a),
        ("11111111-2222-4333-8444-555555555552", &b),
        ("11111111-2222-4333-8444-555555555553", &d),
    ];
    let mut texto = v1.to_string();
    for (velho, f) in ids {
        texto = texto.replace(velho, &f.vm.id.to_string());
    }
    v1 = serde_json::from_str(&texto).unwrap();
    // tempos da v1 são tempo de fonte (§2.4, "iguais"): dentro dos 15 s das fontes, e o TS começa em 3600 s
    let t5 = (d.tab.tempo(0), d.tab.tempo(60));
    for (i, (ini, fim)) in [(0usize, (2.0, 7.0)), (3, (10.0, 11.0)), (4, t5)] {
        v1["itens"][i]["inicio_s"] = json!(ini);
        v1["itens"][i]["fim_s"] = json!(fim);
    }
    v1["saida"]["largura"] = json!(CANVAS.0);
    v1["saida"]["altura"] = json!(CANVAS.1);
    let pid = v1["id"].as_str().unwrap().to_string();
    let montagens = paths::dir_montagens_json(&c.ws);
    std::fs::create_dir_all(montagens.join("audios")).unwrap();
    std::fs::write(montagens.join(format!("{pid}.json")), serde_json::to_vec(&v1).unwrap()).unwrap();
    // as narrações da v1 existem no caso
    for n in ["a1.webm", "a2.wav", "sumiu.ogg"] {
        let st = crate::tools::command(&cap.binario)
            .args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=1"])
            .arg(montagens.join("audios").join(n))
            .status()
            .unwrap();
        assert!(st.success(), "{n}");
    }
    let aberto = cmd::abrir(&c.ws, &pid, &c.app, &fila::projeto_ocupado).expect("edicao_abrir importa");
    assert_eq!(aberto.importado_de, Some(1));
    let analise = cmd::analisar(&c.ws, &aberto.projeto, &c.app).unwrap();
    let ciencias: Vec<String> = analise.problemas.iter().filter(|p| p.nivel == Nivel::Pericial).map(|p| p.codigo.to_string()).collect();
    let tarefa = fila::enfileirar(cmd::pedido(&c.ws, aberto.projeto.clone(), alvo_de("cpu"), ciencias, &c.app).unwrap()).unwrap();
    let fim = fila::esperar(&tarefa).unwrap();
    let saida_id = match fim {
        Estado::Concluida { saida_id } => saida_id,
        Estado::AConferir { saida_id } => fila::conferir_saida(&c.ws, &saida_id, true, PERITO).unwrap().id,
        outro => panic!("{v}: projeto da v1 não exportou: {outro:?}; análise: {:?}", analise.problemas),
    };
    let s = saida_unica(&c, &pid);
    assert_eq!(s.id, saida_id);
    let mp4 = c.ws.join(&s.arquivo);
    assert_eq!(medir::framehash(&cap, &mp4).unwrap().len() as u64, s.n_quadros, "{v}");
    assert_eq!(s.n_quadros, analise.linha.total_q);
}

/// Item desconhecido ligado (versão futura) NÃO exporta: a análise acusa com o id e a fila falha com o código.
#[test]
fn item_desconhecido_nao_exporta_e_diz_por_que() {
    let Some(cap) = fontes::cap() else { return };
    sonda_do_app();
    let mt = montagem("desconhecido", &cap);
    let mut projeto = projeto_do_store(&mt, &cap, false);
    let futuro = "00000000-0000-4000-8000-0000000000ff";
    projeto["principal"].as_array_mut().unwrap().push(json!({ "tipo": "tela_3d", "id": futuro, "desligado": false, "camadas": 3 }));
    let c = &mt.caso;
    let (_, analise, fim) = exportar(c, &projeto, "cpu");
    let p = analise.problemas.iter().find(|p| p.codigo == mensagens::codigo::ITEM_DESCONHECIDO).expect("acusado na análise");
    assert_eq!((p.nivel, p.clipe.as_deref()), (Nivel::Erro, Some(futuro)));
    let Estado::Falhou { mensagem, log_rel } = fim else { panic!("exportou: {fim:?}") };
    assert!(mensagem.starts_with(mensagens::codigo::ITEM_DESCONHECIDO), "{mensagem}");
    assert!(c.cache().join(&log_rel).is_file());
    // a tela recebe a frase do catálogo
    let tela = mensagens::erro_de_comando(crate::error::SicroError::Validation(mensagem)).to_string();
    assert!(tela.contains(&format!("{}: ", mensagens::codigo::ITEM_DESCONHECIDO)), "{tela}");
    assert!(fila::saidas(&c.ws, None).unwrap().is_empty());
}

/// Caso em pasta só de leitura: a exportação falha com `sem_gravacao` ANTES de renderizar (nenhum bloco no cache) e
/// não deixa nada no caso; devolvida a permissão, exporta.
#[cfg(unix)]
#[test]
fn caso_so_de_leitura_falha_antes_de_renderizar() {
    use std::os::unix::fs::PermissionsExt;
    let Some(cap) = fontes::cap() else { return };
    sonda_do_app();
    let mt = montagem("leitura", &cap);
    let projeto = projeto_do_store(&mt, &cap, false);
    let c = &mt.caso;
    let edicoes = paths::dir_edicao_saida(&c.ws);
    std::fs::create_dir_all(&edicoes).unwrap();
    std::fs::set_permissions(&edicoes, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(edicoes.join("prova"), b"x").is_ok() {
        // root grava em qualquer pasta: não há o que medir
        std::fs::set_permissions(&edicoes, std::fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("dispensado: o usuário dos testes grava em pasta 0555");
        return;
    }
    let (doc, _, fim) = exportar(c, &projeto, "cpu");
    std::fs::set_permissions(&edicoes, std::fs::Permissions::from_mode(0o755)).unwrap();
    let Estado::Falhou { mensagem, .. } = fim else { panic!("exportou em pasta só de leitura: {fim:?}") };
    assert!(mensagem.starts_with(mensagens::codigo::SEM_GRAVACAO), "{mensagem}");
    assert_eq!(mensagem, format!("sem_gravacao: {}", mensagens::frase(mensagens::codigo::SEM_GRAVACAO, None, None)));
    assert_eq!(std::fs::read_dir(c.cache().join("blocos")).map(|r| r.count()).unwrap_or(0), 0, "renderizou antes de recusar");
    assert!(fila::saidas(&c.ws, None).unwrap().is_empty());
    assert_eq!(std::fs::read_dir(&edicoes).unwrap().count(), 0);
    let (_, fim) = exportar_aberto(c, &doc, "cpu");
    assert!(matches!(fim, Estado::Concluida { .. }), "{fim:?}");
}

/// Amostras reais de gravador (só leitura, quando existem): projeto de 2 trechos por amostra exporta; o quadro do
/// primeiro, do meio e do último slot de cada trecho é o da tabela do decodificador (o mais próximo entre os vizinhos,
/// a pelo menos 30 dB).
#[test]
#[ignore]
fn amostras_de_gravador_exportam() {
    let Some(cap) = fontes::cap() else { return };
    sonda_do_app();
    let v = cap.versao.clone();
    let amostras = fontes::amostras_de_gravador();
    eprintln!("amostras de gravador: {}", amostras.len());
    let mut lidas = 0;
    let mut discriminados = 0;
    for a in &amostras {
        let t0 = Instant::now();
        let c = Caso::novo("gravador");
        // formato que este ffmpeg não lê (Hikvision sem `moov`): fica de fora, dito
        let (vm, caminho, tab) = match registrar_e_ler(&cap, &c.ws, a) {
            Ok(x) if x.2.pts.len() >= 20 => x,
            Ok(x) => {
                eprintln!("{} ({v}): {} quadros, fica de fora", a.display(), x.2.pts.len());
                continue;
            }
            Err(e) => {
                eprintln!("{} ({v}): ilegível, fica de fora: {e}", a.display());
                continue;
            }
        };
        lidas += 1;
        let f = Fonte1 { vm, caminho, tab, numeros: Vec::new() };
        let n = f.tab.pts.len();
        let info = crate::edicao::quadros::fonte_info(&cap, &f.caminho, &f.vm.sha256, Arc::new(f.tab.clone()), None).unwrap();
        let (w, h) = (info.exibido.0 / 2 * 2, info.exibido.1 / 2 * 2);
        let fps = f.tab.taxa_nominal();
        let k = (n / 10).clamp(2, 30);
        // nada no último GOP (o `test.lvf` decodifica o fim de modo não determinístico no 9.0.1)
        let trechos = [(n / 10, n / 10 + k), (n / 2, n / 2 + k)];
        let trecho = |id: &str, (j_in, j_out): (usize, usize)| {
            json!({ "tipo": "trecho", "id": id, "rotulo": "", "desligado": false, "transicao_entrada": null, "efeitos": [], "midia": "m1",
                    "inicio_s": f.tab.tempo(j_in), "fim_s": f.tab.tempo(j_out), "inicio_pts": f.tab.pts[j_in], "fim_pts": f.tab.pts[j_out],
                    "velocidade": { "tipo": "constante", "fator": 1.0 }, "audio": { "volume": 1.0, "mudo": false, "manter_tom": true, "silencios": [] } })
        };
        let pid = uuid::Uuid::new_v4().to_string();
        let mut projeto: Value = serde_json::from_str(include_str!("../fixtures/edicao/projeto-do-store.json")).unwrap();
        projeto["id"] = json!(pid);
        projeto["titulo"] = json!(a.file_name().unwrap().to_string_lossy());
        projeto["saida"]["largura"] = json!(w);
        projeto["saida"]["altura"] = json!(h);
        projeto["saida"]["fps"] = json!({ "num": fps.num, "den": fps.den });
        projeto["saida"]["audio"] = json!(false);
        projeto["midias"] = json!([midia_ref("m1", &f, json!([]))]);
        projeto["principal"] = json!([trecho("00000000-0000-4000-8000-0000000000a1", trechos[0]), trecho("00000000-0000-4000-8000-0000000000a2", trechos[1])]);
        for k in ["audios", "rastreios", "sobreposicoes", "marcadores"] {
            projeto[k] = json!([]);
        }
        let (doc, analise, fim) = exportar(&c, &projeto, "cpu");
        let nome = a.display().to_string();
        let Estado::Concluida { .. } = fim else { panic!("{v} {nome}: {fim:?}; {:?}", analise.problemas) };
        let s = saida_unica(&c, &pid);
        let mp4 = c.ws.join(&s.arquivo);
        let plano = plano_de(&c, &doc, "cpu");
        for (item, trecho) in trechos.iter().enumerate() {
            let (q0, n_q, mapa) = mapa_do_item(&plano, item);
            assert_eq!(mapa[0] as usize, trecho.0, "{nome}");
            assert_eq!(n_q, slots_pela_tabela(&f.tab, *trecho, fps), "{v} {nome}: slots do trecho {item}");
            for k in [0, n_q / 2, n_q - 1] {
                let saida = medir::quadro(&cap, &mp4, (q0 + k as u64) as u32).unwrap();
                // o quadro esperado sai dos pts da tabela, não da previsão do plano
                let j = quadro_pela_tabela(&f.tab, *trecho, k, fps);
                assert_eq!(mapa[k as usize], j, "{v} {nome}: trecho {item}, slot {k}: previsão do plano");
                let fonte_j = medir::quadro(&cap, &f.caminho, j).unwrap();
                let d = psnr(&saida, &fonte_j);
                assert!(d >= PSNR_MINIMO_DB, "{v} {nome}: slot {k} a {d:.1} dB do quadro {j}");
                // o vizinho só discrimina quando difere do quadro j mais que o ruído da codificação (cena parada não)
                for viz in [j.saturating_sub(1), (j + 1).min(n as u32 - 1)].into_iter().filter(|x| *x != j) {
                    let fonte_v = medir::quadro(&cap, &f.caminho, viz).unwrap();
                    let (dv, entre) = (psnr(&saida, &fonte_v), psnr(&fonte_j, &fonte_v));
                    if entre < d - MARGEM_VIZINHO_DB {
                        assert!(d > dv, "{v} {nome}: slot {k} mais perto do quadro {viz} ({dv:.1} dB) que do {j} ({d:.1} dB)");
                        discriminados += 1;
                    }
                }
            }
        }
        eprintln!("{nome} ({v}): {} quadros na saída, {:.1} s", s.n_quadros, t0.elapsed().as_secs_f64());
    }
    // as do dossiê que o ffmpeg lê (Dahua, Hikvision com `moov`, LVF, IFV)
    if amostras.len() >= 6 {
        assert!(lidas >= 4, "{v}: só {lidas} amostras lidas");
        assert!(discriminados >= 12, "{v}: só {discriminados} comparações com vizinho que discrimina");
    }
    eprintln!("amostras lidas: {lidas}; comparações com vizinho que discrimina: {discriminados}");
}
