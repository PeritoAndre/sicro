//! Edição profissional (módulo `edicao`, arquitetura §5.1).
//! Dono de cada arquivo e decisões além da arquitetura: `contratos-fase0.md` e `contratos-fase1.md` no dossiê.
//! Assinatura pública e tipo do modelo só mudam pelo orquestrador (marcar com `// CONTRATO:`).
//! Códigos de erro (`mapa_grande`, `filtro_ausente`…) viajam em `SicroError::Validation("<código>")`.

pub mod armazem;
pub mod camada;
pub mod conferencia;
pub mod conferir;
pub mod cor;
pub mod curva;
pub mod executar;
pub mod ffmpeg;
pub mod fila;
pub mod fundo;
pub mod geometria;
pub mod importar_v1;
pub mod legendas;
pub mod medidas;
pub mod mensagens;
pub mod modelo;
pub mod motor;
pub mod ondas;
pub mod origem;
pub mod pacote;
pub mod previa;
pub mod proxy;
pub mod quadros;
pub mod rastreio;
pub mod refazer;
pub mod relatorio;
// Fase 4 (contratos-fase4.md): zoom que segue e entrada/saída dos sobrepostos (texto livre)
pub mod segue;
pub mod sobreposto;
pub mod tempo;
pub mod tiras;
pub mod validar;

#[cfg(test)]
pub mod testes;

// ponta a ponta do fecho da Fase 0: mora em `tests/` mas usa o que é `pub(crate)` e os medidores de `testes`
#[cfg(test)]
#[path = "../../tests/edicao/ponta_a_ponta.rs"]
mod ponta_a_ponta;

// verificação da Fase 0 (lente de contratos): projeto gravado pelo store × leitura, validação e render
#[cfg(test)]
#[path = "../../tests/edicao/contratos.rs"]
mod contratos_verif;
