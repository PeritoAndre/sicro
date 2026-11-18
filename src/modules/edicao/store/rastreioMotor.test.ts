// @vitest-environment happy-dom
/**
 * "Motor" do rastreio (contratos-fase4.md D12): a escolha do painel vai no `PedidoSeguir` (Seguir, Seguir para trás,
 * Retomar, atalhos e o palco passam todos por `pedidoDe`), fica guardada no navegador (`sicro.edicao.motor.v1`), e item
 * não instalado ou com a IA desligada cai no automático (o Rust recusaria o pedido explícito).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjetoAberto } from "@domain/edicao";
import type { AiSettings } from "@domain/app_settings";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoRastreioSeguir: vi.fn(),
  edicaoRastreioCancelar: vi.fn(),
  edicaoRastreioLer: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { useSettingsStore } from "@stores/settingsStore";
import { useEdicaoStore } from "./edicaoStore";
import { useRecursos } from "../palco/recursos";
import {
  alvoDoEfeito,
  CHAVE_MOTOR,
  esquecerRastreios,
  estadoDoMotor,
  lerMotor,
  MOTORES,
  motorDoPedido,
  pedidoDe,
  seguirEfeito,
  useRastreioStore,
} from "./rastreioStore";
import { info, projeto, q, SHA, tab, WS } from "../rastreio/cenario.teste";

const st = () => useEdicaoStore.getState();
const ai = (o: Partial<AiSettings>) => useSettingsStore.setState((s) => ({ settings: { ...s.settings, ai: { ...s.settings.ai, ...o } } }));
const SEM_NADA: Partial<AiSettings> = { modelos_desligados: false, rastreio_dir: "", ort_lib_path: "", sr_ort_lib_path: "", gpu_dir: "" };
const TUDO: Partial<AiSettings> = { modelos_desligados: false, rastreio_dir: "/ai/rastreio", ort_lib_path: "/ai/ort/libonnxruntime.so", sr_ort_lib_path: "", gpu_dir: "" };
const motorDoComando = (i = 0) => cmd.edicaoRastreioSeguir.mock.calls[i]![7];
/** O Node 25 põe um `localStorage` vazio por cima do happy-dom: um de mentira, por teste. */
let guardado: Map<string, string>;
const armazem = () => ({
  getItem: (k: string) => guardado.get(k) ?? null,
  setItem: (k: string, v: string) => void guardado.set(k, String(v)),
  removeItem: (k: string) => void guardado.delete(k),
});

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoRastreioSeguir.mockResolvedValue("t1");
  guardado = new Map();
  vi.stubGlobal("localStorage", armazem());
  esquecerRastreios();
  ai(SEM_NADA);
  const p = projeto();
  cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/r" } satisfies ProjetoAberto);
  await st().abrir(WS, p.id);
  useRecursos.setState((s) => ({ midias: { [SHA]: { info, tabela: tab, erro: null, pedindo: false } }, versao: s.versao + 1 }));
  st().posicionar({ modo: "clipe", clipe: "c", t: q(30).t });
});

