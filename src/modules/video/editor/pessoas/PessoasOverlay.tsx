/**
 * Sobreposição das pessoas sobre o player: caixa, número e seta acompanhando o quadro
 * exibido (também pausado e no passo por quadro). A pessoa selecionada se corrige
 * arrastando a caixa ou as alças; o ajuste vale dali até a próxima correção.
 */
import { useEffect, useRef, useState } from "react";
import { containGeom } from "../useMagnifier";
import { desenharPessoa } from "./drawPessoa";
import { caixaEm, quadroMaisProximo, type Caixa } from "./pessoasMath";
import { cadenciaDe, usePessoasStore } from "./pessoasStore";

interface View {
  s: number;
  tx: number;
  ty: number;
}

interface Props {
  wrap: HTMLElement | null;
  view: View;
  workspacePath: string;
  mediaHash: string;
}

type Alca = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";
const ALCAS: Alca[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
const CURSOR: Record<Alca, string> = {
  nw: "nwse-resize", se: "nwse-resize", ne: "nesw-resize", sw: "nesw-resize",
  n: "ns-resize", s: "ns-resize", e: "ew-resize", w: "ew-resize",
};

const alcaPos = (a: Alca, c: Caixa): [number, number] => [
  a.includes("w") ? c.x : a.includes("e") ? c.x + c.w : c.x + c.w / 2,
  a.includes("n") ? c.y : a.includes("s") ? c.y + c.h : c.y + c.h / 2,
];

interface Arraste {
  id: string;
  tipo: "mover" | Alca;
  x0: number;
  y0: number;
  caixa0: Caixa;
  t: number;
}

export function PessoasOverlay({ wrap, view, workspacePath, mediaHash }: Props) {
  const ref = useRef<HTMLCanvasElement | null>(null);
  const estado = usePessoasStore((s) => s.estado);
  const selecionada = usePessoasStore((s) => s.selecionada);
  const mostrar = usePessoasStore((s) => s.mostrar);
  const desenho = usePessoasStore((s) => s.desenho);
  const juntando = usePessoasStore((s) => s.juntando);
  const [draft, setDraft] = useState<{ id: string; caixa: Caixa } | null>(null);
  const arrasteRef = useRef<Arraste | null>(null);

  // Tudo que o laço de desenho lê, sempre atual (o laço roda uma vez só).
  const dados = useRef({ estado, selecionada, mostrar, desenho, draft, view, sujo: true });
  dados.current = { estado, selecionada, mostrar, desenho, draft, view, sujo: true };
  const quadroRef = useRef<{ mt: number } | null>(null);

  /** Instante do quadro exibido: o do vídeo apresentado, quando o navegador informa. */
  const instante = (v: HTMLVideoElement): number => {
    const q = quadroRef.current;
    return q && Math.abs(q.mt - v.currentTime) < 0.25 ? q.mt : v.currentTime;
  };

  useEffect(() => {
    if (!wrap) return;
    const v = wrap.querySelector("video") as (HTMLVideoElement & {
      requestVideoFrameCallback?: (cb: (now: number, meta: { mediaTime: number }) => void) => number;
      cancelVideoFrameCallback?: (id: number) => void;
    }) | null;
    if (!v?.requestVideoFrameCallback) return;
    let id = 0;
    const cb = (_: number, meta: { mediaTime: number }) => {
      quadroRef.current = { mt: meta.mediaTime };
      id = v.requestVideoFrameCallback!(cb);
    };
    id = v.requestVideoFrameCallback(cb);
    return () => v.cancelVideoFrameCallback?.(id);
  }, [wrap]);

  useEffect(() => {
    if (!wrap) return;
    let raf = 0;
    let ultimo = "";
    const laco = () => {
      raf = requestAnimationFrame(laco);
      const c = ref.current;
      const g = containGeom(wrap);
      const d = dados.current;
      const run = d.estado?.run;
      if (!c || !g || !run) return;
      const t = instante(g.video);
      const chave = `${t}|${d.view.s}|${d.view.tx}|${d.view.ty}|${g.W}|${g.H}|${d.sujo}`;
      if (!d.sujo && chave === ultimo) return;
      ultimo = chave;
      d.sujo = false;
      const dpr = window.devicePixelRatio || 1;
      if (c.width !== Math.round(g.W * dpr) || c.height !== Math.round(g.H * dpr)) {
        c.width = Math.round(g.W * dpr);
        c.height = Math.round(g.H * dpr);
      }
      const ctx = c.getContext("2d");
      if (!ctx) return;
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.clearRect(0, 0, c.width, c.height);
      if (!d.mostrar || !d.estado) return;
      const cad = cadenciaDe(d.estado)!;
      const k = (g.cw / run.width) * d.view.s * dpr;
      ctx.setTransform(k, 0, 0, k, (g.cx * d.view.s + d.view.tx) * dpr, (g.cy * d.view.s + d.view.ty) * dpr);
      const dims = { w: run.width, h: run.height };
      let sel: { caixa: Caixa; cor: string } | null = null;
      for (const p of d.estado.pessoas) {
        const eh = p.id === d.selecionada;
        const caixa = eh && d.draft?.id === p.id ? d.draft.caixa : caixaEm(p, t, cad);
        if (!caixa) continue;
        desenharPessoa(ctx, caixa, p.numero, p.cor, d.desenho.modo, dims, d.desenho.fonte, eh);
        if (eh) sel = { caixa, cor: p.cor };
      }
      // alças da pessoa selecionada
      if (sel) {
        const r = 4.5 / k * dpr;
        ctx.fillStyle = "#fff";
        ctx.strokeStyle = "#141414";
        ctx.lineWidth = 1 / k * dpr;
        for (const a of ALCAS) {
          const [x, y] = alcaPos(a, sel.caixa);
          ctx.fillRect(x - r, y - r, 2 * r, 2 * r);
          ctx.strokeRect(x - r, y - r, 2 * r, 2 * r);
        }
      }
    };
    raf = requestAnimationFrame(laco);
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wrap]);

  /** Ponto do mouse em pixels do vídeo, mais o contexto da imagem. */
  const noVideo = (e: { clientX: number; clientY: number }) => {
    const g = containGeom(wrap);
    const run = estado?.run;
    const el = ref.current;
    if (!g || !run || !el) return null;
    const r = el.getBoundingClientRect();
    const v = dados.current.view;
    const kx = run.width / g.cw;
    const px = ((e.clientX - r.left - v.tx) / v.s - g.cx) * kx;
    const py = ((e.clientY - r.top - v.ty) / v.s - g.cy) * kx;
    // tamanho de um pixel de tela em pixels do vídeo (para alças e tolerância)
    return { x: px, y: py, g, run, tela: kx / v.s, t: instante(g.video) };
  };

  const sobre = (px: number, py: number, tela: number, t: number) => {
    const cad = cadenciaDe(estado);
    if (!cad || !estado) return null;
    const sel = estado.pessoas.find((p) => p.id === selecionada);
    if (sel) {
      const c = draft?.id === sel.id ? draft.caixa : caixaEm(sel, t, cad);
      if (c) {
        for (const a of ALCAS) {
          const [hx, hy] = alcaPos(a, c);
          if (Math.abs(px - hx) <= 7 * tela && Math.abs(py - hy) <= 7 * tela) return { id: sel.id, tipo: a as Arraste["tipo"], caixa: c };
        }
        if (px >= c.x && px <= c.x + c.w && py >= c.y && py <= c.y + c.h) return { id: sel.id, tipo: "mover" as const, caixa: c };
      }
    }
    // outra pessoa: a de menor caixa que contém o ponto
    let melhor: { id: string; caixa: Caixa } | null = null;
    for (const p of estado.pessoas) {
      if (p.id === selecionada) continue;
      const c = caixaEm(p, t, cad);
      if (c && px >= c.x && px <= c.x + c.w && py >= c.y && py <= c.y + c.h) {
        if (!melhor || c.w * c.h < melhor.caixa.w * melhor.caixa.h) melhor = { id: p.id, caixa: c };
      }
    }
    return melhor ? { id: melhor.id, tipo: "selecionar" as const, caixa: melhor.caixa } : null;
  };

  const onDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (e.button !== 0 || !mostrar) return;
    const m = noVideo(e);
    if (!m) return;
    const h = sobre(m.x, m.y, m.tela, m.t);
    if (!h) return; // fora de qualquer pessoa: deixa o player (lupa, arrastar)
    e.stopPropagation();
    if (h.tipo === "selecionar") {
      if (juntando && juntando !== h.id) {
        void usePessoasStore.getState().editar(workspacePath, mediaHash, { op: "juntar", destino: juntando, origem: h.id });
        usePessoasStore.getState().setJuntando(null);
      } else {
        usePessoasStore.getState().selecionar(h.id);
      }
      return;
    }
    const p = estado?.pessoas.find((x) => x.id === h.id);
    const t = p ? quadroMaisProximo(p, m.t) : null;
    if (!p || t == null) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    arrasteRef.current = { id: h.id, tipo: h.tipo, x0: m.x, y0: m.y, caixa0: h.caixa, t };
    setDraft({ id: h.id, caixa: h.caixa });
  };

  const onMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const m = noVideo(e);
    if (!m) return;
    const a = arrasteRef.current;
    if (!a) {
      const h = mostrar ? sobre(m.x, m.y, m.tela, m.t) : null;
      e.currentTarget.style.cursor =
        !h ? "" : h.tipo === "mover" ? "move" : h.tipo === "selecionar" ? (juntando ? "copy" : "pointer") : CURSOR[h.tipo];
      return;
    }
    e.stopPropagation();
    const dx = m.x - a.x0;
    const dy = m.y - a.y0;
    const c = { ...a.caixa0 };
    if (a.tipo === "mover") {
      c.x += dx;
      c.y += dy;
    } else {
      if (a.tipo.includes("w")) {
        c.x += dx;
        c.w -= dx;
      }
      if (a.tipo.includes("e")) c.w += dx;
      if (a.tipo.includes("n")) {
        c.y += dy;
        c.h -= dy;
      }
      if (a.tipo.includes("s")) c.h += dy;
      if (c.w < 4) c.w = 4;
      if (c.h < 4) c.h = 4;
    }
    setDraft({ id: a.id, caixa: c });
  };

  const onUp = async (e: React.PointerEvent<HTMLCanvasElement>) => {
    const a = arrasteRef.current;
    if (!a) return;
    e.stopPropagation();
    arrasteRef.current = null;
    const c = draft?.id === a.id ? draft.caixa : null;
    if (c && (Math.abs(c.x - a.caixa0.x) > 0.5 || Math.abs(c.y - a.caixa0.y) > 0.5 || Math.abs(c.w - a.caixa0.w) > 0.5 || Math.abs(c.h - a.caixa0.h) > 0.5)) {
      const r = (n: number) => Math.round(n * 10) / 10;
      await usePessoasStore.getState().editar(workspacePath, mediaHash, {
        op: "ajustar", id: a.id, t: a.t, x: r(c.x), y: r(c.y), w: r(c.w), h: r(c.h),
      });
    }
    setDraft(null);
  };

  if (!estado?.run) return null;
  return (
    <canvas
      ref={ref}
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={(e) => void onUp(e)}
      onPointerCancel={() => {
        arrasteRef.current = null;
        setDraft(null);
      }}
      style={{
        position: "absolute",
        inset: 0,
        width: "100%",
        height: "100%",
        zIndex: 2,
        touchAction: "none",
        pointerEvents: mostrar ? "auto" : "none",
      }}
    />
  );
}
