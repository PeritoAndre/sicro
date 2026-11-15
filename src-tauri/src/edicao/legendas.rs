//! Degravação → legendas e trechos de fala (§5.1 `legendas.rs`; conferência §4): lê a degravação do som da mídia
//! no banco do caso (`audio_transcript_segments`) e devolve os trechos em TEMPO DE FONTE da mídia. A tela faz deles
//! `Legenda` (P1.6, "Da degravação…" das legendas) ou `Silencio` (P1.5, "Silenciar trecho → Da degravação…").
//! Dono: P1.6.
//!
//! A degravação é do WAV de análise do módulo Áudio: extraído do vídeo (`kind = extraido`, `source_video_sha256`),
//! importado (`sha256`/`original_sha256`) ou realce de um desses (`audio_enhancements`, mesmo tempo da origem). O WAV
//! extraído (`-i <vídeo> -vn`, sem `-copyts`) começa na primeira amostra do fluxo de áudio: tempo de fonte = início
//! desse fluxo + tempo do WAV (medido nos dois ffmpeg, `testes::degravacao_em_tempo_de_fonte`).

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::database::repositories::{audio_repo, video_repo};
use crate::edicao::executar::{abrir_banco, sha_valido};
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::OrigemLegendas;
use crate::error::{Result, SicroError};
use crate::models::AudioTranscriptSegment;
use crate::workspace::paths;

/// Teto de áudios seguidos pelos realces (cadeia em laço no banco não prende a leitura).
const MAX_CANDIDATOS: usize = 256;

/// Trecho de fala em tempo de fonte (s). `ia` = rascunho da IA não revisto pelo perito: texto em rascunho (`draft`)
/// ou locutor posto pela separação de locutores (`speaker_auto`); vira legenda `ia`, não conferida. `locutor`
/// vazio = sem locutor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrechoFala {
    pub de: f64,
    pub ate: f64,
    pub texto: String,
    pub locutor: Option<String>,
    pub ia: bool,
}

/// Degravação da mídia: de onde veio (vai para `TrilhaLegendas.origem`) e os trechos em ordem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Degravacao {
    pub origem: OrigemLegendas,
    pub trechos: Vec<TrechoFala>,
}

fn numero(v: &Value) -> Option<f64> {
    v.as_str().and_then(|s| s.trim().parse::<f64>().ok()).or_else(|| v.as_f64()).filter(|x| x.is_finite())
}

/// Início (s, tempo de fonte) do fluxo de áudio que a extração (`-vn`, escolha padrão do ffmpeg: o fluxo `default`,
/// depois o de mais canais, empate no primeiro) leva ao WAV; sem `start_time` no fluxo, o do contêiner.
/// `raw` = `ffprobe -show_format -show_streams -of json`. None = sem fluxo de áudio.
pub(crate) fn inicio_do_som(raw: &str) -> Option<f64> {
    let j: Value = serde_json::from_str(raw).ok()?;
    let nota = |s: &Value| s["channels"].as_u64().unwrap_or(0) + 5_000_000 * u64::from(s["disposition"]["default"].as_u64() == Some(1));
    let mut melhor: Option<&Value> = None;
    for s in j["streams"].as_array()?.iter().filter(|s| s["codec_type"] == "audio") {
        if melhor.is_none_or(|m| nota(s) > nota(m)) {
            melhor = Some(s);
        }
    }
    let s = melhor?;
    numero(&s["start_time"]).or_else(|| numero(&j["format"]["start_time"])).or(Some(0.0))
}

/// Rascunho da IA ainda não revisto (texto ou locutor).
fn rascunho(ai: &Option<Value>) -> bool {
    ai.as_ref().is_some_and(|a| a["draft"] == Value::Bool(true) || a["speaker_auto"] == Value::Bool(true))
}

