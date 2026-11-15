/**
 * O que a tela pede ao backend e guarda: `MidiaInfo` e tabela de quadros por mídia (refeitas quando a fila de fundo
 * termina a tabela do decodificador, a cópia ou o proxy dela), sprites de texto, atlas do carimbo e imagens.
 * `versao` sobe a cada chegada: o palco repinta. Pedido que falha não repete sozinho (só com `renovar`).
 */

import { create } from "zustand";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands } from "@core/commands";
import type { AtlasCarimbo, ClipePrincipal, EstiloTexto, Id, MidiaInfo, PedidoSprite, PreviaMidia, ProjetoEdicao, SpriteTexto, TabelaQuadros, TarefaFundo } from "@domain/edicao";
import { separarErro, useEdicaoStore, type ErroComando } from "../store/edicaoStore";
import { useFundoStore } from "../store/fundoStore";

export interface EstadoRecurso {
  info: MidiaInfo | null;
  tabela: TabelaQuadros | null;
  erro: ErroComando | null;
  pedindo: boolean;
  // CONTRATO: acréscimos (contratos-fase1 F5): `busca_p50_ms`, `pedida`, `erroDoProxy`.
  /** Busca pausada medida pelo palco (p50 de 3, ms; §7.6); ausente = ainda não mediu nesta sessão. */
  busca_p50_ms?: number | null;
  /** O que `edicao_proxy_pedir` respondeu (o proxy quando pronto; senão o que já toca); ausente = não pedido. */
  pedida?: PreviaMidia | null;
  /** O pedido do proxy falhou (ícone no palco). */
  erroDoProxy?: ErroComando | null;
}

interface RecursosState {
  /** Por SHA-256 da mídia. */
  midias: Record<string, EstadoRecurso>;
  versao: number;
}

export const useRecursos = create<RecursosState>(() => ({ midias: {}, versao: 0 }));

const VAZIO: EstadoRecurso = Object.freeze({ info: null, tabela: null, erro: null, pedindo: false }) as EstadoRecurso;
let wsAtual: string | null = null;

const subir = () => useRecursos.setState((s) => ({ versao: s.versao + 1 }));

function definir(sha: string, m: Partial<EstadoRecurso>) {
  useRecursos.setState((s) => ({ midias: { ...s.midias, [sha]: { ...(s.midias[sha] ?? VAZIO), ...m } }, versao: s.versao + 1 }));
}

/** Troca de caso: tudo que veio do caso anterior sai. */
function caso(ws: string) {
  if (wsAtual === ws) return;
  wsAtual = ws;
  sprites.clear();
  atlas.clear();
  proxies.clear();
  useRecursos.setState({ midias: {}, versao: useRecursos.getState().versao + 1 });
}

async function buscar(ws: string, sha: string) {
  definir(sha, { pedindo: true });
  try {
    const [info, tabela] = await Promise.all([commands.edicaoMidiaInfo(ws, sha), commands.edicaoTabelaQuadros(ws, sha)]);
    if (wsAtual === ws) definir(sha, { info, tabela, erro: null, pedindo: false });
  } catch (e) {
    if (wsAtual === ws) definir(sha, { erro: separarErro(e), pedindo: false });
  }
}

/** Pede info e tabela da mídia uma vez (a não ser que `renovar`). */
export function garantirMidia(ws: string, sha: string): void {
  caso(ws);
  ouvirFundo();
  const m = useRecursos.getState().midias[sha];
  if (m && (m.pedindo || m.info || m.erro)) return;
  void buscar(ws, sha);
}

export function renovar(ws: string, sha: string): void {
  caso(ws);
  void buscar(ws, sha);
}

export const recursoDe = (sha: string | null | undefined): EstadoRecurso => (sha ? (useRecursos.getState().midias[sha] ?? VAZIO) : VAZIO);

/** Tipos de fundo que mudam o que a tela sabe da mídia. */
const MUDAM = new Set<TarefaFundo["tipo"]>(["tabela", "copia_previa", "proxy"]);
let ouvindoFundo = false;
const prontas = new Set<string>();

