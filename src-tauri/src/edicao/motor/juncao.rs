//! Junção dos clipes dentro do bloco e catálogo de transições (§5.4). Fase 0: corte e `escurecer`. Dono: P0.6a.

use super::grafo::{filtro, Grafo};
use crate::edicao::modelo::Transicao;
use crate::error::{Result, SicroError};

/// Catálogo de transições aceitas (`Transicao.tipo`); fora dele = erro `transicao`. As do `xfade` entram no P1.1.
/// Se a transição sobrepõe os clipes, quem diz é `tempo::transicao_sobrepoe`.
pub const TRANSICOES: &[&str] = &["escurecer"];

/// Um clipe pronto para juntar: rótulo do vídeo, slots e as transições de entrada e de saída (em slots).
/// O vídeo chega com o pts = slot local do clipe (base 1/fps), que é como o `fade` acha o lugar dele mesmo quando
/// o bloco pega o clipe pelo meio; `n_q` = slots do clipe INTEIRO.
#[derive(Debug, Clone, PartialEq)]
pub struct Peca {
    pub rotulo: String,
    pub n_q: u32,
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

/// Junta as peças na ordem e devolve o rótulo do vídeo do bloco (pts = slot do bloco, base 1/fps).
pub fn montar(g: &mut Grafo, pecas: &[Peca], fps: crate::edicao::modelo::Racional) -> Result<String> {
    if pecas.is_empty() {
        return Err(SicroError::Validation("vazio".into()));
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

    #[test]
    fn corte_seco_concatena_e_escurecer_vira_fade_por_tempo() {
        let fps = Racional { num: 30, den: 1 };
        let mut g = Grafo::novo();
        let pecas = vec![
            Peca { rotulo: "c000v".into(), n_q: 90, entrada: None, saida: Some((t("escurecer"), 15)) },
            Peca { rotulo: "c001v".into(), n_q: 60, entrada: Some((t("escurecer"), 15)), saida: None },
        ];
        let j = montar(&mut g, &pecas, fps).unwrap();
        assert_eq!(
            g.texto(),
            format!(
                "[c000v]fade=t=out:st=2.500000000:d=0.500000000,setpts=PTS-STARTPTS[p000];\n\
                 [c001v]fade=t=in:st=0:d=0.500000000,setpts=PTS-STARTPTS[p001];\n[p000][p001]concat=n=2:v=1:a=0,settb=1/30,setpts=N,fps=30/1[{j}]"
            )
        );
        // uma peça só: sem concat
        let mut g = Grafo::novo();
        let j = montar(&mut g, &pecas[..1], fps).unwrap();
        assert!(g.texto().ends_with(&format!("[p000]settb=1/30,setpts=N,fps=30/1[{j}]")), "{}", g.texto());
        assert!(!g.texto().contains("concat"));
        // transição de outra fase: recusada com o nome
        let fora = vec![Peca { rotulo: "c".into(), n_q: 30, entrada: Some((t("dissolver"), 10)), saida: None }];
        assert!(montar(&mut Grafo::novo(), &fora, fps).unwrap_err().to_string().contains("nao_suportado: transicao dissolver"));
        assert!(montar(&mut Grafo::novo(), &[], fps).is_err());
    }
}
