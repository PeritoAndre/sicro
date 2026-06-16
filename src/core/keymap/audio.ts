/**
 * Atalhos customizáveis do módulo **Áudio / Degravação**. Escopo `audio`.
 * Convenção: `group: "Áudio · <subárea>"`, label PT-BR, `defaultBinding` na
 * forma canônica de `keymap.ts`.
 *
 * IMPORTANTE: a tela de degravação é cheia de campos de texto (locutor +
 * transcrição de cada trecho). Os atalhos do PEDAL usam Ctrl de propósito,
 * para conviver com a digitação — eles disparam MESMO com o cursor dentro de
 * um campo (o wiring usa `allowInInputs: true`). Por isso todos usam Ctrl.
 */
import type { ShortcutAction } from "../keymapActions";

export const AUDIO_ACTIONS: ShortcutAction[] = [
  // Pedal de transporte (valem enquanto degrava, mesmo digitando).
  { id: "audio.playPause", scope: "audio", group: "Áudio · Reprodução", label: "Reproduzir / pausar", defaultBinding: "Ctrl+Space" },
  { id: "audio.back3s", scope: "audio", group: "Áudio · Reprodução", label: "Recuar 3 s", defaultBinding: "Ctrl+Left" },
  { id: "audio.fwd3s", scope: "audio", group: "Áudio · Reprodução", label: "Avançar 3 s", defaultBinding: "Ctrl+Right" },

  // Degravação.
  { id: "audio.capture", scope: "audio", group: "Áudio · Degravação", label: "Capturar trecho no tempo atual", defaultBinding: "Ctrl+Enter" },
  { id: "audio.markEnd", scope: "audio", group: "Áudio · Degravação", label: "Definir fim do trecho atual no tempo corrente", defaultBinding: "Ctrl+Shift+E" },
  { id: "audio.save", scope: "audio", group: "Áudio · Degravação", label: "Salvar degravação", defaultBinding: "Ctrl+S" },
  { id: "audio.transcribeAI", scope: "audio", group: "Áudio · Degravação", label: "Gerar rascunho por IA (whisper)", defaultBinding: "Ctrl+Shift+T" },

  // Exportar / copiar.
  { id: "audio.copyTxt", scope: "audio", group: "Áudio · Exportar", label: "Copiar transcrição (texto)", defaultBinding: "Ctrl+Shift+C" },
  { id: "audio.copySrt", scope: "audio", group: "Áudio · Exportar", label: "Copiar transcrição (SRT)", defaultBinding: "Ctrl+Shift+S" },
];
