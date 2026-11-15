/**
 * Proveniência da mídia até o `MidiaRef` (F6, B18): o que o backend devolve vira `recebido`, `relogio_desvio_s` e
 * `gravador`; nada gravado = null; erro de leitura rejeita (nunca "não informado").
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MidiaInfo, ProjetoEdicao, ProvenienciaMidia } from "@domain/edicao";
import type { VideoMedia } from "@domain/video";

const cmd = vi.hoisted(() => ({
  videoProveniencia: vi.fn(),
  listVideoMedia: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoQuadroEm: vi.fn(),
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { conferirProveniencia, desvioDoTexto, MAX_GRAVADOR, MAX_OBS, provenienciaDaMidia, textoDoDesvio } from "./proveniencia";
import { receber } from "@modules/edicao/store/coleta";
import { definirCorrente } from "@modules/edicao/store/edicaoStore";

const WS = "/casos/prov";
const SHA = "ef".repeat(32);
const p = (recebido_nativo: boolean | null, recebido_obs: string, relogio_desvio_s: number | null, gravador: string | null): ProvenienciaMidia => ({
  recebido_nativo,
  recebido_obs,
  relogio_desvio_s,
  gravador,
});

beforeEach(() => {
  for (const f of Object.values(cmd)) f.mockReset();
});

describe("provenienciaDaMidia", () => {
  it("leva os quatro campos ao formato do MidiaRef", async () => {
    cmd.videoProveniencia.mockResolvedValueOnce(p(false, "Cópia por WhatsApp", 3725.5, "Hikvision DS-7208"));
    expect(await provenienciaDaMidia(WS, "vm1")).toEqual({ recebido: { nativo: false, obs: "Cópia por WhatsApp" }, relogio_desvio_s: 3725.5, gravador: "Hikvision DS-7208" });
    expect(cmd.videoProveniencia).toHaveBeenCalledWith(WS, "vm1");
    cmd.videoProveniencia.mockResolvedValueOnce(p(null, "", null, "Dahua"));
    expect(await provenienciaDaMidia(WS, "vm1")).toEqual({ recebido: { nativo: null, obs: "" }, relogio_desvio_s: null, gravador: "Dahua" });
    cmd.videoProveniencia.mockResolvedValueOnce(p(null, "", -0.04, null));
    expect((await provenienciaDaMidia(WS, "vm1"))?.relogio_desvio_s).toBe(-0.04);
    // gravador vazio é "não informado" também quando outro campo veio
    cmd.videoProveniencia.mockResolvedValueOnce(p(true, "", null, ""));
    expect(await provenienciaDaMidia(WS, "vm1")).toEqual({ recebido: { nativo: true, obs: "" }, relogio_desvio_s: null, gravador: null });
  });

  it("nada informado: null", async () => {
    cmd.videoProveniencia.mockResolvedValueOnce(p(null, "", null, null));
    expect(await provenienciaDaMidia(WS, "vm1")).toBeNull();
    cmd.videoProveniencia.mockResolvedValueOnce(p(null, "", null, ""));
    expect(await provenienciaDaMidia(WS, "vm1")).toBeNull();
  });

  it("erro de leitura rejeita com o erro do backend", async () => {
    const erro = { kind: "database", message: "database error: banco do caso ausente" };
    cmd.videoProveniencia.mockRejectedValueOnce(erro);
    await expect(provenienciaDaMidia(WS, "vm1")).rejects.toBe(erro);
    cmd.videoProveniencia.mockRejectedValueOnce({ kind: "validation", message: "validation error: midia_ausente: Mídia ausente" });
    await expect(provenienciaDaMidia(WS, "vm1")).rejects.toMatchObject({ message: expect.stringContaining("midia_ausente") });
  });

  it("resposta fora do formato rejeita (nenhum undefined chega ao MidiaRef)", async () => {
    for (const ruim of [[], null, {}, { ...p(null, "", null, null), recebido_nativo: 1 }, { ...p(null, "", null, null), recebido_obs: null }, { ...p(null, "", null, null), relogio_desvio_s: "3" }, { ...p(null, "", null, null), relogio_desvio_s: Number.NaN }, { ...p(null, "", null, null), gravador: 5 }]) {
      cmd.videoProveniencia.mockResolvedValueOnce(ruim);
      await expect(provenienciaDaMidia(WS, "vm1")).rejects.toThrow("item_invalido");
    }
    expect(conferirProveniencia(p(true, "x", 0, "g"))).toEqual(p(true, "x", 0, "g"));
  });

  it("desvio na tela: vírgula decimal, vazio = não informado, texto que não é número = undefined", () => {
    expect(desvioDoTexto("−12,25")).toBe(-12.25);
    expect(desvioDoTexto(" 1.5 ")).toBe(1.5);
    expect(desvioDoTexto("3600")).toBe(3600);
    expect(desvioDoTexto("")).toBeNull();
    expect(desvioDoTexto("   ")).toBeNull();
    expect(desvioDoTexto("+,5")).toBe(0.5);
    expect(desvioDoTexto("7,")).toBe(7);
    // só decimal simples: nada de hexadecimal, expoente, palavra ou separador de milhar
    for (const t of ["abc", "1,5,3", "1e999", "--1", "0x10", "1e3", "Infinity", "NaN", "1.234,5", ",", "-", "1 000"]) expect(desvioDoTexto(t), t).toBeUndefined();
    expect(textoDoDesvio(-12.25)).toBe("-12,25");
    expect(textoDoDesvio(0)).toBe("0");
    expect(textoDoDesvio(null)).toBe("");
    expect(desvioDoTexto(textoDoDesvio(-0.125))).toBe(-0.125);
    // o que voltaria à tela com expoente não entra; o que entra volta igual
    for (const t of ["0,0000001", "1" + "0".repeat(21)]) expect(desvioDoTexto(t), t).toBeUndefined();
    for (const n of [0.000001, 86400.5, 123456789012345680000]) expect(desvioDoTexto(textoDoDesvio(n)), String(n)).toBe(n);
  });
});

describe("contrato com o Rust", () => {
  const rs = () => readFileSync(join(process.cwd(), "src-tauri/src/commands/video_proveniencia_commands.rs"), "utf8");

  it("tetos do texto iguais", () => {
    const teto = (nome: string) => Number(new RegExp(`pub const ${nome}: usize = ([\\d_]+);`).exec(rs())?.[1]?.replaceAll("_", ""));
    expect(MAX_OBS).toBe(teto("MAX_OBS"));
    expect(MAX_GRAVADOR).toBe(teto("MAX_GRAVADOR"));
  });

  it("ProvenienciaMidia: mesmos campos, na mesma ordem, com o mesmo opcional", () => {
    // o contratos-tsrust.py não lê este arquivo do Rust
    const corpo = /pub struct ProvenienciaMidia \{([^}]*)\}/.exec(rs())![1]!;
    const doRust = [...corpo.matchAll(/pub (\w+): ([^,]+),/g)].map((m) => `${m[1]}:${m[2]!.startsWith("Option<") ? "nulo" : "fixo"}`);
    const ts = readFileSync(join(process.cwd(), "src/types/edicao.ts"), "utf8");
    const iface = /export interface ProvenienciaMidia \{([^}]*)\}/.exec(ts)![1]!;
    const doTs = iface
      .split(";")
      .map((c) => c.trim())
      .filter(Boolean)
      .map((c) => {
        const [nome, tipo] = c.split(/:\s*/);
        return `${nome}:${/\|\s*null$/.test(tipo!) ? "nulo" : "fixo"}`;
      });
    expect(doRust).toEqual(["recebido_nativo:nulo", "recebido_obs:fixo", "relogio_desvio_s:nulo", "gravador:nulo"]);
    expect(doTs).toEqual(doRust);
  });
});

