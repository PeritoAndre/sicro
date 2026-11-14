//! Compilador do render (§5.2, §5.3): projeto + contexto + alvo → `Plano`. Puro: sem disco, sem processo.
//! Dono: P0.6a (`audio.rs` é do P0.6b).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::ffmpeg::args as ffmpeg_args;
use super::ffmpeg::sonda::Capacidades;
use super::modelo::{
    ClipeBase, ClipePrincipal, ClipeSobreposto, CorMidia, Efeito, EstadoMidia, Exportacao, Faixa, Fundo, LinhaResolvida, Matriz, ModoLegendas, Nivel,
    ProjetoEdicao, Racional, Tolerante,
};
use super::quadros::TabelaQuadros;
use super::rastreio::avaliar::Como;
use super::rastreio::{ArquivoDenso, ArquivoSilhuetas};
use super::{tempo, validar};
use crate::error::{Result, SicroError};
use std::collections::{BTreeMap, BTreeSet};

pub mod audio;
pub mod blocos;
pub mod clipe;
pub mod efeitos;
pub mod expr;
pub mod grafo;
pub mod juncao;
pub mod plano;
pub mod saida;
pub mod sobrepor;

use clipe::Janela;
use grafo::Grafo;
use juncao::Peca;
use plano::{
    Bloco, Canvas, Entrada, Papel, Plano, ProvAudio, ProvAviso, ProvFaixa, ProvFfmpeg, ProvItem, ProvLegendas, ProvMapa, ProvMedido, ProvMidia,
    ProvNaSaida, ProvOcultacao, ProvOrigem, ProvProcessamento, ProvProjeto, ProvQuadrosOcultos, ProvRastreio, ProvSaida, ProvSilencio, Proveniencia,
};

/// Tudo que o compilador enxerga além do projeto. Montado por `executar::preparar`, o único lugar com E/S.
/// `fontes`: chave = `MidiaRef.id` (só as mídias achadas e íntegras); `midias` = situação de cada `MidiaRef`,
/// que a validação usa (`midia_ausente`, `midia_trocada`, `tabela_pendente`).
// CONTRATO: `midias` não está no §5.2; sem ele `compilar` (que valida) não tinha de onde tirar a situação das mídias.
pub struct Contexto<'a> {
    pub fontes: &'a HashMap<String, FonteInfo>,
    pub cap: &'a Capacidades,
    pub rastreios: &'a dyn LeitorRastreios,
    pub pasta_projeto: &'a Path,
    pub midias: &'a [EstadoMidia],
}

/// Leitura dos arquivos imutáveis de rastreio, por (id do rastreio, SHA-256 do ponteiro).
pub trait LeitorRastreios {
    /// Amostras densas de `rastreios/<rastreio>/<sha256>.json`.
    fn denso(&self, rastreio: &str, sha256: &str) -> Result<Arc<ArquivoDenso>>;
    /// Silhuetas de `silhuetas/<rastreio>/<sha256>.sil`.
    fn silhuetas(&self, rastreio: &str, sha256: &str) -> Result<Arc<ArquivoSilhuetas>>;
}

/// Trilha de áudio da fonte: início (s, tempo de fonte), taxa (Hz) e canais.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioInfo {
    pub inicio_s: f64,
    pub taxa: u32,
    pub canais: u32,
}

/// Uma mídia pronta para o compilador. `exibido` = (largura, altura) em px com giro e proporção do pixel
/// aplicados; `giro` em graus; `matriz` e `faixa` já decididas (etiqueta, presunção ou `MidiaRef.cor`).
/// Imagem: tabela de 1 quadro (pts 0, base 1/1). Arquivo só de áudio: tabela vazia, `exibido` (0, 0) e `audio` Some.
// CONTRATO: `conteiner` e `codec` não estão no §5.2; entram porque `feito.midias` (§10.2) e a escolha da prévia (§7.1) pedem.
#[derive(Debug, Clone)]
pub struct FonteInfo {
    pub caminho: PathBuf,
    pub sha256: String,
    pub inicio_s: f64,
    pub exibido: (u32, u32),
    pub sar: Racional,
    pub giro: i32,
    pub entrelacado: bool,
    pub pix_fmt: String,
    pub matriz: Matriz,
    pub faixa: Faixa,
    pub quadros: Arc<TabelaQuadros>,
    pub audio: Option<AudioInfo>,
    /// `format_name` do ffprobe.
    pub conteiner: String,
    /// `codec_name` do fluxo de vídeo.
    pub codec: String,
}

