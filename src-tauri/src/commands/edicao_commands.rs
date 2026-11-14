//! Comandos da Edição, Fase 0 (arquitetura §5.12). Dono: P0.7. Quem registra no `invoke_handler` é o P0.1, ao fechar a fase.
//! `workspace_path` = raiz do caso. Caminhos `relativo` das respostas: ao cache da Edição do caso
//! (`app_cache_dir()/edicao/<id do caso>/`), salvo onde o tipo diz outra coisa.
//! Fora daqui: `edicao_diagnostico_salvar` (P0.9, arquivo próprio), `edicao_refazer`, `edicao_origem_pedir` e `edicao_onda` (Fase 1).

// sai quando o P0.7 implementar
#![allow(unused_variables)]

use serde::Serialize;
use serde_json::Value;
use tauri::AppHandle;

use crate::edicao::armazem::{ProjetoAberto, ResumoProjeto, Salvo, UsoMidia};
use crate::edicao::camada::texto::{Atlas, PedidoSprite, Sprite};
use crate::edicao::ffmpeg::sonda::ResumoCapacidades;
use crate::edicao::fila::{SaidaRegistrada, Tarefa};
use crate::edicao::fundo::TarefaFundo;
use crate::edicao::modelo::{Analise, EstiloTexto, MidiaRef, TipoMidia};
use crate::edicao::motor::Alvo;
use crate::edicao::previa::{MidiaInfo, PreviaMidia};
use crate::edicao::quadros::{AlvoQuadro, QuadroEm, TabelaCompacta};
use crate::error::Result;

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

/// Projetos do caso; arquivo ilegível aparece com `problema`.
#[tauri::command]
pub async fn edicao_listar(workspace_path: String) -> Result<Vec<ResumoProjeto>> {
    todo!()
}

/// Abre o projeto (importa do v1 se for o caso) e devolve o documento como está no disco.
#[tauri::command]
pub async fn edicao_abrir(app: AppHandle, workspace_path: String, projeto_id: String) -> Result<ProjetoAberto> {
    todo!()
}

/// Grava o documento como chegou e devolve o `updated_at` gravado.
#[tauri::command]
pub async fn edicao_salvar(workspace_path: String, projeto: Value) -> Result<Salvo> {
    todo!()
}

/// Apaga o projeto e o cache dele; as exportações ficam.
#[tauri::command]
pub async fn edicao_excluir(app: AppHandle, workspace_path: String, projeto_id: String) -> Result<()> {
    todo!()
}

/// Duplica o projeto e devolve o resumo da cópia.
#[tauri::command]
pub async fn edicao_duplicar(workspace_path: String, projeto_id: String) -> Result<ResumoProjeto> {
    todo!()
}

/// Valida e resolve a linha do tempo do documento da tela (não grava nada). Documento que não lê tipado
/// não é `Err`: volta `Analise` com um problema `esquema` (ou `item_invalido`) e a linha vazia.
#[tauri::command]
pub async fn edicao_analisar(app: AppHandle, workspace_path: String, projeto: Value) -> Result<Analise> {
    todo!()
}

/// O que a tela precisa de uma mídia do caso (tamanho exibido, taxa, faixa de tempo, prévia).
#[tauri::command]
pub async fn edicao_midia_info(app: AppHandle, workspace_path: String, sha256: String) -> Result<MidiaInfo> {
    todo!()
}

/// Prévia da mídia; pede a cópia ou o proxy à fila de fundo quando falta (`pronta` false).
#[tauri::command]
pub async fn edicao_midia_previa(app: AppHandle, workspace_path: String, sha256: String) -> Result<PreviaMidia> {
    todo!()
}

/// Tabela de quadros compacta (a melhor que houver).
#[tauri::command]
pub async fn edicao_tabela_quadros(app: AppHandle, workspace_path: String, sha256: String) -> Result<TabelaCompacta> {
    todo!()
}

/// Quadro exibido num tempo, num índice ou num pts: é quem prende tempo a quadro.
#[tauri::command]
pub async fn edicao_quadro_em(app: AppHandle, workspace_path: String, sha256: String, alvo: AlvoQuadro) -> Result<QuadroEm> {
    todo!()
}

