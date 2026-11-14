/**
 * Sobreposições abertas que não são `Dialog` (Fila, menu "Adicionar" do cartão, menu de contexto do Inspetor): com
 * alguma aberta os atalhos da tela ficam desligados, como com um `Dialog` (contratos (h) 25 e (i)).
 */

import { useEffect } from "react";
import { create } from "zustand";

export const useMenusAbertos = create<{ n: number }>(() => ({ n: 0 }));

/** Conta a sobreposição enquanto `aberto`. */
export function useMenuAberto(aberto: boolean): void {
  useEffect(() => {
    if (!aberto) return;
    useMenusAbertos.setState((s) => ({ n: s.n + 1 }));
    return () => useMenusAbertos.setState((s) => ({ n: Math.max(0, s.n - 1) }));
  }, [aberto]);
}
