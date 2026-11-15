// @vitest-environment happy-dom
/**
 * Verificação da Fase 1 (lente de contratos): um projeto que usa TODO recurso da Fase 1, montado pelas portas da tela
 * (Inspetor, Presos e diálogo Exportar montados de verdade, com a store real; os trechos entram como o diálogo
 * "Trecho" os entrega) e gravado como o salvamento manda ao `edicao_salvar`. O Rust (`edicao::contratos_verif`) lê
 * o mesmo arquivo, troca as mídias de mentira pelas do caso e confere que lê tipado, valida sem erro, compila nos
 * três alvos e exporta com o pacote do júri.
 * As mídias de mentira imitam a `Fonte::Mp4` da matriz (320×180, 30 qps, base 1/15360, 450 quadros).
 * `SICRO_GRAVAR_FIXTURES=1` regrava; sem ela, o documento tem de sair igual ao arquivo, byte a byte.
 */

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import type {
  ClipeCongelar,
  ClipeTitulo,
  ClipeTrecho,
  Degravacao,
  EfEstabilizar,
  EfLupa,
  EfRegua,
  EfVelocidadeExibida,
  Efeito,
  MidiaInfo,
  MidiaRef,
  ProjetoEdicao,
  ProvenienciaMidia,
  RetratoCalculo,
  RetratoCalibracao,
  SobTitulo,
  TabelaQuadros,
} from "@domain/edicao";
import type { VideoMedia } from "@domain/video";
import type { VideoSpeedCalculation, VideoSpeedCalibration } from "@domain/video_speed";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoExcluir: vi.fn(),
  listVideoMedia: vi.fn(),
  videoProveniencia: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoQuadroEm: vi.fn(),
  edicaoMidiaImportar: vi.fn(),
  edicaoFundo: vi.fn(async () => []),
  edicaoOrigemPedir: vi.fn(),
  edicaoOrigemEstado: vi.fn(async () => null),
  edicaoDegravacao: vi.fn(),
  edicaoRetratoCalibracao: vi.fn(),
  edicaoRetratoCalculo: vi.fn(),
  listSpeedCalibrations: vi.fn(),
  listSpeedCalculations: vi.fn(),
  realceLenteObter: vi.fn(),
  edicaoCapacidades: vi.fn(),
  edicaoSpriteTexto: vi.fn(),
  edicaoAtlasCarimbo: vi.fn(),
  edicaoFila: vi.fn(async () => []),
  edicaoSaidas: vi.fn(async () => []),
  edicaoEnfileirar: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => p, invoke: vi.fn() }));
const dialogo = vi.hoisted(() => ({ open: vi.fn(async () => "/fora/arquivo") }));
vi.mock("@tauri-apps/plugin-dialog", () => dialogo);
const ask = vi.hoisted(() => ({ askConfirm: vi.fn(async () => true) }));
vi.mock("@components/Dialog/ask", () => ask);

import { useEdicaoStore } from "./edicaoStore";
import { midiaDoCaso } from "./coleta";
import { garantirMidia as garantirRecurso, recursoDe } from "../palco/recursos";
import { pontas } from "../sequencia/NovoClipe";
import { congelar, inserir, inserirTrechos, moverPara, selecionarClipe } from "../sequencia/acoes";
import { novoClipeImagem, novoClipeTitulo, novoTitulo } from "../sequencia/novos";
import { Presos } from "../sequencia/Presos";
import { Inspetor } from "../inspetor/Inspetor";
import { useInspetorUi } from "../inspetor/ui";
import { esquecerLeituras } from "../inspetor/contexto";
import { ESPERA, esquecerOrigens } from "../inspetor/secoes/RealceSecao";
import { arrastar, botao, campo, clicar, desmontarTudo, digitar, entradaDe, escolher, esperar, montar, sair, tecla, type Tela } from "../inspetor/testeTela";
import { abrirExportar } from "../exportar/avisos";
import { DialogoExportar } from "../exportar/DialogoExportar";
import { provenienciaDaMidia } from "@modules/video/editor/proveniencia";

const WS = "/casos/c-verif1";
const ARQUIVO = "verif-fase1-do-store.json";
/** As linhas da análise das listas "Calibração" e "Cálculo": o Rust as grava no banco do caso e pede os retratos. */
const ARQUIVO_ANALISE = "verif-fase1-analise.json";
const st = () => useEdicaoStore.getState();
const doc = () => st().doc!;
const sha = (c: string) => c.repeat(64);

const fila: string[] = [];
let n = 0;
const uuid = (i: number) => `00000000-0000-4000-8000-${String(i).padStart(12, "0")}`;

/** Tabela compacta da `Fonte::Mp4`: pts = 512·k, 450 quadros, chave a cada 30. */
const TABELA: TabelaQuadros = {
  tb: { num: 1, den: 15360 },
  pts0: 0,
  passos: Array.from({ length: 449 }, () => 512),
  passo_mediano: 512,
  chaves: Array.from({ length: 15 }, (_, i) => i * 30),
  vfr: false,
  origem: "decodificador",
};

