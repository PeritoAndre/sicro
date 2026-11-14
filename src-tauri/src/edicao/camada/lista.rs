//! Listas `.ffconcat` das camadas (§5.5): UMA entrada por slot (ou por quadro lido), nunca `duration`. Dono: P0.5.
//! Sem `duration` a lista não carrega tempo: quem lê impõe (`settb=<den>/<num>,setpts=N` por slot, árvore sobre
//! `N` por quadro lido). Com `duration`, a 29,97 a camada saía do slot a cada 6 quadros (A17).

use crate::edicao::ffmpeg::args;

/// Texto do `.ffconcat`: `arquivos[i]` = nome simples do PNG do quadro i (iguais repetem o nome).
pub fn ffconcat(arquivos: &[String]) -> String {
    let mut s = String::with_capacity(22 + arquivos.len() * 20);
    s.push_str("ffconcat version 1.0\n");
    for a in arquivos {
        s.push_str("file '");
        s.push_str(&args::escapar_concat(a));
        s.push_str("'\n");
    }
    s
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn uma_entrada_por_quadro_sem_duration() {
        let l = ffconcat(&["000000.png".into(), "000000.png".into(), "000002.png".into()]);
        assert_eq!(l, "ffconcat version 1.0\nfile '000000.png'\nfile '000000.png'\nfile '000002.png'\n");
        assert!(!l.contains("duration"));
        assert_eq!(ffconcat(&["a'b.png".into()]), "ffconcat version 1.0\nfile 'a'\\''b.png'\n");
    }
}
