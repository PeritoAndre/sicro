/**
 * Ponte entre as abas irmãs Vídeos e Áudios: cada tela registra o que está
 * aberto; a barra do topo, ao trocar de aba, deixa um pedido (handoff) que a
 * outra consome ao montar — o áudio do vídeo aberto no mesmo instante, ou o
 * vídeo de origem do áudio.
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

interface OpenAudio {
  sourceVideoSha256: string | null;
  /** Instante do player de áudio (tempo do WAV). */
  time: number;
}

/** Pedido para a aba Áudios: o áudio deste vídeo, neste instante do vídeo. */
export type AudioHandoff = OpenVideo;

/** Pedido para a aba Vídeos: o vídeo com este SHA, neste instante do áudio. */
interface VideoHandoff {
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
 * Chamado pela barra do topo ANTES de trocar de aba. Áudios → Vídeos: se o vídeo
 * de origem já é o aberto, grava o instante na posição de retomada; senão fecha
 * o atual para a aba abrir o certo.
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
