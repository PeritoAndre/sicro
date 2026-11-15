//! Validação única (§2.3): o front não valida, só mostra. Dono: o pacote R da fase (P1.R).
//! O que o motor ainda não faz (`nao_suportado`) é perguntado à regra do pacote dono do recurso (`recusa` em
//! `motor::{juncao, audio, legendas, saida}`, `motor::efeitos::{lupa, realce}`, `medidas`): uma regra só (contratos (h) 1).
//! A estrutura de cada item (lista de chaves vazia, chaves fora de ordem) é conferida antes de qualquer curva
//! dele ser avaliada (contratos (f) 7). Toda frase sai de `mensagens`.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::armazem::id_valido;
use super::camada;
use super::cor;
use super::geometria;
use super::mensagens::{self, codigo::*};
use super::modelo::{
    Analise, Animavel, BaseRastreio, Caixa, Chave, ClipePrincipal, ClipeSobreposto, Codec, ConteudoOculto, Efeito, EfOcultar, EstiloTexto,
    Exportacao, FormaDestaque, FormaRegiao, Interpolacao, Lugar, AlinhamentoV, LinhaResolvida, MetodoSincronia, MidiaRef, ModoLegendas,
    ModoOcultar, MostraCarimbo, Nivel, OndePrender, OndeVelocidade, OrigemLegenda, OrigemRastreio, EstadoPessoa, Posicao, Predefinicao,
    Prender, Problema, ProjetoEdicao, Racional, Rastreio, RefLegenda, SituacaoMidia, TamanhoPx, TipoDerivacao, TipoMidia, TipoRastreio,
    Titulo, Fundo, Tolerante, Velocidade, ESQUEMA as ESQUEMA_DOC, ESQUEMA_MENOR as ESQUEMA_MENOR_DOC,
};
use super::modelo::{ClipeTrecho, EfBase};
use super::medidas;
use super::motor::efeitos;
use super::motor::efeitos::{enquadramento, lupa, realce};
use super::motor::{audio as motor_audio, juncao, legendas as motor_legendas, saida as motor_saida};
use super::motor::{clipe as motor_clipe, Alvo, Contexto};
use super::quadros::{OrigemTabela, TabelaQuadros};
use super::rastreio::avaliar::{self as rastreio_avaliar, Como, Instante};
use super::tempo;

/// `tipo` conhecidos (o resto é item de versão mais nova).
pub(crate) const TIPOS_CLIPE: &[&str] = &["trecho", "congelar", "imagem", "titulo", "tela"];
pub(crate) const TIPOS_SOBREPOSTO: &[&str] = &["video", "imagem", "titulo"];
pub(crate) const TIPOS_EFEITO: &[&str] = &[
    "estabilizar", "lente", "desentrelacar", "media_temporal", "ajustes", "nitidez", "girar", "enquadramento", "ocultar", "holofote",
    "destaque", "lupa", "regua", "velocidade_exibida", "texto", "carimbo", "imagem",
];
/// O mesmo teto do rasterizador (`camada::texto`): texto maior é `item_invalido`.
const LIMITE_TEXTO: usize = 20_000;
/// Duração máxima de congelar, imagem e título (s).
const DURACAO_MAX_S: f64 = 3600.0;
/// Enquadramento: menor largura da região (0..1).
const REGIAO_MIN_W: f64 = 0.02;
/// Área segura do texto: 2 %, o canto da grade de posição de cada lado.
const AREA_SEGURA: f64 = camada::titulo::AREA_SEGURA;
/// Ampliação acima disso é aviso.
const AMPLIACAO_MAX: f64 = 4.0;
/// Desfoque abaixo disso é fraco (medido em texto, §2.3).
const DESFOQUE_MIN: f64 = 0.6;
/// Mosaico com mais células que isso no lado menor é fraco.
const MOSAICO_MAX_CELULAS: f64 = 6.0;
/// Carimbo de relógio a mais disso do ponto de vínculo (s).
const RELOGIO_MAX_S: f64 = 600.0;
/// Taxa variável (`receitas-audio-saida.md` §8): tolerância mínima do passo (s), parte dos passos fora de ±25 % e lacuna.
const VFR_TOLERANCIA_S: f64 = 0.0015;
const VFR_FAIXA: f64 = 0.25;
const VFR_PARTE: f64 = 0.005;
const VFR_LACUNA: f64 = 1.5;
/// Folga das comparações de tempo (s).
const EPS: f64 = 1e-9;
/// Ponto de apoio de texto, carimbo, imagem ou bloco de título mais longe que isto do canvas (px, 2^30): `item_invalido`.
const MAX_LUGAR_PX: f64 = 1_073_741_824.0;
/// Maior lado de um sprite de texto (px): o teto de `camada::texto` (2^26 px no total). O canto do bloco de título
/// fica no máximo a isto do ponto de apoio.
const MAX_SPRITE_PX: f64 = 67_108_864.0;
/// Faixas dos parâmetros (§2.3): fora delas, `item_invalido` com o campo.
const BORDA_SUAVE_MAX: f64 = 0.5;
const MEDIA_QUADROS: std::ops::RangeInclusive<u32> = 2..=15;

// ---------------------------------------------------------------------------
// API

/// Problemas que se veem no JSON cru, antes da leitura tipada: `esquema`, `esquema_menor`, id.
pub fn esquema(bruto: &Value) -> Vec<Problema> {
    let com_campo = |codigo: &'static str, campo: &str| {
        let mut p = mensagens::problema(codigo, None, None);
        p.campo = Some(campo.into());
        p
    };
    let Some(doc) = bruto.as_object() else { return vec![mensagens::problema(ESQUEMA, None, None)] };
    let mut v = Vec::new();
    if doc.get("esquema").and_then(Value::as_u64) != Some(ESQUEMA_DOC as u64) {
        v.push(com_campo(ESQUEMA, "esquema"));
    }
    if !doc.get("id").and_then(Value::as_str).is_some_and(id_valido) {
        v.push(com_campo(ESQUEMA, "id"));
    }
    if doc.get("esquema_menor").and_then(Value::as_u64).unwrap_or(0) > ESQUEMA_MENOR_DOC as u64 {
        v.push(com_campo(ESQUEMA_MENOR, "esquema_menor"));
    }
    v
}

/// Todas as regras do §2.3 sobre o projeto tipado; a situação das mídias vem de `ctx.midias`.
/// `alvo` Some(Final) acrescenta as regras da exportação final (`tabela_pendente`, limites do codec).
/// É a mesma função que `motor::compilar` chama antes de compilar.
pub fn validar(p: &ProjetoEdicao, linha: &LinhaResolvida, ctx: &Contexto, alvo: Option<&Alvo>) -> Vec<Problema> {
    let prender = tempo::prender(p, ctx);
    Validacao::nova(p, linha, ctx, alvo).rodar(&prender)
}

/// Resposta de `edicao_analisar`: resolve a linha, valida, lista os tempos a prender e a saída automática
/// (`Analise.midias` = cópia de `ctx.midias`).
pub fn analisar(p: &ProjetoEdicao, ctx: &Contexto, alvo: Option<&Alvo>) -> Analise {
    let linha = tempo::resolver(p, ctx);
    let prender = tempo::prender(p, ctx);
    let problemas = Validacao::nova(p, &linha, ctx, alvo).rodar(&prender);
    Analise { problemas, linha, midias: ctx.midias.to_vec(), prender, saida_auto: tempo::saida_auto(p, ctx) }
}

/// Código de um item que caiu em `Desconhecido`: `tipo` da lista = `item_invalido`; senão, de versão mais nova.
pub(crate) fn codigo_do_desconhecido(v: &Value, tipos: &[&str]) -> &'static str {
    if v.get("tipo").and_then(Value::as_str).is_some_and(|t| tipos.contains(&t)) {
        ITEM_INVALIDO
    } else {
        ITEM_DESCONHECIDO
    }
}

/// Erro do motor para um item desconhecido que vale no render (o mesmo código da validação).
pub(crate) fn erro_do_desconhecido(v: &Value, tipos: &[&str]) -> crate::error::SicroError {
    let texto = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    crate::error::SicroError::Validation(format!("{}: {} {}", codigo_do_desconhecido(v, tipos), texto("tipo"), texto("id")))
}

/// Efeito desconhecido vale no render? Só `"ligado": false` o desliga (o campo pode faltar).
pub(crate) fn efeito_desconhecido_ligado(v: &Value) -> bool {
    v.get("ligado").and_then(Value::as_bool).unwrap_or(true)
}

/// Há algum problema de nível erro?
pub fn tem_erro(problemas: &[Problema]) -> bool {
    problemas.iter().any(|p| p.nivel == Nivel::Erro)
}

/// Códigos periciais presentes de que o perito ainda não deu ciência (`ciencias` = códigos já aceitos).
/// Cada código uma vez, na ordem em que aparece.
pub fn sem_ciencia(problemas: &[Problema], ciencias: &[String]) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for p in problemas.iter().filter(|p| p.nivel == Nivel::Pericial) {
        if !ciencias.iter().any(|c| c == p.codigo) && !v.contains(&p.codigo) {
            v.push(p.codigo);
        }
    }
    v
}

// ---------------------------------------------------------------------------
// Onde o problema está

#[derive(Debug, Clone, Default)]
struct Onde {
    clipe: Option<String>,
    trilha: Option<String>,
    efeito: Option<String>,
    midia: Option<String>,
    campo: Option<&'static str>,
}

impl Onde {
    fn clipe(id: &str) -> Self {
        Onde { clipe: Some(id.into()), ..Default::default() }
    }
    fn midia(id: &str) -> Self {
        Onde { midia: Some(id.into()), ..Default::default() }
    }
    fn trilha(mut self, id: &str) -> Self {
        self.trilha = Some(id.into());
        self
    }
    fn efeito(&self, id: &str) -> Self {
        Onde { efeito: Some(id.into()), ..self.clone() }
    }
    fn com_midia(&self, id: &str) -> Self {
        Onde { midia: Some(id.into()), ..self.clone() }
    }
    fn campo(&self, campo: &'static str) -> Self {
        Onde { campo: Some(campo), ..self.clone() }
    }
}

// ---------------------------------------------------------------------------
// Uso: o clipe que leva os efeitos, com a base de tempo e os quadros em que o rastreio é avaliado

#[derive(Debug, Clone, Copy, PartialEq)]
enum Base {
    /// Trecho: `ativo` e âncoras em tempo de fonte, um quadro lido por amostra.
    Fonte,
    /// Congelar: `ativo` local, rastreio de fonte amostrado no instante.
    Congelado,
    /// Imagem, título e sobrepostos sem vídeo: tudo local, uma amostra por slot.
    Local,
}

/// Um instante avaliado: `inst` = instante na base do `ativo` (o do quadro lido no trecho, o do slot nos demais);
/// `t`/`pts` = onde o rastreio é avaliado.
#[derive(Debug, Clone, Copy)]
struct Amostra {
    inst: f64,
    t: f64,
    pts: Option<i64>,
}

struct Uso<'a> {
    onde: Onde,
    /// `MidiaRef.id` mostrado pelo clipe (trecho, congelar); None nos clipes de base local.
    midia: Option<&'a str>,
    base: Base,
    efeitos: &'a [Tolerante<Efeito>],
    tab: Option<&'a TabelaQuadros>,
    /// Quadros lidos `[j_in, j_out)` (trecho) ou o quadro congelado.
    quadros: Option<(usize, usize)>,
    /// Slots do clipe (0 quando a duração já é inválida: nada a percorrer).
    n_q: u32,
    fps: Racional,
    /// Tamanho da origem do clipe em px (a fonte exibida, a foto em lados pares, o canvas no título); None =
    /// desconhecido (mídia ausente, sobreposto).
    origem: Option<TamanhoPx>,
}

impl Uso<'_> {
    /// Amostras do clipe, na base dos efeitos dele: uma por quadro lido no trecho, uma por slot nos demais
    /// (no congelado o rastreio de fonte é avaliado no quadro dele em todo slot).
    fn cada(&self, mut f: impl FnMut(Amostra)) {
        match (self.base, self.tab, self.quadros) {
            (Base::Fonte, Some(tab), Some((a, b))) => {
                for j in a..b {
                    let t = tab.tempo(j);
                    f(Amostra { inst: t, t, pts: Some(tab.pts[j]) });
                }
            }
            (Base::Congelado, Some(tab), Some((j, _))) => {
                for k in 0..self.n_q {
                    f(Amostra { inst: tempo::instante_do_slot(k as u64, self.fps), t: tab.tempo(j), pts: Some(tab.pts[j]) });
                }
            }
            (Base::Local, _, _) => {
                for k in 0..self.n_q {
                    let t = tempo::instante_do_slot(k as u64, self.fps);
                    f(Amostra { inst: t, t, pts: None });
                }
            }
            _ => {}
        }
    }

    /// Amostras na base de tempo da mídia (as ocultações da mídia): os quadros lidos.
    fn cada_na_fonte(&self, mut f: impl FnMut(Amostra)) {
        match (self.base, self.tab, self.quadros) {
            (Base::Fonte, ..) => self.cada(f),
            (Base::Congelado, Some(tab), Some((j, _))) => f(Amostra { inst: tab.tempo(j), t: tab.tempo(j), pts: Some(tab.pts[j]) }),
            _ => {}
        }
    }
}

/// Um trecho medido: quadros da FONTE que ele mostra `[a, b)` e o tempo de fonte do quadro de cada slot.
struct Lido {
    fonte: (usize, usize),
    tempos: Vec<f64>,
}

/// O efeito vale na amostra? A regra do motor (`efeitos::base_ativa`: intervalo fechado no instante do quadro).
fn ativo_em(base: &EfBase, a: &Amostra) -> bool {
    efeitos::base_ativa(base, a.inst)
}

// ---------------------------------------------------------------------------
// Validação

/// Código e lugar: o mesmo problema no mesmo lugar sai uma vez.
type ChaveProblema = (&'static str, Option<String>, Option<String>, Option<String>, Option<String>, Option<&'static str>);

struct Validacao<'a> {
    p: &'a ProjetoEdicao,
    linha: &'a LinhaResolvida,
    ctx: &'a Contexto<'a>,
    exportacao: &'a Exportacao,
    final_: bool,
    /// Alvo que exporta (a exportação final ou a análise sem alvo): a predefinição pesa.
    exporta: bool,
    /// Alvo com som (todos menos o quadro final).
    com_som: bool,
    fps: Racional,
    midias: HashMap<&'a str, &'a MidiaRef>,
    rastreios: HashMap<&'a str, &'a Rastreio>,
    /// Clipes da principal ligados e conhecidos (os que podem ancorar clipes livres).
    na_linha: HashSet<&'a str>,
    /// Mídias usadas no render, na ordem do primeiro uso.
    usadas: Vec<&'a str>,
    /// As que o ffmpeg lê como vídeo (trecho, quadro congelado): só estas passam pela cadeia de cor da fonte.
    como_video: HashSet<&'a str>,
    saida: Vec<Problema>,
    vistos: HashSet<ChaveProblema>,
}

impl<'a> Validacao<'a> {
    fn nova(p: &'a ProjetoEdicao, linha: &'a LinhaResolvida, ctx: &'a Contexto<'a>, alvo: Option<&'a Alvo>) -> Self {
        let (exportacao, final_) = match alvo {
            Some(Alvo::Final { exportacao }) => (exportacao, true),
            _ => (&p.saida.exportacao, false),
        };
        Validacao {
            p,
            linha,
            ctx,
            exportacao,
            final_,
            exporta: matches!(alvo, None | Some(Alvo::Final { .. })),
            com_som: !matches!(alvo, Some(Alvo::Quadro { .. })),
            fps: p.saida.fps,
            midias: p.midias.iter().map(|m| (m.id.as_str(), m)).collect(),
            rastreios: p.rastreios.iter().map(|r| (r.id.as_str(), r)).collect(),
            na_linha: HashSet::new(),
            usadas: Vec::new(),
            como_video: HashSet::new(),
            saida: Vec::new(),
            vistos: HashSet::new(),
        }
    }

    fn fps_ok(&self) -> bool {
        self.fps.num > 0 && self.fps.den > 0
    }

    fn rodar(mut self, prender: &[Prender]) -> Vec<Problema> {
        self.documento();
        self.ids();
        self.saida_do_projeto();
        for c in &self.p.principal {
            if let Tolerante::Conhecido(c) = c {
                let b = base_clipe(c);
                if !b.desligado {
                    self.na_linha.insert(b.id.as_str());
                }
            }
        }
        self.principal();
        self.transicoes();
        self.sobreposicoes();
        self.audios();
        self.fase_0();
        self.legendas();
        self.marcadores();
        self.midias_usadas();
        self.ajustados(prender);
        self.saida
    }

    // ---- registro

    fn add(&mut self, codigo: &'static str, onde: &Onde, n: Option<u64>, detalhe: Option<&str>, nivel: Option<Nivel>) {
        let chave = (codigo, onde.clipe.clone(), onde.efeito.clone(), onde.trilha.clone(), onde.midia.clone(), onde.campo);
        if !self.vistos.insert(chave) {
            return;
        }
        let mut p = mensagens::problema(codigo, n, detalhe);
        if let Some(nv) = nivel {
            p.nivel = nv;
        }
        p.clipe = onde.clipe.clone();
        p.efeito = onde.efeito.clone();
        p.trilha = onde.trilha.clone();
        p.midia = onde.midia.clone();
        p.campo = onde.campo.map(str::to_string);
        self.saida.push(p);
    }

    fn por(&mut self, codigo: &'static str, onde: &Onde) {
        self.add(codigo, onde, None, None, None);
    }

    fn por_n(&mut self, codigo: &'static str, onde: &Onde, n: u64) {
        self.add(codigo, onde, Some(n), None, None);
    }

    fn usar(&mut self, midia: &'a str) {
        if !self.usadas.contains(&midia) {
            self.usadas.push(midia);
        }
    }

    /// Cor no formato `#rrggbb[aa]`.
    fn cor(&mut self, c: &str, onde: &Onde) {
        if cor::ler(c).is_err() {
            self.por(COR, onde);
        }
    }

    /// Item que caiu em `Desconhecido`: tipo da lista = inválido (com a mensagem do serde); senão, de versão mais nova.
    /// Erro quando vale no render; aviso quando está desligado (a tela o trata como desconhecido do mesmo jeito).
    fn desconhecido<T: serde::de::DeserializeOwned>(&mut self, v: &Value, tipos: &[&str], valendo: bool, onde: &Onde) {
        let nivel = if valendo { Nivel::Erro } else { Nivel::Aviso };
        if codigo_do_desconhecido(v, tipos) == ITEM_INVALIDO {
            let msg = serde_json::from_value::<T>(v.clone()).err().map(|e| e.to_string());
            self.add(ITEM_INVALIDO, onde, None, msg.as_deref(), Some(nivel));
        } else {
            self.add(ITEM_DESCONHECIDO, onde, None, None, Some(nivel));
        }
    }

    // ---- documento e saída

    fn documento(&mut self) {
        if self.p.esquema != ESQUEMA_DOC {
            self.por(ESQUEMA, &Onde::default().campo("esquema"));
        }
        if !id_valido(&self.p.id) {
            self.por(ESQUEMA, &Onde::default().campo("id"));
        }
        if self.p.esquema_menor > ESQUEMA_MENOR_DOC {
            self.por(ESQUEMA_MENOR, &Onde::default().campo("esquema_menor"));
        }
    }

    fn id(&mut self, id: &str, onde: &Onde) {
        if !id_valido(id) {
            self.por(ESQUEMA, onde);
        }
    }

    /// Id que é chave de busca (mídia, rastreio, item da linha do tempo) não se repete: a busca pegaria um só.
    fn unico(&mut self, vistos: &mut HashSet<&'a str>, id: &'a str, onde: &Onde) {
        self.id(id, onde);
        if !vistos.insert(id) {
            self.por(ESQUEMA, onde);
        }
    }

    fn ids_dos_efeitos(&mut self, efeitos: &[Tolerante<Efeito>], onde: &Onde) {
        for e in efeitos {
            if let Tolerante::Conhecido(e) = e {
                let id = &base_efeito(e).id;
                self.id(id, &onde.efeito(id).campo("id"));
            }
        }
    }

    /// Todo id do documento é `[A-Za-z0-9_-]{1,80}`: vira rótulo de grafo, nome de arquivo e chave de busca.
    fn ids(&mut self) {
        let p = self.p;
        let mut vistos = HashSet::new();
        for m in &p.midias {
            let onde = Onde::midia(&m.id);
            self.unico(&mut vistos, &m.id, &onde.campo("id"));
            for o in &m.ocultacoes {
                self.id(&o.base.id, &onde.efeito(&o.base.id).campo("id"));
            }
        }
        let mut vistos = HashSet::new();
        for r in &p.rastreios {
            self.unico(&mut vistos, &r.id, &Onde::default().campo("rastreios"));
        }
        for s in &p.sincronias {
            self.id(&s.id, &Onde::default().campo("sincronias"));
        }
        // itens da linha do tempo: um espaço de nomes só (`Problema.clipe`)
        let mut vistos = HashSet::new();
        for c in &p.principal {
            let Tolerante::Conhecido(c) = c else { continue };
            let b = base_clipe(c);
            let onde = Onde::clipe(&b.id);
            self.unico(&mut vistos, &b.id, &onde.campo("id"));
            self.ids_dos_efeitos(&b.efeitos, &onde);
            match c {
                ClipePrincipal::Trecho(t) => t.audio.silencios.iter().for_each(|s| self.id(&s.id, &onde.campo("silencios"))),
                ClipePrincipal::Titulo(t) => t.titulo.blocos.iter().for_each(|x| self.id(&x.id, &onde.campo("blocos"))),
                ClipePrincipal::Tela(t) => {
                    t.audio.silencios.iter().for_each(|s| self.id(&s.id, &onde.campo("silencios")));
                    t.celulas.iter().for_each(|cel| self.ids_dos_efeitos(&cel.efeitos, &onde));
                }
                ClipePrincipal::Congelar(_) | ClipePrincipal::Imagem(_) => {}
            }
        }
        for t in &p.sobreposicoes {
            self.id(&t.id, &Onde::default().trilha(&t.id).campo("id"));
            for c in &t.clipes {
                let Tolerante::Conhecido(c) = c else { continue };
                let b = base_sobreposto(c);
                let onde = Onde::clipe(&b.id).trilha(&t.id);
                self.unico(&mut vistos, &b.id, &onde.campo("id"));
                self.ids_dos_efeitos(&b.efeitos, &onde);
                if let ClipeSobreposto::Titulo(x) = c {
                    x.titulo.blocos.iter().for_each(|k| self.id(&k.id, &onde.campo("blocos")));
                }
            }
        }
        for t in &p.audios {
            self.id(&t.id, &Onde::default().trilha(&t.id).campo("id"));
            for c in &t.clipes {
                self.unico(&mut vistos, &c.id, &Onde::clipe(&c.id).trilha(&t.id).campo("id"));
            }
        }
        for g in &p.legendas.itens {
            self.unico(&mut vistos, &g.id, &Onde::clipe(&g.id).campo("id"));
        }
        for m in &p.marcadores {
            self.unico(&mut vistos, &m.id, &Onde::clipe(&m.id).campo("id"));
        }
    }

    fn saida_do_projeto(&mut self) {
        let p = self.p;
        let s = &p.saida;
        let lado = |v: u32, max: u32| v.is_multiple_of(2) && (64..=max).contains(&v);
        if !lado(s.largura, 7680) {
            self.por(SAIDA, &Onde::default().campo("largura"));
        }
        if !lado(s.altura, 4320) {
            self.por(SAIDA, &Onde::default().campo("altura"));
        }
        let (num, den) = (s.fps.num as i128, s.fps.den as i128);
        if !(num > 0 && den > 0 && num >= den && num <= 120 * den) {
            self.por(SAIDA, &Onde::default().campo("fps"));
        }
        // H.264: lado maior até 4096 (o NVENC recusa acima; §5.6)
        let e = self.exportacao;
        let (h264, altura_max) = match (e.predefinicao, &e.personalizada) {
            (Predefinicao::Juri, _) => (true, None),
            (Predefinicao::Personalizada, Some(x)) => (x.codec == Codec::H264, x.altura_max),
            (Predefinicao::Personalizada, None) => {
                self.por(SAIDA, &Onde::default().campo("exportacao"));
                (false, None)
            }
            // WhatsApp sai em até 1280×720; Preservação é FFV1
            _ => (false, None),
        };
        if self.final_ && h264 {
            let (mut w, mut h) = (s.largura as f64, s.altura as f64);
            if let Some(m) = altura_max.filter(|&m| m > 0 && (m as f64) < h) {
                w = w * m as f64 / h;
                h = m as f64;
            }
            if w.max(h) > 4096.0 {
                self.por(SAIDA, &Onde::default().campo("largura"));
            }
        }
        self.cor(&s.cor_fundo, &Onde::default().campo("cor_fundo"));
        let desligados = [s.selos.velocidade, s.selos.ampliacao, s.selos.realce].iter().filter(|x| !**x).count() as u64;
        if desligados > 0 {
            self.por_n(SELO_DESLIGADO, &Onde::default().campo("selos"), desligados);
        }
        let m = &p.mixagem;
        if !(0.0..=2.0).contains(&m.volume_original) {
            self.por(AUDIO, &Onde::default().campo("volume_original"));
        }
        // o que o motor de áudio recusa ou ajustaria calado (a regra é dele)
        if let Some(campo) = motor_audio::mixagem_invalida(m) {
            self.por(AUDIO, &Onde::default().campo(campo));
        }
    }

    // ---- principal

    fn principal(&mut self) {
        let p = self.p;
        let mut ligados = 0;
        for c in &p.principal {
            match c {
                Tolerante::Desconhecido(v) => {
                    let valendo = !v.get("desligado").and_then(Value::as_bool).unwrap_or(false);
                    let onde = Onde { clipe: v.get("id").and_then(Value::as_str).map(str::to_string), ..Default::default() };
                    self.desconhecido::<ClipePrincipal>(v, TIPOS_CLIPE, valendo, &onde);
                }
                Tolerante::Conhecido(c) => {
                    let b = base_clipe(c);
                    let onde = Onde::clipe(&b.id);
                    if b.desligado {
                        self.so_desconhecidos(&b.efeitos, &onde);
                        continue;
                    }
                    ligados += 1;
                    self.clipe(c, onde);
                }
            }
        }
        if ligados == 0 {
            self.por(VAZIO, &Onde::default().campo("principal"));
        }
    }

    /// Efeitos de item que não vale no render: só o que a tela tem de tratar como desconhecido.
    fn so_desconhecidos(&mut self, efeitos: &[Tolerante<Efeito>], onde: &Onde) {
        for e in efeitos {
            if let Tolerante::Desconhecido(v) = e {
                let o = efeito_onde(onde, v);
                self.desconhecido::<Efeito>(v, TIPOS_EFEITO, false, &o);
            }
        }
    }

    fn n_q_de(&self, id: &str) -> u32 {
        self.linha.clipes.iter().find(|c| c.id == id).map_or(1, |c| c.n_q)
    }

    fn midia_existe(&mut self, id: &'a str, onde: &Onde) -> bool {
        self.usar(id);
        if self.midias.contains_key(id) {
            true
        } else {
            self.por(MIDIA_AUSENTE, &onde.com_midia(id));
            false
        }
    }

    /// Mídia mostrada como foto: tem de ser do tipo imagem (o rasterizador lê o arquivo como imagem). A ocultação
    /// da mídia não tem quadro lido onde entrar e o motor não a aplica: recusada, nunca ignorada (D17).
    fn imagem_existe(&mut self, id: &'a str, onde: &Onde) -> bool {
        if self.midias.get(id).is_some_and(|m| m.ocultacoes.iter().any(|o| o.base.ligado)) {
            self.add(NAO_SUPORTADO, &onde.com_midia(id), None, Some("ocultar_imagem"), None);
        }
        if self.midias.get(id).is_some_and(|m| m.tipo != TipoMidia::Imagem) {
            self.por(ITEM_INVALIDO, &onde.com_midia(id).campo("midia"));
        }
        self.midia_existe(id, onde)
    }

