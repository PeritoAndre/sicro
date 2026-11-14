//! O `Plano` (§5.2): tudo que o executor roda e o relatório conta, sem E/S. Dono: P0.6a.
//! `PassoAudio` é montado pelo P0.6b; `Proveniencia` é lida pelo P0.R.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::edicao::camada::formas::QuadroCamada;
use crate::edicao::ffmpeg::sonda::Origem;
use crate::edicao::modelo::{
    Confirmacao, ConteudoOculto, CorMidia, Derivacao, Efeito, FaixaAnalise, FormaRegiao, ModoOcultar, ModoSilencio, Normalizar,
    OrigemLegendas, OrigemRastreio, Predefinicao, Racional, Recebido, SincroniaRef, TamanhoPx, Titulo, Transicao, Velocidade,
};
use crate::edicao::motor::Alvo;
use crate::edicao::quadros::Declarado;

/// Canvas da saída: px, taxa e cor de fundo ("#rrggbb").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub fps: Racional,
    pub cor_fundo: String,
}

/// Tudo que uma execução roda: `total_q` slots do canvas (o programa inteiro na exportação final; a janela na
/// prévia; 1 no quadro final). `alvo` = o de `compilar` (só a exportação final passa por "Conferindo" e confere
/// os pts lidos). Na prévia e no quadro final o canvas vem reduzido; a `proveniencia` é sempre a do programa
/// inteiro, no tamanho do projeto.
// CONTRATO: `alvo` não está no §5.2; o executor e o retrato precisam dele (P0.6a, P0.7 e o "refazer").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Plano {
    pub alvo: Alvo,
    pub canvas: Canvas,
    pub total_q: u64,
    /// Em ordem; produzem arquivos usados depois.
    pub previas: Vec<Previa>,
    /// Rasterizadas em paralelo antes dos blocos; bloco em cache dispensa as dele.
    pub camadas: Vec<PedidoCamada>,
    pub blocos: Vec<Bloco>,
    pub audio: Option<PassoAudio>,
    /// `.srt`, `.vtt`.
    pub legendas: Vec<ArquivoTexto>,
    pub juncao: Juncao,
    pub proveniencia: Proveniencia,
}

/// Passo anterior aos blocos. `args` = linha inteira depois do binário.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Previa {
    /// Mede o volume; o resultado vira o ganho do `PassoAudio`. `args` começam por `-cpuflags 0`
    /// e citam arquivos de `PassoAudio.auxiliares`.
    Loudnorm { args: Vec<String> },
    /// Só quando `trf_sha256` é None; roda com `OMP_NUM_THREADS=1` e o `.trf` vai para o projeto.
    /// `efeito` = id do efeito; `trf` = nome simples do arquivo gerado na pasta de trabalho.
    Vidstab { efeito: String, args: Vec<String>, trf: String },
}

/// Arquivo de texto auxiliar (grafo, `.ffconcat`, `.srt`): nome simples na pasta de trabalho, UTF-8 sem BOM.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArquivoTexto {
    pub nome: String,
    pub conteudo: String,
}

/// Um ffmpeg: slots [de_q, de_q + n_q). Linha (§5.8):
/// `ffmpeg <args::globais()> <entradas> <grafo em arquivo> -map [v] <codificador> <saida> -map [h] -fps_mode passthrough -f framehash -hash sha256 <framehash>`.
/// `grafo` = o TEXTO do grafo (quem roda grava num arquivo de nome simples e passa por `args::grafo_em_arquivo`);
/// nele as entradas são citadas pelo índice (`[3:v]`), na ordem de `entradas`, e não há nome de arquivo.
/// `codificador` já traz `-fps_mode passthrough`, as etiquetas de cor, os argumentos de reprodutibilidade e a escala
/// de tempo. `chave` = SHA-256 do bloco (grafo, argumentos, mídias, hash das camadas, ffmpeg);
/// `saida` = "b003.mp4" | "b003.mkv" | "b000.bmp"; `framehash` vazio = sem framehash (prévia do render e quadro final).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bloco {
    pub indice: usize,
    pub de_q: u64,
    pub n_q: u32,
    pub chave: String,
    pub entradas: Vec<Entrada>,
    pub grafo: String,
    pub auxiliares: Vec<ArquivoTexto>,
    pub codificador: Vec<String>,
    pub saida: String,
    pub framehash: String,
    pub conferir: Vec<ConferirClipe>,
}

