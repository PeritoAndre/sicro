/**
 * Estado de tela do Inspetor que outros pedaços da Edição também tocam: a aba aberta (o diálogo de exportar
 * salta para o item com problema) e o pedido de marcação no palco (`ContextoClipe.marcar`).
 */

import { create } from "zustand";
import type { Caixa, Lugar, PedidoMarcacao, Ponto, Pts, Seg } from "@domain/edicao";

export type Aba = "clipe" | "audio" | "legendas" | "saida";

export type Marcado = (v: Caixa | Ponto | Lugar, t: Seg, pts: Pts | null) => void;

interface UiState {
  aba: Aba;
  /** Pedido de marcação em curso: o palco o recebe como `ferramenta` (PalcoProps). */
  ferramenta: PedidoMarcacao | null;
  aoMarcar: Marcado | null;
  /** Quem pediu (o botão que fica aceso enquanto a ferramenta está ligada). */
  chave: string | null;
  abrirAba(a: Aba): void;
  /** Liga a ferramenta no palco; `aoMarcar` grava o que o perito marcou. */
  pedir(p: PedidoMarcacao, aoMarcar: Marcado, chave?: string): void;
  /** O palco chama com o que foi marcado (`PalcoProps.onMarcado`); desliga a ferramenta. */
  marcado(v: Caixa | Ponto | Lugar, t: Seg, pts: Pts | null): void;
  /** Esc ou troca de item. */
  cancelarMarcacao(): void;
}

export const useInspetorUi = create<UiState>((set, get) => ({
  aba: "clipe",
  ferramenta: null,
  aoMarcar: null,
  chave: null,
  abrirAba: (aba) => set({ aba }),
  pedir: (ferramenta, aoMarcar, chave) => set({ ferramenta, aoMarcar, chave: chave ?? null }),
  marcado(v, t, pts) {
    const f = get().aoMarcar;
    set({ ferramenta: null, aoMarcar: null, chave: null });
    f?.(v, t, pts);
  },
  cancelarMarcacao: () => set({ ferramenta: null, aoMarcar: null, chave: null }),
}));
