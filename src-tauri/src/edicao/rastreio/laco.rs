//! Laço do SICRO (porte de `p3a/referencia/laco_sicro.py`, §6.2): decide por quadro se aceita, avisa, procura ou
//! para. Reprocura (VitTrack): ok ≥ 0,45, incerto 0,30..0,45, perdido < 0,30 ou salto > 1 diagonal ou tamanho
//! variando mais de 1,6×; reprocura em duas passadas; para depois de 2 s perdido. Presença (EdgeTAM): ok ≥ 0,95,
//! incerto 0,5..0,95, ausente < 0,5; para depois de 3 s sem o alvo. Simples (mancha): como a presença, com os
//! limiares do motor. Dono: P2.2.
//!
//! Diferenças da referência, de propósito: a cauda de perdidos sai cortada também no cancelamento; teto de quadros
//! (`Motivo::Limite`); quadro que não é o da âncora, fora de ordem ou de outro tamanho = `leitura_divergente`.
//! Buraco reencontrado fica sem caixa (estado ausente, B24).

use std::sync::atomic::{AtomicBool, Ordering};

use super::{
    silhueta, AmostraCorrida, Avanco, Busca, CaixaPx, Corrida, Imagem, Laco, Limiares, LeitorQuadros, Motivo, Parou, PedidoCorrida, Rastreador,
    ResultadoCorrida, Sentido, ESTADO_AUSENTE, ESTADO_INCERTO, ESTADO_OK,
};
use crate::edicao::executar::{ERRO_CANCELADO, ERRO_LEITURA_DIVERGENTE};
use crate::edicao::modelo::TamanhoPx;
use crate::error::{Result, SicroError};

/// Limiares do laço por reprocura (`laco_sicro.py`), ponto de partida medido em 38 sequências (§12).
pub const T_PERDA: f64 = 0.30;
pub const T_INCERTO: f64 = 0.45;
pub const REENCONTRO_REL: f64 = 0.85;
pub const T_REENCONTRO_MIN: f64 = 0.35;
pub const PACIENCIA_S: f64 = 2.0;
pub const SALTO_MAX: f64 = 1.0;
pub const RAZAO_MAX: f64 = 1.6;
pub const RAZAO_REENCONTRO: f64 = 2.0;
pub const CRESCE_BUSCA: f64 = 0.10;
pub const BUSCA_MAX: f64 = 3.0;
/// Laço por presença.
pub const P_PRESENTE: f64 = 0.5;
pub const P_INCERTO: f64 = 0.95;
pub const PACIENCIA_MEMORIA_S: f64 = 3.0;
/// Confiança típica antes do primeiro quadro aceito (referência).
const TIPICA_INICIAL: f64 = 0.6;

// CONTRATO: acréscimo. Os limiares dos motores na taxa da mídia (o P2.3 usa nos `limiares(fps)`).
/// Segundos → quadros na taxa `fps` (pelo menos 1).
pub fn paciencia(segundos: f64, fps: f64) -> u32 {
    let q = (segundos * fps).round();
    if q.is_finite() && q >= 1.0 {
        q.min(u32::MAX as f64) as u32
    } else {
        1
    }
}

/// Limiares do laço por reprocura na taxa `fps` (VitTrack).
pub fn limiares_reprocura(fps: f64) -> Limiares {
    Limiares { ok: T_INCERTO, perdido: T_PERDA, paciencia: paciencia(PACIENCIA_S, fps) }
}

/// Limiares do laço por presença na taxa `fps` (EdgeTAM).
pub fn limiares_presenca(fps: f64) -> Limiares {
    Limiares { ok: P_INCERTO, perdido: P_PRESENTE, paciencia: paciencia(PACIENCIA_MEMORIA_S, fps) }
}

fn divergente(d: impl std::fmt::Display) -> SicroError {
    SicroError::Validation(format!("{ERRO_LEITURA_DIVERGENTE}: {d}"))
}

fn plausivel(nova: &CaixaPx, velha: &CaixaPx, razao_max: f64, salto_max: f64) -> bool {
    let (rw, rh) = (nova.w / velha.w, nova.h / velha.h);
    if !(1.0 / razao_max <= rw && rw <= razao_max && 1.0 / razao_max <= rh && rh <= razao_max) {
        return false;
    }
    let dx = (nova.x + nova.w / 2.0) - (velha.x + velha.w / 2.0);
    let dy = (nova.y + nova.h / 2.0) - (velha.y + velha.h / 2.0);
    dx.hypot(dy) <= salto_max * velha.w.hypot(velha.h)
}

