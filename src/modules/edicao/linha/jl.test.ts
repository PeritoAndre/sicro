/** J e L como no rótulo do atalho: repetir acelera (1×, 2×, 4×, 8×); K para; trocar de sentido volta a 1×. */

import { describe, expect, it } from "vitest";
import { teclaJL, velocidadeJL, type EstadoJL } from "./LinhaDoTempo";

const teclas = (s: string) => {
  let e: EstadoJL | null = null;
  const r: string[] = [];
  for (const k of s) {
    if (k === "K") e = null;
    else {
      e = velocidadeJL(e, k === "L" ? 1 : -1);
      r.push(`${e.sentido > 0 ? "" : "-"}${e.velocidade}`);
    }
  }
  return r.join(" ");
};

describe("J e L", () => {
  it("repetir acelera até 8×; K e a troca de sentido voltam a 1×", () => {
    expect(teclas("LLLLL")).toBe("1 2 4 8 8");
    expect(teclas("LLJJ")).toBe("1 2 -1 -2");
    expect(teclas("LLKL")).toBe("1 2 1");
  });

  it("o editor parte do que o palco faz agora: L L 2×, J J J ré a 4×, K para; parado sozinho, recomeça em 1×", () => {
    let v = 0;
    const palco = { velocidade: () => v, tocar: (d: 1 | -1, vel = 1) => void (v = d * vel) };
    const k = (s: string) => {
      for (const c of s) if (c === "K") v = 0;
      else teclaJL(palco, c === "L" ? 1 : -1);
    };
    k("LL");
    expect(v).toBe(2);
    k("JJJ");
    expect(v).toBe(-4);
    k("K");
    expect(v).toBe(0);
    k("L");
    expect(v).toBe(1);
    k("LLLL");
    expect(v).toBe(8);
    v = 0;
    k("L");
    expect(v).toBe(1);
  });
});
