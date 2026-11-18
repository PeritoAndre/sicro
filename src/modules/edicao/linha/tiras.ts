/**
 * Tiras de miniaturas da V1 pelo comando do P4.3 (`edicao_tira`, contratos-fase4.md (g)). Regras do revisor do P4.3:
 * pede só os trechos visíveis, com espera curta entre pedidos; canal = id do clipe (zoom profundo: a faixa em blocos,
 * canal `clipe#bloco`); vale o `n` da resposta; `cancelada` → pede de novo só se a chave ainda é a última do canal;
 * `tabela_pendente` → pede de novo quando `tabela:<sha256>` fica pronta (evento `edicao:fundo`); a imagem vem de
 * `raiz_cache` + `relativo`. Sem tira (pedida, sem quadro-chave, falhou): faixa lisa. Dono: P4.1.
 */

import { useEffect, useMemo } from "react";
import { create } from "zustand";
import { commands } from "@core/commands";
import type { PedidoTira, RespostaTira, Seg, TarefaFundo, TiraMiniaturas } from "@domain/edicao";
import { separarErro } from "../store/edicaoStore";
import { useFundoStore } from "../store/fundoStore";
import { urlDoCache } from "../palco/recursos";

/** Largura máxima de um bloco (px): acima, a faixa do clipe é pedida em pedaços (zoom profundo). */
export const BLOCO_PX = 2048;
/** Espera depois da última mudança da área visível (rolagem, zoom) antes de pedir. */
export const ESPERA_MS = 160;
/** Trecho mais estreito que isto fica com a faixa lisa. */
export const LARGURA_MIN_PX = 40;
/** Teto de blocos pedidos de uma vez (zoom afastado com centenas de clipes na tela). */
export const MAX_BLOCOS = 48;
/** Tetos do P4.3 (`tiras::{MIN_ALTURA, MAX_ALTURA, MAX_LARGURA}`). */
const ALTURA_MIN = 16;
const ALTURA_MAX = 360;
const LARGURA_MAX = 16384;

/** Um pedaço da faixa de um trecho: o pedido e onde ele fica no item (px). */
export interface BlocoTira {
  canal: string;
  pedido: PedidoTira;
  /** Começo do bloco dentro do item e largura dele na tela (px). */
  x: number;
  w: number;
}

/** Largura pedida em degraus de um quarto de oitava: o zoom contínuo não pede uma tira nova a cada passo (a imagem
 *  é desenhada na largura real; cada miniatura mantém a proporção). */
export function larguraPedida(w: number): number {
  const k = Math.round(4 * Math.log2(Math.max(w, 1)));
  return Math.min(LARGURA_MAX, Math.max(ALTURA_MIN, Math.round(2 ** (k / 4))));
}

/** Altura pedida (px reais), em degraus de 8. */
export function alturaPedida(h: number, dpr: number): number {
  return Math.min(ALTURA_MAX, Math.max(ALTURA_MIN, Math.ceil((h * Math.max(dpr, 1)) / 8) * 8));
}

/** Chave do pedido sem o canal (o que muda a imagem). */
export const chaveDoPedido = (p: PedidoTira) => `${p.sha256}|${p.de_s}|${p.ate_s}|${p.n ?? ""}|${p.largura ?? ""}|${p.altura}`;

/**
 * Blocos de um trecho de fonte `[de_s, ate_s)` mostrado em `largura` px, só os que cruzam `[vis0, vis1]` (px dentro do
 * item). Até `BLOCO_PX`: um bloco, canal = id do clipe. Mais largo: blocos de duração em potência de 2 (estáveis
 * enquanto o zoom muda pouco), canal `clipe#i`.
 */
export function blocosDoTrecho(x: { clipe: string; sha256: string; de_s: Seg; ate_s: Seg; largura: number; altura: number; vis0: number; vis1: number }): BlocoTira[] {
  const dur = x.ate_s - x.de_s;
  if (!(dur > 0) || !(x.largura >= LARGURA_MIN_PX) || x.vis1 < 0 || x.vis0 > x.largura) return [];
  const base = { sha256: x.sha256, n: null, altura: x.altura };
  if (x.largura <= BLOCO_PX) {
    return [{ canal: x.clipe, x: 0, w: x.largura, pedido: { ...base, de_s: x.de_s, ate_s: x.ate_s, largura: larguraPedida(x.largura), canal: x.clipe } }];
  }
  const k = x.largura / dur;
  const D = 2 ** Math.round(Math.log2(BLOCO_PX / k));
  const i0 = Math.max(0, Math.floor(Math.max(x.vis0, 0) / (D * k)));
  const i1 = Math.min(Math.ceil(dur / D) - 1, Math.floor(Math.min(x.vis1, x.largura) / (D * k)));
  const r: BlocoTira[] = [];
  for (let i = i0; i <= i1; i++) {
    const de = x.de_s + i * D;
    const ate = Math.min(x.ate_s, de + D);
    const w = (ate - de) * k;
    if (w < 1) continue;
    const canal = `${x.clipe}#${i}`;
    r.push({ canal, x: i * D * k, w, pedido: { ...base, de_s: de, ate_s: ate, largura: larguraPedida(w), canal } });
  }
  return r;
}

