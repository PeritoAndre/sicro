/**
 * Desenho da pessoa no canvas: caixa fina, seta e número. Mesmas medidas de
 * `render.rs` (em pixels do vídeo) — o que se vê na tela é o que sai no MP4.
 */
import type { ModoDesenho } from "@domain/video_pessoas";
import { DIGITOS } from "./digitFont";
import { GLIFOS, textoEtiqueta } from "./fonteEtiqueta";
import type { Caixa } from "./pessoasMath";

export function corContraste(hex: string): string {
  const h = hex.replace("#", "");
  const v = parseInt(h.length === 6 ? h : "e6194b", 16);
  const l = 0.299 * ((v >> 16) & 255) + 0.587 * ((v >> 8) & 255) + 0.114 * (v & 255);
  return l > 150 ? "#141414" : "#ffffff";
}

/** Unidade de medida: altura do vídeo / 720 × tamanho escolhido. */
export const unidade = (alturaVideo: number, fonte: number) =>
  (alturaVideo / 720) * Math.min(4, Math.max(0.4, fonte));

function retArredondado(ctx: CanvasRenderingContext2D, x0: number, y0: number, x1: number, y1: number, r: number) {
  const rr = Math.min(r, (x1 - x0) / 2, (y1 - y0) / 2);
  ctx.beginPath();
  ctx.moveTo(x0 + rr, y0);
  ctx.arcTo(x1, y0, x1, y1, rr);
  ctx.arcTo(x1, y1, x0, y1, rr);
  ctx.arcTo(x0, y1, x0, y0, rr);
  ctx.arcTo(x0, y0, x1, y0, rr);
  ctx.closePath();
}

/** Desenha no espaço do vídeo (o chamador já aplicou a transformação para a tela). */
export function desenharPessoa(
  ctx: CanvasRenderingContext2D,
  c: Caixa,
  numero: number,
  cor: string,
  modo: ModoDesenho,
  video: { w: number; h: number },
  fonte: number,
  destaque = false,
  rotulo: string | null = null,
) {
  const alturaVideo = video.h;
  const u = unidade(alturaVideo, fonte);
  const [x0, y0, x1] = [c.x, c.y, c.x + c.w];
  if (modo === "completo" || destaque) {
    ctx.lineWidth = Math.max(1, (destaque ? 2.4 : 1.6) * u);
    ctx.strokeStyle = cor;
    ctx.lineJoin = "miter";
    ctx.strokeRect(x0 + ctx.lineWidth / 2, y0 + ctx.lineWidth / 2, c.w - ctx.lineWidth, c.h - ctx.lineWidth);
  }
  const cx = (x0 + x1) / 2;
  const [sl, sa] = [16 * u, 12 * u];
  const dig = String(Math.max(0, numero)).length;
  const [bh, bw] = [24 * u, (dig * 14 + 12) * u];
  const folga = 3 * u;
  const comSeta = modo !== "numero";
  const topoSeta = y0 - folga - sa;
  if (comSeta) {
    const tri = (ox: number, oy: number, larg: number, alt: number, fill: string) => {
      ctx.fillStyle = fill;
      ctx.beginPath();
      ctx.moveTo(ox - larg / 2, oy);
      ctx.lineTo(ox + larg / 2, oy);
      ctx.lineTo(ox, oy + alt);
      ctx.closePath();
      ctx.fill();
    };
    tri(cx, topoSeta + 0.8 * u, sl + 3 * u, sa + 1 * u, "#141414");
    tri(cx, topoSeta, sl, sa, cor);
  }
  const by1 = comSeta ? topoSeta : y0 - folga;
  const by0 = by1 - bh;
  const bx0 = Math.min(Math.max(cx - bw / 2, 0), Math.max(0, video.w - bw));
  ctx.fillStyle = "#141414";
  retArredondado(ctx, bx0 - u, by0 - u, bx0 + bw + u, by1 + u, 6 * u);
  ctx.fill();
  ctx.fillStyle = cor;
  retArredondado(ctx, bx0, by0, bx0 + bw, by1, 5 * u);
  ctx.fill();
  // número
  const alt = 14 * u;
  const s = alt / 4;
  const texto = String(Math.max(0, numero));
  const avanco = 2.7 * s;
  const total = avanco * texto.length - 0.7 * s;
  let x = bx0 + bw / 2 - total / 2;
  const topo = (by0 + by1) / 2 - alt / 2;
  ctx.strokeStyle = corContraste(cor);
  ctx.lineWidth = 0.62 * s;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  for (const ch of texto) {
    const d = DIGITOS[Number(ch)] ?? DIGITOS[0]!;
    for (const linha of d) {
      ctx.beginPath();
      linha.forEach(([px, py], i) => {
        const X = x + px * s;
        const Y = topo + py * s;
        if (i === 0) ctx.moveTo(X, Y);
        else ctx.lineTo(X, Y);
      });
      ctx.stroke();
    }
    x += avanco;
  }
  ctx.lineCap = "butt";
  if (rotulo && rotulo.trim()) etiqueta(ctx, textoEtiqueta(rotulo), bx0, bw, by0, by1, u, video.w);
}

/** Nome da classe ao lado do número (à direita; à esquerda se não couber), na mesma fonte 5×7 do vídeo gravado. */
function etiqueta(
  ctx: CanvasRenderingContext2D,
  texto: string,
  bx0: number,
  bw: number,
  by0: number,
  by1: number,
  u: number,
  larguraVideo: number,
) {
  const p = 2 * u;
  const n = [...texto].length;
  const lw = n * 6 * p - p + 2 * 3 * u + 2 * p;
  const lx0 = bx0 + bw + u + lw <= larguraVideo ? bx0 + bw + u : bx0 - u - lw;
  ctx.fillStyle = "#141414";
  retArredondado(ctx, lx0, by0, lx0 + lw, by1, 5 * u);
  ctx.fill();
  ctx.fillStyle = "#ffffff";
  let x = lx0 + 3 * u + p;
  const y = (by0 + by1) / 2 - 3.5 * p;
  for (const ch of texto) {
    const g = GLIFOS[ch];
    if (g) {
      g.forEach((linha, ly) => {
        for (let lx = 0; lx < linha.length; lx++) {
          if (linha[lx] === "1") ctx.fillRect(x + lx * p, y + ly * p, p, p);
        }
      });
    }
    x += 6 * p;
  }
}
