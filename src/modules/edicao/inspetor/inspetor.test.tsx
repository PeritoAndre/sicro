// @vitest-environment happy-dom
/**
 * Inspetor montado de verdade (happy-dom), editando pela store: cada mudança de campo vira UM passo com o rótulo do
 * campo; arrasto de controle deslizante e digitação = um passo; valor inválido não entra; item desconhecido ou
 * inválido aparece só para leitura; nenhum texto fora do §8.6 e nenhum texto de exemplo.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeCongelar, ClipeTitulo, ClipeTrecho, EfDestaque, EfEnquadramento, EfGirar, EfOcultar, EfTexto, MidiaRef, ProjetoAberto, ProjetoEdicao, Problema } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoExcluir: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoMidiaImportar: vi.fn(),
  edicaoCapacidades: vi.fn(),
  edicaoGravacaoSalvar: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => "/fora/arquivo") }));
const ask = vi.hoisted(() => ({ askConfirm: vi.fn(async (_o: { title: string; message?: string; confirmLabel?: string; danger?: boolean }) => true) }));
vi.mock("@components/Dialog/ask", () => ask);

import { useToastStore } from "@components/toast/toastStore";
import { useEdicaoStore } from "../store/edicaoStore";
import { useFundoStore } from "../store/fundoStore";
import { Inspetor } from "./Inspetor";
import { esquecerLeituras, useTabela } from "./contexto";
import { marcacoesNoEspaco, removerEfeito } from "./acoes";
import { useInspetorUi } from "./ui";
import { arrastar, botao, campo, clicar, desmontarTudo, digitar, entradaDe, escolher, esperar, lerFixture as ler, montar, sair, tecla, textosVisiveis, type Tela } from "./testeTela";
import { ROTULOS, textoPermitido } from "./rotulos.teste";
import { ADICIONAR, ADICIONAR_PROPRIO, EDITAVEIS, REGISTRO, secoesDe } from "./registro";
import { FECHO_PRONTO, EntradaSecao } from "./secoes/EntradaSecao";
import { LUPA_PRONTA, LupaEditor } from "./secoes/LupaEditor";
import { REGUA_PRONTA, ReguaEditor } from "./secoes/ReguaEditor";
import { VELOCIDADE_PRONTA, VelocidadeEditor } from "./secoes/VelocidadeEditor";
import { REALCE_PRONTO, RealceSecao } from "./secoes/RealceSecao";
import { AUDIO_CLIPE_PRONTO, AudioClipeSecao } from "./secoes/AudioClipeSecao";
import { SELOS_PRONTOS } from "./secoes/SelosSecao";
import { TrechoSecao } from "./secoes/TrechoSecao";
import { EnquadramentoSecao } from "./secoes/EnquadramentoSecao";
import { MarcacoesSecao } from "./secoes/MarcacoesSecao";
import { TituloSecao } from "./secoes/TituloSecao";
import { PAINEL_AUDIO_PRONTO } from "../audio/PainelAudio";
import { EDITOR_TITULO_PRONTO } from "../titulos/EditorTitulo";
import { PRESOS_PRONTOS } from "../sequencia/Presos";
import { LEGENDAS_PRONTAS } from "../legendas/PainelLegendas";

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
const passos = () => st().passado.length;
const ultimo = () => st().passado.at(-1)?.rotulo;

async function abrirCom(projeto: ProjetoEdicao, avisos: Problema[] = []) {
  const r: ProjetoAberto = { projeto, importado_de: null, so_leitura: false, avisos, raiz_cache: "/cache/c1" };
  cmd.edicaoAbrir.mockResolvedValueOnce(r);
  await st().abrir(WS, projeto.id);
}
const selecionar = (k: "clipe" | "livre" | "audio", id: string, trilha?: string) => {
  const it = { k, id, ...(trilha ? { trilha } : {}) };
  st().selecionar({ itens: [it], foco: it, efeito: null });
};
const doc = () => st().doc!;
const selecionarEfeitoAberto = (efeito: string) => st().selecionar({ ...st().selecao, efeito });
const principal = () => doc().principal as [ClipeTitulo, ClipeTrecho, ClipeTrecho, ...ClipeCongelar[]];

let tela: Tela;

beforeEach(async () => {
  // só o relógio de parede: o `act` do React espera por timers de verdade
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-14T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoTabelaQuadros.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  esquecerLeituras();
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });
  await abrirCom(ler<ProjetoEdicao>("projeto-do-store.json"));
  tela = await montar(<Inspetor />);
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

/** Mais de 1 s depois: a próxima mudança do mesmo campo é outro passo. */
const depois = async () => vi.setSystemTime(Date.now() + 1500);

describe("abas", () => {
  it("quatro abas; Legendas vazia nesta fase; sem seleção, a aba Clipe fica vazia", async () => {
    const abas = [...tela.raiz.querySelectorAll('[role="tab"]')].map((x) => x.textContent);
    expect(abas).toEqual(["Clipe", "Áudio", "Legendas", "Saída"]);
    expect(tela.raiz.querySelector('[data-painel="clipe"]')!.textContent).toBe("");
    await clicar(botao(tela.raiz, "Legendas"));
    // vazia até o P1.6 ligar a aba (contratos-fase1, regra 2)
    if (!LEGENDAS_PRONTAS) expect(textosVisiveis(tela.raiz.querySelector('[data-painel="legendas"]')!)).toEqual([]);
  });

  it("foco num clipe de narração abre Áudio; num clipe, abre Clipe", async () => {
    const t = doc().audios[0]!;
    await esperar(() => selecionar("audio", t.clipes[0]!.id, t.id));
    expect(useInspetorUi.getState().aba).toBe("audio");
    await esperar(() => selecionar("clipe", principal()[1].id));
    expect(useInspetorUi.getState().aba).toBe("clipe");
  });
});

