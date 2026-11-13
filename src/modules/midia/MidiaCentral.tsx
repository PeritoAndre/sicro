/** Central de Vídeo e Áudio: duas lâminas e a lista de vídeos e áudios do caso. */

import { useEffect, useMemo, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { Central, type CentralItem, type CentralLamina } from "@components/Central/Central";
import { askConfirm } from "@components/Dialog/ask";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { useShortcuts } from "@core/useShortcuts";
import type { AudioMedia } from "@domain/audio";
import { selectActiveWorkspacePath, useWorkspaceStore } from "@stores/workspaceStore";
import { useVideoStore } from "@modules/video/store/videoStore";
import { VideoThumb } from "@modules/video/VideoThumb";
import { formatDuration } from "@modules/video/editor/format";
import { useMidiaCentral } from "./midiaNav";
import videoImg from "@modules/video/assets/central/video.webp";
import audioImg from "@modules/video/assets/central/audio.webp";
import thumbStyles from "@components/Central/Central.module.css";

const VIDEO_EXT = ["mp4", "mov", "mkv", "avi", "webm", "m4v"];
const AUDIO_EXT = ["opus", "mp3", "m4a", "wav", "amr", "aac", "ogg", "flac", "wma"];

export function MidiaCentral() {
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const videos = useVideoStore((s) => s.list);
  const loadingVideos = useVideoStore((s) => s.isLoadingList);
  const loadList = useVideoStore((s) => s.loadList);
  const registerMedia = useVideoStore((s) => s.registerMedia);
  const openMedia = useVideoStore((s) => s.openMedia);
  const deleteMedia = useVideoStore((s) => s.deleteMedia);
  const setEmCentral = useMidiaCentral((s) => s.setEmCentral);
  const pedirAudio = useMidiaCentral((s) => s.pedirAudio);

  const [audios, setAudios] = useState<AudioMedia[]>([]);
  const [loadingAudios, setLoadingAudios] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setEmCentral(true);
    return () => setEmCentral(false);
  }, [setEmCentral]);

  useEffect(() => {
    if (!workspacePath) return;
    void loadList(workspacePath);
    setLoadingAudios(true);
    void commands
      .listAudioMedia(workspacePath)
      .then(setAudios)
      .catch(() => setAudios([]))
      .finally(() => setLoadingAudios(false));
  }, [workspacePath, loadList]);

  const abrirVideo = async (id: string) => {
    if (!workspacePath) return;
    setBusy(true);
    setError(null);
    try {
      await openMedia(workspacePath, id);
      if (pathname !== "/video") navigate("/video");
    } catch (err) {
      setError(toSicroError(err).message);
    } finally {
      setBusy(false);
    }
  };

  const abrirAudio = (id: string) => {
    pedirAudio(id);
    navigate("/audio");
  };

  const adicionarVideo = async () => {
    if (!workspacePath || busy) return;
    setError(null);
    try {
      const selected = await openFileDialog({
        multiple: false,
        title: "Selecione um vídeo",
        filters: [{ name: "Vídeos", extensions: VIDEO_EXT }],
      });
      if (typeof selected !== "string") return;
      setBusy(true);
      const media = await registerMedia(workspacePath, selected);
      await openMedia(workspacePath, media.id);
      if (pathname !== "/video") navigate("/video");
    } catch (err) {
      setError(toSicroError(err).message);
    } finally {
      setBusy(false);
    }
  };

  const adicionarAudio = async () => {
    if (!workspacePath || busy) return;
    setError(null);
    try {
      const picked = await openFileDialog({
        multiple: false,
        filters: [{ name: "Áudio", extensions: AUDIO_EXT }],
      });
      if (typeof picked !== "string") return;
      setBusy(true);
      const m = await commands.importAudioFile(workspacePath, picked);
      abrirAudio(m.id);
    } catch (err) {
      setError(toSicroError(err).message);
    } finally {
      setBusy(false);
    }
  };

  const excluirVideo = async (id: string) => {
    if (!workspacePath) return;
    const ok = await askConfirm({
      title: "Excluir vídeo",
      message: "Excluir este vídeo e tudo que foi marcado nele?",
      confirmLabel: "Excluir",
      danger: true,
    });
    if (!ok) return;
    try {
      await deleteMedia(workspacePath, id);
    } catch (err) {
      setError(toSicroError(err).message);
    }
  };

  useShortcuts({ "video.addVideo": () => void adicionarVideo() }, { enabled: pathname === "/video" });

  const laminas: CentralLamina[] = [
    { id: "video", nome: "Vídeo", arte: videoImg, onClick: () => void adicionarVideo() },
    { id: "audio", nome: "Áudio", arte: audioImg, onClick: () => void adicionarAudio() },
  ];

  const itens = useMemo<CentralItem[]>(() => {
    const dos_videos = videos.map((m) => ({
      quando: m.created_at,
      item: {
        id: `v-${m.id}`,
        titulo: m.filename,
        miniatura: (
          <VideoThumb
            workspacePath={workspacePath ?? ""}
            mediaId={m.id}
            className={thumbStyles.miniVazia}
          />
        ),
        tipo: "vídeo",
        resumo: [
          m.duration_s != null ? formatDuration(m.duration_s) : null,
          m.width && m.height ? `${m.width}×${m.height}` : null,
          m.fps_declared ? `${m.fps_declared.toFixed(2)} fps` : null,
        ]
          .filter(Boolean)
          .join(" · "),
        quando: m.created_at,
        onAbrir: () => void abrirVideo(m.id),
        onExcluir: () => void excluirVideo(m.id),
        excluirRotulo: "Excluir vídeo",
      } satisfies CentralItem,
    }));
    const dos_audios = audios.map((a) => ({
      quando: a.created_at,
      item: {
        id: `a-${a.id}`,
        titulo: a.filename,
        miniatura: <img src={audioImg} alt="" />,
        tipo: "áudio",
        resumo: a.duration_s != null ? formatDuration(a.duration_s) : "",
        quando: a.created_at,
        onAbrir: () => abrirAudio(a.id),
      } satisfies CentralItem,
    }));
    return [...dos_videos, ...dos_audios]
      .sort((x, y) => Date.parse(y.quando) - Date.parse(x.quando))
      .map((x) => x.item);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [videos, audios, workspacePath, pathname]);

  return (
    <Central
      titulo="Central de Vídeo e Áudio"
      laminas={laminas}
      itens={itens}
      carregando={loadingVideos || loadingAudios}
      busy={busy}
      erro={error}
    />
  );
}