function video(id: string, c: string, nome: string): VideoMedia {
  return {
    id,
    occurrence_id: "c-verif1",
    original_path: null,
    relative_path: `Mídias/Vídeos/${nome}`,
    filename: nome,
    sha256: sha(c),
    size_bytes: 1,
    duration_s: 15,
    codec: "h264",
    width: 320,
    height: 180,
    pixel_format: "yuv420p",
    fps_declared: 30,
    avg_frame_rate: "30/1",
    r_frame_rate: "30/1",
    time_base: "1/15360",
    frame_count: 450,
    bitrate: null,
    raw_probe_json: JSON.stringify({ format: { start_time: "0.000000" }, streams: [{ codec_type: "video" }, { codec_type: "audio" }] }),
    warnings_json: "[]",
    created_at: "2026-11-15T00:00:00Z",
    updated_at: "2026-11-15T00:00:00Z",
  };
}

const info = (id: string, c: string): MidiaInfo => ({
  id,
  sha256: sha(c),
  nome: "",
  exibido: { w: 320, h: 180 },
  taxa: { num: 30, den: 1 },
  vfr: false,
  passo: { min: 1 / 30, max: 1 / 30 },
  de_s: 0,
  ate_s: 15,
  n: 450,
  tem_audio: true,
  previa: { relativo: "", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 15, largura: 320, altura: 180, pronta: true },
  tabela: "decodificador",
  erros: 0,
  marcavel: true,
  cor: { matriz: "bt601", faixa: "tv" },
});

const propria = (id: string, c: string, tipo: "imagem" | "audio", nome: string): MidiaRef => ({
  id,
  tipo,
  origem: "projeto",
  media_id: null,
  sha256: sha(c),
  nome,
  relativo: `Sistema/videos/edicao/${uuid(1)}/midias/${sha(c)}.${tipo === "imagem" ? "png" : "wav"}`,
  duracao_s: tipo === "audio" ? 2 : null,
  inicio_s: null,
  largura: tipo === "imagem" ? 64 : null,
  altura: tipo === "imagem" ? 32 : null,
  taxa: null,
  vfr: false,
  tem_audio: tipo === "audio",
  cor: null,
  derivacao: null,
  recebido: { nativo: null, obs: "" },
  relogio_desvio_s: null,
  gravador: null,
  decodificacao: null,
  ocultacoes: [],
});
const IMAGEM = propria("m3", "3", "imagem", "brasao.png");
const NARRACAO = propria("m4", "4", "audio", "narracao.wav");
const MUSICA = propria("m5", "5", "audio", "fundo.wav");

/** Proveniência gravada na análise da câmera 1 (P1.10). */
const PROV: ProvenienciaMidia = { recebido_nativo: true, recebido_obs: "Pendrive do condomínio", relogio_desvio_s: -2.5, gravador: "DVR Intelbras" };
const SEM_PROV: ProvenienciaMidia = { recebido_nativo: null, recebido_obs: "", relogio_desvio_s: null, gravador: null };

/** Calibração no plano da câmera 1: 100 px por metro no espaço 320×180 (o retrato como o Rust o devolve). */
const CAL = "a0000000-0000-4000-8000-000000000001";
const CALC = "a0000000-0000-4000-8000-000000000002";
const RETRATO: RetratoCalibracao = {
  id: CAL,
  metodo: "plane",
  h: [0.01, 0, 0, 0, 0.01, 0, 0, 0, 1],
  espaco: { w: 320, h: 180 },
  pontos_sha256: "b".repeat(64),
  residuo_m: 0.01,
  regiao: [
    { x: 0.1, y: 0.2 },
    { x: 0.9, y: 0.2 },
    { x: 0.9, y: 0.9 },
    { x: 0.1, y: 0.9 },
  ],
  midia_sha256: sha("1"),
};
/** Velocidade medida entre 2,5 s e 3,5 s da câmera 1. */
const CALCULO: RetratoCalculo = { id: CALC, kmh: 41.6, faixa: [38.7, 45.2], de_s: 2.5, ate_s: 3.5, metodo: "monte_carlo", midia_sha256: sha("1") };

