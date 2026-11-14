//! Lupa no rasterizador (§5.4, §5.5): a primitiva `lupa` (origem na fonte, destino no canvas, forma) mais a moldura
//! e a ligação, desenhadas na camada de anotação. Os pixels ampliados são do ffmpeg (`motor/efeitos/lupa.rs`).
//! Espelho no front: `palco/desenho/lupa.ts`, pela fixture `primitivas-lupa.json`. Dono: P1.2.

use super::formas::{Primitiva, QuadroCamada};
use super::texto::FonteSprites;
use super::Bitmap;
use crate::edicao::modelo::{EfLupa, TamanhoPx};
use crate::edicao::mensagens::codigo;
use crate::error::{Result, SicroError};

/// Primitivas da lupa no quadro: `Primitiva::Lupa` + moldura (caminho ou elipse) + ligação. O rastreio do alvo vem em
/// `quadro.rastreios[e.rastreio]`; sem ele, vazio. Fase 0: `nao_suportado` (o motor e a validação recusam antes).
pub fn primitivas(e: &EfLupa, quadro: &QuadroCamada, sprites: &dyn FonteSprites) -> Result<Vec<Primitiva>> {
    let _ = (e, quadro, sprites);
    Err(SicroError::Validation(format!("{}: lupa", codigo::NAO_SUPORTADO)))
}

/// Máscara da forma (cinza, branco = mostra a ampliação) das `Primitiva::Lupa` de um quadro, em `caixa_px` do
/// `canvas`: é a camada `MascaraCanvas` que `motor/efeitos/lupa.rs` pede ao montador para o `alphamerge` (o
/// rasterizador chama aqui quando o quadro traz lupa; `mascara::forma_lupa` desenha a forma). Fase 0: nunca chamada.
// CONTRATO: acréscimo da revisão da abertura (sem ele a máscara de uma lupa saía vazia: o rasterizador só via `Regiao`).
pub fn mascara(lupas: &[Primitiva], canvas: TamanhoPx, caixa_px: [u32; 4]) -> Result<Bitmap> {
    let _ = (lupas, canvas, caixa_px);
    todo!("P1.2")
}

#[cfg(test)]
mod testes {
    use serde_json::json;

    use crate::edicao::camada::formas::testes::{base, com, fixture, quadro, CAIXA, CANVAS};
    use crate::edicao::geometria::Afim;

    /// Fixture do espelho (`palco/desenho/lupa.ts`): `SICRO_GRAVAR_FIXTURES=1` regrava.
    #[test]
    fn fixture_primitivas_lupa() {
        let q = quadro(CANVAS, Afim::IDENTIDADE, &[("r1", CAIXA)]);
        let lupa = com(base("lupa", "l1"), json!({ "rastreio": "r1", "janela": 0.2, "fator": 2.0, "forma": "circulo", "interpolacao": "bilinear",
            "onde": { "modo": "fixa", "x": 0.8, "y": 0.2 }, "moldura": { "cor": "#ffffff", "espessura": 0.004, "contorno": true }, "ligacao": true }));
        fixture("primitivas-lupa.json", vec![("lupa".into(), lupa, q)]);
    }
}
