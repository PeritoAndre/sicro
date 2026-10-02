/**
 * Schema do `.sicrocorpo` (croqui corporal). O backend trata o arquivo como
 * JSON opaco; `coerceCorpoDoc` carrega qualquer JSON com defaults e nunca
 * quebra ao abrir.
 */

import {
  BODY_TEMPLATES,
  LEGACY_TEMPLATE_MAP,
  type BodyView,
} from "../assets/bodyTemplates";
import { isLesaoTipo, type LesaoTipo } from "./lesions";
import type { Lateralidade } from "./regions";

export const CORPO_SCHEMA_VERSION = "0.1";

/** Marcador de lesão/achado posicionado sobre a prancha. */
export interface SicroLesaoMarker {
  id: string;
  /** Número sequencial exibido no marcador e na legenda. */
  number: number;
  /** Posição em coordenadas lógicas da prancha (viewBox do template). */
  x: number;
  y: number;
  /** Prancha em que foi marcada (marcações não migram entre pranchas);
   *  markers antigos sem o campo recebem a prancha do doc no coerce. */
  template?: BodyView;
  tipo: LesaoTipo;
  /** id de região (regions.ts / regionsPop.ts). */
  regiao?: string | null;
  lateralidade?: Lateralidade | null;
  /** Meio/instrumento/ação que produziu (texto livre — POP 6.01). */
  instrumento?: string | null;
  /** Dimensões textuais ("2,0 x 0,8 cm") — texto livre, sem medição automática. */
  dimensoes_cm?: string | null;
  observacao?: string | null;
  /** Override de cor; se ausente, usa a cor do tipo. */
  color?: string | null;
  /** Raio do marcador em px (default 12). */
  size?: number;
}

interface SicroCorpoCanvas {
  width_px: number;
  height_px: number;
}

export interface SicroCorpoDoc {
  schema_version: string;
  corpo_id: string;
  occurrence_id: string;
  title: string;
  created_at: string;
  updated_at: string;
  /** Prancha ativa. */
  template_id: BodyView;
  canvas: SicroCorpoCanvas;
  markers: SicroLesaoMarker[];
}

// ---------------------------------------------------------------------------
// Coerção

const VALID_VIEWS: BodyView[] = Object.keys(BODY_TEMPLATES) as BodyView[];
const VALID_LADOS: Lateralidade[] = ["D", "E", "central"];

function str(o: Record<string, unknown>, k: string): string | undefined {
  const v = o[k];
  return typeof v === "string" ? v : undefined;
}

function num(v: unknown, fallback: number): number {
  return typeof v === "number" && Number.isFinite(v) ? v : fallback;
}

function strOrNull(v: unknown): string | null {
  return typeof v === "string" && v.length > 0 ? v : null;
}

function coerceMarker(raw: unknown, index: number): SicroLesaoMarker | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  const tipo: LesaoTipo = isLesaoTipo(o["tipo"]) ? o["tipo"] : "outro";
  const ladoRaw = o["lateralidade"];
  const lateralidade =
    typeof ladoRaw === "string" && VALID_LADOS.includes(ladoRaw as Lateralidade)
      ? (ladoRaw as Lateralidade)
      : null;
  return {
    id: str(o, "id") ?? `lesao_${Date.now().toString(36)}_${index}`,
    number: num(o["number"], index + 1),
    x: num(o["x"], 0),
    y: num(o["y"], 0),
    // Validado no pós-passe do coerceCorpoDoc (precisa do template do doc).
    template: str(o, "template") as SicroLesaoMarker["template"],
    tipo,
    regiao: strOrNull(o["regiao"]),
    lateralidade,
    instrumento: strOrNull(o["instrumento"]),
    dimensoes_cm: strOrNull(o["dimensoes_cm"]),
    observacao: strOrNull(o["observacao"]),
    color: strOrNull(o["color"]),
    size: typeof o["size"] === "number" ? (o["size"] as number) : 12,
  };
}

export function coerceCorpoDoc(raw: unknown): SicroCorpoDoc {
  if (!raw || typeof raw !== "object") {
    throw new Error("invalid .sicrocorpo: not an object");
  }
  const o = raw as Record<string, unknown>;
  const corpo_id = str(o, "corpo_id");
  const occurrence_id = str(o, "occurrence_id");
  if (!corpo_id || !occurrence_id) {
    throw new Error("invalid .sicrocorpo: missing corpo_id or occurrence_id");
  }

  // Ids legados (pranchas SVG antigas) mapeiam pro equivalente novo;
  // irreconhecível cai em "corpo_masc".
  const templateRaw = str(o, "template_id") ?? "";
  const template_id: BodyView = VALID_VIEWS.includes(templateRaw as BodyView)
    ? (templateRaw as BodyView)
    : (LEGACY_TEMPLATE_MAP[templateRaw] ?? "corpo_masc");
  const tpl = BODY_TEMPLATES[template_id];

  // Canvas persistido (dimensões em que os markers foram posicionados).
  const canvasRaw = (o["canvas"] ?? {}) as Record<string, unknown>;
  const savedCanvas: SicroCorpoCanvas = {
    width_px: num(canvasRaw["width_px"], tpl.width),
    height_px: num(canvasRaw["height_px"], tpl.height),
  };

  // Doc legado: canvas salvo ≠ dims da prancha (a troca SVG→PNG mudou as
  // dimensões). Re-escala só markers sem prancha própria; os com `template`
  // válido já estão nas coordenadas da própria prancha.
  const needsRescale =
    savedCanvas.width_px > 0 &&
    savedCanvas.height_px > 0 &&
    (savedCanvas.width_px !== tpl.width || savedCanvas.height_px !== tpl.height);
  const sx = needsRescale ? tpl.width / savedCanvas.width_px : 1;
  const sy = needsRescale ? tpl.height / savedCanvas.height_px : 1;

  const markersRaw = Array.isArray(o["markers"]) ? o["markers"] : [];
  const markers = markersRaw
    .map((m, i) => coerceMarker(m, i))
    .filter((m): m is SicroLesaoMarker => m !== null)
    .map((m) => {
      const raw = (m.template as string | undefined) ?? "";
      if (VALID_VIEWS.includes(raw as BodyView)) {
        return { ...m, template: raw as BodyView };
      }
      // Legado: re-escala e herda a prancha resolvida do doc.
      return { ...m, x: m.x * sx, y: m.y * sy, template: template_id };
    });

  // Canvas do doc = dims da prancha ativa (o editor atualiza ao trocar).
  const canvas: SicroCorpoCanvas = { width_px: tpl.width, height_px: tpl.height };

  const now = str(o, "updated_at") ?? str(o, "created_at") ?? "";
  return {
    schema_version: str(o, "schema_version") ?? CORPO_SCHEMA_VERSION,
    corpo_id,
    occurrence_id,
    title: str(o, "title") ?? "Croqui corporal",
    created_at: str(o, "created_at") ?? now,
    updated_at: str(o, "updated_at") ?? now,
    template_id,
    canvas,
    markers,
  };
}

/** Próximo número sequencial (maior existente + 1), determinístico. */
export function nextMarkerNumber(doc: SicroCorpoDoc): number {
  return doc.markers.reduce((max, m) => Math.max(max, m.number), 0) + 1;
}
