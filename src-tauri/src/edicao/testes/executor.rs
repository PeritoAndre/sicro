//! Executor mínimo de `Plano`, SÓ para os testes do motor: rasteriza as camadas, roda cada bloco, confere a leitura
//! (`frames in` do `fps@` e pts do `showinfo`) e junta. `exportar` faz a exportação inteira (blocos, áudio, junção,
//! conferência) e devolve o `Resultado` que o relatório lê. O de verdade é `edicao::executar` (P0.7).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use sha2::{Digest, Sha256};

use super::medir;
use crate::edicao::camada::{self, ArquivoCamada, CamadaPronta};
use crate::edicao::conferir;
use crate::edicao::executar::{BlocoFeito, LidosClipe, Resultado};
use crate::edicao::ffmpeg::args;
use crate::edicao::ffmpeg::exec::{self, Comando, Termino};
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::motor::audio;
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
    /// pts lidos (`showinfo@`) de cada clipe, um registro por bloco, na ordem dos blocos.
    pub pts: Vec<LidosClipe>,
    /// Camadas rasterizadas (relativas à pasta das camadas) e os processos rodados.
    pub camadas: Vec<ArquivoCamada>,
    pub passos: Vec<Termino>,
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

#[allow(clippy::type_complexity)]
fn rodar_bloco(cap: &Capacidades, plano: &Plano, bloco: &Bloco, camadas: &[CamadaPronta], pasta: &Path, op: &Opcoes) -> Result<(PathBuf, Vec<String>, HashMap<String, u64>, u64, Vec<LidosClipe>, Termino)> {
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
    // os pts que o `showinfo@` de cada clipe imprimiu (só na exportação final)
    let pts = exec::pts_lidos(&cmd.stderr)?;
    let por_clipe = bloco.conferir.iter().filter(|c| !c.pts.is_empty()).map(|c| LidosClipe { clipe: c.clipe.clone(), pts: pts.get(&c.rotulo).cloned().unwrap_or_default() }).collect();
    Ok((pasta.join(saida), hashes, lidos, pico_kb(&cmd.stderr), por_clipe, fim))
}

/// Roda o plano na pasta (camadas em `<pasta>/cache`). Leitura divergente = `Err("leitura_divergente: …")`.
pub fn rodar(cap: &Capacidades, plano: &Plano, pasta: &Path, op: &Opcoes) -> Result<Rodado> {
    std::fs::create_dir_all(pasta)?;
    let camadas = camada::rasterizar_todos(&plano.camadas, &pasta.join("cache"), &AtomicBool::new(false), &mut |_| {})?;
    let mut r = Rodado { camadas: camadas.iter().flat_map(|c| c.arquivos.iter().cloned()).collect(), ..Rodado::default() };
    for b in &plano.blocos {
        let (arquivo, hashes, lidos, pico, pts, passo) = rodar_bloco(cap, plano, b, &camadas, pasta, op)?;
        if !b.framehash.is_empty() && hashes.len() != b.n_q as usize {
            return Err(SicroError::Validation(format!("bloco {}: {} quadros compostos × {} slots", b.indice, hashes.len(), b.n_q)));
        }
        r.blocos.push(arquivo);
        r.compostos.extend(hashes);
        r.lidos.push(lidos);
        r.pico_kb = r.pico_kb.max(pico);
        r.pts.extend(pts);
        r.passos.push(passo);
    }
    if let (true, Juncao::Concat { lista, args: a, extensao }) = (op.juntar, &plano.juncao) {
        args::gravar_texto(pasta, &lista.nome, &lista.conteudo)?;
        let mut linha: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-nostats", "-loglevel", "warning", "-f", "concat", "-safe", "0", "-i"].map(String::from).into();
        linha.push(lista.nome.clone());
        // o passo de áudio já rodou (`exportar`): a junção leva o WAV como segunda entrada
        if let Some(passo) = plano.audio.as_ref().filter(|p| pasta.join(&p.saida).exists()) {
            linha.extend(["-i".to_string(), passo.saida.clone()]);
        }
        linha.extend(a.iter().cloned());
        let saida = format!("saida.{extensao}");
        linha.push(saida.clone());
        let cmd = comando(cap, pasta, "juncao", linha);
        let fim = exec::rodar(&cmd, &AtomicBool::new(false))?;
        exec::exigir_sucesso(&cmd, &fim)?;
        r.passos.push(fim);
        r.junto = Some(pasta.join(saida));
    }
    Ok(r)
}

