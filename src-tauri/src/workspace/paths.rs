//! Caminhos do workspace `.sicro`: nome da pasta, layout (Exportações / Mídias /
//! Sistema) e leitura de caminhos relativos antigos.

use std::path::{Path, PathBuf};

use chrono::{Datelike, NaiveDate};

use crate::error::{Result, SicroError};
use crate::filesystem::sanitize_folder_name;

// ---------------------------------------------------------------------------
// Layout 2: três pastas na raiz. Os `REL_*` são o que se grava no banco/JSON
// (relativo à raiz do caso, sempre com `/`).

pub const LAYOUT_ATUAL: u32 = 2;

pub const DIR_EXPORTACOES: &str = "Exportações";
pub const DIR_MIDIAS: &str = "Mídias";
pub const DIR_SISTEMA: &str = "Sistema";

pub const REL_MANIFEST: &str = "Sistema/manifest.json";
pub const REL_SQLITE: &str = "Sistema/sicro.sqlite";
pub const REL_CACHE: &str = "Sistema/cache";
pub const REL_LOGS: &str = "Sistema/logs";
pub const REL_DOSSIE: &str = "Sistema/dossie";
pub const REL_LAUDOS: &str = "Sistema/laudos";
pub const REL_LAUDOS_ASSETS: &str = "Sistema/laudos/assets";
pub const REL_IMPORTS: &str = "Sistema/imports";

pub const REL_CROQUIS_DOCS: &str = "Sistema/croquis";
pub const REL_CROQUIS_BACKGROUNDS: &str = "Sistema/croquis/backgrounds";
pub const REL_CROQUIS_THUMBS: &str = "Sistema/croquis/thumbs";
pub const REL_CROQUIS_EXPORTS: &str = "Exportações/Croquis";

pub const REL_VIDEOS_ORIGINAIS: &str = "Mídias/Vídeos";
pub const REL_VIDEOS_FRAMES: &str = "Sistema/videos/storyboards/frames";
pub const REL_VIDEOS_TRECHOS: &str = "Exportações/Vídeos/Trechos";
pub const REL_MONTAGENS_JSON: &str = "Sistema/videos/montagens";
pub const REL_MONTAGENS_SAIDA: &str = "Exportações/Vídeos/Montagens";
pub const REL_PESSOAS: &str = "Sistema/videos/pessoas";
pub const REL_REALCE: &str = "Sistema/videos/realce";
pub const REL_ESTABILIZADOS: &str = "Sistema/videos/estabilizados";
/// Projetos da Edição (só dado; entra no backup).
pub const REL_EDICAO: &str = "Sistema/videos/edicao";
pub const REL_EDICAO_SAIDA: &str = "Exportações/Vídeos/Edições";
/// Pasta da Edição dentro de `app_cache_dir()`: fora do caso e do backup.
pub const DIR_EDICAO_CACHE: &str = "edicao";

pub const REL_IMAGENS_ORIGINAIS: &str = "Mídias/Imagens";
pub const REL_IMAGENS_ANALISES: &str = "Sistema/imagens/analises";
pub const REL_IMAGENS_CAMADAS: &str = "Sistema/imagens/camadas";
pub const REL_IMAGENS_REALCE: &str = "Sistema/imagens/realce";
pub const REL_IMAGENS_EXPORTS: &str = "Exportações/Imagens";
pub const REL_IMAGENS_RELATORIOS: &str = "Exportações/Imagens/relatorios";

pub const REL_ANEXOS: &str = "Mídias/Anexos";
pub const REL_FOTOS_IMPORTADAS: &str = "Mídias/Fotos";
pub const REL_AUDIO_ORIGINAIS: &str = "Mídias/Áudios";
pub const REL_AUDIO_WAV: &str = "Sistema/audio/wav";
pub const REL_AUDIO_ESPECTROGRAMAS: &str = "Sistema/audio/espectrogramas";

