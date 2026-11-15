//! Registro de uso de modelos (§6.6): `feito.ia` da exportação junta `{ modelo, arquivos, runtime, provedor, uso }`
//! das corridas dos rastreios usados (e do motor de transcrição das legendas). Dono: P2.1.
//! Abertura: `da_proveniencia` devolve vazio (o `feito` da Fase 1) até o P2.1 entregar.

use crate::edicao::motor::plano::{ProvIa, ProvRastreio};
use crate::edicao::rastreio::Corrida;

/// Uma entrada de `feito.ia` por modelo e provedor, a partir de `feito.rastreios[].corridas` (o compilador é puro:
/// lê só o que já está na proveniência). `uso` = "rastreio".
pub fn da_proveniencia(rastreios: &[ProvRastreio]) -> Vec<ProvIa> {
    let _ = rastreios;
    Vec::new()
}

/// A entrada de uma corrida.
pub fn da_corrida(c: &Corrida, uso: &str) -> ProvIa {
    let m = &c.motor;
    ProvIa {
        modelo: m.modelo.nome.clone(),
        arquivos: m.modelo.arquivos.iter().map(|a| crate::edicao::motor::plano::ProvArquivo { nome: a.nome.clone(), sha256: a.sha256.clone() }).collect(),
        origem: m.modelo.origem.clone(),
        commit: m.modelo.commit.clone(),
        desvio: m.modelo.desvio.clone(),
        runtime: m.runtime.clone(),
        provedor: m.provedor.clone(),
        uso: uso.to_string(),
    }
}
