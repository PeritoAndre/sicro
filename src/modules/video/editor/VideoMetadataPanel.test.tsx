// @vitest-environment happy-dom
/**
 * Propriedades da mídia na análise (§8.6 Mídia, F6): "Formato do gravador", "Observação", "Desvio do relógio (s)" e
 * "Gravador" gravam pelo comando e chegam ao `MidiaRef` da coleta. Os campos mostram só o que o banco tem: leitura
 * que falhou desliga, gravação que falhou volta ao gravado.
 */

import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MidiaInfo, ProjetoEdicao, ProvenienciaMidia } from "@domain/edicao";
import type { VideoMedia } from "@domain/video";

const cmd = vi.hoisted(() => ({
  videoProveniencia: vi.fn(),
  videoProvenienciaSalvar: vi.fn(),
  listVideoMedia: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoQuadroEm: vi.fn(),
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { VideoMetadataPanel } from "./VideoMetadataPanel";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { receber } from "@modules/edicao/store/coleta";
import { definirCorrente } from "@modules/edicao/store/edicaoStore";
import { clicar, desmontarTudo, digitar, escolher, montar, sair, tecla, textosVisiveis } from "@modules/edicao/inspetor/testeTela";

const WS = "/casos/c meta";
const SHA = "12".repeat(32);
const ROTULOS_MIDIA = ["Formato do gravador", "Observação", "Desvio do relógio (s)", "Gravador"];
const vazio: ProvenienciaMidia = { recebido_nativo: null, recebido_obs: "", relogio_desvio_s: null, gravador: null };

const media = (id: string, sha256 = SHA) =>
  ({
    id,
    occurrence_id: "o1",
    relative_path: `Mídias/Vídeos/${id}.mp4`,
    filename: `${id}.mp4`,
    sha256,
    size_bytes: 2048,
    duration_s: 60,
    codec: "h264",
    width: 1280,
    height: 720,
    pixel_format: "yuv420p",
    fps_declared: 25,
    avg_frame_rate: "25/1",
    r_frame_rate: "25/1",
    time_base: "1/1000",
    frame_count: 1500,
    bitrate: null,
    raw_probe_json: "{}",
    warnings_json: "[]",
    derived_from_hash: null,
    derivation_json: null,
  }) as unknown as VideoMedia;

/** Banco falso com a regra do Rust: texto aparado, gravador vazio = nulo. */
let banco: Map<string, ProvenienciaMidia>;
const backend = () => {
  cmd.videoProveniencia.mockImplementation(async (_ws: string, id: string) => ({ ...(banco.get(id) ?? vazio) }));
  cmd.videoProvenienciaSalvar.mockImplementation(async (_ws: string, id: string, p: ProvenienciaMidia) => {
    const g = p.gravador?.trim() ?? "";
    const n = { recebido_nativo: p.recebido_nativo, recebido_obs: p.recebido_obs.trim(), relogio_desvio_s: p.relogio_desvio_s, gravador: g === "" ? null : g };
    banco.set(id, n);
    return { ...n };
  });
};

let raiz: HTMLElement;
const abrir = async (id = "vm1") => {
  const t = await montar(<VideoMetadataPanel media={media(id)} warnings={[]} />);
  raiz = t.raiz;
  await act(async () => {});
  return t;
};
/** Entrada pelo rótulo visível (`label for`). */
const entrada = <T extends HTMLElement = HTMLInputElement>(rotulo: string): T => {
  const l = [...raiz.querySelectorAll("label")].find((x) => x.textContent === rotulo);
  if (!l) throw new Error(`sem rótulo ${rotulo}`);
  return document.getElementById(l.htmlFor) as T;
};
const estado = (e: string) => raiz.querySelector(`[data-estado="${e}"]`);
const gravacoes = () => cmd.videoProvenienciaSalvar.mock.calls.map((c) => c[2] as ProvenienciaMidia);

beforeEach(() => {
  for (const f of Object.values(cmd)) f.mockReset();
  banco = new Map();
  backend();
  useWorkspaceStore.setState({ activeWorkspacePath: WS });
});
afterEach(() => desmontarTudo());

describe("VideoMetadataPanel: proveniência", () => {
  it("mostra o gravado, só com os rótulos do §8.6 Mídia (sem 'Cor') e sem texto de exemplo", async () => {
    banco.set("vm1", { recebido_nativo: true, recebido_obs: "Pendrive", relogio_desvio_s: -3.5, gravador: "Dahua XVR" });
    await abrir();
    expect(cmd.videoProveniencia).toHaveBeenCalledWith(WS, "vm1");
    expect(entrada<HTMLSelectElement>("Formato do gravador").value).toBe("sim");
    expect(entrada("Observação").value).toBe("Pendrive");
    expect(entrada("Desvio do relógio (s)").value).toBe("-3,5");
    expect(entrada("Gravador").value).toBe("Dahua XVR");
    const sec = raiz.querySelector<HTMLElement>("[data-proveniencia]")!;
    expect(textosVisiveis(sec)).toEqual([...ROTULOS_MIDIA.slice(0, 1), "Sim", "Não", ...ROTULOS_MIDIA.slice(1)]);
    expect(sec.querySelector("[placeholder]")).toBeNull();
    // o campo não aceita mais do que o backend grava (senão a recusa devolveria o campo ao valor antigo)
    expect([entrada("Observação").maxLength, entrada("Gravador").maxLength]).toEqual([4000, 200]);
    expect([...sec.querySelectorAll("option")].map((o) => o.textContent)).toEqual(["", "Sim", "Não"]);
    expect(estado("erro-leitura")).toBeNull();
  });

  it("os quatro campos gravam pelo comando e chegam ao MidiaRef da coleta", async () => {
    await abrir();
    expect(entrada<HTMLSelectElement>("Formato do gravador").value).toBe("");
    await escolher(entrada<HTMLSelectElement>("Formato do gravador"), "nao");
    await digitar(entrada("Observação"), " Cópia por WhatsApp ");
    await sair(entrada("Observação"));
    await digitar(entrada("Desvio do relógio (s)"), "−12,25");
    await tecla(entrada("Desvio do relógio (s)"), "Enter");
    await digitar(entrada("Gravador"), "  Intelbras MHDX 1016 ");
    await sair(entrada("Gravador"));
    await act(async () => {});
    expect(gravacoes()).toEqual([
      { ...vazio, recebido_nativo: false },
      { ...vazio, recebido_nativo: false, recebido_obs: " Cópia por WhatsApp " },
      { ...vazio, recebido_nativo: false, recebido_obs: "Cópia por WhatsApp", relogio_desvio_s: -12.25 },
      { recebido_nativo: false, recebido_obs: "Cópia por WhatsApp", relogio_desvio_s: -12.25, gravador: "  Intelbras MHDX 1016 " },
    ]);
    expect(cmd.videoProvenienciaSalvar.mock.calls.every((c) => c[0] === WS && c[1] === "vm1")).toBe(true);
    // a tela mostra o que ficou gravado
    expect(entrada("Observação").value).toBe("Cópia por WhatsApp");
    expect(entrada("Gravador").value).toBe("Intelbras MHDX 1016");
    expect(entrada("Desvio do relógio (s)").value).toBe("-12,25");
    expect(estado("gravada")).not.toBeNull();

    // a coleta (Ctrl+3, "Trecho…") lê o mesmo banco
    definirCorrente(WS, null);
    cmd.listVideoMedia.mockResolvedValue([media("vm1")]);
    cmd.edicaoMidiaInfo.mockResolvedValue({ exibido: { w: 1280, h: 720 }, taxa: { num: 25, den: 1 }, vfr: false, tem_audio: false, de_s: 0, ate_s: 60 } as unknown as MidiaInfo);
    cmd.edicaoQuadroEm.mockImplementation(async (_w: string, _s: string, a: { t: number }) => ({ indice: a.t * 25, pts: a.t * 1000, pts_s: a.t, dur_s: 0.04 }));
    cmd.edicaoSalvar.mockResolvedValue({ updated_at: "x", raiz_cache: "" });
    await receber(WS, [{ sha256: SHA, inicio_s: 1, fim_s: 3 }]);
    const m = (cmd.edicaoSalvar.mock.calls[0]![1] as ProjetoEdicao).midias[0]!;
    expect(m).toMatchObject({ media_id: "vm1", recebido: { nativo: false, obs: "Cópia por WhatsApp" }, relogio_desvio_s: -12.25, gravador: "Intelbras MHDX 1016" });
  });

  it("desvio que não é número volta ao gravado sem gravar; vazio grava 'não informado'; Esc desfaz o rascunho", async () => {
    banco.set("vm1", { ...vazio, relogio_desvio_s: 2 });
    await abrir();
    const d = entrada("Desvio do relógio (s)");
    await digitar(d, "dois");
    await sair(d);
    expect(d.value).toBe("2");
    await digitar(d, "9");
    await tecla(d, "Escape");
    await sair(d);
    expect(d.value).toBe("2");
    expect(cmd.videoProvenienciaSalvar).not.toHaveBeenCalled();
    await digitar(d, "");
    await sair(d);
    await act(async () => {});
    expect(gravacoes()).toEqual([vazio]);
    expect(banco.get("vm1")).toEqual(vazio);
    // sair sem mudar não grava
    await sair(entrada("Observação"));
    await escolher(entrada<HTMLSelectElement>("Formato do gravador"), "");
    await act(async () => {});
    expect(cmd.videoProvenienciaSalvar).toHaveBeenCalledTimes(1);
  });

  it("leitura que falha: campos desligados e vazios, ícone de erro, nada grava; 'Testar de novo' relê", async () => {
    banco.set("vm1", { recebido_nativo: true, recebido_obs: "x", relogio_desvio_s: 1, gravador: "g" });
    cmd.videoProveniencia.mockRejectedValueOnce({ kind: "database", message: "database error: banco do caso ausente" });
    await abrir();
    expect(estado("erro-leitura")?.getAttribute("title")).toBe("banco do caso ausente");
    for (const r of ROTULOS_MIDIA) {
      const e = entrada<HTMLInputElement | HTMLSelectElement>(r);
      expect(e.disabled, r).toBe(true);
      expect(e.value, r).toBe("");
    }
    // desligados também na aparência
    const grade = () => raiz.querySelector<HTMLElement>("[data-proveniencia] dl")!.style.opacity;
    expect(grade()).toBe("0.5");
    await digitar(entrada("Observação"), "y");
    await sair(entrada("Observação"));
    await escolher(entrada<HTMLSelectElement>("Formato do gravador"), "nao");
    await act(async () => {});
    expect(cmd.videoProvenienciaSalvar).not.toHaveBeenCalled();
    await clicar(raiz.querySelector<HTMLButtonElement>('button[aria-label="Testar de novo"]')!);
    await act(async () => {});
    expect(grade()).toBe("1");
    expect(estado("erro-leitura")).toBeNull();
    expect(entrada<HTMLSelectElement>("Formato do gravador").value).toBe("sim");
    expect(entrada<HTMLInputElement>("Gravador").disabled).toBe(false);
    expect(entrada("Gravador").value).toBe("g");
  });

  it("gravação que falha: ícone com a frase e o campo volta ao gravado", async () => {
    banco.set("vm1", { ...vazio, gravador: "Antigo" });
    await abrir();
    cmd.videoProvenienciaSalvar.mockRejectedValueOnce({ kind: "validation", message: "validation error: item_invalido: Item inválido: gravador" });
    await digitar(entrada("Gravador"), "Novo");
    await sair(entrada("Gravador"));
    await act(async () => {});
    expect(estado("erro-gravacao")?.getAttribute("title")).toBe("Item inválido: gravador");
    expect(entrada("Gravador").value).toBe("Antigo");
    expect(banco.get("vm1")?.gravador).toBe("Antigo");
    // código sem frase: a dica leva o código
    cmd.videoProvenienciaSalvar.mockRejectedValueOnce({ kind: "validation", message: "validation error: midia_ausente" });
    await digitar(entrada("Gravador"), "Outro");
    await sair(entrada("Gravador"));
    await act(async () => {});
    expect(estado("erro-gravacao")?.getAttribute("title")).toBe("midia_ausente");
    expect(entrada("Gravador").value).toBe("Antigo");
  });

  it("uma gravação por vez, a seguinte parte da anterior; trocar de mídia relê e ignora a resposta velha", async () => {
    await abrir();
    let solta = () => {};
    const salvar = cmd.videoProvenienciaSalvar.getMockImplementation()!;
    cmd.videoProvenienciaSalvar.mockImplementationOnce((...a: [string, string, ProvenienciaMidia]) => new Promise((ok) => (solta = () => ok(salvar(...a)))));
    await escolher(entrada<HTMLSelectElement>("Formato do gravador"), "sim");
    await digitar(entrada("Gravador"), "G1");
    await sair(entrada("Gravador"));
    expect(cmd.videoProvenienciaSalvar).toHaveBeenCalledTimes(1);
    expect(estado("gravando")).not.toBeNull();
    await act(async () => solta());
    await act(async () => {});
    expect(gravacoes()).toEqual([
      { ...vazio, recebido_nativo: true },
      { ...vazio, recebido_nativo: true, gravador: "G1" },
    ]);

    // outra mídia no mesmo painel: leitura de vm2 atrasada; volta para vm1 antes dela chegar
    banco.set("vm2", { ...vazio, gravador: "da vm2" });
    const div = document.createElement("div");
    document.body.appendChild(div);
    const r = createRoot(div);
    raiz = div;
    await act(async () => r.render(<VideoMetadataPanel media={media("vm1")} warnings={[]} />));
    expect(entrada("Gravador").value).toBe("G1");
    let leVm2 = () => {};
    cmd.videoProveniencia.mockImplementationOnce((_w: string, id: string) => new Promise((ok) => (leVm2 = () => ok({ ...(banco.get(id) ?? vazio) }))));
    await act(async () => r.render(<VideoMetadataPanel media={media("vm2")} warnings={[]} />));
    expect(div.querySelector("[data-proveniencia]")?.getAttribute("data-proveniencia")).toBe("lendo");
    expect(entrada<HTMLInputElement>("Gravador").disabled).toBe(true);
    await act(async () => r.render(<VideoMetadataPanel media={media("vm1")} warnings={[]} />));
    await act(async () => leVm2());
    expect(entrada("Gravador").value).toBe("G1");
    await act(async () => r.render(<VideoMetadataPanel media={media("vm2")} warnings={[]} />));
    expect(entrada("Gravador").value).toBe("da vm2");
    act(() => r.unmount());
  });
  it("troca de mídia com gravação em curso: nada da mídia velha chega à nova", async () => {
    banco.set("vm2", { ...vazio, gravador: "da vm2" });
    const div = document.createElement("div");
    document.body.appendChild(div);
    const r = createRoot(div);
    raiz = div;
    const mostrar = (id: string) => act(async () => r.render(<VideoMetadataPanel media={media(id)} warnings={[]} />));
    await mostrar("vm1");
    let solta = () => {};
    const salvar = cmd.videoProvenienciaSalvar.getMockImplementation()!;
    cmd.videoProvenienciaSalvar.mockImplementationOnce((...a: [string, string, ProvenienciaMidia]) => new Promise((ok) => (solta = () => ok(salvar(...a)))));
    // uma gravação em curso e outra na fila, as duas da vm1
    await escolher(entrada<HTMLSelectElement>("Formato do gravador"), "sim");
    await digitar(entrada("Gravador"), "G1");
    await sair(entrada("Gravador"));
    expect(cmd.videoProvenienciaSalvar).toHaveBeenCalledTimes(1);
    await mostrar("vm2");
    expect(entrada("Gravador").value).toBe("da vm2");
    await act(async () => solta());
    await act(async () => {});
    // a em curso valeu para a vm1; a da fila não sai (partiria do gravado da vm2)
    expect(cmd.videoProvenienciaSalvar).toHaveBeenCalledTimes(1);
    expect(banco.get("vm1")).toEqual({ ...vazio, recebido_nativo: true });
    expect(banco.get("vm2")).toEqual({ ...vazio, gravador: "da vm2" });
    expect(entrada<HTMLSelectElement>("Formato do gravador").value).toBe("");
    expect(entrada("Gravador").value).toBe("da vm2");
    expect(estado("gravada")).toBeNull();
    // editar a vm2 agora parte do gravado dela
    await digitar(entrada("Observação"), "obs da vm2");
    await sair(entrada("Observação"));
    await act(async () => {});
    expect(banco.get("vm2")).toEqual({ ...vazio, recebido_obs: "obs da vm2", gravador: "da vm2" });
    expect(banco.get("vm1")).toEqual({ ...vazio, recebido_nativo: true });

    // gravação que falha depois da troca: o erro não aparece na outra mídia
    let falha = () => {};
    cmd.videoProvenienciaSalvar.mockImplementationOnce(() => new Promise((_ok, no) => (falha = () => no({ kind: "validation", message: "validation error: midia_ausente" }))));
    await digitar(entrada("Gravador"), "X");
    await sair(entrada("Gravador"));
    await mostrar("vm1");
    await act(async () => falha());
    await act(async () => {});
    expect(estado("erro-gravacao")).toBeNull();
    expect(entrada<HTMLSelectElement>("Formato do gravador").value).toBe("sim");

    // leitura que falha depois da troca: idem
    cmd.videoProveniencia.mockImplementationOnce(() => new Promise((_ok, no) => (falha = () => no({ kind: "database", message: "database error: banco do caso ausente" }))));
    await mostrar("vm2");
    await mostrar("vm1");
    await act(async () => falha());
    await act(async () => {});
    expect(estado("erro-leitura")).toBeNull();
    expect(entrada<HTMLSelectElement>("Formato do gravador").disabled).toBe(false);
    act(() => r.unmount());
  });

  it("leitura fora do formato: erro e campos desligados, não 'não informado'", async () => {
    cmd.videoProveniencia.mockResolvedValueOnce({ recebido_nativo: null, recebido_obs: "", relogio_desvio_s: null });
    await abrir();
    expect(estado("erro-leitura")).not.toBeNull();
    for (const r of ROTULOS_MIDIA) expect(entrada<HTMLInputElement | HTMLSelectElement>(r).disabled, r).toBe(true);
  });

  it("resposta de uma gravação não apaga o que está sendo digitado em outro campo", async () => {
    await abrir();
    let solta = () => {};
    const salvar = cmd.videoProvenienciaSalvar.getMockImplementation()!;
    cmd.videoProvenienciaSalvar.mockImplementationOnce((...a: [string, string, ProvenienciaMidia]) => new Promise((ok) => (solta = () => ok(salvar(...a)))));
    await digitar(entrada("Observação"), "Pendrive");
    await sair(entrada("Observação"));
    // a gravação da observação ainda não voltou e o perito já digita no campo seguinte
    await digitar(entrada("Gravador"), "Dahua");
    await digitar(entrada("Desvio do relógio (s)"), "4,5");
    await act(async () => solta());
    await act(async () => {});
    expect(entrada("Observação").value).toBe("Pendrive");
    expect(entrada("Gravador").value).toBe("Dahua");
    expect(entrada("Desvio do relógio (s)").value).toBe("4,5");
    await sair(entrada("Gravador"));
    await sair(entrada("Desvio do relógio (s)"));
    await act(async () => {});
    expect(banco.get("vm1")).toEqual({ recebido_nativo: null, recebido_obs: "Pendrive", relogio_desvio_s: 4.5, gravador: "Dahua" });
  });
});
