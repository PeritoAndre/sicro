/**
 * Degravação assistida: player sincronizado com a lista de trechos (tempo +
 * locutor + texto), autosave. A IA só preenche rascunhos; a degravação é do perito.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useImmersive } from "@stores/immersiveStore";
import { useNavigate, useParams } from "react-router-dom";
import { convertFileSrc } from "@tauri-apps/api/core";
import { mediaSrc } from "@core/mediaSrc";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ArrowLeft,
  Bot,
  Check,
  ClipboardCopy,
  Flag,
  Plus,
  Trash2,
  Users,
  Volume2,
} from "lucide-react";
import { Button } from "@components/Button/Button";
import { EmptyState } from "@components/EmptyState/EmptyState";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { useShortcuts } from "@core/useShortcuts";
import {
  selectActiveWorkspacePath,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import { useSettingsStore } from "@stores/settingsStore";
import type { AudioDiarization, AudioMedia, TranscriptWord } from "@domain/audio";
import { AudioPlayer, fmtTime, type AudioPlayerHandle } from "./AudioPlayer";
import { assignSpeakers, speakerColor, speakerName } from "./speakers";
import { formatTranscript } from "./transcriptFormat";
import styles from "./DegravacaoView.module.css";

interface LocalSeg {
  localId: string;
  t_start: number;
  t_end: number | null;
  speaker: string;
  text: string;
  /** Rascunho gerado por IA, ainda NÃO revisado pelo perito. */
  draft?: boolean;
  /** Confiança da IA (0..1) — só nos trechos vindos da transcrição automática. */
  confidence?: number | null;
  /** Palavras da IA com tempo e confiança (as duvidosas viram "ouvir de novo"). */
  words?: TranscriptWord[];
  /** Locutor preenchido pela separação de locutores (o perito ainda não mexeu). */
  speakerAuto?: boolean;
  /** Há fala de mais de um locutor no trecho. */
  speakerMixed?: boolean;
}


/** Abaixo disto a palavra é marcada para ouvir de novo; abaixo de LOW, em vermelho. */
const DOUBT_P = 0.5;
const DOUBT_LOW_P = 0.3;
/** "Ouvir de novo" começa um pouco antes (o tempo por palavra do whisper é aproximado). */
const LISTEN_PREROLL_S = 0.7;

type SaveState = "idle" | "saving" | "saved" | "error";

