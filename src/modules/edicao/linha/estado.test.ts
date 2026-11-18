// @vitest-environment happy-dom
/** Estado de tela da linha: guardado por projeto, lixo vira padrão, altura automática pela janela. */

import { beforeEach, describe, expect, it } from "vitest";
import { alturaDoPainel, ALTURA_PAINEL_MIN, lerGuardado, PX_POR_S_MAX, PX_POR_S_PADRAO, UI_PADRAO, useLinhaUi } from "./estado";

/** `localStorage` em memória (o do node sem `--localstorage-file` é indefinido). */
function armazenamento(): Storage {
  const m = new Map<string, string>();
  return {
    get length() {
      return m.size;
    },
    clear: () => m.clear(),
    getItem: (k) => m.get(k) ?? null,
    key: (i) => [...m.keys()][i] ?? null,
    removeItem: (k) => void m.delete(k),
    setItem: (k, v) => void m.set(k, String(v)),
  };
}

beforeEach(() => {
  Object.defineProperty(globalThis, "localStorage", { value: armazenamento(), configurable: true, writable: true });
  useLinhaUi.setState({ ...UI_PADRAO, projeto: null, ajustarAoAbrir: false });
});

describe("estado da linha", () => {
  it("lixo no localStorage vira o padrão, campo a campo", () => {
    expect(lerGuardado({ pxPorS: "x", ferramenta: "martelo", ima: false, alturas: { a: 30, b: 9999, c: "1" }, travadas: { a: true, b: 1 }, altura: -5 })).toEqual({
      ...UI_PADRAO,
      ima: false,
      alturas: { a: 30 },
      travadas: { a: true },
      altura: ALTURA_PAINEL_MIN,
    });
    expect(lerGuardado({ pxPorS: 1e9 }).pxPorS).toBe(PX_POR_S_MAX);
    expect(lerGuardado(null).pxPorS).toBe(PX_POR_S_PADRAO);
  });

  it("altura automática pela janela; a escolhida até 70 % dela", () => {
    expect(alturaDoPainel(0, 700)).toBe(210);
    expect(alturaDoPainel(0, 2000)).toBe(320);
    expect(alturaDoPainel(0, 400)).toBe(170);
    expect(alturaDoPainel(500, 600)).toBe(420);
    expect(alturaDoPainel(260, 900)).toBe(260);
  });

  it("projeto novo abre ajustado; o guardado volta como estava", async () => {
    useLinhaUi.getState().carregar("p1");
    expect(useLinhaUi.getState()).toMatchObject({ projeto: "p1", ajustarAoAbrir: true, pxPorS: PX_POR_S_PADRAO });
    useLinhaUi.getState().definir({ pxPorS: 33, ferramenta: "lamina", travadas: { principal: true } });
    await new Promise((r) => setTimeout(r, 350));
    useLinhaUi.getState().carregar("p2");
    expect(useLinhaUi.getState().pxPorS).toBe(PX_POR_S_PADRAO);
    useLinhaUi.getState().carregar("p1");
    // a ferramenta não é guardada: reabre em Selecionar
    expect(useLinhaUi.getState()).toMatchObject({ projeto: "p1", ajustarAoAbrir: false, pxPorS: 33, ferramenta: "selecionar", travadas: { principal: true } });
  });
});
