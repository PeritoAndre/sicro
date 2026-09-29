/**
 * Diálogos leves do editor de vídeo, desenhados DENTRO do painel do
 * reprodutor (continuam visíveis em tela cheia):
 *
 *   - ClockDialog: vincula o tempo do vídeo ao relógio que a câmera imprime.
 *   - SequenceDialog: coleta N quadros seguidos a partir do instante atual.
 */
import { useState } from "react";
import type { VideoClockCalibration } from "@domain/video";
import { formatClock, formatDuration, parseClockInput } from "./format";
import styles from "./AnalysisDialogs.module.css";

// ---- relógio da câmera -------------------------------------------------------

interface ClockProps {
  /** Instante atual do vídeo (onde o vínculo será feito). */
  mediaTime: number;
  current: VideoClockCalibration | null;
  busy: boolean;
  onSave: (v: { clockSeconds: number; clockLabel: string; clockDate: string | null; note: string }) => void;
  onDelete: () => void;
  onClose: () => void;
}

export function ClockDialog({ mediaTime, current, busy, onSave, onDelete, onClose }: ClockProps) {
  // Sugere o horário que o vínculo atual já prevê para este instante.
  const suggested = current
    ? formatClock(mediaTime - current.media_time_s + current.clock_seconds, false)
    : "";
  const [text, setText] = useState(suggested);
  const [date, setDate] = useState(current?.clock_date ?? "");
  const [note, setNote] = useState(current?.note ?? "");
  const parsed = parseClockInput(text);

  const submit = () => {
    if (parsed == null) return;
    onSave({ clockSeconds: parsed, clockLabel: text.trim(), clockDate: date || null, note });
  };

  return (
    <div className={styles.backdrop} data-no-magnify onPointerDown={(e) => e.stopPropagation()}>
      <div className={styles.dialog} role="dialog" aria-label="Relógio da câmera">
        <h3>Relógio da câmera</h3>
        <p className={styles.lead}>
          Neste instante do vídeo (<code>{formatDuration(mediaTime)}</code>), o relógio impresso
          pela câmera marca:
        </p>
        <div className={styles.fields}>
          <label>
            Horário
            <input
              autoFocus
              value={text}
              placeholder="03:36:05"
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") submit();
                if (e.key === "Escape") onClose();
              }}
              className={text && parsed == null ? styles.bad : ""}
            />
          </label>
          <label>
            Data na imagem <span className={styles.opt}>(opcional)</span>
            <input type="date" value={date} onChange={(e) => setDate(e.target.value)} />
          </label>
        </div>
        <label className={styles.full}>
          Observação <span className={styles.opt}>(opcional)</span>
          <input
            value={note}
            placeholder="ex.: relógio da câmera ~2 min adiantado em relação ao horário oficial"
            onChange={(e) => setNote(e.target.value)}
          />
        </label>
        <p className={styles.tip}>
          Dica: pare num quadro em que o segundo do relógio acabou de virar — o vínculo fica
          preciso ao quadro. O vínculo fica gravado no caso, com registro na trilha de operações.
        </p>
        <div className={styles.actions}>
          {current && (
            <button type="button" className={styles.danger} onClick={onDelete} disabled={busy}>
              Remover vínculo
            </button>
          )}
          <span className={styles.spacer} />
          <button type="button" onClick={onClose}>
            Cancelar
          </button>
          <button
            type="button"
            className={styles.primary}
            onClick={submit}
            disabled={busy || parsed == null}
          >
            Salvar
          </button>
        </div>
      </div>
    </div>
  );
}

// ---- sequência de quadros ------------------------------------------------------

interface SeqProps {
  startTime: number;
  fps: number | null;
  duration: number;
  onStart: (count: number, stepFrames: number) => void;
  onClose: () => void;
}

const SEQ_KEY = "sicro.video.sequence.v1";

