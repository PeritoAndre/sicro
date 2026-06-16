//! Tauri commands do Calculador de Velocidade (Fase 3).
//!
//! Superfície:
//!   - create_speed_calibration → resolve a homografia (DLT 4-pts OU linha),
//!     calcula o RMS de reprojeção e persiste a calibração.
//!   - compute_speed            → projeta a trajetória pixel→mundo pela
//!     homografia, ajusta a velocidade (regressão por eixo p/ ≥3 pontos OU
//!     média p/ 2 pontos), roda Monte Carlo quando aplicável e persiste.
//!   - list_speed_calibrations / list_speed_calculations / get_speed_calculation
//!
//! Convenção (idêntica ao `video_commands`): o frontend passa apenas
//! `workspace_path`; o `occurrence_id` vem SEMPRE do Manifest (backend),
//! nunca do frontend.
//!
//! Reprodutibilidade pericial: quando o Monte Carlo roda, a semente
//! (`mc_seed`) e os sigmas (`mc_sigmas`) são SEMPRE persistidos — reabrir o
//! cálculo e reprocessar com a mesma semente reproduz o número exato.

use std::path::PathBuf;

use chrono::Utc;
use nalgebra::Matrix3;
use rusqlite::Connection;
use serde_json::json;
use uuid::Uuid;

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::{occurrence_repo, video_repo, video_speed_repo};
use crate::error::{Result, SicroError};
use crate::models::{
    ComputeSpeedInput, CreateSpeedCalibrationInput, McSigmas, TrajectoryPoint,
    VideoSpeedCalculation, VideoSpeedCalibration,
};
use crate::video::speed::crossratio::{
    fit_1d_projectivity, fit_traffic_line, lift_projectivity_to_homography, project_onto_line,
    CrossRatioReference,
};
use crate::video::speed::homography::{line_calibration, solve_homography_dlt, Homography};
use crate::video::speed::montecarlo::{
    monte_carlo_velocity, monte_carlo_velocity_cross_ratio, MonteCarloConfig,
    MonteCarloCrossRatioConfig,
};
use crate::video::speed::velocity::{average_velocity, regression_velocity};
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};

const ALLOWED_REFERENCE_SOURCES: &[&str] = &["campo", "norma_viaria", "entre_eixos"];
const DEFAULT_MC_ITERATIONS: u32 = 10_000;

// ===========================================================================
// Comandos Tauri (plumbing fino: ws → manifest → conn; lógica nos *_impl)

#[tauri::command]
pub async fn create_speed_calibration(
    workspace_path: String,
    input: CreateSpeedCalibrationInput,
) -> Result<VideoSpeedCalibration> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    create_speed_calibration_impl(&conn, occurrence_id, input)
}

#[tauri::command]
pub async fn compute_speed(
    workspace_path: String,
    input: ComputeSpeedInput,
) -> Result<VideoSpeedCalculation> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    compute_speed_impl(&conn, occurrence_id, input)
}

#[tauri::command]
pub async fn list_speed_calibrations(
    workspace_path: String,
    media_hash: String,
) -> Result<Vec<VideoSpeedCalibration>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    video_speed_repo::list_calibrations_for_media(&conn, &manifest.occurrence_id, &media_hash)
}

#[tauri::command]
pub async fn list_speed_calculations(
    workspace_path: String,
    media_hash: String,
) -> Result<Vec<VideoSpeedCalculation>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    video_speed_repo::list_calculations_for_media(&conn, &manifest.occurrence_id, &media_hash)
}


/// Lista TODOS os cálculos de velocidade da ocorrência (qualquer mídia),
/// mais recentes primeiro. Usado pelo laudo para escolher um cálculo a
/// transcrever na seção de metodologia, sem precisar saber o media_hash.
#[tauri::command]
pub async fn list_speed_calculations_for_occurrence(
    workspace_path: String,
) -> Result<Vec<VideoSpeedCalculation>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    video_speed_repo::list_calculations_for_occurrence(&conn, &manifest.occurrence_id)
}

/// Lê uma calibração pelo id (a calibração referenciada por um cálculo).
#[tauri::command]
pub async fn get_speed_calibration(
    workspace_path: String,
    id: String,
) -> Result<VideoSpeedCalibration> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let uuid =
        Uuid::parse_str(&id).map_err(|e| SicroError::Validation(format!("id inválido: {e}")))?;
    video_speed_repo::find_calibration_by_id(&conn, &uuid)?
        .ok_or_else(|| SicroError::Validation(format!("calibração {id} não encontrada")))
}

// ===========================================================================
// Lógica testável (sem Tauri/workspace)

