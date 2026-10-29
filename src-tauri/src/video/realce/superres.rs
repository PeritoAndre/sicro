//! Super-resolução com Real-ESRGAN (BSD-3) em ONNX, rodando na CPU pelo onnxruntime que o
//! Gerenciador de IA baixa. A imagem é processada em ladrilhos com sobreposição para não
//! estourar a memória. O resultado é uma estimativa: a rede pode inventar detalhe.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{Result, SicroError};

/// Aviso gravado junto de toda imagem gerada por rede neural.
pub const AVISO: &str = "Realce por rede neural: pode inventar detalhe que não existia na imagem. Não usar para identificação sem o original ao lado.";

/// Lado do ladrilho (px da entrada) e sobreposição de cada lado.
const LADRILHO: usize = 160;
const SOBRA: usize = 16;
/// Limite de pixels da saída (evita pedir uma imagem que não cabe na memória).
pub const MAX_PIXELS_SAIDA: u64 = 64_000_000;

pub struct SuperRes {
    session: ort::session::Session,
    input_name: String,
    /// Lado fixo do modelo, se ele não aceita tamanho livre.
    fixo: Option<usize>,
}

fn err(e: &dyn std::fmt::Display) -> SicroError {
    SicroError::Validation(format!("onnxruntime: {e}"))
}

impl SuperRes {
    pub fn load(lib: &Path, model: &Path) -> Result<Self> {
        let env = crate::video::ort_env::entorno(lib)?;
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
        let session = ort::session::Session::builder(env)
            .map_err(|e| err(&e))?
            .with_intra_threads(threads)
            .map_err(|e| err(&e))?
            .commit_from_file(model)
            .map_err(|e| err(&e))?;
        let input = session.inputs().first().ok_or_else(|| err(&"modelo sem entrada"))?;
        let input_name = input.name().to_string();
        let fixo = match input.dtype() {
            ort::value::ValueType::Tensor { shape, .. } => {
                let (h, w) = (shape.get(2).copied().unwrap_or(-1), shape.get(3).copied().unwrap_or(-1));
                (h > 0 && w > 0 && h == w).then_some(h as usize)
            }
            _ => None,
        };
        Ok(Self { session, input_name, fixo })
    }

