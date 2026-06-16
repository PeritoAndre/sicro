//! Repositório do Calculador de Velocidade (Fase 2 — camada de dados).
//!
//! Duas tabelas: `video_speed_calibrations` e `video_speed_calculations`.
//! Diferente do `video_repo` (que guarda JSON como `String` opaca), aqui o
//! repositório **(de)serializa** as colunas `*_json` para tipos estruturados
//! (matriz da homografia, pontos da trajetória, sigmas do Monte Carlo). Isso é
//! deliberado: o laudo precisa reabrir um cálculo e reproduzir o número exato,
//! então a forma estruturada — e não um blob opaco — é o que cruza o limite.
//!
//! Sem comandos Tauri e sem UI nesta fase: apenas insert / get / list.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::error::Result;
use crate::models::{McSigmas, VideoSpeedCalculation, VideoSpeedCalibration};

// ---------------------------------------------------------------------------
// video_speed_calibrations

const CALIBRATION_COLS: &str = "
    id, occurrence_id, media_hash, method, control_points_json,
    reference_source, homography_json, residuals_px, distortion_model_json,
    author, created_at
";

pub fn insert_calibration(conn: &Connection, c: &VideoSpeedCalibration) -> Result<()> {
    let control_points_json = serde_json::to_string(&c.control_points)?;
    let homography_json = serde_json::to_string(&c.homography)?;
    let distortion_model_json = c
        .distortion_model
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    conn.execute(
        &format!(
            "INSERT INTO video_speed_calibrations ({CALIBRATION_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)"
        ),
        params![
            c.id.to_string(),
            c.occurrence_id.to_string(),
            c.media_hash,
            c.method,
            control_points_json,
            c.reference_source,
            homography_json,
            c.residuals_px,
            distortion_model_json,
            c.author,
            c.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn find_calibration_by_id(
    conn: &Connection,
    id: &Uuid,
) -> Result<Option<VideoSpeedCalibration>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CALIBRATION_COLS} FROM video_speed_calibrations WHERE id = ?1"
    ))?;
    let row = stmt
        .query_row([id.to_string()], row_to_calibration)
        .optional()?;
    Ok(row)
}