fn create_speed_calibration_impl(
    conn: &Connection,
    occurrence_id: Uuid,
    input: CreateSpeedCalibrationInput,
) -> Result<VideoSpeedCalibration> {
    let method = input.method.trim().to_ascii_lowercase();
    let reference_source = input.reference_source.trim().to_ascii_lowercase();
    if !ALLOWED_REFERENCE_SOURCES.contains(&reference_source.as_str()) {
        return Err(SicroError::Validation(format!(
            "reference_source desconhecido: '{reference_source}'. Aceitos: {ALLOWED_REFERENCE_SOURCES:?}"
        )));
    }

    // Detalhes do modelo da linha (cross_ratio) p/ o log de auditoria.
    let mut model_log: Option<serde_json::Value> = None;
    let (homography, residuals_px) = match method.as_str() {
        "plane" => {
            if input.control_points.len() != 4 {
                return Err(SicroError::Validation(format!(
                    "calibração 'plane' (DLT) exige exatamente 4 pontos de controle (recebido {})",
                    input.control_points.len()
                )));
            }
            let (image_pts, world_pts) = control_points_as_4(&input.control_points);
            let h = solve_homography_dlt(&image_pts, &world_pts).map_err(|e| {
                SicroError::Validation(format!("falha ao resolver homografia (DLT): {e}"))
            })?;
            let rms = rms_reprojection(&h, &image_pts, &world_pts)?;
            (h, Some(rms))
        }
        "line" => {
            if input.control_points.len() != 2 {
                return Err(SicroError::Validation(format!(
                    "calibração 'line' exige exatamente 2 pontos de controle (recebido {})",
                    input.control_points.len()
                )));
            }
            let p1 = &input.control_points[0];
            let p2 = &input.control_points[1];
            let distance_m = ((p2.world_x_m - p1.world_x_m).powi(2)
                + (p2.world_y_m - p1.world_y_m).powi(2))
            .sqrt();
            let h = line_calibration((p1.px, p1.py), (p2.px, p2.py), distance_m)
                .map_err(|e| SicroError::Validation(format!("falha na calibração por linha: {e}")))?;
            // Resíduo contra os alvos canônicos da linha (0,0)→(distance,0):
            // afim exato em 2 pontos ⇒ ~0; registramos como qualidade do ajuste.
            let img = [(p1.px, p1.py), (p2.px, p2.py)];
            let canon = [(0.0, 0.0), (distance_m, 0.0)];
            let rms = rms_reprojection(&h, &img, &canon)?;
            (h, Some(rms))
        }
        "cross_ratio" => {
            // Razão cruzada: >= 3 pontos de controle COLINEARES sobre o eixo
            // de tráfego. `world_x_m` = posição real ao longo da linha (m);
            // `world_y_m` é ignorado (a referência está sobre a linha).
            if input.control_points.len() < 3 {
                return Err(SicroError::Validation(format!(
                    "calibração 'cross_ratio' exige pelo menos 3 pontos de controle colineares (recebido {})",
                    input.control_points.len()
                )));
            }
            let image_pts: Vec<(f64, f64)> =
                input.control_points.iter().map(|c| (c.px, c.py)).collect();
            let world_scalars: Vec<f64> =
                input.control_points.iter().map(|c| c.world_x_m).collect();
            // Ajusta linha + projetividade 1D explicitamente para registrar o
            // modelo no log; o lift produz a mesma 3×3 de
            // `fit_cross_ratio_homography`.
            let line = fit_traffic_line(&image_pts).map_err(|e| {
                SicroError::Validation(format!("falha ao ajustar a linha de tráfego: {e}"))
            })?;
            let image_scalars: Vec<f64> = image_pts
                .iter()
                .map(|&(px, py)| project_onto_line(&line, px, py))
                .collect();
            let proj = fit_1d_projectivity(&image_scalars, &world_scalars).map_err(|e| {
                SicroError::Validation(format!("falha ao ajustar a projetividade 1D: {e}"))
            })?;
            let h = lift_projectivity_to_homography(&line, &proj);
            // Resíduo: projeta cada referência via a 3×3 e compara a
            // (world_x_m, 0) — em metros, como nos demais modos.
            let world_pts: Vec<(f64, f64)> = world_scalars.iter().map(|&w| (w, 0.0)).collect();
            let rms = rms_reprojection(&h, &image_pts, &world_pts)?;
            model_log = Some(json!({
                "anchor": [line.anchor.0, line.anchor.1],
                "direction": [line.direction.0, line.direction.1],
                "mobius_abcd": [
                    proj.m[(0, 0)], proj.m[(0, 1)], proj.m[(1, 0)], proj.m[(1, 1)],
                ],
            }));
            (h, Some(rms))
        }
        other => {
            return Err(SicroError::Validation(format!(
                "método de calibração desconhecido: '{other}'. Aceitos: 'line', 'plane', 'cross_ratio'."
            )));
        }
    };

    let now = Utc::now();
    let calibration = VideoSpeedCalibration {
        id: Uuid::new_v4(),
        occurrence_id,
        media_hash: input.media_hash.clone(),
        method,
        control_points: input.control_points,
        reference_source,
        homography: homography_to_row_major(&homography),
        residuals_px,
        distortion_model: None,
        author: input.author.unwrap_or_default(),
        created_at: now,
    };
    video_speed_repo::insert_calibration(conn, &calibration)?;
    let _ = video_repo::insert_log(
        conn,
        &occurrence_id,
        Some(&calibration.media_hash),
        "speed.calibration.create",
        &json!({
            "calibration_id": calibration.id.to_string(),
            "method": calibration.method,
            "reference_source": calibration.reference_source,
            "residuals_px": calibration.residuals_px,
            "control_points": calibration.control_points.len(),
            "cross_ratio_model": model_log,
        })
        .to_string(),
    );
    let _ = occurrence_repo::record_audit(
        conn,
        Some(&occurrence_id),
        "video.speed.calibration.created",
        Some("video_speed"),
        Some("video_speed_calibrations"),
        Some(&calibration.id),
        None,
    );
    Ok(calibration)
}

