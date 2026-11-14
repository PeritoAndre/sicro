//! `ocultar` (§5.3, §5.4): primeiro efeito depois da cor, no espaço original, máscara por quadro lido.
//! A mesma máquina (recorte da união → variante → `alphamerge` → `overlay`) serve ao holofote, por slot.
//! Dono: P0.6a.

use crate::edicao::cor;
use crate::edicao::modelo::{EfHolofote, EfOcultar, ModoOcultar};
use crate::edicao::motor::grafo::{filtro, scale, Grafo};
use crate::error::Result;

/// Fração do lado menor da região que vira raio do desfoque na intensidade 1.
const RAIO_DESFOQUE: f64 = 0.25;
/// Células do mosaico no lado menor da região: de 12 (intensidade 0) a 1 (intensidade 1).
const CELULAS_MAX: f64 = 12.0;

/// Variante que troca o recorte `[x, y, w, h]`: desfoque, mosaico ou sólido. `lado` = maior lado menor da região (px).
fn variante(o: &EfOcultar, recorte: [u32; 4], lado: u32) -> Result<Vec<String>> {
    let [_, _, w, h] = recorte;
    let intensidade = if o.intensidade.is_finite() { o.intensidade.clamp(0.0, 1.0) } else { 1.0 };
    let lado = lado.clamp(2, w.min(h).max(2)) as f64;
    Ok(match o.modo {
        ModoOcultar::Solido => vec![cor::filtro_cor_cheia(cor::ler(&o.cor)?)],
        ModoOcultar::Desfoque => {
            // o 9.0.1 recusa 2·raio ≥ lado do plano; croma tem metade do recorte
            let lr = ((RAIO_DESFOQUE * intensidade * lado).round() as u32).clamp(1, ((w.min(h).max(3) - 1) / 2).max(1));
            let cr = lr.min(((w.min(h) / 2).max(3) - 1) / 2).max(1);
            vec![format!("boxblur=lr={lr}:lp=3:cr={cr}:cp=3")]
        }
        ModoOcultar::Mosaico => {
            let celulas = (1.0 + (1.0 - intensidade) * (CELULAS_MAX - 1.0)).round();
            let celula = (lado / celulas).max(2.0);
            let reduz = |v: u32| ((v as f64 / celula).ceil() as u32).max(1);
            vec![format!("scale={}:{}:flags=area+accurate_rnd+bitexact", reduz(w), reduz(h)), scale(w, h, "neighbor")]
        }
    })
}

/// `split` → recorte → variante → `alphamerge` com a máscara → `overlay` de volta. Devolve o rótulo de saída.
fn trocar(g: &mut Grafo, entrada: &str, recorte: [u32; 4], variante: Vec<String>, mascara: &str) -> String {
    let [x, y, w, h] = recorte;
    let (a, b, var, fx, saida) = (g.rotulo("r"), g.rotulo("r"), g.rotulo("r"), g.rotulo("r"), g.rotulo("r"));
    g.cadeia(&[entrada], &["split".to_string()], &[&a, &b]);
    let mut f = vec![format!("crop={w}:{h}:{x}:{y}:exact=1")];
    f.extend(variante);
    g.cadeia(&[&b], &f, &[&var]);
    g.cadeia(&[&var, mascara], &["alphamerge".to_string()], &[&fx]);
    let o = filtro("overlay", None, &[("x", x.to_string()), ("y", y.to_string()), ("format", "yuv420".into()), ("eof_action", "pass".into())]);
    g.cadeia(&[&a, &fx], &[o], &[&saida]);
    saida
}

/// Aplica as ocultações à cadeia `entrada` (fonte já na cor interna) e devolve o rótulo de saída.
/// `mascaras[i]` = rótulo do fluxo da máscara de `ocultacoes[i]` (cinza, pts imposto); `uniao_px[i]` = recorte par [x, y, w, h];
/// `lado_px[i]` = maior lado menor da região entre os quadros (px): dá o raio do desfoque e a célula do mosaico.
// CONTRATO: `lado_px` a mais. O recorte é a união de todos os quadros: tirado dele, o raio cresceria com o caminho do alvo.
pub fn ocultar(
    g: &mut Grafo,
    entrada: &str,
    ocultacoes: &[&EfOcultar],
    mascaras: &[String],
    uniao_px: &[[u32; 4]],
    lado_px: &[u32],
) -> Result<String> {
    let mut atual = entrada.to_string();
    for (i, o) in ocultacoes.iter().enumerate() {
        let v = variante(o, uniao_px[i], lado_px[i])?;
        atual = trocar(g, &atual, uniao_px[i], v, &mascaras[i]);
    }
    Ok(atual)
}

