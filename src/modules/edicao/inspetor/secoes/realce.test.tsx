// @vitest-environment happy-dom
/**
 * Clipe · Realce (P1.4), montado no Inspetor de verdade: cada efeito de origem liga, edita e sai pela store (um passo
 * com o rótulo do campo); a análise da estabilização é a faixa do trecho; o `.trf` e o zoom do intermediário entram
 * no efeito por `alterar()`; só rótulos do §8.6.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeCongelar, ClipeTrecho, EfAjustes, EfDesentrelacar, EfEstabilizar, EfLente, EfMediaTemporal, EfNitidez, Efeito, EstadoOrigem, Problema, ProjetoAberto, ProjetoEdicao, TabelaQuadros, TrfNovo } from "@domain/edicao";

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
const ask = vi.hoisted(() => ({ askConfirm: vi.fn(async (_o: { title: string; message?: string; confirmLabel?: string; danger?: boolean }) => true) }));
vi.mock("@components/Dialog/ask", () => ask);

import { useEdicaoStore } from "../../store/edicaoStore";
import { useFundoStore } from "../../store/fundoStore";
import { Inspetor } from "../Inspetor";
import { esquecerLeituras } from "../contexto";
import { useInspetorUi } from "../ui";
import { arrastar, botao, clicar, desmontarTudo, entradaDe, esperar, lerFixture as ler, montar, textosVisiveis, type Tela } from "../testeTela";
import { textoPermitido } from "../rotulos.teste";
import { ESPERA, esquecerOrigens, REALCE_PRONTO, useOrigemStore } from "./RealceSecao";

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
const doc = () => st().doc!;
const passos = () => st().passado.length;
const ultimo = () => st().passado.at(-1)?.rotulo;
const trecho = () => doc().principal[1] as ClipeTrecho;
const realces = () => trecho().efeitos.filter((e) => ["estabilizar", "lente", "desentrelacar", "media_temporal", "ajustes", "nitidez"].includes((e as Efeito).tipo)) as Efeito[];
const um = <E extends Efeito>(tipo: E["tipo"]) => realces().find((e) => e.tipo === tipo) as E;

/** m1: 30 qps na base 1/15360, 20 s. O trecho vai de 12 s (quadro 360) a 15,5 s (quadro 465, de fora). */
const TABELA: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 0, passos: Array<number>(599).fill(512), passo_mediano: 512, chaves: [0, 30, 60], vfr: false, origem: "decodificador" };

let tela: Tela;
const linha = (tipo: string) => tela.raiz.querySelector<HTMLElement>(`[data-efeito="${tipo}"], [data-realce="${tipo}"]`)!;
const olho = (tipo: string, rotulo: string) => botao(linha(tipo), rotulo);
const pausa = (ms: number) => esperar(() => new Promise((r) => setTimeout(r, ms)));

async function abrir(projeto: ProjetoEdicao, avisos: Problema[] = []) {
  const r: ProjetoAberto = { projeto, importado_de: null, so_leitura: false, avisos, raiz_cache: "/cache/c1" };
  cmd.edicaoAbrir.mockResolvedValueOnce(r);
  await st().abrir(WS, projeto.id);
  tela = await montar(<Inspetor />);
}
const selecionar = (id: string) => esperar(() => st().selecionar({ itens: [{ k: "clipe", id }], foco: { k: "clipe", id }, efeito: null }));

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-14T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  ask.askConfirm.mockClear();
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
});

