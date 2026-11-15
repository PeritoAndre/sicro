//! Comandos do rastreio da Edição e da conferência das ocultações (§6.3, Fase 2). Compartilhado (O2): o integrador
//! declara e registra; o corpo chama a função do pacote dono (`contratos-fase2.md` (c)). `edicao_rastreio_tira` é da
//! Fase 4. Erro sai por `mensagens::erro_de_comando` (`"<código>: <frase>"`); o trabalho roda em `spawn_blocking`.

use std::path::{Path, PathBuf};

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::settings_commands::get_app_settings;
use crate::edicao::armazem;
use crate::edicao::conferencia::{self, TrechoConferencia};
use crate::edicao::mensagens::{self, codigo};
use crate::edicao::modelo::{Caixa, Chave, Rastreio};
use crate::edicao::rastreio::arquivo::{self, Editado, OpRastreio};
use crate::edicao::rastreio::corrida::{self, EventoRastreio, PedidoSeguir, EVENTO_RASTREIO};
use crate::edicao::rastreio::{ArquivoDenso, EscolhaMotor, SentidoPedido};
use crate::error::{Result, SicroError};
use crate::workspace::manifest::Manifest;
use crate::workspace::paths;

/// Roda `f` numa thread que dura a chamada e passa o erro pelo catálogo (o mesmo de `edicao_commands`).
async fn em_fundo<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| SicroError::Workspace(format!("edição: {e}")))
        .and_then(|r| r)
        .map_err(mensagens::erro_de_comando)
}

/// `app_cache_dir()/edicao/<id do caso>/` (criada).
fn cache_do_caso(app: &AppHandle, ws: &Path) -> Result<PathBuf> {
    let raiz = app.path().app_cache_dir().map_err(|e| SicroError::Filesystem(format!("cache dir: {e}")))?;
    let c = paths::dir_edicao_cache(&raiz, &Manifest::read(ws)?.occurrence_id);
    std::fs::create_dir_all(&c)?;
    Ok(c)
}

fn pasta(ws: &Path, projeto_id: &str) -> Result<PathBuf> {
    if !armazem::id_valido(projeto_id) {
        return Err(SicroError::Validation(codigo::ESQUEMA.into()));
    }
    Ok(armazem::pasta_projeto(ws, projeto_id))
}

/// Arquivo denso do rastreio (§6.1), com o SHA-256 conferido (P2.2).
#[tauri::command]
pub async fn edicao_rastreio_ler(workspace_path: String, projeto_id: String, rastreio_id: String, sha256: String) -> Result<ArquivoDenso> {
    em_fundo(move || arquivo::ler(&pasta(Path::new(&workspace_path), &projeto_id)?, &rastreio_id, &sha256)).await
}

/// Começa uma corrida (§6.3) e devolve o `tarefa_id`; o andamento e o fim chegam pelo evento `edicao:rastreio`.
/// Não grava nada no projeto (P2.2).
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn edicao_rastreio_seguir(
    app: AppHandle,
    workspace_path: String,
    projeto: Value,
    rastreio_id: String,
    clipe_id: String,
    ancora: Chave<Caixa>,
    sentido: SentidoPedido,
    ate_s: Option<f64>,
    motor: EscolhaMotor,
) -> Result<String> {
    let ai = get_app_settings(app.clone()).await?.ai;
    em_fundo(move || {
        let ws = Path::new(&workspace_path);
        let cache = cache_do_caso(&app, ws)?;
        let pedido = PedidoSeguir { rastreio_id, clipe_id, ancora, sentido, ate_s, motor };
        let emitir: corrida::Emissor = Box::new(move |ev: EventoRastreio| {
            let _ = app.emit(EVENTO_RASTREIO, ev);
        });
        corrida::pedir(ws, &cache, &ai, &projeto, &pedido, emitir)
    })
    .await
}

/// Para a corrida (P2.2).
#[tauri::command]
pub async fn edicao_rastreio_cancelar(tarefa_id: String) -> Result<()> {
    em_fundo(move || corrida::cancelar(&tarefa_id)).await
}

/// "Apagar daqui em diante" e "Simplificar" (§6.3): arquivo denso novo e âncoras; o front aplica por `alterar()` (P2.2).
#[tauri::command]
pub async fn edicao_rastreio_editar(app: AppHandle, workspace_path: String, projeto: Value, rastreio_id: String, op: OpRastreio) -> Result<Editado> {
    em_fundo(move || {
        let ws = Path::new(&workspace_path);
        let cache = cache_do_caso(&app, ws)?;
        arquivo::editar(ws, &cache, &projeto, &rastreio_id, &op)
    })
    .await
}

/// "Da análise" (§6.4): a trilha da pessoa vira rastreio com denso copiado para o projeto (P2.5).
#[tauri::command]
pub async fn edicao_rastreio_de_pessoas(
    app: AppHandle,
    workspace_path: String,
    projeto_id: String,
    midia_id: String,
    media_sha256: String,
    pessoa_id: String,
) -> Result<Rastreio> {
    em_fundo(move || {
        let ws = Path::new(&workspace_path);
        let cache = cache_do_caso(&app, ws)?;
        crate::commands::pessoas_commands::rastreio_da_pessoa(ws, &cache, &projeto_id, &midia_id, &media_sha256, &pessoa_id)
    })
    .await
}

/// Intervalos ocultados no arquivo exportado, para "Conferir ocultações" (P2.6).
#[tauri::command]
pub async fn edicao_conferencia_trechos(app: AppHandle, workspace_path: String, saida_id: String) -> Result<Vec<TrechoConferencia>> {
    let cache = cache_do_caso(&app, Path::new(&workspace_path))?;
    em_fundo(move || conferencia::trechos_com_cache(Path::new(&workspace_path), &saida_id, Some(&cache))).await
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Os comandos montam num `generate_handler!` (argumentos e retornos conferidos pelo Tauri); a mesma lista do
    /// teste de registro em `lib.rs`.
    #[test]
    fn comandos_registraveis() {
        let _h: Box<dyn Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync> = Box::new(tauri::generate_handler![
            edicao_rastreio_ler,
            edicao_rastreio_seguir,
            edicao_rastreio_cancelar,
            edicao_rastreio_editar,
            edicao_rastreio_de_pessoas,
            edicao_conferencia_trechos,
        ]);
    }
}
