/**
 * Só para testes (happy-dom): monta um componente de verdade e dispara eventos como o navegador. Os campos são
 * achados pelo rótulo visível, como o perito os acha.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { act, type ReactElement } from "react";
import { createRoot, type Root } from "react-dom/client";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

export interface Tela {
  raiz: HTMLElement;
  desmontar(): void;
}

let montadas: Root[] = [];

export async function montar(el: ReactElement): Promise<Tela> {
  const raiz = document.createElement("div");
  document.body.appendChild(raiz);
  const r = createRoot(raiz);
  montadas.push(r);
  await act(async () => r.render(el));
  return {
    raiz,
    desmontar: () => {
      act(() => r.unmount());
      raiz.remove();
    },
  };
}

export function desmontarTudo(): void {
  for (const r of montadas) act(() => r.unmount());
  montadas = [];
  document.body.innerHTML = "";
}

export const esperar = async (f?: () => void | Promise<void>) => {
  await act(async () => {
    await f?.();
  });
};

/** Elemento rotulado: o `label`/`div` de campo cujo rótulo é `texto` (o primeiro, a partir de `de`). */
export function campo(de: ParentNode, texto: string): HTMLElement {
  const achado = [...de.querySelectorAll<HTMLElement>("label, div, [data-campo]")].find((e) => {
    const r = e.querySelector(":scope > span");
    return (r?.textContent ?? "").trim() === texto || e.getAttribute("data-campo") === texto;
  });
  if (!achado) throw new Error(`campo sem rótulo "${texto}"`);
  return achado;
}

export function entradaDe(de: ParentNode, texto: string): HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement {
  const c = campo(de, texto);
  const e = c.querySelector<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>("input:not([type=range]), textarea, select") ?? c.querySelector("input");
  if (!e) throw new Error(`campo "${texto}" sem entrada`);
  return e;
}

export function botao(de: ParentNode, texto: string): HTMLButtonElement {
  const b = [...de.querySelectorAll<HTMLButtonElement>("button")].find((x) => (x.textContent ?? "").trim() === texto || x.getAttribute("aria-label") === texto);
  if (!b) throw new Error(`botão "${texto}"`);
  return b;
}

/** Troca o valor como o usuário (o React só vê a mudança pelo setter nativo). */
function setarValor(el: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement, v: string) {
  const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : el instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, "value")!.set!.call(el, v);
}

export async function digitar(el: HTMLInputElement | HTMLTextAreaElement, texto: string, op: { aos?: number } = {}) {
  // um caractere por vez, como no teclado
  for (let i = 1; i <= texto.length; i += op.aos ?? 1) {
    await act(async () => {
      setarValor(el, texto.slice(0, i));
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });
  }
  if (el.value !== texto)
    await act(async () => {
      setarValor(el, texto);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });
}

export async function sair(el: HTMLElement) {
  await act(async () => {
    el.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    el.dispatchEvent(new FocusEvent("blur"));
  });
}

export async function tecla(el: HTMLElement, key: string) {
  await act(async () => {
    el.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  });
}

export async function escolher(el: HTMLSelectElement, v: string) {
  await act(async () => {
    setarValor(el, v);
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

export async function clicar(el: HTMLElement) {
  await act(async () => {
    el.click();
  });
}

/** Arrasto de controle deslizante: aperta, passa pelos valores, solta. `entre` roda entre um valor e outro (arrasto
 *  lento: o relógio passa da janela de agrupamento, e só o gesto faz dele um passo). */
export async function arrastar(el: HTMLInputElement, valores: number[], op: { entre?: () => unknown } = {}) {
  await act(async () => {
    el.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
  });
  for (const v of valores)
    await act(async () => {
      await op.entre?.();
      setarValor(el, String(v));
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });
  await act(async () => {
    el.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));
  });
}

/** Todo texto visível (fora de campos e de `code`, que são dados). */
export function textosVisiveis(raiz: HTMLElement): string[] {
  const r: string[] = [];
  const w = document.createTreeWalker(raiz, NodeFilter.SHOW_TEXT);
  for (let n = w.nextNode(); n; n = w.nextNode()) {
    const t = (n.textContent ?? "").trim();
    if (!t) continue;
    const p = n.parentElement;
    if (p?.closest("code, textarea, output, [data-dado]")) continue;
    r.push(t);
  }
  return r;
}

/** Fixture do Rust (no ambiente happy-dom o `import.meta.url` não é `file:`). */
export const lerFixture = <T,>(nome: string): T =>
  JSON.parse(readFileSync(join(process.env.SICRO_FIXTURES_EDICAO ?? join(process.cwd(), "src-tauri/tests/fixtures/edicao"), nome), "utf8")) as T;
