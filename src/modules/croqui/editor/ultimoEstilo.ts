/** Última espessura e rótulo ajustados por tipo (cota, R1, R2…), relativos ao fator da folha:
 *  a cota seguinte nasce como a última que o perito acertou, em qualquer foto. */

import type { SicroObject } from "../engine";

const CHAVE = "sicro.croqui.ultimoEstilo";

interface EstiloLembrado {
  stroke_width?: number;
  label_size?: number;
}

function chaveDe(o: SicroObject): string | null {
  if (o.kind === "measurement") return "measurement";
  if (o.kind === "line") return `line:${o.subtype}`;
  return null;
}

function ler(): Record<string, EstiloLembrado> {
  try {
    return JSON.parse(localStorage.getItem(CHAVE) ?? "{}") as Record<string, EstiloLembrado>;
  } catch {
    return {};
  }
}

/** Guarda o que o patch mudou (espessura, tamanho do rótulo) dividido pelo fator. */
export function lembrarEstilo(o: SicroObject, patch: Partial<SicroObject>, fator: number): void {
  const k = chaveDe(o);
  if (!k) return;
  const p = patch as EstiloLembrado;
  if (p.stroke_width === undefined && p.label_size === undefined) return;
  const tudo = ler();
  const atual = tudo[k] ?? {};
  if (p.stroke_width !== undefined && p.stroke_width > 0) atual.stroke_width = p.stroke_width / fator;
  if (p.label_size !== undefined && p.label_size > 0) atual.label_size = p.label_size / fator;
  tudo[k] = atual;
  try {
    localStorage.setItem(CHAVE, JSON.stringify(tudo));
  } catch {
    /* sem storage: só não lembra */
  }
}

/** Objeto novo com a última espessura/rótulo lembrados para o tipo, escalados pelo fator. */
export function comUltimoEstilo<T extends SicroObject>(o: T, fator: number): T {
  const k = chaveDe(o);
  if (!k) return o;
  const e = ler()[k];
  if (!e) return o;
  const extra: EstiloLembrado = {};
  if (e.stroke_width) extra.stroke_width = Math.round(e.stroke_width * fator * 10) / 10;
  if (e.label_size) extra.label_size = Math.round(e.label_size * fator);
  return { ...o, ...extra };
}