fn sha256_de(arquivo: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(std::fs::read(arquivo)?)))
}

/// O passo de áudio do plano (grafo com ganho 0, a linha do E9): devolve o SHA-256 do PCM que o próprio passo gravou.
fn rodar_audio(cap: &Capacidades, plano: &Plano, pasta: &Path, passos: &mut Vec<Termino>) -> Result<Option<String>> {
    let Some(passo) = &plano.audio else { return Ok(None) };
    args::gravar_texto(pasta, "audio.txt", &audio::grafo_com_ganho(passo, None))?;
    let cmd = comando(cap, pasta, "audio", audio::argumentos(passo, cap, "audio.txt"));
    let fim = exec::rodar(&cmd, &AtomicBool::new(false))?;
    exec::exigir_sucesso(&cmd, &fim)?;
    passos.push(fim);
    let texto = std::fs::read_to_string(pasta.join(&passo.hash))?;
    let sha = texto.trim().strip_prefix("SHA256=").ok_or_else(|| SicroError::Validation(format!("hash do PCM ilegível: {texto}")))?;
    Ok(Some(sha.to_string()))
}

/// Exportação inteira do plano na pasta: áudio, blocos codificados, junção com o som, hash do composto e a
/// conferência do arquivo entregue (decodificado em CPU). Devolve o que rodou e o `Resultado` do relatório.
pub fn exportar(cap: &Capacidades, plano: &Plano, pasta: &Path, renderizado_em: &str) -> Result<(Rodado, Resultado)> {
    std::fs::create_dir_all(pasta)?;
    let mut passos = Vec::new();
    let audio_sha256 = rodar_audio(cap, plano, pasta, &mut passos)?;
    let mut r = rodar(cap, plano, pasta, &Opcoes { juntar: true, ..Opcoes::default() })?;
    passos.append(&mut r.passos);
    r.passos = passos;
    let arquivo = r.junto.clone().ok_or_else(|| SicroError::Validation("sem junção".into()))?;
    let framehashes: Vec<PathBuf> = plano.blocos.iter().map(|b| pasta.join(&b.framehash)).collect();
    let compostos = pasta.join("quadros.compostos.sha256");
    let composto_sha256 = conferir::hash_do_composto(&framehashes, &compostos)?;
    let conferencia = conferir::conferir(cap, &arquivo, plano.total_q, pasta, &AtomicBool::new(false), &mut |_| {})?;
    let mut blocos = Vec::with_capacity(plano.blocos.len());
    for (b, f) in plano.blocos.iter().zip(&r.blocos) {
        blocos.push(BlocoFeito { indice: b.indice, de_q: b.de_q, n_q: b.n_q, chave: b.chave.clone(), reaproveitado: false, sha256: sha256_de(f)? });
    }
    let codificador = plano.blocos[0].codificador.iter().skip_while(|a| *a != "-c:v").nth(1).cloned().unwrap_or_default();
    let res = Resultado {
        sha256: sha256_de(&arquivo)?,
        tamanho: std::fs::metadata(&arquivo)?.len(),
        arquivo,
        codificador,
        argumentos: plano.blocos[0].codificador.clone(),
        queda_para_cpu: None,
        composto_sha256: Some(composto_sha256),
        compostos: Some(compostos),
        audio_sha256,
        loudnorm: None,
        ganho_db: None,
        blocos,
        camadas: r.camadas.clone(),
        passos: r.passos.clone(),
        conferencia: Some(conferencia),
        lidos: r.pts.clone(),
        legendas: Vec::new(),
        trf: Vec::new(),
        renderizado_em: renderizado_em.to_string(),
    };
    Ok((r, res))
}
