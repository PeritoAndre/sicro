import { describe, expect, it } from "vitest";
import type { Caixa, Chave, ClipeCongelar, ClipeTrecho, EfEstabilizar, ProjetoEdicao } from "@domain/edicao";
import { ESQUEMA_EDICAO, ESQUEMA_MENOR } from "@domain/edicao";
import {
  acharClipe,
  acharEfeito,
  animaveisVazios,
  aplicarMudanca,
  aplicarPrender,
  aplicarSaidaAuto,
  garantirMidia, mesmaProveniencia,
  moverClipe,
  novoProjeto,
  porChave,
  removerClipe,
  tirarChave,
  completar,
  mixagemPadrao,
} from "./projeto";
import { ler } from "./testeFixtures";

const ch = (t: number, v: number, pts: number | null = null): Chave<number> => ({ t, pts, v, interp: "linear" });

describe("projeto novo", () => {
  it("nasce com saída automática, normalização em −16 LUFS / −1,5 dBTP (§2.4) e os padrões do Rust (M7)", () => {
    const p = novoProjeto("X", "2026-11-14T04:00:00.000Z");
    // menor 0: o arquivo só sobe para ESQUEMA_MENOR quando usa campo da Fase 4 (armazem::salvar, contratos-fase4 B1)
    expect(ESQUEMA_MENOR).toBe(1);
    expect(p).toMatchObject({ esquema: ESQUEMA_EDICAO, esquema_menor: 0, titulo: "X", created_at: "2026-11-14T04:00:00.000Z", fecho: null });
    expect(p.id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(p.saida.auto).toBe(true);
    expect(p.mixagem).toEqual({ volume_original: 1, abaixar: { db: -12, ataque_s: 0.3, soltura_s: 0.5 }, normalizar: { lufs: -16, pico_db: -1.5 }, limitador: false });
    // cada projeto com o seu objeto; o que completa arquivo antigo sem `mixagem` segue neutro (o `Default` do Rust)
    expect(novoProjeto().mixagem.normalizar).not.toBe(p.mixagem.normalizar);
    expect(mixagemPadrao().normalizar).toBeNull();
    const { mixagem: _sem, ...semMixagem } = p;
    expect(completar(semMixagem as ProjetoEdicao).mixagem.normalizar).toBeNull();
    expect(p.legendas.lugar).toEqual({ x: 0.5, y: 0.94, h: "centro", v: "base" });
    expect(p.legendas.estilo.caixa).toEqual({ cor: "#0000008c", margem: 0.013, raio: 0 });
    expect(p.legendas.linhas_max).toBe(2);
    expect(novoProjeto().id).not.toBe(p.id);
  });
});

describe("chaves", () => {
  it("porChave põe em ordem e troca a do mesmo quadro ou instante", () => {
    let ks = [ch(1, 10, 100), ch(3, 30, 300)];
    ks = porChave(ks, ch(2, 20, 200));
    expect(ks.map((k) => k.v)).toEqual([10, 20, 30]);
    ks = porChave(ks, ch(2.0001, 21, 200));
    expect(ks.map((k) => k.v)).toEqual([10, 21, 30]);
    ks = porChave(ks, ch(0, 0));
    expect(ks.map((k) => k.v)).toEqual([0, 10, 21, 30]);
  });

  it("tirar a última chave vira valor fixo", () => {
    expect(tirarChave([ch(1, 10), ch(2, 20)], 0)).toEqual([ch(2, 20)]);
    expect(tirarChave([ch(1, 10)], 0)).toBe(10);
  });

  it("acusa Animavel com lista vazia", () => {
    const p = ler<ProjetoEdicao>("completo.json");
    expect(animaveisVazios(p)).toEqual([]);
    (p.principal[0] as ClipeTrecho).audio.volume = [];
    const t = p.sobreposicoes[0]!.clipes[0]!;
    if ("transformacao" in t) (t.transformacao as { x: unknown }).x = [];
    p.audios[0]!.clipes[0]!.volume = [];
    expect(animaveisVazios(p)).toEqual(["principal[0].audio.volume", "sobreposicoes[0].clipes[0].transformacao.x", "audios[0].clipes[0].volume"]);
  });
});

describe("busca e mudanças", () => {
  it("acha clipe e efeito em qualquer lugar (principal, célula, sobreposto, ocultação da mídia)", () => {
    const p = ler<ProjetoEdicao>("completo.json");
    const c = p.principal[1]!;
    expect(acharClipe(p, c.id as string)?.onde).toBe("principal");
    const s = p.sobreposicoes[1]!.clipes[0]!;
    expect(acharClipe(p, s.id as string)).toMatchObject({ onde: "sobreposicao", trilha: p.sobreposicoes[1]!.id });
    for (const [lista, dono] of [
      [(p.principal[0] as ClipeTrecho).efeitos, p.principal[0]!.id],
      [p.sobreposicoes[0]!.clipes.flatMap((x) => ("efeitos" in x ? (x.efeitos as { id: string }[]) : [])), null],
    ] as const) {
      const e = lista[0] as { id: string } | undefined;
      if (e) expect(acharEfeito(p, e.id)?.efeito.id).toBe(e.id);
      if (e && dono) expect(acharEfeito(p, e.id)?.clipe).toBe(dono);
    }
    const oc = p.midias.flatMap((m) => m.ocultacoes)[0];
    expect(acharEfeito(p, oc!.id)).toMatchObject({ clipe: null, midia: p.midias[0]!.id });
    expect(acharClipe(p, "nao-existe")).toBeNull();
  });

  it("aplicarMudanca: efeito, âncora e campos do clipe", () => {
    const p = ler<ProjetoEdicao>("completo.json");
    const e = (p.principal[0] as ClipeTrecho).efeitos[0]!;
    expect(aplicarMudanca(p, { efeito: { ...(e as EfEstabilizar), nome: "novo" } })).toBe(true);
    expect((p.principal[0] as ClipeTrecho).efeitos[0]).toMatchObject({ nome: "novo" });
    const r = p.rastreios[0]!;
    const n = r.ancoras.length;
    const ancora: Chave<Caixa> = { t: 999, pts: null, v: { x: 0, y: 0, w: 0.1, h: 0.1 }, interp: "linear" };
    expect(aplicarMudanca(p, { rastreio: r.id, ancora })).toBe(true);
    expect(r.ancoras).toHaveLength(n + 1);
    expect(aplicarMudanca(p, { clipe: p.principal[0]!.id as string, campos: { inicio_s: 4 } })).toBe(true);
    expect((p.principal[0] as ClipeTrecho).inicio_s).toBe(4);
    expect(aplicarMudanca(p, { clipe: "x", campos: {} })).toBe(false);
  });

  it("mover, remover e mídia já presente", () => {
    const p = ler<ProjetoEdicao>("completo.json");
    const ids = p.principal.map((c) => c.id);
    moverClipe(p, 0, 2);
    expect(p.principal.map((c) => c.id)).toEqual([ids[1], ids[2], ids[0], ...ids.slice(3)]);
    expect(removerClipe(p, ids[0] as string)).toBe(true);
    expect(p.principal).toHaveLength(ids.length - 1);
    const m = p.midias[0]!;
    expect(garantirMidia(p, { ...m, id: "outro" })).toBe(m.id);
    expect(p.midias[0]).toBe(m);
    expect(garantirMidia(p, { ...m, id: "outro", sha256: "b".repeat(64) })).toBe("outro");
  });

  it("garantirMidia: mídia do caso já no projeto recebe a proveniência nova; a própria do projeto, não", () => {
    const p = ler<ProjetoEdicao>("completo.json");
    const m = p.midias.find((x) => x.origem === "caso")!;
    const i = p.midias.indexOf(m);
    const nova = { recebido: { nativo: !m.recebido.nativo, obs: `${m.recebido.obs} (conferido)` }, relogio_desvio_s: (m.relogio_desvio_s ?? 0) + 2, gravador: "DVR X" };
    expect(mesmaProveniencia(m, nova)).toBe(false);
    expect(garantirMidia(p, { ...m, id: "outro", nome: "outro nome", ...nova })).toBe(m.id);
    // só os três campos mudam, num item novo (o antigo fica como estava)
    expect(p.midias[i]).toEqual({ ...m, ...nova });
    expect(p.midias[i]).not.toBe(m);
    expect(mesmaProveniencia(p.midias[i]!, nova)).toBe(true);
    // cada campo sozinho conta como mudança
    for (const campo of [{ recebido: { ...nova.recebido, nativo: null } }, { recebido: { ...nova.recebido, obs: "" } }, { relogio_desvio_s: null }, { gravador: null }]) {
      expect(mesmaProveniencia(nova, { ...nova, ...campo })).toBe(false);
    }
    const propria = p.midias.find((x) => x.origem === "projeto");
    if (!propria) throw new Error("fixture sem mídia própria");
    const j = p.midias.indexOf(propria);
    garantirMidia(p, { ...propria, id: "z", ...nova });
    expect(p.midias[j]).toBe(propria);
  });

  it("aplicarPrender grava (t, pts) onde o Rust mandou; nada muda se já está igual", () => {
    const p = ler<ProjetoEdicao>("completo.json");
    const t = p.principal[0] as ClipeTrecho;
    expect(aplicarPrender(p, { onde: { tipo: "fim", clipe: t.id }, pts: null, t: 9.5 })).toBe(true);
    expect([t.fim_s, t.fim_pts]).toEqual([9.5, null]);
    expect(aplicarPrender(p, { onde: { tipo: "fim", clipe: t.id }, pts: null, t: 9.5 })).toBe(false);
    const g = p.principal.find((c) => c.tipo === "congelar") as ClipeCongelar;
    expect(aplicarPrender(p, { onde: { tipo: "instante", clipe: g.id }, pts: 4242, t: 2 })).toBe(true);
    expect([g.instante_s, g.instante_pts]).toEqual([2, 4242]);
    // tipo de clipe errado para o lugar: ignora
    expect(aplicarPrender(p, { onde: { tipo: "instante", clipe: t.id }, pts: 1, t: 1 })).toBe(false);
    const r = p.rastreios[0]!;
    expect(aplicarPrender(p, { onde: { tipo: "ancora", rastreio: r.id, indice: 0 }, pts: 77, t: 0.5 })).toBe(true);
    expect([r.ancoras[0]!.t, r.ancoras[0]!.pts]).toEqual([0.5, 77]);
    // âncora sem a chave `pts` já vale nulo: nada muda (e o projeto não é regravado por isso)
    delete r.ancoras[1]!.pts;
    expect(aplicarPrender(p, { onde: { tipo: "ancora", rastreio: r.id, indice: 1 }, pts: null, t: r.ancoras[1]!.t })).toBe(false);
    expect("pts" in r.ancoras[1]!).toBe(false);
    const dono = p.principal[0] as ClipeTrecho;
    const est = dono.efeitos.find((e) => e.tipo === "estabilizar") as EfEstabilizar | undefined;
    expect(aplicarPrender(p, { onde: { tipo: "analise_ate", clipe: dono.id, efeito: est!.id }, pts: 5000, t: 7 })).toBe(true);
    expect([est!.analise.ate_s, est!.analise.ate_pts]).toEqual([7, 5000]);
  });

  it("saída automática só com saida.auto", () => {
    const p = ler<ProjetoEdicao>("completo.json");
    p.saida.auto = false;
    expect(aplicarSaidaAuto(p, [1280, 720, { num: 25, den: 1 }])).toBe(false);
    p.saida.auto = true;
    expect(aplicarSaidaAuto(p, [1280, 720, { num: 25, den: 1 }])).toBe(true);
    expect(aplicarSaidaAuto(p, [1280, 720, { num: 25, den: 1 }])).toBe(false);
    expect(aplicarSaidaAuto(p, null)).toBe(false);
  });
});
