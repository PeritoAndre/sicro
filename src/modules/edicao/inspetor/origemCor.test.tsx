// @vitest-environment happy-dom
/**
 * Verificação da Fase 1 (lente de contratos): a "Cor" da mídia entra na chave do intermediário de origem no Rust
 * (`origem::chave` lê a `FonteInfo` já com `MidiaRef.cor`, `executar::preparar`); a tela tem de pedir de novo quando ela
 * muda, senão o palco segue com o intermediário da cor anterior.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeTrecho, EstadoOrigem, ProjetoAberto, ProjetoEdicao, TabelaQuadros } from "@domain/edicao";

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
import { ESPERA, esquecerOrigens, useOrigemStore } from "./secoes/RealceSecao";

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
const doc = () => st().doc!;
const trecho = () => doc().principal[1] as ClipeTrecho;
const TABELA: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 0, passos: Array<number>(599).fill(512), passo_mediano: 512, chaves: [0, 30, 60], vfr: false, origem: "decodificador" };
const pausa = (ms: number) => esperar(() => new Promise((r) => setTimeout(r, ms)));
const pronto = (relativo: string): EstadoOrigem => ({
  tipo: "pronto",
  previa: { relativo, tipo: "origem", desloc_s: 0, de_s: 12, ate_s: 15.5, largura: 320, altura: 180, pronta: true },
  trf: null,
});

let tela: Tela;

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-15T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString(), raiz_cache: "/cache/c1" }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoTabelaQuadros.mockResolvedValue(TABELA);
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
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
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

describe("intermediário de origem × Cor da mídia", () => {
  // a chave do intermediário no Rust leva a Cor da mídia; a assinatura da tela também
  it("trocar a Cor da mídia do trecho pede o intermediário de novo", async () => {
    cmd.edicaoOrigemPedir.mockImplementation(async () => pronto("origem/cor-da-fonte.mp4"));
    await clicar(botao(tela.raiz.querySelector<HTMLElement>('[data-realce="nitidez"]')!, "Nitidez"));
    await pausa(30);
    expect(cmd.edicaoOrigemPedir).toHaveBeenCalledTimes(1);
    expect(useOrigemStore.getState().porClipe[trecho().id]?.tipo).toBe("pronto");

    // o Rust põe a cor escolhida na chave: outra cor, outro intermediário
    cmd.edicaoOrigemPedir.mockImplementation(async () => pronto("origem/bt709-cheia.mp4"));
    const m = doc().midias.findIndex((x) => x.id === trecho().midia);
    await esperar(() => st().alterar("Cor", (d) => void (d.midias[m]!.cor = { matriz: "bt709", faixa: "pc" })));
    await pausa(30);
    expect(cmd.edicaoOrigemPedir).toHaveBeenCalledTimes(2);
    const agora = useOrigemStore.getState().porClipe[trecho().id];
    expect(agora?.tipo === "pronto" ? agora.previa.relativo : null).toBe("origem/bt709-cheia.mp4");
  });
});
