/**
 * Prancha técnica do croqui de PLANTA — composição (Canvas 2D) + export.
 *
 * Recebe a imagem da planta (capturada do Pixi via app.view.toDataURL) e compõe
 * uma PRANCHA TÉCNICA: cabeçalho do caso + planta + barra de escala + rosa dos
 * ventos + legenda automática dos vestígios + rodapé §13. Espelha o padrão de
 * `corpo/editor/exportCorpo.ts`. O PNG resultante é salvo via `exportCroquiPng`
 * (vira inserível no laudo) e pode ir pra um PDF A4 (view de impressão).
 */
import { evidenceMeta, evidenceLabelFor, type EvidenceLabelKind } from "../evidence";
import type { PlantaEvidenceMarker } from "../schema";

export interface PlantaStampMeta {
  title: string;
  occurrence: {
    numero_bo?: string | null;
    tipo_pericia?: string | null;
    municipio?: string | null;
  } | null;
  timestamp: Date;
  compassDeg: number;
  labelKind: EvidenceLabelKind;
}

interface LegendRow {
  label: string;
  color: string;
  tipo: string; // nome por extenso
  descricao: string;
}

function fmtDate(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getDate())}/${p(d.getMonth() + 1)}/${d.getFullYear()} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = reject;
    img.src = src;
  });
}

/** Constrói as linhas da legenda (rótulo derivado da ORDEM). */
export function buildLegendRows(
  evidences: PlantaEvidenceMarker[],
  labelKind: EvidenceLabelKind,
): LegendRow[] {
  return evidences.map((ev, i) => {
    const meta = evidenceMeta(ev.tipo);
    return {
      label: evidenceLabelFor(i + 1, labelKind),
      color: ev.cor || meta.color,
      tipo: meta.label,
      descricao: (ev.descricao ?? "").trim(),
    };
  });
}

/** Escolhe um comprimento "redondo" (m) cuja barra fique entre 60–150 px. */
function niceScaleLength(pxPerM: number): number {
  const candidates = [0.25, 0.5, 1, 2, 5, 10, 20, 50, 100];
  for (const m of candidates) {
    const px = m * pxPerM;
    if (px >= 60 && px <= 150) return m;
  }
  // fallback: alvo ~100px
  return Math.max(0.25, Math.round((100 / pxPerM) * 4) / 4);
}

/**
 * Compõe a prancha técnica. `imgPxPerM` = px/metro NA IMAGEM capturada (a escala
 * de tela vira exata aqui, independente de dpr). Retorna data URL PNG.
 */
