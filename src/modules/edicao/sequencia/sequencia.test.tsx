// @vitest-environment happy-dom
/** Sequência montada: "Imagem…" e "Adicionar → Narração" só oferecem o que a importação do Rust aceita. */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjetoAberto, ProjetoEdicao } from "@domain/edicao";

const cmd = vi.hoisted(() => ({ edicaoAbrir: vi.fn(), edicaoSalvar: vi.fn(), edicaoAnalisar: vi.fn(), edicaoMidiaInfo: vi.fn(), edicaoTabelaQuadros: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
const dialogo = vi.hoisted(() => ({ open: vi.fn(async (_o: { filters: { extensions: string[] }[] }) => null) }));
vi.mock("@tauri-apps/plugin-dialog", () => dialogo);

import { useEdicaoStore } from "../store/edicaoStore";
import { Sequencia } from "./Sequencia";
import { EXTENSOES } from "../inspetor/secoes/importar";
import { botao, clicar, desmontarTudo, esperar, lerFixture, montar } from "../inspetor/testeTela";

const st = () => useEdicaoStore.getState();

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  dialogo.open.mockClear();
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  for (const f of [cmd.edicaoMidiaInfo, cmd.edicaoTabelaQuadros]) f.mockImplementation(() => new Promise(() => {}));
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  cmd.edicaoAbrir.mockResolvedValue({ projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" } satisfies ProjetoAberto);
  await st().abrir("/casos/c1", projeto.id);
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

const extensoesPedidas = () => dialogo.open.mock.calls.at(-1)?.[0].filters[0]?.extensions;

describe("Sequência: importar", () => {
  it("Imagem… e Narração oferecem as extensões de armazem::EXT_IMAGEM e EXT_AUDIO", async () => {
    const t = await montar(<Sequencia />);
    await clicar(botao(t.raiz, "Imagem…"));
    await esperar();
    expect(extensoesPedidas()).toEqual(EXTENSOES.imagem);
    // presos: Adicionar → Narração no primeiro cartão
    await clicar(botao(t.raiz, "Adicionar"));
    await clicar(botao(t.raiz, "Narração"));
    await esperar();
    expect(extensoesPedidas()).toEqual(EXTENSOES.audio);
  });
});