/** As linhas da análise que as listas "Calibração" e "Cálculo" mostram (o retrato vem do comando). */
const CALIBRACAO: VideoSpeedCalibration = {
  id: CAL, occurrence_id: "c-verif1", media_hash: sha("1"), method: "plane", reference_source: "campo", residuals_px: 0.01, distortion_model: null, author: "", created_at: "2026-11-14T10:00:00Z",
  control_points: [{ px: 32, py: 36, world_x_m: 0.32, world_y_m: 0.36 }, { px: 288, py: 36, world_x_m: 2.88, world_y_m: 0.36 }, { px: 288, py: 162, world_x_m: 2.88, world_y_m: 1.62 }, { px: 32, py: 162, world_x_m: 0.32, world_y_m: 1.62 }],
  homography: [0.01, 0, 0, 0, 0.01, 0, 0, 0, 1],
};
const CALCULO_DA_ANALISE: VideoSpeedCalculation = {
  id: CALC, occurrence_id: "c-verif1", media_hash: sha("1"), calibration_id: CAL, velocity_kmh: 41.6, vx_m_per_s: 11, vy_m_per_s: 3, se_m_per_s: 0.5, ci_low: 39, ci_high: 44,
  confidence: 0.95, r_squared: 0.99, residuals: [], mc_seed: 7, mc_sigmas: null, mc_n: 10000, mc_failed: 0, mc_mean_kmh: 41.7, mc_median_kmh: 41.6,
  mc_p2_5_kmh: 38.7, mc_p97_5_kmh: 45.2, limitations: [], audit: null, author: "", created_at: "2026-11-14T11:00:00Z",
  points: [2.5, 3.0, 3.5].map((t) => ({ storyboard_frame_id: null, export_id: null, px: 1, py: 2, u_px: 1, actual_timestamp_s: t, delta_s: null, manual: true })),
};

/** Degravação da câmera 1 (tempo de fonte): uma fala escrita pelo perito e um rascunho da IA. */
const DEGRAVACAO: Degravacao = {
  origem: { audio_sha256: sha("1"), importado_em: "2026-11-15T01:00:00Z", motor: "whisper", modelo_sha256: "c".repeat(64) },
  trechos: [
    { de: 2.2, ate: 2.8, texto: "O portão está fechado", locutor: "Testemunha", ia: false },
    { de: 3.5, ate: 3.9, texto: "Pode abrir", locutor: null, ia: true },
    { de: 3.35, ate: 3.45, texto: "Certo", locutor: null, ia: true },
  ],
};

const pausa = (ms = 0) => esperar(() => new Promise((r) => setTimeout(r, ms)));
const focarClipe = async (id: string) => {
  await esperar(() => selecionarClipe(id));
  await pausa(1);
};
const aba = async (tela: Tela, rotulo: string) => {
  await clicar(botao(tela.raiz, rotulo));
  await pausa(1);
};
const marca = (de: ParentNode, rotulo: string) => [...de.querySelectorAll("label")].find((l) => l.textContent?.trim() === rotulo)!.querySelector<HTMLInputElement>("input")!;
const q = <T extends HTMLElement>(de: ParentNode, s: string): T => {
  const e = de.querySelector<T>(s);
  if (!e) throw new Error(`sem ${s}`);
  return e;
};
/** "Adicionar → <rótulo>" pelo menu da seção de marcações. */
async function adicionar(tela: Tela, rotulo: string) {
  await clicar(q(tela.raiz, '[data-acao="adicionar"]'));
  const item = [...document.querySelectorAll<HTMLElement>("button, [role=menuitem]")].find((x) => x.textContent?.trim() === rotulo);
  if (!item) throw new Error(`Adicionar sem ${rotulo}`);
  await clicar(item);
  await pausa(1);
}
const efeitoDe = <E extends Efeito>(clipe: string, tipo: E["tipo"]) => (doc().principal.find((c) => (c as { id: string }).id === clipe) as ClipeTrecho).efeitos.find((e) => (e as Efeito).tipo === tipo) as E;
const linhaDoEfeito = (tela: Tela, tipo: string) => q<HTMLElement>(tela.raiz, `[data-efeito="${tipo}"], [data-realce="${tipo}"]`);

let documento: ProjetoEdicao | null = null;
const rotulos: string[] = [];

