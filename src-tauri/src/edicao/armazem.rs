//! Armazenamento do projeto (§4): `Sistema/videos/edicao/<id>/`. Dono: P0.2.
//! Só o front manda gravar o `projeto.json`; aqui ele é gravado como chegou (`Value`), de forma atômica.

use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::importar_v1;
use super::mensagens::codigo;
use super::modelo::{
    ClipePrincipal, ClipeSobreposto, Efeito, MidiaRef, Nivel, OrigemMidia, Problema, ProjetoEdicao, Recebido,
    TipoMidia, Tolerante, Velocidade, ESQUEMA, ESQUEMA_MENOR,
};
use crate::error::{Result, SicroError};
use crate::workspace::manifest::APP_VERSION;
use crate::workspace::paths;

/// Linha da lista de projetos. `duracao_s` do programa; `miniatura_rel` relativo ao caso;
/// `problema` Some = arquivo ilegível (a frase vem do catálogo), com os demais campos no que deu para ler.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumoProjeto {
    pub id: String,
    pub titulo: String,
    pub updated_at: String,
    pub esquema: u32,
    pub duracao_s: f64,
    pub n_clipes: u32,
    pub miniatura_rel: Option<String>,
    pub problema: Option<String>,
}

/// Resposta de `edicao_abrir`. `projeto` = o JSON como está no disco; `importado_de` = Some(1) quando veio do v1 agora;
/// `so_leitura` = `esquema_menor` maior que o do programa; `avisos` = o que a abertura achou (importação, esquema).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProjetoAberto {
    pub projeto: Value,
    pub importado_de: Option<u32>,
    pub so_leitura: bool,
    pub avisos: Vec<Problema>,
    /// Raiz absoluta do cache da Edição deste caso (todo `relativo` de resposta parte daqui). O armazém deixa vazio; o comando preenche.
    pub raiz_cache: String,
}

/// Resposta de `edicao_salvar`: o `updated_at` gravado (RFC 3339).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Salvo {
    pub updated_at: String,
}

/// Uso de uma mídia (`edicao_usos`): projeto, clipes que a usam e exportações registradas desse projeto que a usaram.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsoMidia {
    pub projeto_id: String,
    pub titulo: String,
    pub n_clipes: u32,
    pub saidas: u32,
}

const PROJETO: &str = "projeto.json";
const ANTERIOR: &str = "projeto.anterior.json";
const EXPORTACOES: &str = "exportacoes";
const MINIATURA: &str = "miniatura.jpg";
const MIDIA: &str = "midia";
const RASTREIOS: &str = "rastreios";
const SILHUETAS: &str = "silhuetas";
const VIDSTAB: &str = "vidstab";

/// Extensões aceitas na mídia própria do projeto (as do áudio são as da v1).
const EXT_AUDIO: &[&str] = &["mp3", "wav", "m4a", "ogg", "webm", "aac", "flac", "opus", "mp4"];
const EXT_IMAGEM: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp"];

const TIPOS_CLIPE: &[&str] = &["trecho", "congelar", "imagem", "titulo", "tela"];
const TIPOS_SOBREPOSTO: &[&str] = &["video", "imagem", "titulo"];
const TIPOS_EFEITO: &[&str] = &[
    "estabilizar", "lente", "desentrelacar", "media_temporal", "ajustes", "nitidez", "girar", "enquadramento", "ocultar",
    "holofote", "destaque", "lupa", "regua", "velocidade_exibida", "texto", "carimbo", "imagem",
];

/// Onde o documento define ids locais (`*` = cada item da lista). Itens desconhecidos entram pelo `id` de cima.
const CAMINHOS_DE_ID: &[&[&str]] = &[
    &["midias", "*", "id"],
    &["midias", "*", "ocultacoes", "*", "id"],
    &["principal", "*", "id"],
    &["principal", "*", "efeitos", "*", "id"],
    &["principal", "*", "audio", "silencios", "*", "id"],
    &["principal", "*", "titulo", "blocos", "*", "id"],
    &["principal", "*", "celulas", "*", "efeitos", "*", "id"],
    &["sobreposicoes", "*", "id"],
    &["sobreposicoes", "*", "clipes", "*", "id"],
    &["sobreposicoes", "*", "clipes", "*", "efeitos", "*", "id"],
    &["sobreposicoes", "*", "clipes", "*", "audio", "silencios", "*", "id"],
    &["sobreposicoes", "*", "clipes", "*", "titulo", "blocos", "*", "id"],
    &["audios", "*", "id"],
    &["audios", "*", "clipes", "*", "id"],
    &["legendas", "itens", "*", "id"],
    &["rastreios", "*", "id"],
    &["marcadores", "*", "id"],
    &["sincronias", "*", "id"],
];

/// Chaves que guardam um id local (definição ou referência).
const CHAVES_DE_REFERENCIA: &[&str] = &["id", "midia", "rastreio", "clipe", "sincronia", "referencia", "outra"];
/// Objetos cujo `id` é de um registro do caso (calibração, cálculo de velocidade): nunca reescrito.
const RETRATOS_DE_FORA: &[&str] = &["calibracao", "calculo"];

/// Uma gravação por vez (salvar, importar, retrato anterior): o `.tmp` é um só por arquivo.
static GRAVACAO: Mutex<()> = Mutex::new(());
/// Abrir (importar, ler, guardar a anterior), salvar e excluir, um por vez: a anterior e o documento não se cruzam.
/// Sempre tomada ANTES de `GRAVACAO`.
static VEZ_PROJETO: Mutex<()> = Mutex::new(());
/// Pastas de projeto que já têm `projeto.anterior.json` nesta sessão.
static SESSAO: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// O id serve de nome de pasta? (`[A-Za-z0-9_-]{1,80}`)
pub fn id_valido(id: &str) -> bool {
    !id.is_empty() && id.len() <= 80 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Pasta do projeto: `<ws>/Sistema/videos/edicao/<id>/`.
pub fn pasta_projeto(ws: &Path, projeto_id: &str) -> PathBuf {
    paths::dir_edicao(ws).join(projeto_id)
}

/// Projetos do caso (v2 e v1 ainda não importados), do mais recente para o mais antigo.
pub fn listar(ws: &Path) -> Result<Vec<ResumoProjeto>> {
    let mut lista = Vec::new();
    let mut vistos = BTreeSet::new();
    if let Ok(rd) = fs::read_dir(paths::dir_edicao(ws)) {
        for e in rd.flatten() {
            let Some(id) = e.file_name().to_str().map(str::to_string) else { continue };
            let arq = e.path().join(PROJETO);
            // pasta só com `exportacoes/` = projeto excluído (O7)
            if !id_valido(&id) || !arq.is_file() {
                continue;
            }
            lista.push(resumo_v2(ws, &id, &arq));
            vistos.insert(id);
        }
    }
    if let Ok(rd) = fs::read_dir(paths::dir_montagens_json(ws)) {
        for e in rd.flatten() {
            let nome = e.file_name().to_string_lossy().to_string();
            let Some(id) = nome.strip_suffix(".json") else { continue };
            if !id_valido(id) || vistos.contains(id) || !e.path().is_file() {
                continue;
            }
            lista.push(resumo_v1(id, &e.path()));
        }
    }
    lista.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.id.cmp(&b.id)));
    Ok(lista)
}

/// Abre o projeto; importa do v1 se só houver o v1; faz `projeto.anterior.json` antes da primeira gravação da sessão.
pub fn abrir(ws: &Path, projeto_id: &str) -> Result<ProjetoAberto> {
    exigir_id(projeto_id)?;
    let _vez = VEZ_PROJETO.lock().unwrap_or_else(|e| e.into_inner());
    let pasta = pasta_projeto(ws, projeto_id);
    let arq = pasta.join(PROJETO);
    let mut importado_de = None;
    if !arq.is_file() {
        if !importar_v1::existe(ws, projeto_id) {
            return Err(SicroError::Filesystem(format!("projeto não encontrado: {projeto_id}")));
        }
        let banco = paths::sqlite_path(ws);
        if !banco.is_file() {
            return Err(SicroError::Database("banco do caso ausente".into()));
        }
        let conn = crate::database::connection::open_connection(&banco)?;
        importar_v1::importar(ws, &conn, projeto_id)?;
        importado_de = Some(1);
    }
    let bytes = fs::read(&arq)?;
    let projeto: Value =
        serde_json::from_slice(&bytes).map_err(|e| SicroError::Validation(format!("{}: {e}", codigo::ESQUEMA)))?;
    // os mesmos bytes devolvidos (reler o disco aqui pegaria uma gravação que entrou no meio);
    // pasta que não aceita gravação: o projeto abre só para ver
    let sem_gravacao = guardar_anterior(&pasta, Some(&bytes)).is_err();

    let mut avisos = Vec::new();
    let mut so_leitura = sem_gravacao;
    if sem_gravacao {
        avisos.push(problema(Nivel::Erro, codigo::SEM_GRAVACAO, String::new()));
    }
    let esquema_ok = projeto.get("esquema").and_then(Value::as_u64) == Some(ESQUEMA as u64);
    if !esquema_ok {
        so_leitura = true;
        avisos.push(problema(Nivel::Erro, codigo::ESQUEMA, String::new()));
    } else if projeto.get("id").and_then(Value::as_str) != Some(projeto_id) {
        // salvar gravaria em outra pasta
        so_leitura = true;
        let mut p = problema(Nivel::Erro, codigo::ESQUEMA, String::new());
        p.campo = Some("id".into());
        avisos.push(p);
    }
    if esquema_menor(&projeto) > ESQUEMA_MENOR as u64 {
        so_leitura = true;
        avisos.push(problema(Nivel::Erro, codigo::ESQUEMA_MENOR, String::new()));
    }
    if esquema_ok {
        match tipado(&projeto) {
            Ok(p) => {
                avisos.extend(desconhecidos(&p));
                if importado_de.is_some() {
                    for m in p.midias.iter().filter(|m| m.sha256.is_empty()) {
                        let mut a = problema(Nivel::Erro, codigo::MIDIA_AUSENTE, String::new());
                        a.midia = Some(m.id.clone());
                        avisos.push(a);
                    }
                }
            }
            Err(e) => {
                so_leitura = true;
                avisos.push(problema(Nivel::Erro, codigo::ITEM_INVALIDO, detalhe_do_erro(&e)));
            }
        }
    }
    Ok(ProjetoAberto { projeto, importado_de, so_leitura, avisos, raiz_cache: String::new() })
}

/// Leitura tipada de um documento (tolera item desconhecido).
pub fn tipado(projeto: &Value) -> Result<ProjetoEdicao> {
    if projeto.get("esquema").and_then(Value::as_u64) != Some(ESQUEMA as u64) {
        return Err(SicroError::Validation(codigo::ESQUEMA.into()));
    }
    serde_json::from_value(projeto.clone())
        .map_err(|e| SicroError::Validation(format!("{}: {e}", codigo::ITEM_INVALIDO)))
}

/// Grava o documento como chegou (`.tmp` + rename), carimbando só `updated_at` e `gravado_por`.
pub fn salvar(ws: &Path, projeto: &Value) -> Result<Salvo> {
    let id = projeto
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| id_valido(id))
        .ok_or_else(|| SicroError::Validation(codigo::ESQUEMA.into()))?;
    if projeto.get("esquema").and_then(Value::as_u64) != Some(ESQUEMA as u64) {
        return Err(SicroError::Validation(codigo::ESQUEMA.into()));
    }
    if esquema_menor(projeto) > ESQUEMA_MENOR as u64 {
        return Err(SicroError::Validation(codigo::ESQUEMA_MENOR.into()));
    }
    let pasta = pasta_projeto(ws, id);
    let _vez = VEZ_PROJETO.lock().unwrap_or_else(|e| e.into_inner());
    let marcado = SESSAO.lock().unwrap_or_else(|e| e.into_inner()).contains(&pasta);
    if !marcado {
        guardar_anterior(&pasta, None)?;
    }
    let updated_at = agora();
    let mut doc = projeto.clone();
    doc["updated_at"] = Value::String(updated_at.clone());
    doc["gravado_por"] = Value::String(APP_VERSION.into());
    gravar_atomico(&pasta.join(PROJETO), &serde_json::to_vec_pretty(&doc)?)?;
    Ok(Salvo { updated_at })
}

/// Apaga a pasta do projeto MENOS `exportacoes/` (O7) e o cache dele (`cache` = pasta da Edição do caso). Audita.
pub fn excluir(ws: &Path, projeto_id: &str, cache: Option<&Path>) -> Result<()> {
    exigir_id(projeto_id)?;
    let _vez = VEZ_PROJETO.lock().unwrap_or_else(|e| e.into_inner());
    let pasta = pasta_projeto(ws, projeto_id);
    let v1 = paths::dir_montagens_json(ws).join(format!("{projeto_id}.json"));
    let v1_importado = paths::dir_montagens_json(ws).join(format!("{projeto_id}.json.v1"));
    let tem_v2 = pasta.join(PROJETO).is_file();
    if !tem_v2 && !v1.is_file() {
        return Err(SicroError::Filesystem(format!("projeto não encontrado: {projeto_id}")));
    }
    let doc = ler_json(&pasta.join(PROJETO)).or_else(|| ler_json(&v1));
    let titulo = doc.as_ref().and_then(|d| d.get("titulo")).and_then(Value::as_str).unwrap_or("").to_string();
    let exportacoes = contar_entradas(&pasta.join(EXPORTACOES));
    auditar(
        ws,
        "video.edicao.projeto_excluido",
        projeto_id,
        serde_json::json!({ "projeto_id": projeto_id, "titulo": titulo, "exportacoes": exportacoes }),
    )?;

    if pasta.is_dir() {
        // `projeto.json` por último: se algo não sai (arquivo em uso), o projeto segue na lista e dá para repetir
        for e in fs::read_dir(&pasta)?.flatten() {
            let caminho = e.path();
            if (e.file_name() == EXPORTACOES && caminho.is_dir()) || e.file_name() == PROJETO {
                continue;
            }
            if caminho.is_dir() {
                fs::remove_dir_all(&caminho)?;
            } else {
                fs::remove_file(&caminho)?;
            }
        }
        if tem_v2 {
            fs::remove_file(pasta.join(PROJETO))?;
        }
        if contar_entradas(&pasta.join(EXPORTACOES)) == 0 {
            fs::remove_dir_all(&pasta)?;
        }
    }
    for arq in [&v1, &v1_importado] {
        if arq.is_file() {
            fs::remove_file(arq)?;
        }
    }
    if let Some(cache) = cache {
        limpar_cache(cache, projeto_id);
    }
    SESSAO.lock().unwrap_or_else(|e| e.into_inner()).remove(&pasta);
    Ok(())
}

