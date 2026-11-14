//! Régua de escala e velocidade exibida (§2.1 `EfRegua`/`EfVelocidadeExibida`, §3.2): retratos lidos das análises
//! (calibração e cálculo de velocidade do banco do caso), pontas da régua pela homografia, texto da velocidade sempre
//! com faixa ou "≈", e as regras que a validação pergunta. O desenho é de `camada/medidas.rs`. Dono: P1.3.

use std::path::Path;

use crate::edicao::modelo::{Efeito, EfRegua, EfVelocidadeExibida, Ponto, Problema, ProjetoEdicao, RetratoCalculo, RetratoCalibracao};
use crate::error::Result;

/// Detalhe do `nao_suportado` enquanto régua e velocidade não renderizam; None = renderizam (ou outro efeito).
/// Uma regra só: `validar` e `clipe::cadeia` perguntam aqui (contratos (h) 1).
pub fn recusa(e: &Efeito) -> Option<&'static str> {
    match e {
        Efeito::Regua(_) => Some("regua"),
        Efeito::VelocidadeExibida(_) => Some("velocidade_exibida"),
        _ => None,
    }
}

/// Retrato da calibração `id` da análise (`list_speed_calibrations`), no espaço de pixel em que foi marcada (§3.2).
/// Erro `calibracao_ausente` se não existe. Lê o banco do caso.
pub fn retrato_calibracao(ws: &Path, id: &str) -> Result<RetratoCalibracao> {
    let _ = (ws, id);
    todo!("P1.3")
}

/// Retrato do cálculo de velocidade `id` (`list_speed_calculations`): km/h, faixa, intervalo medido e método.
/// Erro `calculo_ausente` se não existe.
pub fn retrato_calculo(ws: &Path, id: &str) -> Result<RetratoCalculo> {
    let _ = (ws, id);
    todo!("P1.3")
}

/// Pontas e divisões da régua em 0..1 da origem do clipe: âncora no chão, direção e comprimento pela homografia do
/// retrato (`line`, `plane`, `cross_ratio`). Primeiro e último = pontas.
pub fn pontas_da_regua(e: &EfRegua) -> Result<Vec<Ponto>> {
    let _ = e;
    todo!("P1.3")
}

/// Texto da velocidade: com a faixa ("42 km/h (39 a 45)") ou com "≈" quando o cálculo não tem faixa.
pub fn texto_da_velocidade(c: &RetratoCalculo) -> String {
    let _ = c;
    todo!("P1.3")
}

/// A velocidade aparece fora de `[calculo.de_s, calculo.ate_s]`? `t_fonte` = tempo de FONTE do quadro mostrado num
/// instante em que o efeito está ativo (trecho: o quadro lido; congelado: o quadro congelado; o `ativo` do congelado
/// é local e não se compara com o cálculo); None = clipe sem fonte (foto, título). A validação acusa
/// `velocidade_fora_do_intervalo` (O5). Fase 0: nunca (o efeito é recusado antes).
// CONTRATO: era `t: f64` no instante do `ativo`; no congelado esse instante é local (revisão da abertura).
pub fn fora_do_intervalo(e: &EfVelocidadeExibida, t_fonte: Option<f64>) -> bool {
    let _ = (e, t_fonte);
    false
}

/// Avisos `retrato_desatualizado` (§2.3): cálculo ou calibração que mudaram na análise depois do retrato gravado no
/// projeto. `edicao_analisar` acrescenta à `Analise`. Fase 0: nenhum.
pub fn desatualizados(ws: &Path, p: &ProjetoEdicao) -> Vec<Problema> {
    let _ = (ws, p);
    Vec::new()
}
