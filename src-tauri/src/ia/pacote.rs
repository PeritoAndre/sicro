//! Itens do Gerenciador de IA (molde `install_pessoas`): URL, bytes e SHA-256 fixos; arquivo que não confere não
//! entra. Lê da rede ou de uma pasta com os pacotes já baixados (testes, sem baixar 1,3 GB de novo).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};

use crate::error::{Result, SicroError};

/// Um arquivo a baixar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arquivo {
    pub nome: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
}

/// De onde vêm os arquivos: a URL de cada um, ou uma pasta com eles pelo nome.
#[derive(Debug, Clone)]
pub enum Fonte {
    Rede,
    Pastas(Vec<PathBuf>),
}

fn fs(e: impl std::fmt::Display) -> SicroError {
    SicroError::Filesystem(e.to_string())
}

pub(crate) fn cancelado() -> SicroError {
    SicroError::Validation("cancelado".into())
}

fn abrir(a: &Arquivo, fonte: &Fonte) -> Result<Box<dyn Read>> {
    match fonte {
        Fonte::Rede => {
            let tls = native_tls::TlsConnector::new().map_err(|e| SicroError::Validation(format!("TLS indisponível: {e}")))?;
            let agente = ureq::AgentBuilder::new()
                .timeout_connect(std::time::Duration::from_secs(30))
                .timeout_read(std::time::Duration::from_secs(120))
                .tls_connector(std::sync::Arc::new(tls))
                .build();
            let r = agente.get(&a.url).call().map_err(|e| SicroError::Validation(format!("falha ao baixar ({}): {e}", a.url)))?;
            Ok(Box::new(r.into_reader()))
        }
        Fonte::Pastas(ps) => {
            let p = ps.iter().map(|d| d.join(&a.nome)).find(|p| p.is_file()).ok_or_else(|| fs(format!("{} não está nas pastas", a.nome)))?;
            Ok(Box::new(std::fs::File::open(&p).map_err(fs)?))
        }
    }
}

/// Grava `a` em `destino` conferindo tamanho e SHA-256; lê no máximo `bytes + 1` (teto). Erro apaga o arquivo.
/// `progresso(n)` recebe os bytes de cada pedaço.
pub fn obter(a: &Arquivo, fonte: &Fonte, destino: &Path, cancelar: &AtomicBool, progresso: &mut dyn FnMut(u64)) -> Result<()> {
    let r = (|| -> Result<()> {
        let mut leitor = abrir(a, fonte)?.take(a.bytes + 1);
        let mut f = std::fs::File::create(destino).map_err(fs)?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 16];
        let mut n = 0u64;
        loop {
            if cancelar.load(Ordering::Relaxed) {
                return Err(cancelado());
            }
            let k = leitor.read(&mut buf).map_err(|e| SicroError::Validation(format!("leitura de {}: {e}", a.nome)))?;
            if k == 0 {
                break;
            }
            n += k as u64;
            if n > a.bytes {
                return Err(SicroError::Validation(format!("{} maior que o esperado", a.nome)));
            }
            f.write_all(&buf[..k]).map_err(fs)?;
            h.update(&buf[..k]);
            progresso(k as u64);
        }
        f.flush().map_err(fs)?;
        if n != a.bytes {
            return Err(SicroError::Validation(format!("{}: {n} bytes, esperados {}", a.nome, a.bytes)));
        }
        if format!("{:x}", h.finalize()) != a.sha256 {
            return Err(SicroError::Validation(format!("{} não confere com o SHA-256 esperado", a.nome)));
        }
        Ok(())
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(destino);
    }
    r
}

fn nome_de(p: &Path) -> Option<String> {
    p.file_name().and_then(|n| n.to_str()).map(str::to_string)
}

fn copiar(mut de: impl Read, para: &Path) -> Result<()> {
    let mut f = std::fs::File::create(para).map_err(fs)?;
    std::io::copy(&mut de, &mut f).map_err(fs)?;
    Ok(())
}

