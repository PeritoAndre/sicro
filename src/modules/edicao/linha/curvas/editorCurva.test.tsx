// @vitest-environment happy-dom
/**
 * Editor da curva (P4.2, D7) sozinho: losangos nos slots, arrasto em slots inteiros sem cruzar a vizinha (um
 * `aplicar` ao soltar; Esc desiste), clique leva o cursor, menu da interpolação, setas e Delete.
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import { act, useState } from "react";
import type { Animavel, Chave, Seg } from "@domain/edicao";
import { desmontarTudo, montar, type Tela } from "../../inspetor/testeTela";
import { textoPermitido } from "../../inspetor/rotulos.teste";
import { EditorCurva } from "./EditorCurva";

// item nos slots [100, 160) do programa, a 30 qps, tempo local; a faixa tem 600 px (10 px por slot)
const DE = 100;
const ATE = 160;
const deSlot = (q: number): Seg => (q - DE) / 30;
const paraSlot = (t: Seg) => DE + Math.round(t * 30);
const LARGURA = 600;

function Banco(props: { inicial: Animavel<number>; aplicar(v: Animavel<number>): void; onIr?(t: Seg): void; desligado?: boolean }) {
  const [v, setV] = useState(props.inicial);
  const [esc, setEsc] = useState<number | null>(null);
  return (
    <EditorCurva
      rotulo="Opacidade"
      valor={v}
      de_q={DE}
      ate_q={ATE}
      deSlot={deSlot}
      paraSlot={paraSlot}
      cursor_q={130}
      faixa={[0, 1]}
      escolhida={esc}
      onEscolher={setEsc}
      onIr={props.onIr}
      desligado={props.desligado}
      aplicar={(n) => {
        props.aplicar(n);
        setV(n);
      }}
    />
  );
}

const ks = (): Chave<number>[] => [
  { t: 0, v: 0.2, interp: "suave" },
  { t: 1, v: 1, interp: "linear" },
  { t: 1.5, v: 0.5 },
];

let tela: Tela;
afterEach(() => desmontarTudo());

async function abrir(props: Partial<Parameters<typeof Banco>[0]> = {}) {
  const aplicar = vi.fn();
  const onIr = vi.fn();
  tela = await montar(<Banco inicial={ks()} aplicar={aplicar} onIr={onIr} {...props} />);
  const faixa = tela.raiz.querySelector<HTMLElement>("[data-curva]")!;
  faixa.getBoundingClientRect = () => ({ left: 0, top: 0, width: LARGURA, height: 22, right: LARGURA, bottom: 22, x: 0, y: 0, toJSON: () => ({}) });
  return { aplicar, onIr, faixa };
}

const losango = (i: number) => tela.raiz.querySelector<HTMLButtonElement>(`[data-chave="${i}"]`)!;
const ponteiro = async (el: HTMLElement, tipo: string, x: number, extra: PointerEventInit = {}) =>
  act(async () => void el.dispatchEvent(new PointerEvent(tipo, { bubbles: true, clientX: x, button: 0, pointerId: 1, ...extra })));
/** x no meio do slot `q`. */
const xDo = (q: number) => (q - DE) * (LARGURA / (ATE - DE)) + 5;

