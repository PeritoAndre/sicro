//! Modelo v2 da Edição (arquitetura §2.2). Espelho de `src/types/edicao.ts`: mesmos nomes
//! e mesma ordem de campos; os nomes são o fio do serde (snake_case).
//! Só o front grava o `projeto.json`; aqui ele é lido tipado e validado.

use serde::{Deserialize, Serialize};

use super::quadros::{Busca, OrigemTabela};

/// Valor de `esquema` no `projeto.json`.
pub const ESQUEMA: u32 = 2;
/// Sobe a cada campo novo com efeito no render; arquivo com valor maior abre só para leitura.
pub const ESQUEMA_MENOR: u32 = 0;

fn sim() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Blocos básicos

/// Fração exata (taxa de quadros, base de tempo).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Racional {
    pub num: i64,
    pub den: i64,
}

/// Ponto em 0..1.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Ponto {
    pub x: f64,
    pub y: f64,
}

/// Caixa em 0..1; (x, y) = canto superior esquerdo; ponto: w = h = 0.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Caixa {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Tamanho em pixels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TamanhoPx {
    pub w: u32,
    pub h: u32,
}

/// Interpolação entre uma chave e a seguinte.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Interp {
    #[default]
    Linear,
    Suave,
    Degrau,
}

/// Quadro-chave; `interp` vale desta chave até a próxima. `pts` só em chave presa a quadro de mídia.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chave<T> {
    pub t: f64,
    #[serde(default)]
    pub pts: Option<i64>,
    pub v: T,
    #[serde(default)]
    pub interp: Interp,
}

/// Valor fixo ou animado. T nunca é sequência, então `untagged` não é ambíguo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Animavel<T> {
    Fixo(T),
    Chaves(Vec<Chave<T>>),
}

/// Intervalo na base de tempo do clipe; campo ausente = até a borda do clipe.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Intervalo {
    #[serde(default)]
    pub de: Option<f64>,
    #[serde(default)]
    pub ate: Option<f64>,
}

/// Item de versão futura: lido como JSON cru, preservado, e acusado na validação.
/// Um `tipo` conhecido com campo faltando também cai em `Desconhecido`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Tolerante<T> {
    Conhecido(T),
    Desconhecido(serde_json::Value),
}

// ---------------------------------------------------------------------------
// Projeto

/// O `projeto.json`. Datas em RFC 3339; `principal` na ordem do vídeo; `sobreposicoes` de baixo para cima.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjetoEdicao {
    pub esquema: u32,
    #[serde(default)]
    pub esquema_menor: u32,
    pub id: String,
    pub titulo: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub gravado_por: String,
    pub saida: Saida,
    #[serde(default)]
    pub midias: Vec<MidiaRef>,
    #[serde(default)]
    pub principal: Vec<Tolerante<ClipePrincipal>>,
    #[serde(default)]
    pub fecho: Option<Transicao>,
    #[serde(default)]
    pub sobreposicoes: Vec<TrilhaSobreposicao>,
    #[serde(default)]
    pub audios: Vec<TrilhaAudio>,
    #[serde(default)]
    pub legendas: TrilhaLegendas,
    #[serde(default)]
    pub rastreios: Vec<Rastreio>,
    #[serde(default)]
    pub marcadores: Vec<Marcador>,
    #[serde(default)]
    pub sincronias: Vec<SincroniaRef>,
    #[serde(default)]
    pub mixagem: Mixagem,
}

// ---------------------------------------------------------------------------
// Saída

/// Saída: px (pares), taxa, cor de fundo e a última escolha do diálogo Exportar. `auto` = segue as fontes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Saida {
    pub largura: u32,
    pub altura: u32,
    pub fps: Racional,
    #[serde(default = "sim")]
    pub auto: bool,
    pub cor_fundo: String,
    pub audio: bool,
    pub selos: Selos,
    pub exportacao: Exportacao,
}

/// Selos de edição na imagem (nascem ligados).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Selos {
    pub velocidade: bool,
    pub ampliacao: bool,
    pub realce: bool,
}

/// Escolhas do diálogo Exportar; `revisado_por` vazio = não houve revisão.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Exportacao {
    pub predefinicao: Predefinicao,
    pub codificador: EscolhaCodificador,
    pub legendas: ModoLegendas,
    pub pacote: Option<Pacote>,
    pub pdf: ModoPdf,
    pub revisado_por: String,
    pub personalizada: Option<Personalizada>,
}

/// `Whatsapp`, não `WhatsApp`: o literal é "whatsapp".
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Predefinicao {
    Juri,
    Whatsapp,
    Preservacao,
    Personalizada,
}

/// `Cpu` = "Arquivo reprodutível".
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EscolhaCodificador {
    Auto,
    Cpu,
}

/// Onde as legendas saem.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModoLegendas {
    Nenhuma,
    Queimar,
    Arquivo,
    Ambos,
}

/// Pacote do júri; `originais` = incluir as gravações.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Pacote {
    pub originais: bool,
}

/// "a3" | "a2_sem_anexos".
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModoPdf {
    A3,
    A2SemAnexos,
}

/// Predefinição personalizada: `qualidade` = crf/cq; `altura_max` em px; áudio em kb/s.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Personalizada {
    pub codec: Codec,
    pub conteiner: Conteiner,
    pub qualidade: f64,
    pub altura_max: Option<u32>,
    pub audio_kbps: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    H264,
    Hevc,
    Av1,
    Ffv1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Conteiner {
    Mp4,
    Mkv,
    Mov,
}

// ---------------------------------------------------------------------------
// Mídias usadas

/// Retrato de uma mídia usada. `id` local ao projeto; `relativo` ao caso; durações e início em s (tempo do contêiner);
/// `largura`/`altura` exibidas em px; `cor` None = automático; `decodificacao` None = passada de integridade ainda não feita.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MidiaRef {
    pub id: String,
    pub tipo: TipoMidia,
    pub origem: OrigemMidia,
    pub media_id: Option<String>,
    pub sha256: String,
    pub nome: String,
    pub relativo: String,
    pub duracao_s: Option<f64>,
    pub inicio_s: Option<f64>,
    pub largura: Option<u32>,
    pub altura: Option<u32>,
    pub taxa: Option<Racional>,
    pub vfr: bool,
    pub tem_audio: bool,
    pub cor: Option<CorMidia>,
    pub derivacao: Option<Derivacao>,
    pub recebido: Recebido,
    pub relogio_desvio_s: Option<f64>,
    pub gravador: Option<String>,
    pub decodificacao: Option<Decodificacao>,
    /// No fio cada item leva `"tipo": "ocultar"` (é o `EfOcultar` do TS).
    #[serde(default, with = "ocultacoes_da_midia")]
    pub ocultacoes: Vec<EfOcultar>,
}

/// `EfOcultar` fora do enum `Efeito` perde a tag; aqui ela é posta e exigida.
mod ocultacoes_da_midia {
    use super::EfOcultar;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize)]
    #[serde(tag = "tipo", rename_all = "snake_case")]
    enum Ida<'a> {
        Ocultar(&'a EfOcultar),
    }

    #[derive(Deserialize)]
    #[serde(tag = "tipo", rename_all = "snake_case")]
    enum Volta {
        Ocultar(EfOcultar),
    }

    pub fn serialize<S: Serializer>(v: &[EfOcultar], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(v.iter().map(Ida::Ocultar))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<EfOcultar>, D::Error> {
        let v = Vec::<Volta>::deserialize(d)?;
        Ok(v.into_iter().map(|Volta::Ocultar(e)| e).collect())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TipoMidia {
    Video,
    Audio,
    Imagem,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrigemMidia {
    Caso,
    Projeto,
}

/// Cor escolhida pelo perito para a mídia.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct CorMidia {
    pub matriz: Matriz,
    pub faixa: Faixa,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Matriz {
    Bt601,
    Bt709,
}

/// `Tv` = limitada, `Pc` = cheia.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Faixa {
    Tv,
    Pc,
}

/// Proveniência de mídia derivada, até a raiz.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Derivacao {
    pub tipo: TipoDerivacao,
    pub de_sha256: String,
    pub raiz_sha256: String,
    pub comando: String,
    pub com_perdas: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TipoDerivacao {
    Trecho,
    Estabilizado,
    Lente,
    CopiaDvr,
    Superres,
    Outro,
}

/// Veio no formato do gravador? `nativo` None = não informado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Recebido {
    pub nativo: Option<bool>,
    pub obs: String,
}

/// Erros contados na passada de integridade.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Decodificacao {
    pub erros: u32,
}

// ---------------------------------------------------------------------------
// Posição, transição, velocidade, áudio do clipe

/// Posição de clipe livre: tempo do programa (s) ou deslocamento (s) do início de um clipe da principal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "ref", rename_all = "snake_case")]
pub enum Posicao {
    Programa { t: f64 },
    Clipe { clipe: String, desloc: f64 },
}

/// Transição de entrada (ou o `fecho`); `duracao_s` em s.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transicao {
    /// Catálogo em `motor/juncao.rs`; id desconhecido = erro de validação, preservado.
    pub tipo: String,
    pub duracao_s: f64,
    pub audio: AudioTransicao,
}

/// Som na transição.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioTransicao {
    Acompanhar,
    Cortar,
}

/// `fator` vira racional por milésimos em `tempo.rs`; `pontos` em tempo de fonte.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Velocidade {
    Constante { fator: f64 },
    Rampa { pontos: Vec<Chave<f64>> },
}

/// Som do clipe: `volume` 0..2; `manter_tom` true = `atempo`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioClipe {
    pub volume: Animavel<f64>,
    pub mudo: bool,
    pub manter_tom: bool,
    #[serde(default)]
    pub silencios: Vec<Silencio>,
}

impl Default for AudioClipe {
    fn default() -> Self {
        Self { volume: Animavel::Fixo(1.0), mudo: false, manter_tom: true, silencios: Vec::new() }
    }
}

/// `de` e `ate` em tempo de fonte.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Silencio {
    pub id: String,
    pub de: f64,
    pub ate: f64,
    pub modo: ModoSilencio,
    pub motivo: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModoSilencio {
    Tom,
    Mudo,
}

// ---------------------------------------------------------------------------
// Clipes da principal

