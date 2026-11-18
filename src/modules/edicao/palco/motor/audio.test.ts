/** Som pelo próprio elemento (P3.3, rodada 2): volume do item × mestre, limitado a 0..1; sem Web Audio, o apito cala. */

import { describe, expect, it } from "vitest";
import { criarMistura } from "./audio";

describe("mistura pelo volume do elemento", () => {
  it("v × mestre, limitado a 0..1 (acima de 1 a prévia fica em 1); valor inválido cala", () => {
    const m = criarMistura();
    const el = { volume: 0.5 } as HTMLMediaElement;
    m.volume(el, 1.7);
    expect(el.volume).toBe(1);
    m.volume(el, 0.25);
    expect(el.volume).toBe(0.25);
    m.mestre(0.5);
    m.volume(el, 1.7);
    expect(el.volume).toBe(0.85);
    m.volume(el, -1);
    expect(el.volume).toBe(0);
    m.volume(el, Number.NaN);
    expect(el.volume).toBe(0);
    m.mestre(0);
    m.volume(el, 1);
    expect(el.volume).toBe(0);
  });

  it("forçar escreve mesmo igual (o fluxo novo herda o volume guardado); parado volta a 1", () => {
    const m = criarMistura();
    const escritos: number[] = [];
    let vol = 1;
    const el = { paused: false } as unknown as HTMLMediaElement;
    Object.defineProperty(el, "volume", { get: () => vol, set: (x: number) => void escritos.push((vol = x)) });
    m.volume(el, 1);
    expect(escritos).toEqual([]);
    m.volume(el, 1, true);
    expect(escritos).toEqual([0.999, 1]);
    m.volume(el, 0.3);
    m.repouso(el);
    expect(vol).toBe(0.3);
    Object.defineProperty(el, "paused", { get: () => true });
    m.repouso(el);
    expect(vol).toBe(1);
  });

  it("sem Web Audio: o apito não quebra e nada de contexto nasce calado", () => {
    const m = criarMistura();
    m.apito(0);
    expect(m.contexto).toBeNull();
    expect(() => m.apito(0.125)).not.toThrow();
    m.fechar();
  });
});
