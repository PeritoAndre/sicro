//! Cadeia de vídeo de um clipe da principal (§5.3): entradas, filtros, pedidos de camada e prévias. Dono: P0.6a.
//!
//! Domínio da fonte: corte por pts → [showinfo] → cor → `ocultar` (máscara por quadro lido) → clone do último
//! quadro → mapa de quadros → `fps@cNNN`. Domínio do slot: enquadramento → holofote → anotação. O vídeo sai com o
//! pts = slot local do clipe (base 1/fps): a junção acha por ele o lugar do escurecimento.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::efeitos::{self, anotacao, enquadramento, lupa, realce, regiao, Familia};
use super::grafo::{filtro, Grafo};
use super::plano::{ArquivoTexto, Canvas, ConferirClipe, Desenho, Entrada, ImagemCamada, ItemCamada, Papel, PedidoCamada, Previa, QuadroItem, TipoCamada, TransformacaoAvaliada};
use super::{expr, Alvo, Contexto, FonteInfo};
use crate::edicao::camada::formas::{self, QuadroCamada};
use crate::edicao::camada::texto::{Atlas, FonteSprites, PedidoSprite, Sprite};
use crate::edicao::camada::{self, mascara};
use crate::edicao::cor;
use crate::edicao::ffmpeg::args;
use crate::edicao::geometria::{self, Afim};
use crate::edicao::modelo::{
    BaseRastreio, Caixa, ClipeCongelar, ClipeImagem, ClipePrincipal, ClipeResolvido, ClipeTitulo, ClipeTrecho, EfOcultar, Efeito, EstiloTexto,
    FormaRegiao, Fundo, MidiaRef, MostraCarimbo, ProjetoEdicao, Racional, Rastreio, TamanhoPx, Titulo, Tolerante,
};
use crate::edicao::quadros::TabelaQuadros;
use crate::edicao::rastreio::avaliar::{self, Avaliado, Instante};
use crate::edicao::rastreio::ArquivoDenso;
use crate::edicao::tempo::{self, MapaQuadros};
use crate::edicao::validar;
use crate::error::{Result, SicroError};

/// Janela de slots do bloco dentro do clipe: [de, ate) em slots locais do clipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Janela {
    pub de: u32,
    pub ate: u32,
}

/// O que um clipe põe no bloco. `saida` = rótulo do vídeo pronto (W×H, yuv420p, `ate − de` quadros);
/// `primeira_entrada` = índice da primeira entrada dele no bloco.
#[derive(Debug, Clone, PartialEq)]
pub struct CadeiaClipe {
    pub saida: String,
    pub entradas: Vec<Entrada>,
    pub camadas: Vec<PedidoCamada>,
    pub previas: Vec<Previa>,
    pub auxiliares: Vec<ArquivoTexto>,
    pub conferir: Option<ConferirClipe>,
}

pub(crate) fn erro(codigo: &str, detalhe: impl std::fmt::Display) -> SicroError {
    SicroError::Validation(format!("{codigo}: {detalhe}"))
}

/// Erro do `FonteSprites` do compilador: o tamanho do texto só existe depois de rasterizar.
const SPRITE_PENDENTE: &str = "sprite_pendente";

/// `FonteSprites` sem disco: sabe o tamanho das imagens (pelo contexto) e não sabe o dos textos.
pub(crate) struct SpritesDoPlano<'a> {
    pub p: &'a ProjetoEdicao,
    pub ctx: &'a Contexto<'a>,
}

impl FonteSprites for SpritesDoPlano<'_> {
    fn sprite(&self, _: &PedidoSprite) -> Result<Sprite> {
        Err(SicroError::Validation(SPRITE_PENDENTE.into()))
    }
    fn atlas(&self, _: &EstiloTexto, _: u32) -> Result<Atlas> {
        Err(SicroError::Validation(SPRITE_PENDENTE.into()))
    }
    fn imagem(&self, midia: &str) -> Option<TamanhoPx> {
        tamanho_da_imagem(self.p, self.ctx, midia)
    }
}

/// Tamanho da imagem como o navegador mostra: o do `MidiaRef` (quem grava é o front); senão o exibido.
pub(crate) fn tamanho_da_imagem(p: &ProjetoEdicao, ctx: &Contexto, midia: &str) -> Option<TamanhoPx> {
    let f = ctx.fontes.get(midia)?;
    let do_ref = p.midias.iter().find(|m| m.id == midia).and_then(|m| Some(TamanhoPx { w: m.largura?, h: m.altura? }));
    Some(do_ref.filter(|t| t.w > 0 && t.h > 0).unwrap_or(TamanhoPx { w: f.exibido.0, h: f.exibido.1 }))
}

/// Imagem do projeto citada por uma camada.
pub(crate) fn imagem_da_camada(p: &ProjetoEdicao, ctx: &Contexto, midia: &str) -> Result<ImagemCamada> {
    let f = ctx.fontes.get(midia).ok_or_else(|| erro("midia_ausente", midia))?;
    let tamanho = tamanho_da_imagem(p, ctx, midia).ok_or_else(|| erro("midia_ausente", midia))?;
    Ok(ImagemCamada { midia: midia.to_string(), sha256: f.sha256.clone(), caminho: f.caminho.clone(), tamanho })
}

/// Imagens que um título cita (o fundo).
pub(crate) fn imagens_do_titulo(p: &ProjetoEdicao, ctx: &Contexto, t: &Titulo) -> Result<Vec<ImagemCamada>> {
    match &t.fundo {
        Fundo::Imagem { midia } => Ok(vec![imagem_da_camada(p, ctx, midia)?]),
        _ => Ok(Vec::new()),
    }
}

/// Canvas inteiro em px pares.
pub(crate) fn canvas_par(c: &Canvas) -> [u32; 4] {
    [0, 0, (c.w / 2 * 2).max(2), (c.h / 2 * 2).max(2)]
}

/// Um slot da janela: slot do programa, tempo na base do clipe e o quadro de fonte mostrado (tempo, pts, índice).
#[derive(Debug, Clone, Copy)]
struct Slot {
    q: u64,
    tau: f64,
    fonte: Option<(f64, i64, usize)>,
}

/// Leitor de rastreios com os densos já abertos.
pub(crate) struct Rastreios<'a> {
    p: &'a ProjetoEdicao,
    ctx: &'a Contexto<'a>,
    densos: BTreeMap<String, Option<Arc<ArquivoDenso>>>,
}

