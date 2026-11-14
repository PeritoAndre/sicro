//! Comandos da Edição, Fase 0 (arquitetura §5.12). Dono: P0.7. Quem registra no `invoke_handler` é o P0.1, ao fechar a fase.
//! `workspace_path` = raiz do caso. Caminhos `relativo` das respostas: ao cache da Edição do caso
//! (`app_cache_dir()/edicao/<id do caso>/`), salvo onde o tipo diz outra coisa.
//! Fora daqui: `edicao_diagnostico_salvar` (P0.9, arquivo próprio), `edicao_origem_pedir` e `edicao_onda` (Fase 1).
//! Todo erro sai por `mensagens::erro_de_comando` (`"<código>: <frase>"`); o trabalho roda em `spawn_blocking`
//! (o `PR_SET_PDEATHSIG` dos filhos vale enquanto a thread que os criou viver).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::edicao::armazem::{self, ProjetoAberto, ResumoProjeto, Salvo, UsoMidia};
use crate::edicao::camada::texto::{self, Atlas, PedidoSprite, Sprite};
use crate::edicao::executar::{self, abrir_banco, sha_valido};
use crate::edicao::ffmpeg::sonda::{self, Capacidades, ResumoCapacidades};
use crate::edicao::fila::{self, Estado, PedidoRender, SaidaRegistrada, Tarefa, TipoTarefa};
use crate::edicao::fundo::{self, TarefaFundo, TipoFundo};
use crate::edicao::mensagens::{self, codigo};
use crate::edicao::modelo::{Analise, EstiloTexto, LinhaResolvida, MidiaRef, Problema, Racional, TipoMidia};
use crate::edicao::motor::{Alvo, FonteInfo};
use crate::edicao::previa::{self, MidiaInfo, PreviaMidia, TipoPrevia};
use crate::edicao::quadros::{AlvoQuadro, OrigemTabela, QuadroEm, TabelaCompacta, TabelaQuadros};
use crate::edicao::validar;
use crate::error::{Result, SicroError};
use crate::models::VideoMedia;
use crate::workspace::manifest::Manifest;
use crate::workspace::paths;

/// Cabeçalhos de `edicao_gravacao_salvar` (o corpo são os bytes crus). Valores em UTF-8 com escape de URL
/// (`encodeURIComponent`): cabeçalho HTTP não leva acento.
pub const CABECALHO_WORKSPACE: &str = "workspace_path";
pub const CABECALHO_PROJETO: &str = "projeto_id";
pub const CABECALHO_MIME: &str = "mime";

/// Resposta de `edicao_quadro_final`: BMP nomeado pelo hash do pedido (relativo ao cache) e o slot renderizado.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QuadroFinal {
    pub relativo: String,
    pub q: u64,
}

fn erro(t: impl Into<String>) -> SicroError {
    SicroError::Validation(t.into())
}

/// Roda `f` numa thread que dura a chamada e passa o erro pelo catálogo.
async fn em_fundo<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| SicroError::Workspace(format!("edição: {e}")))
        .and_then(|r| r)
        .map_err(mensagens::erro_de_comando)
}

fn app_cache(app: &AppHandle) -> Result<PathBuf> {
    app.path().app_cache_dir().map_err(|e| SicroError::Filesystem(format!("cache dir: {e}")))
}

/// `app_cache_dir()/edicao/<id do caso>/` (criada).
pub(crate) fn cache_do_caso(app_cache: &Path, ws: &Path) -> Result<PathBuf> {
    let occ = Manifest::read(ws)?.occurrence_id;
    let c = paths::dir_edicao_cache(app_cache, &occ);
    std::fs::create_dir_all(&c)?;
    Ok(c)
}

// ---------------------------------------------------------------------------------------------- projeto

/// Projetos do caso; arquivo ilegível aparece com `problema`.
#[tauri::command]
pub async fn edicao_listar(workspace_path: String) -> Result<Vec<ResumoProjeto>> {
    em_fundo(move || armazem::listar(Path::new(&workspace_path))).await
}

/// Abre o projeto (importa do v1 se for o caso) e devolve o documento como está no disco.
#[tauri::command]
pub async fn edicao_abrir(app: AppHandle, workspace_path: String, projeto_id: String) -> Result<ProjetoAberto> {
    let raiz = app_cache(&app)?;
    em_fundo(move || abrir(Path::new(&workspace_path), &projeto_id, &raiz)).await
}

