/**
 * Coordinate parsing helpers — MVP 10 OSM Road Import.
 *
 * Forensic users routinely paste a `(lat, lon)` pair into the SICRO
 * UI from a variety of sources (Google Maps, GPS, smartphone share
 * sheets, the operational SICRO 1.0). This module collapses the most
 * common Brazilian formats into a single normalised result, with
 * strict validation and clear error reasons so the UI can show the
 * user something more useful than "invalid input".
 *
 * Pure functions only — no React, no DOM, no I/O.
 */
import type { OsmViewport } from "./osm";

/** Successfully parsed coordinate pair. */
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

export interface CoordinateParseResult {
  ok: boolean;
  value: LatLon | null;
  error: CoordinateParseError | null;
}

/**
 * Parse a free-form coordinate string into `{ lat, lon }`.
 *
 * Accepted formats (whitespace tolerant):
 *
 *   `-0.0345, -51.0694`         — standard decimal
 *   `-0.0345 -51.0694`          — single space, no comma
 *   `0,0345S 51,0694W`          — Brazilian decimal-comma + hemisphere
 *   `0.0345 S, 51.0694 W`       — hemisphere with space
 *   `N 0.0345, E -51.0694`      — leading hemisphere
 *   `(-0.0345, -51.0694)`       — wrapping parens
 *   `lat: -0.0345 lon: -51.0694` — labelled
 *
 * Latitude is always the first number, longitude the second — Google
 * Maps convention. Hemisphere letters (N/S/E/W) override numeric sign
 * if they conflict.
 *
 * Returns `{ ok: true, value, error: null }` or `{ ok: false, value:
 * null, error }`. Never throws.
 */
export function parseCoordinates(input: string): CoordinateParseResult {
  if (input == null) return fail("empty");
  const raw = input.trim();
  if (raw.length === 0) return fail("empty");

  // Strip parens / labels / hemisphere letters into separate sign-fixes.
  let s = raw
    .replace(/[()]/g, " ")
    .replace(/lat\s*:?/gi, " ")
    .replace(/lon\s*:?/gi, " ")
    .replace(/long\s*:?/gi, " ")
    .replace(/latitude\s*:?/gi, " ")
    .replace(/longitude\s*:?/gi, " ");

  // Detect hemisphere letters BEFORE we lose them to splitting.
  // Strategy: every uppercase N/S/E/W (with optional surrounding space)
  // gets translated to a leading sign on whatever number it's attached to.
  // We do this in two passes: first capture the letters and their
  // approximate position, then apply signs after numeric extraction.
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

  // Brazilian users often write `0,0345` for `0.0345`. Distinguish the
  // decimal comma from the lat/lon separator: ONLY treat a comma as a
  // decimal mark when it sits between two digits. The "split comma"
  // (between numbers) gets handled by the tokenizer below.
  //
  // Heuristic: if there are exactly TWO numeric tokens separated by
  // ONE comma that's between digits + a *single* other comma between
  // numbers, that's ambiguous. We bail on the second comma and keep
  // the decimal one. In practice if the string has multiple commas we
  // assume `dec,dec` Brazilian format only when there's no other
  // delimiter; otherwise commas are pair separators.
  //
  // Pragmatic resolution: try a tokeniser that recognises both forms.
  const tokens = s.match(/-?\d+(?:[.,]\d+)?/g);
  if (!tokens || tokens.length < 2) {
    return fail(tokens && tokens.length === 1 ? "missing_separator" : "empty");
  }

  // If there are exactly two tokens, treat them as `lat`, `lon` with
  // decimal-aware parsing. If there are MORE, we look for the most
  // likely "lat" + "lon" pair (the first two; extras might be altitude
  // or noise).
  const latStr = tokens[0]!;
  const lonStr = tokens[1]!;
  const lat = parseDecimal(latStr);
  const lon = parseDecimal(lonStr);
  if (!Number.isFinite(lat) || !Number.isFinite(lon)) {
    return fail("not_a_number");
  }

  // Apply hemisphere signs.
  // Conservative rule: the first hemisphere letter applies to lat, the
  // second to lon. If only one letter exists, it applies to its axis.
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

/**
 * Format a coordinate pair for display — full precision but compact.
 * Mirrors what Google Maps shows so the user can paste the value back.
 */
export function formatCoordinates(c: LatLon): string {
  return `${c.lat.toFixed(6)}, ${c.lon.toFixed(6)}`;
}

/**
 * Compute an approximate bounding box around a centre point, given a
 * radius in metres. Uses the spherical Earth assumption (R ≈ 6,371 km)
 * and corrects longitude span by `cos(lat)` so the bbox stays tight at
 * mid-latitudes too.
 *
 * Good enough for ~1 km city-block buffers (sub-1% error). Use a
 * proper geodesic library if you ever need < 0.1 % at continental
 * distances.
 */
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

/**
 * Estimate canvas-pixel-per-real-metre scale for a given viewport.
 * The width of the bbox in metres ≈ (max_lon - min_lon) · cos(centre_lat) · EARTH_R · π / 180.
 *
 * Returns `null` for a degenerate viewport (zero span / zero pixels).
 */
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

// ---------------------------------------------------------------------------
// Helpers

function fail(error: CoordinateParseError): CoordinateParseResult {
  return { ok: false, value: null, error };
}

function parseDecimal(token: string): number {
  // Brazilian decimal comma → treat as decimal point if there's exactly
  // one comma and no dot. Otherwise assume dot is the decimal point and
  // the comma was the pair separator (which we've already removed).
  if (token.includes(",") && !token.includes(".")) {
    return Number.parseFloat(token.replace(",", "."));
  }
  return Number.parseFloat(token);
}

/**
 * Human-readable explanation for a parse error. The modal surfaces
 * these directly so the user knows what to fix.
 */
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
