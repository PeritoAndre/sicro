//! Retificação de perspectiva: 4 pontos de um plano (chão, parede, placa) com medidas reais
//! viram a imagem "vista de cima" (ou de frente), por homografia DLT (a mesma da Velocidade).

use image::RgbaImage;
use nalgebra::Vector3;
use serde::Serialize;

use crate::video::speed::homography::solve_homography_dlt;

#[derive(Debug, Clone, Copy)]
pub struct Par {
    pub px: f64,
    pub py: f64,
    /// Metros no plano (y cresce para o fundo da cena: no resultado fica para cima).
    pub mx: f64,
    pub my: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct InfoRetificada {
    pub largura: u32,
    pub altura: u32,
    pub px_por_m: f64,
    /// Plano mostrado, em metros: x mín., x máx., y mín., y máx.
    pub mundo: [f64; 4],
}

/// Pontos de um retângulo marcados como: 1 perto-esquerda, 2 perto-direita, 3 longe-direita,
/// 4 longe-esquerda. `largura` = lado 1→2, `profundidade` = lado 2→3.
pub fn pares_de_retangulo(px: &[(f64, f64); 4], largura: f64, profundidade: f64) -> [Par; 4] {
    let m = [(0.0, 0.0), (largura, 0.0), (largura, profundidade), (0.0, profundidade)];
    std::array::from_fn(|i| Par { px: px[i].0, py: px[i].1, mx: m[i].0, my: m[i].1 })
}

/// `margem`: folga ao redor do quadrilátero, em fração do maior lado (0 = só o retângulo).
/// `px_por_m` `None` = a resolução da própria imagem no quadrilátero (limitada a 2400 px).
pub fn retificar(
    rgb: &[u8],
    w: u32,
    h: u32,
    pares: &[Par; 4],
    margem: f64,
    px_por_m: Option<f64>,
) -> Result<(RgbaImage, InfoRetificada), String> {
    let img: [(f64, f64); 4] = std::array::from_fn(|i| (pares[i].px, pares[i].py));
    let mun: [(f64, f64); 4] = std::array::from_fn(|i| (pares[i].mx, pares[i].my));
    // H: mundo → imagem (o sentido que a amostragem inversa precisa).
    let h_mi = solve_homography_dlt(&mun, &img).map_err(|e| e.to_string())?.h;
    let (x0, x1) = mun.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.0), a.1.max(p.0)));
    let (y0, y1) = mun.iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
    let lado = (x1 - x0).max(y1 - y0);
    if lado <= 0.0 {
        return Err("as medidas reais precisam ser maiores que zero".into());
    }
    let folga = lado * margem.max(0.0);
    let (mx0, mx1, my0, my1) = (x0 - folga, x1 + folga, y0 - folga, y1 + folga);

    let ppm = px_por_m.filter(|v| *v > 0.0).unwrap_or_else(|| {
        // resolução da imagem: média de (comprimento em px / comprimento real) dos 4 lados
        let mut s = 0.0;
        for i in 0..4 {
            let j = (i + 1) % 4;
            let dp = ((img[i].0 - img[j].0).powi(2) + (img[i].1 - img[j].1).powi(2)).sqrt();
            let dm = ((mun[i].0 - mun[j].0).powi(2) + (mun[i].1 - mun[j].1).powi(2)).sqrt();
            if dm > 0.0 {
                s += dp / dm;
            }
        }
        s / 4.0
    });
    let ppm = ppm.min(2400.0 / (mx1 - mx0).max(my1 - my0)).max(1.0);
    let (ow, oh) = ((((mx1 - mx0) * ppm).round() as u32).max(2), (((my1 - my0) * ppm).round() as u32).max(2));
    if ow.max(oh) > 4000 {
        return Err("imagem retificada grande demais".into());
    }

    let mut out = RgbaImage::new(ow, oh);
    let src = |x: usize, y: usize, c: usize| rgb[(y * w as usize + x) * 3 + c] as f64;
    for j in 0..oh {
        // linha 0 = fundo da cena (maior y)
        let wy = my1 - (j as f64 + 0.5) / ppm;
        for i in 0..ow {
            let wx = mx0 + (i as f64 + 0.5) / ppm;
            let p: Vector3<f64> = h_mi * Vector3::new(wx, wy, 1.0);
            if p.z.abs() < 1e-12 {
                continue;
            }
            let (u, v) = (p.x / p.z - 0.5, p.y / p.z - 0.5);
            if u < 0.0 || v < 0.0 || u > (w - 1) as f64 || v > (h - 1) as f64 {
                continue;
            }
            let (xa, ya) = (u.floor() as usize, v.floor() as usize);
            let (xb, yb) = ((xa + 1).min(w as usize - 1), (ya + 1).min(h as usize - 1));
            let (fx, fy) = (u - xa as f64, v - ya as f64);
            let mut px = [0u8, 0, 0, 255];
            for c in 0..3 {
                let a = src(xa, ya, c) * (1.0 - fx) + src(xb, ya, c) * fx;
                let b = src(xa, yb, c) * (1.0 - fx) + src(xb, yb, c) * fx;
                px[c] = (a * (1.0 - fy) + b * fy).round().clamp(0.0, 255.0) as u8;
            }
            out.put_pixel(i, j, image::Rgba(px));
        }
    }
    Ok((out, InfoRetificada { largura: ow, altura: oh, px_por_m: ppm, mundo: [mx0, mx1, my0, my1] }))
}

