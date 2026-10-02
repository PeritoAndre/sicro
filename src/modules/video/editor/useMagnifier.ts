/**
 * Lupa do reprodutor: zoom e deslocamento só de tela (CSS transform) — o
 * arquivo, os quadros coletados e as medições não mudam.
 */
import { useCallback, useEffect, useRef, useState } from "react";

const MAGNIFIER_MAX = 8;

interface View {
  s: number;
  tx: number;
  ty: number;
}
const IDENTITY: View = { s: 1, tx: 0, ty: 0 };

export function useMagnifier(resetKey: unknown) {
  const [el, setEl] = useState<HTMLDivElement | null>(null);
  const [view, setView] = useState<View>(IDENTITY);
  const viewRef = useRef(view);
  viewRef.current = view;
  const panRef = useRef<{ x: number; y: number; tx: number; ty: number } | null>(null);
  const [panning, setPanning] = useState(false);

  // Limita o deslocamento à IMAGEM (não à caixa do <video>, que inclui as
  // faixas pretas do "contain"): maior que a área → não deixa sobrar borda;
  // menor → fica centralizada naquele eixo.
  const clamp = useCallback(
    (s: number, tx: number, ty: number): View => {
      const W = el?.clientWidth ?? 0;
      const H = el?.clientHeight ?? 0;
      const vid = el?.querySelector("video");
      const ar = vid && vid.videoWidth > 0 && vid.videoHeight > 0 ? vid.videoWidth / vid.videoHeight : W / (H || 1);
      // retângulo da imagem dentro da caixa, sem zoom
      const cw = W / H > ar ? H * ar : W;
      const ch = W / H > ar ? H : W / ar;
      const cx = (W - cw) / 2;
      const cy = (H - ch) / 2;
      const axis = (t: number, size: number, c: number, cs: number) =>
        s * cs >= size
          ? Math.min(-s * c, Math.max(size - s * (c + cs), t))
          : (size - s * cs) / 2 - s * c;
      return { s, tx: axis(tx, W, cx, cw), ty: axis(ty, H, cy, ch) };
    },
    [el],
  );

  /** Zoom por `factor` mantendo o ponto (cx, cy) — px dentro da área — parado. */
  const zoomAt = useCallback(
    (factor: number, cx?: number, cy?: number) => {
      if (!el) return;
      const v = viewRef.current;
      const s2 = Math.min(MAGNIFIER_MAX, Math.max(1, v.s * factor));
      if (s2 <= 1.001) {
        setView(IDENTITY);
        return;
      }
      const px = cx ?? el.clientWidth / 2;
      const py = cy ?? el.clientHeight / 2;
      const qx = (px - v.tx) / v.s;
      const qy = (py - v.ty) / v.s;
      setView(clamp(s2, px - s2 * qx, py - s2 * qy));
    },
    [el, clamp],
  );

  const reset = useCallback(() => setView(IDENTITY), []);

  // Outro vídeo → imagem inteira de novo.
  useEffect(() => {
    setView(IDENTITY);
  }, [resetKey]);

  // Roda do mouse: listener nativo não-passivo (o preventDefault precisa valer).
  useEffect(() => {
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      // Painéis sobre o vídeo (ajustes, diálogos) não disparam a lupa.
      if ((e.target as HTMLElement | null)?.closest?.("[data-no-magnify]")) return;
      e.preventDefault();
      const r = el.getBoundingClientRect();
      zoomAt(Math.exp(-e.deltaY * 0.0015), e.clientX - r.left, e.clientY - r.top);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [el, zoomAt]);

  const onPointerDown = (e: React.PointerEvent<HTMLElement>) => {
    if (e.button !== 0 || viewRef.current.s <= 1) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    panRef.current = { x: e.clientX, y: e.clientY, tx: viewRef.current.tx, ty: viewRef.current.ty };
    setPanning(true);
  };
  const onPointerMove = (e: React.PointerEvent<HTMLElement>) => {
    const p = panRef.current;
    if (!p) return;
    setView(clamp(viewRef.current.s, p.tx + e.clientX - p.x, p.ty + e.clientY - p.y));
  };
  const onPointerUp = () => {
    panRef.current = null;
    setPanning(false);
  };

  return {
    wrapRef: setEl,
    scale: view.s,
    transform:
      view.s > 1 ? `translate(${view.tx}px, ${view.ty}px) scale(${view.s})` : undefined,
    panning,
    zoomAt,
    reset,
    panHandlers: {
      onPointerDown,
      onPointerMove,
      onPointerUp,
      onPointerCancel: onPointerUp,
    },
  };
}