/// O que renderizar. `Previa`: altura em px e slots [de_q, ate_q). `Quadro`: slot `q` e largura em px.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Alvo {
    Final { exportacao: Exportacao },
    Previa { altura: u32, de_q: u64, ate_q: u64 },
    Quadro { q: u64, largura: u32 },
}

/// Canvas do alvo: o da saída na exportação final; reduzido à altura da prévia ou à largura do quadro final
/// (nunca ampliado), com lados pares.
pub(crate) fn canvas_do_alvo(p: &ProjetoEdicao, alvo: &Alvo) -> Canvas {
    let (w, h) = (p.saida.largura.max(2) as u64, p.saida.altura.max(2) as u64);
    let par = |v: u64| (v.div_ceil(2) * 2).clamp(2, u32::MAX as u64 - 1) as u32;
    let (cw, ch) = match alvo {
        Alvo::Final { .. } => (w as u32, h as u32),
        Alvo::Previa { altura, .. } => {
            let a = (*altura as u64).clamp(2, h);
            (par(w * a / h), par(a))
        }
        Alvo::Quadro { largura, .. } => {
            let l = (*largura as u64).clamp(2, w);
            (par(l), par(h * l / w))
        }
    };
    Canvas { w: cw, h: ch, fps: p.saida.fps, cor_fundo: p.saida.cor_fundo.clone() }
}

fn erro(codigo: &str, detalhe: impl std::fmt::Display) -> SicroError {
    SicroError::Validation(format!("{codigo}: {detalhe}"))
}

/// Erro de `compilar` quando a validação barra: `<código>: <frase do catálogo>`, como `mensagens::erro_de_comando`.
pub(crate) fn erro_da_validacao(e: &super::modelo::Problema) -> SicroError {
    erro(e.codigo, &e.mensagem)
}

fn base_do_clipe(c: &ClipePrincipal) -> &ClipeBase {
    match c {
        ClipePrincipal::Trecho(x) => &x.base,
        ClipePrincipal::Congelar(x) => &x.base,
        ClipePrincipal::Imagem(x) => &x.base,
        ClipePrincipal::Titulo(x) => &x.base,
        ClipePrincipal::Tela(x) => &x.base,
    }
}

fn tipo_do_clipe(c: &ClipePrincipal) -> &'static str {
    match c {
        ClipePrincipal::Trecho(_) => "trecho",
        ClipePrincipal::Congelar(_) => "congelar",
        ClipePrincipal::Imagem(_) => "imagem",
        ClipePrincipal::Titulo(_) => "titulo",
        ClipePrincipal::Tela(_) => "tela",
    }
}

fn clipe_da_principal<'a>(p: &'a ProjetoEdicao, id: &str) -> Result<&'a ClipePrincipal> {
    p.principal
        .iter()
        .find_map(|c| match c {
            Tolerante::Conhecido(c) if base_do_clipe(c).id == id => Some(c),
            _ => None,
        })
        .ok_or_else(|| erro("item_invalido", format!("clipe {id} fora da principal")))
}

/// Compila o plano de render (§5.3): `tempo::resolver` → `validar::validar(p, linha, ctx, Some(alvo))` (havendo
/// erro, `Err` com o primeiro, sem chegar ao motor) → vídeo → áudio → proveniência com os avisos periciais.
pub fn compilar(p: &ProjetoEdicao, ctx: &Contexto, alvo: &Alvo) -> Result<Plano> {
    let linha = tempo::resolver(p, ctx);
    let problemas = validar::validar(p, &linha, ctx, Some(alvo));
    if let Some(e) = problemas.iter().find(|x| x.nivel == Nivel::Erro) {
        return Err(erro_da_validacao(e));
    }
    let mut plano = compilar_video(p, &linha, ctx, alvo)?;
    let (audio, previas) = audio::montar(p, &linha, ctx, alvo)?;
    plano.previas.extend(previas);
    if let Some(a) = &audio {
        let cod = saida::codificacao(alvo, &plano.canvas, ctx.cap)?;
        plano.juncao = blocos::juncao(&plano.blocos, &plano.canvas, &cod, Some(a), alvo);
    }
    plano.audio = audio;
    // avisos periciais presentes, um por código
    let mut avisos: Vec<ProvAviso> = Vec::new();
    for x in problemas.iter().filter(|x| x.nivel == Nivel::Pericial) {
        let n = x.n.unwrap_or(1);
        match avisos.iter_mut().find(|a| a.codigo == x.codigo) {
            Some(a) => a.n += n,
            None => avisos.push(ProvAviso { codigo: x.codigo.to_string(), n }),
        }
    }
    plano.proveniencia.avisos = avisos;
    Ok(plano)
}

