//! Áudio do programa inteiro numa passada (§5.9). Dono: P1.5 (herda do P0.6b; inclui o som das transições, O3).
//! Decisão O6: a passada de áudio e a de medição rodam SEMPRE com `-cpuflags 0`, antes das entradas.
//!
//! Fase 0: uma entrada por mídia (`-vn`, sem `-ss`) com `asplit`, e outra da mesma mídia quando o som teria de
//! esperar na memória mais de `TETO_ESPERA_S` (`distribuir`); por clipe `atrim` absoluto, `asetpts=PTS-T0/TB`,
//! `aresample=48000:async=1:first_pts=0` (silêncio inicial e buracos), velocidade, `apad` e `atrim=end_sample`;
//! principal emendada por `concat`, `afade` no `escurecer`; trilhas presas por `adelay` e somadas por `amix=normalize=0`
//! (`de_s`/`ate_s` do clipe de áudio contam do começo do som do arquivo).
//! Normalização, abaixar, silenciar, volume animado, limitador e transição com sobreposição: `nao_suportado` (Fase 1).

use std::collections::HashMap;
use std::fmt::Write as _;

use super::plano::{Entrada, Papel, PassoAudio, Previa, MARCA_GANHO};
use super::{Alvo, Contexto, FonteInfo};
use crate::edicao::ffmpeg::args;
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::modelo::{
    Animavel, AudioTransicao, ClipeBase, ClipePrincipal, ClipeSobreposto, LinhaResolvida, Normalizar, ProjetoEdicao, Racional,
    Tolerante, Transicao, Velocidade,
};
use crate::edicao::tempo::{self, TAXA_AUDIO};
use crate::error::{Result, SicroError};

/// Primeiros argumentos de toda linha de áudio (O6).
pub const CPUFLAGS: [&str; 2] = ["-cpuflags", "0"];

// CONTRATO: acréscimo (contratos (h) 7), com `tolerancia_buraco_ms`.
/// Buraco no som da fonte que o `aresample=async=1` deixa como está (ms; `min_hard_comp` padrão, 0,1 s): o som
/// depois dele adianta até isso (medido: 60 ms → 64 ms). Vai para `feito.audio` (contratos (h) 7).
pub const TOLERANCIA_BURACO_MS: f64 = 100.0;

/// Medição do `loudnorm` (passada 1): volume integrado (LUFS), pico real (dBTP), faixa (LU) e limiar (LUFS).
/// `integrado` None = `-inf` (silêncio): não normaliza.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Loudnorm {
    pub integrado: Option<f64>,
    pub pico: Option<f64>,
    pub faixa: f64,
    pub limiar: Option<f64>,
}

const SAIDA: &str = "audio.wav";
const HASH: &str = "audio.sha256";
/// Todo pedaço antes de juntar.
const FORMATO: &str = "aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo";
/// O PCM da saída sai do grafo já em s16: `[a]` e `[ah]` recebem os mesmos bytes.
const FORMATO_SAIDA: &str = "aformat=sample_fmts=s16:sample_rates=48000:channel_layouts=stereo";
/// Transição com som "cortar": `afade` de 10 ms de cada lado do corte.
const CORTE_AMOSTRAS: u64 = TAXA_AUDIO as u64 / 100;
/// Quadro do silêncio gerado: 100 ms.
const QUADRO_SILENCIO: u32 = TAXA_AUDIO / 10;
/// Abaixo disso (LUFS) a medição é silêncio: não normaliza (`receitas-audio-saida.md` §1).
const SILENCIO_LUFS: f64 = -70.0;
/// Desvio do `atempo` por estágio, em ms do tempo de saída do estágio: estágio que desacelera e que acelera;
/// o de cada estágio é esticado pelos seguintes (÷ fator). Teto do medido nos dois binários
/// (`atempo_desvio_medido_cabe_na_tolerancia`: 25 ms a 0,5×, 70 ms a 0,25×, 432 ms a 0,05×, até 7 ms acelerando).
const ATEMPO_LENTO_MS: f64 = 30.0;
const ATEMPO_RAPIDO_MS: f64 = 12.0;
/// Som de uma entrada que pode ficar esperando no grafo (s de programa; medido: ~0,7 MB por segundo, 30 min = 1,26 GB
/// nos dois binários). Acima disso a mídia ganha outra entrada.
const TETO_ESPERA_S: f64 = 120.0;
/// Entradas a mais no passo inteiro (a linha de comando não cresce além disso).
const MAX_ENTRADAS_EXTRAS: usize = 16;

fn erro(codigo: impl Into<String>) -> SicroError {
    SicroError::Validation(codigo.into())
}

fn nao_suportado(o_que: &str) -> SicroError {
    erro(format!("nao_suportado: {o_que}"))
}

/// O que o motor de áudio ainda não faz, pelo detalhe do catálogo (`mensagens`). Uma regra só: `validar` (fase_0 e
/// transições) e `recusar` perguntam aqui; o P1.5 tira da lista o que passa a fazer, junto com o sítio de recusa.
const RECUSAS: &[&str] = &["normalizar", "limitador", "silenciar", "volume_animado", "abaixar", "transicao"];

/// `detalhe` ainda é recusado pelo motor de áudio?
pub fn recusa(detalhe: &str) -> bool {
    RECUSAS.contains(&detalhe)
}

/// Recorte de mídia que vira som (tempo de fonte). `ate` None = até o fim do som; `entrada`/`saida` = `afade` em amostras.
struct Som<'a> {
    fonte: &'a FonteInfo,
    de: f64,
    ate: Option<f64>,
    velocidade: Ritmo,
    volume: f64,
    entrada: u64,
    saida: u64,
}

/// Velocidade já decidida: 1×, `atempo` em estágios ou `asetrate` (Hz).
enum Ritmo {
    Normal,
    Atempo(Vec<f64>),
    Asetrate(u64),
}

/// Pedaço da trilha principal: amostras e o som (None = silêncio).
struct Pedaco<'a> {
    amostras: u64,
    som: Option<(&'a str, Som<'a>)>,
}

/// Um uso de mídia: recorte de fonte `[de, ate)` (s) que vira `n` amostras a partir da amostra `ini` do programa.
struct Uso<'a> {
    id: &'a str,
    de: f64,
    ate: f64,
    ini: u64,
    n: u64,
}

/// Entradas (na ordem do primeiro uso) e as cadeias do grafo.
#[derive(Default)]
struct Grafo<'a> {
    /// (mídia, instância, fonte): uma por mídia, salvo quando o som teria de esperar demais (`distribuir`).
    entradas: Vec<(&'a str, usize, &'a FonteInfo)>,
    usos: Vec<usize>,
    linhas: Vec<String>,
}

// CONTRATO: a mesma mídia pode ter mais de uma entrada no `PassoAudio` (mesmo `Papel::Midia`, mesmo caminho).
/// Instância da entrada de cada uso. A entrada é lida uma vez, em ordem: o uso que vem depois no programa e antes
/// na fonte fica na memória enquanto a leitura atende o anterior. Passou de `teto` segundos de programa esperando,
/// o uso vai para outra instância da mesma mídia (lida do começo de novo, sem busca: o PCM não muda).
fn distribuir(usos: &[Uso], teto: f64) -> Vec<usize> {
    // por instância: (mídia, [(até onde a leitura foi ao atender o uso j, som dos usos seguintes esperando nessa hora)])
    let mut inst: Vec<(&str, Vec<(f64, f64)>)> = Vec::new();
    let mut ordem: Vec<usize> = (0..usos.len()).collect();
    ordem.sort_by_key(|&i| usos[i].ini);
    let mut de_quem = vec![0usize; usos.len()];
    let mut extras = 0usize;
    for i in ordem {
        let u = &usos[i];
        let dur = u.n as f64 / TAXA_AUDIO as f64;
        let escala = dur / (u.ate - u.de).max(1e-9);
        let espera = |alto: f64| ((u.ate.min(alto) - u.de).max(0.0) * escala).min(dur);
        let pico = |v: &[(f64, f64)]| v.iter().map(|&(alto, fila)| fila + espera(alto)).fold(0.0, f64::max);
        let da_midia: Vec<usize> = (0..inst.len()).filter(|&k| inst[k].0 == u.id).collect();
        let k = match da_midia.iter().copied().find(|&k| pico(&inst[k].1) <= teto) {
            Some(k) => k,
            None if da_midia.is_empty() || extras < MAX_ENTRADAS_EXTRAS => {
                extras += usize::from(!da_midia.is_empty());
                inst.push((u.id, Vec::new()));
                inst.len() - 1
            }
            // sem mais entradas: a que espera menos
            None => da_midia.iter().copied().min_by(|&a, &b| pico(&inst[a].1).total_cmp(&pico(&inst[b].1))).unwrap_or(0),
        };
        let v = &mut inst[k].1;
        for x in v.iter_mut() {
            x.1 += espera(x.0);
        }
        let alto = v.last().map_or(u.ate, |x| x.0.max(u.ate));
        v.push((alto, 0.0));
        de_quem[i] = da_midia.iter().position(|&x| x == k).unwrap_or(da_midia.len());
    }
    de_quem
}

impl<'a> Grafo<'a> {
    /// Rótulo de um novo uso da instância `inst` da mídia (uma saída do `asplit` dela).
    fn uso(&mut self, id: &'a str, inst: usize, fonte: &'a FonteInfo) -> String {
        let k = match self.entradas.iter().position(|(i, n, _)| *i == id && *n == inst) {
            Some(k) => k,
            None => {
                self.entradas.push((id, inst, fonte));
                self.usos.push(0);
                self.entradas.len() - 1
            }
        };
        self.usos[k] += 1;
        format!("m{k}u{}", self.usos[k] - 1)
    }

    fn texto(&self) -> String {
        let mut l: Vec<String> = Vec::with_capacity(self.entradas.len() + self.linhas.len());
        for (k, n) in self.usos.iter().enumerate() {
            let saidas: String = (0..*n).map(|u| format!("[m{k}u{u}]")).collect();
            l.push(format!("[{k}:a:0]asplit={n}{saidas}"));
        }
        l.extend(self.linhas.iter().cloned());
        l.join(";\n")
    }
}

/// Segundos para opção de duração (µs, como o ffmpeg lê).
fn seg(t: f64) -> String {
    format!("{t:.6}")
}

/// Silêncio estéreo de `n` amostras.
fn silencio(n: u64) -> String {
    format!("anullsrc=r={TAXA_AUDIO}:cl=stereo:n={QUADRO_SILENCIO},atrim=end_sample={n},{FORMATO}")
}

/// Cadeia de um recorte: `n` amostras a 48 kHz, estéreo, pts a partir de 0 (§5.3, "Áudio do trecho").
fn cadeia(s: &Som, n: u64) -> String {
    let mut c = format!("atrim=start={}", seg(s.de));
    if let Some(ate) = s.ate {
        let _ = write!(c, ":end={}", seg(ate));
    }
    // T0 absoluto: o silêncio antes do som da fonte fica (A9)
    if s.de < 0.0 {
        let _ = write!(c, ",asetpts=PTS+{}/TB", seg(-s.de));
    } else {
        let _ = write!(c, ",asetpts=PTS-{}/TB", seg(s.de));
    }
    // mono e estéreo pelo índice do canal (cópia exata); mais canais: o `aformat` faz o downmix
    match s.fonte.audio.as_ref().map_or(0, |a| a.canais) {
        1 => c.push_str(",pan=stereo|c0=c0|c1=c0"),
        2 => c.push_str(",pan=stereo|c0=c0|c1=c1"),
        _ => {}
    }
    let _ = write!(c, ",aresample={TAXA_AUDIO}:async=1:first_pts=0");
    match &s.velocidade {
        Ritmo::Normal => {}
        Ritmo::Atempo(e) => {
            for f in e {
                let _ = write!(c, ",atempo={f}");
            }
        }
        Ritmo::Asetrate(hz) => {
            let _ = write!(c, ",asetrate={hz},aresample={TAXA_AUDIO}");
        }
    }
    let _ = write!(c, ",apad,atrim=end_sample={n},asetpts=N/SR/TB,{FORMATO}");
    if s.volume != 1.0 {
        let _ = write!(c, ",volume={}", s.volume);
    }
    if s.entrada > 0 {
        let _ = write!(c, ",afade=t=in:ss=0:ns={}", s.entrada.min(n));
    }
    if s.saida > 0 {
        let k = s.saida.min(n);
        let _ = write!(c, ",afade=t=out:ss={}:ns={k}", n - k);
    }
    c
}

/// Fatores do `atempo` em cadeia, cada um em [0,5; 2] (§5.4).
fn estagios_atempo(v: Racional) -> Vec<f64> {
    let (mut n, mut d) = (v.num as i128, v.den as i128);
    let mut e = Vec::new();
    while n > 2 * d {
        e.push(2.0);
        d *= 2;
    }
    while 2 * n < d {
        e.push(0.5);
        n *= 2;
    }
    if n != d {
        e.push(n as f64 / d as f64);
    }
    e
}

/// Maior desvio do som com `manter_tom` (ms, tempo do programa): o de cada estágio, esticado pelos seguintes.
fn tolerancia_atempo_ms(v: Racional) -> f64 {
    estagios_atempo(v).iter().fold(0.0, |t, &f| t / f + if f < 1.0 { ATEMPO_LENTO_MS } else { ATEMPO_RAPIDO_MS })
}

fn velocidade(v: &Velocidade, manter_tom: bool) -> Result<(Ritmo, f64)> {
    let r = match v {
        Velocidade::Constante { fator } => tempo::fator_racional(*fator)?,
        Velocidade::Rampa { .. } => return Err(nao_suportado("rampa")),
    };
    if r.num == r.den {
        return Ok((Ritmo::Normal, 0.0));
    }
    if manter_tom {
        return Ok((Ritmo::Atempo(estagios_atempo(r)), tolerancia_atempo_ms(r)));
    }
    // exato: 48000·v é inteiro (fator com até 3 casas)
    let hz = TAXA_AUDIO as i128 * r.num as i128;
    if hz % r.den as i128 != 0 {
        return Err(erro("velocidade"));
    }
    Ok((Ritmo::Asetrate((hz / r.den as i128) as u64), 0.0))
}

/// Volume fixo (0..2); chaves todas iguais contam como fixo. Animado: Fase 1.
fn volume_fixo(v: &Animavel<f64>) -> Result<f64> {
    let x = match v {
        Animavel::Fixo(x) => *x,
        Animavel::Chaves(c) => {
            let Some(k) = c.first() else { return Err(erro("item_invalido: volume")) };
            if c.iter().any(|o| o.v != k.v) {
                return Err(nao_suportado("volume animado"));
            }
            k.v
        }
    };
    if !x.is_finite() || x < 0.0 {
        return Err(erro("item_invalido: volume"));
    }
    Ok(x)
}

fn base(c: &ClipePrincipal) -> &ClipeBase {
    match c {
        ClipePrincipal::Trecho(x) => &x.base,
        ClipePrincipal::Congelar(x) => &x.base,
        ClipePrincipal::Imagem(x) => &x.base,
        ClipePrincipal::Titulo(x) => &x.base,
        ClipePrincipal::Tela(x) => &x.base,
    }
}

