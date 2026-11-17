//! Junção dos clipes dentro do bloco e catálogo de transições (§5.4). Corte e `escurecer` por `fade`; as do `xfade`
//! por pedaços: cabeça, corpo e cauda de cada clipe, e só a sobreposição passa pelo `xfade`, plano a plano (o
//! `xfade` só aceita 4:4:4: em 4:2:0 o ffmpeg poria um `scale` sem `bitexact`). A entrada do primeiro clipe e o
//! `fecho` misturam com um clipe virtual da cor do fundo, dentro da duração do clipe. O som das transições é do
//! `audio.rs` (P1.5, O3), com os mesmos slots: `ClipeResolvido.sobrepoe_q` entre clipes e `blocos::escurecimentos`
//! na entrada do primeiro e no `fecho`. Dono: P1.1.

use super::grafo::{filtro, Grafo};
use super::plano::Canvas;
use crate::edicao::cor;
use crate::edicao::modelo::{Racional, Transicao};
use crate::error::{Result, SicroError};

/// Catálogo de transições aceitas (`Transicao.tipo`); fora dele = `nao_suportado` (validação e motor, a mesma lista).
/// Se a transição sobrepõe os clipes, quem diz é `tempo::transicao_sobrepoe` (todas menos `escurecer`).
pub const TRANSICOES: &[&str] = &["escurecer", "fundir", "cortina_esq", "cortina_dir", "deslizar_esq", "deslizar_dir", "circulo"];

/// Nome no `xfade` das que sobrepõem. Só entram as que saem iguais no 9.0.1 e no 7.0.2 e não leem fora do quadro
/// (medido: `slideleft` e `slideup` leem além da borda no primeiro quadro; `dissolve` e `radial` mudam entre versões).
const XFADE: &[(&str, &str)] = &[
    ("fundir", "fade"),
    ("cortina_esq", "wipeleft"),
    ("cortina_dir", "wiperight"),
    ("deslizar_esq", "coverleft"),
    ("deslizar_dir", "coverright"),
    ("circulo", "circleopen"),
];

/// Maior duração que o `xfade` aceita (s).
pub const XFADE_MAX_S: f64 = 60.0;

// CONTRATO: acréscimo (fecho da Fase 1): a validação acusa antes o que `misturar` recusa.
/// Transição que sobrepõe com mais slots do que o `xfade` aceita (`XFADE_MAX_S`)? As de `fade` (`escurecer`) não têm limite.
pub fn longa_demais(t: &Transicao, fps: Racional) -> bool {
    nome_xfade(&t.tipo).is_some() && fps.num > 0 && fps.den > 0 && !(crate::edicao::tempo::slots(t.duracao_s, fps) as f64 * fps.den as f64 / fps.num as f64 <= XFADE_MAX_S)
}

const ETIQUETA_709_TV: &str = "setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv";

fn nome_xfade(tipo: &str) -> Option<&'static str> {
    XFADE.iter().find(|(t, _)| *t == tipo).map(|(_, x)| *x)
}

/// Um clipe pronto para juntar: rótulo do vídeo, slots e as transições de entrada e de saída (em slots).
/// O vídeo chega com o pts = slot local do clipe (base 1/fps), que é como o `fade` acha o lugar dele mesmo quando
/// o bloco pega o clipe pelo meio; `n_q` = slots do clipe INTEIRO; o rótulo traz só os slots [de, ate) dele.
/// `sobrepoe` = slots em que este clipe se sobrepõe ao anterior (transição com sobreposição; 0 no corte, no
/// `escurecer` e no primeiro clipe). O bloco nunca corta dentro de uma transição (`blocos::dividir`).
// CONTRATO: `clipe`, `de`, `ate` e `sobrepoe` são da abertura da Fase 1 (a junção por pedaços do P1.1 precisa deles).
#[derive(Debug, Clone, PartialEq)]
pub struct Peca {
    pub rotulo: String,
    pub clipe: String,
    pub n_q: u32,
    pub de: u32,
    pub ate: u32,
    pub sobrepoe: u32,
    pub entrada: Option<(Transicao, u32)>,
    pub saida: Option<(Transicao, u32)>,
}

fn segundos(slots: u32, fps: Racional) -> String {
    // 9 casas: o `fade` e o `xfade` contam em microssegundos
    format!("{:.9}", slots as f64 * fps.den as f64 / fps.num as f64)
}

fn conferir(t: &Transicao) -> Result<()> {
    if TRANSICOES.contains(&t.tipo.as_str()) {
        Ok(())
    } else {
        Err(SicroError::Validation(format!("nao_suportado: transicao {}", t.tipo)))
    }
}

fn erro(detalhe: impl std::fmt::Display) -> SicroError {
    SicroError::Validation(format!("transicao: {detalhe}"))
}

/// Transição de um lado da peça, com slots > 0: (slots, nome no `xfade` quando sobrepõe).
fn lado(t: &Option<(Transicao, u32)>, n_q: u32) -> Result<Option<(u32, Option<&'static str>)>> {
    let Some((t, d)) = t.as_ref().filter(|(_, d)| *d > 0) else { return Ok(None) };
    conferir(t)?;
    Ok(Some(((*d).min(n_q), nome_xfade(&t.tipo))))
}

/// Cauda de uma peça à espera da cabeça da seguinte (ou do fundo, no `fecho`).
struct Cauda {
    rotulo: String,
    xfade: &'static str,
    d: u32,
    /// Quadros [k0, k1) da transição que a janela pega.
    k: (u32, u32),
}

enum Lado {
    Rotulo(String),
    Fundo,
}

#[derive(Clone, Copy, PartialEq)]
enum Pedaco {
    Cabeca,
    Corpo,
    Cauda,
}

/// Junta as peças na ordem e devolve o rótulo do vídeo do bloco (pts = slot do bloco, base 1/fps). `canvas` dá
/// tamanho e cor do clipe virtual do primeiro clipe e do `fecho`.
// CONTRATO: `canvas` no lugar de `fps` (abertura da Fase 1).
pub fn montar(g: &mut Grafo, pecas: &[Peca], canvas: &Canvas) -> Result<String> {
    let fps = canvas.fps;
    if pecas.is_empty() {
        return Err(SicroError::Validation("vazio".into()));
    }
    let mut partes = Vec::with_capacity(pecas.len());
    let mut cauda: Option<Cauda> = None;
    for p in pecas {
        let entrada = lado(&p.entrada, p.n_q)?;
        let saida = lado(&p.saida, p.n_q)?;
        let mut f = Vec::new();
        // `escurecer`: por tempo (o pts é o slot do clipe), para a janela no meio do clipe dar o mesmo quadro
        if let Some((d, None)) = entrada {
            f.push(filtro("fade", None, &[("t", "in".into()), ("st", "0".into()), ("d", segundos(d, fps))]));
        }
        if let Some((d, None)) = saida {
            f.push(filtro("fade", None, &[("t", "out".into()), ("st", segundos(p.n_q - d, fps)), ("d", segundos(d, fps))]));
        }
        let cabeca = entrada.and_then(|(d, x)| Some((x?, d)));
        let fim = saida.and_then(|(d, x)| Some((x?, d)));
        if cabeca.is_none() && fim.is_none() {
            if cauda.is_some() {
                return Err(erro(format!("sem a cabeça da sobreposição no clipe {}", p.clipe)));
            }
            f.push("setpts=PTS-STARTPTS".to_string());
            let r = g.rotulo("p");
            g.cadeia(&[&p.rotulo], &f, &[&r]);
            partes.push(r);
            continue;
        }
        pedacos(g, p, f, cabeca, fim, &mut cauda, &mut partes, canvas)?;
    }
    // `fecho`: a cauda do último clipe com o fundo
    if let Some(c) = cauda.take() {
        partes.push(misturar(g, Lado::Rotulo(c.rotulo), Lado::Fundo, c.xfade, c.d, c.k, canvas)?);
    }
    let saida = g.rotulo("j");
    let entradas: Vec<&str> = partes.iter().map(String::as_str).collect();
    // o `concat` empurra a peça seguinte pela duração média da anterior (zero com 1 quadro: o `fps` descartava um).
    // Cada peça já traz a contagem exata: o slot é a ordem. O `fps` fica, também com uma peça só, pela duração do
    // quadro: sem ela o 7.0.2 fecha o MP4 do bloco sem o último passo e a junção repete o pts.
    let mut f = if partes.len() > 1 { vec![format!("concat=n={}:v=1:a=0", partes.len())] } else { Vec::new() };
    f.extend([format!("settb={}/{}", fps.den, fps.num), "setpts=N".to_string(), format!("fps={}/{}", fps.num, fps.den)]);
    g.cadeia(&entradas, &f, &[&saida]);
    Ok(saida)
}

