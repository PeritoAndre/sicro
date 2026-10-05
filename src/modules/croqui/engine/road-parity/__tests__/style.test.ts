import { describe, expect, it } from "vitest";
import { PARITY_TEMAS, resolveParityStyle } from "../style";
import { makeParityRoad } from "../factories";
import { marcacaoFromEixo, resolveParityEixo } from "../types";

describe("estilo das vias", () => {
  it("sem estilo cai na planta técnica", () => {
    expect(resolveParityStyle(null)).toEqual(PARITY_TEMAS.tecnico);
    expect(resolveParityStyle(undefined).tema).toBe("tecnico");
  });

  it("parcial completa com o tema e ignora tipo errado", () => {
    const st = resolveParityStyle({ tema: "pb", borda_px: 3, asfalto: 12 as unknown as string });
    expect(st.tema).toBe("pb");
    expect(st.borda_px).toBe(3);
    expect(st.asfalto).toBe(PARITY_TEMAS.pb.asfalto);
    expect(st.calcada).toBe("hachura");
  });
});

describe("eixo da via", () => {
  it("croqui antigo: marcacao vira eixo", () => {
    expect(resolveParityEixo(makeParityRoad(0, 0, 10, 0, { marcacao: "amarela" }))).toBe("amarela_dupla");
    expect(resolveParityEixo(makeParityRoad(0, 0, 10, 0, { marcacao: "branca" }))).toBe("branca_trac");
    expect(resolveParityEixo(makeParityRoad(0, 0, 10, 0, { mao_dupla: false }))).toBe("nenhuma");
  });

  it("eixo explícito manda, e marcacao acompanha", () => {
    const r = makeParityRoad(0, 0, 10, 0, { marcacao: "amarela", eixo: "branca_trac" });
    expect(resolveParityEixo(r)).toBe("branca_trac");
    expect(marcacaoFromEixo("amarela_mista")).toBe("amarela");
    expect(marcacaoFromEixo("nenhuma")).toBe("nenhuma");
  });

  it("campos opcionais só entram quando pedidos", () => {
    const plain = makeParityRoad(0, 0, 10, 0);
    expect("faixas" in plain).toBe(false);
    const full = makeParityRoad(0, 0, 10, 0, { faixas: 2, acostamento_m: 2.5, calcada_m: null });
    expect(full.faixas).toBe(2);
    expect(full.acostamento_m).toBe(2.5);
    expect(full.calcada_m).toBeNull();
  });
});