describe("Clipe · Trecho", () => {
  beforeEach(async () => {
    await esperar(() => selecionar("clipe", principal()[1].id));
  });

  it("Nome: digitar é um passo só, com prévia viva; Esc desfaz", async () => {
    const n = passos();
    const nome = entradaDe(tela.raiz, "Nome") as HTMLInputElement;
    await digitar(nome, "Chegada do veículo");
    expect(principal()[1].rotulo).toBe("Chegada do veículo");
    expect(passos()).toBe(n);
    await sair(nome);
    expect(passos()).toBe(n + 1);
    expect(ultimo()).toBe("Nome");
    await digitar(nome, "x");
    await tecla(nome, "Escape");
    expect(principal()[1].rotulo).toBe("Chegada do veículo");
    expect(passos()).toBe(n + 1);
  });

  it("Cor da mídia: escolher grava no MidiaRef (vale em todo clipe dela), um passo; Automática volta à da fonte (O4/F5)", async () => {
    const c = principal()[1];
    const midia = () => doc().midias.find((m) => m.id === c.midia)!;
    expect(midia().cor).toBeNull();
    const cor = entradaDe(tela.raiz, "Cor") as HTMLSelectElement;
    expect(cor.value).toBe("");
    const n = passos();
    await escolher(cor, "bt709:pc");
    expect(midia().cor).toEqual({ matriz: "bt709", faixa: "pc" });
    expect((entradaDe(tela.raiz, "Cor") as HTMLSelectElement).value).toBe("bt709:pc");
    expect([passos(), ultimo()]).toEqual([n + 1, "Cor"]);
    await clicar(botao(tela.raiz, "Automática"));
    expect(midia().cor).toBeNull();
    expect([passos(), ultimo()]).toEqual([n + 2, "Cor"]);
    expect(() => botao(tela.raiz, "Automática")).toThrow();
  });

  it("Início e Fim: um passo cada; início depois do fim não entra", async () => {
    const n = passos();
    const ini = entradaDe(tela.raiz, "Início") as HTMLInputElement;
    await digitar(ini, "13,25", { aos: 5 });
    await tecla(ini, "Enter");
    expect(principal()[1]).toMatchObject({ inicio_s: 13.25, inicio_pts: null });
    expect([passos(), ultimo()]).toEqual([n + 1, "Início"]);
    await depois();
    await digitar(ini, "20", { aos: 5 });
    await tecla(ini, "Enter");
    expect(principal()[1].inicio_s).toBe(13.25);
    expect(passos()).toBe(n + 1);
    expect(campo(tela.raiz, "Início").className).toMatch(/ruim/);
    const fim = entradaDe(tela.raiz, "Fim") as HTMLInputElement;
    await digitar(fim, "15", { aos: 5 });
    await tecla(fim, "Enter");
    expect(principal()[1].fim_s).toBe(15);
    expect([passos(), ultimo()]).toEqual([n + 2, "Fim"]);
  });

  it("Velocidade: digitação seguida é um passo; mais de 3 casas não entra", async () => {
    const n = passos();
    const v = entradaDe(tela.raiz, "Velocidade") as HTMLInputElement;
    await digitar(v, "0,25");
    expect(principal()[1].velocidade).toEqual({ tipo: "constante", fator: 0.25 });
    expect([passos(), ultimo()]).toEqual([n + 1, "Velocidade"]);
    await depois();
    // colado de uma vez: 4 casas não entram (o racional exato do Rust pede até 3)
    await digitar(v, "0,2501", { aos: 6 });
    expect(principal()[1].velocidade).toEqual({ tipo: "constante", fator: 0.25 });
    expect(campo(tela.raiz, "Velocidade").className).toMatch(/ruim/);
    expect(passos()).toBe(n + 1);
  });

  it("Som: arrasto do volume = um passo; mudo e Manter o tom = um passo cada", async () => {
    const n = passos();
    const r = campo(tela.raiz, "Som").querySelector<HTMLInputElement>('input[type="range"]')!;
    await arrastar(r, [0.8, 0.6, 1.4, 1.5], { entre: depois });
    expect(principal()[1].audio.volume).toBe(1.5);
    expect([passos(), ultimo()]).toEqual([n + 1, "Som"]);
    await clicar(campo(tela.raiz, "Som").querySelector<HTMLButtonElement>("button")!);
    expect(principal()[1].audio.mudo).toBe(true);
    expect([passos(), ultimo()]).toEqual([n + 2, "Som"]);
    await clicar(entradaDe(tela.raiz, "Manter o tom") as HTMLInputElement);
    expect(principal()[1].audio.manter_tom).toBe(false);
    expect([passos(), ultimo()]).toEqual([n + 3, "Manter o tom"]);
    st().desfazer();
    st().desfazer();
    st().desfazer();
    expect(principal()[1].audio).toMatchObject({ volume: 1, mudo: false, manter_tom: true });
  });
});

describe("Clipe · Entrada", () => {
  it("Transição Corte ↔ Escurecer e Duração (s)", async () => {
    const c = principal()[1];
    await esperar(() => selecionar("clipe", c.id));
    const n = passos();
    await clicar(botao(campo(tela.raiz, "Transição"), "Escurecer"));
    expect(principal()[1].transicao_entrada).toEqual({ tipo: "escurecer", duracao_s: 0.5, audio: "acompanhar" });
    expect([passos(), ultimo()]).toEqual([n + 1, "Transição"]);
    await digitar(entradaDe(tela.raiz, "Duração (s)") as HTMLInputElement, "1,5");
    expect(principal()[1].transicao_entrada?.duracao_s).toBe(1.5);
    expect([passos(), ultimo()]).toEqual([n + 2, "Duração (s)"]);
    await clicar(botao(campo(tela.raiz, "Transição"), "Corte"));
    expect(principal()[1].transicao_entrada).toBeNull();
    expect(passos()).toBe(n + 3);
  });
});

describe("Clipe · Enquadramento", () => {
  it("Girar, Espelhar, Ajuste, Interpolação, Marcar início/fim", async () => {
    const id = principal()[2].id;
    await esperar(() => selecionar("clipe", id));
    const efs = () => principal()[2].efeitos;
    const g = () => efs().find((e) => (e as EfGirar).tipo === "girar") as EfGirar | undefined;
    const e = () => efs().find((x) => (x as EfEnquadramento).tipo === "enquadramento") as EfEnquadramento;
    const n = passos();
    await clicar(botao(campo(tela.raiz, "Girar"), "180°"));
    expect(g()?.graus).toBe(180);
    expect(ultimo()).toBe("Girar");
    await clicar(campo(tela.raiz, "Espelhar").querySelector<HTMLButtonElement>('[data-valor="h"]')!);
    expect(g()?.espelhar_h).toBe(true);
    expect(ultimo()).toBe("Espelhar");
    await clicar(botao(campo(tela.raiz, "Ajuste"), "Preencher"));
    expect(e().ajuste).toBe("cobrir");
    // região animada: sem "Vizinho"
    expect(() => botao(campo(tela.raiz, "Interpolação"), "Vizinho")).toThrow();
    await clicar(botao(campo(tela.raiz, "Interpolação"), "Bicúbica"));
    expect(e().interpolacao).toBe("bicubica");
    expect(passos()).toBe(n + 4);
    // marcar o fim de novo pelo palco
    await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="marcar-fim"]')!);
    expect(useInspetorUi.getState().ferramenta).toMatchObject({ tipo: "regiao", efeito: e().id, proporcao: 16 / 9 });
    await esperar(() => useInspetorUi.getState().marcado({ x: 0.1, y: 0.1, w: 0.3, h: 0.3 }, 7, null));
    expect(e().regiao).toEqual([
      { t: 3, v: { x: 0, y: 0, w: 1, h: 1 } },
      { t: 7, v: { x: 0.1, y: 0.1, w: 0.3, h: 0.3 } },
    ]);
    expect([passos(), ultimo()]).toEqual([n + 5, "Marcar fim"]);
    // tirar a animação volta a valor fixo (nunca lista vazia)
    await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="marcar-fim"]')!.parentElement!.querySelector<HTMLButtonElement>('[aria-label="Remover"]')!);
    expect(e().regiao).toEqual({ x: 0, y: 0, w: 1, h: 1 });
    expect(ultimo()).toBe("Marcar fim");
  });

  it("girar a 0° sem espelho tira o efeito", async () => {
    const id = principal()[1].id;
    await esperar(() => selecionar("clipe", id));
    await clicar(botao(campo(tela.raiz, "Girar"), "0°"));
    expect(principal()[1].efeitos.some((e) => (e as EfGirar).tipo === "girar")).toBe(false);
    expect(ultimo()).toBe("Girar");
  });
});

