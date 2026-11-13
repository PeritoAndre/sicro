/** Player do item com o desenho das regiões no instante atual e a camada de marcação. */

import { useEffect, useRef, useState, type MutableRefObject } from "react";
import type { MontagemTrecho, Regiao } from "@domain/montagem";
import { posicaoEm } from "./montagemModel";
import type { AlvoMarcacao, Rascunho } from "./marcacao";
import styles from "./MontagemView.module.css";

export interface PreviaItem {
  src: string;
  /** Início no vídeo de origem (s). */
  t: number;
  fim: number | null;
  vel: number;
  key: string;
  itemId: string;
}

const pct = (v: number) => `${(v * 100).toFixed(3)}%`;
const clamp = (v: number, a: number, b: number) => Math.max(a, Math.min(b, v));
/** Retângulo da imagem dentro do <video> (object-fit: contain), em px relativos ao elemento. */
function caixaVideo(v: HTMLVideoElement) {
  const { clientWidth: cw, clientHeight: ch, videoWidth: vw, videoHeight: vh } = v;
  if (!vw || !vh || !cw || !ch) return { l: 0, t: 0, w: cw, h: ch };
  const k = Math.min(cw / vw, ch / vh);
  const w = vw * k;
  const h = vh * k;
  return { l: (cw - w) / 2, t: (ch - h) / 2, w, h };
}

const r4 = (v: number) => Math.round(v * 10000) / 10000;

function Caixa({ x, y, w, h, cor, tracejado, preenche }: { x: number; y: number; w: number; h: number; cor: string; tracejado?: boolean; preenche?: boolean }) {
  return (
    <div
      className={styles.caixa}
      style={{
        left: pct(x),
        top: pct(y),
        width: pct(w),
        height: pct(h),
        borderColor: cor,
        borderStyle: tracejado ? "dashed" : "solid",
        background: preenche ? "rgba(255,255,255,0.18)" : "transparent",
      }}
    />
  );
}

function Ponto({ x, y, cor, forma }: { x: number; y: number; cor: string; forma: "circulo" | "seta" }) {
  if (forma === "circulo") {
    return <div className={styles.pontoCirculo} style={{ left: pct(x), top: pct(y), borderColor: cor }} />;
  }
  return (
    <svg className={styles.pontoSeta} style={{ left: pct(x), top: pct(y) }} viewBox="0 0 20 20" aria-hidden>
      <path d="M1 19 L18 2 M1 19 L1 9 M1 19 L11 19" stroke={cor} strokeWidth="2.4" fill="none" strokeLinecap="round" />
    </svg>
  );
}

