// @vitest-environment happy-dom
/**
 * Menu "Adicionar" dos presos (P1.7): abre para a direita do botão; sem espaço até a borda da coluna da Sequência
 * (1100 px de janela, botão no fim da linha), abre para a esquerda. Antes passava da coluna e ela rolava de lado.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjetoAberto, ProjetoEdicao } from "@domain/edicao";

const cmd = vi.hoisted(() => ({ edicaoAbrir: vi.fn(), edicaoSalvar: vi.fn(), edicaoAnalisar: vi.fn(), edicaoMidiaInfo: vi.fn(), edicaoTabelaQuadros: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { useEdicaoStore } from "../store/edicaoStore";
import { Presos } from "./Presos";
import styles from "./Sequencia.module.css";
import { clicar, desmontarTudo, lerFixture, montar } from "../inspetor/testeTela";

const st = () => useEdicaoStore.getState();
const C1 = "00000000-0000-4000-8000-000000000002";
const caixa = (left: number, right: number) => ({ left, right, top: 0, bottom: 20, width: right - left, height: 20, x: left, y: 0, toJSON() {} }) as DOMRect;

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockResolvedValue({ updated_at: "2026-11-15T12:00:00Z" });
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  for (const f of [cmd.edicaoMidiaInfo, cmd.edicaoTabelaQuadros]) f.mockImplementation(() => new Promise(() => {}));
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  cmd.edicaoAbrir.mockResolvedValue({ projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" } satisfies ProjetoAberto);
  await st().abrir("/casos/c1", projeto.id);
  st().selecionar({ itens: [{ k: "clipe", id: C1 }], foco: { k: "clipe", id: C1 }, efeito: null });
});

afterEach(async () => {
  desmontarTudo();
  vi.restoreAllMocks();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

async function abrirMenu(botaoEm: [number, number]): Promise<HTMLElement> {
  const tela = await montar(
    <aside data-coluna="sequencia">
      <Presos clipe={C1} />
    </aside>,
  );
  const col = tela.raiz.querySelector<HTMLElement>("[data-coluna]")!;
  const b = tela.raiz.querySelector<HTMLButtonElement>("button[aria-expanded]")!;
  vi.spyOn(col, "getBoundingClientRect").mockReturnValue(caixa(0, 300));
  vi.spyOn(b, "getBoundingClientRect").mockReturnValue(caixa(...botaoEm));
  await clicar(b);
  return tela.raiz.querySelector<HTMLElement>('[role="menu"]')!;
}

describe("menu Adicionar dos presos", () => {
  it("com espaço à direita abre para a direita", async () => {
    const m = await abrirMenu([30, 110]);
    expect(m.className).not.toContain(styles.menuEsq);
  });

  it("botão perto da borda da coluna: abre para a esquerda", async () => {
    const m = await abrirMenu([190, 270]);
    expect(m.className).toContain(styles.menuEsq);
  });
});