pub(crate) fn abrir(ws: &Path, projeto_id: &str, app_cache: &Path) -> Result<ProjetoAberto> {
    let mut a = armazem::abrir(ws, projeto_id)?;
    a.raiz_cache = cache_do_caso(app_cache, ws)?.to_string_lossy().into_owned();
    Ok(a)
}

/// Grava o documento como chegou e devolve o `updated_at` gravado.
#[tauri::command]
pub async fn edicao_salvar(workspace_path: String, projeto: Value) -> Result<Salvo> {
    em_fundo(move || armazem::salvar(Path::new(&workspace_path), &projeto)).await
}

/// Apaga o projeto e o cache dele; as exportações ficam.
#[tauri::command]
pub async fn edicao_excluir(app: AppHandle, workspace_path: String, projeto_id: String) -> Result<()> {
    let raiz = app_cache(&app)?;
    em_fundo(move || excluir(Path::new(&workspace_path), &projeto_id, &raiz, &fila::projeto_ocupado)).await
}

/// `ocupado(caso, projeto)`: há tarefa de render por rodar desse projeto (a limpeza apagaria `render/<tarefa>/`).
pub(crate) fn excluir(ws: &Path, projeto_id: &str, app_cache: &Path, ocupado: &dyn Fn(&str, &str) -> bool) -> Result<()> {
    let cache = cache_do_caso(app_cache, ws)?;
    let caso = cache.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if ocupado(&caso, projeto_id) {
        return Err(erro(format!("na_fila: {projeto_id}")));
    }
    armazem::excluir(ws, projeto_id, Some(&cache))
}

/// Duplica o projeto e devolve o resumo da cópia.
#[tauri::command]
pub async fn edicao_duplicar(workspace_path: String, projeto_id: String) -> Result<ResumoProjeto> {
    em_fundo(move || armazem::duplicar(Path::new(&workspace_path), &projeto_id)).await
}

/// Valida e resolve a linha do tempo do documento da tela (não grava nada). Documento que não lê tipado
/// não é `Err`: volta `Analise` com um problema `esquema` (ou `item_invalido`) e a linha vazia.
#[tauri::command]
pub async fn edicao_analisar(app: AppHandle, workspace_path: String, projeto: Value) -> Result<Analise> {
    let raiz = app_cache(&app)?;
    em_fundo(move || analisar(Path::new(&workspace_path), &projeto, &raiz)).await
}

pub(crate) fn analisar(ws: &Path, projeto: &Value, app_cache: &Path) -> Result<Analise> {
    let mut problemas = validar::esquema(projeto);
    let p = match armazem::tipado(projeto) {
        // o id vira pasta (rastreios, `.trf`): texto da tela não passa sem conferir
        Ok(p) if !armazem::id_valido(&p.id) => {
            problemas.push(mensagens::problema(codigo::ESQUEMA, None, None));
            return Ok(analise_vazia(projeto, problemas));
        }
        Ok(p) if !validar::tem_erro(&problemas) => p,
        Ok(_) => return Ok(analise_vazia(projeto, problemas)),
        Err(e) => {
            if problemas.is_empty() {
                problemas.push(problema_de_leitura(&e));
            }
            return Ok(analise_vazia(projeto, problemas));
        }
    };
    let cache = cache_do_caso(app_cache, ws)?;
    let prep = executar::preparar(ws, &p, &cache, false)?;
    Ok(validar::analisar(&p, &prep.contexto(), None))
}

/// `esquema` ou `item_invalido` (com o texto do serde) da leitura tipada.
fn problema_de_leitura(e: &SicroError) -> Problema {
    match e {
        SicroError::Validation(m) if m.starts_with(codigo::ITEM_INVALIDO) => {
            let detalhe = m.split_once(": ").map(|(_, d)| d);
            mensagens::problema(codigo::ITEM_INVALIDO, None, detalhe)
        }
        _ => mensagens::problema(codigo::ESQUEMA, None, None),
    }
}

fn analise_vazia(projeto: &Value, problemas: Vec<Problema>) -> Analise {
    let fps = projeto
        .pointer("/saida/fps")
        .and_then(|v| serde_json::from_value::<Racional>(v.clone()).ok())
        .filter(|f| f.num > 0 && f.den > 0)
        .unwrap_or(Racional { num: 30, den: 1 });
    Analise {
        problemas,
        linha: LinhaResolvida { fps, total_q: 0, clipes: Vec::new(), livres: Vec::new() },
        midias: Vec::new(),
        prender: Vec::new(),
        saida_auto: None,
    }
}