/// Clipe da trilha principal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum ClipePrincipal {
    Trecho(ClipeTrecho),
    Congelar(ClipeCongelar),
    Imagem(ClipeImagem),
    Titulo(ClipeTitulo),
    Tela(ClipeTela),
}

/// Comum a todo clipe da principal; `rotulo` = "Nome" na tela.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipeBase {
    pub id: String,
    #[serde(default)]
    pub rotulo: String,
    #[serde(default)]
    pub desligado: bool,
    #[serde(default)]
    pub transicao_entrada: Option<Transicao>,
    #[serde(default)]
    pub efeitos: Vec<Tolerante<Efeito>>,
}

/// [quadro exibido em inicio_s, quadro exibido em fim_s); `fim_pts` None = fim da mídia.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipeTrecho {
    #[serde(flatten)]
    pub base: ClipeBase,
    pub midia: String,
    pub inicio_s: f64,
    pub fim_s: f64,
    #[serde(default)]
    pub inicio_pts: Option<i64>,
    #[serde(default)]
    pub fim_pts: Option<i64>,
    pub velocidade: Velocidade,
    #[serde(default)]
    pub audio: AudioClipe,
}

/// Quadro congelado: o exibido em `instante_s` (tempo de fonte), por `duracao_s` s.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipeCongelar {
    #[serde(flatten)]
    pub base: ClipeBase,
    pub midia: String,
    pub instante_s: f64,
    #[serde(default)]
    pub instante_pts: Option<i64>,
    pub duracao_s: f64,
}

/// Imagem parada por `duracao_s` s.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipeImagem {
    #[serde(flatten)]
    pub base: ClipeBase,
    pub midia: String,
    pub duracao_s: f64,
}

/// Cartela por `duracao_s` s.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipeTitulo {
    #[serde(flatten)]
    pub base: ClipeBase,
    pub duracao_s: f64,
    pub titulo: Titulo,
}

/// Fase 3. Tempo e áudio vêm da célula `referencia`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipeTela {
    #[serde(flatten)]
    pub base: ClipeBase,
    pub arranjo: Arranjo,
    pub celulas: Vec<Celula>,
    pub referencia: usize,
    pub inicio_s: f64,
    pub fim_s: f64,
    #[serde(default)]
    pub inicio_pts: Option<i64>,
    #[serde(default)]
    pub fim_pts: Option<i64>,
    pub velocidade: Velocidade,
    pub audio: AudioClipe,
    pub pip: Option<Caixa>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Arranjo {
    LadoALado,
    Empilhado,
    Grade,
    Pip,
}

/// `sincronia` None na célula de referência; `efeitos` no espaço da própria célula.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Celula {
    pub midia: String,
    pub sincronia: Option<String>,
    pub rotulo: String,
    pub efeitos: Vec<Tolerante<Efeito>>,
}

// ---------------------------------------------------------------------------
// Efeitos

/// Efeito de clipe; a ordem dentro de cada família é a de aplicação.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Efeito {
    Estabilizar(EfEstabilizar),
    Lente(EfLente),
    Desentrelacar(EfDesentrelacar),
    MediaTemporal(EfMediaTemporal),
    Ajustes(EfAjustes),
    Nitidez(EfNitidez),
    Girar(EfGirar),
    Enquadramento(EfEnquadramento),
    Ocultar(EfOcultar),
    Holofote(EfHolofote),
    Destaque(EfDestaque),
    Lupa(EfLupa),
    Regua(EfRegua),
    VelocidadeExibida(EfVelocidadeExibida),
    Texto(EfTexto),
    Carimbo(EfCarimbo),
    Imagem(EfImagem),
}

/// Comum a todo efeito; `ativo` None = o clipe inteiro.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfBase {
    pub id: String,
    #[serde(default = "sim")]
    pub ligado: bool,
    #[serde(default)]
    pub nome: String,
    #[serde(default)]
    pub ativo: Option<Intervalo>,
}

/// `espessura` em fração da altura do canvas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Traco {
    pub cor: String,
    pub espessura: f64,
    pub contorno: bool,
}

/// `tamanho` em fração da altura do canvas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EstiloTexto {
    pub fonte: Fonte,
    pub negrito: bool,
    pub italico: bool,
    pub tamanho: f64,
    pub cor: String,
    pub alinhamento: Alinhamento,
    pub caixa: Option<CaixaTexto>,
    pub contorno: Option<ContornoTexto>,
}

/// Família tipográfica (não é fonte de mídia).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Fonte {
    Sans,
    Serifa,
    Mono,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Alinhamento {
    Esq,
    Centro,
    Dir,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlinhamentoV {
    Topo,
    Meio,
    Base,
}

/// Caixa atrás do texto: `margem` e `raio` em fração da altura do canvas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaixaTexto {
    pub cor: String,
    pub margem: f64,
    pub raio: f64,
}

/// Contorno do texto: `largura` em fração da altura do canvas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContornoTexto {
    pub cor: String,
    pub largura: f64,
}

/// Ponto do canvas (0..1) e que ponto da caixa do texto cai nele.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Lugar {
    pub x: f64,
    pub y: f64,
    pub h: Alinhamento,
    pub v: AlinhamentoV,
}

/// `vertices` (polígono) em 0..1 dentro da caixa; `borda_suave` cresce para fora.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlvoRegiao {
    pub rastreio: String,
    pub forma: FormaRegiao,
    pub vertices: Option<Vec<Ponto>>,
    pub margem: f64,
    pub borda_suave: f64,
    pub inverter: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FormaRegiao {
    Retangulo,
    Elipse,
    Poligono,
    Silhueta,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Interpolacao {
    Vizinho,
    Bilinear,
    Bicubica,
}

/// `suavizar` em quadros; `zoom_pct` calculado uma vez com borda "ampliar"; `trf_sha256` None = ainda não analisado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfEstabilizar {
    #[serde(flatten)]
    pub base: EfBase,
    pub suavizar: u32,
    pub borda: BordaEstab,
    pub analise: FaixaAnalise,
    pub zoom_pct: f64,
    pub trf_sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BordaEstab {
    Preta,
    Ampliar,
}

/// Faixa do `.trf`; não segue a borda do clipe.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct FaixaAnalise {
    pub de_s: f64,
    pub ate_s: f64,
    pub de_pts: i64,
    pub ate_pts: i64,
}

/// Correção radial: `k1`, `k2` e centro (`cx`, `cy`) em 0..1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfLente {
    #[serde(flatten)]
    pub base: EfBase,
    pub k1: f64,
    pub k2: f64,
    pub cx: f64,
    pub cy: f64,
    pub perfil: String,
}

/// `quadro` = um quadro por quadro; `campo` = um por campo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfDesentrelacar {
    #[serde(flatten)]
    pub base: EfBase,
    pub modo: ModoDesentrelace,
    pub ordem: OrdemCampos,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModoDesentrelace {
    Quadro,
    Campo,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrdemCampos {
    Auto,
    Tff,
    Bff,
}

/// `quadros` em 2..15.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfMediaTemporal {
    #[serde(flatten)]
    pub base: EfBase,
    pub quadros: u32,
}

/// Ajustes de tom (valores do filtro `eq`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfAjustes {
    #[serde(flatten)]
    pub base: EfBase,
    pub brilho: f64,
    pub contraste: f64,
    pub gama: f64,
    pub saturacao: f64,
}

/// Nitidez (`unsharp`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfNitidez {
    #[serde(flatten)]
    pub base: EfBase,
    pub quantidade: f64,
}

/// `graus` em {0, 90, 180, 270}.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfGirar {
    #[serde(flatten)]
    pub base: EfBase,
    pub graus: u16,
    pub espelhar_h: bool,
    pub espelhar_v: bool,
}

/// `regiao` em 0..1 da origem; "vizinho" só com região fixa.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfEnquadramento {
    #[serde(flatten)]
    pub base: EfBase,
    pub regiao: Animavel<Caixa>,
    pub ajuste: Ajuste,
    pub interpolacao: Interpolacao,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Ajuste {
    Conter,
    Cobrir,
}

/// `intensidade` em 0..1; `cor` só no modo sólido (alfa ff).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfOcultar {
    #[serde(flatten)]
    pub base: EfBase,
    pub modo: ModoOcultar,
    pub alvo: AlvoRegiao,
    pub intensidade: f64,
    pub cor: String,
    pub conteudo: ConteudoOculto,
    pub motivo: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModoOcultar {
    Desfoque,
    Mosaico,
    Solido,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConteudoOculto {
    Rosto,
    Placa,
    Corpo,
    Tela,
    Documento,
    Outro,
}

/// Escurece (0..1) e dessatura (0..1) fora da região.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfHolofote {
    #[serde(flatten)]
    pub base: EfBase,
    pub alvo: AlvoRegiao,
    pub escurecer: f64,
    pub dessaturar: f64,
}

/// `tamanho` (círculo e seta sobre ponto) em fração da altura do canvas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfDestaque {
    #[serde(flatten)]
    pub base: EfBase,
    pub forma: FormaDestaque,
    pub rastreio: String,
    pub traco: Traco,
    pub tamanho: f64,
    pub angulo_graus: f64,
    pub margem: f64,
    pub rotulo: Option<RotuloAnexo>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FormaDestaque {
    Circulo,
    Elipse,
    Retangulo,
    Seta,
    Colchetes,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RotuloAnexo {
    pub texto: String,
    pub estilo: EstiloTexto,
    pub lado: Lado,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lado {
    Cima,
    Baixo,
    Esq,
    Dir,
}

/// `janela` em fração da altura da fonte; `fator` em 1,5..8.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfLupa {
    #[serde(flatten)]
    pub base: EfBase,
    pub rastreio: String,
    pub janela: f64,
    pub fator: f64,
    pub forma: FormaLupa,
    pub interpolacao: Interpolacao,
    pub onde: OndeLupa,
    pub moldura: Traco,
    pub ligacao: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FormaLupa {
    Circulo,
    Retangulo,
}

/// Centro da lupa no canvas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "modo", rename_all = "snake_case")]
pub enum OndeLupa {
    Fixa { x: f64, y: f64 },
    Segue { dx: f64, dy: f64 },
}

/// Régua no plano do chão: `ancora` em 0..1, direção em graus, comprimento em m.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfRegua {
    #[serde(flatten)]
    pub base: EfBase,
    pub calibracao: RetratoCalibracao,
    pub ancora: Ponto,
    pub direcao_graus: f64,
    pub comprimento_m: f64,
    pub divisoes: u32,
    pub traco: Traco,
    pub rotulo: bool,
}

/// Retrato: homografia, resíduo e polígono dos pontos de controle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetratoCalibracao {
    pub id: String,
    pub metodo: MetodoCalibracao,
    pub h: Vec<f64>,
    pub espaco: TamanhoPx,
    pub pontos_sha256: String,
    pub residuo_m: f64,
    pub regiao: Vec<Ponto>,
    // CONTRATO: acréscimo (fecho da Fase 1). None = retrato gravado antes (nada a conferir). A falta do campo lê como
    // None sem atributo de padrão: o front não precisa completá-lo (contratos (g) 12).
    /// SHA-256 da mídia em que a calibração foi marcada: a régua só vale em clipe dessa mídia.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midia_sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetodoCalibracao {
    Line,
    Plane,
    CrossRatio,
}

