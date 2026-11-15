// @vitest-environment happy-dom
/**
 * Gesto das setas (§8.5), o mesmo da análise e da Edição: toque = um quadro no keyup, segurar = movimento até soltar,
 * Shift = ±1 s; nada com campo em foco, modificador ou `ativo` falso.
 */

import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SEGURAR_MS, useGestoTransporte, type AcoesDoGesto } from "./useGestoTransporte";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let raiz: Root | null = null;
let log: string[] = [];
const acoes = (prefixo = ""): AcoesDoGesto => ({
  passo: (d) => log.push(`${prefixo}passo ${d}`),
  mover: (d) => log.push(`${prefixo}mover ${d}`),
  parar: () => log.push(`${prefixo}parar`),
  saltar: (s) => log.push(`${prefixo}saltar ${s}`),
});

function Gancho({ a, ativo }: { a: AcoesDoGesto; ativo: boolean }) {
  useGestoTransporte(a, ativo);
  return null;
}
const montar = (a: AcoesDoGesto, ativo = true) =>
  act(() => {
    if (!raiz) {
      const div = document.createElement("div");
      document.body.appendChild(div);
      raiz = createRoot(div);
    }
    raiz.render(createElement(Gancho, { a, ativo }));
  });

/** Dispara e devolve se o evento foi consumido (preventDefault). */
const tecla = (tipo: "keydown" | "keyup", key: string, mods: Partial<KeyboardEventInit> = {}) => {
  const e = new KeyboardEvent(tipo, { key, bubbles: true, cancelable: true, ...mods });
  act(() => void window.dispatchEvent(e));
  return e.defaultPrevented;
};
const toque = (key: string, mods: Partial<KeyboardEventInit> = {}) => {
  tecla("keydown", key, mods);
  act(() => vi.advanceTimersByTime(SEGURAR_MS - 1));
  tecla("keyup", key, mods);
};
const segurar = (key: string, ms = SEGURAR_MS) => {
  tecla("keydown", key);
  act(() => vi.advanceTimersByTime(ms));
};

