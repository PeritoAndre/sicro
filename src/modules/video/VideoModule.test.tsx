// @vitest-environment happy-dom
/**
 * Hospedagem do §8.1: análise e Edição montadas juntas, uma escondida; trocar pelo trilho ou por Alt+1..6 não
 * desmonta nenhuma das duas e pausa o vídeo do lado que saiu da frente; o trilho vale nas duas.
 */

import { act, useEffect, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const vidas = vi.hoisted(() => ({ montou: { analise: 0, lista: 0, edicao: 0 }, desmontou: { analise: 0, lista: 0, edicao: 0 }, visivel: { analise: [] as boolean[], edicao: [] as boolean[] } }));

vi.mock("@core/commands", () => ({ commands: { listVideoMedia: vi.fn(async () => []) } }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

/** Lado falso: conta montagens e tem um <video> "tocando" cujo pause() o teste vê. */
function Falso({ nome, visivel }: { nome: "analise" | "lista" | "edicao"; visivel?: boolean }) {
  const v = useRef<HTMLVideoElement | null>(null);
  useEffect(() => {
    vidas.montou[nome]++;
    const el = v.current!;
    Object.defineProperty(el, "paused", { value: false, configurable: true, writable: true });
    el.pause = vi.fn(() => Object.defineProperty(el, "paused", { value: true, configurable: true, writable: true }));
    return () => void vidas.desmontou[nome]++;
  }, [nome]);
  if (nome !== "lista") vidas.visivel[nome].push(visivel ?? true);
  return <video ref={v} data-falso={nome} />;
}
vi.mock("./editor/VideoAnalysisView", () => ({ VideoAnalysisView: (p: { visivel?: boolean }) => <Falso nome="analise" visivel={p.visivel} /> }));
vi.mock("./VideoListView", () => ({ VideoListView: () => <Falso nome="lista" /> }));
vi.mock("@modules/edicao/EdicaoRaiz", () => ({ EdicaoRaiz: (p: { visivel?: boolean }) => <Falso nome="edicao" visivel={p.visivel} /> }));

import { VideoModule } from "./VideoModule";
import { useVideoStore } from "./store/videoStore";
import { useMedirStore } from "./editor/medir/medirStore";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { useDialogosAbertos } from "@components/Dialog/Dialog";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let raiz: Root | null = null;
const montar = async () => {
  const div = document.createElement("div");
  document.body.appendChild(div);
  raiz = createRoot(div);
  await act(async () => raiz!.render(<VideoModule />));
};
const lado = (n: string) => document.querySelector<HTMLElement>(`[data-lado="${n}"]`);
const video = (n: string) => document.querySelector<HTMLVideoElement>(`[data-falso="${n}"]`)!;
const trilho = (m: string) => act(async () => document.querySelector<HTMLButtonElement>(`[data-trilho] [data-modo="${m}"]`)!.click());
const tecla = (key: string) => act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key, code: `Digit${key}`, altKey: true, bubbles: true })));

beforeEach(() => {
  for (const k of ["analise", "lista", "edicao"] as const) {
    vidas.montou[k] = 0;
    vidas.desmontou[k] = 0;
  }
  vidas.visivel.analise = [];
  vidas.visivel.edicao = [];
  useWorkspaceStore.setState({ activeWorkspacePath: "/casos/c1", activeOccurrence: { id: "o1" } as never });
  useVideoStore.setState({ activeMediaId: "v1", edicao: false });
  useMedirStore.getState().setModo("assistir");
  useDialogosAbertos.setState({ pilha: [] });
});
afterEach(() => {
  act(() => raiz?.unmount());
  raiz = null;
  document.body.innerHTML = "";
});

describe("VideoModule: análise e Edição juntas (§8.1)", () => {
  it("trocar pelo trilho não desmonta nenhuma das duas e pausa o vídeo escondido", async () => {
    await montar();
    expect(vidas.montou).toMatchObject({ analise: 1, edicao: 0 });
    // o trilho mora aqui: seis modos, Assistir aceso
    expect([...document.querySelectorAll("[data-trilho] [data-modo]")].map((b) => b.getAttribute("data-modo"))).toEqual(["assistir", "velocidade", "distancia", "pessoas", "realce", "edicao"]);
    await trilho("edicao");
    expect(vidas.montou).toMatchObject({ analise: 1, edicao: 1 });
    expect(vidas.desmontou).toMatchObject({ analise: 0, edicao: 0 });
    expect(lado("analise")!.hidden).toBe(true);
    expect(lado("edicao")!.hidden).toBe(false);
    expect(video("analise").pause).toHaveBeenCalledTimes(1);
    expect(vidas.visivel.analise.at(-1)).toBe(false);
    expect(document.querySelector('[data-modo="edicao"]')!.getAttribute("aria-pressed")).toBe("true");
    // volta para a análise num modo dela: a Edição fica montada, escondida e pausada
    await trilho("velocidade");
    expect(useMedirStore.getState().modo).toBe("velocidade");
    expect(lado("analise")!.hidden).toBe(false);
    expect(lado("edicao")!.hidden).toBe(true);
    expect(video("edicao").pause).toHaveBeenCalledTimes(1);
    expect(vidas.visivel.edicao.at(-1)).toBe(false);
    await trilho("edicao");
    await trilho("assistir");
    expect(vidas.montou).toMatchObject({ analise: 1, edicao: 1 });
    expect(vidas.desmontou).toMatchObject({ analise: 0, edicao: 0 });
  });

  it("Alt+1..6 valem com a Edição na frente e na lista; diálogo aberto desliga", async () => {
    useVideoStore.setState({ activeMediaId: null });
    await montar();
    expect(vidas.montou.lista).toBe(1);
    await tecla("6");
    expect(useVideoStore.getState().edicao).toBe(true);
    await tecla("2");
    expect([useVideoStore.getState().edicao, useMedirStore.getState().modo]).toEqual([false, "velocidade"]);
    await tecla("6");
    await act(async () => useDialogosAbertos.setState({ pilha: [1] }));
    await tecla("1");
    expect(useVideoStore.getState().edicao).toBe(true);
    await act(async () => useDialogosAbertos.setState({ pilha: [] }));
    await tecla("1");
    expect([useVideoStore.getState().edicao, useMedirStore.getState().modo]).toEqual([false, "assistir"]);
    expect(vidas.desmontou).toMatchObject({ lista: 0, edicao: 0 });
  });

  it("caso trocado com o módulo desmontado (pela tela inicial): a Edição não monta nem aparece no caso novo", async () => {
    await montar();
    await trilho("edicao");
    act(() => raiz?.unmount());
    raiz = null;
    document.body.innerHTML = "";
    const montagens = vidas.montou.edicao;
    vidas.visivel.edicao = [];
    await act(async () => useWorkspaceStore.setState({ activeWorkspacePath: "/casos/c2" }));
    await montar();
    expect(vidas.montou.edicao).toBe(montagens);
    expect(vidas.visivel.edicao).toEqual([]);
    expect(useVideoStore.getState().edicao).toBe(false);
    expect(lado("analise")!.hidden).toBe(false);
  });

  it("trocar de caso desmonta a Edição e volta à análise", async () => {
    await montar();
    await trilho("edicao");
    await act(async () => useWorkspaceStore.setState({ activeWorkspacePath: "/casos/c2" }));
    expect(useVideoStore.getState().edicao).toBe(false);
    expect(vidas.desmontou.edicao).toBe(1);
    expect(lado("edicao")).toBeNull();
  });
});