// ---------------------------------------------------------------------------------------------- mídia do caso

/// Vídeo do caso pelo SHA-256: registro, `FonteInfo` (tabela que houver) e capacidades. Pede a tabela do
/// decodificador à fila de fundo quando só há a de pacotes.
pub(crate) fn fonte_do_caso(ws: &Path, sha256: &str, cache: &Path) -> Result<(VideoMedia, FonteInfo, Arc<Capacidades>)> {
    if !sha_valido(sha256) {
        return Err(erro(format!("{}: {sha256}", codigo::SHA256_INVALIDO)));
    }
    let (conn, occ) = abrir_banco(ws)?;
    let vm = crate::database::repositories::video_repo::find_media_by_sha256(&conn, &occ, sha256)?
        .ok_or_else(|| erro(format!("{}: {sha256}", codigo::MIDIA_AUSENTE)))?;
    let caminho = paths::resolver_relativo(ws, &vm.relative_path);
    if !caminho.is_file() {
        return Err(erro(format!("{}: {}", codigo::MIDIA_AUSENTE, vm.relative_path)));
    }
    let cap = sonda::capacidades()?;
    let tab = TabelaQuadros::obter_com(&cap, cache, sha256, &caminho)?;
    // o ffprobe de `fonte_info` só roda de novo quando a tabela muda (a tela pede quadro a quadro)
    let fonte = executar::fonte_info_memo(&cap, &caminho, sha256, tab, None)?;
    if fonte.quadros.origem == OrigemTabela::Pacotes {
        executar::pedir_tabela(&cap, cache, sha256, &caminho, &fonte.conteiner, false);
    }
    Ok((vm, fonte, cap))
}

/// Pede a cópia ou o proxy à fila de fundo (o original nunca entra).
fn pedir_previa(cap: &Arc<Capacidades>, cache: &Path, fonte: &FonteInfo, p: &PreviaMidia) {
    let tipo = match p.tipo {
        TipoPrevia::Copia => TipoFundo::CopiaPrevia,
        TipoPrevia::Proxy => TipoFundo::Proxy,
        TipoPrevia::Original | TipoPrevia::Origem => return,
    };
    let (cap, cache, fonte) = (cap.clone(), cache.to_path_buf(), fonte.clone());
    let chave = fundo::chave(tipo, &fonte.sha256);
    fundo::pedir(tipo, &chave, Box::new(move |c, prog| previa::produzir(&cap, &cache, &fonte, c, prog).map(|_| ())));
}

pub(crate) fn midia_previa(ws: &Path, sha256: &str, cache: &Path) -> Result<PreviaMidia> {
    let (_, fonte, cap) = fonte_do_caso(ws, sha256, cache)?;
    let p = previa::consultar(ws, cache, &fonte)?;
    if !p.pronta {
        pedir_previa(&cap, cache, &fonte, &p);
    }
    Ok(p)
}

/// O que a tela precisa de uma mídia do caso (tamanho exibido, taxa, faixa de tempo, prévia).
#[tauri::command]
pub async fn edicao_midia_info(app: AppHandle, workspace_path: String, sha256: String) -> Result<MidiaInfo> {
    let raiz = app_cache(&app)?;
    em_fundo(move || {
        let ws = Path::new(&workspace_path);
        let cache = cache_do_caso(&raiz, ws)?;
        let (vm, fonte, cap) = fonte_do_caso(ws, &sha256, &cache)?;
        let p = previa::consultar(ws, &cache, &fonte)?;
        if !p.pronta {
            pedir_previa(&cap, &cache, &fonte, &p);
        }
        Ok(previa::midia_info(&vm.id.to_string(), &vm.filename, &fonte, p))
    })
    .await
}

/// Prévia da mídia; pede a cópia ou o proxy à fila de fundo quando falta (`pronta` false).
#[tauri::command]
pub async fn edicao_midia_previa(app: AppHandle, workspace_path: String, sha256: String) -> Result<PreviaMidia> {
    let raiz = app_cache(&app)?;
    em_fundo(move || {
        let ws = Path::new(&workspace_path);
        midia_previa(ws, &sha256, &cache_do_caso(&raiz, ws)?)
    })
    .await
}

