// @vitest-environment happy-dom
/**
 * "Começar no cursor" e "Terminar no cursor" na linha do efeito aberto (contratos-fase4.md D6, (h)): a borda vai ao
 * quadro exibido no cursor (fonte no trecho; slot no título), a outra fica e nunca é cruzada; um passo cada.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeTitulo, ClipeTrecho, EfTexto, MidiaInfo, ProjetoAberto, ProjetoEdicao } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: new Proxy(cmd, { get: (t, k) => (t as Record<string | symbol, unknown>)[k] ?? (() => new Promise(() => {})) }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { useEdicaoStore } from "../store/edicaoStore";
import { Inspetor } from "../inspetor/Inspetor";
import { esquecerLeituras } from "../inspetor/contexto";
import { useInspetorUi } from "../inspetor/ui";
import { novoTexto } from "../inspetor/novos";
import { botao, clicar, desmontarTudo, esperar, lerFixture, montar } from "../inspetor/testeTela";
import { tabelaSintetica } from "./testeLinha";

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
const TAB = tabelaSintetica(1800);

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoTabelaQuadros.mockImplementation(async () => TAB);
  cmd.edicaoMidiaInfo.mockImplementation(async (_ws: string, sha: string) => ({ sha256: sha, marcavel: true, tabela: "decodificador", exibido: { w: 1280, h: 720 } }) as unknown as MidiaInfo);
  esquecerLeituras();
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  cmd.edicaoAbrir.mockResolvedValueOnce({ projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" } satisfies ProjetoAberto);
  await st().abrir(WS, projeto.id);
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

/** Põe um texto no clipe `i`, foca o clipe e abre o efeito. */
async function comTexto(i: number): Promise<{ clipe: string; efeito: string }> {
  const e = novoTexto();
  const clipe = (st().doc!.principal[i] as { id: string }).id;
  st().alterar("Texto", (d) => void (d.principal[i] as ClipeTrecho).efeitos.push(e));
  const it = { k: "clipe" as const, id: clipe };
  st().selecionar({ itens: [it], foco: it, efeito: e.id });
  return { clipe, efeito: e.id };
}
const ativo = (i: number) => ((st().doc!.principal[i] as ClipeTrecho).efeitos.at(-1) as EfTexto).ativo;

describe("Começar e Terminar no cursor", () => {
  it("trecho: o quadro exibido no cursor (tempo de fonte); a outra borda fica; nunca cruza; um passo cada", async () => {
    const t = await montar(<Inspetor />);
    const { clipe } = await comTexto(1);
    await esperar();
    // cursor no meio do quadro 390 (13 s)
    await esperar(() => st().posicionar({ modo: "clipe", clipe, t: 13.02 }));
    const n = st().passado.length;
    await clicar(botao(t.raiz, "Começar no cursor"));
    expect(ativo(1)).toEqual({ de: 13, ate: null });
    expect(st().passado.length).toBe(n + 1);
    expect(st().passado.at(-1)?.rotulo).toBe("Começar no cursor");
    await esperar(() => st().posicionar({ modo: "clipe", clipe, t: 14.51 }));
    await clicar(botao(t.raiz, "Terminar no cursor"));
    expect(ativo(1)).toEqual({ de: 13, ate: 14.5 });
    // cursor antes do começo: o fim fica um quadro depois dele
    await esperar(() => st().posicionar({ modo: "clipe", clipe, t: 12.2 }));
    await clicar(botao(t.raiz, "Terminar no cursor"));
    expect(ativo(1)).toEqual({ de: 13, ate: 13 + 1 / 30 });
    expect(st().passado.length).toBe(n + 3);
  });

  it("título (base local): o instante do slot", async () => {
    const t = await montar(<Inspetor />);
    const { clipe } = await comTexto(0);
    await esperar(() => st().posicionar({ modo: "clipe", clipe, t: 1.01 }));
    await clicar(botao(t.raiz, "Começar no cursor"));
    expect((st().doc!.principal[0] as ClipeTitulo).efeitos.at(-1)).toMatchObject({ ativo: { de: 1, ate: null } });
  });
});