beforeEach(() => {
  vi.useFakeTimers();
  log = [];
});
afterEach(() => {
  act(() => raiz?.unmount());
  raiz = null;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

describe("useGestoTransporte", () => {
  it("toque: um quadro no keyup, nada no keydown", () => {
    montar(acoes());
    expect(tecla("keydown", "ArrowRight")).toBe(true);
    expect(log).toEqual([]);
    act(() => vi.advanceTimersByTime(SEGURAR_MS - 1));
    tecla("keyup", "ArrowRight");
    toque("ArrowLeft");
    expect(log).toEqual(["passo 1", "passo -1"]);
  });

  it("segurar: move a partir de SEGURAR_MS e para ao soltar, sem passo", () => {
    montar(acoes());
    segurar("ArrowRight", SEGURAR_MS - 1);
    expect(log).toEqual([]);
    act(() => vi.advanceTimersByTime(1));
    expect(log).toEqual(["mover 1"]);
    // a repetição do sistema não conta
    tecla("keydown", "ArrowRight", { repeat: true });
    act(() => vi.advanceTimersByTime(5 * SEGURAR_MS));
    tecla("keyup", "ArrowRight");
    segurar("ArrowLeft");
    tecla("keyup", "ArrowLeft");
    expect(log).toEqual(["mover 1", "parar", "mover -1", "parar"]);
  });

  it("Shift+seta: ±1 s a cada keydown, também na repetição; o keyup não dá passo", () => {
    montar(acoes());
    expect(tecla("keydown", "ArrowRight", { shiftKey: true })).toBe(true);
    tecla("keydown", "ArrowRight", { shiftKey: true, repeat: true });
    tecla("keyup", "ArrowRight", { shiftKey: true });
    tecla("keydown", "ArrowLeft", { shiftKey: true });
    tecla("keyup", "ArrowLeft");
    act(() => vi.advanceTimersByTime(2 * SEGURAR_MS));
    expect(log).toEqual(["saltar 1", "saltar 1", "saltar -1"]);
  });

  it("uma direção por vez: a outra seta apertada junto é ignorada", () => {
    montar(acoes());
    tecla("keydown", "ArrowRight");
    tecla("keydown", "ArrowLeft");
    tecla("keyup", "ArrowLeft");
    tecla("keyup", "ArrowRight");
    expect(log).toEqual(["passo 1"]);
  });

  it("Ctrl, Alt e Meta (Seguir na Edição) e campo editável em foco: nada, e a tecla segue para quem for", () => {
    montar(acoes());
    for (const mods of [{ ctrlKey: true }, { altKey: true }, { metaKey: true }]) {
      expect(tecla("keydown", "ArrowRight", mods)).toBe(false);
      act(() => vi.advanceTimersByTime(SEGURAR_MS));
      tecla("keyup", "ArrowRight", mods);
    }
    for (const html of ["<input>", "<textarea></textarea>", "<select></select>", '<div contenteditable="true"></div>']) {
      const box = document.createElement("div");
      box.innerHTML = html;
      document.body.appendChild(box);
      const el = box.firstElementChild as HTMLElement;
      if (el.getAttribute("contenteditable")) Object.defineProperty(el, "isContentEditable", { value: true });
      el.tabIndex = 0;
      el.focus();
      expect(document.activeElement).toBe(el);
      expect(tecla("keydown", "ArrowLeft")).toBe(false);
      tecla("keydown", "ArrowLeft", { shiftKey: true });
      act(() => vi.advanceTimersByTime(SEGURAR_MS));
      tecla("keyup", "ArrowLeft");
      el.blur();
      box.remove();
    }
    expect(log).toEqual([]);
    toque("ArrowRight");
    expect(log).toEqual(["passo 1"]);
  });

  it("ativo falso: nada; desligar segurando para o movimento uma vez e o keyup não dá passo", () => {
    montar(acoes(), false);
    toque("ArrowRight");
    segurar("ArrowRight");
    tecla("keyup", "ArrowRight");
    tecla("keydown", "ArrowRight", { shiftKey: true });
    expect(log).toEqual([]);
    montar(acoes(), true);
    segurar("ArrowLeft");
    montar(acoes(), false);
    tecla("keyup", "ArrowLeft");
    expect(log).toEqual(["mover -1", "parar"]);
    // desligar no meio do toque: nem passo nem movimento depois
    montar(acoes(), true);
    tecla("keydown", "ArrowRight");
    montar(acoes(), false);
    act(() => vi.advanceTimersByTime(2 * SEGURAR_MS));
    tecla("keyup", "ArrowRight");
    expect(log).toEqual(["mover -1", "parar"]);
  });

  it("janela perde o foco segurando: para já (o keyup não virá) e a próxima seta vale", () => {
    montar(acoes());
    segurar("ArrowRight");
    act(() => void window.dispatchEvent(new Event("blur")));
    expect(log).toEqual(["mover 1", "parar"]);
    // o foco volta com a seta ainda apertada: a repetição do sistema não recomeça o movimento
    tecla("keydown", "ArrowRight", { repeat: true });
    act(() => vi.advanceTimersByTime(2 * SEGURAR_MS));
    tecla("keyup", "ArrowRight");
    expect(log).toEqual(["mover 1", "parar"]);
    toque("ArrowLeft");
    expect(log).toEqual(["mover 1", "parar", "passo -1"]);
  });

  it("usa as ações do último render e desmontar segurando para", () => {
    montar(acoes("a:"));
    montar(acoes("b:"));
    toque("ArrowRight");
    segurar("ArrowLeft");
    act(() => raiz?.unmount());
    raiz = null;
    tecla("keyup", "ArrowLeft");
    toque("ArrowRight");
    expect(log).toEqual(["b:passo 1", "b:mover -1", "b:parar"]);
  });
});
