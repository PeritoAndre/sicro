// @vitest-environment happy-dom
/**
 * Análise escondida atrás da Edição (§8.1): montada, mas sem atalhos. A tela real (sem vídeo carregado os atalhos
 * já estão ligados): Alt+1..5 e os atalhos do reprodutor só valem com `visivel`.
 */

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@core/commands", () => ({ commands: new Proxy({}, { get: () => vi.fn(async () => []) }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));

import { open as abrirArquivo } from "@tauri-apps/plugin-dialog";
import { VideoAnalysisView } from "./VideoAnalysisView";
import { useMedirStore } from "./medir/medirStore";
import { useVideoStore } from "../store/videoStore";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { useDialogosAbertos } from "@components/Dialog/Dialog";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let raiz: Root | null = null;
const montar = async (visivel: boolean) => {
  const div = document.createElement("div");
  document.body.appendChild(div);
  raiz = createRoot(div);
  await act(async () => raiz!.render(<VideoAnalysisView visivel={visivel} />));
};
const tecla = (code: string, mods: Partial<KeyboardEventInit>) => act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { code, bubbles: true, ...mods })));

beforeEach(() => {
  useWorkspaceStore.setState({ activeWorkspacePath: "/casos/c1", activeOccurrence: { id: "o1", peritos: [] } as never });
  useVideoStore.setState({ activeMediaId: "v1", bundle: null, edicao: false, list: [] });
  useMedirStore.getState().setModo("assistir");
  useDialogosAbertos.setState({ pilha: [] });
  vi.mocked(abrirArquivo).mockClear();
});
afterEach(() => {
  act(() => raiz?.unmount());
  raiz = null;
  document.body.innerHTML = "";
});

describe("VideoAnalysisView: atalhos só com a análise na frente", () => {
  it("visível: Alt+2 troca o modo, Alt+6 abre a Edição e Ctrl+Shift+O pede o arquivo", async () => {
    await montar(true);
    await tecla("Digit2", { altKey: true });
    expect(useMedirStore.getState().modo).toBe("velocidade");
    await tecla("Digit1", { altKey: true });
    await tecla("KeyO", { ctrlKey: true, shiftKey: true });
    expect(abrirArquivo).toHaveBeenCalledTimes(1);
    await tecla("Digit6", { altKey: true });
    expect(useVideoStore.getState().edicao).toBe(true);
  });

  it("escondida: nem os modos nem os atalhos do reprodutor disparam", async () => {
    await montar(false);
    await tecla("Digit2", { altKey: true });
    expect(useMedirStore.getState().modo).toBe("assistir");
    await tecla("KeyO", { ctrlKey: true, shiftKey: true });
    expect(abrirArquivo).not.toHaveBeenCalled();
  });
});
