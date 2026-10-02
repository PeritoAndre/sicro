/**
 * Assistente de foto de drone: correção radial de lente + crop antes de
 * virar fundo. O preview é um port JS do modelo do Rust em baixa resolução;
 * o derivado final (com sidecar JSON) é sempre gerado pelo Rust.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type { CropRectInput, DroneImportResult } from "@domain/croqui";
import styles from "./CroquiEditor.module.css";

interface DroneImportModalProps {
  workspacePath: string;
  croquiId?: string;
  occurrenceId?: string;
  onConfirm: (result: DroneImportResult) => void;
  onCancel: () => void;
}

interface LoadedImage {
  absolutePath: string;
  width: number;
  height: number;
  pixels: Uint8ClampedArray; // RGBA em resolução cheia
}

// Limita o loop JS por pixel do preview.
const PREVIEW_MAX = 600;

export function DroneImportModal({
  workspacePath,
  croquiId,
  occurrenceId,
  onConfirm,
  onCancel,
}: DroneImportModalProps) {
  const [intensity, setIntensity] = useState(0.5);
  const [loaded, setLoaded] = useState<LoadedImage | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Crop em pixels da resolução cheia (unidade que o Rust espera).
  const [crop, setCrop] = useState<CropRectInput | null>(null);

  const previewRef = useRef<HTMLCanvasElement | null>(null);
  // Preview corrigido em cache: mudar só o crop não recalcula.
  const correctedPreviewRef = useRef<{
    intensity: number;
    width: number;
    height: number;
    pixels: Uint8ClampedArray;
  } | null>(null);

  const handlePick = useCallback(async () => {
    setError(null);
    try {
      const picked = await openFileDialog({
        multiple: false,
        title: "Selecionar imagem de drone",
        filters: [
          {
            name: "Imagens",
            extensions: ["png", "jpg", "jpeg", "webp", "tif", "tiff"],
          },
        ],
      });
      if (typeof picked !== "string") return;
      // Asset protocol do Tauri: o WebView não carrega file:// direto.
      const url = convertFileSrc(picked);
      const img = new window.Image();
      img.crossOrigin = "anonymous";
      img.src = url;
      await new Promise<void>((resolve, reject) => {
        img.onload = () => resolve();
        img.onerror = () => reject(new Error("falha ao carregar imagem"));
      });
      const c = document.createElement("canvas");
      c.width = img.width;
      c.height = img.height;
      const ctx = c.getContext("2d");
      if (!ctx) throw new Error("canvas 2D context não disponível");
      ctx.drawImage(img, 0, 0);
      const data = ctx.getImageData(0, 0, c.width, c.height);
      setLoaded({
        absolutePath: picked,
        width: c.width,
        height: c.height,
        pixels: data.data,
      });
      // Crop inicial de 80 % centralizado: imagem inteira ficava invisível
      // contra a borda do canvas.
      const margin = 0.1;
      const cw = Math.round(c.width * (1 - 2 * margin));
      const ch = Math.round(c.height * (1 - 2 * margin));
      const cx = Math.round((c.width - cw) / 2);
      const cy = Math.round((c.height - ch) / 2);
      setCrop({ x: cx, y: cy, width: cw, height: ch });
      correctedPreviewRef.current = null;
    } catch (e) {
      setError(`Falha ao abrir imagem: ${(e as Error).message}`);
    }
  }, []);

  useEffect(() => {
    const canvas = previewRef.current;
    if (!canvas || !loaded) return;
    drawPreview(canvas, loaded, intensity, crop, correctedPreviewRef);
  }, [loaded, intensity, crop]);

  // Rust refaz o pipeline em resolução cheia.
  const handleConfirm = useCallback(async () => {
    if (!loaded || !crop) return;
    setBusy(true);
    setError(null);
    try {
      const result = await commands.importDroneImage(workspacePath, {
        source_absolute_path: loaded.absolutePath,
        intensity,
        crop,
        croqui_id: croquiId,
        occurrence_id: occurrenceId,
      });
      onConfirm(result);
    } catch (e) {
      setError(`Falha ao processar imagem: ${toSicroError(e).message}`);
    } finally {
      setBusy(false);
    }
  }, [loaded, crop, intensity, workspacePath, croquiId, occurrenceId, onConfirm]);

  const previewSize = useMemo(() => {
    if (!loaded) return { width: PREVIEW_MAX, height: PREVIEW_MAX * 0.66 };
    const scale = Math.min(
      PREVIEW_MAX / loaded.width,
      PREVIEW_MAX / loaded.height,
      1,
    );
    return {
      width: Math.round(loaded.width * scale),
      height: Math.round(loaded.height * scale),
    };
  }, [loaded]);

  return (
    <div
      className={styles.overlay}
      role="dialog"
      aria-modal="true"
      aria-labelledby="drone-modal-title"
      onClick={(e) => {
        if (e.target === e.currentTarget && !busy) onCancel();
      }}
    >
      <div className={styles.dialog} style={{ maxWidth: 760 }}>
        <header className={styles.dialogHeader}>
          <strong id="drone-modal-title">Importar imagem de drone</strong>
          <button
            type="button"
            onClick={onCancel}
            className={styles.dialogClose}
            disabled={busy}
          >
            Fechar
          </button>
        </header>

        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          {!loaded && (
            <p className={styles.dim}>
              Escolha uma imagem de drone para começar. A imagem original
              não será alterada; o SICRO gera um derivado corrigido e
              recortado dentro do workspace.
            </p>
          )}
          <div style={{ display: "flex", gap: 8 }}>
            <button
              type="button"
              className={styles.dialogClose}
              onClick={() => void handlePick()}
              disabled={busy}
            >
              {loaded ? "Trocar imagem…" : "Escolher imagem…"}
            </button>
            {loaded && (
              <button
                type="button"
                className={styles.dialogClose}
                onClick={() => {
                  if (!loaded) return;
                  const m = 0.1;
                  const cw = Math.round(loaded.width * (1 - 2 * m));
                  const ch = Math.round(loaded.height * (1 - 2 * m));
                  setCrop({
                    x: Math.round((loaded.width - cw) / 2),
                    y: Math.round((loaded.height - ch) / 2),
                    width: cw,
                    height: ch,
                  });
                }}
                disabled={busy}
                title="Restaura o retângulo de crop para 80% centralizado"
              >
                Resetar crop
              </button>
            )}
          </div>

          {loaded && (
            <>
              <div
                style={{
                  position: "relative",
                  width: previewSize.width,
                  height: previewSize.height,
                  border: "1px solid var(--sicro-border)",
                  background: "#0f172a",
                  margin: "0 auto",
                  flexShrink: 0,
                }}
              >
                <canvas
                  ref={previewRef}
                  width={previewSize.width}
                  height={previewSize.height}
                  style={{
                    width: previewSize.width,
                    height: previewSize.height,
                    display: "block",
                  }}
                />
                <CropOverlay
                  imageW={loaded.width}
                  imageH={loaded.height}
                  previewW={previewSize.width}
                  previewH={previewSize.height}
                  crop={crop}
                  onChange={(c) => setCrop(c)}
                />
              </div>

              <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                <span style={{ fontSize: 12 }}>
                  Correção de lente —{" "}
                  <strong>{Math.round(intensity * 100)}%</strong>{" "}
                  <span style={{ color: "var(--sicro-fg-dim)" }}>
                    (0% sem correção · 50% moderada · 100% forte)
                  </span>
                </span>
                <input
                  type="range"
                  min={0}
                  max={1}
                  step={0.05}
                  value={intensity}
                  onChange={(e) => setIntensity(Number(e.target.value))}
                  disabled={busy}
                />
              </label>

              <div style={{ fontSize: 11, color: "var(--sicro-fg-dim)" }}>
                Tamanho original: {loaded.width} × {loaded.height} px
                {crop && (
                  <>
                    {" · "}Crop:{" "}
                    {`${crop.width} × ${crop.height} px`} (a partir de{" "}
                    {`${crop.x}, ${crop.y}`})
                  </>
                )}
              </div>
            </>
          )}

          {error && (
            <p className={styles.danger} style={{ margin: 0 }}>
              {error}
            </p>
          )}

          <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
            <button
              type="button"
              className={styles.dialogClose}
              onClick={onCancel}
              disabled={busy}
            >
              Cancelar
            </button>
            <button
              type="button"
              className={styles.dialogClose}
              style={{
                color: "#5aa9e6",
                fontWeight: 600,
                opacity: loaded && crop && !busy ? 1 : 0.5,
              }}
              onClick={() => void handleConfirm()}
              disabled={!loaded || !crop || busy}
              title={
                loaded
                  ? "Aplica a correção e o crop em alta resolução no backend"
                  : "Escolha uma imagem primeiro"
              }
            >
              {busy ? "Processando…" : "Aplicar e usar como fundo"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ===========================================================================
// Preview (port JS da correção radial do Rust)

function drawPreview(
  canvas: HTMLCanvasElement,
  loaded: LoadedImage,
  intensity: number,
  crop: CropRectInput | null,
  cache: React.MutableRefObject<{
    intensity: number;
    width: number;
    height: number;
    pixels: Uint8ClampedArray;
  } | null>,
) {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const W = canvas.width;
  const H = canvas.height;
  ctx.fillStyle = "#0f172a";
  ctx.fillRect(0, 0, W, H);

  let cached = cache.current;
  if (
    !cached ||
    cached.width !== W ||
    cached.height !== H ||
    Math.abs(cached.intensity - intensity) > 1e-6
  ) {
    cached = computeCorrectedPreview(loaded, W, H, intensity);
    cache.current = cached;
  }

  // Copia para um ImageData novo: o lib DOM do TS exige ArrayBuffer não compartilhado.
  const imageData = ctx.createImageData(W, H);
  imageData.data.set(cached.pixels);
  ctx.putImageData(imageData, 0, 0);

  // Escurece fora do crop.
  if (crop) {
    const scaleX = W / loaded.width;
    const scaleY = H / loaded.height;
    const cx = crop.x * scaleX;
    const cy = crop.y * scaleY;
    const cw = crop.width * scaleX;
    const ch = crop.height * scaleY;
    ctx.fillStyle = "rgba(0, 0, 0, 0.55)";
    ctx.fillRect(0, 0, W, cy);
    ctx.fillRect(0, cy + ch, W, H - (cy + ch));
    ctx.fillRect(0, cy, cx, ch);
    ctx.fillRect(cx + cw, cy, W - (cx + cw), ch);
    ctx.strokeStyle = "#5aa9e6";
    ctx.lineWidth = 1.5;
    ctx.setLineDash([4, 4]);
    ctx.strokeRect(cx + 0.5, cy + 0.5, cw - 1, ch - 1);
    ctx.setLineDash([]);
  }
}

/**
 * Reamostra para o preview com a mesma correção do Rust
 * (`image_processing::lens_correction`): u_src = u · (1 + k1·r² + k2·r⁴),
 * normalizado pelo lado maior da imagem original.
 */
