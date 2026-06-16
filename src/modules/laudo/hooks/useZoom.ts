/**
 * useZoom — controle de zoom do editor de laudo.
 *
 * O zoom é aplicado via CSS `transform: scale(...)` sobre o `EditorPage`.
 * Não altera o conteúdo do documento — só a renderização visual.
 *
 * Valores aceitos: 0.5 (50%) até 5.0 (500%), em incrementos de 0.1.
 *
 * Funcionalidades:
 *   - `zoom`             — valor atual (1.0 = 100%).
 *   - `setZoom(n)`       — define valor (clamp em [0.5, 5.0]).
 *   - `zoomIn()` / `zoomOut()` — incrementa/decrementa em 0.1.
 *   - `reset()`          — volta para 1.0.
 *   - `fitWidth()`       — ajusta para preencher a largura disponível.
 *   - `fitPage()`        — ajusta para mostrar a página inteira.
 *
 * `fitWidth` / `fitPage` recebem o container width (px) e calculam o zoom
 * apropriado dado A4 widthCm × 37.795 px/cm (96 dpi).
 *
 * Persistência: por enquanto, apenas em memória. Caller pode salvar em
 * `localStorage` se quiser persistir entre sessões. F4+ vai considerar
 * salvar como preferência do laudo.
 */

import { useCallback, useState } from "react";

export const ZOOM_MIN = 0.5;
export const ZOOM_MAX = 5.0;
export const ZOOM_STEP = 0.1;
export const ZOOM_DEFAULT = 1.0;

/** px/cm a 96 DPI (1cm = 37.7952756 px). */
export const PX_PER_CM_96DPI = 37.7952756;

export interface UseZoomReturn {
  zoom: number;
  setZoom: (value: number) => void;
  zoomIn: () => void;
  zoomOut: () => void;
  reset: () => void;
  /** Ajusta zoom para que `pageWidthCm` caiba em `containerPx` (px). */
  fitWidth: (containerPx: number, pageWidthCm: number) => void;
  /**
   * Ajusta zoom para que a página inteira (largura E altura) caiba no
   * container — pega o menor dos dois ratios para garantir que nada
   * é cortado.
   */
  fitPage: (
    containerWidthPx: number,
    containerHeightPx: number,
    pageWidthCm: number,
    pageHeightCm: number,
  ) => void;
}

function clamp(v: number): number {
  if (v < ZOOM_MIN) return ZOOM_MIN;
  if (v > ZOOM_MAX) return ZOOM_MAX;
  // Quantiza para 1 casa decimal para evitar valores estranhos no slider.
  return Math.round(v * 100) / 100;
}

export function useZoom(initial: number = ZOOM_DEFAULT): UseZoomReturn {
  const [zoom, setZoomRaw] = useState(clamp(initial));

  const setZoom = useCallback((value: number) => {
    setZoomRaw(clamp(value));
  }, []);

  const zoomIn = useCallback(() => {
    setZoomRaw((z) => clamp(z + ZOOM_STEP));
  }, []);

  const zoomOut = useCallback(() => {
    setZoomRaw((z) => clamp(z - ZOOM_STEP));
  }, []);

  const reset = useCallback(() => {
    setZoomRaw(ZOOM_DEFAULT);
  }, []);

  const fitWidth = useCallback(
    (containerPx: number, pageWidthCm: number) => {
      if (containerPx <= 0 || pageWidthCm <= 0) return;
      const pageWidthPx = pageWidthCm * PX_PER_CM_96DPI;
      // Subtrai 40px de padding (margem do scroll container típica).
      const usablePx = Math.max(containerPx - 40, 100);
      setZoomRaw(clamp(usablePx / pageWidthPx));
    },
    [],
  );

  const fitPage = useCallback(
    (
      containerWidthPx: number,
      containerHeightPx: number,
      pageWidthCm: number,
      pageHeightCm: number,
    ) => {
      if (containerWidthPx <= 0 || containerHeightPx <= 0) return;
      if (pageWidthCm <= 0 || pageHeightCm <= 0) return;
      const pageWidthPx = pageWidthCm * PX_PER_CM_96DPI;
      const pageHeightPx = pageHeightCm * PX_PER_CM_96DPI;
      const usableW = Math.max(containerWidthPx - 40, 100);
      const usableH = Math.max(containerHeightPx - 40, 100);
      const ratio = Math.min(usableW / pageWidthPx, usableH / pageHeightPx);
      setZoomRaw(clamp(ratio));
    },
    [],
  );

  return { zoom, setZoom, zoomIn, zoomOut, reset, fitWidth, fitPage };
}
