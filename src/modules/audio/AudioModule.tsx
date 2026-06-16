/**
 * AudioModule — módulo Áudio (Camada 1): importar/extrair, listar e tocar.
 *
 * Determinístico e com cadeia de custódia (o backend hasheia e registra tudo).
 * Sem transcrição/IA ainda — isso é Camada 2.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  Activity,
  AlertTriangle,
  Captions,
  FileAudio,
  Film,
  FolderOpen,
  Headphones,
  Import,
  ListMusic,
  ListPlus,
  RefreshCw,
  Scissors,
  ShieldCheck,
  SlidersHorizontal,
  type LucideIcon,
} from "lucide-react";
import { Button } from "@components/Button/Button";
import { EmptyState } from "@components/EmptyState/EmptyState";
import { NoOccurrenceState } from "@components/NoOccurrenceState/NoOccurrenceState";
import {
  ModuleLanding,
  type ModuleLandingFeature,
} from "@components/ModuleLanding/ModuleLanding";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import {
  selectActiveWorkspacePath,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import type { AudioMedia } from "@domain/audio";
import type { VideoMedia } from "@domain/video";
import { AudioPlayer, fmtTime, type AudioPlayerHandle } from "./AudioPlayer";
import { AudioAnalysisPanel } from "./AudioAnalysisPanel";
import styles from "./AudioModule.module.css";

const AUDIO_EXT = ["opus", "mp3", "m4a", "wav", "amr", "aac", "ogg", "flac", "wma"];
const VIDEO_EXT = ["mp4", "mov", "avi", "mkv", "webm", "m4v", "mpg", "mpeg", "ts"];

/** Filtros de realce — auxílio de escuta, todos padrão e determinísticos. */
const REALCE_FILTERS: { key: string; label: string; hint: string }[] = [
  { key: "denoise", label: "Reduzir ruído", hint: "afftdn — ruído de banda larga (chiado/ventilador)" },
  { key: "highpass", label: "Cortar graves (<80 Hz)", hint: "highpass — remove ronco/rumble de fundo" },
  { key: "lowpass", label: "Cortar agudos (>8 kHz)", hint: "lowpass — atenua sibilância/chiado agudo" },
  { key: "normalize", label: "Normalizar volume", hint: "dynaudnorm — equaliza trechos baixos e altos" },
];
const REALCE_KEYS = REALCE_FILTERS.map((f) => f.key);

// W16 — abas do detalhe por INTENÇÃO (o player fica sempre visível acima).
// Substitui a rolagem única onde tudo ficava empilhado.
type DetailTab = "realcar" | "analisar" | "trechos" | "ficha";
const DETAIL_TABS: {
  key: DetailTab;
  label: string;
  hint: string;
  icon: LucideIcon;
}[] = [
  {
    key: "realcar",
    label: "Realçar",
    hint: "Filtros de escuta (não-destrutivos): ruído, graves, agudos, volume.",
    icon: SlidersHorizontal,
  },
  {
    key: "analisar",
    label: "Analisar",
    hint: "Espectrograma + medições objetivas (pico/RMS/clipping, espectro FFT, ENF).",
    icon: Activity,
  },
  {
    key: "trechos",
    label: "Trechos",
    hint: "Recortar trecho (A-B) e montar compilação rotulada.",
    icon: Scissors,
  },
  {
    key: "ficha",
    label: "Ficha",
    hint: "Metadados técnicos, hashes e cadeia de custódia.",
    icon: FileAudio,
  },
];

function kindLabel(kind: string): string {
  if (kind === "extraido") return "extraído de vídeo";
  if (kind === "realce") return "realce";
  if (kind === "recorte") return "trecho recortado";
  if (kind === "compilacao") return "compilação rotulada";
  return "importado";
}
function kindChipLabel(kind: string): string {
  if (kind === "extraido") return "extraído";
  if (kind === "realce") return "realce";
  if (kind === "recorte") return "trecho";
  if (kind === "compilacao") return "compilação";
  return "importado";
}

