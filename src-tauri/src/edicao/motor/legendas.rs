//! Legendas no render (§5.3 passos 6 e 9): queimadas na camada global (`EfTexto` por trecho exibido, no slot certo) e
//! em arquivo (`.srt` e `.vtt` ao lado do vídeo; sem faixa dentro do contêiner, B9). As duas saem da mesma lista
//! (`exibidas`): a imagem e o arquivo mostram as mesmas linhas nos mesmos slots. Legenda de fonte acompanha o SOM do
//! trecho (velocidade, corte no meio, repetição). Dono: P1.6. Espelho: `modelo/legendas.ts` (`legendas-exibidas.json`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::edicao::camada::formas::QuadroCamada;
use crate::edicao::geometria::Afim;
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{
    ClipePrincipal, ClipeTrecho, EfBase, EfTexto, Efeito, Exportacao, Fonte, LinhaResolvida, ModoLegendas, OrigemLegenda, ProjetoEdicao, Racional,
    RefLegenda, TamanhoPx, Tolerante, Velocidade,
};
use crate::edicao::motor::plano::{ArquivoTexto, Desenho, ItemCamada, QuadroItem};
use crate::edicao::motor::{Alvo, Contexto};
use crate::edicao::tempo;
use crate::error::{Result, SicroError};

/// Largura máxima da legenda, em fração da largura do canvas: limite do texto no sprite e da quebra de linha.
pub const LARGURA: f64 = 0.9;
/// Avanço que a quebra supõe por caractere, em em. Medido no sprite do Typst: Inter 0,48 (negrito 0,49), Libertinus
/// 0,41 (0,45), DejaVu Mono 0,60; a folga faz a linha caber sem o Typst quebrar de novo.
pub const AVANCO: f64 = 0.6;
pub const AVANCO_MONO: f64 = 0.62;
/// Teto de caracteres por linha (estilo estragado não pede linha sem fim).
pub const MAX_POR_LINHA: usize = 10_000;
/// Teto da legenda no eixo de slots do trecho (legenda sem fim não estoura a conta das páginas); cabe todo `n_q`.
const MAX_SLOTS: f64 = 4_294_967_296.0;
/// Nomes na pasta de trabalho; a publicação troca pelo título do vídeo (`<Título>.srt`).
pub const SRT: &str = "legendas.srt";
pub const VTT: &str = "legendas.vtt";

/// Legenda na tela: slots [de_q, ate_q) do programa e as linhas, como saem na imagem e no arquivo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Exibida {
    pub de_q: u64,
    pub ate_q: u64,
    pub linhas: Vec<String>,
}

/// Uma regra só: `validar` (fase_0) e `recusar` perguntam aqui. None = as legendas saem (P1.6 entregue).
pub fn recusa(p: &ProjetoEdicao, exportacao: &Exportacao) -> Option<&'static str> {
    let _ = (p, exportacao);
    None
}

fn modo(p: &ProjetoEdicao, alvo: &Alvo) -> ModoLegendas {
    match alvo {
        Alvo::Final { exportacao } => exportacao.legendas,
        // prévia e quadro final mostram o que a última escolha de exportação queima
        _ => p.saida.exportacao.legendas,
    }
}

fn erro(c: &str, id: &str) -> SicroError {
    SicroError::Validation(format!("{c}: {id}"))
}

/// O motor nunca entrega, calada, legenda inválida ou automática não conferida: a validação acusa antes (`legenda`,
/// `legenda_nao_conferida`); aqui é a segunda trava, só quando saem. A de silêncio (precisa da linha) está em
/// `queimadas` e `arquivos`.
pub fn recusar(p: &ProjetoEdicao, alvo: &Alvo) -> Result<()> {
    let Alvo::Final { exportacao } = alvo else { return Ok(()) };
    if exportacao.legendas == ModoLegendas::Nenhuma {
        return Ok(());
    }
    for l in &p.legendas.itens {
        let (de, ate) = intervalo(&l.alvo);
        if !(de.is_finite() && ate.is_finite() && ate > de) || l.texto.trim().is_empty() {
            return Err(erro(codigo::LEGENDA, &l.id));
        }
        if l.origem == OrigemLegenda::Ia && !l.conferida {
            return Err(erro(codigo::LEGENDA_NAO_CONFERIDA, &l.id));
        }
    }
    Ok(())
}

fn intervalo(r: &RefLegenda) -> (f64, f64) {
    match r {
        RefLegenda::Fonte { de, ate, .. } | RefLegenda::Programa { de, ate } => (*de, *ate),
    }
}

/// Arredondamento ao slot: `floor(x + 0,5)` (o `Math.floor(x + 0.5)` do espelho dá os mesmos bits).
fn arredondar(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// Slots [a, b) de 0..n por um par de posições já em slots (fracionários); None = vazio ou não finito.
fn cortar(xa: f64, xb: f64, n: u64) -> Option<(u64, u64)> {
    if !(xa.is_finite() && xb.is_finite()) {
        return None;
    }
    let lim = n as f64;
    let (a, b) = (arredondar(xa).clamp(0.0, lim) as u64, arredondar(xb).clamp(0.0, lim) as u64);
    (b > a).then_some((a, b))
}

/// Trechos ligados da principal com o clipe resolvido: (início no programa, n_q, início na fonte, fator, clipe).
/// Rampa (P3.4): fator 0, e o tempo de programa sai de `tempo::programa_da_fonte` (`no_trecho`).
fn trechos<'a>(p: &'a ProjetoEdicao, linha: &LinhaResolvida) -> Vec<(u64, u64, f64, f64, &'a ClipeTrecho)> {
    let mut v = Vec::new();
    for r in linha.clipes.iter().filter(|r| r.fonte.is_some()) {
        let t = p.principal.iter().find_map(|c| match c {
            Tolerante::Conhecido(ClipePrincipal::Trecho(t)) if t.base.id == r.id && !t.base.desligado => Some(t),
            _ => None,
        });
        let Some(t) = t else { continue };
        let fator = match &t.velocidade {
            Velocidade::Constante { fator } if fator.is_finite() && *fator > 0.0 => *fator,
            Velocidade::Rampa { pontos } if tempo::pontos_da_rampa_ok(pontos).is_ok() => 0.0,
            _ => continue,
        };
        v.push((r.inicio_q, r.n_q as u64, t.inicio_s, fator, t));
    }
    v
}

/// Segundos de programa, desde o início do trecho, do instante de fonte `s` (fator 0 = rampa).
fn no_trecho(t: &ClipeTrecho, ini_s: f64, fator: f64, s: f64) -> f64 {
    if fator > 0.0 {
        return (s - ini_s) / fator;
    }
    tempo::programa_da_fonte(&t.velocidade, ini_s, s).unwrap_or(f64::NAN)
}

/// A legenda inteira no eixo de slots da janela (0 = primeiro slot dela), sem cortar: pode começar antes de 0 e acabar
/// depois do fim. Mesmo arredondamento de `cortar`; None = vazia ou não finita.
fn inteira(xa: f64, xb: f64) -> Option<(i64, i64)> {
    if !(xa.is_finite() && xb.is_finite()) {
        return None;
    }
    let c = |x: f64| arredondar(x).clamp(-MAX_SLOTS, MAX_SLOTS) as i64;
    let (a, b) = (c(xa), c(xb));
    (b > a).then_some((a, b))
}

/// Uma aparição da legenda: índice em `itens`, os slots [xa, xb) que ela INTEIRA ocupa no eixo da janela, o início
/// da janela no programa e o tamanho dela. De fonte: a janela é o trecho, pelo tempo do SOM dele (a conta do áudio:
/// programa = início do clipe + (t − inicio_s)/v), e a legenda pode sair dela (o que sobra não toca). De programa: a
/// janela é o programa e a legenda já vem cortada a ele.
struct Aparicao {
    i: usize,
    xa: i64,
    xb: i64,
    ini_q: u64,
    n_q: u64,
}