pub const REL_RELATORIOS: &str = "Exportações/Relatórios";
pub const REL_BACKUPS: &str = "Exportações/Backups";

/// Prefixos do layout 1 (antigo) → layout 2. Do mais específico para o mais
/// geral: a primeira entrada que casa (em fronteira de componente) vale.
pub const PREFIXOS_ANTIGOS: &[(&str, &str)] = &[
    ("croquis/exports", REL_CROQUIS_EXPORTS),
    ("croquis", REL_CROQUIS_DOCS),
    ("videos/trechos", REL_VIDEOS_TRECHOS),
    ("videos/originais", REL_VIDEOS_ORIGINAIS),
    ("videos/media", "Mídias/Vídeos/media"),
    ("videos", "Sistema/videos"),
    ("imagens/originais", REL_IMAGENS_ORIGINAIS),
    ("imagens/tratadas", "Exportações/Imagens/tratadas"),
    ("imagens/anotadas", "Exportações/Imagens/anotadas"),
    ("imagens/exports", REL_IMAGENS_EXPORTS),
    ("imagens/relatorios", REL_IMAGENS_RELATORIOS),
    ("imagens", "Sistema/imagens"),
    ("midias/anexos", REL_ANEXOS),
    ("midias", DIR_MIDIAS),
    ("media/photos", REL_FOTOS_IMPORTADAS),
    ("audio/originais", REL_AUDIO_ORIGINAIS),
    ("audio", "Sistema/audio"),
    ("exports", "Exportações/Laudos"),
    ("reports", REL_RELATORIOS),
    ("backups", REL_BACKUPS),
    ("imports", REL_IMPORTS),
    ("cache", REL_CACHE),
    ("logs", REL_LOGS),
    ("dossie", REL_DOSSIE),
    ("laudos", REL_LAUDOS),
    ("manifest.json", REL_MANIFEST),
    ("sicro.sqlite", REL_SQLITE),
    ("sicro.sqlite-wal", "Sistema/sicro.sqlite-wal"),
    ("sicro.sqlite-shm", "Sistema/sicro.sqlite-shm"),
];

/// Mapeia um relativo antigo para o novo (só texto, não toca o disco).
/// `videos/montagens/*`: o JSON da montagem vai para Sistema, o resto
/// (.mp4, .relatorio.json) para Exportações. `None` = já não é caminho antigo.
pub fn mapear_prefixo_antigo(rel: &str) -> Option<String> {
    let rel = rel.replace('\\', "/");
    let rel = rel.trim_start_matches("./");
    if let Some(resto) = rel.strip_prefix("videos/montagens/") {
        let destino = if resto.ends_with(".json") && !resto.ends_with(".relatorio.json") {
            REL_MONTAGENS_JSON
        } else {
            REL_MONTAGENS_SAIDA
        };
        return Some(format!("{destino}/{resto}"));
    }
    for (antigo, novo) in PREFIXOS_ANTIGOS {
        if rel == *antigo {
            return Some((*novo).to_string());
        }
        if let Some(resto) = rel.strip_prefix(antigo) {
            if let Some(resto) = resto.strip_prefix('/') {
                return Some(format!("{novo}/{resto}"));
            }
        }
    }
    None
}

/// Caminho absoluto de um relativo gravado (novo ou antigo): usa como está se
/// existir; senão tenta o equivalente do layout novo. Se nada existe, devolve o
/// caminho "como está" (quem chama trata a ausência).
pub fn resolver_relativo(ws: &Path, rel: impl AsRef<str>) -> PathBuf {
    let rel = rel.as_ref();
    let direto = ws.join(rel);
    if direto.exists() {
        return direto;
    }
    match mapear_prefixo_antigo(rel) {
        Some(novo) => {
            let p = ws.join(novo);
            if p.exists() {
                p
            } else {
                direto
            }
        }
        None => direto,
    }
}

fn sob(ws: &Path, rel: &str) -> PathBuf {
    ws.join(rel)
}

