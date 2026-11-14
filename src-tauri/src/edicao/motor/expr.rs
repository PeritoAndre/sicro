//! Expressões do ffmpeg (D9, §5.11): árvores balanceadas, nunca `if` aninhado em fila. Dono: P0.6a.

use crate::edicao::modelo::Racional;

/// Tolerância padrão de `tabela`, em px.
pub const TOLERANCIA_PX: f64 = 0.25;

/// Árvore balanceada sobre `t` que devolve `valores_por_slot[q]` (px) no slot q: junta slots seguidos que caem
/// numa reta (tolerância `TOLERANCIA_PX`), limiares em (q − 0,5)/fps.
pub fn tabela(valores_por_slot: &[f64], fps: Racional) -> String {
    todo!()
}

/// Como `tabela`, com tolerância e variável escolhidas (`variavel` = "t", ou "(on-1)/<FPS>" no `perspective`).
pub fn tabela_em(valores_por_slot: &[f64], fps: Racional, tolerancia: f64, variavel: &str) -> String {
    todo!()
}

/// Árvore balanceada sobre `N` que devolve `pts[N]` (inteiros, na base do elo): impõe o pts da camada por quadro lido.
pub fn arvore_pts(pts: &[i64]) -> String {
    todo!()
}

/// `setpts` do mapa de quadros (§3.1): `ceil(((PTS-<p0>)*<kn>-<e>)/<kd>)*<m>`.
pub fn setpts_mapa(p0: i64, kn: i64, kd: i64, e: i64, m: i64) -> String {
    todo!()
}
