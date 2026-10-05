/** Tarja de anonimização: desfoque, pixelização ou tarja preta, em retângulo ou elipse. */

import type { SicroAnnotation } from "../engine";

export type RedactionStyle = "blur" | "pixelate" | "solid";
export type RedactionShape = "rect" | "ellipse" | "free";

/** Fonte de pixels da prévia: px da imagem = (mundo + offset) × scale. */
export interface RedactionBase {
  image: CanvasImageSource & { width: number; height: number };
  scale: number;
  offsetX: number;
  offsetY: number;
}

/** Docs antigos (antes do desfoque) eram tarja preta retangular. */
export function redactionOpts(a: SicroAnnotation) {
  return {
    style: (a.redaction_style ?? "solid") as RedactionStyle,
    shape: (a.redaction_shape ?? "rect") as RedactionShape,
    strength: Math.max(10, Math.min(100, a.redaction_strength ?? 60)),
  };
}

/** Retângulo com largura/altura positivas (a tarja pode ter sido desenhada "para trás"); na livre, o envelope dos pontos. */
export function redactionRect(a: SicroAnnotation) {
  if (a.redaction_shape === "free" && a.points && a.points.length >= 3) {
    const xs = a.points.map((p) => p.x);
    const ys = a.points.map((p) => p.y);
    const x = Math.min(...xs);
    const y = Math.min(...ys);
    return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y };
  }
  const w = a.width ?? 0;
  const h = a.height ?? 0;
  return { x: w < 0 ? a.x + w : a.x, y: h < 0 ? a.y + h : a.y, w: Math.abs(w), h: Math.abs(h) };
}

/** Raio do desfoque / lado do bloco em px, proporcionais à área e à intensidade (10–100). */
function amount(style: RedactionStyle, w: number, h: number, strength: number): number {
  const m = Math.min(w, h);
  const k = strength / 100;
  return style === "blur" ? Math.max(3, Math.round(m * (0.03 + 0.12 * k))) : Math.max(4, Math.round(m * (0.04 + 0.16 * k)));
}

function blurPass(src: Uint8ClampedArray, dst: Uint8ClampedArray, w: number, h: number, r: number, horizontal: boolean) {
  const div = 2 * r + 1;
  const outer = horizontal ? h : w;
  const inner = horizontal ? w : h;
  for (let o = 0; o < outer; o++) {
    const at = (i: number) => (horizontal ? (o * w + i) * 4 : (i * w + o) * 4);
    for (let c = 0; c < 4; c++) {
      let acc = 0;
      for (let i = -r; i <= r; i++) acc += src[at(Math.min(inner - 1, Math.max(0, i)))! + c]!;
      for (let i = 0; i < inner; i++) {
        dst[at(i) + c] = acc / div;
        acc += src[at(Math.min(inner - 1, i + r + 1)) + c]! - src[at(Math.max(0, i - r)) + c]!;
      }
    }
  }
}

/** Três caixas separáveis ≈ gaussiano. */
export function boxBlur(data: Uint8ClampedArray, w: number, h: number, r: number) {
  const tmp = new Uint8ClampedArray(data.length);
  for (let p = 0; p < 3; p++) {
    blurPass(data, tmp, w, h, r, true);
    blurPass(tmp, data, w, h, r, false);
  }
}

export function pixelate(data: Uint8ClampedArray, w: number, h: number, block: number, ox: number, oy: number) {
  // Grade ancorada no canto da tarja (ox, oy dentro do recorte), não no do recorte.
  const x0 = ((ox % block) + block) % block - block;
  const y0 = ((oy % block) + block) % block - block;
  for (let by = y0; by < h; by += block) {
    for (let bx = x0; bx < w; bx += block) {
      const xs = Math.max(0, bx);
      const ys = Math.max(0, by);
      const xe = Math.min(w, bx + block);
      const ye = Math.min(h, by + block);
      let r = 0, g = 0, b = 0, a = 0, n = 0;
      for (let y = ys; y < ye; y++) {
        for (let x = xs; x < xe; x++) {
          const i = (y * w + x) * 4;
          r += data[i]!; g += data[i + 1]!; b += data[i + 2]!; a += data[i + 3]!; n++;
        }
      }
      if (!n) continue;
      for (let y = ys; y < ye; y++) {
        for (let x = xs; x < xe; x++) {
          const i = (y * w + x) * 4;
          data[i] = r / n; data[i + 1] = g / n; data[i + 2] = b / n; data[i + 3] = a / n;
        }
      }
    }
  }
}

/**
 * Recorte processado da área pedida (px de `src`), limitado ao que existe na imagem: devolve o
 * canvas e o retângulo que ele cobre. O desfoque lê uma margem em volta para a borda não clarear.
 */