describe("Clipe · Marcações", () => {
  const abrirEfeito = async (tipo: string) => {
    const linha = tela.raiz.querySelector<HTMLElement>(`[data-efeito="${tipo}"]`)!;
    if (!linha.className.match(/efeitoAberto/)) await clicar(linha.querySelector("button")!);
    return tela.raiz.querySelector<HTMLElement>(`[data-efeito="${tipo}"]`)!;
  };

  it("Adicionar: Destaque, Ocultar, Texto, Tempo; um passo cada, com o rótulo do §8.6", async () => {
    const id = principal()[1].id;
    await esperar(() => selecionar("clipe", id));
    const menu = async (r: string) => {
      await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="adicionar"]')!);
      const item = [...document.querySelectorAll<HTMLElement>("button, [role=menuitem]")].find((x) => x.textContent?.trim() === r)!;
      expect(item).toBeTruthy();
      await clicar(item);
    };
    const n = passos();
    const nr = doc().rastreios.length;
    const no = doc().midias[0]!.ocultacoes.length;
    const nc = principal()[1].efeitos.filter((e) => (e as { tipo: string }).tipo === "ocultar").length;
    for (const r of ["Destaque", "Ocultar", "Texto", "Tempo"]) await menu(r);
    expect(passos()).toBe(n + 4);
    expect(st().passado.slice(-4).map((p) => p.rotulo)).toEqual(["Destaque", "Ocultar", "Texto", "Tempo"]);
    expect(doc().rastreios.length).toBe(nr + 2);
    // ocultar de trecho é da mídia, com `ativo` em tempo de fonte; o clipe não ganha ocultar
    expect(doc().midias[0]!.ocultacoes).toHaveLength(no + 1);
    expect(doc().midias[0]!.ocultacoes.at(-1)).toMatchObject({ tipo: "ocultar", ativo: { de: 12, ate: 15.5 } });
    expect(principal()[1].efeitos.filter((e) => (e as { tipo: string }).tipo === "ocultar")).toHaveLength(nc);
  });

  it("Destaque: Forma, Cor, Espessura, Rótulo, Marcar", async () => {
    const id = principal()[1].id;
    await esperar(() => selecionar("clipe", id));
    const linha = await abrirEfeito("destaque");
    const d = () => principal()[1].efeitos.find((e) => (e as EfDestaque).tipo === "destaque") as EfDestaque;
    const n = passos();
    await clicar(campo(linha, "Forma").querySelector<HTMLButtonElement>('[data-valor="seta"]')!);
    expect(d().forma).toBe("seta");
    expect(ultimo()).toBe("Forma");
    await digitar(entradaDe(linha, "Espessura") as HTMLInputElement, "1,2");
    expect(d().traco.espessura).toBeCloseTo(0.012, 12);
    expect(ultimo()).toBe("Espessura");
    await digitar(entradaDe(linha, "Direção") as HTMLInputElement, "135");
    expect(d().angulo_graus).toBe(135);
    const rot = entradaDe(linha, "Rótulo") as HTMLInputElement;
    await digitar(rot, "Suspeito");
    await sair(rot);
    expect(d().rotulo?.texto).toBe("Suspeito");
    expect([passos(), ultimo()]).toEqual([n + 4, "Rótulo"]);
    await clicar(linha.querySelector<HTMLButtonElement>('[data-acao="marcar"]')!);
    const f = useInspetorUi.getState().ferramenta!;
    expect(f).toMatchObject({ tipo: "caixa", rastreio: d().rastreio, efeito: d().id });
    await esperar(() => useInspetorUi.getState().marcado({ x: 0.5, y: 0.5, w: 0.1, h: 0.1 }, 14, 14 * 30 * 512));
    const r = doc().rastreios.find((x) => x.id === d().rastreio)!;
    expect(r.ancoras.map((a) => a.t)).toEqual([12, 14]);
    expect([passos(), ultimo()]).toEqual([n + 5, "Marcar"]);
  });

  it("Ocultar: Modo, Intensidade (arrasto = um passo), Margem negativa não entra, Só neste clipe", async () => {
    const c2 = principal()[2];
    await esperar(() => selecionar("clipe", c2.id));
    const linha = await abrirEfeito("ocultar");
    const o = () => principal()[2].efeitos.find((e) => (e as EfOcultar).tipo === "ocultar") as EfOcultar | undefined;
    const n = passos();
    await clicar(botao(campo(linha, "Modo"), "Mosaico"));
    expect(o()?.modo).toBe("mosaico");
    const r = campo(linha, "Intensidade").querySelector<HTMLInputElement>('input[type="range"]')!;
    await arrastar(r, [0.1, 0.3, 0.9], { entre: depois });
    expect(o()?.intensidade).toBe(0.9);
    expect([passos(), ultimo()]).toEqual([n + 2, "Intensidade"]);
    await escolher(entradaDe(linha, "Conteúdo") as HTMLSelectElement, "documento");
    expect(o()?.conteudo).toBe("documento");
    const motivo = entradaDe(linha, "Motivo") as HTMLTextAreaElement;
    await digitar(motivo, "Dado pessoal de terceiro");
    await sair(motivo);
    expect(o()?.motivo).toBe("Dado pessoal de terceiro");
    expect([passos(), ultimo()]).toEqual([n + 4, "Motivo"]);
    // só neste clipe → da mídia: `ativo` em tempo de fonte
    await clicar(entradaDe(linha, "Só neste clipe") as HTMLInputElement);
    expect(o()).toBeUndefined();
    expect(doc().midias[1]!.ocultacoes.at(-1)).toMatchObject({ modo: "mosaico", ativo: { de: 3, ate: 8 } });
    expect([passos(), ultimo()]).toEqual([n + 5, "Só neste clipe"]);
  });

  it("Ocultar: Margem abaixo de zero não entra", async () => {
    const c1 = principal()[1];
    await esperar(() => selecionar("clipe", c1.id));
    const linha = await abrirEfeito("ocultar");
    const n = passos();
    const antes = doc().midias[0]!.ocultacoes[0]!.alvo.margem;
    await digitar(entradaDe(linha, "Margem") as HTMLInputElement, "-5");
    expect(doc().midias[0]!.ocultacoes[0]!.alvo.margem).toBe(antes);
    expect(passos()).toBe(n);
  });

  it("Texto: Texto, Fonte, Tamanho, Cor, Posição e ligar/desligar", async () => {
    const c2 = principal()[2];
    await esperar(() => selecionar("clipe", c2.id));
    const linha = await abrirEfeito("texto");
    const t = () => principal()[2].efeitos.find((e) => (e as EfTexto).tipo === "texto") as EfTexto;
    const n = passos();
    const txt = entradaDe(linha, "Texto") as HTMLTextAreaElement;
    await digitar(txt, "\nPlaca ABC1D23");
    await sair(txt);
    expect(t().conteudo).toBe("\nPlaca ABC1D23");
    await clicar(campo(linha, "Fonte").querySelectorAll<HTMLButtonElement>("button")[2]!);
    expect(t().estilo.fonte).toBe("mono");
    await digitar(entradaDe(linha, "Tamanho") as HTMLInputElement, "7,8");
    expect(t().estilo.tamanho).toBeCloseTo(0.078, 12);
    await clicar(campo(linha, "Posição").querySelector<HTMLButtonElement>('[data-valor="dir-topo"]')!);
    expect(t().lugar).toEqual({ x: 0.98, y: 0.03, h: "dir", v: "topo" });
    expect(st().passado.slice(-4).map((p) => p.rotulo)).toEqual(["Texto", "Fonte", "Tamanho", "Posição"]);
    expect(passos()).toBe(n + 4);
    await clicar(linha.querySelector<HTMLButtonElement>('[aria-pressed="true"][aria-label="Texto"]')!);
    expect(t().ligado).toBe(false);
  });

  it("Tempo (carimbo): Mostrar; Imagem: Adicionar pelo arquivo do perito", async () => {
    const id = principal()[1].id;
    await esperar(() => selecionar("clipe", id));
    const img: MidiaRef = { ...doc().midias[2]!, id: "m4", tipo: "imagem", nome: "brasao.png", sha256: "4".repeat(64), duracao_s: null, tem_audio: false };
    cmd.edicaoMidiaImportar.mockResolvedValueOnce(img);
    await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="adicionar"]')!);
    await clicar([...document.querySelectorAll<HTMLElement>("button")].find((x) => x.textContent?.trim() === "Imagem")!);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(cmd.edicaoMidiaImportar).toHaveBeenCalledWith(WS, doc().id, "/fora/arquivo", "imagem");
    expect(doc().midias.at(-1)!.id).toBe("m4");
    expect(principal()[1].efeitos.at(-1)).toMatchObject({ tipo: "imagem", midia: "m4" });
    expect(ultimo()).toBe("Imagem");
    await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="adicionar"]')!);
    await clicar([...document.querySelectorAll<HTMLElement>("button")].find((x) => x.textContent?.trim() === "Tempo")!);
    const linha = await abrirEfeito("carimbo");
    await escolher(entradaDe(linha, "Mostrar") as HTMLSelectElement, "quadro");
    expect(principal()[1].efeitos.at(-1)).toMatchObject({ tipo: "carimbo", mostra: "quadro" });
    expect(ultimo()).toBe("Mostrar");
  });

  it("Adicionar que falha mostra o erro no aviso do app (nada calado): o diálogo de arquivo que não abre", async () => {
    const id = principal()[1].id;
    await esperar(() => selecionar("clipe", id));
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockRejectedValueOnce(new Error("portal de arquivos indisponível"));
    useToastStore.getState().clear();
    const n = passos();
    await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="adicionar"]')!);
    await clicar([...document.querySelectorAll<HTMLElement>("button")].find((x) => x.textContent?.trim() === "Imagem")!);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(useToastStore.getState().toasts.map((t) => [t.kind, t.message])).toEqual([["error", "portal de arquivos indisponível"]]);
    expect(passos()).toBe(n);
    useToastStore.getState().clear();
    // o "Adicionar" próprio de um pacote (lupa, régua, velocidade) que rejeita: a frase do catálogo
    const lupa = ADICIONAR_PROPRIO.lupa;
    ADICIONAR_PROPRIO.lupa = async () => Promise.reject({ kind: "validation", message: "validation error: calibracao_ausente: Calibração não encontrada" });
    try {
      await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="adicionar"]')!);
      await clicar([...document.querySelectorAll<HTMLElement>("button")].find((x) => x.textContent?.trim() === "Lupa")!);
      await esperar(() => new Promise((r) => setTimeout(r, 0)));
    } finally {
      ADICIONAR_PROPRIO.lupa = lupa;
    }
    expect(useToastStore.getState().toasts.map((t) => [t.kind, t.message])).toEqual([["error", "Calibração não encontrada"]]);
    useToastStore.getState().clear();
  });

  it("De e Até: um passo cada; Até antes de De não entra; Remover", async () => {
    const id = principal()[2].id;
    await esperar(() => selecionar("clipe", id));
    const linha = await abrirEfeito("texto");
    const t = () => principal()[2].efeitos.find((e) => (e as EfTexto).tipo === "texto") as EfTexto | undefined;
    const n = passos();
    const de = entradaDe(linha, "De") as HTMLInputElement;
    await digitar(de, "4", { aos: 5 });
    await tecla(de, "Enter");
    expect(t()?.ativo).toEqual({ de: 4, ate: null });
    const ate = entradaDe(linha, "Até") as HTMLInputElement;
    await digitar(ate, "3", { aos: 5 });
    await tecla(ate, "Enter");
    expect(t()?.ativo).toEqual({ de: 4, ate: null });
    await digitar(ate, "6", { aos: 5 });
    await tecla(ate, "Enter");
    expect(t()?.ativo).toEqual({ de: 4, ate: 6 });
    expect(st().passado.slice(-2).map((p) => p.rotulo)).toEqual(["De", "Até"]);
    await clicar(linha.querySelector<HTMLButtonElement>('[aria-label="Remover"]')!);
    expect(t()).toBeUndefined();
    expect([passos(), ultimo()]).toEqual([n + 3, "Remover"]);
  });
});