/// Trechos em ordem de início, deslocados para o tempo de fonte. Fim ausente: o início do seguinte, senão o fim do
/// áudio; sem nenhum dos dois, igual ao início (a validação acusa a legenda). Fim gravado vale como está.
fn trechos(segs: &[AudioTranscriptSegment], desloc: f64, duracao: Option<f64>) -> Vec<TrechoFala> {
    let mut v: Vec<&AudioTranscriptSegment> = segs.iter().filter(|s| s.t_start.is_finite()).collect();
    v.sort_by(|a, b| a.t_start.total_cmp(&b.t_start).then(a.idx.cmp(&b.idx)));
    (0..v.len())
        .map(|i| {
            let s = v[i];
            let fim = match s.t_end {
                Some(e) if e.is_finite() => e,
                _ => v
                    .get(i + 1)
                    .map(|n| n.t_start)
                    .filter(|e| *e > s.t_start)
                    .or_else(|| duracao.filter(|e| e.is_finite() && *e > s.t_start))
                    .unwrap_or(s.t_start),
            };
            TrechoFala {
                de: desloc + s.t_start,
                ate: desloc + fim,
                texto: s.text.trim().to_string(),
                locutor: Some(s.speaker.trim()).filter(|x| !x.is_empty()).map(str::to_string),
                ia: rascunho(&s.ai),
            }
        })
        .collect()
}

/// Motor da transcrição automática pelo último registro `audio.transcribe` do áudio ("<programa> <modelo>").
/// O SHA-256 do modelo não é registrado pelo módulo Áudio: fica None.
fn transcricao(conn: &rusqlite::Connection, occ: &uuid::Uuid, audio_sha256: &str) -> Option<String> {
    let detalhes: String = conn
        .query_row(
            "SELECT details_json FROM audio_operation_logs WHERE occurrence_id = ?1 AND media_hash = ?2 AND action = 'audio.transcribe' \
             ORDER BY id DESC LIMIT 1",
            rusqlite::params![occ.to_string(), audio_sha256],
            |r| r.get(0),
        )
        .ok()?;
    let d: Value = serde_json::from_str(&detalhes).ok()?;
    let m = format!("{} {}", d["tool"].as_str().unwrap_or(""), d["model"].as_str().unwrap_or("")).trim().to_string();
    (!m.is_empty()).then_some(m)
}

