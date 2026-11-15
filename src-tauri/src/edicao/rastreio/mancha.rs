//! Motor `mancha` (§6.2): adaptador do `Seguidor` de `video/pessoas/mancha.rs` (correlação, sem modelo). Só com os
//! modelos desligados ou sem onnxruntime. Laço simples (para quando a correlação cai). Dono: P2.2.

use super::{recusa, Busca, CaixaPx, Imagem, Laco, Limiares, MotorCorrida, Passo, Rastreador};
use crate::error::Result;

pub struct Mancha {
    _privado: (),
}

impl Mancha {
    /// Abertura: recusa `nao_suportado: mancha` até o P2.2 entregar.
    pub fn nova() -> Result<Self> {
        Err(recusa("mancha"))
    }
}

impl Rastreador for Mancha {
    fn iniciar(&mut self, quadro: &Imagem, caixa: CaixaPx) -> Result<Passo> {
        let _ = (quadro, caixa);
        todo!("P2.2: atrás da recusa de `Mancha::nova`")
    }
    fn seguir(&mut self, quadro: &Imagem, busca: &Busca) -> Result<Passo> {
        let _ = (quadro, busca);
        todo!("P2.2: atrás da recusa de `Mancha::nova`")
    }
    fn laco(&self) -> Laco {
        Laco::Simples
    }
    fn limiares(&self, fps: f64) -> Limiares {
        let _ = fps;
        todo!("P2.2: atrás da recusa de `Mancha::nova`")
    }
    fn corrida(&self) -> MotorCorrida {
        todo!("P2.2: atrás da recusa de `Mancha::nova`")
    }
}