describe("Título e quadro congelado", () => {
  it("Título: Modelo, Subtítulo (cria o bloco), Fundo, Fonte, Tamanho escala os blocos, Duração (s)", async () => {
    const t = principal()[0];
    await esperar(() => selecionar("clipe", t.id));
    const n = passos();
    await escolher(entradaDe(tela.raiz, "Modelo") as HTMLSelectElement, "encerramento");
    expect(principal()[0].titulo.modelo).toBe("encerramento");
    const sub = entradaDe(tela.raiz, "Subtítulo") as HTMLInputElement;
    await digitar(sub, "Laudo 4567");
    await sair(sub);
    expect(principal()[0].titulo.blocos.map((b) => b.papel)).toEqual(["titulo", "rodape", "subtitulo"]);
    const tam0 = principal()[0].titulo.blocos.map((b) => b.estilo.tamanho);
    await digitar(entradaDe(tela.raiz, "Tamanho") as HTMLInputElement, "3,9");
    const tam1 = principal()[0].titulo.blocos.map((b) => b.estilo.tamanho);
    expect(tam1[0]).toBeCloseTo(0.039, 12);
    expect(tam1[1]! / tam0[1]!).toBeCloseTo(0.5, 12);
    await digitar(entradaDe(tela.raiz, "Duração (s)") as HTMLInputElement, "4");
    expect(principal()[0].duracao_s).toBe(4);
    expect(st().passado.slice(-4).map((p) => p.rotulo)).toEqual(["Modelo", "Subtítulo", "Tamanho", "Duração (s)"]);
    expect(passos()).toBe(n + 4);
    // apagar o texto tira o bloco
    await digitar(sub, "");
    await sair(sub);
    expect(principal()[0].titulo.blocos.map((b) => b.papel)).toEqual(["titulo", "rodape"]);
  });

  it("Quadro congelado: Posição e Duração (s); duração zero não entra", async () => {
    st().alterar("Congelar aqui", (d) =>
      void d.principal.push({ tipo: "congelar", id: "cg1", rotulo: "", desligado: false, transicao_entrada: null, efeitos: [], midia: "m1", instante_s: 13, instante_pts: null, duracao_s: 2 }),
    );
    await esperar(() => selecionar("clipe", "cg1"));
    const cg = () => doc().principal.find((c) => (c as ClipeCongelar).id === "cg1") as ClipeCongelar;
    const n = passos();
    const pos = entradaDe(tela.raiz, "Posição") as HTMLInputElement;
    await digitar(pos, "14,5", { aos: 5 });
    await tecla(pos, "Enter");
    expect(cg()).toMatchObject({ instante_s: 14.5, instante_pts: null });
    await digitar(entradaDe(tela.raiz, "Duração (s)") as HTMLInputElement, "3");
    expect(cg().duracao_s).toBe(3);
    expect(st().passado.slice(-2).map((p) => p.rotulo)).toEqual(["Posição", "Duração (s)"]);
    await depois();
    await digitar(entradaDe(tela.raiz, "Duração (s)") as HTMLInputElement, "0");
    expect(cg().duracao_s).toBe(3);
    expect(passos()).toBe(n + 2);
  });
});

describe("Áudio", () => {
  it("Volume do original, volume da narração (arrasto = um passo), Preso a, Entrada (s), Importar…", async () => {
    await clicar(botao(tela.raiz, "Áudio"));
    const n = passos();
    await arrastar(campo(tela.raiz, "Volume do original").querySelector<HTMLInputElement>('input[type="range"]')!, [0.5, 0.4], { entre: depois });
    expect(doc().mixagem.volume_original).toBe(0.4);
    expect([passos(), ultimo()]).toEqual([n + 1, "Volume do original"]);
    await arrastar(tela.raiz.querySelector<HTMLInputElement>('input[type="range"][aria-label="Narração"]')!, [1.2, 1.6], { entre: depois });
    expect(doc().audios[0]!.volume).toBe(1.6);
    expect([passos(), ultimo()]).toEqual([n + 2, "Narração"]);
    // o clipe de narração abre pelo foco (seleção, sem passo)
    expect(tela.raiz.querySelector('[data-campo="Entrada (s)"]')).toBeNull();
    await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="abrir-narracao"]')!);
    expect(st().selecao.foco).toMatchObject({ k: "audio", id: doc().audios[0]!.clipes[0]!.id });
    expect(passos()).toBe(n + 2);
    await escolher(entradaDe(tela.raiz, "Preso a") as HTMLSelectElement, principal()[2].id);
    expect(doc().audios[0]!.clipes[0]!.pos).toEqual({ ref: "clipe", clipe: principal()[2].id, desloc: 0 });
    await digitar(entradaDe(tela.raiz, "Entrada (s)") as HTMLInputElement, "0,5");
    expect(doc().audios[0]!.clipes[0]!.entrada_s).toBe(0.5);
    expect(st().passado.slice(-2).map((p) => p.rotulo)).toEqual(["Preso a", "Entrada (s)"]);
    const outra: MidiaRef = { ...doc().midias[2]!, id: "m5", nome: "narracao2.wav", sha256: "5".repeat(64) };
    cmd.edicaoMidiaImportar.mockResolvedValueOnce(outra);
    await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="importar"]')!);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(doc().audios[0]!.clipes.map((c) => c.midia)).toEqual(["m3", "m5"]);
    expect([passos(), ultimo()]).toEqual([n + 5, "Importar…"]);
    // sem microfone no ambiente: Gravar fica desligado
    expect(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="gravar"]')!.disabled).toBe(true);
  });
});

