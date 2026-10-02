/**
 * Catálogo de ações com atalho (id estável + tecla padrão canônica; o usuário
 * sobrescreve pela store). Geral fica aqui; cada módulo em `./keymap/<módulo>.ts`.
 */

import { CROQUI_ACTIONS } from "./keymap/croqui";
import { VIDEO_ACTIONS } from "./keymap/video";
import { AUDIO_ACTIONS } from "./keymap/audio";
import { IMAGEM_ACTIONS } from "./keymap/imagem";

export type ActionScope =
  | "geral"
  | "croqui"
  | "video"
  | "audio"
  | "imagem"
  | "integridade";

export interface ShortcutAction {
  id: string;
  scope: ActionScope;
  /** Grupo na tela de Configurações. */
  group: string;
  label: string;
  /** Forma canônica de `keymap.ts`. */
  defaultBinding: string;
}

/** Valem em qualquer tela (ligados no AppShell). Shift evita colidir com o Ctrl+=/-/0 dos módulos. */
const GERAL: ShortcutAction[] = [
  { id: "geral.uiZoomIn", scope: "geral", group: "Geral · Interface", label: "Aumentar a interface", defaultBinding: "Ctrl+Shift+=" },
  { id: "geral.uiZoomOut", scope: "geral", group: "Geral · Interface", label: "Diminuir a interface", defaultBinding: "Ctrl+Shift+-" },
  { id: "geral.uiZoomReset", scope: "geral", group: "Geral · Interface", label: "Interface em 100%", defaultBinding: "Ctrl+Shift+0" },
];

const INTEGRIDADE: ShortcutAction[] = [
  { id: "integridade.tab.prev", scope: "integridade", group: "Integridade · Navegação", label: "Aba anterior", defaultBinding: "Ctrl+PgUp" },
  { id: "integridade.tab.next", scope: "integridade", group: "Integridade · Navegação", label: "Próxima aba", defaultBinding: "Ctrl+PgDn" },
];

// Geral primeiro; depois a ordem dos módulos no ActivityRail.
export const SHORTCUT_ACTIONS: ShortcutAction[] = [
  ...GERAL,
  ...CROQUI_ACTIONS,
  ...VIDEO_ACTIONS,
  ...AUDIO_ACTIONS,
  ...IMAGEM_ACTIONS,
  ...INTEGRIDADE,
];

export const ACTION_BY_ID: Record<string, ShortcutAction> = Object.fromEntries(
  SHORTCUT_ACTIONS.map((a) => [a.id, a]),
);