/** Onde desenhar cada miniatura: a janela k (de `n`, a da resposta) ocupa `[k·w/n, (k+1)·w/n)` do bloco; a célula vai
 *  na proporção dela a partir do começo da janela, cortada na janela. */
export function janelasDaTira(t: TiraMiniaturas, w: number, h: number): { celula: number; x: number; largura: number; sx: number; sw: number }[] {
  if (!t.relativo || t.n <= 0 || t.w <= 0 || t.h <= 0) return [];
  const passo = w / t.n;
  const natural = (t.w * h) / t.h;
  const r = [];
  for (let k = 0; k < t.n; k++) {
    const celula = t.miniaturas[k];
    if (celula === undefined) continue;
    const largura = Math.min(natural, passo + 0.5);
    r.push({ celula, x: k * passo, largura, sx: celula * t.w, sw: (largura / natural) * t.w });
  }
  return r;
}

// ---- pedidos: um por canal, o último vence ----

type Situacao = { tipo: "pedindo" } | { tipo: "fundo"; chave: string } | { tipo: "tabela"; sha256: string } | { tipo: "pronta" } | { tipo: "falhou" };

interface Canal {
  ws: string;
  pedido: PedidoTira;
  chave: string;
  situacao: Situacao;
  /** Pedidos repetidos sem resposta nova (a tarefa já estava pronta quando a resposta chegou): teto contra laço. */
  repetidos: number;
}

export type PedirTira = (ws: string, p: PedidoTira) => Promise<RespostaTira>;

/** Repetições seguidas de um canal sem tira (cache apagado, tabela que não muda a resposta). */
const MAX_REPETIDOS = 2;

/** Estado dos pedidos de tira (fora do React). `aoMudar` avisa a tela quando chega uma. */
export class Tiras {
  private canais = new Map<string, Canal>();
  private prontas = new Map<string, TiraMiniaturas>();
  /** Tarefas de fundo já tratadas (id + estado): a lista do evento vem inteira a cada mudança. */
  private vistas = new Set<string>();

  constructor(
    private readonly pedirTira: PedirTira,
    private readonly aoMudar: () => void,
    /** A lista atual da fila de fundo: a tarefa pode terminar antes de a resposta do pedido chegar. */
    private readonly tarefasAgora: () => readonly TarefaFundo[] = () => [],
  ) {}

  /** A tira desse pedido, se já chegou. */
  tira(p: PedidoTira): TiraMiniaturas | null {
    return this.prontas.get(chaveDoPedido(p)) ?? null;
  }

  /** Pede (se ainda não tem nem está pedindo a mesma coisa no canal). */
  pedir(ws: string, p: PedidoTira): void {
    const chave = chaveDoPedido(p);
    if (this.prontas.has(chave)) return;
    const atual = this.canais.get(p.canal);
    if (atual && atual.chave === chave && atual.ws === ws && atual.situacao.tipo !== "pronta") return;
    this.enviar({ ws, pedido: p, chave, situacao: { tipo: "pedindo" }, repetidos: 0 });
  }

  private enviar(c: Canal): void {
    this.canais.set(c.pedido.canal, c);
    c.situacao = { tipo: "pedindo" };
    this.pedirTira(c.ws, c.pedido).then(
      (r) => {
        if (this.canais.get(c.pedido.canal) !== c) return;
        if (r.tira) {
          this.prontas.set(c.chave, r.tira);
          c.situacao = { tipo: "pronta" };
          this.aoMudar();
          return;
        }
        c.situacao = { tipo: "fundo", chave: r.chave };
        const t = ultimaComChave(this.tarefasAgora(), r.chave);
        if (t && (t.estado.tipo === "pronta" || t.estado.tipo === "cancelada")) this.repetir(c);
      },
      (e: unknown) => {
        if (this.canais.get(c.pedido.canal) !== c) return;
        if (separarErro(e).codigo !== "tabela_pendente") {
          c.situacao = { tipo: "falhou" };
          return;
        }
        c.situacao = { tipo: "tabela", sha256: c.pedido.sha256 };
        if (ultimaComChave(this.tarefasAgora(), `tabela:${c.pedido.sha256}`)?.estado.tipo === "pronta") this.repetir(c);
      },
    );
  }

