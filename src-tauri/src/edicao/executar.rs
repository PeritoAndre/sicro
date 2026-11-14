//! Roda um `Plano` (§5.2, §5.8): prévias, camadas, blocos, áudio, junção e conferência. Dono: P0.7.
//! `preparar` é o único lugar com E/S antes do render (banco, tabelas, sonda, leitor de rastreios).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::camada::ArquivoCamada;
use super::conferir::ConferenciaSaida;
use super::ffmpeg::exec::Termino;
use super::ffmpeg::sonda::Capacidades;
use super::fila::Fase;
use super::modelo::{EstadoMidia, ProjetoEdicao};
use super::motor::audio::Loudnorm;
use super::motor::plano::Plano;
use super::motor::{Contexto, FonteInfo, LeitorRastreios};
use crate::error::Result;

/// Prefixos de `SicroError::Validation` com que `executar` classifica a falha.
/// `ERRO_GPU`: a fila recompila com `EscolhaCodificador::Cpu` e refaz tudo (um codificador por exportação).
pub const ERRO_GPU: &str = "falha_gpu";
pub const ERRO_CANCELADO: &str = "cancelado";
pub const ERRO_LEITURA_DIVERGENTE: &str = "leitura_divergente";

/// Retrato do perito (`execucao.autor`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Autor {
    pub nome: String,
    pub matricula: String,
    pub cargo: String,
}

/// Retrato do caso (`execucao.caso`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RetratoCaso {
    pub numero: String,
    pub referencia: String,
    pub orgao: String,
    pub requisitante: String,
}

/// `execucao.sicro`: versão, commit e alvo de compilação.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VersaoSicro {
    pub versao: String,
    pub commit: String,
    pub alvo: String,
}

/// `instrucoes` = conjuntos de instruções da CPU (avx2, avx512f…): definem a classe de máquina.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Cpu {
    pub modelo: String,
    pub instrucoes: Vec<String>,
}

/// `execucao.sistema`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Sistema {
    pub so: String,
    pub cpu: Cpu,
    pub gpu: Option<String>,
    pub driver: Option<String>,
}

/// Quem, onde e com quê: a parte de `execucao` que não depende do render.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Identidade {
    pub autor: Autor,
    pub caso: RetratoCaso,
    pub sicro: VersaoSicro,
    pub sistema: Sistema,
}

/// Ciência do perito num aviso pericial: código do §2.3 e quando (RFC 3339).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ciencia {
    pub codigo: String,
    pub ciente_em: String,
}

/// Onde e com quê o plano roda. `trabalho` = `render/<tarefa>/` (é o `current_dir` dos processos);
/// `cache` = pasta da Edição do caso no cache; `caso` = raiz do caso; `pasta_projeto` = `Sistema/videos/edicao/<id>/`
/// (destino dos `.trf`); `paralelos` = blocos ao mesmo tempo.
#[derive(Debug, Clone)]
pub struct Ambiente {
    pub cap: Arc<Capacidades>,
    pub trabalho: PathBuf,
    pub cache: PathBuf,
    pub caso: PathBuf,
    pub pasta_projeto: PathBuf,
    pub paralelos: u32,
    pub identidade: Identidade,
    pub ciencias: Vec<Ciencia>,
}

/// Avanço do render: fase, slots feitos e totais, quadros por segundo e segundos que faltam.
/// É o evento `edicao:progresso` sem o `tarefa_id` (`fila::EventoProgresso`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Progresso {
    pub fase: Fase,
    pub feito_q: u64,
    pub total_q: u64,
    pub qps: f32,
    pub restante_s: u32,
}

/// Bloco rodado (`execucao.blocos`): `reaproveitado` = veio do cache; `sha256` = do arquivo do bloco.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlocoFeito {
    pub indice: usize,
    pub de_q: u64,
    pub n_q: u32,
    pub chave: String,
    pub reaproveitado: bool,
    pub sha256: String,
}

