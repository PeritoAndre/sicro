//! Legendas no render (§5.3 passos 6 e 9): queimadas na camada global (`EfTexto` por legenda, no slot certo) e em
//! arquivo (`.srt` e `.vtt` ao lado do vídeo; sem faixa dentro do contêiner). Legenda de fonte acompanha o trecho
//! (velocidade, corte no meio). Dono: P1.6.

use crate::edicao::modelo::{Exportacao, LinhaResolvida, ModoLegendas, ProjetoEdicao, TamanhoPx};
use crate::edicao::motor::plano::{ArquivoTexto, ItemCamada};
use crate::edicao::motor::{Alvo, Contexto};
use crate::edicao::mensagens::codigo;
use crate::error::{Result, SicroError};

/// Detalhe do `nao_suportado` enquanto as legendas não saem: exportação que pede legendas com itens no projeto.
/// Uma regra só: `validar` (fase_0) e `recusar` perguntam aqui. None = saem.
pub fn recusa(p: &ProjetoEdicao, exportacao: &Exportacao) -> Option<&'static str> {
    (exportacao.legendas != ModoLegendas::Nenhuma && !p.legendas.itens.is_empty()).then_some("legendas")
}

/// O motor nunca entrega a exportação sem as legendas pedidas, calado.
pub fn recusar(p: &ProjetoEdicao, alvo: &Alvo) -> Result<()> {
    match alvo {
        Alvo::Final { exportacao } if recusa(p, exportacao).is_some() => Err(SicroError::Validation(format!("{}: legendas", codigo::NAO_SUPORTADO))),
        _ => Ok(()),
    }
}

/// `.srt` e `.vtt` da exportação (`Plano.legendas`; o executor grava e a fila publica com o nome do vídeo), em tempo
/// do programa. Só com `Arquivo` ou `Ambos`. Fase 0: nenhum (sem recusa não há legenda a gerar).
pub fn arquivos(p: &ProjetoEdicao, linha: &LinhaResolvida, ctx: &Contexto, alvo: &Alvo) -> Result<Vec<ArquivoTexto>> {
    let _ = (p, linha, ctx, alvo);
    Ok(Vec::new())
}

/// Legendas queimadas nos slots [de_q, de_q + n_q) do bloco: um `ItemCamada` por legenda visível (`Desenho::Efeito`
/// com um `EfTexto` no estilo e lugar da trilha), em px do `canvas`. Só com `Queimar` ou `Ambos` na exportação final
/// (a prévia e o quadro final mostram as queimadas também). Fase 0: nenhuma.
pub fn queimadas(p: &ProjetoEdicao, linha: &LinhaResolvida, de_q: u64, n_q: u32, canvas: TamanhoPx, alvo: &Alvo) -> Result<Vec<ItemCamada>> {
    let _ = (p, linha, de_q, n_q, canvas, alvo);
    Ok(Vec::new())
}
