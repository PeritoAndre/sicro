/**
 * Relógio da prévia viva (P3.3) com relógio falso: parede sem mestre, mestre que manda (e atrasado não volta o
 * relógio), espera com o mestre buscando ou parado há 250 ms, guiado fora de 0,25..8 e na rampa, ré, fim do
 * programa e o cursor para o store a cada 250 ms.
 */

import { describe, expect, it } from "vitest";
import { PARADO_MS, PUBLICAR_MS, criarRelogio, type EstadoRelogio, type Mestre } from "./relogio";

function montar(total_q = 300, fps = { num: 30, den: 1 }) {
  let t = 1000;
  const posts: number[] = [];
  const avisos: [number, EstadoRelogio][] = [];
  const r = criarRelogio({ fps, total_q, agora: () => t, publicar: (q) => posts.push(q) });
  r.assinar((q, e) => avisos.push([q, e]));
  const andar = (ms: number, passo = 16) => {
    for (let x = 0; x < ms; x += passo) {
      t += Math.min(passo, ms - x);
      r.tique();
    }
  };
  return { r, posts, avisos, andar, agora: () => t };
}

/** Mestre falso: tempo de fonte que o teste anda. */
function mestre(t0: number) {
  const m = { t: t0, pronto: true };
  const api: Mestre = { tempo: () => m.t, pronto: () => m.pronto };
  return { m, api };
}

describe("relógio", () => {
  it("parado não anda; sem mestre anda pelo relógio de parede", () => {
    const { r, andar } = montar();
    andar(500);
    expect(r.q()).toBe(0);
    expect(r.estado()).toBe("parado");
    r.tocar();
    expect(r.estado()).toBe("tocando");
    andar(1000);
    expect(r.q()).toBe(30);
    r.pausar();
    expect(r.estado()).toBe("parado");
    andar(1000);
    expect(r.q()).toBe(30);
  });

  it("publica o cursor ao pausar, ao buscar e a cada 250 ms tocando", () => {
    const { r, posts, andar } = montar();
    r.irPara(12);
    expect(posts).toEqual([12]);
    r.tocar();
    andar(1000, 10);
    // 4 publicações em 1 s (a cada 250 ms), crescentes
    const tocando = posts.slice(1);
    expect(tocando.length).toBe(Math.floor(1000 / PUBLICAR_MS));
    expect([...tocando].sort((a, b) => a - b)).toEqual(tocando);
    r.pausar();
    expect(posts.at(-1)).toBe(r.q());
  });

  it("com mestre o slot vem do tempo do elemento: T = de_q + (tempo − t0)/v·fps; atrasado não volta", () => {
    const { r, andar } = montar();
    const { m, api } = mestre(10);
    r.irPara(60);
    r.mestre(api, { de_q: 60, t0: 10, velocidade: 0.5 });
    r.tocar();
    // o elemento anda a 0,5× do tempo: 0,1 s de fonte = 0,2 s de programa = 6 slots
    m.t = 10.1;
    andar(16);
    expect(r.q()).toBe(66);
    // o elemento volta um pouco (busca de seguidor, partida atrasada): o relógio fica
    m.t = 10.08;
    andar(16);
    expect(r.q()).toBe(66);
    m.t = 10.2;
    andar(16);
    expect(r.q()).toBe(72);
  });

  it("espera com o mestre buscando ou parado há 250 ms; volta quando ele anda", () => {
    const { r, andar, avisos } = montar();
    const { m, api } = mestre(0);
    r.mestre(api, { de_q: 0, t0: 0, velocidade: 1 });
    r.tocar();
    m.t = 0.5;
    andar(16);
    expect(r.estado()).toBe("tocando");
    // parado por menos de 250 ms: ainda tocando
    andar(PARADO_MS - 20);
    expect(r.estado()).toBe("tocando");
    andar(40);
    expect(r.estado()).toBe("esperando");
    const q = r.q();
    andar(500);
    expect(r.q()).toBe(q);
    m.t = 0.7;
    andar(16);
    expect(r.estado()).toBe("tocando");
    expect(r.q()).toBe(21);
    m.pronto = false;
    andar(16);
    expect(r.estado()).toBe("esperando");
    expect(avisos.map((a) => a[1])).toContain("esperando");
  });

  it("velocidade fora de 0,25..8 e segmento guiado: relógio de parede no estado guiado; ré para no começo", () => {
    const { r, andar } = montar();
    r.irPara(100);
    r.tocar(0.2);
    expect(r.estado()).toBe("guiado");
    andar(1000);
    expect(r.q()).toBe(106);
    r.pausar();
    r.tocar(1);
    r.guiar(true);
    expect(r.estado()).toBe("guiado");
    andar(500);
    expect(r.q()).toBe(121);
    r.guiar(false);
    expect(r.estado()).toBe("tocando");
    r.pausar();
    r.tocar(-1);
    expect(r.estado()).toBe("guiado");
    andar(1000);
    expect(r.q()).toBe(91);
    andar(10000);
    expect(r.q()).toBe(0);
    expect(r.estado()).toBe("parado");
  });

  it("para no último slot; tocar de novo recomeça do começo", () => {
    const { r, andar, posts } = montar(45);
    r.tocar();
    andar(3000);
    expect(r.estado()).toBe("parado");
    expect(r.q()).toBe(44);
    expect(posts.at(-1)).toBe(44);
    r.tocar();
    expect(r.q()).toBe(0);
    expect(r.estado()).toBe("tocando");
  });

  it("programa vazio não toca; o total novo limita o cursor", () => {
    const { r } = montar(0);
    r.tocar();
    expect(r.estado()).toBe("parado");
    const x = montar(100);
    x.r.irPara(90);
    x.r.total(50);
    expect(x.r.q()).toBe(49);
    x.r.irPara(500);
    expect(x.r.q()).toBe(49);
  });

  it("29,97 qps: 1001 ms de parede são 30 slots", () => {
    const { r, andar } = montar(1000, { num: 30000, den: 1001 });
    r.tocar();
    andar(1001, 7);
    expect(r.q()).toBe(30);
  });
});
