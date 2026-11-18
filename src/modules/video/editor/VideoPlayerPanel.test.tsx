// @vitest-environment happy-dom
/**
 * Reprodutor da análise (P1.10): Ctrl+3 leva a entrada e a saída à edição corrente com a Edição fechada (§8.1), com a
 * proveniência da mídia no `MidiaRef`; as setas são só as de `useGestoTransporte` (as mesmas da Edição, §8.5).
 */

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MidiaInfo, ProjetoEdicao, ProvenienciaMidia } from "@domain/edicao";
import type { VideoMedia } from "@domain/video";

const cmd = vi.hoisted(() => ({
  listVideoMedia: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoQuadroEm: vi.fn(),
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  videoProveniencia: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn(async () => null) }));
vi.mock("./useGestoTransporte", async (original) => {
  const m = await original<typeof import("./useGestoTransporte")>();
  return { ...m, useGestoTransporte: vi.fn(m.useGestoTransporte) };
});

import { VideoPlayerPanel } from "./VideoPlayerPanel";
import { SEGURAR_MS, useGestoTransporte, type AcoesDoGesto } from "./useGestoTransporte";
import { definirCorrente, useEdicaoStore } from "@modules/edicao/store/edicaoStore";
import { midiaDoCaso } from "@modules/edicao/store/coleta";
import { novoProjeto, novoTrecho } from "@modules/edicao/modelo/projeto";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const WS = "/casos/c p1.10";
const SHA = "ab".repeat(32);
const gancho = vi.mocked(useGestoTransporte);
const ganchoReal = (await vi.importActual<typeof import("./useGestoTransporte")>("./useGestoTransporte")).useGestoTransporte;

const video = (id: string, sha256: string, nome: string): VideoMedia =>
  ({
    id,
    occurrence_id: "o1",
    original_path: null,
    relative_path: `Mídias/Vídeos/${nome}`,
    filename: nome,
    sha256,
    size_bytes: 1,
    duration_s: 600,
    codec: "h264",
    width: 1920,
    height: 1080,
    pixel_format: "yuv420p",
    fps_declared: 30,
    avg_frame_rate: "30/1",
    r_frame_rate: "30/1",
    time_base: "1/15360",
    frame_count: 18000,
    bitrate: null,
    raw_probe_json: JSON.stringify({ format: { start_time: "0" }, streams: [{ codec_type: "video" }] }),
    warnings_json: "[]",
    created_at: "",
    updated_at: "",
  }) as VideoMedia;

const info: MidiaInfo = {
  id: "vm1",
  sha256: SHA,
  nome: "cam 1.mp4",
  exibido: { w: 1920, h: 1080 },
  taxa: { num: 30, den: 1 },
  vfr: false,
  passo: { min: 1 / 30, max: 1 / 30 },
  de_s: 0,
  ate_s: 600,
  n: 18000,
  tem_audio: false,
  previa: { relativo: "", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 600, largura: 1920, altura: 1080, pronta: true },
  tabela: "decodificador",
  erros: 0,
  marcavel: true,
  cor: { matriz: "bt709", faixa: "tv" },
} as MidiaInfo;

const prov: ProvenienciaMidia = { recebido_nativo: true, recebido_obs: "Pendrive do DVR", relogio_desvio_s: -3.5, gravador: "Intelbras MHDX" };

/** Edição corrente gravada com um trecho de outra câmera. */
function corrente(): ProjetoEdicao {
  const p = novoProjeto("Corrente");
  const outra = midiaDoCaso(video("vm0", "cd".repeat(32), "cam 0.mp4"), [], null);
  p.midias.push(outra);
  p.principal.push(novoTrecho(outra.id, 1, 2));
  return p;
}

let raiz: Root | null = null;
let div: HTMLDivElement;
type Props = Parameters<typeof VideoPlayerPanel>[0];
const montar = async (extra: Partial<Props> = {}) => {
  div = document.createElement("div");
  document.body.appendChild(div);
  raiz = createRoot(div);
  const props: Props = {
    workspacePath: WS,
    relativePath: "Mídias/Vídeos/cam 1.mp4",
    fps: 30,
    active: true,
    onTimeUpdate: () => {},
    onDurationLoaded: () => {},
    onCollectFrame: () => {},
    registerController: () => {},
    mediaKey: SHA,
    ...extra,
  };
  await act(async () => raiz!.render(<VideoPlayerPanel {...props} />));
  return (mais: Partial<Props>) => act(async () => raiz!.render(<VideoPlayerPanel {...props} {...mais} />));
};
const ctrl3 = () => act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { code: "Digit3", key: "3", ctrlKey: true, bubbles: true, cancelable: true })));
const aviso = () => div.querySelector("[class*=notice]");
const salvo = () => cmd.edicaoSalvar.mock.calls.at(-1)?.[1] as ProjetoEdicao;
/** Espera dentro do `act` (a coleta responde depois do evento). */
const esperar = (f: () => void) => act(async () => void (await vi.waitFor(f)));