/// Copia o projeto com id novo (rastreios, `.trf` e mídia própria juntos; exportações não) e devolve o resumo da cópia.
pub fn duplicar(ws: &Path, projeto_id: &str) -> Result<ResumoProjeto> {
    exigir_id(projeto_id)?;
    let origem = pasta_projeto(ws, projeto_id);
    if !origem.join(PROJETO).is_file() && importar_v1::existe(ws, projeto_id) {
        abrir(ws, projeto_id)?;
    }
    let doc: Value = serde_json::from_slice(&fs::read(origem.join(PROJETO))?)
        .map_err(|e| SicroError::Validation(format!("{}: {e}", codigo::ESQUEMA)))?;
    if doc.get("esquema").and_then(Value::as_u64) != Some(ESQUEMA as u64) {
        return Err(SicroError::Validation(codigo::ESQUEMA.into()));
    }
    if esquema_menor(&doc) > ESQUEMA_MENOR as u64 {
        return Err(SicroError::Validation(codigo::ESQUEMA_MENOR.into()));
    }

    let novo_id = Uuid::new_v4().to_string();
    let mut mapa: HashMap<String, String> = HashMap::new();
    for caminho in CAMINHOS_DE_ID {
        let mut ids = Vec::new();
        strings_em(&doc, caminho, &mut ids);
        for id in ids {
            mapa.entry(id.to_string()).or_insert_with(|| Uuid::new_v4().to_string());
        }
    }
    let mut novo = doc.clone();
    reescrever_referencias(&mut novo, &mapa);
    let prefixo_velho = format!("{}/{projeto_id}/", paths::REL_EDICAO);
    let prefixo_novo = format!("{}/{novo_id}/", paths::REL_EDICAO);
    if let Some(midias) = novo.get_mut("midias").and_then(Value::as_array_mut) {
        for m in midias {
            if let Some(Value::String(rel)) = m.get_mut("relativo") {
                if let Some(resto) = rel.strip_prefix(&prefixo_velho) {
                    *rel = format!("{prefixo_novo}{resto}");
                }
            }
        }
    }
    let quando = agora();
    novo["id"] = Value::String(novo_id.clone());
    novo["created_at"] = Value::String(quando.clone());
    novo["updated_at"] = Value::String(quando);
    novo["gravado_por"] = Value::String(APP_VERSION.into());

    let destino = pasta_projeto(ws, &novo_id);
    let r = (|| -> Result<ResumoProjeto> {
        fs::create_dir_all(&destino)?;
        for sub in [MIDIA, VIDSTAB] {
            copiar_arvore(&origem.join(sub), &destino.join(sub))?;
        }
        if origem.join(MINIATURA).is_file() {
            fs::copy(origem.join(MINIATURA), destino.join(MINIATURA))?;
        }
        // denso e silhuetas vão byte a byte (mesmo SHA-256: a confirmação do perito segue valendo);
        // só id de rastreio que serve de nome de pasta vira caminho
        let mut rids = Vec::new();
        strings_em(&doc, &["rastreios", "*", "id"], &mut rids);
        for velho in rids.into_iter().filter(|r| id_valido(r)) {
            let Some(rid) = mapa.get(velho) else { continue };
            copiar_arvore(&origem.join(RASTREIOS).join(velho), &destino.join(RASTREIOS).join(rid))?;
            copiar_arvore(&origem.join(SILHUETAS).join(velho), &destino.join(SILHUETAS).join(rid))?;
        }
        let arq = destino.join(PROJETO);
        gravar_atomico(&arq, &serde_json::to_vec_pretty(&novo)?)?;
        Ok(resumo_v2(ws, &novo_id, &arq))
    })();
    if r.is_err() {
        let _ = fs::remove_dir_all(&destino);
    }
    r
}

/// Copia um arquivo para `midia/<uuid>.<ext>` do projeto, calcula o SHA-256 e devolve a `MidiaRef` (`origem: projeto`).
pub fn importar_midia(ws: &Path, projeto_id: &str, origem: &Path, tipo: TipoMidia) -> Result<MidiaRef> {
    exigir_id(projeto_id)?;
    let ext = origem.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let aceitas = match tipo {
        TipoMidia::Audio => EXT_AUDIO,
        TipoMidia::Imagem => EXT_IMAGEM,
        // vídeo entra pelo registro do caso (proveniência), nunca como mídia própria
        TipoMidia::Video => &[][..],
    };
    if !aceitas.contains(&ext.as_str()) {
        return Err(SicroError::Validation(format!("{}: {ext}", codigo::EXTENSAO)));
    }
    let nome = origem.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let fonte = File::open(origem)?;
    if fonte.metadata()?.len() == 0 {
        return Err(SicroError::Validation(codigo::ARQUIVO_VAZIO.into()));
    }
    gravar_midia_de(ws, projeto_id, &ext, tipo, Some(nome), fonte)
}

/// Grava os bytes de uma narração em `midia/<uuid>.<ext>` (extensão pelo `mime`) e devolve a `MidiaRef`.
pub fn gravar_midia(ws: &Path, projeto_id: &str, mime: &str, dados: &[u8]) -> Result<MidiaRef> {
    exigir_id(projeto_id)?;
    if dados.is_empty() {
        return Err(SicroError::Validation(codigo::ARQUIVO_VAZIO.into()));
    }
    let (tipo, ext) = por_mime(mime).ok_or_else(|| SicroError::Validation(format!("{}: {mime}", codigo::MIME)))?;
    gravar_midia_de(ws, projeto_id, ext, tipo, None, dados)
}

/// Projetos e exportações que usam a mídia com esse SHA-256.
pub fn usos(ws: &Path, sha256: &str) -> Result<Vec<UsoMidia>> {
    let mut lista = Vec::new();
    let Ok(rd) = fs::read_dir(paths::dir_edicao(ws)) else { return Ok(lista) };
    for e in rd.flatten() {
        let Some(id) = e.file_name().to_str().map(str::to_string) else { continue };
        if !id_valido(&id) || !e.path().is_dir() {
            continue;
        }
        let mut titulo = String::new();
        let mut n_clipes = 0;
        let mut usa = false;
        if let Some(doc) = ler_json(&e.path().join(PROJETO)) {
            let ids = midias_com_sha(&doc, sha256);
            if !ids.is_empty() {
                usa = true;
                titulo = doc.get("titulo").and_then(Value::as_str).unwrap_or("").to_string();
                n_clipes = contar_clipes(&doc, &ids);
            }
        }
        let mut saidas = 0;
        if let Ok(rd) = fs::read_dir(e.path().join(EXPORTACOES)) {
            for s in rd.flatten() {
                let Some(retrato) = ler_json(&s.path().join(PROJETO)) else { continue };
                if !midias_com_sha(&retrato, sha256).is_empty() {
                    saidas += 1;
                    if titulo.is_empty() {
                        titulo = retrato.get("titulo").and_then(Value::as_str).unwrap_or("").to_string();
                    }
                }
            }
        }
        if usa || saidas > 0 {
            lista.push(UsoMidia { projeto_id: id, titulo, n_clipes, saidas });
        }
    }
    lista.sort_by(|a, b| a.titulo.cmp(&b.titulo).then_with(|| a.projeto_id.cmp(&b.projeto_id)));
    Ok(lista)
}

/// Apaga densos, silhuetas e `.trf` do projeto que o documento não cita mais (ao fechar o projeto).
/// `manter` = SHA-256 ainda vivos no histórico da tela.
pub fn limpar_orfaos(ws: &Path, projeto: &ProjetoEdicao, manter: &[String]) -> Result<u32> {
    exigir_id(&projeto.id)?;
    let pasta = pasta_projeto(ws, &projeto.id);
    let mut vivos: BTreeSet<String> = manter.iter().cloned().collect();
    // O tipado perde campo novo de item conhecido: o que o disco e a anterior citam (qualquer texto) também vale.
    // Documento de versão mais nova, ou que não dá para ler: não se apaga nada.
    if projeto.esquema_menor > ESQUEMA_MENOR {
        return Ok(0);
    }
    for nome in [PROJETO, ANTERIOR] {
        let arq = pasta.join(nome);
        if !arq.is_file() {
            continue;
        }
        match ler_json(&arq) {
            Some(v) if nome == ANTERIOR || esquema_menor(&v) <= ESQUEMA_MENOR as u64 => textos(&v, &mut vivos),
            _ if nome == ANTERIOR => {}
            _ => return Ok(0),
        }
    }
    for r in &projeto.rastreios {
        vivos.extend(r.denso.iter().map(|d| d.sha256.clone()));
        vivos.extend(r.silhuetas.iter().map(|s| s.sha256.clone()));
        vivos.extend(r.confirmacao.iter().filter_map(|c| c.denso_sha256.clone()));
    }
    // item desconhecido ou inválido pode citar um arquivo: tudo que é texto nele conta
    let mut efeito = |e: &Tolerante<Efeito>| match e {
        Tolerante::Conhecido(Efeito::Estabilizar(x)) => vivos.extend(x.trf_sha256.clone()),
        Tolerante::Conhecido(_) => {}
        Tolerante::Desconhecido(v) => textos(v, &mut vivos),
    };
    for c in &projeto.principal {
        match c {
            Tolerante::Conhecido(c) => {
                let base = base_do_clipe(c);
                base.efeitos.iter().for_each(&mut efeito);
                if let ClipePrincipal::Tela(t) = c {
                    t.celulas.iter().flat_map(|cel| cel.efeitos.iter()).for_each(&mut efeito);
                }
            }
            Tolerante::Desconhecido(v) => efeito(&Tolerante::Desconhecido(v.clone())),
        }
    }
    for t in &projeto.sobreposicoes {
        for c in &t.clipes {
            match c {
                Tolerante::Conhecido(c) => base_do_sobreposto(c).efeitos.iter().for_each(&mut efeito),
                Tolerante::Desconhecido(v) => efeito(&Tolerante::Desconhecido(v.clone())),
            }
        }
    }
    if let Ok(rd) = fs::read_dir(pasta.join(EXPORTACOES)) {
        for s in rd.flatten() {
            if let Some(v) = ler_json(&s.path().join(PROJETO)) {
                textos(&v, &mut vivos);
            }
        }
    }

    let mut apagados = 0u32;
    for (sub, ext) in [(RASTREIOS, "json"), (SILHUETAS, "sil")] {
        let Ok(rd) = fs::read_dir(pasta.join(sub)) else { continue };
        for d in rd.flatten().filter(|d| d.path().is_dir()) {
            apagados += apagar_sem_referencia(&d.path(), ext, &vivos)?;
            if contar_entradas(&d.path()) == 0 {
                let _ = fs::remove_dir(d.path());
            }
        }
    }
    apagados += apagar_sem_referencia(&pasta.join(VIDSTAB), "trf", &vivos)?;
    Ok(apagados)
}

// ---------------------------------------------------------------------------
// Gravação atômica

/// `.tmp` + rename na mesma pasta; a pasta é criada se faltar (caso antigo não tem as pastas da Edição).
pub(super) fn gravar_atomico(destino: &Path, bytes: &[u8]) -> Result<()> {
    gravar_atomico_com(destino, |f| f.write_all(bytes))
}

fn gravar_atomico_com(destino: &Path, escrever: impl FnOnce(&mut File) -> io::Result<()>) -> Result<()> {
    let _vez = GRAVACAO.lock().unwrap_or_else(|e| e.into_inner());
    let pasta = destino.parent().ok_or_else(|| SicroError::Filesystem(format!("sem pasta: {}", destino.display())))?;
    fs::create_dir_all(pasta)?;
    let nome = destino.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = pasta.join(format!("{nome}.tmp"));
    let escrito = (|| -> io::Result<()> {
        let mut f = File::create(&tmp)?;
        escrever(&mut f)?;
        f.sync_all()
    })();
    if let Err(e) = escrito.and_then(|_| fs::rename(&tmp, destino)) {
        let _ = fs::remove_file(&tmp);
        return Err(SicroError::Filesystem(format!("não gravou {}: {e}", destino.display())));
    }
    #[cfg(unix)]
    if let Ok(d) = File::open(pasta) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// `projeto.anterior.json` = o `projeto.json` como estava ao abrir (`lido`) ou, sem abertura, como está no disco.
fn guardar_anterior(pasta: &Path, lido: Option<&[u8]>) -> Result<()> {
    let arq = pasta.join(PROJETO);
    match lido {
        Some(bytes) => gravar_atomico(&pasta.join(ANTERIOR), bytes)?,
        None if arq.is_file() => gravar_atomico(&pasta.join(ANTERIOR), &fs::read(&arq)?)?,
        None => {}
    }
    SESSAO.lock().unwrap_or_else(|e| e.into_inner()).insert(pasta.to_path_buf());
    Ok(())
}

// ---------------------------------------------------------------------------
// Auxiliares

fn exigir_id(id: &str) -> Result<()> {
    if id_valido(id) {
        Ok(())
    } else {
        Err(SicroError::Validation(codigo::ESQUEMA.into()))
    }
}

fn agora() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn esquema_menor(doc: &Value) -> u64 {
    doc.get("esquema_menor").and_then(Value::as_u64).unwrap_or(0)
}

fn ler_json(arq: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(arq).ok()?).ok()
}

