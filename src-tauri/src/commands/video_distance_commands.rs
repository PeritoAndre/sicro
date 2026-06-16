//! Tauri commands da Medição de Distância por fotogrametria.
//!
//! Superfície:
//!   - create_distance_measurement → carrega uma calibração já existente,
//!     projeta os 2 pontos pixel→mundo pela MESMA homografia da velocidade,
//!     calcula a distância pontual e (se o perito informou σ) roda o Monte
//!     Carlo. Persiste.
//!   - list_distance_measurements / get_distance_measurement
//!
//! Convenção (idêntica ao `video_speed_commands`): o frontend passa apenas
//! `workspace_path`; o `occurrence_id` vem SEMPRE do Manifest (backend).
//!
//! Reprodutibilidade pericial: quando o Monte Carlo roda, `mc_seed` e
//! `mc_sigmas` são SEMPRE persistidos.
//!
//! Diferença para a velocidade: distância de 2 pontos NÃO tem IC de regressão.
//! A ÚNICA incerteza é o Monte Carlo — sem σ, sai só a distância pontual.

use std::path::PathBuf;

use chrono::Utc;
use nalgebra::Matrix3;
use rusqlite::Connection;
use serde_json::json;
use uuid::Uuid;

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::{
    occurrence_repo, video_distance_repo, video_repo, video_speed_repo,
};
use crate::error::{Result, SicroError};
use crate::models::{
    CreateDistanceMeasurementInput, McSigmasDistance, VideoDistanceMeasurement,
    VideoSpeedCalibration,
};
use crate::video::measure::distance::world_distance;
use crate::video::measure::montecarlo::{
    monte_carlo_distance, monte_carlo_distance_cross_ratio, MonteCarloCrossRatioDistanceConfig,
    MonteCarloDistanceConfig,
};
use crate::video::speed::crossratio::CrossRatioReference;
use crate::video::speed::homography::Homography;
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};

const DEFAULT_MC_ITERATIONS: u32 = 10_000;

// ===========================================================================
// Comandos Tauri (plumbing fino: ws → manifest → conn; lógica nos *_impl)

#[tauri::command]
pub async fn create_distance_measurement(
    workspace_path: String,
    input: CreateDistanceMeasurementInput,
) -> Result<VideoDistanceMeasurement> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    create_distance_measurement_impl(&conn, occurrence_id, input)
}

#[tauri::command]
pub async fn list_distance_measurements(
    workspace_path: String,
    media_hash: String,
) -> Result<Vec<VideoDistanceMeasurement>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    video_distance_repo::list_measurements_for_media(&conn, &manifest.occurrence_id, &media_hash)
}

#[tauri::command]
pub async fn get_distance_measurement(
    workspace_path: String,
    id: String,
) -> Result<VideoDistanceMeasurement> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let uuid =
        Uuid::parse_str(&id).map_err(|e| SicroError::Validation(format!("id inválido: {e}")))?;
    video_distance_repo::find_measurement_by_id(&conn, &uuid)?
        .ok_or_else(|| SicroError::Validation(format!("medição {id} não encontrada")))
}

/// Lista TODAS as medições de distância da ocorrência (qualquer mídia), mais
/// recentes primeiro. Usado pelo laudo para escolher uma medição a transcrever
/// na seção de metodologia, sem precisar saber o media_hash.
#[tauri::command]
pub async fn list_distance_measurements_for_occurrence(
    workspace_path: String,
) -> Result<Vec<VideoDistanceMeasurement>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    video_distance_repo::list_measurements_for_occurrence(&conn, &manifest.occurrence_id)
}

// ===========================================================================
// Lógica testável (sem Tauri/workspace)

