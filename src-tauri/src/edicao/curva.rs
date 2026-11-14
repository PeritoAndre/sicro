//! `Animavel`: avaliar, amostrar e simplificar (§5.11). Espelho no front: `modelo/curva.ts`. Dono: P0.3a.
//!
//! Determinístico: só `+ − × ÷` de f64, na ordem escrita (sem FMA, sem seno ou cosseno),
//! para o espelho em JS dar os mesmos bits. As fixtures `curva-*.json` são o contrato.

use super::modelo::{Animavel, Caixa, Chave, Interp, Ponto};

/// Valor que se mistura entre duas chaves.
pub trait Interpolavel: Clone {
    /// Mistura linear: f = 0 dá `a`, f = 1 dá `b`.
    fn misturar(a: &Self, b: &Self, f: f64) -> Self;
}

/// `a + (b − a)·f`, nessa ordem.
fn lerp(a: f64, b: f64, f: f64) -> f64 {
    a + (b - a) * f
}

impl Interpolavel for f64 {
    fn misturar(a: &Self, b: &Self, f: f64) -> Self {
        lerp(*a, *b, f)
    }
}

impl Interpolavel for Ponto {
    fn misturar(a: &Self, b: &Self, f: f64) -> Self {
        Ponto { x: lerp(a.x, b.x, f), y: lerp(a.y, b.y, f) }
    }
}

impl Interpolavel for Caixa {
    fn misturar(a: &Self, b: &Self, f: f64) -> Self {
        Caixa { x: lerp(a.x, b.x, f), y: lerp(a.y, b.y, f), w: lerp(a.w, b.w, f), h: lerp(a.h, b.h, f) }
    }
}

/// Valor em `t` (s, na base do dono da curva). Fora das chaves vale a mais próxima.
/// Lista de chaves vazia é erro de validação; aqui é pânico.
pub fn avaliar<T: Interpolavel>(a: &Animavel<T>, t: f64) -> T {
    match a {
        Animavel::Fixo(v) => v.clone(),
        Animavel::Chaves(ks) => {
            assert!(!ks.is_empty(), "Animavel sem chaves");
            // n = chaves com t ≤ instante; chaves de mesmo t: vale a última
            let n = ks.partition_point(|k| k.t <= t);
            if n == 0 {
                ks[0].v.clone()
            } else if n == ks.len() {
                ks[n - 1].v.clone()
            } else {
                entre(&ks[n - 1], &ks[n], t)
            }
        }
    }
}

/// Valor entre duas chaves seguidas, pela `interp` da primeira; fora de [k0.t, k1.t) segura a ponta.
/// Fração `f = (t − t0) / (t1 − t0)`; "suave" mistura com `suave(f)`.
pub(crate) fn entre<T: Interpolavel>(k0: &Chave<T>, k1: &Chave<T>, t: f64) -> T {
    if t >= k1.t {
        return k1.v.clone();
    }
    if t <= k0.t {
        return k0.v.clone();
    }
    let f = (t - k0.t) / (k1.t - k0.t);
    match k0.interp {
        Interp::Linear => T::misturar(&k0.v, &k1.v, f),
        Interp::Suave => T::misturar(&k0.v, &k1.v, suave(f)),
        Interp::Degrau => k0.v.clone(),
    }
}

/// Um valor por instante de `tempos` (s, crescentes).
pub fn amostrar<T: Interpolavel>(a: &Animavel<T>, tempos: &[f64]) -> Vec<T> {
    tempos.iter().map(|&t| avaliar(a, t)).collect()
}

/// Fração 0..1 da curva "suave" (entrada e saída suaves) para a fração linear `f`.
/// `f·f·(3 − 2·f)`, a cúbica do `receitas-video.md` §3 (a mesma do grafo). Fora de 0..1 segura
/// a ponta (sem isso f = 1,5 daria 0 e f = 2 daria −4).
pub fn suave(f: f64) -> f64 {
    if f <= 0.0 {
        return 0.0;
    }
    if f >= 1.0 {
        return 1.0;
    }
    f * f * (3.0 - 2.0 * f)
}