/// Copia um arquivo (`origem` = caminho absoluto) para a mídia própria do projeto.
#[tauri::command]
pub async fn edicao_midia_importar(workspace_path: String, projeto_id: String, origem: String, tipo: TipoMidia) -> Result<MidiaRef> {
    todo!()
}

/// Projetos e exportações que usam a mídia (para o diálogo de exclusão de vídeo).
#[tauri::command]
pub async fn edicao_usos(workspace_path: String, sha256: String) -> Result<Vec<UsoMidia>> {
    todo!()
}

/// Narração gravada: cabeçalhos `CABECALHO_*`, corpo = bytes crus. No Linux o corpo pode chegar como
/// `InvokeBody::Json` (lista de números) quando o IPC cai para `postMessage`: aceitar os dois.
#[tauri::command]
pub async fn edicao_gravacao_salvar(request: tauri::ipc::Request<'_>) -> Result<MidiaRef> {
    todo!()
}

/// Sprite de texto para a prévia (o MESMO do render).
#[tauri::command]
pub async fn edicao_sprite_texto(app: AppHandle, workspace_path: String, pedido: PedidoSprite) -> Result<Sprite> {
    todo!()
}

/// Atlas de glifos do carimbo nesse estilo; `altura_canvas` em px.
#[tauri::command]
pub async fn edicao_atlas_carimbo(app: AppHandle, workspace_path: String, estilo: EstiloTexto, altura_canvas: u32) -> Result<Atlas> {
    todo!()
}

/// Resumo das capacidades do ffmpeg; `testar_de_novo` refaz a sonda de GPU.
#[tauri::command]
pub async fn edicao_capacidades(testar_de_novo: bool) -> Result<ResumoCapacidades> {
    todo!()
}

/// Enfileira um render do documento da tela e devolve o id da tarefa. `ciencias` = códigos periciais aceitos.
#[tauri::command]
pub async fn edicao_enfileirar(app: AppHandle, workspace_path: String, projeto: Value, alvo: Alvo, ciencias: Vec<String>) -> Result<String> {
    todo!()
}

/// Quadro final do slot `q` pelo motor real, com `largura` px (sem `-hwaccel`).
#[tauri::command]
pub async fn edicao_quadro_final(app: AppHandle, workspace_path: String, projeto: Value, q: u64, largura: u32) -> Result<QuadroFinal> {
    todo!()
}

/// Fecha uma saída em "A conferir" (conferida ou não) e conclui o relatório.
#[tauri::command]
pub async fn edicao_conferir(app: AppHandle, workspace_path: String, saida_id: String, conferida: bool) -> Result<SaidaRegistrada> {
    todo!()
}

/// Cancela uma tarefa da fila de render.
#[tauri::command]
pub async fn edicao_cancelar(tarefa_id: String) -> Result<()> {
    todo!()
}

/// Tarefas da fila de render (de todos os casos; a tela filtra por `caso`).
#[tauri::command]
pub async fn edicao_fila() -> Result<Vec<Tarefa>> {
    todo!()
}

/// Tira da fila as tarefas terminadas e devolve as que ficam.
#[tauri::command]
pub async fn edicao_fila_limpar() -> Result<Vec<Tarefa>> {
    todo!()
}

/// Tarefas da fila de fundo.
#[tauri::command]
pub async fn edicao_fundo() -> Result<Vec<TarefaFundo>> {
    todo!()
}

/// Saídas registradas do caso, ou só as de um projeto.
#[tauri::command]
pub async fn edicao_saidas(workspace_path: String, projeto_id: Option<String>) -> Result<Vec<SaidaRegistrada>> {
    todo!()
}

/// Exclui uma exportação: linha do banco, retrato e a pasta da saída (somente leitura incluída); audita. Erro se estiver na fila.
#[tauri::command]
pub async fn edicao_saida_excluir(app: AppHandle, workspace_path: String, saida_id: String) -> Result<()> {
    todo!()
}

#[cfg(test)]
mod testes {
    use super::*;

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
        ]);
    }
}
