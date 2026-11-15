// @vitest-environment happy-dom
/**
 * Análise com um vídeo aberto (fecho da Fase 1): o reprodutor recebe a entrada e a saída marcadas (`trecho`) mesmo com
 * a repetição desligada (o Ctrl+3 coleta por elas, §8.1); "Excluir vídeo" só exclui depois da confirmação, pelo
 * `deleteMedia` da store, e a falha aparece na tela.
 */

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { VideoBundle, VideoMedia } from "@domain/video";

vi.mock("@core/commands", () => ({ commands: new Proxy({}, { get: () => vi.fn(async () => []) }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
const confirmar = vi.hoisted(() => ({ confirmarExclusaoVideo: vi.fn(async (_ws: string, _sha: string) => true) }));
vi.mock("../excluirVideo", () => confirmar);
// reprodutor falso: guarda as props e entrega um controlador com o tempo que o teste põe
const palco = vi.hoisted(() => ({ props: [] as Record<string, unknown>[], tempo: 0 }));
vi.mock("./VideoPlayerPanel", () => ({
  VideoPlayerPanel: (p: Record<string, unknown> & { registerController(c: unknown): void }) => {
    palco.props.push(p);
    p.registerController({ seek() {}, scrubStart() {}, scrub() {}, scrubEnd() {}, getTime: () => palco.tempo });
    return null;
  },
}));

import { VideoAnalysisView } from "./VideoAnalysisView";
import { useMedirStore } from "./medir/medirStore";
import { useVideoStore } from "../store/videoStore";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { useDialogosAbertos } from "@components/Dialog/Dialog";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const WS = "/casos/c1";
const SHA = "ab".repeat(32);
const media = {
  id: "v1",
  occurrence_id: "o1",
  original_path: null,
  relative_path: "Mídias/Vídeos/cam1.mp4",
  filename: "cam1.mp4",
  sha256: SHA,
  size_bytes: 1,
  duration_s: 60,
  codec: "h264",
  width: 1280,
  height: 720,
  pixel_format: "yuv420p",
  fps_declared: 30,
  avg_frame_rate: "30/1",
  r_frame_rate: "30/1",
  time_base: "1/15360",
  frame_count: 1800,
  bitrate: null,
  raw_probe_json: JSON.stringify({ format: { start_time: "0" }, streams: [{ codec_type: "video" }] }),
  warnings_json: "[]",
  created_at: "",
  updated_at: "",
} as unknown as VideoMedia;
const bundle = { media, events: [], storyboard: [] } as unknown as VideoBundle;

let raiz: Root | null = null;
let div: HTMLDivElement;
const montar = async () => {
  div = document.createElement("div");
  document.body.appendChild(div);
  raiz = createRoot(div);
  await act(async () => raiz!.render(<VideoAnalysisView visivel />));
};
const tecla = (code: string, mods: Partial<KeyboardEventInit> = {}) =>
  act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { code, key: code.replace(/^Key/, "").toLowerCase(), bubbles: true, ...mods })));
const ultimas = () => palco.props.at(-1) as { loop: unknown; trecho: unknown };
const excluir = vi.fn(async (_ws: string, _id: string) => {});

beforeEach(() => {
  palco.props = [];
  palco.tempo = 0;
  confirmar.confirmarExclusaoVideo.mockReset();
  confirmar.confirmarExclusaoVideo.mockResolvedValue(true);
  excluir.mockReset();
  excluir.mockResolvedValue(undefined);
  useWorkspaceStore.setState({ activeWorkspacePath: WS, activeOccurrence: { id: "o1", peritos: [] } as never });
  useVideoStore.setState({ activeMediaId: "v1", bundle, edicao: false, list: [media], deleteMedia: excluir });
  useMedirStore.getState().setModo("assistir");
  useDialogosAbertos.setState({ pilha: [] });
});
afterEach(() => {
  act(() => raiz?.unmount());
  raiz = null;
  document.body.innerHTML = "";
});

describe("VideoAnalysisView com vídeo aberto", () => {
  it("I e O marcados chegam ao reprodutor como `trecho`, com a repetição ligada ou desligada (Ctrl+3 coleta por eles)", async () => {
    await montar();
    expect(ultimas().trecho ?? null).toBeNull();
    palco.tempo = 5;
    await tecla("KeyO");
    // só a saída: ainda não há trecho
    expect(ultimas().trecho ?? null).toBeNull();
    palco.tempo = 2;
    await tecla("KeyI");
    // fechou o trecho: a repetição liga sozinha
    expect(ultimas()).toMatchObject({ trecho: { a: 2, b: 5 }, loop: { a: 2, b: 5 } });
    // Ctrl+L desliga a repetição: o trecho continua indo
    await tecla("KeyL", { ctrlKey: true });
    expect(ultimas().loop).toBeNull();
    expect(ultimas().trecho).toEqual({ a: 2, b: 5 });
  });

  it("Excluir vídeo: pergunta com o SHA-256 da mídia; confirmado, exclui pela store; recusado, não exclui; falha aparece", async () => {
    await montar();
    const botao = [...div.querySelectorAll("button")].find((b) => (b.textContent ?? "").trim() === "Excluir vídeo")!;
    expect(botao).toBeTruthy();
    // recusado
    confirmar.confirmarExclusaoVideo.mockResolvedValueOnce(false);
    await act(async () => botao.click());
    expect(confirmar.confirmarExclusaoVideo).toHaveBeenCalledWith(WS, SHA);
    expect(excluir).not.toHaveBeenCalled();
    // confirmado
    await act(async () => botao.click());
    expect(excluir).toHaveBeenCalledTimes(1);
    expect(excluir).toHaveBeenCalledWith(WS, "v1");
    // a exclusão falhou: a mensagem do erro na tela, o vídeo continua aberto
    excluir.mockRejectedValueOnce({ kind: "io", message: "arquivo em uso" });
    await act(async () => botao.click());
    expect(div.textContent).toContain("Falha: arquivo em uso");
    expect(useVideoStore.getState().bundle).toBe(bundle);
  });
});