pub fn dir_sistema(ws: &Path) -> PathBuf {
    sob(ws, DIR_SISTEMA)
}
pub fn dir_exportacoes(ws: &Path) -> PathBuf {
    sob(ws, DIR_EXPORTACOES)
}
pub fn dir_midias(ws: &Path) -> PathBuf {
    sob(ws, DIR_MIDIAS)
}

/// Layout antigo = manifesto na raiz e nada em Sistema/.
pub fn layout_antigo(ws: &Path) -> bool {
    !ws.join(REL_MANIFEST).is_file() && ws.join("manifest.json").is_file()
}

/// Manifesto do caso. Layout antigo ainda não migrado é lido no lugar velho.
pub fn manifest_path(ws: &Path) -> PathBuf {
    if layout_antigo(ws) {
        ws.join("manifest.json")
    } else {
        sob(ws, REL_MANIFEST)
    }
}

pub fn sqlite_path(ws: &Path) -> PathBuf {
    if layout_antigo(ws) && ws.join("sicro.sqlite").exists() {
        ws.join("sicro.sqlite")
    } else {
        sob(ws, REL_SQLITE)
    }
}

pub fn dir_cache(ws: &Path) -> PathBuf {
    sob(ws, REL_CACHE)
}
pub fn dir_logs(ws: &Path) -> PathBuf {
    sob(ws, REL_LOGS)
}
pub fn dir_dossie(ws: &Path) -> PathBuf {
    sob(ws, REL_DOSSIE)
}
pub fn dir_laudos(ws: &Path) -> PathBuf {
    sob(ws, REL_LAUDOS)
}
pub fn dir_laudos_assets(ws: &Path) -> PathBuf {
    sob(ws, REL_LAUDOS_ASSETS)
}
pub fn dir_imports(ws: &Path) -> PathBuf {
    sob(ws, REL_IMPORTS)
}
pub fn dir_croquis_docs(ws: &Path) -> PathBuf {
    sob(ws, REL_CROQUIS_DOCS)
}
pub fn dir_croquis_backgrounds(ws: &Path) -> PathBuf {
    sob(ws, REL_CROQUIS_BACKGROUNDS)
}
pub fn dir_croquis_thumbs(ws: &Path) -> PathBuf {
    sob(ws, REL_CROQUIS_THUMBS)
}
pub fn dir_croquis_exports(ws: &Path) -> PathBuf {
    sob(ws, REL_CROQUIS_EXPORTS)
}
pub fn dir_videos_originais(ws: &Path) -> PathBuf {
    sob(ws, REL_VIDEOS_ORIGINAIS)
}
pub fn dir_videos_frames(ws: &Path) -> PathBuf {
    sob(ws, REL_VIDEOS_FRAMES)
}
pub fn dir_videos_trechos(ws: &Path) -> PathBuf {
    sob(ws, REL_VIDEOS_TRECHOS)
}
pub fn dir_montagens_json(ws: &Path) -> PathBuf {
    sob(ws, REL_MONTAGENS_JSON)
}
pub fn dir_montagens_saida(ws: &Path) -> PathBuf {
    sob(ws, REL_MONTAGENS_SAIDA)
}
pub fn dir_imagens_originais(ws: &Path) -> PathBuf {
    sob(ws, REL_IMAGENS_ORIGINAIS)
}
pub fn dir_anexos(ws: &Path) -> PathBuf {
    sob(ws, REL_ANEXOS)
}
pub fn dir_fotos_importadas(ws: &Path) -> PathBuf {
    sob(ws, REL_FOTOS_IMPORTADAS)
}
pub fn dir_audio_originais(ws: &Path) -> PathBuf {
    sob(ws, REL_AUDIO_ORIGINAIS)
}
pub fn dir_audio_wav(ws: &Path) -> PathBuf {
    sob(ws, REL_AUDIO_WAV)
}
pub fn dir_audio_espectrogramas(ws: &Path) -> PathBuf {
    sob(ws, REL_AUDIO_ESPECTROGRAMAS)
}
pub fn dir_pessoas(ws: &Path) -> PathBuf {
    sob(ws, REL_PESSOAS)
}
pub fn dir_realce(ws: &Path) -> PathBuf {
    sob(ws, REL_REALCE)
}
pub fn dir_estabilizados(ws: &Path) -> PathBuf {
    sob(ws, REL_ESTABILIZADOS)
}
pub fn dir_imagens_analises(ws: &Path) -> PathBuf {
    sob(ws, REL_IMAGENS_ANALISES)
}
pub fn dir_imagens_camadas(ws: &Path) -> PathBuf {
    sob(ws, REL_IMAGENS_CAMADAS)
}
pub fn dir_imagens_realce(ws: &Path) -> PathBuf {
    sob(ws, REL_IMAGENS_REALCE)
}
pub fn dir_backups(ws: &Path) -> PathBuf {
    sob(ws, REL_BACKUPS)
}
pub fn dir_edicao(ws: &Path) -> PathBuf {
    sob(ws, REL_EDICAO)
}
pub fn dir_edicao_saida(ws: &Path) -> PathBuf {
    sob(ws, REL_EDICAO_SAIDA)
}
/// `app_cache_dir()/edicao`.
pub fn dir_edicao_cache_raiz(app_cache: &Path) -> PathBuf {
    app_cache.join(DIR_EDICAO_CACHE)
}
/// `app_cache_dir()/edicao/<id do caso>`. Id tipado: nada de texto do usuário no caminho.
pub fn dir_edicao_cache(app_cache: &Path, caso_id: &uuid::Uuid) -> PathBuf {
    dir_edicao_cache_raiz(app_cache).join(caso_id.to_string())
}

