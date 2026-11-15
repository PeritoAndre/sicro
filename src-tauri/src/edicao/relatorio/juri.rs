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

use chrono::{DateTime, Datelike, FixedOffset, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::frases::{self, condicao, modelo, tipo, Classe};
use super::tecnico;
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::ModoPdf;
use crate::edicao::rastreio::{Corrida, MotorRastreio};
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
/// Teto do texto de parâmetros de um processamento no anexo C (o inteiro está no `tecnico.json` anexado).
const MAX_PARAMETROS: usize = 600;
const MAX_VALOR: usize = 160;
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
    com_perdas: bool,
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

/// `feito.rastreios[]`. `ancoras` sem o campo = não se afirma quantas; `corridas` lê como `rastreio::Corrida` (B2).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FRastreio {
    id: String,
    origem: Value,
    ancoras: Option<u32>,
    denso_sha256: Option<String>,
    corridas: Vec<Value>,
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
    efeitos: Vec<String>,
    selos: Vec<String>,
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
fn dec(v: f64, casas: usize) -> String {
    format!("{v:.casas$}").replace('.', ",").replace('-', "−")
}

/// Número com até `casas`, sem zeros sobrando.
fn num(v: f64, casas: usize) -> String {
    let s = format!("{v:.casas$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { &s };
    let s = if s == "-0" { "0" } else { s };
    s.replace('.', ",").replace('-', "−")
}

/// "0:07,500", "1:02:03,500".
fn tempo(s: f64) -> String {
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

fn rot(chave: &str) -> &'static str {
    frases::rotulo(chave).unwrap_or("—")
}

/// Rótulo do valor do grupo; fora do catálogo, o próprio valor.
fn rot_ou(grupo: &str, valor: &str) -> String {
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

/// Parâmetros de um processamento em texto: `chave: valor; …`, sem o que só identifica o item.
fn parametros(v: &Value) -> String {
    let Some(o) = v.as_object() else { return "—".into() };
    let partes: Vec<String> = o
        .iter()
        .filter(|(k, _)| !matches!(k.as_str(), "tipo" | "id" | "ligado" | "nome"))
        .map(|(k, v)| {
            let valor = match v {
                Value::Null => "—".to_string(),
                Value::String(s) => s.clone(),
                outro => outro.to_string(),
            };
            format!("{k}: {}", cortar(valor, MAX_VALOR))
        })
        .collect();
    if partes.is_empty() {
        "—".into()
    } else {
        cortar(partes.join("; "), MAX_PARAMETROS)
    }
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

fn formato(conteiner: &str, codec: &str) -> String {
    let c = conteiner.to_ascii_lowercase();
    let cont = if c.contains("mp4") {
        "MP4"
    } else if c.contains("matroska") {
        "MKV"
    } else if c.contains("avi") {
        "AVI"
    } else if c.contains("mpegts") {
        "MPEG-TS"
    } else if c.contains("mpeg") {
        "MPEG"
    } else {
        conteiner
    };
    let cod = match codec.to_ascii_lowercase().as_str() {
        "h264" => "H.264".to_string(),
        "hevc" | "h265" => "H.265".to_string(),
        "mpeg4" => "MPEG-4".to_string(),
        "mjpeg" => "MJPEG".to_string(),
        "ffv1" => "FFV1".to_string(),
        "" => String::new(),
        _ => codec.to_string(),
    };
    match (cont.is_empty(), cod.is_empty()) {
        (true, true) => "—".into(),
        (false, true) => cont.to_string(),
        (true, false) => cod,
        (false, false) => format!("{cont} ({cod})"),
    }
}

fn original(i: usize, m: &FMidia) -> Original {
    let med = &m.medido;
    let especie = match med.quadros {
        0 => "especie.som",
        1 => "especie.imagem",
        _ => "especie.video",
    };
    let variavel = med.quadros > 1 && ritmo_variavel(med);
    let ritmo = match (med.quadros, por_segundo(med.passo_med, &med.tb)) {
        (0, _) => rot("ritmo.som").to_string(),
        (1, _) | (_, None) => rot("ritmo.imagem").to_string(),
        (_, Some(_)) if variavel => {
            let (de, ate) = (por_segundo(med.passo_max, &med.tb).unwrap_or(0.0), por_segundo(med.passo_min, &med.tb).unwrap_or(0.0));
            frases::preencher(rot("ritmo.variavel"), &[("de", &num(de, 2)), ("ate", &num(ate, 2))])
        }
        (_, Some(f)) => frases::preencher(rot("ritmo.constante"), &[("fps", &num(f, 2))]),
    };
    let mut recebido = rot(match m.recebido.nativo {
        Some(true) => "recebido.nativo",
        Some(false) => "recebido.convertido",
        None => "recebido.nao_informado",
    })
    .to_string();
    if !m.recebido.obs.trim().is_empty() {
        recebido = format!("{recebido} ({})", m.recebido.obs.trim());
    }
    let mut relogio = match m.relogio_desvio_s {
        Some(d) => {
            let sinal = if d < 0.0 { "−" } else { "+" };
            frases::preencher(rot("relogio.desvio"), &[("d", &format!("{sinal}{}", duracao(d.abs())))])
        }
        None => rot("relogio.sem").to_string(),
    };
    if let Some(g) = m.gravador.as_deref().filter(|g| !g.trim().is_empty()) {
        relogio = format!("{relogio} · {g}");
    }
    let derivacao = m.derivacao.as_ref().map(|d| {
        let como = format!("{}, {}", rot_ou("derivacao", &d.tipo), rot(if d.com_perdas { "derivacao.com_perdas" } else { "derivacao.sem_perdas" }));
        Derivada { texto: frases::preencher(modelo::DERIVADA, &[("como", &como)]), origem_sha256: d.raiz_sha256.clone() }
    });
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
        formato: formato(&m.conteiner, &m.codec),
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
        },
    }
}

/// Fator da velocidade constante; None = rampa ou ausente.
fn fator(v: &Option<Value>) -> Option<f64> {
    v.as_ref().filter(|v| v["tipo"] == "constante").and_then(|v| v["fator"].as_f64())
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
        None if v.is_object() => rot("velocidade.variavel").to_string(),
        None => return None,
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

/// Id do rastreio que o processamento usa.
fn rastreio_do_proc(p: &FProc) -> Option<&str> {
    p.parametros["rastreio"].as_str().or_else(|| p.parametros["alvo"]["rastreio"].as_str())
}

fn automatico(r: &FRastreio) -> bool {
    matches!(r.origem["tipo"].as_str(), Some("deteccao") | Some("pessoas"))
}

/// Corridas que leem como `rastreio::Corrida` (B2) e quantas não leem (gravadas antes da Fase 2): destas nada se afirma.
fn corridas_de(r: &FRastreio) -> (Vec<Corrida>, usize) {
    let lidas: Vec<Corrida> = r.corridas.iter().filter_map(|v| Corrida::deserialize(v).ok()).collect();
    let ilegiveis = r.corridas.len() - lidas.len();
    (lidas, ilegiveis)
}

fn neural(c: &Corrida) -> bool {
    matches!(c.motor.motor, MotorRastreio::Edgetam | MotorRastreio::Vittrack)
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

/// `dados.json` do modelo, com todos os valores já em texto pronto para imprimir.
pub fn dados(tecnico: &Value, miniaturas: &[Miniatura], arquivos: &[ArquivoDoPacote]) -> Result<Value> {
    if tecnico.get("esquema").and_then(Value::as_str) != Some(tecnico::ESQUEMA_TECNICO) {
        return Err(SicroError::Validation("relatorio: tecnico.json de outro esquema".into()));
    }
    let t: Tecnico = serde_json::from_value(tecnico.clone())?;
    let (f, e) = (&t.feito, &t.execucao);
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
        formato: format!("{}, {}", formato(if ext == "mkv" { "matroska" } else { &ext }, codec_saida), rot(if com_som { "som.com" } else { "som.sem" })),
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
    for it in &f.roteiro {
        let mut efeitos: Vec<String> = Vec::new();
        for p in &it.processamentos {
            let Some(t) = tipo_do_proc(p) else { continue };
            let r = rotulo_do_proc(p, t);
            usar(t, r.clone(), it.n);
            if !efeitos.contains(&r) {
                efeitos.push(r);
            }
        }
        // ocultação da MÍDIA (D17, o padrão da tela) não está nos processamentos do clipe: muda a imagem do item do
        // mesmo jeito (sem isto o relatório dizia "mostradas como foram registradas" com um rosto coberto)
        for o in f.ocultacoes.iter().filter(|o| o.clipes.iter().any(|c| *c == it.clipe)) {
            let t = tipo_da_ocultacao(&o.modo);
            let r = format!("{} ({})", frases::rotulo_do_efeito(t).unwrap_or(""), rot_ou("conteudo", &o.conteudo));
            usar(t, r.clone(), it.n);
            if !efeitos.contains(&r) {
                efeitos.push(r);
            }
        }
        for x in extras_do_item(it, com_som) {
            let r = frases::rotulo_do_efeito(x).unwrap_or(x).to_string();
            usar(x, r.clone(), it.n);
            // o tipo do item e a velocidade já estão escritos no bloco
            if !matches!(x, tipo::CONGELAR | tipo::TITULO | tipo::IMAGEM_CLIPE | tipo::CAMERA_LENTA | tipo::ACELERADO) {
                efeitos.push(r);
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
        acrescimos.push(Linha { o_que, onde, explicacao: frases::do_efeito(t).unwrap_or("").to_string() });
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
    for (t, rotulo, itens) in &usos {
        let linha = Linha { o_que: rotulo.clone(), onde: onde(itens), explicacao: frases::do_efeito(t).unwrap_or("").to_string() };
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
    let procs = || f.roteiro.iter().flat_map(|it| it.processamentos.iter().map(move |p| (it, p)));
    let mut marcados: Vec<&str> =
        procs().filter(|(_, p)| p.nome != "ocultar").filter_map(|(_, p)| rastreio_do_proc(p)).filter(|id| rastreio(id).is_some_and(automatico)).collect();
    marcados.sort_unstable();
    marcados.dedup();
    if !marcados.is_empty() {
        ia_resumo.push(frases::preencher(modelo::IA_MARCACOES, &[("n", &marcados.len().to_string())]));
    }
    // rastreios em que um programa atuou (anexo H), na ordem do `feito`: "R1", "R2"…
    struct Listado<'a> {
        r: &'a FRastreio,
        id: String,
        lidas: Vec<Corrida>,
        ilegiveis: usize,
    }
    let listados: Vec<Listado> = f
        .rastreios
        .iter()
        .map(|r| (r, corridas_de(r)))
        .filter(|(r, (lidas, ilegiveis))| automatico(r) || r.origem["tipo"] == "assistido" || !lidas.is_empty() || *ilegiveis > 0)
        .enumerate()
        .map(|(i, (r, (lidas, ilegiveis)))| Listado { r, id: format!("R{}", i + 1), lidas, ilegiveis })
        .collect();
    let id_listado = |id: &str| listados.iter().find(|l| l.r.id == id).map(|l| l.id.clone());
    // rastreio assistido: IA só com corrida de motor neural registrada; sem corrida legível, não se sabe qual programa
    // (e então "nenhuma etapa por IA" também não pode ser dito, qualquer que seja a origem); só correlação (mancha) não é IA
    let (mut com_ia, mut sem_registro) = (0usize, 0usize);
    for l in listados.iter().filter(|l| !automatico(l.r)) {
        let assistido = l.r.origem["tipo"] == "assistido";
        let com_neural = l.lidas.iter().any(neural);
        if assistido && com_neural {
            com_ia += 1;
        } else if !com_neural && (l.ilegiveis > 0 || (assistido && l.lidas.is_empty())) {
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
    // modelos por arquivo: `feito.ia` (P2.1 agrupa por modelo e provedor) e os das corridas que ele ainda não traz
    let mut modelos: Vec<FIa> = f.ia.clone();
    for c in listados.iter().flat_map(|l| &l.lidas).filter(|c| neural(c)) {
        let x = FIa::da_corrida(c, "rastreio");
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
    for a in &f.avisos {
        // aviso de código que o catálogo não conhece (versão mais nova) sai pelo código, nunca calado
        let frase = frases::da_condicao(&a.codigo).map(str::to_string).unwrap_or_else(|| a.codigo.clone());
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
    let (mut processamentos, mut diferencas) = (Vec::new(), Vec::new());
    for it in &f.roteiro {
        let item = it.n.to_string();
        if let Some(o) = &it.origem {
            let mapa: Vec<String> = ["c", "kn", "kd", "e"].iter().filter_map(|k| o.mapa.get(k).map(|v| format!("{k}={v}"))).collect();
            let v = fator(&it.velocidade).map(|x| format!("; velocidade {}×", num(x, 3))).unwrap_or_default();
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
            let v = fator(&it.velocidade).filter(|x| (x - 1.0).abs() > 1e-9).map(|x| format!(", a {}×", num(x, 3))).unwrap_or_default();
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
        for p in &it.processamentos {
            processamentos.push(Processamento {
                item: item.clone(),
                ordem: p.ordem.to_string(),
                nome: match tipo_do_proc(p) {
                    Some(t) => frases::rotulo_do_efeito(t).map(str::to_string).unwrap_or_else(|| t.to_string()),
                    None => rot("enquadramento.inteiro").to_string(),
                },
                parametros: parametros(&p.parametros),
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
        let q = &o.quadros;
        let mut quadros = frases::preencher(
            modelo::QUADROS_OCULTOS,
            &[("ancora", &q.ancora.to_string()), ("medido", &q.medido.to_string()), ("interpolado", &q.interpolado.to_string()), ("incerto", &q.incerto.to_string())],
        );
        // ausências aceitas do rastreio dentro do intervalo da ocultação: ali nada foi coberto
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
        ["SICRO".to_string(), format!("{} ({})", ou_traco(&e.sicro.versao), ou_traco(&e.sicro.commit)), rot("papel.sicro").to_string()],
        ["FFmpeg".to_string(), ou_traco(&e.ffmpeg.versao), format!("{} · origem: {} · SHA-256 {}", rot("papel.ffmpeg"), ou_traco(&e.ffmpeg.origem), ou_traco(&e.ffmpeg.sha256))],
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
        listados.iter().filter(|l| l.lidas.iter().filter(|c| neural(c)).any(|c| FIa::da_corrida(c, &m.uso).mesmo(m))).map(|l| l.id.clone()).collect()
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
        caso: ECaso { numero: e.caso.numero.clone(), referencia: e.caso.referencia.clone(), orgao: e.caso.orgao.clone(), requisitante: e.caso.requisitante.clone() },
        perito: EAutor { nome: e.autor.nome.clone(), cargo: e.autor.cargo.clone(), matricula: e.autor.matricula.clone() },
        gerado_em: data(&t.renderizado_em),
        revisado_por: Some(f.revisado_por.trim().to_string()).filter(|s| !s.is_empty()),
        saida,
        originais: f.midias.iter().enumerate().map(|(i, m)| original(i, m)).collect(),
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
    /// base 1/15360 da cena de apoio).
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
            MotorRastreio::Mancha => ("", vec![], "", "", "", ""),
        };
        let modelo = ModeloCorrida { nome: nome.into(), origem: origem.into(), commit: commit.into(), arquivos, desvio: desvio.into() };
        Corrida {
            motor: MotorCorrida { motor, modelo, runtime: runtime.into(), provedor: provedor.into() },
            sentido,
            ancora_pts: 512 * de,
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
        ProvRastreio { id: id.into(), origem, ancoras, denso_sha256: Some(format!("{:x}", Sha256::digest(id.as_bytes()))), silhuetas_sha256: None, corridas, confirmacao: None, ausencias: vec![] }
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
        let (vit, mancha) = (corrida(MotorRastreio::Vittrack, "cpu", Sentido::Frente, 100, 101), corrida(MotorRastreio::Mancha, "cpu", Sentido::Tras, 90, 100));
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
            proc(3, "destaque", json!({ "tipo": "destaque", "id": "e3", "rastreio": "r1", "forma": "elipse" })),
            proc(4, "carimbo", json!({ "tipo": "carimbo", "id": "e4", "mostra": "tempo_fonte" })),
            proc(5, "velocidade_exibida", json!({ "tipo": "velocidade_exibida", "id": "e5", "calculo": { "kmh": 42.0, "faixa": [39.0, 45.5] } })),
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
        let t = tecnico_completo(&c);
        let d = dados(&t, &miniaturas(), &pacote()).unwrap();
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
        tem("quadros: 2 pela marcação do perito, 0 medidos, 0 interpolados, 0 incertos; trechos sem ocultação, aceitos pelo perito: 0:01,500 a 0:01,750 (alvo fora de cena)");
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
        assert!(item["efeitos"].as_array().unwrap().contains(&json!("Ocultação (rosto)")), "{item}");
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
        assert_eq!(d["ocultacoes"][0]["quadros"], json!("quadros: 2 pela marcação do perito, 0 medidos, 0 interpolados, 0 incertos"));
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
        assert!(d["roteiro"][1]["efeitos"].as_array().unwrap().contains(&json!("Som da gravação retirado")));
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
            let do_item: Vec<&str> = d["roteiro"][1]["efeitos"].as_array().unwrap().iter().map(|e| e.as_str().unwrap()).collect();
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
        assert_eq!(resumo(&plano), ["Ocultações propostas por detecção automática e confirmadas pelo perito: 1."]);
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
        placa.quadros = ProvQuadrosOcultos { ancora: 1, medido: 40, interpolado: 3, incerto: 5 };
        pv.ocultacoes.push(placa);
        let t = montar(&plano, &res, &c.ambiente("t1"), &fecho()).unwrap();
        let d = dados(&t, &[], &[]).unwrap();
        let confirmado = "confirmado por Perito Teste em 14/11/2026, 6h00 (Brasília), de 0:01,000 a 0:02,000";
        assert_eq!(linha_da_planilha(&d, 0), [
            json!("1"), json!("2"), json!("A"), json!("cam 1.mp4"), json!("0:01,000 · 0:02,000"), json!("rosto"), json!("retângulo"),
            json!("sólido opaco, cor #000000ff"), json!("terceiro sem relação"), json!("quadros: 2 pela marcação do perito, 0 medidos, 0 interpolados, 0 incertos"),
            json!(format!("detecção automática (rosto) · marcações do perito: 2 · {confirmado} · R1 (anexo H)")),
        ]);
        assert_eq!(linha_da_planilha(&d, 1), [
            json!("2"), json!("2, 3"), json!("A"), json!("cam 1.mp4"), json!("0:01,000 · 0:03,300"), json!("placa"), json!("elipse, margem 10 %, borda suave 2 %"),
            json!("mosaico, intensidade 0,75"), json!("placa de veículo de terceiro"), json!("quadros: 1 pela marcação do perito, 40 medidos, 3 interpolados, 5 incertos"),
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
        assert_eq!(com(&|t| t["feito"]["ocultacoes"][1]["quadros"]["incerto"] = json!(9))["ocultacoes"][1]["quadros"], json!("quadros: 1 pela marcação do perito, 40 medidos, 3 interpolados, 9 incertos"));
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
            "quadros: 1 pela marcação do perito, 40 medidos, 3 interpolados, 5 incertos",
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
        let vit = corrida(MotorRastreio::Vittrack, "cpu", Sentido::Frente, 100, 101);
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
        assert_eq!((m.as_array().unwrap().len(), &m[0]["modelo"], &m[0]["uso"], &m[0]["execucao"]), (3, &json!("YOLOX-S"), &json!("deteccao"), &json!("onnxruntime 1.31.1 · CPU")));
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
                  "VitTrack · onnxruntime 1.31.1 · CPU · para a frente, 0:03,333 a 0:03,367 (pts 51200 a 51712) · parada: fim do trecho · limiares 0,95 e 0,5, paciência de 90 quadros · 14/11/2026, 5h30 (Brasília)",
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
        assert_eq!(ia, ["Ocultações propostas por detecção automática e confirmadas pelo perito: 1.", "Alvos acompanhados por programa de inteligência artificial a partir de marcação do perito: 1.", modelo::IA_MODELOS]);
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

    /// Fecho com o P2.1: `feito.ia` pelo `ia::uso::da_proveniencia` das mesmas corridas. O anexo H não repete modelo,
    /// não perde nenhum das corridas, não lista correlação como modelo e sai igual em duas chamadas.
    #[test]
    #[ignore = "fecho da Fase 2: P2.1"]
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
        assert_eq!(parametros(&json!({ "tipo": "x", "id": "e1", "ligado": true, "nome": "n", "b": 1.5, "a": "t /x", "c": null, "d": { "k": [1, 2] } })), "a: t /x; b: 1.5; c: —; d: {\"k\":[1,2]}");
        assert_eq!(parametros(&json!({ "regiao": "x".repeat(1000) })).chars().count(), "regiao: ".len() + MAX_VALOR + 1);
        assert_eq!((onde(&[3]), onde(&[2, 5])), ("item 3".into(), "itens 2, 5".into()));
    }

    /// Ritmo da gravação pelos passos medidos: constante, variável (com a faixa) e lacuna.
    #[test]
    fn ritmo_e_intervalo_da_gravacao() {
        let midia = |min: i64, med: i64, max: i64, lacunas: Value| -> Original {
            let m: FMidia = serde_json::from_value(json!({ "id": "m1", "nome": "cam.mp4", "medido": { "tb": { "num": 1, "den": 1000 }, "de_s": 0.0, "ate_s": 20.0,
                "quadros": 500, "passo_min": min, "passo_med": med, "passo_max": max, "lacunas": lacunas }, "usado": [{ "de_s": 3.0, "ate_s": 9.0 }], "usado_total_s": 6.0 })).unwrap();
            original(1, &m)
        };
        let o = midia(40, 40, 40, json!([]));
        assert_eq!((o.id.as_str(), o.ritmo.as_str(), o.taxa_variavel, o.tecnico.intervalo_quadros.as_str()), ("B", "25 imagens por segundo, constante", false, "40,000 ms (mín. = máx.)"));
        let c = o.cobertura.unwrap();
        assert_eq!((c.usado.as_str(), c.total.as_str(), c.intervalos), ("6 s", "20 s", vec!["0:03,000 a 0:09,000".to_string()]));
        // oscilação de ±4 ms em 40 ms: constante
        assert!(!midia(36, 40, 44, json!([])).taxa_variavel);
        let o = midia(40, 40, 83, json!([]));
        assert_eq!((o.ritmo.as_str(), o.taxa_variavel), ("entre 12,05 e 25 imagens por segundo (variável)", true));
        assert_eq!(o.tecnico.intervalo_quadros, "40,000 a 83,000 ms (mediana 40,000)");
        assert!(midia(40, 40, 40, json!([[400, 4000]])).taxa_variavel, "lacuna");
        assert_eq!((o.recebido.as_str(), o.relogio.as_str(), o.formato_nativo), ("não informado", "sem informação", None));
        // som (narração) e foto: sem tabela de quadros não há duração nem parte usada a declarar (nunca "0 s")
        for (quadros, especie, ritmo) in [(0, "Som", "só som"), (1, "Imagem", "imagem única")] {
            let m: FMidia = serde_json::from_value(json!({ "id": "m2", "nome": "x", "medido": { "tb": { "num": 1, "den": 1000 }, "de_s": 0.0, "ate_s": 0.0, "quadros": quadros } })).unwrap();
            let o = original(0, &m);
            assert_eq!((o.especie.as_str(), o.ritmo.as_str(), o.duracao.as_str(), o.cobertura.is_none(), o.taxa_variavel), (especie, ritmo, "—", true, false));
        }
    }
}
