//! `tecnico.json` (§10.2), esquema `sicro.edicao.tecnico/2`: sai do MESMO `Plano` que gerou o render. Dono: P0.R.
//! `feito` é determinístico; `execucao` guarda o que varia. Nenhum caminho absoluto, nenhum nome de rastreio.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::edicao::camada::{self, texto};
use crate::edicao::executar::{Ambiente, Resultado};
use crate::edicao::motor::plano::Plano;
use crate::error::{Result, SicroError};

/// Valor do campo `esquema` do `tecnico.json`.
pub const ESQUEMA_TECNICO: &str = "sicro.edicao.tecnico/2";

/// Versão do Typst que rasteriza textos (fixada com `=` no `Cargo.toml`; um teste confere).
const VERSAO_TYPST: &str = "0.15.1";

/// Conferência das ocultações no arquivo exportado: feita ou não, por quem e quando (RFC 3339).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConferenciaOcultacoes {
    pub conferida: bool,
    pub por: String,
    pub em: String,
}

/// O que só se sabe ao fechar a exportação: SHA-256 do `projeto.json` do retrato, caminho do arquivo final
/// relativo ao caso e a conferência das ocultações (None = a exportação não tem ocultação nem silêncio).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fecho {
    pub projeto_sha256: String,
    pub arquivo_rel: String,
    pub ocultacoes: Option<ConferenciaOcultacoes>,
}

/// O `tecnico.json` inteiro: `{ esquema, feito, execucao, renderizado_em }`.
/// Erro quando o `Resultado` não é deste plano (blocos de outra chave).
pub fn montar(plano: &Plano, res: &Resultado, amb: &Ambiente, fecho: &Fecho) -> Result<Value> {
    mesmo_plano(plano, res)?;
    let feito = feito(plano, amb, fecho)?;
    let execucao = execucao(plano, res, amb, fecho)?;
    Ok(ordenado(json!({ "esquema": ESQUEMA_TECNICO, "feito": feito, "execucao": execucao, "renderizado_em": res.renderizado_em })))
}

/// Só a parte `feito` (mesmo projeto e mesmas fontes = mesmos bytes).
/// Avisos periciais levam `ciente` (o perito deu ciência); a hora fica em `execucao.ciencias`.
pub fn feito(plano: &Plano, amb: &Ambiente, fecho: &Fecho) -> Result<Value> {
    let pv = &plano.proveniencia;
    let mut roteiro = Vec::with_capacity(pv.roteiro.len());
    for item in &pv.roteiro {
        let mut v = serde_json::to_value(item)?;
        // ficam no `mapa.csv`
        if let Some(o) = v.get_mut("origem").and_then(Value::as_object_mut) {
            for k in ["j_in", "quadro_do_slot", "pts"] {
                o.remove(k);
            }
        }
        roteiro.push(v);
    }
    let avisos: Vec<Value> = pv
        .avisos
        .iter()
        .map(|a| json!({ "codigo": a.codigo, "n": a.n, "ciente": amb.ciencias.iter().any(|c| c.codigo == a.codigo) }))
        .collect();
    let p = &pv.projeto;
    let mut f = json!({
        "projeto": { "id": p.id, "titulo": p.titulo, "esquema": p.esquema, "esquema_menor": p.esquema_menor, "sha256": fecho.projeto_sha256 },
        "saida": pv.saida,
        "midias": pv.midias,
        "roteiro": roteiro,
        "ocultacoes": pv.ocultacoes,
        "audio": pv.audio,
        "legendas": pv.legendas,
        "sincronias": pv.sincronias,
        "rastreios": pv.rastreios,
        "ia": pv.ia,
        "estabilizacoes": pv.estabilizacoes,
        "avisos": avisos,
        "selos_desligados": pv.selos_desligados,
        "revisado_por": pv.revisado_por,
    });
    // raízes conhecidas primeiro (o caminho pode ter espaço); depois o que sobrou no texto gerado pela máquina
    Raizes::de(amb).relativizar(&mut f);
    for (lista, campo) in [("midias", "/derivacao/comando"), ("roteiro", "/processamentos"), ("rastreios", "/corridas")] {
        for x in f[lista].as_array_mut().into_iter().flatten() {
            if let Some(v) = x.pointer_mut(campo) {
                sem_absolutos(v);
            }
        }
    }
    sem_absolutos(&mut f["ia"]);
    Ok(ordenado(f))
}

/// Bytes do JSON como vão para o disco (serialização estável: chaves em ordem, 2 espaços, `\n` no fim).
pub fn bytes(tecnico: &Value) -> Vec<u8> {
    // Value só tem chaves de texto: não falha
    let mut b = serde_json::to_vec_pretty(&ordenado(tecnico.clone())).unwrap_or_default();
    b.push(b'\n');
    b
}