describe("Saída", () => {
  it("Automática; Largura ímpar não entra; Largura, Quadros/s, Fundo e Áudio: um passo cada e desligam a automática", async () => {
    await clicar(botao(tela.raiz, "Saída"));
    const n = passos();
    const larg = entradaDe(tela.raiz, "Largura") as HTMLInputElement;
    await digitar(larg, "1279");
    expect(doc().saida.largura).toBe(1920);
    expect(passos()).toBe(n);
    expect(campo(tela.raiz, "Largura").className).toMatch(/ruim/);
    await depois();
    await digitar(larg, "1280");
    expect(doc().saida).toMatchObject({ largura: 1280, auto: false });
    expect([passos(), ultimo()]).toEqual([n + 1, "Largura"]);
    await escolher(entradaDe(tela.raiz, "Quadros/s") as HTMLSelectElement, "25/1");
    expect(doc().saida.fps).toEqual({ num: 25, den: 1 });
    expect(ultimo()).toBe("Quadros/s");
    await clicar(entradaDe(tela.raiz, "Automática") as HTMLInputElement);
    expect(doc().saida.auto).toBe(true);
    expect(ultimo()).toBe("Automática");
    await clicar(entradaDe(tela.raiz, "Áudio") as HTMLInputElement);
    expect(doc().saida.audio).toBe(false);
    expect(passos()).toBe(n + 4);
    expect([...(campo(tela.raiz, "Quadros/s").querySelectorAll("option") as NodeListOf<HTMLOptionElement>)].map((o) => o.textContent)).toEqual(["12,5", "15", "23,976", "24", "25", "29,97", "30", "50", "59,94", "60"]);
  });
});

describe("item desconhecido ou inválido", () => {
  it("clipe e efeito de versão futura: só o tipo, sem editor, sem quebrar; item_invalido idem", async () => {
    desmontarTudo();
    await st().fechar({ descartar: true });
    const p = ler<ProjetoEdicao>("projeto-desconhecido.json");
    const invalido: Problema = { nivel: "erro", codigo: "item_invalido", mensagem: "", clipe: "c1", efeito: "e8", trilha: null, midia: null, campo: null, n: null };
    await abrirCom(p, [invalido]);
    tela = await montar(<Inspetor />);
    await esperar(() => selecionar("clipe", "c9"));
    expect(tela.raiz.querySelector('[data-desconhecido="clipe_do_futuro"]')).toBeTruthy();
    expect(tela.raiz.querySelectorAll("input, select, textarea").length).toBe(0);
    await esperar(() => selecionar("clipe", "c1"));
    const linhas = [...tela.raiz.querySelectorAll<HTMLElement>("[data-efeito]")];
    // Realce: e1 (nitidez conhecida, sem editor nesta fase) e e8 (nitidez que o Rust acusa, item_invalido: apagada);
    // Marcações: e9 (versão futura, apagada)
    expect(linhas.map((l) => [l.dataset.efeito, /efeitoApagado/.test(l.className)])).toEqual([
      ["nitidez", false],
      ["nitidez", true],
      ["efeito_do_futuro", true],
    ]);
    // a linha apagada não abre editor
    const futuro = linhas[2]!;
    await clicar(futuro.querySelector("button")!);
    expect(futuro.querySelector("input, select, textarea")).toBeNull();
    expect(futuro.querySelector("code")?.textContent).toBe("efeito_do_futuro");
  });
});

