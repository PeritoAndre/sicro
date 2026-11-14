//! Executor mínimo de `Plano`, SÓ para os testes do motor: rasteriza as camadas, roda cada bloco, confere a leitura
//! (`frames in` do `fps@` e pts do `showinfo`) e junta. O de verdade é `edicao::executar` (P0.7).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use super::medir;
use crate::edicao::camada::{self, CamadaPronta};
use crate::edicao::ffmpeg::args;
use crate::edicao::ffmpeg::exec::{self, Comando};
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::motor::plano::{Bloco, Juncao, Papel, Plano};
use crate::error::{Result, SicroError};

/// Como rodar: `sem_perdas` troca o codificador por vídeo cru (para medir pixel); `globais` entram antes das
/// entradas (`-cpuflags …`); `sem_busca` lê toda mídia do começo (o "refeito sem `-ss`"); `juntar` faz o mux final.
#[derive(Debug, Clone, Default)]
pub struct Opcoes {
    pub sem_perdas: bool,
    pub globais: Vec<String>,
    pub sem_busca: bool,
    pub juntar: bool,
}

/// O que saiu: o arquivo de cada bloco, a coluna de hash dos `framehash` na ordem dos slots, os `frames in` de
/// cada `fps@` por bloco, o arquivo junto e o maior pico de memória (RSS, kB) de um ffmpeg.
#[derive(Debug, Default)]
pub struct Rodado {
    pub blocos: Vec<PathBuf>,
    pub compostos: Vec<String>,
    pub lidos: Vec<HashMap<String, u64>>,
    pub junto: Option<PathBuf>,
    pub pico_kb: u64,
}

fn divergente(d: String) -> SicroError {
    SicroError::Validation(format!("leitura_divergente: {d}"))
}

fn comando(cap: &Capacidades, pasta: &Path, nome: &str, a: Vec<String>) -> Comando {
    Comando { nome: nome.into(), programa: cap.binario.clone(), args: a, pasta: pasta.to_path_buf(), ambiente: Vec::new(), sem_ambiente: Vec::new(), stderr: pasta.join(format!("{nome}.stderr")) }
}

/// Pico de memória do processo, lido do `-benchmark` (`maxrss=NNNKiB`).
fn pico_kb(stderr: &Path) -> u64 {
    let texto = std::fs::read(stderr).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    texto.lines().filter_map(|l| l.split("maxrss=").nth(1)).filter_map(|r| r.trim_end_matches("KiB").trim().parse().ok()).max().unwrap_or(0)
}

/// Confere o que o bloco leu: `frames in` = lidos + 1 (o clone do fim) e, na exportação final, os pts.
pub fn conferir(bloco: &Bloco, stderr: &Path) -> Result<HashMap<String, u64>> {
    let lidos = exec::quadros_lidos(stderr)?;
    let pts = exec::pts_lidos(stderr)?;
    for c in &bloco.conferir {
        let n = lidos.get(&c.rotulo).copied();
        if n != Some(c.lidos as u64 + 1) {
            return Err(divergente(format!("bloco {} clipe {}: frames in {:?} × lidos {} + 1", bloco.indice, c.clipe, n, c.lidos)));
        }
        if !c.pts.is_empty() && pts.get(&c.rotulo) != Some(&c.pts) {
            let v = pts.get(&c.rotulo).map(|v| v.len());
            return Err(divergente(format!("bloco {} clipe {}: pts lidos ({v:?}) × esperados ({})", bloco.indice, c.clipe, c.pts.len())));
        }
    }
    Ok(lidos)
}