fn conferir_quadro(q: &Imagem, pixel: TamanhoPx) -> Result<()> {
    if q.w != pixel.w || q.h != pixel.h || q.rgb.len() != q.w as usize * q.h as usize * 3 {
        return Err(divergente(format!("quadro {}x{} para {}x{}", q.w, q.h, pixel.w, pixel.h)));
    }
    Ok(())
}

/// RLE da silhueta que o motor deu, no tamanho do quadro lido (máscara torta = erro, não pânico no `rle`).
fn rle_da_silhueta(m: Option<&super::Mascara>, pixel: TamanhoPx) -> Result<Option<Vec<u8>>> {
    let Some(m) = m else { return Ok(None) };
    if m.w != pixel.w || m.h != pixel.h || m.bits.len() != m.w as usize * m.h as usize {
        return Err(SicroError::Validation(format!("{}: silhueta {}x{} para {}x{}", crate::edicao::mensagens::codigo::ITEM_INVALIDO, m.w, m.h, pixel.w, pixel.h)));
    }
    Ok(Some(silhueta::rle(m)))
}

/// O que um quadro deu: caixa aceita (ou não), nota e silhueta.
struct Decisao {
    caixa: Option<CaixaPx>,
    conf: f64,
    silhueta: Option<super::Mascara>,
    aceita: bool,
}

