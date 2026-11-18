// @vitest-environment happy-dom
/**
 * Medida da linha para o Diagnóstico: monta 200 clipes sem tocar na store (o projeto aberto não muda), roda as quatro
 * fases e devolve os tempos. O número que vale é o do app (WebKitGTK); aqui só a forma e a montagem.
 */

import { describe, expect, it, vi } from "vitest";

vi.mock("@core/commands", () => ({ commands: new Proxy({}, { get: () => () => new Promise(() => {}) }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { useEdicaoStore } from "../store/edicaoStore";
import { resolver } from "../modelo/tempo";
import { LIMITE_MS, medirLinha, N_CLIPES, projetoDe200 } from "./diagnostico";
import { LIMITE_LINHA_MS } from "../diagnostico/resumo";
import { trilhasDaLinha, TRILHA_PRINCIPAL } from "./modelo";

describe("medida da linha (Diagnóstico)", () => {
  it("a tabela do Diagnóstico usa o mesmo limite", () => {
    expect(LIMITE_LINHA_MS).toBe(LIMITE_MS);
  });

  it("projeto de 200 clipes com sobreposições, áudio e efeitos", () => {
    const { doc, ctx } = projetoDe200();
    const linha = resolver(doc, ctx);
    expect(linha.clipes).toHaveLength(N_CLIPES);
    const ts = trilhasDaLinha(doc, linha, ctx);
    expect(ts.find((t) => t.id === TRILHA_PRINCIPAL)!.itens).toHaveLength(N_CLIPES);
    expect(ts.filter((t) => t.tipo === "sobreposicao").flatMap((t) => t.itens)).toHaveLength(40);
    expect(ts.filter((t) => t.tipo === "efeito").flatMap((t) => t.itens).length).toBeGreaterThan(40);
  });

  it("quatro fases, um passo por quadro, sem mexer na store; a caixa sai no fim", async () => {
    const palco = document.createElement("canvas");
    const caixa = document.createElement("div");
    caixa.appendChild(palco);
    document.body.appendChild(caixa);
    const doc0 = useEdicaoStore.getState().doc;
    const amb = { agora: () => performance.now(), quadro: () => new Promise<number>((r) => setTimeout(() => r(performance.now()), 1)), palco };
    const r = await medirLinha(amb, { quadros: 4 });
    expect(r.clipes).toBe(N_CLIPES);
    expect(Object.keys(r.fases)).toEqual(["rolar", "zoom", "cabeca", "arrastar"]);
    for (const f of Object.values(r.fases)) {
      expect(f.quadros).toBe(4);
      expect(f.trabalho_p95).toBeGreaterThanOrEqual(0);
      expect(f.intervalo_p95).toBeGreaterThan(0);
    }
    expect(r.ok).toBe(Object.values(r.fases).every((f) => f.trabalho_p95 < LIMITE_MS));
    expect(r.assentar.quadros).toBe(40);
    expect(useEdicaoStore.getState().doc).toBe(doc0);
    expect(caixa.children).toHaveLength(1);
  }, 60_000);
});
