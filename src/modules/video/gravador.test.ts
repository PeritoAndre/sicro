/**
 * Importação de gravador (P4.6): conferir antes de registrar, cópia de trabalho depois (é ela que abre), Hikvision
 * antigo recusado com a frase do catálogo sem entrar no caso, e a `Derivacao` que a Edição tira da cópia.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { VideoMedia } from "@domain/video";

const cmd = vi.hoisted(() => ({
  videoDvrConferir: vi.fn(),
  videoDvrCopia: vi.fn(),
  registerVideoMedia: vi.fn(),
  listVideoMedia: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));

import { EXT_GRAVADOR, registrarVideo } from "./gravador";
import { useVideoStore } from "./store/videoStore";
import { midiaDoCaso } from "@modules/edicao/store/coleta";

const WS = "/casos/c1";
const SHA_O = "a".repeat(64);
const SHA_C = "c".repeat(64);
const midia = (id: string, x: Partial<VideoMedia> = {}): VideoMedia => ({
  id, occurrence_id: "o1", original_path: null, relative_path: `Mídias/Vídeos/${id}`, filename: id, sha256: SHA_O, size_bytes: 1,
  duration_s: 51, codec: "h264", width: 2592, height: 1520, pixel_format: "yuv420p", fps_declared: 15, avg_frame_rate: "15/1",
  r_frame_rate: "100/1", time_base: "1/1000", frame_count: null, bitrate: null, raw_probe_json: "{}", warnings_json: "[]",
  created_at: "2026-11-17T10:00:00Z", updated_at: "2026-11-17T10:00:00Z", derived_from_hash: null, derivation_json: null, ...x,
});
// `derivation_json` como o Rust grava (`video/dvr.rs::copia_no_caso`)
const derivacaoDvr = (epoca: number | null) => JSON.stringify({
  kind: "copia_dvr", formato: "dahua", rotulo: "Dahua (DAV)", source: { sha256: SHA_O, filename: "cam 1.dav", relative_path: "Mídias/Vídeos/cam 1.dav" },
  ffmpeg_command: ["/usr/bin/ffmpeg", "-fflags", "+genpts", "-i", "/caso/Mídias/Vídeos/cam 1.dav", "-c", "copy", "-copyinkf", "/caso/Sistema/videos/copias-dvr/cam 1.mkv"],
  recodificado: false, quadros: 726, framemd5_sha256: "f".repeat(64), inicio_s: epoca ?? 40001.404, inicio_epoca_s: epoca, desloc_s: epoca ?? 40001.404,
  lacunas: [{ de_s: 1505762700.2, ate_s: 1505762701, quadros: 10 }],
});
const original = midia("cam 1.dav");
const copia = midia("cam 1.mkv", { relative_path: "Sistema/videos/copias-dvr/cam 1.mkv", sha256: SHA_C, derived_from_hash: SHA_O, derivation_json: derivacaoDvr(1505762699) });
const validacao = (message: string) => ({ kind: "validation", message });

beforeEach(() => {
  for (const f of Object.values(cmd)) f.mockReset();
  useVideoStore.setState({ list: [], lastError: null, isMutating: false });
  cmd.registerVideoMedia.mockResolvedValue(original);
});

describe("importação de gravador", () => {
  it("vídeo comum: só registra", async () => {
    cmd.videoDvrConferir.mockResolvedValue(null);
    const m = midia("cena.mp4");
    cmd.registerVideoMedia.mockResolvedValue(m);
    expect(await registrarVideo(WS, "/ext/cena.mp4")).toBe(m);
    expect(cmd.videoDvrConferir).toHaveBeenCalledWith("/ext/cena.mp4");
    expect(cmd.registerVideoMedia).toHaveBeenCalledWith(WS, { source_path: "/ext/cena.mp4" });
    expect(cmd.videoDvrCopia).not.toHaveBeenCalled();
    expect(useVideoStore.getState().list).toEqual([m]);
  });

  it("gravador: confere, registra o original, faz a cópia e devolve ela; os dois na lista, a cópia primeiro", async () => {
    cmd.videoDvrConferir.mockResolvedValue("dahua");
    cmd.videoDvrCopia.mockResolvedValue({ media: copia, formato: "dahua", lacunas: [{ de_s: 1505762700.2, ate_s: 1505762701, quadros: 10 }], inicio_epoca_s: 1505762699, ja_existia: false });
    expect(await registrarVideo(WS, "/ext/cam 1.dav")).toBe(copia);
    expect(cmd.videoDvrConferir.mock.invocationCallOrder[0]!).toBeLessThan(cmd.registerVideoMedia.mock.invocationCallOrder[0]!);
    expect(cmd.registerVideoMedia.mock.invocationCallOrder[0]!).toBeLessThan(cmd.videoDvrCopia.mock.invocationCallOrder[0]!);
    expect(cmd.videoDvrCopia).toHaveBeenCalledWith(WS, original.id);
    expect(useVideoStore.getState().list.map((m) => m.id)).toEqual([copia.id, original.id]);
    // anexo repetido: a cópia que já existia volta igual, sem duplicar na lista
    cmd.videoDvrCopia.mockResolvedValue({ media: copia, formato: "dahua", lacunas: [], inicio_epoca_s: 1505762699, ja_existia: true });
    expect(await registrarVideo(WS, "/ext/cam 1.dav")).toBe(copia);
    expect(useVideoStore.getState().list.map((m) => m.id)).toEqual([copia.id, original.id]);
  });

  it("Hikvision antigo: recusado com a frase do catálogo, sem entrar no caso", async () => {
    cmd.videoDvrConferir.mockRejectedValue(validacao("gravador_nao_suportado: Formato de gravador não suportado: Hikvision antigo (4HKH)"));
    await expect(registrarVideo(WS, "/ext/ch01.mp4")).rejects.toThrow(/^Formato de gravador não suportado: Hikvision antigo \(4HKH\)$/);
    expect(cmd.registerVideoMedia).not.toHaveBeenCalled();
    expect(cmd.videoDvrCopia).not.toHaveBeenCalled();
    expect(useVideoStore.getState().list).toEqual([]);
  });

  it("cópia que não confere: o original fica no caso e o erro é só a frase", async () => {
    cmd.videoDvrConferir.mockResolvedValue("ifv");
    cmd.videoDvrCopia.mockRejectedValue(validacao("copia_divergente: Cópia de trabalho diferente do original: 6000 × 0 quadros"));
    await expect(registrarVideo(WS, "/ext/x.ifv")).rejects.toThrow(/^Cópia de trabalho diferente do original: 6000 × 0 quadros$/);
    expect(useVideoStore.getState().list).toEqual([original]);
  });

  it("extensões do diálogo: as de gravador", () => {
    for (const x of ["dav", "ifv", "lvf", "264", "h264", "265", "h265", "hevc"]) expect(EXT_GRAVADOR).toContain(x);
  });
});

describe("a cópia na Edição (coleta)", () => {
  it("copia_dvr, sem perdas, com a data do relógio do gravador; sem data, o campo não vai", () => {
    const lista = [original, copia];
    const d = midiaDoCaso(copia, lista, null).derivacao!;
    expect(d).toEqual({ tipo: "copia_dvr", de_sha256: SHA_O, raiz_sha256: SHA_O, comando: expect.stringContaining("-c copy -copyinkf"), com_perdas: false, inicio_epoca_s: 1505762699 });
    const semData = { ...copia, derivation_json: derivacaoDvr(null) };
    const d2 = midiaDoCaso(semData, [original, semData], null).derivacao!;
    expect(d2.tipo).toBe("copia_dvr");
    expect("inicio_epoca_s" in d2).toBe(false);
    // outro tipo nunca leva o campo
    const trecho = { ...copia, derivation_json: JSON.stringify({ kind: "clip", mode: "copy", inicio_epoca_s: 1 }) };
    expect(midiaDoCaso(trecho, [original, trecho], null).derivacao).toEqual({ tipo: "trecho", de_sha256: SHA_O, raiz_sha256: SHA_O, comando: "", com_perdas: false });
  });
});
