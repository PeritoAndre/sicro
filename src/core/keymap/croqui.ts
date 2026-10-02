/**
 * Atalhos do módulo Croqui. Objetos (veículo/vestígio/mobiliário/pessoa) têm UM
 * atalho por categoria, que ativa a ferramenta padrão; o subtipo se troca no popover.
 */
import type { ShortcutAction } from "../keymapActions";

export const CROQUI_ACTIONS: ShortcutAction[] = [
  // Ferramentas básicas / atômicas.
  { id: "croqui.tool.select", scope: "croqui", group: "Croqui · Ferramentas", label: "Selecionar / mover", defaultBinding: "V" },
  { id: "croqui.tool.pan", scope: "croqui", group: "Croqui · Ferramentas", label: "Mão (pan)", defaultBinding: "H" },
  { id: "croqui.tool.measure", scope: "croqui", group: "Croqui · Ferramentas", label: "Medida / cota", defaultBinding: "M" },
  { id: "croqui.tool.scale", scope: "croqui", group: "Croqui · Ferramentas", label: "Definir escala", defaultBinding: "E" },
  { id: "croqui.tool.text", scope: "croqui", group: "Croqui · Ferramentas", label: "Texto / etiqueta", defaultBinding: "T" },

  // Referencial (R1 / R2).
  { id: "croqui.tool.r1", scope: "croqui", group: "Croqui · Referencial", label: "Referencial R1", defaultBinding: "1" },
  { id: "croqui.tool.r2", scope: "croqui", group: "Croqui · Referencial", label: "Referencial R2", defaultBinding: "2" },

  // Vias (Road Engine).
  { id: "croqui.tool.roadUrban", scope: "croqui", group: "Croqui · Vias", label: "Via urbana", defaultBinding: "R" },
  { id: "croqui.tool.roadAvenue", scope: "croqui", group: "Croqui · Vias", label: "Avenida", defaultBinding: "A" },
  { id: "croqui.tool.roadHighway", scope: "croqui", group: "Croqui · Vias", label: "Rodovia", defaultBinding: "Y" },
  { id: "croqui.tool.roadDirt", scope: "croqui", group: "Croqui · Vias", label: "Estrada de terra", defaultBinding: "D" },
  { id: "croqui.tool.roadParking", scope: "croqui", group: "Croqui · Vias", label: "Estacionamento", defaultBinding: "P" },
  { id: "croqui.tool.roundabout", scope: "croqui", group: "Croqui · Vias", label: "Rotatória", defaultBinding: "O" },

  // Objetos.
  { id: "croqui.tool.vehicle", scope: "croqui", group: "Croqui · Objetos", label: "Veículo (sedan)", defaultBinding: "C" },
  { id: "croqui.tool.vestigio", scope: "croqui", group: "Croqui · Objetos", label: "Vestígio (ponto de colisão X)", defaultBinding: "X" },
  { id: "croqui.tool.mobiliario", scope: "croqui", group: "Croqui · Objetos", label: "Mobiliário urbano (semáforo)", defaultBinding: "U" },
  { id: "croqui.tool.pessoa", scope: "croqui", group: "Croqui · Objetos", label: "Pessoa (pedestre)", defaultBinding: "B" },

  // Anotações (setas / chamadas / trajetória).
  { id: "croqui.tool.arrow", scope: "croqui", group: "Croqui · Anotações", label: "Seta direcional", defaultBinding: "Shift+A" },
  { id: "croqui.tool.callout", scope: "croqui", group: "Croqui · Anotações", label: "Chamada (callout)", defaultBinding: "Shift+C" },
  { id: "croqui.tool.trajectory", scope: "croqui", group: "Croqui · Anotações", label: "Trajetória", defaultBinding: "Shift+T" },

  // Edição.
  { id: "croqui.delete", scope: "croqui", group: "Croqui · Edição", label: "Excluir selecionado(s)", defaultBinding: "Del" },
  { id: "croqui.duplicate", scope: "croqui", group: "Croqui · Edição", label: "Duplicar", defaultBinding: "Ctrl+D" },
  { id: "croqui.undo", scope: "croqui", group: "Croqui · Edição", label: "Desfazer", defaultBinding: "Ctrl+Z" },
  { id: "croqui.redo", scope: "croqui", group: "Croqui · Edição", label: "Refazer", defaultBinding: "Ctrl+Y" },
  { id: "croqui.save", scope: "croqui", group: "Croqui · Edição", label: "Salvar croqui", defaultBinding: "Ctrl+S" },

  // Vista.
  { id: "croqui.zoomIn", scope: "croqui", group: "Croqui · Vista", label: "Ampliar", defaultBinding: "Ctrl+=" },
  { id: "croqui.zoomOut", scope: "croqui", group: "Croqui · Vista", label: "Reduzir", defaultBinding: "Ctrl+-" },
  { id: "croqui.zoomReset", scope: "croqui", group: "Croqui · Vista", label: "Zoom 100% / centralizar", defaultBinding: "Ctrl+0" },
  { id: "croqui.fit", scope: "croqui", group: "Croqui · Vista", label: "Enquadrar a prancha", defaultBinding: "F" },
  { id: "croqui.toggleGrid", scope: "croqui", group: "Croqui · Vista", label: "Mostrar / ocultar grade", defaultBinding: "Shift+G" },

  // Imagem de fundo.
  { id: "croqui.bg.import", scope: "croqui", group: "Croqui · Imagem de fundo", label: "Importar imagem de fundo", defaultBinding: "Ctrl+Shift+B" },
  { id: "croqui.bg.toggleLock", scope: "croqui", group: "Croqui · Imagem de fundo", label: "Bloquear / desbloquear fundo", defaultBinding: "L" },
  { id: "croqui.bg.fit", scope: "croqui", group: "Croqui · Imagem de fundo", label: "Ajustar fundo à área útil", defaultBinding: "Shift+F" },
  { id: "croqui.importDrone", scope: "croqui", group: "Croqui · Imagem de fundo", label: "Importar imagem de drone", defaultBinding: "Ctrl+Shift+D" },
  { id: "croqui.importOsm", scope: "croqui", group: "Croqui · Imagem de fundo", label: "Importar vias do OSM", defaultBinding: "Ctrl+Shift+O" },

  // Exportação / saída.
  { id: "croqui.exportPng", scope: "croqui", group: "Croqui · Exportação", label: "Exportar PNG técnico", defaultBinding: "Ctrl+E" },
  { id: "croqui.exportPngClean", scope: "croqui", group: "Croqui · Exportação", label: "Exportar PNG limpo", defaultBinding: "Ctrl+Shift+E" },

  // Geral.
  { id: "croqui.cancel", scope: "croqui", group: "Croqui · Geral", label: "Cancelar ferramenta / seleção", defaultBinding: "Esc" },
];
