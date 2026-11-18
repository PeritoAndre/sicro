//! Refazer e comparar (§10.4): lê só o retrato da exportação (O7), confere os hashes do manifesto, recompila com o
//! mesmo alvo, compara o plano, renderiza em `render/<tarefa>/` e compara `quadros.compostos.sha256` e o PCM.
//! A refeita é publicada como saída nova (`video_edit_outputs.refeita_de`), com o resultado em `comparacao.json` no
//! retrato dela e no log `edicao.refazer`. Dono: P1.8.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::database::repositories::video_edit_output_repo;
use crate::edicao::executar::{self, abrir_banco, Ambiente, Ciencia, Progresso, Resultado};
use crate::edicao::fila::{self, Estado, EstadoSaida, Fila, Manifesto, PedidoRender, SaidaRegistrada, Tarefa, TipoTarefa, COMPOSTOS};
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{EscolhaCodificador, Racional};
use crate::edicao::motor::plano::Plano;
use crate::edicao::motor::Alvo;
use crate::edicao::{armazem, tempo, validar};
use crate::error::{Result, SicroError};
use crate::workspace::paths;

/// Resultado da comparação. `Diferente.tempo` em s do programa (o slot `primeiro_q`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Comparacao {
    Igual,
    ConteudoIgual { diferencas: Vec<String> },
    Diferente { primeiro_q: u64, tempo: f64 },
    NaoComparavel { motivo: String },
}

/// Arquivo do resultado no retrato da saída refeita.
pub(crate) const COMPARACAO: &str = "comparacao.json";
/// Motivos do `NaoComparavel`.
pub(crate) const MOTIVO_PLANO: &str = "plano";
pub(crate) const MOTIVO_FONTE: &str = "fonte_danificada";
pub(crate) const MOTIVO_SOM: &str = "som";
/// Aviso pericial sem ciência na exportação de origem (código novo do programa); o motivo leva os códigos.
pub(crate) const MOTIVO_CIENCIA: &str = "ciencia_nova";

/// Cache só da refeita, em `render/<tarefa>/`: com o do caso, os blocos da exportação de origem voltavam e a
/// comparação dava "Igual" sem render nenhum.
pub(crate) const CACHE_PROPRIO: &str = "cache";

/// Apaga o cache próprio no fim, dê no que der.
struct CacheProprio(PathBuf);

impl Drop for CacheProprio {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Tarefa `Refazer` → saída de origem (o pedido de render não leva a saída; a tarefa a busca aqui ao rodar).
static PEDIDOS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

fn erro(t: impl Into<String>) -> SicroError {
    SicroError::Validation(t.into())
}

/// O retrato lido e conferido; qualquer divergência dos hashes vira `retrato_divergente`.
fn retrato(ws: &Path, projeto_id: &str, saida_id: &str) -> Result<(Value, Manifesto)> {
    fila::ler_retrato(ws, projeto_id, saida_id).map_err(|e| match &e {
        SicroError::Validation(m) => erro(format!("{}: {}", codigo::RETRATO_DIVERGENTE, m.split_once(": ").map_or(m.as_str(), |(_, d)| d))),
        outro => erro(format!("{}: {outro}", codigo::RETRATO_DIVERGENTE)),
    })
}

fn ler_json(a: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(a)?)?)
}

/// `edicao_refazer`: põe na fila de render uma tarefa `Refazer` da saída `saida_id` e devolve o id dela.
/// `cache` = pasta da Edição do caso.
// CONTRATO: ganhou `cache` (a tarefa renderiza em `render/<tarefa>/`, e o comando é quem conhece o cache do app).
pub fn pedir(ws: &Path, cache: &Path, saida_id: &str) -> Result<String> {
    let ausente = || erro(format!("saida_ausente: {saida_id}"));
    let (conn, occ) = abrir_banco(ws)?;
    let id = uuid::Uuid::parse_str(saida_id).map_err(|_| ausente())?;
    let s = video_edit_output_repo::obter(&conn, &occ, &id)?.ok_or_else(ausente)?;
    // a conferência ainda não fechou o relatório: não há com o que comparar
    let tecnico_rel = s.tecnico_rel.as_deref().filter(|_| s.estado == EstadoSaida::Concluida).ok_or_else(|| erro(format!("na_fila: {saida_id}")))?;
    let (projeto, m) = retrato(ws, &s.projeto_id, saida_id)?;
    let tecnico = ler_json(&paths::resolver_relativo(ws, tecnico_rel))?;
    let ciencias = ciencias_de(&tecnico).into_iter().map(|c| c.codigo).collect();
    fila::limpar_sobras_de_publicar(ws);
    // a trava fica de pé até o registro: a tarefa que começar antes espera por ele
    let mut pedidos = PEDIDOS.lock().unwrap_or_else(|e| e.into_inner());
    let tarefa = fila::enfileirar(PedidoRender { tipo: TipoTarefa::Refazer, ws: ws.to_path_buf(), cache: cache.to_path_buf(), projeto, alvo: m.alvo, ciencias })?;
    pedidos.push((tarefa.clone(), saida_id.to_string()));
    Ok(tarefa)
}

/// Ciências registradas no `tecnico.json` (código e hora): a refeita leva as mesmas.
fn ciencias_de(tecnico: &Value) -> Vec<Ciencia> {
    tecnico
        .pointer("/execucao/ciencias")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| Some(Ciencia { codigo: c.get("codigo")?.as_str()?.to_string(), ciente_em: c.get("ciente_em").and_then(Value::as_str).unwrap_or("").to_string() }))
        .collect()
}

