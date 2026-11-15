/**
 * Situação do intermediário de origem de cada clipe (§7.5). Quem pede e acompanha é `useOrigem` (seção Realce e
 * palco); os cartões da Sequência e o "Marcar" das anotações leem daqui.
 * CONTRATO: módulo novo (contratos-fase1 F3); saiu de `RealceSecao.tsx`.
 */

import { create } from "zustand";
import type { EstadoOrigem, Id } from "@domain/edicao";
import type { ErroComando } from "./edicaoStore";

export type OrigemDoClipe = EstadoOrigem | { tipo: "falhou"; erro: ErroComando };

interface OrigemState {
  porClipe: Record<Id, OrigemDoClipe | undefined>;
  guardar(clipe: Id, e: OrigemDoClipe | undefined): void;
}

export const useOrigemStore = create<OrigemState>((set) => ({
  porClipe: {},
  guardar: (clipe, e) => set((s) => (s.porClipe[clipe] === e ? s : { porClipe: { ...s.porClipe, [clipe]: e } })),
}));
