//! Avaliação de um rastreio num quadro (§6.1). Espelho no front: `modelo/rastreio.ts`. Dono: P0.3a.
//! Ordem: âncora no quadro → amostra densa (ok ou incerta) → interpolação entre âncoras →
//! depois da última âncora segura a última (só origem manual) → antes da primeira, nada.

use serde::{Deserialize, Serialize};

use super::ArquivoDenso;
use crate::edicao::curva;
use crate::edicao::modelo::{Caixa, Chave, OrigemRastreio, Rastreio};

/// Onde avaliar: tempo (s) na base do rastreio e, quando há mídia, o pts do quadro lido ou exibido.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Instante {
    pub t: f64,
    pub pts: Option<i64>,
}

/// De onde saiu a caixa.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Como {
    Ancora,
    Denso,
    Interpolado,
    Segurado,
}

/// Caixa (0..1, no espaço do rastreio) e como foi obtida; `incerto` = amostra densa de estado incerto.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Avaliado {
    pub caixa: Caixa,
    pub como: Como,
    pub incerto: bool,
}

/// Âncora "neste quadro": mesmo pts quando os dois têm; senão o mesmo `t`. Procura só em volta de `n`
/// (chaves com t ≤ instante): o ruído de `t` não passa de um quadro. Duas no mesmo quadro: vale a última.
fn ancora_no_quadro(ks: &[Chave<Caixa>], n: usize, em: Instante) -> Option<usize> {
    let mut achada = None;
    for i in n.saturating_sub(2)..(n + 2).min(ks.len()) {
        let mesmo = match (em.pts, ks[i].pts) {
            (Some(a), Some(b)) => a == b,
            _ => ks[i].t == em.t,
        };
        if mesmo {
            achada = Some(i);
        }
    }
    achada
}

/// Ponto de extensão da Fase 2 (leitor de denso): `Some(Some(_))` = amostra ok ou incerta nesse pts,
/// `Some(None)` = estado ausente (nada), `None` = sem linha nesse quadro (segue pelas âncoras).
/// `como` = `Denso` na linha medida e `Interpolado` na de `tipo` interpolado ou estendido (a validação conta
/// `ocultar_interpolado` e a conferência marca o trecho fraco por isso); `incerto` = estado incerto (P2.2).
/// Na Fase 0 não há denso: segue sempre pelas âncoras.
fn do_denso(_d: &ArquivoDenso, _em: Instante) -> Option<Option<Avaliado>> {
    None
}

/// Caixa do rastreio no instante; None = "nada" (sem cobertura). `margem` e `forma` entram depois.
/// Entre âncoras vale a `interp` da âncora anterior (`curva`); a mesma conta do `Animavel`.
pub fn avaliar(r: &Rastreio, denso: Option<&ArquivoDenso>, em: Instante) -> Option<Avaliado> {
    let ks = &r.ancoras;
    let n = ks.partition_point(|k| k.t <= em.t);
    if let Some(i) = ancora_no_quadro(ks, n, em) {
        return Some(Avaliado { caixa: ks[i].v, como: Como::Ancora, incerto: false });
    }
    if let Some(d) = denso {
        if let Some(v) = do_denso(d, em) {
            return v;
        }
    }
    if n == 0 {
        return None;
    }
    if n == ks.len() {
        let segura = matches!(r.origem, OrigemRastreio::Manual);
        return segura.then_some(Avaliado { caixa: ks[n - 1].v, como: Como::Segurado, incerto: false });
    }
    Some(Avaliado { caixa: curva::entre(&ks[n - 1], &ks[n], em.t), como: Como::Interpolado, incerto: false })
}