beforeAll(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-15T12:00:00.000Z") });
  vi.spyOn(globalThis.crypto, "randomUUID").mockImplementation(() => (fila.shift() ?? uuid(++n)) as `${string}-${string}-${string}-${string}-${string}`);
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString(), raiz_cache: "/cache/edicao/c-verif1" }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoAbrir.mockResolvedValue({ projeto: null, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/edicao/c-verif1" });
  const lista = [video("vm-1", "1", "cam1.mp4"), video("vm-2", "2", "cam2.mp4")];
  cmd.listVideoMedia.mockResolvedValue(lista);
  cmd.videoProveniencia.mockImplementation(async (_ws: string, id: string) => (id === "vm-1" ? PROV : SEM_PROV));
  cmd.edicaoMidiaInfo.mockImplementation(async (_ws: string, s: string) => (s === sha("1") ? info("vm-1", "1") : info("vm-2", "2")));
  cmd.edicaoTabelaQuadros.mockResolvedValue(TABELA);
  cmd.edicaoMidiaImportar.mockImplementation(async (_ws: string, _p: string, _o: string, tipo: string) => (tipo === "imagem" ? IMAGEM : NARRACAO));
  cmd.edicaoOrigemPedir.mockImplementation(async () => ({ tipo: "fazendo", tarefa: "t1", feito: 0 }));
  cmd.edicaoDegravacao.mockResolvedValue(DEGRAVACAO);
  cmd.listSpeedCalibrations.mockResolvedValue([CALIBRACAO]);
  cmd.listSpeedCalculations.mockResolvedValue([CALCULO_DA_ANALISE]);
  cmd.edicaoRetratoCalibracao.mockResolvedValue(RETRATO);
  cmd.edicaoRetratoCalculo.mockResolvedValue(CALCULO);
  cmd.realceLenteObter.mockResolvedValue({ nome: "Câmera do portão", k1: -0.08, k2: 0.01, cx: 0.5, cy: 0.5 });
  cmd.edicaoCapacidades.mockResolvedValue({ versao: "9.0.1", origem: "path", sha256: "", faltam: [], opcionais_ausentes: [], codificador: "libx264", gpu: null, reprovados: [], sondando: false });
  cmd.edicaoSpriteTexto.mockRejectedValue(new Error("sem backend"));
  cmd.edicaoAtlasCarimbo.mockRejectedValue(new Error("sem backend"));
  esquecerLeituras();
  esquecerOrigens();
  Object.assign(ESPERA, { mudanca: 5, fazendo: 5, tabela: 5 });
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });

  await st().novo(WS);
  const tela = await montar(<Inspetor />);

  // ---- Saída: 640×360 a 30 qps, fundo escuro (aba Saída)
  await aba(tela, "Saída");
  await digitar(entradaDe(tela.raiz, "Largura") as HTMLInputElement, "640", { aos: 3 });
  await digitar(entradaDe(tela.raiz, "Altura") as HTMLInputElement, "360", { aos: 3 });
  await aba(tela, "Clipe");

  // ---- três trechos pelo diálogo "Trecho" (o que ele entrega: mídia com a proveniência da análise e as pontas)
  for (const v of lista) garantirRecurso(WS, v.sha256);
  await pausa(1);
  const trecho = async (v: VideoMedia, ini: number, fim: number) => {
    const r = recursoDe(v.sha256);
    const p = pontas(r.tabela, ini, fim);
    if (!p) throw new Error("pontas");
    return { midia: midiaDoCaso(v, lista, r.info, await provenienciaDaMidia(WS, v.id)), ...p };
  };
  fila.push("m1");
  await esperar(async () => inserirTrechos([await trecho(lista[0]!, 2.0, 4.0)]));
  fila.push("m2");
  await esperar(async () => inserirTrechos([await trecho(lista[1]!, 5.0, 7.0)]));
  await esperar(async () => inserirTrechos([await trecho(lista[0]!, 8.0, 9.0)]));
  const [a, b, c3] = doc().principal as ClipeTrecho[];
  if (!a || !b || !c3) throw new Error("trechos");

  // ---- trecho A (câmera 1): lupa, régua e velocidade pelo "Adicionar"; som com silêncio, volume animado
  await focarClipe(a.id);
  await adicionar(tela, "Destaque");
  await clicar(q(linhaDoEfeito(tela, "destaque"), '[data-acao="marcar"]'));
  await esperar(() => useInspetorUi.getState().marcado({ x: 0.6, y: 0.5, w: 0.15, h: 0.2 }, 2.0, 60 * 512));
  await adicionar(tela, "Lupa");
  {
    const l = linhaDoEfeito(tela, "lupa");
    await clicar(q(campo(l, "Forma"), '[data-valor="retangulo"]'));
    await digitar(entradaDe(l, "Ampliação") as HTMLInputElement, "2,5");
    await digitar(entradaDe(l, "Janela") as HTMLInputElement, "25");
    await clicar(q(campo(l, "Posição"), '[data-valor="esq-topo"]'));
    const lupa = efeitoDe<EfLupa>(a.id, "lupa");
    // "Marcar" o alvo no palco: a caixa no quadro mostrado (o par t/pts que o palco entrega)
    await clicar(q(l, '[data-acao="marcar"]'));
    await esperar(() => useInspetorUi.getState().marcado({ x: 0.55, y: 0.45, w: 0.1, h: 0.12 }, 2.0, 60 * 512));
    expect(doc().rastreios.find((r) => r.id === lupa.rastreio)!.ancoras).toHaveLength(1);
  }
  await adicionar(tela, "Régua");
  {
    const l = linhaDoEfeito(tela, "regua");
    await digitar(entradaDe(l, "Comprimento (m)") as HTMLInputElement, "1,5");
    await digitar(entradaDe(l, "Divisões") as HTMLInputElement, "3");
    await clicar(q(l, '[data-acao="marcar-ancora"]'));
    await esperar(() => useInspetorUi.getState().marcado({ x: 0.3, y: 0.8, w: 0, h: 0 }, 2.0, 60 * 512));
  }
  await adicionar(tela, "Velocidade");
  expect(efeitoDe<EfVelocidadeExibida>(a.id, "velocidade_exibida").ativo).toEqual({ de: 2.5, ate: 3.5 });
  // som do clipe: silêncio no cursor (Tom, com motivo), fala da degravação silenciada (Mudo) e volume em duas chaves
  await esperar(() => st().posicionar({ modo: "clipe", clipe: a.id, t: 3.0 + 1 / 60 }));
  await clicar(q(tela.raiz, '[data-acao="silenciar"]'));
  {
    const sil = q<HTMLElement>(tela.raiz, "[data-silencio], [data-silencios] li");
    await digitar(entradaDe(sil, "Até") as HTMLInputElement, "00:00:03,300");
    await tecla(entradaDe(sil, "Até") as HTMLInputElement, "Enter");
    await digitar(entradaDe(sil, "Motivo") as HTMLInputElement, "Nome de testemunha", { aos: 18 });
    await sair(entradaDe(sil, "Motivo"));
  }
  await clicar(q(tela.raiz, '[data-acao="da-degravacao"]'));
  await pausa(1);
  await clicar(q(tela.raiz, '[data-fala="3.5"]'));
  await esperar(() => st().posicionar({ modo: "clipe", clipe: a.id, t: 2.0 + 1 / 60 }));
  await clicar(q(tela.raiz, '[data-acao="marcar-volume"]'));
  await esperar(() => st().posicionar({ modo: "clipe", clipe: a.id, t: 2.5 + 1 / 60 }));
  await clicar(q(tela.raiz, '[data-acao="marcar-volume"]'));
  await arrastar(q<HTMLInputElement>(tela.raiz, '[data-chave-volume="1"] input[type="range"]'), [0.8, 0.6]);

  // ---- trecho B (câmera 2): entrada por fusão com o som cortado, a "Cor" escolhida e os seis efeitos de origem
  await focarClipe(b.id);
  await clicar(q(tela.raiz, '[data-campo="Transição"] button[data-valor="fundir"]'));
  await digitar(entradaDe(tela.raiz, "Duração (s)") as HTMLInputElement, "0,5");
  await clicar(q(tela.raiz, '[data-campo="Som"] button[data-valor="cortar"]'));
  await escolher(entradaDe(tela.raiz, "Cor") as HTMLSelectElement, "bt709:tv");
  // ocultação da mídia (espaço original), marcada no primeiro quadro do trecho
  await adicionar(tela, "Ocultar");
  await clicar(q(linhaDoEfeito(tela, "ocultar"), '[data-acao="marcar"]'));
  await esperar(() => useInspetorUi.getState().marcado({ x: 0.7, y: 0.1, w: 0.2, h: 0.2 }, 5.0, 150 * 512));
  for (const [tipo, rotulo] of [["desentrelacar", "Desentrelaçar"], ["estabilizar", "Estabilizar"], ["lente", "Lente"], ["ajustes", "Brilho"], ["nitidez", "Nitidez"], ["media_temporal", "Média de quadros"]] as const) {
    await clicar(botao(linhaDoEfeito(tela, tipo), rotulo));
    await pausa(2);
    expect(efeitoDe(b.id, tipo), tipo).toBeTruthy();
  }
  await arrastar(entradaDe(linhaDoEfeito(tela, "ajustes"), "Contraste") as HTMLInputElement, [1.1]);
  await arrastar(q<HTMLInputElement>(linhaDoEfeito(tela, "ajustes"), 'input[type="range"]'), [0.05]);
  await arrastar(q<HTMLInputElement>(linhaDoEfeito(tela, "nitidez"), 'input[type="range"]'), [0.8]);
  await arrastar(q<HTMLInputElement>(linhaDoEfeito(tela, "media_temporal"), 'input[type="range"]'), [3]);
  expect(efeitoDe<EfEstabilizar>(b.id, "estabilizar").analise).toEqual({ de_s: 5, ate_s: 7 - 1 / 30, de_pts: 150 * 512, ate_pts: 209 * 512 });

  // ---- trecho C (câmera 1): congelar no quadro 255 (divide o trecho) e nitidez no congelado
  await esperar(() => st().posicionar({ modo: "clipe", clipe: c3.id, t: 8.5 + 1 / 60 }));
  await focarClipe(c3.id);
  await esperar(() => st().posicionar({ modo: "clipe", clipe: c3.id, t: 8.5 + 1 / 60 }));
  await esperar(() => congelar());
  const cong = doc().principal.find((x) => (x as { tipo: string }).tipo === "congelar") as ClipeCongelar;
  await focarClipe(cong.id);
  await digitar(entradaDe(tela.raiz, "Duração (s)") as HTMLInputElement, "1");
  await clicar(botao(linhaDoEfeito(tela, "nitidez"), "Nitidez"));
  await pausa(2);

  // ---- títulos pelo diálogo "Título" (os modelos do P1.7): abertura no começo, encerramento no fim; imagem como clipe
  const canvas = () => ({ w: doc().saida.largura, h: doc().saida.altura });
  await esperar(() => inserir("Título", () => [novoClipeTitulo(novoTitulo("abertura", "Ocorrência 123/2026", "Perícia de vídeo", canvas()), 1)]));
  const abertura = doc().principal.find((x) => (x as { tipo: string }).tipo === "titulo") as ClipeTitulo;
  await esperar(() => moverPara(doc().principal.indexOf(abertura), 0));
  await focarClipe(abertura.id);
  await digitar(entradaDe(tela.raiz, "Rodapé") as HTMLTextAreaElement, "Laudo 77/2026", { aos: 13 });
  await sair(entradaDe(tela.raiz, "Rodapé"));
  await esperar(() => st().selecionar({ itens: [], foco: null, efeito: null }));
  await esperar(() => st().posicionar({ modo: "programa", q: 0 }));
  const img = (await cmd.edicaoMidiaImportar(WS, doc().id, "/fora/brasao.png", "imagem")) as MidiaRef;
  await esperar(() => st().alterar("Imagem", (d) => void (d.midias.some((m) => m.id === img.id) || d.midias.push(img))));
  await esperar(() => inserir("Imagem", () => [novoClipeImagem(img.id, 1)]));
  await esperar(() => inserir("Título", () => [novoClipeTitulo(novoTitulo("encerramento", "Fim do exame", "", canvas()), 1)]));
  await esperar(() => inserir("Título", () => [novoClipeTitulo(novoTitulo("cartela", "Câmera 1, mais tarde", "", canvas()), 1)]));
  const cartela = doc().principal.at(-1) as ClipeTitulo;
  await esperar(() => moverPara(doc().principal.findIndex((x) => (x as { id: string }).id === cartela.id), doc().principal.findIndex((x) => (x as { id: string }).id === c3.id)));
  const fim = doc().principal.at(-1) as ClipeTitulo;
  expect(fim.tipo).toBe("titulo");

  // ---- presos ao clipe (cartão → "Adicionar"): terço inferior no trecho A, título de evento no B, imagem e narração no A
  await focarClipe(a.id);
  const presos = await montar(<Presos clipe={a.id} />);
  await clicar(botao(presos.raiz, "Adicionar"));
  await clicar(botao(presos.raiz, "Terço inferior"));
  await pausa(1);
  const terco = doc().sobreposicoes[0]!.clipes.at(-1) as SobTitulo;
  await digitar(entradaDe(tela.raiz, "Título") as HTMLTextAreaElement, "Rua das Flores, 120", { aos: 19 });
  await sair(entradaDe(tela.raiz, "Título"));
  expect((doc().sobreposicoes[0]!.clipes.find((x) => (x as SobTitulo).id === terco.id) as SobTitulo).duracao_s).toBeNull();
  await focarClipe(a.id);
  dialogo.open.mockResolvedValueOnce("/fora/brasao.png");
  await clicar(botao(presos.raiz, "Adicionar"));
  await clicar(botao(presos.raiz, "Imagem"));
  await pausa(2);
  cmd.edicaoMidiaImportar.mockResolvedValueOnce(NARRACAO);
  dialogo.open.mockResolvedValueOnce("/fora/narracao.wav");
  await focarClipe(a.id);
  await clicar(botao(presos.raiz, "Adicionar"));
  await clicar(botao(presos.raiz, "Narração"));
  await pausa(2);
  presos.desmontar();
  await focarClipe(b.id);
  const presosB = await montar(<Presos clipe={b.id} />);
  await clicar(botao(presosB.raiz, "Adicionar"));
  await clicar(botao(q(presosB.raiz, '[role="menu"]'), "Título"));
  await pausa(1);
  await digitar(entradaDe(tela.raiz, "Título") as HTMLTextAreaElement, "Chegada do veículo", { aos: 18 });
  await sair(entradaDe(tela.raiz, "Título"));
  presosB.desmontar();

  // ---- aba Áudio: música importada, abaixar sob a narração, limitador (a normalização nasce ligada)
  await focarClipe(b.id);
  await aba(tela, "Áudio");
  cmd.edicaoMidiaImportar.mockResolvedValueOnce(MUSICA);
  await clicar(q(tela.raiz, '[data-papel="musica"] [data-acao="importar"]'));
  await pausa(2);
  await clicar(marca(tela.raiz, "Abaixar sob a narração"));
  await clicar(marca(tela.raiz, "Limitador"));
  expect(doc().mixagem).toMatchObject({ normalizar: { lufs: -16, pico_db: -1.5 }, limitador: true });

  // ---- aba Legendas: da degravação (a da IA conferida), uma nova no trecho A e uma no programa; posição e linhas
  await focarClipe(a.id);
  await esperar(() => st().posicionar({ modo: "clipe", clipe: a.id, t: 2.9 + 1 / 60 }));
  await aba(tela, "Legendas");
  const painel = () => q<HTMLElement>(tela.raiz, '[data-painel="legendas"]');
  await clicar(botao(painel(), "Da degravação…"));
  await clicar(q(painel(), '[data-midia="m1"]'));
  await pausa(2);
  const [silenciada, certa] = doc().legendas.itens.filter((g) => g.origem === "ia");
  const linhaDa = (id: string) => q<HTMLElement>(painel(), `[data-legenda="${id}"]`);
  // a fala sob o silêncio do clipe fica de fora (legenda sob silêncio barra a exportação); a outra da IA é conferida
  await clicar(q(linhaDa(silenciada!.id), '[data-acao="abrir-legenda"]'));
  await clicar(q(linhaDa(silenciada!.id), '[aria-label="Remover"]'));
  await clicar(q(linhaDa(certa!.id), '[data-acao="abrir-legenda"]'));
  await clicar(entradaDe(linhaDa(certa!.id), "Conferida") as HTMLInputElement);
  await clicar(q(linhaDa(certa!.id), '[data-acao="abrir-legenda"]'));
  await clicar(botao(painel(), "Nova legenda"));
  {
    const g = doc().legendas.itens.at(-1)!;
    const l = () => q<HTMLElement>(painel(), `[data-legenda="${g.id}"]`);
    await digitar(l().querySelector("textarea") as HTMLTextAreaElement, "Mãos para cima!", { aos: 15 });
    await sair(l().querySelector("textarea") as HTMLTextAreaElement);
    await digitar(entradaDe(l(), "Até") as HTMLInputElement, "00:00:03,000");
    await sair(entradaDe(l(), "Até"));
  }
  await esperar(() => st().posicionar({ modo: "programa", q: 6 }));
  await clicar(botao(painel(), "Nova legenda"));
  {
    const g = doc().legendas.itens.at(-1)!;
    const l = () => q<HTMLElement>(painel(), `[data-legenda="${g.id}"]`);
    await digitar(l().querySelector("textarea") as HTMLTextAreaElement, "Abertura do exame", { aos: 17 });
    await sair(l().querySelector("textarea") as HTMLTextAreaElement);
    await digitar(entradaDe(l(), "Até") as HTMLInputElement, "00:00:00,800");
    await sair(entradaDe(l(), "Até"));
  }
  await clicar(q(campo(painel(), "Posição"), '[data-valor="centro-base"]'));
  await digitar(entradaDe(painel(), "Linhas") as HTMLInputElement, "3");

  // ---- aba Saída: fecho escurecendo e o selo de velocidade desligado
  await aba(tela, "Saída");
  await clicar(botao(campo(tela.raiz, "Transição"), "Escurecer"));
  await digitar(entradaDe(tela.raiz, "Duração (s)") as HTMLInputElement, "0,3");
  await clicar(entradaDe(tela.raiz, "Selo de velocidade") as HTMLInputElement);

  // ---- diálogo Exportar: júri, arquivo reprodutível, legendas nas duas formas, pacote com originais, revisor
  await montar(<DialogoExportar />);
  await esperar(() => abrirExportar());
  await pausa(2);
  const campoDe = <T extends HTMLElement>(c: string) => q<T>(document.body, `[data-campo="${c}"]`);
  await escolher(campoDe<HTMLSelectElement>("legendas"), "ambos");
  await clicar(campoDe<HTMLInputElement>("pacote"));
  await clicar(campoDe<HTMLInputElement>("originais"));
  await digitar(campoDe<HTMLInputElement>("revisado_por"), "Dra. Lia", { aos: 8 });
  await sair(campoDe<HTMLInputElement>("revisado_por"));
  await clicar([...document.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')].find((x) => x.parentElement?.textContent === "Arquivo reprodutível")!);

  rotulos.push(...st().passado.map((p) => p.rotulo));
  await st().descarregar();
  documento = cmd.edicaoSalvar.mock.calls.at(-1)?.[1] as ProjetoEdicao;
});

