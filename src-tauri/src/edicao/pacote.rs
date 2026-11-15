//! Pacote do júri (§10.1): `Relatório.pdf` (a raiz), `mapa.csv`, `quadros.*.sha256`, `audio.flac`, `projeto/`,
//! `originais/` e `SHA256SUMS.txt` (de tudo menos o PDF), na pasta da exportação, depois do `tecnico.json` final.
//! Em dois tempos: `preparar` guarda no retrato, na hora do render, as peças que só o render tem (mapa, quadros
//! decodificados, som e miniaturas); `montar` sai com o relatório final, na publicação ou, com ocultação, quando o
//! perito conclui a conferência (até em outro dia, sem nada em memória). Dono: P1.8.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::edicao::conferir::DECODIFICADOS;
use crate::edicao::executar::{self, Ambiente, Preparado, Resultado};
use crate::edicao::ffmpeg::exec::{self, Comando};
use crate::edicao::fila::{self, Manifesto, COMPOSTOS};
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{ModoPdf, Pacote, ProjetoEdicao};
use crate::edicao::motor::plano::Plano;
use crate::edicao::motor::Alvo;
use crate::edicao::relatorio::juri::{self, ArquivoDoPacote, Miniatura};
use crate::edicao::relatorio::mapa;
use crate::error::{Result, SicroError};
use crate::hashing::sha256::sha256_file;
use crate::workspace::paths;

/// Nomes fixos do pacote (§10.1).
pub(crate) const PDF: &str = "Relatório.pdf";
pub(crate) const SOMAS: &str = "SHA256SUMS.txt";
pub(crate) const MAPA: &str = "mapa.csv";
pub(crate) const AUDIO: &str = "audio.flac";
pub(crate) const PROJETO: &str = "projeto";
pub(crate) const ORIGINAIS: &str = "originais";
/// Peças guardadas no retrato até o pacote sair: `<retrato>/pacote/` com `pecas.json` (nome, SHA-256 e tamanho).
pub(crate) const PECAS: &str = "pacote";
const PECAS_JSON: &str = "pecas.json";
/// Miniatura de cada item do roteiro (§10.3): quadro final do primeiro slot, com esta largura.
pub(crate) const LARGURA_MINIATURA: u32 = 480;
const QUALIDADE_JPEG: u8 = 85;
/// Teto de miniaturas num PDF (o roteiro inteiro continua listado; além disso o item sai sem imagem).
const MAX_MINIATURAS: usize = 400;

/// O que o pacote precisa. `pasta` = pasta da exportação; `base` = nome saneado dos arquivos; `tecnico` = o final;
/// `retrato` = `exportacoes/<saida_id>/` (projeto, manifesto, grafos, dados e as peças de `preparar`).
// CONTRATO: `retrato` e `pdf` no lugar de `plano`, `res`, `amb` e `projeto` da abertura: a conferência conclui sem o
// render em memória (contratos-fase0 (e) 15), e o pacote lê só do retrato, como o "refazer".
pub struct PedidoPacote<'a> {
    pub ws: &'a Path,
    pub pasta: &'a Path,
    pub base: &'a str,
    pub pacote: Pacote,
    pub pdf: ModoPdf,
    pub retrato: &'a Path,
    pub tecnico: &'a Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Peca {
    nome: String,
    sha256: String,
    tamanho: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
struct Pecas {
    esquema: u32,
    arquivos: Vec<Peca>,
}

fn erro(t: impl Into<String>) -> SicroError {
    SicroError::Validation(t.into())
}

fn cancelado(c: &AtomicBool) -> Result<()> {
    if c.load(Ordering::SeqCst) {
        return Err(erro(format!("{}: pacote", executar::ERRO_CANCELADO)));
    }
    Ok(())
}

/// Miniatura do slot `q` pelo motor real (`Alvo::Quadro`, 480 px, sem GPU), em JPEG.
pub(crate) fn miniatura(p: &ProjetoEdicao, prep: &Preparado, amb: &Ambiente, q: u64, cancelar: &AtomicBool) -> Result<Vec<u8>> {
    let plano = fila::compilar_alvo(p, prep, &Alvo::Quadro { q, largura: LARGURA_MINIATURA }, true)?;
    let amb = Ambiente { trabalho: amb.trabalho.join("miniaturas"), paralelos: 1, ..amb.clone() };
    let res = executar::executar(&plano, &amb, cancelar, &mut |_| {})?;
    jpeg_do_bmp(&res.arquivo)
}

fn jpeg_do_bmp(bmp: &Path) -> Result<Vec<u8>> {
    let img = image::open(bmp).map_err(|e| erro(format!("{}: miniatura: {e}", codigo::ITEM_INVALIDO)))?.to_rgb8();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, QUALIDADE_JPEG)
        .encode_image(&img)
        .map_err(|e| erro(format!("{}: miniatura: {e}", codigo::ITEM_INVALIDO)))?;
    Ok(jpeg)
}

/// Na hora do render (com "Pacote do júri"): guarda em `<retrato>/pacote/` o `mapa.csv` (dos pts lidos), a lista dos
/// quadros decodificados, o som em FLAC (o PCM conferível) e a miniatura de cada item do roteiro.
pub(crate) fn preparar(
    plano: &Plano,
    res: &Resultado,
    amb: &Ambiente,
    retrato: &Path,
    miniatura: Option<&dyn Fn(u64) -> Result<Vec<u8>>>,
    cancelar: &AtomicBool,
) -> Result<()> {
    let dir = retrato.join(PECAS);
    fs::create_dir_all(dir.join("img"))?;
    let mut nomes: Vec<String> = Vec::new();
    fs::write(dir.join(MAPA), mapa::mapa_csv(plano, Some(res))?)?;
    nomes.push(MAPA.into());
    if let Some(c) = res.conferencia.as_ref().filter(|c| c.decodificados.is_file()) {
        fs::copy(&c.decodificados, dir.join(DECODIFICADOS))?;
        nomes.push(DECODIFICADOS.into());
    }
    cancelado(cancelar)?;
    // o PCM do programa em FLAC: decodificado, dá o `audio_sha256` (o AAC do arquivo não repete entre versões)
    if let Some(a) = plano.audio.as_ref().filter(|a| !a.saida.contains(['/', '\\']) && amb.trabalho.join(&a.saida).is_file()) {
        let destino = dir.join(AUDIO);
        let args: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-nostats", "-loglevel", "error", "-i", &a.saida, "-c:a", "flac", "-compression_level", "8"]
            .iter()
            .map(|s| s.to_string())
            .chain(crate::edicao::ffmpeg::args::bitexact())
            .chain([destino.to_string_lossy().into_owned()])
            .collect();
        let cmd = Comando {
            nome: "flac".into(),
            programa: amb.cap.binario.clone(),
            args,
            pasta: amb.trabalho.clone(),
            ambiente: Vec::new(),
            sem_ambiente: Vec::new(),
            stderr: amb.trabalho.join("flac.log"),
        };
        let fim = exec::rodar(&cmd, cancelar)?;
        exec::exigir_sucesso(&cmd, &fim)?;
        nomes.push(AUDIO.into());
    }
    if let Some(f) = miniatura {
        for it in plano.proveniencia.roteiro.iter().take(MAX_MINIATURAS) {
            cancelado(cancelar)?;
            match f(it.na_saida.de_q) {
                Ok(jpeg) => {
                    let nome = format!("img/{}.jpg", it.n);
                    fs::write(dir.join(&nome), jpeg)?;
                    nomes.push(nome);
                }
                Err(e) if executar::codigo_de(&e) == Some(executar::ERRO_CANCELADO) => return Err(e),
                // o item sai sem imagem no PDF; o vídeo já está pronto
                Err(e) => tracing::warn!("edição: miniatura do item {}: {e}", it.n),
            }
        }
    }
    let mut pecas = Pecas { esquema: 1, arquivos: Vec::new() };
    for nome in nomes {
        let a = dir.join(&nome);
        pecas.arquivos.push(Peca { sha256: sha256_file(&a)?, tamanho: fs::metadata(&a)?.len(), nome });
    }
    fs::write(dir.join(PECAS_JSON), serde_json::to_vec_pretty(&pecas)?)?;
    Ok(())
}