function computeCorrectedPreview(
  loaded: LoadedImage,
  previewW: number,
  previewH: number,
  intensity: number,
): {
  intensity: number;
  width: number;
  height: number;
  pixels: Uint8ClampedArray;
} {
  const t = Math.max(0, Math.min(1, intensity));
  const k1 = -0.3 * t;
  const k2 = 0.08 * t;
  const out = new Uint8ClampedArray(previewW * previewH * 4);

  // Cada pixel do preview é levado à resolução cheia antes de aplicar o
  // fator, para o raio nos cantos bater com o Rust.
  const halfW = loaded.width / 2;
  const halfH = loaded.height / 2;
  const norm = Math.max(halfW, halfH);

  for (let py = 0; py < previewH; py++) {
    const yd = (py / previewH) * loaded.height;
    const v = (yd - halfH) / norm;
    for (let px = 0; px < previewW; px++) {
      const xd = (px / previewW) * loaded.width;
      const u = (xd - halfW) / norm;
      const r2 = u * u + v * v;
      const r4 = r2 * r2;
      const f = 1 + k1 * r2 + k2 * r4;
      const xs = u * f * norm + halfW;
      const ys = v * f * norm + halfH;
      const [r, g, b, a] = sampleBilinear(
        loaded.pixels,
        loaded.width,
        loaded.height,
        xs,
        ys,
      );
      const oi = (py * previewW + px) * 4;
      out[oi] = r;
      out[oi + 1] = g;
      out[oi + 2] = b;
      out[oi + 3] = a;
    }
  }

  return { intensity: t, width: previewW, height: previewH, pixels: out };
}

