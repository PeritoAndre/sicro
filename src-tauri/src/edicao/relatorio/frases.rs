//! Catálogo fixo das frases do relatório (§10.3): uma por tipo de efeito e por condição detectada, para o texto não
//! variar de caso para caso. Nenhuma frase do PDF é montada fora daqui. Dono: P1.R.

/// Frase de "O que foi acrescentado e modificado" para o tipo de efeito (o `tipo` do fio).
pub fn do_efeito(tipo: &str) -> Option<&'static str> {
    let _ = tipo;
    todo!("P1.R")
}

/// Frase de "Limites" para o código do catálogo (`mensagens::codigo`) que aparece no `feito.avisos`.
pub fn da_condicao(codigo: &str) -> Option<&'static str> {
    let _ = codigo;
    todo!("P1.R")
}