fn compute_speed_impl(
    conn: &Connection,
    occurrence_id: Uuid,
    input: ComputeSpeedInput,
) -> Result<VideoSpeedCalculation> {
    // 1. Carrega calibração e valida pertencimento à ocorrência.
    let calibration = video_speed_repo::find_calibration_by_id(conn, &input.calibration_id)?
        .ok_or_else(|| {
            SicroError::Validation(format!("calibração {} não encontrada", input.calibration_id))
        })?;
    if calibration.occurrence_id != occurrence_id {
        return Err(SicroError::Validation(
            "calibração pertence a outra ocorrência".into(),
        ));
    }

    let n = input.points.len();
    if n < 2 {
        return Err(SicroError::Validation(format!(
            "cálculo de velocidade exige ao menos 2 pontos de trajetória (recebido {n})"
        )));
    }

    // 2. Projeta cada ponto pixel→mundo pela homografia (row-major reconstruída).
    let homography = homography_from_row_major(&calibration.homography);
    let mut world_pts: Vec<(f64, f64)> = Vec::with_capacity(n);
    let mut times: Vec<f64> = Vec::with_capacity(n);
    for (i, p) in input.points.iter().enumerate() {
        let w = homography.project((p.px, p.py)).map_err(|e| {
            SicroError::Validation(format!(
                "ponto {i} (px={}, py={}) projeta fora do plano calibrado: {e}",
                p.px, p.py
            ))
        })?;
        world_pts.push(w);
        times.push(p.actual_timestamp_s);
    }

    // 3. Ressalvas comuns.
    let mut limitations: Vec<String> = vec![
        "Marcação manual da posição do veículo (sem tracking automático).".into(),
        "Estimativa assume movimento aproximadamente retilíneo e uniforme na janela analisada."
            .into(),
        "Possível erro de paralaxe: pontos marcados fora do plano calibrado (ex.: altura do veículo ≠ plano da via) introduzem viés.".into(),
        reference_source_caveat(&calibration.reference_source),
    ];
    if let Some(vfr) = vfr_limitation(conn, &occurrence_id, &calibration.media_hash) {
        limitations.push(vfr);
    }
    if calibration.method == "cross_ratio" {
        limitations.push(
            "Velocidade medida ao longo da linha de referência (modelo 1D por razão cruzada); movimento lateral à linha não é capturado.".into(),
        );
    }

    let confidence_req = input.confidence.unwrap_or(0.95);
    let now = Utc::now();
    let author = input.author.clone().unwrap_or_default();

    // 4. Velocidade por caso (2 pts: média; ≥3: regressão + Monte Carlo).
    let mut calc = if n == 2 {
        limitations.push(
            "2 pontos — sem incerteza estatística (mínimo 3 para regressão e Monte Carlo)."
                .into(),
        );
        let v = average_velocity(world_pts[0], times[0], world_pts[1], times[1])
            .map_err(|e| SicroError::Validation(format!("velocidade média (2 pts): {e}")))?;
        let dt = times[1] - times[0];
        let (vx, vy) = if dt.abs() > 1e-12 {
            (
                (world_pts[1].0 - world_pts[0].0) / dt,
                (world_pts[1].1 - world_pts[0].1) / dt,
            )
        } else {
            (0.0, 0.0)
        };
        VideoSpeedCalculation {
            id: Uuid::new_v4(),
            occurrence_id,
            media_hash: calibration.media_hash.clone(),
            calibration_id: calibration.id,
            points: input.points.clone(),
            velocity_kmh: v.km_per_h,
            vx_m_per_s: vx,
            vy_m_per_s: vy,
            se_m_per_s: None,
            ci_low: None,
            ci_high: None,
            confidence: None,
            r_squared: None,
            residuals: Vec::new(),
            mc_seed: None,
            mc_sigmas: None,
            mc_n: None,
            mc_failed: None,
            mc_mean_kmh: None,
            mc_median_kmh: None,
            mc_p2_5_kmh: None,
            mc_p97_5_kmh: None,
            limitations: Vec::new(),
            audit: serde_json::Value::Null,
            author: author.clone(),
            created_at: now,
        }
    } else {
        let reg = regression_velocity(&world_pts, &times)
            .map_err(|e| SicroError::Validation(format!("regressão de velocidade: {e}")))?;
        // Invariante: um resíduo por ponto.
        if reg.residuals.len() != n {
            return Err(SicroError::Validation(format!(
                "invariante violada: nº de resíduos ({}) != nº de pontos ({n})",
                reg.residuals.len()
            )));
        }
        if (confidence_req - 0.95).abs() > 1e-6 {
            limitations.push(format!(
                "IC calculado a 95% (nível solicitado {confidence_req:.3} não suportado nesta fase)."
            ));
        }
        let (ci_lo_kmh, ci_hi_kmh) = reg.ci95_km_per_h;

        let mut calc = VideoSpeedCalculation {
            id: Uuid::new_v4(),
            occurrence_id,
            media_hash: calibration.media_hash.clone(),
            calibration_id: calibration.id,
            points: input.points.clone(),
            velocity_kmh: reg.velocity.km_per_h,
            vx_m_per_s: reg.vx_m_per_s,
            vy_m_per_s: reg.vy_m_per_s,
            se_m_per_s: Some(reg.se_m_per_s),
            ci_low: Some(ci_lo_kmh),
            ci_high: Some(ci_hi_kmh),
            confidence: Some(0.95),
            r_squared: Some(reg.r_squared),
            residuals: reg.residuals.clone(),
            mc_seed: None,
            mc_sigmas: None,
            mc_n: None,
            mc_failed: None,
            mc_mean_kmh: None,
            mc_median_kmh: None,
            mc_p2_5_kmh: None,
            mc_p97_5_kmh: None,
            limitations: Vec::new(),
            audit: serde_json::Value::Null,
            author: author.clone(),
            created_at: now,
        };

        // Monte Carlo só roda com (1) calibração de plano (4 pontos
        // coplanares) E (2) incertezas informadas pelo perito (≥1 σ > 0).
        // Sem σ NÃO inventamos incerteza: rodar o MC com sigmas zerados
        // produziria um intervalo de largura zero — falsa precisão. Nesse
        // caso o resultado sai SÓ com o IC do ajuste, e a limitação fica
        // registrada (a decisão de informar σ é do perito).
        let sigmas_opt = input.mc_sigmas.clone();
        let has_sigmas = sigmas_opt.as_ref().map_or(false, |s| {
            s.calibration_px > 0.0
                || s.world_m > 0.0
                || s.trajectory_px > 0.0
                || s.time_s > 0.0
        });
        // MC só roda em modos capazes (plano 4-pts OU razão cruzada ≥3-pts).
        let mc_capable = calibration.method == "plane" || calibration.method == "cross_ratio";
        if !mc_capable {
            limitations.push(
                "Monte Carlo indisponível para calibração por linha (2 pontos): requer plano (4 pts coplanares) ou razão cruzada (≥3 pts colineares). Incerteza limitada ao IC do ajuste (regressão).".into(),
            );
        } else if !has_sigmas {
            limitations.push(
                "Monte Carlo não executado: incertezas (σ de marcação, calibração e tempo) não informadas pelo perito. Resultado limitado ao IC do ajuste por regressão.".into(),
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
            // Ramifica por método: razão cruzada re-ajusta linha+projetividade
            // (referências colineares — `solve_homography_dlt` NÃO serve);
            // plano usa o DLT de 4 pontos.
            let mc = if calibration.method == "cross_ratio" {
                let cfg =
                    build_cross_ratio_mc_config(&calibration, &input.points, &sigmas, mc_n, seed)?;
                monte_carlo_velocity_cross_ratio(&cfg).map_err(|e| {
                    SicroError::Validation(format!("Monte Carlo (razão cruzada): {e}"))
                })?
            } else {
                let cfg = build_mc_config(&calibration, &input.points, &sigmas, mc_n, seed)?;
                monte_carlo_velocity(&cfg)
                    .map_err(|e| SicroError::Validation(format!("Monte Carlo: {e}")))?
            };
            calc.mc_seed = Some(seed as i64);
            calc.mc_sigmas = Some(sigmas);
            calc.mc_n = Some(mc_n as i64);
            calc.mc_failed = Some(mc.failed_iterations as i64);
            calc.mc_mean_kmh = Some(mc.mean_km_per_h);
            calc.mc_median_kmh = Some(mc.median_km_per_h);
            calc.mc_p2_5_kmh = Some(mc.p2_5_km_per_h);
            calc.mc_p97_5_kmh = Some(mc.p97_5_km_per_h);
            if mc.failed_iterations > 0 {
                limitations.push(format!(
                    "{} de {} iterações Monte Carlo descartadas (calibração/regressão degenerada sob perturbação).",
                    mc.failed_iterations, mc_n
                ));
            }
        }
        calc
    };

    // 5. Finaliza limitations + audit e persiste.
    let estimator = if n == 2 {
        "average_2pt"
    } else {
        "per_axis_regression"
    };
    calc.audit = json!({
        "estimator": estimator,
        "n_points": n,
        "projection": "homography_row_major_3x3",
        "calibration_method": calibration.method,
        "calibration_id": calibration.id.to_string(),
        "confidence_requested": confidence_req,
        "computed_at": now.to_rfc3339(),
        "tool": "sicro-desktop/video.speed",
    });
    calc.limitations = limitations;

    video_speed_repo::insert_calculation(conn, &calc)?;
    let _ = video_repo::insert_log(
        conn,
        &occurrence_id,
        Some(&calc.media_hash),
        "speed.compute",
        &json!({
            "calculation_id": calc.id.to_string(),
            "calibration_id": calc.calibration_id.to_string(),
            "n_points": n,
            "velocity_kmh": calc.velocity_kmh,
            "mc_seed": calc.mc_seed,
            "mc_n": calc.mc_n,
        })
        .to_string(),
    );
    let _ = occurrence_repo::record_audit(
        conn,
        Some(&occurrence_id),
        "video.speed.computed",
        Some("video_speed"),
        Some("video_speed_calculations"),
        Some(&calc.id),
        None,
    );
    Ok(calc)
}

// ===========================================================================
// helpers

/// Extrai os 4 pares (imagem, mundo) de uma calibração de plano. Pressupõe
/// `control_points.len() == 4` (validado pelo chamador).
fn control_points_as_4(
    cps: &[crate::models::ControlPoint],
) -> ([(f64, f64); 4], [(f64, f64); 4]) {
    let image = [
        (cps[0].px, cps[0].py),
        (cps[1].px, cps[1].py),
        (cps[2].px, cps[2].py),
        (cps[3].px, cps[3].py),
    ];
    let world = [
        (cps[0].world_x_m, cps[0].world_y_m),
        (cps[1].world_x_m, cps[1].world_y_m),
        (cps[2].world_x_m, cps[2].world_y_m),
        (cps[3].world_x_m, cps[3].world_y_m),
    ];
    (image, world)
}

/// RMS de reprojeção (metros) nos pontos de controle.
fn rms_reprojection(
    h: &Homography,
    image_pts: &[(f64, f64)],
    world_pts: &[(f64, f64)],
) -> Result<f64> {
    let n = image_pts.len();
    if n == 0 {
        return Ok(0.0);
    }
    let mut sum_sq = 0.0;
    for i in 0..n {
        let r = h
            .reprojection_residual(image_pts[i], world_pts[i])
            .map_err(|e| SicroError::Validation(format!("reprojeção falhou: {e}")))?;
        sum_sq += r * r;
    }
    Ok((sum_sq / n as f64).sqrt())
}

/// Serializa a matriz 3×3 em ordem ROW-MAJOR (a convenção persistida).
fn homography_to_row_major(h: &Homography) -> [f64; 9] {
    let m = &h.h;
    [
        m[(0, 0)], m[(0, 1)], m[(0, 2)],
        m[(1, 0)], m[(1, 1)], m[(1, 2)],
        m[(2, 0)], m[(2, 1)], m[(2, 2)],
    ]
}

/// Reconstrói a homografia a partir do array row-major persistido.
/// `Matrix3::new` recebe os argumentos em ordem row-major, casando 1:1.
fn homography_from_row_major(a: &[f64; 9]) -> Homography {
    Homography::from_matrix(Matrix3::new(
        a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], a[8],
    ))
}