    fn tabela(&self, midia: &str) -> Option<&'a TabelaQuadros> {
        self.ctx.fontes.get(midia).map(|f| &*f.quadros).filter(|t| !t.pts.is_empty())
    }

    fn clipe(&mut self, c: &'a ClipePrincipal, onde: Onde) {
        let fps = self.fps;
        match c {
            ClipePrincipal::Trecho(t) => {
                let existe = self.midia_existe(&t.midia, &onde);
                self.como_video.insert(&t.midia);
                self.velocidade(&t.velocidade, Some((t.inicio_s, t.fim_s)), &onde);
                self.audio_do_clipe(&t.audio, &onde);
                let tab = if existe { self.tabela(&t.midia) } else { None };
                let lido = match tab {
                    // a tabela do mapa do clipe (a de campos no desentrelace por campo): a de `tempo::medir` e da proveniência
                    Some(tab) => self.lido_do_trecho(t, tab, &realce::tabela(&t.base.efeitos, tab), &onde.com_midia(&t.midia)),
                    None => {
                        let fonte_sem_quadros = existe && self.ctx.fontes.contains_key(t.midia.as_str());
                        if fonte_sem_quadros || !(t.inicio_s.is_finite() && t.fim_s.is_finite() && t.fim_s > t.inicio_s) {
                            self.por(INTERVALO, &onde);
                        }
                        None
                    }
                };
                let quadros = lido.as_ref().map(|l| l.fonte);
                let origem = self.ctx.fontes.get(t.midia.as_str()).map(|f| TamanhoPx { w: f.exibido.0, h: f.exibido.1 });
                let uso = Uso { onde, midia: Some(&t.midia), base: Base::Fonte, efeitos: &t.base.efeitos, tab, quadros, n_q: self.n_q_de(&t.base.id), fps, origem };
                self.efeitos(&uso);
                self.da_fonte(&uso);
                if let (Some(tab), Some((a, b))) = (tab, quadros) {
                    self.estabilizar_na_analise(&uso, tab, a, b, t.inicio_s, t.fim_s);
                }
                // enquadramento animado: o destino em cada slot, pelo quadro que o slot mostra
                let animado = enquadramento::animado(&t.base.efeitos);
                if let (true, Some(l), Some(f)) = (animado, &lido, self.ctx.fontes.get(t.midia.as_str())) {
                    self.destino_do_enquadramento(&uso, TamanhoPx { w: f.exibido.0, h: f.exibido.1 }, &l.tempos);
                }
                self.ocultacoes_da_midia(&uso);
            }
            ClipePrincipal::Congelar(g) => {
                let existe = self.midia_existe(&g.midia, &onde);
                self.como_video.insert(&g.midia);
                self.duracao(g.duracao_s, &onde);
                let tab = if existe { self.tabela(&g.midia) } else { None };
                let quadros = tab.and_then(|tab| {
                    let fora = g.instante_pts.and_then(|p| tab.indice_do_pts(p)).is_none()
                        && !(g.instante_s.is_finite() && g.instante_s >= tab.tempo(0) - EPS && g.instante_s < tab.fim_s());
                    if fora {
                        self.por(INTERVALO, &onde.campo("instante_s"));
                        return None;
                    }
                    let j = tab.do_par(g.instante_s, g.instante_pts);
                    Some((j, j + 1))
                });
                let n_q = self.slots_locais(Some(g.duracao_s), self.n_q_de(&g.base.id));
                let origem = self.ctx.fontes.get(g.midia.as_str()).map(|f| TamanhoPx { w: f.exibido.0, h: f.exibido.1 });
                let uso = Uso { onde, midia: Some(&g.midia), base: Base::Congelado, efeitos: &g.base.efeitos, tab, quadros, n_q, fps, origem };
                self.efeitos(&uso);
                self.da_fonte(&uso);
                if let Some(f) = self.ctx.fontes.get(g.midia.as_str()).filter(|_| enquadramento::animado(&g.base.efeitos)) {
                    self.destino_do_enquadramento(&uso, TamanhoPx { w: f.exibido.0, h: f.exibido.1 }, &self.tempos_locais(n_q));
                }
                self.ocultacoes_da_midia(&uso);
            }
            ClipePrincipal::Imagem(i) => {
                self.imagem_existe(&i.midia, &onde);
                self.duracao(i.duracao_s, &onde);
                let mut uso = self.uso_local(onde, &i.base.efeitos, Some(i.duracao_s), self.n_q_de(&i.base.id));
                // a origem do clipe de imagem é a foto em lados pares (a do motor)
                uso.origem = motor_clipe::tamanho_da_imagem(self.p, self.ctx, &i.midia).map(|t| TamanhoPx { w: (t.w / 2 * 2).max(2), h: (t.h / 2 * 2).max(2) });
                self.efeitos(&uso);
                self.sem_fonte(&uso);
                self.ampliacao(&uso, &i.midia);
                if let Some(origem) = uso.origem.filter(|_| enquadramento::animado(&i.base.efeitos)) {
                    self.destino_do_enquadramento(&uso, origem, &self.tempos_locais(uso.n_q));
                }
            }
            ClipePrincipal::Titulo(t) => {
                self.duracao(t.duracao_s, &onde);
                self.titulo(&t.titulo, &onde);
                let mut uso = self.uso_local(onde, &t.base.efeitos, Some(t.duracao_s), self.n_q_de(&t.base.id));
                let canvas = TamanhoPx { w: self.p.saida.largura, h: self.p.saida.altura };
                uso.origem = Some(canvas);
                self.efeitos(&uso);
                self.sem_fonte(&uso);
                if enquadramento::animado(&t.base.efeitos) {
                    self.destino_do_enquadramento(&uso, canvas, &self.tempos_locais(uso.n_q));
                }
            }
            ClipePrincipal::Tela(t) => {
                // Fase 3: recusada antes do render; o resto só pela estrutura
                self.add(NAO_SUPORTADO, &onde, None, Some("tela"), None);
                self.velocidade(&t.velocidade, Some((t.inicio_s, t.fim_s)), &onde);
                self.audio_do_clipe(&t.audio, &onde);
                self.so_desconhecidos(&t.base.efeitos, &onde);
                for cel in &t.celulas {
                    self.midia_existe(&cel.midia, &onde);
                    self.so_desconhecidos(&cel.efeitos, &onde);
                }
                self.sincronias_da_tela(t, &onde);
            }
        }
    }

    /// `dur_s` = duração declarada; None = até o fim da âncora ou do programa (sobreposto).
    fn uso_local(&self, onde: Onde, efeitos: &'a [Tolerante<Efeito>], dur_s: Option<f64>, n_q: u32) -> Uso<'a> {
        Uso { onde, midia: None, base: Base::Local, efeitos, tab: None, quadros: None, n_q: self.slots_locais(dur_s, n_q), fps: self.fps, origem: None }
    }

    /// Slots a percorrer num clipe de base local: duração fora de faixa já é `duracao` e não percorre bilhões de slots.
    fn slots_locais(&self, dur_s: Option<f64>, n_q: u32) -> u32 {
        match dur_s {
            Some(d) if !(d.is_finite() && d <= DURACAO_MAX_S) => 0,
            _ if !self.fps_ok() => 0,
            _ => n_q.min(tempo::MAX_SLOTS),
        }
    }

    /// Instantes locais dos `n` slots.
    fn tempos_locais(&self, n: u32) -> Vec<f64> {
        (0..n as u64).map(|k| tempo::instante_do_slot(k, self.fps)).collect()
    }

    /// Enquadramento animado em `conter` cujo destino muda entre os slots: o motor recusa (a regra é a dele,
    /// `enquadramento::destino_variavel`, no canvas da saída).
    fn destino_do_enquadramento(&mut self, uso: &Uso<'a>, origem: TamanhoPx, tempos: &[f64]) {
        let s = &self.p.saida;
        if origem.w == 0 || origem.h == 0 || s.largura == 0 || s.altura == 0 || !estrutura_do_enquadre_ok(uso.efeitos) {
            return;
        }
        if enquadramento::destino_variavel(uso.efeitos, origem, TamanhoPx { w: s.largura, h: s.altura }, tempos) {
            let id = primeiro_enquadramento(uso.efeitos).map(|q| q.base.id.as_str()).unwrap_or("");
            self.add(NAO_SUPORTADO, &uso.onde.efeito(id).campo("regiao"), None, Some("enquadramento_variavel"), None);
        }
    }

    /// Clipe sem fonte (foto, título): `ocultar` não tem quadro lido onde entrar e o motor recusa.
    fn sem_fonte(&mut self, uso: &Uso<'a>) {
        for e in uso.efeitos {
            if let Tolerante::Conhecido(Efeito::Ocultar(o)) = e {
                if o.base.ligado {
                    self.add(NAO_SUPORTADO, &uso.onde.efeito(&o.base.id), None, Some("ocultar_sem_fonte"), None);
                }
            }
        }
    }

    fn duracao(&mut self, d: f64, onde: &Onde) {
        if !d.is_finite() || d > DURACAO_MAX_S || (self.fps_ok() && tempo::slots(d, self.fps) < 1) {
            self.por(DURACAO, &onde.campo("duracao_s"));
        }
    }

    /// Fator em 0,05..16 com até 3 casas; rampa (Fase 3) recusada e conferida: pontos > 0 dentro do trecho.
    fn velocidade(&mut self, v: &Velocidade, trecho: Option<(f64, f64)>, onde: &Onde) {
        let o = onde.campo("velocidade");
        match v {
            Velocidade::Constante { fator } => {
                if tempo::fator_racional(*fator).is_err() {
                    self.por(VELOCIDADE, &o);
                }
            }
            Velocidade::Rampa { pontos } => {
                self.add(NAO_SUPORTADO, &o, None, Some("rampa"), None);
                let (a, b) = trecho.unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
                let ruim = pontos.is_empty()
                    || !chaves_em_ordem(pontos)
                    || pontos.iter().any(|k| !(k.v.is_finite() && k.v > 0.0) || k.t < a - EPS || k.t > b + EPS);
                if ruim {
                    self.por(VELOCIDADE, &o);
                }
            }
        }
    }

    fn audio_do_clipe(&mut self, a: &super::modelo::AudioClipe, onde: &Onde) {
        self.volume(&a.volume, &onde.campo("audio"));
        for s in &a.silencios {
            if !(s.de.is_finite() && s.ate.is_finite() && s.ate > s.de) {
                self.por(AUDIO, &onde.campo("silencios"));
            }
        }
    }

    /// Volume 0..2: estrutura antes dos valores.
    fn volume(&mut self, v: &Animavel<f64>, onde: &Onde) {
        if !animavel_ok(v) {
            self.por(ITEM_INVALIDO, onde);
            return;
        }
        let fora = |x: f64| !(0.0..=2.0).contains(&x);
        let ruim = match v {
            Animavel::Fixo(x) => fora(*x),
            Animavel::Chaves(ks) => ks.iter().any(|k| fora(k.v)),
        };
        if ruim {
            self.por(AUDIO, onde);
        }
    }

    /// O trecho medido na tabela do CLIPE (`realce::tabela`: a da fonte, ou uma entrada por campo no desentrelace por
    /// campo), a mesma do mapa do motor: slots, repetidos, descartados e passos saem dela. O que vale por quadro LIDO
    /// (ocultação, rastreio, faixa da estabilização) fica nos quadros da fonte de onde as entradas do clipe saem.
    fn lido_do_trecho(&mut self, t: &ClipeTrecho, tab: &TabelaQuadros, tab_clipe: &TabelaQuadros, onde: &Onde) -> Option<Lido> {
        let ((a, b), mapa) = self.trecho_na_tabela(tab_clipe, t.inicio_s, t.inicio_pts, t.fim_s, t.fim_pts, &t.velocidade, onde)?;
        let fonte = if std::ptr::eq(tab, tab_clipe) || (tab.pts.len() == tab_clipe.pts.len() && tab.tb == tab_clipe.tb) {
            (a, b)
        } else {
            // quadro da fonte exibido no instante da primeira e da última entrada do clipe
            (tab.do_par(tab_clipe.tempo(a), None), tab.do_par(tab_clipe.tempo(b - 1), None) + 1)
        };
        Some(Lido { fonte, tempos: mapa.iter().map(|&j| tab_clipe.tempo(j as usize)).collect() })
    }

    /// Faixa e mapa do trecho na tabela de quadros; devolve os quadros lidos e o quadro de cada slot quando o mapa fecha.
    #[allow(clippy::too_many_arguments)]
    fn trecho_na_tabela(
        &mut self,
        tab: &TabelaQuadros,
        inicio_s: f64,
        inicio_pts: Option<i64>,
        fim_s: f64,
        fim_pts: Option<i64>,
        v: &Velocidade,
        onde: &Onde,
    ) -> Option<((usize, usize), Vec<u32>)> {
        let sem = Onde { midia: None, ..onde.clone() };
        let inicio_na_tabela = inicio_pts.and_then(|p| tab.indice_do_pts(p)).is_some();
        let fim_na_tabela = fim_pts.and_then(|p| tab.indice_do_pts(p)).is_some();
        // fora da mídia: começa depois do último quadro ou termina antes do primeiro
        let fora = (!inicio_na_tabela && !(inicio_s.is_finite() && inicio_s < tab.fim_s()))
            || (!fim_na_tabela && !(fim_s.is_finite() && fim_s > tab.tempo(0)));
        let (a, b) = match tempo::faixa_do_trecho(tab, inicio_s, inicio_pts, fim_s, fim_pts) {
            Ok(f) if !fora => f,
            _ => {
                self.por(INTERVALO, &sem);
                return None;
            }
        };
        if !self.fps_ok() {
            return None;
        }
        match tempo::mapa_trecho(tab, a, b, v, self.fps) {
            Ok(m) => {
                // menos de 1 slot: o trecho todo dura menos que um quadro da saída
                let pfim = if b < tab.pts.len() { tab.pts[b] as i128 * m.r as i128 } else { (*tab.pts.last()? as i128 + tab.passo_mediano.max(1) as i128) * m.r as i128 };
                if (pfim - m.p0 as i128) * (m.kn as i128) < m.kd as i128 {
                    self.por(INTERVALO, &sem);
                }
                let fator = match v {
                    Velocidade::Constante { fator } => *fator,
                    Velocidade::Rampa { .. } => 1.0,
                };
                if fator <= 1.0 && m.descartados > 0 {
                    self.por_n(QUADROS_DESCARTADOS, onde, m.descartados as u64);
                }
                let taxa = tab.taxa_nominal();
                if taxa.num as i128 * self.fps.den as i128 != self.fps.num as i128 * taxa.den as i128 {
                    let n = (fator == 1.0).then_some(m.repetidos as u64 + m.descartados as u64);
                    self.add(FPS_DIFERENTE, onde, n, None, None);
                }
                if let Some(n) = taxa_variavel(tab, a, b) {
                    self.por_n(TAXA_VARIAVEL, onde, n);
                }
                Some(((a, b), m.quadro_do_slot))
            }
            Err(e) => {
                let texto = e.to_string();
                let codigo = if texto.contains(MAPA_GRANDE) {
                    MAPA_GRANDE
                } else if texto.contains(NAO_SUPORTADO) || texto.contains(VELOCIDADE) {
                    // rampa e fator já acusados em `velocidade`
                    return None;
                } else {
                    INTERVALO
                };
                self.por(codigo, &sem);
                None
            }
        }
    }

    /// Avisos e periciais que dependem da fonte mostrada: entrelaçado, ampliação, média de quadros.
    fn da_fonte(&mut self, uso: &Uso<'a>) {
        let Some(midia) = uso.midia else { return };
        if let Some(f) = self.ctx.fontes.get(midia) {
            let desentrelaca = uso.efeitos.iter().any(|e| matches!(e, Tolerante::Conhecido(Efeito::Desentrelacar(d)) if d.base.ligado));
            if f.entrelacado && !desentrelaca {
                self.por(ENTRELACADO, &uso.onde.com_midia(midia));
            }
        }
        self.ampliacao(uso, midia);
    }

    /// Ampliação em px da origem para o canvas, pior instante entre as chaves do enquadramento.
    fn ampliacao(&mut self, uso: &Uso<'a>, midia: &str) {
        let Some(f) = self.ctx.fontes.get(midia) else { return };
        let (w, h) = f.exibido;
        let p = self.p;
        let s = &p.saida;
        if w == 0 || h == 0 || s.largura == 0 || s.altura == 0 || !estrutura_do_enquadre_ok(uso.efeitos) {
            return;
        }
        let mut taus = vec![0.0];
        if let Some(Animavel::Chaves(ks)) = primeiro_enquadramento(uso.efeitos).map(|q| &q.regiao) {
            taus = ks.iter().map(|k| k.t).collect();
        }
        let origem = TamanhoPx { w, h };
        let canvas = TamanhoPx { w: s.largura, h: s.altura };
        let area = (canvas.w as f64 * canvas.h as f64) / (origem.w as f64 * origem.h as f64);
        let maior = taus
            .iter()
            .map(|&tau| {
                let a = geometria::a_de(uso.efeitos, tau, origem, canvas).0;
                ((a[0] * a[3] - a[1] * a[2]).abs() * area).sqrt()
            })
            .fold(0.0, f64::max);
        if maior > AMPLIACAO_MAX + EPS {
            self.por(AMPLIACAO_ALTA, &uso.onde.com_midia(midia));
        }
    }

    /// O trecho fica dentro da faixa analisada de cada `estabilizar` ligado.
    fn estabilizar_na_analise(&mut self, uso: &Uso<'a>, tab: &TabelaQuadros, a: usize, b: usize, inicio_s: f64, fim_s: f64) {
        for e in uso.efeitos {
            let Tolerante::Conhecido(Efeito::Estabilizar(s)) = e else { continue };
            if !s.base.ligado {
                continue;
            }
            let an = &s.analise;
            let fora = if tab.pts.is_empty() {
                inicio_s < an.de_s - EPS || fim_s > an.ate_s + EPS
            } else {
                let (de, ate) = (tab.do_par(an.de_s, Some(an.de_pts)), tab.do_par(an.ate_s, Some(an.ate_pts)));
                a < de || b - 1 > ate
            };
            if fora {
                self.por(ESTABILIZAR_FORA_DA_ANALISE, &uso.onde.efeito(&s.base.id));
            }
        }
    }

    // ---- efeitos

    fn efeitos(&mut self, uso: &Uso<'a>) {
        let mut enquadramentos = 0;
        // o que a cadeia de origem recusa com `item_invalido` (a regra é do `realce`), só onde ela roda
        if uso.base != Base::Local {
            let exibido = uso.origem.map_or((0, 0), |o| (o.w, o.h));
            for (id, campo) in realce::invalidos(uso.efeitos, exibido) {
                // sem a fonte, a conversão da lente não tem tamanho: a mídia ausente já é acusada
                if campo == Some("k1") && uso.origem.is_none() {
                    continue;
                }
                let onde = uso.onde.efeito(&id);
                self.por(ITEM_INVALIDO, &campo.map_or(onde.clone(), |c| onde.campo(c)));
            }
        }
        // SHA-256 da mídia que o clipe mostra (medidas das análises só valem nela)
        let sha_do_clipe = uso.midia.and_then(|m| self.midias.get(m)).map(|m| m.sha256.as_str());
        for e in uso.efeitos {
            let e = match e {
                Tolerante::Desconhecido(v) => {
                    let valendo = efeito_desconhecido_ligado(v);
                    let o = efeito_onde(&uso.onde, v);
                    self.desconhecido::<Efeito>(v, TIPOS_EFEITO, valendo, &o);
                    continue;
                }
                Tolerante::Conhecido(e) => e,
            };
            let base = base_efeito(e);
            if !base.ligado {
                continue;
            }
            let onde = uso.onde.efeito(&base.id);
            if let Some(i) = &base.ativo {
                let finito = i.de.is_none_or(f64::is_finite) && i.ate.is_none_or(f64::is_finite);
                if !finito || matches!((i.de, i.ate), (Some(de), Some(ate)) if ate < de) {
                    self.por(INTERVALO, &onde.campo("ativo"));
                }
            }
            for filtro in filtros_de(e, uso.efeitos) {
                if !self.ctx.cap.filtros.contains(filtro) {
                    self.add(FILTRO_AUSENTE, &onde, None, Some(filtro), None);
                }
            }
            // efeitos de origem que o motor ainda não aplica (P1.4); o rótulo é o tipo
            if let Some(d) = realce::recusa(e, uso.base != Base::Local) {
                self.add(NAO_SUPORTADO, &onde, None, Some(d), None);
            }
            // por campo num trecho de fonte sem meio entre os quadros (o motor recusa em `realce::leitura`)
            if let (Base::Fonte, Some(tab)) = (uso.base, uso.tab) {
                if let Some(d) = realce::recusa_na_fonte(e, tab) {
                    self.add(NAO_SUPORTADO, &onde, None, Some(d), None);
                }
            }
            match e {
                Efeito::Enquadramento(q) => {
                    enquadramentos += 1;
                    if enquadramentos > 1 {
                        // vale o primeiro ligado (contratos (f) 6)
                        self.por(ITEM_INVALIDO, &onde);
                        continue;
                    }
                    self.regiao_do_enquadramento(&q.regiao, q.interpolacao, &onde);
                }
                Efeito::Girar(g) => {
                    if g.graus % 90 != 0 || g.graus >= 360 {
                        self.por(ITEM_INVALIDO, &onde.campo("graus"));
                    }
                }
                Efeito::MediaTemporal(m) => {
                    // trecho e quadro congelado (média centrada nele): a imagem mostrada é média de várias da gravação
                    if uso.base != Base::Local {
                        self.por(MEDIA_DE_QUADROS, &onde);
                    }
                    if !MEDIA_QUADROS.contains(&m.quadros) {
                        self.por(ITEM_INVALIDO, &onde.campo("quadros"));
                    }
                }
                Efeito::Ocultar(oc) => {
                    self.ocultar_estatico(oc, &onde);
                    if uso.base == Base::Congelado && ocultar_do_congelado(oc, uso).is_none() {
                        self.add(NAO_SUPORTADO, &onde.campo("ativo"), None, Some("ocultar_congelado"), None);
                    }
                    if let Some(r) = self.rastreio_do_uso(&oc.alvo.rastreio, uso, &onde, true) {
                        self.ocultar_no_uso(oc, r, uso, &onde, false);
                    }
                }
                Efeito::Holofote(h) => {
                    self.alvo_regiao(&h.alvo, &onde);
                    if h.alvo.forma == FormaRegiao::Silhueta {
                        self.add(NAO_SUPORTADO, &onde, None, Some("silhueta"), None);
                    }
                    if let Some(r) = self.rastreio_do_uso(&h.alvo.rastreio, uso, &onde, false) {
                        self.tamanho_do_rastreio(r, h.alvo.forma != FormaRegiao::Silhueta, &onde);
                        self.confirmado(r, uso, base, &onde);
                    }
                }
                Efeito::Destaque(d) => {
                    self.cor(&d.traco.cor, &onde.campo("traco"));
                    if let Some(r) = &d.rotulo {
                        self.estilo(&r.estilo, &onde);
                        self.texto_longo(&r.texto, &onde);
                    }
                    if let Some(r) = self.rastreio_do_uso(&d.rastreio, uso, &onde, false) {
                        let exige = matches!(d.forma, FormaDestaque::Retangulo | FormaDestaque::Elipse | FormaDestaque::Colchetes);
                        self.tamanho_do_rastreio(r, exige, &onde);
                        self.confirmado(r, uso, base, &onde);
                    }
                }
                Efeito::Lupa(l) => {
                    if let Some(d) = lupa::recusa(l) {
                        self.add(NAO_SUPORTADO, &onde, None, Some(d), None);
                    }
                    self.cor(&l.moldura.cor, &onde.campo("moldura"));
                    // o que o motor recusa com `item_invalido` (a regra é da lupa); sem o tamanho da origem, só os campos
                    let campo = match uso.origem {
                        Some(o) => lupa::campo_invalido(l, o),
                        None => camada::lupa::campo_invalido(l),
                    };
                    if let Some(c) = campo {
                        self.por(ITEM_INVALIDO, &onde.campo(c));
                    }
                    if let Some(r) = self.rastreio_do_uso(&l.rastreio, uso, &onde, false) {
                        self.confirmado(r, uso, base, &onde);
                    }
                }
                Efeito::Regua(rg) => {
                    if let Some(d) = medidas::recusa(e) {
                        self.add(NAO_SUPORTADO, &onde, None, Some(d), None);
                    }
                    self.cor(&rg.traco.cor, &onde.campo("traco"));
                    let geometrico = uso.efeitos.iter().any(|x| {
                        matches!(x, Tolerante::Conhecido(Efeito::Estabilizar(s)) if s.base.ligado)
                            || matches!(x, Tolerante::Conhecido(Efeito::Lente(l)) if l.base.ligado)
                    });
                    if geometrico {
                        self.por(REGUA_ESPACO, &onde);
                    }
                    // a calibração é de uma mídia: fora dela (outra mídia, foto, título) a régua não mede nada
                    if medidas::de_outra_midia(rg.calibracao.midia_sha256.as_deref(), sha_do_clipe) {
                        self.por(REGUA_FORA_DA_CALIBRACAO, &onde.campo("calibracao"));
                    } else if medidas::recusa(e).is_none() {
                        // tudo o que o desenho recusa (âncora, ponta, modelos 1D, horizonte, campos): a regra é do P1.3
                        if let Some((cod, campo)) = medidas::problema_da_regua(rg) {
                            self.por(cod, &onde.campo(campo));
                        }
                    }
                }
                Efeito::VelocidadeExibida(v) => {
                    if let Some(d) = medidas::recusa(e) {
                        self.add(NAO_SUPORTADO, &onde, None, Some(d), None);
                    }
                    // só dentro do intervalo medido (O5; a regra é do P1.3), no tempo de fonte do quadro mostrado
                    if self.em_algum_quadro_mostrado(uso, base, &|t_fonte| medidas::fora_do_intervalo(v, t_fonte)) {
                        self.por(VELOCIDADE_FORA_DO_INTERVALO, &onde);
                    }
                    // medida em outra mídia: o intervalo medido não é desta gravação
                    if sha_do_clipe.is_some() && medidas::de_outra_midia(v.calculo.midia_sha256.as_deref(), sha_do_clipe) {
                        self.por(VELOCIDADE_FORA_DO_INTERVALO, &onde.campo("calculo"));
                    }
                    self.estilo(&v.estilo, &onde);
                    if let OndeVelocidade::Segue { rastreio, .. } = &v.onde {
                        if let Some(r) = self.rastreio_do_uso(rastreio, uso, &onde, false) {
                            self.confirmado(r, uso, base, &onde);
                        }
                    }
                }
                Efeito::Texto(t) => {
                    self.estilo(&t.estilo, &onde);
                    self.texto_longo(&t.conteudo, &onde);
                    if !(t.largura_max.is_finite() && t.largura_max > 0.0) {
                        self.por(ITEM_INVALIDO, &onde.campo("largura_max"));
                    }
                    if self.lugar_valido(&t.lugar, &onde) {
                        self.lugar(&t.lugar, t.estilo.tamanho, &onde);
                    }
                }
                Efeito::Carimbo(c) => {
                    self.estilo(&c.estilo, &onde);
                    if self.lugar_valido(&c.lugar, &onde) {
                        self.lugar(&c.lugar, c.estilo.tamanho, &onde);
                    }
                    if c.mostra == MostraCarimbo::Relogio {
                        match &c.relogio {
                            None => self.por(ITEM_INVALIDO, &onde.campo("relogio")),
                            Some(r) => {
                                let mut longe = false;
                                self.cada_ativo(uso, base, |a| longe |= (a.t - r.media_time_s).abs() > RELOGIO_MAX_S);
                                if longe {
                                    self.por(RELOGIO_DISTANTE, &onde);
                                }
                            }
                        }
                    }
                }
                Efeito::Imagem(i) => {
                    self.imagem_existe(&i.midia, &onde);
                    self.lugar_valido(&i.lugar, &onde);
                }
                Efeito::Estabilizar(_) | Efeito::Lente(_) | Efeito::Desentrelacar(_) | Efeito::Ajustes(_) | Efeito::Nitidez(_) => {}
            }
        }
    }

    fn regiao_do_enquadramento(&mut self, r: &Animavel<Caixa>, interp: Interpolacao, onde: &Onde) {
        let o = onde.campo("regiao");
        if !animavel_ok(r) {
            self.por(REGIAO, &o);
            return;
        }
        let ruim = |c: &Caixa| !caixa_finita(c) || c.w < REGIAO_MIN_W || c.h <= 0.0;
        let (caixas_ruins, animada) = match r {
            Animavel::Fixo(c) => (ruim(c), false),
            Animavel::Chaves(ks) => (ks.iter().any(|k| ruim(&k.v)), ks.len() > 1),
        };
        if caixas_ruins || (animada && interp == Interpolacao::Vizinho) {
            self.por(REGIAO, &o);
        }
    }

    fn estilo(&mut self, e: &EstiloTexto, onde: &Onde) {
        let o = onde.campo("estilo");
        self.cor(&e.cor, &o);
        if let Some(c) = &e.caixa {
            self.cor(&c.cor, &o);
        }
        if let Some(c) = &e.contorno {
            self.cor(&c.cor, &o);
        }
    }

    fn texto_longo(&mut self, texto: &str, onde: &Onde) {
        if texto.chars().count() > LIMITE_TEXTO {
            self.por(ITEM_INVALIDO, &onde.campo("conteudo"));
        }
    }

    /// Ponto de apoio finito e a até 2^30 px do canvas da saída (o limite do rasterizador; mais longe a conta em
    /// px estoura): senão `item_invalido` no `lugar`.
    fn lugar_valido(&mut self, l: &Lugar, onde: &Onde) -> bool {
        let s = &self.p.saida;
        let longe = |v: f64, lado: u32| !v.is_finite() || (v * lado.max(1) as f64).abs() > MAX_LUGAR_PX;
        let ok = !longe(l.x, s.largura) && !longe(l.y, s.altura);
        if !ok {
            self.por(ITEM_INVALIDO, &onde.campo("lugar"));
        }
        ok
    }

    /// Ponto de apoio fora da área segura, ou uma linha do texto já passando dela na vertical.
    fn lugar(&mut self, l: &Lugar, tamanho: f64, onde: &Onde) {
        let dentro = |v: f64| (AREA_SEGURA - EPS..=1.0 - AREA_SEGURA + EPS).contains(&v);
        let t = if tamanho.is_finite() { tamanho.max(0.0) } else { 0.0 };
        let (cima, baixo) = match l.v {
            AlinhamentoV::Topo => (l.y, l.y + t),
            AlinhamentoV::Meio => (l.y - t / 2.0, l.y + t / 2.0),
            AlinhamentoV::Base => (l.y - t, l.y),
        };
        if !(dentro(l.x) && dentro(cima) && dentro(baixo)) {
            self.por(TEXTO_FORA, &onde.campo("lugar"));
        }
    }

    fn titulo(&mut self, t: &'a Titulo, onde: &Onde) {
        match &t.fundo {
            Fundo::Cor { cor } => self.cor(cor, &onde.campo("fundo")),
            Fundo::Imagem { midia } => {
                self.imagem_existe(midia, onde);
            }
            Fundo::Transparente => {}
        }
        for (i, b) in t.blocos.iter().enumerate() {
            self.estilo(&b.estilo, onde);
            self.texto_longo(&b.conteudo, onde);
            if !(b.largura_max.is_finite() && b.largura_max > 0.0) {
                self.por(ITEM_INVALIDO, &onde.campo("largura_max"));
            }
            if self.lugar_do_bloco(t, i, onde) {
                self.lugar(&b.lugar, b.estilo.tamanho, onde);
            }
        }
    }

    /// Lugar de um bloco de título. O rasterizador (`camada::titulo`) recusa pelo CANTO do sprite, a até um sprite
    /// do ponto de apoio: longe do limite a conta sai do apoio; na faixa de dúvida (só valor absurdo, de arquivo
    /// editado à mão) pergunta-se ao próprio rasterizador, e a exportação não descobre o erro no meio.
    fn lugar_do_bloco(&mut self, t: &Titulo, i: usize, onde: &Onde) -> bool {
        let b = &t.blocos[i];
        let s = &self.p.saida;
        let canvas = TamanhoPx { w: s.largura.max(1), h: s.altura.max(1) };
        let (x, y) = (b.lugar.x * canvas.w as f64, b.lugar.y * canvas.h as f64);
        let longe = x.abs().max(y.abs());
        let ok = if !(x.is_finite() && y.is_finite()) || longe > MAX_LUGAR_PX + MAX_SPRITE_PX {
            false
        } else if longe <= MAX_LUGAR_PX - MAX_SPRITE_PX {
            true
        } else if b.conteudo.is_empty() {
            // bloco vazio não é desenhado: vale o ponto de apoio
            longe <= MAX_LUGAR_PX
        } else {
            canto_do_bloco_cabe(t, i, canvas)
        };
        if !ok {
            self.por(ITEM_INVALIDO, &onde.campo("lugar"));
        }
        ok
    }

    /// Alvo de região: margem não negativa no `ocultar`, polígono com 3 vértices finitos.
    fn alvo_regiao(&mut self, a: &super::modelo::AlvoRegiao, onde: &Onde) {
        let o = onde.campo("alvo");
        if !(a.margem.is_finite() && a.borda_suave.is_finite()) {
            self.por(REGIAO, &o);
        } else if !(0.0..=BORDA_SUAVE_MAX).contains(&a.borda_suave) {
            self.por(ITEM_INVALIDO, &onde.campo("borda_suave"));
        }
        if a.forma == FormaRegiao::Poligono {
            let ok = a.vertices.as_ref().is_some_and(|v| v.len() >= 3 && v.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
            if !ok {
                self.por(REGIAO, &o);
            }
        }
    }

    // ---- ocultar

    /// Regras que não dependem do clipe: cor, região, força da ocultação.
    fn ocultar_estatico(&mut self, oc: &EfOcultar, onde: &Onde) {
        self.alvo_regiao(&oc.alvo, onde);
        if oc.alvo.margem < 0.0 {
            self.por(REGIAO, &onde.campo("alvo"));
        }
        match cor::ler(&oc.cor) {
            Err(_) => self.por(COR, &onde.campo("cor")),
            Ok(c) if oc.modo == ModoOcultar::Solido && c.a != 255 => self.por(OCULTAR_COR_OPACA, &onde.campo("cor")),
            Ok(_) => {}
        }
        let texto = matches!(oc.conteudo, ConteudoOculto::Placa | ConteudoOculto::Documento | ConteudoOculto::Tela);
        if oc.modo != ModoOcultar::Solido && texto {
            self.por(OCULTACAO_REVERSIVEL, onde);
        }
        // o motor prende a intensidade em 0..1: valor fora disso não sai como foi gravado
        if !(0.0..=1.0).contains(&oc.intensidade) {
            self.por(ITEM_INVALIDO, &onde.campo("intensidade"));
        }
        let i = oc.intensidade.clamp(0.0, 1.0);
        let fraca = match oc.modo {
            ModoOcultar::Desfoque => oc.intensidade < DESFOQUE_MIN,
            ModoOcultar::Mosaico => (12.0 - 11.0 * i).round() > MOSAICO_MAX_CELULAS,
            ModoOcultar::Solido => false,
        };
        if fraca {
            self.por(OCULTACAO_FRACA, onde);
        }
        if oc.alvo.forma == FormaRegiao::Silhueta {
            self.por(SILHUETA_EM_OCULTAR, onde);
            self.add(NAO_SUPORTADO, onde, None, Some("silhueta"), None);
        }
    }

    /// Regras que dependem dos quadros usados: tamanho da caixa, confirmação, cobertura.
    fn ocultar_no_uso(&mut self, oc: &EfOcultar, r: &Rastreio, uso: &Uso<'a>, onde: &Onde, da_midia: bool) {
        let silhueta = oc.alvo.forma == FormaRegiao::Silhueta;
        self.tamanho_do_rastreio(r, !silhueta, onde);
        let denso = self.denso(r);
        let (mut nada, mut interpolado, mut incerto) = (0u64, 0u64, 0u64);
        let mut conta = |a: Amostra| {
            match rastreio_avaliar::avaliar(r, denso.as_deref(), Instante { t: a.t, pts: a.pts }) {
                None if !rastreio_avaliar::em_ausencia(r, a.t) => nada += 1,
                None => {}
                Some(x) => {
                    interpolado += u64::from(x.como == Como::Interpolado);
                    incerto += u64::from(x.incerto);
                }
            }
        };
        let b = &oc.base;
        match (da_midia, uso.base) {
            (true, _) => uso.cada_na_fonte(|a| {
                if ativo_em(b, &a) {
                    conta(a)
                }
            }),
            // congelado: a ocultação do clipe vale por quadro lido (um só) e o `ativo` é local; vale no quadro só
            // quando pega todos os slots (`ocultar_do_congelado`; parte é recusada lá)
            (false, Base::Congelado) => {
                if ocultar_do_congelado(oc, uso) == Some(true) {
                    uso.cada_na_fonte(&mut conta)
                }
            }
            (false, _) => uso.cada(|a| {
                if ativo_em(b, &a) {
                    conta(a)
                }
            }),
        }
        if nada > 0 {
            self.por_n(SEM_COBERTURA, onde, nada);
        }
        if interpolado > 0 {
            self.por_n(OCULTAR_INTERPOLADO, onde, interpolado);
        }
        if incerto > 0 {
            self.por_n(OCULTAR_INCERTO, onde, incerto);
        }
        if da_midia {
            self.confirmado_na_fonte(r, uso, b, onde);
        } else {
            self.confirmado(r, uso, b, onde);
        }
    }

    /// Ocultações da mídia mostrada pelo clipe (valem em todo uso dela, em tempo de fonte).
    fn ocultacoes_da_midia(&mut self, uso: &Uso<'a>) {
        let Some(m) = uso.midia.and_then(|id| self.midias.get(id).copied()) else { return };
        for oc in m.ocultacoes.iter().filter(|o| o.base.ligado) {
            let estatico = Onde::midia(&m.id).efeito(&oc.base.id);
            self.ocultar_estatico(oc, &estatico);
            if let Some(i) = &oc.base.ativo {
                let finito = i.de.is_none_or(f64::is_finite) && i.ate.is_none_or(f64::is_finite);
                if !finito || matches!((i.de, i.ate), (Some(de), Some(ate)) if ate < de) {
                    self.por(INTERVALO, &estatico.campo("ativo"));
                }
            }
            let Some(r) = self.rastreio_da_midia(&oc.alvo.rastreio, m, &estatico) else { continue };
            let onde = uso.onde.com_midia(&m.id).efeito(&oc.base.id);
            self.ocultar_no_uso(oc, r, uso, &onde, true);
        }
    }

    // ---- rastreio

    /// Rastreio de um efeito do clipe: existe, é da mesma mídia, base e espaço certos, âncoras em ordem.
    /// None = não dá para avaliar (o problema já foi acusado).
    fn rastreio_do_uso(&mut self, id: &str, uso: &Uso<'a>, onde: &Onde, ocultar: bool) -> Option<&'a Rastreio> {
        let o = onde.campo("rastreio");
        let Some(r) = self.rastreios.get(id).copied() else {
            self.por(RASTREIO_AUSENTE, &o);
            return None;
        };
        match (uso.midia, r.base) {
            (Some(m), BaseRastreio::Fonte) => {
                if r.midia.as_deref() != Some(m) {
                    self.por(RASTREIO_AUSENTE, &o);
                    return None;
                }
                let sha = self.midias.get(m).map(|x| x.sha256.as_str()).unwrap_or("");
                let esperado = if ocultar { "original".to_string() } else { geometria::espaco_de_origem(sha, uso.efeitos) };
                if r.espaco != esperado {
                    self.por(RASTREIO_ESPACO, &o);
                    return None;
                }
            }
            (None, BaseRastreio::Local) => {}
            _ => {
                self.por(RASTREIO_BASE, &o);
                return None;
            }
        }
        self.ancoras_ok(r, &o).then_some(r)
    }

    /// Rastreio de uma ocultação da mídia: de fonte, da própria mídia, no espaço original.
    fn rastreio_da_midia(&mut self, id: &str, m: &MidiaRef, onde: &Onde) -> Option<&'a Rastreio> {
        let o = onde.campo("rastreio");
        let Some(r) = self.rastreios.get(id).copied() else {
            self.por(RASTREIO_AUSENTE, &o);
            return None;
        };
        if r.base != BaseRastreio::Fonte {
            self.por(RASTREIO_BASE, &o);
            return None;
        }
        if r.midia.as_deref() != Some(m.id.as_str()) {
            self.por(RASTREIO_AUSENTE, &o);
            return None;
        }
        if r.espaco != "original" {
            self.por(RASTREIO_ESPACO, &o);
            return None;
        }
        self.ancoras_ok(r, &o).then_some(r)
    }

    /// Âncoras em ordem de tempo e com caixa finita (antes de avaliar a curva).
    fn ancoras_ok(&mut self, r: &Rastreio, onde: &Onde) -> bool {
        let ok = chaves_em_ordem(&r.ancoras) && r.ancoras.iter().all(|k| caixa_finita(&k.v));
        if !ok {
            self.por(REGIAO, onde);
        }
        ok
    }

    /// Forma que precisa de tamanho: rastreio de caixa com lados positivos.
    fn tamanho_do_rastreio(&mut self, r: &Rastreio, exige: bool, onde: &Onde) {
        if exige && (r.tipo == TipoRastreio::Ponto || r.ancoras.iter().any(|k| !(k.v.w > 0.0 && k.v.h > 0.0))) {
            self.por(REGIAO, &onde.campo("rastreio"));
        }
    }

    fn denso(&self, r: &Rastreio) -> Option<std::sync::Arc<super::rastreio::ArquivoDenso>> {
        let d = r.denso.as_ref()?;
        self.ctx.rastreios.denso(&r.id, &d.sha256).ok()
    }

    /// Amostras do clipe em que o efeito está ativo.
    fn cada_ativo(&self, uso: &Uso<'a>, base: &EfBase, mut f: impl FnMut(Amostra)) {
        uso.cada(|a| {
            if ativo_em(base, &a) {
                f(a)
            }
        });
    }

    /// `regra` vale em algum instante em que o efeito está ativo? Ela recebe o tempo de FONTE do quadro mostrado
    /// (trecho: o quadro lido; congelado: o quadro congelado, em todo slot ativo) ou None em clipe sem fonte (B17).
    fn em_algum_quadro_mostrado(&self, uso: &Uso<'a>, base: &EfBase, regra: &dyn Fn(Option<f64>) -> bool) -> bool {
        let mut achou = false;
        self.cada_ativo(uso, base, |a| achou |= regra(a.pts.map(|_| a.t)));
        achou
    }

    /// Detecção automática (ou Pessoas em `auto`) só vale com confirmação do mesmo conteúdo cobrindo o uso.
    fn confirmado(&mut self, r: &Rastreio, uso: &Uso<'a>, base: &EfBase, onde: &Onde) {
        let mut faixa: Option<(f64, f64)> = None;
        self.cada_ativo(uso, base, |a| faixa = Some(faixa.map_or((a.t, a.t), |(x, y)| (x.min(a.t), y.max(a.t)))));
        self.confirmacao(r, faixa, onde);
    }

    fn confirmado_na_fonte(&mut self, r: &Rastreio, uso: &Uso<'a>, base: &EfBase, onde: &Onde) {
        let mut faixa: Option<(f64, f64)> = None;
        uso.cada_na_fonte(|a| {
            if ativo_em(base, &a) {
                faixa = Some(faixa.map_or((a.t, a.t), |(x, y)| (x.min(a.t), y.max(a.t))));
            }
        });
        self.confirmacao(r, faixa, onde);
    }

    fn confirmacao(&mut self, r: &Rastreio, faixa: Option<(f64, f64)>, onde: &Onde) {
        let automatico = matches!(r.origem, OrigemRastreio::Deteccao { .. } | OrigemRastreio::Pessoas { estado: EstadoPessoa::Auto, .. });
        let Some((de, ate)) = faixa.filter(|_| automatico) else { return };
        let valida = r.confirmacao.as_ref().is_some_and(|c| {
            c.denso_sha256.as_deref() == r.denso.as_ref().map(|d| d.sha256.as_str()) && c.de_s <= de + EPS && c.ate_s >= ate - EPS
        });
        if !valida {
            self.por(RASTREIO_NAO_CONFIRMADO, &onde.campo("rastreio"));
        }
    }

    // ---- transições

    fn transicoes(&mut self) {
        let p = self.p;
        let clipes: Vec<&ClipePrincipal> = p
            .principal
            .iter()
            .filter_map(|c| match c {
                Tolerante::Conhecido(c) if !base_clipe(c).desligado => Some(c),
                _ => None,
            })
            .collect();
        fn entrada(c: &ClipePrincipal) -> Option<&super::modelo::Transicao> {
            base_clipe(c).transicao_entrada.as_ref()
        }
        for (i, c) in clipes.iter().enumerate() {
            let b = base_clipe(c);
            let saida = if i + 1 < clipes.len() { entrada(clipes[i + 1]) } else { p.fecho.as_ref() };
            let onde = Onde::clipe(&b.id).campo("transicao_entrada");
            let (mut ok, mut de_outra_fase) = (true, false);
            for t in [b.transicao_entrada.as_ref(), if i + 1 == clipes.len() { p.fecho.as_ref() } else { None }].into_iter().flatten() {
                if t.tipo.trim().is_empty() || !(t.duracao_s.is_finite() && t.duracao_s >= 0.0) {
                    ok = false;
                } else if !juncao::TRANSICOES.contains(&t.tipo.as_str()) {
                    // fora do catálogo do P1.1: o motor recusa
                    de_outra_fase = true;
                } else if self.com_som && p.saida.audio && tempo::transicao_sobrepoe(&t.tipo) && motor_audio::recusa("transicao") {
                    // o som da transição com sobreposição é do P1.5 (O3)
                    de_outra_fase = true;
                } else if juncao::longa_demais(t, self.fps) {
                    // acima do que o `xfade` aceita: o motor recusa (a regra é da junção)
                    ok = false;
                }
            }
            if !ok {
                self.por(TRANSICAO, &onde);
                continue;
            }
            if de_outra_fase {
                self.add(NAO_SUPORTADO, &onde, None, Some("transicao"), None);
                continue;
            }
            if !self.fps_ok() {
                continue;
            }
            let q = |t: Option<&super::modelo::Transicao>| t.map_or(0, |t| tempo::slots(t.duracao_s, self.fps));
            let (e, s) = (q(b.transicao_entrada.as_ref()), q(saida));
            let n = self.n_q_de(&b.id) as u64;
            let sobrepoe = [b.transicao_entrada.as_ref(), saida].into_iter().flatten().any(|t| tempo::transicao_sobrepoe(&t.tipo));
            let limite = if sobrepoe { n.saturating_sub(1) } else { n };
            if e + s > limite {
                self.por(TRANSICAO, &onde);
            }
        }
    }

    // ---- sobreposições

    fn sobreposicoes(&mut self) {
        let p = self.p;
        for t in &p.sobreposicoes {
            for c in &t.clipes {
                let c = match c {
                    Tolerante::Desconhecido(v) => {
                        let onde = Onde { clipe: v.get("id").and_then(Value::as_str).map(str::to_string), ..Default::default() }.trilha(&t.id);
                        self.desconhecido::<ClipeSobreposto>(v, TIPOS_SOBREPOSTO, !t.oculta, &onde);
                        continue;
                    }
                    Tolerante::Conhecido(c) => c,
                };
                let b = base_sobreposto(c);
                let onde = Onde::clipe(&b.id).trilha(&t.id);
                if t.oculta {
                    self.so_desconhecidos(&b.efeitos, &onde);
                    continue;
                }
                self.posicao(&b.pos, &onde, INTERVALO);
                if let Some(d) = b.duracao_s {
                    self.duracao(d, &onde);
                }
                let tr = &b.transformacao;
                if ![&tr.x, &tr.y, &tr.escala, &tr.rotacao, &tr.opacidade].iter().all(|a| animavel_ok(a)) {
                    self.por(ITEM_INVALIDO, &onde.campo("transformacao"));
                    continue;
                }
                let n_q = self.linha.livres.iter().find(|l| l.id == b.id).map_or(1, |l| l.n_q);
                match c {
                    ClipeSobreposto::Video(v) => {
                        self.add(NAO_SUPORTADO, &onde, None, Some("video"), None);
                        self.midia_existe(&v.midia, &onde);
                        if let Some(a) = &v.audio {
                            self.audio_do_clipe(a, &onde);
                        }
                        self.so_desconhecidos(&b.efeitos, &onde);
                        if let Some(id) = &v.sincronia {
                            if let Some(s) = self.p.sincronias.iter().find(|s| &s.id == id) {
                                if matches!(s.metodo, MetodoSincronia::Audio | MetodoSincronia::Relogio) && s.conferencias.is_empty() {
                                    self.por(SINCRONIA_SEM_CONFERENCIA, &onde);
                                }
                            }
                        }
                    }
                    ClipeSobreposto::Imagem(i) => {
                        self.imagem_existe(&i.midia, &onde);
                        let uso = self.uso_local(onde, &b.efeitos, b.duracao_s, n_q);
                        self.efeitos_em_sobreposto(&uso);
                        self.efeitos(&uso);
                    }
                    ClipeSobreposto::Titulo(x) => {
                        self.titulo(&x.titulo, &onde);
                        let uso = self.uso_local(onde, &b.efeitos, b.duracao_s, n_q);
                        self.efeitos_em_sobreposto(&uso);
                        self.efeitos(&uso);
                    }
                }
            }
        }
    }

    /// Efeito ligado em clipe sobreposto: a Fase 0 não aplica (o motor recusa). Um problema por efeito.
    fn efeitos_em_sobreposto(&mut self, uso: &Uso<'a>) {
        for e in uso.efeitos {
            if let Tolerante::Conhecido(e) = e {
                let b = base_efeito(e);
                if b.ligado {
                    self.add(NAO_SUPORTADO, &uso.onde.efeito(&b.id), None, Some("efeito_sobreposto"), None);
                }
            }
        }
    }

    /// Posição presa a um clipe que está na linha; tempo finito e não negativo.
    fn posicao(&mut self, pos: &Posicao, onde: &Onde, codigo: &'static str) {
        let ok = match pos {
            Posicao::Programa { t } => t.is_finite() && *t >= 0.0,
            Posicao::Clipe { clipe, desloc } => desloc.is_finite() && self.na_linha.contains(clipe.as_str()),
        };
        if !ok {
            self.por(codigo, &onde.campo("pos"));
        }
    }

    // ---- áudio

    fn audios(&mut self) {
        let p = self.p;
        for t in &p.audios {
            let trilha = Onde::default().trilha(&t.id);
            if !(0.0..=2.0).contains(&t.volume) {
                self.por(AUDIO, &trilha.campo("volume"));
            }
            let toca = p.saida.audio && !t.mudo;
            for c in &t.clipes {
                let onde = Onde::clipe(&c.id).trilha(&t.id);
                if toca {
                    self.midia_existe(&c.midia, &onde);
                }
                self.posicao(&c.pos, &onde, AUDIO);
                self.volume(&c.volume, &onde.campo("volume"));
                let recorte_ok = c.de_s.is_finite()
                    && c.de_s >= 0.0
                    && c.ate_s.is_none_or(|a| a.is_finite() && a > c.de_s)
                    && c.entrada_s.is_finite()
                    && c.entrada_s >= 0.0
                    && c.saida_s.is_finite()
                    && c.saida_s >= 0.0;
                if !recorte_ok {
                    self.por(AUDIO, &onde.campo("de_s"));
                }
            }
        }
    }

    // ---- o que o motor da Fase 0 recusa fora dos efeitos (contratos (h) 1)

    /// Predefinição e legendas de outra fase (só na exportação) e o som que o motor de áudio da Fase 0 não faz (só quando o
    /// alvo tem som e a saída leva áudio): as mesmas condições de `saida::codificacao` e de `audio::montar`.
    fn fase_0(&mut self) {
        let p = self.p;
        if self.exporta {
            if let Some(d) = motor_saida::recusa(self.exportacao) {
                self.add(NAO_SUPORTADO, &Onde::default().campo("exportacao"), None, Some(d), None);
            }
            // legendas que o motor ainda não gera (P1.6): nunca saem faltando caladas
            if let Some(d) = motor_legendas::recusa(p, self.exportacao) {
                self.add(NAO_SUPORTADO, &Onde::default().campo("legendas"), None, Some(d), None);
            }
        }
        if !(self.com_som && p.saida.audio) {
            return;
        }
        // o som que o motor de áudio ainda não faz (P1.5)
        if p.mixagem.normalizar.is_some() && motor_audio::recusa("normalizar") {
            self.add(NAO_SUPORTADO, &Onde::default().campo("normalizar"), None, Some("normalizar"), None);
        }
        if p.mixagem.limitador && motor_audio::recusa("limitador") {
            self.add(NAO_SUPORTADO, &Onde::default().campo("limitador"), None, Some("limitador"), None);
        }
        for c in &p.principal {
            let Tolerante::Conhecido(ClipePrincipal::Trecho(t)) = c else { continue };
            if t.base.desligado || t.audio.mudo {
                continue;
            }
            let onde = Onde::clipe(&t.base.id).campo("audio");
            if !t.audio.silencios.is_empty() && motor_audio::recusa("silenciar") {
                self.add(NAO_SUPORTADO, &onde, None, Some("silenciar"), None);
            }
            if volume_animado(&t.audio.volume) && motor_audio::recusa("volume_animado") {
                self.add(NAO_SUPORTADO, &onde, None, Some("volume_animado"), None);
            }
        }
        for t in p.audios.iter().filter(|t| !t.mudo && !t.clipes.is_empty()) {
            if t.abaixa_original && motor_audio::recusa("abaixar") {
                self.add(NAO_SUPORTADO, &Onde::default().trilha(&t.id).campo("abaixa_original"), None, Some("abaixar"), None);
            }
            for c in t.clipes.iter().filter(|c| volume_animado(&c.volume) && motor_audio::recusa("volume_animado")) {
                self.add(NAO_SUPORTADO, &Onde::clipe(&c.id).trilha(&t.id).campo("volume"), None, Some("volume_animado"), None);
            }
        }
    }

    // ---- legendas

    fn legendas(&mut self) {
        let p = self.p;
        let l = &p.legendas;
        let saem = self.exportacao.legendas != ModoLegendas::Nenhuma;
        if !l.itens.is_empty() {
            self.estilo(&l.estilo, &Onde::default().campo("legendas"));
        }
        // as que mostrariam o que um silêncio cala: a regra do motor (a mesma conta do som)
        let sob_silencio: HashSet<String> = if saem { motor_legendas::sob_silencio(p, self.linha).into_iter().collect() } else { HashSet::new() };
        for g in &l.itens {
            let onde = Onde::clipe(&g.id).campo("legendas");
            let (de, ate) = match &g.alvo {
                RefLegenda::Fonte { de, ate, .. } | RefLegenda::Programa { de, ate } => (*de, *ate),
            };
            let midia_ok = match &g.alvo {
                RefLegenda::Fonte { midia, .. } => self.midias.contains_key(midia.as_str()),
                RefLegenda::Programa { .. } => true,
            };
            if !(de.is_finite() && ate.is_finite() && ate > de) || g.texto.trim().is_empty() || g.texto.chars().count() > LIMITE_TEXTO || !midia_ok {
                // só barra quando as legendas saem; com "nenhuma" a tela avisa
                self.add(LEGENDA, &onde, None, None, Some(if saem { Nivel::Erro } else { Nivel::Aviso }));
                continue;
            }
            if !saem {
                continue;
            }
            if g.origem == OrigemLegenda::Ia && !g.conferida {
                self.por(LEGENDA_NAO_CONFERIDA, &onde);
            }
            if sob_silencio.contains(&g.id) {
                self.por(LEGENDA_SOB_SILENCIO, &onde);
            }
        }
    }

    // ---- marcadores e sincronias

    fn marcadores(&mut self) {
        for m in &self.p.marcadores {
            self.cor(&m.cor, &Onde::clipe(&m.id).campo("cor"));
        }
    }

    fn sincronias_da_tela(&mut self, t: &super::modelo::ClipeTela, onde: &Onde) {
        for cel in &t.celulas {
            let Some(id) = &cel.sincronia else { continue };
            let Some(s) = self.p.sincronias.iter().find(|s| &s.id == id) else { continue };
            if matches!(s.metodo, MetodoSincronia::Audio | MetodoSincronia::Relogio) && s.conferencias.is_empty() {
                self.por(SINCRONIA_SEM_CONFERENCIA, onde);
            }
            if t.inicio_s < s.valido_de_s - EPS || t.fim_s > s.valido_ate_s + EPS {
                self.por(SINCRONIA_EXTRAPOLADA, onde);
            }
        }
    }

    // ---- mídias usadas

    fn midias_usadas(&mut self) {
        let usadas = std::mem::take(&mut self.usadas);
        for id in &usadas {
            let Some(m) = self.midias.get(id).copied() else { continue };
            let onde = Onde::midia(id);
            let estado = self.ctx.midias.iter().find(|e| e.id == *id);
            let fonte = self.ctx.fontes.get(*id);
            match estado.map(|e| e.estado) {
                Some(SituacaoMidia::Ausente) => self.por(MIDIA_AUSENTE, &onde),
                Some(SituacaoMidia::Trocada) => self.por(MIDIA_TROCADA, &onde),
                // dada como presente, mas sem `FonteInfo`: não há o que renderizar
                _ if fonte.is_none() => self.por(MIDIA_AUSENTE, &onde),
                _ => {}
            }
            if let Some(f) = fonte {
                // foto (PNG = `rgb24` de faixa cheia) entra pelo rasterizador, não por esta cadeia
                let lida = self.como_video.contains(id);
                if lida && f.exibido.0 > 0 && f.exibido.1 > 0 && cor::cadeia_fonte(f.matriz, f.faixa, &f.pix_fmt, f.exibido).is_none() {
                    self.por(PIXEL_NAO_SUPORTADO, &onde);
                }
                if m.tipo == TipoMidia::Video && mais_que_420_8bits(&f.pix_fmt) {
                    self.por(FONTE_10BITS, &onde);
                }
            }
            let origem = fonte.map(|f| f.quadros.origem).or(estado.and_then(|e| e.tabela));
            if self.final_ && m.tipo == TipoMidia::Video && origem == Some(OrigemTabela::Pacotes) {
                self.por(TABELA_PENDENTE, &onde);
            }
            let erros = [
                m.decodificacao.map(|d| d.erros),
                estado.and_then(|e| e.erros),
                fonte.filter(|f| f.quadros.origem == OrigemTabela::Decodificador).map(|f| f.quadros.erros_decodificacao),
            ]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(0);
            if erros > 0 {
                self.por_n(FONTE_DANIFICADA, &onde, erros as u64);
            }
            if let Some(d) = &m.derivacao {
                if d.com_perdas {
                    self.por(MIDIA_DERIVADA_COM_PERDAS, &onde);
                }
                if d.tipo == TipoDerivacao::Superres {
                    self.por(IMAGEM_IA, &onde);
                }
            }
        }
        self.usadas = usadas;
    }

    // ---- tempos movidos para o quadro que o decodificador entrega

    fn ajustados(&mut self, prender: &[Prender]) {
        let mut por_clipe: Vec<(String, u64)> = Vec::new();
        let mut ancoras = 0u64;
        for x in prender {
            let Some(antes) = self.pts_gravado(&x.onde) else { continue };
            if x.pts == Some(antes) {
                continue;
            }
            match &x.onde {
                OndePrender::Ancora { .. } => ancoras += 1,
                OndePrender::Inicio { clipe }
                | OndePrender::Fim { clipe }
                | OndePrender::Instante { clipe }
                | OndePrender::AnaliseDe { clipe, .. }
                | OndePrender::AnaliseAte { clipe, .. } => match por_clipe.iter_mut().find(|(c, _)| c == clipe) {
                    Some((_, n)) => *n += 1,
                    None => por_clipe.push((clipe.clone(), 1)),
                },
            }
        }
        for (c, n) in por_clipe {
            self.por_n(QUADRO_AJUSTADO, &Onde::clipe(&c), n);
        }
        if ancoras > 0 {
            self.por_n(QUADRO_AJUSTADO, &Onde::default().campo("ancoras"), ancoras);
        }
    }

    /// pts que o documento já tinha no lugar (None = ainda não preso).
    fn pts_gravado(&self, onde: &OndePrender) -> Option<i64> {
        let principal = |id: &str| {
            self.p.principal.iter().find_map(|c| match c {
                Tolerante::Conhecido(c) if base_clipe(c).id == id => Some(c),
                _ => None,
            })
        };
        let sobreposto = |id: &str| {
            self.p.sobreposicoes.iter().flat_map(|t| &t.clipes).find_map(|c| match c {
                Tolerante::Conhecido(ClipeSobreposto::Video(v)) if v.base.id == id => Some(v),
                _ => None,
            })
        };
        let analise = |clipe: &str, efeito: &str| {
            let efeitos = principal(clipe).map(|c| &base_clipe(c).efeitos).or(sobreposto(clipe).map(|v| &v.base.efeitos))?;
            efeitos.iter().find_map(|e| match e {
                Tolerante::Conhecido(Efeito::Estabilizar(s)) if s.base.id == efeito => Some(&s.analise),
                _ => None,
            })
        };
        match onde {
            OndePrender::Inicio { clipe } => match principal(clipe) {
                Some(ClipePrincipal::Trecho(t)) => t.inicio_pts,
                Some(ClipePrincipal::Tela(t)) => t.inicio_pts,
                _ => sobreposto(clipe).and_then(|v| v.inicio_pts),
            },
            OndePrender::Fim { clipe } => match principal(clipe) {
                Some(ClipePrincipal::Trecho(t)) => t.fim_pts,
                Some(ClipePrincipal::Tela(t)) => t.fim_pts,
                _ => None,
            },
            OndePrender::Instante { clipe } => match principal(clipe) {
                Some(ClipePrincipal::Congelar(g)) => g.instante_pts,
                _ => None,
            },
            OndePrender::Ancora { rastreio, indice } => self.rastreios.get(rastreio.as_str())?.ancoras.get(*indice as usize)?.pts,
            OndePrender::AnaliseDe { clipe, efeito } => analise(clipe, efeito).map(|a| a.de_pts),
            OndePrender::AnaliseAte { clipe, efeito } => analise(clipe, efeito).map(|a| a.ate_pts),
        }
    }
}

// ---------------------------------------------------------------------------
// Auxiliares

fn base_clipe(c: &ClipePrincipal) -> &super::modelo::ClipeBase {
    match c {
        ClipePrincipal::Trecho(x) => &x.base,
        ClipePrincipal::Congelar(x) => &x.base,
        ClipePrincipal::Imagem(x) => &x.base,
        ClipePrincipal::Titulo(x) => &x.base,
        ClipePrincipal::Tela(x) => &x.base,
    }
}

fn base_sobreposto(c: &ClipeSobreposto) -> &super::modelo::SobBase {
    match c {
        ClipeSobreposto::Video(x) => &x.base,
        ClipeSobreposto::Imagem(x) => &x.base,
        ClipeSobreposto::Titulo(x) => &x.base,
    }
}

fn base_efeito(e: &Efeito) -> &super::modelo::EfBase {
    match e {
        Efeito::Estabilizar(x) => &x.base,
        Efeito::Lente(x) => &x.base,
        Efeito::Desentrelacar(x) => &x.base,
        Efeito::MediaTemporal(x) => &x.base,
        Efeito::Ajustes(x) => &x.base,
        Efeito::Nitidez(x) => &x.base,
        Efeito::Girar(x) => &x.base,
        Efeito::Enquadramento(x) => &x.base,
        Efeito::Ocultar(x) => &x.base,
        Efeito::Holofote(x) => &x.base,
        Efeito::Destaque(x) => &x.base,
        Efeito::Lupa(x) => &x.base,
        Efeito::Regua(x) => &x.base,
        Efeito::VelocidadeExibida(x) => &x.base,
        Efeito::Texto(x) => &x.base,
        Efeito::Carimbo(x) => &x.base,
        Efeito::Imagem(x) => &x.base,
    }
}

fn efeito_onde(clipe: &Onde, v: &Value) -> Onde {
    Onde { efeito: v.get("id").and_then(Value::as_str).map(str::to_string), ..clipe.clone() }
}

/// Filtros opcionais do ffmpeg que o efeito pede (os obrigatórios a sonda já exige). Enquadramento: só o que vale
/// (o primeiro ligado) e só quando o motor o trata como animado (`enquadramento::animado`).
fn filtros_de(e: &Efeito, todos: &[Tolerante<Efeito>]) -> Vec<&'static str> {
    match e {
        Efeito::Estabilizar(s) if s.trf_sha256.is_none() => vec!["vidstabdetect", "vidstabtransform"],
        Efeito::Estabilizar(_) => vec!["vidstabtransform"],
        Efeito::Lente(_) => vec!["lenscorrection"],
        Efeito::Desentrelacar(_) => vec!["bwdif"],
        Efeito::MediaTemporal(_) => vec!["tmix"],
        Efeito::Nitidez(_) => vec!["unsharp"],
        Efeito::Enquadramento(q) if primeiro_enquadramento(todos).is_some_and(|p| std::ptr::eq(p, q)) && enquadramento::animado(todos) => vec!["perspective"],
        _ => Vec::new(),
    }
}