/// Grava `<Título>.tecnico.json` em `pasta` (com `create_new`) e devolve o SHA-256.
pub fn gravar(pasta: &Path, titulo: &str, tecnico: &Value) -> Result<String> {
    // `sanitize_folder_name` corta em 100 bytes e entra em pânico se o corte cai no meio de um caractere:
    // o título já vai cortado em limite de caractere
    let aparado = titulo.trim().trim_matches('.');
    let mut fim = aparado.len().min(100);
    while !aparado.is_char_boundary(fim) {
        fim -= 1;
    }
    let nome = format!("{}.tecnico.json", crate::filesystem::safe_paths::sanitize_folder_name(&aparado[..fim]));
    let b = bytes(tecnico);
    let destino = pasta.join(nome);
    let mut arq = OpenOptions::new().write(true).create_new(true).open(&destino)?;
    if let Err(e) = arq.write_all(&b).and_then(|_| arq.sync_all()) {
        // nunca um `tecnico.json` pela metade
        drop(arq);
        let _ = std::fs::remove_file(&destino);
        return Err(e.into());
    }
    Ok(sha256_hex(&b))
}

// CONTRATO: acréscimo (resposta (e) 15 do orquestrador). `edicao_conferir` completa o `tecnico.pendente.json`.
/// Completa a conferência das ocultações num `tecnico.json` já montado (estado `pendente`); `feito` não muda.
pub fn completar_conferencia(tecnico: &mut Value, c: &ConferenciaOcultacoes) -> Result<()> {
    if tecnico.get("esquema").and_then(Value::as_str) != Some(ESQUEMA_TECNICO) {
        return Err(SicroError::Validation("conferencia: tecnico.json de outro esquema".into()));
    }
    let conf = tecnico
        .pointer_mut("/execucao/conferencia")
        .and_then(Value::as_object_mut)
        .filter(|o| o.get("ocultacoes").and_then(Value::as_str) == Some(PENDENTE))
        .ok_or_else(|| SicroError::Validation("conferencia: nada pendente".into()))?;
    conf.insert("ocultacoes".into(), json!(if c.conferida { CONFERIDA } else { NAO_CONFERIDA }));
    conf.insert("por".into(), json!(c.por));
    conf.insert("em".into(), json!(c.em));
    Ok(())
}

const CONFERIDA: &str = "conferida";
const NAO_CONFERIDA: &str = "nao_conferida";
const PENDENTE: &str = "pendente";

/// Os blocos do resultado têm de ser os do plano (índice, slots e chave).
fn mesmo_plano(plano: &Plano, res: &Resultado) -> Result<()> {
    let do_plano: Vec<_> = plano.blocos.iter().map(|b| (b.indice, b.de_q, b.n_q, b.chave.as_str())).collect();
    let feitos: Vec<_> = res.blocos.iter().map(|b| (b.indice, b.de_q, b.n_q, b.chave.as_str())).collect();
    if do_plano != feitos {
        return Err(SicroError::Validation("plano_divergente: blocos do resultado não são os do plano".into()));
    }
    Ok(())
}

fn execucao(plano: &Plano, res: &Resultado, amb: &Ambiente, fecho: &Fecho) -> Result<Value> {
    let pv = &plano.proveniencia;
    let id = &amb.identidade;
    let conf = res.conferencia.as_ref();
    // tem ocultação ou silêncio: sem a conferência ainda, fica pendente
    let exige = !pv.ocultacoes.is_empty() || !pv.audio.silencios.is_empty();
    let (estado, por, em) = match (&fecho.ocultacoes, exige) {
        (Some(c), _) => (json!(if c.conferida { CONFERIDA } else { NAO_CONFERIDA }), c.por.as_str(), c.em.as_str()),
        (None, true) => (json!(PENDENTE), "", ""),
        (None, false) => (Value::Null, "", ""),
    };
    let fontes: Vec<Value> = texto::fontes().into_iter().map(|(nome, sha256)| json!({ "nome": nome, "sha256": sha256 })).collect();
    let ciencias: Vec<Value> =
        amb.ciencias.iter().map(|c| json!({ "codigo": c.codigo, "ciente_em": c.ciente_em, "por": id.autor.nome })).collect();
    let mut e = json!({
        "autor": id.autor,
        "caso": id.caso,
        "sicro": id.sicro,
        "sistema": id.sistema,
        "typst": VERSAO_TYPST,
        "rasterizador": camada::VERSAO_RASTERIZADOR,
        "fontes": fontes,
        "ffmpeg": pv.ffmpeg,
        "saida": {
            "arquivo": fecho.arquivo_rel, "sha256": res.sha256, "tamanho": res.tamanho, "codificador": res.codificador,
            "argumentos": res.argumentos, "queda_para_cpu": res.queda_para_cpu,
        },
        "conteudo": {
            "composto_sha256": res.composto_sha256,
            "decodificado_sha256": conf.map(|c| c.decodificado_sha256.as_str()),
            "audio_sha256": res.audio_sha256,
        },
        "blocos": res.blocos,
        "camadas": res.camadas,
        "audio": { "loudnorm": res.loudnorm, "ganho_db": res.ganho_db },
        "passos": res.passos,
        "conferencia": { "quadros": conf.map(|c| c.quadros), "erros": conf.map(|c| c.erros), "ocultacoes": estado, "por": por, "em": em },
        "ciencias": ciencias,
        "trf": res.trf,
    });
    Raizes::de(amb).relativizar(&mut e);
    for k in ["ffmpeg", "saida", "passos"] {
        sem_absolutos(&mut e[k]);
    }
    Ok(e)
}

