/**
 * Corrida do rastreio na Edição (§6.3, §8.4): pede "Seguir", acompanha o evento `edicao:rastreio`, aplica o fim
 * (denso e silhuetas) por `alterar()` e guarda os densos lidos para o palco e a faixa.
 * Dono: P2.4 (contratos-fase2.md). Abertura: casca com `RASTREIO_PRONTO = false` (nada escuta nem pede).
 */

import { useEffect } from "react";
import { create } from "zustand";
import type { ArquivoDenso, AvancoRastreio, EventoRastreio, Id, PedidoSeguir, Rastreio } from "@domain/edicao";

/** Ligado pelo P2.4 ao entregar: painel, ferramenta, faixa, atalhos e o denso no palco. */
export const RASTREIO_PRONTO = false;

/** Corrida em andamento (uma por vez, §6.3); `ultimo` = último andamento recebido (miniatura, caixa, estado). */
export interface CorridaNaTela {
  tarefa: string;
  rastreio: Id;
  pedido: PedidoSeguir;
  ultimo: AvancoRastreio | null;
}

export interface RastreioState {
  corrida: CorridaNaTela | null;
  /** Densos lidos (`edicao_rastreio_ler`), por SHA-256. */
  densos: Record<string, ArquivoDenso>;
  /** "Seguir" / "Seguir para trás" / "Retomar". */
  seguir(p: PedidoSeguir): Promise<void>;
  /** "Parar". */
  parar(): Promise<void>;
  /** Evento `edicao:rastreio`: andamento na tela; fim aplicado por `alterar()`; falha no aviso do app. */
  receber(ev: EventoRastreio): void;
}

export const useRastreioStore = create<RastreioState>(() => ({
  corrida: null,
  densos: {},
  seguir: async () => {},
  parar: async () => {},
  receber: () => {},
}));

/** Denso do rastreio já lido (null = sem denso ou ainda não chegou; o P2.4 pede ao backend quando falta).
 *  Quem chama: `palco/cena.ts` (`avaliar(r, em, densoEmCache(r))`) e a faixa. */
export function densoEmCache(r: Rastreio): ArquivoDenso | null {
  if (!RASTREIO_PRONTO || !r.denso) return null;
  return useRastreioStore.getState().densos[r.denso.sha256] ?? null;
}

/** Escuta `edicao:rastreio` enquanto o editor está montado (`EditorEdicao`). */
export function useOuvirRastreio(): void {
  useEffect(() => {
    if (!RASTREIO_PRONTO) return;
    return undefined;
  }, []);
}

/** Atalhos da Edição que o rastreio atende (`edicao.seguirFrente`, `edicao.seguirTras`, `edicao.trechoFraco`);
 *  vazio = seguem com o que o editor já faz. */
export function atalhosDoRastreio(): Record<string, () => void> {
  return {};
}

/** Esc ("Parar") com corrida em andamento: para e devolve true. */
export function pararCorrida(): boolean {
  if (!RASTREIO_PRONTO) return false;
  return useRastreioStore.getState().corrida !== null;
}