function sampleBilinear(
  src: Uint8ClampedArray,
  w: number,
  h: number,
  x: number,
  y: number,
): [number, number, number, number] {
  if (x < 0 || y < 0 || x > w - 1 || y > h - 1) return [0, 0, 0, 0];
  const x0 = Math.floor(x);
  const y0 = Math.floor(y);
  const x1 = Math.min(x0 + 1, w - 1);
  const y1 = Math.min(y0 + 1, h - 1);
  const dx = x - x0;
  const dy = y - y0;
  const i00 = (y0 * w + x0) * 4;
  const i10 = (y0 * w + x1) * 4;
  const i01 = (y1 * w + x0) * 4;
  const i11 = (y1 * w + x1) * 4;
  const lerp = (a: number, b: number, t: number) => a * (1 - t) + b * t;
  const out: [number, number, number, number] = [0, 0, 0, 0];
  for (let c = 0; c < 4; c++) {
    const top = lerp(src[i00 + c] ?? 0, src[i10 + c] ?? 0, dx);
    const bot = lerp(src[i01 + c] ?? 0, src[i11 + c] ?? 0, dx);
    out[c] = Math.round(lerp(top, bot, dy));
  }
  return out;
}

// ===========================================================================
// Overlay de crop (converte entre pixels do preview e da resolução cheia)

