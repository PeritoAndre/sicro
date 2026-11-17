// @vitest-environment happy-dom
/**
 * Gerenciador de IA (P2.1, contratos-fase2 (f)): o interruptor "Usar modelos de IA" grava `modelos_desligados`;
 * os itens "Aceleração por GPU (NVIDIA)" e "Rastreador" com estado por ícone, tamanho, "Baixar"/"Remover",
 * "Testar de novo" e o motivo pela frase do catálogo ("Reabrir o SICRO" incluso); nenhum texto fora dos rótulos.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EstadoGpuIa, EstadoIa } from "@domain/ai";
import { defaultAppSettings } from "@domain/app_settings";

const cmd = vi.hoisted(() => ({
  getAiCatalog: vi.fn(),
  getAiStatus: vi.fn(),
  checkAiUpdates: vi.fn(),
  getAppSettings: vi.fn(),
  saveAppSettings: vi.fn(),
  iaEstado: vi.fn(),
  iaGpuInstalar: vi.fn(),
  iaGpuRemover: vi.fn(),
  iaGpuAutoteste: vi.fn(),
  iaRastreadorInstalar: vi.fn(),
  iaRastreadorRemover: vi.fn(),
  iaRedacaoInstalar: vi.fn(),
  iaRedacaoRemover: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { useSettingsStore } from "@stores/settingsStore";
import { AiManagerCard, FRASES_IA } from "./AiManagerCard";
import { desmontarTudo, esperar, montar, type Tela } from "../edicao/inspetor/testeTela";

const gpu = (o: Partial<EstadoGpuIa> = {}): EstadoGpuIa => ({
  disponivel: true,
  placa: { nome: "NVIDIA GeForce RTX 4060 Ti", driver: "610.57.04", compute_cap: "8.9", memoria_mb: 16380 },
  requisito: null,
  instalada: false,
  pronta: false,
  motivo: null,
  mb: 1343,
  ...o,
});
const estado = (o: Partial<EstadoIa> = {}, g: Partial<EstadoGpuIa> = {}): EstadoIa => ({
  ligada: true,
  gpu: gpu(g),
  rastreador: false,
  rastreador_mb: 65,
  redacao: false,
  redacao_mb: 245,
  carregada: null,
  reabrir: false,
  ...o,
});

const linha = (t: Tela, id: string) => {
  const r = t.raiz.querySelector<HTMLElement>(`[data-item="${id}"]`);
  if (!r) throw new Error(`sem a linha ${id}`);
  return r;
};
const botoes = (de: HTMLElement) => [...de.querySelectorAll("button")].map((b) => (b.textContent ?? "").trim());
const botao = (de: HTMLElement, texto: string) => {
  const b = [...de.querySelectorAll("button")].find((x) => (x.textContent ?? "").trim() === texto);
  if (!b) throw new Error(`sem o botão ${texto}: ${botoes(de).join(" | ")}`);
  return b as HTMLButtonElement;
};
const icone = (de: HTMLElement) => de.querySelector("svg[aria-label]")?.getAttribute("aria-label");
const interruptor = (t: Tela) => {
  const l = [...t.raiz.querySelectorAll("label")].find((x) => (x.textContent ?? "").trim() === "Usar modelos de IA");
  if (!l) throw new Error("sem o interruptor");
  return l.querySelector("input") as HTMLInputElement;
};

/** Textos visíveis da linha: só os rótulos de (f), tamanhos e frases do catálogo. */
function textosFora(de: HTMLElement): string[] {
  const permitidos = new Set([
    "Aceleração por GPU (NVIDIA)", "Rastreador", "Baixar", "Baixando…", "Remover", "Testar de novo",
    "bibliotecas da NVIDIA, sob licença própria", ...Object.values(FRASES_IA),
  ]);
  const fora: string[] = [];
  const andar = (n: Node) => {
    if (n.nodeType === 3) {
      const t = (n.textContent ?? "").trim();
      if (t && !permitidos.has(t) && !/^\d+ MB$/.test(t) && !/^\d+%$/.test(t) && !/^\d+$/.test(t) && t !== "MB") fora.push(t);
    }
    n.childNodes.forEach(andar);
  };
  andar(de);
  return fora;
}

