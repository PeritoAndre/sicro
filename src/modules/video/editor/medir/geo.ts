/**
 * Conta do lado da tela: referência → pontos de controle, homografia inversa para
 * desenhar a grade de 1 m, e posição × tempo para o gráfico. O cálculo oficial é do Rust.
 */

import type { ControlPoint, VideoSpeedCalibration } from "@domain/video_speed";
import type { Pt, RefDraft } from "./medirStore";

export const num = (s: string): number => {
  const n = Number(String(s).replace(",", ".").trim());
  return Number.isFinite(n) ? n : NaN;
};

export const fmt = (v: number, d = 1) => v.toFixed(d).replace(".", ",");

/** Entre-eixos médios (m) por tipo; o perito troca pelo do modelo exato. */
export const VEICULOS: [string, string, number][] = [
  ["hatch", "Hatch compacto", 2.5],
  ["sedan", "Sedan médio", 2.65],
  ["suv", "SUV médio", 2.7],
  ["pickup", "Pickup média", 3.1],
  ["van", "Van / furgão", 3.0],
  ["moto", "Moto", 1.4],
  ["caminhao", "Caminhão leve", 3.6],
  ["onibus", "Ônibus urbano", 6.0],
];

/** Cadência do tracejado (traço, espaço) em m. */
export const FILAS: Record<"urbano" | "rodovia", [number, number]> = { urbano: [2, 4], rodovia: [4, 12] };

/** Posição (m) do i-ésimo clique: início e fim de cada traço, alternados. */
export function filaPosicao(fila: "urbano" | "rodovia", i: number): number {
  const [t, e] = FILAS[fila];
  const k = Math.floor(i / 2);
  return k * (t + e) + (i % 2 === 1 ? t : 0);
}

/** Quantos cliques cada referência pede. */
export function pontosPedidos(kind: RefDraft["kind"]): { min: number; max: number } {
  switch (kind) {
    case "retangulo":
      return { min: 4, max: 4 };
    case "fila":
      return { min: 3, max: 12 };
    default:
      return { min: 2, max: 2 };
  }
}

/** Texto curto do que clicar agora (aparece sobre o quadro). */
export function dicaClique(ref: RefDraft): string | null {
  const n = ref.points.length;
  const { max } = pontosPedidos(ref.kind);
  if (n >= max) return null;
  switch (ref.kind) {
    case "retangulo":
      return `Canto ${n + 1} de 4: ${["perto, à esquerda", "perto, à direita", "longe, à direita", "longe, à esquerda"][n]}`;
    case "fila":
      return n % 2 === 0 ? `Clique ${n + 1}: começo de um traço` : `Clique ${n + 1}: fim do mesmo traço`;
    case "medida":
      return n === 0 ? "Primeira ponta da distância" : "Segunda ponta da distância";
    case "veiculo":
      return n === 0 ? "Onde a roda da frente toca o chão" : "Onde a roda de trás (mesmo lado) toca o chão";
  }
}

export interface RefBuild {
  method: "plane" | "line" | "cross_ratio";
  control_points: ControlPoint[];
  reference_source: "campo" | "norma_viaria" | "entre_eixos";
}

/** Monta a calibração a partir do rascunho; devolve o problema em texto se faltar algo. */
export function buildReference(ref: RefDraft): RefBuild | string {
  const p = ref.points;
  const fonte = ref.fonte;
  if (ref.kind === "retangulo") {
    if (p.length !== 4) return "Clique nos 4 cantos.";
    const L = num(ref.comprimento);
    const W = num(ref.largura);
    if (!(L > 0) || !(W > 0)) return "Informe o comprimento e a largura do retângulo.";
    const world = [[0, 0], [W, 0], [W, L], [0, L]];
    return {
      method: "plane",
      reference_source: fonte,
      control_points: p.map((q, i) => ({ px: q.x, py: q.y, world_x_m: world[i]![0]!, world_y_m: world[i]![1]!, label: "ABCD"[i] })),
    };
  }
  if (ref.kind === "fila") {
    if (p.length < 3) return "Clique em pelo menos 3 pontos das marcas.";
    const pos = p.map((_, i) => (ref.fila === "livre" ? num(ref.filaPos[i] ?? "") : filaPosicao(ref.fila, i)));
    if (pos.some((v) => !Number.isFinite(v))) return "Informe a posição (m) de cada ponto.";
    const sorted = [...pos].sort((a, b) => a - b);
    if (sorted.some((v, i) => i > 0 && Math.abs(v - sorted[i - 1]!) < 1e-9)) return "As posições precisam ser diferentes.";
    return {
      method: "cross_ratio",
      reference_source: fonte,
      control_points: p.map((q, i) => ({ px: q.x, py: q.y, world_x_m: pos[i]!, world_y_m: 0, label: String(i + 1) })),
    };
  }
  if (p.length !== 2) return "Clique nos 2 pontos.";
  const d = num(ref.kind === "veiculo" ? ref.entreEixos : ref.distancia);
  if (!(d > 0)) return ref.kind === "veiculo" ? "Informe o entre-eixos (m)." : "Informe a distância medida (m).";
  return {
    method: "line",
    reference_source: ref.kind === "veiculo" ? "entre_eixos" : fonte,
    control_points: [
      { px: p[0]!.x, py: p[0]!.y, world_x_m: 0, world_y_m: 0, label: "A" },
      { px: p[1]!.x, py: p[1]!.y, world_x_m: d, world_y_m: 0, label: "B" },
    ],
  };
}

