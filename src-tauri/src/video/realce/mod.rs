//! Leitura e realce de imagem no vídeo (5.3): média de quadros alinhados, super-resolução,
//! estabilização, correção de lente e retificação de perspectiva. Realce não é prova:
//! o original nunca é alterado e tudo que sai tem SHA-256 e registro.

pub mod align;
pub mod average;
pub mod janela;
pub mod perspective;
pub mod stabilize;
pub mod superres;

#[cfg(test)]
mod integracao {
    //! Roda com vídeos de verdade quando `SICRO_REALCE_DIR` aponta para a pasta dos vídeos
    //! sintéticos (limpo_tremido.mp4, ruidoso_tremido.mp4) e `SICRO_REALCE_SAIDA` para onde
    //! gravar os PNG. Sem as variáveis, não faz nada.

    use super::average::{combinar, Opcoes};
    use super::janela::{escolher_janela, ler_janela};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;

    fn rmse(a: &[u8], b: &[u8]) -> f64 {
        let s: f64 = a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).powi(2)).sum();
        (s / a.len() as f64).sqrt()
    }

    fn salvar(nome: &str, saida: &PathBuf, rgb: &[u8], w: u32, h: u32) {
        image::RgbImage::from_raw(w, h, rgb.to_vec()).unwrap().save(saida.join(nome)).unwrap();
    }

    #[test]
    fn media_de_quadros_no_video_sintetico() {
        let (Ok(dir), Ok(saida)) = (std::env::var("SICRO_REALCE_DIR"), std::env::var("SICRO_REALCE_SAIDA")) else { return };
        let (dir, saida) = (PathBuf::from(dir), PathBuf::from(saida));
        std::fs::create_dir_all(&saida).unwrap();
        let (w, h) = (640u32, 360u32);
        let roi = (170u32, 140u32, 300u32, 100u32);
        let margem = 40u32;
        let rect = (roi.0 - margem, roi.1 - margem, roi.2 + 2 * margem, roi.3 + 2 * margem);
        let cancel = AtomicBool::new(false);
        for n in [10usize, 30, 60] {
            let (tempos, rp) = escolher_janela(&dir.join("ruidoso_tremido.mp4"), 3.0, n).unwrap();
            assert_eq!(tempos.len(), n);
            let ini = std::time::Instant::now();
            let ruid = ler_janela(&dir.join("ruidoso_tremido.mp4"), 0.0, &tempos, w, h, rect, rp, None, &cancel).unwrap();
            let t_ler = ini.elapsed().as_secs_f64();
            let limpo = ler_janela(&dir.join("limpo_tremido.mp4"), 0.0, &tempos, w, h, rect, rp, None, &cancel).unwrap();
            let roi_rel = (margem as usize, margem as usize, roi.2 as usize, roi.3 as usize);
            let recorta = |r: &super::average::Recorte| -> Vec<u8> {
                let mut o = Vec::new();
                for y in roi_rel.1..roi_rel.1 + roi_rel.3 {
                    let a = (y * r.w + roi_rel.0) * 3;
                    o.extend_from_slice(&r.rgb[a..a + roi_rel.2 * 3]);
                }
                o
            };
            let verdade = recorta(&limpo.recortes[rp]);
            let crua = recorta(&ruid.recortes[rp]);
            for (nome, mediana, homog) in [("media", false, false), ("mediana", true, false), ("media_homografia", false, true)] {
                let ini = std::time::Instant::now();
                let r = combinar(
                    &ruid.recortes,
                    Opcoes { roi: roi_rel, referencia: rp, homografia: homog, mediana, busca: (margem - 2) as usize },
                    &cancel,
                    &|_, _| {},
                )
                .unwrap();
                let t = ini.elapsed().as_secs_f64();
                println!(
                    "n={n} {nome}: usados {}/{n}, RMSE cru {:.2} -> {:.2}, leitura {t_ler:.2}s, alinhar+combinar {t:.2}s",
                    r.usados, rmse(&crua, &verdade), rmse(&r.rgb, &verdade)
                );
                if n == 30 {
                    salvar(&format!("media_n{n}_{nome}.png"), &saida, &r.rgb, roi.2, roi.3);
                }
            }
            if n == 30 {
                salvar("roi_crua.png", &saida, &crua, roi.2, roi.3);
                salvar("roi_verdade.png", &saida, &verdade, roi.2, roi.3);
            }
        }
    }
}
