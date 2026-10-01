//! Separação de locutores ("quem fala quando") com o sherpa-onnx LOCAL
//! (baixado sob demanda no gerenciador de IA; roda offline). Segmentação
//! pyannote 3.0 + assinatura de voz WeSpeaker + agrupamento.
//!
//! É apoio à degravação: diz que vozes DIFERENTES falam em tais trechos — não
//! identifica ninguém. O agrupamento automático pode juntar ou separar vozes;
//! informar o número de locutores, quando conhecido, é mais confiável.

use std::path::Path;

use crate::error::{Result, SicroError};
use crate::models::DiarTurn;

/// Limiar do modo automático (distância no agrupamento; menor → mais vozes).
/// Escolhido nos áudios de teste do sherpa-onnx (2 e 4 locutores).
pub const AUTO_THRESHOLD: f32 = 0.4;

pub fn run(
    bin: &Path,
    segmentation: &Path,
    embedding: &Path,
    wav16k: &Path,
    num_speakers: Option<u32>,
) -> Result<Vec<DiarTurn>> {
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().min(4));
    let mut args = vec![
        format!("--segmentation.pyannote-model={}", segmentation.display()),
        format!("--embedding.model={}", embedding.display()),
        format!("--segmentation.num-threads={threads}"),
        format!("--embedding.num-threads={threads}"),
    ];
    match num_speakers {
        Some(n) if n > 0 => args.push(format!("--clustering.num-clusters={n}")),
        _ => args.push(format!("--clustering.cluster-threshold={AUTO_THRESHOLD}")),
    }
    // O áudio vai por último (o programa lê as opções antes do arquivo).
    args.push(wav16k.display().to_string());
    let out = crate::tools::command(bin)
        .args(&args)
        .output()
        .map_err(|e| SicroError::Validation(format!("falha ao executar o separador de locutores: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let lines: Vec<&str> = err.lines().filter(|l| !l.trim().is_empty()).collect();
        let tail = lines[lines.len().saturating_sub(3)..].join(" | ");
        return Err(SicroError::Validation(format!("o separador de locutores falhou: {tail}")));
    }
    Ok(parse_output(&String::from_utf8_lossy(&out.stdout)))
}

/// Lê as linhas `início -- fim speaker_XX`. Os números do programa têm buracos
/// (speaker_00, speaker_02…): renumera 1..N pela ordem da primeira fala.
pub fn parse_output(stdout: &str) -> Vec<DiarTurn> {
    let mut order: Vec<String> = Vec::new();
    let mut turns = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let [a, "--", b, spk, ..] = parts.as_slice() else { continue };
        let (Ok(t_start), Ok(t_end)) = (a.parse::<f64>(), b.parse::<f64>()) else { continue };
        if !spk.starts_with("speaker_") || t_end <= t_start {
            continue;
        }
        let idx = match order.iter().position(|s| s == spk) {
            Some(i) => i,
            None => {
                order.push(spk.to_string());
                order.len() - 1
            }
        };
        turns.push(DiarTurn { t_start, t_end, speaker: idx as u32 + 1 });
    }
    turns.sort_by(|x, y| x.t_start.total_cmp(&y.t_start));
    turns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_a_saida_e_renumera_pela_primeira_fala() {
        let out = "OfflineSpeakerDiarizationConfig(...)\nStarted\n\
            0.318 -- 6.865 speaker_02\n\
            7.017 -- 10.747 speaker_00\n\
            11.455 -- 13.632 speaker_00\n\
            22.137 -- 24.837 speaker_02 confidence=0.653\n\
            48.040 -- 50.470 speaker_10\n\
            lixo -- 3 speaker_01\n";
        let t = parse_output(out);
        let spk: Vec<u32> = t.iter().map(|x| x.speaker).collect();
        assert_eq!(spk, vec![1, 2, 2, 1, 3]);
        assert!((t[0].t_start - 0.318).abs() < 1e-9 && (t[0].t_end - 6.865).abs() < 1e-9);
    }
}
