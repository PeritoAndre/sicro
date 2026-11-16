// @vitest-environment happy-dom
/** Configurações → Vídeo (fecho da Fase 2): o campo "ffmpeg" (P2.1) fica montado na categoria, acima do diagnóstico. */

import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultAppSettings } from "@domain/app_settings";

const cmd = vi.hoisted(() => ({
  edicaoCapacidades: vi.fn(() => new Promise(() => {})),
  saveAppSettings: vi.fn(async () => undefined),
  getAppSettings: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ writeText: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@modules/edicao/diagnostico/comandos", () => ({ edicaoDiagnosticoMidias: vi.fn(), edicaoDiagnosticoSalvar: vi.fn() }));

import { MemoryRouter } from "react-router-dom";
import { useSettingsStore } from "@stores/settingsStore";
import { ConfiguracoesModule } from "./ConfiguracoesModule";
import { desmontarTudo, esperar, montar } from "../edicao/inspetor/testeTela";

afterEach(() => desmontarTudo());

describe("categoria Vídeo", () => {
  it("mostra o campo do ffmpeg com Escolher… e Automático", async () => {
    useSettingsStore.setState({ settings: defaultAppSettings(), loaded: true });
    const t = await montar(
      <MemoryRouter initialEntries={["/configuracoes?aba=video"]}>
        <ConfiguracoesModule />
      </MemoryRouter>,
    );
    await esperar();
    const rotulo = t.raiz.querySelector("label[for=campo-ffmpeg]");
    expect(rotulo?.textContent).toBe("ffmpeg");
    const botoes = [...t.raiz.querySelectorAll("button")].map((b) => (b.textContent ?? "").trim());
    expect(botoes).toEqual(expect.arrayContaining(["Escolher…", "Automático"]));
  });
});
