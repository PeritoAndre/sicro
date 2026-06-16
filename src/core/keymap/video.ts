/**
 * Atalhos customizáveis do módulo **Vídeo**. Escopo `video`.
 * Convenção: `group: "Vídeo · <subárea>"`, label PT-BR, `defaultBinding` na
 * forma canônica de `keymap.ts`.
 *
 * Estes atalhos são DISCRETOS (uma ação por toque) e só disparam enquanto a
 * aba "Reprodutor" do editor está visível. A navegação com as SETAS
 * esquerda/direita (toque = ±1 quadro · segurar = reproduz à frente / em ré ·
 * Shift = ±1 s) NÃO entra no catálogo: é um gesto com estado de
 * pressionar/segurar/soltar (keydown + keyup + temporizador) que o modelo
 * customizável (só keydown) não consegue representar — fica fixa no player.
 */
import type { ShortcutAction } from "../keymapActions";

export const VIDEO_ACTIONS: ShortcutAction[] = [
  // Reprodução / transporte.
  { id: "video.playPause", scope: "video", group: "Vídeo · Reprodução", label: "Reproduzir / pausar", defaultBinding: "Space" },
  { id: "video.playPauseK", scope: "video", group: "Vídeo · Reprodução", label: "Reproduzir / pausar (alternativo)", defaultBinding: "K" },
  { id: "video.reverse", scope: "video", group: "Vídeo · Reprodução", label: "Reproduzir em ré", defaultBinding: "J" },
  { id: "video.forward", scope: "video", group: "Vídeo · Reprodução", label: "Reproduzir à frente", defaultBinding: "L" },

  // Navegação por quadro / posição.
  { id: "video.prevFrame", scope: "video", group: "Vídeo · Navegação", label: "Quadro anterior", defaultBinding: "," },
  { id: "video.nextFrame", scope: "video", group: "Vídeo · Navegação", label: "Próximo quadro", defaultBinding: "." },
  { id: "video.seekStart", scope: "video", group: "Vídeo · Navegação", label: "Ir para o início", defaultBinding: "Home" },
  { id: "video.seekEnd", scope: "video", group: "Vídeo · Navegação", label: "Ir para o fim", defaultBinding: "End" },

  // Velocidade de reprodução.
  { id: "video.speedUp", scope: "video", group: "Vídeo · Velocidade", label: "Aumentar velocidade", defaultBinding: "Up" },
  { id: "video.speedDown", scope: "video", group: "Vídeo · Velocidade", label: "Diminuir velocidade", defaultBinding: "Down" },

  // Captura.
  { id: "video.collectFrame", scope: "video", group: "Vídeo · Captura", label: "Coletar quadro (storyboard)", defaultBinding: "Ctrl+1" },
];
