/**
 * Editor da curva de uma propriedade animável (posição, giro, escala, opacidade do sobreposto; volume; D7): as chaves
 * como losangos sobre a faixa do item (os slots `[de_q, ate_q)` ocupam a largura do pai), arrastáveis em slots
 * inteiros sem cruzar a vizinha, e a interpolação de cada uma (Linear, Suave, Degrau) pelo menu do botão direito.
 * Arrastar = um passo (`aplicar` ao soltar; Esc desiste); clique = escolhe a chave e leva o cursor a ela; setas andam
 * um slot (Shift: 10); Delete tira. O mesmo componente vai no Inspetor e no item da linha do tempo. Dono: P4.2.
 */

import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent as ME, type PointerEvent as PE } from "react";
import { createPortal } from "react-dom";
import type { Animavel, Chave, Interp, Seg } from "@domain/edicao";
import { avaliar, ehChaves } from "../../modelo/curva";
import { interpDe, moverChave, tirarChaveEm, trocarInterp } from "./chaves";
import st from "./Curvas.module.css";

export const INTERPS: readonly { v: Interp; r: string }[] = [
  { v: "linear", r: "Linear" },
  { v: "suave", r: "Suave" },
  { v: "degrau", r: "Degrau" },
];

export interface EditorCurvaProps {
  /** Rótulo da propriedade (texto exato de contratos-fase4.md (h)). */
  rotulo: string;
  valor: Animavel<number>;
  /** Base de tempo das chaves → slots do programa (o item na linha). */
  deSlot(q: number): Seg;
  paraSlot(t: Seg): number;
  aplicar(v: Animavel<number>): void;
  // CONTRATO: acréscimos do P4.2 (a casca não dizia onde o item fica nem como a cabeça aparece)
  /** Slots do item `[de_q, ate_q)`: a faixa ocupa a largura do pai. */
  de_q: number;
  ate_q: number;
  /** Slot da cabeça de leitura (linha fina); null = não mostra. */
  cursor_q?: number | null;
  /** "inspetor": faixa própria; "linha": por cima do item, só os losangos pegam o ponteiro. */
  modo?: "inspetor" | "linha";
  /** Desenha o valor no tempo entre `faixa[0]` (embaixo) e `faixa[1]` (em cima); "auto" = entre as chaves. */
  faixa?: readonly [number, number] | "auto";
  /** Chave escolhida (destacada; a interpolação dela é a do grupo do Inspetor). */
  escolhida?: number | null;
  onEscolher?(i: number | null): void;
  /** Clique num losango: o cursor vai à chave. */
  onIr?(t: Seg): void;
  desligado?: boolean;
}

/** Folga do losango na ponta da faixa (px): na beira ele não sai cortado ao meio. */
const BEIRA = 6;

