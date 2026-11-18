//! Zoom de câmera que acompanha a pessoa (Fase 4): o centro da região do `EfEnquadramento` segue um rastreio,
//! suavizado; o tamanho vem de `regiao` (fixa ou com chaves); a região nunca sai da imagem (desliza na borda).
//! A cauda do clipe (`motor/clipe.rs`) troca o efeito por um com chaves por slot ANTES do enquadramento e de `A(τ)`:
//! o resto do motor (perspective, camadas, lupa) não muda. Espelho: `src/modules/edicao/modelo/segue.ts`, pelas
//! fixtures `segue-*.json`. Dono: pacote do zoom que segue (contratos-fase4.md); corpo `todo!()` atrás da `recusa`.

use serde::{Deserialize, Serialize};

use crate::edicao::modelo::{Caixa, EfEnquadramento, Efeito, Ponto, ProjetoEdicao, Tolerante};
use crate::edicao::motor::clipe::Rastreios;
use crate::edicao::motor::Contexto;
use crate::error::Result;

/// Suavização que "Acompanhar rastreio" grava (s, na base do rastreio).
pub const SUAVIZAR_PADRAO_S: f64 = 1.0;

/// Detalhe do `nao_suportado` enquanto o pacote não entrega; None = renderiza. Regra única: a validação e o
/// despacho do motor (`clipe::recusa_do_efeito`) perguntam aqui.
pub fn recusa(q: &EfEnquadramento) -> Option<&'static str> {
    q.segue.as_ref().map(|_| "enquadramento_segue")
}

/// Problemas do parâmetro (código do catálogo, campo) que a validação acusa no efeito, além da conferência do
/// rastreio seguido (existe, mídia, espaço, confirmação: a mesma do destaque, já feita pela validação): por exemplo
/// `suavizar_s` fora da faixa (`item_invalido`, campo `segue`). Vazio enquanto o pacote não entrega.
pub fn problemas(q: &EfEnquadramento, p: &ProjetoEdicao, midia_do_clipe: Option<&str>) -> Vec<(&'static str, &'static str)> {
    let _ = (q, p, midia_do_clipe);
    Vec::new()
}

/// Centro da caixa do rastreio num instante da base dele; None = sem caixa ali (ausente, antes da primeira âncora).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct AmostraSegue {
    pub t: f64,
    pub centro: Option<Ponto>,
}

/// Região (0..1 da origem já girada) em cada instante de `em` (base do clipe): centro = média das `amostras` em
/// [t − suavizar_s/2, t + suavizar_s/2] (sem amostra na janela: a mais próxima; nenhuma: o centro de `regiao`),
/// tamanho = `regiao` em t, deslocada para dentro de [0, 1]² (lado maior que 1 fica 1, em x = 0).
pub fn regiao_seguida(e: &EfEnquadramento, amostras: &[AmostraSegue], em: &[f64]) -> Vec<Caixa> {
    let _ = (e, amostras, em);
    todo!("pacote do zoom que segue (contratos-fase4.md)")
}

/// Gancho da cauda do clipe: os efeitos com o enquadramento que segue trocado por um de chaves nos instantes `taus`
/// (os slots da cauda, na base do clipe). None = nenhum enquadramento ligado segue (o caso de todo projeto antigo).
pub(crate) fn na_cauda(efeitos: &[Tolerante<Efeito>], taus: &[f64], rastreios: &mut Rastreios<'_>, ctx: &Contexto<'_>) -> Result<Option<Vec<Tolerante<Efeito>>>> {
    let segue = efeitos.iter().any(|e| matches!(e, Tolerante::Conhecido(Efeito::Enquadramento(q)) if q.base.ligado && q.segue.is_some()));
    if !segue {
        return Ok(None);
    }
    // atrás da recusa: `clipe::recusar_de_outra_fase` para antes de chegar aqui
    let _ = (taus, rastreios, ctx);
    todo!("pacote do zoom que segue (contratos-fase4.md)")
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    fn enq(segue: serde_json::Value) -> EfEnquadramento {
        let mut v = json!({ "id": "q1", "ligado": true, "nome": "", "ativo": null, "regiao": { "x": 0.25, "y": 0.25, "w": 0.5, "h": 0.5 },
                            "ajuste": "cobrir", "interpolacao": "bilinear" });
        if !segue.is_null() {
            v["segue"] = segue;
        }
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn recusa_so_com_segue() {
        assert_eq!(recusa(&enq(serde_json::Value::Null)), None);
        assert_eq!(recusa(&enq(json!({ "rastreio": "r1", "suavizar_s": SUAVIZAR_PADRAO_S }))), Some("enquadramento_segue"));
    }
}
