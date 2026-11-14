//! Fila de fundo (§5.10): tabelas de quadros, cópias de prévia, proxies, intermediários, sonda. Dono: P0.7.
//! Do processo, 2 trabalhadores de baixa prioridade, chave única por trabalho. A tela nunca espera.

use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Tipo do trabalho de fundo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TipoFundo {
    Tabela,
    CopiaPrevia,
    Proxy,
    Origem,
    Onda,
    Tira,
    Sonda,
}

/// Situação de um trabalho de fundo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum EstadoFundo {
    NaFila,
    Rodando,
    Pronta,
    Falhou { mensagem: String },
    Cancelada,
}

/// `chave` = "<tipo>:<sha256 da mídia ou hash do pedido>" (ex.: "tabela:ab12…"; a sonda de GPU é "sonda:gpu");
/// `feito` em 0..1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TarefaFundo {
    pub id: String,
    pub tipo: TipoFundo,
    pub chave: String,
    pub feito: f32,
    pub estado: EstadoFundo,
}

/// Evento `edicao:fundo`: leva `Vec<TarefaFundo>` (as vivas e as terminadas há pouco) a cada mudança.
pub const EVENTO_FUNDO: &str = "edicao:fundo";

/// O trabalho: recebe o sinal de cancelamento e uma função de progresso (0..1).
pub type Trabalho = Box<dyn FnOnce(&AtomicBool, &mut dyn FnMut(f32)) -> Result<()> + Send + 'static>;

/// Monta a chave de um trabalho.
pub fn chave(tipo: TipoFundo, alvo: &str) -> String {
    todo!()
}

/// Pede um trabalho e devolve o id da tarefa. Chave repetida (na fila ou rodando) reaproveita a tarefa e descarta `trabalho`.
pub fn pedir(tipo: TipoFundo, chave: &str, trabalho: Trabalho) -> String {
    todo!()
}

/// Tarefa dessa chave, se houver.
pub fn estado(chave: &str) -> Option<TarefaFundo> {
    todo!()
}

/// Tarefas atuais.
pub fn listar() -> Vec<TarefaFundo> {
    todo!()
}

/// Liga quem recebe a lista a cada mudança (o integrador emite `edicao:fundo`).
pub fn ouvir(ouvinte: Box<dyn Fn(&[TarefaFundo]) + Send + Sync>) {
    todo!()
}

/// Cancela tudo e espera os trabalhadores (chamada no `ExitRequested`).
pub fn encerrar() {
    todo!()
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    #[test]
    fn estado_de_fundo_tag_tipo() {
        for j in [json!({ "tipo": "na_fila" }), json!({ "tipo": "rodando" }), json!({ "tipo": "pronta" }),
                  json!({ "tipo": "falhou", "mensagem": "x" }), json!({ "tipo": "cancelada" })] {
            let e: EstadoFundo = serde_json::from_value(j.clone()).expect("lê");
            assert_eq!(serde_json::to_value(&e).unwrap(), j);
        }
        let t = json!({ "id": "f1", "tipo": "copia_previa", "chave": "copia_previa:ab12", "feito": 0.5, "estado": { "tipo": "rodando" } });
        let lida: TarefaFundo = serde_json::from_value(t.clone()).unwrap();
        assert_eq!(serde_json::to_value(&lida).unwrap(), t);
    }
}
