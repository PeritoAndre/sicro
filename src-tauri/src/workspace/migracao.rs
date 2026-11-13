//! Migração do layout 1 (10 pastas na raiz) para o layout 2
//! (Exportações / Mídias / Sistema). Só renomeia dentro do mesmo disco; em
//! falha para com erro claro e não apaga nada. Pode ser repetida: o que já
//! foi movido some da origem e o resto continua.

use std::fs;
use std::path::Path;

use crate::error::{Result, SicroError};
use crate::filesystem::atomic_write_bytes;
use crate::workspace::manifest::Manifest;
use crate::workspace::paths::{DIR_EXPORTACOES, DIR_MIDIAS, DIR_SISTEMA, LAYOUT_ATUAL, PASTAS_DO_CASO, PREFIXOS_ANTIGOS, REL_MANIFEST, layout_antigo, mapear_prefixo_antigo};

fn falha(acao: &str, p: &Path, e: impl std::fmt::Display) -> SicroError {
    SicroError::Workspace(format!(
        "migração do caso interrompida ({acao} {}): {e}. Nada foi apagado; abra o caso de novo para continuar.",
        p.display()
    ))
}

fn mover(src: &Path, dst: &Path) -> Result<()> {
    if fs::symlink_metadata(dst).is_err() {
        if let Some(pai) = dst.parent() {
            fs::create_dir_all(pai).map_err(|e| falha("criar", pai, e))?;
        }
        return fs::rename(src, dst).map_err(|e| falha("mover", src, e));
    }
    if src.is_dir() && dst.is_dir() {
        let filhos = fs::read_dir(src).map_err(|e| falha("ler", src, e))?;
        for f in filhos {
            let f = f.map_err(|e| falha("ler", src, e))?;
            mover(&f.path(), &dst.join(f.file_name()))?;
        }
        let _ = fs::remove_dir(src);
        return Ok(());
    }
    Err(SicroError::Workspace(format!(
        "migração do caso interrompida: {} já existe e conflita com {}",
        dst.display(),
        src.display()
    )))
}

