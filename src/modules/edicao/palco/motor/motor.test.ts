// @vitest-environment happy-dom
/**
 * Motor da prévia viva (P3.3) com elementos que buscam devagar, como no app: o modo guiado (ré, rampa, velocidade
 * fora de 0,25..8) pede um quadro por rAF; cada quadro que chega tem de ir ao canvas antes da busca seguinte.
 * Medido no app (WebKitGTK, revisão do P3.3): com a busca seguinte encadeada no `seeked`, a ré andava o relógio e a
 * imagem ficava parada no quadro de antes de tocar.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeTrecho, MidiaRef, PreviaMidia, ProjetoEdicao, TabelaQuadros } from "@domain/edicao";
import { novoProjeto, novoTrecho } from "../../modelo/projeto";
import { resolver, tempoDoQuadro, type ContextoTempo } from "../../modelo/tempo";
import { planoDe } from "./plano";
import { relogioMidia } from "./relogioMidia";
import { criarMotor, type FontesMotor } from "./motor";
import { planoDoDoc } from "./cursor";
import { useRecursos } from "../recursos";

const TAB: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(599).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem: "decodificador" };
const CTX: ContextoTempo = { tabela: () => TAB };
const PREVIA: PreviaMidia = { relativo: "m.mp4", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 20, largura: 640, altura: 360, pronta: true };
const BUSCA_MS = 30;

function projeto(): ProjetoEdicao {
  const p = novoProjeto();
  p.saida.fps = { num: 30, den: 1 };
  p.saida.audio = false;
  p.midias = [{ id: "m", sha256: "a".repeat(64), tipo: "video", nome: "m", relativo: "m.mp4", origem: "caso", vfr: false, tem_audio: false, ocultacoes: [] } as unknown as MidiaRef];
  p.principal = [{ ...novoTrecho("m", tempoDoQuadro(TAB, 0), tempoDoQuadro(TAB, 300)), id: "c" } as ClipeTrecho];
  return p;
}

/** Elemento do pool que busca em `BUSCA_MS` (seeking até o `seeked`) e fica parado. */
function lento(v: HTMLVideoElement) {
  let t = 0;
  let buscando = false;
  let h: ReturnType<typeof setTimeout> | null = null;
  Object.defineProperties(v, {
    readyState: { get: () => 4, configurable: true },
    seeking: { get: () => buscando, configurable: true },
    paused: { get: () => true, configurable: true },
    ended: { get: () => false, configurable: true },
    currentTime: {
      get: () => t,
      set: (x: number) => {
        t = x;
        buscando = true;
        if (h) clearTimeout(h);
        h = setTimeout(() => {
          buscando = false;
          v.dispatchEvent(new Event("seeked"));
        }, BUSCA_MS);
      },
      configurable: true,
    },
  });
  v.play = async () => {};
  v.pause = () => {};
}

/** Contexto 2D falso: guarda o tempo do elemento a cada `drawImage(video)`. */
function canvasFalso(desenhos: number[]) {
  const ctx = new Proxy(
    {},
    {
      get: (_, k) => {
        if (k === "drawImage") return (img: unknown) => void (img instanceof HTMLVideoElement && desenhos.push(img.currentTime));
        if (k === "createRadialGradient" || k === "createLinearGradient") return () => ({ addColorStop() {} });
        return () => {};
      },
      set: () => true,
    },
  );
  return { width: 640, height: 360, getContext: () => ctx } as unknown as HTMLCanvasElement;
}

beforeEach(() => {
  vi.stubGlobal("requestAnimationFrame", (f: FrameRequestCallback) => setTimeout(() => f(performance.now()), 16) as unknown as number);
  vi.stubGlobal("cancelAnimationFrame", (h: number) => clearTimeout(h));
});
afterEach(() => vi.unstubAllGlobals());

/** Elemento que toca (o tempo anda com o relógio de parede) e busca em `BUSCA_MS`. */
function tocavel(v: HTMLVideoElement) {
  let t = 0;
  let desde = 0;
  let tocando = false;
  let buscando = false;
  let h: ReturnType<typeof setTimeout> | null = null;
  const agora = () => (tocando ? t + ((performance.now() - desde) / 1000) * (v.playbackRate || 1) : t);
  Object.defineProperties(v, {
    readyState: { get: () => 4, configurable: true },
    seeking: { get: () => buscando, configurable: true },
    paused: { get: () => !tocando, configurable: true },
    ended: { get: () => false, configurable: true },
    currentTime: {
      get: agora,
      set: (x: number) => {
        t = x;
        desde = performance.now();
        buscando = true;
        if (h) clearTimeout(h);
        h = setTimeout(() => {
          buscando = false;
          v.dispatchEvent(new Event("seeked"));
        }, BUSCA_MS);
      },
      configurable: true,
    },
  });
  v.play = async () => {
    if (!tocando) {
      t = agora();
      desde = performance.now();
      tocando = true;
    }
  };
  v.pause = () => {
    t = agora();
    tocando = false;
  };
}

const esperar = async (f: () => boolean, ms: number) => {
  const t0 = performance.now();
  while (!f() && performance.now() - t0 < ms) await new Promise((r) => setTimeout(r, 10));
  return f();
};