/// A corrida inteira pelo laço que o motor pede (`Rastreador::laco`).
pub fn correr(
    p: &PedidoCorrida,
    leitor: &mut dyn LeitorQuadros,
    r: &mut dyn Rastreador,
    cancelar: &AtomicBool,
    avanco: &mut dyn FnMut(Avanco),
) -> Result<ResultadoCorrida> {
    let laco = r.laco();
    let lim = r.limiares(p.fps);
    let (pts0, silhueta0) = {
        let Some((pts0, q0)) = leitor.proximo()? else {
            if cancelar.load(Ordering::SeqCst) {
                return Err(SicroError::Validation(format!("{ERRO_CANCELADO}: rastreio")));
            }
            return Err(divergente("corrida sem o quadro da âncora"));
        };
        conferir_quadro(&q0, p.pixel)?;
        if p.ancora.pts.is_some_and(|a| a != pts0) {
            return Err(divergente(format!("primeiro quadro {pts0}, âncora {:?}", p.ancora.pts)));
        }
        let passo0 = r.iniciar(&q0, CaixaPx::de_caixa(&p.ancora.v, p.pixel))?;
        let s0 = rle_da_silhueta(passo0.silhueta.as_ref(), p.pixel)?;
        avanco(Avanco { pts: pts0, caixa: Some(p.ancora.v), conf: 1.0, estado: ESTADO_OK, feitos: 0, quadro: &q0 });
        (pts0, s0)
    };
    let mut am = vec![AmostraCorrida { pts: pts0, caixa: Some(p.ancora.v), conf: 1.0, estado: ESTADO_OK, silhueta: silhueta0 }];

    let mut caixa = CaixaPx::de_caixa(&p.ancora.v, p.pixel);
    let mut tipica: Option<f64> = None;
    let mut perdidos = 0u32;
    let mut primeiro: Option<i64> = None;
    let mut ultimo = pts0;
    let mut feitos = 0u32;
    let mut parada: Option<Motivo> = None;
    loop {
        if cancelar.load(Ordering::SeqCst) {
            parada = Some(Motivo::Cancelado);
            break;
        }
        if feitos >= p.max_quadros {
            parada = Some(Motivo::Limite);
            break;
        }
        let Some((pts, q)) = leitor.proximo()? else {
            // o leitor para de esperar quando a corrida é cancelada
            if cancelar.load(Ordering::SeqCst) {
                parada = Some(Motivo::Cancelado);
            }
            break;
        };
        conferir_quadro(&q, p.pixel)?;
        let em_ordem = match p.sentido {
            Sentido::Frente => pts > ultimo,
            Sentido::Tras => pts < ultimo,
        };
        if !em_ordem {
            return Err(divergente(format!("quadro {pts} depois de {ultimo}")));
        }
        ultimo = pts;
        feitos += 1;

        let d = match laco {
            Laco::Reprocura if perdidos == 0 => {
                let ps = r.seguir(&q, &Busca { caixa, janela: true })?;
                let aceita = ps.caixa.is_some_and(|n| ps.conf >= lim.perdido && plausivel(&n, &caixa, RAZAO_MAX, SALTO_MAX));
                Decisao { caixa: ps.caixa, conf: ps.conf, silhueta: ps.silhueta, aceita }
            }
            Laco::Reprocura => {
                // 1) janela larga em volta da última aceita, sem a janela de Hann: só a posição
                let k = (1.0 + CRESCE_BUSCA * perdidos as f64).min(BUSCA_MAX);
                let (cx, cy) = (caixa.x + caixa.w / 2.0, caixa.y + caixa.h / 2.0);
                let larga = CaixaPx { x: cx - caixa.w * k / 2.0, y: cy - caixa.h * k / 2.0, w: caixa.w * k, h: caixa.h * k };
                let c1 = r.seguir(&q, &Busca { caixa: larga, janela: false })?;
                match c1.caixa {
                    None => Decisao { caixa: None, conf: c1.conf, silhueta: None, aceita: false },
                    Some(cand) => {
                        // 2) busca normal no candidato, com o tamanho da última aceita: confiança comparável
                        let (ccx, ccy) = (cand.x + cand.w / 2.0, cand.y + cand.h / 2.0);
                        let busca = CaixaPx { x: ccx - caixa.w / 2.0, y: ccy - caixa.h / 2.0, w: caixa.w, h: caixa.h };
                        let ps = r.seguir(&q, &Busca { caixa: busca, janela: true })?;
                        let limiar = T_REENCONTRO_MIN.max(REENCONTRO_REL * tipica.unwrap_or(TIPICA_INICIAL));
                        let aceita = ps.caixa.is_some_and(|n| ps.conf >= limiar && plausivel(&n, &caixa, RAZAO_REENCONTRO, 1e9));
                        Decisao { caixa: ps.caixa, conf: ps.conf, silhueta: ps.silhueta, aceita }
                    }
                }
            }
            Laco::Presenca | Laco::Simples => {
                let ps = r.seguir(&q, &Busca { caixa, janela: true })?;
                let aceita = ps.caixa.is_some() && ps.conf >= lim.perdido;
                Decisao { caixa: ps.caixa, conf: ps.conf, silhueta: ps.silhueta, aceita }
            }
        };

        let nova = d.caixa.filter(|_| d.aceita);
        let amostra = match nova {
            Some(n) => {
                if laco == Laco::Reprocura && perdidos == 0 {
                    tipica = Some(match tipica {
                        None => d.conf,
                        Some(t) => 0.9 * t + 0.1 * d.conf,
                    });
                }
                caixa = n;
                perdidos = 0;
                primeiro = None;
                let estado = if d.conf >= lim.ok { ESTADO_OK } else { ESTADO_INCERTO };
                AmostraCorrida { pts, caixa: Some(n.caixa(p.pixel)), conf: d.conf, estado, silhueta: rle_da_silhueta(d.silhueta.as_ref(), p.pixel)? }
            }
            None => {
                perdidos += 1;
                primeiro.get_or_insert(pts);
                AmostraCorrida { pts, caixa: None, conf: d.conf, estado: ESTADO_AUSENTE, silhueta: None }
            }
        };
        avanco(Avanco { pts, caixa: amostra.caixa, conf: amostra.conf, estado: amostra.estado, feitos, quadro: &q });
        am.push(amostra);
        if perdidos >= lim.paciencia {
            break;
        }
    }

    // o trecho válido acaba no último aceito
    while am.last().is_some_and(|a| a.caixa.is_none()) {
        am.pop();
    }
    let parou = match parada {
        Some(m) => Parou { motivo: m, pts: Some(ultimo) },
        None if perdidos > 0 => Parou { motivo: Motivo::Perdido, pts: primeiro },
        None => Parou { motivo: Motivo::Fim, pts: Some(ultimo) },
    };
    let (de_pts, ate_pts) = (pts0.min(ultimo), pts0.max(ultimo));
    let corrida = Corrida {
        motor: r.corrida(),
        sentido: p.sentido,
        ancora_pts: pts0,
        de_pts,
        ate_pts,
        limiares: lim,
        parou: parou.motivo,
        feito_em: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    };
    Ok(ResultadoCorrida { amostras: am, parou, corrida })
}

