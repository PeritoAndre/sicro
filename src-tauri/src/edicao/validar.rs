//! Validação única (§2.3): o front não valida, só mostra. Dono: P0.R.

use serde_json::Value;

use super::modelo::{Analise, LinhaResolvida, Problema, ProjetoEdicao};
use super::motor::{Alvo, Contexto};

/// Problemas que se veem no JSON cru, antes da leitura tipada: `esquema`, `esquema_menor`, id.
pub fn esquema(bruto: &Value) -> Vec<Problema> {
    todo!()
}

/// Todas as regras do §2.3 sobre o projeto tipado; a situação das mídias vem de `ctx.midias`.
/// `alvo` Some(Final) acrescenta as regras da exportação final (`tabela_pendente`, limites do codec).
/// É a mesma função que `motor::compilar` chama antes de compilar.
pub fn validar(p: &ProjetoEdicao, linha: &LinhaResolvida, ctx: &Contexto, alvo: Option<&Alvo>) -> Vec<Problema> {
    todo!()
}

/// Resposta de `edicao_analisar`: resolve a linha, valida, lista os tempos a prender e a saída automática
/// (`Analise.midias` = cópia de `ctx.midias`).
pub fn analisar(p: &ProjetoEdicao, ctx: &Contexto, alvo: Option<&Alvo>) -> Analise {
    todo!()
}

/// Há algum problema de nível erro?
pub fn tem_erro(problemas: &[Problema]) -> bool {
    todo!()
}

/// Códigos periciais presentes de que o perito ainda não deu ciência (`ciencias` = códigos já aceitos).
pub fn sem_ciencia(problemas: &[Problema], ciencias: &[String]) -> Vec<&'static str> {
    todo!()
}
