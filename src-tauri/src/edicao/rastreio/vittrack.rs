//! Motor `vittrack` (OpenCV Zoo, Apache-2.0; §6.2): 715 kB embutidos no binário, sempre no provedor de CPU, 2 a 5 ms
//! por quadro; laço por reprocura. Porte de `p3a/referencia/rastreio_vittrack.py` (pontos em `modelos-rastreio.md`
//! §10: RGB, ImageNet, bilinear com centro em +0,5, recorte `ceil(sqrt(w·h)·fator)`, Hann de 16). Dono: P2.3.

use super::{recusa, Busca, CaixaPx, Imagem, Laco, Limiares, MotorCorrida, Passo, Rastreador};
use crate::commands::settings_commands::AiSettings;
use crate::error::Result;

/// `object_tracking_vittrack_2023sep.onnx` (o instalador leva; nada a baixar).
pub const MODELO: &[u8] = include_bytes!("vittrack_2023sep.onnx");
pub const NOME_ARQUIVO: &str = "object_tracking_vittrack_2023sep.onnx";
pub const SHA256: &str = "2990f0b7cd44d92afa48cd97db6de7be113fc1d9594fddb74e2725c10478e91d";
pub const ORIGEM: &str = "huggingface.co/opencv/object_tracking_vittrack";
pub const COMMIT: &str = "868e4c941984c6363594ec34ee1c3ba0b75378a3";

pub struct VitTrack {
    _privado: (),
}

impl VitTrack {
    /// Sessão em CPU (`Opcoes::so_cpu`) pelo onnxruntime instalado (`ia::criar_sessao_de_bytes`). Sem onnxruntime:
    /// `modelo_ausente: onnxruntime`; interruptor desligado: `ia_desligada`. Abertura: recusa.
    pub fn novo(ai: &AiSettings) -> Result<Self> {
        let _ = ai;
        Err(recusa("vittrack"))
    }
}

impl Rastreador for VitTrack {
    fn iniciar(&mut self, quadro: &Imagem, caixa: CaixaPx) -> Result<Passo> {
        let _ = (quadro, caixa);
        todo!("P2.3: atrás da recusa de `VitTrack::novo`")
    }
    fn seguir(&mut self, quadro: &Imagem, busca: &Busca) -> Result<Passo> {
        let _ = (quadro, busca);
        todo!("P2.3: atrás da recusa de `VitTrack::novo`")
    }
    fn laco(&self) -> Laco {
        Laco::Reprocura
    }
    fn limiares(&self, fps: f64) -> Limiares {
        let _ = fps;
        todo!("P2.3: atrás da recusa de `VitTrack::novo`")
    }
    fn corrida(&self) -> MotorCorrida {
        todo!("P2.3: atrás da recusa de `VitTrack::novo`")
    }
}