// ---------------------------------------------------------------------------
// Homografia (row-major, px → m)

export function applyH(h: number[], x: number, y: number): Pt {
  const w = h[6]! * x + h[7]! * y + h[8]!;
  return { x: (h[0]! * x + h[1]! * y + h[2]!) / w, y: (h[3]! * x + h[4]! * y + h[5]!) / w };
}

export function invert3(m: number[]): number[] | null {
  const [a, b, c, d, e, f, g, h, i] = m as [number, number, number, number, number, number, number, number, number];
  const A = e * i - f * h;
  const B = -(d * i - f * g);
  const C = d * h - e * g;
  const det = a * A + b * B + c * C;
  if (!Number.isFinite(det) || Math.abs(det) < 1e-15) return null;
  const k = 1 / det;
  return [
    A * k, -(b * i - c * h) * k, (b * f - c * e) * k,
    B * k, (a * i - c * g) * k, -(a * f - c * d) * k,
    C * k, -(a * h - b * g) * k, (a * e - b * d) * k,
  ];
}

/** Linhas (em px do quadro) para conferir a referência: grade de 1 m (plano) ou régua (linha/fila). */
export function guideLines(cal: VideoSpeedCalibration): { points: number[]; strong?: boolean }[] {
  const inv = invert3(cal.homography);
  if (!inv) return [];
  const toPx = (x: number, y: number) => applyH(inv, x, y);
  const xs = cal.control_points.map((c) => c.world_x_m);
  const ys = cal.control_points.map((c) => c.world_y_m);
  const out: { points: number[]; strong?: boolean }[] = [];
  if (cal.method === "plane") {
    const x0 = Math.floor(Math.min(...xs)) - 3;
    const x1 = Math.ceil(Math.max(...xs)) + 3;
    const y0 = Math.floor(Math.min(...ys)) - 8;
    const y1 = Math.ceil(Math.max(...ys)) + 8;
    for (let x = x0; x <= x1; x++) {
      const pts: number[] = [];
      for (let y = y0; y <= y1; y += 0.5) {
        const p = toPx(x, y);
        pts.push(p.x, p.y);
      }
      out.push({ points: pts });
    }
    for (let y = y0; y <= y1; y++) {
      const a = toPx(x0, y);
      const b = toPx(x1, y);
      out.push({ points: [a.x, a.y, b.x, b.y] });
    }
    const poly = cal.control_points.flatMap((c) => [c.px, c.py]);
    out.push({ points: [...poly, poly[0]!, poly[1]!], strong: true });
    return out;
  }
  // Régua ao longo da referência: um traço por metro.
  const a = Math.floor(Math.min(...xs)) - 2;
  const b = Math.ceil(Math.max(...xs)) + 2;
  const p0 = toPx(a, 0);
  const p1 = toPx(b, 0);
  out.push({ points: [p0.x, p0.y, p1.x, p1.y], strong: true });
  for (let x = a; x <= b; x++) {
    const p = toPx(x, 0);
    const q = toPx(x + 0.001, 0);
    const len = Math.hypot(q.x - p.x, q.y - p.y) || 1;
    const nx = -(q.y - p.y) / len;
    const ny = (q.x - p.x) / len;
    const k = x % 5 === 0 ? 9 : 5;
    out.push({ points: [p.x - nx * k, p.y - ny * k, p.x + nx * k, p.y + ny * k] });
  }
  return out;
}

/** Posição ao longo do movimento (m) por tempo (s), para o gráfico. */
export function serie(cal: VideoSpeedCalibration, pts: { px: number; py: number; actual_timestamp_s: number }[]): { t: number[]; s: number[]; v: number; b0: number } | null {
  if (pts.length < 2) return null;
  const P = pts.map((p) => applyH(cal.homography, p.px, p.py));
  const t = pts.map((p) => p.actual_timestamp_s);
  const first = P[0]!;
  const last = P[P.length - 1]!;
  const dx = last.x - first.x;
  const dy = last.y - first.y;
  const dl = Math.hypot(dx, dy) || 1;
  const s = P.map((q) => ((q.x - first.x) * dx + (q.y - first.y) * dy) / dl);
  const n = t.length;
  const mt = t.reduce((x, y) => x + y, 0) / n;
  const ms = s.reduce((x, y) => x + y, 0) / n;
  let sxx = 0;
  let sxy = 0;
  for (let i = 0; i < n; i++) {
    sxx += (t[i]! - mt) ** 2;
    sxy += (t[i]! - mt) * (s[i]! - ms);
  }
  const v = sxx > 0 ? sxy / sxx : 0;
  return { t, s, v, b0: ms - v * mt };
}
