// @ts-nocheck -- ponte com o motor Pixi vendido (arcada). Camada PERICIAL do SICRO.
/**
 * Camada de marcadores de vestígio sobre a planta (Pixi). É SICRO (não faz parte
 * do serializer do arcada): os vestígios vivem em `SicroPlantaDoc.evidences` e
 * são desenhados aqui por cima do floorplan. Marcador = círculo colorido (cor do
 * tipo) + rótulo sequencial (A,B,C… ou 1,2,3…). §13: só rotula/organiza.
 */
import { Container, Graphics, Text, TextStyle } from "pixi.js";
import { main } from "./editor/EditorRoot";
import { METER } from "./editor/editor/constants";
import { evidenceMeta, evidenceLabelFor } from "../evidence";

let layer; // Container de vestígios (filho de main, acima do floorplan)
let trajLayer; // Container de trajetórias balísticas
let structureLayer; // Container de estruturas (muro/cerca/calçada)
let textLayer; // Container de rótulos de texto livre

function parseColor(hex) {
  if (typeof hex === "string" && hex[0] === "#") {
    const n = parseInt(hex.slice(1), 16);
    if (!Number.isNaN(n)) return n;
  }
  return 0x475569;
}

/** Garante o container da camada de vestígios adicionado ao viewport. */
export function ensureEvidenceLayer() {
  if (!main) return null;
  if (!layer || layer.destroyed) {
    layer = new Container();
    layer.zIndex = 9000;
  }
  if (layer.parent !== main) main.addChild(layer); // sempre por cima (último filho)
  return layer;
}

/** Converte coords de tela (clientX/Y) → coords do mundo Pixi (ou null). */
export function screenToWorldPoint(clientX, clientY) {
  if (!main) return null;
  const canvas = document.getElementById("planta-pixi-canvas");
  const r = canvas ? canvas.getBoundingClientRect() : { left: 0, top: 0 };
  const p = main.toWorld(clientX - r.left, clientY - r.top);
  return { x: p.x, y: p.y };
}

/** Converte coords do mundo Pixi → coords de tela (clientX/Y), inverso do acima. */
export function worldToScreenPoint(wx, wy) {
  if (!main) return null;
  const canvas = document.getElementById("planta-pixi-canvas");
  const r = canvas ? canvas.getBoundingClientRect() : { left: 0, top: 0 };
  const g = main.toGlobal({ x: wx, y: wy });
  return { x: g.x + r.left, y: g.y + r.top };
}

/** Limpa e redesenha todos os marcadores. O rótulo deriva da ORDEM da lista. */
export function renderEvidenceMarkers(markers, labelKind) {
  const lyr = ensureEvidenceLayer();
  if (!lyr) return;
  for (const c of lyr.removeChildren()) c.destroy({ children: true });
  if (!Array.isArray(markers)) return;

  const R = 22; // raio (world px); METER=100 → ~0,22 m
  markers.forEach((m, i) => {
    const meta = evidenceMeta(m.tipo);
    const color = parseColor(m.cor || meta.color);
    const label = evidenceLabelFor(i + 1, labelKind);

    const g = new Graphics();
    g.beginFill(color, 1);
    g.lineStyle(3, 0xffffff, 1); // anel branco interno
    g.drawCircle(0, 0, R);
    g.endFill();
    g.lineStyle(1.5, 0x111827, 0.9); // contorno escuro p/ contraste
    g.drawCircle(0, 0, R + 1.5);
    g.position.set(m.x, m.y);
    lyr.addChild(g);

    const t = new Text(
      label,
      new TextStyle({
        fontFamily: "Arial",
        fontSize: 26,
        fontWeight: "bold",
        fill: 0xffffff,
        align: "center",
      }),
    );
    t.anchor.set(0.5);
    t.position.set(m.x, m.y);
    lyr.addChild(t);
  });
}

