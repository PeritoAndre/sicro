/** J e L como no rótulo do atalho: repetir acelera (1×, 2×, 4×, 8×); K para; trocar de sentido volta a 1×. */

import { describe, expect, it } from "vitest";
import { velocidadeJL, type EstadoJL } from "./LinhaDoTempo";

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

  // o palco ainda toca só a 1×: `PalcoApi.tocar(sentido)` sem velocidade; a linha já manda `tocar(sentido, velocidade)`
  it.skip("fecho do bloco 6.4: P3.3", () => {
    // com `Relogio.velocidade` (P3.3): L L → o palco toca a 2×; J J J → ré a 4×; K → parado
  });
});
