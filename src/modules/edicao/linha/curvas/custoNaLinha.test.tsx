// @vitest-environment happy-dom
/**
 * Custo da linha com curvas (fecho do bloco 6.4): cada item com curva pedia a linha resolvida inteira a cada mudança
 * do documento (200 clipes = 200 `resolver`, desfazer a 62 ms no WebKitGTK). Uma mudança resolve a linha poucas vezes,
 * qualquer que seja o número de itens com chaves, e as chaves seguem na linha.
 */

import { act } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { MidiaInfo, ProjetoAberto, ProjetoEdicao } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoFundo: vi.fn(),
  edicaoTira: vi.fn(),
  edicaoOnda: vi.fn(),
}));
const conta = vi.hoisted(() => ({ resolver: 0, base: 0 }));
vi.mock("@core/commands", () => ({ commands: new Proxy(cmd, { get: (t, k) => (t as Record<string | symbol, unknown>)[k] ?? (() => new Promise(() => {})) }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));
vi.mock("./itens", async (original) => {
  const m = await original<typeof import("./itens")>();
  return { ...m, baseDoItem: (...a: Parameters<typeof m.baseDoItem>) => (conta.base++, m.baseDoItem(...a)) };
});
vi.mock("../../modelo/tempo", async (original) => {
  const m = await original<typeof import("../../modelo/tempo")>();
  return { ...m, resolver: (...a: Parameters<typeof m.resolver>) => (conta.resolver++, m.resolver(...a)) };
});

import { useEdicaoStore } from "../../store/edicaoStore";
import { desmontarTudo, montar } from "../../inspetor/testeTela";
import { LinhaDoTempo } from "../LinhaDoTempo";
import { useLinhaUi } from "../estado";
import { midiaSintetica, tabelaSintetica, trechoNosQuadros } from "../testeLinha";
import { novoProjeto } from "../../modelo/projeto";

const st = () => useEdicaoStore.getState();
const TAB = tabelaSintetica(1800);

function info(sha: string): MidiaInfo {
  return {
    id: "m1", sha256: sha, nome: "m1.mp4", exibido: { w: 1280, h: 720 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 },
    de_s: 0, ate_s: 60, n: 1800, tem_audio: true,
    previa: { relativo: "m1.mp4", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 60, largura: 1280, altura: 720, pronta: false },
    tabela: "decodificador", erros: null, marcavel: true, cor: { matriz: "bt709", faixa: "tv" },
  };
}

/** 200 trechos de 6 quadros; um em cada cinco com o volume em chaves. */
function duzentos(): ProjetoEdicao {
  const doc = novoProjeto("custo");
  doc.midias.push(midiaSintetica("m1", TAB));
  for (let i = 0; i < 200; i++) {
    const c = trechoNosQuadros("m1", TAB, (i * 8) % 1700, ((i * 8) % 1700) + 6);
    c.rotulo = `c${i}`;
    if (i % 5 === 0) c.audio.volume = [{ t: c.inicio_s, v: 1 }, { t: c.inicio_s + 0.1, v: 0.5 }];
    doc.principal.push(c);
  }
  return doc;
}

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString(), raiz_cache: "/cache/c1" }));
  cmd.edicaoTabelaQuadros.mockImplementation(async () => TAB);
  cmd.edicaoMidiaInfo.mockImplementation(async (_ws: string, sha: string) => info(sha));
  cmd.edicaoFundo.mockResolvedValue([]);
  cmd.edicaoTira.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoOnda.mockResolvedValue(null);
  const projeto = duzentos();
  cmd.edicaoAbrir.mockResolvedValue({ projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" } satisfies ProjetoAberto);
  await st().abrir("/casos/c1", projeto.id);
  useLinhaUi.setState({ projeto: projeto.id, pxPorS: 14, ferramenta: "selecionar", ima: true, travadas: {}, alturas: {}, rolagem_s: 0, altura: 300 });
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

it("200 clipes, 40 com chaves de volume: uma mudança do documento resolve a linha poucas vezes e só os itens com chaves fazem o mapa; as chaves seguem na linha", async () => {
  const t = await montar(<LinhaDoTempo palco={() => null} atalhos={false} />);
  await act(async () => {});
  const curvas = () => t.raiz.querySelectorAll('[data-trilha="som"] [data-curva]').length;
  expect(curvas()).toBe(40);
  for (const nome of ["um", "dois", "três"]) {
    [conta.resolver, conta.base] = [0, 0];
    await act(async () => st().alterar("Renomear", (d) => void ((d.principal[7] as { rotulo: string }).rotulo = nome)));
    expect(conta.resolver, nome).toBeLessThanOrEqual(3);
    // o mapa do trecho só nos itens com chaves: um nas chaves da trilha (ímã) e um no desenho, 2 × 40
    expect(conta.base, nome).toBeLessThanOrEqual(80);
  }
  conta.resolver = 0;
  await act(async () => st().desfazer());
  expect(conta.resolver).toBeLessThanOrEqual(3);
  expect(curvas()).toBe(40);
});