impl Aparicao {
    /// Slots [a, b) do programa do pedaço [x, y) do eixo da janela; None = fora dela.
    fn visivel(&self, x: i64, y: i64) -> Option<(u64, u64)> {
        let (a, b) = (x.max(0), y.min(self.n_q as i64));
        (a < b).then(|| (self.ini_q + a as u64, self.ini_q + b as u64))
    }
}

/// Aparições na ordem das legendas e, em cada uma, na dos trechos da principal. Sem texto ou fora do programa: nenhuma.
fn aparicoes(p: &ProjetoEdicao, linha: &LinhaResolvida) -> Vec<Aparicao> {
    let fps = linha.fps;
    if fps.num <= 0 || fps.den <= 0 {
        return Vec::new();
    }
    let (num, den) = (fps.num as f64, fps.den as f64);
    let trechos = trechos(p, linha);
    let mut v = Vec::new();
    for (i, l) in p.legendas.itens.iter().enumerate() {
        if vazio(&l.texto) {
            continue;
        }
        match &l.alvo {
            RefLegenda::Programa { de, ate } => {
                let x = |t: f64| t * num / den;
                if let Some((a, b)) = cortar(x(*de), x(*ate), linha.total_q) {
                    v.push(Aparicao { i, xa: a as i64, xb: b as i64, ini_q: 0, n_q: linha.total_q });
                }
            }
            RefLegenda::Fonte { midia, de, ate } => {
                for &(ini_q, n_q, ini_s, fator, t) in &trechos {
                    if &t.midia != midia {
                        continue;
                    }
                    // rampa: pela integral (a mesma conta do vídeo e do som)
                    let x = |s: f64| if fator > 0.0 { (s - ini_s) * num / den / fator } else { no_trecho(t, ini_s, fator, s) * num / den };
                    let Some((xa, xb)) = inteira(x(*de), x(*ate)) else { continue };
                    let a = Aparicao { i, xa, xb, ini_q, n_q };
                    if a.visivel(xa, xb).is_some() {
                        v.push(a);
                    }
                }
            }
        }
    }
    v
}

/// Onde cada legenda aparece: (índice em `itens`, slots [a, b) do programa). De programa: `round(t·fps)`, como o clipe
/// livre. De fonte: em cada trecho ligado da mídia, na ordem da principal, arredondado ao slot e cortado ao clipe.
pub fn no_programa(p: &ProjetoEdicao, linha: &LinhaResolvida) -> Vec<(usize, u64, u64)> {
    aparicoes(p, linha).iter().filter_map(|x| x.visivel(x.xa, x.xb).map(|(a, b)| (x.i, a, b))).collect()
}

/// Ids das legendas que mostrariam o que um `Silencio` cala (regra única da validação e do motor). O silêncio age só
/// na janela do trecho (o som que toca, mesma conta de `aparicoes`). De fonte: o tempo dela cruza o silêncio na mesma
/// mídia; de programa: cruza o tempo do programa em que ele age. Sem arredondar ao slot: sobra de meio slot já leva
/// parte da fala calada.
pub fn sob_silencio(p: &ProjetoEdicao, linha: &LinhaResolvida) -> Vec<String> {
    let fps = linha.fps;
    if fps.num <= 0 || fps.den <= 0 {
        return Vec::new();
    }
    let (num, den) = (fps.num as f64, fps.den as f64);
    // (mídia, de, ate na fonte, de, ate no programa), já cortados à janela do trecho
    let mut agem: Vec<(&str, f64, f64, f64, f64)> = Vec::new();
    for (ini_q, n_q, ini_s, fator, t) in trechos(p, linha) {
        // rampa: a janela vai até o fim do trecho na fonte
        let fim_s = if fator > 0.0 { ini_s + n_q as f64 * den / num * fator } else { t.fim_s };
        let t0 = ini_q as f64 * den / num;
        for s in t.audio.silencios.iter().filter(|s| s.de.is_finite() && s.ate.is_finite()) {
            let (a, b) = (s.de.max(ini_s), s.ate.min(fim_s));
            if a < b {
                agem.push((t.midia.as_str(), a, b, t0 + no_trecho(t, ini_s, fator, a), t0 + no_trecho(t, ini_s, fator, b)));
            }
        }
    }
    let mut ids: Vec<String> = Vec::new();
    let mut vistos = std::collections::HashSet::new();
    for l in p.legendas.itens.iter().filter(|l| !vazio(&l.texto)) {
        let cruza = agem.iter().any(|&(m, fa, fb, pa, pb)| match &l.alvo {
            RefLegenda::Fonte { midia, de, ate } => midia == m && de < ate && *de < fb && fa < *ate,
            RefLegenda::Programa { de, ate } => de < ate && *de < pb && pa < *ate,
        });
        if cruza && vistos.insert(l.id.as_str()) {
            ids.push(l.id.clone());
        }
    }
    ids
}

fn sem_silencio(p: &ProjetoEdicao, linha: &LinhaResolvida) -> Result<()> {
    match sob_silencio(p, linha).first() {
        Some(id) => Err(erro(codigo::LEGENDA_SOB_SILENCIO, id)),
        None => Ok(()),
    }
}

/// Separa palavras (o espaço sem quebra, não: fica dentro da palavra).
fn espaco(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\u{0b}' | '\u{0c}')
}

/// Texto sem nada além de espaço e quebra: a legenda não aparece.
fn vazio(t: &str) -> bool {
    t.chars().all(|c| espaco(c) || c == '\n')
}

/// Caracteres por linha que cabem em `LARGURA` no estilo da trilha (pela proporção da saída: a prévia quebra igual).
pub fn por_linha(p: &ProjetoEdicao) -> usize {
    let e = &p.legendas.estilo;
    let avanco = if e.fonte == Fonte::Mono { AVANCO_MONO } else { AVANCO };
    let x = LARGURA * p.saida.largura as f64 / (avanco * e.tamanho * p.saida.altura as f64);
    if !(x >= 1.0) {
        1
    } else if x > MAX_POR_LINHA as f64 {
        MAX_POR_LINHA
    } else {
        x.floor() as usize
    }
}

/// Linhas do texto: quebra do perito respeitada, palavras juntas até `n` caracteres, palavra maior partida.
pub fn quebrar(texto: &str, n: usize) -> Vec<String> {
    let n = n.max(1);
    let mut linhas = Vec::new();
    for paragrafo in texto.split('\n') {
        let mut atual: Vec<char> = Vec::new();
        for palavra in paragrafo.split(espaco).filter(|w| !w.is_empty()) {
            let cs: Vec<char> = palavra.chars().collect();
            if !atual.is_empty() && atual.len() + 1 + cs.len() <= n {
                atual.push(' ');
                atual.extend(&cs);
                continue;
            }
            if !atual.is_empty() {
                linhas.push(atual.iter().collect());
                atual.clear();
            }
            let mut pedacos = cs.chunks(n).peekable();
            while let Some(c) = pedacos.next() {
                if pedacos.peek().is_some() {
                    linhas.push(c.iter().collect());
                } else {
                    atual.extend(c);
                }
            }
        }
        if !atual.is_empty() {
            linhas.push(atual.iter().collect());
        }
    }
    linhas
}