/// O instante cai numa ausência aceita pelo perito? Intervalo fechado `[de_s, ate_s]`.
pub fn em_ausencia(r: &Rastreio, t: f64) -> bool {
    r.ausencias.iter().any(|a| a.de_s <= t && t <= a.ate_s)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::curva::fixtures;
    use crate::edicao::modelo::{Ausencia, BaseRastreio, ClasseDeteccao, EstadoPessoa, Interp, MotivoAusencia, TipoRastreio};
    use serde_json::{json, Value};

    /// Base de tempo 1/15360 (MP4): quadro k a 30 qps tem pts 512·k e t = pts/15360.
    fn ch(k: i64, v: Caixa, interp: Interp) -> Chave<Caixa> {
        Chave { t: (512 * k) as f64 / 15360.0, pts: Some(512 * k), v, interp }
    }
    fn em(k: i64) -> Instante {
        Instante { t: (512 * k) as f64 / 15360.0, pts: Some(512 * k) }
    }
    fn cx(x: f64, y: f64, w: f64, h: f64) -> Caixa {
        Caixa { x, y, w, h }
    }
    /// Quadro de pts `pts` numa base de tempo `num/den`.
    fn em_tb(pts: i64, num: i64, den: i64) -> Instante {
        Instante { t: (pts * num) as f64 / den as f64, pts: Some(pts) }
    }
    fn ch_tb(pts: i64, num: i64, den: i64, v: Caixa, interp: Interp) -> Chave<Caixa> {
        Chave { t: em_tb(pts, num, den).t, pts: Some(pts), v, interp }
    }
    /// (nome, base de tempo, pts do quadro k): 29,97 qps em 1/30000; TS 1/90000 que começa em 1,4 s;
    /// MKV 1/1000 (pts arredondado ao milissegundo, passo de 33 ou 34).
    fn bases() -> Vec<(&'static str, i64, i64, fn(i64) -> i64)> {
        vec![
            ("29,97 em 1/30000", 1, 30000, |k| 1001 * k),
            ("TS 1/90000 com inicio em 1,4 s", 1, 90000, |k| 126000 + 3003 * k),
            ("MKV 1/1000", 1, 1000, |k| (k * 1000 + 15) / 30),
        ]
    }
    fn rastreio(ancoras: Vec<Chave<Caixa>>, origem: OrigemRastreio) -> Rastreio {
        Rastreio {
            id: "r1".into(),
            nome: String::new(),
            cor: "#ffcc00".into(),
            midia: Some("m1".into()),
            base: BaseRastreio::Fonte,
            tipo: TipoRastreio::Caixa,
            espaco: "original".into(),
            ancoras,
            denso: None,
            silhuetas: None,
            ausencias: vec![],
            origem,
            confirmacao: None,
        }
    }
    fn tres() -> Vec<Chave<Caixa>> {
        vec![
            ch(30, cx(0.1, 0.1, 0.2, 0.2), Interp::Linear),
            ch(60, cx(0.5, 0.3, 0.2, 0.4), Interp::Suave),
            ch(90, cx(0.7, 0.3, 0.1, 0.1), Interp::Degrau),
        ]
    }

    #[test]
    fn interpolacao_entre_ancoras() {
        let r = rastreio(tres(), OrigemRastreio::Manual);
        // linear: quadro 45 = meio entre 30 e 60
        let a = avaliar(&r, None, em(45)).unwrap();
        assert_eq!(a.como, Como::Interpolado);
        assert!(!a.incerto);
        let esperado = cx(0.3, 0.2, 0.2, 0.3);
        assert!((a.caixa.x - esperado.x).abs() < 1e-15 && (a.caixa.y - esperado.y).abs() < 1e-15);
        assert!((a.caixa.w - esperado.w).abs() < 1e-15 && (a.caixa.h - esperado.h).abs() < 1e-15);
        // suave: quadro 75 = meio entre 60 e 90 (suave(0,5) = 0,5)
        let a = avaliar(&r, None, em(75)).unwrap();
        assert!((a.caixa.x - 0.6).abs() < 1e-15 && (a.caixa.w - 0.15).abs() < 1e-15);
        // na âncora: a própria, como âncora
        for (k, i) in [(30, 0), (60, 1), (90, 2)] {
            let a = avaliar(&r, None, em(k)).unwrap();
            assert_eq!((a.caixa, a.como), (r.ancoras[i].v, Como::Ancora));
        }
    }

    #[test]
    fn ancora_pelo_pts_mesmo_com_t_ruidoso() {
        let r = rastreio(tres(), OrigemRastreio::Assistido);
        for desvio in [-1e-9, 1e-9] {
            let e = Instante { t: r.ancoras[0].t + desvio, pts: r.ancoras[0].pts };
            assert_eq!(avaliar(&r, None, e).map(|a| a.como), Some(Como::Ancora));
            let e = Instante { t: r.ancoras[2].t + desvio, pts: r.ancoras[2].pts };
            assert_eq!(avaliar(&r, None, e).map(|a| a.como), Some(Como::Ancora));
        }
        // sem pts no instante, só o t exato casa
        let e = Instante { t: r.ancoras[1].t, pts: None };
        assert_eq!(avaliar(&r, None, e).unwrap().como, Como::Ancora);
    }

    /// Taxa 29,97, início ≠ 0 e base de tempo grossa: âncora pelo pts, meio do caminho e bordas.
    #[test]
    fn bases_de_tempo_reais() {
        for (nome, num, den, pts) in bases() {
            let a0 = cx(0.1, 0.2, 0.2, 0.2);
            let a1 = cx(0.5, 0.6, 0.4, 0.1);
            let r = rastreio(vec![ch_tb(pts(100), num, den, a0, Interp::Linear), ch_tb(pts(200), num, den, a1, Interp::Linear)], OrigemRastreio::Assistido);
            assert!(avaliar(&r, None, em_tb(pts(99), num, den)).is_none(), "{nome}");
            assert!(avaliar(&r, None, em_tb(pts(201), num, den)).is_none(), "{nome}");
            for (k, v) in [(100, a0), (200, a1)] {
                let a = avaliar(&r, None, em_tb(pts(k), num, den)).unwrap();
                assert_eq!((a.caixa, a.como), (v, Como::Ancora), "{nome}");
            }
            for k in [101, 150, 199] {
                let e = em_tb(pts(k), num, den);
                let a = avaliar(&r, None, e).unwrap();
                assert_eq!(a.como, Como::Interpolado, "{nome} {k}");
                // fração pelo pts (inteiros): a conta em segundos não pode se afastar dela
                let f = (pts(k) - pts(100)) as f64 / (pts(200) - pts(100)) as f64;
                assert!((a.caixa.x - (0.1 + 0.4 * f)).abs() < 1e-12 && (a.caixa.h - (0.2 - 0.1 * f)).abs() < 1e-12, "{nome} {k}: {:?}", a.caixa);
            }
        }
    }

    #[test]
    fn ultima_ancora_segura_so_no_manual() {
        let manual = rastreio(tres(), OrigemRastreio::Manual);
        for k in [91, 120, 100_000] {
            let a = avaliar(&manual, None, em(k)).unwrap();
            assert_eq!((a.caixa, a.como), (manual.ancoras[2].v, Como::Segurado));
        }
        for origem in [
            OrigemRastreio::Assistido,
            OrigemRastreio::Pessoas { pessoa_id: "p".into(), estado: EstadoPessoa::Revisada, copiado_em: "2026-11-14T00:00:00Z".into() },
            OrigemRastreio::Deteccao { classe: ClasseDeteccao::Placa },
        ] {
            let r = rastreio(tres(), origem);
            assert!(avaliar(&r, None, em(91)).is_none());
            assert_eq!(avaliar(&r, None, em(60)).unwrap().como, Como::Ancora);
        }
    }

    #[test]
    fn antes_da_primeira_nada() {
        for origem in [OrigemRastreio::Manual, OrigemRastreio::Assistido] {
            let r = rastreio(tres(), origem);
            assert!(avaliar(&r, None, em(29)).is_none());
            assert!(avaliar(&r, None, em(0)).is_none());
            assert!(avaliar(&r, None, Instante { t: -5.0, pts: None }).is_none());
        }
        assert!(avaliar(&rastreio(vec![], OrigemRastreio::Manual), None, em(10)).is_none());
    }

    #[test]
    fn ancora_unica() {
        let r = rastreio(vec![ch(100, cx(0.4, 0.4, 0.1, 0.1), Interp::Linear)], OrigemRastreio::Manual);
        assert!(avaliar(&r, None, em(99)).is_none());
        assert_eq!(avaliar(&r, None, em(100)).unwrap().como, Como::Ancora);
        let a = avaliar(&r, None, em(5000)).unwrap();
        assert_eq!((a.caixa, a.como), (r.ancoras[0].v, Como::Segurado));
    }

    #[test]
    fn degrau_segura_ate_a_proxima() {
        let mut ks = tres();
        ks[0].interp = Interp::Degrau;
        let r = rastreio(ks, OrigemRastreio::Manual);
        let a = avaliar(&r, None, em(59)).unwrap();
        assert_eq!((a.caixa, a.como), (r.ancoras[0].v, Como::Interpolado));
    }

    #[test]
    fn ausencia_fechada() {
        let mut r = rastreio(tres(), OrigemRastreio::Assistido);
        r.ausencias.push(Ausencia { de_s: 4.0, ate_s: 5.0, motivo: MotivoAusencia::Oculto, aceita_em: "2026-11-14T00:00:00Z".into(), por: "perito".into() });
        assert!(!em_ausencia(&r, 3.999));
        assert!(em_ausencia(&r, 4.0));
        assert!(em_ausencia(&r, 4.5));
        assert!(em_ausencia(&r, 5.0));
        assert!(!em_ausencia(&r, 5.001));
        assert!(!em_ausencia(&rastreio(vec![], OrigemRastreio::Manual), 4.5));
    }

    fn caso(nome: &str, r: &Rastreio, e: Instante) -> Value {
        json!({ "nome": nome, "entrada": { "rastreio": r, "em": e }, "saida": avaliar(r, None, e) })
    }

    /// `rastreio-avaliar.json`: interpolação (as três), âncora pelo pts, âncora única, antes da primeira,
    /// depois da última (manual segura; assistido, pessoas e detecção não), base local sem pts, ponto.
    #[test]
    fn fixture_avaliar() {
        let mut casos = Vec::new();
        let manual = rastreio(tres(), OrigemRastreio::Manual);
        for k in [0, 29, 30, 31, 45, 59, 60, 61, 75, 89, 90, 91, 300] {
            casos.push(caso(&format!("manual quadro {k}"), &manual, em(k)));
        }
        casos.push(caso("manual pts da ancora com t ruidoso", &manual, Instante { t: manual.ancoras[1].t - 1e-9, pts: manual.ancoras[1].pts }));
        casos.push(caso("manual sem pts no t da ancora", &manual, Instante { t: manual.ancoras[1].t, pts: None }));
        let assistido = rastreio(tres(), OrigemRastreio::Assistido);
        for k in [10, 30, 50, 90, 91] {
            casos.push(caso(&format!("assistido quadro {k}"), &assistido, em(k)));
        }
        let pessoas = rastreio(tres(), OrigemRastreio::Pessoas { pessoa_id: "p7".into(), estado: EstadoPessoa::Auto, copiado_em: "2026-11-14T00:00:00Z".into() });
        casos.push(caso("pessoas depois da ultima", &pessoas, em(120)));
        let deteccao = rastreio(tres(), OrigemRastreio::Deteccao { classe: ClasseDeteccao::Rosto });
        casos.push(caso("deteccao depois da ultima", &deteccao, em(120)));
        let unica = rastreio(vec![ch(100, cx(0.4, 0.4, 0.1, 0.1), Interp::Suave)], OrigemRastreio::Manual);
        for k in [99, 100, 101, 5000] {
            casos.push(caso(&format!("ancora unica quadro {k}"), &unica, em(k)));
        }
        let mut local = rastreio(
            vec![
                Chave { t: 0.5, pts: None, v: cx(0.2, 0.2, 0.1, 0.1), interp: Interp::Linear },
                Chave { t: 1.5, pts: None, v: cx(0.6, 0.4, 0.2, 0.1), interp: Interp::Linear },
            ],
            OrigemRastreio::Manual,
        );
        local.midia = None;
        local.base = BaseRastreio::Local;
        for t in [0.0, 0.5, 0.8, 1.5, 2.0] {
            casos.push(caso(&format!("local t={t}"), &local, Instante { t, pts: None }));
        }
        let mut ponto = rastreio(
            vec![ch(10, cx(0.25, 0.75, 0.0, 0.0), Interp::Suave), ch(40, cx(0.75, 0.25, 0.0, 0.0), Interp::Linear)],
            OrigemRastreio::Manual,
        );
        ponto.tipo = TipoRastreio::Ponto;
        for k in [5, 10, 20, 25, 40, 41] {
            casos.push(caso(&format!("ponto quadro {k}"), &ponto, em(k)));
        }
        for (nome, num, den, pts) in bases() {
            let r = rastreio(
                vec![
                    ch_tb(pts(100), num, den, cx(0.1, 0.2, 0.2, 0.2), Interp::Linear),
                    ch_tb(pts(137), num, den, cx(0.5, 0.6, 0.4, 0.1), Interp::Suave),
                    ch_tb(pts(200), num, den, cx(0.3, 0.3, 0.3, 0.3), Interp::Linear),
                ],
                OrigemRastreio::Manual,
            );
            for k in [99, 100, 101, 136, 137, 138, 163, 199, 200, 201] {
                casos.push(caso(&format!("{nome} quadro {k}"), &r, em_tb(pts(k), num, den)));
            }
        }
        // como o front grava: âncora sem `interp` (= linear); âncora sem `pts` num rastreio de fonte
        // só é "âncora neste quadro" pelo `t` exato
        let cru = json!({ "id": "r2", "nome": "", "cor": "#ffcc00", "midia": "m1", "base": "fonte", "tipo": "caixa", "espaco": "original",
            "ancoras": [{ "t": 1, "pts": 15360, "v": { "x": 0.1, "y": 0.1, "w": 0.2, "h": 0.2 } }, { "t": 2, "v": { "x": 0.5, "y": 0.3, "w": 0.2, "h": 0.4 } }],
            "denso": null, "silhuetas": null, "ausencias": [], "origem": { "tipo": "manual" }, "confirmacao": null });
        let r: Rastreio = serde_json::from_value(cru.clone()).unwrap();
        for (nome, e) in [
            ("meio", Instante { t: 1.5, pts: Some(23040) }),
            ("na ancora sem pts, t exato", Instante { t: 2.0, pts: Some(30720) }),
            ("na ancora sem pts, t ruidoso", Instante { t: 2.0 - 1e-9, pts: Some(30720) }),
            ("depois", Instante { t: 2.5, pts: Some(38400) }),
        ] {
            casos.push(json!({ "nome": format!("ancoras cruas: {nome}"), "entrada": { "rastreio": cru, "em": e }, "saida": avaliar(&r, None, e) }));
        }
        fixtures::conferir("rastreio-avaliar.json", Value::Array(casos));
    }

    /// `rastreio-ausencia.json`.
    #[test]
    fn fixture_ausencia() {
        let mut r = rastreio(tres(), OrigemRastreio::Assistido);
        r.ausencias = vec![
            Ausencia { de_s: 1.2, ate_s: 1.6, motivo: MotivoAusencia::Oculto, aceita_em: "2026-11-14T00:00:00Z".into(), por: "perito".into() },
            Ausencia { de_s: 2.5, ate_s: 2.5, motivo: MotivoAusencia::ForaDeCena, aceita_em: "2026-11-14T00:00:00Z".into(), por: "perito".into() },
        ];
        let casos: Vec<Value> = [0.0, 1.2, 1.4, 1.6, 1.6000001, 2.4999, 2.5, 2.6]
            .iter()
            .map(|&t| json!({ "nome": format!("t={t}"), "entrada": { "rastreio": r, "t": t }, "saida": em_ausencia(&r, t) }))
            .collect();
        fixtures::conferir("rastreio-ausencia.json", Value::Array(casos));
    }
}
