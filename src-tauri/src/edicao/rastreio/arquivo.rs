//! Denso imutável e endereçado pelo conteúdo (§6.1): `rastreios/<rid>/<sha256>.json` na pasta do projeto. Corrida,
//! "Apagar daqui em diante" e "Simplificar" gravam um arquivo NOVO e devolvem o ponteiro; quem troca o ponteiro é o
//! front (`alterar()`). Dono: P2.2. Abertura: recusa `nao_suportado: rastreio` até o P2.2 entregar.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{recusa, ArquivoDenso, Corrida, ResultadoCorrida};
use crate::edicao::modelo::{Caixa, Chave, Denso, Racional, Rastreio};
use crate::error::Result;

/// Esquema do arquivo denso (§6.1).
pub const ESQUEMA_DENSO: u32 = 2;
/// Teto do arquivo lido do disco (10 min a 30 qps ≈ 1 MB; 2 h a 60 qps cabe com folga).
pub const MAX_BYTES_DENSO: u64 = 256 << 20;

/// Edição de um denso (`edicao_rastreio_editar`). Faixas por pts do quadro (o denso não guarda a base de tempo);
/// `ate_pts` None = até o fim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum OpRastreio {
    /// "Apagar daqui em diante": tira as amostras (e as âncoras) da faixa, inclusive as pontas.
    ApagarFaixa { de_pts: i64, ate_pts: Option<i64> },
    /// "Simplificar": denso → âncoras (Douglas-Peucker em x, y, w, h; `tolerancia` = fração do lado médio, mínimo 2 px).
    Simplificar { tolerancia: f64 },
}

/// Resultado de uma edição: o front grava `ancoras` quando vem e `denso` SEMPRE (null = rastreio sem denso).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Editado {
    pub ancoras: Option<Vec<Chave<Caixa>>>,
    pub denso: Option<Denso>,
}

/// `<pasta do projeto>/rastreios/<rid>/<sha256>.json`. `rastreio` e `sha256` chegam da tela (`edicao_rastreio_ler`) e do
/// documento: `ler`, `gravar` e `editar` recusam com `item_invalido` o id fora de `armazem::id_valido` e o SHA-256
/// que não seja 64 hex minúsculos, antes de montar o caminho.
pub fn caminho(pasta_projeto: &Path, rastreio: &str, sha256: &str) -> PathBuf {
    pasta_projeto.join("rastreios").join(rastreio).join(format!("{sha256}.json"))
}

/// Lê e confere o SHA-256 dos bytes; ausente ou diferente = `rastreio_ausente`.
pub fn ler(pasta_projeto: &Path, rastreio: &str, sha256: &str) -> Result<ArquivoDenso> {
    let _ = (pasta_projeto, rastreio, sha256);
    Err(recusa("rastreio"))
}

/// Grava o arquivo (nome = SHA-256 do texto; já existente não é regravado) e devolve o ponteiro com as contagens.
/// `tb` = base da tabela do decodificador (para `de_s`/`ate_s`).
pub fn gravar(pasta_projeto: &Path, d: &ArquivoDenso, tb: Racional) -> Result<Denso> {
    let _ = (pasta_projeto, d, tb);
    Err(recusa("rastreio"))
}

/// Junta uma corrida a um denso (ou a nenhum): a corrida substitui só a faixa que ela percorreu ("Retomar");
/// o resto fica byte a byte. Entre duas âncoras com corridas dos dois lados, `fundir`.
pub fn juntar(base: Option<&ArquivoDenso>, res: &ResultadoCorrida) -> Result<ArquivoDenso> {
    let _ = (base, res);
    Err(recusa("rastreio"))
}

/// As corridas tipadas do arquivo.
pub fn corridas(d: &ArquivoDenso) -> Result<Vec<Corrida>> {
    let _ = d;
    Err(recusa("rastreio"))
}

/// Aplica `op` ao rastreio `rastreio_id` do documento da tela (D2): denso (lido de `denso.sha256`) e âncoras;
/// grava o arquivo novo quando há. A base de tempo vem da tabela do decodificador da mídia (`cache` do caso).
pub fn editar(ws: &Path, cache: &Path, projeto: &serde_json::Value, rastreio_id: &str, op: &OpRastreio) -> Result<Editado> {
    let _ = (ws, cache, projeto, rastreio_id, op);
    Err(recusa("rastreio"))
}

/// O núcleo de `editar`, sem disco: âncoras e denso novos de `r` (None = sem mudança nas âncoras; denso None =
/// rastreio sem denso).
pub fn aplicar(r: &Rastreio, denso: Option<&ArquivoDenso>, op: &OpRastreio) -> Result<(Option<Vec<Chave<Caixa>>>, Option<ArquivoDenso>)> {
    let _ = (r, denso, op);
    Err(recusa("rastreio"))
}
