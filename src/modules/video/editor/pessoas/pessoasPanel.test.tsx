// @vitest-environment happy-dom
/**
 * Aba Rastreio da análise (fecho da Fase 2): com "Usar modelos de IA" desligado some o "Rastrear" (YOLOX) e o que
 * só serve a ele; a mancha segue. A primeira mancha de uma mídia prepara a tabela de quadros: barra com "Preparando"
 * e "Cancelar".
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defaultAppSettings } from "@domain/app_settings";
import type { VideoMedia } from "@domain/video";

const cmd = vi.hoisted(() => ({
  getAiStatus: vi.fn(async () => ({ pessoas_ok: true })),
  pessoasListar: vi.fn(async () => ({ run: null, pessoas: [], tamanho: { width: 320, height: 240 } })),
  pessoasClasses: vi.fn(async () => []),
  pessoasCancelar: vi.fn(async () => undefined),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { MemoryRouter } from "react-router-dom";
import { useSettingsStore } from "@stores/settingsStore";
import { PessoasPanel } from "./PessoasPanel";
import { usePessoasStore } from "./pessoasStore";
import { desmontarTudo, esperar, montar } from "../../../edicao/inspetor/testeTela";

const media = { id: "v1", sha256: "a".repeat(64), filename: "cam.mp4" } as unknown as VideoMedia;
const botoes = (raiz: HTMLElement) => [...raiz.querySelectorAll("button")].map((b) => (b.textContent ?? "").trim());

async function painel(desligada: boolean) {
  useSettingsStore.setState({ settings: { ...defaultAppSettings(), ai: { ...defaultAppSettings().ai, modelos_desligados: desligada } }, loaded: true });
  const t = await montar(
    <MemoryRouter>
      <PessoasPanel workspacePath="/ws" media={media} currentTime={1} events={[]} onSeek={() => {}} hasAudio={false} />
    </MemoryRouter>,
  );
  await esperar();
  return t;
}

beforeEach(() => usePessoasStore.setState({ tarefa: null, marcando: null, erro: null }));
afterEach(() => desmontarTudo());

describe("aba Rastreio e o interruptor", () => {
  it("ligado: Rastrear e Marcar à mão; desligado: só a mancha", async () => {
    let t = await painel(false);
    expect(botoes(t.raiz)).toEqual(expect.arrayContaining(["Rastrear", "Marcar à mão"]));
    expect(t.raiz.textContent).toContain("Detectar");
    desmontarTudo();
    t = await painel(true);
    const b = botoes(t.raiz);
    expect(b).toContain("Marcar à mão");
    expect(b.some((x) => x.startsWith("Rastrear") || x.startsWith("Baixar"))).toBe(false);
    expect(t.raiz.textContent).not.toContain("Detectar");
    expect(t.raiz.textContent).not.toContain("Confiança");
  });

  it("preparar: barra com o andamento, Preparando e Cancelar", async () => {
    const t = await painel(true);
    await esperar(async () => usePessoasStore.setState({ tarefa: { fase: "preparar", feitos: 250, total: 1000, pessoas: 0 } }));
    const barra = t.raiz.querySelector("[data-fase=preparar]") as HTMLElement;
    expect(barra.textContent).toBe("Preparando");
    expect((barra.firstElementChild as HTMLElement).style.width).toBe("25%");
    expect(botoes(t.raiz)).toContain("Cancelar");
    expect(botoes(t.raiz)).not.toContain("Marcar à mão");
    await esperar(async () => usePessoasStore.setState({ tarefa: { fase: "mancha", feitos: 12, total: 0, pessoas: 0 } }));
    expect(t.raiz.querySelector("[data-fase=mancha]")?.textContent).toBe("seguindo… 12");
  });
});
