// @vitest-environment happy-dom
/**
 * Interruptor "Usar modelos de IA" (§6.6, P2.1) na degravação: desligado, somem a transcrição ("Rascunho IA", o
 * idioma, o whisper e o modelo) e a separação de locutores ("Locutores" e o número de pessoas); o resto da tela fica.
 */

import { forwardRef } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defaultAppSettings } from "@domain/app_settings";

const cmd = vi.hoisted(() => ({
  openAudioMedia: vi.fn(),
  listAudioTranscript: vi.fn(),
  getAudioDiarization: vi.fn(),
  whisperStatus: vi.fn(),
  transcribeAudio: vi.fn(),
  diarizeAudio: vi.fn(),
  saveAudioTranscript: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));
vi.mock("@core/mediaSrc", () => ({ mediaSrc: (p: string) => p }));
vi.mock("react-router-dom", () => ({ useParams: () => ({ audioId: "a1" }), useNavigate: () => vi.fn() }));
vi.mock("../AudioPlayer", () => ({
  AudioPlayer: forwardRef(function AudioPlayer() {
    return <div data-player />;
  }),
  fmtTime: (t: number) => t.toFixed(1),
}));

import { useSettingsStore } from "@stores/settingsStore";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { DegravacaoView } from "../DegravacaoView";
import { desmontarTudo, esperar, montar, type Tela } from "../../edicao/inspetor/testeTela";

const textos = (t: Tela) => [...t.raiz.querySelectorAll("button, label")].map((b) => (b.textContent ?? "").trim());

async function abrir(desligada: boolean): Promise<Tela> {
  const s = defaultAppSettings();
  s.ai.modelos_desligados = desligada;
  useSettingsStore.setState({ settings: s, loaded: true });
  const t = await montar(<DegravacaoView />);
  await esperar();
  await esperar();
  return t;
}

beforeEach(() => {
  const m = new Map<string, string>();
  vi.stubGlobal("localStorage", { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), removeItem: (k: string) => void m.delete(k) });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.openAudioMedia.mockResolvedValue({ id: "a1", sha256: "s".repeat(64), filename: "fala.wav", relative_path: "audio/wav/fala.wav", duration_s: 12 });
  cmd.listAudioTranscript.mockResolvedValue([]);
  cmd.getAudioDiarization.mockResolvedValue(null);
  cmd.whisperStatus.mockResolvedValue({ available: true, path: "/usr/bin/whisper-cli" });
  useWorkspaceStore.setState({ activeWorkspacePath: "/casos/c1" });
});

afterEach(() => {
  desmontarTudo();
  vi.unstubAllGlobals();
});

describe("Degravação e o interruptor da IA", () => {
  it("ligado: \"Rascunho IA\" e \"Locutores\" na barra", async () => {
    const t = await abrir(false);
    const b = textos(t);
    expect(b).toContain("Capturar trecho");
    expect(b).toContain("Rascunho IA");
    expect(b).toContain("Locutores");
    expect(t.raiz.textContent).toContain("whisper:");
  });

  it("desligado: transcrição e locutores somem; capturar trecho fica", async () => {
    const t = await abrir(true);
    const b = textos(t);
    expect(b).toContain("Capturar trecho");
    expect(b).not.toContain("Rascunho IA");
    expect(b).not.toContain("Locutores");
    expect(b.some((x) => x.startsWith("idioma") || x.startsWith("pessoas"))).toBe(false);
    expect(t.raiz.textContent).not.toContain("whisper:");
    expect(cmd.transcribeAudio).not.toHaveBeenCalled();
    expect(cmd.diarizeAudio).not.toHaveBeenCalled();
  });

  it("desligado: o atalho do rascunho (Ctrl+Shift+T) não transcreve nem abre o seletor de modelo", async () => {
    const atalho = () =>
      esperar(async () => {
        window.dispatchEvent(new KeyboardEvent("keydown", { key: "T", code: "KeyT", ctrlKey: true, shiftKey: true, bubbles: true }));
      });
    const { open } = await import("@tauri-apps/plugin-dialog");
    cmd.transcribeAudio.mockResolvedValue([]);
    await abrir(true);
    await atalho();
    expect(cmd.transcribeAudio).not.toHaveBeenCalled();
    expect(open).not.toHaveBeenCalled();
    desmontarTudo();
    // ligado, o mesmo atalho transcreve (o teste enxerga o disparo)
    const s = defaultAppSettings();
    s.ai.model_path = "/modelos/ggml-base.bin";
    useSettingsStore.setState({ settings: s, loaded: true });
    await montar(<DegravacaoView />);
    await esperar();
    await esperar();
    await atalho();
    expect(cmd.transcribeAudio).toHaveBeenCalledTimes(1);
  });
});
