/**
 * Cursor ↔ slot da linha (D1): a cabeça de leitura no cursor da store e o cursor que um clique na linha põe. Com o
 * palco no modo clipe (enquanto a prévia viva não liga), o clique vira cursor de CLIPE: o clipe da principal sob o
 * slot e o meio do quadro exibido nele (a convenção do palco). Puro. Dono: P4.1.
 */

import type { Cursor, Id, LinhaResolvida, ProjetoEdicao, Seg } from "@domain/edicao";
import { clipeConhecido } from "../modelo/projeto";
import { exibido, type ContextoTempo } from "../modelo/tempo";
import { meioDoQuadro } from "../palco/faixa";
import { donoDoClipe, quadroNoSlot, tempoDoSlot, type Dono } from "./modelo";

/** Slot local do instante `t` (base do dono): o do quadro exibido em `t` (o primeiro slot dele); `fora` = `t` fora do
 *  clipe (a cabeça fica na ponta, apagada). */
export function slotNoTempo(d: Dono, t: Seg): { s: number; fora: boolean } {
  if (d.n_q <= 0) return { s: 0, fora: true };
  const ult = d.n_q - 1;
  if (d.base === "local") {
    const v = (t * d.fps.num) / d.fps.den;
    const s = v < 0 ? -Math.round(-v) : Math.round(v);
    return { s: Math.min(Math.max(s, 0), ult), fora: !(s >= 0 && s <= ult) };
  }
  if (!d.mapa) {
    const s = Math.round(((t - d.inicio_s) / d.fator) * (d.fps.num / d.fps.den));
    return { s: Math.min(Math.max(s, 0), ult), fora: !(s >= 0 && s <= ult) };
  }
  const j = exibido(d.mapa.tab, t);
  if (quadroNoSlot(d.mapa, 0) > j) return { s: 0, fora: true };
  if (quadroNoSlot(d.mapa, ult) < j) return { s: ult, fora: true };
  let lo = 0;
  let hi = ult;
  while (lo < hi) {
    const m = (lo + hi) >>> 1;
    if (quadroNoSlot(d.mapa, m) >= j) hi = m;
    else lo = m + 1;
  }
  return { s: lo, fora: false };
}

const clipeDe = (doc: ProjetoEdicao, id: Id) => {
  const c = doc.principal.find((x) => (x as { id?: Id }).id === id);
  return c && clipeConhecido(c) ? c : null;
};

/** Cabeça de leitura (slot do programa) no cursor; null = o clipe do cursor não está na linha. */
export function cabecaNoCursor(doc: ProjetoEdicao, linha: LinhaResolvida, cursor: Cursor, ctx: ContextoTempo): { q: number; fora: boolean } | null {
  if (cursor.modo === "programa") return { q: cursor.q, fora: false };
  const rc = linha.clipes.find((x) => x.id === cursor.clipe);
  const c = rc ? clipeDe(doc, rc.id) : null;
  if (!rc || !c) return null;
  const r = slotNoTempo(donoDoClipe(c, rc, ctx, linha.fps), cursor.t);
  return { q: rc.inicio_q + r.s, fora: r.fora };
}

/** Clipe da principal sob o slot: o último que começa até ele (na transição, o que entra); antes do começo, o primeiro. */
export function clipeNoSlot(linha: LinhaResolvida, q: number): Id | null {
  let a = linha.clipes[0]?.id ?? null;
  for (const c of linha.clipes) if (c.inicio_q <= q) a = c.id;
  return a;
}

/** Tempo na base do clipe que o cursor guarda para o slot local `s` (fonte: meio do quadro; fora do clipe, a conta
 *  segue pela velocidade). */
function tempoDoCursor(d: Dono, s: number): Seg {
  if (d.base === "fonte" && d.mapa) {
    const t = tempoDoSlot(d, s);
    return meioDoQuadro(d.mapa.tab, s >= 0 && s < d.n_q ? quadroNoSlot(d.mapa, s) : exibido(d.mapa.tab, t));
  }
  return tempoDoSlot(d, s);
}

/** Cursor de clipe no slot `q` (`clipe` dado: nele, mesmo fora; senão o clipe sob `q`, preso a ele). */
export function cursorNoSlot(doc: ProjetoEdicao, linha: LinhaResolvida, q: number, ctx: ContextoTempo, clipe?: Id | null, livre = false): Cursor | null {
  const id = clipe ?? clipeNoSlot(linha, q);
  const rc = id ? linha.clipes.find((x) => x.id === id) : null;
  const c = rc ? clipeDe(doc, rc.id) : null;
  if (!rc || !c) return null;
  const d = donoDoClipe(c, rc, ctx, linha.fps);
  const s = livre ? q - rc.inicio_q : Math.min(Math.max(q - rc.inicio_q, 0), Math.max(rc.n_q - 1, 0));
  return { modo: "clipe", clipe: rc.id, t: tempoDoCursor(d, s) };
}