/// Recusa o que a Fase 0 não faz (nunca ignora calado).
fn recusar(p: &ProjetoEdicao, linha: &LinhaResolvida, clipes: &HashMap<&str, &ClipePrincipal>) -> Result<()> {
    if p.mixagem.normalizar.is_some() && recusa("normalizar") {
        return Err(nao_suportado("normalizar"));
    }
    if p.mixagem.limitador && recusa("limitador") {
        return Err(nao_suportado("limitador"));
    }
    let com_sobreposicao = |t: &Option<Transicao>| t.as_ref().is_some_and(|t| tempo::transicao_sobrepoe(&t.tipo) && recusa("transicao"));
    for r in &linha.clipes {
        let Some(c) = clipes.get(r.id.as_str()) else { continue };
        if com_sobreposicao(&base(c).transicao_entrada) {
            return Err(nao_suportado("transicao"));
        }
        match c {
            ClipePrincipal::Tela(_) => return Err(nao_suportado("tela")),
            ClipePrincipal::Trecho(t) if !t.audio.mudo => {
                if matches!(t.velocidade, Velocidade::Rampa { .. }) {
                    return Err(nao_suportado("rampa"));
                }
                if !t.audio.silencios.is_empty() && recusa("silenciar") {
                    return Err(nao_suportado("silenciar"));
                }
                volume_fixo(&t.audio.volume)?;
            }
            _ => {}
        }
    }
    if com_sobreposicao(&p.fecho) {
        return Err(nao_suportado("transicao"));
    }
    for t in p.audios.iter().filter(|t| !t.mudo && !t.clipes.is_empty()) {
        if t.abaixa_original && recusa("abaixar") {
            return Err(nao_suportado("abaixar"));
        }
        for c in &t.clipes {
            volume_fixo(&c.volume)?;
        }
    }
    // trilha oculta não entra no render (nem na imagem, `sobrepor::aplicar`): nada a recusar nela
    for t in p.sobreposicoes.iter().filter(|t| !t.oculta) {
        for c in &t.clipes {
            if let Tolerante::Conhecido(ClipeSobreposto::Video(v)) = c {
                if v.audio.as_ref().is_some_and(|a| !a.mudo) {
                    return Err(nao_suportado("video sobreposto"));
                }
            }
        }
    }
    Ok(())
}

/// Amostras do `afade` de uma transição `escurecer` de `d` slots que começa no slot `q`.
fn amostras_da_transicao(t: &Transicao, q: u64, d: u64, fps: Racional) -> u64 {
    let n = tempo::amostra_do_slot(q + d, fps) - tempo::amostra_do_slot(q, fps);
    match t.audio {
        AudioTransicao::Acompanhar => n,
        AudioTransicao::Cortar => CORTE_AMOSTRAS.min(n),
    }
}

/// `afade` de entrada e de saída (amostras) de cada clipe da linha: `escurecer` de entrada (e a saída do anterior) e o `fecho`.
fn fades(p: &ProjetoEdicao, linha: &LinhaResolvida, clipes: &HashMap<&str, &ClipePrincipal>) -> (Vec<u64>, Vec<u64>) {
    let fps = linha.fps;
    let n = linha.clipes.len();
    let (mut ent, mut sai) = (vec![0u64; n], vec![0u64; n]);
    let slots = |t: &Transicao| tempo::slots(t.duracao_s, fps);
    for (i, r) in linha.clipes.iter().enumerate() {
        let Some(t) = clipes.get(r.id.as_str()).and_then(|c| base(c).transicao_entrada.as_ref()) else { continue };
        if tempo::transicao_sobrepoe(&t.tipo) {
            continue;
        }
        let d = slots(t).min(r.n_q as u64);
        ent[i] = amostras_da_transicao(t, r.inicio_q, d, fps);
        if i > 0 {
            let a = &linha.clipes[i - 1];
            let d = slots(t).min(a.n_q as u64);
            let fim = a.inicio_q + a.n_q as u64;
            sai[i - 1] = amostras_da_transicao(t, fim - d, d, fps);
        }
    }
    if let (Some(t), Some(r)) = (p.fecho.as_ref().filter(|t| !tempo::transicao_sobrepoe(&t.tipo)), linha.clipes.last()) {
        let d = slots(t).min(r.n_q as u64);
        let fim = r.inicio_q + r.n_q as u64;
        sai[n - 1] = amostras_da_transicao(t, fim - d, d, fps);
    }
    (ent, sai)
}

/// Maior desvio do `atempo` (ms) entre os trechos que tocam som no programa INTEIRO, qualquer que seja o alvo
/// (o `PassoAudio.tolerancia_atempo_ms` é o da janela do alvo).
pub(crate) fn tolerancia_atempo_do_programa(p: &ProjetoEdicao, linha: &LinhaResolvida, ctx: &Contexto) -> f64 {
    if !p.saida.audio {
        return 0.0;
    }
    let mut atempo = 0.0f64;
    for r in &linha.clipes {
        let Some(Tolerante::Conhecido(ClipePrincipal::Trecho(t))) = p.principal.iter().find(|c| matches!(c, Tolerante::Conhecido(c) if base(c).id == r.id)) else {
            continue;
        };
        let toca = !t.audio.mudo && volume_fixo(&t.audio.volume).is_ok_and(|v| v > 0.0);
        if toca && ctx.fontes.get(&t.midia).is_some_and(|f| f.audio.is_some()) {
            atempo = atempo.max(velocidade(&t.velocidade, t.audio.manter_tom).map_or(0.0, |x| x.1));
        }
    }
    atempo
}

/// Buraco de som aceito (ms) num passo: `TOLERANCIA_BURACO_MS` quando ele lê som de alguma fonte; 0 sem passo
/// ou só com silêncio gerado.
pub fn tolerancia_buraco_ms(passo: Option<&PassoAudio>) -> f64 {
    if passo.is_some_and(|a| !a.entradas.is_empty()) {
        TOLERANCIA_BURACO_MS
    } else {
        0.0
    }
}

/// Monta o passo de áudio do programa e a prévia de medição. (None, vazio) quando o alvo não tem som (`Alvo::Quadro`).
/// Programa sem `saida.audio`: trilha de silêncio. `Alvo::Previa`: as amostras dos slots `[de_q, ate_q)`.
/// Fase 0: nenhuma prévia (a medição do `loudnorm` entra com a normalização, P1.5).
pub fn montar(p: &ProjetoEdicao, linha: &LinhaResolvida, ctx: &Contexto, alvo: &Alvo) -> Result<(Option<PassoAudio>, Vec<Previa>)> {
    montar_com_teto(p, linha, ctx, alvo, TETO_ESPERA_S)
}

/// `montar` com o teto de espera (`distribuir`) dado.
fn montar_com_teto(p: &ProjetoEdicao, linha: &LinhaResolvida, ctx: &Contexto, alvo: &Alvo, teto: f64) -> Result<(Option<PassoAudio>, Vec<Previa>)> {
    let (de_q, ate_q) = match alvo {
        Alvo::Quadro { .. } => return Ok((None, Vec::new())),
        Alvo::Final { .. } => (0, linha.total_q),
        Alvo::Previa { de_q, ate_q, .. } => (*de_q, (*ate_q).min(linha.total_q)),
    };
    if linha.total_q == 0 || linha.clipes.is_empty() {
        return Err(erro("vazio"));
    }
    if de_q >= ate_q {
        return Err(erro("intervalo"));
    }
    let fps = linha.fps;
    let a = |q: u64| tempo::amostra_do_slot(q, fps);
    let total = a(linha.total_q);
    let (s0, s1) = (a(de_q), a(ate_q));
    let mut g = Grafo::default();
    let mut tolerancia: f64 = 0.0;

    let fim_do_programa = if !p.saida.audio {
        g.linhas.push(format!("{}[sl]", silencio(s1 - s0)));
        "sl".to_string()
    } else {
        let clipes: HashMap<&str, &ClipePrincipal> = p
            .principal
            .iter()
            .filter_map(|c| match c {
                Tolerante::Conhecido(c) => Some((base(c).id.as_str(), c)),
                Tolerante::Desconhecido(_) => None,
            })
            .collect();
        recusar(p, linha, &clipes)?;
        let (ent, sai) = fades(p, linha, &clipes);

        // trilha principal: um pedaço por clipe; os de fora da janela e os sem som são silêncio
        let mut pedacos: Vec<Pedaco> = Vec::with_capacity(linha.clipes.len());
        for (i, r) in linha.clipes.iter().enumerate() {
            let (ini, fim) = (a(r.inicio_q), a(r.inicio_q + r.n_q as u64));
            let mut som = None;
            if let Some(ClipePrincipal::Trecho(t)) = clipes.get(r.id.as_str()).copied() {
                let volume = if t.audio.mudo { 0.0 } else { volume_fixo(&t.audio.volume)? };
                if volume > 0.0 && fim > s0 && ini < s1 {
                    let fonte = ctx.fontes.get(&t.midia).ok_or_else(|| erro(format!("midia_ausente: {}", t.midia)))?;
                    let faixa = r.fonte.as_ref().ok_or_else(|| erro(format!("intervalo: {}", r.id)))?;
                    if fonte.audio.is_some() {
                        let tab = &fonte.quadros;
                        let tb = |pts: i64| pts as f64 * tab.tb.num as f64 / tab.tb.den as f64;
                        let (v, tol) = velocidade(&t.velocidade, t.audio.manter_tom)?;
                        tolerancia = tolerancia.max(tol);
                        let s = Som {
                            fonte,
                            de: tb(faixa.pts_in),
                            ate: Some(faixa.pts_out.map_or_else(|| tab.fim_s(), tb)),
                            velocidade: v,
                            volume,
                            entrada: ent[i],
                            saida: sai[i],
                        };
                        som = Some((t.midia.as_str(), s));
                    }
                }
            }
            // silêncios vizinhos viram um só
            if som.is_none() {
                if let Some(u) = pedacos.last_mut().filter(|u| u.som.is_none()) {
                    u.amostras += fim - ini;
                    continue;
                }
            }
            pedacos.push(Pedaco { amostras: fim - ini, som });
        }
        // trilhas de áudio: cada clipe no lugar dele, do tamanho do programa
        let mut faixas: Vec<(&str, Som, u64, u64)> = Vec::new();
        for t in p.audios.iter().filter(|t| !t.mudo) {
            for c in &t.clipes {
                let Some(r) = linha.livres.iter().find(|l| l.id == c.id && l.trilha == t.id) else { continue };
                let (ini, fim) = (a(r.inicio_q), a(r.inicio_q + r.n_q as u64).min(total));
                let volume = t.volume * volume_fixo(&c.volume)?;
                if !volume.is_finite() || volume <= 0.0 || ini >= fim || fim <= s0 || ini >= s1 {
                    continue;
                }
                let fonte = ctx.fontes.get(&c.midia).ok_or_else(|| erro(format!("midia_ausente: {}", c.midia)))?;
                let Some(som_da_fonte) = &fonte.audio else { continue };
                let n = a(r.inicio_q + r.n_q as u64) - ini;
                let amostras_de = |s: f64| if s.is_finite() && s > 0.0 { ((s * TAXA_AUDIO as f64).round() as u64).min(n) } else { 0 };
                // `de_s`/`ate_s` contam do começo do som do arquivo (MP3 começa em 0,025 s; início negativo é pré-rolagem)
                let inicio = som_da_fonte.inicio_s.max(0.0);
                let s = Som {
                    fonte,
                    de: inicio + c.de_s,
                    ate: c.ate_s.map(|x| inicio + x),
                    velocidade: Ritmo::Normal,
                    volume,
                    entrada: amostras_de(c.entrada_s),
                    saida: amostras_de(c.saida_s),
                };
                faixas.push((c.midia.as_str(), s, n, ini));
            }
        }

        // instância da entrada de cada uso: os da principal (na ordem) e depois os das trilhas
        let mut usos: Vec<Uso> = Vec::new();
        let mut ini = 0u64;
        for pd in &pedacos {
            if let Some((id, s)) = &pd.som {
                usos.push(Uso { id, de: s.de, ate: s.ate.unwrap_or(s.de), ini, n: pd.amostras });
            }
            ini += pd.amostras;
        }
        for (id, s, n, ini) in &faixas {
            let ate = s.ate.unwrap_or(s.de + *n as f64 / TAXA_AUDIO as f64);
            usos.push(Uso { id, de: s.de, ate, ini: *ini, n: *n });
        }
        let mut inst = distribuir(&usos, teto).into_iter();
        let mut proxima = || inst.next().unwrap_or(0);

        let rotulos: Vec<String> = pedacos
            .iter()
            .enumerate()
            .map(|(k, pd)| {
                let r = format!("p{k}");
                let linha = match &pd.som {
                    Some((id, s)) => format!("[{}]{}[{r}]", g.uso(id, proxima(), s.fonte), cadeia(s, pd.amostras)),
                    None => format!("{}[{r}]", silencio(pd.amostras)),
                };
                g.linhas.push(linha);
                r
            })
            .collect();
        let mut atual = match rotulos.len() {
            1 => rotulos[0].clone(),
            n => {
                g.linhas.push(format!("{}concat=n={n}:v=0:a=1[pr]", rotulos.iter().map(|r| format!("[{r}]")).collect::<String>()));
                "pr".to_string()
            }
        };
        let vo = p.mixagem.volume_original;
        if !vo.is_finite() || vo < 0.0 {
            return Err(erro("item_invalido: volume_original"));
        }
        if vo != 1.0 {
            g.linhas.push(format!("[{atual}]volume={vo}[po]"));
            atual = "po".into();
        }

        let mut mistura = vec![atual];
        for (id, s, n, ini) in &faixas {
            let r = format!("n{}", mistura.len() - 1);
            let mut l = format!("[{}]{}", g.uso(id, proxima(), s.fonte), cadeia(s, *n));
            if *ini > 0 {
                let _ = write!(l, ",adelay=delays={ini}S:all=1");
            }
            let _ = write!(l, ",apad,atrim=end_sample={total}[{r}]");
            g.linhas.push(l);
            mistura.push(r);
        }
        let mut atual = match mistura.len() {
            1 => mistura.pop().unwrap_or_default(),
            n => {
                let entradas: String = mistura.iter().map(|r| format!("[{r}]")).collect();
                g.linhas.push(format!("{entradas}amix=inputs={n}:normalize=0:dropout_transition=0:duration=first[mi]"));
                "mi".into()
            }
        };
        if (s0, s1) != (0, total) {
            g.linhas.push(format!("[{atual}]atrim=start_sample={s0}:end_sample={s1},asetpts=N/SR/TB[jn]"));
            atual = "jn".into();
        }
        atual
    };
    g.linhas.push(format!("[{fim_do_programa}]volume={MARCA_GANHO}dB,{FORMATO_SAIDA},asplit=2[a][ah]"));

    let mut globais: Vec<String> = CPUFLAGS.iter().map(|s| s.to_string()).collect();
    globais.extend(args::globais());
    let entradas = g
        .entradas
        .iter()
        .map(|(id, _, f)| Entrada {
            antes: vec!["-vn".into()],
            antes_sem_busca: None,
            caminho: f.caminho.clone(),
            papel: Papel::Midia { id: id.to_string(), clipe: String::new() },
        })
        .collect();
    let passo = PassoAudio {
        globais,
        entradas,
        grafo: g.texto(),
        auxiliares: Vec::new(),
        saida: SAIDA.into(),
        hash: HASH.into(),
        amostras: s1 - s0,
        normalizar: None,
        tolerancia_atempo_ms: tolerancia,
    };
    Ok((Some(passo), Vec::new()))
}

// CONTRATO: acréscimo. A linha do E9 montada num lugar só (executor e testes).
/// Argumentos do ffmpeg da passada (depois do binário), com o grafo (`grafo_com_ganho`) já gravado como `nome_grafo`
/// na pasta de trabalho: `<globais> <entradas> <grafo> -map [a] -c:a pcm_s16le <bitexact> audio.wav
/// -map [ah] -c:a pcm_s16le -f hash -hash sha256 audio.sha256`.
pub fn argumentos(passo: &PassoAudio, cap: &Capacidades, nome_grafo: &str) -> Vec<String> {
    let mut a = passo.globais.clone();
    for e in &passo.entradas {
        a.extend(e.antes.iter().cloned());
        a.push("-i".into());
        a.push(e.caminho.to_string_lossy().into_owned());
    }
    a.extend(args::grafo_em_arquivo(cap, nome_grafo));
    let fixos = ["-map", "[a]", "-c:a", "pcm_s16le", "-fflags", "+bitexact", "-flags:a", "+bitexact"];
    a.extend(fixos.iter().map(|s| s.to_string()));
    a.push(passo.saida.clone());
    let hash = ["-map", "[ah]", "-c:a", "pcm_s16le", "-f", "hash", "-hash", "sha256"];
    a.extend(hash.iter().map(|s| s.to_string()));
    a.push(passo.hash.clone());
    a
}

