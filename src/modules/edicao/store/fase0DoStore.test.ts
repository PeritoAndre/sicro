/**
 * Verificação da Fase 0 (lente de contratos): um projeto que usa TODO recurso da Fase 0 montado só pelas portas da
 * tela (store, Sequência, Inspetor, diálogo de exportação), gravado como o salvamento manda ao `edicao_salvar`.
 * O Rust (`edicao::contratos_verif`) lê o mesmo arquivo, troca as mídias de mentira pelas do caso e confere que
 * lê tipado, valida sem erro e compila para os três alvos. As mídias de mentira imitam a `Fonte::Mp4` da matriz
 * (320×180, 30 qps, base 1/15360, 450 quadros, quadro-chave a cada 30).
 * `SICRO_GRAVAR_FIXTURES=1` regrava; sem ela, o documento tem de sair igual ao arquivo, byte a byte.
 */

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import type {
  ClipeAudio,
  ClipeCongelar,
  ClipeImagem,
  ClipeTitulo,
  ClipeTrecho,
  EfCarimbo,
  EfDestaque,
  EfOcultar,
  EfTexto,
  Exportacao,
  MidiaInfo,
  MidiaRef,
  ProjetoEdicao,
  SobImagem,
  SobTitulo,
  TabelaQuadros,
} from "@domain/edicao";
import type { VideoMedia } from "@domain/video";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoExcluir: vi.fn(),
  listVideoMedia: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoQuadroEm: vi.fn(),
  edicaoMidiaImportar: vi.fn(),
  edicaoFundo: vi.fn(async () => []),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => p, invoke: vi.fn() }));

import { useEdicaoStore } from "./edicaoStore";
import { garantirMidia as garantirRecurso, recursoDe } from "../palco/recursos";
import { garantirMidia } from "../modelo/projeto";
import { midiaDoCaso } from "./coleta";
import { pontas } from "../sequencia/NovoClipe";
import { congelar, inserir, inserirTrechos, moverPara, selecionarClipe } from "../sequencia/acoes";
import { novoClipeImagem, novoClipeTitulo, tituloDaFase0 as novoTitulo, prenderImagem, prenderNarracao, prenderTitulo } from "../sequencia/novos";
import {
  adicionarEfeito,
  adicionarImagem,
  adicionarNarracao,
  marcarAncora,
  marcarRegiao,
  mudar,
  mudarGiro,
  noClipe,
  noEfeito,
  soNesteClipe,
  transicaoEm,
} from "../inspetor/acoes";
import { estiloPadrao, TAMANHO_TEXTO } from "../inspetor/novos";
import { escreverBloco } from "../inspetor/secoes/TituloSecao";

const WS = "/casos/c-verif";
const ARQUIVO = "verif-fase0-do-store.json";
const st = () => useEdicaoStore.getState();
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
    occurrence_id: "c-verif",
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
    created_at: "2026-11-14T00:00:00Z",
    updated_at: "2026-11-14T00:00:00Z",
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
  cor: { matriz: "bt709", faixa: "tv" },
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

/** Espera os pedidos da tela ao backend (mocks resolvidos em microtarefas). */
async function vazar() {
  for (let i = 0; i < 20; i++) await Promise.resolve();
}

let documento: ProjetoEdicao | null = null;
/** Trilha criada por "Preso ao clipe → Narração" da Sequência (num projeto à parte). */
let trilhaDaSequencia: { abaixa_original: boolean } | null = null;