    /// Um ladrilho RGB `tw`×`th` → RGB `4tw`×`4th`.
    fn rodar(&mut self, rgb: &[u8], tw: usize, th: usize) -> Result<Vec<u8>> {
        let plano = tw * th;
        let mut data = vec![0.0f32; 3 * plano];
        for i in 0..plano {
            for c in 0..3 {
                data[c * plano + i] = rgb[i * 3 + c] as f32 / 255.0;
            }
        }
        let tensor = ort::value::Tensor::from_array(([1usize, 3, th, tw], data)).map_err(|e| err(&e))?;
        let saida = self.session.run(ort::inputs![self.input_name.as_str() => tensor]).map_err(|e| err(&e))?;
        let (forma, v) = saida[0].try_extract_tensor::<f32>().map_err(|e| err(&e))?;
        let (oh, ow) = (forma[2] as usize, forma[3] as usize);
        let po = oh * ow;
        let mut out = vec![0u8; po * 3];
        for i in 0..po {
            for c in 0..3 {
                out[i * 3 + c] = (v[c * po + i] * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
        Ok(out)
    }

    /// Amplia 4x. `progresso(feitos, total)` a cada ladrilho.
    pub fn ampliar4(
        &mut self,
        rgb: &[u8],
        w: usize,
        h: usize,
        cancelar: &AtomicBool,
        progresso: &mut dyn FnMut(usize, usize),
    ) -> Result<Vec<u8>> {
        if (w * 4) as u64 * (h * 4) as u64 > MAX_PIXELS_SAIDA {
            return Err(SicroError::Validation(
                "a imagem ampliada ficaria grande demais: recorte a região de interesse antes".into(),
            ));
        }
        let (core, sobra) = match self.fixo {
            Some(l) => (l.saturating_sub(2 * SOBRA).max(8), SOBRA),
            None => (LADRILHO, SOBRA),
        };
        let nx = w.div_ceil(core);
        let ny = h.div_ceil(core);
        let (ow, oh) = (w * 4, h * 4);
        let mut out = vec![0u8; ow * oh * 3];
        let pega = |x: i64, y: i64| -> usize { (y.clamp(0, h as i64 - 1) as usize * w + x.clamp(0, w as i64 - 1) as usize) * 3 };
        for ty in 0..ny {
            for tx in 0..nx {
                if cancelar.load(Ordering::Relaxed) {
                    return Err(SicroError::Validation("cancelado".into()));
                }
                let (x0, y0) = (tx * core, ty * core);
                let (cw, ch) = (core.min(w - x0), core.min(h - y0));
                // com modelo de lado fixo, o ladrilho de entrada tem sempre esse lado
                let (iw, ih) = match self.fixo {
                    Some(l) => (l, l),
                    None => (cw + 2 * sobra, ch + 2 * sobra),
                };
                let mut tile = vec![0u8; iw * ih * 3];
                for yy in 0..ih {
                    for xx in 0..iw {
                        let o = pega(x0 as i64 + xx as i64 - sobra as i64, y0 as i64 + yy as i64 - sobra as i64);
                        tile[(yy * iw + xx) * 3..(yy * iw + xx) * 3 + 3].copy_from_slice(&rgb[o..o + 3]);
                    }
                }
                let up = self.rodar(&tile, iw, ih)?;
                let uw = iw * 4;
                for yy in 0..ch * 4 {
                    let sy = yy + sobra * 4;
                    let dst = ((y0 * 4 + yy) * ow + x0 * 4) * 3;
                    let src = (sy * uw + sobra * 4) * 3;
                    out[dst..dst + cw * 4 * 3].copy_from_slice(&up[src..src + cw * 4 * 3]);
                }
                progresso(ty * nx + tx + 1, nx * ny);
            }
        }
        Ok(out)
    }
}

/// Reduz por `fator` (2 = metade) com Lanczos: o 2x sai do modelo de 4x.
pub fn reduzir(rgb: &[u8], w: usize, h: usize, novo_w: usize, novo_h: usize) -> Vec<u8> {
    let img = image::RgbImage::from_raw(w as u32, h as u32, rgb.to_vec()).expect("tamanho coerente");
    image::imageops::resize(&img, novo_w as u32, novo_h as u32, image::imageops::FilterType::Lanczos3).into_raw()
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Precisa de SICRO_ORT_LIB e SICRO_SR_MODEL; sem elas o teste não faz nada.
    #[test]
    fn ladrilhado_nao_deixa_emenda() {
        let (Ok(lib), Ok(modelo)) = (std::env::var("SICRO_ORT_LIB"), std::env::var("SICRO_SR_MODEL")) else { return };
        // o YOLOX (Pessoas) e a super-resolução convivem no mesmo processo
        if let Ok(y) = std::env::var("SICRO_YOLOX_MODEL") {
            crate::video::pessoas::yolox::Yolox::load(Path::new(&lib), Path::new(&y)).expect("yolox");
        }
        let mut sr = SuperRes::load(Path::new(&lib), Path::new(&modelo)).expect("carrega");
        // imagem suave com 70x50: em ladrilhos pequenos de 8 px não pode surgir emenda
        let (w, h) = (70usize, 50usize);
        let mut rgb = vec![0u8; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                for c in 0..3 {
                    rgb[(y * w + x) * 3 + c] = (60.0 + 1.5 * x as f32 + 0.8 * y as f32 + 10.0 * c as f32) as u8;
                }
            }
        }
        let cancel = AtomicBool::new(false);
        let up = sr.ampliar4(&rgb, w, h, &cancel, &mut |_, _| {}).expect("amplia");
        assert_eq!(up.len(), w * 4 * h * 4 * 3);
        // gradiente suave: diferença entre colunas vizinhas sempre pequena (sem degrau de emenda)
        let ow = w * 4;
        let mut maior = 0i32;
        for y in 0..h * 4 {
            for x in 1..ow {
                let d = (up[(y * ow + x) * 3] as i32 - up[(y * ow + x - 1) * 3] as i32).abs();
                maior = maior.max(d);
            }
        }
        assert!(maior < 12, "degrau {maior}");
    }

    /// Real-ESRGAN num recorte de um quadro de verdade: SICRO_SR_IMG (PNG), SICRO_SR_ROI ("x,y,w,h")
    /// e SICRO_REALCE_SAIDA (onde gravar). Sem elas, não faz nada.
    #[test]
    fn recorte_de_quadro_real() {
        let (Ok(lib), Ok(modelo), Ok(img), Ok(roi), Ok(saida)) = (
            std::env::var("SICRO_ORT_LIB"), std::env::var("SICRO_SR_MODEL"), std::env::var("SICRO_SR_IMG"),
            std::env::var("SICRO_SR_ROI"), std::env::var("SICRO_REALCE_SAIDA"),
        ) else { return };
        let r: Vec<u32> = roi.split(',').map(|v| v.trim().parse().unwrap()).collect();
        let nome = std::env::var("SICRO_SR_NOME").unwrap_or_else(|_| "sr".into());
        let full = image::open(&img).unwrap().to_rgb8();
        let crop = image::imageops::crop_imm(&full, r[0], r[1], r[2], r[3]).to_image();
        let (w, h) = crop.dimensions();
        let saida = std::path::PathBuf::from(saida);
        let mut sr = SuperRes::load(Path::new(&lib), Path::new(&modelo)).unwrap();
        let cancel = AtomicBool::new(false);
        let ini = std::time::Instant::now();
        let up = sr.ampliar4(crop.as_raw(), w as usize, h as usize, &cancel, &mut |_, _| {}).unwrap();
        let t4 = ini.elapsed().as_secs_f64();
        println!("{nome}: recorte {w}x{h} -> 4x {}x{} em {t4:.2}s ({:.2} Mpx/s de entrada)", w * 4, h * 4, (w * h) as f64 / 1e6 / t4);
        let up2 = reduzir(&up, (w * 4) as usize, (h * 4) as usize, (w * 2) as usize, (h * 2) as usize);
        // lado a lado: ampliação comum (bicubic, 4x) | super-resolução 4x
        let comum = image::imageops::resize(&crop, w * 4, h * 4, image::imageops::FilterType::CatmullRom);
        let mut par = image::RgbImage::new(w * 8 + 8, h * 4);
        image::imageops::replace(&mut par, &comum, 0, 0);
        let sr4 = image::RgbImage::from_raw(w * 4, h * 4, up).unwrap();
        image::imageops::replace(&mut par, &sr4, (w * 4 + 8) as i64, 0);
        par.save(saida.join(format!("{nome}_comum_x_superres4x.png"))).unwrap();
        image::RgbImage::from_raw(w * 2, h * 2, up2).unwrap().save(saida.join(format!("{nome}_superres2x.png"))).unwrap();
        crop.save(saida.join(format!("{nome}_entrada.png"))).unwrap();
    }
}