/// Holofote: a variante escurecida e dessaturada entra onde a máscara (por slot, px do canvas) manda.
pub(crate) fn holofote(g: &mut Grafo, entrada: &str, h: &EfHolofote, mascara: &str, recorte: [u32; 4]) -> String {
    let f = |v: f64| if v.is_finite() { 1.0 - v.clamp(0.0, 1.0) } else { 1.0 };
    // escurecer multiplica o RGB: o croma encolhe junto com a luma
    let (ky, kc) = (f(h.escurecer), f(h.dessaturar) * f(h.escurecer));
    let lut = filtro(
        "lutyuv",
        None,
        &[
            ("y", format!("clip(round(16+(val-16)*{ky}),16,235)")),
            ("u", format!("clip(round(128+(val-128)*{kc}),16,240)")),
            ("v", format!("clip(round(128+(val-128)*{kc}),16,240)")),
        ],
    );
    trocar(g, entrada, recorte, vec![lut], mascara)
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    fn ocultacao(modo: &str, intensidade: f64) -> EfOcultar {
        serde_json::from_value(json!({
            "id": "o1", "ligado": true, "nome": "", "ativo": null, "modo": modo,
            "alvo": { "rastreio": "r1", "forma": "retangulo", "vertices": null, "margem": 0.0, "borda_suave": 0.0, "inverter": false },
            "intensidade": intensidade, "cor": "#000000", "conteudo": "rosto", "motivo": ""
        }))
        .unwrap()
    }

    #[test]
    fn cadeia_do_ocultar_solido_e_a_do_ensaio() {
        let mut g = Grafo::novo();
        let o = ocultacao("solido", 1.0);
        let s = ocultar(&mut g, "cor", &[&o], &["mk".into()], &[[176, 188, 244, 154]], &[64]).unwrap();
        assert_eq!(
            g.texto(),
            format!(
                "[cor]split[r000][r001];\n[r001]crop=244:154:176:188:exact=1,lutyuv=y=16:u=128:v=128[r002];\n[r002][mk]alphamerge[r003];\n\
                 [r000][r003]overlay=x=176:y=188:format=yuv420:eof_action=pass[{s}]"
            )
        );
    }

    #[test]
    fn desfoque_respeita_o_limite_do_boxblur_e_mosaico_tem_de_1_a_12_celulas() {
        // intensidade 1 num recorte do tamanho da região: 2·raio < lado, no luma e no croma (T3)
        for (w, h, lado) in [(64u32, 64u32, 64u32), (20, 60, 20), (6, 6, 6), (2, 2, 2), (400, 300, 80)] {
            let v = variante(&ocultacao("desfoque", 1.0), [0, 0, w, h], lado).unwrap();
            let nums: Vec<u32> = v[0].split(['=', ':']).filter_map(|x| x.parse().ok()).collect();
            let (lr, cr) = (nums[0], nums[2]);
            assert!(lr >= 1 && (2 * lr < w.min(h) || w.min(h) <= 2), "{v:?}");
            assert!(cr >= 1 && (2 * cr < (w.min(h) / 2).max(3)), "{v:?}");
        }
        let v = variante(&ocultacao("desfoque", 0.5), [0, 0, 400, 300], 80).unwrap();
        assert_eq!(v, vec!["boxblur=lr=10:lp=3:cr=10:cp=3"]);
        // mosaico: 1 célula no lado menor na intensidade 1; 12 na intensidade 0
        assert_eq!(variante(&ocultacao("mosaico", 1.0), [0, 0, 120, 60], 60).unwrap()[0], "scale=2:1:flags=area+accurate_rnd+bitexact");
        assert_eq!(variante(&ocultacao("mosaico", 0.0), [0, 0, 120, 60], 60).unwrap()[0], "scale=24:12:flags=area+accurate_rnd+bitexact");
        assert_eq!(variante(&ocultacao("mosaico", 0.0), [0, 0, 120, 60], 60).unwrap()[1], "scale=120:60:flags=neighbor+accurate_rnd+bitexact");
    }
}