/** Quando a fila de fundo termina algo de uma mídia já pedida, pede de novo. */
function ouvirFundo() {
  if (ouvindoFundo) return;
  ouvindoFundo = true;
  useFundoStore.subscribe((s) => {
    for (const t of s.tarefas) {
      if (!MUDAM.has(t.tipo) || t.estado.tipo !== "pronta" || prontas.has(t.id)) continue;
      prontas.add(t.id);
      const sha = t.chave.slice(t.chave.indexOf(":") + 1);
      if (wsAtual && useRecursos.getState().midias[sha]) void buscar(wsAtual, sha);
      // o proxy que o palco pediu ficou pronto: a resposta nova traz o arquivo
      if (t.tipo === "proxy") void proxies.get(sha)?.();
    }
  });
}

// ---- proxy pedido pelo palco (§7.6) ----

/** Busca pausada medida (p50 de 3, ms) acima disso pede proxy: a de `proxy::BUSCA_P50_MAX_MS`. */
export const BUSCA_P50_MAX_MS = 120;
/** Clipe abaixo desta velocidade toca pelo proxy: a de `proxy::VELOCIDADE_MIN`. */
export const VELOCIDADE_MIN = 0.25;

/** Mediana das buscas medidas; null sem medida. */
export function p50(tempos: readonly number[]): number | null {
  if (tempos.length === 0) return null;
  const o = [...tempos].sort((a, b) => a - b);
  const m = o.length >> 1;
  return o.length % 2 ? (o[m] as number) : ((o[m - 1] as number) + (o[m] as number)) / 2;
}

/** O clipe pede proxy por si (§7.6): rampa, ré ou abaixo de 0,25×. */
export function clipePedeProxy(c: Pick<ClipePrincipal, "tipo"> & { velocidade?: { tipo: string; fator?: number } }): boolean {
  if (c.tipo !== "trecho" || !c.velocidade) return false;
  return c.velocidade.tipo !== "constante" || !((c.velocidade.fator as number) >= VELOCIDADE_MIN);
}

/** O palco pede o proxy? Busca medida lenta, ou o clipe pede. */
export function precisaDeProxy(c: Parameters<typeof clipePedeProxy>[0], buscaP50Ms: number | null | undefined): boolean {
  return clipePedeProxy(c) || (buscaP50Ms != null && buscaP50Ms > BUSCA_P50_MAX_MS);
}

/** Guarda a busca medida da mídia (uma vez por sessão; null = não deu para medir). */
export function registrarBusca(ws: string, sha: string, ms: number | null): void {
  if (wsAtual !== ws || !useRecursos.getState().midias[sha]) return;
  definir(sha, { busca_p50_ms: ms });
}

/** Mídias com proxy pedido nesta sessão → como pedir de novo (quando a fila de fundo termina). */
const proxies = new Map<string, () => Promise<void>>();

/** Trecho da principal que usa a mídia: o `clipe` se ainda está no projeto; senão outro da mesma mídia. */
function trechoDaMidia(doc: ProjetoEdicao, clipe: Id, sha: string): Id | null {
  const midias = new Set(doc.midias.filter((m) => m.sha256 === sha).map((m) => m.id));
  const trechos = (doc.principal as { id?: Id; tipo?: string; midia?: Id }[]).filter((c) => c.tipo === "trecho" && !!c.id && !!c.midia && midias.has(c.midia));
  return (trechos.find((c) => c.id === clipe) ?? trechos[0])?.id ?? null;
}

/** `edicao_proxy_pedir`, a chamada isolada: pede o proxy da mídia do clipe e guarda a resposta em `pedida` (o proxy
 *  quando pronto; senão o que já toca). Pronto o proxy na fila de fundo, pede de novo sozinho. Falha: `erroDoProxy`.
 *  O clipe que pediu pode sair do projeto enquanto o proxy é feito: o pedido seguinte vai por outro trecho da mesma
 *  mídia; sem nenhum, a mídia volta a "não pedido". */