/// As peças guardadas, conferidas pelo SHA-256 (peça trocada ou faltando = `retrato_divergente`).
fn ler_pecas(dir: &Path) -> Result<Pecas> {
    let divergente = |o: &str| erro(format!("{}: {o}", codigo::RETRATO_DIVERGENTE));
    let pecas: Pecas = fs::read(dir.join(PECAS_JSON)).ok().and_then(|b| serde_json::from_slice(&b).ok()).ok_or_else(|| divergente(PECAS_JSON))?;
    for p in &pecas.arquivos {
        let simples = !p.nome.is_empty() && !p.nome.contains('\\') && p.nome.split('/').all(|c| !c.is_empty() && c != "." && c != "..");
        let a = dir.join(&p.nome);
        if !simples || !a.is_file() || fs::metadata(&a)?.len() != p.tamanho || sha256_file(&a)? != p.sha256 {
            return Err(divergente(&p.nome));
        }
    }
    Ok(pecas)
}

/// Copia sem sobrescrever, por arquivo parcial (nunca fica truncado com o nome final). Cópia de verdade, não
/// ligação: o pacote fica somente leitura, e a permissão é do arquivo, não do nome.
fn copiar(de: &Path, para: &Path) -> Result<()> {
    if let Some(pai) = para.parent() {
        fs::create_dir_all(pai)?;
    }
    fila::copiar_inteiro(&mut fs::File::open(de)?, para)?;
    Ok(())
}

/// Maior nome de original em `originais/`, em bytes: sobra para ` (999)` e o `.parcial` da cópia dentro dos 255 do
/// sistema de arquivos.
const NOME_MAX: usize = 200;

/// Nome de arquivo sem separador, controle nem os reservados do Windows; nunca vazio; no máximo `NOME_MAX` bytes
/// (corta o tronco numa letra inteira e mantém a extensão).
fn nome_seguro(n: &str) -> String {
    let s: String = n.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c }).collect();
    let s = s.trim().trim_end_matches(['.', ' ']).trim_start_matches('.');
    if s.is_empty() {
        return "arquivo".into();
    }
    if s.len() <= NOME_MAX {
        return s.to_string();
    }
    let (tronco, ext) = match s.rsplit_once('.') {
        Some((t, e)) if !t.is_empty() && e.len() <= 16 => (t, format!(".{e}")),
        _ => (s, String::new()),
    };
    let mut fim = NOME_MAX - ext.len();
    while !tronco.is_char_boundary(fim) {
        fim -= 1;
    }
    format!("{}{ext}", tronco[..fim].trim_end_matches(['.', ' ']))
}

/// Arquivos da pasta (em qualquer profundidade), relativos com `/`, em ordem; sem os ocultos (marca de publicação,
/// temporários).
fn listar(pasta: &Path) -> Result<Vec<String>> {
    fn andar(base: &Path, p: &Path, v: &mut Vec<String>) -> Result<()> {
        for e in fs::read_dir(p)? {
            let e = e?;
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if e.file_type()?.is_dir() {
                andar(base, &e.path(), v)?;
            } else {
                v.push(e.path().strip_prefix(base).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default());
            }
        }
        Ok(())
    }
    let mut v = Vec::new();
    andar(pasta, pasta, &mut v)?;
    v.sort();
    Ok(v)
}

