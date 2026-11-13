/** Abre/fecha os diálogos globais de caso (Ctrl+O e Ctrl+N), de qualquer tela. */

import { create } from "zustand";

interface DialogosCaso {
  trocar: boolean;
  novo: boolean;
  abrirTrocar: () => void;
  fecharTrocar: () => void;
  abrirNovo: () => void;
  fecharNovo: () => void;
}

export const useDialogosCaso = create<DialogosCaso>((set) => ({
  trocar: false,
  novo: false,
  abrirTrocar: () => set({ trocar: true, novo: false }),
  fecharTrocar: () => set({ trocar: false }),
  abrirNovo: () => set({ novo: true, trocar: false }),
  fecharNovo: () => set({ novo: false }),
}));