/// O que conferir depois de rodar o bloco: `frames in` do `fps@<rotulo>` = `lidos` + 1 (a cadeia põe sempre um
/// clone do último quadro antes do mapa; `lidos` = quadros que saem da fonte); na exportação final, os pts do
/// `showinfo@<rotulo>` = `pts` (vazio fora dela). Divergência = o bloco falha e é refeito com `antes_sem_busca`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConferirClipe {
    pub rotulo: String,
    pub clipe: String,
    pub lidos: u32,
    pub pts: Vec<i64>,
}

/// Uma entrada do ffmpeg. `antes` = argumentos antes do `-i` (`-ss -t -framerate -f concat …`).
/// `antes_sem_busca` = os mesmos sem `-ss`, para refazer o bloco quando a leitura diverge (None = não usa busca).
/// Em `Papel::Camada` o `caminho` fica vazio: o executor põe a lista da camada rasterizada (`antes` =
/// `-f concat -safe 0`); se a camada saiu FIXA (um PNG só, sem lista: só se sabe depois de rasterizar), a entrada
/// é o PNG com `-loop 1 -framerate <fps do canvas>` no lugar de `antes`. O grafo corta toda camada no tamanho
/// dela, e serve aos dois casos.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entrada {
    pub antes: Vec<String>,
    pub antes_sem_busca: Option<Vec<String>>,
    pub caminho: PathBuf,
    pub papel: Papel,
}

/// `Midia.id` = `MidiaRef.id`; `Camada.id` = `PedidoCamada.id`; `Gerada` = fonte `lavfi`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Papel {
    Midia { id: String, clipe: String },
    Camada { id: String },
    Imagem,
    Gerada,
}

/// `Anotacao`: RGBA por slot, px do canvas. `MascaraCanvas`: cinza por slot. `MascaraFonte`: cinza por quadro lido, px da fonte exibida.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TipoCamada {
    Anotacao,
    MascaraCanvas,
    MascaraFonte,
}

/// Camada a rasterizar. `caixa_px` = [x, y, w, h] pares (px do canvas, ou da fonte exibida em `MascaraFonte`);
/// `n` = slots ou quadros lidos; `hash` = `camada::hash_do_pedido` (entra na chave do bloco);
/// `imagens` = arquivos de imagem que os itens citam por `MidiaRef.id`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PedidoCamada {
    pub id: String,
    pub tipo: TipoCamada,
    pub caixa_px: [u32; 4],
    pub n: u32,
    pub hash: String,
    pub itens: Vec<ItemCamada>,
    pub imagens: Vec<ImagemCamada>,
}

/// Imagem do caso ou do projeto usada numa camada. O `hash` do pedido leva o `sha256`, nunca o `caminho` (absoluto).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImagemCamada {
    pub midia: String,
    pub sha256: String,
    pub caminho: PathBuf,
    pub tamanho: TamanhoPx,
}

/// Um desenho da camada, de baixo para cima. `quadros[i]` vale no quadro `de + i` da camada; fora disso o item não aparece.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemCamada {
    pub desenho: Desenho,
    pub de: u32,
    pub quadros: Vec<QuadroItem>,
}

/// O que o item desenha. `Efeito` sai por `camada::formas::primitivas`; `Titulo` por `camada::titulo::primitivas`;
/// `Imagem` (sobreposta) = a de `PedidoCamada.imagens` com esse `MidiaRef.id`, posta pela transformação do quadro.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Desenho {
    Efeito { efeito: Efeito },
    Titulo { titulo: Titulo },
    Imagem { midia: String },
}

/// Estado do item num quadro: o quadro (rastreio avaliado + A(τ)) e, em sobreposto, a transformação avaliada.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuadroItem {
    pub quadro: QuadroCamada,
    pub transformacao: Option<TransformacaoAvaliada>,
}

/// `Transformacao` no tempo local do quadro: centro (0..1 do canvas), escala (1 = largura do canvas),
/// rotação em graus e opacidade 0..1 já com a entrada e a saída.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct TransformacaoAvaliada {
    pub x: f64,
    pub y: f64,
    pub escala: f64,
    pub rotacao: f64,
    pub opacidade: f64,
}

