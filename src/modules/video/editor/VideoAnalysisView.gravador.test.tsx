// @vitest-environment happy-dom
/**
 * Análise com a cópia de trabalho de gravador aberta (P4.6): a faixa diz de qual original ela é, o relógio do
 * gravador e as lacunas, e abre o original; a aba da cópia tem o ícone próprio.
 */

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { VideoBundle, VideoMedia } from "@domain/video";

vi.mock("@core/commands", () => ({ commands: new Proxy({}, { get: () => vi.fn(async () => []) }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("./VideoPlayerPanel", () => ({
  VideoPlayerPanel: (p: { registerController(c: unknown): void }) => {
    p.registerController({ seek() {}, scrubStart() {}, scrub() {}, scrubEnd() {}, getTime: () => 0 });
    return null;
  },
}));

import { VideoAnalysisView } from "./VideoAnalysisView";
import { useMedirStore } from "./medir/medirStore";
import { useVideoStore } from "../store/videoStore";
import { useWorkspaceStore } from "@stores/workspaceStore";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const WS = "/casos/c1";
const SHA_O = "a".repeat(64);
const midia = (id: string, x: Partial<VideoMedia> = {}): VideoMedia => ({
  id, occurrence_id: "o1", original_path: null, relative_path: `Mídias/Vídeos/${id}`, filename: id, sha256: SHA_O, size_bytes: 1,
  duration_s: 51, codec: "h264", width: 2592, height: 1520, pixel_format: "yuv420p", fps_declared: 15, avg_frame_rate: "15/1",
  r_frame_rate: "15/1", time_base: "1/1000", frame_count: null, bitrate: null,
  raw_probe_json: JSON.stringify({ format: { start_time: "0" }, streams: [{ codec_type: "video" }] }), warnings_json: "[]",
  created_at: "", updated_at: "", derived_from_hash: null, derivation_json: null, ...x,
});
const original = midia("cam 1.dav");
// `derivation_json` como `video/dvr.rs::copia_no_caso` grava (só o que a tela lê)
const copia = midia("cam 1.mkv", {
  relative_path: "Sistema/videos/copias-dvr/cam 1.mkv",
  sha256: "c".repeat(64),
  derived_from_hash: SHA_O,
  derivation_json: JSON.stringify({
    kind: "copia_dvr", formato: "dahua", source: { sha256: SHA_O, filename: "cam 1.dav", relative_path: "Mídias/Vídeos/cam 1.dav" },
    inicio_epoca_s: 1505762699, relogio: "2017-09-18 19:24:59.000", lacunas: [{ de_s: 1505762700.2, ate_s: 1505762701, quadros: 10 }],
  }),
});

let raiz: Root | null = null;
let div: HTMLDivElement;
const abrir = vi.fn(async (_ws: string, _id: string) => ({}) as VideoBundle);

beforeEach(async () => {
  abrir.mockClear();
  useWorkspaceStore.setState({ activeWorkspacePath: WS, activeOccurrence: { id: "o1", peritos: [] } as never });
  useVideoStore.setState({
    activeMediaId: copia.id,
    bundle: { media: copia, events: [], storyboard: [] } as unknown as VideoBundle,
    edicao: false,
    list: [copia, original],
    openMedia: abrir,
  });
  useMedirStore.getState().setModo("assistir");
  div = document.createElement("div");
  document.body.appendChild(div);
  raiz = createRoot(div);
  await act(async () => raiz!.render(<VideoAnalysisView visivel />));
});
afterEach(() => {
  act(() => raiz?.unmount());
  raiz = null;
  document.body.innerHTML = "";
});

describe("cópia de trabalho de gravador aberta", () => {
  it("a faixa mostra o original, o relógio do gravador e as lacunas, e abre o original", async () => {
    const faixa = div.querySelector<HTMLElement>("[data-copia-gravador]");
    expect(faixa).toBeTruthy();
    const texto = (faixa!.textContent ?? "").replace(/\s+/g, " ");
    expect(texto).toContain("cópia de trabalho de cam 1.dav");
    expect(texto).toContain("relógio do gravador 2017-09-18 19:24:59");
    expect(texto).toContain("1 lacuna");
    const botao = [...faixa!.querySelectorAll("button")].find((b) => b.textContent === "abrir original")!;
    await act(async () => botao.click());
    expect(abrir).toHaveBeenCalledWith(WS, original.id);
  });

  it("a aba da cópia tem o ícone de cópia de trabalho; a do original, não", () => {
    const aba = (id: string) => div.querySelector<HTMLElement>(`[data-video-id="${id}"]`);
    expect(aba(copia.id)?.querySelector('[aria-label="cópia de trabalho"]')).toBeTruthy();
    expect(aba(original.id)?.querySelector('[aria-label="cópia de trabalho"]')).toBeNull();
  });

  it("fluxo cru sem temporização: ícone de taxa suposta na faixa (só title, sem frase); com a taxa do SPS, sem ícone", async () => {
    expect(div.querySelector("[data-taxa-suposta]")).toBeNull();
    const cru = (suposta: boolean) =>
      midia("cam 2.mkv", {
        id: `cru-${suposta}`,
        sha256: "d".repeat(64),
        derived_from_hash: SHA_O,
        derivation_json: JSON.stringify({ kind: "copia_dvr", source: { sha256: SHA_O, filename: "cam 2.h265" }, taxa: suposta ? "25/1" : "15/1", taxa_suposta: suposta, lacunas: [] }),
      });
    const abre = (m: VideoMedia) => act(async () => useVideoStore.setState({ activeMediaId: m.id, list: [m, original], bundle: { media: m, events: [], storyboard: [] } as unknown as VideoBundle }));
    await abre(cru(true));
    const icone = div.querySelector<HTMLElement>("[data-copia-gravador] [data-taxa-suposta]");
    expect(icone?.getAttribute("title")).toBe("Taxa suposta: 25 quadros/s");
    expect(icone?.textContent).toBe("");
    await abre(cru(false));
    expect(div.querySelector("[data-copia-gravador]")).toBeTruthy();
    expect(div.querySelector("[data-taxa-suposta]")).toBeNull();
  });

  it("importação de gravador em curso: o painel do avanço aparece na análise", async () => {
    const { useImportacaoGravador } = await import("../gravador");
    expect(div.querySelector("[data-importacao-gravador]")).toBeNull();
    await act(async () => useImportacaoGravador.setState({ fase: "prova", fracao: 0.42 }));
    const painel = div.querySelector<HTMLElement>("[data-importacao-gravador]");
    expect(painel?.querySelector('[title="Prova"]')).toBeTruthy();
    expect(painel?.querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe("42");
    expect(painel?.textContent).toContain("42%");
    // no registro não há Cancelar
    await act(async () => useImportacaoGravador.setState({ fase: "registro", fracao: null }));
    expect([...div.querySelectorAll("[data-importacao-gravador] button")].map((b) => b.textContent)).toEqual([]);
    await act(async () => useImportacaoGravador.setState({ fase: null, fracao: null }));
    expect(div.querySelector("[data-importacao-gravador]")).toBeNull();
  });

  it("vídeo comum aberto: sem a faixa", async () => {
    await act(async () => useVideoStore.setState({ activeMediaId: original.id, bundle: { media: original, events: [], storyboard: [] } as unknown as VideoBundle }));
    expect(div.querySelector("[data-copia-gravador]")).toBeNull();
  });
});
