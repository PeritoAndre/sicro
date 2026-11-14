// @vitest-environment happy-dom
/**
 * Central de Vídeo e Áudio com a Edição (§8.1, "uma lista só"): a lâmina "Edição" cria; o item abre, duplica e
 * exclui (confirmação do app); projeto ilegível mostra o ícone. Exclusão de vídeo mostra os usos na Edição.
 */

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjetoAberto, ProjetoEdicao, ResumoProjeto } from "@domain/edicao";
import type { VideoMedia } from "@domain/video";

const cmd = vi.hoisted(() => ({
  listVideoMedia: vi.fn(),
  listAudioMedia: vi.fn(),
  videoThumbnail: vi.fn(),
  deleteVideoMedia: vi.fn(),
  edicaoListar: vi.fn(),
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoDuplicar: vi.fn(),
  edicaoExcluir: vi.fn(),
  edicaoUsos: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));
const ask = vi.hoisted(() => ({ askConfirm: vi.fn(async (_o: { title: string; message?: string; confirmLabel?: string; danger?: boolean }) => true) }));
vi.mock("@components/Dialog/ask", () => ask);

import { MidiaCentral } from "./MidiaCentral";
import { useVideoStore } from "@modules/video/store/videoStore";
import { useEdicaoStore } from "@modules/edicao/store/edicaoStore";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { lerFixture } from "@modules/edicao/inspetor/testeTela";
import { confirmarExclusaoVideo, linhaDeUso } from "@modules/video/excluirVideo";
import { FRASE_DO_PROBLEMA } from "@modules/edicao/central";
import { open as abrirArquivo } from "@tauri-apps/plugin-dialog";
import { readFileSync } from "node:fs";
import { join } from "node:path";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const WS = "/casos/c1";
const resumo = (id: string, x: Partial<ResumoProjeto> = {}): ResumoProjeto => ({ id, titulo: id, updated_at: "2026-11-14T10:00:00Z", esquema: 2, duracao_s: 0, n_clipes: 0, miniatura_rel: null, problema: null, ...x });
const st = () => useEdicaoStore.getState();
let raiz: Root | null = null;
const fila = () => act(async () => new Promise<void>((r) => setTimeout(r, 0)));
const linha = (id: string) => document.querySelector<HTMLElement>(`[data-item="e-${id}"]`);
const clicar = (el: Element) => act(async () => (el as HTMLElement).click());

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  ask.askConfirm.mockClear();
  cmd.listVideoMedia.mockResolvedValue([]);
  cmd.listAudioMedia.mockResolvedValue([]);
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoListar.mockResolvedValue([resumo("p1", { titulo: "Júri 1", n_clipes: 5, duracao_s: 47.3 }), resumo("p2", { titulo: "velho", problema: "esquema", updated_at: "2026-11-13T10:00:00Z" })]);
  const projeto = { ...lerFixture<ProjetoEdicao>("projeto-do-store.json"), id: "p1" };
  cmd.edicaoAbrir.mockResolvedValue({ projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" } satisfies ProjetoAberto);
  cmd.edicaoDuplicar.mockResolvedValue(resumo("p3"));
  cmd.edicaoExcluir.mockResolvedValue(undefined);
  useWorkspaceStore.setState({ activeWorkspacePath: WS, activeOccurrence: { id: "o1" } as never });
  useVideoStore.setState({ edicao: false, list: [] });
  const div = document.createElement("div");
  document.body.appendChild(div);
  raiz = createRoot(div);
  await act(async () =>
    raiz!.render(
      <MemoryRouter initialEntries={["/video"]}>
        <MidiaCentral />
      </MemoryRouter>,
    ),
  );
  await fila();
});

afterEach(async () => {
  act(() => raiz?.unmount());
  raiz = null;
  document.body.innerHTML = "";
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

describe("Central: a Edição numa lista só", () => {
  it("lâmina Edição cria e leva à Edição; itens mostram clipes · duração e o ícone de problema", async () => {
    const lamina = document.querySelector<HTMLButtonElement>('[data-lamina="edicao"]')!;
    expect(lamina.getAttribute("aria-label")).toBe("Nova edição");
    expect(lamina.textContent).toBe("Edição");
    expect(linha("p1")!.textContent).toContain("5 clipes · 0:47");
    expect(linha("p2")!.querySelector('[data-problema="esquema"]')).not.toBeNull();
    expect(linha("p2")!.querySelector('[aria-label^="Duplicar"]')).toBeNull();
    await clicar(lamina);
    expect(st().aberta).not.toBeNull();
    expect(st().doc!.principal).toEqual([]);
    expect(useVideoStore.getState().edicao).toBe(true);
    // projeto novo só vai para o disco depois da primeira mudança
    expect(cmd.edicaoSalvar).not.toHaveBeenCalled();
  });

  it("item abre, duplica e exclui com confirmação; a lista é relida", async () => {
    const lidas = cmd.edicaoListar.mock.calls.length;
    await clicar(linha("p1")!.querySelector('[aria-label="Abrir Júri 1"]')!);
    expect(cmd.edicaoAbrir).toHaveBeenCalledWith(WS, "p1");
    expect(st().aberta).toEqual({ projetoId: "p1" });
    expect(useVideoStore.getState().edicao).toBe(true);
    // a lista fica montada atrás da Edição: ao voltar, relê
    await act(async () => useVideoStore.setState({ edicao: false }));
    await fila();
    expect(cmd.edicaoListar.mock.calls.length).toBe(lidas + 1);
    // duplicar o aberto grava o pendente antes
    st().alterar("Título", (d) => void (d.titulo = "Júri 1 b"));
    await clicar(linha("p1")!.querySelector('[aria-label="Duplicar Júri 1"]')!);
    await fila();
    expect(cmd.edicaoSalvar.mock.invocationCallOrder[0]!).toBeLessThan(cmd.edicaoDuplicar.mock.invocationCallOrder[0]!);
    expect(cmd.edicaoDuplicar).toHaveBeenCalledWith(WS, "p1");
    expect(cmd.edicaoListar.mock.calls.length).toBe(lidas + 2);
    // excluir: confirmação do app com o título; recusada não exclui
    ask.askConfirm.mockResolvedValueOnce(false);
    await clicar(linha("p1")!.querySelector('[aria-label="Excluir edição Júri 1"]')!);
    await fila();
    expect(ask.askConfirm).toHaveBeenLastCalledWith({ title: "Excluir edição", message: "Júri 1", confirmLabel: "Excluir edição", danger: true });
    expect(cmd.edicaoExcluir).not.toHaveBeenCalled();
    await clicar(linha("p1")!.querySelector('[aria-label="Excluir edição Júri 1"]')!);
    await fila();
    expect(cmd.edicaoExcluir).toHaveBeenCalledWith(WS, "p1");
    expect(st().aberta).toBeNull();
    expect(cmd.edicaoListar.mock.calls.length).toBe(lidas + 3);
  });

  it("excluir vídeo: o diálogo mostra as edições que o usam (edicao_usos)", async () => {
    cmd.edicaoUsos.mockResolvedValue([
      { projeto_id: "p1", titulo: "Júri 1", n_clipes: 3, saidas: 1 },
      { projeto_id: "p4", titulo: "Rascunho", n_clipes: 1, saidas: 0 },
    ]);
    ask.askConfirm.mockResolvedValueOnce(false);
    expect(await confirmarExclusaoVideo(WS, "a".repeat(64))).toBe(false);
    expect(cmd.edicaoUsos).toHaveBeenCalledWith(WS, "a".repeat(64));
    const msg = ask.askConfirm.mock.calls.at(-1)![0].message!;
    expect(msg.split("\n").slice(-2)).toEqual(["Júri 1 · 3 clipes · 1 exportação", "Rascunho · 1 clipe"]);
    expect(linhaDeUso({ projeto_id: "x", titulo: "T", n_clipes: 2, saidas: 4 })).toBe("T · 2 clipes · 4 exportações");
    // pela Central: o vídeo passa pelo mesmo diálogo antes de sair
    useVideoStore.setState({ list: [{ id: "v1", sha256: "b".repeat(64), filename: "cam.mp4", created_at: "2026-11-14T09:00:00Z" } as unknown as VideoMedia] });
    cmd.edicaoUsos.mockResolvedValue([]);
    cmd.videoThumbnail.mockRejectedValue(new Error("x"));
    await fila();
    ask.askConfirm.mockResolvedValueOnce(false);
    await clicar(document.querySelector('[data-item="v-v1"] [aria-label="Excluir vídeo cam.mp4"]')!);
    await fila();
    expect(cmd.edicaoUsos).toHaveBeenLastCalledWith(WS, "b".repeat(64));
    expect(cmd.deleteVideoMedia).not.toHaveBeenCalled();
  });

  it("atalho de adicionar vídeo (Ctrl+Shift+O) não dispara com a Edição na frente", async () => {
    const tecla = () => act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "O", code: "KeyO", ctrlKey: true, shiftKey: true, bubbles: true })));
    vi.mocked(abrirArquivo).mockClear();
    await act(async () => useVideoStore.setState({ edicao: true }));
    await tecla();
    expect(abrirArquivo).not.toHaveBeenCalled();
    await act(async () => useVideoStore.setState({ edicao: false }));
    await tecla();
    expect(abrirArquivo).toHaveBeenCalledTimes(1);
  });

  it("ícone do projeto ilegível: a frase é a do catálogo do Rust (mensagens.rs)", () => {
    const rs = readFileSync(join(process.cwd(), "src-tauri/src/edicao/mensagens.rs"), "utf8");
    for (const [codigo, frase] of Object.entries(FRASE_DO_PROBLEMA)) {
      const constante = new RegExp(`pub const ([A-Z_]+): &str = "${codigo}";`).exec(rs)?.[1];
      expect(constante, codigo).toBeTruthy();
      expect(rs).toContain(`(${constante}, Erro, "${frase}")`);
    }
  });
});
