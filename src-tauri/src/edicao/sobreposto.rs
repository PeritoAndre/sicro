//! Entrada e saída dos clipes sobrepostos (título, texto livre, imagem): a `Transformacao` avaliada num instante
//! local, com a opacidade de `entrada_s`/`saida_s` e (Fase 4) a `animacao` (corte, aparecer, deslizar).
//! Espelho: `src/modules/edicao/modelo/sobreposto.ts`, pela fixture `sobreposto-transformacao.json`.
//! Dono: pacote do texto livre (contratos-fase4.md); a camada global (`motor/sobrepor.rs`) só chama.

use crate::edicao::curva;
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{SobBase, Surgir};
use crate::edicao::motor::plano::TransformacaoAvaliada;
use crate::error::{Result, SicroError};

/// Deslocamento do "deslizar" no começo da entrada (e no fim da saída): fração do canvas no eixo do `lado`.
pub const DESLIZE: f64 = 0.08;

/// Detalhe do `nao_suportado` enquanto a `animacao` pedir o que o motor ainda não faz; None = renderiza.
/// Regra única do pacote: a validação e a camada global perguntam aqui (regra 2 de contratos-fase4.md).
pub fn recusa(b: &SobBase) -> Option<&'static str> {
    match b.animacao {
        Some(a) if a.entrada != Surgir::Aparecer || a.saida != Surgir::Aparecer => Some("animacao_sobreposto"),
        _ => None,
    }
}

/// A `recusa` como erro (`<código>: <detalhe> <id>`), para a camada global.
pub fn conferir(b: &SobBase) -> Result<()> {
    match recusa(b) {
        Some(d) => Err(SicroError::Validation(format!("{}: {d} {}", codigo::NAO_SUPORTADO, b.id))),
        None => Ok(()),
    }
}

/// `Transformacao` no tempo local `tau` de um clipe de `dur` s, com a entrada e a saída na opacidade
/// (rampa linear de `entrada_s` e `saida_s`; duração não positiva ou não finita = sem rampa).
pub fn transformacao_em(b: &SobBase, tau: f64, dur: f64) -> TransformacaoAvaliada {
    let t = &b.transformacao;
    let rampa = |v: f64, d: f64| if d > 0.0 && d.is_finite() { (v / d).clamp(0.0, 1.0) } else { 1.0 };
    let opacidade = curva::avaliar(&t.opacidade, tau).clamp(0.0, 1.0) * rampa(tau, b.entrada_s) * rampa(dur - tau, b.saida_s);
    TransformacaoAvaliada { x: curva::avaliar(&t.x, tau), y: curva::avaliar(&t.y, tau), escala: curva::avaliar(&t.escala, tau), rotacao: curva::avaliar(&t.rotacao, tau), opacidade }
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::{json, Value};

    fn base(extra: Value) -> SobBase {
        let mut v = json!({ "id": "s1", "rotulo": "", "pos": { "ref": "programa", "t": 0.0 }, "duracao_s": 2.0,
            "transformacao": { "x": 0.25, "y": [{ "t": 0.0, "v": 0.2 }, { "t": 2.0, "v": 0.8, "interp": "suave" }], "escala": 1.0, "rotacao": 30.0,
                               "opacidade": [{ "t": 0.0, "v": 1.0 }, { "t": 1.0, "v": 0.5 }] },
            "entrada_s": 0.5, "saida_s": 0.25, "efeitos": [] });
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        serde_json::from_value(v).unwrap()
    }

    /// Os casos do espelho (`modelo/sobreposto.ts`): sem `animacao` e com "aparecer" nos dois lados, que é o mesmo.
    #[test]
    fn fixture_transformacao() {
        let mut casos = Vec::new();
        for (nome, extra) in [
            ("sem animação", json!({})),
            ("aparecer nos dois lados", json!({ "animacao": { "entrada": "aparecer", "saida": "aparecer", "lado": "baixo" } })),
            ("sem rampa", json!({ "entrada_s": 0.0, "saida_s": -1.0 })),
        ] {
            let b = base(extra.clone());
            for tau in [0.0, 0.1, 0.25, 0.5, 1.0, 1.8, 1.9, 2.0, 2.5] {
                casos.push(json!({ "nome": format!("{nome} em {tau}"), "entrada": { "base": serde_json::to_value(&b).unwrap(), "tau": tau, "dur": 2.0 },
                                   "saida": serde_json::to_value(transformacao_em(&b, tau, 2.0)).unwrap() }));
            }
        }
        crate::edicao::curva::fixtures::conferir("sobreposto-transformacao.json", Value::Array(casos));
    }

    #[test]
    fn recusa_so_o_que_nao_e_aparecer() {
        assert_eq!(recusa(&base(json!({}))), None);
        assert_eq!(recusa(&base(json!({ "animacao": { "entrada": "aparecer", "saida": "aparecer", "lado": "esq" } }))), None);
        for (e, s) in [("corte", "aparecer"), ("aparecer", "deslizar"), ("deslizar", "corte")] {
            let b = base(json!({ "animacao": { "entrada": e, "saida": s, "lado": "dir" } }));
            assert_eq!(recusa(&b), Some("animacao_sobreposto"), "{e} {s}");
        }
    }
}