/// Biblioteca do onnxruntime do pacote (`.tgz` no Linux, `.zip` no Windows): só os arquivos diretos de `lib/`
/// (`libonnxruntime.so.*`, provedores, `*.dll`), sem atalhos, sem o TensorRT. Devolve os nomes extraídos.
pub fn extrair_lib_onnxruntime(pacote: &Path, destino: &Path) -> Result<Vec<String>> {
    std::fs::create_dir_all(destino).map_err(fs)?;
    let serve = |p: &Path| -> Option<String> {
        let pai = p.parent()?.file_name()?.to_str()?;
        let nome = nome_de(p)?;
        let lib = nome.contains(".so") || nome.to_lowercase().ends_with(".dll");
        (pai == "lib" && lib && !nome.contains("tensorrt")).then_some(nome)
    };
    let mut feitos = Vec::new();
    let nome_pacote = nome_de(pacote).unwrap_or_default();
    if nome_pacote.ends_with(".zip") {
        let mut z = zip::ZipArchive::new(std::fs::File::open(pacote).map_err(fs)?).map_err(|e| SicroError::Validation(format!("zip inválido: {e}")))?;
        for i in 0..z.len() {
            let e = z.by_index(i).map_err(|e| SicroError::Validation(format!("zip: {e}")))?;
            let Some(nome) = e.enclosed_name().and_then(|p| serve(&p)).filter(|_| e.is_file()) else { continue };
            copiar(e, &destino.join(&nome))?;
            feitos.push(nome);
        }
    } else {
        let f = std::fs::File::open(pacote).map_err(fs)?;
        let mut t = tar::Archive::new(flate2::read::GzDecoder::new(std::io::BufReader::new(f)));
        for e in t.entries().map_err(|e| SicroError::Validation(format!("pacote inválido: {e}")))? {
            let e = e.map_err(|e| SicroError::Validation(format!("pacote inválido: {e}")))?;
            if !e.header().entry_type().is_file() {
                continue;
            }
            let Some(nome) = e.path().ok().and_then(|p| serve(&p)) else { continue };
            copiar(e, &destino.join(&nome))?;
            feitos.push(nome);
        }
    }
    #[cfg(unix)]
    for n in &feitos {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(destino.join(n), std::fs::Permissions::from_mode(0o755));
    }
    Ok(feitos)
}

/// Roda do PyPI (zip): as bibliotecas (`*.so*`, `*.dll`) achatadas em `destino`, sem o `nvblas` (dispensável), e o
/// `License.txt` como `LICENSE-<distribuição>.txt`. Devolve os nomes extraídos.
pub fn extrair_roda(roda: &Path, destino: &Path) -> Result<Vec<String>> {
    std::fs::create_dir_all(destino).map_err(fs)?;
    let dist = nome_de(roda).and_then(|n| n.split('-').next().map(str::to_string)).unwrap_or_default();
    let mut z = zip::ZipArchive::new(std::fs::File::open(roda).map_err(fs)?).map_err(|e| SicroError::Validation(format!("roda inválida: {e}")))?;
    let mut feitos = Vec::new();
    for i in 0..z.len() {
        let e = z.by_index(i).map_err(|e| SicroError::Validation(format!("roda: {e}")))?;
        if !e.is_file() {
            continue;
        }
        let Some(nome) = e.enclosed_name().and_then(|p| nome_de(&p)) else { continue };
        let baixo = nome.to_lowercase();
        let alvo = if baixo == "license.txt" {
            format!("LICENSE-{dist}.txt")
        } else if (baixo.contains(".so") || baixo.ends_with(".dll")) && !baixo.contains("nvblas") {
            nome
        } else {
            continue;
        };
        copiar(e, &destino.join(&alvo))?;
        feitos.push(alvo);
    }
    Ok(feitos)
}

/// Uma instalação por vez no processo: duas no mesmo item dividiriam a pasta `.novo`.
pub(crate) fn trava() -> std::sync::MutexGuard<'static, ()> {
    static T: std::sync::Mutex<()> = std::sync::Mutex::new(());
    T.lock().unwrap_or_else(|e| e.into_inner())
}

