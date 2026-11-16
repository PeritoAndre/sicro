// @vitest-environment happy-dom
/** Aba Realce (fecho da Fase 2): com "Usar modelos de IA" desligado some a super-resolução; as outras ferramentas
 *  ficam, e quem estava nela volta para a primeira. */

import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultAppSettings } from "@domain/app_settings";
import type { VideoMedia } from "@domain/video";

const cmd = vi.hoisted(() => ({
  realceListar: vi.fn(async () => []),
  realceLenteObter: vi.fn(async () => null),
  getAiStatus: vi.fn(async () => ({ superres_ok: true })),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { MemoryRouter } from "react-router-dom";
import { useSettingsStore } from "@stores/settingsStore";
import { RealcePanel } from "./RealcePanel";
import { useRealceStore } from "./realceStore";
import { desmontarTudo, esperar, montar } from "../../../edicao/inspetor/testeTela";

const media = { id: "v1", sha256: "b".repeat(64), filename: "cam.mp4" } as unknown as VideoMedia;
const abas = (raiz: HTMLElement) => [...raiz.querySelectorAll("nav button")].map((b) => (b.textContent ?? "").trim());

async function painel(desligada: boolean) {
  useSettingsStore.setState({ settings: { ...defaultAppSettings(), ai: { ...defaultAppSettings().ai, modelos_desligados: desligada } }, loaded: true });
  const t = await montar(
    <MemoryRouter>
      <RealcePanel workspacePath="/ws" media={media} currentTime={1} onVideoDerivado={() => {}} />
    </MemoryRouter>,
  );
  await esperar();
  return t;
}

afterEach(() => desmontarTudo());

describe("realce e o interruptor", () => {
  it("ligado: Super-res.; desligado: some, e a aba aberta nela volta para Média", async () => {
    let t = await painel(false);
    expect(abas(t.raiz)).toEqual(["Média", "Super-res.", "Estabilizar", "Lente", "Perspectiva"]);
    await esperar(async () => useRealceStore.getState().setAba("superres"));
    expect(cmd.getAiStatus).toHaveBeenCalled();
    desmontarTudo();
    cmd.getAiStatus.mockClear();
    t = await painel(true);
    expect(abas(t.raiz)).toEqual(["Média", "Estabilizar", "Lente", "Perspectiva"]);
    expect(useRealceStore.getState().aba).toBe("media");
    expect(cmd.getAiStatus).not.toHaveBeenCalled();
  });
});