/// Monta a config do Monte Carlo a partir de uma calibração de plano (4 pts)
/// e da trajetória marcada. Compartilhado entre `compute_speed` e os testes de
/// reprodutibilidade — por isso é determinístico dada a semente.
fn build_mc_config(
    calibration: &VideoSpeedCalibration,
    points: &[TrajectoryPoint],
    sigmas: &McSigmas,
    mc_n: u32,
    seed: u64,
) -> Result<MonteCarloConfig> {
    if calibration.control_points.len() != 4 {
        return Err(SicroError::Validation(
            "Monte Carlo exige calibração de plano com 4 pontos".into(),
        ));
    }
    let (calibration_image_pts, calibration_world_pts) =
        control_points_as_4(&calibration.control_points);
    Ok(MonteCarloConfig {
        calibration_image_pts,
        calibration_world_pts,
        trajectory_image_pts: points.iter().map(|p| (p.px, p.py)).collect(),
        trajectory_times: points.iter().map(|p| p.actual_timestamp_s).collect(),
        sigma_calibration_px: sigmas.calibration_px,
        sigma_world_m: sigmas.world_m,
        sigma_trajectory_px: sigmas.trajectory_px,
        sigma_time_s: sigmas.time_s,
        iterations: mc_n as usize,
        seed: Some(seed),
    })
}