fn sha256_hex(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}

/// Mesmo valor com as chaves de todo objeto em ordem (com o `Map` de árvore do serde_json já sai assim;
/// a ordem explícita vale se a feature `preserve_order` entrar por outra dependência).
fn ordenado(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut itens: Vec<(String, Value)> = m.into_iter().collect();
            itens.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(itens.into_iter().map(|(k, v)| (k, ordenado(v))).collect::<Map<_, _>>())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(ordenado).collect()),
        x => x,
    }
}

// ---------------------------------------------------------------------------
// Caminhos: relativos ao caso, à pasta de trabalho (nome simples) ou ao cache da Edição

/// Pastas conhecidas da execução e o que entra no lugar de cada uma (da mais longa para a mais curta).
struct Raizes(Vec<(String, &'static str)>);

impl Raizes {
    fn de(amb: &Ambiente) -> Self {
        let mut v = Vec::new();
        let mut por = |p: &Path, troca: &'static str| {
            let s = p.to_string_lossy().trim_end_matches(['/', '\\']).to_string();
            // raiz vazia ou "/" trocaria toda barra
            if p.is_absolute() && s.len() > 1 {
                v.push((s.replace('\\', "/"), troca));
                v.push((s, troca));
            }
        };
        por(&amb.trabalho, "");
        por(&amb.cache, "cache/");
        // dentro do caso, a regra do caso vale
        if !amb.pasta_projeto.starts_with(&amb.caso) {
            por(&amb.pasta_projeto, "projeto/");
        }
        por(&amb.caso, "");
        if let Some(d) = amb.cap.binario.parent() {
            por(d, "");
        }
        if let Some(d) = amb.cap.ffprobe.parent() {
            por(d, "");
        }
        v.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
        v.dedup();
        Raizes(v)
    }

    fn relativizar(&self, v: &mut Value) {
        match v {
            Value::String(s) => {
                for (raiz, troca) in &self.0 {
                    if s.contains(raiz.as_str()) {
                        *s = trocar_raiz(s, raiz, troca);
                    }
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|x| self.relativizar(x)),
            Value::Object(m) => m.values_mut().for_each(|x| self.relativizar(x)),
            _ => {}
        }
    }
}

/// Troca `raiz/` (ou `raiz\`) por `troca`; a raiz sozinha (seguida de fim, aspas ou espaço) vira `troca` sem a barra, ou ".".
fn trocar_raiz(s: &str, raiz: &str, troca: &str) -> String {
    let mut saida = String::with_capacity(s.len());
    let mut resto = s;
    while let Some(i) = resto.find(raiz) {
        saida.push_str(&resto[..i]);
        let depois = &resto[i + raiz.len()..];
        match depois.chars().next() {
            Some('/') | Some('\\') => {
                saida.push_str(troca);
                resto = &depois[1..];
            }
            None | Some(' ') | Some('"') | Some('\'') | Some(',') | Some(';') => {
                saida.push_str(if troca.is_empty() { "." } else { troca.trim_end_matches('/') });
                resto = depois;
            }
            // prefixo de outro nome (`/caso` dentro de `/caso2`): fica
            Some(c) => {
                saida.push_str(raiz);
                saida.push(c);
                resto = &depois[c.len_utf8()..];
            }
        }
    }
    saida.push_str(resto);
    saida
}

/// Texto gerado pela máquina (argumentos, comandos, configuração do ffmpeg): todo caminho absoluto que sobrou
/// vira o nome do arquivo. Texto do perito não passa por aqui.
fn sem_absolutos(v: &mut Value) {
    limpar(v, false);
}

/// `argv` = o valor é uma lista de argumentos (`argumentos`): cada texto é um argumento inteiro.
fn limpar(v: &mut Value, argv: bool) {
    match v {
        Value::String(s) => {
            if tem_absoluto(s) {
                *s = if argv { tirar_do_argumento(s) } else { tirar_absolutos(s) };
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| limpar(x, argv)),
        Value::Object(m) => m.iter_mut().for_each(|(k, x)| limpar(x, k == "argumentos")),
        _ => {}
    }
}

/// Um argumento que começa por caminho absoluto É o caminho, com os espaços que tiver: fica o nome do arquivo.
fn tirar_do_argumento(s: &str) -> String {
    match inicio_do_caminho(s) {
        Some(i) => format!("{}{}", &s[..i], nome_do_arquivo(&s[i..])),
        None => tirar_absolutos(s),
    }
}

fn nome_do_arquivo(caminho: &str) -> &str {
    caminho.rsplit(['/', '\\']).next().unwrap_or("")
}

const SEPARADORES: &[char] = &[' ', '\t', '\n', '\r', '"', '\'', '=', ',', ';', '(', ')', '[', ']', '|', '<', '>'];

/// Pedaço que é caminho absoluto: `/x`, `~/x`, `C:\x`, `C:/x` ou `\\servidor`.
fn absoluto(tok: &str) -> bool {
    let b = tok.as_bytes();
    match b {
        [b'/', c, ..] => *c != b'/' && !c.is_ascii_whitespace(),
        [b'~', b'/', ..] | [b'\\', b'\\', ..] => true,
        [l, b':', b'\\' | b'/', ..] => l.is_ascii_alphabetic(),
        _ => false,
    }
}

/// Onde começa o caminho absoluto do pedaço: no início, ou depois de uma opção curta colada (`-I/opt/x`).
fn inicio_do_caminho(tok: &str) -> Option<usize> {
    let b = tok.as_bytes();
    if absoluto(tok) {
        Some(0)
    } else if b.len() > 2 && b[0] == b'-' && b[1].is_ascii_alphabetic() && absoluto(&tok[2..]) {
        Some(2)
    } else {
        None
    }
}

fn tem_absoluto(s: &str) -> bool {
    s.split(SEPARADORES).any(|t| inicio_do_caminho(t).is_some())
}

fn tirar_absolutos(s: &str) -> String {
    let mut saida = String::with_capacity(s.len());
    let mut tok = String::new();
    let fecha = |tok: &mut String, saida: &mut String| {
        match inicio_do_caminho(tok) {
            Some(i) => {
                saida.push_str(&tok[..i]);
                saida.push_str(nome_do_arquivo(&tok[i..]));
            }
            None => saida.push_str(tok),
        }
        tok.clear();
    };
    let mut resto = s;
    while let Some(c) = resto.chars().next() {
        let depois = &resto[c.len_utf8()..];
        // entre aspas é um pedaço só (caminho com espaço); aspa sem par é separador comum
        if (c == '"' || c == '\'') && tok.is_empty() {
            if let Some(fim) = depois.find(c) {
                let dentro = &depois[..fim];
                saida.push(c);
                saida.push_str(&tirar_do_argumento(dentro));
                saida.push(c);
                resto = &depois[fim + 1..];
                continue;
            }
        }
        if SEPARADORES.contains(&c) {
            fecha(&mut tok, &mut saida);
            saida.push(c);
        } else {
            tok.push(c);
        }
        resto = depois;
    }
    fecha(&mut tok, &mut saida);
    saida
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::relatorio::apoio::{self, absolutos_no_texto, Cena};

    fn fecho(ocultacoes: Option<ConferenciaOcultacoes>) -> Fecho {
        Fecho { projeto_sha256: "9".repeat(64), arquivo_rel: "Exportações/Vídeos/Edições/Caso 2026-11-14 10h00m00/Caso.mp4".into(), ocultacoes }
    }

    #[test]
    fn bytes_com_chaves_em_ordem_dois_espacos_e_quebra_no_fim() {
        let v = json!({ "b": 1, "a": { "d": [1, { "z": null, "y": 0.5 }], "c": "x" } });
        let esperado = "{\n  \"a\": {\n    \"c\": \"x\",\n    \"d\": [\n      1,\n      {\n        \"y\": 0.5,\n        \"z\": null\n      }\n    ]\n  },\n  \"b\": 1\n}\n";
        assert_eq!(String::from_utf8(bytes(&v)).unwrap(), esperado);
    }

    #[test]
    fn feito_identico_para_a_mesma_entrada() {
        let c = Cena::nova("a");
        let (plano, res1) = apoio::plano_e_resultado(&c);
        // outra tarefa (outra pasta de trabalho), outra hora de ciência e de render, blocos reaproveitados
        let res2 = apoio::resultado(&c, &plano, "t2 ç");
        let amb1 = c.ambiente("t1");
        let mut amb2 = c.ambiente("t2 ç");
        amb2.ciencias[0].ciente_em = "2026-11-15T09:00:00Z".into();
        let fa = bytes(&feito(&plano, &amb1, &fecho(None)).unwrap());
        let fb = bytes(&feito(&plano, &amb2, &fecho(None)).unwrap());
        assert_eq!(String::from_utf8(fa.clone()).unwrap(), String::from_utf8(fb).unwrap());
        let t1 = montar(&plano, &res1, &amb1, &fecho(None)).unwrap();
        let t2 = montar(&plano, &res2, &amb2, &fecho(None)).unwrap();
        assert_eq!(bytes(&t1["feito"]), fa);
        assert_eq!(bytes(&t2["feito"]), fa);
        assert_ne!(bytes(&t1["execucao"]), bytes(&t2["execucao"]));
        // montar de novo dá os mesmos bytes
        assert_eq!(bytes(&t1), bytes(&montar(&plano, &res1, &amb1, &fecho(None)).unwrap()));
        // o retrato do projeto muda o `feito`
        let outro = Fecho { projeto_sha256: "8".repeat(64), ..fecho(None) };
        assert_ne!(bytes(&feito(&plano, &amb1, &outro).unwrap()), fa);
    }

    #[test]
    fn sem_caminho_absoluto_nem_nome_de_rastreio() {
        let c = Cena::nova("pasta d'água ç");
        let (plano, res) = apoio::plano_e_resultado(&c);
        let t = montar(&plano, &res, &c.ambiente("t1"), &fecho(None)).unwrap();
        let texto = String::from_utf8(bytes(&t)).unwrap();
        // as raízes estão na entrada (argumentos, comando de derivação, corridas, configuração do ffmpeg) e somem na saída
        let entrada = format!("{:?} {:?}", plano.proveniencia, res.passos);
        assert!(entrada.contains(&*c.caso.to_string_lossy()) && entrada.contains(&*c.cache.to_string_lossy()) && entrada.contains("/usr"));
        assert_eq!(absolutos_no_texto(&texto, &c.raizes(), &[apoio::NOME_RASTREIO]), Vec::<String>::new(), "{texto}");
        // e a varredura acusa quando há
        assert!(!absolutos_no_texto(&format!("[\"x {}\"]", c.caso.to_string_lossy()), &[], &[]).is_empty());
        assert!(!absolutos_no_texto("[\"-I/opt/x\", \"C:\\\\v\"]", &[], &[]).is_empty());
        // relativos no lugar
        let passo = &t["execucao"]["passos"][0]["argumentos"];
        assert_eq!(passo, &json!(["-i", "Mídias/Vídeos/cam 1.mp4", "-/filter_complex", "b000.txt", "-i", "cache/camadas/abc/lista.ffconcat", "b000.mp4"]));
        assert_eq!(t["feito"]["midias"][0]["derivacao"]["comando"], json!("ffmpeg -i Mídias/orig.dav -c copy dvr 1/out.mp4"));
        assert_eq!(t["execucao"]["ffmpeg"]["configuracao"], json!("--prefix=usr --enable-gpl --extra-cflags=-Iinclude"));
        assert_eq!(t["execucao"]["saida"]["arquivo"], json!(fecho(None).arquivo_rel));
        // o texto do perito não é mexido
        assert_eq!(t["feito"]["midias"][0]["recebido"]["obs"], json!("veio por pendrive"));
    }

    #[test]
    fn ciencia_no_feito_sem_hora_e_na_execucao_com_hora() {
        let c = Cena::nova("x");
        let (plano, res) = apoio::plano_e_resultado(&c);
        let t = montar(&plano, &res, &c.ambiente("t1"), &fecho(None)).unwrap();
        assert_eq!(t["feito"]["avisos"], json!([{ "ciente": true, "codigo": "taxa_variavel", "n": 3 }, { "ciente": false, "codigo": "fonte_danificada", "n": 2 }]));
        assert!(!String::from_utf8(bytes(&t["feito"])).unwrap().contains("ciente_em"));
        assert_eq!(t["execucao"]["ciencias"], json!([{ "codigo": "taxa_variavel", "ciente_em": "2026-11-14T10:00:00Z", "por": "Perito Teste" }]));
        assert_eq!(t["esquema"], json!(ESQUEMA_TECNICO));
        assert_eq!(t["renderizado_em"], json!(res.renderizado_em));
        assert_eq!(t["feito"]["projeto"]["sha256"], json!("9".repeat(64)));
        // a origem do roteiro não leva o mapa por slot (vai para o mapa.csv)
        let o = t["feito"]["roteiro"][1]["origem"].as_object().unwrap();
        assert!(o.contains_key("mapa") && o.contains_key("lidos") && !o.contains_key("quadro_do_slot") && !o.contains_key("pts") && !o.contains_key("j_in"));
        assert_eq!(t["execucao"]["typst"], json!(VERSAO_TYPST));
        assert!(!t["execucao"]["fontes"].as_array().unwrap().is_empty());
    }

    #[test]
    fn resultado_de_outro_plano_e_recusado() {
        let c = Cena::nova("x");
        let (plano, mut res) = apoio::plano_e_resultado(&c);
        res.blocos[0].chave = "outra".into();
        let e = montar(&plano, &res, &c.ambiente("t1"), &fecho(None)).unwrap_err();
        assert!(matches!(&e, SicroError::Validation(m) if m.starts_with("plano_divergente")), "{e:?}");
        res.blocos.clear();
        assert!(montar(&plano, &res, &c.ambiente("t1"), &fecho(None)).is_err());
    }

    #[test]
    fn conferencia_das_ocultacoes() {
        let c = Cena::nova("x");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        let amb = c.ambiente("t1");
        // tem ocultação: sem fecho, pendente
        let mut t = montar(&plano, &res, &amb, &fecho(None)).unwrap();
        assert_eq!(t["execucao"]["conferencia"], json!({ "quadros": plano.total_q, "erros": 0, "ocultacoes": "pendente", "por": "", "em": "" }));
        let antes = bytes(&t["feito"]);
        let conf = ConferenciaOcultacoes { conferida: true, por: "Perito Teste".into(), em: "2026-11-14T11:00:00Z".into() };
        completar_conferencia(&mut t, &conf).unwrap();
        assert_eq!(t["execucao"]["conferencia"]["ocultacoes"], json!("conferida"));
        assert_eq!(t["execucao"]["conferencia"]["por"], json!("Perito Teste"));
        assert_eq!(bytes(&t["feito"]), antes);
        // igual ao montado com o fecho completo; completar de novo é recusado
        assert_eq!(t, montar(&plano, &res, &amb, &fecho(Some(conf.clone()))).unwrap());
        assert!(completar_conferencia(&mut t, &conf).is_err());
        let nao = ConferenciaOcultacoes { conferida: false, ..conf.clone() };
        assert_eq!(montar(&plano, &res, &amb, &fecho(Some(nao))).unwrap()["execucao"]["conferencia"]["ocultacoes"], json!("nao_conferida"));
        // só silêncio também pede conferência; sem ocultação nem silêncio: nulo
        plano.proveniencia.ocultacoes.clear();
        assert_eq!(montar(&plano, &res, &amb, &fecho(None)).unwrap()["execucao"]["conferencia"]["ocultacoes"], json!("pendente"));
        plano.proveniencia.audio.silencios.clear();
        let mut t = montar(&plano, &res, &amb, &fecho(None)).unwrap();
        assert_eq!(t["execucao"]["conferencia"]["ocultacoes"], Value::Null);
        assert!(completar_conferencia(&mut t, &conf).is_err());
        let e = completar_conferencia(&mut json!({ "esquema": "x" }), &conf).unwrap_err();
        // não se confunde com o `esquema` do catálogo (projeto inválido)
        assert!(matches!(&e, SicroError::Validation(m) if m.starts_with("conferencia: ")), "{e:?}");
    }

    #[test]
    fn gravar_cria_novo_e_devolve_o_sha256() {
        let c = Cena::nova("x");
        let (plano, res) = apoio::plano_e_resultado(&c);
        let t = montar(&plano, &res, &c.ambiente("t1"), &fecho(None)).unwrap();
        let sha = gravar(&c.caso, "Caso: 1/2", &t).unwrap();
        let arq = c.caso.join("Caso_ 1_2.tecnico.json");
        let lido = std::fs::read(&arq).unwrap();
        assert_eq!(lido, bytes(&t));
        assert_eq!(sha, sha256_hex(&lido));
        assert_eq!(serde_json::from_slice::<Value>(&lido).unwrap(), t);
        // não sobrescreve
        assert!(gravar(&c.caso, "Caso: 1/2", &t).is_err());
        assert_eq!(std::fs::read(&arq).unwrap(), lido);
    }

    #[test]
    fn versao_do_typst_e_a_do_cargo() {
        let cargo = include_str!("../../../Cargo.toml");
        assert!(cargo.contains(&format!("typst = \"={VERSAO_TYPST}\"")), "subir VERSAO_TYPST junto com o Cargo.toml");
    }

    #[test]
    fn raizes_e_caminhos_soltos() {
        assert_eq!(trocar_raiz("/c/x /c2/y /c", "/c", ""), "x /c2/y .");
        assert_eq!(trocar_raiz("-i /cache/a.png", "/cache", "cache/"), "-i cache/a.png");
        assert_eq!(tirar_absolutos("movie=/a/b c.png:x=1, -i C:\\v\\x.mp4 \\\\srv\\d\\e.mp4 ~/z.txt -L/usr/lib"), "movie=b c.png:x=1, -i x.mp4 e.mp4 z.txt -Llib");
        // frações, URL, opção e barra solta ficam
        for s in ["30000/1001", "https://github.com/x/y", "a / b", "N/FRAME_RATE/TB", "-", "-/filter_complex", "setpts=PTS-STARTPTS"] {
            assert!(!tem_absoluto(s), "{s}");
            assert_eq!(tirar_absolutos(s), s);
        }
    }

    /// Caminho com espaço fora das raízes conhecidas: o argumento inteiro é o caminho; entre aspas, o trecho entre aspas.
    /// Nenhum pedaço de pasta (nome de usuário do Windows) sobra.
    #[test]
    fn caminho_com_espaco_nao_deixa_pedaco() {
        let c = Cena::nova("x");
        let (plano, mut res) = apoio::plano_e_resultado(&c);
        res.passos[0].argumentos = vec![
            "-i".into(),
            "/mnt/dvr 1/cam 2.mp4".into(),
            "-i".into(),
            "C:\\Users\\André Barroso\\Videos\\cam 3.mp4".into(),
            "-vf".into(),
            "scale=iw/2:-2".into(),
            "-metadata".into(),
            "title=Caso 1/2".into(),
        ];
        res.argumentos = vec!["-passlogfile".into(), "\\\\srv\\meus casos\\x.log".into()];
        let t = montar(&plano, &res, &c.ambiente("t1"), &fecho(None)).unwrap();
        assert_eq!(t["execucao"]["passos"][0]["argumentos"], json!(["-i", "cam 2.mp4", "-i", "cam 3.mp4", "-vf", "scale=iw/2:-2", "-metadata", "title=Caso 1/2"]));
        assert_eq!(t["execucao"]["saida"]["argumentos"], json!(["-passlogfile", "x.log"]));
        let texto = String::from_utf8(bytes(&t)).unwrap();
        assert_eq!(absolutos_no_texto(&texto, &c.raizes(), &["André Barroso", "meus casos"]), Vec::<String>::new(), "{texto}");
        // linha de comando em texto corrido: o que está entre aspas é um pedaço só
        assert_eq!(
            tirar_absolutos("ffmpeg -i \"/mnt/dvr 1/in.mp4\" 'C:\\Users\\André B\\x y.mp4' -vf \"scale=iw/2:-2\" out.mp4"),
            "ffmpeg -i \"in.mp4\" 'x y.mp4' -vf \"scale=iw/2:-2\" out.mp4"
        );
        // aspas sem par: segue pedaço a pedaço
        assert_eq!(tirar_absolutos("d'água -i /a/b.mp4"), "d'água -i b.mp4");
    }

    /// O mesmo caso em outra pasta (outra máquina, pasta movida): mesmo `feito`.
    #[test]
    fn feito_nao_depende_de_onde_o_caso_esta() {
        let (a, b) = (Cena::nova("a"), Cena::nova("outro lugar ç"));
        let (pa, _) = apoio::plano_e_resultado(&a);
        let (pb, _) = apoio::plano_e_resultado(&b);
        assert_ne!(format!("{:?}", pa.proveniencia), format!("{:?}", pb.proveniencia));
        let fa = bytes(&feito(&pa, &a.ambiente("t1"), &fecho(None)).unwrap());
        let fb = bytes(&feito(&pb, &b.ambiente("t9"), &fecho(None)).unwrap());
        assert_eq!(String::from_utf8(fa).unwrap(), String::from_utf8(fb).unwrap());
    }

    /// Título longo com acento: o nome é cortado em limite de caractere, sem pânico; erro de escrita não deixa arquivo pela metade.
    #[test]
    fn gravar_com_titulo_longo_e_acentuado() {
        let c = Cena::nova("x");
        let (plano, res) = apoio::plano_e_resultado(&c);
        let t = montar(&plano, &res, &c.ambiente("t1"), &fecho(None)).unwrap();
        let titulo = format!("a{}", "ç".repeat(80));
        let sha = gravar(&c.caso, &titulo, &t).unwrap();
        let nomes: Vec<String> = std::fs::read_dir(&c.caso).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".tecnico.json")).collect();
        assert_eq!(nomes.len(), 1, "{nomes:?}");
        assert!(nomes[0].starts_with("açççç") && nomes[0].len() <= 100 + ".tecnico.json".len(), "{nomes:?}");
        assert_eq!(sha, sha256_hex(&std::fs::read(c.caso.join(&nomes[0])).unwrap()));
        // pasta que não existe: erro, nada criado
        assert!(gravar(&c.caso.join("nao-existe"), "x", &t).is_err());
    }

    /// O que o PDF (`pdf-relatorio.md` §8, contrato `sicro.relatorio_juri/1`, e o §10.3) lê do `tecnico.json`.
    /// O que falta está no relato do P0.R (tamanho e imagem dos originais, taxa nominal, cópia de trabalho, miniatura).
    #[test]
    fn tem_o_que_o_pdf_le() {
        let c = Cena::nova("x");
        let (plano, res) = apoio::plano_e_resultado(&c);
        let t = montar(&plano, &res, &c.ambiente("t1"), &fecho(None)).unwrap();
        let caminhos = [
            // abertura: caso, perito, data
            "/execucao/caso/numero", "/execucao/caso/referencia", "/execucao/caso/orgao", "/execucao/caso/requisitante",
            "/execucao/autor/nome", "/execucao/autor/cargo", "/execucao/autor/matricula", "/renderizado_em",
            // saída
            "/execucao/saida/arquivo", "/execucao/saida/sha256", "/execucao/saida/tamanho", "/feito/saida/duracao_s", "/feito/saida/largura",
            "/feito/saida/altura", "/feito/saida/fps", "/feito/saida/predefinicao", "/feito/saida/com_perdas", "/execucao/saida/codificador",
            // originais (seção 3 e anexo B)
            "/feito/midias/0/id", "/feito/midias/0/nome", "/feito/midias/0/sha256", "/feito/midias/0/conteiner", "/feito/midias/0/codec",
            "/feito/midias/0/pix_fmt", "/feito/midias/0/sar", "/feito/midias/0/medido/quadros", "/feito/midias/0/medido/passo_min",
            "/feito/midias/0/medido/passo_max", "/feito/midias/0/medido/tb", "/feito/midias/0/recebido/nativo", "/feito/midias/0/relogio_desvio_s",
            "/feito/midias/0/derivacao", "/feito/midias/0/declarado/duracao", "/feito/midias/0/declarado/r_frame_rate",
            "/feito/midias/0/discrepancias", "/feito/midias/0/erros_decodificacao",
            // cobertura (§10.3)
            "/feito/midias/0/usado", "/feito/midias/0/usado_total_s", "/feito/midias/0/medido/de_s", "/feito/midias/0/medido/ate_s",
            // roteiro, processamentos (anexo C), diferenças (anexo D)
            "/feito/roteiro/1/n", "/feito/roteiro/1/tipo", "/feito/roteiro/1/nome", "/feito/roteiro/1/na_saida/de_s", "/feito/roteiro/1/na_saida/n_q",
            "/feito/roteiro/1/origem/de_s", "/feito/roteiro/1/origem/lidos", "/feito/roteiro/1/origem/repetidos", "/feito/roteiro/1/origem/descartados",
            "/feito/roteiro/1/velocidade", "/feito/roteiro/1/processamentos/0/ordem", "/feito/roteiro/1/processamentos/0/parametros",
            // ocultações de imagem e de som (anexo E)
            "/feito/ocultacoes/0/de_s", "/feito/ocultacoes/0/conteudo", "/feito/ocultacoes/0/modo", "/feito/ocultacoes/0/intensidade",
            "/feito/ocultacoes/0/motivo", "/feito/audio/silencios/0/motivo", "/execucao/conferencia/ocultacoes",
            // sincronização (F), IA (ia_resumo), limites (7), reprodução e programas (H)
            "/feito/sincronias", "/feito/ia/0/modelo", "/feito/ia/0/arquivos/0/sha256", "/feito/rastreios/0/origem", "/feito/rastreios/0/confirmacao",
            "/feito/avisos", "/feito/selos_desligados", "/feito/revisado_por", "/feito/projeto/sha256",
            "/execucao/ffmpeg/versao", "/execucao/ffmpeg/sha256", "/execucao/sicro/versao", "/execucao/sicro/commit", "/execucao/typst", "/execucao/fontes",
            "/execucao/sistema/cpu/modelo", "/execucao/sistema/gpu", "/execucao/saida/queda_para_cpu", "/execucao/saida/argumentos",
            "/execucao/conteudo/decodificado_sha256", "/execucao/conteudo/composto_sha256",
        ];
        let faltam: Vec<_> = caminhos.iter().filter(|c| t.pointer(c).is_none()).collect();
        assert!(faltam.is_empty(), "{faltam:?}");
    }
}
