//! Importador v1 → v2 (§2.4): `Sistema/videos/montagens/<id>.json` vira `edicao/<id>/projeto.json`. Dono: P0.2.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use chrono::{SecondsFormat, Utc};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::armazem;
use super::mensagens::codigo;
use super::modelo::{
    Abaixar, Ajuste, Alinhamento, AlinhamentoV, AlvoRegiao, Animavel, AudioClipe, AudioTransicao, BaseRastreio, BlocoTexto,
    Caixa, CaixaTexto, Chave, ClipeBase, ClipeAudio, ClipeCongelar, ClipePrincipal, ClipeTitulo, ClipeTrecho, ConteudoOculto,
    Derivacao, EfBase, EfCarimbo, EfDestaque, EfEnquadramento, EfOcultar, EfTexto, Efeito, EscolhaCodificador, EstiloTexto,
    Exportacao, Fonte, FormaDestaque, FormaRegiao, FormatoCarimbo, Fundo, Interp, Interpolacao, Intervalo, Lugar, MidiaRef,
    Mixagem, ModeloTitulo, ModoLegendas, ModoOcultar, ModoPdf, MostraCarimbo, OrigemMidia, OrigemRastreio, PapelAudio,
    PapelTexto, Posicao, Predefinicao, ProjetoEdicao, Racional, Rastreio, Recebido, Saida, Selos, TipoDerivacao, TipoMidia,
    TipoRastreio, Titulo, Tolerante, Traco, TrilhaAudio, TrilhaLegendas, Transicao, Velocidade, ESQUEMA, ESQUEMA_MENOR,
};
use crate::database::repositories::video_repo;
use crate::error::{Result, SicroError};
use crate::models::VideoMedia;
use crate::workspace::manifest::APP_VERSION;
use crate::workspace::paths;

/// O que a conversão precisa de fora do arquivo v1.
pub trait FontesV1 {
    /// `MidiaRef` do vídeo do caso com esse `media_id`; None = mídia sumida.
    fn video(&self, media_id: &str) -> Option<MidiaRef>;
    /// `MidiaRef` (origem projeto) do áudio de narração em `relativo`.
    fn narracao(&self, relativo: &str) -> Option<MidiaRef>;
    /// pts do quadro exibido em `t` (s, tempo de fonte) na mídia `sha256`; None sem tabela.
    fn pts(&self, sha256: &str, t: f64) -> Option<i64>;
    /// (largura, altura) exibidas da mídia `sha256`, em px.
    fn tamanho(&self, sha256: &str) -> Option<(u32, u32)>;
}

/// Há um projeto v1 com esse id ainda não importado?
pub fn existe(ws: &Path, projeto_id: &str) -> bool {
    armazem::id_valido(projeto_id) && paths::dir_montagens_json(ws).join(format!("{projeto_id}.json")).is_file()
}

/// Conversão pura, campo a campo (§2.4). `agora` em RFC 3339; `versao` = versão do SICRO (`gravado_por`).
pub fn converter(v1: &Value, fontes: &dyn FontesV1, agora: &str, versao: &str) -> Result<ProjetoEdicao> {
    let m: V1Montagem = serde_json::from_value(v1.clone())
        .map_err(|e| SicroError::Validation(format!("{}: {e}", codigo::ESQUEMA)))?;
    Conversao { m: &m, fontes, midias: Vec::new(), por_chave: HashMap::new(), rastreios: Vec::new() }.projeto(agora, versao)
}

/// Lê o v1, converte, grava o v2 e renomeia o v1 para `<id>.json.v1` (os bytes não mudam). Devolve o documento gravado.
/// Leitura que falha (banco, narração) não grava nada e deixa o v1 como está: a próxima abertura tenta de novo.
pub fn importar(ws: &Path, conn: &Connection, projeto_id: &str) -> Result<Value> {
    if !armazem::id_valido(projeto_id) {
        return Err(SicroError::Validation(codigo::ESQUEMA.into()));
    }
    let v1_arq = paths::dir_montagens_json(ws).join(format!("{projeto_id}.json"));
    let destino = armazem::pasta_projeto(ws, projeto_id).join("projeto.json");
    if destino.exists() {
        return Err(SicroError::Validation(format!("{}: já importado", codigo::ESQUEMA)));
    }
    let v1: Value = serde_json::from_slice(&std::fs::read(&v1_arq)?)
        .map_err(|e| SicroError::Validation(format!("{}: {e}", codigo::ESQUEMA)))?;
    let fontes = FontesBanco { ws, conn, tamanhos: RefCell::new(HashMap::new()), erro: RefCell::new(None) };
    let agora = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut p = converter(&v1, &fontes, &agora, APP_VERSION)?;
    if let Some(e) = fontes.erro.into_inner() {
        return Err(e);
    }
    // a pasta é pelo nome do arquivo
    p.id = projeto_id.to_string();
    let doc = serde_json::to_value(&p)?;
    armazem::gravar_atomico(&destino, &serde_json::to_vec_pretty(&doc)?)?;
    // o v2 já está gravado e a lista o prefere ao v1 de mesmo id: renomear que falha não perde nada
    if let Err(e) = std::fs::rename(&v1_arq, v1_arq.with_extension("json.v1")) {
        tracing::warn!("v1 importado não renomeado ({}): {e}", v1_arq.display());
    }
    Ok(doc)
}

// ---------------------------------------------------------------------------
// Formato do arquivo v1, lido aqui sem o código da v1 (que saiu na Fase 1). Conferido contra uma montagem gravada
// pela própria v1 (`tests/fixtures/edicao/v1-salvo-pela-v1.json`).

fn um() -> f64 {
    1.0
}

#[derive(Deserialize)]
struct V1Montagem {
    id: String,
    titulo: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
    itens: Vec<V1Item>,
    saida: V1Saida,
    #[serde(default)]
    narracoes: Vec<V1Narracao>,
    #[serde(default = "um")]
    volume_original: f64,
}

#[derive(Deserialize)]
struct V1Saida {
    largura: u32,
    altura: u32,
    fps: u32,
    incluir_audio: bool,
}

#[derive(Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
enum V1Item {
    Trecho {
        id: String,
        media_id: String,
        inicio_s: f64,
        fim_s: f64,
        velocidade: f64,
        #[serde(default)]
        texto: Option<V1Texto>,
        #[serde(default)]
        transicao_entrada: Option<String>,
        #[serde(default)]
        zoom: Option<V1Zoom>,
        #[serde(default)]
        destaques: Vec<V1Destaque>,
        #[serde(default)]
        desfoques: Vec<V1Desfoque>,
    },
    Cartela {
        id: String,
        duracao_s: f64,
        texto: String,
        cor_fundo: String,
        cor_texto: String,
        tamanho: String,
        #[serde(default)]
        transicao_entrada: Option<String>,
    },
    Congelar {
        id: String,
        media_id: String,
        instante_s: f64,
        duracao_s: f64,
        #[serde(default)]
        texto: Option<V1Texto>,
    },
}

#[derive(Deserialize)]
struct V1Texto {
    conteudo: String,
    posicao: String,
    tamanho: String,
    #[serde(default)]
    carimbo_tempo: bool,
}

#[derive(Deserialize, Clone, Copy)]
struct V1Regiao {
    x: f64,
    y: f64,
    w: f64,
    #[allow(dead_code)]
    h: f64,
}

#[derive(Deserialize)]
struct V1Zoom {
    inicio: V1Regiao,
    fim: V1Regiao,
}

/// `t` em s desde o início do trecho; círculo = centro, seta = ponta, retângulo e desfoque = canto superior esquerdo.
#[derive(Deserialize, Clone)]
struct V1Quadro {
    t: f64,
    x: f64,
    y: f64,
    #[serde(default)]
    w: Option<f64>,
    #[serde(default)]
    h: Option<f64>,
}

#[derive(Deserialize)]
struct V1Destaque {
    id: String,
    forma: String,
    cor: String,
    quadros: Vec<V1Quadro>,
}

#[derive(Deserialize)]
struct V1Desfoque {
    id: String,
    quadros: Vec<V1Quadro>,
}

#[derive(Deserialize)]
struct V1Narracao {
    id: String,
    arquivo_relativo: String,
    inicio_s: f64,
    volume: f64,
    nome: String,
}

// ---------------------------------------------------------------------------
// Conversão

struct Conversao<'a> {
    m: &'a V1Montagem,
    fontes: &'a dyn FontesV1,
    midias: Vec<MidiaRef>,
    /// `media_id` ou `relativo` da narração → índice em `midias`
    por_chave: HashMap<String, usize>,
    rastreios: Vec<Rastreio>,
}