fn create_distance_measurement_impl(
    conn: &Connection,
    occurrence_id: Uuid,
    input: CreateDistanceMeasurementInput,
) -> Result<VideoDistanceMeasurement> {
    // 1. Carrega a calibração e valida o pertencimento à ocorrência.
    let calibration = video_speed_repo::find_calibration_by_id(conn, &input.calibration_id)?
        .ok_or_else(|| {
            SicroError::Validation(format!("calibração {} não encontrada", input.calibration_id))
        })?;
    if calibration.occurrence_id != occurrence_id {
        return Err(SicroError::Validation(
            "calibração pertence a outra ocorrência".into(),
        ));
    }

    // 2. Reconstrói a homografia e calcula a distância pontual (m).
    let homography = homography_from_row_major(&calibration.homography);
    let p1 = (input.p1_px, input.p1_py);
    let p2 = (input.p2_px, input.p2_py);
    let distance_m = world_distance(&homography, p1, p2).map_err(|e| {
        SicroError::Validation(format!(
            "ponto medido projeta fora do plano calibrado: {e}"
        ))
    })?;

    // 3. Ressalvas comuns (o perito transcreve no laudo).
    let mut limitations: Vec<String> = vec![
        "Marcação manual dos dois pontos (sem detecção automática).".into(),
        "Possível erro de paralaxe: os pontos devem estar SOBRE o plano calibrado (ex.: no solo); pontos fora do plano introduzem viés.".into(),
        reference_source_caveat(&calibration.reference_source),
    ];
    if calibration.method == "cross_ratio" {
        limitations.push(
            "Distância medida ao longo da linha de referência (modelo 1D por razão cruzada); separação lateral à linha não é capturada.".into(),
        );
    }

    let now = Utc::now();
    let author = input.author.clone().unwrap_or_default();

    let mut measurement = VideoDistanceMeasurement {
        id: Uuid::new_v4(),
        occurrence_id,
        media_hash: calibration.media_hash.clone(),
        calibration_id: calibration.id,
        p1_px: input.p1_px,
        p1_py: input.p1_py,
        p2_px: input.p2_px,
        p2_py: input.p2_py,
        distance_m,
        mc_seed: None,
        mc_sigmas: None,
        mc_n: None,
        mc_failed: None,
        mc_mean_m: None,
        mc_median_m: None,
        mc_p2_5_m: None,
        mc_p97_5_m: None,
        limitations: Vec::new(),
        audit: serde_json::Value::Null,
        author: author.clone(),
        created_at: now,
    };

    // 4. Monte Carlo method-aware, com o MESMO gate de σ>0 da velocidade. Sem σ
    // NÃO inventamos incerteza: sai só a distância pontual + a ressalva.
    let sigmas_opt = input.mc_sigmas.clone();
    let has_sigmas = sigmas_opt.as_ref().map_or(false, |s| {
        s.calibration_px > 0.0 || s.world_m > 0.0 || s.measure_px > 0.0
    });
    let mc_capable = calibration.method == "plane" || calibration.method == "cross_ratio";
    if !mc_capable {
        limitations.push(
            "Monte Carlo indisponível para calibração por linha (2 pontos): requer plano (4 pts coplanares) ou razão cruzada (≥3 pts colineares). Distância sem intervalo de incerteza.".into(),
        );
    } else if !has_sigmas {
        limitations.push(
            "Sem incerteza: σ (de marcação, calibração e medição) não informados pelo perito — resultado limitado à distância pontual, sem intervalo Monte Carlo.".into(),
        );
    } else {
        let sigmas = sigmas_opt.unwrap();
        let mc_n = input.mc_n.unwrap_or(DEFAULT_MC_ITERATIONS);
        if mc_n < 10 {
            return Err(SicroError::Validation(format!(
                "mc_n precisa ser >= 10 para o Monte Carlo (recebido {mc_n})"
            )));
        }
        // Reprodutibilidade: semente explícita, persistida como i64.
        let seed: u64 = rand::random();
        let mc = if calibration.method == "cross_ratio" {
            let cfg = build_cross_ratio_distance_mc_config(&calibration, p1, p2, &sigmas, mc_n, seed)?;
            monte_carlo_distance_cross_ratio(&cfg).map_err(|e| {
                SicroError::Validation(format!("Monte Carlo de distância (razão cruzada): {e}"))
            })?
        } else {
            let cfg = build_distance_mc_config(&calibration, p1, p2, &sigmas, mc_n, seed)?;
            monte_carlo_distance(&cfg)
                .map_err(|e| SicroError::Validation(format!("Monte Carlo de distância: {e}")))?
        };
        measurement.mc_seed = Some(seed as i64);
        measurement.mc_sigmas = Some(sigmas);
        measurement.mc_n = Some(mc_n as i64);
        measurement.mc_failed = Some(mc.failed_iterations as i64);
        measurement.mc_mean_m = Some(mc.mean_m);
        measurement.mc_median_m = Some(mc.median_m);
        measurement.mc_p2_5_m = Some(mc.p2_5_m);
        measurement.mc_p97_5_m = Some(mc.p97_5_m);
        if mc.failed_iterations > 0 {
            limitations.push(format!(
                "{} de {} iterações Monte Carlo descartadas (calibração degenerada sob perturbação).",
                mc.failed_iterations, mc_n
            ));
        }
    }

    // 5. Finaliza audit + limitations e persiste.
    measurement.audit = json!({
        "estimator": "homography_point_distance",
        "projection": "homography_row_major_3x3",
        "calibration_method": calibration.method,
        "calibration_id": calibration.id.to_string(),
        "computed_at": now.to_rfc3339(),
        "tool": "sicro-desktop/video.measure.distance",
    });
    measurement.limitations = limitations;

    video_distance_repo::insert_measurement(conn, &measurement)?;
    let _ = video_repo::insert_log(
        conn,
        &occurrence_id,
        Some(&measurement.media_hash),
        "measure.distance.create",
        &json!({
            "measurement_id": measurement.id.to_string(),
            "calibration_id": measurement.calibration_id.to_string(),
            "distance_m": measurement.distance_m,
            "mc_seed": measurement.mc_seed,
            "mc_n": measurement.mc_n,
        })
        .to_string(),
    );
    let _ = occurrence_repo::record_audit(
        conn,
        Some(&occurrence_id),
        "video.measure.distance.created",
        Some("video_measure"),
        Some("video_distance_measurements"),
        Some(&measurement.id),
        None,
    );
    Ok(measurement)
}