/// Tabela de quadros compacta (a melhor que houver).
#[tauri::command]
pub async fn edicao_tabela_quadros(app: AppHandle, workspace_path: String, sha256: String) -> Result<TabelaCompacta> {
    let raiz = app_cache(&app)?;
    em_fundo(move || {
        let ws = Path::new(&workspace_path);
        fonte_do_caso(ws, &sha256, &cache_do_caso(&raiz, ws)?)?.1.quadros.compacta()
    })
    .await
}

/// Quadro exibido num tempo, num índice ou num pts: é quem prende tempo a quadro.
#[tauri::command]
pub async fn edicao_quadro_em(app: AppHandle, workspace_path: String, sha256: String, alvo: AlvoQuadro) -> Result<QuadroEm> {
    let raiz = app_cache(&app)?;
    em_fundo(move || {
        let ws = Path::new(&workspace_path);
        fonte_do_caso(ws, &sha256, &cache_do_caso(&raiz, ws)?)?.1.quadros.quadro_em(alvo)
    })
    .await
}

/// Copia um arquivo (`origem` = caminho absoluto) para a mídia própria do projeto.
#[tauri::command]
pub async fn edicao_midia_importar(workspace_path: String, projeto_id: String, origem: String, tipo: TipoMidia) -> Result<MidiaRef> {
    em_fundo(move || armazem::importar_midia(Path::new(&workspace_path), &projeto_id, Path::new(&origem), tipo)).await
}

/// Projetos e exportações que usam a mídia (para o diálogo de exclusão de vídeo).
#[tauri::command]
pub async fn edicao_usos(workspace_path: String, sha256: String) -> Result<Vec<UsoMidia>> {
    em_fundo(move || armazem::usos(Path::new(&workspace_path), &sha256)).await
}

/// Narração gravada: cabeçalhos `CABECALHO_*`, corpo = bytes crus. No Linux o corpo pode chegar como
/// `InvokeBody::Json` (lista de números) quando o IPC cai para `postMessage`: aceitar os dois.
#[tauri::command]
pub async fn edicao_gravacao_salvar(request: tauri::ipc::Request<'_>) -> Result<MidiaRef> {
    let cab = |nome: &str| -> Result<String> {
        let v = request.headers().get(nome).ok_or_else(|| erro(format!("{}: cabeçalho {nome}", codigo::ITEM_INVALIDO)))?;
        sem_escape(v.to_str().map_err(|_| erro(format!("{}: cabeçalho {nome}", codigo::ITEM_INVALIDO)))?)
    };
    let pedido = (|| Ok((cab(CABECALHO_WORKSPACE)?, cab(CABECALHO_PROJETO)?, cab(CABECALHO_MIME)?, corpo(request.body())?)))();
    let (ws, projeto_id, mime, dados) = pedido.map_err(mensagens::erro_de_comando)?;
    em_fundo(move || armazem::gravar_midia(Path::new(&ws), &projeto_id, &mime, &dados)).await
}

/// Bytes do corpo: crus, ou JSON com a lista de números (ou `{ "bytes": [...] }`).
pub(crate) fn corpo(b: &tauri::ipc::InvokeBody) -> Result<Vec<u8>> {
    let lista = match b {
        tauri::ipc::InvokeBody::Raw(v) => return Ok(v.clone()),
        tauri::ipc::InvokeBody::Json(Value::Array(a)) => a,
        tauri::ipc::InvokeBody::Json(Value::Object(o)) => match o.get("bytes") {
            Some(Value::Array(a)) => a,
            _ => return Err(erro(format!("{}: corpo", codigo::ITEM_INVALIDO))),
        },
        tauri::ipc::InvokeBody::Json(_) => return Err(erro(format!("{}: corpo", codigo::ITEM_INVALIDO))),
    };
    lista
        .iter()
        .map(|v| v.as_u64().filter(|n| *n <= 255).map(|n| n as u8).ok_or_else(|| erro(format!("{}: corpo", codigo::ITEM_INVALIDO))))
        .collect()
}

/// Desfaz o `encodeURIComponent` (UTF-8).
pub(crate) fn sem_escape(s: &str) -> Result<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
            out.push(h.ok_or_else(|| erro(format!("{}: escape", codigo::ITEM_INVALIDO)))?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| erro(format!("{}: escape", codigo::ITEM_INVALIDO)))
}

// ---------------------------------------------------------------------------------------------- texto e capacidades