fn data_de(arq: &Path) -> String {
    fs::metadata(arq)
        .and_then(|m| m.modified())
        .map(|t| DateTime::<Utc>::from(t).to_rfc3339_opts(SecondsFormat::Millis, true))
        .unwrap_or_default()
}

fn contar_entradas(pasta: &Path) -> usize {
    fs::read_dir(pasta).map(|rd| rd.flatten().count()).unwrap_or(0)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Até o catálogo do P0.R (`mensagens::problema`) existir, a mensagem é o detalhe (texto do serde em
/// `item_invalido`) ou vazia; o nível segue o §2.3.
fn problema(nivel: Nivel, codigo: &'static str, mensagem: String) -> Problema {
    Problema { nivel, codigo, mensagem, clipe: None, efeito: None, trilha: None, midia: None, campo: None, n: None }
}

/// O texto do serde depois do código (`item_invalido: <texto>`).
fn detalhe_do_erro(e: &SicroError) -> String {
    let s = match e {
        SicroError::Validation(s) => s.as_str(),
        _ => "",
    };
    s.split_once(": ").map(|(_, d)| d).unwrap_or("").to_string()
}

fn resumo_v2(ws: &Path, id: &str, arq: &Path) -> ResumoProjeto {
    let miniatura = pasta_projeto(ws, id).join(MINIATURA);
    let miniatura_rel = miniatura.is_file().then(|| format!("{}/{id}/{MINIATURA}", paths::REL_EDICAO));
    let mut r = ResumoProjeto {
        id: id.to_string(),
        titulo: String::new(),
        updated_at: data_de(arq),
        esquema: 0,
        duracao_s: 0.0,
        n_clipes: 0,
        miniatura_rel,
        problema: Some(codigo::ESQUEMA.into()),
    };
    let Some(doc) = ler_json(arq).filter(Value::is_object) else { return r };
    r.titulo = doc.get("titulo").and_then(Value::as_str).unwrap_or("").to_string();
    if let Some(u) = doc.get("updated_at").and_then(Value::as_str) {
        r.updated_at = u.to_string();
    }
    r.esquema = doc.get("esquema").and_then(Value::as_u64).unwrap_or(0) as u32;
    r.n_clipes = doc.get("principal").and_then(Value::as_array).map(|a| a.len() as u32).unwrap_or(0);
    if r.esquema != ESQUEMA {
        return r;
    }
    match tipado(&doc) {
        Ok(p) => {
            r.duracao_s = duracao_programa(&p);
            r.problema = None;
        }
        Err(_) => r.problema = Some(codigo::ITEM_INVALIDO.into()),
    }
    r
}

fn resumo_v1(id: &str, arq: &Path) -> ResumoProjeto {
    let mut r = ResumoProjeto {
        id: id.to_string(),
        titulo: String::new(),
        updated_at: data_de(arq),
        esquema: 1,
        duracao_s: 0.0,
        n_clipes: 0,
        miniatura_rel: None,
        problema: Some(codigo::ESQUEMA.into()),
    };
    let Some(doc) = ler_json(arq).filter(Value::is_object) else { return r };
    r.titulo = doc.get("titulo").and_then(Value::as_str).unwrap_or("").to_string();
    if let Some(u) = doc.get("updated_at").and_then(Value::as_str) {
        r.updated_at = u.to_string();
    }
    let itens = doc.get("itens").and_then(Value::as_array).cloned().unwrap_or_default();
    r.n_clipes = itens.len() as u32;
    r.duracao_s = itens
        .iter()
        .map(|it| {
            let n = |k: &str| it.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            let d = if it.get("tipo").and_then(Value::as_str) == Some("trecho") {
                let v = n("velocidade");
                (n("fim_s") - n("inicio_s")) / if v > 0.0 { v } else { 1.0 }
            } else {
                n("duracao_s")
            };
            if d.is_finite() { d.max(0.0) } else { 0.0 }
        })
        .sum();
    r.problema = None;
    r
}

/// Duração do programa em s, para a lista (a exata é `Analise.linha.total_q`, que pede as tabelas de quadros).
fn duracao_programa(p: &ProjetoEdicao) -> f64 {
    let mut total = 0.0;
    let mut primeiro = true;
    for c in &p.principal {
        let Tolerante::Conhecido(c) = c else { continue };
        let d = match c {
            ClipePrincipal::Trecho(x) => duracao_na_saida(x.inicio_s, x.fim_s, &x.velocidade),
            ClipePrincipal::Tela(x) => duracao_na_saida(x.inicio_s, x.fim_s, &x.velocidade),
            ClipePrincipal::Congelar(x) => x.duracao_s,
            ClipePrincipal::Imagem(x) => x.duracao_s,
            ClipePrincipal::Titulo(x) => x.duracao_s,
        };
        let base = base_do_clipe(c);
        if base.desligado {
            continue;
        }
        let d = if d.is_finite() { d.max(0.0) } else { 0.0 };
        if !primeiro {
            if let Some(t) = base.transicao_entrada.as_ref().filter(|t| t.tipo != "escurecer" && t.duracao_s.is_finite()) {
                total -= t.duracao_s.clamp(0.0, d);
            }
        }
        total += d;
        primeiro = false;
    }
    total.max(0.0)
}

/// Tempo de fonte [ini, fim) passado na velocidade do clipe; rampa = ∫ dt / v(t), linear por partes.
fn duracao_na_saida(ini: f64, fim: f64, v: &Velocidade) -> f64 {
    let vao = fim - ini;
    if !(vao > 0.0) {
        return 0.0;
    }
    let pontos = match v {
        Velocidade::Constante { fator } => return if *fator > 0.0 { vao / fator } else { vao },
        Velocidade::Rampa { pontos } => pontos,
    };
    let mut p: Vec<(f64, f64, bool)> = pontos
        .iter()
        .filter(|c| c.t.is_finite() && c.v > 0.0)
        .map(|c| (c.t, c.v, c.interp == super::modelo::Interp::Degrau))
        .collect();
    p.sort_by(|a, b| a.0.total_cmp(&b.0));
    if p.is_empty() {
        return vao;
    }
    // ∫ dt / v com v linear de va a vb em [a, b]
    let pedaco = |a: f64, b: f64, va: f64, vb: f64| {
        if b <= a {
            0.0
        } else if (vb - va).abs() < 1e-12 {
            (b - a) / va
        } else {
            (b - a) * (vb / va).ln() / (vb - va)
        }
    };
    let mut total = pedaco(ini, fim.min(p[0].0), p[0].1, p[0].1);
    for w in p.windows(2) {
        let ((t0, v0, degrau), (t1, v1, _)) = (w[0], w[1]);
        let (a, b) = (ini.max(t0), fim.min(t1));
        if b <= a {
            continue;
        }
        let em = |t: f64| if degrau || t1 <= t0 { v0 } else { v0 + (v1 - v0) * (t - t0) / (t1 - t0) };
        total += pedaco(a, b, em(a), em(b));
    }
    let (tn, vn, _) = p[p.len() - 1];
    total + pedaco(ini.max(tn), fim, vn, vn)
}

fn base_do_clipe(c: &ClipePrincipal) -> &super::modelo::ClipeBase {
    match c {
        ClipePrincipal::Trecho(x) => &x.base,
        ClipePrincipal::Congelar(x) => &x.base,
        ClipePrincipal::Imagem(x) => &x.base,
        ClipePrincipal::Titulo(x) => &x.base,
        ClipePrincipal::Tela(x) => &x.base,
    }
}

fn base_do_sobreposto(c: &ClipeSobreposto) -> &super::modelo::SobBase {
    match c {
        ClipeSobreposto::Video(x) => &x.base,
        ClipeSobreposto::Imagem(x) => &x.base,
        ClipeSobreposto::Titulo(x) => &x.base,
    }
}

/// Itens que caíram em `Desconhecido`: `tipo` fora da lista = item de versão mais nova; conhecido = inválido.
/// Erro quando o item vale no render (clipe ligado, efeito ligado em clipe ligado); senão aviso.
fn desconhecidos(p: &ProjetoEdicao) -> Vec<Problema> {
    fn um(v: &Value, tipos: &[&str], ligado: bool, ler: impl Fn(&Value) -> Option<String>) -> Problema {
        let conhecido = v.get("tipo").and_then(Value::as_str).is_some_and(|t| tipos.contains(&t));
        let nivel = if ligado { Nivel::Erro } else { Nivel::Aviso };
        if conhecido {
            problema(nivel, codigo::ITEM_INVALIDO, ler(v).unwrap_or_default())
        } else {
            problema(nivel, codigo::ITEM_DESCONHECIDO, String::new())
        }
    }
    let id_de = |v: &Value| v.get("id").and_then(Value::as_str).map(str::to_string);
    let erro_efeito = |v: &Value| serde_json::from_value::<Efeito>(v.clone()).err().map(|e| e.to_string());
    let mut lista = Vec::new();
    let efeitos = |efs: &[Tolerante<Efeito>], clipe: Option<String>, trilha: Option<String>, clipe_ligado: bool, lista: &mut Vec<Problema>| {
        for e in efs {
            if let Tolerante::Desconhecido(v) = e {
                let ligado = clipe_ligado && v.get("ligado").and_then(Value::as_bool).unwrap_or(true);
                let mut a = um(v, TIPOS_EFEITO, ligado, erro_efeito);
                a.clipe = clipe.clone();
                a.efeito = id_de(v);
                a.trilha = trilha.clone();
                lista.push(a);
            }
        }
    };
    for c in &p.principal {
        match c {
            Tolerante::Conhecido(c) => {
                let base = base_do_clipe(c);
                efeitos(&base.efeitos, Some(base.id.clone()), None, !base.desligado, &mut lista);
                if let ClipePrincipal::Tela(t) = c {
                    for cel in &t.celulas {
                        efeitos(&cel.efeitos, Some(base.id.clone()), None, !base.desligado, &mut lista);
                    }
                }
            }
            Tolerante::Desconhecido(v) => {
                let ligado = !v.get("desligado").and_then(Value::as_bool).unwrap_or(false);
                let mut a = um(v, TIPOS_CLIPE, ligado, |v| {
                    serde_json::from_value::<ClipePrincipal>(v.clone()).err().map(|e| e.to_string())
                });
                a.clipe = id_de(v);
                lista.push(a);
            }
        }
    }
    for t in &p.sobreposicoes {
        for c in &t.clipes {
            match c {
                Tolerante::Conhecido(c) => {
                    let base = base_do_sobreposto(c);
                    efeitos(&base.efeitos, Some(base.id.clone()), Some(t.id.clone()), !t.oculta, &mut lista);
                }
                Tolerante::Desconhecido(v) => {
                    let mut a = um(v, TIPOS_SOBREPOSTO, !t.oculta, |v| {
                        serde_json::from_value::<ClipeSobreposto>(v.clone()).err().map(|e| e.to_string())
                    });
                    a.clipe = id_de(v);
                    a.trilha = Some(t.id.clone());
                    lista.push(a);
                }
            }
        }
    }
    lista
}

/// Strings no fim de `caminho` (`*` = cada item da lista).
fn strings_em<'a>(v: &'a Value, caminho: &[&str], saida: &mut Vec<&'a str>) {
    match caminho.split_first() {
        None => saida.extend(v.as_str()),
        Some((&"*", resto)) => {
            for x in v.as_array().into_iter().flatten() {
                strings_em(x, resto, saida);
            }
        }
        Some((chave, resto)) => {
            if let Some(x) = v.get(*chave) {
                strings_em(x, resto, saida);
            }
        }
    }
}

/// Troca, em todo o documento (itens desconhecidos inclusive), os ids locais nas chaves de referência.
fn reescrever_referencias(v: &mut Value, mapa: &HashMap<String, String>) {
    reescrever(v, mapa, false)
}

/// `de_fora` = o objeto é um retrato de registro do caso (`RETRATOS_DE_FORA`): o `id` dele não é local.
fn reescrever(v: &mut Value, mapa: &HashMap<String, String>, de_fora: bool) {
    match v {
        Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                if let Value::String(s) = x {
                    if CHAVES_DE_REFERENCIA.contains(&k.as_str()) && !(de_fora && k == "id") {
                        if let Some(n) = mapa.get(s.as_str()) {
                            *s = n.clone();
                        }
                    }
                } else {
                    reescrever(x, mapa, RETRATOS_DE_FORA.contains(&k.as_str()));
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| reescrever(x, mapa, false)),
        _ => {}
    }
}

fn textos(v: &Value, saida: &mut BTreeSet<String>) {
    match v {
        Value::String(s) => {
            saida.insert(s.clone());
        }
        Value::Array(a) => a.iter().for_each(|x| textos(x, saida)),
        Value::Object(o) => o.values().for_each(|x| textos(x, saida)),
        _ => {}
    }
}

fn apagar_sem_referencia(pasta: &Path, ext: &str, vivos: &BTreeSet<String>) -> Result<u32> {
    let mut n = 0;
    let Ok(rd) = fs::read_dir(pasta) else { return Ok(0) };
    for e in rd.flatten() {
        let nome = e.file_name().to_string_lossy().to_string();
        let Some(sha) = nome.strip_suffix(&format!(".{ext}")) else { continue };
        if e.path().is_file() && !vivos.contains(sha) {
            fs::remove_file(e.path())?;
            n += 1;
        }
    }
    Ok(n)
}

fn copiar_arvore(de: &Path, para: &Path) -> Result<()> {
    if !de.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(para)?;
    for e in fs::read_dir(de)?.flatten() {
        let nome = e.file_name();
        if nome.to_string_lossy().ends_with(".tmp") {
            continue;
        }
        if e.path().is_dir() {
            copiar_arvore(&e.path(), &para.join(&nome))?;
        } else {
            fs::copy(e.path(), para.join(&nome))?;
        }
    }
    Ok(())
}

fn midias_com_sha(doc: &Value, sha256: &str) -> BTreeSet<String> {
    doc.get("midias")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|m| m.get("sha256").and_then(Value::as_str) == Some(sha256))
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// Clipes (principal, sobrepostos e de áudio) que citam alguma dessas mídias em qualquer campo `midia`.
fn contar_clipes(doc: &Value, ids: &BTreeSet<String>) -> u32 {
    fn cita(v: &Value, ids: &BTreeSet<String>) -> bool {
        match v {
            Value::Object(o) => o.iter().any(|(k, x)| {
                (k == "midia" && x.as_str().is_some_and(|s| ids.contains(s))) || cita(x, ids)
            }),
            Value::Array(a) => a.iter().any(|x| cita(x, ids)),
            _ => false,
        }
    }
    let mut clipes = Vec::new();
    for caminho in [&["principal", "*"][..], &["sobreposicoes", "*", "clipes", "*"][..], &["audios", "*", "clipes", "*"][..]] {
        valores_em(doc, caminho, &mut clipes);
    }
    clipes.into_iter().filter(|c| cita(c, ids)).count() as u32
}

fn valores_em<'a>(v: &'a Value, caminho: &[&str], saida: &mut Vec<&'a Value>) {
    match caminho.split_first() {
        None => saida.push(v),
        Some((&"*", resto)) => {
            for x in v.as_array().into_iter().flatten() {
                valores_em(x, resto, saida);
            }
        }
        Some((chave, resto)) => {
            if let Some(x) = v.get(*chave) {
                valores_em(x, resto, saida);
            }
        }
    }
}