  /** Pede de novo já (a tarefa terminou antes de a resposta chegar), até o teto. */
  private repetir(c: Canal): void {
    if (c.repetidos >= MAX_REPETIDOS) {
      c.situacao = { tipo: "falhou" };
      return;
    }
    c.repetidos++;
    this.enviar(c);
  }

  /** Lista da fila de fundo (`edicao:fundo`): tira pronta ou cancelada da última chave do canal, tabela pronta. */
  receberFundo(tarefas: readonly TarefaFundo[]): void {
    const novas = tarefas.filter((t) => {
      const v = `${t.id}:${t.estado.tipo}`;
      if (this.vistas.has(v)) return false;
      this.vistas.add(v);
      return true;
    });
    if (novas.length === 0) return;
    for (const c of [...this.canais.values()]) {
      const s = c.situacao;
      if (s.tipo === "fundo") {
        const t = ultimaComChave(novas, s.chave);
        // pronta: a resposta agora vem do cache; cancelada: só porque ainda é a última do canal (o canal guarda só ela)
        if (t && (t.estado.tipo === "pronta" || t.estado.tipo === "cancelada")) {
          c.repetidos = 0;
          this.enviar(c);
        } else if (t?.estado.tipo === "falhou") c.situacao = { tipo: "falhou" };
      } else if (s.tipo === "tabela") {
        const t = ultimaComChave(novas, `tabela:${s.sha256}`);
        if (t?.estado.tipo === "pronta") {
          c.repetidos = 0;
          this.enviar(c);
        }
      }
    }
  }

  /** Só para testes. */
  situacao(canal: string): Situacao["tipo"] | null {
    return this.canais.get(canal)?.situacao.tipo ?? null;
  }

  /** Troca de caso: esquece tudo. */
  limpar(): void {
    this.canais.clear();
    this.prontas.clear();
  }
}

function ultimaComChave(ts: readonly TarefaFundo[], chave: string): TarefaFundo | null {
  for (let i = ts.length - 1; i >= 0; i--) if (ts[i]!.chave === chave) return ts[i]!;
  return null;
}

// ---- a instância da tela ----

export const useTirasVersao = create<{ versao: number }>(() => ({ versao: 0 }));
const subir = () => useTirasVersao.setState((s) => ({ versao: s.versao + 1 }));

export const tiras = new Tiras(
  // erro síncrono (comando ausente) vira recusa: a faixa fica lisa
  (ws, p) => Promise.resolve().then(() => commands.edicaoTira(ws, p)),
  subir,
  () => useFundoStore.getState().tarefas,
);
let ouvindo = false;
function ouvirFundo() {
  if (ouvindo) return;
  ouvindo = true;
  useFundoStore.subscribe((s) => tiras.receberFundo(s.tarefas));
}

/** Pede os blocos visíveis depois de `ESPERA_MS` sem mudança (rolagem e zoom pedem só ao parar). */
export function useTirasVisiveis(ws: string | null, blocos: readonly BlocoTira[]): void {
  const assinatura = useMemo(() => blocos.map((b) => `${b.canal}=${chaveDoPedido(b.pedido)}`).join("\n"), [blocos]);
  useEffect(() => {
    if (!ws || blocos.length === 0) return;
    ouvirFundo();
    const t = setTimeout(() => {
      for (const b of blocos.slice(0, MAX_BLOCOS)) tiras.pedir(ws, b.pedido);
    }, ESPERA_MS);
    return () => clearTimeout(t);
  }, [ws, assinatura]); // eslint-disable-line react-hooks/exhaustive-deps
}

// ---- imagens da tira (fora do `palco/recursos`: chegar uma não repinta o editor inteiro) ----

const imagens = new Map<string, { img: HTMLImageElement; pronta: boolean }>();

/** Imagem da tira já carregada; começa a carregar na primeira vez (avisa a linha ao chegar). */
export function imagemDaTira(raizCache: string, t: TiraMiniaturas): HTMLImageElement | null {
  if (!t.relativo) return null;
  const url = urlDoCache(raizCache, t.relativo);
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
