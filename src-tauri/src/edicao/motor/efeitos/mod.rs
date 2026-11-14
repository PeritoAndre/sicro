//! Efeito → mecanismo (§5.4): enquadramento, região (ocultar, holofote), anotação; `lupa.rs` (P1.2) e
//! `realce.rs` (P1.4) na Fase 1. Compartilhado (integrador) desde o P0.8.

use crate::edicao::modelo::{EfBase, Efeito, Tolerante};

pub mod anotacao;
pub mod enquadramento;
pub mod lupa;
pub mod realce;
pub mod regiao;

/// Família do efeito: decide o ponto da cadeia em que ele entra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Familia {
    Origem,
    Enquadre,
    Regiao,
    Anotacao,
}

/// Família de um efeito.
pub fn familia(e: &Efeito) -> Familia {
    match e {
        Efeito::Estabilizar(_) | Efeito::Lente(_) | Efeito::Desentrelacar(_) | Efeito::MediaTemporal(_) | Efeito::Ajustes(_) | Efeito::Nitidez(_) => {
            Familia::Origem
        }
        Efeito::Girar(_) | Efeito::Enquadramento(_) => Familia::Enquadre,
        Efeito::Ocultar(_) | Efeito::Holofote(_) => Familia::Regiao,
        Efeito::Destaque(_) | Efeito::Lupa(_) | Efeito::Regua(_) | Efeito::VelocidadeExibida(_) | Efeito::Texto(_) | Efeito::Carimbo(_) | Efeito::Imagem(_) => {
            Familia::Anotacao
        }
    }
}

/// Base comum do efeito.
pub(crate) fn base(e: &Efeito) -> &EfBase {
    match e {
        Efeito::Estabilizar(x) => &x.base,
        Efeito::Lente(x) => &x.base,
        Efeito::Desentrelacar(x) => &x.base,
        Efeito::MediaTemporal(x) => &x.base,
        Efeito::Ajustes(x) => &x.base,
        Efeito::Nitidez(x) => &x.base,
        Efeito::Girar(x) => &x.base,
        Efeito::Enquadramento(x) => &x.base,
        Efeito::Ocultar(x) => &x.base,
        Efeito::Holofote(x) => &x.base,
        Efeito::Destaque(x) => &x.base,
        Efeito::Lupa(x) => &x.base,
        Efeito::Regua(x) => &x.base,
        Efeito::VelocidadeExibida(x) => &x.base,
        Efeito::Texto(x) => &x.base,
        Efeito::Carimbo(x) => &x.base,
        Efeito::Imagem(x) => &x.base,
    }
}

/// Literal do `tipo` do efeito (o do fio).
pub(crate) fn tipo(e: &Efeito) -> &'static str {
    match e {
        Efeito::Estabilizar(_) => "estabilizar",
        Efeito::Lente(_) => "lente",
        Efeito::Desentrelacar(_) => "desentrelacar",
        Efeito::MediaTemporal(_) => "media_temporal",
        Efeito::Ajustes(_) => "ajustes",
        Efeito::Nitidez(_) => "nitidez",
        Efeito::Girar(_) => "girar",
        Efeito::Enquadramento(_) => "enquadramento",
        Efeito::Ocultar(_) => "ocultar",
        Efeito::Holofote(_) => "holofote",
        Efeito::Destaque(_) => "destaque",
        Efeito::Lupa(_) => "lupa",
        Efeito::Regua(_) => "regua",
        Efeito::VelocidadeExibida(_) => "velocidade_exibida",
        Efeito::Texto(_) => "texto",
        Efeito::Carimbo(_) => "carimbo",
        Efeito::Imagem(_) => "imagem",
    }
}

/// Rastreio que a anotação segue (a caixa avaliada vai em `QuadroCamada.rastreios`): destaque, lupa e velocidade que
/// acompanha. Região (`ocultar`, holofote) avalia o dela à parte.
pub(crate) fn rastreio_seguido(e: &Efeito) -> Option<&str> {
    match e {
        Efeito::Destaque(d) => Some(&d.rastreio),
        Efeito::Lupa(l) => Some(&l.rastreio),
        Efeito::VelocidadeExibida(v) => match &v.onde {
            crate::edicao::modelo::OndeVelocidade::Segue { rastreio, .. } => Some(rastreio),
            crate::edicao::modelo::OndeVelocidade::Fixa { .. } => None,
        },
        _ => None,
    }
}

/// Efeitos conhecidos e ligados de uma família, na ordem de aplicação.
pub fn ligados(efeitos: &[Tolerante<Efeito>], familia_pedida: Familia) -> Vec<&Efeito> {
    efeitos
        .iter()
        .filter_map(|e| match e {
            Tolerante::Conhecido(e) if base(e).ligado && familia(e) == familia_pedida => Some(e),
            _ => None,
        })
        .collect()
}

/// O efeito está ativo no tempo `t` (s, base do clipe)? `ativo` None = o clipe inteiro. Intervalo fechado.
pub fn ativo_em(e: &Efeito, t: f64) -> bool {
    base_ativa(base(e), t)
}

/// `ativo_em` pela base (as ocultações da mídia não vêm dentro de um `Efeito`).
pub(crate) fn base_ativa(b: &EfBase, t: f64) -> bool {
    match &b.ativo {
        None => true,
        Some(i) => i.de.is_none_or(|de| t >= de) && i.ate.is_none_or(|ate| t <= ate),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    fn ef(v: serde_json::Value) -> Tolerante<Efeito> {
        serde_json::from_value(v).expect("efeito")
    }

    #[test]
    fn familias_ligados_e_ativo() {
        let lista = vec![
            ef(json!({ "tipo": "girar", "id": "g", "ligado": true, "nome": "", "ativo": null, "graus": 90, "espelhar_h": false, "espelhar_v": false })),
            ef(json!({ "tipo": "nitidez", "id": "n", "ligado": false, "nome": "", "ativo": null, "quantidade": 0.5 })),
            ef(json!({ "tipo": "nitidez", "id": "n2", "ligado": true, "nome": "", "ativo": { "de": 2.0, "ate": 4.0 }, "quantidade": 0.5 })),
            ef(json!({ "tipo": "do_futuro", "id": "x" })),
        ];
        assert_eq!(ligados(&lista, Familia::Enquadre).len(), 1);
        let origem = ligados(&lista, Familia::Origem);
        assert_eq!(origem.len(), 1);
        assert_eq!((base(origem[0]).id.as_str(), tipo(origem[0])), ("n2", "nitidez"));
        assert!(ligados(&lista, Familia::Anotacao).is_empty());
        let e = origem[0];
        assert!(!ativo_em(e, 1.999) && ativo_em(e, 2.0) && ativo_em(e, 4.0) && !ativo_em(e, 4.001));
        assert!(ativo_em(ligados(&lista, Familia::Enquadre)[0], -5.0));
    }
}
