/**
 * Atalhos do módulo Vídeo (só na aba "Reprodutor"). As setas ←/→ (toque/segurar/
 * Shift) ficam fixas no player: gesto com keyup + temporizador não cabe no modelo só-keydown.
 */
import type { ShortcutAction } from "../keymapActions";

export const VIDEO_ACTIONS: ShortcutAction[] = [
  // Reprodução / transporte.
  { id: "video.playPause", scope: "video", group: "Vídeo · Reprodução", label: "Reproduzir / pausar", defaultBinding: "Space" },
  { id: "video.playPauseK", scope: "video", group: "Vídeo · Reprodução", label: "Reproduzir / pausar (alternativo)", defaultBinding: "K" },
  { id: "video.reverse", scope: "video", group: "Vídeo · Reprodução", label: "Ré — repetir acelera (1× → 2× → 4× → 8×)", defaultBinding: "J" },
  { id: "video.forward", scope: "video", group: "Vídeo · Reprodução", label: "À frente — repetir acelera (1× → 2× → 4× → 8×)", defaultBinding: "L" },

  // Navegação por quadro / posição.
  { id: "video.prevFrame", scope: "video", group: "Vídeo · Navegação", label: "Quadro anterior", defaultBinding: "," },
  { id: "video.nextFrame", scope: "video", group: "Vídeo · Navegação", label: "Próximo quadro", defaultBinding: "." },
  { id: "video.seekStart", scope: "video", group: "Vídeo · Navegação", label: "Ir para o início", defaultBinding: "Home" },
  { id: "video.seekEnd", scope: "video", group: "Vídeo · Navegação", label: "Ir para o fim", defaultBinding: "End" },
  { id: "video.prevEvent", scope: "video", group: "Vídeo · Navegação", label: "Evento anterior", defaultBinding: "Shift+Up" },
  { id: "video.nextEvent", scope: "video", group: "Vídeo · Navegação", label: "Próximo evento", defaultBinding: "Shift+Down" },
  { id: "video.gotoTime", scope: "video", group: "Vídeo · Navegação", label: "Ir para tempo / quadro (digitar)", defaultBinding: "Ctrl+G" },

  // Trecho (entrada/saída) e repetição.
  { id: "video.markIn", scope: "video", group: "Vídeo · Trecho", label: "Marcar entrada do trecho", defaultBinding: "I" },
  { id: "video.markOut", scope: "video", group: "Vídeo · Trecho", label: "Marcar saída do trecho", defaultBinding: "O" },
  { id: "video.gotoIn", scope: "video", group: "Vídeo · Trecho", label: "Ir para a entrada", defaultBinding: "Shift+I" },
  { id: "video.gotoOut", scope: "video", group: "Vídeo · Trecho", label: "Ir para a saída", defaultBinding: "Shift+O" },
  { id: "video.toggleLoop", scope: "video", group: "Vídeo · Trecho", label: "Repetir o trecho (liga / desliga)", defaultBinding: "Ctrl+L" },
  { id: "video.clearInOut", scope: "video", group: "Vídeo · Trecho", label: "Limpar entrada e saída", defaultBinding: "Alt+X" },

  // Linha do tempo.
  { id: "video.timelineZoomIn", scope: "video", group: "Vídeo · Linha do tempo", label: "Aproximar a linha do tempo", defaultBinding: "=" },
  { id: "video.timelineZoomOut", scope: "video", group: "Vídeo · Linha do tempo", label: "Afastar a linha do tempo", defaultBinding: "-" },
  { id: "video.timelineZoomFit", scope: "video", group: "Vídeo · Linha do tempo", label: "Linha do tempo inteira", defaultBinding: "0" },

  // Velocidade de reprodução.
  { id: "video.speedUp", scope: "video", group: "Vídeo · Velocidade", label: "Aumentar velocidade", defaultBinding: "Up" },
  { id: "video.speedDown", scope: "video", group: "Vídeo · Velocidade", label: "Diminuir velocidade", defaultBinding: "Down" },

  // Tela.
  { id: "video.fullscreen", scope: "video", group: "Vídeo · Tela", label: "Tela cheia (entrar / sair)", defaultBinding: "F" },
  { id: "video.magnifyIn", scope: "video", group: "Vídeo · Tela", label: "Lupa: aproximar a imagem", defaultBinding: "Ctrl+=" },
  { id: "video.magnifyOut", scope: "video", group: "Vídeo · Tela", label: "Lupa: afastar a imagem", defaultBinding: "Ctrl+-" },
  { id: "video.magnifyReset", scope: "video", group: "Vídeo · Tela", label: "Lupa: imagem inteira", defaultBinding: "Ctrl+0" },
  { id: "video.adjustToggle", scope: "video", group: "Vídeo · Tela", label: "Ajustes de brilho/contraste: ligar / desligar (comparar)", defaultBinding: "A" },

  // Áudio.
  { id: "video.mute", scope: "video", group: "Vídeo · Áudio", label: "Mudo (liga / desliga)", defaultBinding: "Ctrl+M" },
  { id: "video.volumeUp", scope: "video", group: "Vídeo · Áudio", label: "Aumentar o volume", defaultBinding: "Ctrl+Up" },
  { id: "video.volumeDown", scope: "video", group: "Vídeo · Áudio", label: "Diminuir o volume", defaultBinding: "Ctrl+Down" },

  // Captura.
  { id: "video.collectFrame", scope: "video", group: "Vídeo · Captura", label: "Coletar quadro (storyboard)", defaultBinding: "Ctrl+1" },
  { id: "video.collectSequence", scope: "video", group: "Vídeo · Captura", label: "Coletar sequência de quadros…", defaultBinding: "Ctrl+2" },
  { id: "video.quickEvent", scope: "video", group: "Vídeo · Captura", label: "Marcar evento no tempo atual", defaultBinding: "M" },
  { id: "video.copyTime", scope: "video", group: "Vídeo · Captura", label: "Copiar o tempo (formato de laudo)", defaultBinding: "Ctrl+Shift+C" },

  { id: "video.prevVideo", scope: "video", group: "Vídeo · Vídeos do caso", label: "Vídeo anterior", defaultBinding: "Ctrl+PgUp" },
  { id: "video.nextVideo", scope: "video", group: "Vídeo · Vídeos do caso", label: "Próximo vídeo", defaultBinding: "Ctrl+PgDn" },
  { id: "video.addVideo", scope: "video", group: "Vídeo · Vídeos do caso", label: "Adicionar vídeos ao caso", defaultBinding: "Ctrl+O" },
];
