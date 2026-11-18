//! Áudio do programa inteiro numa passada (§5.9). Dono: P1.5 (herda do P0.6b; inclui o som das transições, O3).
//! Decisão O6: a passada de áudio e a de medição rodam SEMPRE com `-cpuflags 0`, antes das entradas.
//!
//! Uma entrada por mídia (`-vn`, sem `-ss`) com `asplit`, e outra da mesma mídia quando o som teria de esperar na
//! memória mais de `TETO_ESPERA_S` (`distribuir`); por clipe `atrim` absoluto, `asetpts=PTS-T0/TB`,
//! `aresample=48000:async=1:first_pts=0` (silêncio inicial e buracos), volume animado, silenciar e tom (no tempo
//! da fonte), velocidade, `apad` e `atrim=end_sample`. A principal é emendada por `concat`; onde dois clipes se
//! sobrepõem (transição), os pedaços saem por amostra e somam por `amix` com `afade` de cada lado ("cortar": a troca
//! no meio). Depois: volume do original, abaixar sob as trilhas, trilhas presas por `adelay` e somadas por
//! `amix=normalize=0` (`de_s`/`ate_s` do clipe de áudio contam do começo do som do arquivo), ganho da normalização
//! (medido numa prévia) e limitador.

use std::collections::HashMap;
use std::fmt::Write as _;

use super::grafo::filtro;
use super::plano::{ArquivoTexto, Entrada, Papel, PassoAudio, Previa, MARCA_GANHO};
use super::{Alvo, Contexto, FonteInfo};
use crate::edicao::ffmpeg::args;
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::modelo::{
    Animavel, AudioTransicao, Chave, ClipeBase, ClipePrincipal, ClipeSobreposto, Interp, LinhaResolvida, Mixagem, ModoSilencio, Normalizar, ProjetoEdicao, Racional,
    Silencio, Tolerante, Transicao, Velocidade,
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
/// Grafo da medição (auxiliar do passo).
const MEDIR: &str = "audio-medir.txt";
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
/// Quadro do envelope por intervalos: o `enable` do `afade` vale por quadro (`receitas-audio-saida.md` §3).
const QUADRO_ENVELOPE: u64 = 1024;
/// Intervalo mínimo do envelope (45 ms): meio intervalo tem de passar de um quadro.
const MIN_ENVELOPE: u64 = 2160;
/// O `enable` cobre a rampa com esta folga (50 ms, mais de dois quadros).
const FOLGA_ENVELOPE: u64 = 2400;
/// Rampa do silenciar e do tom: 10 ms.
const RAMPA_SILENCIO: u64 = TAXA_AUDIO as u64 / 100;
/// Tom do "Silenciar trecho": 1 kHz a −18 dBFS (`aevalsrc` sai igual nos dois binários; o `sine` não).
const TOM: &str = "aevalsrc=exprs=0.125*sin(2*PI*1000*t):s=48000:n=1024";
/// Quadro do volume animado (`volume` com `eval=frame`): 5 ms.
const QUADRO_VOLUME: u32 = 256;
/// `loudnorm` da medição: só os `input_*` são lidos, os alvos daqui não pesam.
const LOUDNORM: &str = "loudnorm=I=-16:TP=-1.5:LRA=50:print_format=json";
/// Teto do limitador sem normalização (dBTP).
const LIMITE_PADRAO_DB: f64 = -1.0;
// CONTRATO: com o limitador ligado, `PassoAudio.normalizar.pico_db` leva este valor em vez do alvo do projeto
// (`executar` chama `ganho_db(medido, passo.normalizar)` e não sabe do limitador).
/// Com o limitador ligado o pico é dele: o ganho da normalização deixa de ser limitado pelo pico medido
/// (o alvo de pico do projeto vira o teto do `alimiter` e segue na proveniência, em `mixagem`).
const PICO_COM_LIMITADOR_DB: f64 = 120.0;

fn erro(codigo: impl Into<String>) -> SicroError {
    SicroError::Validation(codigo.into())
}

fn nao_suportado(o_que: &str) -> SicroError {
    erro(format!("nao_suportado: {o_que}"))
}

/// O que o motor de áudio ainda não faz, pelo detalhe do catálogo (`mensagens`). Uma regra só: `validar` (fase_0 e
/// transições) pergunta aqui. Vazia desde o P1.5 (rampa, `tela` e vídeo sobreposto são de outras fases).
const RECUSAS: &[&str] = &[];

/// `detalhe` ainda é recusado pelo motor de áudio?
pub fn recusa(detalhe: &str) -> bool {
    RECUSAS.contains(&detalhe)
}

/// Alvo de volume aceito pelo `loudnorm` (LUFS) e teto de pico (dBTP; o do limitador: −24..0).
pub const NORMALIZAR_LUFS: std::ops::RangeInclusive<f64> = -70.0..=-5.0;
pub const NORMALIZAR_PICO_DB: std::ops::RangeInclusive<f64> = -24.0..=0.0;

// CONTRATO: acréscimo (fecho da Fase 1): a validação acusa antes o que o motor recusa ou ajustaria calado.
/// Campo da mixagem fora do que o motor faz: `abaixar` (rampa negativa ou não finita, ou ganho acima de 0 dB, que
/// não abaixa) e `normalizar` (alvo ou pico fora da faixa). None = ok.
pub fn mixagem_invalida(m: &Mixagem) -> Option<&'static str> {
    let rampa_ok = |x: f64| x.is_finite() && x >= 0.0;
    let ab = &m.abaixar;
    if !(ab.db <= 0.0) || !rampa_ok(ab.ataque_s) || !rampa_ok(ab.soltura_s) {
        return Some("abaixar");
    }
    match &m.normalizar {
        Some(n) if !(NORMALIZAR_LUFS.contains(&n.lufs) && NORMALIZAR_PICO_DB.contains(&n.pico_db)) => Some("normalizar"),
        _ => None,
    }
}

/// Recorte de mídia que vira som (tempo de fonte). `ate` None = até o fim do som. `curva`, `mudos` e `tons` valem
/// no tempo do recorte, antes da velocidade; `entrada`/`saida` = `afade` (amostra inicial, amostras; 0 = nenhum)
/// no clipe pronto.
struct Som<'a> {
    fonte: &'a FonteInfo,
    de: f64,
    ate: Option<f64>,
    velocidade: Ritmo,
    volume: f64,
    curva: Option<String>,
    mudos: Vec<(u64, u64)>,
    tons: Vec<(u64, u64)>,
    entrada: (u64, u64),
    saida: (u64, u64),
}

/// Velocidade já decidida: 1×, `atempo` em estágios, `asetrate` (Hz) ou rampa em fatias (P3.4).
enum Ritmo {
    Normal,
    Atempo(Vec<f64>),
    Asetrate(u64),
    Fatias(Vec<Fatia>),
}

/// Fatia de 0,2 s de fonte da rampa (`receitas-video.md` §10): amostras [de, ate) do recorte que viram `n` amostras
/// por `atempo` constante (`estagios`) ou, sem "manter o tom", por `asetrate` + `aresample` (`hz`, exato), cortadas
/// na amostra. Lê até `ler` (≥ `ate`): o `atempo` segura o fim da entrada e, sem som depois da fatia, ela acabava
/// em silêncio (de 3 a 178 ms por emenda, medido na revisão do P3.4).
struct Fatia {
    de: u64,
    ate: u64,
    ler: u64,
    n: u64,
    estagios: Vec<f64>,
    hz: Option<u64>,
}

/// Fonte de cada fatia da rampa: 0,2 s.
const FATIA_AMOSTRAS: u64 = TAXA_AUDIO as u64 / 5;
/// Som lido depois da fatia, por unidade de fator ≥ 1 (0,1 s de saída a mais, cortado depois do `atempo`).
const FATIA_MARGEM: u64 = TAXA_AUDIO as u64 / 10;
/// Cruzamento entre fatias vizinhas: 5 ms.
const FATIA_CRUZA: u64 = TAXA_AUDIO as u64 / 200;

/// Clipe da trilha principal no programa: amostras [ini, fim) e o som (None = silêncio).
struct Item<'a> {
    ini: u64,
    fim: u64,
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
    /// Contador dos rótulos auxiliares (tom).
    aux: usize,
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

    fn entradas_do_passo(&self) -> Vec<Entrada> {
        self.entradas
            .iter()
            .map(|(id, _, f)| Entrada {
                antes: vec!["-vn".into()],
                antes_sem_busca: None,
                caminho: f.caminho.clone(),
                papel: Papel::Midia { id: id.to_string(), clipe: String::new() },
            })
            .collect()
    }
}

/// Segundos para opção de duração (µs, como o ffmpeg lê).
fn seg(t: f64) -> String {
    format!("{t:.6}")
}

/// Número dentro de expressão: negativo entre parênteses.
fn num(v: f64) -> String {
    let v = v + 0.0;
    if v < 0.0 {
        format!("({v})")
    } else {
        format!("{v}")
    }
}

/// Segundos em amostras a 48 kHz (negativo e NaN viram 0).
fn em_amostras(s: f64) -> u64 {
    (s * TAXA_AUDIO as f64).round() as u64
}

/// Silêncio estéreo de `n` amostras.
fn silencio(n: u64) -> String {
    format!("anullsrc=r={TAXA_AUDIO}:cl=stereo:n={QUADRO_SILENCIO},atrim=end_sample={n},{FORMATO}")
}

/// Intervalos [a, b) em ordem, com o mínimo de `MIN_ENVELOPE`, cortados em `total` e unidos quando as rampas
/// (`desce` antes, `sobe` depois) se cruzariam.
fn unir(mut v: Vec<(u64, u64)>, desce: u64, sobe: u64, total: u64) -> Vec<(u64, u64)> {
    v.retain(|&(a, b)| b > a && a < total);
    v.sort_unstable();
    let mut o: Vec<(u64, u64)> = Vec::with_capacity(v.len());
    for (a, b) in v {
        let b = b.max(a.saturating_add(MIN_ENVELOPE)).min(total);
        match o.last_mut() {
            Some(u) if a.saturating_sub(desce) <= u.1.saturating_add(sobe) => u.1 = u.1.max(b),
            _ => o.push((a, b)),
        }
    }
    o
}

/// Envelope por intervalos (`receitas-audio-saida.md` §2 e §3; `p1b/out/03c-afade.log`): ganho `piso` em [a, b),
/// descendo nas `desce` amostras antes de `a` e subindo nas `sobe` depois de `b`. O `afade` de saída segura o piso
/// depois da rampa e o de entrada antes dela; o `enable` (por quadro de `QUADRO_ENVELOPE`) passa de um para o outro
/// no meio do intervalo. Intervalo que começa em 0 só tem o de entrada; o que vai até `total`, só o de saída.
fn envelope(intervalos: &[(u64, u64)], total: u64, desce: u64, sobe: u64, piso: f64) -> Vec<String> {
    let s = |x: u64| seg(x as f64 / TAXA_AUDIO as f64);
    let mut f = vec![format!("asetnsamples=n={QUADRO_ENVELOPE}:p=0")];
    for &(a, b) in intervalos {
        let (com_saida, com_entrada) = (a > 0, b < total || a == 0);
        let meio = s(a + (b - a) / 2);
        let mut afade = |tipo: &str, ss: u64, ns: u64, enable: String| {
            let mut o = vec![("t", tipo.to_string()), ("ss", ss.to_string()), ("ns", ns.max(1).to_string())];
            if piso > 0.0 {
                o.push(("silence", format!("{piso:.6}")));
            }
            o.push(("enable", enable));
            f.push(filtro("afade", None, &o));
        };
        if com_saida {
            let ss = a.saturating_sub(desce);
            let de = format!("gte(t,{})", s(ss.saturating_sub(FOLGA_ENVELOPE)));
            afade("out", ss, a - ss, if com_entrada { format!("{de}*lt(t,{meio})") } else { de });
        }
        if com_entrada {
            let ate = format!("lte(t,{})", s(b.saturating_add(sobe).saturating_add(FOLGA_ENVELOPE)));
            afade("in", b, sobe, if com_saida { format!("gte(t,{meio})*{ate}") } else { ate });
        }
    }
    f
}

/// Silêncios do trecho em amostras do recorte `[de, ate)` (antes da velocidade): (todos, só os de tom).
fn silencios(v: &[Silencio], de: f64, ate: f64) -> (Vec<(u64, u64)>, Vec<(u64, u64)>) {
    let total = em_amostras(ate - de);
    let do_modo = |so_tom: bool| {
        let i = v.iter().filter(|s| (!so_tom || s.modo == ModoSilencio::Tom) && s.ate > s.de);
        unir(i.map(|s| (em_amostras(s.de - de), em_amostras(s.ate - de))).collect(), RAMPA_SILENCIO, RAMPA_SILENCIO, total)
    };
    (do_modo(false), do_modo(true))
}

/// Árvore balanceada sobre `t` (D9): `partes[i]` = (vale enquanto `t` é menor que isto, expressão).
fn arvore(partes: &[(f64, String)]) -> String {
    if partes.len() == 1 {
        return partes[0].1.clone();
    }
    let m = partes.len() / 2;
    format!("if(lt(t,{}),{},{})", num(partes[m - 1].0), arvore(&partes[..m]), arvore(&partes[m..]))
}

/// Expressão do volume animado sobre `t` do recorte: chaves em s com o zero em `zero`; fora delas vale a ponta
/// (como `curva::avaliar`).
fn curva_do_volume(ks: &[Chave<f64>], zero: f64) -> String {
    let mut partes: Vec<(f64, String)> = vec![(ks[0].t - zero, num(ks[0].v))];
    for w in ks.windows(2) {
        let (t0, t1) = (w[0].t - zero, w[1].t - zero);
        if t1 <= t0 {
            continue;
        }
        let (v0, dv) = (w[0].v, w[1].v - w[0].v);
        let u = format!("((t-{})/{})", num(t0), num(t1 - t0));
        let e = match w[0].interp {
            _ if dv == 0.0 => num(v0),
            Interp::Degrau => num(v0),
            Interp::Linear => format!("{}+{}*{u}", num(v0), num(dv)),
            Interp::Suave => format!("{}+{}*{u}*{u}*(3-2*{u})", num(v0), num(dv)),
        };
        partes.push((t1, e));
    }
    partes.push((f64::INFINITY, num(ks[ks.len() - 1].v)));
    arvore(&partes)
}