beforeAll(async () => {
  vi.useFakeTimers({ now: new Date("2026-11-14T12:00:00.000Z") });
  vi.spyOn(globalThis.crypto, "randomUUID").mockImplementation(() => (fila.shift() ?? uuid(++n)) as `${string}-${string}-${string}-${string}-${string}`);
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoAbrir.mockResolvedValue({ projeto: null, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/edicao/c-verif" });
  const lista = [video("vm-1", "1", "cam1.mp4"), video("vm-2", "2", "cam2.mp4")];
  cmd.listVideoMedia.mockResolvedValue(lista);
  cmd.edicaoMidiaInfo.mockImplementation(async (_ws: string, s: string) => (s === sha("1") ? info("vm-1", "1") : info("vm-2", "2")));
  cmd.edicaoTabelaQuadros.mockResolvedValue(TABELA);
  cmd.edicaoMidiaImportar.mockImplementation(async (_ws: string, _p: string, _o: string, tipo: string) => (tipo === "imagem" ? IMAGEM : NARRACAO));

  await st().novo(WS);
  // a fixture (lida pelo Rust) é de projeto sem normalização: desligada como na tela (Áudio → "Normalizar")
  st().alterar("Normalizar", (d) => void (d.mixagem.normalizar = null));
  // Saída manual 640×360 a 30 qps, fundo escuro, com áudio
  mudar("Largura", (d) => void ((d.saida.largura = 640), (d.saida.auto = false)));
  mudar("Altura", (d) => void ((d.saida.altura = 360), (d.saida.auto = false)));
  mudar("Fundo", (d) => void (d.saida.cor_fundo = "#101820"));

  // dois trechos pelo diálogo "Trecho" (tabela e info pelo palco/recursos, pontas pelo espelho)
  for (const v of lista) garantirRecurso(WS, v.sha256);
  await vazar();
  const trecho = (v: VideoMedia, ini: number, fim: number) => {
    const r = recursoDe(v.sha256);
    const p = pontas(r.tabela, ini, fim);
    if (!p) throw new Error("pontas");
    return { midia: midiaDoCaso(v, lista, r.info), ...p };
  };
  fila.push("m1");
  inserirTrechos([trecho(lista[0] as VideoMedia, 2.0, 7.0)]);
  fila.push("m2");
  inserirTrechos([trecho(lista[1] as VideoMedia, 1.0, 5.0)]);
  const [c1, c2] = st().doc!.principal as ClipeTrecho[];
  if (!c1 || !c2) throw new Error("trechos");

  // congelar no quadro 120 do trecho 1 (divide o trecho; o congelado fica entre as partes)
  st().posicionar({ modo: "clipe", clipe: c1.id, t: 4.0 + 1 / 60 });
  congelar();
  const cong = st().doc!.principal.find((c) => (c as { tipo: string }).tipo === "congelar") as ClipeCongelar;
  if (!cong) throw new Error("congelado");
  mudar("Duração (s)", noClipe<ClipeCongelar>(cong.id, (x) => void (x.duracao_s = 1.5)));
  adicionarEfeito(cong.id, "destaque", "Destaque");
  adicionarEfeito(cong.id, "ocultar", "Ocultar");

  // trecho 1: velocidade, volume, destaque em seta com rótulo e em colchetes marcados no palco, ocultação da mídia
  // em elipse com borda suave, texto, três carimbos, imagem, enquadramento fixo, girar com espelho
  // cartão do clipe: o cursor vai ao meio do primeiro quadro (como na tela)
  selecionarClipe(c1.id);
  expect(st().cursor).toEqual({ modo: "clipe", clipe: c1.id, t: (2 * 30720 + 512) / 2 / 15360 });
  mudar("Velocidade", noClipe<ClipeTrecho>(c1.id, (x) => void (x.velocidade = { tipo: "constante", fator: 0.5 })));
  mudar("Volume", noClipe<ClipeTrecho>(c1.id, (x) => void (x.audio.volume = 0.8)));
  const seta = adicionarEfeito(c1.id, "destaque", "Destaque")!;
  mudar("Forma", noEfeito<EfDestaque>(seta, (x) => void (x.forma = "seta")));
  mudar("Rótulo", noEfeito<EfDestaque>(seta, (x) => void (x.rotulo = { texto: "Suspeito", estilo: { ...estiloPadrao(TAMANHO_TEXTO.p), cor: x.traco.cor }, lado: "cima" })));
  mudar("Rótulo", noEfeito<EfDestaque>(seta, (x) => void (x.rotulo && (x.rotulo.lado = "baixo"))));
  const rSeta = st().doc!.rastreios.at(-1)!.id;
  marcarAncora(rSeta, { t: 2.0, pts: 60 * 512, v: { x: 0.5, y: 0.5, w: 0, h: 0 } }, "Marcar");
  marcarAncora(rSeta, { t: 3.0, pts: 90 * 512, v: { x: 0.6, y: 0.45, w: 0, h: 0 } }, "Marcar");
  const colchetes = adicionarEfeito(c1.id, "destaque", "Destaque")!;
  mudar("Forma", noEfeito<EfDestaque>(colchetes, (x) => void (x.forma = "colchetes")));
  mudar("Margem", noEfeito<EfDestaque>(colchetes, (x) => void (x.margem = 0.1)));
  marcarAncora(st().doc!.rastreios.at(-1)!.id, { t: 2.0, pts: 60 * 512, v: { x: 0.2, y: 0.2, w: 0.2, h: 0.3 } }, "Marcar");
  const ocM = adicionarEfeito(c1.id, "ocultar", "Ocultar")!;
  mudar("Forma", noEfeito<EfOcultar>(ocM, (x) => void (x.alvo.forma = "elipse")));
  mudar("Borda suave", noEfeito<EfOcultar>(ocM, (x) => void (x.alvo.borda_suave = 0.1)));
  mudar("Conteúdo", noEfeito<EfOcultar>(ocM, (x) => void ((x.conteudo = "placa"), (x.motivo = "Placa de terceiro"))));
  marcarAncora(st().doc!.rastreios.at(-1)!.id, { t: 2.0, pts: 60 * 512, v: { x: 0.1, y: 0.6, w: 0.2, h: 0.15 } }, "Marcar");
  const texto = adicionarEfeito(c1.id, "texto", "Texto")!;
  mudar("Texto", noEfeito<EfTexto>(texto, (x) => void (x.conteudo = "Veículo branco")));
  mudar("Fonte", noEfeito<EfTexto>(texto, (x) => void ((x.estilo.fonte = "serifa"), (x.estilo.italico = true), (x.estilo.caixa = { cor: "#0000008c", margem: 0.01, raio: 0.005 }))));
  for (const mostra of ["tempo_fonte", "quadro", "tempo_programa"] as const) {
    const k = adicionarEfeito(c1.id, "carimbo", "Carimbo")!;
    mudar("Mostrar", noEfeito<EfCarimbo>(k, (x) => void (x.mostra = mostra)));
    if (mostra === "tempo_programa") mudar("Formato", noEfeito<EfCarimbo>(k, (x) => void ((x.formato = "hms"), (x.lugar = { x: 0.98, y: 0.97, h: "dir", v: "base" }))));
  }
  // "Imagem" sobre o vídeo: a mídia vem do comando de importar
  const img = await cmd.edicaoMidiaImportar(WS, st().doc!.id, "/tmp/brasao.png", "imagem");
  adicionarImagem(c1.id, img as MidiaRef, "Imagem");
  marcarRegiao(c1.id, "inicio", 2.0, null, { x: 0.1, y: 0.1, w: 0.8, h: 0.8 }, "Marcar início");
  mudarGiro(c1.id, { graus: 90 }, "Girar");
  mudarGiro(c1.id, { espelhar_h: true }, "Espelhar");

  // trecho 2: entrada Escurecer, sem som, ocultação só neste clipe em mosaico, enquadramento animado em cobrir
  selecionarClipe(c2.id);
  mudar("Transição", transicaoEm(c2.id, "escurecer"));
  mudar("Duração (s)", noClipe<ClipeTrecho>(c2.id, (x) => void (x.transicao_entrada && (x.transicao_entrada.duracao_s = 0.4))));
  mudar("Som", noClipe<ClipeTrecho>(c2.id, (x) => void (x.audio.mudo = true)));
  const ocC = adicionarEfeito(c2.id, "ocultar", "Ocultar")!;
  soNesteClipe(ocC, c2.id, true, "Só neste clipe");
  mudar("Modo", noEfeito<EfOcultar>(ocC, (x) => void ((x.modo = "mosaico"), (x.intensidade = 0.5))));
  marcarRegiao(c2.id, "inicio", 1.0, null, { x: 0, y: 0, w: 1, h: 1 }, "Marcar início");
  marcarRegiao(c2.id, "fim", 4.5, 135 * 512, { x: 0.25, y: 0.25, w: 0.5, h: 0.5 }, "Marcar fim");
  mudar("Ajuste", noClipe<ClipeTrecho>(c2.id, (x) => {
    const e = x.efeitos.find((y) => (y as { tipo: string }).tipo === "enquadramento") as { ajuste: string } | undefined;
    if (e) e.ajuste = "cobrir";
  }));

  // cartela de título no começo (diálogo "Título"), e imagem como clipe no fim (Sequência → Imagem)
  inserir("Título", () => [novoClipeTitulo(novoTitulo("cartela", "Ocorrência 123/2026", "Perícia de vídeo"), 2)]);
  const tit = st().doc!.principal.find((c) => (c as { tipo: string }).tipo === "titulo") as ClipeTitulo;
  moverPara(st().doc!.principal.indexOf(tit), 0);
  mudar("Rodapé", noClipe<ClipeTitulo>(tit.id, (x) => escreverBloco(x.titulo, "rodape", "Laudo 77/2026")));
  mudar("Fundo", noClipe<ClipeTitulo>(tit.id, (x) => void (x.titulo.fundo = { tipo: "cor", cor: "#1b2a41" })));
  let imgId = "";
  st().alterar("Imagem", (d) => void (imgId = garantirMidia(d, img as MidiaRef)));
  st().selecionar({ itens: [], foco: null, efeito: null });
  st().posicionar({ modo: "programa", q: 0 });
  inserir("Imagem", () => [novoClipeImagem(imgId, 1)]);
  const clipeImg = st().doc!.principal.find((c) => (c as { tipo: string }).tipo === "imagem") as ClipeImagem;
  const txImg = adicionarEfeito(clipeImg.id, "texto", "Texto")!;
  mudar("Texto", noEfeito<EfTexto>(txImg, (x) => void (x.conteudo = "Brasão")));

  // presos ao clipe (Sequência): título de evento, terço inferior e imagem
  st().alterar("Título", (d) => void prenderTitulo(d, c1.id, "evento"));
  st().alterar("Terço inferior", (d) => void prenderTitulo(d, c2.id, "terco_inferior"));
  const [ev, terco] = st().doc!.sobreposicoes[0]!.clipes as SobTitulo[];
  mudar("Título", noClipe<SobTitulo>(ev!.id, (x) => escreverBloco(x.titulo, "titulo", "Chegada do veículo")));
  mudar("Título", noClipe<SobTitulo>(terco!.id, (x) => escreverBloco(x.titulo, "titulo", "Câmera 2")));
  mudar("Até o fim do clipe", noClipe<SobTitulo>(ev!.id, (x) => void (x.duracao_s = 1.5)));
  st().alterar("Imagem", (d) => void prenderImagem(d, tit.id, imgId));
  const sobImg = st().doc!.sobreposicoes[0]!.clipes.at(-1) as SobImagem;
  mudar("Tamanho", noClipe<SobImagem>(sobImg.id, (x) => void (x.transformacao.escala = 0.2)));

  // narração pelo Inspetor (aba Áudio → Importar…), presa ao trecho 1, com recorte, volume e rampas
  const narr = await cmd.edicaoMidiaImportar(WS, st().doc!.id, "/tmp/narracao.wav", "audio");
  const ca = adicionarNarracao(narr as MidiaRef, c1.id, "Importar…");
  const noCa = (f: (c: ClipeAudio) => void) => (d: ProjetoEdicao) => d.audios.forEach((t) => t.clipes.forEach((c) => c.id === ca && f(c)));
  mudar("De", noCa((c) => void (c.de_s = 0.5)));
  mudar("Até", noCa((c) => void (c.ate_s = 1.5)));
  mudar("Entrada (s)", noCa((c) => void (c.entrada_s = 0.1)));
  mudar("Saída (s)", noCa((c) => void (c.saida_s = 0.1)));
  mudar("Narração", (d) => void (d.audios[0]!.volume = 0.9));
  mudar("Original", (d) => void (d.mixagem.volume_original = 0.7));

  // diálogo Exportar: "Arquivo reprodutível"
  const exp = (rotulo: string, f: (e: Exportacao) => void) => st().alterar(rotulo, (d) => f(d.saida.exportacao));
  exp("Arquivo reprodutível", (x) => void (x.codificador = "cpu"));

  await st().descarregar();
  documento = cmd.edicaoSalvar.mock.calls.at(-1)?.[1] as ProjetoEdicao;

  // projeto à parte: a narração presa pela Sequência
  await st().fechar();
  await st().novo(WS);
  // a fixture (lida pelo Rust) é de projeto sem normalização: desligada como na tela (Áudio → "Normalizar")
  st().alterar("Normalizar", (d) => void (d.mixagem.normalizar = null));
  st().alterar("Narração", (d) => {
    const id = garantirMidia(d, { ...NARRACAO });
    prenderNarracao(d, "c", id, "narracao");
  });
  trilhaDaSequencia = st().doc!.audios[0] ?? null;
});

afterAll(async () => {
  await st().fechar({ descartar: true }).catch(() => {});
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("projeto da Fase 0 montado pelas portas da tela", () => {
  it("usa todo recurso da Fase 0", () => {
    const d = documento!;
    expect(d.midias.map((m) => [m.id, m.tipo, m.origem])).toEqual([
      ["m1", "video", "caso"],
      ["m2", "video", "caso"],
      ["m3", "imagem", "projeto"],
      ["m4", "audio", "projeto"],
    ]);
    expect(d.principal.map((c) => (c as { tipo: string }).tipo)).toEqual(["titulo", "trecho", "congelar", "trecho", "trecho", "imagem"]);
    const tipos = new Set(d.principal.flatMap((c) => (c as { efeitos: { tipo: string }[] }).efeitos.map((e) => e.tipo)));
    for (const t of ["destaque", "ocultar", "texto", "carimbo", "imagem", "enquadramento", "girar"]) expect(tipos, t).toContain(t);
    expect(d.midias[0]!.ocultacoes.length).toBeGreaterThanOrEqual(2);
    expect(d.sobreposicoes[0]!.clipes.map((c) => (c as { tipo: string }).tipo)).toEqual(["titulo", "titulo", "imagem"]);
    expect(d.audios[0]!.clipes).toHaveLength(1);
    expect(d.saida).toMatchObject({ auto: false, largura: 640, altura: 360, exportacao: { codificador: "cpu" } });
    expect(trilhaDaSequencia).not.toBeNull();
  });

  it("grava (SICRO_GRAVAR_FIXTURES=1) ou confere a fixture byte a byte", () => {
    const texto = `${JSON.stringify(documento, null, 2)}\n`;
    const caminho = fileURLToPath(new URL(`../../../../src-tauri/tests/fixtures/edicao/${ARQUIVO}`, import.meta.url));
    if (process.env.SICRO_GRAVAR_FIXTURES === "1") writeFileSync(caminho, texto);
    expect(readFileSync(caminho, "utf8")).toBe(texto);
  });

  // DEFEITO entre pacotes (relatado ao integrador): a Sequência cria a trilha de narração com `abaixa_original`
  // ligado; a Fase 0 recusa "abaixar" (`nao_suportado`, validar.rs e motor/audio.rs) e a tela não tem como
  // desligar. `it.fails` passa enquanto o defeito existir; corrigido o novos.ts, trocar por `it`.
  // DEFEITO entre pacotes (relatado ao integrador): "Adicionar" põe a âncora padrão no tempo do cursor (meio do
  // quadro, sem pts) e "Marcar" no mesmo quadro grava o par do palco (início do quadro, com pts): `porChave` não troca
  // uma pela outra e o rastreio segura a caixa padrão do quadro seguinte em diante (edicao::contratos_verif no Rust).
  it("marcar no quadro do cursor troca a âncora padrão (uma âncora por quadro)", () => {
    const r = documento!.rastreios.filter((x) => x.ancoras.some((k) => k.pts === 60 * 512));
    expect(r.length).toBe(3);
    for (const x of r) expect(x.ancoras.filter((k) => k.t < 2 + 1 / 30)).toHaveLength(1);
  });

  it("narração presa pela Sequência nasce sem abaixar o original (Fase 0 recusa abaixar)", () => {
    expect(trilhaDaSequencia?.abaixa_original).toBe(false);
  });
});