/// Sprite de texto para a prévia (o MESMO do render).
#[tauri::command]
pub async fn edicao_sprite_texto(app: AppHandle, workspace_path: String, pedido: PedidoSprite) -> Result<Sprite> {
    let raiz = app_cache(&app)?;
    em_fundo(move || texto::sprite(&cache_do_caso(&raiz, Path::new(&workspace_path))?, &pedido)).await
}

/// Atlas de glifos do carimbo nesse estilo; `altura_canvas` em px.
#[tauri::command]
pub async fn edicao_atlas_carimbo(app: AppHandle, workspace_path: String, estilo: EstiloTexto, altura_canvas: u32) -> Result<Atlas> {
    let raiz = app_cache(&app)?;
    em_fundo(move || texto::atlas_carimbo(&cache_do_caso(&raiz, Path::new(&workspace_path))?, &estilo, altura_canvas)).await
}

/// Resumo das capacidades do ffmpeg; `testar_de_novo` refaz a sonda de GPU.
#[tauri::command]
pub async fn edicao_capacidades(testar_de_novo: bool) -> Result<ResumoCapacidades> {
    em_fundo(move || {
        let cap = if testar_de_novo { sonda::testar_de_novo()? } else { sonda::capacidades()? };
        Ok(sonda::resumo(&cap))
    })
    .await
}

// ---------------------------------------------------------------------------------------------- render

fn tipo_do_alvo(a: &Alvo) -> TipoTarefa {
    match a {
        Alvo::Final { .. } => TipoTarefa::Exportar,
        Alvo::Previa { .. } => TipoTarefa::Previa,
        Alvo::Quadro { .. } => TipoTarefa::QuadroFinal,
    }
}

pub(crate) fn pedido(ws: &Path, projeto: Value, alvo: Alvo, ciencias: Vec<String>, app_cache: &Path) -> Result<PedidoRender> {
    Ok(PedidoRender { tipo: tipo_do_alvo(&alvo), ws: ws.to_path_buf(), cache: cache_do_caso(app_cache, ws)?, projeto, alvo, ciencias })
}

/// Enfileira um render do documento da tela e devolve o id da tarefa. `ciencias` = códigos periciais aceitos.
#[tauri::command]
pub async fn edicao_enfileirar(app: AppHandle, workspace_path: String, projeto: Value, alvo: Alvo, ciencias: Vec<String>) -> Result<String> {
    let raiz = app_cache(&app)?;
    em_fundo(move || fila::enfileirar(pedido(Path::new(&workspace_path), projeto, alvo, ciencias, &raiz)?)).await
}

/// Quadro final do slot `q` pelo motor real, com `largura` px (sem `-hwaccel`).
#[tauri::command]
pub async fn edicao_quadro_final(app: AppHandle, workspace_path: String, projeto: Value, q: u64, largura: u32) -> Result<QuadroFinal> {
    let raiz = app_cache(&app)?;
    em_fundo(move || {
        let id = fila::enfileirar(pedido(Path::new(&workspace_path), projeto, Alvo::Quadro { q, largura }, Vec::new(), &raiz)?)?;
        quadro_final_de(fila::esperar(&id)?, q)
    })
    .await
}

pub(crate) fn quadro_final_de(e: Estado, q: u64) -> Result<QuadroFinal> {
    match e {
        Estado::Pronta { relativo } => Ok(QuadroFinal { relativo, q }),
        Estado::Falhou { mensagem, .. } => Err(erro(mensagem)),
        Estado::Cancelada => Err(erro(executar::ERRO_CANCELADO)),
        outro => Err(erro(format!("{}: estado {outro:?}", codigo::ITEM_INVALIDO))),
    }
}

/// Nome do perito do caso (quem confere).
fn perito(ws: &Path) -> String {
    abrir_banco(ws)
        .ok()
        .and_then(|(c, o)| crate::database::repositories::occurrence_repo::find_by_id(&c, &o).ok().flatten())
        .and_then(|o| o.peritos.first().cloned())
        .unwrap_or_default()
}

/// Fecha uma saída em "A conferir" (conferida ou não) e conclui o relatório.
#[tauri::command]
pub async fn edicao_conferir(app: AppHandle, workspace_path: String, saida_id: String, conferida: bool) -> Result<SaidaRegistrada> {
    let _ = app;
    em_fundo(move || {
        let ws = Path::new(&workspace_path);
        fila::conferir_saida(ws, &saida_id, conferida, &perito(ws))
    })
    .await
}

/// Cancela uma tarefa da fila de render.
#[tauri::command]
pub async fn edicao_cancelar(tarefa_id: String) -> Result<()> {
    fila::cancelar(&tarefa_id).map_err(mensagens::erro_de_comando)
}