/// Arquivos simples (sem extrair) numa pasta nova: tudo ou nada. `progresso(recebidos, total)`.
pub fn instalar_arquivos(itens: &[Arquivo], fonte: &Fonte, pasta: &Path, cancelar: &AtomicBool, progresso: &mut dyn FnMut(u64, u64)) -> Result<()> {
    let _t = trava();
    let total: u64 = itens.iter().map(|a| a.bytes).sum();
    let novo = irma(pasta, "novo");
    let _ = std::fs::remove_dir_all(&novo);
    std::fs::create_dir_all(&novo).map_err(fs)?;
    let mut feito = 0u64;
    let r = itens.iter().try_for_each(|a| {
        obter(a, fonte, &novo.join(&a.nome), cancelar, &mut |k| {
            feito += k;
            progresso(feito, total);
        })
    });
    if let Err(e) = r {
        let _ = std::fs::remove_dir_all(&novo);
        return Err(e);
    }
    trocar(&novo, pasta)
}

/// `<pasta>.<sufixo>`, ao lado.
pub(crate) fn irma(pasta: &Path, sufixo: &str) -> PathBuf {
    let mut n = pasta.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    n.push(format!(".{sufixo}"));
    pasta.with_file_name(n)
}

/// Põe `novo` no lugar de `pasta`; a velha sai depois (no Windows, DLL carregada não sai: fica para a próxima).
pub(crate) fn trocar(novo: &Path, pasta: &Path) -> Result<()> {
    let velha = irma(pasta, "velho");
    let _ = std::fs::remove_dir_all(&velha);
    if pasta.exists() {
        std::fs::rename(pasta, &velha).map_err(|e| {
            let _ = std::fs::remove_dir_all(novo);
            SicroError::Validation(format!("{}: {e}", crate::edicao::mensagens::codigo::REABRIR))
        })?;
    }
    if let Err(e) = std::fs::rename(novo, pasta) {
        // a instalação anterior volta ao lugar
        let _ = std::fs::rename(&velha, pasta);
        let _ = std::fs::remove_dir_all(novo);
        return Err(fs(e));
    }
    let _ = std::fs::remove_dir_all(&velha);
    Ok(())
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;
    use std::io::BufRead;
    use std::net::TcpListener;

    fn sha(b: &[u8]) -> String {
        format!("{:x}", Sha256::digest(b))
    }

    /// Servidor HTTP falso em 127.0.0.1: `GET /<nome>` devolve o conteúdo da tabela (com o tamanho declarado),
    /// e `/<nome>?corta` manda só metade. Atende até `n` pedidos e para.
    pub(crate) fn servidor(arquivos: Vec<(String, Vec<u8>)>, n: usize) -> (String, std::thread::JoinHandle<()>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let h = std::thread::spawn(move || {
            for s in l.incoming().take(n) {
                let Ok(mut s) = s else { continue };
                let mut linha = String::new();
                let mut r = std::io::BufReader::new(s.try_clone().unwrap());
                if r.read_line(&mut linha).is_err() {
                    continue;
                }
                loop {
                    let mut l2 = String::new();
                    if r.read_line(&mut l2).map(|k| k <= 2).unwrap_or(true) {
                        break;
                    }
                }
                let caminho = linha.split_whitespace().nth(1).unwrap_or("/").trim_start_matches('/').to_string();
                let (nome, corta) = match caminho.split_once('?') {
                    Some((n, q)) => (n.to_string(), q == "corta"),
                    None => (caminho, false),
                };
                match arquivos.iter().find(|(n, _)| *n == nome) {
                    Some((_, b)) => {
                        let corpo = if corta { &b[..b.len() / 2] } else { &b[..] };
                        let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", b.len());
                        let _ = s.write_all(corpo);
                    }
                    None => {
                        let _ = write!(s, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                }
            }
        });
        (base, h)
    }

    fn pasta_tmp(nome: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("p21-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Download pelo servidor falso: certo entra com os bytes iguais; SHA-256 errado, arquivo cortado e cancelamento
    /// não deixam nada na pasta (nem a pasta nova); arquivo maior que o declarado para no teto.
    #[test]
    fn baixa_confere_e_recusa() {
        let a = b"primeiro arquivo".repeat(5000);
        let b = vec![7u8; 300_000];
        let (base, h) = servidor(vec![("a.bin".into(), a.clone()), ("b.bin".into(), b.clone())], 7);
        let item = |nome: &str, dados: &[u8], url: String| Arquivo { nome: nome.into(), url, bytes: dados.len() as u64, sha256: sha(dados) };
        let d = pasta_tmp("baixa");
        let alvo = d.join("item");
        let nao = AtomicBool::new(false);
        let mut vistos = Vec::new();
        let itens = [item("a.bin", &a, format!("{base}/a.bin")), item("b.bin", &b, format!("{base}/b.bin"))];
        instalar_arquivos(&itens, &Fonte::Rede, &alvo, &nao, &mut |r, t| vistos.push((r, t))).unwrap();
        assert_eq!(std::fs::read(alvo.join("a.bin")).unwrap(), a);
        assert_eq!(std::fs::read(alvo.join("b.bin")).unwrap(), b);
        let total = (a.len() + b.len()) as u64;
        assert_eq!(vistos.last(), Some(&(total, total)));
        assert!(vistos.windows(2).all(|w| w[0].0 < w[1].0));

        let so = |itens: &[Arquivo], c: &AtomicBool| {
            let alvo = d.join("outro");
            let r = instalar_arquivos(itens, &Fonte::Rede, &alvo, c, &mut |_, _| {});
            let sobra: Vec<_> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n != "item").collect();
            (r.err().map(|e| e.to_string()).unwrap_or_default(), sobra)
        };
        // SHA-256 trocado
        let mut ruim = item("a.bin", &a, format!("{base}/a.bin"));
        ruim.sha256 = sha(b"outra coisa");
        let (e, sobra) = so(&[ruim], &nao);
        assert!(e.contains("SHA-256"), "{e}");
        assert!(sobra.is_empty(), "{sobra:?}");
        // conexão cortada no meio
        let (e, sobra) = so(&[item("b.bin", &b, format!("{base}/b.bin?corta"))], &nao);
        assert!(!e.is_empty() && sobra.is_empty(), "{e} {sobra:?}");
        // declarado menor que o que chega: para no teto
        let mut menor = item("a.bin", &a, format!("{base}/a.bin"));
        menor.bytes = 1000;
        let (e, sobra) = so(&[menor], &nao);
        assert!(e.contains("maior que o esperado") && sobra.is_empty(), "{e} {sobra:?}");
        // cancelado
        let sim = AtomicBool::new(true);
        let (e, sobra) = so(&[item("a.bin", &a, format!("{base}/a.bin"))], &sim);
        assert!(e.contains("cancelado") && sobra.is_empty(), "{e} {sobra:?}");
        // o servidor atendeu os 6 pedidos; o 7º é este, para ele terminar
        let _ = std::net::TcpStream::connect(base.trim_start_matches("http://"));
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Dois "Baixar" do mesmo item ao mesmo tempo (a tela perde o "Baixando…" ao trocar de aba): os dois terminam
    /// e a pasta fica com os arquivos inteiros, sem a pasta nova ao lado.
    #[test]
    fn duas_instalacoes_do_mesmo_item_nao_se_misturam() {
        let d = pasta_tmp("duas");
        let origem = d.join("origem");
        std::fs::create_dir_all(&origem).unwrap();
        let itens: Vec<Arquivo> = (0..4u8)
            .map(|i| {
                let dados: Vec<u8> = (0..6_000_000u32).map(|k| (k as u8).wrapping_mul(31).wrapping_add(i)).collect();
                std::fs::write(origem.join(format!("m{i}.bin")), &dados).unwrap();
                Arquivo { nome: format!("m{i}.bin"), url: String::new(), bytes: dados.len() as u64, sha256: sha(&dados) }
            })
            .collect();
        let alvo = d.join("item");
        let fonte = Fonte::Pastas(vec![origem.clone()]);
        let nao = AtomicBool::new(false);
        for _ in 0..3 {
            let r: Vec<Result<()>> = std::thread::scope(|s| {
                let h: Vec<_> = (0..2).map(|_| s.spawn(|| instalar_arquivos(&itens, &fonte, &alvo, &nao, &mut |_, _| {}))).collect();
                h.into_iter().map(|h| h.join().unwrap()).collect()
            });
            assert!(r.iter().all(|x| x.is_ok()), "{r:?}");
            for a in &itens {
                assert_eq!(sha(&std::fs::read(alvo.join(&a.nome)).unwrap()), a.sha256, "{}", a.nome);
            }
            assert!(!irma(&alvo, "novo").exists() && !irma(&alvo, "velho").exists());
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A biblioteca sai do `.tgz` achatada (sem cabeçalhos, sem `cmake/`, sem TensorRT, sem atalhos) e a roda dá
    /// as bibliotecas e a licença.
    #[test]
    fn extrai_lib_e_roda() {
        let d = pasta_tmp("extrai");
        let tgz = d.join("onnxruntime-x.tgz");
        {
            let enc = flate2::write::GzEncoder::new(std::fs::File::create(&tgz).unwrap(), flate2::Compression::fast());
            let mut b = tar::Builder::new(enc);
            for (nome, dados) in [
                ("ort/include/c.h", &b"h"[..]),
                ("ort/lib/libonnxruntime.so.1.31.1", b"lib"),
                ("ort/lib/libonnxruntime_providers_cuda.so", b"cuda"),
                ("ort/lib/libonnxruntime_providers_tensorrt.so", b"trt"),
                ("ort/lib/cmake/onnxruntime/x.cmake", b"c"),
            ] {
                let mut h = tar::Header::new_gnu();
                h.set_size(dados.len() as u64);
                h.set_mode(0o644);
                h.set_cksum();
                b.append_data(&mut h, nome, dados).unwrap();
            }
            let mut h = tar::Header::new_gnu();
            h.set_entry_type(tar::EntryType::Symlink);
            h.set_size(0);
            h.set_cksum();
            b.append_link(&mut h, "ort/lib/libonnxruntime.so", "libonnxruntime.so.1.31.1").unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        let mut n = extrair_lib_onnxruntime(&tgz, &d.join("lib")).unwrap();
        n.sort();
        assert_eq!(n, ["libonnxruntime.so.1.31.1", "libonnxruntime_providers_cuda.so"]);
        assert_eq!(std::fs::read(d.join("lib/libonnxruntime.so.1.31.1")).unwrap(), b"lib");

        let whl = d.join("nvidia_cudnn_cu13-9.28.1.4-py3-none-x.whl");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&whl).unwrap());
            let o = zip::write::FileOptions::default();
            for (nome, dados) in [
                ("nvidia/cudnn/lib/libcudnn.so.9", &b"a"[..]),
                ("nvidia/cudnn/lib/libcudnn_ops.so.9", b"b"),
                ("nvidia/cudnn/include/cudnn.h", b"c"),
                ("nvidia/cublas/lib/libnvblas.so.13", b"d"),
                ("nvidia_cudnn_cu13-9.28.1.4.dist-info/License.txt", b"licenca"),
                ("nvidia/__init__.py", b""),
            ] {
                z.start_file(nome, o).unwrap();
                z.write_all(dados).unwrap();
            }
            z.finish().unwrap();
        }
        let mut n = extrair_roda(&whl, &d.join("cudnn")).unwrap();
        n.sort();
        assert_eq!(n, ["LICENSE-nvidia_cudnn_cu13.txt", "libcudnn.so.9", "libcudnn_ops.so.9"]);
        assert_eq!(std::fs::read(d.join("cudnn/LICENSE-nvidia_cudnn_cu13.txt")).unwrap(), b"licenca");
        let _ = std::fs::remove_dir_all(&d);
    }
}
