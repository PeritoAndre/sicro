/**
 * Atalhos customizáveis do módulo **Laudo**. Escopo `laudo`.
 * Convenção: `group: "Laudo · <subárea>"`, label PT-BR, `defaultBinding` na
 * forma canônica de `keymap.ts` (ex.: "Ctrl+B", "Alt+1", "Esc", "Ctrl+PgDn").
 *
 * IMPORTANTE: o editor de texto rico é o TipTap/ProseMirror, que JÁ trata
 * internamente Ctrl+B/I/U (negrito/itálico/sublinhado), Ctrl+Z/Y
 * (desfazer/refazer), Ctrl+Shift+X (tachado) e a navegação/edição de texto.
 * Esses atalhos NÃO entram no catálogo — reescrevê-los aqui brigaria com o
 * editor. O que cobrimos abaixo são as ações de NÍVEL DE APLICAÇÃO, ligadas
 * na página do editor (`LaudoEditorView`), fora do ProseMirror: salvar,
 * exportar, localizar/substituir, zoom, estilos documentais (que são um
 * comando próprio, `applyLaudoStyle`, e não os headings nativos), modos de
 * exibição, preview e a lista de atalhos.
 */
import type { ShortcutAction } from "../keymapActions";

export const LAUDO_ACTIONS: ShortcutAction[] = [
  // Documento — persistência e saída.
  { id: "laudo.save", scope: "laudo", group: "Laudo · Documento", label: "Salvar laudo", defaultBinding: "Ctrl+S" },
  { id: "laudo.exportPdf", scope: "laudo", group: "Laudo · Documento", label: "Exportar PDF", defaultBinding: "Ctrl+P" },

  // Edição — localizar/substituir (a barra é de nível de app, não do TipTap).
  { id: "laudo.find", scope: "laudo", group: "Laudo · Edição", label: "Localizar", defaultBinding: "Ctrl+F" },
  { id: "laudo.replace", scope: "laudo", group: "Laudo · Edição", label: "Localizar e substituir", defaultBinding: "Ctrl+H" },

  // Estilos documentais — aplicam um estilo do catálogo (applyLaudoStyle),
  // não os headings nativos do TipTap. Ctrl+Alt+dígito por convenção.
  { id: "laudo.style.normal", scope: "laudo", group: "Laudo · Estilos", label: "Estilo: Normal (parágrafo)", defaultBinding: "Ctrl+Alt+0" },
  { id: "laudo.style.titulo1", scope: "laudo", group: "Laudo · Estilos", label: "Estilo: Título 1 (principal)", defaultBinding: "Ctrl+Alt+1" },
  { id: "laudo.style.titulo2", scope: "laudo", group: "Laudo · Estilos", label: "Estilo: Título 2 (seção)", defaultBinding: "Ctrl+Alt+2" },
  { id: "laudo.style.titulo3", scope: "laudo", group: "Laudo · Estilos", label: "Estilo: Título 3 (subseção)", defaultBinding: "Ctrl+Alt+3" },
  { id: "laudo.style.subtitulo", scope: "laudo", group: "Laudo · Estilos", label: "Estilo: Subtítulo", defaultBinding: "Ctrl+Alt+4" },
  { id: "laudo.style.secaoTecnica", scope: "laudo", group: "Laudo · Estilos", label: "Estilo: Seção técnica", defaultBinding: "Ctrl+Alt+5" },
  { id: "laudo.style.quesito", scope: "laudo", group: "Laudo · Estilos", label: "Estilo: Quesito", defaultBinding: "Ctrl+Alt+6" },
  { id: "laudo.style.resposta", scope: "laudo", group: "Laudo · Estilos", label: "Estilo: Resposta", defaultBinding: "Ctrl+Alt+7" },

  // Vista — zoom (CSS scale, não altera o documento) e enquadramento.
  { id: "laudo.zoomIn", scope: "laudo", group: "Laudo · Vista", label: "Ampliar", defaultBinding: "Ctrl+=" },
  { id: "laudo.zoomOut", scope: "laudo", group: "Laudo · Vista", label: "Reduzir", defaultBinding: "Ctrl+-" },
  { id: "laudo.zoomReset", scope: "laudo", group: "Laudo · Vista", label: "Zoom 100%", defaultBinding: "Ctrl+0" },
  { id: "laudo.fitWidth", scope: "laudo", group: "Laudo · Vista", label: "Ajustar à largura", defaultBinding: "Alt+W" },
  { id: "laudo.fitPage", scope: "laudo", group: "Laudo · Vista", label: "Ajustar à página", defaultBinding: "Alt+F" },
  { id: "laudo.togglePreview", scope: "laudo", group: "Laudo · Vista", label: "Alternar pré-visualização (HTML)", defaultBinding: "Alt+V" },

  // Modos do editor — edição / leitura / foco / revisão.
  { id: "laudo.mode.edicao", scope: "laudo", group: "Laudo · Modos", label: "Modo Edição", defaultBinding: "Alt+1" },
  { id: "laudo.mode.leitura", scope: "laudo", group: "Laudo · Modos", label: "Modo Leitura", defaultBinding: "Alt+2" },
  { id: "laudo.mode.foco", scope: "laudo", group: "Laudo · Modos", label: "Modo Foco", defaultBinding: "Alt+3" },
  { id: "laudo.mode.revisao", scope: "laudo", group: "Laudo · Modos", label: "Modo Revisão", defaultBinding: "Alt+4" },

  // Ajuda / geral.
  { id: "laudo.help", scope: "laudo", group: "Laudo · Geral", label: "Abrir lista de atalhos", defaultBinding: "?" },
  { id: "laudo.escape", scope: "laudo", group: "Laudo · Geral", label: "Cancelar (fechar localizar / sair do Foco)", defaultBinding: "Esc" },
];
