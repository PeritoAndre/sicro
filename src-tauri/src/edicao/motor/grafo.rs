//! Gerador do grafo de filtros (§5.3, regras fixas): rótulos por contador, texto sem BOM e sem comentário. Dono: P0.6a.

use crate::edicao::ffmpeg::args;

/// Grafo em construção: uma cadeia por linha.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Grafo {
    cadeias: Vec<String>,
    contador: usize,
    n_filtros: usize,
}

impl Grafo {
    /// Grafo vazio.
    pub fn novo() -> Self {
        Self::default()
    }

    /// Rótulo novo e único: `<prefixo><contador com 3 dígitos>` (sem colchetes).
    pub fn rotulo(&mut self, prefixo: &str) -> String {
        let r = args::nome(prefixo, self.contador, "");
        self.contador += 1;
        r
    }

    /// Acrescenta `[e1][e2]filtro1,filtro2[s1][s2]`. Rótulos sem colchetes; filtros já escapados.
    pub fn cadeia(&mut self, entradas: &[&str], filtros: &[String], saidas: &[&str]) {
        let mut s = String::new();
        for e in entradas {
            s.push('[');
            s.push_str(e);
            s.push(']');
        }
        s.push_str(&filtros.join(","));
        for o in saidas {
            s.push('[');
            s.push_str(o);
            s.push(']');
        }
        self.n_filtros += filtros.len();
        self.cadeias.push(s);
    }

    /// Quantos filtros o grafo tem.
    pub fn filtros(&self) -> usize {
        self.n_filtros
    }

    /// Texto final do grafo (cadeias separadas por `;` e quebra de linha).
    pub fn texto(&self) -> String {
        self.cadeias.join(";\n")
    }
}

/// `nome=k1=v1:k2=v2`, com os valores escapados; `rotulo` Some vira `nome@rotulo=…`.
pub fn filtro(nome: &str, rotulo: Option<&str>, opcoes: &[(&str, String)]) -> String {
    let mut s = String::from(nome);
    if let Some(r) = rotulo {
        s.push('@');
        s.push_str(r);
    }
    for (i, (k, v)) in opcoes.iter().enumerate() {
        s.push(if i == 0 { '=' } else { ':' });
        s.push_str(k);
        s.push('=');
        s.push_str(&args::escapar_filtro(v));
    }
    s
}

/// `scale` sempre com `+accurate_rnd+bitexact`: largura e altura em px, `flags` base ("bicubic", "bilinear", "neighbor", "area").
pub fn scale(w: u32, h: u32, flags: &str) -> String {
    format!("scale={w}:{h}:flags={flags}+accurate_rnd+bitexact")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn rotulos_cadeias_e_contagem() {
        let mut g = Grafo::novo();
        assert_eq!(g.rotulo("c"), "c000");
        assert_eq!(g.rotulo("m"), "m001");
        g.cadeia(&["0:v"], &["trim=end_frame=3".into(), "format=yuv420p".into()], &["a"]);
        g.cadeia(&["a", "1:v"], &["overlay=x=0:y=0".into()], &["v"]);
        assert_eq!(g.filtros(), 3);
        let t = g.texto();
        assert_eq!(t, "[0:v]trim=end_frame=3,format=yuv420p[a];\n[a][1:v]overlay=x=0:y=0[v]");
        assert!(!t.starts_with('\u{feff}') && !t.lines().any(|l| l.trim_start().starts_with('#')));
    }

    #[test]
    fn filtro_escapa_o_valor_e_scale_leva_as_flags_exatas() {
        assert_eq!(filtro("fps", Some("c003"), &[("fps", "30000/1001".into()), ("start_time", "0".into())]), "fps@c003=fps=30000/1001:start_time=0");
        assert_eq!(filtro("setpts", None, &[("expr", "if(lt(N,2),0,5)".into())]), "setpts=expr=if(lt(N\\,2)\\,0\\,5)");
        assert_eq!(filtro("hflip", None, &[]), "hflip");
        assert_eq!(scale(640, 360, "bicubic"), "scale=640:360:flags=bicubic+accurate_rnd+bitexact");
    }
}