fn rodar_bloco(cap: &Capacidades, plano: &Plano, bloco: &Bloco, camadas: &[CamadaPronta], pasta: &Path, op: &Opcoes) -> Result<(PathBuf, Vec<String>, HashMap<String, u64>, u64)> {
    for a in &bloco.auxiliares {
        args::gravar_texto(pasta, &a.nome, &a.conteudo)?;
    }
    let grafo = args::nome("b", bloco.indice, ".txt");
    args::gravar_texto(pasta, &grafo, &bloco.grafo)?;
    let mut a = args::globais();
    a.push("-benchmark".into());
    a.extend(op.globais.iter().cloned());
    for e in &bloco.entradas {
        match &e.papel {
            Papel::Camada { id } => {
                let c = camadas.iter().find(|c| &c.id == id).ok_or_else(|| SicroError::Validation(format!("camada {id} sem pedido")))?;
                match &c.lista {
                    // camada fixa: um PNG em laço na taxa do canvas
                    None => {
                        a.extend(["-loop".into(), "1".into(), "-framerate".into(), format!("{}/{}", plano.canvas.fps.num, plano.canvas.fps.den)]);
                        a.extend(["-i".into(), c.pasta.join(&c.arquivos[0].arquivo).to_string_lossy().into_owned()]);
                    }
                    Some(l) => {
                        a.extend(e.antes.iter().cloned());
                        a.extend(["-i".into(), c.pasta.join(l).to_string_lossy().into_owned()]);
                    }
                }
            }
            _ => {
                let antes = if op.sem_busca { e.antes_sem_busca.as_ref().unwrap_or(&e.antes) } else { &e.antes };
                a.extend(antes.iter().cloned());
                a.extend(["-i".into(), e.caminho.to_string_lossy().into_owned()]);
            }
        }
    }
    a.extend(args::grafo_em_arquivo(cap, &grafo));
    a.extend(["-map".into(), "[v]".into()]);
    let saida = if op.sem_perdas && bloco.saida.ends_with(".mp4") {
        a.extend(["-c:v", "rawvideo", "-fps_mode", "passthrough"].map(String::from));
        args::nome("b", bloco.indice, ".nut")
    } else {
        a.extend(bloco.codificador.iter().cloned());
        bloco.saida.clone()
    };
    a.push(saida.clone());
    if !bloco.framehash.is_empty() {
        a.extend(["-map", "[h]", "-fps_mode", "passthrough", "-f", "framehash", "-hash", "sha256"].map(String::from));
        a.push(bloco.framehash.clone());
    }
    let cmd = comando(cap, pasta, &args::nome("b", bloco.indice, ""), a);
    let fim = exec::rodar(&cmd, &AtomicBool::new(false))?;
    exec::exigir_sucesso(&cmd, &fim)?;
    let lidos = conferir(bloco, &cmd.stderr)?;
    let hashes = if bloco.framehash.is_empty() { Vec::new() } else { medir::coluna_de_hash(&std::fs::read_to_string(pasta.join(&bloco.framehash))?) };
    Ok((pasta.join(saida), hashes, lidos, pico_kb(&cmd.stderr)))
}

/// Roda o plano na pasta (camadas em `<pasta>/cache`). Leitura divergente = `Err("leitura_divergente: …")`.
pub fn rodar(cap: &Capacidades, plano: &Plano, pasta: &Path, op: &Opcoes) -> Result<Rodado> {
    std::fs::create_dir_all(pasta)?;
    let camadas = camada::rasterizar_todos(&plano.camadas, &pasta.join("cache"), &AtomicBool::new(false), &mut |_| {})?;
    let mut r = Rodado::default();
    for b in &plano.blocos {
        let (arquivo, hashes, lidos, pico) = rodar_bloco(cap, plano, b, &camadas, pasta, op)?;
        if !b.framehash.is_empty() && hashes.len() != b.n_q as usize {
            return Err(SicroError::Validation(format!("bloco {}: {} quadros compostos × {} slots", b.indice, hashes.len(), b.n_q)));
        }
        r.blocos.push(arquivo);
        r.compostos.extend(hashes);
        r.lidos.push(lidos);
        r.pico_kb = r.pico_kb.max(pico);
    }
    if let (true, Juncao::Concat { lista, args: a, extensao }) = (op.juntar, &plano.juncao) {
        args::gravar_texto(pasta, &lista.nome, &lista.conteudo)?;
        let mut linha: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-nostats", "-loglevel", "warning", "-f", "concat", "-safe", "0", "-i"].map(String::from).into();
        linha.push(lista.nome.clone());
        linha.extend(a.iter().cloned());
        let saida = format!("saida.{extensao}");
        linha.push(saida.clone());
        let cmd = comando(cap, pasta, "juncao", linha);
        let fim = exec::rodar(&cmd, &AtomicBool::new(false))?;
        exec::exigir_sucesso(&cmd, &fim)?;
        r.junto = Some(pasta.join(saida));
    }
    Ok(r)
}
