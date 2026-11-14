//! Expressões do ffmpeg (D9, §5.11): árvores balanceadas, nunca `if` aninhado em fila. Dono: P0.6a.
//! O texto sai cru (com vírgulas): quem põe no grafo passa por `grafo::filtro`, que escapa.

use crate::edicao::modelo::Racional;

/// Tolerância padrão de `tabela`, em px.
pub const TOLERANCIA_PX: f64 = 0.25;

/// Árvore balanceada sobre `t` que devolve `valores_por_slot[q]` (px) no slot q: junta slots seguidos que caem
/// numa reta (tolerância `TOLERANCIA_PX`), limiares em (q − 0,5)/fps.
pub fn tabela(valores_por_slot: &[f64], fps: Racional) -> String {
    tabela_em(valores_por_slot, fps, TOLERANCIA_PX, "t")
}

/// Um trecho reto: slots [de, ate), valor `v0` no slot `de` e inclinação por slot.
struct Reta {
    de: usize,
    ate: usize,
    v0: f64,
    k: f64,
}

/// Retas que passam a até `tol` de todo ponto (método do cone: uma passada, cada reta presa ao primeiro ponto dela).
fn retas(v: &[f64], tol: f64) -> Vec<Reta> {
    let mut r = Vec::new();
    let mut i = 0;
    while i < v.len() {
        let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
        let mut j = i + 1;
        while j < v.len() {
            let d = (j - i) as f64;
            let (a, b) = ((v[j] - tol - v[i]) / d, (v[j] + tol - v[i]) / d);
            if a.max(lo) > b.min(hi) {
                break;
            }
            (lo, hi) = (a.max(lo), b.min(hi));
            j += 1;
        }
        let k = if j - i < 2 { 0.0 } else { (lo + hi) / 2.0 };
        r.push(Reta { de: i, ate: j, v0: v[i], k });
        i = j;
    }
    r
}

fn num(v: f64) -> String {
    // −0 e inteiros sem ponto; o resto na menor forma que volta igual
    let v = v + 0.0;
    if v < 0.0 {
        format!("({v})")
    } else {
        format!("{v}")
    }
}

fn arvore_retas(r: &[Reta], fps: Racional, var: &str) -> String {
    if r.len() == 1 {
        let x = &r[0];
        if x.k == 0.0 {
            return num(x.v0);
        }
        // inclinação por segundo; o tempo do slot `de` é de·den/num
        let t0 = x.de as f64 * fps.den as f64 / fps.num as f64;
        let k = x.k * fps.num as f64 / fps.den as f64;
        return format!("{}+({var}-{})*{}", num(x.v0), num(t0), num(k));
    }
    let m = r.len() / 2;
    let limiar = (r[m].de as f64 - 0.5) * fps.den as f64 / fps.num as f64;
    format!("if(lt({var},{}),{},{})", num(limiar), arvore_retas(&r[..m], fps, var), arvore_retas(&r[m..], fps, var))
}

/// Como `tabela`, com tolerância e variável escolhidas (`variavel` = "t", ou "(on-1)/<FPS>" no `perspective`).
pub fn tabela_em(valores_por_slot: &[f64], fps: Racional, tolerancia: f64, variavel: &str) -> String {
    if valores_por_slot.is_empty() {
        return "0".into();
    }
    let tol = if tolerancia.is_finite() && tolerancia > 0.0 { tolerancia } else { 0.0 };
    arvore_retas(&retas(valores_por_slot, tol), fps, variavel)
}

/// Slots por bloco de `tabela_alinhada`: nenhuma reta atravessa um múltiplo dele.
pub(crate) const BLOCO_DA_TABELA: usize = 16;