/// Pastas efêmeras que ficam fora do backup (layout novo e antigo).
pub const PASTAS_FORA_DO_BACKUP: &[&str] =
    &[REL_CACHE, REL_LOGS, REL_BACKUPS, "cache", "logs", "backups"];

/// `rel` (diretório, relativo à raiz do caso, com `/`) fica fora do backup?
pub fn fora_do_backup(rel: &str) -> bool {
    PASTAS_FORA_DO_BACKUP.contains(&rel)
}

/// Pastas que todo caso novo ganha.
pub const PASTAS_DO_CASO: &[&str] = &[
    REL_DOSSIE,
    REL_LAUDOS,
    REL_LAUDOS_ASSETS,
    REL_CROQUIS_DOCS,
    REL_CROQUIS_EXPORTS,
    REL_VIDEOS_ORIGINAIS,
    REL_VIDEOS_FRAMES,
    REL_VIDEOS_TRECHOS,
    REL_MONTAGENS_JSON,
    REL_MONTAGENS_SAIDA,
    REL_EDICAO,
    REL_EDICAO_SAIDA,
    REL_IMAGENS_ORIGINAIS,
    "Exportações/Imagens/tratadas",
    "Exportações/Imagens/anotadas",
    REL_IMAGENS_ANALISES,
    REL_ANEXOS,
    "Exportações/Laudos/docx",
    "Exportações/Laudos/pdf",
    "Exportações/Laudos/png",
    "Exportações/Laudos/pacotes",
    REL_LOGS,
    REL_CACHE,
    "Sistema/cache/thumbnails",
    "Sistema/cache/previews",
];

/// Nome da pasta do caso: o título; sem título, "Caso de DD-MM-AAAA".
/// (Barra não vale em nome de arquivo, então a data usa traço.)
pub fn derive_workspace_name(titulo: Option<&str>, hoje: NaiveDate) -> String {
    if let Some(t) = titulo.map(str::trim).filter(|t| !t.is_empty()) {
        return sanitize_folder_name(t);
    }
    format!(
        "Caso de {:02}-{:02}-{:04}",
        hoje.day(),
        hoje.month(),
        hoje.year()
    )
}

