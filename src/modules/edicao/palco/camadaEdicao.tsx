/**
 * Camada de edição sobre o canvas (§7.2): alças do item selecionado e a ferramenta de marcação. Posições em
 * 0..1 do canvas da saída, levadas à tela pela vista. Um arrasto = um gesto (quem chama abre e fecha).
 */

import { useState, type PointerEvent as PE } from "react";
import type { Alca, Caixa, PedidoMarcacao, Ponto } from "@domain/edicao";
import { caixaDeCantos } from "./alcas";
import { daTela, paraTela, type Retangulo, type Vista } from "./vista";
import type { Fase } from "./Transporte";
import styles from "./Palco.module.css";
import { capturar as prenderPonteiro } from "./capturar";

export type Modificadores = { shift: boolean; alt: boolean };

interface Props {
  alcas: readonly Alca[];
  vista: Vista;
  img: Retangulo;
  /** Contorno a desenhar (região do enquadramento na vista de origem), em 0..1 do canvas. */
  contorno: Caixa | null;
  ferramenta: PedidoMarcacao | null;
  /** Tamanho da saída em px: a proporção pedida é de px, não de 0..1. */
  saida: { w: number; h: number };
  onArrasto(a: Alca, de: Ponto, para: Ponto, mod: Modificadores, fase: Fase): void;
  onMarcar(v: Caixa | Ponto, fim: boolean): void;
  /** Arrasto no fundo, sem ferramenta e fora das alças: deslocar a vista. */
  onFundo(dx: number, dy: number, fase: Fase): void;
}

/** Caixa entre dois pontos do canvas com a proporção pedida (largura/altura em px da saída). */
export function caixaComProporcao(a: Ponto, b: Ponto, proporcao: number | undefined, saida: { w: number; h: number }): Caixa {
  if (!proporcao || !(proporcao > 0) || !(saida.w > 0 && saida.h > 0)) return caixaDeCantos(a, b);
  const wpx = Math.abs(b.x - a.x) * saida.w;
  const hpx = Math.abs(b.y - a.y) * saida.h;
  // vale o lado que mais andou
  const [w, h] = wpx / hpx >= proporcao ? [wpx, wpx / proporcao] : [hpx * proporcao, hpx];
  const fim = { x: a.x + Math.sign(b.x - a.x || 1) * (w / saida.w), y: a.y + Math.sign(b.y - a.y || 1) * (h / saida.h) };
  return caixaDeCantos(a, fim);
}

const LADO_ALCA = 10;

/** Com a ferramenta de rastreio do próprio efeito ligada (ela segue ligada depois de marcar e depois de o Seguir
 *  perder o alvo), as alças continuam: arrastar ajusta a caixa; desenhar fora delas marca uma caixa nova. */
export function alcasComFerramenta(f: PedidoMarcacao | null, efeito: string | null | undefined): boolean {
  return !f || (!!f.rastreio && (f.tipo === "caixa" || f.tipo === "ponto") && f.efeito === efeito);
}

export function CamadaEdicao(p: Props) {
  const [rascunho, setRascunho] = useState<Caixa | null>(null);
  const local = (e: { clientX: number; clientY: number }, el: Element) => {
    const r = el.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };

  const capturar = (e: PE<HTMLElement>, mover: (ev: PointerEvent) => void, soltar: (ev: PointerEvent) => void) => {
    const alvo = e.currentTarget;
    prenderPonteiro(alvo, e.pointerId);
    const fim = (ev: PointerEvent) => {
      alvo.removeEventListener("pointermove", mover);
      alvo.removeEventListener("pointerup", fim);
      alvo.removeEventListener("pointercancel", fim);
      soltar(ev);
    };
    alvo.addEventListener("pointermove", mover);
    alvo.addEventListener("pointerup", fim);
    alvo.addEventListener("pointercancel", fim);
  };

  const noFundo = (e: PE<HTMLDivElement>) => {
    if (e.button !== 0 && e.button !== 1) return;
    const raiz = e.currentTarget;
    e.preventDefault();
    const ini = local(e, raiz);
    const f = p.ferramenta;
    if (f && e.button === 0) {
      const a = daTela(p.vista, p.img, ini);
      const ponto = f.tipo === "ponto" || f.tipo === "lugar";
      if (!ponto) setRascunho(caixaDeCantos(a, a));
      capturar(
        e,
        (ev) => {
          const b = daTela(p.vista, p.img, local(ev, raiz));
          if (!ponto) setRascunho(caixaComProporcao(a, b, f.proporcao, p.saida));
        },
        (ev) => {
          const b = daTela(p.vista, p.img, local(ev, raiz));
          setRascunho(null);
          p.onMarcar(ponto ? { x: Math.min(Math.max(b.x, 0), 1), y: Math.min(Math.max(b.y, 0), 1) } : caixaComProporcao(a, b, f.proporcao, p.saida), true);
        },
      );
      return;
    }
    let ultimo = ini;
    p.onFundo(0, 0, "inicio");
    capturar(
      e,
      (ev) => {
        const q = local(ev, raiz);
        p.onFundo(q.x - ultimo.x, q.y - ultimo.y, "move");
        ultimo = q;
      },
      () => p.onFundo(0, 0, "fim"),
    );
  };

  const naAlca = (a: Alca) => (e: PE<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const raiz = e.currentTarget.parentElement as HTMLElement;
    const de = daTela(p.vista, p.img, local(e, raiz));
    const mod = (ev: { shiftKey: boolean; altKey: boolean }) => ({ shift: ev.shiftKey, alt: ev.altKey });
    p.onArrasto(a, de, de, mod(e), "inicio");
    capturar(
      e,
      (ev) => p.onArrasto(a, de, daTela(p.vista, p.img, local(ev, raiz)), mod(ev), "move"),
      (ev) => p.onArrasto(a, de, daTela(p.vista, p.img, local(ev, raiz)), mod(ev), "fim"),
    );
  };

  const caixaNaTela = (c: Caixa) => {
    const a = paraTela(p.vista, p.img, { x: c.x, y: c.y });
    const b = paraTela(p.vista, p.img, { x: c.x + c.w, y: c.y + c.h });
    return { left: a.x, top: a.y, width: b.x - a.x, height: b.y - a.y };
  };

  return (
    <div className={`${styles.camada} ${p.ferramenta ? styles.marcando : ""}`} onPointerDown={noFundo} data-camada="">
      {p.contorno && <div className={styles.contorno} style={caixaNaTela(p.contorno)} />}
      {rascunho && <div className={styles.rascunho} style={caixaNaTela(rascunho)} data-rascunho="" />}
      {p.alcas.map((a) => {
          const q = paraTela(p.vista, p.img, a.em);
          return (
            <div
              key={a.id}
              className={`${styles.alca} ${a.tipo === "mover" ? styles.alcaMover : ""}`}
              style={{ left: q.x - LADO_ALCA / 2, top: q.y - LADO_ALCA / 2, width: LADO_ALCA, height: LADO_ALCA, cursor: a.cursor }}
              onPointerDown={naAlca(a)}
              data-alca={a.id}
            />
          );
        })}
    </div>
  );
}