impl Conversao<'_> {
    fn projeto(mut self, agora: &str, versao: &str) -> Result<ProjetoEdicao> {
        let m = self.m;
        let duracoes: Vec<f64> = m.itens.iter().map(duracao_v1).collect();
        let mut principal = Vec::with_capacity(m.itens.len());
        for (i, it) in m.itens.iter().enumerate() {
            let antes = i.checked_sub(1).map(|j| duracoes[j]);
            let c = self.item(it, antes, duracoes[i]);
            principal.push(Tolerante::Conhecido(c));
        }
        let audios = self.narracoes();
        let quando = |s: &str| if s.is_empty() { agora.to_string() } else { s.to_string() };
        Ok(ProjetoEdicao {
            esquema: ESQUEMA,
            esquema_menor: ESQUEMA_MENOR,
            id: m.id.clone(),
            titulo: m.titulo.clone(),
            created_at: quando(&m.created_at),
            updated_at: quando(&m.updated_at),
            gravado_por: versao.to_string(),
            saida: Saida {
                largura: m.saida.largura,
                altura: m.saida.altura,
                fps: Racional { num: m.saida.fps as i64, den: 1 },
                auto: false,
                cor_fundo: "#000000".into(),
                audio: m.saida.incluir_audio,
                selos: Selos { velocidade: true, ampliacao: true, realce: true },
                exportacao: Exportacao {
                    predefinicao: Predefinicao::Juri,
                    codificador: EscolhaCodificador::Auto,
                    legendas: ModoLegendas::Nenhuma,
                    pacote: None,
                    pdf: ModoPdf::A3,
                    revisado_por: String::new(),
                    personalizada: None,
                },
            },
            midias: self.midias,
            principal,
            fecho: None,
            sobreposicoes: Vec::new(),
            audios,
            legendas: TrilhaLegendas::default(),
            rastreios: self.rastreios,
            marcadores: Vec::new(),
            sincronias: Vec::new(),
            mixagem: Mixagem {
                volume_original: m.volume_original,
                abaixar: Abaixar { db: -12.0, ataque_s: 0.3, soltura_s: 0.5 },
                normalizar: None,
                limitador: false,
            },
        })
    }

    /// Id derivado (formato v4) do projeto e do item: importar duas vezes dá o mesmo documento.
    fn id(&self, partes: &[&str]) -> String {
        let mut h = Sha256::new();
        h.update(self.m.id.as_bytes());
        for p in partes {
            h.update([0]);
            h.update(p.as_bytes());
        }
        let d = h.finalize();
        let mut b = [0u8; 16];
        b.copy_from_slice(&d[..16]);
        uuid::Builder::from_random_bytes(b).into_uuid().to_string()
    }

    fn item(&mut self, it: &V1Item, antes: Option<f64>, dur: f64) -> ClipePrincipal {
        match it {
            V1Item::Trecho { id, media_id, inicio_s, fim_s, velocidade, texto, transicao_entrada, zoom, destaques, desfoques } => {
                let (midia, sha) = self.video(media_id);
                let inicio_pts = self.pts(&sha, *inicio_s);
                let fim_pts = self.pts(&sha, *fim_s);
                let mut efeitos = Vec::new();
                if let Some(z) = zoom {
                    let caixa = |r: V1Regiao| self.regiao(r, &sha);
                    efeitos.push(Efeito::Enquadramento(EfEnquadramento {
                        base: ef_base(self.id(&["zoom", id]), None),
                        regiao: Animavel::Chaves(vec![
                            Chave { t: *inicio_s, pts: None, v: caixa(z.inicio), interp: Interp::Linear },
                            Chave { t: *fim_s, pts: None, v: caixa(z.fim), interp: Interp::Linear },
                        ]),
                        ajuste: Ajuste::Cobrir,
                        interpolacao: Interpolacao::Bicubica,
                    }));
                }
                for d in destaques {
                    efeitos.push(self.destaque(d, &midia, *inicio_s));
                }
                for d in desfoques {
                    self.desfoque(d, &midia, *inicio_s, *fim_s);
                }
                efeitos.extend(self.textos(id, texto.as_ref()));
                ClipePrincipal::Trecho(ClipeTrecho {
                    base: clipe_base(id, transicao(transicao_entrada.as_deref(), antes, dur), efeitos),
                    midia,
                    inicio_s: *inicio_s,
                    fim_s: *fim_s,
                    inicio_pts,
                    fim_pts,
                    velocidade: Velocidade::Constante { fator: *velocidade },
                    audio: AudioClipe::default(),
                })
            }
            V1Item::Cartela { id, duracao_s, texto, cor_fundo, cor_texto, tamanho, transicao_entrada } => {
                let mut blocos = Vec::new();
                if !texto.trim().is_empty() {
                    blocos.push(BlocoTexto {
                        id: self.id(&["bloco", id]),
                        papel: PapelTexto::Corpo,
                        conteudo: texto.clone(),
                        estilo: EstiloTexto {
                            fonte: Fonte::Sans,
                            negrito: false,
                            italico: false,
                            tamanho: tamanho_v1(tamanho),
                            cor: cor_texto.clone(),
                            alinhamento: Alinhamento::Centro,
                            caixa: None,
                            contorno: None,
                        },
                        lugar: Lugar { x: 0.5, y: 0.5, h: Alinhamento::Centro, v: AlinhamentoV::Meio },
                        largura_max: 0.9,
                    });
                }
                ClipePrincipal::Titulo(ClipeTitulo {
                    base: clipe_base(id, transicao(transicao_entrada.as_deref(), antes, dur), Vec::new()),
                    duracao_s: *duracao_s,
                    titulo: Titulo { modelo: ModeloTitulo::Cartela, fundo: Fundo::Cor { cor: cor_fundo.clone() }, blocos },
                })
            }
            V1Item::Congelar { id, media_id, instante_s, duracao_s, texto } => {
                let (midia, sha) = self.video(media_id);
                let efeitos = self.textos(id, texto.as_ref());
                ClipePrincipal::Congelar(ClipeCongelar {
                    base: clipe_base(id, None, efeitos),
                    midia,
                    instante_s: *instante_s,
                    instante_pts: self.pts(&sha, *instante_s),
                    duracao_s: *duracao_s,
                })
            }
        }
    }

    /// `MidiaRef.id` e SHA-256 do vídeo; mídia sumida vira `MidiaRef` só com `media_id`.
    fn video(&mut self, media_id: &str) -> (String, String) {
        if let Some(&i) = self.por_chave.get(media_id) {
            return (self.midias[i].id.clone(), self.midias[i].sha256.clone());
        }
        let id = self.id(&["midia", media_id]);
        let mut r = self.fontes.video(media_id).unwrap_or_else(|| sumida(TipoMidia::Video, OrigemMidia::Caso, ""));
        r.id = id;
        r.media_id = Some(media_id.to_string());
        let par = (r.id.clone(), r.sha256.clone());
        self.por_chave.insert(media_id.to_string(), self.midias.len());
        self.midias.push(r);
        par
    }

    fn pts(&self, sha: &str, t: f64) -> Option<i64> {
        if sha.is_empty() {
            None
        } else {
            self.fontes.pts(sha, t)
        }
    }

    /// Zoom da v1 (região do canvas, `h` ignorado) → região da origem: `w` em 0,1..1,
    /// `h = w × proporção da fonte ÷ proporção da saída`.
    fn regiao(&self, r: V1Regiao, sha: &str) -> Caixa {
        let w = if r.w.is_finite() { r.w.clamp(0.1, 1.0) } else { 1.0 };
        let saida = self.m.saida.largura as f64 / self.m.saida.altura.max(1) as f64;
        let fonte = self
            .fontes
            .tamanho(sha)
            .filter(|&(fw, fh)| fw > 0 && fh > 0)
            .map(|(fw, fh)| fw as f64 / fh as f64)
            .unwrap_or(saida);
        Caixa { x: r.x, y: r.y, w, h: w * fonte / saida }
    }

    fn rastreio(&mut self, id: String, nome: &str, cor: &str, midia: &str, tipo: TipoRastreio, ancoras: Vec<Chave<Caixa>>) {
        self.rastreios.push(Rastreio {
            id,
            nome: nome.into(),
            cor: cor.into(),
            midia: Some(midia.into()),
            base: BaseRastreio::Fonte,
            tipo,
            espaco: "original".into(),
            ancoras,
            denso: None,
            silhuetas: None,
            ausencias: Vec::new(),
            origem: OrigemRastreio::Manual,
            confirmacao: None,
            ancoras_do_programa: Vec::new(),
            corridas_do_programa: Vec::new(),
        });
    }

    fn destaque(&mut self, d: &V1Destaque, midia: &str, inicio_s: f64) -> Efeito {
        let forma = match d.forma.as_str() {
            "seta" => FormaDestaque::Seta,
            "retangulo" => FormaDestaque::Retangulo,
            _ => FormaDestaque::Circulo,
        };
        let caixa = forma == FormaDestaque::Retangulo;
        let quadros = ordenados(&d.quadros);
        let rid = self.id(&["rastreio", &d.id]);
        let tipo = if caixa { TipoRastreio::Caixa } else { TipoRastreio::Ponto };
        self.rastreio(rid.clone(), "Destaque", &d.cor, midia, tipo, ancoras(&quadros, inicio_s, caixa));
        let ativo = quadros.first().map(|q| Intervalo { de: Some(inicio_s + q.t), ate: None });
        Efeito::Destaque(EfDestaque {
            base: ef_base(d.id.clone(), ativo),
            forma,
            rastreio: rid,
            traco: Traco { cor: d.cor.clone(), espessura: 0.006, contorno: !caixa },
            tamanho: if forma == FormaDestaque::Seta { 0.14 } else { 0.12 },
            angulo_graus: 45.0,
            margem: 0.0,
            rotulo: None,
        })
    }

    /// Vira ocultação da MÍDIA (vale em todo uso dela), dentro de [primeiro quadro, fim do trecho].
    fn desfoque(&mut self, d: &V1Desfoque, midia: &str, inicio_s: f64, fim_s: f64) {
        let quadros = ordenados(&d.quadros);
        let rid = self.id(&["rastreio", &d.id]);
        self.rastreio(rid.clone(), "Ocultar", "#ff2d2d", midia, TipoRastreio::Caixa, ancoras(&quadros, inicio_s, true));
        let ativo = quadros.first().map(|q| Intervalo { de: Some(inicio_s + q.t), ate: Some(fim_s) });
        let oc = EfOcultar {
            base: ef_base(d.id.clone(), ativo),
            modo: ModoOcultar::Desfoque,
            alvo: AlvoRegiao {
                rastreio: rid,
                forma: FormaRegiao::Retangulo,
                vertices: None,
                margem: 0.0,
                borda_suave: 0.0,
                inverter: false,
            },
            intensidade: 0.6,
            cor: "#000000ff".into(),
            conteudo: ConteudoOculto::Outro,
            motivo: String::new(),
        };
        if let Some(m) = self.midias.iter_mut().find(|m| m.id == midia) {
            m.ocultacoes.push(oc);
        }
    }

    /// Carimbo e texto do trecho ou do congelado, nessa ordem (o texto por cima, como na v1).
    fn textos(&self, item_id: &str, texto: Option<&V1Texto>) -> Vec<Efeito> {
        let Some(t) = texto else { return Vec::new() };
        let mut efeitos = Vec::new();
        if t.carimbo_tempo {
            efeitos.push(Efeito::Carimbo(EfCarimbo {
                base: ef_base(self.id(&["carimbo", item_id]), None),
                mostra: MostraCarimbo::TempoFonte,
                formato: FormatoCarimbo::HmsMs,
                relogio: None,
                estilo: estilo(Fonte::Mono, 0.033, Alinhamento::Dir, 0.0074),
                lugar: Lugar { x: 0.984, y: 0.028, h: Alinhamento::Dir, v: AlinhamentoV::Topo },
            }));
        }
        if !t.conteudo.trim().is_empty() {
            let lugar = match t.posicao.as_str() {
                "topo" => Lugar { x: 0.5, y: 0.06, h: Alinhamento::Centro, v: AlinhamentoV::Topo },
                "centro" => Lugar { x: 0.5, y: 0.5, h: Alinhamento::Centro, v: AlinhamentoV::Meio },
                _ => Lugar { x: 0.5, y: 0.94, h: Alinhamento::Centro, v: AlinhamentoV::Base },
            };
            efeitos.push(Efeito::Texto(EfTexto {
                base: ef_base(self.id(&["texto", item_id]), None),
                conteudo: t.conteudo.clone(),
                estilo: estilo(Fonte::Sans, tamanho_v1(&t.tamanho), Alinhamento::Centro, 0.013),
                lugar,
                largura_max: 0.92,
            }));
        }
        efeitos
    }

    /// Uma trilha "Narração" com um clipe por narração; o áudio entra em `midias` (origem projeto).
    fn narracoes(&mut self) -> Vec<TrilhaAudio> {
        if self.m.narracoes.is_empty() {
            return Vec::new();
        }
        let m = self.m;
        let mut clipes = Vec::new();
        for n in &m.narracoes {
            let rel = n.arquivo_relativo.as_str();
            let midia = match self.por_chave.get(rel) {
                Some(&i) => self.midias[i].id.clone(),
                None => {
                    let mut r = self
                        .fontes
                        .narracao(rel)
                        .unwrap_or_else(|| sumida(TipoMidia::Audio, OrigemMidia::Projeto, rel));
                    r.id = self.id(&["narracao", rel]);
                    r.relativo = rel.to_string();
                    self.por_chave.insert(rel.to_string(), self.midias.len());
                    let id = r.id.clone();
                    self.midias.push(r);
                    id
                }
            };
            clipes.push(ClipeAudio {
                id: n.id.clone(),
                nome: n.nome.clone(),
                midia,
                pos: Posicao::Programa { t: n.inicio_s },
                de_s: 0.0,
                ate_s: None,
                volume: Animavel::Fixo(n.volume),
                entrada_s: 0.0,
                saida_s: 0.0,
            });
        }
        vec![TrilhaAudio {
            id: self.id(&["trilha", "narracao"]),
            nome: "Narração".into(),
            papel: PapelAudio::Narracao,
            volume: 1.0,
            mudo: false,
            abaixa_original: false,
            clipes,
        }]
    }
}