afterEach(async () => {
  window.dispatchEvent(new MouseEvent("pointerup", { bubbles: true }));
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

const projeto = () => ler<ProjetoEdicao>("projeto-do-store.json");

describe("Clipe · Realce", () => {
  beforeEach(async () => {
    await abrir(projeto());
    await selecionar(trecho().id);
    await pausa(1);
  });

  it("a seção do P1.4 está ligada; trecho sem efeito de origem mostra as seis linhas desligadas, na ordem do motor", () => {
    expect(REALCE_PRONTO).toBe(true);
    expect([...tela.raiz.querySelectorAll<HTMLElement>("[data-realce]")].map((l) => l.dataset.realce)).toEqual(["desentrelacar", "estabilizar", "lente", "ajustes", "nitidez", "media_temporal"]);
    expect([...tela.raiz.querySelectorAll<HTMLElement>("[data-realce] button")].map((b) => [b.getAttribute("aria-label"), b.getAttribute("aria-pressed")])).toEqual([
      ["Desentrelaçar", "false"],
      ["Estabilizar", "false"],
      ["Lente", "false"],
      ["Brilho", "false"],
      ["Nitidez", "false"],
      ["Média de quadros", "false"],
    ]);
    // o vídeo não tem perfil de lente salvo: só a Lente fica desligada
    expect([...tela.raiz.querySelectorAll<HTMLButtonElement>("[data-realce] button")].filter((b) => b.disabled).map((b) => b.getAttribute("aria-label"))).toEqual(["Lente"]);
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
    expect(tela.raiz.querySelector("[placeholder]")).toBeNull();
    // sem efeito de origem ligado não há intermediário a pedir
    expect(cmd.edicaoOrigemPedir).not.toHaveBeenCalled();
  });

  it("Nitidez: ligar cria (um passo), o arrasto é um passo, desligar esconde os campos, remover tira", async () => {
    const n = passos();
    await clicar(olho("nitidez", "Nitidez"));
    await pausa(1);
    expect(um<EfNitidez>("nitidez")).toMatchObject({ tipo: "nitidez", ligado: true, nome: "", ativo: null, quantidade: 0.5 });
    expect([passos(), ultimo()]).toEqual([n + 1, "Nitidez"]);
    const l = linha("nitidez");
    expect(l.dataset.efeito).toBe("nitidez");
    await arrastar(l.querySelector<HTMLInputElement>('input[type="range"]')!, [0.8, 1.1, 1.25]);
    expect(um<EfNitidez>("nitidez").quantidade).toBe(1.25);
    expect([passos(), ultimo()]).toEqual([n + 2, "Nitidez"]);
    await clicar(olho("nitidez", "Nitidez"));
    expect(um<EfNitidez>("nitidez").ligado).toBe(false);
    expect(linha("nitidez").querySelector("input")).toBeNull();
    await clicar(botao(linha("nitidez"), "Remover"));
    expect(realces()).toEqual([]);
    expect(linha("nitidez").dataset.realce).toBe("nitidez");
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
  });

  it("Desentrelaçar (Quadro, Campo), ajustes (Brilho, Contraste, Gama, Saturação) e Média de quadros gravam os valores", async () => {
    await clicar(olho("desentrelacar", "Desentrelaçar"));
    await pausa(1);
    expect(um<EfDesentrelacar>("desentrelacar")).toMatchObject({ modo: "quadro", ordem: "auto", ligado: true });
    await clicar(botao(linha("desentrelacar"), "Campo"));
    expect([um<EfDesentrelacar>("desentrelacar").modo, ultimo()]).toEqual(["campo", "Desentrelaçar"]);

    await clicar(olho("ajustes", "Brilho"));
    await pausa(1);
    expect(um<EfAjustes>("ajustes")).toMatchObject({ brilho: 0, contraste: 1, gama: 1, saturacao: 1 });
    const l = linha("ajustes");
    for (const [rotulo, valor, chave] of [["Brilho", 0.12, "brilho"], ["Contraste", 1.3, "contraste"], ["Gama", 0.8, "gama"], ["Saturação", 1.5, "saturacao"]] as const) {
      await arrastar(l.querySelector<HTMLInputElement>(`[data-campo="${rotulo}"] input[type="range"]`)!, [valor]);
      expect([um<EfAjustes>("ajustes")[chave], ultimo()]).toEqual([valor, rotulo]);
    }

    await clicar(olho("media_temporal", "Média de quadros"));
    await pausa(1);
    expect(um<EfMediaTemporal>("media_temporal").quadros).toBe(4);
    const media = linha("media_temporal").querySelector<HTMLInputElement>('input[type="range"]')!;
    expect([media.min, media.max]).toEqual(["2", "15"]);
    await arrastar(media, [7]);
    expect(um<EfMediaTemporal>("media_temporal").quadros).toBe(7);
    // na lista do clipe entram no fim, na ordem em que foram ligados; a tela mostra na ordem do motor
    expect(realces().map((e) => e.tipo)).toEqual(["desentrelacar", "ajustes", "media_temporal"]);
    expect([...tela.raiz.querySelectorAll<HTMLElement>("[data-efeito], [data-realce]")].map((x) => x.dataset.efeito ?? `(${x.dataset.realce})`).slice(0, 6)).toEqual([
      "desentrelacar", "(estabilizar)", "(lente)", "ajustes", "(nitidez)", "media_temporal",
    ]);
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
  });

  it("Estabilizar: análise = quadros do trecho; confirma com a contagem; o .trf e o zoom do intermediário entram por alterar()", async () => {
    const pronto = (trf: TrfNovo | null): EstadoOrigem => ({ tipo: "pronto", previa: { relativo: "origem/k.mp4", tipo: "origem", desloc_s: 12, de_s: 12, ate_s: 15.5, largura: 640, altura: 360, pronta: true }, trf });
    const n = passos();
    await clicar(olho("estabilizar", "Estabilizar"));
    await pausa(1);
    // o trecho tem um destaque marcado: mudar o espaço pede confirmação com a contagem
    expect(ask.askConfirm).toHaveBeenCalledTimes(1);
    expect(ask.askConfirm.mock.calls[0]![0]).toMatchObject({ title: "Estabilizar", confirmLabel: "Estabilizar", danger: true });
    expect(ask.askConfirm.mock.calls[0]![0].message).toMatch(/^\d+ marcaç(ão|ões)$/);
    const e = um<EfEstabilizar>("estabilizar");
    // do quadro 360 (12 s) ao 464, o último do trecho (o 465 é o fim, de fora)
    expect(e).toMatchObject({ suavizar: 15, borda: "preta", zoom_pct: 0, trf_sha256: null, analise: { de_s: 12, ate_s: (464 * 512) / 15360, de_pts: 360 * 512, ate_pts: 464 * 512 } });
    expect([passos(), ultimo()]).toEqual([n + 1, "Estabilizar"]);

    // o intermediário: fazendo (ícone, sem texto) e depois pronto com o `.trf` novo
    await pausa(15);
    expect(cmd.edicaoOrigemPedir).toHaveBeenCalled();
    expect(cmd.edicaoOrigemPedir.mock.calls[0]).toEqual([WS, doc(), trecho().id]);
    expect(tela.raiz.querySelector('[data-origem="fazendo"]')).toBeTruthy();
    expect(useOrigemStore.getState().porClipe[trecho().id]?.tipo).toBe("fazendo");
    const sha = "ab".repeat(32);
    cmd.edicaoOrigemPedir.mockImplementation(async (_ws: string, p: ProjetoEdicao) => {
      const ef = (p.principal[1] as ClipeTrecho).efeitos.find((x) => (x as Efeito).tipo === "estabilizar") as EfEstabilizar;
      return pronto(ef.trf_sha256 ? null : { efeito: e.id, sha256: sha, zoom_pct: 0 });
    });
    await pausa(30);
    expect(um<EfEstabilizar>("estabilizar")).toMatchObject({ trf_sha256: sha, zoom_pct: 0 });
    expect([passos(), ultimo()]).toEqual([n + 2, "Estabilizar"]);
    expect(tela.raiz.querySelector('[data-origem="fazendo"]')).toBeNull();
    expect(useOrigemStore.getState().porClipe[trecho().id]?.tipo).toBe("pronto");

    // desfazer tira o `.trf` e ele não volta sozinho (senão o Desfazer ficaria preso ali)
    await esperar(() => st().desfazer());
    await pausa(30);
    expect(um<EfEstabilizar>("estabilizar").trf_sha256).toBeNull();
    expect(passos()).toBe(n + 1);
    await esperar(() => st().refazer());
    expect(um<EfEstabilizar>("estabilizar").trf_sha256).toBe(sha);

    // Suavizar e Borda; "Ampliar" zera o zoom, que o intermediário calcula e devolve
    const l = linha("estabilizar");
    await clicar(botao(l, "Ampliar"));
    expect(um<EfEstabilizar>("estabilizar")).toMatchObject({ borda: "ampliar", zoom_pct: 0, trf_sha256: sha });
    expect(ultimo()).toBe("Borda");
    cmd.edicaoOrigemPedir.mockImplementation(async (_ws: string, p: ProjetoEdicao) => {
      const ef = (p.principal[1] as ClipeTrecho).efeitos.find((x) => (x as Efeito).tipo === "estabilizar") as EfEstabilizar;
      return pronto(ef.zoom_pct > 0 ? null : { efeito: e.id, sha256: sha, zoom_pct: 5.25 });
    });
    await pausa(30);
    expect(um<EfEstabilizar>("estabilizar")).toMatchObject({ borda: "ampliar", zoom_pct: 5.25, trf_sha256: sha });
    expect((entradaDe(linha("estabilizar"), "Suavizar") as HTMLInputElement).value).toBe("15");
    // de volta a "Preta" o zoom zera; "Ampliar" outra vez: o mesmo zoom volta do intermediário e é gravado de novo
    cmd.edicaoOrigemPedir.mockImplementation(async (_ws: string, p: ProjetoEdicao) => {
      const ef = (p.principal[1] as ClipeTrecho).efeitos.find((x) => (x as Efeito).tipo === "estabilizar") as EfEstabilizar;
      const falta = !ef.trf_sha256 || (ef.borda === "ampliar" && ef.zoom_pct === 0);
      return pronto(falta ? { efeito: e.id, sha256: sha, zoom_pct: ef.borda === "ampliar" ? 5.25 : 0 } : null);
    });
    await clicar(botao(linha("estabilizar"), "Preta"));
    expect(um<EfEstabilizar>("estabilizar")).toMatchObject({ borda: "preta", zoom_pct: 0, trf_sha256: sha });
    await pausa(30);
    expect(um<EfEstabilizar>("estabilizar")).toMatchObject({ borda: "preta", zoom_pct: 0 });
    await clicar(botao(linha("estabilizar"), "Ampliar"));
    await pausa(30);
    expect(um<EfEstabilizar>("estabilizar")).toMatchObject({ borda: "ampliar", zoom_pct: 5.25, trf_sha256: sha });

    // "Refazer estabilização": o trecho cresceu; a análise passa a ser a faixa de agora e o `.trf` é pedido de novo
    await esperar(() => st().alterar("Fim", (d) => void Object.assign(d.principal[1] as ClipeTrecho, { fim_s: 16, fim_pts: 480 * 512 })));
    ask.askConfirm.mockClear();
    const refeito = cmd.edicaoOrigemPedir.getMockImplementation()!;
    cmd.edicaoOrigemPedir.mockImplementation(async (): Promise<EstadoOrigem> => ({ tipo: "fazendo", tarefa: "t2", feito: 0 }));
    await clicar(botao(linha("estabilizar"), "Refazer estabilização"));
    await pausa(1);
    expect(ask.askConfirm).toHaveBeenCalledTimes(1);
    expect(um<EfEstabilizar>("estabilizar")).toMatchObject({ trf_sha256: null, zoom_pct: 0, analise: { de_pts: 360 * 512, ate_pts: 479 * 512, ate_s: (479 * 512) / 15360 } });
    expect(ultimo()).toBe("Refazer estabilização");
    // a análise refeita dá o mesmo arquivo: ele é gravado de novo no efeito
    cmd.edicaoOrigemPedir.mockImplementation(refeito);
    await pausa(30);
    expect(um<EfEstabilizar>("estabilizar")).toMatchObject({ trf_sha256: sha, zoom_pct: 5.25, analise: { ate_pts: 479 * 512 } });
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
  });

  it("Estabilizar recusado na confirmação não cria nada; sem a tabela de quadros o botão fica desligado", async () => {
    ask.askConfirm.mockResolvedValueOnce(false);
    const n = passos();
    await clicar(olho("estabilizar", "Estabilizar"));
    await pausa(1);
    expect([realces(), passos()]).toEqual([[], n]);
    // outra mídia sem tabela ainda
    cmd.edicaoTabelaQuadros.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
    esquecerLeituras();
    await selecionar((doc().principal[2] as ClipeTrecho).id);
    await pausa(1);
    expect(olho("estabilizar", "Estabilizar").disabled).toBe(true);
    expect(olho("nitidez", "Nitidez").disabled).toBe(false);
  });

  it("Lente: sem perfil salvo no vídeo o botão fica desligado; com perfil, cria com os valores dele", async () => {
    expect(cmd.realceLenteObter).toHaveBeenCalledWith(WS, doc().midias[0]!.sha256);
    expect(olho("lente", "Lente").disabled).toBe(true);
    // o perfil foi salvo na análise com a Edição montada: o ponteiro sobre a linha confere de novo
    cmd.realceLenteObter.mockResolvedValue({ nome: "Câmera do portão", k1: -0.21, k2: 0.03, cx: 0.5, cy: 0.48 });
    await esperar(() => void linha("lente").dispatchEvent(new MouseEvent("pointerover", { bubbles: true })));
    await pausa(1);
    expect(olho("lente", "Lente").disabled).toBe(false);
    // perfil apagado entre a leitura e o clique: nada é criado e o botão volta a ficar desligado
    cmd.realceLenteObter.mockResolvedValueOnce(null).mockResolvedValueOnce(null);
    await clicar(olho("lente", "Lente"));
    await pausa(1);
    expect([realces(), olho("lente", "Lente").disabled]).toEqual([[], true]);
    await esperar(() => void linha("lente").dispatchEvent(new MouseEvent("pointerover", { bubbles: true })));
    await pausa(1);
    await clicar(olho("lente", "Lente"));
    await pausa(1);
    expect(um<EfLente>("lente")).toMatchObject({ tipo: "lente", ligado: true, k1: -0.21, k2: 0.03, cx: 0.5, cy: 0.48, perfil: "Câmera do portão" });
    expect(ultimo()).toBe("Lente");
    // o nome do perfil é dado do caso: não vai para a tela
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
    // desligar e remover também confirmam (a lente muda o espaço das marcações)
    ask.askConfirm.mockClear();
    await clicar(botao(linha("lente"), "Remover"));
    await pausa(1);
    expect(ask.askConfirm.mock.calls[0]![0]).toMatchObject({ title: "Lente", confirmLabel: "Remover" });
    expect(realces()).toEqual([]);
  });

  it("a fila de fundo avisa: progresso no estado e nova consulta quando a tarefa termina, sem esperar a reserva", async () => {
    Object.assign(ESPERA, { mudanca: 5, fazendo: 600_000, tabela: 5 });
    cmd.edicaoOrigemPedir.mockImplementation(async (): Promise<EstadoOrigem> => ({ tipo: "fazendo", tarefa: "t7", feito: 0 }));
    await clicar(olho("nitidez", "Nitidez"));
    await pausa(20);
    expect(cmd.edicaoOrigemPedir).toHaveBeenCalledTimes(1);
    const tarefa = (feito: number, estado: "rodando" | "pronta") => ({ id: "t7", tipo: "origem" as const, chave: "origem:k", feito, estado: { tipo: estado } });
    // tarefa de outro pedido não mexe em nada
    await esperar(() => useFundoStore.getState().receber([{ ...tarefa(1, "pronta"), id: "outra" }]));
    await esperar(() => useFundoStore.getState().receber([tarefa(0.4, "rodando")]));
    expect(useOrigemStore.getState().porClipe[trecho().id]).toEqual({ tipo: "fazendo", tarefa: "t7", feito: 0.4 });
    expect(cmd.edicaoOrigemPedir).toHaveBeenCalledTimes(1);
    cmd.edicaoOrigemPedir.mockImplementation(async (): Promise<EstadoOrigem> => ({ tipo: "pronto", previa: { relativo: "origem/k.mp4", tipo: "origem", desloc_s: 12, de_s: 12, ate_s: 15.5, largura: 640, altura: 360, pronta: true }, trf: null }));
    await esperar(() => useFundoStore.getState().receber([tarefa(1, "pronta")]));
    await pausa(5);
    expect(cmd.edicaoOrigemPedir).toHaveBeenCalledTimes(2);
    expect(useOrigemStore.getState().porClipe[trecho().id]?.tipo).toBe("pronto");
    // pronto: a fila não provoca mais consulta
    await esperar(() => useFundoStore.getState().receber([tarefa(1, "pronta")]));
    await pausa(5);
    expect(cmd.edicaoOrigemPedir).toHaveBeenCalledTimes(2);
  });

  it("com o ponteiro apertado (arrasto em curso) o intermediário espera; ao soltar, é pedido com o valor final", async () => {
    await clicar(olho("ajustes", "Brilho"));
    await pausa(20);
    const antes = cmd.edicaoOrigemPedir.mock.calls.length;
    expect(antes).toBeGreaterThan(0);
    cmd.edicaoOrigemPedir.mockImplementation(async (): Promise<EstadoOrigem> => ({ tipo: "pronto", previa: { relativo: "origem/k.mp4", tipo: "origem", desloc_s: 12, de_s: 12, ate_s: 15.5, largura: 640, altura: 360, pronta: true }, trf: null }));
    await pausa(20);
    const parado = cmd.edicaoOrigemPedir.mock.calls.length;
    window.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
    const mexer = (v: number) => esperar(() => st().alterar("Brilho", (d) => void (((d.principal[1] as ClipeTrecho).efeitos.find((x) => (x as Efeito).tipo === "ajustes") as EfAjustes).brilho = v)));
    await mexer(0.1);
    await pausa(25);
    await mexer(0.2);
    await pausa(25);
    expect(cmd.edicaoOrigemPedir.mock.calls.length).toBe(parado);
    window.dispatchEvent(new MouseEvent("pointerup", { bubbles: true }));
    await pausa(25);
    expect(cmd.edicaoOrigemPedir.mock.calls.length).toBe(parado + 1);
    const pedido = cmd.edicaoOrigemPedir.mock.calls.at(-1)![1] as ProjetoEdicao;
    expect(((pedido.principal[1] as ClipeTrecho).efeitos.find((x) => (x as Efeito).tipo === "ajustes") as EfAjustes).brilho).toBe(0.2);
  });

  it("falha do intermediário: ícone, sem texto; tabela pendente: pede de novo", async () => {
    cmd.edicaoOrigemPedir.mockRejectedValueOnce({ kind: "validation", message: "validation error: tabela_pendente: Tabela de quadros em preparo" });
    cmd.edicaoOrigemPedir.mockRejectedValue({ kind: "validation", message: "validation error: filtro_ausente: Filtro ausente no ffmpeg: unsharp" });
    await clicar(olho("nitidez", "Nitidez"));
    await pausa(40);
    expect(cmd.edicaoOrigemPedir.mock.calls.length).toBe(2);
    expect(tela.raiz.querySelector('[data-origem="falhou"]')).toBeTruthy();
    expect(useOrigemStore.getState().porClipe[trecho().id]).toMatchObject({ tipo: "falhou", erro: { codigo: "filtro_ausente" } });
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
  });
});

describe("Clipe · Realce em base de um tique por quadro", () => {
  it("AVI: Desentrelaçar só oferece Quadro; projeto que já traz Campo deixa voltar a Quadro", async () => {
    cmd.edicaoTabelaQuadros.mockResolvedValue({ ...TABELA, tb: { num: 1, den: 25 }, passos: Array<number>(599).fill(1), passo_mediano: 1 });
    const p = projeto();
    const c = p.principal[1] as ClipeTrecho;
    Object.assign(c, { inicio_s: 12, inicio_pts: 300, fim_s: 15.48, fim_pts: 387 });
    await abrir(p);
    await selecionar(c.id);
    await pausa(1);
    await clicar(olho("desentrelacar", "Desentrelaçar"));
    await pausa(1);
    const opcoes = () => [...linha("desentrelacar").querySelectorAll<HTMLElement>('[data-campo="Desentrelaçar"] button, [data-campo="Desentrelaçar"] option')].map((b) => b.textContent);
    expect(opcoes()).toEqual(["Quadro"]);
    await esperar(() => st().alterar("Desentrelaçar", (d) => void ((((d.principal[1] as ClipeTrecho).efeitos[(d.principal[1] as ClipeTrecho).efeitos.length - 1] as EfDesentrelacar).modo = "campo"))));
    expect(opcoes()).toEqual(["Quadro", "Campo"]);
    await clicar(botao(linha("desentrelacar"), "Quadro"));
    expect(um<EfDesentrelacar>("desentrelacar").modo).toBe("quadro");
    expect(opcoes()).toEqual(["Quadro"]);
  });
});

describe("Clipe · Realce em outros clipes", () => {
  it("quadro congelado não oferece Estabilizar nem pede intermediário; título não tem a seção", async () => {
    const p = projeto();
    const congelado: ClipeCongelar = { tipo: "congelar", id: "00000000-0000-4000-8000-0000000000c9", rotulo: "", desligado: false, transicao_entrada: null, efeitos: [], midia: "m1", instante_s: 13, instante_pts: 390 * 512, duracao_s: 2 };
    p.principal.push(congelado);
    await abrir(p);
    await selecionar(congelado.id);
    await pausa(1);
    expect([...tela.raiz.querySelectorAll<HTMLElement>("[data-realce]")].map((l) => l.dataset.realce)).toEqual(["desentrelacar", "lente", "ajustes", "nitidez", "media_temporal"]);
    await clicar(olho("media_temporal", "Média de quadros"));
    await pausa(20);
    expect((doc().principal[3] as ClipeCongelar).efeitos).toMatchObject([{ tipo: "media_temporal", quadros: 4 }]);
    expect(cmd.edicaoOrigemPedir).not.toHaveBeenCalled();
    await selecionar((doc().principal[0] as ClipeTrecho).id);
    await pausa(1);
    expect(tela.raiz.querySelector("[data-realce], [data-efeito]")).toBeNull();
  });

  it("efeito de origem que o Rust acusa (item_invalido) fica apagado, só com o tipo e o Remover", async () => {
    const p = projeto();
    const c = p.principal[1] as ClipeTrecho;
    c.efeitos.push({ tipo: "media_temporal", id: "e-ruim", ligado: true, nome: "", ativo: null, quadros: 99 });
    const invalido: Problema = { nivel: "erro", codigo: "item_invalido", mensagem: "", clipe: c.id, efeito: "e-ruim", trilha: null, midia: null, campo: "quadros", n: null };
    await abrir(p, [invalido]);
    await selecionar(c.id);
    await pausa(1);
    const l = linha("media_temporal");
    expect([l.dataset.efeito, /efeitoApagado/.test(l.className), l.querySelector("code")?.textContent, l.querySelector("input")]).toEqual(["media_temporal", true, "media_temporal", null]);
    await clicar(botao(l, "Remover"));
    expect(trecho().efeitos.some((e) => (e as Efeito).id === "e-ruim")).toBe(false);
  });
});
