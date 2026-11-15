//! Motor `mancha` (§6.2): adaptador do `Seguidor` de `video/pessoas/mancha.rs` (correlação, sem modelo). Roda sem os
//! modelos (interruptor desligado ou sem onnxruntime). Laço simples: para no primeiro quadro em que a correlação
//! cai (como o `mancha::seguir` da análise). Dono: P2.2.
//!
//! Trabalha em tons de cinza reduzidos por um fator inteiro (média dos blocos) até ~960 px no lado maior, menos se a
//! caixa for pequena (lado menor da caixa ≥ 28 px), como o seguidor da análise.

use super::{Busca, CaixaPx, Imagem, Laco, Limiares, ModeloCorrida, MotorCorrida, MotorRastreio, Passo, Rastreador};
use crate::edicao::mensagens::codigo;
use crate::error::{Result, SicroError};
use crate::video::pessoas::mancha::{Cinza, Seguidor};

/// Abaixo desta correlação o seguidor não aceita (o `OpcoesMancha::default` da análise).
pub const CONF_MIN: f64 = 0.55;
/// Daqui para cima o modelo se atualiza (`CONF_ATUALIZA` do seguidor): abaixo, a amostra fica incerta.
pub const CONF_OK: f64 = 0.7;
/// Lado maior da imagem de trabalho e lado menor mínimo da caixa nela (px).
const LADO_TRABALHO: f64 = 960.0;
const CAIXA_MIN: f64 = 28.0;

pub struct Mancha {
    seguidor: Option<Seguidor>,
    /// Fator inteiro de redução da imagem de trabalho.
    fator: u32,
}

impl Mancha {
    pub fn nova() -> Result<Self> {
        Ok(Self { seguidor: None, fator: 1 })
    }

    /// Cinza reduzido por `fator` (média de cada bloco `fator`×`fator`; a sobra da borda fica de fora).
    fn cinza(q: &Imagem, fator: u32) -> Cinza {
        let f = fator.max(1) as usize;
        let (w, h) = (q.w as usize / f, q.h as usize / f);
        let mut d = vec![0u8; w * h];
        let largura = q.w as usize * 3;
        for y in 0..h {
            for x in 0..w {
                let mut soma = 0u32;
                for yy in y * f..(y + 1) * f {
                    let linha = &q.rgb[yy * largura..];
                    for xx in x * f..(x + 1) * f {
                        let p = &linha[xx * 3..xx * 3 + 3];
                        soma += (p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) >> 8;
                    }
                }
                d[y * w + x] = (soma / (f * f) as u32) as u8;
            }
        }
        Cinza { w, h, d }
    }
}

impl Rastreador for Mancha {
    fn iniciar(&mut self, quadro: &Imagem, caixa: CaixaPx) -> Result<Passo> {
        let lado = quadro.w.max(quadro.h) as f64;
        let menor = caixa.w.min(caixa.h).max(1.0);
        let esc = (LADO_TRABALHO / lado).max(CAIXA_MIN / menor).min(1.0);
        self.fator = (1.0 / esc).floor().max(1.0) as u32;
        let f = self.fator as f64;
        let img = Self::cinza(quadro, self.fator);
        let s = Seguidor::novo(&img, (caixa.x / f) as f32, (caixa.y / f) as f32, (caixa.w / f) as f32, (caixa.h / f) as f32)
            .ok_or_else(|| SicroError::Validation(format!("{}: caixa pequena ou lisa demais para a mancha", codigo::REGIAO)))?;
        self.seguidor = Some(s);
        Ok(Passo { caixa: Some(caixa), conf: 1.0, silhueta: None })
    }

    fn seguir(&mut self, quadro: &Imagem, _busca: &Busca) -> Result<Passo> {
        let s = self.seguidor.as_mut().ok_or_else(|| SicroError::Validation(format!("{}: mancha sem início", codigo::ITEM_INVALIDO)))?;
        let img = Self::cinza(quadro, self.fator);
        let f = self.fator as f64;
        Ok(match s.passo(&img, CONF_MIN as f32) {
            Some((_, _, conf)) => {
                let (x, y, w, h) = s.caixa();
                Passo { caixa: Some(CaixaPx { x: x as f64 * f, y: y as f64 * f, w: w as f64 * f, h: h as f64 * f }), conf: conf as f64, silhueta: None }
            }
            None => Passo { caixa: None, conf: 0.0, silhueta: None },
        })
    }

    fn laco(&self) -> Laco {
        Laco::Simples
    }

    fn limiares(&self, _fps: f64) -> Limiares {
        Limiares { ok: CONF_OK, perdido: CONF_MIN, paciencia: 1 }
    }