/// Monta a config do Monte Carlo no modo razão cruzada a partir das `>= 3`
/// referências colineares da calibração + a trajetória. Compartilhado entre
/// `compute_speed` e os testes de reprodutibilidade — determinístico dada a
/// semente.
fn build_cross_ratio_mc_config(
    calibration: &VideoSpeedCalibration,
    points: &[TrajectoryPoint],
    sigmas: &McSigmas,
    mc_n: u32,
    seed: u64,
) -> Result<MonteCarloCrossRatioConfig> {
    if calibration.control_points.len() < 3 {
        return Err(SicroError::Validation(
            "Monte Carlo (razão cruzada) exige >= 3 referências colineares".into(),
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
    Ok(MonteCarloCrossRatioConfig {
        references,
        trajectory_image_pts: points.iter().map(|p| (p.px, p.py)).collect(),
        trajectory_times: points.iter().map(|p| p.actual_timestamp_s).collect(),
        sigma_calibration_px: sigmas.calibration_px,
        sigma_world_m: sigmas.world_m,
        sigma_trajectory_px: sigmas.trajectory_px,
        sigma_time_s: sigmas.time_s,
        iterations: mc_n as usize,
        seed: Some(seed),
    })
}

fn reference_source_caveat(source: &str) -> String {
    match source {
        "norma_viaria" => "Calibração por dimensão de norma viária (presumida, não medida em campo) — confirme se a via segue o padrão assumido.".into(),
        "entre_eixos" => "Calibração pela distância entre-eixos do veículo (presumida pela ficha técnica) — confirme o modelo.".into(),
        "campo" => "Calibração por medição em campo.".into(),
        other => format!("Fonte de calibração: {other}."),
    }
}

/// Ressalva de VFR quando a mídia tem `avg_frame_rate` ≠ `r_frame_rate`.
/// Best-effort: se a mídia não estiver registrada, não adiciona ressalva.
fn vfr_limitation(conn: &Connection, occurrence_id: &Uuid, media_hash: &str) -> Option<String> {
    let media = video_repo::find_media_by_sha256(conn, occurrence_id, media_hash).ok()??;
    let avg = media.avg_frame_rate.as_deref()?;
    let r = media.r_frame_rate.as_deref()?;
    if avg != r {
        Some(format!(
            "Taxa de quadros ambígua: avg_frame_rate ({avg}) ≠ r_frame_rate ({r}); possível VFR — timestamps por quadro têm incerteza adicional."
        ))
    } else {
        None
    }
}

// ===========================================================================
// tests

#[cfg(test)]
mod tests {
    use super::{
        build_cross_ratio_mc_config, build_mc_config, compute_speed_impl,
        create_speed_calibration_impl, homography_from_row_major,
    };
    use crate::database::migrations::run_migrations;
    use crate::database::repositories::video_speed_repo;
    use crate::error::Result;
    use crate::models::{
        ComputeSpeedInput, ControlPoint, CreateSpeedCalibrationInput, McSigmas, TrajectoryPoint,
    };
    use crate::video::speed::homography::solve_homography_dlt;
    use crate::video::speed::montecarlo::{
        monte_carlo_velocity, monte_carlo_velocity_cross_ratio,
    };
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

    /// Calibração escala-identidade: 100 px = 1 m. Quadrado 100×100 px → 1×1 m.
    fn identity_scale_calibration_input() -> CreateSpeedCalibrationInput {
        let cp = |px: f64, py: f64, x: f64, y: f64| ControlPoint {
            px,
            py,
            world_x_m: x,
            world_y_m: y,
            label: None,
        };
        CreateSpeedCalibrationInput {
            media_hash: "vid_abc".into(),
            method: "plane".into(),
            control_points: vec![
                cp(0.0, 0.0, 0.0, 0.0),
                cp(100.0, 0.0, 1.0, 0.0),
                cp(100.0, 100.0, 1.0, 1.0),
                cp(0.0, 100.0, 0.0, 1.0),
            ],
            reference_source: "campo".into(),
            author: Some("André".into()),
        }
    }

    /// Trajetória retilínea a `true_v` m/s ao longo de X (100 px/m), `n` pontos.
    fn straight_trajectory(true_v: f64, n: usize, dt: f64) -> Vec<TrajectoryPoint> {
        (0..n)
            .map(|i| {
                let t = i as f64 * dt;
                TrajectoryPoint {
                    storyboard_frame_id: Some(Uuid::new_v4()),
                    export_id: None,
                    px: true_v * t * 100.0, // 100 px/m
                    py: 50.0,
                    u_px: 1.0,
                    actual_timestamp_s: t,
                    delta_s: Some(0.0),
                    manual: true,
                }
            })
            .collect()
    }

    // ----- Cenário sintético para o modo razão cruzada -----
    // Linha em perspectiva na imagem: world (m) → escalar de imagem por uma
    // Möbius conhecida; pontos colocados ao longo de uma direção 2D.
    fn cr_dir() -> (f64, f64) {
        let inv = 1.0 / (10.0_f64).sqrt();
        (3.0 * inv, 1.0 * inv)
    }
    fn cr_s_of_w(w: f64) -> f64 {
        // s(w) = (1·w + 0)/(0.01·w + 1) — perspectiva (c ≠ 0).
        (1.0 * w) / (0.01 * w + 1.0)
    }
    fn cr_image_point(w: f64) -> (f64, f64) {
        let s = cr_s_of_w(w);
        let d = cr_dir();
        (120.0 + s * d.0, 220.0 + s * d.1)
    }
    fn cr_calibration_input() -> CreateSpeedCalibrationInput {
        let refs_w = [0.0, 8.0, 20.0];
        let control_points = refs_w
            .iter()
            .map(|&w| {
                let (px, py) = cr_image_point(w);
                ControlPoint { px, py, world_x_m: w, world_y_m: 0.0, label: None }
            })
            .collect();
        CreateSpeedCalibrationInput {
            media_hash: "vid_cr".into(),
            method: "cross_ratio".into(),
            control_points,
            reference_source: "campo".into(),
            author: Some("André".into()),
        }
    }
    fn cr_trajectory(v: f64, n: usize, dt: f64) -> Vec<TrajectoryPoint> {
        (0..n)
            .map(|i| {
                let t = i as f64 * dt;
                let (px, py) = cr_image_point(v * t);
                TrajectoryPoint {
                    storyboard_frame_id: Some(Uuid::new_v4()),
                    export_id: None,
                    px,
                    py,
                    u_px: 1.0,
                    actual_timestamp_s: t,
                    delta_s: Some(0.0),
                    manual: true,
                }
            })
            .collect()
    }

    /// End-to-end 'cross_ratio': cena 1D em perspectiva conhecida → velocidade
    /// conhecida, exata sem ruído. Reusa a coluna homography_json (sem migração).
    #[test]
    fn cross_ratio_end_to_end_recovers_known_velocity() -> Result<()> {
        let (conn, occ) = setup();
        let cal = create_speed_calibration_impl(&conn, occ, cr_calibration_input())?;
        assert_eq!(cal.method, "cross_ratio");
        // Calibração exata em 3 pontos ⇒ RMS de reprojeção ~0.
        assert!(
            cal.residuals_px.unwrap() < 1e-6,
            "RMS esperado ~0, veio {:?}",
            cal.residuals_px
        );

        let input = ComputeSpeedInput {
            calibration_id: cal.id,
            points: cr_trajectory(15.0, 5, 0.1), // 15 m/s = 54 km/h
            mc_n: None,
            mc_sigmas: None, // foco na velocidade (sem MC)
            confidence: Some(0.95),
            author: None,
        };
        let calc = compute_speed_impl(&conn, occ, input)?;
        assert!(
            (calc.velocity_kmh - 54.0).abs() < 1e-3,
            "velocity_kmh = {}",
            calc.velocity_kmh
        );
        assert!(calc.vy_m_per_s.abs() < 1e-6, "vy = {}", calc.vy_m_per_s);
        assert_eq!(calc.mc_seed, None);
        // Ressalva 1D obrigatória no modo razão cruzada.
        assert!(
            calc.limitations
                .iter()
                .any(|l| l.contains("modelo 1D por razão cruzada")),
            "faltou a ressalva 1D: {:?}",
            calc.limitations
        );
        Ok(())
    }

    /// Reprodutibilidade do MC razão cruzada: a semente gravada, reaplicada no
    /// mesmo cenário, reproduz exatamente o número.
    #[test]
    fn cross_ratio_mc_reproducible() -> Result<()> {
        let (conn, occ) = setup();
        let cal = create_speed_calibration_impl(&conn, occ, cr_calibration_input())?;
        let points = cr_trajectory(15.0, 6, 0.1);
        let sigmas = McSigmas {
            calibration_px: 0.4,
            world_m: 0.02,
            trajectory_px: 0.6,
            time_s: 0.004,
        };
        let mc_n = 400u32;
        let input = ComputeSpeedInput {
            calibration_id: cal.id,
            points: points.clone(),
            mc_n: Some(mc_n),
            mc_sigmas: Some(sigmas.clone()),
            confidence: Some(0.95),
            author: None,
        };
        let calc = compute_speed_impl(&conn, occ, input)?;
        let seed = calc.mc_seed.expect("MC rodou") as u64;
        assert!(calc.mc_p2_5_kmh.is_some());

        // Reprocessa do zero com a MESMA semente gravada.
        let cfg = build_cross_ratio_mc_config(&cal, &points, &sigmas, mc_n, seed)?;
        let replay = monte_carlo_velocity_cross_ratio(&cfg).expect("replay MC");
        assert!(
            (replay.mean_km_per_h - calc.mc_mean_kmh.unwrap()).abs() < 1e-9,
            "MC não reproduzido: replay {} vs gravado {:?}",
            replay.mean_km_per_h,
            calc.mc_mean_kmh
        );
        Ok(())
    }

    /// Calibração 'cross_ratio' com menos de 3 pontos é rejeitada.
    #[test]
    fn cross_ratio_calibration_rejects_fewer_than_three() {
        let (conn, occ) = setup();
        let mut input = cr_calibration_input();
        input.control_points.truncate(2);
        let err = create_speed_calibration_impl(&conn, occ, input);
        assert!(err.is_err(), "esperava rejeição com 2 pontos, veio {err:?}");
    }

    /// End-to-end: calibração identidade-escala + trajetória a 10 m/s
    /// (= 36 km/h) → o cálculo recupera a velocidade esperada, com MC presente.
    #[test]
    fn compute_speed_recovers_known_velocity_end_to_end() -> Result<()> {
        let (conn, occ) = setup();
        let cal = create_speed_calibration_impl(&conn, occ, identity_scale_calibration_input())?;

        let input = ComputeSpeedInput {
            calibration_id: cal.id,
            points: straight_trajectory(10.0, 5, 0.1),
            mc_n: Some(500),
            // σ pequenos NÃO-NULOS: o MC só roda quando o perito informa
            // incertezas (rodar com σ=0 seria falsa precisão).
            mc_sigmas: Some(McSigmas {
                calibration_px: 0.2,
                world_m: 0.0,
                trajectory_px: 0.5,
                time_s: 0.002,
            }),
            confidence: Some(0.95),
            author: None,
        };
        let calc = compute_speed_impl(&conn, occ, input)?;

        // Regressão sobre pontos limpos ⇒ velocidade exata.
        assert!(
            (calc.velocity_kmh - 36.0).abs() < 1e-6,
            "velocity_kmh = {}",
            calc.velocity_kmh
        );
        assert!((calc.vx_m_per_s - 10.0).abs() < 1e-9, "vx = {}", calc.vx_m_per_s);
        assert!(calc.vy_m_per_s.abs() < 1e-9, "vy = {}", calc.vy_m_per_s);
        assert_eq!(calc.confidence, Some(0.95));
        assert_eq!(calc.residuals.len(), 5);
        // MC rodou (calibração de plano + σ informados): seed + sigmas persistidos.
        assert!(calc.mc_seed.is_some());
        assert!(calc.mc_sigmas.is_some());
        assert_eq!(calc.mc_n, Some(500));
        // Ruído pequeno ⇒ média MC próxima de 36 km/h (não exata).
        assert!(
            (calc.mc_mean_kmh.unwrap() - 36.0).abs() < 2.0,
            "mc_mean_kmh = {:?}",
            calc.mc_mean_kmh
        );

        // Persistiu e relê idêntico.
        let back = video_speed_repo::find_calculation_by_id(&conn, &calc.id)?.unwrap();
        assert_eq!(back, calc);
        Ok(())
    }

    /// Sem σ informados, o Monte Carlo NÃO roda (mesmo com calibração de
    /// plano e ≥3 pontos): sai o IC do ajuste e a limitação registrada.
    #[test]
    fn no_sigmas_skips_monte_carlo() -> Result<()> {
        let (conn, occ) = setup();
        let cal = create_speed_calibration_impl(&conn, occ, identity_scale_calibration_input())?;
        let input = ComputeSpeedInput {
            calibration_id: cal.id,
            points: straight_trajectory(10.0, 5, 0.1),
            mc_n: Some(500),
            mc_sigmas: None, // perito não informou incertezas
            confidence: Some(0.95),
            author: None,
        };
        let calc = compute_speed_impl(&conn, occ, input)?;
        // IC do ajuste presente; MC ausente.
        assert!(calc.ci_low.is_some() && calc.ci_high.is_some());
        assert_eq!(calc.mc_seed, None);
        assert_eq!(calc.mc_mean_kmh, None);
        assert!(
            calc.limitations
                .iter()
                .any(|l| l.contains("Monte Carlo não executado")),
            "faltou a limitação de MC não executado: {:?}",
            calc.limitations
        );
        Ok(())
    }

    /// Reprodutibilidade: a semente gravada, reaplicada no mesmo cenário,
    /// reproduz exatamente o número do Monte Carlo persistido.
    #[test]
    fn stored_seed_reproduces_monte_carlo_number() -> Result<()> {
        let (conn, occ) = setup();
        let cal = create_speed_calibration_impl(&conn, occ, identity_scale_calibration_input())?;

        let points = straight_trajectory(20.0, 8, 0.1);
        let sigmas = McSigmas {
            calibration_px: 0.5,
            world_m: 0.02,
            trajectory_px: 1.0,
            time_s: 0.005,
        };
        let mc_n = 500u32;
        let input = ComputeSpeedInput {
            calibration_id: cal.id,
            points: points.clone(),
            mc_n: Some(mc_n),
            mc_sigmas: Some(sigmas.clone()),
            confidence: Some(0.95),
            author: None,
        };
        let calc = compute_speed_impl(&conn, occ, input)?;
        let stored_seed = calc.mc_seed.expect("MC rodou") as u64;

        // Reprocessa do zero com a MESMA semente gravada.
        let cfg = build_mc_config(&cal, &points, &sigmas, mc_n, stored_seed)?;
        let replay = monte_carlo_velocity(&cfg).expect("replay MC");

        assert!(
            (replay.mean_km_per_h - calc.mc_mean_kmh.unwrap()).abs() < 1e-9,
            "replay {} != gravado {:?}",
            replay.mean_km_per_h,
            calc.mc_mean_kmh
        );
        assert!(
            (replay.p2_5_km_per_h - calc.mc_p2_5_kmh.unwrap()).abs() < 1e-9
                && (replay.p97_5_km_per_h - calc.mc_p97_5_kmh.unwrap()).abs() < 1e-9,
            "percentis não reproduzidos"
        );
        Ok(())
    }

    /// 2 pontos → média, SEM IC e SEM Monte Carlo (campos nulos), com a
    /// ressalva explícita.
    #[test]
    fn two_points_yields_average_without_ci_or_mc() -> Result<()> {
        let (conn, occ) = setup();
        let cal = create_speed_calibration_impl(&conn, occ, identity_scale_calibration_input())?;
        let input = ComputeSpeedInput {
            calibration_id: cal.id,
            points: straight_trajectory(10.0, 2, 0.1),
            mc_n: Some(500),
            mc_sigmas: None,
            confidence: None,
            author: None,
        };
        let calc = compute_speed_impl(&conn, occ, input)?;
        assert!((calc.velocity_kmh - 36.0).abs() < 1e-6, "v = {}", calc.velocity_kmh);
        assert_eq!(calc.se_m_per_s, None);
        assert_eq!(calc.ci_low, None);
        assert_eq!(calc.ci_high, None);
        assert_eq!(calc.confidence, None);
        assert_eq!(calc.mc_seed, None);
        assert_eq!(calc.mc_sigmas, None);
        assert!(calc.residuals.is_empty());
        assert!(
            calc.limitations.iter().any(|l| l.contains("2 pontos")),
            "faltou a ressalva de 2 pontos: {:?}",
            calc.limitations
        );
        let back = video_speed_repo::find_calculation_by_id(&conn, &calc.id)?.unwrap();
        assert_eq!(back, calc);
        Ok(())
    }

    /// Calibração 'plane' com nº errado de pontos é rejeitada.
    #[test]
    fn plane_calibration_rejects_wrong_point_count() {
        let (conn, occ) = setup();
        let mut input = identity_scale_calibration_input();
        input.control_points.pop(); // 3 pontos
        let err = create_speed_calibration_impl(&conn, occ, input);
        assert!(err.is_err(), "esperava rejeição, veio {err:?}");
    }

    /// Cross-módulo: homografia resolvida pelo solver → persistida → relida →
    /// usada na projeção dá AS MESMAS coordenadas de mundo. Valida a convenção
    /// row-major ponta a ponta (Matrix3 column-major ↔ [f64;9] row-major).
    #[test]
    fn homography_row_major_round_trips_through_db() -> Result<()> {
        let (conn, occ) = setup();

        // Quadrilátero em perspectiva → retângulo 4×3 m.
        let image = [
            (100.0, 200.0),
            (300.0, 200.0),
            (350.0, 100.0),
            (50.0, 100.0),
        ];
        let world = [(0.0, 0.0), (4.0, 0.0), (4.0, 3.0), (0.0, 3.0)];
        let h_ref = solve_homography_dlt(&image, &world).unwrap();

        let cp = |i: usize| ControlPoint {
            px: image[i].0,
            py: image[i].1,
            world_x_m: world[i].0,
            world_y_m: world[i].1,
            label: None,
        };
        let input = CreateSpeedCalibrationInput {
            media_hash: "vid_persp".into(),
            method: "plane".into(),
            control_points: vec![cp(0), cp(1), cp(2), cp(3)],
            reference_source: "campo".into(),
            author: None,
        };
        let cal = create_speed_calibration_impl(&conn, occ, input)?;

        // Relê e reconstrói a homografia a partir do array persistido.
        let reloaded = video_speed_repo::find_calibration_by_id(&conn, &cal.id)?.unwrap();
        let h_db = homography_from_row_major(&reloaded.homography);

        // Projeções idênticas (inclui ponto interior não usado no ajuste).
        for test_px in [
            (100.0, 200.0),
            (300.0, 200.0),
            (200.0, 150.0),
            (175.0, 175.0),
        ] {
            let a = h_ref.project(test_px).unwrap();
            let b = h_db.project(test_px).unwrap();
            assert!(
                (a.0 - b.0).abs() < 1e-12 && (a.1 - b.1).abs() < 1e-12,
                "projeção divergiu em {test_px:?}: ref={a:?} db={b:?}"
            );
        }
        Ok(())
    }
}