beforeEach(() => {
  for (const f of Object.values(cmd)) f.mockReset();
  gancho.mockClear();
  gancho.mockImplementation(ganchoReal);
  useEdicaoStore.setState({ aberta: null, ws: null, doc: null });
  definirCorrente(WS, null);
  cmd.listVideoMedia.mockResolvedValue([video("vm0", "cd".repeat(32), "cam 0.mp4"), video("vm1", SHA, "cam 1.mp4")]);
  cmd.edicaoMidiaInfo.mockResolvedValue(info);
  cmd.edicaoQuadroEm.mockImplementation(async (_ws: string, _s: string, alvo: { t: number }) => ({ indice: Math.round(alvo.t * 30), pts: Math.round(alvo.t * 15360), pts_s: alvo.t, dur_s: 1 / 30 }));
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: "x", raiz_cache: "" }));
  cmd.videoProveniencia.mockResolvedValue(prov);
});
afterEach(() => {
  act(() => raiz?.unmount());
  raiz = null;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

describe("Ctrl+3: trecho para a edição corrente", () => {
  it("com a Edição fechada: acrescenta à corrente, com a proveniência no MidiaRef, e avisa 'Trecho 2'", async () => {
    const p = corrente();
    definirCorrente(WS, p.id);
    cmd.edicaoAbrir.mockResolvedValueOnce({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "" });
    await montar({ trecho: { a: 12, b: 15.5 } });
    await ctrl3();
    await esperar(() => expect(cmd.edicaoSalvar).toHaveBeenCalledTimes(1));
    expect(useEdicaoStore.getState().aberta).toBeNull();
    expect(cmd.edicaoAbrir).toHaveBeenCalledWith(WS, p.id);
    expect(cmd.videoProveniencia).toHaveBeenCalledWith(WS, "vm1");
    const d = salvo();
    expect(d.id).toBe(p.id);
    expect(d.principal).toHaveLength(2);
    expect(d.principal[0]).toEqual(p.principal[0]);
    const m = d.midias.find((x) => x.sha256 === SHA)!;
    expect(d.principal[1]).toMatchObject({ tipo: "trecho", midia: m.id, inicio_s: 12, fim_s: 15.5, inicio_pts: 12 * 15360, fim_pts: 15.5 * 15360 });
    expect(m).toMatchObject({ media_id: "vm1", recebido: { nativo: true, obs: "Pendrive do DVR" }, relogio_desvio_s: -3.5, gravador: "Intelbras MHDX" });
    await esperar(() => expect(aviso()?.textContent).toBe("Trecho 2"));
  });

  it("sem edição corrente: cria uma e ela vira a corrente; sem `trecho`, vale o trecho em repetição", async () => {
    await montar({ loop: { a: 3, b: 4 } });
    await ctrl3();
    await esperar(() => expect(cmd.edicaoSalvar).toHaveBeenCalledTimes(1));
    expect(cmd.edicaoAbrir).not.toHaveBeenCalled();
    expect(salvo().principal).toEqual([expect.objectContaining({ inicio_s: 3, fim_s: 4 })]);
    await esperar(() => expect(aviso()?.textContent).toBe("Trecho 1"));
    // `trecho` vence a repetição
    cmd.edicaoAbrir.mockResolvedValueOnce({ projeto: salvo(), importado_de: null, so_leitura: false, avisos: [], raiz_cache: "" });
    act(() => raiz!.unmount());
    await montar({ loop: { a: 3, b: 4 }, trecho: { a: 7, b: 9 } });
    await ctrl3();
    await esperar(() => expect(cmd.edicaoSalvar).toHaveBeenCalledTimes(2));
    expect(salvo().principal.map((c) => [c.tipo === "trecho" && c.inicio_s, c.tipo === "trecho" && c.fim_s])).toEqual([
      [3, 4],
      [7, 9],
    ]);
  });

  it("sem entrada e saída, ou com a análise escondida: nada é lido nem gravado", async () => {
    const mudar = await montar({ trecho: null, loop: null });
    await ctrl3();
    expect(aviso()?.querySelector("svg")).not.toBeNull();
    expect(aviso()?.textContent).toBe("");
    await mudar({ trecho: { a: 5, b: 5 } });
    await ctrl3();
    await mudar({ trecho: { a: 1, b: 2 }, active: false });
    await ctrl3();
    await act(async () => {});
    expect(cmd.listVideoMedia).not.toHaveBeenCalled();
    expect(cmd.edicaoSalvar).not.toHaveBeenCalled();
  });

  it("Ctrl+3 segurado: a repetição do sistema não acrescenta o trecho de novo", async () => {
    await montar({ trecho: { a: 2, b: 6 } });
    await ctrl3();
    for (let i = 0; i < 5; i++)
      await act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { code: "Digit3", key: "3", ctrlKey: true, repeat: true, bubbles: true, cancelable: true })));
    await esperar(() => expect(aviso()?.textContent).toBe("Trecho 1"));
    await act(async () => {});
    expect(cmd.edicaoSalvar).toHaveBeenCalledTimes(1);
    expect(salvo().principal).toHaveLength(1);
  });

  it("coleta demorada (tabela de quadros): ícone enquanto espera, depois 'Trecho 1'", async () => {
    let solta = () => {};
    cmd.edicaoMidiaInfo.mockImplementationOnce(() => new Promise((ok) => (solta = () => ok(info))));
    await montar({ trecho: { a: 2, b: 6 } });
    expect(aviso()).toBeNull();
    vi.useFakeTimers();
    await ctrl3();
    await esperar(() => expect(cmd.edicaoMidiaInfo).toHaveBeenCalledTimes(1));
    expect(aviso()?.querySelector('[data-estado="coletando"]')).not.toBeNull();
    expect(aviso()?.textContent).toBe("");
    expect(cmd.edicaoSalvar).not.toHaveBeenCalled();
    await act(async () => solta());
    await esperar(() => expect(aviso()?.textContent).toBe("Trecho 1"));
    expect(aviso()?.querySelector('[data-estado="coletando"]')).toBeNull();
    // o aviso passa e o ícone de espera não volta
    await act(async () => void vi.advanceTimersByTime(5000));
    expect(aviso()).toBeNull();
    // falha: o ícone de espera também sai
    cmd.edicaoSalvar.mockRejectedValueOnce({ kind: "validation", message: "validation error: sem_gravacao: Pasta do caso sem gravação" });
    await ctrl3();
    await esperar(() => expect(aviso()?.querySelector('[data-estado="erro"]')).not.toBeNull());
    await act(async () => void vi.advanceTimersByTime(5000));
    expect(aviso()).toBeNull();
  });

  it("falha ao gravar: ícone e a frase do catálogo escrita (o aviso não recebe o ponteiro), sem 'Trecho'", async () => {
    cmd.edicaoSalvar.mockRejectedValue({ kind: "validation", message: "validation error: sem_gravacao: Pasta do caso sem gravação" });
    await montar({ trecho: { a: 1, b: 2 } });
    await ctrl3();
    await esperar(() => expect(aviso()?.textContent).toBe("Pasta do caso sem gravação"));
    expect(aviso()?.querySelector('[data-estado="erro"] svg')).not.toBeNull();
    // código sem frase: vai o código
    cmd.edicaoSalvar.mockRejectedValue({ kind: "validation", message: "validation error: disco_cheio" });
    await ctrl3();
    await esperar(() => expect(aviso()?.textContent).toBe("disco_cheio"));
    // erro fora do catálogo (banco): a mensagem
    cmd.edicaoSalvar.mockRejectedValue({ kind: "database", message: "database error: banco do caso ausente" });
    await ctrl3();
    await esperar(() => expect(aviso()?.textContent).toBe("banco do caso ausente"));
  });
});