/// Como `tabela_em`, para uma JANELA de um clipe: `valores[i]` é o do slot absoluto `primeiro + i` e a janela
/// começa no slot `primeiro + antes` (a variável vale 0 nele). As retas são cortadas nos múltiplos de
/// `BLOCO_DA_TABELA` do slot absoluto, e por isso são as mesmas qualquer que seja a janela: o quadro final e a
/// exportação avaliam a mesma conta. `primeiro` tem de ser múltiplo do bloco e `valores` tem de ir até o fim de
/// um bloco (ou do clipe): a reta depende de todos os pontos dela.
pub(crate) fn tabela_alinhada(valores: &[f64], primeiro: usize, antes: usize, fps: Racional, tolerancia: f64, variavel: &str) -> String {
    if valores.len() <= antes {
        return "0".into();
    }
    let tol = if tolerancia.is_finite() && tolerancia > 0.0 { tolerancia } else { 0.0 };
    let mut r: Vec<Reta> = Vec::new();
    let mut i = 0;
    while i < valores.len() {
        // até o próximo múltiplo do bloco, em slots absolutos
        let fim = (((primeiro + i) / BLOCO_DA_TABELA + 1) * BLOCO_DA_TABELA - primeiro).min(valores.len());
        for x in retas(&valores[i..fim], tol) {
            r.push(Reta { de: x.de + i, ate: x.ate + i, v0: x.v0, k: x.k });
        }
        i = fim;
    }
    // só as retas que tocam a janela, com o tempo contado do começo dela
    let r: Vec<RetaNoTempo> = r
        .into_iter()
        .filter(|x| x.ate > antes)
        .map(|x| RetaNoTempo { t0: (x.de as f64 - antes as f64) * fps.den as f64 / fps.num as f64, v0: x.v0, k: x.k * fps.num as f64 / fps.den as f64 })
        .collect();
    arvore_no_tempo(&r, fps, variavel)
}

/// Reta com o tempo (s) do primeiro slot dela (negativo quando começa antes da janela) e a inclinação por segundo.
struct RetaNoTempo {
    t0: f64,
    v0: f64,
    k: f64,
}

fn arvore_no_tempo(r: &[RetaNoTempo], fps: Racional, var: &str) -> String {
    if r.len() == 1 {
        let x = &r[0];
        return if x.k == 0.0 { num(x.v0) } else { format!("{}+({var}-{})*{}", num(x.v0), num(x.t0), num(x.k)) };
    }
    let m = r.len() / 2;
    let limiar = r[m].t0 - 0.5 * fps.den as f64 / fps.num as f64;
    format!("if(lt({var},{}),{},{})", num(limiar), arvore_no_tempo(&r[..m], fps, var), arvore_no_tempo(&r[m..], fps, var))
}

/// Corrida de pts de passo constante: quadros [de, ate).
struct Corrida {
    de: usize,
    ate: usize,
    p0: i64,
    passo: i64,
}

fn arvore_corridas(c: &[Corrida]) -> String {
    if c.len() == 1 {
        let x = &c[0];
        let p0 = if x.p0 < 0 { format!("({})", x.p0) } else { x.p0.to_string() };
        if x.ate - x.de == 1 || x.passo == 0 {
            return p0;
        }
        let n = if x.de == 0 { "N".to_string() } else { format!("(N-{})", x.de) };
        return format!("{p0}+{n}*{}", x.passo);
    }
    let m = c.len() / 2;
    format!("if(lt(N,{}),{},{})", c[m].de, arvore_corridas(&c[..m]), arvore_corridas(&c[m..]))
}

/// Árvore balanceada sobre `N` que devolve `pts[N]` (inteiros, na base do elo): impõe o pts da camada por quadro lido.
/// Quadros seguidos de passo igual viram uma folha `p0+(N-n0)*passo` (só inteiros: exato em `double`).
pub fn arvore_pts(pts: &[i64]) -> String {
    if pts.is_empty() {
        return "0".into();
    }
    let mut c: Vec<Corrida> = Vec::new();
    let mut i = 0;
    while i < pts.len() {
        let mut j = i + 1;
        let passo = if j < pts.len() { pts[j].wrapping_sub(pts[i]) } else { 0 };
        while j < pts.len() && pts[j].wrapping_sub(pts[j - 1]) == passo {
            j += 1;
        }
        // uma corrida de 2 quadros não poupa nada e rouba o passo da seguinte
        if j - i == 2 && j < pts.len() {
            j = i + 1;
        }
        c.push(Corrida { de: i, ate: j, p0: pts[i], passo });
        i = j;
    }
    arvore_corridas(&c)
}

/// `setpts` do mapa de quadros (§3.1): `ceil(((PTS-<p0>)*<kn>-<e>)/<kd>)*<m>`.
pub fn setpts_mapa(p0: i64, kn: i64, kd: i64, e: i64, m: i64) -> String {
    let menos = if p0 < 0 { format!("+{}", p0.unsigned_abs()) } else { format!("-{p0}") };
    format!("ceil(((PTS{menos})*{kn}-{e})/{kd})*{m}")
}

#[cfg(test)]
mod testes {
    use super::*;

    const F30: Racional = Racional { num: 30, den: 1 };