/// Só o vídeo (etapas 3 a 7 e 10 do §5.3), sem validar e sem áudio (`Plano.audio` None, `legendas` vazio).
/// É o que `compilar` chama no meio; existe para o motor de vídeo ser testável sem `validar` nem `audio`.
pub fn compilar_video(p: &ProjetoEdicao, linha: &LinhaResolvida, ctx: &Contexto, alvo: &Alvo) -> Result<Plano> {
    let canvas = canvas_do_alvo(p, alvo);
    if canvas.fps.num <= 0 || canvas.fps.den <= 0 || canvas.fps != linha.fps {
        return Err(erro("saida", "taxa"));
    }
    if linha.total_q == 0 || linha.clipes.is_empty() {
        return Err(SicroError::Validation("vazio".into()));
    }
    // clipe de versão futura (ou inválido) na principal: nunca ignorado calado; o código é o da validação
    for c in &p.principal {
        if let Tolerante::Desconhecido(v) = c {
            if !v.get("desligado").and_then(|x| x.as_bool()).unwrap_or(false) {
                return Err(validar::erro_do_desconhecido(v, validar::TIPOS_CLIPE));
            }
        }
    }
    let cod = saida::codificacao(alvo, &canvas, ctx.cap)?;
    // legendas queimadas ou em arquivo são do P1.6: a exportação nunca sai sem elas calada
    if matches!(alvo, Alvo::Final { exportacao } if exportacao.legendas != ModoLegendas::Nenhuma) && !p.legendas.itens.is_empty() {
        return Err(erro("nao_suportado", "legendas"));
    }
    let janelas = blocos::dividir(p, linha, alvo);
    if janelas.is_empty() {
        return Err(erro("intervalo", "janela fora do programa"));
    }
    let exportacao_final = matches!(alvo, Alvo::Final { .. });
    let (mut blocos_prontos, mut camadas) = (Vec::with_capacity(janelas.len()), Vec::new());
    for (indice, jb) in janelas.iter().enumerate() {
        let (de, ate) = (jb.de_q, jb.de_q + jb.n_q as u64);
        let prefixo = ffmpeg_args::nome("b", indice, "");
        let mut g = Grafo::novo();
        let (mut entradas, mut pecas, mut conferir, mut hashes) = (Vec::<Entrada>::new(), Vec::new(), Vec::new(), Vec::new());
        for (i, res) in linha.clipes.iter().enumerate() {
            let (ci, cf) = (res.inicio_q, res.inicio_q + res.n_q as u64);
            if cf <= de || ci >= ate {
                continue;
            }
            if res.sobrepoe_q > 0 {
                return Err(erro("nao_suportado", format!("transicao com sobreposicao no clipe {}", res.id)));
            }
            let c = clipe_da_principal(p, &res.id)?;
            let janela = Janela { de: (de.max(ci) - ci) as u32, ate: (ate.min(cf) - ci) as u32 };
            let mut cad = clipe::cadeia(&mut g, p, c, res, janela, pecas.len(), entradas.len(), ctx, alvo)?;
            // escurecimento: só quando a janela pega os slots dele
            let (d_in, d_out) = blocos::escurecimentos(p, linha, i);
            let seguinte = match linha.clipes.get(i + 1) {
                Some(s) => blocos::transicao_de(p, &s.id),
                None => p.fecho.as_ref(),
            };
            let entrada = base_do_clipe(c).transicao_entrada.as_ref().filter(|_| d_in > 0 && janela.de < d_in).map(|t| (t.clone(), d_in));
            let saida = seguinte.filter(|_| d_out > 0 && janela.ate > res.n_q - d_out).map(|t| (t.clone(), d_out));
            pecas.push(Peca { rotulo: cad.saida.clone(), n_q: res.n_q, entrada, saida });
            // o id da camada é único no plano: ganha o bloco na frente
            for e in &mut cad.entradas {
                if let Papel::Camada { id } = &mut e.papel {
                    *id = format!("{prefixo}{id}");
                }
            }
            for k in &mut cad.camadas {
                k.id = format!("{prefixo}{}", k.id);
                hashes.push(k.hash.clone());
            }
            entradas.append(&mut cad.entradas);
            camadas.append(&mut cad.camadas);
            conferir.extend(cad.conferir);
        }
        let junto = juncao::montar(&mut g, &pecas, canvas.fps)?;
        let mut sob = sobrepor::aplicar(&mut g, &junto, p, linha, jb.de_q, jb.n_q, entradas.len(), ctx, alvo)?;
        if let (Some(mut e), Some(mut k)) = (sob.entrada.take(), sob.camada.take()) {
            k.id = format!("{prefixo}{}", k.id);
            e.papel = Papel::Camada { id: k.id.clone() };
            hashes.push(k.hash.clone());
            entradas.push(e);
            camadas.push(k);
        }
        match alvo {
            Alvo::Final { .. } => saida::fecho(&mut g, &sob.saida, true),
            Alvo::Previa { .. } => saida::fecho(&mut g, &sob.saida, false),
            Alvo::Quadro { .. } => saida::fecho_quadro(&mut g, &sob.saida),
        }
        limitar_decodificadores(&mut entradas);
        let grafo = g.texto();
        let codificador = saida::argumentos_do_bloco(&cod);
        // a chave leva os argumentos de leitura (busca) e o SHA-256 de cada mídia, nunca o caminho
        let mut args_da_chave = codificador.clone();
        let mut midias = Vec::new();
        for e in &entradas {
            args_da_chave.push("-i".into());
            args_da_chave.extend(e.antes.iter().cloned());
            if let Papel::Midia { id, .. } = &e.papel {
                midias.push(ctx.fontes.get(id).map(|f| f.sha256.clone()).unwrap_or_default());
            }
        }
        let chave = blocos::chave(&grafo, &args_da_chave, &midias, &hashes, &ctx.cap.sha256);
        blocos_prontos.push(Bloco {
            indice,
            de_q: jb.de_q,
            n_q: jb.n_q,
            chave,
            entradas,
            grafo,
            auxiliares: Vec::new(),
            codificador,
            saida: ffmpeg_args::nome("b", indice, &format!(".{}", cod.extensao)),
            framehash: if exportacao_final { ffmpeg_args::nome("b", indice, ".framehash") } else { String::new() },
            conferir,
        });
    }
    let total_q = blocos_prontos.iter().map(|b| b.n_q as u64).sum();
    let juncao = blocos::juncao(&blocos_prontos, &canvas, &cod, None, alvo);
    let proveniencia = proveniencia(p, linha, ctx, alvo)?;
    Ok(Plano { alvo: alvo.clone(), canvas, total_q, previas: Vec::new(), camadas, blocos: blocos_prontos, audio: None, legendas: Vec::new(), juncao, proveniencia })
}