/// Duração do item na saída, como `duracaoItem` da v1.
fn duracao_v1(it: &V1Item) -> f64 {
    let d = match it {
        V1Item::Trecho { inicio_s, fim_s, velocidade, .. } => {
            (fim_s - inicio_s) / if *velocidade > 0.0 { *velocidade } else { 1.0 }
        }
        V1Item::Cartela { duracao_s, .. } | V1Item::Congelar { duracao_s, .. } => *duracao_s,
    };
    if d.is_finite() { d.max(0.0) } else { 0.0 }
}

/// "fade" → escurecer com min(0,5; quem sai ÷ 2; quem entra ÷ 2); "corte", ausente ou outro → sem transição.
fn transicao(v1: Option<&str>, antes: Option<f64>, dur: f64) -> Option<Transicao> {
    (v1 == Some("fade")).then(|| {
        let mut d = (0.5f64).min(dur / 2.0);
        if let Some(a) = antes {
            d = d.min(a / 2.0);
        }
        Transicao { tipo: "escurecer".into(), duracao_s: d, audio: AudioTransicao::Acompanhar }
    })
}

fn tamanho_v1(t: &str) -> f64 {
    match t {
        "p" => 0.033,
        "g" => 0.078,
        _ => 0.052,
    }
}

fn estilo(fonte: Fonte, tamanho: f64, alinhamento: Alinhamento, margem: f64) -> EstiloTexto {
    EstiloTexto {
        fonte,
        negrito: false,
        italico: false,
        tamanho,
        cor: "#ffffff".into(),
        alinhamento,
        caixa: Some(CaixaTexto { cor: "#0000008c".into(), margem, raio: 0.0 }),
        contorno: None,
    }
}

fn ef_base(id: String, ativo: Option<Intervalo>) -> EfBase {
    EfBase { id, ligado: true, nome: String::new(), ativo }
}

fn clipe_base(id: &str, transicao_entrada: Option<Transicao>, efeitos: Vec<Efeito>) -> ClipeBase {
    ClipeBase {
        id: id.to_string(),
        rotulo: String::new(),
        desligado: false,
        transicao_entrada,
        efeitos: efeitos.into_iter().map(Tolerante::Conhecido).collect(),
    }
}

fn ordenados(q: &[V1Quadro]) -> Vec<V1Quadro> {
    let mut q = q.to_vec();
    q.sort_by(|a, b| a.t.total_cmp(&b.t));
    q
}

/// Âncoras em tempo de fonte; ponto = caixa de lado 0. O `pts` fica para `Analise.prender`.
fn ancoras(q: &[V1Quadro], inicio_s: f64, caixa: bool) -> Vec<Chave<Caixa>> {
    q.iter()
        .map(|k| Chave {
            t: inicio_s + k.t,
            pts: None,
            v: Caixa {
                x: k.x,
                y: k.y,
                w: if caixa { k.w.unwrap_or(0.0) } else { 0.0 },
                h: if caixa { k.h.unwrap_or(0.0) } else { 0.0 },
            },
            interp: Interp::Linear,
        })
        .collect()
}

/// Mídia que o banco ou o disco não têm: só o que se sabe (a validação acusa `midia_ausente`).
fn sumida(tipo: TipoMidia, origem: OrigemMidia, relativo: &str) -> MidiaRef {
    MidiaRef {
        id: String::new(),
        tipo,
        origem,
        media_id: None,
        sha256: String::new(),
        nome: Path::new(relativo).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        relativo: relativo.to_string(),
        duracao_s: None,
        inicio_s: None,
        largura: None,
        altura: None,
        taxa: None,
        vfr: false,
        tem_audio: tipo == TipoMidia::Audio,
        cor: None,
        derivacao: None,
        recebido: Recebido { nativo: None, obs: String::new() },
        relogio_desvio_s: None,
        gravador: None,
        decodificacao: None,
        ocultacoes: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Fontes reais: banco do caso e disco

struct FontesBanco<'a> {
    ws: &'a Path,
    conn: &'a Connection,
    tamanhos: RefCell<HashMap<String, (u32, u32)>>,
    /// Primeiro erro de leitura (o trait só devolve Option): `importar` falha com ele.
    erro: RefCell<Option<SicroError>>,
}

impl FontesBanco<'_> {
    fn ou_guarda<T>(&self, r: Result<T>) -> Option<T> {
        r.map_err(|e| {
            self.erro.borrow_mut().get_or_insert(e);
        })
        .ok()
    }
}

impl FontesV1 for FontesBanco<'_> {
    fn video(&self, media_id: &str) -> Option<MidiaRef> {
        // id fora do formato: a v1 também não achava a mídia
        let uuid = Uuid::parse_str(media_id).ok()?;
        let m = self.ou_guarda(video_repo::find_media_by_id(self.conn, &uuid))??;
        let bruto: Value = serde_json::from_str(&m.raw_probe_json).unwrap_or(Value::Null);
        let (largura, altura) = tamanho_exibido(&bruto).map(|(w, h)| (Some(w), Some(h))).unwrap_or((m.width, m.height));
        if let (Some(w), Some(h)) = (largura, altura) {
            self.tamanhos.borrow_mut().insert(m.sha256.clone(), (w, h));
        }
        let (nativo, obs, relogio_desvio_s, gravador) = self.ou_guarda(recebimento(self.conn, &m.id))?;
        let derivacao = self.ou_guarda(derivacao(self.conn, &m))?;
        Some(MidiaRef {
            id: String::new(),
            tipo: TipoMidia::Video,
            origem: OrigemMidia::Caso,
            media_id: Some(media_id.to_string()),
            sha256: m.sha256.clone(),
            nome: m.filename.clone(),
            relativo: m.relative_path.clone(),
            duracao_s: m.duration_s,
            inicio_s: Some(crate::video::probe::container_start_time(&m.raw_probe_json)),
            largura,
            altura,
            // a taxa nominal sai da tabela de pts (nunca do avg_frame_rate): fica para a análise
            taxa: None,
            vfr: false,
            tem_audio: tem_audio(&bruto),
            cor: None,
            derivacao,
            recebido: Recebido { nativo, obs },
            relogio_desvio_s,
            gravador,
            decodificacao: None,
            ocultacoes: Vec::new(),
        })
    }

    fn narracao(&self, relativo: &str) -> Option<MidiaRef> {
        // a regra do executor: no Windows `C:x` e `\x` não são absolutos e o `join` sairia do caso
        if !super::executar::relativo_seguro(relativo) {
            return None;
        }
        let abs = paths::resolver_relativo(self.ws, relativo);
        if !abs.is_file() {
            return None;
        }
        let sha256 = self.ou_guarda(crate::hashing::sha256::sha256_file(&abs))?;
        let mut r = sumida(TipoMidia::Audio, OrigemMidia::Projeto, relativo);
        r.sha256 = sha256;
        if let Ok(p) = crate::video::probe_media(&abs) {
            r.duracao_s = p.duration_s;
            r.inicio_s = Some(crate::video::probe::container_start_time(&p.raw_json));
        }
        Some(r)
    }

    // Tabela do decodificador: mora no cache do app (P0.3b/P0.7); daqui não se alcança. `Analise.prender` preenche.
    fn pts(&self, _sha256: &str, _t: f64) -> Option<i64> {
        None
    }

    fn tamanho(&self, sha256: &str) -> Option<(u32, u32)> {
        self.tamanhos.borrow().get(sha256).copied()
    }
}

/// Tamanho exibido pelo primeiro fluxo de vídeo do ffprobe guardado (giro e proporção do pixel aplicados,
/// como em `video/pessoas/decode.rs`).
fn tamanho_exibido(bruto: &Value) -> Option<(u32, u32)> {
    let s = bruto["streams"].as_array()?.iter().find(|s| s["codec_type"].as_str() == Some("video"))?;
    let (w, h) = (s["width"].as_f64()?, s["height"].as_f64()?);
    if w < 2.0 || h < 2.0 {
        return None;
    }
    let sar = s["sample_aspect_ratio"]
        .as_str()
        .and_then(|r| {
            let (a, b) = r.split_once(':')?;
            let (a, b) = (a.parse::<f64>().ok()?, b.parse::<f64>().ok()?);
            (a > 0.0 && b > 0.0).then_some(a / b)
        })
        .unwrap_or(1.0);
    let rot = s["side_data_list"]
        .as_array()
        .and_then(|l| l.iter().find_map(|d| d["rotation"].as_f64()))
        .or_else(|| s["tags"]["rotate"].as_str().and_then(|r| r.parse::<f64>().ok()))
        .unwrap_or(0.0);
    let w = (w * sar).round();
    let (w, h) = if (rot.abs() as i64) % 180 == 90 { (h, w) } else { (w, h) };
    let par = |v: f64| ((v as u32) / 2 * 2).max(2);
    Some((par(w), par(h)))
}

fn tem_audio(bruto: &Value) -> bool {
    bruto["streams"].as_array().is_some_and(|s| s.iter().any(|s| s["codec_type"].as_str() == Some("audio")))
}