export function DegravacaoView() {
  useImmersive();
  const ws = useWorkspaceStore(selectActiveWorkspacePath);
  const { audioId } = useParams<{ audioId: string }>();
  const navigate = useNavigate();
  // Caminhos da IA: preferir a config do gerenciador (Configurações); senão localStorage.
  const aiSettings = useSettingsStore((s) => s.settings.ai);

  const [media, setMedia] = useState<AudioMedia | null>(null);
  const [segments, setSegments] = useState<LocalSeg[]>([]);
  const [currentTime, setCurrentTime] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<SaveState>("idle");
  const [savedAt, setSavedAt] = useState<Date | null>(null);
  const [focusId, setFocusId] = useState<string | null>(null);
  const [copied, setCopied] = useState<null | "txt" | "srt">(null);
  const [transcribing, setTranscribing] = useState(false);
  const [transcribeErr, setTranscribeErr] = useState<string | null>(null);
  const [whisperOk, setWhisperOk] = useState<boolean | null>(null);
  const [modelName, setModelName] = useState("");
  const [whisperBin, setWhisperBin] = useState("");
  const [language, setLanguage] = useState(
    () => localStorage.getItem("sicro.whisper.lang") || "pt",
  );
  // Separação de locutores (sherpa-onnx local).
  const [diar, setDiar] = useState<AudioDiarization | null>(null);
  const diarRef = useRef(diar);
  diarRef.current = diar;
  const [diarBusy, setDiarBusy] = useState(false);
  const [diarErr, setDiarErr] = useState<string | null>(null);
  const [numSpeakers, setNumSpeakers] = useState<number | null>(() => {
    const v = Number(localStorage.getItem("sicro.diar.n"));
    return v > 0 ? v : null;
  });

  const playerRef = useRef<AudioPlayerHandle>(null);
  const segsRef = useRef<LocalSeg[]>(segments);
  segsRef.current = segments;
  const dirtyRef = useRef(false);
  const textRefs = useRef<Map<string, HTMLTextAreaElement>>(new Map());

  // carga: áudio + degravação persistida
  useEffect(() => {
    if (!ws || !audioId) return;
    let cancelled = false;
    dirtyRef.current = false;
    setLoading(true);
    setError(null);
    void (async () => {
      try {
        const m = await commands.openAudioMedia(ws, audioId);
        const segs = await commands.listAudioTranscript(ws, m.sha256);
        const d = await commands.getAudioDiarization(ws, m.sha256).catch(() => null);
        if (cancelled) return;
        setMedia(m);
        setDiar(d);
        setSegments(
          segs.map((s) => ({
            localId: crypto.randomUUID(),
            t_start: s.t_start,
            t_end: s.t_end,
            speaker: s.speaker,
            text: s.text,
            draft: s.ai?.draft ?? false,
            confidence: s.ai?.confidence ?? null,
            words: s.ai?.words ?? [],
            speakerAuto: s.ai?.speaker_auto ?? false,
            speakerMixed: s.ai?.speaker_mixed ?? false,
          })),
        );
        setSaveState(segs.length > 0 ? "saved" : "idle");
      } catch (e) {
        if (!cancelled) setError(toSicroError(e).message);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [ws, audioId]);

  // persistência (replace-all)
  const persistNow = useCallback(() => {
    if (!ws || !media) return;
    const payload = segsRef.current.map((s, i) => ({
      idx: i,
      t_start: s.t_start,
      t_end: s.t_end,
      speaker: s.speaker,
      text: s.text,
      ai:
        s.draft || s.confidence != null || (s.words?.length ?? 0) > 0 || s.speakerAuto || s.speakerMixed
          ? {
              draft: !!s.draft,
              confidence: s.confidence ?? null,
              words: s.words ?? [],
              speaker_auto: !!s.speakerAuto,
              speaker_mixed: !!s.speakerMixed,
            }
          : null,
    }));
    setSaveState("saving");
    void commands
      .saveAudioTranscript(ws, media.sha256, payload)
      .then(() => {
        dirtyRef.current = false;
        setSaveState("saved");
        setSavedAt(new Date());
      })
      .catch(() => setSaveState("error"));
  }, [ws, media]);

  // Autosave com debounce (só quando há edição do perito).
  useEffect(() => {
    if (!dirtyRef.current) return;
    setSaveState("saving");
    const h = window.setTimeout(() => persistNow(), 1000);
    return () => window.clearTimeout(h);
  }, [segments, persistNow]);

  // Flush ao sair da tela, se houver pendência.
  useEffect(() => {
    return () => {
      if (dirtyRef.current) persistNow();
    };
  }, [persistNow]);

  // mutações (marcam dirty)
  const mutate = useCallback((updater: (prev: LocalSeg[]) => LocalSeg[]) => {
    dirtyRef.current = true;
    setSegments(updater);
  }, []);

  const capture = useCallback(() => {
    const t = playerRef.current?.getTime() ?? 0;
    const localId = crypto.randomUUID();
    mutate((prev) => {
      const last = prev[prev.length - 1];
      const speaker = last ? last.speaker : "";
      const next = [...prev, { localId, t_start: t, t_end: null, speaker, text: "" }];
      next.sort((a, b) => a.t_start - b.t_start);
      return next;
    });
    setFocusId(localId);
  }, [mutate]);

  // O perito escreveu o locutor: passa a ser dele (a separação não mexe mais).
  const setSpeaker = useCallback(
    (localId: string, speaker: string) => {
      mutate((prev) =>
        prev.map((s) => (s.localId === localId ? { ...s, speaker, speakerAuto: false } : s)),
      );
    },
    [mutate],
  );

  const updateSeg = useCallback(
    (localId: string, patch: Partial<LocalSeg>) => {
      // Editar um trecho marca-o como revisado (deixa de ser rascunho da IA).
      mutate((prev) =>
        prev.map((s) =>
          s.localId === localId ? { ...s, ...patch, draft: false } : s,
        ),
      );
    },
    [mutate],
  );

  const setEnd = useCallback(
    (localId: string) => {
      const t = playerRef.current?.getTime() ?? 0;
      updateSeg(localId, { t_end: t });
    },
    [updateSeg],
  );

  const removeSeg = useCallback(
    (localId: string) => {
      mutate((prev) => prev.filter((s) => s.localId !== localId));
    },
    [mutate],
  );

  const seek = useCallback((t: number) => {
    playerRef.current?.seekTo(t);
  }, []);

  const listen = useCallback((t: number) => {
    playerRef.current?.seekTo(Math.max(0, t - LISTEN_PREROLL_S));
    playerRef.current?.play();
  }, []);

  // "Conferido": tira a marcação das palavras duvidosas (não mexe no texto).
  const clearDoubts = useCallback(
    (localId: string) => {
      mutate((prev) => prev.map((s) => (s.localId === localId ? { ...s, words: [] } : s)));
    },
    [mutate],
  );

  // Atalhos de pedal (escopo `audio`): todos com Ctrl para conviver com a
  // digitação, por isso `allowInInputs`. As funções são lidas só no disparo —
  // a ordem de declaração no componente não importa.
  useShortcuts(
    {
      "audio.playPause": () => playerRef.current?.togglePlay(),
      "audio.back3s": () => {
        const t = playerRef.current?.getTime() ?? 0;
        playerRef.current?.seekTo(Math.max(0, t - 3));
      },
      "audio.fwd3s": () => {
        const t = playerRef.current?.getTime() ?? 0;
        playerRef.current?.seekTo(t + 3);
      },
      "audio.capture": () => capture(),
      "audio.markEnd": () => {
        const seg = segsRef.current[activeIdx];
        if (seg) setEnd(seg.localId);
      },
      "audio.save": () => persistNow(),
      "audio.transcribeAI": () => void runTranscribe(),
      "audio.copyTxt": () => void copy("txt"),
      "audio.copySrt": () => void copy("srt"),
    },
    { allowInInputs: true },
  );

  // Foca o textarea do segmento recém-capturado.
  useEffect(() => {
    if (!focusId) return;
    textRefs.current.get(focusId)?.focus();
    setFocusId(null);
  }, [focusId, segments]);

  const fileUrl = useMemo(
    () => (ws && media ? convertFileSrc(`${ws}/${media.relative_path}`) : null),
    [ws, media],
  );
  const mediaUrl = useMemo(
    () => (ws && media ? mediaSrc(`${ws}/${media.relative_path}`) : undefined),
    [ws, media],
  );

  const activeIdx = useMemo(() => {
    for (let i = 0; i < segments.length; i++) {
      const s = segments[i];
      if (!s) continue;
      const next = segments[i + 1];
      const end = s.t_end != null ? s.t_end : next ? next.t_start : Infinity;
      if (currentTime >= s.t_start && currentTime < end) return i;
    }
    return -1;
  }, [segments, currentTime]);

  const copy = useCallback(
    async (fmt: "txt" | "srt") => {
      const text = formatTranscript(
        segsRef.current.map((s) => ({
          t_start: s.t_start,
          t_end: s.t_end,
          speaker: s.speaker,
          text: s.text,
        })),
        fmt,
      );
      try {
        await navigator.clipboard.writeText(text);
        setCopied(fmt);
        window.setTimeout(() => setCopied(null), 1500);
      } catch {
        /* clipboard indisponível */
      }
    },
    [],
  );

  // transcrição (whisper.cpp local, rascunho)
  useEffect(() => {
    const bin =
      aiSettings.whisper_bin_path || (localStorage.getItem("sicro.whisper.bin") ?? "");
    setWhisperBin(bin);
    void commands
      .whisperStatus(bin || undefined)
      .then((s) => setWhisperOk(s.available))
      .catch(() => setWhisperOk(false));
    const model =
      aiSettings.model_path || (localStorage.getItem("sicro.whisper.modelPath") ?? "");
    setModelName(model ? (model.split(/[\\/]/).pop() ?? "") : "");
  }, [aiSettings]);

  const pickModel = useCallback(async (): Promise<string | null> => {
    const picked = await openFileDialog({
      multiple: false,
      filters: [{ name: "Modelo whisper (GGUF/bin)", extensions: ["bin", "gguf"] }],
    });
    if (typeof picked !== "string") return null;
    localStorage.setItem("sicro.whisper.modelPath", picked);
    setModelName(picked.split(/[\\/]/).pop() ?? "");
    return picked;
  }, []);

  const pickWhisperBin = useCallback(async () => {
    const picked = await openFileDialog({
      multiple: false,
      filters: [{ name: "whisper-cli (executável)", extensions: ["exe"] }],
    });
    if (typeof picked !== "string") return;
    localStorage.setItem("sicro.whisper.bin", picked);
    setWhisperBin(picked);
    setTranscribeErr(null);
    try {
      const s = await commands.whisperStatus(picked);
      setWhisperOk(s.available);
    } catch {
      setWhisperOk(false);
    }
  }, []);

  const runTranscribe = useCallback(async () => {
    if (!ws || !media) return;
    if (whisperOk === false) {
      setTranscribeErr(
        "whisper.cpp não encontrado. Instale o whisper-cli (github.com/ggml-org/whisper.cpp), " +
          "deixe-o no PATH e baixe um modelo (ex.: ggml-large-v3-turbo). " +
          "Depois selecione o modelo ao gerar o rascunho.",
      );
      return;
    }
    let modelPath =
      aiSettings.model_path || (localStorage.getItem("sicro.whisper.modelPath") ?? "");
    if (!modelPath) {
      const picked = await pickModel();
      if (!picked) return;
      modelPath = picked;
    }
    const whisperBin =
      aiSettings.whisper_bin_path ||
      (localStorage.getItem("sicro.whisper.bin") ?? undefined);
    setTranscribing(true);
    setTranscribeErr(null);
    try {
      const cands = await commands.transcribeAudio(ws, media.id, {
        modelPath,
        whisperBin,
        language,
        // VAD (anti-alucinação) entra automaticamente se o modelo Silero foi instalado.
        vadModelPath: aiSettings.vad_model_path || undefined,
      });
      if (cands.length === 0) {
        setTranscribeErr("Nenhuma fala detectada no áudio.");
        return;
      }
      // Re-rodar substitui apenas os rascunhos NÃO revisados; preserva o que o perito já editou.
      mutate((prev) => {
        const kept = prev.filter((s) => !s.draft);
        const incoming: LocalSeg[] = cands.map((c) => ({
          localId: crypto.randomUUID(),
          t_start: c.t_start,
          t_end: c.t_end,
          speaker: "",
          text: c.text,
          draft: true,
          confidence: c.confidence,
          words: c.words,
        }));
        return assignSpeakers(
          [...kept, ...incoming].sort((a, b) => a.t_start - b.t_start),
          diarRef.current,
        );
      });
    } catch (e) {
      setTranscribeErr(toSicroError(e).message);
    } finally {
      setTranscribing(false);
    }
  }, [ws, media, whisperOk, pickModel, mutate, aiSettings, language]);

  const runDiarize = useCallback(async () => {
    if (!ws || !media) return;
    if (!aiSettings.diar_bin_path) {
      setDiarErr("Separador de locutores não instalado — baixe em Configurações → IA (≈ 58 MB, uma vez; depois roda offline).");
      return;
    }
    setDiarBusy(true);
    setDiarErr(null);
    try {
      const d = await commands.diarizeAudio(ws, media.id, numSpeakers);
      setDiar(d);
      mutate((prev) => assignSpeakers(prev, d));
    } catch (e) {
      setDiarErr(toSicroError(e).message);
    } finally {
      setDiarBusy(false);
    }
  }, [ws, media, aiSettings.diar_bin_path, numSpeakers, mutate]);

  // Nome dado na legenda vale para todos os trechos preenchidos pela separação.
  const renameSpeaker = useCallback(
    (n: number, name: string) => {
      const d = diarRef.current;
      if (!ws || !media || !d) return;
      const before = speakerName(d, n);
      const names = Array.from({ length: Math.max(d.names.length, n) }, (_, i) => d.names[i] ?? "");
      names[n - 1] = name.trim();
      const next = { ...d, names };
      const after = speakerName(next, n);
      setDiar(next);
      void commands.saveDiarizationNames(ws, media.sha256, names).catch(() => undefined);
      if (after !== before) {
        mutate((prev) =>
          prev.map((s) => (s.speakerAuto && s.speaker === before ? { ...s, speaker: after } : s)),
        );
      }
    },
    [ws, media, mutate],
  );

  const draftCount = useMemo(
    () => segments.filter((s) => s.draft).length,
    [segments],
  );

  if (!ws) {
    return (
      <div className={styles.wrap}>
        <EmptyState
          icon={<Flag size={34} strokeWidth={1.5} />}
          title="Nenhum caso aberto"
          description="Abra uma ocorrência para degravar áudios."
        />
      </div>
    );
  }

  return (
    <div className={styles.wrap}>
      <header className={styles.bar}>
        <button type="button" className={styles.back} onClick={() => navigate("/audio")}>
          <ArrowLeft size={15} /> Áudios
        </button>
        <div className={styles.barTitle} title={media?.filename}>
          Degravação — {media?.filename ?? "…"}
        </div>
        <SaveBadge state={saveState} at={savedAt} />
        <div className={styles.barActions}>
          <button
            type="button"
            className={styles.copyBtn}
            onClick={() => void copy("txt")}
            disabled={segments.length === 0}
          >
            <ClipboardCopy size={13} /> {copied === "txt" ? "copiado!" : "Copiar texto"}
          </button>
          <button
            type="button"
            className={styles.copyBtn}
            onClick={() => void copy("srt")}
            disabled={segments.length === 0}
          >
            <ClipboardCopy size={13} /> {copied === "srt" ? "copiado!" : "Copiar SRT"}
          </button>
        </div>
      </header>

      {error && <div className={styles.errorBanner}>{error}</div>}

      {loading ? (
        <div className={styles.loading}>Carregando…</div>
      ) : !media || !fileUrl ? (
        <div className={styles.loading}>Áudio não encontrado.</div>
      ) : (
        <>
          <div className={styles.playerZone}>
            <AudioPlayer
              ref={playerRef}
              fileUrl={fileUrl}
              mediaUrl={mediaUrl}
              workspacePath={ws}
              audioSha256={media.sha256}
              onTimeChange={setCurrentTime}
            />
            {diar && diar.turns.length > 0 && (
              <SpeakerStrip
                diar={diar}
                duration={media.duration_s ?? 0}
                time={currentTime}
                onSeek={seek}
                onRename={renameSpeaker}
              />
            )}
            <div className={styles.captureBar}>
              <Button
                variant="primary"
                size="sm"
                leftIcon={<Plus size={14} />}
                onClick={() => capture()}
              >
                Capturar trecho
              </Button>
              <Button
                variant="secondary"
                size="sm"
                leftIcon={<Bot size={14} />}
                onClick={() => void runTranscribe()}
                disabled={transcribing}
              >
                {transcribing ? "Transcrevendo…" : "Rascunho IA"}
              </Button>
              <label className={styles.langSelect} title="Idioma do áudio para a transcrição">
                idioma:
                <select
                  value={language}
                  onChange={(e) => {
                    setLanguage(e.target.value);
                    localStorage.setItem("sicro.whisper.lang", e.target.value);
                  }}
                  disabled={transcribing}
                >
                  <option value="pt">Português</option>
                  <option value="auto">Detectar automaticamente</option>
                  <option value="es">Espanhol</option>
                  <option value="en">Inglês</option>
                  <option value="fr">Francês</option>
                  <option value="it">Italiano</option>
                  <option value="de">Alemão</option>
                </select>
              </label>
              <Button
                variant="secondary"
                size="sm"
                leftIcon={<Users size={14} />}
                onClick={() => void runDiarize()}
                disabled={diarBusy}
                title="Separa as vozes: quem fala quando (local, offline)"
              >
                {diarBusy ? "Separando…" : "Locutores"}
              </Button>
              <label
                className={styles.langSelect}
                title="Se souber quantas pessoas falam, informe — é mais confiável que o automático"
              >
                pessoas:
                <select
                  value={numSpeakers ?? 0}
                  onChange={(e) => {
                    const v = Number(e.target.value);
                    setNumSpeakers(v > 0 ? v : null);
                    localStorage.setItem("sicro.diar.n", String(v));
                  }}
                  disabled={diarBusy}
                >
                  <option value={0}>automático (estimativa)</option>
                  {[2, 3, 4, 5, 6, 7, 8].map((n) => (
                    <option key={n} value={n}>
                      {n}
                    </option>
                  ))}
                </select>
              </label>
              <span className={styles.hint}>
                <kbd>Ctrl</kbd>+<kbd>Espaço</kbd> tocar · <kbd>Ctrl</kbd>+<kbd>←</kbd>/<kbd>→</kbd> ±3s ·{" "}
                <kbd>Ctrl</kbd>+<kbd>Enter</kbd> capturar · <kbd>Ctrl</kbd>+<kbd>S</kbd> salvar
              </span>
              <span className={styles.modelLine}>
                whisper:{" "}
                {whisperBin
                  ? (whisperBin.split(/[\\/]/).pop() ?? whisperBin)
                  : whisperOk
                    ? "no PATH"
                    : "não encontrado"}{" "}
                <button
                  type="button"
                  className={styles.modelChange}
                  onClick={() => void pickWhisperBin()}
                >
                  localizar
                </button>
              </span>
              {modelName && (
                <span className={styles.modelLine}>
                  modelo IA: {modelName}{" "}
                  <button
                    type="button"
                    className={styles.modelChange}
                    onClick={() => void pickModel()}
                  >
                    trocar
                  </button>
                </span>
              )}
            </div>
          </div>

          {transcribing && (
            <div className={styles.transcribingNote}>
              Transcrevendo localmente (offline)… pode levar alguns minutos conforme o
              tamanho do áudio e o modelo. A saída é um <strong>rascunho</strong> — você
              revisa em seguida.
            </div>
          )}
          {transcribeErr && <div className={styles.errorBanner}>{transcribeErr}</div>}
          {diarErr && <div className={styles.errorBanner}>{diarErr}</div>}
          {draftCount > 0 && (
            <div className={styles.draftBanner}>
              <AlertTriangle size={14} aria-hidden />
              <span>
                {draftCount} trecho(s) são <strong>rascunho da IA</strong> (não revisados).
                O whisper pode errar ou inventar texto em ruído/silêncio — revise cada um
                antes de usar no laudo. Editar um trecho marca-o como revisado. As
                palavras em que a IA teve dúvida aparecem embaixo do trecho — clique para
                ouvir de novo.
              </span>
            </div>
          )}

          <div className={styles.segList}>
            {segments.length === 0 ? (
              <p className={styles.empty}>
                Nenhum trecho ainda. Toque o áudio e use <strong>Capturar trecho</strong>{" "}
                (ou <kbd>Ctrl</kbd>+<kbd>Enter</kbd>) para marcar o início de cada fala,
                então digite o locutor e a transcrição. Tudo salva sozinho.
              </p>
            ) : (
              segments.map((s, i) => (
                <div
                  key={s.localId}
                  className={`${styles.seg} ${i === activeIdx ? styles.segActive : ""} ${s.draft ? styles.segDraft : ""}`}
                >
                  <button
                    type="button"
                    className={styles.segTime}
                    onClick={() => seek(s.t_start)}
                    title="Ir para este ponto"
                  >
                    {fmtTime(s.t_start)}
                  </button>
                  <input
                    className={styles.speaker}
                    value={s.speaker}
                    placeholder="Locutor"
                    style={(() => {
                      const n = diar ? diar.turns.find((t) => speakerName(diar, t.speaker) === s.speaker)?.speaker : undefined;
                      return n ? { borderLeft: `3px solid ${speakerColor(n)}` } : undefined;
                    })()}
                    title={s.speakerAuto ? "Preenchido pela separação de locutores — confira" : undefined}
                    onChange={(e) => setSpeaker(s.localId, e.target.value)}
                  />
                  <textarea
                    ref={(el) => {
                      if (el) textRefs.current.set(s.localId, el);
                      else textRefs.current.delete(s.localId);
                    }}
                    className={styles.text}
                    value={s.text}
                    placeholder="Transcrição do trecho…"
                    rows={2}
                    onChange={(e) => updateSeg(s.localId, { text: e.target.value })}
                  />
                  <div className={styles.segActions}>
                    {s.draft && (
                      <span className={styles.iaTag} title="Rascunho da IA — revise">
                        IA
                      </span>
                    )}
                    {s.speakerMixed && (
                      <span
                        className={styles.mixedTag}
                        title="A separação ouviu mais de uma voz neste trecho — confira e, se for o caso, divida"
                      >
                        2+ vozes
                      </span>
                    )}
                    {s.draft && s.confidence != null && (
                      <span
                        className={styles.confChip}
                        data-level={
                          s.confidence >= 0.75
                            ? "hi"
                            : s.confidence >= 0.5
                              ? "mid"
                              : "lo"
                        }
                        title={`Confiança da IA neste trecho: ${Math.round(s.confidence * 100)}% — quanto menor, mais atenção na revisão`}
                      >
                        {Math.round(s.confidence * 100)}%
                      </span>
                    )}
                    <button
                      type="button"
                      className={styles.segBtn}
                      onClick={() => setEnd(s.localId)}
                      title="Definir fim no tempo atual"
                    >
                      fim{s.t_end != null ? ` ${fmtTime(s.t_end)}` : ""}
                    </button>
                    <button
                      type="button"
                      className={styles.segDel}
                      onClick={() => removeSeg(s.localId)}
                      title="Remover trecho"
                    >
                      <Trash2 size={13} />
                    </button>
                  </div>
                  <DoubtWords
                    words={s.words}
                    onListen={listen}
                    onClear={() => clearDoubts(s.localId)}
                  />
                </div>
              ))
            )}
          </div>
        </>
      )}
    </div>
  );
}

/** Faixa "quem fala quando" + legenda com os nomes (editáveis) dos locutores. */
function SpeakerStrip({
  diar,
  duration,
  time,
  onSeek,
  onRename,
}: {
  diar: AudioDiarization;
  duration: number;
  time: number;
  onSeek: (t: number) => void;
  onRename: (n: number, name: string) => void;
}) {
  const total = duration > 0 ? duration : Math.max(...diar.turns.map((t) => t.t_end));
  const speakers = [...new Set(diar.turns.map((t) => t.speaker))].sort((a, b) => a - b);
  const talk = (n: number) =>
    diar.turns.filter((t) => t.speaker === n).reduce((acc, t) => acc + (t.t_end - t.t_start), 0);
  const informed = diar.params["locutores_informados"];
  return (
    <div className={styles.diar}>
      <div className={styles.diarStrip}>
        {diar.turns.map((t, i) => (
          <button
            key={i}
            type="button"
            className={styles.diarTurn}
            style={{
              left: `${(t.t_start / total) * 100}%`,
              width: `${Math.max(0.15, ((t.t_end - t.t_start) / total) * 100)}%`,
              background: speakerColor(t.speaker),
            }}
            onClick={() => onSeek(t.t_start)}
            title={`${speakerName(diar, t.speaker)} · ${fmtTime(t.t_start)}–${fmtTime(t.t_end)}`}
          />
        ))}
        {total > 0 && <div className={styles.diarHead} style={{ left: `${(time / total) * 100}%` }} />}
      </div>
      <div className={styles.diarLegend}>
        {speakers.map((n) => (
          <label key={n} className={styles.diarName}>
            <span className={styles.diarDot} style={{ background: speakerColor(n) }} />
            <SpeakerNameInput
              value={diar.names[n - 1] ?? ""}
              placeholder={`Locutor ${n}`}
              onCommit={(v) => onRename(n, v)}
            />
            <span className={styles.diarTalk}>{fmtTime(talk(n))}</span>
          </label>
        ))}
        <span className={styles.diarMeta}>
          {typeof informed === "number"
            ? `${informed} pessoas informadas`
            : "nº de vozes estimado — se souber quantas pessoas falam, informe e rode de novo"}{" "}
          · separação de vozes, não identificação
        </span>
      </div>
    </div>
  );
}

function SpeakerNameInput({
  value,
  placeholder,
  onCommit,
}: {
  value: string;
  placeholder: string;
  onCommit: (v: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return (
    <input
      className={styles.diarInput}
      value={draft}
      placeholder={placeholder}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => draft !== value && onCommit(draft)}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
      }}
      title="Nome deste locutor — vale para todos os trechos preenchidos pela separação"
    />
  );
}

/** Palavras em que a IA teve pouca confiança — clique toca o áudio ali. */
function DoubtWords({
  words,
  onListen,
  onClear,
}: {
  words: TranscriptWord[] | undefined;
  onListen: (t: number) => void;
  onClear: () => void;
}) {
  const doubts = (words ?? []).filter((w) => w.p < DOUBT_P);
  if (doubts.length === 0) return null;
  return (
    <div className={styles.doubts}>
      <span
        className={styles.doubtsLabel}
        title={`Palavras em que a IA teve menos de ${Math.round(DOUBT_P * 100)}% de confiança`}
      >
        ouvir de novo:
      </span>
      {doubts.map((w, k) => (
        <button
          key={`${w.t_start}-${k}`}
          type="button"
          className={styles.doubt}
          data-level={w.p < DOUBT_LOW_P ? "lo" : "mid"}
          onClick={() => onListen(w.t_start)}
          title={`Tocar a partir de ${fmtTime(Math.max(0, w.t_start - LISTEN_PREROLL_S))} — confiança ${Math.round(w.p * 100)}% (tempo aproximado)`}
        >
          <Volume2 size={11} aria-hidden /> {w.text} <span>{Math.round(w.p * 100)}%</span>
        </button>
      ))}
      <button
        type="button"
        className={styles.doubtsOk}
        onClick={onClear}
        title="Já conferi estas palavras — tirar a marcação"
      >
        <Check size={11} aria-hidden /> conferido
      </button>
    </div>
  );
}

function SaveBadge({ state, at }: { state: SaveState; at: Date | null }) {
  if (state === "saving") return <span className={styles.saveBadge}>salvando…</span>;
  if (state === "error")
    return <span className={`${styles.saveBadge} ${styles.saveErr}`}>erro ao salvar</span>;
  if (state === "saved") {
    const hm = at
      ? ` ${at.getHours().toString().padStart(2, "0")}:${at.getMinutes().toString().padStart(2, "0")}`
      : "";
    return <span className={`${styles.saveBadge} ${styles.saveOk}`}>salvo{hm}</span>;
  }
  return <span className={styles.saveBadge}>—</span>;
}
