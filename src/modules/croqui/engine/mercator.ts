/** Web Mercator (EPSG:3857) em pixels de tile (256 px), como os servidores de tiles usam. */

export const TILE = 256;
const R = 6_378_137;

/** Coordenada de pixel global no zoom `z`. */
export function lonLatParaPixel(lon: number, lat: number, z: number): { x: number; y: number } {
  const n = TILE * 2 ** z;
  const latR = (Math.max(-85.05, Math.min(85.05, lat)) * Math.PI) / 180;
  return {
    x: ((lon + 180) / 360) * n,
    y: ((1 - Math.log(Math.tan(latR) + 1 / Math.cos(latR)) / Math.PI) / 2) * n,
  };
}

/** Metros do chão por pixel de tile, na latitude, no zoom. */
export function metrosPorPixel(lat: number, z: number): number {
  return (2 * Math.PI * R * Math.cos((lat * Math.PI) / 180)) / (TILE * 2 ** z);
}

/** Menor zoom (até `max`) em que o pixel do tile é no mínimo tão fino quanto `alvoPxPorM` pede. */
export function zoomPara(lat: number, alvoPxPorM: number, max: number): number {
  for (let z = 1; z <= max; z++) if (1 / metrosPorPixel(lat, z) >= alvoPxPorM) return z;
  return max;
}

export interface CaixaLatLon {
  min_lat: number;
  max_lat: number;
  min_lon: number;
  max_lon: number;
}

/** Quadrado de lado 2·raio centrado no ponto, em graus (Terra esférica; erro < 1 % em 1 km). */
export function caixaDoRaio(lat: number, lon: number, raioM: number): CaixaLatLon {
  const dLat = (raioM / 6_371_000) * (180 / Math.PI);
  const dLon = dLat / Math.max(Math.cos((lat * Math.PI) / 180), 1e-6);
  return { min_lat: lat - dLat, max_lat: lat + dLat, min_lon: lon - dLon, max_lon: lon + dLon };
}
