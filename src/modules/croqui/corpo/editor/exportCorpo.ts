/**
 * PNG técnico do croqui corporal: cabeçalho + prancha com marcadores + legenda
 * numerada, numa imagem só (entra no laudo como figura, no estilo do stampPng
 * do croqui de via). O rodapé avisa que é esquema ilustrativo, sem escala.
 */

import type { LegendRow } from "../engine";

interface CorpoStampMeta {
  title: string;
  occurrence: {
    numero_bo?: string | null;
    tipo_pericia?: string | null;
    municipio?: string | null;
  } | null;
  templateLabel: string;
  timestamp: Date;
  /** Listas de regiões do POP (colunas entre a prancha e a legenda); só
   *  pranchas numeradas enviam. */
  regionLists?: Array<{ title: string; items: string[] }>;
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const im = new Image();
    im.onload = () => resolve(im);
    im.onerror = reject;
    im.src = src;
  });
}

/** Bounding box dos pixels não-brancos: descarta a sobra branca da arte pra
 *  figura aproveitar a página. */
function contentBBox(
  img: HTMLImageElement,
  margin: number,
): { x: number; y: number; w: number; h: number } {
  const full = { x: 0, y: 0, w: img.width, h: img.height };
  const c = document.createElement("canvas");
  c.width = img.width;
  c.height = img.height;
  const cx = c.getContext("2d");
  if (!cx) return full;
  cx.drawImage(img, 0, 0);
  let data: Uint8ClampedArray;
  try {
    data = cx.getImageData(0, 0, c.width, c.height).data;
  } catch {
    return full; // canvas tainted (não deve ocorrer com data URL) → sem corte
  }
  let minX = c.width;
  let minY = c.height;
  let maxX = 0;
  let maxY = 0;
  let found = false;
  const step = 2; // amostragem (rápido; precisão de 2px é suficiente)
  for (let yy = 0; yy < c.height; yy += step) {
    for (let xx = 0; xx < c.width; xx += step) {
      const i = (yy * c.width + xx) * 4;
      const a = data[i + 3] ?? 0;
      const r = data[i] ?? 255;
      const g = data[i + 1] ?? 255;
      const b = data[i + 2] ?? 255;
      if (a > 8 && (r < 246 || g < 246 || b < 246)) {
        if (xx < minX) minX = xx;
        if (xx > maxX) maxX = xx;
        if (yy < minY) minY = yy;
        if (yy > maxY) maxY = yy;
        found = true;
      }
    }
  }
  if (!found) return full;
  const x = Math.max(0, minX - margin);
  const y = Math.max(0, minY - margin);
  return {
    x,
    y,
    w: Math.min(img.width - x, maxX - minX + 1 + margin * 2),
    h: Math.min(img.height - y, maxY - minY + 1 + margin * 2),
  };
}