export function pedirProxy(ws: string, clipe: Id, sha: string, buscaP50Ms: number | null): Promise<void> {
  caso(ws);
  ouvirFundo();
  const pedir = async () => {
    const doc = useEdicaoStore.getState().doc;
    if (!doc || wsAtual !== ws) return;
    const id = trechoDaMidia(doc, clipe, sha);
    if (id === null) {
      if (proxies.get(sha) === pedir) proxies.delete(sha);
      return;
    }
    try {
      const pedida = await commands.edicaoProxyPedir(ws, doc, id, buscaP50Ms);
      if (wsAtual === ws) definir(sha, { pedida, erroDoProxy: null });
    } catch (e) {
      if (wsAtual === ws) definir(sha, { erroDoProxy: separarErro(e) });
    }
  };
  proxies.set(sha, pedir);
  return pedir();
}

/** Proxy já pedido para a mídia nesta sessão? */
export const proxyPedido = (sha: string): boolean => proxies.has(sha);

/** A prévia da mídia que o palco toca: o proxy pedido quando pronto; senão a da `MidiaInfo`. */
export function previaDaMidia(r: EstadoRecurso): PreviaMidia | null {
  return r.pedida?.pronta && r.pedida.tipo === "proxy" ? r.pedida : (r.info?.previa ?? null);
}

/** Quadros das buscas medidas: três GOPs espalhados na mídia (20, 50 e 80 %), no meio de cada um (a busca aleatória
 *  cai em média a meio GOP do quadro-chave), nunca o quadro que está na tela. */
export function alvosDaMedida(tab: Pick<TabelaQuadros, "passos" | "chaves">, atual: number | null): number[] {
  const n = tab.passos.length + 1;
  if (n < 4) return [];
  const chaves = tab.chaves.filter((c) => c >= 0 && c < n);
  const alvos: number[] = [];
  for (const f of [0.2, 0.5, 0.8]) {
    let j = Math.floor(n * f);
    if (chaves.length > 1) {
      let a = 0;
      for (const c of chaves) if (c <= j) a = c;
      const b = chaves.find((c) => c > j) ?? n;
      j = a + Math.floor((b - a) / 2);
    }
    j = Math.min(Math.max(j, 0), n - 1);
    if (j === atual || alvos.includes(j)) j = j + 1 < n ? j + 1 : j - 1;
    if (j !== atual && !alvos.includes(j)) alvos.push(j);
  }
  return alvos;
}

// ---- sprites e atlas: pedidos ao backend, guardados pelo pedido ----

type Pedido<T> = { valor: T | null; falhou: boolean };
const sprites = new Map<string, Pedido<SpriteTexto>>();
const atlas = new Map<string, Pedido<AtlasCarimbo>>();

function pedirUma<T>(mapa: Map<string, Pedido<T>>, chave: string, f: () => Promise<T>): T | null {
  const p = mapa.get(chave);
  if (p) return p.valor;
  const novo: Pedido<T> = { valor: null, falhou: false };
  mapa.set(chave, novo);
  f().then(
    (v) => {
      novo.valor = v;
      subir();
    },
    () => {
      novo.falhou = true;
    },
  );
  return null;
}

export function spriteDe(ws: string, p: PedidoSprite): SpriteTexto | null {
  caso(ws);
  return pedirUma(sprites, JSON.stringify(p), () => commands.edicaoSpriteTexto(ws, p));
}

export function atlasDe(ws: string, e: EstiloTexto, alturaCanvas: number): AtlasCarimbo | null {
  caso(ws);
  return pedirUma(atlas, JSON.stringify([e, alturaCanvas]), () => commands.edicaoAtlasCarimbo(ws, e, alturaCanvas));
}

// ---- imagens (sprites, atlas e imagens do projeto) ----

const imagens = new Map<string, { img: HTMLImageElement; pronta: boolean }>();

/** Imagem já carregada desse endereço; começa a carregar na primeira vez. */
export function imagemDe(url: string): HTMLImageElement | null {
  let e = imagens.get(url);
  if (!e) {
    const img = new Image();
    const novo = { img, pronta: false };
    e = novo;
    imagens.set(url, novo);
    img.onload = () => {
      novo.pronta = true;
      subir();
    };
    img.src = url;
  }
  return e.pronta ? e.img : null;
}

/** Arquivo do cache da Edição (`relativo` de resposta do backend): raiz vinda de `ProjetoAberto.raiz_cache`. */
export function urlDoCache(raizCache: string, relativo: string): string {
  return convertFileSrc(`${raizCache.replace(/[\\/]+$/, "")}/${relativo}`);
}
