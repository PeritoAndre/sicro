/**
 * useEditorMode — controle dos modos de operação do editor de laudo.
 *
 * Modos:
 *   - `"edicao"`  — comportamento padrão. Toolbar visível, Inspector visível,
 *                   status bar visível, editor editável.
 *   - `"leitura"` — somente leitura. Toolbar reduzida, status bar mostra modo,
 *                   editor não editável. Usado para revisão visual sem risco
 *                   de modificar acidentalmente.
 *   - `"foco"`    — minimalista. Esconde Inspector, status bar reduzida, só
 *                   texto. Útil para escrita concentrada. Toolbar mantida
 *                   (perito ainda precisa de B/I/etc).
 *   - `"revisao"` — modo revisão. Editor permanece editável mas registra
 *                   alterações como sugestões (preparação para F8 — controle
 *                   de alterações). Inspector mostra painel de comentários.
 *
 * O hook é leve — apenas state + setter. As decisões de UI ficam nos
 * componentes que reagem ao modo via prop ou useEditorMode(read).
 */

import { useCallback, useState } from "react";

export type LaudoMode = "edicao" | "leitura" | "foco" | "revisao";

export interface UseEditorModeReturn {
  mode: LaudoMode;
  setMode: (mode: LaudoMode) => void;
  isEditable: boolean;
  showInspector: boolean;
  showStatusBar: boolean;
}

export function useEditorMode(initial: LaudoMode = "edicao"): UseEditorModeReturn {
  const [mode, setMode] = useState<LaudoMode>(initial);

  const set = useCallback((m: LaudoMode) => setMode(m), []);

  // Centraliza as decisões de UI por modo em UM lugar — caller só precisa
  // ler o boolean apropriado.
  const isEditable = mode === "edicao" || mode === "revisao" || mode === "foco";
  const showInspector = mode === "edicao" || mode === "revisao";
  const showStatusBar = mode !== "foco";

  return {
    mode,
    setMode: set,
    isEditable,
    showInspector,
    showStatusBar,
  };
}

/** Label humano em PT-BR para exibir na status bar ou seletor. */
export function laudoModeLabel(mode: LaudoMode): string {
  switch (mode) {
    case "leitura":
      return "Leitura";
    case "foco":
      return "Foco";
    case "revisao":
      return "Revisão";
    case "edicao":
    default:
      return "Edição";
  }
}