/// Onde `PassoAudio.grafo` recebe o ganho da normalização, em dB (o executor troca pelo número; sem normalizar, por 0).
pub const MARCA_GANHO: &str = "@GANHO_DB@";

/// Áudio do programa inteiro numa passada (§5.9). Linha:
/// `ffmpeg <globais> <entradas> <grafo em arquivo> -map [a] -c:a pcm_s16le <saida> -map [ah] -f hash -hash sha256 <hash>`.
/// `globais` começam SEMPRE por `-cpuflags 0` (decisão O6); uma entrada por mídia, repetida quando o som teria de esperar (`audio::distribuir`; `-vn`, sem `-ss`; `Papel::Midia`
/// com `clipe` vazio);
/// o grafo entrega `[a]` e `[ah]` (48 kHz, estéreo) com `amostras` amostras e contém `MARCA_GANHO`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PassoAudio {
    pub globais: Vec<String>,
    pub entradas: Vec<Entrada>,
    pub grafo: String,
    /// Grafos e listas que os argumentos citam (inclusive o da medição).
    pub auxiliares: Vec<ArquivoTexto>,
    /// "audio.wav" (PCM s16).
    pub saida: String,
    /// "audio.sha256" (hash do PCM).
    pub hash: String,
    pub amostras: u64,
    /// Alvo da normalização; None = só mede (para o aviso de pico).
    pub normalizar: Option<Normalizar>,
    /// Maior desvio do `atempo` entre os clipes, em ms (vai para `feito.audio`).
    pub tolerancia_atempo_ms: f64,
}

/// Fecho da exportação (§5.8).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Juncao {
    /// A saída é o arquivo do bloco único (quadro final).
    Direta,
    /// `ffmpeg -f concat -safe 0 -i <lista.nome> [-i <PassoAudio.saida>] <args> saida.<extensao>`.
    /// `lista` = `blocos.ffconcat` com as saídas dos blocos em ordem; `args` = mapas, `-c:v copy`, codec de áudio,
    /// `-movflags`, `-video_track_timescale`, bitexact (sem o nome da saída); `extensao` = "mp4" | "mkv" | "mov".
    Concat { lista: ArquivoTexto, args: Vec<String>, extensao: String },
}

// ---------------------------------------------------------------------------
// Proveniência: o que o `feito` do tecnico.json (§10.2) conta, calculado junto com o plano

/// Mídias, roteiro com o mapa de quadros por clipe, ocultações, rastreios, modelos de IA, ffmpeg e avisos
/// que pedem ciência. Sem caminho absoluto e sem `Rastreio.nome`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Proveniencia {
    pub projeto: ProvProjeto,
    pub saida: ProvSaida,
    pub midias: Vec<ProvMidia>,
    pub roteiro: Vec<ProvItem>,
    pub ocultacoes: Vec<ProvOcultacao>,
    pub audio: ProvAudio,
    pub legendas: ProvLegendas,
    pub sincronias: Vec<SincroniaRef>,
    pub rastreios: Vec<ProvRastreio>,
    pub ia: Vec<ProvIa>,
    pub estabilizacoes: Vec<ProvEstabilizacao>,
    /// Avisos periciais (código e contagem); o `ciente_em` vem de `Ambiente.ciencias`.
    pub avisos: Vec<ProvAviso>,
    pub selos_desligados: Vec<String>,
    pub revisado_por: String,
    pub ffmpeg: ProvFfmpeg,
}

/// O SHA-256 do `projeto.json` não está aqui (o compilador não vê o arquivo): vem do retrato.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvProjeto {
    pub id: String,
    pub titulo: String,
    pub esquema: u32,
    pub esquema_menor: u32,
}

/// Saída do programa inteiro no tamanho do projeto, para qualquer alvo; `predefinicao` = a da exportação do alvo
/// (na prévia e no quadro final, a do projeto).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvSaida {
    pub largura: u32,
    pub altura: u32,
    pub fps: Racional,
    pub duracao_s: f64,
    pub n_quadros: u64,
    pub predefinicao: Option<Predefinicao>,
    pub com_perdas: bool,
}