afterEach(async () => {
  vi.unstubAllGlobals();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

describe("motor no PedidoSeguir", () => {
  it("padrão automático; a escolha vai no pedido e no comando", async () => {
    ai(TUDO);
    expect(useRastreioStore.getState().motor).toBe("auto");
    await seguirEfeito("e1", "c", "frente");
    expect(motorDoComando(0)).toBe("auto");
    await useRastreioStore.getState().parar();
    useRastreioStore.setState({ corrida: null });
    for (const [m] of MOTORES) {
      useRastreioStore.getState().escolherMotor(m);
      expect(pedidoDe(alvoDoEfeito(st().doc!, "e1", "c")!, "frente", "seguir", {})!.motor).toBe(m);
    }
    useRastreioStore.getState().escolherMotor("vittrack");
    cmd.edicaoRastreioSeguir.mockClear();
    await seguirEfeito("e1", "c", "frente");
    expect(motorDoComando(0)).toBe("vittrack");
    useRastreioStore.setState({ corrida: null });
    useRastreioStore.getState().escolherMotor("mancha");
    await seguirEfeito("e1", "c", "frente");
    expect(motorDoComando(1)).toBe("mancha");
  });

  it("não instalado: EdgeTAM sem o item, VitTrack sem onnxruntime → o pedido sai automático", async () => {
    ai(SEM_NADA);
    useRastreioStore.getState().escolherMotor("edgetam");
    await seguirEfeito("e1", "c", "frente");
    expect(motorDoComando(0)).toBe("auto");
    useRastreioStore.setState({ corrida: null });
    useRastreioStore.getState().escolherMotor("vittrack");
    await seguirEfeito("e1", "c", "frente");
    expect(motorDoComando(1)).toBe("auto");
    // a escolha fica guardada: instalado depois, volta a valer
    expect(useRastreioStore.getState().motor).toBe("vittrack");
    ai({ sr_ort_lib_path: "/ai/sr/libonnxruntime.so" });
    expect(motorDoPedido()).toBe("vittrack");
  });

  it("IA desligada: os neurais caem no automático; a mancha segue", () => {
    ai({ ...TUDO, modelos_desligados: true });
    expect(motorDoPedido("edgetam")).toBe("auto");
    expect(motorDoPedido("vittrack")).toBe("auto");
    expect(motorDoPedido("mancha")).toBe("mancha");
    expect(motorDoPedido("auto")).toBe("auto");
  });
});

describe("estado de cada motor pelas Configurações", () => {
  it("automático e mancha sempre; EdgeTAM pelo item Rastreador; VitTrack por qualquer onnxruntime", () => {
    const base = { modelos_desligados: false, rastreio_dir: "", ort_lib_path: "", sr_ort_lib_path: "", gpu_dir: "" };
    expect(MOTORES.map(([m]) => estadoDoMotor(m, base))).toEqual(["pronto", "ausente", "ausente", "pronto"]);
    expect(estadoDoMotor("edgetam", { ...base, rastreio_dir: "/r" })).toBe("pronto");
    for (const lib of ["ort_lib_path", "sr_ort_lib_path", "gpu_dir"] as const) expect(estadoDoMotor("vittrack", { ...base, [lib]: "/x" })).toBe("pronto");
    expect(MOTORES.map(([m]) => estadoDoMotor(m, { ...base, rastreio_dir: "/r", ort_lib_path: "/o", modelos_desligados: true }))).toEqual([
      "pronto",
      "desligada",
      "desligada",
      "pronto",
    ]);
  });
});

describe("escolha guardada no navegador", () => {
  it("grava em sicro.edicao.motor.v1 e lê de volta; valor estranho ou ausente = automático", () => {
    expect(CHAVE_MOTOR).toBe("sicro.edicao.motor.v1");
    expect(lerMotor()).toBe("auto");
    useRastreioStore.getState().escolherMotor("edgetam");
    expect(localStorage.getItem(CHAVE_MOTOR)).toBe("edgetam");
    expect(lerMotor()).toBe("edgetam");
    localStorage.setItem(CHAVE_MOTOR, "yolo");
    expect(lerMotor()).toBe("auto");
    localStorage.setItem(CHAVE_MOTOR, '"vittrack"');
    expect(lerMotor()).toBe("auto");
  });

  it("navegador sem armazenamento: a escolha vale na sessão e nada quebra", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("bloqueado");
      },
      setItem: () => {
        throw new Error("cheio");
      },
    });
    useRastreioStore.getState().escolherMotor("mancha");
    expect(useRastreioStore.getState().motor).toBe("mancha");
    expect(lerMotor()).toBe("auto");
    vi.stubGlobal("localStorage", undefined);
    expect(lerMotor()).toBe("auto");
    useRastreioStore.getState().escolherMotor("vittrack");
    expect(useRastreioStore.getState().motor).toBe("vittrack");
  });
});