/// Páginas de até `linhas_max` linhas nos slots [a, b) (eixo da janela): cada uma com pelo menos 1 slot e o resto pela
/// quantidade de caracteres (com poucos slots, mais linhas por página).
fn paginas(linhas: Vec<String>, linhas_max: u32, a: i64, b: i64) -> Vec<(i64, i64, Vec<String>)> {
    let total = linhas.len() as u128;
    if total == 0 || b <= a {
        return Vec::new();
    }
    let n = (b - a) as u128;
    let mut por = (linhas_max.max(1) as u128).min(total);
    if total.div_ceil(por) > n {
        por = total.div_ceil(n);
    }
    let grupos: Vec<Vec<String>> = linhas.chunks(por as usize).map(|g| g.to_vec()).collect();
    let k = grupos.len() as u128;
    let pesos: Vec<u128> = grupos.iter().map(|g| g.iter().map(|l| l.chars().count() as u128).sum::<u128>().max(1)).collect();
    let soma: u128 = pesos.iter().sum();
    let (mut acc, mut v) = (0u128, Vec::with_capacity(grupos.len()));
    for (i, g) in grupos.into_iter().enumerate() {
        let ini = a + (i as u128 + (n - k) * acc / soma) as i64;
        acc += pesos[i];
        let fim = a + (i as u128 + 1 + (n - k) * acc / soma) as i64;
        v.push((ini, fim, g));
    }
    v
}

/// O que a tela mostra, em ordem de slot: cada faixa seguida com as mesmas linhas vira uma `Exibida`. Legendas ao
/// mesmo tempo empilham (a que começou antes em cima). A imagem e os arquivos saem daqui.
pub fn exibidas(p: &ProjetoEdicao, linha: &LinhaResolvida) -> Vec<Exibida> {
    let n = por_linha(p);
    let mut pags: Vec<(u64, u64, Vec<String>)> = Vec::new();
    for ap in aparicoes(p, linha) {
        let l = &p.legendas.itens[ap.i];
        let texto = match l.locutor.as_deref().map(|x| x.trim_matches(|c| espaco(c) || c == '\n')).filter(|x| !x.is_empty()) {
            Some(loc) => format!("{loc}: {}", l.texto),
            None => l.texto.clone(),
        };
        // as páginas são da legenda inteira: cada trecho mostra só as da fala que toca nele (corte no meio da fala
        // e clipe dividido não recomeçam o texto)
        for (x, y, g) in paginas(quebrar(&texto, n), p.legendas.linhas_max, ap.xa, ap.xb) {
            if let Some((a, b)) = ap.visivel(x, y) {
                pags.push((a, b, g));
            }
        }
    }
    // varredura: pelos pontos onde alguma página começa ou acaba
    let mut ordem: Vec<usize> = (0..pags.len()).collect();
    ordem.sort_by_key(|&k| (pags[k].0, k));
    let mut pontos: Vec<u64> = pags.iter().flat_map(|x| [x.0, x.1]).collect();
    pontos.sort_unstable();
    pontos.dedup();
    let (mut ativas, mut prox): (Vec<usize>, usize) = (Vec::new(), 0);
    let mut v: Vec<Exibida> = Vec::new();
    for w in pontos.windows(2) {
        let (x, y) = (w[0], w[1]);
        ativas.retain(|&k| pags[k].1 > x);
        while prox < ordem.len() && pags[ordem[prox]].0 <= x {
            ativas.push(ordem[prox]);
            prox += 1;
        }
        if ativas.is_empty() {
            continue;
        }
        let linhas: Vec<String> = ativas.iter().flat_map(|&k| pags[k].2.iter().cloned()).collect();
        // o mesmo texto seguido (a legenda que atravessa um corte) é uma só
        match v.last_mut() {
            Some(e) if e.ate_q == x && e.linhas == linhas => e.ate_q = y,
            _ => v.push(Exibida { de_q: x, ate_q: y, linhas }),
        }
    }
    v
}

/// Legendas queimadas nos slots [de_q, de_q + n_q) do bloco: um `ItemCamada` por `Exibida` visível (`Desenho::Efeito`
/// com um `EfTexto` no estilo e lugar da trilha), em px do `canvas`. Com `Queimar` ou `Ambos` (na prévia e no quadro
/// final, a escolha gravada em `saida.exportacao`).
pub fn queimadas(p: &ProjetoEdicao, linha: &LinhaResolvida, de_q: u64, n_q: u32, canvas: TamanhoPx, alvo: &Alvo) -> Result<Vec<ItemCamada>> {
    if !matches!(modo(p, alvo), ModoLegendas::Queimar | ModoLegendas::Ambos) || p.legendas.itens.is_empty() {
        return Ok(Vec::new());
    }
    if matches!(alvo, Alvo::Final { .. }) {
        sem_silencio(p, linha)?;
    }
    let ate_q = de_q + n_q as u64;
    let fps = linha.fps;
    let mut itens = Vec::new();
    for e in exibidas(p, linha) {
        let (a, b) = (e.de_q.max(de_q), e.ate_q.min(ate_q));
        if a >= b {
            continue;
        }
        let texto = EfTexto {
            // id fixo: o mesmo texto dá o mesmo sprite em qualquer bloco
            base: EfBase { id: "legenda".into(), ligado: true, nome: String::new(), ativo: None },
            conteudo: e.linhas.join("\n"),
            estilo: p.legendas.estilo.clone(),
            lugar: p.legendas.lugar,
            largura_max: LARGURA,
        };
        let quadros = (a..b)
            .map(|q| {
                let t = tempo::instante_do_slot(q, fps);
                QuadroItem { quadro: QuadroCamada { q, t, pts: None, a: Afim::IDENTIDADE, canvas, rastreios: BTreeMap::new(), indice: None, programa_s: t }, transformacao: None }
            })
            .collect();
        itens.push(ItemCamada { desenho: Desenho::Efeito { efeito: Efeito::Texto(texto) }, de: (a - de_q) as u32, quadros });
    }
    Ok(itens)
}

/// Instante do slot em milissegundos inteiros: round(q·1000·den/num).
fn ms(q: u64, fps: Racional) -> u128 {
    let (num, den) = (fps.num.max(1) as u128, fps.den.max(1) as u128);
    (2 * q as u128 * 1000 * den + num) / (2 * num)
}

