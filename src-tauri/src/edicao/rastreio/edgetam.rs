//! Motor `edgetam` (EdgeTAM vídeo, Meta, Apache-2.0; §6.2): 4 grafos (item "Rastreador" do Gerenciador, em
//! `AiSettings.rastreio_dir`), GPU com queda para CPU, caixa + silhueta + presença por quadro; laço por presença.
//! Porte de `p3a/referencia/rastreio_edgetam.py` (`modelos-rastreio.md` §6 e §10): quadro esticado para 1024×1024,
//! `input_labels` int32, banco de memória (condicionante + 6 quadros + 15 ponteiros), máscara de volta por bilinear
//! nos logits; as duas constantes que não vêm nos grafos embutidas. Dono: P2.3.

use super::{recusa, Busca, CaixaPx, Imagem, Laco, Limiares, MotorCorrida, Passo, Rastreador};
use crate::commands::settings_commands::AiSettings;
use crate::error::Result;
use crate::ia::Opcoes;

/// `constantes_memoria.f32le` (2 816 bytes; SHA-256 `c69e56b1…a1ebb`), tiradas do checkpoint oficial.
pub const CONSTANTES: &[u8] = include_bytes!("edgetam_constantes.f32le");
pub const SHA256_CONSTANTES: &str = "c69e56b1d72fdc9104f9ccb4da175e1c698c3ce58305eb1888ae68b9706a1ebb";

/// Um arquivo do item do Gerenciador: nome, bytes e SHA-256 fixos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArquivoItem {
    pub nome: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

/// Os 4 grafos (65,2 MB), na pasta do item (`modelos-rastreio.md` §9).
pub const ARQUIVOS: [ArquivoItem; 4] = [
    ArquivoItem { nome: "vision_encoder.onnx", bytes: 19_826_101, sha256: "57fe1a2b3d500813fe4987c4209b856920f187ab0b7723d81586b607ea2cffc1" },
    ArquivoItem { nome: "mask_decoder.onnx", bytes: 17_800_677, sha256: "ee24ee1cae6ecc71889912c1893e08b09f645bb63875108ccd180f7218a3647d" },
    ArquivoItem { nome: "memory_attention.onnx", bytes: 20_914_958, sha256: "6477d775639905945949508c08a2c652d01e5933c0eb00a02225ae5f6aa1602c" },
    ArquivoItem { nome: "memory_encoder.onnx", bytes: 6_692_568, sha256: "035b30f8fb1f7c99211ada6eb9d72f0819de819ef94310b14e8147dc97f120ec" },
];
/// Fonte com commit fixo (terceiro: espelhar numa release do SICRO e deixar esta como segunda fonte, decisão do dono).
pub const URL_BASE: &str = "https://huggingface.co/jax-image-tools/edgetam-video-onnx/resolve/8ca3d3e4169938e65b552cdf14542fde25e8badb/";
pub const ORIGEM: &str = "jax-image-tools/edgetam-video-onnx";
pub const COMMIT: &str = "8ca3d3e4169938e65b552cdf14542fde25e8badb";
/// Desvio conhecido do export (§6.1, `modelos-rastreio.md` §6).
pub const DESVIO: &str = "melhor de 3 máscaras no quadro marcado";

pub struct EdgeTam {
    _privado: (),
}

impl EdgeTam {
    /// Os 4 grafos de `ai.rastreio_dir` com o SHA-256 conferido; sessões por `ia::criar_sessao` (GPU com queda; os
    /// três últimos no provedor que o primeiro conseguiu). Item ausente: `modelo_ausente: edgetam`. Abertura: recusa.
    pub fn novo(ai: &AiSettings, op: &Opcoes) -> Result<Self> {
        let _ = (ai, op);
        Err(recusa("edgetam"))
    }

    /// Os 4 arquivos estão em `ai.rastreio_dir` (sem conferir o SHA-256: isso é no `novo`).
    pub fn instalado(ai: &AiSettings) -> bool {
        let _ = ai;
        false
    }
}

impl Rastreador for EdgeTam {
    fn iniciar(&mut self, quadro: &Imagem, caixa: CaixaPx) -> Result<Passo> {
        let _ = (quadro, caixa);
        todo!("P2.3: atrás da recusa de `EdgeTam::novo`")
    }
    fn seguir(&mut self, quadro: &Imagem, busca: &Busca) -> Result<Passo> {
        let _ = (quadro, busca);
        todo!("P2.3: atrás da recusa de `EdgeTam::novo`")
    }
    fn laco(&self) -> Laco {
        Laco::Presenca
    }
    fn limiares(&self, fps: f64) -> Limiares {
        let _ = fps;
        todo!("P2.3: atrás da recusa de `EdgeTam::novo`")
    }
    fn corrida(&self) -> MotorCorrida {
        todo!("P2.3: atrás da recusa de `EdgeTam::novo`")
    }
}