type CompileItem = {
  audioId: string;
  audioName: string;
  start: number;
  end: number;
  label: string;
};

const AUDIO_FEATURES: ModuleLandingFeature[] = [
  {
    icon: <ShieldCheck size={18} />,
    title: "Integridade e custódia",
    desc: "Original preservado + WAV de análise (PCM 16-bit), ambos com hash.",
  },
  {
    icon: <SlidersHorizontal size={18} />,
    title: "Realce para escuta",
    desc: "Reduzir ruído, cortar graves/agudos, normalizar — não-destrutivo.",
  },
  {
    icon: <Activity size={18} />,
    title: "Espectrograma e medições",
    desc: "Tempo × frequência, pico/RMS/clipping e ENF — visualização objetiva.",
  },
  {
    icon: <Captions size={18} />,
    title: "Trechos e degravação",
    desc: "Recorte A-B, compilação rotulada e degravação assistida.",
  },
];

function prettyBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function channelsLabel(n: number | null): string {
  if (n === 1) return "mono";
  if (n === 2) return "estéreo";
  if (n && n > 2) return `${n} canais`;
  return "—";
}

function parseWarnings(json: string): string[] {
  try {
    const v = JSON.parse(json);
    return Array.isArray(v) ? (v as string[]) : [];
  } catch {
    return [];
  }
}

