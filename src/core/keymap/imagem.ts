/**
 * Atalhos customizáveis do módulo **Imagem** (editor de imagem pericial).
 * Escopo `imagem`. Convenção: `group: "Imagem · <subárea>"`, label PT-BR,
 * `defaultBinding` na forma canônica de `keymap.ts`.
 *
 * Cobre ferramentas (seleção/pan + anotações + medição/escala + tarja +
 * corte), edição (excluir/salvar), vista (zoom/enquadrar) e exportação.
 */
import type { ShortcutAction } from "../keymapActions";

export const IMAGEM_ACTIONS: ShortcutAction[] = [
  // Ferramentas.
  { id: "imagem.tool.select", scope: "imagem", group: "Imagem · Ferramentas", label: "Selecionar / mover", defaultBinding: "V" },
  { id: "imagem.tool.pan", scope: "imagem", group: "Imagem · Ferramentas", label: "Mão (pan)", defaultBinding: "H" },
  { id: "imagem.tool.arrow", scope: "imagem", group: "Imagem · Ferramentas", label: "Seta", defaultBinding: "A" },
  { id: "imagem.tool.line", scope: "imagem", group: "Imagem · Ferramentas", label: "Linha", defaultBinding: "L" },
  { id: "imagem.tool.rect", scope: "imagem", group: "Imagem · Ferramentas", label: "Retângulo", defaultBinding: "R" },
  { id: "imagem.tool.ellipse", scope: "imagem", group: "Imagem · Ferramentas", label: "Elipse", defaultBinding: "E" },
  { id: "imagem.tool.text", scope: "imagem", group: "Imagem · Ferramentas", label: "Texto", defaultBinding: "T" },
  { id: "imagem.tool.marker", scope: "imagem", group: "Imagem · Ferramentas", label: "Marcador numerado", defaultBinding: "N" },
  { id: "imagem.tool.point", scope: "imagem", group: "Imagem · Ferramentas", label: "Ponto", defaultBinding: "P" },
  { id: "imagem.tool.measurement", scope: "imagem", group: "Imagem · Ferramentas", label: "Medida", defaultBinding: "M" },
  { id: "imagem.tool.scale", scope: "imagem", group: "Imagem · Ferramentas", label: "Definir escala", defaultBinding: "K" },
  { id: "imagem.tool.redaction", scope: "imagem", group: "Imagem · Ferramentas", label: "Tarja (redação)", defaultBinding: "X" },
  { id: "imagem.tool.crop", scope: "imagem", group: "Imagem · Ferramentas", label: "Cortar imagem", defaultBinding: "C" },

  // Seleção (W20) — região estilo Photoshop (marquee + inverter).
  { id: "imagem.tool.select_rect", scope: "imagem", group: "Imagem · Seleção", label: "Seleção retangular", defaultBinding: "Shift+R" },
  { id: "imagem.tool.select_ellipse", scope: "imagem", group: "Imagem · Seleção", label: "Seleção elíptica", defaultBinding: "Shift+E" },
  { id: "imagem.tool.select_lasso", scope: "imagem", group: "Imagem · Seleção", label: "Laço (segue o mouse)", defaultBinding: "Shift+L" },
  { id: "imagem.tool.select_polygon", scope: "imagem", group: "Imagem · Seleção", label: "Poligonal (por cliques)", defaultBinding: "Shift+P" },
  { id: "imagem.tool.select_magnetic", scope: "imagem", group: "Imagem · Seleção", label: "Magnética (básica)", defaultBinding: "Shift+M" },
  { id: "imagem.selectInvert", scope: "imagem", group: "Imagem · Seleção", label: "Inverter seleção", defaultBinding: "Ctrl+Shift+I" },
  { id: "imagem.selectClear", scope: "imagem", group: "Imagem · Seleção", label: "Deselecionar", defaultBinding: "Ctrl+D" },
  { id: "imagem.selectAll", scope: "imagem", group: "Imagem · Seleção", label: "Selecionar tudo", defaultBinding: "Ctrl+A" },
  { id: "imagem.duplicateSelectionLayer", scope: "imagem", group: "Imagem · Seleção", label: "Nova camada da seleção", defaultBinding: "Ctrl+J" },

  // Edição.
  { id: "imagem.delete", scope: "imagem", group: "Imagem · Edição", label: "Excluir anotação selecionada", defaultBinding: "Del" },
  { id: "imagem.save", scope: "imagem", group: "Imagem · Edição", label: "Salvar análise", defaultBinding: "Ctrl+S" },
  { id: "imagem.cropApply", scope: "imagem", group: "Imagem · Edição", label: "Aplicar corte (modo corte)", defaultBinding: "Enter" },
  { id: "imagem.toggleAnnotations", scope: "imagem", group: "Imagem · Edição", label: "Mostrar / ocultar anotações", defaultBinding: "Shift+A" },

  // Vista.
  { id: "imagem.zoomIn", scope: "imagem", group: "Imagem · Vista", label: "Ampliar", defaultBinding: "Ctrl+=" },
  { id: "imagem.zoomOut", scope: "imagem", group: "Imagem · Vista", label: "Reduzir", defaultBinding: "Ctrl+-" },
  { id: "imagem.zoomActual", scope: "imagem", group: "Imagem · Vista", label: "Pixels reais (1:1)", defaultBinding: "Ctrl+0" },
  { id: "imagem.fit", scope: "imagem", group: "Imagem · Vista", label: "Enquadrar à tela", defaultBinding: "F" },

  // Exportação.
  { id: "imagem.export", scope: "imagem", group: "Imagem · Exportação", label: "Exportar imagem", defaultBinding: "Ctrl+E" },

  // Geral.
  { id: "imagem.commandPalette", scope: "imagem", group: "Imagem · Geral", label: "Paleta de comandos", defaultBinding: "Ctrl+K" },
  { id: "imagem.cancel", scope: "imagem", group: "Imagem · Geral", label: "Cancelar ferramenta / seleção / corte", defaultBinding: "Esc" },
];