/// Colunas da migração 027 (`recebido_*`, desvio do relógio, gravador), pelas regras de `video_proveniencia`:
/// erro de leitura e valor que a tela não grava sobem, nunca viram "não informado".
fn recebimento(conn: &Connection, id: &Uuid) -> Result<(Option<bool>, String, Option<f64>, Option<String>)> {
    let (nativo, obs, desvio, gravador) = conn.query_row(
        "SELECT recebido_nativo, recebido_obs, relogio_desvio_s, gravador FROM video_media WHERE id = ?1",
        [id.to_string()],
        |r| {
            Ok((
                r.get::<_, Option<i64>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<f64>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        },
    )?;
    let nativo = match nativo {
        None => None,
        Some(0) => Some(false),
        Some(1) => Some(true),
        Some(_) => return Err(SicroError::Database(format!("video_media.recebido_nativo fora de 0/1 ({id})"))),
    };
    if desvio.is_some_and(|d| !d.is_finite()) {
        return Err(SicroError::Database(format!("video_media.relogio_desvio_s não finito ({id})")));
    }
    Ok((nativo, obs.unwrap_or_default(), desvio, gravador.filter(|g| !g.is_empty())))
}

/// Retrato de `derived_from_hash` + `derivation_json`, seguindo a cadeia até a raiz. JSON ilegível e erro do banco sobem.
pub(crate) fn derivacao(conn: &Connection, m: &VideoMedia) -> Result<Option<Derivacao>> {
    let Some(de) = m.derived_from_hash.clone() else { return Ok(None) };
    let j: Value = match m.derivation_json.as_deref() {
        None => Value::Null,
        Some(s) => serde_json::from_str(s)
            .map_err(|e| SicroError::Database(format!("video_media.derivation_json ilegível ({}): {e}", m.id)))?,
    };
    let tipo = match j["kind"].as_str() {
        Some("clip") => TipoDerivacao::Trecho,
        Some("estabilizado") => TipoDerivacao::Estabilizado,
        Some("lente_video") => TipoDerivacao::Lente,
        // P4.6: remux (ou FFV1 do fluxo cru) com a lista `framemd5` igual à do original
        Some("copia_dvr") => TipoDerivacao::CopiaDvr,
        _ => TipoDerivacao::Outro,
    };
    let com_perdas = !(tipo == TipoDerivacao::CopiaDvr || (tipo == TipoDerivacao::Trecho && j["mode"].as_str() == Some("copy")));
    let inicio_epoca_s = j["inicio_epoca_s"].as_f64().filter(|s| tipo == TipoDerivacao::CopiaDvr && s.is_finite());
    let comando = ["ffmpeg_command", "comando", "comandos"]
        .iter()
        .find_map(|k| match &j[*k] {
            Value::String(s) => Some(s.clone()),
            Value::Array(a) => Some(
                a.iter()
                    .map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string()))
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            _ => None,
        })
        .unwrap_or_default();
    let mut raiz = de.clone();
    for _ in 0..64 {
        match video_repo::find_media_by_sha256(conn, &m.occurrence_id, &raiz)? {
            Some(x) => match x.derived_from_hash {
                Some(h) if h != raiz => raiz = h,
                _ => break,
            },
            // a origem saiu do caso: a raiz é a última conhecida
            None => break,
        }
    }
    Ok(Some(Derivacao { tipo, de_sha256: de, raiz_sha256: raiz, comando, com_perdas, inicio_epoca_s }))
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;

    const M1: &str = "11111111-2222-4333-8444-555555555551";
    const M2: &str = "11111111-2222-4333-8444-555555555552";
    const M3: &str = "11111111-2222-4333-8444-555555555553";
    const AGORA: &str = "2026-11-14T05:00:00.000Z";

    /// M1 e M2 no banco (M1 em 4:3), M3 sumida; pts na base 1/1000; narrações que existem por `relativo`.
    struct Falsas;
    impl FontesV1 for Falsas {
        fn video(&self, media_id: &str) -> Option<MidiaRef> {
            let n = match media_id {
                M1 => 1,
                M2 => 2,
                _ => return None,
            };
            let mut r = sumida(TipoMidia::Video, OrigemMidia::Caso, &format!("Mídias/Vídeos/cam {n}.mp4"));
            r.id = "qualquer".into();
            r.sha256 = n.to_string().repeat(64);
            r.duracao_s = Some(60.0);
            Some(r)
        }
        fn narracao(&self, relativo: &str) -> Option<MidiaRef> {
            (!relativo.contains("sumiu")).then(|| {
                let mut r = sumida(TipoMidia::Audio, OrigemMidia::Projeto, relativo);
                r.sha256 = format!("{:x}", Sha256::digest(relativo.as_bytes()));
                r
            })
        }
        fn pts(&self, _sha256: &str, t: f64) -> Option<i64> {
            Some((t * 1000.0).round() as i64)
        }
        fn tamanho(&self, sha256: &str) -> Option<(u32, u32)> {
            (sha256 == "1".repeat(64)).then_some((1440, 1080))
        }
    }

    fn converte(nome: &str) -> ProjetoEdicao {
        let texto = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edicao").join(nome)).unwrap();
        let p = converter(&serde_json::from_str(&texto).unwrap(), &Falsas, AGORA, "6.0.0").unwrap();
        // o documento gerado lê tipado e volta igual
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(armazem::tipado(&v).unwrap(), p);
        p
    }

    fn clipes(p: &ProjetoEdicao) -> Vec<&ClipePrincipal> {
        p.principal.iter().map(|c| match c {
            Tolerante::Conhecido(c) => c,
            Tolerante::Desconhecido(v) => panic!("desconhecido: {v}"),
        }).collect()
    }

    fn efeitos(c: &ClipePrincipal) -> Vec<&Efeito> {
        let base = match c {
            ClipePrincipal::Trecho(x) => &x.base,
            ClipePrincipal::Congelar(x) => &x.base,
            ClipePrincipal::Titulo(x) => &x.base,
            _ => unreachable!(),
        };
        base.efeitos.iter().map(|e| match e {
            Tolerante::Conhecido(e) => e,
            Tolerante::Desconhecido(v) => panic!("desconhecido: {v}"),
        }).collect()
    }

    fn rastreio<'a>(p: &'a ProjetoEdicao, id: &str) -> &'a Rastreio {
        p.rastreios.iter().find(|r| r.id == id).unwrap()
    }

    #[test]
    fn velocidade_cartela_e_congelado() {
        let p = converte("v1-velocidade.json");
        assert_eq!((p.esquema, p.esquema_menor, p.id.as_str(), p.titulo.as_str()), (2, ESQUEMA_MENOR, "a0000000-0000-4000-8000-000000000001", "Velocidade"));
        assert_eq!((p.created_at.as_str(), p.updated_at.as_str(), p.gravado_por.as_str()), ("2026-11-01T10:00:00.000Z", "2026-11-01T10:05:00.000Z", "6.0.0"));
        assert_eq!(p.saida, Saida {
            largura: 1920, altura: 1080, fps: Racional { num: 30, den: 1 }, auto: false, cor_fundo: "#000000".into(), audio: true,
            selos: Selos { velocidade: true, ampliacao: true, realce: true },
            exportacao: Exportacao { predefinicao: Predefinicao::Juri, codificador: EscolhaCodificador::Auto, legendas: ModoLegendas::Nenhuma,
                                     pacote: None, pdf: ModoPdf::A3, revisado_por: String::new(), personalizada: None },
        });
        assert_eq!((p.fecho.clone(), p.sobreposicoes.len(), p.audios.len(), p.marcadores.len(), p.sincronias.len(), p.rastreios.len()), (None, 0, 0, 0, 0, 0));
        assert_eq!(p.legendas, TrilhaLegendas::default());
        assert_eq!(p.mixagem, Mixagem { volume_original: 1.0, abaixar: Abaixar { db: -12.0, ataque_s: 0.3, soltura_s: 0.5 }, normalizar: None, limitador: false });
        // uma mídia para os dois usos de M1
        assert_eq!(p.midias.len(), 1);
        let m = &p.midias[0];
        assert_eq!((m.media_id.as_deref(), m.sha256.as_str(), m.tipo, m.origem), (Some(M1), "1".repeat(64).as_str(), TipoMidia::Video, OrigemMidia::Caso));
        assert_ne!(m.id, "qualquer");
        let c = clipes(&p);
        let ClipePrincipal::Trecho(t) = c[0] else { panic!() };
        assert_eq!((t.base.id.as_str(), t.base.rotulo.as_str(), t.base.desligado, t.base.transicao_entrada.clone()), ("a1", "", false, None));
        assert_eq!((t.midia.as_str(), t.inicio_s, t.fim_s, t.inicio_pts, t.fim_pts), (m.id.as_str(), 2.0, 4.0, Some(2000), Some(4000)));
        assert_eq!(t.velocidade, Velocidade::Constante { fator: 0.5 });
        assert_eq!(t.audio, AudioClipe { volume: Animavel::Fixo(1.0), mudo: false, manter_tom: true, silencios: vec![] });
        assert!(t.base.efeitos.is_empty());
        let ClipePrincipal::Titulo(k) = c[1] else { panic!() };
        assert_eq!((k.base.id.as_str(), k.duracao_s, k.base.transicao_entrada.clone()), ("a2", 2.0, None));
        assert_eq!((k.titulo.modelo, k.titulo.fundo.clone()), (ModeloTitulo::Cartela, Fundo::Cor { cor: "#000000".into() }));
        let b = &k.titulo.blocos[0];
        assert_eq!((b.papel, b.conteudo.as_str(), b.largura_max), (PapelTexto::Corpo, "x", 0.9));
        assert_eq!(b.estilo, EstiloTexto { fonte: Fonte::Sans, negrito: false, italico: false, tamanho: 0.052, cor: "#ffffff".into(),
                                           alinhamento: Alinhamento::Centro, caixa: None, contorno: None });
        assert_eq!(b.lugar, Lugar { x: 0.5, y: 0.5, h: Alinhamento::Centro, v: AlinhamentoV::Meio });
        let ClipePrincipal::Congelar(g) = c[2] else { panic!() };
        assert_eq!((g.base.id.as_str(), g.midia.as_str(), g.instante_s, g.instante_pts, g.duracao_s), ("a3", m.id.as_str(), 1.0, Some(1000), 1.5));
        assert!(g.base.efeitos.is_empty());
    }

    #[test]
    fn zoom_destaques_e_desfoques() {
        let p = converte("v1-efeitos.json");
        let c = clipes(&p);
        let ClipePrincipal::Trecho(t) = c[0] else { panic!() };
        let e = efeitos(c[0]);
        assert_eq!(e.len(), 3, "enquadramento + 2 destaques (o desfoque vai para a mídia)");
        // fonte 4:3 numa saída 16:9: h = w × (4/3) ÷ (16/9) = 0,75 w
        let Efeito::Enquadramento(z) = e[0] else { panic!() };
        assert_eq!((z.ajuste, z.interpolacao, z.base.ligado, z.base.ativo), (Ajuste::Cobrir, Interpolacao::Bicubica, true, None));
        assert_eq!(z.regiao, Animavel::Chaves(vec![
            Chave { t: 0.0, pts: None, v: Caixa { x: 0.0, y: 0.0, w: 1.0, h: 0.75 }, interp: Interp::Linear },
            Chave { t: 4.0, pts: None, v: Caixa { x: 0.25, y: 0.25, w: 0.5, h: 0.375 }, interp: Interp::Linear },
        ]));
        let Efeito::Destaque(d1) = e[1] else { panic!() };
        assert_eq!((d1.base.id.as_str(), d1.forma, d1.tamanho, d1.angulo_graus, d1.margem, d1.rotulo.clone()), ("d1", FormaDestaque::Circulo, 0.12, 45.0, 0.0, None));
        assert_eq!(d1.traco, Traco { cor: "#ff0000".into(), espessura: 0.006, contorno: true });
        assert_eq!(d1.base.ativo, Some(Intervalo { de: Some(1.0), ate: None }));
        let r1 = rastreio(&p, &d1.rastreio);
        assert_eq!((r1.tipo, r1.base, r1.espaco.as_str(), r1.midia.as_deref(), r1.origem.clone()), (TipoRastreio::Ponto, BaseRastreio::Fonte, "original", Some(t.midia.as_str()), OrigemRastreio::Manual));
        assert_eq!(r1.ancoras, vec![Chave { t: 1.0, pts: None, v: Caixa { x: 0.5, y: 0.5, w: 0.0, h: 0.0 }, interp: Interp::Linear }]);
        assert!(r1.denso.is_none() && r1.silhuetas.is_none() && r1.confirmacao.is_none() && r1.ausencias.is_empty());
        let Efeito::Destaque(d2) = e[2] else { panic!() };
        assert_eq!((d2.forma, d2.traco.contorno), (FormaDestaque::Retangulo, false));
        let r2 = rastreio(&p, &d2.rastreio);
        assert_eq!(r2.tipo, TipoRastreio::Caixa);
        assert_eq!(r2.ancoras[0].v, Caixa { x: 0.5, y: 0.5, w: 0.1, h: 0.1 });
        // desfoque = ocultação da mídia, de [primeiro quadro, fim do trecho]
        let oc = &p.midias[0].ocultacoes;
        assert_eq!(oc.len(), 1);
        let o = &oc[0];
        assert_eq!((o.base.id.as_str(), o.base.ativo), ("f1", Some(Intervalo { de: Some(0.0), ate: Some(4.0) })));
        assert_eq!((o.modo, o.intensidade, o.cor.as_str(), o.conteudo, o.motivo.as_str()), (ModoOcultar::Desfoque, 0.6, "#000000ff", ConteudoOculto::Outro, ""));
        assert_eq!((o.alvo.forma, o.alvo.vertices.clone(), o.alvo.margem, o.alvo.borda_suave, o.alvo.inverter), (FormaRegiao::Retangulo, None, 0.0, 0.0, false));
        let r3 = rastreio(&p, &o.alvo.rastreio);
        assert_eq!((r3.tipo, r3.ancoras[0].t, r3.ancoras[0].v), (TipoRastreio::Caixa, 0.0, Caixa { x: 0.1, y: 0.1, w: 0.2, h: 0.2 }));
        assert_eq!(p.rastreios.len(), 3);
    }

    #[test]
    fn json_antigo_sem_campos_novos() {
        let p = converte("v1-antigo.json");
        assert_eq!(p.mixagem.volume_original, 1.0);
        assert!(p.audios.is_empty());
        let c = clipes(&p);
        assert_eq!(c.len(), 1);
        let ClipePrincipal::Trecho(t) = c[0] else { panic!() };
        assert!(t.base.efeitos.is_empty() && t.base.transicao_entrada.is_none());
        assert_eq!(t.velocidade, Velocidade::Constante { fator: 1.0 });
        // sem created_at/updated_at a importação carimba a hora
        let mut v: Value = serde_json::from_str(&std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edicao/v1-antigo.json")).unwrap()).unwrap();
        v.as_object_mut().unwrap().remove("created_at");
        v.as_object_mut().unwrap().remove("updated_at");
        let p = converter(&v, &Falsas, AGORA, "6.0.0").unwrap();
        assert_eq!((p.created_at.as_str(), p.updated_at.as_str()), (AGORA, AGORA));
        // item de tipo que a v1 não tinha: recusado (a v1 também não abria)
        v["itens"][0]["tipo"] = json!("outro");
        assert!(matches!(converter(&v, &Falsas, AGORA, "x"), Err(SicroError::Validation(c)) if c.starts_with("esquema")));
    }

    #[test]
    fn narracao_e_volume() {
        let p = converte("v1-narracao.json");
        assert!(p.principal.is_empty());
        assert_eq!(p.midias.len(), 1);
        let m = &p.midias[0];
        assert_eq!((m.tipo, m.origem, m.relativo.as_str(), m.media_id.clone(), m.sha256.len()), (TipoMidia::Audio, OrigemMidia::Projeto, "a/b.wav", None, 64));
        assert_eq!(p.audios.len(), 1);
        let t = &p.audios[0];
        assert_eq!((t.nome.as_str(), t.papel, t.volume, t.mudo, t.abaixa_original), ("Narração", PapelAudio::Narracao, 1.0, false, false));
        assert_eq!(t.clipes, vec![ClipeAudio {
            id: "n1".into(), nome: "n".into(), midia: m.id.clone(), pos: Posicao::Programa { t: 1.0 }, de_s: 0.0, ate_s: None,
            volume: Animavel::Fixo(1.0), entrada_s: 0.0, saida_s: 0.0,
        }]);
    }

    #[test]
    fn quadros_fora_de_ordem_viram_ancoras_em_ordem() {
        let p = converte("v1-quadros.json");
        let e = efeitos(clipes(&p)[0]);
        let Efeito::Destaque(d) = e[0] else { panic!() };
        assert_eq!((d.forma, d.tamanho, d.traco.contorno, d.traco.cor.as_str()), (FormaDestaque::Seta, 0.14, true, "#ff2d2d"));
        assert_eq!(d.base.ativo, Some(Intervalo { de: Some(10.0), ate: None }));
        let r = rastreio(&p, &d.rastreio);
        let ts: Vec<(f64, f64)> = r.ancoras.iter().map(|k| (k.t, k.v.x)).collect();
        assert_eq!(ts, [(10.0, 0.2), (12.0, 0.6)]);
        assert_eq!(r.tipo, TipoRastreio::Ponto);
    }

    #[test]
    fn todos_os_efeitos_da_v1() {
        let p = converte("v1-completo.json");
        let c = clipes(&p);
        assert_eq!(c.len(), 6);
        let trans = |c: &ClipePrincipal| match c {
            ClipePrincipal::Trecho(x) => x.base.transicao_entrada.clone(),
            ClipePrincipal::Titulo(x) => x.base.transicao_entrada.clone(),
            ClipePrincipal::Congelar(x) => x.base.transicao_entrada.clone(),
            _ => unreachable!(),
        };
        let esc = |d: f64| Some(Transicao { tipo: "escurecer".into(), duracao_s: d, audio: AudioTransicao::Acompanhar });
        // f1 primeiro (10 s): 0,5 · f2 cartela de 0,6 s depois de 10 s: 0,3 · f4 (0,5 s) depois de congelar 2 s: 0,25
        assert_eq!(c.iter().map(|c| trans(c)).collect::<Vec<_>>(), [esc(0.5), esc(0.3), None, esc(0.25), None, None]);

        let e = efeitos(c[0]);
        let tipos: Vec<String> = e.iter().map(|e| serde_json::to_value(e).unwrap()["tipo"].as_str().unwrap().to_string()).collect();
        assert_eq!(tipos, ["enquadramento", "destaque", "destaque", "destaque", "carimbo", "texto"]);
        let Efeito::Enquadramento(z) = e[0] else { panic!() };
        let Animavel::Chaves(k) = &z.regiao else { panic!() };
        assert_eq!(k[1].v, Caixa { x: 0.2, y: 0.3, w: 0.1, h: 0.075 }, "w preso em 0,1");
        let Efeito::Carimbo(cb) = e[4] else { panic!() };
        assert_eq!((cb.mostra, cb.formato, cb.relogio.clone()), (MostraCarimbo::TempoFonte, FormatoCarimbo::HmsMs, None));
        assert_eq!(cb.estilo, EstiloTexto { fonte: Fonte::Mono, negrito: false, italico: false, tamanho: 0.033, cor: "#ffffff".into(),
            alinhamento: Alinhamento::Dir, caixa: Some(CaixaTexto { cor: "#0000008c".into(), margem: 0.0074, raio: 0.0 }), contorno: None });
        assert_eq!(cb.lugar, Lugar { x: 0.984, y: 0.028, h: Alinhamento::Dir, v: AlinhamentoV::Topo });
        let Efeito::Texto(tx) = e[5] else { panic!() };
        assert_eq!((tx.conteudo.as_str(), tx.largura_max), ("Câmera 1 — entrada", 0.92));
        assert_eq!(tx.estilo, EstiloTexto { fonte: Fonte::Sans, negrito: false, italico: false, tamanho: 0.078, cor: "#ffffff".into(),
            alinhamento: Alinhamento::Centro, caixa: Some(CaixaTexto { cor: "#0000008c".into(), margem: 0.013, raio: 0.0 }), contorno: None });
        assert_eq!(tx.lugar, Lugar { x: 0.5, y: 0.06, h: Alinhamento::Centro, v: AlinhamentoV::Topo });
        // cartela: tamanho p, cores da v1
        let ClipePrincipal::Titulo(k) = c[1] else { panic!() };
        assert_eq!(k.titulo.fundo, Fundo::Cor { cor: "#101010".into() });
        assert_eq!((k.titulo.blocos[0].estilo.tamanho, k.titulo.blocos[0].estilo.cor.as_str()), (0.033, "#fafafa"));
        // congelado com carimbo e texto no centro
        let ClipePrincipal::Congelar(g) = c[2] else { panic!() };
        assert_eq!((g.instante_pts, g.midia.as_str()), (Some(3500), p.midias[1].id.as_str()));
        let eg = efeitos(c[2]);
        let Efeito::Texto(tg) = eg[1] else { panic!() };
        assert_eq!((eg.len(), tg.lugar, tg.estilo.tamanho), (2, Lugar { x: 0.5, y: 0.5, h: Alinhamento::Centro, v: AlinhamentoV::Meio }, 0.033));
        // texto em branco não vira efeito; velocidade 2
        let ClipePrincipal::Trecho(t4) = c[3] else { panic!() };
        assert!(t4.base.efeitos.is_empty());
        assert_eq!((t4.velocidade.clone(), t4.midia.as_str()), (Velocidade::Constante { fator: 2.0 }, p.midias[0].id.as_str()));
        // mídia sumida: só o media_id, sem pts
        let ClipePrincipal::Trecho(t5) = c[4] else { panic!() };
        let m3 = p.midias.iter().find(|m| m.id == t5.midia).unwrap();
        assert_eq!((m3.media_id.as_deref(), m3.sha256.as_str(), m3.nome.as_str(), m3.relativo.as_str()), (Some(M3), "", "", ""));
        assert_eq!((t5.inicio_pts, t5.fim_pts), (None, None));
        // cartela em branco: sem bloco
        let ClipePrincipal::Titulo(k6) = c[5] else { panic!() };
        assert!(k6.titulo.blocos.is_empty());

        // mídias: M1, M2, M3 e as três narrações distintas (a repetida reaproveita)
        let rels: Vec<&str> = p.midias.iter().map(|m| m.relativo.as_str()).collect();
        assert_eq!(&rels[3..], ["Sistema/videos/montagens/audios/a1.webm", "Sistema/videos/montagens/audios/a2.wav", "Sistema/videos/montagens/audios/sumiu.ogg"]);
        assert_eq!(p.midias[0].media_id.as_deref(), Some(M1));
        assert_eq!(p.midias[1].media_id.as_deref(), Some(M2));
        let sumiu = &p.midias[5];
        assert_eq!((sumiu.sha256.as_str(), sumiu.nome.as_str(), sumiu.tipo, sumiu.origem), ("", "sumiu.ogg", TipoMidia::Audio, OrigemMidia::Projeto));
        let t = &p.audios[0];
        let vols: Vec<(&str, f64)> = t.clipes.iter().map(|c| (c.midia.as_str(), match c.volume { Animavel::Fixo(v) => v, _ => panic!() })).collect();
        assert_eq!(vols, [(p.midias[3].id.as_str(), 0.8), (p.midias[4].id.as_str(), 1.5), (p.midias[3].id.as_str(), 1.0), (p.midias[5].id.as_str(), 1.0)]);
        assert_eq!(p.mixagem.volume_original, 0.8);

        // desfoques na mídia M1, âncoras em ordem, `ate` = fim do trecho
        let oc = &p.midias[0].ocultacoes;
        assert_eq!(oc.iter().map(|o| (o.base.id.as_str(), o.base.ativo)).collect::<Vec<_>>(),
                   [("g1", Some(Intervalo { de: Some(12.0), ate: Some(20.0) })), ("g2", Some(Intervalo { de: Some(10.0), ate: Some(20.0) }))]);
        let rg1 = rastreio(&p, &oc[0].alvo.rastreio);
        assert_eq!(rg1.ancoras.iter().map(|k| k.t).collect::<Vec<_>>(), [12.0, 14.0]);
        assert_eq!(p.rastreios.len(), 5);
        assert!(p.rastreios.iter().all(|r| r.midia.as_deref() == Some(p.midias[0].id.as_str())));

        // ids: nenhum repetido; tudo que é gerado tem formato de UUID; duas importações = mesmo documento
        let v = serde_json::to_value(&p).unwrap();
        let mut ids = Vec::new();
        for caminho in [&["midias", "*", "id"][..], &["midias", "*", "ocultacoes", "*", "id"], &["principal", "*", "id"],
                        &["principal", "*", "efeitos", "*", "id"], &["principal", "*", "titulo", "blocos", "*", "id"],
                        &["audios", "*", "id"], &["audios", "*", "clipes", "*", "id"], &["rastreios", "*", "id"]] {
            fn em(v: &Value, c: &[&str], s: &mut Vec<String>) {
                match c.split_first() {
                    None => s.extend(v.as_str().map(str::to_string)),
                    Some((&"*", r)) => v.as_array().into_iter().flatten().for_each(|x| em(x, r, s)),
                    Some((k, r)) => { if let Some(x) = v.get(*k) { em(x, r, s) } }
                }
            }
            em(&v, caminho, &mut ids);
        }
        let unicos: BTreeSet<&String> = ids.iter().collect();
        assert_eq!(unicos.len(), ids.len(), "id repetido");
        for r in &p.rastreios {
            assert_eq!(Uuid::parse_str(&r.id).unwrap().get_version_num(), 4);
        }
        assert_eq!(converte("v1-completo.json"), p);
    }

    /// Montagem gravada pela própria v1 antes de ela sair (modelo do front da v1 e o `save_montagem` de então):
    /// `null` nos opcionais, `"corte"`, `updated_at` do Rust, narrações em `montagens/audios`.
    #[test]
    fn montagem_gravada_pela_v1() {
        let p = converte("v1-salvo-pela-v1.json");
        assert_eq!(
            (p.id.as_str(), p.titulo.as_str(), p.created_at.as_str(), p.updated_at.as_str()),
            ("361ddd9a-fffb-4720-9d22-93de2c2d967e", "Furto na farmácia", "2026-11-15T07:46:46.390Z", "2026-11-15T07:47:51.124745224+00:00")
        );
        assert_eq!((p.saida.largura, p.saida.altura, p.saida.fps, p.saida.audio, p.saida.auto), (1920, 1080, Racional { num: 25, den: 1 }, true, false));
        assert_eq!(p.mixagem.volume_original, 0.6);
        let c = clipes(&p);
        let id_de = |c: &ClipePrincipal| match c {
            ClipePrincipal::Trecho(x) => x.base.id.clone(),
            ClipePrincipal::Titulo(x) => x.base.id.clone(),
            ClipePrincipal::Congelar(x) => x.base.id.clone(),
            _ => unreachable!(),
        };
        assert_eq!(
            c.iter().map(|c| id_de(c)).collect::<Vec<_>>(),
            ["e06b1931-ee90-4107-831b-bf510f232cb7", "9285a726-11f8-49fd-b3c6-1817ceca39ed", "d11b8794-cc50-4a20-8529-ac1f7ffc5401",
             "157db123-1f1c-4bc9-99ea-3e47b66cf9fa", "3947b750-2e30-43d2-8a2a-2cff0005c4e5"]
        );
        let (m1, m2, m3) = (&p.midias[0], &p.midias[1], &p.midias[2]);
        assert_eq!((m1.media_id.as_deref(), m2.media_id.as_deref(), m3.media_id.as_deref()), (Some(M1), Some(M2), Some(M3)));
        assert_eq!((m3.sha256.as_str(), m3.relativo.as_str()), ("", ""), "M3 não está no caso");
        let esc = |d: f64| Some(Transicao { tipo: "escurecer".into(), duracao_s: d, audio: AudioTransicao::Acompanhar });

        // 1: 0,5×, "fade" no primeiro (13 s na saída) = 0,5; zoom, círculo, retângulo, desfoque, carimbo e texto no topo
        let ClipePrincipal::Trecho(t1) = c[0] else { panic!() };
        assert_eq!((t1.midia.as_str(), t1.inicio_s, t1.fim_s, t1.inicio_pts, t1.fim_pts), (m1.id.as_str(), 12.4, 18.9, Some(12400), Some(18900)));
        assert_eq!((t1.velocidade.clone(), t1.base.transicao_entrada.clone()), (Velocidade::Constante { fator: 0.5 }, esc(0.5)));
        let e = efeitos(c[0]);
        let tipos: Vec<String> = e.iter().map(|e| serde_json::to_value(e).unwrap()["tipo"].as_str().unwrap().to_string()).collect();
        assert_eq!(tipos, ["enquadramento", "destaque", "destaque", "carimbo", "texto"]);
        let Efeito::Enquadramento(z) = e[0] else { panic!() };
        let Animavel::Chaves(k) = &z.regiao else { panic!() };
        // fonte 4:3 numa saída 16:9: h = 0,75 w
        assert_eq!((k.len(), k[0].t, k[0].v, k[1].t), (2, 12.4, Caixa { x: 0.0, y: 0.0, w: 1.0, h: 0.75 }, 18.9));
        assert!((k[1].v.x, k[1].v.y, k[1].v.w) == (0.3, 0.2, 0.4) && (k[1].v.h - 0.3).abs() < 1e-12);
        let Efeito::Destaque(d1) = e[1] else { panic!() };
        assert_eq!((d1.base.id.as_str(), d1.forma, d1.traco.cor.as_str(), d1.traco.contorno), ("ae01d5ac-9c44-414f-8b4e-a1263df026a4", FormaDestaque::Circulo, "#ff2d2d", true));
        assert_eq!(d1.base.ativo, Some(Intervalo { de: Some(12.4 + 0.5), ate: None }));
        let r1 = rastreio(&p, &d1.rastreio);
        assert_eq!(r1.ancoras.iter().map(|a| (a.t, a.v)).collect::<Vec<_>>(),
                   [(12.4 + 0.5, Caixa { x: 0.42, y: 0.55, w: 0.0, h: 0.0 }), (12.4 + 2.0, Caixa { x: 0.47, y: 0.5, w: 0.0, h: 0.0 })]);
        let Efeito::Destaque(d2) = e[2] else { panic!() };
        assert_eq!((d2.forma, d2.traco.cor.as_str(), d2.traco.contorno, d2.tamanho), (FormaDestaque::Retangulo, "#ffd400", false, 0.12));
        assert_eq!(rastreio(&p, &d2.rastreio).ancoras[0].v, Caixa { x: 0.6, y: 0.3, w: 0.1, h: 0.2 });
        let Efeito::Carimbo(cb) = e[3] else { panic!() };
        assert_eq!((cb.mostra, cb.formato), (MostraCarimbo::TempoFonte, FormatoCarimbo::HmsMs));
        let Efeito::Texto(tx) = e[4] else { panic!() };
        assert_eq!((tx.conteudo.as_str(), tx.estilo.tamanho, tx.lugar), ("Entrada do suspeito", 0.078, Lugar { x: 0.5, y: 0.06, h: Alinhamento::Centro, v: AlinhamentoV::Topo }));
        let oc = &m1.ocultacoes;
        assert_eq!((oc.len(), oc[0].base.id.as_str(), oc[0].base.ativo), (1, "be9476e7-6f24-4060-b64e-f64ba7d202e0", Some(Intervalo { de: Some(12.4), ate: Some(18.9) })));
        assert_eq!((oc[0].modo, oc[0].alvo.forma), (ModoOcultar::Desfoque, FormaRegiao::Retangulo));
        assert_eq!(rastreio(&p, &oc[0].alvo.rastreio).ancoras.iter().map(|a| a.t).collect::<Vec<_>>(), [12.4, 12.4 + 3.0]);

        // 2: cartela de 3 s em "p", fade = min(0,5; 13 ÷ 2; 3 ÷ 2)
        let ClipePrincipal::Titulo(k2) = c[1] else { panic!() };
        assert_eq!((k2.duracao_s, k2.base.transicao_entrada.clone(), k2.titulo.fundo.clone()), (3.0, esc(0.5), Fundo::Cor { cor: "#101820".into() }));
        let b = &k2.titulo.blocos[0];
        assert_eq!((k2.titulo.blocos.len(), b.conteudo.as_str(), b.estilo.tamanho, b.estilo.cor.as_str()), (1, "Câmera 2 — corredor", 0.033, "#ffffff"));
        // 3: quadro congelado de M2 com texto no centro, sem carimbo
        let ClipePrincipal::Congelar(g3) = c[2] else { panic!() };
        assert_eq!((g3.midia.as_str(), g3.instante_s, g3.instante_pts, g3.duracao_s, g3.base.transicao_entrada.clone()), (m2.id.as_str(), 7.25, Some(7250), 2.0, None));
        let eg = efeitos(c[2]);
        let Efeito::Texto(tg) = eg[0] else { panic!() };
        assert_eq!((eg.len(), tg.conteudo.as_str(), tg.lugar.v), (1, "Momento do contato", AlinhamentoV::Meio));
        // 4: M3 a 2×, sem pts; "corte" = sem transição
        let ClipePrincipal::Trecho(t4) = c[3] else { panic!() };
        assert_eq!((t4.midia.as_str(), t4.inicio_pts, t4.velocidade.clone(), t4.base.transicao_entrada.clone()), (m3.id.as_str(), None, Velocidade::Constante { fator: 2.0 }, None));
        // 5: seta em M2
        let ClipePrincipal::Trecho(t5) = c[4] else { panic!() };
        let Efeito::Destaque(s5) = efeitos(c[4])[0] else { panic!() };
        assert_eq!((t5.midia.as_str(), s5.forma, s5.tamanho, s5.base.ativo), (m2.id.as_str(), FormaDestaque::Seta, 0.14, Some(Intervalo { de: Some(5.0 + 0.4), ate: None })));

        // narrações gravadas pela v1: mídia do projeto, uma trilha "Narração"
        let narr: Vec<&str> = p.midias[3..].iter().map(|m| m.relativo.as_str()).collect();
        assert_eq!(narr, ["Sistema/videos/montagens/audios/d98986a5-ed7f-47d9-bc0c-5e6ff7c8393a.webm",
                          "Sistema/videos/montagens/audios/2ec742a7-0858-4e4e-b14a-4cb4d022ee35.wav"]);
        assert!(p.midias[3..].iter().all(|m| m.origem == OrigemMidia::Projeto && m.tipo == TipoMidia::Audio && m.sha256.len() == 64));
        let t = &p.audios[0];
        let cl: Vec<(&str, &str, f64, f64)> = t.clipes.iter().map(|c| (c.id.as_str(), c.nome.as_str(), match c.pos { Posicao::Programa { t } => t, _ => panic!() }, match c.volume { Animavel::Fixo(v) => v, _ => panic!() })).collect();
        assert_eq!((p.audios.len(), t.papel), (1, PapelAudio::Narracao));
        assert_eq!(cl, [("eed7ee58-c644-491f-9c92-cc8327232f92", "Narração 1", 1.5, 0.8), ("7d9ff0a1-12d5-43c0-bd1d-8c732820d9fb", "Narração 2", 20.0, 1.0)]);
        assert_eq!(p.rastreios.len(), 4);
        assert_eq!(converte("v1-salvo-pela-v1.json"), p, "duas importações, o mesmo documento");
    }

    fn caso_temporario(nome: &str) -> (tempfile::TempDir, std::path::PathBuf, Connection) {
        let tmp = tempfile::Builder::new().prefix("sicro-p111-").tempdir().unwrap();
        let entrada = crate::models::NewOccurrenceInput { titulo: Some(nome.into()), ..Default::default() };
        let ws = crate::workspace::create::create_workspace(entrada, tmp.path()).unwrap().path;
        let conn = crate::database::connection::open_connection(&paths::sqlite_path(&ws)).unwrap();
        (tmp, ws, conn)
    }

    /// Importação que não consegue ler (proveniência estragada, derivação ilegível, narração sem leitura, v1 que a
    /// v1 não abriria) não grava nada e deixa o v1 com o mesmo nome e os mesmos bytes; corrigido, importa.
    #[test]
    fn importacao_que_falha_deixa_o_v1_como_esta() {
        let (_tmp, ws, conn) = caso_temporario("Roubo Jardim Felicidade");
        let occ = crate::workspace::manifest::Manifest::read(&ws).unwrap().occurrence_id;
        let agora = Utc::now();
        let (raiz, uso) = ("d".repeat(64), "e".repeat(64));
        let id_uso = Uuid::new_v4();
        let midia = |id: Uuid, sha: &str, derivado: Option<&str>| VideoMedia {
            id, occurrence_id: occ, original_path: None, relative_path: format!("Mídias/Vídeos/{sha}.mp4"), filename: "cam.mp4".into(),
            sha256: sha.into(), size_bytes: 10, duration_s: Some(30.0), codec: Some("h264".into()), width: Some(1440), height: Some(1080),
            pixel_format: Some("yuv420p".into()), fps_declared: Some(30.0), avg_frame_rate: Some("30/1".into()), r_frame_rate: Some("30/1".into()),
            time_base: Some("1/15360".into()), frame_count: Some(900), bitrate: None, raw_probe_json: "{}".into(),
            warnings_json: "[]".into(), created_at: agora, updated_at: agora,
            derived_from_hash: derivado.map(str::to_string), derivation_json: derivado.map(|_| r#"{"kind":"clip","mode":"copy"}"#.to_string()),
        };
        video_repo::insert_media(&conn, &midia(Uuid::new_v4(), &raiz, None)).unwrap();
        video_repo::insert_media(&conn, &midia(id_uso, &uso, Some(&raiz))).unwrap();
        // a montagem gravada pela v1, com o primeiro vídeo trocado pelo do caso
        let texto = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edicao/v1-salvo-pela-v1.json")).unwrap();
        let bytes = texto.replace(M1, &id_uso.to_string()).into_bytes();
        let id = "361ddd9a-fffb-4720-9d22-93de2c2d967e";
        let montagens = paths::dir_montagens_json(&ws);
        std::fs::create_dir_all(montagens.join("audios")).unwrap();
        let v1_arq = montagens.join(format!("{id}.json"));
        std::fs::write(&v1_arq, &bytes).unwrap();
        let narracao = montagens.join("audios/d98986a5-ed7f-47d9-bc0c-5e6ff7c8393a.webm");
        std::fs::write(&narracao, b"narracao").unwrap();
        let projeto = armazem::pasta_projeto(&ws, id).join("projeto.json");
        let intacto = |caso: &str| {
            assert_eq!(std::fs::read(&v1_arq).unwrap(), bytes, "{caso}: v1 alterado");
            assert!(!v1_arq.with_extension("json.v1").exists(), "{caso}: v1 renomeado");
            assert!(!projeto.exists(), "{caso}: v2 gravado");
            let l = armazem::listar(&ws).unwrap();
            assert_eq!((l.len(), l[0].esquema), (1, 1), "{caso}");
        };
        let sql = |q: &str| conn.execute(q, [id_uso.to_string()]).unwrap();

        sql("UPDATE video_media SET recebido_nativo = 2 WHERE id = ?1");
        assert!(matches!(armazem::abrir(&ws, id), Err(SicroError::Database(m)) if m.contains("recebido_nativo")));
        intacto("recebido_nativo = 2");
        sql("UPDATE video_media SET recebido_nativo = 1, relogio_desvio_s = 'dois' WHERE id = ?1");
        assert!(matches!(armazem::abrir(&ws, id), Err(SicroError::Sqlite(_))));
        intacto("relogio_desvio_s em texto");
        // 9e999 vira infinito no SQLite: a tela não grava, a importação não aceita
        sql("UPDATE video_media SET relogio_desvio_s = 9e999 WHERE id = ?1");
        assert!(matches!(armazem::abrir(&ws, id), Err(SicroError::Database(m)) if m.contains("relogio_desvio_s")));
        intacto("relogio_desvio_s infinito");
        sql("UPDATE video_media SET relogio_desvio_s = -1.5, derivation_json = '{' WHERE id = ?1");
        assert!(matches!(armazem::abrir(&ws, id), Err(SicroError::Database(m)) if m.contains("derivation_json")));
        intacto("derivation_json ilegível");
        // gravador em branco = não informado, como na análise
        sql("UPDATE video_media SET derivation_json = '{\"kind\":\"clip\",\"mode\":\"copy\"}', gravador = '' WHERE id = ?1");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&narracao, std::fs::Permissions::from_mode(0o000)).unwrap();
            // como root a leitura passa: o caso só vale para usuário comum
            if std::fs::File::open(&narracao).is_err() {
                assert!(matches!(armazem::abrir(&ws, id), Err(SicroError::Filesystem(m)) if m.contains("d98986a5")));
                intacto("narração sem leitura");
            }
            std::fs::set_permissions(&narracao, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        // item que a v1 também não abria: recusado, nada gravado
        let mut ruim: Value = serde_json::from_slice(&bytes).unwrap();
        ruim["itens"][0]["tipo"] = json!("outro");
        let bytes_ruim = serde_json::to_vec(&ruim).unwrap();
        std::fs::write(&v1_arq, &bytes_ruim).unwrap();
        assert!(matches!(armazem::abrir(&ws, id), Err(SicroError::Validation(c)) if c.starts_with("esquema")));
        assert_eq!(std::fs::read(&v1_arq).unwrap(), bytes_ruim);
        assert!(!projeto.exists() && !v1_arq.with_extension("json.v1").exists());

        // com o banco certo, importa; o v1 só muda de nome
        std::fs::write(&v1_arq, &bytes).unwrap();
        let a = armazem::abrir(&ws, id).unwrap();
        assert_eq!(a.importado_de, Some(1));
        assert_eq!(std::fs::read(v1_arq.with_extension("json.v1")).unwrap(), bytes);
        let p = armazem::tipado(&a.projeto).unwrap();
        let m = &p.midias[0];
        assert_eq!((m.media_id.clone(), m.recebido.nativo, m.relogio_desvio_s, m.gravador.clone()), (Some(id_uso.to_string()), Some(true), Some(-1.5), None));
        assert_eq!(m.derivacao.as_ref().map(|d| (d.tipo, d.raiz_sha256.as_str(), d.com_perdas)), Some((TipoDerivacao::Trecho, raiz.as_str(), false)));
        assert_eq!(p.midias[3].sha256, crate::hashing::sha256::sha256_bytes(b"narracao"));
    }

    /// Ponta a ponta pelo `abrir`: caso de verdade (banco e pastas), vídeo derivado, narração no disco.
    #[test]
    fn importar_pelo_abrir() {
        let tmp = tempfile::Builder::new().prefix("sicro-p02-").tempdir().unwrap();
        let entrada = crate::models::NewOccurrenceInput { titulo: Some("Furto São Benedito".into()), ..Default::default() };
        let ws = crate::workspace::create::create_workspace(entrada, tmp.path()).unwrap().path;
        let occ = crate::workspace::manifest::Manifest::read(&ws).unwrap().occurrence_id;
        let conn = crate::database::connection::open_connection(&paths::sqlite_path(&ws)).unwrap();
        let agora = Utc::now();
        let midia = |id: Uuid, sha: &str, nome: &str, derivado: Option<(&str, Value)>| VideoMedia {
            id, occurrence_id: occ, original_path: None, relative_path: format!("Mídias/Vídeos/{nome}"), filename: nome.into(),
            sha256: sha.into(), size_bytes: 10, duration_s: Some(12.5), codec: Some("h264".into()), width: Some(1920), height: Some(1080),
            pixel_format: Some("yuv420p".into()), fps_declared: Some(30.0), avg_frame_rate: Some("30/1".into()), r_frame_rate: Some("30/1".into()),
            time_base: Some("1/15360".into()), frame_count: Some(375), bitrate: None,
            raw_probe_json: json!({ "streams": [
                { "codec_type": "video", "width": 1920, "height": 1080, "sample_aspect_ratio": "1:1", "side_data_list": [{ "rotation": -90 }] },
                { "codec_type": "audio" } ], "format": { "start_time": "1.500000" } }).to_string(),
            warnings_json: "[]".into(), created_at: agora, updated_at: agora,
            derived_from_hash: derivado.as_ref().map(|d| d.0.to_string()), derivation_json: derivado.map(|d| d.1.to_string()),
        };
        let (raiz, meio, uso) = ("a".repeat(64), "b".repeat(64), "c".repeat(64));
        let id_uso = Uuid::new_v4();
        video_repo::insert_media(&conn, &midia(Uuid::new_v4(), &raiz, "original.dav", None)).unwrap();
        video_repo::insert_media(&conn, &midia(Uuid::new_v4(), &meio, "estab.mp4", Some((&raiz, json!({ "kind": "estabilizado", "comandos": ["a", "b"] }))))).unwrap();
        video_repo::insert_media(&conn, &midia(id_uso, &uso, "trecho 1.mp4",
            Some((&meio, json!({ "kind": "clip", "mode": "copy", "ffmpeg_command": ["ffmpeg", "-ss", "1", "-c", "copy"] }))))).unwrap();
        conn.execute("UPDATE video_media SET recebido_nativo = 1, recebido_obs = 'do DVR', relogio_desvio_s = -2.5, gravador = 'DVR 1' WHERE id = ?1",
                     [id_uso.to_string()]).unwrap();
        // v1 com um trecho do vídeo derivado e uma narração gravada
        let montagens = paths::dir_montagens_json(&ws);
        std::fs::create_dir_all(montagens.join("audios")).unwrap();
        let audio = b"narracao gravada";
        std::fs::write(montagens.join("audios/n1.webm"), audio).unwrap();
        let id = "b0000000-0000-4000-8000-000000000001";
        let v1 = json!({ "id": id, "titulo": "Furto", "created_at": "2026-11-01T10:00:00Z", "updated_at": "2026-11-02T10:00:00Z",
            "itens": [{ "tipo": "trecho", "id": "t1", "media_id": id_uso.to_string(), "inicio_s": 2.0, "fim_s": 5.0, "velocidade": 1,
                        "zoom": { "inicio": { "x": 0, "y": 0, "w": 1, "h": 1 }, "fim": { "x": 0.1, "y": 0.1, "w": 0.5, "h": 0.5 } } }],
            "saida": { "largura": 1920, "altura": 1080, "fps": 25, "incluir_audio": false },
            "narracoes": [{ "id": "n1", "arquivo_relativo": "Sistema/videos/montagens/audios/n1.webm", "inicio_s": 0.5, "volume": 1.2, "nome": "N" },
                          { "id": "n2", "arquivo_relativo": "../../fora.wav", "inicio_s": 1.0, "volume": 1.0, "nome": "Fora" }] });
        let bytes_v1 = serde_json::to_vec_pretty(&v1).unwrap();
        std::fs::write(montagens.join(format!("{id}.json")), &bytes_v1).unwrap();
        assert!(existe(&ws, id));
        let lista = armazem::listar(&ws).unwrap();
        assert_eq!((lista.len(), lista[0].esquema), (1, 1));

        let a = armazem::abrir(&ws, id).unwrap();
        assert_eq!((a.importado_de, a.so_leitura), (Some(1), false));
        assert!(!existe(&ws, id));
        assert_eq!(std::fs::read(montagens.join(format!("{id}.json.v1"))).unwrap(), bytes_v1, "o v1 só muda de nome");
        assert!(!montagens.join(format!("{id}.json")).exists());
        let gravado: Value = serde_json::from_slice(&std::fs::read(armazem::pasta_projeto(&ws, id).join("projeto.json")).unwrap()).unwrap();
        assert_eq!(gravado, a.projeto);
        assert_eq!(gravado["gravado_por"], json!(APP_VERSION));
        let p = armazem::tipado(&gravado).unwrap();
        let m = &p.midias[0];
        assert_eq!((m.media_id.clone(), m.sha256.as_str(), m.nome.as_str(), m.relativo.as_str()), (Some(id_uso.to_string()), uso.as_str(), "trecho 1.mp4", "Mídias/Vídeos/trecho 1.mp4"));
        assert_eq!((m.largura, m.altura, m.inicio_s, m.duracao_s, m.tem_audio, m.taxa), (Some(1080), Some(1920), Some(1.5), Some(12.5), true, None));
        assert_eq!(m.recebido, Recebido { nativo: Some(true), obs: "do DVR".into() });
        assert_eq!((m.relogio_desvio_s, m.gravador.as_deref()), (Some(-2.5), Some("DVR 1")));
        assert_eq!(m.derivacao, Some(Derivacao { tipo: TipoDerivacao::Trecho, de_sha256: meio.clone(), raiz_sha256: raiz.clone(),
                                                 comando: "ffmpeg -ss 1 -c copy".into(), com_perdas: false, inicio_epoca_s: None }));
        // tamanho exibido (vertical) entra no zoom: h = w × (1080/1920) ÷ (16/9)
        let ClipePrincipal::Trecho(t) = clipes(&p)[0] else { panic!() };
        assert_eq!((t.inicio_pts, t.fim_pts), (None, None), "sem tabela de quadros aqui");
        let Efeito::Enquadramento(z) = efeitos(clipes(&p)[0])[0] else { panic!() };
        let Animavel::Chaves(k) = &z.regiao else { panic!() };
        assert!((k[1].v.h - 0.5 * (1080.0 / 1920.0) / (16.0 / 9.0)).abs() < 1e-12);
        // narração: SHA-256 do arquivo; a que sai do caso fica sem arquivo
        let n = &p.midias[1];
        assert_eq!((n.sha256.clone(), n.origem, n.relativo.as_str()), (crate::hashing::sha256::sha256_bytes(audio), OrigemMidia::Projeto, "Sistema/videos/montagens/audios/n1.webm"));
        assert_eq!(p.midias[2].sha256, "");
        assert!(a.avisos.iter().any(|w| w.codigo == "midia_ausente" && w.midia.as_deref() == Some(p.midias[2].id.as_str())));
        assert_eq!((p.saida.fps, p.saida.audio), (Racional { num: 25, den: 1 }, false));
        // a lista agora mostra o v2; reabrir não importa de novo
        let lista = armazem::listar(&ws).unwrap();
        assert_eq!((lista.len(), lista[0].esquema, lista[0].problema.clone()), (1, 2, None));
        assert_eq!(armazem::abrir(&ws, id).unwrap().importado_de, None);
        assert!(importar(&ws, &conn, id).is_err(), "já importado");

        // duplicar um projeto que só existe no v1 importa antes
        let id2 = "b0000000-0000-4000-8000-000000000002";
        let mut outro = v1.clone();
        outro["id"] = json!(id2);
        std::fs::write(montagens.join(format!("{id2}.json")), outro.to_string()).unwrap();
        let copia = armazem::duplicar(&ws, id2).unwrap();
        assert!(armazem::pasta_projeto(&ws, id2).join("projeto.json").is_file());
        assert!(!existe(&ws, id2));
        assert_ne!(copia.id, id2);
        assert_eq!(armazem::listar(&ws).unwrap().len(), 3);
    }

    /// O fade limitado por quem SAI, a velocidade na duração, transição desconhecida, e o texto no rodapé
    /// (posição e tamanho desconhecidos caem em rodapé e "m", como na v1).
    #[test]
    fn fade_por_quem_sai_e_texto_no_rodape() {
        let texto = |c: &str, pos: &str, tam: &str| json!({ "conteudo": c, "posicao": pos, "tamanho": tam, "carimbo_tempo": false });
        let v1 = json!({ "id": "d0000000-0000-4000-8000-000000000001", "titulo": "Fades", "created_at": "", "updated_at": "",
            "saida": { "largura": 1280, "altura": 720, "fps": 30, "incluir_audio": true },
            "itens": [
                { "tipo": "congelar", "id": "a", "media_id": M1, "instante_s": 1.0, "duracao_s": 0.4, "texto": texto("no rodapé", "rodape", "m") },
                { "tipo": "trecho", "id": "b", "media_id": M1, "inicio_s": 0.0, "fim_s": 10.0, "velocidade": 1.0, "transicao_entrada": "fade",
                  "texto": texto("posição que não existe", "esquerda", "enorme") },
                { "tipo": "trecho", "id": "c", "media_id": M1, "inicio_s": 0.0, "fim_s": 3.0, "velocidade": 0.5, "transicao_entrada": "deslizar" },
                { "tipo": "cartela", "id": "d", "duracao_s": 5.0, "texto": "x", "cor_fundo": "#000000", "cor_texto": "#ffffff", "tamanho": "m", "transicao_entrada": "fade" },
                { "tipo": "trecho", "id": "e", "media_id": M1, "inicio_s": 0.0, "fim_s": 1.0, "velocidade": 4.0, "transicao_entrada": "fade" },
                { "tipo": "cartela", "id": "f", "duracao_s": 5.0, "texto": "x", "cor_fundo": "#000000", "cor_texto": "#ffffff", "tamanho": "m", "transicao_entrada": "fade" },
                { "tipo": "trecho", "id": "g", "media_id": M1, "inicio_s": 0.0, "fim_s": 2.0, "velocidade": 1.0, "transicao_entrada": "corte" }
            ] });
        let p = converter(&v1, &Falsas, AGORA, "6.0.0").unwrap();
        let c = clipes(&p);
        let trans: Vec<Option<f64>> = c.iter().map(|c| match c {
            ClipePrincipal::Trecho(x) => x.base.transicao_entrada.as_ref(),
            ClipePrincipal::Titulo(x) => x.base.transicao_entrada.as_ref(),
            ClipePrincipal::Congelar(x) => x.base.transicao_entrada.as_ref(),
            _ => unreachable!(),
        }.map(|t| { assert_eq!((t.tipo.as_str(), t.audio), ("escurecer", AudioTransicao::Acompanhar)); t.duracao_s })).collect();
        // b: sai 0,4 s → 0,2 · c: "deslizar" não existia → sem · d: 0,5 cheio (sai 6 s a 0,5×, entra 5 s)
        // e: entra 0,25 s (1 s a 4×) → 0,125 · f: sai 0,25 s → 0,125 · g: corte
        assert_eq!(trans, [None, Some(0.2), None, Some(0.5), Some(0.125), Some(0.125), None]);
        let rodape = Lugar { x: 0.5, y: 0.94, h: Alinhamento::Centro, v: AlinhamentoV::Base };
        for (i, tamanho) in [(0, 0.052), (1, 0.052)] {
            let e = efeitos(c[i]);
            let Efeito::Texto(t) = e[0] else { panic!() };
            assert_eq!((e.len(), t.lugar, t.estilo.tamanho), (1, rodape, tamanho));
        }
        assert_eq!((p.created_at.as_str(), p.saida.largura, p.saida.altura), (AGORA, 1280, 720));
    }

    /// Duas aberturas ao mesmo tempo do mesmo v1 (efeito em dobro na tela): uma importa, a outra abre o v2; nenhuma falha.
    #[test]
    fn abrir_em_dobro_importa_uma_vez() {
        let tmp = tempfile::Builder::new().prefix("sicro-p02-").tempdir().unwrap();
        let entrada = crate::models::NewOccurrenceInput { titulo: Some("Roubo Santa Inês".into()), ..Default::default() };
        let ws = crate::workspace::create::create_workspace(entrada, tmp.path()).unwrap().path;
        let montagens = paths::dir_montagens_json(&ws);
        std::fs::create_dir_all(&montagens).unwrap();
        let texto = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edicao/v1-completo.json")).unwrap();
        for volta in 0..5 {
            let id = format!("c0000000-0000-4000-8000-00000000000{volta}");
            let mut v1: Value = serde_json::from_str(&texto).unwrap();
            v1["id"] = json!(id);
            std::fs::write(montagens.join(format!("{id}.json")), v1.to_string()).unwrap();
            let abertos: Vec<_> = std::thread::scope(|s| {
                let fios: Vec<_> = (0..6).map(|_| s.spawn(|| armazem::abrir(&ws, &id))).collect();
                fios.into_iter().map(|f| f.join().unwrap()).collect()
            });
            let importou = abertos.iter().filter(|a| matches!(a, Ok(a) if a.importado_de == Some(1))).count();
            assert!(abertos.iter().all(|a| a.is_ok()), "{:?}", abertos.iter().filter_map(|a| a.as_ref().err()).collect::<Vec<_>>());
            assert_eq!(importou, 1);
            let docs: BTreeSet<String> = abertos.iter().map(|a| a.as_ref().unwrap().projeto.to_string()).collect();
            assert_eq!(docs.len(), 1, "todos veem o mesmo documento");
            assert!(montagens.join(format!("{id}.json.v1")).is_file());
        }
    }
}
