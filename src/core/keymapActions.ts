/**
 * Catálogo de AÇÕES com atalho. Cada ação tem um id estável, um rótulo e a
 * tecla PADRÃO (canônica). O usuário pode sobrescrever qualquer uma (store).
 *
 * O catálogo é dividido por módulo: Dossiê + Documentoscopia (Confronto e Exame)
 * ficam inline aqui; os demais módulos vivem em `./keymap/<módulo>.ts` e são
 * concatenados em SHORTCUT_ACTIONS na ORDEM DOS MÓDULOS do app.
 */

import { LAUDO_ACTIONS } from "./keymap/laudo";
import { CROQUI_ACTIONS } from "./keymap/croqui";
import { VIDEO_ACTIONS } from "./keymap/video";
import { AUDIO_ACTIONS } from "./keymap/audio";
import { IMAGEM_ACTIONS } from "./keymap/imagem";

export type ActionScope =
  | "dossie"
  | "laudo"
  | "croqui"
  | "video"
  | "audio"
  | "imagem"
  | "confronto"
  | "exame";

export interface ShortcutAction {
  id: string;
  scope: ActionScope;
  /** Rótulo do grupo para exibição (na tela de Configurações). */
  group: string;
  label: string;
  /** Combinação padrão, na forma canônica de `keymap.ts`. */
  defaultBinding: string;
}

/** Dossiê — navegação entre as duas lentes + abas, e editar o cabeçalho. */
const DOSSIE: ShortcutAction[] = [
  { id: "dossie.lens.operacional", scope: "dossie", group: "Dossiê · Navegação", label: "Lente SICRO Operacional", defaultBinding: "Alt+O" },
  { id: "dossie.lens.provas", scope: "dossie", group: "Dossiê · Navegação", label: "Lente Central de Provas", defaultBinding: "Alt+P" },
  { id: "dossie.tab.prev", scope: "dossie", group: "Dossiê · Navegação", label: "Aba anterior", defaultBinding: "Ctrl+PgUp" },
  { id: "dossie.tab.next", scope: "dossie", group: "Dossiê · Navegação", label: "Próxima aba", defaultBinding: "Ctrl+PgDn" },

  { id: "dossie.editIdentificacao", scope: "dossie", group: "Dossiê · Ações", label: "Editar identificação do caso", defaultBinding: "Alt+E" },
];

/** Confronto sincronizado — bancada tipo "canvas", atalhos de tecla única. */
const CONFRONTO: ShortcutAction[] = [
  { id: "confronto.tool.pan", scope: "confronto", group: "Confronto · Ferramentas", label: "Selecionar / mover (pan)", defaultBinding: "V" },
  { id: "confronto.tool.marker", scope: "confronto", group: "Confronto · Ferramentas", label: "Marcador numerado", defaultBinding: "M" },
  { id: "confronto.tool.ruler", scope: "confronto", group: "Confronto · Ferramentas", label: "Medir distância", defaultBinding: "R" },
  { id: "confronto.tool.angle", scope: "confronto", group: "Confronto · Ferramentas", label: "Medir ângulo", defaultBinding: "A" },
  { id: "confronto.tool.pencil", scope: "confronto", group: "Confronto · Ferramentas", label: "Lápis (traço livre)", defaultBinding: "P" },
  { id: "confronto.tool.text", scope: "confronto", group: "Confronto · Ferramentas", label: "Texto", defaultBinding: "T" },
  { id: "confronto.nextMarker", scope: "confronto", group: "Confronto · Ferramentas", label: "Próximo nº de marcador", defaultBinding: "N" },
  { id: "confronto.cancel", scope: "confronto", group: "Confronto · Ferramentas", label: "Cancelar ferramenta / seleção", defaultBinding: "Esc" },

  { id: "confronto.mode.side", scope: "confronto", group: "Confronto · Vista", label: "Modo lado a lado", defaultBinding: "S" },
  { id: "confronto.mode.overlay", scope: "confronto", group: "Confronto · Vista", label: "Modo sobreposição", defaultBinding: "O" },
  { id: "confronto.lockZoom", scope: "confronto", group: "Confronto · Vista", label: "Travar zoom", defaultBinding: "L" },
  { id: "confronto.syncPan", scope: "confronto", group: "Confronto · Vista", label: "Pan sincronizado", defaultBinding: "Y" },
  { id: "confronto.grid", scope: "confronto", group: "Confronto · Vista", label: "Grade de referência", defaultBinding: "G" },
  { id: "confronto.sharp", scope: "confronto", group: "Confronto · Vista", label: "Pixels nítidos", defaultBinding: "K" },
  { id: "confronto.fit", scope: "confronto", group: "Confronto · Vista", label: "Enquadrar", defaultBinding: "F" },
  { id: "confronto.oneToOne", scope: "confronto", group: "Confronto · Vista", label: "1:1 (pixel real)", defaultBinding: "0" },
  { id: "confronto.zoomIn", scope: "confronto", group: "Confronto · Vista", label: "Ampliar", defaultBinding: "=" },
  { id: "confronto.zoomOut", scope: "confronto", group: "Confronto · Vista", label: "Reduzir", defaultBinding: "-" },
  { id: "confronto.resetView", scope: "confronto", group: "Confronto · Vista", label: "Redefinir vista", defaultBinding: "Shift+F" },
  { id: "confronto.swap", scope: "confronto", group: "Confronto · Vista", label: "Trocar lados", defaultBinding: "X" },

  { id: "confronto.undo", scope: "confronto", group: "Confronto · Edição", label: "Desfazer", defaultBinding: "Ctrl+Z" },
  { id: "confronto.redo", scope: "confronto", group: "Confronto · Edição", label: "Refazer", defaultBinding: "Ctrl+Y" },
  { id: "confronto.save", scope: "confronto", group: "Confronto · Edição", label: "Salvar confronto", defaultBinding: "Ctrl+S" },
  { id: "confronto.export", scope: "confronto", group: "Confronto · Edição", label: "Exportar imagem", defaultBinding: "Ctrl+E" },
];

/** Exame de documento — navegação entre grupos do painel + ações comuns. */
const EXAME: ShortcutAction[] = [
  { id: "exame.group.leitura", scope: "exame", group: "Exame · Navegação", label: "Grupo Leitura", defaultBinding: "Alt+1" },
  { id: "exame.group.extracao", scope: "exame", group: "Exame · Navegação", label: "Grupo Extração", defaultBinding: "Alt+2" },
  { id: "exame.group.indicios", scope: "exame", group: "Exame · Navegação", label: "Grupo Indícios digitais", defaultBinding: "Alt+3" },
  { id: "exame.group.proveniencia", scope: "exame", group: "Exame · Navegação", label: "Grupo Proveniência", defaultBinding: "Alt+4" },
];

// Ordem dos módulos (ActivityRail): Dossiê → Laudo → Croqui → Vídeo → Áudio →
// Imagem → Documentoscopia (Confronto + Exame).
export const SHORTCUT_ACTIONS: ShortcutAction[] = [
  ...DOSSIE,
  ...LAUDO_ACTIONS,
  ...CROQUI_ACTIONS,
  ...VIDEO_ACTIONS,
  ...AUDIO_ACTIONS,
  ...IMAGEM_ACTIONS,
  ...CONFRONTO,
  ...EXAME,
];

/** Mapa id → ação (lookup rápido). */
export const ACTION_BY_ID: Record<string, ShortcutAction> = Object.fromEntries(
  SHORTCUT_ACTIONS.map((a) => [a.id, a]),
);