/// Quadros-chave (Douglas-Peucker em x, y, w, h): a caixa interpolada fica a menos de `tolerancia` × lado médio
/// (mínimo `piso_px`) de toda amostra; buraco vira fim e início de trecho. Devolve índices em `amostras`.
/// `amostras` = quadros seguidos (uma corrida, ou um trecho contínuo do denso); a fração entre duas chaves é pelo pts.
/// Como na referência, o passo entre as duas primeiras amostras com caixa é o passo "contínuo" do corte em trechos.
pub fn quadros_chave(amostras: &[AmostraCorrida], pixel: TamanhoPx, tolerancia: f64, piso_px: f64) -> Vec<usize> {
    let validas: Vec<usize> = amostras.iter().enumerate().filter(|(_, a)| a.caixa.is_some()).map(|(k, _)| k).collect();
    if validas.len() <= 2 {
        return validas;
    }
    let px = |k: usize| -> [f64; 4] {
        let c = CaixaPx::de_caixa(&amostras[k].caixa.expect("válida"), pixel);
        [c.x, c.y, c.w, c.h]
    };
    let passo0 = validas[1] - validas[0];
    let mut trechos: Vec<&[usize]> = Vec::new();
    let mut ini = 0;
    for j in 1..=validas.len() {
        if j == validas.len() || validas[j] - validas[j - 1] != passo0 {
            trechos.push(&validas[ini..j]);
            ini = j;
        }
    }
    let mut chaves = std::collections::BTreeSet::new();
    // pilha no lugar da recursão: 54 000 quadros não cabem na pilha do sistema
    let mut pilha: Vec<&[usize]> = trechos;
    while let Some(ks) = pilha.pop() {
        let (a, b) = (ks[0], ks[ks.len() - 1]);
        chaves.insert(a);
        chaves.insert(b);
        if ks.len() <= 2 {
            continue;
        }
        let (ca, cb) = (px(a), px(b));
        let (pa, pb) = (amostras[a].pts, amostras[b].pts);
        let (mut pior, mut onde) = (0.0f64, None);
        for (i, &k) in ks.iter().enumerate().take(ks.len() - 1).skip(1) {
            let t = (amostras[k].pts - pa) as f64 / (pb - pa) as f64;
            let c = px(k);
            let it: [f64; 4] = std::array::from_fn(|i| ca[i] + (cb[i] - ca[i]) * t);
            let mut e = (c[0] - it[0]).abs().max((c[1] - it[1]).abs()).max((c[0] + c[2] - it[0] - it[2]).abs()).max((c[1] + c[3] - it[1] - it[3]).abs());
            e /= (0.5 * (c[2] + c[3])).max(piso_px / tolerancia);
            if e > pior {
                (pior, onde) = (e, Some(i));
            }
        }
        if pior > tolerancia {
            if let Some(i) = onde {
                pilha.push(&ks[i..]);
                pilha.push(&ks[..=i]);
            }
        }
    }
    chaves.into_iter().collect()
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;
    use crate::edicao::modelo::{Caixa, Chave, Interp};
    use crate::edicao::rastreio::{ModeloCorrida, MotorCorrida, MotorRastreio, Passo, MAX_QUADROS};
    use serde_json::Value;
    use std::path::Path;

    /// Leitor falso: quadros 1×1 com o índice nos 3 bytes; pts = índice.
    pub(crate) struct LeitorFalso {
        pub indices: Vec<i64>,
        pub i: usize,
        buf: [u8; 3],
    }

    impl LeitorFalso {
        pub fn novo(indices: Vec<i64>) -> Self {
            Self { indices, i: 0, buf: [0; 3] }
        }
    }

    impl LeitorQuadros for LeitorFalso {
        fn proximo(&mut self) -> Result<Option<(i64, Imagem<'_>)>> {
            let Some(&k) = self.indices.get(self.i) else { return Ok(None) };
            self.i += 1;
            self.buf = [(k & 0xff) as u8, ((k >> 8) & 0xff) as u8, ((k >> 16) & 0xff) as u8];
            Ok(Some((k, Imagem { rgb: &self.buf, w: 1, h: 1 })))
        }
    }

    fn indice(q: &Imagem) -> i64 {
        q.rgb[0] as i64 | (q.rgb[1] as i64) << 8 | (q.rgb[2] as i64) << 16
    }

    fn motor(m: MotorRastreio) -> MotorCorrida {
        MotorCorrida { motor: m, modelo: ModeloCorrida { nome: "falso".into(), origem: String::new(), commit: String::new(), arquivos: vec![], desvio: String::new() }, runtime: String::new(), provedor: "cpu".into() }
    }

    /// Chamada gravada da referência: quadro, caixa de busca, janela, caixa devolvida, confiança.
    struct Chamada {
        quadro: i64,
        busca: [f64; 4],
        janela: bool,
        caixa: [f64; 4],
        conf: f64,
    }

    /// VitTrack falso: devolve as chamadas gravadas na ordem e confere que o laço pediu a MESMA busca (bit a bit).
    struct VitFalso {
        chamadas: Vec<Chamada>,
        i: usize,
    }

    impl Rastreador for VitFalso {
        fn iniciar(&mut self, _: &Imagem, c: CaixaPx) -> Result<Passo> {
            Ok(Passo { caixa: Some(c), conf: 1.0, silhueta: None })
        }
        fn seguir(&mut self, q: &Imagem, b: &Busca) -> Result<Passo> {
            let c = self.chamadas.get(self.i).ok_or_else(|| SicroError::Validation(format!("chamada a mais no quadro {}", indice(q))))?;
            self.i += 1;
            let pedida = [b.caixa.x, b.caixa.y, b.caixa.w, b.caixa.h];
            if c.quadro != indice(q) || c.janela != b.janela || pedida.iter().zip(&c.busca).any(|(a, r)| a.to_bits() != r.to_bits()) {
                return Err(SicroError::Validation(format!("chamada {}: quadro {} busca {pedida:?} janela {}; referência quadro {} busca {:?} janela {}", self.i - 1, indice(q), b.janela, c.quadro, c.busca, c.janela)));
            }
            Ok(Passo { caixa: Some(CaixaPx { x: c.caixa[0], y: c.caixa[1], w: c.caixa[2], h: c.caixa[3] }), conf: c.conf, silhueta: None })
        }
        fn laco(&self) -> Laco {
            Laco::Reprocura
        }
        // os da referência (paciência fixa em 60, sem escalar pelo fps)
        fn limiares(&self, _: f64) -> Limiares {
            Limiares { ok: T_INCERTO, perdido: T_PERDA, paciencia: 60 }
        }
        fn corrida(&self) -> MotorCorrida {
            motor(MotorRastreio::Vittrack)
        }
    }

    /// EdgeTAM falso: presença e caixa de cada quadro (os quadros cortados da cauda vêm sem o alvo).
    struct MemoriaFalsa {
        por_quadro: std::collections::HashMap<i64, (Option<[f64; 4]>, f64)>,
        chamadas: usize,
    }

    impl Rastreador for MemoriaFalsa {
        fn iniciar(&mut self, _: &Imagem, c: CaixaPx) -> Result<Passo> {
            Ok(Passo { caixa: Some(c), conf: 1.0, silhueta: None })
        }
        fn seguir(&mut self, q: &Imagem, _: &Busca) -> Result<Passo> {
            self.chamadas += 1;
            let (c, p) = self.por_quadro.get(&indice(q)).copied().unwrap_or((None, 0.0));
            Ok(Passo { caixa: c.map(|c| CaixaPx { x: c[0], y: c[1], w: c[2], h: c[3] }), conf: p, silhueta: None })
        }
        fn laco(&self) -> Laco {
            Laco::Presenca
        }
        fn limiares(&self, _: f64) -> Limiares {
            Limiares { ok: P_INCERTO, perdido: P_PRESENTE, paciencia: 90 }
        }
        fn corrida(&self) -> MotorCorrida {
            motor(MotorRastreio::Edgetam)
        }
    }

    fn pasta_fixtures() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edicao")
    }

    /// As 38 fixtures `rastreio-laco-<seq>.json` (gravadas por `rastreio-laco-gerar.py` sobre a referência).
    pub(crate) fn fixtures_do_laco() -> Vec<Value> {
        let mut v: Vec<_> = std::fs::read_dir(pasta_fixtures())
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("rastreio-laco-") && n.ends_with(".json")))
            .collect();
        v.sort();
        v.iter().map(|p| serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()).collect()
    }

    fn quatro(v: &Value) -> [f64; 4] {
        std::array::from_fn(|i| v[i].as_f64().unwrap())
    }

    fn pedido(f: &Value) -> PedidoCorrida {
        let c0 = quatro(&f["caixa0"]);
        let ini = f["ini"].as_i64().unwrap();
        PedidoCorrida {
            rastreio: "r1".into(),
            midia_sha256: "0".repeat(64),
            espaco: "original".into(),
            // pixel 1×1: a caixa em px é a própria (0..1 × 1), sem arredondamento na ida e na volta
            pixel: TamanhoPx { w: 1, h: 1 },
            ancora: Chave { t: 0.0, pts: Some(ini), v: Caixa { x: c0[0], y: c0[1], w: c0[2], h: c0[3] }, interp: Interp::Linear },
            sentido: Sentido::Frente,
            ate_pts: None,
            max_quadros: MAX_QUADROS,
            fps: 30.0,
        }
    }

    fn estado_ref(e: &str) -> u8 {
        match e {
            "ok" => ESTADO_OK,
            "incerto" => ESTADO_INCERTO,
            _ => ESTADO_AUSENTE,
        }
    }

    /// Decisão por quadro, motivo e quadro da parada iguais aos da referência. Devolve as amostras.
    fn conferir_com_referencia(f: &Value, r: &mut dyn Rastreador, lado: &str) -> Vec<AmostraCorrida> {
        let seq = f["seq"].as_str().unwrap();
        let ini = f["ini"].as_i64().unwrap();
        let n = f["n"].as_i64().unwrap();
        let p = pedido(f);
        let mut leitor = LeitorFalso::novo((ini..ini + n).collect());
        let mut avancos = 0u32;
        let res = correr(&p, &mut leitor, r, &AtomicBool::new(false), &mut |_| avancos += 1).unwrap_or_else(|e| panic!("{seq} ({lado}): {e}"));
        let traj = f[lado]["traj"].as_array().unwrap();
        assert_eq!(res.amostras.len(), traj.len(), "{seq} ({lado}): amostras");
        for (a, t) in res.amostras.iter().zip(traj) {
            let k = t[0].as_i64().unwrap();
            assert_eq!(a.pts, k, "{seq} ({lado})");
            assert_eq!(a.estado, estado_ref(t[3].as_str().unwrap()), "{seq} ({lado}) quadro {k}: estado");
            assert_eq!(a.conf.to_bits(), t[2].as_f64().unwrap().to_bits(), "{seq} ({lado}) quadro {k}: confiança");
            match (&a.caixa, t[1].is_null()) {
                (None, true) => {}
                (Some(c), false) => assert_eq!([c.x, c.y, c.w, c.h], quatro(&t[1]), "{seq} ({lado}) quadro {k}: caixa"),
                _ => panic!("{seq} ({lado}) quadro {k}: caixa {:?} × {}", a.caixa, t[1]),
            }
        }
        let (motivo, parou) = (f[lado]["motivo"].as_str().unwrap(), f[lado]["parou_em"].as_i64());
        let esperado = if motivo == "fim" { Motivo::Fim } else { Motivo::Perdido };
        assert_eq!(res.parou.motivo, esperado, "{seq} ({lado}): motivo");
        if esperado == Motivo::Perdido {
            assert_eq!(res.parou.pts, parou, "{seq} ({lado}): onde parou");
        }
        assert_eq!(res.corrida.ancora_pts, ini);
        assert!(res.corrida.de_pts == ini && res.corrida.ate_pts >= res.amostras.last().unwrap().pts);
        assert_eq!(avancos as usize, (res.corrida.ate_pts - ini + 1) as usize, "{seq} ({lado}): um avanço por quadro lido");
        res.amostras
    }

    fn chamadas(f: &Value) -> Vec<Chamada> {
        f["vittrack"]["chamadas"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| Chamada { quadro: c[0].as_i64().unwrap(), busca: quatro(&c[1]), janela: c[2].as_bool().unwrap(), caixa: quatro(&c[3]), conf: c[4].as_f64().unwrap() })
            .collect()
    }

    #[test]
    fn reprocura_como_a_referencia() {
        let fs = fixtures_do_laco();
        assert_eq!(fs.len(), 38, "as 38 sequências do p3a");
        let mut com_reprocura = 0;
        for f in &fs {
            assert!(f["vittrack"]["igual_a_res_laco"].as_bool().unwrap(), "{}: a gravação não repetiu p3a/res_laco", f["seq"]);
            let mut r = VitFalso { chamadas: chamadas(f), i: 0 };
            conferir_com_referencia(f, &mut r, "vittrack");
            assert_eq!(r.i, r.chamadas.len(), "{}: chamadas que o laço não fez", f["seq"]);
            com_reprocura += usize::from(r.chamadas.iter().any(|c| !c.janela));
        }
        // a reprocura (janela larga sem Hann) acontece em parte das sequências: o teste passa por ela
        assert!(com_reprocura >= 5, "{com_reprocura}");
    }

    #[test]
    fn presenca_como_a_referencia() {
        for f in &fixtures_do_laco() {
            let mut por_quadro = std::collections::HashMap::new();
            // quadro sem o alvo na referência: o motor falso devolve caixa (a última) com a presença gravada, < 0,5;
            // quem recusa é o limiar de presença do laço
            let mut ultima = quatro(&f["caixa0"]);
            for t in f["edgetam"]["traj"].as_array().unwrap() {
                if !t[1].is_null() {
                    ultima = quatro(&t[1]);
                }
                por_quadro.insert(t[0].as_i64().unwrap(), (Some(ultima), t[2].as_f64().unwrap()));
            }
            let mut r = MemoriaFalsa { por_quadro, chamadas: 0 };
            let am = conferir_com_referencia(f, &mut r, "edgetam");
            // uma chamada por quadro depois do marcado (o EdgeTAM não reprocura)
            assert!(r.chamadas as i64 >= am.last().unwrap().pts - f["ini"].as_i64().unwrap());
        }
    }

    #[test]
    fn quadros_chave_como_a_referencia() {
        let mut total = 0;
        for f in &fixtures_do_laco() {
            for lado in ["vittrack", "edgetam"] {
                let am: Vec<AmostraCorrida> = f[lado]["traj"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| AmostraCorrida {
                        pts: t[0].as_i64().unwrap(),
                        caixa: (!t[1].is_null()).then(|| {
                            let c = quatro(&t[1]);
                            Caixa { x: c[0], y: c[1], w: c[2], h: c[3] }
                        }),
                        conf: t[2].as_f64().unwrap(),
                        estado: estado_ref(t[3].as_str().unwrap()),
                        silhueta: None,
                    })
                    .collect();
                let esperado: Vec<usize> = f[lado]["chaves"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect();
                assert_eq!(quadros_chave(&am, TamanhoPx { w: 1, h: 1 }, 0.04, 2.0), esperado, "{} ({lado})", f["seq"]);
                total += esperado.len();
            }
        }
        assert!(total > 1000, "{total}");
    }

    /// Paciência pela taxa da mídia; teto de quadros; cancelamento; quadro que não é o da âncora; tamanho errado.
    #[test]
    fn limites_e_leitura_conferida() {
        assert_eq!((paciencia(2.0, 30.0), paciencia(2.0, 25.0), paciencia(3.0, 30000.0 / 1001.0), paciencia(2.0, 0.0)), (60, 50, 90, 1));
        assert_eq!(limiares_reprocura(30.0), Limiares { ok: 0.45, perdido: 0.30, paciencia: 60 });
        assert_eq!(limiares_presenca(25.0), Limiares { ok: 0.95, perdido: 0.5, paciencia: 75 });
        let f = &fixtures_do_laco()[0];
        let ini = f["ini"].as_i64().unwrap();
        let p = PedidoCorrida { max_quadros: 10, ..pedido(f) };
        let mut r = VitFalso { chamadas: chamadas(f), i: 0 };
        let res = correr(&p, &mut LeitorFalso::novo((ini..ini + 100).collect()), &mut r, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!((res.parou.motivo, res.corrida.ate_pts, res.amostras.len()), (Motivo::Limite, ini + 10, 11));
        let res = correr(&pedido(f), &mut LeitorFalso::novo((ini..ini + 100).collect()), &mut VitFalso { chamadas: chamadas(f), i: 0 }, &AtomicBool::new(true), &mut |_| {}).unwrap();
        assert_eq!((res.parou.motivo, res.amostras.len()), (Motivo::Cancelado, 1));
        // o leitor começou em outro quadro
        let e = correr(&pedido(f), &mut LeitorFalso::novo((ini + 1..ini + 100).collect()), &mut VitFalso { chamadas: chamadas(f), i: 0 }, &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert!(e.to_string().contains(ERRO_LEITURA_DIVERGENTE), "{e}");
        // fora de ordem
        let mut r = MemoriaFalsa { por_quadro: (0..1000).map(|k| (k, (Some([1.0, 1.0, 2.0, 2.0]), 1.0))).collect(), chamadas: 0 };
        let e = correr(&pedido(f), &mut LeitorFalso::novo(vec![ini, ini + 2, ini + 1]), &mut r, &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert_eq!(r.chamadas, 1);
        assert!(e.to_string().contains(ERRO_LEITURA_DIVERGENTE), "{e}");
        // quadro de outro tamanho
        let p = PedidoCorrida { pixel: TamanhoPx { w: 2, h: 1 }, ..pedido(f) };
        assert!(correr(&p, &mut LeitorFalso::novo(vec![ini]), &mut VitFalso { chamadas: vec![], i: 0 }, &AtomicBool::new(false), &mut |_| {}).is_err());
    }

    /// Motor que dá silhueta: a do quadro marcado e as dos aceitos viram RLE; máscara de outro tamanho é recusada.
    struct ComSilhueta {
        lado: (u32, u32),
        curta: bool,
        /// quadro em que o alvo some
        sem: i64,
    }

    impl ComSilhueta {
        fn m(&self, k: i64) -> crate::edicao::rastreio::Mascara {
            let (w, h) = self.lado;
            let mut bits: Vec<u8> = (0..w * h).map(|i| u8::from((i as i64 + k) % 3 == 0)).collect();
            if self.curta {
                bits.pop();
            }
            crate::edicao::rastreio::Mascara { w, h, bits }
        }
    }

    impl Rastreador for ComSilhueta {
        fn iniciar(&mut self, q: &Imagem, c: CaixaPx) -> Result<Passo> {
            Ok(Passo { caixa: Some(c), conf: 1.0, silhueta: Some(self.m(indice(q))) })
        }
        fn seguir(&mut self, q: &Imagem, b: &Busca) -> Result<Passo> {
            let k = indice(q);
            Ok(Passo { caixa: (k != self.sem).then_some(b.caixa), conf: 0.99, silhueta: Some(self.m(k)) })
        }
        fn laco(&self) -> Laco {
            Laco::Presenca
        }
        fn limiares(&self, _: f64) -> Limiares {
            Limiares { ok: P_INCERTO, perdido: P_PRESENTE, paciencia: 90 }
        }
        fn corrida(&self) -> MotorCorrida {
            motor(MotorRastreio::Edgetam)
        }
    }

    #[test]
    fn silhueta_do_motor_conferida() {
        let p = PedidoCorrida { pixel: TamanhoPx { w: 1, h: 1 }, ..pedido(&fixtures_do_laco()[0]) };
        let ini = p.ancora.pts.unwrap();
        let mut r = ComSilhueta { lado: (1, 1), curta: false, sem: ini + 3 };
        let res = correr(&p, &mut LeitorFalso::novo((ini..ini + 6).collect()), &mut r, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(res.amostras.len(), 6);
        // sem silhueta só o quadro sem o alvo
        assert_eq!(res.amostras.iter().filter(|a| a.silhueta.is_none()).map(|a| a.pts).collect::<Vec<_>>(), vec![ini + 3]);
        for a in &res.amostras {
            assert_eq!(a.silhueta.is_some(), a.caixa.is_some(), "{}", a.pts);
            if let Some(s) = &a.silhueta {
                assert_eq!(s, &silhueta::rle(&r.m(a.pts)), "{}", a.pts);
            }
        }
        for r in [ComSilhueta { lado: (2, 1), curta: false, sem: ini + 3 }, ComSilhueta { lado: (1, 1), curta: true, sem: ini + 3 }] {
            let mut r = r;
            let e = correr(&p, &mut LeitorFalso::novo((ini..ini + 6).collect()), &mut r, &AtomicBool::new(false), &mut |_| {}).unwrap_err();
            assert!(e.to_string().contains(crate::edicao::mensagens::codigo::ITEM_INVALIDO), "{e}");
        }
    }

    /// Para trás: mesmo laço com os pts em ordem decrescente; a faixa percorrida sai na ordem da mídia.
    #[test]
    fn para_tras_e_o_mesmo_laco() {
        let mut por_quadro = std::collections::HashMap::new();
        for k in 0..50i64 {
            por_quadro.insert(k, (Some([10.0 + k as f64, 5.0, 4.0, 8.0]), if k == 20 { 0.7 } else { 0.99 }));
        }
        let mut r = MemoriaFalsa { por_quadro, chamadas: 0 };
        let p = PedidoCorrida {
            pixel: TamanhoPx { w: 1, h: 1 },
            ancora: Chave { t: 0.0, pts: Some(40), v: Caixa { x: 50.0, y: 5.0, w: 4.0, h: 8.0 }, interp: Interp::Linear },
            sentido: Sentido::Tras,
            ..pedido(&fixtures_do_laco()[0])
        };
        let res = correr(&p, &mut LeitorFalso::novo((0..=40).rev().collect()), &mut r, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!((res.corrida.de_pts, res.corrida.ate_pts, res.corrida.sentido), (0, 40, Sentido::Tras));
        assert_eq!(res.amostras.iter().map(|a| a.pts).collect::<Vec<_>>(), (0..=40).rev().collect::<Vec<_>>());
        assert_eq!(res.amostras.iter().filter(|a| a.estado == ESTADO_INCERTO).map(|a| a.pts).collect::<Vec<_>>(), vec![20]);
        assert_eq!(res.parou, Parou { motivo: Motivo::Fim, pts: Some(0) });
    }
}
