/**
 * Frota SVG do designer (CorelDRAW, em `../assets/transito/`), desenhada em
 * escala real (1 mm = 1 m) e em retrato: largura no eixo x, comprimento no y,
 * frente para cima. Módulo puro (sem React/Konva); só `load*Image` toca o browser.
 */

import type { VehicleBodyType } from "./schema";

// ---- SVGs (texto cru via Vite ?raw) ----
import svgSedan from "../assets/transito/carro_sedan.svg?raw";
import svgHatch from "../assets/transito/carro_hatch.svg?raw";
import svgSuv from "../assets/transito/carro_suv.svg?raw";
import svgPickup from "../assets/transito/pickup_cabine_dupla.svg?raw";
import svgCaminhaoLeve from "../assets/transito/caminhao_leve.svg?raw";
import svgCaminhaoPesado from "../assets/transito/caminhao_pesado.svg?raw";
import svgCarreta from "../assets/transito/carreta_longa.svg?raw";
import svgMotoUrbana from "../assets/transito/moto_urbana.svg?raw";
import svgMotoEsportiva from "../assets/transito/moto_esportiva.svg?raw";
import svgMotoCarga from "../assets/transito/moto_carga.svg?raw";
import svgBikeUrbana from "../assets/transito/bicicleta_urbana.svg?raw";
import svgBikeEstrada from "../assets/transito/bicicleta_estrada.svg?raw";
import svgBikeCargueira from "../assets/transito/bicicleta_cargueira.svg?raw";
import svgVanPassageiro from "../assets/transito/van_passageiro.svg?raw";
import svgVanFurgao from "../assets/transito/van_furgao.svg?raw";
import svgOnibus from "../assets/transito/onibus_convencional.svg?raw";
import svgMicroOnibus from "../assets/transito/micro_onibus.svg?raw";
import svgOnibusLeito from "../assets/transito/onibus_leito.svg?raw";
import svgAmbulancia from "../assets/transito/ambulancia.svg?raw";
import svgTaxi from "../assets/transito/taxi.svg?raw";
import svgVtrPm from "../assets/transito/vtr_pm.svg?raw";
import svgVtrPc from "../assets/transito/vtr_pc.svg?raw";
import svgVtrPci from "../assets/transito/vtr_pci.svg?raw";
import svgVtrBm from "../assets/transito/vtr_bm.svg?raw";
import svgVtrPp from "../assets/transito/vtr_pp.svg?raw";
import svgGuincho from "../assets/transito/reboque_guincho.svg?raw";
import svgTrator from "../assets/transito/trator.svg?raw";
import svgPedMDorsal from "../assets/transito/pedestre_m_dorsal.svg?raw";
import svgPedMLateral from "../assets/transito/pedestre_m_lateral.svg?raw";
import svgPedMVentral from "../assets/transito/pedestre_m_ventral.svg?raw";
import svgPedFDorsal from "../assets/transito/pedestre_f_dorsal.svg?raw";
import svgPedFLateral from "../assets/transito/pedestre_f_lateral.svg?raw";
import svgPedFVentral from "../assets/transito/pedestre_f_ventral.svg?raw";

/**
 * Cor-chave da lataria nos SVGs civis, definida uma vez no CSS de cada arquivo
 * (o nome da classe varia: .fil0, .fil74…); recolorir = trocar o VALOR.
 */
export const ART_KEY_COLOR = "#CC0000";
/** Sombra da lataria (só no Sedan) — vira a cor alvo escurecida. */
const ART_KEY_SHADE = "#B0080A";

interface VehicleArtEntry {
  svg: string;
  /** Largura real (m) — eixo x do SVG. */
  widthM: number;
  /** Comprimento real (m) — eixo y do SVG; frente para cima. */
  lengthM: number;
  /** false = pintura oficial fixa (VTRs, ambulância, táxi); a cor do Inspector não repinta. */
  recolorable: boolean;
}