/// `edicao_degravacao`: degravação do som da mídia `sha256` do caso. Erro `degravacao_ausente` se não há.
/// Mais de um áudio com degravação (extraído e realce, duas extrações): vale a gravada por último.
pub fn degravacao(ws: &Path, sha256: &str) -> Result<Degravacao> {
    if !sha_valido(sha256) {
        return Err(SicroError::Validation(format!("{}: {sha256}", codigo::SHA256_INVALIDO)));
    }
    let (conn, occ) = abrir_banco(ws)?;
    let audios = audio_repo::list_for_occurrence(&conn, &occ)?;
    // (WAV de análise, onde o 0 dele cai no tempo de fonte da mídia; None = não se sabe)
    let mut candidatos: Vec<(String, Option<f64>)> = Vec::new();
    let extraidos: Vec<&str> =
        audios.iter().filter(|a| a.kind == "extraido" && a.source_video_sha256.as_deref() == Some(sha256)).map(|a| a.sha256.as_str()).collect();
    if !extraidos.is_empty() {
        if let Some(v) = video_repo::find_media_by_sha256(&conn, &occ, sha256)? {
            // probe gravado no registro; registro antigo sem fluxos: o arquivo
            let desloc = inicio_do_som(&v.raw_probe_json)
                .or_else(|| crate::video::probe::probe_media(&paths::resolver_relativo(ws, &v.relative_path)).ok().and_then(|p| inicio_do_som(&p.raw_json)));
            candidatos.extend(extraidos.iter().map(|s| (s.to_string(), desloc)));
        }
    }
    for a in audios.iter().filter(|a| a.sha256 == sha256 || a.original_sha256.as_deref() == Some(sha256)) {
        candidatos.push((a.sha256.clone(), Some(0.0)));
    }
    // realces: mesmo tempo da origem
    let realces = audio_repo::list_enhancements_for_occurrence(&conn, &occ)?;
    let mut vistos: HashSet<String> = candidatos.iter().map(|c| c.0.clone()).collect();
    let mut i = 0;
    while i < candidatos.len() && candidatos.len() < MAX_CANDIDATOS {
        let (sha, d) = candidatos[i].clone();
        for r in realces.iter().filter(|r| r.source_audio_sha256 == sha) {
            if vistos.insert(r.output_audio_sha256.clone()) {
                candidatos.push((r.output_audio_sha256.clone(), d));
            }
        }
        i += 1;
    }
    let mut melhor: Option<(chrono::DateTime<chrono::Utc>, usize, Vec<AudioTranscriptSegment>)> = None;
    for (k, (sha, _)) in candidatos.iter().enumerate() {
        let segs = audio_repo::list_segments(&conn, &occ, sha)?;
        let Some(quando) = segs.iter().map(|s| s.created_at).max() else { continue };
        if melhor.as_ref().is_none_or(|(q, ..)| quando > *q) {
            melhor = Some((quando, k, segs));
        }
    }
    let Some((_, k, segs)) = melhor else {
        return Err(SicroError::Validation(format!("{}: {sha256}", codigo::DEGRAVACAO_AUSENTE)));
    };
    let (audio_sha256, desloc) = candidatos.swap_remove(k);
    // início do som desconhecido (registro sem fluxos e arquivo fora do lugar): erro, nunca 0 calado
    let desloc = desloc.ok_or_else(|| SicroError::Validation(format!("{}: {sha256}", codigo::MIDIA_AUSENTE)))?;
    let duracao = audios.iter().find(|a| a.sha256 == audio_sha256).and_then(|a| a.duration_s);
    let motor = if segs.iter().any(|s| s.ai.is_some()) { transcricao(&conn, &occ, &audio_sha256) } else { None };
    Ok(Degravacao {
        trechos: trechos(&segs, desloc, duracao),
        origem: OrigemLegendas { audio_sha256, importado_em: chrono::Utc::now().to_rfc3339(), motor, modelo_sha256: None },
    })
}

#[cfg(test)]
mod testes {
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;

    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::edicao::ffmpeg::exec::{self, Comando};
    use crate::edicao::ffmpeg::sonda::Capacidades;
    use crate::edicao::testes::fontes;
    use crate::models::{AudioEnhancement, AudioMedia, TranscriptSegmentInput};

    fn caso(tmp: &Path, nome: &str) -> PathBuf {
        let entrada = crate::models::NewOccurrenceInput { titulo: Some(format!("Degravação {nome}")), ..Default::default() };
        let pai = tmp.join(format!("caso-{nome}"));
        std::fs::create_dir_all(&pai).unwrap();
        crate::workspace::create::create_workspace(entrada, &pai).unwrap().path
    }

