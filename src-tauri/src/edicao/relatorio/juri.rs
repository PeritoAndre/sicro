//! Dados do relatório do júri (§10.3, contrato `sicro.relatorio_juri/2` = o `/1` de `pdf-relatorio.md` §8 mais
//! cobertura por gravação, resumo de IA, selos desligados e comandos de conferência), a partir do `tecnico.json`
//! FINAL; as frases vêm de `frases.rs`. Formatação pt-BR (vírgula decimal, datas, "0:07,500") fica aqui. Dono: P1.R.
//!
//! O que o `/2` muda no `/1`: `originais[]` ganha `especie`, `recebido`, `relogio`, `derivacao`, `cobertura` e
//! `tecnico.{cor, observacoes}`, perde `copia_trabalho`, e `tamanho`/`imagem` passam a aceitar nulo (o `tecnico.json`
//! não os traz); `roteiro[].selos`; `ocultacoes[].quadros`; e os campos novos `revisado_por`, `ia_resumo`, `relogios`,
//! `medidas`, `selos_desligados`, `conferir`, `conferencia_ocultacoes`, `modelos_ia`, `pacote`, `referencias`.
// CONTRATO: acréscimo da Fase 2 (P2.R) ao `/2`, só campos novos (dados e modelo andam juntos no binário):
// `ocultacoes[].{n, arquivo, forma, regiao}` (planilha do 18-M §11; `filtro` sem a forma), `modelos_ia[].{execucao,
// desvio}` (o provedor sai de `origem`) e `rastreios[]` (anexo H: cada rastreio em que um programa atuou).
// CONTRATO: acréscimo do P4.R (bloco 6.4, só campos novos): `originais[].tecnico.{derivacao, comando}` (como a cópia
// foi feita: ffmpeg, taxa suposta, relógio do gravador; e o comando registrado).
// CONTRATO: acréscimo da revisão do relatório (só campos novos): `programa` ("SICRO 6.0.0"), `roteiro[].textos` (o
// texto das telas de texto, do `projeto.json`), `roteiro[].efeitos[]` passa a `{ texto, marcas[] }` (marcas com a cor
// para a amostra) e `originais[].tecnico.intervalos_usados` (percentis dos intervalos nos trechos usados, do `mapa.csv`).

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Datelike, FixedOffset, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::frases::{self, condicao, modelo, tipo, Classe};
use super::legivel::{self, Marca};
use super::tecnico;
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::ModoPdf;
use crate::edicao::rastreio::{Corrida, MotorRastreio, Sentido};
use crate::error::{Result, SicroError};
use crate::relatorio_pdf::{Entrada, Modelo};

/// Esquema dos dados do PDF.
pub const ESQUEMA_JURI: &str = "sicro.relatorio_juri/2";

/// Miniatura de um item do roteiro: quadro final do primeiro slot (`Alvo::Quadro`, 480 px), em JPEG.
#[derive(Debug, Clone, PartialEq)]
pub struct Miniatura {
    pub n: u32,
    pub jpeg: Vec<u8>,
}

/// Arquivo do pacote listado no anexo A (o PDF é a raiz): caminho relativo à pasta da exportação e SHA-256.
#[derive(Debug, Clone, PartialEq)]
pub struct ArquivoDoPacote {
    pub rel: String,
    pub sha256: String,
}

/// Lista dos quadros decodificados no pacote (§10.1): o que o comando de conferência dos quadros reproduz.
const LISTA_DE_QUADROS: &str = "quadros.decodificados.sha256";
/// Menos intervalos que isto nos trechos usados: sem percentis (não dizem nada).
const MIN_INTERVALOS: usize = 20;
/// Texto do sobreposto que identifica a linha dele na lista de acréscimos.
const MAX_TEXTO_SOBREPOSTO: usize = 60;