impl<'a> Rastreios<'a> {
    pub(crate) fn novo(p: &'a ProjetoEdicao, ctx: &'a Contexto<'a>) -> Self {
        Self { p, ctx, densos: BTreeMap::new() }
    }

    pub(crate) fn achar(&self, id: &str) -> Result<&'a Rastreio> {
        self.p.rastreios.iter().find(|r| r.id == id).ok_or_else(|| erro("rastreio_ausente", id))
    }

    /// Caixa do rastreio no instante (None = nada).
    pub(crate) fn em(&mut self, r: &Rastreio, em: Instante) -> Result<Option<Avaliado>> {
        if !self.densos.contains_key(&r.id) {
            let d = match &r.denso {
                Some(d) => Some(self.ctx.rastreios.denso(&r.id, &d.sha256)?),
                None => None,
            };
            self.densos.insert(r.id.clone(), d);
        }
        Ok(avaliar::avaliar(r, self.densos.get(&r.id).and_then(|d| d.as_deref()), em))
    }
}

/// Ocultações que valem num clipe dessa mídia: as da mídia (D17) e as do próprio clipe (O5), ligadas.
pub(crate) fn ocultacoes_de<'a>(midia: Option<&'a MidiaRef>, efeitos: &'a [Tolerante<Efeito>]) -> Vec<&'a EfOcultar> {
    let mut v: Vec<&EfOcultar> = midia.map(|m| m.ocultacoes.iter().filter(|o| o.base.ligado).collect()).unwrap_or_default();
    v.extend(efeitos.iter().filter_map(|e| match e {
        Tolerante::Conhecido(Efeito::Ocultar(o)) if o.base.ligado => Some(o),
        _ => None,
    }));
    v
}

/// Efeitos do clipe congelado com o `ativo` das ocultações dele resolvido. No congelado o `ativo` é tempo LOCAL
/// (§3.1), mas a ocultação vale por quadro lido e só há um: vale no congelado inteiro quando pega todos os slots,
/// não vale quando não pega nenhum; pegar só parte é recusado (lido em tempo de fonte, saía descoberto calado).
pub(crate) fn efeitos_do_congelado(efeitos: &[Tolerante<Efeito>], n_q: u32, fps: Racional) -> Result<Vec<Tolerante<Efeito>>> {
    let mut v = efeitos.to_vec();
    for e in &mut v {
        let Tolerante::Conhecido(Efeito::Ocultar(o)) = e else { continue };
        if !o.base.ligado || o.base.ativo.is_none() {
            continue;
        }
        let (mut algum, mut todos) = (false, true);
        for q in 0..n_q as u64 {
            let ativo = efeitos::base_ativa(&o.base, tempo::instante_do_slot(q, fps));
            (algum, todos) = (algum || ativo, todos && ativo);
            if algum && !todos {
                return Err(erro("nao_suportado", format!("efeito ocultar {} em parte do congelado", o.base.id)));
            }
        }
        if todos {
            o.base.ativo = None;
        } else {
            o.base.ligado = false;
        }
    }
    Ok(v)
}

/// Caixa da ocultação no quadro `j` da fonte: None = não vale nesse quadro (fora do `ativo`, ou ausência aceita).
/// `ocultar` ativo sem caixa fora de ausência aceita é erro `sem_cobertura`: nunca sai quadro descoberto calado.
pub(crate) fn caixa_da_ocultacao(rs: &mut Rastreios, o: &EfOcultar, tab: &TabelaQuadros, j: usize) -> Result<Option<Avaliado>> {
    let t = tab.tempo(j);
    if !efeitos::base_ativa(&o.base, t) {
        return Ok(None);
    }
    let r = rs.achar(&o.alvo.rastreio)?;
    match rs.em(r, Instante { t, pts: Some(tab.pts[j]) })? {
        Some(a) => Ok(Some(a)),
        None if avaliar::em_ausencia(r, t) => Ok(None),
        None => Err(erro("sem_cobertura", &o.base.id)),
    }
}

// CONTRATO: revisão do fecho da Fase 2 (`caixa_no_lido` devolvia `Option<Avaliado>`: a ausência aceita e o "não vale"
// saíam iguais, e a invertida não cobria nada sem o alvo).
/// A ocultação num quadro lido.
pub(crate) enum NoLido {
    /// Não vale nesse quadro (fora do `ativo`).
    Fora,
    /// Vale, sem caixa: o alvo está numa ausência aceita.
    Ausente,
    Caixa(Avaliado),
}

/// `caixa_da_ocultacao` com a ausência aceita separada do "não vale".
fn no_quadro(rs: &mut Rastreios, o: &EfOcultar, tab: &TabelaQuadros, j: usize) -> Result<NoLido> {
    Ok(match caixa_da_ocultacao(rs, o, tab, j)? {
        Some(a) => NoLido::Caixa(a),
        // dentro do `ativo`, sem caixa só numa ausência aceita (fora dela, `sem_cobertura`)
        None if efeitos::base_ativa(&o.base, tab.tempo(j)) => NoLido::Ausente,
        None => NoLido::Fora,
    })
}

/// A ocultação num quadro LIDO. `mostrados` = [a, b): os quadros da fonte que o clipe mostra. Fora deles o
/// quadro só alimenta média, desentrelace ou estabilização e nunca aparece sozinho: vale o `ativo` do quadro
/// mostrado mais próximo (a ocultação criada pela tela nasce com `ativo` = faixa do clipe; lido pelo instante do
/// vizinho, ele entrava descoberto na média). Sem caixa no vizinho, a do quadro mostrado mais próximo.
pub(crate) fn caixa_no_lido(rs: &mut Rastreios, o: &EfOcultar, tab: &TabelaQuadros, j: usize, mostrados: (usize, usize)) -> Result<NoLido> {
    let (a, b) = mostrados;
    if b <= a || (a..b).contains(&j) {
        return no_quadro(rs, o, tab, j);
    }
    let perto = j.clamp(a, b - 1);
    if !efeitos::base_ativa(&o.base, tab.tempo(perto)) {
        return Ok(NoLido::Fora);
    }
    let r = rs.achar(&o.alvo.rastreio)?;
    let t = tab.tempo(j);
    match rs.em(r, Instante { t, pts: Some(tab.pts[j]) })? {
        Some(x) => Ok(NoLido::Caixa(x)),
        None if avaliar::em_ausencia(r, t) => Ok(NoLido::Ausente),
        None => no_quadro(rs, o, tab, perto),
    }
}