/// Resultado gravado no retrato de uma saída refeita; None nas outras (ou se o arquivo não lê).
pub(crate) fn ler_comparacao(ws: &Path, s: &SaidaRegistrada) -> Option<Comparacao> {
    if !executar::relativo_seguro(&s.retrato_rel) {
        return None;
    }
    serde_json::from_slice(&fs::read(paths::resolver_relativo(ws, &s.retrato_rel).join(COMPARACAO)).ok()?).ok()
}

/// O que se compara de uma exportação: lista por slot do composto, PCM, arquivo, decodificado e onde foi feita.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Fatos {
    pub compostos: Vec<String>,
    pub audio: Option<String>,
    pub arquivo: String,
    pub decodificado: Option<String>,
    pub codificador: String,
    pub argumentos: Vec<String>,
    pub ffmpeg: String,
    pub cpu: Vec<String>,
    pub gpu: Option<(String, String)>,
    /// Alguma mídia usada tem erro de decodificação.
    pub danificada: bool,
}

fn linhas(texto: &str) -> Vec<String> {
    texto.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

impl Fatos {
    /// Da exportação registrada: banco, retrato e `tecnico.json`.
    fn da_saida(s: &crate::models::VideoEditOutput, retrato: &Path, tecnico: &Value) -> Result<Fatos> {
        let texto = |p: &str| tecnico.pointer(p).and_then(Value::as_str).map(String::from);
        let lista = |p: &str| -> Vec<String> { tecnico.pointer(p).and_then(Value::as_array).into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect() };
        Ok(Fatos {
            compostos: linhas(&fs::read_to_string(retrato.join(COMPOSTOS)).map_err(|_| erro(format!("{}: {COMPOSTOS}", codigo::RETRATO_DIVERGENTE)))?),
            audio: s.audio_sha256.clone(),
            arquivo: s.sha256.clone(),
            decodificado: s.decodificado_sha256.clone(),
            codificador: s.codificador.clone(),
            argumentos: lista("/execucao/saida/argumentos"),
            ffmpeg: texto("/execucao/ffmpeg/sha256").unwrap_or_default(),
            cpu: lista("/execucao/sistema/cpu/instrucoes"),
            gpu: texto("/execucao/sistema/gpu").map(|g| (g, texto("/execucao/sistema/driver").unwrap_or_default())),
            danificada: danificada(tecnico),
        })
    }

    /// Do render que acabou de rodar.
    fn do_render(plano: &Plano, res: &Resultado, amb: &Ambiente) -> Result<Fatos> {
        let compostos = match &res.compostos {
            Some(c) => linhas(&fs::read_to_string(c)?),
            None => Vec::new(),
        };
        let sis = &amb.identidade.sistema;
        Ok(Fatos {
            compostos,
            audio: res.audio_sha256.clone(),
            arquivo: res.sha256.clone(),
            decodificado: res.conferencia.as_ref().map(|c| c.decodificado_sha256.clone()),
            codificador: res.codificador.clone(),
            argumentos: res.argumentos.clone(),
            ffmpeg: plano.proveniencia.ffmpeg.sha256.clone(),
            cpu: sis.cpu.instrucoes.clone(),
            gpu: sis.gpu.clone().map(|g| (g, sis.driver.clone().unwrap_or_default())),
            danificada: plano.proveniencia.midias.iter().any(|m| m.erros_decodificacao > 0),
        })
    }
}

fn danificada(tecnico: &Value) -> bool {
    tecnico.pointer("/feito/midias").and_then(Value::as_array).into_iter().flatten().any(|m| m.get("erros_decodificacao").and_then(Value::as_u64).unwrap_or(0) > 0)
}

/// A regra do §10.4, passo a passo: plano → quadros compostos (o primeiro slot diferente) → PCM → arquivo.
/// Fonte danificada só tira a comparação quando há diferença (o decodificador não repete o que reconstrói).
/// O PCM só é prova com o mesmo ffmpeg (contratos-fase0 (h) 7); o arquivo, na mesma classe de máquina.
pub(crate) fn classificar(a: &Fatos, b: &Fatos, plano_igual: bool, fps: Racional) -> Comparacao {
    if !plano_igual {
        return Comparacao::NaoComparavel { motivo: MOTIVO_PLANO.into() };
    }
    let fonte = a.danificada || b.danificada;
    let primeiro = a.compostos.iter().zip(&b.compostos).position(|(x, y)| x != y).or_else(|| (a.compostos.len() != b.compostos.len()).then(|| a.compostos.len().min(b.compostos.len())));
    if let Some(q) = primeiro {
        if fonte {
            return Comparacao::NaoComparavel { motivo: MOTIVO_FONTE.into() };
        }
        return Comparacao::Diferente { primeiro_q: q as u64, tempo: tempo::instante_do_slot(q as u64, fps) };
    }
    let mesmo_ffmpeg = a.ffmpeg == b.ffmpeg;
    if a.audio != b.audio {
        if fonte {
            return Comparacao::NaoComparavel { motivo: MOTIVO_FONTE.into() };
        }
        if !mesmo_ffmpeg {
            return Comparacao::NaoComparavel { motivo: MOTIVO_SOM.into() };
        }
        // os quadros batem e o som não: diferente desde o começo (o PCM não tem lista por slot)
        return Comparacao::Diferente { primeiro_q: 0, tempo: 0.0 };
    }
    if a.arquivo == b.arquivo {
        return Comparacao::Igual;
    }
    let mut d: Vec<String> = Vec::new();
    let gpu = executar::e_gpu(&a.codificador) || executar::e_gpu(&b.codificador);
    for (nome, difere) in [
        ("codificador", a.codificador != b.codificador),
        ("argumentos", a.argumentos != b.argumentos),
        ("ffmpeg", !mesmo_ffmpeg),
        ("cpu", a.cpu != b.cpu),
        ("gpu", gpu && a.gpu != b.gpu),
    ] {
        if difere {
            d.push(nome.into());
        }
    }
    // mesma classe de máquina e arquivo diferente: fica dito
    if d.is_empty() {
        d.push("arquivo".into());
    }
    if a.decodificado != b.decodificado {
        d.push("decodificado".into());
    }
    Comparacao::ConteudoIgual { diferencas: d }
}

/// Grafos, listas e argumentos de leitura dos dois planos são os mesmos? O codificador fica de fora (NVENC contra
/// x264 compara pelo conteúdo): em `argumentos.json` saem `codificador` e `chave` de cada bloco.
pub(crate) fn planos_iguais(registrado: &Path, novo: &Path) -> Result<bool> {
    let lista = |d: &Path| -> Result<Vec<String>> {
        let pai = d.parent().unwrap_or(d);
        let nome = d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(fila::arquivos_em(pai, &nome)?.into_iter().map(|r| r[nome.len()..].trim_start_matches('/').to_string()).collect())
    };
    let (a, b) = (lista(registrado)?, lista(novo)?);
    if a != b {
        return Ok(false);
    }
    for rel in &a {
        let (x, y) = (fs::read(registrado.join(rel))?, fs::read(novo.join(rel))?);
        let igual = if rel == "argumentos.json" {
            let sem = |b: &[u8]| -> Result<Value> {
                let mut v: Value = serde_json::from_slice(b)?;
                for bloco in v.get_mut("blocos").and_then(Value::as_array_mut).into_iter().flatten() {
                    if let Some(o) = bloco.as_object_mut() {
                        o.remove("codificador");
                        o.remove("chave");
                    }
                }
                Ok(v)
            };
            sem(&x)? == sem(&y)?
        } else {
            x == y
        };
        if !igual {
            return Ok(false);
        }
    }
    Ok(true)
}

/// O que o projeto do retrato cita (densos, `.trf`, mídia própria), reposto na pasta do projeto quando falta (projeto
/// excluído, O7): o motor e o retrato novo leem de lá. Só o que foi posto aqui sai no fim.
struct Repostos(Vec<PathBuf>);

impl Repostos {
    fn por(ws: &Path, projeto_id: &str, retrato: &Path, m: &Manifesto) -> Result<Repostos> {
        let pasta = armazem::pasta_projeto(ws, projeto_id);
        let mut r = Repostos(Vec::new());
        for it in m.itens.iter().filter(|i| !i.de.is_empty()) {
            if !executar::relativo_seguro(&it.de) || !executar::relativo_seguro(&it.arquivo) {
                return Err(erro(format!("{}: {}", codigo::RETRATO_DIVERGENTE, it.arquivo)));
            }
            let destino = pasta.join(&it.de);
            if destino.exists() {
                continue;
            }
            if let Some(pai) = destino.parent() {
                fs::create_dir_all(pai)?;
            }
            let origem = retrato.join(&it.arquivo);
            if fs::hard_link(&origem, &destino).is_err() {
                fila::copiar_inteiro(&mut fs::File::open(&origem)?, &destino)?;
            }
            r.0.push(destino);
        }
        Ok(r)
    }
}

impl Drop for Repostos {
    fn drop(&mut self) {
        for a in &self.0 {
            let _ = fs::remove_file(a);
            // pastas que ficaram vazias (`rastreios/<id>`, `rastreios`)
            let mut pai = a.parent();
            for _ in 0..2 {
                let Some(d) = pai else { break };
                if fs::remove_dir(d).is_err() {
                    break;
                }
                pai = d.parent();
            }
        }
    }
}

/// A tarefa `Refazer` da fila: renderiza de novo pelo retrato, compara com o registrado e publica a refeita.
pub(crate) fn rodar(fila: &Fila, tarefa: &Tarefa, pedido: &PedidoRender, trabalho: &Path, cancelar: &AtomicBool, progresso: &mut dyn FnMut(Progresso)) -> Result<Estado> {
    let origem = {
        let mut g = PEDIDOS.lock().unwrap_or_else(|e| e.into_inner());
        let i = g.iter().position(|(t, _)| *t == tarefa.id).ok_or_else(|| erro(format!("tarefa_ausente: {}", tarefa.id)))?;
        g.remove(i).1
    };
    #[cfg(test)]
    let sem_gpu = testes::FORCAR_CPU.lock().unwrap_or_else(|e| e.into_inner()).contains(&origem);
    #[cfg(not(test))]
    let sem_gpu = false;
    rodar_com(fila, tarefa, pedido, trabalho, cancelar, progresso, &origem, sem_gpu)
}

/// `rodar` com a saída de origem dada; `sem_gpu` força o codificador de CPU (os testes comparam NVENC com x264).
#[allow(clippy::too_many_arguments)]
pub(crate) fn rodar_com(
    fila: &Fila,
    tarefa: &Tarefa,
    pedido: &PedidoRender,
    trabalho: &Path,
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(Progresso),
    origem: &str,
    sem_gpu: bool,
) -> Result<Estado> {
    let ws = pedido.ws.as_path();
    let ausente = || erro(format!("saida_ausente: {origem}"));
    let id = uuid::Uuid::parse_str(origem).map_err(|_| ausente())?;
    let s = {
        let (conn, occ) = abrir_banco(ws)?;
        video_edit_output_repo::obter(&conn, &occ, &id)?.ok_or_else(ausente)?
    };
    let dir_retrato = fila::pasta_retrato(ws, &s.projeto_id, origem)?;
    // 1. o retrato, conferido de novo na hora de rodar (pode ter mudado desde o pedido)
    let (projeto, m) = retrato(ws, &s.projeto_id, origem)?;
    if projeto != pedido.projeto {
        return Err(erro(format!("{}: projeto.json", codigo::RETRATO_DIVERGENTE)));
    }
    let tecnico = ler_json(&paths::resolver_relativo(ws, s.tecnico_rel.as_deref().ok_or_else(ausente)?))?;
    let antes = Fatos::da_saida(&s, &dir_retrato, &tecnico)?;
    let p = armazem::tipado(&pedido.projeto)?;
    fila::exigir_gravacao(ws)?;
    let _repostos = Repostos::por(ws, &s.projeto_id, &dir_retrato, &m)?;
    let prep = executar::preparar_com(ws, &p, &pedido.cache, true, Some(cancelar))?;
    // aviso pericial que a exportação de origem não teve (regra mais nova que ela): refaz, mas não compara
    let regra_nova = {
        let ctx = prep.contexto();
        let linha = tempo::resolver(&p, &ctx);
        validar::sem_ciencia(&validar::validar(&p, &linha, &ctx, Some(&pedido.alvo)), &pedido.ciencias)
    };
    let auto = matches!(&pedido.alvo, Alvo::Final { exportacao } if exportacao.codificador == EscolhaCodificador::Auto) && !sem_gpu;
    // blocos, camadas e quadros feitos de novo, num cache vazio só desta tarefa
    let cache = CacheProprio(trabalho.join(CACHE_PROPRIO));
    let _ = fs::remove_dir_all(&cache.0);
    fs::create_dir_all(&cache.0)?;
    let amb = Ambiente {
        cap: prep.cap.clone(),
        trabalho: trabalho.to_path_buf(),
        cache: cache.0.clone(),
        caso: pedido.ws.clone(),
        pasta_projeto: prep.pasta_projeto.clone(),
        paralelos: executar::paralelos(auto && prep.cap.gpu.is_some()),
        identidade: fila::identidade(ws, &prep.cap),
        // a ciência é a da exportação de origem, com a hora dela
        ciencias: ciencias_de(&tecnico),
    };
    // 2 e 3. recompila com o mesmo alvo e renderiza
    let compilar = |cpu: bool| fila::compilar_alvo(&p, &prep, &pedido.alvo, cpu || sem_gpu);
    let (plano, res) = executar::executar_com_queda(&compilar, &amb, cancelar, progresso)?;
    let dir_novo = trabalho.join("grafos-refeitos");
    let _ = fs::remove_dir_all(&dir_novo);
    fila::gravar_grafos(&dir_novo, &plano)?;
    let plano_igual = planos_iguais(&dir_retrato.join("grafos"), &dir_novo)?;
    // 4 e 5. compara e registra
    let c = match regra_nova.is_empty() {
        true => classificar(&antes, &Fatos::do_render(&plano, &res, &amb)?, plano_igual, plano.canvas.fps),
        false => Comparacao::NaoComparavel { motivo: format!("{MOTIVO_CIENCIA}: {}", regra_nova.join(", ")) },
    };
    fila::publicar_da_fila(fila, tarefa, pedido, &p, &prep, &amb, &plano, &res, cancelar, progresso, Some((id, &c)))
}

#[cfg(test)]
pub(crate) mod testes {
    use serde_json::json;

    use super::*;
    use crate::edicao::pacote::ponta::{self, Caso};
    use crate::edicao::testes::aceite::{efeito, trecho};
    use crate::edicao::testes::fontes::{self, Fonte};

    /// Saídas de origem cuja refeita os testes forçam para a CPU (NVENC contra x264).
    pub(crate) static FORCAR_CPU: Mutex<Vec<String>> = Mutex::new(Vec::new());

    const FPS: Racional = Racional { num: 30, den: 1 };

    fn fatos(compostos: &[&str], audio: &str, arquivo: &str) -> Fatos {
        Fatos {
            compostos: compostos.iter().map(|s| s.to_string()).collect(),
            audio: Some(audio.into()),
            arquivo: arquivo.into(),
            decodificado: Some(format!("d-{arquivo}")),
            codificador: "libx264".into(),
            argumentos: vec!["-crf".into(), "18".into()],
            ffmpeg: "f1".into(),
            cpu: vec!["avx2".into()],
            gpu: Some(("RTX".into(), "610".into())),
            danificada: false,
        }
    }

    /// A regra do §10.4 caso a caso (sem render): cada resultado e a ordem em que as provas valem.
    #[test]
    fn classificar_os_quatro_resultados() {
        let a = fatos(&["q0", "q1", "q2", "q3"], "pcm", "arq");
        assert_eq!(classificar(&a, &a.clone(), true, FPS), Comparacao::Igual);
        // plano diferente: nem olha o resto
        assert_eq!(classificar(&a, &a.clone(), false, FPS), Comparacao::NaoComparavel { motivo: MOTIVO_PLANO.into() });
        // quadro diferente: o primeiro slot, com o tempo dele
        let mut b = fatos(&["q0", "q1", "x2", "x3"], "pcm", "outro");
        assert_eq!(classificar(&a, &b, true, FPS), Comparacao::Diferente { primeiro_q: 2, tempo: 2.0 / 30.0 });
        assert_eq!(classificar(&a, &b, true, Racional { num: 30000, den: 1001 }), Comparacao::Diferente { primeiro_q: 2, tempo: 2.0 * 1001.0 / 30000.0 });
        // contagem diferente: o primeiro slot que falta
        assert_eq!(classificar(&a, &fatos(&["q0", "q1", "q2"], "pcm", "outro"), true, FPS), Comparacao::Diferente { primeiro_q: 3, tempo: 0.1 });
        // a mesma diferença com fonte danificada (de qualquer lado) não prova nada
        b.danificada = true;
        assert_eq!(classificar(&a, &b, true, FPS), Comparacao::NaoComparavel { motivo: MOTIVO_FONTE.into() });
        let mut a_ruim = a.clone();
        a_ruim.danificada = true;
        assert_eq!(classificar(&a_ruim, &fatos(&["q0", "q1", "x2", "x3"], "pcm", "outro"), true, FPS), Comparacao::NaoComparavel { motivo: MOTIVO_FONTE.into() });
        // fonte danificada que repetiu tudo é igual
        assert_eq!(classificar(&a_ruim, &a_ruim.clone(), true, FPS), Comparacao::Igual);
        // som: com o mesmo ffmpeg é diferença; com outro, o PCM não é prova
        let som = fatos(&["q0", "q1", "q2", "q3"], "outro pcm", "outro");
        assert_eq!(classificar(&a, &som, true, FPS), Comparacao::Diferente { primeiro_q: 0, tempo: 0.0 });
        let mut som_7 = som.clone();
        som_7.ffmpeg = "f2".into();
        assert_eq!(classificar(&a, &som_7, true, FPS), Comparacao::NaoComparavel { motivo: MOTIVO_SOM.into() });
        // conteúdo igual, arquivo diferente: diz por quê
        let mut nvenc = fatos(&["q0", "q1", "q2", "q3"], "pcm", "arq-nvenc");
        (nvenc.codificador, nvenc.argumentos) = ("h264_nvenc".into(), vec!["-cq".into(), "21".into()]);
        assert_eq!(classificar(&nvenc, &a, true, FPS), Comparacao::ConteudoIgual { diferencas: vec!["codificador".into(), "argumentos".into(), "decodificado".into()] });
        let mut outra_placa = nvenc.clone();
        (outra_placa.arquivo, outra_placa.gpu) = ("arq-2".into(), Some(("RTX".into(), "620".into())));
        assert_eq!(classificar(&nvenc, &outra_placa, true, FPS), Comparacao::ConteudoIgual { diferencas: vec!["gpu".into()] });
        let mut outra_cpu = fatos(&["q0", "q1", "q2", "q3"], "pcm", "arq-avx512");
        (outra_cpu.cpu, outra_cpu.decodificado) = (vec!["avx2".into(), "avx512f".into()], a.decodificado.clone());
        assert_eq!(classificar(&a, &outra_cpu, true, FPS), Comparacao::ConteudoIgual { diferencas: vec!["cpu".into()] });
        // placa diferente não pesa no x264
        let mut so_placa = a.clone();
        (so_placa.arquivo, so_placa.gpu, so_placa.decodificado) = ("arq-3".into(), None, a.decodificado.clone());
        assert_eq!(classificar(&a, &so_placa, true, FPS), Comparacao::ConteudoIgual { diferencas: vec!["arquivo".into()] });
        // o literal do fio
        assert_eq!(serde_json::to_value(Comparacao::ConteudoIgual { diferencas: vec!["cpu".into()] }).unwrap(), json!({ "tipo": "conteudo_igual", "diferencas": ["cpu"] }));
        assert_eq!(serde_json::to_value(Comparacao::NaoComparavel { motivo: "plano".into() }).unwrap(), json!({ "tipo": "nao_comparavel", "motivo": "plano" }));
    }

    /// Planos comparados pelos grafos, listas e argumentos de leitura; codificador e chave do bloco não pesam.
    #[test]
    fn planos_iguais_sem_olhar_o_codificador() {
        let d = fontes::pasta_temporaria("p18-planos");
        let gravar = |nome: &str, grafo: &str, cod: &str, antes: &str| {
            let g = d.join(nome);
            fs::create_dir_all(g.join("b000")).unwrap();
            fs::write(g.join("b000/b000.txt"), grafo).unwrap();
            let args = json!({ "blocos": [{ "indice": 0, "chave": format!("chave de {cod}"), "codificador": ["-c:v", cod], "entradas": [{ "antes": [antes], "fonte": "midia:m1" }] }], "juncao": { "tipo": "concat" } });
            fs::write(g.join("argumentos.json"), serde_json::to_vec_pretty(&args).unwrap()).unwrap();
            g
        };
        let a = gravar("a", "[0:v]trim[v]", "h264_nvenc", "-ss 1");
        assert!(planos_iguais(&a, &gravar("b", "[0:v]trim[v]", "libx264", "-ss 1")).unwrap(), "só o codificador mudou");
        assert!(!planos_iguais(&a, &gravar("c", "[0:v]trim=1[v]", "h264_nvenc", "-ss 1")).unwrap(), "grafo");
        assert!(!planos_iguais(&a, &gravar("d", "[0:v]trim[v]", "h264_nvenc", "-ss 2")).unwrap(), "leitura");
        let e = gravar("e", "[0:v]trim[v]", "h264_nvenc", "-ss 1");
        fs::write(e.join("b000/a_mais.ffconcat"), "x").unwrap();
        assert!(!planos_iguais(&a, &e).unwrap(), "arquivo a mais");
    }

    fn projeto_simples(caso: &Caso, titulo: &str, fonte: &std::path::Path, efeitos: impl Fn(&crate::edicao::quadros::TabelaQuadros) -> Value) -> Value {
        let (m, tab) = caso.midia("m1", fonte);
        ponta::projeto(titulo, true, vec![m], vec![trecho("c1", "m1", &tab, 30, 75, 1.0, efeitos(&tab))], json!({}))
    }

    /// Pede a refeita, espera e devolve (saída de origem, saída refeita). Toda refeita renderiza de novo: nenhum bloco
    /// do cache no `tecnico.json` nem na marca da fila, e o cache dela some no fim.
    fn refazer(caso: &Caso, origem: &str) -> (SaidaRegistrada, SaidaRegistrada) {
        let t = pedir(&caso.ws, &caso.cache(), origem).expect("edicao_refazer");
        let fim = fila::esperar(&t).expect("esperar");
        let nova = caso.saida(&ponta::id_da(&fim));
        assert_eq!(nova.refeita_de.as_deref(), Some(origem));
        let tecnico = ler_json(&paths::resolver_relativo(&caso.ws, nova.tecnico_rel.as_deref().unwrap())).unwrap();
        let blocos = tecnico["execucao"]["blocos"].as_array().expect("blocos no tecnico.json");
        assert!(!blocos.is_empty() && blocos.iter().all(|b| b["reaproveitado"] == json!(false)), "refeita com bloco do cache: {blocos:?}");
        assert_eq!(fila::listar().into_iter().find(|x| x.id == t).map(|x| x.reaproveitada), Some(false));
        assert!(!caso.cache().join("render").join(&t).join(CACHE_PROPRIO).exists(), "o cache da refeita ficou");
        (caso.saida(origem), nova)
    }

    /// Aceite `Igual` (cpu): a refeita sai byte a byte igual, vira saída nova ligada à de origem, com o resultado no
    /// retrato dela, na lista de saídas e no log; o projeto pode até ter sido excluído (lê só do retrato).
    #[test]
    fn igual() {
        let Some(caso) = ponta::caso("refazer-igual") else { return };
        let doc = projeto_simples(&caso, "Refazer", &fontes::gerar(Fonte::Mp4).unwrap(), |_| json!([]));
        let (fim, _) = caso.exportar(&doc, ponta::exportacao("juri", "cpu", Value::Null, "a3"));
        let origem = ponta::id_da(&fim);
        let (a, b) = refazer(&caso, &origem);
        assert_eq!(b.comparacao, Some(Comparacao::Igual), "{}", caso.cap.versao);
        assert_eq!((&a.sha256, &a.composto_sha256, &a.decodificado_sha256, &a.audio_sha256), (&b.sha256, &b.composto_sha256, &b.decodificado_sha256, &b.audio_sha256));
        assert_ne!(a.pasta, b.pasta, "a refeita nunca sobrescreve a de origem");
        assert_eq!(a.comparacao, None);
        assert_eq!(serde_json::from_slice::<Comparacao>(&fs::read(caso.retrato(&b).join(COMPARACAO)).unwrap()).unwrap(), Comparacao::Igual);
        assert_eq!(caso.contar("SELECT COUNT(*) FROM video_operation_logs WHERE action = 'edicao.refazer'"), 1);
        assert_eq!(caso.contar("SELECT COUNT(*) FROM video_edit_outputs WHERE refeita_de IS NOT NULL"), 1);
        // saída a conferir, inexistente ou de retrato mexido não entra na fila
        assert!(pedir(&caso.ws, &caso.cache(), "nao-e-uuid").unwrap_err().to_string().contains("saida_ausente"));
        let arq = caso.retrato(&a).join("projeto.json");
        let bom = fs::read(&arq).unwrap();
        fs::write(&arq, b"{}").unwrap();
        let e = pedir(&caso.ws, &caso.cache(), &origem).unwrap_err().to_string();
        assert!(e.contains(codigo::RETRATO_DIVERGENTE), "{e}");
        fs::write(&arq, bom).unwrap();
        // o "refazer" lê só do retrato: sem a pasta do projeto dá o mesmo
        assert!(!crate::edicao::armazem::pasta_projeto(&caso.ws, doc["id"].as_str().unwrap()).join("projeto.json").exists());
        let (_, c) = refazer(&caso, &b.id);
        assert_eq!(c.comparacao, Some(Comparacao::Igual), "refeita da refeita");
    }

    /// `Igual` com camada (carimbo de tempo): atlas e camada feitos de novo no cache da refeita dão os mesmos quadros.
    #[test]
    fn igual_com_camada_feita_de_novo() {
        let Some(caso) = ponta::caso("refazer-camada") else { return };
        let doc = projeto_simples(&caso, "Carimbo", &fontes::gerar(Fonte::Mp4).unwrap(), |_| {
            json!([efeito("carimbo", "k1", json!({ "mostra": "tempo_fonte", "formato": "hms_ms", "relogio": null, "lugar": { "x": 0.03, "y": 0.97, "h": "esq", "v": "base" },
                "estilo": { "fonte": "mono", "negrito": false, "italico": false, "tamanho": 0.07, "cor": "#ffffff", "alinhamento": "esq", "caixa": { "cor": "#000000", "margem": 0.01, "raio": 0.0 }, "contorno": null } }))])
        });
        let (fim, _) = caso.exportar(&doc, ponta::exportacao("juri", "cpu", Value::Null, "a3"));
        let (a, b) = refazer(&caso, &ponta::id_da(&fim));
        assert_eq!(b.comparacao, Some(Comparacao::Igual), "{}", caso.cap.versao);
        assert_eq!((&a.sha256, &a.composto_sha256), (&b.sha256, &b.composto_sha256));
        let tecnico = ler_json(&paths::resolver_relativo(&caso.ws, b.tecnico_rel.as_deref().unwrap())).unwrap();
        assert!(tecnico["execucao"]["camadas"].as_array().is_some_and(|c| !c.is_empty()), "sem camada: {}", tecnico["execucao"]["camadas"]);
    }

    /// Aceite `Igual` em projeto estabilizado: a análise guardada no retrato (`.trf`) dá os mesmos quadros.
    #[test]
    fn igual_em_projeto_estabilizado() {
        let Some(caso) = ponta::caso("refazer-estab") else { return };
        if !caso.cap.filtros.contains("vidstabdetect") || !caso.cap.filtros.contains("vidstabtransform") {
            eprintln!("aviso: teste dispensado, {} sem vidstab", caso.cap.versao);
            return;
        }
        let doc = projeto_simples(&caso, "Estabilizado", &fontes::gerar(Fonte::Mp4).unwrap(), |tab| {
            let analise = json!({ "de_s": tab.tempo(30), "ate_s": tab.tempo(75), "de_pts": tab.pts[30], "ate_pts": tab.pts[75] });
            json!([efeito("estabilizar", "s1", json!({ "suavizar": 15, "borda": "preta", "analise": analise, "zoom_pct": 0.0, "trf_sha256": null }))])
        });
        let (fim, _) = caso.exportar(&doc, ponta::exportacao("juri", "cpu", Value::Null, "a3"));
        let origem = ponta::id_da(&fim);
        let a = caso.saida(&origem);
        let m: Manifesto = serde_json::from_slice(&fs::read(caso.retrato(&a).join("manifesto.json")).unwrap()).unwrap();
        assert!(m.itens.iter().any(|i| i.papel == fila::PapelRetrato::Trf), "o retrato guarda o .trf: {:?}", m.itens.iter().map(|i| &i.arquivo).collect::<Vec<_>>());
        let (a, b) = refazer(&caso, &origem);
        assert_eq!(b.comparacao, Some(Comparacao::Igual), "{}", caso.cap.versao);
        assert_eq!((&a.sha256, &a.composto_sha256), (&b.sha256, &b.composto_sha256));
    }

    /// Aceite `ConteudoIgual`: a de origem pelo NVENC, a refeita pelo x264. Os quadros compostos e o PCM batem; o
    /// arquivo não, e o resultado diz que foi o codificador. Sem NVENC aprovado neste binário, a dispensa fica registrada.
    #[test]
    fn conteudo_igual() {
        let Some(caso) = ponta::caso("refazer-nvenc") else { return };
        let doc = projeto_simples(&caso, "NVENC", &fontes::gerar(Fonte::Mp4).unwrap(), |_| json!([]));
        // a sonda de GPU feita agora (os testes não têm o cache do app): a exportação `auto` pega o que ela aprovar
        let mut origem = None;
        for _ in 0..3 {
            let cap = crate::edicao::ffmpeg::sonda::testar_de_novo().expect("sonda de GPU");
            if cap.gpu.as_ref().is_none_or(|g| g.codificador != "h264_nvenc") {
                eprintln!("aviso: teste dispensado, sem NVENC aprovado em {} (gpu {:?}; reprovados {:?})", cap.versao, cap.gpu.as_ref().map(|g| &g.codificador), cap.reprovados.iter().map(|r| &r.codificador).collect::<Vec<_>>());
                return;
            }
            let (fim, _) = caso.exportar(&doc, ponta::exportacao("juri", "auto", Value::Null, "a3"));
            let id = ponta::id_da(&fim);
            // outro teste pode ter refeito a sonda no meio (estado do processo): tenta de novo
            if caso.saida(&id).codificador == "h264_nvenc" {
                origem = Some(id);
                break;
            }
        }
        let origem = origem.expect("NVENC aprovado e a exportação `auto` não o usou");
        FORCAR_CPU.lock().unwrap().push(origem.clone());
        let (a, b) = refazer(&caso, &origem);
        assert_eq!((a.codificador.as_str(), b.codificador.as_str()), ("h264_nvenc", "libx264"));
        assert_eq!((&a.composto_sha256, &a.audio_sha256), (&b.composto_sha256, &b.audio_sha256), "o conteúdo é o mesmo");
        assert_ne!(a.sha256, b.sha256);
        match &b.comparacao {
            // mesma máquina, mesmo ffmpeg, mesma placa: só o codificador (e o que decorre dele) difere
            Some(Comparacao::ConteudoIgual { diferencas }) => assert_eq!(diferencas, &["codificador", "argumentos", "decodificado"]),
            outro => panic!("{outro:?}"),
        }
    }

    /// Troca o hash do slot `q` na lista de compostos do retrato e acerta o manifesto: a exportação de origem "tinha"
    /// outro quadro ali.
    fn trocar_slot(retrato: &std::path::Path, q: usize) {
        let lista = retrato.join(COMPOSTOS);
        let mut l: Vec<String> = fs::read_to_string(&lista).unwrap().lines().map(String::from).collect();
        l[q] = "0".repeat(64);
        fs::write(&lista, l.join("\n") + "\n").unwrap();
        let arq = retrato.join("manifesto.json");
        let mut m: Manifesto = serde_json::from_slice(&fs::read(&arq).unwrap()).unwrap();
        let it = m.itens.iter_mut().find(|i| i.papel == fila::PapelRetrato::Compostos).expect("compostos no manifesto");
        (it.sha256, it.tamanho) = (crate::hashing::sha256::sha256_file(&lista).unwrap(), fs::metadata(&lista).unwrap().len());
        fs::write(&arq, serde_json::to_vec_pretty(&m).unwrap()).unwrap();
    }

    /// Aceite `Diferente` no slot certo: o primeiro slot cujo quadro composto não bate, com o tempo dele.
    #[test]
    fn diferente_no_slot() {
        let Some(caso) = ponta::caso("refazer-diferente") else { return };
        let doc = projeto_simples(&caso, "Diferente", &fontes::gerar(Fonte::Mp4).unwrap(), |_| json!([]));
        let (fim, _) = caso.exportar(&doc, ponta::exportacao("juri", "cpu", Value::Null, "a3"));
        let origem = ponta::id_da(&fim);
        trocar_slot(&caso.retrato(&caso.saida(&origem)), 17);
        trocar_slot(&caso.retrato(&caso.saida(&origem)), 31);
        let (_, b) = refazer(&caso, &origem);
        assert_eq!(b.comparacao, Some(Comparacao::Diferente { primeiro_q: 17, tempo: 17.0 / 30.0 }), "{}", caso.cap.versao);
    }

    /// Exportação de antes de uma regra pericial nova (cópia de gravador com taxa suposta, sem a ciência de
    /// `taxa_suposta`): a refeita sai "Não comparável" com o código no motivo, sem falhar.
    #[test]
    fn regra_nova_sem_ciencia_da_nao_comparavel() {
        let Some(caso) = ponta::caso("refazer-regra-nova") else { return };
        let (mut m, tab) = caso.midia("m1", &fontes::gerar(Fonte::Mp4).unwrap());
        m["derivacao"] = json!({ "tipo": "copia_dvr", "de_sha256": m["sha256"], "raiz_sha256": m["sha256"], "comando": "ffmpeg -c copy",
            "com_perdas": false, "taxa_suposta": { "num": 25, "den": 1 } });
        let doc = ponta::projeto("Gravador", true, vec![m], vec![trecho("c1", "m1", &tab, 30, 75, 1.0, json!([]))], json!({}));
        let (fim, ciencias) = caso.exportar(&doc, ponta::exportacao("juri", "cpu", Value::Null, "a3"));
        assert!(ciencias.iter().any(|c| c == codigo::TAXA_SUPOSTA), "{ciencias:?}");
        let origem = ponta::id_da(&fim);
        // a exportação "de antes" da regra: o tecnico.json sem essa ciência
        let arq = paths::resolver_relativo(&caso.ws, caso.saida(&origem).tecnico_rel.as_deref().unwrap());
        let mut t = ler_json(&arq).unwrap();
        t["execucao"]["ciencias"].as_array_mut().unwrap().retain(|c| c["codigo"] != codigo::TAXA_SUPOSTA);
        // a pasta da saída é somente leitura
        let mut perm = fs::metadata(&arq).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        fs::set_permissions(&arq, perm).unwrap();
        fs::write(&arq, serde_json::to_vec_pretty(&t).unwrap()).unwrap();
        let (_, b) = refazer(&caso, &origem);
        assert_eq!(b.comparacao, Some(Comparacao::NaoComparavel { motivo: format!("{MOTIVO_CIENCIA}: {}", codigo::TAXA_SUPOSTA) }), "{}", caso.cap.versao);
    }

    /// Aceite `NaoComparavel` com fonte danificada: a mesma diferença de quadro, numa exportação cuja fonte tem erro de
    /// decodificação, não é atribuída ao vídeo (o decodificador não repete o que reconstrói).
    #[test]
    fn nao_comparavel() {
        let Some(caso) = ponta::caso("refazer-danificada") else { return };
        // a fonte da matriz com um pedaço do fim do fluxo zerado
        let mut bytes = fs::read(fontes::gerar(Fonte::Mp4).unwrap()).unwrap();
        let n = bytes.len();
        let mdat = bytes.windows(4).position(|w| w == b"mdat").expect("mdat");
        let (de, ate) = (mdat + (n - mdat) * 8 / 10, mdat + (n - mdat) * 8 / 10 + 4096);
        bytes[de..ate.min(n)].fill(0);
        let ruim = fontes::pasta_temporaria("p18-danificada").join("danificada.mp4");
        fs::write(&ruim, bytes).unwrap();
        let doc = projeto_simples(&caso, "Danificada", &ruim, |_| json!([]));
        let (fim, ciencias) = caso.exportar(&doc, ponta::exportacao("juri", "cpu", Value::Null, "a3"));
        let origem = ponta::id_da(&fim);
        let a = caso.saida(&origem);
        let tecnico = ler_json(&paths::resolver_relativo(&caso.ws, a.tecnico_rel.as_deref().unwrap())).unwrap();
        assert!(danificada(&tecnico), "{}: a fonte não ficou danificada: {}", caso.cap.versao, tecnico["feito"]["midias"]);
        assert!(ciencias.iter().any(|c| c == "fonte_danificada"), "{ciencias:?}");
        trocar_slot(&caso.retrato(&a), 9);
        let (_, b) = refazer(&caso, &origem);
        assert_eq!(b.comparacao, Some(Comparacao::NaoComparavel { motivo: MOTIVO_FONTE.into() }), "{}", caso.cap.versao);
        // a ciência da refeita é a da exportação de origem, com a mesma hora
        let refeito = ler_json(&paths::resolver_relativo(&caso.ws, b.tecnico_rel.as_deref().unwrap())).unwrap();
        assert_eq!(refeito["execucao"]["ciencias"], tecnico["execucao"]["ciencias"]);
    }
}