#[cfg(test)]
mod testes {
    use super::*;
    use nalgebra::Matrix3;

    /// Plano 2 m × 1 m (100 px/m) com textura; projetado numa imagem por uma homografia conhecida.
    #[test]
    fn retifica_de_volta_ao_plano() {
        let plano = |x: f64, y: f64| -> [f64; 3] {
            let a = 128.0 + 90.0 * ((x * 0.35).sin() * (y * 0.5).cos());
            let b = 128.0 + 90.0 * ((x * 0.11 + y * 0.2).sin());
            [a, b, if ((x / 20.0) as i64 + (y / 20.0) as i64) % 2 == 0 { 220.0 } else { 40.0 }]
        };
        // plano (px, y para cima: linha 0 = y = 1 m) → imagem
        let h_pi = Matrix3::new(1.2, 0.35, 90.0, 0.1, 0.9, 60.0, 0.0006, 0.0011, 1.0);
        let inv = h_pi.try_inverse().unwrap();
        let (w, h) = (480u32, 360u32);
        let mut rgb = vec![0u8; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w {
                let p = inv * Vector3::new(x as f64 + 0.5, y as f64 + 0.5, 1.0);
                let (u, v) = (p.x / p.z, p.y / p.z);
                let c = if (0.0..200.0).contains(&u) && (0.0..100.0).contains(&v) { plano(u, v) } else { [0.0; 3] };
                for k in 0..3 {
                    rgb[((y * w + x) * 3) as usize + k] = c[k] as u8;
                }
            }
        }
        // cantos do plano (px do plano: u, v com v para baixo) → imagem. Marcação: perto = v grande (baixo).
        let proj = |u: f64, v: f64| {
            let p = h_pi * Vector3::new(u, v, 1.0);
            (p.x / p.z, p.y / p.z)
        };
        // 1 perto-esquerda (u=0,v=100), 2 perto-direita (200,100), 3 longe-direita (200,0), 4 longe-esquerda (0,0)
        let pts = [proj(0.0, 100.0), proj(200.0, 100.0), proj(200.0, 0.0), proj(0.0, 0.0)];
        let pares = pares_de_retangulo(&pts, 2.0, 1.0);
        let (out, info) = retificar(&rgb, w, h, &pares, 0.0, Some(100.0)).unwrap();
        assert_eq!((info.largura, info.altura), (200, 100));
        let (mut s, mut n) = (0.0f64, 0u32);
        for j in 10..90 {
            for i in 10..190 {
                let esp = plano(i as f64 + 0.5, j as f64 + 0.5);
                let got = out.get_pixel(i, j).0;
                for k in 0..3 {
                    s += (esp[k].floor() - got[k] as f64).powi(2);
                    n += 1;
                }
            }
        }
        let rms = (s / n as f64).sqrt();
        assert!(rms < 14.0, "rms {rms}");
    }
}