/// Peça com sobreposição: corta a janela dela em cabeça, corpo e cauda. A cabeça mistura com a cauda pendente da
/// anterior (ou com o fundo, no primeiro clipe); a cauda fica pendente para a seguinte.
#[allow(clippy::too_many_arguments)]
fn pedacos(
    g: &mut Grafo,
    p: &Peca,
    escurecer: Vec<String>,
    cabeca: Option<(&'static str, u32)>,
    fim: Option<(&'static str, u32)>,
    cauda: &mut Option<Cauda>,
    partes: &mut Vec<String>,
    canvas: &Canvas,
) -> Result<()> {
    let fps = canvas.fps;
    let (n, de, ate) = (p.n_q, p.de, p.ate);
    let (h, t) = (cabeca.map_or(0, |(_, d)| d), fim.map_or(0, |(_, d)| d));
    if h + t > n || de >= ate || ate > n {
        return Err(erro(format!("sobreposições maiores que o clipe {}", p.clipe)));
    }
    let faixas: Vec<(Pedaco, u32, u32)> = [(Pedaco::Cabeca, 0, h), (Pedaco::Corpo, h, n - t), (Pedaco::Cauda, n - t, n)]
        .into_iter()
        .map(|(x, a, b)| (x, a.max(de), b.min(ate)))
        .filter(|(_, a, b)| a < b)
        .collect();
    if cauda.is_some() && faixas.first().map(|x| x.0) != Some(Pedaco::Cabeca) {
        return Err(erro(format!("sem a cabeça da sobreposição no clipe {}", p.clipe)));
    }
    let base = if escurecer.is_empty() {
        p.rotulo.clone()
    } else {
        let r = g.rotulo("p");
        g.cadeia(&[&p.rotulo], &escurecer, &[&r]);
        r
    };
    let origens: Vec<String> = if faixas.len() > 1 {
        let rs: Vec<String> = faixas.iter().map(|_| g.rotulo("s")).collect();
        let saidas: Vec<&str> = rs.iter().map(String::as_str).collect();
        g.cadeia(&[&base], &[format!("split={}", faixas.len())], &saidas);
        rs
    } else {
        vec![base]
    };
    for (&(qual, a, b), origem) in faixas.iter().zip(origens) {
        let r = g.rotulo("p");
        let mut f = vec![format!("trim=start_frame={}:end_frame={}", a - de, b - de), "setpts=PTS-STARTPTS".to_string()];
        // o `xfade` exige a taxa no elo (o `setpts` do 7.0.2 a apaga)
        if qual != Pedaco::Corpo {
            f.push(format!("fps={}/{}", fps.num, fps.den));
        }
        g.cadeia(&[&origem], &f, &[&r]);
        match (qual, cabeca, fim) {
            (Pedaco::Corpo, _, _) => partes.push(r),
            (Pedaco::Cabeca, Some((x, d)), _) => {
                let outro = if p.sobrepoe == 0 {
                    if cauda.is_some() {
                        return Err(erro(format!("sobreposição sem slots no clipe {}", p.clipe)));
                    }
                    Lado::Fundo
                } else {
                    let c = cauda.take().ok_or_else(|| erro(format!("sem a cauda do clipe anterior a {}", p.clipe)))?;
                    if p.sobrepoe != d || (c.xfade, c.d, c.k) != (x, d, (a, b)) {
                        return Err(erro(format!("sobreposição diferente dos dois lados no clipe {}", p.clipe)));
                    }
                    Lado::Rotulo(c.rotulo)
                };
                partes.push(misturar(g, outro, Lado::Rotulo(r), x, d, (a, b), canvas)?);
            }
            (Pedaco::Cauda, _, Some((x, d))) => {
                *cauda = Some(Cauda { rotulo: r, xfade: x, d, k: (a - (n - t), b - (n - t)) });
            }
            _ => return Err(erro(format!("pedaço sem transição no clipe {}", p.clipe))),
        }
    }
    Ok(())
}

/// Os quadros [k0, k1) de uma transição de `d` quadros: `a` sai, `b` entra. Cada lado chega com os quadros [k0, k1)
/// (pts 0…) e é completado com cópias até `d`: o `xfade` acha o progresso pelo pts desde o primeiro quadro, e o
/// quadro k da janela tem de ser o k da transição inteira. Um `xfade` por plano, com `offset=0`.
fn misturar(g: &mut Grafo, a: Lado, b: Lado, xfade: &str, d: u32, k: (u32, u32), canvas: &Canvas) -> Result<String> {
    let fps = canvas.fps;
    let duracao = d as f64 * fps.den as f64 / fps.num as f64;
    if !(duracao <= XFADE_MAX_S) {
        return Err(erro(format!("{duracao} s acima de {XFADE_MAX_S} s")));
    }
    let (k0, k1) = k;
    if !(k0 < k1 && k1 <= d) {
        return Err(erro(format!("quadros {k0}..{k1} fora dos {d} da transição")));
    }
    let inteira = k0 == 0 && k1 == d;
    let fundo = cor::filtro_cor_cheia(cor::ler(&canvas.cor_fundo)?);
    let planos = |g: &mut Grafo, lado: Lado| -> [String; 3] {
        let saidas = [g.rotulo("y"), g.rotulo("u"), g.rotulo("v")];
        let refs: Vec<&str> = saidas.iter().map(String::as_str).collect();
        match lado {
            Lado::Rotulo(r) => {
                let mut f = Vec::new();
                if !inteira {
                    f.push(format!("tpad=start={k0}:start_mode=clone:stop={}:stop_mode=clone", d - k1));
                }
                f.push("extractplanes=y+u+v".to_string());
                g.cadeia(&[&r], &f, &refs);
            }
            Lado::Fundo => {
                let f = vec![
                    format!("color=c=black:s={}x{}:r={}/{}", canvas.w, canvas.h, fps.num, fps.den),
                    format!("trim=end_frame={d}"),
                    "format=yuv420p".to_string(),
                    fundo.clone(),
                    ETIQUETA_709_TV.to_string(),
                    "extractplanes=y+u+v".to_string(),
                ];
                g.cadeia(&[], &f, &refs);
            }
        }
        saidas
    };
    let pa = planos(g, a);
    let pb = planos(g, b);
    let mut x = Vec::with_capacity(3);
    for (ea, eb) in pa.iter().zip(&pb) {
        let r = g.rotulo("x");
        let f = filtro("xfade", None, &[("transition", xfade.to_string()), ("duration", segundos(d, fps)), ("offset", "0".to_string())]);
        g.cadeia(&[ea, eb], &[f], &[&r]);
        x.push(r);
    }
    let r = g.rotulo("m");
    let mut f = vec!["mergeplanes=0x001020:yuv420p".to_string(), "setsar=1".to_string(), ETIQUETA_709_TV.to_string()];
    if !inteira {
        f.extend([format!("trim=start_frame={k0}:end_frame={k1}"), "setpts=PTS-STARTPTS".to_string()]);
    }
    g.cadeia(&[&x[0], &x[1], &x[2]], &f, &[&r]);
    Ok(r)
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::AtomicBool;

    use serde_json::{json, Value};

    use crate::edicao::ffmpeg::exec::{self, Comando};
    use crate::edicao::ffmpeg::sonda::Capacidades;
    use crate::edicao::modelo::{AudioTransicao, LinhaResolvida, Nivel, ProjetoEdicao};
    use crate::edicao::motor::plano::Plano;
    use crate::edicao::motor::{blocos, compilar_video, Alvo, Contexto};
    use crate::edicao::quadros::apoio::{self, Medida, SemRastreios};
    use crate::edicao::testes::aceite::{alvo_final, midia, projeto, trecho, Mundo};
    use crate::edicao::testes::executor::{self, Opcoes};
    use crate::edicao::testes::fontes::{self, Fonte};
    use crate::edicao::testes::medir::{self, QuadroRgb};
    use crate::edicao::{mensagens, tempo, validar};

    const TAXAS: [(i64, i64); 3] = [(30, 1), (30000, 1001), (25, 1)];
    /// Largura × altura das fontes da matriz (o canvas dos testes).
    const W: u32 = 320;
    const H: u32 = 180;

    fn t(tipo: &str) -> Transicao {
        Transicao { tipo: tipo.into(), duracao_s: 0.5, audio: AudioTransicao::Acompanhar }
    }

    fn peca(rotulo: &str, n_q: u32, entrada: Option<(Transicao, u32)>, saida: Option<(Transicao, u32)>) -> Peca {
        Peca { rotulo: rotulo.into(), clipe: rotulo.into(), n_q, de: 0, ate: n_q, sobrepoe: 0, entrada, saida }
    }

    fn canvas(fps: (i64, i64)) -> Canvas {
        Canvas { w: 640, h: 360, fps: Racional { num: fps.0, den: fps.1 }, cor_fundo: "#000000".into() }
    }

    fn tr(tipo: &str, s: f64) -> Value {
        json!({ "tipo": tipo, "duracao_s": s, "audio": "acompanhar" })
    }

    // ------------------------------------------------------------------ sem ffmpeg

    #[test]
    fn corte_seco_concatena_e_escurecer_vira_fade_por_tempo() {
        let canvas = canvas((30, 1));
        let mut g = Grafo::novo();
        let pecas = vec![peca("c000v", 90, None, Some((t("escurecer"), 15))), peca("c001v", 60, Some((t("escurecer"), 15)), None)];
        let j = montar(&mut g, &pecas, &canvas).unwrap();
        assert_eq!(
            g.texto(),
            format!(
                "[c000v]fade=t=out:st=2.500000000:d=0.500000000,setpts=PTS-STARTPTS[p000];\n\
                 [c001v]fade=t=in:st=0:d=0.500000000,setpts=PTS-STARTPTS[p001];\n[p000][p001]concat=n=2:v=1:a=0,settb=1/30,setpts=N,fps=30/1[{j}]"
            )
        );
        // uma peça só: sem concat
        let mut g = Grafo::novo();
        let j = montar(&mut g, &pecas[..1], &canvas).unwrap();
        assert!(g.texto().ends_with(&format!("[p000]settb=1/30,setpts=N,fps=30/1[{j}]")), "{}", g.texto());
        assert!(!g.texto().contains("concat"));
        // fora do catálogo (e nome cru do `xfade`): recusada com o nome
        for tipo in ["transicao_do_futuro", "fade", "slideleft"] {
            let fora = vec![peca("c", 30, Some((t(tipo), 10)), None)];
            assert!(montar(&mut Grafo::novo(), &fora, &canvas).unwrap_err().to_string().contains(&format!("nao_suportado: transicao {tipo}")));
        }
        assert!(montar(&mut Grafo::novo(), &[], &canvas).is_err());
    }

    #[test]
    fn juncao_por_pedacos_no_grafo() {
        let canvas = canvas((30, 1));
        let xfade = |g: &Grafo| g.texto().matches("xfade=transition=").count();
        // A (cauda de 10) → B (cabeça de 10): corpo de A, mistura, corpo de B
        let mut b = peca("c001v", 40, Some((t("fundir"), 10)), None);
        b.sobrepoe = 10;
        let pecas = vec![peca("c000v", 30, None, Some((t("fundir"), 10))), b.clone()];
        let mut g = Grafo::novo();
        montar(&mut g, &pecas, &canvas).unwrap();
        let s = g.texto();
        assert_eq!(xfade(&g), 3, "um por plano: {s}");
        assert!(s.contains("xfade=transition=fade:duration=0.333333333:offset=0"), "{s}");
        assert!(s.contains("[c000v]split=2") && s.contains("[c001v]split=2") && s.contains("concat=n=3:v=1:a=0"), "{s}");
        assert!(s.contains("trim=start_frame=0:end_frame=20,setpts=PTS-STARTPTS[") && s.contains("trim=start_frame=20:end_frame=30,setpts=PTS-STARTPTS,fps=30/1["), "{s}");
        assert_eq!(s.matches("mergeplanes=0x001020:yuv420p,setsar=1,setparams=").count(), 1);
        // nenhuma conversão de formato antes do `xfade` (plano a plano), nenhum `scale`
        assert!(!s.contains("scale") && !s.contains("format=yuv444p"), "{s}");
        assert!(!s.contains("color="), "sem fundo entre dois clipes: {s}");
        // primeiro clipe e `fecho`: com o fundo (cor exata por `lutyuv`), sem mudar a contagem
        let mut so = peca("c000v", 30, Some((t("circulo"), 6)), Some((t("cortina_dir"), 8)));
        so.sobrepoe = 0;
        let mut g = Grafo::novo();
        let mut fundo = canvas.clone();
        fundo.cor_fundo = "#ff0000".into();
        montar(&mut g, &[so.clone()], &fundo).unwrap();
        let s = g.texto();
        assert_eq!(s.matches("color=c=black:s=640x360:r=30/1,trim=end_frame=").count(), 2, "{s}");
        assert!(s.contains("lutyuv=y=63:u=102:v=240") && s.contains("transition=circleopen") && s.contains("transition=wiperight"), "{s}");
        assert!(s.contains("concat=n=3:v=1:a=0"), "abertura, corpo, fecho: {s}");
        // escurecer na entrada e sobreposição na saída do mesmo clipe: o `fade` (por tempo) antes de cortar em pedaços
        let mut g = Grafo::novo();
        montar(&mut g, &[peca("c000v", 30, Some((t("escurecer"), 6)), Some((t("fundir"), 10))), b.clone()], &canvas).unwrap();
        let s = g.texto();
        assert!(s.starts_with("[c000v]fade=t=in:st=0:d=0.200000000[p000];\n[p000]split=2["), "{s}");
        // janela no meio da transição: completa com cópias até `d` e corta depois (o quadro k é o k da inteira)
        let mut a = peca("c000v", 30, None, Some((t("fundir"), 10)));
        (a.de, a.ate) = (25, 28);
        let mut b2 = b.clone();
        (b2.de, b2.ate) = (5, 8);
        let mut g = Grafo::novo();
        montar(&mut g, &[a.clone(), b2.clone()], &canvas).unwrap();
        let s = g.texto();
        assert_eq!(s.matches("tpad=start=5:start_mode=clone:stop=2:stop_mode=clone,extractplanes=y+u+v").count(), 2, "{s}");
        assert!(s.contains("trim=start_frame=5:end_frame=8,setpts=PTS-STARTPTS[") && !s.contains("split"), "{s}");
        // incoerências: nunca um grafo calado errado
        let mut b3 = b2.clone();
        b3.sobrepoe = 9;
        assert!(montar(&mut Grafo::novo(), &[a.clone(), b3], &canvas).unwrap_err().to_string().contains("transicao:"));
        let mut b4 = b2.clone();
        (b4.de, b4.ate) = (4, 8);
        assert!(montar(&mut Grafo::novo(), &[a.clone(), b4], &canvas).unwrap_err().to_string().contains("transicao:"));
        assert!(montar(&mut Grafo::novo(), &[a.clone(), peca("c001v", 40, None, None)], &canvas).unwrap_err().to_string().contains("transicao:"));
        // além do limite do `xfade` (60 s)
        let longa = vec![peca("c000v", 4000, None, Some((t("fundir"), 1801))), {
            let mut b = peca("c001v", 4000, Some((t("fundir"), 1801)), None);
            b.sobrepoe = 1801;
            b
        }];
        assert!(montar(&mut Grafo::novo(), &longa, &canvas).unwrap_err().to_string().contains("transicao:"));
    }

    /// Contexto sem disco: duas mídias sintéticas de 300 quadros a 30 qps.
    fn mundo_sem_ffmpeg() -> (std::collections::HashMap<String, crate::edicao::motor::FonteInfo>, Capacidades) {
        let info = apoio::fonte(apoio::tabela((1, 30), (0..300).collect()), (W, H));
        let mut fontes = std::collections::HashMap::new();
        fontes.insert("m1".to_string(), info);
        (fontes, apoio::cap_falsa())
    }

    fn linha_de(p: &ProjetoEdicao, ctx: &Contexto) -> LinhaResolvida {
        tempo::resolver(p, ctx)
    }

    /// O catálogo é um só: o que a validação aceita o motor faz, e o que ela recusa o motor recusa com o mesmo nome.
    /// O espelho do front lê `juncao-transicoes.json`.
    #[test]
    fn catalogo_validacao_igual_ao_motor() {
        assert_eq!(TRANSICOES[0], "escurecer");
        assert_eq!(TRANSICOES[1..].to_vec(), XFADE.iter().map(|(t, _)| *t).collect::<Vec<_>>());
        for t in TRANSICOES {
            assert_eq!(tempo::transicao_sobrepoe(t), nome_xfade(t).is_some(), "{t}");
        }
        let (fontes, cap) = mundo_sem_ffmpeg();
        let ctx = Contexto { fontes: &fontes, cap: &cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
        let tab = &fontes["m1"].quadros;
        let alvo = alvo_final("cpu");
        let projeto_com = |tipo: &str| {
            let mut c2 = trecho("c2", "m1", tab, 100, 160, 1.0, json!([]));
            c2["transicao_entrada"] = tr(tipo, 0.3);
            let mut c1 = trecho("c1", "m1", tab, 0, 60, 1.0, json!([]));
            c1["transicao_entrada"] = tr(tipo, 0.2);
            projeto(W, H, (30, 1), vec![midia("m1", "video", json!([]))], vec![c1, c2], json!({ "fecho": tr(tipo, 0.2) }))
        };
        for tipo in TRANSICOES {
            let p = projeto_com(tipo);
            let linha = linha_de(&p, &ctx);
            let problemas = validar::validar(&p, &linha, &ctx, Some(&alvo));
            assert!(problemas.iter().all(|x| x.nivel != Nivel::Erro), "{tipo}: {problemas:#?}");
            let plano = compilar_video(&p, &linha, &ctx, &alvo).unwrap_or_else(|e| panic!("{tipo}: {e}"));
            let g = &plano.blocos[0].grafo;
            match nome_xfade(tipo) {
                // abertura, entre os dois e fecho, um `xfade` por plano
                Some(x) => assert_eq!(g.matches(&format!("xfade=transition={x}:")).count(), 9, "{tipo}"),
                None => assert_eq!(g.matches("fade=t=").count(), 4, "{tipo}"),
            }
        }
        // fora do catálogo: a validação acusa no clipe e o motor recusa com o nome
        for tipo in ["transicao_do_futuro", "fade", "slideleft", "dissolver"] {
            let p = projeto_com(tipo);
            let linha = linha_de(&p, &ctx);
            let problemas = validar::validar(&p, &linha, &ctx, Some(&alvo));
            assert!(problemas.iter().any(|x| x.codigo == mensagens::codigo::NAO_SUPORTADO && x.clipe.as_deref() == Some("c1")), "{tipo}: {problemas:#?}");
            let e = compilar_video(&p, &linha, &ctx, &alvo).unwrap_err().to_string();
            assert!(e.contains(&format!("nao_suportado: transicao {tipo}")), "{tipo}: {e}");
        }
        let casos: Vec<Value> = TRANSICOES.iter().map(|t| json!({ "nome": t, "entrada": t, "saida": { "sobrepoe": tempo::transicao_sobrepoe(t), "xfade": nome_xfade(t) } })).collect();
        crate::edicao::curva::fixtures::conferir("juncao-transicoes.json", json!(casos));
    }

    #[test]
    fn primeiro_clipe_e_fecho_nao_mudam_total_q() {
        let (fontes, cap) = mundo_sem_ffmpeg();
        let ctx = Contexto { fontes: &fontes, cap: &cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
        let tab = &fontes["m1"].quadros;
        for (fps, alvo) in TAXAS.iter().flat_map(|&f| [(f, alvo_final("cpu")), (f, Alvo::Previa { altura: H, de_q: 0, ate_q: 10_000 })]) {
            let sem = projeto(W, H, fps, vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, 0, 90, 1.0, json!([])), trecho("c2", "m1", tab, 120, 200, 1.0, json!([]))], json!({}));
            let total = linha_de(&sem, &ctx).total_q;
            for tipo in TRANSICOES {
                let mut p = sem.clone();
                let mut v = serde_json::to_value(&p).unwrap();
                v["principal"][0]["transicao_entrada"] = tr(tipo, 0.5);
                v["fecho"] = tr(tipo, 0.7);
                p = serde_json::from_value(v).unwrap();
                let linha = linha_de(&p, &ctx);
                assert_eq!((linha.total_q, linha.clipes[0].sobrepoe_q, linha.clipes[0].inicio_q), (total, 0, 0), "{tipo} a {fps:?}");
                let plano = compilar_video(&p, &linha, &ctx, &alvo).unwrap();
                assert_eq!(plano.total_q, total, "{tipo} a {fps:?}");
                // o som lê os mesmos slots (`blocos::escurecimentos`): os da transição, dentro do clipe
                let (entra, _) = blocos::escurecimentos(&p, &linha, 0);
                let (_, sai) = blocos::escurecimentos(&p, &linha, 1);
                assert_eq!((entra as u64, sai as u64), (tempo::slots(0.5, linha.fps), tempo::slots(0.7, linha.fps)));
            }
        }
    }

    // ------------------------------------------------------------------ com ffmpeg

    fn comando(cap: &Capacidades, pasta: &Path, nome: &str, args: Vec<String>) -> Comando {
        Comando { nome: nome.into(), programa: cap.binario.clone(), args, pasta: pasta.to_path_buf(), ambiente: Vec::new(), sem_ambiente: Vec::new(), stderr: pasta.join(format!("{nome}.stderr")) }
    }

    /// Quadros yuv420p crus de um vídeo (o `.nut` sem perdas do executor sai sem conversão).
    fn yuv(cap: &Capacidades, video: &Path, w: u32, h: u32) -> Vec<Vec<u8>> {
        let a: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-i", &video.to_string_lossy(), "-f", "rawvideo", "-pix_fmt", "yuv420p", "-"].map(String::from).into();
        let cmd = comando(cap, video.parent().unwrap(), "p11-yuv", a);
        let (fim, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).expect("ffmpeg");
        exec::exigir_sucesso(&cmd, &fim).expect("decodificar");
        let t = (w * h * 3 / 2) as usize;
        assert_eq!(out.len() % t, 0, "{}", video.display());
        out.chunks_exact(t).map(<[u8]>::to_vec).collect()
    }

    /// RGB pela conversão do quadro final (BT.709 limitada → cheia), para comparar com o BMP dele.
    fn rgb709(cap: &Capacidades, video: &Path, w: u32, h: u32) -> Vec<QuadroRgb> {
        let a: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-i", &video.to_string_lossy(), "-vf",
            "scale=flags=bicubic+accurate_rnd+bitexact+full_chroma_int:in_color_matrix=bt709:in_range=tv:out_range=pc,format=rgb24", "-f", "rawvideo", "-"].map(String::from).into();
        let cmd = comando(cap, video.parent().unwrap(), "p11-rgb", a);
        let (fim, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).expect("ffmpeg");
        exec::exigir_sucesso(&cmd, &fim).expect("decodificar");
        out.chunks_exact((w * h * 3) as usize).map(|px| QuadroRgb { w, h, px: px.to_vec() }).collect()
    }

    /// Roda o plano sem perdas e devolve os quadros de todos os blocos, na ordem.
    fn quadros(cap: &Capacidades, plano: &Plano, nome: &str) -> Vec<Vec<u8>> {
        let pasta = fontes::pasta_temporaria(nome);
        let r = executor::rodar(cap, plano, &pasta, &Opcoes { sem_perdas: true, ..Opcoes::default() }).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
        let mut q = Vec::new();
        for b in &r.blocos {
            q.extend(yuv(cap, b, plano.canvas.w, plano.canvas.h));
        }
        assert_eq!(q.len() as u64, plano.total_q, "{nome} ({}): quadros × slots", cap.versao);
        q
    }

    /// Quadro inteiro na cor cheia (BT.709 limitada), como o clipe virtual do fundo.
    fn chapado(w: u32, h: u32, cor: &str) -> Vec<u8> {
        let (y, u, v) = cor::yuv_bt709(cor::ler(cor).unwrap());
        [vec![y; (w * h) as usize], vec![u; (w * h / 4) as usize], vec![v; (w * h / 4) as usize]].concat()
    }

    /// Oráculo: cada par (`xfade`, quadros que saem, quadros que entram) feito à parte, de arquivos crus, plano a
    /// plano, sem janela, sem pedaços e sem `concat`.
    fn oraculo(cap: &Capacidades, nome: &str, fps: (i64, i64), pares: &[(&str, Vec<Vec<u8>>, Vec<Vec<u8>>)]) -> Vec<Vec<Vec<u8>>> {
        let pasta = fontes::pasta_temporaria(nome);
        let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-loglevel", "error"].map(String::from).into();
        let mut g = Vec::new();
        for (i, (x, sai, entra)) in pares.iter().enumerate() {
            assert_eq!(sai.len(), entra.len());
            for (lado, qs) in [("a", sai), ("b", entra)] {
                let arquivo = format!("o{i}{lado}.yuv");
                std::fs::write(pasta.join(&arquivo), qs.concat()).unwrap();
                a.extend(["-f", "rawvideo", "-pix_fmt", "yuv420p", "-s", &format!("{W}x{H}"), "-framerate", &format!("{}/{}", fps.0, fps.1), "-i", &arquivo].map(String::from));
            }
            let d = format!("{:.6}", sai.len() as f64 * fps.1 as f64 / fps.0 as f64);
            g.push(format!("[{}]extractplanes=y+u+v[a{i}y][a{i}u][a{i}v];[{}]extractplanes=y+u+v[b{i}y][b{i}u][b{i}v]", 2 * i, 2 * i + 1));
            for pl in ["y", "u", "v"] {
                g.push(format!("[a{i}{pl}][b{i}{pl}]xfade=transition={x}:duration={d}:offset=0[x{i}{pl}]"));
            }
            g.push(format!("[x{i}y][x{i}u][x{i}v]mergeplanes=0x001020:yuv420p[o{i}]"));
        }
        a.extend(["-filter_complex".to_string(), g.join(";")]);
        for i in 0..pares.len() {
            a.extend(["-map".to_string(), format!("[o{i}]"), "-f".into(), "rawvideo".into(), "-pix_fmt".into(), "yuv420p".into(), format!("o{i}.yuv")]);
        }
        let cmd = comando(cap, &pasta, "oraculo", a);
        let fim = exec::rodar(&cmd, &AtomicBool::new(false)).expect("ffmpeg");
        exec::exigir_sucesso(&cmd, &fim).expect("oráculo");
        let t = (W * H * 3 / 2) as usize;
        (0..pares.len()).map(|i| std::fs::read(pasta.join(format!("o{i}.yuv"))).unwrap().chunks_exact(t).map(<[u8]>::to_vec).collect()).collect()
    }

    /// `fade` do `xfade` à mão (f32, truncado, como no ffmpeg): quadro k de d.
    fn fade_a_mao(a: &[u8], b: &[u8], k: u32, d: u32) -> Vec<u8> {
        let p = 1.0f32 - k as f32 / d as f32;
        a.iter().zip(b).map(|(&x, &y)| (x as f32 * p + y as f32 * (1.0 - p)) as u8).collect()
    }

    /// Três trechos da fonte da matriz (números desenhados e quadrado vermelho que anda), a 1×.
    const FAIXAS: [(usize, usize); 3] = [(30, 75), (150, 195), (300, 330)];

    fn trechos(m: &Medida) -> Vec<Value> {
        FAIXAS.iter().enumerate().map(|(i, &(a, b))| trecho(&format!("c{}", i + 1), "m1", &m.info.quadros, a, b, 1.0, json!([]))).collect()
    }

    /// Cada tipo do catálogo, nas três taxas: quantos quadros, e em cada slot o quadro exato. Corpo = o clipe sozinho
    /// (sem transição), conferido pelos números desenhados; sobreposição = o oráculo sobre os mesmos quadros (e o
    /// `fundir` também pela conta, sem ffmpeg). A entrada do primeiro e o `fecho` com o fundo. As regiões saem de
    /// `sobrepoe_q` e de `blocos::escurecimentos`, que são o que o som usa.
    #[test]
    fn cada_transicao_quadros_exatos_em_30_2997_25() {
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        assert_eq!(m.info.exibido, (W, H));
        let mundo = Mundo::novo(&cap).com("m1", &m.info);
        let ctx = Contexto { fontes: &mundo.fontes, cap: &cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
        let midias = vec![midia("m1", "video", json!([]))];
        let mut conferidos = (0usize, 0usize);
        for fps in TAXAS {
            // cada clipe sozinho: os quadros que ele entrega, e o número desenhado de cada um = o previsto
            let sozinho: Vec<Vec<Vec<u8>>> = trechos(&m)
                .into_iter()
                .enumerate()
                .map(|(i, c)| {
                    let p = projeto(W, H, fps, midias.clone(), vec![c], json!({}));
                    let plano = mundo.compilar(&p, &alvo_final("cpu")).unwrap();
                    let pasta = fontes::pasta_temporaria("p11-sozinho");
                    let r = executor::rodar(&cap, &plano, &pasta, &Opcoes { sem_perdas: true, ..Opcoes::default() }).unwrap();
                    let lidos = medir::quadros_de_origem(&cap, &r.blocos[0]).unwrap();
                    let previstos: Vec<u32> = plano.proveniencia.roteiro[0].origem.as_ref().unwrap().quadro_do_slot.iter().map(|&j| m.numeros[j as usize]).collect();
                    assert_eq!(lidos, previstos, "clipe {i} sozinho a {fps:?} ({})", cap.versao);
                    yuv(&cap, &r.blocos[0], W, H)
                })
                .collect();
            // fundo colorido: a croma do clipe virtual também é conferida
            let cor_fundo = "#3060a0";
            let fundo = chapado(W, H, cor_fundo);
            for &(tipo, xfade) in XFADE.iter().chain([("escurecer", "")].iter()) {
                let mut cs = trechos(&m);
                for (c, s) in cs.iter_mut().zip([0.4, 0.5, 0.3]) {
                    c["transicao_entrada"] = tr(tipo, s);
                }
                let mut p = projeto(W, H, fps, midias.clone(), cs, json!({ "fecho": tr(tipo, 0.2) }));
                p.saida.cor_fundo = cor_fundo.into();
                let linha = tempo::resolver(&p, &ctx);
                assert!(validar::validar(&p, &linha, &ctx, Some(&alvo_final("cpu"))).iter().all(|x| x.nivel != Nivel::Erro), "{tipo}");
                let plano = mundo.compilar(&p, &alvo_final("cpu")).unwrap();
                let q = quadros(&cap, &plano, "p11-exatos");
                let n: Vec<u32> = sozinho.iter().map(|s| s.len() as u32).collect();
                let s: Vec<u32> = linha.clipes.iter().map(|c| c.sobrepoe_q).collect();
                let esperado = n.iter().sum::<u32>() - s.iter().sum::<u32>();
                assert_eq!((q.len() as u64, plano.total_q, linha.total_q), (esperado as u64, esperado as u64, esperado as u64), "{tipo} a {fps:?}");
                let sobrepoe = !xfade.is_empty();
                assert_eq!(s, if sobrepoe { vec![0, tempo::slots(0.5, linha.fps) as u32, tempo::slots(0.3, linha.fps) as u32] } else { vec![0, 0, 0] }, "{tipo} a {fps:?}");
                let (abre, _) = blocos::escurecimentos(&p, &linha, 0);
                let (_, fecha) = blocos::escurecimentos(&p, &linha, 2);
                // quadro previsto em cada slot: Some = o do clipe sozinho; None = região de transição
                let mut previsto: Vec<Option<&[u8]>> = vec![None; q.len()];
                let mut regioes: Vec<(usize, usize)> = Vec::new();
                let mut misturas: Vec<(usize, Vec<Vec<u8>>, Vec<Vec<u8>>)> = Vec::new();
                for i in 0..3 {
                    let ini = linha.clipes[i].inicio_q as usize;
                    let (cab, cau) = if sobrepoe { (if i == 0 { abre } else { s[i] }, if i == 2 { fecha } else { s[i + 1] }) } else { blocos::escurecimentos(&p, &linha, i) };
                    let (cab, cau, ni) = (cab as usize, cau as usize, n[i] as usize);
                    for k in cab..ni - cau {
                        assert!(previsto[ini + k].is_none(), "{tipo}: slot {} de dois clipes", ini + k);
                        previsto[ini + k] = Some(&sozinho[i][k]);
                    }
                    regioes.extend([(ini, ini + cab), (ini + ni - cau, ini + ni)]);
                    if !sobrepoe {
                        continue;
                    }
                    if i == 0 {
                        misturas.push((ini, vec![fundo.clone(); cab], sozinho[0][..cab].to_vec()));
                    }
                    let entra = if i == 2 { vec![fundo.clone(); cau] } else { sozinho[i + 1][..cau].to_vec() };
                    misturas.push((ini + ni - cau, sozinho[i][ni - cau..].to_vec(), entra));
                }
                for (slot, (x, e)) in q.iter().zip(&previsto).enumerate() {
                    match e {
                        Some(e) => {
                            assert!(x == e, "{tipo} a {fps:?} ({}): slot {slot} não é o quadro do clipe", cap.versao);
                            conferidos.0 += 1;
                        }
                        None => assert!(regioes.iter().any(|&(a, b)| (a..b).contains(&slot)), "{tipo} a {fps:?}: slot {slot} sem dono"),
                    }
                }
                if !sobrepoe {
                    continue;
                }
                let pares: Vec<(&str, Vec<Vec<u8>>, Vec<Vec<u8>>)> = misturas.iter().map(|(_, a, b)| (xfade, a.clone(), b.clone())).collect();
                let feitos = oraculo(&cap, "p11-oraculo", fps, &pares);
                let mut cobertos = 0;
                for ((ini, a, b), o) in misturas.iter().zip(&feitos) {
                    let d = a.len() as u32;
                    assert_eq!(o.len(), a.len());
                    for k in 0..d as usize {
                        let slot = ini + k;
                        assert!(previsto[slot].is_none(), "{tipo}: slot {slot} do clipe e da transição");
                        assert!(q[slot] == o[k], "{tipo} a {fps:?} ({}): slot {slot} (quadro {k} de {d}) ≠ oráculo", cap.versao);
                        if xfade == "fade" {
                            let conta = fade_a_mao(&a[k], &b[k], k as u32, d);
                            let pior = q[slot].iter().zip(&conta).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
                            assert!(pior <= 1, "{tipo} a {fps:?}: slot {slot} difere da conta em {pior}");
                        }
                        cobertos += 1;
                    }
                }
                // toda região de transição conferida pelo oráculo, nenhuma a mais
                assert_eq!(cobertos, previsto.iter().filter(|x| x.is_none()).count(), "{tipo} a {fps:?}");
                conferidos.1 += cobertos;
            }
        }
        eprintln!("transições ({}): {} slots iguais ao clipe sozinho, {} iguais ao oráculo", cap.versao, conferidos.0, conferidos.1);
    }

    fn titulo(id: &str, cor: &str, duracao_s: f64) -> Value {
        json!({ "tipo": "titulo", "id": id, "rotulo": "", "desligado": false, "transicao_entrada": null, "efeitos": [], "duracao_s": duracao_s,
                "titulo": { "modelo": "cartela", "fundo": { "tipo": "cor", "cor": cor }, "blocos": [] } })
    }

    /// Média de α no luma (0 = quem sai, 1 = quem entra) e o pior desvio para fora do segmento entre os dois, nos
    /// três planos (nenhum valor que não seja mistura dos dois lados).
    fn mistura(q: &[u8], a: &[u8], b: &[u8]) -> (f64, u8) {
        let ny = (W * H) as usize;
        let alfa = q[..ny].iter().zip(&a[..ny]).zip(&b[..ny]).map(|((&x, &ya), &yb)| (x as f64 - ya as f64) / (yb as f64 - ya as f64)).sum::<f64>() / ny as f64;
        let fora = q.iter().zip(a).zip(b).map(|((&x, &pa), &pb)| x.saturating_sub(pa.max(pb)).max(pa.min(pb).saturating_sub(x))).max().unwrap();
        (alfa, fora)
    }

    /// Cores chapadas de luma bem diferente: em todo tipo e taxa, o quadro do meio é mistura dos dois lados
    /// (α médio entre 0,25 e 0,75, nenhum valor fora do segmento) e a transição anda de quem sai para quem entra.
    #[test]
    fn quadro_do_meio_mistura_os_dois_lados() {
        let Some(cap) = fontes::cap() else { return };
        let mundo = Mundo::novo(&cap);
        let (c1, c2, f) = ("#f0d040", "#203070", "#000000");
        for fps in TAXAS {
            for &(tipo, _) in XFADE {
                let mut cs = vec![titulo("t1", c1, 1.0), titulo("t2", c2, 1.0)];
                cs[0]["transicao_entrada"] = tr(tipo, 0.4);
                cs[1]["transicao_entrada"] = tr(tipo, 0.4);
                let mut p = projeto(W, H, fps, vec![], cs, json!({ "fecho": tr(tipo, 0.4) }));
                p.saida.cor_fundo = f.into();
                let plano = mundo.compilar(&p, &alvo_final("cpu")).unwrap();
                let q = quadros(&cap, &plano, "p11-meio");
                let d = tempo::slots(0.4, p.saida.fps) as usize;
                let n = q.len();
                let (a, b, z) = (q[d + 1].clone(), q[n - d - 2].clone(), chapado(W, H, f));
                let ini2 = (tempo::slots(1.0, p.saida.fps) as usize) - d;
                for (nome, ini, sai, entra) in [("abertura", 0, &z, &a), ("entre", ini2, &a, &b), ("fecho", n - d, &b, &z)] {
                    let alfas: Vec<f64> = (0..d).map(|k| mistura(&q[ini + k], sai, entra).0).collect();
                    let (meio, fora) = mistura(&q[ini + d / 2], sai, entra);
                    assert!((0.25..=0.75).contains(&meio) && fora <= 2, "{tipo} {nome} a {fps:?} ({}): α do meio {meio:.3}, {fora} fora", cap.versao);
                    assert!(alfas.windows(2).all(|w| w[1] >= w[0] - 1e-9) && alfas[0] < 0.1 && alfas[d - 1] > 0.85, "{tipo} {nome} a {fps:?}: {alfas:.3?}");
                }
            }
        }
    }

    /// Prévia e quadro final no meio de uma transição (entre clipes, na abertura e no fecho) dão o quadro da
    /// exportação: o `xfade` acha o progresso pelo primeiro quadro, e a janela completa os lados até a transição inteira.
    /// A segunda variante põe `escurecer` e sobreposição no mesmo clipe (entrada e fecho escurecendo).
    #[test]
    fn janela_no_meio_da_transicao_da_o_quadro_da_exportacao() {
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let mundo = Mundo::novo(&cap).com("m1", &m.info);
        for [abertura, t2, t3, fecho] in [["cortina_esq", "circulo", "fundir", "deslizar_dir"], ["escurecer", "fundir", "cortina_dir", "escurecer"]] {
            let mut cs = trechos(&m);
            cs[0]["transicao_entrada"] = tr(abertura, 0.3);
            cs[1]["transicao_entrada"] = tr(t2, 0.5);
            cs[2]["transicao_entrada"] = tr(t3, 0.4);
            let p = projeto(W, H, (30000, 1001), vec![midia("m1", "video", json!([]))], cs, json!({ "fecho": tr(fecho, 0.3) }));
            let final_ = mundo.compilar(&p, &alvo_final("cpu")).unwrap();
            let pasta = fontes::pasta_temporaria("p11-janela-final");
            let r = executor::rodar(&cap, &final_, &pasta, &Opcoes { sem_perdas: true, ..Opcoes::default() }).unwrap();
            let e = yuv(&cap, &r.blocos[0], W, H);
            let e_rgb = rgb709(&cap, &r.blocos[0], W, H);
            let total = final_.total_q;
            let ctx = Contexto { fontes: &mundo.fontes, cap: &cap, rastreios: &SemRastreios, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
            let linha = tempo::resolver(&p, &ctx);
            let (j1, s2) = (linha.clipes[1].inicio_q, linha.clipes[1].sobrepoe_q as u64);
            let (j2, s3) = (linha.clipes[2].inicio_q, linha.clipes[2].sobrepoe_q as u64);
            let (abre, fecha) = (tempo::slots(0.3, linha.fps), tempo::slots(0.3, linha.fps));
            assert!(s2 > 6 && s3 > 6 && abre > 4);
            let janelas = [
                (j1 + 3, j1 + 9),
                (j1, j1 + s2),
                (j1 - 4, j1 + 2),
                (j1 + s2 - 2, j1 + s2 + 5),
                (j2 + 1, j2 + s3 - 1),
                (2, abre - 1),
                (0, abre + 3),
                (total - fecha + 1, total - 1),
                (total - fecha - 2, total),
            ];
            for (de, ate) in janelas {
                let pv = mundo.compilar(&p, &Alvo::Previa { altura: H, de_q: de, ate_q: ate }).unwrap();
                assert_eq!((pv.canvas.w, pv.canvas.h), (W, H));
                let qv = quadros(&cap, &pv, "p11-janela-previa");
                for (i, x) in qv.iter().enumerate() {
                    assert!(x == &e[de as usize + i], "{abertura}…{fecho}: prévia [{de}, {ate}) ({}): slot {} ≠ exportação", cap.versao, de as usize + i);
                }
            }
            // prévia reduzida: fundo e lados no tamanho do alvo; a janela dá os quadros da prévia inteira
            let reduzida = |de: u64, ate: u64| {
                let pv = mundo.compilar(&p, &Alvo::Previa { altura: H / 2, de_q: de, ate_q: ate }).unwrap();
                assert_eq!((pv.canvas.w, pv.canvas.h), (W / 2, H / 2));
                quadros(&cap, &pv, "p11-janela-reduzida")
            };
            let inteira = reduzida(0, total);
            assert!(inteira[(j1 + s2 / 2) as usize] != inteira[(j1 - 1) as usize]);
            for (de, ate) in [janelas[0], janelas[5], janelas[7]] {
                assert!(reduzida(de, ate)[..] == inteira[de as usize..ate as usize], "{abertura}…{fecho}: prévia reduzida [{de}, {ate}) ({})", cap.versao);
            }
            for q in [0, abre / 2, abre - 1, j1, j1 + s2 / 2, j1 + s2 - 1, j2 + s3 / 2, total - fecha, total - 1] {
                let pq = mundo.compilar(&p, &Alvo::Quadro { q, largura: W }).unwrap();
                let rq = executor::rodar(&cap, &pq, &fontes::pasta_temporaria("p11-janela-quadro"), &Opcoes::default()).unwrap_or_else(|e| panic!("quadro {q}: {e}"));
                let bmp = image::open(&rq.blocos[0]).expect("bmp").to_rgb8();
                let lido = QuadroRgb { w: bmp.width(), h: bmp.height(), px: bmp.into_raw() };
                assert_eq!(medir::pixels_diferentes(&e_rgb[q as usize], &lido, 1), 0, "{abertura}…{fecho}: quadro final do slot {q} ≠ exportação ({})", cap.versao);
            }
        }
    }

    /// A transição no meio de um bloco e a que começa na fronteira entre dois blocos (o corte de 5 min recua para
    /// antes da cauda: o bloco 2 começa no primeiro quadro da transição) dão os mesmos quadros.
    #[test]
    fn transicao_no_bloco_e_na_fronteira_igual() {
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let mundo = Mundo::novo(&cap).com("m1", &m.info);
        let midias = vec![midia("m1", "video", json!([]))];
        let com = |n_a: u32| {
            let mut b = trecho("c2", "m1", &m.info.quadros, 30, 90, 1.0, json!([]));
            b["transicao_entrada"] = tr("fundir", 0.5);
            projeto(W, H, (30, 1), midias.clone(), vec![titulo("c1", "#c03020", n_a as f64 / 30.0), b], json!({}))
        };
        // título de 9010 slots: o corte de 5 min (9000) cairia na cauda (8995..9010) e recua para 8995
        let longo = com(9010);
        let plano = mundo.compilar(&longo, &alvo_final("cpu")).unwrap();
        assert_eq!(plano.blocos.iter().map(|b| (b.de_q, b.n_q)).collect::<Vec<_>>(), vec![(0, 8995), (8995, 60)]);
        let mut so2 = plano.clone();
        so2.blocos.retain(|b| b.indice == 1);
        so2.total_q = 60;
        let fronteira = quadros(&cap, &so2, "p11-fronteira");
        // no meio de um bloco: a prévia que pega corpo e cauda do título e o trecho inteiro
        let pv = mundo.compilar(&longo, &Alvo::Previa { altura: H, de_q: 8990, ate_q: 9055 }).unwrap();
        assert_eq!(pv.blocos.len(), 1);
        let meio = quadros(&cap, &pv, "p11-fronteira-previa");
        assert!(meio[5..] == fronteira[..], "fronteira × meio do bloco ({})", cap.versao);
        // janela que fecha no último quadro da transição (o bloco que acaba no fim da cabeça): do trecho, só a cabeça
        let pf = mundo.compilar(&longo, &Alvo::Previa { altura: H, de_q: 8990, ate_q: 9010 }).unwrap();
        assert!(quadros(&cap, &pf, "p11-fronteira-cabeca")[..] == meio[..20], "janela até o fim da cabeça ({})", cap.versao);
        // e com o título curto (programa de um bloco só): os mesmos 15 quadros da transição
        let curto = com(40);
        let pc = mundo.compilar(&curto, &alvo_final("cpu")).unwrap();
        assert_eq!(pc.blocos.len(), 1);
        let qc = quadros(&cap, &pc, "p11-fronteira-curto");
        assert!(qc[25..40] == fronteira[..15] && qc[40..] == fronteira[15..], "transição num bloco curto ({})", cap.versao);
        // a mistura de fato aconteceu: o meio da transição não é nenhum dos lados
        assert!(fronteira[7] != fronteira[0] && fronteira[7] != fronteira[20]);
    }

    /// `escurecer` e sobreposição no mesmo clipe, com um quadro congelado: fora da sobreposição cada slot é o do clipe
    /// sozinho com o seu `escurecer`; dentro, a conta do `fundir` sobre esses quadros.
    #[test]
    fn escurecer_e_sobreposicao_no_mesmo_clipe() {
        // mede os pixels da cadeia; o selo (camada global) cairia na faixa medida
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let mundo = Mundo::novo(&cap).com("m1", &m.info);
        let tab = &m.info.quadros;
        let fps = (30000, 1001);
        let mut c1 = trecho("c1", "m1", tab, 30, 75, 1.0, json!([]));
        c1["transicao_entrada"] = tr("escurecer", 0.3);
        let congelado = |entrada: Value| {
            json!({ "tipo": "congelar", "id": "c2", "rotulo": "", "desligado": false, "transicao_entrada": entrada, "efeitos": [], "midia": "m1",
                    "instante_s": tab.tempo(200), "instante_pts": tab.pts[200], "duracao_s": 1.0 })
        };
        let fecho = json!({ "fecho": tr("escurecer", 0.3) });
        let render = |cs: Vec<Value>, extra: Value, nome: &str| {
            let p = projeto(W, H, fps, vec![midia("m1", "video", json!([]))], cs, extra);
            quadros(&cap, &mundo.compilar(&p, &alvo_final("cpu")).unwrap(), nome)
        };
        let s1 = render(vec![c1.clone()], json!({}), "p11-misto-a");
        let s2 = render(vec![congelado(Value::Null)], fecho.clone(), "p11-misto-b");
        let e = render(vec![c1, congelado(tr("fundir", 0.5))], fecho, "p11-misto");
        let d = tempo::slots(0.5, Racional { num: fps.0, den: fps.1 }) as usize;
        let (n1, n2) = (s1.len(), s2.len());
        assert_eq!(e.len(), n1 + n2 - d, "{}", cap.versao);
        // os dois `escurecer` existem: o clipe abre no preto e o congelado fecha mais escuro do que começa
        assert!(s1[0] == chapado(W, H, "#000000") && s2[n2 - 1] != s2[0]);
        for (slot, x) in e.iter().enumerate() {
            if slot < n1 - d {
                assert!(x == &s1[slot], "slot {slot} ≠ clipe 1 sozinho ({})", cap.versao);
            } else if slot >= n1 {
                assert!(x == &s2[slot - (n1 - d)], "slot {slot} ≠ congelado sozinho ({})", cap.versao);
            } else {
                let k = slot - (n1 - d);
                let conta = fade_a_mao(&s1[slot], &s2[k], k as u32, d as u32);
                let pior = x.iter().zip(&conta).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
                assert!(pior <= 1, "slot {slot} (quadro {k} de {d}) difere da conta em {pior} ({})", cap.versao);
            }
        }
    }

    /// Todo tipo do catálogo, nas três taxas, dá os mesmos bytes no 9.0.1 e no 7.0.2 (o hash do composto não pode
    /// depender da versão).
    #[test]
    fn cada_transicao_igual_nos_dois_binarios() {
        let Some(cap) = fontes::cap() else { return };
        let Some(outro) = fontes::outro_ffmpeg(&cap) else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        for fps in TAXAS {
            for &(tipo, _) in XFADE {
                let mut cs = trechos(&m);
                for c in cs.iter_mut() {
                    c["transicao_entrada"] = tr(tipo, 0.4);
                }
                let mut p = projeto(W, H, fps, vec![midia("m1", "video", json!([]))], cs, json!({ "fecho": tr(tipo, 0.3) }));
                p.saida.cor_fundo = "#3060a0".into();
                let a = quadros(&cap, &Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap(), "p11-binario-a");
                let b = quadros(&outro, &Mundo::novo(&outro).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap(), "p11-binario-b");
                assert!(a == b, "{tipo} a {fps:?}: {} × {}", cap.versao, outro.versao);
            }
        }
    }

    /// Luma do retângulo [x0, x1) × [y0, y1) de um quadro yuv420p.
    fn luma(q: &[u8], x: (usize, usize), y: (usize, usize)) -> Vec<u8> {
        (y.0..y.1).flat_map(|l| q[l * W as usize + x.0..l * W as usize + x.1].iter().copied()).collect()
    }

    fn distancia(a: &[u8], b: &[u8]) -> f64 {
        a.iter().zip(b).map(|(x, y)| x.abs_diff(*y) as f64).sum::<f64>() / a.len() as f64
    }

    /// O nome de cada tipo diz para onde a imagem anda, medido na imagem e não pela tabela `XFADE` (esquerda trocada
    /// por direita passaria no oráculo, que lê a mesma tabela): no quadro do meio, de que lado está quem entra, e se
    /// ele chega parado (cortina) ou andando (deslizar). Quem sai é uma cor chapada; quem entra, a fonte da matriz.
    #[test]
    fn cada_tipo_anda_como_o_nome_diz() {
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let mundo = Mundo::novo(&cap).com("m1", &m.info);
        let midias = vec![midia("m1", "video", json!([]))];
        let b = || trecho("c2", "m1", &m.info.quadros, 30, 75, 1.0, json!([]));
        let render = |cs: Vec<Value>, nome: &str| quadros(&cap, &mundo.compilar(&projeto(W, H, (30, 1), midias.clone(), cs, json!({})), &alvo_final("cpu")).unwrap(), nome);
        let entra = render(vec![b()], "p11-sentido-b");
        let (w, h, d) = (W as usize, H as usize, 12usize);
        let (esq, dir, tudo) = ((0, w / 4), (3 * w / 4, w), (0, h));
        for &tipo in TRANSICOES {
            let mut c2 = b();
            c2["transicao_entrada"] = tr(tipo, 0.4);
            let q = render(vec![titulo("c1", "#c03020", 1.0), c2], "p11-sentido");
            let k = d / 2;
            let (sai, meio, eb) = (&q[0], &q[30 - d + k], &entra[k]);
            // quem entra, parado e deslocado de meia largura, e quem sai: os três bem diferentes nas duas faixas
            let (a_esq, a_dir) = (luma(sai, esq, tudo), luma(sai, dir, tudo));
            let (b_esq, b_dir) = (luma(eb, esq, tudo), luma(eb, dir, tudo));
            let (b_esq_andou, b_dir_andou) = (luma(eb, (w / 2, 3 * w / 4), tudo), luma(eb, (w / 4, w / 2), tudo));
            for (x, y) in [(&a_esq, &b_esq), (&a_dir, &b_dir), (&b_esq, &b_esq_andou), (&b_dir, &b_dir_andou)] {
                assert!(distancia(x, y) > 4.0, "fonte de teste sem contraste: {}", distancia(x, y));
            }
            let (m_esq, m_dir) = (luma(meio, esq, tudo), luma(meio, dir, tudo));
            let visto = match tipo {
                "escurecer" | "fundir" => continue,
                "cortina_esq" => m_dir == b_dir && m_esq == a_esq,
                "cortina_dir" => m_esq == b_esq && m_dir == a_dir,
                "deslizar_esq" => m_dir == b_dir_andou && m_esq == a_esq,
                "deslizar_dir" => m_esq == b_esq_andou && m_dir == a_dir,
                "circulo" => {
                    let (centro, canto) = (((w / 2 - 4, w / 2 + 4), (h / 2 - 4, h / 2 + 4)), ((0, 8), (0, 8)));
                    let de = |q: &[u8], r: ((usize, usize), (usize, usize))| luma(q, r.0, r.1);
                    assert!(distancia(&de(sai, centro), &de(eb, centro)) > 4.0 && distancia(&de(sai, canto), &de(eb, canto)) > 4.0, "fonte de teste sem contraste");
                    distancia(&de(meio, centro), &de(eb, centro)) <= 1.0 && distancia(&de(meio, canto), &de(sai, canto)) <= 3.0
                }
                outro => panic!("tipo do catálogo sem sentido medido: {outro}"),
            };
            assert!(visto, "{tipo} ({}): o quadro do meio não anda como o nome diz", cap.versao);
        }
    }

    /// Transição de um slot só (0,04 s a 25 qps): a contagem fecha e nada fora do slot da junção muda.
    #[test]
    fn transicao_de_um_slot() {
        let Some(cap) = fontes::cap() else { return };
        let mundo = Mundo::novo(&cap);
        let render = |tipo: Option<&str>, nome: &str| {
            let mut cs = vec![titulo("t1", "#f0d040", 0.4), titulo("t2", "#203070", 0.4)];
            if let Some(tipo) = tipo {
                cs[1]["transicao_entrada"] = tr(tipo, 0.04);
            }
            quadros(&cap, &mundo.compilar(&projeto(W, H, (25, 1), vec![], cs, json!({})), &alvo_final("cpu")).unwrap(), nome)
        };
        let sem = render(None, "p11-um-sem");
        assert_eq!(sem.len(), 20);
        for &(tipo, _) in XFADE {
            let com = render(Some(tipo), "p11-um");
            assert_eq!(com.len(), 19, "{tipo} ({})", cap.versao);
            assert!(com[..9] == sem[..9] && com[10..] == sem[11..], "{tipo} ({}): quadro fora da junção mudou", cap.versao);
            assert_eq!(mistura(&com[9], &sem[9], &sem[10]).1, 0, "{tipo} ({}): valor fora dos dois lados", cap.versao);
        }
    }
}
