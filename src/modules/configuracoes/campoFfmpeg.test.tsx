// @vitest-environment happy-dom
/**
 * Configurações → ffmpeg (P2.1, contratos-fase2 (f)): "ffmpeg" (caminho; vazio = automático, sem texto de exemplo),
 * "Escolher…" e "Automático" gravam `paths.ffmpeg_path` pelas Configurações.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defaultAppSettings } from "@domain/app_settings";

const cmd = vi.hoisted(() => ({ saveAppSettings: vi.fn(), getAppSettings: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
const dialogo = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialogo);

import { act } from "react";
import { useSettingsStore } from "@stores/settingsStore";
import { CampoFfmpeg } from "./CampoFfmpeg";
import { desmontarTudo, esperar, montar } from "../edicao/inspetor/testeTela";

const botao = (raiz: HTMLElement, t: string) => [...raiz.querySelectorAll("button")].find((b) => (b.textContent ?? "").trim() === t) as HTMLButtonElement;
const salvo = () => cmd.saveAppSettings.mock.calls.at(-1)?.[0]?.paths?.ffmpeg_path;

beforeEach(() => {
  cmd.saveAppSettings.mockReset().mockResolvedValue(undefined);
  dialogo.open.mockReset();
  useSettingsStore.setState({ settings: defaultAppSettings(), loaded: true });
});
afterEach(() => desmontarTudo());

describe("campo do ffmpeg", () => {
  it("rótulos, sem texto de exemplo; Escolher… grava o caminho; Automático limpa", async () => {
    const t = await montar(<CampoFfmpeg />);
    const input = t.raiz.querySelector("input") as HTMLInputElement;
    expect(t.raiz.querySelector("label")?.textContent).toBe("ffmpeg");
    expect(input.value).toBe("");
    expect(input.getAttribute("placeholder")).toBeNull();
    expect([...t.raiz.querySelectorAll("button")].map((b) => b.textContent?.trim())).toEqual(["Escolher…", "Automático"]);
    expect(botao(t.raiz, "Automático").disabled).toBe(true);

    dialogo.open.mockResolvedValue("/opt/ffmpeg-9/ffmpeg");
    await esperar(async () => botao(t.raiz, "Escolher…").click());
    expect(salvo()).toBe("/opt/ffmpeg-9/ffmpeg");
    expect(input.value).toBe("/opt/ffmpeg-9/ffmpeg");
    expect(useSettingsStore.getState().settings.paths.ffmpeg_path).toBe("/opt/ffmpeg-9/ffmpeg");

    await esperar(async () => botao(t.raiz, "Automático").click());
    expect(salvo()).toBe("");
    expect(input.value).toBe("");
  });

  it("digitar e sair do campo grava (espaços cortados); o mesmo valor não regrava", async () => {
    const t = await montar(<CampoFfmpeg />);
    const input = t.raiz.querySelector("input") as HTMLInputElement;
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    await act(async () => {
      set.call(input, "  /usr/bin/ffmpeg ");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => {
      input.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    });
    expect(salvo()).toBe("/usr/bin/ffmpeg");
    const n = cmd.saveAppSettings.mock.calls.length;
    await act(async () => {
      input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    });
    expect(cmd.saveAppSettings.mock.calls.length).toBe(n);
  });
});