/// Gravações do caso usadas no vídeo, copiadas para `originais/` e conferidas pelo SHA-256 do retrato.
fn copiar_originais(p: &PedidoPacote, projeto: &Value) -> Result<()> {
    let usadas: Vec<&str> = p.tecnico.pointer("/feito/midias").and_then(Value::as_array).into_iter().flatten().filter_map(|m| m.get("sha256").and_then(Value::as_str)).collect();
    let mut banco = None;
    let mut postos: Vec<String> = Vec::new();
    for m in projeto.get("midias").and_then(Value::as_array).into_iter().flatten() {
        let sha = m.get("sha256").and_then(Value::as_str).unwrap_or("");
        if m.get("origem").and_then(Value::as_str) != Some("caso") || !usadas.contains(&sha) || postos.iter().any(|s| s == sha) {
            continue;
        }
        let ausente = || erro(format!("{}: {sha}", codigo::MIDIA_AUSENTE));
        let rel = m.get("relativo").and_then(Value::as_str).filter(|r| executar::relativo_seguro(r));
        let mut origem = rel.map(|r| paths::resolver_relativo(p.ws, r)).filter(|a| a.is_file());
        if origem.is_none() {
            if banco.is_none() {
                banco = Some(executar::abrir_banco(p.ws)?);
            }
            let (conn, occ) = banco.as_ref().ok_or_else(ausente)?;
            let v = crate::database::repositories::video_repo::find_media_by_sha256(conn, occ, sha)?.ok_or_else(ausente)?;
            origem = Some(paths::resolver_relativo(p.ws, &v.relative_path)).filter(|a| a.is_file());
        }
        let origem = origem.ok_or_else(ausente)?;
        let nome = nome_seguro(&origem.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        let dir = p.pasta.join(ORIGINAIS);
        let (tronco, ext) = match nome.rsplit_once('.') {
            Some((t, e)) if !t.is_empty() => (t.to_string(), format!(".{e}")),
            _ => (nome.clone(), String::new()),
        };
        let destino = (1..=999u32)
            .map(|n| dir.join(if n == 1 { nome.clone() } else { format!("{tronco} ({n}){ext}") }))
            .find(|d| !d.exists())
            .ok_or_else(|| erro(format!("pasta_existe: {nome}")))?;
        copiar(&origem, &destino)?;
        if sha256_file(&destino)? != sha {
            return Err(erro(format!("{}: {sha}", codigo::MIDIA_TROCADA)));
        }
        postos.push(sha.to_string());
    }
    Ok(())
}

/// O que uma tentativa anterior do pacote deixou (a saída ainda não foi concluída): sai antes de montar de novo.
fn limpar_anterior(pasta: &Path) -> Result<()> {
    for nome in [PDF, SOMAS, MAPA, COMPOSTOS, DECODIFICADOS, AUDIO] {
        // o `.parcial` é de cópia que o processo morto deixou no meio: barraria a cópia seguinte
        for a in [pasta.join(nome), pasta.join(format!("{nome}.parcial"))] {
            if a.is_file() {
                fila::gravavel(&a);
                fs::remove_file(&a)?;
            }
        }
    }
    for nome in [PROJETO, ORIGINAIS] {
        let d = pasta.join(nome);
        if d.is_dir() {
            fila::gravavel(&d);
            fs::remove_dir_all(&d)?;
        }
    }
    Ok(())
}

/// Monta o pacote; devolve os arquivos criados (relativos à pasta, com `/`). Somente leitura no fim.
/// As peças ficam no retrato: quem chama as tira (`descartar_pecas`) depois de registrar a saída no banco, para a
/// tentativa seguinte ainda ter com o que montar se o registro falhar.
pub fn montar(p: &PedidoPacote, cancelar: &AtomicBool) -> Result<Vec<String>> {
    let pecas_dir = p.retrato.join(PECAS);
    let pecas = ler_pecas(&pecas_dir)?;
    conferir_retrato(p)?;
    let antes = listar(p.pasta)?;
    limpar_anterior(p.pasta)?;
    let antes: Vec<String> = antes.into_iter().filter(|a| p.pasta.join(a).is_file()).collect();
    let r = montar_em(p, cancelar, &pecas_dir, &pecas);
    if r.is_err() {
        // pacote pela metade não fica na pasta da saída
        let _ = limpar_anterior(p.pasta);
    }
    Ok(r?.into_iter().filter(|a| !antes.contains(a)).collect())
}

/// O retrato de onde o pacote sai é o que a exportação gravou? (a conferência pode vir dias depois do render)
/// Projeto, grafos, dados e a lista do composto conferidos pelo manifesto; divergência = `retrato_divergente`.
fn conferir_retrato(p: &PedidoPacote) -> Result<()> {
    let divergente = |o: &str| erro(format!("{}: {o}", codigo::RETRATO_DIVERGENTE));
    let m: Manifesto = fs::read(p.retrato.join("manifesto.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).ok_or_else(|| divergente("manifesto.json"))?;
    let no_lugar = fila::pasta_retrato(p.ws, &m.projeto_id, &m.saida_id).ok().and_then(|d| fs::canonicalize(d).ok());
    if no_lugar.is_none() || no_lugar != fs::canonicalize(p.retrato).ok() {
        return Err(divergente("manifesto.json"));
    }
    fila::ler_retrato(p.ws, &m.projeto_id, &m.saida_id).map(drop).map_err(|e| match &e {
        SicroError::Validation(t) => divergente(t.split_once(": ").map_or(t.as_str(), |(_, d)| d)),
        outro => divergente(&outro.to_string()),
    })
}

/// Tira da pasta da saída o pacote de uma tentativa que não concluiu (a saída segue "a conferir").
pub(crate) fn desfazer(pasta: &Path) {
    let _ = limpar_anterior(pasta);
}

/// Tira do retrato as peças do pacote (depois de a saída estar registrada como concluída).
pub(crate) fn descartar_pecas(retrato: &Path) {
    let _ = fs::remove_dir_all(retrato.join(PECAS));
}

/// Grava inteiro ou não grava (parcial + troca de nome), sem sobrescrever.
fn gravar_inteiro(para: &Path, bytes: &[u8]) -> Result<()> {
    fila::copiar_inteiro(&mut &bytes[..], para)?;
    Ok(())
}

fn montar_em(p: &PedidoPacote, cancelar: &AtomicBool, pecas_dir: &Path, pecas: &Pecas) -> Result<Vec<String>> {
    // peças do render e a lista por slot do composto
    for nome in [MAPA, DECODIFICADOS, AUDIO] {
        if pecas.arquivos.iter().any(|x| x.nome == nome) {
            copiar(&pecas_dir.join(nome), &p.pasta.join(nome))?;
        }
    }
    if p.retrato.join(COMPOSTOS).is_file() {
        copiar(&p.retrato.join(COMPOSTOS), &p.pasta.join(COMPOSTOS))?;
    }
    cancelado(cancelar)?;
    // projeto/: o `projeto.json` e só o que ele cita, mais os grafos e listas de cada passo
    let projeto_bytes = fs::read(p.retrato.join("projeto.json"))?;
    let projeto: Value = serde_json::from_slice(&projeto_bytes)?;
    let dir_projeto = p.pasta.join(PROJETO);
    copiar(&p.retrato.join("projeto.json"), &dir_projeto.join("projeto.json"))?;
    let m: Manifesto = serde_json::from_slice(&fs::read(p.retrato.join("manifesto.json"))?)?;
    for it in m.itens.iter().filter(|i| !i.de.is_empty()) {
        // nome oculto ficaria fora das somas (`listar` pula o que começa com ponto)
        let oculto = it.de.split(['/', '\\']).any(|c| c.starts_with('.'));
        if oculto || !executar::relativo_seguro(&it.de) || !executar::relativo_seguro(&it.arquivo) {
            return Err(erro(format!("{}: {}", codigo::RETRATO_DIVERGENTE, it.arquivo)));
        }
        let destino = dir_projeto.join(&it.de);
        if !destino.exists() {
            copiar(&p.retrato.join(&it.arquivo), &destino)?;
        }
    }
    for rel in fila::arquivos_em(p.retrato, "grafos")? {
        copiar(&p.retrato.join(&rel), &dir_projeto.join(&rel))?;
    }
    cancelado(cancelar)?;
    if p.pacote.originais {
        copiar_originais(p, &projeto)?;
    }
    cancelado(cancelar)?;
    // SHA256SUMS.txt de tudo menos o PDF; o PDF traz o SHA-256 de todos, inclusive o das somas
    let lista: Vec<String> = listar(p.pasta)?.into_iter().filter(|a| a != PDF && a != SOMAS).collect();
    let somas = sha256sums(p.pasta, &lista)?;
    gravar_inteiro(&p.pasta.join(SOMAS), somas.as_bytes())?;
    let mut arquivos: Vec<ArquivoDoPacote> = somas
        .lines()
        .filter_map(|l| l.split_once("  "))
        .map(|(sha, rel)| ArquivoDoPacote { rel: rel.to_string(), sha256: sha.to_string() })
        .collect();
    arquivos.push(ArquivoDoPacote { rel: SOMAS.into(), sha256: executar::sha256_bytes(somas.as_bytes()) });
    cancelado(cancelar)?;
    let mut miniaturas: Vec<Miniatura> = Vec::new();
    for x in &pecas.arquivos {
        let n = x.nome.strip_prefix("img/").and_then(|r| r.strip_suffix(".jpg")).and_then(|n| n.parse::<u32>().ok());
        if let Some(n) = n {
            miniaturas.push(Miniatura { n, jpeg: fs::read(pecas_dir.join(&x.nome))? });
        }
    }
    let gerar = |mini: &[Miniatura]| -> Result<crate::relatorio_pdf::Saida> {
        let dados = juri::dados(p.tecnico, mini, &arquivos)?;
        crate::relatorio_pdf::gerar(&juri::entrada(p.tecnico, dados, mini, &projeto_bytes, p.pdf)?)
    };
    let pdf = match gerar(&miniaturas) {
        // miniatura que o PDF não lê: o relatório sai sem as imagens, nunca deixa de sair
        Err(e) if e.to_string().contains("imagem inválida") => {
            tracing::warn!("edição: {e}");
            gerar(&[])?
        }
        r => r?,
    };
    for a in &pdf.avisos {
        tracing::warn!("edição: relatório em PDF: {a}");
    }
    gravar_inteiro(&p.pasta.join(PDF), &pdf.pdf)?;
    let todos = listar(p.pasta)?;
    for a in &todos {
        fila::somente_leitura(&p.pasta.join(a))?;
    }
    let _ = p.base;
    Ok(todos)
}

/// Texto do `SHA256SUMS.txt` (formato do `sha256sum`), em ordem de caminho, sem o PDF.
pub fn sha256sums(pasta: &Path, arquivos: &[String]) -> Result<String> {
    let mut v: Vec<&String> = arquivos.iter().filter(|a| a.as_str() != PDF && a.as_str() != SOMAS).collect();
    v.sort();
    v.dedup();
    let mut t = String::new();
    for a in v {
        // o formato do `sha256sum` não leva quebra de linha nem barra invertida no nome sem escape
        if a.contains(['\n', '\r', '\\']) || !executar::relativo_seguro(a) {
            return Err(erro(format!("{}: {a}", codigo::ITEM_INVALIDO)));
        }
        t.push_str(&format!("{}  {a}\n", sha256_file(&pasta.join(a))?));
    }
    Ok(t)
}

/// Caso temporário de verdade (banco, mídia registrada) e o caminho dos comandos, para os aceites de ponta a ponta do
/// pacote e do "refazer".
#[cfg(test)]
pub(crate) mod ponta {
    use std::path::PathBuf;
    use std::sync::Arc;

    use serde_json::{json, Value};

    use super::*;
    use crate::commands::edicao_commands as cmd;
    use crate::edicao::ffmpeg::sonda::Capacidades;
    use crate::edicao::fila::{Estado, SaidaRegistrada};
    use crate::edicao::modelo::Nivel;
    use crate::edicao::quadros::TabelaQuadros;
    use crate::edicao::testes::aceite;
    use crate::edicao::testes::fontes;

    pub(crate) const PERITO: &str = "Perita do Júri";

    pub(crate) struct Caso {
        pub ws: PathBuf,
        pub app: PathBuf,
        pub cap: Arc<Capacidades>,
    }

    /// None = sem ffmpeg (com `SICRO_EXIGE_FFMPEG=1`, pânico em `fontes::cap`).
    pub(crate) fn caso(nome: &str) -> Option<Caso> {
        fontes::cap()?;
        let cap = crate::edicao::ffmpeg::sonda::capacidades().expect("sonda");
        let d = fontes::pasta_temporaria(&format!("p18-{nome}"));
        let pai = d.join("Casos ç");
        fs::create_dir_all(&pai).unwrap();
        let criado = crate::workspace::create::create_workspace(
            crate::models::NewOccurrenceInput {
                titulo: Some("Pacote".into()),
                numero_bo: None,
                protocolo: Some("18/26".into()),
                oficio: None,
                tipo_pericia: None,
                municipio: None,
                peritos: vec![PERITO.into()],
                parent_directory: Some(pai.to_string_lossy().into()),
            },
            &pai,
        )
        .unwrap();
        Some(Caso { ws: criado.path, app: d.join("cache do app"), cap })
    }

    impl Caso {
        pub fn cache(&self) -> PathBuf {
            cmd::cache_do_caso(&self.app, &self.ws).unwrap()
        }

        /// Registra o vídeo no caso como o app faz; devolve o `MidiaRef` e a tabela do decodificador da cópia.
        pub fn midia(&self, id: &str, arquivo: &Path) -> (Value, TabelaQuadros) {
            let vm = tauri::async_runtime::block_on(crate::commands::video_commands::register_video_media(
                self.ws.to_string_lossy().into(),
                crate::models::RegisterVideoInput { source_path: arquivo.to_string_lossy().into(), allow_duplicate: false },
            ))
            .expect("registrar vídeo");
            let caminho = paths::resolver_relativo(&self.ws, &vm.relative_path);
            let tab = TabelaQuadros::do_decodificador(&self.cap, &caminho, &AtomicBool::new(false), &mut |_| {}).expect("tabela");
            let m = json!({ "id": id, "tipo": "video", "origem": "caso", "media_id": vm.id.to_string(), "sha256": vm.sha256,
                "nome": vm.filename, "relativo": vm.relative_path, "duracao_s": vm.duration_s, "inicio_s": null,
                "largura": vm.width, "altura": vm.height, "taxa": null, "vfr": false, "tem_audio": true, "cor": null,
                "derivacao": null, "recebido": { "nativo": null, "obs": "" }, "relogio_desvio_s": null, "gravador": null,
                "decodificacao": null, "ocultacoes": [] });
            (m, tab)
        }

        /// Analisa, dá ciência dos avisos periciais e exporta pela fila; devolve o estado final e as ciências dadas.
        pub fn exportar(&self, doc: &Value, exportacao: Value) -> (Estado, Vec<String>) {
            let analise = cmd::analisar(&self.ws, doc, &self.app).expect("edicao_analisar");
            let mut ciencias: Vec<String> = analise.problemas.iter().filter(|p| p.nivel == Nivel::Pericial).map(|p| p.codigo.to_string()).collect();
            ciencias.dedup();
            for vez in 0..2 {
                match self.enfileirar(doc, exportacao.clone(), &ciencias) {
                    // aviso que só aparece com a tabela do decodificador (fonte danificada): ciência e de novo
                    Estado::Falhou { mensagem, .. } if vez == 0 && mensagem.starts_with("ciencia: ") => {
                        ciencias.extend(mensagem.rsplit(": ").next().unwrap_or_default().split(", ").map(String::from));
                    }
                    fim => return (fim, ciencias),
                }
            }
            unreachable!()
        }

        pub fn enfileirar(&self, doc: &Value, exportacao: Value, ciencias: &[String]) -> Estado {
            let alvo: Alvo = serde_json::from_value(json!({ "tipo": "final", "exportacao": exportacao })).unwrap();
            let t = fila::enfileirar(cmd::pedido(&self.ws, doc.clone(), alvo, ciencias.to_vec(), &self.app).unwrap()).expect("edicao_enfileirar");
            fila::esperar(&t).expect("esperar")
        }

        pub fn saida(&self, id: &str) -> SaidaRegistrada {
            fila::saidas(&self.ws, None).unwrap().into_iter().find(|s| s.id == id).unwrap_or_else(|| panic!("saída {id}"))
        }

        pub fn pasta(&self, s: &SaidaRegistrada) -> PathBuf {
            paths::resolver_relativo(&self.ws, &s.pasta)
        }

        pub fn retrato(&self, s: &SaidaRegistrada) -> PathBuf {
            paths::resolver_relativo(&self.ws, &s.retrato_rel)
        }

        pub fn contar(&self, sql: &str) -> i64 {
            let (c, _) = executar::abrir_banco(&self.ws).unwrap();
            c.query_row(sql, [], |r| r.get(0)).unwrap()
        }
    }

    pub(crate) fn exportacao(predefinicao: &str, codificador: &str, pacote: Value, pdf: &str) -> Value {
        json!({ "predefinicao": predefinicao, "codificador": codificador, "legendas": "nenhuma", "pacote": pacote, "pdf": pdf, "revisado_por": "", "personalizada": null })
    }

    /// Documento como a tela manda: id novo, `extra` por cima (rastreios, efeitos).
    pub(crate) fn projeto(titulo: &str, audio: bool, midias: Vec<Value>, principal: Vec<Value>, extra: Value) -> Value {
        let mut v = json!({
            "esquema": 2, "esquema_menor": 0, "id": uuid::Uuid::new_v4().to_string(), "titulo": titulo, "created_at": "2026-11-15T00:00:00Z", "updated_at": "2026-11-15T00:00:00Z",
            "saida": { "largura": 640, "altura": 360, "fps": { "num": 30, "den": 1 }, "auto": false, "cor_fundo": "#202020", "audio": audio,
                       "selos": { "velocidade": true, "ampliacao": true, "realce": true }, "exportacao": aceite::exportacao("cpu") },
            "midias": midias, "principal": principal
        });
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            for (k, x) in e {
                o.insert(k.clone(), x.clone());
            }
        }
        v
    }

    pub(crate) fn id_da(e: &Estado) -> String {
        match e {
            Estado::Concluida { saida_id } | Estado::AConferir { saida_id } => saida_id.clone(),
            outro => panic!("exportação não terminou: {outro:?}"),
        }
    }

    /// Texto do PDF sem espaços nem quebras (hash e frase podem quebrar de linha); None sem o `pdftotext`.
    pub(crate) fn texto_do_pdf(pdf: &Path) -> Option<String> {
        let o = std::process::Command::new("pdftotext").arg(pdf).arg("-").output().ok()?;
        o.status.success().then(|| String::from_utf8_lossy(&o.stdout).split_whitespace().collect())
    }

    /// Confere o `SHA256SUMS.txt` da pasta sem ferramenta de fora: cada linha, e a lista contra o que há na pasta.
    pub(crate) fn conferir_somas(pasta: &Path) -> Vec<String> {
        let texto = fs::read_to_string(pasta.join(SOMAS)).expect(SOMAS);
        let mut listados = Vec::new();
        for l in texto.lines() {
            let (sha, rel) = l.split_once("  ").unwrap_or_else(|| panic!("linha: {l}"));
            assert_eq!(sha256_file(&pasta.join(rel)).unwrap(), sha, "{rel}");
            listados.push(rel.to_string());
        }
        let na_pasta: Vec<String> = listar(pasta).unwrap().into_iter().filter(|a| a != PDF && a != SOMAS).collect();
        assert_eq!(listados, na_pasta, "SHA256SUMS.txt lista tudo menos o PDF e ele mesmo");
        listados
    }
}

#[cfg(test)]
mod testes {
    use std::path::PathBuf;
    use std::sync::OnceLock;

    use serde_json::{json, Value};

    use super::ponta::{self, Caso, PERITO};
    use super::*;
    use crate::commands::edicao_commands as cmd;
    use crate::edicao::conferir;
    use crate::edicao::fila::{Estado, SaidaRegistrada};
    use crate::edicao::testes::aceite::{ancora, ocultar, rastreio, trecho};
    use crate::edicao::testes::fontes::{self, Fonte};

    struct Pronto {
        caso: Caso,
        s: SaidaRegistrada,
        pasta: PathBuf,
        sha_midia: String,
    }

    /// Uma exportação com "Pacote do júri" e "Incluir originais", feita uma vez para os dois aceites do pacote.
    fn pronto() -> Option<&'static Pronto> {
        static P: OnceLock<Option<Pronto>> = OnceLock::new();
        P.get_or_init(|| {
            let caso = ponta::caso("pacote")?;
            let (m, tab) = caso.midia("m1", &fontes::gerar(Fonte::Mp4).unwrap());
            let sha_midia = m["sha256"].as_str().unwrap().to_string();
            // com uma legenda em arquivo: o `.srt` e o `.vtt` saem ao lado do vídeo e entram nas somas
            let mut legendas = serde_json::to_value(crate::edicao::modelo::TrilhaLegendas::default()).unwrap();
            legendas["itens"] = json!([{ "id": "l1", "texto": "Parado!", "locutor": null, "origem": "perito", "conferida": true, "locutor_auto": false, "ref": { "tipo": "programa", "de": 0.2, "ate": 0.9 } }]);
            let doc = ponta::projeto("Pacote: júri ç", true, vec![m], vec![trecho("c1", "m1", &tab, 30, 60, 1.0, json!([])), trecho("c2", "m1", &tab, 90, 105, 1.0, json!([]))], json!({ "legendas": legendas }));
            let mut exp = ponta::exportacao("juri", "cpu", json!({ "originais": true }), "a3");
            exp["legendas"] = json!("arquivo");
            let (fim, _) = caso.exportar(&doc, exp);
            assert!(matches!(fim, Estado::Concluida { .. }), "{fim:?}");
            let s = caso.saida(&ponta::id_da(&fim));
            let pasta = caso.pasta(&s);
            Some(Pronto { caso, s, pasta, sha_midia })
        })
        .as_ref()
    }

    /// Aceite: o pacote sai inteiro, o `SHA256SUMS.txt` confere pelo `sha256sum` de fora e lista tudo menos o PDF; o PDF
    /// é a raiz (traz o SHA-256 das somas e do vídeo); tudo somente leitura; adulterar um arquivo é acusado.
    #[test]
    fn sha256sums_confere_pdf_fora() {
        let Some(p) = pronto() else { return };
        let nome = "Pacote_ júri ç";
        let listados = ponta::conferir_somas(&p.pasta);
        for esperado in [format!("{nome}.mp4"), format!("{nome}.tecnico.json"), format!("{nome}.srt"), format!("{nome}.vtt"), MAPA.into(), COMPOSTOS.into(), DECODIFICADOS.into(), AUDIO.into(), "projeto/projeto.json".into(), "projeto/grafos/argumentos.json".into(), "projeto/grafos/b000/b000.txt".into()] {
            assert!(listados.contains(&esperado), "falta {esperado} em {listados:?}");
        }
        assert!(!listados.iter().any(|a| a == PDF || a == SOMAS || a.starts_with('.')), "{listados:?}");
        assert!(fs::read_to_string(p.pasta.join(format!("{nome}.srt"))).unwrap().contains("Parado!"));
        // a gravação usada, com o SHA-256 que o projeto cita
        let originais: Vec<&String> = listados.iter().filter(|a| a.starts_with("originais/")).collect();
        assert_eq!(originais.len(), 1, "{listados:?}");
        assert_eq!(sha256_file(&p.pasta.join(originais[0])).unwrap(), p.sha_midia);
        // somente leitura, e nada das peças fica no retrato
        for a in listados.iter().map(String::as_str).chain([PDF, SOMAS]) {
            assert!(fs::metadata(p.pasta.join(a)).unwrap().permissions().readonly(), "{a} gravável");
        }
        assert!(!p.caso.retrato(&p.s).join(PECAS).exists());
        assert!(!p.pasta.join(fila::PUBLICANDO).exists(), "a marca de publicação sai");
        // o PDF fora das somas e raiz delas
        let pdf = fs::read(p.pasta.join(PDF)).unwrap();
        assert!(pdf.starts_with(b"%PDF-") && pdf.len() > 20_000, "{} bytes", pdf.len());
        match ponta::texto_do_pdf(&p.pasta.join(PDF)) {
            Some(t) => {
                assert!(t.contains(&sha256_file(&p.pasta.join(SOMAS)).unwrap()), "o PDF traz o SHA-256 do {SOMAS}");
                assert!(t.contains(&p.s.sha256), "o PDF traz o SHA-256 do vídeo");
            }
            None => eprintln!("aviso: sem pdftotext, o texto do PDF não foi conferido"),
        }
        // o conferidor de fora
        #[cfg(unix)]
        {
            let confere = |pasta: &Path| std::process::Command::new("sha256sum").args(["-c", "--quiet", SOMAS]).current_dir(pasta).output().map(|o| o.status.success());
            match confere(&p.pasta) {
                Ok(ok) => {
                    assert!(ok, "sha256sum -c {SOMAS}");
                    // cópia com um byte trocado no mapa: o mesmo conferidor acusa
                    let copia = fontes::pasta_temporaria("p18-pacote-adulterado");
                    for a in listados.iter().map(String::as_str).chain([SOMAS]) {
                        fs::create_dir_all(copia.join(a).parent().unwrap()).unwrap();
                        fs::copy(p.pasta.join(a), copia.join(a)).unwrap();
                    }
                    let mapa = copia.join(MAPA);
                    fila::gravavel(&mapa);
                    let mut b = fs::read(&mapa).unwrap();
                    *b.last_mut().unwrap() ^= 1;
                    fs::write(&mapa, b).unwrap();
                    assert_eq!(confere(&copia).ok(), Some(false), "adulteração não acusada");
                }
                Err(e) => eprintln!("aviso: sem sha256sum ({e}); conferido só por dentro"),
            }
        }
        // a saída registrada aponta para o relatório técnico da pasta
        assert_eq!(p.s.tecnico_rel.as_deref(), Some(format!("{}/{nome}.tecnico.json", p.s.pasta).as_str()));
    }

    /// Aceite: o comando que o PDF ensina (`ffmpeg -i <arquivo> -map 0:v:0 -f framehash -hash sha256 -`), rodado por
    /// fora sobre o vídeo entregue, dá a lista `quadros.decodificados.sha256`; e o `audio.flac` decodificado dá o
    /// `audio_sha256` registrado (o PCM conferível).
    #[test]
    fn framehash_por_fora() {
        let Some(p) = pronto() else { return };
        let ffmpeg = |args: &[&str]| {
            let o = crate::tools::command(&p.caso.cap.binario).args(["-hide_banner", "-nostdin", "-loglevel", "error"]).args(args).output().expect("ffmpeg");
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).into_owned()
        };
        let video = paths::resolver_relativo(&p.caso.ws, &p.s.arquivo);
        let por_fora = conferir::coluna_de_hash(&ffmpeg(&["-i", &video.to_string_lossy(), "-map", "0:v:0", "-f", "framehash", "-hash", "sha256", "-"]));
        let lista: Vec<String> = fs::read_to_string(p.pasta.join(DECODIFICADOS)).unwrap().lines().map(String::from).collect();
        assert_eq!(lista.len() as u64, p.s.n_quadros);
        assert_eq!(p.s.n_quadros, 45);
        assert_eq!(por_fora, lista, "{}", p.caso.cap.versao);
        assert_eq!(Some(conferir::hash_de_conteudo(&lista)), p.s.decodificado_sha256);
        // e a lista do composto é a do retrato, com um hash por slot
        assert_eq!(fs::read(p.pasta.join(COMPOSTOS)).unwrap(), fs::read(p.caso.retrato(&p.s).join(COMPOSTOS)).unwrap());
        assert_eq!(fs::read_to_string(p.pasta.join(COMPOSTOS)).unwrap().lines().count(), 45);
        let pcm = ffmpeg(&["-i", &p.pasta.join(AUDIO).to_string_lossy(), "-c:a", "pcm_s16le", "-f", "hash", "-hash", "sha256", "-"]);
        assert_eq!(pcm.trim().strip_prefix("SHA256="), p.s.audio_sha256.as_deref(), "{}", p.caso.cap.versao);
        // mapa: um slot por linha, com o quadro de origem
        let mapa = fs::read_to_string(p.pasta.join(MAPA)).unwrap();
        assert!(mapa.starts_with("q,item,clipe,midia,pts,indice,t_s,tipo\n") && mapa.lines().count() >= 46, "{mapa}");
    }

    fn com_ocultar(caso: &Caso, titulo: &str) -> Value {
        let (mut m, tab) = caso.midia("m1", &fontes::gerar(Fonte::Mp4).unwrap());
        // mosaico em placa: aviso pericial `ocultacao_reversivel`, que pede ciência
        m["ocultacoes"] = json!([ocultar("o1", "r1", "mosaico", json!({ "conteudo": "placa", "motivo": "placa de terceiro" }))]);
        let r = rastreio("r1", Some("m1"), "fonte", vec![ancora(tab.tempo(30), Some(tab.pts[30]), [0.4, 0.4, 0.2, 0.2])]);
        ponta::projeto(titulo, true, vec![m], vec![trecho("c1", "m1", &tab, 30, 60, 1.0, json!([]))], json!({ "rastreios": [r] }))
    }

    /// Aceites: exportação com `ocultar` para em "A conferir" sem relatório nem pacote; a conferência conclui e o PDF diz
    /// se foi conferida ou não; a ciência do aviso pericial fica gravada (quem, quando, o quê) e sem ela não exporta.
    #[test]
    fn ocultar_a_conferir_pdf_diz_e_ciencia_gravada() {
        let Some(caso) = ponta::caso("conferir") else { return };
        let doc = com_ocultar(&caso, "Ocultado");
        let exp = ponta::exportacao("juri", "cpu", json!({ "originais": false }), "a2_sem_anexos");
        // sem ciência não exporta, e nada fica na pasta das exportações
        match caso.enfileirar(&doc, exp.clone(), &[]) {
            Estado::Falhou { mensagem, .. } => assert!(mensagem.starts_with("ciencia: ") && mensagem.contains("ocultacao_reversivel"), "{mensagem}"),
            outro => panic!("exportou sem ciência: {outro:?}"),
        }
        assert_eq!(caso.contar("SELECT COUNT(*) FROM video_edit_outputs"), 0);
        for (conferida, frase, nao_frase) in [(true, "foramconferidaspeloperito", "nãoforamconferidas"), (false, "nãoforamconferidaspeloperito", "Asocultaçõesforamconferidas")] {
            let (fim, ciencias) = caso.exportar(&doc, exp.clone());
            assert_eq!(ciencias, ["ocultacao_reversivel"]);
            let Estado::AConferir { saida_id } = fim else { panic!("não parou em A conferir: {fim:?}") };
            let s = caso.saida(&saida_id);
            let (pasta, retrato) = (caso.pasta(&s), caso.retrato(&s));
            assert!(s.exige_conferencia && s.tecnico_rel.is_none());
            assert!(pasta.join("Ocultado (ocultado).mp4").is_file());
            for a in [PDF, SOMAS, MAPA, "Ocultado.tecnico.json"] {
                assert!(!pasta.join(a).exists(), "{a} antes da conferência");
            }
            assert!(retrato.join(fila::PENDENTE).is_file() && retrato.join(PECAS).join("pecas.json").is_file() && retrato.join(PECAS).join(MAPA).is_file());
            // a miniatura do item 1: quadro final do primeiro slot pelo motor, com 480 px de largura
            let mini = image::load_from_memory(&fs::read(retrato.join(PECAS).join("img/1.jpg")).expect("miniatura do item 1")).expect("JPEG");
            assert_eq!((mini.width(), mini.height()), (LARGURA_MINIATURA, 270));
            let feita = cmd::conferir(&caso.ws, &saida_id, conferida).expect("edicao_conferir");
            assert_eq!((feita.conferida_por.as_deref(), feita.tecnico_rel.is_some()), (conferida.then_some(PERITO), true));
            // o pacote saiu com o relatório final, e as peças saíram do retrato
            ponta::conferir_somas(&pasta);
            assert!(!retrato.join(PECAS).exists() && !retrato.join(fila::PENDENTE).exists());
            let tecnico: Value = serde_json::from_slice(&fs::read(pasta.join("Ocultado.tecnico.json")).unwrap()).unwrap();
            assert_eq!(tecnico["execucao"]["conferencia"]["ocultacoes"], json!(if conferida { "conferida" } else { "nao_conferida" }));
            // o PDF diz
            let dados = juri::dados(&tecnico, &[], &[]).unwrap().to_string().split_whitespace().collect::<String>();
            assert!(dados.contains(frase) && !dados.contains(nao_frase), "dados do PDF sem a frase da conferência");
            match ponta::texto_do_pdf(&pasta.join(PDF)) {
                Some(t) => assert!(t.contains(frase) && !t.contains(nao_frase), "o PDF não diz {frase}"),
                None => eprintln!("aviso: sem pdftotext, o texto do PDF não foi conferido"),
            }
            // PDF sem anexos: o projeto não vai dentro dele
            assert!(!crate::relatorio_pdf::testes::contem(&fs::read(pasta.join(PDF)).unwrap(), b"projeto.json"));
            // ciência gravada: o aviso no `feito` e quem, quando e o quê no `execucao`
            let c = &tecnico["execucao"]["ciencias"];
            assert_eq!(c.as_array().map(Vec::len), Some(1), "{c}");
            assert_eq!((c[0]["codigo"].as_str(), c[0]["por"].as_str()), (Some("ocultacao_reversivel"), Some(PERITO)));
            assert!(chrono::DateTime::parse_from_rfc3339(c[0]["ciente_em"].as_str().unwrap_or("")).is_ok(), "{c}");
            let avisos = tecnico["feito"]["avisos"].as_array().cloned().unwrap_or_default();
            assert!(avisos.iter().any(|a| a["codigo"] == "ocultacao_reversivel" && a["ciente"] == true), "{avisos:?}");
        }
    }

    /// Peça do pacote trocada no retrato entre o render e a conferência: o pacote não sai e a saída segue a conferir.
    #[test]
    fn peca_trocada_no_retrato_barra_o_pacote() {
        let Some(caso) = ponta::caso("peca") else { return };
        let doc = com_ocultar(&caso, "Peça");
        let (fim, _) = caso.exportar(&doc, ponta::exportacao("juri", "cpu", json!({ "originais": false }), "a3"));
        let id = ponta::id_da(&fim);
        let s = caso.saida(&id);
        let mapa = caso.retrato(&s).join(PECAS).join(MAPA);
        let original = fs::read(&mapa).unwrap();
        fs::write(&mapa, b"q,item\n").unwrap();
        let e = cmd::conferir(&caso.ws, &id, true).unwrap_err().to_string();
        assert!(e.contains(codigo::RETRATO_DIVERGENTE), "{e}");
        assert_eq!(caso.saida(&id).estado, crate::edicao::fila::EstadoSaida::AConferir);
        assert!(!caso.pasta(&s).join(PDF).exists() && !caso.pasta(&s).join(SOMAS).exists());
        // com a peça de volta, a mesma conferência conclui (o relatório da tentativa que falhou não atrapalha)
        fs::write(&mapa, original).unwrap();
        let feita = cmd::conferir(&caso.ws, &id, true).unwrap();
        assert_eq!(feita.estado, crate::edicao::fila::EstadoSaida::Concluida);
        ponta::conferir_somas(&caso.pasta(&s));
    }

    /// Revisão do P1.8: a conferência que morre no meio da cópia (sobra `.parcial` e pasta `projeto/` pela metade) ou
    /// cujo registro no banco falha depois de o pacote estar montado conclui na tentativa seguinte: as peças só saem do
    /// retrato com a saída registrada, e a sobra não barra a cópia.
    #[test]
    fn conferencia_interrompida_ou_sem_registro_conclui_depois() {
        let Some(caso) = ponta::caso("retomar") else { return };
        let doc = com_ocultar(&caso, "Retomar");
        let (fim, _) = caso.exportar(&doc, ponta::exportacao("juri", "cpu", json!({ "originais": true }), "a3"));
        let id = ponta::id_da(&fim);
        let s = caso.saida(&id);
        let (pasta, retrato) = (caso.pasta(&s), caso.retrato(&s));
        // processo morto no meio da cópia do pacote
        fs::write(pasta.join(format!("{MAPA}.parcial")), b"q,item\n0").unwrap();
        fs::create_dir_all(pasta.join(PROJETO)).unwrap();
        fs::write(pasta.join(PROJETO).join("projeto.json.parcial"), b"{").unwrap();
        // o registro falha depois de o pacote estar montado
        let sql = |t: &str| executar::abrir_banco(&caso.ws).unwrap().0.execute_batch(t).unwrap();
        sql("CREATE TRIGGER rev18_ocupado BEFORE UPDATE ON video_edit_outputs BEGIN SELECT RAISE(ABORT, 'banco ocupado'); END;");
        let e = cmd::conferir(&caso.ws, &id, true).unwrap_err().to_string();
        assert!(e.contains("banco ocupado"), "a sobra da cópia barrou a conferência: {e}");
        assert_eq!(caso.saida(&id).estado, crate::edicao::fila::EstadoSaida::AConferir);
        assert!(retrato.join(PECAS).join(PECAS_JSON).is_file() && retrato.join(fila::PENDENTE).is_file(), "as peças saíram antes do registro");
        // nada na pasta diz "conferida" enquanto o banco diz "a conferir"
        for a in [PDF, SOMAS, MAPA, PROJETO, ORIGINAIS, "Retomar.tecnico.json"] {
            assert!(!pasta.join(a).exists(), "{a} ficou da conferência que não foi registrada");
        }
        sql("DROP TRIGGER rev18_ocupado;");
        let feita = cmd::conferir(&caso.ws, &id, true).expect("a mesma conferência conclui depois");
        assert_eq!((feita.estado, feita.conferida_por.as_deref()), (crate::edicao::fila::EstadoSaida::Concluida, Some(PERITO)));
        let listados = ponta::conferir_somas(&pasta);
        assert!(!listados.iter().any(|a| a.ends_with(".parcial")), "{listados:?}");
        assert_eq!(listados.iter().filter(|a| a.starts_with("originais/")).count(), 1, "{listados:?}");
        assert!(!retrato.join(PECAS).exists() && !retrato.join(fila::PENDENTE).exists());
        assert!(fs::read(pasta.join(PDF)).unwrap().starts_with(b"%PDF-"));
    }

    /// Pacote que falha no meio (a gravação original sumiu do caso depois do render) não deixa nada dele na pasta da
    /// saída; e o retrato mexido entre o render e a conferência (grafo trocado) barra o pacote antes de copiar.
    #[test]
    fn pacote_que_falha_no_meio_nao_deixa_metade() {
        let Some(caso) = ponta::caso("metade") else { return };
        let doc = com_ocultar(&caso, "Metade");
        let (fim, _) = caso.exportar(&doc, ponta::exportacao("juri", "cpu", json!({ "originais": true }), "a3"));
        let id = ponta::id_da(&fim);
        let s = caso.saida(&id);
        let (pasta, retrato) = (caso.pasta(&s), caso.retrato(&s));
        let sem_pacote = |quando: &str| {
            for a in [PDF, SOMAS, MAPA, COMPOSTOS, DECODIFICADOS, AUDIO, PROJETO, ORIGINAIS] {
                assert!(!pasta.join(a).exists(), "{quando}: {a} ficou do pacote que falhou");
            }
            assert!(!pasta.join("Metade.tecnico.json").exists(), "{quando}: relatório de conferência que não concluiu");
            assert!(pasta.join("Metade (ocultado).mp4").is_file());
            assert_eq!(caso.saida(&id).estado, crate::edicao::fila::EstadoSaida::AConferir);
        };
        let original = paths::resolver_relativo(&caso.ws, doc["midias"][0]["relativo"].as_str().unwrap());
        let fora = original.with_extension("fora");
        fs::rename(&original, &fora).unwrap();
        let e = cmd::conferir(&caso.ws, &id, true).unwrap_err().to_string();
        assert!(e.contains(codigo::MIDIA_AUSENTE), "{e}");
        sem_pacote("original ausente");
        fs::rename(&fora, &original).unwrap();
        // grafo do retrato trocado
        let grafo = retrato.join("grafos/b000/b000.txt");
        let bom = fs::read(&grafo).unwrap();
        fs::write(&grafo, [&bom[..], b" "].concat()).unwrap();
        let e = cmd::conferir(&caso.ws, &id, true).unwrap_err().to_string();
        assert!(e.contains(codigo::RETRATO_DIVERGENTE) && e.contains("b000.txt"), "{e}");
        sem_pacote("retrato mexido");
        fs::write(&grafo, bom).unwrap();
        // item do retrato com nome oculto ficaria no pacote e fora das somas: barrado
        let manifesto = retrato.join("manifesto.json");
        let m_bom = fs::read(&manifesto).unwrap();
        let mut m: Manifesto = serde_json::from_slice(&m_bom).unwrap();
        let sha = executar::sha256_bytes(b"x");
        fs::write(retrato.join(format!("dados/{sha}.bin")), b"x").unwrap();
        m.itens.push(fila::ItemManifesto { papel: fila::PapelRetrato::Midia, sha256: sha.clone(), arquivo: format!("dados/{sha}.bin"), de: "midia/.oculta.bin".into(), tamanho: 1 });
        fs::write(&manifesto, serde_json::to_vec_pretty(&m).unwrap()).unwrap();
        let e = cmd::conferir(&caso.ws, &id, true).unwrap_err().to_string();
        assert!(e.contains(codigo::RETRATO_DIVERGENTE) && e.contains(&sha), "{e}");
        sem_pacote("item oculto");
        fs::write(&manifesto, m_bom).unwrap();
        fs::remove_file(retrato.join(format!("dados/{sha}.bin"))).unwrap();
        // sem o manifesto (é ele que diz que há pacote) a saída não conclui sem o pacote pedido
        fs::rename(&manifesto, retrato.join("manifesto.fora")).unwrap();
        let e = cmd::conferir(&caso.ws, &id, false).unwrap_err().to_string();
        assert!(e.contains(codigo::RETRATO_DIVERGENTE), "{e}");
        sem_pacote("sem manifesto");
        fs::rename(retrato.join("manifesto.fora"), &manifesto).unwrap();
        assert_eq!(cmd::conferir(&caso.ws, &id, true).unwrap().estado, crate::edicao::fila::EstadoSaida::Concluida);
        ponta::conferir_somas(&pasta);
    }

    #[test]
    fn nome_de_original_e_formato_das_somas() {
        // nome comprido (255 bytes no disco): cortado numa letra inteira, com a extensão, e cabe com ` (999)` e `.parcial`
        let longo = nome_seguro(&format!("{}.mp4", "gravação ".repeat(27)));
        assert!(longo.len() <= NOME_MAX && longo.ends_with(".mp4") && longo.starts_with("gravação gravação") && !longo.contains(" .mp4"), "{longo}");
        assert!(longo.len() + " (999)".len() + ".parcial".len() <= 255);
        assert_eq!(nome_seguro(&"ã".repeat(150)).len(), NOME_MAX);
        assert_eq!(nome_seguro("câmera 1: pátio?.mp4"), "câmera 1_ pátio_.mp4");
        assert_eq!(nome_seguro("../..\\x"), "_.._x");
        assert_eq!(nome_seguro(" . "), "arquivo");
        let d = fontes::pasta_temporaria("p18-somas");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("b.txt"), b"b").unwrap();
        fs::write(d.join("sub/a.txt"), b"a").unwrap();
        fs::write(d.join(PDF), b"pdf").unwrap();
        let t = sha256sums(&d, &["sub/a.txt".into(), "b.txt".into(), PDF.into(), "b.txt".into()]).unwrap();
        assert_eq!(t, format!("{}  b.txt\n{}  sub/a.txt\n", executar::sha256_bytes(b"b"), executar::sha256_bytes(b"a")));
        for ruim in ["../fora.txt", "a\nb", "c:\\x"] {
            assert!(sha256sums(&d, &[ruim.to_string()]).is_err(), "{ruim}");
        }
    }
}
