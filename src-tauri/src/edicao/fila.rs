//! Fila de render (§5.10) e registro das saídas (§4, decisão O7). Dono: P0.7.
//! A fila é do processo (não é campo de `AppState`): uma `Exportar`/`Refazer` por vez, em ordem;
//! `Previa` e `QuadroFinal` em faixa própria (a nova cancela a anterior do mesmo projeto).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::executar::{Autor, Progresso};
use super::modelo::Predefinicao;
use super::motor::plano::Plano;
use super::motor::Alvo;
use crate::error::Result;

/// O que a tarefa produz.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TipoTarefa {
    Exportar,
    Previa,
    QuadroFinal,
    Refazer,
}

/// `Blocos`: blocos prontos e totais ("Vídeo 3/7").
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Fase {
    Previas,
    Camadas,
    Blocos { feitos: u32, total: u32 },
    Audio,
    Juncao,
    Conferindo,
    Relatorio,
    Pacote,
}

/// `Rodando`: slots feitos e totais, quadros por segundo e segundos que faltam.
/// `AConferir` e `Concluida`: exportação final registrada. `Pronta`: fim de `Previa` e `QuadroFinal`, com o arquivo
/// (relativo ao cache da Edição do caso). `Falhou`: mensagem curta classificada e o registro (relativo ao mesmo cache).
// CONTRATO: `Pronta` não está no §5.10; sem ele a prévia do render (Fase 0) não tinha como chegar à tela.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Estado {
    NaFila,
    Preparando,
    Rodando { fase: Fase, feito_q: u64, total_q: u64, qps: f32, restante_s: u32 },
    AConferir { saida_id: String },
    Concluida { saida_id: String },
    Pronta { relativo: String },
    Falhou { mensagem: String, log_rel: String },
    Cancelada,
}

/// `caso` = id do caso; `criado_em` em RFC 3339.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tarefa {
    pub id: String,
    pub tipo: TipoTarefa,
    pub caso: String,
    pub projeto_id: String,
    pub titulo: String,
    pub estado: Estado,
    pub criado_em: String,
}

/// Evento `edicao:progresso`: `{ tarefa_id, fase, feito_q, total_q, qps, restante_s }`, até 5 por segundo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventoProgresso {
    pub tarefa_id: String,
    #[serde(flatten)]
    pub progresso: Progresso,
}

/// Nomes dos eventos. `edicao:fila` leva `Vec<Tarefa>` a cada mudança.
pub const EVENTO_FILA: &str = "edicao:fila";
pub const EVENTO_PROGRESSO: &str = "edicao:progresso";

/// Pedido de render. `projeto` = o documento da tela (vira retrato em `render/<tarefa>/projeto/`);
/// `ws` = raiz do caso; `cache` = pasta da Edição do caso; `ciencias` = códigos periciais de que o perito deu ciência.
#[derive(Debug, Clone, PartialEq)]
pub struct PedidoRender {
    pub tipo: TipoTarefa,
    pub ws: PathBuf,
    pub cache: PathBuf,
    pub projeto: Value,
    pub alvo: Alvo,
    pub ciencias: Vec<String>,
}

/// Quem recebe os eventos da fila (o integrador liga ao `AppHandle`; os testes usam um falso).
pub trait Ouvinte: Send + Sync {
    /// Lista resumida, a cada mudança.
    fn fila(&self, tarefas: &[Tarefa]);
    /// Avanço da tarefa que roda.
    fn progresso(&self, evento: &EventoProgresso);
}

/// Liga o ouvinte dos eventos (uma vez, na partida).
pub fn ouvir(ouvinte: Box<dyn Ouvinte>) {
    todo!()
}

/// Enfileira e devolve o id da tarefa. Grava o retrato do projeto antes de voltar.
pub fn enfileirar(pedido: PedidoRender) -> Result<String> {
    todo!()
}

/// Cancela a tarefa (na fila ou rodando): mata os processos dela e apaga a saída parcial.
pub fn cancelar(tarefa_id: &str) -> Result<()> {
    todo!()
}

/// Tarefas atuais, na ordem de entrada.
pub fn listar() -> Vec<Tarefa> {
    todo!()
}

/// Tira da lista as tarefas terminadas (concluída, falhou, cancelada) e devolve as que ficam.
pub fn limpar() -> Vec<Tarefa> {
    todo!()
}

/// Espera a tarefa terminar e devolve o estado final (quadro final e testes).
pub fn esperar(tarefa_id: &str) -> Result<Estado> {
    todo!()
}

/// Cancela tudo e mata os filhos (chamada no `ExitRequested`).
pub fn encerrar() {
    todo!()
}

// ---------------------------------------------------------------------------
// Saídas registradas e retrato da exportação

/// Coluna `estado` de `video_edit_outputs`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EstadoSaida {
    AConferir,
    Concluida,
}

/// Linha de `video_edit_outputs` (migração 027), como a tela a recebe. Caminhos (`pasta`, `arquivo`, `tecnico_rel`,
/// `retrato_rel`) relativos ao caso; `tamanho` em bytes; datas em RFC 3339.
/// `decodificado_sha256`, `audio_sha256` e `tecnico_rel` None enquanto não existem (`tecnico_rel` só sai depois de `edicao_conferir`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SaidaRegistrada {
    pub id: String,
    pub projeto_id: String,
    pub titulo: String,
    pub pasta: String,
    pub arquivo: String,
    pub sha256: String,
    pub composto_sha256: String,
    pub decodificado_sha256: Option<String>,
    pub audio_sha256: Option<String>,
    pub tamanho: u64,
    pub duracao_s: f64,
    pub n_quadros: u64,
    pub predefinicao: Predefinicao,
    pub codificador: String,
    pub tecnico_rel: Option<String>,
    pub retrato_rel: String,
    pub projeto_sha256: String,
    pub autor: Autor,
    pub refeita_de: Option<String>,
    pub estado: EstadoSaida,
    /// Tem ocultação ou silêncio. Concluída com `exige_conferencia` e sem `conferida_por` = não conferida.
    pub exige_conferencia: bool,
    pub conferida_por: Option<String>,
    pub conferida_em: Option<String>,
    pub criado_em: String,
}