export async function stampPlantaPng(
  planDataUrl: string,
  imgPxPerM: number,
  meta: PlantaStampMeta,
  legend: LegendRow[],
): Promise<string> {
  const img = await loadImage(planDataUrl);

  const PAD = 24;
  const HEADER_H = 72;
  const FOOTER_H = 30;
  const ROW_H = 24;
  const LEGEND_HEAD_H = 28;
  const MIN_W = 920;

  // Planta desenhada com largura-alvo (mantém proporção).
  const planDrawW = Math.min(img.width || MIN_W, 1120);
  const drawScale = (img.width ? planDrawW / img.width : 1) || 1;
  const planDrawH = (img.height || 600) * drawScale;
  const platePxPerM = imgPxPerM * drawScale; // px/metro NA PRANCHA final

  const contentW = Math.max(MIN_W, planDrawW + PAD * 2);
  const legendH =
    legend.length > 0 ? LEGEND_HEAD_H + legend.length * ROW_H + PAD : 0;
  const totalH = HEADER_H + PAD + planDrawH + PAD + legendH + FOOTER_H;

  const canvas = document.createElement("canvas");
  canvas.width = Math.round(contentW);
  canvas.height = Math.round(totalH);
  const ctx = canvas.getContext("2d");
  if (!ctx) return planDataUrl;

  // Fundo branco
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);

  // ---- Cabeçalho ----
  ctx.fillStyle = "#0f172a";
  ctx.fillRect(0, 0, canvas.width, HEADER_H);
  ctx.textBaseline = "middle";
  ctx.fillStyle = "#f8fafc";
  ctx.font = "bold 18px Arial, sans-serif";
  ctx.textAlign = "left";
  ctx.fillText(meta.title || "Croqui de planta", PAD, 26);

  const occ = meta.occurrence;
  const subParts = [
    "Planta esquemática (pericial)",
    occ?.numero_bo ? `BO ${occ.numero_bo}` : null,
    occ?.tipo_pericia || null,
    occ?.municipio || null,
  ].filter(Boolean);
  ctx.font = "12px Arial, sans-serif";
  ctx.fillStyle = "#cbd5e1";
  ctx.fillText(subParts.join("  ·  "), PAD, 50);

  ctx.textAlign = "right";
  ctx.fillStyle = "#cbd5e1";
  ctx.fillText(`Exportado em ${fmtDate(meta.timestamp)}`, canvas.width - PAD, 26);
  ctx.fillText("Escala gráfica abaixo · 1 m = 100 px (projeto)", canvas.width - PAD, 50);

  // ---- Planta ----
  const planX = (canvas.width - planDrawW) / 2;
  const planY = HEADER_H + PAD;
  ctx.drawImage(img, planX, planY, planDrawW, planDrawH);
  // moldura
  ctx.strokeStyle = "#cbd5e1";
  ctx.lineWidth = 1;
  ctx.strokeRect(planX + 0.5, planY + 0.5, planDrawW - 1, planDrawH - 1);

  // ---- Barra de escala (canto inferior esquerdo da planta) ----
  const niceM = niceScaleLength(platePxPerM);
  const barPx = niceM * platePxPerM;
  const barX = planX + 14;
  const barY = planY + planDrawH - 20;
  ctx.strokeStyle = "#0f172a";
  ctx.fillStyle = "#0f172a";
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.moveTo(barX, barY);
  ctx.lineTo(barX + barPx, barY);
  ctx.stroke();
  // ticks
  ctx.beginPath();
  ctx.moveTo(barX, barY - 5);
  ctx.lineTo(barX, barY + 5);
  ctx.moveTo(barX + barPx, barY - 5);
  ctx.lineTo(barX + barPx, barY + 5);
  ctx.stroke();
  ctx.font = "bold 12px Arial, sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "bottom";
  // fundo claro pro texto legível sobre a planta
  const barLabel = niceM >= 1 ? `${niceM} m` : `${niceM * 100} cm`;
  const tw = ctx.measureText(barLabel).width;
  ctx.fillStyle = "rgba(255,255,255,0.85)";
  ctx.fillRect(barX + barPx / 2 - tw / 2 - 3, barY - 22, tw + 6, 16);
  ctx.fillStyle = "#0f172a";
  ctx.fillText(barLabel, barX + barPx / 2, barY - 7);

  // ---- Rosa dos ventos (canto superior direito da planta) ----
  const compR = 26;
  const compX = planX + planDrawW - compR - 16;
  const compY = planY + compR + 16;
  ctx.save();
  ctx.translate(compX, compY);
  ctx.rotate((-(meta.compassDeg || 0) * Math.PI) / 180);
  // círculo de fundo
  ctx.beginPath();
  ctx.arc(0, 0, compR, 0, Math.PI * 2);
  ctx.fillStyle = "rgba(255,255,255,0.85)";
  ctx.fill();
  ctx.strokeStyle = "#0f172a";
  ctx.lineWidth = 1.5;
  ctx.stroke();
  // seta Norte
  ctx.beginPath();
  ctx.moveTo(0, -compR + 4);
  ctx.lineTo(6, 4);
  ctx.lineTo(0, 0);
  ctx.lineTo(-6, 4);
  ctx.closePath();
  ctx.fillStyle = "#b91c1c";
  ctx.fill();
  ctx.restore();
  // "N" sempre legível (não rotaciona o texto)
  ctx.fillStyle = "#0f172a";
  ctx.font = "bold 12px Arial, sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText("N", compX, compY - compR - 8);

  // ---- Legenda dos vestígios ----
  if (legend.length > 0) {
    let ly = HEADER_H + PAD + planDrawH + PAD;
    ctx.textAlign = "left";
    ctx.textBaseline = "middle";
    ctx.fillStyle = "#0f172a";
    ctx.font = "bold 14px Arial, sans-serif";
    ctx.fillText("Legenda — vestígios", PAD, ly + LEGEND_HEAD_H / 2);
    ly += LEGEND_HEAD_H;

    for (const row of legend) {
      const cy = ly + ROW_H / 2;
      // badge
      ctx.beginPath();
      ctx.arc(PAD + 11, cy, 11, 0, Math.PI * 2);
      ctx.fillStyle = row.color;
      ctx.fill();
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = "#ffffff";
      ctx.stroke();
      ctx.fillStyle = "#ffffff";
      ctx.font = "bold 11px Arial, sans-serif";
      ctx.textAlign = "center";
      ctx.fillText(row.label, PAD + 11, cy + 0.5);
      // texto
      ctx.textAlign = "left";
      ctx.fillStyle = "#0f172a";
      ctx.font = "13px Arial, sans-serif";
      const txt = row.descricao ? `${row.tipo} — ${row.descricao}` : row.tipo;
      ctx.fillText(txt, PAD + 30, cy);
      // separador
      ctx.strokeStyle = "#e2e8f0";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(PAD, ly + ROW_H - 0.5);
      ctx.lineTo(canvas.width - PAD, ly + ROW_H - 0.5);
      ctx.stroke();
      ly += ROW_H;
    }
  }

  // ---- Rodapé §13 ----
  const fy = canvas.height - FOOTER_H;
  ctx.fillStyle = "#1f2937";
  ctx.fillRect(0, fy, canvas.width, FOOTER_H);
  ctx.fillStyle = "#94a3b8";
  ctx.font = "10px Arial, sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(
    "SICRO Desktop — Croqui de planta · esquema técnico conforme levantamento do perito · documento sujeito a revisão.",
    canvas.width / 2,
    fy + FOOTER_H / 2,
  );

  return canvas.toDataURL("image/png");
}

