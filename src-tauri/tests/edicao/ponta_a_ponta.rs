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
        Self::novo_em(nome, "Casos ç", "cache do app")
    }

    /// Caso dentro da pasta `pai` e cache do app em `cache` (nomes que o perito e o sistema podem ter).
    fn novo_em(nome: &str, pai: &str, cache: &str) -> Self {
        let d = tempfile::Builder::new().prefix(&format!("p0f-{nome}-")).tempdir().expect("pasta temporária");
        let pai = d.path().join(pai);
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
        let app = d.path().join(cache);
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
                // `ciencia: <frase>: <códigos>`
                ciencias.extend(mensagem.rsplit(": ").next().unwrap_or_default().split(", ").map(String::from));
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
    let s = cmd::conferir(&c.ws, &saida_id, true).expect("edicao_conferir");
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
    // zoom ATIVO × ocultar: no meio do zoom animado o borrão vai com a imagem (a ocultação é da fonte, antes do
    // enquadramento). A caixa do borrão pela `A(τ)` do quadro mostrado já saiu de onde estava sem zoom: mede-se a
    // parte dela que fica FORA da posição antiga (ocultação aplicada depois do zoom deixaria a borda nítida ali)
    {
        let kz = 56u32;
        let tipado = armazem::tipado(&doc).unwrap();
        let efeitos = tipado
            .principal
            .iter()
            .find_map(|c| match c {
                crate::edicao::modelo::Tolerante::Conhecido(crate::edicao::modelo::ClipePrincipal::Trecho(t)) if t.base.id == "00000000-0000-4000-8000-000000000003" => Some(&t.base.efeitos),
                _ => None,
            })
            .expect("clipe 2");
        let tau = mt.m2.tab.tempo(mapa2[kz as usize] as usize);
        let origem = (mt.m2.vm.width.unwrap_or(320), mt.m2.vm.height.unwrap_or(180));
        let (cz, _) = crate::edicao::testes::aceite::no_canvas(efeitos, tau, BORRAO, origem, CANVAS);
        // linha de referência: a mesma faixa de colunas, na altura 0,45 da fonte (fora do borrão)
        let (rz, _) = crate::edicao::testes::aceite::no_canvas(efeitos, tau, [BORRAO[0], 0.44, BORRAO[2], 0.02], origem, CANVAS);
        assert!((cz[0] - bx0 as f64).abs() > 30.0, "{v}: o zoom ainda não moveu a caixa ({cz:?})");
        let (x0, x1) = ((cz[0].max(0.0) + 4.0) as u32, ((cz[0] + cz[2] - 4.0).min(bx0 as f64 - 2.0)) as u32);
        assert!(x1 > x0 + 20, "{v}: faixa medida estreita demais: {x0}..{x1} ({cz:?})");
        let y_dentro = ((cz[1].max(0.0) + cz[1] + cz[3]) / 2.0) as u32;
        let y_fora = (rz[1] + rz[3] / 2.0) as u32;
        let qz = medir::quadro(&cap, &mp4, (q2 + kz as u64) as u32).unwrap();
        let (dentro_z, fora_z) = (salto_max(&qz, y_dentro, x0, x1), salto_max(&qz, y_fora, x0, x1));
        // a ampliação (bilinear) amacia a borda: 98 medido fora da caixa, 11 dentro
        assert!(fora_z > 60 && (dentro_z as u32) * 3 < fora_z as u32, "{v}: com o zoom ativo: borda dentro {dentro_z}, fora {fora_z} (caixa {cz:?}, colunas {x0}..{x1})");
    }
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
            // `auto` sem NVENC aprovado: é a exportação de CPU, sem queda nenhuma, e o relatório diz o codificador
            assert_eq!((sg.codificador.as_str(), &sg.sha256), ("libx264", &sa.sha256), "{v}: sem GPU aprovada ({outro:?})");
            let tg = ler_json(&c.ws.join(sg.tecnico_rel.as_ref().expect("tecnico.json")));
            assert_eq!((tg["execucao"]["saida"]["codificador"].as_str(), &tg["execucao"]["saida"]["queda_para_cpu"]), (Some("libx264"), &Value::Null), "{v}");
            assert_eq!(tg["execucao"]["sistema"]["gpu"], Value::Null, "{v}: sem GPU aprovada, o relatório não cita placa");
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

/// Prévia do render com a GPU aprovada pela sonda falhando no render (D12): refeita em CPU pelo mesmo `compilar_alvo`
/// da fila, com o mesmo arquivo de uma prévia feita direto em CPU. Antes, o plano "de CPU" da prévia voltava com o
/// NVENC (a troca só existia na exportação) e a prévia falhava com `item_invalido`. A falha vem de um ffmpeg falso que
/// recusa o NVENC (a prévia não leva os argumentos da GPU: não há como forçar a falha pela linha).
#[cfg(unix)]
#[test]
fn previa_com_gpu_que_falha_cai_para_a_cpu() {
    use std::os::unix::fs::PermissionsExt;
    let Some(cap) = fontes::cap() else { return };
    let v = cap.versao.clone();
    let mt = montagem("previa-gpu", &cap);
    let doc = projeto_do_store(&mt, &cap, false);
    let c = &mt.caso;
    armazem::salvar(&c.ws, &doc).unwrap();
    let p = armazem::tipado(&doc).unwrap();
    let cache = c.cache();
    let mut prep = executar::preparar_com(&c.ws, &p, &cache, false, Some(&AtomicBool::new(false))).unwrap();
    let sem_gpu = Arc::new(Capacidades { gpu: None, ..(*prep.cap).clone() });
    let falsa = sonda::Gpu {
        nome: "falsa".into(),
        driver: String::new(),
        codificador: "h264_nvenc".into(),
        degrau: String::new(),
        args: vec![],
        nvdec: false,
        scale_cuda: false,
    };
    prep.cap = Arc::new(Capacidades { gpu: Some(falsa), ..(*prep.cap).clone() });
    // o ffmpeg do render: recusa o NVENC como a placa sem sessão livre; o resto vai ao verdadeiro
    let script = fontes::pasta_temporaria("previa-gpu-ffmpeg").join("ffmpeg");
    let real = cap.binario.to_string_lossy().replace('\'', "'\\''");
    std::fs::write(
        &script,
        format!(
            "#!/bin/bash\nfor a in \"$@\"; do [ \"$a\" = h264_nvenc ] && {{ echo '[h264_nvenc @ 0x0] OpenEncodeSessionEx failed: out of memory (10)' >&2; exit 1; }}; done\nexec '{real}' \"$@\"\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let render = Arc::new(Capacidades { binario: script, ..(*prep.cap).clone() });
    let alvo = Alvo::Previa { altura: 180, de_q: 0, ate_q: 45 };
    let gpu = |pl: &Plano| pl.blocos.iter().any(|b| b.codificador.iter().any(|a| a.ends_with("_nvenc")));
    // a codificação de prova no tamanho do alvo (`compilar_alvo`, P1.8) é da placa de verdade: a falsa "serve"
    let serve = |_: &Capacidades, _: u32, _: u32, _: Racional| true;
    assert!(gpu(&fila::compilar_alvo_com(&p, &prep, &alvo, false, &serve).unwrap()), "{v}: a prévia usa a GPU aprovada");
    let ambiente = |cap: Arc<Capacidades>, tarefa: &str| executar::Ambiente {
        cap,
        trabalho: cache.join("render").join(tarefa),
        cache: cache.clone(),
        caso: c.ws.clone(),
        pasta_projeto: prep.pasta_projeto.clone(),
        paralelos: 2,
        identidade: Default::default(),
        ciencias: vec![],
    };
    let (usado, r) = executar::executar_com_queda(
        &|cpu| fila::compilar_alvo_com(&p, &prep, &alvo, cpu, &serve),
        &ambiente(render, "previa-gpu"),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap_or_else(|e| panic!("{v}: prévia não caiu para a CPU: {e}"));
    assert!(!gpu(&usado) && r.codificador == "libx264", "{v}: {}", r.codificador);
    assert!(r.queda_para_cpu.is_some(), "{v}: queda não registrada");
    // o mesmo arquivo e o mesmo composto de uma prévia feita direto em CPU; quadros = slots da janela
    let mut so_cpu = executar::preparar_com(&c.ws, &p, &cache, false, None).unwrap();
    so_cpu.cap = sem_gpu.clone();
    let plano_cpu = fila::compilar_alvo(&p, &so_cpu, &alvo, false).unwrap();
    assert_eq!(usado, plano_cpu, "{v}");
    std::fs::remove_dir_all(cache.join("blocos")).unwrap();
    let direto = executar::executar(&plano_cpu, &ambiente(sem_gpu, "previa-cpu"), &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!((&r.sha256, &r.composto_sha256), (&direto.sha256, &direto.composto_sha256), "{v}");
    assert_eq!(medir::framehash(&cap, &r.arquivo).unwrap().len() as u64, usado.total_q, "{v}");
    assert_eq!(medir::framehash(&cap, &r.arquivo).unwrap(), medir::framehash(&cap, &direto.arquivo).unwrap(), "{v}");
}

/// Prévia do render pela fila real: o arquivo tocado fica (vídeo com os slots da janela e o som), o PCM intermediário
/// sai da pasta de trabalho. Antes ficava: ~11 MB por minuto de janela, 7 dias no cache e fora da conta do teto.
#[test]
fn previa_do_render_nao_guarda_o_pcm() {
    let Some(cap) = fontes::cap() else { return };
    sonda_do_app();
    let v = cap.versao.clone();
    let mt = montagem("previa-pcm", &cap);
    let doc = projeto_do_store(&mt, &cap, false);
    let c = &mt.caso;
    armazem::salvar(&c.ws, &doc).unwrap();
    let fps: Racional = serde_json::from_value(doc["saida"]["fps"].clone()).unwrap();
    let (de_q, ate_q) = (10u64, 70u64);
    let tarefa = fila::enfileirar(cmd::pedido(&c.ws, doc, Alvo::Previa { altura: 180, de_q, ate_q }, vec![], &c.app).unwrap()).unwrap();
    let Estado::Pronta { relativo } = fila::esperar(&tarefa).unwrap() else { panic!("{v}: prévia não ficou pronta") };
    let arq = c.cache().join(&relativo);
    let trabalho = c.cache().join("render").join(&tarefa);
    assert!(arq.starts_with(&trabalho), "{relativo}");
    assert_eq!(medir::framehash(&cap, &arq).unwrap().len() as u64, ate_q - de_q, "{v}");
    // o som da janela no arquivo tocado (o AAC acrescenta até um quadro de 1024 amostras)
    let esperadas = ((ate_q - de_q) as i64 * 48_000 * fps.den / fps.num) as u64;
    let amostras = medir::amostras(&cap, &arq).unwrap_or_else(|e| panic!("{v}: prévia sem som: {e}"));
    assert!(amostras.abs_diff(esperadas) <= 1024, "{v}: {amostras} amostras, esperadas {esperadas}");
    let sobras: Vec<String> = std::fs::read_dir(&trabalho).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert!(!sobras.iter().any(|n| n.ends_with(".wav")), "{v}: PCM na pasta de trabalho: {sobras:?}");
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

// ------------------------------------------------------------------ Fase 1: um projeto com tudo

/// Canvas e taxa do projeto da Fase 1.
const F1_CANVAS: (u32, u32) = (640, 360);
/// Trechos da m1 (índices da tabela): o das medidas (lupa, régua, velocidade) e o do realce (estabilizar e média).
const F1_MEDIDAS: (usize, usize) = (30, 90);
const F1_REALCE: (usize, usize) = (150, 210);
/// Volume do arquivo entregue contra o medido mais o ganho (LU): o AAC e o limite do aceite do P1.5.
const F1_LU: f64 = 0.5;

fn titulo_do_modelo(modelo: &str, preso: bool, textos: &[&str]) -> Value {
    use crate::edicao::camada::titulo;
    use crate::edicao::modelo::{ModeloTitulo, PapelTexto, TamanhoPx};
    let m: ModeloTitulo = serde_json::from_value(json!(modelo)).unwrap();
    let canvas = TamanhoPx { w: F1_CANVAS.0, h: F1_CANVAS.1 };
    let mut t = titulo::titulo_no_modelo(m, preso, "b0", canvas);
    t.blocos[0].conteudo = textos[0].to_string();
    for (i, x) in textos.iter().enumerate().skip(1) {
        let b = titulo::bloco_no_modelo(&t, PapelTexto::Subtitulo, &format!("b{i}"), x, canvas);
        t.blocos.push(b);
    }
    serde_json::to_value(&t).unwrap()
}

fn clipe_titulo(id: &str, modelo: &str, textos: &[&str]) -> Value {
    json!({ "tipo": "titulo", "id": id, "rotulo": format!("Tela {modelo}"), "desligado": false, "transicao_entrada": null, "efeitos": [], "duracao_s": 1.0,
            "titulo": titulo_do_modelo(modelo, false, textos) })
}

/// Um projeto com tudo o que a Fase 1 entrega: transição, lupa, régua, velocidade, realce (estabilizar e média), som
/// com narração, silêncio e normalização, legendas na imagem e em arquivo, um título de cada modelo e sobrepostos
/// presos (terço inferior no clipe das medidas, marca no programa inteiro).
fn projeto_da_fase_1(c: &Caso, cap: &Capacidades, m1: &Fonte1, trf: Option<(&str, f64)>) -> Value {
    let id = uuid::Uuid::new_v4().to_string();
    let t = &m1.tab;
    let sha = m1.vm.sha256.clone();
    // foto (a marca) e narração: mídia própria do projeto, pelos comandos de importar
    let dir = fontes::pasta_temporaria("fase1-midias");
    let png = dir.join("marca.png");
    image::RgbaImage::from_fn(64, 64, |x, y| if (x / 8 + y / 8) % 2 == 0 { image::Rgba([255, 0, 255, 255]) } else { image::Rgba([0, 255, 255, 255]) }).save(&png).unwrap();
    let mut foto = serde_json::to_value(armazem::importar_midia(&c.ws, &id, &png, TipoMidia::Imagem).expect("importar imagem")).unwrap();
    foto["id"] = json!("foto");
    let wav = dir.join("narração.wav");
    gerar_wav(cap, &wav);
    let mut narra = serde_json::to_value(armazem::importar_midia(&c.ws, &id, &wav, TipoMidia::Audio).expect("importar narração")).unwrap();
    narra["id"] = json!("narra");
    let estilo = json!({ "fonte": "sans", "negrito": true, "italico": false, "tamanho": 0.05, "cor": "#ffffff", "alinhamento": "centro", "caixa": { "cor": "#000000c0", "margem": 0.01, "raio": 0.2 }, "contorno": null });
    let traco = json!({ "cor": "#ffff00", "espessura": 0.008, "contorno": true });
    let base = |tipo: &str, id: &str| json!({ "tipo": tipo, "id": id, "ligado": true, "nome": "", "ativo": null });
    let com = |mut a: Value, b: Value| {
        for (k, v) in b.as_object().unwrap() {
            a[k] = v.clone();
        }
        a
    };
    let trecho = |id: &str, (j_in, j_out): (usize, usize), efeitos: Value, silencios: Value, transicao: Value| {
        json!({ "tipo": "trecho", "id": id, "rotulo": id, "desligado": false, "transicao_entrada": transicao, "efeitos": efeitos, "midia": "m1",
                "inicio_s": t.tempo(j_in), "fim_s": t.tempo(j_out), "inicio_pts": t.pts[j_in], "fim_pts": t.pts[j_out],
                "velocidade": { "tipo": "constante", "fator": 1.0 }, "audio": { "volume": 1.0, "mudo": false, "manter_tom": true, "silencios": silencios } })
    };
    // medidas: a calibração é da m1 (plano com H = identidade no espaço de 320×180: 1 m = 1 px)
    let lupa = com(base("lupa", "e-lupa"), json!({ "rastreio": "r-lupa", "janela": 0.2, "fator": 2.0, "forma": "circulo", "interpolacao": "bilinear",
        "onde": { "modo": "fixa", "x": 0.25, "y": 0.35 }, "moldura": traco, "ligacao": true }));
    let regua = com(base("regua", "e-regua"), json!({ "calibracao": { "id": "k1", "metodo": "plane", "h": [1, 0, 0, 0, 1, 0, 0, 0, 1], "espaco": { "w": 320, "h": 180 },
        "pontos_sha256": "a".repeat(64), "residuo_m": 0.02, "regiao": [{ "x": 0.1, "y": 0.5 }, { "x": 0.9, "y": 0.5 }, { "x": 0.9, "y": 0.95 }, { "x": 0.1, "y": 0.95 }], "midia_sha256": sha },
        "ancora": { "x": 0.3, "y": 0.8 }, "direcao_graus": 0.0, "comprimento_m": 100.0, "divisoes": 4, "traco": traco, "rotulo": true }));
    let velocidade = com(base("velocidade_exibida", "e-vel"), json!({ "calculo": { "id": "v1", "kmh": 42.0, "faixa": [39.0, 45.0], "de_s": t.tempo(F1_MEDIDAS.0), "ate_s": t.tempo(F1_MEDIDAS.1 - 1),
        "metodo": "monte_carlo", "midia_sha256": sha }, "onde": { "modo": "fixa", "lugar": { "x": 0.25, "y": 0.6, "h": "centro", "v": "topo" } }, "estilo": estilo }));
    // realce: estabilizar com a análise no trecho inteiro e média de 3
    let (d0, d1) = (F1_REALCE.0, F1_REALCE.1 - 1);
    let estabilizar = com(base("estabilizar", "e-estab"), json!({ "suavizar": 10, "borda": "preta", "analise": { "de_s": t.tempo(d0), "ate_s": t.tempo(d1), "de_pts": t.pts[d0], "ate_pts": t.pts[d1] },
        "zoom_pct": trf.map_or(0.0, |x| x.1), "trf_sha256": trf.map(|x| x.0) }));
    let media = com(base("media_temporal", "e-media"), json!({ "quadros": 3 }));
    let silencio = json!([{ "id": "si1", "de": t.tempo(165), "ate": t.tempo(180), "modo": "tom", "motivo": "nome de terceiro" }]);
    let tr = json!({ "x": 0.5, "y": 0.5, "escala": 1.0, "rotacao": 0.0, "opacidade": 1.0 });
    let legenda = |id: &str, texto: &str, origem: &str, r: Value| json!({ "id": id, "texto": texto, "locutor": null, "origem": origem, "conferida": true, "locutor_auto": false, "ref": r });
    json!({
        "esquema": 2, "esquema_menor": 0, "id": id, "titulo": "Fase 1", "created_at": "2026-11-15T00:00:00Z", "updated_at": "2026-11-15T00:00:00Z", "gravado_por": "6.0.0",
        "saida": { "largura": F1_CANVAS.0, "altura": F1_CANVAS.1, "fps": { "num": 30, "den": 1 }, "auto": false, "cor_fundo": "#101010", "audio": true,
                   "selos": { "velocidade": true, "ampliacao": true, "realce": true },
                   "exportacao": { "predefinicao": "juri", "codificador": "cpu", "legendas": "ambos", "pacote": null, "pdf": "a3", "revisado_por": "", "personalizada": null } },
        "midias": [midia_ref("m1", m1, json!([])), foto, narra],
        "principal": [
            clipe_titulo("t-abertura", "abertura", &["Exame de vídeo", "Caso 77/26"]),
            trecho("c-medidas", F1_MEDIDAS, json!([lupa, regua, velocidade]), json!([]), json!(null)),
            clipe_titulo("t-evento", "evento", &["O portão abre"]),
            trecho("c-realce", F1_REALCE, json!([estabilizar, media]), silencio, json!({ "tipo": "fundir", "duracao_s": 0.5, "audio": "acompanhar" })),
            clipe_titulo("t-cartela", "cartela", &["Fim da gravação"]),
            clipe_titulo("t-fim", "encerramento", &["Perícia criminal"]),
        ],
        "fecho": { "tipo": "escurecer", "duracao_s": 0.3, "audio": "acompanhar" },
        "sobreposicoes": [{ "id": "s1", "nome": "S1", "oculta": false, "clipes": [
            { "tipo": "titulo", "id": "sob-terco", "rotulo": "Local", "pos": { "ref": "clipe", "clipe": "c-medidas", "desloc": 0.0 }, "duracao_s": null, "transformacao": tr,
              "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [], "titulo": titulo_do_modelo("terco_inferior", true, &["Rua das Flores, 120"]) },
            { "tipo": "imagem", "id": "sob-marca", "rotulo": "Marca", "pos": { "ref": "programa", "t": 0.0 }, "duracao_s": null,
              "transformacao": { "x": 0.08, "y": 0.12, "escala": 0.1, "rotacao": 0.0, "opacidade": 1.0 }, "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [], "midia": "foto" },
        ] }],
        "audios": [{ "id": "a1", "nome": "Narração", "papel": "narracao", "volume": 1.0, "mudo": false, "abaixa_original": true,
            "clipes": [{ "id": "ca1", "nome": "fala", "midia": "narra", "pos": { "ref": "clipe", "clipe": "c-medidas", "desloc": 0.2 }, "de_s": 0.0, "ate_s": 1.0, "volume": 1.0, "entrada_s": 0.0, "saida_s": 0.1 }] }],
        "mixagem": { "volume_original": 1.0, "abaixar": { "db": -12.0, "ataque_s": 0.3, "soltura_s": 0.5 }, "normalizar": { "lufs": -16.0, "pico_db": -1.5 }, "limitador": false },
        "legendas": { "estilo": estilo, "lugar": { "x": 0.5, "y": 0.95, "h": "centro", "v": "base" }, "linhas_max": 2, "origem": null, "itens": [
            legenda("g1", "Abertura do exame", "perito", json!({ "tipo": "programa", "de": 0.2, "ate": 0.8 })),
            legenda("g2", "O portão está fechado", "ia", json!({ "tipo": "fonte", "midia": "m1", "de": t.tempo(50), "ate": t.tempo(70) })),
        ] },
        "rastreios": [{ "id": "r-lupa", "nome": "alvo", "cor": "#ff0000", "midia": "m1", "base": "fonte", "tipo": "caixa", "espaco": "original",
            "ancoras": [{ "t": t.tempo(F1_MEDIDAS.0), "pts": t.pts[F1_MEDIDAS.0], "v": caixa([0.6, 0.45, 0.1, 0.15]), "interp": "linear" }],
            "denso": null, "silhuetas": null, "ausencias": [], "origem": { "tipo": "manual" }, "confirmacao": null }],
        "marcadores": [], "sincronias": []
    })
}

/// Pixels claros (os três canais acima de 200) na caixa `[x0, y0, x1, y1]` em fração do quadro.
fn claros(q: &QuadroRgb, c: [f64; 4]) -> usize {
    let (x0, y0, x1, y1) = ((c[0] * q.w as f64) as u32, (c[1] * q.h as f64) as u32, (c[2] * q.w as f64) as u32, (c[3] * q.h as f64) as u32);
    (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| px(q, x, y).iter().all(|v| *v > 200)).count()
}

/// Pixels em que os dois quadros diferem em mais de 24 num canal, na caixa (fração do quadro).
fn diferentes(a: &QuadroRgb, b: &QuadroRgb, c: [f64; 4]) -> usize {
    let (x0, y0, x1, y1) = ((c[0] * a.w as f64) as u32, (c[1] * a.h as f64) as u32, (c[2] * a.w as f64) as u32, (c[3] * a.h as f64) as u32);
    (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| px(a, x, y).iter().zip(px(b, x, y)).any(|(p, q)| p.abs_diff(q) > 24)).count()
}

/// Volume integrado (LUFS) de um arquivo, pela mesma medição do motor.
fn lufs_de(cap: &Capacidades, arquivo: &Path) -> f64 {
    let out = crate::tools::command(&cap.binario)
        .args(["-hide_banner", "-nostdin", "-nostats", "-cpuflags", "0", "-i"])
        .arg(arquivo)
        .args(["-vn", "-af", "loudnorm=I=-16:TP=-1.5:LRA=50:print_format=json", "-f", "null", "-"])
        .output()
        .expect("ffmpeg");
    let texto = String::from_utf8_lossy(&out.stderr).into_owned();
    crate::edicao::motor::audio::ler_loudnorm(&texto).expect("loudnorm").integrado.expect("volume medido")
}

/// Texto do PDF como um leitor de PDF o entrega (`pdftotext`); None sem o programa.
fn texto_do_pdf(pdf: &Path) -> Option<String> {
    let out = std::process::Command::new("pdftotext").args(["-layout", "-enc", "UTF-8"]).arg(pdf).arg("-").output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Texto na ordem em que foi escrito (célula de tabela que quebra de linha fica inteira, o `-layout` a intercala).
fn texto_cru_do_pdf(pdf: &Path) -> Option<String> {
    let out = std::process::Command::new("pdftotext").args(["-raw", "-enc", "UTF-8"]).arg(pdf).arg("-").output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).split_whitespace().collect::<Vec<_>>().join(" "))
}

/// O projeto da Fase 1 pela fila, em CPU, até conferido: MP4 com a contagem certa, legendas em arquivo, cada recurso
/// na imagem ou no som, `tecnico.json` com tudo no `feito` e o PDF do júri gerado dele, aberto por fora e com os
/// números do `feito`. A segunda exportação, com o `.trf` já gravado no projeto, dá os mesmos quadros compostos.
#[test]
fn fase_1_projeto_com_tudo_exporta_com_relatorio_e_pdf() {
    use crate::edicao::modelo::ModoPdf;
    use crate::edicao::relatorio::juri;
    let Some(cap) = fontes::cap() else { return };
    let cap_app = sonda_do_app();
    assert_eq!(cap_app.sha256, cap.sha256, "o ffmpeg do app é o dos testes");
    let v = cap.versao.clone();
    let c = Caso::novo("fase1");
    let m1 = registrar(&cap, &c.ws, &fontes::gerar(Fonte::Mp4).unwrap());
    let projeto = projeto_da_fase_1(&c, &cap, &m1, None);
    let id = projeto["id"].as_str().unwrap().to_string();

    let (doc, analise, fim) = exportar(&c, &projeto, "cpu");
    let erros: Vec<_> = analise.problemas.iter().filter(|p| p.nivel == Nivel::Erro && p.codigo != mensagens::codigo::TABELA_PENDENTE).collect();
    assert!(erros.is_empty(), "{v}: {erros:#?}");
    // a média de quadros pede ciência; o silêncio pede conferência
    assert!(analise.problemas.iter().any(|p| p.codigo == mensagens::codigo::MEDIA_DE_QUADROS && p.nivel == Nivel::Pericial), "{v}");
    let Estado::AConferir { saida_id } = fim else { panic!("{v}: esperava a conferir, veio {fim:?}") };
    let s = cmd::conferir(&c.ws, &saida_id, true).expect("edicao_conferir");
    assert_eq!(s.estado, EstadoSaida::Concluida);
    let plano = plano_de(&c, &doc, "cpu");
    // 30 + 60 + 30 + 60 − 15 (fundir de 0,5 s) + 30 + 30
    let total_q = 225u64;
    assert_eq!((plano.total_q, analise.linha.total_q, s.n_quadros), (total_q, total_q, total_q), "{v}");
    let fps = plano.canvas.fps;
    assert_eq!(mapa_do_item(&plano, 1).2, (0..60).map(|k| quadro_pela_tabela(&m1.tab, F1_MEDIDAS, k, fps)).collect::<Vec<_>>(), "{v}: mapa do clipe das medidas");

    // ---- o arquivo entregue
    let mp4 = c.ws.join(&s.arquivo);
    let decodificados = medir::framehash(&cap, &mp4).unwrap();
    assert_eq!(decodificados.len() as u64, total_q, "{v}");
    assert_eq!(s.decodificado_sha256, Some(conferir::hash_de_conteudo(&decodificados)), "{v}");
    let pasta = mp4.parent().unwrap().to_path_buf();
    let base = mp4.file_stem().unwrap().to_string_lossy().into_owned();
    // legendas em arquivo ao lado do vídeo (`<Título>.srt` e `.vtt`), com os dois textos e os tempos do programa
    let na_pasta: Vec<String> = std::fs::read_dir(&pasta).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    let com_ext = |ext: &str| {
        let nomes: Vec<&String> = na_pasta.iter().filter(|n| n.ends_with(ext)).collect();
        assert_eq!(nomes.len(), 1, "{v}: um {ext} na pasta da saída: {na_pasta:?}");
        std::fs::read_to_string(pasta.join(nomes[0])).unwrap()
    };
    let (srt, vtt) = (com_ext(".srt"), com_ext(".vtt"));
    assert!(srt.contains("Abertura do exame") && srt.contains("O portão está fechado") && srt.contains("00:00:00,200 --> 00:00:00,800"), "{v}: {srt}");
    assert!(vtt.starts_with("WEBVTT") && vtt.contains("O portão está fechado"), "{v}");
    // a legenda de fonte (quadros 50 a 70 do trecho que começa no 30) cai nos slots 30 + 20 .. 30 + 40
    assert!(srt.contains("00:00:01,667 --> 00:00:02,333"), "{v}: {srt}");

    // ---- imagem: cada recurso onde tem de estar
    let quadro = |q: u32| medir::quadro(&cap, &mp4, q).unwrap_or_else(|e| panic!("{v}: quadro {q}: {e}"));
    let embaixo = [0.2, 0.84, 0.8, 0.99];
    // legenda queimada: no slot 10 (0,33 s) sim, no 27 (0,9 s) não, sobre a mesma tela de abertura
    let (com_legenda, sem_legenda) = (quadro(10), quadro(27));
    assert!(claros(&com_legenda, embaixo) > claros(&sem_legenda, embaixo) + 150, "{v}: legenda queimada: {} × {}", claros(&com_legenda, embaixo), claros(&sem_legenda, embaixo));
    // régua (traço amarelo de 0,3 a 0,61 da largura, na altura 0,8) e a lupa (moldura amarela em volta de (0,25; 0,35))
    let medidas = quadro(30 + 5);
    let amarelos = |q: &QuadroRgb, c: [f64; 4]| {
        let (x0, y0, x1, y1) = ((c[0] * q.w as f64) as u32, (c[1] * q.h as f64) as u32, (c[2] * q.w as f64) as u32, (c[3] * q.h as f64) as u32);
        (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| eh_amarelo(px(q, x, y))).count()
    };
    assert!(amarelos(&medidas, [0.28, 0.76, 0.64, 0.84]) > 150, "{v}: régua: {}", amarelos(&medidas, [0.28, 0.76, 0.64, 0.84]));
    assert!(amarelos(&medidas, [0.05, 0.05, 0.45, 0.6]) > 150, "{v}: moldura da lupa: {}", amarelos(&medidas, [0.05, 0.05, 0.45, 0.6]));
    // a marca (xadrez magenta e ciano no canto de cima à esquerda) no programa inteiro: títulos e trechos
    let magenta = |q: &QuadroRgb| {
        let (w, h) = (q.w, q.h);
        (0..h / 4).flat_map(|y| (0..w / 5).map(move |x| (x, y))).filter(|&(x, y)| { let p = px(q, x, y); p[0] > 180 && p[1] < 90 && p[2] > 180 }).count()
    };
    for q in [3u32, 40, 100, 150, 220] {
        assert!(magenta(&quadro(q)) > 100, "{v}: marca no slot {q}: {}", magenta(&quadro(q)));
    }
    // selo no canto de cima à direita: no clipe do realce (média e imagem tratada) e no das medidas (ampliada); a tela
    // de texto não leva
    let selo = [0.5, 0.02, 0.99, 0.12];
    let (realce, tela) = (quadro(150), quadro(125));
    assert!(claros(&realce, selo) > 80, "{v}: selo do realce: {}", claros(&realce, selo));
    assert!(claros(&medidas, selo) > 80, "{v}: selo da ampliação: {}", claros(&medidas, selo));
    assert!(diferentes(&realce, &tela, selo) > 80, "{v}");
    // terço inferior preso ao clipe das medidas: aparece nele e some na tela seguinte
    let terco = [0.02, 0.62, 0.7, 0.9];
    assert!(claros(&medidas, terco) > claros(&quadro(95), terco) + 100, "{v}: terço inferior: {} × {}", claros(&medidas, terco), claros(&quadro(95), terco));
    // transição: no meio do fundir (slots 120 a 134) o quadro não é nem a tela do evento nem o trecho
    let (antes, meio, depois) = (quadro(118), quadro(127), quadro(140));
    let tudo = [0.0, 0.15, 1.0, 0.8];
    assert!(diferentes(&meio, &antes, tudo) > 2000 && diferentes(&meio, &depois, tudo) > 2000, "{v}: fundir: {} e {}", diferentes(&meio, &antes, tudo), diferentes(&meio, &depois, tudo));

    // ---- relatório técnico: o `feito` conta tudo
    let tec = ler_json(&c.ws.join(s.tecnico_rel.as_ref().expect("tecnico.json")));
    let f = &tec["feito"];
    assert_eq!((f["saida"]["n_quadros"].as_u64(), f["saida"]["largura"].as_u64(), f["roteiro"].as_array().map(Vec::len)), (Some(total_q), Some(640), Some(6)), "{v}");
    let nomes = |item: usize| f["roteiro"][item]["processamentos"].as_array().unwrap().iter().map(|p| p["nome"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(nomes(1), ["lupa", "regua", "velocidade_exibida"], "{v}");
    assert_eq!(nomes(3), ["estabilizar", "media_temporal"], "{v}");
    assert_eq!(f["roteiro"][3]["transicao"]["tipo"], "fundir");
    let selos = |item: usize| f["roteiro"][item]["selos"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!((selos(1), selos(3), selos(0)), (vec!["Imagem ampliada".to_string()], vec!["Imagem tratada".to_string(), "Média de 3 imagens".to_string()], vec![]), "{v}");
    let sob = f["sobrepostos"].as_array().expect("sobrepostos no feito");
    assert_eq!(sob.iter().map(|x| (x["clipe"].as_str().unwrap(), x["tipo"].as_str().unwrap(), x["na_saida"]["de_q"].as_u64().unwrap(), x["na_saida"]["n_q"].as_u64().unwrap())).collect::<Vec<_>>(),
               vec![("sob-terco", "titulo", 30, 60), ("sob-marca", "imagem", 0, total_q)], "{v}");
    assert_eq!((sob[0]["preso_a"].as_str(), sob[0]["modelo"].as_str(), sob[0]["textos"][0].as_str()), (Some("c-medidas"), Some("terco_inferior"), Some("Rua das Flores, 120")));
    assert_eq!((f["legendas"]["modo"].as_str(), f["legendas"]["n"].as_u64(), f["legendas"]["ia"].as_u64(), f["legendas"]["ia_conferidas"].as_u64()), (Some("ambos"), Some(2), Some(1), Some(1)), "{v}");
    assert_eq!((f["audio"]["silencios"].as_array().map(Vec::len), f["audio"]["trilhas"][0]["papel"].as_str(), f["audio"]["mixagem"]["normalizar"]["lufs"].as_f64()), (Some(1), Some("narracao"), Some(-16.0)), "{v}");
    assert_eq!(f["estabilizacoes"].as_array().map(Vec::len), Some(1), "{v}");
    let midia1 = f["midias"].as_array().unwrap().iter().find(|m| m["id"] == "m1").unwrap();
    assert_eq!((midia1["largura"].as_u64(), midia1["altura"].as_u64(), midia1["tamanho"].as_u64()), (Some(320), Some(180), Some(std::fs::metadata(&m1.caminho).unwrap().len())), "{v}");
    assert_eq!(f["avisos"].as_array().unwrap().iter().filter(|a| a["codigo"] == "media_de_quadros" && a["ciente"] == true).count(), 1, "{v}");
    // normalização pelo executor de verdade: medição, ganho pela regra e o volume do arquivo entregue
    let e = &tec["execucao"];
    let (medido, pico, ganho) = (e["audio"]["loudnorm"]["integrado"].as_f64().expect("volume medido"), e["audio"]["loudnorm"]["pico"].as_f64().expect("pico medido"), e["audio"]["ganho_db"].as_f64().expect("ganho"));
    assert!((ganho - (-16.0 - medido).min(-1.5 - pico)).abs() < 1e-6, "{v}: ganho {ganho} com {medido} LUFS e {pico} dBTP");
    let entregue = lufs_de(&cap, &mp4);
    assert!((entregue - (medido + ganho)).abs() <= F1_LU, "{v}: entregue {entregue:.2} LUFS × {:.2} (medido {medido:.2} + ganho {ganho:.2})", medido + ganho);
    assert_eq!(e["conferencia"]["quadros"].as_u64(), Some(total_q));
    // o `.trf` da estabilização ficou no projeto
    let vidstab = armazem::pasta_projeto(&c.ws, &id).join("vidstab");
    let trfs: Vec<PathBuf> = std::fs::read_dir(&vidstab).unwrap_or_else(|e| panic!("{v}: vidstab: {e}")).flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "trf")).collect();
    assert_eq!(trfs.len(), 1, "{v}: {trfs:?}");
    let trf_sha = trfs[0].file_stem().unwrap().to_string_lossy().into_owned();
    assert_eq!(sha256_bytes(&std::fs::read(&trfs[0]).unwrap()), trf_sha);

    // ---- PDF do júri, gerado do `tecnico.json` como o pacote faz
    let arquivos = vec![juri::ArquivoDoPacote { rel: format!("{base}.mp4"), sha256: s.sha256.clone() }];
    let dados = juri::dados(&tec, &[], &arquivos).unwrap_or_else(|e| panic!("{v}: dados do júri: {e}"));
    let projeto_json = serde_json::to_vec(&doc).unwrap();
    let entrada = juri::entrada(&tec, dados.clone(), &[], &projeto_json, ModoPdf::A3).unwrap();
    let saida = crate::relatorio_pdf::gerar(&entrada).unwrap_or_else(|e| panic!("{v}: PDF: {e}"));
    assert!(saida.pdf.starts_with(b"%PDF-") && saida.paginas >= 6, "{v}: {} páginas", saida.paginas);
    assert!(saida.avisos.is_empty(), "{v}: {:?}", saida.avisos);
    let pdf = fontes::pasta_temporaria("fase1-pdf").join(format!("Relatório-{v}.pdf"));
    std::fs::write(&pdf, &saida.pdf).unwrap();
    // aberto por um leitor de fora (poppler), quando há; senão, o texto que o próprio modelo compôs
    let texto = match texto_do_pdf(&pdf) {
        Some(t) => t,
        None => {
            assert!(std::env::var_os("SICRO_EXIGE_FFMPEG").is_none(), "pdftotext ausente: o PDF não foi aberto por fora");
            eprintln!("dispensado: sem pdftotext, o PDF é conferido pelo texto do modelo");
            crate::relatorio_pdf::texto(&entrada).unwrap()
        }
    };
    let liso: String = texto.split_whitespace().collect::<Vec<_>>().join(" ");
    let cru = texto_cru_do_pdf(&pdf).unwrap_or_default();
    for secao in ["O que é este documento", "O vídeo entregue", "As gravações usadas", "O que aparece no vídeo, na ordem", "O que foi acrescentado e o que foi modificado",
                  "Tempo, velocidade e câmeras alinhadas", "Limites deste vídeo", "Como conferir", "Identificação dos arquivos", "Propriedades das gravações",
                  "Processamentos, na ordem de aplicação", "Diferenças entre origem e saída", "Ocultações de imagem e de som", "Codificação da saída", "Programas e versões", "Referências"] {
        assert!(liso.contains(secao), "{v}: PDF sem a seção {secao:?}");
    }
    // os números do `feito`, como o PDF os escreve
    let dur = f["saida"]["duracao_s"].as_f64().unwrap();
    assert_eq!(dur, 7.5);
    for (o_que, trecho) in [
        ("duração do vídeo", "7,5 s".to_string()),
        ("tamanho da saída", "640 × 360 pontos".to_string()),
        ("tamanho da gravação", "320 × 180 pontos".to_string()),
        ("legendas e o modo", "Legendas (2)".to_string()),
        ("onde as legendas saíram", "na imagem e em arquivo que acompanha o vídeo".to_string()),
        ("terço inferior", "Texto sobreposto: Rua das Flores, 120".to_string()),
        ("marca", "Imagem sobreposta: Marca".to_string()),
        ("média", "Média de imagens (3)".to_string()),
        ("selo da média", "Média de 3 imagens".to_string()),
        ("régua", "Régua de escala de 100,00 m".to_string()),
        ("velocidade", "42 km/h".to_string()),
        ("narração", "Narração".to_string()),
        ("som ocultado", "Som ocultado".to_string()),
        ("conferência", PERITO.to_string()),
    ] {
        assert!(liso.contains(&trecho) || cru.contains(&trecho), "{v}: PDF sem {o_que} ({trecho:?})");
    }
    // o SHA-256 do vídeo: o PDF é a raiz (o código pode quebrar de linha: os 16 primeiros dígitos)
    let sem_espaco: String = texto.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(sem_espaco.contains(&s.sha256[..16]), "{v}: SHA-256 do vídeo fora do PDF");
    // e nada do que o `feito` não prova: a frase da conferência das legendas automáticas vale (1 de 1)
    assert!(liso.contains("Legendas escritas a partir de degravação automática") == f["legendas"]["origem"]["motor"].is_string(), "{v}");
    // os acréscimos do PDF são os do `feito`: um por sobreposto, mais as legendas
    let acrescimos: Vec<&str> = dados["acrescimos"].as_array().unwrap().iter().map(|l| l["o_que"].as_str().unwrap()).collect();
    assert_eq!(acrescimos.iter().filter(|x| x.starts_with("Texto sobreposto") || x.starts_with("Imagem sobreposta")).count(), sob.len(), "{v}: {acrescimos:?}");

    // ---- de novo, com o `.trf` gravado no projeto (o que a tela faz depois do intermediário): mesmos quadros
    let zoom = 0.0;
    let mut de_novo = projeto_da_fase_1(&c, &cap, &m1, Some((&trf_sha, zoom)));
    de_novo["titulo"] = json!("Fase 1 de novo");
    // a pasta do projeto anterior tem o `.trf`: o novo id recebe a cópia, como `duplicar` faz
    let destino = armazem::pasta_projeto(&c.ws, de_novo["id"].as_str().unwrap()).join("vidstab");
    std::fs::create_dir_all(&destino).unwrap();
    std::fs::copy(&trfs[0], destino.join(trfs[0].file_name().unwrap())).unwrap();
    let (doc2, _, fim2) = exportar(&c, &de_novo, "cpu");
    let Estado::AConferir { saida_id: saida2 } = fim2 else { panic!("{v}: de novo: {fim2:?}") };
    let s2 = cmd::conferir(&c.ws, &saida2, true).expect("edicao_conferir");
    let plano2 = plano_de(&c, &doc2, "cpu");
    assert!(plano2.previas.iter().all(|p| !matches!(p, crate::edicao::motor::plano::Previa::Vidstab { .. })), "{v}: com o .trf gravado não há análise de novo");
    assert_eq!(s2.composto_sha256, s.composto_sha256, "{v}: o .trf gravado dá os mesmos quadros da análise feita na exportação");
    let tec2 = ler_json(&c.ws.join(s2.tecnico_rel.as_ref().unwrap()));
    assert_eq!(tec2["feito"]["estabilizacoes"][0]["trf_sha256"].as_str(), Some(trf_sha.as_str()), "{v}");
    eprintln!("Fase 1 ({v}): {total_q} quadros, PDF de {} páginas, {medido:.2} LUFS medido, ganho {ganho:.2} dB, entregue {entregue:.2} LUFS", saida.paginas);
}

// ------------------------------------------------------------------ comandos do palco (fecho da Fase 1)

/// Espera a tarefa de fundo dessa chave acabar e devolve o estado final.
fn esperar_fundo(chave: &str) -> crate::edicao::fundo::EstadoFundo {
    use crate::edicao::fundo::{self, EstadoFundo};
    let t0 = Instant::now();
    loop {
        match fundo::estado(chave).map(|t| t.estado) {
            Some(EstadoFundo::NaFila | EstadoFundo::Rodando) => {}
            Some(fim) => return fim,
            None => panic!("tarefa de fundo {chave} sumiu"),
        }
        assert!(t0.elapsed() < Duration::from_secs(180), "tarefa de fundo {chave} presa");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// `edicao_proxy_pedir` e `edicao_origem_estado`, pelas funções dos comandos, num caso de verdade: sem medição (ou com
/// busca rápida) o palco recebe o que já toca e nada entra na fila; com busca lenta medida, ou clipe abaixo de 0,25×, o
/// proxy é pedido e passa a ser a resposta quando fica pronto. O estado do intermediário não pede nada: só depois de
/// `edicao_origem_pedir` ele diz "fazendo" e "pronto".
#[test]
fn comandos_do_palco_proxy_por_busca_e_estado_do_intermediario() {
    use crate::edicao::fundo::{self, EstadoFundo, TipoFundo};
    use crate::edicao::origem::{self, EstadoOrigem};
    use crate::edicao::previa::TipoPrevia;
    use crate::edicao::proxy;
    let Some(cap) = fontes::cap() else { return };
    let _ = sonda_do_app();
    let v = cap.versao.clone();
    let c = Caso::novo("palco");
    // fonte só deste teste (a chave da fila de fundo é o SHA-256 da mídia)
    let dir = fontes::pasta_temporaria("palco-fonte");
    let video = dir.join(format!("palco-{}.mp4", if v.starts_with('7') { "a" } else { "b" }));
    let st = crate::tools::command(&cap.binario)
        .args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=s=322x182:r=30:d=4", "-c:v", "libx264", "-preset", "ultrafast", "-g", "30", "-bf", "0", "-pix_fmt", "yuv420p", "-metadata", "comment=palco"])
        .arg(&video)
        .status()
        .unwrap();
    assert!(st.success());
    let (vm, _, tab) = registrar_e_ler(&cap, &c.ws, &video).expect("vídeo do caso");
    let m = json!({ "id": "m1", "tipo": "video", "origem": "caso", "media_id": vm.id.to_string(), "sha256": vm.sha256, "nome": vm.filename, "relativo": vm.relative_path,
        "duracao_s": vm.duration_s, "inicio_s": null, "largura": vm.width, "altura": vm.height, "taxa": null, "vfr": false, "tem_audio": false, "cor": null, "derivacao": null,
        "recebido": { "nativo": null, "obs": "" }, "relogio_desvio_s": null, "gravador": null, "decodificacao": null, "ocultacoes": [] });
    let trecho = |id: &str, fator: f64, efeitos: Value| json!({ "tipo": "trecho", "id": id, "rotulo": id, "desligado": false, "transicao_entrada": null, "efeitos": efeitos, "midia": "m1",
        "inicio_s": tab.tempo(10), "fim_s": tab.tempo(40), "inicio_pts": tab.pts[10], "fim_pts": tab.pts[40], "velocidade": { "tipo": "constante", "fator": fator },
        "audio": { "volume": 1.0, "mudo": true, "manter_tom": true, "silencios": [] } });
    let nitidez = json!([{ "tipo": "nitidez", "id": "e1", "ligado": true, "nome": "", "ativo": null, "quantidade": 1.0 }]);
    let doc = json!({
        "esquema": 2, "esquema_menor": 0, "id": uuid::Uuid::new_v4().to_string(), "titulo": "Palco", "created_at": "2026-11-15T00:00:00Z", "updated_at": "2026-11-15T00:00:00Z",
        "saida": { "largura": 640, "altura": 360, "fps": { "num": 30, "den": 1 }, "auto": false, "cor_fundo": "#000000", "audio": false,
                   "selos": { "velocidade": true, "ampliacao": true, "realce": true },
                   "exportacao": { "predefinicao": "juri", "codificador": "cpu", "legendas": "nenhuma", "pacote": null, "pdf": "a3", "revisado_por": "", "personalizada": null } },
        "midias": [m],
        "principal": [trecho("normal", 1.0, json!([])), trecho("lento", 0.2, json!([])), trecho("nitido", 1.0, nitidez),
            { "tipo": "congelar", "id": "parado", "rotulo": "", "midia": "m1", "instante_s": tab.tempo(20), "instante_pts": tab.pts[20], "duracao_s": 1.0, "efeitos": [] }],
    });
    let cache = c.cache();
    let chave_proxy = fundo::chave(TipoFundo::Proxy, &vm.sha256);

    // ---- proxy por busca medida
    assert!(proxy::clipe_pede(&serde_json::from_value(json!({ "tipo": "constante", "fator": 0.2 })).unwrap()));
    assert!(!proxy::clipe_pede(&serde_json::from_value(json!({ "tipo": "constante", "fator": 0.25 })).unwrap()));
    assert!(proxy::clipe_pede(&serde_json::from_value(json!({ "tipo": "constante", "fator": -1.0 })).unwrap()), "ré");
    assert!(proxy::clipe_pede(&serde_json::from_value(json!({ "tipo": "rampa", "pontos": [{ "t": 0.0, "v": 1.0 }] })).unwrap()));
    // sem medição e com busca rápida: o original, pronto, e nada na fila
    for busca in [None, Some(119.0), Some(proxy::BUSCA_P50_MAX_MS)] {
        let p = proxy::pedir_do_projeto(&c.ws, &cache, &doc, "normal", busca).unwrap_or_else(|e| panic!("{v}: {busca:?}: {e}"));
        assert_eq!((p.tipo, p.pronta), (TipoPrevia::Original, true), "{v}: {busca:?}");
        assert!(fundo::estado(&chave_proxy).is_none(), "{v}: proxy pedido sem busca lenta ({busca:?})");
    }
    for (clipe, busca, erro) in [("normal", Some(-1.0), "item_invalido"), ("normal", Some(f64::NAN), "item_invalido"), ("nao-existe", None, "item_invalido")] {
        let e = proxy::pedir_do_projeto(&c.ws, &cache, &doc, clipe, busca).unwrap_err().to_string();
        assert!(e.contains(erro), "{v}: {clipe} {busca:?}: {e}");
    }
    assert!(fundo::estado(&chave_proxy).is_none());
    // busca lenta medida: o que toca agora (o original) e o proxy na fila; pronto, ele passa a ser a resposta
    let p = proxy::pedir_do_projeto(&c.ws, &cache, &doc, "normal", Some(238.0)).unwrap();
    assert_eq!((p.tipo, p.pronta), (TipoPrevia::Original, true), "{v}: enquanto o proxy não fica pronto");
    assert!(fundo::estado(&chave_proxy).is_some(), "{v}: busca de 238 ms não pediu o proxy");
    assert_eq!(esperar_fundo(&chave_proxy), EstadoFundo::Pronta, "{v}");
    let p = proxy::pedir_do_projeto(&c.ws, &cache, &doc, "normal", Some(238.0)).unwrap();
    assert_eq!((p.tipo, p.pronta), (TipoPrevia::Proxy, true), "{v}");
    assert!(cache.join(&p.relativo).is_file(), "{v}: {}", p.relativo);
    // o clipe lento pede por si, sem medição; o quadro congelado e o trecho a 1× sem medição seguem no original
    let p = proxy::pedir_do_projeto(&c.ws, &cache, &doc, "lento", None).unwrap();
    assert_eq!(p.tipo, TipoPrevia::Proxy, "{v}: clipe a 0,2×");
    for clipe in ["normal", "parado"] {
        assert_eq!(proxy::pedir_do_projeto(&c.ws, &cache, &doc, clipe, None).unwrap().tipo, TipoPrevia::Original, "{v}: {clipe}");
    }

    // ---- estado do intermediário de origem
    let estado = |clipe: &str| origem::estado_do_projeto(&c.ws, &cache, &doc, clipe);
    // perguntar não pede: nada na pasta do intermediário, nem depois de um tempo
    assert_eq!(estado("nitido").unwrap(), None, "{v}");
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(estado("nitido").unwrap(), None, "{v}");
    assert!(!cache.join("origem").exists(), "{v}: `edicao_origem_estado` pôs o intermediário na fila");
    // clipe sem efeito de origem e quadro congelado: não há intermediário a esperar; clipe que não existe é erro
    assert_eq!((estado("normal").unwrap(), estado("parado").unwrap()), (None, None), "{v}");
    assert!(estado("nao-existe").unwrap_err().to_string().contains("item_invalido"));
    // pedido (a tabela do decodificador pode ainda estar na fila de fundo): fazendo, depois pronto, e o estado acompanha
    let t0 = Instant::now();
    let pronto = loop {
        match origem::pedir_do_projeto(&c.ws, &cache, &doc, "nitido") {
            Ok(EstadoOrigem::Pronto { previa, .. }) => break previa,
            Ok(EstadoOrigem::Fazendo { tarefa, .. }) => match estado("nitido").unwrap() {
                Some(EstadoOrigem::Fazendo { tarefa: a_mesma, .. }) => assert_eq!(a_mesma, tarefa, "{v}"),
                // acabou entre uma chamada e a outra
                Some(EstadoOrigem::Pronto { .. }) | None => {}
            },
            Err(e) if e.to_string().contains("tabela_pendente") => {}
            Err(e) => panic!("{v}: {e}"),
        }
        assert!(t0.elapsed() < Duration::from_secs(180), "{v}: intermediário preso");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!((pronto.tipo, pronto.pronta), (TipoPrevia::Origem, true), "{v}");
    match estado("nitido").unwrap() {
        Some(EstadoOrigem::Pronto { previa, trf }) => assert_eq!((previa, trf), (pronto, None), "{v}"),
        outro => panic!("{v}: estado depois de pronto: {outro:?}"),
    }
}

// ------------------------------------------------------------------ verificação adversa da Fase 1 (robustez do app)

/// O texto cru de `falha.log` da tarefa que falhou (a mensagem da fila é só a frase do catálogo).
fn falha_de(c: &Caso, fim: &Estado) -> String {
    match fim {
        Estado::Falhou { log_rel, .. } => std::fs::read_to_string(c.cache().join(log_rel)).unwrap_or_default(),
        _ => String::new(),
    }
}

/// Pastas do caso e do cache com apóstrofo, `%`, `:`, colchetes, chaves, `;`, `,`, `=`, `#`, `&` e `$` (o ffmpeg
/// trata vários deles em lista de concat, filtro e padrão de imagem): o projeto da Fase 1 exporta com pacote e
/// originais, confere, e os quadros compostos e o som são os MESMOS do caso em pasta comum. Proxy, onda e
/// intermediário também saem nessas pastas.
#[test]
fn caso_e_cache_em_pastas_com_caracteres_especiais() {
    use crate::edicao::fundo::{self, EstadoFundo, TipoFundo};
    use crate::edicao::origem::{self, EstadoOrigem};
    use crate::edicao::previa::TipoPrevia;
    let Some(cap) = fontes::cap() else { return };
    let _ = sonda_do_app();
    let v = cap.versao.clone();
    let fonte = fontes::gerar(Fonte::Mp4).unwrap();
    let exportar_em = |c: &Caso, titulo: &str| {
        let m1 = registrar(&cap, &c.ws, &fonte);
        let mut projeto = projeto_da_fase_1(c, &cap, &m1, None);
        projeto["titulo"] = json!(titulo);
        projeto["saida"]["exportacao"]["pacote"] = json!({ "originais": true });
        let (_, analise, fim) = exportar(c, &projeto, "cpu");
        let erros: Vec<_> = analise.problemas.iter().filter(|p| p.nivel == Nivel::Erro && p.codigo != mensagens::codigo::TABELA_PENDENTE).collect();
        assert!(erros.is_empty(), "{v}: {erros:#?}");
        let Estado::AConferir { saida_id } = fim else { panic!("{v}: {titulo}: esperava a conferir, veio {fim:?}\n{}", falha_de(c, &fim)) };
        (m1, cmd::conferir(&c.ws, &saida_id, true).unwrap_or_else(|e| panic!("{v}: {titulo}: edicao_conferir: {e}")))
    };
    let comum = Caso::novo("nomes-comum");
    let (_, s0) = exportar_em(&comum, "Fase 1");
    let c = Caso::novo_em("nomes", "Casos d'água 100% de [a:b] {c};x=y, #1 & $z", "cache d'app 50% [k:v];,=#");
    let (m1, s) = exportar_em(&c, "Laudo: d'água 100% [a]; x=y, #2");
    assert_eq!(s.estado, EstadoSaida::Concluida, "{v}");
    assert_eq!(s.n_quadros, 225, "{v}");
    assert_eq!((&s.composto_sha256, &s.audio_sha256), (&s0.composto_sha256, &s0.audio_sha256), "{v}: o nome da pasta mudou a imagem ou o som");
    assert!(!s.composto_sha256.is_empty() && s.audio_sha256.is_some(), "{v}");
    // o arquivo entregue decodifica nos quadros conferidos
    let mp4 = c.ws.join(&s.arquivo);
    assert_eq!(s.decodificado_sha256, Some(conferir::hash_de_conteudo(&medir::framehash(&cap, &mp4).unwrap())), "{v}");
    // pacote: as somas conferem com o que está na pasta (PDF fora das somas) e os originais vieram
    let pasta = mp4.parent().unwrap();
    let somas = std::fs::read_to_string(pasta.join("SHA256SUMS.txt")).unwrap_or_else(|e| panic!("{v}: SHA256SUMS.txt: {e}"));
    let mut n = 0;
    for l in somas.lines().filter(|l| !l.trim().is_empty()) {
        let (sha, rel) = l.split_once("  ").unwrap_or_else(|| panic!("{v}: linha das somas: {l:?}"));
        let rel = rel.trim_start_matches('*');
        assert_eq!(crate::hashing::sha256::sha256_file(&pasta.join(rel)).unwrap(), sha, "{v}: {rel}");
        n += 1;
    }
    assert!(n >= 4, "{v}: {somas}");
    assert!(somas.contains(&m1.vm.sha256), "{v}: original fora do pacote: {somas}");
    assert!(pasta.join("Relatório.pdf").is_file() && !somas.contains("Relatório.pdf"), "{v}");

    // ---- mídia de prévia nas mesmas pastas: proxy (busca lenta), onda e intermediário de origem
    let cache = c.cache();
    let trecho = json!({ "tipo": "trecho", "id": "nitido", "rotulo": "", "desligado": false, "transicao_entrada": null, "midia": "m1",
        "efeitos": [{ "tipo": "nitidez", "id": "e1", "ligado": true, "nome": "", "ativo": null, "quantidade": 1.0 }],
        "inicio_s": m1.tab.tempo(10), "fim_s": m1.tab.tempo(40), "inicio_pts": m1.tab.pts[10], "fim_pts": m1.tab.pts[40],
        "velocidade": { "tipo": "constante", "fator": 1.0 }, "audio": { "volume": 1.0, "mudo": true, "manter_tom": true, "silencios": [] } });
    let doc = json!({
        "esquema": 2, "esquema_menor": 0, "id": uuid::Uuid::new_v4().to_string(), "titulo": "Palco", "created_at": "2026-11-15T00:00:00Z", "updated_at": "2026-11-15T00:00:00Z",
        "saida": { "largura": 640, "altura": 360, "fps": { "num": 30, "den": 1 }, "auto": false, "cor_fundo": "#000000", "audio": false,
                   "selos": { "velocidade": true, "ampliacao": true, "realce": true },
                   "exportacao": { "predefinicao": "juri", "codificador": "cpu", "legendas": "nenhuma", "pacote": null, "pdf": "a3", "revisado_por": "", "personalizada": null } },
        "midias": [midia_ref("m1", &m1, json!([]))], "principal": [trecho],
    });
    let t0 = Instant::now();
    let espera = |o_que: &str| assert!(t0.elapsed() < Duration::from_secs(240), "{v}: {o_que} preso");
    // proxy pedido pela busca lenta: pronto, com os pts da fonte
    let chave = fundo::chave(TipoFundo::Proxy, &m1.vm.sha256);
    let proxy = loop {
        let p = crate::edicao::proxy::pedir_do_projeto(&c.ws, &cache, &doc, "nitido", Some(500.0)).unwrap_or_else(|e| panic!("{v}: proxy: {e}"));
        if p.tipo == TipoPrevia::Proxy && p.pronta {
            break p;
        }
        if let Some(EstadoFundo::Falhou { mensagem }) = fundo::estado(&chave).map(|t| t.estado) {
            panic!("{v}: proxy falhou: {mensagem}");
        }
        espera("proxy");
        std::thread::sleep(Duration::from_millis(50));
    };
    let pts = |arq: &Path| TabelaQuadros::do_decodificador(&cap, arq, &AtomicBool::new(false), &mut |_| {}).unwrap().pts;
    assert_eq!(pts(&cache.join(&proxy.relativo)).len(), m1.tab.pts.len(), "{v}: quadros do proxy");
    // onda: None enquanto a fila de fundo faz; depois os picos
    let onda = loop {
        match crate::edicao::ondas::pedir(&c.ws, &cache, &m1.vm.sha256) {
            Ok(Some(o)) => break o,
            Ok(None) => {}
            Err(e) => panic!("{v}: onda: {e}"),
        }
        espera("onda");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(!onda.max.is_empty() && onda.max.iter().any(|x| *x != 0), "{v}: onda vazia");
    // intermediário de origem
    let pronto = loop {
        match origem::pedir_do_projeto(&c.ws, &cache, &doc, "nitido") {
            Ok(EstadoOrigem::Pronto { previa, .. }) => break previa,
            Ok(EstadoOrigem::Fazendo { .. }) => {}
            Err(e) if e.to_string().contains("tabela_pendente") => {}
            Err(e) => panic!("{v}: intermediário: {e}"),
        }
        espera("intermediário");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!((pronto.tipo, pronto.pronta), (TipoPrevia::Origem, true), "{v}");
    assert_eq!(pts(&cache.join(&pronto.relativo)).len(), 30, "{v}: quadros do intermediário");
}

// ------------------------------------------------------------------ fecho da Fase 2: rastreio de ponta a ponta

/// Fonte do rastreio: a TS de 59,94 qps com início fora de zero; o alvo é o quadrado vermelho da matriz, que anda na
/// horizontal (`fontes::marca_esperada`) sobre faixas cinza: nenhum outro pixel vermelho no quadro.
const R_FONTE: Fonte = Fonte::Ts5994;
/// Trecho `[j_in, j_out)`, âncora do perito, até onde a corrida segue e a segunda âncora: entre o fim da corrida e a
/// segunda âncora a caixa sai da interpolação (trecho fraco). A ocultação vale da primeira à segunda âncora.
const R_TRECHO: (usize, usize) = (4, 70);
const R_ANCORA: usize = 8;
const R_ATE: usize = 40;
const R_SEGUNDA: usize = 56;
/// Contexto em volta do quadrado na caixa marcada (fração do quadro).
const R_FOLGA: f64 = 0.02;

/// Caixa marcada pelo perito no quadro desenhado `n`: o quadrado com a folga.
fn caixa_do_alvo(n: u32) -> [f64; 4] {
    let [x, y, w, h] = fontes::marca_esperada(R_FONTE, n);
    [x - R_FOLGA, y - R_FOLGA, w + 2.0 * R_FOLGA, h + 2.0 * R_FOLGA]
}

fn iou(a: [f64; 4], b: [f64; 4]) -> f64 {
    let ix = ((a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0])).max(0.0);
    let iy = ((a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1])).max(0.0);
    ix * iy / (a[2] * a[3] + b[2] * b[3] - ix * iy)
}

/// Projeto do rastreio: o trecho, o rastreio manual com a âncora do perito e a ocultação da mídia (sólido) seguindo-o.
fn projeto_do_rastreio(f: &Fonte1) -> Value {
    let tab = &f.tab;
    let fps = tab.taxa_nominal();
    let ancora = crate::edicao::testes::aceite::ancora(tab.tempo(R_ANCORA), Some(tab.pts[R_ANCORA]), caixa_do_alvo(f.numeros[R_ANCORA]));
    let oc = crate::edicao::testes::aceite::ocultar("o1", "r1", "solido", json!({ "ativo": { "de": tab.tempo(R_ANCORA), "ate": tab.tempo(R_SEGUNDA) } }));
    let p = crate::edicao::testes::aceite::projeto(
        f.vm.width.unwrap() as u32,
        f.vm.height.unwrap() as u32,
        (fps.num, fps.den),
        vec![midia_ref("m1", f, json!([oc]))],
        vec![crate::edicao::testes::aceite::trecho("c1", "m1", tab, R_TRECHO.0, R_TRECHO.1, 1.0, json!([]))],
        json!({ "rastreios": [crate::edicao::testes::aceite::rastreio("r1", Some("m1"), "fonte", vec![ancora])] }),
    );
    let mut p = serde_json::to_value(&p).unwrap();
    p["id"] = json!(uuid::Uuid::new_v4().to_string());
    p["titulo"] = json!("Rastreio");
    p
}

/// O que a tela faz: pede "Seguir" pelo corpo do comando (`iniciar`), espera o evento final e aplica o fim por
/// `alterar()` (ponteiro do denso e das silhuetas, origem assistida); depois o perito marca a segunda âncora.
fn seguir_e_aplicar(
    c: &Caso,
    doc: &mut Value,
    f: &Fonte1,
    iniciar: &mut dyn FnMut(&Path, &Value, &crate::edicao::rastreio::corrida::PedidoSeguir, crate::edicao::rastreio::corrida::Emissor) -> crate::error::Result<String>,
) -> crate::edicao::rastreio::corrida::FimRastreio {
    use crate::edicao::rastreio::corrida::{EventoRastreio, PedidoSeguir};
    use crate::edicao::rastreio::{EscolhaMotor, SentidoPedido};
    let tab = &f.tab;
    let ancora = serde_json::from_value(rastreio_mut(doc, "r1")["ancoras"][0].clone()).unwrap();
    let pedido = PedidoSeguir { rastreio_id: "r1".into(), clipe_id: "c1".into(), ancora, sentido: SentidoPedido::Frente, ate_s: Some(tab.tempo(R_ATE)), motor: EscolhaMotor::Auto };
    let eventos: Arc<std::sync::Mutex<Vec<EventoRastreio>>> = Arc::default();
    let e2 = eventos.clone();
    let _tarefa = iniciar(&c.cache(), doc, &pedido, Box::new(move |e| e2.lock().unwrap().push(e))).expect("edicao_rastreio_seguir");
    let t = Instant::now();
    let fim = loop {
        if let Some(e) = eventos.lock().unwrap().iter().find(|e| !matches!(e, EventoRastreio::Avanco(_))) {
            break e.clone();
        }
        assert!(t.elapsed() < Duration::from_secs(900), "a corrida não acabou");
        std::thread::sleep(Duration::from_millis(10));
    };
    let EventoRastreio::Fim(fim) = fim else { panic!("{fim:?}") };
    let r = rastreio_mut(doc, "r1");
    r["denso"] = serde_json::to_value(fim.denso.as_ref().expect("denso novo")).unwrap();
    r["silhuetas"] = serde_json::to_value(&fim.silhuetas).unwrap();
    r["origem"] = json!({ "tipo": "assistido" });
    r["ancoras"].as_array_mut().unwrap().push(crate::edicao::testes::aceite::ancora(tab.tempo(R_SEGUNDA), Some(tab.pts[R_SEGUNDA]), caixa_do_alvo(f.numeros[R_SEGUNDA])));
    fim
}

/// Quadros fracos esperados (slot do trecho → como), pela avaliação do rastreio do documento sobre o denso (B24).
fn fracos_esperados(doc: &Value, d: &crate::edicao::rastreio::ArquivoDenso, tab: &TabelaQuadros) -> std::collections::BTreeMap<usize, crate::edicao::conferencia::ComoFraco> {
    use crate::edicao::conferencia::ComoFraco;
    use crate::edicao::rastreio::avaliar::{avaliar, Como, Instante};
    let p = armazem::tipado(doc).unwrap();
    let r = p.rastreios.iter().find(|r| r.id == "r1").unwrap();
    (R_ANCORA..=R_SEGUNDA)
        .filter_map(|j| {
            let a = avaliar(r, Some(d), Instante { t: tab.tempo(j), pts: Some(tab.pts[j]) }).expect("quadro coberto");
            let como = if a.incerto { Some(ComoFraco::Incerto) } else if a.como == Como::Interpolado { Some(ComoFraco::Interpolado) } else { None };
            como.map(|c| (j - R_TRECHO.0, c))
        })
        .collect()
}

/// Rastreio de ponta a ponta (fecho da Fase 2), nos dois ffmpeg: marcar uma caixa na TS que começa fora de zero,
/// seguir pelo corpo do comando real (`edicao_rastreio_seguir`) com a mancha, com o VitTrack (o `auto` sem GPU) e, com
/// a placa livre, com o EdgeTAM na GPU; ocultar seguindo o denso; exportar pela fila; conferir. No arquivo exportado:
/// o quadrado vermelho coberto em cada quadro em que a ocultação vale (0 pixel vermelho; antes e depois dela, à
/// vista); o `tecnico.json` com a corrida (motor, modelo, provedor) e o SHA-256 do denso; a conferência
/// (`edicao_conferencia_trechos`) com o intervalo da ocultação e os trechos fracos certos.
#[test]
fn rastreio_seguir_ocultar_exportar_conferir() {
    use crate::commands::edicao_rastreio_commands as rcmd;
    use crate::commands::settings_commands::AiSettings;
    use crate::edicao::conferencia::ComoFraco;
    use crate::edicao::rastreio::{arquivo, corrida, EscolhaMotor, Rastreador};
    let Some(cap) = fontes::cap() else { return };
    let cap_app = sonda_do_app();
    assert_eq!(cap_app.sha256, cap.sha256, "o ffmpeg do app é o dos testes");
    let v = cap.versao.clone();
    let c = Caso::novo("rastreio");
    let f = registrar(&cap, &c.ws, &fontes::gerar(R_FONTE).unwrap());
    assert!(f.tab.pts[0] > 0, "{v}: a fonte começa fora de zero");
    let ai_ia = crate::ia::testes::ambiente();
    let mut motores: Vec<(&str, Box<dyn FnMut(&Path, &Value, &corrida::PedidoSeguir, corrida::Emissor) -> crate::error::Result<String>>)> = Vec::new();
    // a mancha (pedida explícita: os modelos podem estar desligados)
    let ws = c.ws.clone();
    motores.push(("mancha", Box::new(move |cache, doc, p, emitir| rcmd::seguir(&ws, cache, &AiSettings::default(), doc, &corrida::PedidoSeguir { motor: EscolhaMotor::Mancha, ..p.clone() }, emitir))));
    // o `auto` com o onnxruntime dos testes e sem GPU pronta = VitTrack
    if let Some(ai) = crate::ia::testes::ou_dispensa(ai_ia.clone(), "rastreio_seguir_ocultar_exportar_conferir", "a lib dos testes") {
        let ws = c.ws.clone();
        motores.push(("vittrack", Box::new(move |cache, doc, p, emitir| rcmd::seguir(&ws, cache, &ai, doc, p, emitir))));
    }
    // EdgeTAM na GPU: o motor do `auto` com a GPU pronta, criado com as sessões de GPU dos testes
    let gpu = crate::ia::testes::ou_dispensa_gpu(crate::ia::gpu::placa(), "rastreio_seguir_ocultar_exportar_conferir", "placa NVIDIA")
        .and_then(|_| crate::ia::testes::ou_dispensa_gpu(crate::ia::testes::pacote_gpu(), "rastreio_seguir_ocultar_exportar_conferir", "o pacote de GPU dos testes"));
    let grafos: Option<Vec<PathBuf>> = crate::edicao::rastreio::edgetam::ARQUIVOS.iter().map(|a| crate::ia::testes::modelo(&format!("rastreio/edgetam_video/{}", a.nome), a.sha256)).collect();
    let grafos = crate::ia::testes::ou_dispensa(grafos, "rastreio_seguir_ocultar_exportar_conferir", "os grafos do EdgeTAM");
    let _trava_gpu = gpu.as_ref().map(|_| crate::ia::testes::trava_gpu());
    if let (Some(pacote), Some(g), Some(ai)) = (gpu, grafos, ai_ia) {
        let ws = c.ws.clone();
        motores.push((
            "edgetam",
            Box::new(move |cache, doc, p, emitir| {
                let mut criar = |_: &AiSettings, _: EscolhaMotor| -> crate::error::Result<Box<dyn Rastreador>> {
                    let s = |i: usize, aquecer| {
                        let op = crate::ia::Opcoes { preferencia: crate::ia::Preferencia::Gpu, aquecer, ..Default::default() };
                        crate::ia::criar_sessao_com(&pacote.lib, Some(&pacote), crate::ia::Modelo::Arquivo(&g[i]), &op)
                    };
                    Ok(Box::new(crate::edicao::rastreio::edgetam::EdgeTam::de_sessoes(s(0, Some([1, 3, 1024, 1024]))?, s(1, None)?, s(2, None)?, s(3, None)?)))
                };
                corrida::pedir_com(&ws, cache, &ai, doc, p, emitir, &mut criar)
            }),
        ));
    }
    let _vez = corrida::testes::vez();
    let tab = f.tab.clone();
    let fps = tab.taxa_nominal();
    let slot_s = |k: usize| k as f64 * fps.den as f64 / fps.num as f64;
    for (nome, iniciar) in motores.iter_mut() {
        let onde = format!("{v} {nome}");
        let mut doc = projeto_do_rastreio(&f);
        let fim = seguir_e_aplicar(&c, &mut doc, &f, iniciar.as_mut());
        // o denso, pelo corpo de `edicao_rastreio_ler`: a corrida da âncora até onde o perito pediu, no alvo
        let ptr = fim.denso.clone().unwrap();
        let pasta = armazem::pasta_projeto(&c.ws, doc["id"].as_str().unwrap());
        let d = arquivo::ler(&pasta, "r1", &ptr.sha256).unwrap();
        let cs = arquivo::corridas(&d).unwrap();
        assert_eq!((cs.len(), serde_json::to_value(cs[0].motor.motor).unwrap()), (1, json!(nome)), "{onde}");
        assert_eq!((d.pts.first(), d.pts.last() <= Some(&tab.pts[R_ATE])), (Some(&tab.pts[R_ANCORA]), true), "{onde}");
        let (mut seguidos, mut pior) = (0, 1.0f64);
        for (i, &p) in d.pts.iter().enumerate() {
            if d.estado[i] == crate::edicao::rastreio::ESTADO_AUSENTE {
                continue;
            }
            let j = tab.indice_do_pts(p).unwrap();
            let [x, y, w, h] = fontes::marca_esperada(R_FONTE, f.numeros[j]);
            let alvo = iou([d.x[i], d.y[i], d.w[i], d.h[i]], caixa_do_alvo(f.numeros[j])).max(iou([d.x[i], d.y[i], d.w[i], d.h[i]], [x, y, w, h]));
            pior = pior.min(alvo);
            seguidos += 1;
        }
        eprintln!("{onde}: {} linhas, {seguidos} com o alvo, parou {:?}, pior IoU {pior:.3}", d.pts.len(), fim.parou);
        assert!(seguidos >= 10 && pior >= 0.5, "{onde}: {seguidos} quadros seguidos, pior IoU {pior:.3}");

        // exportar pela fila e conferir
        let (doc, analise, estado) = exportar(&c, &doc, "cpu");
        let erros: Vec<_> = analise.problemas.iter().filter(|p| p.nivel == Nivel::Erro && p.codigo != mensagens::codigo::TABELA_PENDENTE).collect();
        assert!(erros.is_empty(), "{onde}: {erros:?}");
        let Estado::AConferir { saida_id } = estado else { panic!("{onde}: esperava a conferir, veio {estado:?}") };
        let trechos = rcmd::conferencia_trechos(&c.ws, &c.cache(), &saida_id).expect("edicao_conferencia_trechos");
        let s = cmd::conferir(&c.ws, &saida_id, true).expect("edicao_conferir");
        let mp4 = c.ws.join(&s.arquivo);

        // o quadrado coberto em cada quadro da ocultação; à vista antes e depois
        let lidos = medir::quadros_de_origem(&cap, &mp4).unwrap();
        let quadros = medir::quadros(&cap, &mp4).unwrap();
        assert_eq!(lidos.len(), R_TRECHO.1 - R_TRECHO.0, "{onde}");
        for (k, q) in quadros.iter().enumerate() {
            let j = R_TRECHO.0 + k;
            assert_eq!(lidos[k], f.numeros[j], "{onde}: slot {k}");
            let vermelhos = crate::edicao::testes::aceite::vermelho(q);
            if (R_ANCORA..=R_SEGUNDA).contains(&j) {
                assert_eq!(vermelhos, 0, "{onde}: quadro {j}: {vermelhos} pixels do alvo à vista");
            } else {
                assert!(vermelhos > 300, "{onde}: quadro {j} fora da ocultação: {vermelhos} pixels vermelhos");
            }
        }

        // o `tecnico.json`: a corrida (motor, modelo, provedor) e o SHA-256 do denso
        let tecnico = ler_json(&c.ws.join(s.tecnico_rel.as_ref().unwrap()));
        let r = tecnico["feito"]["rastreios"].as_array().unwrap().iter().find(|r| r["id"] == "r1").unwrap_or_else(|| panic!("{onde}: {}", tecnico["feito"]["rastreios"]));
        assert_eq!(r["denso_sha256"], json!(ptr.sha256), "{onde}");
        let corrida_tec = &r["corridas"][0];
        assert_eq!((&corrida_tec["motor"], &corrida_tec["provedor"]), (&json!(nome), &json!(if *nome == "edgetam" { "cuda" } else { "cpu" })), "{onde}");
        assert_eq!(corrida_tec["modelo"], serde_json::to_value(&cs[0].motor.modelo).unwrap(), "{onde}");
        let ia = tecnico["feito"]["ia"].as_array().unwrap();
        if *nome == "mancha" {
            assert!(ia.is_empty(), "{onde}: a correlação não é modelo de IA: {ia:?}");
        } else {
            let m = ia.iter().find(|m| m["modelo"] == json!(cs[0].motor.modelo.nome)).unwrap_or_else(|| panic!("{onde}: {ia:?}"));
            assert_eq!(m["provedor"], corrida_tec["provedor"], "{onde}");
            assert_eq!(m["arquivos"].as_array().unwrap().len(), cs[0].motor.modelo.arquivos.len(), "{onde}");
        }

        // a conferência: o intervalo da ocultação no arquivo e os fracos certos
        assert_eq!(trechos.len(), 1, "{onde}: {trechos:?}");
        let t = &trechos[0];
        let (k0, k1) = (R_ANCORA - R_TRECHO.0, R_SEGUNDA - R_TRECHO.0);
        assert!((t.efeito.as_str(), (t.de_s - slot_s(k0)).abs() < 1e-6, (t.ate_s - slot_s(k1 + 1)).abs() < 1e-6) == ("o1", true, true), "{onde}: {t:?}");
        let esperado = fracos_esperados(&doc, &d, &tab);
        assert!(esperado.values().any(|c| *c == ComoFraco::Interpolado), "{onde}: sem trecho interpolado");
        let mut achado = std::collections::BTreeMap::new();
        for fr in &t.fracos {
            for k in k0..=k1 {
                let ts = slot_s(k);
                if ts >= fr.de_s - 1e-6 && ts < fr.ate_s - 1e-6 {
                    assert!(achado.insert(k, fr.como).is_none(), "{onde}: slot {k} em dois trechos fracos");
                }
            }
        }
        assert_eq!(achado, esperado, "{onde}: fracos {:?}", t.fracos);
    }
}

/// Defeito de 17/11 (caso PINHO, "sem registro do programa usado: 62"): o "Simplificar" trocava o denso por âncoras e
/// o registro do programa que seguiu o alvo sumia (e o denso, órfão, sai ao fechar). Nos dois ffmpeg, com a mancha e
/// (com a lib dos testes) o VitTrack: seguir pelo corpo do comando; "Simplificar" pelo corpo de
/// `edicao_rastreio_editar`, aplicado como a tela aplica; fechar (o denso, que nada cita, sai); exportar. O
/// `tecnico.json` traz a corrida (motor, modelo) em `corridas_do_programa` e o modelo em `feito.ia`; o PDF não diz
/// "sem registro" e lista a corrida no anexo H; resumo de IA, anexo H e modelos iguais aos da exportação do mesmo
/// projeto com o denso (a referência, exportada depois com o arquivo devolvido).
#[test]
fn simplificar_guarda_o_programa_ate_o_pdf() {
    use crate::commands::edicao_rastreio_commands as rcmd;
    use crate::commands::settings_commands::AiSettings;
    use crate::edicao::modelo::{Caixa, Chave, ModoPdf};
    use crate::edicao::rastreio::arquivo::{self, OpRastreio};
    use crate::edicao::rastreio::{corrida, EscolhaMotor};
    use crate::edicao::relatorio::juri;
    let Some(cap) = fontes::cap() else { return };
    let v = cap.versao.clone();
    let c = Caso::novo("simplificar");
    let f = registrar(&cap, &c.ws, &fontes::gerar(R_FONTE).unwrap());
    let mut motores: Vec<(&str, AiSettings, EscolhaMotor)> = vec![("mancha", AiSettings::default(), EscolhaMotor::Mancha)];
    if let Some(ai) = crate::ia::testes::ou_dispensa(crate::ia::testes::ambiente(), "simplificar_guarda_o_programa_ate_o_pdf", "a lib dos testes") {
        motores.push(("vittrack", ai, EscolhaMotor::Vittrack));
    }
    let _vez = corrida::testes::vez();
    // exporta, confere e monta os dados e o texto do PDF como o pacote faz
    let exportar_pdf = |doc: &Value, onde: &str| -> (Value, Value, String) {
        let (doc, _, estado) = exportar(&c, doc, "cpu");
        let Estado::AConferir { saida_id } = estado else { panic!("{onde}: esperava a conferir, veio {estado:?}") };
        let s = cmd::conferir(&c.ws, &saida_id, true).expect("edicao_conferir");
        let tec = ler_json(&c.ws.join(s.tecnico_rel.as_ref().unwrap()));
        let dados = juri::dados(&tec, &[], &[]).unwrap_or_else(|e| panic!("{onde}: dados do júri: {e}"));
        let entrada = juri::entrada(&tec, dados.clone(), &[], &serde_json::to_vec(&doc).unwrap(), ModoPdf::A3).unwrap();
        let saida = crate::relatorio_pdf::gerar(&entrada).unwrap_or_else(|e| panic!("{onde}: PDF: {e}"));
        let pdf = fontes::pasta_temporaria("simplificar-pdf").join(format!("Relatório-{}.pdf", onde.replace(' ', "-")));
        std::fs::write(&pdf, &saida.pdf).unwrap();
        let texto = texto_do_pdf(&pdf).unwrap_or_else(|| {
            assert!(std::env::var_os("SICRO_EXIGE_FFMPEG").is_none(), "pdftotext ausente: o PDF não foi aberto por fora");
            crate::relatorio_pdf::texto(&entrada).unwrap()
        });
        (tec, dados, texto.split_whitespace().collect::<Vec<_>>().join(" "))
    };
    for (nome, ai, motor) in motores {
        let onde = format!("{v} {nome}");
        let mut doc = projeto_do_rastreio(&f);
        let ws = c.ws.clone();
        let mut iniciar = |cache: &Path, doc: &Value, p: &corrida::PedidoSeguir, emitir: corrida::Emissor| rcmd::seguir(&ws, cache, &ai, doc, &corrida::PedidoSeguir { motor, ..p.clone() }, emitir);
        let fim = seguir_e_aplicar(&c, &mut doc, &f, &mut iniciar);
        let pasta = armazem::pasta_projeto(&c.ws, doc["id"].as_str().unwrap());
        let sha_denso = fim.denso.as_ref().unwrap().sha256.clone();
        let cs = arquivo::corridas(&arquivo::ler(&pasta, "r1", &sha_denso).unwrap()).unwrap();
        assert_eq!((cs.len(), serde_json::to_value(cs[0].motor.motor).unwrap()), (1, json!(nome)), "{onde}");
        let (doc_denso, bytes_denso) = (doc.clone(), std::fs::read(arquivo::caminho(&pasta, "r1", &sha_denso)).unwrap());

        // "Simplificar" (ocultação: sem média) e o que a tela grava (`editarRastreio`, `ancorasDoSimplificar`)
        let op = OpRastreio::Simplificar { tolerancia: 0.04, suavizar_s: None };
        let ed = arquivo::editar(&c.ws, &c.cache(), &doc, "r1", &op).expect("edicao_rastreio_editar");
        assert!(ed.denso.is_none(), "{onde}");
        let r = rastreio_mut(&mut doc, "r1");
        let perito: Vec<Chave<Caixa>> = serde_json::from_value(r["ancoras"].clone()).unwrap();
        let ancoras = ed.ancoras.expect("âncoras do Simplificar");
        let programa: Vec<f64> = ancoras.iter().filter(|k| !perito.iter().any(|p| p.t == k.t && p.v == k.v)).map(|k| k.t).collect();
        assert!(programa.len() >= 2, "{onde}: {programa:?}");
        (r["ancoras"], r["ancoras_do_programa"], r["denso"], r["silhuetas"]) = (json!(ancoras), json!(programa), Value::Null, Value::Null);
        if let Some(cs) = ed.corridas_do_programa.filter(|cs| !cs.is_empty()) {
            r["corridas_do_programa"] = json!(cs);
        }
        // fechar: o denso trocado não é citado por nenhum estado e sai
        armazem::salvar(&c.ws, &doc).unwrap();
        armazem::limpar_orfaos(&c.ws, &armazem::tipado(&doc).unwrap(), &[]).unwrap();
        assert!(arquivo::ler(&pasta, "r1", &sha_denso).is_err(), "{onde}: o denso órfão saiu");

        let (tec, simplificado, pdf) = exportar_pdf(&doc, &format!("{onde} simplificado"));
        let r = tec["feito"]["rastreios"].as_array().unwrap().iter().find(|r| r["id"] == "r1").unwrap();
        assert_eq!((&r["denso_sha256"], &r["corridas"]), (&Value::Null, &json!([])), "{onde}");
        let registro = &r["corridas_do_programa"][0];
        assert_eq!((&registro["motor"], &registro["modelo"]), (&json!(nome), &serde_json::to_value(&cs[0].motor.modelo).unwrap()), "{onde}: {r}");
        let ia: Vec<&str> = tec["feito"]["ia"].as_array().unwrap().iter().map(|m| m["modelo"].as_str().unwrap()).collect();
        assert_eq!(ia, if nome == "mancha" { vec![] } else { vec![cs[0].motor.modelo.nome.as_str()] }, "{onde}");
        // o PDF: o programa registrado, nunca "sem registro"
        assert!(!pdf.contains("sem registro do programa usado") && !pdf.contains("sem registro legível"), "{onde}: o PDF perdeu o programa");
        let marca = if nome == "mancha" { "correlação, sem inteligência artificial".to_string() } else { cs[0].motor.modelo.nome.clone() };
        assert!(pdf.contains(&marca), "{onde}: PDF sem a corrida ({marca})");
        if nome != "mancha" {
            assert!(pdf.contains("Alvos acompanhados por programa de inteligência artificial a partir de marcação do perito: 1."), "{onde}");
        }
        // o que se diz do programa é o que se diria com o denso
        armazem::gravar_atomico(&arquivo::caminho(&pasta, "r1", &sha_denso), &bytes_denso).unwrap();
        let (_, com_denso, _) = exportar_pdf(&doc_denso, &format!("{onde} denso"));
        for campo in ["ia_resumo", "modelos_ia"] {
            assert_eq!(simplificado[campo], com_denso[campo], "{onde}: {campo}");
        }
        assert_eq!(simplificado["rastreios"][0]["corridas"], com_denso["rastreios"][0]["corridas"], "{onde}: anexo H");
    }
}
