//! Comandos da Edição (arquitetura §5.12). Compartilhado desde a Fase 1 (O2: o integrador declara e registra; o corpo
//! de cada comando da Fase 1 chama a função do pacote dono, `contratos-fase1.md` (c)).
//! `workspace_path` = raiz do caso. Caminhos `relativo` das respostas: ao cache da Edição do caso
//! (`app_cache_dir()/edicao/<id do caso>/`), salvo onde o tipo diz outra coisa.
//! Fora daqui: `edicao_diagnostico_salvar` (P0.9, arquivo próprio); proveniência da mídia (P1.10,
//! `video_proveniencia_commands.rs`).
//! Todo erro sai por `mensagens::erro_de_comando` (`"<código>: <frase>"`); o trabalho roda em `spawn_blocking`
//! (o `PR_SET_PDEATHSIG` dos filhos vale enquanto a thread que os criou viver).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::edicao::armazem::{self, ProjetoAberto, ResumoProjeto, Salvo, UsoMidia};
use crate::edicao::legendas::{self as degravacao, Degravacao};
use crate::edicao::medidas;
use crate::edicao::ondas::{self, Onda};
use crate::edicao::origem::{self, EstadoOrigem};
use crate::edicao::refazer;
use crate::edicao::camada::texto::{self, Atlas, PedidoSprite, Sprite};
use crate::edicao::executar::{self, abrir_banco, sha_valido};
use crate::edicao::ffmpeg::sonda::{self, Capacidades, ResumoCapacidades};
use crate::edicao::fila::{self, Estado, PedidoRender, SaidaRegistrada, Tarefa, TipoTarefa};
use crate::edicao::fundo::{self, TarefaFundo};
use crate::edicao::mensagens::{self, codigo};
use crate::edicao::modelo::{Analise, EstiloTexto, LinhaResolvida, MidiaRef, Problema, Racional, RetratoCalculo, RetratoCalibracao, TipoMidia};
use crate::edicao::motor::{Alvo, FonteInfo};
use crate::edicao::previa::{self, MidiaInfo, PreviaMidia};
use crate::edicao::proxy;
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
    em_fundo(move || abrir(Path::new(&workspace_path), &projeto_id, &raiz, &fila::projeto_ocupado)).await
}

/// Abrir também fecha a sessão anterior do projeto: densos, silhuetas e `.trf` que nada cita saem aqui (§4; o
/// histórico da tela começa vazio). Com render do projeto na fila não sai nada (o `.trf` novo ainda não é citado).
pub(crate) fn abrir(ws: &Path, projeto_id: &str, app_cache: &Path, ocupado: &dyn Fn(&str, &str) -> bool) -> Result<ProjetoAberto> {
    let mut a = armazem::abrir(ws, projeto_id)?;
    let cache = cache_do_caso(app_cache, ws)?;
    let caso = cache.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !a.so_leitura && !ocupado(&caso, projeto_id) {
        if let Ok(p) = armazem::tipado(&a.projeto) {
            if let Err(e) = armazem::limpar_orfaos(ws, &p, &[]) {
                tracing::warn!("edição: órfãos de {projeto_id}: {e}");
            }
        }
    }
    a.raiz_cache = cache.to_string_lossy().into_owned();
    Ok(a)
}

/// Grava o documento como chegou e devolve o `updated_at` gravado.
/// `raiz_cache` vem junto: a edição nova aprende a raiz do cache sem `edicao_abrir` (contratos (g) 11).
#[tauri::command]
pub async fn edicao_salvar(app: AppHandle, workspace_path: String, projeto: Value) -> Result<Salvo> {
    let raiz = app_cache(&app).ok();
    em_fundo(move || salvar(Path::new(&workspace_path), &projeto, raiz.as_deref())).await
}

