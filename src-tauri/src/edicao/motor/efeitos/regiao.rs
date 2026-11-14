//! `ocultar` (§5.3, §5.4): primeiro efeito depois da cor, no espaço original, máscara por quadro lido.
//! Holofote e os demais modos fecham no P2.6. Dono: P0.6a.

use crate::edicao::modelo::EfOcultar;
use crate::edicao::motor::grafo::Grafo;
use crate::error::Result;

/// Aplica as ocultações à cadeia `entrada` (fonte já na cor interna) e devolve o rótulo de saída.
/// `mascaras[i]` = rótulo do fluxo da máscara de `ocultacoes[i]` (cinza, pts imposto); `uniao_px[i]` = recorte par [x, y, w, h].
pub fn ocultar(
    g: &mut Grafo,
    entrada: &str,
    ocultacoes: &[&EfOcultar],
    mascaras: &[String],
    uniao_px: &[[u32; 4]],
) -> Result<String> {
    todo!()
}