describe("revisão adversa", () => {
  it("item_desconhecido: Remover tira o efeito de versão futura (senão a exportação fica presa)", async () => {
    desmontarTudo();
    await st().fechar({ descartar: true });
    await abrirCom(ler<ProjetoEdicao>("projeto-desconhecido.json"));
    tela = await montar(<Inspetor />);
    await esperar(() => selecionar("clipe", "c1"));
    const futuro = tela.raiz.querySelector<HTMLElement>('[data-efeito="efeito_do_futuro"]')!;
    const n = passos();
    await clicar(futuro.querySelector<HTMLButtonElement>('[aria-label="Remover"]')!);
    const ids = () => (doc().principal[0] as unknown as { efeitos: { id: string }[] }).efeitos.map((e) => e.id);
    expect(ids()).toEqual(["e1", "e8"]);
    expect([passos(), ultimo()]).toEqual([n + 1, "Remover"]);
    st().desfazer();
    expect(ids()).toEqual(["e1", "e9", "e8"]);
  });

  it("Gravar: trocar de aba no meio solta o microfone e guarda o que foi gravado", async () => {
    const trilha = { stop: vi.fn() };
    const gravadores: { estado: string }[] = [];
    class Gravador {
      estado = "inactive";
      mimeType = "audio/webm";
      ondataavailable: ((e: { data: Blob }) => void) | null = null;
      onstop: (() => void) | null = null;
      constructor() {
        gravadores.push(this);
      }
      start() {
        this.estado = "recording";
      }
      stop() {
        if (this.estado === "inactive") return;
        this.estado = "inactive";
        this.ondataavailable?.({ data: new Blob(["abc"]) });
        this.onstop?.();
      }
    }
    vi.stubGlobal("MediaRecorder", Gravador);
    Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: { getUserMedia: vi.fn(async () => ({ getTracks: () => [trilha] })) } });
    const m: MidiaRef = { ...doc().midias[2]!, id: "m6", nome: "", sha256: "6".repeat(64) };
    cmd.edicaoGravacaoSalvar.mockResolvedValue(m);
    try {
      desmontarTudo();
      tela = await montar(<Inspetor />);
      await clicar(botao(tela.raiz, "Áudio"));
      await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="gravar"]')!);
      await esperar(() => new Promise((r) => setTimeout(r, 0)));
      expect(gravadores.map((g) => g.estado)).toEqual(["recording"]);
      // foco num clipe da Sequência: o Inspetor vai para a aba Clipe
      await esperar(() => selecionar("clipe", principal()[1].id));
      expect(useInspetorUi.getState().aba).toBe("clipe");
      await esperar(() => new Promise((r) => setTimeout(r, 10)));
      expect(gravadores.map((g) => g.estado)).toEqual(["inactive"]);
      expect(trilha.stop).toHaveBeenCalled();
      expect(cmd.edicaoGravacaoSalvar).toHaveBeenCalledTimes(1);
      expect(doc().audios[0]!.clipes.map((c) => c.midia)).toContain("m6");
    } finally {
      vi.unstubAllGlobals();
      delete (navigator as unknown as { mediaDevices?: unknown }).mediaDevices;
    }
  });

  it("Gravar e trocar de edição antes de parar: a narração vai para o projeto em que começou, não para o novo", async () => {
    const trilha = { stop: vi.fn() };
    class Gravador {
      mimeType = "audio/webm";
      ondataavailable: ((e: { data: Blob }) => void) | null = null;
      onstop: (() => void) | null = null;
      start() {}
      stop() {
        this.ondataavailable?.({ data: new Blob(["abc"]) });
        this.onstop?.();
      }
    }
    vi.stubGlobal("MediaRecorder", Gravador);
    Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: { getUserMedia: vi.fn(async () => ({ getTracks: () => [trilha] })) } });
    const a = doc();
    const b: ProjetoEdicao = { ...ler<ProjetoEdicao>("projeto-do-store.json"), id: "f".repeat(8) + "-0000-4000-8000-" + "0".repeat(12), titulo: "B" };
    const m: MidiaRef = { ...a.midias[2]!, id: "m8", nome: "", sha256: "8".repeat(64) };
    cmd.edicaoGravacaoSalvar.mockResolvedValue(m);
    const salvos: ProjetoEdicao[] = [];
    cmd.edicaoSalvar.mockImplementation(async (_ws: string, p: ProjetoEdicao) => (salvos.push(p), { updated_at: new Date().toISOString() }));
    cmd.edicaoAbrir.mockImplementation(async (_ws: string, id: string) => ({ projeto: id === a.id ? a : b, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" }));
    try {
      await clicar(botao(tela.raiz, "Áudio"));
      await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="gravar"]')!);
      await esperar(() => new Promise((r) => setTimeout(r, 0)));
      // a Central abre outra edição com a gravação em curso (no app a tela passa pelo projeto fechado e o Gravador sai;
      // aqui o React pode juntar as duas mudanças: então o perito para já no projeto novo)
      await esperar(() => st().abrir(WS, b.id));
      await esperar(() => new Promise((r) => setTimeout(r, 10)));
      expect(st().aberta?.projetoId).toBe(b.id);
      const parar = tela.raiz.querySelector<HTMLButtonElement>('[data-acao="parar"]');
      if (parar) await clicar(parar);
      await esperar(() => new Promise((r) => setTimeout(r, 10)));
      expect(cmd.edicaoGravacaoSalvar).toHaveBeenCalledTimes(1);
      expect(cmd.edicaoGravacaoSalvar.mock.calls[0]!.slice(0, 2)).toEqual([WS, a.id]);
      expect(doc().audios.flatMap((t) => t.clipes).some((c) => c.midia === "m8" || c.nome === m.nome && c.midia === m.id)).toBe(false);
      expect(doc().midias.some((x) => x.sha256 === m.sha256)).toBe(false);
      const deA = salvos.filter((p) => p.id === a.id).at(-1)!;
      expect(deA.midias.some((x) => x.sha256 === m.sha256)).toBe(true);
      const id = deA.midias.find((x) => x.sha256 === m.sha256)!.id;
      expect(deA.audios.flatMap((t) => t.clipes).some((c) => c.midia === id)).toBe(true);
    } finally {
      vi.unstubAllGlobals();
      delete (navigator as unknown as { mediaDevices?: unknown }).mediaDevices;
    }
  });

  it("Parar com o projeto sem conseguir gravar: a narração entra assim mesmo", async () => {
    const trilha = { stop: vi.fn() };
    class Gravador {
      mimeType = "audio/webm";
      ondataavailable: ((e: { data: Blob }) => void) | null = null;
      onstop: (() => void) | null = null;
      start() {}
      stop() {
        this.ondataavailable?.({ data: new Blob(["abc"]) });
        this.onstop?.();
      }
    }
    vi.stubGlobal("MediaRecorder", Gravador);
    Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: { getUserMedia: vi.fn(async () => ({ getTracks: () => [trilha] })) } });
    cmd.edicaoGravacaoSalvar.mockResolvedValue({ ...doc().midias[2]!, id: "m7", sha256: "7".repeat(64) });
    try {
      st().alterar("Nome", (d) => void (d.titulo = "pendente"));
      cmd.edicaoSalvar.mockRejectedValue({ kind: "filesystem", message: "filesystem error: ocupado" });
      await clicar(botao(tela.raiz, "Áudio"));
      await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="gravar"]')!);
      await esperar(() => new Promise((r) => setTimeout(r, 0)));
      await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="parar"]')!);
      await esperar(() => new Promise((r) => setTimeout(r, 10)));
      expect(cmd.edicaoGravacaoSalvar).toHaveBeenCalledTimes(1);
      expect(doc().audios[0]!.clipes.map((c) => c.midia)).toContain("m7");
    } finally {
      vi.unstubAllGlobals();
      delete (navigator as unknown as { mediaDevices?: unknown }).mediaDevices;
      cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
    }
  });

  it("campo de texto: gesto encerrado por fora (alça do palco sem tirar o foco) não perde o que se digita depois", async () => {
    await esperar(() => selecionar("clipe", principal()[1].id));
    const nome = entradaDe(tela.raiz, "Nome") as HTMLInputElement;
    await digitar(nome, "abc");
    st().iniciarGesto("Mover").concluir();
    await digitar(nome, "abcdef", { aos: 6 });
    expect(principal()[1].rotulo).toBe("abcdef");
    await sair(nome);
    expect(principal()[1].rotulo).toBe("abcdef");
  });

  it("tabela de pacotes não fica guardada depois que a do decodificador fica pronta", async () => {
    const S = "1".repeat(64);
    const tab = (origem: "pacotes" | "decodificador") => ({ tb: { num: 1, den: 15360 }, pts0: 0, passos: [512], passo_mediano: 512, chaves: [0], vfr: false, origem });
    function Sonda() {
      const t = useTabela(WS, S);
      return <span data-origem={t?.origem ?? ""} />;
    }
    const origem = (t: Tela) => t.raiz.querySelector("[data-origem]")!.getAttribute("data-origem");
    cmd.edicaoTabelaQuadros.mockResolvedValue(tab("pacotes"));
    useFundoStore.setState({ tarefas: [] });
    const t1 = await montar(<Sonda />);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(origem(t1)).toBe("pacotes");
    cmd.edicaoTabelaQuadros.mockResolvedValue(tab("decodificador"));
    await esperar(() => useFundoStore.getState().receber([{ id: "f1", tipo: "tabela", chave: `tabela:${S}`, feito: 1, estado: { tipo: "pronta" } }]));
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(origem(t1)).toBe("decodificador");
    useFundoStore.setState({ tarefas: [] });
  });

  it("efeito malformado de outro tipo no projeto não impede Remover", async () => {
    await esperar(() => selecionar("clipe", principal()[1].id));
    st().alterar("x", (d) => void (d.principal[2] as ClipeTrecho).efeitos.push({ tipo: "velocidade_exibida", id: "vx", ligado: true, nome: "", ativo: null } as unknown as EfTexto));
    const d = principal()[1].efeitos.find((e) => (e as EfDestaque).tipo === "destaque") as EfDestaque;
    expect(() => removerEfeito(d.id, "Remover")).not.toThrow();
    expect(principal()[1].efeitos.some((e) => (e as EfDestaque).id === d.id)).toBe(false);
  });

  it("Posição: o lugar padrão de Texto, Tempo e Imagem aparece marcado na grade", async () => {
    const id = principal()[1].id;
    await esperar(() => selecionar("clipe", id));
    for (const r of ["Texto", "Tempo"]) {
      await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="adicionar"]')!);
      await clicar([...document.querySelectorAll<HTMLElement>("button")].find((x) => x.textContent?.trim() === r)!);
      const aberto = tela.raiz.querySelector<HTMLElement>('[class*="efeitoAberto"]')!;
      expect(campo(aberto, "Posição").querySelectorAll('[aria-pressed="true"]').length, r).toBe(1);
    }
  });

  it("Cor: trocar o tom mantém a transparência que a cor já tinha", async () => {
    await esperar(() => selecionar("clipe", principal()[1].id));
    const d = () => principal()[1].efeitos.find((e) => (e as EfDestaque).tipo === "destaque") as EfDestaque;
    st().alterar("x", (x) => {
      ((x.principal[1] as ClipeTrecho).efeitos.find((e) => (e as EfDestaque).tipo === "destaque") as EfDestaque).traco.cor = "#ff000080";
    });
    const linha = tela.raiz.querySelector<HTMLElement>('[data-efeito="destaque"]')!;
    await clicar(linha.querySelector("button")!);
    await digitar(campo(tela.raiz.querySelector<HTMLElement>('[data-efeito="destaque"]')!, "Cor").querySelector("input")!, "#00ff00", { aos: 7 });
    expect(d().traco.cor).toBe("#00ff0080");
  });

  it("Imagem…: o mesmo arquivo de novo não duplica a mídia", async () => {
    const id = principal()[1].id;
    await esperar(() => selecionar("clipe", id));
    const img: MidiaRef = { ...doc().midias[2]!, id: "m4", tipo: "imagem", nome: "brasao.png", sha256: "4".repeat(64), duracao_s: null, tem_audio: false };
    cmd.edicaoMidiaImportar.mockResolvedValueOnce(img);
    await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="adicionar"]')!);
    await clicar([...document.querySelectorAll<HTMLElement>("button")].find((x) => x.textContent?.trim() === "Imagem")!);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    const n = doc().midias.length;
    cmd.edicaoMidiaImportar.mockResolvedValueOnce({ ...img, id: "m4b" });
    await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="trocar-imagem"]')!);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(doc().midias.length).toBe(n);
    expect(principal()[1].efeitos.at(-1)).toMatchObject({ tipo: "imagem", midia: "m4" });
  });

  it("quadro congelado mostra a ocultação da mídia que vale no instante dele", async () => {
    st().alterar("Congelar aqui", (d) =>
      void d.principal.push({ tipo: "congelar", id: "cg2", rotulo: "", desligado: false, transicao_entrada: null, efeitos: [], midia: "m1", instante_s: 13, instante_pts: null, duracao_s: 2 }),
    );
    await esperar(() => selecionar("clipe", "cg2"));
    const linha = tela.raiz.querySelector<HTMLElement>('[data-efeito="ocultar"]');
    expect(linha?.querySelector("[data-da-midia]")).toBeTruthy();
  });

  it("projeto mínimo (sem clipe, sem trilha, campos com padrão do Rust ausentes): as quatro abas abrem sem quebrar", async () => {
    desmontarTudo();
    await st().fechar({ descartar: true });
    await abrirCom(ler<ProjetoEdicao>("projeto-minimo.json"));
    tela = await montar(<Inspetor />);
    await esperar(() => selecionar("clipe", "nao-existe"));
    for (const aba of ["Clipe", "Áudio", "Legendas", "Saída"]) {
      await clicar(botao(tela.raiz, aba));
      expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
    }
    expect(entradaDe(tela.raiz, "Largura")).toBeTruthy();
  });

  it("Início, Fim e Posição seguem só MidiaInfo.marcavel (h 17): falso desliga; verdadeiro aceita #n com o pts da tabela", async () => {
    const info = (marcavel: boolean) => ({
      id: "vm1", sha256: "1".repeat(64), nome: "cam1.h264", exibido: { w: 1920, h: 1080 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 },
      de_s: 0, ate_s: 60, n: 1800, tem_audio: false, previa: { relativo: "", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 60, largura: 1920, altura: 1080, pronta: true },
      tabela: marcavel ? "decodificador" : "pacotes", erros: 0, marcavel,
    });
    const tab = (origem: "pacotes" | "decodificador") => ({ tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(1799).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem });
    const espera = () => esperar(() => new Promise((r) => setTimeout(r, 0)));
    cmd.edicaoMidiaInfo.mockResolvedValue(info(false));
    cmd.edicaoTabelaQuadros.mockResolvedValue(tab("pacotes"));
    esquecerLeituras();
    tela.desmontar();
    tela = await montar(<Inspetor />);
    await esperar(() => selecionar("clipe", principal()[1].id));
    await espera();
    await espera();
    expect((entradaDe(tela.raiz, "Início") as HTMLInputElement).disabled).toBe(true);
    expect((entradaDe(tela.raiz, "Fim") as HTMLInputElement).disabled).toBe(true);
    // marcável (tabela do decodificador): #3 vira o pts do quadro 3
    cmd.edicaoMidiaInfo.mockResolvedValue(info(true));
    cmd.edicaoTabelaQuadros.mockResolvedValue(tab("decodificador"));
    esquecerLeituras();
    tela.desmontar();
    tela = await montar(<Inspetor />);
    await espera();
    await espera();
    const ini = entradaDe(tela.raiz, "Início") as HTMLInputElement;
    expect(ini.disabled).toBe(false);
    await digitar(ini, "#3", { aos: 2 });
    await tecla(ini, "Enter");
    expect(principal()[1]).toMatchObject({ inicio_s: 0.1, inicio_pts: 1536 });
  });

  it("Cor da mídia sem escolha mostra a da fonte (MidiaInfo.cor), sem gravar nada (O4/F5)", async () => {
    const info = {
      id: "vm1", sha256: "1".repeat(64), nome: "cam1.mp4", exibido: { w: 720, h: 480 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 },
      de_s: 0, ate_s: 60, n: 1800, tem_audio: false, previa: { relativo: "", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 60, largura: 720, altura: 480, pronta: true },
      tabela: "decodificador", erros: 0, marcavel: true, cor: { matriz: "bt601", faixa: "tv" },
    };
    cmd.edicaoMidiaInfo.mockResolvedValue(info);
    esquecerLeituras();
    tela.desmontar();
    tela = await montar(<Inspetor />);
    await esperar(() => selecionar("clipe", principal()[1].id));
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    const c = principal()[1] as ClipeTrecho;
    const n = passos();
    expect((entradaDe(tela.raiz, "Cor") as HTMLSelectElement).value).toBe("bt601:tv");
    expect(tela.raiz.querySelector("[data-cor-midia]")?.getAttribute("data-cor-midia")).toBe("fonte");
    expect(() => botao(tela.raiz, "Automática")).toThrow();
    expect([doc().midias.find((m) => m.id === c.midia)!.cor, passos()]).toEqual([null, n]);
  });

  it("Fim no fim da mídia: ↑ não tira o último quadro; ↓ tira um (não dois)", async () => {
    const info = {
      id: "vm1", sha256: "1".repeat(64), nome: "cam1.mp4", exibido: { w: 1920, h: 1080 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 },
      de_s: 0, ate_s: 60, n: 1800, tem_audio: false, previa: { relativo: "", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 60, largura: 1920, altura: 1080, pronta: true },
      tabela: "decodificador", erros: 0, marcavel: true,
    };
    cmd.edicaoMidiaInfo.mockResolvedValue(info);
    cmd.edicaoTabelaQuadros.mockResolvedValue({ tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(1799).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem: "decodificador" });
    esquecerLeituras();
    tela.desmontar();
    tela = await montar(<Inspetor />);
    const id = principal()[1].id;
    // trecho até o fim da mídia (como nasce da mídia inteira)
    st().alterar("x", (d) => {
      const c = d.principal[1] as ClipeTrecho;
      c.fim_s = 60;
      c.fim_pts = null;
    });
    await esperar(() => selecionar("clipe", id));
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    const fim = () => entradaDe(tela.raiz, "Fim") as HTMLInputElement;
    await tecla(fim(), "ArrowUp");
    expect(principal()[1]).toMatchObject({ fim_s: 60, fim_pts: null });
    await tecla(fim(), "ArrowDown");
    expect(principal()[1]).toMatchObject({ fim_s: (1799 * 512) / 15360, fim_pts: 1799 * 512 });
  });

  it("De e Até das marcações: #n e setas só com a mídia marcável (h 17)", async () => {
    const info = (marcavel: boolean) => ({
      id: "vm1", sha256: "1".repeat(64), nome: "cam1.h264", exibido: { w: 1920, h: 1080 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 },
      de_s: 0, ate_s: 60, n: 1800, tem_audio: false, previa: { relativo: "", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 60, largura: 1920, altura: 1080, pronta: true },
      tabela: marcavel ? "decodificador" : "pacotes", erros: 0, marcavel,
    });
    const tab = (origem: "pacotes" | "decodificador") => ({ tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(1799).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem });
    const espera = () => esperar(() => new Promise((r) => setTimeout(r, 0)));
    const destaque = () => principal()[1].efeitos.find((e) => (e as { tipo: string }).tipo === "destaque") as EfDestaque;
    const de = () => entradaDe(tela.raiz.querySelector<HTMLElement>('[data-efeito="destaque"]')!, "De") as HTMLInputElement;
    for (const marcavel of [false, true]) {
      cmd.edicaoMidiaInfo.mockResolvedValue(info(marcavel));
      cmd.edicaoTabelaQuadros.mockResolvedValue(tab(marcavel ? "decodificador" : "pacotes"));
      esquecerLeituras();
      tela.desmontar();
      tela = await montar(<Inspetor />);
      await esperar(() => selecionar("clipe", principal()[1].id));
      await esperar(() => selecionarEfeitoAberto(destaque().id));
      await espera();
      await espera();
      const antes = destaque().ativo;
      await digitar(de(), "#3", { aos: 2 });
      await tecla(de(), "Enter");
      if (marcavel) expect(destaque().ativo?.de).toBe(0.1);
      else expect(destaque().ativo).toEqual(antes);
    }
  });

  it("estabilizar e lente: ligar, desligar e Remover pedem confirmação com a contagem de marcações (h 27)", async () => {
    const id = principal()[1].id;
    st().alterar("x", (d) =>
      void (d.principal[1] as ClipeTrecho).efeitos.push({ tipo: "estabilizar", id: "est", ligado: true, nome: "", ativo: null, suavizar: 15, borda: "preta", zoom_pct: 0, trf_sha256: null, analise: { de_s: 12, ate_s: 16, de_pts: 184320, ate_pts: 245760 } } as unknown as EfTexto),
    );
    await esperar(() => selecionar("clipe", id));
    const linha = () => tela.raiz.querySelector<HTMLElement>('[data-efeito="estabilizar"]')!;
    const marcas = marcacoesNoEspaco(doc(), principal()[1]);
    expect(marcas).toBeGreaterThan(0);
    ask.askConfirm.mockClear();
    ask.askConfirm.mockResolvedValueOnce(false);
    await clicar(botao(linha(), "Remover"));
    await esperar();
    expect(ask.askConfirm).toHaveBeenCalledWith({ title: "Estabilizar", message: `${marcas} marcações`, confirmLabel: "Remover", danger: true });
    expect(principal()[1].efeitos.some((e) => (e as { id: string }).id === "est")).toBe(true);
    ask.askConfirm.mockResolvedValueOnce(false);
    await clicar(linha().querySelector<HTMLButtonElement>('[aria-label="Estabilizar"][aria-pressed]')!);
    await esperar();
    expect(ask.askConfirm).toHaveBeenLastCalledWith({ title: "Estabilizar", message: `${marcas} marcações`, confirmLabel: "Estabilizar", danger: true });
    expect((principal()[1].efeitos.find((e) => (e as { id: string }).id === "est") as { ligado: boolean }).ligado).toBe(true);
    await clicar(linha().querySelector<HTMLButtonElement>('[aria-label="Estabilizar"][aria-pressed]')!);
    await esperar();
    expect((principal()[1].efeitos.find((e) => (e as { id: string }).id === "est") as { ligado: boolean }).ligado).toBe(false);
    await clicar(botao(linha(), "Remover"));
    await esperar();
    expect(principal()[1].efeitos.some((e) => (e as { id: string }).id === "est")).toBe(false);
    // efeito que não muda o espaço não pergunta
    ask.askConfirm.mockClear();
    const d = principal()[1].efeitos.find((e) => (e as { tipo: string }).tipo === "destaque") as EfDestaque;
    await esperar(() => selecionarEfeitoAberto(d.id));
    await clicar(botao(tela.raiz.querySelector<HTMLElement>('[data-efeito="destaque"]')!, "Remover"));
    await esperar();
    expect(ask.askConfirm).not.toHaveBeenCalled();
  });

  it("só leitura: Preso a não muda", async () => {
    desmontarTudo();
    await st().fechar({ descartar: true });
    const r: ProjetoAberto = { projeto: ler<ProjetoEdicao>("projeto-do-store.json"), importado_de: null, so_leitura: true, avisos: [], raiz_cache: "/cache/c1" };
    cmd.edicaoAbrir.mockResolvedValueOnce(r);
    await st().abrir(WS, r.projeto.id);
    tela = await montar(<Inspetor />);
    const t = doc().audios[0]!;
    await esperar(() => selecionar("audio", t.clipes[0]!.id, t.id));
    expect((entradaDe(tela.raiz, "Preso a") as HTMLSelectElement).disabled).toBe(true);
  });
});

