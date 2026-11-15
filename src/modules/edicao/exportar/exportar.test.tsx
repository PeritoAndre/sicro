// @vitest-environment happy-dom
/**
 * Diálogo Exportar, Fila e Saídas montados de verdade: o botão muda com os avisos, a ciência é obrigatória, erro
 * bloqueia e leva ao item; a fila mostra fase, progresso e as ações de cada estado; só rótulos do §8.6.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Analise, Problema, ProjetoAberto, ProjetoEdicao, SaidaRegistrada, Tarefa } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoExcluir: vi.fn(),
  edicaoCapacidades: vi.fn(),
  edicaoEnfileirar: vi.fn(),
  edicaoFila: vi.fn(),
  edicaoCancelar: vi.fn(),
  edicaoConferir: vi.fn(),
  edicaoSaidas: vi.fn(),
  edicaoSaidaExcluir: vi.fn(),
  edicaoRefazer: vi.fn(),
  revealPathInExplorer: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
const ask = vi.hoisted(() => ({ askConfirm: vi.fn(async (_o: { title: string; confirmLabel?: string }) => true) }));
vi.mock("@components/Dialog/ask", () => ask);

import { useEdicaoStore } from "../store/edicaoStore";
import { useRenderStore } from "../store/renderStore";
import { useInspetorUi } from "../inspetor/ui";
import { botao, clicar, desmontarTudo, digitar, escolher, esperar, lerFixture, montar, sair, textosVisiveis } from "../inspetor/testeTela";
import { textoPermitido } from "../inspetor/rotulos.teste";
import { abrirExportar, classificar, contarAvisos, faltaCiencia, podeExportar, rotuloExportar, useExportarUi } from "./avisos";
import { codificadorDe, DialogoExportar } from "./DialogoExportar";
import { Fila, rotuloEstado } from "./Fila";
import { conferenciaEmCurso, conferirSaida, estadoDaSaida, rotuloComparacao } from "./Saidas";
import { useToastStore } from "@components/toast/toastStore";

const toasts = () => useToastStore.getState().toasts;

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
const pr = (nivel: Problema["nivel"], codigo: string, x: Partial<Problema> = {}): Problema => ({ nivel, codigo, mensagem: `[${codigo}]`, clipe: null, efeito: null, trilha: null, midia: null, campo: null, n: null, ...x });

async function comAnalise(problemas: Problema[]) {
  const a: Analise = { problemas, linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null };
  await esperar(() => useEdicaoStore.setState({ analise: a, analiseRevisao: st().revisao }));
}
const corpo = () => document.body;
const exportarBtn = () => document.querySelector<HTMLButtonElement>('[data-acao="exportar"]')!;
const semFrase = () => {
  const fora = textosVisiveis(corpo()).filter((t) => !textoPermitido(t));
  expect(fora).toEqual([]);
  expect(corpo().querySelectorAll("[placeholder]").length).toBe(0);
};

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-14T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {})); // a análise vem do teste
  cmd.edicaoCapacidades.mockResolvedValue({ versao: "9.0.1", origem: "path", sha256: "", faltam: [], opcionais_ausentes: [], codificador: "h264_nvenc", gpu: { nome: "RTX", driver: "", codificador: "h264_nvenc", degrau: "", args: [], nvdec: false, scale_cuda: false }, reprovados: [], sondando: false });
  cmd.edicaoEnfileirar.mockResolvedValue("t-novo");
  cmd.edicaoFila.mockResolvedValue([]);
  cmd.edicaoSaidas.mockResolvedValue([]);
  cmd.revealPathInExplorer.mockResolvedValue(undefined);
  cmd.edicaoCancelar.mockResolvedValue(undefined);
  cmd.edicaoConferir.mockResolvedValue({});
  cmd.edicaoSaidaExcluir.mockResolvedValue(undefined);
  cmd.edicaoRefazer.mockResolvedValue("t-refazer");
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  const r: ProjetoAberto = { projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" };
  cmd.edicaoAbrir.mockResolvedValueOnce(r);
  await st().abrir(WS, projeto.id);
  useExportarUi.setState({ dialogo: false, fila: false });
  useRenderStore.setState({ tarefas: [], progresso: {} });
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

describe("regras", () => {
  it("contagem, rótulo do botão, ciência por código, erro bloqueia", () => {
    const c = classificar([pr("aviso", "fps_diferente"), pr("pericial", "taxa_variavel"), pr("pericial", "taxa_variavel"), pr("pericial", "ocultacao_reversivel")]);
    expect(contarAvisos(c)).toBe(4);
    expect(c.periciais.map((g) => [g.codigo, g.itens.length])).toEqual([
      ["taxa_variavel", 2],
      ["ocultacao_reversivel", 1],
    ]);
    expect([0, 1, 3].map(rotuloExportar)).toEqual(["Exportar", "Exportar com 1 aviso", "Exportar com 3 avisos"]);
    expect(faltaCiencia(c, new Set(["taxa_variavel"]))).toEqual(["ocultacao_reversivel"]);
    expect(podeExportar(c, new Set(["taxa_variavel", "ocultacao_reversivel"]))).toBe(true);
    expect(podeExportar(classificar([pr("erro", "intervalo")]), new Set())).toBe(false);
  });
});

describe("Exportar", () => {
  it("0 avisos: Exportar; enfileira o documento com a predefinição e sem ciências", async () => {
    await montar(<DialogoExportar />);
    await esperar(() => abrirExportar());
    await comAnalise([]);
    expect(exportarBtn().textContent).toBe("Exportar");
    expect(exportarBtn().disabled).toBe(false);
    semFrase();
    await clicar(exportarBtn());
    expect(cmd.edicaoEnfileirar).toHaveBeenCalledTimes(1);
    const [ws, doc, alvo, ciencias] = cmd.edicaoEnfileirar.mock.calls[0]!;
    expect([ws, (doc as ProjetoEdicao).id, alvo, ciencias]).toEqual([WS, st().doc!.id, { tipo: "final", exportacao: st().doc!.saida.exportacao }, []]);
    expect(useExportarUi.getState()).toMatchObject({ dialogo: false, fila: true });
  });

  it("análise que falha: o botão para de girar, fica desligado e o erro aparece (contratos (h) 27)", async () => {
    cmd.edicaoAnalisar.mockRejectedValue({ kind: "validation", message: "validation error: fonte_danificada: Vídeo danificado" });
    await montar(<DialogoExportar />);
    await esperar(() => abrirExportar());
    // uma mudança pede a análise de novo (250 ms depois)
    await esperar(() => st().alterar("Título", (d) => void (d.titulo = "x")));
    await esperar(() => new Promise((r) => setTimeout(r, 300)));
    expect(st().analiseFalhou).toBe(st().revisao);
    expect(exportarBtn().disabled).toBe(true);
    expect(exportarBtn().querySelector("svg")).toBeNull();
    expect(corpo().querySelector("[data-erro-analise]")?.textContent).toBe("Vídeo danificado");
    // a gravação que dá certo limpa o erro da store: o da análise fica (senão o botão voltaria a girar)
    await esperar(() => st().salvar());
    expect(st().erro).toBeNull();
    expect(exportarBtn().querySelector("svg")).toBeNull();
    expect(corpo().querySelector("[data-erro-analise]")?.textContent).toBe("Vídeo danificado");
  });

  it("análise que falhou é pedida de novo ao abrir o diálogo (contratos (i))", async () => {
    // a análise da abertura (timer 0) primeiro: a falha é a da mudança
    await vi.waitFor(() => expect(cmd.edicaoAnalisar).toHaveBeenCalled());
    cmd.edicaoAnalisar.mockRejectedValueOnce({ kind: "validation", message: "validation error: outra: Falha no ffmpeg" });
    await montar(<DialogoExportar />);
    await esperar(() => st().alterar("Título", (d) => void (d.titulo = "y")));
    await esperar(() => new Promise((r) => setTimeout(r, 300)));
    expect(st().analiseFalhou).toBe(st().revisao);
    const antes = cmd.edicaoAnalisar.mock.calls.length;
    // a falha era passageira: a resposta seguinte é boa
    cmd.edicaoAnalisar.mockResolvedValueOnce({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null } satisfies Analise);
    await esperar(() => abrirExportar());
    await esperar(() => new Promise((r) => setTimeout(r, 50)));
    expect(cmd.edicaoAnalisar.mock.calls.length).toBe(antes + 1);
    expect(st().analiseFalhou).toBe(-1);
    expect(exportarBtn().disabled).toBe(false);
  });

  it("1 aviso: Exportar com 1 aviso, sem ciência", async () => {
    await montar(<DialogoExportar />);
    await esperar(() => abrirExportar());
    await comAnalise([pr("aviso", "fps_diferente")]);
    expect(exportarBtn().textContent).toBe("Exportar com 1 aviso");
    expect(exportarBtn().disabled).toBe(false);
    expect(corpo().querySelectorAll('[data-nivel="aviso"]').length).toBe(1);
    semFrase();
  });

  it("3 avisos com periciais: botão só liga com Ciente em cada código; ciências vão para a fila", async () => {
    await montar(<DialogoExportar />);
    await esperar(() => abrirExportar());
    await comAnalise([pr("aviso", "fps_diferente"), pr("pericial", "taxa_variavel", { midia: "m1" }), pr("pericial", "taxa_variavel", { midia: "m2" })]);
    expect(exportarBtn().textContent).toBe("Exportar com 3 avisos");
    expect(exportarBtn().disabled).toBe(true);
    const ciente = corpo().querySelector<HTMLInputElement>('[data-codigo="taxa_variavel"] input[type="checkbox"]')!;
    semFrase();
    await clicar(ciente);
    expect(exportarBtn().disabled).toBe(false);
    await clicar(ciente);
    expect(exportarBtn().disabled).toBe(true);
    await clicar(ciente);
    await clicar(exportarBtn());
    expect(cmd.edicaoEnfileirar.mock.calls[0]![3]).toEqual(["taxa_variavel"]);
  });

  it("erro: bloqueia; a linha leva ao item e fecha o diálogo", async () => {
    await montar(<DialogoExportar />);
    await esperar(() => abrirExportar());
    const c2 = (st().doc!.principal[2] as { id: string }).id;
    const ef = (st().doc!.principal[2] as { efeitos: { id: string }[] }).efeitos[0]!.id;
    await comAnalise([pr("erro", "intervalo", { clipe: c2, efeito: ef }), pr("pericial", "taxa_variavel")]);
    await clicar(corpo().querySelector<HTMLInputElement>('[data-codigo="taxa_variavel"] input')!);
    expect(exportarBtn().disabled).toBe(true);
    expect(exportarBtn().textContent).toBe("Exportar com 1 aviso");
    semFrase();
    await clicar(corpo().querySelector<HTMLButtonElement>('[data-nivel="erro"] button')!);
    expect(st().selecao).toEqual({ itens: [{ k: "clipe", id: c2 }], foco: { k: "clipe", id: c2 }, efeito: ef });
    expect(useInspetorUi.getState().aba).toBe("clipe");
    expect(useExportarUi.getState().dialogo).toBe(false);
  });

  it("problema de ocultação da mídia (só o efeito) leva ao trecho que a usa, com ela aberta", async () => {
    await montar(<DialogoExportar />);
    await esperar(() => abrirExportar());
    const o = st().doc!.midias[0]!.ocultacoes[0]!;
    const c1 = (st().doc!.principal[1] as { id: string }).id;
    await comAnalise([pr("erro", "sem_cobertura", { efeito: o.id })]);
    await clicar(corpo().querySelector<HTMLButtonElement>('[data-nivel="erro"] button')!);
    expect(st().selecao.foco).toEqual({ k: "clipe", id: c1 });
    expect(st().selecao.efeito).toBe(o.id);
    expect(useExportarUi.getState().dialogo).toBe(false);
  });

  it("problema da saída leva à aba Saída; narração leva à aba Áudio", async () => {
    await montar(<DialogoExportar />);
    await esperar(() => abrirExportar());
    const audio = st().doc!.audios[0]!;
    await comAnalise([pr("erro", "saida", { campo: "saida.largura" }), pr("erro", "audio", { clipe: audio.clipes[0]!.id, trilha: audio.id })]);
    await clicar(corpo().querySelectorAll<HTMLButtonElement>('[data-nivel="erro"] button')[0]!);
    expect(useInspetorUi.getState().aba).toBe("saida");
    await esperar(() => abrirExportar());
    await clicar(corpo().querySelectorAll<HTMLButtonElement>('[data-nivel="erro"] button')[1]!);
    expect(st().selecao.foco).toEqual({ k: "audio", id: audio.clipes[0]!.id, trilha: audio.id });
    expect(useInspetorUi.getState().aba).toBe("audio");
  });

  it("análise velha (documento mudou depois dela): não exporta", async () => {
    await montar(<DialogoExportar />);
    await esperar(() => abrirExportar());
    await comAnalise([]);
    await esperar(() => st().alterar("Nome", (d) => void (d.titulo = "outro")));
    expect(exportarBtn().disabled).toBe(true);
  });

  it("Predefinição: Júri, WhatsApp e Preservação; só o júri escolhe o codificador", async () => {
    await montar(<DialogoExportar />);
    await esperar(() => abrirExportar());
    await comAnalise([]);
    const predef = campoDe<HTMLSelectElement>("predefinicao");
    const reprodutivel = () => [...corpo().querySelectorAll<HTMLInputElement>('input[type="checkbox"]')].find((x) => x.parentElement?.textContent === "Arquivo reprodutível")!;
    const marca = () => corpo().querySelector("[data-codificador]");
    expect([...predef.options].map((o) => [o.value, o.textContent])).toEqual([
      ["juri", "Júri"],
      ["whatsapp", "WhatsApp"],
      ["preservacao", "Preservação"],
    ]);
    expect([marca()?.getAttribute("data-codificador"), marca()?.querySelector("code")?.textContent, reprodutivel().disabled]).toEqual(["h264_nvenc", "h264_nvenc", false]);
    const n = st().passado.length;
    await escolher(predef, "whatsapp");
    expect([st().doc!.saida.exportacao.predefinicao, st().passado.length, st().passado.at(-1)?.rotulo]).toEqual(["whatsapp", n + 1, "Predefinição"]);
    // WhatsApp e Preservação codificam sempre em CPU: a marca não é escolha
    expect([marca()?.getAttribute("data-codificador"), marca()?.querySelector("code")?.textContent]).toEqual(["cpu", "libx264"]);
    expect([reprodutivel().checked, reprodutivel().disabled, st().doc!.saida.exportacao.codificador]).toEqual([true, true, "auto"]);
    await escolher(predef, "preservacao");
    expect([marca()?.getAttribute("data-codificador"), marca()?.querySelector("code")?.textContent, reprodutivel().disabled]).toEqual(["ffv1", "ffv1", true]);
    semFrase();
    await comAnalise([]);
    await clicar(exportarBtn());
    expect((cmd.edicaoEnfileirar.mock.calls[0]![2] as { exportacao: { predefinicao: string } }).exportacao.predefinicao).toBe("preservacao");
    // a personalizada não é oferecida; a do documento aparece e mostra o codificador dela
    const cap = await cmd.edicaoCapacidades.mock.results[0]!.value;
    const e = st().doc!.saida.exportacao;
    expect(codificadorDe({ ...e, predefinicao: "personalizada", personalizada: { codec: "hevc", conteiner: "mkv", qualidade: 20, altura_max: null, audio_kbps: 160 } }, cap)).toEqual({ nome: "libx265", marca: "cpu", gpu: false });
    expect(codificadorDe({ ...e, predefinicao: "juri", codificador: "cpu" }, cap)).toEqual({ nome: "libx264", marca: "cpu", gpu: false });
  });

  it("Legendas, Pacote do júri, Incluir originais, PDF sem anexos e Revisado por gravam na exportação e vão para a fila", async () => {
    await montar(<DialogoExportar />);
    await esperar(() => abrirExportar());
    await comAnalise([]);
    const exp = () => st().doc!.saida.exportacao;
    const [pacote, originais, pdf] = (["pacote", "originais", "pdf"] as const).map((c) => () => campoDe<HTMLInputElement>(c));
    expect([...campoDe<HTMLSelectElement>("legendas").options].map((o) => [o.value, o.textContent])).toEqual([
      ["nenhuma", "Não"],
      ["queimar", "Na imagem"],
      ["arquivo", "Arquivo"],
      ["ambos", "As duas"],
    ]);
    // sem pacote, as opções dele não mexem
    expect([exp().pacote, pacote!().checked, originais!().disabled, pdf!().disabled]).toEqual([null, false, true, true]);
    const n = st().passado.length;
    await escolher(campoDe<HTMLSelectElement>("legendas"), "ambos");
    await clicar(pacote!());
    expect([exp().pacote, originais!().disabled, originais!().checked, pdf!().checked]).toEqual([{ originais: false }, false, false, false]);
    await clicar(originais!());
    await clicar(pdf!());
    const revisado = campoDe<HTMLInputElement>("revisado_por");
    await digitar(revisado, "  Dra. Lia ");
    // digitar não grava a cada tecla: um passo só, ao sair do campo
    expect(exp().revisado_por).toBe("");
    await sair(revisado);
    expect(exp()).toMatchObject({ legendas: "ambos", pacote: { originais: true }, pdf: "a2_sem_anexos", revisado_por: "Dra. Lia" });
    expect(st().passado.slice(n).map((x) => x.rotulo)).toEqual(["Legendas", "Pacote do júri", "Incluir originais", "PDF sem anexos", "Revisado por"]);
    semFrase();
    await comAnalise([]);
    await clicar(exportarBtn());
    expect(cmd.edicaoEnfileirar.mock.calls[0]![2]).toEqual({ tipo: "final", exportacao: exp() });
    expect((cmd.edicaoEnfileirar.mock.calls[0]![2] as { exportacao: unknown }).exportacao).toMatchObject({ pacote: { originais: true }, pdf: "a2_sem_anexos", revisado_por: "Dra. Lia", legendas: "ambos" });
  });

  it("desligar o Pacote do júri tira os originais; o PDF sem anexos deixa de valer", async () => {
    await montar(<DialogoExportar />);
    await esperar(() => abrirExportar());
    await comAnalise([]);
    await clicar(campoDe<HTMLInputElement>("pacote"));
    await clicar(campoDe<HTMLInputElement>("originais"));
    await clicar(campoDe<HTMLInputElement>("pdf"));
    await clicar(campoDe<HTMLInputElement>("pacote"));
    expect(st().doc!.saida.exportacao.pacote).toBeNull();
    expect([campoDe<HTMLInputElement>("originais").checked, campoDe<HTMLInputElement>("pdf").checked, campoDe<HTMLInputElement>("pdf").disabled]).toEqual([false, false, true]);
  });

  it("Testar de novo refaz a sonda e a marca do codificador muda", async () => {
    await montar(<DialogoExportar />);
    await esperar(() => abrirExportar());
    await comAnalise([]);
    expect(cmd.edicaoCapacidades).toHaveBeenLastCalledWith(false);
    const cap = await cmd.edicaoCapacidades.mock.results[0]!.value;
    cmd.edicaoCapacidades.mockResolvedValueOnce({ ...cap, codificador: "libx264", gpu: null });
    await clicar(corpo().querySelector<HTMLButtonElement>('[aria-label="Testar de novo"]')!);
    expect(cmd.edicaoCapacidades).toHaveBeenLastCalledWith(true);
    expect(corpo().querySelector("[data-codificador]")?.getAttribute("data-codificador")).toBe("libx264");
    // a sonda que falha mostra o erro pelo mecanismo do diálogo
    cmd.edicaoCapacidades.mockRejectedValueOnce({ kind: "validation", message: "validation error: filtro_ausente: Filtro ausente no ffmpeg" });
    await clicar(corpo().querySelector<HTMLButtonElement>('[aria-label="Testar de novo"]')!);
    expect(corpo().textContent).toContain("Filtro ausente no ffmpeg");
  });

  it("Arquivo reprodutível = um passo; erro da fila aparece", async () => {
    await montar(<DialogoExportar />);
    await esperar(() => abrirExportar());
    await comAnalise([]);
    expect(corpo().querySelector("[data-codificador]")?.getAttribute("data-codificador")).toBe("h264_nvenc");
    const n = st().passado.length;
    await clicar([...corpo().querySelectorAll<HTMLInputElement>('input[type="checkbox"]')].find((x) => x.parentElement?.textContent === "Arquivo reprodutível")!);
    expect(st().doc!.saida.exportacao.codificador).toBe("cpu");
    expect([st().passado.length, st().passado.at(-1)?.rotulo]).toEqual([n + 1, "Arquivo reprodutível"]);
    expect(corpo().querySelector("[data-codificador]")?.getAttribute("data-codificador")).toBe("cpu");
    await comAnalise([]);
    cmd.edicaoEnfileirar.mockRejectedValueOnce({ kind: "validation", message: "validation error: tabela_pendente: Tabela de quadros em preparo" });
    await clicar(exportarBtn());
    expect(corpo().textContent).toContain("Tabela de quadros em preparo");
    expect(useExportarUi.getState().dialogo).toBe(true);
    await clicar(botao(corpo(), "Cancelar"));
    expect(useExportarUi.getState().dialogo).toBe(false);
  });
});

function campoDe<T extends HTMLElement>(nome: string): T {
  const el = corpo().querySelector<T>(`[data-campo="${nome}"]`);
  if (!el) throw new Error(`campo ${nome}`);
  return el;
}

const tarefa = (id: string, estado: Tarefa["estado"]): Tarefa => ({ id, tipo: "exportar", caso: "c1", projeto_id: "p", titulo: `Edição ${id}`, estado, criado_em: "2026-11-14T10:00:00Z" });
const saida = (id: string, x: Partial<SaidaRegistrada>): SaidaRegistrada => ({
  id,
  projeto_id: "p",
  titulo: "Edição",
  pasta: `Exportações/Vídeos/Edições/${id}`,
  arquivo: `Exportações/Vídeos/Edições/${id}/edicao.mp4`,
  sha256: "a".repeat(64),
  composto_sha256: "b".repeat(64),
  decodificado_sha256: null,
  audio_sha256: null,
  tamanho: 1,
  duracao_s: 47.3,
  n_quadros: 1419,
  predefinicao: "juri",
  codificador: "libx264",
  tecnico_rel: null,
  retrato_rel: "",
  projeto_sha256: "",
  autor: { nome: "", matricula: "", cargo: "" },
  refeita_de: null,
  estado: "concluida",
  exige_conferencia: false,
  conferida_por: null,
  conferida_em: null,
  criado_em: "2026-11-14T10:00:00Z",
  comparacao: null,
  ...x,
});

describe("Fila e Saídas", () => {
  it("fase, progresso, falha, cancelada; cancelar, ver registro, conferir", async () => {
    await montar(<Fila />);
    await esperar(() =>
      useRenderStore.getState().receberFila([
        tarefa("1", { tipo: "na_fila" }),
        tarefa("2", { tipo: "rodando", fase: { tipo: "blocos", feitos: 3, total: 7 }, feito_q: 300, total_q: 700, qps: 90, restante_s: 4.4 }),
        tarefa("3", { tipo: "falhou", mensagem: "Disco cheio", log_rel: "render/3/b001.log" }),
        tarefa("4", { tipo: "cancelada" }),
        tarefa("5", { tipo: "a_conferir", saida_id: "s5" }),
        tarefa("6", { tipo: "concluida", saida_id: "s6" }),
        { ...tarefa("7", { tipo: "pronta", relativo: "render/7/previa.mp4" }), tipo: "previa" },
      ]),
    );
    await clicar(corpo().querySelector<HTMLButtonElement>('[data-acao="fila"]')!);
    const estado = (id: string) => corpo().querySelector(`[data-estado] [title="Edição ${id}"]`)?.closest("li")?.querySelector("span:nth-of-type(2)")?.textContent;
    expect(["1", "2", "3", "4", "5", "6"].map(estado)).toEqual(["Na fila", "Vídeo 3/7", "Falhou", "Cancelado", "A conferir", "Pronto"]);
    // prévia não é exportação
    expect(estado("7")).toBeUndefined();
    semFrase();
    const rodando = corpo().querySelector<HTMLElement>('[data-estado="rodando"]')!;
    expect(rodando.querySelector<HTMLElement>("[style]")!.style.width).toBe(`${(300 / 700) * 100}%`);
    await esperar(() => useRenderStore.getState().receberProgresso({ tarefa_id: "2", fase: { tipo: "juncao" }, feito_q: 700, total_q: 700, qps: 0, restante_s: 0 }));
    expect(estado("2")).toBe("Juntando");
    await clicar(rodando.querySelector<HTMLButtonElement>('[aria-label="Cancelar"]')!);
    expect(cmd.edicaoCancelar).toHaveBeenCalledWith("2");
    expect(corpo().querySelector('[data-estado="falhou"] [aria-label="Cancelar"]')).toBeNull();
    await clicar(botao(corpo().querySelector('[data-estado="falhou"]')!, "Ver registro"));
    expect(cmd.revealPathInExplorer).toHaveBeenCalledWith("/cache/c1/render/3/b001.log");
    await clicar(botao(corpo().querySelector('[data-estado="a_conferir"]')!, "Concluir sem conferir"));
    expect(cmd.edicaoConferir).toHaveBeenCalledWith(WS, "s5", false);
    expect(rotuloEstado({ tipo: "rodando", fase: { tipo: "camadas" }, feito_q: 0, total_q: 1, qps: 0, restante_s: 0 })).toBe("Camadas");
  });

  it("saídas: estado por ícone, Conferido, Abrir pasta, Excluir com confirmação", async () => {
    cmd.edicaoSaidas.mockResolvedValue([
      saida("s1", { estado: "a_conferir", exige_conferencia: true }),
      saida("s2", { exige_conferencia: true, conferida_por: "Perito", conferida_em: "2026-11-14T11:00:00Z" }),
      saida("s3", { exige_conferencia: true }),
      saida("s4", {}),
    ]);
    await montar(<Fila />);
    await clicar(corpo().querySelector<HTMLButtonElement>('[data-acao="fila"]')!);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect([...corpo().querySelectorAll<HTMLElement>("[data-saidas] > li")].map((l) => l.dataset.estado)).toEqual(["a_conferir", "conferida", "nao_conferida", "pronta"]);
    semFrase();
    const primeira = corpo().querySelector<HTMLElement>("[data-saidas] > li")!;
    // "Conferido" pede confirmação (h 24): recusada, nada é registrado
    ask.askConfirm.mockClear();
    ask.askConfirm.mockResolvedValueOnce(false);
    await clicar(botao(primeira, "Conferido"));
    expect(ask.askConfirm).toHaveBeenCalledWith({ title: "Conferido", confirmLabel: "Conferido" });
    expect(cmd.edicaoConferir).not.toHaveBeenCalled();
    await clicar(botao(primeira, "Conferido"));
    expect(cmd.edicaoConferir).toHaveBeenCalledWith(WS, "s1", true);
    await clicar(primeira.querySelector<HTMLButtonElement>('[aria-label="Abrir pasta"]')!);
    expect(cmd.revealPathInExplorer).toHaveBeenCalledWith(`${WS}/Exportações/Vídeos/Edições/s1/edicao.mp4`);
    await clicar(primeira.querySelector<HTMLButtonElement>('[aria-label="Excluir"]')!);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(cmd.edicaoSaidaExcluir).toHaveBeenCalledWith(WS, "s1");
    expect(estadoDaSaida(saida("x", { exige_conferencia: true }))).toBe("nao_conferida");
  });

  it("Refazer e comparar: pede a refeita, mostra a fila; a refeita traz o resultado com o rótulo do §8.6", async () => {
    cmd.edicaoSaidas.mockResolvedValue([
      saida("s1", { estado: "a_conferir", exige_conferencia: true }),
      saida("r1", { refeita_de: "s0", comparacao: { tipo: "igual" } }),
      saida("r2", { refeita_de: "s0", comparacao: { tipo: "conteudo_igual", diferencas: ["codificador"] } }),
      saida("r3", { refeita_de: "s0", comparacao: { tipo: "diferente", primeiro_q: 2172, tempo: 72.4 } }),
      saida("r4", { refeita_de: "s0", comparacao: { tipo: "nao_comparavel", motivo: "fonte_danificada" } }),
      saida("s0", {}),
    ]);
    await montar(<Fila />);
    await clicar(corpo().querySelector<HTMLButtonElement>('[data-acao="fila"]')!);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    const linhas = [...corpo().querySelectorAll<HTMLElement>("[data-saidas] > li")];
    const resultado = (l: HTMLElement) => l.querySelector<HTMLElement>("[data-comparacao]");
    expect(linhas.map((l) => [resultado(l)?.dataset.comparacao ?? null, resultado(l)?.textContent ?? null])).toEqual([
      [null, null],
      ["igual", "Igual"],
      ["conteudo_igual", "Conteúdo igual, arquivo diferente"],
      ["diferente", "Diferente a partir de 00:01:12,400"],
      ["nao_comparavel", "Não comparável"],
      [null, null],
    ]);
    semFrase();
    // saída a conferir ainda não tem relatório: não se refaz
    expect(linhas.map((l) => l.querySelector('[data-acao="refazer"]') !== null)).toEqual([false, true, true, true, true, true]);
    const mostrarFila = useExportarUi.getState().mostrarFila;
    const mostrou = vi.fn(mostrarFila);
    await esperar(() => useExportarUi.setState({ mostrarFila: mostrou }));
    try {
      await clicar(botao(linhas[5]!, "Refazer e comparar"));
    } finally {
      await esperar(() => useExportarUi.setState({ mostrarFila }));
    }
    expect(cmd.edicaoRefazer.mock.calls).toEqual([[WS, "s0"]]);
    expect(mostrou.mock.calls).toEqual([[true]]);
    // tarefa de refazer aparece na fila como as exportações
    await esperar(() => useRenderStore.setState({ tarefas: [{ ...tarefa("t9", { tipo: "rodando", fase: { tipo: "pacote" }, feito_q: 9, total_q: 9, qps: 0, restante_s: 0 }), tipo: "refazer" }] }));
    expect(corpo().querySelector('[data-estado="rodando"]')?.textContent).toContain("Pacote do júri");
    expect([rotuloComparacao({ tipo: "diferente", primeiro_q: 0, tempo: 0 }), rotuloEstado({ tipo: "rodando", fase: { tipo: "relatorio" }, feito_q: 0, total_q: 0, qps: 0, restante_s: 0 })]).toEqual(["Diferente a partir de 00:00:00,000", "Relatório"]);
  });

  it("conferência em curso: os dois botões desligam e o segundo pedido não sai (o pacote é montado nela)", async () => {
    cmd.edicaoSaidas.mockResolvedValue([saida("s1", { estado: "a_conferir", exige_conferencia: true })]);
    let responder: (v: unknown) => void = () => {};
    cmd.edicaoConferir.mockReturnValueOnce(new Promise((r) => (responder = r)));
    await montar(<Fila />);
    await clicar(corpo().querySelector<HTMLButtonElement>('[data-acao="fila"]')!);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    const linha = () => corpo().querySelector<HTMLElement>("[data-saidas] > li")!;
    const botoes = () => [botao(linha(), "Conferido"), botao(linha(), "Concluir sem conferir")].map((b) => (b as HTMLButtonElement).disabled);
    expect(botoes()).toEqual([false, false]);
    await clicar(botao(linha(), "Concluir sem conferir"));
    expect([botoes(), conferenciaEmCurso("s1"), linha().querySelector("[data-conferindo]") !== null]).toEqual([[true, true], true, true]);
    await esperar(() => conferirSaida(WS, "s1", true));
    expect(cmd.edicaoConferir.mock.calls).toEqual([[WS, "s1", false]]);
    await esperar(async () => {
      responder({});
      await new Promise((r) => setTimeout(r, 0));
    });
    expect([botoes(), conferenciaEmCurso("s1")]).toEqual([[false, false], false]);
    semFrase();
  });

  it("Refazer e comparar que o backend recusa (retrato alterado) não abre a fila", async () => {
    cmd.edicaoSaidas.mockResolvedValue([saida("s0", {})]);
    cmd.edicaoRefazer.mockRejectedValueOnce({ kind: "validation", message: "validation error: retrato_divergente: Retrato da exportação alterado" });
    await montar(<Fila />);
    await clicar(corpo().querySelector<HTMLButtonElement>('[data-acao="fila"]')!);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    const antes = toasts().length;
    await clicar(botao(corpo(), "Refazer e comparar"));
    expect(cmd.edicaoRefazer).toHaveBeenCalledWith(WS, "s0");
    expect(toasts().slice(antes).map((t) => [t.kind, t.message])).toEqual([["error", "Retrato da exportação alterado"]]);
  });
});