fn por_mime(mime: &str) -> Option<(TipoMidia, &'static str)> {
    let base = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let ext = match base.as_str() {
        "audio/webm" => "webm",
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => "m4a",
        "audio/aac" => "aac",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/wav" | "audio/x-wav" | "audio/wave" | "audio/vnd.wave" => "wav",
        "audio/flac" | "audio/x-flac" => "flac",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        _ => return None,
    };
    Some((if base.starts_with("image/") { TipoMidia::Imagem } else { TipoMidia::Audio }, ext))
}

/// Copia `fonte` para `midia/<uuid>.<ext>` (`.tmp` + rename) calculando o SHA-256 no caminho.
fn gravar_midia_de(
    ws: &Path,
    projeto_id: &str,
    ext: &str,
    tipo: TipoMidia,
    nome: Option<String>,
    mut fonte: impl Read,
) -> Result<MidiaRef> {
    let id = Uuid::new_v4().to_string();
    let arquivo = format!("{id}.{ext}");
    let pasta = pasta_projeto(ws, projeto_id).join(MIDIA);
    fs::create_dir_all(&pasta)?;
    let destino = pasta.join(&arquivo);
    let tmp = pasta.join(format!("{arquivo}.tmp"));
    let copiado = (|| -> io::Result<String> {
        let mut f = File::create(&tmp)?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = fonte.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
            f.write_all(&buf[..n])?;
        }
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, &destino)?;
        Ok(format!("{:x}", h.finalize()))
    })();
    let sha256 = match copiado {
        Ok(s) => s,
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            return Err(SicroError::Filesystem(format!("não gravou {}: {e}", destino.display())));
        }
    };
    let mut m = MidiaRef {
        id,
        tipo,
        origem: OrigemMidia::Projeto,
        media_id: None,
        sha256,
        nome: nome.unwrap_or_else(|| arquivo.clone()),
        relativo: format!("{}/{projeto_id}/{MIDIA}/{arquivo}", paths::REL_EDICAO),
        duracao_s: None,
        inicio_s: None,
        largura: None,
        altura: None,
        taxa: None,
        vfr: false,
        tem_audio: tipo == TipoMidia::Audio,
        cor: None,
        derivacao: None,
        recebido: Recebido { nativo: None, obs: String::new() },
        relogio_desvio_s: None,
        gravador: None,
        decodificacao: None,
        ocultacoes: Vec::new(),
    };
    // o resto vem do ffprobe quando ele lê o arquivo; sem ele a mídia entra só com o SHA-256
    if let Ok(p) = crate::video::probe_media(&destino) {
        let bruto: Value = serde_json::from_str(&p.raw_json).unwrap_or(Value::Null);
        match tipo {
            TipoMidia::Imagem => {
                m.largura = p.width;
                m.altura = p.height;
            }
            _ => {
                m.duracao_s = p.duration_s;
                m.inicio_s = Some(crate::video::probe::container_start_time(&p.raw_json));
                m.tem_audio = bruto["streams"]
                    .as_array()
                    .is_some_and(|s| s.iter().any(|s| s["codec_type"].as_str() == Some("audio")));
            }
        }
    }
    Ok(m)
}

/// `render/<tarefa>/` cujo retrato é deste projeto. O resto do cache é endereçado pelo conteúdo (pode ser de outro projeto).
fn limpar_cache(cache: &Path, projeto_id: &str) {
    let Ok(rd) = fs::read_dir(cache.join("render")) else { return };
    for e in rd.flatten() {
        let deste = [e.path().join("projeto").join(PROJETO), e.path().join(PROJETO)]
            .iter()
            .filter_map(|a| ler_json(a))
            .any(|v| v.get("id").and_then(Value::as_str) == Some(projeto_id));
        if deste {
            let _ = fs::remove_dir_all(e.path());
        }
    }
}

