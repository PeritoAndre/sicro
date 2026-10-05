/**
 * PNG técnico da planta: cabeçalho (sem título do croqui), desenho da folha, coluna de legenda
 * (vestígios com medidas, pessoas, trajetórias, cômodos e áreas, escala gráfica) e rodapé.
 */

export interface PlantaStampMeta {
  occurrence: { numero_bo?: string | null; tipo_pericia?: string | null; municipio?: string | null } | null;
  sheetLabel: string;
  timestamp: Date;
  /** px por metro no desenho exportado. */
  pxPerM: number;
}

export interface PlantaLegend {
  evidences: { label: string; title: string; sub: string }[];
  people: { title: string; sub: string; caido: boolean }[];
  trajs: { title: string; sub: string; color: string }[];
  rooms: { name: string; area: number }[];
  usable: number;
  built: number;
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const im = new Image();
    im.onload = () => resolve(im);
    im.onerror = () => reject(new Error("não foi possível montar o PNG técnico"));
    im.src = src;
  });
}

const fmt = (v: number, d = 2) => v.toFixed(d).replace(".", ",");
const pad2 = (n: number) => String(n).padStart(2, "0");
const fmtDate = (d: Date) => `${pad2(d.getDate())}/${pad2(d.getMonth() + 1)}/${d.getFullYear()} ${pad2(d.getHours())}:${pad2(d.getMinutes())}`;

function wrap(ctx: CanvasRenderingContext2D, text: string, maxW: number): string[] {
  const words = text.split(/\s+/).filter(Boolean);
  const lines: string[] = [];
  let cur = "";
  for (const w of words) {
    const t = cur ? `${cur} ${w}` : w;
    if (ctx.measureText(t).width > maxW && cur) {
      lines.push(cur);
      cur = w;
    } else cur = t;
  }
  if (cur) lines.push(cur);
  return lines;
}

