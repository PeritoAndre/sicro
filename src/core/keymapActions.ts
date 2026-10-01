/**
 * Catálogo de AÇÕES com atalho. Cada ação tem um id estável, um rótulo e a
 * tecla PADRÃO (canônica). O usuário pode sobrescrever qualquer uma (store).
 *
 * O catálogo é dividido por módulo: o Geral fica aqui; cada módulo vive em
 * `./keymap/<módulo>.ts` e entra em SHORTCUT_ACTIONS na ORDEM DOS MÓDULOS do app.
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
  | "imagem";

export interface ShortcutAction {
  id: string;
  scope: ActionScope;
  /** Rótulo do grupo para exibição (na tela de Configurações). */
  group: string;
  label: string;
  /** Combinação padrão, na forma canônica de `keymap.ts`. */
  defaultBinding: string;
}

/**
 * Geral — valem em qualquer tela (ligados no AppShell). Zoom da interface
 * inteira usa Shift para não colidir com o Ctrl + = / - / 0 dos módulos.
 */
const GERAL: ShortcutAction[] = [
  { id: "geral.uiZoomIn", scope: "geral", group: "Geral · Interface", label: "Aumentar a interface", defaultBinding: "Ctrl+Shift+=" },
  { id: "geral.uiZoomOut", scope: "geral", group: "Geral · Interface", label: "Diminuir a interface", defaultBinding: "Ctrl+Shift+-" },
  { id: "geral.uiZoomReset", scope: "geral", group: "Geral · Interface", label: "Interface em 100%", defaultBinding: "Ctrl+Shift+0" },
];

// Geral primeiro; depois a ordem dos módulos (ActivityRail): Croqui → Vídeo e
// Áudio → Imagem.
export const SHORTCUT_ACTIONS: ShortcutAction[] = [
  ...GERAL,
  ...CROQUI_ACTIONS,
  ...VIDEO_ACTIONS,
  ...AUDIO_ACTIONS,
  ...IMAGEM_ACTIONS,
];

/** Mapa id → ação (lookup rápido). */
export const ACTION_BY_ID: Record<string, ShortcutAction> = Object.fromEntries(
  SHORTCUT_ACTIONS.map((a) => [a.id, a]),
);