/** Tipos sem arte (other) caem na silhueta vetorial antiga do CanvasStage. */
export const VEHICLE_ART: Partial<Record<VehicleBodyType, VehicleArtEntry>> = {
  // -- civis recoloríveis --
  car: { svg: svgSedan, widthM: 2.21, lengthM: 4.6, recolorable: true },
  sedan: { svg: svgSedan, widthM: 2.21, lengthM: 4.6, recolorable: true },
  hatch: { svg: svgHatch, widthM: 2.12, lengthM: 4.0, recolorable: true },
  suv: { svg: svgSuv, widthM: 2.43, lengthM: 4.7, recolorable: true },
  pickup: { svg: svgPickup, widthM: 2.4, lengthM: 5.3, recolorable: true },
  truck: { svg: svgCaminhaoLeve, widthM: 3.19, lengthM: 6.54, recolorable: true },
  caminhao: { svg: svgCaminhaoLeve, widthM: 3.19, lengthM: 6.54, recolorable: true },
  caminhao_pesado: { svg: svgCaminhaoPesado, widthM: 3.45, lengthM: 9.6, recolorable: true },
  carreta: { svg: svgCarreta, widthM: 3.21, lengthM: 10.36, recolorable: true },
  moto: { svg: svgMotoUrbana, widthM: 1.04, lengthM: 2.1, recolorable: true },
  moto_esportiva: { svg: svgMotoEsportiva, widthM: 0.95, lengthM: 2.0, recolorable: true },
  moto_carga: { svg: svgMotoCarga, widthM: 1.08, lengthM: 2.21, recolorable: true },
  bike: { svg: svgBikeUrbana, widthM: 0.83, lengthM: 1.7, recolorable: true },
  bike_estrada: { svg: svgBikeEstrada, widthM: 0.78, lengthM: 1.7, recolorable: true },
  bike_cargueira: { svg: svgBikeCargueira, widthM: 0.8, lengthM: 1.73, recolorable: true },
  van: { svg: svgVanPassageiro, widthM: 2.29, lengthM: 5.5, recolorable: true },
  van_furgao: { svg: svgVanFurgao, widthM: 2.29, lengthM: 5.5, recolorable: true },
  onibus: { svg: svgOnibus, widthM: 3.81, lengthM: 12.0, recolorable: true },
  micro_onibus: { svg: svgMicroOnibus, widthM: 2.43, lengthM: 8.0, recolorable: true },
  onibus_leito: { svg: svgOnibusLeito, widthM: 4.53, lengthM: 14.0, recolorable: true },
  reboque_guincho: { svg: svgGuincho, widthM: 3.12, lengthM: 7.0, recolorable: true },
  trator: { svg: svgTrator, widthM: 2.17, lengthM: 4.5, recolorable: true },
  // -- pintura oficial fixa --
  ambulancia: { svg: svgAmbulancia, widthM: 2.43, lengthM: 5.5, recolorable: false },
  taxi: { svg: svgTaxi, widthM: 1.96, lengthM: 4.6, recolorable: false },
  vtr_pm: { svg: svgVtrPm, widthM: 2.04, lengthM: 4.7, recolorable: false },
  vtr_pc: { svg: svgVtrPc, widthM: 1.99, lengthM: 4.7, recolorable: false },
  vtr_pci: { svg: svgVtrPci, widthM: 1.89, lengthM: 4.7, recolorable: false },
  vtr_bm: { svg: svgVtrBm, widthM: 2.3, lengthM: 5.39, recolorable: false },
  vtr_pp: { svg: svgVtrPp, widthM: 1.93, lengthM: 4.7, recolorable: false },
};

/** Pedestres em decúbito (markers do grupo Pessoa), sem cor-chave. `lengthM` = comprimento deitado. */
export const PESSOA_ART: Record<
  string,
  { svg: string; widthM: number; lengthM: number }
> = {
  pedestre_m_dorsal: { svg: svgPedMDorsal, widthM: 0.51, lengthM: 1.75 },
  pedestre_m_lateral: { svg: svgPedMLateral, widthM: 0.7, lengthM: 1.75 },
  pedestre_m_ventral: { svg: svgPedMVentral, widthM: 0.52, lengthM: 1.75 },
  pedestre_f_dorsal: { svg: svgPedFDorsal, widthM: 0.43, lengthM: 1.65 },
  pedestre_f_lateral: { svg: svgPedFLateral, widthM: 0.66, lengthM: 1.6 },
  pedestre_f_ventral: { svg: svgPedFVentral, widthM: 0.46, lengthM: 1.6 },
};