/**
 * Abre uma view de impressão A4 (retrato/paisagem auto) com a prancha, e dispara
 * o diálogo de impressão — o perito escolhe "Salvar como PDF". Padrão já usado no
 * editor de imagem (print via iframe no WebView2).
 */
export function openPlantaPrintView(plateDataUrl: string, title: string): void {
  const landscape = true; // pranchas tendem a ser largas
  const html = `<!doctype html><html><head><meta charset="utf-8"><title>${escapeHtml(
    title,
  )}</title><style>
    @page { size: A4 ${landscape ? "landscape" : "portrait"}; margin: 10mm; }
    html,body { margin:0; padding:0; }
    .wrap { width:100%; text-align:center; }
    img { max-width:100%; max-height:190mm; height:auto; }
  </style></head><body><div class="wrap"><img src="${plateDataUrl}" /></div>
  <script>window.onload=function(){setTimeout(function(){window.print();},250);};</script>
  </body></html>`;

  const iframe = document.createElement("iframe");
  iframe.style.position = "fixed";
  iframe.style.right = "0";
  iframe.style.bottom = "0";
  iframe.style.width = "0";
  iframe.style.height = "0";
  iframe.style.border = "0";
  document.body.appendChild(iframe);
  const doc = iframe.contentWindow?.document;
  if (!doc) {
    iframe.remove();
    return;
  }
  doc.open();
  doc.write(html);
  doc.close();
  // remove o iframe depois (a impressão já terá sido disparada)
  window.setTimeout(() => iframe.remove(), 60000);
}

function escapeHtml(s: string): string {
  return s.replace(
    /[&<>"]/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] ?? c,
  );
}
