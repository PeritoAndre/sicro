// @vitest-environment happy-dom
/**
 * O `.trf` que o intermediário devolve (P1.4) chega a qualquer hora. Gravado por `alterar()` no meio de um Desfazer,
 * apagava o Refazer do perito; no meio de um arrasto, cortava o gesto. Fica para quando o histórico estiver na ponta
 * e o ponteiro solto. E a "Cor" da mídia entra no que pede o intermediário de novo.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeTrecho, EfEstabilizar, Efeito, EstadoOrigem, ProjetoAberto, ProjetoEdicao, TabelaQuadros, TrfNovo } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoOrigemPedir: vi.fn(),
  realceLenteObter: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@components/Dialog/ask", () => ({ askConfirm: vi.fn(async () => true) }));

import { useEdicaoStore } from "../store/edicaoStore";
import { useFundoStore } from "../store/fundoStore";
import { Inspetor } from "./Inspetor";
import { esquecerLeituras } from "./contexto";
import { useInspetorUi } from "./ui";
import { botao, clicar, desmontarTudo, esperar, lerFixture, montar, type Tela } from "./testeTela";
import { ESPERA, esquecerOrigens } from "./origemDoClipe";
import { useOrigemStore } from "../store/origemStore";

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
const trecho = () => st().doc!.principal[1] as ClipeTrecho;
const estab = () => trecho().efeitos.find((e) => (e as Efeito).tipo === "estabilizar") as EfEstabilizar;
const TABELA: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 0, passos: Array<number>(599).fill(512), passo_mediano: 512, chaves: [0, 30, 60], vfr: false, origem: "decodificador" };
const pausa = (ms: number) => esperar(() => new Promise((r) => setTimeout(r, ms)));
const SHA = "cd".repeat(32);
let tela: Tela;
/** Espera (até 2 s) a condição: com a máquina carregada a resposta falsa demora mais que as esperas de 5 ms. */
async function ate(f: () => boolean) {
  for (let i = 0; i < 200 && !f(); i++) await pausa(10);
  expect(f()).toBe(true);
}
const pronto = () => useOrigemStore.getState().porClipe[trecho().id]?.tipo === "pronto";

/** O intermediário pronto devolve o `.trf` enquanto o documento mandado não o tem. */
function prontoComTrf() {
  cmd.edicaoOrigemPedir.mockImplementation(async (_ws: string, p: ProjetoEdicao): Promise<EstadoOrigem> => {
    const ef = (p.principal[1] as ClipeTrecho).efeitos.find((x) => (x as Efeito).tipo === "estabilizar") as EfEstabilizar;
    const trf: TrfNovo | null = ef.trf_sha256 ? null : { efeito: ef.id, sha256: SHA, zoom_pct: 0 };
    return { tipo: "pronto", previa: { relativo: "origem/k.mp4", tipo: "origem", desloc_s: 12, de_s: 12, ate_s: 15.5, largura: 640, altura: 360, pronta: true }, trf };
  });
}

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-15T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoTabelaQuadros.mockResolvedValue(TABELA);
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoOrigemPedir.mockImplementation(async (): Promise<EstadoOrigem> => ({ tipo: "fazendo", tarefa: "t1", feito: 0 }));
  cmd.realceLenteObter.mockResolvedValue(null);
  esquecerLeituras();
  esquecerOrigens();
  useFundoStore.setState({ tarefas: [] });
  Object.assign(ESPERA, { mudanca: 5, fazendo: 5, tabela: 5 });
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  const r: ProjetoAberto = { projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" };
  cmd.edicaoAbrir.mockResolvedValueOnce(r);
  await st().abrir(WS, projeto.id);
  tela = await montar(<Inspetor />);
  await esperar(() => st().selecionar({ itens: [{ k: "clipe", id: trecho().id }], foco: { k: "clipe", id: trecho().id }, efeito: null }));
  await pausa(1);
  const linha = tela.raiz.querySelector<HTMLElement>('[data-realce="estabilizar"]')!;
  await clicar(botao(linha, "Estabilizar"));
  await pausa(15);
  expect(estab()).toMatchObject({ trf_sha256: null });
});

