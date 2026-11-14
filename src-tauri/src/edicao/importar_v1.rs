//! Importador v1 → v2 (§2.4): `Sistema/videos/montagens/<id>.json` vira `edicao/<id>/projeto.json`. Dono: P0.2.

use std::path::Path;

use rusqlite::Connection;
use serde_json::Value;

use super::modelo::{MidiaRef, ProjetoEdicao};
use crate::error::Result;

/// O que a conversão precisa de fora do arquivo v1.
pub trait FontesV1 {
    /// `MidiaRef` do vídeo do caso com esse `media_id`; None = mídia sumida.
    fn video(&self, media_id: &str) -> Option<MidiaRef>;
    /// `MidiaRef` (origem projeto) do áudio de narração em `relativo`.
    fn narracao(&self, relativo: &str) -> Option<MidiaRef>;
    /// pts do quadro exibido em `t` (s, tempo de fonte) na mídia `sha256`; None sem tabela.
    fn pts(&self, sha256: &str, t: f64) -> Option<i64>;
    /// (largura, altura) exibidas da mídia `sha256`, em px.
    fn tamanho(&self, sha256: &str) -> Option<(u32, u32)>;
}

/// Há um projeto v1 com esse id ainda não importado?
pub fn existe(ws: &Path, projeto_id: &str) -> bool {
    todo!()
}

/// Conversão pura, campo a campo (§2.4). `agora` em RFC 3339; `versao` = versão do SICRO (`gravado_por`).
pub fn converter(v1: &Value, fontes: &dyn FontesV1, agora: &str, versao: &str) -> Result<ProjetoEdicao> {
    todo!()
}

/// Lê o v1, converte, grava o v2 e renomeia o v1 para `<id>.json.v1`. Devolve o documento gravado.
pub fn importar(ws: &Path, conn: &Connection, projeto_id: &str) -> Result<Value> {
    todo!()
}