/// Threads de decodificação por entrada de mídia, pelo número delas no bloco. Cada decodificador segura
/// quadros por thread: com 12 entradas de 1080p e as threads automáticas o 7.0.2 passa de 2 GiB só nisso
/// (0,43 GiB com 1 thread; o 9.0.1 fica em 0,4 a 0,5 GiB de qualquer jeito). Os clipes de um bloco são lidos um
/// depois do outro, e o conteúdo decodificado não depende do número de threads.
fn limitar_decodificadores(entradas: &mut [Entrada]) {
    let midias = entradas.iter().filter(|e| matches!(e.papel, Papel::Midia { .. })).count();
    let threads = match midias {
        0..=2 => return,
        3..=4 => "2",
        _ => "1",
    };
    for e in entradas.iter_mut().filter(|e| matches!(e.papel, Papel::Midia { .. })) {
        let com = |v: &[String]| ["-threads".to_string(), threads.to_string()].into_iter().chain(v.iter().cloned()).collect::<Vec<_>>();
        e.antes = com(&e.antes);
        e.antes_sem_busca = e.antes_sem_busca.as_ref().map(|v| com(v));
    }
}

/// Posição do efeito na cadeia do clipe (§5.3, `clipe.rs`): `ocultar` (domínio da fonte) → origem → giro →
/// enquadramento → holofote → lupa → anotação (desenhada na ordem da lista).
fn lugar_na_cadeia(e: &Efeito) -> u8 {
    match e {
        Efeito::Ocultar(_) => 0,
        e if efeitos::familia(e) == efeitos::Familia::Origem => 1,
        Efeito::Girar(_) => 2,
        Efeito::Enquadramento(_) => 3,
        Efeito::Holofote(_) => 4,
        Efeito::Lupa(_) => 5,
        _ => 6,
    }
}