/// Velocidade calculada na análise, mostrada como texto.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfVelocidadeExibida {
    #[serde(flatten)]
    pub base: EfBase,
    pub calculo: RetratoCalculo,
    pub onde: OndeVelocidade,
    pub estilo: EstiloTexto,
}

/// `faixa` = (mínimo, máximo) em km/h; no fio, lista de dois números.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetratoCalculo {
    pub id: String,
    pub kmh: f64,
    pub faixa: Option<(f64, f64)>,
    pub de_s: f64,
    pub ate_s: f64,
    pub metodo: String,
    // CONTRATO: acréscimo (fecho da Fase 1). None = retrato gravado antes (nada a conferir).
    /// SHA-256 da mídia em que a velocidade foi medida: o texto só vale em clipe dessa mídia.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midia_sha256: Option<String>,
}

/// Onde o texto fica: lugar fixo ou deslocamento (0..1 do canvas) de um rastreio.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "modo", rename_all = "snake_case")]
pub enum OndeVelocidade {
    Fixa { lugar: Lugar },
    Segue { rastreio: String, dx: f64, dy: f64 },
}

/// Texto livre; `largura_max` em fração da largura do canvas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfTexto {
    #[serde(flatten)]
    pub base: EfBase,
    pub conteudo: String,
    pub estilo: EstiloTexto,
    pub lugar: Lugar,
    pub largura_max: f64,
}

/// Carimbo de tempo; `relogio` só com `mostra: relogio`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfCarimbo {
    #[serde(flatten)]
    pub base: EfBase,
    pub mostra: MostraCarimbo,
    pub formato: FormatoCarimbo,
    pub relogio: Option<RetratoRelogio>,
    pub estilo: EstiloTexto,
    pub lugar: Lugar,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MostraCarimbo {
    TempoFonte,
    Relogio,
    Quadro,
    TempoPrograma,
}

/// "hms_ms" | "hms" | "data_hora".
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FormatoCarimbo {
    HmsMs,
    Hms,
    DataHora,
}

/// Retrato do vínculo de relógio (um ponto).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetratoRelogio {
    pub media_time_s: f64,
    pub clock_seconds: f64,
    pub clock_date: Option<String>,
}

/// Imagem sobre o clipe: `largura` em fração da largura do canvas, `opacidade` 0..1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EfImagem {
    #[serde(flatten)]
    pub base: EfBase,
    pub midia: String,
    pub lugar: Lugar,
    pub largura: f64,
    pub opacidade: f64,
}

// ---------------------------------------------------------------------------
// Títulos

/// Título: `modelo` é só rótulo; o conteúdo está em `blocos`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Titulo {
    pub modelo: ModeloTitulo,
    pub fundo: Fundo,
    #[serde(default)]
    pub blocos: Vec<BlocoTexto>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModeloTitulo {
    Cartela,
    Abertura,
    Evento,
    TercoInferior,
    Encerramento,
    Livre,
}

/// Fundo do título.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Fundo {
    Cor { cor: String },
    Transparente,
    Imagem { midia: String },
}

/// Bloco de texto do título; `largura_max` em fração da largura do canvas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlocoTexto {
    pub id: String,
    pub papel: PapelTexto,
    pub conteudo: String,
    pub estilo: EstiloTexto,
    pub lugar: Lugar,
    pub largura_max: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PapelTexto {
    Titulo,
    Subtitulo,
    Corpo,
    Rodape,
}

// ---------------------------------------------------------------------------
// Sobreposições

/// Trilha de clipes livres sobre a principal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrilhaSobreposicao {
    pub id: String,
    pub nome: String,
    pub oculta: bool,
    #[serde(default)]
    pub clipes: Vec<Tolerante<ClipeSobreposto>>,
}

/// Tempo local do clipe. `x`, `y` = centro no canvas (0..1); `escala` 1 = largura do canvas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transformacao {
    pub x: Animavel<f64>,
    pub y: Animavel<f64>,
    pub escala: Animavel<f64>,
    pub rotacao: Animavel<f64>,
    pub opacidade: Animavel<f64>,
}

/// Clipe livre sobre a principal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum ClipeSobreposto {
    Video(SobVideo),
    Imagem(SobImagem),
    Titulo(SobTitulo),
}

/// `duracao_s` None = até o fim do clipe de âncora (ou do programa).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SobBase {
    pub id: String,
    pub rotulo: String,
    pub pos: Posicao,
    pub duracao_s: Option<f64>,
    pub transformacao: Transformacao,
    pub entrada_s: f64,
    pub saida_s: f64,
    #[serde(default)]
    pub efeitos: Vec<Tolerante<Efeito>>,
}

/// Fase 3.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SobVideo {
    #[serde(flatten)]
    pub base: SobBase,
    pub midia: String,
    pub inicio_s: f64,
    pub inicio_pts: Option<i64>,
    pub velocidade: Velocidade,
    pub audio: Option<AudioClipe>,
    pub moldura: Option<Traco>,
    pub sincronia: Option<String>,
}

/// Imagem sobreposta.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SobImagem {
    #[serde(flatten)]
    pub base: SobBase,
    pub midia: String,
}

/// Título sobreposto (terço inferior, marca fixa).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SobTitulo {
    #[serde(flatten)]
    pub base: SobBase,
    pub titulo: Titulo,
}

// ---------------------------------------------------------------------------
// Áudio

/// Trilha de áudio; `volume` 0..2; `abaixa_original` = o original desce sob ela.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrilhaAudio {
    pub id: String,
    pub nome: String,
    pub papel: PapelAudio,
    pub volume: f64,
    pub mudo: bool,
    pub abaixa_original: bool,
    #[serde(default)]
    pub clipes: Vec<ClipeAudio>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PapelAudio {
    Narracao,
    Musica,
    Efeitos,
}

/// `de_s`/`ate_s` = recorte dentro do arquivo; `volume` em tempo local.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipeAudio {
    pub id: String,
    pub nome: String,
    pub midia: String,
    pub pos: Posicao,
    pub de_s: f64,
    pub ate_s: Option<f64>,
    pub volume: Animavel<f64>,
    pub entrada_s: f64,
    pub saida_s: f64,
}

/// Mixagem do programa; `volume_original` 0..2.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mixagem {
    pub volume_original: f64,
    pub abaixar: Abaixar,
    pub normalizar: Option<Normalizar>,
    pub limitador: bool,
}

/// Arquivo sem `mixagem`: nada muda o som (projeto NOVO nasce com normalizar −16/−1,5, posto pelo front).
impl Default for Mixagem {
    fn default() -> Self {
        Self {
            volume_original: 1.0,
            abaixar: Abaixar { db: -12.0, ataque_s: 0.3, soltura_s: 0.5 },
            normalizar: None,
            limitador: false,
        }
    }
}

/// Abaixar o original sob a narração: dB, ataque e soltura em s.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Abaixar {
    pub db: f64,
    pub ataque_s: f64,
    pub soltura_s: f64,
}

/// EBU R128: medição + ganho linear.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Normalizar {
    pub lufs: f64,
    pub pico_db: f64,
}

// ---------------------------------------------------------------------------
// Legendas

/// Legendas do programa; `origem` = degravação de onde vieram.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrilhaLegendas {
    pub estilo: EstiloTexto,
    pub lugar: Lugar,
    pub linhas_max: u32,
    pub origem: Option<OrigemLegendas>,
    #[serde(default)]
    pub itens: Vec<Legenda>,
}

/// Trilha vazia (arquivo sem `legendas` e importador v1).
impl Default for TrilhaLegendas {
    fn default() -> Self {
        Self {
            estilo: EstiloTexto {
                fonte: Fonte::Sans,
                negrito: false,
                italico: false,
                tamanho: 0.052,
                cor: "#ffffff".into(),
                alinhamento: Alinhamento::Centro,
                caixa: Some(CaixaTexto { cor: "#0000008c".into(), margem: 0.013, raio: 0.0 }),
                contorno: None,
            },
            lugar: Lugar { x: 0.5, y: 0.94, h: Alinhamento::Centro, v: AlinhamentoV::Base },
            linhas_max: 2,
            origem: None,
            itens: Vec::new(),
        }
    }
}

/// Degravação de onde as legendas vieram.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrigemLegendas {
    pub audio_sha256: String,
    pub importado_em: String,
    pub motor: Option<String>,
    pub modelo_sha256: Option<String>,
}

/// `origem: ia` não conferida barra a exportação. O campo `ref` do fio é `alvo` aqui.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Legenda {
    pub id: String,
    pub texto: String,
    pub locutor: Option<String>,
    pub origem: OrigemLegenda,
    pub conferida: bool,
    pub locutor_auto: bool,
    #[serde(rename = "ref")]
    pub alvo: RefLegenda,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrigemLegenda {
    Perito,
    Ia,
}

/// Tempo da legenda (s): preso à fonte ou ao programa.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum RefLegenda {
    Fonte { midia: String, de: f64, ate: f64 },
    Programa { de: f64, ate: f64 },
}

// ---------------------------------------------------------------------------
// Rastreios (§6.1)

