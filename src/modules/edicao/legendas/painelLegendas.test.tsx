// @vitest-environment happy-dom
/**
 * Aba Legendas montada de verdade (Inspetor, happy-dom), editando pela store: "Da degravação…" pelo comando (falso),
 * num passo só e sem duplicar; "Nova legenda" no cursor; cada campo um passo; ícones de estado; só rótulos do §8.6.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Analise, ClipeTrecho, Degravacao, Legenda, ProjetoAberto, ProjetoEdicao } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoExcluir: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoCapacidades: vi.fn(),
  edicaoDegravacao: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
const toast = vi.hoisted(() => ({ pushToast: vi.fn() }));
vi.mock("@components/toast/toastStore", () => toast);

import { useEdicaoStore } from "../store/edicaoStore";
import { Inspetor } from "../inspetor/Inspetor";
import { esquecerLeituras } from "../inspetor/contexto";
import { useInspetorUi } from "../inspetor/ui";
import { botao, campo, clicar, desmontarTudo, digitar, entradaDe, esperar, lerFixture as ler, montar, sair, textosVisiveis, type Tela } from "../inspetor/testeTela";
import { textoPermitido } from "../inspetor/rotulos.teste";
import { DURACAO_NOVA, LEGENDAS_PRONTAS } from "./PainelLegendas";

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
const doc = () => st().doc!;
const passos = () => st().passado.length;
const ultimo = () => st().passado.at(-1)?.rotulo;
const itens = () => doc().legendas.itens;
const trechos = () => doc().principal.filter((c) => (c as { tipo: string }).tipo === "trecho") as ClipeTrecho[];

const DEGRAVACAO: Degravacao = {
  origem: { audio_sha256: "a".repeat(64), importado_em: "2026-11-14T10:00:00Z", motor: "whisper-cli ggml-base.bin", modelo_sha256: null },
  trechos: [
    { de: 4.25, ate: 5, texto: "Para!", locutor: "Locutor 1", ia: true },
    { de: 5.5, ate: 6, texto: "Ok", locutor: null, ia: false },
    { de: 6, ate: 6.5, texto: " ", locutor: null, ia: false },
  ],
};

let tela: Tela;

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-14T10:00:00Z") });
  for (const f of [...Object.values(cmd), toast.pushToast]) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoTabelaQuadros.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  esquecerLeituras();
  const r: ProjetoAberto = { projeto: ler<ProjetoEdicao>("projeto-do-store.json"), importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" };
  cmd.edicaoAbrir.mockResolvedValueOnce(r);
  await st().abrir(WS, r.projeto.id);
  useInspetorUi.setState({ aba: "legendas", ferramenta: null, aoMarcar: null, chave: null });
  tela = await montar(<Inspetor />);
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

const painel = () => tela.raiz.querySelector<HTMLElement>('[data-painel="legendas"]')!;
const linhaDe = (id: string) => painel().querySelector<HTMLElement>(`[data-legenda="${id}"]`);

describe("Legendas", () => {
  it("a aba está ligada (contratos-fase1, regra 3)", () => {
    expect(LEGENDAS_PRONTAS).toBe(true);
    expect(textosVisiveis(painel())).toEqual(["Da degravação…", "Nova legenda", "Posição", "Linhas"]);
  });

  it("Da degravação…: escolhe o vídeo, um passo, tempo de fonte da mídia, IA não conferida com ícone; repetir não duplica", async () => {
    cmd.edicaoDegravacao.mockResolvedValue(DEGRAVACAO);
    const n = passos();
    await clicar(botao(painel(), "Da degravação…"));
    // dois vídeos no projeto: a escolha mostra os nomes (dado)
    const escolha = painel().querySelector('[data-escolha="degravacao"]')!;
    expect([...escolha.querySelectorAll("[data-midia]")].map((b) => b.getAttribute("data-midia"))).toEqual(["m1", "m2"]);
    await clicar(escolha.querySelector<HTMLElement>('[data-midia="m2"]')!);
    await esperar();
    expect(cmd.edicaoDegravacao.mock.calls).toEqual([[WS, doc().midias.find((m) => m.id === "m2")!.sha256]]);
    expect([passos(), ultimo()]).toEqual([n + 1, "Da degravação…"]);
    expect(itens().map(({ id: _, ...x }) => x)).toEqual<Omit<Legenda, "id">[]>([
      { texto: "Para!", locutor: "Locutor 1", origem: "ia", conferida: false, locutor_auto: true, ref: { tipo: "fonte", midia: "m2", de: 4.25, ate: 5 } },
      { texto: "Ok", locutor: null, origem: "perito", conferida: true, locutor_auto: false, ref: { tipo: "fonte", midia: "m2", de: 5.5, ate: 6 } },
    ]);
    expect(doc().legendas.origem).toEqual(DEGRAVACAO.origem);
    expect(linhaDe(itens()[0]!.id)!.querySelector('[data-estado="ia"]')).not.toBeNull();
    expect(linhaDe(itens()[1]!.id)!.querySelector('[data-estado="ia"]')).toBeNull();
    expect(painel().querySelector('[data-escolha="degravacao"]')).toBeNull();
    // de novo: nada duplica
    await clicar(botao(painel(), "Da degravação…"));
    await clicar(painel().querySelector<HTMLElement>('[data-midia="m2"]')!);
    await esperar();
    expect(itens().length).toBe(2);
  });

  it("degravação ausente: a frase do catálogo em aviso e o documento não muda", async () => {
    cmd.edicaoDegravacao.mockRejectedValue({ kind: "validation", message: "validation error: degravacao_ausente: Degravação não encontrada" });
    const antes = doc();
    await clicar(botao(painel(), "Da degravação…"));
    await clicar(painel().querySelector<HTMLElement>('[data-midia="m1"]')!);
    await esperar();
    expect(toast.pushToast.mock.calls).toEqual([["error", "Degravação não encontrada"]]);
    expect(doc()).toBe(antes);
  });

  it("Nova legenda no cursor do trecho: de fonte, aberta, vazia; Texto, Até, Conferida e Remover, um passo cada", async () => {
    const c = trechos()[0]!;
    st().posicionar({ modo: "clipe", clipe: c.id, t: 13 });
    const n = passos();
    await clicar(botao(painel(), "Nova legenda"));
    expect([passos(), ultimo()]).toEqual([n + 1, "Nova legenda"]);
    const g = itens()[0]!;
    expect(g).toMatchObject({ texto: "", origem: "perito", conferida: true, ref: { tipo: "fonte", midia: c.midia, de: 13, ate: 13 + DURACAO_NOVA } });
    expect(st().selecao.foco).toEqual({ k: "legenda", id: g.id });
    const texto = entradaDe(linhaDe(g.id)!, "Texto") as HTMLTextAreaElement;
    expect(texto.value).toBe("");
    expect(painel().querySelectorAll("[placeholder]").length).toBe(0);
    await digitar(texto, "Mãos para cima!");
    await sair(texto);
    expect([itens()[0]!.texto, passos(), ultimo()]).toEqual(["Mãos para cima!", n + 2, "Texto"]);
    await digitar(entradaDe(linhaDe(g.id)!, "Até") as HTMLInputElement, "00:00:14,500");
    await sair(entradaDe(linhaDe(g.id)!, "Até"));
    expect([itens()[0]!.ref.ate, ultimo()]).toEqual([14.5, "Até"]);
    // fim antes do início não entra
    const m = passos();
    await digitar(entradaDe(linhaDe(g.id)!, "Até") as HTMLInputElement, "00:00:12,000");
    await sair(entradaDe(linhaDe(g.id)!, "Até"));
    expect([itens()[0]!.ref.ate, passos()]).toEqual([14.5, m]);
    await clicar(entradaDe(linhaDe(g.id)!, "Conferida"));
    expect([itens()[0]!.conferida, ultimo()]).toEqual([false, "Conferida"]);
    await clicar(linhaDe(g.id)!.querySelector<HTMLElement>('[aria-label="Remover"]')!);
    expect([itens().length, ultimo()]).toEqual([0, "Remover"]);
  });

  it("Nova legenda no modo programa: de programa no slot do cursor", async () => {
    st().posicionar({ modo: "programa", q: 45 });
    await clicar(botao(painel(), "Nova legenda"));
    expect(itens()[0]!.ref).toEqual({ tipo: "programa", de: 1.5, ate: 1.5 + DURACAO_NOVA });
  });

  it("Posição e Linhas da trilha, um passo cada; Linhas fora de 1..4 não entra", async () => {
    const n = passos();
    await clicar(campo(painel(), "Posição").querySelector<HTMLElement>('[data-valor="centro-topo"]')!);
    expect([doc().legendas.lugar, ultimo()]).toEqual([{ x: 0.5, y: 0.03, h: "centro", v: "topo" }, "Posição"]);
    await digitar(entradaDe(painel(), "Linhas") as HTMLInputElement, "3");
    expect([doc().legendas.linhas_max, passos(), ultimo()]).toEqual([3, n + 2, "Linhas"]);
    await digitar(entradaDe(painel(), "Linhas") as HTMLInputElement, "9");
    expect(doc().legendas.linhas_max).toBe(3);
  });

  it("pelo espelho do motor: ordem de aparição, ícone de fora do programa e a linha do problema", async () => {
    const [c1, c2] = trechos() as [ClipeTrecho, ClipeTrecho];
    cmd.edicaoDegravacao.mockResolvedValue({ ...DEGRAVACAO, trechos: [{ de: 20, ate: 21, texto: "Fora", locutor: null, ia: false }, { de: 13, ate: 14, texto: "Dentro", locutor: null, ia: true }] });
    await clicar(botao(painel(), "Da degravação…"));
    await clicar(painel().querySelector<HTMLElement>('[data-midia="m1"]')!);
    await esperar();
    const [fora, dentro] = itens() as [Legenda, Legenda];
    // c1 = m1 de 12 a 15,5 s no slot 90 do programa; c2 = m2
    const linha: Analise["linha"] = {
      fps: { num: 30, den: 1 },
      total_q: 345,
      clipes: [
        { id: doc().principal[0]!.id as string, inicio_q: 0, n_q: 90, sobrepoe_q: 0, fonte: null },
        { id: c1.id, inicio_q: 90, n_q: 105, sobrepoe_q: 0, fonte: { midia: "m1", j_in: 360, j_out: 465, pts_in: 0, pts_out: null, lidos: 105, repetidos: 0, descartados: 0, tabela: "decodificador" } },
        { id: c2.id, inicio_q: 195, n_q: 150, sobrepoe_q: 0, fonte: { midia: "m2", j_in: 90, j_out: 240, pts_in: 0, pts_out: null, lidos: 150, repetidos: 0, descartados: 0, tabela: "decodificador" } },
      ],
      livres: [],
    };
    const problema = { nivel: "erro" as const, codigo: "legenda_nao_conferida", mensagem: "Legenda automática não conferida", clipe: dentro.id, efeito: null, trilha: null, midia: null, campo: "legendas", n: null };
    await esperar(() => useEdicaoStore.setState({ analise: { problemas: [problema], linha, midias: [], prender: [], saida_auto: null } }));
    expect([...painel().querySelectorAll("[data-legenda]")].map((x) => x.getAttribute("data-legenda"))).toEqual([dentro.id, fora.id]);
    expect(linhaDe(fora.id)!.querySelector('[data-estado="fora"]')).not.toBeNull();
    expect(linhaDe(dentro.id)!.querySelector('[data-estado="fora"]')).toBeNull();
    expect(linhaDe(dentro.id)!.querySelector('[data-problema="legenda_nao_conferida"]')!.textContent).toBe("Legenda automática não conferida");
  });

  it("rótulos: só os do §8.6 (com a escolha do vídeo e uma legenda aberta), nenhum texto de exemplo", async () => {
    cmd.edicaoDegravacao.mockResolvedValue(DEGRAVACAO);
    await clicar(botao(painel(), "Da degravação…"));
    const conferir = () => {
      expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
      expect(tela.raiz.querySelectorAll("[placeholder]").length).toBe(0);
    };
    conferir();
    await clicar(painel().querySelector<HTMLElement>('[data-midia="m1"]')!);
    await esperar();
    await clicar(linhaDe(itens()[0]!.id)!.querySelector<HTMLElement>('[data-acao="abrir-legenda"]')!);
    expect(entradaDe(linhaDe(itens()[0]!.id)!, "Texto")).toBeTruthy();
    conferir();
  });
});