export function EditorCurva(p: EditorCurvaProps) {
  const { de_q, ate_q } = p;
  const n = Math.max(ate_q - de_q, 1);
  const ref = useRef<HTMLDivElement | null>(null);
  const ks: readonly Chave<number>[] = ehChaves(p.valor) ? p.valor : [];
  // arrasto: a chave, o slot de onde saiu e a lista com ela no lugar novo (só a tela muda até soltar)
  const [arrasto, setArrasto] = useState<{ i: number; q0: number; lista: Chave<number>[]; moveu: boolean } | null>(null);
  const [menu, setMenu] = useState<{ i: number; x: number; y: number } | null>(null);
  const lista = arrasto?.lista ?? ks;
  const slot = (t: Seg) => p.paraSlot(t);
  const esquerda = (q: number) => `clamp(${BEIRA}px, ${((q - de_q + 0.5) / n) * 100}%, calc(100% - ${BEIRA}px))`;
  // com escala de valor, o losango fica na altura do valor (sobre a curva); sem ela, no meio
  const faixaFixa = p.faixa && p.faixa !== "auto" ? p.faixa : null;
  const altura = (v: number) => {
    // no item da linha, sem escala, embaixo: o nome do item fica em cima
    if (!faixaFixa || !(faixaFixa[1] > faixaFixa[0])) return p.modo === "linha" ? `calc(100% - ${BEIRA + 3}px)` : undefined;
    const y = 1 - (Math.min(Math.max(v, faixaFixa[0]), faixaFixa[1]) - faixaFixa[0]) / (faixaFixa[1] - faixaFixa[0]);
    return `clamp(${BEIRA}px, calc(3px + ${y} * (100% - 6px)), calc(100% - ${BEIRA}px))`;
  };

  /** Slot sob o x da tela (px), dentro do item; null sem tamanho (fora da tela). */
  const slotNoX = (x: number): number | null => {
    const r = ref.current?.getBoundingClientRect();
    if (!r || !(r.width > 0)) return null;
    return Math.min(Math.max(de_q + Math.floor(((x - r.left) / r.width) * n), de_q), ate_q - 1);
  };

  /** A lista com a chave `i` no slot `q`, entre as vizinhas (em slots e no tempo); null = não cabe. */
  const noSlot = (base: readonly Chave<number>[], i: number, q: number): Chave<number>[] | null => {
    const antes = base[i - 1];
    const depois = base[i + 1];
    const min = antes ? slot(antes.t) + 1 : de_q;
    const max = depois ? slot(depois.t) - 1 : ate_q - 1;
    if (min > max) return null;
    const t = p.deSlot(Math.min(Math.max(q, min), max));
    if ((antes && !(t > antes.t)) || (depois && !(t < depois.t))) return null;
    return moverChave(base, i, t);
  };

  useEffect(() => {
    if (!arrasto) return;
    const esc = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        setArrasto(null);
      }
    };
    window.addEventListener("keydown", esc, true);
    return () => window.removeEventListener("keydown", esc, true);
  }, [arrasto]);

  useEffect(() => {
    if (!menu) return;
    const fechar = (e: Event) => {
      if (e instanceof KeyboardEvent && e.key !== "Escape") return;
      if (e.type === "pointerdown" && (e.target as Element | null)?.closest?.(`.${st.menu}`)) return;
      setMenu(null);
    };
    window.addEventListener("pointerdown", fechar, true);
    window.addEventListener("keydown", fechar, true);
    return () => {
      window.removeEventListener("pointerdown", fechar, true);
      window.removeEventListener("keydown", fechar, true);
    };
  }, [menu]);

  const pegar = (i: number) => (e: PE<HTMLButtonElement>) => {
    if (e.button !== 0 || p.desligado) return;
    e.preventDefault();
    e.stopPropagation();
    e.currentTarget.setPointerCapture?.(e.pointerId);
    setArrasto({ i, q0: slot((ks[i] as Chave<number>).t), lista: [...ks], moveu: false });
  };
  const mover = (e: PE<HTMLButtonElement>) => {
    if (!arrasto) return;
    const q = slotNoX(e.clientX);
    if (q === null) return;
    const l = noSlot(ks, arrasto.i, q);
    if (l) setArrasto({ ...arrasto, lista: l, moveu: arrasto.moveu || q !== arrasto.q0 });
  };
  const soltar = (i: number) => (e: PE<HTMLButtonElement>) => {
    if (!arrasto) return;
    e.currentTarget.releasePointerCapture?.(e.pointerId);
    const a = arrasto;
    setArrasto(null);
    const k = a.lista[a.i] as Chave<number>;
    if (a.moveu && k.t !== (ks[a.i] as Chave<number>).t) {
      p.aplicar(a.lista);
      p.onEscolher?.(i);
      return;
    }
    p.onEscolher?.(i);
    p.onIr?.((ks[i] as Chave<number>).t);
  };

  const teclas = (i: number) => (e: KeyboardEvent<HTMLButtonElement>) => {
    if (p.desligado) return;
    const k = ks[i] as Chave<number>;
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      const l = noSlot(ks, i, slot(k.t) + (e.key === "ArrowLeft" ? -1 : 1) * (e.shiftKey ? 10 : 1));
      if (l && (l[i] as Chave<number>).t !== k.t) {
        p.aplicar(l);
        p.onEscolher?.(i);
      }
    } else if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      e.stopPropagation();
      p.aplicar(tirarChaveEm(ks as Chave<number>[], k.t));
      p.onEscolher?.(null);
    } else if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
      e.preventDefault();
      const r = e.currentTarget.getBoundingClientRect();
      setMenu({ i, x: r.left, y: r.bottom + 4 });
    }
  };
  const abrirMenu = (i: number) => (e: ME<HTMLButtonElement>) => {
    e.preventDefault();
    if (p.desligado) return;
    p.onEscolher?.(i);
    setMenu({ i, x: e.clientX, y: e.clientY });
  };
  const escolherInterp = (i: number, v: Interp) => {
    setMenu(null);
    const k = ks[i];
    if (k && interpDe(k) !== v) p.aplicar(trocarInterp(ks, i, v));
  };

  // o valor no tempo, um ponto por slot (até 240)
  const pontos = useMemo(() => {
    if (!p.faixa || lista.length === 0) return null;
    const vs = lista.map((k) => k.v);
    const [lo, hi] = p.faixa === "auto" ? [Math.min(...vs), Math.max(...vs)] : p.faixa;
    const a: Animavel<number> = lista as Chave<number>[];
    const m = Math.min(n, 240);
    const pts: string[] = [];
    for (let j = 0; j <= m; j++) {
      const q = de_q + Math.min(Math.floor((j / m) * n), n - 1);
      const v = avaliar(a, p.deSlot(q));
      const y = hi > lo ? 1 - (v - lo) / (hi - lo) : 0.5;
      pts.push(`${((j / m) * 1000).toFixed(1)},${(Math.min(Math.max(y, 0), 1) * 100).toFixed(2)}`);
    }
    return pts.join(" ");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lista, p.faixa, de_q, n, p.deSlot]);

  const cursorDentro = p.cursor_q != null && p.cursor_q >= de_q && p.cursor_q < ate_q;
  const classes = [st.curva, p.modo === "linha" ? st.naLinha : st.noInspetor, p.desligado && st.desligada, arrasto && st.andando].filter(Boolean).join(" ");
  return (
    <div ref={ref} className={classes} data-curva={p.rotulo} data-de-q={de_q} data-ate-q={ate_q}>
      {pontos && (
        <svg className={st.traco} viewBox="0 0 1000 100" preserveAspectRatio="none" aria-hidden>
          <polyline points={pontos} vectorEffect="non-scaling-stroke" />
        </svg>
      )}
      {cursorDentro && <i className={st.cabeca} style={{ left: `${(((p.cursor_q as number) - de_q + 0.5) / n) * 100}%` }} aria-hidden />}
      {lista.map((k, i) => {
        const q0 = slot(k.t);
        // a chave no fim do item (t = duração) é dele; a de fora (o item encolheu depois dela) ainda pesa na curva:
        // fica na beira, apagada, para arrastar para dentro ou tirar
        const fora = q0 < de_q || q0 > ate_q;
        const q = Math.min(Math.max(q0, de_q), ate_q);
        const escolhida = p.escolhida === i;
        return (
          <button
            key={i}
            type="button"
            className={[st.losango, escolhida && st.escolhido, fora && st.fora].filter(Boolean).join(" ")}
            style={{ left: esquerda(q), top: altura(k.v) }}
            aria-label={p.rotulo}
            aria-pressed={escolhida}
            title={p.rotulo}
            data-chave={i}
            data-q={q0}
            data-fora={fora || undefined}
            data-interp={interpDe(k)}
            disabled={p.desligado}
            onPointerDown={pegar(i)}
            onPointerMove={mover}
            onPointerUp={soltar(i)}
            onPointerCancel={() => setArrasto(null)}
            onKeyDown={teclas(i)}
            onContextMenu={abrirMenu(i)}
          />
        );
      })}
      {menu &&
        ks[menu.i] &&
        createPortal(
          <div className={st.menu} role="menu" style={{ left: menu.x, top: menu.y }} data-menu-interp={menu.i}>
            {INTERPS.map((o) => (
              <button key={o.v} type="button" role="menuitemradio" aria-checked={interpDe(ks[menu.i] as Chave<number>) === o.v} data-valor={o.v} onClick={() => escolherInterp(menu.i, o.v)}>
                {o.r}
              </button>
            ))}
          </div>,
          document.body,
        )}
    </div>
  );
}