describe("coleta: a proveniência chega ao MidiaRef", () => {
  const video = { id: "vm1", sha256: SHA, filename: "cam 1.mp4", relative_path: "Mídias/Vídeos/cam 1.mp4", duration_s: 60, width: 1280, height: 720, raw_probe_json: "{}", derived_from_hash: null, derivation_json: null } as unknown as VideoMedia;
  const info = { exibido: { w: 1280, h: 720 }, taxa: { num: 25, den: 1 }, vfr: false, tem_audio: false, de_s: 0, ate_s: 60 } as unknown as MidiaInfo;
  beforeEach(() => {
    definirCorrente(WS, null);
    cmd.listVideoMedia.mockResolvedValue([video]);
    cmd.edicaoMidiaInfo.mockResolvedValue(info);
    cmd.edicaoQuadroEm.mockImplementation(async (_w: string, _s: string, a: { t: number }) => ({ indice: a.t * 25, pts: a.t * 1000, pts_s: a.t, dur_s: 0.04 }));
    cmd.edicaoSalvar.mockResolvedValue({ updated_at: "x", raiz_cache: "" });
  });
  const midiaSalva = () => (cmd.edicaoSalvar.mock.calls.at(-1)![1] as ProjetoEdicao).midias.find((m) => m.sha256 === SHA)!;

  it("gravada: Formato do gravador, Observação, Desvio do relógio e Gravador no MidiaRef novo", async () => {
    cmd.videoProveniencia.mockResolvedValue(p(true, "Exportado no próprio DVR", -7.25, "Intelbras MHDX 1016"));
    await receber(WS, [{ sha256: SHA, inicio_s: 2, fim_s: 4 }]);
    expect(cmd.videoProveniencia).toHaveBeenCalledWith(WS, "vm1");
    expect(midiaSalva()).toMatchObject({ recebido: { nativo: true, obs: "Exportado no próprio DVR" }, relogio_desvio_s: -7.25, gravador: "Intelbras MHDX 1016" });
  });

  it("nada gravado: MidiaRef com 'não informado'", async () => {
    cmd.videoProveniencia.mockResolvedValue(p(null, "", null, null));
    await receber(WS, [{ sha256: SHA, inicio_s: 2, fim_s: 4 }]);
    expect(midiaSalva()).toMatchObject({ recebido: { nativo: null, obs: "" }, relogio_desvio_s: null, gravador: null });
  });
});
