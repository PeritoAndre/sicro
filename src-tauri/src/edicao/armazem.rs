//! Armazenamento do projeto (§4): `Sistema/videos/edicao/<id>/`. Dono: P0.2.
//! Só o front manda gravar o `projeto.json`; aqui ele é gravado como chegou (`Value`), de forma atômica.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::modelo::{MidiaRef, Problema, ProjetoEdicao, TipoMidia};
use crate::error::Result;

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

/// O id serve de nome de pasta? (`[A-Za-z0-9_-]{1,80}`)
pub fn id_valido(id: &str) -> bool {
    todo!()
}

/// Pasta do projeto: `<ws>/Sistema/videos/edicao/<id>/`.
pub fn pasta_projeto(ws: &Path, projeto_id: &str) -> PathBuf {
    todo!()
}

/// Projetos do caso (v2 e v1 ainda não importados), do mais recente para o mais antigo.
pub fn listar(ws: &Path) -> Result<Vec<ResumoProjeto>> {
    todo!()
}

/// Abre o projeto; importa do v1 se só houver o v1; faz `projeto.anterior.json` antes da primeira gravação da sessão.
pub fn abrir(ws: &Path, projeto_id: &str) -> Result<ProjetoAberto> {
    todo!()
}

/// Leitura tipada de um documento (tolera item desconhecido).
pub fn tipado(projeto: &Value) -> Result<ProjetoEdicao> {
    todo!()
}

/// Grava o documento como chegou (`.tmp` + rename), carimbando só `updated_at` e `gravado_por`.
pub fn salvar(ws: &Path, projeto: &Value) -> Result<Salvo> {
    todo!()
}

/// Apaga a pasta do projeto MENOS `exportacoes/` (O7) e o cache dele (`cache` = pasta da Edição do caso). Audita.
pub fn excluir(ws: &Path, projeto_id: &str, cache: Option<&Path>) -> Result<()> {
    todo!()
}

/// Copia o projeto com id novo (rastreios, `.trf` e mídia própria juntos; exportações não) e devolve o resumo da cópia.
pub fn duplicar(ws: &Path, projeto_id: &str) -> Result<ResumoProjeto> {
    todo!()
}

/// Copia um arquivo para `midia/<uuid>.<ext>` do projeto, calcula o SHA-256 e devolve a `MidiaRef` (`origem: projeto`).
pub fn importar_midia(ws: &Path, projeto_id: &str, origem: &Path, tipo: TipoMidia) -> Result<MidiaRef> {
    todo!()
}

/// Grava os bytes de uma narração em `midia/<uuid>.<ext>` (extensão pelo `mime`) e devolve a `MidiaRef`.
pub fn gravar_midia(ws: &Path, projeto_id: &str, mime: &str, dados: &[u8]) -> Result<MidiaRef> {
    todo!()
}

/// Projetos e exportações que usam a mídia com esse SHA-256.
pub fn usos(ws: &Path, sha256: &str) -> Result<Vec<UsoMidia>> {
    todo!()
}

/// Apaga densos, silhuetas e `.trf` do projeto que o documento não cita mais (ao fechar o projeto).
/// `manter` = SHA-256 ainda vivos no histórico da tela.
pub fn limpar_orfaos(ws: &Path, projeto: &ProjetoEdicao, manter: &[String]) -> Result<u32> {
    todo!()
}