function fmtDate(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getDate())}/${p(d.getMonth() + 1)}/${d.getFullYear()} ${p(
    d.getHours(),
  )}:${p(d.getMinutes())}`;
}

const HEADER_H = 64;
const FOOTER_H = 30;
const PAD = 20;
const ROW_H = 26;
const LEGEND_HEADER_H = 30;
const MIN_W = 760;
/** Margem ao redor do conteúdo recortado (px da arte nativa). */
const BODY_MARGIN = 18;
/** Largura-alvo da prancha recortada, pra figura preencher a página. */
const BODY_TARGET_W = 820;

/**
 * @param bodyPng data URL da prancha (CorpoCanvas.toPng)
 * @param legend  linhas já numeradas/ordenadas (buildLegend)
 */
export async function stampCorpoPng(
  bodyPng: string,
  legend: LegendRow[],
  meta: CorpoStampMeta,
): Promise<string> {
  const body = await loadImage(bodyPng);
  // Recorta ao conteúdo e escala pra largura-alvo.
  const bb = contentBBox(body, BODY_MARGIN);
  const bodyScale = BODY_TARGET_W / bb.w;
  const bodyDrawW = BODY_TARGET_W;
  const bodyDrawH = Math.round(bb.h * bodyScale);

  const legendBlockH =
    legend.length > 0 ? LEGEND_HEADER_H + legend.length * ROW_H + PAD : 0;
  const REGION_LINE_H = 16;
  const regionLists = meta.regionLists ?? [];
  const regionRows =
    regionLists.length > 0
      ? Math.max(...regionLists.map((l) => l.items.length))
      : 0;
  // +1 linha = título da coluna ("FRENTE…"/"COSTAS…").
  const regionBlockH =
    regionLists.length > 0
      ? LEGEND_HEADER_H + (regionRows + 1) * REGION_LINE_H + PAD
      : 0;
  const canvasW = Math.max(MIN_W, bodyDrawW + PAD * 2);
  const canvasH =
    HEADER_H + bodyDrawH + PAD + regionBlockH + legendBlockH + FOOTER_H;

  const canvas = document.createElement("canvas");
  canvas.width = canvasW;
  canvas.height = canvasH;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("canvas 2d indisponível");
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";

  // Fundo
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, canvasW, canvasH);

  // Cabeçalho
  ctx.fillStyle = "#0f172a";
  ctx.fillRect(0, 0, canvasW, HEADER_H);
  ctx.fillStyle = "#f8fafc";
  ctx.font = "bold 17px Inter, system-ui, sans-serif";
  ctx.textAlign = "left";
  ctx.textBaseline = "middle";
  ctx.fillText(meta.title, PAD, HEADER_H / 2 - 8);
  ctx.font = "12px Inter, system-ui, sans-serif";
  ctx.fillStyle = "#cbd5e1";
  const sub: string[] = [`Carta de lesões — ${meta.templateLabel}`];
  if (meta.occurrence?.numero_bo) sub.push(`BO ${meta.occurrence.numero_bo}`);
  if (meta.occurrence?.municipio) sub.push(meta.occurrence.municipio);
  ctx.fillText(sub.join(" · "), PAD, HEADER_H / 2 + 12);
  ctx.textAlign = "right";
  ctx.fillText(`Exportado em ${fmtDate(meta.timestamp)}`, canvasW - PAD, HEADER_H / 2);

  // Prancha (centralizada)
  const bodyX = (canvasW - bodyDrawW) / 2;
  ctx.drawImage(body, bb.x, bb.y, bb.w, bb.h, bodyX, HEADER_H, bodyDrawW, bodyDrawH);

  let y = HEADER_H + bodyDrawH + PAD;

  // Listas de regiões do POP (colunas lado a lado)
  if (regionLists.length > 0) {
    ctx.textAlign = "left";
    ctx.fillStyle = "#0f172a";
    ctx.font = "bold 13px Inter, system-ui, sans-serif";
    ctx.fillText(
      "Regiões anatômicas — POP/SENASP (Anexo 1)",
      PAD,
      y + 6,
    );
    y += LEGEND_HEADER_H;

    const colW = (canvasW - PAD * 2) / regionLists.length;
    regionLists.forEach((list, ci) => {
      const x = PAD + ci * colW;
      ctx.fillStyle = "#475569";
      ctx.font = "bold 11px Inter, system-ui, sans-serif";
      ctx.fillText(list.title, x, y + 4);
      ctx.font = "11px Inter, system-ui, sans-serif";
      list.items.forEach((item, ri) => {
        ctx.fillText(item, x, y + 4 + (ri + 1) * REGION_LINE_H);
      });
    });
    y += (regionRows + 1) * REGION_LINE_H + PAD;
  }

  // Legenda
  if (legend.length > 0) {
    ctx.textAlign = "left";
    ctx.fillStyle = "#0f172a";
    ctx.font = "bold 13px Inter, system-ui, sans-serif";
    ctx.fillText("Legenda das lesões / achados", PAD, y + 6);
    y += LEGEND_HEADER_H;

    for (const row of legend) {
      // Badge numerado colorido
      const cx = PAD + 11;
      const cy = y + ROW_H / 2;
      ctx.beginPath();
      ctx.arc(cx, cy, 10, 0, Math.PI * 2);
      ctx.fillStyle = row.color;
      ctx.fill();
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = "#ffffff";
      ctx.stroke();
      ctx.fillStyle = "#ffffff";
      ctx.font = "bold 11px Inter, system-ui, sans-serif";
      ctx.textAlign = "center";
      ctx.fillText(String(row.number), cx, cy);

      // Texto da linha
      ctx.textAlign = "left";
      ctx.fillStyle = "#0f172a";
      ctx.font = "12px Inter, system-ui, sans-serif";
      const parts = [row.tipo];
      if (row.regiao) parts.push(row.regiao);
      if (row.instrumento) parts.push(row.instrumento);
      if (row.dimensoes) parts.push(row.dimensoes);
      let line = parts.join(" — ");
      if (row.observacao) line += ` (${row.observacao})`;
      // Trunca p/ caber na largura
      const maxW = canvasW - (PAD + 28) - PAD;
      while (ctx.measureText(line).width > maxW && line.length > 4) {
        line = line.slice(0, -2);
      }
      ctx.fillText(line, PAD + 28, cy);

      // separador
      ctx.strokeStyle = "#e2e8f0";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(PAD, y + ROW_H);
      ctx.lineTo(canvasW - PAD, y + ROW_H);
      ctx.stroke();
      y += ROW_H;
    }
    y += PAD;
  }

  // Rodapé
  ctx.fillStyle = "#1f2937";
  ctx.fillRect(0, canvasH - FOOTER_H, canvasW, FOOTER_H);
  ctx.fillStyle = "#94a3b8";
  ctx.font = "10px Inter, system-ui, sans-serif";
  ctx.textAlign = "left";
  ctx.fillText(
    "SICRO Desktop — Croqui corporal · esquema ilustrativo (não escala métrica) · documento técnico sujeito a revisão pelo perito.",
    PAD,
    canvasH - FOOTER_H / 2,
  );

  return canvas.toDataURL("image/png");
}