fn caixa_finita(c: &Caixa) -> bool {
    c.x.is_finite() && c.y.is_finite() && c.w.is_finite() && c.h.is_finite()
}

/// Tempos finitos e em ordem (iguais valem: a última vence).
fn chaves_em_ordem<T>(ks: &[Chave<T>]) -> bool {
    ks.iter().all(|k| k.t.is_finite()) && ks.windows(2).all(|w| w[0].t <= w[1].t)
}

/// `ocultar` do próprio clipe congelado, pela regra do motor (`clipe::efeitos_do_congelado`): Some(true) = vale no
/// quadro (o `ativo` local pega todos os slots), Some(false) = não vale (nenhum), None = só parte (recusado).
fn ocultar_do_congelado(oc: &EfOcultar, uso: &Uso) -> Option<bool> {
    let e = [Tolerante::Conhecido(Efeito::Ocultar(oc.clone()))];
    match motor_clipe::efeitos_do_congelado(&e, uso.n_q, uso.fps) {
        Err(_) => None,
        Ok(v) => Some(matches!(&v[0], Tolerante::Conhecido(Efeito::Ocultar(o)) if o.base.ligado)),
    }
}

/// Volume com chaves de valores diferentes (o motor de áudio da Fase 0 só faz volume fixo; chaves iguais valem como fixo).
fn volume_animado(v: &Animavel<f64>) -> bool {
    matches!(v, Animavel::Chaves(ks) if ks.windows(2).any(|w| w[0].v != w[1].v))
}