describe("rótulos", () => {
  it("toda seção, aberta uma a uma: só rótulos do §8.6, nenhum texto de exemplo", async () => {
    const vistos = new Set<string>();
    const fora = new Set<string>();
    const conferir = () => {
      for (const t of textosVisiveis(tela.raiz)) {
        vistos.add(t);
        if (!textoPermitido(t)) fora.add(t);
      }
      expect(tela.raiz.querySelectorAll("[placeholder]").length).toBe(0);
    };
    const [t, c1, c2] = principal();
    for (const id of [t.id, c1.id, c2.id]) {
      await esperar(() => selecionar("clipe", id));
      conferir();
      for (const l of [...tela.raiz.querySelectorAll<HTMLElement>("[data-efeito]")]) {
        await clicar(l.querySelector("button")!);
        conferir();
      }
    }
    for (const aba of ["Áudio", "Legendas", "Saída"]) {
      await clicar(botao(tela.raiz, aba));
      conferir();
      const narracao = tela.raiz.querySelector<HTMLButtonElement>('[data-acao="abrir-narracao"]');
      if (narracao) {
        await clicar(narracao);
        conferir();
      }
    }
    expect([...fora]).toEqual([]);
    // cobertura: os rótulos centrais apareceram
    for (const r of ["Nome", "Vídeo", "Início", "Fim", "Velocidade", "Manter o tom", "Transição", "Girar", "Espelhar", "Marcar início", "Marcar fim", "Ajuste", "Interpolação", "Adicionar", "De", "Até", "Marcar", "Forma", "Cor", "Espessura", "Contorno", "Rótulo", "Conteúdo", "Modo", "Intensidade", "Borda suave", "Margem", "Inverter", "Motivo", "Só neste clipe", "Texto", "Fonte", "Tamanho", "Posição", "Modelo", "Título", "Subtítulo", "Rodapé", "Fundo", "Volume do original", "Narração", "Preso a", "Entrada (s)", "Saída (s)", "Importar…", "Gravar", "Automática", "Largura", "Altura", "Quadros/s", "Áudio"])
      expect(vistos.has(r), r).toBe(true);
    // Fase 1: cada casca no lugar dela (§8.6), com os rótulos dela na tela
    const FASE_1: Record<string, string[]> = {
      "Clipe · Som do trecho (P1.5)": ["Som", "Silenciar trecho", "Da degravação…"],
      "Clipe · Realce (P1.4)": ["Realce", "Estabilizar", "Lente", "Desentrelaçar", "Brilho", "Nitidez", "Média de quadros"],
      "Áudio (P1.5)": ["Abaixar sob a narração", "Normalizar", "Limitador", "Música", "Efeitos"],
      "Legendas (P1.6)": ["Da degravação…", "Nova legenda"],
      "Saída · selos (P1.R)": ["Selo de velocidade", "Selo de ampliação", "Selo de realce"],
      "Saída · Fecho (P1.1)": ["Fecho", "Duração (s)"],
    };
    for (const [onde, rotulos] of Object.entries(FASE_1)) for (const r of rotulos) expect(vistos.has(r), `${onde}: ${r}`).toBe(true);
    expect(ROTULOS.size).toBeGreaterThan(50);
  });

  it("cascas da Fase 1: as onze bandeiras ligadas e cada casca no registro, no lugar do §8.6", () => {
    expect({ FECHO_PRONTO, LUPA_PRONTA, REGUA_PRONTA, VELOCIDADE_PRONTA, REALCE_PRONTO, AUDIO_CLIPE_PRONTO, PAINEL_AUDIO_PRONTO, LEGENDAS_PRONTAS, EDITOR_TITULO_PRONTO, PRESOS_PRONTOS, SELOS_PRONTOS }).toEqual({
      FECHO_PRONTO: true, LUPA_PRONTA: true, REGUA_PRONTA: true, VELOCIDADE_PRONTA: true, REALCE_PRONTO: true, AUDIO_CLIPE_PRONTO: true, PAINEL_AUDIO_PRONTO: true, LEGENDAS_PRONTAS: true, EDITOR_TITULO_PRONTO: true, PRESOS_PRONTOS: true, SELOS_PRONTOS: true,
    });
    // "Adicionar" na ordem do §8.6 (Holofote é de fase seguinte), com o editor do pacote
    expect(ADICIONAR.map((t) => REGISTRO[t].rotulo)).toEqual(["Destaque", "Ocultar", "Lupa", "Régua", "Velocidade", "Texto", "Tempo", "Imagem"]);
    expect([REGISTRO.lupa.Editor, REGISTRO.regua.Editor, REGISTRO.velocidade_exibida.Editor]).toEqual([LupaEditor, ReguaEditor, VelocidadeEditor]);
    for (const t of ["lupa", "regua", "velocidade_exibida"] as const) expect(EDITAVEIS.has(t), t).toBe(true);
    // seções por tipo de item: som do trecho depois do Trecho; Realce (P1.4) antes das Marcações
    expect(secoesDe("principal", "trecho")).toEqual([TrechoSecao, AudioClipeSecao, EntradaSecao, EnquadramentoSecao, RealceSecao, MarcacoesSecao]);
    expect(secoesDe("principal", "congelar")?.slice(-2)).toEqual([RealceSecao, MarcacoesSecao]);
    expect(secoesDe("principal", "titulo")).toEqual([TituloSecao, EntradaSecao, MarcacoesSecao]);
    expect(secoesDe("livre", "titulo")).toEqual([TituloSecao]);
  });
});