export function getPessoaArt(
  subtype: string,
): { svg: string; widthM: number; lengthM: number } | null {
  return PESSOA_ART[subtype] ?? null;
}

/** Escurece um #rrggbb multiplicando os canais (sombra da lataria). */
export function darkenHex(hex: string, factor = 0.86): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return hex;
  const n = parseInt(m[1]!, 16);
  const ch = (v: number) =>
    Math.max(0, Math.min(255, Math.round(v * factor)))
      .toString(16)
      .padStart(2, "0");
  return `#${ch((n >> 16) & 0xff)}${ch((n >> 8) & 0xff)}${ch(n & 0xff)}`;
}

/** SVG recolorido para (tipo, cor); pintura fixa ignora a cor. `null` se o tipo não tem arte. */
export function getVehicleArtSvg(
  body: VehicleBodyType,
  color: string | null | undefined,
): string | null {
  const entry = VEHICLE_ART[body];
  if (!entry) return null;
  if (!entry.recolorable || !color) return entry.svg;
  const target = color.trim();
  if (!/^#?[0-9a-f]{6}$/i.test(target)) return entry.svg;
  const hex = target.startsWith("#") ? target : `#${target}`;
  return entry.svg
    .split(ART_KEY_COLOR)
    .join(hex)
    .split(ART_KEY_SHADE)
    .join(darkenHex(hex));
}

export function getVehicleRealDims(
  body: VehicleBodyType,
): { widthM: number; lengthM: number } | null {
  const entry = VEHICLE_ART[body];
  return entry ? { widthM: entry.widthM, lengthM: entry.lengthM } : null;
}

// ---- Carregamento de imagem (browser) com cache por (tipo, cor) ----

const imageCache = new Map<string, HTMLImageElement>();
const pendingCache = new Map<string, Promise<HTMLImageElement | null>>();

function cacheKey(body: VehicleBodyType, color: string | null | undefined) {
  const entry = VEHICLE_ART[body];
  // Pintura fixa: a cor não participa da chave (mesma imagem pra qualquer cor).
  return entry && entry.recolorable ? `${body}|${color ?? ""}` : `${body}|`;
}

/** Síncrono — para o primeiro paint sem flicker. */
export function getCachedVehicleArtImage(
  body: VehicleBodyType,
  color: string | null | undefined,
): HTMLImageElement | null {
  return imageCache.get(cacheKey(body, color)) ?? null;
}

export function getCachedPessoaArtImage(
  subtype: string,
): HTMLImageElement | null {
  return imageCache.get(`pessoa:${subtype}`) ?? null;
}

export function loadPessoaArtImage(
  subtype: string,
): Promise<HTMLImageElement | null> {
  const entry = PESSOA_ART[subtype];
  if (!entry || typeof window === "undefined") return Promise.resolve(null);
  const key = `pessoa:${subtype}`;
  const cached = imageCache.get(key);
  if (cached) return Promise.resolve(cached);
  const pending = pendingCache.get(key);
  if (pending) return pending;
  const promise = new Promise<HTMLImageElement | null>((resolve) => {
    const img = new window.Image();
    img.onload = () => {
      imageCache.set(key, img);
      pendingCache.delete(key);
      resolve(img);
    };
    img.onerror = () => {
      pendingCache.delete(key);
      resolve(null);
    };
    img.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(entry.svg)}`;
  });
  pendingCache.set(key, promise);
  return promise;
}

export function loadVehicleArtImage(
  body: VehicleBodyType,
  color: string | null | undefined,
): Promise<HTMLImageElement | null> {
  const svg = getVehicleArtSvg(body, color);
  if (!svg || typeof window === "undefined") return Promise.resolve(null);
  const key = cacheKey(body, color);
  const cached = imageCache.get(key);
  if (cached) return Promise.resolve(cached);
  const pending = pendingCache.get(key);
  if (pending) return pending;
  const promise = new Promise<HTMLImageElement | null>((resolve) => {
    const img = new window.Image();
    img.onload = () => {
      imageCache.set(key, img);
      pendingCache.delete(key);
      resolve(img);
    };
    img.onerror = () => {
      pendingCache.delete(key);
      resolve(null);
    };
    img.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  });
  pendingCache.set(key, promise);
  return promise;
}
