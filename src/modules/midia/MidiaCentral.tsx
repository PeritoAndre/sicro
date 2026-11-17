/** Central de Vídeo e Áudio: lâminas Vídeo, Áudio e Edição, e uma lista só com vídeos, áudios e edições do caso (§8.1). */

import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Copy, Scissors, TriangleAlert } from "lucide-react";
import { Central, CentralMini, type CentralItem, type CentralLamina } from "@components/Central/Central";
import { askConfirm } from "@components/Dialog/ask";
import type { ResumoProjeto } from "@domain/edicao";
import { abrirEdicao, duplicarEdicao, excluirEdicao, fraseDoProblema, listarEdicoes, novaEdicao, resumoDaEdicao } from "@modules/edicao/central";
import { separarErro } from "@modules/edicao/store/edicaoStore";
import { confirmarExclusaoVideo } from "@modules/video/excluirVideo";
import { EXT_GRAVADOR, copiaDeGravador, registrarVideo } from "@modules/video/gravador";
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
import edicaoImg from "@modules/video/assets/central/edicao.webp";
import thumbStyles from "@components/Central/Central.module.css";

const VIDEO_EXT = ["mp4", "mov", "mkv", "avi", "webm", "m4v", ...EXT_GRAVADOR];
const AUDIO_EXT = ["opus", "mp3", "m4a", "wav", "amr", "aac", "ogg", "flac", "wma"];

export function MidiaCentral() {
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const videos = useVideoStore((s) => s.list);
  const loadingVideos = useVideoStore((s) => s.isLoadingList);
  const loadList = useVideoStore((s) => s.loadList);
  const openMedia = useVideoStore((s) => s.openMedia);
  const deleteMedia = useVideoStore((s) => s.deleteMedia);
  const mostrarEdicao = useVideoStore((s) => s.mostrarEdicao);
  // a lista fica montada atrás da Edição: ao voltar, relê as edições
  const edicaoNaFrente = useVideoStore((s) => s.edicao);
  const setEmCentral = useMidiaCentral((s) => s.setEmCentral);
  const pedirAudio = useMidiaCentral((s) => s.pedirAudio);

  const [audios, setAudios] = useState<AudioMedia[]>([]);
  const [loadingAudios, setLoadingAudios] = useState(true);
  const [edicoes, setEdicoes] = useState<ResumoProjeto[]>([]);
  const [semMiniatura, setSemMiniatura] = useState<Record<string, true>>({});
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

  const lerEdicoes = useCallback(() => {
    if (!workspacePath) return;
    void listarEdicoes(workspacePath)
      .then(setEdicoes)
      .catch(() => setEdicoes([]));
  }, [workspacePath]);
  useEffect(() => {
    if (!edicaoNaFrente) lerEdicoes();
  }, [lerEdicoes, edicaoNaFrente]);

  /** Leva à Edição (no módulo Vídeo) depois de `f` abrir ou criar o projeto. */
  const irParaEdicao = async (f: () => Promise<unknown>) => {
    if (!workspacePath || busy) return;
    setBusy(true);
    setError(null);
    try {
      await f();
      mostrarEdicao(true);
      if (pathname !== "/video") navigate("/video");
    } catch (err) {
      setError(separarErro(err).frase);
    } finally {
      setBusy(false);
    }
  };

  const duplicar = async (id: string) => {
    if (!workspacePath) return;
    setError(null);
    try {
      await duplicarEdicao(workspacePath, id);
    } catch (err) {
      setError(separarErro(err).frase);
    }
    lerEdicoes();
  };

  const excluirEd = async (p: ResumoProjeto) => {
    if (!workspacePath) return;
    if (!(await askConfirm({ title: "Excluir edição", message: p.titulo, confirmLabel: "Excluir edição", danger: true }))) return;
    setError(null);
    try {
      await excluirEdicao(workspacePath, p.id);
    } catch (err) {
      setError(separarErro(err).frase);
    }
    lerEdicoes();
  };

  const abrirVideo = async (id: string) => {
    if (!workspacePath) return;
    setBusy(true);
    setError(null);
    try {
      await openMedia(workspacePath, id);
      mostrarEdicao(false);
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
      const media = await registrarVideo(workspacePath, selected);
      await openMedia(workspacePath, media.id);
      mostrarEdicao(false);
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
    const v = videos.find((m) => m.id === id);
    if (!v || !(await confirmarExclusaoVideo(workspacePath, v.sha256))) return;
    try {
      await deleteMedia(workspacePath, id);
    } catch (err) {
      setError(toSicroError(err).message);
    }
  };

  // escondida atrás da Edição a lista não responde ao teclado
  useShortcuts({ "video.addVideo": () => void adicionarVideo() }, { enabled: pathname === "/video" && !edicaoNaFrente });

  const laminas: CentralLamina[] = [
    { id: "video", nome: "Vídeo", arte: videoImg, onClick: () => void adicionarVideo() },
    { id: "audio", nome: "Áudio", arte: audioImg, onClick: () => void adicionarAudio() },
    { id: "edicao", nome: "Edição", rotulo: "Nova edição", arte: edicaoImg, onClick: () => workspacePath && void irParaEdicao(() => novaEdicao(workspacePath)) },
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
        tipo: copiaDeGravador(m.derivation_json) ? "cópia de trabalho" : "vídeo",
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
    const das_edicoes = edicoes.map((p) => ({
      quando: p.updated_at,
      item: {
        id: `e-${p.id}`,
        titulo: p.titulo,
        miniatura: (
          <CentralMini>
            {p.problema ? (
              <TriangleAlert size={18} aria-label={fraseDoProblema(p.problema)} data-problema={p.problema}>
                <title>{fraseDoProblema(p.problema)}</title>
              </TriangleAlert>
            ) : p.miniatura_rel && workspacePath && !semMiniatura[p.id] ? (
              <img src={convertFileSrc(`${workspacePath}/${p.miniatura_rel}`)} alt="" onError={() => setSemMiniatura((m) => ({ ...m, [p.id]: true }))} />
            ) : (
              <Scissors size={18} aria-hidden />
            )}
          </CentralMini>
        ),
        tipo: "edição",
        resumo: p.problema ? "" : resumoDaEdicao(p),
        quando: p.updated_at,
        onAbrir: () => workspacePath && void irParaEdicao(() => abrirEdicao(workspacePath, p.id)),
        acoes: p.problema ? [] : [{ rotulo: "Duplicar", icone: <Copy size={14} />, onClick: () => void duplicar(p.id) }],
        onExcluir: () => void excluirEd(p),
        excluirRotulo: "Excluir edição",
      } satisfies CentralItem,
    }));
    return [...dos_videos, ...dos_audios, ...das_edicoes]
      .sort((x, y) => Date.parse(y.quando) - Date.parse(x.quando))
      .map((x) => x.item);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [videos, audios, edicoes, semMiniatura, workspacePath, pathname]);

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

