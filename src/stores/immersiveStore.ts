/** Dentro de um item específico (croqui, vídeo, áudio, imagem) o menu lateral se recolhe. */

import { useEffect } from "react";
import { create } from "zustand";

export const useImmersiveStore = create<{ on: boolean; set: (on: boolean) => void }>((set) => ({
  on: false,
  set: (on) => set({ on }),
}));

/** Liga o modo imersivo enquanto o componente estiver montado (e `on` for verdadeiro). */
export function useImmersive(on = true): void {
  const set = useImmersiveStore((s) => s.set);
  useEffect(() => {
    set(on);
    return () => set(false);
  }, [on, set]);
}
