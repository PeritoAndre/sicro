//! Fusão entre duas âncoras (§6.2): corrida para a frente da primeira e para trás da segunda, média ponderada pela
//! distância a cada âncora. Usada por `arquivo::juntar` quando a faixa tem corridas dos dois lados. Dono: P2.2.

use std::collections::{BTreeMap, BTreeSet};

use super::{AmostraCorrida, ESTADO_AUSENTE, ESTADO_INCERTO, ESTADO_OK};
use crate::edicao::modelo::Caixa;

/// Peso da corrida para trás no quadro `pts` (0 na âncora da frente, 1 na de trás).
pub(crate) fn peso_de_tras(pts: i64, de_pts: i64, ate_pts: i64) -> f64 {
    if ate_pts <= de_pts {
        return 0.0;
    }
    (pts - de_pts) as f64 / (ate_pts - de_pts) as f64
}

/// As duas corridas da faixa `[de_pts, ate_pts]` (pts das duas âncoras) → uma amostra por quadro, em ordem de pts.
/// Quadro que só um lado tem fica com ele; os dois com caixa = média ponderada (caixa e nota; incerto se um dos dois
/// for; silhueta do lado mais perto); os dois sem caixa = ausente.
pub fn fundir(frente: &[AmostraCorrida], tras: &[AmostraCorrida], de_pts: i64, ate_pts: i64) -> Vec<AmostraCorrida> {
    let dentro = |a: &&AmostraCorrida| a.pts >= de_pts && a.pts <= ate_pts;
    let f: BTreeMap<i64, &AmostraCorrida> = frente.iter().filter(dentro).map(|a| (a.pts, a)).collect();
    let t: BTreeMap<i64, &AmostraCorrida> = tras.iter().filter(dentro).map(|a| (a.pts, a)).collect();
    let todos: BTreeSet<i64> = f.keys().chain(t.keys()).copied().collect();
    todos
        .into_iter()
        .map(|p| match (f.get(&p), t.get(&p)) {
            (Some(a), None) | (None, Some(a)) => (*a).clone(),
            (Some(a), Some(b)) => {
                let wt = peso_de_tras(p, de_pts, ate_pts);
                let wf = 1.0 - wt;
                let perto = if wf >= wt { a } else { b };
                match (a.caixa, b.caixa) {
                    (Some(ca), Some(cb)) => AmostraCorrida {
                        pts: p,
                        caixa: Some(Caixa { x: wf * ca.x + wt * cb.x, y: wf * ca.y + wt * cb.y, w: wf * ca.w + wt * cb.w, h: wf * ca.h + wt * cb.h }),
                        conf: wf * a.conf + wt * b.conf,
                        estado: if a.estado == ESTADO_OK && b.estado == ESTADO_OK { ESTADO_OK } else { ESTADO_INCERTO },
                        silhueta: perto.silhueta.clone(),
                    },
                    (Some(_), None) => (*a).clone(),
                    (None, Some(_)) => (*b).clone(),
                    (None, None) => AmostraCorrida { pts: p, caixa: None, conf: a.conf.max(b.conf), estado: ESTADO_AUSENTE, silhueta: None },
                }
            }
            (None, None) => unreachable!("pts sem lado"),
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn a(pts: i64, x: Option<f64>, conf: f64, estado: u8) -> AmostraCorrida {
        AmostraCorrida { pts, caixa: x.map(|x| Caixa { x, y: 0.5, w: 0.1, h: 0.2 }), conf, estado, silhueta: x.map(|_| vec![pts as u8]) }
    }

    #[test]
    fn media_ponderada_pela_distancia() {
        // âncoras em 0 e 100; a frente cobre 0..=60, a de trás 40..=100
        let frente: Vec<_> = (0..=60).map(|p| a(p, Some(0.1), 0.9, ESTADO_OK)).collect();
        let mut tras: Vec<_> = (40..=100).rev().map(|p| a(p, Some(0.5), 0.7, ESTADO_OK)).collect();
        tras[10] = a(90, None, 0.2, ESTADO_AUSENTE);
        let mut f2 = frente.clone();
        f2[45] = a(45, Some(0.1), 0.4, ESTADO_INCERTO);
        f2[50] = a(50, None, 0.1, ESTADO_AUSENTE);
        let r = fundir(&f2, &tras, 0, 100);
        assert_eq!(r.iter().map(|a| a.pts).collect::<Vec<_>>(), (0..=100).collect::<Vec<_>>());
        // só a frente / só a de trás
        assert_eq!(r[10], f2[10]);
        assert_eq!(r[80], tras.iter().find(|a| a.pts == 80).unwrap().clone());
        // os dois: peso pela distância
        let c = r[42].caixa.unwrap();
        let (wt, wf) = (0.42, 1.0 - 0.42);
        assert_eq!(c.x, wf * 0.1 + wt * 0.5);
        assert_eq!(r[42].conf, wf * 0.9 + wt * 0.7);
        assert_eq!((r[42].estado, r[42].silhueta.clone()), (ESTADO_OK, Some(vec![42])));
        assert_eq!(r[55].silhueta, Some(vec![55]));
        // incerto de um lado; ausente de um lado fica com o outro
        assert_eq!(r[45].estado, ESTADO_INCERTO);
        assert_eq!(r[50].caixa.unwrap().x, 0.5);
        // de trás ausente e frente fora: ausente
        assert_eq!((r[90].caixa, r[90].estado), (None, ESTADO_AUSENTE));
        // fora da faixa não entra
        assert!(fundir(&frente, &tras, 10, 20).iter().all(|a| (10..=20).contains(&a.pts)));
    }
}
