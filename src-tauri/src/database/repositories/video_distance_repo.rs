//! Repositório da Medição de Distância por fotogrametria.
//!
//! Tabela única: `video_distance_measurements` (migration 011). Como no
//! `video_speed_repo`, este repositório **(de)serializa** as colunas `*_json`
//! (sigmas do Monte Carlo, ressalvas, auditoria) para tipos estruturados — o
//! laudo precisa reabrir a medição e reproduzir o número exato.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::error::Result;
use crate::models::{McSigmasDistance, VideoDistanceMeasurement};

const MEASUREMENT_COLS: &str = "
    id, occurrence_id, media_hash, calibration_id, p1_px, p1_py, p2_px, p2_py,
    distance_m, mc_seed, mc_sigmas_json, mc_n, mc_failed, mc_mean_m,
    mc_median_m, mc_p2_5_m, mc_p97_5_m, limitations_json, audit_json,
    author, created_at
";

pub fn insert_measurement(conn: &Connection, m: &VideoDistanceMeasurement) -> Result<()> {
    let mc_sigmas_json = m.mc_sigmas.as_ref().map(serde_json::to_string).transpose()?;
    let limitations_json = serde_json::to_string(&m.limitations)?;
    let audit_json = serde_json::to_string(&m.audit)?;

    conn.execute(
        &format!(
            "INSERT INTO video_distance_measurements ({MEASUREMENT_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)"
        ),
        params![
            m.id.to_string(),
            m.occurrence_id.to_string(),
            m.media_hash,
            m.calibration_id.to_string(),
            m.p1_px,
            m.p1_py,
            m.p2_px,
            m.p2_py,
            m.distance_m,
            m.mc_seed,
            mc_sigmas_json,
            m.mc_n,
            m.mc_failed,
            m.mc_mean_m,
            m.mc_median_m,
            m.mc_p2_5_m,
            m.mc_p97_5_m,
            limitations_json,
            audit_json,
            m.author,
            m.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn find_measurement_by_id(
    conn: &Connection,
    id: &Uuid,
) -> Result<Option<VideoDistanceMeasurement>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEASUREMENT_COLS} FROM video_distance_measurements WHERE id = ?1"
    ))?;
    let row = stmt
        .query_row([id.to_string()], row_to_measurement)
        .optional()?;
    Ok(row)
}

