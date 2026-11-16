/**
 * Fixture de ponta a ponta `projeto-do-store.json`: um projeto montado SÓ pelas portas que a tela usa (store, coleta
 * e as ações do Inspetor), gravado como o salvamento automático o manda ao `edicao_salvar`. O teste do Rust
 * (`armazem::testes::projeto_do_store_le_tipado`) lê o mesmo arquivo; o de ponta a ponta da leva de fecho troca as
 * mídias de mentira (m1, m2, m3; SHA-256 "1"×64…) pelas da matriz.
 * `SICRO_GRAVAR_FIXTURES=1` regrava; sem ela, o documento tem de sair igual ao arquivo, byte a byte.
 *
 * Também confere `completar()` (contratos (g) 12): todo campo que o Rust lê com `#[serde(default)]` volta com o
 * padrão do Rust quando falta, em `projeto-minimo.json`, `contrato-todas-variantes.json` e nesta fixture.
 */

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import type { Caixa, ClipeTitulo, ClipeTrecho, EfOcultar, EfTexto, MidiaInfo, MidiaRef, ProjetoEdicao, QuadroEm } from "@domain/edicao";
import type { VideoMedia } from "@domain/video";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoExcluir: vi.fn(),
  listVideoMedia: vi.fn(),
  videoProveniencia: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoQuadroEm: vi.fn(),
  edicaoMidiaImportar: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { useEdicaoStore } from "./edicaoStore";
