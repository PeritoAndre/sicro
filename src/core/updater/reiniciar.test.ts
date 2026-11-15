/**
 * Atualização com a Edição aberta: a guarda de saída roda ANTES de instalar (no Windows o instalador fecha o app e
 * nada depois dele roda) e de novo antes do reinício (fecho da Fase 1). Gravação pendente é gravada antes; se a
 * gravação falha, o perito decide, e sem "Descartar" nada é instalado.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjetoEdicao } from "@domain/edicao";

const cmd = vi.hoisted(() => ({ edicaoAbrir: vi.fn(), edicaoSalvar: vi.fn(), edicaoAnalisar: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(async () => "6.0.0") }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn(async () => {}) }));
const proc = vi.hoisted(() => ({ relaunch: vi.fn(async () => {}) }));
vi.mock("@tauri-apps/plugin-process", () => proc);
const plugin = vi.hoisted(() => ({ check: vi.fn() }));
vi.mock("@tauri-apps/plugin-updater", () => plugin);
const ask = vi.hoisted(() => ({ askConfirm: vi.fn(async (_o: { title: string; message?: string; confirmLabel?: string; danger?: boolean }) => false) }));
vi.mock("@components/Dialog/ask", () => ask);
vi.mock("@/components/toast/toastStore", () => ({ pushToast: vi.fn() }));

import { instalarAtualizacao, useUpdaterStore, verificarAtualizacao } from "./updaterStore";
import { useEdicaoStore } from "@modules/edicao/store/edicaoStore";
import { novoProjeto } from "@modules/edicao/modelo/projeto";

const st = () => useEdicaoStore.getState();
const ordem: string[] = [];

/** Atualização achada e pronta para instalar (o plugin falso baixa na hora). */
async function comAtualizacao() {
  plugin.check.mockResolvedValue({ version: "6.1.0", body: "", downloadAndInstall: vi.fn(async () => void ordem.push("instalou")) });
  await verificarAtualizacao(true);
  expect(useUpdaterStore.getState()).toMatchObject({ status: "available", instalavel: true });
}

async function abrirComPendencia() {
  const p: ProjetoEdicao = novoProjeto();
  cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "" });
  await st().abrir("/casos/atualiza", p.id);
  st().alterar("Título", (d) => void (d.titulo = "pendente"));
}

beforeEach(async () => {
  ordem.length = 0;
  for (const f of [cmd.edicaoAbrir, cmd.edicaoSalvar, cmd.edicaoAnalisar, proc.relaunch, plugin.check, ask.askConfirm]) f.mockReset();
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoSalvar.mockImplementation(async (_ws: string, p: ProjetoEdicao) => (ordem.push(`salvou:${p.titulo}`), { updated_at: "x", raiz_cache: "" }));
  proc.relaunch.mockImplementation(async () => void ordem.push("reiniciou"));
  ask.askConfirm.mockResolvedValue(false);
  await st().fechar({ descartar: true }).catch(() => {});
  useUpdaterStore.setState({ status: "idle", adiada: false, aviso: false, instalavel: false });
});

describe("reinício depois de instalar a atualização", () => {
  it("sem edição aberta: reinicia", async () => {
    await comAtualizacao();
    await instalarAtualizacao();
    expect(ordem).toEqual(["instalou", "reiniciou"]);
    expect(ask.askConfirm).not.toHaveBeenCalled();
  });

  it("edição com pendência: grava ANTES de instalar (o instalador do Windows fecha o app)", async () => {
    await comAtualizacao();
    await abrirComPendencia();
    await instalarAtualizacao();
    expect(ordem).toEqual(["salvou:pendente", "instalou", "reiniciou"]);
  });

  it("editado durante a descarga: grava de novo antes de reiniciar", async () => {
    await abrirComPendencia();
    plugin.check.mockResolvedValue({
      version: "6.1.0",
      body: "",
      downloadAndInstall: vi.fn(async () => {
        ordem.push("instalou");
        st().alterar("Título", (d) => void (d.titulo = "durante"));
      }),
    });
    await verificarAtualizacao(true);
    await instalarAtualizacao();
    expect(ordem).toEqual(["salvou:pendente", "instalou", "salvou:durante", "reiniciou"]);
  });

  it("gravação que falha: pergunta; sem Descartar NÃO instala nem reinicia (a edição fica aberta); com Descartar, instala e reinicia", async () => {
    await comAtualizacao();
    await abrirComPendencia();
    cmd.edicaoSalvar.mockRejectedValue({ kind: "filesystem", message: "filesystem error: disco cheio" });
    await instalarAtualizacao();
    expect(ask.askConfirm).toHaveBeenCalledWith({ title: "Não salvou", message: "disco cheio", confirmLabel: "Descartar", danger: true });
    expect(ordem).toEqual([]);
    expect(proc.relaunch).not.toHaveBeenCalled();
    expect(st().aberta).not.toBeNull();
    expect(st().doc?.titulo).toBe("pendente");
    // a atualização continua oferecida
    expect(useUpdaterStore.getState()).toMatchObject({ status: "available", instalavel: true });
    // o perito manda instalar de novo e descarta
    ask.askConfirm.mockResolvedValue(true);
    await comAtualizacao();
    await instalarAtualizacao();
    expect(ordem).toEqual(["instalou", "reiniciou"]);
    expect(st().aberta).toBeNull();
  });
});
