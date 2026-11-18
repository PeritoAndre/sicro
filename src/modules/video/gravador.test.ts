/**
 * Importação de gravador (P4.6, P4.6b): conferir antes de tudo; a cópia de trabalho feita e provada a partir do
 * arquivo escolhido, com avanço e Cancelar, ANTES de o original entrar no caso; depois o original e a cópia (é ela que
 * abre). Cancelada, nada no caso. Hikvision antigo recusado com a frase do catálogo. A `Derivacao` que a Edição tira
 * da cópia (data do relógio, taxa suposta).
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AvancoGravador, VideoMedia } from "@domain/video";

const cmd = vi.hoisted(() => ({
  videoDvrConferir: vi.fn(),
  videoDvrCopia: vi.fn(),
  videoDvrPreparar: vi.fn(),
  videoDvrRegistrar: vi.fn(),
  videoDvrCancelar: vi.fn(),
  registerVideoMedia: vi.fn(),
  listVideoMedia: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
// o evento do avanço: quem escuta e o que manda
const evento = vi.hoisted(() => ({ nome: null as string | null, ouvinte: null as ((e: { payload: unknown }) => void) | null, soltou: 0 }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (nome: string, f: (e: { payload: unknown }) => void) => {
    evento.nome = nome;
    evento.ouvinte = f;
    return () => {
      evento.ouvinte = null;
      evento.soltou += 1;
    };
  }),
}));

import { EXT_GRAVADOR, ImportacaoCancelada, cancelarImportacao, copiaDeGravador, ehCancelamento, registrarVideo, useImportacaoGravador } from "./gravador";
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
// `derivation_json` como o Rust grava (`video/dvr.rs::registrar_copia`)
const derivacaoDvr = (epoca: number | null, cru: { taxa: string; suposta: boolean } | null = null) => JSON.stringify({
  kind: "copia_dvr", formato: cru ? "hevc_cru" : "dahua", rotulo: "Dahua (DAV)", source: { sha256: SHA_O, filename: "cam 1.dav", relative_path: "Mídias/Vídeos/cam 1.dav" },
  ffmpeg_command: ["/usr/bin/ffmpeg", "-fflags", "+genpts", "-i", "/caso/Mídias/Vídeos/cam 1.dav", "-c", "copy", "-copyinkf", "/caso/Sistema/videos/copias-dvr/cam 1.mkv"],
  recodificado: false, taxa: cru?.taxa ?? null, taxa_suposta: cru ? cru.suposta : null, quadros: 726, framemd5_sha256: "f".repeat(64),
  inicio_s: epoca ?? 40001.404, inicio_epoca_s: epoca, desloc_s: epoca ?? 40001.404,
  lacunas: [{ de_s: 1505762700.2, ate_s: 1505762701, quadros: 10 }],
});
const original = midia("cam 1.dav");
const copia = midia("cam 1.mkv", { relative_path: "Sistema/videos/copias-dvr/cam 1.mkv", sha256: SHA_C, derived_from_hash: SHA_O, derivation_json: derivacaoDvr(1505762699) });
const validacao = (message: string) => ({ kind: "validation", message });
const registrada = (ja_existia: boolean) => ({ media: copia, formato: "dahua", lacunas: [], inicio_epoca_s: 1505762699, ja_existia });
const ordem = (f: { mock: { invocationCallOrder: number[] } }) => f.mock.invocationCallOrder[0]!;

beforeEach(() => {
  for (const f of Object.values(cmd)) f.mockReset();
  Object.assign(evento, { nome: null, ouvinte: null, soltou: 0 });
  useVideoStore.setState({ list: [], lastError: null, isMutating: false });
  useImportacaoGravador.setState({ fase: null, fracao: null });
  cmd.registerVideoMedia.mockResolvedValue(original);
  cmd.videoDvrCancelar.mockResolvedValue(undefined);
});

describe("importação de gravador", () => {
  it("vídeo comum: só registra", async () => {
    cmd.videoDvrConferir.mockResolvedValue(null);
    const m = midia("cena.mp4");
    cmd.registerVideoMedia.mockResolvedValue(m);
    expect(await registrarVideo(WS, "/ext/cena.mp4")).toBe(m);
    expect(cmd.videoDvrConferir).toHaveBeenCalledWith("/ext/cena.mp4");
    expect(cmd.registerVideoMedia).toHaveBeenCalledWith(WS, { source_path: "/ext/cena.mp4" });
    expect(cmd.videoDvrPreparar).not.toHaveBeenCalled();
    expect(cmd.videoDvrRegistrar).not.toHaveBeenCalled();
    expect(useVideoStore.getState().list).toEqual([m]);
  });

  it("gravador: confere, prepara a cópia do arquivo escolhido, registra o original e depois a cópia; devolve a cópia", async () => {
    cmd.videoDvrConferir.mockResolvedValue("dahua");
    cmd.videoDvrPreparar.mockResolvedValue({ chave: "k1", formato: "dahua", ja_existia: false });
    cmd.videoDvrRegistrar.mockResolvedValue(registrada(false));
    expect(await registrarVideo(WS, "/ext/cam 1.dav")).toBe(copia);
    expect(cmd.videoDvrPreparar).toHaveBeenCalledWith(WS, "/ext/cam 1.dav");
    expect(cmd.videoDvrRegistrar).toHaveBeenCalledWith(WS, original.id, "k1");
    // conferir → preparar → original → cópia
    expect(ordem(cmd.videoDvrConferir)).toBeLessThan(ordem(cmd.videoDvrPreparar));
    expect(ordem(cmd.videoDvrPreparar)).toBeLessThan(ordem(cmd.registerVideoMedia));
    expect(ordem(cmd.registerVideoMedia)).toBeLessThan(ordem(cmd.videoDvrRegistrar));
    expect(useVideoStore.getState().list.map((m) => m.id)).toEqual([copia.id, original.id]);
    expect(useImportacaoGravador.getState()).toMatchObject({ fase: null, fracao: null });
    expect(evento.soltou).toBe(1);
    // anexo repetido: a cópia que já existia volta igual, sem duplicar na lista
    cmd.videoDvrPreparar.mockResolvedValue({ chave: "k2", formato: "dahua", ja_existia: true });
    cmd.videoDvrRegistrar.mockResolvedValue(registrada(true));
    expect(await registrarVideo(WS, "/ext/cam 1.dav")).toBe(copia);
    expect(useVideoStore.getState().list.map((m) => m.id)).toEqual([copia.id, original.id]);
  });

  it("avanço: as fases e frações do evento vão para a tela; no registro, sem Cancelar e sem o evento por cima", async () => {
    cmd.videoDvrConferir.mockResolvedValue("dahua");
    let fim!: (p: unknown) => void;
    cmd.videoDvrPreparar.mockImplementation(() => new Promise((r) => (fim = r)));
    let soltarRegistro!: (m: VideoMedia) => void;
    cmd.registerVideoMedia.mockImplementation(() => new Promise((r) => (soltarRegistro = r)));
    cmd.videoDvrRegistrar.mockResolvedValue(registrada(false));
    const p = registrarVideo(WS, "/ext/cam 1.dav");
    await vi.waitFor(() => expect(cmd.videoDvrPreparar).toHaveBeenCalled());
    expect(evento.nome).toBe("dvr-progress");
    expect(useImportacaoGravador.getState()).toEqual({ fase: "copia", fracao: null });
    const manda = (payload: AvancoGravador) => evento.ouvinte!({ payload });
    manda({ fase: "copia", fracao: 0.5 });
    expect(useImportacaoGravador.getState()).toEqual({ fase: "copia", fracao: 0.5 });
    manda({ fase: "prova", fracao: 0.37 });
    expect(useImportacaoGravador.getState()).toEqual({ fase: "prova", fracao: 0.37 });
    fim({ chave: "k1", formato: "dahua", ja_existia: false });
    await vi.waitFor(() => expect(cmd.registerVideoMedia).toHaveBeenCalled());
    expect(useImportacaoGravador.getState()).toEqual({ fase: "registro", fracao: null });
    // evento atrasado não volta a fase
    evento.ouvinte?.({ payload: { fase: "prova", fracao: 0.99 } });
    expect(useImportacaoGravador.getState().fase).toBe("registro");
    soltarRegistro(original);
    await p;
    expect(useImportacaoGravador.getState()).toEqual({ fase: null, fracao: null });
  });

  it("Cancelar: a importação volta cancelada e nada entra no caso", async () => {
    cmd.videoDvrConferir.mockResolvedValue("dahua");
    cmd.videoDvrPreparar.mockRejectedValue(validacao("cancelado"));
    const e = await registrarVideo(WS, "/ext/cam 1.dav").catch((x: unknown) => x);
    expect(e).toBeInstanceOf(ImportacaoCancelada);
    expect(ehCancelamento(e)).toBe(true);
    expect(cmd.registerVideoMedia).not.toHaveBeenCalled();
    expect(cmd.videoDvrRegistrar).not.toHaveBeenCalled();
    expect(useVideoStore.getState().list).toEqual([]);
    expect(useImportacaoGravador.getState()).toEqual({ fase: null, fracao: null });
    expect(evento.soltou).toBe(1);
    // o botão pede ao backend
    cancelarImportacao();
    expect(cmd.videoDvrCancelar).toHaveBeenCalledTimes(1);
    expect(ehCancelamento(new Error("Cópia de trabalho diferente do original"))).toBe(false);
  });

  it("Hikvision antigo: recusado com a frase do catálogo, sem entrar no caso", async () => {
    cmd.videoDvrConferir.mockRejectedValue(validacao("gravador_nao_suportado: Formato de gravador não suportado: Hikvision antigo (4HKH)"));
    await expect(registrarVideo(WS, "/ext/ch01.mp4")).rejects.toThrow(/^Formato de gravador não suportado: Hikvision antigo \(4HKH\)$/);
    expect(cmd.registerVideoMedia).not.toHaveBeenCalled();
    expect(cmd.videoDvrPreparar).not.toHaveBeenCalled();
    expect(useVideoStore.getState().list).toEqual([]);
  });

  it("cópia que não confere: o original fica no caso e o erro é só a frase", async () => {
    cmd.videoDvrConferir.mockResolvedValue("ifv");
    cmd.videoDvrPreparar.mockRejectedValue(validacao("copia_divergente: Cópia de trabalho diferente do original: 6000 × 0 quadros"));
    await expect(registrarVideo(WS, "/ext/x.ifv")).rejects.toThrow(/^Cópia de trabalho diferente do original: 6000 × 0 quadros$/);
    expect(useVideoStore.getState().list).toEqual([original]);
    expect(cmd.videoDvrRegistrar).not.toHaveBeenCalled();
    expect(useImportacaoGravador.getState().fase).toBeNull();
  });

  it("extensões do diálogo: as de gravador", () => {
    for (const x of ["dav", "ifv", "lvf", "264", "h264", "265", "h265", "hevc"]) expect(EXT_GRAVADOR).toContain(x);
  });
});

describe("a cópia na tela (copiaDeGravador)", () => {
  it("original, relógio, lacunas e taxa suposta da cópia; outro derivado ou JSON ruim: null", () => {
    expect(copiaDeGravador(copia.derivation_json)).toEqual({ origem: { sha256: SHA_O, filename: "cam 1.dav" }, relogio: null, lacunas: 1, taxaSuposta: null });
    const comRelogio = JSON.stringify({ ...JSON.parse(derivacaoDvr(1505762699)), relogio: "2017-09-18 19:24:59.000", lacunas: [] });
    expect(copiaDeGravador(comRelogio)).toEqual({ origem: { sha256: SHA_O, filename: "cam 1.dav" }, relogio: "2017-09-18 19:24:59.000", lacunas: 0, taxaSuposta: null });
    expect(copiaDeGravador(derivacaoDvr(null, { taxa: "25/1", suposta: true }))?.taxaSuposta).toBe("25");
    expect(copiaDeGravador(derivacaoDvr(null, { taxa: "30000/1001", suposta: true }))?.taxaSuposta).toBe("29,97");
    // taxa do SPS: nada suposto
    expect(copiaDeGravador(derivacaoDvr(null, { taxa: "15/1", suposta: false }))?.taxaSuposta).toBeNull();
    for (const j of [null, "", "{", JSON.stringify({ kind: "clip", source: { sha256: SHA_O, filename: "x" } }), JSON.stringify({ kind: "copia_dvr" })]) {
      expect(copiaDeGravador(j)).toBeNull();
    }
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

  it("fluxo cru sem temporização: a taxa suposta vai na Derivacao (igual a importar_v1::derivacao); do SPS, não vai", () => {
    const sup = { ...copia, derivation_json: derivacaoDvr(null, { taxa: "25/1", suposta: true }) };
    expect(midiaDoCaso(sup, [original, sup], null).derivacao!.taxa_suposta).toEqual({ num: 25, den: 1 });
    const sps = { ...copia, derivation_json: derivacaoDvr(null, { taxa: "15/1", suposta: false }) };
    expect("taxa_suposta" in midiaDoCaso(sps, [original, sps], null).derivacao!).toBe(false);
    const outro = { ...copia, derivation_json: JSON.stringify({ kind: "clip", taxa: "25/1", taxa_suposta: true }) };
    expect("taxa_suposta" in midiaDoCaso(outro, [original, outro], null).derivacao!).toBe(false);
  });
});