/// `midia` None = coordenadas do canvas. `espaco` = "original" ou "origem:<hash>".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rastreio {
    pub id: String,
    pub nome: String,
    pub cor: String,
    pub midia: Option<String>,
    pub base: BaseRastreio,
    pub tipo: TipoRastreio,
    pub espaco: String,
    #[serde(default)]
    pub ancoras: Vec<Chave<Caixa>>,
    pub denso: Option<Denso>,
    pub silhuetas: Option<Silhuetas>,
    #[serde(default)]
    pub ausencias: Vec<Ausencia>,
    pub origem: OrigemRastreio,
    pub confirmacao: Option<Confirmacao>,
    /// `t` das âncoras escolhidas pelo "Simplificar" e não mexidas pelo perito desde então.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ancoras_do_programa: Vec<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BaseRastreio {
    Fonte,
    Local,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TipoRastreio {
    Caixa,
    Ponto,
}

/// Ponteiro para `rastreios/<id>/<sha256>.json` (o conteúdo é `rastreio::ArquivoDenso`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Denso {
    pub sha256: String,
    pub n: u32,
    pub de_s: f64,
    pub ate_s: f64,
    pub ok: u32,
    pub incertos: u32,
    pub ausentes: u32,
}

/// Ponteiro para `silhuetas/<id>/<sha256>.sil` (o conteúdo é `rastreio::ArquivoSilhuetas`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Silhuetas {
    pub sha256: String,
    pub n: u32,
    pub modelo: String,
}

/// Lacuna aceita pelo perito.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ausencia {
    pub de_s: f64,
    pub ate_s: f64,
    pub motivo: MotivoAusencia,
    pub aceita_em: String,
    pub por: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MotivoAusencia {
    Oculto,
    ForaDeCena,
    Perdido,
}

/// Como o rastreio nasceu.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum OrigemRastreio {
    Manual,
    Assistido,
    Pessoas { pessoa_id: String, estado: EstadoPessoa, copiado_em: String },
    Deteccao { classe: ClasseDeteccao },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EstadoPessoa {
    Auto,
    Revisada,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClasseDeteccao {
    Rosto,
    Placa,
    Cabeca,
    Pessoa,
    Veiculo,
}

/// Vale só para esse conteúdo (`denso_sha256`) e intervalo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Confirmacao {
    pub por: String,
    pub em: String,
    pub denso_sha256: Option<String>,
    pub de_s: f64,
    pub ate_s: f64,
}

// ---------------------------------------------------------------------------
// Marcadores e sincronias

/// Marcador na linha do tempo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Marcador {
    pub id: String,
    pub rotulo: String,
    pub cor: String,
    pub pos: PosMarcador,
}

/// Posição do marcador: programa, clipe ou tempo de fonte (s).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "ref", rename_all = "snake_case")]
pub enum PosMarcador {
    Programa { t: f64 },
    Clipe { clipe: String, desloc: f64 },
    Fonte { midia: String, t: f64 },
}

/// Retrato de um vínculo (§9). t_outra = t_ref + desloc_s + deriva_ppm·1e-6·(t_ref − t0_s).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SincroniaRef {
    pub id: String,
    pub vinculo_id: Option<String>,
    pub referencia: String,
    pub outra: String,
    pub desloc_s: f64,
    pub deriva_ppm: f64,
    pub deriva_incerteza_ppm: f64,
    pub t0_s: f64,
    pub valido_de_s: f64,
    pub valido_ate_s: f64,
    pub metodo: MetodoSincronia,
    /// None = não determinada.
    pub incerteza_s: Option<f64>,
    pub confianca: Option<f64>,
    #[serde(default)]
    pub conferencias: Vec<Conferencia>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetodoSincronia {
    Audio,
    Evento,
    Relogio,
    Manual,
}

/// Conferência visual de um vínculo de sincronia.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "modo", rename_all = "snake_case")]
pub enum Conferencia {
    EventoVisivel { t_ref_s: f64, diferenca_quadros: i32, por: String, em: String },
    SemEvento { por: String, em: String },
}

// ---------------------------------------------------------------------------
// Resultado de `edicao_analisar` (só saída: `Problema.codigo` é estático, sem Deserialize)

/// `erro` bloqueia o render; `aviso` informa; `pericial` pede ciência.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Nivel {
    Erro,
    Aviso,
    Pericial,
}

/// `mensagem` vem do catálogo fixo (`mensagens.rs`); `n` = contagem quando o código tem uma.
/// `clipe` = id do item da linha do tempo (clipe da principal, livre ou de áudio, legenda, marcador);
/// `trilha` = id da trilha desse item, quando ele está numa; `campo` = nome do campo no fio.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Problema {
    pub nivel: Nivel,
    pub codigo: &'static str,
    pub mensagem: String,
    pub clipe: Option<String>,
    pub efeito: Option<String>,
    pub trilha: Option<String>,
    pub midia: Option<String>,
    pub campo: Option<String>,
    pub n: Option<u64>,
}

/// Trecho lido da fonte: quadros `j_in..j_out` da tabela; `pts_out` None = fim da mídia.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FaixaFonte {
    pub midia: String,
    pub j_in: u32,
    pub j_out: u32,
    pub pts_in: i64,
    pub pts_out: Option<i64>,
    pub lidos: u32,
    pub repetidos: u32,
    pub descartados: u32,
    pub tabela: OrigemTabela,
}

/// Clipe da principal na linha do tempo, em slots; `sobrepoe_q` = transição de entrada.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ClipeResolvido {
    pub id: String,
    pub inicio_q: u64,
    pub n_q: u32,
    pub sobrepoe_q: u32,
    pub fonte: Option<FaixaFonte>,
}

/// Clipe livre (sobreposto ou de áudio) na linha do tempo, em slots.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LivreResolvido {
    pub id: String,
    pub trilha: String,
    pub inicio_q: u64,
    pub n_q: u32,
}

/// Linha do tempo em slots na taxa `fps`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LinhaResolvida {
    pub fps: Racional,
    pub total_q: u64,
    pub clipes: Vec<ClipeResolvido>,
    pub livres: Vec<LivreResolvido>,
}

/// O arquivo da mídia foi achado e é o mesmo?
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SituacaoMidia {
    Ok,
    Ausente,
    Trocada,
}

/// `id` = `MidiaRef.id`. `tabela`, `busca` e `erros` None enquanto não há tabela.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EstadoMidia {
    pub id: String,
    pub estado: SituacaoMidia,
    pub tabela: Option<OrigemTabela>,
    pub busca: Option<Busca>,
    pub erros: Option<u32>,
}

/// Campo do documento que guarda um tempo preso a quadro.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum OndePrender {
    /// `inicio_s`/`inicio_pts` de trecho, tela ou vídeo sobreposto.
    Inicio { clipe: String },
    /// `fim_s`/`fim_pts` de trecho ou tela.
    Fim { clipe: String },
    /// `instante_s`/`instante_pts` de quadro congelado.
    Instante { clipe: String },
    /// `ancoras[indice]` de um rastreio.
    Ancora { rastreio: String, indice: u32 },
    /// `analise.de_s`/`de_pts` de um `estabilizar`.
    AnaliseDe { clipe: String, efeito: String },
    /// `analise.ate_s`/`ate_pts` de um `estabilizar`.
    AnaliseAte { clipe: String, efeito: String },
}

/// Tempo preso a quadro que o front grava por `alterar()`. `pts` None só em `Fim` no fim da mídia.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Prender {
    pub onde: OndePrender,
    pub pts: Option<i64>,
    pub t: f64,
}

/// `saida_auto` = (largura, altura, fps) enquanto `saida.auto`; no fio, lista de três.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Analise {
    pub problemas: Vec<Problema>,
    pub linha: LinhaResolvida,
    pub midias: Vec<EstadoMidia>,
    pub prender: Vec<Prender>,
    pub saida_auto: Option<(u32, u32, Racional)>,
}

// ---------------------------------------------------------------------------
// Testes: uma ida e volta por enum com tag (os completos são do P0.2)