/// Estrutura de um `Animavel`: lista de chaves não vazia e em ordem (contratos (f) 7).
fn animavel_ok<T>(a: &Animavel<T>) -> bool {
    match a {
        Animavel::Fixo(_) => true,
        Animavel::Chaves(ks) => !ks.is_empty() && chaves_em_ordem(ks),
    }
}

fn primeiro_enquadramento(efeitos: &[Tolerante<Efeito>]) -> Option<&super::modelo::EfEnquadramento> {
    efeitos.iter().find_map(|e| match e {
        Tolerante::Conhecido(Efeito::Enquadramento(q)) if q.base.ligado => Some(q),
        _ => None,
    })
}

/// O enquadramento que vale pode ser avaliado?
fn estrutura_do_enquadre_ok(efeitos: &[Tolerante<Efeito>]) -> bool {
    primeiro_enquadramento(efeitos).is_none_or(|q| animavel_ok(&q.regiao))
}

/// O canto do bloco `i` do título cabe no limite do rasterizador? A resposta é a dele: `camada::titulo::primitivas`
/// com o bloco sozinho e o sprite medido em memória (sem cache em disco).
fn canto_do_bloco_cabe(t: &Titulo, i: usize, canvas: TamanhoPx) -> bool {
    use camada::texto::{self, Atlas, FonteSprites, PedidoSprite, Sprite};
    struct Medidor;
    impl FonteSprites for Medidor {
        fn sprite(&self, pedido: &PedidoSprite) -> crate::error::Result<Sprite> {
            let (b, _) = texto::bitmap_sprite(pedido)?;
            Ok(Sprite { relativo: String::new(), w: b.w, h: b.h })
        }
        fn atlas(&self, _: &EstiloTexto, _: u32) -> crate::error::Result<Atlas> {
            Err(crate::error::SicroError::Validation("atlas: título não usa".into()))
        }
        fn imagem(&self, _: &str) -> Option<TamanhoPx> {
            None
        }
    }
    let so = Titulo { fundo: Fundo::Transparente, blocos: vec![t.blocos[i].clone()], ..t.clone() };
    let quadro = camada::formas::QuadroCamada {
        q: 0,
        t: 0.0,
        pts: None,
        a: geometria::Afim::IDENTIDADE,
        canvas,
        rastreios: std::collections::BTreeMap::new(),
        indice: None,
        programa_s: 0.0,
    };
    // só a recusa de lugar interessa aqui: cor, largura e texto longo têm regra própria
    !matches!(camada::titulo::primitivas(&so, &quadro, &Medidor), Err(crate::error::SicroError::Validation(m)) if m.starts_with(ITEM_INVALIDO) && m.contains("lugar"))
}

/// Formato com mais de 8 bits ou croma 4:2:2/4:4:4 (convertido para 8 bits 4:2:0 antes dos realces).
fn mais_que_420_8bits(pix_fmt: &str) -> bool {
    let f = pix_fmt.to_ascii_lowercase();
    let bits = ["p9", "p10", "p12", "p14", "p16", "010", "012", "016", "210", "410", "y212", "48", "64"].iter().any(|x| f.contains(x));
    let croma = ["422", "444", "440", "nv16", "nv24", "nv42", "v210", "v308", "v410", "ayuv"].iter().any(|x| f.contains(x));
    bits || croma
}