    fn rodar(cap: &Capacidades, pasta: &Path, programa: &Path, nome: &str, args: &[&str]) -> Vec<u8> {
        let cmd = Comando {
            nome: nome.into(),
            programa: programa.to_path_buf(),
            args: args.iter().map(|s| s.to_string()).collect(),
            pasta: pasta.to_path_buf(),
            ambiente: Vec::new(),
            sem_ambiente: Vec::new(),
            stderr: pasta.join(format!("{nome}.stderr")),
        };
        let (fim, saida) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).unwrap();
        exec::exigir_sucesso(&cmd, &fim).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
        saida
    }

    /// (canais, taxa, amostras s16 intercaladas) de um WAV PCM 16 bits.
    fn ler_wav(b: &[u8]) -> (usize, u32, Vec<i16>) {
        let (mut i, mut fmt, mut dados) = (12, None, None);
        while i + 8 <= b.len() {
            let n = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
            let corpo = &b[i + 8..(i + 8 + n).min(b.len())];
            match &b[i..i + 4] {
                b"fmt " => fmt = Some((u16::from_le_bytes([corpo[2], corpo[3]]) as usize, u32::from_le_bytes(corpo[4..8].try_into().unwrap()))),
                b"data" => dados = Some(corpo.chunks_exact(2).map(|x| i16::from_le_bytes([x[0], x[1]])).collect()),
                _ => {}
            }
            i += 8 + n + (n & 1);
        }
        let (canais, taxa) = fmt.expect("fmt");
        (canais, taxa, dados.expect("data"))
    }

    /// Índice (por canal) da primeira amostra do bipe.
    fn bipe(amostras: &[i16], canais: usize) -> usize {
        amostras.iter().position(|x| x.unsigned_abs() > 2000).expect("bipe") / canais
    }

    /// O trecho da degravação marcado no bipe do WAV de análise cai, em tempo de fonte, onde o mesmo bipe está no
    /// vídeo lido com `-copyts` (o tempo do motor): MKV com o som 0,5 s depois da imagem (início do fluxo ≠ do
    /// contêiner) e TS começando em 10 s com AAC (pré-rolagem). O WAV vem do comando do módulo Áudio e também do
    /// ffmpeg escolhido, com os mesmos argumentos: o bipe fica no mesmo índice.
    #[test]
    fn degravacao_em_tempo_de_fonte() {
        let Some(cap) = fontes::cap() else { return };
        let tmp = tempfile::Builder::new().prefix("sicro-p16-").tempdir().unwrap();
        let som = "aevalsrc='if(between(t,1,1.1),sin(2*PI*1000*t),0)':s=48000:d=3";
        let imagem = "testsrc2=s=160x120:r=30:d=4";
        let fontes_: [(&str, Vec<&str>, f64); 2] = [
            ("atraso.mkv", vec!["-f", "lavfi", "-i", imagem, "-itsoffset", "0.5", "-f", "lavfi", "-i", som, "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-c:a", "pcm_s16le"], 0.5),
            ("inicio.ts", vec!["-f", "lavfi", "-i", imagem, "-f", "lavfi", "-i", som, "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-c:a", "aac", "-output_ts_offset", "10", "-f", "mpegts"], 10.0),
        ];
        for (nome, args, inicio) in fontes_ {
            let mut a = vec!["-hide_banner", "-nostdin", "-y", "-loglevel", "error"];
            a.extend(&args);
            a.push(nome);
            rodar(&cap, tmp.path(), &cap.binario, "gerar", &a);
            let ws = caso(tmp.path(), nome);
            let vm = tauri::async_runtime::block_on(crate::commands::video_commands::register_video_media(
                ws.to_string_lossy().into(),
                crate::models::RegisterVideoInput { source_path: tmp.path().join(nome).to_string_lossy().into(), allow_duplicate: false },
            ))
            .unwrap();
            let video = paths::resolver_relativo(&ws, &vm.relative_path);
            // o WAV de análise como o módulo Áudio faz
            let am = tauri::async_runtime::block_on(crate::commands::audio_commands::extract_audio_from_video(
                ws.to_string_lossy().into(),
                video.to_string_lossy().into(),
                Some(vm.sha256.clone()),
            ))
            .unwrap();
            let (canais, taxa, amostras) = ler_wav(&std::fs::read(paths::resolver_relativo(&ws, &am.relative_path)).unwrap());
            let k = bipe(&amostras, canais);
            // o mesmo com o ffmpeg escolhido
            let v = video.to_string_lossy().into_owned();
            rodar(&cap, tmp.path(), &cap.binario, "extrair", &["-hide_banner", "-nostdin", "-y", "-i", &v, "-vn", "-acodec", "pcm_s16le", "-ar", "48000", "outro.wav"]);
            let (c2, _, a2) = ler_wav(&std::fs::read(tmp.path().join("outro.wav")).unwrap());
            assert_eq!(bipe(&a2, c2), k, "{nome}: bipe no WAV do {}", cap.versao);
            // o tempo do motor: -copyts, pts do primeiro quadro de som + índice do bipe
            let cru = rodar(&cap, tmp.path(), &cap.binario, "copyts", &["-hide_banner", "-nostdin", "-copyts", "-i", &v, "-map", "0:a:0", "-af", "ashowinfo", "-f", "s16le", "-"]);
            let log = std::fs::read_to_string(tmp.path().join("copyts.stderr")).unwrap();
            let pts0: f64 = log.lines().find(|l| l.contains("ashowinfo") && l.contains(" n:0 ")).and_then(|l| l.split("pts_time:").nth(1)).and_then(|x| x.split_whitespace().next()).unwrap().parse().unwrap();
            let pcm: Vec<i16> = cru.chunks_exact(2).map(|x| i16::from_le_bytes([x[0], x[1]])).collect();
            let no_motor = pts0 + bipe(&pcm, canais) as f64 / taxa as f64;
            let t_wav = k as f64 / taxa as f64;
            // a degravação: o trecho no bipe e um sem fim
            let (mut conn, occ) = abrir_banco(&ws).unwrap();
            let segs = [
                TranscriptSegmentInput { idx: 0, t_start: t_wav, t_end: Some(t_wav + 0.25), speaker: " Locutor 1 ".into(), text: " Para! ".into(), ai: Some(json!({ "draft": true, "confidence": 0.8 })) },
                TranscriptSegmentInput { idx: 1, t_start: t_wav + 0.5, t_end: None, speaker: String::new(), text: "ok".into(), ai: None },
            ];
            audio_repo::replace_segments(&mut conn, &occ, &am.sha256, &segs).unwrap();
            let d = degravacao(&ws, &vm.sha256).unwrap();
            assert!((d.trechos[0].de - no_motor).abs() < 1e-6, "{nome}: degravação {} × motor {no_motor} ({})", d.trechos[0].de, cap.versao);
            // o deslocamento é o início do fluxo de som, não o do contêiner
            let desloc = d.trechos[0].de - t_wav;
            assert!(desloc >= inicio && desloc < inicio + 1.5, "{nome}: {desloc}");
            if nome.ends_with(".mkv") {
                assert!((desloc - 0.5).abs() < 1e-6, "{nome}: {desloc}");
            }
            assert!((d.trechos[0].ate - d.trechos[0].de - 0.25).abs() < 1e-9);
            assert_eq!((d.trechos[0].texto.as_str(), d.trechos[0].locutor.as_deref(), d.trechos[0].ia), ("Para!", Some("Locutor 1"), true));
            // sem fim e sem seguinte: até o fim do áudio
            let dur = am.duration_s.unwrap();
            assert!((d.trechos[1].ate - (desloc + dur)).abs() < 1e-9 && d.trechos[1].locutor.is_none() && !d.trechos[1].ia);
            assert_eq!(d.origem.audio_sha256, am.sha256);
            assert_eq!((d.origem.motor.as_deref(), d.origem.modelo_sha256.as_deref()), (None, None));
            audio_repo::insert_log(&conn, &occ, Some(&am.sha256), "audio.transcribe", &json!({ "model": "ggml-base.bin", "tool": "whisper-cli", "segments": 2 }).to_string()).unwrap();
            assert_eq!(degravacao(&ws, &vm.sha256).unwrap().origem.motor.as_deref(), Some("whisper-cli ggml-base.bin"));
            // realce do extraído, degravado depois: vale ele, com o deslocamento da origem
            let realce = "d".repeat(64);
            audio_repo::insert_media(&conn, &audio(occ, "realce", &realce, None)).unwrap();
            let e = AudioEnhancement { id: Uuid::new_v4(), occurrence_id: occ, source_audio_sha256: am.sha256.clone(), output_audio_sha256: realce.clone(), filters_json: "{}".into(), created_at: Utc::now() };
            audio_repo::insert_enhancement(&conn, &e).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
            audio_repo::replace_segments(&mut conn, &occ, &realce, &[seg(0, t_wav, Some(t_wav + 0.25), "no realce")]).unwrap();
            let r = degravacao(&ws, &vm.sha256).unwrap();
            assert_eq!((r.origem.audio_sha256.as_str(), r.trechos[0].texto.as_str()), (realce.as_str(), "no realce"));
            assert!((r.trechos[0].de - no_motor).abs() < 1e-6, "{nome}: realce {} × motor {no_motor} ({})", r.trechos[0].de, cap.versao);
            // registro sem os fluxos: o início do som vem do arquivo; sem o arquivo, erro (nunca 0 calado)
            conn.execute("UPDATE video_media SET raw_probe_json = '{}' WHERE sha256 = ?1", [&vm.sha256]).unwrap();
            assert_eq!(degravacao(&ws, &vm.sha256).unwrap().trechos[0].de, r.trechos[0].de, "{nome}: pelo arquivo");
            std::fs::rename(&video, video.with_extension("fora")).unwrap();
            let e = degravacao(&ws, &vm.sha256).unwrap_err().to_string();
            assert!(e.contains(&format!("{}: {}", codigo::MIDIA_AUSENTE, vm.sha256)), "{nome}: {e}");
        }
    }

    fn audio(occ: Uuid, kind: &str, sha: &str, original: Option<&str>) -> AudioMedia {
        let agora = Utc::now();
        AudioMedia {
            id: Uuid::new_v4(),
            occurrence_id: occ,
            kind: kind.into(),
            original_path: None,
            original_relative_path: None,
            relative_path: format!("audio/wav/{sha}.wav"),
            filename: format!("{sha}.wav"),
            sha256: sha.into(),
            original_sha256: original.map(str::to_string),
            source_video_sha256: None,
            size_bytes: 0,
            duration_s: Some(9.0),
            sample_rate: Some(48_000),
            channels: Some(1),
            codec: None,
            bitrate: None,
            raw_probe_json: "{}".into(),
            warnings_json: "[]".into(),
            created_at: agora,
            updated_at: agora,
        }
    }

    fn seg(idx: i64, t: f64, fim: Option<f64>, texto: &str) -> TranscriptSegmentInput {
        TranscriptSegmentInput { idx, t_start: t, t_end: fim, speaker: String::new(), text: texto.into(), ai: None }
    }

    /// Áudio importado (pelo original ou pelo WAV) em tempo 0; realce com o tempo da origem; vale a gravada por
    /// último; erros com código.
    #[test]
    fn importado_realce_e_a_gravada_por_ultimo() {
        let tmp = tempfile::Builder::new().prefix("sicro-p16-").tempdir().unwrap();
        let ws = caso(tmp.path(), "importado");
        let (mut conn, occ) = abrir_banco(&ws).unwrap();
        let (wav, original, realce) = ("a".repeat(64), "b".repeat(64), "c".repeat(64));
        audio_repo::insert_media(&conn, &audio(occ, "importado", &wav, Some(&original))).unwrap();
        audio_repo::insert_media(&conn, &audio(occ, "realce", &realce, None)).unwrap();
        let e = AudioEnhancement { id: Uuid::new_v4(), occurrence_id: occ, source_audio_sha256: wav.clone(), output_audio_sha256: realce.clone(), filters_json: "{}".into(), created_at: Utc::now() };
        audio_repo::insert_enhancement(&conn, &e).unwrap();
        // laço no banco (realce de si mesmo) não prende a leitura
        let laco = AudioEnhancement { id: Uuid::new_v4(), output_audio_sha256: wav.clone(), source_audio_sha256: realce.clone(), ..e };
        audio_repo::insert_enhancement(&conn, &laco).unwrap();
        let ausente = degravacao(&ws, &original).unwrap_err().to_string();
        assert!(ausente.contains(codigo::DEGRAVACAO_AUSENTE), "{ausente}");
        assert!(degravacao(&ws, "../x").unwrap_err().to_string().contains(codigo::SHA256_INVALIDO));
        audio_repo::replace_segments(&mut conn, &occ, &wav, &[seg(0, 2.0, Some(3.0), "no original")]).unwrap();
        for sha in [&original, &wav] {
            let d = degravacao(&ws, sha).unwrap();
            assert_eq!((d.origem.audio_sha256.as_str(), d.trechos[0].de, d.trechos[0].texto.as_str()), (wav.as_str(), 2.0, "no original"));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        audio_repo::replace_segments(&mut conn, &occ, &realce, &[seg(0, 4.0, None, "no realce"), seg(1, 1.0, None, "antes")]).unwrap();
        let d = degravacao(&ws, &original).unwrap();
        assert_eq!(d.origem.audio_sha256, realce);
        // em ordem de início; sem fim: até o seguinte, o último até o fim do áudio (9 s)
        let v: Vec<(f64, f64, &str)> = d.trechos.iter().map(|t| (t.de, t.ate, t.texto.as_str())).collect();
        assert_eq!(v, vec![(1.0, 4.0, "antes"), (4.0, 9.0, "no realce")]);
    }

    #[test]
    fn inicio_do_som_e_o_fluxo_que_a_extracao_leva() {
        let raw = |fluxos: serde_json::Value| json!({ "format": { "start_time": "1.000000" }, "streams": fluxos }).to_string();
        let v = json!({ "codec_type": "video", "start_time": "1.000000" });
        assert_eq!(inicio_do_som(&raw(json!([v]))), None);
        // sem start_time no fluxo: o do contêiner
        assert_eq!(inicio_do_som(&raw(json!([v, { "codec_type": "audio", "channels": 1 }]))), Some(1.0));
        // o de mais canais; empate: o primeiro; o `default` vence os canais
        let a = |c: u32, t: &str, d: u32| json!({ "codec_type": "audio", "channels": c, "start_time": t, "disposition": { "default": d } });
        assert_eq!(inicio_do_som(&raw(json!([v, a(1, "1.2", 0), a(2, "1.5", 0), a(2, "1.7", 0)]))), Some(1.5));
        assert_eq!(inicio_do_som(&raw(json!([a(6, "1.2", 0), a(1, "0.9", 1)]))), Some(0.9));
        assert_eq!(inicio_do_som("{}"), None);
    }

    #[test]
    fn fim_ausente_e_rascunho() {
        let s = |idx: i64, t: f64, fim: Option<f64>, ai: Option<serde_json::Value>| AudioTranscriptSegment {
            id: Uuid::new_v4(),
            occurrence_id: Uuid::new_v4(),
            audio_sha256: String::new(),
            idx,
            t_start: t,
            t_end: fim,
            speaker: String::new(),
            text: "x".into(),
            ai,
            created_at: Utc::now(),
        };
        let v = trechos(
            &[
                s(0, 5.0, None, Some(json!({ "draft": false, "speaker_auto": true }))),
                s(1, 1.0, Some(0.5), Some(json!({ "draft": false }))),
                s(2, 5.0, None, None),
                s(3, f64::NAN, None, None),
            ],
            10.0,
            None,
        );
        // fim gravado vale como está (a validação acusa); dois começando juntos: sem fim e sem seguinte depois = início
        let r: Vec<(f64, f64, bool)> = v.iter().map(|t| (t.de, t.ate, t.ia)).collect();
        assert_eq!(r, vec![(11.0, 10.5, false), (15.0, 15.0, true), (15.0, 15.0, false)]);
    }
}