fn hms(ms: u128, sep: char) -> String {
    format!("{:02}:{:02}:{:02}{sep}{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// SubRip: um bloco por `Exibida`, numerado de 1, tempos do programa em ms.
pub fn srt(v: &[Exibida], fps: Racional) -> String {
    let mut s = String::new();
    for (i, e) in v.iter().enumerate() {
        s.push_str(&format!("{}\n{} --> {}\n{}\n\n", i + 1, hms(ms(e.de_q, fps), ','), hms(ms(e.ate_q, fps), ','), e.linhas.join("\n")));
    }
    s
}

/// WebVTT: os mesmos blocos; `&`, `<` e `>` escapados (o texto do perito é dado, nunca marcação).
pub fn vtt(v: &[Exibida], fps: Racional) -> String {
    let esc = |l: &String| l.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let mut s = String::from("WEBVTT\n\n");
    for e in v {
        let linhas: Vec<String> = e.linhas.iter().map(esc).collect();
        s.push_str(&format!("{} --> {}\n{}\n\n", hms(ms(e.de_q, fps), '.'), hms(ms(e.ate_q, fps), '.'), linhas.join("\n")));
    }
    s
}

/// `.srt` e `.vtt` da exportação (`Plano.legendas`; o executor grava e a fila publica com o nome do vídeo), em tempo
/// do programa. Só na exportação final com `Arquivo` ou `Ambos`; nenhuma legenda no programa = nenhum arquivo.
pub fn arquivos(p: &ProjetoEdicao, linha: &LinhaResolvida, ctx: &Contexto, alvo: &Alvo) -> Result<Vec<ArquivoTexto>> {
    let _ = ctx;
    let Alvo::Final { exportacao } = alvo else { return Ok(Vec::new()) };
    if !matches!(exportacao.legendas, ModoLegendas::Arquivo | ModoLegendas::Ambos) {
        return Ok(Vec::new());
    }
    sem_silencio(p, linha)?;
    let v = exibidas(p, linha);
    if v.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![ArquivoTexto { nome: SRT.into(), conteudo: srt(&v, linha.fps) }, ArquivoTexto { nome: VTT.into(), conteudo: vtt(&v, linha.fps) }])
}

#[cfg(test)]
mod testes {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    use serde_json::{json, Value};

    use super::*;
    use crate::edicao::camada::texto::{bitmap_sprite, PedidoSprite};
    use crate::edicao::ffmpeg::exec::{self, Comando};
    use crate::edicao::ffmpeg::sonda::Capacidades;
    use crate::edicao::modelo::{BlocoTexto, PapelTexto};
    use crate::edicao::motor::{compilar, compilar_video, FonteInfo};
    use crate::edicao::quadros::apoio::{self, SemRastreios};
    use crate::edicao::testes::aceite::{midia, projeto, trecho, Mundo};
    use crate::edicao::testes::executor::{self, Opcoes};
    use crate::edicao::testes::fontes::{self, Fonte as FonteTeste};
    use crate::edicao::testes::medir::{self, QuadroRgb};

    const VERDE: &str = "#00ff00";

    fn estilo(fonte: &str) -> Value {
        // texto da cor da caixa: a legenda inteira é um retângulo verde (mede-se presença e lugar, não glifo)
        json!({ "fonte": fonte, "negrito": false, "italico": false, "tamanho": 0.052, "cor": VERDE, "alinhamento": "centro",
                "caixa": { "cor": format!("{VERDE}ff"), "margem": 0.013, "raio": 0.0 }, "contorno": null })
    }

    fn trilha(itens: Vec<Value>, linhas_max: u32, fonte: &str) -> Value {
        json!({ "legendas": { "estilo": estilo(fonte), "lugar": { "x": 0.5, "y": 0.94, "h": "centro", "v": "base" }, "linhas_max": linhas_max,
                              "origem": null, "itens": itens } })
    }

    fn leg(id: &str, texto: &str, r: Value) -> Value {
        json!({ "id": id, "texto": texto, "locutor": null, "origem": "perito", "conferida": true, "locutor_auto": false, "ref": r })
    }

    fn de_fonte(id: &str, midia: &str, de: f64, ate: f64, texto: &str) -> Value {
        leg(id, texto, json!({ "tipo": "fonte", "midia": midia, "de": de, "ate": ate }))
    }

    fn de_programa(id: &str, de: f64, ate: f64, texto: &str) -> Value {
        leg(id, texto, json!({ "tipo": "programa", "de": de, "ate": ate }))
    }

    fn com(mut v: Value, k: &str, x: Value) -> Value {
        v[k] = x;
        v
    }

    fn alvo(modo: &str) -> Alvo {
        serde_json::from_value(json!({ "tipo": "final", "exportacao": { "predefinicao": "juri", "codificador": "cpu", "legendas": modo, "pacote": null,
                                                                         "pdf": "a3", "revisado_por": "", "personalizada": null } }))
        .unwrap()
    }

    fn titulo(id: &str, dur: f64) -> Value {
        json!({ "tipo": "titulo", "id": id, "rotulo": "", "duracao_s": dur, "titulo": { "modelo": "cartela", "fundo": { "tipo": "cor", "cor": "#000000" }, "blocos": [] } })
    }

    /// Fonte sintética: 450 quadros a 30 qps, base 1/15360, começando em 0.
    fn sintetica() -> FonteInfo {
        apoio::fonte(apoio::tabela((1, 15360), apoio::pts_regulares(450, (1, 30), (1, 15360), 0)), (320, 180))
    }

    struct Sem {
        fontes: HashMap<String, FonteInfo>,
        cap: Capacidades,
    }

    impl Sem {
        fn novo() -> Self {
            Sem { fontes: HashMap::from([("m1".to_string(), sintetica())]), cap: apoio::cap_falsa() }
        }
        fn ctx(&self) -> Contexto<'_> {
            Contexto { fontes: &self.fontes, cap: &self.cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] }
        }
        fn linha(&self, p: &ProjetoEdicao) -> LinhaResolvida {
            tempo::resolver(p, &self.ctx())
        }
    }

    // ------------------------------------------------------------------ sem ffmpeg

    /// Os casos da fixture `legendas-exibidas.json` (o espelho `modelo/legendas.ts` confere os mesmos bits).
    fn casos() -> Vec<(&'static str, ProjetoEdicao)> {
        let s = sintetica();
        let t = &s.quadros;
        let m = || vec![midia("m1", "video", json!([]))];
        let mut v = Vec::new();
        v.push((
            "fonte a 0,5× com corte no meio; a do pedaço cortado e a de outra mídia não aparecem",
            projeto(320, 180, (30, 1), m(), vec![trecho("c1", "m1", t, 30, 60, 0.5, json!([])), trecho("c2", "m1", t, 75, 120, 0.5, json!([]))],
                trilha(vec![de_fonte("l1", "m1", t.tempo(45), t.tempo(84), "Para, polícia!"), de_fonte("l2", "m1", t.tempo(63), t.tempo(72), "no corte"),
                            de_fonte("l3", "m2", 1.0, 2.0, "outra mídia")], 2, "sans")),
        ));
        v.push((
            "programa e fonte a 29,97 sobre título e trecho",
            projeto(320, 180, (30000, 1001), m(), vec![titulo("c0", 1.0), trecho("c1", "m1", t, 15, 165, 1.0, json!([]))],
                trilha(vec![de_programa("l1", 0.5, 1.5, "Um"), de_fonte("l2", "m1", 2.0, 3.0, "Dois")], 2, "sans")),
        ));
        let longo = "O veículo prata entrou na contramão e o motorista desceu correndo em direção à porta da farmácia, gritando que ia voltar.";
        v.push((
            "páginas de uma linha, locutor, quebra do perito, palavra maior que a linha e espaço sem quebra junto",
            projeto(320, 180, (30, 1), m(), vec![trecho("c1", "m1", t, 0, 300, 1.0, json!([]))],
                trilha(vec![com(de_programa("l1", 0.0, 4.0, longo), "locutor", json!("Policial 2")),
                            de_programa("l2", 5.0, 6.0, "Primeira\nsegunda\u{a0}linha\n\n"),
                            de_programa("l3", 6.0, 9.5, &"A".repeat(130))], 1, "sans")),
        ));
        v.push((
            "legendas ao mesmo tempo empilham; a que começou antes fica em cima",
            projeto(320, 180, (30, 1), m(), vec![trecho("c1", "m1", t, 0, 120, 1.0, json!([]))],
                trilha(vec![de_programa("l2", 1.0, 3.0, "B"), de_programa("l1", 0.0, 2.0, "A"), de_fonte("l3", "m1", 0.5, 1.5, "C")], 2, "sans")),
        ));
        let mut silencio = trecho("c1", "m1", t, 30, 120, 1.0, json!([]));
        silencio["audio"]["silencios"] = json!([{ "id": "s1", "de": 1.6, "ate": 1.8, "modo": "tom", "motivo": "nome" }]);
        v.push((
            "silêncio: a que cruza e a de programa por cima dele; a vizinha não",
            projeto(320, 180, (30, 1), m(), vec![silencio],
                trilha(vec![de_fonte("l1", "m1", 1.5, 1.7, "nome"), de_fonte("l2", "m1", 1.8, 2.0, "depois"), de_programa("l3", 0.65, 0.7, "por cima"),
                            de_programa("l4", 0.0, 0.2, "antes")], 2, "sans")),
        ));
        v.push((
            "repetida em dois trechos, a 1× e a 2×",
            projeto(320, 180, (30, 1), m(), vec![trecho("c1", "m1", t, 0, 60, 1.0, json!([])), trecho("c2", "m1", t, 30, 90, 2.0, json!([]))],
                trilha(vec![de_fonte("l1", "m1", 1.2, 1.6, "de novo")], 2, "sans")),
        ));
        let mut fundir = trecho("c2", "m1", t, 60, 120, 1.0, json!([]));
        fundir["transicao_entrada"] = json!({ "tipo": "fundir", "duracao_s": 0.5, "audio": "acompanhar" });
        v.push((
            "transição com sobreposição: o som de quem entra já toca nela",
            projeto(320, 180, (30, 1), m(), vec![trecho("c1", "m1", t, 0, 60, 1.0, json!([])), fundir],
                trilha(vec![de_fonte("l1", "m1", 1.9, 2.1, "junção")], 2, "sans")),
        ));
        let mut desligado = trecho("c1", "m1", t, 0, 60, 1.0, json!([]));
        desligado["desligado"] = json!(true);
        v.push((
            "mono em 4:3; clipe desligado não leva a legenda",
            projeto(640, 480, (25, 1), m(), vec![desligado, trecho("c2", "m1", t, 30, 90, 1.0, json!([]))],
                trilha(vec![de_fonte("l1", "m1", 0.5, 1.5, "Um texto um pouco maior que a linha do mono em quatro por três, para quebrar")], 3, "mono")),
        ));
        v.push((
            "vazia, fora do programa e antes do zero",
            projeto(320, 180, (30, 1), m(), vec![trecho("c1", "m1", t, 0, 60, 1.0, json!([]))],
                trilha(vec![de_programa("l1", 0.0, 1.0, "  \n "), de_programa("l2", 10.0, 20.0, "fora"), de_programa("l3", -1.0, 0.5, "antes"),
                            de_programa("l4", 1.0, 1.0, "zero")], 2, "sans")),
        ));
        // c1 mostra 1,0..2,0 s; o 2,5..3,0 não toca (não age); o de programa em 1,5 s está sobre c2
        let mut c1 = trecho("c1", "m1", t, 30, 60, 1.0, json!([]));
        c1["audio"]["silencios"] = json!([{ "id": "s1", "de": 1.6, "ate": 1.8, "modo": "tom", "motivo": "" },
                                          { "id": "s2", "de": 2.5, "ate": 3.0, "modo": "mudo", "motivo": "" }]);
        v.push((
            "silêncio: sobra de menos de meio slot barra; fora da janela do trecho não age",
            projeto(320, 180, (30, 1), m(), vec![c1, trecho("c2", "m1", t, 90, 120, 1.0, json!([]))],
                trilha(vec![de_fonte("l1", "m1", 1.5, 1.61, "um"), de_fonte("l2", "m1", 2.6, 2.8, "dois"), de_programa("l3", 1.5, 1.7, "três")], 2, "sans")),
        ));
        let mut c1 = trecho("c1", "m1", t, 0, 60, 1.0, json!([]));
        c1["audio"]["silencios"] = json!([{ "id": "s1", "de": 1.7, "ate": 2.5, "modo": "tom", "motivo": "" }]);
        let mut c2 = trecho("c2", "m1", t, 60, 120, 1.0, json!([]));
        c2["transicao_entrada"] = json!({ "tipo": "fundir", "duracao_s": 0.5, "audio": "acompanhar" });
        v.push((
            "silêncio no fim de quem sai: a de fonte de quem entra não; a de programa por cima, sim",
            projeto(320, 180, (30, 1), m(), vec![c1, c2],
                trilha(vec![de_fonte("l1", "m1", 2.0, 2.3, "entra"), de_programa("l2", 1.75, 1.8, "por cima")], 2, "sans")),
        ));
        // c1 e c2 seguidos na fonte (clipe dividido em 2,0 s); entre c2 e c3 sai o pedaço de 4,0 a 5,0 s
        v.push((
            "páginas de fonte seguem a fala: clipe dividido e corte com pedaço tirado não recomeçam o texto",
            projeto(320, 180, (30, 1), m(), vec![trecho("c1", "m1", t, 0, 60, 1.0, json!([])), trecho("c2", "m1", t, 60, 120, 1.0, json!([])),
                                                 trecho("c3", "m1", t, 150, 210, 1.0, json!([]))],
                trilha(vec![de_fonte("l1", "m1", 1.0, 3.0, "aaaa\nbbbb"), de_fonte("l2", "m1", 3.5, 6.0, "cccc\ndddd")], 1, "sans")),
        ));
        v.push((
            "legenda de fonte sem começo nem fim: a conta das páginas tem teto",
            projeto(320, 180, (30, 1), m(), vec![trecho("c1", "m1", t, 30, 90, 1.0, json!([]))],
                trilha(vec![de_fonte("l1", "m1", -1e300, 1e300, "aaaa\nbbbb")], 1, "sans")),
        ));
        // P3.4: rampa 1× → 0,25× → 1× de 1 s a 4 s; a fala em 2,0..2,5 s (a parte lenta) ocupa mais slots
        let mut rampa = trecho("c1", "m1", t, 30, 120, 1.0, json!([]));
        rampa["velocidade"] = json!({ "tipo": "rampa", "pontos": [{ "t": 1.5, "v": 1.0 }, { "t": 2.5, "v": 0.25 }, { "t": 3.5, "v": 1.0 }] });
        rampa["audio"]["silencios"] = json!([{ "id": "s1", "de": 3.0, "ate": 3.2, "modo": "tom", "motivo": "" }]);
        v.push((
            "rampa: legenda de fonte pela integral; silêncio na parte lenta",
            projeto(320, 180, (30, 1), m(), vec![rampa],
                trilha(vec![de_fonte("l1", "m1", 1.2, 1.4, "antes"), de_fonte("l2", "m1", 2.0, 2.5, "lenta"), de_fonte("l3", "m1", 3.1, 3.3, "calada")], 2, "sans")),
        ));
        v
    }

    /// Programa (s) do instante de fonte `s` no trecho em rampa do caso 13, a partir do início `ini`.
    fn no_trecho_s(ini: f64, s: f64) -> f64 {
        let v: Velocidade = serde_json::from_value(json!({ "tipo": "rampa", "pontos": [{ "t": 1.5, "v": 1.0 }, { "t": 2.5, "v": 0.25 }, { "t": 3.5, "v": 1.0 }] })).unwrap();
        tempo::programa_da_fonte(&v, ini, s).unwrap()
    }

    #[test]
    fn fixture_legendas_exibidas() {
        let sem = Sem::novo();
        let lista: Vec<Value> = casos()
            .into_iter()
            .map(|(nome, p)| {
                let linha = sem.linha(&p);
                json!({ "nome": nome, "entrada": { "projeto": p, "linha": linha },
                        "saida": { "exibidas": exibidas(&p, &linha), "sob_silencio": sob_silencio(&p, &linha), "por_linha": por_linha(&p) } })
            })
            .collect();
        // o que os casos afirmam, além de serem o contrato do espelho
        let ex = |i: usize| serde_json::from_value::<Vec<Exibida>>(lista[i]["saida"]["exibidas"].clone()).unwrap();
        let faixas = |i: usize| ex(i).iter().map(|e| (e.de_q, e.ate_q)).collect::<Vec<_>>();
        // 0,5×: quadros 45..59 em c1 (slots 30..59) e 75..83 em c2 (slots 60..77), sem buraco no corte
        assert_eq!(faixas(0), vec![(30, 78)]);
        assert_eq!(ex(0)[0].linhas, vec!["Para, polícia!".to_string()]);
        // 29,97: round(0,5·29,97) = 15; c1 começa em 30 com a fonte em 0,5 s: (2,0 − 0,5)·29,97 → 45
        assert_eq!(faixas(1), vec![(15, 45), (75, 105)]);
        // uma linha por página, todas dentro de [0, 120) e nenhuma vazia
        let e2 = ex(2);
        assert!(e2.iter().all(|e| e.linhas.len() == 1 && e.ate_q > e.de_q), "{e2:?}");
        assert!(e2[0].linhas[0].starts_with("Policial 2: O veículo"));
        assert_eq!(e2.iter().filter(|e| e.ate_q <= 120).map(|e| e.ate_q - e.de_q).sum::<u64>(), 120);
        assert_eq!(faixas(3), vec![(0, 15), (15, 30), (30, 45), (45, 60), (60, 90)]);
        assert_eq!(ex(3)[1].linhas, vec!["A", "C"]);
        assert_eq!(ex(3)[2].linhas, vec!["A", "C", "B"]);
        assert_eq!(lista[4]["saida"]["sob_silencio"], json!(["l1", "l3"]));
        assert_eq!(faixas(5), vec![(36, 48), (63, 69)]);
        assert_eq!(faixas(6), vec![(45, 48), (57, 60)]);
        assert_eq!(faixas(8), vec![(0, 15)]);
        // l1 acaba em 1,61 s: no slot ela sai em 18 junto com a entrada do silêncio, mas a fala de 1,60 a 1,61 é calada
        assert_eq!(faixas(9)[0], (15, 18));
        assert_eq!(lista[9]["saida"]["sob_silencio"], json!(["l1"]));
        assert_eq!(lista[10]["saida"]["sob_silencio"], json!(["l2"]));
        // cada página uma vez só, na metade da fala que é dela (nunca a, b, a, b)
        let e11: Vec<(u64, u64, String)> = ex(11).into_iter().map(|e| (e.de_q, e.ate_q, e.linhas.join("|"))).collect();
        let esperado = [(30, 60, "aaaa"), (60, 90, "bbbb"), (105, 120, "cccc"), (120, 150, "dddd")];
        assert_eq!(e11, esperado.map(|(a, b, s)| (a, b, s.to_string())).to_vec());
        // 2^33 slots no eixo do trecho, metade para cada página: a segunda começa no slot 0 dele
        assert_eq!((faixas(12), ex(12)[0].linhas.clone()), (vec![(0, 60)], vec!["bbbb".to_string()]));
        // rampa: 1× até 1,5 s (l1 em 6..12), a fala lenta dura mais do que 15 slots; a do silêncio é barrada
        assert_eq!(faixas(13)[0], (6, 12));
        let lenta = ex(13).into_iter().find(|e| e.linhas == ["lenta"]).unwrap();
        let (a, b) = (no_trecho_s(1.0, 2.0) * 30.0, no_trecho_s(1.0, 2.5) * 30.0);
        assert_eq!((lenta.de_q, lenta.ate_q), ((a + 0.5).floor() as u64, (b + 0.5).floor() as u64));
        assert!(lenta.ate_q - lenta.de_q > 25, "{lenta:?}");
        assert_eq!(lista[13]["saida"]["sob_silencio"], json!(["l3"]));
        crate::edicao::curva::fixtures::conferir("legendas-exibidas.json", Value::Array(lista));
    }

    #[test]
    fn recusa_saiu_e_os_arquivos_so_com_arquivo() {
        let sem = Sem::novo();
        let (_, p) = casos().swap_remove(1);
        let linha = sem.linha(&p);
        for modo in ["nenhuma", "queimar", "arquivo", "ambos"] {
            let Alvo::Final { exportacao } = alvo(modo) else { unreachable!() };
            assert_eq!(recusa(&p, &exportacao), None);
            let a = arquivos(&p, &linha, &sem.ctx(), &alvo(modo)).unwrap();
            let nomes: Vec<&str> = a.iter().map(|x| x.nome.as_str()).collect();
            assert_eq!(nomes, if matches!(modo, "arquivo" | "ambos") { vec![SRT, VTT] } else { vec![] }, "{modo}");
        }
        // prévia e quadro final nunca levam arquivo
        assert!(arquivos(&p, &linha, &sem.ctx(), &Alvo::Previa { altura: 90, de_q: 0, ate_q: 10 }).unwrap().is_empty());
        let srt = &arquivos(&p, &linha, &sem.ctx(), &alvo("ambos")).unwrap()[0].conteudo;
        // 15·1001/30000 = 0,5005 s → 501 ms; 45 → 1,5015 s
        assert!(srt.starts_with("1\n00:00:00,501 --> 00:00:01,502\nUm\n\n2\n00:00:02,503 --> 00:00:03,504\nDois\n\n"), "{srt}");
    }

    #[test]
    fn queimadas_por_bloco_e_modo() {
        let sem = Sem::novo();
        let (_, mut p) = casos().swap_remove(1);
        let linha = sem.linha(&p);
        let t = TamanhoPx { w: 320, h: 180 };
        // bloco [20, 80): a primeira começa antes (corta em 20) e a segunda cai a partir de 75
        let q = queimadas(&p, &linha, 20, 60, t, &alvo("queimar")).unwrap();
        let resumo: Vec<(u32, usize, String)> = q
            .iter()
            .map(|i| match &i.desenho {
                Desenho::Efeito { efeito: Efeito::Texto(x) } => (i.de, i.quadros.len(), x.conteudo.clone()),
                d => panic!("{d:?}"),
            })
            .collect();
        assert_eq!(resumo, vec![(0, 25, "Um".to_string()), (55, 5, "Dois".to_string())]);
        assert_eq!(q[0].quadros.iter().map(|x| x.quadro.q).collect::<Vec<_>>(), (20..45).collect::<Vec<_>>());
        assert!(q.iter().all(|i| i.quadros.iter().all(|x| x.quadro.canvas == t && x.transformacao.is_none())));
        for modo in ["nenhuma", "arquivo"] {
            assert!(queimadas(&p, &linha, 0, 200, t, &alvo(modo)).unwrap().is_empty(), "{modo}");
        }
        assert_eq!(queimadas(&p, &linha, 0, 200, t, &alvo("ambos")).unwrap().len(), 2);
        // prévia e quadro final: o que a última exportação escolheu
        let previa = Alvo::Previa { altura: 90, de_q: 0, ate_q: 200 };
        assert!(queimadas(&p, &linha, 0, 200, TamanhoPx { w: 160, h: 90 }, &previa).unwrap().is_empty());
        p.saida.exportacao.legendas = ModoLegendas::Queimar;
        assert_eq!(queimadas(&p, &linha, 0, 200, TamanhoPx { w: 160, h: 90 }, &previa).unwrap().len(), 2);
    }

    /// Legenda `ia` não conferida e legenda sob silêncio barram a exportação: a validação acusa e `compilar` para nela;
    /// o motor sozinho (`compilar_video`) também recusa. Com "Não" nas legendas, nada barra.
    #[test]
    fn ia_nao_conferida_e_sob_silencio_barram() {
        let sem = Sem::novo();
        let ctx = sem.ctx();
        let (_, base) = casos().swap_remove(4);
        let mut ia = base.clone();
        ia.legendas.itens.retain(|l| l.id == "l2" || l.id == "l4");
        ia.legendas.itens[0].origem = OrigemLegenda::Ia;
        ia.legendas.itens[0].conferida = false;
        for modo in ["queimar", "arquivo", "ambos"] {
            let a = alvo(modo);
            let e = compilar(&ia, &ctx, &a).expect_err(modo).to_string();
            assert!(e.contains("legenda_nao_conferida"), "{modo}: {e}");
            let linha = tempo::resolver(&ia, &ctx);
            assert!(compilar_video(&ia, &linha, &ctx, &a).expect_err(modo).to_string().contains("legenda_nao_conferida: l2"));
            let e = compilar(&base, &ctx, &a).expect_err(modo).to_string();
            assert!(e.contains("legenda_sob_silencio"), "{modo}: {e}");
        }
        // conferida: sai; sem a que cruza o silêncio, sai
        let mut ok = ia.clone();
        ok.legendas.itens[0].conferida = true;
        assert!(compilar(&ok, &ctx, &alvo("ambos")).is_ok());
        let mut fora = base.clone();
        fora.legendas.itens.retain(|l| l.id == "l2" || l.id == "l4");
        let plano = compilar(&fora, &ctx, &alvo("ambos")).expect("sem legenda sob silêncio");
        assert_eq!(plano.legendas.len(), 2);
        // legenda inválida (fim = início): o motor sozinho também recusa
        let mut invalida = fora.clone();
        invalida.legendas.itens[0].alvo = RefLegenda::Programa { de: 1.0, ate: 1.0 };
        let linha = tempo::resolver(&invalida, &ctx);
        assert!(compilar_video(&invalida, &linha, &ctx, &alvo("arquivo")).unwrap_err().to_string().contains("legenda: l2"));
        // o motor também trava a de silêncio (queimada e em arquivo)
        let linha = tempo::resolver(&base, &ctx);
        assert!(compilar_video(&base, &linha, &ctx, &alvo("queimar")).unwrap_err().to_string().contains("legenda_sob_silencio: l1"));
        assert!(arquivos(&base, &linha, &ctx, &alvo("arquivo")).unwrap_err().to_string().contains("legenda_sob_silencio: l1"));
        // "Não": nada barra
        for p in [&ia, &base] {
            assert!(compilar(p, &ctx, &alvo("nenhuma")).is_ok());
        }
    }

    /// A quebra estimada (`AVANCO`) cabe na largura: o sprite do Typst sai com as linhas pedidas, sem quebra a mais.
    #[test]
    fn quebra_cabe_no_sprite() {
        let textos = [
            "A testemunha disse que viu o carro passar em alta velocidade pela avenida principal, perto da esquina da farmácia.",
            "Ação, exceção, coração, informação: ÁÉÍÓÚ ÂÊÔ ÃÕ Ç — 0123456789 e mais palavras comuns para encher a linha toda.",
        ];
        for (fonte, (w, h)) in [("sans", (1920, 1080)), ("serifa", (1280, 720)), ("mono", (640, 480)), ("sans", (720, 576))] {
            let mut p = projeto(w, h, (30, 1), vec![], vec![], trilha(vec![], 2, fonte));
            p.legendas.estilo.caixa = None;
            let n = por_linha(&p);
            let altura = |linhas: &[String]| {
                let b = BlocoTexto {
                    id: "legenda".into(),
                    papel: PapelTexto::Corpo,
                    conteudo: linhas.join("\n"),
                    estilo: p.legendas.estilo.clone(),
                    lugar: p.legendas.lugar,
                    largura_max: LARGURA,
                };
                // o mesmo pedido de `formas::primitivas` para um `EfTexto`
                let pedido = PedidoSprite { blocos: vec![b], largura_max: LARGURA * w as f64 / h as f64, altura_canvas: h, fundo: true };
                bitmap_sprite(&pedido).unwrap().0.h
            };
            let (h1, h2) = (altura(&["a".into()]), altura(&["a".into(), "b".into()]));
            for texto in textos {
                let linhas = quebrar(texto, n);
                assert!(linhas.len() >= 2 && linhas.iter().all(|l| l.chars().count() <= n), "{fonte}: {linhas:?}");
                // linha a mais (o Typst quebrando de novo) somaria um passo inteiro; o arredondamento, menos de meio
                let esperado = h1 + (linhas.len() as u32 - 1) * (h2 - h1);
                assert!(2 * altura(&linhas).abs_diff(esperado) < h2 - h1, "{fonte} {w}×{h}: {} linhas pedidas, sprite de {} px (uma linha: {h1}, duas: {h2})", linhas.len(), altura(&linhas));
            }
        }
    }

    // ------------------------------------------------------------------ com ffmpeg

    fn pasta(nome: &str) -> PathBuf {
        fontes::pasta_temporaria(nome)
    }

    fn eh_verde(p: &[u8]) -> bool {
        p[1] > 170 && p[0] < 100 && p[2] < 100
    }

    /// Caixa (x0, y0, x1, y1) dos pixels verdes e quantos são.
    fn verde(q: &QuadroRgb) -> Option<([u32; 4], u32)> {
        let (mut c, mut n) = ([u32::MAX, u32::MAX, 0, 0], 0);
        for (i, p) in q.px.chunks_exact(3).enumerate() {
            if eh_verde(p) {
                let (x, y) = (i as u32 % q.w, i as u32 / q.w);
                c = [c[0].min(x), c[1].min(y), c[2].max(x + 1), c[3].max(y + 1)];
                n += 1;
            }
        }
        (n > 0).then_some((c, n))
    }

    /// Renderiza sem perdas; devolve, por slot, a caixa verde (se há legenda) e o número do quadro de origem.
    fn renderizar(cap: &Arc<Capacidades>, plano: &crate::edicao::motor::plano::Plano, nome: &str) -> Vec<(Option<[u32; 4]>, u32)> {
        let dir = pasta(nome);
        let r = executor::rodar(cap, plano, &dir, &Opcoes { sem_perdas: true, ..Opcoes::default() }).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
        let mut v = Vec::new();
        for b in &r.blocos {
            let qs = medir::quadros(cap, b).unwrap();
            let ns = medir::quadros_de_origem(cap, b).unwrap();
            assert_eq!(qs.len(), ns.len());
            // menos de 40 px verdes = ruído de croma, não legenda
            v.extend(qs.iter().zip(ns).map(|(q, n)| (verde(q).filter(|x| x.1 >= 40).map(|x| x.0), n)));
        }
        assert_eq!(v.len() as u64, plano.total_q, "{nome}: quadros × slots");
        let _ = std::fs::remove_dir_all(&dir);
        v
    }

    /// A caixa da legenda está no lugar da trilha: centro em x = w/2 e base em 0,94·h (até 2 px).
    fn no_lugar(c: [u32; 4], w: u32, h: u32) -> bool {
        let cx = (c[0] + c[2]) as f64 / 2.0;
        (cx - w as f64 / 2.0).abs() <= 2.0 && (c[3] as f64 - 0.94 * h as f64).abs() <= 2.0
    }

    /// Legenda de fonte acompanha o trecho a 0,5× e o corte no meio: aparece em todo slot que mostra um quadro do
    /// intervalo dela (45..83) e só neles; a do pedaço cortado (63..71) não aparece. Mede quadro e pixel.
    #[test]
    fn fonte_acompanha_trecho_a_meia_velocidade_e_corte() {
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, FonteTeste::Mp4);
        let t = &m.info.quadros;
        let mundo = Mundo::novo(&cap).com("m1", &m.info);
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))],
            vec![trecho("c1", "m1", t, 30, 60, 0.5, json!([])), trecho("c2", "m1", t, 75, 120, 0.5, json!([]))],
            trilha(vec![de_fonte("l1", "m1", t.tempo(45), t.tempo(84), "Para"), de_fonte("l2", "m1", t.tempo(63), t.tempo(72), "Corte")], 2, "sans"));
        let plano = mundo.compilar(&p, &alvo("queimar")).unwrap();
        let slots = renderizar(&cap, &plano, "p16-meia");
        let (a, b) = (m.numeros[45], m.numeros[84]);
        let mut vistos = 0;
        for (q, (caixa, n)) in slots.iter().enumerate() {
            assert_ne!(*n, medir::NAO_IDENTIFICADO, "slot {q}: quadro ilegível ({})", cap.versao);
            assert_eq!(caixa.is_some(), (a..b).contains(n), "slot {q}, quadro {n}: legenda {:?} ({})", caixa, cap.versao);
            if let Some(c) = caixa {
                assert!(no_lugar(*c, 320, 180), "slot {q}: caixa {c:?}");
                vistos += 1;
            }
        }
        // 15 quadros a 0,5× em c1 e 9 em c2
        assert_eq!(vistos, 48, "{}", cap.versao);
        // uma linha em todo slot (a altura do sprite de "Para"): a do pedaço cortado não se empilha sobre a outra
        let b = BlocoTexto { id: "legenda".into(), papel: PapelTexto::Corpo, conteudo: "Para".into(), estilo: p.legendas.estilo.clone(), lugar: p.legendas.lugar, largura_max: LARGURA };
        let uma = bitmap_sprite(&PedidoSprite { blocos: vec![b], largura_max: LARGURA * 320.0 / 180.0, altura_canvas: 180, fundo: true }).unwrap().0.h;
        let alturas: Vec<u32> = slots.iter().filter_map(|s| s.0).map(|c| c[3] - c[1]).collect();
        assert!(alturas.iter().all(|h| h.abs_diff(uma) <= 2), "{alturas:?}, uma linha: {uma} ({})", cap.versao);
    }

    /// O tempo de fonte é o absoluto do arquivo: fonte que não começa em zero (TS a 59,94), base de 1 ms (MKV),
    /// 23,976, pixel 10:11 e giro de 90°, cada uma na própria taxa. A legenda cai nos quadros 35..49 do trecho (slots
    /// 15..29), só neles, e no lugar da trilha (o lugar é da saída, não da fonte). Nas duas últimas o quadro reduzido
    /// não se identifica pelo conteúdo: mede-se o slot.
    #[test]
    fn fonte_fora_do_zero_base_grossa_e_giro() {
        let Some(cap) = fontes::cap() else { return };
        let fontes_ = [
            (FonteTeste::Ts5994, (60000, 1001), true),
            (FonteTeste::Mkv30, (30, 1), true),
            (FonteTeste::Mkv23976, (24000, 1001), true),
            (FonteTeste::PixelNaoQuadrado, (30, 1), false),
            (FonteTeste::Giro90, (30, 1), false),
        ];
        for (f, fps, identifica) in fontes_ {
            let m = apoio::medida(&cap, f);
            let t = &m.info.quadros;
            let mundo = Mundo::novo(&cap).com("m1", &m.info);
            let p = projeto(320, 180, fps, vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", t, 20, 80, 1.0, json!([]))],
                trilha(vec![de_fonte("l1", "m1", t.tempo(35), t.tempo(50), "Fala")], 2, "sans"));
            let plano = mundo.compilar(&p, &alvo("queimar")).unwrap_or_else(|e| panic!("{f:?}: {e}"));
            let slots = renderizar(&cap, &plano, "p16-fontes");
            assert_eq!(slots.len(), 60, "{f:?}");
            for (q, (caixa, n)) in slots.iter().enumerate() {
                assert_eq!(caixa.is_some(), (15..30).contains(&q), "{f:?} slot {q}: legenda {caixa:?} ({})", cap.versao);
                assert!(caixa.is_none_or(|c| no_lugar(c, 320, 180)), "{f:?} slot {q}: caixa {caixa:?}");
                if identifica {
                    assert_eq!(*n, m.numeros[20 + q], "{f:?} slot {q}: quadro ({})", cap.versao);
                }
            }
        }
    }

    /// Queimada no slot certo a 29,97: a de programa de 0,5 a 1,5 s nos slots 15..44 (por cima do título também) e a de
    /// fonte de 2,0 a 3,0 s nos slots 75..104; o `.srt` diz os mesmos slots.
    #[test]
    fn queimada_no_slot_certo() {
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, FonteTeste::Mp4);
        let t = &m.info.quadros;
        let mundo = Mundo::novo(&cap).com("m1", &m.info);
        let p = projeto(320, 180, (30000, 1001), vec![midia("m1", "video", json!([]))], vec![titulo("c0", 1.0), trecho("c1", "m1", t, 15, 165, 1.0, json!([]))],
            trilha(vec![de_programa("l1", 0.5, 1.5, "Um"), de_fonte("l2", "m1", t.tempo(60), t.tempo(90), "Dois")], 2, "sans"));
        let plano = mundo.compilar(&p, &alvo("ambos")).unwrap();
        let slots = renderizar(&cap, &plano, "p16-slot");
        let com_legenda: Vec<usize> = slots.iter().enumerate().filter(|(_, s)| s.0.is_some()).map(|(q, _)| q).collect();
        let esperado: Vec<usize> = (15..45).chain(75..105).collect();
        assert_eq!(com_legenda, esperado, "{}", cap.versao);
        assert!(slots.iter().filter_map(|s| s.0).all(|c| no_lugar(c, 320, 180)));
        // a de fonte cai nos quadros 60..89 do trecho
        let n: Vec<u32> = slots[75..105].iter().map(|s| s.1).collect();
        assert_eq!(n, (60..90).map(|j| m.numeros[j]).collect::<Vec<_>>());
        let linha = tempo::resolver(&p, &Contexto { fontes: &mundo.fontes, cap: &mundo.cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/x"), midias: &[] });
        let ex: Vec<(u64, u64)> = exibidas(&p, &linha).iter().map(|e| (e.de_q, e.ate_q)).collect();
        assert_eq!(ex, vec![(15, 45), (75, 105)]);
    }

    /// Hexadecimal do `-show_data` do ffprobe de volta a texto.
    fn dados(hex: &str) -> String {
        let mut b = Vec::new();
        for l in hex.lines() {
            let Some((_, resto)) = l.split_once(": ") else { continue };
            let h: String = resto.chars().take(39).filter(|c| !c.is_whitespace()).collect();
            b.extend((0..h.len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap()));
        }
        String::from_utf8(b).unwrap()
    }

    /// `.srt` e `.vtt` do plano (`compilar`), lidos pelo ffprobe do binário escolhido: codec, pts e duração em ms iguais
    /// aos slots das exibidas e o texto igual (acento, locutor, várias linhas; `&<>` escapados no VTT).
    #[test]
    fn srt_e_vtt_lidos_pelo_ffprobe() {
        let Some(cap) = fontes::cap() else { return };
        let sem = Sem::novo();
        let ctx = sem.ctx();
        let (_, mut p) = casos().swap_remove(1);
        p.legendas.itens.push(serde_json::from_value(com(de_programa("l3", 4.0, 6.0, "Ação & <reação> com uma frase longa que vai quebrar em duas linhas na tela"), "locutor", json!("Testemunha"))).unwrap());
        let plano = compilar(&p, &ctx, &alvo("ambos")).expect("compila");
        let ex = exibidas(&p, &tempo::resolver(&p, &ctx));
        assert!(ex.len() >= 3 && ex.iter().any(|e| e.linhas.len() == 2), "{ex:?}");
        let dir = pasta("p16-srt");
        for (arquivo, codec) in plano.legendas.iter().zip(["subrip", "webvtt"]) {
            std::fs::write(dir.join(&arquivo.nome), &arquivo.conteudo).unwrap();
            let a = ["-v", "error", "-of", "json", "-show_entries", "stream=codec_name:packet=pts,duration,data", "-show_data", &arquivo.nome];
            let cmd = Comando {
                nome: "ffprobe".into(),
                programa: cap.ffprobe.clone(),
                args: a.iter().map(|s| s.to_string()).collect(),
                pasta: dir.clone(),
                ambiente: Vec::new(),
                sem_ambiente: Vec::new(),
                stderr: dir.join("ffprobe.stderr"),
            };
            let (fim, saida) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).unwrap();
            exec::exigir_sucesso(&cmd, &fim).unwrap();
            let j: Value = serde_json::from_slice(&saida).unwrap();
            assert_eq!(j["streams"][0]["codec_name"], codec, "{}", cap.versao);
            let pacotes = j["packets"].as_array().unwrap();
            assert_eq!(pacotes.len(), ex.len(), "{codec}");
            for (k, e) in pacotes.iter().zip(&ex) {
                let (de, ate) = (ms(e.de_q, p.saida.fps) as i64, ms(e.ate_q, p.saida.fps) as i64);
                assert_eq!((k["pts"].as_i64(), k["duration"].as_i64()), (Some(de), Some(ate - de)), "{codec} ({})", cap.versao);
                let texto = e.linhas.join("\n");
                let texto = if codec == "webvtt" { texto.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") } else { texto };
                assert_eq!(dados(k["data"].as_str().unwrap()), texto, "{codec} ({})", cap.versao);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