/// Faixas unidas (s) e o total delas.
fn unir_faixas(mut v: Vec<(f64, f64)>) -> (Vec<ProvFaixa>, f64) {
    v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut u: Vec<ProvFaixa> = Vec::new();
    for (de, ate) in v {
        match u.last_mut() {
            Some(f) if de <= f.ate_s => f.ate_s = f.ate_s.max(ate),
            _ => u.push(ProvFaixa { de_s: de, ate_s: ate }),
        }
    }
    let total = u.iter().map(|f| f.ate_s - f.de_s).sum();
    (u, total)
}

/// O `feito` do `tecnico.json` que sai do projeto e das tabelas: sempre do programa inteiro, qualquer que seja o alvo
/// (prévia e quadro final dão a mesma da exportação com a exportação do projeto).
fn proveniencia(p: &ProjetoEdicao, linha: &LinhaResolvida, ctx: &Contexto, alvo: &Alvo) -> Result<Proveniencia> {
    let fps = linha.fps;
    let exportacao = match alvo {
        Alvo::Final { exportacao } => exportacao.clone(),
        _ => p.saida.exportacao.clone(),
    };
    let final_ = Alvo::Final { exportacao: exportacao.clone() };
    let canvas = canvas_do_alvo(p, &final_);
    let total_q = linha.total_q;
    let tolerancia_atempo_ms = audio::tolerancia_atempo_do_programa(p, linha, ctx);
    let seg = |q: u64| tempo::instante_do_slot(q, fps);
    let mut rs = clipe::Rastreios::novo(p, ctx);
    let mut usados: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    let mut citadas: BTreeSet<String> = BTreeSet::new();
    let mut rastreios_usados: BTreeSet<String> = BTreeSet::new();
    let (mut roteiro, mut ocultacoes, mut silencios) = (Vec::new(), Vec::<ProvOcultacao>::new(), Vec::new());
    let mut ids_das_ocultacoes: Vec<(String, String)> = Vec::new();
    for (i, res) in linha.clipes.iter().enumerate() {
        let c = clipe_da_principal(p, &res.id)?;
        let b = base_do_clipe(c);
        let (mut origem, mut velocidade) = (None, None);
        // (mídia, quadros lidos) de quem lê uma fonte
        let lido = match c {
            ClipePrincipal::Trecho(t) => res.fonte.as_ref().map(|f| (t.midia.as_str(), f.j_in as usize, f.j_out as usize)),
            ClipePrincipal::Congelar(g) => res.fonte.as_ref().map(|f| (g.midia.as_str(), f.j_in as usize, f.j_in as usize + 1)),
            _ => None,
        };
        if let Some((midia, j_in, j_out)) = lido {
            let fonte = ctx.fontes.get(midia).ok_or_else(|| erro("midia_ausente", midia))?;
            let tab = &fonte.quadros;
            if j_in >= j_out || j_out > tab.pts.len() {
                return Err(erro("intervalo", &res.id));
            }
            citadas.insert(midia.to_string());
            let (de_s, ate_s) = (tab.tempo(j_in), if j_out < tab.pts.len() { tab.tempo(j_out) } else { tab.fim_s() });
            usados.entry(midia.to_string()).or_default().push((de_s, ate_s));
            let (mapa, quadro_do_slot, repetidos, descartados) = match c {
                ClipePrincipal::Trecho(t) => {
                    let m = tempo::mapa_trecho(tab, j_in, j_out, &t.velocidade, fps)?;
                    velocidade = Some(t.velocidade.clone());
                    for s in &t.audio.silencios {
                        silencios.push(ProvSilencio { midia: midia.to_string(), de_s: s.de, ate_s: s.ate, modo: s.modo, motivo: s.motivo.clone() });
                    }
                    (ProvMapa { c: m.c, kn: m.kn, kd: m.kd, e: m.e }, m.quadro_do_slot, m.repetidos, m.descartados)
                }
                _ => (ProvMapa { c: 0, kn: 0, kd: 0, e: 0 }, vec![j_in as u32; res.n_q as usize], res.n_q.saturating_sub(1), 0),
            };
            origem = Some(ProvOrigem {
                midia: midia.to_string(),
                de_s,
                ate_s,
                pts_primeiro: tab.pts[j_in],
                pts_ultimo: tab.pts[j_out - 1],
                lidos: (j_out - j_in) as u32,
                repetidos,
                descartados,
                mapa,
                j_in: j_in as u32,
                quadro_do_slot,
                pts: tab.pts[j_in..j_out].to_vec(),
            });
            // ocultações aplicadas neste clipe, por quadro lido (no congelado, com o `ativo` local já resolvido)
            let midia_ref = p.midias.iter().find(|m| m.id == midia);
            let efeitos_oc = match c {
                ClipePrincipal::Congelar(_) => std::borrow::Cow::Owned(clipe::efeitos_do_congelado(&b.efeitos, res.n_q, fps)?),
                _ => std::borrow::Cow::Borrowed(&b.efeitos[..]),
            };
            for o in clipe::ocultacoes_de(midia_ref, &efeitos_oc) {
                rastreios_usados.insert(o.alvo.rastreio.clone());
                let (mut n, mut de, mut ate) = (ProvQuadrosOcultos { ancora: 0, medido: 0, interpolado: 0, incerto: 0 }, f64::INFINITY, f64::NEG_INFINITY);
                for j in j_in..j_out {
                    let Some(a) = clipe::caixa_da_ocultacao(&mut rs, o, tab, j)? else { continue };
                    match a.como {
                        Como::Ancora | Como::Segurado => n.ancora += 1,
                        Como::Denso => n.medido += 1,
                        Como::Interpolado => n.interpolado += 1,
                    }
                    n.incerto += u32::from(a.incerto);
                    (de, ate) = (de.min(tab.tempo(j)), ate.max(tab.tempo(j)));
                }
                if de > ate {
                    continue;
                }
                // a mesma ocultação (mídia + id do efeito) em vários clipes vira uma linha só
                let ja_tem = ids_das_ocultacoes.iter().position(|(m, id)| m == midia && *id == o.base.id);
                match ja_tem.map(|k| &mut ocultacoes[k]) {
                    Some(x) => {
                        x.clipes.push(res.id.clone());
                        (x.de_s, x.ate_s) = (x.de_s.min(de), x.ate_s.max(ate));
                        x.quadros = ProvQuadrosOcultos {
                            ancora: x.quadros.ancora + n.ancora,
                            medido: x.quadros.medido + n.medido,
                            interpolado: x.quadros.interpolado + n.interpolado,
                            incerto: x.quadros.incerto + n.incerto,
                        };
                    }
                    None => {
                        ids_das_ocultacoes.push((midia.to_string(), o.base.id.clone()));
                        ocultacoes.push(ProvOcultacao {
                        midia: midia.to_string(),
                        clipes: vec![res.id.clone()],
                        de_s: de,
                        ate_s: ate,
                        modo: o.modo,
                        forma: o.alvo.forma,
                        intensidade: o.intensidade,
                        conteudo: o.conteudo,
                        motivo: o.motivo.clone(),
                        quadros: n,
                        inverter: o.alvo.inverter,
                        });
                    }
                }
            }
        }
        match c {
            ClipePrincipal::Imagem(x) => {
                citadas.insert(x.midia.clone());
            }
            ClipePrincipal::Titulo(x) => {
                if let Fundo::Imagem { midia } = &x.titulo.fundo {
                    citadas.insert(midia.clone());
                }
            }
            _ => {}
        }
        // no congelado, o `ocultar` do clipe que não pega nenhum slot sai desligado da cadeia
        let desligadas: Vec<String> = match c {
            ClipePrincipal::Congelar(_) => clipe::efeitos_do_congelado(&b.efeitos, res.n_q, fps)?
                .iter()
                .filter_map(|e| match e {
                    Tolerante::Conhecido(Efeito::Ocultar(o)) if !o.base.ligado => Some(o.base.id.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let mut aplicados: Vec<&Efeito> = b
            .efeitos
            .iter()
            .filter_map(|e| match e {
                Tolerante::Conhecido(e) if efeitos::base(e).ligado && !desligadas.contains(&efeitos::base(e).id) => Some(e),
                _ => None,
            })
            .collect();
        // `ordem` é a da cadeia (§5.3), não a da lista do clipe
        aplicados.sort_by_key(|e| lugar_na_cadeia(e));
        let mut processamentos = Vec::new();
        for e in aplicados {
            match e {
                Efeito::Ocultar(o) => drop(rastreios_usados.insert(o.alvo.rastreio.clone())),
                Efeito::Holofote(h) => drop(rastreios_usados.insert(h.alvo.rastreio.clone())),
                Efeito::Destaque(d) => drop(rastreios_usados.insert(d.rastreio.clone())),
                Efeito::Imagem(im) => drop(citadas.insert(im.midia.clone())),
                _ => {}
            }
            processamentos.push(ProvProcessamento { ordem: processamentos.len() as u32 + 1, nome: efeitos::tipo(e).to_string(), parametros: serde_json::to_value(e)? });
        }
        roteiro.push(ProvItem {
            n: i as u32 + 1,
            clipe: res.id.clone(),
            nome: b.rotulo.clone(),
            tipo: tipo_do_clipe(c).to_string(),
            na_saida: ProvNaSaida { de_q: res.inicio_q, n_q: res.n_q, de_s: seg(res.inicio_q), ate_s: seg(res.inicio_q + res.n_q as u64) },
            origem,
            velocidade,
            transicao: b.transicao_entrada.clone(),
            processamentos,
            selos: Vec::new(),
        });
    }
    for trilha in p.sobreposicoes.iter().filter(|t| !t.oculta) {
        for c in &trilha.clipes {
            match c {
                Tolerante::Conhecido(ClipeSobreposto::Imagem(x)) => drop(citadas.insert(x.midia.clone())),
                Tolerante::Conhecido(ClipeSobreposto::Titulo(x)) => {
                    if let Fundo::Imagem { midia } = &x.titulo.fundo {
                        citadas.insert(midia.clone());
                    }
                }
                _ => {}
            }
        }
    }
    // trilhas de som (narração): a mídia de cada clipe que toca também é fonte do vídeo entregue
    if p.saida.audio {
        for trilha in p.audios.iter().filter(|t| !t.mudo) {
            for c in trilha.clipes.iter().filter(|c| linha.livres.iter().any(|l| l.id == c.id && l.trilha == trilha.id)) {
                citadas.insert(c.midia.clone());
            }
        }
    }
    let mut midias = Vec::new();
    for m in p.midias.iter().filter(|m| citadas.contains(&m.id)) {
        let Some(f) = ctx.fontes.get(&m.id) else { continue };
        let tab = &f.quadros;
        let (de_s, ate_s) = if tab.pts.is_empty() { (0.0, 0.0) } else { (tab.tempo(0), tab.fim_s()) };
        let mut discrepancias = Vec::new();
        if tab.declarado.nb_frames.is_some_and(|n| n != tab.pts.len() as u64) {
            discrepancias.push("nb_frames".to_string());
        }
        let passo_s = tab.passo_mediano as f64 * tab.tb.num as f64 / tab.tb.den.max(1) as f64;
        if tab.declarado.duracao.is_some_and(|d| (d - (ate_s - de_s)).abs() > 2.0 * passo_s.abs()) && tab.pts.len() > 1 {
            discrepancias.push("duracao".to_string());
        }
        if !tab.declarado.r_frame_rate.is_empty() && !tab.declarado.avg_frame_rate.is_empty() && tab.declarado.r_frame_rate != tab.declarado.avg_frame_rate {
            discrepancias.push("taxa".to_string());
        }
        let (usado, usado_total_s) = unir_faixas(usados.remove(&m.id).unwrap_or_default());
        midias.push(ProvMidia {
            id: m.id.clone(),
            nome: m.nome.clone(),
            sha256: f.sha256.clone(),
            derivacao: m.derivacao.clone(),
            recebido: m.recebido.clone(),
            relogio_desvio_s: m.relogio_desvio_s,
            gravador: m.gravador.clone(),
            cor: CorMidia { matriz: f.matriz, faixa: f.faixa },
            conteiner: f.conteiner.clone(),
            codec: f.codec.clone(),
            pix_fmt: f.pix_fmt.clone(),
            sar: f.sar,
            giro: f.giro,
            inicio_s: f.inicio_s,
            declarado: tab.declarado.clone(),
            medido: ProvMedido { tb: tab.tb, de_s, ate_s, pacotes: tab.pacotes, quadros: tab.pts.len() as u64, passo_min: tab.passo.0, passo_med: tab.passo_mediano, passo_max: tab.passo.1, lacunas: tab.lacunas.clone() },
            discrepancias,
            erros_decodificacao: tab.erros_decodificacao,
            usado,
            usado_total_s,
        });
    }
    let mut rastreios = Vec::new();
    for r in p.rastreios.iter().filter(|r| rastreios_usados.contains(&r.id)) {
        let corridas = match &r.denso {
            Some(d) => ctx.rastreios.denso(&r.id, &d.sha256)?.corridas.clone(),
            None => Vec::new(),
        };
        rastreios.push(ProvRastreio {
            id: r.id.clone(),
            origem: r.origem.clone(),
            ancoras: r.ancoras.len() as u32,
            denso_sha256: r.denso.as_ref().map(|d| d.sha256.clone()),
            silhuetas_sha256: r.silhuetas.as_ref().map(|s| s.sha256.clone()),
            corridas,
            confirmacao: r.confirmacao.clone(),
        });
    }
    let selos = &p.saida.selos;
    let selos_desligados = [("velocidade", selos.velocidade), ("ampliacao", selos.ampliacao), ("realce", selos.realce)].iter().filter(|(_, ligado)| !ligado).map(|(n, _)| n.to_string()).collect();
    Ok(Proveniencia {
        projeto: ProvProjeto { id: p.id.clone(), titulo: p.titulo.clone(), esquema: p.esquema, esquema_menor: p.esquema_menor },
        saida: ProvSaida {
            largura: canvas.w,
            altura: canvas.h,
            fps,
            duracao_s: seg(total_q),
            n_quadros: total_q,
            predefinicao: Some(exportacao.predefinicao),
            com_perdas: saida::com_perdas(&final_),
        },
        midias,
        roteiro,
        ocultacoes,
        audio: ProvAudio { silencios, tolerancia_atempo_ms },
        legendas: ProvLegendas { origem: p.legendas.origem.clone(), n: p.legendas.itens.len() as u32, conferidas: p.legendas.itens.iter().filter(|l| l.conferida).count() as u32 },
        sincronias: Vec::new(),
        rastreios,
        ia: Vec::new(),
        estabilizacoes: Vec::new(),
        avisos: Vec::new(),
        selos_desligados,
        revisado_por: exportacao.revisado_por.clone(),
        ffmpeg: ProvFfmpeg { versao: ctx.cap.versao.clone(), configuracao: ctx.cap.configuracao.clone(), origem: ctx.cap.origem, sha256: ctx.cap.sha256.clone() },
    })
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    #[test]
    fn alvo_tag_tipo() {
        let casos = vec![
            json!({ "tipo": "final", "exportacao": { "predefinicao": "juri", "codificador": "auto", "legendas": "nenhuma", "pacote": null,
                                                    "pdf": "a3", "revisado_por": "", "personalizada": null } }),
            json!({ "tipo": "previa", "altura": 540, "de_q": 0, "ate_q": 90 }),
            json!({ "tipo": "quadro", "q": 14000, "largura": 1920 }),
        ];
        for j in casos {
            let a: Alvo = serde_json::from_value(j.clone()).expect("lê");
            assert_eq!(serde_json::to_value(&a).unwrap(), j);
        }
    }
}