// ===========================================================================
// helpers

/// Reconstrói a homografia a partir do array row-major persistido (mesma
/// convenção do `video_speed_commands`: `Matrix3::new` é row-major).
fn homography_from_row_major(a: &[f64; 9]) -> Homography {
    Homography::from_matrix(Matrix3::new(
        a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], a[8],
    ))
}

/// Monta a config do Monte Carlo de distância (modo plano) a partir da
/// calibração de 4 pontos + os 2 pontos medidos. Compartilhado entre o comando
/// e os testes de reprodutibilidade — determinístico dada a semente.
fn build_distance_mc_config(
    calibration: &VideoSpeedCalibration,
    p1: (f64, f64),
    p2: (f64, f64),
    sigmas: &McSigmasDistance,
    mc_n: u32,
    seed: u64,
) -> Result<MonteCarloDistanceConfig> {
    let cps = &calibration.control_points;
    if cps.len() != 4 {
        return Err(SicroError::Validation(
            "Monte Carlo de distância (plano) exige calibração com 4 pontos".into(),
        ));
    }
    let calibration_image_pts = [
        (cps[0].px, cps[0].py),
        (cps[1].px, cps[1].py),
        (cps[2].px, cps[2].py),
        (cps[3].px, cps[3].py),
    ];
    let calibration_world_pts = [
        (cps[0].world_x_m, cps[0].world_y_m),
        (cps[1].world_x_m, cps[1].world_y_m),
        (cps[2].world_x_m, cps[2].world_y_m),
        (cps[3].world_x_m, cps[3].world_y_m),
    ];
    Ok(MonteCarloDistanceConfig {
        calibration_image_pts,
        calibration_world_pts,
        p1_px: p1,
        p2_px: p2,
        sigma_calibration_px: sigmas.calibration_px,
        sigma_world_m: sigmas.world_m,
        sigma_measure_px: sigmas.measure_px,
        iterations: mc_n as usize,
        seed: Some(seed),
    })
}

/// Monta a config do Monte Carlo de distância (modo razão cruzada) a partir das
/// `>= 3` referências colineares + os 2 pontos medidos.
fn build_cross_ratio_distance_mc_config(
    calibration: &VideoSpeedCalibration,
    p1: (f64, f64),
    p2: (f64, f64),
    sigmas: &McSigmasDistance,
    mc_n: u32,
    seed: u64,
) -> Result<MonteCarloCrossRatioDistanceConfig> {
    if calibration.control_points.len() < 3 {
        return Err(SicroError::Validation(
            "Monte Carlo de distância (razão cruzada) exige >= 3 referências colineares".into(),
        ));
    }
    let references: Vec<CrossRatioReference> = calibration
        .control_points
        .iter()
        .map(|c| CrossRatioReference {
            px: c.px,
            py: c.py,
            world_m: c.world_x_m,
        })
        .collect();
    Ok(MonteCarloCrossRatioDistanceConfig {
        references,
        p1_px: p1,
        p2_px: p2,
        sigma_calibration_px: sigmas.calibration_px,
        sigma_world_m: sigmas.world_m,
        sigma_measure_px: sigmas.measure_px,
        iterations: mc_n as usize,
        seed: Some(seed),
    })
}

