// @vitest-environment happy-dom
/**
 * Editor inteiro montado (happy-dom): os atalhos da tela valem só com a Edição na frente e sem diálogo aberto
 * (Sequência, Inspetor, confirmações, exportação; contratos (h) 25).
 */

import { act, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjetoAberto, ProjetoEdicao } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoExcluir: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoFila: vi.fn(),
  edicaoFundo: vi.fn(),
  edicaoSaidas: vi.fn(),
  edicaoCapacidades: vi.fn(),
  edicaoSpriteTexto: vi.fn(),
  edicaoAtlasCarimbo: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));

import { useEdicaoStore } from "./store/edicaoStore";
import { useDialogosAbertos } from "@components/Dialog/Dialog";
import { EditorEdicao } from "./EditorEdicao";
import { desmontarTudo, lerFixture, montar } from "./inspetor/testeTela";

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
const n = () => st().doc!.principal.length;
const del = () => act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", code: "Delete", bubbles: true })));

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-14T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  for (const f of [cmd.edicaoTabelaQuadros, cmd.edicaoMidiaInfo, cmd.edicaoSpriteTexto, cmd.edicaoAtlasCarimbo, cmd.edicaoCapacidades]) f.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoFila.mockResolvedValue([]);
  cmd.edicaoFundo.mockResolvedValue([]);
  cmd.edicaoSaidas.mockResolvedValue([]);
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  cmd.edicaoAbrir.mockResolvedValue({ projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" } satisfies ProjetoAberto);
  await st().abrir(WS, projeto.id);
  useDialogosAbertos.setState({ pilha: [] });
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

const focar = (i: number) => {
  const c = st().doc!.principal[i] as { id: string };
  const it = { k: "clipe" as const, id: c.id };
  st().selecionar({ itens: [it], foco: it, efeito: null });
};

describe("editor: atalhos só na frente e sem diálogo", () => {
  it("Del remove o clipe em foco; com diálogo aberto ou com a Edição escondida, não", async () => {
    const t = await montar(<EditorEdicao ativo />);
    const n0 = n();
    await act(async () => focar(2));
    await act(async () => useDialogosAbertos.setState({ pilha: [7] }));
    await del();
    expect(n()).toBe(n0);
    await act(async () => useDialogosAbertos.setState({ pilha: [] }));
    await del();
    expect(n()).toBe(n0 - 1);
    t.desmontar();
    // escondida atrás da análise: nenhum atalho
    const t2 = await montar(<EditorEdicao ativo={false} />);
    await act(async () => focar(1));
    await del();
    expect(n()).toBe(n0 - 1);
    t2.desmontar();
  });

  it("escondida para de tocar o clipe sem fonte (toca pelo relógio, sem <video> para pausar)", async () => {
    let esconder: () => void = () => {};
    function Casca() {
      const [ativo, setAtivo] = useState(true);
      esconder = () => setAtivo(false);
      return <EditorEdicao ativo={ativo} />;
    }
    const t = await montar(<Casca />);
    // clipe 0 da fixture: título
    expect(st().doc!.principal[0]!.tipo).toBe("titulo");
    await act(async () => focar(0));
    const tocar = () => t.raiz.querySelector<HTMLButtonElement>('button[aria-label="Reproduzir"]')!;
    await act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key: " ", code: "Space", bubbles: true })));
    expect(tocar().getAttribute("aria-pressed")).toBe("true");
    await act(async () => esconder());
    expect(tocar().getAttribute("aria-pressed")).toBe("false");
    t.desmontar();
  });
});