/** Garante o container de trajetórias (acima dos vestígios). */
export function ensureTrajLayer() {
  if (!main) return null;
  if (!trajLayer || trajLayer.destroyed) {
    trajLayer = new Container();
    trajLayer.zIndex = 8900;
  }
  if (trajLayer.parent !== main) main.addChild(trajLayer);
  return trajLayer;
}

/** Desenha as trajetórias balísticas (origem → impacto), com seta + comprimento. */
export function renderTrajectories(list, color = 0xb91c1c) {
  const lyr = ensureTrajLayer();
  if (!lyr) return;
  for (const c of lyr.removeChildren()) c.destroy({ children: true });
  if (!Array.isArray(list)) return;

  list.forEach((t, i) => {
    const col = parseColor(t.cor) || color;
    const dx = t.x2 - t.x1;
    const dy = t.y2 - t.y1;
    const len = Math.sqrt(dx * dx + dy * dy);
    if (len < 1) return;
    const ang = Math.atan2(dy, dx);

    const g = new Graphics();
    // linha
    g.lineStyle(4, col, 1);
    g.moveTo(t.x1, t.y1);
    g.lineTo(t.x2, t.y2);
    // ponto de origem (pequeno círculo)
    g.lineStyle(0);
    g.beginFill(col, 1);
    g.drawCircle(t.x1, t.y1, 6);
    g.endFill();
    // seta no impacto
    const ah = 22; // tamanho da ponta (world px)
    const aw = 10;
    const bx = t.x2 - Math.cos(ang) * ah;
    const by = t.y2 - Math.sin(ang) * ah;
    const px = -Math.sin(ang);
    const py = Math.cos(ang);
    g.beginFill(col, 1);
    g.moveTo(t.x2, t.y2);
    g.lineTo(bx + px * aw, by + py * aw);
    g.lineTo(bx - px * aw, by - py * aw);
    g.closePath();
    g.endFill();
    lyr.addChild(g);

    // rótulo: "T1 · 3,40 m" no meio, com leve deslocamento perpendicular
    const meters = (len / METER).toFixed(2).replace(".", ",");
    const labelText = `${t.label || "T" + (i + 1)} · ${meters} m`;
    const mx = (t.x1 + t.x2) / 2 + px * 16;
    const my = (t.y1 + t.y2) / 2 + py * 16;
    const txt = new Text(
      labelText,
      new TextStyle({
        fontFamily: "Arial",
        fontSize: 18,
        fontWeight: "bold",
        fill: col,
        stroke: 0xffffff,
        strokeThickness: 4,
        align: "center",
      }),
    );
    txt.anchor.set(0.5);
    txt.position.set(mx, my);
    lyr.addChild(txt);
  });
}

// ---------------------------------------------------------------------------
// Estruturas lineares: muro / cerca de madeira / cerca de arame / calçada.

const STRUCT_DEFAULT_W = {
  muro: 16, // ~0,16 m
  cerca_madeira: 6,
  cerca_arame: 4,
  calcada: 120, // ~1,2 m
};

/** Garante o container de estruturas (abaixo dos vestígios/trajetórias). */
export function ensureStructureLayer() {
  if (!main) return null;
  if (!structureLayer || structureLayer.destroyed) {
    structureLayer = new Container();
    structureLayer.zIndex = 8500;
  }
  if (structureLayer.parent !== main) main.addChild(structureLayer);
  return structureLayer;
}

/** Desenha um polígono-faixa (banda) ao longo do segmento, com largura w. */
function drawBand(g, x1, y1, x2, y2, w, fill, lineColor, lineW) {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const len = Math.sqrt(dx * dx + dy * dy) || 1;
  const px = -dy / len;
  const py = dx / len;
  const h = w / 2;
  if (lineColor != null) g.lineStyle(lineW || 1.5, lineColor, 1);
  else g.lineStyle(0);
  g.beginFill(fill, 1);
  g.drawPolygon([
    x1 + px * h, y1 + py * h,
    x2 + px * h, y2 + py * h,
    x2 - px * h, y2 - py * h,
    x1 - px * h, y1 - py * h,
  ]);
  g.endFill();
}

