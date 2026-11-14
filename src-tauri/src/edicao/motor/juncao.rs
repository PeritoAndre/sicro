//! Junção dos clipes dentro do bloco e catálogo de transições (§5.4): corte, `escurecer` e, no P1.1, as do `xfade`
//! (junção por pedaços; primeiro clipe e `fecho` com um clipe virtual da cor do fundo). O som das transições é do
//! `audio.rs` (P1.5, O3). Dono: P1.1.

use super::grafo::{filtro, Grafo};
use super::plano::Canvas;
use crate::edicao::modelo::Transicao;
use crate::error::{Result, SicroError};

/// Catálogo de transições aceitas (`Transicao.tipo`); fora dele = `nao_suportado` (validação e motor, a mesma lista).
/// Se a transição sobrepõe os clipes, quem diz é `tempo::transicao_sobrepoe`.
pub const TRANSICOES: &[&str] = &["escurecer"];

/// Um clipe pronto para juntar: rótulo do vídeo, slots e as transições de entrada e de saída (em slots).
/// O vídeo chega com o pts = slot local do clipe (base 1/fps), que é como o `fade` acha o lugar dele mesmo quando
/// o bloco pega o clipe pelo meio; `n_q` = slots do clipe INTEIRO; o rótulo traz só os slots [de, ate) dele.
/// `sobrepoe` = slots em que este clipe se sobrepõe ao anterior (transição com sobreposição; 0 no corte e no
/// `escurecer`). O bloco nunca corta dentro de uma transição (`blocos::dividir`).
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

fn segundos(slots: u32, fps: crate::edicao::modelo::Racional) -> String {
    // 9 casas: o `fade` conta em microssegundos
    format!("{:.9}", slots as f64 * fps.den as f64 / fps.num as f64)
}

fn conferir(t: &Transicao) -> Result<()> {
    if TRANSICOES.contains(&t.tipo.as_str()) {
        Ok(())
    } else {
        Err(SicroError::Validation(format!("nao_suportado: transicao {}", t.tipo)))
    }
}

/// Junta as peças na ordem e devolve o rótulo do vídeo do bloco (pts = slot do bloco, base 1/fps). `canvas` dá
/// tamanho e cor do clipe virtual do primeiro clipe e do `fecho`.
// CONTRATO: `canvas` no lugar de `fps` (abertura da Fase 1).
pub fn montar(g: &mut Grafo, pecas: &[Peca], canvas: &Canvas) -> Result<String> {
    let fps = canvas.fps;
    if pecas.is_empty() {
        return Err(SicroError::Validation("vazio".into()));
    }
    // junção por pedaços (`xfade`) é do P1.1
    if let Some(p) = pecas.iter().find(|p| p.sobrepoe > 0) {
        return Err(SicroError::Validation(format!("nao_suportado: transicao com sobreposicao no clipe {}", p.clipe)));
    }
    let mut prontas = Vec::with_capacity(pecas.len());
    for p in pecas {
        let mut f = Vec::new();
        // `escurecer`: por tempo (o pts é o slot do clipe), para a janela no meio do clipe dar o mesmo quadro
        if let Some((t, d)) = p.entrada.as_ref().filter(|(_, d)| *d > 0) {
            conferir(t)?;
            f.push(filtro("fade", None, &[("t", "in".into()), ("st", "0".into()), ("d", segundos((*d).min(p.n_q), fps))]));
        }
        if let Some((t, d)) = p.saida.as_ref().filter(|(_, d)| *d > 0) {
            conferir(t)?;
            let d = (*d).min(p.n_q);
            f.push(filtro("fade", None, &[("t", "out".into()), ("st", segundos(p.n_q - d, fps)), ("d", segundos(d, fps))]));
        }
        f.push("setpts=PTS-STARTPTS".to_string());
        let r = g.rotulo("p");
        g.cadeia(&[&p.rotulo], &f, &[&r]);
        prontas.push(r);
    }
    let saida = g.rotulo("j");
    let entradas: Vec<&str> = prontas.iter().map(String::as_str).collect();
    // o `concat` empurra a peça seguinte pela duração média da anterior (zero com 1 quadro: o `fps` descartava um).
    // Cada peça já traz a contagem exata: o slot é a ordem. O `fps` fica, também com uma peça só, pela duração do
    // quadro: sem ela o 7.0.2 fecha o MP4 do bloco sem o último passo e a junção repete o pts.
    let mut f = if prontas.len() > 1 { vec![format!("concat=n={}:v=1:a=0", prontas.len())] } else { Vec::new() };
    f.extend([format!("settb={}/{}", fps.den, fps.num), "setpts=N".to_string(), format!("fps={}/{}", fps.num, fps.den)]);
    g.cadeia(&entradas, &f, &[&saida]);
    Ok(saida)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::modelo::{AudioTransicao, Racional};

    fn t(tipo: &str) -> Transicao {
        Transicao { tipo: tipo.into(), duracao_s: 0.5, audio: AudioTransicao::Acompanhar }
    }

    fn peca(rotulo: &str, n_q: u32, entrada: Option<(Transicao, u32)>, saida: Option<(Transicao, u32)>) -> Peca {
        Peca { rotulo: rotulo.into(), clipe: rotulo.into(), n_q, de: 0, ate: n_q, sobrepoe: 0, entrada, saida }
    }

    #[test]
    fn corte_seco_concatena_e_escurecer_vira_fade_por_tempo() {
        let fps = Racional { num: 30, den: 1 };
        let canvas = Canvas { w: 640, h: 360, fps, cor_fundo: "#000000".into() };
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
        // transição de outra fase: recusada com o nome
        let fora = vec![peca("c", 30, Some((t("dissolver"), 10)), None)];
        assert!(montar(&mut Grafo::novo(), &fora, &canvas).unwrap_err().to_string().contains("nao_suportado: transicao dissolver"));
        assert!(montar(&mut Grafo::novo(), &[], &canvas).is_err());
    }
}
