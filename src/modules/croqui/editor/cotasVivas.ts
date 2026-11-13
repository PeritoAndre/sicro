/**
 * Cotas vivas: presas pela perpendicular ao R1/R2 (`ref_linha`) e/ou a um objeto (`alvo_id`, com o
 * deslocamento do centro dele até a ponta). Mexeu o carro, a cota acompanha, perpendicular e com a medida nova.
 */

import type { SicroMeasurementObject, SicroObject, SicroPoint } from "../engine";
import { acharReferencial, peDaPerpendicular } from "../engine/referenciais";
import { getObjectBoundsStagePx } from "./bounds";

/** Tipos que uma cota pode seguir. */
const SEGUIVEIS = new Set(["vehicle", "person", "marker", "fixture", "trace", "text"]);

export function podeSerAlvo(o: SicroObject): boolean {
  return SEGUIVEIS.has(o.kind);
}

/** Centro da caixa do objeto (px de mundo). */
export function ancoraDoObjeto(o: SicroObject, pxPerM: number): SicroPoint {
  const b = getObjectBoundsStagePx(o, pxPerM);
  return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
}

/** Objeto seguível mais à frente sob o ponto. */
export function objetoSob(objects: ReadonlyArray<SicroObject>, p: SicroPoint, pxPerM: number): SicroObject | null {
  for (let i = objects.length - 1; i >= 0; i--) {
    const o = objects[i]!;
    if (!podeSerAlvo(o) || o.visible === false) continue;
    const b = getObjectBoundsStagePx(o, pxPerM);
    if (p.x >= b.x && p.x <= b.x + b.width && p.y >= b.y && p.y <= b.y + b.height) return o;
  }
  return null;
}

/** Patch que prende a cota ao objeto mantendo a ponta onde está. */
export function ligarAoObjeto(cota: SicroMeasurementObject, alvo: SicroObject, pxPerM: number): Partial<SicroMeasurementObject> {
  const a = ancoraDoObjeto(alvo, pxPerM);
  return { alvo_id: alvo.id, alvo_dx: cota.p2.x - a.x, alvo_dy: cota.p2.y - a.y };
}

const igual = (a: SicroPoint, b: SicroPoint) => Math.abs(a.x - b.x) < 1e-6 && Math.abs(a.y - b.y) < 1e-6;

/** Recalcula as pontas das cotas vivas; devolve a mesma lista se nada mudou. */
export function resolverCotasVivas(objects: SicroObject[], pxPerM: number): SicroObject[] {
  let mudou = false;
  const out = objects.map((o) => {
    if (o.kind !== "measurement" || (!o.ref_linha && !o.alvo_id)) return o;
    let p2 = o.p2;
    if (o.alvo_id) {
      const alvo = objects.find((x) => x.id === o.alvo_id);
      if (alvo) {
        const a = ancoraDoObjeto(alvo, pxPerM);
        p2 = { x: a.x + (o.alvo_dx ?? 0), y: a.y + (o.alvo_dy ?? 0) };
      }
    }
    let p1 = o.p1;
    if (o.ref_linha) {
      const ref = acharReferencial(objects, o.ref_linha);
      if (ref) p1 = peDaPerpendicular(ref.points, p2);
    }
    if (igual(p1, o.p1) && igual(p2, o.p2)) return o;
    mudou = true;
    return { ...o, p1, p2 };
  });
  return mudou ? out : objects;
}
