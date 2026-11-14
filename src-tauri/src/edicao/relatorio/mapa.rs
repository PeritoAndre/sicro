//! `mapa.csv` (§10.1): q, clipe, midia, pts, indice, tipo (novo | repetido), e os quadros descartados por clipe. Dono: P0.R.

use crate::edicao::executar::Resultado;
use crate::edicao::motor::plano::Plano;
use crate::error::Result;

/// Texto do `mapa.csv` (UTF-8, `\n`). Com `res`, exige que os pts lidos sejam os previstos; sem ele, só a previsão do plano.
pub fn mapa_csv(plano: &Plano, res: Option<&Resultado>) -> Result<String> {
    todo!()
}
