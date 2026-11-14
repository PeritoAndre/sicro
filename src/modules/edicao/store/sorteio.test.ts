/**
 * Roteiros sorteados (semente fixa) sobre a store com gravação lenta e com falha: em nenhum deles pode haver
 * duas gravações ao mesmo tempo, documento que não é o atual indo para o disco, mudança que fica sem gravar
 * (B13 da v1) nem gravação depois de excluir.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Analise, ClipeTrecho, ProjetoAberto, ProjetoEdicao } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoExcluir: vi.fn(),
  listVideoMedia: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoQuadroEm: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { definirCorrente, useEdicaoStore } from "./edicaoStore";
import { receber } from "./coleta";
import { LIMITE_PASSOS } from "./historico";
import { ler } from "../modelo/testeFixtures";

const WS = "/casos/sorteio";
const st = () => useEdicaoStore.getState();
const sha = "c".repeat(64);

/** Gerador com semente (mulberry32). */
function sorteio(semente: number) {
  let a = semente >>> 0;
  return (n: number) => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return Math.floor((((t ^ (t >>> 14)) >>> 0) / 4294967296) * n);
  };
}

const analiseVazia = (): Analise => ({
  problemas: [],
  linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] },
  midias: [],
  prender: [],
  saida_auto: null,
});

