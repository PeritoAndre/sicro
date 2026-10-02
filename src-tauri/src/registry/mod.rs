//! Registro de evidências: projeção sob demanda das tabelas de cada módulo
//! (nenhuma tabela nova). Submódulos são folhas independentes: `build_summary`
//! é barato, `verify_workspace` com hash é caro.

pub mod aggregator;
pub mod broken_links;
pub mod integrity;
pub mod report;

pub use aggregator::{build_registry, build_summary};
pub use broken_links::detect_broken_laudo_links;
pub use integrity::verify_workspace;
pub use report::render_html_report;