// ---------------------------------------------------------------------------
// O que se lê do `tecnico.json` (campo que falta vale o padrão: um relatório de versão anterior ainda gera)

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct R {
    num: i64,
    den: i64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Tecnico {
    feito: Feito,
    execucao: Execucao,
    renderizado_em: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Feito {
    projeto: FProjeto,
    saida: FSaida,
    midias: Vec<FMidia>,
    roteiro: Vec<FItem>,
    ocultacoes: Vec<FOcultacao>,
    audio: FAudio,
    legendas: FLegendas,
    sincronias: Vec<FSincronia>,
    rastreios: Vec<FRastreio>,
    ia: Vec<FIa>,
    avisos: Vec<FAviso>,
    selos_desligados: Vec<String>,
    revisado_por: String,
    fecho: Option<FTransicao>,
    sobrepostos: Vec<FSobreposto>,
}

/// `feito.sobrepostos[]`: título ou imagem por cima da principal.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FSobreposto {
    clipe: String,
    tipo: String,
    nome: String,
    na_saida: FNaSaida,
    textos: Vec<String>,
    selos: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FProjeto {
    sha256: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FSaida {
    largura: u32,
    altura: u32,
    fps: R,
    duracao_s: f64,
    predefinicao: Option<String>,
    com_perdas: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FMidia {
    id: String,
    nome: String,
    sha256: String,
    derivacao: Option<FDerivacao>,
    recebido: FRecebido,
    relogio_desvio_s: Option<f64>,
    gravador: Option<String>,
    cor: FCor,
    conteiner: String,
    codec: String,
    pix_fmt: String,
    sar: R,
    giro: i64,
    declarado: FDeclarado,
    medido: FMedido,
    discrepancias: Vec<String>,
    erros_decodificacao: u64,
    usado: Vec<FFaixa>,
    usado_total_s: f64,
    largura: u32,
    altura: u32,
    tamanho: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FDerivacao {
    tipo: String,
    raiz_sha256: String,
    comando: String,
    com_perdas: bool,
    inicio_epoca_s: Option<f64>,
    taxa_suposta: Option<R>,
    de_nome: Option<String>,
    ffmpeg_versao: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FRecebido {
    nativo: Option<bool>,
    obs: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FCor {
    matriz: String,
    faixa: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FDeclarado {
    r_frame_rate: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FMedido {
    tb: R,
    de_s: f64,
    ate_s: f64,
    quadros: u64,
    passo_min: i64,
    passo_med: i64,
    passo_max: i64,
    lacunas: Vec<(i64, i64)>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FFaixa {
    de_s: f64,
    ate_s: f64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FItem {
    n: u32,
    clipe: String,
    nome: String,
    tipo: String,
    na_saida: FNaSaida,
    origem: Option<FOrigem>,
    velocidade: Option<Value>,
    transicao: Option<FTransicao>,
    processamentos: Vec<FProc>,
    selos: Vec<String>,
    audio: Option<FAudioClipe>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FNaSaida {
    de_q: u64,
    n_q: u32,
    de_s: f64,
    ate_s: f64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FOrigem {
    midia: String,
    de_s: f64,
    ate_s: f64,
    pts_primeiro: i64,
    pts_ultimo: i64,
    lidos: u32,
    repetidos: u32,
    descartados: u32,
    mapa: Value,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FTransicao {
    tipo: String,
    duracao_s: f64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FProc {
    ordem: u32,
    nome: String,
    parametros: Value,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FAudioClipe {
    volume: Value,
    mudo: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FOcultacao {
    midia: String,
    clipes: Vec<String>,
    de_s: f64,
    ate_s: f64,
    modo: String,
    forma: String,
    intensidade: f64,
    conteudo: String,
    motivo: String,
    quadros: FQuadros,
    inverter: bool,
    margem: f64,
    borda_suave: f64,
    cor: String,
    rastreio: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FQuadros {
    ancora: u32,
    medido: u32,
    interpolado: u32,
    incerto: u32,
    segurado: u32,
    ancora_do_programa: u32,
    interpolado_entre_marcacoes: u32,
    interpolado_do_programa: u32,
    ausente: u32,
}

/// "quadros — na marcação do perito: 1 · medidos: 40 · …": as três primeiras sempre; as outras só com quadro.
/// `invertida`: o quadro sem região sai todo coberto.
fn quadros_ocultos(q: &FQuadros, invertida: bool) -> String {
    let parte = |m: &str, n: u32| frases::preencher(m, &[("n", &n.to_string())]);
    let mut v = vec![parte(modelo::QUADROS_MARCADOS, q.ancora)];
    if q.ancora_do_programa > 0 {
        v.push(parte(modelo::QUADROS_DO_PROGRAMA, q.ancora_do_programa));
    }
    if q.segurado > 0 {
        v.push(parte(modelo::QUADROS_SEGURADOS, q.segurado));
    }
    v.push(parte(modelo::QUADROS_MEDIDOS, q.medido));
    v.push(parte(modelo::QUADROS_INTERPOLADOS, q.interpolado));
    if q.medido + q.interpolado > 0 {
        v.push(parte(modelo::QUADROS_INCERTOS, q.incerto));
    }
    if q.ausente > 0 {
        v.push(parte(if invertida { modelo::QUADROS_AUSENTES_INVERTIDA } else { modelo::QUADROS_AUSENTES }, q.ausente));
    }
    frases::preencher(modelo::QUADROS_OCULTOS, &[("lista", &v.join(" · "))])
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FAudio {
    silencios: Vec<FSilencio>,
    mixagem: Option<Value>,
    trilhas: Vec<FTrilha>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FSilencio {
    midia: String,
    de_s: f64,
    ate_s: f64,
    modo: String,
    motivo: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FTrilha {
    papel: String,
    clipes: Vec<FClipeAudio>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FClipeAudio {
    de_q: u64,
    n_q: u32,
}

/// `modo` = `Exportacao.legendas` do render ("nenhuma" | "queimar" | "arquivo" | "ambos"); `n` = as que saíram.
/// `feito` antigo, sem o modo: o relatório não sabe se as legendas saíram e não fala delas. `ia` = as que saíram com
/// origem automática; `ia_conferidas` = dessas, as com conferência registrada.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FLegendas {
    origem: Option<FOrigemLegendas>,
    n: u32,
    modo: Option<String>,
    ia: u32,
    ia_conferidas: u32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FOrigemLegendas {
    motor: Option<String>,
    modelo_sha256: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FSincronia {
    referencia: String,
    outra: String,
    desloc_s: f64,
    deriva_ppm: f64,
    deriva_incerteza_ppm: f64,
    metodo: String,
    incerteza_s: Option<f64>,
    conferencias: Vec<Value>,
}

/// `feito.rastreios[]`. `ancoras` sem o campo = não se afirma quantas; `corridas` e `corridas_do_programa` leem
/// como `rastreio::Corrida` (B2).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FRastreio {
    id: String,
    origem: Value,
    ancoras: Option<u32>,
    denso_sha256: Option<String>,
    corridas: Vec<Value>,
    corridas_do_programa: Vec<Value>,
    confirmacao: Option<FConfirmacao>,
    ausencias: Vec<FAusencia>,
}

/// Confirmação do perito: só vira frase com todos os campos e presa às amostras do rastreio (`denso_sha256`).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FConfirmacao {
    por: Option<String>,
    em: Option<String>,
    denso_sha256: Option<String>,
    de_s: Option<f64>,
    ate_s: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FAusencia {
    de_s: f64,
    ate_s: f64,
    motivo: String,
}

#[derive(Debug, Default, Deserialize, Clone, PartialEq)]
#[serde(default)]
struct FIa {
    modelo: String,
    arquivos: Vec<FArquivo>,
    origem: String,
    commit: String,
    desvio: String,
    runtime: String,
    provedor: String,
    uso: String,
}

impl FIa {
    /// A entrada de `feito.ia` que a corrida daria (`ia::uso::da_corrida`, a regra do P2.1).
    fn da_corrida(c: &Corrida, uso: &str) -> Self {
        let p = crate::ia::uso::da_corrida(c, uso);
        FIa {
            modelo: p.modelo,
            arquivos: p.arquivos.into_iter().map(|a| FArquivo { nome: a.nome, sha256: a.sha256 }).collect(),
            origem: p.origem,
            commit: p.commit,
            desvio: p.desvio,
            runtime: p.runtime,
            provedor: p.provedor,
            uso: p.uso,
        }
    }

    /// Mesmo modelo, arquivos, execução e uso; o provedor em qualquer caixa (`cpu` no fio, "CPU" em `feito` antigo).
    fn mesmo(&self, o: &FIa) -> bool {
        let sem_caixa = |x: &FIa| FIa { provedor: x.provedor.trim().to_ascii_lowercase(), ..x.clone() };
        sem_caixa(self) == sem_caixa(o)
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
#[serde(default)]
struct FArquivo {
    nome: String,
    sha256: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FAviso {
    codigo: String,
    n: u64,
    ciente: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Execucao {
    autor: EAutor,
    caso: ECaso,
    sicro: ESicro,
    sistema: ESistema,
    typst: String,
    ffmpeg: EFfmpeg,
    saida: ESaida,
    conteudo: EConteudo,
    conferencia: EConferencia,
    /// Lista editável das Configurações; ausente = a padrão (`frases::REFERENCIAS`).
    referencias: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct EAutor {
    nome: String,
    cargo: String,
    matricula: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct ECaso {
    numero: String,
    referencia: String,
    orgao: String,
    requisitante: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ESicro {
    versao: String,
    commit: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ESistema {
    so: String,
    cpu: ECpu,
    gpu: Option<String>,
    driver: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ECpu {
    modelo: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct EFfmpeg {
    versao: String,
    origem: String,
    sha256: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ESaida {
    arquivo: String,
    sha256: String,
    tamanho: u64,
    codificador: String,
    argumentos: Vec<String>,
    queda_para_cpu: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct EConteudo {
    composto_sha256: Option<String>,
    decodificado_sha256: Option<String>,
    audio_sha256: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct EConferencia {
    quadros: Option<u64>,
    erros: Option<u64>,
    ocultacoes: Option<String>,
    por: String,
    em: String,
}

// ---------------------------------------------------------------------------
// O que o modelo imprime (contrato `/2`): tudo texto pronto

#[derive(Debug, Serialize)]
struct RelatorioJuri {
    esquema: &'static str,
    programa: String,
    caso: ECaso,
    perito: EAutor,
    gerado_em: String,
    revisado_por: Option<String>,
    saida: Saida,
    originais: Vec<Original>,
    roteiro: Vec<ItemRoteiro>,
    acrescimos: Vec<Linha>,
    mudancas: Vec<Linha>,
    ia_resumo: Vec<String>,
    relogios: Vec<String>,
    medidas: Vec<String>,
    sincronizacao: Vec<Sincronia>,
    limitacoes: Vec<String>,
    selos_desligados: Vec<String>,
    conferir: Conferir,
    processamentos: Vec<Processamento>,
    diferencas: Vec<Diferenca>,
    ocultacoes: Vec<Ocultacao>,
    conferencia_ocultacoes: Option<String>,
    codificacao: Vec<[String; 2]>,
    software: Vec<[String; 3]>,
    modelos_ia: Vec<ModeloIa>,
    rastreios: Vec<RastreioAnexo>,
    reproducao: Reproducao,
    pacote: Vec<ArquivoPacote>,
    anexos: Vec<Anexo>,
    referencias: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Saida {
    arquivo: String,
    sha256: String,
    tamanho: String,
    duracao: String,
    imagem: String,
    ritmo: String,
    formato: String,
    perfil: String,
    com_perdas: bool,
}

#[derive(Debug, Serialize)]
struct Original {
    id: String,
    especie: String,
    rotulo: String,
    arquivo: String,
    sha256: String,
    tamanho: Option<String>,
    duracao: String,
    imagem: Option<String>,
    ritmo: String,
    formato: String,
    taxa_variavel: bool,
    formato_nativo: Option<bool>,
    recebido: String,
    relogio: String,
    derivacao: Option<Derivada>,
    cobertura: Option<Cobertura>,
    tecnico: OriginalTecnico,
}

#[derive(Debug, Serialize)]
struct Derivada {
    texto: String,
    origem_sha256: String,
}

#[derive(Debug, Serialize)]
struct Cobertura {
    usado: String,
    total: String,
    intervalos: Vec<String>,
}

#[derive(Debug, Serialize)]
struct OriginalTecnico {
    conteiner: String,
    codec: String,
    pix_fmt: String,
    par: String,
    quadros: String,
    fps_nominal: String,
    intervalo_quadros: String,
    cor: String,
    observacoes: String,
    intervalos_usados: Option<String>,
    derivacao: Option<String>,
    comando: Option<String>,
}

#[derive(Debug, Serialize)]
struct ItemRoteiro {
    n: u32,
    tipo: String,
    descricao: String,
    na_saida: String,
    miniatura: Option<String>,
    origem: Option<String>,
    velocidade: Option<String>,
    efeitos: Vec<Efeito>,
    selos: Vec<String>,
    textos: Vec<TextoTela>,
}

/// O que foi posto ou mudado no item; `marcas` só nas marcações ("3 setas" e cada uma com a cor).
#[derive(Debug, Serialize)]
struct Efeito {
    texto: String,
    marcas: Vec<Marca>,
}

impl Efeito {
    fn so(texto: String) -> Self {
        Efeito { texto, marcas: Vec::new() }
    }
}

/// Um bloco de uma tela de texto: o papel ("Título") e o texto como o perito escreveu.
#[derive(Debug, Serialize)]
struct TextoTela {
    papel: String,
    texto: String,
}

#[derive(Debug, Serialize)]
struct Linha {
    o_que: String,
    onde: String,
    explicacao: String,
}

#[derive(Debug, Serialize)]
struct Sincronia {
    par: String,
    explicacao: String,
    metodo: String,
    deslocamento: String,
    deriva: String,
    incerteza: String,
    conferencia: String,
    tecnico: SincroniaTecnico,
}

#[derive(Debug, Serialize)]
struct SincroniaTecnico {
    metodo: String,
    deslocamento: String,
    deriva: String,
    incerteza: String,
}

#[derive(Debug, Serialize)]
struct Conferir {
    arquivo: String,
    windows: String,
    linux: String,
    quadros: String,
    lista: Option<String>,
    prova_arquivo: String,
    prova_quadros: String,
}

#[derive(Debug, Serialize)]
struct Processamento {
    item: String,
    ordem: String,
    nome: String,
    parametros: String,
}

#[derive(Debug, Serialize)]
struct Diferenca {
    item: String,
    imagem: String,
    taxa: String,
    quadros: String,
    repetidos: String,
    descartados: String,
    obs: String,
}

/// Linha da planilha de ocultações (anexo E, 18-M §11 figura 4). `n` numera imagem e som; `regiao` e `quadros`
/// só na ocultação de imagem.
#[derive(Debug, Serialize)]
struct Ocultacao {
    n: String,
    item: String,
    gravacao: String,
    arquivo: String,
    intervalo: String,
    conteudo: String,
    forma: String,
    filtro: String,
    motivo: String,
    quadros: Option<String>,
    regiao: Option<String>,
}

#[derive(Debug, Serialize)]
struct ModeloIa {
    modelo: String,
    arquivos: Vec<FArquivo>,
    origem: String,
    execucao: String,
    desvio: Option<String>,
    uso: String,
}

/// Rastreio em que um programa atuou (anexo H): `id` = "R1"…, nunca o nome que o perito deu.
#[derive(Debug, Serialize)]
struct RastreioAnexo {
    id: String,
    origem: String,
    uso: String,
    corridas: Vec<String>,
    confirmacao: String,
    amostras: String,
}

#[derive(Debug, Serialize)]
struct Reproducao {
    projeto: String,
    projeto_sha256: String,
    observacao: String,
}

#[derive(Debug, Serialize)]
struct ArquivoPacote {
    arquivo: String,
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Anexo {
    arquivo: String,
    descricao: String,
    sha256: String,
}

// ---------------------------------------------------------------------------
// Formatação pt-BR

/// Número com vírgula e `casas` fixas.
pub(super) fn dec(v: f64, casas: usize) -> String {
    format!("{v:.casas$}").replace('.', ",").replace('-', "−")
}

/// Número com até `casas`, sem zeros sobrando.
pub(super) fn num(v: f64, casas: usize) -> String {
    let s = format!("{v:.casas$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { &s };
    let s = if s == "-0" { "0" } else { s };
    s.replace('.', ",").replace('-', "−")
}

/// "0:07,500", "1:02:03,500".
pub(super) fn tempo(s: f64) -> String {
    if !s.is_finite() {
        return "—".into();
    }
    let ms = (s.abs() * 1000.0).round() as u64;
    let (h, m, seg, mili) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    let sinal = if s < 0.0 && ms > 0 { "−" } else { "" };
    if h > 0 {
        format!("{sinal}{h}:{m:02}:{seg:02},{mili:03}")
    } else {
        format!("{sinal}{m}:{seg:02},{mili:03}")
    }
}

/// "47 s", "37,5 s", "12 min 03 s", "1 h 02 min 03 s".
fn duracao(s: f64) -> String {
    if !s.is_finite() {
        return "—".into();
    }
    let sinal = if s < 0.0 { "−" } else { "" };
    let s = s.abs();
    if s < 60.0 {
        return format!("{sinal}{} s", num(s, 1));
    }
    let t = s.round() as u64;
    let (h, m, seg) = (t / 3600, t / 60 % 60, t % 60);
    if h > 0 {
        format!("{sinal}{h} h {m:02} min {seg:02} s")
    } else {
        format!("{sinal}{m} min {seg:02} s")
    }
}

/// 5129295 → "5.129.295".
fn milhar(n: u64) -> String {
    let s = n.to_string();
    let mut saida = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            saida.push('.');
        }
        saida.push(c);
    }
    saida
}

/// "5,1 MB (5.129.295 bytes)".
fn tamanho(n: u64) -> String {
    let (v, u) = match n {
        0..=999 => return format!("{n} bytes"),
        1_000..=999_999 => (n as f64 / 1e3, "kB"),
        1_000_000..=999_999_999 => (n as f64 / 1e6, "MB"),
        _ => (n as f64 / 1e9, "GB"),
    };
    format!("{} {u} ({} bytes)", num(v, if v < 100.0 { 1 } else { 0 }), milhar(n))
}

const MESES: [&str; 12] = ["janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"];

fn instante(rfc3339: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(rfc3339).ok().map(|d| d.with_timezone(&Utc))
}

/// "13 de novembro de 2026, às 20h40 (horário de Brasília)". O Brasil não tem horário de verão desde 2019: −03:00 fixo.
fn data(rfc3339: &str) -> String {
    let (Some(d), Some(fuso)) = (instante(rfc3339), FixedOffset::west_opt(3 * 3600)) else { return rfc3339.to_string() };
    let l = d.with_timezone(&fuso);
    format!("{} de {} de {}, às {}h{:02} (horário de Brasília)", l.day(), MESES[l.month0() as usize], l.year(), l.hour(), l.minute())
}

/// "14/11/2026, 6h00 (Brasília)": datas das tabelas do anexo.
fn data_curta(rfc3339: &str) -> String {
    let (Some(d), Some(fuso)) = (instante(rfc3339), FixedOffset::west_opt(3 * 3600)) else { return ou_traco(rfc3339) };
    let l = d.with_timezone(&fuso);
    format!("{:02}/{:02}/{}, {}h{:02} (Brasília)", l.day(), l.month(), l.year(), l.hour(), l.minute())
}

/// Relógio do gravador (`Derivacao.inicio_epoca_s`, como o gravador gravou, sem fuso): "18/09/2017, 19:24:59" e os
/// milésimos quando há ("…:59,250").
fn relogio_do_gravador(s: f64) -> Option<String> {
    let ms = (s * 1000.0).round();
    if !ms.is_finite() || ms.abs() > 1e15 {
        return None;
    }
    let d = DateTime::<Utc>::from_timestamp_millis(ms as i64)?;
    let mili = d.timestamp_subsec_millis();
    let seg = if mili > 0 { format!("{:02},{mili:03}", d.second()) } else { format!("{:02}", d.second()) };
    Some(format!("{:02}/{:02}/{}, {:02}:{:02}:{seg}", d.day(), d.month(), d.year(), d.hour(), d.minute()))
}

/// `cpu` | `cuda` do fio (B2) como o PDF mostra; fora do catálogo, como veio.
fn provedor(p: &str) -> String {
    let p = p.trim();
    frases::rotulo(&format!("provedor.{}", p.to_ascii_lowercase())).map(str::to_string).unwrap_or_else(|| p.to_string())
}

/// Texto de um enum serde (`saiu_do_quadro`).
fn fio<T: Serialize>(x: &T) -> String {
    serde_json::to_value(x).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

/// Taxa em imagens por segundo: "30", "29,97".
fn taxa(num_: i64, den: i64) -> String {
    if num_ <= 0 || den <= 0 {
        return "—".into();
    }
    num(num_ as f64 / den as f64, 2)
}

/// Identificador da gravação no relatório: A, B, … Z, AA, AB…
fn letra(i: usize) -> String {
    let mut n = i;
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s
}

pub(super) fn rot(chave: &str) -> &'static str {
    frases::rotulo(chave).unwrap_or("—")
}

/// Rótulo do valor do grupo; fora do catálogo, o próprio valor.
pub(super) fn rot_ou(grupo: &str, valor: &str) -> String {
    frases::rotulo(&format!("{grupo}.{valor}")).map(str::to_string).unwrap_or_else(|| valor.to_string())
}

fn cond(c: &str) -> String {
    frases::da_condicao(c).unwrap_or("").to_string()
}

fn ou_traco(s: &str) -> String {
    if s.trim().is_empty() {
        "—".into()
    } else {
        s.to_string()
    }
}

fn nome_do_arquivo(rel: &str) -> &str {
    rel.rsplit(['/', '\\']).next().unwrap_or(rel)
}

fn cortar(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        return s;
    }
    let mut c: String = s.chars().take(max).collect();
    c.push('…');
    c
}

/// "item 3" · "itens 2, 5".
fn onde(itens: &[u32]) -> String {
    let lista = itens.iter().map(u32::to_string).collect::<Vec<_>>().join(", ");
    frases::preencher(rot(if itens.len() == 1 { "onde.item" } else { "onde.itens" }), &[("n", &lista)])
}

fn gpu(codificador: &str) -> bool {
    ["nvenc", "vaapi", "qsv", "amf", "videotoolbox"].iter().any(|g| codificador.contains(g))
}

// ---------------------------------------------------------------------------
// Montagem

/// Mídia que o ritmo medido mostra variável: lacuna, ou passo fora de ±25 % do mediano.
fn ritmo_variavel(m: &FMedido) -> bool {
    let med = m.passo_med as f64;
    med > 0.0 && (!m.lacunas.is_empty() || (m.passo_min as f64) < 0.75 * med || (m.passo_max as f64) > 1.25 * med)
}

/// Imagens por segundo de um passo em ticks de `tb`.
fn por_segundo(passo: i64, tb: &R) -> Option<f64> {
    (passo > 0 && tb.num > 0 && tb.den > 0).then(|| tb.den as f64 / (passo as f64 * tb.num as f64))
}

fn ms_do_passo(passo: i64, tb: &R) -> String {
    if tb.den <= 0 {
        return "—".into();
    }
    dec(passo as f64 * tb.num as f64 / tb.den as f64 * 1000.0, 3)
}

/// Nome do codec que se reconhece fora do ffmpeg ("h264" → "H.264"); `imagem` = arquivo de imagem parada.
fn codec_leigo(codec: &str, imagem: bool) -> String {
    let c = codec.trim().to_ascii_lowercase();
    let nome = match c.as_str() {
        "" => "",
        "h264" => "H.264",
        "hevc" | "h265" => "H.265",
        "mpeg4" => "MPEG-4",
        "msmpeg4" | "msmpeg4v2" | "msmpeg4v3" => "MPEG-4 (Microsoft)",
        "mjpeg" | "jpeg" | "jpg" if imagem => "JPEG",
        "mjpeg" => "Motion JPEG",
        "mpeg2video" => "MPEG-2",
        "mpeg1video" => "MPEG-1",
        "h263" => "H.263",
        "wmv1" | "wmv2" | "wmv3" => "WMV",
        "vp8" => "VP8",
        "vp9" => "VP9",
        "av1" => "AV1",
        "prores" => "ProRes",
        "dvvideo" => "DV",
        "rawvideo" => "sem compressão",
        "theora" => "Theora",
        "webp" => "WebP",
        "jpegxl" => "JPEG XL",
        "opus" => "Opus",
        "vorbis" => "Vorbis",
        "amr_nb" | "amr_wb" => "AMR",
        _ if c.starts_with("pcm_") => "PCM",
        // png, bmp, tiff, gif, ffv1, aac, mp3, flac, h263…: a sigla
        _ => return c.to_ascii_uppercase(),
    };
    nome.to_string()
}

/// Formato como quem não é técnico conhece ("MP4 (H.264)", "PNG"); o nome do ffmpeg fica no anexo B. `nome` = nome
/// do arquivo: a extensão separa MOV de MP4, que o ffmpeg lê com o mesmo leitor.
fn formato(conteiner: &str, codec: &str, nome: &str) -> String {
    let c = conteiner.trim().to_ascii_lowercase();
    let ext = nome.rsplit_once('.').map(|x| x.1.to_ascii_lowercase()).unwrap_or_default();
    // imagem parada (image2, image2pipe, png_pipe…): o formato é o da imagem
    if c.starts_with("image2") || c.ends_with("_pipe") {
        let cod = codec_leigo(codec, true);
        return if !cod.is_empty() {
            cod
        } else if !ext.is_empty() {
            ext.to_ascii_uppercase()
        } else {
            "—".into()
        };
    }
    let cont = if c.contains("mp4") || c.starts_with("mov") {
        match ext.as_str() {
            "mov" => "MOV".to_string(),
            "3gp" => "3GP".to_string(),
            "m4a" => "M4A".to_string(),
            _ => "MP4".to_string(),
        }
    } else if c.contains("matroska") || c.contains("webm") {
        if ext == "webm" { "WebM" } else { "MKV" }.to_string()
    } else {
        match c.as_str() {
            "" => String::new(),
            "avi" => "AVI".into(),
            "mpegts" => "MPEG-TS".into(),
            "mpeg" | "mpegps" | "mpegvideo" => "MPEG".into(),
            "asf" => if ext == "wmv" { "WMV" } else { "ASF" }.into(),
            "dhav" => "DAV".into(),
            "h264" => "H.264".into(),
            "hevc" => "H.265".into(),
            "flv" | "wav" | "mp3" | "ogg" | "flac" | "aac" | "mxf" => c.to_ascii_uppercase(),
            // leitor de nome técnico: vale a extensão, que é o que se vê no arquivo
            _ if !ext.is_empty() => ext.to_ascii_uppercase(),
            _ => conteiner.trim().to_string(),
        }
    };
    let cod = codec_leigo(codec, false);
    match (cont.is_empty(), cod.is_empty()) {
        (true, true) => "—".into(),
        (false, true) => cont,
        (true, false) => cod,
        // fluxo sem contêiner (H.264 puro): diz uma vez
        (false, false) if cont == cod => cont,
        (false, false) => format!("{cont} ({cod})"),
    }
}

/// Intervalos entre imagens seguidas da gravação nos trechos usados, por mídia, em ticks: do `mapa.csv` (colunas
/// `q,item,clipe,midia,pts,indice,t_s,tipo`), pelos quadros de índice seguido. Linha que não lê fica de fora.
fn intervalos_do_mapa(csv: &str) -> HashMap<String, Vec<i64>> {
    let mut quadros: HashMap<String, BTreeMap<u64, i64>> = HashMap::new();
    for linha in csv.lines().skip(1) {
        let c = campos_csv(linha);
        let (Some(midia), Some(pts), Some(j)) = (c.get(3).filter(|m| !m.is_empty()), c.get(4).and_then(|p| p.parse::<i64>().ok()), c.get(5).and_then(|j| j.parse::<u64>().ok())) else {
            continue;
        };
        quadros.entry(midia.clone()).or_default().insert(j, pts);
    }
    quadros
        .into_iter()
        .map(|(m, q)| {
            let mut passos: Vec<i64> = q.iter().filter_map(|(j, pts)| q.get(&(j + 1)).map(|prox| prox - pts)).filter(|p| *p > 0).collect();
            passos.sort_unstable();
            (m, passos)
        })
        .collect()
}

/// Campos de uma linha CSV (aspas duplas com `""` dentro, como o `mapa::campo` grava).
fn campos_csv(linha: &str) -> Vec<String> {
    let (mut v, mut atual, mut aspas, mut cs) = (Vec::new(), String::new(), false, linha.chars().peekable());
    while let Some(c) = cs.next() {
        match c {
            '"' if aspas && cs.peek() == Some(&'"') => {
                atual.push('"');
                cs.next();
            }
            '"' => aspas = !aspas,
            ',' if !aspas => v.push(std::mem::take(&mut atual)),
            _ => atual.push(c),
        }
    }
    v.push(atual);
    v
}

/// Percentil `p` (0..1) de uma lista ordenada, pelo posto mais próximo.
fn percentil(v: &[i64], p: f64) -> i64 {
    v[((p * (v.len() - 1) as f64).round() as usize).min(v.len() - 1)]
}

/// Imagens por segundo para leigo: inteiro a partir de 10, uma casa abaixo disso ("15", "7,5").
fn por_segundo_txt(f: f64) -> String {
    num(f, if f >= 10.0 { 0 } else { 1 })
}

fn original(i: usize, m: &FMidia, passos: Option<&[i64]>) -> Original {
    let med = &m.medido;
    let especie = match med.quadros {
        0 => "especie.som",
        1 => "especie.imagem",
        _ => "especie.video",
    };
    let variavel = med.quadros > 1 && ritmo_variavel(med);
    // percentis 10, 50 e 90 dos intervalos nos trechos usados: o mínimo e o máximo absolutos (1 tique, um buraco de
    // segundos) não dizem o ritmo a quem assiste; ficam no anexo B
    let passos = passos.filter(|p| med.quadros > 1 && p.len() >= MIN_INTERVALOS);
    // fluxo cru sem temporização (P4.6b): a taxa da cópia foi suposta, não medida
    let suposta = m.derivacao.as_ref().and_then(|d| d.taxa_suposta.as_ref()).filter(|r| r.num > 0 && r.den > 0);
    let ritmo = match (med.quadros, por_segundo(med.passo_med, &med.tb)) {
        (0, _) => rot("ritmo.som").to_string(),
        (_, _) if suposta.is_some() => frases::preencher(rot("ritmo.suposto"), &[("fps", &suposta.map(|r| taxa(r.num, r.den)).unwrap_or_default())]),
        (1, _) | (_, None) => rot("ritmo.imagem").to_string(),
        (_, Some(f)) if variavel => {
            let faixa = passos.and_then(|p| Some((por_segundo(percentil(p, 0.9), &med.tb)?, por_segundo(percentil(p, 0.1), &med.tb)?)));
            match faixa.map(|(de, ate)| (por_segundo_txt(de), por_segundo_txt(ate))) {
                Some((de, ate)) if de != ate => frases::preencher(rot("ritmo.variavel_faixa"), &[("fps", &por_segundo_txt(f)), ("de", &de), ("ate", &ate)]),
                _ => frases::preencher(rot("ritmo.variavel"), &[("fps", &por_segundo_txt(f))]),
            }
        }
        (_, Some(f)) => frases::preencher(rot("ritmo.constante"), &[("fps", &num(f, 2))]),
    };
    let intervalos_usados = passos.map(|p| {
        let ms = |q: f64| ms_do_passo(percentil(p, q), &med.tb);
        format!("percentis 10, 50 e 90: {} · {} · {} ms ({} intervalos nos trechos usados)", ms(0.1), ms(0.5), ms(0.9), milhar(p.len() as u64))
    });
    let mut recebido = rot(match m.recebido.nativo {
        Some(true) => "recebido.nativo",
        Some(false) => "recebido.convertido",
        None => "recebido.nao_informado",
    })
    .to_string();
    if !m.recebido.obs.trim().is_empty() {
        recebido = format!("{recebido} ({})", m.recebido.obs.trim());
    }
    // P4.6: o relógio do gravador no começo do original, quando ele grava a data
    let inicio = m.derivacao.as_ref().and_then(|d| d.inicio_epoca_s).and_then(relogio_do_gravador).map(|q| frases::preencher(rot("relogio.inicio"), &[("quando", &q)]));
    let desvio = m.relogio_desvio_s.map(|d| {
        let sinal = if d < 0.0 { "−" } else { "+" };
        frases::preencher(rot("relogio.desvio"), &[("d", &format!("{sinal}{}", duracao(d.abs())))])
    });
    let mut relogio = match [inicio, desvio].into_iter().flatten().collect::<Vec<_>>() {
        v if v.is_empty() => rot("relogio.sem").to_string(),
        v => v.join(" · "),
    };
    if let Some(g) = m.gravador.as_deref().filter(|g| !g.trim().is_empty()) {
        relogio = format!("{relogio} · {g}");
    }
    let derivacao = m.derivacao.as_ref().map(|d| {
        // cópia de gravador em FFV1 (fluxo cru com quadros B): recodificada, sem perda (a prova confere cada imagem)
        let perdas = match (d.com_perdas, d.tipo == "copia_dvr" && m.codec.eq_ignore_ascii_case("ffv1")) {
            (true, _) => "derivacao.com_perdas",
            (false, true) => "derivacao.sem_perdas_recodificada",
            (false, false) => "derivacao.sem_perdas",
        };
        let como = format!("{}, {}", rot_ou("derivacao", &d.tipo), rot(perdas));
        let texto = match d.de_nome.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            Some(nome) => frases::preencher(modelo::DERIVADA_DE, &[("nome", nome), ("como", &como)]),
            None => frases::preencher(modelo::DERIVADA, &[("como", &como)]),
        };
        Derivada { texto, origem_sha256: d.raiz_sha256.clone() }
    });
    // anexo B: como a cópia foi feita (ffmpeg, taxa suposta, relógio) e o comando registrado
    let (derivacao_tecnica, comando) = match &m.derivacao {
        Some(d) => {
            let mut v = vec![rot_ou("derivacao", &d.tipo)];
            v.extend(d.ffmpeg_versao.iter().filter(|x| !x.trim().is_empty()).map(|x| format!("FFmpeg {}", x.trim())));
            v.extend(suposta.map(|r| format!("taxa suposta {}/{} quadros/s", r.num, r.den)));
            v.extend(d.inicio_epoca_s.filter(|x| x.is_finite()).map(|x| format!("início no relógio do gravador {} ({} s desde 1970)", relogio_do_gravador(x).unwrap_or_else(|| "—".into()), num(x, 3))));
            (Some(v.join(" · ")), Some(d.comando.trim().to_string()).filter(|c| !c.is_empty()))
        }
        None => (None, None),
    };
    let cobertura = (med.quadros > 1).then(|| Cobertura {
        usado: duracao(m.usado_total_s),
        total: duracao(med.ate_s - med.de_s),
        intervalos: m.usado.iter().map(|f| format!("{} a {}", tempo(f.de_s), tempo(f.ate_s))).collect(),
    });
    let intervalo = if med.quadros < 2 {
        "—".to_string()
    } else if med.passo_min == med.passo_max {
        format!("{} ms (mín. = máx.)", ms_do_passo(med.passo_med, &med.tb))
    } else {
        format!("{} a {} ms (mediana {})", ms_do_passo(med.passo_min, &med.tb), ms_do_passo(med.passo_max, &med.tb), ms_do_passo(med.passo_med, &med.tb))
    };
    let mut obs = Vec::new();
    if !m.discrepancias.is_empty() {
        obs.push(format!("declarado ≠ medido: {}", m.discrepancias.join(", ")));
    }
    if !med.lacunas.is_empty() {
        obs.push(format!("lacunas: {}", med.lacunas.len()));
    }
    if m.erros_decodificacao > 0 {
        obs.push(format!("erros de decodificação: {}", m.erros_decodificacao));
    }
    if m.giro != 0 {
        obs.push(format!("giro: {}°", m.giro));
    }
    Original {
        id: letra(i),
        especie: rot(especie).to_string(),
        rotulo: m.nome.clone(),
        arquivo: m.nome.clone(),
        sha256: m.sha256.clone(),
        tamanho: (m.tamanho > 0).then(|| tamanho(m.tamanho)),
        // foto e som não têm tabela de quadros: a duração não vem daqui
        duracao: if med.quadros <= 1 { "—".into() } else { duracao(med.ate_s - med.de_s) },
        imagem: (m.largura > 0 && m.altura > 0).then(|| format!("{} × {} pontos", m.largura, m.altura)),
        ritmo,
        formato: formato(&m.conteiner, &m.codec, &m.nome),
        taxa_variavel: variavel,
        formato_nativo: m.recebido.nativo,
        recebido,
        relogio,
        derivacao,
        cobertura,
        tecnico: OriginalTecnico {
            conteiner: ou_traco(&m.conteiner),
            codec: ou_traco(&m.codec),
            pix_fmt: ou_traco(&m.pix_fmt),
            par: if m.sar.num > 0 && m.sar.den > 0 { format!("{}:{}", m.sar.num, m.sar.den) } else { "—".into() },
            quadros: milhar(med.quadros),
            fps_nominal: ou_traco(&m.declarado.r_frame_rate),
            intervalo_quadros: intervalo,
            cor: if m.cor.matriz.is_empty() { "—".into() } else { format!("{}, faixa {}", m.cor.matriz.to_uppercase().replace("BT", "BT."), if m.cor.faixa == "pc" { "cheia" } else { "limitada" }) },
            observacoes: if obs.is_empty() { "—".into() } else { obs.join("; ") },
            intervalos_usados,
            derivacao: derivacao_tecnica,
            comando,
        },
    }
}

/// Fator da velocidade constante; None = rampa ou ausente.
fn fator(v: &Option<Value>) -> Option<f64> {
    v.as_ref().filter(|v| v["tipo"] == "constante").and_then(|v| v["fator"].as_f64())
}

/// Pontos da rampa (P3.4): (tempo de fonte, fator), na ordem; None = não é rampa.
fn pontos_da_rampa(v: &Option<Value>) -> Option<Vec<(f64, f64)>> {
    let v = v.as_ref().filter(|v| v["tipo"] == "rampa")?;
    Some(v["pontos"].as_array()?.iter().filter_map(|p| Some((p["t"].as_f64()?, p["v"].as_f64()?))).collect())
}

/// Menor e maior fator da rampa.
fn faixa_da_rampa(v: &Option<Value>) -> Option<(f64, f64)> {
    let p = pontos_da_rampa(v)?;
    let (a, b) = p.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(x.1), b.max(x.1)));
    (a <= b).then_some((a, b))
}

/// "1× em 00:00:02,000, 0,25× em 00:00:03,000 e 1× em 00:00:04,000" (`tempo_de` formata o instante).
fn lista_da_rampa(p: &[(f64, f64)], tempo_de: impl Fn(f64) -> String) -> String {
    let itens: Vec<String> = p.iter().map(|&(t, v)| format!("{}× em {}", num(v, 3), tempo_de(t))).collect();
    match itens.split_last() {
        Some((ultimo, resto)) if !resto.is_empty() => format!("{} e {ultimo}", resto.join(", ")),
        _ => itens.join(""),
    }
}

/// Tipos (do catálogo de frases) do que o item traz além dos processamentos: velocidade, quadro parado, cartela, som.
fn extras_do_item(it: &FItem, com_som: bool) -> Vec<&'static str> {
    let mut v = Vec::new();
    match it.tipo.as_str() {
        "congelar" => v.push(tipo::CONGELAR),
        "titulo" => v.push(tipo::TITULO),
        "imagem" => v.push(tipo::IMAGEM_CLIPE),
        _ => {}
    }
    match fator(&it.velocidade) {
        Some(f) if f < 1.0 => v.push(tipo::CAMERA_LENTA),
        Some(f) if f > 1.0 => v.push(tipo::ACELERADO),
        _ => {}
    }
    // rampa: a parte lenta e a acelerada
    if let Some((lento, rapido)) = faixa_da_rampa(&it.velocidade) {
        if lento < 1.0 {
            v.push(tipo::CAMERA_LENTA);
        }
        if rapido > 1.0 {
            v.push(tipo::ACELERADO);
        }
    }
    if it.transicao.is_some() {
        v.push(tipo::TRANSICAO);
    }
    // sem som na saída, o som do clipe não existe no vídeo: nada a dizer dele
    if let Some(a) = it.audio.as_ref().filter(|_| com_som) {
        if a.mudo {
            v.push(tipo::MUDO);
        } else if a.volume.as_f64().is_some_and(|x| (x - 1.0).abs() > 1e-9) || a.volume.is_array() {
            v.push(tipo::VOLUME);
        }
    }
    v
}

fn velocidade_txt(it: &FItem) -> Option<String> {
    if it.tipo == "congelar" {
        return Some(rot("velocidade.parada").to_string());
    }
    let v = it.velocidade.as_ref()?;
    Some(match fator(&it.velocidade) {
        Some(f) if (f - 1.0).abs() < 1e-9 => rot("velocidade.normal").to_string(),
        Some(f) if f < 1.0 => frases::preencher(rot("velocidade.lenta"), &[("v", &num(f, 3))]),
        Some(f) => frases::preencher(rot("velocidade.rapida"), &[("v", &num(f, 3))]),
        None => match pontos_da_rampa(&it.velocidade) {
            Some(p) if !p.is_empty() => format!("{}: {}", rot("velocidade.variavel"), lista_da_rampa(&p, tempo)),
            _ if v.is_object() => rot("velocidade.variavel").to_string(),
            _ => return None,
        },
    })
}

/// Rótulo do efeito no item: o do catálogo, com o que o distingue (quantas imagens na média, o que foi ocultado).
fn rotulo_do_proc(p: &FProc, tipo: &str) -> String {
    let base = frases::rotulo_do_efeito(tipo).map(str::to_string).unwrap_or_else(|| tipo.to_string());
    match tipo {
        "media_temporal" => match p.parametros["quadros"].as_u64() {
            Some(n) => format!("{base} ({n})"),
            None => base,
        },
        "ocultar" | tipo::OCULTAR_PARCIAL => match p.parametros["conteudo"].as_str() {
            Some(c) => format!("{base} ({})", rot_ou("conteudo", c)),
            None => base,
        },
        _ => base,
    }
}

/// A região do enquadramento mostra só parte da imagem em algum instante (a regra de `camada::selos`)? Forma que
/// não se reconhece conta como parte.
fn regiao_parcial(regiao: &Value) -> bool {
    let parte = |c: &Value| match (c["w"].as_f64(), c["h"].as_f64()) {
        (Some(w), Some(h)) => w < 1.0 - 1e-6 || h < 1.0 - 1e-6,
        _ => true,
    };
    match regiao {
        Value::Array(chaves) => chaves.iter().any(|k| parte(&k["v"])),
        fixa => parte(fixa),
    }
}

/// Tipo do catálogo de frases para o processamento. O enquadramento só é "ampliação" quando mostra parte da imagem;
/// com a imagem inteira, "Preencher" corta as bordas e "Inteiro" não muda o que se vê (None: fica só no anexo C).
fn tipo_do_proc(p: &FProc) -> Option<&str> {
    if p.nome == "ocultar" {
        Some(tipo_da_ocultacao(p.parametros["modo"].as_str().unwrap_or("")))
    } else if p.nome == "enquadramento" && segue_rastreio(p).is_some() && regiao_parcial(&p.parametros["regiao"]) {
        // Fase 4 (ZS): a parte mostrada se desloca com o alvo
        Some(tipo::AMPLIACAO_QUE_SEGUE)
    } else if p.nome != "enquadramento" || regiao_parcial(&p.parametros["regiao"]) {
        Some(&p.nome)
    } else if p.parametros["ajuste"] == "cobrir" {
        Some(tipo::PREENCHER)
    } else {
        None
    }
}

/// Tipo do catálogo de frases para a ocultação: só o sólido opaco é dado como irreversível (D17).
fn tipo_da_ocultacao(modo: &str) -> &'static str {
    match modo {
        "mosaico" | "desfoque" => tipo::OCULTAR_PARCIAL,
        _ => "ocultar",
    }
}

/// Itens do roteiro cujos slots cruzam [de_q, de_q + n_q).
fn itens_nos_slots(roteiro: &[FItem], de_q: u64, n_q: u32) -> Vec<u32> {
    let ate = de_q + n_q as u64;
    roteiro.iter().filter(|it| it.na_saida.de_q < ate && de_q < it.na_saida.de_q + it.na_saida.n_q as u64).map(|it| it.n).collect()
}

/// Rastreio que o enquadramento acompanha (`EfEnquadramento.segue`, Fase 4); None = região própria.
fn segue_rastreio(p: &FProc) -> Option<&str> {
    p.parametros["segue"]["rastreio"].as_str().filter(|x| !x.trim().is_empty())
}

/// Id do rastreio que o processamento usa (destaque e lupa; ocultar e holofote pelo `alvo`; velocidade que
/// acompanha pelo `onde`; enquadramento que segue pelo `segue`, como `efeitos::rastreio_seguido`).
fn rastreio_do_proc(p: &FProc) -> Option<&str> {
    p.parametros["rastreio"].as_str().or_else(|| p.parametros["alvo"]["rastreio"].as_str()).or_else(|| p.parametros["onde"]["rastreio"].as_str()).or_else(|| segue_rastreio(p))
}

fn automatico(r: &FRastreio) -> bool {
    matches!(r.origem["tipo"].as_str(), Some("deteccao") | Some("pessoas"))
}

/// Faixas de pts (na gravação do rastreio) dos quadros lidos dos clipes em que o rastreio `id` é usado: os de uma
/// ocultação dele e os com um processamento que o segue; com o passo mínimo entre quadros da gravação (0 = não se
/// sabe). Vazia = `feito` sem os pts (nada se recorta).
fn faixas_de_uso(f: &Feito, id: &str) -> Vec<(i64, i64, i64)> {
    let clipes: Vec<&str> = f.ocultacoes.iter().filter(|o| o.rastreio == id).flat_map(|o| o.clipes.iter().map(String::as_str)).collect();
    f.roteiro
        .iter()
        .filter(|it| clipes.contains(&it.clipe.as_str()) || it.processamentos.iter().any(|p| rastreio_do_proc(p) == Some(id)))
        .filter_map(|it| it.origem.as_ref())
        .filter(|o| o.lidos > 0)
        .map(|o| {
            let passo = f.midias.iter().find(|m| m.id == o.midia).map_or(0, |m| m.medido.passo_min.max(0));
            (o.pts_primeiro.min(o.pts_ultimo), o.pts_primeiro.max(o.pts_ultimo), passo)
        })
        .collect()
}

/// `o` (mais antiga) e `w` estão de frente uma para a outra, a condição de `rastreio::arquivo::juntar` para fundir:
/// sentidos opostos e a âncora de `o` no fim da faixa de `w` ou além.
fn de_frente(o: &Corrida, w: &Corrida) -> bool {
    let (lo, hi, x, y) = (w.de_pts.min(w.ate_pts), w.de_pts.max(w.ate_pts), w.ancora_pts, o.ancora_pts);
    o.sentido != w.sentido
        && match w.sentido {
            Sentido::Frente => y >= hi && y > x,
            Sentido::Tras => y <= lo && y < x,
        }
}

/// Que corridas de `cs` (na ordem de `corridas[]`) deram amostra a algum quadro de `usos` (`faixas_de_uso`). Regra do
/// arquivo denso (`rastreio::arquivo`), sem a coluna `corrida`: cada corrida substitui a faixa inteira que percorreu,
/// então a linha no pts p é da ÚLTIMA corrida cuja faixa cobre p (a escritora); a linha fundida entre duas âncoras é
/// das duas: da escritora e da anterior a ela em p, quando estão de frente uma para a outra. Sem `usos`, todas.
fn corridas_no_video(cs: &[Corrida], usos: &[(i64, i64, i64)]) -> Vec<bool> {
    if usos.is_empty() {
        return vec![true; cs.len()];
    }
    let faixa = |c: &Corrida| (c.de_pts.min(c.ate_pts), c.de_pts.max(c.ate_pts));
    // intervalos [quebras[i], quebras[i + 1]) em que as corridas que cobrem o pts não mudam
    let mut quebras: Vec<i64> = cs.iter().flat_map(|c| [faixa(c).0, faixa(c).1.saturating_add(1)]).collect();
    quebras.sort_unstable();
    quebras.dedup();
    let pos = |v: i64| quebras.partition_point(|&q| q < v);
    let (mut entram, mut saem) = (vec![Vec::new(); quebras.len()], vec![Vec::new(); quebras.len()]);
    for (k, c) in cs.iter().enumerate() {
        entram[pos(faixa(c).0)].push(k);
        saem[pos(faixa(c).1.saturating_add(1))].push(k);
    }
    // em cada intervalo: a escritora e a anterior a ela (as duas de maior índice que o cobrem)
    let mut cobrem = std::collections::BTreeSet::new();
    let mut topo: Vec<(Option<usize>, Option<usize>)> = Vec::with_capacity(quebras.len());
    for i in 0..quebras.len() {
        for k in &saem[i] {
            cobrem.remove(k);
        }
        cobrem.extend(entram[i].iter().copied());
        let mut it = cobrem.iter().rev().copied();
        topo.push((it.next(), it.next()));
    }
    let mut usada = vec![false; cs.len()];
    for &(x, y, passo) in usos {
        let mut i = quebras.partition_point(|&q| q <= x).saturating_sub(1);
        while i < quebras.len() && quebras[i] <= y {
            // entre o fim de uma corrida (a − 1) e o início de outra (b) a menos de 2 passos: nenhum quadro, nenhuma linha
            let (a, b) = (quebras[i], quebras.get(i + 1).copied().unwrap_or(i64::MAX));
            let vazio = entram[i].is_empty()
                && saem.get(i + 1).is_some_and(Vec::is_empty)
                && (b as i128 - a as i128 + 1) < 2 * passo as i128
                && !(a..b).contains(&x)
                && !(a..b).contains(&y);
            if let (false, (Some(w), anterior)) = (vazio, topo[i]) {
                usada[w] = true;
                if let Some(o) = anterior.filter(|&o| de_frente(&cs[o], &cs[w])) {
                    usada[o] = true;
                }
            }
            i += 1;
        }
    }
    usada
}

/// Corridas que leem como `rastreio::Corrida` (B2), do denso e das âncoras do "Simplificar", e quantas não leem
/// (gravadas antes da Fase 2, âncora do programa sem registro): destas nada se afirma.
fn corridas_de(r: &FRastreio) -> (Vec<Corrida>, Vec<Corrida>, usize) {
    let ler = |v: &[Value]| -> Vec<Corrida> { v.iter().filter_map(|v| Corrida::deserialize(v).ok()).collect() };
    let (denso, programa) = (ler(&r.corridas), ler(&r.corridas_do_programa));
    let ilegiveis = r.corridas.len() + r.corridas_do_programa.len() - denso.len() - programa.len();
    (denso, programa, ilegiveis)
}

fn neural(c: &Corrida) -> bool {
    matches!(c.motor.motor, MotorRastreio::Edgetam | MotorRastreio::Vittrack)
}

/// Corrida feita por modelo de IA identificado por arquivos: os rastreadores neurais e a detecção (P4.5).
fn com_modelo(c: &Corrida) -> bool {
    neural(c) || c.motor.motor == MotorRastreio::Deteccao
}

/// `uso` da corrida em `feito.ia` (a regra de `ia::uso::da_proveniencia`).
fn uso_da(c: &Corrida) -> &'static str {
    if c.motor.motor == MotorRastreio::Deteccao {
        "deteccao"
    } else {
        "rastreio"
    }
}

/// Como o rastreio nasceu; None = tipo que o catálogo não conhece (não se afirma).
fn origem_do_rastreio(r: &FRastreio) -> Option<String> {
    let rot_de = |k: &str| rot(k).to_string();
    match r.origem["tipo"].as_str()? {
        "manual" => Some(rot_de("rastreio.manual")),
        "assistido" => Some(rot_de("rastreio.assistido")),
        "deteccao" => Some(frases::preencher(rot("rastreio.deteccao"), &[("classe", &rot_ou("classe", r.origem["classe"].as_str().unwrap_or("—")))])),
        "pessoas" => match r.origem["estado"].as_str()? {
            "auto" => Some(rot_de("rastreio.pessoas.auto")),
            "revisada" => Some(rot_de("rastreio.pessoas.revisada")),
            _ => None,
        },
        _ => None,
    }
}

/// "confirmado por X em 14/11/2026, 6h00 (Brasília), de 0:01,000 a 0:02,000"; só com todos os campos. Confirmação de
/// outras amostras (nova corrida depois dela) não vale (§6.6, `validar::confirmacao`) e não sai.
fn confirmacao_do_rastreio(r: &FRastreio) -> Option<String> {
    let c = r.confirmacao.as_ref().filter(|c| c.denso_sha256 == r.denso_sha256)?;
    let (por, em, de, ate) = (c.por.as_deref()?, c.em.as_deref()?, c.de_s?, c.ate_s?);
    Some(frases::preencher(modelo::CONFIRMADO, &[("por", por), ("quando", &data_curta(em)), ("de", &tempo(de)), ("ate", &tempo(ate))]))
}

/// Origem e número de marcações do perito, no que o `feito` diz.
fn origem_e_marcacoes(r: &FRastreio) -> Vec<String> {
    let mut partes: Vec<String> = origem_do_rastreio(r).into_iter().collect();
    if let Some(n) = r.ancoras {
        partes.push(frases::preencher(modelo::MARCACOES, &[("n", &n.to_string())]));
    }
    partes
}

/// Instante (s) de um pts na base de tempo da gravação.
fn instante_do_pts(pts: i64, tb: &R) -> Option<f64> {
    (tb.num > 0 && tb.den > 0).then(|| pts as f64 * tb.num as f64 / tb.den as f64)
}

/// Uma corrida em uma linha (anexo H). `tb` = base de tempo da gravação do rastreio, quando se sabe qual é.
fn corrida_txt(c: &Corrida, tb: Option<&R>) -> String {
    let m = &c.motor;
    let modelo_ = match m.motor {
        MotorRastreio::Mancha => rot("motor.mancha").to_string(),
        _ if !m.modelo.nome.trim().is_empty() => m.modelo.nome.trim().to_string(),
        outro => rot_ou("motor", &fio(&outro)),
    };
    let execucao: Vec<String> = [m.runtime.trim().to_string(), provedor(&m.provedor)].into_iter().filter(|x| !x.is_empty()).collect();
    let (a, b) = (c.de_pts.min(c.ate_pts), c.de_pts.max(c.ate_pts));
    let faixa = match tb.and_then(|tb| Some((instante_do_pts(a, tb)?, instante_do_pts(b, tb)?))) {
        Some((x, y)) => format!("{} a {} (pts {a} a {b})", tempo(x), tempo(y)),
        None => format!("pts {a} a {b}"),
    };
    let l = &c.limiares;
    frases::preencher(
        modelo::CORRIDA,
        &[
            ("modelo", &modelo_),
            ("execucao", &ou_traco(&execucao.join(" · "))),
            ("sentido", &rot_ou("sentido", &fio(&c.sentido))),
            ("faixa", &faixa),
            ("parou", &rot_ou("parou", &fio(&c.parou))),
            ("ok", &num(l.ok, 3)),
            ("perdido", &num(l.perdido, 3)),
            ("paciencia", &l.paciencia.to_string()),
            ("quando", &data_curta(&c.feito_em)),
        ],
    )
}

/// "ocultação 1" · "ocultações 1, 3".
fn onde_oc(ns: &[usize]) -> String {
    let lista = ns.iter().map(usize::to_string).collect::<Vec<_>>().join(", ");
    frases::preencher(rot(if ns.len() == 1 { "onde.ocultacao" } else { "onde.ocultacoes" }), &[("n", &lista)])
}

/// O que o relatório lê do pacote além do `tecnico.json`: o `projeto.json` (texto das telas de texto; só vale com o
/// SHA-256 de `feito.projeto`) e o `mapa.csv` (intervalos entre imagens nos trechos usados). Sem eles o relatório sai
/// sem esses textos.
#[derive(Debug, Default, Clone, Copy)]
pub struct Complementos<'a> {
    pub projeto: Option<&'a [u8]>,
    pub mapa: Option<&'a str>,
}

/// Texto de cada tela de texto do `projeto.json` (`principal[]` de tipo `titulo`), por id do clipe: os blocos com
/// texto, na ordem título, subtítulo, corpo, rodapé.
fn textos_das_telas(projeto: &Value) -> HashMap<String, Vec<TextoTela>> {
    const ORDEM: [&str; 4] = ["titulo", "subtitulo", "corpo", "rodape"];
    let mut telas = HashMap::new();
    for c in projeto["principal"].as_array().into_iter().flatten().filter(|c| c["tipo"] == "titulo") {
        let Some(id) = c["id"].as_str() else { continue };
        let mut blocos: Vec<&Value> = c["titulo"]["blocos"].as_array().into_iter().flatten().filter(|b| b["conteudo"].as_str().is_some_and(|t| !t.trim().is_empty())).collect();
        blocos.sort_by_key(|b| ORDEM.iter().position(|p| b["papel"] == *p).unwrap_or(ORDEM.len()));
        let textos =
            blocos.into_iter().map(|b| TextoTela { papel: rot_ou("bloco", b["papel"].as_str().unwrap_or("")), texto: b["conteudo"].as_str().unwrap_or("").trim().to_string() }).collect();
        telas.insert(id.to_string(), textos);
    }
    telas
}

/// Entrada e saída de cada sobreposto com `animacao` (Fase 4; sem ela, "aparecer" nos dois lados, o padrão: nada a
/// dizer), por id do clipe: "Entra deslizando de baixo em 0,3 s e some de uma vez." Valor fora do catálogo: nada.
fn entradas_e_saidas(projeto: &Value) -> HashMap<String, String> {
    let parte = |qual: &str, a: &Value, d: f64| -> Option<String> {
        // aparecer ou deslizar sem duração é corte (`sobreposto::transformacao_em`); sem o campo, nada se afirma
        let s = a[qual].as_str()?;
        let s = if s == "corte" || (d > 0.0 && d.is_finite()) { s } else { "corte" };
        let lado = frases::rotulo(&format!("surgir.{}.{}", if qual == "entrada" { "de" } else { "para" }, a["lado"].as_str().unwrap_or("baixo")))?;
        Some(frases::preencher(frases::rotulo(&format!("surgir.{qual}.{s}"))?, &[("d", &num(d, 2)), ("lado", lado)]))
    };
    let mut v = HashMap::new();
    for c in projeto["sobreposicoes"].as_array().into_iter().flatten().flat_map(|t| t["clipes"].as_array().into_iter().flatten()) {
        let (Some(id), a) = (c["id"].as_str(), &c["animacao"]) else { continue };
        // "aparecer" nos dois lados é o padrão, com ou sem o campo
        if !a.is_object() || (a["entrada"] == "aparecer" && a["saida"] == "aparecer") {
            continue;
        }
        let d = |k: &str| c[k].as_f64().unwrap_or(0.0);
        if let (Some(e), Some(s)) = (parte("entrada", a, d("entrada_s")), parte("saida", a, d("saida_s"))) {
            let frase = format!("{e} e {s}.");
            let mut cs = frase.chars();
            v.insert(id.to_string(), cs.next().map(|p| p.to_uppercase().chain(cs).collect()).unwrap_or_default());
        }
    }
    v
}

/// `dados.json` do modelo, com todos os valores já em texto pronto para imprimir.
pub fn dados(tecnico: &Value, miniaturas: &[Miniatura], arquivos: &[ArquivoDoPacote]) -> Result<Value> {
    dados_com(tecnico, miniaturas, arquivos, Complementos::default())
}

/// `dados` com o que vem do pacote (`Complementos`).
pub fn dados_com(tecnico: &Value, miniaturas: &[Miniatura], arquivos: &[ArquivoDoPacote], extra: Complementos) -> Result<Value> {
    if tecnico.get("esquema").and_then(Value::as_str) != Some(tecnico::ESQUEMA_TECNICO) {
        return Err(SicroError::Validation("relatorio: tecnico.json de outro esquema".into()));
    }
    let t: Tecnico = serde_json::from_value(tecnico.clone())?;
    let (f, e) = (&t.feito, &t.execucao);
    // o projeto só conta se é o que o `feito` registra
    let projeto_lido = extra
        .projeto
        .filter(|b| !f.projeto.sha256.is_empty() && format!("{:x}", Sha256::digest(b)) == f.projeto.sha256)
        .and_then(|b| serde_json::from_slice::<Value>(b).ok());
    let mut telas = projeto_lido.as_ref().map(textos_das_telas).unwrap_or_default();
    let surgir = projeto_lido.as_ref().map(entradas_e_saidas).unwrap_or_default();
    let intervalos = extra.mapa.map(intervalos_do_mapa).unwrap_or_default();
    let com_som = f.audio.mixagem.is_some() || e.conteudo.audio_sha256.is_some();
    let id_da_midia = |id: &str| f.midias.iter().position(|m| m.id == id).map(letra).unwrap_or_else(|| "—".into());
    let n_do_clipe = |id: &str| f.roteiro.iter().find(|it| it.clipe == id).map(|it| it.n);

    // ---- saída
    let video = nome_do_arquivo(&e.saida.arquivo).to_string();
    let ext = video.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let cod = e.saida.codificador.to_ascii_lowercase();
    let codec_saida = if cod.contains("264") {
        "h264"
    } else if cod.contains("265") || cod.contains("hevc") {
        "hevc"
    } else if cod.contains("ffv1") {
        "ffv1"
    } else {
        ""
    };
    let saida = Saida {
        arquivo: video.clone(),
        sha256: e.saida.sha256.clone(),
        tamanho: tamanho(e.saida.tamanho),
        duracao: duracao(f.saida.duracao_s),
        imagem: format!("{} × {} pontos", f.saida.largura, f.saida.altura),
        ritmo: frases::preencher(rot("ritmo.saida"), &[("fps", &taxa(f.saida.fps.num, f.saida.fps.den))]),
        formato: format!("{}, {}", formato(if ext == "mkv" { "matroska" } else { &ext }, codec_saida, &video), rot(if com_som { "som.com" } else { "som.sem" })),
        perfil: f.saida.predefinicao.as_deref().map(|p| rot_ou("predefinicao", p)).unwrap_or_else(|| "—".into()),
        com_perdas: f.saida.com_perdas,
    };

    // ---- roteiro, acréscimos e mudanças
    let mut roteiro = Vec::new();
    // (tipo, rótulo) → itens, na ordem em que aparecem
    let mut usos: Vec<(String, String, Vec<u32>)> = Vec::new();
    let mut usar = |tipo: &str, rotulo: String, n: u32| match usos.iter_mut().find(|u| u.0 == tipo && u.1 == rotulo) {
        Some(u) => {
            if !u.2.contains(&n) {
                u.2.push(n)
            }
        }
        None => usos.push((tipo.to_string(), rotulo, vec![n])),
    };
    // formas das marcações do vídeo inteiro, para a linha da seção 5
    let mut formas_destaque: Vec<&str> = Vec::new();
    for it in &f.roteiro {
        let mut efeitos: Vec<Efeito> = Vec::new();
        let simples = |efeitos: &mut Vec<Efeito>, r: String| {
            if !efeitos.iter().any(|e| e.marcas.is_empty() && e.texto == r) {
                efeitos.push(Efeito::so(r));
            }
        };
        // marcações, lupas e holofotes do item: um resumo de cada tipo, no lugar do primeiro
        let do_tipo = |nome: &str| it.processamentos.iter().filter(|p| p.nome == nome).map(|p| &p.parametros).collect::<Vec<_>>();
        let mut resumidos: Vec<&str> = Vec::new();
        for p in &it.processamentos {
            let Some(t) = tipo_do_proc(p) else { continue };
            let r = rotulo_do_proc(p, t);
            usar(t, r.clone(), it.n);
            match t {
                "destaque" | "lupa" | "holofote" if resumidos.contains(&t) => {}
                "destaque" => {
                    resumidos.push(t);
                    let ps = do_tipo(t);
                    for x in &ps {
                        let fm = x["forma"].as_str().unwrap_or("");
                        if !formas_destaque.contains(&fm) {
                            formas_destaque.push(fm);
                        }
                    }
                    efeitos.extend(legivel::destaques(&ps).into_iter().map(|(texto, marcas)| Efeito { texto, marcas }));
                }
                "lupa" => {
                    resumidos.push(t);
                    simples(&mut efeitos, legivel::lupas(&do_tipo(t)));
                }
                "holofote" => {
                    resumidos.push(t);
                    simples(&mut efeitos, legivel::holofotes(do_tipo(t).len()));
                }
                _ => simples(&mut efeitos, r),
            }
        }
        // ocultação da MÍDIA (D17, o padrão da tela) não está nos processamentos do clipe: muda a imagem do item do
        // mesmo jeito (sem isto o relatório dizia "mostradas como foram registradas" com um rosto coberto)
        for o in f.ocultacoes.iter().filter(|o| o.clipes.iter().any(|c| *c == it.clipe)) {
            let t = tipo_da_ocultacao(&o.modo);
            let r = format!("{} ({})", frases::rotulo_do_efeito(t).unwrap_or(""), rot_ou("conteudo", &o.conteudo));
            usar(t, r.clone(), it.n);
            simples(&mut efeitos, r);
        }
        for x in extras_do_item(it, com_som) {
            let r = frases::rotulo_do_efeito(x).unwrap_or(x).to_string();
            usar(x, r.clone(), it.n);
            // o tipo do item e a velocidade já estão escritos no bloco
            if !matches!(x, tipo::CONGELAR | tipo::TITULO | tipo::IMAGEM_CLIPE | tipo::CAMERA_LENTA | tipo::ACELERADO) {
                efeitos.push(Efeito::so(r));
            }
        }
        let origem = it.origem.as_ref().map(|o| {
            let g = format!("{} {}", rot("especie.video").to_lowercase(), id_da_midia(&o.midia));
            if it.tipo == "congelar" {
                format!("{g}, no instante {}", tempo(o.de_s))
            } else {
                format!("{g}, de {} a {}", tempo(o.de_s), tempo(o.ate_s))
            }
        });
        // o texto da tela e o dos títulos sobrepostos que aparecem no item, como estão no vídeo
        let mut textos = if it.tipo == "titulo" { telas.remove(&it.clipe).unwrap_or_default() } else { Vec::new() };
        for s in f.sobrepostos.iter().filter(|s| s.tipo == "titulo" && itens_nos_slots(&f.roteiro, s.na_saida.de_q, s.na_saida.n_q).contains(&it.n)) {
            let texto = s.textos.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n");
            if !texto.is_empty() {
                textos.push(TextoTela { papel: frases::rotulo_do_efeito(tipo::TEXTO_SOBREPOSTO).unwrap_or("").to_string(), texto });
            }
        }
        roteiro.push(ItemRoteiro {
            n: it.n,
            tipo: rot_ou("clipe", &it.tipo),
            descricao: it.nome.clone(),
            na_saida: format!("{} a {}", tempo(it.na_saida.de_s), tempo(it.na_saida.ate_s)),
            miniatura: miniaturas.iter().find(|m| m.n == it.n).map(|m| format!("img/{}.jpg", m.n)),
            origem,
            velocidade: velocidade_txt(it),
            efeitos,
            selos: it.selos.clone(),
            textos,
        });
    }
    if com_som {
        for trilha in &f.audio.trilhas {
            let t = match trilha.papel.as_str() {
                "narracao" => tipo::NARRACAO,
                "musica" => tipo::MUSICA,
                _ => tipo::EFEITOS_SONOROS,
            };
            for c in &trilha.clipes {
                for n in itens_nos_slots(&f.roteiro, c.de_q, c.n_q) {
                    usar(t, frases::rotulo_do_efeito(t).unwrap_or(t).to_string(), n);
                }
            }
        }
        for s in &f.audio.silencios {
            for it in f.roteiro.iter().filter(|it| it.origem.as_ref().is_some_and(|o| o.midia == s.midia && o.de_s < s.ate_s && s.de_s < o.ate_s)) {
                usar(tipo::SILENCIO, frases::rotulo_do_efeito(tipo::SILENCIO).unwrap_or("").to_string(), it.n);
            }
        }
    }
    let (mut acrescimos, mut mudancas) = (Vec::new(), Vec::new());
    // legendas que saíram: na imagem, em arquivo ao lado do vídeo ou as duas; valem ao longo do vídeo
    let modo_das_legendas = f.legendas.modo.as_deref().filter(|m| *m != "nenhuma" && f.legendas.n > 0);
    let com_legendas = modo_das_legendas.is_some();
    if let Some(modo) = modo_das_legendas {
        let onde = frases::rotulo(&format!("legendas.{modo}")).unwrap_or(rot("onde.video")).to_string();
        acrescimos.push(Linha { o_que: format!("{} ({})", frases::rotulo_do_efeito(tipo::LEGENDAS).unwrap_or(""), f.legendas.n), onde, explicacao: frases::do_efeito(tipo::LEGENDAS).unwrap_or("").to_string() });
    }
    // sobrepostos (texto, terço inferior, imagem, marca): um por linha, com o tempo do vídeo e os itens que cobrem
    for s in &f.sobrepostos {
        let t = if s.tipo == "titulo" { tipo::TEXTO_SOBREPOSTO } else { "imagem" };
        let rotulo = frases::rotulo_do_efeito(t).unwrap_or(t);
        // o que identifica o item para quem lê: o primeiro texto, senão o nome dado pelo perito
        let qual = s.textos.first().map(|x| x.replace('\n', " ")).or_else(|| Some(s.nome.clone())).filter(|x| !x.trim().is_empty());
        let o_que = match qual {
            Some(q) => format!("{rotulo}: {}", cortar(q.trim().to_string(), MAX_TEXTO_SOBREPOSTO)),
            None => rotulo.to_string(),
        };
        let itens = itens_nos_slots(&f.roteiro, s.na_saida.de_q, s.na_saida.n_q);
        let faixa = frases::preencher(rot("onde.de_ate"), &[("de", &tempo(s.na_saida.de_s)), ("ate", &tempo(s.na_saida.ate_s))]);
        let onde = if itens.is_empty() { faixa } else { format!("{}, {faixa}", onde(&itens)) };
        // entrada e saída quando não são o padrão (Fase 4, texto livre)
        let explicacao = [frases::do_efeito(t).unwrap_or(""), surgir.get(&s.clipe).map(String::as_str).unwrap_or("")].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(" ");
        acrescimos.push(Linha { o_que, onde, explicacao });
    }
    // avisos que o programa escreve na imagem (selos): também são acréscimo; um por texto, com os itens que o levam
    let mut avisos: Vec<(&str, Vec<u32>)> = Vec::new();
    for it in &f.roteiro {
        for s in it.selos.iter().filter(|s| !s.trim().is_empty()) {
            match avisos.iter_mut().find(|a| a.0 == s.as_str()) {
                Some(a) => a.1.push(it.n),
                None => avisos.push((s.as_str(), vec![it.n])),
            }
        }
    }
    // o selo do sobreposto (imagem por IA) vale nos itens em que ele aparece
    for s in &f.sobrepostos {
        let itens = itens_nos_slots(&f.roteiro, s.na_saida.de_q, s.na_saida.n_q);
        for x in s.selos.iter().filter(|x| !x.trim().is_empty() && !itens.is_empty()) {
            let i = match avisos.iter().position(|a| a.0 == x.as_str()) {
                Some(i) => i,
                None => {
                    avisos.push((x.as_str(), Vec::new()));
                    avisos.len() - 1
                }
            };
            for n in &itens {
                if !avisos[i].1.contains(n) {
                    avisos[i].1.push(*n);
                }
            }
            avisos[i].1.sort_unstable();
        }
    }
    for (texto, itens) in &avisos {
        let rotulo = frases::rotulo_do_efeito(tipo::AVISO_NA_IMAGEM).unwrap_or("");
        acrescimos.push(Linha { o_que: format!("{rotulo}: {texto}"), onde: onde(itens), explicacao: frases::do_efeito(tipo::AVISO_NA_IMAGEM).unwrap_or("").to_string() });
    }
    // forma fora do catálogo: a linha não diz as formas
    let formas: Vec<String> = formas_destaque.iter().map(|f| legivel::plural_da_forma(f).map(str::to_string)).collect::<Option<_>>().unwrap_or_default();
    for (t, rotulo, itens) in &usos {
        // "Marcação (setas e retângulos)": o que foi desenhado; cada uma, no item do roteiro
        let o_que = if t == "destaque" && !formas.is_empty() { frases::preencher(rot("marca.secao"), &[("formas", &legivel::lista_e(&formas))]) } else { rotulo.clone() };
        let linha = Linha { o_que, onde: onde(itens), explicacao: frases::do_efeito(t).unwrap_or("").to_string() };
        match frases::classe_do_efeito(t) {
            Some(Classe::Acrescimo) => acrescimos.push(linha),
            // tipo fora do catálogo (versão mais nova) entra como mudança, sem frase
            _ => mudancas.push(linha),
        }
    }

    // ---- inteligência artificial
    let rastreio = |id: &str| f.rastreios.iter().find(|r| r.id == id);
    let mut ia_resumo = Vec::new();
    // toda ocultação de origem automática que chega à exportação passou pelo perito: confirmada (`confirmacao`, que a
    // validação exige) ou revisada na análise (`pessoas` em estado `revisada`)
    let n_oc = f.ocultacoes.iter().filter(|o| rastreio(&o.rastreio).is_some_and(automatico)).count();
    if n_oc > 0 {
        ia_resumo.push(frases::preencher(modelo::IA_OCULTACOES, &[("n", &n_oc.to_string())]));
    }
    // como cada imagem coberta por elas obteve a posição (propostas, confirmadas e calculadas: P4.5)
    let q = f.ocultacoes.iter().filter(|o| rastreio(&o.rastreio).is_some_and(automatico)).fold(FQuadros::default(), |mut a, o| {
        a.ancora += o.quadros.ancora;
        a.medido += o.quadros.medido + o.quadros.ancora_do_programa;
        a.interpolado += o.quadros.interpolado;
        a
    });
    if n_oc > 0 {
        let (achadas, calculadas, marcadas) = (milhar(q.medido as u64), milhar(q.interpolado as u64), milhar(q.ancora as u64));
        ia_resumo.push(if q.ancora > 0 {
            frases::preencher(modelo::IA_OCULTACOES_QUADROS_PERITO, &[("achadas", &achadas), ("calculadas", &calculadas), ("marcadas", &marcadas)])
        } else {
            frases::preencher(modelo::IA_OCULTACOES_QUADROS, &[("achadas", &achadas), ("calculadas", &calculadas)])
        });
    }
    let procs = || f.roteiro.iter().flat_map(|it| it.processamentos.iter().map(move |p| (it, p)));
    // marcações e ampliações que acompanham alvo de origem automática, contadas por rastreio
    let automaticos = |enquadramento: bool| -> usize {
        let mut v: Vec<&str> = procs()
            .filter(|(_, p)| p.nome != "ocultar" && (p.nome == "enquadramento") == enquadramento)
            .filter_map(|(_, p)| rastreio_do_proc(p))
            .filter(|id| rastreio(id).is_some_and(automatico))
            .collect();
        v.sort_unstable();
        v.dedup();
        v.len()
    };
    let marcados = automaticos(false);
    if marcados > 0 {
        ia_resumo.push(frases::preencher(modelo::IA_MARCACOES, &[("n", &marcados.to_string())]));
    }
    let ampliacoes = automaticos(true);
    if ampliacoes > 0 {
        ia_resumo.push(frases::preencher(modelo::IA_AMPLIACOES, &[("n", &ampliacoes.to_string())]));
    }
    // rastreios em que um programa atuou (anexo H), na ordem do `feito`: "R1", "R2"…; `lidas` = as corridas que deram
    // amostra a algum quadro usado (`corridas_no_video`), `fora` = as outras
    struct Listado<'a> {
        r: &'a FRastreio,
        id: String,
        lidas: Vec<Corrida>,
        fora: usize,
        ilegiveis: usize,
    }
    let listados: Vec<Listado> = f
        .rastreios
        .iter()
        .map(|r| (r, corridas_de(r)))
        .filter(|(r, (lidas, programa, ilegiveis))| automatico(r) || r.origem["tipo"] == "assistido" || !lidas.is_empty() || !programa.is_empty() || *ilegiveis > 0)
        .enumerate()
        .map(|(i, (r, (todas, programa, ilegiveis)))| {
            let usada = corridas_no_video(&todas, &faixas_de_uso(f, &r.id));
            let (lidas, fora): (Vec<(Corrida, bool)>, Vec<(Corrida, bool)>) = todas.into_iter().zip(usada).partition(|x| x.1);
            let mut lidas: Vec<Corrida> = lidas.into_iter().map(|x| x.0).collect();
            // a âncora do "Simplificar" leva a posição até a vizinha (interpolação) e mistura amostras (média): a faixa
            // da corrida não diz que quadro ela tocou, então conta sempre (nunca IA a menos)
            lidas.extend(programa);
            Listado { r, id: format!("R{}", i + 1), lidas, fora: fora.len(), ilegiveis }
        })
        .collect();
    let id_listado = |id: &str| listados.iter().find(|l| l.r.id == id).map(|l| l.id.clone());
    // rastreio assistido: IA só com corrida de motor neural registrada que pode ter dado amostra ao vídeo; sem corrida
    // legível, ou com posição do "Simplificar" no vídeo sem `corridas_do_programa` (`feito` de antes dele: o programa
    // que a escolheu saiu com o denso), não se sabe qual programa (e então "nenhuma etapa por IA" também não pode ser
    // dito, qualquer que seja a origem); só correlação (mancha) não é IA; corridas só fora dos trechos usados não
    // dizem nada do vídeo
    let (mut com_ia, mut sem_registro) = (0usize, 0usize);
    for l in listados.iter().filter(|l| !automatico(l.r)) {
        let assistido = l.r.origem["tipo"] == "assistido";
        let com_neural = l.lidas.iter().any(neural);
        let do_programa = l.r.corridas_do_programa.is_empty() && f.ocultacoes.iter().any(|o| o.rastreio == l.r.id && o.quadros.ancora_do_programa + o.quadros.interpolado_do_programa > 0);
        if assistido && com_neural {
            com_ia += 1;
        } else if !com_neural && (l.ilegiveis > 0 || (assistido && l.lidas.is_empty() && l.fora == 0) || do_programa) {
            sem_registro += 1;
        }
    }
    if com_ia > 0 {
        ia_resumo.push(frases::preencher(modelo::IA_ASSISTIDO, &[("n", &com_ia.to_string())]));
    }
    if sem_registro > 0 {
        ia_resumo.push(frases::preencher(modelo::IA_ASSISTIDO_SEM_REGISTRO, &[("n", &sem_registro.to_string())]));
    }
    if let Some(a) = f.avisos.iter().find(|a| a.codigo == codigo::IMAGEM_IA) {
        ia_resumo.push(frases::preencher(modelo::IA_IMAGENS, &[("n", &a.n.max(1).to_string())]));
    }
    if com_legendas && f.legendas.origem.as_ref().is_some_and(|o| o.motor.as_deref().is_some_and(|m| !m.trim().is_empty())) {
        // "conferidas" só quando o `feito` não conta legenda automática sem a conferência registrada
        let sem = f.legendas.ia.saturating_sub(f.legendas.ia_conferidas);
        ia_resumo.push(if sem == 0 { modelo::IA_LEGENDAS.to_string() } else { frases::preencher(modelo::IA_LEGENDAS_SEM_CONFERENCIA, &[("n", &sem.to_string())]) });
    }
    // modelos por arquivo: `feito.ia` (P2.1 agrupa por modelo e provedor, de todas as corridas) só com os das corridas
    // que podem ter dado amostra ao vídeo, e os delas que ele ainda não traz
    let no_video_ia = |m: &FIa| m.uso.trim() != "rastreio" || listados.iter().flat_map(|l| &l.lidas).filter(|c| neural(c)).any(|c| FIa::da_corrida(c, "rastreio").mesmo(m));
    let mut modelos: Vec<FIa> = f.ia.iter().filter(|m| no_video_ia(m)).cloned().collect();
    for c in listados.iter().flat_map(|l| &l.lidas).filter(|c| com_modelo(c)) {
        let x = FIa::da_corrida(c, uso_da(c));
        if !modelos.iter().any(|m| m.mesmo(&x)) {
            modelos.push(x);
        }
    }
    let legendas_por = f.legendas.origem.as_ref().filter(|_| com_legendas).and_then(|o| o.motor.as_deref()).map(str::trim).filter(|m| !m.is_empty());
    if !modelos.is_empty() || legendas_por.is_some() {
        ia_resumo.push(modelo::IA_MODELOS.to_string());
    }
    if ia_resumo.is_empty() {
        ia_resumo.push(cond(condicao::IA_NENHUMA));
    }

    // ---- relógios e medidas mostrados na imagem
    let mostra: Vec<&str> = procs().filter(|(_, p)| p.nome == "carimbo").filter_map(|(_, p)| p.parametros["mostra"].as_str()).collect();
    let mut relogios: Vec<String> = [
        ("tempo_fonte", condicao::RELOGIO_TEMPO_FONTE),
        ("relogio", condicao::RELOGIO_CAMERA),
        ("quadro", condicao::RELOGIO_QUADRO),
        ("tempo_programa", condicao::RELOGIO_TEMPO_PROGRAMA),
    ]
    .iter()
    .filter(|(m, _)| mostra.contains(m))
    .map(|(_, c)| cond(c))
    .collect();
    if relogios.is_empty() {
        relogios.push(cond(condicao::SEM_RELOGIO));
    }
    let mut medidas = Vec::new();
    for (it, p) in procs() {
        let item = it.n.to_string();
        match p.nome.as_str() {
            "regua" => medidas.push(frases::preencher(
                modelo::MEDIDA_REGUA,
                &[("m", &dec(p.parametros["comprimento_m"].as_f64().unwrap_or(0.0), 2)), ("item", &item), ("residuo", &dec(p.parametros["calibracao"]["residuo_m"].as_f64().unwrap_or(0.0), 2))],
            )),
            "velocidade_exibida" => {
                let c = &p.parametros["calculo"];
                let kmh = num(c["kmh"].as_f64().unwrap_or(0.0), 1);
                medidas.push(match (c["faixa"][0].as_f64(), c["faixa"][1].as_f64()) {
                    (Some(a), Some(b)) => frases::preencher(modelo::MEDIDA_VELOCIDADE, &[("item", &item), ("kmh", &kmh), ("de", &num(a, 1)), ("ate", &num(b, 1))]),
                    _ => frases::preencher(modelo::MEDIDA_VELOCIDADE_SEM_FAIXA, &[("item", &item), ("kmh", &kmh)]),
                });
            }
            _ => {}
        }
    }

    // ---- sincronização
    let sincronizacao = f
        .sincronias
        .iter()
        .map(|s| {
            let incerteza = match s.incerteza_s {
                Some(i) => format!("± {} s", dec(i, 3)),
                None => cond(condicao::INCERTEZA_NAO_DETERMINADA),
            };
            let visiveis = s.conferencias.iter().filter(|c| c["modo"] == "evento_visivel").count();
            let conferencia = if visiveis > 0 {
                frases::preencher(rot("sincronia.conferida"), &[("n", &visiveis.to_string())])
            } else if s.conferencias.is_empty() {
                rot("sincronia.sem_conferencia").to_string()
            } else {
                rot("sincronia.sem_evento").to_string()
            };
            let sinal = |v: f64| if v < 0.0 { "−" } else { "+" };
            Sincronia {
                par: format!("{} e {}", id_da_midia(&s.referencia), id_da_midia(&s.outra)),
                explicacao: cond(condicao::SINCRONIA),
                metodo: rot_ou("sincronia", &s.metodo),
                deslocamento: format!("{} s", dec(s.desloc_s, 3)),
                deriva: format!("{} s a cada minuto", dec(s.deriva_ppm * 60e-6, 4)),
                incerteza: incerteza.clone(),
                conferencia,
                tecnico: SincroniaTecnico {
                    metodo: s.metodo.clone(),
                    deslocamento: format!("{}{} s", sinal(s.desloc_s), dec(s.desloc_s.abs(), 3)),
                    deriva: format!("{}{} ppm (± {})", sinal(s.deriva_ppm), num(s.deriva_ppm.abs(), 1), num(s.deriva_incerteza_ppm, 1)),
                    incerteza,
                },
            }
        })
        .collect();

    // ---- limites
    let mut limitacoes = Vec::new();
    if f.saida.com_perdas {
        limitacoes.push(cond(condicao::COM_PERDAS));
    }
    if f.saida.predefinicao.as_deref() == Some("whatsapp") {
        limitacoes.push(cond(condicao::SO_COMO_DOCUMENTO));
    }
    // "entre duas marcações do perito" só quando toda interpolação foi entre duas âncoras do perito (nenhuma do
    // "Simplificar", nenhuma linha interpolada do denso); `feito` sem a contagem não afirma
    let (interpolados, entre_marcacoes) = f.ocultacoes.iter().fold((0u64, 0u64), |(a, b), o| (a + o.quadros.interpolado as u64, b + o.quadros.interpolado_entre_marcacoes as u64));
    let do_perito = interpolados > 0 && entre_marcacoes == interpolados;
    for a in &f.avisos {
        let cod = if a.codigo == codigo::OCULTAR_INTERPOLADO && !do_perito { condicao::OCULTAR_INTERPOLADO_CALCULADO } else { a.codigo.as_str() };
        // aviso de código que o catálogo não conhece (versão mais nova) sai pelo código, nunca calado
        let frase = frases::da_condicao(cod).map(str::to_string).unwrap_or_else(|| a.codigo.clone());
        limitacoes.push(if a.ciente { format!("{frase} {}", cond(condicao::CIENTE)) } else { frase });
    }
    if f.midias.iter().any(|m| m.recebido.nativo == Some(false)) {
        limitacoes.push(cond(condicao::RECEBIDA_CONVERTIDA));
    }
    match e.conferencia.ocultacoes.as_deref() {
        Some("nao_conferida") => limitacoes.push(cond(condicao::OCULTACAO_NAO_CONFERIDA)),
        Some("pendente") => limitacoes.push(cond(condicao::OCULTACAO_PENDENTE)),
        _ => {}
    }
    if !f.selos_desligados.is_empty() {
        limitacoes.push(cond(condicao::SELO_DESLIGADO));
    }
    if e.saida.queda_para_cpu.is_some() {
        limitacoes.push(cond(condicao::QUEDA_PARA_CPU));
    }
    limitacoes.push(cond(condicao::APARENCIA));
    let selos_desligados = f.selos_desligados.iter().map(|s| rot_ou("selo", s)).collect();

    // ---- conferência por quem recebe
    let conferir = Conferir {
        arquivo: video.clone(),
        windows: format!("certutil -hashfile \"{video}\" SHA256"),
        linux: format!("sha256sum \"{video}\""),
        quadros: format!("ffmpeg -i \"{video}\" -map 0:v:0 -f framehash -hash sha256 -"),
        lista: arquivos.iter().map(|a| nome_do_arquivo(&a.rel)).find(|n| *n == LISTA_DE_QUADROS).map(str::to_string),
        prova_arquivo: cond(condicao::PROVA_ARQUIVO),
        prova_quadros: cond(condicao::PROVA_QUADROS),
    };

    // ---- anexos técnicos C, D e E
    // rastreio citado no anexo C: o "R" do anexo H; o que não está lá, pelo começo do id e pela origem
    let ref_rastreio = |id: &str| -> String {
        id_listado(id).unwrap_or_else(|| {
            let curto: String = id.chars().take(8).collect();
            // origem e "marcações do perito: n" (o contador do painel, `marcadasAMao`)
            match rastreio(id).map(origem_e_marcacoes).filter(|v| !v.is_empty()) {
                Some(partes) => format!("{curto} ({})", partes.join(" · ")),
                None => curto,
            }
        })
    };
    let (mut processamentos, mut diferencas) = (Vec::new(), Vec::new());
    for it in &f.roteiro {
        let item = it.n.to_string();
        if let Some(o) = &it.origem {
            // rampa (kd = 0): sem fórmula, o slot de cada quadro lido vai em tabela
            let rampa = pontos_da_rampa(&it.velocidade);
            let chaves: &[&str] = if rampa.is_some() { &["c"] } else { &["c", "kn", "kd", "e"] };
            let mut mapa: Vec<String> = chaves.iter().filter_map(|k| o.mapa.get(k).map(|v| format!("{k}={v}"))).collect();
            if rampa.is_some() {
                mapa.push("slot de cada quadro em tabela".into());
            }
            let v = match (fator(&it.velocidade), &rampa) {
                (Some(x), _) => format!("; velocidade {}×", num(x, 3)),
                (None, Some(p)) => format!("; velocidade em rampa: {}", lista_da_rampa(p, |t| format!("{} s", dec(t, 6)))),
                _ => String::new(),
            };
            processamentos.push(Processamento {
                item: item.clone(),
                ordem: "0".into(),
                nome: rot("corte").to_string(),
                parametros: format!(
                    "{} {}: de {} s a {} s (pts {} a {}), {}{v}; mapa {}",
                    rot("especie.video").to_lowercase(),
                    id_da_midia(&o.midia),
                    dec(o.de_s, 6),
                    dec(o.ate_s, 6),
                    o.pts_primeiro,
                    o.pts_ultimo,
                    if o.lidos == 1 { "1 quadro lido".to_string() } else { format!("{} quadros lidos", o.lidos) },
                    if mapa.is_empty() { "—".into() } else { mapa.join(" ") }
                ),
            });
            let fonte = f.midias.iter().find(|m| m.id == o.midia).and_then(|m| por_segundo(m.medido.passo_med, &m.medido.tb)).map(|x| num(x, 2));
            let v = match (fator(&it.velocidade), faixa_da_rampa(&it.velocidade)) {
                (Some(x), _) if (x - 1.0).abs() > 1e-9 => format!(", a {}×", num(x, 3)),
                (None, Some((a, b))) => format!(", em rampa de {}× a {}×", num(a, 3), num(b, 3)),
                _ => String::new(),
            };
            diferencas.push(Diferenca {
                item: item.clone(),
                imagem: match f.midias.iter().find(|m| m.id == o.midia).filter(|m| m.largura > 0 && m.altura > 0) {
                    Some(m) => format!("{}×{} para {}×{}", m.largura, m.altura, f.saida.largura, f.saida.altura),
                    None => format!("para {}×{}", f.saida.largura, f.saida.altura),
                },
                taxa: if it.tipo == "congelar" { "quadro único".into() } else { format!("{} → {}{v}", fonte.unwrap_or_else(|| "—".into()), taxa(f.saida.fps.num, f.saida.fps.den)) },
                quadros: format!("{} → {}", o.lidos, it.na_saida.n_q),
                repetidos: o.repetidos.to_string(),
                descartados: o.descartados.to_string(),
                obs: if it.tipo == "congelar" { format!("mesmo quadro por {}", duracao(it.na_saida.ate_s - it.na_saida.de_s)) } else { "—".into() },
            });
        }
        // o `ativo` e as chaves animadas estão na base de tempo do clipe: a da gravação, no trecho
        let base = if it.origem.is_some() { "da gravação" } else { "do clipe" };
        for p in &it.processamentos {
            processamentos.push(Processamento {
                item: item.clone(),
                ordem: p.ordem.to_string(),
                nome: match tipo_do_proc(p) {
                    Some(t) => frases::rotulo_do_efeito(t).map(str::to_string).unwrap_or_else(|| t.to_string()),
                    None => rot("enquadramento.inteiro").to_string(),
                },
                parametros: legivel::parametros(&p.nome, &p.parametros, &ref_rastreio, base),
            });
        }
    }
    // planilha de ocultações (18-M §11, figura 4): gravação e arquivo original, intervalo no original, forma, filtro,
    // o que foi protegido e por quê; imagem primeiro, som depois, numeradas em seguida
    let nome_da_midia = |id: &str| f.midias.iter().find(|m| m.id == id).map(|m| ou_traco(&m.nome)).unwrap_or_else(|| "—".into());
    let mut ocultacoes = Vec::new();
    for o in &f.ocultacoes {
        let mut filtro = vec![rot_ou("modo", &o.modo)];
        if o.modo != "solido" {
            filtro.push(format!("intensidade {}", num(o.intensidade, 2)));
        } else if !o.cor.is_empty() {
            filtro.push(format!("cor {}", o.cor));
        }
        let mut forma = vec![rot_ou("forma", &o.forma)];
        if o.inverter {
            forma.push(rot("forma.invertida").to_string());
        }
        if o.margem > 0.0 {
            forma.push(format!("margem {} %", num(o.margem * 100.0, 1)));
        }
        if o.borda_suave > 0.0 {
            forma.push(format!("borda suave {} %", num(o.borda_suave * 100.0, 1)));
        }
        // de onde veio a região: origem do rastreio, marcações, confirmação e a linha dele no anexo H
        let regiao = match rastreio(&o.rastreio) {
            Some(r) => {
                let mut partes = origem_e_marcacoes(r);
                partes.extend(confirmacao_do_rastreio(r));
                partes.extend(id_listado(&r.id).map(|id| frases::preencher(rot("rastreio.ref"), &[("r", &id)])));
                ou_traco(&partes.join(" · "))
            }
            None => "—".into(),
        };
        let itens: Vec<String> = o.clipes.iter().filter_map(|c| n_do_clipe(c)).map(|n| n.to_string()).collect();
        let mut quadros = quadros_ocultos(&o.quadros, o.inverter);
        // ausências aceitas do rastreio no intervalo da ocultação (que inclui os quadros sem região delas)
        let ausencias: Vec<String> = rastreio(&o.rastreio)
            .into_iter()
            .flat_map(|r| &r.ausencias)
            .filter(|a| a.de_s <= o.ate_s && o.de_s <= a.ate_s)
            .map(|a| format!("{} a {} ({})", tempo(a.de_s), tempo(a.ate_s), rot_ou("ausencia", &a.motivo)))
            .collect();
        if !ausencias.is_empty() {
            quadros = format!("{quadros}; {}", frases::preencher(modelo::AUSENCIAS, &[("lista", &ausencias.join("; "))]));
        }
        ocultacoes.push(Ocultacao {
            n: (ocultacoes.len() + 1).to_string(),
            item: if itens.is_empty() { "—".into() } else { itens.join(", ") },
            gravacao: id_da_midia(&o.midia),
            arquivo: nome_da_midia(&o.midia),
            intervalo: format!("{} · {}", tempo(o.de_s), tempo(o.ate_s)),
            conteudo: rot_ou("conteudo", &o.conteudo),
            forma: forma.join(", "),
            filtro: filtro.join(", "),
            motivo: ou_traco(&o.motivo),
            quadros: Some(quadros),
            regiao: Some(regiao),
        });
    }
    if com_som {
        for s in &f.audio.silencios {
            let itens: Vec<String> = f
                .roteiro
                .iter()
                .filter(|it| it.origem.as_ref().is_some_and(|o| o.midia == s.midia && o.de_s < s.ate_s && s.de_s < o.ate_s))
                .map(|it| it.n.to_string())
                .collect();
            ocultacoes.push(Ocultacao {
                n: (ocultacoes.len() + 1).to_string(),
                item: if itens.is_empty() { "—".into() } else { itens.join(", ") },
                gravacao: id_da_midia(&s.midia),
                arquivo: nome_da_midia(&s.midia),
                intervalo: format!("{} · {}", tempo(s.de_s), tempo(s.ate_s)),
                conteudo: rot("conteudo.som").to_string(),
                forma: "—".into(),
                filtro: rot_ou("modo", &s.modo),
                motivo: ou_traco(&s.motivo),
                quadros: None,
                regiao: None,
            });
        }
    }
    let quem_quando = |c: &str| frases::preencher(modelo::CONFERIDA_POR, &[("frase", &cond(c)), ("por", &e.conferencia.por), ("quando", &data(&e.conferencia.em))]);
    let conferencia_ocultacoes = match e.conferencia.ocultacoes.as_deref() {
        Some("conferida") => Some(quem_quando(condicao::OCULTACAO_CONFERIDA)),
        Some("nao_conferida") => Some(quem_quando(condicao::OCULTACAO_NAO_CONFERIDA)),
        Some("pendente") => Some(cond(condicao::OCULTACAO_PENDENTE)),
        _ => None,
    };

    // ---- anexos G e H
    let por_gpu = gpu(&cod);
    let mut codificacao = vec![
        ["Predefinição".to_string(), saida.perfil.clone()],
        ["Codificador de vídeo".to_string(), format!("{}; argumentos: {}", ou_traco(&e.saida.codificador), ou_traco(&e.saida.argumentos.join(" ")))],
        ["Formato com perdas".to_string(), if f.saida.com_perdas { "sim" } else { "não" }.to_string()],
        ["Repetição bit a bit do arquivo".to_string(), rot(if por_gpu { "codificacao.gpu" } else { "codificacao.cpu" }).to_string()],
    ];
    if let Some(q) = &e.saida.queda_para_cpu {
        codificacao.push(["Queda para CPU".to_string(), q.clone()]);
    }
    if let (Some(q), Some(er)) = (e.conferencia.quadros, e.conferencia.erros) {
        codificacao.push(["Conferência da saída".to_string(), format!("{} quadros decodificados, {} erros", milhar(q), er)]);
    }
    for (rotulo, h) in [
        ("SHA-256 dos quadros antes da compressão", &e.conteudo.composto_sha256),
        ("SHA-256 dos quadros do arquivo entregue", &e.conteudo.decodificado_sha256),
        ("SHA-256 do som (PCM)", &e.conteudo.audio_sha256),
    ] {
        if let Some(h) = h {
            codificacao.push([rotulo.to_string(), h.clone()]);
        }
    }
    if let Some(x) = &f.fecho {
        codificacao.push(["Fecho".to_string(), format!("{}, {} s", x.tipo, dec(x.duracao_s, 3))]);
    }
    let mut maquina = vec![ou_traco(&e.sistema.cpu.modelo)];
    maquina.extend(e.sistema.gpu.iter().map(|g| format!("GPU {g}")));
    maquina.extend(e.sistema.driver.iter().map(|d| format!("driver {d}")));
    let mut software = vec![
        [
            "SICRO".to_string(),
            if e.sicro.commit.trim().is_empty() { ou_traco(&e.sicro.versao) } else { format!("{} ({})", ou_traco(&e.sicro.versao), e.sicro.commit.trim()) },
            rot("papel.sicro").to_string(),
        ],
        [
            "FFmpeg".to_string(),
            ou_traco(&e.ffmpeg.versao),
            format!("{} · origem: {} · SHA-256 {}", rot("papel.ffmpeg"), ou_traco(&rot_ou("ffmpeg", e.ffmpeg.origem.trim())), ou_traco(&e.ffmpeg.sha256)),
        ],
        ["Typst".to_string(), ou_traco(&e.typst), rot("papel.typst").to_string()],
    ];
    // o programa que rodou os modelos ("onnxruntime 1.31.1") e onde (CPU, CUDA): a saída da GPU não é a da CPU (§6.6)
    let mut runtimes: Vec<(&str, Vec<String>)> = Vec::new();
    for m in modelos.iter().filter(|m| !m.runtime.trim().is_empty()) {
        let p = provedor(&m.provedor);
        match runtimes.iter_mut().find(|x| x.0 == m.runtime.trim()) {
            Some(x) if p.is_empty() || x.1.contains(&p) => {}
            Some(x) => x.1.push(p),
            None => runtimes.push((m.runtime.trim(), if p.is_empty() { vec![] } else { vec![p] })),
        }
    }
    for (rt, provedores) in runtimes {
        let (programa, versao) = rt.split_once(' ').map(|(a, b)| (a.to_string(), b.trim().to_string())).unwrap_or((rt.to_string(), "—".into()));
        let papel = if provedores.is_empty() { rot("papel.runtime").to_string() } else { format!("{} · {}", rot("papel.runtime"), provedores.join(", ")) };
        software.push([programa, versao, papel]);
    }
    software.push(["Sistema".to_string(), ou_traco(&e.sistema.so), maquina.join(" · ")]);
    // rastreios em que cada modelo rodou
    let refs = |m: &FIa| -> Vec<String> {
        listados
            .iter()
            .filter(|l| l.lidas.iter().filter(|c| com_modelo(c) && uso_da(c) == m.uso.trim()).any(|c| FIa::da_corrida(c, &m.uso).mesmo(m)))
            .map(|l| l.id.clone())
            .collect()
    };
    let mut modelos_ia: Vec<ModeloIa> = modelos
        .iter()
        .map(|m| {
            let rs = refs(m);
            let uso = rot_ou("uso", m.uso.trim());
            ModeloIa {
                modelo: ou_traco(&m.modelo),
                arquivos: m.arquivos.clone(),
                origem: ou_traco(&[m.origem.trim(), m.commit.trim()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(" · ")),
                execucao: ou_traco(&[m.runtime.trim().to_string(), provedor(&m.provedor)].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" · ")),
                desvio: Some(m.desvio.trim().to_string()).filter(|d| !d.is_empty()),
                uso: if rs.is_empty() { ou_traco(&uso) } else { format!("{} ({})", ou_traco(&uso), rs.join(", ")) },
            }
        })
        .collect();
    // o motor da degravação das legendas que saíram (§6.6)
    if let (Some(motor), Some(o)) = (legendas_por, f.legendas.origem.as_ref()) {
        let arquivos = o.modelo_sha256.iter().filter(|s| !s.trim().is_empty()).map(|s| FArquivo { nome: rot("arquivo.modelo").to_string(), sha256: s.clone() }).collect();
        modelos_ia.push(ModeloIa { modelo: motor.to_string(), arquivos, origem: "—".into(), execucao: "—".into(), desvio: None, uso: rot("uso.legendas").to_string() });
    }
    // anexo H: cada rastreio em que um programa atuou, com as corridas e o código das amostras
    let midia_do_rastreio = |id: &str| -> Option<&FMidia> {
        let m = f.ocultacoes.iter().find(|o| o.rastreio == id).map(|o| o.midia.as_str()).or_else(|| {
            f.roteiro.iter().find(|it| it.processamentos.iter().any(|p| rastreio_do_proc(p) == Some(id))).and_then(|it| it.origem.as_ref()).map(|o| o.midia.as_str())
        })?;
        f.midias.iter().find(|x| x.id == m)
    };
    let rastreios_anexo = listados
        .iter()
        .map(|l| {
            let tb = midia_do_rastreio(&l.r.id).map(|m| &m.medido.tb);
            let mut corridas: Vec<String> = l.lidas.iter().map(|c| corrida_txt(c, tb)).collect();
            if l.fora > 0 {
                corridas.push(frases::preencher(modelo::CORRIDAS_FORA, &[("n", &l.fora.to_string())]));
            }
            if l.ilegiveis > 0 {
                corridas.push(frases::preencher(modelo::CORRIDAS_ILEGIVEIS, &[("n", &l.ilegiveis.to_string())]));
            }
            let ocs: Vec<usize> = f.ocultacoes.iter().enumerate().filter(|(_, o)| o.rastreio == l.r.id).map(|(i, _)| i + 1).collect();
            let itens: Vec<u32> = f.roteiro.iter().filter(|it| it.processamentos.iter().any(|p| rastreio_do_proc(p) == Some(l.r.id.as_str()))).map(|it| it.n).collect();
            let uso: Vec<String> = [(!ocs.is_empty()).then(|| onde_oc(&ocs)), (!itens.is_empty()).then(|| onde(&itens))].into_iter().flatten().collect();
            RastreioAnexo {
                id: l.id.clone(),
                origem: ou_traco(&origem_e_marcacoes(l.r).join(" · ")),
                uso: ou_traco(&uso.join("; ")),
                corridas,
                confirmacao: confirmacao_do_rastreio(l.r).unwrap_or_else(|| "—".into()),
                amostras: l.r.denso_sha256.clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "—".into()),
            }
        })
        .collect();

    let projeto = arquivos.iter().map(|a| a.rel.replace('\\', "/")).find(|r| nome_do_arquivo(r) == "projeto.json").unwrap_or_else(|| "projeto.json".into());
    let r = RelatorioJuri {
        esquema: ESQUEMA_JURI,
        programa: ["SICRO", e.sicro.versao.trim()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(" "),
        caso: ECaso { numero: e.caso.numero.clone(), referencia: e.caso.referencia.clone(), orgao: e.caso.orgao.clone(), requisitante: e.caso.requisitante.clone() },
        perito: EAutor { nome: e.autor.nome.clone(), cargo: e.autor.cargo.clone(), matricula: e.autor.matricula.clone() },
        gerado_em: data(&t.renderizado_em),
        revisado_por: Some(f.revisado_por.trim().to_string()).filter(|s| !s.is_empty()),
        saida,
        originais: f.midias.iter().enumerate().map(|(i, m)| original(i, m, intervalos.get(&m.id).map(Vec::as_slice))).collect(),
        roteiro,
        acrescimos,
        mudancas,
        ia_resumo,
        relogios,
        medidas,
        sincronizacao,
        limitacoes,
        selos_desligados,
        conferir,
        processamentos,
        diferencas,
        ocultacoes,
        conferencia_ocultacoes,
        codificacao,
        software,
        modelos_ia,
        rastreios: rastreios_anexo,
        reproducao: Reproducao { projeto, projeto_sha256: f.projeto.sha256.clone(), observacao: cond(if por_gpu { condicao::REFAZER_GPU } else { condicao::REFAZER_CPU }) },
        pacote: arquivos.iter().map(|a| ArquivoPacote { arquivo: a.rel.replace('\\', "/"), sha256: a.sha256.clone() }).collect(),
        // os anexos dependem do modo do PDF: `entrada` os põe
        anexos: Vec::new(),
        referencias: e.referencias.clone().unwrap_or_else(|| frases::REFERENCIAS.iter().map(|s| s.to_string()).collect()),
    };
    Ok(serde_json::to_value(r)?)
}

/// Entrada do `relatorio_pdf::gerar`: dados, miniaturas, anexos (`tecnico.json` e `projeto.json`; nenhum em
/// `a2_sem_anexos`), `momento` = `renderizado_em` e `ident` = SHA-256 do vídeo (refazer devolve o mesmo PDF).
/// Os anexos entram em `dados.anexos` com o SHA-256 dos bytes anexados.
pub fn entrada(tecnico: &Value, dados: Value, miniaturas: &[Miniatura], projeto_json: &[u8], pdf: ModoPdf) -> Result<Entrada> {
    let renderizado_em = tecnico.get("renderizado_em").and_then(Value::as_str).unwrap_or("");
    let momento = instante(renderizado_em).ok_or_else(|| SicroError::Validation("relatorio: renderizado_em inválido".into()))?;
    let ident = tecnico.pointer("/execucao/saida/sha256").and_then(Value::as_str).filter(|s| !s.is_empty());
    let ident = ident.ok_or_else(|| SicroError::Validation("relatorio: tecnico.json sem o SHA-256 do vídeo".into()))?.to_string();
    let mut dados = dados;
    let mut arquivos: Vec<(String, Vec<u8>)> = miniaturas.iter().map(|m| (format!("img/{}.jpg", m.n), m.jpeg.clone())).collect();
    let mut anexos = Vec::new();
    if pdf == ModoPdf::A3 {
        for (nome, descricao, bytes) in [("tecnico.json", "anexo.tecnico", tecnico::bytes(tecnico)), ("projeto.json", "anexo.projeto", projeto_json.to_vec())] {
            anexos.push(Anexo { arquivo: nome.into(), descricao: rot(descricao).to_string(), sha256: format!("{:x}", Sha256::digest(&bytes)) });
            arquivos.push((nome.into(), bytes));
        }
    }
    dados["anexos"] = serde_json::to_value(anexos)?;
    Ok(Entrada { modelo: Modelo::Juri, dados, arquivos, momento, ident, pdf_a: true })
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::curva::fixtures;
    use crate::edicao::modelo::{AudioClipe, Ausencia, ClasseDeteccao, Mixagem, MotivoAusencia, OrigemRastreio, SincroniaRef};
    use crate::edicao::motor::plano::{ProvProcessamento, ProvRastreio, ProvTrilha};
    use crate::edicao::rastreio::{edgetam, vittrack, ArquivoModelo, Limiares, ModeloCorrida, Motivo, MotorCorrida, Sentido};
    use crate::edicao::relatorio::apoio::{self, Cena};
    use crate::edicao::relatorio::tecnico::{completar_conferencia, montar, ConferenciaOcultacoes, Fecho};
    use crate::relatorio_pdf;
    use serde_json::json;
    use std::collections::BTreeSet;

    const JPEG: &[u8] = include_bytes!("../../relatorio_pdf/exemplo/img/1.jpg");

    fn fecho() -> Fecho {
        Fecho { projeto_sha256: "9".repeat(64), arquivo_rel: "Exportações/Vídeos/Edições/Caso 2026-11-14 10h00m00/Caso.mp4".into(), ocultacoes: None }
    }

    fn pacote() -> Vec<ArquivoDoPacote> {
        ["Caso.mp4", "Caso.tecnico.json", "mapa.csv", "quadros.compostos.sha256", "quadros.decodificados.sha256", "projeto/projeto.json", "SHA256SUMS.txt"]
            .iter()
            .enumerate()
            .map(|(i, rel)| ArquivoDoPacote { rel: rel.to_string(), sha256: format!("{i:x}").repeat(64) })
            .collect()
    }

    /// Uma corrida como o laço grava (§6.1, B2), com os arquivos reais do modelo; de `de` a `ate` (quadros a 30 qps na
    /// base 1/15360 da cena de apoio), a partir da âncora (para trás, no `ate`).
    fn corrida(motor: MotorRastreio, provedor: &str, sentido: Sentido, de: i64, ate: i64) -> Corrida {
        let (nome, arquivos, origem, commit, desvio, runtime) = match motor {
            MotorRastreio::Edgetam => (
                "EdgeTAM vídeo",
                edgetam::ARQUIVOS.iter().map(|a| ArquivoModelo { nome: a.nome.into(), sha256: a.sha256.into() }).collect(),
                edgetam::ORIGEM,
                edgetam::COMMIT,
                edgetam::DESVIO,
                "onnxruntime 1.31.1",
            ),
            MotorRastreio::Vittrack => (
                "VitTrack",
                vec![ArquivoModelo { nome: vittrack::NOME_ARQUIVO.into(), sha256: vittrack::SHA256.into() }],
                vittrack::ORIGEM,
                vittrack::COMMIT,
                "",
                "onnxruntime 1.31.1",
            ),
            // CONTRATO: variante nova (P4.5); os testes daqui não a usam
            MotorRastreio::Mancha | MotorRastreio::Deteccao => ("", vec![], "", "", "", ""),
        };
        let modelo = ModeloCorrida { nome: nome.into(), origem: origem.into(), commit: commit.into(), arquivos, desvio: desvio.into() };
        Corrida {
            motor: MotorCorrida { motor, modelo, runtime: runtime.into(), provedor: provedor.into() },
            sentido,
            ancora_pts: 512 * if sentido == Sentido::Tras { ate } else { de },
            de_pts: 512 * de,
            ate_pts: 512 * ate,
            limiares: Limiares { ok: 0.95, perdido: 0.5, paciencia: 90 },
            parou: Motivo::Fim,
            feito_em: "2026-11-14T08:30:00Z".into(),
        }
    }

    fn valor<T: serde::Serialize>(x: &T) -> Value {
        serde_json::to_value(x).unwrap()
    }

    /// Rastreio usado no plano de apoio (`feito.rastreios[]`), sem nome nem confirmação.
    fn prov_rastreio(id: &str, origem: OrigemRastreio, ancoras: u32, corridas: Vec<Value>) -> ProvRastreio {
        ProvRastreio { id: id.into(), origem, ancoras, denso_sha256: Some(format!("{:x}", Sha256::digest(id.as_bytes()))), silhuetas_sha256: None, corridas, corridas_do_programa: vec![], confirmacao: None, ausencias: vec![] }
    }

    /// O `tecnico.json` do plano de apoio, com o que as seções do `/2` pedem: detecção confirmada e seguida pelo
    /// EdgeTAM, alvo seguido pelo VitTrack e pela correlação, ocultação invertida, média de quadros, relógio, régua,
    /// velocidade, narração, sincronia e fecho.
    fn tecnico_completo(c: &Cena) -> Value {
        let (mut plano, res) = apoio::plano_e_resultado(c);
        let pv = &mut plano.proveniencia;
        pv.rastreios[0].origem = OrigemRastreio::Deteccao { classe: ClasseDeteccao::Rosto };
        // a corrida antiga da cena (sem a forma da `Corrida`) fica: o anexo só diz que ela existe
        let edgetam = corrida(MotorRastreio::Edgetam, "cuda", Sentido::Frente, 30, 60);
        pv.rastreios[0].corridas.push(valor(&edgetam));
        // entre as marcações do 90 e do 100, de frente uma para a outra: no quadro 100 (o usado) a linha é das duas
        let (vit, mancha) = (corrida(MotorRastreio::Vittrack, "cpu", Sentido::Frente, 90, 100), corrida(MotorRastreio::Mancha, "cpu", Sentido::Tras, 90, 100));
        pv.rastreios.push(prov_rastreio("r2", OrigemRastreio::Assistido, 3, vec![valor(&vit), valor(&mancha)]));
        // `feito.ia` como o P2.1 agrupa (modelo, depois provedor)
        pv.ia = vec![crate::ia::uso::da_corrida(&edgetam, "rastreio"), crate::ia::uso::da_corrida(&vit, "rastreio")];
        pv.ocultacoes[0].inverter = true;
        pv.ocultacoes[0].margem = 0.05;
        // uma ausência dentro do intervalo da ocultação (1 a 2 s) e outra fora dele
        let ausencia = |de_s: f64, ate_s: f64, motivo: MotivoAusencia| Ausencia { de_s, ate_s, motivo, aceita_em: "2026-11-14T09:30:00Z".into(), por: "Perito Teste".into() };
        pv.rastreios[0].ausencias = vec![ausencia(1.5, 1.75, MotivoAusencia::ForaDeCena), ausencia(5.0, 6.0, MotivoAusencia::Oculto)];
        let proc = |ordem: u32, nome: &str, parametros: Value| ProvProcessamento { ordem, nome: nome.into(), parametros };
        pv.roteiro[1].processamentos = vec![
            proc(1, "ocultar", json!({ "tipo": "ocultar", "id": "e1", "conteudo": "rosto", "motivo": "terceiro /sem relação", "alvo": { "rastreio": "r1" } })),
            proc(2, "media_temporal", json!({ "tipo": "media_temporal", "id": "e2", "quadros": 8 })),
            proc(3, "destaque", json!({ "tipo": "destaque", "id": "e3", "rastreio": "r1", "forma": "elipse", "margem": 0.05, "traco": { "cor": "#ed333b", "espessura": 0.004, "contorno": true },
                "rotulo": { "texto": "P1", "lado": "cima", "estilo": { "tamanho": 0.03, "cor": "#ffffff", "negrito": true } } })),
            proc(4, "carimbo", json!({ "tipo": "carimbo", "id": "e4", "mostra": "tempo_fonte" })),
            proc(5, "velocidade_exibida", json!({ "tipo": "velocidade_exibida", "id": "e5", "calculo": { "kmh": 42.0, "faixa": [39.0, 45.5] } })),
            proc(6, "destaque", json!({ "tipo": "destaque", "id": "e8", "rastreio": "r1", "forma": "seta", "tamanho": 0.02, "angulo_graus": 90.0, "traco": { "cor": "#ffd84a", "espessura": 0.002, "contorno": true } })),
        ];
        pv.roteiro[2].processamentos = vec![
            proc(1, "regua", json!({ "tipo": "regua", "id": "e6", "comprimento_m": 2.0, "calibracao": { "residuo_m": 0.02 } })),
            proc(2, "holofote", json!({ "tipo": "holofote", "id": "e7", "alvo": { "rastreio": "r2" } })),
        ];
        pv.roteiro[1].selos = vec!["Média de 8 imagens".into()];
        pv.roteiro[1].audio = Some(serde_json::from_value::<AudioClipe>(json!({ "volume": 1.0, "mudo": true, "manter_tom": true, "silencios": [] })).unwrap());
        pv.audio.mixagem = Some(Mixagem::default());
        pv.audio.trilhas = vec![serde_json::from_value::<ProvTrilha>(json!({ "id": "ta", "papel": "narracao", "volume": 1.0, "mudo": false, "abaixa_original": false,
            "clipes": [{ "id": "a1", "midia": "m1", "de_q": 40, "n_q": 60, "de_s": 0.0, "ate_s": null, "volume": 1.0, "entrada_s": 0.0, "saida_s": 0.0 }] })).unwrap()];
        pv.sincronias = vec![serde_json::from_value::<SincroniaRef>(json!({ "id": "s1", "vinculo_id": null, "referencia": "m1", "outra": "m1", "desloc_s": -2.24,
            "deriva_ppm": 500.0, "deriva_incerteza_ppm": 20.0, "t0_s": 0.0, "valido_de_s": 0.0, "valido_ate_s": 60.0, "metodo": "audio", "incerteza_s": null,
            "confianca": null, "conferencias": [] })).unwrap()];
        pv.fecho = Some(serde_json::from_value(json!({ "tipo": "escurecer", "duracao_s": 0.5, "audio": "acompanhar" })).unwrap());
        montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap()
    }

    fn miniaturas() -> Vec<Miniatura> {
        vec![Miniatura { n: 2, jpeg: JPEG.to_vec() }]
    }

    /// O `projeto.json` com a tela de texto do item 1 (o `feito` passa a registrar o SHA-256 dele) e um `mapa.csv` dos
    /// quadros 30..90 da gravação do item 2, sem o 60 (o par que falta não conta como intervalo).
    fn complementos(t: &mut Value) -> (Vec<u8>, String) {
        let projeto = serde_json::to_vec(&json!({ "principal": [{ "id": "c1", "tipo": "titulo", "titulo": { "modelo": "cartela", "blocos": [
            { "papel": "rodape", "conteudo": "Perícia 0000/2026" },
            { "papel": "corpo", "conteudo": "  " },
            { "papel": "titulo", "conteudo": "Chegada do veículo\nà portaria" },
            { "papel": "subtitulo", "conteudo": "Câmera 1" },
        ] } }] }))
        .unwrap();
        t["feito"]["projeto"]["sha256"] = json!(format!("{:x}", Sha256::digest(&projeto)));
        let mut mapa = String::from("q,item,clipe,midia,pts,indice,t_s,tipo\n0,1,c1,,,,,\n");
        for (q, j) in (30..90u64).filter(|j| *j != 60).enumerate() {
            mapa.push_str(&format!("{q},2,c2,m1,{},{j},{},novo\n", 512 * j, j as f64 / 30.0));
        }
        (projeto, mapa)
    }

    fn pdf_de(tecnico: &Value, modo: ModoPdf) -> (relatorio_pdf::Entrada, String) {
        let d = dados(tecnico, &miniaturas(), &pacote()).unwrap();
        let e = entrada(tecnico, d, &miniaturas(), b"{\"esquema\":2}\n", modo).unwrap();
        let texto = relatorio_pdf::texto(&e).unwrap();
        // depuração e conferência por fora (qpdf, pdfinfo, veraPDF): SICRO_GUARDAR_PDF=<pasta> grava o PDF
        if let Ok(d) = std::env::var("SICRO_GUARDAR_PDF") {
            let nome = if modo == ModoPdf::A3 { "relatorio-a3.pdf" } else { "relatorio-a2-sem-anexos.pdf" };
            let _ = std::fs::write(std::path::Path::new(&d).join(nome), relatorio_pdf::gerar(&e).unwrap().pdf);
        }
        (e, texto)
    }

    /// Chaves de cada objeto, com as das listas unidas; `cheias` = listas com algum elemento (lista sempre vazia não
    /// tem forma: não dá para comparar os campos dela).
    #[derive(Default)]
    struct Forma {
        chaves: BTreeSet<String>,
        listas: BTreeSet<String>,
        cheias: BTreeSet<String>,
    }

    impl Forma {
        fn de(v: &Value) -> Self {
            let mut f = Forma::default();
            f.ler(v, "");
            f
        }
        fn ler(&mut self, v: &Value, caminho: &str) {
            match v {
                Value::Object(o) => o.iter().for_each(|(k, x)| {
                    let c = format!("{caminho}/{k}");
                    self.chaves.insert(c.clone());
                    self.ler(x, &c);
                }),
                Value::Array(a) => {
                    self.listas.insert(caminho.to_string());
                    if !a.is_empty() {
                        self.cheias.insert(caminho.to_string());
                    }
                    a.iter().for_each(|x| self.ler(x, &format!("{caminho}[]")));
                }
                _ => {}
            }
        }
        fn vazias(&self) -> Vec<&String> {
            self.listas.difference(&self.cheias).collect()
        }
    }

    /// Contrato `/2`: o que `dados` produz tem exatamente os campos do exemplo que o modelo imprime (o dos 7 testes do
    /// PDF), o modelo recusa dados de outro contrato, e o resultado está fixado em `juri-dados.json`.
    #[test]
    fn contrato_2() {
        assert_eq!(ESQUEMA_JURI, "sicro.relatorio_juri/2");
        let c = Cena::nova("juri");
        let mut t = tecnico_completo(&c);
        let (projeto, mapa) = complementos(&mut t);
        let d = dados_com(&t, &miniaturas(), &pacote(), Complementos { projeto: Some(&projeto), mapa: Some(&mapa) }).unwrap();
        assert_eq!(d["esquema"], json!(ESQUEMA_JURI));
        // mesmos campos do exemplo, em todos os níveis
        let mut exemplo: Value = serde_json::from_slice(include_bytes!("../../relatorio_pdf/exemplo/dados.json")).unwrap();
        exemplo.as_object_mut().unwrap().remove("exemplo");
        let pronto = entrada(&t, d.clone(), &miniaturas(), b"{}", ModoPdf::A3).unwrap().dados;
        let (fa, fb) = (Forma::de(&pronto), Forma::de(&exemplo));
        assert_eq!(fa.vazias(), Vec::<&String>::new(), "lista vazia nos dados de teste: sem como conferir os campos dela");
        assert_eq!(fb.vazias(), vec!["/selos_desligados"], "listas vazias do exemplo");
        assert_eq!(fa.listas, fb.listas, "as mesmas listas");
        let so_nos_dados: Vec<_> = fa.chaves.difference(&fb.chaves).collect();
        let so_no_exemplo: Vec<_> = fb.chaves.difference(&fa.chaves).collect();
        assert!(so_nos_dados.is_empty() && so_no_exemplo.is_empty(), "só nos dados: {so_nos_dados:?}; só no exemplo: {so_no_exemplo:?}");
        // todo valor é texto pronto (número só em `roteiro[].n`; booleano só nos três sinais do modelo)
        fn folhas(v: &Value, caminho: &str, saida: &mut Vec<String>) {
            match v {
                Value::Object(o) => o.iter().for_each(|(k, x)| folhas(x, &format!("{caminho}/{k}"), saida)),
                Value::Array(a) => a.iter().for_each(|x| folhas(x, caminho, saida)),
                Value::String(_) | Value::Null => {}
                _ => saida.push(caminho.to_string()),
            }
        }
        let mut outras = Vec::new();
        folhas(&d, "", &mut outras);
        outras.sort();
        outras.dedup();
        assert_eq!(outras, ["/originais/formato_nativo", "/originais/taxa_variavel", "/roteiro/n", "/saida/com_perdas"]);
        // o modelo recusa o contrato anterior com erro que diz o porquê
        let mut velho = entrada(&t, d.clone(), &miniaturas(), b"{}", ModoPdf::A3).unwrap();
        velho.dados["esquema"] = json!("sicro.relatorio_juri/1");
        let erro = relatorio_pdf::gerar(&velho).unwrap_err().to_string();
        assert!(erro.contains("dados de outro contrato: sicro.relatorio_juri/1"), "{erro}");
        // `tecnico.json` de outro esquema não vira relatório
        let mut outro = t.clone();
        outro["esquema"] = json!("sicro.edicao.tecnico/1");
        assert!(dados(&outro, &[], &[]).is_err());
        // fixado: mudança de texto ou de campo aparece na revisão
        let mut minimo = t.clone();
        for k in ["midias", "roteiro", "ocultacoes", "rastreios", "ia", "avisos", "selos_desligados", "sincronias"] {
            minimo["feito"][k] = json!([]);
        }
        minimo["feito"]["audio"] = json!({ "silencios": [], "tolerancia_atempo_ms": 0.0 });
        minimo["execucao"]["conferencia"]["ocultacoes"] = Value::Null;
        // o mínimo (só telas de texto apagadas: nenhuma gravação, nenhum item) também vira PDF
        let d_min = dados(&minimo, &[], &[]).unwrap();
        let s_min = relatorio_pdf::gerar(&entrada(&minimo, d_min.clone(), &[], b"{}", ModoPdf::A2SemAnexos).unwrap()).unwrap();
        assert!(s_min.paginas >= 2 && s_min.avisos.is_empty(), "{:?}", s_min.avisos);
        fixtures::conferir(
            "juri-dados.json",
            json!([
                { "nome": "completo", "entrada": "relatorio::juri::testes::tecnico_completo", "saida": d },
                { "nome": "minimo", "entrada": "o completo sem mídias, roteiro, ocultações, rastreios, IA, avisos e sincronias", "saida": d_min },
            ]),
        );
    }

    /// Aceite do §11: o PDF traz a cobertura por gravação, o resumo de IA, os selos desligados e os comandos de
    /// conferência (texto lido do documento paginado, não dos dados).
    #[test]
    fn cobertura_ia_selos_e_conferencia() {
        let c = Cena::nova("juri");
        let t = tecnico_completo(&c);
        let (e, texto) = pdf_de(&t, ModoPdf::A3);
        let tem = |s: &str| assert!(texto.contains(s), "falta no PDF: {s}\n{texto}");
        let nao_tem = |s: &str| assert!(!texto.contains(s), "sobra no PDF: {s}");
        // cobertura por gravação, recebimento, derivação e relógio (seção 3)
        tem("usados 2 s de 10 s (0:01,000 a 0:03,000)");
        tem("já convertida, e não no formato próprio do aparelho gravador (veio por pendrive)");
        tem("desvio informado pelo perito: −2,5 s · DVR 1");
        tem("Este arquivo foi feito pela perícia a partir de outra gravação do caso (cópia convertida do formato do gravador, sem recompressão).");
        tem(&"b".repeat(64));
        // resumo de IA (seção 5)
        tem("Ocultações propostas por detecção automática e confirmadas pelo perito: 1.");
        tem("Marcações que acompanham alvo achado por detecção automática e confirmado pelo perito: 1.");
        tem("Os programas de inteligência artificial usados estão identificados no anexo técnico, item H.");
        nao_tem("Nenhuma etapa deste vídeo foi feita por programa de inteligência artificial.");
        // frase fixa da média de quadros, com a contagem no rótulo
        tem("Média de imagens (8)");
        tem("Cada imagem mostrada é a média de várias imagens seguidas da gravação.");
        tem("Aviso escrito na imagem: Média de 8 imagens.");
        // tempo e medidas (seção 6): relógio, régua, velocidade com a faixa, incerteza não determinada
        tem("O relógio marcado \"Original\" conta o tempo desde o início da gravação original.");
        tem("Régua de escala de 2,00 m (item 3): erro médio da calibração de 0,02 m.");
        tem("Velocidade calculada (item 2): 42 km/h, na faixa de 39 a 45,5 km/h.");
        tem("Margem de erro não determinada");
        tem("sem conferência por acontecimento visível");
        // limites (seção 7): com perdas, avisos (com a ciência), recebida convertida, conferência pendente, selos
        tem("O vídeo entregue foi gravado em formato com compressão.");
        tem("Movimentos vistos nela podem parecer mais rápidos ou mais lentos do que foram. O perito registrou ciência deste aviso antes de exportar.");
        tem("Há gravação com defeito de leitura.");
        nao_tem("Há gravação com defeito de leitura. Partes da imagem podem ter sido reconstruídas pelo programa de leitura e não devem servir a exame de detalhe. O perito registrou");
        tem("A conferência das ocultações no arquivo final ainda não foi registrada.");
        tem("Avisos na imagem desligados pelo perito: Selo de realce.");
        // comandos de conferência (seção 8)
        tem("certutil -hashfile \"Caso.mp4\" SHA256");
        tem("sha256sum \"Caso.mp4\"");
        tem("ffmpeg -i \"Caso.mp4\" -map 0:v:0 -f framehash -hash sha256 -");
        // (o itálico separa o nome do arquivo da pontuação no texto extraído)
        tem("A lista do pacote está no arquivo quadros.decodificados.sha256");
        tem("O projeto da edição ( projeto/projeto.json ) acompanha o pacote.");
        // anexos: o PDF é a raiz (SHA-256 de cada arquivo do pacote), ocultação invertida, som, IA
        for a in pacote() {
            tem(&a.rel);
            tem(&a.sha256);
        }
        tem("retângulo, invertida (cobre tudo o que está fora da forma), margem 5 %");
        tem("sólido opaco, cor #000000ff");
        // a ausência aceita dentro do intervalo da ocultação sai com o motivo; a de fora dele não
        tem("quadros — na marcação do perito: 2 · medidos: 0 · interpolados: 0; ausências do alvo aceitas pelo perito: 0:01,500 a 0:01,750 (alvo fora de cena)");
        nao_tem("alvo encoberto");
        tem("tom no lugar do som");
        tem("terceiro /sem relação");
        tem("vision_encoder.onnx");
        tem(edgetam::ARQUIVOS[0].sha256);
        tem("Fecho escurecer, 0,500 s");
        tem("SWGDE Best Practices for Digital Forensic Video Analysis, 18-V-001, versão 2.0 (10/12/2025)");
        let baixo = texto.to_lowercase();
        assert!(!baixo.contains("conforme") && !baixo.contains("certificad"));
        nao_tem("Revisado por");
        // nenhuma pasta da máquina nem o nome do rastreio
        assert!(!texto.contains(&*c.caso.to_string_lossy()) && !texto.contains(apoio::NOME_RASTREIO));
        // sai arquivável, com os dois anexos e o SHA-256 dos bytes anexados
        let s = relatorio_pdf::gerar(&e).unwrap();
        assert!(s.avisos.is_empty(), "{:?}", s.avisos);
        assert!(relatorio_pdf::testes::contem(&s.pdf, b"<pdfaid:part>3</pdfaid:part>") && relatorio_pdf::testes::contem(&s.pdf, b"/AFRelationship"));
        let anexos = e.dados["anexos"].as_array().unwrap();
        assert_eq!(anexos.iter().map(|a| a["arquivo"].as_str().unwrap()).collect::<Vec<_>>(), ["tecnico.json", "projeto.json"]);
        assert_eq!(anexos[0]["sha256"], json!(format!("{:x}", Sha256::digest(tecnico::bytes(&t)))));
        assert_eq!(anexos[1]["sha256"], json!(format!("{:x}", Sha256::digest(b"{\"esquema\":2}\n"))));
        assert!(e.arquivos.iter().any(|(n, b)| n == "tecnico.json" && *b == tecnico::bytes(&t)));
        assert!(e.arquivos.iter().any(|(n, b)| n == "img/2.jpg" && b == JPEG));
        assert_eq!((e.ident.as_str(), e.momento.to_rfc3339().as_str(), e.pdf_a), ("1111111111111111111111111111111111111111111111111111111111111111", "2026-11-14T10:00:04+00:00", true));
        tem("Emitido em 14 de novembro de 2026, às 7h00 (horário de Brasília)");
    }

    /// A ocultação da MÍDIA (o padrão da tela, D17) não está nos processamentos do clipe: entra nas mudanças e no
    /// item do roteiro. Mosaico e desfoque não levam a frase do sólido ("não é possível desfazer").
    #[test]
    fn ocultacao_da_midia_entra_nas_mudancas_e_whatsapp_nos_limites() {
        let c = Cena::nova("juri-oc");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        for it in &mut plano.proveniencia.roteiro {
            it.processamentos.clear();
        }
        let linhas = |plano: &crate::edicao::motor::plano::Plano| {
            let t = montar(plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
            let d = dados(&t, &[], &[]).unwrap();
            let m: Vec<(String, String, String)> = d["mudancas"].as_array().unwrap().iter().map(|l| (l["o_que"].as_str().unwrap().into(), l["onde"].as_str().unwrap().into(), l["explicacao"].as_str().unwrap().into())).collect();
            (m, d)
        };
        let (m, d) = linhas(&plano);
        let oc: Vec<_> = m.iter().filter(|l| l.0 == "Ocultação (rosto)").collect();
        assert_eq!(oc.len(), 1, "{m:?}");
        assert_eq!(oc[0].1, "item 2");
        assert!(oc[0].2.contains("não é possível desfazer"));
        let item = d["roteiro"].as_array().unwrap().iter().find(|it| it["n"] == 2).unwrap();
        assert!(item["efeitos"].as_array().unwrap().contains(&json!({ "texto": "Ocultação (rosto)", "marcas": [] })), "{item}");
        assert!(!d["limitacoes"].as_array().unwrap().iter().any(|l| l.as_str().unwrap().contains("como documento")));
        // mosaico: outra frase; WhatsApp: o código só confere no envio como documento
        plano.proveniencia.ocultacoes[0].modo = crate::edicao::modelo::ModoOcultar::Mosaico;
        plano.proveniencia.saida.predefinicao = Some(crate::edicao::modelo::Predefinicao::Whatsapp);
        let (m, d) = linhas(&plano);
        let oc: Vec<_> = m.iter().filter(|l| l.0 == "Ocultação (rosto)").collect();
        assert_eq!(oc.len(), 1, "{m:?}");
        assert!(oc[0].2.contains("não garante") && !oc[0].2.contains("não é possível desfazer"), "{}", oc[0].2);
        assert!(d["limitacoes"].as_array().unwrap().iter().any(|l| l.as_str().unwrap().contains("só confere se ele for enviado como documento")), "{}", d["limitacoes"]);
    }

    /// O outro lado de cada condição: sem IA, selos ligados, conferência feita ou não feita, revisão preenchida.
    #[test]
    fn sem_ia_selos_ligados_conferida_e_revisado() {
        let c = Cena::nova("juri");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        plano.proveniencia.ia.clear();
        // a corrida da cena não se lê: com ela não se pode dizer "nenhuma etapa por IA"
        plano.proveniencia.rastreios[0].corridas.clear();
        plano.proveniencia.selos_desligados.clear();
        plano.proveniencia.revisado_por = " Beltrana de Tal ".into();
        plano.proveniencia.avisos.clear();
        plano.proveniencia.saida.com_perdas = false;
        let mut t = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        completar_conferencia(&mut t, &ConferenciaOcultacoes { conferida: true, por: "Perito Teste".into(), em: "2026-11-14T11:00:00Z".into() }).unwrap();
        let (_, texto) = pdf_de(&t, ModoPdf::A3);
        assert!(texto.contains("Nenhuma etapa deste vídeo foi feita por programa de inteligência artificial."), "{texto}");
        assert!(!texto.contains("desligados pelo perito") && !texto.contains("O perito desligou o aviso"));
        assert!(texto.contains("Revisado por Beltrana de Tal"));
        assert!(texto.contains("As ocultações foram conferidas pelo perito no arquivo final. Perito Teste, em 14 de novembro de 2026, às 8h00 (horário de Brasília)."));
        assert!(!texto.contains("formato com compressão") && !texto.contains("ainda não foi registrada"));
        assert!(texto.contains("O vídeo não mostra relógio na imagem."));
        // concluída sem conferir: dito nos limites e no anexo E
        let mut t = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        completar_conferencia(&mut t, &ConferenciaOcultacoes { conferida: false, por: "Perito Teste".into(), em: "2026-11-14T11:00:00Z".into() }).unwrap();
        let d = dados(&t, &[], &[]).unwrap();
        assert!(d["limitacoes"].as_array().unwrap().contains(&json!("As ocultações não foram conferidas pelo perito no arquivo final.")));
        assert_eq!(d["conferencia_ocultacoes"], json!("As ocultações não foram conferidas pelo perito no arquivo final. Perito Teste, em 14 de novembro de 2026, às 8h00 (horário de Brasília)."));
        // rastreio sem ausência aceita: só a contagem
        assert_eq!(d["ocultacoes"][0]["quadros"], json!("quadros — na marcação do perito: 2 · medidos: 0 · interpolados: 0"));
        // sem a lista de quadros no pacote, o PDF não cita o arquivo; sem projeto no pacote, o nome simples
        assert_eq!((&d["conferir"]["lista"], &d["reproducao"]["projeto"]), (&Value::Null, &json!("projeto.json")));
        // codificador de GPU: outra frase de repetição
        t["execucao"]["saida"]["codificador"] = json!("h264_nvenc");
        let d = dados(&t, &[], &[]).unwrap();
        assert!(d["reproducao"]["observacao"].as_str().unwrap().starts_with("O vídeo foi comprimido pela placa de vídeo."));
        assert!(d["codificacao"].as_array().unwrap().iter().any(|l| l[0] == "Repetição bit a bit do arquivo" && l[1].as_str().unwrap().starts_with("não esperada")));
    }

    /// `ProvItem.audio` e os silêncios saem no `feito` mesmo com `saida.audio` falso (contratos-fase1 (g)). O PDF só
    /// fala de som quando o vídeo tem som: sem ele, diz "sem som" e não lista volume, mudo, narração nem trecho silenciado.
    #[test]
    fn som_do_clipe_so_aparece_com_som_na_saida() {
        let c = Cena::nova("juri");
        let com = tecnico_completo(&c);
        let d = dados(&com, &[], &[]).unwrap();
        let o_que = |d: &Value, lista: &str| d[lista].as_array().unwrap().iter().map(|l| l["o_que"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert!(d["saida"]["formato"].as_str().unwrap().ends_with("com som"), "{}", d["saida"]["formato"]);
        assert!(o_que(&d, "mudancas").contains(&"Som da gravação retirado".to_string()) && o_que(&d, "mudancas").contains(&"Som ocultado".to_string()));
        assert!(o_que(&d, "acrescimos").contains(&"Narração".to_string()));
        assert_eq!(d["acrescimos"].as_array().unwrap().iter().find(|l| l["o_que"] == "Narração").unwrap()["onde"], json!("itens 2, 3"));
        assert!(d["ocultacoes"].as_array().unwrap().iter().any(|o| o["conteudo"] == "som"));
        assert!(d["roteiro"][1]["efeitos"].as_array().unwrap().contains(&json!({ "texto": "Som da gravação retirado", "marcas": [] })));
        // a mesma proveniência, sem som na saída: o `feito` continua trazendo o som do clipe e os silêncios
        let mut sem = com.clone();
        sem["feito"]["audio"]["mixagem"] = Value::Null;
        sem["execucao"]["conteudo"]["audio_sha256"] = Value::Null;
        assert!(sem["feito"]["roteiro"][1]["audio"]["mudo"] == json!(true) && !sem["feito"]["audio"]["silencios"].as_array().unwrap().is_empty());
        let d = dados(&sem, &[], &[]).unwrap();
        assert!(d["saida"]["formato"].as_str().unwrap().ends_with("sem som"), "{}", d["saida"]["formato"]);
        let tudo = d.to_string();
        for s in ["Som da gravação retirado", "Som ocultado", "Narração", "Volume alterado", "tom no lugar do som"] {
            assert!(!tudo.contains(s), "{s} num vídeo sem som");
        }
        assert!(!d["ocultacoes"].as_array().unwrap().iter().any(|o| o["conteudo"] == "som"));
        assert_eq!(d["ocultacoes"].as_array().unwrap().len(), 1, "a ocultação de imagem fica");
    }

    /// Rampa (P3.4): o item diz os pontos (fator e instante do original), a parte lenta e a acelerada entram nas
    /// mudanças, e o anexo traz os pontos em segundos, a faixa da rampa e o mapa sem fórmula.
    #[test]
    fn rampa_listada_no_item_nas_mudancas_e_no_anexo() {
        let c = Cena::nova("juri");
        let mut t = tecnico_completo(&c);
        let rampa = json!({ "tipo": "rampa", "pontos": [{ "t": 1.5, "v": 1.0 }, { "t": 2.0, "v": 0.25 }, { "t": 2.5, "v": 1.0 }, { "t": 2.9, "v": 2.0, "interp": "degrau" }] });
        t["feito"]["roteiro"][1]["velocidade"] = rampa;
        t["feito"]["roteiro"][1]["origem"]["mapa"] = json!({ "c": 15360, "kn": 0, "kd": 0, "e": 0 });
        let d = dados(&t, &[], &[]).unwrap();
        assert_eq!(
            d["roteiro"][1]["velocidade"],
            json!("variável ao longo do trecho: 1× em 0:01,500, 0,25× em 0:02,000, 1× em 0:02,500 e 2× em 0:02,900")
        );
        let o_que: Vec<String> = d["mudancas"].as_array().unwrap().iter().map(|l| l["o_que"].as_str().unwrap().to_string()).collect();
        let (lenta, acelerada) = (frases::rotulo_do_efeito(tipo::CAMERA_LENTA).unwrap(), frases::rotulo_do_efeito(tipo::ACELERADO).unwrap());
        assert!(o_que.iter().any(|x| x == lenta) && o_que.iter().any(|x| x == acelerada), "{o_que:?}");
        let corte = d["processamentos"].as_array().unwrap().iter().find(|p| p["item"] == "2" && p["ordem"] == "0").unwrap()["parametros"].as_str().unwrap().to_string();
        assert!(corte.contains("velocidade em rampa: 1× em 1,500000 s, 0,25× em 2,000000 s, 1× em 2,500000 s e 2× em 2,900000 s"), "{corte}");
        assert!(corte.ends_with("mapa c=15360 slot de cada quadro em tabela") && !corte.contains("kd="), "{corte}");
        let dif = d["diferencas"].as_array().unwrap().iter().find(|p| p["item"] == "2").unwrap()["taxa"].as_str().unwrap().to_string();
        assert!(dif.ends_with(", em rampa de 0,25× a 2×"), "{dif}");
        // no PDF
        let (_, texto) = pdf_de(&t, ModoPdf::A3);
        let plano = texto.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(plano.contains("0,25× em 0:02,000"), "o PDF não lista a rampa");
        // só desacelera: não acusa aceleração
        t["feito"]["roteiro"][1]["velocidade"]["pontos"][3]["v"] = json!(0.5);
        let d = dados(&t, &[], &[]).unwrap();
        assert!(!d["mudancas"].as_array().unwrap().iter().any(|l| l["o_que"] == acelerada));
    }

    /// O enquadramento só é "Ampliação" (seção 5 e item do roteiro) quando mostra parte da imagem; com a imagem inteira,
    /// "Preencher" diz que as bordas saem e "Inteiro" não declara mudança (o anexo C traz os três, com os parâmetros).
    #[test]
    fn enquadramento_so_e_ampliacao_quando_mostra_parte() {
        let c = Cena::nova("juri");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        // só o enquadramento no item 2 (a ocultação da mídia da cena também entra na lista do item)
        plano.proveniencia.ocultacoes.clear();
        let q = |regiao: Value, ajuste: &str| vec![ProvProcessamento { ordem: 1, nome: "enquadramento".into(), parametros: json!({ "tipo": "enquadramento", "id": "q", "regiao": regiao, "ajuste": ajuste, "interpolacao": "bilinear" }) }];
        let inteira = json!({ "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 });
        let parte = json!({ "x": 0.25, "y": 0.25, "w": 0.5, "h": 0.5 });
        let animada = json!([{ "t": 0.0, "v": inteira, "interp": "linear" }, { "t": 1.0, "v": parte, "interp": "linear" }]);
        let casos: [(Value, &str, Option<&str>, &str); 5] = [
            (inteira.clone(), "conter", None, "Ajuste de tamanho"),
            (inteira.clone(), "cobrir", Some("Imagem ajustada para preencher a tela"), "Imagem ajustada para preencher a tela"),
            (parte.clone(), "conter", Some("Ampliação"), "Ampliação"),
            (animada, "conter", Some("Ampliação"), "Ampliação"),
            // forma que não se reconhece: declara
            (json!("?"), "conter", Some("Ampliação"), "Ampliação"),
        ];
        for (regiao, ajuste, na_secao_5, no_anexo_c) in casos {
            plano.proveniencia.roteiro.iter_mut().for_each(|it| it.processamentos.clear());
            plano.proveniencia.roteiro[1].processamentos = q(regiao.clone(), ajuste);
            let t = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
            let d = dados(&t, &[], &[]).unwrap();
            let linhas: Vec<&str> = d["mudancas"].as_array().unwrap().iter().map(|l| l["o_que"].as_str().unwrap()).filter(|o| ["Ampliação", "Imagem ajustada para preencher a tela", "Ajuste de tamanho"].contains(o)).collect();
            assert_eq!(linhas, na_secao_5.into_iter().collect::<Vec<_>>(), "{regiao} {ajuste}");
            let do_item: Vec<&str> = d["roteiro"][1]["efeitos"].as_array().unwrap().iter().map(|e| e["texto"].as_str().unwrap()).collect();
            assert_eq!(do_item, na_secao_5.into_iter().collect::<Vec<_>>(), "{regiao} {ajuste}");
            let anexo: Vec<&str> = d["processamentos"].as_array().unwrap().iter().filter(|p| p["item"] == "2" && p["ordem"] == "1").map(|p| p["nome"].as_str().unwrap()).collect();
            assert_eq!(anexo, [no_anexo_c], "{regiao} {ajuste}");
        }
    }

    /// Ocultação sobre trilha da análise de Pessoas já revisada (sem `confirmacao`, que só a automática leva): a
    /// região veio de programa e o resumo de IA diz isso, nunca "nenhuma etapa".
    #[test]
    fn pessoas_revisada_conta_como_ia() {
        let c = Cena::nova("juri");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        plano.proveniencia.ia.clear();
        let nenhuma = frases::da_condicao(condicao::IA_NENHUMA).unwrap();
        let resumo = |plano: &crate::edicao::motor::plano::Plano| {
            let t = montar(plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
            dados(&t, &[], &[]).unwrap()["ia_resumo"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect::<Vec<_>>()
        };
        plano.proveniencia.rastreios[0].origem = OrigemRastreio::Manual;
        plano.proveniencia.rastreios[0].confirmacao = None;
        plano.proveniencia.rastreios[0].corridas.clear();
        assert_eq!(resumo(&plano), [nenhuma]);
        plano.proveniencia.rastreios[0].origem = serde_json::from_value(json!({ "tipo": "pessoas", "pessoa_id": "p1", "estado": "revisada", "copiado_em": "2026-11-14T09:00:00Z" })).unwrap();
        assert_eq!(resumo(&plano), ["Ocultações propostas por detecção automática e confirmadas pelo perito: 1.", "Nessas ocultações, imagens com a posição achada pelo programa: 0; calculada a partir das posições vizinhas: 0; marcada pelo perito: 2."]);
    }

    /// Legendas: o relatório só fala delas quando o `feito` diz que saíram (`legendas.modo`, que a proveniência ainda
    /// não traz); aí entram nos acréscimos e, vindas de degravação automática, no resumo de IA.
    #[test]
    fn legendas_so_com_o_modo_no_feito() {
        let c = Cena::nova("juri");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        plano.proveniencia.ia.clear();
        plano.proveniencia.rastreios.iter_mut().for_each(|r| (r.origem, r.corridas) = (OrigemRastreio::Manual, vec![]));
        let base = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        let legendas = |d: &Value| d["acrescimos"].as_array().unwrap().iter().filter(|l| l["o_que"].as_str().unwrap().starts_with("Legendas")).map(|l| format!("{}: {}", l["o_que"].as_str().unwrap(), l["onde"].as_str().unwrap())).collect::<Vec<_>>();
        let ia = |d: &Value| d["ia_resumo"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect::<Vec<_>>();
        let com = |n: u32, modo: Value, motor: Value| {
            let mut t = base.clone();
            t["feito"]["legendas"] = json!({ "origem": { "audio_sha256": "a", "importado_em": "", "motor": motor, "modelo_sha256": null }, "n": n, "conferidas": n, "modo": modo });
            dados(&t, &[], &[]).unwrap()
        };
        let nenhuma_ia = vec![frases::da_condicao(condicao::IA_NENHUMA).unwrap().to_string()];
        // sem o modo (o `feito` de hoje), com "nenhuma" ou sem itens: nada
        for d in [dados(&base, &[], &[]).unwrap(), com(3, Value::Null, json!("whisper")), com(3, json!("nenhuma"), json!("whisper")), com(0, json!("queimar"), json!("whisper"))] {
            assert_eq!((legendas(&d), ia(&d)), (vec![], nenhuma_ia.clone()));
        }
        // o modo que saiu vai escrito, com a contagem do que saiu
        for (modo, onde) in [("queimar", "na imagem, ao longo do vídeo"), ("arquivo", "em arquivo que acompanha o vídeo"), ("ambos", "na imagem e em arquivo que acompanha o vídeo")] {
            let d = com(3, json!(modo), json!("whisper"));
            assert_eq!((legendas(&d), ia(&d)), (vec![format!("Legendas (3): {onde}")], vec![modelo::IA_LEGENDAS.to_string(), modelo::IA_MODELOS.to_string()]), "{modo}");
            // o motor da degravação no anexo H (§6.6), sem arquivo quando o `feito` não traz o SHA-256 do modelo
            assert_eq!(d["modelos_ia"], json!([{ "modelo": "whisper", "arquivos": [], "origem": "—", "execucao": "—", "desvio": null, "uso": "degravação das legendas" }]), "{modo}");
            // escritas pelo perito: acréscimo sem IA
            let d = com(3, json!(modo), Value::Null);
            assert_eq!((legendas(&d), ia(&d)), (vec![format!("Legendas (3): {onde}")], nenhuma_ia.clone()), "{modo}");
        }
        // "conferidas pelo perito" só quando o `feito` não conta automática sem conferência
        let mut t = base.clone();
        t["feito"]["legendas"] = json!({ "origem": { "audio_sha256": "a", "importado_em": "", "motor": "whisper", "modelo_sha256": null }, "n": 5, "conferidas": 3, "modo": "queimar", "no_projeto": 7, "ia": 4, "ia_conferidas": 4 });
        assert_eq!(ia(&dados(&t, &[], &[]).unwrap()), vec![modelo::IA_LEGENDAS.to_string(), modelo::IA_MODELOS.to_string()]);
        t["feito"]["legendas"]["ia_conferidas"] = json!(3);
        t["feito"]["legendas"]["origem"]["modelo_sha256"] = json!("7".repeat(64));
        let d = dados(&t, &[], &[]).unwrap();
        assert_eq!(ia(&d), vec!["Legendas escritas a partir de degravação automática; sem conferência registrada pelo perito: 1.".to_string(), modelo::IA_MODELOS.to_string()]);
        assert_eq!(d["modelos_ia"][0]["arquivos"], json!([{ "nome": "arquivo do modelo", "sha256": "7".repeat(64) }]));
        assert!(!serde_json::to_string(&d).unwrap().contains("e conferidas pelo perito"), "o PDF não afirma a conferência que o feito não registra");
    }

    /// Sobrepostos (texto, terço inferior, imagem, marca): cada um vira uma linha dos acréscimos, com o tempo do
    /// vídeo e os itens que cobre. Sem eles no `feito` (exportação anterior), nada é inventado.
    #[test]
    fn sobrepostos_item_por_item() {
        let c = Cena::nova("juri");
        let (plano, res) = apoio::plano_e_resultado(&c);
        let mut t = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        let linhas = |t: &Value| {
            let d = dados(t, &[], &[]).unwrap();
            d["acrescimos"].as_array().unwrap().iter().map(|l| (l["o_que"].as_str().unwrap().to_string(), l["onde"].as_str().unwrap().to_string(), l["explicacao"].as_str().unwrap().to_string())).collect::<Vec<_>>()
        };
        let antes = linhas(&t);
        assert!(!antes.iter().any(|l| l.0.starts_with("Texto sobreposto") || l.0.starts_with("Imagem sobreposta")));
        // o roteiro da cena: item 1 nos slots 0..30, item 2 em 30..(30 + n), a 30 qps
        let n2 = t["feito"]["roteiro"][1]["na_saida"]["n_q"].as_u64().unwrap();
        t["feito"]["sobrepostos"] = json!([
            { "trilha": "s1", "clipe": "t1", "tipo": "titulo", "nome": "", "na_saida": { "de_q": 15, "n_q": 30, "de_s": 0.5, "ate_s": 1.5 }, "midia": null, "preso_a": "c1", "modelo": "terco_inferior", "textos": ["Rua das Flores, 120\nPortão", "14/11/2026"] },
            { "trilha": "s1", "clipe": "i1", "tipo": "imagem", "nome": "Brasão", "na_saida": { "de_q": 0, "n_q": 30 + n2, "de_s": 0.0, "ate_s": (30 + n2) as f64 / 30.0 }, "midia": "m9", "preso_a": null, "modelo": null, "textos": [] },
            { "trilha": "s2", "clipe": "i2", "tipo": "imagem", "nome": "", "na_saida": { "de_q": 40, "n_q": 5, "de_s": 40.0 / 30.0, "ate_s": 1.5 }, "midia": "m9", "preso_a": null, "modelo": null, "textos": [] },
        ]);
        let novas: Vec<_> = linhas(&t).into_iter().filter(|l| !antes.contains(l)).collect();
        assert_eq!(novas.len(), 3, "{novas:#?}");
        assert_eq!((novas[0].0.as_str(), novas[0].1.as_str()), ("Texto sobreposto: Rua das Flores, 120 Portão", "itens 1, 2, de 0:00,500 a 0:01,500"));
        assert_eq!(novas[0].2, frases::do_efeito(tipo::TEXTO_SOBREPOSTO).unwrap());
        assert_eq!(novas[1].0, "Imagem sobreposta: Brasão");
        assert!(novas[1].1.starts_with("itens 1, 2, de 0:00,000 a "), "{}", novas[1].1);
        assert_eq!((novas[2].0.as_str(), novas[2].1.as_str()), ("Imagem sobreposta", "item 2, de 0:01,333 a 0:01,500"));
        assert_eq!(novas[2].2, frases::do_efeito("imagem").unwrap());
        // o texto inteiro do título sobreposto, nos itens em que ele aparece; imagem sobreposta não tem texto
        let d = dados(&t, &[], &[]).unwrap();
        for i in [0, 1] {
            assert_eq!(d["roteiro"][i]["textos"], json!([{ "papel": "Texto sobreposto", "texto": "Rua das Flores, 120\nPortão\n14/11/2026" }]), "item {}", i + 1);
        }
        assert_eq!(d["roteiro"][2]["textos"], json!([]));
        // texto longo do perito não estoura a coluna
        t["feito"]["sobrepostos"][0]["textos"] = json!(["x".repeat(500)]);
        assert!(linhas(&t).iter().any(|l| l.0.chars().count() == "Texto sobreposto: ".chars().count() + 61));
    }

    /// O aviso que o programa escreve na imagem (selo) é acréscimo: com ele na imagem o PDF não diz "Nada foi
    /// acrescentado"; um por texto, com os itens que o levam.
    #[test]
    fn selos_entram_nos_acrescimos() {
        let c = Cena::nova("juri");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        let pv = &mut plano.proveniencia;
        // só mudança de forma de mostrar: nenhuma marcação, legenda, som posto nem sobreposto
        pv.roteiro.iter_mut().for_each(|it| {
            it.processamentos.clear();
            it.selos.clear();
        });
        pv.sobrepostos.clear();
        let sem = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        let avisos = |t: &Value| {
            let d = dados(t, &[], &[]).unwrap();
            d["acrescimos"].as_array().unwrap().iter().filter(|l| l["o_que"].as_str().unwrap().starts_with("Aviso escrito na imagem")).map(|l| (l["o_que"].as_str().unwrap().to_string(), l["onde"].as_str().unwrap().to_string(), l["explicacao"].as_str().unwrap().to_string())).collect::<Vec<_>>()
        };
        assert_eq!(avisos(&sem), vec![]);
        let antes = dados(&sem, &[], &[]).unwrap()["acrescimos"].as_array().unwrap().len();
        let mut com = sem.clone();
        com["feito"]["roteiro"][0]["selos"] = json!(["Velocidade 0,5×", "Imagem ampliada"]);
        com["feito"]["roteiro"][1]["selos"] = json!(["Imagem ampliada"]);
        let frase = frases::do_efeito(tipo::AVISO_NA_IMAGEM).unwrap().to_string();
        assert_eq!(avisos(&com), vec![
            ("Aviso escrito na imagem: Velocidade 0,5×".to_string(), "item 1".to_string(), frase.clone()),
            ("Aviso escrito na imagem: Imagem ampliada".to_string(), "itens 1, 2".to_string(), frase),
        ]);
        assert_eq!(dados(&com, &[], &[]).unwrap()["acrescimos"].as_array().unwrap().len(), antes + 2);
        assert_eq!(frases::classe_do_efeito(tipo::AVISO_NA_IMAGEM), Some(Classe::Acrescimo));
        // o selo de um sobreposto (imagem por IA) entra pelos itens em que ele aparece; texto igual ao de um clipe
        // junta numa linha só
        com["feito"]["sobrepostos"] = json!([{ "trilha": "s1", "clipe": "i1", "tipo": "imagem", "nome": "", "na_saida": { "de_q": 40, "n_q": 5, "de_s": 40.0 / 30.0, "ate_s": 1.5 },
            "midia": "m9", "preso_a": null, "modelo": null, "textos": [], "selos": ["Imagem ampliada por IA", "Imagem ampliada"] }]);
        let frase = frases::do_efeito(tipo::AVISO_NA_IMAGEM).unwrap().to_string();
        assert_eq!(avisos(&com), vec![
            ("Aviso escrito na imagem: Velocidade 0,5×".to_string(), "item 1".to_string(), frase.clone()),
            ("Aviso escrito na imagem: Imagem ampliada".to_string(), "itens 1, 2".to_string(), frase.clone()),
            ("Aviso escrito na imagem: Imagem ampliada por IA".to_string(), "item 2".to_string(), frase),
        ]);
    }

    /// Resolução e tamanho de cada gravação (18-V §4.3.1) saem do `feito`; sem eles, vazio, nunca inventado.
    #[test]
    fn resolucao_e_tamanho_dos_originais() {
        let c = Cena::nova("juri");
        let (plano, res) = apoio::plano_e_resultado(&c);
        let mut t = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        let d = dados(&t, &[], &[]).unwrap();
        assert_eq!((d["originais"][0]["imagem"].as_str(), d["originais"][0]["tamanho"].as_str()), (Some("1280 × 720 pontos"), Some("123 MB (123.456.789 bytes)")));
        assert!(d["diferencas"][0]["imagem"].as_str().unwrap().starts_with("1280×720 para "), "{}", d["diferencas"][0]["imagem"]);
        for k in ["largura", "altura", "tamanho"] {
            t["feito"]["midias"][0].as_object_mut().unwrap().remove(k);
        }
        let d = dados(&t, &[], &[]).unwrap();
        assert_eq!((&d["originais"][0]["imagem"], &d["originais"][0]["tamanho"]), (&Value::Null, &Value::Null));
        assert!(d["diferencas"][0]["imagem"].as_str().unwrap().starts_with("para "));
    }

    /// "PDF sem anexos": nenhum arquivo anexado e PDF/A-2b; a data e o identificador são os do render.
    #[test]
    fn entrada_sem_anexos_e_invalidas() {
        let c = Cena::nova("juri");
        let t = tecnico_completo(&c);
        let (e, texto) = pdf_de(&t, ModoPdf::A2SemAnexos);
        assert_eq!(e.dados["anexos"], json!([]));
        assert!(!e.arquivos.iter().any(|(n, _)| n.ends_with(".json")));
        assert!(texto.contains("Nenhum arquivo anexado."));
        let s = relatorio_pdf::gerar(&e).unwrap();
        assert!(relatorio_pdf::testes::contem(&s.pdf, b"<pdfaid:part>2</pdfaid:part>") && !relatorio_pdf::testes::contem(&s.pdf, b"/AFRelationship"));
        // mesma entrada, mesmos bytes; outro render, outro PDF
        assert!(relatorio_pdf::gerar(&e).unwrap().pdf == s.pdf);
        let mut outro = t.clone();
        outro["renderizado_em"] = json!("2026-11-15T10:00:04Z");
        let d = dados(&outro, &miniaturas(), &pacote()).unwrap();
        assert!(relatorio_pdf::gerar(&entrada(&outro, d, &miniaturas(), b"{}", ModoPdf::A2SemAnexos).unwrap()).unwrap().pdf != s.pdf);
        // sem data ou sem o SHA-256 do vídeo não há relatório
        let d = dados(&t, &[], &[]).unwrap();
        let mut ruim = t.clone();
        ruim["renderizado_em"] = json!("ontem");
        assert!(entrada(&ruim, d.clone(), &[], b"{}", ModoPdf::A3).is_err());
        let mut ruim = t.clone();
        ruim["execucao"]["saida"]["sha256"] = json!("");
        assert!(entrada(&ruim, d, &[], b"{}", ModoPdf::A3).is_err());
    }

    /// Uma linha da planilha: (nº, item, gravação, arquivo, intervalo, conteúdo, forma, filtro, motivo, quadros, região).
    fn linha_da_planilha(d: &Value, i: usize) -> [Value; 11] {
        let o = &d["ocultacoes"][i];
        ["n", "item", "gravacao", "arquivo", "intervalo", "conteudo", "forma", "filtro", "motivo", "quadros", "regiao"].map(|k| o[k].clone())
    }

    /// Planilha de ocultações do anexo E (SWGDE 18-M §11, figura 4): cada célula sai de um campo do `feito` (o campo
    /// muda, a célula muda; sem o campo, nada se afirma), com o vídeo, a data e a conferência no arquivo final.
    #[test]
    fn planilha_de_ocultacoes() {
        use crate::edicao::modelo::{ConteudoOculto, FormaRegiao, ModoOcultar};
        use crate::edicao::motor::plano::ProvQuadrosOcultos;
        let c = Cena::nova("juri-planilha");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        let pv = &mut plano.proveniencia;
        // r1: detecção de rosto confirmada pelo perito; r2: alvo seguido pelo VitTrack a partir de uma marcação
        pv.rastreios[0].origem = OrigemRastreio::Deteccao { classe: ClasseDeteccao::Rosto };
        pv.rastreios[0].corridas.clear();
        pv.rastreios.push(prov_rastreio("r2", OrigemRastreio::Assistido, 1, vec![valor(&corrida(MotorRastreio::Vittrack, "cpu", Sentido::Frente, 30, 89))]));
        let mut placa = pv.ocultacoes[0].clone();
        (placa.modo, placa.intensidade, placa.conteudo, placa.forma) = (ModoOcultar::Mosaico, 0.75, ConteudoOculto::Placa, FormaRegiao::Elipse);
        (placa.margem, placa.borda_suave, placa.motivo, placa.rastreio) = (0.1, 0.02, "placa de veículo de terceiro".into(), "r2".into());
        (placa.clipes, placa.de_s, placa.ate_s) = (vec!["c2".into(), "c3".into()], 1.0, 3.3);
        placa.quadros = ProvQuadrosOcultos { ancora: 1, medido: 40, interpolado: 3, incerto: 5, ..Default::default() };
        pv.ocultacoes.push(placa);
        let t = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        let d = dados(&t, &[], &[]).unwrap();
        let confirmado = "confirmado por Perito Teste em 14/11/2026, 6h00 (Brasília), de 0:01,000 a 0:02,000";
        assert_eq!(linha_da_planilha(&d, 0), [
            json!("1"), json!("2"), json!("A"), json!("cam 1.mp4"), json!("0:01,000 · 0:02,000"), json!("rosto"), json!("retângulo"),
            json!("sólido opaco, cor #000000ff"), json!("terceiro sem relação"), json!("quadros — na marcação do perito: 2 · medidos: 0 · interpolados: 0"),
            json!(format!("detecção automática (rosto) · marcações do perito: 2 · {confirmado} · R1 (anexo H)")),
        ]);
        assert_eq!(linha_da_planilha(&d, 1), [
            json!("2"), json!("2, 3"), json!("A"), json!("cam 1.mp4"), json!("0:01,000 · 0:03,300"), json!("placa"), json!("elipse, margem 10 %, borda suave 2 %"),
            json!("mosaico, intensidade 0,75"), json!("placa de veículo de terceiro"), json!("quadros — na marcação do perito: 1 · medidos: 40 · interpolados: 3 · dos medidos e interpolados, incertos: 5"),
            json!("acompanhamento por programa a partir de marcação do perito · marcações do perito: 1 · R2 (anexo H)"),
        ]);
        // o som: sem forma, sem região, sem quadros
        assert_eq!(linha_da_planilha(&d, 2), [
            json!("3"), json!("2"), json!("A"), json!("cam 1.mp4"), json!("0:01,500 · 0:02,000"), json!("som"), json!("—"), json!("tom no lugar do som"),
            json!("nome"), Value::Null, Value::Null,
        ]);
        // cada célula pelo seu campo do `feito`
        let com = |muda: &dyn Fn(&mut Value)| {
            let mut t = t.clone();
            muda(&mut t);
            dados(&t, &[], &[]).unwrap()
        };
        let regiao = |d: &Value, i: usize| d["ocultacoes"][i]["regiao"].as_str().unwrap().to_string();
        // detecção vira marcação (e sem corrida o r1 sai do anexo H: o r2 passa a ser R1)
        let d2 = com(&|t| t["feito"]["rastreios"][0]["origem"] = json!({ "tipo": "manual" }));
        assert_eq!((regiao(&d2, 0), regiao(&d2, 1)), (format!("marcação do perito · marcações do perito: 2 · {confirmado}"), "acompanhamento por programa a partir de marcação do perito · marcações do perito: 1 · R1 (anexo H)".into()));
        let d2 = com(&|t| t["feito"]["rastreios"][0]["origem"] = json!({ "tipo": "pessoas", "pessoa_id": "p", "estado": "auto", "copiado_em": "x" }));
        assert!(regiao(&d2, 0).starts_with("trilha automática da análise de pessoas · "), "{}", regiao(&d2, 0));
        let d2 = com(&|t| t["feito"]["rastreios"][0]["origem"] = json!({ "tipo": "pessoas", "pessoa_id": "p", "estado": "revisada", "copiado_em": "x" }));
        assert!(regiao(&d2, 0).starts_with("trilha da análise de pessoas, revisada pelo perito na análise · "), "{}", regiao(&d2, 0));
        let d2 = com(&|t| t["feito"]["rastreios"][0]["origem"] = json!({ "tipo": "deteccao", "classe": "placa" }));
        assert!(regiao(&d2, 0).starts_with("detecção automática (placa) · "));
        // origem que o catálogo não conhece: não se diz como nasceu
        let d2 = com(&|t| t["feito"]["rastreios"][0]["origem"] = json!({ "tipo": "futuro" }));
        assert_eq!(regiao(&d2, 0), format!("marcações do perito: 2 · {confirmado}"));
        // marcações: o número do `feito`; sem o campo, não se afirma
        assert!(regiao(&com(&|t| t["feito"]["rastreios"][1]["ancoras"] = json!(7)), 1).contains("marcações do perito: 7 ·"));
        assert_eq!(regiao(&com(&|t| drop(t["feito"]["rastreios"][1].as_object_mut().unwrap().remove("ancoras"))), 1), "acompanhamento por programa a partir de marcação do perito · R2 (anexo H)");
        // confirmação: só inteira
        assert!(!regiao(&com(&|t| t["feito"]["rastreios"][0]["confirmacao"] = Value::Null), 0).contains("confirmado"));
        assert!(!regiao(&com(&|t| drop(t["feito"]["rastreios"][0]["confirmacao"].as_object_mut().unwrap().remove("em"))), 0).contains("confirmado"));
        assert!(regiao(&com(&|t| t["feito"]["rastreios"][0]["confirmacao"]["por"] = json!("Outro Perito")), 0).contains("confirmado por Outro Perito em"));
        // confirmação presa a outras amostras (nova corrida depois dela) não vale (§6.6): não se afirma
        assert!(!regiao(&com(&|t| t["feito"]["rastreios"][0]["confirmacao"]["denso_sha256"] = json!("9".repeat(64))), 0).contains("confirmado"));
        let conf = |sha: Value| json!({ "por": "Perito Teste", "em": "2026-11-14T09:00:00Z", "denso_sha256": sha, "de_s": 1.0, "ate_s": 3.3 });
        let sha_r2 = t["feito"]["rastreios"][1]["denso_sha256"].clone();
        assert!(sha_r2.as_str().is_some_and(|s| s.len() == 64));
        assert!(regiao(&com(&|t| t["feito"]["rastreios"][1]["confirmacao"] = conf(sha_r2.clone())), 1).contains("confirmado por Perito Teste em 14/11/2026, 6h00 (Brasília), de 0:01,000 a 0:03,300"));
        for outra in [json!("9".repeat(64)), Value::Null] {
            let d2 = com(&|t| t["feito"]["rastreios"][1]["confirmacao"] = conf(outra.clone()));
            assert!(!regiao(&d2, 1).contains("confirmado"), "{outra}");
            assert_eq!(d2["rastreios"][1]["confirmacao"], json!("—"), "{outra}");
        }
        // rastreio que o `feito` não traz: nada se afirma da região
        assert_eq!(regiao(&com(&|t| t["feito"]["ocultacoes"][0]["rastreio"] = json!("rx")), 0), "—");
        // arquivo, quadros, forma e filtro
        assert_eq!(com(&|t| t["feito"]["midias"][0]["nome"] = json!("outra câmera.mp4"))["ocultacoes"][0]["arquivo"], json!("outra câmera.mp4"));
        assert_eq!(com(&|t| t["feito"]["ocultacoes"][1]["quadros"]["incerto"] = json!(9))["ocultacoes"][1]["quadros"], json!("quadros — na marcação do perito: 1 · medidos: 40 · interpolados: 3 · dos medidos e interpolados, incertos: 9"));
        // quadros (fecho da Fase 2): os que seguram a última marcação, os do "Simplificar" (nunca "do perito") e os sem
        // região numa ausência aceita, cada um na sua parte, só quando há; os incertos ditos dentro dos medidos e
        // interpolados (não se somam)
        let quadros = |muda: Value| {
            let d = com(&|t| muda.as_object().unwrap().iter().for_each(|(k, v)| t["feito"]["ocultacoes"][1]["quadros"][k] = v.clone()));
            d["ocultacoes"][1]["quadros"].as_str().unwrap().to_string()
        };
        assert_eq!(
            quadros(json!({ "segurado": 30, "ancora_do_programa": 12, "ausente": 4 })),
            "quadros — na marcação do perito: 1 · em ponto escolhido pelo programa entre as amostras: 12 · com a última marcação do perito mantida: 30 · \
             medidos: 40 · interpolados: 3 · dos medidos e interpolados, incertos: 5 · sem região, em ausência aceita pelo perito: 4"
        );
        assert_eq!(quadros(json!({ "medido": 0, "interpolado": 0, "incerto": 0 })), "quadros — na marcação do perito: 1 · medidos: 0 · interpolados: 0");
        // invertida: o quadro sem região sai todo coberto
        let d = com(&|t| {
            t["feito"]["ocultacoes"][1]["inverter"] = json!(true);
            t["feito"]["ocultacoes"][1]["quadros"]["ausente"] = json!(4);
        });
        assert_eq!(
            d["ocultacoes"][1]["quadros"],
            json!("quadros — na marcação do perito: 1 · medidos: 40 · interpolados: 3 · dos medidos e interpolados, incertos: 5 · quadro inteiro coberto, em ausência aceita pelo perito: 4")
        );
        assert_eq!(quadros(json!({ "ancora": 0, "ancora_do_programa": 2 })), "quadros — na marcação do perito: 0 · em ponto escolhido pelo programa entre as amostras: 2 · medidos: 40 · interpolados: 3 · dos medidos e interpolados, incertos: 5");
        assert_eq!(com(&|t| t["feito"]["ocultacoes"][1]["inverter"] = json!(true))["ocultacoes"][1]["forma"], json!("elipse, invertida (cobre tudo o que está fora da forma), margem 10 %, borda suave 2 %"));
        assert_eq!(com(&|t| t["feito"]["ocultacoes"][1]["modo"] = json!("desfoque"))["ocultacoes"][1]["filtro"], json!("desfoque, intensidade 0,75"));
        // no PDF: a planilha com o vídeo, a data e a conferência (pendente; depois conferida, por quem e quando)
        let (_, texto) = pdf_de(&t, ModoPdf::A3);
        let tem = |texto: &str, s: &str| assert!(texto.contains(s), "falta no PDF: {s}\n{texto}");
        for s in [
            "Vídeo com as ocultações Caso.mp4",
            "Exportado em 14 de novembro de 2026, às 7h00 (horário de Brasília)",
            "Conferência no arquivo final A conferência das ocultações no arquivo final ainda não foi registrada.",
            "Gravação original",
            "A · cam 1.mp4",
            "elipse, margem 10 %, borda suave 2 %",
            "mosaico, intensidade 0,75",
            "placa de veículo de terceiro",
            "Região de cada ocultação de imagem",
            "acompanhamento por programa a partir de marcação do perito · marcações do perito: 1 · R2 (anexo H)",
            "quadros — na marcação do perito: 1 · medidos: 40 · interpolados: 3 · dos medidos e interpolados, incertos: 5",
            confirmado,
        ] {
            tem(&texto, s);
        }
        let mut t = t.clone();
        completar_conferencia(&mut t, &ConferenciaOcultacoes { conferida: true, por: "Perito Teste".into(), em: "2026-11-14T11:00:00Z".into() }).unwrap();
        let (_, texto) = pdf_de(&t, ModoPdf::A3);
        tem(&texto, "Conferência no arquivo final As ocultações foram conferidas pelo perito no arquivo final. Perito Teste, em 14 de novembro de 2026, às 8h00 (horário de Brasília).");
        assert!(!texto.contains(apoio::NOME_RASTREIO) && !texto.contains(&*c.caso.to_string_lossy()));
    }

    /// Anexo H, modelos por arquivo: os de `feito.ia` e os das corridas (`feito.rastreios[].corridas`) que ele não traz,
    /// cada arquivo com o SHA-256, onde rodou ("CPU" / "CUDA") e em que rastreio; cada rastreio em que um programa
    /// atuou, com as corridas. Correlação não é modelo de IA; marcação só do perito não entra no anexo H.
    #[test]
    fn anexo_h_modelos_por_arquivo() {
        let c = Cena::nova("juri-anexo-h");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        let pv = &mut plano.proveniencia;
        pv.ia.clear();
        // r1 (detecção) seguido duas vezes pelo EdgeTAM na GPU: um modelo só; a corrida antiga da cena fica
        pv.rastreios[0].origem = OrigemRastreio::Deteccao { classe: ClasseDeteccao::Rosto };
        let (frente, tras) = (corrida(MotorRastreio::Edgetam, "cuda", Sentido::Frente, 30, 60), corrida(MotorRastreio::Edgetam, "cuda", Sentido::Tras, 0, 30));
        pv.rastreios[0].corridas.extend([valor(&frente), valor(&tras)]);
        // r2: VitTrack em CPU e correlação; r3: só correlação; r4: só marcação
        let vit = corrida(MotorRastreio::Vittrack, "cpu", Sentido::Frente, 90, 100);
        pv.rastreios.push(prov_rastreio("r2", OrigemRastreio::Assistido, 3, vec![valor(&vit), valor(&corrida(MotorRastreio::Mancha, "cpu", Sentido::Tras, 90, 100))]));
        pv.rastreios.push(prov_rastreio("r3", OrigemRastreio::Assistido, 1, vec![valor(&corrida(MotorRastreio::Mancha, "cpu", Sentido::Frente, 30, 89))]));
        pv.rastreios.push(prov_rastreio("r4", OrigemRastreio::Manual, 2, vec![]));
        let proc = |nome: &str, ordem: u32, alvo: Value| ProvProcessamento { ordem, nome: nome.into(), parametros: alvo };
        pv.roteiro[1].processamentos = vec![proc("destaque", 1, json!({ "tipo": "destaque", "id": "e1", "rastreio": "r3" })), proc("destaque", 2, json!({ "tipo": "destaque", "id": "e2", "rastreio": "r4" }))];
        pv.roteiro[2].processamentos = vec![proc("holofote", 1, json!({ "tipo": "holofote", "id": "e3", "alvo": { "rastreio": "r2" } }))];
        let t = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        let d = dados(&t, &[], &[]).unwrap();
        let arquivos_de = |c: &Corrida| json!(c.motor.modelo.arquivos.iter().map(|a| json!({ "nome": a.nome, "sha256": a.sha256 })).collect::<Vec<_>>());
        let esperado = json!([
            { "modelo": "EdgeTAM vídeo", "arquivos": arquivos_de(&frente), "origem": format!("{} · {}", edgetam::ORIGEM, edgetam::COMMIT),
              "execucao": "onnxruntime 1.31.1 · CUDA", "desvio": edgetam::DESVIO, "uso": "acompanhamento de alvo (R1)" },
            { "modelo": "VitTrack", "arquivos": [{ "nome": vittrack::NOME_ARQUIVO, "sha256": vittrack::SHA256 }], "origem": format!("{} · {}", vittrack::ORIGEM, vittrack::COMMIT),
              "execucao": "onnxruntime 1.31.1 · CPU", "desvio": null, "uso": "acompanhamento de alvo (R2)" },
        ]);
        assert_eq!(d["modelos_ia"], esperado);
        assert_eq!(d["modelos_ia"][0]["arquivos"].as_array().unwrap().len(), 4, "os quatro grafos do EdgeTAM");
        // `feito.ia` como o P2.1 agrupa (e o de exportação antiga, com o provedor em maiúsculas): mesma tabela, sem repetir
        for prov in ["cuda", "CUDA"] {
            let mut t2 = t.clone();
            let mut ia = vec![valor(&crate::ia::uso::da_corrida(&frente, "rastreio")), valor(&crate::ia::uso::da_corrida(&vit, "rastreio"))];
            ia[0]["provedor"] = json!(prov);
            t2["feito"]["ia"] = json!(ia);
            assert_eq!(dados(&t2, &[], &[]).unwrap()["modelos_ia"], esperado, "{prov}");
        }
        // modelo de `feito.ia` que nenhuma corrida usa: listado como o `feito` diz, sem rastreio
        let mut t2 = t.clone();
        t2["feito"]["ia"] = json!([{ "modelo": "YOLOX-S", "arquivos": [{ "nome": "yolox_s.onnx", "sha256": "8".repeat(64) }], "origem": "x", "commit": "", "desvio": "",
            "runtime": "onnxruntime 1.31.1", "provedor": "cpu", "uso": "deteccao" }]);
        let m = dados(&t2, &[], &[]).unwrap()["modelos_ia"].clone();
        assert_eq!((m.as_array().unwrap().len(), &m[0]["modelo"], &m[0]["uso"], &m[0]["execucao"]), (3, &json!("YOLOX-S"), &json!("proposta de ocultação por detecção automática"), &json!("onnxruntime 1.31.1 · CPU")));
        // onnxruntime com a versão e onde rodou, antes do sistema
        let sw: Vec<Vec<String>> = serde_json::from_value(d["software"].clone()).unwrap();
        assert_eq!(sw.iter().map(|l| l[0].as_str()).collect::<Vec<_>>(), ["SICRO", "FFmpeg", "Typst", "onnxruntime", "Sistema"]);
        assert_eq!(sw[3], ["onnxruntime", "1.31.1", "execução dos modelos de inteligência artificial · CUDA, CPU"]);
        // rastreios: r1, r2 e r3 (r4 só tem marcação); a correlação aparece como tal
        let corrida_frente = "EdgeTAM vídeo · onnxruntime 1.31.1 · CUDA · para a frente, 0:01,000 a 0:02,000 (pts 15360 a 30720) · parada: fim do trecho · limiares 0,95 e 0,5, paciência de 90 quadros · 14/11/2026, 5h30 (Brasília)";
        assert_eq!(d["rastreios"], json!([
            { "id": "R1", "origem": "detecção automática (rosto) · marcações do perito: 2", "uso": "ocultação 1",
              "corridas": [corrida_frente, corrida_frente.replace("para a frente, 0:01,000 a 0:02,000 (pts 15360 a 30720)", "para trás, 0:00,000 a 0:01,000 (pts 0 a 15360)"), "corridas sem registro legível: 1"],
              "confirmacao": "confirmado por Perito Teste em 14/11/2026, 6h00 (Brasília), de 0:01,000 a 0:02,000", "amostras": "—" },
            { "id": "R2", "origem": "acompanhamento por programa a partir de marcação do perito · marcações do perito: 3", "uso": "item 3",
              "corridas": [
                  "VitTrack · onnxruntime 1.31.1 · CPU · para a frente, 0:03,000 a 0:03,333 (pts 46080 a 51200) · parada: fim do trecho · limiares 0,95 e 0,5, paciência de 90 quadros · 14/11/2026, 5h30 (Brasília)",
                  "correlação, sem inteligência artificial · CPU · para trás, 0:03,000 a 0:03,333 (pts 46080 a 51200) · parada: fim do trecho · limiares 0,95 e 0,5, paciência de 90 quadros · 14/11/2026, 5h30 (Brasília)",
              ],
              "confirmacao": "—", "amostras": format!("{:x}", Sha256::digest(b"r2")) },
            { "id": "R3", "origem": "acompanhamento por programa a partir de marcação do perito · marcações do perito: 1", "uso": "item 2",
              "corridas": ["correlação, sem inteligência artificial · CPU · para a frente, 0:01,000 a 0:02,967 (pts 15360 a 45568) · parada: fim do trecho · limiares 0,95 e 0,5, paciência de 90 quadros · 14/11/2026, 5h30 (Brasília)"],
              "confirmacao": "—", "amostras": format!("{:x}", Sha256::digest(b"r3")) },
        ]));
        // cada pedaço da corrida pelo seu campo: limiares, parada, quando, base de tempo da gravação
        let mut t2 = t.clone();
        let cr = &mut t2["feito"]["rastreios"][0]["corridas"][1];
        (cr["limiares"], cr["parou"], cr["feito_em"]) = (json!({ "ok": 0.45, "perdido": 0.3, "paciencia": 60 }), json!("saiu_do_quadro"), json!("2026-11-20T14:02:11Z"));
        t2["feito"]["midias"][0]["medido"]["tb"] = json!({ "num": 1, "den": 30720 });
        let r1 = dados(&t2, &[], &[]).unwrap()["rastreios"][0]["corridas"][0].as_str().unwrap().to_string();
        assert_eq!(r1, "EdgeTAM vídeo · onnxruntime 1.31.1 · CUDA · para a frente, 0:00,500 a 0:01,000 (pts 15360 a 30720) · parada: alvo saiu da imagem · limiares 0,45 e 0,3, paciência de 60 quadros · 20/11/2026, 11h02 (Brasília)");
        // sem saber a gravação do rastreio, só os pts
        t2["feito"]["ocultacoes"][0]["rastreio"] = json!("rx");
        assert!(dados(&t2, &[], &[]).unwrap()["rastreios"][0]["corridas"][0].as_str().unwrap().contains("para a frente, pts 15360 a 30720 · "));
        // resumo para leigo: detecção, alvo seguido por IA (só o r2: o r3 é correlação) e a remissão ao anexo H
        let ia: Vec<String> = serde_json::from_value(d["ia_resumo"].clone()).unwrap();
        assert_eq!(ia, ["Ocultações propostas por detecção automática e confirmadas pelo perito: 1.", "Nessas ocultações, imagens com a posição achada pelo programa: 0; calculada a partir das posições vizinhas: 0; marcada pelo perito: 2.", "Alvos acompanhados por programa de inteligência artificial a partir de marcação do perito: 1.", modelo::IA_MODELOS]);
        // no PDF: arquivos e SHA-256 inteiros, provedor em maiúsculas, as corridas
        let (_, texto) = pdf_de(&t, ModoPdf::A3);
        let tem = |s: &str| assert!(texto.contains(s), "falta no PDF: {s}\n{texto}");
        for a in edgetam::ARQUIVOS {
            tem(a.nome);
            tem(a.sha256);
        }
        for s in [vittrack::NOME_ARQUIVO, vittrack::SHA256, "Acompanhamento de alvos", "desvio conhecido: melhor de 3 máscaras no quadro marcado", "acompanhamento de alvo (R1)",
                  "onnxruntime 1.31.1 · CUDA", "onnxruntime 1.31.1 · CPU", corrida_frente, "correlação, sem inteligência artificial · CPU", "corridas sem registro legível: 1"] {
            tem(s);
        }
        assert!(!texto.contains("cuda") && !texto.contains(" cpu "), "provedor como veio do fio");
        assert!(!texto.contains(apoio::NOME_RASTREIO) && !texto.contains(&*c.caso.to_string_lossy()));
    }

    /// Resumo para leigo do rastreio assistido, pelas corridas: correlação não é IA ("nenhuma etapa" vale); motor
    /// neural é; sem corrida legível não se sabe, e nem "nenhuma etapa" pode ser dito.
    #[test]
    fn ia_do_rastreio_assistido_pelas_corridas() {
        let c = Cena::nova("juri-ia");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        plano.proveniencia.ia.clear();
        (plano.proveniencia.rastreios[0].origem, plano.proveniencia.rastreios[0].confirmacao) = (OrigemRastreio::Manual, None);
        plano.proveniencia.rastreios[0].corridas.clear();
        let base = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        let resumo = |origem: &str, corridas: Value| {
            let mut t = base.clone();
            (t["feito"]["rastreios"][0]["origem"], t["feito"]["rastreios"][0]["corridas"]) = (json!({ "tipo": origem }), corridas);
            dados(&t, &[], &[]).unwrap()["ia_resumo"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect::<Vec<_>>()
        };
        let nenhuma = frases::da_condicao(condicao::IA_NENHUMA).unwrap().to_string();
        let sem_registro = "Alvos acompanhados por programa a partir de marcação do perito, sem registro do programa usado: 1.".to_string();
        let mancha = valor(&corrida(MotorRastreio::Mancha, "cpu", Sentido::Frente, 30, 60));
        let vit = valor(&corrida(MotorRastreio::Vittrack, "cpu", Sentido::Frente, 30, 60));
        let ilegivel = json!({ "motor": "edgetam" });
        assert_eq!(resumo("manual", json!([])), [nenhuma.clone()]);
        assert_eq!(resumo("assistido", json!([mancha])), [nenhuma.clone()]);
        assert_eq!(resumo("assistido", json!([])), [sem_registro.clone()]);
        assert_eq!(resumo("assistido", json!([ilegivel])), [sem_registro.clone()]);
        assert_eq!(resumo("assistido", json!([mancha, ilegivel])), [sem_registro.clone()]);
        let com_ia = ["Alvos acompanhados por programa de inteligência artificial a partir de marcação do perito: 1.".to_string(), modelo::IA_MODELOS.to_string()];
        assert_eq!(resumo("assistido", json!([mancha, vit])), com_ia);
        // corrida neural em rastreio manual (o perito marcou depois por cima): o modelo vai ao anexo H e ao resumo
        assert_eq!(resumo("manual", json!([vit])), [modelo::IA_MODELOS.to_string()]);
        // corrida ilegível também em rastreio manual: não se sabe se houve IA, "nenhuma etapa" não é dito
        assert_eq!(resumo("manual", json!([ilegivel])), [sem_registro.clone()]);
        assert_eq!(resumo("manual", json!([mancha, ilegivel])), [sem_registro.clone()]);
    }

    /// Fecho da Fase 2: a corrida só diz algo do vídeo se a faixa dela toca um trecho em que o rastreio é usado (fora
    /// dele a amostra que se vê veio de outro motor ou de âncora). Corrida neural só fora não vira IA no resumo nem
    /// modelo no anexo H (nem pelo `feito.ia` do P2.1); posição do "Simplificar" no vídeo sem registro do programa é
    /// "sem registro"; `feito` sem os pts do roteiro não recorta nada.
    #[test]
    fn ia_so_das_corridas_com_amostra_no_video() {
        let c = Cena::nova("juri-ia-video");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        plano.proveniencia.ia.clear();
        (plano.proveniencia.rastreios[0].origem, plano.proveniencia.rastreios[0].confirmacao) = (OrigemRastreio::Assistido, None);
        plano.proveniencia.rastreios[0].corridas.clear();
        let base = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        // r1 é usado pela ocultação 1, no clipe c2 (quadros 30..89 da gravação, passo 512)
        assert_eq!(faixas_de_uso(&serde_json::from_value(base["feito"].clone()).unwrap(), "r1"), [(512 * 30, 512 * 89, 512)]);
        let edgetam = |de: i64, ate: i64| corrida(MotorRastreio::Edgetam, "cuda", Sentido::Frente, de, ate);
        let mancha = corrida(MotorRastreio::Mancha, "cpu", Sentido::Frente, 30, 60);
        let com = |corridas: &[&Corrida], muda: &dyn Fn(&mut Value)| {
            let mut t = base.clone();
            let rs: Vec<ProvRastreio> = vec![prov_rastreio("r1", OrigemRastreio::Assistido, 1, corridas.iter().map(|c| valor(*c)).collect())];
            t["feito"]["rastreios"][0]["corridas"] = json!(rs[0].corridas);
            // o `feito.ia` como o P2.1 grava: de todas as corridas do denso
            t["feito"]["ia"] = valor(&crate::ia::uso::da_proveniencia(&rs));
            muda(&mut t);
            dados(&t, &[], &[]).unwrap()
        };
        let resumo = |d: &Value| d["ia_resumo"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect::<Vec<_>>();
        let corridas_de = |d: &Value| d["rastreios"][0]["corridas"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect::<Vec<_>>();
        let nenhuma = frases::da_condicao(condicao::IA_NENHUMA).unwrap().to_string();
        let com_ia = ["Alvos acompanhados por programa de inteligência artificial a partir de marcação do perito: 1.".to_string(), modelo::IA_MODELOS.to_string()];
        let sem_registro = "Alvos acompanhados por programa a partir de marcação do perito, sem registro do programa usado: 1.".to_string();
        let nada = |_: &mut Value| {};
        // EdgeTAM só nos quadros 200..250: o que se vê veio da correlação
        let fora = edgetam(200, 250);
        let d = com(&[&fora, &mancha], &nada);
        assert_eq!(resumo(&d), [nenhuma.clone()]);
        assert_eq!(d["modelos_ia"], json!([]));
        let linhas = corridas_de(&d);
        assert_eq!(linhas.len(), 2, "{linhas:?}");
        assert!(linhas[0].starts_with("correlação, sem inteligência artificial · CPU · para a frente, 0:01,000 a 0:02,000"), "{}", linhas[0]);
        assert_eq!(linhas[1], "corridas sem amostra nos trechos usados: 1");
        // só a corrida de fora: nada se afirma do rastreio (as posições do vídeo são as do perito)
        assert_eq!(resumo(&com(&[&fora], &nada)), [nenhuma.clone()]);
        // a faixa toca o trecho (quadros 80..120): IA, e o modelo no anexo H
        let dentro = edgetam(80, 120);
        let d = com(&[&dentro, &mancha], &nada);
        assert_eq!(resumo(&d), com_ia);
        assert_eq!(d["modelos_ia"].as_array().unwrap().len(), 1);
        assert_eq!(corridas_de(&d).len(), 2);
        // velocidade exibida que acompanha o r1 no congelado c3 (quadro 100, `onde.rastreio`): a corrida que só
        // cobre esse quadro deu amostra ao vídeo
        let no_c3 = edgetam(100, 100);
        assert_eq!(resumo(&com(&[&no_c3, &mancha], &nada)), [nenhuma.clone()]);
        let velocidade = |t: &mut Value| {
            let it = t["feito"]["roteiro"].as_array_mut().unwrap().iter_mut().find(|it| it["clipe"] == "c3").unwrap();
            let v = json!({ "ordem": 9, "nome": "velocidade_exibida", "parametros": { "onde": { "modo": "segue", "rastreio": "r1", "dx": 0.0, "dy": 0.0 } } });
            it["processamentos"].as_array_mut().unwrap().push(v);
        };
        let d = com(&[&no_c3, &mancha], &velocidade);
        assert_eq!((resumo(&d), d["modelos_ia"].as_array().unwrap().len()), (com_ia.to_vec(), 1));
        // `feito` sem os pts do roteiro (anterior a eles): não se recorta, como antes
        let sem_pts = |t: &mut Value| t["feito"]["roteiro"].as_array_mut().unwrap().iter_mut().filter(|it| it["origem"].is_object()).for_each(|it| it["origem"]["lidos"] = json!(0));
        assert_eq!(resumo(&com(&[&fora, &mancha], &sem_pts)), com_ia);
        // posição do "Simplificar" no vídeo (o denso que a deu saiu): programa sem registro, mesmo com a correlação
        let do_programa = |t: &mut Value| t["feito"]["ocultacoes"][0]["quadros"]["ancora_do_programa"] = json!(3);
        assert_eq!(resumo(&com(&[&mancha], &do_programa)), [sem_registro.clone()]);
        let interpolado_do_programa = |t: &mut Value| t["feito"]["ocultacoes"][0]["quadros"]["interpolado_do_programa"] = json!(5);
        assert_eq!(resumo(&com(&[&fora], &interpolado_do_programa)), [sem_registro.clone()]);
        assert_eq!(resumo(&com(&[&dentro], &do_programa)), com_ia);
    }

    /// Revisão do fecho: a linha do denso é da ÚLTIMA corrida cuja faixa cobre o pts (cada corrida substitui a faixa
    /// que percorreu); a fundida entre duas âncoras, das duas. EdgeTAM de 0 a 120 e depois "Retomar" de 25 a 120 por
    /// outro motor: o trecho usado (30..89) é todo do segundo e o EdgeTAM não vira IA no resumo nem modelo no anexo H
    /// (antes, a faixa dele tocava o trecho e virava). Para trás de 120 até 25, de frente para ele: linhas fundidas, o
    /// EdgeTAM conta. Uma corrida só: como antes.
    #[test]
    fn ia_pela_corrida_que_escreveu_a_linha() {
        let c = Cena::nova("juri-ia-escritora");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        plano.proveniencia.ia.clear();
        (plano.proveniencia.rastreios[0].origem, plano.proveniencia.rastreios[0].confirmacao) = (OrigemRastreio::Assistido, None);
        plano.proveniencia.rastreios[0].corridas.clear();
        let base = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        let com = |corridas: &[&Corrida]| {
            let mut t = base.clone();
            let rs: Vec<ProvRastreio> = vec![prov_rastreio("r1", OrigemRastreio::Assistido, 1, corridas.iter().map(|c| valor(*c)).collect())];
            t["feito"]["rastreios"][0]["corridas"] = json!(rs[0].corridas);
            // o `feito.ia` como o P2.1 grava: de todas as corridas do denso
            t["feito"]["ia"] = valor(&crate::ia::uso::da_proveniencia(&rs));
            dados(&t, &[], &[]).unwrap()
        };
        let resumo = |d: &Value| d["ia_resumo"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect::<Vec<_>>();
        let corridas_de = |d: &Value| d["rastreios"][0]["corridas"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect::<Vec<_>>();
        let modelos = |d: &Value| d["modelos_ia"].to_string();
        let nenhuma = vec![frases::da_condicao(condicao::IA_NENHUMA).unwrap().to_string()];
        let com_ia = vec!["Alvos acompanhados por programa de inteligência artificial a partir de marcação do perito: 1.".to_string(), modelo::IA_MODELOS.to_string()];
        let com_ancora = |motor: MotorRastreio, sentido: Sentido, ancora: i64, de: i64, ate: i64| {
            let mut x = corrida(motor, if motor == MotorRastreio::Edgetam { "cuda" } else { "cpu" }, sentido, de, ate);
            x.ancora_pts = 512 * ancora;
            x
        };
        let edgetam = com_ancora(MotorRastreio::Edgetam, Sentido::Frente, 0, 0, 120);
        // "Retomar" pela correlação: nenhuma IA; o EdgeTAM fica só na conta
        let d = com(&[&edgetam, &com_ancora(MotorRastreio::Mancha, Sentido::Frente, 25, 25, 120)]);
        assert_eq!((resumo(&d), d["modelos_ia"].clone()), (nenhuma.clone(), json!([])));
        let linhas = corridas_de(&d);
        assert!(linhas.len() == 2 && linhas[0].starts_with("correlação, sem inteligência artificial") && linhas[1] == "corridas sem amostra nos trechos usados: 1", "{linhas:?}");
        // pelo VitTrack: IA, e no anexo H só ele
        let d = com(&[&edgetam, &com_ancora(MotorRastreio::Vittrack, Sentido::Frente, 25, 25, 120)]);
        assert_eq!(resumo(&d), com_ia);
        assert!(modelos(&d).contains("VitTrack") && !modelos(&d).contains("EdgeTAM"), "{}", modelos(&d));
        assert_eq!(corridas_de(&d).last().unwrap(), "corridas sem amostra nos trechos usados: 1");
        // do mesmo sentido, para trás: substitui também
        let edgetam_tras = com_ancora(MotorRastreio::Edgetam, Sentido::Tras, 120, 0, 120);
        assert_eq!(resumo(&com(&[&edgetam_tras, &com_ancora(MotorRastreio::Mancha, Sentido::Tras, 120, 25, 120)])), nenhuma);
        // para trás de 120 até 25, de frente para o EdgeTAM (âncora 0): fundidas, o EdgeTAM conta
        let d = com(&[&edgetam, &com_ancora(MotorRastreio::Mancha, Sentido::Tras, 120, 25, 120)]);
        assert_eq!(resumo(&d), com_ia);
        assert!(modelos(&d).contains("EdgeTAM"), "{}", modelos(&d));
        assert_eq!(corridas_de(&d).len(), 2);
        // a ordem de `corridas[]` decide: o EdgeTAM por último cobre o trecho
        let d = com(&[&com_ancora(MotorRastreio::Mancha, Sentido::Frente, 25, 25, 120), &edgetam]);
        assert!(resumo(&d) == com_ia && modelos(&d).contains("EdgeTAM"));
        // uma corrida só: como antes
        assert_eq!(resumo(&com(&[&edgetam])), com_ia);
        assert_eq!(resumo(&com(&[&com_ancora(MotorRastreio::Edgetam, Sentido::Frente, 0, 0, 29)])), nenhuma);
    }

    /// A regra quadro a quadro: a escritora (a última que cobre o pts) e, se de frente para ela, a anterior. Dois
    /// "Retomar" encostados (quadros 60 e 61) não deixam a de baixo entre eles quando o passo da gravação é conhecido;
    /// sem os pts do uso, todas.
    #[test]
    fn corridas_no_video_pela_escritora() {
        let c = |s: Sentido, ancora: i64, de: i64, ate: i64| {
            let mut x = corrida(MotorRastreio::Mancha, "cpu", s, de, ate);
            x.ancora_pts = 512 * ancora;
            x
        };
        // clipe dos quadros `de..=ate`, passo 512
        let q = |de: i64, ate: i64| [(512 * de, 512 * ate, 512)];
        let (a, b, d) = (c(Sentido::Frente, 0, 0, 100), c(Sentido::Frente, 30, 30, 60), c(Sentido::Frente, 61, 61, 90));
        let abd = [a.clone(), b.clone(), d.clone()];
        assert_eq!(corridas_no_video(&abd, &q(60, 61)), [false, true, true]);
        assert_eq!(corridas_no_video(&abd, &q(40, 70)), [false, true, true]);
        assert_eq!(corridas_no_video(&abd, &q(40, 55)), [false, true, false]);
        assert_eq!(corridas_no_video(&abd, &q(91, 95)), [true, false, false]);
        assert_eq!(corridas_no_video(&abd, &q(101, 200)), [false, false, false]);
        // com um quadro entre as duas (62), a de baixo escreve nele
        assert_eq!(corridas_no_video(&[a.clone(), b.clone(), c(Sentido::Frente, 62, 62, 90)], &q(40, 70)), [true, true, true]);
        // passo desconhecido: a de baixo entra (não se afirma que não há quadro entre elas)
        assert_eq!(corridas_no_video(&abd, &[(512 * 40, 512 * 70, 0)]), [true, true, true]);
        // de frente: para trás de 100 até 30 sobre a para a frente desde 0
        let t = c(Sentido::Tras, 100, 30, 100);
        assert_eq!(corridas_no_video(&[a.clone(), t.clone()], &q(50, 50)), [true, true]);
        assert_eq!(corridas_no_video(&[a.clone(), t.clone()], &q(10, 29)), [true, false]);
        // âncora da anterior antes da faixa da nova (não estão de frente): substitui
        assert_eq!(corridas_no_video(&[c(Sentido::Tras, 50, 10, 50), c(Sentido::Frente, 40, 40, 90)], &q(45, 50)), [false, true]);
        // "ambos" da âncora 60: só se tocam no quadro da âncora, sem fusão
        assert_eq!(corridas_no_video(&[c(Sentido::Frente, 60, 60, 150), c(Sentido::Tras, 60, 0, 60)], &q(60, 60)), [false, true]);
        assert_eq!(corridas_no_video(&[a, t], &[]), [true, true]);
    }

    /// Fecho da Fase 2, no PDF paginado: as partes novas dos quadros (as do "Simplificar" sem "perito", a última
    /// marcação mantida, os sem região na ausência aceita), a frase neutra do `ocultar_interpolado`, a ausência aceita
    /// sem "sem ocultação" e a corrida fora dos trechos usados. `SICRO_GUARDAR_PDF=<pasta>` grava o PDF (pdftotext).
    #[test]
    fn pdf_com_as_frases_do_fecho() {
        use crate::edicao::motor::plano::ProvQuadrosOcultos;
        let c = Cena::nova("juri-pdf-fecho");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        let pv = &mut plano.proveniencia;
        pv.rastreios[0].origem = OrigemRastreio::Assistido;
        // uma corrida neural só nos quadros 200..250 (o trecho usa 30..89) e a correlação dentro
        pv.rastreios[0].corridas = vec![valor(&corrida(MotorRastreio::Edgetam, "cuda", Sentido::Frente, 200, 250)), valor(&corrida(MotorRastreio::Mancha, "cpu", Sentido::Frente, 30, 60))];
        pv.ia = crate::ia::uso::da_proveniencia(&pv.rastreios);
        pv.ocultacoes[0].quadros = ProvQuadrosOcultos { ancora: 1, ancora_do_programa: 12, segurado: 30, medido: 40, interpolado: 3, interpolado_do_programa: 3, incerto: 5, ausente: 4, ..Default::default() };
        pv.rastreios[0].ausencias = vec![Ausencia { de_s: 1.5, ate_s: 1.75, motivo: MotivoAusencia::Oculto, aceita_em: "2026-11-14T10:00:00Z".into(), por: "Perito Teste".into() }];
        pv.avisos = vec![crate::edicao::motor::plano::ProvAviso { codigo: codigo::OCULTAR_INTERPOLADO.into(), n: 3 }];
        let t = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        let (_, texto) = pdf_de(&t, ModoPdf::A3);
        let junto = texto.split_whitespace().collect::<Vec<_>>().join(" ");
        for s in [
            "quadros — na marcação do perito: 1 · em ponto escolhido pelo programa entre as amostras: 12 · com a última marcação do perito mantida: 30 · medidos: 40 · interpolados: 3 · dos medidos e interpolados, incertos: 5 · sem região, em ausência aceita pelo perito: 4",
            "ausências do alvo aceitas pelo perito: 0:01,500 a 0:01,750 (alvo encoberto)",
            frases::da_condicao(condicao::OCULTAR_INTERPOLADO_CALCULADO).unwrap(),
            "corridas sem amostra nos trechos usados: 1",
            "correlação, sem inteligência artificial · CPU",
            "Alvos acompanhados por programa a partir de marcação do perito, sem registro do programa usado: 1.",
        ] {
            assert!(junto.contains(s), "falta no PDF: {s}\n{junto}");
        }
        for s in ["entre duas marcações do perito", "trechos sem ocultação", "EdgeTAM vídeo", "Alvos acompanhados por programa de inteligência artificial", "Nenhuma etapa deste vídeo"] {
            assert!(!junto.contains(s), "sobra no PDF: {s}");
        }
    }

    /// Revisão do fecho, no PDF paginado: a ocultação invertida diz o quadro inteiro coberto na ausência aceita; o
    /// EdgeTAM substituído no trecho usado por um "Retomar" da correlação não aparece (nem no resumo, nem no anexo H).
    /// `SICRO_GUARDAR_PDF=<pasta>` grava o PDF (pdftotext).
    #[test]
    fn pdf_invertida_e_corrida_substituida() {
        use crate::edicao::motor::plano::ProvQuadrosOcultos;
        let c = Cena::nova("juri-pdf-revisao");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        let pv = &mut plano.proveniencia;
        (pv.rastreios[0].origem, pv.rastreios[0].confirmacao) = (OrigemRastreio::Assistido, None);
        pv.rastreios[0].corridas = vec![valor(&corrida(MotorRastreio::Edgetam, "cuda", Sentido::Frente, 0, 120)), valor(&corrida(MotorRastreio::Mancha, "cpu", Sentido::Frente, 25, 120))];
        pv.ia = crate::ia::uso::da_proveniencia(&pv.rastreios);
        pv.ocultacoes[0].inverter = true;
        pv.ocultacoes[0].quadros = ProvQuadrosOcultos { ancora: 1, medido: 40, ausente: 4, ..Default::default() };
        let t = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        let (_, texto) = pdf_de(&t, ModoPdf::A3);
        let junto = texto.split_whitespace().collect::<Vec<_>>().join(" ");
        for s in [
            "invertida (cobre tudo o que está fora da forma)",
            "quadros — na marcação do perito: 1 · medidos: 40 · interpolados: 0 · dos medidos e interpolados, incertos: 0 · quadro inteiro coberto, em ausência aceita pelo perito: 4",
            "correlação, sem inteligência artificial · CPU",
            "corridas sem amostra nos trechos usados: 1",
            frases::da_condicao(condicao::IA_NENHUMA).unwrap(),
        ] {
            assert!(junto.contains(s), "falta no PDF: {s}\n{junto}");
        }
        for s in ["sem região, em ausência", "EdgeTAM", "Alvos acompanhados por programa de inteligência artificial"] {
            assert!(!junto.contains(s), "sobra no PDF: {s}");
        }
    }

    /// Fecho da Fase 2: "entre duas marcações do perito" só quando toda interpolação foi entre duas âncoras do perito;
    /// com âncora do "Simplificar" ou linha interpolada do denso, a frase neutra; `feito` sem a contagem não afirma.
    #[test]
    fn ocultar_interpolado_so_entre_marcacoes_do_perito() {
        let c = Cena::nova("juri-interpolado");
        let (plano, res) = apoio::plano_e_resultado(&c);
        let mut base = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        base["feito"]["avisos"] = json!([{ "codigo": codigo::OCULTAR_INTERPOLADO, "n": 6, "ciente": false }]);
        let perito = frases::da_condicao(codigo::OCULTAR_INTERPOLADO).unwrap();
        let neutra = frases::da_condicao(condicao::OCULTAR_INTERPOLADO_CALCULADO).unwrap();
        assert!(perito.contains("marcações do perito") && !neutra.contains("perito"));
        let limites = |q: Value| {
            let mut t = base.clone();
            q.as_object().unwrap().iter().for_each(|(k, v)| t["feito"]["ocultacoes"][0]["quadros"][k] = v.clone());
            let d = dados(&t, &[], &[]).unwrap();
            let l: Vec<String> = serde_json::from_value(d["limitacoes"].clone()).unwrap();
            (l.contains(&perito.to_string()), l.contains(&neutra.to_string()))
        };
        assert_eq!(limites(json!({ "interpolado": 6, "interpolado_entre_marcacoes": 6 })), (true, false));
        assert_eq!(limites(json!({ "interpolado": 6, "interpolado_entre_marcacoes": 4, "interpolado_do_programa": 2 })), (false, true));
        // o resto (2) veio de linha interpolada do denso
        assert_eq!(limites(json!({ "interpolado": 6, "interpolado_entre_marcacoes": 4 })), (false, true));
        // `feito` anterior à contagem (sem `interpolado_entre_marcacoes`): não se afirma que foi o perito
        assert_eq!(limites(json!({ "interpolado": 6 })), (false, true));
    }

    /// Fecho com o P2.1: `feito.ia` pelo `ia::uso::da_proveniencia` das mesmas corridas. O anexo H não repete modelo,
    /// não perde nenhum das corridas, não lista correlação como modelo e sai igual em duas chamadas.
    #[test]
    fn feito_ia_das_corridas_pelo_p2_1() {
        let c = Cena::nova("juri-fecho-ia");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        let pv = &mut plano.proveniencia;
        let (frente, tras) = (corrida(MotorRastreio::Edgetam, "cuda", Sentido::Frente, 30, 60), corrida(MotorRastreio::Edgetam, "cuda", Sentido::Tras, 0, 30));
        pv.rastreios[0].origem = OrigemRastreio::Assistido;
        pv.rastreios[0].corridas = vec![valor(&frente), valor(&tras), valor(&corrida(MotorRastreio::Mancha, "cpu", Sentido::Frente, 60, 89))];
        pv.rastreios.push(prov_rastreio("r2", OrigemRastreio::Assistido, 1, vec![valor(&corrida(MotorRastreio::Vittrack, "cpu", Sentido::Frente, 30, 89))]));
        pv.roteiro[2].processamentos = vec![ProvProcessamento { ordem: 1, nome: "holofote".into(), parametros: json!({ "tipo": "holofote", "id": "e3", "alvo": { "rastreio": "r2" } }) }];
        pv.ia = crate::ia::uso::da_proveniencia(&pv.rastreios);
        assert!(!pv.ia.is_empty(), "o P2.1 não preencheu `feito.ia`");
        let t = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        let d = dados(&t, &[], &[]).unwrap();
        let mut sem = t.clone();
        sem["feito"]["ia"] = json!([]);
        let pelas_corridas = dados(&sem, &[], &[]).unwrap();
        let chaves = |d: &Value| {
            let mut v: Vec<String> = d["modelos_ia"].as_array().unwrap().iter().map(|m| format!("{} | {} | {}", m["modelo"], m["execucao"], m["arquivos"])).collect();
            v.sort();
            v
        };
        let (do_p21, das_corridas) = (chaves(&d), chaves(&pelas_corridas));
        assert_eq!(do_p21, das_corridas, "o anexo H pelo `feito.ia` do P2.1 e pelas corridas");
        let mut unicos = do_p21.clone();
        unicos.dedup();
        assert_eq!(unicos, do_p21, "modelo repetido");
        assert!(d["modelos_ia"].as_array().unwrap().iter().all(|m| !m["arquivos"].as_array().unwrap().is_empty()), "correlação listada como modelo: {}", d["modelos_ia"]);
        assert_eq!(serde_json::to_vec(&d).unwrap(), serde_json::to_vec(&dados(&t, &[], &[]).unwrap()).unwrap());
    }

    /// No PDF paginado: o texto das telas de texto, cada marcação com a forma, o rótulo e a cor, a linha legível do
    /// anexo C, o ritmo para leigo, a marca do SICRO; e nada passa da margem, nem com nome longo sem pausa.
    #[test]
    fn pdf_com_telas_marcacoes_e_identidade() {
        let c = Cena::nova("juri-pdf-marcas");
        let mut t = tecnico_completo(&c);
        let (projeto, mapa) = complementos(&mut t);
        let longo = "Camera_portaria_entrada_principal_gravacao_original_do_DVR_canal_01_2026-11-14_10h00m00s_ate_10h59m59s.mp4";
        t["feito"]["midias"][0]["nome"] = json!(longo);
        let d = dados_com(&t, &miniaturas(), &pacote(), Complementos { projeto: Some(&projeto), mapa: Some(&mapa) }).unwrap();
        let e = entrada(&t, d.clone(), &miniaturas(), &projeto, ModoPdf::A3).unwrap();
        let texto = relatorio_pdf::texto(&e).unwrap();
        let tem = |s: &str| assert!(texto.contains(s), "falta no PDF: {s}\n{texto}");
        // telas de texto, na ordem título, subtítulo, corpo (vazio: fora), rodapé
        assert_eq!(
            d["roteiro"][0]["textos"],
            json!([{ "papel": "Título", "texto": "Chegada do veículo\nà portaria" }, { "papel": "Subtítulo", "texto": "Câmera 1" }, { "papel": "Rodapé", "texto": "Perícia 0000/2026" }])
        );
        tem("TÍTULO Chegada do veículo à portaria SUBTÍTULO Câmera 1 RODAPÉ Perícia 0000/2026");
        // marcações: forma, quantidade, rótulo e cor (com a amostra), e a seção 5 com as formas
        assert_eq!(
            d["roteiro"][1]["efeitos"].as_array().unwrap().iter().filter(|e| !e["marcas"].as_array().unwrap().is_empty()).cloned().collect::<Vec<_>>(),
            [json!({ "texto": "1 elipse", "marcas": [{ "texto": "P1 (vermelha)", "cor": "#ed333b" }] }), json!({ "texto": "1 seta", "marcas": [{ "texto": "sem rótulo (amarela)", "cor": "#ffd84a" }] })]
        );
        tem("1 elipse: P1 (vermelha); ");
        tem("1 seta: sem rótulo (amarela)");
        tem("Marcação (elipses e setas)");
        // anexo C: uma linha legível por processamento, sem JSON
        tem("Elipse vermelha (#ed333b), espessura 0,4 %, com contorno · margem 5 % · rótulo «P1» acima, letra 3 %, branca (#ffffff), negrito · segue o rastreio R1");
        tem("Seta amarela (#ffd84a), espessura 0,2 %, com contorno · tamanho 2 % · direção 90° · sem rótulo · segue o rastreio R1");
        assert!(!d["processamentos"].to_string().contains("\\\"traco\\\""), "JSON cru no anexo C");
        // ritmo e formato para leigo; o nome do leitor do ffmpeg só no anexo B
        tem("30 imagens por segundo, constante");
        tem("MP4 (H.264) · 123 MB");
        tem("percentis 10, 50 e 90: 33,333 · 33,333 · 33,333 ms (57 intervalos nos trechos usados)");
        // a marca do programa
        tem("Gerado pelo SICRO 6.0.0");
        let s = relatorio_pdf::gerar(&e).unwrap();
        assert!(s.avisos.is_empty() && relatorio_pdf::testes::contem(&s.pdf, b"<pdfaid:part>3</pdfaid:part>"), "{:?}", s.avisos);
        let fora = relatorio_pdf::testes::transbordos(&e);
        assert_eq!(fora, vec![], "texto além da margem");
        // projeto que não é o do `feito`: sem o texto das telas
        let outro = dados_com(&t, &[], &[], Complementos { projeto: Some(b"{\"principal\":[]}"), mapa: None }).unwrap();
        assert_eq!(outro["roteiro"][0]["textos"], json!([]));
    }

    /// Ferramenta, não teste: o relatório de uma exportação de verdade, sem tocar nela. `SICRO_RELATORIO_DE` = pasta
    /// com `*.tecnico.json`, `SHA256SUMS.txt`, `mapa.csv` e `projeto/projeto.json`; `SICRO_RELATORIO_IMG` = pasta com
    /// as miniaturas `N.jpg` (opcional); `SICRO_GUARDAR_PDF` = onde gravar `Relatório.pdf` e `dados.json`.
    #[test]
    #[ignore = "ferramenta: relatório a partir de uma pasta de exportação"]
    fn relatorio_de_uma_pasta() {
        use std::path::PathBuf;
        let de = PathBuf::from(std::env::var("SICRO_RELATORIO_DE").expect("SICRO_RELATORIO_DE"));
        let para = PathBuf::from(std::env::var("SICRO_GUARDAR_PDF").expect("SICRO_GUARDAR_PDF"));
        let tec = std::fs::read_dir(&de).unwrap().filter_map(|x| x.ok()).map(|x| x.path()).find(|p| p.to_string_lossy().ends_with(".tecnico.json")).expect("*.tecnico.json");
        let t: Value = serde_json::from_slice(&std::fs::read(tec).unwrap()).unwrap();
        let somas = std::fs::read_to_string(de.join("SHA256SUMS.txt")).unwrap_or_default();
        let mut arquivos: Vec<ArquivoDoPacote> = somas.lines().filter_map(|l| l.split_once("  ")).map(|(s, r)| ArquivoDoPacote { rel: r.into(), sha256: s.into() }).collect();
        if !somas.is_empty() {
            arquivos.push(ArquivoDoPacote { rel: "SHA256SUMS.txt".into(), sha256: format!("{:x}", Sha256::digest(somas.as_bytes())) });
        }
        let projeto = std::fs::read(de.join("projeto/projeto.json")).unwrap_or_default();
        let mapa = std::fs::read_to_string(de.join("mapa.csv")).ok();
        let mut mini: Vec<Miniatura> = Vec::new();
        if let Ok(img) = std::env::var("SICRO_RELATORIO_IMG") {
            for x in std::fs::read_dir(img).unwrap().filter_map(|x| x.ok()) {
                if let Some(n) = x.file_name().to_string_lossy().strip_suffix(".jpg").and_then(|n| n.parse::<u32>().ok()) {
                    mini.push(Miniatura { n, jpeg: std::fs::read(x.path()).unwrap() });
                }
            }
        }
        mini.sort_by_key(|m| m.n);
        let d = dados_com(&t, &mini, &arquivos, Complementos { projeto: Some(&projeto), mapa: mapa.as_deref() }).unwrap();
        std::fs::write(para.join("dados.json"), serde_json::to_vec_pretty(&d).unwrap()).unwrap();
        let e = entrada(&t, d, &mini, &projeto, ModoPdf::A3).unwrap();
        let s = relatorio_pdf::gerar(&e).unwrap();
        std::fs::write(para.join("Relatório.pdf"), &s.pdf).unwrap();
        println!("páginas: {} · avisos: {:?}", s.paginas, s.avisos);
        let fora = relatorio_pdf::testes::transbordos(&e);
        assert_eq!(fora, vec![], "texto além da margem");
    }

    #[test]
    fn formatacao_pt_br() {
        assert_eq!((tempo(7.5), tempo(0.0), tempo(62.0004), tempo(3723.5), tempo(-1.0)), ("0:07,500".into(), "0:00,000".into(), "1:02,000".into(), "1:02:03,500".into(), "−0:01,000".into()));
        assert_eq!((duracao(47.0), duracao(37.5), duracao(723.0), duracao(3723.4), duracao(59.96)), ("47 s".into(), "37,5 s".into(), "12 min 03 s".into(), "1 h 02 min 03 s".into(), "60 s".into()));
        assert_eq!((tamanho(5_129_295), tamanho(117_085), tamanho(512), tamanho(123_456_789_012)), ("5,1 MB (5.129.295 bytes)".into(), "117 kB (117.085 bytes)".into(), "512 bytes".into(), "123 GB (123.456.789.012 bytes)".into()));
        // 23h40 UTC = 20h40 em Brasília, no mesmo dia; 01h30 UTC cai no dia anterior
        assert_eq!(data("2026-11-13T23:40:12Z"), "13 de novembro de 2026, às 20h40 (horário de Brasília)");
        assert_eq!(data("2026-03-01T01:30:00Z"), "28 de fevereiro de 2026, às 22h30 (horário de Brasília)");
        assert_eq!(data("x"), "x");
        assert_eq!((taxa(30, 1), taxa(30000, 1001), taxa(25, 1), taxa(0, 0)), ("30".into(), "29,97".into(), "25".into(), "—".into()));
        assert_eq!((letra(0), letra(25), letra(26), letra(27), letra(701), letra(702)), ("A".into(), "Z".into(), "AA".into(), "AB".into(), "ZZ".into(), "AAA".into()));
        assert_eq!((num(2.0, 2), num(0.5, 3), num(-0.0, 1), num(45.55, 1), dec(2.0, 2)), ("2".into(), "0,5".into(), "0".into(), "45,5".into(), "2,00".into()));
        assert_eq!((onde(&[3]), onde(&[2, 5])), ("item 3".into(), "itens 2, 5".into()));
        assert_eq!((por_segundo_txt(15.15), por_segundo_txt(29.97), por_segundo_txt(7.5), por_segundo_txt(1.0)), ("15".into(), "30".into(), "7,5".into(), "1".into()));
    }

    /// Formato com o nome que se reconhece fora do ffmpeg; o leitor do ffmpeg fica no anexo B.
    #[test]
    fn formato_sem_nome_do_ffmpeg() {
        let casos = [
            ("image2pipe", "png", "Untitled-1_.png", "PNG"),
            ("image2", "mjpeg", "foto.jpg", "JPEG"),
            ("png_pipe", "", "x.png", "PNG"),
            ("mov,mp4,m4a,3gp,3g2,mj2", "h264", "cam 1.mp4", "MP4 (H.264)"),
            ("mov,mp4,m4a,3gp,3g2,mj2", "hevc", "IMG_0001.MOV", "MOV (H.265)"),
            ("matroska,webm", "vp9", "a.webm", "WebM (VP9)"),
            ("matroska,webm", "h264", "a.mkv", "MKV (H.264)"),
            ("dhav", "h264", "canal1.dav", "DAV (H.264)"),
            ("h264", "h264", "bruto.264", "H.264"),
            ("asf", "wmv3", "x.wmv", "WMV"),
            ("wav", "pcm_s16le", "voz.wav", "WAV (PCM)"),
            ("lavfi_desconhecido", "rawvideo", "x.yuv", "YUV (sem compressão)"),
            ("", "", "", "—"),
        ];
        for (c, k, n, esperado) in casos {
            assert_eq!(formato(c, k, n), esperado, "{c} {k} {n}");
        }
    }

    /// Intervalos dos trechos usados pelo `mapa.csv`: só entre quadros de índice seguido, por gravação.
    #[test]
    fn intervalos_pelo_mapa() {
        let csv = "q,item,clipe,midia,pts,indice,t_s,tipo\n0,1,c1,,,,,\n1,2,c2,m1,100,10,0.1,novo\n2,2,c2,m1,100,10,0.1,repetido\n3,2,c2,m1,133,11,0.133,novo\n\
                   4,2,c2,m1,200,13,0.2,novo\n5,3,\"c,3\",m2,0,0,0,novo\n6,3,\"c,3\",m2,40,1,0.04,novo\n,2,c2,m1,166,12,0.166,descartado\nlixo\n";
        let m = intervalos_do_mapa(csv);
        assert_eq!((m["m1"].as_slice(), m["m2"].as_slice()), ([33, 33, 34].as_slice(), [40].as_slice()));
        assert_eq!(campos_csv("a,\"b,\"\"c\"\"\",d"), ["a", "b,\"c\"", "d"]);
        assert_eq!((percentil(&[1, 2, 3, 4, 5], 0.0), percentil(&[1, 2, 3, 4, 5], 0.5), percentil(&[1, 2, 3, 4, 5], 1.0)), (1, 3, 5));
    }

    /// Ritmo da gravação pelos passos medidos: constante, variável (a mediana e a faixa típica) e lacuna.
    #[test]
    fn ritmo_e_intervalo_da_gravacao() {
        let com_passos = |min: i64, med: i64, max: i64, lacunas: Value, passos: Option<&[i64]>| -> Original {
            let m: FMidia = serde_json::from_value(json!({ "id": "m1", "nome": "cam.mp4", "medido": { "tb": { "num": 1, "den": 1000 }, "de_s": 0.0, "ate_s": 20.0,
                "quadros": 500, "passo_min": min, "passo_med": med, "passo_max": max, "lacunas": lacunas }, "usado": [{ "de_s": 3.0, "ate_s": 9.0 }], "usado_total_s": 6.0 })).unwrap();
            original(1, &m, passos)
        };
        let midia = |min: i64, med: i64, max: i64, lacunas: Value| com_passos(min, med, max, lacunas, None);
        let o = midia(40, 40, 40, json!([]));
        assert_eq!((o.id.as_str(), o.ritmo.as_str(), o.taxa_variavel, o.tecnico.intervalo_quadros.as_str()), ("B", "25 imagens por segundo, constante", false, "40,000 ms (mín. = máx.)"));
        let c = o.cobertura.unwrap();
        assert_eq!((c.usado.as_str(), c.total.as_str(), c.intervalos), ("6 s", "20 s", vec!["0:03,000 a 0:09,000".to_string()]));
        // oscilação de ±4 ms em 40 ms: constante
        assert!(!midia(36, 40, 44, json!([])).taxa_variavel);
        let o = midia(40, 40, 83, json!([]));
        assert_eq!((o.ritmo.as_str(), o.taxa_variavel), ("cerca de 25 imagens por segundo, em intervalos irregulares", true));
        assert_eq!((o.tecnico.intervalo_quadros.as_str(), &o.tecnico.intervalos_usados), ("40,000 a 83,000 ms (mediana 40,000)", &None));
        // a câmera que alterna 30 e 15 imagens por segundo (o 1 tique e o buraco de 1 s ficam fora da faixa típica)
        let mut passos: Vec<i64> = [vec![1, 1000], vec![33; 45], vec![66; 55]].concat();
        passos.sort_unstable();
        let o = com_passos(1, 66, 1000, json!([[0, 1000]]), Some(&passos));
        assert_eq!(o.ritmo, "cerca de 15 imagens por segundo, em intervalos irregulares (na maior parte, entre 15 e 30 por segundo)");
        assert_eq!(o.tecnico.intervalos_usados.as_deref(), Some("percentis 10, 50 e 90: 33,000 · 66,000 · 66,000 ms (102 intervalos nos trechos usados)"));
        // faixa que dá o mesmo número nas duas pontas não se escreve; poucos intervalos não fazem percentil
        assert_eq!(com_passos(1, 66, 1000, json!([]), Some(&[66; 30])).ritmo, "cerca de 15 imagens por segundo, em intervalos irregulares");
        assert_eq!(com_passos(1, 66, 1000, json!([]), Some(&[33, 66, 66])).tecnico.intervalos_usados, None);
        assert!(midia(40, 40, 40, json!([[400, 4000]])).taxa_variavel, "lacuna");
        assert_eq!((o.recebido.as_str(), o.relogio.as_str(), o.formato_nativo), ("não informado", "sem informação", None));
        // som (narração) e foto: sem tabela de quadros não há duração nem parte usada a declarar (nunca "0 s")
        for (quadros, especie, ritmo) in [(0, "Som", "só som"), (1, "Imagem", "imagem única")] {
            let m: FMidia = serde_json::from_value(json!({ "id": "m2", "nome": "x", "medido": { "tb": { "num": 1, "den": 1000 }, "de_s": 0.0, "ate_s": 0.0, "quadros": quadros } })).unwrap();
            let o = original(0, &m, Some(&[40; 30]));
            assert_eq!((o.especie.as_str(), o.ritmo.as_str(), o.duracao.as_str(), o.cobertura.is_none(), o.taxa_variavel), (especie, ritmo, "—", true, false));
            assert_eq!(o.tecnico.intervalos_usados, None);
        }
    }

    /// Texto do PDF numa linha só (a quebra de linha do leiaute vira espaço).
    fn plano_do_pdf(e: &relatorio_pdf::Entrada) -> String {
        relatorio_pdf::texto(e).unwrap().split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Zoom que acompanha o rastreio (ZS; contratos-fase4 D9 e (f) P4.R), pelo `feito` que o motor grava
    /// (`parametros.segue` e o rastreio em `feito.rastreios`): o item e as mudanças dizem "Ampliação que acompanha o
    /// alvo", o anexo C diz "acompanhando o rastreio R1 (média de 1,5 s)" e o R1 do anexo H tem o uso do item; alvo de
    /// detecção conta à parte das marcações no resumo de IA; região inteira não é ampliação; sem média, diz.
    #[test]
    fn zoom_que_segue() {
        let c = Cena::nova("juri-segue");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        let pv = &mut plano.proveniencia;
        // o r1 só no zoom (a ocultação dele sai)
        pv.ocultacoes.clear();
        pv.rastreios[0].origem = OrigemRastreio::Deteccao { classe: ClasseDeteccao::Pessoa };
        let enq = |regiao: Value, suavizar: f64| ProvProcessamento {
            ordem: 1,
            nome: "enquadramento".into(),
            parametros: json!({ "tipo": "enquadramento", "id": "q1", "ligado": true, "nome": "", "ativo": null, "regiao": regiao, "ajuste": "cobrir",
                                "interpolacao": "bilinear", "segue": { "rastreio": "r1", "suavizar_s": suavizar } }),
        };
        let metade = json!({ "x": 0.25, "y": 0.25, "w": 0.5, "h": 0.5 });
        pv.roteiro[1].processamentos = vec![enq(metade.clone(), 1.5)];
        let base = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        let d = dados(&base, &[], &[]).unwrap();
        let rotulo = frases::rotulo_do_efeito(tipo::AMPLIACAO_QUE_SEGUE).unwrap();
        assert_eq!(rotulo, "Ampliação que acompanha o alvo");
        let efeitos: Vec<&str> = d["roteiro"][1]["efeitos"].as_array().unwrap().iter().map(|e| e["texto"].as_str().unwrap()).collect();
        assert_eq!(efeitos, [rotulo]);
        let linha = d["mudancas"].as_array().unwrap().iter().find(|l| l["o_que"] == rotulo).expect("nas mudanças");
        assert_eq!((linha["onde"].as_str(), linha["explicacao"].as_str()), (Some("item 2"), frases::do_efeito(tipo::AMPLIACAO_QUE_SEGUE)));
        let proc_ = d["processamentos"].as_array().unwrap().iter().find(|p| p["item"] == "2" && p["ordem"] == "1").unwrap();
        assert_eq!(proc_["nome"], json!(rotulo));
        let parametros = proc_["parametros"].as_str().unwrap();
        assert!(parametros.ends_with("região fixa: x 25\u{a0}%, y 25\u{a0}%, largura 50\u{a0}%, altura 50\u{a0}% · acompanhando o rastreio R1 (média de 1,5 s)"), "{parametros}");
        // o R1 do anexo H: o rastreio da detecção, usado no item 2 pelo zoom
        assert_eq!((d["rastreios"][0]["id"].as_str(), d["rastreios"][0]["uso"].as_str()), (Some("R1"), Some("item 2")));
        let ia: Vec<String> = serde_json::from_value(d["ia_resumo"].clone()).unwrap();
        assert!(ia.contains(&"Ampliações que acompanham alvo achado por detecção automática e confirmado pelo perito: 1.".to_string()), "{ia:?}");
        assert!(!ia.iter().any(|l| l.starts_with("Marcações")), "zoom não é marcação: {ia:?}");
        // no PDF
        let e = entrada(&base, d, &[], b"{}", ModoPdf::A3).unwrap();
        let texto = plano_do_pdf(&e);
        for s in ["acompanhando o rastreio R1 (média de 1,5 s)", "Ampliação que acompanha o alvo", "Ampliações que acompanham alvo achado por detecção automática"] {
            assert!(texto.contains(s), "falta no PDF: {s}");
        }
        // região inteira: nada é ampliado nem se desloca (fica o "Preencher"); o anexo C segue dizendo o rastreio
        plano.proveniencia.roteiro[1].processamentos = vec![enq(json!({ "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 }), 0.0)];
        let d = dados(&montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap(), &[], &[]).unwrap();
        assert_eq!(d["roteiro"][1]["efeitos"][0]["texto"], json!(frases::rotulo_do_efeito(tipo::PREENCHER).unwrap()));
        assert!(d["processamentos"].to_string().contains("acompanhando o rastreio R1, sem média"), "{}", d["processamentos"]);
        // rastreio do perito, fora do anexo H: o anexo C diz a origem e as marcações do perito
        plano.proveniencia.roteiro[1].processamentos = vec![enq(metade, 1.0)];
        (plano.proveniencia.rastreios[0].origem, plano.proveniencia.rastreios[0].corridas) = (OrigemRastreio::Manual, vec![]);
        let d = dados(&montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap(), &[], &[]).unwrap();
        assert_eq!(d["rastreios"], json!([]));
        assert!(d["processamentos"].to_string().contains("acompanhando o rastreio r1 (marcação do perito · marcações do perito: 2) (média de 1 s)"), "{}", d["processamentos"]);
        assert!(!d["ia_resumo"].to_string().contains("Ampliações"));
    }

    /// Texto livre (TL; (f) P4.R): entrada e saída quando não são o padrão ("aparecer" nos dois lados), lidas do
    /// `projeto.json` que o `feito` registra; uma linha de acréscimo por texto do projeto; sombra e faixa no anexo C.
    #[test]
    fn texto_livre_entrada_saida() {
        use crate::edicao::modelo::ModeloTitulo;
        use crate::edicao::motor::plano::{ProvNaSaida, ProvSobreposto};
        let c = Cena::nova("juri-texto");
        let (mut plano, res) = apoio::plano_e_resultado(&c);
        let fps = plano.proveniencia.saida.fps;
        let s = |q: u64| crate::edicao::tempo::instante_do_slot(q, fps);
        let sob = |id: &str, texto: &str, de_q: u64, n_q: u32| ProvSobreposto {
            trilha: "t1".into(),
            clipe: id.into(),
            tipo: "titulo".into(),
            nome: String::new(),
            na_saida: ProvNaSaida { de_q, n_q, de_s: s(de_q), ate_s: s(de_q + n_q as u64) },
            midia: None,
            preso_a: Some("c2".into()),
            modelo: Some(ModeloTitulo::Livre),
            textos: vec![texto.into()],
            selos: vec![],
        };
        plano.proveniencia.sobrepostos =
            vec![sob("s1", "Portão", 30, 30), sob("s2", "Vítima", 40, 20), sob("s3", "Carro", 50, 30), sob("s4", "Moto", 55, 10), sob("s5", "Placa", 60, 10)];
        plano.proveniencia.roteiro[1].processamentos = vec![ProvProcessamento { ordem: 1, nome: "texto".into(), parametros: json!({ "tipo": "texto", "id": "e1",
            "conteudo": "Portão", "lugar": { "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" }, "largura_max": 0.6,
            "estilo": { "tamanho": 0.052, "cor": "#ffffff", "caixa": { "cor": "#000000a0", "margem": 0.012, "raio": 0.0, "faixa": true },
                        "sombra": { "cor": "#000000b3", "dx": 0.0, "dy": 0.003, "desfoque": 0.004 } } }) }];
        let clipe = |id: &str, e: f64, sa: f64, animacao: Value| {
            let mut v = json!({ "tipo": "titulo", "id": id, "entrada_s": e, "saida_s": sa });
            if !animacao.is_null() {
                v["animacao"] = animacao;
            }
            v
        };
        let anima = |e: &str, s: &str, lado: &str| json!({ "entrada": e, "saida": s, "lado": lado });
        let projeto = serde_json::to_vec(&json!({ "sobreposicoes": [{ "id": "t1", "clipes": [
            clipe("s1", 0.3, 0.3, anima("deslizar", "corte", "baixo")),
            clipe("s2", 0.3, 0.3, Value::Null),
            clipe("s3", 0.5, 0.25, anima("aparecer", "deslizar", "esq")),
            clipe("s4", 0.0, 0.3, anima("deslizar", "aparecer", "dir")),
        ] }, { "id": "t2", "clipes": [clipe("s5", 0.3, 0.3, anima("aparecer", "aparecer", "cima"))] }] }))
        .unwrap();
        let sha = format!("{:x}", Sha256::digest(&projeto));
        let t = montar(&plano, &res, &c.ambiente("t1"), &Fecho { projeto_sha256: sha, ..fecho() }).unwrap();
        let extra = Complementos { projeto: Some(&projeto), mapa: None };
        let d = dados_com(&t, &[], &[], extra).unwrap();
        let base = frases::do_efeito(tipo::TEXTO_SOBREPOSTO).unwrap();
        let rotulo = frases::rotulo_do_efeito(tipo::TEXTO_SOBREPOSTO).unwrap();
        let linhas: Vec<(String, String)> = d["acrescimos"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| l["o_que"].as_str().unwrap().starts_with(rotulo))
            .map(|l| (l["o_que"].as_str().unwrap().to_string(), l["explicacao"].as_str().unwrap().to_string()))
            .collect();
        // uma linha por texto que sai no vídeo (os cinco do `feito`)
        assert_eq!(linhas.len(), plano.proveniencia.sobrepostos.len());
        let esperado = [
            ("Portão", "Entra deslizando de baixo em 0,3 s e some de uma vez."),
            ("Vítima", ""),
            ("Carro", "Aparece aos poucos em 0,5 s e sai deslizando para a esquerda em 0,25 s."),
            // deslizar sem duração = corte (`sobreposto::transformacao_em`)
            ("Moto", "Aparece de uma vez e some aos poucos em 0,3 s."),
            // "aparecer" nos dois lados é o padrão, com o campo gravado ou não
            ("Placa", ""),
        ];
        for ((o_que, explicacao), (texto, frase)) in linhas.iter().zip(esperado) {
            assert_eq!(o_que, &format!("{rotulo}: {texto}"));
            let todo = if frase.is_empty() { base.to_string() } else { format!("{base} {frase}") };
            assert_eq!(explicacao, &todo, "{texto}");
        }
        // sombra e faixa do texto no anexo C
        let p = d["processamentos"].as_array().unwrap().iter().find(|p| p["item"] == "2" && p["ordem"] == "1").unwrap()["parametros"].as_str().unwrap().replace('\u{a0}', " ");
        assert!(p.contains("faixa preta (#000000), sombra preta (#000000) desfocada 0,4 %"), "{p}");
        // no PDF
        let e = entrada(&t, d, &[], &projeto, ModoPdf::A3).unwrap();
        let texto = plano_do_pdf(&e);
        for s in ["Entra deslizando de baixo em 0,3 s e some de uma vez.", "sai deslizando para a esquerda em 0,25 s."] {
            assert!(texto.contains(s), "falta no PDF: {s}");
        }
        // sem o projeto, ou com outro projeto (SHA-256 diferente do `feito`): nada sobre entrada e saída
        for extra in [Complementos::default(), Complementos { projeto: Some(b"{}"), mapa: None }] {
            let d = dados_com(&t, &[], &[], extra).unwrap();
            assert!(!d["acrescimos"].to_string().contains("deslizando"), "{}", d["acrescimos"]);
        }
    }
}