let tela: Tela;

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.getAiCatalog.mockResolvedValue({ gpu_detected: true, items: [] });
  cmd.getAiStatus.mockRejectedValue(new Error("sem status"));
  cmd.getAppSettings.mockResolvedValue(defaultAppSettings());
  cmd.saveAppSettings.mockResolvedValue(undefined);
  useSettingsStore.setState({ settings: defaultAppSettings(), loaded: true });
});

afterEach(() => desmontarTudo());

describe("Gerenciador de IA: aceleração por GPU e rastreador", () => {
  it("itens com tamanho, nota da NVIDIA, \"Baixar\" e estado por ícone; baixar a GPU vira \"Testar de novo\"/\"Remover\"", async () => {
    cmd.iaEstado.mockResolvedValue(estado());
    cmd.iaGpuInstalar.mockResolvedValue(estado({}, { instalada: true, pronta: true }));
    tela = await montar(<AiManagerCard />);
    await esperar();
    const g = linha(tela, "gpu");
    const r = linha(tela, "rastreador");
    expect(g.textContent).toContain("Aceleração por GPU (NVIDIA)");
    expect(g.textContent).toContain("1343 MB");
    expect(g.textContent).toContain("bibliotecas da NVIDIA, sob licença própria");
    expect(r.textContent).toContain("Rastreador");
    expect(r.textContent).toContain("65 MB");
    expect(botoes(g)).toEqual(["Baixar"]);
    expect(botoes(r)).toEqual(["Baixar"]);
    expect([icone(g), icone(r)]).toEqual(["não instalado", "não instalado"]);
    expect(interruptor(tela).checked).toBe(true);
    expect([...textosFora(g), ...textosFora(r)]).toEqual([]);
    expect(tela.raiz.querySelectorAll("[placeholder]").length).toBe(0);

    await esperar(async () => botao(g, "Baixar").click());
    expect(cmd.iaGpuInstalar).toHaveBeenCalledTimes(1);
    expect(botoes(linha(tela, "gpu"))).toEqual(["Testar de novo", "Remover"]);
    expect(icone(linha(tela, "gpu"))).toBe("pronto");
  });

  it("\"Testar de novo\" roda o autoteste e mostra o motivo pela frase do catálogo", async () => {
    cmd.iaEstado.mockResolvedValue(estado({}, { instalada: true, pronta: true }));
    cmd.iaGpuAutoteste.mockResolvedValue(estado({}, { instalada: true, pronta: false, motivo: "gpu_sem_cudnn" }));
    tela = await montar(<AiManagerCard />);
    await esperar();
    expect(linha(tela, "gpu").textContent).not.toContain("Falta o cuDNN");
    await esperar(async () => botao(linha(tela, "gpu"), "Testar de novo").click());
    expect(cmd.iaGpuAutoteste).toHaveBeenCalledTimes(1);
    const g = linha(tela, "gpu");
    expect(g.textContent).toContain("Falta o cuDNN");
    expect(icone(g)).toBe("não pronto");
    expect(textosFora(g)).toEqual([]);
  });

  it("\"Reabrir o SICRO\" quando a GPU ficou pronta com o onnxruntime de CPU já carregado", async () => {
    cmd.iaEstado.mockResolvedValue(estado({ reabrir: true, carregada: "cpu" }, { instalada: true, pronta: true }));
    tela = await montar(<AiManagerCard />);
    await esperar();
    expect(linha(tela, "gpu").textContent).toContain("Reabrir o SICRO");
  });

  it("placa sem requisito: frase e \"Baixar\" desligado; erro de comando sai pela frase", async () => {
    cmd.iaEstado.mockResolvedValue(estado({}, { placa: null, requisito: "gpu_sem_placa" }));
    tela = await montar(<AiManagerCard />);
    await esperar();
    const g = linha(tela, "gpu");
    expect(g.textContent).toContain("Nenhuma placa NVIDIA");
    expect(icone(g)).toBe("sem suporte");
    expect(botao(g, "Baixar").disabled).toBe(true);

    desmontarTudo();
    cmd.iaEstado.mockResolvedValue(estado());
    cmd.iaGpuInstalar.mockRejectedValue({ kind: "validation", message: "validation error: gpu_driver_antigo: Driver da NVIDIA antigo" });
    tela = await montar(<AiManagerCard />);
    await esperar();
    await esperar(async () => botao(linha(tela, "gpu"), "Baixar").click());
    expect(tela.raiz.textContent).toContain("Driver da NVIDIA antigo");
    expect(tela.raiz.textContent).not.toContain("validation error");
  });

  it("sem pacote para o sistema: a linha da GPU não aparece; o rastreador baixa e remove", async () => {
    cmd.iaEstado.mockResolvedValue(estado({}, { disponivel: false }));
    cmd.iaRastreadorInstalar.mockResolvedValue(estado({ rastreador: true }, { disponivel: false }));
    cmd.iaRastreadorRemover.mockResolvedValue(estado({}, { disponivel: false }));
    tela = await montar(<AiManagerCard />);
    await esperar();
    expect(tela.raiz.querySelector('[data-item="gpu"]')).toBeNull();
    await esperar(async () => botao(linha(tela, "rastreador"), "Baixar").click());
    expect(icone(linha(tela, "rastreador"))).toBe("pronto");
    await esperar(async () => botao(linha(tela, "rastreador"), "Remover").click());
    expect(cmd.iaRastreadorRemover).toHaveBeenCalledTimes(1);
    expect(botoes(linha(tela, "rastreador"))).toEqual(["Baixar"]);
  });

  it("item \"Ocultação\" (P4.5): tamanho, baixa, fica pronto e remove", async () => {
    cmd.iaEstado.mockResolvedValue(estado());
    cmd.iaRedacaoInstalar.mockResolvedValue(estado({ redacao: true }));
    cmd.iaRedacaoRemover.mockResolvedValue(estado());
    tela = await montar(<AiManagerCard />);
    await esperar();
    const r = () => linha(tela, "redacao");
    expect(r().textContent).toContain("Ocultação");
    expect(r().textContent).toContain("245");
    expect(icone(r())).toBe("não instalado");
    await esperar(async () => botao(r(), "Baixar").click());
    expect(cmd.iaRedacaoInstalar).toHaveBeenCalledTimes(1);
    expect(icone(r())).toBe("pronto");
    await esperar(async () => botao(r(), "Remover").click());
    expect(cmd.iaRedacaoRemover).toHaveBeenCalledTimes(1);
    expect(botoes(r())).toEqual(["Baixar"]);
  });

  it("\"Usar modelos de IA\" grava `modelos_desligados` pelas Configurações e relê o estado", async () => {
    cmd.iaEstado.mockResolvedValue(estado());
    tela = await montar(<AiManagerCard />);
    await esperar();
    expect(cmd.iaEstado).toHaveBeenCalledTimes(1);
    await esperar(async () => interruptor(tela).click());
    expect(cmd.saveAppSettings).toHaveBeenCalledTimes(1);
    expect(cmd.saveAppSettings.mock.calls[0]![0].ai.modelos_desligados).toBe(true);
    expect(useSettingsStore.getState().settings.ai.modelos_desligados).toBe(true);
    expect(interruptor(tela).checked).toBe(false);
    expect(cmd.iaEstado).toHaveBeenCalledTimes(2);
    await esperar(async () => interruptor(tela).click());
    expect(cmd.saveAppSettings.mock.calls[1]![0].ai.modelos_desligados).toBe(false);
  });

  it("as frases da tela são as do catálogo do Rust (`edicao/mensagens.rs`)", () => {
    const fonte = readFileSync(join(process.cwd(), "src-tauri/src/edicao/mensagens.rs"), "utf8");
    const constante = new Map<string, string>();
    for (const m of fonte.matchAll(/pub const ([A-Z0-9_]+): &str = "([a-z0-9_]+)";/g)) constante.set(m[2]!, m[1]!);
    for (const [cod, frase] of Object.entries(FRASES_IA)) {
      const nome = constante.get(cod);
      expect(nome, cod).toBeTruthy();
      const linhaRust = new RegExp(`e\\(${nome}, Erro, "([^"]+)"\\)`).exec(fonte);
      expect(linhaRust?.[1], cod).toBe(frase);
    }
  });
});