export function AudioModule() {
  const ws = useWorkspaceStore(selectActiveWorkspacePath);
  const navigate = useNavigate();
  const [items, setItems] = useState<AudioMedia[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [caseVideos, setCaseVideos] = useState<VideoMedia[]>([]);
  const [pickedVideo, setPickedVideo] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState<
    null | "import" | "extract" | "enhance" | "clip"
  >(null);
  const [error, setError] = useState<string | null>(null);
  const playerRef = useRef<AudioPlayerHandle>(null);
  const [compileList, setCompileList] = useState<CompileItem[]>([]);
  const [compiling, setCompiling] = useState(false);
  const [realceSel, setRealceSel] = useState<Record<string, boolean>>({
    denoise: true,
    highpass: true,
    lowpass: false,
    normalize: false,
  });
  const [spectro, setSpectro] = useState<{
    id: string;
    url: string;
    rel: string;
  } | null>(null);
  const [spectroBusy, setSpectroBusy] = useState(false);
  // W16 — aba ativa do detalhe (Realçar/Analisar/Trechos/Ficha).
  const [detailTab, setDetailTab] = useState<DetailTab>("realcar");

  const reload = useCallback(async () => {
    if (!ws) return;
    setLoading(true);
    setError(null);
    try {
      const list = await commands.listAudioMedia(ws);
      setItems(list);
      setSelectedId((cur) =>
        cur && list.some((i) => i.id === cur) ? cur : (list[0]?.id ?? null),
      );
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setLoading(false);
    }
  }, [ws]);

  useEffect(() => {
    void reload();
  }, [reload]);

  // Vídeos do caso (para "extrair do caso atual").
  useEffect(() => {
    if (!ws) return;
    void commands
      .listVideoMedia(ws)
      .then((v) => setCaseVideos(v))
      .catch(() => setCaseVideos([]));
  }, [ws]);

  const handleImport = async () => {
    if (!ws) return;
    const picked = await openFileDialog({
      multiple: false,
      filters: [{ name: "Áudio", extensions: AUDIO_EXT }],
    });
    if (typeof picked !== "string") return;
    setBusy("import");
    setError(null);
    try {
      const m = await commands.importAudioFile(ws, picked);
      await reload();
      setSelectedId(m.id);
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
    }
  };

  const handleExtractExternal = async () => {
    if (!ws) return;
    const picked = await openFileDialog({
      multiple: false,
      filters: [{ name: "Vídeo", extensions: VIDEO_EXT }],
    });
    if (typeof picked !== "string") return;
    await runExtract(picked, null);
  };

  const handleExtractCase = async () => {
    if (!ws || !pickedVideo) return;
    const video = caseVideos.find((v) => v.id === pickedVideo);
    if (!video) return;
    await runExtract(`${ws}/${video.relative_path}`, video.sha256);
  };

  const runExtract = async (videoPath: string, sha: string | null) => {
    if (!ws) return;
    setBusy("extract");
    setError(null);
    try {
      const m = await commands.extractAudioFromVideo(ws, videoPath, sha);
      await reload();
      setSelectedId(m.id);
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
    }
  };

  const selected = useMemo(
    () => items.find((i) => i.id === selectedId) ?? null,
    [items, selectedId],
  );
  const fileUrl = useMemo(
    () => (ws && selected ? convertFileSrc(`${ws}/${selected.relative_path}`) : null),
    [ws, selected],
  );
  const warnings = useMemo(
    () => (selected ? parseWarnings(selected.warnings_json) : []),
    [selected],
  );

  const handleEnhance = async () => {
    if (!ws || !selected) return;
    const filters = REALCE_KEYS.filter((k) => realceSel[k]);
    if (filters.length === 0) {
      setError("Selecione ao menos um filtro de realce.");
      return;
    }
    setBusy("enhance");
    setError(null);
    try {
      const m = await commands.enhanceAudio(ws, selected.id, filters);
      await reload();
      setSelectedId(m.id);
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
    }
  };

  const handleExtractClip = async () => {
    if (!ws || !selected) return;
    const loop = playerRef.current?.getLoop();
    if (!loop) {
      setError(
        "Defina o trecho com os botões A e B no player antes de extrair.",
      );
      return;
    }
    setBusy("clip");
    setError(null);
    try {
      const m = await commands.extractAudioClip(ws, selected.id, loop.a, loop.b);
      await reload();
      setSelectedId(m.id);
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
    }
  };

  const addToCompilation = () => {
    if (!selected) return;
    const loop = playerRef.current?.getLoop();
    if (!loop) {
      setError(
        "Defina o trecho com os botões A e B no player antes de adicionar.",
      );
      return;
    }
    setError(null);
    setCompileList((list) => [
      ...list,
      {
        audioId: selected.id,
        audioName: selected.filename,
        start: loop.a,
        end: loop.b,
        label: "",
      },
    ]);
  };

  const removeCompileItem = (idx: number) =>
    setCompileList((l) => l.filter((_, i) => i !== idx));

  const moveCompileItem = (idx: number, dir: -1 | 1) =>
    setCompileList((l) => {
      const j = idx + dir;
      if (j < 0 || j >= l.length) return l;
      const copy = [...l];
      const a = copy[idx];
      const b = copy[j];
      if (!a || !b) return l;
      copy[idx] = b;
      copy[j] = a;
      return copy;
    });

  const setCompileLabel = (idx: number, label: string) =>
    setCompileList((l) => l.map((c, i) => (i === idx ? { ...c, label } : c)));

  const handleCompile = async () => {
    if (!ws || compileList.length < 2) {
      setError("Adicione pelo menos 2 trechos à compilação.");
      return;
    }
    setCompiling(true);
    setError(null);
    try {
      const segments = compileList.map((c) => ({
        audio_id: c.audioId,
        start_s: c.start,
        end_s: c.end,
        label: c.label,
      }));
      const m = await commands.compileAudioClips(ws, segments);
      setCompileList([]);
      await reload();
      setSelectedId(m.id);
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setCompiling(false);
    }
  };

  const genSpectrogram = async () => {
    if (!ws || !selected) return;
    setSpectroBusy(true);
    setError(null);
    try {
      const rel = await commands.audioSpectrogram(ws, selected.id);
      setSpectro({ id: selected.id, url: convertFileSrc(`${ws}/${rel}`), rel });
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setSpectroBusy(false);
    }
  };

  if (!ws) {
    return (
      <NoOccurrenceState
        icon={<Headphones size={36} strokeWidth={1.5} />}
        moduleName="Áudio"
      />
    );
  }

  if (!loading && items.length === 0) {
    return (
      <div className={styles.wrap}>
        <ModuleLanding
          icon={<Headphones size={44} strokeWidth={1.2} />}
          title="Áudio — Análise Pericial"
          subtitle="Importe áudios (WhatsApp, gravador…) ou extraia de vídeos. O original é preservado e um WAV de análise é gerado, ambos com hash. Realce de escuta, espectrograma, recorte e degravação."
          actions={
            <>
              <Button
                variant="primary"
                leftIcon={<Import size={15} />}
                onClick={() => void handleImport()}
                disabled={busy !== null}
              >
                {busy === "import" ? "Importando…" : "Importar áudio"}
              </Button>
              <Button
                variant="secondary"
                leftIcon={<Film size={15} />}
                onClick={() => void handleExtractExternal()}
                disabled={busy !== null}
              >
                {busy === "extract" ? "Extraindo…" : "Extrair de vídeo…"}
              </Button>
            </>
          }
          features={AUDIO_FEATURES}
          note="Realce, espectrograma e medições são apoio técnico (FFmpeg, determinístico). Não recuperam nem alteram o conteúdo; a interpretação cabe ao perito responsável."
        >
          {caseVideos.length > 0 && (
            <div className={styles.caseExtract}>
              <span className={styles.caseExtractLabel}>
                <Film size={12} aria-hidden /> Extrair do vídeo do caso:
              </span>
              <select
                className={styles.select}
                value={pickedVideo}
                onChange={(e) => setPickedVideo(e.target.value)}
              >
                <option value="">selecione um vídeo…</option>
                {caseVideos.map((v) => (
                  <option key={v.id} value={v.id}>
                    {v.filename}
                  </option>
                ))}
              </select>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => void handleExtractCase()}
                disabled={busy !== null || !pickedVideo}
              >
                Extrair
              </Button>
            </div>
          )}
        </ModuleLanding>
        {error && (
          <div
            className={styles.errorBanner}
            style={{ margin: "var(--space-3)" }}
          >
            <AlertTriangle size={14} /> {error}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className={styles.wrap}>
      <header className={styles.topBar}>
        <div className={styles.title}>
          <h1>
            <Headphones size={16} aria-hidden /> Áudio
          </h1>
          <p className={styles.subtitle}>
            Importe, extraia de vídeos e analise áudios — com hash e cadeia de
            custódia. Transcrição assistida vem na próxima etapa.
          </p>
        </div>
        <div className={styles.headActions}>
          <Button
            variant="secondary"
            size="sm"
            leftIcon={<Import size={14} />}
            onClick={() => void handleImport()}
            disabled={busy !== null}
          >
            {busy === "import" ? "Importando…" : "Importar áudio"}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            leftIcon={<Film size={14} />}
            onClick={() => void handleExtractExternal()}
            disabled={busy !== null}
          >
            {busy === "extract" ? "Extraindo…" : "Extrair de vídeo…"}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            leftIcon={<RefreshCw size={13} />}
            onClick={() => void reload()}
            disabled={loading}
          >
            Reindexar
          </Button>
        </div>
      </header>

      {caseVideos.length > 0 && (
        <div className={styles.caseExtract}>
          <span className={styles.caseExtractLabel}>
            <Film size={12} aria-hidden /> Extrair do vídeo do caso:
          </span>
          <select
            className={styles.select}
            value={pickedVideo}
            onChange={(e) => setPickedVideo(e.target.value)}
          >
            <option value="">selecione um vídeo…</option>
            {caseVideos.map((v) => (
              <option key={v.id} value={v.id}>
                {v.filename}
              </option>
            ))}
          </select>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void handleExtractCase()}
            disabled={busy !== null || !pickedVideo}
          >
            Extrair
          </Button>
        </div>
      )}

      {error && (
        <div className={styles.errorBanner}>
          <AlertTriangle size={14} /> {error}
        </div>
      )}

      {!loading && items.length === 0 ? (
        <EmptyState
          icon={<FileAudio size={32} strokeWidth={1.5} />}
          title="Nenhum áudio neste caso"
          description="Use “Importar áudio” (WhatsApp, gravador…) ou “Extrair de vídeo”. O original é preservado e um WAV de análise é gerado, ambos com hash."
        />
      ) : (
        <div className={styles.body}>
          <aside className={styles.list}>
            {items.map((m) => (
              <button
                key={m.id}
                type="button"
                className={`${styles.listItem} ${m.id === selectedId ? styles.listItemActive : ""}`}
                onClick={() => setSelectedId(m.id)}
              >
                <FileAudio size={15} className={styles.listIcon} />
                <span className={styles.listMain}>
                  <span className={styles.listName}>{m.filename}</span>
                  <span className={styles.listMeta}>
                    {kindLabel(m.kind)}
                    {m.duration_s != null && ` · ${fmtTime(m.duration_s)}`}
                  </span>
                </span>
              </button>
            ))}
          </aside>

          <section className={styles.detail}>
            {selected && fileUrl ? (
              <>
                <div className={styles.detailHead}>
                  <h2 className={styles.detailName}>{selected.filename}</h2>
                  <span className={styles.kindChip} data-kind={selected.kind}>
                    {kindChipLabel(selected.kind)}
                  </span>
                  <span style={{ marginLeft: "auto" }}>
                    <Button
                      variant="secondary"
                      size="sm"
                      leftIcon={<Captions size={14} />}
                      onClick={() => navigate(`/audio/degravacao/${selected.id}`)}
                    >
                      Degravar
                    </Button>
                  </span>
                </div>

                <AudioPlayer
                  ref={playerRef}
                  fileUrl={fileUrl}
                  workspacePath={ws}
                  audioSha256={selected.sha256}
                />

                {/* W16 — abas por intenção. O player fica sempre visível
                    acima; aqui embaixo só troca a "bancada" de ferramentas. */}
                <nav className={styles.tabs} role="tablist">
                  {DETAIL_TABS.map((t) => {
                    const Icon = t.icon;
                    return (
                      <button
                        key={t.key}
                        type="button"
                        role="tab"
                        aria-selected={detailTab === t.key}
                        className={`${styles.tab} ${detailTab === t.key ? styles.tabActive : ""}`}
                        onClick={() => setDetailTab(t.key)}
                        title={t.hint}
                      >
                        <Icon size={13} aria-hidden /> {t.label}
                      </button>
                    );
                  })}
                </nav>

                <div className={styles.tabBody}>
                  {/* ---- Realçar (escuta, não-destrutivo) ---- */}
                  {detailTab === "realcar" &&
                    (selected.kind === "realce" ? (
                      <div className={styles.realceNote}>
                        <SlidersHorizontal size={13} aria-hidden /> Derivado de
                        realce
                        {selected.original_path ? ` (${selected.original_path})` : ""}.
                        Filtros aplicados de forma determinística (FFmpeg); o WAV
                        de análise original permanece intacto e com hash próprio.
                      </div>
                    ) : (
                      <div className={styles.realceBox}>
                        <div className={styles.realceHead}>
                          <SlidersHorizontal size={14} aria-hidden />
                          <span>
                            Realce para escuta{" "}
                            <em>(auxílio, não-destrutivo)</em>
                          </span>
                        </div>
                        <div className={styles.realceFilters}>
                          {REALCE_FILTERS.map((f) => (
                            <label
                              key={f.key}
                              className={styles.realceFilter}
                              title={f.hint}
                            >
                              <input
                                type="checkbox"
                                checked={!!realceSel[f.key]}
                                onChange={(e) =>
                                  setRealceSel((s) => ({
                                    ...s,
                                    [f.key]: e.target.checked,
                                  }))
                                }
                              />
                              {f.label}
                            </label>
                          ))}
                        </div>
                        <div className={styles.realceFoot}>
                          <Button
                            variant="secondary"
                            size="sm"
                            leftIcon={<SlidersHorizontal size={13} />}
                            onClick={() => void handleEnhance()}
                            disabled={busy !== null}
                          >
                            {busy === "enhance"
                              ? "Gerando…"
                              : "Gerar áudio realçado"}
                          </Button>
                          <span className={styles.realceDisclaimer}>
                            Gera uma nova mídia derivada (com hash e custódia).
                            Não recupera nem altera o conteúdo — apenas filtra
                            para facilitar a audição pelo perito.
                          </span>
                        </div>
                      </div>
                    ))}

                  {/* ---- Analisar (medição objetiva, §13) ---- */}
                  {detailTab === "analisar" && (
                    <>
                      <div className={styles.spectroBox}>
                        <div className={styles.spectroHead}>
                          <Button
                            variant="secondary"
                            size="sm"
                            leftIcon={<Activity size={14} />}
                            onClick={() => void genSpectrogram()}
                            disabled={spectroBusy}
                          >
                            {spectroBusy ? "Gerando…" : "Espectrograma"}
                          </Button>
                          {spectro && spectro.id === selected.id && (
                            <Button
                              variant="ghost"
                              size="sm"
                              leftIcon={<FolderOpen size={14} />}
                              onClick={() => {
                                if (ws && spectro) {
                                  void commands.revealEvidenceInFolder(
                                    ws,
                                    spectro.rel,
                                  );
                                }
                              }}
                            >
                              Abrir pasta
                            </Button>
                          )}
                          <span className={styles.spectroHint}>
                            Tempo × frequência do sinal (FFmpeg) — visualização
                            objetiva, não interpreta.
                          </span>
                        </div>
                        {spectro && spectro.id === selected.id && (
                          <img
                            className={styles.spectroImg}
                            src={spectro.url}
                            alt={`Espectrograma de ${selected.filename}`}
                          />
                        )}
                      </div>

                      {ws && (
                        <AudioAnalysisPanel
                          workspacePath={ws}
                          audioId={selected.id}
                        />
                      )}
                    </>
                  )}

                  {/* ---- Trechos (recorte A-B + compilação) ---- */}
                  {detailTab === "trechos" && (
                    <>
                      <div className={styles.clipBar}>
                        <Button
                          variant="secondary"
                          size="sm"
                          leftIcon={<Scissors size={14} />}
                          onClick={() => void handleExtractClip()}
                          disabled={busy !== null}
                        >
                          {busy === "clip"
                            ? "Extraindo trecho…"
                            : "Extrair trecho (A-B)"}
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          leftIcon={<ListPlus size={14} />}
                          onClick={addToCompilation}
                        >
                          Adicionar à compilação
                        </Button>
                        <span className={styles.clipHint}>
                          Defina o loop A-B no player e gere um clipe do trecho —
                          nova mídia derivada, com hash e custódia. O áudio
                          original permanece intacto.
                        </span>
                      </div>

                      {compileList.length > 0 && (
                        <div className={styles.compileBox}>
                          <div className={styles.compileHead}>
                            <ListMusic size={14} aria-hidden /> Compilação
                            rotulada{" "}
                            <em>
                              ({compileList.length} trecho
                              {compileList.length > 1 ? "s" : ""})
                            </em>
                          </div>
                          <ol className={styles.compileList}>
                            {compileList.map((c, i) => (
                              <li key={i} className={styles.compileItem}>
                                <span className={styles.compileNum}>
                                  {i + 1}
                                </span>
                                <div className={styles.compileInfo}>
                                  <span className={styles.compileSrc}>
                                    {c.audioName}
                                  </span>
                                  <span className={styles.compileTimes}>
                                    {fmtTime(c.start)} – {fmtTime(c.end)} (
                                    {fmtTime(c.end - c.start)})
                                  </span>
                                </div>
                                <input
                                  className={styles.compileLabelInput}
                                  placeholder="rótulo (opcional)"
                                  value={c.label}
                                  onChange={(e) =>
                                    setCompileLabel(i, e.target.value)
                                  }
                                />
                                <div className={styles.compileBtns}>
                                  <button
                                    type="button"
                                    onClick={() => moveCompileItem(i, -1)}
                                    disabled={i === 0}
                                    title="Subir"
                                  >
                                    ↑
                                  </button>
                                  <button
                                    type="button"
                                    onClick={() => moveCompileItem(i, 1)}
                                    disabled={i === compileList.length - 1}
                                    title="Descer"
                                  >
                                    ↓
                                  </button>
                                  <button
                                    type="button"
                                    onClick={() => removeCompileItem(i)}
                                    title="Remover"
                                  >
                                    ×
                                  </button>
                                </div>
                              </li>
                            ))}
                          </ol>
                          <div className={styles.compileFoot}>
                            <Button
                              variant="primary"
                              size="sm"
                              leftIcon={<ListMusic size={13} />}
                              onClick={() => void handleCompile()}
                              disabled={compiling || compileList.length < 2}
                            >
                              {compiling ? "Compilando…" : "Gerar compilação"}
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => setCompileList([])}
                              disabled={compiling}
                            >
                              Limpar
                            </Button>
                            <span className={styles.compileDisclaimer}>
                              Junta os trechos na ordem acima num novo áudio
                              derivado (com hash, custódia e um manifesto{" "}
                              <code>.json</code> documentando a origem e os
                              tempos de cada trecho). Há uma pausa curta entre
                              trechos, para que as junções sejam audíveis. Os
                              áudios originais permanecem intactos.
                            </span>
                          </div>
                        </div>
                      )}
                    </>
                  )}

                  {/* ---- Ficha (metadados + custódia) ---- */}
                  {detailTab === "ficha" && (
                    <>
                      <div className={styles.metaGrid}>
                        <Meta label="Duração" value={selected.duration_s != null ? fmtTime(selected.duration_s) : "—"} />
                        <Meta label="Amostragem" value={selected.sample_rate ? `${(selected.sample_rate / 1000).toFixed(1)} kHz` : "—"} />
                        <Meta label="Canais" value={channelsLabel(selected.channels)} />
                        <Meta label="Codec (origem)" value={selected.codec ?? "—"} />
                        <Meta label="Tamanho (WAV)" value={prettyBytes(selected.size_bytes)} />
                        <Meta label="SHA-256 (WAV)" value={selected.sha256.slice(0, 16) + "…"} mono />
                        {selected.original_sha256 && (
                          <Meta label="SHA-256 (original)" value={selected.original_sha256.slice(0, 16) + "…"} mono />
                        )}
                        {selected.source_video_sha256 && (
                          <Meta label="Vídeo de origem" value={selected.source_video_sha256.slice(0, 16) + "…"} mono />
                        )}
                      </div>

                      {warnings.length > 0 && (
                        <div className={styles.warnBox}>
                          <AlertTriangle size={12} /> {warnings.join(" · ")}
                        </div>
                      )}

                      <p className={styles.custody}>
                        Original preservado e WAV de análise gerados de forma
                        determinística (FFmpeg, PCM 16-bit). Ambos com hash
                        registrado na cadeia de custódia.
                      </p>
                    </>
                  )}
                </div>
              </>
            ) : (
              <p className={styles.detailHint}>Selecione um áudio à esquerda.</p>
            )}
          </section>
        </div>
      )}
    </div>
  );
}

function Meta({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className={styles.meta}>
      <span className={styles.metaLabel}>{label}</span>
      <span className={`${styles.metaValue} ${mono ? styles.metaMono : ""}`}>{value}</span>
    </div>
  );
}