afterEach(async () => {
  window.dispatchEvent(new MouseEvent("pointerup", { bubbles: true }));
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

describe("o .trf do intermediário não atropela o histórico nem o gesto", () => {
  it("chegando no meio de um Desfazer, o Refazer fica; o .trf entra quando o histórico volta à ponta", async () => {
    await esperar(() => st().alterar("Nome", (d) => void ((d.principal[0] as { rotulo: string }).rotulo = "Cartela")));
    await esperar(() => st().alterar("Nome", (d) => void ((d.principal[0] as { rotulo: string }).rotulo = "Cartela 2")));
    await esperar(() => st().desfazer());
    await esperar(() => st().desfazer());
    expect(st().futuro.length).toBe(2);
    prontoComTrf();
    await ate(pronto);
    await pausa(20);
    // o Refazer do perito continua lá
    expect(st().futuro.length).toBe(2);
    expect(estab().trf_sha256).toBeNull();
    await esperar(() => st().refazer());
    await esperar(() => st().refazer());
    expect((st().doc!.principal[0] as { rotulo: string }).rotulo).toBe("Cartela 2");
    // de volta à ponta: o .trf entra, num passo próprio
    await ate(() => estab().trf_sha256 === SHA);
    expect(st().passado.at(-1)?.rotulo).toBe("Estabilizar");
    expect(st().futuro.length).toBe(0);
  });

  it("chegando no meio de um arrasto, o gesto segue inteiro; o .trf entra ao soltar", async () => {
    // a resposta sai antes do arrasto e chega durante ele
    prontoComTrf();
    const real = cmd.edicaoOrigemPedir.getMockImplementation()!;
    let soltar: () => void = () => {};
    cmd.edicaoOrigemPedir.mockImplementation((...a: unknown[]) => new Promise((ok) => (soltar = () => ok((real as (...x: unknown[]) => unknown)(...a)))));
    await pausa(40);
    expect(cmd.edicaoOrigemPedir).toHaveBeenCalled();
    window.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
    const g = st().iniciarGesto("Nome");
    g.alterar((d) => void ((d.principal[0] as { rotulo: string }).rotulo = "a"));
    await esperar(() => soltar());
    await ate(pronto);
    await pausa(20);
    expect(estab().trf_sha256).toBeNull();
    // o gesto não foi cortado: a mudança seguinte ainda vale
    g.alterar((d) => void ((d.principal[0] as { rotulo: string }).rotulo = "ab"));
    expect((st().doc!.principal[0] as { rotulo: string }).rotulo).toBe("ab");
    g.concluir();
    // gesto concluído com o botão ainda apertado: espera soltar
    await pausa(20);
    expect(estab().trf_sha256).toBeNull();
    window.dispatchEvent(new MouseEvent("pointerup", { bubbles: true }));
    await ate(() => estab().trf_sha256 === SHA);
    expect(st().passado.slice(-2).map((p) => p.rotulo)).toEqual(["Nome", "Estabilizar"]);
  });

  it("o efeito mudou enquanto esperava: o .trf adiado não entra (o pedido novo decide)", async () => {
    await esperar(() => st().alterar("Nome", (d) => void ((d.principal[0] as { rotulo: string }).rotulo = "x")));
    await esperar(() => st().desfazer());
    prontoComTrf();
    await ate(pronto);
    await pausa(20);
    expect(estab().trf_sha256).toBeNull();
    // o perito muda a suavização antes de voltar à ponta: o .trf daquele pedido não vale mais
    cmd.edicaoOrigemPedir.mockImplementation(async (): Promise<EstadoOrigem> => ({ tipo: "fazendo", tarefa: "t9", feito: 0 }));
    await esperar(() => st().alterar("Suavizar", (d) => {
      const ef = (d.principal[1] as ClipeTrecho).efeitos.find((x) => (x as Efeito).tipo === "estabilizar") as EfEstabilizar;
      ef.suavizar = 30;
    }));
    await pausa(40);
    expect(estab()).toMatchObject({ suavizar: 30, trf_sha256: null });
  });
});

describe("Cor da mídia e o intermediário", () => {
  it("trocar a Cor pede o intermediário de novo (a chave do Rust leva a cor) e o pronto de antes sai do palco", async () => {
    prontoComTrf();
    await ate(() => estab().trf_sha256 === SHA);
    // o pedido que a gravação do .trf provoca volta pronto, sem .trf: nada mais é pedido
    await pausa(60);
    expect(useOrigemStore.getState().porClipe[trecho().id]?.tipo).toBe("pronto");
    cmd.edicaoOrigemPedir.mockClear();
    cmd.edicaoOrigemPedir.mockImplementation(async (): Promise<EstadoOrigem> => ({ tipo: "fazendo", tarefa: "t5", feito: 0 }));
    await pausa(40);
    expect(cmd.edicaoOrigemPedir).not.toHaveBeenCalled();
    await esperar(() =>
      st().alterar("Cor", (d) => {
        const m = d.midias.find((x) => x.id === trecho().midia)!;
        m.cor = { matriz: "bt709", faixa: "pc" };
      }),
    );
    // já sai do palco: o pronto era da cor de antes
    expect(useOrigemStore.getState().porClipe[trecho().id]?.tipo).not.toBe("pronto");
    await pausa(40);
    expect(cmd.edicaoOrigemPedir).toHaveBeenCalled();
    const doc = cmd.edicaoOrigemPedir.mock.calls.at(-1)![1] as ProjetoEdicao;
    expect(doc.midias.find((x) => x.id === trecho().midia)!.cor).toEqual({ matriz: "bt709", faixa: "pc" });
    expect(useOrigemStore.getState().porClipe[trecho().id]?.tipo).toBe("fazendo");
  });
});