/// Invertida sem o alvo (ausência aceita): tudo está fora da forma, então a região é o quadro inteiro (retângulo
/// 0..1 sobre a caixa `QUADRO_TODO`), com a mesma variante.
fn quadro_inteiro(o: &EfOcultar) -> EfOcultar {
    let mut x = o.clone();
    (x.alvo.forma, x.alvo.vertices, x.alvo.margem, x.alvo.borda_suave, x.alvo.inverter) = (FormaRegiao::Retangulo, None, 0.0, 0.0, false);
    x
}

const QUADRO_TODO: Caixa = Caixa { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

/// Detalhe do `nao_suportado` de um efeito que o motor ainda não renderiza; None = renderiza. A regra é do pacote
/// dono do efeito (realce: P1.4; lupa: P1.2; régua e velocidade: P1.3); a validação pergunta o mesmo.
pub(crate) fn recusa_do_efeito(e: &Efeito, com_fonte: bool) -> Option<&'static str> {
    match e {
        // CONTRATO: Fase 4 (contratos-fase4.md B4): enquadramento que segue, regra do pacote do zoom que segue
        Efeito::Enquadramento(q) => crate::edicao::segue::recusa(q),
        Efeito::Lupa(l) => lupa::recusa(l),
        Efeito::Regua(_) | Efeito::VelocidadeExibida(_) => crate::edicao::medidas::recusa(e),
        e if efeitos::familia(e) == Familia::Origem => realce::recusa(e, com_fonte),
        _ => None,
    }
}

