/**
 * Parsing de coordenadas coladas pelo perito (Google Maps, GPS, SICRO 1.0),
 * nos formatos brasileiros mais comuns, com motivo de erro para a UI.
 * Funções puras — sem React, DOM ou I/O.
 */
import type { OsmViewport } from "./osm";

export interface LatLon {
  lat: number;
  lon: number;
}

export type CoordinateParseError =
  | "empty"
  | "missing_separator"
  | "not_a_number"
  | "lat_out_of_range"
  | "lon_out_of_range";

interface CoordinateParseResult {
  ok: boolean;
  value: LatLon | null;
  error: CoordinateParseError | null;
}

/**
 * Aceita `-0.0345, -51.0694`, `0,0345S 51,0694W`, `(lat: x, lon: y)`, hemisfério
 * antes ou depois do número etc. Latitude sempre primeiro (convenção Google Maps);
 * letra de hemisfério vence o sinal numérico. Nunca lança.
 */
export function parseCoordinates(input: string): CoordinateParseResult {
  if (input == null) return fail("empty");
  const raw = input.trim();
  if (raw.length === 0) return fail("empty");

  let s = raw
    .replace(/[()]/g, " ")
    .replace(/lat\s*:?/gi, " ")
    .replace(/lon\s*:?/gi, " ")
    .replace(/long\s*:?/gi, " ")
    .replace(/latitude\s*:?/gi, " ")
    .replace(/longitude\s*:?/gi, " ");

  // Captura as letras N/S/E/W antes do split; o sinal é aplicado depois da extração numérica.
  const hemispheres: Array<{ index: number; sign: number; axis: "lat" | "lon" }> =
    [];
  s = s.replace(/([NSEW])/gi, (_match, letter, offset: number) => {
    const L = String(letter).toUpperCase();
    hemispheres.push({
      index: offset,
      sign: L === "S" || L === "W" ? -1 : 1,
      axis: L === "N" || L === "S" ? "lat" : "lon",
    });
    return " ";
  });

  // Vírgula entre dígitos é decimal (formato brasileiro); vírgula entre números é separador.
  const tokens = s.match(/-?\d+(?:[.,]\d+)?/g);
  if (!tokens || tokens.length < 2) {
    return fail(tokens && tokens.length === 1 ? "missing_separator" : "empty");
  }

  // Dois primeiros tokens = lat, lon; extras (altitude, ruído) são ignorados.
  const latStr = tokens[0]!;
  const lonStr = tokens[1]!;
  const lat = parseDecimal(latStr);
  const lon = parseDecimal(lonStr);
  if (!Number.isFinite(lat) || !Number.isFinite(lon)) {
    return fail("not_a_number");
  }

  // Primeira letra de hemisfério vale para lat, segunda para lon.
  let finalLat = lat;
  let finalLon = lon;
  if (hemispheres.length >= 1) {
    const first = hemispheres[0]!;
    if (first.axis === "lat") finalLat = Math.abs(lat) * first.sign;
    else finalLon = Math.abs(lat) * first.sign;
  }
  if (hemispheres.length >= 2) {
    const second = hemispheres[1]!;
    if (second.axis === "lon") finalLon = Math.abs(lon) * second.sign;
    else finalLat = Math.abs(lon) * second.sign;
  }

  if (finalLat < -90 || finalLat > 90) return fail("lat_out_of_range");
  if (finalLon < -180 || finalLon > 180) return fail("lon_out_of_range");

  return { ok: true, value: { lat: finalLat, lon: finalLon }, error: null };
}

/** Mesmo formato do Google Maps, para o usuário poder colar de volta. */
export function formatCoordinates(c: LatLon): string {
  return `${c.lat.toFixed(6)}, ${c.lon.toFixed(6)}`;
}

/** Bbox aproximada (Terra esférica, R ≈ 6 371 km, span de longitude corrigido por cos(lat)). Erro < 1 % em ~1 km. */
export function bboxFromCenterRadius(
  centre: LatLon,
  radius_m: number,
  width_px: number,
  height_px: number,
): OsmViewport {
  const EARTH_R = 6_371_000; // metres
  const latDelta = (radius_m / EARTH_R) * (180 / Math.PI);
  const cosLat = Math.cos((centre.lat * Math.PI) / 180);
  const lonDelta = latDelta / Math.max(cosLat, 0.000001);
  return {
    min_lat: centre.lat - latDelta,
    max_lat: centre.lat + latDelta,
    min_lon: centre.lon - lonDelta,
    max_lon: centre.lon + lonDelta,
    width_px,
    height_px,
  };
}

/** px/m estimado pela largura da bbox em metros; `null` se viewport degenerada. */
export function estimatePxPerMeter(view: OsmViewport): number | null {
  const EARTH_R = 6_371_000;
  const lonSpanDeg = view.max_lon - view.min_lon;
  if (lonSpanDeg <= 0 || view.width_px <= 0) return null;
  const centreLat = (view.min_lat + view.max_lat) / 2;
  const cosLat = Math.cos((centreLat * Math.PI) / 180);
  const widthMetres = lonSpanDeg * cosLat * EARTH_R * (Math.PI / 180);
  if (widthMetres <= 0) return null;
  return view.width_px / widthMetres;
}

// ---- Helpers ----

function fail(error: CoordinateParseError): CoordinateParseResult {
  return { ok: false, value: null, error };
}

function parseDecimal(token: string): number {
  // Vírgula sem ponto = decimal brasileiro; o separador de par já foi removido.
  if (token.includes(",") && !token.includes(".")) {
    return Number.parseFloat(token.replace(",", "."));
  }
  return Number.parseFloat(token);
}

/** Mensagem mostrada direto no modal. */
export function coordinateParseErrorMessage(
  err: CoordinateParseError,
): string {
  switch (err) {
    case "empty":
      return "Informe as coordenadas (latitude, longitude).";
    case "missing_separator":
      return "Faltou separar latitude e longitude — use vírgula ou espaço.";
    case "not_a_number":
      return "Não consegui interpretar como número.";
    case "lat_out_of_range":
      return "Latitude fora do intervalo [-90, 90].";
    case "lon_out_of_range":
      return "Longitude fora do intervalo [-180, 180].";
  }
}
