/**
 * Abertura da Fase 4 (contratos-fase4.md): os projetos gravados antes abrem iguais na tela (nenhuma chave nova
 * inventada pelo `completar`), o documento novo nasce no `esquema_menor` 0, e a mudança de alça do texto livre
 * (`{ livre, trilha, sobreposto }`) troca o sobreposto inteiro na trilha dele.
 */

import { describe, expect, it } from "vitest";
import { produce } from "immer";
import type { ClipeSobreposto, ProjetoEdicao, SobTitulo } from "@domain/edicao";
import { aplicarMudanca, completar, novoProjeto } from "./projeto";
import { ler } from "./testeFixtures";

/** Caminhos das chaves que só a Fase 4 grava (`faixa` só quando verdadeira: também é a faixa de cor da mídia). */
function chavesDaFase4(v: unknown, onde = "$", saida: string[] = []): string[] {
  if (Array.isArray(v)) v.forEach((x, i) => chavesDaFase4(x, `${onde}[${i}]`, saida));
  else if (v && typeof v === "object") {
    for (const [k, x] of Object.entries(v)) {
      if (["sombra", "animacao", "segue"].includes(k) || (k === "faixa" && x === true)) saida.push(`${onde}.${k}`);
      chavesDaFase4(x, `${onde}.${k}`, saida);
    }
  }
  return saida;
}

describe("projetos de antes da Fase 4 na tela", () => {
  const antigos = ["completo.json", "contrato-todas-variantes.json", "projeto-do-store.json", "projeto-minimo.json", "verif-fase0-do-store.json", "verif-fase1-do-store.json"];
  for (const nome of antigos) {
    it(`${nome}: \`completar\` não inventa campo da Fase 4`, () => {
      const p = ler<ProjetoEdicao>(nome);
      const c = completar(p);
      expect(chavesDaFase4(c)).toEqual([]);
      expect(c.esquema_menor ?? 0).toBe(0);
    });
  }

  it("contrato-fase4.json: completo como chegou (os campos novos são opcionais no TS e o Rust não os repõe)", () => {
    const p = ler<ProjetoEdicao>("contrato-fase4.json");
    expect(completar(p)).toBe(p);
    expect(chavesDaFase4(p)).toHaveLength(13);
  });

  it("projeto novo nasce sem campo da Fase 4 e no esquema_menor 0", () => {
    const p = novoProjeto("X", "2026-11-17T00:00:00.000Z");
    expect(p.esquema_menor).toBe(0);
    expect(chavesDaFase4(p)).toEqual([]);
  });
});

describe("mudança de alça do sobreposto (B6)", () => {
  it("troca o sobreposto inteiro na trilha dele; trilha ou id errado não muda nada", () => {
    const p = ler<ProjetoEdicao>("contrato-fase4.json");
    const t = p.sobreposicoes[0]!;
    const i = t.clipes.findIndex((c) => (c as ClipeSobreposto).tipo === "titulo");
    const velho = t.clipes[i] as SobTitulo;
    const novo: SobTitulo = { ...velho, transformacao: { ...velho.transformacao, x: 0.1, rotacao: 12 } };
    let ok = false;
    const d = produce(p, (dd) => {
      ok = aplicarMudanca(dd, { livre: velho.id, trilha: t.id, sobreposto: novo });
    });
    expect(ok).toBe(true);
    expect(d.sobreposicoes[0]!.clipes[i]).toEqual(novo);
    for (const m of [{ livre: velho.id, trilha: "outra", sobreposto: novo }, { livre: "nao-existe", trilha: t.id, sobreposto: novo }]) {
      let r = true;
      const e = produce(p, (dd) => {
        r = aplicarMudanca(dd, m);
      });
      expect(r).toBe(false);
      expect(e).toBe(p);
    }
  });
});