/// Tarefas da fila de render (de todos os casos; a tela filtra por `caso`).
#[tauri::command]
pub async fn edicao_fila() -> Result<Vec<Tarefa>> {
    Ok(fila::listar())
}

/// Tira da fila as tarefas terminadas e devolve as que ficam.
#[tauri::command]
pub async fn edicao_fila_limpar() -> Result<Vec<Tarefa>> {
    Ok(fila::limpar())
}

/// Tarefas da fila de fundo.
#[tauri::command]
pub async fn edicao_fundo() -> Result<Vec<TarefaFundo>> {
    Ok(fundo::listar())
}

/// Saídas registradas do caso, ou só as de um projeto.
#[tauri::command]
pub async fn edicao_saidas(workspace_path: String, projeto_id: Option<String>) -> Result<Vec<SaidaRegistrada>> {
    em_fundo(move || fila::saidas(Path::new(&workspace_path), projeto_id.as_deref())).await
}

/// Exclui uma exportação: linha do banco, retrato e a pasta da saída (somente leitura incluída); audita. Erro se estiver na fila.
#[tauri::command]
pub async fn edicao_saida_excluir(app: AppHandle, workspace_path: String, saida_id: String) -> Result<()> {
    let _ = app;
    em_fundo(move || {
        if fila::saida_na_fila(&saida_id) {
            return Err(erro(format!("na_fila: {saida_id}")));
        }
        fila::excluir_saida(Path::new(&workspace_path), &saida_id)
    })
    .await
}

// CONTRATO: acréscimo. Está no §5.12; o `refazer.rs` é da Fase 1 (P1.8).
/// Refaz uma exportação a partir do retrato (Fase 1): por ora `nao_suportado`.
#[tauri::command]
pub async fn edicao_refazer(workspace_path: String, saida_id: String) -> Result<String> {
    let _ = (workspace_path, saida_id);
    Err(mensagens::erro_de_comando(erro(format!("{}: refazer", codigo::NAO_SUPORTADO))))
}

// ---------------------------------------------------------------------------------------------- partida e saída

struct OuvinteTauri(AppHandle);

impl fila::Ouvinte for OuvinteTauri {
    fn fila(&self, tarefas: &[Tarefa]) {
        let _ = self.0.emit(fila::EVENTO_FILA, tarefas);
    }
    fn progresso(&self, evento: &fila::EventoProgresso) {
        let _ = self.0.emit(fila::EVENTO_PROGRESSO, evento);
    }
}

// CONTRATO: acréscimo. O integrador chama no `setup` e no `RunEvent::ExitRequested`.
/// Na partida (setup): cache da sonda, eventos das filas e limpeza do cache (o mais antigo sai ao abrir o app).
pub fn iniciar(app: &AppHandle) {
    // os eventos primeiro: não dependem da pasta de cache
    fila::ouvir(Box::new(OuvinteTauri(app.clone())));
    let a = app.clone();
    fundo::ouvir(Box::new(move |l: &[TarefaFundo]| {
        let _ = a.emit(fundo::EVENTO_FUNDO, l);
    }));
    let raiz = match app_cache(app) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("edição: {e}");
            return;
        }
    };
    let edicao = paths::dir_edicao_cache_raiz(&raiz);
    sonda::configurar(&edicao);
    std::thread::spawn(move || executar::limpar_cache(&edicao, executar::TETO_CACHE, &[]));
}