/// pts lidos de um clipe na exportação final (do `showinfo`), na ordem: alimentam o `mapa.csv`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LidosClipe {
    pub clipe: String,
    pub pts: Vec<i64>,
}

/// `.trf` gerado por uma prévia `Vidstab`: já copiado para `vidstab/<sha256>.trf` do projeto.
/// O front grava `trf_sha256` e `zoom_pct` no efeito por `alterar()`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrfNovo {
    pub efeito: String,
    pub sha256: String,
    pub zoom_pct: f64,
}

/// O que o render produziu (a parte `execucao` do `tecnico.json` que varia, §10.2).
/// Caminhos absolutos, na pasta de trabalho: a fila leva à pasta da exportação.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Resultado {
    /// Vídeo final (ou o BMP do quadro final).
    pub arquivo: PathBuf,
    pub sha256: String,
    /// Bytes.
    pub tamanho: u64,
    /// Nome do codificador de vídeo usado e os argumentos dele.
    pub codificador: String,
    pub argumentos: Vec<String>,
    /// Motivo da queda de GPU para CPU; None = não houve.
    pub queda_para_cpu: Option<String>,
    /// Hash do composto (coluna de hash dos `framehash`, na ordem dos slots); None sem framehash.
    pub composto_sha256: Option<String>,
    /// Arquivo `quadros.compostos.sha256` (um hash por slot); None sem framehash.
    pub compostos: Option<PathBuf>,
    /// SHA-256 do PCM (`audio.wav`); None sem passo de áudio.
    pub audio_sha256: Option<String>,
    /// Medição do volume e ganho aplicado (dB).
    pub loudnorm: Option<Loudnorm>,
    pub ganho_db: Option<f64>,
    pub blocos: Vec<BlocoFeito>,
    pub camadas: Vec<ArquivoCamada>,
    pub passos: Vec<Termino>,
    /// Só na exportação final.
    pub conferencia: Option<ConferenciaSaida>,
    pub lidos: Vec<LidosClipe>,
    /// Arquivos de legenda gravados.
    pub legendas: Vec<PathBuf>,
    pub trf: Vec<TrfNovo>,
    /// RFC 3339.
    pub renderizado_em: String,
}

/// Dados lidos do disco para compilar e analisar; `contexto()` empresta tudo ao compilador e à validação.
/// `midias` = situação de cada `MidiaRef` (só as `Ok` entram em `fontes`, com a chave `MidiaRef.id`).
pub struct Preparado {
    pub fontes: HashMap<String, FonteInfo>,
    pub cap: Arc<Capacidades>,
    pub rastreios: Box<dyn LeitorRastreios + Send + Sync>,
    pub pasta_projeto: PathBuf,
    pub midias: Vec<EstadoMidia>,
}

impl Preparado {
    /// Contexto do compilador sobre estes dados.
    pub fn contexto(&self) -> Contexto<'_> {
        Contexto {
            fontes: &self.fontes,
            cap: &self.cap,
            rastreios: self.rastreios.as_ref(),
            pasta_projeto: &self.pasta_projeto,
            midias: &self.midias,
        }
    }
}

/// Lê banco, tabelas de quadros, sonda e rastreios do projeto. `ws` = raiz do caso; `cache` = pasta da Edição do caso.
/// `conferir_sha256` = recalcula o SHA-256 de cada mídia (sempre na exportação final; uma vez por sessão no resto).
pub fn preparar(ws: &Path, p: &ProjetoEdicao, cache: &Path, conferir_sha256: bool) -> Result<Preparado> {
    todo!()
}

/// Roda o plano até o arquivo final na pasta de trabalho. Cancelar não deixa saída nem bloco truncado no cache.
/// `progresso` é chamado até 5 vezes por segundo.
pub fn executar(plano: &Plano, amb: &Ambiente, cancelar: &AtomicBool, progresso: &mut dyn FnMut(Progresso)) -> Result<Resultado> {
    todo!()
}