/// Quanto a caixa `b` se afasta de `a`: o maior deslocamento entre as quatro bordas.
pub(crate) fn desvio(a: &Caixa, b: &Caixa) -> f64 {
    let dx0 = (a.x - b.x).abs();
    let dy0 = (a.y - b.y).abs();
    let dx1 = ((a.x + a.w) - (b.x + b.w)).abs();
    let dy1 = ((a.y + a.h) - (b.y + b.h)).abs();
    // caixa não finita nunca é descartada (`max` engoliria o NaN)
    if dx0.is_nan() || dy0.is_nan() || dx1.is_nan() || dy1.is_nan() {
        return f64::INFINITY;
    }
    dx0.max(dy0).max(dx1).max(dy1)
}

/// Douglas-Peucker sobre caixas: devolve as chaves que ficam. `tolerancia` em 0..1 (mesma unidade das caixas).
/// Erro de uma chave = `desvio` entre ela e o que a curva simplificada dá no `t` dela (mesma `interp`);
/// fica a de maior erro (a primeira, no empate) enquanto passar da tolerância. Idempotente.
pub fn simplificar(chaves: &[Chave<Caixa>], tolerancia: f64) -> Vec<Chave<Caixa>> {
    let n = chaves.len();
    if n <= 2 {
        return chaves.to_vec();
    }
    let tol = tolerancia.max(0.0);
    let mut fica = vec![false; n];
    fica[0] = true;
    fica[n - 1] = true;
    // pilha explícita: um denso de 10 min tem 18 000 chaves
    let mut pilha = vec![(0usize, n - 1)];
    while let Some((i, j)) = pilha.pop() {
        if j <= i + 1 {
            continue;
        }
        let mut pior = 0.0;
        let mut onde = None;
        for k in i + 1..j {
            let d = desvio(&chaves[k].v, &entre(&chaves[i], &chaves[j], chaves[k].t));
            if d > pior {
                pior = d;
                onde = Some(k);
            }
        }
        if let Some(k) = onde {
            if pior > tol {
                fica[k] = true;
                pilha.push((i, k));
                pilha.push((k, j));
            }
        }
    }
    chaves.iter().zip(fica).filter(|(_, f)| *f).map(|(c, _)| c.clone()).collect()
}

/// Fixtures do contrato com o front (`src-tauri/tests/fixtures/edicao/`): `SICRO_GRAVAR_FIXTURES=1`
/// regrava; sem ela, confere (gravada ≠ recalculada = falha). Usada também por `geometria` e `rastreio`.
#[cfg(test)]
pub(crate) mod fixtures {
    use serde_json::Value;
    use std::path::PathBuf;

    pub const VARIAVEL: &str = "SICRO_GRAVAR_FIXTURES";

    pub fn caminho(nome: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edicao").join(nome)
    }

