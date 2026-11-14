//! Conferência da saída e hashes de conteúdo (D11, §5.8). Dono: P0.7.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::ffmpeg::exec::{self, Comando};
use super::ffmpeg::sonda::Capacidades;
use crate::error::{Result, SicroError};

/// Arquivo com um hash por quadro decodificado (vai para o retrato e para o pacote do júri).
pub(crate) const DECODIFICADOS: &str = "quadros.decodificados.sha256";

/// Resultado de decodificar o arquivo entregue: quadros contados, erros do decodificador,
/// hash do decodificado e o arquivo `quadros.decodificados.sha256` (um hash por quadro).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConferenciaSaida {
    pub quadros: u64,
    pub erros: u32,
    pub decodificado_sha256: String,
    pub decodificados: PathBuf,
}

/// Decodifica `arquivo` em CPU com `-xerror`, grava os hashes por quadro em `trabalho` e exige `total_q` quadros sem erro.
/// `progresso` recebe os quadros já decodificados.
pub fn conferir(
    cap: &Capacidades,
    arquivo: &Path,
    total_q: u64,
    trabalho: &Path,
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(u64),
) -> Result<ConferenciaSaida> {
    std::fs::create_dir_all(trabalho)?;
    let framehash = trabalho.join("conferir.framehash");
    let _ = std::fs::remove_file(&framehash);
    // nenhum `-hwaccel`: o decodificado é o que quem recebe consegue refazer em qualquer máquina
    let args: Vec<String> = [
        "-hide_banner",
        "-nostdin",
        "-y",
        "-nostats",
        "-loglevel",
        "error",
        "-xerror",
        "-i",
        &arquivo.to_string_lossy(),
        "-map",
        "0:v:0",
        "-fps_mode",
        "passthrough",
        "-f",
        "framehash",
        "-hash",
        "sha256",
        &framehash.to_string_lossy(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let cmd = Comando {
        nome: "conferir".into(),
        programa: cap.binario.clone(),
        args,
        pasta: trabalho.to_path_buf(),
        ambiente: Vec::new(),
        sem_ambiente: Vec::new(),
        stderr: trabalho.join("conferir.log"),
    };
    let fim = exec::rodar_com_progresso(&cmd, cancelar, &mut |p| progresso(p.quadro))?;
    exec::exigir_sucesso(&cmd, &fim)?;
    // com `-loglevel error`, toda linha do stderr é erro do decodificador
    let erros = exec::ultimas_linhas(&cmd.stderr, usize::MAX).len() as u32;
    let hashes = coluna_de_hash(&std::fs::read_to_string(&framehash)?);
    let _ = std::fs::remove_file(&framehash);
    let quadros = hashes.len() as u64;
    progresso(quadros);
    if erros > 0 || quadros != total_q {
        return Err(SicroError::Validation(format!(
            "{}: conferência da saída com {quadros} quadros de {total_q} e {erros} erros",
            super::mensagens::codigo::LEITURA_DIVERGENTE
        )));
    }
    let decodificados = trabalho.join(DECODIFICADOS);
    std::fs::write(&decodificados, texto_de_hashes(&hashes))?;
    Ok(ConferenciaSaida { quadros, erros, decodificado_sha256: hash_de_conteudo(&hashes), decodificados })
}

/// Coluna de hash de um arquivo `framehash` (uma linha por quadro; as colunas de pts ficam de fora).
pub fn coluna_de_hash(framehash: &str) -> Vec<String> {
    framehash
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.rsplit(',').next().map(|h| h.trim().to_string()))
        .collect()
}

fn texto_de_hashes(hashes: &[String]) -> String {
    let mut t = String::with_capacity(hashes.len() * 65);
    for h in hashes {
        t.push_str(h);
        t.push('\n');
    }
    t
}

/// Hash de conteúdo: SHA-256 do texto com um hash por linha (terminada em `\n`), na ordem dos slots.
pub fn hash_de_conteudo(hashes: &[String]) -> String {
    format!("{:x}", Sha256::digest(texto_de_hashes(hashes).as_bytes()))
}

/// Junta os `framehash` dos blocos (em ordem), grava `destino` (um hash por slot) e devolve o hash do composto.
pub fn hash_do_composto(framehashes: &[PathBuf], destino: &Path) -> Result<String> {
    let mut todos = Vec::new();
    for f in framehashes {
        todos.extend(coluna_de_hash(&std::fs::read_to_string(f)?));
    }
    if let Some(p) = destino.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(destino, texto_de_hashes(&todos))?;
    Ok(hash_de_conteudo(&todos))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::testes::{fontes, medir};

    const FH: &str = "#format: frame checksums\n#version: 2\n#hash: SHA256\n#tb 0: 1/30\n#media_type 0: video\n\
                      #stream#, dts,        pts, duration,     size, hash\n\
                      0,          0,          0,        1,    86400, aa11\n\
                      0,          1,          1,        1,    86400, bb22\n";

    #[test]
    fn coluna_de_hash_tira_comentarios_e_pts() {
        assert_eq!(coluna_de_hash(FH), ["aa11", "bb22"]);
        assert!(coluna_de_hash("#só comentário\n\n").is_empty());
    }

    #[test]
    fn hash_de_conteudo_e_o_do_texto_com_um_por_linha() {
        let h = vec!["aa11".to_string(), "bb22".to_string()];
        assert_eq!(hash_de_conteudo(&h), format!("{:x}", Sha256::digest(b"aa11\nbb22\n")));
        assert_ne!(hash_de_conteudo(&h), hash_de_conteudo(&["bb22".to_string(), "aa11".to_string()]));
        assert_eq!(hash_de_conteudo(&[]), format!("{:x}", Sha256::digest(b"")));
    }

    #[test]
    fn hash_do_composto_junta_na_ordem() {
        let d = tempfile::tempdir().unwrap();
        let (a, b) = (d.path().join("b000.framehash"), d.path().join("b001.framehash"));
        std::fs::write(&a, FH).unwrap();
        std::fs::write(&b, FH.replace("aa11", "cc33").replace("bb22", "dd44")).unwrap();
        let destino = d.path().join("x").join("quadros.compostos.sha256");
        let h = hash_do_composto(&[a.clone(), b.clone()], &destino).unwrap();
        assert_eq!(std::fs::read_to_string(&destino).unwrap(), "aa11\nbb22\ncc33\ndd44\n");
        assert_eq!(h, format!("{:x}", Sha256::digest(b"aa11\nbb22\ncc33\ndd44\n")));
        assert_ne!(h, hash_do_composto(&[b, a], &destino).unwrap());
    }

    /// MP4 de `n` quadros de `testsrc2` (libx264), gerado na pasta.
    fn sintetico(cap: &Capacidades, pasta: &Path, n: u32) -> PathBuf {
        let saida = pasta.join("sintetico.mp4");
        let args: Vec<String> = [
            "-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i", &format!("testsrc2=s=320x180:r=30:d={}", n as f64 / 30.0),
            "-frames:v", &n.to_string(), "-c:v", "libx264", "-preset", "veryfast", "-g", "15", "-pix_fmt", "yuv420p",
            &saida.to_string_lossy(),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let cmd = Comando {
            nome: "sintetico".into(),
            programa: cap.binario.clone(),
            args,
            pasta: pasta.to_path_buf(),
            ambiente: vec![],
            sem_ambiente: vec![],
            stderr: pasta.join("sintetico.log"),
        };
        let t = exec::rodar(&cmd, &AtomicBool::new(false)).unwrap();
        exec::exigir_sucesso(&cmd, &t).unwrap();
        saida
    }

    /// Contagem certa: passa, e o hash é o dos quadros decodificados (medido por outro caminho); contagem a mais
    /// ou a menos falha; arquivo danificado no meio falha (`-xerror`).
    #[test]
    fn conferir_conta_hash_e_recusa() {
        let Some(cap) = fontes::cap() else { return };
        let d = tempfile::tempdir().unwrap();
        let arq = sintetico(&cap, d.path(), 60);
        let nao = AtomicBool::new(false);
        let mut vistos = 0;
        let c = conferir(&cap, &arq, 60, &d.path().join("t"), &nao, &mut |q| vistos = q).unwrap();
        assert_eq!((c.quadros, c.erros, vistos), (60, 0, 60), "{}", cap.versao);
        let medidos = medir::framehash(&cap, &arq).unwrap();
        assert_eq!(medidos.len(), 60);
        assert_eq!(c.decodificado_sha256, hash_de_conteudo(&medidos));
        assert_eq!(std::fs::read_to_string(&c.decodificados).unwrap(), texto_de_hashes(&medidos));
        assert_eq!(c.decodificados, d.path().join("t").join(DECODIFICADOS));

        for errado in [59, 61] {
            let e = conferir(&cap, &arq, errado, &d.path().join("t2"), &nao, &mut |_| {}).unwrap_err().to_string();
            assert!(e.contains("leitura_divergente") && e.contains("60 quadros"), "{e}");
        }

        // bytes do meio do fluxo zerados: o decodificador acusa e `-xerror` para
        let mut bytes = std::fs::read(&arq).unwrap();
        let meio = bytes.len() / 2;
        for b in &mut bytes[meio..meio + 4000] {
            *b = 0x55;
        }
        let estragado = d.path().join("estragado.mp4");
        std::fs::write(&estragado, &bytes).unwrap();
        let e = conferir(&cap, &estragado, 60, &d.path().join("t3"), &nao, &mut |_| {});
        assert!(e.is_err(), "{}: arquivo danificado passou", cap.versao);

        let cancelado = AtomicBool::new(true);
        let e = conferir(&cap, &arq, 60, &d.path().join("t4"), &cancelado, &mut |_| {}).unwrap_err().to_string();
        assert!(e.starts_with("validation error: cancelado"), "{e}");
    }
}
