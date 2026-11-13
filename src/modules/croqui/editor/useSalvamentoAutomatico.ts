/** Salva sozinho alguns segundos depois da última mudança: queda de energia não leva o trabalho. */

import { useEffect, useRef } from "react";

const ATRASO_MS = 3000;

export function useSalvamentoAutomatico(dirty: boolean, salvar: () => Promise<boolean>, ativo = true): void {
  const salvarRef = useRef(salvar);
  salvarRef.current = salvar;
  const rodando = useRef(false);
  useEffect(() => {
    if (!ativo || !dirty) return;
    const t = setTimeout(() => {
      if (rodando.current) return; // o próximo dirty reagenda
      rodando.current = true;
      void salvarRef.current().finally(() => {
        rodando.current = false;
      });
    }, ATRASO_MS);
    return () => clearTimeout(t);
    // `salvar` fica fora das deps de propósito: muda a cada render e zeraria o temporizador.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dirty, ativo]);
}
