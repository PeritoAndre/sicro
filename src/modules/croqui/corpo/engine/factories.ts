/** Factories do croqui corporal: documento e marcadores de lesão. */

import { BODY_TEMPLATES, type BodyView } from "../assets/bodyTemplates";
import type { LesaoTipo } from "./lesions";
import {
  CORPO_SCHEMA_VERSION,
  type SicroCorpoDoc,
  type SicroLesaoMarker,
} from "./schema";

function uid(prefix: string): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `${prefix}_${crypto.randomUUID()}`;
  }
  return `${prefix}_${Date.now().toString(36)}_${Math.random()
    .toString(36)
    .slice(2, 8)}`;
}

interface MakeCorpoDocOptions {
  template_id?: BodyView;
  title?: string;
  now?: string; // ISO; injetável pra testes determinísticos
}

export function makeCorpoDoc(
  corpo_id: string,
  occurrence_id: string,
  opts: MakeCorpoDocOptions = {},
): SicroCorpoDoc {
  const template_id = opts.template_id ?? "corpo_masc";
  const tpl = BODY_TEMPLATES[template_id];
  const now = opts.now ?? new Date().toISOString();
  return {
    schema_version: CORPO_SCHEMA_VERSION,
    corpo_id,
    occurrence_id,
    title: opts.title ?? "Croqui corporal",
    created_at: now,
    updated_at: now,
    template_id,
    canvas: { width_px: tpl.width, height_px: tpl.height },
    markers: [],
  };
}

/** Cria um marcador de lesão no ponto (x,y) com o número informado. */
export function makeLesao(
  x: number,
  y: number,
  tipo: LesaoTipo,
  number: number,
): SicroLesaoMarker {
  return {
    id: uid("lesao"),
    number,
    x,
    y,
    tipo,
    regiao: null,
    lateralidade: null,
    instrumento: null,
    dimensoes_cm: null,
    observacao: null,
    color: null,
    size: 12,
  };
}