afterAll(async () => {
  desmontarTudo();
  await st().fechar({ descartar: true }).catch(() => {});
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("projeto da Fase 1 montado pelas portas da tela", () => {
  it("usa todo recurso da Fase 1", () => {
    const d = documento!;
    expect(d.midias.map((m) => [m.id, m.tipo, m.origem])).toEqual([
      ["m1", "video", "caso"],
      ["m2", "video", "caso"],
      ["m3", "imagem", "projeto"],
      ["m4", "audio", "projeto"],
      ["m5", "audio", "projeto"],
    ]);
    // proveniência da análise e a "Cor" escolhida chegam ao MidiaRef
    expect(d.midias[0]).toMatchObject({ recebido: { nativo: true, obs: "Pendrive do condomínio" }, relogio_desvio_s: -2.5, gravador: "DVR Intelbras", cor: null });
    expect(d.midias[1]).toMatchObject({ recebido: { nativo: null, obs: "" }, gravador: null, cor: { matriz: "bt709", faixa: "tv" } });
    expect(d.principal.map((c) => (c as { tipo: string }).tipo)).toEqual(["titulo", "trecho", "trecho", "titulo", "trecho", "congelar", "trecho", "imagem", "titulo"]);
    const [, a, b, cartela, , cong] = d.principal as [ClipeTitulo, ClipeTrecho, ClipeTrecho, ClipeTitulo, ClipeTrecho, ClipeCongelar];
    expect(cartela.titulo.modelo).toBe("cartela");
    expect(a.efeitos.map((e) => (e as Efeito).tipo)).toEqual(["destaque", "lupa", "regua", "velocidade_exibida"]);
    expect((a.efeitos[2] as EfRegua).calibracao).toEqual(RETRATO);
    expect((a.efeitos[3] as EfVelocidadeExibida).calculo).toEqual(CALCULO);
    // a ocultação marcada no trecho B foi para a mídia (D17), com o `ativo` na faixa do trecho (tempo de fonte)
    expect(d.midias[1]!.ocultacoes.map((o) => [o.tipo, o.ativo])).toEqual([["ocultar", { de: 5, ate: 7 }]]);
    expect(a.audio.silencios.map((s) => [s.de, s.ate, s.modo])).toEqual([
      [3 + 1 / 60, 3.3, "tom"],
      [3.5, 3.9, "tom"],
    ]);
    expect(Array.isArray(a.audio.volume) && a.audio.volume.map((k) => k.v)).toEqual([1, 0.6]);
    expect(b.transicao_entrada).toEqual({ tipo: "fundir", duracao_s: 0.5, audio: "cortar" });
    expect(b.efeitos.map((e) => (e as Efeito).tipo).sort()).toEqual(["ajustes", "desentrelacar", "estabilizar", "lente", "media_temporal", "nitidez"]);
    expect(cong.efeitos.map((e) => (e as Efeito).tipo)).toEqual(["nitidez"]);
    expect((d.principal[0] as ClipeTitulo).titulo.modelo).toBe("abertura");
    expect((d.principal.at(-1) as ClipeTitulo).titulo.modelo).toBe("encerramento");
    expect(d.fecho).toEqual({ tipo: "escurecer", duracao_s: 0.3, audio: "acompanhar" });
    expect(d.sobreposicoes[0]!.clipes.map((c) => [(c as { tipo: string }).tipo, (c as SobTitulo).titulo?.modelo ?? null])).toEqual([
      ["titulo", "terco_inferior"],
      ["imagem", null],
      ["titulo", "evento"],
    ]);
    expect(d.audios.map((t) => [t.papel, t.abaixa_original, t.clipes.length])).toEqual([
      ["narracao", true, 1],
      ["musica", false, 1],
    ]);
    expect(d.mixagem).toMatchObject({ normalizar: { lufs: -16, pico_db: -1.5 }, limitador: true });
    expect(d.legendas.itens.map((g) => [g.origem, g.conferida, g.ref.tipo, g.texto])).toEqual([
      ["perito", true, "fonte", "O portão está fechado"],
      ["ia", true, "fonte", "Certo"],
      ["perito", true, "fonte", "Mãos para cima!"],
      ["perito", true, "programa", "Abertura do exame"],
    ]);
    expect(d.legendas).toMatchObject({ linhas_max: 3, origem: DEGRAVACAO.origem });
    expect(d.saida).toMatchObject({ auto: false, largura: 640, altura: 360, selos: { velocidade: false, ampliacao: true, realce: true } });
    expect(d.saida.exportacao).toEqual({ predefinicao: "juri", codificador: "cpu", legendas: "ambos", pacote: { originais: true }, pdf: "a3", revisado_por: "Dra. Lia", personalizada: null });
  });

  it("grava (SICRO_GRAVAR_FIXTURES=1) ou confere as fixtures byte a byte", () => {
    const pasta = process.env.SICRO_FIXTURES_EDICAO ?? join(process.cwd(), "src-tauri/tests/fixtures/edicao");
    for (const [arquivo, valor] of [
      [ARQUIVO, documento],
      [ARQUIVO_ANALISE, { calibracao: CALIBRACAO, calculo: CALCULO_DA_ANALISE }],
    ] as const) {
      const texto = `${JSON.stringify(valor, null, 2)}\n`;
      const caminho = join(pasta, arquivo);
      if (process.env.SICRO_GRAVAR_FIXTURES === "1") writeFileSync(caminho, texto);
      expect(readFileSync(caminho, "utf8"), arquivo).toBe(texto);
    }
  });
});