describe("←/→ e Shift+←/→: o gesto de useGestoTransporte", () => {
  /** `<video>` com relógio e tocar/pausar observáveis. */
  function preparar() {
    const v = div.querySelector("video")!;
    const estado = { t: 10, pausado: true };
    Object.defineProperty(v, "currentTime", { get: () => estado.t, set: (x: number) => void (estado.t = x), configurable: true });
    Object.defineProperty(v, "duration", { value: 100, configurable: true });
    Object.defineProperty(v, "paused", { get: () => estado.pausado, configurable: true });
    v.play = vi.fn(async () => void (estado.pausado = false));
    v.pause = vi.fn(() => void (estado.pausado = true));
    return { v, estado };
  }
  const tecla = (tipo: "keydown" | "keyup", key: string, mods: Partial<KeyboardEventInit> = {}) =>
    act(async () => void window.dispatchEvent(new KeyboardEvent(tipo, { key, code: key, bubbles: true, cancelable: true, ...mods })));
  const ultimasAcoes = () => gancho.mock.calls.at(-1)![0] as AcoesDoGesto;

  it("liga o gancho com `active`; com o gancho desligado as setas não fazem nada (nenhum outro ouvinte)", async () => {
    const mudar = await montar();
    expect(gancho.mock.calls.at(-1)![1]).toBe(true);
    await mudar({ active: false });
    expect(gancho.mock.calls.at(-1)![1]).toBe(false);
    // mesmo reprodutor com o gancho mudo (montado de novo: a ordem dos hooks não muda num componente vivo)
    act(() => raiz!.unmount());
    gancho.mockImplementation(() => {});
    await montar();
    const { v, estado } = preparar();
    vi.useFakeTimers();
    await tecla("keydown", "ArrowRight");
    await act(async () => vi.advanceTimersByTime(2 * SEGURAR_MS));
    await tecla("keyup", "ArrowRight");
    await tecla("keydown", "ArrowLeft", { shiftKey: true });
    await tecla("keydown", "ArrowLeft");
    await tecla("keyup", "ArrowLeft");
    expect(estado.t).toBe(10);
    expect(v.play).not.toHaveBeenCalled();
    expect(v.pause).not.toHaveBeenCalled();
  });

  it("pelas teclas: toque = um quadro, Shift = ±1 s, segurar → toca e soltar pausa", async () => {
    await montar();
    const { v, estado } = preparar();
    vi.useFakeTimers();
    await tecla("keydown", "ArrowRight");
    await tecla("keyup", "ArrowRight");
    expect(estado.t).toBeCloseTo(10 + 1 / 30, 12);
    await tecla("keydown", "ArrowLeft");
    await tecla("keyup", "ArrowLeft");
    expect(estado.t).toBeCloseTo(10, 12);
    await tecla("keydown", "ArrowRight", { shiftKey: true });
    expect(estado.t).toBeCloseTo(11, 12);
    await tecla("keydown", "ArrowLeft", { shiftKey: true });
    await tecla("keydown", "ArrowLeft", { shiftKey: true, repeat: true });
    expect(estado.t).toBeCloseTo(9, 12);
    await tecla("keydown", "ArrowRight");
    await act(async () => vi.advanceTimersByTime(SEGURAR_MS));
    expect(v.play).toHaveBeenCalledTimes(1);
    expect(estado.pausado).toBe(false);
    await tecla("keyup", "ArrowRight");
    expect(estado.pausado).toBe(true);
    expect(estado.t).toBeCloseTo(9, 12);
  });

  it("as ações que o reprodutor entrega ao gancho: passo, saltar, mover (frente e ré) e parar", async () => {
    const raf = vi.spyOn(window, "requestAnimationFrame").mockImplementation(() => 7);
    const caf = vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {});
    await montar();
    const { v, estado } = preparar();
    const a = ultimasAcoes();
    act(() => a.passo(-1));
    expect(estado.t).toBeCloseTo(10 - 1 / 30, 12);
    act(() => a.saltar(1));
    expect(estado.t).toBeCloseTo(11 - 1 / 30, 12);
    await act(async () => a.mover(1));
    expect(v.play).toHaveBeenCalledTimes(1);
    act(() => a.parar());
    expect(estado.pausado).toBe(true);
    act(() => a.mover(-1));
    expect(raf).toHaveBeenCalled();
    expect(v.play).toHaveBeenCalledTimes(1);
    act(() => a.parar());
    expect(caf).toHaveBeenCalledWith(7);
    raf.mockRestore();
    caf.mockRestore();
  });
});

describe("volume do <video> (o PipeWire guarda o do aplicativo)", () => {
  it("parado: volume 1 e sem mudo; tocando: o escolhido; desmontado: 1", async () => {
    vi.stubGlobal("localStorage", { getItem: (k: string) => (k === "sicro.video.audio.v1" ? JSON.stringify({ volume: 0.3, muted: true }) : null), setItem: () => {} });
    await montar();
    await esperar(() => expect(div.querySelector("video")).toBeTruthy());
    const v = div.querySelector("video")!;
    expect([v.volume, v.muted]).toEqual([1, false]);
    let parado = false;
    Object.defineProperty(v, "paused", { get: () => parado, configurable: true });
    await act(async () => void v.dispatchEvent(new Event("play")));
    expect([v.volume, v.muted]).toEqual([0.3, true]);
    parado = true;
    await act(async () => void v.dispatchEvent(new Event("pause")));
    expect([v.volume, v.muted]).toEqual([1, false]);
    parado = false;
    await act(async () => void v.dispatchEvent(new Event("play")));
    act(() => raiz!.unmount());
    raiz = null;
    expect([v.volume, v.muted]).toEqual([1, false]);
    vi.unstubAllGlobals();
  });
});