describe("motor da prévia viva", () => {
  it("planoDoDoc: o mesmo plano enquanto o documento e as tabelas não mudam (a Sequência pede a cada cursor)", () => {
    const doc = projeto();
    const p1 = planoDoDoc(doc);
    expect(planoDoDoc(doc)).toBe(p1);
    // tabela chegou: plano novo, agora com os quadros
    useRecursos.setState((s) => ({ midias: { ...s.midias, ["a".repeat(64)]: { info: null, tabela: TAB, erro: null, pedindo: false } }, versao: s.versao + 1 }));
    const p2 = planoDoDoc(doc);
    expect(p2).not.toBe(p1);
    expect(p1.segmentos[0]!.quadros).toBeNull();
    expect(p2.segmentos[0]!.quadros?.length).toBe(300);
    // documento editado: plano novo
    const doc2 = { ...doc, principal: [{ ...(doc.principal[0] as ClipeTrecho), fim_s: tempoDoQuadro(TAB, 150) }] };
    expect(planoDoDoc(doc2).total_q).toBe(150);
  });

  it("repetir o programa: o elemento que já tocou volta ao quadro de entrada (o relógio não pula o clipe)", async () => {
    const doc = projeto();
    doc.midias.push({ ...doc.midias[0]!, id: "n", sha256: "b".repeat(64), relativo: "n.mp4" } as MidiaRef);
    // a: m 0..60; b: n 0..60
    doc.principal = [
      { ...novoTrecho("m", tempoDoQuadro(TAB, 0), tempoDoQuadro(TAB, 60)), id: "a" } as ClipeTrecho,
      { ...novoTrecho("n", tempoDoQuadro(TAB, 0), tempoDoQuadro(TAB, 60)), id: "b" } as ClipeTrecho,
    ];
    const plano = planoDe(doc, resolver(doc, CTX), CTX);
    const rel = relogioMidia(PREVIA, TAB);
    const fontes: FontesMotor = {
      midia: (s) => ({ src: `file:///${s.fonte!.midia}.mp4`, relogio: rel }),
      som: () => null,
      pintor: { imagem: () => null, cena: { sprite: () => null, atlas: () => null, midia: () => null }, origem: () => ({ w: 640, h: 360 }), livres: () => [], globais: () => [] },
    };
    const motor = criarMotor({ fontes, mistura: null });
    const host = document.createElement("div");
    motor.ligar(canvasFalso([]), host);
    for (const v of host.querySelectorAll("video")) tocavel(v);
    motor.plano(plano, doc);
    motor.irPara(0);
    expect(await esperar(() => motor.exato(), 2000)).toBe(true);
    motor.tocar(1);
    // até o meio do clipe b
    expect(await esperar(() => motor.relogio.q() >= 75, 5000)).toBe(true);
    motor.pausar();
    motor.irPara(0);
    expect(await esperar(() => motor.exato(), 2000)).toBe(true);
    const doA = [...host.querySelectorAll("video")].find((v) => v.src.endsWith("/m.mp4"))!;
    // parado no slot 0, o elemento do clipe a está no meio do quadro 0 (e não onde parou de tocar, ~2 s)
    expect(doA.currentTime).toBeCloseTo(rel.alvo(0), 6);
    motor.tocar(1);
    await new Promise((r) => setTimeout(r, 300));
    const q = motor.relogio.q();
    motor.pausar();
    motor.desligar();
    expect(q).toBeLessThan(30);
  });


  it("ré (guiado): cada quadro buscado vai ao canvas, e a imagem anda com o relógio", async () => {
    const doc = projeto();
    const plano = planoDe(doc, resolver(doc, CTX), CTX);
    const rel = relogioMidia(PREVIA, TAB);
    const fontes: FontesMotor = {
      midia: () => ({ src: "file:///m.mp4", relogio: rel }),
      som: () => null,
      pintor: { imagem: () => null, cena: { sprite: () => null, atlas: () => null, midia: () => null }, origem: () => ({ w: 640, h: 360 }), livres: () => [], globais: () => [] },
    };
    const desenhos: number[] = [];
    const motor = criarMotor({ fontes, mistura: null });
    const host = document.createElement("div");
    motor.ligar(canvasFalso(desenhos), host);
    for (const v of host.querySelectorAll("video")) lento(v);
    motor.plano(plano, doc);
    motor.irPara(200);
    const t0 = performance.now();
    while (!motor.exato() && performance.now() - t0 < 2000) await new Promise((r) => setTimeout(r, 10));
    expect(motor.exato()).toBe(true);
    desenhos.length = 0;
    motor.tocar(-1);
    expect(motor.relogio.estado()).toBe("guiado");
    await new Promise((r) => setTimeout(r, 1000));
    const q = motor.relogio.q();
    motor.pausar();
    motor.desligar();
    // o relógio andou ~30 quadros para trás; a imagem acompanhou (quadros distintos, decrescentes, perto do relógio)
    expect(q).toBeLessThan(185);
    const quadros = [...new Set(desenhos.map((t) => Math.floor(t * 30)))];
    expect(quadros.length).toBeGreaterThanOrEqual(8);
    expect(quadros.every((j, i) => i === 0 || j < quadros[i - 1]!)).toBe(true);
    expect(Math.min(...quadros)).toBeLessThan(q + 8);
  });
});
