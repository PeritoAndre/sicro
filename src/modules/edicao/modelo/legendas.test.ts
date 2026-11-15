/** Espelho de `motor::legendas` × fixture `legendas-exibidas.json` (mesmos bits) e a degravação → legendas. */

import { describe, expect, it } from "vitest";
import type { Degravacao, LinhaResolvida, ProjetoEdicao } from "@domain/edicao";
import { casos, conferirCasos } from "./testeFixtures";
import { exibidas, legendasDaDegravacao, porLinha, quebrar, sobSilencio } from "./legendas";

type E = { projeto: ProjetoEdicao; linha: LinhaResolvida };

describe("legendas × fixture", () => {
  it("legendas-exibidas.json: exibidas, sob silêncio e caracteres por linha iguais aos do Rust", () => {
    const lista = casos<E, unknown>("legendas-exibidas.json");
    expect(lista.length).toBeGreaterThanOrEqual(9);
    const erros = conferirCasos(lista, (e) => ({ exibidas: exibidas(e.projeto, e.linha), sob_silencio: sobSilencio(e.projeto, e.linha), por_linha: porLinha(e.projeto) }));
    expect(erros).toEqual([]);
  });
});

describe("quebra", () => {
  it("espaço sem quebra fica na palavra; palavra maior que a linha é partida por caractere (não por unidade UTF-16)", () => {
    expect(quebrar("a b c", 3)).toEqual(["a b", "c"]);
    expect(quebrar("😀😀😀😀😀", 2)).toEqual(["😀😀", "😀😀", "😀"]);
    expect(quebrar("  \n\n x  ", 10)).toEqual(["x"]);
  });
});

describe("degravação → legendas", () => {
  const d: Degravacao = {
    origem: { audio_sha256: "a".repeat(64), importado_em: "2026-11-14T10:00:00Z", motor: "whisper-cli ggml-base.bin", modelo_sha256: null },
    trechos: [
      { de: 11.5, ate: 12.25, texto: "Para!", locutor: "Locutor 1", ia: true },
      { de: 12.5, ate: 13, texto: "Ok", locutor: null, ia: false },
      { de: 13, ate: 13.5, texto: " \n ", locutor: null, ia: true },
      { de: 14, ate: 15, texto: "sem locutor", locutor: null, ia: true },
    ],
  };

  it("tempo de fonte da mídia; IA chega não conferida; trecho sem texto não vira legenda", () => {
    const l = legendasDaDegravacao(d, "m1");
    expect(l.map(({ id: _, ...x }) => x)).toEqual([
      { texto: "Para!", locutor: "Locutor 1", origem: "ia", conferida: false, locutor_auto: true, ref: { tipo: "fonte", midia: "m1", de: 11.5, ate: 12.25 } },
      { texto: "Ok", locutor: null, origem: "perito", conferida: true, locutor_auto: false, ref: { tipo: "fonte", midia: "m1", de: 12.5, ate: 13 } },
      { texto: "sem locutor", locutor: null, origem: "ia", conferida: false, locutor_auto: false, ref: { tipo: "fonte", midia: "m1", de: 14, ate: 15 } },
    ]);
    expect(new Set(l.map((x) => x.id)).size).toBe(3);
  });
});