/** O backend falso: disco por id, uma gravação que demora e às vezes falha. */
function backend(r: (n: number) => number) {
  const disco = new Map<string, ProjetoEdicao>();
  const b = {
    disco,
    emVoo: 0,
    maxEmVoo: 0,
    falhar: false,
    excluidos: new Set<string>(),
    /** Gravações começadas: o documento era o atual da store? (só para projeto aberto na tela) */
    velhas: [] as string[],
    depoisDeExcluir: [] as string[],
    gravacoes: 0,
  };
  cmd.edicaoSalvar.mockImplementation((_ws: string, d: ProjetoEdicao) => {
    b.gravacoes++;
    b.emVoo++;
    b.maxEmVoo = Math.max(b.maxEmVoo, b.emVoo);
    if (b.excluidos.has(d.id)) b.depoisDeExcluir.push(d.id);
    const s = st();
    if (s.aberta?.projetoId === d.id && s.doc !== d) b.velhas.push(d.titulo);
    const falha = b.falhar;
    const demora = [0, 0, 200, 2000, 5000][r(5)] as number;
    return new Promise((ok, erro) => {
      const fim = () => {
        b.emVoo--;
        if (falha) erro({ kind: "io", message: "i/o error: destino em uso" });
        else {
          disco.set(d.id, d);
          ok({ updated_at: "" });
        }
      };
      if (demora > 0) setTimeout(fim, demora);
      else fim();
    });
  });
  cmd.edicaoAbrir.mockImplementation(async (_ws: string, id: string): Promise<ProjetoAberto> => {
    const d = disco.get(id);
    if (!d) throw { kind: "filesystem", message: "filesystem error: não existe" };
    return { projeto: structuredClone(d), importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/s" };
  });
  cmd.edicaoExcluir.mockImplementation(async (_ws: string, id: string) => {
    disco.delete(id);
    b.excluidos.add(id);
  });
  return b;
}

/** Espera `p` andando o relógio falso (gravações lentas dependem dele). */
async function ate<T>(p: Promise<T>): Promise<{ ok: true; v: T } | { ok: false }> {
  let fim: { ok: true; v: T } | { ok: false } | null = null;
  p.then(
    (v) => (fim = { ok: true, v }),
    () => (fim = { ok: false }),
  );
  for (let i = 0; i < 400 && fim === null; i++) await vi.advanceTimersByTimeAsync(250);
  if (fim === null) throw new Error("não terminou em 100 s");
  return fim;
}

beforeEach(() => {
  vi.useFakeTimers({ now: new Date("2026-11-14T12:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoAnalisar.mockImplementation(async () => analiseVazia());
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente" });
  cmd.edicaoQuadroEm.mockImplementation(async (_w: string, _s: string, alvo: { t: number }) => ({ indice: 0, pts: Math.round(alvo.t * 1000), pts_s: alvo.t, dur_s: 0.04 }));
  cmd.listVideoMedia.mockResolvedValue([
    { id: "vm", sha256: sha, filename: "cam.mkv", relative_path: "Mídias/Vídeos/cam.mkv", duration_s: 100, width: 1280, height: 720, raw_probe_json: "{}", derived_from_hash: null, derivation_json: null },
  ]);
  definirCorrente(WS, null);
});

afterEach(async () => {
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: "" }));
  await st().fechar({ descartar: true }).catch(() => {});
  vi.useRealTimers();
});

describe("roteiros sorteados: salvamento", () => {
  it("mudar, desfazer, arrastar, gravar devagar, falhar, fechar, reabrir, excluir: o disco acaba igual à tela", async () => {
    const ROTEIROS = Number(process.env.SICRO_ROTEIROS ?? 120);
    const PASSOS = 60;
    for (let semente = 1; semente <= ROTEIROS; semente++) {
      const r = sorteio(semente);
      const b = backend(r);
      const base = ler<ProjetoEdicao>("completo.json");
      b.disco.set(base.id, base);
      let id = base.id;
      let n = 0;
      expect((await ate(st().abrir(WS, id))).ok, `semente ${semente}: abrir`).toBe(true);
      let gesto: ReturnType<ReturnType<typeof st>["iniciarGesto"]> | null = null;
      // último documento que a tela mostrou, e a revisão dele
      const ultimo = { doc: st().doc, rev: 0 };
      const parar = useEdicaoStore.subscribe((s) => {
        if (s.doc) [ultimo.doc, ultimo.rev] = [s.doc, s.revisao];
      });
      let gravacoesAoAbrir = cmd.edicaoSalvar.mock.calls.length;
      // o que só o perito muda neste roteiro (a análise só prende tempos)
      const campos = (d: ProjetoEdicao) => [d.titulo, d.saida.largura, d.saida.cor_fundo];
      let aoAbrir = campos(st().doc as ProjetoEdicao);
      const onde = (i: number) => `semente ${semente}, passo ${i}`;

      for (let i = 0; i < PASSOS; i++) {
        const dado = r(100);
        if (dado < 30) st().alterar("Nome", (d) => void (d.titulo = `t${++n}`));
        else if (dado < 38) st().alterar("Largura", (d) => void (d.saida.largura = 640 + 2 * ++n), { agrupar: "largura" });
        else if (dado < 44) st().desfazer();
        else if (dado < 48) st().refazer();
        else if (dado < 54) {
          gesto ??= st().iniciarGesto("Mover");
          gesto.alterar((d) => void (d.saida.cor_fundo = `#${(++n).toString(16).padStart(6, "0")}`));
        } else if (dado < 58) {
          if (r(2) === 0) gesto?.concluir();
          else gesto?.cancelar();
          gesto = null;
        } else if (dado < 62) b.falhar = !b.falhar;
        else if (dado < 66) void st().salvar().catch(() => {});
        else if (dado < 70) {
          // a análise prende um tempo no meio do que estiver acontecendo
          const c = st().doc?.principal.find((x): x is ClipeTrecho => x.tipo === "trecho");
          if (c) cmd.edicaoAnalisar.mockResolvedValueOnce({ ...analiseVazia(), prender: [{ onde: { tipo: "inicio", clipe: c.id }, pts: ++n, t: n / 1000 }] });
        } else if (dado < 76) {
          // fechar e reabrir: o que a tela tinha tem de estar no disco
          gesto = null;
          const f = await ate(st().fechar());
          if (f.ok) {
            expect(st().aberta, onde(i)).toBeNull();
            // `ultimo` = o que a tela tinha ao fechar (a análise pode ter prendido um tempo durante a espera)
            if (ultimo.rev > 0) expect(b.disco.get(id), `${onde(i)}: fechou sem gravar`).toBe(ultimo.doc);
            if (b.disco.has(id)) {
              // sem mudança na sessão, o disco fica como estava
              if (ultimo.rev === 0) expect(cmd.edicaoSalvar.mock.calls.length, onde(i)).toBe(gravacoesAoAbrir);
              expect((await ate(st().abrir(WS, id))).ok, onde(i)).toBe(true);
              gravacoesAoAbrir = cmd.edicaoSalvar.mock.calls.length;
              [ultimo.doc, ultimo.rev] = [st().doc, st().revisao];
              aoAbrir = campos(st().doc as ProjetoEdicao);
            } else {
              // projeto novo fechado sem mudança: nunca foi ao disco
              expect(ultimo.rev, onde(i)).toBe(0);
              const novo = await ate(st().novo(WS));
              if (novo.ok) id = novo.v;
              gravacoesAoAbrir = cmd.edicaoSalvar.mock.calls.length;
              [ultimo.doc, ultimo.rev] = [st().doc, st().revisao];
              aoAbrir = campos(st().doc as ProjetoEdicao);
            }
          } else {
            // gravação falhando: continua aberto, com tudo
            expect(st().aberta?.projetoId, onde(i)).toBe(id);
            expect(st().doc, onde(i)).toBe(ultimo.doc);
          }
        } else if (dado < 78) {
          // excluir: nada mais é gravado nesse id; segue num projeto novo
          gesto = null;
          const x = await ate(st().excluir(WS, id));
          expect(x.ok, onde(i)).toBe(true);
          expect(st().aberta, onde(i)).toBeNull();
          const novo = await ate(st().novo(WS));
          expect(novo.ok, onde(i)).toBe(true);
          if (novo.ok) id = novo.v;
          gravacoesAoAbrir = cmd.edicaoSalvar.mock.calls.length;
          [ultimo.doc, ultimo.rev] = [st().doc, st().revisao];
          aoAbrir = campos(st().doc as ProjetoEdicao);
        } else if (dado < 84) {
          // sossego no meio: sem falha e sem mudança, em 30 s tudo está no disco
          b.falhar = false;
          await vi.advanceTimersByTimeAsync(30_000);
          expect([st().salvando, st().revisaoSalva], `${onde(i)}: ficou sem gravar`).toEqual(["salvo", st().revisao]);
          if (st().revisao > 0) expect(b.disco.get(id), `${onde(i)}: o disco não tem o documento da tela`).toBe(st().doc);
        } else await vi.advanceTimersByTimeAsync([0, 100, 1000, 3000, 7000][r(5)] as number);
      }

      // sossego: o backend volta a gravar e o tempo passa
      gesto?.concluir();
      b.falhar = false;
      await vi.advanceTimersByTimeAsync(60_000);
      const s = st();
      expect(b.maxEmVoo, `semente ${semente}: gravações ao mesmo tempo`).toBeLessThanOrEqual(1);
      expect(b.velhas, `semente ${semente}: gravou documento que não era o atual`).toEqual([]);
      expect(b.depoisDeExcluir, `semente ${semente}: gravou depois de excluir`).toEqual([]);
      expect(s.aberta?.projetoId, `semente ${semente}`).toBe(id);
      expect([s.salvando, s.revisaoSalva], `semente ${semente}: ficou sem gravar`).toEqual(["salvo", s.revisao]);
      if (s.revisao > 0) expect(b.disco.get(id), `semente ${semente}: o disco não tem o documento da tela`).toBe(s.doc);
      expect(s.passado.length).toBeLessThanOrEqual(LIMITE_PASSOS);
      // histórico: desfazer tudo devolve o que o perito mudou ao que era ao abrir; refazer tudo devolve o final
      const final = s.doc as ProjetoEdicao;
      const passos = s.passado.length;
      for (let k = 0; k < passos; k++) st().desfazer();
      expect(st().passado, `semente ${semente}: desfazer tudo`).toHaveLength(0);
      expect(campos(st().doc as ProjetoEdicao), `semente ${semente}: desfazer tudo`).toEqual(aoAbrir);
      for (let k = 0; k < passos; k++) st().refazer();
      expect(st().doc, `semente ${semente}: refazer tudo`).toBe(final);
      parar();
      await ate(st().fechar());
    }
  }, 120_000);

  it("coleta com a tela aberta, fechada e abrindo: nenhum trecho coletado se perde", async () => {
    const ROTEIROS = Number(process.env.SICRO_ROTEIROS ?? 80);
    const PASSOS = 30;
    for (let semente = 1001; semente <= 1000 + ROTEIROS; semente++) {
      const r = sorteio(semente);
      const b = backend(r);
      const base = ler<ProjetoEdicao>("completo.json");
      const inicial = base.principal.length;
      b.disco.set(base.id, base);
      definirCorrente(WS, base.id);
      let coletados = 0;
      let n = 0;
      const pendentes: Promise<unknown>[] = [];
      for (let i = 0; i < PASSOS; i++) {
        const dado = r(100);
        if (dado < 30) {
          const t = ++n;
          pendentes.push(
            receber(WS, [{ sha256: sha, inicio_s: t, fim_s: t + 1 }], base.id).then(
              () => void coletados++,
              () => {},
            ),
          );
        } else if (dado < 45) pendentes.push(st().abrir(WS, base.id).catch(() => {}));
        else if (dado < 60) pendentes.push(st().fechar().catch(() => {}));
        else if (dado < 75) st().alterar("Nome", (d) => void (d.titulo = `t${++n}`));
        else if (dado < 80) b.falhar = !b.falhar;
        else await vi.advanceTimersByTimeAsync([0, 100, 1000, 4000][r(4)] as number);
      }
      b.falhar = false;
      await ate(Promise.all(pendentes));
      await vi.advanceTimersByTimeAsync(60_000);
      await ate(st().fechar());
      const disco = b.disco.get(base.id) as ProjetoEdicao;
      expect(b.maxEmVoo, `semente ${semente}: gravações ao mesmo tempo`).toBeLessThanOrEqual(1);
      expect(disco.principal.length - inicial, `semente ${semente}: trechos no disco × coletas que deram certo`).toBe(coletados);
      // cada coleta entrou uma vez só
      const inicios = disco.principal.slice(inicial).map((c) => (c as ClipeTrecho).inicio_s);
      expect(new Set(inicios).size, `semente ${semente}: trecho repetido`).toBe(inicios.length);
      expect(disco.midias.filter((m) => m.sha256 === sha), `semente ${semente}: mídia repetida`).toHaveLength(coletados > 0 ? 1 : 0);
    }
  }, 120_000);
});
