// @vitest-environment happy-dom
/**
 * Central: a lâmina "Vídeo" com arquivo de gravador (P4.6). O diálogo mostra as extensões de gravador; o `.dav` entra
 * com a cópia de trabalho, que é o que abre; o Hikvision antigo fica fora do caso, com a frase do catálogo.
 */

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { VideoMedia } from "@domain/video";

const cmd = vi.hoisted(() => ({
  listVideoMedia: vi.fn(),
  listAudioMedia: vi.fn(),
  videoThumbnail: vi.fn(),
  edicaoListar: vi.fn(),
  videoDvrConferir: vi.fn(),
  videoDvrCopia: vi.fn(),
  registerVideoMedia: vi.fn(),
  openVideoMedia: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));
const dialogo = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialogo);

import { MidiaCentral } from "./MidiaCentral";
import { useVideoStore } from "@modules/video/store/videoStore";
import { useWorkspaceStore } from "@stores/workspaceStore";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const WS = "/casos/c1";
const midia = (id: string, x: Partial<VideoMedia> = {}): VideoMedia => ({
  id, occurrence_id: "o1", original_path: null, relative_path: `Mídias/Vídeos/${id}`, filename: id, sha256: "a".repeat(64), size_bytes: 1,
  duration_s: 51, codec: "h264", width: 2592, height: 1520, pixel_format: "yuv420p", fps_declared: 15, avg_frame_rate: "15/1",
  r_frame_rate: "100/1", time_base: "1/1000", frame_count: null, bitrate: null, raw_probe_json: "{}", warnings_json: "[]",
  created_at: "2026-11-17T10:00:00Z", updated_at: "2026-11-17T10:00:00Z", derived_from_hash: null, derivation_json: null, ...x,
});
const original = midia("cam 1.dav");
const copia = midia("cam 1.mkv", {
  sha256: "c".repeat(64),
  derived_from_hash: original.sha256,
  derivation_json: JSON.stringify({ kind: "copia_dvr", source: { sha256: original.sha256, filename: original.filename } }),
});

let raiz: Root | null = null;
const fila = () => act(async () => new Promise<void>((r) => setTimeout(r, 0)));
const lamina = () => document.querySelector<HTMLButtonElement>('[data-lamina="video"]')!;

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  dialogo.open.mockReset();
  cmd.listVideoMedia.mockResolvedValue([]);
  cmd.listAudioMedia.mockResolvedValue([]);
  cmd.edicaoListar.mockResolvedValue([]);
  cmd.videoThumbnail.mockImplementation(() => new Promise(() => {}));
  cmd.registerVideoMedia.mockResolvedValue(original);
  cmd.openVideoMedia.mockImplementation(async (_ws: string, id: string) => ({ media: id === copia.id ? copia : original, events: [], storyboard: [], exports: [] }));
  useWorkspaceStore.setState({ activeWorkspacePath: WS, activeOccurrence: { id: "o1" } as never });
  useVideoStore.setState({ edicao: false, list: [], bundle: null, activeMediaId: null });
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

afterEach(() => {
  act(() => raiz?.unmount());
  raiz = null;
  document.body.innerHTML = "";
});

describe("Central: vídeo de gravador", () => {
  it("o diálogo aceita as extensões de gravador; o .dav entra com a cópia de trabalho, que abre", async () => {
    dialogo.open.mockResolvedValue("/ext/cam 1.dav");
    cmd.videoDvrConferir.mockResolvedValue("dahua");
    cmd.videoDvrCopia.mockResolvedValue({ media: copia, formato: "dahua", lacunas: [], inicio_epoca_s: 1505762699, ja_existia: false });
    await act(async () => lamina().click());
    await fila();
    const filtros = (dialogo.open.mock.calls[0]![0] as { filters: { extensions: string[] }[] }).filters[0]!.extensions;
    for (const x of ["mp4", "avi", "dav", "ifv", "lvf", "264", "h264", "265", "hevc"]) expect(filtros).toContain(x);
    expect(cmd.registerVideoMedia).toHaveBeenCalledWith(WS, { source_path: "/ext/cam 1.dav" });
    expect(cmd.videoDvrCopia).toHaveBeenCalledWith(WS, original.id);
    expect(cmd.openVideoMedia).toHaveBeenCalledWith(WS, copia.id);
    expect(useVideoStore.getState().activeMediaId).toBe(copia.id);
    expect(document.querySelector("p")?.textContent ?? "").not.toMatch(/gravador|cópia/i);
  });

  it("na lista, a cópia vem marcada como cópia de trabalho e o original como vídeo", async () => {
    await act(async () => useVideoStore.setState({ list: [copia, original] }));
    const chips = [...document.querySelectorAll("span")].map((s) => s.textContent);
    expect(chips.filter((t) => t === "cópia de trabalho")).toHaveLength(1);
    expect(chips.filter((t) => t === "vídeo")).toHaveLength(1);
  });

  it("Hikvision antigo: a frase do catálogo na tela, nada registrado nem aberto", async () => {
    dialogo.open.mockResolvedValue("/ext/ch01.mp4");
    cmd.videoDvrConferir.mockRejectedValue({ kind: "validation", message: "gravador_nao_suportado: Formato de gravador não suportado: Hikvision antigo (4HKH)" });
    await act(async () => lamina().click());
    await fila();
    const erro = [...document.querySelectorAll("p")].map((p) => p.textContent).find((t) => t?.includes("4HKH"));
    expect(erro).toBe("Formato de gravador não suportado: Hikvision antigo (4HKH)");
    expect(cmd.registerVideoMedia).not.toHaveBeenCalled();
    expect(cmd.openVideoMedia).not.toHaveBeenCalled();
  });

  it("vídeo comum: registra e abre o próprio, sem cópia", async () => {
    const mp4 = midia("cena.mp4");
    dialogo.open.mockResolvedValue("/ext/cena.mp4");
    cmd.videoDvrConferir.mockResolvedValue(null);
    cmd.registerVideoMedia.mockResolvedValue(mp4);
    cmd.openVideoMedia.mockResolvedValue({ media: mp4, events: [], storyboard: [], exports: [] });
    await act(async () => lamina().click());
    await fila();
    expect(cmd.videoDvrCopia).not.toHaveBeenCalled();
    expect(cmd.openVideoMedia).toHaveBeenCalledWith(WS, mp4.id);
  });
});