/// Lê o JSON que o `loudnorm=print_format=json` imprime no stderr: o ÚLTIMO bloco depois de uma linha que começa
/// por `[Parsed_loudnorm_` (metadado do arquivo, impresso antes, não se passa por ele).
pub fn ler_loudnorm(stderr: &str) -> Result<Loudnorm> {
    let linhas: Vec<&str> = stderr.lines().map(|l| l.strip_prefix("[info] ").unwrap_or(l)).collect();
    let ilegivel = || erro("loudnorm_ilegivel");
    let i = linhas.iter().rposition(|l| l.starts_with("[Parsed_loudnorm_")).ok_or_else(ilegivel)?;
    let ini = (i + 1..linhas.len()).find(|&k| linhas[k].trim() == "{").ok_or_else(ilegivel)?;
    let fim = (ini..linhas.len()).find(|&k| linhas[k].trim() == "}").ok_or_else(ilegivel)?;
    let j: serde_json::Value = serde_json::from_str(&linhas[ini..=fim].join("\n")).map_err(|_| ilegivel())?;
    let num = |k: &str| j[k].as_str().and_then(|s| s.trim().parse::<f64>().ok()).filter(|x| x.is_finite());
    if j.get("input_i").is_none() {
        return Err(ilegivel());
    }
    Ok(Loudnorm { integrado: num("input_i"), pico: num("input_tp"), faixa: num("input_lra").unwrap_or(0.0), limiar: num("input_thresh") })
}

/// Ganho em dB da passada 2: min(alvo − integrado, pico alvo − pico medido); None = não normaliza
/// (sem alvo, silêncio ou integrado abaixo de −70 LUFS).
pub fn ganho_db(medido: &Loudnorm, alvo: Option<&Normalizar>) -> Option<f64> {
    let alvo = alvo?;
    let i = medido.integrado.filter(|i| *i >= SILENCIO_LUFS)?;
    let mut g = alvo.lufs - i;
    if let Some(tp) = medido.pico {
        g = g.min(alvo.pico_db - tp);
    }
    g.is_finite().then_some(g)
}