type DragMode =
  | "move"
  | "draw"
  | "resize_nw"
  | "resize_ne"
  | "resize_sw"
  | "resize_se"
  | "resize_n"
  | "resize_s"
  | "resize_w"
  | "resize_e";

interface DragState {
  mode: DragMode;
  /** Em coordenadas da imagem. */
  anchor: { x: number; y: number };
  startCrop: CropRectInput;
  startMouseScreen: { x: number; y: number };
}

function CropOverlay({
  imageW,
  imageH,
  previewW,
  previewH,
  crop,
  onChange,
}: {
  imageW: number;
  imageH: number;
  previewW: number;
  previewH: number;
  crop: CropRectInput | null;
  onChange: (c: CropRectInput) => void;
}) {
  const dragRef = useRef<DragState | null>(null);
  const wrapperRef = useRef<HTMLDivElement | null>(null);
  // Refs para os listeners do document serem ligados uma vez só.
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const cropRef = useRef(crop);
  cropRef.current = crop;
  const sizesRef = useRef({ imageW, imageH, previewW, previewH });
  sizesRef.current = { imageW, imageH, previewW, previewH };

  useEffect(() => {
    const screenToImage = (clientX: number, clientY: number) => {
      const wrap = wrapperRef.current;
      if (!wrap) return null;
      const rect = wrap.getBoundingClientRect();
      const px = clientX - rect.left;
      const py = clientY - rect.top;
      const { imageW: iw, imageH: ih, previewW: pw, previewH: ph } =
        sizesRef.current;
      return {
        x: clamp((px / pw) * iw, 0, iw),
        y: clamp((py / ph) * ih, 0, ih),
      };
    };

    const onMove = (e: MouseEvent) => {
      const state = dragRef.current;
      if (!state) return;
      const pt = screenToImage(e.clientX, e.clientY);
      if (!pt) return;
      const { imageW: iw, imageH: ih } = sizesRef.current;
      const start = state.startCrop;
      const MIN = 16;

      switch (state.mode) {
        case "move": {
          const dx = pt.x - state.anchor.x;
          const dy = pt.y - state.anchor.y;
          onChangeRef.current({
            x: clamp(
              Math.round(start.x + dx),
              0,
              iw - start.width,
            ),
            y: clamp(
              Math.round(start.y + dy),
              0,
              ih - start.height,
            ),
            width: start.width,
            height: start.height,
          });
          return;
        }
        case "draw": {
          // Normaliza para width/height positivos mesmo arrastando para trás.
          const x1 = Math.min(state.anchor.x, pt.x);
          const y1 = Math.min(state.anchor.y, pt.y);
          const x2 = Math.max(state.anchor.x, pt.x);
          const y2 = Math.max(state.anchor.y, pt.y);
          onChangeRef.current({
            x: Math.round(x1),
            y: Math.round(y1),
            width: Math.max(MIN, Math.round(x2 - x1)),
            height: Math.max(MIN, Math.round(y2 - y1)),
          });
          return;
        }
        case "resize_se": {
          const w = clamp(Math.round(pt.x - start.x), MIN, iw - start.x);
          const h = clamp(Math.round(pt.y - start.y), MIN, ih - start.y);
          onChangeRef.current({ x: start.x, y: start.y, width: w, height: h });
          return;
        }
        case "resize_ne": {
          const w = clamp(Math.round(pt.x - start.x), MIN, iw - start.x);
          const maxYTop = start.y + start.height - MIN;
          const yNew = clamp(Math.round(pt.y), 0, maxYTop);
          const h = start.y + start.height - yNew;
          onChangeRef.current({ x: start.x, y: yNew, width: w, height: h });
          return;
        }
        case "resize_sw": {
          const maxXLeft = start.x + start.width - MIN;
          const xNew = clamp(Math.round(pt.x), 0, maxXLeft);
          const w = start.x + start.width - xNew;
          const h = clamp(Math.round(pt.y - start.y), MIN, ih - start.y);
          onChangeRef.current({ x: xNew, y: start.y, width: w, height: h });
          return;
        }
        case "resize_nw": {
          const maxXLeft = start.x + start.width - MIN;
          const maxYTop = start.y + start.height - MIN;
          const xNew = clamp(Math.round(pt.x), 0, maxXLeft);
          const yNew = clamp(Math.round(pt.y), 0, maxYTop);
          const w = start.x + start.width - xNew;
          const h = start.y + start.height - yNew;
          onChangeRef.current({ x: xNew, y: yNew, width: w, height: h });
          return;
        }
        case "resize_n": {
          const maxYTop = start.y + start.height - MIN;
          const yNew = clamp(Math.round(pt.y), 0, maxYTop);
          const h = start.y + start.height - yNew;
          onChangeRef.current({
            x: start.x,
            y: yNew,
            width: start.width,
            height: h,
          });
          return;
        }
        case "resize_s": {
          const h = clamp(Math.round(pt.y - start.y), MIN, ih - start.y);
          onChangeRef.current({
            x: start.x,
            y: start.y,
            width: start.width,
            height: h,
          });
          return;
        }
        case "resize_w": {
          const maxXLeft = start.x + start.width - MIN;
          const xNew = clamp(Math.round(pt.x), 0, maxXLeft);
          const w = start.x + start.width - xNew;
          onChangeRef.current({
            x: xNew,
            y: start.y,
            width: w,
            height: start.height,
          });
          return;
        }
        case "resize_e": {
          const w = clamp(Math.round(pt.x - start.x), MIN, iw - start.x);
          onChangeRef.current({
            x: start.x,
            y: start.y,
            width: w,
            height: start.height,
          });
          return;
        }
      }
    };
    const onUp = () => {
      dragRef.current = null;
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
    return () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
    };
  }, []);

  const startDrag = (mode: DragMode, e: React.MouseEvent) => {
    if (!cropRef.current) return;
    const wrap = wrapperRef.current;
    if (!wrap) return;
    const rect = wrap.getBoundingClientRect();
    const px = e.clientX - rect.left;
    const py = e.clientY - rect.top;
    const ix = (px / previewW) * imageW;
    const iy = (py / previewH) * imageH;
    dragRef.current = {
      mode,
      anchor: { x: ix, y: iy },
      startCrop: { ...cropRef.current },
      startMouseScreen: { x: e.clientX, y: e.clientY },
    };
    e.preventDefault();
    e.stopPropagation();
  };

  // Mousedown na área vazia desenha um retângulo novo.
  const startDrawNew = (e: React.MouseEvent) => {
    const wrap = wrapperRef.current;
    if (!wrap) return;
    const rect = wrap.getBoundingClientRect();
    const px = e.clientX - rect.left;
    const py = e.clientY - rect.top;
    const ix = (px / previewW) * imageW;
    const iy = (py / previewH) * imageH;
    dragRef.current = {
      mode: "draw",
      anchor: { x: ix, y: iy },
      startCrop: { x: Math.round(ix), y: Math.round(iy), width: 16, height: 16 },
      startMouseScreen: { x: e.clientX, y: e.clientY },
    };
    onChangeRef.current({
      x: Math.round(ix),
      y: Math.round(iy),
      width: 16,
      height: 16,
    });
    e.preventDefault();
  };

  // O wrapper cobre o preview inteiro mesmo sem crop, para poder desenhar um.
  const left = crop ? (crop.x / imageW) * previewW : 0;
  const top = crop ? (crop.y / imageH) * previewH : 0;
  const width = crop ? (crop.width / imageW) * previewW : 0;
  const height = crop ? (crop.height / imageH) * previewH : 0;

  const HANDLE = 10; // px, lado de cada alça
  const handleStyle: React.CSSProperties = {
    position: "absolute",
    width: HANDLE,
    height: HANDLE,
    background: "#5aa9e6",
    border: "1px solid #fff",
    borderRadius: 2,
    boxSizing: "border-box",
  };

  return (
    <div
      ref={wrapperRef}
      style={{
        position: "absolute",
        inset: 0,
        cursor: crop ? "crosshair" : "crosshair",
      }}
      onMouseDown={(e) => {
        if (e.target !== e.currentTarget) return; // alça/retângulo já tratou
        startDrawNew(e);
      }}
    >
      {crop && (
        <div
          style={{
            position: "absolute",
            left,
            top,
            width,
            height,
            cursor: "move",
            boxSizing: "border-box",
          }}
          onMouseDown={(e) => startDrag("move", e)}
        >
          {/* Cantos */}
          <div
            style={{
              ...handleStyle,
              left: -HANDLE / 2,
              top: -HANDLE / 2,
              cursor: "nwse-resize",
            }}
            onMouseDown={(e) => startDrag("resize_nw", e)}
          />
          <div
            style={{
              ...handleStyle,
              right: -HANDLE / 2,
              top: -HANDLE / 2,
              cursor: "nesw-resize",
            }}
            onMouseDown={(e) => startDrag("resize_ne", e)}
          />
          <div
            style={{
              ...handleStyle,
              left: -HANDLE / 2,
              bottom: -HANDLE / 2,
              cursor: "nesw-resize",
            }}
            onMouseDown={(e) => startDrag("resize_sw", e)}
          />
          <div
            style={{
              ...handleStyle,
              right: -HANDLE / 2,
              bottom: -HANDLE / 2,
              cursor: "nwse-resize",
            }}
            onMouseDown={(e) => startDrag("resize_se", e)}
          />
          {/* Meios das bordas */}
          <div
            style={{
              ...handleStyle,
              left: width / 2 - HANDLE / 2,
              top: -HANDLE / 2,
              cursor: "ns-resize",
            }}
            onMouseDown={(e) => startDrag("resize_n", e)}
          />
          <div
            style={{
              ...handleStyle,
              left: width / 2 - HANDLE / 2,
              bottom: -HANDLE / 2,
              cursor: "ns-resize",
            }}
            onMouseDown={(e) => startDrag("resize_s", e)}
          />
          <div
            style={{
              ...handleStyle,
              left: -HANDLE / 2,
              top: height / 2 - HANDLE / 2,
              cursor: "ew-resize",
            }}
            onMouseDown={(e) => startDrag("resize_w", e)}
          />
          <div
            style={{
              ...handleStyle,
              right: -HANDLE / 2,
              top: height / 2 - HANDLE / 2,
              cursor: "ew-resize",
            }}
            onMouseDown={(e) => startDrag("resize_e", e)}
          />
        </div>
      )}
    </div>
  );
}

function clamp(v: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, v));
}