/// O que é o arquivo guardado no retrato.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PapelRetrato {
    Denso,
    Silhuetas,
    Trf,
    Midia,
}

/// Um arquivo que o retrato cita: guardado em `dados/<sha256>.<ext>` (ligação física, ou cópia quando o sistema
/// de arquivos não deixa). `de` = caminho de origem relativo à pasta do projeto; `tamanho` em bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemManifesto {
    pub papel: PapelRetrato,
    pub sha256: String,
    pub arquivo: String,
    pub de: String,
    pub tamanho: u64,
}

/// `manifesto.json` do retrato autossuficiente (O7): `exportacoes/<saida_id>/` tem `projeto.json`, este manifesto,
/// `grafos/` e `dados/`. O "refazer" lê só daqui. `alvo` = `Plano.alvo` da exportação (o "refazer" recompila com ele).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifesto {
    pub esquema: u32,
    pub saida_id: String,
    pub projeto_id: String,
    pub projeto_sha256: String,
    pub alvo: Alvo,
    pub itens: Vec<ItemManifesto>,
}

/// Grava o retrato em `Sistema/videos/edicao/<projeto_id>/exportacoes/<saida_id>/`: o projeto como chegou,
/// os grafos e listas do plano e tudo que o projeto cita (densos, silhuetas, `.trf`, mídia própria).
pub fn gravar_retrato(ws: &Path, projeto_id: &str, saida_id: &str, projeto: &Value, plano: &Plano) -> Result<Manifesto> {
    todo!()
}

/// Lê o retrato e confere o SHA-256 de cada item.
pub fn ler_retrato(ws: &Path, projeto_id: &str, saida_id: &str) -> Result<(Value, Manifesto)> {
    todo!()
}

/// Saídas registradas do caso (as de um projeto quando `projeto_id` é Some), da mais nova para a mais antiga.
pub fn saidas(ws: &Path, projeto_id: Option<&str>) -> Result<Vec<SaidaRegistrada>> {
    todo!()
}

/// Fecha uma saída em "A conferir": grava `conferida_por`/`conferida_em` (ou "não conferida"), gera o `tecnico.json`
/// e marca concluída. `por` = nome do perito.
pub fn conferir_saida(ws: &Path, saida_id: &str, conferida: bool, por: &str) -> Result<SaidaRegistrada> {
    todo!()
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde::de::DeserializeOwned;
    use serde_json::json;

    fn ida_e_volta<T: Serialize + DeserializeOwned>(j: Value) {
        let lido: T = serde_json::from_value(j.clone()).expect("lê");
        assert_eq!(serde_json::to_value(&lido).unwrap(), j);
    }

    #[test]
    fn fase_tag_tipo() {
        ida_e_volta::<Fase>(json!({ "tipo": "previas" }));
        ida_e_volta::<Fase>(json!({ "tipo": "blocos", "feitos": 3, "total": 7 }));
        ida_e_volta::<Fase>(json!({ "tipo": "conferindo" }));
    }

    #[test]
    fn estado_tag_tipo() {
        ida_e_volta::<Estado>(json!({ "tipo": "na_fila" }));
        ida_e_volta::<Estado>(json!({ "tipo": "preparando" }));
        ida_e_volta::<Estado>(json!({ "tipo": "rodando", "fase": { "tipo": "blocos", "feitos": 1, "total": 2 },
                                       "feito_q": 300, "total_q": 1419, "qps": 112.5, "restante_s": 10 }));
        ida_e_volta::<Estado>(json!({ "tipo": "a_conferir", "saida_id": "s1" }));
        ida_e_volta::<Estado>(json!({ "tipo": "concluida", "saida_id": "s1" }));
        ida_e_volta::<Estado>(json!({ "tipo": "pronta", "relativo": "render/t1/previa.mp4" }));
        ida_e_volta::<Estado>(json!({ "tipo": "falhou", "mensagem": "disco cheio", "log_rel": "render/t1/b000.log" }));
        ida_e_volta::<Estado>(json!({ "tipo": "cancelada" }));
        ida_e_volta::<Tarefa>(json!({ "id": "t1", "tipo": "quadro_final", "caso": "c1", "projeto_id": "p1", "titulo": "Teste",
                                       "estado": { "tipo": "na_fila" }, "criado_em": "2026-11-14T03:00:00Z" }));
    }

    #[test]
    fn evento_de_progresso_achatado() {
        ida_e_volta::<EventoProgresso>(json!({ "tarefa_id": "t1", "fase": { "tipo": "audio" }, "feito_q": 10, "total_q": 20, "qps": 30.0, "restante_s": 1 }));
    }

    #[test]
    fn saida_registrada_literais() {
        assert_eq!(serde_json::to_value(EstadoSaida::AConferir).unwrap(), json!("a_conferir"));
        assert_eq!(serde_json::to_value(TipoTarefa::QuadroFinal).unwrap(), json!("quadro_final"));
        assert_eq!(serde_json::to_value(PapelRetrato::Trf).unwrap(), json!("trf"));
    }
}