/// Ressalva da fonte de calibração (igual à velocidade — a calibração é a
/// mesma geometria de cena).
fn reference_source_caveat(source: &str) -> String {
    match source {
        "norma_viaria" => "Calibração por dimensão de norma viária (presumida, não medida em campo) — confirme se a via segue o padrão assumido.".into(),
        "entre_eixos" => "Calibração pela distância entre-eixos do veículo (presumida pela ficha técnica) — confirme o modelo.".into(),
        "campo" => "Calibração por medição em campo.".into(),
        other => format!("Fonte de calibração: {other}."),
    }
}

// ===========================================================================
// tests

#[cfg(test)]
mod tests {
    use super::{
        build_distance_mc_config, create_distance_measurement_impl, homography_from_row_major,
    };
    use crate::database::migrations::run_migrations;
    use crate::database::repositories::{video_distance_repo, video_speed_repo};
    use crate::error::Result;
    use crate::models::{
        ControlPoint, CreateDistanceMeasurementInput, McSigmasDistance, VideoSpeedCalibration,
    };
    use crate::video::measure::montecarlo::monte_carlo_distance;
    use chrono::{DateTime, Utc};
    use rusqlite::{params, Connection};
    use uuid::Uuid;

    fn setup() -> (Connection, Uuid) {
        let mut conn = Connection::open_in_memory().expect("open in-memory");
        conn.pragma_update(None, "foreign_keys", "ON")
            .expect("enable fk");
        run_migrations(&mut conn).expect("run migrations");
        let occ = Uuid::new_v4();
        conn.execute(
            "INSERT INTO occurrences (id, created_at, updated_at) VALUES (?1, ?2, ?2)",
            params![occ.to_string(), "2026-05-31T12:00:00Z"],
        )
        .expect("seed occurrence");
        (conn, occ)
    }

