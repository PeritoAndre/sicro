// @vitest-environment happy-dom
/**
 * Aba Áudio e som do clipe (P1.5), montados no Inspetor de verdade: mixagem (um passo por controle), trilhas por
 * papel, narração gravada por UM comando com os bytes crus, silêncios a partir da degravação (comando falso),
 * chaves do volume, e a narração presa ao clipe depois de reordenar a principal.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeTitulo, ClipeTrecho, Degravacao, MidiaRef, ProjetoAberto, ProjetoEdicao } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoMidiaImportar: vi.fn(),
  edicaoGravacaoSalvar: vi.fn(),
  edicaoDegravacao: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => "/fora/arquivo") }));
const aviso = vi.hoisted(() => ({ pushToast: vi.fn() }));
vi.mock("@components/toast/toastStore", () => aviso);

import { useEdicaoStore } from "../store/edicaoStore";
import { Inspetor } from "../inspetor/Inspetor";
import { esquecerLeituras } from "../inspetor/contexto";
import { useInspetorUi } from "../inspetor/ui";
import { arrastar, botao, clicar, desmontarTudo, digitar, entradaDe, esperar, lerFixture as ler, montar, tecla, textosVisiveis, type Tela } from "../inspetor/testeTela";
import { textoPermitido } from "../inspetor/rotulos.teste";
import { resolver } from "../modelo/tempo";
import { moverPara } from "../sequencia/acoes";
import { falasDoTrecho, silencioDaFala, AUDIO_CLIPE_PRONTO } from "../inspetor/secoes/AudioClipeSecao";
import { NORMALIZAR_PADRAO, PAINEL_AUDIO_PRONTO } from "./PainelAudio";

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
const doc = () => st().doc!;
const passos = () => st().passado.length;
const ultimo = () => st().passado.at(-1)?.rotulo;
const principal = () => doc().principal as [ClipeTitulo, ClipeTrecho, ClipeTrecho];
const trecho = () => principal()[1];
const q = <T extends HTMLElement>(s: string) => tela.raiz.querySelector<T>(s)!;
const marca = (rotulo: string) => [...tela.raiz.querySelectorAll("label")].find((l) => l.textContent?.trim() === rotulo)!.querySelector<HTMLInputElement>("input")!;
const tique = () => esperar(() => new Promise((r) => setTimeout(r, 0)));

let tela: Tela;

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-14T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  aviso.pushToast.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoTabelaQuadros.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  esquecerLeituras();
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });
  const projeto = ler<ProjetoEdicao>("projeto-do-store.json");
  const r: ProjetoAberto = { projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" };
  cmd.edicaoAbrir.mockResolvedValueOnce(r);
  await st().abrir(WS, projeto.id);
  tela = await montar(<Inspetor />);
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
  vi.unstubAllGlobals();
  delete (navigator as unknown as { mediaDevices?: unknown }).mediaDevices;
});

const focarTrecho = () => esperar(() => st().selecionar({ itens: [{ k: "clipe", id: trecho().id }], foco: { k: "clipe", id: trecho().id }, efeito: null }));

describe("aba Áudio", () => {
  it("as bandeiras do pacote estão ligadas", () => {
    expect([PAINEL_AUDIO_PRONTO, AUDIO_CLIPE_PRONTO]).toEqual([true, true]);
  });

  it("Abaixar sob a narração, Normalizar e Limitador: um passo cada; só rótulos do §8.6", async () => {
    await clicar(botao(tela.raiz, "Áudio"));
    const n = passos();
    await clicar(marca("Abaixar sob a narração"));
    expect(doc().audios[0]!.abaixa_original).toBe(true);
    expect([passos(), ultimo()]).toEqual([n + 1, "Abaixar sob a narração"]);
    const soNoRender = () => [...tela.raiz.querySelectorAll<HTMLElement>('[data-estado="so-no-render"]')].map((e) => [e.dataset.de, e.textContent, e.querySelector("svg") !== null]);
    // ligado: ícone de estado (a prévia viva não aplica), sem texto
    expect(soNoRender()).toEqual([["abaixar", "", true]]);
    await clicar(marca("Normalizar"));
    expect(doc().mixagem.normalizar).toEqual(NORMALIZAR_PADRAO);
    expect(doc().mixagem.normalizar).toEqual({ lufs: -16, pico_db: -1.5 });
    expect(soNoRender()).toEqual([["abaixar", "", true], ["normalizar", "", true]]);
    await clicar(marca("Limitador"));
    expect(doc().mixagem.limitador).toBe(true);
    expect(soNoRender().map((x) => x[0])).toEqual(["abaixar", "normalizar", "limitador"]);
    expect(st().passado.slice(-2).map((p) => p.rotulo)).toEqual(["Normalizar", "Limitador"]);
    await clicar(marca("Normalizar"));
    expect(doc().mixagem.normalizar).toBeNull();
    await clicar(marca("Abaixar sob a narração"));
    expect(doc().audios[0]!.abaixa_original).toBe(false);
    expect(soNoRender().map((x) => x[0])).toEqual(["limitador"]);
    expect(passos()).toBe(n + 5);
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
    expect(tela.raiz.querySelectorAll("[placeholder]").length).toBe(0);
  });

  it("sem trilha de narração, Abaixar sob a narração fica desligado", async () => {
    await esperar(() => st().alterar("Remover", (d) => void (d.audios = [])));
    await clicar(botao(tela.raiz, "Áudio"));
    expect(marca("Abaixar sob a narração").disabled).toBe(true);
    expect(textosVisiveis(tela.raiz).filter((t) => ["Narração", "Música", "Efeitos"].includes(t))).toEqual(["Narração", "Música", "Efeitos"]);
  });

  it("Importar… em Música e em Efeitos cria a trilha do papel, num passo, presa ao clipe em foco", async () => {
    await focarTrecho();
    await clicar(botao(tela.raiz, "Áudio"));
    const n = passos();
    // gravação só na narração
    expect([...tela.raiz.querySelectorAll('[data-acao="gravar"]')].map((b) => b.closest("[data-papel]")?.getAttribute("data-papel"))).toEqual(["narracao"]);
    const musica: MidiaRef = { ...doc().midias[2]!, id: "m5", nome: "fundo.mp3", sha256: "5".repeat(64) };
    cmd.edicaoMidiaImportar.mockResolvedValueOnce(musica);
    await clicar(q<HTMLButtonElement>('[data-papel="musica"] [data-acao="importar"]'));
    await tique();
    expect(doc().audios.map((t) => [t.papel, t.clipes.map((c) => c.midia)])).toEqual([
      ["narracao", ["m3"]],
      ["musica", ["m5"]],
    ]);
    expect([passos(), ultimo()]).toEqual([n + 1, "Importar…"]);
    expect(doc().audios[1]).toMatchObject({ volume: 1, mudo: false, abaixa_original: false });
    expect(doc().audios[1]!.clipes[0]!.pos).toEqual({ ref: "clipe", clipe: trecho().id, desloc: 0 });
    // sem clipe em foco, no começo do programa
    await esperar(() => st().selecionar({ itens: [], foco: null, efeito: null }));
    await clicar(botao(tela.raiz, "Áudio"));
    const efeito: MidiaRef = { ...doc().midias[2]!, id: "m6", nome: "sirene.wav", sha256: "6".repeat(64) };
    cmd.edicaoMidiaImportar.mockResolvedValueOnce(efeito);
    await clicar(q<HTMLButtonElement>('[data-papel="efeitos"] [data-acao="importar"]'));
    await tique();
    expect(doc().audios.map((t) => t.papel)).toEqual(["narracao", "musica", "efeitos"]);
    expect(doc().audios[2]!.clipes[0]!.pos).toEqual({ ref: "programa", t: 0 });
    // a segunda música vai para a mesma trilha
    cmd.edicaoMidiaImportar.mockResolvedValueOnce({ ...musica, id: "m7", sha256: "7".repeat(64) });
    await clicar(q<HTMLButtonElement>('[data-papel="musica"] [data-acao="importar"]'));
    await tique();
    expect(doc().audios[1]!.clipes.map((c) => c.midia)).toEqual(["m5", "m7"]);
    expect(passos()).toBe(n + 3);
    // "Abaixar sob a narração" só mexe nas trilhas de narração
    await clicar(marca("Abaixar sob a narração"));
    expect(doc().audios.map((t) => t.abaixa_original)).toEqual([true, false, false]);
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
  });

  it("narração gravada: UM comando, com os bytes crus da gravação inteira", async () => {
    const trilha = { stop: vi.fn() };
    class Gravador {
      mimeType = "audio/webm;codecs=opus";
      ondataavailable: ((e: { data: Blob }) => void) | null = null;
      onstop: (() => void) | null = null;
      start() {}
      stop() {
        // o navegador entrega em pedaços
        this.ondataavailable?.({ data: new Blob([new Uint8Array([26, 69, 223, 163, 0])]) });
        this.ondataavailable?.({ data: new Blob([new Uint8Array([255, 1, 2])]) });
        this.onstop?.();
      }
    }
    vi.stubGlobal("MediaRecorder", Gravador);
    Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: { getUserMedia: vi.fn(async () => ({ getTracks: () => [trilha] })) } });
    const m: MidiaRef = { ...doc().midias[2]!, id: "m9", nome: "", sha256: "9".repeat(64) };
    cmd.edicaoGravacaoSalvar.mockResolvedValue(m);
    desmontarTudo();
    tela = await montar(<Inspetor />);
    await clicar(botao(tela.raiz, "Áudio"));
    const n = passos();
    await clicar(q<HTMLButtonElement>('[data-acao="gravar"]'));
    await tique();
    expect(cmd.edicaoGravacaoSalvar).not.toHaveBeenCalled();
    await clicar(q<HTMLButtonElement>('[data-acao="parar"]'));
    await esperar(() => new Promise((r) => setTimeout(r, 10)));
    expect(cmd.edicaoGravacaoSalvar).toHaveBeenCalledTimes(1);
    const [ws, projeto, mime, bytes] = cmd.edicaoGravacaoSalvar.mock.calls[0]!;
    expect([ws, projeto, mime]).toEqual([WS, doc().id, "audio/webm;codecs=opus"]);
    expect(bytes).toBeInstanceOf(Uint8Array);
    expect([...(bytes as Uint8Array)]).toEqual([26, 69, 223, 163, 0, 255, 1, 2]);
    expect(trilha.stop).toHaveBeenCalled();
    // entra na trilha de narração, num passo
    expect(doc().audios[0]!.clipes.map((c) => c.midia)).toEqual(["m3", "m9"]);
    expect([passos(), ultimo()]).toEqual([n + 1, "Gravar"]);
  });

  it("reordenar a principal mantém a narração presa ao clipe", async () => {
    const ancora = trecho().id;
    await esperar(() => st().alterar("Posição", (d) => void (d.audios[0]!.clipes[0]!.pos = { ref: "clipe", clipe: ancora, desloc: 0.5 })));
    const narracao = doc().audios[0]!.clipes[0]!.id;
    const lugar = () => {
      const l = resolver(doc(), { tabela: () => null });
      return { clipe: l.clipes.find((c) => c.id === ancora)!.inicio_q, narracao: l.livres.find((x) => x.id === narracao)!.inicio_q, ordem: l.clipes.map((c) => c.id) };
    };
    const antes = lugar();
    expect(antes.narracao).toBe(antes.clipe + 15);
    // o trecho âncora (2º da lista) vai para o fim
    await esperar(() => moverPara(1, 2));
    const depois = lugar();
    expect(depois.ordem).not.toEqual(antes.ordem);
    expect(depois.clipe).toBeGreaterThan(antes.clipe);
    expect(doc().audios[0]!.clipes[0]!.pos).toEqual({ ref: "clipe", clipe: ancora, desloc: 0.5 });
    expect(depois.narracao).toBe(depois.clipe + 15);
    // a aba mostra o mesmo clipe em "Preso a", agora na posição nova
    await clicar(botao(tela.raiz, "Áudio"));
    await clicar(q<HTMLButtonElement>('[data-acao="abrir-narracao"]'));
    const preso = entradaDe(tela.raiz, "Preso a") as HTMLSelectElement;
    expect(preso.value).toBe(ancora);
    expect(preso.selectedOptions[0]!.textContent!.startsWith("3 ")).toBe(true);
  });
});

describe("som do clipe", () => {
  const falas: Degravacao = {
    origem: { audio_sha256: "a".repeat(64), importado_em: "2026-11-10T12:00:00Z", motor: "whisper", modelo_sha256: null },
    trechos: [
      { de: 11.0, ate: 12.4, texto: "para o carro", locutor: "A", ia: true },
      { de: 13.0, ate: 13.8, texto: "meu nome é Fulano", locutor: "B", ia: true },
      { de: 20.0, ate: 21.0, texto: "fora do trecho", locutor: null, ia: false },
    ],
  };

  it("falas da degravação: só as que tocam no trecho, cortadas nele", () => {
    expect(falasDoTrecho(falas.trechos, 12, 15.5).map((f) => [f.de, f.ate])).toEqual([
      [12, 12.4],
      [13, 13.8],
    ]);
    expect(falasDoTrecho(falas.trechos, 12.4, 13)).toEqual([]);
    const s = [{ id: "s", de: 13, ate: 13.8, modo: "tom" as const, motivo: "" }];
    expect([silencioDaFala(s, falas.trechos[1]!)?.id, silencioDaFala(s, falas.trechos[0]!)]).toEqual(["s", null]);
  });

  it("Da degravação…: a fala escolhida vira silêncio em tempo de fonte, num passo; escolhida de novo, sai", async () => {
    await focarTrecho();
    expect([trecho().midia, trecho().inicio_s, trecho().fim_s]).toEqual(["m1", 12, 15.5]);
    cmd.edicaoDegravacao.mockResolvedValue(falas);
    const n = passos();
    await clicar(q<HTMLButtonElement>('[data-acao="da-degravacao"]'));
    await tique();
    expect(cmd.edicaoDegravacao).toHaveBeenCalledTimes(1);
    expect(cmd.edicaoDegravacao).toHaveBeenCalledWith(WS, doc().midias[0]!.sha256);
    const linhas = () => [...tela.raiz.querySelectorAll<HTMLButtonElement>("[data-fala]")];
    expect(linhas().map((b) => b.dataset.fala)).toEqual(["12", "13"]);
    expect(passos()).toBe(n);
    await clicar(linhas()[1]!);
    expect(trecho().audio.silencios.map(({ de, ate, modo, motivo }) => ({ de, ate, modo, motivo }))).toEqual([{ de: 13, ate: 13.8, modo: "tom", motivo: "" }]);
    expect([passos(), ultimo()]).toEqual([n + 1, "Da degravação…"]);
    expect(linhas()[1]!.getAttribute("aria-pressed")).toBe("true");
    // a que começa antes do trecho entra cortada no começo dele
    await clicar(linhas()[0]!);
    expect(trecho().audio.silencios.map((s) => [s.de, s.ate])).toEqual([
      [13, 13.8],
      [12, 12.4],
    ]);
    await clicar(linhas()[1]!);
    expect(trecho().audio.silencios.map((s) => [s.de, s.ate])).toEqual([[12, 12.4]]);
    expect(passos()).toBe(n + 3);
    st().desfazer();
    expect(trecho().audio.silencios.length).toBe(2);
    // o texto da fala é dado: fora dele, só rótulos do §8.6
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
    expect(tela.raiz.querySelectorAll("[placeholder]").length).toBe(0);
    // fechar a lista não muda o documento
    await clicar(q<HTMLButtonElement>('[data-acao="da-degravacao"]'));
    expect(linhas()).toEqual([]);
    expect(trecho().audio.silencios.length).toBe(2);
  });

  it("degravação que não existe: o aviso leva a frase do catálogo e nada muda", async () => {
    await focarTrecho();
    cmd.edicaoDegravacao.mockRejectedValue({ kind: "validation", message: "validation error: degravacao_ausente: Degravação não encontrada" });
    const n = passos();
    await clicar(q<HTMLButtonElement>('[data-acao="da-degravacao"]'));
    await tique();
    expect(aviso.pushToast).toHaveBeenCalledWith("error", "Degravação não encontrada");
    expect(tela.raiz.querySelector("[data-fala]")).toBeNull();
    expect([passos(), trecho().audio.silencios]).toEqual([n, []]);
  });

  it("Silenciar trecho no cursor: Tom por padrão; De, Até, Mudo e Motivo, um passo cada", async () => {
    await focarTrecho();
    await esperar(() => st().posicionar({ modo: "clipe", clipe: trecho().id, t: 13 }));
    const n = passos();
    expect(tela.raiz.querySelector("[data-silencio]")).toBeNull();
    await clicar(q<HTMLButtonElement>('[data-acao="silenciar"]'));
    expect(trecho().audio.silencios.map(({ de, ate, modo, motivo }) => ({ de, ate, modo, motivo }))).toEqual([{ de: 13, ate: 14, modo: "tom", motivo: "" }]);
    expect([passos(), ultimo()]).toEqual([n + 1, "Silenciar trecho"]);
    // nasce aberto, com o ícone de estado (a prévia viva toca o som da fonte), sem texto
    const linha = q<HTMLElement>("[data-silencio]");
    expect([...linha.querySelectorAll<HTMLElement>('[data-estado="so-no-render"]')].map((e) => [e.dataset.de, e.textContent, e.querySelector("svg") !== null])).toEqual([["silencio", "", true]]);
    await digitar(entradaDe(linha, "Até") as HTMLInputElement, "00:00:14,500");
    await tecla(entradaDe(linha, "Até") as HTMLInputElement, "Enter");
    expect(trecho().audio.silencios[0]!.ate).toBe(14.5);
    await clicar(botao(linha, "Mudo"));
    expect(trecho().audio.silencios[0]!.modo).toBe("mudo");
    expect(ultimo()).toBe("Silenciar trecho");
    await digitar(entradaDe(linha, "Motivo") as HTMLInputElement, "nome de testemunha", { aos: 18 });
    expect(trecho().audio.silencios[0]!.motivo).toBe("nome de testemunha");
    // fim antes do começo não entra
    await digitar(entradaDe(linha, "Até") as HTMLInputElement, "00:00:12,500");
    await tecla(entradaDe(linha, "Até") as HTMLInputElement, "Enter");
    expect(trecho().audio.silencios[0]!.ate).toBe(14.5);
    // nem fim depois do fim do trecho (15,5 s), nem começo antes do começo dele (12 s)
    await digitar(entradaDe(linha, "Até") as HTMLInputElement, "00:00:16,000");
    await tecla(entradaDe(linha, "Até") as HTMLInputElement, "Enter");
    await digitar(entradaDe(linha, "De") as HTMLInputElement, "00:00:11,000");
    await tecla(entradaDe(linha, "De") as HTMLInputElement, "Enter");
    expect(trecho().audio.silencios[0]).toMatchObject({ de: 13, ate: 14.5 });
    // no fim do trecho o silêncio novo cabe até o fim; no último instante não nasce
    await esperar(() => st().posicionar({ modo: "clipe", clipe: trecho().id, t: 15.2 }));
    await clicar(q<HTMLButtonElement>('[data-acao="silenciar"]'));
    expect(trecho().audio.silencios[1]).toMatchObject({ de: 15.2, ate: 15.5 });
    await esperar(() => st().posicionar({ modo: "clipe", clipe: trecho().id, t: 15.5 }));
    await clicar(q<HTMLButtonElement>('[data-acao="silenciar"]'));
    expect(trecho().audio.silencios.length).toBe(2);
    await clicar(linha.querySelector<HTMLButtonElement>('[aria-label="Remover"]')!);
    expect(trecho().audio.silencios.map((s) => s.de)).toEqual([15.2]);
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
  });

  // P4.2 (D7): a raia "Volume" com "Chave no cursor" / "Tirar chave"; o "Som" do trecho grava a chave do cursor
  it("volume acima de 100 %: \"só no render\" no Som do trecho e na narração; até 100 %, nada", async () => {
    await focarTrecho();
    const icones = (sel: string) => tela.raiz.querySelectorAll(`${sel} [data-estado="so-no-render"][data-de="volume"]`).length;
    await esperar(() => st().alterar("Som", (d) => void ((d.principal[1] as ClipeTrecho).audio.volume = [{ t: 13, v: 0.8 }, { t: 14, v: 1 }])));
    expect(icones("")).toBe(0);
    await esperar(() => st().alterar("Som", (d) => void ((d.principal[1] as ClipeTrecho).audio.volume = [{ t: 13, v: 0.8 }, { t: 14, v: 1.5 }])));
    expect(icones("")).toBe(1);
    await esperar(() => st().alterar("Som", (d) => void ((d.principal[1] as ClipeTrecho).audio.volume = 1.2)));
    expect(icones("")).toBe(1);
    // narração
    await clicar(botao(tela.raiz, "Áudio"));
    await clicar(q<HTMLButtonElement>('[data-acao="abrir-narracao"]'));
    await esperar(() => st().alterar("Volume", (d) => void (d.audios[0]!.clipes[0]!.volume = [{ t: 0, v: 0.5 }, { t: 1, v: 1 }])));
    expect(icones("[data-audio]")).toBe(0);
    await esperar(() => st().alterar("Volume", (d) => void (d.audios[0]!.clipes[0]!.volume = [{ t: 0, v: 0.5 }, { t: 1, v: 2 }])));
    expect(icones("[data-audio]")).toBe(1);
  });

  it("Som: chave do volume no cursor (tempo de fonte); a última que sai deixa o valor fixo", async () => {
    await focarTrecho();
    await esperar(() => st().alterar("Som", (d) => void ((d.principal[1] as ClipeTrecho).audio.volume = 0.8)));
    await esperar(() => st().posicionar({ modo: "clipe", clipe: trecho().id, t: 13 }));
    const n = passos();
    const chave = () => q<HTMLButtonElement>('[data-raia="volume"] [data-acao="marcar-volume"]');
    expect(chave().getAttribute("aria-label")).toBe("Chave no cursor");
    await clicar(chave());
    // linear, sem o campo: o mesmo que a chave do som gravava antes da Fase 4
    expect(trecho().audio.volume).toEqual([{ t: 13, v: 0.8 }]);
    expect([passos(), ultimo()]).toEqual([n + 1, "Chave no cursor"]);
    expect(chave().getAttribute("aria-label")).toBe("Tirar chave");
    // até 100 % a prévia toca o volume (Programa e Clipe): nada de "só no render"
    expect(tela.raiz.querySelectorAll('[data-estado="so-no-render"][data-de="volume"]').length).toBe(0);
    await esperar(() => st().posicionar({ modo: "clipe", clipe: trecho().id, t: 14 }));
    await clicar(chave());
    expect(trecho().audio.volume).toEqual([
      { t: 13, v: 0.8 },
      { t: 14, v: 0.8 },
    ]);
    expect(tela.raiz.querySelectorAll('[data-curva="Volume"] [data-chave]').length).toBe(2);
    // o "Som" mostra o volume do cursor e o arrasto grava a chave dele: um passo
    const som = q<HTMLInputElement>('[data-campo="Som"] input[type="range"]');
    expect(som.disabled).toBe(false);
    const m = passos();
    await arrastar(som, [0.5, 0.2]);
    expect(trecho().audio.volume).toEqual([
      { t: 13, v: 0.8 },
      { t: 14, v: 0.2 },
    ]);
    expect(passos()).toBe(m + 1);
    // no meio: a chave nova leva o valor da curva
    await esperar(() => st().posicionar({ modo: "clipe", clipe: trecho().id, t: 13.5 }));
    expect(Number(som.value)).toBeCloseTo(0.5, 9);
    await clicar(chave());
    expect((trecho().audio.volume as { t: number; v: number }[]).map((k) => [k.t, Math.round(k.v * 100) / 100])).toEqual([
      [13, 0.8],
      [13.5, 0.5],
      [14, 0.2],
    ]);
    // na chave do cursor: Tirar chave
    await clicar(chave());
    await esperar(() => st().posicionar({ modo: "clipe", clipe: trecho().id, t: 13 }));
    await clicar(chave());
    expect(trecho().audio.volume).toEqual([{ t: 14, v: 0.2 }]);
    // cursor fora do trecho (aparando): nada de chave fora dele
    await esperar(() => st().posicionar({ modo: "clipe", clipe: trecho().id, t: 20 }));
    expect(chave().disabled).toBe(true);
    await esperar(() => st().posicionar({ modo: "clipe", clipe: trecho().id, t: 14 }));
    await clicar(chave());
    expect(trecho().audio.volume).toBe(0.2);
    expect(tela.raiz.querySelector('[data-curva="Volume"] [data-chave]')).toBeNull();
    expect(tela.raiz.querySelector('[data-estado="so-no-render"]')).toBeNull();
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
  });
});
