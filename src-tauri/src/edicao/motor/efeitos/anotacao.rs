//! Anotações (§5.4): `overlay` da camada por slot sobre a base já enquadrada. Dono: P0.6a.
//! Toda entrada de camada é cortada no tamanho dela ANTES de ter o tempo imposto: a camada fixa entra em laço
//! (`-loop 1`) e não acabaria.

use crate::edicao::cor;
use crate::edicao::motor::expr;
use crate::edicao::motor::grafo::{filtro, Grafo};
use crate::error::Result;

/// Sobrepõe a camada (`camada` = rótulo do fluxo RGBA, já na cor de camada) em (`x`, `y`) px pares e devolve o rótulo de saída.
pub fn sobrepor(g: &mut Grafo, base: &str, camada: &str, x: u32, y: u32) -> Result<String> {
    let saida = g.rotulo("s");
    let f = filtro("overlay", None, &[("x", x.to_string()), ("y", y.to_string()), ("format", "yuv420".into()), ("eof_action", "pass".into())]);
    g.cadeia(&[base, camada], &[f], &[&saida]);
    Ok(saida)
}

/// Cadeia de entrada de uma camada por slot: `settb`, `setpts=N`, `trim=end_frame=<n_q>` e a cor de camada (ou `format=gray`).
pub fn entrada_por_slot(g: &mut Grafo, entrada: &str, fps: crate::edicao::modelo::Racional, n_q: u32, cinza: bool) -> String {
    let saida = g.rotulo("k");
    let filtros = vec![
        format!("trim=end_frame={n_q}"),
        format!("settb={}/{}", fps.den, fps.num),
        "setpts=N".to_string(),
        if cinza { "format=gray".to_string() } else { cor::filtro_camada() },
    ];
    g.cadeia(&[entrada], &filtros, &[&saida]);
    saida
}

/// Cadeia de entrada de uma máscara por quadro lido: `format=gray`, `settb=<tb>` e pts imposto por árvore sobre `N`.
pub fn entrada_por_quadro(g: &mut Grafo, entrada: &str, tb: crate::edicao::modelo::Racional, pts: &[i64]) -> String {
    let saida = g.rotulo("k");
    let filtros = vec![
        format!("trim=end_frame={}", pts.len()),
        "format=gray".to_string(),
        format!("settb={}/{}", tb.num, tb.den),
        filtro("setpts", None, &[("expr", expr::arvore_pts(pts))]),
    ];
    g.cadeia(&[entrada], &filtros, &[&saida]);
    saida
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::modelo::Racional;

    #[test]
    fn entradas_de_camada_e_overlay() {
        let mut g = Grafo::novo();
        let a = entrada_por_slot(&mut g, "1:v", Racional { num: 30000, den: 1001 }, 45, false);
        let m = entrada_por_slot(&mut g, "2:v", Racional { num: 30, den: 1 }, 7, true);
        let q = entrada_por_quadro(&mut g, "3:v", Racional { num: 1, den: 15360 }, &[1024, 1536, 2048]);
        let s = sobrepor(&mut g, "base", &a, 12, 40).unwrap();
        let t = g.texto();
        let linhas: Vec<&str> = t.split(";\n").collect();
        assert!(linhas[0].starts_with("[1:v]trim=end_frame=45,settb=1001/30000,setpts=N,scale=flags=bicubic+accurate_rnd+bitexact:out_color_matrix=bt709"));
        assert_eq!(linhas[1], format!("[2:v]trim=end_frame=7,settb=1/30,setpts=N,format=gray[{m}]"));
        assert_eq!(linhas[2], format!("[3:v]trim=end_frame=3,format=gray,settb=1/15360,setpts=expr=1024+N*512[{q}]"));
        assert_eq!(linhas[3], format!("[base][{a}]overlay=x=12:y=40:format=yuv420:eof_action=pass[{s}]"));
        assert!(!t.contains("duration"));
    }
}
