/** Duas imagens lado a lado, com zoom (roda) e arraste em conjunto, para conferir original × realce. */
import { useRef, useState } from "react";
import type { Comparacao } from "./realceStore";
import styles from "./Realce.module.css";

export function CompareView({ c }: { c: Comparacao }) {
  const [v, setV] = useState({ z: 1, x: 0, y: 0 });
  const arraste = useRef<{ x: number; y: number; vx: number; vy: number } | null>(null);

  const roda = (e: React.WheelEvent<HTMLDivElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const px = e.clientX - r.left - r.width / 2;
    const py = e.clientY - r.top - r.height / 2;
    const f = e.deltaY < 0 ? 1.2 : 1 / 1.2;
    setV((o) => {
      const z = Math.min(32, Math.max(1, o.z * f));
      const k = z / o.z;
      return z === 1 ? { z, x: 0, y: 0 } : { z, x: px - (px - o.x) * k, y: py - (py - o.y) * k };
    });
  };
  const estilo = { transform: `translate(${v.x}px, ${v.y}px) scale(${v.z})` };
  const painel = (lado: Comparacao["esquerda"]) => (
    <figure className={styles.painel}>
      <div
        className={styles.painelImg}
        onWheel={roda}
        onPointerDown={(e) => {
          if (v.z === 1) return;
          e.currentTarget.setPointerCapture(e.pointerId);
          arraste.current = { x: e.clientX, y: e.clientY, vx: v.x, vy: v.y };
        }}
        onPointerMove={(e) => {
          const a = arraste.current;
          if (a) setV((o) => ({ ...o, x: a.vx + e.clientX - a.x, y: a.vy + e.clientY - a.y }));
        }}
        onPointerUp={() => (arraste.current = null)}
        onDoubleClick={() => setV({ z: 1, x: 0, y: 0 })}
        style={{ cursor: v.z > 1 ? "grab" : "zoom-in" }}
      >
        <img src={lado.src} alt={lado.rotulo} style={estilo} draggable={false} />
      </div>
      <figcaption>{lado.rotulo}</figcaption>
    </figure>
  );
  return (
    <div className={styles.compare}>
      <div className={styles.painelPar}>
        {painel(c.esquerda)}
        {painel(c.direita)}
      </div>
      <div className={styles.compareNota}>
        <span>{c.nota}</span>
        <span className={styles.dim}>roda: zoom · arrastar: mover · duplo clique: inteira</span>
      </div>
      {c.aviso && <div className={styles.avisoRede}>{c.aviso}</div>}
    </div>
  );
}