    fn corrida(&self) -> MotorCorrida {
        MotorCorrida {
            motor: MotorRastreio::Mancha,
            modelo: ModeloCorrida {
                nome: "mancha".into(),
                origem: "SICRO".into(),
                commit: env!("CARGO_PKG_VERSION").into(),
                arquivos: vec![],
                desvio: String::new(),
            },
            runtime: String::new(),
            provedor: crate::ia::Provedor::Cpu.fio().into(),
        }
    }
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;
    use crate::edicao::modelo::{Chave, Interp, TamanhoPx};
    use crate::edicao::rastreio::laco;
    use crate::edicao::rastreio::{LeitorQuadros, Motivo, PedidoCorrida, Sentido, ESTADO_OK, MAX_QUADROS};
    use std::sync::atomic::AtomicBool;

    fn ruido(x: i32, y: i32, sem: u32) -> u8 {
        let mut h = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263) ^ sem.wrapping_mul(2246822519);
        h = (h ^ (h >> 13)).wrapping_mul(1274126177);
        ((h ^ (h >> 16)) & 255) as u8
    }

    /// Quadro RGB `w`×`h`: fundo liso escuro e um bloco texturizado `bw`×`bh` em (`bx`, `by`) (sem bloco se `bx` < 0).
    pub(crate) fn quadro(w: u32, h: u32, bx: i32, by: i32, bw: i32, bh: i32) -> Vec<u8> {
        let mut v = vec![0u8; (w * h * 3) as usize];
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let fundo = 40 + ruido(x / 8, y / 8, 1) / 8;
                let c = if bx >= 0 && x >= bx && x < bx + bw && y >= by && y < by + bh { ruido(x - bx, y - by, 7) } else { fundo };
                let i = ((y as u32 * w + x as u32) * 3) as usize;
                v[i..i + 3].copy_from_slice(&[c, c / 2, 255 - c]);
            }
        }
        v
    }

    struct Quadros {
        q: Vec<Vec<u8>>,
        w: u32,
        h: u32,
        i: usize,
    }

    impl LeitorQuadros for Quadros {
        fn proximo(&mut self) -> Result<Option<(i64, Imagem<'_>)>> {
            let i = self.i;
            self.i += 1;
            Ok(self.q.get(i).map(|q| (i as i64 * 512, Imagem { rgb: q, w: self.w, h: self.h })))
        }
    }

    /// Bloco que anda 3 px por quadro e some no quadro 40: segue a menos de 1 px (da imagem de trabalho) e para ali.
    #[test]
    fn segue_e_para_quando_some() {
        for (w, h, passo) in [(320u32, 240u32, 3i32), (1920, 1080, 6)] {
            let pos = |k: i32| (60 + passo * k, 100 + k / 2);
            let q: Vec<Vec<u8>> = (0..60).map(|k| if k < 40 { quadro(w, h, pos(k).0, pos(k).1, 48, 72) } else { quadro(w, h, -1, 0, 0, 0) }).collect();
            let pixel = TamanhoPx { w, h };
            let ancora = CaixaPx { x: 60.0, y: 100.0, w: 48.0, h: 72.0 }.caixa(pixel);
            let p = PedidoCorrida { rastreio: "r1".into(), midia_sha256: "0".repeat(64), espaco: "original".into(), pixel, ancora: Chave { t: 0.0, pts: Some(0), v: ancora, interp: Interp::Linear }, sentido: Sentido::Frente, ate_pts: None, max_quadros: MAX_QUADROS, fps: 30.0 };
            let mut m = Mancha::nova().unwrap();
            let r = laco::correr(&p, &mut Quadros { q, w, h, i: 0 }, &mut m, &AtomicBool::new(false), &mut |_| {}).unwrap();
            assert_eq!((r.parou.motivo, r.parou.pts), (Motivo::Perdido, Some(40 * 512)), "{w}x{h}");
            assert_eq!(r.amostras.len(), 40);
            let tol = m.fator as f64 + 0.5;
            for (k, a) in r.amostras.iter().enumerate() {
                let c = CaixaPx::de_caixa(&a.caixa.unwrap(), pixel);
                let (x, y) = pos(k as i32);
                assert!((c.x - x as f64).abs() <= tol && (c.y - y as f64).abs() <= tol, "{w}x{h} quadro {k}: ({}, {}) × ({x}, {y})", c.x, c.y);
                assert_eq!(a.estado, ESTADO_OK, "{w}x{h} quadro {k}");
            }
            assert_eq!(r.corrida.motor.motor, MotorRastreio::Mancha);
            assert_eq!((r.corrida.limiares.perdido, r.corrida.limiares.paciencia), (CONF_MIN, 1));
        }
    }

    #[test]
    fn caixa_lisa_e_recusada() {
        let q = vec![90u8; 100 * 100 * 3];
        let mut m = Mancha::nova().unwrap();
        let e = m.iniciar(&Imagem { rgb: &q, w: 100, h: 100 }, CaixaPx { x: 10.0, y: 10.0, w: 30.0, h: 30.0 }).unwrap_err();
        assert!(e.to_string().contains(codigo::REGIAO), "{e}");
    }
}