/// Medido pela tabela de quadros: passos em ticks de `tb`; `lacunas` = (pts antes, pts depois);
/// `de_s`/`ate_s` = tempo de fonte do primeiro quadro e do fim do último (a "cobertura" do PDF compara o usado com isso).
// CONTRATO: `de_s` e `ate_s` não estão no §10.2; sem eles só haveria a duração declarada pelo contêiner.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvMedido {
    pub tb: Racional,
    pub de_s: f64,
    pub ate_s: f64,
    pub pacotes: u64,
    pub quadros: u64,
    pub passo_min: i64,
    pub passo_med: i64,
    pub passo_max: i64,
    pub lacunas: Vec<(i64, i64)>,
}

/// Faixa em tempo de fonte (s).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ProvFaixa {
    pub de_s: f64,
    pub ate_s: f64,
}

/// `cor` = a usada no render (etiqueta, presunção ou escolha do perito); `giro` em graus.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvMidia {
    pub id: String,
    pub nome: String,
    pub sha256: String,
    pub derivacao: Option<Derivacao>,
    pub recebido: Recebido,
    pub relogio_desvio_s: Option<f64>,
    pub gravador: Option<String>,
    pub cor: CorMidia,
    pub conteiner: String,
    pub codec: String,
    pub pix_fmt: String,
    pub sar: Racional,
    pub giro: i32,
    pub inicio_s: f64,
    pub declarado: Declarado,
    pub medido: ProvMedido,
    pub discrepancias: Vec<String>,
    pub erros_decodificacao: u32,
    pub usado: Vec<ProvFaixa>,
    pub usado_total_s: f64,
}

/// Onde o item cai na saída: slots e segundos do programa.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ProvNaSaida {
    pub de_q: u64,
    pub n_q: u32,
    pub de_s: f64,
    pub ate_s: f64,
}

/// Constantes inteiras do mapa de quadros (§3.1).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvMapa {
    pub c: i64,
    pub kn: i64,
    pub kd: i64,
    pub e: i64,
}

/// Faixa de fonte do item. `quadro_do_slot[q]` = índice na tabela do quadro do slot q do clipe;
/// `pts` = pts dos quadros `j_in..j_out` (o que tem de ser lido). Os dois alimentam o `mapa.csv` e não vão
/// para o `feito.roteiro[].origem` do `tecnico.json` (lá ficam os campos do §10.2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvOrigem {
    pub midia: String,
    pub de_s: f64,
    pub ate_s: f64,
    pub pts_primeiro: i64,
    pub pts_ultimo: i64,
    pub lidos: u32,
    pub repetidos: u32,
    pub descartados: u32,
    pub mapa: ProvMapa,
    pub j_in: u32,
    pub quadro_do_slot: Vec<u32>,
    pub pts: Vec<i64>,
}

/// Um processamento aplicado, na ordem da cadeia; `parametros` = objeto JSON com os valores usados.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvProcessamento {
    pub ordem: u32,
    pub nome: String,
    pub parametros: serde_json::Value,
}

/// Item do roteiro. `n` começa em 1; `tipo` = literal do clipe ("trecho", "congelar"…); `nome` = `rotulo` do clipe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvItem {
    pub n: u32,
    pub clipe: String,
    pub nome: String,
    pub tipo: String,
    pub na_saida: ProvNaSaida,
    pub origem: Option<ProvOrigem>,
    pub velocidade: Option<Velocidade>,
    pub transicao: Option<Transicao>,
    pub processamentos: Vec<ProvProcessamento>,
    pub selos: Vec<String>,
}

/// Quadros cobertos por tipo de amostra.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvQuadrosOcultos {
    pub ancora: u32,
    pub medido: u32,
    pub interpolado: u32,
    pub incerto: u32,
}

/// Ocultação aplicada: tempo de fonte (s) e os clipes em que aparece.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvOcultacao {
    pub midia: String,
    pub clipes: Vec<String>,
    pub de_s: f64,
    pub ate_s: f64,
    pub modo: ModoOcultar,
    pub forma: FormaRegiao,
    pub intensidade: f64,
    pub conteudo: ConteudoOculto,
    pub motivo: String,
    pub quadros: ProvQuadrosOcultos,
    /// `alvo.inverter`: cobre o que está FORA da forma (contratos (g) 6).
    // CONTRATO: acréscimo da verificação da Fase 0; sem ele a invertida saía no `tecnico.json` como a comum.
    #[serde(default)]
    pub inverter: bool,
}

/// Trecho silenciado, em tempo de fonte (s).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvSilencio {
    pub midia: String,
    pub de_s: f64,
    pub ate_s: f64,
    pub modo: ModoSilencio,
    pub motivo: String,
}