/// Caminho novo que ainda não existe: `Nome.sicro`, depois `Nome.2.sicro`…
pub fn unique_workspace_path(parent: &Path, base_name: &str) -> Result<PathBuf> {
    let mut candidate = parent.join(format!("{base_name}.sicro"));
    let mut suffix = 1;
    while candidate.exists() {
        suffix += 1;
        candidate = parent.join(format!("{base_name}.{suffix}.sicro"));
        if suffix > 999 {
            return Err(SicroError::Workspace(format!(
                "could not find unique workspace name under {}",
                parent.display()
            )));
        }
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nome_com_titulo_e_sem() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 13).unwrap();
        assert_eq!(derive_workspace_name(Some(" Acidente BR-156 "), d), "Acidente BR-156");
        assert_eq!(derive_workspace_name(Some("a/b"), d), "a_b");
        assert_eq!(derive_workspace_name(None, d), "Caso de 13-11-2026");
        assert_eq!(derive_workspace_name(Some("  "), d), "Caso de 13-11-2026");
    }

    #[test]
    fn colisao_ganha_sufixo_antes_do_sicro() {
        let tmp = tempfile::tempdir().unwrap();
        let a = unique_workspace_path(tmp.path(), "Caso").unwrap();
        assert!(a.ends_with("Caso.sicro"));
        std::fs::create_dir(&a).unwrap();
        let b = unique_workspace_path(tmp.path(), "Caso").unwrap();
        assert!(b.ends_with("Caso.2.sicro"));
        std::fs::create_dir(&b).unwrap();
        assert!(unique_workspace_path(tmp.path(), "Caso").unwrap().ends_with("Caso.3.sicro"));
    }

    #[test]
    fn pastas_da_edicao() {
        let ws = Path::new("/casos/A.sicro");
        assert_eq!(dir_edicao(ws), Path::new("/casos/A.sicro/Sistema/videos/edicao"));
        assert_eq!(dir_edicao_saida(ws), Path::new("/casos/A.sicro/Exportações/Vídeos/Edições"));
        let id = uuid::Uuid::parse_str("6f1c2a52-0d5e-4c1b-9a43-2f7d0c1e9b10").unwrap();
        assert_eq!(
            dir_edicao_cache(Path::new("/c/app"), &id),
            Path::new("/c/app/edicao/6f1c2a52-0d5e-4c1b-9a43-2f7d0c1e9b10")
        );
        assert!(PASTAS_DO_CASO.contains(&REL_EDICAO) && PASTAS_DO_CASO.contains(&REL_EDICAO_SAIDA));
        assert!(!fora_do_backup(REL_EDICAO) && !fora_do_backup(REL_EDICAO_SAIDA));
    }

    #[test]
    fn prefixos_antigos() {
        let m = |s| mapear_prefixo_antigo(s);
        assert_eq!(m("croquis/backgrounds/x.png").unwrap(), "Sistema/croquis/backgrounds/x.png");
        assert_eq!(m("croquis/exports/a.png").unwrap(), "Exportações/Croquis/a.png");
        assert_eq!(m("videos/originais/y.mp4").unwrap(), "Mídias/Vídeos/y.mp4");
        assert_eq!(m("videos/montagens/m.json").unwrap(), "Sistema/videos/montagens/m.json");
        assert_eq!(m("videos/montagens/m.mp4").unwrap(), "Exportações/Vídeos/Montagens/m.mp4");
        assert_eq!(m("videos/montagens/m.relatorio.json").unwrap(), "Exportações/Vídeos/Montagens/m.relatorio.json");
        assert_eq!(m("imagens/camadas/c.png").unwrap(), "Sistema/imagens/camadas/c.png");
        assert_eq!(m("sicro.sqlite").unwrap(), "Sistema/sicro.sqlite");
        assert!(m("Sistema/croquis/x").is_none());
        assert!(m("croquisx/y").is_none());
    }
}
