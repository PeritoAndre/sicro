// @vitest-environment happy-dom
/**
 * Esc da marcação da mancha (P2.5, pendência (e) da Fase 2): com a análise escondida atrás da Edição (o lado dela
 * com `hidden`, `VideoModule`), o Esc é da Edição e não desiste da marcação; com a análise na frente, desiste.
 */

import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@core/commands", () => ({ commands: {} }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { PessoasOverlay } from "./PessoasOverlay";
import { usePessoasStore } from "./pessoasStore";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let raizes: Root[] = [];

/** Overlay dentro do lado da análise, dois níveis abaixo dele (como no `VideoModule`). */
async function montar(escondida: boolean): Promise<HTMLElement> {
  const lado = document.createElement("div");
  lado.setAttribute("data-lado", "analise");
  lado.hidden = escondida;
  const player = document.createElement("div");
  lado.appendChild(player);
  document.body.appendChild(lado);
  const r = createRoot(player);
  raizes.push(r);
  await act(async () => r.render(<PessoasOverlay wrap={null} view={{ s: 1, tx: 0, ty: 0 }} workspacePath="/ws" mediaHash="h" />));
  return lado;
}

const tecla = async (key: string) => {
  await act(async () => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key }));
  });
};

const marcando = () => usePessoasStore.getState().marcando;

describe("PessoasOverlay: Esc da marcação", () => {
  afterEach(() => {
    for (const r of raizes) act(() => r.unmount());
    raizes = [];
    document.body.innerHTML = "";
    usePessoasStore.setState({ marcando: null, estado: null });
  });

  it("com a análise escondida, o Esc não desiste; de volta à frente, desiste", async () => {
    usePessoasStore.setState({ marcando: { nome: "", direcao: "frente" }, estado: null });
    const lado = await montar(true);
    expect(lado.querySelector("canvas")).not.toBeNull();
    await tecla("Escape");
    expect(marcando()).toEqual({ nome: "", direcao: "frente" });
    lado.hidden = false;
    await tecla("Enter");
    expect(marcando()).not.toBeNull();
    await tecla("Escape");
    expect(marcando()).toBeNull();
  });

  it("com a análise na frente, o Esc desiste da marcação", async () => {
    usePessoasStore.setState({ marcando: { nome: "saco", direcao: "ambos", id: "p1" }, estado: null });
    await montar(false);
    await tecla("Escape");
    expect(marcando()).toBeNull();
  });
});