/// A raiz vai de carona: sem ela (cache sem gravação) a gravação vale e a raiz fica vazia (a tela não aprende).
pub(crate) fn salvar(ws: &Path, projeto: &Value, app_cache: Option<&Path>) -> Result<Salvo> {
    let mut s = armazem::salvar(ws, projeto)?;
    s.raiz_cache = app_cache.and_then(|c| cache_do_caso(c, ws).ok()).map(|c| c.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(s)
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
    let mut a = validar::analisar(&p, &prep.contexto(), None);
    // retratos de análise que mudaram depois de gravados no projeto (P1.3)
    a.problemas.extend(medidas::desatualizados(ws, &p));
    // rastreios "Da análise" cuja trilha mudou depois da cópia (P2.5, contratos-fase2 (c))
    a.problemas.extend(crate::commands::pessoas_commands::desatualizados(ws, &p));
    Ok(a)
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

pub(crate) fn midia_previa(ws: &Path, sha256: &str, cache: &Path) -> Result<PreviaMidia> {
    let (_, fonte, cap) = fonte_do_caso(ws, sha256, cache)?;
    let p = previa::consultar(ws, cache, &fonte)?;
    // a cópia ou o proxy que faltam e, com o tocável pronto, o proxy por desempenho (P1.9)
    previa::pedir_falta(&cap, cache, &fonte, &p);
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
        previa::pedir_falta(&cap, &cache, &fonte, &p);
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

/// Nome do perito do caso (quem confere); vazio = caso sem perito (`edicao_conferir` recusa com `perito_ausente`).
/// Banco que não abre é erro do banco, não caso sem perito.
pub(crate) fn perito(ws: &Path) -> Result<String> {
    let (c, o) = abrir_banco(ws)?;
    let caso = crate::database::repositories::occurrence_repo::find_by_id(&c, &o)?;
    Ok(caso.and_then(|o| o.peritos.first().cloned()).unwrap_or_default())
}

/// Fecha uma saída em "A conferir" (conferida ou não) e conclui o relatório.
#[tauri::command]
pub async fn edicao_conferir(app: AppHandle, workspace_path: String, saida_id: String, conferida: bool) -> Result<SaidaRegistrada> {
    let _ = app;
    em_fundo(move || conferir(Path::new(&workspace_path), &saida_id, conferida)).await
}

pub(crate) fn conferir(ws: &Path, saida_id: &str, conferida: bool) -> Result<SaidaRegistrada> {
    fila::conferir_saida(ws, saida_id, conferida, &perito(ws)?)
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
    em_fundo(move || saidas(Path::new(&workspace_path), projeto_id.as_deref())).await
}

/// A lista das saídas abre o caso para a Edição: a sobra de uma publicação interrompida (processo morto no meio da
/// cópia) sai aqui, e não só na próxima exportação.
pub(crate) fn saidas(ws: &Path, projeto_id: Option<&str>) -> Result<Vec<SaidaRegistrada>> {
    fila::limpar_sobras_de_publicar(ws);
    fila::saidas(ws, projeto_id)
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

/// Refaz uma exportação a partir do retrato (§10.4): o id da tarefa `Refazer` (P1.8, `refazer::pedir`).
#[tauri::command]
pub async fn edicao_refazer(app: AppHandle, workspace_path: String, saida_id: String) -> Result<String> {
    let raiz = app_cache(&app)?;
    em_fundo(move || {
        let ws = Path::new(&workspace_path);
        refazer::pedir(ws, &cache_do_caso(&raiz, ws)?, &saida_id)
    })
    .await
}

// ---------------------------------------------------------------------------------------------- Fase 1

/// Intermediário de origem do clipe (§7.5; P1.4): o pronto, ou o progresso na fila de fundo (evento `edicao:fundo`).
#[tauri::command]
pub async fn edicao_origem_pedir(app: AppHandle, workspace_path: String, projeto: Value, clipe_id: String) -> Result<EstadoOrigem> {
    let raiz = app_cache(&app)?;
    em_fundo(move || {
        let ws = Path::new(&workspace_path);
        origem::pedir_do_projeto(ws, &cache_do_caso(&raiz, ws)?, &projeto, &clipe_id)
    })
    .await
}

/// Forma de onda da mídia (P1.9): a do cache, ou None depois de pedir à fila de fundo (`onda:<sha256>`).
#[tauri::command]
pub async fn edicao_onda(app: AppHandle, workspace_path: String, sha256: String) -> Result<Option<Onda>> {
    let raiz = app_cache(&app)?;
    em_fundo(move || {
        let ws = Path::new(&workspace_path);
        ondas::pedir(ws, &cache_do_caso(&raiz, ws)?, &sha256)
    })
    .await
}

/// Degravação do som da mídia, em tempo de fonte (P1.6): legendas "Da degravação…" e silêncios (P1.5).
#[tauri::command]
pub async fn edicao_degravacao(workspace_path: String, sha256: String) -> Result<Degravacao> {
    em_fundo(move || degravacao::degravacao(Path::new(&workspace_path), &sha256)).await
}

/// Retrato da calibração da análise para a régua (P1.3).
#[tauri::command]
pub async fn edicao_retrato_calibracao(workspace_path: String, calibracao_id: String) -> Result<RetratoCalibracao> {
    em_fundo(move || medidas::retrato_calibracao(Path::new(&workspace_path), &calibracao_id)).await
}

/// Retrato do cálculo de velocidade da análise (P1.3).
#[tauri::command]
pub async fn edicao_retrato_calculo(workspace_path: String, calculo_id: String) -> Result<RetratoCalculo> {
    em_fundo(move || medidas::retrato_calculo(Path::new(&workspace_path), &calculo_id)).await
}

// CONTRATO: acréscimo do fecho da Fase 1 (contratos-fase1 (g)). O estado do intermediário SEM pedir: o cartão e o
// palco perguntam por todo clipe; `edicao_origem_pedir` poria um render de 1,1× a duração na fila a cada pergunta.
/// Situação do intermediário de origem do clipe (§7.5), sem pedir: null = nem pronto nem na fila.
#[tauri::command]
pub async fn edicao_origem_estado(app: AppHandle, workspace_path: String, projeto: Value, clipe_id: String) -> Result<Option<EstadoOrigem>> {
    let raiz = app_cache(&app)?;
    em_fundo(move || {
        let ws = Path::new(&workspace_path);
        origem::estado_do_projeto(ws, &cache_do_caso(&raiz, ws)?, &projeto, &clipe_id)
    })
    .await
}

// CONTRATO: acréscimo do fecho da Fase 1 (contratos-fase1 (g)). O proxy que o palco pede (§7.6).
/// Prévia do clipe para o palco: com busca lenta medida (`busca_p50_ms`: p50 de 3 buscas pausadas, ms; null = sem
/// medição) ou clipe com rampa, ré ou abaixo de 0,25×, pede o proxy à fila de fundo. Devolve o que toca agora.
#[tauri::command]
pub async fn edicao_proxy_pedir(app: AppHandle, workspace_path: String, projeto: Value, clipe_id: String, busca_p50_ms: Option<f64>) -> Result<PreviaMidia> {
    let raiz = app_cache(&app)?;
    em_fundo(move || {
        let ws = Path::new(&workspace_path);
        proxy::pedir_do_projeto(ws, &cache_do_caso(&raiz, ws)?, &projeto, &clipe_id, busca_p50_ms)
    })
    .await
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
    std::thread::spawn(move || {
        executar::limpar_cache(&edicao, executar::TETO_CACHE, &[]);
        executar::limpar_temporarias(&std::env::temp_dir());
    });
}

/// O que o `ExitRequested` espera das filas, no máximo. O que não olha o cancelamento (a publicação copiando e
/// conferindo o arquivo numa pasta de rede, a sonda de GPU) segurava a janela parada até acabar.
pub const ESPERA_NA_SAIDA: std::time::Duration = std::time::Duration::from_secs(4);

/// No `ExitRequested`: cancela as filas e mata os filhos, sem prender a janela além de `ESPERA_NA_SAIDA`.
pub fn encerrar() {
    let acoes: Vec<Box<dyn FnOnce() + Send>> = vec![Box::new(fila::encerrar), Box::new(fundo::encerrar)];
    if !esperar_ate(acoes, ESPERA_NA_SAIDA) {
        tracing::warn!("edição: filas ainda ocupadas depois de {ESPERA_NA_SAIDA:?}; saindo");
    }
    // o que nasceu depois do primeiro corte (a sonda lança um filho por tentativa) também morre
    crate::edicao::ffmpeg::exec::matar_todos();
}

/// Roda cada ação numa thread própria (o sinal de cancelamento de todas sobe já) e espera todas até `espera`.
/// Devolve se todas acabaram; as que sobram ficam soltas (o processo está saindo).
pub(crate) fn esperar_ate(acoes: Vec<Box<dyn FnOnce() + Send>>, espera: std::time::Duration) -> bool {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let n = acoes.len();
    for a in acoes {
        let tx = tx.clone();
        let lancada = std::thread::Builder::new().name("edicao-saida".into()).spawn(move || {
            a();
            let _ = tx.send(());
        });
        if lancada.is_err() {
            return false;
        }
    }
    drop(tx);
    let fim = std::time::Instant::now() + espera;
    for _ in 0..n {
        let falta = fim.saturating_duration_since(std::time::Instant::now());
        if rx.recv_timeout(falta).is_err() {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::fundo::TipoFundo;
    use crate::edicao::previa::TipoPrevia;
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

    /// A edição nova aprende a raiz do cache na primeira gravação, a mesma de `edicao_abrir` (contratos (g) 11).
    #[test]
    fn salvar_devolve_a_raiz_do_cache() {
        let (_d, ws, app) = caso();
        let id = uuid::Uuid::new_v4().to_string();
        let s = salvar(&ws, &minimo(&id), Some(&app)).unwrap();
        let occ = Manifest::read(&ws).unwrap().occurrence_id;
        assert_eq!(s.raiz_cache, paths::dir_edicao_cache(&app, &occ).to_string_lossy());
        assert!(Path::new(&s.raiz_cache).is_dir());
        assert_eq!(s.raiz_cache, abrir(&ws, &id, &app, &|_: &str, _: &str| false).unwrap().raiz_cache);
        // cache que não aceita pasta (um arquivo no lugar): a gravação vale, a raiz fica vazia
        let ruim = app.parent().unwrap().join("cache-arquivo");
        std::fs::write(&ruim, b"x").unwrap();
        let mut v = minimo(&id);
        v["titulo"] = Value::String("Depois".into());
        let s = salvar(&ws, &v, Some(&ruim)).unwrap();
        assert_eq!(s.raiz_cache, "");
        assert_eq!(armazem::abrir(&ws, &id).unwrap().projeto["titulo"], "Depois");
        assert_eq!(salvar(&ws, &minimo(&id), None).unwrap().raiz_cache, "");
    }

    /// Criar, salvar, abrir, listar, duplicar e excluir num caso temporário, pelas funções dos comandos.
    #[test]
    fn comandos_de_projeto_num_caso() {
        let (_d, ws, app) = caso();
        let id = uuid::Uuid::new_v4().to_string();
        assert!(armazem::listar(&ws).unwrap().is_empty());
        let livre = |_: &str, _: &str| false;
        let e = abrir(&ws, &id, &app, &livre).unwrap_err();
        assert!(e.to_string().contains("projeto_nao_encontrado"), "{e}");

        let salvo = armazem::salvar(&ws, &minimo(&id)).unwrap();
        let lista = armazem::listar(&ws).unwrap();
        assert_eq!(lista.iter().map(|r| (r.id.as_str(), r.titulo.as_str())).collect::<Vec<_>>(), [(id.as_str(), "Mínimo")]);
        let a = abrir(&ws, &id, &app, &livre).unwrap();
        let occ = Manifest::read(&ws).unwrap().occurrence_id;
        assert_eq!(a.raiz_cache, paths::dir_edicao_cache(&app, &occ).to_string_lossy());
        assert!(Path::new(&a.raiz_cache).is_dir());
        assert_eq!(a.projeto["updated_at"], salvo.updated_at);
        assert!(!a.so_leitura, "{:?}", a.avisos);

        let mut v = a.projeto.clone();
        v["titulo"] = "Renomeado".into();
        armazem::salvar(&ws, &v).unwrap();
        assert_eq!(abrir(&ws, &id, &app, &livre).unwrap().projeto["titulo"], "Renomeado");

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
        assert!(abrir(&ws, &id, &app, &livre).unwrap_err().to_string().contains("projeto_nao_encontrado"));
    }

    /// Abrir apaga o `.trf` que nada cita (o citado fica); com render do projeto na fila, não apaga nada.
    #[test]
    fn abrir_limpa_orfaos_da_sessao_anterior() {
        let (_d, ws, app) = caso();
        let id = uuid::Uuid::new_v4().to_string();
        let (citado, orfao) = ("c".repeat(64), "e".repeat(64));
        let mut v = minimo(&id);
        // qualquer texto do documento que cite o arquivo o mantém
        v["titulo"] = Value::String(citado.clone());
        armazem::salvar(&ws, &v).unwrap();
        let vidstab = armazem::pasta_projeto(&ws, &id).join("vidstab");
        std::fs::create_dir_all(&vidstab).unwrap();
        for s in [&citado, &orfao] {
            std::fs::write(vidstab.join(format!("{s}.trf")), b"trf").unwrap();
        }
        abrir(&ws, &id, &app, &|_, p| p == id).unwrap();
        assert!(vidstab.join(format!("{orfao}.trf")).is_file(), "projeto com render na fila");
        abrir(&ws, &id, &app, &|_, _| false).unwrap();
        assert!(vidstab.join(format!("{citado}.trf")).is_file());
        assert!(!vidstab.join(format!("{orfao}.trf")).exists());
    }

    /// Edição nova que nunca foi gravada: importar mídia e gravar narração criam a pasta do projeto (decisão (h)18); o
    /// projeto só entra na lista quando o documento é gravado.
    #[test]
    fn midia_de_edicao_nova_sem_gravacao() {
        let (d, ws, _) = caso();
        let id = uuid::Uuid::new_v4().to_string();
        assert!(!armazem::pasta_projeto(&ws, &id).exists());
        let png = d.path().join("logo.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255])).save(&png).unwrap();
        let m = armazem::importar_midia(&ws, &id, &png, TipoMidia::Imagem).unwrap();
        assert!(paths::resolver_relativo(&ws, &m.relativo).is_file(), "{}", m.relativo);
        let n = armazem::gravar_midia(&ws, &uuid::Uuid::new_v4().to_string(), "audio/webm;codecs=opus", b"\x1a\x45\xdf\xa3").unwrap();
        assert!(paths::resolver_relativo(&ws, &n.relativo).is_file(), "{}", n.relativo);
        assert!(armazem::listar(&ws).unwrap().is_empty(), "sem projeto.json não é projeto");
        armazem::salvar(&ws, &minimo(&id)).unwrap();
        assert_eq!(armazem::listar(&ws).unwrap().len(), 1);
    }

    /// Caso sem perito: o nome vem vazio e a conferência recusa com `perito_ausente` (nada muda no banco).
    #[test]
    fn conferir_sem_perito_recusa() {
        let d = tempfile::Builder::new().prefix("p07-sem-perito-").tempdir().unwrap();
        let criado = crate::workspace::create::create_workspace(
            crate::models::NewOccurrenceInput {
                titulo: Some("Sem perito".into()),
                numero_bo: None,
                protocolo: None,
                oficio: None,
                tipo_pericia: None,
                municipio: None,
                peritos: vec![],
                parent_directory: Some(d.path().to_string_lossy().into()),
            },
            d.path(),
        )
        .unwrap();
        assert_eq!(perito(&criado.path).unwrap(), "");
        let e = conferir(&criado.path, &uuid::Uuid::new_v4().to_string(), true).unwrap_err();
        assert!(e.to_string().contains("perito_ausente"), "{e}");
        let e = mensagens::erro_de_comando(e).to_string();
        assert!(e.contains("perito_ausente: Caso sem perito"), "{e}");
        let (_d, com_perito, _) = caso();
        assert_eq!(perito(&com_perito).unwrap(), "Perito");
        // banco que não abre não é "caso sem perito"
        std::fs::write(paths::sqlite_path(&com_perito), b"isto nao e um banco sqlite").unwrap();
        let e = conferir(&com_perito, &uuid::Uuid::new_v4().to_string(), true).unwrap_err().to_string();
        assert!(!e.contains("perito_ausente"), "{e}");
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
        // a tabela pedida ao fundo grava no cache deste caso: espera antes de a pasta temporária sair (senão sobra em /tmp)
        let chave = fundo::chave(TipoFundo::Tabela, &vm.sha256);
        let t0 = std::time::Instant::now();
        while fundo::estado(&chave).is_some_and(|t| matches!(t.estado, fundo::EstadoFundo::NaFila | fundo::EstadoFundo::Rodando)) {
            assert!(t0.elapsed() < std::time::Duration::from_secs(120), "tabela de fundo presa");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// Ligação do P1.9 nos comandos: mídia que toca mas pede proxy por desempenho (aqui, 120 qps) sai de
    /// `midia_previa` no original, já tocável, e o proxy entra na fila de fundo; pronto, ele passa a ser a resposta.
    /// Com o `if !p.pronta` de antes o proxy por desempenho nunca era pedido.
    #[test]
    fn previa_pronta_ainda_pede_o_proxy_por_desempenho() {
        let Some(cap) = fontes::cap() else { return };
        let (_d, ws, app) = caso();
        // fonte só deste teste (a chave da fila de fundo é o SHA-256 da mídia)
        let dir = fontes::pasta_temporaria("comandos-120qps");
        let video = dir.join("rápida 120.mp4");
        let st = crate::tools::command(&cap.binario)
            .args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=s=326x184:r=120:d=1", "-c:v", "libx264", "-preset", "ultrafast", "-g", "30", "-bf", "0", "-pix_fmt", "yuv420p"])
            .args(["-metadata", &format!("comment=desempenho {}", cap.versao)])
            .arg(&video)
            .status()
            .unwrap();
        assert!(st.success());
        let vm = tauri::async_runtime::block_on(crate::commands::video_commands::register_video_media(
            ws.to_string_lossy().into(),
            crate::models::RegisterVideoInput { source_path: video.to_string_lossy().into(), allow_duplicate: false },
        ))
        .unwrap();
        let cache = cache_do_caso(&app, &ws).unwrap();
        let chave = fundo::chave(TipoFundo::Proxy, &vm.sha256);
        assert!(fundo::estado(&chave).is_none());
        let p = midia_previa(&ws, &vm.sha256, &cache).unwrap();
        assert_eq!((p.tipo, p.pronta), (TipoPrevia::Original, true), "{}: toca o original enquanto o proxy falta", cap.versao);
        assert!(fundo::estado(&chave).is_some(), "{}: o proxy por desempenho não foi pedido", cap.versao);
        let t0 = std::time::Instant::now();
        loop {
            match fundo::estado(&chave).map(|t| t.estado) {
                Some(fundo::EstadoFundo::NaFila | fundo::EstadoFundo::Rodando) => {}
                Some(fim) => {
                    assert_eq!(fim, fundo::EstadoFundo::Pronta, "{}", cap.versao);
                    break;
                }
                None => panic!("tarefa do proxy sumiu"),
            }
            assert!(t0.elapsed() < std::time::Duration::from_secs(180), "proxy de fundo preso");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let p = midia_previa(&ws, &vm.sha256, &cache).unwrap();
        assert_eq!((p.tipo, p.pronta), (TipoPrevia::Proxy, true), "{}", cap.versao);
        assert!(cache.join(&p.relativo).is_file(), "{}", p.relativo);
        // a tabela pedida ao fundo grava no cache deste caso: espera antes de a pasta temporária sair
        let tabela = fundo::chave(TipoFundo::Tabela, &vm.sha256);
        while fundo::estado(&tabela).is_some_and(|t| matches!(t.estado, fundo::EstadoFundo::NaFila | fundo::EstadoFundo::Rodando)) {
            assert!(t0.elapsed() < std::time::Duration::from_secs(300), "tabela de fundo presa");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// Folga do relógio dos testes de tempo, com outros agentes na máquina (O1).
    const FOLGA: u32 = 3;

    /// A saída do app não espera além do teto por quem não olha o cancelamento (a publicação, a sonda): as ações
    /// rápidas terminam, a lenta fica solta e a espera volta dentro do teto.
    #[test]
    fn saida_nao_espera_alem_do_teto() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::time::{Duration, Instant};
        let feitas = Arc::new(AtomicUsize::new(0));
        let solta = Arc::new(AtomicBool::new(false));
        let rapida = |f: &Arc<AtomicUsize>| {
            let f = f.clone();
            Box::new(move || {
                f.fetch_add(1, Ordering::SeqCst);
            }) as Box<dyn FnOnce() + Send>
        };
        // tudo acaba: verdadeiro, sem gastar o teto
        let t0 = Instant::now();
        assert!(esperar_ate(vec![rapida(&feitas), rapida(&feitas)], Duration::from_secs(30)));
        assert_eq!(feitas.load(Ordering::SeqCst), 2);
        assert!(t0.elapsed() < Duration::from_secs(10), "{:?}", t0.elapsed());
        // uma ação presa (a publicação numa pasta de rede): a espera volta no teto, com a rápida feita
        let teto = Duration::from_millis(300);
        let presa = {
            let s = solta.clone();
            Box::new(move || {
                while !s.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }) as Box<dyn FnOnce() + Send>
        };
        let t0 = Instant::now();
        assert!(!esperar_ate(vec![presa, rapida(&feitas)], teto));
        let levou = t0.elapsed();
        assert!(levou >= teto && levou < teto * FOLGA, "{levou:?} (teto {teto:?})");
        assert_eq!(feitas.load(Ordering::SeqCst), 3, "a rápida roda ao lado da presa, não depois dela");
        solta.store(true, Ordering::SeqCst);
        assert!(ESPERA_NA_SAIDA <= Duration::from_secs(5));
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

    /// Processo morto no meio da publicação: a pasta pela metade (vídeo somente leitura, marca solta) e o retrato
    /// saem quando a lista das saídas é pedida; a saída registrada fica.
    #[test]
    fn lista_das_saidas_tira_a_sobra_de_publicacao_interrompida() {
        let (_d, ws, _app) = caso();
        let raiz = paths::dir_edicao_saida(&ws);
        let morta = raiz.join("Sobra 2026-11-15 10h00m00");
        std::fs::create_dir_all(&morta).unwrap();
        std::fs::write(morta.join("Sobra.mp4"), b"pela metade").unwrap();
        fila::somente_leitura(&morta.join("Sobra.mp4")).unwrap();
        let (pid, sid) = (uuid::Uuid::new_v4().to_string(), uuid::Uuid::new_v4().to_string());
        std::fs::write(morta.join(fila::PUBLICANDO), serde_json::json!({ "saida_id": sid, "projeto_id": pid }).to_string()).unwrap();
        let retrato = fila::pasta_retrato(&ws, &pid, &sid).unwrap();
        std::fs::create_dir_all(retrato.join("dados")).unwrap();
        std::fs::write(retrato.join("projeto.json"), b"{}").unwrap();
        let alheia = raiz.join("Recortes");
        std::fs::create_dir_all(&alheia).unwrap();
        assert!(saidas(&ws, None).unwrap().is_empty());
        assert!(!morta.exists() && !retrato.exists(), "a sobra ficou");
        assert!(alheia.is_dir());
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
            edicao_origem_pedir,
            edicao_onda,
            edicao_degravacao,
            edicao_retrato_calibracao,
            edicao_retrato_calculo,
            edicao_origem_estado,
            edicao_proxy_pedir,
        ]);
    }
}