    /// `casos` = lista de `{ nome, entrada, saida }`.
    pub fn conferir(nome: &str, casos: Value) {
        let arq = caminho(nome);
        let casos_lista = casos.as_array().expect("lista de casos");
        for c in casos_lista {
            assert!(c.get("nome").is_some() && c.get("entrada").is_some() && c.get("saida").is_some(), "caso sem nome/entrada/saida");
        }
        if std::env::var(VARIAVEL).map(|v| v == "1").unwrap_or(false) {
            let texto = serde_json::to_string_pretty(&casos).unwrap() + "\n";
            std::fs::write(&arq, texto).expect("gravar fixture");
            return;
        }
        let texto = std::fs::read_to_string(&arq).unwrap_or_else(|_| panic!("{} ausente: rode com {VARIAVEL}=1", arq.display()));
        let gravada: Value = serde_json::from_str(&texto).expect("fixture legível");
        let gravados = gravada.as_array().expect("fixture é lista");
        assert_eq!(gravados.len(), casos_lista.len(), "{nome}: número de casos mudou");
        for (g, c) in gravados.iter().zip(casos_lista) {
            assert_eq!(g, c, "{nome}: caso {} difere do recalculado", c["nome"]);
            // o `==` de número não separa −0 de 0; o texto separa (e o `toEqual` do front também)
            assert_eq!(g.to_string(), c.to_string(), "{nome}: caso {} difere no texto", c["nome"]);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::{json, Value};

    fn ch<T>(t: f64, v: T, interp: Interp) -> Chave<T> {
        Chave { t, pts: None, v, interp }
    }
    fn cx(x: f64, y: f64, w: f64, h: f64) -> Caixa {
        Caixa { x, y, w, h }
    }

    #[test]
    fn fixo_vale_sempre() {
        assert_eq!(avaliar(&Animavel::Fixo(0.25), -5.0), 0.25);
        assert_eq!(avaliar(&Animavel::Fixo(0.25), 1e9), 0.25);
    }

    #[test]
    fn linear_suave_degrau_a_mao() {
        // 0 → 10 em [1, 3]; meio do caminho em t = 2
        for (interp, t, esperado) in [
            (Interp::Linear, 2.0, 5.0),
            (Interp::Linear, 1.5, 2.5),
            (Interp::Linear, 2.5, 7.5),
            // suave: f = 0,25 → 0,0625·2,5 = 0,15625
            (Interp::Suave, 1.5, 1.5625),
            (Interp::Suave, 2.0, 5.0),
            // f = 0,75 → 0,5625·1,5 = 0,84375
            (Interp::Suave, 2.5, 8.4375),
            (Interp::Degrau, 1.0, 0.0),
            (Interp::Degrau, 2.999, 0.0),
            (Interp::Degrau, 3.0, 10.0),
        ] {
            let a = Animavel::Chaves(vec![ch(1.0, 0.0, interp), ch(3.0, 10.0, Interp::Linear)]);
            assert_eq!(avaliar(&a, t), esperado, "{interp:?} em {t}");
        }
    }

    #[test]
    fn antes_da_primeira_e_depois_da_ultima_seguram() {
        let a = Animavel::Chaves(vec![ch(1.0, 4.0, Interp::Linear), ch(2.0, 8.0, Interp::Suave)]);
        assert_eq!(avaliar(&a, 0.0), 4.0);
        assert_eq!(avaliar(&a, -1e30), 4.0);
        assert_eq!(avaliar(&a, 2.0), 8.0);
        assert_eq!(avaliar(&a, 99.0), 8.0);
        let uma = Animavel::Chaves(vec![ch(5.0, 7.0, Interp::Degrau)]);
        assert_eq!(avaliar(&uma, 0.0), 7.0);
        assert_eq!(avaliar(&uma, 9.0), 7.0);
    }

    #[test]
    fn chaves_no_mesmo_instante_vale_a_ultima() {
        let a = Animavel::Chaves(vec![ch(0.0, 0.0, Interp::Linear), ch(1.0, 2.0, Interp::Linear), ch(1.0, 6.0, Interp::Linear), ch(2.0, 8.0, Interp::Linear)]);
        assert_eq!(avaliar(&a, 0.5), 1.0);
        assert_eq!(avaliar(&a, 1.0), 6.0);
        assert_eq!(avaliar(&a, 1.5), 7.0);
    }

    #[test]
    fn caixa_e_ponto_por_componente() {
        let a = Animavel::Chaves(vec![ch(0.0, cx(0.0, 0.5, 0.25, 0.25), Interp::Linear), ch(4.0, cx(0.5, 0.25, 0.5, 0.75), Interp::Linear)]);
        assert_eq!(avaliar(&a, 1.0), cx(0.125, 0.4375, 0.3125, 0.375));
        let p = Animavel::Chaves(vec![ch(0.0, Ponto { x: 1.0, y: 0.0 }, Interp::Linear), ch(2.0, Ponto { x: 0.0, y: 1.0 }, Interp::Linear)]);
        assert_eq!(avaliar(&p, 0.5), Ponto { x: 0.75, y: 0.25 });
    }

    #[test]
    fn suave_nas_pontas_e_simetrica() {
        assert_eq!(suave(0.0), 0.0);
        assert_eq!(suave(1.0), 1.0);
        assert_eq!(suave(0.5), 0.5);
        for i in 1..100 {
            let f = i as f64 / 100.0;
            assert!((suave(f) + suave(1.0 - f) - 1.0).abs() < 1e-15);
            assert!(suave(f) > suave(f - 0.01));
        }
    }

    #[test]
    fn suave_fora_de_0_1_segura_a_ponta() {
        for f in [-1e300, -0.5, -1e-300, -0.0] {
            assert_eq!(suave(f).to_bits(), 0.0f64.to_bits(), "{f}");
        }
        for f in [1.0 + 1e-15, 1.5, 2.0, 1e300] {
            assert_eq!(suave(f), 1.0, "{f}");
        }
        assert!(suave(f64::NAN).is_nan());
    }

    #[test]
    fn amostrar_igual_a_avaliar() {
        let a = Animavel::Chaves(vec![ch(0.0, 0.0, Interp::Suave), ch(1.0, 1.0, Interp::Degrau), ch(2.0, 3.0, Interp::Linear), ch(3.0, -1.0, Interp::Linear)]);
        let tempos: Vec<f64> = (0..120).map(|q| (q as f64 - 10.0) * 1001.0 / 30000.0).collect();
        let v = amostrar(&a, &tempos);
        for (t, x) in tempos.iter().zip(&v) {
            assert_eq!(avaliar(&a, *t).to_bits(), x.to_bits());
        }
    }

    /// Trilha densa sorteada (LCG fixo): caminhada com ruído de até 0,002.
    fn trilha(n: usize) -> Vec<Chave<Caixa>> {
        let mut s: u64 = 0x5eed_1234;
        let mut rnd = || {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((s >> 11) as f64) / ((1u64 << 53) as f64)
        };
        let (mut x, mut y) = (0.2, 0.3);
        (0..n)
            .map(|i| {
                let t = i as f64 / 30.0;
                // segmentos retos com mudança de direção a cada 40 quadros
                let fase = (i / 40) % 4;
                let (vx, vy) = [(0.002, 0.0), (0.0, 0.003), (-0.001, 0.0), (0.0, -0.002)][fase];
                x += vx;
                y += vy;
                let r = 0.002 * (rnd() - 0.5);
                ch(t, cx(x + r, y - r, 0.1 + r, 0.08), Interp::Linear)
            })
            .collect()
    }

    #[test]
    fn simplificar_respeita_a_tolerancia() {
        let ks = trilha(600);
        for tol in [0.0005, 0.002, 0.01, 0.05] {
            let s = simplificar(&ks, tol);
            assert!(s.len() >= 2 && s.len() < ks.len());
            assert_eq!(s.first(), ks.first());
            assert_eq!(s.last(), ks.last());
            let a = Animavel::Chaves(s.clone());
            let pior = ks.iter().map(|k| desvio(&k.v, &avaliar(&a, k.t))).fold(0.0, f64::max);
            assert!(pior <= tol, "tol {tol}: desvio {pior}");
            // toda chave que ficou é uma das originais, na mesma ordem
            let mut it = ks.iter();
            for k in &s {
                assert!(it.any(|o| o == k));
            }
        }
        // tolerância folgada: só as pontas e as quinas
        assert!(simplificar(&ks, 0.01).len() < 40);
    }

    #[test]
    fn simplificar_e_idempotente() {
        let ks = trilha(900);
        for tol in [0.0, 0.0005, 0.003, 0.02] {
            let uma = simplificar(&ks, tol);
            let duas = simplificar(&uma, tol);
            assert_eq!(uma, duas, "tol {tol}");
        }
    }

    #[test]
    fn simplificar_com_suave_e_degrau() {
        // a curva simplificada usa a interp da chave que fica: o erro conta com ela
        let ks = vec![
            ch(0.0, cx(0.0, 0.0, 0.1, 0.1), Interp::Degrau),
            ch(1.0, cx(0.0, 0.0, 0.1, 0.1), Interp::Linear),
            ch(2.0, cx(0.5, 0.0, 0.1, 0.1), Interp::Linear),
        ];
        // degrau 0→2 seguraria (0,0) em t = 1: erro 0 → a do meio sai
        assert_eq!(simplificar(&ks, 0.0).len(), 2);
        let ks2 = vec![ch(0.0, cx(0.0, 0.0, 0.1, 0.1), Interp::Suave), ch(1.0, cx(0.25, 0.0, 0.1, 0.1), Interp::Linear), ch(2.0, cx(0.5, 0.0, 0.1, 0.1), Interp::Linear)];
        // suave em f = 0,5 dá o meio exato
        assert_eq!(simplificar(&ks2, 0.0).len(), 2);
        let ks3 = vec![ch(0.0, cx(0.0, 0.0, 0.1, 0.1), Interp::Suave), ch(0.5, cx(0.125, 0.0, 0.1, 0.1), Interp::Linear), ch(2.0, cx(0.5, 0.0, 0.1, 0.1), Interp::Linear)];
        // em f = 0,25 a suave dá 0,078125, não 0,125
        assert_eq!(simplificar(&ks3, 0.04).len(), 3);
        assert_eq!(simplificar(&ks3, 0.05).len(), 2);
    }

    /// Chaves sorteadas com as três interpolações misturadas: a tolerância vale nos instantes das
    /// chaves contra a curva simplificada (com a `interp` de quem ficou), e simplificar de novo não muda.
    #[test]
    fn simplificar_com_interpolacoes_misturadas() {
        let mut s: u64 = 0xabcd_ef01;
        let mut rnd = || {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((s >> 11) as f64) / ((1u64 << 53) as f64)
        };
        for rodada in 0..40 {
            let n = 3 + (rnd() * 200.0) as usize;
            let (mut x, mut y, mut t) = (0.4, 0.4, 0.0);
            let ks: Vec<Chave<Caixa>> = (0..n)
                .map(|_| {
                    t += 0.01 + rnd() * 0.2;
                    x += (rnd() - 0.5) * 0.02;
                    y += (rnd() - 0.5) * 0.02;
                    let interp = [Interp::Linear, Interp::Suave, Interp::Degrau][(rnd() * 3.0) as usize % 3];
                    ch(t, cx(x, y, 0.1 + rnd() * 0.01, 0.1), interp)
                })
                .collect();
            for tol in [0.0, 0.003, 0.01, 0.03] {
                let uma = simplificar(&ks, tol);
                let a = Animavel::Chaves(uma.clone());
                let pior = ks.iter().map(|k| desvio(&k.v, &avaliar(&a, k.t))).fold(0.0, f64::max);
                assert!(pior <= tol, "rodada {rodada} tol {tol}: desvio {pior}");
                assert_eq!(simplificar(&uma, tol), uma, "rodada {rodada} tol {tol}");
            }
        }
    }

    #[test]
    fn simplificar_no_empate_fica_a_primeira() {
        // as duas do meio erram igual (0,125) contra a reta 0 → 3; ficando uma, a outra erra 0,0625
        let ks = vec![
            ch(0.0, cx(0.0, 0.0, 0.25, 0.25), Interp::Linear),
            ch(1.0, cx(0.125, 0.0, 0.25, 0.25), Interp::Linear),
            ch(2.0, cx(0.125, 0.0, 0.25, 0.25), Interp::Linear),
            ch(3.0, cx(0.0, 0.0, 0.25, 0.25), Interp::Linear),
        ];
        let tempos = |tol: f64| simplificar(&ks, tol).iter().map(|k| k.t).collect::<Vec<_>>();
        assert_eq!(tempos(0.2), vec![0.0, 3.0]);
        assert_eq!(tempos(0.1), vec![0.0, 1.0, 3.0]);
        assert_eq!(tempos(0.05), vec![0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn simplificar_poucas_chaves_e_nao_finitas() {
        let ks = vec![ch(0.0, cx(0.0, 0.0, 0.1, 0.1), Interp::Linear), ch(1.0, cx(0.3, 0.0, 0.1, 0.1), Interp::Linear)];
        assert_eq!(simplificar(&ks, 1.0), ks);
        assert!(simplificar(&[], 1.0).is_empty());
        let nan = vec![ks[0].clone(), ch(0.5, cx(f64::NAN, 0.0, 0.1, 0.1), Interp::Linear), ks[1].clone()];
        assert_eq!(simplificar(&nan, 1.0).len(), 3);
    }

    /// Nada de f32 nem FMA no caminho das contas (os arquivos do P0.3a, fora dos testes).
    #[test]
    fn sem_f32_nem_fma() {
        let fontes = [
            ("curva.rs", include_str!("curva.rs")),
            ("geometria.rs", include_str!("geometria.rs")),
            ("cor.rs", include_str!("cor.rs")),
            ("rastreio/avaliar.rs", include_str!("rastreio/avaliar.rs")),
        ];
        // além de f32 e FMA: nada de potência, raiz ou trigonometria (podem mudar o último bit entre máquinas)
        let proibidos = [
            concat!("f", "32"),
            concat!("mul", "_add"),
            concat!("fm", "a("),
            concat!("pow", "i("),
            concat!("pow", "f("),
            concat!(".sq", "rt("),
            concat!(".si", "n("),
            concat!(".co", "s("),
            concat!(".ta", "n("),
            concat!("hyp", "ot("),
            concat!(".ex", "p("),
            concat!(".l", "n("),
        ];
        for (nome, texto) in fontes {
            let codigo = texto.split(concat!("#[cfg(", "test)]")).next().unwrap();
            for p in proibidos {
                assert!(!codigo.contains(p), "{nome} usa {p}");
            }
        }
    }

    fn caso(nome: &str, tipo: &str, a: Value, t: f64, saida: Value) -> Value {
        json!({ "nome": nome, "entrada": { "tipo": tipo, "a": a, "t": t }, "saida": saida })
    }

    /// `curva-avaliar.json`: as três interpolações, número, caixa e ponto, chave única, antes da primeira,
    /// depois da última, chaves no mesmo instante e tempos de 29,97 qps.
    #[test]
    fn fixture_avaliar() {
        let mut casos = Vec::new();
        let num = |ks: Vec<Chave<f64>>| Animavel::Chaves(ks);
        let animados: Vec<(&str, Animavel<f64>)> = vec![
            ("fixo", Animavel::Fixo(0.75)),
            ("linear", num(vec![ch(1.0, 0.0, Interp::Linear), ch(3.0, 10.0, Interp::Linear)])),
            ("suave", num(vec![ch(1.0, 0.0, Interp::Suave), ch(3.0, 10.0, Interp::Linear)])),
            ("degrau", num(vec![ch(1.0, 0.0, Interp::Degrau), ch(3.0, 10.0, Interp::Linear)])),
            ("chave unica", num(vec![ch(2.0, 0.3, Interp::Suave)])),
            ("mesmo instante", num(vec![ch(0.0, 0.0, Interp::Linear), ch(1.0, 2.0, Interp::Linear), ch(1.0, 6.0, Interp::Suave), ch(2.0, 8.0, Interp::Linear)])),
            (
                "tres trechos",
                num(vec![ch(0.0, 1.0, Interp::Suave), ch(0.7, 3.0, Interp::Degrau), ch(1.3, 2.0, Interp::Linear), ch(2.9, 0.1, Interp::Linear)]),
            ),
        ];
        let tempos = [-1.0, 0.0, 0.5, 1.0, 1.0 / 3.0, 1.5, 2.0, 2.5, 2.999, 3.0, 4.0, 1001.0 / 30000.0 * 37.0];
        for (nome, a) in &animados {
            for &t in &tempos {
                casos.push(caso(&format!("numero {nome} t={t}"), "numero", json!(a), t, json!(avaliar(a, t))));
            }
        }
        let zoom = Animavel::Chaves(vec![
            ch(0.0, cx(0.0, 0.0, 1.0, 1.0), Interp::Suave),
            ch(2.0, cx(0.3, 0.2, 0.4, 0.4), Interp::Linear),
            ch(3.5, cx(0.1, 0.5, 0.25, 0.25), Interp::Degrau),
            ch(4.0, cx(0.6, 0.6, 0.3, 0.3), Interp::Linear),
        ]);
        for t in [-0.5, 0.0, 0.6, 1.0, 1.9, 2.0, 2.75, 3.5, 3.9, 4.0, 10.0] {
            casos.push(caso(&format!("caixa zoom t={t}"), "caixa", json!(zoom), t, json!(avaliar(&zoom, t))));
        }
        let p = Animavel::Chaves(vec![ch(0.5, Ponto { x: 0.1, y: 0.9 }, Interp::Linear), ch(1.5, Ponto { x: 0.7, y: 0.2 }, Interp::Suave), ch(2.5, Ponto { x: 0.3, y: 0.3 }, Interp::Linear)]);
        for t in [0.0, 0.5, 0.8, 1.5, 2.1, 3.0] {
            casos.push(caso(&format!("ponto t={t}"), "ponto", json!(p), t, json!(avaliar(&p, t))));
        }
        // como o front grava: chave sem `interp` (= linear) e sem `pts`, números inteiros
        let cru = json!([{ "t": 0, "v": 1 }, { "t": 2, "v": 3, "interp": "suave" }, { "t": 4, "v": 0 }]);
        let a: Animavel<f64> = serde_json::from_value(cru.clone()).unwrap();
        for t in [0.5, 3.0, 5.0] {
            casos.push(caso(&format!("numero sem interp nem pts t={t}"), "numero", cru.clone(), t, json!(avaliar(&a, t))));
        }
        let cru = json!([{ "t": 1, "v": { "x": 0, "y": 0, "w": 1, "h": 1 } }, { "t": 2, "v": { "x": 0.5, "y": 0.25, "w": 0.5, "h": 0.5 } }]);
        let a: Animavel<Caixa> = serde_json::from_value(cru.clone()).unwrap();
        casos.push(caso("caixa sem interp nem pts t=1.25", "caixa", cru, 1.25, json!(avaliar(&a, 1.25))));
        fixtures::conferir("curva-avaliar.json", Value::Array(casos));
    }

    /// `curva-suave.json`: a cúbica em 0..1.
    #[test]
    fn fixture_suave() {
        let casos: Vec<Value> = (0..=20)
            .map(|i| {
                let f = i as f64 / 20.0;
                json!({ "nome": format!("f={f}"), "entrada": { "f": f }, "saida": suave(f) })
            })
            .chain([1.0 / 3.0, 0.1, 0.7, -0.5, 1.5, 2.0].iter().map(|&f| json!({ "nome": format!("f={f}"), "entrada": { "f": f }, "saida": suave(f) })))
            .collect();
        fixtures::conferir("curva-suave.json", Value::Array(casos));
    }
}
