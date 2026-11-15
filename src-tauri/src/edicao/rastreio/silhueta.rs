//! Silhuetas por quadro (§6.1): `silhuetas/<rid>/<sha256>.sil`, um arquivo por denso, cabeçalho JSON (esquema,
//! `pixel`, `n`, `pts[]`, modelo) e, por quadro, a máscara binária em RLE por linha na resolução da fonte exibida.
//! A Fase 2 grava (EdgeTAM), a Fase 5 usa. Imutável e endereçado pelo conteúdo, como o denso. Dono: P2.2.

use std::path::{Path, PathBuf};

use super::{recusa, ArquivoSilhuetas, Mascara};
use crate::edicao::modelo::Silhuetas;
use crate::error::Result;

pub const ESQUEMA_SILHUETAS: u32 = 1;

/// RLE por linha de uma máscara (formato fixado pelo P2.2 e escrito em contratos-fase2 ao entregar).
pub fn rle(m: &Mascara) -> Vec<u8> {
    let _ = m;
    Vec::new()
}

/// Volta do RLE; tamanho que não fecha com `w`×`h` = `item_invalido`.
pub fn mascara(rle: &[u8], w: u32, h: u32) -> Result<Mascara> {
    let _ = (rle, w, h);
    Err(recusa("silhuetas"))
}

/// `<pasta do projeto>/silhuetas/<rid>/<sha256>.sil`. Mesma regra do `arquivo::caminho` (id e SHA-256 conferidos antes).
pub fn caminho(pasta_projeto: &Path, rastreio: &str, sha256: &str) -> PathBuf {
    pasta_projeto.join("silhuetas").join(rastreio).join(format!("{sha256}.sil"))
}

pub fn gravar(pasta_projeto: &Path, rastreio: &str, s: &ArquivoSilhuetas) -> Result<Silhuetas> {
    let _ = (pasta_projeto, rastreio, s);
    Err(recusa("silhuetas"))
}

/// Lê e confere o SHA-256 (o `LeitorRastreios::silhuetas` do executor passa a chamar daqui na Fase 5).
pub fn ler(pasta_projeto: &Path, rastreio: &str, sha256: &str) -> Result<ArquivoSilhuetas> {
    let _ = (pasta_projeto, rastreio, sha256);
    Err(recusa("silhuetas"))
}
