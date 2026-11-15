//! `ocultar` (§5.3, §5.4): primeiro efeito depois da cor, no espaço original, máscara por quadro lido.
//! A mesma máquina (recorte da união → variante → `alphamerge` → `overlay`) serve ao holofote, por slot.
//! Dono: P0.6a.

use crate::edicao::cor;
use crate::edicao::modelo::{ConteudoOculto, EfHolofote, EfOcultar, ModoOcultar};
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
/// `eof_action=repeat`: um quadro a mais numa leitura anormal (a conferência acusa, mas o bloco já existe) sai
/// coberto pela última máscara, nunca descoberto.
fn trocar(g: &mut Grafo, entrada: &str, recorte: [u32; 4], variante: Vec<String>, mascara: &str) -> String {
    let [x, y, w, h] = recorte;
    let (a, b, var, fx, saida) = (g.rotulo("r"), g.rotulo("r"), g.rotulo("r"), g.rotulo("r"), g.rotulo("r"));
    g.cadeia(&[entrada], &["split".to_string()], &[&a, &b]);
    let mut f = vec![format!("crop={w}:{h}:{x}:{y}:exact=1")];
    f.extend(variante);
    g.cadeia(&[&b], &f, &[&var]);
    g.cadeia(&[&var, mascara], &["alphamerge".to_string()], &[&fx]);
    let o = filtro("overlay", None, &[("x", x.to_string()), ("y", y.to_string()), ("format", "yuv420".into()), ("eof_action", "repeat".into())]);
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

// CONTRATO: abertura da Fase 2 (contratos-fase2 (c)): a força da ocultação é regra do P2.6 (mede o ataque e fixa os
// limiares, §12 item 4); a validação pergunta aqui.
// Medidos pelo ataque do E7b contra o motor (`testes::limiares_pelo_ataque`: 16 placas por ponto, H.264 crf 18, o
// atacante passa cada caractere pelo mesmo filtro): abaixo de 10 % na placa de letra de 14 px só o desfoque 1,0 (7 %;
// 0,8 dá 35 %) e o mosaico de 1 célula (8 %; 2 células, 24 %). Na letra de 28 px nem eles (25 % e 12 %).
/// Desfoque abaixo disso é fraco.
pub(crate) const DESFOQUE_MIN: f64 = 1.0;
/// Mosaico com mais células que isso no lado menor é fraco.
pub(crate) const MOSAICO_MAX_CELULAS: f64 = 1.0;

/// Conteúdo com texto: mosaico e desfoque não bastam (§6.6 (2)).
fn de_texto(c: ConteudoOculto) -> bool {
    matches!(c, ConteudoOculto::Placa | ConteudoOculto::Documento | ConteudoOculto::Tela)
}

/// `ocultacao_reversivel`: mosaico ou desfoque em placa, documento ou tela, em qualquer intensidade (§6.6 (2)).
pub(crate) fn reversivel(o: &EfOcultar) -> bool {
    o.modo != ModoOcultar::Solido && de_texto(o.conteudo)
}

/// Padrão por conteúdo (§6.6 (1)): texto → sólido; rosto, corpo e outro → mosaico 0,75 (4 células). Quem aplica é a
/// tela (`inspetor/secoes/ocultarPadrao.ts::padraoDoConteudo`); aqui é a referência do ataque e da fixture
/// `conferencia-padrao.json`, que o espelho confere.
#[cfg(test)]
pub(crate) fn padrao(c: ConteudoOculto) -> (ModoOcultar, f64) {
    if de_texto(c) {
        (ModoOcultar::Solido, 1.0)
    } else {
        (ModoOcultar::Mosaico, 0.75)
    }
}

/// `ocultacao_fraca`: desfoque abaixo de `DESFOQUE_MIN` ou mosaico com mais de `MOSAICO_MAX_CELULAS` células.
pub(crate) fn fraca(o: &EfOcultar) -> bool {
    let i = o.intensidade.clamp(0.0, 1.0);
    match o.modo {
        ModoOcultar::Desfoque => o.intensidade < DESFOQUE_MIN,
        ModoOcultar::Mosaico => (CELULAS_MAX - (CELULAS_MAX - 1.0) * i).round() > MOSAICO_MAX_CELULAS,
        ModoOcultar::Solido => false,
    }
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
pub(crate) mod testes {
    use super::*;
    use crate::edicao::ffmpeg::exec;
    use crate::edicao::ffmpeg::sonda::Capacidades;
    use crate::edicao::testes::aceite::{efeito, ocultar as ocultar_json};
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

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
                 [r000][r003]overlay=x=176:y=188:format=yuv420:eof_action=repeat[{s}]"
            )
        );
    }

    /// Leitura anormal: a base traz um quadro a mais que as máscaras (6 × 5). O quadro a mais sai coberto, nos dois
    /// binários (com `eof_action=pass` saía com o alvo à mostra).
    #[test]
    fn quadro_a_mais_sai_coberto() {
        use crate::edicao::ffmpeg::{args, exec};
        let Some(cap) = crate::edicao::testes::fontes::cap() else { return };
        let dir = tempfile::Builder::new().prefix("p0f-regiao-").tempdir().unwrap();
        let mut g = Grafo::novo();
        g.cadeia(&["0:v"], &["trim=end_frame=6".into(), "format=yuv420p".into()], &["base"]);
        g.cadeia(&["1:v"], &["trim=end_frame=5".into(), "format=gray".into()], &["mk"]);
        let o = ocultacao("solido", 1.0);
        let s = ocultar(&mut g, "base", &[&o], &["mk".into()], &[[20, 20, 40, 40]], &[40]).unwrap();
        g.cadeia(&[&s], &["format=rgb24".into()], &["x"]);
        args::gravar_texto(dir.path(), "g.txt", &g.texto()).unwrap();
        let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=s=160x90:r=30:d=1", "-f", "lavfi", "-i", "color=c=white:s=40x40:r=30:d=1"]
            .map(String::from)
            .into();
        a.extend(args::grafo_em_arquivo(&cap, "g.txt"));
        a.extend(["-map", "[x]", "-fps_mode", "passthrough", "-f", "rawvideo", "-"].map(String::from));
        let cmd = exec::Comando { nome: "ocultar".into(), programa: cap.binario.clone(), args: a, pasta: dir.path().into(), ambiente: vec![], sem_ambiente: vec![], stderr: dir.path().join("e.log") };
        let (fim, out) = exec::rodar_com_saida(&cmd, &std::sync::atomic::AtomicBool::new(false)).unwrap();
        exec::exigir_sucesso(&cmd, &fim).unwrap();
        let tam = 160 * 90 * 3;
        assert_eq!(out.len(), 6 * tam, "{}", cap.versao);
        for (i, q) in out.chunks_exact(tam).enumerate() {
            // dentro do recorte: o sólido preto em todo pixel, inclusive no 6º quadro
            let fora_do_preto = (20..60).flat_map(|y| (20..60).map(move |x| (y * 160 + x) * 3)).filter(|&k| q[k..k + 3].iter().any(|&v| v > 2)).count();
            assert_eq!(fora_do_preto, 0, "quadro {i} descoberto ({})", cap.versao);
        }
    }

    /// Força da ocultação com os limiares medidos (`limiares_pelo_ataque`, §12 item 4); até a abertura da Fase 2 eram
    /// de `validar.rs`, que agora só pergunta aqui.
    #[test]
    fn reversivel_e_fraca_nos_limiares_de_hoje() {
        let com = |modo: &str, i: f64, conteudo: &str| {
            let mut o = ocultacao(modo, i);
            o.conteudo = serde_json::from_value(json!(conteudo)).unwrap();
            o
        };
        assert!(reversivel(&com("mosaico", 1.0, "placa")));
        assert!(reversivel(&com("desfoque", 1.0, "documento")));
        assert!(reversivel(&com("desfoque", 0.1, "tela")));
        assert!(!reversivel(&com("solido", 1.0, "tela")));
        assert!(!reversivel(&com("mosaico", 1.0, "rosto")));
        assert!(fraca(&com("desfoque", 0.99, "rosto")));
        assert!(fraca(&com("desfoque", 0.6, "rosto")));
        assert!(!fraca(&com("desfoque", 1.0, "rosto")));
        // mosaico: round(12 − 11·i) células; 0,95 → 2 (fraca), 1,0 → 1; o padrão de rosto (0,75: 4 células) é fraco
        assert!(fraca(&com("mosaico", 0.95, "rosto")));
        assert!(!fraca(&com("mosaico", 1.0, "rosto")));
        let (modo, i) = padrao(ConteudoOculto::Rosto);
        let mut o = com("mosaico", i, "rosto");
        o.modo = modo;
        assert!(fraca(&o));
        assert!(!fraca(&com("solido", 0.0, "placa")));
    }

    /// Padrão por conteúdo: a fixture que o espelho do front (`ocultarPadrao.ts`) confere.
    #[test]
    fn padrao_por_conteudo() {
        let casos: Vec<serde_json::Value> = ["rosto", "placa", "corpo", "tela", "documento", "outro"]
            .iter()
            .map(|c| {
                let (modo, intensidade) = padrao(serde_json::from_value(json!(c)).unwrap());
                json!({ "nome": c, "entrada": c, "saida": { "modo": modo, "intensidade": intensidade } })
            })
            .collect();
        crate::edicao::curva::fixtures::conferir("conferencia-padrao.json", serde_json::Value::Array(casos));
    }

    // ------------------------------------------------------------------ apoio dos testes com ffmpeg

    /// Roda o ffmpeg dos testes na pasta e devolve a saída padrão.
    fn ffmpeg(cap: &Capacidades, pasta: &Path, nome: &str, a: Vec<String>) -> Vec<u8> {
        let cmd = exec::Comando { nome: nome.into(), programa: cap.binario.clone(), args: a, pasta: pasta.into(), ambiente: vec![], sem_ambiente: vec![], stderr: pasta.join(format!("{nome}.stderr")) };
        let (fim, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
        exec::exigir_sucesso(&cmd, &fim).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
        out
    }

    fn textos(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    /// Quadros `w`×`h` do arquivo em yuv420p cru (planos em sequência), na ordem.
    fn quadros_yuv(cap: &Capacidades, video: &Path, w: u32, h: u32) -> Vec<Vec<u8>> {
        let pasta = video.parent().unwrap();
        let mut a = textos(&["-hide_banner", "-nostdin", "-loglevel", "error", "-i"]);
        a.push(video.to_string_lossy().into_owned());
        a.extend(textos(&["-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "yuv420p", "-"]));
        let out = ffmpeg(cap, pasta, "yuv", a);
        let tam = (w * h * 3 / 2) as usize;
        assert_eq!(out.len() % tam, 0, "{} ({}): quadros de {w}×{h}", video.display(), cap.versao);
        out.chunks_exact(tam).map(<[u8]>::to_vec).collect()
    }

    /// Aplica a cadeia `filtros` a quadros yuv420p `w`×`h` gravados em `pasta` e devolve os da saída (`ws`×`hs`).
    fn filtrar(cap: &Capacidades, pasta: &Path, nome: &str, quadros: &[Vec<u8>], (w, h): (u32, u32), filtros: &str, (ws, hs): (u32, u32)) -> Vec<Vec<u8>> {
        let entrada = format!("{nome}.yuv");
        std::fs::write(pasta.join(&entrada), quadros.concat()).unwrap();
        let mut a = textos(&["-hide_banner", "-nostdin", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "yuv420p", "-s"]);
        a.extend([format!("{w}x{h}"), "-r".into(), "30".into(), "-i".into(), entrada, "-vf".into(), filtros.to_string()]);
        a.extend(textos(&["-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "yuv420p", "-"]));
        let out = ffmpeg(cap, pasta, nome, a);
        let tam = (ws * hs * 3 / 2) as usize;
        assert_eq!(out.len(), tam * quadros.len(), "{nome} ({})", cap.versao);
        out.chunks_exact(tam).map(<[u8]>::to_vec).collect()
    }

    /// Vídeo sem perdas (H.264 qp 0, 30 qps, base 1/15360) a partir de quadros yuv420p `w`×`h`.
    fn video_sem_perdas(cap: &Capacidades, pasta: &Path, nome: &str, quadros: &[Vec<u8>], (w, h): (u32, u32)) -> PathBuf {
        std::fs::write(pasta.join(format!("{nome}.yuv")), quadros.concat()).unwrap();
        let mut a = textos(&["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "yuv420p", "-s"]);
        a.extend([format!("{w}x{h}"), "-r".into(), "30".into(), "-i".into(), format!("{nome}.yuv")]);
        a.extend(textos(&["-c:v", "libx264", "-qp", "0", "-preset", "ultrafast", "-g", "30", "-bf", "0", "-pix_fmt", "yuv420p", "-video_track_timescale", "15360"]));
        a.push(format!("{nome}.mp4"));
        ffmpeg(cap, pasta, "sem-perdas", a);
        pasta.join(format!("{nome}.mp4"))
    }

    /// Recorte `[x, y, w, h]` e a variante da (única) ocultação no grafo do bloco: o que o atacante lê no retrato.
    fn recorte_e_variante(grafo: &str) -> ([u32; 4], String) {
        let i = grafo.find(":exact=1,").expect("recorte da ocultação no grafo");
        let ini = grafo[..i].rfind("crop=").expect("crop") + "crop=".len();
        let n: Vec<u32> = grafo[ini..i].split(':').map(|x| x.parse().expect("número do crop")).collect();
        let resto = &grafo[i + ":exact=1,".len()..];
        let v = resto[..resto.find('[').expect("rótulo")].to_string();
        assert!(["lutyuv=", "boxblur=", "scale="].iter().any(|p| v.starts_with(p)), "variante inesperada: {v}");
        ([n[2], n[3], n[0], n[1]], v)
    }

    // ------------------------------------------------------------------ pixel a pixel (§11, P2.6)

    mod pixel {
        use super::*;
        use crate::edicao::modelo::{Caixa, ProjetoEdicao};
        use crate::edicao::motor::plano::Plano;
        use crate::edicao::motor::FonteInfo;
        use crate::edicao::testes::aceite::{alvo_final, ancora, midia, projeto, rastreio, trecho, Mundo};
        use crate::edicao::testes::executor::{self, Opcoes};
        use crate::edicao::motor::efeitos::realce::testes::fonte_de;

        pub(super) const W: u32 = 320;
        pub(super) const H: u32 = 180;
        /// Caixa do alvo (0..1), fora da grade de pixel de propósito.
        pub(super) const CAIXA: [f64; 4] = [0.3031, 0.2517, 0.2969, 0.4011];
        pub(super) const MARGEM: f64 = 0.1;
        /// Vértices do polígono (0..1 da caixa com a margem).
        pub(super) const VERTICES: [(f64, f64); 4] = [(0.0, 0.2), (0.7, 0.0), (1.0, 0.9), (0.3, 1.0)];
        const QUADROS: usize = 6;

        /// Fonte com textura em todo pixel (desfoque e mosaico mudam tudo o que tocam): `testsrc2` sem perdas.
        pub(super) fn textura(cap: &Capacidades) -> FonteInfo {
            let pasta = crate::edicao::testes::fontes::pasta_temporaria("p26-textura");
            let mut a = textos(&["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "lavfi", "-i"]);
            a.push(format!("testsrc2=s={W}x{H}:r=30,trim=end_frame={QUADROS}"));
            a.extend(textos(&["-c:v", "libx264", "-qp", "0", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-video_track_timescale", "15360", "textura.mp4"]));
            ffmpeg(cap, &pasta, "textura", a);
            fonte_de(cap, &pasta.join("textura.mp4"))
        }

        pub(super) fn alvo(forma: &str, borda: f64, inverter: bool) -> serde_json::Value {
            let v: Vec<serde_json::Value> = VERTICES.iter().map(|(x, y)| json!({ "x": x, "y": y })).collect();
            json!({ "rastreio": "r1", "forma": forma, "vertices": if forma == "poligono" { json!(v) } else { serde_json::Value::Null },
                    "margem": MARGEM, "borda_suave": borda, "inverter": inverter })
        }

        /// Trecho inteiro da textura no canvas do tamanho dela, com `ocultacoes` da mídia e `efeitos` do clipe; o
        /// rastreio `r1` parado na `CAIXA`.
        pub(super) fn plano(cap: &Arc<Capacidades>, f: &FonteInfo, ocultacoes: serde_json::Value, efeitos: serde_json::Value) -> (ProjetoEdicao, Plano) {
            let r = rastreio("r1", Some("m1"), "fonte", vec![ancora(f.quadros.tempo(0), Some(f.quadros.pts[0]), CAIXA)]);
            let p = projeto(W, H, (30, 1), vec![midia("m1", "video", ocultacoes)], vec![trecho("c1", "m1", &f.quadros, 0, QUADROS, 1.0, efeitos)], json!({ "rastreios": [r] }));
            let plano = Mundo::novo(cap).com("m1", f).compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{e}"));
            (p, plano)
        }

        /// Quadros yuv420p do render sem perdas.
        pub(super) fn render(cap: &Arc<Capacidades>, plano: &Plano, nome: &str) -> Vec<Vec<u8>> {
            let pasta = crate::edicao::testes::fontes::pasta_temporaria(nome);
            let r = executor::rodar(cap, plano, &pasta, &Opcoes { sem_perdas: true, ..Opcoes::default() }).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
            let q: Vec<Vec<u8>> = r.blocos.iter().flat_map(|b| quadros_yuv(cap, b, W, H)).collect();
            assert_eq!(q.len() as u64, plano.total_q, "{nome}");
            q
        }

        /// Região da forma no espaço da fonte (px), medida aqui, sem o rasterizador: dentro de um ponto.
        pub(super) struct Forma {
            forma: String,
            /// Caixa com a margem, px.
            b: [f64; 4],
        }

        impl Forma {
            pub fn nova(forma: &str) -> Self {
                let c = crate::edicao::geometria::com_margem(Caixa { x: CAIXA[0], y: CAIXA[1], w: CAIXA[2], h: CAIXA[3] }, MARGEM);
                Forma { forma: forma.into(), b: [c.x * W as f64, c.y * H as f64, c.w * W as f64, c.h * H as f64] }
            }

            pub fn lado_menor(&self) -> f64 {
                self.b[2].min(self.b[3])
            }

            fn dentro(&self, x: f64, y: f64) -> bool {
                let [bx, by, bw, bh] = self.b;
                match self.forma.as_str() {
                    "retangulo" => x > bx && x < bx + bw && y > by && y < by + bh,
                    "elipse" => {
                        let (u, v) = ((x - bx - bw / 2.0) / (bw / 2.0), (y - by - bh / 2.0) / (bh / 2.0));
                        u * u + v * v < 1.0
                    }
                    _ => {
                        let p: Vec<(f64, f64)> = VERTICES.iter().map(|(u, v)| (bx + u * bw, by + v * bh)).collect();
                        let mut dentro = false;
                        for i in 0..p.len() {
                            let (a, b) = (p[i], p[(i + 1) % p.len()]);
                            if (a.1 > y) != (b.1 > y) && x < a.0 + (y - a.1) * (b.0 - a.0) / (b.1 - a.1) {
                                dentro = !dentro;
                            }
                        }
                        dentro
                    }
                }
            }

            /// Quantos dos 8×8 pontos do pixel caem dentro (0 a 64).
            pub fn dentro_do_pixel(&self, x: u32, y: u32) -> u32 {
                (0..8).flat_map(|i| (0..8).map(move |j| (i, j))).filter(|(i, j)| self.dentro(x as f64 + (*i as f64 + 0.5) / 8.0, y as f64 + (*j as f64 + 0.5) / 8.0)).count() as u32
            }
        }

        /// Pixels (máscara W×H) a até `r` px de algum pixel de `perto`.
        pub(super) fn dilatar(perto: &[bool], r: f64) -> Vec<bool> {
            let ri = r.ceil() as i64;
            let mut v = perto.to_vec();
            for y in 0..H as i64 {
                for x in 0..W as i64 {
                    let i = (y * W as i64 + x) as usize;
                    // só a borda do conjunto espalha
                    let borda = perto[i] && [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| {
                        let (a, b) = (x + dx, y + dy);
                        a < 0 || b < 0 || a >= W as i64 || b >= H as i64 || !perto[(b * W as i64 + a) as usize]
                    });
                    if !borda {
                        continue;
                    }
                    for dy in -ri..=ri {
                        for dx in -ri..=ri {
                            let (a, b) = (x + dx, y + dy);
                            if a >= 0 && b >= 0 && a < W as i64 && b < H as i64 && ((dx * dx + dy * dy) as f64) <= r * r {
                                v[(b * W as i64 + a) as usize] = true;
                            }
                        }
                    }
                }
            }
            v
        }

        /// Blocos 2×2 de posição par que contêm algum pixel de `m`, inteiros.
        pub(super) fn blocos(m: &[bool]) -> Vec<bool> {
            let mut v = vec![false; m.len()];
            for y in (0..H).step_by(2) {
                for x in (0..W).step_by(2) {
                    let px = [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)].map(|(a, b)| (b * W + a) as usize);
                    if px.iter().any(|&i| m[i]) {
                        px.iter().for_each(|&i| v[i] = true);
                    }
                }
            }
            v
        }

        /// Pixels (luma e o croma do bloco) iguais nos dois quadros yuv420p W×H, só onde `onde`; devolve os diferentes.
        pub(super) fn diferentes(a: &[u8], b: &[u8], onde: &[bool]) -> Vec<(u32, u32)> {
            let (w, h) = (W as usize, H as usize);
            let mut d = Vec::new();
            for y in 0..h {
                for x in 0..w {
                    if !onde[y * w + x] {
                        continue;
                    }
                    let c = (y / 2) * (w / 2) + x / 2;
                    let (u, v) = (w * h + c, w * h + w * h / 4 + c);
                    if a[y * w + x] != b[y * w + x] || a[u] != b[u] || a[v] != b[v] {
                        d.push((x as u32, y as u32));
                    }
                }
            }
            d
        }

        /// O quadro `q` (W×H) com o recorte `rec` trocado por `var` (quadro do tamanho do recorte).
        pub(super) fn com_recorte(q: &[u8], rec: [u32; 4], var: &[u8]) -> Vec<u8> {
            let (w, h) = (W as usize, H as usize);
            let [rx, ry, rw, rh] = rec.map(|v| v as usize);
            let mut o = q.to_vec();
            for y in 0..rh {
                o[(ry + y) * w + rx..(ry + y) * w + rx + rw].copy_from_slice(&var[y * rw..y * rw + rw]);
            }
            let (cw, crw, crh) = (w / 2, rw / 2, rh / 2);
            // planos U e V
            for (base_o, base_v) in [(w * h, rw * rh), (w * h + w * h / 4, rw * rh + crw * crh)] {
                for y in 0..crh {
                    let (de, para) = (base_v + y * crw, base_o + (ry / 2 + y) * cw + rx / 2);
                    o[para..para + crw].copy_from_slice(&var[de..de + crw]);
                }
            }
            o
        }
    }

    /// §11 (P2.6): sólido, mosaico e desfoque em retângulo, elipse e polígono, com e sem borda suave e invertidos,
    /// medidos em pixel contra o render sem a ocultação e contra a variante aplicada ao recorte inteiro (luma e croma):
    /// todo bloco 2×2 que toca a forma (no invertido, que toca o lado de fora) sai 100 % trocado; o que fica a mais de
    /// borda + 3 px da forma (no invertido, dentro dela) sai idêntico. Nos dois binários (`SICRO_FFMPEG`).
    #[test]
    fn mosaico_solido_elipse_poligono_borda_inverter_em_pixel() {
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = crate::edicao::testes::fontes::cap() else { return };
        let f = pixel::textura(&cap);
        let (_, sem) = pixel::plano(&cap, &f, json!([]), json!([]));
        let original = pixel::render(&cap, &sem, "p26-sem");
        let pasta = crate::edicao::testes::fontes::pasta_temporaria("p26-variante");
        let mut casos = 0;
        for forma in ["retangulo", "elipse", "poligono"] {
            let fm = pixel::Forma::nova(forma);
            let toca: Vec<bool> = (0..pixel::H).flat_map(|y| (0..pixel::W).map(move |x| (x, y))).map(|(x, y)| fm.dentro_do_pixel(x, y) > 0).collect();
            let fora: Vec<bool> = (0..pixel::H).flat_map(|y| (0..pixel::W).map(move |x| (x, y))).map(|(x, y)| fm.dentro_do_pixel(x, y) < 64).collect();
            for modo in ["solido", "mosaico", "desfoque"] {
                for (borda, inverter) in [(0.0, false), (0.25, false), (0.25, true)] {
                    let rotulo = format!("{modo} {forma} borda {borda} inverter {inverter} ({})", cap.versao);
                    let oc = ocultar_json("o1", "r1", modo, json!({ "intensidade": 0.5, "cor": "#204060ff", "alvo": pixel::alvo(forma, borda, inverter) }));
                    let (_, pl) = pixel::plano(&cap, &f, json!([oc]), json!([]));
                    let saida = pixel::render(&cap, &pl, "p26-oculta");
                    let (rec, variante) = recorte_e_variante(&pl.blocos[0].grafo);
                    let filtro = format!("crop={}:{}:{}:{}:exact=1,{variante}", rec[2], rec[3], rec[0], rec[1]);
                    let var = filtrar(&cap, &pasta, "var", &original, (pixel::W, pixel::H), &filtro, (rec[2], rec[3]));
                    // o lado coberto (blocos inteiros) e o que fica longe da borda
                    let (coberto, longe) = if inverter {
                        (pixel::blocos(&fora), pixel::dilatar(&fora, borda * fm.lado_menor() + 3.0).iter().map(|v| !v).collect::<Vec<bool>>())
                    } else {
                        (pixel::blocos(&toca), pixel::dilatar(&toca, borda * fm.lado_menor() + 3.0).iter().map(|v| !v).collect())
                    };
                    let n_coberto = coberto.iter().filter(|v| **v).count();
                    let n_longe = longe.iter().filter(|v| **v).count();
                    assert!(n_coberto > 1000 && n_longe > 200, "{rotulo}: {n_coberto} cobertos, {n_longe} longe");
                    for (k, q) in saida.iter().enumerate() {
                        let trocado = pixel::com_recorte(&original[k], rec, &var[k]);
                        let d = pixel::diferentes(q, &trocado, &coberto);
                        assert!(d.is_empty(), "{rotulo}: quadro {k}, {} px cobertos que não são a variante, ex. {:?}", d.len(), &d[..d.len().min(5)]);
                        let d = pixel::diferentes(q, &original[k], &longe);
                        assert!(d.is_empty(), "{rotulo}: quadro {k}, {} px longe da forma alterados, ex. {:?}", d.len(), &d[..d.len().min(5)]);
                        // a variante muda o que cobre (senão o teste não veria nada)
                        assert!(pixel::diferentes(&trocado, &original[k], &coberto).len() > n_coberto / 4, "{rotulo}: variante igual ao original");
                        // sólido: a cor exata do efeito (BT.709 limitada), medida sem o grafo
                        if modo == "solido" {
                            let (y, u, v) = cor::yuv_bt709(cor::ler("#204060ff").unwrap());
                            let cheio: Vec<u8> = [vec![y; (pixel::W * pixel::H) as usize], vec![u; (pixel::W * pixel::H / 4) as usize], vec![v; (pixel::W * pixel::H / 4) as usize]].concat();
                            let d = pixel::diferentes(q, &cheio, &coberto);
                            assert!(d.is_empty(), "{rotulo}: quadro {k}, {} px cobertos fora da cor {:?}", d.len(), (y, u, v));
                        }
                    }
                    casos += 1;
                }
            }
        }
        eprintln!("ocultação em pixel ({}): {casos} casos × 6 quadros", cap.versao);
    }

    /// §11 (P2.6): holofote: fora da região (além de borda + 3 px) escurecido e dessaturado exatamente como o
    /// `lutyuv` do grafo e como "Escurecer"/"Dessaturar" pedem (conta feita aqui); dentro (todo bloco que toca a
    /// forma) intacto. Invertido, ao contrário. Só "Dessaturar": a luma fica, o croma vai ao neutro.
    #[test]
    fn holofote_fora_escurecido_dentro_intacto() {
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = crate::edicao::testes::fontes::cap() else { return };
        let f = pixel::textura(&cap);
        let (_, sem) = pixel::plano(&cap, &f, json!([]), json!([]));
        let original = pixel::render(&cap, &sem, "p26-hol-sem");
        let pasta = crate::edicao::testes::fontes::pasta_temporaria("p26-holofote");
        for forma in ["retangulo", "elipse", "poligono"] {
            let fm = pixel::Forma::nova(forma);
            let toca: Vec<bool> = (0..pixel::H).flat_map(|y| (0..pixel::W).map(move |x| (x, y))).map(|(x, y)| fm.dentro_do_pixel(x, y) > 0).collect();
            for (inverter, escurecer, dessaturar) in [(false, 0.6, 0.5), (true, 0.6, 0.5), (false, 0.0, 1.0)] {
                let rotulo = format!("holofote {forma} inverter {inverter} {escurecer}/{dessaturar} ({})", cap.versao);
                let borda = 0.25;
                let h = efeito("holofote", "h1", json!({ "alvo": pixel::alvo(forma, borda, inverter), "escurecer": escurecer, "dessaturar": dessaturar }));
                let (_, pl) = pixel::plano(&cap, &f, json!([]), json!([h]));
                let saida = pixel::render(&cap, &pl, "p26-holofote");
                let (rec, lut) = recorte_e_variante(&pl.blocos[0].grafo);
                assert!(lut.starts_with("lutyuv="), "{rotulo}: {lut}");
                let filtro = format!("crop={}:{}:{}:{}:exact=1,{lut}", rec[2], rec[3], rec[0], rec[1]);
                let escuro = filtrar(&cap, &pasta, "lut", &original, (pixel::W, pixel::H), &filtro, (rec[2], rec[3]));
                let r = borda * fm.lado_menor() + 3.0;
                // blocos que tocam a forma × além da borda; invertido troca os dois (o holofote é o complemento exato)
                let (perto, longe) = (pixel::blocos(&toca), pixel::dilatar(&toca, r).iter().map(|v| !v).collect::<Vec<bool>>());
                let (intacto, escurecido) = if inverter { (longe, perto) } else { (perto, longe) };
                for (k, q) in saida.iter().enumerate() {
                    let esperado = pixel::com_recorte(&original[k], rec, &escuro[k]);
                    let d = pixel::diferentes(q, &original[k], &intacto);
                    assert!(d.is_empty(), "{rotulo}: quadro {k}, {} px que deviam ficar intactos, ex. {:?}", d.len(), &d[..d.len().min(5)]);
                    let d = pixel::diferentes(q, &esperado, &escurecido);
                    assert!(d.is_empty(), "{rotulo}: quadro {k}, {} px que deviam sair escurecidos, ex. {:?}", d.len(), &d[..d.len().min(5)]);
                    assert!(pixel::diferentes(&esperado, &original[k], &escurecido).len() > 2000, "{rotulo}: o escurecido não muda a imagem");
                    // e o escurecido é o que "Escurecer" e "Dessaturar" pedem, medido sem o grafo: luma para o preto,
                    // croma para o neutro (escurecer multiplica o RGB, então encolhe o croma junto)
                    let (w, hh) = (pixel::W as usize, pixel::H as usize);
                    let (ky, kc) = (1.0 - escurecer, (1.0 - dessaturar) * (1.0 - escurecer));
                    let o = &original[k];
                    let mut n = 0;
                    for y in 0..hh {
                        for x in 0..w {
                            if !escurecido[y * w + x] {
                                continue;
                            }
                            let c = (y / 2) * (w / 2) + x / 2;
                            let luma = (16.0 + (o[y * w + x] as f64 - 16.0) * ky).round().clamp(16.0, 235.0) as u8;
                            let croma = |i: usize| (128.0 + (o[i] as f64 - 128.0) * kc).round().clamp(16.0, 240.0) as u8;
                            let (iu, iv) = (w * hh + c, w * hh + w * hh / 4 + c);
                            assert_eq!((q[y * w + x], q[iu], q[iv]), (luma, croma(iu), croma(iv)), "{rotulo}: quadro {k} em ({x}, {y})");
                            n += 1;
                        }
                    }
                    assert!(n > 1000);
                }
                // só "Dessaturar": croma neutro, luma igual, onde escurece
                if escurecer == 0.0 {
                    let (w, hh) = (pixel::W as usize, pixel::H as usize);
                    let q = &saida[0];
                    let mut n = 0;
                    for y in (0..hh).step_by(2) {
                        for x in (0..w).step_by(2) {
                            if [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)].iter().all(|&(a, b)| escurecido[b * w + a]) {
                                let c = (y / 2) * (w / 2) + x / 2;
                                assert_eq!((q[w * hh + c], q[w * hh + w * hh / 4 + c]), (128, 128), "{rotulo}: croma em ({x}, {y})");
                                assert_eq!(q[y * w + x], original[0][y * w + x], "{rotulo}: luma em ({x}, {y})");
                                n += 1;
                            }
                        }
                    }
                    assert!(n > 1000);
                }
            }
        }
    }

    // ------------------------------------------------------------------ a ocultação da mídia onde ela reaparece

    /// Pixels do quadrado vermelho da matriz no quadro RGB.
    fn vermelhos(q: &crate::edicao::testes::medir::QuadroRgb) -> usize {
        q.px.chunks_exact(3).filter(|p| crate::edicao::motor::efeitos::realce::testes::vermelho(p)).count()
    }

    /// Quadros RGB de todos os blocos do render sem perdas.
    fn render_rgb(cap: &Arc<Capacidades>, plano: &crate::edicao::motor::plano::Plano, nome: &str) -> Vec<crate::edicao::testes::medir::QuadroRgb> {
        crate::edicao::motor::efeitos::realce::testes::renderizar(cap, plano, nome, &[]).1
    }

    /// §11 (P2.6): a ocultação da MÍDIA aparece na repetição (o mesmo trecho de novo, a 0,5×) e no quadro congelado,
    /// sem nenhum pixel do alvo de sobra; sem ela o alvo está em todo slot. A "célula" (tela dividida) é da Fase 3.
    #[test]
    fn repeticao_e_congelado() {
        use crate::edicao::quadros::apoio;
        use crate::edicao::testes::aceite::{alvo_final, ancoras_da_marca, midia, projeto, rastreio, trecho, Mundo};
        use crate::edicao::testes::fontes::{self, Fonte};
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let tab = &m.info.quadros;
        let congelado = json!({ "tipo": "congelar", "id": "c3", "rotulo": "", "midia": "m1", "instante_s": tab.tempo(60), "instante_pts": tab.pts[60], "duracao_s": 0.5, "efeitos": [] });
        let montar = |ocultacoes: serde_json::Value| {
            let p = projeto(
                320,
                180,
                (30, 1),
                vec![midia("m1", "video", ocultacoes)],
                vec![trecho("c1", "m1", tab, 40, 100, 1.0, json!([])), trecho("c2", "m1", tab, 50, 90, 0.5, json!([])), congelado.clone()],
                json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(Fonte::Mp4, &m, 40, 100))] }),
            );
            Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{e}"))
        };
        let com = montar(json!([ocultar_json("o1", "r1", "solido", json!({ "ativo": { "de": tab.tempo(40), "ate": tab.tempo(99) } }))]));
        let quadros = render_rgb(&cap, &com, "p26-repeticao");
        // 60 + 80 (0,5×) + 15 slots
        assert_eq!(quadros.len(), 155, "{}", cap.versao);
        for (q, quadro) in quadros.iter().enumerate() {
            assert_eq!(vermelhos(quadro), 0, "slot {q} ({}) com pixel do alvo de sobra", cap.versao);
        }
        let sem = render_rgb(&cap, &montar(json!([])), "p26-repeticao-sem");
        assert!(sem.iter().all(|q| vermelhos(q) > 200), "o medidor não vê o alvo em algum slot sem a ocultação");
        eprintln!("repetição e congelado ({}): {} slots sem sobra", cap.versao, quadros.len());
    }

    /// §11 (P2.6): estabilizar + ocultar e girar + ocultar com 0 pixel de sobra. A ocultação marcada no ORIGINAL
    /// (caixa do alvo medida em cada quadro cru) entra antes da estabilização e do giro; sem ela o alvo aparece.
    #[test]
    fn estabilizar_e_girar_sem_sobra() {
        use crate::edicao::motor::efeitos::realce::testes::{caixa_da_cor, estabilizar, magenta, renderizar, tremido};
        use crate::edicao::quadros::apoio;
        use crate::edicao::testes::aceite::{alvo_final, ancora, ancoras_da_marca, efeito, midia, projeto, rastreio, trecho, Mundo};
        use crate::edicao::testes::fontes::{self, Fonte};
        use crate::edicao::testes::medir;
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        // girar 90° com espelho: canvas 16:9, a imagem em pé no meio
        let m = apoio::medida(&cap, Fonte::Mp4);
        let tab = &m.info.quadros;
        let girar = efeito("girar", "g1", json!({ "graus": 90, "espelhar_h": true, "espelhar_v": false }));
        let montar = |ocultacoes: serde_json::Value| {
            let p = projeto(
                320,
                180,
                (30, 1),
                vec![midia("m1", "video", ocultacoes)],
                vec![trecho("c1", "m1", tab, 40, 100, 1.0, json!([girar.clone()]))],
                json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras_da_marca(Fonte::Mp4, &m, 40, 100))] }),
            );
            Mundo::novo(&cap).com("m1", &m.info).compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{e}"))
        };
        let quadros = render_rgb(&cap, &montar(json!([ocultar_json("o1", "r1", "solido", json!({}))])), "p26-girar");
        for (q, quadro) in quadros.iter().enumerate() {
            assert_eq!(vermelhos(quadro), 0, "girar: slot {q} ({}) com pixel do alvo de sobra", cap.versao);
        }
        let sem = render_rgb(&cap, &montar(json!([])), "p26-girar-sem");
        assert!(sem.iter().all(|q| vermelhos(q) > 100), "girar: o medidor não vê o alvo sem a ocultação");
        // estabilizar: a câmera treme até 11 px; o alvo magenta é marcado em cada quadro cru
        let f = tremido(&cap, &fontes::pasta_temporaria("p26-tremido"));
        let tq = &f.quadros;
        let (j_in, j_out) = (20usize, 90usize);
        let cru = medir::quadros(&cap, &f.caminho).unwrap();
        let ancoras: Vec<serde_json::Value> = (j_in..j_out)
            .map(|j| {
                let (c, _) = caixa_da_cor(&cru[j], magenta).expect("alvo no quadro cru");
                let (w, h) = (cru[j].w as f64, cru[j].h as f64);
                ancora(tq.tempo(j), Some(tq.pts[j]), [c[0] as f64 / w, c[1] as f64 / h, (c[2] - c[0]) as f64 / w, (c[3] - c[1]) as f64 / h])
            })
            .collect();
        let efeitos = json!([estabilizar(tq, 10, 109, "preta", 0.0, None)]);
        let magentas = |q: &medir::QuadroRgb| q.px.chunks_exact(3).filter(|p| magenta(p)).count();
        for (nome, oc) in [("p26-estab", json!([ocultar_json("o1", "r1", "solido", json!({}))])), ("p26-estab-sem", json!([]))] {
            let p = projeto(
                crate::edicao::motor::efeitos::realce::testes::TREMIDO.0,
                crate::edicao::motor::efeitos::realce::testes::TREMIDO.1,
                (30, 1),
                vec![midia("m1", "video", oc.clone())],
                vec![trecho("c1", "m1", tq, j_in, j_out, 1.0, efeitos.clone())],
                json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras.clone())] }),
            );
            let plano = Mundo::novo(&cap).com("m1", &f).compilar(&p, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{e}"));
            assert!(plano.blocos[0].grafo.contains("vidstabtransform"), "{nome}: sem estabilização no grafo");
            let (_, quadros) = renderizar(&cap, &plano, nome, &[]);
            assert_eq!(quadros.len(), j_out - j_in);
            if oc.as_array().is_some_and(|a| !a.is_empty()) {
                for (q, quadro) in quadros.iter().enumerate() {
                    assert_eq!(magentas(quadro), 0, "estabilizar: slot {q} ({}) com pixel do alvo de sobra", cap.versao);
                }
            } else {
                assert!(quadros.iter().all(|q| magentas(q) > 500), "estabilizar: o medidor não vê o alvo sem a ocultação");
            }
        }
        eprintln!("estabilizar e girar com ocultar ({}): sem sobra", cap.versao);
    }

    // ------------------------------------------------------------------ rastreio denso, gravado à mão (fecho da Fase 2)

    pub(crate) mod denso {
        use super::*;
        use crate::edicao::modelo::ProjetoEdicao;
        use crate::edicao::motor::plano::Plano;
        use crate::edicao::motor::{compilar_video, Alvo, Contexto, FonteInfo, LeitorRastreios};
        use crate::edicao::quadros::apoio::Medida;
        use crate::edicao::rastreio::{ArquivoDenso, ArquivoSilhuetas};
        use crate::edicao::testes::fontes::{self, Fonte};
        use std::collections::HashMap;

        /// Leitor falso: os densos gravados à mão, pelo SHA-256 do ponteiro (sem o rastreador nem o disco).
        pub struct Duble(pub HashMap<String, Arc<ArquivoDenso>>);

        impl LeitorRastreios for Duble {
            fn denso(&self, rastreio: &str, sha256: &str) -> crate::error::Result<Arc<ArquivoDenso>> {
                self.0.get(sha256).cloned().ok_or_else(|| crate::error::SicroError::Validation(format!("rastreio_ausente: {rastreio}")))
            }
            fn silhuetas(&self, _: &str, _: &str) -> crate::error::Result<Arc<ArquivoSilhuetas>> {
                Err(crate::error::SicroError::Validation("nao_suportado: silhuetas".into()))
            }
        }

        /// Denso da marca vermelha da matriz nos quadros `[a, b)`: caixa exata, `estado` e `tipo` por quadro.
        pub fn da_marca(m: &Medida, (a, b): (usize, usize), estado: impl Fn(usize) -> u8, tipo: impl Fn(usize) -> u8) -> ArquivoDenso {
            let tab = &m.info.quadros;
            let caixas: Vec<[f64; 4]> = (a..b).map(|j| fontes::marca_esperada(Fonte::Mp4, m.numeros[j])).collect();
            let e: Vec<u8> = (a..b).map(&estado).collect();
            ArquivoDenso {
                esquema: 2,
                rastreio: "r1".into(),
                midia_sha256: "0".repeat(64),
                espaco: "original".into(),
                pixel: crate::edicao::modelo::TamanhoPx { w: m.info.exibido.0, h: m.info.exibido.1 },
                pts: tab.pts[a..b].to_vec(),
                x: caixas.iter().map(|c| c[0]).collect(),
                y: caixas.iter().map(|c| c[1]).collect(),
                w: caixas.iter().map(|c| c[2]).collect(),
                h: caixas.iter().map(|c| c[3]).collect(),
                conf: e.iter().map(|&s| [0.9, 0.4, 0.1][s as usize]).collect(),
                estado: e,
                tipo: (a..b).map(tipo).collect(),
                corrida: vec![0; b - a],
                corridas: vec![],
            }
        }

        /// Rastreio assistido da marca com o denso `sha`, a âncora do perito no quadro `a` e as `ausencias`.
        pub fn rastreio(m: &Medida, d: &ArquivoDenso, sha: &str, a: usize, ausencias: serde_json::Value) -> serde_json::Value {
            let tab = &m.info.quadros;
            let conta = |s: u8| d.estado.iter().filter(|e| **e == s).count();
            let c = fontes::marca_esperada(Fonte::Mp4, m.numeros[a]);
            json!({ "id": "r1", "nome": "r1", "cor": "#ff0000", "midia": "m1", "base": "fonte", "tipo": "caixa", "espaco": "original",
                    "ancoras": [{ "t": tab.tempo(a), "pts": tab.pts[a], "v": { "x": c[0], "y": c[1], "w": c[2], "h": c[3] }, "interp": "linear" }],
                    "denso": { "sha256": sha, "n": d.pts.len(), "de_s": tab.tempo(a), "ate_s": tab.tempo(a + d.pts.len() - 1), "ok": conta(0), "incertos": conta(1), "ausentes": conta(2) },
                    "silhuetas": null, "ausencias": ausencias, "origem": { "tipo": "assistido" }, "confirmacao": null })
        }

        pub fn compilar(cap: &Capacidades, f: &FonteInfo, p: &ProjetoEdicao, leitor: &Duble, alvo: &Alvo) -> crate::error::Result<Plano> {
            let fontes: HashMap<String, FonteInfo> = [("m1".to_string(), f.clone())].into();
            let ctx = Contexto { fontes: &fontes, cap, rastreios: leitor, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
            compilar_video(p, &crate::edicao::tempo::resolver(p, &ctx), &ctx, alvo)
        }

        pub fn analisar(cap: &Capacidades, f: &FonteInfo, p: &ProjetoEdicao, leitor: &Duble) -> Vec<(String, Option<u64>)> {
            let fontes: HashMap<String, FonteInfo> = [("m1".to_string(), f.clone())].into();
            let ctx = Contexto { fontes: &fontes, cap, rastreios: leitor, pasta_projeto: Path::new("/nao/existe"), midias: &[] };
            crate::edicao::validar::analisar(p, &ctx, None).problemas.iter().map(|x| (x.codigo.to_string(), x.n)).collect()
        }
    }

    /// Ocultar seguindo um rastreio DENSO (o caso que o dono usa primeiro), com o denso gravado à mão: quadro ok e
    /// incerto cobertos (o incerto acende `ocultar_incerto` com a contagem), ausente dentro da ausência aceita sem
    /// ocultação (e sem erro), ausente fora dela = `sem_cobertura` (a validação acusa e o motor recusa).
    #[test]
    #[ignore = "fecho da Fase 2: P2.2"]
    fn ocultar_segue_denso_ok_incerto_e_ausencia() {
        use crate::edicao::quadros::apoio;
        use crate::edicao::testes::aceite::{alvo_final, midia, projeto, trecho};
        use crate::edicao::testes::fontes::{self, Fonte};
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let tab = &m.info.quadros;
        let (a, b) = (40usize, 100usize);
        // ok 40..59 e 80..99, incerto 60..69, ausente 70..79
        let estado = |j: usize| match j {
            60..=69 => crate::edicao::rastreio::ESTADO_INCERTO,
            70..=79 => crate::edicao::rastreio::ESTADO_AUSENTE,
            _ => crate::edicao::rastreio::ESTADO_OK,
        };
        let d = denso::da_marca(&m, (a, b), estado, |_| crate::edicao::rastreio::TIPO_MEDIDO);
        let sha = "d".repeat(64);
        let leitor = denso::Duble([(sha.clone(), Arc::new(d.clone()))].into());
        let aceita = json!([{ "de_s": tab.tempo(70), "ate_s": tab.tempo(79), "motivo": "oculto", "aceita_em": "2026-11-15T10:00:00Z", "por": "Perito" }]);
        let montar = |ausencias: serde_json::Value| {
            let oc = ocultar_json("o1", "r1", "solido", json!({ "ativo": { "de": tab.tempo(a), "ate": tab.tempo(b - 1) } }));
            projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([oc]))], vec![trecho("c1", "m1", tab, a, b, 1.0, json!([]))], json!({ "rastreios": [denso::rastreio(&m, &d, &sha, a, ausencias)] }))
        };
        let p = montar(aceita);
        let problemas = denso::analisar(&cap, &m.info, &p, &leitor);
        assert!(problemas.contains(&("ocultar_incerto".to_string(), Some(10))), "{problemas:?}");
        assert!(!problemas.iter().any(|x| x.0 == "sem_cobertura"), "{problemas:?}");
        let plano = denso::compilar(&cap, &m.info, &p, &leitor, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{e}"));
        let q = &plano.proveniencia.ocultacoes[0].quadros;
        assert_eq!((q.ancora, q.medido, q.interpolado, q.incerto), (1, 49, 0, 10));
        let quadros = render_rgb(&cap, &plano, "p26-denso");
        assert_eq!(quadros.len(), b - a);
        for (k, quadro) in quadros.iter().enumerate() {
            let j = a + k;
            let v = vermelhos(quadro);
            if (70..80).contains(&j) {
                assert!(v > 200, "quadro {j} ({}): ausência aceita, o alvo aparece ({v} px)", cap.versao);
            } else {
                assert_eq!(v, 0, "quadro {j} ({}): pixel do alvo de sobra", cap.versao);
            }
        }
        // ausente fora de ausência aceita: erro antes do render
        let p = montar(json!([]));
        let problemas = denso::analisar(&cap, &m.info, &p, &leitor);
        assert!(problemas.iter().any(|x| x.0 == "sem_cobertura"), "{problemas:?}");
        let e = denso::compilar(&cap, &m.info, &p, &leitor, &alvo_final("cpu")).unwrap_err().to_string();
        assert!(e.contains("sem_cobertura"), "{e}");
        eprintln!("ocultar seguindo denso ({}): {} slots", cap.versao, quadros.len());
    }

    /// O9 (P2.6): destaque seguindo rastreio DENSO marca a até 2 px em 3 slots (moldura × alvo, pelo centro das caixas).
    #[test]
    #[ignore = "fecho da Fase 2: P2.2"]
    fn destaque_segue_denso_a_2_px_em_3_slots() {
        use crate::edicao::motor::efeitos::realce::testes::{caixa_da_cor, verde, vermelho};
        use crate::edicao::quadros::apoio;
        use crate::edicao::testes::aceite::{alvo_final, midia, projeto, trecho};
        use crate::edicao::testes::fontes::{self, Fonte};
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let m = apoio::medida(&cap, Fonte::Mp4);
        let tab = &m.info.quadros;
        let (a, b) = (40usize, 100usize);
        let d = denso::da_marca(&m, (a, b), |_| crate::edicao::rastreio::ESTADO_OK, |_| crate::edicao::rastreio::TIPO_MEDIDO);
        let sha = "e".repeat(64);
        let leitor = denso::Duble([(sha.clone(), Arc::new(d.clone()))].into());
        let destaque = efeito("destaque", "d1", json!({ "forma": "retangulo", "rastreio": "r1", "traco": { "cor": "#00ff00", "espessura": 0.02, "contorno": false },
            "tamanho": 0.1, "angulo_graus": 0.0, "margem": 0.4, "rotulo": null }));
        let p = projeto(320, 180, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, a, b, 1.0, json!([destaque]))], json!({ "rastreios": [denso::rastreio(&m, &d, &sha, a, json!([]))] }));
        let plano = denso::compilar(&cap, &m.info, &p, &leitor, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{e}"));
        let quadros = render_rgb(&cap, &plano, "p26-destaque-denso");
        let mut pior = 0.0f64;
        // o primeiro é a âncora do perito: os três seguintes só têm o denso
        for k in [7usize, 31, 55] {
            // centro da caixa envolvente (a média pesaria mais o lado do traço sobre fundo claro)
            let meio = |c: [u32; 4]| ((c[0] + c[2]) as f64 / 2.0, (c[1] + c[3]) as f64 / 2.0);
            let (mx, my) = meio(caixa_da_cor(&quadros[k], vermelho).expect("alvo").0);
            let (vx, vy) = meio(caixa_da_cor(&quadros[k], verde).unwrap_or_else(|| panic!("slot {k} ({}): sem a moldura", cap.versao)).0);
            let desvio = (vx - mx).abs().max((vy - my).abs());
            assert!(desvio <= 2.0, "slot {k} ({}): moldura em ({vx:.1}, {vy:.1}), alvo em ({mx:.1}, {my:.1})", cap.versao);
            pior = pior.max(desvio);
        }
        eprintln!("destaque seguindo denso ({}): pior desvio {pior:.2} px em 3 slots", cap.versao);
    }

    // ------------------------------------------------------------------ ataque (critica-pericia/e7b_limiar.py)

    /// O ataque do E7b (`critica-pericia/e7b_limiar.py`) contra o MOTOR: placas de 7 caracteres numa fonte conhecida,
    /// ocultadas pelo compilador (rastreio, máscara, recorte, variante), codificadas pela predefinição Júri (H.264
    /// crf 18) e decodificadas. O atacante conhece a fonte, a caixa e o grafo do retrato; como desfoque e mosaico são
    /// quase lineares, mede a contribuição de cada caractere passando pelo MESMO filtro (ffmpeg) e escolhe, posição por
    /// posição, o que deixa o menor resíduo com ganho e nível livres (3 partidas × 4 voltas, como o script).
    mod ataque {
        use super::*;
        use crate::edicao::camada::texto::{bitmap_sprite, PedidoSprite};
        use crate::edicao::testes::aceite::{alvo_final, ancora, midia, ocultar as ocultar_json, projeto, rastreio, trecho, Mundo};
        use crate::edicao::testes::executor::{self, Opcoes};
        use crate::edicao::motor::efeitos::realce::testes::fonte_de;

        pub(super) const ALFA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        const W: u32 = 640;
        const H: u32 = 360;
        /// Cores da cena (luma): lataria, placa e tinta (as do script).
        const LATARIA: f64 = 60.0;
        const FUNDO_PLACA: f64 = 200.0;
        const TINTA: f64 = 170.0;

        /// Glifo de cada caractere de `ALFA` (tinta 0..1), todos `cw`×`ch` (mono negrito do rasterizador do SICRO).
        struct Glifos {
            tinta: Vec<Vec<f64>>,
            cw: usize,
            ch: usize,
        }

        fn glifos(fs: f64) -> Glifos {
            let (mut tinta, mut cw, mut ch) = (Vec::new(), 0, 0);
            for &c in ALFA {
                let estilo = json!({ "fonte": "mono", "negrito": true, "italico": false, "tamanho": fs / H as f64, "cor": "#000000", "alinhamento": "esq", "caixa": null, "contorno": null });
                let bloco = json!({ "id": "b", "papel": "corpo", "conteudo": (c as char).to_string(), "estilo": estilo, "lugar": { "x": 0.0, "y": 0.0, "h": "esq", "v": "topo" }, "largura_max": 1.0 });
                let p = PedidoSprite { blocos: vec![serde_json::from_value(bloco).unwrap()], largura_max: 1.0, altura_canvas: H, fundo: false };
                let (b, _) = bitmap_sprite(&p).expect("glifo");
                if cw == 0 {
                    (cw, ch) = (b.w as usize, b.h as usize);
                }
                assert_eq!((b.w as usize, b.h as usize), (cw, ch), "fonte mono: todo glifo do mesmo tamanho");
                tinta.push(b.px.chunks_exact(4).map(|p| p[3] as f64 / 255.0).collect());
            }
            Glifos { tinta, cw, ch }
        }

        /// Geometria da placa (a do script: margem fs/6, caixa com 15 % de folga) e os glifos.
        pub(super) struct Placa {
            g: Glifos,
            mg: usize,
            pw: usize,
            ph: usize,
            /// Caixa do rastreio, px: x, y, w, h (pares).
            pub caixa: [u32; 4],
            /// Canto da placa na cena.
            canto: (usize, usize),
        }

        pub(super) fn placa(fs: f64) -> Placa {
            let g = glifos(fs);
            let mg = ((fs as usize) / 6).max(2);
            let (pw, ph) = (7 * g.cw + 2 * mg, g.ch + 2 * mg);
            let par = |v: f64| (v as u32) / 2 * 2;
            let (w, h) = (par(pw as f64 * 1.15) + 2, par(ph as f64 * 1.15) + 2);
            let caixa = [240, 140, w, h];
            let canto = (240 + (w as usize - pw) / 2, 140 + (h as usize - ph) / 2);
            Placa { g, mg, pw, ph, caixa, canto }
        }

        impl Placa {
            /// Tinta (0..1) da placa com `txt` (índices em `ALFA`).
            fn tinta(&self, txt: &[usize]) -> Vec<f64> {
                let mut t = vec![0.0; self.pw * self.ph];
                for (i, &c) in txt.iter().enumerate() {
                    for r in 0..self.g.ch {
                        for k in 0..self.g.cw {
                            t[(self.mg + r) * self.pw + self.mg + i * self.g.cw + k] = self.g.tinta[c][r * self.g.cw + k];
                        }
                    }
                }
                t
            }

            /// Quadro yuv420p `w`×`h` com lataria e a placa (tinta `tinta`) com o canto em `(x, y)`.
            fn cena(&self, tinta: &[f64], (w, h): (u32, u32), (x, y): (usize, usize)) -> Vec<u8> {
                let (w, h) = (w as usize, h as usize);
                let mut q = vec![LATARIA as u8; w * h];
                for r in 0..self.ph {
                    for k in 0..self.pw {
                        q[(y + r) * w + x + k] = (FUNDO_PLACA - TINTA * tinta[r * self.pw + k]).round().clamp(0.0, 255.0) as u8;
                    }
                }
                q.resize(w * h * 3 / 2, 128);
                q
            }
        }

        /// Gerador fixo (xorshift64*): as mesmas placas a cada rodada.
        pub(super) struct Sorteio(u64);
        impl Sorteio {
            pub fn novo(semente: u64) -> Self {
                Self(semente.max(1))
            }
            pub fn ate(&mut self, n: usize) -> usize {
                self.0 ^= self.0 >> 12;
                self.0 ^= self.0 << 25;
                self.0 ^= self.0 >> 27;
                (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 33) as usize % n
            }
        }

        /// Resultado de uma rodada: caracteres certos na posição certa, de quantos, e dois exemplos.
        pub(super) struct Rodada {
            pub certos: usize,
            pub total: usize,
            pub exemplos: Vec<String>,
            pub variante: String,
        }

        /// `n` placas ocultadas pelo motor com `modo`/`intensidade`/`conteudo` e atacadas.
        pub(super) fn rodada(cap: &Arc<Capacidades>, p: &Placa, modo: &str, intensidade: f64, conteudo: &str, n: usize, semente: u64) -> Rodada {
            let pasta = crate::edicao::testes::fontes::pasta_temporaria("p26-ataque");
            let mut sorte = Sorteio::novo(semente);
            let placas: Vec<Vec<usize>> = (0..n).map(|_| (0..7).map(|_| sorte.ate(ALFA.len())).collect()).collect();
            let cenas: Vec<Vec<u8>> = placas.iter().map(|t| p.cena(&p.tinta(t), (W, H), p.canto)).collect();
            let video = video_sem_perdas(cap, &pasta, "placas", &cenas, (W, H));
            let f = fonte_de(cap, &video);
            assert_eq!(f.quadros.pts.len(), n, "quadros da fonte ({})", cap.versao);
            // a caixa do perito em volta da placa (margem 0: a caixa já tem a folga do script)
            let [x, y, w, h] = p.caixa;
            let c01 = [x as f64 / W as f64, y as f64 / H as f64, w as f64 / W as f64, h as f64 / H as f64];
            let alvo = json!({ "rastreio": "r1", "forma": "retangulo", "vertices": null, "margem": 0.0, "borda_suave": 0.0, "inverter": false });
            let oc = ocultar_json("o1", "r1", modo, json!({ "intensidade": intensidade, "conteudo": conteudo, "alvo": alvo }));
            let pr = projeto(
                W,
                H,
                (30, 1),
                vec![midia("m1", "video", json!([oc]))],
                vec![trecho("c1", "m1", &f.quadros, 0, n, 1.0, json!([]))],
                json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", vec![ancora(f.quadros.tempo(0), Some(f.quadros.pts[0]), c01)])] }),
            );
            let plano = Mundo::novo(cap).com("m1", &f).compilar(&pr, &alvo_final("cpu")).unwrap_or_else(|e| panic!("{e}"));
            // a predefinição Júri (H.264 crf 18), como sai para o júri
            let r = executor::rodar(cap, &plano, &pasta.join("render"), &Opcoes::default()).unwrap_or_else(|e| panic!("render ({}): {e}", cap.versao));
            let saida = quadros_yuv(cap, &r.blocos[0], W, H);
            assert_eq!(saida.len(), n);
            let (rec, variante) = recorte_e_variante(&plano.blocos[0].grafo);
            // o atacante passa cada caractere pelo mesmo filtro, no recorte do grafo
            let (rw, rh) = (rec[2], rec[3]);
            let canto = (p.canto.0 - rec[0] as usize, p.canto.1 - rec[1] as usize);
            let vazio = vec![0.0; p.pw * p.ph];
            let mut entradas = vec![p.cena(&vazio, (rw, rh), canto)];
            for i in 0..7 {
                for c in 0..ALFA.len() {
                    let mut so = vec![0.0; p.pw * p.ph];
                    for r in 0..p.g.ch {
                        for k in 0..p.g.cw {
                            so[(p.mg + r) * p.pw + p.mg + i * p.g.cw + k] = p.g.tinta[c][r * p.g.cw + k];
                        }
                    }
                    entradas.push(p.cena(&so, (rw, rh), canto));
                }
            }
            let op = filtrar(cap, &pasta, "op", &entradas, (rw, rh), &variante, (rw, rh));
            // só a caixa (máscara cheia): fora dela o recorte mostra a lataria intacta
            let (bx, by) = ((x - rec[0]) as usize, (y - rec[1]) as usize);
            let na_caixa = |q: &[u8], larg: usize| -> Vec<f64> { (0..h as usize).flat_map(|r| (0..w as usize).map(move |k| (r, k))).map(|(r, k)| q[(by + r) * larg + bx + k] as f64).collect() };
            let base = na_caixa(&op[0], rw as usize);
            let deltas: Vec<Vec<f64>> = op[1..].iter().map(|q| na_caixa(q, rw as usize).iter().zip(&base).map(|(a, b)| a - b).collect()).collect();
            let mut certos = 0;
            let mut exemplos = Vec::new();
            for (k, txt) in placas.iter().enumerate() {
                let obs: Vec<f64> = (0..h as usize).flat_map(|r| (0..w as usize).map(move |c| (r, c))).map(|(r, c)| saida[k][(y as usize + r) * W as usize + x as usize + c] as f64).collect();
                let achado = atacar(&obs, &base, &deltas, &mut sorte);
                certos += txt.iter().zip(&achado).filter(|(a, b)| a == b).count();
                if k < 2 {
                    let s = |v: &[usize]| v.iter().map(|&c| ALFA[c] as char).collect::<String>();
                    exemplos.push(format!("{}->{}", s(txt), s(&achado)));
                }
            }
            Rodada { certos, total: 7 * n, exemplos, variante }
        }

        /// Casamento de candidatos: soma linear das contribuições, ganho e nível livres, descida por coordenada.
        fn atacar(obs: &[f64], base: &[f64], deltas: &[Vec<f64>], sorte: &mut Sorteio) -> Vec<usize> {
            let n = obs.len() as f64;
            let nc = ALFA.len();
            let d = |i: usize, c: usize| &deltas[i * nc + c];
            let (sy, syy) = (obs.iter().sum::<f64>(), obs.iter().map(|v| v * v).sum::<f64>());
            let var_y = syy - sy * sy / n;
            // por (posição, caractere): Σd, Σd², Σd·obs
            let pre: Vec<(f64, f64, f64)> = deltas.iter().map(|v| (v.iter().sum(), v.iter().map(|a| a * a).sum(), v.iter().zip(obs).map(|(a, b)| a * b).sum())).collect();
            // resíduo de obs ≈ g·sintético + b (mínimos quadrados); sintético constante = sem informação
            let residuo = |sx: f64, sxx: f64, sxy: f64| {
                let var_x = sxx - sx * sx / n;
                if var_x <= 1e-9 * n {
                    return var_y;
                }
                let cov = sxy - sx * sy / n;
                var_y - cov * cov / var_x
            };
            let (mut melhor, mut menor) = (vec![0; 7], f64::INFINITY);
            for _ in 0..3 {
                let mut s: Vec<usize> = (0..7).map(|_| sorte.ate(nc)).collect();
                for _ in 0..4 {
                    for i in 0..7 {
                        // resto fixo: a base e os outros seis caracteres
                        let mut r = base.to_vec();
                        for (j, &c) in s.iter().enumerate().filter(|(j, _)| *j != i) {
                            for (a, b) in r.iter_mut().zip(d(j, c)) {
                                *a += b;
                            }
                        }
                        let (sr, srr, sro) = (r.iter().sum::<f64>(), r.iter().map(|a| a * a).sum::<f64>(), r.iter().zip(obs).map(|(a, b)| a * b).sum::<f64>());
                        let mut escolha = (f64::INFINITY, 0usize);
                        // empate: o menor caractere (a ordem do script)
                        let mut ordem: Vec<usize> = (0..nc).collect();
                        ordem.sort_by_key(|&c| ALFA[c]);
                        for c in ordem {
                            let (dd_s, dd_ss, dd_so) = pre[i * nc + c];
                            let rd: f64 = r.iter().zip(d(i, c)).map(|(a, b)| a * b).sum();
                            let e = residuo(sr + dd_s, srr + 2.0 * rd + dd_ss, sro + dd_so);
                            if e < escolha.0 {
                                escolha = (e, c);
                            }
                        }
                        s[i] = escolha.1;
                    }
                }
                let mut r = base.to_vec();
                for (j, &c) in s.iter().enumerate() {
                    for (a, b) in r.iter_mut().zip(d(j, c)) {
                        *a += b;
                    }
                }
                let e = residuo(r.iter().sum(), r.iter().map(|a| a * a).sum(), r.iter().zip(obs).map(|(a, b)| a * b).sum());
                if e < menor {
                    (melhor, menor) = (s, e);
                }
            }
            melhor
        }
    }

    /// Recuperação aceita no padrão de cada conteúdo de texto (§11, P2.6): menos de 10 % (acaso: 1/36 ≈ 2,8 %).
    const RECUPERA_MAX: f64 = 0.10;

    /// §11 (P2.6): o ataque do E7b recupera menos de 10 % no padrão de cada conteúdo de texto, nos dois binários
    /// (`SICRO_FFMPEG`). Controle: com o padrão de rosto (mosaico 0,75) a mesma placa sai legível, e o ataque acha.
    #[test]
    fn ataque_menos_de_10_por_cento() {
        let Some(cap) = crate::edicao::testes::fontes::cap() else { return };
        let p = ataque::placa(28.0);
        for c in [ConteudoOculto::Placa, ConteudoOculto::Documento, ConteudoOculto::Tela] {
            let (modo, i) = padrao(c);
            let nome = serde_json::to_value(c).unwrap().as_str().unwrap().to_string();
            let modo_s = serde_json::to_value(modo).unwrap().as_str().unwrap().to_string();
            let r = ataque::rodada(&cap, &p, &modo_s, i, &nome, 8, 20261113);
            let f = r.certos as f64 / r.total as f64;
            eprintln!("ataque ({}): {nome} no padrão ({modo_s} {i}, {}): {}/{} ({:.0} %) {}", cap.versao, r.variante, r.certos, r.total, 100.0 * f, r.exemplos.join(" "));
            assert!(f < RECUPERA_MAX, "{nome} no padrão: o ataque recuperou {}/{} ({})", r.certos, r.total, cap.versao);
        }
        let (modo, i) = padrao(ConteudoOculto::Rosto);
        let r = ataque::rodada(&cap, &p, serde_json::to_value(modo).unwrap().as_str().unwrap(), i, "placa", 8, 20261113);
        eprintln!("controle ({}): placa com o padrão de rosto ({}): {}/{} {}", cap.versao, r.variante, r.certos, r.total, r.exemplos.join(" "));
        assert!(r.certos as f64 / r.total as f64 > 0.5, "controle: o ataque não lê a placa em mosaico 0,75 ({}/{})", r.certos, r.total);
    }

    /// Medição dos limiares de `fraca` (§12 item 4): o ataque do E7b pelo motor (boxblur, mosaico de 1 a 12 células),
    /// 16 placas por ponto, em dois tamanhos. Imprime a tabela e confere que os limiares de hoje são o primeiro ponto
    /// abaixo de 10 % na letra de 14 px. `cargo test --lib -- --ignored --nocapture limiares_pelo_ataque`.
    #[test]
    #[ignore = "medição (minutos): os limiares de `fraca` saem daqui"]
    fn limiares_pelo_ataque() {
        let Some(cap) = crate::edicao::testes::fontes::cap() else { return };
        let mut medidas = Vec::new();
        for fs in [28.0, 14.0] {
            let p = ataque::placa(fs);
            for (modo, lista) in [("desfoque", &[0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 1.0][..]), ("mosaico", &[0.0, 0.4, 0.55, 0.6, 0.7, 0.75, 0.8, 0.9, 1.0][..])] {
                for &i in lista {
                    let r = ataque::rodada(&cap, &p, modo, i, "outro", 16, 20261115);
                    medidas.push((fs, modo, i, r.certos as f64 / r.total as f64));
                    eprintln!(
                        "medição ({}): placa {}×{} px, {modo} {i:.2} [{}]: {}/{} ({:.0} %) {}",
                        cap.versao,
                        p.caixa[2],
                        p.caixa[3],
                        r.variante,
                        r.certos,
                        r.total,
                        100.0 * r.certos as f64 / r.total as f64,
                        r.exemplos.join(" ")
                    );
                }
            }
        }
        let em = |modo: &str, i: f64| medidas.iter().find(|m| m.0 == 14.0 && m.1 == modo && m.2 == i).unwrap().3;
        let o = |modo: &str, i: f64| ocultacao(modo, i);
        assert!(em("desfoque", DESFOQUE_MIN) < RECUPERA_MAX && !fraca(&o("desfoque", DESFOQUE_MIN)));
        assert!(em("desfoque", 0.8) >= RECUPERA_MAX && fraca(&o("desfoque", 0.8)));
        assert!(em("mosaico", 1.0) < RECUPERA_MAX && !fraca(&o("mosaico", 1.0)));
        assert!(em("mosaico", 0.9) >= RECUPERA_MAX && fraca(&o("mosaico", 0.9)));
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
