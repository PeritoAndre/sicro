/**
 * DegravacaoView — tela dedicada (modo foco) de degravação assistida MANUAL.
 *
 * Sincroniza o player (via ref) com uma lista de segmentos editáveis
 * (timestamp + locutor + texto). Autosave com indicador (replace-all no
 * backend). O tool NÃO transcreve nem interpreta — a degravação é trabalho do
 * perito. A IA (Fase 2) apenas preencherá candidatos nesta mesma lista.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { convertFileSrc } from "@tauri-apps/api/core";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, ArrowLeft, Bot, ClipboardCopy, Flag, Plus, Trash2 } from "lucide-react";
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
import type { AudioMedia } from "@domain/audio";
import { AudioPlayer, fmtTime, type AudioPlayerHandle } from "./AudioPlayer";
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
}

type SaveState = "idle" | "saving" | "saved" | "error";

export function DegravacaoView() {
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

  const playerRef = useRef<AudioPlayerHandle>(null);
  const segsRef = useRef<LocalSeg[]>(segments);
  segsRef.current = segments;
  const dirtyRef = useRef(false);
  const textRefs = useRef<Map<string, HTMLTextAreaElement>>(new Map());

  // ---- Carga: áudio + degravação persistida ------------------------------
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
        if (cancelled) return;
        setMedia(m);
        setSegments(
          segs.map((s) => ({
            localId: crypto.randomUUID(),
            t_start: s.t_start,
            t_end: s.t_end,
            speaker: s.speaker,
            text: s.text,
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

  // ---- Persistência (replace-all) ----------------------------------------
  const persistNow = useCallback(() => {
    if (!ws || !media) return;
    const payload = segsRef.current.map((s, i) => ({
      idx: i,
      t_start: s.t_start,
      t_end: s.t_end,
      speaker: s.speaker,
      text: s.text,
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

  // ---- Mutações (marcam dirty) -------------------------------------------
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

  // ---- Atalhos de pedal (customizáveis, escopo `audio`) ------------------
  //
  // Todos usam Ctrl de propósito para conviver com a digitação nos campos da
  // degravação, então passamos `allowInInputs: true` — disparam mesmo com o
  // cursor num textarea/input. As funções referenciadas (capture, persistNow,
  // copy, runTranscribe, setEnd) são lidas só no momento do disparo, então a
  // ordem de declaração no corpo do componente não importa.
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

  // ---- Transcrição (Fase 2 — whisper.cpp local, RASCUNHO) ----------------
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
        }));
        return [...kept, ...incoming].sort((a, b) => a.t_start - b.t_start);
      });
    } catch (e) {
      setTranscribeErr(toSicroError(e).message);
    } finally {
      setTranscribing(false);
    }
  }, [ws, media, whisperOk, pickModel, mutate, aiSettings, language]);

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
              workspacePath={ws}
              audioSha256={media.sha256}
              onTimeChange={setCurrentTime}
            />
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
          {draftCount > 0 && (
            <div className={styles.draftBanner}>
              <AlertTriangle size={14} aria-hidden />
              <span>
                {draftCount} trecho(s) são <strong>rascunho da IA</strong> (não revisados).
                O whisper pode errar ou inventar texto em ruído/silêncio — revise cada um
                antes de usar no laudo. Editar um trecho marca-o como revisado.
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
                    onChange={(e) => updateSeg(s.localId, { speaker: e.target.value })}
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
                </div>
              ))
            )}
          </div>
        </>
      )}
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