/// No `ExitRequested`: cancela as filas e mata os filhos.
pub fn encerrar() {
    fila::encerrar();
    fundo::encerrar();
    crate::edicao::ffmpeg::exec::matar_todos();
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::testes::fontes::{self, Fonte};

    /// Caso de verdade (manifesto e banco) e uma pasta de cache, com espaço e acento no caminho.
    fn caso() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let d = tempfile::Builder::new().prefix("p07-comandos-").tempdir().unwrap();
        let pai = d.path().join("Casos ç");
        std::fs::create_dir_all(&pai).unwrap();
        let criado = crate::workspace::create::create_workspace(
            crate::models::NewOccurrenceInput {
                titulo: Some("Comandos".into()),
                numero_bo: None,
                protocolo: None,
                oficio: None,
                tipo_pericia: None,
                municipio: None,
                peritos: vec!["Perito".into()],
                parent_directory: Some(pai.to_string_lossy().into()),
            },
            &pai,
        )
        .unwrap();
        let cache = d.path().join("cache do app");
        (d, criado.path, cache)
    }

    fn minimo(id: &str) -> Value {
        let mut v: Value = serde_json::from_str(include_str!("../../tests/fixtures/edicao/projeto-minimo.json")).unwrap();
        v["id"] = Value::String(id.into());
        v
    }

    /// Criar, salvar, abrir, listar, duplicar e excluir num caso temporário, pelas funções dos comandos.
    #[test]
    fn comandos_de_projeto_num_caso() {
        let (_d, ws, app) = caso();
        let id = uuid::Uuid::new_v4().to_string();
        assert!(armazem::listar(&ws).unwrap().is_empty());
        let e = abrir(&ws, &id, &app).unwrap_err();
        assert!(e.to_string().contains("projeto_nao_encontrado"), "{e}");

        let salvo = armazem::salvar(&ws, &minimo(&id)).unwrap();
        let lista = armazem::listar(&ws).unwrap();
        assert_eq!(lista.iter().map(|r| (r.id.as_str(), r.titulo.as_str())).collect::<Vec<_>>(), [(id.as_str(), "Mínimo")]);
        let a = abrir(&ws, &id, &app).unwrap();
        let occ = Manifest::read(&ws).unwrap().occurrence_id;
        assert_eq!(a.raiz_cache, paths::dir_edicao_cache(&app, &occ).to_string_lossy());
        assert!(Path::new(&a.raiz_cache).is_dir());
        assert_eq!(a.projeto["updated_at"], salvo.updated_at);
        assert!(!a.so_leitura, "{:?}", a.avisos);

        let mut v = a.projeto.clone();
        v["titulo"] = "Renomeado".into();
        armazem::salvar(&ws, &v).unwrap();
        assert_eq!(abrir(&ws, &id, &app).unwrap().projeto["titulo"], "Renomeado");

        let copia = armazem::duplicar(&ws, &id).unwrap();
        assert_ne!(copia.id, id);
        assert_eq!(armazem::listar(&ws).unwrap().len(), 2);

        // com tarefa na fila de render: recusado, nada apagado
        let caso_id = occ.to_string();
        let e = excluir(&ws, &id, &app, &|c, p| c == caso_id && p == id).unwrap_err();
        assert!(e.to_string().contains("na_fila"), "{e}");
        assert_eq!(armazem::listar(&ws).unwrap().len(), 2);
        // o cache de render do projeto sai junto
        let render = paths::dir_edicao_cache(&app, &occ).join("render").join("t1");
        std::fs::create_dir_all(&render).unwrap();
        std::fs::write(render.join(fila::RETRATO_TAREFA), serde_json::to_vec(&json_id(&id)).unwrap()).unwrap();
        excluir(&ws, &id, &app, &|_, _| false).unwrap();
        assert!(!render.exists());
        assert_eq!(armazem::listar(&ws).unwrap().iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), [copia.id.as_str()]);
        assert!(abrir(&ws, &id, &app).unwrap_err().to_string().contains("projeto_nao_encontrado"));
    }

    fn json_id(id: &str) -> Value {
        serde_json::json!({ "esquema": 2, "id": id })
    }

    /// Vídeo registrado no caso: prévia (original, pronta), tabela compacta e quadro num tempo, pelos comandos.
    #[test]
    fn midia_do_caso_previa_tabela_e_quadro() {
        let Some(cap) = fontes::cap() else { return };
        let (_d, ws, app) = caso();
        let f = fontes::gerar(Fonte::Mp4).unwrap();
        let vm = tauri::async_runtime::block_on(crate::commands::video_commands::register_video_media(
            ws.to_string_lossy().into(),
            crate::models::RegisterVideoInput { source_path: f.to_string_lossy().into(), allow_duplicate: false },
        ))
        .unwrap();
        let cache = cache_do_caso(&app, &ws).unwrap();
        let p = midia_previa(&ws, &vm.sha256, &cache).unwrap();
        assert_eq!((p.tipo, p.pronta, p.desloc_s), (TipoPrevia::Original, true, 0.0), "{}", cap.versao);
        assert_eq!((p.largura, p.altura), (320, 180));
        let (_, fonte, _) = fonte_do_caso(&ws, &vm.sha256, &cache).unwrap();
        let (_, de_novo, _) = fonte_do_caso(&ws, &vm.sha256, &cache).unwrap();
        // a mesma tabela dá a mesma `FonteInfo` (a do decodificador pode chegar da fila de fundo no meio)
        if de_novo.quadros.origem == fonte.quadros.origem {
            assert!(Arc::ptr_eq(&fonte.quadros, &de_novo.quadros) && fonte.conteiner == de_novo.conteiner);
        }
        let t = fonte.quadros.compacta().unwrap();
        assert_eq!(t.passos.len(), 449);
        let q = fonte.quadros.quadro_em(AlvoQuadro::T { t: 1.0 }).unwrap();
        assert_eq!((q.indice, q.pts), (30, 30 * 512));
        assert!(fonte_do_caso(&ws, &"0".repeat(64), &cache).unwrap_err().to_string().contains("midia_ausente"));
        assert!(fonte_do_caso(&ws, "../x", &cache).unwrap_err().to_string().contains("sha256_invalido"));
    }

    #[test]
    fn corpo_da_gravacao_e_cabecalhos() {
        use tauri::ipc::InvokeBody;
        assert_eq!(corpo(&InvokeBody::Raw(vec![1, 2, 255])).unwrap(), [1, 2, 255]);
        assert_eq!(corpo(&InvokeBody::Json(serde_json::json!([0, 128, 255]))).unwrap(), [0, 128, 255]);
        assert_eq!(corpo(&InvokeBody::Json(serde_json::json!({ "bytes": [7] }))).unwrap(), [7]);
        assert!(corpo(&InvokeBody::Json(serde_json::json!([256]))).is_err());
        assert!(corpo(&InvokeBody::Json(serde_json::json!(["a"]))).is_err());
        assert!(corpo(&InvokeBody::Json(serde_json::json!("x"))).is_err());
        assert_eq!(sem_escape("%2Fhome%2Fandr%C3%A9%2FCaso%20%C3%A7").unwrap(), "/home/andré/Caso ç");
        assert_eq!(sem_escape("audio%2Fwebm%3Bcodecs%3Dopus").unwrap(), "audio/webm;codecs=opus");
        assert!(sem_escape("%G1").is_err() && sem_escape("%C3").is_err() && sem_escape("%2").is_err());
    }

    #[test]
    fn pedido_e_resposta_do_quadro_final() {
        let (_d, ws, app) = caso();
        let p = pedido(&ws, json_id("p1"), Alvo::Quadro { q: 7, largura: 640 }, vec![], &app).unwrap();
        assert_eq!(p.tipo, TipoTarefa::QuadroFinal);
        assert_eq!(p.cache, paths::dir_edicao_cache(&app, &Manifest::read(&ws).unwrap().occurrence_id));
        assert_eq!(pedido(&ws, json_id("p1"), Alvo::Previa { altura: 360, de_q: 0, ate_q: 9 }, vec![], &app).unwrap().tipo, TipoTarefa::Previa);
        assert_eq!(quadro_final_de(Estado::Pronta { relativo: "quadro/ab.bmp".into() }, 7).unwrap(), QuadroFinal { relativo: "quadro/ab.bmp".into(), q: 7 });
        assert!(quadro_final_de(Estado::Cancelada, 7).unwrap_err().to_string().contains("cancelado"));
        let e = quadro_final_de(Estado::Falhou { mensagem: "filtro_ausente: b000".into(), log_rel: String::new() }, 7).unwrap_err();
        assert!(e.to_string().contains("filtro_ausente: b000"));
    }

    /// Só compila se todo argumento desserializa e toda resposta serializa: é o que o `invoke_handler` vai exigir.
    #[test]
    fn comandos_registraveis() {
        let _h: Box<dyn Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync> = Box::new(tauri::generate_handler![
            edicao_listar,
            edicao_abrir,
            edicao_salvar,
            edicao_excluir,
            edicao_duplicar,
            edicao_analisar,
            edicao_midia_info,
            edicao_midia_previa,
            edicao_tabela_quadros,
            edicao_quadro_em,
            edicao_midia_importar,
            edicao_usos,
            edicao_gravacao_salvar,
            edicao_sprite_texto,
            edicao_atlas_carimbo,
            edicao_capacidades,
            edicao_enfileirar,
            edicao_quadro_final,
            edicao_conferir,
            edicao_cancelar,
            edicao_fila,
            edicao_fila_limpar,
            edicao_fundo,
            edicao_saidas,
            edicao_saida_excluir,
            edicao_refazer,
        ]);
    }
}