/** Render das estruturas (muro/cerca/calçada). Em escala — comprimento real. */
export function renderStructures(list, selectedId: string | null = null) {
  const lyr = ensureStructureLayer();
  if (!lyr) return;
  for (const c of lyr.removeChildren()) c.destroy({ children: true });
  if (!Array.isArray(list)) return;

  list.forEach((s) => {
    const dx = s.x2 - s.x1;
    const dy = s.y2 - s.y1;
    const len = Math.sqrt(dx * dx + dy * dy);
    if (len < 1) return;
    const ux = dx / len;
    const uy = dy / len;
    const px = -uy;
    const py = ux;
    const w = s.espessura || STRUCT_DEFAULT_W[s.kind] || 8;

    // Halo de seleção (dourado) sob a estrutura, quando selecionada.
    if (s.id === selectedId) {
      const hl = new Graphics();
      hl.lineStyle(w + 14, 0xd7a84f, 0.55);
      hl.moveTo(s.x1, s.y1);
      hl.lineTo(s.x2, s.y2);
      lyr.addChild(hl);
    }

    const g = new Graphics();

    if (s.kind === "calcada") {
      drawBand(g, s.x1, s.y1, s.x2, s.y2, w, 0xe6e6e6, 0x9aa3ad, 1.2);
    } else if (s.kind === "muro") {
      // banda cinza (alvenaria) + contorno escuro
      drawBand(g, s.x1, s.y1, s.x2, s.y2, w, 0xbdbdbd, 0x3f3f3f, 1.5);
      // hachuras diagonais leves
      g.lineStyle(1, 0x6b6b6b, 0.6);
      const step = 14;
      for (let d = 0; d <= len; d += step) {
        const cx = s.x1 + ux * d;
        const cy = s.y1 + uy * d;
        g.moveTo(cx + px * (w / 2), cy + py * (w / 2));
        g.lineTo(cx - px * (w / 2) + ux * 6, cy - py * (w / 2) + uy * 6);
      }
    } else if (s.kind === "cerca_madeira") {
      g.lineStyle(3.5, 0x8a5a2b, 1);
      g.moveTo(s.x1, s.y1);
      g.lineTo(s.x2, s.y2);
      // moirões (postes) periódicos perpendiculares
      const postLen = 9;
      const step = 46;
      g.lineStyle(3, 0x6e4420, 1);
      for (let d = 0; d <= len; d += step) {
        const cx = s.x1 + ux * d;
        const cy = s.y1 + uy * d;
        g.moveTo(cx + px * postLen, cy + py * postLen);
        g.lineTo(cx - px * postLen, cy - py * postLen);
      }
    } else if (s.kind === "cerca_arame") {
      // linha tracejada
      g.lineStyle(2, 0x707070, 1);
      const dash = 16;
      const gap = 10;
      let d = 0;
      while (d < len) {
        const e = Math.min(d + dash, len);
        g.moveTo(s.x1 + ux * d, s.y1 + uy * d);
        g.lineTo(s.x1 + ux * e, s.y1 + uy * e);
        d = e + gap;
      }
    }

    lyr.addChild(g);

    // comprimento (m) — exceto calçada, que é contexto
    if (s.kind !== "calcada") {
      const meters = (len / METER).toFixed(2).replace(".", ",");
      const t = new Text(
        `${meters} m`,
        new TextStyle({
          fontFamily: "Arial",
          fontSize: 15,
          fill: 0x1f2937,
          stroke: 0xffffff,
          strokeThickness: 3,
        }),
      );
      t.anchor.set(0.5);
      t.position.set(
        (s.x1 + s.x2) / 2 + px * 16,
        (s.y1 + s.y2) / 2 + py * 16,
      );
      lyr.addChild(t);
    }
  });
}

// ---------------------------------------------------------------------------
// Texto livre (rótulos). Interativos: arrastar pra mover, duplo-clique pra editar.

