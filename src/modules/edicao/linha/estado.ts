/**
 * Estado de tela da linha do tempo (não é documento: não entra no histórico nem no `projeto.json`). Zoom, ferramenta,
 * ímã, rolagem, altura das trilhas e trilhas travadas; guardado por projeto no `localStorage` (falha = só a sessão).
 * Dono: P4.1.
 */

import { create } from "zustand";
import type { Ferramenta } from "./arrastar";

/** Zoom da linha (px por segundo do programa). */
export const PX_POR_S_MIN = 2;
export const PX_POR_S_MAX = 400;
export const PX_POR_S_PADRAO = 14;

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
  definir(p: Partial<Omit<LinhaUi, "definir">>): void;
}

export const useLinhaUi = create<LinhaUi>((set) => ({
  aberta: true,
  pxPorS: PX_POR_S_PADRAO,
  ferramenta: "selecionar",
  ima: true,
  rolagem_s: 0,
  alturas: {},
  travadas: {},
  definir: (p) => set(p),
}));