/// Efeito que o motor ainda não renderiza: erro com o tipo e o id. Desligado nunca é recusado, conhecido ou não;
/// desconhecido ligado é o `item_desconhecido` (ou `item_invalido`) da validação.
fn recusar_de_outra_fase(efeitos: &[Tolerante<Efeito>], com_fonte: bool) -> Result<()> {
    for e in efeitos {
        match e {
            Tolerante::Desconhecido(v) if validar::efeito_desconhecido_ligado(v) => {
                return Err(validar::erro_do_desconhecido(v, validar::TIPOS_EFEITO));
            }
            Tolerante::Conhecido(e) if efeitos::base(e).ligado => {
                if recusa_do_efeito(e, com_fonte).is_some() {
                    return Err(erro("nao_suportado", format!("efeito {} {}", efeitos::tipo(e), efeitos::base(e).id))); // despacho
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// O que um efeito do clipe pede à cadeia (lupa, realce): o grafo, camadas, prévias e arquivos auxiliares.
pub trait Montador {
    fn grafo(&mut self) -> &mut Grafo;
    /// Registra um pedido de camada (`letra` entra no id) e devolve o rótulo da entrada dela no grafo.
    fn camada(&mut self, letra: char, tipo: TipoCamada, caixa_px: [u32; 4], n: u32, itens: Vec<ItemCamada>, imagens: Vec<ImagemCamada>) -> String;
    /// Prévia que roda antes dos blocos (`Previa::Vidstab`); repetida no plano = uma só.
    fn previa(&mut self, p: Previa);
    /// Arquivo auxiliar do bloco (nome simples, único no bloco: comece pelo `rotulo()`).
    fn auxiliar(&mut self, a: ArquivoTexto);
    fn canvas(&self) -> &Canvas;
    /// Prefixo dos rótulos do clipe no bloco (`c003`).
    fn rotulo(&self) -> &str;
    fn exportacao_final(&self) -> bool;
}

struct Montagem<'a, 'g> {
    g: &'g mut Grafo,
    p: &'a ProjetoEdicao,
    ctx: &'a Contexto<'a>,
    canvas: Canvas,
    rot: String,
    clipe: String,
    proxima_entrada: usize,
    entradas: Vec<Entrada>,
    camadas: Vec<PedidoCamada>,
    previas: Vec<Previa>,
    auxiliares: Vec<ArquivoTexto>,
    rastreios: Rastreios<'a>,
    exportacao_final: bool,
}

impl Montador for Montagem<'_, '_> {
    fn grafo(&mut self) -> &mut Grafo {
        self.g
    }
    fn camada(&mut self, letra: char, tipo: TipoCamada, caixa_px: [u32; 4], n: u32, itens: Vec<ItemCamada>, imagens: Vec<ImagemCamada>) -> String {
        self.entrada_camada(letra, tipo, caixa_px, n, itens, imagens)
    }
    fn previa(&mut self, p: Previa) {
        if !self.previas.contains(&p) {
            self.previas.push(p);
        }
    }
    fn auxiliar(&mut self, a: ArquivoTexto) {
        self.auxiliares.push(a);
    }
    fn canvas(&self) -> &Canvas {
        &self.canvas
    }
    fn rotulo(&self) -> &str {
        &self.rot
    }
    fn exportacao_final(&self) -> bool {
        self.exportacao_final
    }
}

impl Montagem<'_, '_> {
    fn entrada_midia(&mut self, fonte: &FonteInfo, midia: &str, busca: Option<(f64, f64)>) -> String {
        let (antes, sem) = match busca {
            Some((alvo, t)) => (vec!["-ss".to_string(), format!("{alvo:.6}"), "-t".to_string(), format!("{t:.6}")], Some(Vec::new())),
            None => (Vec::new(), None),
        };
        self.entradas.push(Entrada { antes, antes_sem_busca: sem, caminho: fonte.caminho.clone(), papel: Papel::Midia { id: midia.to_string(), clipe: self.clipe.clone() } });
        self.proxima()
    }

    fn proxima(&mut self) -> String {
        let r = format!("{}:v", self.proxima_entrada);
        self.proxima_entrada += 1;
        r
    }

    /// Registra a camada e devolve o rótulo da entrada dela.
    fn entrada_camada(&mut self, letra: char, tipo: TipoCamada, caixa_px: [u32; 4], n: u32, itens: Vec<ItemCamada>, imagens: Vec<ImagemCamada>) -> String {
        let id = format!("{}{letra}{}", self.rot, self.camadas.len());
        let mut pedido = PedidoCamada { id: id.clone(), tipo, caixa_px, n, hash: String::new(), itens, imagens };
        pedido.hash = camada::hash_do_pedido(&pedido);
        self.camadas.push(pedido);
        self.entradas.push(Entrada {
            antes: ["-f", "concat", "-safe", "0"].iter().map(|s| s.to_string()).collect(),
            antes_sem_busca: None,
            caminho: PathBuf::new(),
            papel: Papel::Camada { id },
        });
        self.proxima()
    }

    fn sprites(&self) -> SpritesDoPlano<'_> {
        SpritesDoPlano { p: self.p, ctx: self.ctx }
    }

    /// `ocultar` sobre `base` (fonte já na cor interna), para os quadros `[ja, jb)` lidos da tabela.
    /// O recorte e o raio saem dos quadros `[ca, cb)` do clipe INTEIRO: desfoque e mosaico dependem do recorte, e
    /// a janela (quadro final, prévia, bloco) tem de dar os mesmos pixels da exportação.
    /// `mostrados` = quadros da fonte que o clipe mostra (os demais lidos são vizinhos: `caixa_no_lido`).
    fn ocultar(&mut self, base: String, midia: &str, efeitos_do_clipe: &[Tolerante<Efeito>], fonte: &FonteInfo, (ja, jb): (usize, usize), (ca, cb): (usize, usize), mostrados: (usize, usize)) -> Result<String> {
        let tab = &fonte.quadros;
        let espaco = TamanhoPx { w: fonte.exibido.0, h: fonte.exibido.1 };
        let midia_ref = self.p.midias.iter().find(|m| m.id == midia);
        let (mut os, mut mks, mut unioes, mut lados) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for o in ocultacoes_de(midia_ref, efeitos_do_clipe) {
            let efeito = Efeito::Ocultar(o.clone());
            // invertida: os quadros sem o alvo vão num segundo item, que cobre o quadro inteiro
            let inteiro = o.alvo.inverter.then(|| Efeito::Ocultar(quadro_inteiro(o)));
            let (mut quadros, mut prims, mut lado, mut algum) = (Vec::with_capacity(jb - ja), Vec::with_capacity(cb - ca), 0u32, false);
            let (mut quadros_inteiro, mut sem_alvo) = (Vec::new(), false);
            for j in ca..cb {
                let (mut rastreios, mut todo) = (BTreeMap::new(), BTreeMap::new());
                match caixa_no_lido(&mut self.rastreios, o, tab, j, mostrados)? {
                    NoLido::Caixa(a) => {
                        rastreios.insert(o.alvo.rastreio.clone(), a.caixa);
                        lado = lado.max((a.caixa.w * espaco.w as f64).min(a.caixa.h * espaco.h as f64).round().max(0.0) as u32);
                        algum |= (ja..jb).contains(&j);
                    }
                    NoLido::Ausente if inteiro.is_some() => {
                        todo.insert(o.alvo.rastreio.clone(), QUADRO_TODO);
                        // a região é o quadro inteiro: dá o raio do desfoque e a célula do mosaico
                        lado = lado.max(espaco.w.min(espaco.h));
                        algum |= (ja..jb).contains(&j);
                        sem_alvo |= (ja..jb).contains(&j);
                    }
                    _ => {}
                }
                // `q` e `programa_s` fixos: a máscara só depende da fonte e serve a todo clipe que lê esses quadros
                let q = QuadroCamada { q: j as u64, t: tab.tempo(j), pts: Some(tab.pts[j]), a: Afim::IDENTIDADE, canvas: espaco, rastreios, indice: Some(j as u64), programa_s: 0.0 };
                prims.push(formas::primitivas(&efeito, &q, &self.sprites())?);
                if let Some(e) = &inteiro {
                    let q = QuadroCamada { rastreios: todo, ..q.clone() };
                    prims.push(formas::primitivas(e, &q, &self.sprites())?);
                    if (ja..jb).contains(&j) {
                        quadros_inteiro.push(QuadroItem { quadro: q, transformacao: None });
                    }
                }
                if (ja..jb).contains(&j) {
                    quadros.push(QuadroItem { quadro: q, transformacao: None });
                }
            }
            let Some(uniao) = mascara::uniao_px(&prims, espaco).filter(|_| algum) else { continue };
            let mut itens = vec![ItemCamada { desenho: Desenho::Efeito { efeito }, de: 0, quadros }];
            // sem quadro sem o alvo, a camada fica a de antes (mesmo hash)
            if let Some(e) = inteiro.filter(|_| sem_alvo) {
                itens.push(ItemCamada { desenho: Desenho::Efeito { efeito: e }, de: 0, quadros: quadros_inteiro });
            }
            let ent = self.entrada_camada('m', TipoCamada::MascaraFonte, uniao, (jb - ja) as u32, itens, Vec::new());
            mks.push(anotacao::entrada_por_quadro(self.g, &ent, tab.tb, &tab.pts[ja..jb]));
            os.push(o);
            unioes.push(uniao);
            lados.push(lado);
        }
        regiao::ocultar(self.g, &base, &os, &mks, &unioes, &lados)
    }

    fn instante(r: &Rastreio, s: &Slot) -> Instante {
        match (r.base, s.fonte) {
            (BaseRastreio::Fonte, Some((t, pts, _))) => Instante { t, pts: Some(pts) },
            _ => Instante { t: s.tau, pts: None },
        }
    }

    /// Domínio do slot: enquadramento → holofote → anotação → formato, com o pts = slot local do clipe.
    /// `slots` = os `n` da janela, entre os `antes` da frente e os do fim que alinham a tabela do enquadramento animado.
    fn cauda(&mut self, base: String, origem: TamanhoPx, efeitos: &[Tolerante<Efeito>], slots: &[Slot], antes: usize, janela: Janela) -> Result<String> {
        let de = janela.de;
        let canvas_px = TamanhoPx { w: self.canvas.w, h: self.canvas.h };
        let fps = self.canvas.fps;
        let tempos: Vec<f64> = slots.iter().map(|s| s.tau).collect();
        // Fase 4 (contratos-fase4.md B4): o enquadramento que segue um rastreio vira chaves nos slots, antes de tudo
        let seguidos = crate::edicao::segue::na_cauda(efeitos, &tempos, &mut self.rastreios, self.ctx)?;
        let efeitos: &[Tolerante<Efeito>] = seguidos.as_deref().unwrap_or(efeitos);
        let f = enquadramento::filtros(efeitos, origem, &self.canvas, &tempos, de as usize - antes, antes)?;
        let slots = &slots[antes..antes + (janela.ate - janela.de) as usize];
        let n = slots.len() as u32;
        let mut atual = base;
        // lupas: cópias da base antes do enquadramento (pixels da origem do clipe)
        let lupas: Vec<&crate::edicao::modelo::EfLupa> = efeitos::ligados(efeitos, Familia::Anotacao)
            .into_iter()
            .filter_map(|e| match e {
                Efeito::Lupa(l) if lupa::recusa(l).is_none() => Some(l),
                _ => None,
            })
            .collect();
        let mut copias = Vec::new();
        if !lupas.is_empty() {
            let mut saidas = vec![self.g.rotulo("s")];
            saidas.extend((0..lupas.len()).map(|_| self.g.rotulo("l")));
            let refs: Vec<&str> = saidas.iter().map(String::as_str).collect();
            self.g.cadeia(&[&atual], &[format!("split={}", saidas.len())], &refs);
            atual = saidas[0].clone();
            copias = saidas[1..].to_vec();
        }
        if !f.is_empty() {
            let r = self.g.rotulo("e");
            self.g.cadeia(&[&atual], &[f], &[&r]);
            atual = r;
        }
        let quadros: Vec<QuadroCamada> = slots
            .iter()
            .map(|s| QuadroCamada {
                q: s.q,
                t: s.tau,
                pts: s.fonte.map(|f| f.1),
                a: geometria::a_de(efeitos, s.tau, origem, canvas_px),
                canvas: canvas_px,
                rastreios: BTreeMap::new(),
                indice: s.fonte.map(|f| f.2 as u64),
                programa_s: tempo::instante_do_slot(s.q, fps),
            })
            .collect();
        // holofote: máscara por slot, em px do canvas
        for e in efeitos::ligados(efeitos, Familia::Regiao) {
            let Efeito::Holofote(h) = e else { continue };
            let r = self.rastreios.achar(&h.alvo.rastreio)?;
            let (mut itens, mut prims) = (Vec::with_capacity(slots.len()), Vec::with_capacity(slots.len()));
            for (s, q) in slots.iter().zip(&quadros) {
                let mut q = q.clone();
                if efeitos::ativo_em(e, s.tau) {
                    if let Some(a) = self.rastreios.em(r, Self::instante(r, s))? {
                        q.rastreios.insert(r.id.clone(), a.caixa);
                    }
                }
                prims.push(formas::primitivas(e, &q, &self.sprites())?);
                itens.push(QuadroItem { quadro: q, transformacao: None });
            }
            if prims.iter().all(|p| p.is_empty()) {
                continue;
            }
            let Some(uniao) = mascara::uniao_px(&prims, canvas_px) else { continue };
            let item = ItemCamada { desenho: Desenho::Efeito { efeito: e.clone() }, de: 0, quadros: itens };
            let ent = self.entrada_camada('h', TipoCamada::MascaraCanvas, uniao, n, vec![item], Vec::new());
            let mk = anotacao::entrada_por_slot(self.g, &ent, fps, n, true);
            atual = regiao::holofote(self.g, &atual, h, &mk, uniao);
        }
        for (l, copia) in lupas.iter().zip(&copias) {
            let r = self.rastreios.achar(&l.rastreio)?;
            let mut qs = Vec::with_capacity(slots.len());
            for (s, q) in slots.iter().zip(&quadros) {
                let mut q = q.clone();
                if let Some(a) = self.rastreios.em(r, Self::instante(r, s))? {
                    q.rastreios.insert(r.id.clone(), a.caixa);
                }
                qs.push(q);
            }
            atual = lupa::aplicar(self, &atual, copia, l, &qs, origem)?;
        }
        // anotação: uma camada RGBA por slot com tudo que é desenhado
        let (mut itens, mut imagens, mut env, mut inteiro) = (Vec::new(), Vec::<ImagemCamada>::new(), None::<(f64, f64, f64, f64)>, false);
        for e in efeitos::ligados(efeitos, Familia::Anotacao) {
            if let Efeito::Imagem(i) = e {
                if !imagens.iter().any(|x| x.midia == i.midia) {
                    imagens.push(imagem_da_camada(self.p, self.ctx, &i.midia)?);
                }
            }
            let rastreio = match efeitos::rastreio_seguido(e) {
                Some(id) => Some(self.rastreios.achar(id)?),
                None => None,
            };
            // no quadro congelado o carimbo do original mostra o tempo de fonte do quadro, não o tempo local
            let tempo_de_fonte = matches!(e, Efeito::Carimbo(c) if matches!(c.mostra, MostraCarimbo::TempoFonte | MostraCarimbo::Relogio));
            let mut corrida: Option<(u32, Vec<QuadroItem>)> = None;
            for (i, (s, q)) in slots.iter().zip(&quadros).enumerate() {
                if !efeitos::ativo_em(e, s.tau) {
                    if let Some((de, qs)) = corrida.take() {
                        itens.push(ItemCamada { desenho: Desenho::Efeito { efeito: e.clone() }, de, quadros: qs });
                    }
                    continue;
                }
                let mut q = q.clone();
                if let (true, Some((t, _, _))) = (tempo_de_fonte, s.fonte) {
                    q.t = t;
                }
                if let Some(r) = rastreio {
                    if let Some(a) = self.rastreios.em(r, Self::instante(r, s))? {
                        q.rastreios.insert(r.id.clone(), a.caixa);
                    }
                }
                if !inteiro {
                    match formas::primitivas(e, &q, &self.sprites()) {
                        Ok(p) => unir(&mut env, formas::envolvente(&p, canvas_px)),
                        // texto: o tamanho só existe depois de rasterizar; a camada pega o canvas inteiro
                        Err(SicroError::Validation(m)) if m == SPRITE_PENDENTE => inteiro = true,
                        Err(outro) => return Err(outro),
                    }
                }
                corrida.get_or_insert_with(|| (i as u32, Vec::new())).1.push(QuadroItem { quadro: q, transformacao: None });
            }
            if let Some((de, qs)) = corrida.take() {
                itens.push(ItemCamada { desenho: Desenho::Efeito { efeito: e.clone() }, de, quadros: qs });
            }
        }
        let caixa = match (inteiro, env) {
            _ if itens.is_empty() => None,
            (true, _) => Some(canvas_par(&self.canvas)),
            (false, Some((a, b, c, d))) => Some(geometria::caixa_px_par(Caixa { x: a, y: b, w: c - a, h: d - b }, canvas_px)),
            (false, None) => None,
        };
        if let Some(caixa) = caixa {
            let ent = self.entrada_camada('a', TipoCamada::Anotacao, caixa, n, itens, imagens);
            let k = anotacao::entrada_por_slot(self.g, &ent, fps, n, false);
            atual = anotacao::sobrepor(self.g, &atual, &k, caixa[0], caixa[1])?;
        }
        let saida = format!("{}v", self.rot);
        let mut f = vec!["format=yuv420p".to_string(), "setsar=1".to_string()];
        if de > 0 {
            f.push(format!("setpts=PTS+{de}"));
        }
        self.g.cadeia(&[&atual], &f, &[&saida]);
        Ok(saida)
    }

    /// Fundo de 1 quadro na cor do canvas, do tamanho pedido.
    fn fundo(&mut self, t: TamanhoPx) -> Result<String> {
        let r = self.g.rotulo("f");
        let f = vec![
            format!("color=c=black:s={}x{}:r={}/{}", t.w, t.h, self.canvas.fps.num, self.canvas.fps.den),
            "trim=end_frame=1".to_string(),
            "format=yuv420p".to_string(),
            cor::filtro_cor_cheia(cor::ler(&self.canvas.cor_fundo)?),
            "setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv".to_string(),
        ];
        self.g.cadeia(&[], &f, &[&r]);
        Ok(r)
    }

    /// Camada de 1 quadro sobre o fundo, repetida por `n` slots: a "fonte" do clipe de imagem e do título.
    fn parado(&mut self, camada: &str, t: TamanhoPx, n: u32) -> Result<String> {
        let fundo = self.fundo(t)?;
        let k = anotacao::entrada_por_slot(self.g, camada, self.canvas.fps, 1, false);
        let s = anotacao::sobrepor(self.g, &fundo, &k, 0, 0)?;
        let r = self.g.rotulo("f");
        self.g.cadeia(&[&s], &["tpad=stop_mode=clone:stop=-1".to_string(), format!("trim=end_frame={n}"), "setpts=N".to_string()], &[&r]);
        Ok(r)
    }

    fn fechar(self, saida: String, conferir: Option<ConferirClipe>) -> CadeiaClipe {
        CadeiaClipe { saida, entradas: self.entradas, camadas: self.camadas, previas: self.previas, auxiliares: self.auxiliares, conferir }
    }
}

fn unir(env: &mut Option<(f64, f64, f64, f64)>, c: Option<Caixa>) {
    let Some(c) = c else { return };
    let n = (c.x, c.y, c.x + c.w, c.y + c.h);
    *env = Some(match *env {
        None => n,
        Some((a, b, cc, d)) => (a.min(n.0), b.min(n.1), cc.max(n.2), d.max(n.3)),
    });
}

/// Quadros `[ja, jb)` que alimentam os slots `[a, b)` do mapa. Clipe inteiro: todos os lidos (os descartados
/// também passam pelo `fps@`). Com pts repetido o corte por pts pega todos os quadros desse pts.
pub(crate) fn faixa_lida(tab: &TabelaQuadros, mapa: &MapaQuadros, a: usize, b: usize) -> (usize, usize) {
    let mut ja = if a == 0 { mapa.j_in } else { mapa.quadro_do_slot[a] as usize };
    let mut jb = if b >= mapa.n_q as usize { mapa.j_out } else { mapa.quadro_do_slot[b - 1] as usize + 1 };
    while ja > 0 && tab.pts[ja - 1] == tab.pts[ja] {
        ja -= 1;
    }
    while jb < tab.pts.len() && jb > 0 && tab.pts[jb] == tab.pts[jb - 1] {
        jb += 1;
    }
    (ja, jb)
}

fn trecho(mut m: Montagem, t: &ClipeTrecho, res: &ClipeResolvido, janela: Janela) -> Result<CadeiaClipe> {
    let fonte = m.ctx.fontes.get(&t.midia).ok_or_else(|| erro("midia_ausente", &t.midia))?;
    let tab = &fonte.quadros;
    // tabela do mapa: a da fonte, ou a de campos no desentrelace por campo (P1.4)
    let tab_clipe = realce::tabela(&t.base.efeitos, tab);
    let ff = res.fonte.as_ref().ok_or_else(|| erro("intervalo", &res.id))?;
    let mapa = tempo::mapa_trecho(&tab_clipe, ff.j_in as usize, ff.j_out as usize, &t.velocidade, m.canvas.fps)?;
    if mapa.n_q != res.n_q {
        return Err(erro("intervalo", format!("{}: a linha não é a desta tabela", res.id)));
    }
    let (a, b) = (janela.de as usize, janela.ate as usize);
    let faixa = faixa_lida(&tab_clipe, &mapa, a, b);
    let leitura = realce::leitura(&t.base.efeitos, tab, &tab_clipe, faixa, (mapa.j_in, mapa.j_out))?;
    let (ja, jb) = leitura.ler;
    // `ler` é da fonte e nunca vazio: leitura fora dela é erro, não índice estourado
    if ja >= jb || jb > tab.pts.len() {
        return Err(erro("intervalo", &res.id));
    }
    let alvo_busca = busca(tab, fonte.inicio_s, ja).map(|alvo| {
        let fim = if jb < tab.pts.len() { tab.tempo(jb) } else { tab.fim_s() } - fonte.inicio_s;
        (alvo, fim - alvo + 1.0)
    });
    let ent = m.entrada_midia(fonte, &t.midia, alvo_busca);
    // com `desentrelacar` ligado a cor sai por campo (P1.4): sem isso o croma do outro campo entra no quadro
    let cor = realce::cor_da_fonte(&t.base.efeitos, fonte).ok_or_else(|| erro("pixel_nao_suportado", &t.midia))?;
    let mut f = vec![match tab.pts.get(jb) {
        Some(fim) => format!("trim=start_pts={}:end_pts={fim}", tab.pts[ja]),
        None => format!("trim=start_pts={}", tab.pts[ja]),
    }];
    if m.exportacao_final {
        f.push(format!("showinfo@{}=checksum=0", m.rot));
    }
    f.push(cor);
    let na_cor = m.g.rotulo("f");
    m.g.cadeia(&[&ent], &f, &[&na_cor]);
    // quadros da fonte que o clipe mostra (por campo, o mapa conta campos: dois por quadro)
    let mostrados = if leitura.campos { (mapa.j_in / 2, mapa.j_out.div_ceil(2)) } else { (mapa.j_in, mapa.j_out) };
    let oculto = m.ocultar(na_cor, &t.midia, &t.base.efeitos, fonte, (ja, jb), leitura.clipe, mostrados)?;
    let oculto = if realce::ativos(&t.base.efeitos, true) { realce::aplicar(&mut m, &oculto, &t.base.efeitos, fonte, &leitura)? } else { oculto };
    // o clone do último quadro vai sempre: sem ele o 7.0.2 perde o último no `fps` (faixa cheia, fim da mídia,
    // filtro de duas entradas); o `fps@` conta lidos + 1
    let mut f = vec![
        "tpad=stop_mode=clone:stop=1".to_string(),
        format!("settb=1/{}", mapa.c),
        // P3.4: a constante pela fórmula, a rampa pela tabela de slots
        filtro("setpts", None, &[("expr", tempo::setpts(&tab_clipe, &mapa))]),
        format!("fps@{}={}/{}:start_time=0", m.rot, m.canvas.fps.num, m.canvas.fps.den),
        "tpad=stop_mode=clone:stop=-1".to_string(),
    ];
    if a == 0 {
        f.push(format!("trim=end_frame={b}"));
    } else {
        f.extend([format!("trim=start_frame={a}:end_frame={b}"), "setpts=PTS-STARTPTS".to_string()]);
    }
    let base = m.g.rotulo("f");
    m.g.cadeia(&[&oculto], &f, &[&base]);
    let (antes, fim) = alinhar(janela, res.n_q);
    let slots: Vec<Slot> = (a - antes..fim)
        .map(|q| {
            let j = mapa.quadro_do_slot[q] as usize;
            Slot { q: res.inicio_q + q as u64, tau: tab_clipe.tempo(j), fonte: Some((tab_clipe.tempo(j), tab_clipe.pts[j], j)) }
        })
        .collect();
    let saida = m.cauda(base, TamanhoPx { w: fonte.exibido.0, h: fonte.exibido.1 }, &t.base.efeitos, &slots, antes, janela)?;
    let conferir = ConferirClipe {
        rotulo: m.rot.clone(),
        clipe: res.id.clone(),
        lidos: leitura.lidos,
        pts: if m.exportacao_final { tab.pts[ja..jb].to_vec() } else { Vec::new() },
    };
    Ok(m.fechar(saida, Some(conferir)))
}

fn congelar(mut m: Montagem, c: &ClipeCongelar, res: &ClipeResolvido, janela: Janela) -> Result<CadeiaClipe> {
    let fonte = m.ctx.fontes.get(&c.midia).ok_or_else(|| erro("midia_ausente", &c.midia))?;
    let tab = &fonte.quadros;
    let j = res.fonte.as_ref().map(|f| f.j_in as usize).filter(|&j| j < tab.pts.len()).ok_or_else(|| erro("intervalo", &res.id))?;
    let pj = tab.pts[j];
    // o que se lê para entregar o quadro j: só ele, ou mais (média centrada, desentrelace) pela regra do P1.4
    let leitura = realce::leitura(&c.base.efeitos, tab, tab, (j, j + 1), (j, j + 1))?;
    let (ja, jb) = leitura.ler;
    if ja >= jb || jb > tab.pts.len() {
        return Err(erro("intervalo", &res.id));
    }
    let alvo_busca = busca(tab, fonte.inicio_s, ja).map(|alvo| {
        let fim = if jb < tab.pts.len() { tab.tempo(jb) } else { tab.fim_s() } - fonte.inicio_s;
        (alvo, fim - alvo + 1.0)
    });
    let ent = m.entrada_midia(fonte, &c.midia, alvo_busca);
    let cor = realce::cor_da_fonte(&c.base.efeitos, fonte).ok_or_else(|| erro("pixel_nao_suportado", &c.midia))?;
    // com pts repetido vale o primeiro quadro desse pts
    let mut f = vec![format!("trim=start_pts={}:end_pts={}", tab.pts[ja], tab.pts[jb - 1].saturating_add(1)), format!("trim=end_frame={}", jb - ja)];
    if m.exportacao_final {
        f.push(format!("showinfo@{}=checksum=0", m.rot));
    }
    f.push(cor);
    let na_cor = m.g.rotulo("f");
    m.g.cadeia(&[&ent], &f, &[&na_cor]);
    let efeitos_oc = efeitos_do_congelado(&c.base.efeitos, res.n_q, m.canvas.fps)?;
    let oculto = m.ocultar(na_cor, &c.midia, &efeitos_oc, fonte, (ja, jb), leitura.clipe, (j, j + 1))?;
    // média de quadros e desentrelace no congelado (P1.4): de `leitura.ler` para o quadro j (`leitura.lidos`)
    let oculto = if realce::ativos(&c.base.efeitos, true) { realce::aplicar(&mut m, &oculto, &c.base.efeitos, fonte, &leitura)? } else { oculto };
    let n = janela.ate - janela.de;
    let f = vec![
        "tpad=stop_mode=clone:stop=1".to_string(),
        format!("settb={}/{}", m.canvas.fps.den, m.canvas.fps.num),
        "setpts=N".to_string(),
        format!("fps@{}={}/{}:start_time=0", m.rot, m.canvas.fps.num, m.canvas.fps.den),
        "tpad=stop_mode=clone:stop=-1".to_string(),
        format!("trim=end_frame={n}"),
    ];
    let base = m.g.rotulo("f");
    m.g.cadeia(&[&oculto], &f, &[&base]);
    let fps = m.canvas.fps;
    let (antes, fim) = alinhar(janela, res.n_q);
    let slots: Vec<Slot> = (janela.de as usize - antes..fim)
        .map(|q| Slot { q: res.inicio_q + q as u64, tau: tempo::instante_do_slot(q as u64, fps), fonte: Some((tab.tempo(j), pj, j)) })
        .collect();
    let saida = m.cauda(base, TamanhoPx { w: fonte.exibido.0, h: fonte.exibido.1 }, &c.base.efeitos, &slots, antes, janela)?;
    let conferir = ConferirClipe { rotulo: m.rot.clone(), clipe: res.id.clone(), lidos: leitura.lidos, pts: if m.exportacao_final { tab.pts[ja..jb].to_vec() } else { Vec::new() } };
    Ok(m.fechar(saida, Some(conferir)))
}

/// Janela alargada até os múltiplos do bloco da tabela do enquadramento animado: (slots a mais na frente, fim).
fn alinhar(janela: Janela, n_q: u32) -> (usize, usize) {
    let b = expr::BLOCO_DA_TABELA;
    (janela.de as usize % b, (janela.ate as usize).div_ceil(b).saturating_mul(b).min(n_q as usize))
}

/// Slots de um clipe sem fonte, com os que alinham a tabela.
fn slots_locais(res: &ClipeResolvido, janela: Janela, fps: crate::edicao::modelo::Racional) -> (Vec<Slot>, usize) {
    let (antes, fim) = alinhar(janela, res.n_q);
    ((janela.de as usize - antes..fim).map(|q| Slot { q: res.inicio_q + q as u64, tau: tempo::instante_do_slot(q as u64, fps), fonte: None }).collect(), antes)
}

/// `ocultar` em clipe sem fonte (foto, título) não tem quadro lido onde entrar: recusado, nunca ignorado.
fn sem_ocultar(efeitos: &[Tolerante<Efeito>]) -> Result<()> {
    match efeitos.iter().find_map(|e| match e {
        Tolerante::Conhecido(Efeito::Ocultar(o)) if o.base.ligado => Some(o),
        _ => None,
    }) {
        Some(o) => Err(erro("nao_suportado", format!("efeito ocultar {} em clipe sem fonte", o.base.id))),
        None => Ok(()),
    }
}

fn imagem(mut m: Montagem, c: &ClipeImagem, res: &ClipeResolvido, janela: Janela) -> Result<CadeiaClipe> {
    sem_ocultar(&c.base.efeitos)?;
    let img = imagem_da_camada(m.p, m.ctx, &c.midia)?;
    // a foto é a origem do clipe, no tamanho dela (lados pares): 1 px da imagem = 1 px da camada
    let t = TamanhoPx { w: (img.tamanho.w / 2 * 2).max(2), h: (img.tamanho.h / 2 * 2).max(2) };
    let (iw, ih) = (img.tamanho.w as f64, img.tamanho.h as f64);
    let quadro = QuadroCamada { q: 0, t: 0.0, pts: None, a: Afim::IDENTIDADE, canvas: t, rastreios: BTreeMap::new(), indice: None, programa_s: 0.0 };
    let tr = TransformacaoAvaliada { x: iw * 0.5 / t.w as f64, y: ih * 0.5 / t.h as f64, escala: iw / t.w as f64, rotacao: 0.0, opacidade: 1.0 };
    let item = ItemCamada { desenho: Desenho::Imagem { midia: c.midia.clone() }, de: 0, quadros: vec![QuadroItem { quadro, transformacao: Some(tr) }] };
    let ent = m.entrada_camada('i', TipoCamada::Anotacao, [0, 0, t.w, t.h], 1, vec![item], vec![img]);
    let base = m.parado(&ent, t, janela.ate - janela.de)?;
    let (slots, antes) = slots_locais(res, janela, m.canvas.fps);
    let saida = m.cauda(base, t, &c.base.efeitos, &slots, antes, janela)?;
    Ok(m.fechar(saida, None))
}

fn titulo(mut m: Montagem, c: &ClipeTitulo, res: &ClipeResolvido, janela: Janela) -> Result<CadeiaClipe> {
    sem_ocultar(&c.base.efeitos)?;
    let caixa = canvas_par(&m.canvas);
    let t = TamanhoPx { w: m.canvas.w, h: m.canvas.h };
    let quadro = QuadroCamada { q: 0, t: 0.0, pts: None, a: Afim::IDENTIDADE, canvas: t, rastreios: BTreeMap::new(), indice: None, programa_s: 0.0 };
    let item = ItemCamada { desenho: Desenho::Titulo { titulo: c.titulo.clone() }, de: 0, quadros: vec![QuadroItem { quadro, transformacao: None }] };
    let imagens = imagens_do_titulo(m.p, m.ctx, &c.titulo)?;
    let ent = m.entrada_camada('t', TipoCamada::Anotacao, caixa, 1, vec![item], imagens);
    let base = m.parado(&ent, t, janela.ate - janela.de)?;
    let (slots, antes) = slots_locais(res, janela, m.canvas.fps);
    let saida = m.cauda(base, t, &c.base.efeitos, &slots, antes, janela)?;
    Ok(m.fechar(saida, None))
}

/// Escreve no grafo a cadeia do clipe para a janela. `n` = número do clipe no bloco (rótulos `c003…`);
/// `primeira_entrada` = índice que a primeira entrada dele terá no ffmpeg.
pub fn cadeia(
    g: &mut Grafo,
    p: &ProjetoEdicao,
    clipe: &ClipePrincipal,
    res: &ClipeResolvido,
    janela: Janela,
    n: usize,
    primeira_entrada: usize,
    ctx: &Contexto,
    alvo: &Alvo,
) -> Result<CadeiaClipe> {
    if janela.de >= janela.ate || janela.ate > res.n_q {
        return Err(erro("intervalo", format!("{}: janela {}..{} de {}", res.id, janela.de, janela.ate, res.n_q)));
    }
    let efeitos = match clipe {
        ClipePrincipal::Trecho(x) => &x.base.efeitos,
        ClipePrincipal::Congelar(x) => &x.base.efeitos,
        ClipePrincipal::Imagem(x) => &x.base.efeitos,
        ClipePrincipal::Titulo(x) => &x.base.efeitos,
        ClipePrincipal::Tela(x) => &x.base.efeitos,
    };
    recusar_de_outra_fase(efeitos, matches!(clipe, ClipePrincipal::Trecho(_) | ClipePrincipal::Congelar(_)))?;
    let m = Montagem {
        g,
        p,
        ctx,
        canvas: super::canvas_do_alvo(p, alvo),
        rot: args::nome("c", n, ""),
        clipe: res.id.clone(),
        proxima_entrada: primeira_entrada,
        entradas: Vec::new(),
        camadas: Vec::new(),
        previas: Vec::new(),
        auxiliares: Vec::new(),
        rastreios: Rastreios::novo(p, ctx),
        exportacao_final: matches!(alvo, Alvo::Final { .. }),
    };
    match clipe {
        ClipePrincipal::Trecho(x) => trecho(m, x, res, janela),
        ClipePrincipal::Congelar(x) => congelar(m, x, res, janela),
        ClipePrincipal::Imagem(x) => imagem(m, x, res, janela),
        ClipePrincipal::Titulo(x) => titulo(m, x, res, janela),
        ClipePrincipal::Tela(x) => Err(erro("nao_suportado", format!("clipe tela {}", x.base.id))),
    }
}

/// Argumentos de busca de uma leitura que começa no quadro `j_in`: Some((-ss em s, relativo ao início do arquivo))
/// ou None quando a regra do §5.3 proíbe a busca (a menos de 10 s do início, sem chave conhecida, `SemBusca`).
pub fn busca(tab: &crate::edicao::quadros::TabelaQuadros, inicio_s: f64, j_in: usize) -> Option<f64> {
    tab.busca_em(inicio_s, j_in)
}