export function SequenceDialog({ startTime, fps, duration, onStart, onClose }: SeqProps) {
  const saved = (() => {
    try {
      return JSON.parse(localStorage.getItem(SEQ_KEY) ?? "null") as { n?: number; step?: number } | null;
    } catch {
      return null;
    }
  })();
  const [count, setCount] = useState(saved?.n ?? 10);
  const [step, setStep] = useState(saved?.step ?? 1);
  const frameDur = fps && fps > 0 ? 1 / fps : 1 / 30;
  const n = Math.max(1, Math.min(200, Math.round(count) || 1));
  const k = Math.max(1, Math.min(100, Math.round(step) || 1));
  const end = Math.min(duration || Infinity, startTime + (n - 1) * k * frameDur);

  const start = () => {
    try {
      localStorage.setItem(SEQ_KEY, JSON.stringify({ n, step: k }));
    } catch {
      /* só não lembra */
    }
    onStart(n, k);
  };

  return (
    <div className={styles.backdrop} data-no-magnify onPointerDown={(e) => e.stopPropagation()}>
      <div className={styles.dialog} role="dialog" aria-label="Coletar sequência">
        <h3>Coletar sequência de quadros</h3>
        <p className={styles.lead}>
          A partir de <code>{formatDuration(startTime)}</code>, cada quadro vira um PNG no
          storyboard (com hash e JSON), igual ao "Coletar frame".
        </p>
        <div className={styles.fields}>
          <label>
            Quantos quadros
            <input
              autoFocus
              type="number"
              min={1}
              max={200}
              value={count}
              onChange={(e) => setCount(Number(e.target.value))}
              onKeyDown={(e) => {
                if (e.key === "Enter") start();
                if (e.key === "Escape") onClose();
              }}
            />
          </label>
          <label>
            De quantos em quantos quadros
            <input
              type="number"
              min={1}
              max={100}
              value={step}
              onChange={(e) => setStep(Number(e.target.value))}
              onKeyDown={(e) => {
                if (e.key === "Enter") start();
                if (e.key === "Escape") onClose();
              }}
            />
          </label>
        </div>
        <p className={styles.tip}>
          Vai de <code>{formatDuration(startTime)}</code> a <code>{formatDuration(end)}</code>
          {fps ? ` (fps declarado ${fps.toFixed(2)})` : " (fps desconhecido — passo estimado em 1/30 s)"}.
          Em vídeo com fps variável os instantes são estimados; cada PNG registra o tempo real
          entregue pelo ffmpeg.
        </p>
        <div className={styles.actions}>
          <span className={styles.spacer} />
          <button type="button" onClick={onClose}>
            Cancelar
          </button>
          <button type="button" className={styles.primary} onClick={start}>
            Coletar {n} quadros
          </button>
        </div>
      </div>
    </div>
  );
}

// ---- exportar trecho -----------------------------------------------------------

interface ClipProps {
  range: { a: number; b: number };
  hasAudio: boolean;
  busy: boolean;
  result: import("@domain/video").ExportClipResult | null;
  onExport: (mode: import("@domain/video").ClipMode, includeAudio: boolean) => void;
  onOpenClip: () => void;
  onClose: () => void;
}

export function ExportClipDialog({ range, hasAudio, busy, result, onExport, onOpenClip, onClose }: ClipProps) {
  const [mode, setMode] = useState<import("@domain/video").ClipMode>("copy");
  const [audio, setAudio] = useState(hasAudio);
  const len = range.b - range.a;

  return (
    <div className={styles.backdrop} data-no-magnify onPointerDown={(e) => e.stopPropagation()}>
      <div className={styles.dialog} role="dialog" aria-label="Exportar trecho">
        <h3>Exportar trecho</h3>
        {result ? (
          <>
            <p className={styles.lead}>
              {result.already_existed ? "Esse trecho já tinha sido exportado: " : "Trecho salvo no caso como "}
              <code>{result.media.filename}</code>.
            </p>
            <p className={styles.tip}>
              Cobre <code>{formatDuration(result.actual_start_s)}</code> →{" "}
              <code>{formatDuration(result.actual_end_s)}</code> do vídeo de origem. Hash próprio,
              JSON ao lado e registro na trilha de operações; o original não foi alterado.
            </p>
            {result.warnings.map((w, i) => (
              <p key={i} className={styles.warn}>
                {w}
              </p>
            ))}
            <div className={styles.actions}>
              <span className={styles.spacer} />
              <button type="button" onClick={onClose}>
                Fechar
              </button>
              <button type="button" className={styles.primary} onClick={onOpenClip}>
                Abrir o trecho
              </button>
            </div>
          </>
        ) : (
          <>
            <p className={styles.lead}>
              De <code>{formatDuration(range.a)}</code> a <code>{formatDuration(range.b)}</code> (
              {len.toFixed(3).replace(".", ",")} s). Vira um vídeo NOVO do caso; o original não é tocado.
            </p>
            <label className={styles.choice}>
              <input type="radio" checked={mode === "copy"} onChange={() => setMode("copy")} />
              <span>
                <strong>Sem recompressão (recomendado)</strong>
                <br />
                Quadros idênticos ao original, byte a byte. Começa no quadro-chave anterior à entrada e
                pode levar alguns quadros depois da saída — o SICRO informa quanto.
              </span>
            </label>
            <label className={styles.choice}>
              <input type="radio" checked={mode === "reencode"} onChange={() => setMode("reencode")} />
              <span>
                <strong>Recomprimir</strong>
                <br />
                Começa e termina exatamente nos quadros marcados, mas a imagem é recodificada (H.264 de
                alta qualidade) — os pixels deixam de ser os do original.
              </span>
            </label>
            <label className={styles.check}>
              <input
                type="checkbox"
                checked={audio && hasAudio}
                disabled={!hasAudio}
                onChange={(e) => setAudio(e.target.checked)}
              />{" "}
              {hasAudio ? "Incluir o áudio" : "Este vídeo não tem áudio"}
            </label>
            <div className={styles.actions}>
              <span className={styles.spacer} />
              <button type="button" onClick={onClose} disabled={busy}>
                Cancelar
              </button>
              <button
                type="button"
                className={styles.primary}
                onClick={() => onExport(mode, audio && hasAudio)}
                disabled={busy}
              >
                {busy ? "Exportando…" : "Exportar"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
