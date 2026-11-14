// @vitest-environment happy-dom
/** Diálogo do app: prende o foco (Tab e Shift+Tab ficam dentro), devolve o foco ao fechar, só o de cima responde
 *  ao Esc, e a pilha de abertos serve às telas que desligam atalhos (contratos (h) 25 e 27). */

import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Dialog, useDialogosAbertos } from "./Dialog";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let raiz: Root | null = null;
afterEach(() => {
  act(() => raiz?.unmount());
  raiz = null;
  document.body.innerHTML = "";
});

const tecla = (key: string, shiftKey = false) => act(() => void window.dispatchEvent(new KeyboardEvent("keydown", { key, shiftKey, bubbles: true })));
const esperar = () => act(() => new Promise<void>((r) => setTimeout(r, 0)));

function Tela({ fora, dentro }: { fora: () => void; dentro: () => void }) {
  const [a, setA] = useState(false);
  const [b, setB] = useState(false);
  return (
    <>
      <button type="button" id="atras" onClick={() => setA(true)}>
        atras
      </button>
      <Dialog open={a} title="A" onClose={() => (setA(false), fora())} footer={<button type="button" id="b-abre" onClick={() => setB(true)}>b</button>}>
        <input id="a1" />
      </Dialog>
      <Dialog open={b} title="B" onClose={() => (setB(false), dentro())}>
        <button type="button" id="b1">1</button>
      </Dialog>
    </>
  );
}

describe("Dialog", () => {
  it("foco dentro, Tab circula, Esc só no de cima, foco devolvido, pilha de abertos", async () => {
    const fora = vi.fn();
    const dentro = vi.fn();
    const div = document.createElement("div");
    document.body.appendChild(div);
    raiz = createRoot(div);
    await act(async () => raiz!.render(<Tela fora={fora} dentro={dentro} />));
    const atras = document.getElementById("atras") as HTMLButtonElement;
    atras.focus();
    await act(async () => atras.click());
    await esperar();
    expect(useDialogosAbertos.getState().pilha.length).toBe(1);
    // o primeiro campo (não o X) recebe o foco
    expect(document.activeElement?.id).toBe("a1");
    // Tab no último volta ao primeiro; Shift+Tab no primeiro vai ao último
    (document.getElementById("b-abre") as HTMLElement).focus();
    tecla("Tab");
    expect((document.activeElement as HTMLElement).getAttribute("aria-label")).toBe("Fechar");
    tecla("Tab", true);
    expect(document.activeElement?.id).toBe("b-abre");
    // foco fora do diálogo (botão de trás) volta para dentro no Tab
    atras.focus();
    tecla("Tab");
    expect(document.querySelector('[aria-label="A"]')!.contains(document.activeElement)).toBe(true);
    // segundo diálogo por cima: Esc fecha só ele, e o foco volta ao botão que o abriu
    const abre = document.getElementById("b-abre") as HTMLButtonElement;
    abre.focus();
    await act(async () => abre.click());
    await esperar();
    expect(useDialogosAbertos.getState().pilha.length).toBe(2);
    tecla("Escape");
    await esperar();
    expect([dentro.mock.calls.length, fora.mock.calls.length]).toEqual([1, 0]);
    expect(useDialogosAbertos.getState().pilha.length).toBe(1);
    expect(document.activeElement?.id).toBe("b-abre");
    tecla("Escape");
    await esperar();
    expect(fora).toHaveBeenCalledTimes(1);
    expect(useDialogosAbertos.getState().pilha).toEqual([]);
    expect(document.activeElement?.id).toBe("atras");
  });
});
