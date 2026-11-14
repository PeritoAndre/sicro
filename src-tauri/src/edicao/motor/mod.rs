//! Compilador do render (§5.2, §5.3): projeto + contexto + alvo → `Plano`. Puro: sem disco, sem processo.
//! Dono: P0.6a (`audio.rs` é do P0.6b).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::ffmpeg::sonda::Capacidades;
use super::modelo::{EstadoMidia, Exportacao, Faixa, LinhaResolvida, Matriz, ProjetoEdicao, Racional};
use super::quadros::TabelaQuadros;
use super::rastreio::{ArquivoDenso, ArquivoSilhuetas};
use crate::error::Result;

pub mod audio;
pub mod blocos;
pub mod clipe;
pub mod efeitos;
pub mod expr;
pub mod grafo;
pub mod juncao;
pub mod plano;
pub mod saida;
pub mod sobrepor;

use plano::Plano;

/// Tudo que o compilador enxerga além do projeto. Montado por `executar::preparar`, o único lugar com E/S.
/// `fontes`: chave = `MidiaRef.id` (só as mídias achadas e íntegras); `midias` = situação de cada `MidiaRef`,
/// que a validação usa (`midia_ausente`, `midia_trocada`, `tabela_pendente`).
// CONTRATO: `midias` não está no §5.2; sem ele `compilar` (que valida) não tinha de onde tirar a situação das mídias.
pub struct Contexto<'a> {
    pub fontes: &'a HashMap<String, FonteInfo>,
    pub cap: &'a Capacidades,
    pub rastreios: &'a dyn LeitorRastreios,
    pub pasta_projeto: &'a Path,
    pub midias: &'a [EstadoMidia],
}

/// Leitura dos arquivos imutáveis de rastreio, por (id do rastreio, SHA-256 do ponteiro).
pub trait LeitorRastreios {
    /// Amostras densas de `rastreios/<rastreio>/<sha256>.json`.
    fn denso(&self, rastreio: &str, sha256: &str) -> Result<Arc<ArquivoDenso>>;
    /// Silhuetas de `silhuetas/<rastreio>/<sha256>.sil`.
    fn silhuetas(&self, rastreio: &str, sha256: &str) -> Result<Arc<ArquivoSilhuetas>>;
}

/// Trilha de áudio da fonte: início (s, tempo de fonte), taxa (Hz) e canais.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioInfo {
    pub inicio_s: f64,
    pub taxa: u32,
    pub canais: u32,
}

/// Uma mídia pronta para o compilador. `exibido` = (largura, altura) em px com giro e proporção do pixel
/// aplicados; `giro` em graus; `matriz` e `faixa` já decididas (etiqueta, presunção ou `MidiaRef.cor`).
/// Imagem: tabela de 1 quadro (pts 0, base 1/1). Arquivo só de áudio: tabela vazia, `exibido` (0, 0) e `audio` Some.
// CONTRATO: `conteiner` e `codec` não estão no §5.2; entram porque `feito.midias` (§10.2) e a escolha da prévia (§7.1) pedem.
#[derive(Debug, Clone)]
pub struct FonteInfo {
    pub caminho: PathBuf,
    pub sha256: String,
    pub inicio_s: f64,
    pub exibido: (u32, u32),
    pub sar: Racional,
    pub giro: i32,
    pub entrelacado: bool,
    pub pix_fmt: String,
    pub matriz: Matriz,
    pub faixa: Faixa,
    pub quadros: Arc<TabelaQuadros>,
    pub audio: Option<AudioInfo>,
    /// `format_name` do ffprobe.
    pub conteiner: String,
    /// `codec_name` do fluxo de vídeo.
    pub codec: String,
}

/// O que renderizar. `Previa`: altura em px e slots [de_q, ate_q). `Quadro`: slot `q` e largura em px.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Alvo {
    Final { exportacao: Exportacao },
    Previa { altura: u32, de_q: u64, ate_q: u64 },
    Quadro { q: u64, largura: u32 },
}

/// Compila o plano de render: resolve a linha, valida com `validar::validar(p, linha, ctx, Some(alvo))`
/// (havendo erro, `Err` sem plano), compila o vídeo e o áudio.
pub fn compilar(p: &ProjetoEdicao, ctx: &Contexto, alvo: &Alvo) -> Result<Plano> {
    todo!()
}

/// Só o vídeo (etapas 3 a 7 e 10 do §5.3), sem validar e sem áudio (`Plano.audio` None, `legendas` vazio).
/// É o que `compilar` chama no meio; existe para o motor de vídeo ser testável sem `validar` nem `audio`.
pub fn compilar_video(p: &ProjetoEdicao, linha: &LinhaResolvida, ctx: &Contexto, alvo: &Alvo) -> Result<Plano> {
    todo!()
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    #[test]
    fn alvo_tag_tipo() {
        let casos = vec![
            json!({ "tipo": "final", "exportacao": { "predefinicao": "juri", "codificador": "auto", "legendas": "nenhuma", "pacote": null,
                                                    "pdf": "a3", "revisado_por": "", "personalizada": null } }),
            json!({ "tipo": "previa", "altura": 540, "de_q": 0, "ate_q": 90 }),
            json!({ "tipo": "quadro", "q": 14000, "largura": 1920 }),
        ];
        for j in casos {
            let a: Alvo = serde_json::from_value(j.clone()).expect("lê");
            assert_eq!(serde_json::to_value(&a).unwrap(), j);
        }
    }
}
