/**
 * O que a tela pede ao backend e guarda: `MidiaInfo` e tabela de quadros por mídia (refeitas quando a fila de fundo
 * termina a tabela do decodificador, a cópia ou o proxy dela), sprites de texto, atlas do carimbo e imagens.
 * `versao` sobe a cada chegada: o palco repinta. Pedido que falha não repete sozinho (só com `renovar`).
 */

import { create } from "zustand";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands } from "@core/commands";
import type { AtlasCarimbo, EstiloTexto, MidiaInfo, PedidoSprite, SpriteTexto, TabelaQuadros, TarefaFundo } from "@domain/edicao";
import { separarErro, type ErroComando } from "../store/edicaoStore";
import { useFundoStore } from "../store/fundoStore";

export interface EstadoRecurso {
  info: MidiaInfo | null;
  tabela: TabelaQuadros | null;
  erro: ErroComando | null;
  pedindo: boolean;
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
    }
  });
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
