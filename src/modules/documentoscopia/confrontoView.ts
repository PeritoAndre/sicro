/**
 * Matemática pura do visualizador de confronto (questionado × padrão).
 *
 * Cada lado é renderizado com `transform: translate(x,y) scale(s)` e
 * `transform-origin: 0 0` dentro de um painel de tamanho fixo (overflow
 * hidden). O pan é feito por `offset` (não por scroll nativo) — por isso
 * usamos `transform: scale` sem o problema de "comer o topo" que o scroll
 * nativo tinha (lá a solução foi `zoom:`; aqui não há scroll).
 *
 * Nada aqui toca DOM, React ou Tauri — funções puras, fáceis de testar.
 * §13: este módulo só posiciona/escala pixels. Não decide nada sobre o
 * documento.
 */

export interface Vec2 {
  x: number;
  y: number;
}

/** Estado de visualização de um lado: escala + deslocamento (px de tela). */
export interface ViewTransform {
  scale: number;
  x: number;
  y: number;
}

export interface Size {
  w: number;
  h: number;
}

export const ZOOM_MIN = 0.1;
/** Teto alto para "pixel peeping" forense (6400%). Acima de 1× a renderização
 * passa a ser sem interpolação (pixels nítidos) — ampliar não cria detalhe
 * além da resolução capturada (§13). */
export const ZOOM_MAX = 64;

/** Mantém a escala dentro dos limites razoáveis. */
export function clampZoom(scale: number): number {
  if (!Number.isFinite(scale)) return 1;
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, scale));
}

/**
 * Aplica um fator de zoom mantendo FIXO o ponto sob o cursor.
 *
 * Dado o ponto de imagem sob o cursor `p = (cursor - offset) / s`, após
 * mudar a escala para `s'` queremos `cursor = offset' + p·s'`, logo
 * `offset' = cursor - (cursor - offset)·(s'/s)`.
 */
export function zoomAt(
  view: ViewTransform,
  factor: number,
  cursor: Vec2,
): ViewTransform {
  const scale = clampZoom(view.scale * factor);
  const ratio = view.scale === 0 ? 1 : scale / view.scale;
  return {
    scale,
    x: cursor.x - (cursor.x - view.x) * ratio,
    y: cursor.y - (cursor.y - view.y) * ratio,
  };
}

/** Define uma escala EXATA mantendo fixo o ponto-âncora (px de tela). Útil
 * para "1:1" (pixel real) e presets de zoom. */
export function zoomTo(
  view: ViewTransform,
  targetScale: number,
  anchor: Vec2,
): ViewTransform {
  const factor = view.scale === 0 ? targetScale : targetScale / view.scale;
  return zoomAt(view, factor, anchor);
}

/** Move a visualização por um delta de tela (px). Escala inalterada. */
export function pan(view: ViewTransform, delta: Vec2): ViewTransform {
  return { scale: view.scale, x: view.x + delta.x, y: view.y + delta.y };
}

/**
 * Calcula a transformação que ENQUADRA a imagem no painel, centralizada,
 * com uma pequena margem (96%). Retorna escala+offset.
 */
export function fitTransform(
  image: Size,
  container: Size,
  margin = 0.96,
): ViewTransform {
  if (
    image.w <= 0 ||
    image.h <= 0 ||
    container.w <= 0 ||
    container.h <= 0
  ) {
    return { scale: 1, x: 0, y: 0 };
  }
  const scale = clampZoom(
    Math.min(container.w / image.w, container.h / image.h) * margin,
  );
  return {
    scale,
    x: (container.w - image.w * scale) / 2,
    y: (container.h - image.h * scale) / 2,
  };
}

/**
 * Converte um ponto de TELA (relativo ao painel) para coordenadas de IMAGEM
 * (px da imagem original), dada a transformação atual. Útil para marcadores.
 */
export function screenToImage(view: ViewTransform, screen: Vec2): Vec2 {
  const s = view.scale === 0 ? 1 : view.scale;
  return { x: (screen.x - view.x) / s, y: (screen.y - view.y) / s };
}

/** Inverso de `screenToImage`: ponto de imagem → tela. */
export function imageToScreen(view: ViewTransform, image: Vec2): Vec2 {
  return { x: image.x * view.scale + view.x, y: image.y * view.scale + view.y };
}

/** Fator de zoom por "tick" de scroll, na direção do deltaY. */
export function wheelFactor(deltaY: number, step = 0.0015): number {
  // Exponencial suave: cada pixel de scroll multiplica a escala.
  return Math.exp(-deltaY * step);
}

/**
 * Posição-âncora no painel TRAVADO correspondente ao cursor no painel ATIVO,
 * preservando a fração relativa do painel. Corrige o "zoom travado": o lado
 * travado deve ampliar no MESMO ponto relativo do cursor — não no centro —
 * para que os dois lados permaneçam alinhados (quando partem da mesma vista).
 * Se os painéis tiverem tamanhos diferentes, a fração relativa é preservada.
 */
export function correspondingAnchor(
  cursor: Vec2,
  active: Size,
  locked: Size,
): Vec2 {
  const fx = active.w > 0 ? cursor.x / active.w : 0;
  const fy = active.h > 0 ? cursor.y / active.h : 0;
  return { x: fx * locked.w, y: fy * locked.h };
}

/** Distância euclidiana entre dois pontos. */
export function distance(a: Vec2, b: Vec2): number {
  return Math.hypot(b.x - a.x, b.y - a.y);
}

/**
 * Ângulo no vértice `v` entre os raios para `a` e `b`, em graus [0, 180].
 * Invariante à escala (não precisa de calibração).
 */
export function angleAtDeg(a: Vec2, v: Vec2, b: Vec2): number {
  const v1x = a.x - v.x;
  const v1y = a.y - v.y;
  const v2x = b.x - v.x;
  const v2y = b.y - v.y;
  const m1 = Math.hypot(v1x, v1y);
  const m2 = Math.hypot(v2x, v2y);
  if (m1 === 0 || m2 === 0) return 0;
  const cos = (v1x * v2x + v1y * v2y) / (m1 * m2);
  return (Math.acos(Math.max(-1, Math.min(1, cos))) * 180) / Math.PI;
}

/** mm por pixel a partir de um comprimento conhecido (mm) medido em px. */
export function mmPerPixel(knownMm: number, pixelLength: number): number {
  if (pixelLength <= 0) return 0;
  return knownMm / pixelLength;
}
