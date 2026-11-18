/**
 * Atalhos só da Edição. Transporte, navegação, lupa e linha do tempo reaproveitam os
 * ids do Vídeo (quem remapeou lá não remapeia de novo): estão em `VIDEO_NA_EDICAO`.
 */
import type { ShortcutAction } from "../keymapActions";

const G = "Edição";

export const EDICAO_ACTIONS: ShortcutAction[] = [
  { id: "edicao.clipeAnterior", scope: "edicao", group: G, label: "Clipe anterior", defaultBinding: "PgUp" },
  { id: "edicao.proximoClipe", scope: "edicao", group: G, label: "Próximo clipe", defaultBinding: "PgDn" },
  { id: "edicao.subir", scope: "edicao", group: G, label: "Mover o clipe para antes", defaultBinding: "Alt+PgUp" },
  { id: "edicao.descer", scope: "edicao", group: G, label: "Mover o clipe para depois", defaultBinding: "Alt+PgDn" },
  { id: "edicao.dividir", scope: "edicao", group: G, label: "Dividir no cursor", defaultBinding: "Ctrl+B" },
  { id: "edicao.congelar", scope: "edicao", group: G, label: "Congelar aqui", defaultBinding: "Ctrl+Shift+B" },
  { id: "edicao.remover", scope: "edicao", group: G, label: "Remover", defaultBinding: "Del" },
  { id: "edicao.duplicar", scope: "edicao", group: G, label: "Duplicar", defaultBinding: "Ctrl+D" },
  { id: "edicao.desfazer", scope: "edicao", group: G, label: "Desfazer", defaultBinding: "Ctrl+Z" },
  { id: "edicao.refazer", scope: "edicao", group: G, label: "Refazer", defaultBinding: "Ctrl+Shift+Z" },
  { id: "edicao.refazerAlt", scope: "edicao", group: G, label: "Refazer", defaultBinding: "Ctrl+Y" },
  { id: "edicao.salvar", scope: "edicao", group: G, label: "Salvar", defaultBinding: "Ctrl+S" },
  { id: "edicao.exportar", scope: "edicao", group: G, label: "Exportar", defaultBinding: "Ctrl+E" },
  { id: "edicao.marcar", scope: "edicao", group: G, label: "Marcar neste quadro", defaultBinding: "Enter" },
  { id: "edicao.seguirFrente", scope: "edicao", group: G, label: "Seguir", defaultBinding: "Ctrl+Right" },
  { id: "edicao.seguirTras", scope: "edicao", group: G, label: "Seguir para trás", defaultBinding: "Ctrl+Left" },
  { id: "edicao.cancelar", scope: "edicao", group: G, label: "Parar", defaultBinding: "Esc" },
  { id: "edicao.trechoFraco", scope: "edicao", group: G, label: "Próximo trecho fraco", defaultBinding: "N" },
  { id: "edicao.quadroFinal", scope: "edicao", group: G, label: "Quadro final", defaultBinding: "Q" },
  { id: "edicao.marcador", scope: "edicao", group: G, label: "Marcador", defaultBinding: "M" },
  // Fase 4 (§8.5; contratos-fase4.md (i)): ferramentas da linha do tempo; o zoom dela reaproveita video.timelineZoom*
  { id: "edicao.ima", scope: "edicao", group: G, label: "Ímã", defaultBinding: "S" },
  { id: "edicao.selecionar", scope: "edicao", group: G, label: "Selecionar", defaultBinding: "V" },
  { id: "edicao.lamina", scope: "edicao", group: G, label: "Lâmina", defaultBinding: "B" },
];

/** Ações do Vídeo que também valem na tela da Edição (§8.5; o trilho de modos vale nas duas, §8.1). */
export const VIDEO_NA_EDICAO: readonly string[] = [
  "video.playPause",
  "video.playPauseK",
  "video.reverse",
  "video.forward",
  "video.prevFrame",
  "video.nextFrame",
  "video.seekStart",
  "video.seekEnd",
  "video.gotoTime",
  "video.markIn",
  "video.markOut",
  "video.magnifyIn",
  "video.magnifyOut",
  "video.magnifyReset",
  "video.timelineZoomIn",
  "video.timelineZoomOut",
  "video.timelineZoomFit",
  "video.mute",
  "video.fullscreen",
  "video.mode.assistir",
  "video.mode.velocidade",
  "video.mode.distancia",
  "video.mode.pessoas",
  "video.mode.realce",
  "video.mode.edicao",
];
