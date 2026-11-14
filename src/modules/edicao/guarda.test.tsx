// @vitest-environment happy-dom
/**
 * Guarda de saída (§8.1): com mudança pendente (relógio falso: o autosave de 3 s não chegou), trocar de módulo, trocar
 * de caso e fechar a janela gravam ANTES de sair; gravação que falha pergunta e "Descartar" sai sem gravar.
 */

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjetoAberto, ProjetoEdicao } from "@domain/edicao";

const log = vi.hoisted(() => [] as string[]);
const cmd = vi.hoisted(() => ({ edicaoAbrir: vi.fn(), edicaoSalvar: vi.fn(), edicaoAnalisar: vi.fn(), edicaoExcluir: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
const ask = vi.hoisted(() => ({ askConfirm: vi.fn(async (_o: { title: string; message?: string; confirmLabel?: string; danger?: boolean }) => false) }));
vi.mock("@components/Dialog/ask", () => ask);
const janela = vi.hoisted(() => ({
  fechar: null as null | ((ev: { preventDefault(): void }) => Promise<void>),
  solta: vi.fn(async () => void log.push("solta")),
}));
vi.mock("@tauri-apps/api/window", () => {
  const w = {
    isMaximized: async () => false,
    onResized: async () => () => {},
    onCloseRequested: async (h: (ev: { preventDefault(): void }) => Promise<void>) => {
      janela.fechar = h;
      return janela.solta;
    },
    close: vi.fn(async () => void log.push("close")),
    minimize: async () => {},
    toggleMaximize: async () => {},
    startResizeDragging: async () => {},
  };
  return { getCurrentWindow: () => w };
});
vi.mock("./EditorEdicao", () => ({ EditorEdicao: () => <div data-editor-falso="" /> }));
vi.mock("@app/Brand", () => ({ Brand: () => null }));

import { definirCorrente, useEdicaoStore } from "./store/edicaoStore";
import { EdicaoRaiz } from "./EdicaoRaiz";
import { useNavGuard } from "@app/navGuard";
import { TitleBar } from "@app/TitleBar";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { lerFixture } from "./inspetor/testeTela";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
let raizes: Root[] = [];
const montar = async (el: React.ReactElement) => {
  const div = document.createElement("div");
  document.body.appendChild(div);
  const r = createRoot(div);
  raizes.push(r);
  await act(async () => r.render(el));
};
/** Deixa as promessas andarem sem o relógio andar (o autosave de 3 s não dispara). */
const fila = () => act(async () => {
  for (let i = 0; i < 20; i++) await Promise.resolve();
});

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"], now: new Date("2026-11-14T10:00:00Z") });
  log.length = 0;
  for (const f of Object.values(cmd)) f.mockReset();
  ask.askConfirm.mockReset();
  ask.askConfirm.mockResolvedValue(false);
  janela.solta.mockClear();
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoSalvar.mockImplementation(async (_ws: string, p: ProjetoEdicao) => {
    log.push(`salvar:${p.titulo}`);
    return { updated_at: new Date().toISOString() };
  });
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  const r: ProjetoAberto = { projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" };
  cmd.edicaoAbrir.mockResolvedValue(r);
  useWorkspaceStore.setState({ activeWorkspacePath: WS, activeOccurrence: { id: "o1" } as never });
  // a Raiz abre a edição corrente (a fixture), sem gravar nada
  definirCorrente(WS, projeto.id);
  await act(async () => {
    await st().abrir(WS, projeto.id);
  });
});

afterEach(async () => {
  for (const r of raizes) act(() => r.unmount());
  raizes = [];
  document.body.innerHTML = "";
  useNavGuard.getState().unregister();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

describe("guarda de saída da Edição", () => {
  it("trocar de módulo (ou de caso) com pendência: grava antes de navegar", async () => {
    await montar(<EdicaoRaiz visivel />);
    expect(useNavGuard.getState().guard).not.toBeNull();
    st().alterar("Título", (d) => void (d.titulo = "pendente"));
    // 1 s depois: o autosave (3 s) ainda não gravou
    vi.advanceTimersByTime(1000);
    await fila();
    expect(cmd.edicaoSalvar).not.toHaveBeenCalled();
    let foi = false;
    await act(async () => {
      await useNavGuard.getState().attemptNavigation(() => {
        log.push("navegou");
        foi = true;
      });
    });
    expect(foi).toBe(true);
    expect(log).toEqual(["salvar:pendente", "navegou"]);
  });

  it("fechar a janela com pendência: grava, solta o ouvinte e só então fecha", async () => {
    await montar(<TitleBar />);
    await fila();
    expect(janela.fechar).not.toBeNull();
    st().alterar("Título", (d) => void (d.titulo = "antes de fechar"));
    vi.advanceTimersByTime(500);
    const ev = { preventDefault: vi.fn() };
    await act(async () => {
      await janela.fechar!(ev);
    });
    expect(ev.preventDefault).toHaveBeenCalled();
    expect(log).toEqual(["salvar:antes de fechar", "solta", "close"]);
  });

  it("gravação que falha: pergunta; Cancelar fica, Descartar sai sem gravar", async () => {
    await montar(<TitleBar />);
    await fila();
    cmd.edicaoSalvar.mockRejectedValue({ kind: "filesystem", message: "filesystem error: disco cheio" });
    st().alterar("Título", (d) => void (d.titulo = "não grava"));
    await act(async () => {
      await janela.fechar!({ preventDefault() {} });
    });
    expect(ask.askConfirm).toHaveBeenCalledWith({ title: "Não salvou", message: "disco cheio", confirmLabel: "Descartar", danger: true });
    expect(log).not.toContain("close");
    expect(st().aberta).not.toBeNull();
    ask.askConfirm.mockResolvedValueOnce(true);
    await act(async () => {
      await janela.fechar!({ preventDefault() {} });
    });
    expect(log.at(-1)).toBe("close");
    expect(st().aberta).toBeNull();
  });
});