/** Garante o container de textos (acima de tudo). */
export function ensureTextLayer() {
  if (!main) return null;
  if (!textLayer || textLayer.destroyed) {
    textLayer = new Container();
    textLayer.zIndex = 9200;
  }
  if (textLayer.parent !== main) main.addChild(textLayer);
  return textLayer;
}

/**
 * Render dos rótulos de texto. `handlers.onMove(id,x,y)` e `handlers.onEdit(id)`
 * permitem mover (arraste) e editar (duplo-clique) sem sair do canvas Pixi.
 */
export function renderTexts(list, handlers = {}) {
  const lyr = ensureTextLayer();
  if (!lyr) return;
  for (const c of lyr.removeChildren()) c.destroy({ children: true });
  if (!Array.isArray(list)) return;

  list.forEach((tx) => {
    if (!tx.text) return;
    const col = parseColor(tx.cor) || 0x111827;
    const t = new Text(
      tx.text,
      new TextStyle({
        fontFamily: "Arial",
        fontSize: tx.size || 28,
        fill: col,
        stroke: 0xffffff,
        strokeThickness: 4,
        align: "left",
        wordWrap: false,
      }),
    );
    t.anchor.set(0.5);
    t.position.set(tx.x, tx.y);

    // interatividade (Pixi 6): arrastar move; duplo-toque edita.
    t.interactive = true;
    t.cursor = "move";
    let dragging = false;
    let data = null;
    let lastTap = 0;
    t.on("pointerdown", (e) => {
      try {
        e.stopPropagation();
      } catch {
        /* noop */
      }
      const now =
        typeof performance !== "undefined" ? performance.now() : 0;
      if (now - lastTap < 320) {
        lastTap = 0;
        handlers.onEdit && handlers.onEdit(tx.id);
        return;
      }
      lastTap = now;
      dragging = true;
      data = e.data;
    });
    const end = () => {
      if (dragging) {
        dragging = false;
        handlers.onMove && handlers.onMove(tx.id, t.x, t.y);
      }
      data = null;
    };
    t.on("pointerup", end);
    t.on("pointerupoutside", end);
    t.on("pointermove", () => {
      if (dragging && data) {
        const p = data.getLocalPosition(t.parent);
        t.position.set(p.x, p.y);
      }
    });

    lyr.addChild(t);
  });
}

/** Bounds (mundo) dos marcadores + trajetórias, ou null — pra enquadrar o export. */
export function getEvidenceLayerBoundsWorld() {
  const boxes = [];
  for (const lyr of [layer, trajLayer, structureLayer, textLayer]) {
    if (lyr && !lyr.destroyed && lyr.children.length > 0) {
      try {
        const b = lyr.getLocalBounds();
        if (b && b.width > 0 && b.height > 0) boxes.push(b);
      } catch {
        /* noop */
      }
    }
  }
  if (boxes.length === 0) return null;
  const minX = Math.min(...boxes.map((b) => b.x));
  const minY = Math.min(...boxes.map((b) => b.y));
  const maxX = Math.max(...boxes.map((b) => b.x + b.width));
  const maxY = Math.max(...boxes.map((b) => b.y + b.height));
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}

/** Destrói a camada (chamado no dispose do motor). */
export function disposeEvidenceLayer() {
  try {
    if (layer && !layer.destroyed) layer.destroy({ children: true });
  } catch {
    /* noop */
  }
  try {
    if (trajLayer && !trajLayer.destroyed) trajLayer.destroy({ children: true });
  } catch {
    /* noop */
  }
  try {
    if (structureLayer && !structureLayer.destroyed)
      structureLayer.destroy({ children: true });
  } catch {
    /* noop */
  }
  try {
    if (textLayer && !textLayer.destroyed) textLayer.destroy({ children: true });
  } catch {
    /* noop */
  }
  layer = undefined;
  trajLayer = undefined;
  structureLayer = undefined;
  textLayer = undefined;
}
