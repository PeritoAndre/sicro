/**
 * Vídeos e Áudios como abas irmãs: a ponte entre as duas telas.
 *
 * - Vídeos → Áudios: com um vídeo aberto, a aba Áudios recebe o áudio DESSE
 *   vídeo (o já extraído, ou extrai na hora), no mesmo instante.
 * - Áudios → Vídeos: com o áudio de um vídeo aberto, a aba Vídeos reabre o vídeo
 *   de origem no instante em que o áudio estava.
 *
 * Cada tela registra "o que está aberto agora"; a barra do topo, ao trocar de
 * aba, deixa um pedido (handoff) que a outra tela consome ao montar.
 */
import { audioStreamStart } from "@modules/video/editor/format";
import { savePosition } from "@modules/video/editor/resume";
import { useVideoStore } from "@modules/video/store/videoStore";

export interface OpenVideo {
  id: string;
  sha256: string;
  relativePath: string;
  filename: string;
  rawProbeJson: string | null;
  /** Instante do player (tempo do vídeo). */
  time: number;
}

export interface OpenAudio {
  sourceVideoSha256: string | null;
  /** Instante do player de áudio (tempo do WAV). */
  time: number;
}

/** Pedido para a aba Áudios: o áudio deste vídeo, neste instante do vídeo. */
export type AudioHandoff = OpenVideo;

/** Pedido para a aba Vídeos: o vídeo com este SHA, neste instante do áudio. */
export interface VideoHandoff {
  videoSha256: string;
  audioTime: number;
}

let currentVideo: (() => OpenVideo | null) | null = null;
let currentAudio: (() => OpenAudio | null) | null = null;
let audioHandoff: AudioHandoff | null = null;
let videoHandoff: VideoHandoff | null = null;

export function registerOpenVideo(fn: () => OpenVideo | null): () => void {
  currentVideo = fn;
  return () => {
    if (currentVideo === fn) currentVideo = null;
  };
}

export function registerOpenAudio(fn: () => OpenAudio | null): () => void {
  currentAudio = fn;
  return () => {
    if (currentAudio === fn) currentAudio = null;
  };
}

export function takeAudioHandoff(): AudioHandoff | null {
  const h = audioHandoff;
  audioHandoff = null;
  return h;
}

export function takeVideoHandoff(): VideoHandoff | null {
  const h = videoHandoff;
  videoHandoff = null;
  return h;
}

/** Tempo do WAV extraído ↔ tempo do vídeo (o WAV começa no início da trilha). */
export function videoToAudioTime(videoTime: number, rawProbeJson: string | null): number {
  return Math.max(0, videoTime - audioStreamStart(rawProbeJson));
}
export function audioToVideoTime(audioTime: number, rawProbeJson: string | null): number {
  return audioTime + audioStreamStart(rawProbeJson);
}

/**
 * Chamado pela barra do topo ANTES de trocar de aba. Vídeos → Áudios: deixa o
 * vídeo aberto como pedido. Áudios → Vídeos: deixa o vídeo de origem do áudio;
 * se ele já é o vídeo aberto, grava o instante na posição de retomada (o player
 * abre nele) — senão fecha o vídeo atual para a aba abrir o certo.
 */
export function prepareTabSwitch(to: "/video" | "/audio"): void {
  if (to === "/audio") {
    audioHandoff = currentVideo?.() ?? null;
    return;
  }
  const a = currentAudio?.();
  if (!a?.sourceVideoSha256) return;
  const store = useVideoStore.getState();
  const open = store.bundle?.media;
  if (open && open.sha256 === a.sourceVideoSha256) {
    savePosition(open.sha256, audioToVideoTime(a.time, open.raw_probe_json));
    return;
  }
  if (store.activeMediaId) store.closeMedia();
  videoHandoff = { videoSha256: a.sourceVideoSha256, audioTime: a.time };
}