/// Grava em `audit_logs` quando o caso tem banco (caso de teste sem banco não audita).
fn auditar(ws: &Path, acao: &str, projeto_id: &str, detalhes: Value) -> Result<()> {
    let banco = paths::sqlite_path(ws);
    if !banco.is_file() {
        return Ok(());
    }
    let occ = crate::workspace::manifest::Manifest::read(ws).ok().map(|m| m.occurrence_id);
    let conn = crate::database::connection::open_connection(&banco)?;
    let entidade = Uuid::parse_str(projeto_id).ok();
    crate::database::repositories::occurrence_repo::record_audit(
        &conn,
        occ.as_ref(),
        acao,
        Some("video"),
        Some("edicao_projeto"),
        entidade.as_ref(),
        Some(&detalhes.to_string()),
    )
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    const MINIMO: &str = "0b6f1c2a-5d1e-4c1b-9a43-2f7d0c1e9b10";
    const DESCONHECIDO: &str = "7c2d9e4b-1f3a-4b5c-8d6e-0a1b2c3d4e5f";
    const FUTURO: &str = "3e8a1b2c-4d5e-4f60-9a1b-2c3d4e5f6a7b";
    const ILEGIVEL: &str = "9d1e2f3a-4b5c-4d6e-8f70-81a2b3c4d5e6";

    /// Caso antigo (sem as pastas da Edição), com espaço e acento no nome da pasta.
    fn caso() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::Builder::new().prefix("sicro-p02-").tempdir().unwrap();
        let ws = tmp.path().join("Caso São João — ação 1");
        fs::create_dir_all(&ws).unwrap();
        (tmp, ws)
    }

    fn fixture(nome: &str) -> String {
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edicao").join(nome)).unwrap()
    }

    fn por(ws: &Path, id: &str, texto: &str) -> PathBuf {
        let pasta = pasta_projeto(ws, id);
        fs::create_dir_all(&pasta).unwrap();
        fs::write(pasta.join(PROJETO), texto).unwrap();
        pasta
    }

    fn ler(arq: &Path) -> Value {
        serde_json::from_slice(&fs::read(arq).unwrap()).unwrap()
    }

    /// Igualdade de valores bit a bit: número com o mesmo tipo e os mesmos bits; texto e chaves iguais.
    fn bit_a_bit(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => {
                x.is_f64() == y.is_f64()
                    && x.as_u64() == y.as_u64()
                    && x.as_i64() == y.as_i64()
                    && x.as_f64().map(f64::to_bits) == y.as_f64().map(f64::to_bits)
            }
            (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| bit_a_bit(x, y)),
            (Value::Object(x), Value::Object(y)) => {
                x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| bit_a_bit(v, w)))
            }
            _ => a == b,
        }
    }

    fn sem_tmp(pasta: &Path) -> bool {
        fs::read_dir(pasta).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().ends_with(".tmp"))
    }

    #[test]
    fn id_e_pasta() {
        assert!(id_valido(MINIMO) && id_valido("p1") && id_valido(&"a".repeat(80)));
        for ruim in ["", "../x", "a/b", "a b", "ação", &"a".repeat(81), "a.json"] {
            assert!(!id_valido(ruim), "{ruim}");
        }
        let (_t, ws) = caso();
        assert!(ws.to_string_lossy().contains("São João — ação"));
        assert_eq!(pasta_projeto(&ws, "p1"), ws.join("Sistema/videos/edicao/p1"));
        assert!(abrir(&ws, "../fora").is_err());
        assert!(salvar(&ws, &json!({ "esquema": 2, "id": "../fora" })).is_err());
    }

    #[test]
    fn tipado_devolve_o_codigo() {
        let mut v: Value = serde_json::from_str(&fixture("projeto-minimo.json")).unwrap();
        assert_eq!(tipado(&v).unwrap().id, MINIMO);
        v["esquema"] = json!(3);
        assert!(matches!(tipado(&v), Err(SicroError::Validation(c)) if c == "esquema"));
        v["esquema"] = json!(2);
        v.as_object_mut().unwrap().remove("saida");
        assert!(matches!(tipado(&v), Err(SicroError::Validation(c)) if c.starts_with("item_invalido: ") && c.contains("saida")));
    }

    #[test]
    fn listar_v2_v1_e_ilegivel() {
        let (_t, ws) = caso();
        assert!(listar(&ws).unwrap().is_empty(), "caso sem as pastas lista vazio");
        por(&ws, MINIMO, &fixture("projeto-minimo.json"));
        por(&ws, DESCONHECIDO, &fixture("projeto-desconhecido.json"));
        por(&ws, ILEGIVEL, &fixture("projeto-ilegivel.json"));
        // excluído com exportação: só `exportacoes/`
        fs::create_dir_all(pasta_projeto(&ws, "excluido").join("exportacoes/s1")).unwrap();
        // nome que não é id
        fs::create_dir_all(paths::dir_edicao(&ws).join("não é id")).unwrap();
        fs::write(paths::dir_edicao(&ws).join("não é id").join(PROJETO), "{}").unwrap();
        let montagens = paths::dir_montagens_json(&ws);
        fs::create_dir_all(&montagens).unwrap();
        fs::write(montagens.join("a0000000-0000-4000-8000-000000000001.json"), fixture("v1-velocidade.json")).unwrap();
        // v1 com v2 do mesmo id (renomear falhou): vale o v2
        fs::write(montagens.join(format!("{MINIMO}.json")), fixture("v1-antigo.json")).unwrap();
        fs::write(montagens.join("x.json.v1"), "{}").unwrap();

        let l = listar(&ws).unwrap();
        let ids: Vec<&str> = l.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids.len(), 4, "{ids:?}");
        let r = |id: &str| l.iter().find(|r| r.id == id).unwrap().clone();
        let ileg = r(ILEGIVEL);
        assert_eq!(ileg.problema.as_deref(), Some("esquema"));
        assert_eq!((ileg.titulo.as_str(), ileg.esquema, ileg.n_clipes), ("", 0, 0));
        assert!(!ileg.updated_at.is_empty(), "data do arquivo");
        let min = r(MINIMO);
        assert_eq!((min.problema, min.esquema, min.titulo.as_str(), min.n_clipes), (None, 2, "Mínimo", 0));
        let desc = r(DESCONHECIDO);
        assert_eq!((desc.problema, desc.n_clipes), (None, 2));
        assert!((desc.duracao_s - 2.5).abs() < 1e-9, "{}", desc.duracao_s);
        let v1 = r("a0000000-0000-4000-8000-000000000001");
        assert_eq!((v1.esquema, v1.n_clipes, v1.problema), (1, 3, None));
        assert!((v1.duracao_s - 7.5).abs() < 1e-12);
        // mais recente primeiro
        let datas: Vec<&str> = l.iter().map(|r| r.updated_at.as_str()).collect();
        assert!(datas.windows(2).all(|w| w[0] >= w[1]), "{datas:?}");
        // miniatura
        fs::write(pasta_projeto(&ws, MINIMO).join(MINIATURA), b"jpg").unwrap();
        assert_eq!(
            listar(&ws).unwrap().into_iter().find(|r| r.id == MINIMO).unwrap().miniatura_rel.as_deref(),
            Some(format!("Sistema/videos/edicao/{MINIMO}/miniatura.jpg").as_str())
        );
    }

    #[test]
    fn listar_tipado_invalido() {
        let (_t, ws) = caso();
        let mut v: Value = serde_json::from_str(&fixture("projeto-minimo.json")).unwrap();
        v.as_object_mut().unwrap().remove("saida");
        por(&ws, MINIMO, &v.to_string());
        let l = listar(&ws).unwrap();
        assert_eq!(l[0].problema.as_deref(), Some("item_invalido"));
        assert_eq!(l[0].titulo, "Mínimo");
        let a = abrir(&ws, MINIMO).unwrap();
        assert!(a.so_leitura);
        assert_eq!(a.avisos[0].codigo, "item_invalido");
        assert!(a.avisos[0].mensagem.contains("saida"), "{}", a.avisos[0].mensagem);
    }

    #[test]
    fn duracao_estimada() {
        let mut v: Value = serde_json::from_str(&fixture("projeto-desconhecido.json")).unwrap();
        let mut c = v["principal"][0].clone();
        c["inicio_s"] = json!(0.0);
        c["fim_s"] = json!(4.0);
        c["velocidade"] = json!({ "tipo": "rampa", "pontos": [ { "t": 0.0, "v": 1.0 }, { "t": 4.0, "v": 2.0 } ] });
        let mut d = c.clone();
        d["id"] = json!("c2");
        d["velocidade"] = json!({ "tipo": "constante", "fator": 0.5 });
        d["transicao_entrada"] = json!({ "tipo": "fundir", "duracao_s": 1.0, "audio": "acompanhar" });
        let mut e = d.clone();
        e["id"] = json!("c3");
        e["desligado"] = json!(true);
        v["principal"] = json!([c, d, e]);
        let p = tipado(&v).unwrap();
        // ∫ dt/(1 + t/4) de 0 a 4 = 4 ln 2; + 8 a 0,5× − 1 da sobreposição; desligado não conta
        assert!((duracao_programa(&p) - (4.0 * 2f64.ln() + 8.0 - 1.0)).abs() < 1e-9, "{}", duracao_programa(&p));
    }

    #[test]
    fn abrir_preserva_e_acusa_desconhecidos() {
        let (_t, ws) = caso();
        let original: Value = serde_json::from_str(&fixture("projeto-desconhecido.json")).unwrap();
        por(&ws, DESCONHECIDO, &fixture("projeto-desconhecido.json"));
        let a = abrir(&ws, DESCONHECIDO).unwrap();
        assert!(!a.so_leitura);
        assert_eq!(a.importado_de, None);
        assert_eq!(a.raiz_cache, "");
        let achado = |id: &str| a.avisos.iter().find(|p| p.efeito.as_deref() == Some(id) || (p.efeito.is_none() && p.clipe.as_deref() == Some(id))).unwrap();
        let e9 = achado("e9");
        assert_eq!((e9.codigo, e9.nivel, e9.clipe.as_deref()), ("item_desconhecido", Nivel::Erro, Some("c1")));
        let e8 = achado("e8");
        assert_eq!((e8.codigo, e8.nivel), ("item_invalido", Nivel::Aviso));
        assert!(e8.mensagem.contains("quantidade"), "{}", e8.mensagem);
        let c9 = achado("c9");
        assert_eq!((c9.codigo, c9.nivel), ("item_desconhecido", Nivel::Aviso));
        let l9 = achado("l9");
        assert_eq!((l9.codigo, l9.nivel, l9.trilha.as_deref()), ("item_desconhecido", Nivel::Erro, Some("t1")));
        assert_eq!(a.avisos.len(), 4);

        // regravado pelo armazém e pela leitura tipada: os itens do futuro voltam iguais, bit a bit
        salvar(&ws, &a.projeto).unwrap();
        let gravado = ler(&pasta_projeto(&ws, DESCONHECIDO).join(PROJETO));
        let tipado_e_volta = serde_json::to_value(tipado(&gravado).unwrap()).unwrap();
        for doc in [&gravado, &tipado_e_volta] {
            for ponteiro in ["/principal/0/efeitos/1", "/principal/0/efeitos/2", "/principal/1", "/sobreposicoes/0/clipes/0"] {
                let (x, y) = (original.pointer(ponteiro).unwrap(), doc.pointer(ponteiro).unwrap());
                assert!(bit_a_bit(x, y), "{ponteiro}: {x} ≠ {y}");
                assert_eq!(serde_json::to_string(x).unwrap(), serde_json::to_string(y).unwrap());
            }
        }
        // o resto também (fora o carimbo)
        let mut sem_carimbo = gravado.clone();
        sem_carimbo["updated_at"] = original["updated_at"].clone();
        sem_carimbo["gravado_por"] = original["gravado_por"].clone();
        assert!(bit_a_bit(&sem_carimbo, &original));
    }

    #[test]
    fn esquema_menor_maior_abre_so_para_leitura() {
        let (_t, ws) = caso();
        por(&ws, FUTURO, &fixture("projeto-futuro.json"));
        let a = abrir(&ws, FUTURO).unwrap();
        assert!(a.so_leitura);
        assert!(a.avisos.iter().any(|p| p.codigo == "esquema_menor" && p.nivel == Nivel::Erro));
        // o campo novo segue no documento devolvido
        assert_eq!(a.projeto["principal"][0]["campo_novo"]["fator"], json!(0.41952481325285884));
        let antes = fs::read(pasta_projeto(&ws, FUTURO).join(PROJETO)).unwrap();
        assert!(matches!(salvar(&ws, &a.projeto), Err(SicroError::Validation(c)) if c == "esquema_menor"));
        assert!(matches!(duplicar(&ws, FUTURO), Err(SicroError::Validation(c)) if c == "esquema_menor"));
        assert_eq!(fs::read(pasta_projeto(&ws, FUTURO).join(PROJETO)).unwrap(), antes);
        // esquema ≠ 2 também
        let mut v: Value = serde_json::from_str(&fixture("projeto-minimo.json")).unwrap();
        v["esquema"] = json!(3);
        por(&ws, MINIMO, &v.to_string());
        let a = abrir(&ws, MINIMO).unwrap();
        assert!(a.so_leitura && a.avisos.iter().any(|p| p.codigo == "esquema"));
        // id do documento ≠ pasta
        v["esquema"] = json!(2);
        v["id"] = json!("outro");
        por(&ws, MINIMO, &v.to_string());
        let a = abrir(&ws, MINIMO).unwrap();
        assert!(a.so_leitura && a.avisos.iter().any(|p| p.codigo == "esquema" && p.campo.as_deref() == Some("id")));
    }

    #[test]
    fn ilegivel_nao_abre() {
        let (_t, ws) = caso();
        por(&ws, ILEGIVEL, &fixture("projeto-ilegivel.json"));
        assert!(matches!(abrir(&ws, ILEGIVEL), Err(SicroError::Validation(c)) if c.starts_with("esquema")));
        assert!(abrir(&ws, MINIMO).is_err(), "inexistente");
    }

    #[test]
    fn anterior_antes_da_primeira_gravacao_da_sessao() {
        let (_t, ws) = caso();
        let pasta = por(&ws, MINIMO, &fixture("projeto-minimo.json"));
        let original = fs::read(pasta.join(PROJETO)).unwrap();
        let a = abrir(&ws, MINIMO).unwrap();
        assert_eq!(fs::read(pasta.join(ANTERIOR)).unwrap(), original);
        let mut doc = a.projeto.clone();
        doc["titulo"] = json!("Primeira");
        salvar(&ws, &doc).unwrap();
        doc["titulo"] = json!("Segunda");
        salvar(&ws, &doc).unwrap();
        assert_eq!(fs::read(pasta.join(ANTERIOR)).unwrap(), original, "a anterior é a da abertura");
        assert_eq!(ler(&pasta.join(PROJETO))["titulo"], "Segunda");
        // reabrir = sessão nova
        abrir(&ws, MINIMO).unwrap();
        assert_eq!(ler(&pasta.join(ANTERIOR))["titulo"], "Segunda");

        // salvar sem abrir um projeto que já existe: a anterior sai do disco antes de gravar
        let pasta2 = por(&ws, DESCONHECIDO, &fixture("projeto-desconhecido.json"));
        let disco = fs::read(pasta2.join(PROJETO)).unwrap();
        let mut v: Value = serde_json::from_slice(&disco).unwrap();
        v["titulo"] = json!("Mudou");
        salvar(&ws, &v).unwrap();
        assert_eq!(fs::read(pasta2.join(ANTERIOR)).unwrap(), disco);

        // projeto novo: nada a guardar
        let mut novo: Value = serde_json::from_str(&fixture("projeto-minimo.json")).unwrap();
        novo["id"] = json!("novo-1");
        salvar(&ws, &novo).unwrap();
        salvar(&ws, &novo).unwrap();
        assert!(!pasta_projeto(&ws, "novo-1").join(ANTERIOR).exists());
    }

    #[test]
    fn salvar_grava_como_chegou_e_carimba() {
        let (_t, ws) = caso();
        assert!(!paths::dir_edicao(&ws).exists(), "caso antigo, sem a pasta");
        let mut v: Value = serde_json::from_str(&fixture("projeto-minimo.json")).unwrap();
        v["campo_de_cima_do_futuro"] = json!({ "x": [1, 2.5, "três"] });
        let s = salvar(&ws, &v).unwrap();
        let pasta = pasta_projeto(&ws, MINIMO);
        let gravado = ler(&pasta.join(PROJETO));
        assert_eq!(gravado["updated_at"], json!(s.updated_at));
        assert!(DateTime::parse_from_rfc3339(&s.updated_at).is_ok());
        assert_eq!(gravado["gravado_por"], json!(APP_VERSION));
        let mut esperado = v.clone();
        esperado["updated_at"] = json!(s.updated_at);
        esperado["gravado_por"] = json!(APP_VERSION);
        assert!(bit_a_bit(&gravado, &esperado));
        assert!(sem_tmp(&pasta));
        for ruim in [json!([]), json!({ "esquema": 2 }), json!({ "esquema": 1, "id": "p1" })] {
            assert!(salvar(&ws, &ruim).is_err(), "{ruim}");
        }
    }

    /// Classes do E8 (`critica-pericia/e8_saida.txt`): sem `float_roundtrip` 315 496 de 3 000 000 coordenadas mudavam.
    #[test]
    fn numeros_iguais_depois_de_regravar() {
        const N: usize = 100_000;
        let mut s: u64 = 0x9E3779B97F4A7C15;
        let mut prox = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let (mut tempo, mut coord, mut epoca) = (Vec::with_capacity(N), Vec::with_capacity(N), Vec::with_capacity(N));
        for _ in 0..N {
            let u = (prox() >> 11) as f64 / (1u64 << 53) as f64;
            tempo.push(format!("{:.6}", u * 7200.0).parse::<f64>().unwrap());
            coord.push(u);
            epoca.push(format!("{:.6}", 1505762700.0 + u * 600.0).parse::<f64>().unwrap());
        }
        let exemplos = [
            0.9856906946328695, 0.21291890726713458, 0.9259338926496359, 0.44166130716816643, 0.9726104788033849,
            0.41952481325285884, 0.9395774263991487,
        ];
        let mut v: Value = serde_json::from_str(&fixture("projeto-desconhecido.json")).unwrap();
        v["principal"][0]["efeitos"][1]["numeros"] = json!({ "tempo": tempo, "coord": coord, "epoca": epoca, "e8": exemplos });
        // e pelo caminho tipado: âncoras com tempo de época e coordenadas cheias
        let ancoras: Vec<Value> = (0..N / 10)
            .map(|i| json!({ "t": epoca[i], "pts": null, "v": { "x": coord[i], "y": coord[N - 1 - i], "w": exemplos[i % 7], "h": tempo[i] / 7200.0 }, "interp": "linear" }))
            .collect();
        v["rastreios"] = json!([{ "id": "r1", "nome": "", "cor": "#ff0000", "midia": "m1", "base": "fonte", "tipo": "caixa",
            "espaco": "original", "ancoras": ancoras, "denso": null, "silhuetas": null, "ausencias": [], "origem": { "tipo": "manual" },
            "confirmacao": null }]);
        let texto = serde_json::to_string(&v).unwrap();
        for e in exemplos {
            assert!(texto.contains(&e.to_string()), "{e}");
        }
        let (_t, ws) = caso();
        por(&ws, DESCONHECIDO, &texto);
        let a = abrir(&ws, DESCONHECIDO).unwrap();
        salvar(&ws, &a.projeto).unwrap();
        let regravado_txt = fs::read_to_string(pasta_projeto(&ws, DESCONHECIDO).join(PROJETO)).unwrap();
        let regravado: Value = serde_json::from_str(&regravado_txt).unwrap();
        let antes = &v["principal"][0]["efeitos"][1]["numeros"];
        let depois = &regravado["principal"][0]["efeitos"][1]["numeros"];
        assert!(bit_a_bit(antes, depois));
        assert_eq!(serde_json::to_string(antes).unwrap(), serde_json::to_string(depois).unwrap(), "o texto dos números mudou");
        for e in exemplos {
            assert!(regravado_txt.contains(&format!("{e}")), "{e} sumiu do texto");
        }
        assert!(bit_a_bit(&v["rastreios"], &regravado["rastreios"]));
        // leitura tipada e volta: mesmos bits nas âncoras
        let p = tipado(&regravado).unwrap();
        for (i, k) in p.rastreios[0].ancoras.iter().enumerate() {
            assert_eq!(k.t.to_bits(), epoca[i].to_bits());
            assert_eq!(k.v.x.to_bits(), coord[i].to_bits());
            assert_eq!(k.v.y.to_bits(), coord[N - 1 - i].to_bits());
        }
        assert!(bit_a_bit(&serde_json::to_value(&p).unwrap()["rastreios"], &v["rastreios"]));
    }

    #[test]
    fn gravacao_atomica_nao_corrompe_o_anterior() {
        let (_t, ws) = caso();
        let pasta = por(&ws, MINIMO, &fixture("projeto-minimo.json"));
        let arq = pasta.join(PROJETO);
        let antes = fs::read(&arq).unwrap();
        // falha no meio da escrita
        let r = gravar_atomico_com(&arq, |f| {
            f.write_all(b"{\"esquema\": 2, \"id\": \"cort")?;
            Err(io::Error::other("disco cheio"))
        });
        assert!(r.is_err());
        assert_eq!(fs::read(&arq).unwrap(), antes);
        assert!(sem_tmp(&pasta));
        // `.tmp` velho de uma queda anterior não atrapalha
        fs::write(pasta.join("projeto.json.tmp"), b"lixo de uma queda").unwrap();
        let mut v: Value = serde_json::from_slice(&antes).unwrap();
        v["titulo"] = json!("Depois da queda");
        salvar(&ws, &v).unwrap();
        assert_eq!(ler(&arq)["titulo"], "Depois da queda");
        assert!(sem_tmp(&pasta));
        // destino que não dá para renomear (é uma pasta): o arquivo de antes fica
        let bloqueado = pasta.join("bloqueado.json");
        fs::create_dir_all(bloqueado.join("dentro")).unwrap();
        assert!(gravar_atomico(&bloqueado, b"{}").is_err());
        assert!(bloqueado.join("dentro").is_dir());
        assert!(sem_tmp(&pasta));
    }

    /// Projeto com tudo que o armazém conhece em disco (o completo.json, id "p1").
    fn projeto_cheio(ws: &Path) -> PathBuf {
        let pasta = por(ws, "p1", &fixture("completo.json"));
        let f = "f".repeat(64);
        for (sub, arq, conteudo) in [
            ("midia", "planta.png", b"png".to_vec()),
            ("midia", "narracao.ogg", b"ogg".to_vec()),
            ("vidstab", &format!("{}.trf", "a".repeat(64)), b"trf".to_vec()),
            ("exportacoes/s1", "projeto.json", fixture("completo.json").into_bytes()),
            ("exportacoes/s1/dados", "x.bin", b"x".to_vec()),
            (".", MINIATURA, b"jpg".to_vec()),
        ] {
            fs::create_dir_all(pasta.join(sub)).unwrap();
            fs::write(pasta.join(sub).join(arq), conteudo).unwrap();
        }
        for rid in ["r-manual", "r-assistido", "r-pessoas", "r-deteccao", "r-local"] {
            fs::create_dir_all(pasta.join("silhuetas").join(rid)).unwrap();
            fs::write(pasta.join("silhuetas").join(rid).join(format!("{}.sil", "1".repeat(64))), b"sil").unwrap();
            let d = json!({ "esquema": 1, "rastreio": rid, "midia_sha256": "2".repeat(64), "espaco": "original",
                            "pixel": { "w": 1920, "h": 1080 }, "pts": [0], "x": [0.5], "y": [0.5], "w": [0.1], "h": [0.1],
                            "conf": [1.0], "estado": [0], "tipo": [0], "corrida": [0], "corridas": [] });
            fs::create_dir_all(pasta.join("rastreios").join(rid)).unwrap();
            fs::write(pasta.join("rastreios").join(rid).join(format!("{f}.json")), serde_json::to_vec(&d).unwrap()).unwrap();
        }
        // órfão de um rastreio que não existe mais
        fs::create_dir_all(pasta.join("rastreios/r-velho")).unwrap();
        fs::write(pasta.join("rastreios/r-velho").join(format!("{}.json", "e".repeat(64))), b"{}").unwrap();
        pasta
    }

    fn ids_do_documento(doc: &Value) -> Vec<String> {
        let mut ids = Vec::new();
        for c in CAMINHOS_DE_ID {
            let mut v = Vec::new();
            strings_em(doc, c, &mut v);
            ids.extend(v.into_iter().map(str::to_string));
        }
        ids
    }

    fn referencias(v: &Value, saida: &mut Vec<String>) {
        match v {
            Value::Object(o) => {
                for (k, x) in o {
                    match x {
                        Value::String(s) if CHAVES_DE_REFERENCIA.contains(&k.as_str()) && k != "id" => saida.push(s.clone()),
                        _ => referencias(x, saida),
                    }
                }
            }
            Value::Array(a) => a.iter().for_each(|x| referencias(x, saida)),
            _ => {}
        }
    }

    #[test]
    fn duplicar_ids_novos_sem_referencia_ao_original() {
        let (_t, ws) = caso();
        let pasta = projeto_cheio(&ws);
        let original = ler(&pasta.join(PROJETO));
        let r = duplicar(&ws, "p1").unwrap();
        assert_ne!(r.id, "p1");
        assert!(id_valido(&r.id) && Uuid::parse_str(&r.id).is_ok());
        assert_eq!((r.titulo.as_str(), r.problema.clone()), (original["titulo"].as_str().unwrap(), None));
        let copia_pasta = pasta_projeto(&ws, &r.id);
        let copia = ler(&copia_pasta.join(PROJETO));
        tipado(&copia).expect("a cópia lê tipada");

        let ids_o = ids_do_documento(&original);
        let ids_c = ids_do_documento(&copia);
        assert_eq!(ids_o.iter().collect::<BTreeSet<_>>().len(), ids_o.len(), "a fixture repete id");
        assert_eq!(ids_o.len(), ids_c.len());
        let unicos: BTreeSet<&String> = ids_c.iter().collect();
        assert_eq!(unicos.len(), ids_c.len(), "id repetido na cópia");
        assert!(ids_c.iter().all(|id| !ids_o.contains(id)), "id do original na cópia");
        assert!(ids_c.iter().all(|id| Uuid::parse_str(id).is_ok()));
        // toda referência da cópia aponta para um id da cópia; nenhuma para o original
        let mut refs = Vec::new();
        referencias(&copia, &mut refs);
        let ids_o: BTreeSet<&String> = ids_o.iter().collect();
        assert!(!refs.is_empty());
        for r in &refs {
            assert!(!ids_o.contains(r), "referência ao original: {r}");
        }
        let mut refs_o = Vec::new();
        referencias(&original, &mut refs_o);
        assert_eq!(refs.len(), refs_o.len());
        for (a, b) in refs_o.iter().zip(&refs) {
            assert_eq!(ids_o.contains(a), unicos.contains(b), "{a} → {b}");
        }
        // mesma forma: trocando de volta os ids, o documento é o original (fora carimbos e caminhos)
        let volta: HashMap<String, String> =
            ids_do_documento(&copia).into_iter().zip(ids_do_documento(&original)).collect();
        let mut desfeito = copia.clone();
        reescrever_referencias(&mut desfeito, &volta);
        for campo in ["id", "created_at", "updated_at", "gravado_por"] {
            desfeito[campo] = original[campo].clone();
        }
        for (i, m) in original["midias"].as_array().unwrap().iter().enumerate() {
            desfeito["midias"][i]["relativo"] = m["relativo"].clone();
        }
        assert!(bit_a_bit(&desfeito, &original));
        // caminhos da mídia própria vão para a pasta nova; nenhum texto cita a pasta velha
        let txt = serde_json::to_string(&copia).unwrap();
        assert!(!txt.contains("Sistema/videos/edicao/p1/"));
        for m in copia["midias"].as_array().unwrap() {
            let rel = m["relativo"].as_str().unwrap();
            if m["origem"] == "projeto" {
                assert!(rel.starts_with(&format!("Sistema/videos/edicao/{}/midia/", r.id)), "{rel}");
                assert!(ws.join(rel).is_file(), "{rel}");
            }
        }
        // arquivos: mídia, .trf, miniatura e silhuetas copiados; exportações não; órfão não
        assert!(copia_pasta.join("vidstab").join(format!("{}.trf", "a".repeat(64))).is_file());
        assert!(copia_pasta.join(MINIATURA).is_file());
        assert!(!copia_pasta.join(EXPORTACOES).exists());
        assert!(!copia_pasta.join(ANTERIOR).exists());
        let rastreios = fs::read_dir(copia_pasta.join("rastreios")).unwrap().count();
        assert_eq!(rastreios, 5, "só os rastreios do documento");
        // denso copiado byte a byte: mesmo nome (SHA-256) e mesmos bytes; ponteiro e confirmação iguais aos do original
        for (o, r) in original["rastreios"].as_array().unwrap().iter().zip(copia["rastreios"].as_array().unwrap()) {
            let (rid_o, rid) = (o["id"].as_str().unwrap(), r["id"].as_str().unwrap());
            let nome = format!("{}.json", r["denso"]["sha256"].as_str().unwrap());
            let bytes = fs::read(copia_pasta.join("rastreios").join(rid).join(&nome)).unwrap_or_else(|_| panic!("denso de {rid}"));
            assert_eq!(bytes, fs::read(pasta.join("rastreios").join(rid_o).join(&nome)).unwrap());
            assert_eq!(r["denso"], o["denso"]);
            assert_eq!(r["confirmacao"]["denso_sha256"], o["confirmacao"]["denso_sha256"]);
            assert!(copia_pasta.join("silhuetas").join(rid).join(format!("{}.sil", "1".repeat(64))).is_file());
        }
        // o original ficou como estava
        assert_eq!(ler(&pasta.join(PROJETO)), original);
        assert!(pasta.join("rastreios/r-assistido").join(format!("{}.json", "f".repeat(64))).is_file());
        assert_eq!(listar(&ws).unwrap().len(), 2);
    }

    #[test]
    fn excluir_sem_exportacao_some_a_pasta() {
        let (t, ws) = caso();
        let pasta = projeto_cheio(&ws);
        fs::remove_dir_all(pasta.join(EXPORTACOES)).unwrap();
        // cache: o render deste projeto sai, o de outro fica
        let cache = t.path().join("cache edição/6f1c2a52-0d5e-4c1b-9a43-2f7d0c1e9b10");
        for (tarefa, id) in [("t1", "p1"), ("t2", "outro")] {
            let d = cache.join("render").join(tarefa).join("projeto");
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join(PROJETO), json!({ "id": id }).to_string()).unwrap();
        }
        fs::create_dir_all(cache.join("blocos")).unwrap();
        fs::write(cache.join("blocos/abc.mkv"), b"x").unwrap();
        abrir(&ws, "p1").unwrap();
        excluir(&ws, "p1", Some(&cache)).unwrap();
        assert!(!pasta.exists());
        assert!(!cache.join("render/t1").exists());
        assert!(cache.join("render/t2").is_dir() && cache.join("blocos/abc.mkv").is_file());
        assert!(listar(&ws).unwrap().is_empty());
        assert!(excluir(&ws, "p1", None).is_err());
    }

    #[test]
    fn excluir_com_exportacao_fica_so_o_retrato() {
        let (_t, ws) = caso();
        let pasta = projeto_cheio(&ws);
        let retrato = fs::read(pasta.join("exportacoes/s1/projeto.json")).unwrap();
        excluir(&ws, "p1", None).unwrap();
        let sobrou: Vec<String> = fs::read_dir(&pasta).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        assert_eq!(sobrou, [EXPORTACOES]);
        assert_eq!(fs::read(pasta.join("exportacoes/s1/projeto.json")).unwrap(), retrato);
        assert!(pasta.join("exportacoes/s1/dados/x.bin").is_file());
        assert!(listar(&ws).unwrap().is_empty(), "projeto excluído não volta à lista");
        assert!(abrir(&ws, "p1").is_err());
        // a exportação continua achando a mídia
        let u = usos(&ws, &"2".repeat(64)).unwrap();
        assert_eq!((u.len(), u[0].saidas, u[0].n_clipes), (1, 1, 0));
    }

    #[test]
    fn excluir_v1_e_auditar() {
        let tmp = tempfile::Builder::new().prefix("sicro-p02-").tempdir().unwrap();
        let entrada = crate::models::NewOccurrenceInput { titulo: Some("Homicídio São José".into()), ..Default::default() };
        let ws = crate::workspace::create::create_workspace(entrada, tmp.path()).unwrap().path;
        let montagens = paths::dir_montagens_json(&ws);
        fs::create_dir_all(&montagens).unwrap();
        let id = "a0000000-0000-4000-8000-000000000001";
        fs::write(montagens.join(format!("{id}.json")), fixture("v1-velocidade.json")).unwrap();
        excluir(&ws, id, None).unwrap();
        assert!(!montagens.join(format!("{id}.json")).exists());
        por(&ws, MINIMO, &fixture("projeto-minimo.json"));
        excluir(&ws, MINIMO, None).unwrap();
        let conn = crate::database::connection::open_connection(&paths::sqlite_path(&ws)).unwrap();
        let linhas: Vec<(String, Option<String>, String)> = conn
            .prepare("SELECT action, entity_id, details_json FROM audit_logs WHERE action = 'video.edicao.projeto_excluido' ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(linhas.len(), 2);
        assert_eq!(linhas[1].1.as_deref(), Some(MINIMO));
        assert!(linhas[1].2.contains("Mínimo"));
    }

    #[test]
    fn midia_do_projeto() {
        let (t, ws) = caso();
        let origem = t.path().join("narração do perito.WAV");
        let dados: Vec<u8> = (0..300_000u32).map(|i| (i * 31 % 251) as u8).collect();
        fs::write(&origem, &dados).unwrap();
        let m = importar_midia(&ws, MINIMO, &origem, TipoMidia::Audio).unwrap();
        assert_eq!((m.tipo, m.origem, m.media_id.clone(), m.nome.as_str()), (TipoMidia::Audio, OrigemMidia::Projeto, None, "narração do perito.WAV"));
        assert!(Uuid::parse_str(&m.id).is_ok());
        assert_eq!(m.relativo, format!("Sistema/videos/edicao/{MINIMO}/midia/{}.wav", m.id));
        assert_eq!(m.sha256, crate::hashing::sha256::sha256_bytes(&dados));
        assert_eq!(fs::read(ws.join(&m.relativo)).unwrap(), dados);
        assert!(sem_tmp(&pasta_projeto(&ws, MINIMO).join(MIDIA)));
        assert!(m.ocultacoes.is_empty() && m.derivacao.is_none());
        // imagem; extensão fora da lista; vídeo não entra como mídia própria
        let png = t.path().join("logo.png");
        fs::write(&png, b"\x89PNG").unwrap();
        assert_eq!(importar_midia(&ws, MINIMO, &png, TipoMidia::Imagem).unwrap().tipo, TipoMidia::Imagem);
        assert!(importar_midia(&ws, MINIMO, &png, TipoMidia::Audio).is_err());
        assert!(importar_midia(&ws, MINIMO, &origem, TipoMidia::Video).is_err());
        assert!(importar_midia(&ws, "../x", &origem, TipoMidia::Audio).is_err());

        let g = gravar_midia(&ws, MINIMO, "audio/webm;codecs=opus", b"webm").unwrap();
        assert!(g.relativo.ends_with(".webm") && g.nome.ends_with(".webm"));
        assert_eq!(g.sha256, crate::hashing::sha256::sha256_bytes(b"webm"));
        assert_eq!(gravar_midia(&ws, MINIMO, "Audio/MP4", b"m").unwrap().relativo.rsplit('.').next(), Some("m4a"));
        assert!(gravar_midia(&ws, MINIMO, "video/x-msvideo", b"x").is_err());
        assert!(gravar_midia(&ws, MINIMO, "audio/ogg", b"").is_err());
        assert_eq!(fs::read_dir(pasta_projeto(&ws, MINIMO).join(MIDIA)).unwrap().count(), 4);
    }

    #[test]
    fn usos_de_uma_midia() {
        let (_t, ws) = caso();
        let x = "2".repeat(64);
        projeto_cheio(&ws); // p1: m1 = x em vários clipes, retrato s1 também
        por(&ws, MINIMO, &fixture("projeto-minimo.json")); // não usa
        // projeto excluído com duas exportações, uma cita x
        let exc = pasta_projeto(&ws, "excluido");
        for (s, sha) in [("s1", x.clone()), ("s2", "9".repeat(64))] {
            fs::create_dir_all(exc.join("exportacoes").join(s)).unwrap();
            let doc = json!({ "titulo": "Excluído", "midias": [{ "id": "m", "sha256": sha }] });
            fs::write(exc.join("exportacoes").join(s).join(PROJETO), doc.to_string()).unwrap();
        }
        let u = usos(&ws, &x).unwrap();
        assert_eq!(u.len(), 2, "{u:?}");
        let p1 = u.iter().find(|u| u.projeto_id == "p1").unwrap();
        let doc = ler(&pasta_projeto(&ws, "p1").join(PROJETO));
        let esperado = doc["principal"].as_array().unwrap().iter()
            .chain(doc["sobreposicoes"].as_array().unwrap().iter().flat_map(|t| t["clipes"].as_array().unwrap()))
            .chain(doc["audios"].as_array().unwrap().iter().flat_map(|t| t["clipes"].as_array().unwrap()))
            .filter(|c| c.to_string().contains("\"midia\":\"m1\""))
            .count() as u32;
        assert!(esperado > 0);
        assert_eq!((p1.n_clipes, p1.saidas), (esperado, 1));
        let e = u.iter().find(|u| u.projeto_id == "excluido").unwrap();
        assert_eq!((e.titulo.as_str(), e.n_clipes, e.saidas), ("Excluído", 0, 1));
        assert!(usos(&ws, &"0".repeat(64)).unwrap().is_empty());
    }

    #[test]
    fn limpar_orfaos_so_o_que_ninguem_cita() {
        let (_t, ws) = caso();
        let pasta = projeto_cheio(&ws);
        // SHA-256 que não aparecem em texto nenhum da fixture (o retrato é o completo.json inteiro)
        let h = |par: &str| par.repeat(32);
        let arq = |sub: &str, sha: &str, ext: &str| pasta.join(sub).join(format!("{sha}.{ext}"));
        let mut doc = ler(&pasta.join(PROJETO));
        // r-deteccao passa a outro denso; o velho (f…f) segue vivo pelo histórico (`manter`)
        doc["rastreios"][3]["denso"]["sha256"] = json!(h("d1"));
        doc["rastreios"][3]["confirmacao"]["denso_sha256"] = json!(h("d1"));
        fs::write(arq("rastreios/r-deteccao", &h("d1"), "json"), b"{}").unwrap();
        // um denso citado só por um item do futuro; um só pelo retrato; dois sem ninguém
        doc["principal"][0]["efeitos"].as_array_mut().unwrap().push(json!({ "tipo": "efeito_do_futuro", "id": "z", "arquivo": h("c2") }));
        fs::write(arq("rastreios/r-assistido", &h("c2"), "json"), b"{}").unwrap();
        let mut retrato = ler(&pasta.join("exportacoes/s1/projeto.json"));
        retrato["rastreios"][0]["denso"]["sha256"] = json!(h("b3"));
        fs::write(pasta.join("exportacoes/s1/projeto.json"), retrato.to_string()).unwrap();
        fs::write(arq("rastreios/r-assistido", &h("b3"), "json"), b"{}").unwrap();
        fs::write(arq("rastreios/r-assistido", &h("a4"), "json"), b"{}").unwrap();
        fs::write(arq("vidstab", &h("e5"), "trf"), b"trf").unwrap();
        fs::create_dir_all(pasta.join("silhuetas/r-velho")).unwrap();
        fs::write(arq("silhuetas/r-velho", &h("f6"), "sil"), b"sil").unwrap();
        // outro tipo de arquivo na pasta não é da limpeza
        fs::write(pasta.join("rastreios/r-assistido/notas.txt"), b"x").unwrap();
        let p = tipado(&doc).unwrap();
        let f = "f".repeat(64);
        let apagados = limpar_orfaos(&ws, &p, &[f.clone()]).unwrap();
        // a4.json, e…e.json (r-velho), e5.trf, f6.sil
        assert_eq!(apagados, 4);
        for (sub, sha, ext) in [
            ("rastreios/r-assistido", f.clone(), "json"), ("rastreios/r-deteccao", f.clone(), "json"),
            ("rastreios/r-deteccao", h("d1"), "json"), ("rastreios/r-assistido", h("c2"), "json"),
            ("rastreios/r-assistido", h("b3"), "json"), ("vidstab", "a".repeat(64), "trf"), ("silhuetas/r-assistido", "1".repeat(64), "sil"),
        ] {
            assert!(arq(sub, &sha, ext).is_file(), "{sub}/{sha}");
        }
        assert!(pasta.join("rastreios/r-assistido/notas.txt").is_file());
        assert!(!arq("rastreios/r-assistido", &h("a4"), "json").exists());
        assert!(!pasta.join("rastreios/r-velho").exists() && !pasta.join("silhuetas/r-velho").exists());
        assert!(!arq("vidstab", &h("e5"), "trf").exists());
        assert_eq!(limpar_orfaos(&ws, &p, &[f]).unwrap(), 0);
        // sem o histórico, o f…f de r-deteccao ainda vale (r-assistido o cita); o d1 também
        assert_eq!(limpar_orfaos(&ws, &p, &[]).unwrap(), 0);
    }

    // ---- revisão adversa ----

    fn todos_os_textos(v: &Value, onde: &str, saida: &mut Vec<(String, String)>) {
        match v {
            Value::String(s) => saida.push((onde.to_string(), s.clone())),
            Value::Array(a) => a.iter().enumerate().for_each(|(i, x)| todos_os_textos(x, &format!("{onde}/{i}"), saida)),
            Value::Object(o) => o.iter().for_each(|(k, x)| todos_os_textos(x, &format!("{onde}/{k}"), saida)),
            _ => {}
        }
    }

    /// Não usa `CHAVES_DE_REFERENCIA`: ids trocados por UUIDs e procurados em TODO texto da cópia, em qualquer chave.
    #[test]
    fn duplicar_nenhum_texto_da_copia_e_id_do_original() {
        fn ids_por_chave(v: &Value, onde: &str, saida: &mut Vec<(String, String)>) {
            match v {
                Value::Object(o) => {
                    for (k, x) in o {
                        let aqui = format!("{onde}/{k}");
                        match x {
                            Value::String(s) if k == "id" => saida.push((aqui, s.clone())),
                            _ => ids_por_chave(x, &aqui, saida),
                        }
                    }
                }
                Value::Array(a) => a.iter().enumerate().for_each(|(i, x)| ids_por_chave(x, &format!("{onde}/{i}"), saida)),
                _ => {}
            }
        }
        fn de_fora(onde: &str) -> bool {
            onde.ends_with("/media_id") || onde.ends_with("/calibracao/id") || onde.ends_with("/calculo/id")
        }
        fn trocar(v: &mut Value, onde: &str, troca: &HashMap<String, String>) {
            match v {
                Value::String(s) => {
                    if let Some(n) = troca.get(s.as_str()).filter(|_| !de_fora(onde)) {
                        *s = n.clone();
                    }
                }
                Value::Array(a) => a.iter_mut().enumerate().for_each(|(i, x)| trocar(x, &format!("{onde}/{i}"), troca)),
                Value::Object(o) => o.iter_mut().for_each(|(k, x)| trocar(x, &format!("{onde}/{k}"), troca)),
                _ => {}
            }
        }
        let (_t, ws) = caso();
        let mut doc: Value = serde_json::from_str(&fixture("completo.json")).unwrap();
        // a fixture como está: o marcador "k1" tem o mesmo texto do `calibracao.id`, que é de fora e não pode mudar
        assert_eq!(doc["marcadores"][0]["id"], json!("k1"));
        por(&ws, "p0", &doc.to_string().replace("\"id\":\"p1\"", "\"id\":\"p0\""));
        let r0 = duplicar(&ws, "p0").unwrap();
        let copia0 = ler(&pasta_projeto(&ws, &r0.id).join(PROJETO));
        let (mut t0, mut t0_o) = (Vec::new(), Vec::new());
        todos_os_textos(&copia0, "", &mut t0);
        todos_os_textos(&doc, "", &mut t0_o);
        let fora: Vec<_> = t0_o.iter().zip(&t0).filter(|(a, _)| de_fora(&a.0)).collect();
        assert!(fora.iter().any(|(a, _)| a.1 == "k1") && fora.len() >= 7);
        for (a, b) in fora {
            assert_eq!(a, b, "id de fora reescrito");
        }
        assert_ne!(copia0["marcadores"][0]["id"], json!("k1"));

        let mut todos = Vec::new();
        ids_por_chave(&doc, "", &mut todos);
        // `calibracao.id` e `calculo.id` vêm de fora do projeto; o id do projeto é tratado à parte
        let pela_chave: BTreeSet<String> = todos
            .iter()
            .filter(|(onde, _)| onde != "/id" && !onde.ends_with("/calibracao/id") && !onde.ends_with("/calculo/id"))
            .map(|(_, id)| id.clone())
            .collect();
        let pelos_caminhos: BTreeSet<String> = ids_do_documento(&doc).into_iter().collect();
        assert_eq!(pelos_caminhos, pela_chave, "CAMINHOS_DE_ID não cobre todo `id` do documento");
        let troca: HashMap<String, String> = pela_chave.iter().map(|id| (id.clone(), Uuid::new_v4().to_string())).collect();
        trocar(&mut doc, "", &troca);
        let originais: BTreeSet<&String> = troca.values().collect();
        por(&ws, "p1", &doc.to_string());
        let r = duplicar(&ws, "p1").unwrap();
        let copia = ler(&pasta_projeto(&ws, &r.id).join(PROJETO));
        let (mut textos, mut textos_o) = (Vec::new(), Vec::new());
        todos_os_textos(&copia, "", &mut textos);
        todos_os_textos(&doc, "", &mut textos_o);
        let sobrou: Vec<&(String, String)> = textos.iter().filter(|(_, s)| originais.contains(s)).collect();
        assert!(sobrou.is_empty(), "id do original na cópia: {sobrou:?}");
        assert_eq!(textos.len(), textos_o.len());
        assert!(textos_o.iter().filter(|(_, s)| originais.contains(s)).count() > pela_chave.len(), "a fixture tem referências");
        for (a, b) in textos_o.iter().zip(&textos).filter(|(a, _)| de_fora(&a.0)) {
            assert_eq!(a, b);
        }
        tipado(&copia).expect("a cópia lê tipada");
    }

    /// Itens do futuro vão para a cópia com a carga igual, bit a bit, e com as referências de dentro deles trocadas.
    #[test]
    fn duplicar_preserva_desconhecidos_e_troca_as_referencias_deles() {
        let (_t, ws) = caso();
        let mut doc: Value = serde_json::from_str(&fixture("projeto-desconhecido.json")).unwrap();
        doc["principal"][1]["midia"] = json!("m1");
        doc["principal"][0]["efeitos"][1]["alvo"] = json!({ "clipe": "c1", "nota": "m1" });
        doc["sobreposicoes"][0]["clipes"][0]["pos"] = json!({ "ref": "clipe", "clipe": "c1", "desloc": 0.5 });
        doc["campo_de_cima_do_futuro"] = json!({ "midia": "m1", "v": 0.9395774263991487 });
        por(&ws, DESCONHECIDO, &doc.to_string());
        let r = duplicar(&ws, DESCONHECIDO).unwrap();
        let copia = ler(&pasta_projeto(&ws, &r.id).join(PROJETO));
        let em = |p: &str| copia.pointer(p).unwrap_or_else(|| panic!("{p}")).as_str().unwrap().to_string();
        let (m1, c1) = (em("/midias/0/id"), em("/principal/0/id"));
        assert!(m1 != "m1" && c1 != "c1");
        assert_eq!(em("/principal/1/midia"), m1);
        assert_eq!(em("/principal/0/efeitos/1/alvo/clipe"), c1);
        assert_eq!(em("/sobreposicoes/0/clipes/0/pos/clipe"), c1);
        assert_eq!(em("/campo_de_cima_do_futuro/midia"), m1);
        assert_eq!(em("/principal/0/efeitos/1/alvo/nota"), "m1", "texto fora das chaves de referência não é id");
        assert_eq!((em("/principal/0/efeitos/1/tipo"), em("/principal/1/tipo")), ("efeito_do_futuro".into(), "clipe_do_futuro".into()));
        for p in ["/principal/0/efeitos/1/parametros", "/principal/1/carga", "/sobreposicoes/0/clipes/0/valor", "/campo_de_cima_do_futuro/v",
                  "/midias/0/inicio_s", "/principal/0/inicio_s"] {
            let (x, y) = (doc.pointer(p).unwrap(), copia.pointer(p).unwrap());
            assert!(bit_a_bit(x, y), "{p}: {x} ≠ {y}");
            assert_eq!(x.to_string(), y.to_string(), "{p}");
        }
        assert_eq!(abrir(&ws, &r.id).unwrap().avisos.len(), 4, "os mesmos quatro itens acusados");
    }

    /// Id de documento com cara de caminho não vira caminho; pasta de rastreio só pelo id de um rastreio.
    #[test]
    fn duplicar_nao_segue_id_com_cara_de_caminho() {
        fn lista(p: &Path, saida: &mut Vec<PathBuf>) {
            for e in fs::read_dir(p).unwrap().flatten() {
                if e.path().is_dir() {
                    lista(&e.path(), saida)
                } else {
                    saida.push(e.path())
                }
            }
        }
        let (t, ws) = caso();
        let fora = t.path().join("fora do caso");
        fs::create_dir_all(&fora).unwrap();
        fs::write(fora.join("segredo.sil"), b"de fora").unwrap();
        fs::write(fora.join("segredo.json"), b"{}").unwrap();
        let mut doc: Value = serde_json::from_str(&fixture("projeto-desconhecido.json")).unwrap();
        doc["id"] = json!("p1");
        let rastreio = |id: &str| json!({ "id": id, "nome": "", "cor": "#ff0000", "midia": "m1", "base": "fonte", "tipo": "caixa",
            "espaco": "original", "ancoras": [], "denso": null, "silhuetas": null, "ausencias": [], "origem": { "tipo": "manual" },
            "confirmacao": null });
        doc["rastreios"] = json!([rastreio(".."), rastreio(fora.to_str().unwrap()), rastreio("../../../../../../fora do caso")]);
        doc["marcadores"] = json!([{ "id": "r-de-outro", "rotulo": "", "cor": "#ffffff", "pos": { "ref": "programa", "t": 0.0 } }]);
        let pasta = por(&ws, "p1", &doc.to_string());
        for sub in ["rastreios", "silhuetas"] {
            fs::create_dir_all(pasta.join(sub).join("r-de-outro")).unwrap();
            fs::write(pasta.join(sub).join("r-de-outro").join("x.sil"), b"x").unwrap();
            fs::write(pasta.join(sub).join("r-de-outro").join("x.json"), b"{}").unwrap();
        }
        fs::create_dir_all(pasta.join("exportacoes/s1")).unwrap();
        fs::write(pasta.join("exportacoes/s1/projeto.json"), b"{}").unwrap();
        let r = duplicar(&ws, "p1").unwrap();
        let copia = pasta_projeto(&ws, &r.id);
        let mut arquivos = Vec::new();
        lista(&copia, &mut arquivos);
        let nomes: Vec<String> = arquivos.iter().map(|a| a.strip_prefix(&copia).unwrap().to_string_lossy().to_string()).collect();
        assert_eq!(nomes, ["projeto.json"], "entrou arquivo que não é do projeto");
    }

    /// Campo novo num item CONHECIDO (o tipado ignora) que cita um arquivo, e o que a `projeto.anterior.json` cita:
    /// a limpeza não apaga.
    #[test]
    fn limpar_orfaos_respeita_campo_novo_e_a_anterior() {
        let (_t, ws) = caso();
        let pasta = projeto_cheio(&ws);
        let h = |par: &str| par.repeat(32);
        let arq = |sub: &str, sha: &str, ext: &str| pasta.join(sub).join(format!("{sha}.{ext}"));
        let mut doc = ler(&pasta.join(PROJETO));
        doc["rastreios"][0]["denso"]["sha256"] = json!(h("a1"));
        doc["rastreios"][0]["confirmacao"]["denso_sha256"] = json!(h("a1"));
        fs::write(arq("rastreios/r-manual", &h("a1"), "json"), b"{}").unwrap();
        fs::write(pasta.join(PROJETO), doc.to_string()).unwrap();
        abrir(&ws, "p1").unwrap();
        // na sessão: outro denso; a1 só fica citado pela anterior
        doc["rastreios"][0]["denso"]["sha256"] = json!(h("b2"));
        doc["rastreios"][0]["confirmacao"]["denso_sha256"] = json!(h("b2"));
        fs::write(arq("rastreios/r-manual", &h("b2"), "json"), b"{}").unwrap();
        doc["rastreios"][1]["mascara_do_futuro"] = json!({ "sha256": h("c3") });
        fs::write(arq("silhuetas/r-assistido", &h("c3"), "sil"), b"sil").unwrap();
        doc["principal"][0]["efeitos"][0]["trf_do_futuro"] = json!(h("d4"));
        fs::write(arq("vidstab", &h("d4"), "trf"), b"trf").unwrap();
        fs::write(arq("vidstab", &h("e5"), "trf"), b"orfao").unwrap();
        salvar(&ws, &doc).unwrap();
        let p = tipado(&doc).unwrap();
        let apagados = limpar_orfaos(&ws, &p, &[]).unwrap();
        assert!(arq("silhuetas/r-assistido", &h("c3"), "sil").is_file(), "campo novo em rastreio conhecido");
        assert!(arq("vidstab", &h("d4"), "trf").is_file(), "campo novo em efeito conhecido");
        assert!(arq("rastreios/r-manual", &h("a1"), "json").is_file(), "o denso que a anterior cita");
        assert!(arq("rastreios/r-manual", &h("b2"), "json").is_file());
        assert!(!arq("vidstab", &h("e5"), "trf").exists(), "órfão de verdade sai");
        // e…e de r-velho + e5.trf
        assert_eq!(apagados, 2);
    }

    /// Projeto de versão mais nova (só leitura): a limpeza não apaga nada.
    #[test]
    fn limpar_orfaos_nao_mexe_em_projeto_do_futuro() {
        let (_t, ws) = caso();
        let pasta = por(&ws, FUTURO, &fixture("projeto-futuro.json"));
        let trf = pasta.join("vidstab").join(format!("{}.trf", "9".repeat(64)));
        fs::create_dir_all(pasta.join("vidstab")).unwrap();
        fs::write(&trf, b"trf").unwrap();
        // o documento que chega pode ser outro que não o do disco
        let mut v = ler(&pasta.join(PROJETO));
        let p = tipado(&v).unwrap();
        assert_eq!(limpar_orfaos(&ws, &p, &[]).unwrap(), 0);
        v["esquema_menor"] = json!(0);
        assert_eq!(limpar_orfaos(&ws, &tipado(&v).unwrap(), &[]).unwrap(), 0, "o disco é do futuro");
        assert!(trf.is_file());
    }

    /// Algo que não sai no meio da exclusão: o `projeto.json` fica (o projeto segue na lista) e dá para repetir.
    #[cfg(unix)]
    #[test]
    fn excluir_que_falha_no_meio_da_para_repetir() {
        use std::os::unix::fs::PermissionsExt;
        let (_t, ws) = caso();
        let pasta = projeto_cheio(&ws);
        let presa = pasta.join("midia");
        fs::set_permissions(&presa, fs::Permissions::from_mode(0o555)).unwrap();
        // quem roda como root escreve assim mesmo: não há como prender a pasta
        if fs::write(presa.join("sonda"), b"").is_ok() {
            fs::set_permissions(&presa, fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }
        let r = excluir(&ws, "p1", None);
        fs::set_permissions(&presa, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(r.is_err());
        assert!(pasta.join(PROJETO).is_file(), "o documento sai por último");
        assert_eq!(listar(&ws).unwrap().len(), 1);
        assert!(pasta.join("exportacoes/s1/projeto.json").is_file());
        excluir(&ws, "p1", None).unwrap();
        assert!(listar(&ws).unwrap().is_empty());
        assert!(pasta.join("exportacoes/s1/dados/x.bin").is_file());
    }

    /// Duração, início, som e tamanho vêm do ffprobe (o `detect_ffprobe` de sempre; `SICRO_FFMPEG` não o troca).
    #[test]
    fn midia_do_projeto_com_os_dados_do_ffprobe() {
        let (t, ws) = caso();
        // WAV PCM 16 bits mono 8 kHz, 1 s exato
        let amostras = 8000u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + amostras * 2).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        for v in [16u32, 0x0001_0001, 8000, 16000, 0x0010_0002] {
            wav.extend_from_slice(&v.to_le_bytes());
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(amostras * 2).to_le_bytes());
        wav.extend((0..amostras).flat_map(|i| (((i % 80) as i16 - 40) * 300).to_le_bytes()));
        let som = t.path().join("narração 1 s.wav");
        fs::write(&som, &wav).unwrap();
        let figura = t.path().join("planta baixa.png");
        image::RgbImage::from_pixel(6, 4, image::Rgb([200, 30, 30])).save(&figura).unwrap();
        if crate::video::probe_media(&som).is_err() {
            assert!(std::env::var_os("SICRO_EXIGE_FFMPEG").is_none(), "ffprobe ausente");
            return;
        }
        let a = importar_midia(&ws, MINIMO, &som, TipoMidia::Audio).unwrap();
        assert_eq!(a.sha256, sha256_hex(&wav));
        assert!((a.duracao_s.unwrap() - 1.0).abs() < 1e-6, "{:?}", a.duracao_s);
        assert_eq!((a.inicio_s, a.tem_audio, a.largura, a.altura), (Some(0.0), true, None, None));
        let g = gravar_midia(&ws, MINIMO, "audio/wav", &wav).unwrap();
        assert!((g.duracao_s.unwrap() - 1.0).abs() < 1e-6 && g.sha256 == a.sha256 && g.id != a.id);
        let i = importar_midia(&ws, MINIMO, &figura, TipoMidia::Imagem).unwrap();
        assert_eq!((i.largura, i.altura, i.duracao_s, i.tem_audio), (Some(6), Some(4), None, false));
        // o que entra lê tipado dentro de um projeto
        let mut doc: Value = serde_json::from_str(&fixture("projeto-minimo.json")).unwrap();
        doc["midias"] = serde_json::to_value([&a, &g, &i]).unwrap();
        assert_eq!(tipado(&doc).unwrap().midias, [a, g, i]);
    }

    #[test]
    fn midia_de_zero_bytes_nao_entra() {
        let (t, ws) = caso();
        let vazio = t.path().join("vazio.wav");
        fs::write(&vazio, b"").unwrap();
        assert!(matches!(importar_midia(&ws, MINIMO, &vazio, TipoMidia::Audio), Err(SicroError::Validation(c)) if c == "arquivo_vazio"));
        let midia = pasta_projeto(&ws, MINIMO).join(MIDIA);
        assert!(!midia.exists() || fs::read_dir(&midia).unwrap().count() == 0, "sobrou arquivo");
        assert!(importar_midia(&ws, MINIMO, &t.path().join("não existe.wav"), TipoMidia::Audio).is_err());
    }

    /// Gravações e aberturas ao mesmo tempo: quem lê vê sempre um documento inteiro; não sobra `.tmp`.
    #[test]
    fn salvar_e_abrir_ao_mesmo_tempo() {
        let (_t, ws) = caso();
        let pasta = por(&ws, MINIMO, &fixture("projeto-minimo.json"));
        let base: Value = serde_json::from_str(&fixture("projeto-minimo.json")).unwrap();
        let recheio: Vec<f64> = (0..20_000).map(|i| i as f64 / 7.0).collect();
        std::thread::scope(|s| {
            for k in 0..4 {
                let (ws, base, recheio) = (&ws, &base, &recheio);
                s.spawn(move || {
                    for i in 0..15 {
                        let mut d = base.clone();
                        d["titulo"] = json!(format!("gravador {k} volta {i}"));
                        d["recheio"] = json!(recheio);
                        salvar(ws, &d).unwrap();
                    }
                });
            }
            for _ in 0..3 {
                let (ws, pasta) = (&ws, &pasta);
                s.spawn(move || {
                    for _ in 0..30 {
                        let a = abrir(ws, MINIMO).unwrap();
                        assert_eq!(a.projeto["id"], json!(MINIMO));
                        assert!(!a.so_leitura, "{:?}", a.avisos);
                        assert!(listar(ws).unwrap().iter().all(|r| r.problema.is_none()));
                        assert_eq!(ler(&pasta.join(ANTERIOR))["id"], json!(MINIMO));
                    }
                });
            }
        });
        assert!(ler(&pasta.join(PROJETO))["titulo"].as_str().unwrap().starts_with("gravador "));
        assert!(sem_tmp(&pasta));
    }

    /// A anterior é o documento que o `abrir` devolveu, mesmo com uma gravação no meio da abertura.
    #[test]
    fn anterior_e_o_documento_devolvido() {
        let (_t, ws) = caso();
        let pasta = por(&ws, MINIMO, &fixture("projeto-minimo.json"));
        let base: Value = serde_json::from_str(&fixture("projeto-minimo.json")).unwrap();
        use std::sync::atomic::{AtomicBool, Ordering};
        /// Para o gravador também quando a conferência falha (senão o `scope` espera para sempre).
        struct Parar<'a>(&'a AtomicBool);
        impl Drop for Parar<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Relaxed);
            }
        }
        let fim = AtomicBool::new(false);
        std::thread::scope(|s| {
            let (ws2, base, fim) = (&ws, &base, &fim);
            s.spawn(move || {
                let mut i = 0;
                while !fim.load(Ordering::Relaxed) {
                    let mut d = base.clone();
                    d["titulo"] = json!(format!("volta {i}"));
                    salvar(ws2, &d).unwrap();
                    i += 1;
                }
            });
            let _parar = Parar(fim);
            // só este fio abre: depois de marcada a sessão, o salvar não toca na anterior
            for _ in 0..300 {
                let a = abrir(&ws, MINIMO).unwrap();
                assert_eq!(ler(&pasta.join(ANTERIOR)), a.projeto);
            }
        });
    }

    #[cfg(unix)]
    #[test]
    fn pasta_sem_gravacao_abre_so_para_leitura() {
        use std::os::unix::fs::PermissionsExt;
        let (_t, ws) = caso();
        let pasta = por(&ws, MINIMO, &fixture("projeto-minimo.json"));
        fs::set_permissions(&pasta, fs::Permissions::from_mode(0o555)).unwrap();
        let r = abrir(&ws, MINIMO);
        fs::set_permissions(&pasta, fs::Permissions::from_mode(0o755)).unwrap();
        let a = r.expect("abre para ver");
        assert!(a.so_leitura);
        assert!(a.avisos.iter().any(|p| p.codigo == "sem_gravacao" && p.nivel == Nivel::Erro), "{:?}", a.avisos);
        assert!(!pasta.join(ANTERIOR).exists());
    }
}
