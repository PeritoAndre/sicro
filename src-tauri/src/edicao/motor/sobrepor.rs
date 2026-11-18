//! Camada global (§5.3, passo 6): títulos e imagens sobrepostos (presos ou não, P1.7), legendas queimadas (P1.6) e
//! selos (P1.R), por cima da principal já junta. Compartilhado desde a Fase 1 (O2); o vídeo sobreposto é do P3.2.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::clipe::{canvas_par, erro, imagem_da_camada, imagens_do_titulo};
use super::efeitos::{self, anotacao};
use super::grafo::Grafo;
use super::plano::{Desenho, Entrada, ImagemCamada, ItemCamada, Papel, PedidoCamada, QuadroItem, TipoCamada, TransformacaoAvaliada};
use super::{Alvo, Contexto};
use crate::edicao::camada;
use crate::edicao::camada::formas::QuadroCamada;
use crate::edicao::geometria::Afim;
use crate::edicao::modelo::{ClipeSobreposto, LinhaResolvida, ProjetoEdicao, SobBase, TamanhoPx, Tolerante};
use crate::edicao::sobreposto;
use crate::edicao::tempo;
use crate::edicao::validar;
use crate::error::Result;

/// O que a camada global põe no bloco: rótulo de saída, a entrada da camada e o pedido de rasterização (None = nada a sobrepor).
#[derive(Debug, Clone, PartialEq)]
pub struct Sobreposto {
    pub saida: String,
    pub entrada: Option<Entrada>,
    pub camada: Option<PedidoCamada>,
}

// CONTRATO: Fase 4 (contratos-fase4.md B3): a transformação com entrada e saída mora em `edicao::sobreposto`
// (pacote do texto livre, espelho em `modelo/sobreposto.ts`); aqui só chama.
fn avaliada(b: &SobBase, tau: f64, dur: f64) -> TransformacaoAvaliada {
    sobreposto::transformacao_em(b, tau, dur)
}

/// Aplica sobre `base` os clipes livres visíveis nos slots [de_q, de_q + n_q).
pub fn aplicar(
    g: &mut Grafo,
    base: &str,
    p: &ProjetoEdicao,
    linha: &LinhaResolvida,
    de_q: u64,
    n_q: u32,
    indice_entrada: usize,
    ctx: &Contexto,
    alvo: &Alvo,
) -> Result<Sobreposto> {
    let canvas = super::canvas_do_alvo(p, alvo);
    let t = TamanhoPx { w: canvas.w, h: canvas.h };
    let fps = linha.fps;
    let ate_q = de_q + n_q as u64;
    let (mut itens, mut imagens) = (Vec::new(), Vec::<ImagemCamada>::new());
    // trilhas de baixo para cima, clipes na ordem
    for trilha in p.sobreposicoes.iter().filter(|t| !t.oculta) {
        for c in &trilha.clipes {
            let c = match c {
                Tolerante::Conhecido(c) => c,
                Tolerante::Desconhecido(v) => return Err(validar::erro_do_desconhecido(v, validar::TIPOS_SOBREPOSTO)),
            };
            let (b, desenho) = match c {
                ClipeSobreposto::Video(v) => return Err(erro("nao_suportado", format!("sobreposto video {}", v.base.id))),
                ClipeSobreposto::Imagem(i) => (&i.base, Desenho::Imagem { midia: i.midia.clone() }),
                ClipeSobreposto::Titulo(x) => (&x.base, Desenho::Titulo { titulo: x.titulo.clone() }),
            };
            let Some(l) = linha.livres.iter().find(|l| l.id == b.id && l.trilha == trilha.id) else { continue };
            let (ini, fim) = (l.inicio_q, l.inicio_q + l.n_q as u64);
            let (a, z) = (ini.max(de_q), fim.min(ate_q));
            if a >= z {
                continue;
            }
            sobreposto::conferir(b)?;
            // efeito ligado em clipe sobreposto é de outra fase: nunca ignorado calado; desligado não pesa
            for e in &b.efeitos {
                match e {
                    Tolerante::Conhecido(e) if efeitos::base(e).ligado => {
                        return Err(erro("nao_suportado", format!("efeito {} em sobreposto {}", efeitos::base(e).id, b.id)));
                    }
                    Tolerante::Desconhecido(v) if validar::efeito_desconhecido_ligado(v) => {
                        return Err(validar::erro_do_desconhecido(v, validar::TIPOS_EFEITO));
                    }
                    _ => {}
                }
            }
            let novas = match c {
                ClipeSobreposto::Imagem(i) => vec![imagem_da_camada(p, ctx, &i.midia)?],
                ClipeSobreposto::Titulo(x) => imagens_do_titulo(p, ctx, &x.titulo)?,
                ClipeSobreposto::Video(_) => Vec::new(),
            };
            for n in novas {
                if !imagens.iter().any(|x| x.midia == n.midia) {
                    imagens.push(n);
                }
            }
            let dur = tempo::instante_do_slot(l.n_q as u64, fps);
            let quadros = (a..z)
                .map(|q| {
                    let tau = tempo::instante_do_slot(q - ini, fps);
                    let quadro = QuadroCamada { q, t: tau, pts: None, a: Afim::IDENTIDADE, canvas: t, rastreios: BTreeMap::new(), indice: None, programa_s: tempo::instante_do_slot(q, fps) };
                    QuadroItem { quadro, transformacao: Some(avaliada(b, tau, dur)) }
                })
                .collect();
            itens.push(ItemCamada { desenho, de: (a - de_q) as u32, quadros });
        }
    }
    // legendas queimadas (P1.6) e selos (P1.R): por cima dos sobrepostos, nessa ordem
    itens.extend(super::legendas::queimadas(p, linha, de_q, n_q, t, alvo)?);
    itens.extend(crate::edicao::camada::selos::itens(p, linha, de_q, n_q, t)?);
    if itens.is_empty() {
        return Ok(Sobreposto { saida: base.to_string(), entrada: None, camada: None });
    }
    let caixa = canvas_par(&canvas);
    let mut pedido = PedidoCamada { id: "g".into(), tipo: TipoCamada::Anotacao, caixa_px: caixa, n: n_q, hash: String::new(), itens, imagens };
    pedido.hash = camada::hash_do_pedido(&pedido);
    let entrada = Entrada {
        antes: ["-f", "concat", "-safe", "0"].iter().map(|s| s.to_string()).collect(),
        antes_sem_busca: None,
        caminho: PathBuf::new(),
        papel: Papel::Camada { id: pedido.id.clone() },
    };
    let k = anotacao::entrada_por_slot(g, &format!("{indice_entrada}:v"), fps, n_q, false);
    let saida = anotacao::sobrepor(g, base, &k, caixa[0], caixa[1])?;
    Ok(Sobreposto { saida, entrada: Some(entrada), camada: Some(pedido) })
}