export async function stampPlanta(drawingPng: string, meta: PlantaStampMeta, legend: PlantaLegend): Promise<string> {
  const img = await loadImage(drawingPng);
  const Wd = img.width;
  const Hd = img.height;
  const k = Math.max(1, Wd / 1400);
  const headerH = Math.round(64 * k);
  const footerH = Math.round(28 * k);
  const pad = Math.round(20 * k);
  const colW = Math.round(430 * k);
  const font = (px: number, weight = 400, family = "'Source Sans 3', 'Segoe UI', sans-serif") => `${weight} ${Math.round(px * k)}px ${family}`;
  const mono = "'JetBrains Mono', monospace";
  const display = "'Barlow Semi Condensed', 'Arial Narrow', sans-serif";

  // Mede a coluna antes de desenhar, para saber a altura final.
  const meas = document.createElement("canvas").getContext("2d")!;
  const textW = colW - pad * 2 - Math.round(32 * k);
  const block = (title: string, sub: string) => {
    meas.font = font(13, 600);
    const t = wrap(meas, title, textW);
    meas.font = font(11.5);
    const s = sub ? wrap(meas, sub, textW) : [];
    return { t, s, h: t.length * 17 * k + s.length * 15 * k + 12 * k };
  };
  const evid = legend.evidences.map((e) => ({ ...e, b: block(`${e.label} · ${e.title}`, e.sub) }));
  const ppl = legend.people.map((p) => ({ ...p, b: block(p.title, p.sub) }));
  const trj = legend.trajs.map((t) => ({ ...t, b: block(t.title, t.sub) }));
  const hasForensic = evid.length + ppl.length + trj.length > 0;
  let colH = pad;
  if (hasForensic) colH += 26 * k + [...evid, ...ppl, ...trj].reduce((a, x) => a + x.b.h, 0) + 14 * k;
  if (legend.rooms.length) colH += 26 * k + legend.rooms.length * 19 * k + 34 * k;
  colH += 90 * k;

  const W = Wd + colW;
  const H = headerH + Math.max(Hd, colH) + footerH;
  const canvas = document.createElement("canvas");
  canvas.width = W;
  canvas.height = H;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("canvas 2d indisponível");
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, W, H);

  // Cabeçalho
  ctx.fillStyle = "#0f172a";
  ctx.fillRect(0, 0, W, headerH);
  ctx.textBaseline = "middle";
  const occ = meta.occurrence;
  const sub = [occ?.numero_bo ? `BO ${occ.numero_bo}` : "", occ?.tipo_pericia ?? "", occ?.municipio ?? ""].filter(Boolean).join(" · ");
  ctx.fillStyle = "#f8fafc";
  ctx.font = font(19, 700, display);
  ctx.fillText("PLANTA BAIXA · CROQUI PERICIAL", pad, sub ? headerH / 2 - 9 * k : headerH / 2);
  if (sub) {
    ctx.font = font(12);
    ctx.fillStyle = "#cbd5e1";
    ctx.fillText(sub, pad, headerH / 2 + 12 * k);
  }
  ctx.textAlign = "right";
  ctx.font = font(12);
  ctx.fillStyle = "#cbd5e1";
  ctx.fillText(meta.sheetLabel, W - pad, headerH / 2 - 9 * k);
  ctx.fillText(`Exportado em ${fmtDate(meta.timestamp)}`, W - pad, headerH / 2 + 12 * k);
  ctx.textAlign = "left";

  // Desenho
  ctx.drawImage(img, 0, headerH);
  ctx.strokeStyle = "#d1d5db";
  ctx.lineWidth = Math.max(1, k);
  ctx.beginPath();
  ctx.moveTo(Wd + 0.5, headerH);
  ctx.lineTo(Wd + 0.5, H - footerH);
  ctx.stroke();

  // Coluna da legenda
  const x0 = Wd + pad;
  let y = headerH + pad;
  const title = (t: string) => {
    ctx.fillStyle = "#111111";
    ctx.font = font(13, 700, display);
    ctx.textBaseline = "alphabetic";
    ctx.fillText(t, x0, y + 13 * k);
    y += 26 * k;
  };
  const lines = (b: { t: string[]; s: string[] }, tx: number) => {
    ctx.textBaseline = "alphabetic";
    ctx.fillStyle = "#111111";
    ctx.font = font(13, 600);
    for (const l of b.t) {
      ctx.fillText(l, tx, y + 13 * k);
      y += 17 * k;
    }
    ctx.fillStyle = "#4b5563";
    ctx.font = font(11.5);
    for (const l of b.s) {
      ctx.fillText(l, tx, y + 11 * k);
      y += 15 * k;
    }
    y += 12 * k;
  };
  const tx = x0 + Math.round(32 * k);
  if (hasForensic) {
    title("LEGENDA");
    for (const e of evid) {
      ctx.beginPath();
      ctx.arc(x0 + 11 * k, y + 8 * k, 10 * k, 0, Math.PI * 2);
      ctx.fillStyle = "#facc15";
      ctx.fill();
      ctx.lineWidth = 1.3 * k;
      ctx.strokeStyle = "#111111";
      ctx.stroke();
      ctx.fillStyle = "#111111";
      ctx.font = font(e.label.length > 1 ? 9.5 : 11.5, 700);
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(e.label, x0 + 11 * k, y + 8.5 * k);
      ctx.textAlign = "left";
      lines(e.b, tx);
    }
    for (const p of ppl) {
      ctx.fillStyle = "#fde8e8";
      ctx.strokeStyle = "#7f1d1d";
      ctx.lineWidth = 1.2 * k;
      ctx.beginPath();
      if (p.caido) ctx.roundRect(x0 + 1 * k, y + 3 * k, 22 * k, 11 * k, 5 * k);
      else ctx.arc(x0 + 11 * k, y + 8 * k, 7 * k, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
      lines(p.b, tx);
    }
    for (const t of trj) {
      ctx.strokeStyle = t.color;
      ctx.lineWidth = 1.8 * k;
      ctx.setLineDash([6 * k, 4 * k]);
      ctx.beginPath();
      ctx.moveTo(x0, y + 8 * k);
      ctx.lineTo(x0 + 20 * k, y + 8 * k);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.fillStyle = t.color;
      ctx.beginPath();
      ctx.moveTo(x0 + 24 * k, y + 8 * k);
      ctx.lineTo(x0 + 17 * k, y + 4 * k);
      ctx.lineTo(x0 + 17 * k, y + 12 * k);
      ctx.fill();
      lines(t.b, tx);
    }
    y += 2 * k;
  }
  if (legend.rooms.length) {
    title("CÔMODOS");
    const right = Wd + colW - pad;
    ctx.textBaseline = "alphabetic";
    for (const r of legend.rooms) {
      ctx.fillStyle = "#111111";
      ctx.font = font(12.5);
      ctx.fillText(r.name, x0, y + 12 * k);
      ctx.font = font(12, 400, mono);
      ctx.textAlign = "right";
      ctx.fillText(`${fmt(r.area, 1)} m²`, right, y + 12 * k);
      ctx.textAlign = "left";
      y += 19 * k;
    }
    ctx.strokeStyle = "#9ca3af";
    ctx.lineWidth = Math.max(1, 0.8 * k);
    ctx.beginPath();
    ctx.moveTo(x0, y + 2 * k);
    ctx.lineTo(right, y + 2 * k);
    ctx.stroke();
    y += 6 * k;
    ctx.font = font(12.5, 700);
    ctx.fillText("Área útil · construída", x0, y + 14 * k);
    ctx.font = font(12, 700, mono);
    ctx.textAlign = "right";
    ctx.fillText(`${fmt(legend.usable, 1)} · ${fmt(legend.built, 1)} m²`, right, y + 14 * k);
    ctx.textAlign = "left";
    y += 28 * k;
  }
  // Escala gráfica: o maior passo que cabe na coluna.
  const steps = [1, 2, 5, 10, 20, 50];
  const avail = colW - pad * 2 - 30 * k;
  const unit = [...steps].reverse().find((s) => s * 3 * meta.pxPerM <= avail) ?? 1;
  const seg = unit * meta.pxPerM;
  const by = Math.max(y + 16 * k, headerH + Math.max(Hd, colH) - 64 * k);
  ctx.textBaseline = "alphabetic";
  for (let i = 0; i < 3; i++) {
    ctx.fillStyle = i % 2 === 0 ? "#111111" : "#ffffff";
    ctx.fillRect(x0 + i * seg, by, seg, 7 * k);
    ctx.strokeStyle = "#111111";
    ctx.lineWidth = Math.max(1, 0.8 * k);
    ctx.strokeRect(x0 + i * seg, by, seg, 7 * k);
  }
  ctx.fillStyle = "#111111";
  ctx.font = font(11, 400, mono);
  for (let i = 0; i <= 3; i++) {
    ctx.textAlign = i === 0 ? "left" : "center";
    ctx.fillText(i === 3 ? `${i * unit} m` : String(i * unit), x0 + i * seg, by + 22 * k);
  }
  ctx.textAlign = "left";
  ctx.fillStyle = "#4b5563";
  ctx.font = font(11);
  ctx.fillText("Medidas internas, em metros.", x0, by + 42 * k);

  // Rodapé
  ctx.fillStyle = "#0f172a";
  ctx.fillRect(0, H - footerH, W, footerH);
  ctx.fillStyle = "#cbd5e1";
  ctx.font = font(11);
  ctx.textBaseline = "middle";
  ctx.fillText("SICRO · croqui pericial · documento técnico, sujeito a revisão pelo perito", pad, H - footerH / 2);
  return canvas.toDataURL("image/png");
}