/// Taxa variável pelos passos de pts do trecho (`receitas-audio-saida.md` §8): passos fora de ±25 % da mediana
/// em mais de 0,5 % deles, ou alguma lacuna (> 1,5× a mediana). Devolve quantos passos fogem da faixa.
fn taxa_variavel(tab: &TabelaQuadros, a: usize, b: usize) -> Option<u64> {
    // o passo do último quadro do trecho até o seguinte também é tempo do trecho
    let fim = b.min(tab.pts.len().checked_sub(1)?);
    if fim < a + 2 {
        return None;
    }
    let mut passos: Vec<i64> = tab.pts[a..=fim].windows(2).map(|w| w[1] - w[0]).collect();
    let n = passos.len();
    passos.sort_unstable();
    let med = passos[n / 2] as f64;
    if med <= 0.0 {
        return None;
    }
    let tick = tab.tb.num as f64 / tab.tb.den as f64;
    let tol = (VFR_TOLERANCIA_S / tick).max(1.0);
    let (mut fora, mut lacunas) = (0u64, 0u64);
    for &d in &passos {
        let d = d as f64;
        if (d - med).abs() > tol && (d - med).abs() > VFR_FAIXA * med {
            fora += 1;
        }
        if d > VFR_LACUNA * med {
            lacunas += 1;
        }
    }
    (lacunas > 0 || fora as f64 > VFR_PARTE * n as f64).then_some(fora)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::ffmpeg::sonda::Capacidades;
    use crate::edicao::importar_v1::{self, FontesV1};
    use crate::edicao::modelo::{EstadoMidia, Matriz, Faixa, OrigemMidia, Recebido};
    use crate::edicao::motor::FonteInfo;
    use crate::edicao::quadros::apoio::{self, SemRastreios};
    use crate::edicao::quadros::Busca;
    use serde_json::json;
    use std::collections::BTreeSet;
    use std::path::Path;

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA2: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    /// Base 1/15360 a 30 qps: quadro k tem pts 512·k.
    const TB: (i64, i64) = (1, 15360);

    fn t(k: i64) -> f64 {
        (512 * k) as f64 / 15360.0
    }

    /// 300 quadros (10 s) a 30 qps.
    fn tabela() -> TabelaQuadros {
        apoio::tabela(TB, apoio::pts_regulares(300, (1, 30), TB, 0))
    }

    fn fonte(tab: TabelaQuadros) -> FonteInfo {
        let mut f = apoio::fonte(tab, (1280, 720));
        f.sha256 = SHA.into();
        f
    }

    fn imagem() -> FonteInfo {
        let mut f = apoio::fonte(apoio::tabela((1, 1), vec![0]), (1280, 720));
        f.sha256 = SHA2.into();
        f.pix_fmt = "rgb24".into();
        f
    }

    fn cap() -> Capacidades {
        let mut c = apoio::cap_falsa();
        for f in ["vidstabdetect", "vidstabtransform", "lenscorrection", "bwdif", "tmix", "perspective", "unsharp"] {
            c.filtros.insert(f.into());
        }
        c
    }

    fn estado(id: &str, estado: SituacaoMidia) -> EstadoMidia {
        EstadoMidia { id: id.into(), estado, tabela: Some(OrigemTabela::Decodificador), busca: Some(Busca::Confiavel), erros: Some(0) }
    }

    fn midia(id: &str, tipo: &str, sha: &str) -> Value {
        json!({ "id": id, "tipo": tipo, "origem": "caso", "media_id": null, "sha256": sha, "nome": "cam.mp4", "relativo": "Mídias/cam.mp4",
                "duracao_s": 10.0, "inicio_s": 0.0, "largura": 1280, "altura": 720, "taxa": { "num": 30, "den": 1 }, "vfr": false,
                "tem_audio": false, "cor": null, "derivacao": null, "recebido": { "nativo": true, "obs": "" }, "relogio_desvio_s": null,
                "gravador": null, "decodificacao": { "erros": 0 }, "ocultacoes": [] })
    }

    /// Trecho dos quadros 30..90 (1 s a 3 s) da m1.
    fn trecho(id: &str) -> Value {
        json!({ "tipo": "trecho", "id": id, "rotulo": "", "desligado": false, "transicao_entrada": null, "efeitos": [],
                "midia": "m1", "inicio_s": t(30), "fim_s": t(90), "inicio_pts": 512 * 30, "fim_pts": 512 * 90,
                "velocidade": { "tipo": "constante", "fator": 1.0 },
                "audio": { "volume": 1.0, "mudo": false, "manter_tom": true, "silencios": [] } })
    }

    fn estilo() -> Value {
        json!({ "fonte": "sans", "negrito": false, "italico": false, "tamanho": 0.05, "cor": "#ffffff", "alinhamento": "centro",
                "caixa": null, "contorno": null })
    }

    fn base(tipo: &str, id: &str) -> Value {
        json!({ "tipo": tipo, "id": id, "ligado": true, "nome": "", "ativo": null })
    }

    fn com(mut a: Value, b: Value) -> Value {
        for (k, v) in b.as_object().unwrap() {
            a[k] = v.clone();
        }
        a
    }

    fn ocultar(rastreio: &str) -> Value {
        com(base("ocultar", "e1"), json!({ "modo": "solido", "alvo": { "rastreio": rastreio, "forma": "retangulo", "vertices": null,
            "margem": 0.0, "borda_suave": 0.0, "inverter": false }, "intensidade": 1.0, "cor": "#000000ff", "conteudo": "rosto", "motivo": "terceiro" }))
    }

    fn ancora(k: i64) -> Value {
        json!({ "t": t(k), "pts": 512 * k, "v": { "x": 0.1, "y": 0.1, "w": 0.2, "h": 0.2 }, "interp": "linear" })
    }

    /// Rastreio manual de fonte da m1, no espaço original, com âncoras nos quadros dados.
    fn rastreio(quadros: &[i64]) -> Value {
        json!({ "id": "r1", "nome": "NOME-SECRETO", "cor": "#ffcc00", "midia": "m1", "base": "fonte", "tipo": "caixa", "espaco": "original",
                "ancoras": quadros.iter().map(|&k| ancora(k)).collect::<Vec<_>>(), "denso": null, "silhuetas": null, "ausencias": [],
                "origem": { "tipo": "manual" }, "confirmacao": null })
    }

    fn titulo(id: &str, duracao: f64) -> Value {
        json!({ "tipo": "titulo", "id": id, "rotulo": "", "desligado": false, "transicao_entrada": null, "efeitos": [], "duracao_s": duracao,
                "titulo": { "modelo": "cartela", "fundo": { "tipo": "cor", "cor": "#000000" }, "blocos": [] } })
    }

    fn sincronia(metodo: &str, conferencias: Value, ate: f64) -> Value {
        json!({ "id": "s1", "vinculo_id": null, "referencia": "m1", "outra": "m2", "desloc_s": 0.5, "deriva_ppm": 0.0,
                "deriva_incerteza_ppm": 0.0, "t0_s": 0.0, "valido_de_s": 0.0, "valido_ate_s": ate, "metodo": metodo,
                "incerteza_s": null, "confianca": null, "conferencias": conferencias })
    }

    fn tela() -> Value {
        json!({ "tipo": "tela", "id": "c1", "rotulo": "", "desligado": false, "transicao_entrada": null, "efeitos": [], "arranjo": "lado_a_lado",
                "celulas": [ { "midia": "m1", "sincronia": null, "rotulo": "", "efeitos": [] },
                             { "midia": "m1", "sincronia": "s1", "rotulo": "", "efeitos": [] } ],
                "referencia": 0, "inicio_s": t(30), "fim_s": t(90), "inicio_pts": 512 * 30, "fim_pts": 512 * 90,
                "velocidade": { "tipo": "constante", "fator": 1.0 }, "audio": { "volume": 1.0, "mudo": false, "manter_tom": true, "silencios": [] },
                "pip": null })
    }

    /// Projeto limpo: um trecho de 2 s da m1 numa saída 1280×720 a 30 qps; nenhum problema.
    struct Cena {
        p: Value,
        fontes: HashMap<String, FonteInfo>,
        midias: Vec<EstadoMidia>,
        cap: Capacidades,
        alvo: Option<Alvo>,
    }

    /// Efeito do tipo, tirado da fixture com todas as variantes, ligado.
    fn ef_do_tipo(tipo: &str) -> Value {
        let v: Value = serde_json::from_str(include_str!("../../tests/fixtures/edicao/contrato-todas-variantes.json")).unwrap();
        let mut e = v["principal"][0]["efeitos"].as_array().unwrap().iter().find(|e| e["tipo"] == tipo).cloned().expect(tipo);
        e["ligado"] = json!(true);
        e
    }

    /// `NAO_SUPORTADO` enquanto o motor recusar o efeito: a regra é a do pacote dono (contratos-fase1 (c)), e a
    /// expectativa some sozinha quando ele entrega.
    fn ns_efeito(e: &Value) -> Option<&'static str> {
        let e: Efeito = serde_json::from_value(e.clone()).expect("efeito");
        motor_clipe::recusa_do_efeito(&e, true).map(|_| NAO_SUPORTADO)
    }

    fn ns_tipo(tipo: &str) -> Option<&'static str> {
        ns_efeito(&ef_do_tipo(tipo))
    }

    fn ns_se(recusa: bool) -> Option<&'static str> {
        recusa.then_some(NAO_SUPORTADO)
    }

    /// Os códigos esperados mais o `nao_suportado`, se alguma recusa ainda vale.
    fn mais(cods: &[&'static str], recusas: &[Option<&'static str>]) -> Vec<&'static str> {
        let mut v = cods.to_vec();
        if recusas.iter().any(Option::is_some) {
            v.push(NAO_SUPORTADO);
        }
        v
    }

    fn cena() -> Cena {
        let p = json!({
            "esquema": 2, "esquema_menor": 0, "id": "p1", "titulo": "Caso", "created_at": "2026-11-14T00:00:00Z",
            "updated_at": "2026-11-14T00:00:00Z", "gravado_por": "6.0.0",
            "saida": { "largura": 1280, "altura": 720, "fps": { "num": 30, "den": 1 }, "auto": false, "cor_fundo": "#000000", "audio": true,
                       "selos": { "velocidade": true, "ampliacao": true, "realce": true },
                       "exportacao": { "predefinicao": "juri", "codificador": "cpu", "legendas": "nenhuma", "pacote": null, "pdf": "a3",
                                       "revisado_por": "", "personalizada": null } },
            "midias": [ midia("m1", "video", SHA) ],
            "principal": [ trecho("c1") ],
            "fecho": null, "sobreposicoes": [], "audios": [], "rastreios": [], "marcadores": [], "sincronias": []
        });
        Cena {
            p,
            fontes: HashMap::from([("m1".to_string(), fonte(tabela()))]),
            midias: vec![estado("m1", SituacaoMidia::Ok)],
            cap: cap(),
            alvo: None,
        }
    }

    impl Cena {
        fn projeto(&self) -> ProjetoEdicao {
            crate::edicao::armazem::tipado(&self.p).expect("lê tipado")
        }
        fn analise(&self) -> Analise {
            let p = self.projeto();
            let ctx = Contexto { fontes: &self.fontes, cap: &self.cap, rastreios: &SemRastreios, pasta_projeto: Path::new(""), midias: &self.midias };
            let a = analisar(&p, &ctx, self.alvo.as_ref());
            // `validar` sobre a mesma linha dá o mesmo
            assert_eq!(validar(&p, &a.linha, &ctx, self.alvo.as_ref()), a.problemas);
            a
        }
        fn problemas(&self) -> Vec<Problema> {
            self.analise().problemas
        }
        fn codigos(&self) -> BTreeSet<&'static str> {
            self.problemas().iter().map(|p| p.codigo).collect()
        }
        fn so(&self, esperado: &[&'static str]) {
            let problemas = self.problemas();
            let cods: BTreeSet<&str> = problemas.iter().map(|p| p.codigo).collect();
            assert_eq!(cods, esperado.iter().copied().collect::<BTreeSet<_>>(), "{problemas:#?}");
            for p in &problemas {
                frase_do_catalogo(p);
            }
        }
    }

    /// A mensagem é a frase do catálogo (com o detalhe que ela aceita, depois de ": ").
    fn frase_do_catalogo(p: &Problema) {
        let base = mensagens::frase(p.codigo, p.n, None);
        assert!(p.mensagem == base || p.mensagem.starts_with(&format!("{base}: ")), "{p:?} fora do catálogo");
        assert!(mensagens::nivel(p.codigo).is_some(), "{} fora do catálogo", p.codigo);
    }

    fn efeito(c: &mut Cena, e: Value) {
        c.p["principal"][0]["efeitos"].as_array_mut().unwrap().push(e);
    }

    #[test]
    fn projeto_limpo_nao_dispara_nada() {
        let c = cena();
        let a = c.analise();
        assert_eq!(a.problemas, vec![], "{:#?}", a.problemas);
        assert_eq!(a.linha.total_q, 60);
        assert_eq!(a.prender, vec![]);
        assert_eq!(a.midias, c.midias);
        assert_eq!(a.saida_auto, None);
        // com ocultação coberta, título, imagem, texto e carimbo também limpo
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[30])]);
        efeito(&mut c, ocultar("r1"));
        efeito(&mut c, com(base("texto", "e2"), json!({ "conteudo": "Portão", "estilo": estilo(),
            "lugar": { "x": 0.5, "y": 0.9, "h": "centro", "v": "base" }, "largura_max": 0.8 })));
        efeito(&mut c, com(base("carimbo", "e3"), json!({ "mostra": "tempo_fonte", "formato": "hms_ms", "relogio": null, "estilo": estilo(),
            "lugar": { "x": 0.9, "y": 0.1, "h": "dir", "v": "topo" } })));
        c.p["midias"].as_array_mut().unwrap().push(midia("m2", "imagem", SHA2));
        c.p["principal"].as_array_mut().unwrap().push(titulo("c2", 2.0));
        c.p["principal"].as_array_mut().unwrap().push(json!({ "tipo": "imagem", "id": "c3", "rotulo": "", "desligado": false,
            "transicao_entrada": { "tipo": "escurecer", "duracao_s": 0.5, "audio": "acompanhar" }, "efeitos": [], "midia": "m2", "duracao_s": 2.0 }));
        c.fontes.insert("m2".into(), imagem());
        c.midias.push(estado("m2", SituacaoMidia::Ok));
        c.alvo = Some(Alvo::Final { exportacao: c.projeto().saida.exportacao });
        c.so(&[]);
    }

    // ---- um caso por código (erros)

    #[test]
    fn esquema_e_esquema_menor() {
        let mut c = cena();
        c.p["id"] = json!("a/b");
        c.so(&[ESQUEMA]);
        let mut c = cena();
        c.p["esquema_menor"] = json!(1);
        c.so(&[ESQUEMA_MENOR]);
        // no JSON cru
        let cru = |v: Value| esquema(&v).into_iter().map(|p| (p.codigo, p.campo)).collect::<Vec<_>>();
        assert_eq!(cru(cena().p), vec![]);
        assert_eq!(cru(json!({ "esquema": 1, "id": "p1" })), vec![(ESQUEMA, Some("esquema".into()))]);
        assert_eq!(cru(json!({ "esquema": 2, "id": "a".repeat(81) })), vec![(ESQUEMA, Some("id".into()))]);
        assert_eq!(cru(json!({ "esquema": 2, "id": "p1", "esquema_menor": 3 })), vec![(ESQUEMA_MENOR, Some("esquema_menor".into()))]);
        assert_eq!(cru(json!([1])).len(), 1);
    }

    #[test]
    fn saida() {
        for (campo, v) in [("largura", json!(1281)), ("altura", json!(32)), ("fps", json!({ "num": 121, "den": 1 })), ("fps", json!({ "num": 1, "den": 2 }))] {
            let mut c = cena();
            c.p["saida"][campo] = v;
            let p = c.problemas();
            assert!(p.iter().any(|p| p.codigo == SAIDA && p.campo.as_deref() == Some(campo)), "{campo}: {p:#?}");
        }
        let mut c = cena();
        c.p["saida"]["largura"] = json!(1281);
        c.so(&[SAIDA]);
        // H.264 acima de 4096 só barra a exportação final
        let mut c = cena();
        c.p["saida"]["largura"] = json!(4608);
        c.p["saida"]["altura"] = json!(2592);
        c.so(&[]);
        c.alvo = Some(Alvo::Final { exportacao: c.projeto().saida.exportacao });
        c.so(&[SAIDA]);
        // personalizada com altura máxima que traz o lado para baixo de 4096
        let mut e = c.projeto().saida.exportacao;
        e.predefinicao = Predefinicao::Personalizada;
        e.personalizada = Some(super::super::modelo::Personalizada {
            codec: Codec::H264, conteiner: super::super::modelo::Conteiner::Mp4, qualidade: 20.0, altura_max: Some(2160), audio_kbps: 192 });
        let recusa = motor_saida::recusa(&e).is_some();
        c.alvo = Some(Alvo::Final { exportacao: e });
        // a altura máxima traz o lado para baixo de 4096; a predefinição é do P1.8
        c.so(&mais(&[], &[ns_se(recusa)]));
    }

    #[test]
    fn vazio() {
        let mut c = cena();
        c.p["principal"][0]["desligado"] = json!(true);
        c.so(&[VAZIO]);
        let mut c = cena();
        c.p["principal"] = json!([]);
        c.so(&[VAZIO]);
    }

    #[test]
    fn item_desconhecido_e_invalido() {
        let mut c = cena();
        efeito(&mut c, json!({ "tipo": "realce_por_ia", "id": "e9" }));
        c.so(&[ITEM_DESCONHECIDO]);
        let mut c = cena();
        efeito(&mut c, json!({ "tipo": "nitidez", "id": "e9" }));
        c.so(&[ITEM_INVALIDO]);
        let p = c.problemas();
        assert_eq!((p[0].clipe.as_deref(), p[0].efeito.as_deref(), p[0].nivel), (Some("c1"), Some("e9"), Nivel::Erro));
        assert!(p[0].mensagem.starts_with("Item inválido: missing field `quantidade`"), "{}", p[0].mensagem);
        // clipe de tipo conhecido que não desserializa
        let mut c = cena();
        c.p["principal"].as_array_mut().unwrap().push(json!({ "tipo": "congelar", "id": "c9" }));
        c.so(&[ITEM_INVALIDO]);
        // em clipe desligado: aviso (a tela trata como desconhecido do mesmo jeito)
        let mut c = cena();
        c.p["principal"].as_array_mut().unwrap().push(trecho("c2"));
        c.p["principal"][1]["desligado"] = json!(true);
        c.p["principal"][1]["efeitos"] = json!([{ "tipo": "nitidez", "id": "e9" }]);
        let p = c.problemas();
        assert_eq!(p.iter().map(|p| (p.codigo, p.nivel)).collect::<Vec<_>>(), vec![(ITEM_INVALIDO, Nivel::Aviso)]);
        // sobreposto desconhecido em trilha oculta: aviso
        let mut c = cena();
        c.p["sobreposicoes"] = json!([{ "id": "t1", "nome": "", "oculta": true, "clipes": [{ "tipo": "holograma", "id": "x1" }] }]);
        let p = c.problemas();
        assert_eq!(p.iter().map(|p| (p.codigo, p.nivel, p.trilha.as_deref())).collect::<Vec<_>>(), vec![(ITEM_DESCONHECIDO, Nivel::Aviso, Some("t1"))]);
    }

    #[test]
    fn midia_ausente_e_trocada() {
        let mut c = cena();
        c.fontes.clear();
        c.midias[0].estado = SituacaoMidia::Ausente;
        c.so(&[MIDIA_AUSENTE]);
        assert_eq!(c.problemas()[0].midia.as_deref(), Some("m1"));
        let mut c = cena();
        c.fontes.clear();
        c.midias[0].estado = SituacaoMidia::Trocada;
        c.so(&[MIDIA_TROCADA]);
        // referência a mídia que nem está no projeto
        let mut c = cena();
        c.p["principal"][0]["midia"] = json!("m9");
        c.so(&[MIDIA_AUSENTE]);
    }

    #[test]
    fn tabela_pendente_so_na_exportacao_final() {
        let mut c = cena();
        let mut tab = tabela();
        tab.origem = OrigemTabela::Pacotes;
        c.fontes.insert("m1".into(), fonte(tab));
        c.so(&[]);
        c.alvo = Some(Alvo::Final { exportacao: c.projeto().saida.exportacao });
        c.so(&[TABELA_PENDENTE]);
        c.alvo = Some(Alvo::Previa { altura: 360, de_q: 0, ate_q: 30 });
        c.so(&[]);
    }

    #[test]
    fn intervalo_e_mapa_grande() {
        // começa depois do fim da mídia
        let mut c = cena();
        for (k, v) in [("inicio_s", json!(20.0)), ("fim_s", json!(25.0)), ("inicio_pts", json!(null)), ("fim_pts", json!(null))] {
            c.p["principal"][0][k] = v;
        }
        c.so(&[INTERVALO]);
        // fim antes do início
        let mut c = cena();
        c.p["principal"][0]["fim_pts"] = json!(512 * 10);
        c.p["principal"][0]["fim_s"] = json!(t(10));
        c.so(&[INTERVALO]);
        // dura menos que um slot da saída: 2 quadros a 16×
        let mut c = cena();
        c.p["principal"][0]["fim_pts"] = json!(512 * 32);
        c.p["principal"][0]["fim_s"] = json!(t(32));
        c.p["principal"][0]["velocidade"]["fator"] = json!(16.0);
        c.so(&[INTERVALO]);
        // ativo invertido num efeito
        let mut c = cena();
        efeito(&mut c, com(base("girar", "e2"), json!({ "graus": 0, "espelhar_h": false, "espelhar_v": false, "ativo": { "de": 2.0, "ate": 1.5 } })));
        c.so(&[INTERVALO]);
        // P passando de 2^53 (base de tempo fina demais)
        let mut c = cena();
        let tb = (1, 1i64 << 50);
        c.fontes.insert("m1".into(), fonte(apoio::tabela(tb, apoio::pts_regulares(300, (1, 30), tb, 0))));
        c.p["principal"][0]["inicio_pts"] = json!(null);
        c.p["principal"][0]["fim_pts"] = json!(null);
        c.so(&[MAPA_GRANDE]);
    }

    #[test]
    fn duracao() {
        let mut c = cena();
        c.p["principal"] = json!([titulo("c1", 0.001)]);
        c.so(&[DURACAO]);
        let mut c = cena();
        c.p["principal"] = json!([titulo("c1", 3600.5)]);
        c.so(&[DURACAO]);
        // título de duração absurda com ocultação local: acusa sem percorrer os slots
        let mut c = cena();
        let mut t = titulo("c1", 1e12);
        t["efeitos"] = json!([ocultar("r2")]);
        c.p["principal"] = json!([t]);
        c.p["rastreios"] = json!([{ "id": "r2", "nome": "", "cor": "#ffcc00", "midia": null, "base": "local", "tipo": "caixa", "espaco": "original",
            "ancoras": [{ "t": 5.0, "pts": null, "v": { "x": 0.1, "y": 0.1, "w": 0.2, "h": 0.2 }, "interp": "linear" }], "denso": null,
            "silhuetas": null, "ausencias": [], "origem": { "tipo": "manual" }, "confirmacao": null }]);
        let inicio = std::time::Instant::now();
        // ocultar em título: a Fase 0 recusa (o motor não tem quadro lido onde aplicar)
        c.so(&[DURACAO, NAO_SUPORTADO]);
        assert!(inicio.elapsed().as_secs() < LIMITE_RAPIDO_S * 3, "{:?}", inicio.elapsed());
        // com duração válida a mesma ocultação percorre os slots e acha os 150 antes da âncora
        c.p["principal"][0]["duracao_s"] = json!(10.0);
        c.so(&[SEM_COBERTURA, NAO_SUPORTADO]);
        assert_eq!(c.problemas().iter().find(|p| p.codigo == SEM_COBERTURA).unwrap().n, Some(150));
    }

    /// Limite real (s) do teste acima; medido com folga de 3× enquanto houver outros agentes na máquina.
    const LIMITE_RAPIDO_S: u64 = 1;

    #[test]
    fn velocidade() {
        for fator in [0.0505, 0.04, 17.0] {
            let mut c = cena();
            c.p["principal"][0]["velocidade"]["fator"] = json!(fator);
            c.so(&[VELOCIDADE]);
        }
        // rampa: Fase 3 (recusada), com ponto fora do trecho
        let mut c = cena();
        c.p["principal"][0]["velocidade"] = json!({ "tipo": "rampa", "pontos": [ { "t": t(30), "v": 1.0 }, { "t": t(90), "v": 0.5 } ] });
        c.so(&[NAO_SUPORTADO]);
        c.p["principal"][0]["velocidade"]["pontos"][1]["t"] = json!(9.0);
        c.so(&[NAO_SUPORTADO, VELOCIDADE]);
    }

    #[test]
    fn transicao() {
        let mut c = cena();
        // transição do `xfade` (com sobreposição): o motor recusa até o P1.1 (imagem) e o P1.5 (som) entregarem
        c.p["principal"][0]["transicao_entrada"] = json!({ "tipo": "fundir", "duracao_s": 0.5, "audio": "acompanhar" });
        let som = c.p["saida"]["audio"].as_bool().unwrap_or(false);
        let recusa = !juncao::TRANSICOES.contains(&"fundir") || (som && motor_audio::recusa("transicao"));
        c.so(&mais(&[], &[ns_se(recusa)]));
        if recusa {
            assert_eq!(c.problemas()[0].mensagem, "Ainda não suportado: transição");
        }
        c.p["principal"][0]["transicao_entrada"]["tipo"] = json!(" ");
        c.so(&[TRANSICAO]);
        c.p["principal"][0]["transicao_entrada"] = json!({ "tipo": "escurecer", "duracao_s": -1.0, "audio": "acompanhar" });
        c.so(&[TRANSICAO]);
        // escurecer: entrada + saída maior que o clipe (60 slots)
        let mut c = cena();
        c.p["principal"][0]["transicao_entrada"] = json!({ "tipo": "escurecer", "duracao_s": 1.5, "audio": "acompanhar" });
        c.p["fecho"] = json!({ "tipo": "escurecer", "duracao_s": 1.0, "audio": "acompanhar" });
        c.so(&[TRANSICAO]);
        c.p["fecho"]["duracao_s"] = json!(0.5);
        c.so(&[]);
    }

    #[test]
    fn regiao() {
        // lista de chaves vazia: acusada antes de qualquer curva (nada entra em pânico)
        let enq = |regiao: Value, interp: &str| com(base("enquadramento", "e2"), json!({ "regiao": regiao, "ajuste": "conter", "interpolacao": interp }));
        let caixa = |x: f64, w: f64| json!({ "x": x, "y": 0.1, "w": w, "h": 0.5 });
        let chave = |k: i64, v: Value| json!({ "t": t(k), "pts": null, "v": v, "interp": "linear" });
        for r in [json!([]), json!([chave(60, caixa(0.1, 0.5)), chave(30, caixa(0.1, 0.5))]), caixa(0.1, 0.01)] {
            let mut c = cena();
            efeito(&mut c, enq(r.clone(), "bilinear"));
            c.so(&[REGIAO]);
        }
        // vizinho só com região fixa
        let mut c = cena();
        efeito(&mut c, enq(json!([chave(30, caixa(0.1, 0.5)), chave(60, caixa(0.2, 0.5))]), "vizinho"));
        c.so(&[REGIAO]);
        // ocultar com margem negativa
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[30])]);
        let mut o = ocultar("r1");
        o["alvo"]["margem"] = json!(-0.1);
        efeito(&mut c, o);
        c.so(&[REGIAO]);
        // ocultar sobre rastreio de ponto (sem tamanho)
        let mut c = cena();
        let mut r = rastreio(&[30]);
        r["tipo"] = json!("ponto");
        c.p["rastreios"] = json!([r]);
        efeito(&mut c, ocultar("r1"));
        c.so(&[REGIAO]);
        // âncoras fora de ordem: acusadas, sem avaliar
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[60, 30])]);
        efeito(&mut c, ocultar("r1"));
        c.so(&[REGIAO]);
    }

    #[test]
    fn segundo_enquadramento_e_item_invalido() {
        let mut c = cena();
        let enq = |id: &str| com(base("enquadramento", id), json!({ "regiao": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 }, "ajuste": "conter",
            "interpolacao": "bilinear" }));
        efeito(&mut c, enq("e2"));
        efeito(&mut c, enq("e3"));
        let p = c.problemas();
        assert_eq!(p.iter().map(|p| (p.codigo, p.efeito.as_deref())).collect::<Vec<_>>(), vec![(ITEM_INVALIDO, Some("e3"))]);
        // desligado não conta
        c.p["principal"][0]["efeitos"][0]["ligado"] = json!(false);
        c.so(&[]);
    }

    #[test]
    fn rastreio_ausente_base_e_espaco() {
        let mut c = cena();
        efeito(&mut c, ocultar("r9"));
        c.so(&[RASTREIO_AUSENTE]);
        // de outra mídia
        let mut c = cena();
        let mut r = rastreio(&[30]);
        r["midia"] = json!("m2");
        c.p["rastreios"] = json!([r]);
        efeito(&mut c, ocultar("r1"));
        c.so(&[RASTREIO_AUSENTE]);
        let mut c = cena();
        let mut r = rastreio(&[30]);
        r["base"] = json!("local");
        r["midia"] = json!(null);
        c.p["rastreios"] = json!([r]);
        efeito(&mut c, ocultar("r1"));
        c.so(&[RASTREIO_BASE]);
        // ocultar exige o espaço original
        let mut c = cena();
        let mut r = rastreio(&[30]);
        r["espaco"] = json!("origem:1234");
        c.p["rastreios"] = json!([r]);
        efeito(&mut c, ocultar("r1"));
        c.so(&[RASTREIO_ESPACO]);
        // destaque num clipe com lente: o espaço de origem do clipe
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[30])]);
        efeito(&mut c, com(base("lente", "e3"), json!({ "k1": -0.1, "k2": 0.0, "cx": 0.5, "cy": 0.5, "perfil": "" })));
        efeito(&mut c, destaque("r1"));
        // a lente é de origem: recusada até o P1.4 entregar
        c.so(&mais(&[RASTREIO_ESPACO], &[ns_tipo("lente")]));
        let espaco = geometria::espaco_de_origem(SHA, &c.projeto().principal_efeitos());
        c.p["rastreios"][0]["espaco"] = json!(espaco);
        c.so(&mais(&[], &[ns_tipo("lente")]));
    }

    trait Efeitos {
        fn principal_efeitos(&self) -> Vec<Tolerante<Efeito>>;
    }
    impl Efeitos for ProjetoEdicao {
        fn principal_efeitos(&self) -> Vec<Tolerante<Efeito>> {
            match &self.principal[0] {
                Tolerante::Conhecido(c) => base_clipe(c).efeitos.clone(),
                _ => panic!(),
            }
        }
    }

    fn destaque(rastreio: &str) -> Value {
        com(base("destaque", "e2"), json!({ "forma": "circulo", "rastreio": rastreio, "traco": { "cor": "#ff0000", "espessura": 0.006, "contorno": false },
            "tamanho": 0.12, "angulo_graus": 45.0, "margem": 0.0, "rotulo": null }))
    }

    #[test]
    fn rastreio_nao_confirmado() {
        let mut c = cena();
        let mut r = rastreio(&[30, 89]);
        r["origem"] = json!({ "tipo": "deteccao", "classe": "rosto" });
        c.p["rastreios"] = json!([r]);
        efeito(&mut c, destaque("r1"));
        c.so(&[RASTREIO_NAO_CONFIRMADO]);
        // confirmação que cobre o uso (1 s a 2,967 s) e o mesmo conteúdo
        c.p["rastreios"][0]["confirmacao"] = json!({ "por": "Perito", "em": "2026-11-14T00:00:00Z", "denso_sha256": null, "de_s": 1.0, "ate_s": 3.0 });
        c.so(&[]);
        // intervalo curto
        c.p["rastreios"][0]["confirmacao"]["ate_s"] = json!(2.0);
        c.so(&[RASTREIO_NAO_CONFIRMADO]);
        // Pessoas revisada não precisa
        c.p["rastreios"][0]["origem"] = json!({ "tipo": "pessoas", "pessoa_id": "x", "estado": "revisada", "copiado_em": "2026-11-14T00:00:00Z" });
        c.so(&[]);
        c.p["rastreios"][0]["origem"]["estado"] = json!("auto");
        c.so(&[RASTREIO_NAO_CONFIRMADO]);
    }

    #[test]
    fn sem_cobertura() {
        // primeira âncora no quadro 60: 30..59 sem caixa
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[60])]);
        efeito(&mut c, ocultar("r1"));
        c.so(&[SEM_COBERTURA]);
        assert_eq!(c.problemas()[0].n, Some(30));
        assert_eq!(c.problemas()[0].mensagem, "30 quadros sem região na ocultação");
        // ausência aceita cobre o buraco (intervalo fechado)
        c.p["rastreios"][0]["ausencias"] = json!([{ "de_s": t(30), "ate_s": t(59), "motivo": "fora_de_cena", "aceita_em": "x", "por": "y" }]);
        c.so(&[]);
        // ativo só depois da âncora
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[60])]);
        let mut o = ocultar("r1");
        o["ativo"] = json!({ "de": t(60), "ate": null });
        efeito(&mut c, o);
        c.so(&[]);
    }

    #[test]
    fn ocultar_cor_opaca_e_cor() {
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[30])]);
        let mut o = ocultar("r1");
        o["cor"] = json!("#00000080");
        efeito(&mut c, o);
        c.so(&[OCULTAR_COR_OPACA]);
        let mut c = cena();
        c.p["saida"]["cor_fundo"] = json!("preto");
        c.so(&[COR]);
    }

    #[test]
    fn legendas() {
        let legenda = |origem: &str, conferida: bool, texto: &str| json!({ "id": "l1", "texto": texto, "locutor": null, "origem": origem,
            "conferida": conferida, "locutor_auto": false, "ref": { "tipo": "fonte", "midia": "m1", "de": 1.6, "ate": 1.8 } });
        let mut c = cena();
        // sem som na saída: silenciar (Fase 1) não pesa, e a regra das legendas não depende do som
        c.p["saida"]["audio"] = json!(false);
        c.p["legendas"] = json!({ "estilo": estilo(), "lugar": { "x": 0.5, "y": 0.9, "h": "centro", "v": "base" }, "linhas_max": 2, "origem": null,
                                  "itens": [legenda("perito", true, " ")] });
        c.so(&[LEGENDA]);
        assert_eq!(c.problemas()[0].clipe.as_deref(), Some("l1"));
        // legenda vazia: com "nenhuma" é aviso; só barra quando as legendas saem (contratos (h) 4)
        assert_eq!(c.problemas()[0].nivel, Nivel::Aviso);
        // quando saem, o motor recusa até o P1.6 entregar (`nao_suportado`), ao lado das regras delas
        let ns_leg = |c: &Cena| ns_se(motor_legendas::recusa(&c.projeto(), &c.projeto().saida.exportacao).is_some());
        let das_legendas = |c: &Cena| c.problemas().iter().filter(|p| p.codigo != NAO_SUPORTADO).map(|p| (p.codigo, p.nivel)).collect::<Vec<_>>();
        c.p["saida"]["exportacao"]["legendas"] = json!("arquivo");
        assert_eq!(das_legendas(&c), vec![(LEGENDA, Nivel::Erro)]);
        let recusa: Vec<_> = c.problemas().into_iter().filter(|p| p.codigo == NAO_SUPORTADO).collect();
        let esperada = if ns_leg(&c).is_some() { vec![(Nivel::Erro, Some("legendas"), "Ainda não suportado: legendas")] } else { vec![] };
        assert_eq!(recusa.iter().map(|p| (p.nivel, p.campo.as_deref(), p.mensagem.as_str())).collect::<Vec<_>>(), esperada);
        c.p["legendas"]["itens"][0]["ref"]["ate"] = json!(1.6);
        assert_eq!(das_legendas(&c), vec![(LEGENDA, Nivel::Erro)]);
        c.p["saida"]["exportacao"]["legendas"] = json!("nenhuma");
        assert_eq!(c.problemas().iter().map(|p| (p.codigo, p.nivel)).collect::<Vec<_>>(), vec![(LEGENDA, Nivel::Aviso)]);
        // só valem quando as legendas saem
        c.p["legendas"]["itens"] = json!([legenda("ia", false, "Para!")]);
        c.so(&[]);
        c.p["saida"]["exportacao"]["legendas"] = json!("arquivo");
        c.so(&mais(&[LEGENDA_NAO_CONFERIDA], &[ns_leg(&c)]));
        c.p["legendas"]["itens"] = json!([legenda("perito", true, "Para!")]);
        c.so(&mais(&[], &[ns_leg(&c)]));
        // o silêncio só é recusado (até o P1.5 entregar) com som na saída; aqui não há
        let ns_sil = ns_se(c.p["saida"]["audio"].as_bool() == Some(true) && motor_audio::recusa("silenciar"));
        c.p["principal"][0]["audio"]["silencios"] = json!([{ "id": "s1", "de": 1.5, "ate": 2.0, "modo": "tom", "motivo": "nome" }]);
        c.so(&mais(&[LEGENDA_SOB_SILENCIO], &[ns_leg(&c), ns_sil]));
        // no tempo do programa: o silêncio de 1,5 a 2,0 na fonte cai em 0,5 a 1,0 no programa
        c.p["legendas"]["itens"][0]["ref"] = json!({ "tipo": "programa", "de": 0.9, "ate": 1.2 });
        c.so(&mais(&[LEGENDA_SOB_SILENCIO], &[ns_leg(&c), ns_sil]));
        c.p["legendas"]["itens"][0]["ref"] = json!({ "tipo": "programa", "de": 1.0, "ate": 1.2 });
        c.so(&mais(&[], &[ns_leg(&c), ns_sil]));
        // a 2× o silêncio cai em 0,25 a 0,5 s do programa
        c.p["principal"][0]["velocidade"]["fator"] = json!(2.0);
        c.p["legendas"]["itens"][0]["ref"] = json!({ "tipo": "programa", "de": 0.3, "ate": 0.4 });
        c.so(&mais(&[LEGENDA_SOB_SILENCIO], &[ns_leg(&c), ns_sil]));
        c.p["legendas"]["itens"][0]["ref"] = json!({ "tipo": "programa", "de": 0.5, "ate": 0.6 });
        c.so(&mais(&[], &[ns_leg(&c), ns_sil]));
        // sem itens não há o que sair: nada a recusar
        c.p["legendas"]["itens"] = json!([]);
        c.so(&[]);
    }

    fn estabilizar(de: i64, ate: i64) -> Value {
        com(base("estabilizar", "e5"), json!({ "suavizar": 10, "borda": "preta",
            "analise": { "de_s": t(de), "ate_s": t(ate), "de_pts": 512 * de, "ate_pts": 512 * ate }, "zoom_pct": 0.0, "trf_sha256": SHA2 }))
    }

    #[test]
    fn estabilizar_fora_da_analise() {
        let mut c = cena();
        efeito(&mut c, estabilizar(45, 75));
        // estabilizar é de origem: recusado até o P1.4 entregar (a regra da faixa segue conferida)
        c.so(&mais(&[ESTABILIZAR_FORA_DA_ANALISE], &[ns_efeito(&estabilizar(45, 75))]));
        c.p["principal"][0]["efeitos"] = json!([estabilizar(0, 89)]);
        c.so(&mais(&[], &[ns_efeito(&estabilizar(0, 89))]));
    }

    fn regua(regiao: Value) -> Value {
        com(base("regua", "e6"), json!({ "calibracao": { "id": "k1", "metodo": "plane", "h": [1, 0, 0, 0, 1, 0, 0, 0, 1],
            "espaco": { "w": 1280, "h": 720 }, "pontos_sha256": SHA2, "residuo_m": 0.01, "regiao": regiao },
            "ancora": { "x": 0.5, "y": 0.5 }, "direcao_graus": 0.0, "comprimento_m": 5.0, "divisoes": 5,
            "traco": { "cor": "#ffffff", "espessura": 0.004, "contorno": true }, "rotulo": true }))
    }

    #[test]
    fn regua_espaco_e_fora_da_calibracao() {
        let quadrado = json!([{ "x": 0.2, "y": 0.2 }, { "x": 0.8, "y": 0.2 }, { "x": 0.8, "y": 0.8 }, { "x": 0.2, "y": 0.8 }]);
        // régua é do P1.3: recusada até ele entregar
        let mut c = cena();
        efeito(&mut c, regua(quadrado.clone()));
        c.so(&mais(&[], &[ns_efeito(&regua(quadrado.clone()))]));
        efeito(&mut c, estabilizar(0, 89));
        c.so(&mais(&[REGUA_ESPACO], &[ns_efeito(&regua(quadrado.clone())), ns_efeito(&estabilizar(0, 89))]));
        let mut c = cena();
        let fora = json!([{ "x": 0.6, "y": 0.6 }, { "x": 0.9, "y": 0.6 }, { "x": 0.9, "y": 0.9 }]);
        efeito(&mut c, regua(fora.clone()));
        c.so(&mais(&[REGUA_FORA_DA_CALIBRACAO], &[ns_efeito(&regua(fora))]));
    }

    #[test]
    fn filtro_ausente() {
        let mut c = cena();
        c.cap.filtros.remove("unsharp");
        efeito(&mut c, com(base("nitidez", "e2"), json!({ "quantidade": 0.5 })));
        // efeito de origem: recusado até o P1.4 entregar (contratos (h) 1)
        c.so(&mais(&[FILTRO_AUSENTE], &[ns_tipo("nitidez")]));
        assert_eq!(c.problemas()[0].mensagem, "Filtro ausente no ffmpeg: unsharp");
        // estabilizar sem .trf pede também o vidstabdetect
        let mut c = cena();
        c.cap.filtros.remove("vidstabdetect");
        let mut e = estabilizar(0, 89);
        e["trf_sha256"] = json!(null);
        efeito(&mut c, e.clone());
        c.so(&mais(&[FILTRO_AUSENTE], &[ns_efeito(&e)]));
        c.p["principal"][0]["efeitos"][0]["trf_sha256"] = json!(SHA2);
        c.so(&mais(&[], &[ns_efeito(&estabilizar(0, 89))]));
        // enquadramento animado sem `perspective`: o filtro falta só quando o motor o trata como animado
        let mut c = cena();
        c.cap.filtros.remove("perspective");
        let regiao = |w: f64| json!([{ "t": t(30), "v": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 } }, { "t": t(80), "v": { "x": 0.0, "y": 0.0, "w": w, "h": w } }]);
        efeito(&mut c, com(base("enquadramento", "e2"), json!({ "regiao": regiao(0.5), "ajuste": "cobrir", "interpolacao": "bilinear" })));
        c.so(&[FILTRO_AUSENTE]);
        assert_eq!(c.problemas()[0].mensagem, "Filtro ausente no ffmpeg: perspective");
        // chaves iguais valem como região fixa (contratos (f) 5): sem `perspective`
        c.p["principal"][0]["efeitos"][0]["regiao"] = regiao(1.0);
        c.so(&[]);
    }

    #[test]
    fn audio() {
        let mut c = cena();
        c.p["principal"][0]["audio"]["volume"] = json!(2.5);
        c.so(&[AUDIO]);
        let mut c = cena();
        c.p["principal"][0]["audio"]["silencios"] = json!([{ "id": "s1", "de": 2.0, "ate": 1.5, "modo": "tom", "motivo": "" }]);
        // silenciar é do P1.5: recusado até ele entregar (contratos (h) 1)
        c.so(&mais(&[AUDIO], &[ns_se(motor_audio::recusa("silenciar"))]));
        // narração presa a clipe inexistente, recorte invertido
        let clipe_audio = |pos: Value, ate: Value| json!({ "id": "a1", "nome": "", "midia": "m1", "pos": pos, "de_s": 1.0, "ate_s": ate,
            "volume": 1.0, "entrada_s": 0.0, "saida_s": 0.0 });
        let trilha = |c: Value| json!([{ "id": "ta", "nome": "Narração", "papel": "narracao", "volume": 1.0, "mudo": false, "abaixa_original": false,
            "clipes": [c] }]);
        let mut c = cena();
        c.p["audios"] = trilha(clipe_audio(json!({ "ref": "clipe", "clipe": "c9", "desloc": 0.0 }), json!(null)));
        c.so(&[AUDIO]);
        c.p["audios"] = trilha(clipe_audio(json!({ "ref": "clipe", "clipe": "c1", "desloc": 0.0 }), json!(0.5)));
        c.so(&[AUDIO]);
        c.p["audios"] = trilha(clipe_audio(json!({ "ref": "clipe", "clipe": "c1", "desloc": 0.0 }), json!(null)));
        c.so(&[]);
        // volume animado sem chaves: estrutura, antes de qualquer curva
        c.p["audios"][0]["clipes"][0]["volume"] = json!([]);
        c.so(&[ITEM_INVALIDO]);
    }

    #[test]
    fn pixel_nao_suportado() {
        let mut c = cena();
        let mut f = fonte(tabela());
        f.pix_fmt = "nv12".into();
        f.faixa = Faixa::Pc;
        c.fontes.insert("m1".into(), f);
        c.so(&[PIXEL_NAO_SUPORTADO]);
        assert_eq!(c.problemas()[0].midia.as_deref(), Some("m1"));
    }

    #[test]
    fn nao_suportado() {
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[30])]);
        efeito(&mut c, com(base("lupa", "e7"), json!({ "rastreio": "r1", "janela": 0.2, "fator": 2.0, "forma": "circulo",
            "interpolacao": "bilinear", "onde": { "modo": "fixa", "x": 0.8, "y": 0.2 }, "moldura": { "cor": "#ffffff", "espessura": 0.004,
            "contorno": true }, "ligacao": true })));
        c.so(&mais(&[], &[ns_tipo("lupa")]));
        if ns_tipo("lupa").is_some() {
            assert_eq!(c.problemas()[0].mensagem, "Ainda não suportado: lupa");
        }
        let mut c = cena();
        c.p["sobreposicoes"] = json!([{ "id": "t1", "nome": "", "oculta": false, "clipes": [{ "tipo": "video", "id": "v1", "rotulo": "",
            "pos": { "ref": "programa", "t": 0.0 }, "duracao_s": 1.0, "transformacao": { "x": 0.5, "y": 0.5, "escala": 0.3, "rotacao": 0.0,
            "opacidade": 1.0 }, "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [], "midia": "m1", "inicio_s": 1.0, "inicio_pts": 15360,
            "velocidade": { "tipo": "constante", "fator": 1.0 }, "audio": null, "moldura": null, "sincronia": null }] }]);
        c.so(&[NAO_SUPORTADO]);
    }

    // ---- periciais

    #[test]
    fn taxa_variavel() {
        let mut c = cena();
        let pts: Vec<i64> = apoio::pts_regulares(300, (1, 30), TB, 0).into_iter().enumerate().filter(|(k, _)| !(50..56).contains(k)).map(|x| x.1).collect();
        c.fontes.insert("m1".into(), fonte(apoio::tabela(TB, pts)));
        c.so(&[TAXA_VARIAVEL]);
        assert_eq!(c.problemas()[0].n, Some(1));
        // a mídia inteira: 1 passo irregular em 293 (menos de 0,5 %), mas é lacuna
        for (k, v) in [("inicio_s", json!(0.0)), ("inicio_pts", json!(0)), ("fim_s", json!(10.0)), ("fim_pts", json!(null))] {
            c.p["principal"][0][k] = v;
        }
        c.so(&[TAXA_VARIAVEL]);
        // oscilação de ±2 ms em 30 qps (passos de 29 a 35 ms) fica dentro de ±25 % da mediana: constante com jitter
        let mut c = cena();
        let tb = (1, 1000);
        let pts: Vec<i64> = (0..300).map(|k: i64| (k * 1000 + 15) / 30 + [0, 2, -2][k as usize % 3]).collect();
        c.fontes.insert("m1".into(), fonte(apoio::tabela(tb, pts)));
        c.p["principal"][0]["inicio_pts"] = json!(null);
        c.p["principal"][0]["fim_pts"] = json!(null);
        assert!(!c.codigos().contains(TAXA_VARIAVEL), "{:?}", c.codigos());
    }

    #[test]
    fn quadros_descartados() {
        // 30 qps num programa de 25 a 1×: descarta (e a taxa difere)
        let mut c = cena();
        c.p["saida"]["fps"] = json!({ "num": 25, "den": 1 });
        c.so(&[QUADROS_DESCARTADOS, FPS_DIFERENTE]);
        assert_eq!(c.problemas()[0].n, Some(10));
        // a 2× o descarte é a própria aceleração
        c.p["principal"][0]["velocidade"]["fator"] = json!(2.0);
        c.so(&[FPS_DIFERENTE]);
    }

    #[test]
    fn fonte_danificada_derivada_e_imagem_ia() {
        let mut c = cena();
        c.p["midias"][0]["decodificacao"]["erros"] = json!(3);
        c.so(&[FONTE_DANIFICADA]);
        assert_eq!(c.problemas()[0].mensagem, "Gravação danificada: 3 erros");
        // pela tabela do decodificador também (avisos de pacote corrompido)
        let mut c = cena();
        let mut tab = tabela();
        tab.erros_decodificacao = 2;
        c.fontes.insert("m1".into(), fonte(tab));
        c.so(&[FONTE_DANIFICADA]);
        let mut c = cena();
        c.p["midias"][0]["derivacao"] = json!({ "tipo": "estabilizado", "de_sha256": SHA2, "raiz_sha256": SHA2, "comando": "ffmpeg", "com_perdas": true });
        c.so(&[MIDIA_DERIVADA_COM_PERDAS]);
        let mut c = cena();
        let mut m2 = midia("m2", "imagem", SHA2);
        m2["derivacao"] = json!({ "tipo": "superres", "de_sha256": SHA, "raiz_sha256": SHA, "comando": "", "com_perdas": false });
        c.p["midias"].as_array_mut().unwrap().push(m2);
        c.p["principal"].as_array_mut().unwrap().push(json!({ "tipo": "imagem", "id": "c2", "rotulo": "", "desligado": false,
            "transicao_entrada": null, "efeitos": [], "midia": "m2", "duracao_s": 2.0 }));
        c.fontes.insert("m2".into(), imagem());
        c.midias.push(estado("m2", SituacaoMidia::Ok));
        c.so(&[IMAGEM_IA]);
    }

    #[test]
    fn ocultacao_reversivel_e_fraca() {
        let caso = |modo: &str, intensidade: f64, conteudo: &str| {
            let mut c = cena();
            c.p["rastreios"] = json!([rastreio(&[30])]);
            let mut o = ocultar("r1");
            o["modo"] = json!(modo);
            o["intensidade"] = json!(intensidade);
            o["conteudo"] = json!(conteudo);
            efeito(&mut c, o);
            c
        };
        caso("mosaico", 1.0, "placa").so(&[OCULTACAO_REVERSIVEL]);
        caso("desfoque", 1.0, "documento").so(&[OCULTACAO_REVERSIVEL]);
        caso("solido", 1.0, "tela").so(&[]);
        caso("desfoque", 0.5, "rosto").so(&[OCULTACAO_FRACA]);
        caso("desfoque", 0.6, "rosto").so(&[]);
        // mosaico: round(12 − 11·i) células; 0,5 → 7 (fraca), 0,55 → 6
        caso("mosaico", 0.5, "rosto").so(&[OCULTACAO_FRACA]);
        caso("mosaico", 0.55, "rosto").so(&[]);
    }

    #[test]
    fn ocultar_interpolado_e_silhueta() {
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[30, 60])]);
        efeito(&mut c, ocultar("r1"));
        c.so(&[OCULTAR_INTERPOLADO]);
        assert_eq!(c.problemas()[0].n, Some(29));
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[30])]);
        let mut o = ocultar("r1");
        o["alvo"]["forma"] = json!("silhueta");
        efeito(&mut c, o);
        c.so(&[SILHUETA_EM_OCULTAR, NAO_SUPORTADO]);
    }

    #[test]
    fn ocultacao_da_midia_vale_em_todo_uso() {
        // ocultação da mídia com o rastreio começando no quadro 60: o trecho e o quadro congelado no 40 ficam sem região
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[60])]);
        let mut o = ocultar("r1");
        o["ativo"] = json!({ "de": 0.0, "ate": 10.0 });
        c.p["midias"][0]["ocultacoes"] = json!([o]);
        c.p["principal"].as_array_mut().unwrap().push(json!({ "tipo": "congelar", "id": "c2", "rotulo": "", "desligado": false,
            "transicao_entrada": null, "efeitos": [], "midia": "m1", "instante_s": t(40), "instante_pts": 512 * 40, "duracao_s": 1.0 }));
        let p = c.problemas();
        let sem: Vec<_> = p.iter().filter(|p| p.codigo == SEM_COBERTURA).map(|p| (p.clipe.as_deref(), p.midia.as_deref(), p.efeito.as_deref(), p.n)).collect();
        assert_eq!(sem, vec![(Some("c1"), Some("m1"), Some("e1"), Some(30)), (Some("c2"), Some("m1"), Some("e1"), Some(1))]);
        c.so(&[SEM_COBERTURA]);
        // problema estático da ocultação aparece uma vez, preso à mídia
        c.p["midias"][0]["ocultacoes"][0]["cor"] = json!("#000000");
        c.p["rastreios"] = json!([rastreio(&[0])]);
        let p = c.problemas();
        assert_eq!(p.iter().map(|p| (p.codigo, p.clipe.as_deref(), p.midia.as_deref())).collect::<Vec<_>>(), vec![]);
        c.p["midias"][0]["ocultacoes"][0]["cor"] = json!("#00000000");
        let p = c.problemas();
        assert_eq!(p.iter().map(|p| (p.codigo, p.clipe.as_deref(), p.midia.as_deref())).collect::<Vec<_>>(), vec![(OCULTAR_COR_OPACA, None, Some("m1"))]);
    }

    #[test]
    fn media_de_quadros() {
        let media = || com(base("media_temporal", "e2"), json!({ "quadros": 4 }));
        let mut c = cena();
        efeito(&mut c, media());
        c.so(&mais(&[MEDIA_DE_QUADROS], &[ns_tipo("media_temporal")]));
        let p = c.problemas().into_iter().find(|p| p.codigo == MEDIA_DE_QUADROS).unwrap();
        assert_eq!((p.nivel, p.clipe.as_deref(), p.efeito.as_deref()), (Nivel::Pericial, Some("c1"), Some("e2")));
        // quadro congelado: a imagem parada também é média (centrada no quadro); pede ciência do mesmo jeito
        let mut c = cena();
        c.p["principal"] = json!([{ "tipo": "congelar", "id": "c1", "rotulo": "", "desligado": false, "transicao_entrada": null, "efeitos": [media()],
            "midia": "m1", "instante_s": t(40), "instante_pts": 512 * 40, "duracao_s": 1.0 }]);
        c.so(&mais(&[MEDIA_DE_QUADROS], &[ns_tipo("media_temporal")]));
        assert_eq!(sem_ciencia(&c.problemas(), &[]), vec![MEDIA_DE_QUADROS]);
        // título não tem gravação: nenhuma média a declarar (o efeito é recusado ali)
        let mut c = cena();
        let mut tit = titulo("c2", 2.0);
        tit["efeitos"] = json!([media()]);
        c.p["principal"].as_array_mut().unwrap().push(tit);
        let p = c.problemas();
        assert!(!p.iter().any(|p| p.codigo == MEDIA_DE_QUADROS), "{p:#?}");
        assert!(p.iter().any(|p| p.codigo == NAO_SUPORTADO && p.clipe.as_deref() == Some("c2")), "{p:#?}");
        // desligada não acusa
        let mut c = cena();
        let mut e = media();
        e["ligado"] = json!(false);
        efeito(&mut c, e);
        c.so(&[]);
    }

    #[test]
    fn sincronias() {
        let mut c = cena();
        c.p["principal"] = json!([tela()]);
        c.p["sincronias"] = json!([sincronia("audio", json!([]), 10.0)]);
        c.so(&[NAO_SUPORTADO, SINCRONIA_SEM_CONFERENCIA]);
        c.p["sincronias"] = json!([sincronia("audio", json!([{ "modo": "sem_evento", "por": "P", "em": "x" }]), 10.0)]);
        c.so(&[NAO_SUPORTADO]);
        c.p["sincronias"] = json!([sincronia("evento", json!([]), 2.0)]);
        c.so(&[NAO_SUPORTADO, SINCRONIA_EXTRAPOLADA]);
    }

    #[test]
    fn relogio_distante() {
        let carimbo = |media_time: f64| com(base("carimbo", "e3"), json!({ "mostra": "relogio", "formato": "data_hora",
            "relogio": { "media_time_s": media_time, "clock_seconds": 43200.0, "clock_date": "2026-11-01" }, "estilo": estilo(),
            "lugar": { "x": 0.9, "y": 0.1, "h": "dir", "v": "topo" } }));
        let mut c = cena();
        efeito(&mut c, carimbo(1000.0));
        c.so(&[RELOGIO_DISTANTE]);
        c.p["principal"][0]["efeitos"] = json!([carimbo(500.0)]);
        c.so(&[]);
        c.p["principal"][0]["efeitos"][0]["relogio"] = json!(null);
        c.so(&[ITEM_INVALIDO]);
    }

    // ---- avisos

    #[test]
    fn fps_diferente_ampliacao_10bits_entrelacado() {
        let mut c = cena();
        c.p["saida"]["fps"] = json!({ "num": 60, "den": 1 });
        c.so(&[FPS_DIFERENTE]);
        assert_eq!(c.problemas()[0].n, Some(60), "60 repetidos");
        let mut c = cena();
        efeito(&mut c, com(base("enquadramento", "e2"), json!({ "regiao": { "x": 0.4, "y": 0.4, "w": 0.2, "h": 0.2 }, "ajuste": "conter",
            "interpolacao": "bilinear" })));
        c.so(&[AMPLIACAO_ALTA]);
        c.p["principal"][0]["efeitos"][0]["regiao"] = json!({ "x": 0.3, "y": 0.3, "w": 0.4, "h": 0.4 });
        c.so(&[]);
        let mut c = cena();
        let mut f = fonte(tabela());
        f.pix_fmt = "yuv420p10le".into();
        c.fontes.insert("m1".into(), f);
        c.so(&[FONTE_10BITS]);
        let mut c = cena();
        let mut f = fonte(tabela());
        f.entrelacado = true;
        c.fontes.insert("m1".into(), f);
        c.so(&[ENTRELACADO]);
        efeito(&mut c, com(base("desentrelacar", "e2"), json!({ "modo": "quadro", "ordem": "auto" })));
        // o aviso sai; o efeito é de origem, recusado até o P1.4 entregar
        c.so(&mais(&[], &[ns_efeito(&com(base("desentrelacar", "e2"), json!({ "modo": "quadro", "ordem": "auto" })))]));
    }

    #[test]
    fn texto_fora_e_selo_desligado() {
        let mut c = cena();
        efeito(&mut c, com(base("texto", "e2"), json!({ "conteudo": "x", "estilo": estilo(),
            "lugar": { "x": 0.5, "y": 0.99, "h": "centro", "v": "base" }, "largura_max": 0.8 })));
        c.so(&[TEXTO_FORA]);
        let mut c = cena();
        c.p["saida"]["selos"]["realce"] = json!(false);
        c.so(&[SELO_DESLIGADO]);
        assert_eq!(c.problemas()[0].mensagem, "1 selo desligado");
        // texto acima do teto do rasterizador
        let mut c = cena();
        efeito(&mut c, com(base("texto", "e2"), json!({ "conteudo": "a".repeat(LIMITE_TEXTO + 1), "estilo": estilo(),
            "lugar": { "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" }, "largura_max": 0.8 })));
        c.so(&[ITEM_INVALIDO]);
    }

    #[test]
    fn quadro_ajustado() {
        let mut c = cena();
        c.p["principal"][0]["inicio_pts"] = json!(512 * 30 + 1);
        c.so(&[QUADRO_AJUSTADO]);
        let a = c.analise();
        assert_eq!(a.prender.len(), 1);
        assert_eq!(a.prender[0].pts, Some(512 * 30));
        // pts que faltavam não contam como ajuste
        let mut c = cena();
        c.p["principal"][0]["inicio_pts"] = json!(null);
        c.p["principal"][0]["fim_pts"] = json!(null);
        c.so(&[]);
        assert_eq!(c.analise().prender.len(), 2);
    }

    // ---- estrutura e regras de apoio

    #[test]
    fn estrutura_antes_da_curva() {
        // nenhuma destas entra em pânico: listas vazias e fora de ordem são acusadas antes de avaliar
        let mut c = cena();
        c.p["principal"][0]["audio"]["volume"] = json!([]);
        c.so(&[ITEM_INVALIDO]);
        let mut c = cena();
        c.p["principal"][0]["audio"]["volume"] = json!([{ "t": 2.0, "v": 1.0 }, { "t": 1.0, "v": 1.0 }]);
        c.so(&[ITEM_INVALIDO]);
        let mut c = cena();
        c.p["sobreposicoes"] = json!([{ "id": "t1", "nome": "", "oculta": false, "clipes": [{ "tipo": "titulo", "id": "x1", "rotulo": "",
            "pos": { "ref": "clipe", "clipe": "c1", "desloc": 0.0 }, "duracao_s": null, "transformacao": { "x": [], "y": 0.5, "escala": 1.0,
            "rotacao": 0.0, "opacidade": 1.0 }, "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [],
            "titulo": { "modelo": "terco_inferior", "fundo": { "tipo": "transparente" }, "blocos": [] } }] }]);
        c.so(&[ITEM_INVALIDO]);
        assert_eq!(c.problemas()[0].campo.as_deref(), Some("transformacao"));
        // preso a clipe que não está na linha
        c.p["sobreposicoes"][0]["clipes"][0]["transformacao"]["x"] = json!(0.5);
        c.so(&[]);
        c.p["sobreposicoes"][0]["clipes"][0]["pos"]["clipe"] = json!("c9");
        c.so(&[INTERVALO]);
    }

    #[test]
    fn tem_erro_e_sem_ciencia() {
        let mut c = cena();
        c.p["midias"][0]["decodificacao"]["erros"] = json!(3);
        c.p["saida"]["fps"] = json!({ "num": 25, "den": 1 });
        let p = c.problemas();
        assert!(!tem_erro(&p));
        assert_eq!(sem_ciencia(&p, &[]), vec![QUADROS_DESCARTADOS, FONTE_DANIFICADA]);
        assert_eq!(sem_ciencia(&p, &[FONTE_DANIFICADA.to_string()]), vec![QUADROS_DESCARTADOS]);
        c.p["saida"]["largura"] = json!(3);
        assert!(tem_erro(&c.problemas()));
    }

    /// Projeto importado do v1 chega com os pts nulos: a análise os devolve presos à tabela, sem "ajustado".
    #[test]
    fn analise_de_projeto_importado_prende_os_pts() {
        struct Fontes;
        impl FontesV1 for Fontes {
            fn video(&self, media_id: &str) -> Option<MidiaRef> {
                Some(MidiaRef {
                    id: String::new(), tipo: TipoMidia::Video, origem: OrigemMidia::Caso, media_id: Some(media_id.into()), sha256: SHA.into(),
                    nome: "cam.mp4".into(), relativo: "Mídias/cam.mp4".into(), duracao_s: Some(10.0), inicio_s: Some(0.0), largura: Some(1280),
                    altura: Some(720), taxa: None, vfr: false, tem_audio: false, cor: None, derivacao: None,
                    recebido: Recebido { nativo: None, obs: String::new() }, relogio_desvio_s: None, gravador: None, decodificacao: None,
                    ocultacoes: Vec::new(),
                })
            }
            fn narracao(&self, _: &str) -> Option<MidiaRef> {
                None
            }
            fn pts(&self, _: &str, _: f64) -> Option<i64> {
                None
            }
            fn tamanho(&self, _: &str) -> Option<(u32, u32)> {
                Some((1280, 720))
            }
        }
        let texto = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edicao/v1-efeitos.json")).unwrap();
        let p = importar_v1::converter(&serde_json::from_str(&texto).unwrap(), &Fontes, "2026-11-14T00:00:00Z", "6.0.0").unwrap();
        let mut c = cena();
        c.p = serde_json::to_value(&p).unwrap();
        let id = p.midias[0].id.clone();
        c.fontes = HashMap::from([(id.clone(), fonte(tabela()))]);
        c.midias = vec![estado(&id, SituacaoMidia::Ok)];
        let a = c.analise();
        let c1 = match &p.principal[0] {
            Tolerante::Conhecido(c) => base_clipe(c).id.clone(),
            _ => panic!(),
        };
        let onde: Vec<_> = a.prender.iter().map(|x| (x.onde.clone(), x.pts)).collect();
        assert!(onde.contains(&(OndePrender::Inicio { clipe: c1.clone() }, Some(0))), "{onde:#?}");
        assert!(onde.contains(&(OndePrender::Fim { clipe: c1.clone() }, Some(512 * 120))), "{onde:#?}");
        let ancoras = a.prender.iter().filter(|x| matches!(x.onde, OndePrender::Ancora { .. })).count();
        let total: usize = p.rastreios.iter().map(|r| r.ancoras.len()).sum();
        assert!(total > 0 && ancoras == total, "{ancoras} de {total}");
        assert!(a.prender.iter().all(|x| x.pts.is_some_and(|v| v % 512 == 0)));
        assert!(!a.problemas.iter().any(|p| p.codigo == QUADRO_AJUSTADO), "{:#?}", a.problemas);
        // nada de pânico nem item inválido no que o importador gera
        assert!(!a.problemas.iter().any(|p| p.codigo == ITEM_INVALIDO || p.codigo == REGIAO || p.codigo == ESQUEMA), "{:#?}", a.problemas);
    }

    // ---- revisão adversa

    /// O motor testa o instante do quadro em intervalo fechado: o quadro que começa exatamente em `ate` é ocultado
    /// e tem de ter região.
    #[test]
    fn ativo_fechado_no_fim() {
        let mut c = cena();
        let mut r = rastreio(&(30..60).collect::<Vec<_>>());
        r["origem"] = json!({ "tipo": "assistido" });
        c.p["rastreios"] = json!([r]);
        let mut o = ocultar("r1");
        o["ativo"] = json!({ "de": t(30), "ate": t(60) });
        efeito(&mut c, o);
        c.so(&[SEM_COBERTURA]);
        assert_eq!(c.problemas()[0].n, Some(1));
        // um quadro antes: nada descoberto
        c.p["principal"][0]["efeitos"][0]["ativo"]["ate"] = json!(t(59));
        c.so(&[]);
        // `de` no meio da duração de um quadro: o motor testa o instante do quadro (`efeitos::base_ativa`) e esse
        // quadro não conta; a validação usa a mesma regra (contratos (h) 3)
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[60])]);
        let mut o = ocultar("r1");
        o["ativo"] = json!({ "de": t(59) + 0.01, "ate": null });
        efeito(&mut c, o);
        c.so(&[]);
        c.p["principal"][0]["efeitos"][0]["ativo"]["de"] = json!(t(59));
        c.so(&[SEM_COBERTURA]);
        assert_eq!(c.problemas()[0].n, Some(1));
    }

    /// A lacuna entre o último quadro do trecho e o seguinte é tempo do trecho (o último quadro fica parado nela).
    #[test]
    fn taxa_variavel_na_lacuna_do_fim_do_trecho() {
        let mut c = cena();
        let pts: Vec<i64> = apoio::pts_regulares(300, (1, 30), TB, 0).into_iter().enumerate().filter(|(k, _)| !(50..56).contains(k)).map(|x| x.1).collect();
        c.fontes.insert("m1".into(), fonte(apoio::tabela(TB, pts)));
        // quadros 30..49 e o fim no primeiro quadro depois da lacuna
        c.p["principal"][0]["fim_pts"] = json!(512 * 56);
        c.p["principal"][0]["fim_s"] = json!(t(56));
        assert!(c.codigos().contains(TAXA_VARIAVEL), "{:?}", c.codigos());
        // sem a lacuna no caminho: nada
        c.p["principal"][0]["fim_pts"] = json!(512 * 49);
        c.p["principal"][0]["fim_s"] = json!(t(49));
        c.so(&[]);
    }

    fn clipe_imagem(id: &str) -> Value {
        json!({ "tipo": "imagem", "id": id, "rotulo": "", "desligado": false, "transicao_entrada": null, "efeitos": [], "midia": "m2", "duracao_s": 2.0 })
    }

    /// Ocultação da mídia numa foto: o motor não tem quadro lido onde aplicar; recusada, nunca ignorada.
    #[test]
    fn ocultacao_de_imagem_e_recusada() {
        let mut c = cena();
        let mut m2 = midia("m2", "imagem", SHA2);
        m2["ocultacoes"] = json!([ocultar("r2")]);
        c.p["midias"].as_array_mut().unwrap().push(m2);
        c.p["principal"].as_array_mut().unwrap().push(clipe_imagem("c2"));
        c.fontes.insert("m2".into(), imagem());
        c.midias.push(estado("m2", SituacaoMidia::Ok));
        c.so(&[NAO_SUPORTADO]);
        let p = c.problemas();
        assert_eq!((p[0].clipe.as_deref(), p[0].midia.as_deref(), p[0].mensagem.as_str()), (Some("c2"), Some("m2"), "Ainda não suportado: ocultação em imagem"));
        // desligada não conta
        c.p["midias"][1]["ocultacoes"][0]["ligado"] = json!(false);
        c.so(&[]);
        // como imagem sobreposta e como efeito de imagem
        c.p["midias"][1]["ocultacoes"][0]["ligado"] = json!(true);
        c.p["principal"].as_array_mut().unwrap().pop();
        efeito(&mut c, com(base("imagem", "e8"), json!({ "midia": "m2", "lugar": { "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" }, "largura": 0.2, "opacidade": 1.0 })));
        c.so(&[NAO_SUPORTADO]);
    }

    /// Mídia dada como presente mas sem `FonteInfo`: não há o que renderizar.
    #[test]
    fn midia_sem_fonte_e_ausente() {
        let mut c = cena();
        c.fontes.clear();
        c.so(&[MIDIA_AUSENTE]);
    }

    /// Sobreposto "até o fim do programa" com mais de 1 h: os efeitos dele continuam conferidos.
    #[test]
    fn sobreposto_longo_continua_conferido() {
        let mut c = cena();
        c.p["principal"] = json!([titulo("c1", 3600.0), titulo("c2", 3600.0)]);
        let mut r = json!({ "id": "r2", "nome": "", "cor": "#ffcc00", "midia": null, "base": "local", "tipo": "caixa", "espaco": "original",
            "ancoras": [{ "t": 5.0, "pts": null, "v": { "x": 0.1, "y": 0.1, "w": 0.2, "h": 0.2 }, "interp": "linear" }], "denso": null,
            "silhuetas": null, "ausencias": [], "origem": { "tipo": "deteccao", "classe": "rosto" }, "confirmacao": null });
        c.p["rastreios"] = json!([r.clone()]);
        let mut d = destaque("r2");
        d["forma"] = json!("retangulo");
        c.p["sobreposicoes"] = json!([{ "id": "t1", "nome": "", "oculta": false, "clipes": [{ "tipo": "titulo", "id": "x1", "rotulo": "",
            "pos": { "ref": "programa", "t": 0.0 }, "duracao_s": null, "transformacao": { "x": 0.5, "y": 0.5, "escala": 1.0,
            "rotacao": 0.0, "opacidade": 1.0 }, "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [d],
            "titulo": { "modelo": "terco_inferior", "fundo": { "tipo": "transparente" }, "blocos": [] } }] }]);
        let inicio = std::time::Instant::now();
        // efeito em sobreposto: a Fase 0 recusa, mas a conferência dele segue (o problema aparece igual)
        c.so(&[NAO_SUPORTADO, RASTREIO_NAO_CONFIRMADO]);
        assert!(inicio.elapsed().as_secs() < LIMITE_RAPIDO_S * 3, "{:?}", inicio.elapsed());
        r["origem"] = json!({ "tipo": "manual" });
        c.p["rastreios"] = json!([r]);
        c.so(&[NAO_SUPORTADO]);
    }

    /// Todo id é `[A-Za-z0-9_-]{1,80}` (vira rótulo de grafo e nome de arquivo) e não se repete onde é chave de busca.
    #[test]
    fn ids_do_documento() {
        let mut c = cena();
        c.p["principal"][0]["id"] = json!("a/b");
        c.so(&[ESQUEMA]);
        assert_eq!((c.problemas()[0].clipe.as_deref(), c.problemas()[0].campo.as_deref()), (Some("a/b"), Some("id")));
        let mut c = cena();
        efeito(&mut c, com(base("girar", "e 2"), json!({ "graus": 0, "espelhar_h": false, "espelhar_v": false })));
        c.so(&[ESQUEMA]);
        let mut c = cena();
        let mut r = rastreio(&[30]);
        r["id"] = json!("../fora");
        c.p["rastreios"] = json!([r]);
        c.so(&[ESQUEMA]);
        let mut c = cena();
        c.p["midias"][0]["ocultacoes"] = json!([com(ocultar("r1"), json!({ "id": "" }))]);
        c.p["rastreios"] = json!([rastreio(&[0])]);
        c.so(&[ESQUEMA]);
        // repetidos: dois clipes, duas mídias, dois rastreios com o mesmo id
        let mut c = cena();
        c.p["principal"].as_array_mut().unwrap().push(trecho("c1"));
        c.so(&[ESQUEMA]);
        let mut c = cena();
        c.p["midias"].as_array_mut().unwrap().push(midia("m1", "video", SHA2));
        c.so(&[ESQUEMA]);
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[30]), rastreio(&[60])]);
        c.so(&[ESQUEMA]);
    }

    /// Limites de cima da saída, altura ímpar e as outras portas das regras com mais de uma.
    #[test]
    fn bordas_das_regras() {
        for (campo, v) in [("largura", json!(7682)), ("altura", json!(4322)), ("altura", json!(721))] {
            let mut c = cena();
            c.p["saida"][campo] = v;
            c.so(&[SAIDA]);
        }
        let mut c = cena();
        c.p["saida"]["largura"] = json!(7680);
        c.p["saida"]["altura"] = json!(4320);
        c.p["saida"]["exportacao"]["predefinicao"] = json!("preservacao");
        assert!(!c.codigos().contains(SAIDA), "{:?}", c.codigos());
        // ocultação reversível: tela também, em mosaico
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[30])]);
        efeito(&mut c, com(ocultar("r1"), json!({ "modo": "mosaico", "conteudo": "tela" })));
        c.so(&[OCULTACAO_REVERSIVEL]);
        // enquadramento sem altura
        let mut c = cena();
        efeito(&mut c, com(base("enquadramento", "e2"), json!({ "regiao": { "x": 0.1, "y": 0.1, "w": 0.5, "h": 0.0 }, "ajuste": "conter",
            "interpolacao": "bilinear" })));
        c.so(&[REGIAO]);
        // estabilização: a análise tem de alcançar o último quadro do trecho (89) e o primeiro (30)
        let mut c = cena();
        efeito(&mut c, estabilizar(30, 88));
        let ns = ns_efeito(&estabilizar(30, 89));
        c.so(&mais(&[ESTABILIZAR_FORA_DA_ANALISE], &[ns]));
        c.p["principal"][0]["efeitos"] = json!([estabilizar(31, 89)]);
        c.so(&mais(&[ESTABILIZAR_FORA_DA_ANALISE], &[ns]));
        c.p["principal"][0]["efeitos"] = json!([estabilizar(30, 89)]);
        c.so(&mais(&[], &[ns]));
        // confirmação presa a outro denso
        let mut c = cena();
        let mut r = rastreio(&[30, 89]);
        r["origem"] = json!({ "tipo": "deteccao", "classe": "rosto" });
        r["denso"] = json!({ "sha256": SHA, "n": 60, "de_s": 1.0, "ate_s": 3.0, "ok": 60, "incertos": 0, "ausentes": 0 });
        r["confirmacao"] = json!({ "por": "Perito", "em": "2026-11-14T00:00:00Z", "denso_sha256": SHA2, "de_s": 1.0, "ate_s": 3.0 });
        c.p["rastreios"] = json!([r]);
        efeito(&mut c, destaque("r1"));
        c.so(&[RASTREIO_NAO_CONFIRMADO]);
        c.p["rastreios"][0]["confirmacao"]["denso_sha256"] = json!(SHA);
        c.so(&[]);
        // confirmação que começa depois do uso
        c.p["rastreios"][0]["confirmacao"]["de_s"] = json!(1.5);
        c.so(&[RASTREIO_NAO_CONFIRMADO]);
        // sincronia usada antes do intervalo medido
        let mut c = cena();
        c.p["principal"] = json!([tela()]);
        let mut s = sincronia("evento", json!([]), 10.0);
        s["valido_de_s"] = json!(2.0);
        c.p["sincronias"] = json!([s]);
        c.so(&[NAO_SUPORTADO, SINCRONIA_EXTRAPOLADA]);
        // erros de decodificação vindos do estado da mídia
        let mut c = cena();
        c.midias[0].erros = Some(4);
        c.so(&[FONTE_DANIFICADA]);
        assert_eq!(c.problemas()[0].n, Some(4));
        // volume da trilha e do original
        let mut c = cena();
        c.p["audios"] = json!([{ "id": "ta", "nome": "", "papel": "musica", "volume": 2.5, "mudo": false, "abaixa_original": false, "clipes": [] }]);
        c.so(&[AUDIO]);
        let mut c = cena();
        c.p["mixagem"] = json!({ "volume_original": -0.1, "abaixar": { "db": -12.0, "ataque_s": 0.3, "soltura_s": 0.5 }, "normalizar": null, "limitador": false });
        c.so(&[AUDIO]);
        // legenda com fim antes do início; congelar e imagem com duração inválida
        let mut c = cena();
        c.p["legendas"] = json!({ "estilo": estilo(), "lugar": { "x": 0.5, "y": 0.9, "h": "centro", "v": "base" }, "linhas_max": 2, "origem": null,
            "itens": [{ "id": "l1", "texto": "Para!", "locutor": null, "origem": "perito", "conferida": true, "locutor_auto": false,
                        "ref": { "tipo": "programa", "de": 1.0, "ate": 1.0 } }] });
        c.so(&[LEGENDA]);
        let mut c = cena();
        c.p["principal"].as_array_mut().unwrap().push(json!({ "tipo": "congelar", "id": "c2", "rotulo": "", "desligado": false,
            "transicao_entrada": null, "efeitos": [], "midia": "m1", "instante_s": t(40), "instante_pts": 512 * 40, "duracao_s": 0.0 }));
        c.so(&[DURACAO]);
        // âncora com pts fora da tabela: movida para o quadro exibido
        let mut c = cena();
        let mut r = rastreio(&[30]);
        r["ancoras"][0]["pts"] = json!(512 * 30 + 3);
        c.p["rastreios"] = json!([r]);
        efeito(&mut c, ocultar("r1"));
        c.so(&[QUADRO_AJUSTADO]);
        assert_eq!(c.problemas()[0].campo.as_deref(), Some("ancoras"));
    }

    /// PNG chega do ffprobe como `rgb24` de faixa cheia: a foto entra pelo rasterizador, não pela cadeia de cor da fonte.
    #[test]
    fn imagem_png_nao_e_pixel_nao_suportado() {
        let mut c = cena();
        c.p["midias"].as_array_mut().unwrap().push(midia("m2", "imagem", SHA2));
        c.p["principal"].as_array_mut().unwrap().push(clipe_imagem("c2"));
        efeito(&mut c, com(base("imagem", "e8"), json!({ "midia": "m2", "lugar": { "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" }, "largura": 0.2, "opacidade": 1.0 })));
        let mut f = imagem();
        f.faixa = Faixa::Pc;
        assert!(cor::cadeia_fonte(f.matriz, f.faixa, &f.pix_fmt, f.exibido).is_none());
        c.fontes.insert("m2".into(), f);
        c.midias.push(estado("m2", SituacaoMidia::Ok));
        c.so(&[]);
        // lida como vídeo (quadro congelado), a mesma fonte é recusada
        c.p["principal"].as_array_mut().unwrap().push(json!({ "tipo": "congelar", "id": "c3", "rotulo": "", "desligado": false,
            "transicao_entrada": null, "efeitos": [], "midia": "m2", "instante_s": 0.0, "instante_pts": 0, "duracao_s": 1.0 }));
        c.so(&[PIXEL_NAO_SUPORTADO]);
    }

    /// Documento torto (mas que desserializa) nunca derruba a análise: sai problema, não pânico.
    #[test]
    fn documento_torto_nao_entra_em_panico() {
        let enq_animado = com(base("enquadramento", "e2"), json!({ "regiao": [
            { "t": t(30), "pts": null, "v": { "x": 0.0, "y": 0.0, "w": 0.0, "h": 0.0 }, "interp": "suave" },
            { "t": t(30), "pts": null, "v": { "x": 0.5, "y": 0.5, "w": 1e300, "h": -1.0 }, "interp": "degrau" } ],
            "ajuste": "cobrir", "interpolacao": "bicubica" }));
        type Mexe = Box<dyn Fn(&mut Cena)>;
        let tortos: Vec<(&str, Mexe)> = vec![
            ("fps zero", Box::new(|c| c.p["saida"]["fps"] = json!({ "num": 0, "den": 0 }))),
            ("fps negativo", Box::new(|c| c.p["saida"]["fps"] = json!({ "num": -30, "den": 1 }))),
            ("fps enorme", Box::new(|c| c.p["saida"]["fps"] = json!({ "num": i64::MAX, "den": 1 }))),
            ("fps mínimo", Box::new(|c| c.p["saida"]["fps"] = json!({ "num": 1, "den": i64::MAX }))),
            ("saída zero", Box::new(|c| { c.p["saida"]["largura"] = json!(0); c.p["saida"]["altura"] = json!(0); })),
            ("trecho invertido e enorme", Box::new(|c| { c.p["principal"][0]["inicio_s"] = json!(1e300); c.p["principal"][0]["fim_s"] = json!(-1e300);
                c.p["principal"][0]["inicio_pts"] = json!(i64::MAX); c.p["principal"][0]["fim_pts"] = json!(i64::MIN); })),
            ("giro torto com zoom animado", {
                let e = enq_animado.clone();
                Box::new(move |c| { efeito(c, com(base("girar", "e1"), json!({ "graus": 45, "espelhar_h": true, "espelhar_v": true }))); efeito(c, e.clone()); })
            }),
            ("tabela de um quadro", Box::new(|c| drop(c.fontes.insert("m1".into(), fonte(apoio::tabela(TB, vec![0])))))),
            ("pts repetidos", Box::new(|c| drop(c.fontes.insert("m1".into(), fonte(apoio::tabela(TB, vec![512 * 30; 80])))))),
            ("tabela vazia", Box::new(|c| drop(c.fontes.insert("m1".into(), fonte(apoio::tabela(TB, vec![])))))),
            ("fonte sem tamanho", Box::new(|c| { let mut f = fonte(tabela()); f.exibido = (0, 0); c.fontes.insert("m1".into(), f); c.p["saida"]["largura"] = json!(2); })),
            ("rastreio sem âncora, com denso", Box::new(|c| { let mut r = rastreio(&[]);
                r["denso"] = json!({ "sha256": "../x", "n": 1, "de_s": 0.0, "ate_s": 1.0, "ok": 1, "incertos": 0, "ausentes": 0 });
                c.p["rastreios"] = json!([r]); efeito(c, ocultar("r1")); c.p["midias"][0]["ocultacoes"] = json!([ocultar("r1")]); })),
            ("congelar fora, duração negativa", Box::new(|c| c.p["principal"] = json!([{ "tipo": "congelar", "id": "c1", "rotulo": "", "desligado": false,
                "transicao_entrada": { "tipo": "escurecer", "duracao_s": 1e300, "audio": "acompanhar" }, "efeitos": [estabilizar(0, 5)], "midia": "m1",
                "instante_s": -5.0, "instante_pts": 7, "duracao_s": -1.0 }]))),
            ("tela sem células", Box::new(|c| { let mut x = tela(); x["celulas"] = json!([]); x["referencia"] = json!(9); c.p["principal"] = json!([x]); })),
            ("rampa vazia", Box::new(|c| c.p["principal"][0]["velocidade"] = json!({ "tipo": "rampa", "pontos": [] }))),
            ("análise fora da tabela", Box::new(|c| { let mut e = estabilizar(0, 5); e["analise"] = json!({ "de_s": 1e9, "ate_s": -9.0, "de_pts": 3, "ate_pts": i64::MAX }); efeito(c, e); })),
            ("livre preso longe", Box::new(|c| c.p["sobreposicoes"] = json!([{ "id": "t1", "nome": "", "oculta": false, "clipes": [{ "tipo": "imagem",
                "id": "x1", "rotulo": "", "pos": { "ref": "clipe", "clipe": "c1", "desloc": -1e300 }, "duracao_s": 1e300, "transformacao": { "x": 0.5,
                "y": 0.5, "escala": 0.0, "rotacao": 1e300, "opacidade": -1.0 }, "entrada_s": -1.0, "saida_s": 1e300, "efeitos": [ocultar("r9")], "midia": "m9" }] }]))),
            ("áudio sem mídia", Box::new(|c| c.p["audios"] = json!([{ "id": "ta", "nome": "", "papel": "narracao", "volume": 1.0, "mudo": false,
                "abaixa_original": true, "clipes": [{ "id": "a1", "nome": "", "midia": "m9", "pos": { "ref": "programa", "t": -1.0 }, "de_s": -1.0,
                "ate_s": null, "volume": [{ "t": 0.0, "v": 9.0 }], "entrada_s": 0.0, "saida_s": 0.0 }] }]))),
        ];
        for (nome, mexe) in &tortos {
            for alvo in [None, Some(Alvo::Final { exportacao: cena().projeto().saida.exportacao }), Some(Alvo::Quadro { q: 0, largura: 480 })] {
                let mut c = cena();
                mexe(&mut c);
                c.alvo = alvo;
                let p = c.problemas();
                assert!(tem_erro(&p), "{nome}: {p:#?}");
                p.iter().for_each(frase_do_catalogo);
            }
        }
    }

    #[test]
    fn variantes_de_matriz_nao_mudam_o_resultado_limpo() {
        let mut c = cena();
        let mut f = fonte(tabela());
        f.matriz = Matriz::Bt601;
        c.fontes.insert("m1".into(), f);
        c.so(&[]);
    }

    /// Acréscimos do fecho (contratos (h) 4): lugar finito e a até 2^30 px, fundo de imagem do tipo imagem, cor do
    /// fundo, parâmetros fora de faixa com o campo.
    #[test]
    fn lugar_fundo_e_parametros_fora_de_faixa() {
        let texto = |x: f64| com(base("texto", "e2"), json!({ "conteudo": "x", "estilo": estilo(), "lugar": { "x": x, "y": 0.5, "h": "centro", "v": "meio" }, "largura_max": 0.8 }));
        let campo = |c: &Cena, codigo: &str| c.problemas().iter().filter(|p| p.codigo == codigo).map(|p| p.campo.clone().unwrap_or_default()).collect::<Vec<_>>();
        // 2^30 px = x · 1280: no limite passa (só a área segura avisa); além dele, e infinito, é inválido
        let limite = MAX_LUGAR_PX / 1280.0;
        for (x, esperado) in [(limite, vec![TEXTO_FORA]), (limite * 1.000001, vec![ITEM_INVALIDO]), (f64::MAX, vec![ITEM_INVALIDO]), (-limite * 2.0, vec![ITEM_INVALIDO])] {
            let mut c = cena();
            efeito(&mut c, texto(x));
            c.so(&esperado);
            if esperado == [ITEM_INVALIDO] {
                assert_eq!(campo(&c, ITEM_INVALIDO), vec!["lugar"], "{x}");
            }
        }
        // carimbo e bloco de título: a mesma regra
        let mut c = cena();
        efeito(&mut c, com(base("carimbo", "e3"), json!({ "mostra": "tempo_fonte", "formato": "hms_ms", "relogio": null, "estilo": estilo(),
            "lugar": { "x": 1e12, "y": 0.1, "h": "dir", "v": "topo" } })));
        c.so(&[ITEM_INVALIDO]);
        let mut c = cena();
        let mut t = titulo("c2", 2.0);
        t["titulo"]["blocos"] = json!([{ "id": "b1", "papel": "titulo", "conteudo": "Fim", "largura_max": 0.8, "estilo": estilo(),
            "lugar": { "x": 0.5, "y": -1e10, "h": "centro", "v": "meio" } }]);
        c.p["principal"].as_array_mut().unwrap().push(t.clone());
        c.so(&[ITEM_INVALIDO]);
        assert_eq!(c.problemas()[0].clipe.as_deref(), Some("c2"));
        // fundo: cor fora do formato; imagem que não existe ou que não é imagem
        let mut c = cena();
        t["titulo"]["blocos"] = json!([]);
        t["titulo"]["fundo"] = json!({ "tipo": "cor", "cor": "azul" });
        c.p["principal"].as_array_mut().unwrap().push(t.clone());
        c.so(&[COR]);
        let mut c = cena();
        t["titulo"]["fundo"] = json!({ "tipo": "imagem", "midia": "m9" });
        c.p["principal"].as_array_mut().unwrap().push(t.clone());
        c.so(&[MIDIA_AUSENTE]);
        t["titulo"]["fundo"] = json!({ "tipo": "imagem", "midia": "m1" });
        c.p["principal"][1] = t.clone();
        c.so(&[ITEM_INVALIDO]);
        assert_eq!((campo(&c, ITEM_INVALIDO), c.problemas()[0].midia.as_deref()), (vec!["midia".to_string()], Some("m1")));
        c.p["midias"].as_array_mut().unwrap().push(midia("m2", "imagem", SHA2));
        c.fontes.insert("m2".into(), imagem());
        c.midias.push(estado("m2", SituacaoMidia::Ok));
        c.p["principal"][1]["titulo"]["fundo"]["midia"] = json!("m2");
        c.so(&[]);
        // parâmetros: intensidade fora de 0..1, borda suave acima de 0,5, média de quadros fora de 2..15
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[30])]);
        efeito(&mut c, com(ocultar("r1"), json!({ "intensidade": 1.2 })));
        c.so(&[ITEM_INVALIDO]);
        assert_eq!(campo(&c, ITEM_INVALIDO), vec!["intensidade"]);
        c.p["principal"][0]["efeitos"][0]["intensidade"] = json!(1.0);
        c.p["principal"][0]["efeitos"][0]["alvo"]["borda_suave"] = json!(0.51);
        c.so(&[ITEM_INVALIDO]);
        assert_eq!(campo(&c, ITEM_INVALIDO), vec!["borda_suave"]);
        c.p["principal"][0]["efeitos"][0]["alvo"]["borda_suave"] = json!(0.5);
        c.so(&[]);
        let ns = ns_tipo("media_temporal");
        for (q, esperado) in [(1u32, vec![MEDIA_DE_QUADROS, ITEM_INVALIDO]), (2, vec![MEDIA_DE_QUADROS]), (15, vec![MEDIA_DE_QUADROS]), (16, vec![MEDIA_DE_QUADROS, ITEM_INVALIDO])] {
            let mut c = cena();
            efeito(&mut c, com(base("media_temporal", "e2"), json!({ "quadros": q })));
            c.so(&mais(&esperado, &[ns]));
        }
    }

    /// Recusas da Fase 0 no item: efeito de origem, `ocultar` em foto ou título, efeito em sobreposto, transição de
    /// outra fase, predefinição, som da Fase 1 (contratos (h) 1). O par com o motor está em `testes::aceite`.
    #[test]
    fn fase_0_recusa_no_item_com_rotulo() {
        let rotulo = |c: &Cena| c.problemas().iter().filter(|p| p.codigo == NAO_SUPORTADO).map(|p| p.mensagem.clone()).collect::<Vec<_>>();
        // cada parte só enquanto o pacote dono recusa (P1.4 realce, P1.8 predefinição, P1.5 som)
        let ajustes = com(base("ajustes", "e2"), json!({ "brilho": 0.0, "contraste": 1.0, "saturacao": 1.0, "gama": 1.0 }));
        if ns_efeito(&ajustes).is_some() {
            let mut c = cena();
            efeito(&mut c, ajustes);
            c.so(&[NAO_SUPORTADO]);
            assert_eq!(rotulo(&c), vec!["Ainda não suportado: ajustes de imagem"]);
            assert_eq!(c.problemas()[0].efeito.as_deref(), Some("e2"));
            // desligado: nada
            c.p["principal"][0]["efeitos"][0]["ligado"] = json!(false);
            c.so(&[]);
        }
        // predefinição: só na exportação (e na análise sem alvo); a prévia e o quadro final não pesam
        let mut c = cena();
        c.p["saida"]["exportacao"]["predefinicao"] = json!("whatsapp");
        c.so(&mais(&[], &[ns_se(motor_saida::recusa(&c.projeto().saida.exportacao).is_some())]));
        c.alvo = Some(Alvo::Previa { altura: 360, de_q: 0, ate_q: 10 });
        c.so(&[]);
        c.alvo = Some(Alvo::Quadro { q: 0, largura: 640 });
        c.so(&[]);
        // som da Fase 1: só com som na saída e fora do quadro final
        let mut c = cena();
        c.p["mixagem"] = json!({ "volume_original": 1.0, "abaixar": { "db": -12.0, "ataque_s": 0.3, "soltura_s": 0.5 }, "normalizar": { "lufs": -16.0, "pico_db": -1.5 }, "limitador": true });
        let rotulos: Vec<&str> = [("normalizar", "Ainda não suportado: normalização"), ("limitador", "Ainda não suportado: limitador")]
            .into_iter()
            .filter(|(d, _)| motor_audio::recusa(d))
            .map(|(_, r)| r)
            .collect();
        c.so(&mais(&[], &[ns_se(!rotulos.is_empty())]));
        assert_eq!(rotulo(&c), rotulos);
        c.alvo = Some(Alvo::Quadro { q: 0, largura: 640 });
        c.so(&[]);
        c.alvo = None;
        c.p["saida"]["audio"] = json!(false);
        c.so(&[]);
        // volume com chaves iguais vale como fixo; diferentes é volume animado
        let mut c = cena();
        c.p["principal"][0]["audio"]["volume"] = json!([{ "t": 0.0, "v": 0.8 }, { "t": 1.0, "v": 0.8 }]);
        c.so(&[]);
        c.p["principal"][0]["audio"]["volume"][1]["v"] = json!(0.4);
        c.so(&mais(&[], &[ns_se(motor_audio::recusa("volume_animado"))]));
        c.p["principal"][0]["audio"]["mudo"] = json!(true);
        c.so(&[]);
        // ocultar em título: no efeito do título
        let mut c = cena();
        let mut tit = titulo("c2", 2.0);
        tit["efeitos"] = json!([ocultar("r2")]);
        c.p["principal"].as_array_mut().unwrap().push(tit);
        assert!(c.problemas().iter().any(|p| p.codigo == NAO_SUPORTADO && p.clipe.as_deref() == Some("c2") && p.efeito.as_deref() == Some("e1")
            && p.mensagem == "Ainda não suportado: ocultação em foto ou título"), "{:#?}", c.problemas());
        // ocultar do congelado com `ativo` local em parte dos slots
        let mut c = cena();
        c.p["rastreios"] = json!([rastreio(&[45])]);
        let mut o = ocultar("r1");
        o["ativo"] = json!({ "de": 0.0, "ate": 0.5 });
        c.p["principal"] = json!([{ "tipo": "congelar", "id": "c1", "rotulo": "", "desligado": false, "transicao_entrada": null, "efeitos": [o],
            "midia": "m1", "instante_s": t(45), "instante_pts": 512 * 45, "duracao_s": 1.0 }]);
        c.so(&[NAO_SUPORTADO]);
        c.p["principal"][0]["efeitos"][0]["ativo"]["ate"] = json!(1.0);
        c.so(&[]);
        c.p["principal"][0]["efeitos"][0]["ativo"] = json!({ "de": 2.0, "ate": 3.0 });
        c.so(&[]);
    }

    // ---- Fase 1 (P1.R): aceites do §11 e pendências de contratos-fase1 (e)

    use crate::edicao::camada::texto::SpritesEmCache;
    use crate::edicao::modelo::Intervalo;
    use crate::edicao::testes::aceite::{alvo_final, midia as midia_do_aceite, projeto as projeto_do_aceite, trecho as trecho_do_aceite, Mundo};
    use crate::edicao::testes::fontes::{self, Fonte as FonteDeTeste};
    use crate::edicao::testes::{executor, medir};
    use std::cell::RefCell;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    /// ffmpeg dos testes; sem ele o teste se dispensa, salvo com `SICRO_EXIGE_FFMPEG=1` (aí falha).
    fn cap_real() -> Option<Arc<Capacidades>> {
        let cap = fontes::cap();
        if cap.is_none() && std::env::var("SICRO_EXIGE_FFMPEG").is_ok_and(|v| v == "1") {
            panic!("SICRO_EXIGE_FFMPEG=1 e nenhum ffmpeg achado");
        }
        cap
    }

    /// `FonteInfo` de um arquivo, com a tabela do decodificador.
    fn fonte_real(cap: &Capacidades, video: &Path) -> FonteInfo {
        let tab = TabelaQuadros::do_decodificador(cap, video, &AtomicBool::new(false), &mut |_| {}).expect("tabela do decodificador");
        crate::edicao::quadros::fonte_info(cap, video, SHA, Arc::new(tab), None).expect("fonte_info")
    }

    /// Análise de um trecho a 1× dos quadros `[j_in, j_out)` do arquivo, na saída do tamanho dele e na taxa `fps`.
    fn analise_do_arquivo(cap: &Arc<Capacidades>, info: &FonteInfo, j_in: usize, j_out: usize, fps: (i64, i64)) -> (ProjetoEdicao, Analise) {
        let p = projeto_do_aceite(info.exibido.0, info.exibido.1, fps, vec![midia_do_aceite("m1", "video", json!([]))], vec![trecho_do_aceite("c1", "m1", &info.quadros, j_in, j_out, 1.0, json!([]))], json!({}));
        let fontes = HashMap::from([("m1".to_string(), info.clone())]);
        let midias = [estado("m1", SituacaoMidia::Ok)];
        let ctx = Contexto { fontes: &fontes, cap, rastreios: &SemRastreios, pasta_projeto: Path::new(""), midias: &midias };
        let a = analisar(&p, &ctx, None);
        a.problemas.iter().for_each(frase_do_catalogo);
        (p, a)
    }

    /// Passos de pts fora de ±25 % do mediano, contados dos pts que o decodificador entrega (`showinfo`), sem a tabela.
    fn passos_irregulares(cap: &Capacidades, video: &Path) -> u64 {
        let pts = medir::pts(cap, video).unwrap();
        let mut passos: Vec<i64> = pts.windows(2).map(|w| w[1] - w[0]).collect();
        let fora = |med: i64| passos.iter().filter(|d| ((**d - med) as f64).abs() > 0.25 * med as f64).count() as u64;
        let mut em_ordem = passos.clone();
        em_ordem.sort_unstable();
        let n = fora(em_ordem[em_ordem.len() / 2]);
        passos.clear();
        n
    }

    /// Aceite do §11: `p1a/src/vfr.mp4` acusa "Taxa variável". O arquivo do dossiê (quando presente) e a fonte de taxa
    /// variável da matriz, pela tabela do decodificador: aviso pericial na mídia do clipe, com a contagem dos
    /// intervalos irregulares medida à parte nos pts do arquivo; pede ciência; fonte constante não acusa.
    #[test]
    fn vfr_mp4_acusa_taxa_variavel() {
        let Some(cap) = cap_real() else { return };
        let do_dossie = fontes::dossie().map(|d| d.join("p1a/src/vfr.mp4")).filter(|p| p.is_file());
        if do_dossie.is_none() {
            eprintln!("p1a/src/vfr.mp4 ausente (SICRO_DOSSIE): só a fonte de taxa variável da matriz");
        }
        let da_matriz = fontes::gerar(FonteDeTeste::TaxaVariavel).unwrap();
        for (video, esperado) in [(do_dossie, Some(56u64)), (Some(da_matriz), None)] {
            let Some(video) = video else { continue };
            let info = fonte_real(&cap, &video);
            let n = info.quadros.pts.len();
            let (_, a) = analise_do_arquivo(&cap, &info, 0, n, (30, 1));
            let vfr: Vec<&Problema> = a.problemas.iter().filter(|p| p.codigo == TAXA_VARIAVEL).collect();
            assert_eq!(vfr.len(), 1, "{}: {:#?} ({})", video.display(), a.problemas, cap.versao);
            let p = vfr[0];
            let irregulares = passos_irregulares(&cap, &video);
            assert!(irregulares > 0, "{}: a fonte não é de taxa variável", video.display());
            assert_eq!((p.nivel, p.clipe.as_deref(), p.midia.as_deref(), p.n), (Nivel::Pericial, Some("c1"), Some("m1"), Some(irregulares)), "{} ({})", video.display(), cap.versao);
            assert_eq!(p.mensagem, format!("Taxa variável: {irregulares} intervalos irregulares"));
            if let Some(e) = esperado {
                // 300 quadros a 30 qps menos os de `mod(n,7)=3`, `mod(n,11)=5` e 100..112 (`p1a/scripts/00-fontes.sh`)
                assert_eq!((n, irregulares), (224, e), "{}", video.display());
            }
            // pericial: a exportação pede ciência
            assert!(!tem_erro(&a.problemas), "{:#?}", a.problemas);
            assert_eq!(sem_ciencia(&a.problemas, &[]), vec![TAXA_VARIAVEL]);
            assert_eq!(sem_ciencia(&a.problemas, &[TAXA_VARIAVEL.to_string()]), Vec::<&str>::new());
            // dois quadros vizinhos de passo igual e o passo seguinte também: trecho regular não acusa
            let regular = (0..n - 3).find(|&j| (1..=3).all(|k| info.quadros.pts[j + k] - info.quadros.pts[j + k - 1] == info.quadros.passo_mediano)).expect("três passos regulares seguidos");
            let (_, a) = analise_do_arquivo(&cap, &info, regular, regular + 2, (30, 1));
            assert!(!a.problemas.iter().any(|p| p.codigo == TAXA_VARIAVEL), "{:#?}", a.problemas);
        }
        // fonte de taxa constante: nada
        let info = fonte_real(&cap, &fontes::gerar(FonteDeTeste::Mp4).unwrap());
        let (_, a) = analise_do_arquivo(&cap, &info, 0, info.quadros.pts.len(), (30, 1));
        assert_eq!(a.problemas, vec![], "{:#?}", a.problemas);
    }

    /// Aceite do §11: 25 em 30 acusa repetição com a contagem. Pela conta (tabela sintética) e medido no vídeo
    /// renderizado pelo compilador: os quadros repetidos na saída são os que o aviso conta.
    #[test]
    fn vinte_e_cinco_em_trinta_acusa_repeticao() {
        // 2 s de uma gravação de 25 qps (50 quadros) num programa de 30: 60 slots, 10 repetidos, nenhum descartado
        let mut c = cena();
        let tb = (1, 12800);
        c.fontes.insert("m1".into(), fonte(apoio::tabela(tb, apoio::pts_regulares(250, (1, 25), tb, 0))));
        for (k, v) in [("inicio_s", json!(1.0)), ("inicio_pts", json!(512 * 25)), ("fim_s", json!(3.0)), ("fim_pts", json!(512 * 75))] {
            c.p["principal"][0][k] = v;
        }
        c.so(&[FPS_DIFERENTE]);
        let p = &c.problemas()[0];
        assert_eq!((p.nivel, p.n, p.mensagem.as_str(), p.clipe.as_deref(), p.midia.as_deref()), (Nivel::Aviso, Some(10), "Taxa diferente da saída: 10 quadros repetidos ou descartados", Some("c1"), Some("m1")));
        let f = c.analise().linha.clipes[0].fonte.clone().unwrap();
        assert_eq!((c.analise().linha.total_q, f.lidos, f.repetidos, f.descartados), (60, 50, 10, 0));
        // na própria taxa não há aviso; a 0,5× a repetição é a câmera lenta e o aviso sai sem contagem
        c.p["saida"]["fps"] = json!({ "num": 25, "den": 1 });
        c.so(&[]);
        c.p["saida"]["fps"] = json!({ "num": 30, "den": 1 });
        c.p["principal"][0]["velocidade"]["fator"] = json!(0.5);
        c.so(&[FPS_DIFERENTE]);
        assert_eq!((c.problemas()[0].n, c.problemas()[0].mensagem.as_str()), (None, "Taxa da gravação diferente da saída"));

        // medido: AVI de 25 qps pelo compilador, quadro de origem de cada slot
        let Some(cap) = cap_real() else { return };
        let info = fonte_real(&cap, &fontes::gerar(FonteDeTeste::Avi25).unwrap());
        assert_eq!(info.quadros.taxa_nominal(), Racional { num: 25, den: 1 }, "{}", cap.versao);
        let (p, a) = analise_do_arquivo(&cap, &info, 25, 75, (30, 1));
        let aviso: Vec<&Problema> = a.problemas.iter().filter(|p| p.codigo == FPS_DIFERENTE).collect();
        assert_eq!(aviso.len(), 1, "{:#?}", a.problemas);
        assert!(!a.problemas.iter().any(|p| p.codigo == QUADROS_DESCARTADOS || p.nivel == Nivel::Erro), "{:#?}", a.problemas);
        let n = aviso[0].n.expect("contagem");
        let plano = Mundo::novo(&cap).com("m1", &info).compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{}: {e}", cap.versao));
        let pasta = fontes::pasta_temporaria("p1r-25-em-30");
        let r = executor::rodar(&cap, &plano, &pasta, &executor::Opcoes { sem_perdas: true, ..Default::default() }).unwrap_or_else(|e| panic!("{}: {e}", cap.versao));
        let mut origem = Vec::new();
        for b in &r.blocos {
            origem.extend(medir::quadros_de_origem(&cap, b).unwrap());
        }
        let _ = std::fs::remove_dir_all(&pasta);
        assert_eq!(origem.len(), 60, "{}", cap.versao);
        let repetidos = origem.windows(2).filter(|w| w[0] == w[1]).count() as u64;
        let distintos: BTreeSet<u32> = origem.iter().copied().collect();
        assert_eq!(distintos, (25..75).collect::<BTreeSet<u32>>(), "todo quadro do trecho aparece ({})", cap.versao);
        assert!(origem.windows(2).all(|w| w[0] <= w[1]), "{origem:?}");
        assert_eq!((repetidos, n), (10, 10), "repetidos medidos no vídeo × contagem do aviso ({})", cap.versao);
    }

    fn velocidade_exibida(de_s: f64, ate_s: f64, ativo: Value) -> Value {
        let mut e = com(base("velocidade_exibida", "v1"), json!({ "calculo": { "id": "k1", "kmh": 42.0, "faixa": [39.0, 45.0], "de_s": de_s, "ate_s": ate_s, "metodo": "dois pontos" },
            "onde": { "modo": "fixa", "lugar": { "x": 0.5, "y": 0.9, "h": "centro", "v": "base" } }, "estilo": estilo() }));
        e["ativo"] = ativo;
        e
    }

    /// O5/B17: `velocidade_fora_do_intervalo` é erro e a regra (do P1.3) recebe o tempo de FONTE do quadro mostrado
    /// em cada instante em que o efeito está ativo: o quadro lido no trecho, o quadro congelado no congelado (o
    /// `ativo` dele é local), nada em clipe sem fonte.
    #[test]
    fn velocidade_fora_do_intervalo() {
        assert_eq!((mensagens::nivel(VELOCIDADE_FORA_DO_INTERVALO), mensagens::frase(VELOCIDADE_FORA_DO_INTERVALO, None, None).as_str()), (Some(Nivel::Erro), "Velocidade fora do intervalo medido"));
        let c = cena();
        let p = c.projeto();
        let ctx = Contexto { fontes: &c.fontes, cap: &c.cap, rastreios: &SemRastreios, pasta_projeto: Path::new(""), midias: &c.midias };
        let linha = tempo::resolver(&p, &ctx);
        let v = Validacao::nova(&p, &linha, &ctx, None);
        let tab = tabela();
        let fps = Racional { num: 30, den: 1 };
        let ef_base = |ativo: Option<(f64, f64)>| EfBase { id: "v1".into(), ligado: true, nome: String::new(), ativo: ativo.map(|(de, ate)| Intervalo { de: Some(de), ate: Some(ate) }) };
        let vistos = |uso: &Uso, b: &EfBase| {
            let lista = RefCell::new(Vec::new());
            assert!(!v.em_algum_quadro_mostrado(uso, b, &|t| {
                lista.borrow_mut().push(t);
                false
            }));
            lista.into_inner()
        };
        // trecho dos quadros 30..90, ativo de t(40) a t(50), intervalo fechado: os 11 quadros lidos, no tempo de fonte
        let trecho_uso = Uso { onde: Onde::clipe("c1"), midia: Some("m1"), base: Base::Fonte, efeitos: &[], tab: Some(&tab), quadros: Some((30, 90)), n_q: 60, fps, origem: None };
        assert_eq!(vistos(&trecho_uso, &ef_base(Some((t(40), t(50))))), (40..=50).map(|k| Some(t(k))).collect::<Vec<_>>());
        assert_eq!(vistos(&trecho_uso, &ef_base(None)).len(), 60);
        // congelado no quadro 45 por 1 s, ativo local de 0,2 a 0,5 s: 10 slots, todos com o tempo do quadro 45
        let congelado = Uso { onde: Onde::clipe("c1"), midia: Some("m1"), base: Base::Congelado, efeitos: &[], tab: Some(&tab), quadros: Some((45, 46)), n_q: 30, fps, origem: None };
        assert_eq!(vistos(&congelado, &ef_base(Some((0.2, 0.5)))), vec![Some(t(45)); 10]);
        // título: sem fonte
        let titulo_uso = Uso { onde: Onde::clipe("c1"), midia: None, base: Base::Local, efeitos: &[], tab: None, quadros: None, n_q: 30, fps, origem: None };
        assert_eq!(vistos(&titulo_uso, &ef_base(None)), vec![None; 30]);
        // a regra que acusa num quadro só basta
        assert!(v.em_algum_quadro_mostrado(&trecho_uso, &ef_base(None), &|x| x == Some(t(89))));
        assert!(!v.em_algum_quadro_mostrado(&trecho_uso, &ef_base(None), &|x| x == Some(t(90))), "o quadro 90 não é lido");

        // ligado à regra do P1.3: o erro sai quando ela acusa algum quadro mostrado (e some quando o `ativo` fica
        // dentro do intervalo medido). Enquanto o P1.3 não entrega, a regra não acusa e o efeito é recusado.
        for (ativo, quadros) in [(json!(null), 30..90), (json!({ "de": t(40), "ate": t(50) }), 40..51)] {
            let e = velocidade_exibida(t(40), t(50), ativo);
            let tipado: Efeito = serde_json::from_value(e.clone()).unwrap();
            let Efeito::VelocidadeExibida(ve) = &tipado else { panic!() };
            let regra_acusa = quadros.clone().any(|k| medidas::fora_do_intervalo(ve, Some(t(k))));
            let mut c = cena();
            efeito(&mut c, e.clone());
            let esperado: &[&str] = if regra_acusa { &[VELOCIDADE_FORA_DO_INTERVALO] } else { &[] };
            c.so(&mais(esperado, &[ns_efeito(&e)]));
            if regra_acusa {
                let p = c.problemas().into_iter().find(|p| p.codigo == VELOCIDADE_FORA_DO_INTERVALO).unwrap();
                assert_eq!((p.nivel, p.clipe.as_deref(), p.efeito.as_deref()), (Nivel::Erro, Some("c1"), Some("v1")));
            }
        }
    }

    /// Desentrelace por campo (contratos-fase1 (c) P1.4): o mapa do trecho é contado na tabela do CLIPE (uma entrada
    /// por campo), como no motor; o que vale por quadro lido fica nos quadros da fonte.
    #[test]
    fn trecho_contado_na_tabela_do_clipe() {
        let c = cena();
        let p = c.projeto();
        let ctx = Contexto { fontes: &c.fontes, cap: &c.cap, rastreios: &SemRastreios, pasta_projeto: Path::new(""), midias: &c.midias };
        let linha = tempo::resolver(&p, &ctx);
        let Tolerante::Conhecido(ClipePrincipal::Trecho(tr)) = &p.principal[0] else { panic!() };
        let mut tr = tr.clone();
        // os pts gravados são da base da fonte: na tabela de campos vale o tempo
        (tr.inicio_pts, tr.fim_pts) = (None, None);
        let fonte_tab = tabela();
        // um campo a cada 1/60 s, na base dobrada
        let tb2 = (1, 30720);
        let campos = apoio::tabela(tb2, apoio::pts_regulares(600, (1, 60), tb2, 0));
        let onde = Onde::clipe("c1").com_midia("m1");
        // a própria tabela da fonte: 60 quadros em 60 slots, nada a dizer
        let mut v = Validacao::nova(&p, &linha, &ctx, None);
        let l = v.lido_do_trecho(&tr, &fonte_tab, &fonte_tab, &onde).unwrap();
        assert_eq!((l.fonte, l.tempos.len(), v.saida.len()), ((30, 90), 60, 0), "{:#?}", v.saida);
        assert_eq!((l.tempos[0], l.tempos[59]), (t(30), t(89)));
        // por campo: 120 entradas em 60 slots (60 descartadas, taxa de 60 ≠ 30), dos mesmos quadros 30..90 da fonte
        let mut v = Validacao::nova(&p, &linha, &ctx, None);
        let l = v.lido_do_trecho(&tr, &fonte_tab, &campos, &onde).unwrap();
        assert_eq!(l.fonte, (30, 90), "quadros da fonte de onde os campos saem");
        assert_eq!(l.tempos.len(), 60);
        assert_eq!((l.tempos[0], l.tempos[1]), (1.0, 1.0 + 1.0 / 30.0), "um slot a cada dois campos");
        let cods: Vec<(&str, Option<u64>)> = v.saida.iter().map(|p| (p.codigo, p.n)).collect();
        assert_eq!(cods, [(QUADROS_DESCARTADOS, Some(60)), (FPS_DIFERENTE, Some(60))], "{:#?}", v.saida);
        // tabela de campos na BASE DA FONTE (a que o P1.4 monta): o primeiro campo guarda o pts do quadro, o segundo
        // cai no meio do passo; os pts gravados no trecho continuam valendo como identidade
        let Tolerante::Conhecido(ClipePrincipal::Trecho(com_pts)) = &p.principal[0] else { panic!() };
        assert_eq!((com_pts.inicio_pts, com_pts.fim_pts), (Some(512 * 30), Some(512 * 90)));
        let campos_na_base = apoio::tabela((1, 15360), fonte_tab.pts.iter().flat_map(|&x| [x, x + 256]).collect());
        assert_eq!(campos_na_base.pts.len(), 2 * fonte_tab.pts.len());
        let mut v = Validacao::nova(&p, &linha, &ctx, None);
        let l = v.lido_do_trecho(com_pts, &fonte_tab, &campos_na_base, &onde).unwrap();
        assert_eq!((l.fonte, l.tempos.len()), ((30, 90), 60), "campos 60..180 = quadros 30..90 da fonte");
        assert_eq!((l.tempos[0], l.tempos[1], l.tempos[59]), (t(30), t(31), t(89)));
        let cods: Vec<(&str, Option<u64>)> = v.saida.iter().map(|p| (p.codigo, p.n)).collect();
        assert_eq!(cods, [(QUADROS_DESCARTADOS, Some(60)), (FPS_DIFERENTE, Some(60))], "{:#?}", v.saida);
        // e a regra do pacote dono decide a tabela: enquanto ela devolve a da fonte, o projeto limpo segue limpo
        assert!(matches!(realce::tabela(&tr.base.efeitos, &fonte_tab), std::borrow::Cow::Borrowed(_)));
        c.so(&[]);
    }

    /// contratos-fase1 (e): `lugar` de bloco de título conferido no ponto que o rasterizador usa (o canto do
    /// sprite). A resposta da validação é a de `camada::titulo::primitivas` em cada caso da faixa de dúvida.
    #[test]
    fn lugar_do_bloco_de_titulo_pelo_canto_do_sprite() {
        let tmp = tempfile::Builder::new().prefix("p1r-lugar-").tempdir().unwrap();
        let bloco = |x_px: f64, h: &str, conteudo: &str| json!({ "id": "b1", "papel": "titulo", "conteudo": conteudo, "largura_max": 0.8, "estilo": estilo(),
            "lugar": { "x": x_px / 1280.0, "y": 0.5, "h": h, "v": "meio" } });
        let rasterizador_aceita = |b: &Value| {
            let tit: Titulo = serde_json::from_value(json!({ "modelo": "cartela", "fundo": { "tipo": "transparente" }, "blocos": [b] })).unwrap();
            let quadro = camada::formas::QuadroCamada { q: 0, t: 0.0, pts: None, a: geometria::Afim::IDENTIDADE, canvas: TamanhoPx { w: 1280, h: 720 },
                rastreios: std::collections::BTreeMap::new(), indice: None, programa_s: 0.0 };
            camada::titulo::primitivas(&tit, &quadro, &SpritesEmCache { cache: tmp.path(), imagens: &[] }).is_ok()
        };
        let validacao_aceita = |b: &Value| {
            let mut c = cena();
            let mut tit = titulo("c2", 2.0);
            tit["titulo"]["blocos"] = json!([b]);
            c.p["principal"].as_array_mut().unwrap().push(tit);
            let p = c.problemas();
            let invalido: Vec<_> = p.iter().filter(|p| p.codigo == ITEM_INVALIDO).collect();
            assert!(invalido.iter().all(|p| p.campo.as_deref() == Some("lugar") && p.clipe.as_deref() == Some("c2")), "{p:#?}");
            assert!(!p.iter().any(|p| p.nivel == Nivel::Erro && p.codigo != ITEM_INVALIDO), "{p:#?}");
            invalido.is_empty()
        };
        let limite = MAX_LUGAR_PX;
        // (apoio em px, alinhamento, o que o rasterizador faz)
        let casos = [
            // apoio dentro do limite, canto fora: passava na validação e falhava no meio da exportação
            (-limite + 10.0, "dir", false),
            (-limite + 10.0, "centro", false),
            (-limite + 10.0, "esq", true),
            // apoio fora do limite, canto dentro: a validação recusava o que o rasterizador desenha
            (limite + 10.0, "dir", true),
            (limite + 10.0, "esq", false),
            // longe da dúvida
            (0.5 * 1280.0, "centro", true),
            (-limite * 2.0, "dir", false),
            (limite * 2.0, "dir", false),
        ];
        for (x, h, esperado) in casos {
            let b = bloco(x, h, "Fim");
            assert_eq!(rasterizador_aceita(&b), esperado, "rasterizador: x = {x} px, {h}");
            assert_eq!(validacao_aceita(&b), esperado, "validação: x = {x} px, {h}");
        }
        // bloco vazio não é desenhado: vale o ponto de apoio; não finito é sempre inválido
        assert!(validacao_aceita(&bloco(limite - 10.0, "dir", "")) && !validacao_aceita(&bloco(limite + 10.0, "dir", "")));
        let mut torto = bloco(0.0, "esq", "Fim");
        torto["lugar"]["y"] = json!(1e300);
        assert!(!validacao_aceita(&torto));
    }

    /// Problemas do efeito `id`: (código, campo).
    fn do_efeito(c: &Cena, id: &str) -> Vec<(&'static str, Option<String>)> {
        c.problemas().into_iter().filter(|p| p.efeito.as_deref() == Some(id)).map(|p| (p.codigo, p.campo)).collect()
    }

    /// (h) 1 na régua: tudo o que o desenho recusa (`medidas::pontas_conferidas`, a regra do render) a validação acusa
    /// antes, com o mesmo código e o campo: ponta fora, âncora fora do segmento dos modelos 1D, campos inválidos.
    #[test]
    fn regua_validacao_acusa_o_que_o_desenho_recusa() {
        let quadrado = json!([{ "x": 0.2, "y": 0.2 }, { "x": 0.8, "y": 0.2 }, { "x": 0.8, "y": 0.8 }, { "x": 0.2, "y": 0.8 }]);
        let muda = |f: &dyn Fn(&mut Value)| {
            let mut r = regua(quadrado.clone());
            f(&mut r);
            r
        };
        // H = identidade em 1280×720: 1 m = 1 px; a âncora fica em (640, 360) e a região vai de 256 a 1024 px em x
        let casos: Vec<(&str, Value, Option<(&'static str, &'static str)>)> = vec![
            ("limpa", muda(&|_| {}), None),
            ("ponta na borda", muda(&|r| r["comprimento_m"] = json!(384.0)), None),
            ("ponta fora", muda(&|r| r["comprimento_m"] = json!(400.0)), Some((REGUA_FORA_DA_CALIBRACAO, "comprimento_m"))),
            ("âncora fora", muda(&|r| r["ancora"] = json!({ "x": 0.9, "y": 0.9 })), Some((REGUA_FORA_DA_CALIBRACAO, "ancora"))),
            // região de 2 pontos = segmento do modelo 1D: a âncora projeta fora dele
            ("âncora fora do segmento", muda(&|r| r["calibracao"]["regiao"] = json!([{ "x": 0.2, "y": 0.5 }, { "x": 0.4, "y": 0.5 }])), Some((REGUA_FORA_DA_CALIBRACAO, "ancora"))),
            ("divisões demais", muda(&|r| r["divisoes"] = json!(101)), Some((ITEM_INVALIDO, "divisoes"))),
            ("comprimento zero", muda(&|r| r["comprimento_m"] = json!(0.0)), Some((ITEM_INVALIDO, "comprimento_m"))),
            ("comprimento acima do teto", muda(&|r| r["comprimento_m"] = json!(1000.5)), Some((ITEM_INVALIDO, "comprimento_m"))),
            ("homografia com 8 números", muda(&|r| r["calibracao"]["h"] = json!([1, 0, 0, 0, 1, 0, 0, 0])), Some((ITEM_INVALIDO, "calibracao"))),
            ("espaço zerado", muda(&|r| r["calibracao"]["espaco"] = json!({ "w": 0, "h": 720 })), Some((ITEM_INVALIDO, "calibracao"))),
        ];
        for (nome, r, esperado) in casos {
            let Efeito::Regua(rg) = serde_json::from_value::<Efeito>(r.clone()).unwrap() else { panic!() };
            // o render: o desenho só sai com a régua conferida
            let render = medidas::pontas_conferidas(&rg).err().map(|e| e.to_string());
            let mut c = cena();
            efeito(&mut c, r);
            let achados = do_efeito(&c, "e6");
            match esperado {
                None => {
                    assert!(render.is_none(), "{nome}: o desenho recusa ({render:?})");
                    assert_eq!(achados, vec![], "{nome}");
                }
                Some((cod, campo)) => {
                    let render = render.unwrap_or_else(|| panic!("{nome}: o desenho aceita"));
                    assert!(render.contains(&format!("{cod}: {campo}")), "{nome}: {render}");
                    assert_eq!(achados, vec![(cod, Some(campo.to_string()))], "{nome}");
                }
            }
        }
    }

    /// A medida da análise vale só na mídia em que foi feita: régua e velocidade em clipe de OUTRA mídia, em foto e
    /// em título são erro; na própria mídia, e com retrato gravado antes do campo, não.
    #[test]
    fn regua_e_velocidade_so_na_midia_da_medida() {
        let quadrado = json!([{ "x": 0.2, "y": 0.2 }, { "x": 0.8, "y": 0.2 }, { "x": 0.8, "y": 0.8 }, { "x": 0.2, "y": 0.8 }]);
        let regua_de = |sha: Option<&str>| {
            let mut r = regua(quadrado.clone());
            if let Some(s) = sha {
                r["calibracao"]["midia_sha256"] = json!(s);
            }
            r
        };
        let vel_de = |sha: Option<&str>| {
            let mut v = velocidade_exibida(t(30), t(89), json!(null));
            v["id"] = json!("e7");
            if let Some(s) = sha {
                v["calculo"]["midia_sha256"] = json!(s);
            }
            v
        };
        let fora_regua = vec![(REGUA_FORA_DA_CALIBRACAO, Some("calibracao".to_string()))];
        let fora_vel = vec![(VELOCIDADE_FORA_DO_INTERVALO, Some("calculo".to_string()))];
        // a mídia do clipe é a m1 (SHA): a medida dela e o retrato antigo (sem o campo) passam; a de outra, não
        for (sha, regua_esp, vel_esp) in [(Some(SHA), vec![], vec![]), (None, vec![], vec![]), (Some(SHA2), fora_regua.clone(), fora_vel.clone())] {
            let mut c = cena();
            efeito(&mut c, regua_de(sha));
            efeito(&mut c, vel_de(sha));
            assert_eq!(do_efeito(&c, "e6"), regua_esp, "régua, mídia {sha:?}");
            assert_eq!(do_efeito(&c, "e7"), vel_esp, "velocidade, mídia {sha:?}");
        }
        // o campo novo vai e volta e não aparece em retrato antigo regravado
        let Efeito::Regua(rg) = serde_json::from_value::<Efeito>(regua_de(Some(SHA))).unwrap() else { panic!() };
        assert_eq!(serde_json::to_value(&rg.calibracao).unwrap()["midia_sha256"], SHA);
        let Efeito::Regua(antiga) = serde_json::from_value::<Efeito>(regua_de(None)).unwrap() else { panic!() };
        assert!(serde_json::to_value(&antiga.calibracao).unwrap().get("midia_sha256").is_none());
        // quadro congelado da mesma mídia: vale como no trecho
        let congelar = |efeitos: Value| json!({ "tipo": "congelar", "id": "c1", "rotulo": "", "desligado": false, "transicao_entrada": null,
            "efeitos": efeitos, "midia": "m1", "instante_s": t(45), "instante_pts": 512 * 45, "duracao_s": 1.0 });
        let mut c = cena();
        c.p["principal"] = json!([congelar(json!([regua_de(Some(SHA))]))]);
        assert_eq!(do_efeito(&c, "e6"), vec![]);
        c.p["principal"] = json!([congelar(json!([regua_de(Some(SHA2))]))]);
        assert_eq!(do_efeito(&c, "e6"), fora_regua);
        // foto e título não têm calibração nenhuma, nem com a mídia "certa" no retrato nem com retrato antigo
        for sha in [Some(SHA2), None] {
            let mut c = cena();
            c.p["midias"].as_array_mut().unwrap().push(midia("m2", "imagem", SHA2));
            c.fontes.insert("m2".into(), imagem());
            c.midias.push(estado("m2", SituacaoMidia::Ok));
            c.p["principal"] = json!([{ "tipo": "imagem", "id": "c1", "rotulo": "", "desligado": false, "transicao_entrada": null,
                "efeitos": [regua_de(sha)], "midia": "m2", "duracao_s": 2.0 }]);
            assert_eq!(do_efeito(&c, "e6"), fora_regua, "foto, {sha:?}");
            let mut tit = titulo("c1", 2.0);
            tit["efeitos"] = json!([regua_de(sha), vel_de(sha)]);
            c.p["principal"] = json!([tit]);
            assert_eq!(do_efeito(&c, "e6"), fora_regua, "título, {sha:?}");
            // a velocidade em clipe sem fonte já é fora do intervalo (não há quadro medido)
            assert_eq!(do_efeito(&c, "e7"), vec![(VELOCIDADE_FORA_DO_INTERVALO, None)], "título, {sha:?}");
        }
        assert!(medidas::de_outra_midia(Some(SHA2), Some(SHA)) && !medidas::de_outra_midia(Some(SHA), Some(SHA)) && !medidas::de_outra_midia(None, Some(SHA)) && medidas::de_outra_midia(Some(SHA), None));
    }

    /// O limite do `xfade` (60 s) é acusado antes: a regra é a da junção (`juncao::longa_demais`), só nas que sobrepõem.
    #[test]
    fn transicao_acima_do_limite_do_xfade() {
        let dois = |tipo: &str, d: f64, dur: f64| {
            let mut c = cena();
            let mut a = titulo("c1", dur);
            a["efeitos"] = json!([]);
            let mut b = titulo("c2", dur);
            b["transicao_entrada"] = json!({ "tipo": tipo, "duracao_s": d, "audio": "acompanhar" });
            c.p["principal"] = json!([a, b]);
            c
        };
        // clipes de 200 s: a transição cabe nos dois, só o limite do filtro pesa
        let transicao = |c: &Cena| c.problemas().into_iter().filter(|p| p.codigo == TRANSICAO).map(|p| (p.clipe, p.campo)).collect::<Vec<_>>();
        assert_eq!(transicao(&dois("fundir", 60.0, 200.0)), vec![], "60 s cabem");
        assert_eq!(transicao(&dois("fundir", 60.1, 200.0)), vec![(Some("c2".to_string()), Some("transicao_entrada".to_string()))]);
        assert_eq!(transicao(&dois("circulo", 90.0, 200.0)).len(), 1);
        assert_eq!(transicao(&dois("escurecer", 90.0, 200.0)), vec![], "o `fade` não tem o limite");
        // a mesma regra do motor: `juncao::montar` recusa exatamente o que a validação acusa
        let fps = Racional { num: 30, den: 1 };
        for (tipo, d, longa) in [("fundir", 60.0, false), ("fundir", 60.1, true), ("escurecer", 90.0, false)] {
            let t = super::super::modelo::Transicao { tipo: tipo.into(), duracao_s: d, audio: super::super::modelo::AudioTransicao::Acompanhar };
            assert_eq!(juncao::longa_demais(&t, fps), longa, "{tipo} {d}");
            let slots = tempo::slots(d, fps) as u32;
            let canvas = super::super::motor::plano::Canvas { w: 320, h: 180, fps, cor_fundo: "#000000".into() };
            let pecas = [
                juncao::Peca { rotulo: "a".into(), clipe: "a".into(), n_q: 6000, de: 0, ate: 6000, sobrepoe: 0, entrada: None, saida: Some((t.clone(), slots)) },
                juncao::Peca { rotulo: "b".into(), clipe: "b".into(), n_q: 6000, de: 0, ate: 6000, sobrepoe: if tempo::transicao_sobrepoe(tipo) { slots } else { 0 }, entrada: Some((t.clone(), slots)), saida: None },
            ];
            let r = juncao::montar(&mut super::super::motor::grafo::Grafo::novo(), &pecas, &canvas);
            assert_eq!(r.is_err(), longa, "{tipo} {d}: {:?}", r.err());
        }
    }

    /// O que o motor de áudio recusa (`item_invalido: abaixar`) ou ajustaria calado na mixagem: acusado antes, pela
    /// regra dele (`audio::mixagem_invalida`).
    #[test]
    fn mixagem_invalida_e_acusada() {
        let com_mix = |m: Value| {
            let mut c = cena();
            c.p["mixagem"] = m;
            c.problemas().into_iter().filter(|p| p.codigo == AUDIO).map(|p| p.campo).collect::<Vec<_>>()
        };
        let mix = |abaixar: Value, normalizar: Value| json!({ "volume_original": 1.0, "abaixar": abaixar, "normalizar": normalizar, "limitador": false });
        let ab = |db: f64, a: f64, s: f64| json!({ "db": db, "ataque_s": a, "soltura_s": s });
        assert_eq!(com_mix(mix(ab(-12.0, 0.3, 0.5), json!({ "lufs": -16.0, "pico_db": -1.5 }))), vec![]);
        assert_eq!(com_mix(mix(ab(0.0, 0.0, 0.0), json!(null))), vec![]);
        for ruim in [ab(-12.0, -0.1, 0.5), ab(-12.0, 0.3, -0.5), ab(3.0, 0.3, 0.5)] {
            assert_eq!(com_mix(mix(ruim.clone(), json!(null))), vec![Some("abaixar".to_string())], "{ruim}");
        }
        for ruim in [json!({ "lufs": -80.0, "pico_db": -1.5 }), json!({ "lufs": 0.0, "pico_db": -1.5 }), json!({ "lufs": -16.0, "pico_db": 1.0 }), json!({ "lufs": -16.0, "pico_db": -30.0 })] {
            assert_eq!(com_mix(mix(ab(-12.0, 0.3, 0.5), ruim.clone())), vec![Some("normalizar".to_string())], "{ruim}");
        }
    }

    /// Efeitos de origem: o que a cadeia recusa com `item_invalido` (`ativo` parcial, dois do mesmo tipo, valor fora da
    /// faixa do filtro) e o desentrelace por campo em fonte sem meio entre os quadros são acusados antes, no efeito.
    #[test]
    fn efeitos_de_origem_invalidos_sao_acusados_antes() {
        let nitidez = |id: &str, q: f64| com(base("nitidez", id), json!({ "quantidade": q }));
        let ajustes = |campo: &str, v: f64| {
            let mut a = com(base("ajustes", "e2"), json!({ "brilho": 0.0, "contraste": 1.0, "gama": 1.0, "saturacao": 1.0 }));
            a[campo] = json!(v);
            a
        };
        let lente = |cx: f64| com(base("lente", "e2"), json!({ "k1": -0.2, "k2": 0.0, "cx": cx, "cy": 0.5, "perfil": "" }));
        let trf = |nome: &str| {
            let mut e = estabilizar(0, 89);
            e["id"] = json!("e2");
            e["trf_sha256"] = json!(nome);
            e
        };
        let mut parcial = nitidez("e2", 0.5);
        parcial["ativo"] = json!({ "de": t(40), "ate": t(50) });
        let casos: Vec<(&str, Vec<Value>, Vec<(&'static str, Option<String>)>)> = vec![
            ("limpos", vec![nitidez("e2", 0.5)], vec![]),
            ("nitidez fora", vec![nitidez("e2", 5.5)], vec![(ITEM_INVALIDO, Some("quantidade".into()))]),
            ("brilho fora", vec![ajustes("brilho", 1.5)], vec![(ITEM_INVALIDO, Some("brilho".into()))]),
            ("gama fora", vec![ajustes("gama", 0.0)], vec![(ITEM_INVALIDO, Some("gama".into()))]),
            ("saturação fora", vec![ajustes("saturacao", 3.5)], vec![(ITEM_INVALIDO, Some("saturacao".into()))]),
            ("centro da lente fora", vec![lente(1.2)], vec![(ITEM_INVALIDO, Some("cx".into()))]),
            ("ativo parcial", vec![parcial], vec![(ITEM_INVALIDO, Some("ativo".into()))]),
            // o nome do `.trf` vira arquivo e texto do grafo: fora da forma do SHA-256, o motor recusa
            ("trf com nome de caminho", vec![trf("../../x")], vec![(ITEM_INVALIDO, Some("trf_sha256".into()))]),
            ("trf com texto de filtro", vec![trf("a.trf:optzoom=1")], vec![(ITEM_INVALIDO, Some("trf_sha256".into()))]),
            ("trf certo", vec![trf(SHA2)], vec![]),
        ];
        for (nome, efeitos, esperado) in casos {
            let mut c = cena();
            for e in &efeitos {
                efeito(&mut c, e.clone());
            }
            assert_eq!(do_efeito(&c, "e2"), esperado, "{nome}");
            // a regra é a do motor: `realce::aplicar` recusa exatamente esses
            let tipados: Vec<Tolerante<Efeito>> = serde_json::from_value(json!(efeitos)).unwrap();
            assert_eq!(realce::invalidos(&tipados, (1280, 720)).is_empty(), esperado.is_empty(), "{nome}");
        }
        // dois do mesmo tipo: o segundo
        let mut c = cena();
        efeito(&mut c, nitidez("e2", 0.5));
        efeito(&mut c, nitidez("e3", 0.7));
        assert_eq!((do_efeito(&c, "e2"), do_efeito(&c, "e3")), (vec![], vec![(ITEM_INVALIDO, None)]));
        // foto: a cadeia de origem não roda (é `nao_suportado`), sem `item_invalido` por cima
        // por campo em fonte de um tique por quadro (AVI): o motor recusa em `realce::leitura`
        let campo = com(base("desentrelacar", "e2"), json!({ "modo": "campo", "ordem": "auto" }));
        let mut c = cena();
        efeito(&mut c, campo.clone());
        assert!(!do_efeito(&c, "e2").iter().any(|(cod, _)| *cod == NAO_SUPORTADO), "passo de 512 tiques tem meio");
        let um_tique = apoio::tabela((1, 30), (0..300).collect());
        c.fontes.insert("m1".into(), fonte(um_tique));
        c.p["principal"][0]["inicio_pts"] = json!(30);
        c.p["principal"][0]["fim_pts"] = json!(90);
        assert_eq!(do_efeito(&c, "e2"), vec![(NAO_SUPORTADO, None)]);
        assert!(c.problemas().iter().any(|p| p.mensagem.contains(mensagens::frase(NAO_SUPORTADO, None, None).as_str())));
        // por quadro, na mesma fonte: aceito
        c.p["principal"][0]["efeitos"] = json!([com(base("desentrelacar", "e2"), json!({ "modo": "quadro", "ordem": "auto" }))]);
        assert_eq!(do_efeito(&c, "e2"), vec![]);
    }

    /// Lupa: o que o motor recusa com `item_invalido` (`lupa::campo_invalido`: ampliação fora de 1,5..8, janela fora de
    /// (0, 1] ou com menos de 8 px da fonte) é acusado antes, no campo.
    #[test]
    fn lupa_invalida_e_acusada_antes() {
        let lupa_com = |campo: &str, v: f64| {
            let mut l = ef_do_tipo("lupa");
            l["id"] = json!("e2");
            l["ativo"] = json!(null);
            l["rastreio"] = json!("r1");
            l[campo] = json!(v);
            l
        };
        for (campo, v, esperado) in [("fator", 3.0, None), ("fator", 9.0, Some("fator")), ("fator", 1.0, Some("fator")), ("janela", 1.5, Some("janela")), ("janela", 0.005, Some("janela"))] {
            let mut c = cena();
            c.p["rastreios"] = json!([rastreio(&[30])]);
            let l = lupa_com(campo, v);
            efeito(&mut c, l.clone());
            let invalidos: Vec<_> = do_efeito(&c, "e2").into_iter().filter(|(cod, _)| *cod == ITEM_INVALIDO).collect();
            assert_eq!(invalidos, esperado.map(|x| (ITEM_INVALIDO, Some(x.to_string()))).into_iter().collect::<Vec<_>>(), "{campo} = {v}");
            let Efeito::Lupa(tipada) = serde_json::from_value::<Efeito>(l).unwrap() else { panic!() };
            assert_eq!(lupa::campo_invalido(&tipada, TamanhoPx { w: 1280, h: 720 }), esperado, "{campo} = {v}");
        }
    }
}