describe("EditorCurva", () => {
  it("um losango por chave, no slot dela; a curva desenhada; rótulo de (h)", async () => {
    await abrir();
    expect([...tela.raiz.querySelectorAll("[data-chave]")].map((b) => Number(b.getAttribute("data-q")))).toEqual([100, 130, 145]);
    expect(losango(0).getAttribute("aria-label")).toBe("Opacidade");
    expect(textoPermitido(losango(0).getAttribute("aria-label")!)).toBe(true);
    expect(tela.raiz.querySelector("polyline")!.getAttribute("points")!.split(" ").length).toBe(61);
  });

  it("arrastar: só a tela muda até soltar; um aplicar com a chave no slot; nunca cruza a vizinha", async () => {
    const { aplicar } = await abrir();
    await ponteiro(losango(1), "pointerdown", xDo(130));
    await ponteiro(losango(1), "pointermove", xDo(138));
    expect(losango(1).getAttribute("data-q")).toBe("138");
    expect(aplicar).not.toHaveBeenCalled();
    await ponteiro(losango(1), "pointermove", xDo(152));
    // a vizinha está no 145: para no 144
    expect(losango(1).getAttribute("data-q")).toBe("144");
    await ponteiro(losango(1), "pointerup", xDo(152));
    expect(aplicar).toHaveBeenCalledTimes(1);
    expect(aplicar.mock.calls[0]![0]).toEqual([ks()[0], { t: 44 / 30, v: 1, interp: "linear" }, ks()[2]]);
    // para trás, até o slot depois da primeira
    await ponteiro(losango(1), "pointerdown", xDo(144));
    await ponteiro(losango(1), "pointermove", xDo(40));
    await ponteiro(losango(1), "pointerup", xDo(40));
    expect(aplicar.mock.calls[1]![0][1].t).toBe(1 / 30);
  });

  it("Esc no meio do arrasto desiste; clique sem mover leva o cursor e escolhe a chave", async () => {
    const { aplicar, onIr } = await abrir();
    await ponteiro(losango(2), "pointerdown", xDo(145));
    await ponteiro(losango(2), "pointermove", xDo(155));
    await act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
    expect(losango(2).getAttribute("data-q")).toBe("145");
    await ponteiro(losango(2), "pointerup", xDo(155));
    expect(aplicar).not.toHaveBeenCalled();
    await ponteiro(losango(2), "pointerdown", xDo(145));
    await ponteiro(losango(2), "pointerup", xDo(145));
    expect(aplicar).not.toHaveBeenCalled();
    expect(onIr).toHaveBeenCalledWith(1.5);
    expect(losango(2).getAttribute("aria-pressed")).toBe("true");
  });

  it("menu do botão direito: Linear, Suave, Degrau; a escolhida marcada; troca num aplicar", async () => {
    const { aplicar } = await abrir();
    await act(async () => void losango(0).dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 10, clientY: 10 })));
    const menu = document.querySelector<HTMLElement>("[data-menu-interp]")!;
    expect([...menu.querySelectorAll("button")].map((b) => [b.textContent, b.getAttribute("aria-checked")])).toEqual([
      ["Linear", "false"],
      ["Suave", "true"],
      ["Degrau", "false"],
    ]);
    await act(async () => menu.querySelector<HTMLButtonElement>('[data-valor="degrau"]')!.click());
    expect(aplicar).toHaveBeenCalledWith([{ ...ks()[0], interp: "degrau" }, ks()[1], ks()[2]]);
    expect(document.querySelector("[data-menu-interp]")).toBeNull();
    // a mesma: nada
    await act(async () => void losango(1).dispatchEvent(new MouseEvent("contextmenu", { bubbles: true })));
    await act(async () => document.querySelector<HTMLButtonElement>('[data-menu-interp] [data-valor="linear"]')!.click());
    expect(aplicar).toHaveBeenCalledTimes(1);
  });

  it("teclado: setas andam um slot (Shift: 10, sem cruzar); Delete tira, a última vira valor fixo", async () => {
    const { aplicar } = await abrir();
    const tecla = (i: number, key: string, shift = false) => act(async () => void losango(i).dispatchEvent(new KeyboardEvent("keydown", { key, shiftKey: shift, bubbles: true })));
    await tecla(1, "ArrowRight");
    expect(aplicar.mock.lastCall![0][1].t).toBe(31 / 30);
    await tecla(1, "ArrowRight", true);
    expect(aplicar.mock.lastCall![0][1].t).toBe(41 / 30);
    await tecla(1, "ArrowRight", true);
    expect(aplicar.mock.lastCall![0][1].t).toBe(44 / 30);
    await tecla(2, "Delete");
    expect(aplicar.mock.lastCall![0]).toHaveLength(2);
    await tecla(1, "Delete");
    await tecla(0, "Delete");
    expect(aplicar.mock.lastCall![0]).toBe(0.2);
    expect(tela.raiz.querySelector("[data-chave]")).toBeNull();
  });

  it("desligado (só leitura): nada arrasta", async () => {
    const { aplicar } = await abrir({ desligado: true });
    expect(losango(1).disabled).toBe(true);
    await ponteiro(losango(1), "pointerdown", xDo(130));
    await ponteiro(losango(1), "pointermove", xDo(140));
    await ponteiro(losango(1), "pointerup", xDo(140));
    expect(aplicar).not.toHaveBeenCalled();
  });
});