#[cfg(test)]
mod testes {
    use super::*;
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};

    /// Lê `j` como `T`, regrava e exige o mesmo JSON.
    fn ida_e_volta<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(j: Value) -> T {
        let lido: T = serde_json::from_value(j.clone()).expect("lê");
        assert_eq!(serde_json::to_value(&lido).unwrap(), j);
        lido
    }

    fn estilo() -> Value {
        json!({ "fonte": "sans", "negrito": false, "italico": true, "tamanho": 0.033, "cor": "#ffffff",
                "alinhamento": "centro", "caixa": { "cor": "#0000008c", "margem": 0.013, "raio": 0.0 }, "contorno": null })
    }

    fn lugar() -> Value {
        json!({ "x": 0.5, "y": 0.94, "h": "centro", "v": "base" })
    }

    fn alvo() -> Value {
        json!({ "rastreio": "r1", "forma": "elipse", "vertices": null, "margem": 0.1, "borda_suave": 0.0, "inverter": false })
    }

    fn base_ef(tipo: &str) -> serde_json::Map<String, Value> {
        let mut m = serde_json::Map::new();
        m.insert("tipo".into(), json!(tipo));
        m.insert("id".into(), json!("e1"));
        m.insert("ligado".into(), json!(true));
        m.insert("nome".into(), json!(""));
        m.insert("ativo".into(), json!({ "de": 1.5, "ate": null }));
        m
    }

    fn ef(tipo: &str, campos: Value) -> Value {
        let mut m = base_ef(tipo);
        m.extend(campos.as_object().unwrap().clone());
        Value::Object(m)
    }

    fn sob_base(tipo: &str, campos: Value) -> Value {
        let mut m = serde_json::Map::new();
        m.insert("tipo".into(), json!(tipo));
        m.extend(
            json!({ "id": "s1", "rotulo": "", "pos": { "ref": "clipe", "clipe": "c1", "desloc": 0.5 }, "duracao_s": null,
                    "transformacao": { "x": 0.5, "y": 0.5, "escala": 1.0, "rotacao": 0.0,
                                       "opacidade": [ { "t": 0.0, "pts": null, "v": 0.0, "interp": "suave" }, { "t": 1.0, "pts": null, "v": 1.0, "interp": "linear" } ] },
                    "entrada_s": 0.0, "saida_s": 0.25, "efeitos": [] })
            .as_object()
            .unwrap()
            .clone(),
        );
        m.extend(campos.as_object().unwrap().clone());
        Value::Object(m)
    }

    #[test]
    fn literais_de_enum_unitario() {
        let s = |v: Value| v.as_str().unwrap().to_string();
        assert_eq!(s(serde_json::to_value(Predefinicao::Whatsapp).unwrap()), "whatsapp");
        assert_eq!(s(serde_json::to_value(ModoPdf::A2SemAnexos).unwrap()), "a2_sem_anexos");
        assert_eq!(s(serde_json::to_value(ModoPdf::A3).unwrap()), "a3");
        assert_eq!(s(serde_json::to_value(Codec::H264).unwrap()), "h264");
        assert_eq!(s(serde_json::to_value(Codec::Av1).unwrap()), "av1");
        assert_eq!(s(serde_json::to_value(Codec::Ffv1).unwrap()), "ffv1");
        assert_eq!(s(serde_json::to_value(Conteiner::Mp4).unwrap()), "mp4");
        assert_eq!(s(serde_json::to_value(Matriz::Bt601).unwrap()), "bt601");
        assert_eq!(s(serde_json::to_value(Arranjo::LadoALado).unwrap()), "lado_a_lado");
        assert_eq!(s(serde_json::to_value(TipoDerivacao::CopiaDvr).unwrap()), "copia_dvr");
        assert_eq!(s(serde_json::to_value(TipoDerivacao::Superres).unwrap()), "superres");
        assert_eq!(s(serde_json::to_value(MetodoCalibracao::CrossRatio).unwrap()), "cross_ratio");
        assert_eq!(s(serde_json::to_value(MostraCarimbo::TempoFonte).unwrap()), "tempo_fonte");
        assert_eq!(s(serde_json::to_value(FormatoCarimbo::HmsMs).unwrap()), "hms_ms");
        assert_eq!(s(serde_json::to_value(FormatoCarimbo::DataHora).unwrap()), "data_hora");
        assert_eq!(s(serde_json::to_value(ModeloTitulo::TercoInferior).unwrap()), "terco_inferior");
        assert_eq!(s(serde_json::to_value(MotivoAusencia::ForaDeCena).unwrap()), "fora_de_cena");
        assert_eq!(s(serde_json::to_value(Interp::Degrau).unwrap()), "degrau");
        assert_eq!(s(serde_json::to_value(Nivel::Pericial).unwrap()), "pericial");
    }

    #[test]
    fn animavel_fixo_e_chaves() {
        let fixo: Animavel<f64> = ida_e_volta(json!(0.75));
        assert_eq!(fixo, Animavel::Fixo(0.75));
        // inteiro em campo f64
        assert_eq!(serde_json::from_value::<Animavel<f64>>(json!(1)).unwrap(), Animavel::Fixo(1.0));
        let chaves: Animavel<Caixa> = ida_e_volta(json!([
            { "t": 4.0, "pts": 61440, "v": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 }, "interp": "linear" },
            { "t": 5.0, "pts": null, "v": { "x": 0.2, "y": 0.2, "w": 0.5, "h": 0.5 }, "interp": "degrau" }
        ]));
        assert!(matches!(chaves, Animavel::Chaves(ref c) if c.len() == 2 && c[0].pts == Some(61440)));
        // `pts` e `interp` podem faltar
        let curta: Animavel<f64> = serde_json::from_value(json!([{ "t": 0, "v": 1 }])).unwrap();
        assert_eq!(curta, Animavel::Chaves(vec![Chave { t: 0.0, pts: None, v: 1.0, interp: Interp::Linear }]));
    }

    #[test]
    fn posicao_tag_ref() {
        assert_eq!(ida_e_volta::<Posicao>(json!({ "ref": "programa", "t": 3.5 })), Posicao::Programa { t: 3.5 });
        assert!(matches!(ida_e_volta::<Posicao>(json!({ "ref": "clipe", "clipe": "c1", "desloc": -0.5 })), Posicao::Clipe { .. }));
    }

    #[test]
    fn pos_marcador_tag_ref() {
        ida_e_volta::<PosMarcador>(json!({ "ref": "programa", "t": 1.0 }));
        ida_e_volta::<PosMarcador>(json!({ "ref": "clipe", "clipe": "c1", "desloc": 0.0 }));
        let f: PosMarcador = ida_e_volta(json!({ "ref": "fonte", "midia": "m1", "t": 3.0 }));
        assert_eq!(f, PosMarcador::Fonte { midia: "m1".into(), t: 3.0 });
    }

    #[test]
    fn velocidade_tag_tipo() {
        assert_eq!(ida_e_volta::<Velocidade>(json!({ "tipo": "constante", "fator": 0.25 })), Velocidade::Constante { fator: 0.25 });
        let r: Velocidade = ida_e_volta(json!({ "tipo": "rampa", "pontos": [ { "t": 1.0, "pts": null, "v": 2.0, "interp": "linear" } ] }));
        assert!(matches!(r, Velocidade::Rampa { ref pontos } if pontos.len() == 1));
    }

    #[test]
    fn fundo_tag_tipo() {
        ida_e_volta::<Fundo>(json!({ "tipo": "cor", "cor": "#000000" }));
        assert_eq!(ida_e_volta::<Fundo>(json!({ "tipo": "transparente" })), Fundo::Transparente);
        ida_e_volta::<Fundo>(json!({ "tipo": "imagem", "midia": "m2" }));
    }

    #[test]
    fn onde_lupa_tag_modo() {
        assert_eq!(ida_e_volta::<OndeLupa>(json!({ "modo": "fixa", "x": 0.8, "y": 0.2 })), OndeLupa::Fixa { x: 0.8, y: 0.2 });
        ida_e_volta::<OndeLupa>(json!({ "modo": "segue", "dx": 0.1, "dy": -0.1 }));
    }

    #[test]
    fn onde_velocidade_tag_modo() {
        ida_e_volta::<OndeVelocidade>(json!({ "modo": "fixa", "lugar": lugar() }));
        ida_e_volta::<OndeVelocidade>(json!({ "modo": "segue", "rastreio": "r1", "dx": 0.0, "dy": -0.05 }));
    }

    #[test]
    fn ref_legenda_tag_tipo_no_campo_ref() {
        let l: Legenda = ida_e_volta(json!({
            "id": "l1", "texto": "oi", "locutor": null, "origem": "ia", "conferida": false, "locutor_auto": true,
            "ref": { "tipo": "fonte", "midia": "m1", "de": 1.5, "ate": 2.0 }
        }));
        assert_eq!(l.alvo, RefLegenda::Fonte { midia: "m1".into(), de: 1.5, ate: 2.0 });
        ida_e_volta::<RefLegenda>(json!({ "tipo": "programa", "de": 0.0, "ate": 1.0 }));
    }

    #[test]
    fn origem_rastreio_tag_tipo() {
        assert_eq!(ida_e_volta::<OrigemRastreio>(json!({ "tipo": "manual" })), OrigemRastreio::Manual);
        ida_e_volta::<OrigemRastreio>(json!({ "tipo": "assistido" }));
        ida_e_volta::<OrigemRastreio>(json!({ "tipo": "pessoas", "pessoa_id": "p3", "estado": "revisada", "copiado_em": "2026-11-14T03:00:00Z" }));
        ida_e_volta::<OrigemRastreio>(json!({ "tipo": "deteccao", "classe": "placa" }));
    }

    #[test]
    fn conferencia_tag_modo() {
        ida_e_volta::<Conferencia>(json!({ "modo": "evento_visivel", "t_ref_s": 12.25, "diferenca_quadros": -1, "por": "x", "em": "y" }));
        ida_e_volta::<Conferencia>(json!({ "modo": "sem_evento", "por": "x", "em": "y" }));
    }

    #[test]
    fn efeito_tag_tipo_com_base_achatada() {
        let texto_ef = json!({ "conteudo": "{ não é código }", "estilo": estilo(), "lugar": lugar(), "largura_max": 0.92 });
        let traco = json!({ "cor": "#ffcc00", "espessura": 0.006, "contorno": true });
        let casos = vec![
            ef("estabilizar", json!({ "suavizar": 15, "borda": "ampliar", "analise": { "de_s": 1.0, "ate_s": 9.0, "de_pts": 15360, "ate_pts": 138240 }, "zoom_pct": 4.5, "trf_sha256": null })),
            ef("lente", json!({ "k1": -0.2, "k2": 0.05, "cx": 0.5, "cy": 0.5, "perfil": "" })),
            ef("desentrelacar", json!({ "modo": "campo", "ordem": "tff" })),
            ef("media_temporal", json!({ "quadros": 5 })),
            ef("ajustes", json!({ "brilho": 0.1, "contraste": 1.2, "gama": 1.0, "saturacao": 1.0 })),
            ef("nitidez", json!({ "quantidade": 0.5 })),
            ef("girar", json!({ "graus": 270, "espelhar_h": false, "espelhar_v": true })),
            ef("enquadramento", json!({ "regiao": { "x": 0.25, "y": 0.25, "w": 0.5, "h": 0.5 }, "ajuste": "cobrir", "interpolacao": "bilinear" })),
            ef("ocultar", json!({ "modo": "solido", "alvo": alvo(), "intensidade": 1.0, "cor": "#000000ff", "conteudo": "placa", "motivo": "terceiro" })),
            ef("holofote", json!({ "alvo": alvo(), "escurecer": 0.6, "dessaturar": 0.5 })),
            ef("destaque", json!({ "forma": "colchetes", "rastreio": "r1", "traco": traco, "tamanho": 0.12, "angulo_graus": 45.0, "margem": 0.0,
                                   "rotulo": { "texto": "A", "estilo": estilo(), "lado": "cima" } })),
            ef("lupa", json!({ "rastreio": "r1", "janela": 0.2, "fator": 3.0, "forma": "circulo", "interpolacao": "vizinho",
                               "onde": { "modo": "segue", "dx": 0.2, "dy": 0.0 }, "moldura": traco, "ligacao": true })),
            ef("regua", json!({ "calibracao": { "id": "k1", "metodo": "plane", "h": [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], "espaco": { "w": 1920, "h": 1080 },
                                                "pontos_sha256": "ab", "residuo_m": 0.03, "regiao": [ { "x": 0.1, "y": 0.1 } ] },
                                "ancora": { "x": 0.4, "y": 0.6 }, "direcao_graus": 90.0, "comprimento_m": 2.0, "divisoes": 4, "traco": traco, "rotulo": true })),
            ef("velocidade_exibida", json!({ "calculo": { "id": "v1", "kmh": 42.0, "faixa": [39.0, 45.0], "de_s": 3.0, "ate_s": 5.0, "metodo": "plane" },
                                             "onde": { "modo": "fixa", "lugar": lugar() }, "estilo": estilo() })),
            ef("texto", texto_ef),
            ef("carimbo", json!({ "mostra": "relogio", "formato": "data_hora", "relogio": { "media_time_s": 10.0, "clock_seconds": 43200.0, "clock_date": "2026-11-14" },
                                  "estilo": estilo(), "lugar": lugar() })),
            ef("imagem", json!({ "midia": "m3", "lugar": lugar(), "largura": 0.2, "opacidade": 1.0 })),
        ];
        assert_eq!(casos.len(), 17);
        for j in casos {
            let tipo = j["tipo"].as_str().unwrap().to_string();
            let e: Tolerante<Efeito> = ida_e_volta(j);
            assert!(matches!(e, Tolerante::Conhecido(_)), "{tipo} caiu em Desconhecido");
        }
        // `ligado`, `nome` e `ativo` podem faltar
        let curto: Efeito = serde_json::from_value(json!({ "tipo": "nitidez", "id": "e9", "quantidade": 1 })).unwrap();
        assert!(matches!(curto, Efeito::Nitidez(ref n) if n.base.ligado && n.base.ativo.is_none()));
    }

    #[test]
    fn tolerante_preserva_o_desconhecido() {
        let futuro = json!({ "tipo": "efeito_do_futuro", "id": "e4", "campo_novo": [1, 2, 3] });
        let e: Tolerante<Efeito> = ida_e_volta(futuro.clone());
        assert_eq!(e, Tolerante::Desconhecido(futuro));
        // tipo conhecido com campo faltando também é preservado
        let invalido = json!({ "tipo": "nitidez", "id": "e5" });
        assert_eq!(ida_e_volta::<Tolerante<Efeito>>(invalido.clone()), Tolerante::Desconhecido(invalido));
    }

    #[test]
    fn clipe_principal_tag_tipo() {
        let base = |tipo: &str, campos: Value| {
            let mut m = serde_json::Map::new();
            m.insert("tipo".into(), json!(tipo));
            m.extend(json!({ "id": "c1", "rotulo": "Entrada", "desligado": false, "transicao_entrada": null, "efeitos": [] }).as_object().unwrap().clone());
            m.extend(campos.as_object().unwrap().clone());
            Value::Object(m)
        };
        let vel = json!({ "tipo": "constante", "fator": 1.0 });
        let audio = json!({ "volume": 1.0, "mudo": false, "manter_tom": true,
                            "silencios": [ { "id": "s1", "de": 2.0, "ate": 2.5, "modo": "tom", "motivo": "nome de terceiro" } ] });
        let titulo = json!({ "modelo": "cartela", "fundo": { "tipo": "cor", "cor": "#000000" },
                             "blocos": [ { "id": "b1", "papel": "corpo", "conteudo": "Câmera 1", "estilo": estilo(), "lugar": lugar(), "largura_max": 0.9 } ] });
        let casos = vec![
            base("trecho", json!({ "midia": "m1", "inicio_s": 1713000000.04, "fim_s": 1713000012.0, "inicio_pts": 1713000000040i64, "fim_pts": null,
                                   "velocidade": vel, "audio": audio })),
            base("congelar", json!({ "midia": "m1", "instante_s": 4.0, "instante_pts": 61440, "duracao_s": 3.0 })),
            base("imagem", json!({ "midia": "m2", "duracao_s": 5.0 })),
            base("titulo", json!({ "duracao_s": 3.0, "titulo": titulo })),
            base("tela", json!({ "arranjo": "pip", "celulas": [ { "midia": "m1", "sincronia": null, "rotulo": "A", "efeitos": [] },
                                                              { "midia": "m4", "sincronia": "s1", "rotulo": "B", "efeitos": [] } ],
                                 "referencia": 0, "inicio_s": 0.0, "fim_s": 2.0, "inicio_pts": 0, "fim_pts": 30720, "velocidade": vel, "audio": audio,
                                 "pip": { "x": 0.7, "y": 0.7, "w": 0.25, "h": 0.25 } })),
        ];
        for j in casos {
            let tipo = j["tipo"].as_str().unwrap().to_string();
            let c: Tolerante<ClipePrincipal> = ida_e_volta(j);
            assert!(matches!(c, Tolerante::Conhecido(_)), "{tipo} caiu em Desconhecido");
        }
        let futuro = json!({ "tipo": "clipe_do_futuro", "id": "c2", "qualquer": { "coisa": true } });
        assert_eq!(ida_e_volta::<Tolerante<ClipePrincipal>>(futuro.clone()), Tolerante::Desconhecido(futuro));
    }

    #[test]
    fn clipe_sobreposto_tag_tipo() {
        let titulo = json!({ "modelo": "terco_inferior", "fundo": { "tipo": "transparente" }, "blocos": [] });
        let casos = vec![
            sob_base("video", json!({ "midia": "m1", "inicio_s": 2.0, "inicio_pts": 30720, "velocidade": { "tipo": "constante", "fator": 1.0 },
                                      "audio": null, "moldura": { "cor": "#ffffff", "espessura": 0.004, "contorno": false }, "sincronia": null })),
            sob_base("imagem", json!({ "midia": "m2" })),
            sob_base("titulo", json!({ "titulo": titulo })),
        ];
        for j in casos {
            let tipo = j["tipo"].as_str().unwrap().to_string();
            let c: Tolerante<ClipeSobreposto> = ida_e_volta(j);
            assert!(matches!(c, Tolerante::Conhecido(_)), "{tipo} caiu em Desconhecido");
        }
    }

    #[test]
    fn ocultacao_da_midia_leva_a_tag() {
        let oc = ef("ocultar", json!({ "modo": "desfoque", "alvo": alvo(), "intensidade": 0.6, "cor": "#000000ff", "conteudo": "rosto", "motivo": "" }));
        let m: MidiaRef = ida_e_volta(json!({
            "id": "m1", "tipo": "video", "origem": "caso", "media_id": "v1", "sha256": "aa", "nome": "cam1.mp4", "relativo": "Mídias/Vídeos/cam1.mp4",
            "duracao_s": 12.5, "inicio_s": 0.0, "largura": 1920, "altura": 1080, "taxa": { "num": 30000, "den": 1001 }, "vfr": false, "tem_audio": true,
            "cor": { "matriz": "bt709", "faixa": "tv" },
            "derivacao": { "tipo": "copia_dvr", "de_sha256": "bb", "raiz_sha256": "cc", "comando": "", "com_perdas": false },
            "recebido": { "nativo": null, "obs": "" }, "relogio_desvio_s": null, "gravador": null, "decodificacao": { "erros": 0 },
            "ocultacoes": [ oc ]
        }));
        assert_eq!(m.ocultacoes.len(), 1);
        // sem a tag (ou com outra) não é um EfOcultar
        let mut sem = serde_json::to_value(&m).unwrap();
        sem["ocultacoes"][0].as_object_mut().unwrap().remove("tipo");
        assert!(serde_json::from_value::<MidiaRef>(sem).is_err());
    }

    #[test]
    fn projeto_minimo_recebe_os_padroes() {
        let p: ProjetoEdicao = serde_json::from_value(json!({
            "esquema": 2, "id": "p1", "titulo": "Teste", "created_at": "2026-11-14T03:00:00Z", "updated_at": "2026-11-14T03:00:00Z",
            "saida": { "largura": 1920, "altura": 1080, "fps": { "num": 30, "den": 1 }, "cor_fundo": "#000000", "audio": true,
                       "selos": { "velocidade": true, "ampliacao": true, "realce": true },
                       "exportacao": { "predefinicao": "whatsapp", "codificador": "cpu", "legendas": "nenhuma", "pacote": { "originais": false },
                                       "pdf": "a2_sem_anexos", "revisado_por": "",
                                       "personalizada": { "codec": "hevc", "conteiner": "mkv", "qualidade": 20, "altura_max": null, "audio_kbps": 192 } } },
            "principal": [ { "tipo": "clipe_do_futuro", "id": "c9" } ]
        }))
        .unwrap();
        assert!(p.saida.auto);
        assert_eq!(p.esquema_menor, 0);
        assert_eq!(p.mixagem, Mixagem::default());
        assert_eq!(p.legendas, TrilhaLegendas::default());
        assert!(matches!(p.principal[0], Tolerante::Desconhecido(_)));
        assert_eq!(p.saida.exportacao.predefinicao, Predefinicao::Whatsapp);
        // regravado e relido: igual
        let de_novo: ProjetoEdicao = serde_json::from_value(serde_json::to_value(&p).unwrap()).unwrap();
        assert_eq!(de_novo, p);
    }

    #[test]
    fn onde_prender_tag_tipo() {
        let p = Prender { onde: OndePrender::Ancora { rastreio: "r1".into(), indice: 2 }, pts: Some(61440), t: 4.0 };
        assert_eq!(serde_json::to_value(&p).unwrap(), json!({ "onde": { "tipo": "ancora", "rastreio": "r1", "indice": 2 }, "pts": 61440, "t": 4.0 }));
        let a = Analise {
            problemas: vec![Problema { nivel: Nivel::Erro, codigo: "vazio", mensagem: String::new(), clipe: None, efeito: None, trilha: None, midia: None, campo: None, n: None }],
            linha: LinhaResolvida { fps: Racional { num: 30, den: 1 }, total_q: 0, clipes: vec![], livres: vec![] },
            midias: vec![EstadoMidia { id: "m1".into(), estado: SituacaoMidia::Trocada, tabela: Some(OrigemTabela::Pacotes), busca: Some(Busca::SemBusca), erros: None }],
            prender: vec![p],
            saida_auto: Some((1920, 1080, Racional { num: 30000, den: 1001 })),
        };
        let j = serde_json::to_value(&a).unwrap();
        assert_eq!(j["saida_auto"], json!([1920, 1080, { "num": 30000, "den": 1001 }]));
        assert_eq!(j["midias"][0], json!({ "id": "m1", "estado": "trocada", "tabela": "pacotes", "busca": "sem_busca", "erros": null }));
        assert_eq!(j["problemas"][0]["nivel"], "erro");
    }

    /// Toda variante sem dados de todo enum serde dos contratos (`edicao/**`): sai com o literal do TS e volta.
    /// Lista gerada de `contratos-tsrust.py` (dossiê), que confere os mesmos literais contra `src/types/edicao.ts`.
    #[test]
    fn literal_de_toda_variante_sem_dados() {
        fn ida_volta<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(v: T, j: Value) {
            assert_eq!(serde_json::to_value(&v).unwrap(), j, "{v:?}");
            assert_eq!(serde_json::from_value::<T>(j.clone()).unwrap(), v, "{j}");
        }
        fn ida<T: Serialize + std::fmt::Debug>(v: T, j: Value) {
            assert_eq!(serde_json::to_value(&v).unwrap(), j, "{v:?}");
        }
        macro_rules! lit {
            ($f:ident, $t:ty: $($v:ident = $l:tt),+ $(,)?) => { $( $f(<$t>::$v, json!($l)); )+ };
        }
        lit!(ida_volta, crate::edicao::camada::formas::EspacoRegiao: Fonte = "fonte", Canvas = "canvas");
        lit!(ida_volta, crate::edicao::camada::formas::ModoRegiao: Desfoque = "desfoque", Mosaico = "mosaico", Solido = "solido", Escurecer = "escurecer");
        lit!(ida_volta, crate::edicao::camada::formas::RegraPreenche: Evenodd = "evenodd");
        lit!(ida_volta, crate::edicao::ffmpeg::exec::ClasseFalha: FiltroAusente = "filtro_ausente", ArquivoAusente = "arquivo_ausente", DiscoCheio = "disco_cheio", Gpu = "gpu", LeituraDivergente = "leitura_divergente", Outra = "outra");
        lit!(ida_volta, crate::edicao::ffmpeg::sonda::ClasseReprovacao: SemGpu = "sem_gpu", SemDriver = "sem_driver", DriverAntigo = "driver_antigo", LimiteDeSessoes = "limite_de_sessoes", RecursoNaoSuportado = "recurso_nao_suportado", FfmpegSemCodificador = "ffmpeg_sem_codificador", SaidaInvalida = "saida_invalida", Outra = "outra");
        lit!(ida_volta, crate::edicao::ffmpeg::sonda::GrafoArquivo: BarraFilterComplex = "barra_filter_complex", FilterComplexScript = "filter_complex_script");
        lit!(ida_volta, crate::edicao::ffmpeg::sonda::Origem: Configurado = "configurado", Embutido = "embutido", Path = "path");
        lit!(ida_volta, crate::edicao::fila::Estado: NaFila = { "tipo": "na_fila" }, Preparando = { "tipo": "preparando" }, Cancelada = { "tipo": "cancelada" });
        lit!(ida_volta, crate::edicao::fila::EstadoSaida: AConferir = "a_conferir", Concluida = "concluida");
        lit!(ida_volta, crate::edicao::fila::Fase: Previas = { "tipo": "previas" }, Camadas = { "tipo": "camadas" }, Audio = { "tipo": "audio" }, Juncao = { "tipo": "juncao" }, Conferindo = { "tipo": "conferindo" }, Relatorio = { "tipo": "relatorio" }, Pacote = { "tipo": "pacote" });
        lit!(ida_volta, crate::edicao::fila::PapelRetrato: Denso = "denso", Silhuetas = "silhuetas", Trf = "trf", Midia = "midia");
        lit!(ida_volta, crate::edicao::fila::TipoTarefa: Exportar = "exportar", Previa = "previa", QuadroFinal = "quadro_final", Refazer = "refazer");
        lit!(ida_volta, crate::edicao::fundo::EstadoFundo: NaFila = { "tipo": "na_fila" }, Rodando = { "tipo": "rodando" }, Pronta = { "tipo": "pronta" }, Cancelada = { "tipo": "cancelada" });
        lit!(ida_volta, crate::edicao::fundo::TipoFundo: Tabela = "tabela", CopiaPrevia = "copia_previa", Proxy = "proxy", Origem = "origem", Onda = "onda", Tira = "tira", Sonda = "sonda");
        lit!(ida_volta, super::Ajuste: Conter = "conter", Cobrir = "cobrir");
        lit!(ida_volta, super::Alinhamento: Esq = "esq", Centro = "centro", Dir = "dir");
        lit!(ida_volta, super::AlinhamentoV: Topo = "topo", Meio = "meio", Base = "base");
        lit!(ida_volta, super::Arranjo: LadoALado = "lado_a_lado", Empilhado = "empilhado", Grade = "grade", Pip = "pip");
        lit!(ida_volta, super::AudioTransicao: Acompanhar = "acompanhar", Cortar = "cortar");
        lit!(ida_volta, super::BaseRastreio: Fonte = "fonte", Local = "local");
        lit!(ida_volta, super::BordaEstab: Preta = "preta", Ampliar = "ampliar");
        lit!(ida_volta, super::ClasseDeteccao: Rosto = "rosto", Placa = "placa", Cabeca = "cabeca", Pessoa = "pessoa", Veiculo = "veiculo");
        lit!(ida_volta, super::Codec: H264 = "h264", Hevc = "hevc", Av1 = "av1", Ffv1 = "ffv1");
        lit!(ida_volta, super::Conteiner: Mp4 = "mp4", Mkv = "mkv", Mov = "mov");
        lit!(ida_volta, super::ConteudoOculto: Rosto = "rosto", Placa = "placa", Corpo = "corpo", Tela = "tela", Documento = "documento", Outro = "outro");
        lit!(ida_volta, super::EscolhaCodificador: Auto = "auto", Cpu = "cpu");
        lit!(ida_volta, super::EstadoPessoa: Auto = "auto", Revisada = "revisada");
        lit!(ida_volta, super::Faixa: Tv = "tv", Pc = "pc");
        lit!(ida_volta, super::Fonte: Sans = "sans", Serifa = "serifa", Mono = "mono");
        lit!(ida_volta, super::FormaDestaque: Circulo = "circulo", Elipse = "elipse", Retangulo = "retangulo", Seta = "seta", Colchetes = "colchetes");
        lit!(ida_volta, super::FormaLupa: Circulo = "circulo", Retangulo = "retangulo");
        lit!(ida_volta, super::FormaRegiao: Retangulo = "retangulo", Elipse = "elipse", Poligono = "poligono", Silhueta = "silhueta");
        lit!(ida_volta, super::FormatoCarimbo: HmsMs = "hms_ms", Hms = "hms", DataHora = "data_hora");
        lit!(ida_volta, super::Fundo: Transparente = { "tipo": "transparente" });
        lit!(ida_volta, super::Interp: Linear = "linear", Suave = "suave", Degrau = "degrau");
        lit!(ida_volta, super::Interpolacao: Vizinho = "vizinho", Bilinear = "bilinear", Bicubica = "bicubica");
        lit!(ida_volta, super::Lado: Cima = "cima", Baixo = "baixo", Esq = "esq", Dir = "dir");
        lit!(ida_volta, super::Matriz: Bt601 = "bt601", Bt709 = "bt709");
        lit!(ida_volta, super::MetodoCalibracao: Line = "line", Plane = "plane", CrossRatio = "cross_ratio");
        lit!(ida_volta, super::MetodoSincronia: Audio = "audio", Evento = "evento", Relogio = "relogio", Manual = "manual");
        lit!(ida_volta, super::ModeloTitulo: Cartela = "cartela", Abertura = "abertura", Evento = "evento", TercoInferior = "terco_inferior", Encerramento = "encerramento", Livre = "livre");
        lit!(ida_volta, super::ModoDesentrelace: Quadro = "quadro", Campo = "campo");
        lit!(ida_volta, super::ModoLegendas: Nenhuma = "nenhuma", Queimar = "queimar", Arquivo = "arquivo", Ambos = "ambos");
        lit!(ida_volta, super::ModoOcultar: Desfoque = "desfoque", Mosaico = "mosaico", Solido = "solido");
        lit!(ida_volta, super::ModoPdf: A3 = "a3", A2SemAnexos = "a2_sem_anexos");
        lit!(ida_volta, super::ModoSilencio: Tom = "tom", Mudo = "mudo");
        lit!(ida_volta, super::MostraCarimbo: TempoFonte = "tempo_fonte", Relogio = "relogio", Quadro = "quadro", TempoPrograma = "tempo_programa");
        lit!(ida_volta, super::MotivoAusencia: Oculto = "oculto", ForaDeCena = "fora_de_cena", Perdido = "perdido");
        lit!(ida, super::Nivel: Erro = "erro", Aviso = "aviso", Pericial = "pericial");
        lit!(ida_volta, super::OrdemCampos: Auto = "auto", Tff = "tff", Bff = "bff");
        lit!(ida_volta, super::OrigemLegenda: Perito = "perito", Ia = "ia");
        lit!(ida_volta, super::OrigemMidia: Caso = "caso", Projeto = "projeto");
        lit!(ida_volta, super::OrigemRastreio: Manual = { "tipo": "manual" }, Assistido = { "tipo": "assistido" });
        lit!(ida_volta, super::PapelAudio: Narracao = "narracao", Musica = "musica", Efeitos = "efeitos");
        lit!(ida_volta, super::PapelTexto: Titulo = "titulo", Subtitulo = "subtitulo", Corpo = "corpo", Rodape = "rodape");
        lit!(ida_volta, super::Predefinicao: Juri = "juri", Whatsapp = "whatsapp", Preservacao = "preservacao", Personalizada = "personalizada");
        lit!(ida, super::SituacaoMidia: Ok = "ok", Ausente = "ausente", Trocada = "trocada");
        lit!(ida_volta, super::TipoDerivacao: Trecho = "trecho", Estabilizado = "estabilizado", Lente = "lente", CopiaDvr = "copia_dvr", Superres = "superres", Outro = "outro");
        lit!(ida_volta, super::TipoMidia: Video = "video", Audio = "audio", Imagem = "imagem");
        lit!(ida_volta, super::TipoRastreio: Caixa = "caixa", Ponto = "ponto");
        lit!(ida_volta, crate::edicao::motor::plano::Juncao: Direta = { "tipo": "direta" });
        lit!(ida_volta, crate::edicao::motor::plano::Papel: Imagem = { "tipo": "imagem" }, Gerada = { "tipo": "gerada" });
        lit!(ida_volta, crate::edicao::motor::plano::TipoCamada: Anotacao = "anotacao", MascaraCanvas = "mascara_canvas", MascaraFonte = "mascara_fonte");
        lit!(ida_volta, crate::edicao::previa::TipoPrevia: Original = "original", Copia = "copia", Proxy = "proxy", Origem = "origem");
        lit!(ida_volta, crate::edicao::quadros::Busca: Confiavel = "confiavel", SemBusca = "sem_busca");
        lit!(ida_volta, crate::edicao::quadros::OrigemTabela: Pacotes = "pacotes", Decodificador = "decodificador");
        lit!(ida_volta, crate::edicao::rastreio::avaliar::Como: Ancora = "ancora", Denso = "denso", Interpolado = "interpolado", Segurado = "segurado");
    }

    /// Variantes vistas num projeto lido e ids dos itens que caíram em `Desconhecido`.
    fn cobertura(p: &ProjetoEdicao) -> (std::collections::BTreeSet<String>, Vec<String>) {
        use std::collections::BTreeSet;
        fn tag<T: Serialize>(x: &T, campo: &str) -> String {
            serde_json::to_value(x).unwrap()[campo].as_str().unwrap().to_string()
        }
        let mut vistos: BTreeSet<String> = BTreeSet::new();
        let mut desconhecidos: Vec<String> = Vec::new();
        let efeitos = |lista: &[Tolerante<Efeito>], vistos: &mut BTreeSet<String>, desc: &mut Vec<String>| {
            for e in lista {
                match e {
                    Tolerante::Conhecido(e) => {
                        vistos.insert(format!("efeito:{}", tag(e, "tipo")));
                        match e {
                            Efeito::Lupa(l) => { vistos.insert(format!("onde_lupa:{}", tag(&l.onde, "modo"))); }
                            Efeito::VelocidadeExibida(v) => { vistos.insert(format!("onde_velocidade:{}", tag(&v.onde, "modo"))); }
                            Efeito::Enquadramento(q) => { vistos.insert(format!("animavel:{}", matches!(q.regiao, Animavel::Chaves(_)))); }
                            _ => {}
                        }
                    }
                    Tolerante::Desconhecido(v) => desc.push(v["id"].as_str().unwrap().to_string()),
                }
            }
        };
        for m in &p.midias {
            for o in &m.ocultacoes {
                efeitos(&[Tolerante::Conhecido(Efeito::Ocultar(o.clone()))], &mut vistos, &mut desconhecidos);
            }
        }
        for c in &p.principal {
            match c {
                Tolerante::Conhecido(c) => {
                    vistos.insert(format!("clipe:{}", tag(c, "tipo")));
                    let base = match c {
                        ClipePrincipal::Trecho(x) => { vistos.insert(format!("velocidade:{}", tag(&x.velocidade, "tipo"))); &x.base }
                        ClipePrincipal::Congelar(x) => &x.base,
                        ClipePrincipal::Imagem(x) => &x.base,
                        ClipePrincipal::Titulo(x) => { vistos.insert(format!("fundo:{}", tag(&x.titulo.fundo, "tipo"))); &x.base }
                        ClipePrincipal::Tela(x) => {
                            vistos.insert(format!("velocidade:{}", tag(&x.velocidade, "tipo")));
                            for cel in &x.celulas { efeitos(&cel.efeitos, &mut vistos, &mut desconhecidos); }
                            &x.base
                        }
                    };
                    efeitos(&base.efeitos, &mut vistos, &mut desconhecidos);
                }
                Tolerante::Desconhecido(v) => desconhecidos.push(v["id"].as_str().unwrap().to_string()),
            }
        }
        for t in &p.sobreposicoes {
            vistos.insert("trilha:sobreposicao".into());
            for c in &t.clipes {
                match c {
                    Tolerante::Conhecido(c) => {
                        vistos.insert(format!("sobreposto:{}", tag(c, "tipo")));
                        let base = match c {
                            ClipeSobreposto::Video(x) => &x.base,
                            ClipeSobreposto::Imagem(x) => &x.base,
                            ClipeSobreposto::Titulo(x) => { vistos.insert(format!("fundo:{}", tag(&x.titulo.fundo, "tipo"))); &x.base }
                        };
                        vistos.insert(format!("posicao:{}", tag(&base.pos, "ref")));
                        vistos.insert(format!("animavel:{}", matches!(base.transformacao.x, Animavel::Chaves(_))));
                        efeitos(&base.efeitos, &mut vistos, &mut desconhecidos);
                    }
                    Tolerante::Desconhecido(v) => desconhecidos.push(v["id"].as_str().unwrap().to_string()),
                }
            }
        }
        for t in &p.audios {
            vistos.insert(format!("trilha:audio:{}", serde_json::to_value(t.papel).unwrap().as_str().unwrap()));
            for c in &t.clipes { vistos.insert(format!("posicao:{}", tag(&c.pos, "ref"))); }
        }
        for l in &p.legendas.itens { vistos.insert(format!("trilha:legenda:{}", tag(&l.alvo, "tipo"))); }
        for r in &p.rastreios { vistos.insert(format!("origem_rastreio:{}", tag(&r.origem, "tipo"))); }
        for m in &p.marcadores { vistos.insert(format!("marcador:{}", tag(&m.pos, "ref"))); }
        for s in &p.sincronias {
            for c in &s.conferencias { vistos.insert(format!("conferencia:{}", tag(c, "modo"))); }
        }
        (vistos, desconhecidos)
    }

    const VARIANTES: &[&str] = &[
        "clipe:trecho", "clipe:congelar", "clipe:imagem", "clipe:titulo", "clipe:tela",
        "sobreposto:video", "sobreposto:imagem", "sobreposto:titulo",
        "efeito:estabilizar", "efeito:lente", "efeito:desentrelacar", "efeito:media_temporal", "efeito:ajustes",
        "efeito:nitidez", "efeito:girar", "efeito:enquadramento", "efeito:ocultar", "efeito:holofote", "efeito:destaque",
        "efeito:lupa", "efeito:regua", "efeito:velocidade_exibida", "efeito:texto", "efeito:carimbo", "efeito:imagem",
        "posicao:programa", "posicao:clipe", "marcador:programa", "marcador:clipe", "marcador:fonte",
        "fundo:cor", "fundo:transparente", "fundo:imagem",
        "trilha:sobreposicao", "trilha:audio:narracao", "trilha:audio:musica", "trilha:audio:efeitos",
        "trilha:legenda:fonte", "trilha:legenda:programa",
        "velocidade:constante", "velocidade:rampa", "onde_lupa:fixa", "onde_lupa:segue",
        "onde_velocidade:fixa", "onde_velocidade:segue", "animavel:true", "animavel:false",
        "origem_rastreio:manual", "origem_rastreio:assistido", "origem_rastreio:pessoas", "origem_rastreio:deteccao",
        "conferencia:evento_visivel", "conferencia:sem_evento",
    ];

    fn fixture(nome: &str) -> String {
        let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edicao").join(nome);
        std::fs::read_to_string(&caminho).expect("fixture")
    }

    /// Projeto com todas as variantes de clipe, efeito, posição, fundo e trilha, Option cheio e vazio, e três itens
    /// desconhecidos: lido do TEXTO, nenhum conhecido cai em `Desconhecido`, e volta igual.
    #[test]
    fn contrato_todas_as_variantes_vai_e_volta() {
        let texto = fixture("contrato-todas-variantes.json");
        let bruto: Value = serde_json::from_str(&texto).unwrap();
        let p: ProjetoEdicao = serde_json::from_str(&texto).expect("lê tipado");
        assert_eq!(serde_json::to_value(&p).unwrap(), bruto, "não voltou igual");
        let de_novo: ProjetoEdicao = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(de_novo, p);
        let (vistos, desconhecidos) = cobertura(&p);
        let faltam: Vec<&str> = VARIANTES.iter().copied().filter(|e| !vistos.contains(*e)).collect();
        assert!(faltam.is_empty(), "faltam na fixture: {faltam:?}");
        assert_eq!(desconhecidos, ["e99", "c9", "l9"], "só os três itens do futuro podem cair em Desconhecido");
    }

    /// `completo.json` (a referência do front contra `src/types/edicao.ts`): toda variante e todo campo opcional
    /// preenchido (nenhum `null`, `pts`/`interp`/`de`/`ate` presentes), nenhum item desconhecido, e o Rust o regrava igual.
    #[test]
    fn completo_vai_e_volta() {
        fn nulos(v: &Value, onde: &str, saida: &mut Vec<String>) {
            match v {
                Value::Null => saida.push(onde.to_string()),
                Value::Array(a) => a.iter().enumerate().for_each(|(i, x)| nulos(x, &format!("{onde}/{i}"), saida)),
                Value::Object(o) => o.iter().for_each(|(k, x)| nulos(x, &format!("{onde}/{k}"), saida)),
                _ => {}
            }
        }
        let texto = fixture("completo.json");
        let bruto: Value = serde_json::from_str(&texto).unwrap();
        let mut com_nulo = Vec::new();
        nulos(&bruto, "", &mut com_nulo);
        assert!(com_nulo.is_empty(), "campos nulos em completo.json: {com_nulo:?}");
        let p: ProjetoEdicao = serde_json::from_str(&texto).expect("lê tipado");
        let regravado = serde_json::to_value(&p).unwrap();
        assert_eq!(regravado, bruto, "o Rust não regrava completo.json igual");
        let (vistos, desconhecidos) = cobertura(&p);
        let faltam: Vec<&str> = VARIANTES.iter().copied().filter(|e| !vistos.contains(*e)).collect();
        assert!(faltam.is_empty(), "faltam em completo.json: {faltam:?}");
        assert!(desconhecidos.is_empty(), "caíram em Desconhecido: {desconhecidos:?}");
    }
}