/// Grafo final: `PassoAudio.grafo` com `MARCA_GANHO` trocada pelo ganho (0 quando None).
pub fn grafo_com_ganho(passo: &PassoAudio, ganho_db: Option<f64>) -> String {
    let g = ganho_db.filter(|g| g.is_finite()).unwrap_or(0.0);
    passo.grafo.replace(MARCA_GANHO, &format!("{g:.6}"))
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, OnceLock};
    use std::time::Instant;

    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};

    use crate::edicao::ffmpeg::exec::{self, Comando};
    use crate::edicao::ffmpeg::sonda::{self, Origem};
    use crate::edicao::modelo::{ClipeResolvido, EstadoMidia};
    use crate::edicao::motor::AudioInfo;
    use crate::edicao::quadros::{self, apoio, TabelaQuadros};
    use crate::edicao::testes::fontes;

    /// Cliques de 1 ms em x,5 s sobre seno de 220 Hz (`cm/19`).
    const CLIQUES: &str = "aevalsrc=exprs=0.9*lt(mod(t+0.5\\,1)\\,0.001)+0.02*sin(2*PI*220*t)";
    /// Cliques como `CLIQUES` e rajadas de 150 ms (1 kHz) em x,0 s. O `atempo` acelerando apaga clique mais curto
    /// que a janela dele (~43 ms; medido: a 4× some metade): o som com `manter_tom` é medido pelas rajadas.
    const SINAL: &str = "aevalsrc=exprs=0.9*lt(mod(t+0.5\\,1)\\,0.001)+0.9*lt(mod(t\\,1)\\,0.15)*sin(2*PI*1000*t)+0.02*sin(2*PI*220*t)";
    /// Metade do clique e da rajada (0,9 da escala).
    const LIMIAR: u16 = 14_000;
    /// Tolerância do clique no modo exato e no 1× (amostras): a borda do pulso passa pelo reamostrador.
    const TOL_EXATO: f64 = 3.0;

    // ------------------------------------------------------------ projeto e contexto

    fn projeto(fps: (i64, i64), principal: Vec<Value>, audios: Vec<Value>, midias: Vec<Value>) -> ProjetoEdicao {
        serde_json::from_value(json!({
            "esquema": 2, "id": "p", "titulo": "t", "created_at": "2026-11-14T00:00:00Z", "updated_at": "2026-11-14T00:00:00Z",
            "saida": { "largura": 320, "altura": 180, "fps": { "num": fps.0, "den": fps.1 }, "cor_fundo": "#000000", "audio": true,
                       "selos": { "velocidade": true, "ampliacao": true, "realce": true },
                       "exportacao": { "predefinicao": "juri", "codificador": "cpu", "legendas": "nenhuma", "pdf": "a3", "revisado_por": "" } },
            "midias": midias, "principal": principal, "audios": audios
        }))
        .expect("projeto")
    }

    fn trecho(id: &str, midia: &str, de: f64, ate: f64, fator: f64, manter_tom: bool) -> Value {
        json!({ "tipo": "trecho", "id": id, "midia": midia, "inicio_s": de, "fim_s": ate,
                "velocidade": { "tipo": "constante", "fator": fator },
                "audio": { "volume": 1.0, "mudo": false, "manter_tom": manter_tom, "silencios": [] } })
    }

    fn imagem(id: &str, duracao: f64) -> Value {
        json!({ "tipo": "imagem", "id": id, "midia": "img", "duracao_s": duracao })
    }

    fn trilha(id: &str, volume: f64, clipes: Vec<Value>) -> Value {
        json!({ "id": id, "nome": "Narração", "papel": "narracao", "volume": volume, "mudo": false, "abaixa_original": false, "clipes": clipes })
    }

    fn clipe_audio(id: &str, midia: &str, pos: Value, de: f64, ate: Option<f64>) -> Value {
        json!({ "id": id, "nome": "", "midia": midia, "pos": pos, "de_s": de, "ate_s": ate, "volume": 1.0, "entrada_s": 0.0, "saida_s": 0.0 })
    }

    fn midia_ref(id: &str, duracao: f64) -> Value {
        json!({ "id": id, "tipo": "audio", "origem": "projeto", "sha256": "0", "nome": id, "relativo": id, "duracao_s": duracao,
                "vfr": false, "tem_audio": true, "recebido": { "nativo": null, "obs": "" } })
    }

    fn final_(p: &ProjetoEdicao) -> Alvo {
        Alvo::Final { exportacao: p.saida.exportacao.clone() }
    }

    /// Linha resolvida e passo do projeto.
    fn montar_com(p: &ProjetoEdicao, fontes: &HashMap<String, FonteInfo>, cap: &Capacidades, alvo: &Alvo) -> Result<(PassoAudio, LinhaResolvida)> {
        let (leitor, pasta, midias) = (apoio::SemRastreios, PathBuf::new(), Vec::<EstadoMidia>::new());
        let ctx = Contexto { fontes, cap, rastreios: &leitor, pasta_projeto: &pasta, midias: &midias };
        let linha = tempo::resolver(p, &ctx);
        let (passo, previas) = montar(p, &linha, &ctx, alvo)?;
        assert!(previas.is_empty());
        Ok((passo.expect("passo"), linha))
    }

    // ------------------------------------------------------------ ffmpeg

    fn texto(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn comando(cap: &Capacidades, pasta: &Path, nome: &str, args: Vec<String>) -> Comando {
        Comando {
            nome: nome.into(),
            programa: cap.binario.clone(),
            args,
            pasta: pasta.to_path_buf(),
            ambiente: Vec::new(),
            sem_ambiente: Vec::new(),
            stderr: pasta.join(format!("{nome}.stderr")),
        }
    }

    /// Gera um arquivo de teste com o ffmpeg (`args` sem o nome de saída).
    fn gerar(cap: &Capacidades, pasta: &Path, nome: &str, args: &[&str]) -> PathBuf {
        let mut a = texto(&["-hide_banner", "-nostdin", "-y", "-loglevel", "error"]);
        a.extend(texto(args));
        a.push(nome.into());
        let cmd = comando(cap, pasta, &format!("gerar-{nome}"), a);
        let t = exec::rodar(&cmd, &AtomicBool::new(false)).expect("ffmpeg");
        exec::exigir_sucesso(&cmd, &t).expect("gerar");
        pasta.join(nome)
    }

    /// Vídeo 160×90 a `fps` com o som `lavfi` dado (`audio_antes` = opções antes do `-i` do som).
    fn com_video(cap: &Capacidades, pasta: &Path, nome: &str, (fps, dur): (&str, f64), audio_antes: &[&str], som: &str, codec: &[&str]) -> PathBuf {
        let v = format!("testsrc2=s=160x90:r={fps}:d={dur}");
        let mut a = vec!["-f", "lavfi", "-i", &v];
        a.extend(audio_antes);
        a.extend(["-f", "lavfi", "-i", som, "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "ultrafast", "-g", "30", "-pix_fmt", "yuv420p"]);
        a.extend(codec);
        gerar(cap, pasta, nome, &a)
    }

    fn info(cap: &Capacidades, arquivo: &Path) -> FonteInfo {
        let tab = TabelaQuadros::do_decodificador(cap, arquivo, &AtomicBool::new(false), &mut |_| {}).expect("tabela");
        quadros::fonte_info(cap, arquivo, &"0".repeat(64), Arc::new(tab), None).expect("fonte_info")
    }

    /// Arquivo só de som: a tabela vem vazia de `fonte_info`.
    fn info_som(cap: &Capacidades, arquivo: &Path) -> FonteInfo {
        let tab = TabelaQuadros::de_pacotes(cap, arquivo).expect("tabela");
        let f = quadros::fonte_info(cap, arquivo, &"0".repeat(64), Arc::new(tab), None).expect("fonte_info");
        assert!(f.audio.is_some() && f.quadros.pts.is_empty());
        f
    }

    /// PCM s16 estéreo intercalado e o SHA-256 que o próprio passo gravou.
    struct Pcm {
        x: Vec<i16>,
        sha: String,
    }

    impl Pcm {
        fn n(&self) -> usize {
            self.x.len() / 2
        }
        fn canal(&self, c: usize) -> Vec<i16> {
            self.x.iter().skip(c).step_by(2).copied().collect()
        }
    }

    /// Amostras do `data` de um WAV PCM s16 estéreo 48 kHz.
    fn ler_wav(b: &[u8]) -> &[u8] {
        assert_eq!((&b[0..4], &b[8..12]), (&b"RIFF"[..], &b"WAVE"[..]));
        let mut i = 12;
        let mut fmt = None;
        while i + 8 <= b.len() {
            let n = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
            match &b[i..i + 4] {
                b"fmt " => fmt = Some((u16::from_le_bytes([b[i + 10], b[i + 11]]), u32::from_le_bytes(b[i + 12..i + 16].try_into().unwrap()), u16::from_le_bytes([b[i + 22], b[i + 23]]))),
                b"data" => {
                    assert_eq!(fmt, Some((2, 48_000, 16)), "canais, taxa, bits");
                    return &b[i + 8..i + 8 + n];
                }
                _ => {}
            }
            i += 8 + n + (n & 1);
        }
        panic!("WAV sem data")
    }

    fn s16(b: &[u8]) -> Vec<i16> {
        b.as_chunks::<2>().0.iter().map(|c| i16::from_le_bytes(*c)).collect()
    }

    /// Roda o passo como o executor (grafo em arquivo, linha de `argumentos`); `globais` troca os globais (O6).
    fn rodar_com(cap: &Capacidades, passo: &PassoAudio, pasta: &Path, globais: Option<Vec<String>>) -> (Pcm, f64) {
        std::fs::create_dir_all(pasta).unwrap();
        args::gravar_texto(pasta, "audio.txt", &grafo_com_ganho(passo, None)).unwrap();
        let mut p = passo.clone();
        if let Some(g) = globais {
            p.globais = g;
        }
        let cmd = comando(cap, pasta, "audio", argumentos(&p, cap, "audio.txt"));
        let t0 = Instant::now();
        let t = exec::rodar(&cmd, &AtomicBool::new(false)).expect("ffmpeg");
        let s = t0.elapsed().as_secs_f64();
        exec::exigir_sucesso(&cmd, &t).expect("passo de áudio");
        let wav = std::fs::read(pasta.join(&passo.saida)).unwrap();
        let dados = ler_wav(&wav);
        let sha = std::fs::read_to_string(pasta.join(&passo.hash)).unwrap().trim().trim_start_matches("SHA256=").to_string();
        // o hash do `[ah]` é o do PCM do `[a]`
        assert_eq!(sha, format!("{:x}", Sha256::digest(dados)), "{}", cap.versao);
        (Pcm { x: s16(dados), sha }, s)
    }

    fn rodar(cap: &Capacidades, passo: &PassoAudio, pasta: &Path) -> Pcm {
        rodar_com(cap, passo, pasta, None).0
    }

    /// Roda uma linha do grafo do passo sozinha (sem `asplit`, sem `concat`) e devolve o PCM s16 estéreo.
    /// `busca` = `-ss` antes do `-i` (a leitura que o §5.9 evita).
    fn rodar_pedaco(cap: &Capacidades, pasta: &Path, nome: &str, fonte: Option<&Path>, cadeia: &str, busca: Option<f64>) -> Vec<i16> {
        let grafo = if fonte.is_some() { format!("[0:a:0]{cadeia}[o]") } else { format!("{cadeia}[o]") };
        let g = format!("{nome}.txt");
        args::gravar_texto(pasta, &g, &grafo).unwrap();
        let mut a = texto(&["-cpuflags", "0", "-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-copyts"]);
        if let Some(f) = fonte {
            if let Some(ss) = busca {
                a.extend(["-ss".to_string(), seg(ss)]);
            }
            a.extend(["-vn".to_string(), "-i".to_string(), f.to_string_lossy().into_owned()]);
        }
        a.extend(args::grafo_em_arquivo(cap, &g));
        a.extend(texto(&["-map", "[o]", "-c:a", "pcm_s16le", "-f", "s16le", "-"]));
        let cmd = comando(cap, pasta, nome, a);
        let (t, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).expect("ffmpeg");
        exec::exigir_sucesso(&cmd, &t).expect("pedaço");
        s16(&out)
    }

    /// Os pedaços da principal (`[mKuU]cadeia[pK]` ou `silêncio[pK]`), na ordem: (usa a mídia, cadeia).
    fn pedacos_do_grafo(grafo: &str) -> Vec<(bool, String)> {
        let mut v = Vec::new();
        for k in 0.. {
            let fim = format!("[p{k}]");
            let Some(l) = grafo.split(";\n").find(|l| l.ends_with(&fim)) else { break };
            let l = l.strip_suffix(&fim).unwrap();
            match l.strip_prefix('[') {
                Some(r) => v.push((true, r.split_once(']').unwrap().1.to_string())),
                None => v.push((false, l.to_string())),
            }
        }
        v
    }

    /// ffmpeg do PATH (o do sistema) e o do AppImage: os dois, diferentes, ou None (com `SICRO_EXIGE_FFMPEG=1`, pânico).
    fn dois_binarios() -> Option<[Arc<Capacidades>; 2]> {
        static B: OnceLock<Option<[Arc<Capacidades>; 2]>> = OnceLock::new();
        let b = B
            .get_or_init(|| {
                let sistema = std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join("ffmpeg")).find(|f| f.is_file()));
                let embutido = Path::new(env!("CARGO_MANIFEST_DIR")).join("ffmpeg-linux/ffmpeg");
                let cap = |b: &Path| sonda::capacidades_de(b, Origem::Configurado, None, false).ok().filter(|c| c.faltam.is_empty());
                match (sistema.as_deref().and_then(cap), embutido.is_file().then(|| cap(&embutido)).flatten()) {
                    (Some(a), Some(b)) if a.sha256 != b.sha256 => Some([a, b]),
                    _ => None,
                }
            })
            .clone();
        if b.is_none() {
            // só o Linux tem o do AppImage (`scripts/fetch-ffmpeg-linux.sh`); a CI do Windows roda com um ffmpeg só
            let exige = cfg!(target_os = "linux") && std::env::var("SICRO_EXIGE_FFMPEG").ok().as_deref() == Some("1");
            assert!(!exige, "SICRO_EXIGE_FFMPEG=1 e não há os dois ffmpeg (PATH e src-tauri/ffmpeg-linux)");
            eprintln!("aviso: teste dispensado, sem os dois ffmpeg");
        }
        b
    }

    // ------------------------------------------------------------ medidas no PCM

    /// Primeira amostra acima de `LIMIAR` em [a, b).
    fn inicio_do_clique(x: &[i16], a: usize, b: usize) -> Option<usize> {
        (a..b.min(x.len())).find(|&i| x[i].unsigned_abs() > LIMIAR)
    }

    /// Cliques da fonte (s) que caem no recorte do clipe e a amostra esperada no programa: (s, amostra).
    fn cliques_no_programa(cliques: &[f64], r: &ClipeResolvido, tab: &TabelaQuadros, v: f64, fps: Racional) -> Vec<(f64, f64)> {
        let f = r.fonte.as_ref().expect("faixa");
        let tb = |p: i64| p as f64 * tab.tb.num as f64 / tab.tb.den as f64;
        let (t0, t1) = (tb(f.pts_in), f.pts_out.map_or_else(|| tab.fim_s(), tb));
        let a0 = tempo::amostra_do_slot(r.inicio_q, fps) as f64;
        cliques.iter().filter(|&&s| s >= t0 && s < t1).map(|&s| (s, a0 + (s - t0) / v * TAXA_AUDIO as f64)).collect()
    }

    /// Desvio (amostras) de cada clique esperado do clipe; janela de ¼ do espaço entre cliques, dentro do clipe.
    fn desvios(x: &[i16], r: &ClipeResolvido, esperados: &[(f64, f64)], v: f64, fps: Racional) -> Vec<f64> {
        let (ini, fim) = (tempo::amostra_do_slot(r.inicio_q, fps) as f64, tempo::amostra_do_slot(r.inicio_q + r.n_q as u64, fps) as f64);
        let raio = 0.25 / v * TAXA_AUDIO as f64;
        esperados
            .iter()
            .map(|&(s, e)| {
                let (a, b) = ((e - raio).max(ini) as usize, (e + raio).min(fim) as usize);
                let i = inicio_do_clique(x, a, b).unwrap_or_else(|| panic!("marca de {s} s não achada perto da amostra {e}"));
                i as f64 - e
            })
            .collect()
    }

    fn racional(fps: (i64, i64)) -> Racional {
        Racional { num: fps.0, den: fps.1 }
    }

    // ------------------------------------------------------------ sem ffmpeg

    /// Projeto de teste sem disco: fontes sintéticas com som.
    fn fontes_falsas() -> HashMap<String, FonteInfo> {
        let mut f = HashMap::new();
        for (id, canais) in [("v1", 1), ("v2", 2), ("n1", 1)] {
            let tab = apoio::tabela((1, 15360), apoio::pts_regulares(900, (1, 30), (1, 15360), 0));
            let mut i = apoio::fonte(tab, (320, 180));
            i.caminho = PathBuf::from(format!("/caso/{id}.mp4"));
            i.audio = Some(AudioInfo { inicio_s: 0.0, taxa: 48000, canais });
            f.insert(id.to_string(), i);
        }
        f
    }

    fn projeto_falso() -> ProjetoEdicao {
        let mut b = trecho("b", "v2", 4.0, 6.0, 2.0, true);
        b["transicao_entrada"] = json!({ "tipo": "escurecer", "duracao_s": 0.5, "audio": "acompanhar" });
        let mut p = projeto(
            (30000, 1001),
            vec![trecho("a", "v1", 1.0, 3.0, 1.0, true), imagem("i", 1.0), b, trecho("c", "v1", 10.0, 12.5, 0.5, false)],
            vec![trilha("t", 0.8, vec![clipe_audio("n", "n1", json!({ "ref": "clipe", "clipe": "b", "desloc": 0.25 }), 0.0, Some(3.0))])],
            vec![],
        );
        p.mixagem.volume_original = 0.7;
        p
    }

    #[test]
    fn passo_tem_o_molde_do_contrato() {
        let (cap, fontes) = (apoio::cap_falsa(), fontes_falsas());
        let p = projeto_falso();
        let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        assert_eq!(&passo.globais[..2], &CPUFLAGS.map(String::from)[..]);
        assert!(passo.globais.contains(&"-copyts".to_string()));
        assert_eq!(passo.amostras, tempo::amostra_do_slot(linha.total_q, linha.fps));
        // uma entrada por mídia, sem busca, na ordem do primeiro uso
        let ids: Vec<_> = passo.entradas.iter().map(|e| e.papel.clone()).collect();
        let midia = |id: &str| Papel::Midia { id: id.into(), clipe: String::new() };
        assert_eq!(ids, vec![midia("v1"), midia("v2"), midia("n1")]);
        assert!(passo.entradas.iter().all(|e| e.antes == ["-vn"] && e.antes_sem_busca.is_none()));
        assert_eq!(passo.grafo.matches(MARCA_GANHO).count(), 1);
        assert!(passo.grafo.contains("[0:a:0]asplit=2[m0u0][m0u1]"), "{}", passo.grafo);
        assert!(!passo.grafo.contains("-ss") && !passo.grafo.contains('#'));
        assert!(passo.grafo.ends_with("asplit=2[a][ah]"));
        assert!(passo.grafo.contains("amix=inputs=2:normalize=0"));
        assert!(passo.grafo.contains("volume=0.7[po]"));
        assert!(passo.grafo.contains("asetrate=24000,aresample=48000"));
        assert!(passo.grafo.contains("atempo=2"));
        assert!(passo.grafo.contains("volume=0.8"));
        assert_eq!((passo.saida.as_str(), passo.hash.as_str(), passo.normalizar, passo.auxiliares.len()), ("audio.wav", "audio.sha256", None, 0));
        assert_eq!(passo.tolerancia_atempo_ms, ATEMPO_RAPIDO_MS);
        // a linha do E9
        let a = argumentos(&passo, &cap, "audio.txt");
        let fim = texto(&["-map", "[ah]", "-c:a", "pcm_s16le", "-f", "hash", "-hash", "sha256", "audio.sha256"]);
        assert_eq!(&a[a.len() - fim.len()..], &fim[..]);
        assert!(a.windows(2).any(|w| w == ["-/filter_complex", "audio.txt"]));
        assert_eq!(a.iter().filter(|x| *x == "-i").count(), 3);
    }

    #[test]
    fn montar_e_deterministica() {
        let cap = apoio::cap_falsa();
        let p = projeto_falso();
        let um = fontes_falsas();
        // mesmo conteúdo, outra ordem de inserção no HashMap
        let mut outro: HashMap<String, FonteInfo> = HashMap::with_capacity(64);
        let mut chaves: Vec<_> = um.keys().cloned().collect();
        chaves.sort_unstable_by(|a, b| b.cmp(a));
        for k in chaves {
            outro.insert(k.clone(), um[&k].clone());
        }
        let (a, _) = montar_com(&p, &um, &cap, &final_(&p)).unwrap();
        for f in [&um, &outro, &um] {
            let (b, _) = montar_com(&p, f, &cap, &final_(&p)).unwrap();
            assert_eq!(a, b);
            assert_eq!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&b).unwrap());
        }
    }

    #[test]
    fn quadro_nao_tem_som_e_previa_corta() {
        let (cap, fontes) = (apoio::cap_falsa(), fontes_falsas());
        let p = projeto_falso();
        let (leitor, pasta, midias) = (apoio::SemRastreios, PathBuf::new(), Vec::<EstadoMidia>::new());
        let ctx = Contexto { fontes: &fontes, cap: &cap, rastreios: &leitor, pasta_projeto: &pasta, midias: &midias };
        let linha = tempo::resolver(&p, &ctx);
        assert_eq!(montar(&p, &linha, &ctx, &Alvo::Quadro { q: 3, largura: 640 }).unwrap(), (None, Vec::new()));
        let previa = |de_q, ate_q| montar(&p, &linha, &ctx, &Alvo::Previa { altura: 360, de_q, ate_q });
        let (passo, _) = previa(10, 50).unwrap();
        let passo = passo.unwrap();
        let a = |q| tempo::amostra_do_slot(q, linha.fps);
        assert_eq!(passo.amostras, a(50) - a(10));
        assert!(passo.grafo.contains(&format!("atrim=start_sample={}:end_sample={}", a(10), a(50))));
        // só o primeiro clipe cai na janela: uma mídia só
        assert_eq!(passo.entradas.len(), 1);
        // janela além do fim: até o fim; vazia: erro
        let (passo, _) = previa(10, u64::MAX).unwrap();
        assert_eq!(passo.unwrap().amostras, a(linha.total_q) - a(10));
        assert!(matches!(previa(50, 50), Err(SicroError::Validation(c)) if c == "intervalo"));
    }

    #[test]
    fn sem_som_na_saida_e_silencio_do_tamanho_do_programa() {
        let (cap, fontes) = (apoio::cap_falsa(), fontes_falsas());
        let mut p = projeto_falso();
        p.saida.audio = false;
        // nada da Fase 1 pesa quando a saída não tem som
        p.mixagem.limitador = true;
        let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        assert!(passo.entradas.is_empty());
        assert_eq!(passo.amostras, tempo::amostra_do_slot(linha.total_q, linha.fps));
        assert!(passo.grafo.starts_with("anullsrc="), "{}", passo.grafo);
        assert_eq!(passo.grafo.matches(MARCA_GANHO).count(), 1);
    }

    #[test]
    fn recusa_o_que_e_da_fase_1() {
        let (cap, fontes) = (apoio::cap_falsa(), fontes_falsas());
        let erro_de = |p: &ProjetoEdicao| match montar_com(p, &fontes, &cap, &final_(p)) {
            Err(SicroError::Validation(c)) => c,
            outro => panic!("esperava erro: {outro:?}"),
        };
        type Mudanca = Box<dyn Fn(&mut ProjetoEdicao)>;
        let casos: Vec<(&str, Mudanca)> = vec![
            ("nao_suportado: normalizar", Box::new(|p| p.mixagem.normalizar = Some(Normalizar { lufs: -16.0, pico_db: -1.5 }))),
            ("nao_suportado: limitador", Box::new(|p| p.mixagem.limitador = true)),
            ("nao_suportado: abaixar", Box::new(|p| p.audios[0].abaixa_original = true)),
            (
                "nao_suportado: volume animado",
                Box::new(|p| {
                    p.audios[0].clipes[0].volume = serde_json::from_value(json!([{ "t": 0.0, "v": 1.0 }, { "t": 1.0, "v": 0.5 }])).unwrap()
                }),
            ),
            (
                "nao_suportado: silenciar",
                Box::new(|p| {
                    let Tolerante::Conhecido(ClipePrincipal::Trecho(t)) = &mut p.principal[0] else { panic!() };
                    t.audio.silencios = serde_json::from_value(json!([{ "id": "s", "de": 1.5, "ate": 2.0, "modo": "tom", "motivo": "" }])).unwrap();
                }),
            ),
            (
                "nao_suportado: volume animado",
                Box::new(|p| {
                    let Tolerante::Conhecido(ClipePrincipal::Trecho(t)) = &mut p.principal[0] else { panic!() };
                    t.audio.volume = serde_json::from_value(json!([{ "t": 1.0, "v": 1.0 }, { "t": 2.0, "v": 0.2 }])).unwrap();
                }),
            ),
            (
                "nao_suportado: transicao",
                Box::new(|p| {
                    let Tolerante::Conhecido(ClipePrincipal::Trecho(t)) = &mut p.principal[2] else { panic!() };
                    t.base.transicao_entrada.as_mut().unwrap().tipo = "fundir".into();
                }),
            ),
            ("nao_suportado: transicao", Box::new(|p| p.fecho = serde_json::from_value(json!({ "tipo": "fundir", "duracao_s": 0.5, "audio": "acompanhar" })).unwrap())),
            (
                "nao_suportado: rampa",
                Box::new(|p| {
                    let Tolerante::Conhecido(ClipePrincipal::Trecho(t)) = &mut p.principal[0] else { panic!() };
                    t.velocidade = serde_json::from_value(json!({ "tipo": "rampa", "pontos": [{ "t": 1.0, "v": 1.0 }, { "t": 2.0, "v": 0.5 }] })).unwrap();
                }),
            ),
            (
                "nao_suportado: video sobreposto",
                Box::new(|p| {
                    p.sobreposicoes = serde_json::from_value(json!([{ "id": "s", "nome": "", "oculta": false, "clipes": [{
                        "tipo": "video", "id": "sv", "rotulo": "", "pos": { "ref": "programa", "t": 0.0 }, "duracao_s": 1.0,
                        "transformacao": { "x": 0.5, "y": 0.5, "escala": 0.3, "rotacao": 0.0, "opacidade": 1.0 },
                        "entrada_s": 0.0, "saida_s": 0.0, "midia": "v1", "inicio_s": 0.0, "inicio_pts": null,
                        "velocidade": { "tipo": "constante", "fator": 1.0 },
                        "audio": { "volume": 1.0, "mudo": false, "manter_tom": true, "silencios": [] },
                        "moldura": null, "sincronia": null }] }]))
                    .unwrap();
                }),
            ),
            ("item_invalido: volume_original", Box::new(|p| p.mixagem.volume_original = f64::NAN)),
            ("midia_ausente: n1", Box::new(|p| p.audios[0].clipes[0].midia = "n1".into())),
        ];
        for (i, (esperado, mudar)) in casos.iter().enumerate() {
            let mut p = projeto_falso();
            mudar(&mut p);
            if i == casos.len() - 1 {
                let mut f = fontes.clone();
                f.remove("n1");
                let e = match montar_com(&p, &f, &cap, &final_(&p)) {
                    Err(SicroError::Validation(c)) => c,
                    outro => panic!("{outro:?}"),
                };
                assert_eq!(e, *esperado);
                continue;
            }
            assert_eq!(erro_de(&p), *esperado, "caso {i}");
        }
        // vídeo sobreposto com som em trilha OCULTA não entra no render: nada a recusar (a validação também não acusa)
        let mut p = projeto_falso();
        casos[casos.len() - 3].1(&mut p);
        assert_eq!(erro_de(&p), "nao_suportado: video sobreposto");
        p.sobreposicoes[0].oculta = true;
        assert!(montar_com(&p, &fontes, &cap, &final_(&p)).is_ok(), "trilha oculta recusada");
        // o mesmo sem efeito no som: clipe mudo com silêncio e trilha muda com abaixar passam
        let mut p = projeto_falso();
        let Tolerante::Conhecido(ClipePrincipal::Trecho(t)) = &mut p.principal[0] else { panic!() };
        t.audio.mudo = true;
        t.audio.silencios = serde_json::from_value(json!([{ "id": "s", "de": 1.5, "ate": 2.0, "modo": "mudo", "motivo": "" }])).unwrap();
        p.audios[0].abaixa_original = true;
        p.audios[0].mudo = true;
        let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        // v1 ainda entra pelo clipe "c"; n1 (trilha muda) não
        assert_eq!(passo.entradas.len(), 2);
        // chaves todas iguais = volume fixo
        let mut p = projeto_falso();
        p.audios[0].clipes[0].volume = serde_json::from_value(json!([{ "t": 0.0, "v": 0.5 }, { "t": 1.0, "v": 0.5 }])).unwrap();
        assert!(montar_com(&p, &fontes, &cap, &final_(&p)).unwrap().0.grafo.contains("volume=0.4"));
    }

    #[test]
    fn atempo_em_estagios_de_meio_a_dois() {
        let r = |n, d| Racional { num: n, den: d };
        assert_eq!(estagios_atempo(r(1, 2)), vec![0.5]);
        assert_eq!(estagios_atempo(r(1, 4)), vec![0.5, 0.5]);
        assert_eq!(estagios_atempo(r(1, 20)), vec![0.5, 0.5, 0.5, 0.5, 0.8]);
        assert_eq!(estagios_atempo(r(4, 1)), vec![2.0, 2.0]);
        assert_eq!(estagios_atempo(r(16, 1)), vec![2.0, 2.0, 2.0, 2.0]);
        assert_eq!(estagios_atempo(r(3, 1)), vec![2.0, 1.5]);
        assert_eq!(estagios_atempo(r(3, 4)), vec![0.75]);
        assert_eq!(estagios_atempo(r(1, 1)), Vec::<f64>::new());
        for mil in [50, 77, 333, 999, 1001, 2500, 16000] {
            let v = tempo::fator_racional(mil as f64 / 1000.0).unwrap();
            let e = estagios_atempo(v);
            assert!(e.iter().all(|f| (0.5..=2.0).contains(f)), "{mil}: {e:?}");
            assert!((e.iter().product::<f64>() - mil as f64 / 1000.0).abs() < 1e-12, "{mil}");
        }
        // exato: 48000·v inteiro
        let (Ritmo::Asetrate(hz), 0.0) = velocidade(&Velocidade::Constante { fator: 0.333 }, false).unwrap() else { panic!() };
        assert_eq!(hz, 15_984);
        assert!(matches!(velocidade(&Velocidade::Constante { fator: 1.0 }, true).unwrap(), (Ritmo::Normal, 0.0)));
    }

    #[test]
    fn ganho_e_marca() {
        let m = Loudnorm { integrado: Some(-10.46), pico: Some(-2.81), faixa: 3.0, limiar: Some(-20.5) };
        let alvo = Normalizar { lufs: -16.0, pico_db: -1.5 };
        assert!((ganho_db(&m, Some(&alvo)).unwrap() - -5.54).abs() < 1e-9);
        // limitado pelo pico
        let fala = Loudnorm { integrado: Some(-6.7), pico: Some(-3.2), ..m };
        assert!((ganho_db(&fala, Some(&Normalizar { lufs: -5.0, pico_db: -1.5 })).unwrap() - 1.7).abs() < 1e-9);
        assert_eq!(ganho_db(&m, None), None);
        assert_eq!(ganho_db(&Loudnorm { integrado: None, pico: None, faixa: 0.0, limiar: None }, Some(&alvo)), None);
        assert_eq!(ganho_db(&Loudnorm { integrado: Some(-75.0), ..m }, Some(&alvo)), None);
        let (passo, _) = montar_com(&projeto_falso(), &fontes_falsas(), &apoio::cap_falsa(), &final_(&projeto_falso())).unwrap();
        let g = grafo_com_ganho(&passo, Some(-5.54));
        assert!(g.contains("volume=-5.540000dB,") && !g.contains(MARCA_GANHO));
        assert!(grafo_com_ganho(&passo, None).contains("volume=0.000000dB,"));
        assert!(grafo_com_ganho(&passo, Some(f64::NAN)).contains("volume=0.000000dB,"));
    }

    #[test]
    fn le_o_json_do_loudnorm() {
        let stderr = "Input #0, wav, from 'x.wav':\n  Metadata:\n    title           : a\n[Parsed_loudnorm_0 @ 0x1] \n{\n\"input_i\" : \"1\"\n}\n\
            [out#0/null @ 0x2] video:0KiB\n[Parsed_loudnorm_3 @ 0x55d1] \n{\n\t\"input_i\" : \"-23.04\",\n\t\"input_tp\" : \"-inf\",\n\
            \t\"input_lra\" : \"0.00\",\n\t\"input_thresh\" : \"-33.04\",\n\t\"output_i\" : \"-16.00\",\n\t\"normalization_type\" : \"linear\",\n\
            \t\"target_offset\" : \"0.00\"\n}\n[aost#0:0/pcm_s16le @ 0x3] Terminating thread\n";
        assert_eq!(ler_loudnorm(stderr).unwrap(), Loudnorm { integrado: Some(-23.04), pico: None, faixa: 0.0, limiar: Some(-33.04) });
        // linha imitada no meio de outra (metadado) não conta; sem medição é erro
        assert!(ler_loudnorm("    title : [Parsed_loudnorm_0 @ 0x1]\n{\n\"input_i\" : \"-5\"\n}\n").is_err());
        assert!(ler_loudnorm("").is_err());
        assert!(ler_loudnorm("[Parsed_loudnorm_0 @ 0x1] \n{\n\"input_i\" : \n").is_err());
    }

    // ------------------------------------------------------------ com ffmpeg

    #[test]
    fn le_o_loudnorm_do_ffmpeg() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-loudnorm");
        for (som, silencio) in [("sine=f=1000:r=48000:d=3", false), ("anullsrc=r=48000:cl=stereo:d=3", true)] {
            let mut a = texto(&["-cpuflags", "0", "-hide_banner", "-nostdin", "-y", "-loglevel", "verbose", "-f", "lavfi", "-i", som]);
            a.extend(texto(&["-af", "loudnorm=I=-16:TP=-1.5:LRA=50:print_format=json", "-f", "null", "-"]));
            let cmd = comando(&cap, &pasta, "loudnorm", a);
            let t = exec::rodar(&cmd, &AtomicBool::new(false)).unwrap();
            exec::exigir_sucesso(&cmd, &t).unwrap();
            let m = ler_loudnorm(&std::fs::read_to_string(&cmd.stderr).unwrap()).unwrap();
            eprintln!("{som}: {m:?}");
            if silencio {
                assert_eq!((m.integrado, m.pico), (None, None));
            } else {
                // seno de 1 kHz a −18 dBFS (amplitude 1/8): cerca de −21 LUFS
                assert!((m.integrado.unwrap() - -21.0).abs() < 1.5 && (m.pico.unwrap() - -18.0).abs() < 1.0, "{m:?}");
            }
        }
    }

    #[test]
    fn amostras_exatas_e_cliques_no_lugar_em_tres_taxas() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-amostras");
        let som = format!("{SINAL}:s=48000:c=mono:d=12");
        let src = com_video(&cap, &pasta, "cliques.mkv", ("30", 12.0), &[], &som, &["-c:a", "pcm_s16le"]);
        let fonte = info(&cap, &src);
        let tab = fonte.quadros.clone();
        let fontes = HashMap::from([("v".to_string(), fonte)]);
        let casos = [(1.0, true), (0.5, true), (0.5, false), (2.0, true), (2.0, false), (4.0, true), (4.0, false)];
        for fps in [(30, 1), (30000, 1001), (25, 1)] {
            let principal: Vec<Value> =
                casos.iter().enumerate().flat_map(|(i, (f, tom))| [trecho(&format!("c{i}"), "v", 1.2, 3.8, *f, *tom), imagem(&format!("i{i}"), 0.3)]).collect();
            let p = projeto(fps, principal, vec![], vec![]);
            let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
            let pcm = rodar(&cap, &passo, &pasta.join(format!("{}-{}", fps.0, fps.1)));
            let fr = racional(fps);
            let total = tempo::amostra_do_slot(linha.total_q, fr);
            assert_eq!((pcm.n() as u64, passo.amostras), (total, total), "{fps:?} {}", cap.versao);
            let (l, r) = (pcm.canal(0), pcm.canal(1));
            assert_eq!(l, r, "mono vira estéreo igual");
            let mut maior = 0.0f64;
            for (k, c) in linha.clipes.iter().enumerate() {
                let (ini, fim) = (tempo::amostra_do_slot(c.inicio_q, fr) as usize, tempo::amostra_do_slot(c.inicio_q + c.n_q as u64, fr) as usize);
                if k % 2 == 1 {
                    assert!(l[ini..fim].iter().all(|&x| x == 0), "imagem em silêncio digital");
                    continue;
                }
                let (f, tom) = casos[k / 2];
                let atempo = tom && f != 1.0;
                let marcas: &[f64] = if atempo { &[2.0, 3.0] } else { &[1.5, 2.5, 3.5] };
                let esperados = cliques_no_programa(marcas, c, &tab, f, fr);
                assert_eq!(esperados.len(), marcas.len());
                let d = desvios(&l, c, &esperados, f, fr);
                let tol = if atempo { tolerancia_atempo_ms(tempo::fator_racional(f).unwrap()) * 48.0 } else { TOL_EXATO };
                assert!(d.iter().all(|x| x.abs() <= tol), "{fps:?} {f}× tom {tom}: desvios {d:?} (amostras), tolerância {tol}");
                if !atempo {
                    maior = maior.max(d.iter().fold(0.0, |m, x| m.max(x.abs())));
                }
            }
            eprintln!("{fps:?}: {total} amostras; maior desvio no 1× e no exato: {maior} amostra(s)");
        }
    }

    #[test]
    fn silencio_inicial_mantido_quando_o_som_comeca_depois() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-a9");
        let som = format!("{CLIQUES}:s=48000:c=mono:d=5");
        let src = com_video(&cap, &pasta, "atrasado.mkv", ("30", 6.0), &["-itsoffset", "1.0"], &som, &["-c:a", "pcm_s16le"]);
        let fonte = info(&cap, &src);
        assert_eq!((fonte.inicio_s, fonte.audio.as_ref().unwrap().inicio_s), (0.0, 1.0));
        let fontes = HashMap::from([("v".to_string(), fonte)]);
        let p = projeto((30, 1), vec![trecho("c", "v", 0.5, 2.5, 1.0, true)], vec![], vec![]);
        let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        let l = rodar(&cap, &passo, &pasta).canal(0);
        assert_eq!(l.len(), 96_000);
        // 0,5 s de silêncio digital antes do som da fonte; o clique de 1,5 s da fonte em 1,0 s do programa
        assert!(l[..24_000].iter().all(|&x| x == 0));
        assert!(l[24_000..24_100].iter().any(|&x| x != 0));
        assert_eq!(inicio_do_clique(&l, 0, l.len()), Some(48_000));
    }

    #[test]
    fn buraco_no_som_da_fonte_nao_desloca_o_resto() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-buraco");
        // `cm/19`: A-law 8 kHz, falta o som de 3,0 a 4,2 s
        let som = format!("{CLIQUES}:s=8000:d=10");
        let src = gerar(
            &cap,
            &pasta,
            "buraco.mkv",
            &["-f", "lavfi", "-i", &som, "-f", "lavfi", "-i", "color=c=gray:s=160x90:r=25:d=10", "-af", "aselect='not(between(t,3.0,4.2))'",
              "-c:a", "pcm_alaw", "-c:v", "libx264", "-preset", "ultrafast", "-shortest"],
        );
        let fontes = HashMap::from([("v".to_string(), info(&cap, &src))]);
        let p = projeto((25, 1), vec![trecho("c", "v", 1.0, 9.0, 1.0, true)], vec![], vec![]);
        let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        let l = rodar(&cap, &passo, &pasta).canal(0);
        assert_eq!(l.len(), 384_000);
        let mut d = Vec::new();
        for s in [1.5, 2.5, 4.5, 5.5, 6.5, 7.5, 8.5] {
            let e = ((s - 1.0) * 48_000.0) as usize;
            let i = inicio_do_clique(&l, e - 12_000, e + 12_000).unwrap_or_else(|| panic!("clique de {s} s"));
            d.push(i as i64 - e as i64);
        }
        assert!(d.iter().all(|x| x.abs() <= 12), "desvios (amostras) {d:?}");
        // o clique que caiu no buraco não existe; o buraco é silêncio
        assert_eq!(inicio_do_clique(&l, 2 * 48_000, 3 * 48_000 + 12_000), None);
        assert!(l[(2.1 * 48_000.0) as usize..(3.1 * 48_000.0) as usize].iter().all(|&x| x.abs() <= 1));
        eprintln!("desvios dos cliques depois do buraco: {d:?} amostra(s)");
    }

    #[test]
    fn duzentos_clipes_de_uma_midia_linha_curta_e_pcm_exato() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-200");
        let src = fontes::gerar(fontes::Fonte::Mp4).unwrap();
        let fonte = info(&cap, &src);
        let tab = fonte.quadros.clone();
        assert_eq!(fonte.audio.as_ref().map(|a| (a.taxa, a.canais)), Some((48_000, 2)));
        let principal: Vec<Value> = (0..200)
            .map(|i| {
                let j = (i * 37) % 440;
                let mut c = trecho(&format!("c{i}"), "v", tab.tempo(j), tab.tempo(j + 2), 1.0, true);
                c["inicio_pts"] = json!(tab.pts[j]);
                c["fim_pts"] = json!(tab.pts[j + 2]);
                c
            })
            .collect();
        let p = projeto((30, 1), principal, vec![], vec![]);
        let fontes_ = HashMap::from([("v".to_string(), fonte.clone())]);
        let (passo, linha) = montar_com(&p, &fontes_, &cap, &final_(&p)).unwrap();
        assert_eq!((linha.clipes.len(), passo.entradas.len()), (200, 1));
        // linha de comando com caminho de 150 caracteres, medida pelas regras do Windows
        let mut longo = passo.clone();
        longo.entradas[0].caminho = PathBuf::from(format!("C:\\Users\\perito\\{}\\video.mp4", "x".repeat(150 - 31)));
        let n = args::tamanho_da_linha(Path::new("C:\\Program Files\\SICRO\\ffmpeg\\ffmpeg.exe"), &argumentos(&longo, &cap, "audio.txt"));
        eprintln!("200 clipes: linha de {n} caracteres; grafo de {} bytes", passo.grafo.len());
        assert!(n < 8_000, "{n}");
        let pcm = rodar(&cap, &passo, &pasta);
        assert_eq!(pcm.n(), 400 * 1600);
        // referência: o som inteiro decodificado de uma vez; cada clipe é uma fatia exata dele
        let todo = rodar_pedaco(&cap, &pasta, "inteiro", Some(&src), "aformat=sample_fmts=fltp:channel_layouts=stereo", None);
        let inicio = fonte.audio.as_ref().unwrap().inicio_s;
        for (k, c) in linha.clipes.iter().enumerate() {
            let a0 = ((tab.tempo(c.fonte.as_ref().unwrap().j_in as usize) - inicio) * 48_000.0).round() as usize;
            let q = (k * 3200, (k + 1) * 3200);
            assert!(pcm.x[2 * q.0..2 * q.1] == todo[2 * a0..2 * (a0 + 3200)], "clipe {k}");
        }
    }

    #[test]
    fn transicao_escurecer_e_fecho_com_afade() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-escurecer");
        let src = com_video(&cap, &pasta, "tom.mkv", ("30", 8.0), &[], "aevalsrc=exprs=0.5:s=48000:c=mono:d=8", &["-c:a", "pcm_s16le"]);
        let fontes = HashMap::from([("v".to_string(), info(&cap, &src))]);
        let mut b = trecho("b", "v", 5.0, 7.0, 1.0, true);
        b["transicao_entrada"] = json!({ "tipo": "escurecer", "duracao_s": 0.5, "audio": "acompanhar" });
        let mut p = projeto((30, 1), vec![trecho("a", "v", 1.0, 3.0, 1.0, true), b], vec![], vec![]);
        p.fecho = serde_json::from_value(json!({ "tipo": "escurecer", "duracao_s": 0.2, "audio": "cortar" })).unwrap();
        let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        let l = rodar(&cap, &passo, &pasta).canal(0);
        let cheio = (0.5f64 * 32_768.0).round() as i32;
        let max = |s: &[i16]| s.iter().map(|x| x.unsigned_abs()).max().unwrap() as i32;
        assert_eq!(l.len(), 192_000);
        // A: 1,5 s cheio, depois desce em 0,5 s (24 000 amostras) até perto de zero
        assert_eq!(max(&l[0..72_000]), cheio);
        assert!(max(&l[96_000 - 10..96_000]) <= cheio / 2000 + 1);
        assert!(max(&l[72_000..72_100]) > cheio * 99 / 100);
        // B: sobe em 24 000 amostras
        assert!(max(&l[96_000..96_010]) <= cheio / 2000 + 1);
        assert_eq!(max(&l[120_000..191_520]), cheio);
        // fecho "cortar": afade de 10 ms (480 amostras) no fim, não de 0,2 s
        assert!(max(&l[192_000 - 10..]) <= cheio / 40);
        assert_eq!(max(&l[191_000..191_520]), cheio);
    }

    #[test]
    fn narracao_no_instante_certo_e_presa_acompanha_a_reordenacao() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-narracao");
        // narração mono com cliques em 0,25 s e 1,25 s
        let n = gerar(&cap, &pasta, "narr.wav", &["-f", "lavfi", "-i", "aevalsrc=exprs=0.9*lt(mod(t+0.75\\,1)\\,0.001):s=48000:c=mono:d=2", "-c:a", "pcm_s16le"]);
        let fontes = HashMap::from([("n".to_string(), info_som(&cap, &n))]);
        let cheio = (0.9f64 * 32_768.0).round() as u16;
        // no programa, fora do slot, nas duas taxas; recorte começando em 0,2 s; volume da trilha 0,5
        for (fps, t, de, ate) in [((30, 1), 1.37, 0.0, None), ((30000, 1001), 1.37, 0.2, Some(2.0))] {
            let c = clipe_audio("x", "n", json!({ "ref": "programa", "t": t }), de, ate);
            let p = projeto(fps, vec![imagem("i", 6.0)], vec![trilha("t", 0.5, vec![c])], vec![midia_ref("n", 2.0)]);
            let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
            let pcm = rodar(&cap, &passo, &pasta.join(format!("p{}", fps.0)));
            let (l, r) = (pcm.canal(0), pcm.canal(1));
            assert_eq!(l, r);
            let q = linha.livres[0].inicio_q;
            assert_eq!(q, (t * fps.0 as f64 / fps.1 as f64).round() as u64);
            let e = tempo::amostra_do_slot(q, racional(fps)) as usize + ((0.25 - de) * 48_000.0f64).round() as usize;
            let achados: Vec<usize> = (0..l.len()).filter(|&i| l[i] != 0 && (i == 0 || l[i - 1] == 0)).collect();
            assert_eq!(achados, vec![e, e + 48_000], "{fps:?}");
            assert_eq!(l[e].unsigned_abs(), (cheio as f64 * 0.5).round() as u16);
        }
        // presa ao clipe "b" com deslocamento de 0,5 s: segue "b" quando a ordem muda
        let c = clipe_audio("x", "n", json!({ "ref": "clipe", "clipe": "b", "desloc": 0.5 }), 0.0, Some(2.0));
        let mut achados = Vec::new();
        for ordem in [["a", "b"], ["b", "a"]] {
            let dur = |id: &str| if id == "a" { 2.0 } else { 3.0 };
            let p = projeto((30, 1), ordem.iter().map(|id| imagem(id, dur(id))).collect(), vec![trilha("t", 1.0, vec![c.clone()])], vec![]);
            let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
            let l = rodar(&cap, &passo, &pasta.join(ordem.concat())).canal(0);
            let b = linha.clipes.iter().find(|c| c.id == "b").unwrap().inicio_q;
            let e = tempo::amostra_do_slot(b + 15, racional((30, 1))) as usize + 12_000;
            assert_eq!(inicio_do_clique(&l, 0, l.len()), Some(e), "{ordem:?}");
            achados.push(e);
        }
        assert_eq!(achados, vec![96_000 + 24_000 + 12_000, 24_000 + 12_000]);
    }

    #[test]
    fn previa_e_a_fatia_do_render_final() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-previa");
        let som = format!("{CLIQUES}:s=48000:c=stereo:d=12");
        let src = com_video(&cap, &pasta, "cliques.mkv", ("30", 12.0), &[], &som, &["-c:a", "aac", "-b:a", "128k"]);
        let n = gerar(&cap, &pasta, "narr.wav", &["-f", "lavfi", "-i", "sine=f=700:r=48000:d=3", "-c:a", "pcm_s16le"]);
        let fontes = HashMap::from([("v".to_string(), info(&cap, &src)), ("n".to_string(), info_som(&cap, &n))]);
        let p = projeto(
            (30000, 1001),
            vec![trecho("a", "v", 1.0, 3.0, 1.0, true), imagem("i", 0.7), trecho("b", "v", 6.0, 9.0, 2.0, true), trecho("c", "v", 0.2, 1.4, 0.5, false)],
            vec![trilha("t", 0.6, vec![clipe_audio("x", "n", json!({ "ref": "clipe", "clipe": "b", "desloc": 0.3 }), 0.5, Some(2.5))])],
            vec![],
        );
        let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        let todo = rodar(&cap, &passo, &pasta.join("final"));
        let a = |q| tempo::amostra_do_slot(q, linha.fps) as usize;
        for (de_q, ate_q) in [(0, 20), (37, 101), (50, linha.total_q)] {
            let (previa, _) = montar_com(&p, &fontes, &cap, &Alvo::Previa { altura: 360, de_q, ate_q }).unwrap();
            let x = rodar(&cap, &previa, &pasta.join(format!("previa{de_q}")));
            assert!(x.x == todo.x[2 * a(de_q)..2 * a(ate_q)], "janela {de_q}..{ate_q}");
        }
    }

    #[test]
    fn dois_binarios_e_render_sem_busca_no_aac_mono_de_gravador() {
        let Some(bins) = dois_binarios() else { return };
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg dos testes {}; comparados {} e {}", cap.versao, bins[0].versao, bins[1].versao);
        let pasta = fontes::pasta_temporaria("audio-gravador");
        // AAC mono 64 kbit/s (`receitas-gpu-ffmpeg.md` §3.4: 6 % das amostras mudavam depois de `-ss`)
        let som = "aevalsrc=exprs=0.3*sin(2*PI*440*t)*sin(2*PI*0.5*t)+0.2*sin(2*PI*1234*t+3*sin(2*PI*3*t))+0.9*lt(mod(t+0.5\\,1)\\,0.001):s=48000:c=mono:d=30";
        let src = com_video(&cap, &pasta, "gravador.mp4", ("15", 30.0), &[], som, &["-c:a", "aac", "-b:a", "64k", "-ac", "1"]);
        let fontes = HashMap::from([("v".to_string(), info(&cap, &src))]);
        let mut c = trecho("c", "v", 12.4, 15.0, 1.0, true);
        c["transicao_entrada"] = json!({ "tipo": "escurecer", "duracao_s": 0.4, "audio": "acompanhar" });
        let p = projeto(
            (30, 1),
            vec![
                trecho("a", "v", 0.0, 2.8, 1.0, true),
                trecho("b", "v", 10.2, 14.6, 1.0, true),
                imagem("i", 0.5),
                c,
                trecho("d", "v", 20.0, 23.0, 2.0, true),
                trecho("e", "v", 25.0, 27.0, 0.5, false),
            ],
            vec![],
            vec![],
        );
        let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        let pedacos = pedacos_do_grafo(&passo.grafo);
        assert_eq!(pedacos.len(), 6);
        let mut hashes = Vec::new();
        for b in &bins {
            let pb = pasta.join(&b.versao);
            let pcm = rodar(b, &passo, &pb);
            // referência: cada pedaço num ffmpeg só, lendo do começo
            let mut refe = Vec::new();
            let mut diferentes_com_busca = 0usize;
            for (k, (usa, cadeia)) in pedacos.iter().enumerate() {
                let sem = rodar_pedaco(b, &pb, &format!("r{k}"), usa.then_some(src.as_path()), cadeia, None);
                if *usa {
                    let t0: f64 = cadeia["atrim=start=".len()..].split([':', ',']).next().unwrap().parse().unwrap();
                    let com = rodar_pedaco(b, &pb, &format!("s{k}"), Some(&src), cadeia, Some((t0 - 1.0).max(0.0)));
                    diferentes_com_busca += sem.as_chunks::<2>().0.iter().zip(com.as_chunks::<2>().0).filter(|(x, y)| x != y).count();
                }
                refe.extend(sem);
            }
            assert!(pcm.x == refe, "{}: PCM do passo × render sem busca", b.versao);
            eprintln!("{}: {} amostras, SHA-256 {}; com -ss antes do -i: {diferentes_com_busca} amostras diferentes", b.versao, pcm.n(), pcm.sha);
            hashes.push(pcm.sha);
        }
        assert_eq!(hashes[0], hashes[1], "PCM igual nos dois binários");
    }

    #[test]
    fn inicio_de_uma_hora_tempo_absoluto() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-inicio");
        let som = format!("{CLIQUES}:s=48000:c=mono:d=8");
        let src = com_video(&cap, &pasta, "inicio.mkv", ("30", 8.0), &[], &som, &["-c:a", "pcm_s16le", "-output_ts_offset", "3600"]);
        let fonte = info(&cap, &src);
        assert_eq!((fonte.inicio_s, fonte.audio.as_ref().unwrap().inicio_s), (3600.0, 3600.0));
        let tab = fonte.quadros.clone();
        let fontes = HashMap::from([("v".to_string(), fonte)]);
        let casos = [(1.0, true), (0.5, false)];
        let principal = casos.iter().enumerate().map(|(i, (f, tom))| trecho(&format!("c{i}"), "v", 3602.2, 3604.8, *f, *tom)).collect();
        let p = projeto((30, 1), principal, vec![], vec![]);
        let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        assert!(passo.grafo.contains("atrim=start=3602.200000:end=3604.800000,asetpts=PTS-3602.200000/TB"), "{}", passo.grafo);
        let l = rodar(&cap, &passo, &pasta).canal(0);
        let fr = racional((30, 1));
        assert_eq!(l.len() as u64, tempo::amostra_do_slot(linha.total_q, fr));
        for (c, (f, _)) in linha.clipes.iter().zip(casos) {
            let e = cliques_no_programa(&[3602.5, 3603.5, 3604.5], c, &tab, f, fr);
            let d = desvios(&l, c, &e, f, fr);
            assert!(d.iter().all(|x| x.abs() <= TOL_EXATO), "{f}×: {d:?}");
        }
    }

    #[test]
    fn seis_canais_viram_estereo_e_fades_da_trilha() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-51");
        // só o canal central tem som
        let n = gerar(&cap, &pasta, "51.wav", &["-f", "lavfi", "-i", "aevalsrc=exprs=0|0|0.5*sin(2*PI*440*t)|0|0|0:c=5.1:s=48000:d=2", "-c:a", "pcm_s16le"]);
        let fonte = info_som(&cap, &n);
        assert_eq!(fonte.audio.as_ref().unwrap().canais, 6);
        let fontes = HashMap::from([("n".to_string(), fonte)]);
        let mut c = clipe_audio("x", "n", json!({ "ref": "programa", "t": 0.0 }), 0.0, Some(2.0));
        c["entrada_s"] = json!(0.25);
        c["saida_s"] = json!(0.5);
        let p = projeto((30, 1), vec![imagem("i", 2.0)], vec![trilha("t", 1.0, vec![c])], vec![]);
        let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        assert!(passo.grafo.contains("afade=t=in:ss=0:ns=12000,afade=t=out:ss=72000:ns=24000"), "{}", passo.grafo);
        let pcm = rodar(&cap, &passo, &pasta);
        let (l, r) = (pcm.canal(0), pcm.canal(1));
        assert_eq!(l, r, "o centro vai igual aos dois lados");
        let max = |s: &[i16]| s.iter().map(|x| x.unsigned_abs()).max().unwrap() as f64 / 32_768.0;
        // fora das rampas: centro a −3 dB (0,5 × 0,707) nos dois lados
        let meio = max(&l[12_000..72_000]);
        assert!((meio - 0.5 * std::f64::consts::FRAC_1_SQRT_2).abs() < 0.01, "{meio}");
        // rampas: sobe em 0,25 s, desce em 0,5 s
        assert!(max(&l[..120]) <= 0.011 * meio && max(&l[96_000 - 240..]) <= 0.011 * meio);
        assert!((max(&l[5_000..7_000]) / meio - 0.5).abs() < 0.1);
    }

    /// Fontes da O6 (4 s, mesmo sinal): AAC, MP3, Opus e G.711 A-law.
    fn fontes_o6(cap: &Capacidades, pasta: &Path) -> Vec<(&'static str, PathBuf)> {
        let som = "aevalsrc=exprs=0.4*sin(2*PI*440*t)+0.2*sin(2*PI*3311*t)*sin(2*PI*0.7*t)+0.1*sin(2*PI*97*t)+0.5*lt(mod(t+0.5\\,1)\\,0.001)";
        let c = |taxa: u32, canais: &str| format!("{som}:s={taxa}:c={canais}:d=4");
        vec![
            ("AAC", gerar(cap, pasta, "o6.m4a", &["-f", "lavfi", "-i", &c(48_000, "stereo"), "-c:a", "aac", "-b:a", "128k"])),
            ("MP3", gerar(cap, pasta, "o6.mp3", &["-f", "lavfi", "-i", &c(44_100, "stereo"), "-c:a", "libmp3lame", "-b:a", "128k"])),
            ("Opus", gerar(cap, pasta, "o6.opus", &["-f", "lavfi", "-i", &c(48_000, "stereo"), "-c:a", "libopus", "-b:a", "64k"])),
            ("G.711", gerar(cap, pasta, "o6.wav", &["-f", "lavfi", "-i", &c(8_000, "mono"), "-c:a", "pcm_alaw"])),
        ]
    }

    /// Projeto com o arquivo como narração sobre 4 s de imagem.
    fn projeto_narrado(dur: f64) -> ProjetoEdicao {
        let c = clipe_audio("x", "n", json!({ "ref": "programa", "t": 0.0 }), 0.0, Some(dur));
        projeto((30, 1), vec![imagem("i", dur)], vec![trilha("t", 1.0, vec![c])], vec![])
    }

    #[test]
    fn o6_cpuflags_zero_bate_nos_dois_binarios() {
        let Some(bins) = dois_binarios() else { return };
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg dos testes {}; comparados {} e {}", cap.versao, bins[0].versao, bins[1].versao);
        let pasta = fontes::pasta_temporaria("audio-o6");
        let padrao: Vec<String> = args::globais();
        for (codec, arquivo) in fontes_o6(&cap, &pasta) {
            let fontes = HashMap::from([("n".to_string(), info_som(&cap, &arquivo))]);
            let p = projeto_narrado(4.0);
            let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
            let mut h = Vec::new();
            for b in &bins {
                for (k, g) in [("0", None), ("padrão", Some(padrao.clone()))] {
                    let (pcm, _) = rodar_com(b, &passo, &pasta.join(format!("{codec}-{}-{k}", b.versao)), g);
                    assert_eq!(pcm.n(), 192_000);
                    assert!(pcm.x.iter().any(|&x| x != 0));
                    h.push((b.versao.clone(), k, pcm.sha));
                }
            }
            let curto = |s: &str| s[..16].to_string();
            eprintln!(
                "O6 {codec}: {} cpuflags 0 {} · padrão {} | {} cpuflags 0 {} · padrão {}",
                h[0].0, curto(&h[0].2), curto(&h[1].2), h[2].0, curto(&h[2].2), curto(&h[3].2)
            );
            assert_eq!(h[0].2, h[2].2, "{codec}: -cpuflags 0 nos dois binários");
        }
    }

    /// Custo do `-cpuflags 0` (O6): 10 min de AAC, MP3 e G.711, melhor de 3 em cada binário. Mede tempo: rodar com a máquina livre.
    #[test]
    #[ignore]
    fn o6_custo_de_tempo() {
        let Some(bins) = dois_binarios() else { return };
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("audio-o6-custo");
        let dur = 600.0;
        let arquivos = [
            ("AAC estéreo 128k", gerar(&cap, &pasta, "longo.m4a", &["-f", "lavfi", "-i", "sine=f=440:r=48000:d=600", "-ac", "2", "-c:a", "aac", "-b:a", "128k"])),
            ("AAC mono 64k", gerar(&cap, &pasta, "longo-mono.m4a", &["-f", "lavfi", "-i", "sine=f=440:r=48000:d=600", "-c:a", "aac", "-b:a", "64k"])),
            ("MP3 44,1 kHz", gerar(&cap, &pasta, "longo.mp3", &["-f", "lavfi", "-i", "sine=f=440:r=44100:d=600", "-ac", "2", "-c:a", "libmp3lame", "-b:a", "128k"])),
            ("G.711 8 kHz", gerar(&cap, &pasta, "longo.wav", &["-f", "lavfi", "-i", "sine=f=440:r=8000:d=600", "-c:a", "pcm_alaw"])),
        ];
        for (nome, arquivo) in &arquivos {
            let fontes = HashMap::from([("n".to_string(), info_som(&cap, arquivo))]);
            let p = projeto_narrado(dur);
            let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
            for b in &bins {
                let melhor = |g: Option<Vec<String>>| {
                    (0..3)
                        .map(|i| {
                            let (pcm, s) = rodar_com(b, &passo, &pasta.join(format!("{}-{i}", b.versao)), g.clone());
                            assert_eq!(pcm.n() as u64, passo.amostras);
                            s
                        })
                        .fold(f64::MAX, f64::min)
                };
                let (zero, padrao) = (melhor(None), melhor(Some(args::globais())));
                eprintln!("O6 custo {nome} {dur} s, {}: cpuflags 0 {zero:.2} s, padrão {padrao:.2} s (×{:.2})", b.versao, zero / padrao);
            }
        }
    }

    #[test]
    fn atempo_desvio_medido_cabe_na_tolerancia() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-atempo");
        let som = format!("{SINAL}:s=48000:c=mono:d=6");
        let src = com_video(&cap, &pasta, "cliques.mkv", ("30", 6.0), &[], &som, &["-c:a", "pcm_s16le"]);
        let fonte = info(&cap, &src);
        let tab = fonte.quadros.clone();
        let fontes = HashMap::from([("v".to_string(), fonte)]);
        let fatores = [0.05, 0.1, 0.25, 0.4, 0.5, 0.75, 0.9, 1.25, 1.5, 2.0, 3.0, 4.0, 8.0, 16.0];
        let principal: Vec<Value> = fatores.iter().enumerate().map(|(i, f)| trecho(&format!("c{i}"), "v", 0.8, 4.2, *f, true)).collect();
        let p = projeto((30, 1), principal, vec![], vec![]);
        let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        let l = rodar(&cap, &passo, &pasta).canal(0);
        let fr = racional((30, 1));
        let mut maior = 0.0f64;
        for (c, f) in linha.clipes.iter().zip(fatores) {
            let d = desvios(&l, c, &cliques_no_programa(&[1.0, 2.0, 3.0, 4.0], c, &tab, f, fr), f, fr);
            let medido = d.iter().fold(0.0f64, |m, x| m.max(x.abs())) / 48.0;
            let tol = tolerancia_atempo_ms(tempo::fator_racional(f).unwrap());
            eprintln!("atempo {f}×: desvio máximo {medido:.2} ms (tolerância declarada {tol:.1} ms) {d:?}");
            assert!(medido <= tol, "{f}×: {medido} ms > {tol} ms");
            maior = maior.max(tol);
        }
        assert_eq!(passo.tolerancia_atempo_ms, maior);
    }

    // ------------------------------------------------------------ revisão: bordas

    /// `montar` com outro teto de espera.
    fn montar_teto(p: &ProjetoEdicao, fontes: &HashMap<String, FonteInfo>, cap: &Capacidades, teto: f64) -> PassoAudio {
        let (leitor, pasta, midias) = (apoio::SemRastreios, PathBuf::new(), Vec::<EstadoMidia>::new());
        let ctx = Contexto { fontes, cap, rastreios: &leitor, pasta_projeto: &pasta, midias: &midias };
        let linha = tempo::resolver(p, &ctx);
        montar_com_teto(p, &linha, &ctx, &final_(p), teto).unwrap().0.expect("passo")
    }

    /// Fonte sintética de `seg` segundos a 30 qps, com som.
    fn fonte_longa(seg: usize) -> HashMap<String, FonteInfo> {
        let tab = apoio::tabela((1, 15360), apoio::pts_regulares(seg * 30, (1, 30), (1, 15360), 0));
        let mut i = apoio::fonte(tab, (320, 180));
        i.caminho = PathBuf::from(format!("C:\\Users\\perito\\{}\\video.mp4", "x".repeat(150 - 31)));
        i.audio = Some(AudioInfo { inicio_s: 0.0, taxa: 48000, canais: 2 });
        HashMap::from([("v".to_string(), i)])
    }

    fn caminhos(passo: &PassoAudio) -> Vec<&Path> {
        passo.entradas.iter().map(|e| e.caminho.as_path()).collect()
    }

    #[test]
    fn som_que_esperaria_demais_ganha_outra_entrada() {
        let cap = apoio::cap_falsa();
        let fontes = fonte_longa(3000);
        let t = |id: &str, de: f64, ate: f64| trecho(id, "v", de, ate, 1.0, true);
        let passo_de = |principal: Vec<Value>| {
            let p = projeto((30, 1), principal, vec![], vec![]);
            montar_com(&p, &fontes, &cap, &final_(&p)).unwrap().0
        };
        // em ordem, ou esperando pouco: uma entrada
        assert_eq!(passo_de(vec![t("a", 0.0, 400.0), t("b", 400.0, 410.0)]).entradas.len(), 1);
        assert_eq!(passo_de(vec![t("a", 400.0, 410.0), t("b", 300.0, 399.0)]).entradas.len(), 1);
        // o trecho de destaque e depois a gravação inteira: 400 s esperando, segunda entrada da mesma mídia
        let passo = passo_de(vec![t("a", 400.0, 410.0), t("b", 0.0, 400.0), t("c", 410.0, 420.0)]);
        assert_eq!(passo.entradas.len(), 2);
        assert_eq!(passo.entradas[0], passo.entradas[1]);
        assert!(passo.grafo.contains("[0:a:0]asplit=2[m0u0][m0u1];\n[1:a:0]asplit=1[m1u0];"), "{}", passo.grafo);
        assert!(passo.grafo.contains("[m1u0]atrim=start=0.000000:end=400.000000"), "{}", passo.grafo);
        // a espera soma: 99 s cabem, mais 100 s não
        assert_eq!(passo_de(vec![t("a", 400.0, 410.0), t("b", 300.0, 399.0), t("c", 0.0, 100.0)]).entradas.len(), 2);
        // a câmera lenta espera o dobro
        let lento = |de: f64, ate: f64| trecho("b", "v", de, ate, 0.5, false);
        assert_eq!(passo_de(vec![t("a", 400.0, 410.0), lento(300.0, 359.0)]).entradas.len(), 1);
        assert_eq!(passo_de(vec![t("a", 400.0, 410.0), lento(300.0, 361.0)]).entradas.len(), 2);
        // 20 trechos de 130 s de trás para a frente: as entradas param no teto e a linha continua curta
        let inverso: Vec<Value> = (0..20).rev().map(|i| t(&format!("c{i}"), i as f64 * 130.0, (i + 1) as f64 * 130.0)).collect();
        let passo = passo_de(inverso);
        assert_eq!(passo.entradas.len(), 1 + MAX_ENTRADAS_EXTRAS);
        let n = args::tamanho_da_linha(Path::new("C:\\Program Files\\SICRO\\ffmpeg\\ffmpeg.exe"), &argumentos(&passo, &cap, "audio.txt"));
        assert!(n < 8_000, "{n}");
        assert_eq!(passo.grafo.matches("asplit=").count(), 1 + MAX_ENTRADAS_EXTRAS + 1);
    }

    #[test]
    fn grafo_cresce_linear_e_a_linha_nao() {
        let cap = apoio::cap_falsa();
        let fontes = fonte_longa(30);
        let medir = |n: usize| {
            let principal: Vec<Value> = (0..n).map(|i| trecho(&format!("c{i}"), "v", ((i * 37) % 440) as f64 / 30.0, ((i * 37) % 440 + 2) as f64 / 30.0, 1.0, true)).collect();
            let p = projeto((30, 1), principal, vec![], vec![]);
            let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
            assert_eq!(passo.entradas.len(), 1);
            (passo.grafo.len(), argumentos(&passo, &cap, "audio.txt"))
        };
        let (g100, l100) = medir(100);
        let (g200, l200) = medir(200);
        let (g400, l400) = medir(400);
        assert!(l100 == l200 && l200 == l400);
        // por clipe: a cadeia, o rótulo no `asplit` e no `concat`
        let (d1, d2) = ((g200 - g100) as f64 / 100.0, (g400 - g200) as f64 / 200.0);
        assert!((d2 / d1 - 1.0).abs() < 0.02 && d2 < 300.0, "{d1} e {d2} bytes por clipe");
    }

    #[test]
    fn duas_entradas_da_mesma_midia_dao_o_mesmo_pcm() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-entradas");
        let som = format!("{SINAL}:s=44100:c=stereo:d=12");
        let src = com_video(&cap, &pasta, "cliques.mp4", ("30", 12.0), &[], &som, &["-c:a", "aac", "-b:a", "96k"]);
        let fontes = HashMap::from([("v".to_string(), info(&cap, &src))]);
        let p = projeto(
            (30000, 1001),
            vec![trecho("a", "v", 8.0, 11.0, 1.0, true), trecho("b", "v", 1.0, 7.0, 0.5, false), trecho("c", "v", 7.0, 8.0, 2.0, true)],
            vec![trilha("t", 0.5, vec![clipe_audio("x", "v", json!({ "ref": "programa", "t": 0.2 }), 2.0, Some(5.0))])],
            vec![],
        );
        let uma = montar_teto(&p, &fontes, &cap, f64::INFINITY);
        let varias = montar_teto(&p, &fontes, &cap, 1.0);
        assert_eq!((uma.entradas.len(), varias.entradas.len()), (1, 3));
        assert_eq!(uma.amostras, varias.amostras);
        let (a, b) = (rodar(&cap, &uma, &pasta.join("uma")), rodar(&cap, &varias, &pasta.join("varias")));
        assert_eq!(a.n() as u64, uma.amostras);
        assert!(a.x.iter().any(|&x| x.unsigned_abs() > LIMIAR));
        assert_eq!(a.sha, b.sha);
    }

    /// Conta por fora do módulo: bytes do arquivo e ffprobe.
    #[test]
    fn conta_por_fora_em_2997_tres_clipes_a_meia_velocidade() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-por-fora");
        let som = format!("{SINAL}:s=48000:c=mono:d=12");
        // fonte a 29,97 em MKV (base de 1 ms)
        let src = com_video(&cap, &pasta, "ntsc.mkv", ("30000/1001", 12.0), &[], &som, &["-c:a", "pcm_s16le"]);
        let fontes = HashMap::from([("v".to_string(), info(&cap, &src))]);
        let p = projeto(
            (30000, 1001),
            vec![trecho("a", "v", 1.2, 3.8, 0.5, true), trecho("b", "v", 5.0, 6.3, 0.5, false), trecho("c", "v", 7.1, 9.9, 0.5, false)],
            vec![],
            vec![],
        );
        let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        // a 0,5× na própria taxa cada quadro da fonte ocupa 2 slots
        let quadros: u64 = linha.clipes.iter().map(|c| c.fonte.as_ref().map_or(0, |f| (f.j_out - f.j_in) as u64)).sum();
        assert_eq!((linha.clipes.len(), linha.total_q), (3, 2 * quadros));
        let esperado = (linha.total_q as f64 * 48_000.0 * 1001.0 / 30_000.0).round() as u64;
        let pcm = rodar(&cap, &passo, &pasta);
        assert_eq!((passo.amostras, pcm.n() as u64), (esperado, esperado));
        let arquivo = pasta.join(&passo.saida);
        let bytes = std::fs::metadata(&arquivo).unwrap().len();
        let o = std::process::Command::new(&cap.ffprobe)
            .args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=codec_name,sample_rate,channels,duration_ts", "-of", "default=nw=1"])
            .arg(&arquivo)
            .output()
            .expect("ffprobe");
        let texto = String::from_utf8_lossy(&o.stdout);
        let campo = |k: &str| texto.lines().find_map(|l| l.strip_prefix(k).and_then(|l| l.strip_prefix('='))).unwrap_or_else(|| panic!("{k}: {texto}")).to_string();
        eprintln!("29,97, 3 clipes a 0,5×: {} slots, {esperado} amostras; arquivo de {bytes} bytes; ffprobe duration_ts {}", linha.total_q, campo("duration_ts"));
        assert_eq!((campo("codec_name").as_str(), campo("sample_rate").as_str(), campo("channels").as_str()), ("pcm_s16le", "48000", "2"));
        assert_eq!(campo("duration_ts").parse::<u64>().unwrap(), esperado);
        // cabeçalho de 44 bytes (RIFF, fmt de 16, data): `bitexact` não grava LIST
        assert_eq!(bytes, 44 + 4 * esperado);
        // os dois clipes exatos: o clique de x,5 s da fonte no lugar
        let l = pcm.canal(0);
        let fr = racional((30000, 1001));
        for (c, marcas) in linha.clipes.iter().zip([&[][..], &[5.5][..], &[7.5, 8.5, 9.5][..]]).skip(1) {
            let d = desvios(&l, c, &cliques_no_programa(marcas, c, &fontes["v"].quadros, 0.5, fr), 0.5, fr);
            assert!(d.iter().all(|x| x.abs() <= TOL_EXATO), "{d:?}");
        }
    }

    #[test]
    fn fonte_sem_som_clipe_mudo_volume_e_g711_na_principal() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        // caminho com espaço, acento, vírgula e apóstrofo
        let pasta = fontes::pasta_temporaria("audio-bordas").join("perícia nº 7, câmera d'água [2]");
        std::fs::create_dir_all(&pasta).unwrap();
        let som = format!("{CLIQUES}:s=48000:c=mono:d=6");
        let v1 = com_video(&cap, &pasta, "com som.mkv", ("30", 6.0), &[], &som, &["-c:a", "pcm_s16le"]);
        let v2 = gerar(&cap, &pasta, "sem som.mkv", &["-f", "lavfi", "-i", "testsrc2=s=160x90:r=30:d=4", "-c:v", "libx264", "-preset", "ultrafast"]);
        let som8 = format!("{CLIQUES}:s=8000:d=6");
        let v3 = gerar(
            &cap,
            &pasta,
            "g711 8 kHz.mkv",
            &["-f", "lavfi", "-i", &som8, "-f", "lavfi", "-i", "color=c=gray:s=160x90:r=25:d=6", "-c:a", "pcm_mulaw", "-c:v", "libx264", "-preset", "ultrafast", "-shortest"],
        );
        let (f1, f2, f3) = (info(&cap, &v1), info(&cap, &v2), info(&cap, &v3));
        assert!(f2.audio.is_none());
        assert_eq!(f3.audio.as_ref().map(|a| (a.taxa, a.canais)), Some((8_000, 1)));
        let fontes = HashMap::from([("v1".to_string(), f1), ("v2".to_string(), f2), ("v3".to_string(), f3)]);
        let mut mudo = trecho("c", "v1", 3.0, 5.0, 1.0, true);
        mudo["audio"]["mudo"] = json!(true);
        let mut baixo = trecho("e", "v1", 1.0, 3.0, 1.0, true);
        baixo["audio"]["volume"] = json!(0.5);
        let p = projeto(
            (30, 1),
            vec![trecho("a", "v1", 1.0, 3.0, 1.0, true), trecho("b", "v2", 0.5, 2.5, 1.0, true), mudo, trecho("d", "v3", 1.0, 4.0, 1.0, true), baixo],
            vec![],
            vec![],
        );
        let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        // a mídia sem som não é aberta
        assert_eq!(caminhos(&passo), vec![v1.as_path(), v3.as_path()]);
        let pcm = rodar(&cap, &passo, &pasta.join("saída"));
        let l = pcm.canal(0);
        assert_eq!(l, pcm.canal(1));
        assert_eq!(l.len(), 11 * 48_000);
        let faixa = |k: usize| {
            let c = &linha.clipes[k];
            let a = |q| tempo::amostra_do_slot(q, linha.fps) as usize;
            (a(c.inicio_q), a(c.inicio_q + c.n_q as u64))
        };
        let pico = |k: usize| l[faixa(k).0..faixa(k).1].iter().map(|x| x.unsigned_abs()).max().unwrap() as f64 / 32_768.0;
        // a: clique de 1,5 s da fonte em 0,5 s do clipe
        assert_eq!(inicio_do_clique(&l, faixa(0).0, faixa(0).1), Some(24_000));
        assert!((0.88..0.93).contains(&pico(0)), "{}", pico(0));
        // b (fonte sem som) e c (mudo): silêncio digital
        assert!(l[faixa(1).0..faixa(2).1].iter().all(|&x| x == 0));
        // d: G.711 mono a 8 kHz, cliques de 1,5, 2,5 e 3,5 s
        for s in [0.5, 1.5, 2.5] {
            let e = faixa(3).0 + (s * 48_000.0) as usize;
            let i = inicio_do_clique(&l, e - 12_000, e + 12_000).unwrap_or_else(|| panic!("clique de {s} s"));
            assert!((i as i64 - e as i64).abs() <= 12, "{s}: {}", i as i64 - e as i64);
        }
        // e: volume 0,5
        assert!((pico(4) / pico(0) - 0.5).abs() < 0.005, "{} × {}", pico(4), pico(0));
        assert_eq!(inicio_do_clique(&l, faixa(4).0, faixa(4).1), Some(faixa(4).0 + 24_000));
    }

    #[test]
    fn seis_canais_na_principal_com_velocidade() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-51-principal");
        // só o canal da frente à esquerda tem som
        let src = com_video(&cap, &pasta, "51.mkv", ("30", 6.0), &[], "aevalsrc=exprs=0.5*sin(2*PI*440*t)|0|0|0|0|0:c=5.1:s=48000:d=6", &["-c:a", "pcm_s16le"]);
        let fonte = info(&cap, &src);
        assert_eq!(fonte.audio.as_ref().unwrap().canais, 6);
        let fontes = HashMap::from([("v".to_string(), fonte)]);
        let p = projeto(
            (30, 1),
            vec![trecho("a", "v", 1.0, 3.0, 1.0, true), trecho("b", "v", 1.0, 3.0, 0.5, false), trecho("c", "v", 1.0, 5.0, 2.0, true)],
            vec![],
            vec![],
        );
        let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        let pcm = rodar(&cap, &passo, &pasta);
        assert_eq!(pcm.n() as u64, tempo::amostra_do_slot(linha.total_q, linha.fps));
        assert_eq!(pcm.n(), 8 * 48_000);
        let (l, r) = (pcm.canal(0), pcm.canal(1));
        assert!(r.iter().all(|x| x.unsigned_abs() <= 1), "a direita fica muda");
        for k in 0..3 {
            let (a, b) = (k * 2 * 48_000 + 4_800, (k + 1) * 2 * 48_000 - 4_800);
            let m = l[a..b].iter().map(|x| x.unsigned_abs()).max().unwrap() as f64 / 32_768.0;
            assert!((m - 0.5).abs() < 0.03, "clipe {k}: {m}");
        }
    }

    #[test]
    fn narracao_alem_do_fim_volume_original_e_inicio_do_arquivo() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-narracao-bordas").join("narração do perito");
        std::fs::create_dir_all(&pasta).unwrap();
        // pulsos de 4 ms em 0,25, 1,25 e 2,25 s
        let pulsos = |taxa: u32| format!("aevalsrc=exprs=0.9*lt(mod(t+0.75\\,1)\\,0.004):s={taxa}:c=mono:d=3");
        let wav = gerar(&cap, &pasta, "narração 1.wav", &["-f", "lavfi", "-i", &pulsos(48_000), "-c:a", "pcm_s16le"]);
        let src = com_video(&cap, &pasta, "tom.mkv", ("30", 4.0), &[], "aevalsrc=exprs=0.5:s=48000:c=mono:d=4", &["-c:a", "pcm_s16le"]);
        let fontes = HashMap::from([("v".to_string(), info(&cap, &src)), ("n".to_string(), info_som(&cap, &wav))]);
        // programa de 2 s; a narração de 3 s entra em 1 s (passa do fim); outra começa depois do fim; outra presa a clipe que não existe
        let clipes = vec![
            clipe_audio("x", "n", json!({ "ref": "programa", "t": 1.0 }), 0.0, None),
            clipe_audio("y", "n", json!({ "ref": "programa", "t": 5.0 }), 0.0, None),
            clipe_audio("z", "n", json!({ "ref": "clipe", "clipe": "nenhum", "desloc": 0.0 }), 0.0, None),
        ];
        let mut p = projeto((30, 1), vec![trecho("a", "v", 1.0, 3.0, 1.0, true)], vec![trilha("t", 0.5, clipes)], vec![midia_ref("n", 3.0)]);
        p.mixagem.volume_original = 0.5;
        let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        assert_eq!((passo.amostras, passo.entradas.len()), (96_000, 2));
        let l = rodar(&cap, &passo, &pasta.join("saída")).canal(0);
        assert_eq!(l.len(), 96_000);
        // original a 0,5 × 0,5; a narração (0,9 × 0,5) soma por cima, sem o volume do original
        let v = |x: f64| (x * 32_768.0).round() as i16;
        assert!(l[..60_000].iter().all(|&x| (x - v(0.25)).abs() <= 1));
        assert!(l[60_000..60_192].iter().all(|&x| (x - v(0.25 + 0.45)).abs() <= 1), "{:?}", &l[60_000..60_004]);
        assert!(l[60_192..].iter().all(|&x| (x - v(0.25)).abs() <= 1));

        // `de_s` conta do começo do som do arquivo: MP3 (começa em 0,025 s) e arquivo que começa em 100 s
        let mp3 = gerar(&cap, &pasta, "narração.mp3", &["-f", "lavfi", "-i", &pulsos(44_100), "-c:a", "libmp3lame", "-b:a", "128k"]);
        let mka = gerar(&cap, &pasta, "deslocado.mka", &["-f", "lavfi", "-i", &pulsos(48_000), "-c:a", "pcm_s16le", "-output_ts_offset", "100"]);
        // início negativo é pré-rolagem que o decodificador descarta: o som começa em 0
        for (arquivo, inicio, tol, forcar) in [(&mp3, 0.025057, 2, None), (&mka, 100.0, 0, None), (&wav, 0.0, 0, Some(-0.007))] {
            let mut fonte = info_som(&cap, arquivo);
            assert!((fonte.audio.as_ref().unwrap().inicio_s - inicio).abs() < 1e-6, "{:?}", fonte.audio);
            if let (Some(x), Some(a)) = (forcar, fonte.audio.as_mut()) {
                a.inicio_s = x;
            }
            let fontes = HashMap::from([("n".to_string(), fonte)]);
            for (de, ate, esperado) in [(0.0, None, 24_000 + 12_000), (1.0, Some(2.5), 24_000 + 12_000)] {
                let c = clipe_audio("x", "n", json!({ "ref": "programa", "t": 0.5 }), de, ate);
                let p = projeto((30, 1), vec![imagem("i", 4.0)], vec![trilha("t", 1.0, vec![c])], vec![midia_ref("n", 3.0)]);
                let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
                let l = rodar(&cap, &passo, &pasta.join("início")).canal(0);
                let i = inicio_do_clique(&l, 0, l.len()).expect("pulso") as i64;
                assert!((i - esperado).abs() <= tol, "{}: de {de}: pulso em {i}, esperado {esperado}", arquivo.display());
            }
        }
    }

    /// Amostras reais de gravador com som (`.dav` com tempo de época, MPEG-PS que começa em 40 001 s, IFV):
    /// tempo absoluto, amostras exatas e PCM igual nos dois binários.
    #[test]
    fn amostras_reais_de_gravador() {
        let Some(bins) = dois_binarios() else { return };
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("audio-gravador-real");
        let mut feitas = 0;
        for (k, arquivo) in fontes::amostras_de_gravador().iter().enumerate() {
            let Ok(tab) = TabelaQuadros::do_decodificador(&cap, arquivo, &AtomicBool::new(false), &mut |_| {}) else { continue };
            let Ok(fonte) = quadros::fonte_info(&cap, arquivo, &"0".repeat(64), Arc::new(tab), None) else { continue };
            let Some(som) = fonte.audio.clone() else { continue };
            let de = fonte.inicio_s + 2.0;
            let fontes = HashMap::from([("v".to_string(), fonte)]);
            let p = projeto((30, 1), vec![trecho("a", "v", de, de + 4.0, 1.0, true), trecho("b", "v", de + 6.0, de + 8.0, 0.5, false)], vec![], vec![]);
            let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
            let mut hashes = Vec::new();
            for b in &bins {
                let pcm = rodar(b, &passo, &pasta.join(format!("{k}-{}", b.versao)));
                assert_eq!(pcm.n() as u64, tempo::amostra_do_slot(linha.total_q, linha.fps));
                let com_som = pcm.x.iter().filter(|x| x.unsigned_abs() > 8).count();
                assert!(com_som > pcm.x.len() / 10, "{}: quase tudo silêncio ({com_som})", arquivo.display());
                hashes.push(pcm.sha);
            }
            eprintln!("{}: início {} s, som {:?}, {} amostras, SHA-256 {}", arquivo.display(), de - 2.0, som, passo.amostras, &hashes[0][..16]);
            assert_eq!(hashes[0], hashes[1], "{}", arquivo.display());
            feitas += 1;
        }
        eprintln!("amostras de gravador com som: {feitas}");
    }

    /// Quadros com pts negativo (o som começa em 0): `atrim` e `asetpts` com tempo negativo.
    #[test]
    fn inicio_negativo_tempo_absoluto() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-negativo");
        let som = format!("{CLIQUES}:s=48000:c=mono:d=6");
        let wav = gerar(&cap, &pasta, "cliques.wav", &["-f", "lavfi", "-i", &som, "-c:a", "pcm_s16le"]);
        let mut fonte = info_som(&cap, &wav);
        // 1 s de imagem antes do som
        fonte.quadros = Arc::new(apoio::tabela((1, 15360), apoio::pts_regulares(150, (1, 30), (1, 15360), -15360)));
        fonte.inicio_s = -1.0;
        let fontes = HashMap::from([("v".to_string(), fonte)]);
        let p = projeto(
            (30, 1),
            vec![trecho("a", "v", -0.8, 1.2, 1.0, true), trecho("b", "v", -1.0, 0.0, 0.5, false), trecho("c", "v", 0.0, 1.0, 1.0, true)],
            vec![],
            vec![],
        );
        let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        assert!(passo.grafo.contains("atrim=start=-0.800000:end=1.200000,asetpts=PTS+0.800000/TB"), "{}", passo.grafo);
        let l = rodar(&cap, &passo, &pasta).canal(0);
        assert_eq!(l.len(), 5 * 48_000);
        // a: 0,8 s de silêncio e o clique de 0,5 s; b: antes do som, silêncio; c: o clique de 0,5 s
        assert!(l[..38_400].iter().all(|&x| x == 0) && l[38_400..38_500].iter().any(|&x| x != 0));
        assert!(l[96_000..192_000].iter().all(|&x| x == 0));
        let achados: Vec<usize> = (1..l.len()).filter(|&i| l[i].unsigned_abs() > LIMIAR && l[i - 1].unsigned_abs() <= LIMIAR).collect();
        assert_eq!(achados, vec![62_400, 192_000 + 24_000]);
    }

    /// Cada fonte da matriz com som a 48 kHz: o recorte de um clipe é a fatia exata do som decodificado inteiro,
    /// e o PCM é o mesmo nos dois binários. Matroska fica de fora das duas contas exatas: os pts dos pacotes vêm
    /// em ms (o corte anda até 1 ms) e o 7.0.2 não descarta o pacote de pré-rolagem do AAC que o 9.0.1 descarta.
    #[test]
    fn matriz_de_fontes_recorte_e_fatia_do_som_inteiro() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-matriz");
        let bins = dois_binarios();
        let (mut conferidas, mut divergem) = (Vec::new(), Vec::new());
        for (k, f) in fontes::MATRIZ.iter().enumerate() {
            let src = fontes::gerar(*f).unwrap();
            let fonte = info(&cap, &src);
            let Some(som) = fonte.audio.clone() else { continue };
            let tab = fonte.quadros.clone();
            let n = tab.pts.len();
            let (j0, j1) = (n / 4, n / 2);
            let mut c = trecho("c", "v", tab.tempo(j0), tab.tempo(j1), 1.0, true);
            c["inicio_pts"] = json!(tab.pts[j0]);
            c["fim_pts"] = json!(tab.pts[j1]);
            let p = projeto((30, 1), vec![c], vec![], vec![]);
            let fontes_ = HashMap::from([("v".to_string(), fonte)]);
            let (passo, linha) = montar_com(&p, &fontes_, &cap, &final_(&p)).unwrap();
            let pcm = rodar(&cap, &passo, &pasta.join(format!("m{k}")));
            assert_eq!(pcm.n() as u64, tempo::amostra_do_slot(linha.total_q, linha.fps), "{f:?}");
            assert!(pcm.x.iter().any(|&x| x != 0), "{f:?}: silêncio");
            let matroska = fontes_["v"].conteiner.contains("matroska");
            if let Some(bins) = &bins {
                let h: Vec<String> = bins.iter().map(|b| rodar(b, &passo, &pasta.join(format!("m{k}-{}", b.versao))).sha).collect();
                if h[0] != h[1] {
                    assert!(matroska, "{f:?}: PCM diferente nos dois binários");
                    divergem.push(format!("{f:?}"));
                }
            }
            if som.taxa != 48_000 || som.canais > 2 {
                eprintln!("{f:?}: {som:?}, só contado");
                continue;
            }
            let estereo = if som.canais == 1 { "pan=stereo|c0=c0|c1=c0," } else { "" };
            let todo = rodar_pedaco(&cap, &pasta, &format!("t{k}"), Some(&src), &format!("{estereo}aformat=sample_fmts=fltp:channel_layouts=stereo"), None);
            let a0 = ((tab.tempo(j0) - som.inicio_s) * 48_000.0).round() as i64;
            // o clipe pode durar mais que o trecho da fonte (taxa diferente): compara o que é som da fonte
            let m = (((tab.tempo(j1) - tab.tempo(j0)) * 48_000.0) as usize).min(pcm.n()) - 2;
            assert!(a0 >= 0 && 2 * (a0 as usize + m) <= todo.len(), "{f:?}: a0 {a0}");
            let a0 = a0 as usize;
            // início do som que não cai em amostra inteira: a fatia pode andar 1 amostra (Matroska: 1 ms; com a
            // pré-rolagem que o 7.0.2 não descarta, mais um quadro de AAC)
            let folga: i64 = if matroska { 1024 + 48 } else { 1 };
            let fatia = |d: i64| {
                let i = a0 as i64 + d;
                (i >= 0 && 2 * (i as usize + m) <= todo.len()).then(|| &todo[2 * i as usize..2 * (i as usize + m)])
            };
            let achado = (-folga..=folga).find(|&d| fatia(d) == Some(&pcm.x[..2 * m]));
            assert!(achado.is_some(), "{f:?}: recorte diferente da fatia do som inteiro (som {som:?}, fonte começa em {})", fontes_["v"].inicio_s);
            conferidas.push(format!("{f:?} ({:+})", achado.unwrap_or(0)));
        }
        eprintln!("fatia do som inteiro (desvio em amostras): {}", conferidas.join(", "));
        eprintln!("PCM diferente nos dois binários: {divergem:?}");
        assert!(conferidas.len() >= 3, "{conferidas:?}");
    }
}