pub fn list_calibrations_for_media(
    conn: &Connection,
    occurrence_id: &Uuid,
    media_hash: &str,
) -> Result<Vec<VideoSpeedCalibration>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CALIBRATION_COLS} FROM video_speed_calibrations
         WHERE occurrence_id = ?1 AND media_hash = ?2
         ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map(
            [occurrence_id.to_string(), media_hash.to_string()],
            row_to_calibration,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn row_to_calibration(row: &Row<'_>) -> rusqlite::Result<VideoSpeedCalibration> {
    let distortion_model = row
        .get::<_, Option<String>>("distortion_model_json")?
        .map(parse_json::<serde_json::Value>)
        .transpose()?;

    Ok(VideoSpeedCalibration {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        media_hash: row.get("media_hash")?,
        method: row.get("method")?,
        control_points: parse_json(row.get::<_, String>("control_points_json")?)?,
        reference_source: row.get("reference_source")?,
        homography: parse_json::<[f64; 9]>(row.get::<_, String>("homography_json")?)?,
        residuals_px: row.get("residuals_px")?,
        distortion_model,
        author: row.get("author")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

// ---------------------------------------------------------------------------
// video_speed_calculations

const CALCULATION_COLS: &str = "
    id, occurrence_id, media_hash, calibration_id, points_json,
    velocity_kmh, vx_m_per_s, vy_m_per_s, se_m_per_s, ci_low, ci_high,
    confidence, r_squared, residuals_json, mc_seed, mc_sigmas_json,
    mc_n, mc_failed, mc_mean_kmh, mc_median_kmh, mc_p2_5_kmh,
    mc_p97_5_kmh, limitations_json, audit_json, author, created_at
";

pub fn insert_calculation(conn: &Connection, c: &VideoSpeedCalculation) -> Result<()> {
    let points_json = serde_json::to_string(&c.points)?;
    let residuals_json = serde_json::to_string(&c.residuals)?;
    let mc_sigmas_json = c.mc_sigmas.as_ref().map(serde_json::to_string).transpose()?;
    let limitations_json = serde_json::to_string(&c.limitations)?;
    let audit_json = serde_json::to_string(&c.audit)?;

    conn.execute(
        &format!(
            "INSERT INTO video_speed_calculations ({CALCULATION_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,\
              ?19,?20,?21,?22,?23,?24,?25,?26)"
        ),
        params![
            c.id.to_string(),
            c.occurrence_id.to_string(),
            c.media_hash,
            c.calibration_id.to_string(),
            points_json,
            c.velocity_kmh,
            c.vx_m_per_s,
            c.vy_m_per_s,
            c.se_m_per_s,
            c.ci_low,
            c.ci_high,
            c.confidence,
            c.r_squared,
            residuals_json,
            c.mc_seed,
            mc_sigmas_json,
            c.mc_n,
            c.mc_failed,
            c.mc_mean_kmh,
            c.mc_median_kmh,
            c.mc_p2_5_kmh,
            c.mc_p97_5_kmh,
            limitations_json,
            audit_json,
            c.author,
            c.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn find_calculation_by_id(
    conn: &Connection,
    id: &Uuid,
) -> Result<Option<VideoSpeedCalculation>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CALCULATION_COLS} FROM video_speed_calculations WHERE id = ?1"
    ))?;
    let row = stmt
        .query_row([id.to_string()], row_to_calculation)
        .optional()?;
    Ok(row)
}

pub fn list_calculations_for_media(
    conn: &Connection,
    occurrence_id: &Uuid,
    media_hash: &str,
) -> Result<Vec<VideoSpeedCalculation>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CALCULATION_COLS} FROM video_speed_calculations
         WHERE occurrence_id = ?1 AND media_hash = ?2
         ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map(
            [occurrence_id.to_string(), media_hash.to_string()],
            row_to_calculation,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn list_calculations_for_occurrence(
    conn: &Connection,
    occurrence_id: &Uuid,
) -> Result<Vec<VideoSpeedCalculation>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CALCULATION_COLS} FROM video_speed_calculations
         WHERE occurrence_id = ?1
         ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_calculation)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn row_to_calculation(row: &Row<'_>) -> rusqlite::Result<VideoSpeedCalculation> {
    Ok(VideoSpeedCalculation {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        media_hash: row.get("media_hash")?,
        calibration_id: parse_uuid(row, "calibration_id")?,
        points: parse_json(row.get::<_, String>("points_json")?)?,
        velocity_kmh: row.get("velocity_kmh")?,
        vx_m_per_s: row.get("vx_m_per_s")?,
        vy_m_per_s: row.get("vy_m_per_s")?,
        se_m_per_s: row.get("se_m_per_s")?,
        ci_low: row.get("ci_low")?,
        ci_high: row.get("ci_high")?,
        confidence: row.get("confidence")?,
        r_squared: row.get("r_squared")?,
        residuals: parse_json(row.get::<_, String>("residuals_json")?)?,
        mc_seed: row.get("mc_seed")?,
        mc_sigmas: row
            .get::<_, Option<String>>("mc_sigmas_json")?
            .map(parse_json::<McSigmas>)
            .transpose()?,
        mc_n: row.get("mc_n")?,
        mc_failed: row.get("mc_failed")?,
        mc_mean_kmh: row.get("mc_mean_kmh")?,
        mc_median_kmh: row.get("mc_median_kmh")?,
        mc_p2_5_kmh: row.get("mc_p2_5_kmh")?,
        mc_p97_5_kmh: row.get("mc_p97_5_kmh")?,
        limitations: parse_json(row.get::<_, String>("limitations_json")?)?,
        audit: parse_json(row.get::<_, String>("audit_json")?)?,
        author: row.get("author")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

// ---------------------------------------------------------------------------
// helpers

fn parse_uuid(row: &Row<'_>, col: &str) -> rusqlite::Result<Uuid> {
    let s: String = row.get(col)?;
    Uuid::parse_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn parse_json<T: DeserializeOwned>(s: String) -> rusqlite::Result<T> {
    serde_json::from_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

// ---------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations::run_migrations;
    use crate::models::{ControlPoint, McSigmas, TrajectoryPoint};

    /// In-memory DB with the full migration set and FK enforcement ON, plus a
    /// parent occurrence so the FK chain (occurrence → calibration → calculation)
    /// is actually exercised.
    fn setup() -> (Connection, Uuid) {
        let mut conn = Connection::open_in_memory().expect("open in-memory");
        conn.pragma_update(None, "foreign_keys", "ON")
            .expect("enable fk");
        run_migrations(&mut conn).expect("run migrations");

        let occ_id = Uuid::new_v4();
        let now = "2026-05-31T12:00:00Z";
        conn.execute(
            "INSERT INTO occurrences (id, created_at, updated_at) VALUES (?1, ?2, ?2)",
            params![occ_id.to_string(), now],
        )
        .expect("seed occurrence");

        (conn, occ_id)
    }

    fn fixed_dt() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-31T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn sample_calibration(occ_id: Uuid) -> VideoSpeedCalibration {
        VideoSpeedCalibration {
            id: Uuid::new_v4(),
            occurrence_id: occ_id,
            media_hash: "abc123def456".into(),
            method: "plane".into(),
            control_points: vec![
                ControlPoint { px: 100.0, py: 200.0, world_x_m: 0.0, world_y_m: 0.0, label: Some("A".into()) },
                ControlPoint { px: 900.0, py: 210.0, world_x_m: 10.0, world_y_m: 0.0, label: Some("B".into()) },
                ControlPoint { px: 905.0, py: 780.0, world_x_m: 10.0, world_y_m: 8.0, label: None },
                ControlPoint { px: 110.0, py: 770.0, world_x_m: 0.0, world_y_m: 8.0, label: None },
            ],
            reference_source: "campo".into(),
            homography: [
                0.0125, 0.0001, -1.25,
                0.0002, 0.0140, -2.80,
                0.0000003, 0.0000011, 1.0,
            ],
            residuals_px: Some(0.42),
            distortion_model: None,
            author: "André Ricardo Barroso".into(),
            created_at: fixed_dt(),
        }
    }

    fn sample_calculation(occ_id: Uuid, calibration_id: Uuid) -> VideoSpeedCalculation {
        VideoSpeedCalculation {
            id: Uuid::new_v4(),
            occurrence_id: occ_id,
            media_hash: "abc123def456".into(),
            calibration_id,
            points: vec![
                TrajectoryPoint {
                    storyboard_frame_id: Some(Uuid::new_v4()),
                    export_id: Some(Uuid::new_v4()),
                    px: 480.0,
                    py: 500.0,
                    u_px: 2.5,
                    actual_timestamp_s: 1.0021,
                    delta_s: Some(0.0021),
                    manual: true,
                },
                TrajectoryPoint {
                    storyboard_frame_id: Some(Uuid::new_v4()),
                    export_id: None,
                    px: 520.0,
                    py: 505.0,
                    u_px: 2.5,
                    actual_timestamp_s: 1.4990,
                    delta_s: Some(-0.0010),
                    manual: true,
                },
            ],
            velocity_kmh: 57.6,
            vx_m_per_s: 15.9,
            vy_m_per_s: 1.2,
            se_m_per_s: Some(1.1),
            ci_low: Some(49.3),
            ci_high: Some(65.9),
            confidence: Some(0.95),
            r_squared: Some(0.991),
            residuals: vec![0.03, -0.05, 0.01, -0.02],
            mc_seed: Some(4242),
            mc_sigmas: Some(McSigmas {
                calibration_px: 1.5,
                world_m: 0.05,
                trajectory_px: 2.5,
                time_s: 0.002,
            }),
            mc_n: Some(10_000),
            mc_failed: Some(3),
            mc_mean_kmh: Some(57.4),
            mc_median_kmh: Some(57.5),
            mc_p2_5_kmh: Some(48.9),
            mc_p97_5_kmh: Some(66.2),
            limitations: vec![
                "Marcação manual do veículo (sem tracking automático).".into(),
                "Calibração por 4 pontos sem rejeição de outliers.".into(),
            ],
            audit: serde_json::json!({
                "ffmpeg": "6.1.1",
                "estimator": "per_axis_regression",
                "operator_note": "ponto 2 com leve oclusão",
            }),
            author: "André Ricardo Barroso".into(),
            created_at: fixed_dt(),
        }
    }

    #[test]
    fn calibration_round_trips_field_by_field() -> Result<()> {
        let (conn, occ_id) = setup();
        let cal = sample_calibration(occ_id);

        insert_calibration(&conn, &cal)?;

        let back = find_calibration_by_id(&conn, &cal.id)?.expect("calibration present");
        assert_eq!(back, cal);

        let listed = list_calibrations_for_media(&conn, &occ_id, &cal.media_hash)?;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0], cal);
        Ok(())
    }

    #[test]
    fn calculation_round_trips_field_by_field() -> Result<()> {
        let (conn, occ_id) = setup();
        let cal = sample_calibration(occ_id);
        insert_calibration(&conn, &cal)?;

        let calc = sample_calculation(occ_id, cal.id);
        insert_calculation(&conn, &calc)?;

        let back = find_calculation_by_id(&conn, &calc.id)?.expect("calculation present");
        assert_eq!(back, calc);

        // Reprodutibilidade: a semente e os sigmas sobrevivem ao round-trip.
        assert_eq!(back.mc_seed, Some(4242));
        assert_eq!(back.mc_sigmas, calc.mc_sigmas);
        // Cada ponto continua amarrado ao frame coletado real.
        assert!(back.points.iter().all(|p| p.storyboard_frame_id.is_some()));

        let by_media = list_calculations_for_media(&conn, &occ_id, &calc.media_hash)?;
        assert_eq!(by_media.len(), 1);
        assert_eq!(by_media[0], calc);
        Ok(())
    }

    #[test]
    fn calculation_requires_existing_calibration_fk() {
        let (conn, occ_id) = setup();
        // Nenhuma calibração inserida — a FK deve barrar o cálculo.
        let calc = sample_calculation(occ_id, Uuid::new_v4());
        let err = insert_calculation(&conn, &calc);
        assert!(err.is_err(), "FK órfã deveria falhar, veio: {err:?}");
    }
}