    /// Avaliador mínimo das expressões daqui: `if(lt(x,a),b,c)`, + − × ÷, parênteses, números e uma variável.
    pub(crate) fn avaliar(expr: &str, var: &str, valor: f64) -> f64 {
        struct P<'a> {
            s: &'a [u8],
            i: usize,
            var: &'a str,
            valor: f64,
        }
        impl P<'_> {
            fn soma(&mut self) -> f64 {
                let mut v = self.produto();
                while self.i < self.s.len() && (self.s[self.i] == b'+' || self.s[self.i] == b'-') {
                    let op = self.s[self.i];
                    self.i += 1;
                    let d = self.produto();
                    v = if op == b'+' { v + d } else { v - d };
                }
                v
            }
            fn produto(&mut self) -> f64 {
                let mut v = self.atomo();
                while self.i < self.s.len() && (self.s[self.i] == b'*' || self.s[self.i] == b'/') {
                    let op = self.s[self.i];
                    self.i += 1;
                    let d = self.atomo();
                    v = if op == b'*' { v * d } else { v / d };
                }
                v
            }
            fn come(&mut self, t: &str) -> bool {
                if self.s[self.i..].starts_with(t.as_bytes()) {
                    self.i += t.len();
                    true
                } else {
                    false
                }
            }
            fn atomo(&mut self) -> f64 {
                if self.come("if(lt(") {
                    let x = self.soma();
                    assert!(self.come(","));
                    let a = self.soma();
                    assert!(self.come("),"));
                    let b = self.soma();
                    assert!(self.come(","));
                    let c = self.soma();
                    assert!(self.come(")"));
                    return if x < a { b } else { c };
                }
                if self.come("ceil(") {
                    let x = self.soma();
                    assert!(self.come(")"));
                    return x.ceil();
                }
                let var = self.var.to_string();
                if self.come(&var) {
                    return self.valor;
                }
                if self.come("(") {
                    let x = self.soma();
                    assert!(self.come(")"), "falta ) em {}", self.i);
                    return x;
                }
                if self.come("-") {
                    return -self.atomo();
                }
                let a = self.i;
                while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') {
                    self.i += 1;
                }
                std::str::from_utf8(&self.s[a..self.i]).unwrap().parse().unwrap_or_else(|_| panic!("número em {a}"))
            }
        }
        let mut p = P { s: expr.as_bytes(), i: 0, var, valor };
        let v = p.soma();
        assert_eq!(p.i, expr.len(), "sobra na expressão");
        v
    }

    fn profundidade(e: &str) -> usize {
        let (mut d, mut max) = (0usize, 0usize);
        for c in e.chars() {
            match c {
                '(' => {
                    d += 1;
                    max = max.max(d);
                }
                ')' => d -= 1,
                _ => {}
            }
        }
        max
    }

    #[test]
    fn tabela_devolve_o_valor_de_cada_slot_dentro_da_tolerancia() {
        // rampa, patamar, salto e curva
        let mut v: Vec<f64> = (0..40).map(|q| 100.0 + 3.5 * q as f64).collect();
        v.extend(std::iter::repeat_n(240.0, 25));
        v.extend((0..60).map(|q| 500.0 - 0.02 * (q * q) as f64));
        for fps in [F30, Racional { num: 30000, den: 1001 }, Racional { num: 25, den: 2 }] {
            let e = tabela(&v, fps);
            for (q, &esperado) in v.iter().enumerate() {
                let t = q as f64 * fps.den as f64 / fps.num as f64;
                let obtido = avaliar(&e, "t", t);
                assert!((obtido - esperado).abs() <= TOLERANCIA_PX + 1e-9, "slot {q}: {obtido} × {esperado}");
            }
            // rampa e patamar viram uma folha cada
            assert!(e.matches("if(").count() < 20, "{}", e.len());
        }
        assert_eq!(tabela(&[7.0; 300], F30), "7");
        assert_eq!(tabela(&[], F30), "0");
        assert_eq!(tabela(&[-2.5], F30), "(-2.5)");
    }

    #[test]
    fn tabela_em_troca_a_variavel_e_a_tolerancia() {
        let v: Vec<f64> = (0..90).map(|q| (q as f64 * 0.37).sin() * 50.0).collect();
        let e = tabela_em(&v, F30, 0.01, "(on-1)/30");
        assert!(e.contains("(on-1)/30") && !e.contains("lt(t,"));
        for (q, &esperado) in v.iter().enumerate() {
            // o avaliador troca a variável inteira por um valor
            let obtido = avaliar(&e, "(on-1)/30", q as f64 / 30.0);
            assert!((obtido - esperado).abs() <= 0.01 + 1e-9, "slot {q}");
        }
    }

    #[test]
    fn tabela_alinhada_da_a_mesma_conta_em_qualquer_janela() {
        let fps = Racional { num: 30000, den: 1001 };
        let v: Vec<f64> = (0..200).map(|q| 40.0 + 0.9 * q as f64 + 3.0 * ((q as f64) * 0.05).sin()).collect();
        let var = "(on-1)*1001/30000";
        let inteira = tabela_alinhada(&v, 0, 0, fps, 0.05, var);
        for q in 0..200 {
            let t = q as f64 * 1001.0 / 30000.0;
            assert!((avaliar(&inteira, var, t) - v[q]).abs() <= 0.05 + 1e-9, "slot {q}");
        }
        // janelas no meio: o valor de cada slot é bit a bit o da tabela inteira
        for (a, b) in [(37usize, 38usize), (16, 48), (5, 199), (190, 200), (63, 65)] {
            let primeiro = a / BLOCO_DA_TABELA * BLOCO_DA_TABELA;
            let fim = b.div_ceil(BLOCO_DA_TABELA).saturating_mul(BLOCO_DA_TABELA).min(v.len());
            let e = tabela_alinhada(&v[primeiro..fim], primeiro, a - primeiro, fps, 0.05, var);
            for q in a..b {
                let (na_janela, na_inteira) = (avaliar(&e, var, (q - a) as f64 * 1001.0 / 30000.0), avaliar(&inteira, var, q as f64 * 1001.0 / 30000.0));
                assert!((na_janela - na_inteira).abs() < 1e-9, "janela [{a}, {b}) slot {q}: {na_janela} × {na_inteira}");
            }
        }
        let parada = tabela_alinhada(&[5.0; 40], 32, 3, fps, 0.05, var);
        assert!((0..37).all(|q| avaliar(&parada, var, q as f64 * 1001.0 / 30000.0) == 5.0));
    }

    #[test]
    fn oito_mil_pontos_ficam_em_arvore_rasa() {
        let v: Vec<f64> = (0..8000).map(|q| ((q * 7919) % 1013) as f64).collect();
        let e = tabela(&v, F30);
        // sem reta nenhuma: 8000 folhas, profundidade de log2
        assert!(profundidade(&e) <= 16, "profundidade {}", profundidade(&e));
        assert!(e.len() < 800 * 1024, "{} bytes", e.len());
        for q in [0usize, 1, 4095, 4096, 7999] {
            assert!((avaliar(&e, "t", q as f64 / 30.0) - v[q]).abs() < 1e-6);
        }
    }

    #[test]
    fn arvore_pts_devolve_cada_pts_e_junta_passo_constante() {
        let regulares: Vec<i64> = (0..900).map(|n| 5000 + n * 512).collect();
        let e = arvore_pts(&regulares);
        assert_eq!(e, "5000+N*512");
        // taxa variável, pts negativo, repetido e salto
        let pts = vec![-66, -33, 0, 33, 67, 100, 100, 133, 167, 900, 933, 966, 1000, 1033, 5000];
        let e = arvore_pts(&pts);
        for (n, &p) in pts.iter().enumerate() {
            assert_eq!(avaliar(&e, "N", n as f64), p as f64, "N = {n} em {e}");
        }
        assert_eq!(arvore_pts(&[42]), "42");
        assert_eq!(arvore_pts(&[]), "0");
        let ms: Vec<i64> = (0..3000).map(|n| (n * 100 + 1) / 3).collect();
        let e = arvore_pts(&ms);
        assert!(profundidade(&e) <= 16);
        for (n, &p) in ms.iter().enumerate() {
            assert_eq!(avaliar(&e, "N", n as f64), p as f64);
        }
    }

    #[test]
    fn setpts_do_mapa_em_inteiros() {
        assert_eq!(setpts_mapa(40960, 1, 512, 256, 512), "ceil(((PTS-40960)*1-256)/512)*512");
        assert_eq!(setpts_mapa(-90, 3, 2, 1, 5), "ceil(((PTS+90)*3-1)/2)*5");
        // mesma conta do §3.1
        let e = setpts_mapa(1000, 3, 100, 50, 7);
        for pts in [1000i64, 1016, 1017, 1033, 1034, 1100] {
            let slot = (((pts - 1000) * 3 - 50) as f64 / 100.0).ceil() * 7.0;
            assert_eq!(avaliar(&e, "PTS", pts as f64), slot + 0.0);
        }
    }
}