    /// Calibração identidade-escala 100 px = 1 m, gravada direto pelo repo
    /// (a geometria já existe; a distância só a consome).
    fn insert_identity_calibration(conn: &Connection, occ: Uuid) -> VideoSpeedCalibration {
        let cp = |px: f64, py: f64, x: f64, y: f64| ControlPoint {
            px,
            py,
            world_x_m: x,
            world_y_m: y,
            label: None,
        };
        let cal = VideoSpeedCalibration {
            id: Uuid::new_v4(),
            occurrence_id: occ,
            media_hash: "vid_dist".into(),
            method: "plane".into(),
            control_points: vec![
                cp(0.0, 0.0, 0.0, 0.0),
                cp(100.0, 0.0, 1.0, 0.0),
                cp(100.0, 100.0, 1.0, 1.0),
                cp(0.0, 100.0, 0.0, 1.0),
            ],
            reference_source: "campo".into(),
            homography: [0.01, 0.0, 0.0, 0.0, 0.01, 0.0, 0.0, 0.0, 1.0],
            residuals_px: Some(0.0),
            distortion_model: None,
            author: "André".into(),
            created_at: DateTime::parse_from_rfc3339("2026-05-31T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };
        video_speed_repo::insert_calibration(conn, &cal).expect("insert calibration");
        cal
    }

    /// Sanidade do helper de reconstrução: (100,500)→(1,5), (600,500)→(6,5).
    #[test]
    fn homography_reconstruction_projects_identity_scale() {
        let h = homography_from_row_major(&[0.01, 0.0, 0.0, 0.0, 0.01, 0.0, 0.0, 0.0, 1.0]);
        let a = h.project((100.0, 500.0)).unwrap();
        assert!((a.0 - 1.0).abs() < 1e-12 && (a.1 - 5.0).abs() < 1e-12);
    }

    /// End-to-end (plano): dois pontos a 5 m → distância 5 m, com MC sob σ>0.
    #[test]
    fn distance_end_to_end_with_mc() -> Result<()> {
        let (conn, occ) = setup();
        let cal = insert_identity_calibration(&conn, occ);

        let input = CreateDistanceMeasurementInput {
            calibration_id: cal.id,
            p1_px: 100.0, // (1 m, 5 m)
            p1_py: 500.0,
            p2_px: 600.0, // (6 m, 5 m)
            p2_py: 500.0,
            mc_n: Some(500),
            mc_sigmas: Some(McSigmasDistance {
                calibration_px: 0.3,
                world_m: 0.0,
                measure_px: 0.5,
            }),
            author: None,
        };
        let m = create_distance_measurement_impl(&conn, occ, input)?;

        assert!((m.distance_m - 5.0).abs() < 1e-9, "distance = {}", m.distance_m);
        // MC rodou (plano + σ informados): seed + sigmas persistidos.
        assert!(m.mc_seed.is_some());
        assert!(m.mc_sigmas.is_some());
        assert_eq!(m.mc_n, Some(500));
        assert!(
            (m.mc_mean_m.unwrap() - 5.0).abs() < 0.5,
            "mc_mean_m = {:?}",
            m.mc_mean_m
        );
        assert!(m.mc_p2_5_m.unwrap() <= m.mc_p97_5_m.unwrap());

        // Persistiu e relê idêntico.
        let back = video_distance_repo::find_measurement_by_id(&conn, &m.id)?.unwrap();
        assert_eq!(back, m);
        Ok(())
    }

    /// Sem σ: distância pontual + bloco mc_* nulo + ressalva registrada.
    #[test]
    fn distance_without_sigmas_is_pointwise_only() -> Result<()> {
        let (conn, occ) = setup();
        let cal = insert_identity_calibration(&conn, occ);

        let input = CreateDistanceMeasurementInput {
            calibration_id: cal.id,
            p1_px: 100.0,
            p1_py: 500.0,
            p2_px: 600.0,
            p2_py: 500.0,
            mc_n: Some(500),
            mc_sigmas: None, // perito não informou incertezas
            author: None,
        };
        let m = create_distance_measurement_impl(&conn, occ, input)?;
        assert!((m.distance_m - 5.0).abs() < 1e-9);
        assert_eq!(m.mc_seed, None);
        assert_eq!(m.mc_sigmas, None);
        assert_eq!(m.mc_mean_m, None);
        assert_eq!(m.mc_p97_5_m, None);
        assert!(
            m.limitations.iter().any(|l| l.contains("Sem incerteza")),
            "faltou a ressalva de sem-σ: {:?}",
            m.limitations
        );
        let back = video_distance_repo::find_measurement_by_id(&conn, &m.id)?.unwrap();
        assert_eq!(back, m);
        Ok(())
    }

    /// Reprodutibilidade: a semente gravada, reaplicada, reproduz o número.
    #[test]
    fn stored_seed_reproduces_monte_carlo_distance() -> Result<()> {
        let (conn, occ) = setup();
        let cal = insert_identity_calibration(&conn, occ);
        let sigmas = McSigmasDistance {
            calibration_px: 0.5,
            world_m: 0.02,
            measure_px: 1.0,
        };
        let mc_n = 500u32;
        let input = CreateDistanceMeasurementInput {
            calibration_id: cal.id,
            p1_px: 100.0,
            p1_py: 500.0,
            p2_px: 600.0,
            p2_py: 500.0,
            mc_n: Some(mc_n),
            mc_sigmas: Some(sigmas.clone()),
            author: None,
        };
        let m = create_distance_measurement_impl(&conn, occ, input)?;
        let stored_seed = m.mc_seed.expect("MC rodou") as u64;

        // Reprocessa do zero com a MESMA semente gravada.
        let cfg = build_distance_mc_config(
            &cal,
            (m.p1_px, m.p1_py),
            (m.p2_px, m.p2_py),
            &sigmas,
            mc_n,
            stored_seed,
        )?;
        let replay = monte_carlo_distance(&cfg).expect("replay MC");
        assert!(
            (replay.mean_m - m.mc_mean_m.unwrap()).abs() < 1e-9,
            "replay {} != gravado {:?}",
            replay.mean_m,
            m.mc_mean_m
        );
        assert!(
            (replay.p2_5_m - m.mc_p2_5_m.unwrap()).abs() < 1e-9
                && (replay.p97_5_m - m.mc_p97_5_m.unwrap()).abs() < 1e-9,
            "percentis não reproduzidos"
        );
        Ok(())
    }

    /// Calibração de outra ocorrência é rejeitada (occurrence_id vem do Manifest).
    #[test]
    fn rejects_calibration_from_other_occurrence() {
        let (conn, occ) = setup();
        let cal = insert_identity_calibration(&conn, occ);

        let other_occ = Uuid::new_v4();
        let input = CreateDistanceMeasurementInput {
            calibration_id: cal.id,
            p1_px: 100.0,
            p1_py: 500.0,
            p2_px: 600.0,
            p2_py: 500.0,
            mc_n: None,
            mc_sigmas: None,
            author: None,
        };
        let err = create_distance_measurement_impl(&conn, other_occ, input);
        assert!(
            err.is_err(),
            "esperava rejeição por ocorrência divergente, veio {err:?}"
        );
    }
}