/// Cadeia de um recorte a partir do rótulo `de`, sem o rótulo de saída: `n` amostras a 48 kHz, estéreo, pts a
/// partir de 0 (§5.3, "Áudio do trecho"). Com tom a cadeia se parte: as linhas de antes vão para `g`.
fn cadeia(g: &mut Grafo, de: &str, s: &Som, n: u64) -> String {
    let mut c = format!("[{de}]atrim=start={}", seg(s.de));
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
    let canais = s.fonte.audio.as_ref().map_or(0, |a| a.canais);
    match canais {
        1 => c.push_str(",pan=stereo|c0=c0|c1=c0"),
        2 => c.push_str(",pan=stereo|c0=c0|c1=c1"),
        _ => {}
    }
    let _ = write!(c, ",aresample={TAXA_AUDIO}:async=1:first_pts=0");
    if let Some(e) = &s.curva {
        let _ = write!(c, ",asetnsamples=n={QUADRO_VOLUME}:p=0,{}", filtro("volume", None, &[("volume", e.clone()), ("eval", "frame".into())]));
    }
    let recorte = s.ate.map_or(u64::MAX, |ate| em_amostras(ate - s.de));
    if !s.mudos.is_empty() {
        for f in envelope(&s.mudos, recorte, RAMPA_SILENCIO, RAMPA_SILENCIO, 0.0) {
            c.push(',');
            c.push_str(&f);
        }
    }
    if !s.tons.is_empty() {
        // o tom soma por `amix` (duas entradas): o que vem antes fecha num rótulo e a cadeia segue depois dele
        if !(1..=2).contains(&canais) {
            c.push_str(",aformat=channel_layouts=stereo");
        }
        let base = format!("s{}", g.aux);
        g.aux += 1;
        g.linhas.push(format!("{c}[{base}]"));
        c = format!("[{base}]");
        for (k, &(a, b)) in s.tons.iter().enumerate() {
            let ss = a.saturating_sub(RAMPA_SILENCIO);
            let dur = b.saturating_add(RAMPA_SILENCIO).min(recorte).saturating_sub(ss).max(2);
            let rampa = RAMPA_SILENCIO.min(dur / 2);
            let mut l = format!("{TOM},atrim=end_sample={dur},afade=t=in:ss=0:ns={rampa},afade=t=out:ss={}:ns={rampa},aformat=sample_fmts=fltp,pan=stereo|c0=c0|c1=c0", dur - rampa);
            if ss > 0 {
                let _ = write!(l, ",adelay=delays={ss}S:all=1");
            }
            g.linhas.push(format!("{l}[{base}t{k}]"));
            let _ = write!(c, "[{base}t{k}]");
        }
        let _ = write!(c, "amix=inputs={}:normalize=0:dropout_transition=0:duration=first", s.tons.len() + 1);
    }
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
        Ritmo::Fatias(f) if !f.is_empty() => {
            // `asplit` do recorte, cada fatia cortada na amostra, `atempo` ou `asetrate`, completada e cortada no
            // tamanho dela. Com `atempo` de um lado da emenda as fases não casam (salto de até a escala cheia): a da
            // esquerda continua `cruza` amostras e as duas se cruzam (`afade` linear, soma 1); o exato emenda sem
            // cruzar. Pares e ímpares vão em `concat` separados, com o silêncio até a vez seguinte (`apad`), e o
            // `amix` soma os dois.
            let base = format!("s{}", g.aux);
            g.aux += 1;
            let saidas: String = (0..f.len()).map(|k| format!("[{base}f{k}]")).collect();
            let _ = write!(c, ",asplit={}{saidas}", f.len());
            g.linhas.push(c);
            let cruza: Vec<u64> = (0..f.len())
                .map(|k| match f.get(k + 1) {
                    Some(d) if !(f[k].estagios.is_empty() && d.estagios.is_empty()) => FATIA_CRUZA.min(d.n),
                    _ => 0,
                })
                .collect();
            let mut juntas = [String::new(), String::new()];
            for (k, x) in f.iter().enumerate() {
                let mut l = format!("[{base}f{k}]atrim=start_sample={}:end_sample={},asetpts=N/SR/TB", x.de, x.ler);
                for e in &x.estagios {
                    let _ = write!(l, ",atempo={e}");
                }
                if let Some(hz) = x.hz {
                    let _ = write!(l, ",asetrate={hz},aresample={TAXA_AUDIO}");
                }
                let _ = write!(l, ",apad,atrim=end_sample={},asetpts=N/SR/TB", x.n + cruza[k]);
                if k > 0 && cruza[k - 1] > 0 {
                    let _ = write!(l, ",afade=t=in:ss=0:ns={}", cruza[k - 1]);
                }
                if cruza[k] > 0 {
                    let _ = write!(l, ",afade=t=out:ss={}:ns={}", x.n, cruza[k]);
                }
                // silêncio até a próxima fatia da mesma paridade
                let vao = f.get(k + 1).map_or(0, |d| d.n - cruza[k]);
                if vao > 0 && k + 2 < f.len() {
                    let _ = write!(l, ",apad=pad_len={vao}");
                }
                let _ = write!(l, "[{base}g{k}]");
                g.linhas.push(l);
                let _ = write!(juntas[k % 2], "[{base}g{k}]");
            }
            let [pares, impares] = juntas;
            let n_pares = f.len().div_ceil(2);
            c = if f.len() == 1 {
                format!("{pares}anull")
            } else {
                g.linhas.push(format!("{pares}concat=n={n_pares}:v=0:a=1[{base}p]"));
                g.linhas.push(format!("{impares}concat=n={}:v=0:a=1,adelay=delays={}S:all=1[{base}i]", f.len() / 2, f[0].n));
                format!("[{base}p][{base}i]amix=inputs=2:normalize=0:dropout_transition=0")
            };
        }
        Ritmo::Fatias(_) => {}
    }
    let _ = write!(c, ",apad,atrim=end_sample={n},asetpts=N/SR/TB,{FORMATO}");
    if s.volume != 1.0 {
        let _ = write!(c, ",volume={}", s.volume);
    }
    if s.entrada.1 > 0 {
        let _ = write!(c, ",afade=t=in:ss={}:ns={}", s.entrada.0, s.entrada.1);
    }
    if s.saida.1 > 0 {
        let _ = write!(c, ",afade=t=out:ss={}:ns={}", s.saida.0, s.saida.1);
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

/// `estagios_atempo` de um fator real (> 0).
fn estagios_reais(mut f: f64) -> Vec<f64> {
    let mut e = Vec::new();
    while f > 2.0 {
        e.push(2.0);
        f /= 2.0;
    }
    while f < 0.5 {
        e.push(0.5);
        f /= 0.5;
    }
    if f != 1.0 {
        e.push(f);
    }
    e
}

/// `tolerancia_atempo_ms` de um fator real.
fn tolerancia_real_ms(f: f64) -> f64 {
    estagios_reais(f).iter().fold(0.0, |t, &f| t / f + if f < 1.0 { ATEMPO_LENTO_MS } else { ATEMPO_RAPIDO_MS })
}

/// Rampa: o recorte `[de, ate)` em fatias de 0,2 s de fonte; cada uma vira as amostras de saída que a rampa dá a
/// ela (τ pela mesma conta do vídeo, `tempo::Rampa`), por `atempo` constante (`manter_tom`) ou `asetrate`. O
/// desvio dentro da fatia é o do `atempo` do fator dela: a tolerância é a do pior fator da rampa (0 no exato).
fn fatias(pontos: &[Chave<f64>], de: f64, ate: f64, manter_tom: bool) -> Result<(Ritmo, f64)> {
    let rampa = tempo::Rampa::nova(pontos, de)?;
    let total = em_amostras(ate - de);
    let mut bordas: Vec<u64> = (0..total.div_ceil(FATIA_AMOSTRAS)).map(|k| k * FATIA_AMOSTRAS).collect();
    bordas.push(total);
    let us: Vec<f64> = bordas.iter().map(|&a| a as f64 / TAXA_AUDIO as f64).collect();
    let saida: Vec<u64> = rampa.taus(&us).into_iter().map(em_amostras).collect();
    let mut v = Vec::with_capacity(bordas.len());
    for k in 0..bordas.len() - 1 {
        let n = saida[k + 1].saturating_sub(saida[k]);
        if n == 0 || bordas[k + 1] <= bordas[k] {
            continue;
        }
        let fator = (bordas[k + 1] - bordas[k]) as f64 / n as f64;
        let ler = total.min(bordas[k + 1] + (FATIA_MARGEM as f64 * fator.max(1.0)).ceil() as u64);
        // exato: taxa declarada que faz das amostras da fatia as `n` da saída (o resto do arredondamento sai no corte)
        let (estagios, hz) = match manter_tom {
            true => (estagios_reais(fator), None),
            false => (Vec::new(), Some(((bordas[k + 1] - bordas[k]) * TAXA_AUDIO as u64 + n / 2) / n).filter(|&hz| hz != TAXA_AUDIO as u64)),
        };
        v.push(Fatia { de: bordas[k], ate: bordas[k + 1], ler, n, estagios, hz });
    }
    let (lento, rapido) = rampa.faixa();
    let tol = if manter_tom { tolerancia_real_ms(lento).max(tolerancia_real_ms(rapido)) } else { 0.0 };
    Ok((Ritmo::Fatias(v), tol))
}

fn velocidade(v: &Velocidade, manter_tom: bool) -> Result<(Ritmo, f64)> {
    let r = match v {
        Velocidade::Constante { fator } => tempo::fator_racional(*fator)?,
        // a rampa vai por `fatias` (precisa do recorte); aqui não chega
        Velocidade::Rampa { .. } => return Err(erro("velocidade")),
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

/// Volume do modelo: fixo (0..2; chaves todas iguais contam como fixo) ou as chaves da curva.
enum Vol<'a> {
    Fixo(f64),
    Curva(&'a [Chave<f64>]),
}

fn volume_de(v: &Animavel<f64>) -> Result<Vol<'_>> {
    let ruim = || erro("item_invalido: volume");
    let ok = |x: f64| x.is_finite() && x >= 0.0;
    match v {
        Animavel::Fixo(x) if ok(*x) => Ok(Vol::Fixo(*x)),
        Animavel::Fixo(_) => Err(ruim()),
        Animavel::Chaves(c) => {
            let k = c.first().ok_or_else(ruim)?;
            if c.iter().any(|o| !ok(o.v) || !o.t.is_finite()) {
                return Err(ruim());
            }
            Ok(if c.iter().all(|o| o.v == k.v) { Vol::Fixo(k.v) } else { Vol::Curva(c) })
        }
    }
}

/// (volume fixo, expressão da curva com o zero do tempo em `zero`).
fn volume_e_curva(v: &Animavel<f64>, zero: f64) -> Result<(f64, Option<String>)> {
    Ok(match volume_de(v)? {
        Vol::Fixo(x) => (x, None),
        Vol::Curva(ks) => (1.0, Some(curva_do_volume(ks, zero))),
    })
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

/// Recusa o que é de outra fase (nunca ignora calado) e o volume inválido.
fn recusar(p: &ProjetoEdicao, linha: &LinhaResolvida, clipes: &HashMap<&str, &ClipePrincipal>) -> Result<()> {
    for r in &linha.clipes {
        let Some(c) = clipes.get(r.id.as_str()) else { continue };
        match c {
            ClipePrincipal::Tela(_) => return Err(nao_suportado("tela")),
            ClipePrincipal::Trecho(t) if !t.audio.mudo => {
                volume_de(&t.audio.volume)?;
            }
            _ => {}
        }
    }
    for t in p.audios.iter().filter(|t| !t.mudo && !t.clipes.is_empty()) {
        for c in &t.clipes {
            volume_de(&c.volume)?;
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

/// Amostras do `afade` de uma transição sem sobreposição de `d` slots que começa no slot `q`.
fn amostras_da_transicao(t: &Transicao, q: u64, d: u64, fps: Racional) -> u64 {
    let n = tempo::amostra_do_slot(q + d, fps) - tempo::amostra_do_slot(q, fps);
    match t.audio {
        AudioTransicao::Acompanhar => n,
        AudioTransicao::Cortar => CORTE_AMOSTRAS.min(n),
    }
}

/// `afade` de entrada e de saída de cada clipe da linha, em amostras do clipe: (amostra inicial, amostras).
/// Com sobreposição (`sobrepoe_q`, a mesma de `juncao::Peca.sobrepoe`): "acompanhar" cruza os dois sons na
/// sobreposição inteira; "cortar" troca no meio dela, com 10 ms de cada lado. Sem sobreposição (`escurecer`, a
/// entrada do primeiro clipe e o `fecho`, que misturam com o fundo dentro do próprio clipe): rampa na ponta do
/// clipe, do tamanho da transição ("cortar": 10 ms).
fn fades(p: &ProjetoEdicao, linha: &LinhaResolvida, clipes: &HashMap<&str, &ClipePrincipal>) -> Vec<[(u64, u64); 2]> {
    let fps = linha.fps;
    let a = |q: u64| tempo::amostra_do_slot(q, fps);
    let faixa = |k: usize| (a(linha.clipes[k].inicio_q), a(linha.clipes[k].inicio_q + linha.clipes[k].n_q as u64));
    let n = linha.clipes.len();
    let mut f = vec![[(0u64, 0u64); 2]; n];
    let slots = |t: &Transicao| tempo::slots(t.duracao_s, fps);
    for (i, r) in linha.clipes.iter().enumerate() {
        let Some(t) = clipes.get(r.id.as_str()).and_then(|c| base(c).transicao_entrada.as_ref()) else { continue };
        let (ini, fim) = faixa(i);
        if i > 0 && tempo::transicao_sobrepoe(&t.tipo) {
            let (ini_ant, fim_ant) = faixa(i - 1);
            // amostras em que os dois clipes tocam juntos
            let s = fim_ant.min(fim).saturating_sub(ini);
            if s == 0 || ini < ini_ant {
                continue;
            }
            let de_ant = ini - ini_ant;
            match t.audio {
                AudioTransicao::Acompanhar => {
                    f[i][0] = (0, s);
                    f[i - 1][1] = (de_ant, s);
                }
                AudioTransicao::Cortar => {
                    let meio = s / 2;
                    let k = CORTE_AMOSTRAS.min(meio).min(s - meio).max(1);
                    f[i][0] = (meio, k);
                    f[i - 1][1] = (de_ant + meio.saturating_sub(k), k);
                }
            }
            continue;
        }
        let d = slots(t).min(r.n_q as u64);
        f[i][0] = (0, amostras_da_transicao(t, r.inicio_q, d, fps));
        if i > 0 {
            let ant = &linha.clipes[i - 1];
            let d = slots(t).min(ant.n_q as u64);
            let (ini_ant, fim_ant) = faixa(i - 1);
            let k = amostras_da_transicao(t, ant.inicio_q + ant.n_q as u64 - d, d, fps);
            f[i - 1][1] = (fim_ant - ini_ant - k, k);
        }
    }
    if let (Some(t), Some(r)) = (p.fecho.as_ref(), linha.clipes.last()) {
        let d = slots(t).min(r.n_q as u64);
        let (ini, fim) = faixa(n - 1);
        let k = amostras_da_transicao(t, r.inicio_q + r.n_q as u64 - d, d, fps);
        f[n - 1][1] = (fim - ini - k, k);
    }
    f
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
        let toca = !t.audio.mudo && volume_de(&t.audio.volume).is_ok_and(|v| !matches!(v, Vol::Fixo(x) if x <= 0.0));
        if toca && ctx.fontes.get(&t.midia).is_some_and(|f| f.audio.is_some()) {
            let tol = match &t.velocidade {
                Velocidade::Rampa { pontos } => fatias(pontos, 0.0, 0.0, t.audio.manter_tom).map_or(0.0, |x| x.1),
                v => velocidade(v, t.audio.manter_tom).map_or(0.0, |x| x.1),
            };
            atempo = atempo.max(tol);
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

/// A trilha principal: os clipes no lugar, `concat` dos segmentos entre fronteiras de clipe; segmento com dois
/// clipes (transição com sobreposição) soma os pedaços por `amix`. `inst` = instância da entrada de cada clipe com
/// som, na ordem. Devolve o rótulo.
fn principal<'a>(g: &mut Grafo<'a>, itens: &[Item<'a>], total: u64, inst: &mut dyn Iterator<Item = usize>) -> String {
    let mut cortes: Vec<u64> = itens.iter().flat_map(|i| [i.ini, i.fim]).chain([0, total]).filter(|x| *x <= total).collect();
    cortes.sort_unstable();
    cortes.dedup();
    // segmentos [de, ate) e os clipes com som que tocam nele; vizinhos iguais viram um
    let mut segmentos: Vec<(u64, u64, Vec<usize>)> = Vec::new();
    for w in cortes.windows(2) {
        let quem: Vec<usize> = (0..itens.len()).filter(|&k| itens[k].som.is_some() && itens[k].ini <= w[0] && itens[k].fim >= w[1]).collect();
        match segmentos.last_mut() {
            Some(u) if u.2 == quem => u.1 = w[1],
            _ => segmentos.push((w[0], w[1], quem)),
        }
    }
    let n_pedacos: Vec<usize> = (0..itens.len()).map(|k| segmentos.iter().filter(|s| s.2.contains(&k)).count()).collect();
    let mut dados = vec![0usize; itens.len()];
    let mut rotulos = Vec::with_capacity(segmentos.len());
    for (k, (de, ate, quem)) in segmentos.iter().enumerate() {
        let r = format!("p{k}");
        let mut pedacos = Vec::with_capacity(quem.len());
        for &c in quem {
            let it = &itens[c];
            let Some((id, s)) = &it.som else { continue };
            // rótulo deste pedaço: o do segmento quando o clipe toca sozinho nele
            let pedaco = if quem.len() == 1 { r.clone() } else { format!("c{c}q{}", dados[c]) };
            if dados[c] == 0 {
                let entrada = g.uso(*id, inst.next().unwrap_or(0), s.fonte);
                let mut l = cadeia(g, &entrada, s, it.fim - it.ini);
                if n_pedacos[c] == 1 {
                    let _ = write!(l, "[{pedaco}]");
                } else {
                    let saidas: String = (0..n_pedacos[c]).map(|j| format!("[c{c}s{j}]")).collect();
                    let _ = write!(l, ",asplit={}{saidas}", n_pedacos[c]);
                }
                g.linhas.push(l);
            }
            if n_pedacos[c] > 1 {
                g.linhas.push(format!("[c{c}s{}]atrim=start_sample={}:end_sample={},asetpts=N/SR/TB[{pedaco}]", dados[c], de - it.ini, ate - it.ini));
            }
            dados[c] += 1;
            pedacos.push(pedaco);
        }
        match pedacos.len() {
            0 => g.linhas.push(format!("{}[{r}]", silencio(ate - de))),
            1 => {}
            m => {
                let entradas: String = pedacos.iter().map(|x| format!("[{x}]")).collect();
                g.linhas.push(format!("{entradas}amix=inputs={m}:normalize=0:dropout_transition=0:duration=first[{r}]"));
            }
        }
        rotulos.push(r);
    }
    match rotulos.len() {
        1 => rotulos.remove(0),
        n => {
            g.linhas.push(format!("{}concat=n={n}:v=0:a=1[pr]", rotulos.iter().map(|r| format!("[{r}]")).collect::<String>()));
            "pr".to_string()
        }
    }
}

/// Limitador (`receitas-audio-saida.md` §4): pico real por 4× de sobreamostragem, sem ganho automático e com o
/// atraso compensado; a contagem de amostras volta a `n`.
fn limitador(teto_db: f64, n: u64) -> String {
    let limite = 10f64.powf(teto_db / 20.0).clamp(0.0625, 1.0);
    format!("aresample=192000,alimiter=limit={limite:.6}:attack=5:release=50:level=0:latency=1,aresample={TAXA_AUDIO},apad,atrim=end_sample={n},asetpts=N/SR/TB")
}

/// O grafo até a mixagem (antes do ganho) e o rótulo do fim.
struct Mixado<'a> {
    g: Grafo<'a>,
    fim: String,
    tolerancia: f64,
}

/// Monta o som do programa para as amostras `[s0, s1)`: clipes fora da janela viram silêncio (a mídia nem abre).
fn mixar<'a>(p: &'a ProjetoEdicao, linha: &LinhaResolvida, ctx: &Contexto<'a>, (s0, s1): (u64, u64), teto: f64) -> Result<Mixado<'a>> {
    let fps = linha.fps;
    let a = |q: u64| tempo::amostra_do_slot(q, fps);
    let total = a(linha.total_q);
    let mut g = Grafo::default();
    let mut tolerancia: f64 = 0.0;
    let clipes: HashMap<&str, &ClipePrincipal> = p
        .principal
        .iter()
        .filter_map(|c| match c {
            Tolerante::Conhecido(c) => Some((base(c).id.as_str(), c)),
            Tolerante::Desconhecido(_) => None,
        })
        .collect();
    recusar(p, linha, &clipes)?;
    let rampas = fades(p, linha, &clipes);

    // trilha principal: um item por clipe; os de fora da janela e os sem som são silêncio
    let mut itens: Vec<Item> = Vec::with_capacity(linha.clipes.len());
    for (i, r) in linha.clipes.iter().enumerate() {
        let (ini, fim) = (a(r.inicio_q), a(r.inicio_q + r.n_q as u64));
        let mut som = None;
        if let Some(ClipePrincipal::Trecho(t)) = clipes.get(r.id.as_str()).copied() {
            let vol = if t.audio.mudo { Vol::Fixo(0.0) } else { volume_de(&t.audio.volume)? };
            if !matches!(vol, Vol::Fixo(x) if x <= 0.0) && fim > s0 && ini < s1 {
                let fonte = ctx.fontes.get(&t.midia).ok_or_else(|| erro(format!("midia_ausente: {}", t.midia)))?;
                let faixa = r.fonte.as_ref().ok_or_else(|| erro(format!("intervalo: {}", r.id)))?;
                if fonte.audio.is_some() {
                    let tab = &fonte.quadros;
                    let tb = |pts: i64| pts as f64 * tab.tb.num as f64 / tab.tb.den as f64;
                    let (de, ate) = (tb(faixa.pts_in), faixa.pts_out.map_or_else(|| tab.fim_s(), tb));
                    // rampa: fatias com `atempo` ou, sem "manter o tom", com `asetrate`
                    let (v, tol) = match &t.velocidade {
                        Velocidade::Rampa { pontos } => fatias(pontos, de, ate, t.audio.manter_tom)?,
                        v => velocidade(v, t.audio.manter_tom)?,
                    };
                    tolerancia = tolerancia.max(tol);
                    // chaves do volume e silêncios em tempo de fonte (§3.1)
                    let (volume, curva) = match vol {
                        Vol::Fixo(x) => (x, None),
                        Vol::Curva(ks) => (1.0, Some(curva_do_volume(ks, de))),
                    };
                    let (mudos, tons) = silencios(&t.audio.silencios, de, ate);
                    let s = Som { fonte, de, ate: Some(ate), velocidade: v, volume, curva, mudos, tons, entrada: rampas[i][0], saida: rampas[i][1] };
                    som = Some((t.midia.as_str(), s));
                }
            }
        }
        itens.push(Item { ini, fim, som });
    }
    // trilhas de áudio: cada clipe no lugar dele, do tamanho do programa
    let mut faixas: Vec<(&str, Som, u64, u64)> = Vec::new();
    // onde o original abaixa: os clipes que tocam nas trilhas com `abaixa_original`, no programa inteiro
    let mut falas: Vec<(u64, u64)> = Vec::new();
    for t in p.audios.iter().filter(|t| !t.mudo) {
        for c in &t.clipes {
            let Some(r) = linha.livres.iter().find(|l| l.id == c.id && l.trilha == t.id) else { continue };
            let (ini, fim) = (a(r.inicio_q), a(r.inicio_q + r.n_q as u64).min(total));
            let n = a(r.inicio_q + r.n_q as u64) - ini;
            // volume animado: tempo local do clipe
            let (fixo, curva) = volume_e_curva(&c.volume, 0.0)?;
            let volume = t.volume * fixo;
            if !volume.is_finite() || volume <= 0.0 || ini >= fim {
                continue;
            }
            let fonte = ctx.fontes.get(&c.midia);
            if t.abaixa_original && fonte.is_none_or(|f| f.audio.is_some()) {
                falas.push((ini, fim));
            }
            if fim <= s0 || ini >= s1 {
                continue;
            }
            let fonte = fonte.ok_or_else(|| erro(format!("midia_ausente: {}", c.midia)))?;
            let Some(som_da_fonte) = &fonte.audio else { continue };
            let amostras_de = |s: f64| if s.is_finite() && s > 0.0 { em_amostras(s).min(n) } else { 0 };
            // `de_s`/`ate_s` contam do começo do som do arquivo (MP3 começa em 0,025 s; início negativo é pré-rolagem)
            let inicio = som_da_fonte.inicio_s.max(0.0);
            let saida = amostras_de(c.saida_s);
            let s = Som {
                fonte,
                de: inicio + c.de_s,
                ate: c.ate_s.map(|x| inicio + x),
                velocidade: Ritmo::Normal,
                volume,
                curva,
                mudos: Vec::new(),
                tons: Vec::new(),
                entrada: (0, amostras_de(c.entrada_s)),
                saida: (n - saida, saida),
            };
            faixas.push((c.midia.as_str(), s, n, ini));
        }
    }

    // instância da entrada de cada uso: os da principal (na ordem) e depois os das trilhas
    let mut usos: Vec<Uso> = Vec::new();
    for it in &itens {
        if let Some((id, s)) = &it.som {
            usos.push(Uso { id, de: s.de, ate: s.ate.unwrap_or(s.de), ini: it.ini, n: it.fim - it.ini });
        }
    }
    for (id, s, n, ini) in &faixas {
        let ate = s.ate.unwrap_or(s.de + *n as f64 / TAXA_AUDIO as f64);
        usos.push(Uso { id, de: s.de, ate, ini: *ini, n: *n });
    }
    let mut inst = distribuir(&usos, teto).into_iter();

    let mut atual = principal(&mut g, &itens, total, &mut inst);
    let vo = p.mixagem.volume_original;
    if !vo.is_finite() || vo < 0.0 {
        return Err(erro("item_invalido: volume_original"));
    }
    if vo != 1.0 {
        g.linhas.push(format!("[{atual}]volume={vo}[po]"));
        atual = "po".into();
    }
    // abaixar o original sob as trilhas (`receitas-audio-saida.md` §2)
    if !falas.is_empty() {
        let ab = &p.mixagem.abaixar;
        let rampa_ok = |x: f64| x.is_finite() && x >= 0.0;
        if ab.db.is_nan() || !rampa_ok(ab.ataque_s) || !rampa_ok(ab.soltura_s) {
            return Err(erro("item_invalido: abaixar"));
        }
        let piso = 10f64.powf(ab.db / 20.0).clamp(0.0, 1.0);
        let (desce, sobe) = (em_amostras(ab.ataque_s).max(1), em_amostras(ab.soltura_s).max(1));
        let intervalos = unir(falas, desce, sobe, total);
        if piso < 1.0 && !intervalos.is_empty() {
            g.linhas.push(format!("[{atual}]{}[pb]", envelope(&intervalos, total, desce, sobe, piso).join(",")));
            atual = "pb".into();
        }
    }

    let mut mistura = vec![atual];
    for (id, s, n, ini) in &faixas {
        let r = format!("n{}", mistura.len() - 1);
        let entrada = g.uso(*id, inst.next().unwrap_or(0), s.fonte);
        let mut l = cadeia(&mut g, &entrada, s, *n);
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
    Ok(Mixado { g, fim: atual, tolerancia })
}

/// Monta o passo de áudio do programa e a prévia de medição. (None, vazio) quando o alvo não tem som (`Alvo::Quadro`).
/// Programa sem `saida.audio`: trilha de silêncio, sem medição. `Alvo::Previa`: as amostras dos slots `[de_q, ate_q)`.
/// A medição (`Previa::Loudnorm`, grafo em `auxiliares`) é sempre a do programa INTEIRO: roda em toda exportação
/// final (com `normalizar` None só mede, para o aviso de pico) e, na prévia, só quando há normalização.
/// Com o limitador a prévia de uma janela não é a fatia exata do final (o limitador começa na janela).
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
    let mut globais: Vec<String> = CPUFLAGS.iter().map(|s| s.to_string()).collect();
    globais.extend(args::globais());
    let fecho = |fim: &str, limitador: &str| format!("[{fim}]volume={MARCA_GANHO}dB{limitador},{FORMATO_SAIDA},asplit=2[a][ah]");
    let mut passo = PassoAudio {
        globais,
        entradas: Vec::new(),
        grafo: String::new(),
        auxiliares: Vec::new(),
        saida: SAIDA.into(),
        hash: HASH.into(),
        amostras: s1 - s0,
        normalizar: None,
        tolerancia_atempo_ms: 0.0,
    };
    if !p.saida.audio {
        passo.grafo = format!("{}[sl];\n{}", silencio(s1 - s0), fecho("sl", ""));
        return Ok((Some(passo), Vec::new()));
    }
    let mix = p.mixagem.clone();
    if mix.normalizar.is_some_and(|n| !n.lufs.is_finite() || !n.pico_db.is_finite()) {
        return Err(erro("item_invalido: normalizar"));
    }
    let m = mixar(p, linha, ctx, (s0, s1), teto)?;
    let limite = if mix.limitador { format!(",{}", limitador(mix.normalizar.map_or(LIMITE_PADRAO_DB, |n| n.pico_db), s1 - s0)) } else { String::new() };
    passo.grafo = format!("{};\n{}", m.g.texto(), fecho(&m.fim, &limite));
    passo.entradas = m.g.entradas_do_passo();
    passo.tolerancia_atempo_ms = m.tolerancia;
    passo.normalizar = mix.normalizar.map(|n| Normalizar { lufs: n.lufs, pico_db: if mix.limitador { PICO_COM_LIMITADOR_DB } else { n.pico_db } });

    // medição do programa inteiro, antes do ganho e do limitador
    let inteiro = (s0, s1) == (0, total);
    let medido = match alvo {
        Alvo::Previa { .. } if mix.normalizar.is_none() => None,
        _ if inteiro => Some((m.g.texto(), m.fim.clone(), passo.entradas.clone())),
        _ => {
            let t = mixar(p, linha, ctx, (0, total), teto)?;
            Some((t.g.texto(), t.fim, t.g.entradas_do_passo()))
        }
    };
    let mut previas = Vec::new();
    if let Some((grafo, fim, entradas)) = medido {
        passo.auxiliares.push(ArquivoTexto { nome: MEDIR.into(), conteudo: format!("{grafo};\n[{fim}]{LOUDNORM}[an]") });
        let mut v = passo.globais.clone();
        for e in &entradas {
            v.extend(e.antes.iter().cloned());
            v.push("-i".into());
            v.push(e.caminho.to_string_lossy().into_owned());
        }
        v.extend(args::grafo_em_arquivo(ctx.cap, MEDIR));
        v.extend(["-map", "[an]", "-f", "null", "-"].iter().map(|s| s.to_string()));
        previas.push(Previa::Loudnorm { args: v });
    }
    Ok((Some(passo), previas))
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
        // medição: uma, em toda exportação final com som; na prévia só com normalização
        let mede = p.saida.audio && (matches!(alvo, Alvo::Final { .. }) || p.mixagem.normalizar.is_some());
        assert_eq!(previas.len(), usize::from(mede));
        Ok((passo.expect("passo"), linha))
    }

    /// Como `montar_com`, com a prévia de medição.
    fn montar_e_medir(p: &ProjetoEdicao, fontes: &HashMap<String, FonteInfo>, cap: &Capacidades, alvo: &Alvo) -> (PassoAudio, Vec<Previa>, LinhaResolvida) {
        let (leitor, pasta, midias) = (apoio::SemRastreios, PathBuf::new(), Vec::<EstadoMidia>::new());
        let ctx = Contexto { fontes, cap, rastreios: &leitor, pasta_projeto: &pasta, midias: &midias };
        let linha = tempo::resolver(p, &ctx);
        let (passo, previas) = montar(p, &linha, &ctx, alvo).expect("montar");
        (passo.expect("passo"), previas, linha)
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
        rodar_com_ganho(cap, passo, pasta, globais, None)
    }

    /// `rodar_com` com o ganho da normalização (dB).
    fn rodar_com_ganho(cap: &Capacidades, passo: &PassoAudio, pasta: &Path, globais: Option<Vec<String>>, ganho: Option<f64>) -> (Pcm, f64) {
        std::fs::create_dir_all(pasta).unwrap();
        args::gravar_texto(pasta, "audio.txt", &grafo_com_ganho(passo, ganho)).unwrap();
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
        assert_eq!((passo.saida.as_str(), passo.hash.as_str(), passo.normalizar), ("audio.wav", "audio.sha256", None));
        // o grafo da medição é o mesmo, terminando no `loudnorm`
        assert_eq!(passo.auxiliares.len(), 1);
        let medir = &passo.auxiliares[0];
        let ate_o_ganho = passo.grafo.rsplit_once(";\n").unwrap();
        assert_eq!(medir.nome, "audio-medir.txt");
        assert_eq!(medir.conteudo, format!("{};\n[mi]{LOUDNORM}[an]", ate_o_ganho.0));
        assert!(ate_o_ganho.1.starts_with("[mi]volume="), "{}", ate_o_ganho.1);
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
    fn recusa_o_que_e_de_outra_fase_e_o_valor_invalido() {
        let (cap, fontes) = (apoio::cap_falsa(), fontes_falsas());
        let erro_de = |p: &ProjetoEdicao| match montar_com(p, &fontes, &cap, &final_(p)) {
            Err(SicroError::Validation(c)) => c,
            outro => panic!("esperava erro: {outro:?}"),
        };
        type Mudanca = Box<dyn Fn(&mut ProjetoEdicao)>;
        let trecho_0 = |p: &mut ProjetoEdicao, f: &dyn Fn(&mut crate::edicao::modelo::ClipeTrecho)| {
            let Tolerante::Conhecido(ClipePrincipal::Trecho(t)) = &mut p.principal[0] else { panic!() };
            f(t);
        };
        let casos: Vec<(&str, Mudanca)> = vec![
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
            ("item_invalido: volume", Box::new(move |p| trecho_0(p, &|t| t.audio.volume = Animavel::Chaves(Vec::new())))),
            ("item_invalido: volume", Box::new(|p| p.audios[0].clipes[0].volume = serde_json::from_value(json!([{ "t": 0.0, "v": 1.0 }, { "t": 1.0, "v": -0.5 }])).unwrap())),
            ("item_invalido: normalizar", Box::new(|p| p.mixagem.normalizar = Some(Normalizar { lufs: f64::NAN, pico_db: -1.5 }))),
            (
                "item_invalido: abaixar",
                Box::new(|p| {
                    p.audios[0].abaixa_original = true;
                    p.mixagem.abaixar.ataque_s = -1.0;
                }),
            ),
        ];
        for (i, (esperado, mudar)) in casos.iter().enumerate() {
            let mut p = projeto_falso();
            mudar(&mut p);
            assert_eq!(erro_de(&p), *esperado, "caso {i}");
        }
        // mídia da narração sumida
        let mut f = fontes.clone();
        f.remove("n1");
        let p = projeto_falso();
        assert!(matches!(montar_com(&p, &f, &cap, &final_(&p)), Err(SicroError::Validation(c)) if c == "midia_ausente: n1"));
        // vídeo sobreposto com som em trilha OCULTA não entra no render: nada a recusar (a validação também não acusa)
        let mut p = projeto_falso();
        casos[0].1(&mut p);
        assert_eq!(erro_de(&p), "nao_suportado: video sobreposto");
        p.sobreposicoes[0].oculta = true;
        assert!(montar_com(&p, &fontes, &cap, &final_(&p)).is_ok(), "trilha oculta recusada");
        // sem efeito no som: clipe mudo com silêncio e trilha muda com abaixar não deixam nada no grafo
        let mut p = projeto_falso();
        trecho_0(&mut p, &|t| {
            t.audio.mudo = true;
            t.audio.silencios = serde_json::from_value(json!([{ "id": "s", "de": 1.5, "ate": 2.0, "modo": "mudo", "motivo": "" }])).unwrap();
        });
        p.audios[0].abaixa_original = true;
        p.audios[0].mudo = true;
        let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        // v1 ainda entra pelo clipe "c"; n1 (trilha muda) não
        assert_eq!(passo.entradas.len(), 2);
        assert!(!passo.grafo.contains("enable") && !passo.grafo.contains("aevalsrc"), "{}", passo.grafo);
        // chaves todas iguais = volume fixo
        let mut p = projeto_falso();
        p.audios[0].clipes[0].volume = serde_json::from_value(json!([{ "t": 0.0, "v": 0.5 }, { "t": 1.0, "v": 0.5 }])).unwrap();
        let g = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap().0.grafo;
        assert!(g.contains("volume=0.4") && !g.contains("eval=frame"), "{g}");
        // nada da Fase 1 é recusado
        for d in ["normalizar", "limitador", "silenciar", "volume_animado", "abaixar", "transicao"] {
            assert!(!recusa(d), "{d}");
        }
        // rampa com som (P3.4): fatias, não recusa; mudo, nem abre a cadeia
        let mut p = projeto_falso();
        trecho_0(&mut p, &|t| t.velocidade = serde_json::from_value(json!({ "tipo": "rampa", "pontos": [{ "t": 1.0, "v": 1.0 }, { "t": 2.0, "v": 0.5 }] })).unwrap());
        let g = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap().0.grafo;
        assert!(g.contains("f0]atrim=start_sample=0:") && g.contains(":v=0:a=1") && g.contains("atempo="), "{g}");
        trecho_0(&mut p, &|t| t.audio.mudo = true);
        let g = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap().0.grafo;
        assert!(!g.contains("f0]atrim=start_sample="), "{g}");
    }

    /// Rampa em fatias de 0,2 s de fonte (P3.4): cada fatia é cortada na amostra e vira as amostras que a rampa dá a
    /// ela; os fatores ficam dentro da faixa da rampa.
    #[test]
    fn fatias_da_rampa_somam_a_integral() {
        let pontos: Vec<Chave<f64>> = serde_json::from_value(json!([{ "t": 11.0, "v": 1.0 }, { "t": 12.0, "v": 0.25 }, { "t": 13.0, "v": 1.0 }])).unwrap();
        let (Ritmo::Fatias(f), tol) = fatias(&pontos, 10.3, 13.7, true).unwrap() else { panic!() };
        assert_eq!(tol, tolerancia_real_ms(0.25));
        // 3,4 s de fonte: 17 fatias de 0,2 s, emendadas
        assert_eq!(f.len(), 17);
        assert!(f.windows(2).all(|w| w[0].ate == w[1].de) && f[0].de == 0 && f[16].ate == em_amostras(3.4));
        let rampa = tempo::Rampa::nova(&pontos, 10.3).unwrap();
        assert_eq!(f.iter().map(|x| x.n).sum::<u64>(), em_amostras(rampa.duracao(3.4)));
        for x in &f {
            let fator: f64 = x.estagios.iter().product();
            assert!(x.estagios.iter().all(|e| (0.5..=2.0).contains(e)) && (0.249..=1.001).contains(&fator), "{:?}", x.estagios);
            assert!((fator - (x.ate - x.de) as f64 / x.n as f64).abs() < 1e-9);
        }
        assert!(fatias(&[], 0.0, 1.0, true).is_err());
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

    /// Rampa com som (P3.4, `receitas-video.md` §10): 1× → 0,25× → 1×. As rajadas da fonte (x,0 s) caem no instante
    /// de programa que a rampa dá a elas (`tempo::programa_da_fonte`, a mesma conta do vídeo), dentro do desvio
    /// declarado do `atempo` ou, sem "manter o tom", a até 1 ms; o clipe tem as amostras exatas dos slots.
    #[test]
    fn rampa_em_fatias_marcas_no_lugar() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("audio-rampa");
        let som = format!("{SINAL}:s=48000:c=mono:d=8");
        let src = com_video(&cap, &pasta, "rampa.mkv", ("30", 8.0), &[], &som, &["-c:a", "pcm_s16le"]);
        let fonte = info(&cap, &src);
        let tab = fonte.quadros.clone();
        let fontes = HashMap::from([("v".to_string(), fonte)]);
        for manter_tom in [true, false] {
            let mut t = trecho("c0", "v", 0.8, 6.2, 1.0, manter_tom);
            t["velocidade"] = json!({ "tipo": "rampa", "pontos": [{ "t": 1.8, "v": 1.0 }, { "t": 3.4, "v": 0.25 }, { "t": 5.0, "v": 1.0 }] });
            let v: Velocidade = serde_json::from_value(t["velocidade"].clone()).unwrap();
            let p = projeto((30, 1), vec![imagem("i0", 0.5), t], vec![], vec![]);
            let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
            // 5,4 s de fonte em 27 fatias; as de 1× (antes de 1,8 s e depois de 5 s) não passam por `atempo` nem `asetrate`
            let mudar = if manter_tom { "atempo=" } else { "asetrate=" };
            assert!(passo.grafo.contains(",asplit=27[") && passo.grafo.matches(mudar).count() >= 15, "{}", passo.grafo);
            assert_eq!(passo.grafo.contains("atempo="), manter_tom);
            let fr = racional((30, 1));
            let l = rodar(&cap, &passo, &pasta).canal(0);
            assert_eq!(l.len() as u64, tempo::amostra_do_slot(linha.total_q, fr));
            let c = &linha.clipes[1];
            let f = c.fonte.as_ref().unwrap();
            let t0 = f.pts_in as f64 * tab.tb.num as f64 / tab.tb.den as f64;
            let a0 = tempo::amostra_do_slot(c.inicio_q, fr) as f64;
            // exato: a subida da rajada no tom mais grave (até 1 ms a 0,25×)
            let tol = if manter_tom { passo.tolerancia_atempo_ms * 48.0 } else { 48.0 };
            let mut d = Vec::new();
            for marca in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
                let e = a0 + tempo::programa_da_fonte(&v, t0, marca).unwrap() * TAXA_AUDIO as f64;
                let i = inicio_do_clique(&l, (e - 9600.0) as usize, (e + 9600.0) as usize).unwrap_or_else(|| panic!("rajada de {marca} s não achada ({})", cap.versao));
                d.push(i as f64 - e);
            }
            eprintln!("rampa com som ({}, manter o tom {manter_tom}): desvios {d:?} amostras, tolerância {tol}", cap.versao);
            assert!(passo.tolerancia_atempo_ms > 0.0 || !manter_tom);
            assert!(d.iter().all(|x| x.abs() <= tol), "{d:?} > {tol} ({})", cap.versao);
            // antes da rampa (1×) a fatia não muda: o clique de 1,5 s fica na amostra
            let e = a0 + tempo::programa_da_fonte(&v, t0, 1.5).unwrap() * TAXA_AUDIO as f64;
            let i = inicio_do_clique(&l, (e - 2400.0) as usize, (e + 2400.0) as usize).unwrap();
            assert!((i as f64 - e).abs() <= TOL_EXATO, "clique de 1,5 s: {} amostras ({})", i as f64 - e, cap.versao);
        }
    }

    /// Emendas das fatias da rampa (revisão do P3.4): seno contínuo de 440 Hz por 1× → v× → 1×, de 0,05× a 16×, com
    /// e sem "manter o tom". Sem silêncio no meio do clipe (a fatia lida só até o fim dela acabava muda: até 178 ms
    /// por emenda), sem salto na emenda (fase do `atempo` de cada fatia: até a escala cheia acelerando) e o
    /// cruzamento sem buraco de volume.
    #[test]
    fn rampa_emendas_sem_vao_nem_salto() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("audio-rampa-emendas");
        let src = com_video(&cap, &pasta, "seno.mkv", ("30", 8.0), &[], "sine=f=440:r=48000:d=8", &["-c:a", "pcm_s16le"]);
        let fonte = info(&cap, &src);
        let tab = fonte.quadros.clone();
        let fontes = HashMap::from([("v".to_string(), fonte)]);
        let fr = racional((30, 1));
        for manter_tom in [true, false] {
            for lento in [0.05, 0.25, 4.0, 16.0] {
                let rotulo = format!("{lento}×, manter o tom {manter_tom} ({})", cap.versao);
                let mut t = trecho("c0", "v", 0.8, 6.2, 1.0, manter_tom);
                t["velocidade"] = json!({ "tipo": "rampa", "pontos": [{ "t": 1.8, "v": 1.0 }, { "t": 3.4, "v": lento }, { "t": 5.0, "v": 1.0 }] });
                let pontos: Vec<Chave<f64>> = serde_json::from_value(t["velocidade"]["pontos"].clone()).unwrap();
                let p = projeto((30, 1), vec![imagem("i0", 0.5), t], vec![], vec![]);
                let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
                let l: Vec<f64> = rodar(&cap, &passo, &pasta).canal(0).into_iter().map(f64::from).collect();
                let c = &linha.clipes[1];
                let f = c.fonte.as_ref().unwrap();
                let tb = |p: i64| p as f64 * tab.tb.num as f64 / tab.tb.den as f64;
                let (t0, t1) = (tb(f.pts_in), f.pts_out.map_or_else(|| tab.fim_s(), tb));
                let a0 = tempo::amostra_do_slot(c.inicio_q, fr) as usize;
                let (Ritmo::Fatias(fs), _) = fatias(&pontos, t0, t1, manter_tom).unwrap() else { panic!() };
                let bordas: Vec<usize> = fs.iter().scan(a0, |a, x| { *a += x.n as usize; Some(*a) }).collect();
                // as fatias somam a integral; o clipe tem os slots do vídeo (até meio slot a menos)
                let fim = bordas[bordas.len() - 1].min(tempo::amostra_do_slot(c.inicio_q + c.n_q as u64, fr) as usize);
                let emendas: Vec<usize> = bordas.iter().copied().filter(|&b| b + 240 < fim).collect();
                let a = l[a0..fim].iter().fold(0.0f64, |m, x| m.max(x.abs()));
                let pico = |i: usize, w: usize| l[i..i + w].iter().fold(0.0f64, |m, x| m.max(x.abs()));
                // nenhum 1 ms mudo do início ao fim das fatias
                let mudo = (a0..fim - 48).step_by(48).find(|&i| pico(i, 48) < 0.02 * a);
                assert!(mudo.is_none(), "{rotulo}: mudo em {:?} ms do clipe", mudo.map(|i| (i - a0) / 48));
                // salto na emenda: a maior 1ª diferença do seno puro é a·2π·440/48000 (com o tom mantido)
                let seno = a * 2.0 * std::f64::consts::PI * 440.0 / 48_000.0;
                let salto = emendas.iter().map(|&b| (b - 4..b + 4).map(|i| (l[i] - l[i - 1]).abs()).fold(0.0, f64::max)).fold(0.0, f64::max) / seno;
                // volume no cruzamento (picos de 3 ms): o `atempo` de cada lado não casa a fase (medido: 0,59)
                let vale = emendas.iter().map(|&b| (b - 240..b + 240).step_by(24).map(|i| pico(i, 144)).fold(f64::MAX, f64::min)).fold(f64::MAX, f64::min) / a;
                if manter_tom {
                    eprintln!("emendas {rotulo}: {} fatias, salto {salto:.2}× o do seno, vale {vale:.2}", fs.len());
                    assert!(salto <= 1.3, "{rotulo}: salto de {salto:.2}× o do seno na emenda");
                    assert!(vale >= 0.5, "{rotulo}: vale de {vale:.2} no cruzamento");
                    continue;
                }
                // exato: cada amostra é o seno da fonte no instante que a fatia dá a ela (taxa declarada `hz`); a primeira
                // da fatia sai do `aresample` sem o passado (medido: até 0,18 da escala a 16×, 1 amostra; no resto 0,037)
                let (mut erro, mut primeira): (f64, f64) = (0.0, 0.0);
                for (x, &b) in fs.iter().zip(&bordas) {
                    let ini = b - x.n as usize;
                    for i in ini..b.min(fim) {
                        let s = t0 + (x.de as f64 + (i - ini) as f64 * x.hz.unwrap_or(48_000) as f64 / 48_000.0) / 48_000.0;
                        let e = (l[i] - a * (2.0 * std::f64::consts::PI * 440.0 * s).sin()).abs() / a;
                        if i == ini { primeira = primeira.max(e) } else { erro = erro.max(e) }
                    }
                }
                eprintln!("emendas {rotulo}: {} fatias, erro máx {erro:.3} da escala (primeira amostra {primeira:.3})", fs.len());
                assert!(erro <= 0.05 && primeira <= 0.25, "{rotulo}: erro de {erro:.3} da escala (primeira amostra {primeira:.3})");
            }
        }
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

    // ------------------------------------------------------------ Fase 1 (P1.5)

    /// Valor s16 de um nível 0..1.
    fn nivel(v: f64) -> i32 {
        (v * 32_768.0).round() as i32
    }

    /// Maior degrau entre amostras vizinhas em [a, b).
    fn degrau(x: &[i16], a: usize, b: usize) -> i32 {
        x[a..b.min(x.len())].windows(2).map(|w| (w[1] as i32 - w[0] as i32).abs()).max().unwrap_or(0)
    }

    /// RMS em dBFS de [a, b).
    fn rms_db(x: &[i16], a: usize, b: usize) -> f64 {
        let s = &x[a..b];
        let m = s.iter().map(|&v| (v as f64 / 32_768.0).powi(2)).sum::<f64>() / s.len() as f64;
        10.0 * m.max(1e-24).log10()
    }

    /// Frequência (Hz) pela contagem de passagens por zero em [a, b).
    fn frequencia(x: &[i16], a: usize, b: usize) -> f64 {
        let n = x[a..b].windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count();
        n as f64 / 2.0 / ((b - a) as f64 / 48_000.0)
    }

    /// Roda a medição e a passada como o executor: auxiliares na pasta de trabalho, a linha da prévia, o ganho
    /// de `ganho_db` e o grafo com ele.
    fn rodar_medindo(cap: &Capacidades, passo: &PassoAudio, previas: &[Previa], pasta: &Path) -> (Pcm, Loudnorm, Option<f64>) {
        std::fs::create_dir_all(pasta).unwrap();
        for a in &passo.auxiliares {
            args::gravar_texto(pasta, &a.nome, &a.conteudo).unwrap();
        }
        let [Previa::Loudnorm { args: linha }] = previas else { panic!("uma medição: {previas:?}") };
        assert_eq!(&linha[..2], &CPUFLAGS.map(String::from)[..]);
        let cmd = comando(cap, pasta, "medir", linha.clone());
        let t = exec::rodar(&cmd, &AtomicBool::new(false)).expect("ffmpeg");
        exec::exigir_sucesso(&cmd, &t).expect("medição");
        let m = ler_loudnorm(&std::fs::read_to_string(&cmd.stderr).unwrap()).expect("loudnorm");
        let g = ganho_db(&m, passo.normalizar.as_ref());
        (rodar_com_ganho(cap, passo, pasta, None, g).0, m, g)
    }

    /// Volume do `audio.wav` da pasta, medido por fora do passo.
    fn medir_wav(cap: &Capacidades, pasta: &Path) -> Loudnorm {
        let a = texto(&["-cpuflags", "0", "-hide_banner", "-nostdin", "-y", "-loglevel", "verbose", "-i", SAIDA, "-af", LOUDNORM, "-f", "null", "-"]);
        let cmd = comando(cap, pasta, "conferir", a);
        let t = exec::rodar(&cmd, &AtomicBool::new(false)).expect("ffmpeg");
        exec::exigir_sucesso(&cmd, &t).expect("ebur128");
        ler_loudnorm(&std::fs::read_to_string(&cmd.stderr).unwrap()).unwrap()
    }

    fn mixagem(normalizar: Value, limitador: bool) -> crate::edicao::modelo::Mixagem {
        serde_json::from_value(json!({ "volume_original": 1.0, "abaixar": { "db": -12.0, "ataque_s": 0.3, "soltura_s": 0.5 }, "normalizar": normalizar, "limitador": limitador })).unwrap()
    }

    fn com_transicao(mut c: Value, tipo: &str, duracao: f64, som: &str) -> Value {
        c["transicao_entrada"] = json!({ "tipo": tipo, "duracao_s": duracao, "audio": som });
        c
    }

    #[test]
    fn intervalos_unidos_e_envelope() {
        // mínimo de 45 ms, corte no total, união quando as rampas se cruzam, fora do total cai
        assert_eq!(unir(vec![(48_000, 48_100), (10, 5), (200_000, 300_000)], 480, 480, 100_000), vec![(48_000, 50_160)]);
        assert_eq!(unir(vec![(5_000, 9_000), (1_000, 4_100), (20_000, 99_999_999)], 480, 480, 30_000), vec![(1_000, 9_000), (20_000, 30_000)]);
        assert_eq!(unir(vec![(1_000, 4_000), (4_961, 9_000)], 480, 480, 30_000), vec![(1_000, 4_000), (4_961, 9_000)]);
        assert_eq!(unir(vec![(1_000, 4_000), (4_960, 9_000)], 480, 480, 30_000), vec![(1_000, 9_000)]);
        // do meio: saída até o meio, entrada do meio em diante
        let f = envelope(&[(48_000, 96_000)], 480_000, 14_400, 24_000, 0.25);
        assert_eq!(
            f,
            vec![
                "asetnsamples=n=1024:p=0".to_string(),
                "afade=t=out:ss=33600:ns=14400:silence=0.250000:enable=gte(t\\,0.650000)*lt(t\\,1.500000)".to_string(),
                "afade=t=in:ss=96000:ns=24000:silence=0.250000:enable=gte(t\\,1.500000)*lte(t\\,2.550000)".to_string(),
            ]
        );
        // desde o começo: só a entrada; até o fim: só a saída; o clipe inteiro: a entrada depois do fim
        let f = envelope(&[(0, 4_800), (96_000, 100_000)], 100_000, 480, 480, 0.0);
        assert_eq!(f[1..], ["afade=t=in:ss=4800:ns=480:enable=lte(t\\,0.160000)".to_string(), "afade=t=out:ss=95520:ns=480:enable=gte(t\\,1.940000)".to_string()]);
        assert_eq!(envelope(&[(0, 100_000)], 100_000, 480, 480, 0.0)[1..], ["afade=t=in:ss=100000:ns=480:enable=lte(t\\,2.143333)".to_string()]);
        // rampa mais curta quando o intervalo começa antes dela caber
        assert!(envelope(&[(100, 9_000)], 100_000, 480, 480, 0.0)[1].starts_with("afade=t=out:ss=0:ns=100:enable=gte(t\\,0.000000)*lt(t\\,"));
    }

    #[test]
    fn curva_do_volume_e_arvore_sobre_t() {
        let ks: Vec<Chave<f64>> = serde_json::from_value(json!([
            { "t": 2.0, "v": 1.0 }, { "t": 4.0, "v": 0.2 }, { "t": 5.0, "v": 0.2, "interp": "suave" }, { "t": 6.0, "v": 1.5, "interp": "degrau" }, { "t": 6.5, "v": 0.5 }
        ]))
        .unwrap();
        let e = curva_do_volume(&ks, 1.0);
        let (desce, sobe, u) = (num(0.2 - 1.0), num(1.5 - 0.2), "((t-4)/1)");
        assert_eq!(e, format!("if(lt(t,4),if(lt(t,1),1,if(lt(t,3),1+{desce}*((t-1)/2),0.2)),if(lt(t,5),0.2+{sobe}*{u}*{u}*(3-2*{u}),if(lt(t,5.5),1.5,0.5)))"));
        // fora das chaves vale a ponta; chave repetida no mesmo instante não divide por zero
        let dupla: Vec<Chave<f64>> = serde_json::from_value(json!([{ "t": 1.0, "v": 0.5 }, { "t": 1.0, "v": 1.0 }, { "t": 2.0, "v": 1.0 }])).unwrap();
        assert_eq!(curva_do_volume(&dupla, 0.0), "if(lt(t,1),0.5,if(lt(t,2),1,1))");
        // 1 000 chaves: profundidade de árvore, não de fila
        let muitas: Vec<Chave<f64>> = (0..1000).map(|i| Chave { t: i as f64 * 0.01, pts: None, v: (i % 7) as f64 * 0.1, interp: Interp::Linear }).collect();
        let e = curva_do_volume(&muitas, 0.0);
        let fundo = e.bytes().scan(0i32, |d, b| { *d += i32::from(b == b'(') - i32::from(b == b')'); Some(*d) }).max().unwrap();
        assert!(fundo <= 16, "{fundo}");
    }

    #[test]
    fn grafo_da_fase_1() {
        let (cap, fontes) = (apoio::cap_falsa(), fontes_falsas());
        // a(30000/1001): sobreposição de 15 slots entre "a" e "b"
        let mut p = projeto(
            (30, 1),
            vec![
                trecho("a", "v1", 1.0, 3.0, 1.0, true),
                com_transicao(trecho("b", "v2", 4.0, 6.0, 1.0, true), "fundir", 0.5, "acompanhar"),
                com_transicao(trecho("c", "v1", 10.0, 12.0, 1.0, true), "deslizar_esq", 1.0, "cortar"),
            ],
            vec![trilha("t", 1.0, vec![clipe_audio("n", "n1", json!({ "ref": "clipe", "clipe": "b", "desloc": 0.5 }), 0.0, Some(1.0))])],
            vec![],
        );
        p.audios[0].abaixa_original = true;
        p.mixagem = mixagem(json!({ "lufs": -16.0, "pico_db": -1.5 }), true);
        let Tolerante::Conhecido(ClipePrincipal::Trecho(t)) = &mut p.principal[0] else { panic!() };
        t.audio.silencios = serde_json::from_value(json!([{ "id": "s1", "de": 1.5, "ate": 1.75, "modo": "tom", "motivo": "nome" }, { "id": "s2", "de": 2.0, "ate": 2.5, "modo": "mudo", "motivo": "" }])).unwrap();
        t.audio.volume = serde_json::from_value(json!([{ "t": 1.0, "v": 1.0 }, { "t": 2.0, "v": 0.5 }])).unwrap();
        let (passo, previas, linha) = montar_e_medir(&p, &fontes, &cap, &final_(&p));
        let g = &passo.grafo;
        assert_eq!(linha.clipes.iter().map(|c| (c.inicio_q, c.n_q, c.sobrepoe_q)).collect::<Vec<_>>(), vec![(0, 60, 0), (45, 60, 15), (75, 60, 30)]);
        // "a": volume animado, silêncios e tom antes do resto; o tom entra por amix
        assert!(g.contains("aresample=48000:async=1:first_pts=0,asetnsamples=n=256:p=0,volume=volume=if(lt(t\\,0)\\,1\\,if(lt(t\\,1)\\,1+(-0.5)*((t-0)/1)\\,0.5)):eval=frame,asetnsamples=n=1024:p=0,afade=t=out:ss=23520:ns=480:enable=gte(t\\,0.440000)*lt(t\\,0.625000),afade=t=in:ss=36000:ns=480:enable=gte(t\\,0.625000)*lte(t\\,0.810000),afade=t=out:ss=47520:ns=480:enable=gte(t\\,0.940000)*lt(t\\,1.250000),afade=t=in:ss=72000:ns=480:enable=gte(t\\,1.250000)*lte(t\\,1.560000)[s0]"), "{g}");
        assert!(g.contains("aevalsrc=exprs=0.125*sin(2*PI*1000*t):s=48000:n=1024,atrim=end_sample=12960,afade=t=in:ss=0:ns=480,afade=t=out:ss=12480:ns=480,aformat=sample_fmts=fltp,pan=stereo|c0=c0|c1=c0,adelay=delays=23520S:all=1[s0t0]"), "{g}");
        // "a" toca sozinho em [0, 72000) e com "b" em [72000, 96000): dois pedaços, a saída acompanha (rampa de 24000)
        assert!(g.contains("[s0][s0t0]amix=inputs=2:normalize=0:dropout_transition=0:duration=first,apad,atrim=end_sample=96000,asetpts=N/SR/TB,aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo,afade=t=out:ss=72000:ns=24000,asplit=2[c0s0][c0s1]"), "{g}");
        assert!(g.contains("[c0s0]atrim=start_sample=0:end_sample=72000,asetpts=N/SR/TB[p0]"), "{g}");
        assert!(g.contains("[c0s1]atrim=start_sample=72000:end_sample=96000,asetpts=N/SR/TB[c0q1]"), "{g}");
        // "b": entra acompanhando (24000) e sai cortando no meio dos 48000 da sobreposição com "c"
        assert!(g.contains("afade=t=in:ss=0:ns=24000,afade=t=out:ss=71520:ns=480,asplit=3[c1s0][c1s1][c1s2]"), "{g}");
        assert!(g.contains("[c0q1][c1q0]amix=inputs=2:normalize=0:dropout_transition=0:duration=first[p1]"), "{g}");
        assert!(g.contains("afade=t=in:ss=24000:ns=480,asplit=2[c2s0][c2s1]"), "{g}");
        assert!(g.contains("[p0][p1][p2][p3][p4]concat=n=5:v=0:a=1[pr]"), "{g}");
        // abaixar: a narração toca em [96000, 144000) do programa
        assert!(g.contains("[pr]asetnsamples=n=1024:p=0,afade=t=out:ss=81600:ns=14400:silence=0.251189:enable=gte(t\\,1.650000)*lt(t\\,2.500000),afade=t=in:ss=144000:ns=24000:silence=0.251189:enable=gte(t\\,2.500000)*lte(t\\,3.550000)[pb]"), "{g}");
        // ganho, limitador no pico do alvo e saída
        assert!(g.ends_with(&format!("[mi]volume={MARCA_GANHO}dB,aresample=192000,alimiter=limit=0.841395:attack=5:release=50:level=0:latency=1,aresample=48000,apad,atrim=end_sample=216000,asetpts=N/SR/TB,{FORMATO_SAIDA},asplit=2[a][ah]")), "{g}");
        // com o limitador o ganho não é limitado pelo pico medido
        assert_eq!(passo.normalizar, Some(Normalizar { lufs: -16.0, pico_db: PICO_COM_LIMITADOR_DB }));
        let m = Loudnorm { integrado: Some(-30.0), pico: Some(-3.0), faixa: 1.0, limiar: None };
        assert_eq!(ganho_db(&m, passo.normalizar.as_ref()), Some(14.0));
        // a medição: o mesmo grafo sem ganho nem limitador, e a linha inteira da prévia
        assert_eq!(passo.auxiliares[0].conteudo, format!("{};\n[mi]{LOUDNORM}[an]", g.rsplit_once(";\n").unwrap().0));
        let [Previa::Loudnorm { args: linha_medir }] = &previas[..] else { panic!("{previas:?}") };
        let fim = texto(&["-/filter_complex", "audio-medir.txt", "-map", "[an]", "-f", "null", "-"]);
        assert_eq!(&linha_medir[linha_medir.len() - fim.len()..], &fim[..]);
        assert_eq!((&linha_medir[..2], linha_medir.iter().filter(|x| *x == "-i").count()), (&CPUFLAGS.map(String::from)[..], 3));
        // prévia de uma janela: a passada é a da janela, a medição continua a do programa inteiro
        let (janela, previas_janela, _) = montar_e_medir(&p, &fontes, &cap, &Alvo::Previa { altura: 360, de_q: 100, ate_q: 120 });
        assert_eq!(janela.auxiliares, passo.auxiliares);
        assert_eq!(previas_janela, previas);
        assert!(janela.entradas.len() < passo.entradas.len() && janela.grafo.contains("atrim=start_sample=160000:end_sample=192000"));
        // sem limitador o alvo de pico é o do projeto
        p.mixagem.limitador = false;
        let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        assert_eq!(passo.normalizar, Some(Normalizar { lufs: -16.0, pico_db: -1.5 }));
        assert!(!passo.grafo.contains("alimiter"));
    }

    #[test]
    fn normalizacao_no_alvo() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-normalizar");
        // ruído rosa; e o mesmo, mais alto, com estalos de 0,2 ms a cada 2 s: pico que limita o ganho linear e quase
        // não pesa no volume
        let ruido = gerar(&cap, &pasta, "ruido.wav", &["-f", "lavfi", "-i", "anoisesrc=color=pink:amplitude=0.05:seed=7:r=48000:d=12", "-c:a", "pcm_s16le"]);
        let estalos = gerar(
            &cap,
            &pasta,
            "estalos.wav",
            &["-f", "lavfi", "-i", "anoisesrc=color=pink:amplitude=0.2:seed=7:r=48000:d=12", "-f", "lavfi", "-i", "aevalsrc=exprs=0.8*lt(mod(t+0.5\\,2)\\,0.0002):s=48000:d=12",
              "-filter_complex", "amix=inputs=2:normalize=0:duration=first", "-c:a", "pcm_s16le"],
        );
        let fontes = HashMap::from([("r".to_string(), info_som(&cap, &ruido)), ("e".to_string(), info_som(&cap, &estalos))]);
        let narrado = |midia: &str, mix: crate::edicao::modelo::Mixagem| {
            let c = clipe_audio("x", midia, json!({ "ref": "programa", "t": 0.0 }), 0.0, Some(12.0));
            let mut p = projeto((30, 1), vec![imagem("i", 12.0)], vec![trilha("t", 1.0, vec![c])], vec![]);
            p.mixagem = mix;
            p
        };
        // (mídia, alvo, limitador, o pico limita o ganho?)
        let casos = [("r", (-23.0, -1.0), false, false), ("r", (-16.0, -1.5), false, false), ("e", (-16.0, -1.5), false, true), ("e", (-16.0, -1.5), true, false)];
        for (k, (midia, (lufs, pico), limitador, limitado)) in casos.into_iter().enumerate() {
            let p = narrado(midia, mixagem(json!({ "lufs": lufs, "pico_db": pico }), limitador));
            let (passo, previas, _) = montar_e_medir(&p, &fontes, &cap, &final_(&p));
            let dir = pasta.join(format!("n{k}"));
            let (pcm, medido, ganho) = rodar_medindo(&cap, &passo, &previas, &dir);
            assert_eq!(pcm.n() as u64, passo.amostras);
            let (i0, tp0) = (medido.integrado.expect("integrado"), medido.pico.expect("pico"));
            let ganho = ganho.expect("ganho");
            let saida = medir_wav(&cap, &dir);
            let (i1, tp1) = (saida.integrado.unwrap(), saida.pico.unwrap());
            eprintln!("{midia} alvo {lufs}/{pico} limitador {limitador}: medido {i0} LUFS {tp0} dBTP, ganho {ganho:.2} dB, saída {i1} LUFS {tp1} dBTP");
            if limitado {
                // o pico manda: fica abaixo do alvo de volume, com o pico no alvo
                assert!((ganho - (pico - tp0)).abs() < 1e-9 && ganho < lufs - i0 - 1.0, "ganho {ganho}");
                assert!((i1 - (i0 + ganho)).abs() <= 0.5 && i1 < lufs - 1.0, "saída {i1}");
                assert!((tp1 - pico).abs() <= 0.2, "pico {tp1}");
            } else {
                assert!((ganho - (lufs - i0)).abs() < 1e-9, "ganho {ganho}");
                assert!((i1 - lufs).abs() <= 0.5, "saída {i1} LUFS, alvo {lufs}");
                assert!(tp1 <= pico + 0.2, "pico {tp1}");
            }
        }
        // sem normalização: só mede, o som não muda
        let p = narrado("r", mixagem(Value::Null, false));
        let (passo, previas, _) = montar_e_medir(&p, &fontes, &cap, &final_(&p));
        let (pcm, medido, ganho) = rodar_medindo(&cap, &passo, &previas, &pasta.join("so-mede"));
        assert_eq!(ganho, None);
        assert!(medido.integrado.is_some_and(|i| i < -25.0));
        let original = rodar_pedaco(&cap, &pasta, "original", Some(&ruido), "pan=stereo|c0=c0|c1=c0,aformat=sample_fmts=fltp:channel_layouts=stereo", None);
        assert!(pcm.x == original, "sem normalizar o PCM é o da fonte");
        // prévia de uma janela com normalização: o ganho é o do programa inteiro
        let p = narrado("r", mixagem(json!({ "lufs": -23.0, "pico_db": -1.0 }), false));
        let (inteiro, previas, _) = montar_e_medir(&p, &fontes, &cap, &final_(&p));
        let (todo, _, g_todo) = rodar_medindo(&cap, &inteiro, &previas, &pasta.join("inteiro"));
        let (janela, previas, _) = montar_e_medir(&p, &fontes, &cap, &Alvo::Previa { altura: 360, de_q: 60, ate_q: 90 });
        let (parte, _, g_parte) = rodar_medindo(&cap, &janela, &previas, &pasta.join("janela"));
        assert_eq!(g_parte, g_todo);
        assert!(parte.x == todo.x[2 * 96_000..2 * 144_000], "a prévia é a fatia do final normalizado");
    }

    #[test]
    fn limitador_segura_o_pico_e_nao_mexe_no_que_esta_abaixo() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-limitador");
        // 2 s a 0,3 e 2 s a 0,95 (440 Hz), sem normalização: teto de −1 dBTP
        let n = gerar(&cap, &pasta, "tom.wav", &["-f", "lavfi", "-i", "aevalsrc=exprs=(0.3+0.65*gte(t\\,2))*sin(2*PI*440*t):s=48000:d=4", "-c:a", "pcm_s16le"]);
        let fontes = HashMap::from([("n".to_string(), info_som(&cap, &n))]);
        let mut p = projeto_narrado(4.0);
        let sem = rodar(&cap, &montar_com(&p, &fontes, &cap, &final_(&p)).unwrap().0, &pasta.join("sem")).canal(0);
        p.mixagem = mixagem(Value::Null, true);
        let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        let pcm = rodar(&cap, &passo, &pasta.join("com"));
        let com = pcm.canal(0);
        assert_eq!((com.len(), sem.len()), (192_000, 192_000));
        let pico = |x: &[i16]| x.iter().map(|v| v.unsigned_abs()).max().unwrap() as f64 / 32_768.0;
        let teto = 10f64.powf(LIMITE_PADRAO_DB / 20.0);
        eprintln!("limitador: pico {:.4} → {:.4} (teto {teto:.4})", pico(&sem[100_000..]), pico(&com[100_000..]));
        assert!(pico(&sem[100_000..]) > 0.94);
        assert!(pico(&com[100_000..]) <= teto * 1.01 && pico(&com[100_000..]) > teto * 0.9, "{}", pico(&com[100_000..]));
        // abaixo do teto: igual ao som sem limitador, na mesma amostra (atraso compensado)
        let d = sem[2_000..90_000].iter().zip(&com[2_000..90_000]).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
        assert!(d <= 3, "diferença {d}");
        // PCM igual nos dois binários
        if let Some(bins) = dois_binarios() {
            let h: Vec<String> = bins.iter().map(|b| rodar(b, &passo, &pasta.join(format!("b-{}", b.versao))).sha).collect();
            assert_eq!(h[0], h[1], "limitador: PCM diferente nos dois binários");
        }
    }

    #[test]
    fn abaixar_12_db_com_rampas() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-abaixar");
        let src = com_video(&cap, &pasta, "tom.mkv", ("30", 14.0), &[], "aevalsrc=exprs=0.5:s=48000:c=mono:d=14", &["-c:a", "pcm_s16le"]);
        // narração de silêncio digital: o que se mede é só o original
        let fala = gerar(&cap, &pasta, "fala.wav", &["-f", "lavfi", "-i", "anullsrc=r=48000:cl=mono:d=4", "-c:a", "pcm_s16le"]);
        let fontes = HashMap::from([("v".to_string(), info(&cap, &src)), ("n".to_string(), info_som(&cap, &fala))]);
        // no começo do programa (1 s) e de 3 a 7 s
        let clipes = vec![
            clipe_audio("x", "n", json!({ "ref": "programa", "t": 0.0 }), 0.0, Some(1.0)),
            clipe_audio("y", "n", json!({ "ref": "programa", "t": 3.0 }), 0.0, Some(4.0)),
        ];
        let mut p = projeto((30, 1), vec![trecho("a", "v", 1.0, 13.0, 1.0, true)], vec![trilha("t", 1.0, clipes)], vec![]);
        let sem = rodar(&cap, &montar_com(&p, &fontes, &cap, &final_(&p)).unwrap().0, &pasta.join("sem")).canal(0);
        assert!(sem.iter().all(|&x| x as i32 == nivel(0.5)), "sem abaixar o original fica inteiro");
        p.audios[0].abaixa_original = true;
        let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        let l = rodar(&cap, &passo, &pasta.join("com")).canal(0);
        assert_eq!(l.len(), 12 * 48_000);
        let cheio = nivel(0.5) as f64;
        let ganho = |i: usize| l[i] as f64 / cheio;
        let db = |i: usize| 20.0 * ganho(i).log10();
        let piso = 10f64.powf(-12.0 / 20.0);
        let s = |t: f64| (t * 48_000.0).round() as usize;
        // desde a amostra 0 no piso; sobe em 0,5 s depois de 1 s; cheio até 0,3 s antes de 3 s; −12 dB de 3 a 7 s; sobe em 0,5 s
        for (de, ate, esperado) in [(0.0, 1.0, piso), (1.5, 2.7, 1.0), (3.0, 7.0, piso), (7.5, 12.0, 1.0)] {
            let pior = (s(de)..s(ate)).map(|i| (ganho(i) - esperado).abs()).fold(0.0, f64::max);
            assert!(pior <= 0.0005, "{de}..{ate} s: ganho fora de {esperado} por {pior}");
        }
        assert!((db(s(5.0)) + 12.0).abs() <= 0.02, "{} dB", db(s(5.0)));
        // rampas lineares: no meio, a média entre cheio e piso
        let meio = (1.0 + piso) / 2.0;
        for t in [1.25, 2.85, 7.25] {
            assert!((ganho(s(t)) - meio).abs() <= 0.002, "{t} s: {}", ganho(s(t)));
        }
        // um quarto da descida (0,3 s) e da subida (0,5 s)
        assert!((ganho(s(2.775)) - (1.0 - (1.0 - piso) * 0.25)).abs() <= 0.002 && (ganho(s(7.125)) - (piso + (1.0 - piso) * 0.25)).abs() <= 0.002);
        // nenhum degrau: a rampa mais inclinada anda 0,85 de um nível por amostra
        let maior = degrau(&l, 0, l.len());
        eprintln!("abaixar: {:.2} dB de 3 a 7 s; maior degrau entre amostras {maior} de 32768", db(s(5.0)));
        assert!(maior <= 2, "degrau de {maior}");
    }

    /// Método de `p1b/out/03c-afade.log`: mudo, tom e mudo curto sobre um seno; degrau nas bordas igual ao do
    /// sinal, silêncio digital, tom de 1 kHz a −18 dBFS, e os intervalos no lugar depois da velocidade.
    #[test]
    fn tom_e_mudo_sem_estalo() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-silenciar");
        // seno de 317 Hz, pico −9 dBFS
        let src = com_video(&cap, &pasta, "seno.mkv", ("30", 14.0), &[], "aevalsrc=exprs=0.3548*sin(2*PI*317*t):s=48000:c=mono:d=14", &["-c:a", "pcm_s16le"]);
        let fontes = HashMap::from([("v".to_string(), info(&cap, &src))]);
        // em tempo de FONTE (o trecho começa em 1 s): mudo 6-8, tom 9-10, mudo curto 11,00-11,05. O tom acaba 12
        // amostras depois de 10 s: fora do cruzamento por zero dele (sem a rampa do tom, degrau de 4096)
        let silencios = json!([
            { "id": "s1", "de": 6.0, "ate": 8.0, "modo": "mudo", "motivo": "nome" },
            { "id": "s2", "de": 9.0, "ate": 10.00025, "modo": "tom", "motivo": "nome" },
            { "id": "s3", "de": 11.0, "ate": 11.05, "modo": "mudo", "motivo": "" }
        ]);
        let com = |id: &str, fator: f64, tom: bool| {
            let mut c = trecho(id, "v", 1.0, 13.0, fator, tom);
            c["audio"]["silencios"] = silencios.clone();
            c
        };
        let p = projeto((30, 1), vec![com("a", 1.0, true), com("b", 2.0, true), com("c", 2.0, false)], vec![], vec![]);
        let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        let pcm = rodar(&cap, &passo, &pasta.join("saida"));
        let l = pcm.canal(0);
        assert_eq!(l, pcm.canal(1));
        assert_eq!(l.len(), (12 + 6 + 6) * 48_000);
        let s = |t: f64| (t * 48_000.0).round() as usize;
        let janela = |t: f64| degrau(&l, s(t - 0.03), s(t + 0.03));
        // 1×: no tempo do clipe, 5-7, 8-9 e 10,00-10,05
        let referencia = degrau(&l, 0, 48_000);
        let do_tom = (0.125 * 2.0 * std::f64::consts::PI * 1000.0 / 48_000.0 * 32_768.0).ceil() as i32;
        assert!((referencia - 485).abs() <= 2, "{referencia}");
        for (t, teto) in [(5.0, referencia), (7.0, referencia), (8.0, do_tom), (9.0, do_tom), (10.0, referencia), (10.05, referencia)] {
            assert!(janela(t) <= teto + 2, "estalo em {t} s: degrau {} (o do sinal é {teto})", janela(t));
        }
        assert!(l[s(5.0)..s(7.0)].iter().all(|&x| x == 0) && l[s(10.0)..s(10.05)].iter().all(|&x| x == 0), "silêncio digital");
        for (de, ate) in [(4.0, 4.9), (7.02, 7.9), (9.02, 9.9), (10.07, 11.9)] {
            assert!((rms_db(&l, s(de), s(ate)) + 12.0).abs() <= 0.1 && (frequencia(&l, s(de), s(ate)) - 317.0).abs() <= 2.0, "{de}..{ate}: {} dB", rms_db(&l, s(de), s(ate)));
        }
        let pico = l[s(8.0)..s(9.0)].iter().map(|x| x.unsigned_abs()).max().unwrap() as f64 / 32_768.0;
        let (rms, hz) = (rms_db(&l, s(8.0), s(9.0)), frequencia(&l, s(8.02), s(8.98)));
        eprintln!("1×: degrau do sinal {referencia}, nas bordas {:?}; tom {hz:.1} Hz, pico {:.2} dBFS, RMS {rms:.2} dBFS", [5.0, 7.0, 8.0, 9.0, 10.0, 10.05].map(janela), 20.0 * pico.log10());
        assert!((20.0 * pico.log10() + 18.06).abs() <= 0.05 && (rms + 21.07).abs() <= 0.1 && (hz - 1000.0).abs() <= 2.0);
        // 2× com `atempo` (clipe "b", de 12 a 18 s): os intervalos caem na metade do tempo, o tom segue em 1 kHz
        let b = 12.0;
        assert!(l[s(b + 2.52)..s(b + 3.48)].iter().all(|&x| x == 0), "mudo depois do atempo");
        let (rms, hz) = (rms_db(&l, s(b + 4.03), s(b + 4.47)), frequencia(&l, s(b + 4.03), s(b + 4.47)));
        assert!((rms + 21.07).abs() <= 0.2 && (hz - 1000.0).abs() <= 3.0, "atempo: tom {hz} Hz, {rms} dB");
        // 2× exato (clipe "c", de 18 a 24 s): mesmas amostras de silêncio; o tom sobe uma oitava com o som
        let c = 18.0;
        assert!(l[s(c + 2.51)..s(c + 3.49)].iter().all(|&x| x == 0) && l[s(c + 5.005)..s(c + 5.02)].iter().all(|&x| x == 0), "mudo no modo exato");
        let hz = frequencia(&l, s(c + 4.02), s(c + 4.48));
        assert!((hz - 2000.0).abs() <= 4.0 && (rms_db(&l, s(c + 4.02), s(c + 4.48)) + 21.07).abs() <= 0.2, "exato: {hz} Hz");
        assert!(janela(c + 2.5) <= 2 * referencia + 4 && janela(c + 3.5) <= 2 * referencia + 4);
        // o som de fora dos intervalos é o da fonte, amostra por amostra
        let p0 = projeto((30, 1), vec![trecho("a", "v", 1.0, 13.0, 1.0, true)], vec![], vec![]);
        let limpo = rodar(&cap, &montar_com(&p0, &fontes, &cap, &final_(&p0)).unwrap().0, &pasta.join("limpo")).canal(0);
        for (de, ate) in [(0.0, 4.9), (7.1, 7.9), (9.1, 9.9), (10.2, 12.0)] {
            assert!(l[s(de)..s(ate)] == limpo[s(de)..s(ate)], "{de}..{ate} s mudou");
        }
        // silêncio que cobre o começo e o fim do clipe: zero da primeira à última amostra
        let mut todo = trecho("a", "v", 2.0, 4.0, 1.0, true);
        todo["audio"]["silencios"] = json!([{ "id": "s", "de": 0.0, "ate": 2.5, "modo": "mudo", "motivo": "" }, { "id": "z", "de": 3.5, "ate": 99.0, "modo": "tom", "motivo": "" }]);
        let p = projeto((30, 1), vec![todo], vec![], vec![]);
        let l = rodar(&cap, &montar_com(&p, &fontes, &cap, &final_(&p)).unwrap().0, &pasta.join("pontas")).canal(0);
        assert_eq!(l.len(), 96_000);
        assert!(l[..24_000].iter().all(|&x| x == 0), "começo");
        assert!((rms_db(&l, 28_800, 67_200) + 12.0).abs() <= 0.1, "meio");
        // o tom vai até o fim do clipe, sem o som da fonte por baixo
        assert!((frequencia(&l, 73_000, 95_000) - 1000.0).abs() <= 3.0 && (rms_db(&l, 73_000, 95_000) + 21.07).abs() <= 0.1, "{} dB", rms_db(&l, 73_000, 95_000));
    }

    #[test]
    fn volume_animado() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-volume");
        let src = com_video(&cap, &pasta, "tom.mkv", ("30", 8.0), &[], "aevalsrc=exprs=0.5:s=48000:c=mono:d=8", &["-c:a", "pcm_s16le"]);
        let n = gerar(&cap, &pasta, "narr.wav", &["-f", "lavfi", "-i", "aevalsrc=exprs=0.25:s=48000:c=mono:d=3", "-c:a", "pcm_s16le"]);
        let fontes = HashMap::from([("v".to_string(), info(&cap, &src)), ("n".to_string(), info_som(&cap, &n))]);
        // trecho: chaves em tempo de FONTE (linear, parado, suave, degrau)
        let chaves = json!([{ "t": 2.0, "v": 1.0 }, { "t": 4.0, "v": 0.2 }, { "t": 5.0, "v": 0.2, "interp": "suave" }, { "t": 6.0, "v": 1.5, "interp": "degrau" }, { "t": 6.5, "v": 0.5 }]);
        let mut c = trecho("a", "v", 1.0, 7.0, 1.0, true);
        c["audio"]["volume"] = chaves.clone();
        let p = projeto((30, 1), vec![c], vec![], vec![]);
        let l = rodar(&cap, &montar_com(&p, &fontes, &cap, &final_(&p)).unwrap().0, &pasta.join("trecho")).canal(0);
        assert_eq!(l.len(), 6 * 48_000);
        let curva: Animavel<f64> = serde_json::from_value(chaves).unwrap();
        // o volume vale por quadro de 256 amostras, no instante do começo dele
        let esperado = |i: usize, zero: f64, base: f64| nivel(base * crate::edicao::curva::avaliar(&curva, zero + (i / 256 * 256) as f64 / 48_000.0));
        let pior = (0..l.len()).map(|i| (l[i] as i32 - esperado(i, 1.0, 0.5)).abs()).max().unwrap();
        eprintln!("volume animado no trecho: pior diferença para a curva {pior} de 32768; maior degrau {}", degrau(&l, 0, 5 * 48_000));
        assert!(pior <= 2, "{pior}");
        assert_eq!((l[0] as i32, l[s_(3.5)] as i32, l[s_(5.2)] as i32, l[s_(5.9)] as i32), (nivel(0.5), nivel(0.1), nivel(0.75), nivel(0.25)));
        // nas rampas o degrau é o de um quadro de 256 amostras (a mais inclinada, a suave, anda 1,95 por segundo)
        assert!(degrau(&l, 0, s_(4.99)) <= 180, "{}", degrau(&l, 0, s_(4.99)));
        // narração: chaves em tempo LOCAL do clipe, com o volume da trilha por cima
        let mut x = clipe_audio("x", "n", json!({ "ref": "programa", "t": 1.0 }), 0.5, Some(2.5));
        x["volume"] = json!([{ "t": 0.5, "v": 1.0 }, { "t": 1.5, "v": 0.0 }]);
        let p = projeto((30, 1), vec![imagem("i", 4.0)], vec![trilha("t", 0.5, vec![x])], vec![]);
        let l = rodar(&cap, &montar_com(&p, &fontes, &cap, &final_(&p)).unwrap().0, &pasta.join("narracao")).canal(0);
        let local: Animavel<f64> = serde_json::from_value(json!([{ "t": 0.5, "v": 1.0 }, { "t": 1.5, "v": 0.0 }])).unwrap();
        assert!(l[..48_000].iter().all(|&v| v == 0) && l[3 * 48_000..].iter().all(|&v| v == 0));
        let pior = (0..96_000).map(|i| (l[48_000 + i] as i32 - nivel(0.25 * 0.5 * crate::edicao::curva::avaliar(&local, (i / 256 * 256) as f64 / 48_000.0))).abs()).max().unwrap();
        assert!(pior <= 2, "narração: {pior}");
        // quadro que começa em 0,96 s do clipe: 1 − 0,46 do volume
        assert_eq!((l[48_000] as i32, l[48_000 + 180 * 256] as i32, l[48_000 + s_(1.6)] as i32), (nivel(0.125), nivel(0.125 * 0.54), 0));
    }

    fn s_(t: f64) -> usize {
        (t * 48_000.0).round() as usize
    }

    /// Fontes de nível constante: "a" a 0,5 e "b" a 0,25, 8 s cada, vídeo a `fps`.
    fn dois_niveis(cap: &Capacidades, pasta: &Path, fps: &str) -> HashMap<String, FonteInfo> {
        let a = com_video(cap, pasta, "a.mkv", (fps, 8.0), &[], "aevalsrc=exprs=0.5:s=48000:c=mono:d=8", &["-c:a", "pcm_s16le"]);
        let b = com_video(cap, pasta, "b.mkv", (fps, 8.0), &[], "aevalsrc=exprs=0.25:s=48000:c=mono:d=8", &["-c:a", "pcm_s16le"]);
        HashMap::from([("a".to_string(), info(cap, &a)), ("b".to_string(), info(cap, &b))])
    }

    /// O som das transições programado contra `juncao::Peca` (O3): as peças são montadas à mão, com a sobreposição
    /// em quadros, e o PCM tem de cair nas fronteiras de amostra delas, nas três taxas.
    #[test]
    fn transicao_com_sobreposicao_amostras_exatas() {
        use crate::edicao::motor::juncao::Peca;
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-transicao");
        let t = |tipo: &str, d: f64, som: AudioTransicao| Transicao { tipo: tipo.into(), duracao_s: d, audio: som };
        // (taxa do vídeo e da saída, slots de cada clipe, sobreposição do 2º e do 3º, entrada do 1º e fecho)
        for (k, (fps, nome, n_q, sob, pontas)) in [((30, 1), "30", 60u32, [15u32, 12], [9u64, 6]), ((30000, 1001), "30000/1001", 60, [15, 12], [9, 6]), ((25, 1), "25", 50, [13, 10], [8, 5])].into_iter().enumerate() {
            let dir = pasta.join(format!("f{k}"));
            std::fs::create_dir_all(&dir).unwrap();
            let fontes = dois_niveis(&cap, &dir, nome);
            let fr = racional(fps);
            let quadro = fps.1 as f64 / fps.0 as f64;
            // as peças do vídeo, como o `motor/mod.rs` as entrega à junção
            let (fundir, deslizar) = (t("fundir", 0.5, AudioTransicao::Acompanhar), t("deslizar_esq", 0.4, AudioTransicao::Cortar));
            let (abre, fecha) = (t("fundir", 0.3, AudioTransicao::Acompanhar), t("fundir", 0.2, AudioTransicao::Cortar));
            let peca = |clipe: &str, sobrepoe: u32, entrada: &Transicao, saida: &Transicao, d: (u32, u32)| Peca {
                rotulo: format!("{clipe}v"),
                clipe: clipe.into(),
                n_q,
                de: 0,
                ate: n_q,
                sobrepoe,
                entrada: Some((entrada.clone(), d.0)),
                saida: Some((saida.clone(), d.1)),
            };
            let pecas = [
                peca("c0", 0, &abre, &fundir, (pontas[0] as u32, sob[0])),
                peca("c1", sob[0], &fundir, &deslizar, (sob[0], sob[1])),
                peca("c2", sob[1], &deslizar, &fecha, (sob[1], pontas[1] as u32)),
            ];
            // começo de cada peça em slots: soma das anteriores menos as sobreposições até ela
            let inicio: Vec<u64> = (0..3).map(|i| pecas[..i].iter().map(|p| p.n_q as u64).sum::<u64>() - pecas[..=i].iter().map(|p| p.sobrepoe as u64).sum::<u64>()).collect();
            let total_q = inicio[2] + n_q as u64;
            // o projeto que dá essas peças: cada trecho com `n_q` quadros da fonte, na taxa dela
            let fim = 1.0 + n_q as f64 * quadro;
            let mut p = projeto(
                fps,
                vec![
                    com_transicao(trecho("c0", "a", 1.0, fim, 1.0, true), &abre.tipo, abre.duracao_s, "acompanhar"),
                    com_transicao(trecho("c1", "b", 1.0, fim, 1.0, true), &fundir.tipo, fundir.duracao_s, "acompanhar"),
                    com_transicao(trecho("c2", "a", 3.0, 2.0 + fim, 1.0, true), &deslizar.tipo, deslizar.duracao_s, "cortar"),
                ],
                vec![],
                vec![],
            );
            p.fecho = Some(fecha.clone());
            let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
            assert_eq!(linha.clipes.iter().map(|c| (c.inicio_q, c.n_q, c.sobrepoe_q)).collect::<Vec<_>>(), (0..3).map(|i| (inicio[i], n_q, pecas[i].sobrepoe)).collect::<Vec<_>>(), "{fps:?}");
            assert_eq!(linha.total_q, total_q);
            let pcm = rodar(&cap, &passo, &dir.join("saida"));
            let l = pcm.canal(0);
            let a = |q: u64| tempo::amostra_do_slot(q, fr) as usize;
            assert_eq!((l.len(), passo.amostras as usize), (a(total_q), a(total_q)), "{fps:?}: amostras");
            assert_eq!(l, pcm.canal(1));
            let (na, nb) = (nivel(0.5) as f64, nivel(0.25) as f64);
            // valor esperado em cada amostra, pelas peças
            let mut esperado = vec![0f64; l.len()];
            let soma = |v: &mut Vec<f64>, de: usize, ate: usize, f: &dyn Fn(usize) -> f64| (de..ate).for_each(|i| v[i] += f(i - de));
            // c0: entra do fundo em `pontas[0]` slots, cheio, e sai cruzando com c1
            let (x0, x1) = (a(inicio[1]), a(inicio[1] + sob[0] as u64));
            let r0 = a(pontas[0]);
            soma(&mut esperado, 0, r0, &|i| na * i as f64 / r0 as f64);
            soma(&mut esperado, r0, x0, &|_| na);
            soma(&mut esperado, x0, x1, &|i| na * ((x1 - x0) - i) as f64 / (x1 - x0) as f64);
            // c1: entra cruzando, cheio até 10 ms antes do meio da sobreposição com c2, e corta
            let (y0, y1) = (a(inicio[2]), a(inicio[2] + sob[1] as u64));
            let meio = y0 + (y1 - y0) / 2;
            soma(&mut esperado, x0, x1, &|i| nb * i as f64 / (x1 - x0) as f64);
            soma(&mut esperado, x1, meio - 480, &|_| nb);
            soma(&mut esperado, meio - 480, meio, &|i| nb * (480 - i) as f64 / 480.0);
            // c2: do meio em diante, cheio, e o fecho cortando: 10 ms no fim
            let fim = l.len();
            soma(&mut esperado, meio, meio + 480, &|i| na * i as f64 / 480.0);
            soma(&mut esperado, meio + 480, fim - 480, &|_| na);
            soma(&mut esperado, fim - 480, fim, &|i| na * (480 - i) as f64 / 480.0);
            let pior = (0..l.len()).map(|i| (l[i] as f64 - esperado[i]).abs()).fold(0.0, f64::max);
            eprintln!("{fps:?}: {} slots, {} amostras; sobreposições em {x0}..{x1} e {y0}..{y1} (troca em {meio}); pior diferença {pior:.2} de 32768", total_q, l.len());
            assert!(pior <= 1.5, "{fps:?}: {pior}");
            // o meio da sobreposição cruzada é a mistura dos dois lados
            let m = x0 + (x1 - x0) / 2;
            assert!((l[m] as f64 - (na + nb) / 2.0).abs() <= 2.0, "{}", l[m]);
            assert_eq!((l[x0 - 1] as i32, l[x1 + 1] as i32), (nivel(0.5), nivel(0.25)));
            // PCM igual nos dois binários
            if let Some(bins) = dois_binarios() {
                let h: Vec<String> = bins.iter().map(|b| rodar(b, &passo, &dir.join(format!("b-{}", b.versao))).sha).collect();
                assert_eq!(h[0], h[1], "{fps:?}");
            }
        }
    }

    #[test]
    fn cortar_troca_o_som_no_meio() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-cortar");
        let fontes = dois_niveis(&cap, &pasta, "30");
        let projeto_com = |som: &str| projeto((30, 1), vec![trecho("c0", "a", 1.0, 4.0, 1.0, true), com_transicao(trecho("c1", "b", 1.0, 4.0, 1.0, true), "fundir", 1.0, som)], vec![], vec![]);
        let rodar_com_som = |som: &str| {
            let p = projeto_com(som);
            let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
            // 30 slots de sobreposição: c1 começa no slot 60
            assert_eq!((linha.clipes[1].inicio_q, linha.clipes[1].sobrepoe_q, linha.total_q), (60, 30, 150));
            rodar(&cap, &passo, &pasta.join(som)).canal(0)
        };
        let (cortar, acompanhar) = (rodar_com_som("cortar"), rodar_com_som("acompanhar"));
        assert_eq!((cortar.len(), acompanhar.len()), (240_000, 240_000));
        // sobreposição em [96000, 144000); o meio é 120000
        let (ini, meio, fim) = (96_000usize, 120_000usize, 144_000usize);
        let (na, nb) = (nivel(0.5), nivel(0.25));
        // até 10 ms antes do meio só o som de quem sai; 10 ms depois do meio só o de quem entra
        assert!(cortar[..meio - 480].iter().all(|&x| x as i32 == na), "antes do meio");
        assert!(cortar[meio + 480..].iter().all(|&x| x as i32 == nb), "depois do meio");
        // no meio os dois estão em zero; 5 ms de cada lado, a meia rampa de cada um
        assert_eq!(cortar[meio], 0);
        assert!((cortar[meio - 240] as i32 - na / 2).abs() <= 1 && (cortar[meio + 240] as i32 - nb / 2).abs() <= 1, "{} {}", cortar[meio - 240], cortar[meio + 240]);
        // sem estalo: o degrau é o da rampa de 10 ms
        let maior = degrau(&cortar, 0, cortar.len());
        assert!(maior <= na / 480 + 2, "degrau {maior}");
        // "acompanhar" cruza a sobreposição inteira: no meio, a média
        assert_eq!((acompanhar[ini - 1] as i32, acompanhar[fim] as i32), (na, nb));
        assert!((acompanhar[meio] as i32 - (na + nb) / 2).abs() <= 1, "{}", acompanhar[meio]);
        assert!(degrau(&acompanhar, 0, acompanhar.len()) <= 2);
        eprintln!("cortar: {} até {}, 0 em {meio}, {} de {} em diante; acompanhar no meio {}", na, meio - 480, nb, meio + 480, acompanhar[meio]);
    }

    /// Média das amostras em [a, b).
    fn media(x: &[i16], a: usize, b: usize) -> f64 {
        x[a..b].iter().map(|&v| v as f64).sum::<f64>() / (b - a) as f64
    }

    /// Silenciar nas fontes que o gravador entrega: tempo absoluto (começa em 1 h), 8 kHz A-law e 5.1; intervalo
    /// colado no começo e no fim do clipe, tom vizinho de mudo. Mede o zero digital amostra a amostra e, sob o tom,
    /// a falta do nível contínuo da fonte (média zero em ciclos inteiros).
    #[test]
    fn silenciar_em_tempo_absoluto_8_khz_e_seis_canais() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-silenciar-fontes");
        let com = |mut c: Value, s: Value| {
            c["audio"]["silencios"] = s;
            c
        };
        let rodar_p = |p: &ProjetoEdicao, fontes: &HashMap<String, FonteInfo>, nome: &str| rodar(&cap, &montar_com(p, fontes, &cap, &final_(p)).unwrap().0, &pasta.join(nome)).canal(0);

        // 8 kHz A-law, começo em 1 h: nível contínuo de 0,5
        let h = com_video(&cap, &pasta, "h.mkv", ("30", 8.0), &[], "aevalsrc=exprs=0.5:s=8000:c=mono:d=8", &["-c:a", "pcm_alaw", "-output_ts_offset", "3600"]);
        let fonte = info(&cap, &h);
        assert_eq!((fonte.audio.as_ref().unwrap().inicio_s, fonte.audio.as_ref().unwrap().taxa), (3600.0, 8000));
        let fontes = HashMap::from([("h".to_string(), fonte)]);
        let sil = json!([{ "id": "m", "de": 3603.0, "ate": 3603.5, "modo": "mudo", "motivo": "" }, { "id": "t", "de": 3604.0, "ate": 3604.3, "modo": "tom", "motivo": "" }]);
        let p = projeto((30, 1), vec![com(trecho("a", "h", 3602.2, 3604.8, 1.0, true), sil.clone()), com(trecho("b", "h", 3602.2, 3604.8, 0.5, false), sil)], vec![], vec![]);
        let l = rodar_p(&p, &fontes, "h");
        let p0 = projeto((30, 1), vec![trecho("a", "h", 3602.2, 3604.8, 1.0, true), trecho("b", "h", 3602.2, 3604.8, 0.5, false)], vec![], vec![]);
        let limpo = rodar_p(&p0, &fontes, "h-limpo");
        assert_eq!((l.len(), limpo.len()), (3 * 124_800, 3 * 124_800));
        let cheio = media(&limpo, 20_000, 30_000);
        assert!((cheio - nivel(0.5) as f64).abs() < 400.0, "{cheio}");
        // 1×: mudo em [38400, 62400) e tom em [86400, 100800) do clipe; fora das rampas de 10 ms, o som da fonte
        assert!(l[38_400..62_400].iter().all(|&x| x == 0), "mudo");
        for (de, ate) in [(0, 37_920), (62_880, 85_920), (101_280, 124_800)] {
            assert!(l[de..ate] == limpo[de..ate], "{de}..{ate} mudou");
        }
        assert!((l[38_160] as f64 - cheio / 2.0).abs() < 40.0 && (l[62_640] as f64 - cheio / 2.0).abs() < 40.0, "meia rampa: {} {}", l[38_160], l[62_640]);
        let m = media(&l, 86_400, 100_800);
        assert!(m.abs() <= 2.0 && (rms_db(&l, 86_400, 100_800) + 21.07).abs() <= 0.1 && (frequencia(&l, 86_400, 100_800) - 1000.0).abs() <= 2.0, "tom: média {m}");
        // 0,5× exato: o dobro das amostras, a partir de 124800
        let b = 124_800;
        assert!(l[b + 76_800 + 64..b + 124_800 - 64].iter().all(|&x| x == 0), "mudo a 0,5×");
        assert!((media(&l, b + 60_000, b + 70_000) - cheio).abs() < 40.0 && (media(&l, b + 130_000, b + 160_000) - cheio).abs() < 40.0);
        let m = media(&l, b + 172_800 + 96, b + 201_600 - 96);
        assert!(m.abs() <= 3.0 && (frequencia(&l, b + 172_800 + 96, b + 201_600 - 96) - 500.0).abs() <= 2.0, "tom a 0,5×: média {m}");

        // 5.1 com nível contínuo: tom, e mudo de 20 ms que vira o mínimo de 45 ms
        let seis = com_video(&cap, &pasta, "seis.mkv", ("30", 4.0), &[], "aevalsrc=exprs=0.3|0.3|0.3|0.3|0.3|0.3:c=5.1:s=48000:d=4", &["-c:a", "pcm_s16le"]);
        let fonte = info(&cap, &seis);
        assert_eq!(fonte.audio.as_ref().unwrap().canais, 6);
        let fontes = HashMap::from([("s".to_string(), fonte)]);
        let sil = json!([{ "id": "t", "de": 1.0, "ate": 1.5, "modo": "tom", "motivo": "" }, { "id": "m", "de": 2.0, "ate": 2.02, "modo": "mudo", "motivo": "" }]);
        let p = projeto((30, 1), vec![com(trecho("a", "s", 0.0, 3.0, 1.0, true), sil)], vec![], vec![]);
        let l = rodar_p(&p, &fontes, "seis");
        let p0 = projeto((30, 1), vec![trecho("a", "s", 0.0, 3.0, 1.0, true)], vec![], vec![]);
        let limpo = rodar_p(&p0, &fontes, "seis-limpo");
        assert!(limpo[1_000] != 0 && limpo[1_000..140_000].iter().all(|&x| x == limpo[1_000]), "nível contínuo");
        let m = media(&l, 48_000, 72_000);
        assert!(m.abs() <= 2.0 && (rms_db(&l, 48_000, 72_000) + 21.07).abs() <= 0.1, "tom no 5.1: média {m}");
        assert!(l[96_000..98_160].iter().all(|&x| x == 0), "mudo mínimo");
        for (de, ate) in [(0, 47_520), (72_480, 95_520), (98_640, 144_000)] {
            assert!(l[de..ate] == limpo[de..ate], "5.1: {de}..{ate} mudou");
        }

        // coladas nas pontas: tom 2 ms depois do começo do clipe, mudo vizinho dele, mudo que passa do fim
        let dc = com_video(&cap, &pasta, "dc.mkv", ("30", 5.0), &[], "aevalsrc=exprs=0.5:s=48000:c=mono:d=5", &["-c:a", "pcm_s16le"]);
        let fontes = HashMap::from([("d".to_string(), info(&cap, &dc))]);
        let sil = json!([
            { "id": "t", "de": 1.002, "ate": 1.2, "modo": "tom", "motivo": "" },
            { "id": "m", "de": 1.2, "ate": 1.5, "modo": "mudo", "motivo": "" },
            { "id": "v1", "de": 2.0, "ate": 2.2, "modo": "mudo", "motivo": "" },
            { "id": "v2", "de": 2.23, "ate": 2.4, "modo": "mudo", "motivo": "" },
            { "id": "z", "de": 2.99, "ate": 3.5, "modo": "mudo", "motivo": "" }
        ]);
        let p = projeto((30, 1), vec![com(trecho("a", "d", 1.0, 3.0, 1.0, true), sil)], vec![], vec![]);
        let l = rodar_p(&p, &fontes, "pontas");
        assert_eq!(l.len(), 96_000);
        // o tom ainda sobe nos primeiros 10 ms; o som da fonte (16384) já saiu na amostra 96
        let m = media(&l, 480, 9_600);
        assert!(m.abs() <= 2.0 && l[96..9_600].iter().all(|&x| x.unsigned_abs() <= 4_100), "tom colado no começo: média {m}");
        assert!(l[10_080..24_000].iter().all(|&x| x == 0) && l[95_520..].iter().all(|&x| x == 0), "mudos");
        // dois mudos a 30 ms um do outro (não se unem; os `enable` dos dois se cruzam): 10 ms de som inteiro no meio
        assert!(l[48_000..57_600].iter().all(|&x| x == 0) && l[59_040..67_200].iter().all(|&x| x == 0), "mudos vizinhos");
        assert!(l[58_080..58_560].iter().all(|&x| x as i32 == nivel(0.5)), "entre os mudos vizinhos: {}", l[58_320]);
        assert!(l[24_480..47_520].iter().all(|&x| x as i32 == nivel(0.5)) && l[67_680..95_040].iter().all(|&x| x as i32 == nivel(0.5)), "o resto fica inteiro");
    }

    /// "Da degravação…" marca dezenas de falas: 100 intervalos (tom e mudo alternados) num clipe só.
    #[test]
    fn cem_silencios_num_clipe() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-cem-silencios");
        let dc = com_video(&cap, &pasta, "dc.mkv", ("30", 124.0), &[], "aevalsrc=exprs=0.5:s=48000:c=mono:d=124", &["-c:a", "pcm_s16le"]);
        let fontes = HashMap::from([("d".to_string(), info(&cap, &dc))]);
        // em tempo de fonte: fala i de 2 + 1,1·i a 2,6 + 1,1·i (o clipe começa em 1 s)
        let fala = |i: usize| (2.0 + 1.1 * i as f64, 2.6 + 1.1 * i as f64);
        let sil: Vec<Value> = (0..100).map(|i| json!({ "id": format!("s{i}"), "de": fala(i).0, "ate": fala(i).1, "modo": if i % 2 == 0 { "tom" } else { "mudo" }, "motivo": "" })).collect();
        let mut c = trecho("a", "d", 1.0, 121.0, 1.0, true);
        c["audio"]["silencios"] = json!(sil);
        let p = projeto((30, 1), vec![c], vec![], vec![]);
        let (passo, _) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        let t0 = Instant::now();
        let l = rodar(&cap, &passo, &pasta.join("saida")).canal(0);
        eprintln!("100 silêncios em 120 s de som: {:.1} s", t0.elapsed().as_secs_f64());
        assert_eq!(l.len(), 120 * 48_000);
        let s = |t: f64| ((t - 1.0) * 48_000.0).round() as usize;
        for i in 0..100 {
            let (a, b) = (s(fala(i).0), s(fala(i).1));
            if i % 2 == 0 {
                let m = media(&l, a, b);
                assert!(m.abs() <= 2.0 && l[a..b].iter().all(|&x| x.unsigned_abs() <= 4_100) && (rms_db(&l, a, b) + 21.07).abs() <= 0.1, "tom {i}: média {m}");
            } else {
                assert!(l[a..b].iter().all(|&x| x == 0), "mudo {i}");
            }
            // entre uma fala e a seguinte (fora das rampas de 10 ms), o som da fonte
            let seguinte = if i < 99 { s(fala(i + 1).0) } else { l.len() };
            assert!(l[b + 480..seguinte - 480].iter().all(|&x| x as i32 == nivel(0.5)), "depois da fala {i}");
        }
    }

    /// Tudo da Fase 1 junto: a prévia de uma janela é a fatia exata do render final.
    #[test]
    fn previa_e_a_fatia_do_final_com_transicao_silencio_e_abaixar() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {}", cap.versao);
        let pasta = fontes::pasta_temporaria("audio-previa-f1");
        let som = format!("{SINAL}:s=48000:c=stereo:d=12");
        let src = com_video(&cap, &pasta, "cliques.mkv", ("30", 12.0), &[], &som, &["-c:a", "pcm_s16le"]);
        let n = gerar(&cap, &pasta, "narr.wav", &["-f", "lavfi", "-i", "sine=f=700:r=48000:d=3", "-c:a", "pcm_s16le"]);
        let fontes = HashMap::from([("v".to_string(), info(&cap, &src)), ("n".to_string(), info_som(&cap, &n))]);
        let mut a = trecho("a", "v", 1.0, 3.0, 1.0, true);
        a["audio"]["silencios"] = json!([{ "id": "s", "de": 1.5, "ate": 2.1, "modo": "tom", "motivo": "" }]);
        a["audio"]["volume"] = json!([{ "t": 1.0, "v": 0.4 }, { "t": 3.0, "v": 1.2 }]);
        let mut p = projeto(
            (30000, 1001),
            vec![
                a,
                com_transicao(trecho("b", "v", 6.0, 9.0, 2.0, true), "fundir", 0.5, "acompanhar"),
                com_transicao(imagem("i", 0.7), "fundir", 0.3, "cortar"),
                com_transicao(trecho("c", "v", 0.2, 1.4, 0.5, false), "deslizar_esq", 0.4, "cortar"),
            ],
            vec![trilha("t", 0.6, vec![clipe_audio("x", "n", json!({ "ref": "clipe", "clipe": "b", "desloc": 0.3 }), 0.5, Some(2.5))])],
            vec![],
        );
        p.audios[0].abaixa_original = true;
        let (passo, linha) = montar_com(&p, &fontes, &cap, &final_(&p)).unwrap();
        let todo = rodar(&cap, &passo, &pasta.join("final"));
        assert_eq!(todo.n() as u64, tempo::amostra_do_slot(linha.total_q, linha.fps));
        let a = |q| tempo::amostra_do_slot(q, linha.fps) as usize;
        // a narração toca nos slots 54 a 114: as janelas 40..52 e 116..125 só têm a descida e a subida do original
        assert_eq!(linha.livres.iter().map(|x| (x.inicio_q, x.n_q)).collect::<Vec<_>>(), vec![(54, 60)]);
        for (de_q, ate_q) in [(0, 20), (37, 101), (40, 52), (116, 125), (50, linha.total_q)] {
            let (previa, _) = montar_com(&p, &fontes, &cap, &Alvo::Previa { altura: 360, de_q, ate_q }).unwrap();
            let x = rodar(&cap, &previa, &pasta.join(format!("previa{de_q}")));
            assert!(x.x == todo.x[2 * a(de_q)..2 * a(ate_q)], "janela {de_q}..{ate_q}");
        }
        // PCM igual nos dois binários (O6), com tudo junto
        if let Some(bins) = dois_binarios() {
            let h: Vec<String> = bins.iter().map(|b| rodar(b, &passo, &pasta.join(format!("b-{}", b.versao))).sha).collect();
            assert_eq!(h[0], h[1], "PCM diferente nos dois binários");
        }
    }
}
