//! Régua de escala e velocidade exibida no rasterizador (§5.4, §5.5): só primitivas (`caminho`, `sprite`).
//! A conta da homografia e do texto mora em `edicao/medidas.rs`. Espelho no front: `palco/desenho/medidas.ts`, pela
//! fixture `primitivas-medidas.json`. Dono: P1.3.

use super::formas::{Primitiva, QuadroCamada};
use super::texto::FonteSprites;
use crate::edicao::modelo::{EfRegua, EfVelocidadeExibida};
use crate::edicao::mensagens::codigo;
use crate::error::{Result, SicroError};

/// Régua no quadro: linha das pontas, divisões e rótulo ("1 m"). Pontas por `medidas::pontas_da_regua` passadas por
/// `quadro.a`. Fase 0: `nao_suportado`.
pub fn regua(e: &EfRegua, quadro: &QuadroCamada, sprites: &dyn FonteSprites) -> Result<Vec<Primitiva>> {
    let _ = (e, quadro, sprites);
    Err(SicroError::Validation(format!("{}: regua", codigo::NAO_SUPORTADO)))
}

/// Velocidade no quadro: sprite do texto de `medidas::texto_da_velocidade` no lugar fixo ou ao lado do rastreio.
/// Fase 0: `nao_suportado`.
pub fn velocidade(e: &EfVelocidadeExibida, quadro: &QuadroCamada, sprites: &dyn FonteSprites) -> Result<Vec<Primitiva>> {
    let _ = (e, quadro, sprites);
    Err(SicroError::Validation(format!("{}: velocidade_exibida", codigo::NAO_SUPORTADO)))
}

#[cfg(test)]
mod testes {
    use serde_json::json;

    use crate::edicao::camada::formas::testes::{base, com, estilo, fixture, quadro, CAIXA, CANVAS};
    use crate::edicao::geometria::Afim;

    /// Fixture do espelho (`palco/desenho/medidas.ts`): `SICRO_GRAVAR_FIXTURES=1` regrava.
    #[test]
    fn fixture_primitivas_medidas() {
        let q = quadro(CANVAS, Afim::IDENTIDADE, &[("r1", CAIXA)]);
        let regua = com(base("regua", "g1"), json!({ "calibracao": { "id": "c", "metodo": "plane", "h": [1, 0, 0, 0, 1, 0, 0, 0, 1], "espaco": { "w": 1920, "h": 1080 },
            "pontos_sha256": "", "residuo_m": 0.01, "regiao": [] }, "ancora": { "x": 0.5, "y": 0.5 }, "direcao_graus": 0.0, "comprimento_m": 1.0,
            "divisoes": 10, "traco": { "cor": "#ffffff", "espessura": 0.004, "contorno": true }, "rotulo": true }));
        let velocidade = com(base("velocidade_exibida", "v1"), json!({ "calculo": { "id": "c", "kmh": 42.0, "faixa": [39.0, 45.0], "de_s": 0.0, "ate_s": 1.0, "metodo": "x" },
            "onde": { "modo": "fixa", "lugar": { "x": 0.5, "y": 0.9, "h": "centro", "v": "base" } }, "estilo": estilo("sans", false, false) }));
        fixture("primitivas-medidas.json", vec![("regua".into(), regua, q.clone()), ("velocidade_exibida".into(), velocidade, q)]);
    }
}