pub fn list_measurements_for_media(
    conn: &Connection,
    occurrence_id: &Uuid,
    media_hash: &str,
) -> Result<Vec<VideoDistanceMeasurement>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEASUREMENT_COLS} FROM video_distance_measurements
         WHERE occurrence_id = ?1 AND media_hash = ?2
         ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map(
            [occurrence_id.to_string(), media_hash.to_string()],
            row_to_measurement,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn list_measurements_for_occurrence(
    conn: &Connection,
    occurrence_id: &Uuid,
) -> Result<Vec<VideoDistanceMeasurement>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEASUREMENT_COLS} FROM video_distance_measurements
         WHERE occurrence_id = ?1
         ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_measurement)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn row_to_measurement(row: &Row<'_>) -> rusqlite::Result<VideoDistanceMeasurement> {
    Ok(VideoDistanceMeasurement {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        media_hash: row.get("media_hash")?,
        calibration_id: parse_uuid(row, "calibration_id")?,
        p1_px: row.get("p1_px")?,
        p1_py: row.get("p1_py")?,
        p2_px: row.get("p2_px")?,
        p2_py: row.get("p2_py")?,
        distance_m: row.get("distance_m")?,
        mc_seed: row.get("mc_seed")?,
        mc_sigmas: row
            .get::<_, Option<String>>("mc_sigmas_json")?
            .map(parse_json::<McSigmasDistance>)
            .transpose()?,
        mc_n: row.get("mc_n")?,
        mc_failed: row.get("mc_failed")?,
        mc_mean_m: row.get("mc_mean_m")?,
        mc_median_m: row.get("mc_median_m")?,
        mc_p2_5_m: row.get("mc_p2_5_m")?,
        mc_p97_5_m: row.get("mc_p97_5_m")?,
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
    use crate::database::repositories::video_speed_repo;
    use crate::models::{ControlPoint, VideoSpeedCalibration};

    /// In-memory DB com migrações + FK ON, e uma ocorrência pai para exercitar
    /// a cadeia occurrence → calibration → measurement.
    fn setup() -> (Connection, Uuid) {
        let mut conn = Connection::open_in_memory().expect("open in-memory");
        conn.pragma_update(None, "foreign_keys", "ON")
            .expect("enable fk");
        run_migrations(&mut conn).expect("run migrations");

        let occ_id = Uuid::new_v4();
        conn.execute(
            "INSERT INTO occurrences (id, created_at, updated_at) VALUES (?1, ?2, ?2)",
            params![occ_id.to_string(), "2026-05-31T12:00:00Z"],
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
            media_hash: "dist_media_001".into(),
            method: "plane".into(),
            control_points: vec![
                ControlPoint { px: 0.0, py: 0.0, world_x_m: 0.0, world_y_m: 0.0, label: None },
                ControlPoint { px: 100.0, py: 0.0, world_x_m: 1.0, world_y_m: 0.0, label: None },
                ControlPoint { px: 100.0, py: 100.0, world_x_m: 1.0, world_y_m: 1.0, label: None },
                ControlPoint { px: 0.0, py: 100.0, world_x_m: 0.0, world_y_m: 1.0, label: None },
            ],
            reference_source: "campo".into(),
            homography: [0.01, 0.0, 0.0, 0.0, 0.01, 0.0, 0.0, 0.0, 1.0],
            residuals_px: Some(0.0),
            distortion_model: None,
            author: "André".into(),
            created_at: fixed_dt(),
        }
    }

    fn sample_measurement(
        occ_id: Uuid,
        calibration_id: Uuid,
        with_mc: bool,
    ) -> VideoDistanceMeasurement {
        VideoDistanceMeasurement {
            id: Uuid::new_v4(),
            occurrence_id: occ_id,
            media_hash: "dist_media_001".into(),
            calibration_id,
            p1_px: 100.0,
            p1_py: 500.0,
            p2_px: 600.0,
            p2_py: 500.0,
            distance_m: 5.0,
            mc_seed: with_mc.then_some(4242),
            mc_sigmas: with_mc.then(|| McSigmasDistance {
                calibration_px: 0.5,
                world_m: 0.02,
                measure_px: 1.0,
            }),
            mc_n: with_mc.then_some(5000),
            mc_failed: with_mc.then_some(2),
            mc_mean_m: with_mc.then_some(5.01),
            mc_median_m: with_mc.then_some(5.0),
            mc_p2_5_m: with_mc.then_some(4.88),
            mc_p97_5_m: with_mc.then_some(5.14),
            limitations: vec![
                "Marcação manual dos pontos (sem detecção automática).".into(),
                "Calibração por medição em campo.".into(),
            ],
            audit: serde_json::json!({ "tool": "sicro-desktop/video.measure" }),
            author: "André".into(),
            created_at: fixed_dt(),
        }
    }

    #[test]
    fn measurement_with_mc_round_trips_field_by_field() -> Result<()> {
        let (conn, occ_id) = setup();
        let cal = sample_calibration(occ_id);
        video_speed_repo::insert_calibration(&conn, &cal)?;

        let m = sample_measurement(occ_id, cal.id, true);
        insert_measurement(&conn, &m)?;

        let back = find_measurement_by_id(&conn, &m.id)?.expect("measurement present");
        assert_eq!(back, m);
        // Reprodutibilidade: semente + sigmas sobrevivem ao round-trip.
        assert_eq!(back.mc_seed, Some(4242));
        assert_eq!(back.mc_sigmas, m.mc_sigmas);

        let by_media = list_measurements_for_media(&conn, &occ_id, &m.media_hash)?;
        assert_eq!(by_media.len(), 1);
        assert_eq!(by_media[0], m);

        let by_occ = list_measurements_for_occurrence(&conn, &occ_id)?;
        assert_eq!(by_occ.len(), 1);
        assert_eq!(by_occ[0], m);
        Ok(())
    }

    #[test]
    fn measurement_without_mc_keeps_nulls() -> Result<()> {
        let (conn, occ_id) = setup();
        let cal = sample_calibration(occ_id);
        video_speed_repo::insert_calibration(&conn, &cal)?;

        let m = sample_measurement(occ_id, cal.id, false);
        insert_measurement(&conn, &m)?;

        let back = find_measurement_by_id(&conn, &m.id)?.expect("present");
        assert_eq!(back, m);
        // Sem MC: a distância pontual existe, todo o bloco mc_* é None.
        assert!((back.distance_m - 5.0).abs() < 1e-12);
        assert_eq!(back.mc_seed, None);
        assert_eq!(back.mc_sigmas, None);
        assert_eq!(back.mc_mean_m, None);
        assert_eq!(back.mc_p97_5_m, None);
        Ok(())
    }

    #[test]
    fn measurement_requires_existing_calibration_fk() {
        let (conn, occ_id) = setup();
        // Nenhuma calibração inserida — a FK deve barrar a medição.
        let m = sample_measurement(occ_id, Uuid::new_v4(), false);
        let err = insert_measurement(&conn, &m);
        assert!(err.is_err(), "FK órfã deveria falhar, veio: {err:?}");
    }
}
