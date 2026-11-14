//! Argumentos e escapes (§5.3, regras fixas do gerador): grafo sempre em arquivo, só nomes simples. Dono: P0.4.

use std::path::Path;

use super::sonda::{Capacidades, GrafoArquivo};
use crate::error::{Result, SicroError};

// CONTRATO: acréscimo. Limite da linha de comando do Windows em unidades UTF-16, com o NUL do fim;
// `exec::rodar` recusa (`linha_grande`) em qualquer sistema, para o Linux acusar o que quebraria no Windows.
pub const LIMITE_LINHA: usize = 32_767;

/// Argumentos globais de toda execução de render: `-hide_banner -nostdin -y -nostats -loglevel verbose -copyts`.
/// `verbose` porque no nível padrão o `fps` não imprime `frames in` (T1).
pub fn globais() -> Vec<String> {
    ["-hide_banner", "-nostdin", "-y", "-nostats", "-loglevel", "verbose", "-copyts"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Argumentos de reprodutibilidade da saída (§5.7): `-fflags +bitexact … -map_metadata -1 -map_chapters -1`.
pub fn bitexact() -> Vec<String> {
    [
        "-fflags", "+bitexact", "-flags:v", "+bitexact", "-flags:a", "+bitexact", "-map_metadata", "-1", "-map_chapters", "-1",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Par de argumentos que entrega o grafo em arquivo, conforme a sonda: `-/filter_complex <nome>` ou `-filter_complex_script <nome>`.
pub fn grafo_em_arquivo(cap: &Capacidades, nome: &str) -> Vec<String> {
    let opcao = match cap.grafo_em_arquivo {
        GrafoArquivo::BarraFilterComplex => "-/filter_complex",
        GrafoArquivo::FilterComplexScript => "-filter_complex_script",
    };
    vec![opcao.to_string(), nome.to_string()]
}

/// Grava um arquivo de texto auxiliar (grafo, `.ffconcat`, legenda) em UTF-8, sem BOM, na pasta de trabalho.
/// `nome` tem de ser simples (sem pasta). Grafo (`.txt`) com linha de comentário é recusado: o ffmpeg lê `#` como filtro (T2).
pub fn gravar_texto(pasta: &Path, nome: &str, conteudo: &str) -> Result<()> {
    if !nome_simples(nome) {
        return Err(SicroError::Validation(format!("nome_invalido: {nome}")));
    }
    let conteudo = conteudo.strip_prefix('\u{feff}').unwrap_or(conteudo);
    if nome.ends_with(".txt") && conteudo.lines().any(|l| l.trim_start().starts_with('#')) {
        return Err(SicroError::Validation(format!("grafo_com_comentario: {nome}")));
    }
    std::fs::create_dir_all(pasta)?;
    std::fs::write(pasta.join(nome), conteudo.as_bytes())?;
    Ok(())
}

/// Valor de opção escapado para dentro de um grafo de filtros: escape do valor (`\ ' :` e espaço) e depois
/// do grafo (`\ ' [ ] , ;` e espaço), as duas camadas do `av_get_token`. Serve a caminho com unidade do Windows.
pub fn escapar_filtro(valor: &str) -> String {
    let nivel1 = escapar_com(valor, &['\\', '\'', ':']);
    escapar_com(&nivel1, &['\\', '\'', '[', ']', ',', ';'])
}

fn escapar_com(s: &str, especiais: &[char]) -> String {
    let mut o = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        // espaço escapado não é aparado nas pontas
        if especiais.contains(&c) || c.is_whitespace() {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

/// Caminho escapado para ir entre as aspas de uma linha `file '…'` de `.ffconcat` (exige `-safe 0`).
/// Dentro das aspas tudo é literal, inclusive `\` e `:`; o apóstrofo fecha, entra escapado e reabre.
pub fn escapar_concat(caminho: &str) -> String {
    caminho.replace('\'', "'\\''")
}

/// Nome simples válido para rótulo e arquivo auxiliar (`c003v`, `b003.txt`): prefixo + contador com 3 dígitos + sufixo.
pub fn nome(prefixo: &str, n: usize, sufixo: &str) -> String {
    debug_assert!(prefixo.chars().chain(sufixo.chars()).all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.'));
    format!("{prefixo}{n:03}{sufixo}")
}

/// Linha de comando inteira em unidades UTF-16, como a do `CreateProcessW` montada pelo Rust no Windows
/// (programa sempre entre aspas; argumento com espaço, tabulação ou vazio entre aspas; `\` antes de `"` dobrada).
/// Sem o NUL do fim: cabe quando `tamanho_da_linha < LIMITE_LINHA`.
pub fn tamanho_da_linha(programa: &Path, args: &[String]) -> usize {
    let mut n = tamanho_arg(&programa.to_string_lossy(), true);
    for a in args {
        n += 1 + tamanho_arg(a, false);
    }
    n
}

fn tamanho_arg(arg: &str, sempre_aspas: bool) -> usize {
    let aspas = sempre_aspas || arg.is_empty() || arg.contains([' ', '\t']);
    let mut n = 0;
    let mut barras = 0usize;
    for c in arg.encode_utf16() {
        if c == u16::from(b'\\') {
            barras += 1;
        } else {
            if c == u16::from(b'"') {
                n += barras + 1;
            }
            barras = 0;
        }
        n += 1;
    }
    if aspas {
        n += barras + 2;
    }
    n
}

fn nome_simples(nome: &str) -> bool {
    !nome.is_empty()
        && nome != "."
        && nome != ".."
        // `-x.txt` seria lido pelo ffmpeg como opção
        && !nome.starts_with('-')
        && nome.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::path::PathBuf;

    /// Desfaz uma camada como o `av_get_token` (sem aspas): `\x` vira `x`, pontas aparadas se não escapadas.
    fn desfazer(s: &str) -> String {
        let mut o = String::new();
        let mut esc = false;
        for c in s.chars() {
            if esc {
                o.push(c);
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else {
                o.push(c);
            }
        }
        o
    }

    #[test]
    fn escapar_filtro_ida_e_volta() {
        for v in [
            "simples.png",
            "C:\\Users\\Perito\\Vídeos\\caso 1\\x.mp4",
            "d'água, [a];b:c",
            " ponta ",
            "barra\\no fim\\",
        ] {
            let e = escapar_filtro(v);
            assert_eq!(desfazer(&desfazer(&e)), v, "{e}");
            // nada especial do grafo sobra sem barra antes
            let mut ant = ' ';
            let mut esc = false;
            for c in e.chars() {
                if !esc && matches!(c, '[' | ']' | ',' | ';' | '\'') {
                    panic!("{c} sem escape em {e} (antes: {ant})");
                }
                esc = !esc && c == '\\';
                ant = c;
            }
        }
        assert_eq!(escapar_filtro("C:\\a"), "C\\\\:\\\\\\\\a");
    }

    #[test]
    fn escapar_concat_apostrofo() {
        assert_eq!(escapar_concat("/a/d'água/x.png"), "/a/d'\\''água/x.png");
        assert_eq!(escapar_concat("C:\\caso 1\\x.png"), "C:\\caso 1\\x.png");
    }

    #[test]
    fn nome_com_contador() {
        assert_eq!(nome("c", 3, "v"), "c003v");
        assert_eq!(nome("b", 3, ".txt"), "b003.txt");
        assert_eq!(nome("c", 1234, "_anot.ffconcat"), "c1234_anot.ffconcat");
    }

    #[test]
    fn tamanho_da_linha_segue_o_windows() {
        let p = Path::new("ffmpeg");
        // "ffmpeg" -i a  →  8 + 1 + 2 + 1 + 1
        assert_eq!(tamanho_da_linha(p, &["-i".into(), "a".into()]), 13);
        // vazio vira ""
        assert_eq!(tamanho_da_linha(p, &["".into()]), 8 + 1 + 2);
        // espaço: aspas; barra antes de aspa interna dobra; barras no fim de argumento com aspas dobram
        assert_eq!(tamanho_arg("a b", false), 5);
        assert_eq!(tamanho_arg("a\\\"b", false), 6); // a\\\"b
        assert_eq!(tamanho_arg("c d\\", false), 7); // "c d\\"
        assert_eq!(tamanho_arg("água", false), 4);
        assert_eq!(tamanho_arg("𝄞", false), 2); // UTF-16
    }

    #[test]
    fn gravar_texto_sem_bom_e_sem_comentario() {
        let d = tempfile::tempdir().unwrap();
        gravar_texto(d.path(), "g.txt", "\u{feff}[0:v]null[v]").unwrap();
        assert_eq!(std::fs::read(d.path().join("g.txt")).unwrap(), b"[0:v]null[v]");
        assert!(gravar_texto(d.path(), "g.txt", "# comentário\n[0:v]null[v]").is_err());
        assert!(gravar_texto(d.path(), "../g.txt", "x").is_err());
        assert!(gravar_texto(d.path(), "a/g.txt", "x").is_err());
        assert!(gravar_texto(d.path(), "-y.txt", "x").is_err());
        assert!(gravar_texto(d.path(), "a\\g.txt", "x").is_err());
        gravar_texto(d.path(), "l.srt", "1\n00:00:00,000 --> 00:00:01,000\n#1 na fila\n").unwrap();
    }

    use crate::edicao::ffmpeg::exec::{self, Comando};
    use crate::edicao::testes::{fontes, medir};
    use std::sync::atomic::AtomicBool;

    fn comando(cap: &Capacidades, pasta: &Path, args: Vec<String>) -> Comando {
        Comando {
            nome: "args".into(),
            programa: cap.binario.clone(),
            args,
            pasta: pasta.to_path_buf(),
            ambiente: vec![],
            sem_ambiente: vec![],
            stderr: pasta.join("args.stderr"),
        }
    }

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    /// 1 500 `setpts` com expressão longa (150 KB): vai por arquivo, roda e devolve os mesmos quadros; na linha, é recusado.
    #[test]
    fn grafo_de_150_kb_vai_por_arquivo() {
        let Some(cap) = fontes::cap() else { return };
        let fonte = fontes::gerar(fontes::Fonte::Mp4).unwrap();
        let p = fontes::pasta_temporaria("grafo-150k");
        let termo = format!("setpts='PTS+0*({})'", (1..=30).map(|i| format!("lt(N,{i})")).collect::<Vec<_>>().join("+"));
        let mut g = String::from("[0:v]");
        while g.len() < 150 * 1024 {
            g.push_str(&termo);
            g.push(',');
        }
        g.push_str("null[v]");
        gravar_texto(&p, "g.txt", &g).unwrap();
        assert!(std::fs::metadata(p.join("g.txt")).unwrap().len() >= 150 * 1024);
        let mut a = v(&["-hide_banner", "-nostdin", "-i", &fonte.to_string_lossy()]);
        a.extend(grafo_em_arquivo(&cap, "g.txt"));
        a.extend(v(&["-map", "[v]", "-frames:v", "60", "-f", "framehash", "-hash", "sha256", "-"]));
        let c = comando(&cap, &p, a);
        let (t, out) = exec::rodar_com_saida(&c, &AtomicBool::new(false)).unwrap();
        assert_eq!(t.codigo, Some(0), "{:?}", exec::ultimas_linhas(&c.stderr, 5));
        let h = medir::coluna_de_hash(&String::from_utf8_lossy(&out));
        assert_eq!(h, medir::framehash(&cap, &fonte).unwrap()[..60].to_vec(), "{}", cap.versao);

        let mut a = v(&["-hide_banner", "-nostdin", "-i", &fonte.to_string_lossy(), "-filter_complex", &g]);
        a.extend(v(&["-map", "[v]", "-f", "null", "-"]));
        let r = exec::rodar(&comando(&cap, &p, a), &AtomicBool::new(false));
        assert!(matches!(r, Err(crate::error::SicroError::Validation(m)) if m.starts_with("linha_grande")));
    }

    fn png(caminho: &Path, rgb: [u8; 3]) {
        image::RgbImage::from_pixel(16, 16, image::Rgb(rgb)).save_with_format(caminho, image::ImageFormat::Png).unwrap();
    }

    fn cores(cap: &Capacidades, pasta: &Path, mut a: Vec<String>) -> Vec<[u8; 3]> {
        a.extend(v(&["-f", "rawvideo", "-pix_fmt", "rgb24", "-"]));
        let c = comando(cap, pasta, a);
        let (t, out) = exec::rodar_com_saida(&c, &AtomicBool::new(false)).unwrap();
        assert_eq!(t.codigo, Some(0), "{:?} ({})", exec::ultimas_linhas(&c.stderr, 5), cap.versao);
        out.as_chunks::<{ 16 * 16 * 3 }>().0.iter().map(|q| [q[0], q[1], q[2]]).collect()
    }

    /// Espaço, acento, apóstrofo, vírgula, ponto e vírgula e colchete no `.ffconcat` e no grafo, por caminho absoluto
    /// (no Windows, com a unidade `C:` e `\`) e relativo. No Linux, também um nome com `:` e `\` (a forma de `C:\…`).
    #[test]
    fn caminho_com_espaco_acento_e_apostrofo() {
        let Some(cap) = fontes::cap() else { return };
        let p = fontes::pasta_temporaria("caminhos");
        let pasta = p.join("pasta d'água ç");
        std::fs::create_dir_all(&pasta).unwrap();
        let mut arquivos = vec![("quadro 1 'á'.png", [255, 0, 0]), ("quadro 2 ç;[b],x.png", [0, 255, 0])];
        if cfg!(unix) {
            arquivos.push(("C:\\caso 1\\é.png", [0, 0, 255]));
        }
        for (n, c) in &arquivos {
            png(&pasta.join(n), *c);
        }
        let mut lista = String::from("ffconcat version 1.0\n");
        for (n, _) in &arquivos {
            lista.push_str(&format!("file '{}'\n", escapar_concat(&pasta.join(n).to_string_lossy())));
        }
        gravar_texto(&p, "lista.ffconcat", &lista).unwrap();
        let lidas = cores(&cap, &p, v(&["-hide_banner", "-nostdin", "-f", "concat", "-safe", "0", "-i", "lista.ffconcat"]));
        assert_eq!(lidas, arquivos.iter().map(|(_, c)| *c).collect::<Vec<_>>());

        for (n, c) in &arquivos {
            // relativo com `./`: no Linux, `C:` no começo seria lido como protocolo
            for caminho in [pasta.join(n), PathBuf::from(".").join(n)] {
                let g = format!("movie=filename={},format=rgb24[v]", escapar_filtro(&caminho.to_string_lossy()));
                gravar_texto(&pasta, "g.txt", &g).unwrap();
                let mut a = v(&["-hide_banner", "-nostdin"]);
                a.extend(grafo_em_arquivo(&cap, "g.txt"));
                a.extend(v(&["-map", "[v]", "-frames:v", "1"]));
                // relativo: `current_dir` é a pasta das imagens
                assert_eq!(cores(&cap, &pasta, a), vec![*c], "{g}");
            }
        }
    }
}
