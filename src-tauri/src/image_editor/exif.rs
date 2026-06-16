//! G12.7 — EXIF reader (kamadak-exif).
//!
//! Lê as tags principais (DateTime, Camera Make/Model, GPS, ISO,
//! Exposure, FocalLength, Flash, Software) de JPEG/TIFF/WebP/HEIC.
//! Para PNG (que pode ter EXIF embarcado em chunks), kamadak-exif
//! também lê.
//!
//! Retorna um JSON string com objeto `{ tags: { key: value, ... },
//! gps: { lat, lon, alt }, datetime, camera, sw }`.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use exif::{Reader, Tag};
use serde_json::{json, Value};

/// Lê EXIF e retorna JSON string ou None se não houver/erro.
pub fn read_exif_json(path: &Path) -> Option<String> {
    let value = read_exif_value(path)?;
    serde_json::to_string(&value).ok()
}

/// Versão que retorna direto o Value (útil para outros relatórios).
pub fn read_exif_value(path: &Path) -> Option<Value> {
    let file = File::open(path).ok()?;
    let mut bufreader = BufReader::new(file);
    let exifreader = Reader::new();
    let exif = exifreader.read_from_container(&mut bufreader).ok()?;

    let mut tags = serde_json::Map::new();
    let mut gps_lat: Option<f64> = None;
    let mut gps_lon: Option<f64> = None;
    let mut gps_lat_ref: Option<String> = None;
    let mut gps_lon_ref: Option<String> = None;
    let mut gps_alt: Option<f64> = None;
    let mut datetime: Option<String> = None;
    let mut camera_make: Option<String> = None;
    let mut camera_model: Option<String> = None;
    let mut software: Option<String> = None;
    let mut iso: Option<u32> = None;
    let mut exposure: Option<String> = None;
    let mut f_number: Option<f32> = None;
    let mut focal_length: Option<f32> = None;
    let mut flash: Option<String> = None;
    let mut orientation: Option<u32> = None;

    for field in exif.fields() {
        let key = format!("{:?}", field.tag);
        let value = field.display_value().with_unit(&exif).to_string();
        tags.insert(key.clone(), Value::String(value.clone()));

        match field.tag {
            Tag::DateTime | Tag::DateTimeOriginal => {
                if datetime.is_none() {
                    datetime = Some(value.clone());
                }
            }
            Tag::Make => camera_make = Some(value.clone()),
            Tag::Model => camera_model = Some(value.clone()),
            Tag::Software => software = Some(value.clone()),
            Tag::PhotographicSensitivity | Tag::ISOSpeed => {
                iso = value.trim().parse::<u32>().ok();
            }
            Tag::ExposureTime => exposure = Some(value.clone()),
            Tag::FNumber => {
                f_number = parse_first_float(&value);
            }
            Tag::FocalLength => {
                focal_length = parse_first_float(&value);
            }
            Tag::Flash => flash = Some(value.clone()),
            Tag::Orientation => {
                orientation = value.trim().parse::<u32>().ok();
            }
            Tag::GPSLatitude => {
                gps_lat = parse_dms(&value);
            }
            Tag::GPSLongitude => {
                gps_lon = parse_dms(&value);
            }
            Tag::GPSLatitudeRef => {
                gps_lat_ref = Some(value.trim().to_string());
            }
            Tag::GPSLongitudeRef => {
                gps_lon_ref = Some(value.trim().to_string());
            }
            Tag::GPSAltitude => {
                gps_alt = parse_first_float(&value).map(|v| v as f64);
            }
            _ => {}
        }
    }

    // Aplica sinal correto a partir do hemisfério.
    let lat = match (gps_lat, gps_lat_ref.as_deref()) {
        (Some(v), Some(r)) if r.starts_with('S') => Some(-v),
        (Some(v), _) => Some(v),
        _ => None,
    };
    let lon = match (gps_lon, gps_lon_ref.as_deref()) {
        (Some(v), Some(r)) if r.starts_with('W') => Some(-v),
        (Some(v), _) => Some(v),
        _ => None,
    };

    let gps = match (lat, lon) {
        (Some(la), Some(lo)) => json!({
            "lat": la,
            "lon": lo,
            "alt_m": gps_alt,
        }),
        _ => Value::Null,
    };

    // Verifica se exif está vazio (alguns containers retornam reader sem campos).
    if tags.is_empty() {
        // Inserir flag textual ainda permite ao caller distinguir "vazio" vs "ausente".
        let _ = orientation;
        return None;
    }

    let summary = json!({
        "datetime": datetime,
        "camera": {
            "make": camera_make,
            "model": camera_model,
        },
        "software": software,
        "iso": iso,
        "exposure_time": exposure,
        "f_number": f_number,
        "focal_length_mm": focal_length,
        "flash": flash,
        "orientation": orientation,
        "gps": gps,
    });

    Some(json!({
        "summary": summary,
        "tags": tags,
    }))
}

/// Parseia formato DMS do kamadak (`"23 deg 30' 15.5\" N"`) ou
/// `"23.5°N"` em graus decimais positivos.
fn parse_dms(value: &str) -> Option<f64> {
    let s = value.trim();
    if s.is_empty() {
        return None;
    }
    // Tentativa 1: graus decimais simples.
    if let Ok(v) = s.trim_end_matches(|c: char| c.is_alphabetic() || c == '°' || c == ' ').trim().parse::<f64>() {
        return Some(v);
    }
    // Tentativa 2: parse DMS no formato kamadak: "23 deg 30 min 15 sec".
    let cleaned: String = s
        .replace("deg", " ")
        .replace("min", " ")
        .replace("sec", " ")
        .replace("'", " ")
        .replace('"', " ")
        .replace('°', " ");
    let parts: Vec<f64> = cleaned
        .split_whitespace()
        .filter_map(|p| p.trim_end_matches(|c: char| c.is_alphabetic()).parse::<f64>().ok())
        .collect();
    if parts.is_empty() {
        return None;
    }
    let d = parts.first().copied().unwrap_or(0.0);
    let m = parts.get(1).copied().unwrap_or(0.0);
    let sec = parts.get(2).copied().unwrap_or(0.0);
    Some(d.abs() + m / 60.0 + sec / 3600.0)
}

fn parse_first_float(value: &str) -> Option<f32> {
    value
        .split_whitespace()
        .next()
        .and_then(|t| t.trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.' && c != '-').parse::<f32>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dms_decimal_parse_works() {
        assert!((parse_dms("23.5").unwrap() - 23.5).abs() < 1e-6);
    }

    #[test]
    fn dms_d_m_s_parse_works() {
        // 23° 30' 0" = 23.5°
        let v = parse_dms("23 deg 30 min 0 sec").unwrap();
        assert!((v - 23.5).abs() < 1e-3);
    }

    #[test]
    fn empty_dms_returns_none() {
        assert!(parse_dms("").is_none());
    }

    #[test]
    fn parse_first_float_extracts_value() {
        assert!((parse_first_float("1.8 EV").unwrap() - 1.8).abs() < 1e-3);
        assert!(parse_first_float("").is_none());
    }
}