export function processedPatch(
  src: CanvasImageSource & { width: number; height: number },
  x: number,
  y: number,
  w: number,
  h: number,
  style: RedactionStyle,
  strength: number,
  opts: {
    /** Teto do lado maior do cálculo (prévia em resolução de tela; a exportação é no Rust). */
    maxSide?: number;
    /** Tamanho que define raio/bloco (a forma, não a área com folga). */
    basis?: { w: number; h: number };
  } = {},
): { canvas: HTMLCanvasElement; x: number; y: number; w: number; h: number } | null {
  const W = (src as HTMLImageElement).naturalWidth || src.width;
  const H = (src as HTMLImageElement).naturalHeight || src.height;
  if (style === "solid" || w < 2 || h < 2) return null;
  // Parte da área que existe na imagem.
  const ox0 = Math.max(0, Math.floor(x)), oy0 = Math.max(0, Math.floor(y));
  const ox1 = Math.min(W, Math.ceil(x + w)), oy1 = Math.min(H, Math.ceil(y + h));
  if (ox1 - ox0 < 2 || oy1 - oy0 < 2) return null;
  const amt = amount(style, opts.basis?.w ?? w, opts.basis?.h ?? h, strength);
  const m = style === "blur" ? amt * 3 : 0;
  const sx = Math.max(0, ox0 - m), sy = Math.max(0, oy0 - m);
  const ex = Math.min(W, ox1 + m), ey = Math.min(H, oy1 + m);
  const sw = ex - sx, sh = ey - sy;
  const s = Math.min(1, (opts.maxSide ?? Infinity) / Math.max(sw, sh));
  const tw = Math.max(1, Math.round(sw * s)), th = Math.max(1, Math.round(sh * s));
  const fx = tw / sw, fy = th / sh;
  const work = document.createElement("canvas");
  work.width = tw;
  work.height = th;
  const wctx = work.getContext("2d", { willReadFrequently: true });
  if (!wctx) return null;
  wctx.drawImage(src, sx, sy, sw, sh, 0, 0, tw, th);
  const img = wctx.getImageData(0, 0, tw, th);
  const a = Math.max(1, Math.round(amt * Math.min(fx, fy)));
  if (style === "blur") boxBlur(img.data, tw, th, a);
  else pixelate(img.data, tw, th, Math.max(2, a), Math.round((x - sx) * fx), Math.round((y - sy) * fy));
  wctx.putImageData(img, 0, 0);
  // Cópia sempre dentro do canvas de trabalho (o WebKit desenha só parte se a origem sair dele).
  const cx = (ox0 - sx) * fx, cy = (oy0 - sy) * fy;
  const cw = Math.min(tw - cx, (ox1 - ox0) * fx), ch = Math.min(th - cy, (oy1 - oy0) * fy);
  const out = document.createElement("canvas");
  out.width = Math.max(1, Math.round(cw));
  out.height = Math.max(1, Math.round(ch));
  out.getContext("2d")?.drawImage(work, cx, cy, cw, ch, 0, 0, out.width, out.height);
  return { canvas: out, x: ox0, y: oy0, w: ox1 - ox0, h: oy1 - oy0 };
}

/** Contorno da tarja com origem no canto do envelope; `pts` (relativos) só na forma livre. */
export function shapePath(
  ctx: CanvasRenderingContext2D,
  shape: RedactionShape,
  w: number,
  h: number,
  pts?: { x: number; y: number }[],
) {
  ctx.beginPath();
  if (shape === "free" && pts && pts.length >= 3) {
    ctx.moveTo(pts[0]!.x, pts[0]!.y);
    for (const p of pts.slice(1)) ctx.lineTo(p.x, p.y);
    ctx.closePath();
  } else if (shape === "ellipse") ctx.ellipse(w / 2, h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
  else ctx.rect(0, 0, w, h);
}

/** Pontos relativos ao canto do envelope. */
export function relativePoints(a: SicroAnnotation, r: { x: number; y: number }) {
  return (a.points ?? []).map((p) => ({ x: p.x - r.x, y: p.y - r.y }));
}

/** Retângulo/elipse viram contorno livre com muitos pontos, para moldar ponto a ponto. */
export function toMoldablePoints(a: SicroAnnotation): { x: number; y: number }[] {
  const { x, y, w, h } = redactionRect(a);
  if (a.redaction_shape === "ellipse") {
    const n = 32;
    return Array.from({ length: n }, (_, i) => {
      const t = (i / n) * Math.PI * 2;
      return { x: x + w / 2 + (w / 2) * Math.cos(t), y: y + h / 2 + (h / 2) * Math.sin(t) };
    });
  }
  const per = 6;
  const corners = [
    [x, y],
    [x + w, y],
    [x + w, y + h],
    [x, y + h],
  ] as const;
  const out: { x: number; y: number }[] = [];
  corners.forEach(([cx, cy], i) => {
    const [nx, ny] = corners[(i + 1) % 4]!;
    for (let k = 0; k < per; k++) out.push({ x: cx + ((nx - cx) * k) / per, y: cy + ((ny - cy) * k) / per });
  });
  return out;
}

/** Tarja no formato do backend (a exportação aplica em Rust, em resolução cheia). */
export function redactionSpec(a: SicroAnnotation) {
  const { x, y, w, h } = redactionRect(a);
  const { style, shape, strength } = redactionOpts(a);
  return {
    style,
    shape,
    x,
    y,
    width: w,
    height: h,
    ...(shape === "free" ? { points: (a.points ?? []).map((p) => ({ x: p.x, y: p.y })) } : {}),
    strength,
  };
}
