/**
 * Monta uma imagem única (PNG) com os tiles de um mapa base cobrindo a caixa pedida, no zoom que
 * dá a resolução desejada. Os tiles passam pelo Rust (User-Agent do SICRO, sem CORS).
 */

import { commands } from "@core/commands";
import { type MapaBase, urlDoTile } from "./mapasBase";
import { TILE, caixaDoRaio, lonLatParaPixel, metrosPorPixel, zoomPara } from "./mercator";

export interface FundoMontado {
  pngBase64: string;
  width: number;
  height: number;
  /** px/m da imagem na latitude do centro. */
  px_per_m: number;
  zoom: number;
  /** Tiles que falharam (ficam transparentes). */
  falhas: number;
}

const MAX_LADO_PX = 6000;
const PARALELO = 6;

function mime(url: string): string {
  return /\.png(\?|$)/i.test(url) ? "image/png" : "image/jpeg";
}

async function carregarTile(url: string): Promise<HTMLImageElement> {
  const b64 = await commands.fetchMapTile(url);
  const img = new Image();
  img.src = `data:${mime(url)};base64,${b64}`;
  await new Promise<void>((ok, erro) => {
    img.onload = () => ok();
    img.onerror = () => erro(new Error("tile ilegível"));
  });
  return img;
}

/**
 * `alvoPxPorM`: resolução desejada; o zoom sobe até alcançá-la ou até o máximo do mapa.
 * A imagem sai no tamanho natural dos tiles (sem reamostrar): quem usa escala pelo px_per_m devolvido.
 */
export async function montarFundoDeMapa(
  mapa: MapaBase,
  lat: number,
  lon: number,
  raioM: number,
  alvoPxPorM: number,
  aoProgredir?: (feitos: number, total: number) => void,
): Promise<FundoMontado> {
  let z = zoomPara(lat, alvoPxPorM, mapa.maxzoom);
  const caixa = caixaDoRaio(lat, lon, raioM);
  // Limite de lado: caixa grande em zoom alto estoura memória; desce o zoom até caber.
  let p0 = lonLatParaPixel(caixa.min_lon, caixa.max_lat, z);
  let p1 = lonLatParaPixel(caixa.max_lon, caixa.min_lat, z);
  while (z > 1 && Math.max(p1.x - p0.x, p1.y - p0.y) > MAX_LADO_PX) {
    z--;
    p0 = lonLatParaPixel(caixa.min_lon, caixa.max_lat, z);
    p1 = lonLatParaPixel(caixa.max_lon, caixa.min_lat, z);
  }
  const x0 = Math.floor(p0.x);
  const y0 = Math.floor(p0.y);
  const width = Math.max(1, Math.round(p1.x - p0.x));
  const height = Math.max(1, Math.round(p1.y - p0.y));
  const tx0 = Math.floor(p0.x / TILE);
  const ty0 = Math.floor(p0.y / TILE);
  const tx1 = Math.floor((p1.x - 1) / TILE);
  const ty1 = Math.floor((p1.y - 1) / TILE);

  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("canvas 2D indisponível");

  const fila: [number, number][] = [];
  for (let ty = ty0; ty <= ty1; ty++) for (let tx = tx0; tx <= tx1; tx++) fila.push([tx, ty]);
  const total = fila.length;
  let feitos = 0;
  let falhas = 0;
  const trabalhador = async () => {
    for (;;) {
      const item = fila.shift();
      if (!item) return;
      const [tx, ty] = item;
      try {
        const img = await carregarTile(urlDoTile(mapa, z, tx, ty));
        ctx.drawImage(img, tx * TILE - x0, ty * TILE - y0);
      } catch {
        falhas++;
      }
      feitos++;
      aoProgredir?.(feitos, total);
    }
  };
  await Promise.all(Array.from({ length: Math.min(PARALELO, total) }, trabalhador));

  return {
    pngBase64: canvas.toDataURL("image/png").split(",")[1] ?? "",
    width,
    height,
    px_per_m: 1 / metrosPorPixel(lat, z),
    zoom: z,
    falhas,
  };
}