import { receber } from "./coleta";
import { completar, novoId } from "../modelo/projeto";
import { PASTA_FIXTURES, ler, lerTexto } from "../modelo/testeFixtures";
import {
  adicionarEfeito,
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
import { novoTitulo } from "../inspetor/novos";
import { comConteudo } from "../inspetor/secoes/ocultarPadrao";
import { escreverBloco } from "../inspetor/secoes/TituloSecao";
import { useInspetorUi } from "../inspetor/ui";

const WS = "/casos/c-fixture";
const ARQUIVO = "projeto-do-store.json";
const st = () => useEdicaoStore.getState();
const sha = (c: string) => c.repeat(64);

// ---- ids previsíveis: UUID v4 por contador; as mídias pedem os seus pela fila ----
const fila: string[] = [];
let n = 0;
const uuid = (i: number) => `00000000-0000-4000-8000-${String(i).padStart(12, "0")}`;

function video(id: string, c: string, nome: string, dur: number, inicio: number): VideoMedia {
  return {
    id,
    occurrence_id: "c-fixture",
    original_path: null,
    relative_path: `Mídias/Vídeos/${nome}`,
    filename: nome,
    sha256: sha(c),
    size_bytes: 1,
    duration_s: dur,
    codec: "h264",
    width: 1920,
    height: 1080,
    pixel_format: "yuv420p",
    fps_declared: 30,
    avg_frame_rate: "30/1",
    r_frame_rate: "30/1",
    time_base: "1/15360",
    frame_count: Math.round(dur * 30),
    bitrate: null,
    raw_probe_json: JSON.stringify({ format: { start_time: String(inicio) }, streams: [{ codec_type: "video" }, { codec_type: "audio" }] }),
    warnings_json: "[]",
    created_at: "2026-11-14T00:00:00Z",
    updated_at: "2026-11-14T00:00:00Z",
  };
}

const info = (c: string, de: number, ate: number): MidiaInfo => ({
  id: c,
  sha256: sha(c),
  nome: "",
  exibido: { w: 1920, h: 1080 },
  taxa: { num: 30, den: 1 },
  vfr: false,
  passo: { min: 1 / 30, max: 1 / 30 },
  de_s: de,
  ate_s: ate,
  n: Math.round((ate - de) * 30),
  tem_audio: true,
  previa: { relativo: "", tipo: "original", desloc_s: 0, de_s: de, ate_s: ate, largura: 1920, altura: 1080, pronta: true },
  tabela: "decodificador",
  erros: 0,
  marcavel: true,
  cor: { matriz: "bt709", faixa: "tv" },
});

/** O que o Rust devolveria em base 1/15360 a 30 qps: quadro exibido em t. */
const quadroEm = (_ws: string, _sha: string, alvo: { t: number }): QuadroEm => {
  const j = Math.floor(alvo.t * 30 + 1e-9);
  return { indice: j, pts: j * 512, pts_s: j / 30, dur_s: 1 / 30 };
};

const NARRACAO: MidiaRef = {
  id: "m3",
  tipo: "audio",
  origem: "projeto",
  media_id: null,
  sha256: sha("3"),
  nome: "narracao.wav",
  relativo: "Sistema/videos/edicao/00000000-0000-4000-8000-000000000001/midias/" + sha("3") + ".wav",
  duracao_s: 12.5,
  inicio_s: null,
  largura: null,
  altura: null,
  taxa: null,
  vfr: false,
  tem_audio: true,
  cor: null,
  derivacao: null,
  recebido: { nativo: null, obs: "" },
  relogio_desvio_s: null,
  gravador: null,
  decodificacao: null,
  ocultacoes: [],
};

let documento: ProjetoEdicao | null = null;

beforeAll(async () => {
  vi.useFakeTimers({ now: new Date("2026-11-14T12:00:00.000Z") });
  vi.spyOn(globalThis.crypto, "randomUUID").mockImplementation(() => (fila.shift() ?? uuid(++n)) as `${string}-${string}-${string}-${string}-${string}`);
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoAbrir.mockResolvedValue({ projeto: null, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/edicao/c-fixture" });
  // proveniência da análise: nada gravado (a coleta e o "Trecho…" leem sempre)
  cmd.videoProveniencia.mockResolvedValue({ recebido_nativo: null, recebido_obs: "", relogio_desvio_s: null, gravador: null });
  cmd.listVideoMedia.mockResolvedValue([video("vm-1", "1", "cam1.mp4", 60, 0), video("vm-2", "2", "cam2.mp4", 40, 0)]);
  cmd.edicaoMidiaInfo.mockImplementation(async (_ws: string, s: string) => (s === sha("1") ? info("vm-1", 0, 60) : info("vm-2", 0, 40)));
  cmd.edicaoQuadroEm.mockImplementation(async (ws: string, s: string, alvo: { t: number }) => quadroEm(ws, s, alvo));
  cmd.edicaoMidiaImportar.mockResolvedValue(NARRACAO);

  // projeto novo (botão "Nova edição")
  await st().novo(WS);
  // a fixture (lida pelo Rust) é de projeto sem normalização: desligada como na tela (Áudio → "Normalizar")
  st().alterar("Normalizar", (d) => void (d.mixagem.normalizar = null));
  // dois trechos de mídias diferentes pela coleta (Ctrl+3), com a tela aberta na corrente
  fila.push("m1", "m2");
  const r = await receber(WS, [
    { sha256: sha("1"), inicio_s: 12, fim_s: 15.5, rotulo: "Entrada" },
    { sha256: sha("2"), inicio_s: 3, fim_s: 8 },
  ]);
  expect(r.n).toBe(2);
  const [c1, c2] = st().doc!.principal as ClipeTrecho[];
  if (!c1 || !c2) throw new Error("trechos");

  // cartela de título no começo ("Título…"), com título e rodapé digitados
  const titulo = novoTitulo(3);
  mudar("Título…", (d) => void d.principal.unshift(titulo));
  for (const [papel, texto] of [["titulo", "Ocorrência 123/2026"], ["rodape", "Perícia de vídeo"]] as const) {
    const g = st().iniciarGesto(papel === "titulo" ? "Título" : "Rodapé");
    for (let i = 1; i <= texto.length; i++) g.alterar(noClipe<ClipeTitulo>(titulo.id, (x) => escreverBloco(x.titulo, papel, texto.slice(0, i))));
    g.concluir();
  }

  // destaque de uma âncora no trecho 1, marcada no palco sobre o quadro do início
  const destaque = adicionarEfeito(c1.id, "destaque", "Destaque")!;
  const rastreio = st().doc!.rastreios.at(-1)!.id;
  useInspetorUi.getState().pedir({ tipo: "caixa", rastreio, efeito: destaque }, (v, t, pts) => marcarAncora(rastreio, { t, pts, v: v as Caixa }, "Marcar"));
  useInspetorUi.getState().marcado({ x: 0.3, y: 0.25, w: 0.12, h: 0.2 }, 12, 12 * 30 * 512);

  // texto no trecho 2
  const texto = adicionarEfeito(c2.id, "texto", "Texto")!;
  const g = st().iniciarGesto("Texto");
  g.alterar(noEfeito<EfTexto>(texto, (x) => void (x.conteudo = "Veículo branco")));
  g.concluir();

  // ocultar da mídia do trecho 1: nasce no padrão de rosto (desfoque 1,0, B27) e "Conteúdo" placa o leva ao sólido;
  // desfoque 0,6 só no trecho 2
  const daMidia = adicionarEfeito(c1.id, "ocultar", "Ocultar")!;
  expect(st().doc!.midias[0]!.ocultacoes.at(-1)).toMatchObject({ conteudo: "rosto", modo: "desfoque", intensidade: 1 });
  mudar("Conteúdo", noEfeito<EfOcultar>(daMidia, (x) => comConteudo(x, "placa")));
  const desfoque = adicionarEfeito(c2.id, "ocultar", "Ocultar")!;
  soNesteClipe(desfoque, c2.id, true, "Só neste clipe");
  mudar("Modo", noEfeito<EfOcultar>(desfoque, (x) => void (x.modo = "desfoque")));
  mudar("Intensidade", noEfeito<EfOcultar>(desfoque, (x) => void (x.intensidade = 0.6)));
  mudar("Conteúdo", noEfeito<EfOcultar>(desfoque, (x) => void (x.conteudo = "placa")));

  // enquadramento fixo no trecho 1, animado no trecho 2; girar no trecho 1
  marcarRegiao(c1.id, "inicio", 12, null, { x: 0.25, y: 0.25, w: 0.5, h: 0.5 }, "Marcar início");
  marcarRegiao(c2.id, "inicio", 3, null, { x: 0, y: 0, w: 1, h: 1 }, "Marcar início");
  marcarRegiao(c2.id, "fim", 7.5, null, { x: 0.4, y: 0.3, w: 0.4, h: 0.4 }, "Marcar fim");
  mudarGiro(c1.id, { graus: 90 }, "Girar");

  // transição Escurecer na entrada do trecho 2
  mudar("Transição", transicaoEm(c2.id, "escurecer"));

  // narração importada, presa ao trecho 1
  adicionarNarracao(NARRACAO, c1.id, "Importar…");

  await st().descarregar();
  documento = cmd.edicaoSalvar.mock.calls.at(-1)?.[1] as ProjetoEdicao;
});

afterAll(async () => {
  await st().fechar().catch(() => {});
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("projeto montado pelas portas da tela", () => {
  it("tem tudo o que a fixture promete", () => {
    const d = documento!;
    expect(d.midias.map((m) => [m.id, m.sha256])).toEqual([
      ["m1", sha("1")],
      ["m2", sha("2")],
      ["m3", sha("3")],
    ]);
    expect(d.principal.map((c) => (c as { tipo: string }).tipo)).toEqual(["titulo", "trecho", "trecho"]);
    const [t, c1, c2] = d.principal as [ClipeTitulo, ClipeTrecho, ClipeTrecho];
    expect(t.titulo.blocos.map((b) => [b.papel, b.conteudo])).toEqual([
      ["titulo", "Ocorrência 123/2026"],
      ["rodape", "Perícia de vídeo"],
    ]);
    expect(c1.efeitos.map((e) => (e as { tipo: string }).tipo).sort()).toEqual(["destaque", "enquadramento", "girar"]);
    expect(c2.efeitos.map((e) => (e as { tipo: string }).tipo).sort()).toEqual(["enquadramento", "ocultar", "texto"]);
    const enq = (c: ClipeTrecho) => c.efeitos.find((e) => (e as { tipo: string }).tipo === "enquadramento") as { regiao: unknown };
    expect(Array.isArray(enq(c1).regiao)).toBe(false);
    expect(Array.isArray(enq(c2).regiao) && (enq(c2).regiao as unknown[]).length).toBe(2);
    expect(c2.transicao_entrada).toEqual({ tipo: "escurecer", duracao_s: 0.5, audio: "acompanhar" });
    expect(d.midias[0]!.ocultacoes).toHaveLength(1);
    expect(d.midias[0]!.ocultacoes[0]).toMatchObject({ conteudo: "placa", modo: "solido", intensidade: 1, ativo: { de: 12, ate: 15.5 } });
    expect(c2.efeitos.find((e) => (e as { tipo: string }).tipo === "ocultar")).toMatchObject({ modo: "desfoque", intensidade: 0.6 });
    const destaque = d.rastreios.find((r) => r.ancoras.length === 1 && r.midia === "m1");
    expect(destaque?.ancoras[0]).toEqual({ t: 12, pts: 12 * 30 * 512, v: { x: 0.3, y: 0.25, w: 0.12, h: 0.2 } });
    expect(d.audios).toHaveLength(1);
    expect(d.audios[0]!.clipes[0]).toMatchObject({ midia: "m3", pos: { ref: "clipe", clipe: c1.id, desloc: 0 } });
    expect(d.saida.auto).toBe(true);
    expect(d.mixagem.normalizar).toBeNull();
  });

  it("grava (SICRO_GRAVAR_FIXTURES=1) ou confere a fixture byte a byte", () => {
    const texto = `${JSON.stringify(documento, null, 2)}\n`;
    const caminho = fileURLToPath(new URL(`../../../../src-tauri/tests/fixtures/edicao/${ARQUIVO}`, import.meta.url));
    if (process.env.SICRO_GRAVAR_FIXTURES === "1") writeFileSync(caminho, texto);
    expect(readFileSync(caminho, "utf8")).toBe(texto);
  });
});

// ---- completar() × #[serde(default)] do modelo.rs (contratos (g) 12) ----

type Obj = Record<string, unknown>;
const ehObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);
const lista = (v: unknown): Obj[] => (Array.isArray(v) ? v.filter(ehObj) : []);
const efeitosDe = (c: Obj) => lista(c.efeitos);

/** Onde mora cada struct do Rust num documento (só itens de tipo conhecido; versão futura não é completada). */
const CONHECIDOS_CLIPE = new Set(["trecho", "congelar", "imagem", "titulo", "tela"]);
const CONHECIDOS_SOB = new Set(["video", "imagem", "titulo"]);
const CONHECIDOS_EFEITO = new Set(["estabilizar", "lente", "desentrelacar", "media_temporal", "ajustes", "nitidez", "girar", "enquadramento", "ocultar", "holofote", "destaque", "lupa", "regua", "velocidade_exibida", "texto", "carimbo", "imagem"]);
const principal = (d: Obj) => lista(d.principal).filter((c) => CONHECIDOS_CLIPE.has(c.tipo as string));
const sobrepostos = (d: Obj) => lista(d.sobreposicoes).flatMap((t) => lista(t.clipes)).filter((c) => CONHECIDOS_SOB.has(c.tipo as string));
const todosEfeitos = (d: Obj) =>
  [
    ...principal(d).flatMap((c) => [...efeitosDe(c), ...lista(c.celulas).flatMap(efeitosDe)]),
    ...sobrepostos(d).flatMap(efeitosDe),
    ...lista(d.midias).flatMap((m) => lista(m.ocultacoes)),
  ].filter((e) => CONHECIDOS_EFEITO.has(e.tipo as string));

const ONDE: Record<string, (d: Obj) => Obj[]> = {
  ProjetoEdicao: (d) => [d],
  Saida: (d) => (ehObj(d.saida) ? [d.saida] : []),
  MidiaRef: (d) => lista(d.midias),
  AudioClipe: (d) => [...principal(d), ...sobrepostos(d)].map((c) => c.audio).filter(ehObj),
  ClipeBase: (d) => principal(d),
  ClipeTrecho: (d) => principal(d).filter((c) => c.tipo === "trecho"),
  ClipeCongelar: (d) => principal(d).filter((c) => c.tipo === "congelar"),
  ClipeTela: (d) => principal(d).filter((c) => c.tipo === "tela"),
  EfBase: todosEfeitos,
  Titulo: (d) => [...principal(d), ...sobrepostos(d)].map((c) => c.titulo).filter(ehObj),
  TrilhaSobreposicao: (d) => lista(d.sobreposicoes),
  SobBase: sobrepostos,
  TrilhaAudio: (d) => lista(d.audios),
  TrilhaLegendas: (d) => (ehObj(d.legendas) ? [d.legendas] : []),
  Rastreio: (d) => lista(d.rastreios),
  SincroniaRef: (d) => lista(d.sincronias),
};
/** Opcionais no TS (`?`): o Rust lê a falta e regrava (M2); o front não completa. */
const SO_NO_RUST = new Set(["Chave.pts", "Chave.interp", "Intervalo.de", "Intervalo.ate"]);

/** `#[serde(default…)]` de modelo.rs: "Struct.campo" → o padrão do Rust (`None` = null; `"sim"` = true). */
function padroesDoRust(): Map<string, string> {
  const fonte = readFileSync(fileURLToPath(new URL("../../../../src-tauri/src/edicao/modelo.rs", import.meta.url)), "utf8");
  const r = new Map<string, string>();
  let struct = "";
  const linhas = fonte.split("\n");
  for (let i = 0; i < linhas.length; i++) {
    const l = linhas[i] as string;
    const s = /^pub struct (\w+)/.exec(l);
    if (s) struct = s[1] as string;
    const a = /#\[serde\((default[^)]*)\)\]/.exec(l);
    if (!a) continue;
    const campo = /pub (\w+):\s*([^,]+)/.exec(linhas[i + 1] ?? "");
    if (campo) r.set(`${struct}.${campo[1]}`, `${a[1]}|${(campo[2] as string).trim()}`);
  }
  return r;
}

/** Valor que o `Default` do Rust daria ao campo (os tipos compostos pelos `Default` de M7). */
function padraoEsperado(tipoRust: string, attr: string): unknown {
  if (attr.includes('"sim"')) return true;
  if (tipoRust.startsWith("Vec<")) return [];
  if (tipoRust.startsWith("Option<")) return null;
  if (tipoRust === "bool") return false;
  if (tipoRust === "String") return "";
  if (/^u\d+|^i\d+|^f\d+/.test(tipoRust)) return 0;
  return undefined; // struct com Default próprio (TrilhaLegendas, Mixagem, AudioClipe): confere só que existe
}

describe("completar() põe os padrões do Rust (contratos (g) 12)", () => {
  const rust = padroesDoRust();
  const alvo = [...rust.keys()].filter((k) => !SO_NO_RUST.has(k));

  it("toda struct com padrão no Rust tem onde ser achada no documento", () => {
    const semLugar = alvo.map((k) => k.split(".")[0] as string).filter((s) => !(s in ONDE));
    expect([...new Set(semLugar)]).toEqual([]);
    expect(alvo.length).toBeGreaterThan(30);
  });

  for (const nome of ["projeto-minimo.json", "contrato-todas-variantes.json", ARQUIVO]) {
    it(`${nome}: cada campo tirado volta com o padrão do Rust`, () => {
      const original = JSON.parse(lerTexto(nome)) as Obj;
      // documento completo volta o mesmo objeto
      const completo = completar(original as unknown as ProjetoEdicao) as unknown as Obj;
      expect(completar(completo as unknown as ProjetoEdicao)).toBe(completo);
      const faltas: string[] = [];
      let casos = 0;
      for (const k of alvo) {
        const [struct, campo] = k.split(".") as [string, string];
        const [attr, tipoRust] = (rust.get(k) as string).split("|") as [string, string];
        const n = ONDE[struct]!(completo).length;
        for (let i = 0; i < n; i++) {
          const copia = JSON.parse(JSON.stringify(completo)) as Obj;
          const obj = ONDE[struct]!(copia)[i] as Obj;
          if (!(campo in obj)) continue;
          delete obj[campo];
          casos++;
          const volta = ONDE[struct]!(completar(copia as unknown as ProjetoEdicao) as unknown as Obj)[i];
          const esperado = padraoEsperado(tipoRust, attr);
          if (!volta || !(campo in volta)) faltas.push(`${k}[${i}] ausente`);
          else if (esperado !== undefined && JSON.stringify(volta[campo]) !== JSON.stringify(esperado)) faltas.push(`${k}[${i}] = ${JSON.stringify(volta[campo])}, Rust ${JSON.stringify(esperado)}`);
        }
      }
      expect(faltas).toEqual([]);
      expect(casos).toBeGreaterThan(nome === "projeto-minimo.json" ? 10 : 40);
    });
  }

  it("projeto-minimo.json completado tem todos os campos que o Rust lê com padrão", () => {
    const c = completar(ler<ProjetoEdicao>("projeto-minimo.json")) as unknown as Obj;
    for (const k of alvo.filter((x) => x.startsWith("ProjetoEdicao.") || x.startsWith("Saida."))) {
      const [struct, campo] = k.split(".") as [string, string];
      expect(ONDE[struct]!(c)[0], k).toHaveProperty(campo);
    }
  });
});

void novoId;
void PASTA_FIXTURES;