/// Migra se o caso estiver no layout antigo. `true` = migrou.
pub fn migrar_layout_antigo(ws: &Path) -> Result<bool> {
    if !layout_antigo(ws) {
        return Ok(false);
    }
    fs::create_dir_all(ws.join(DIR_SISTEMA)).map_err(|e| falha("criar", ws, e))?;

    // videos/montagens mistura JSON (Sistema) e saídas (Exportações): arquivo a arquivo.
    let montagens = ws.join("videos").join("montagens");
    if montagens.is_dir() {
        for f in fs::read_dir(&montagens).map_err(|e| falha("ler", &montagens, e))? {
            let f = f.map_err(|e| falha("ler", &montagens, e))?;
            let rel = format!("videos/montagens/{}", f.file_name().to_string_lossy());
            if let Some(novo) = mapear_prefixo_antigo(&rel) {
                mover(&f.path(), &ws.join(novo))?;
            }
        }
        let _ = fs::remove_dir(&montagens);
    }

    for (antigo, novo) in PREFIXOS_ANTIGOS {
        if *antigo == "manifest.json" {
            continue; // por último: é ele que marca "ainda antigo"
        }
        let src = ws.join(antigo);
        if fs::symlink_metadata(&src).is_ok() {
            mover(&src, &ws.join(novo))?;
        }
    }

    // O que sobrou na raiz (pastas desconhecidas) também é "resto" → Sistema.
    for e in fs::read_dir(ws).map_err(|e| falha("ler", ws, e))? {
        let e = e.map_err(|e| falha("ler", ws, e))?;
        let nome = e.file_name().to_string_lossy().into_owned();
        if matches!(nome.as_str(), DIR_EXPORTACOES | DIR_MIDIAS | DIR_SISTEMA | "manifest.json") {
            continue;
        }
        let p = e.path();
        if p.is_dir() && fs::read_dir(&p).map(|mut r| r.next().is_none()).unwrap_or(false) {
            let _ = fs::remove_dir(&p);
            continue;
        }
        mover(&p, &ws.join(DIR_SISTEMA).join(&nome))?;
    }

    for sub in PASTAS_DO_CASO {
        let _ = fs::create_dir_all(ws.join(sub));
    }

    let velho = ws.join("manifest.json");
    let mut m = Manifest::read(ws)?;
    m.layout = Some(LAYOUT_ATUAL);
    m.touch();
    let novo = ws.join(REL_MANIFEST);
    atomic_write_bytes(&novo, &serde_json::to_vec_pretty(&m)?)?;
    fs::remove_file(&velho).map_err(|e| falha("remover", &velho, e))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::paths::{manifest_path, sqlite_path};
    use uuid::Uuid;

    fn caso_antigo(dir: &Path) {
        for sub in [
            "croquis/exports", "croquis/backgrounds", "videos/originais", "videos/montagens",
            "videos/trechos", "videos/frames", "imagens/originais", "imagens/camadas",
            "midias/anexos", "exports/docx", "laudos", "cache", "logs", "dossie", "estranha",
        ] {
            fs::create_dir_all(dir.join(sub)).unwrap();
        }
        let m = Manifest::new(Uuid::new_v4(), Uuid::new_v4());
        fs::write(dir.join("manifest.json"), serde_json::to_vec(&m).unwrap()).unwrap();
        fs::write(dir.join("sicro.sqlite"), b"db").unwrap();
        for f in [
            "croquis/c.sicrocroqui", "croquis/exports/c.png", "croquis/backgrounds/b.png",
            "videos/originais/v.mp4", "videos/montagens/m1.json", "videos/montagens/m1_x.mp4",
            "videos/montagens/m1_x.relatorio.json", "videos/trechos/t.mp4", "videos/frames/f.png",
            "imagens/originais/i.jpg", "imagens/camadas/l.png", "midias/anexos/a.pdf",
            "exports/docx/l.docx", "estranha/z.txt",
        ] {
            fs::write(dir.join(f), f).unwrap();
        }
    }

    #[test]
    fn migra_caso_antigo_completo() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("x.sicro");
        caso_antigo(&ws);
        assert!(migrar_layout_antigo(&ws).unwrap());
        let ok = |r: &str| assert!(ws.join(r).is_file(), "faltou {r}");
        ok("Sistema/manifest.json");
        ok("Sistema/sicro.sqlite");
        ok("Sistema/croquis/c.sicrocroqui");
        ok("Sistema/croquis/backgrounds/b.png");
        ok("Exportações/Croquis/c.png");
        ok("Mídias/Vídeos/v.mp4");
        ok("Sistema/videos/montagens/m1.json");
        ok("Exportações/Vídeos/Montagens/m1_x.mp4");
        ok("Exportações/Vídeos/Montagens/m1_x.relatorio.json");
        ok("Exportações/Vídeos/Trechos/t.mp4");
        ok("Sistema/videos/frames/f.png");
        ok("Mídias/Imagens/i.jpg");
        ok("Sistema/imagens/camadas/l.png");
        ok("Mídias/Anexos/a.pdf");
        ok("Exportações/Laudos/docx/l.docx");
        ok("Sistema/estranha/z.txt");
        let mut raiz: Vec<String> = fs::read_dir(&ws)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        raiz.sort();
        assert_eq!(raiz, ["Exportações", "Mídias", "Sistema"]);
        assert_eq!(Manifest::read(&ws).unwrap().layout, Some(2));
        assert_eq!(manifest_path(&ws), ws.join("Sistema/manifest.json"));
        assert_eq!(sqlite_path(&ws), ws.join("Sistema/sicro.sqlite"));
        assert!(!migrar_layout_antigo(&ws).unwrap());
    }

    #[test]
    fn conflito_para_sem_apagar() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("x.sicro");
        caso_antigo(&ws);
        fs::create_dir_all(ws.join("Mídias/Vídeos")).unwrap();
        fs::write(ws.join("Mídias/Vídeos/v.mp4"), "outro").unwrap();
        assert!(migrar_layout_antigo(&ws).is_err());
        assert!(ws.join("videos/originais/v.mp4").is_file());
        assert!(ws.join("manifest.json").is_file());
    }
}
