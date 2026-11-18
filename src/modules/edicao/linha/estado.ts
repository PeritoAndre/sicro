/**
 * Estado de tela da linha do tempo (não é documento: não entra no histórico nem no `projeto.json`). Zoom, ferramenta,
 * ímã, rolagem, altura das trilhas e do painel, trilhas travadas; guardado por projeto no `localStorage` (falha = só
 * a sessão). Dono: P4.1.
 */

import { create } from "zustand";
import type { Id } from "@domain/edicao";
import type { Ferramenta } from "./arrastar";

/** Zoom da linha (px por segundo do programa). O teto real também respeita a largura que o navegador desenha. */
export const PX_POR_S_MIN = 0.05;
export const PX_POR_S_MAX = 2000;
export const PX_POR_S_PADRAO = 14;
/** Altura do painel (px): barra + régua + trilhas. 0 = automática (uma fração da janela, entre o mínimo e o teto). */
export const ALTURA_PAINEL_MIN = 150;
export const ALTURA_PAINEL_PADRAO = 0;
export const ALTURA_PAINEL_AUTO = { fracao: 0.3, min: 170, max: 320 } as const;

/** Altura do painel na janela de altura `h`: a escolhida (até 70 % da janela) ou a automática. */
export function alturaDoPainel(escolhida: number, h: number): number {
  if (!(h > 0)) return escolhida > 0 ? escolhida : ALTURA_PAINEL_AUTO.max;
  if (escolhida > 0) return Math.max(ALTURA_PAINEL_MIN, Math.min(escolhida, h * 0.7));
  return Math.round(Math.min(Math.max(h * ALTURA_PAINEL_AUTO.fracao, ALTURA_PAINEL_AUTO.min), ALTURA_PAINEL_AUTO.max));
}
export const ALTURA_TRILHA_MIN = 16;
export const ALTURA_TRILHA_MAX = 160;

const CHAVE = "sicro.edicao.linha.v1";
/** Projetos guardados no máximo (os mais recentes). */
const MAX_PROJETOS = 40;

export interface LinhaUi {
  aberta: boolean;
  pxPorS: number;
  ferramenta: Ferramenta;
  ima: boolean;
  /** Início da área visível (s do programa). */
  rolagem_s: number;
  /** Altura de cada trilha (px), por id da trilha. */
  alturas: Record<string, number>;
  /** Trilhas travadas (sem arrasto nem lâmina), por id. */
  travadas: Record<string, boolean>;
  // CONTRATO: acréscimos do P4.1 ao estado de tela (não vão ao documento).
  /** Altura do painel (px; 0 = automática). */
  altura: number;
  /** Projeto aberto pela primeira vez (nada guardado): a linha abre ajustada ao programa. */
  ajustarAoAbrir: boolean;
  /** Projeto cujo estado está carregado (o que `definir` grava). */
  projeto: Id | null;
  definir(p: Partial<Omit<LinhaUi, "definir" | "carregar" | "projeto">>): void;
  /** Lê o estado guardado do projeto (o padrão se não houver). */
  carregar(projeto: Id | null): void;
}

type Guardado = Pick<LinhaUi, "aberta" | "pxPorS" | "ferramenta" | "ima" | "rolagem_s" | "alturas" | "travadas" | "altura">;

export const UI_PADRAO: Guardado = Object.freeze({
  aberta: true,
  pxPorS: PX_POR_S_PADRAO,
  ferramenta: "selecionar",
  ima: true,
  rolagem_s: 0,
  alturas: {},
  travadas: {},
  altura: ALTURA_PAINEL_PADRAO,
}) as Guardado;

const finito = (v: unknown, a: number, b: number, p: number) => (typeof v === "number" && Number.isFinite(v) ? Math.min(Math.max(v, a), b) : p);

function registros(v: unknown, f: (x: unknown) => boolean): Record<string, never> {
  const r: Record<string, unknown> = {};
  if (v && typeof v === "object") for (const [k, x] of Object.entries(v as Record<string, unknown>)) if (f(x)) r[k] = x;
  return r as Record<string, never>;
}

/** O que veio do `localStorage` para um projeto, com cada campo conferido (lixo = padrão). */
export function lerGuardado(o: unknown): Guardado {
  const x = (o && typeof o === "object" ? o : {}) as Partial<Record<keyof Guardado, unknown>>;
  return {
    aberta: x.aberta !== false,
    pxPorS: finito(x.pxPorS, PX_POR_S_MIN, PX_POR_S_MAX, PX_POR_S_PADRAO),
    // a lâmina não volta armada ao reabrir o projeto (o primeiro clique cortaria)
    ferramenta: "selecionar",
    ima: x.ima !== false,
    rolagem_s: finito(x.rolagem_s, 0, 1e7, 0),
    alturas: registros(x.alturas, (h) => typeof h === "number" && h >= ALTURA_TRILHA_MIN && h <= ALTURA_TRILHA_MAX),
    travadas: registros(x.travadas, (b) => b === true),
    altura: x.altura === 0 ? 0 : finito(x.altura, ALTURA_PAINEL_MIN, 2000, ALTURA_PAINEL_PADRAO),
  };
}

function lerTudo(): Record<string, unknown> {
  try {
    const o = JSON.parse(localStorage.getItem(CHAVE) ?? "{}") as unknown;
    return o && typeof o === "object" ? (o as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

const gravar = new Map<Id, ReturnType<typeof setTimeout>>();

/** Grava o estado do projeto (com espera: a rolagem muda a cada quadro). */
function guardar(projeto: Id, g: Guardado): void {
  const antes = gravar.get(projeto);
  if (antes !== undefined) clearTimeout(antes);
  gravar.set(
    projeto,
    setTimeout(() => {
    gravar.delete(projeto);
    try {
      const tudo = lerTudo();
      delete tudo[projeto];
      tudo[projeto] = g;
      const ids = Object.keys(tudo);
      for (const id of ids.slice(0, Math.max(0, ids.length - MAX_PROJETOS))) delete tudo[id];
      localStorage.setItem(CHAVE, JSON.stringify(tudo));
    } catch {
      /* localStorage indisponível: vale só nesta sessão */
    }
    }, 300),
  );
}

const guardado = (s: LinhaUi): Guardado => ({
  aberta: s.aberta,
  pxPorS: s.pxPorS,
  ferramenta: s.ferramenta,
  ima: s.ima,
  rolagem_s: s.rolagem_s,
  alturas: s.alturas,
  travadas: s.travadas,
  altura: s.altura,
});

export const useLinhaUi = create<LinhaUi>((set, get) => ({
  ...UI_PADRAO,
  projeto: null,
  ajustarAoAbrir: false,
  definir: (p) => {
    set(p);
    const s = get();
    if (s.projeto) guardar(s.projeto, guardado(s));
  },
  carregar: (projeto) => {
    if (get().projeto === projeto) return;
    const guardado = projeto ? lerTudo()[projeto] : undefined;
    const g = guardado !== undefined ? lerGuardado(guardado) : { ...UI_PADRAO };
    set({ ...g, projeto, ajustarAoAbrir: guardado === undefined && projeto !== null });
  },
}));