/// `feito.audio`: trechos silenciados e o maior desvio do `atempo` (ms), do programa inteiro. O `tecnico.json`
/// acrescenta `tolerancia_buraco_ms` (contratos (h) 7) pelo passo de áudio do plano (`relatorio::tecnico`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvAudio {
    pub silencios: Vec<ProvSilencio>,
    pub tolerancia_atempo_ms: f64,
}

/// `feito.legendas`: origem, quantas e quantas conferidas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvLegendas {
    pub origem: Option<OrigemLegendas>,
    pub n: u32,
    pub conferidas: u32,
}

/// Rastreio usado (sem o nome). `corridas` = as do arquivo denso.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvRastreio {
    pub id: String,
    pub origem: OrigemRastreio,
    pub ancoras: u32,
    pub denso_sha256: Option<String>,
    pub silhuetas_sha256: Option<String>,
    pub corridas: Vec<serde_json::Value>,
    pub confirmacao: Option<Confirmacao>,
}

/// Arquivo de modelo: nome e SHA-256.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvArquivo {
    pub nome: String,
    pub sha256: String,
}

/// Modelo de IA que deixou marca no vídeo; `provedor` = CUDA ou CPU (§6.6: as saídas diferem); `uso` = para que serviu.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvIa {
    pub modelo: String,
    pub arquivos: Vec<ProvArquivo>,
    pub origem: String,
    pub commit: String,
    pub desvio: String,
    pub runtime: String,
    pub provedor: String,
    pub uso: String,
}

/// `feito.estabilizacoes`: efeito (id), `.trf`, faixa analisada e zoom fixo (%).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvEstabilizacao {
    pub efeito: String,
    pub trf_sha256: Option<String>,
    pub analise: FaixaAnalise,
    pub zoom_pct: f64,
}

/// Aviso pericial presente: código do §2.3 e contagem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvAviso {
    pub codigo: String,
    pub n: u64,
}

/// O ffmpeg vai por origem + SHA-256, nunca por caminho.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvFfmpeg {
    pub versao: String,
    pub configuracao: String,
    pub origem: Origem,
    pub sha256: String,
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};

    fn ida_e_volta<T: Serialize + DeserializeOwned>(j: Value) {
        let lido: T = serde_json::from_value(j.clone()).expect("lê");
        assert_eq!(serde_json::to_value(&lido).unwrap(), j);
    }

    #[test]
    fn previa_tag_tipo() {
        ida_e_volta::<Previa>(json!({ "tipo": "loudnorm", "args": ["-cpuflags", "0", "-i", "a.mp4"] }));
        ida_e_volta::<Previa>(json!({ "tipo": "vidstab", "efeito": "e1", "args": [], "trf": "e001.trf" }));
    }

    #[test]
    fn papel_tag_tipo() {
        ida_e_volta::<Papel>(json!({ "tipo": "midia", "id": "m1", "clipe": "c1" }));
        ida_e_volta::<Papel>(json!({ "tipo": "camada", "id": "k1" }));
        ida_e_volta::<Papel>(json!({ "tipo": "imagem" }));
        ida_e_volta::<Papel>(json!({ "tipo": "gerada" }));
    }

    #[test]
    fn juncao_tag_tipo() {
        ida_e_volta::<Juncao>(json!({ "tipo": "direta" }));
        ida_e_volta::<Juncao>(json!({ "tipo": "concat", "lista": { "nome": "blocos.ffconcat", "conteudo": "ffconcat version 1.0\n" },
                                       "args": ["-c:v", "copy"], "extensao": "mp4" }));
    }

    #[test]
    fn desenho_tag_tipo() {
        ida_e_volta::<Desenho>(json!({ "tipo": "efeito", "efeito": { "tipo": "nitidez", "id": "e1", "ligado": true, "nome": "", "ativo": null, "quantidade": 0.5 } }));
        ida_e_volta::<Desenho>(json!({ "tipo": "titulo", "titulo": { "modelo": "livre", "fundo": { "tipo": "transparente" }, "blocos": [] } }));
        ida_e_volta::<Desenho>(json!({ "tipo": "imagem", "midia": "m2" }));
        assert_eq!(serde_json::to_value(TipoCamada::MascaraFonte).unwrap(), json!("mascara_fonte"));
    }
}