export function PalcoPrevia({
  previa,
  trecho,
  tRel,
  onTempo,
  videoRef,
  alvo,
  rascunho,
  onRascunho,
  aspectoSaida,
}: {
  previa: PreviaItem;
  /** Trecho do item (para desenhar as regiões); null em quadro congelado. */
  trecho: MontagemTrecho | null;
  tRel: number;
  onTempo: (t: number) => void;
  videoRef: MutableRefObject<HTMLVideoElement | null>;
  alvo: AlvoMarcacao | null;
  rascunho: Rascunho | null;
  onRascunho: (r: Rascunho | null) => void;
  aspectoSaida: number;
}) {
  const [cx, setCx] = useState({ l: 0, t: 0, w: 0, h: 0 });
  // Instante do player em quadros de tela (o timeupdate do <video> é lento demais para seguir o ponto).
  useEffect(() => {
    let raf = 0;
    let ult = -1;
    const laco = () => {
      const v = videoRef.current;
      if (v && Math.abs(v.currentTime - ult) > 0.01) {
        ult = v.currentTime;
        onTempo(v.currentTime);
      }
      if (v) {
        const c = caixaVideo(v);
        setCx((o) => (Math.abs(o.l - c.l) + Math.abs(o.t - c.t) + Math.abs(o.w - c.w) + Math.abs(o.h - c.h) < 0.5 ? o : c));
      }
      raf = requestAnimationFrame(laco);
    };
    raf = requestAnimationFrame(laco);
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [previa.key]);

  const arrasto = useRef<{ x: number; y: number } | null>(null);
  const norm = (e: React.PointerEvent) => {
    const v = videoRef.current as HTMLVideoElement;
    const r = v.getBoundingClientRect();
    const c = caixaVideo(v);
    return {
      x: clamp((e.clientX - r.left - c.l) / (c.w || 1), 0, 1),
      y: clamp((e.clientY - r.top - c.t) / (c.h || 1), 0, 1),
    };
  };
  const calcula = (p0: { x: number; y: number }, p1: { x: number; y: number }): Rascunho => {
    if (alvo?.tipo === "destaque" && alvo.ponto) return { x: r4(p1.x), y: r4(p1.y) };
    if (alvo?.tipo === "zoom") {
      const v = videoRef.current;
      const k = v && v.videoHeight ? v.videoWidth / v.videoHeight / aspectoSaida : 1;
      const w = Math.min(Math.abs(p1.x - p0.x), 1, 1 / k);
      const h = w * k;
      return {
        x: r4(clamp(p1.x >= p0.x ? p0.x : p0.x - w, 0, 1 - w)),
        y: r4(clamp(p1.y >= p0.y ? p0.y : p0.y - h, 0, 1 - h)),
        w: r4(w),
        h: r4(h),
      };
    }
    return {
      x: r4(Math.min(p0.x, p1.x)),
      y: r4(Math.min(p0.y, p1.y)),
      w: r4(Math.abs(p1.x - p0.x)),
      h: r4(Math.abs(p1.y - p0.y)),
    };
  };

  const regiaoZoom = (): Regiao | null => {
    if (!trecho?.zoom) return null;
    const { inicio, fim } = trecho.zoom;
    const dur = Math.max(0.04, trecho.fim_s - trecho.inicio_s);
    const f = clamp(tRel / dur, 0, 1);
    const l = (a: number, b: number) => a + (b - a) * f;
    return { x: l(inicio.x, fim.x), y: l(inicio.y, fim.y), w: l(inicio.w, fim.w), h: l(inicio.h, fim.h) };
  };
  const zoomAtual = regiaoZoom();

  return (
    <div className={styles.palco}>
      <video
        key={previa.key}
        ref={videoRef}
        src={previa.src}
        controls
        autoPlay
        onLoadedMetadata={(e) => {
          e.currentTarget.currentTime = previa.t;
          e.currentTarget.playbackRate = previa.vel;
        }}
        onTimeUpdate={(e) => {
          if (previa.fim != null && e.currentTarget.currentTime >= previa.fim) e.currentTarget.pause();
        }}
      />
      <div className={styles.desenho} style={{ left: cx.l, top: cx.t, width: cx.w, height: cx.h }}>
        {zoomAtual && <Caixa {...zoomAtual} cor="#ffd84a" tracejado />}
        {trecho?.desfoques?.map((d) => {
          const p = posicaoEm(d.quadros, tRel);
          return p && p.w != null && p.h != null ? <Caixa key={d.id} x={p.x} y={p.y} w={p.w} h={p.h} cor="#8ecbff" preenche /> : null;
        })}
        {trecho?.destaques?.map((d) => {
          const p = posicaoEm(d.quadros, tRel);
          if (!p) return null;
          if (d.forma === "retangulo") {
            return p.w != null && p.h != null ? <Caixa key={d.id} x={p.x} y={p.y} w={p.w} h={p.h} cor={d.cor} /> : null;
          }
          return <Ponto key={d.id} x={p.x} y={p.y} cor={d.cor} forma={d.forma} />;
        })}
        {rascunho &&
          (rascunho.w != null && rascunho.h != null ? (
            <Caixa x={rascunho.x} y={rascunho.y} w={rascunho.w} h={rascunho.h} cor="#ffffff" tracejado />
          ) : (
            <Ponto x={rascunho.x} y={rascunho.y} cor="#ffffff" forma="circulo" />
          ))}
      </div>
      {alvo && (
        <div
          className={styles.captura}
          style={{ left: cx.l, top: cx.t, width: cx.w, height: Math.max(0, cx.h - 44) }}
          onPointerDown={(e) => {
            e.currentTarget.setPointerCapture(e.pointerId);
            const p = norm(e);
            arrasto.current = p;
            onRascunho(calcula(p, p));
          }}
          onPointerMove={(e) => {
            if (arrasto.current) onRascunho(calcula(arrasto.current, norm(e)));
          }}
          onPointerUp={() => (arrasto.current = null)}
          onPointerCancel={() => (arrasto.current = null)}
        />
      )}
    </div>
  );
}
