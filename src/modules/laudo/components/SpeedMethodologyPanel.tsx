/**
 * SpeedMethodologyPanel — aba "Velocidade" do Inspetor do laudo.
 *
 * Lista os cálculos de velocidade já realizados na ocorrência (módulo de
 * Vídeo) e, para o cálculo escolhido, insere no laudo — como TEXTO editável —
 * a "Seção de Metodologia — Estimativa de Velocidade" gerada por
 * `buildSpeedMethodologyContent`. Usa o mesmo caminho de inserção dos blocos
 * reutilizáveis (`editor.chain().focus().insertContent(...)`).
 *
 * O conteúdo é texto (heading + parágrafos); por isso exporta de forma
 * confiável em HTML/PDF/DOCX, independentemente da limitação de imagem do
 * DOCX (KNOWN_LIMITATIONS §1).
 */

import { useEffect, useState } from "react";
import type { Editor } from "@tiptap/react";
import { Gauge } from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { formatDateTime } from "@core/formatters";
import type { VideoSpeedCalculation } from "@domain/video_speed";
import { buildSpeedMethodologyContent } from "../document-engine/speed-methodology";
import styles from "./Inspector.module.css";

interface Props {
  editor: Editor | null;
  workspacePath: string | null;
}

export function SpeedMethodologyPanel({ editor, workspacePath }: Props) {
  const [calcs, setCalcs] = useState<VideoSpeedCalculation[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!workspacePath) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    commands
      .listSpeedCalculationsForOccurrence(workspacePath)
      .then((list) => {
        if (!cancelled) setCalcs(list);
      })
      .catch((e) => {
        if (!cancelled) setError(toSicroError(e).message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [workspacePath]);

  const handleInsert = async (calc: VideoSpeedCalculation) => {
    if (!editor || !workspacePath) return;
    setError(null);
    setBusy(true);
    try {
      // Calibração é best-effort: se não recuperar, o gerador degrada com
      // uma nota explícita em vez de inventar dados.
      let calibration = null;
      try {
        calibration = await commands.getSpeedCalibration(
          workspacePath,
          calc.calibration_id,
        );
      } catch {
        calibration = null;
      }
      const content = buildSpeedMethodologyContent(calc, calibration);
      editor.chain().focus().insertContent(content).run();
      setFeedback("Seção de metodologia inserida no laudo (texto editável).");
      setTimeout(() => setFeedback(null), 3500);
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(false);
    }
  };

  if (!editor) {
    return (
      <p className={styles.empty}>Abra um laudo para inserir a metodologia.</p>
    );
  }
  if (!workspacePath) {
    return <p className={styles.empty}>Sem workspace ativo.</p>;
  }

  return (
    <>
      <h3 className={styles.sectionTitle}>
        <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
          <Gauge size={14} /> Metodologia de velocidade
        </span>
      </h3>
      <p
        style={{
          fontSize: "var(--text-xs)",
          color: "var(--sicro-fg-muted)",
          lineHeight: 1.5,
          marginTop: 0,
        }}
      >
        Insere no laudo, como <strong>texto editável</strong>, a seção de
        metodologia de um cálculo de velocidade feito no módulo de Vídeo —
        método, resultado (estimativa + intervalos), ressalvas completas e
        dados de reprodutibilidade.
      </p>

      {feedback && (
        <p style={{ fontSize: "var(--text-xs)", color: "var(--sicro-success)", margin: 0 }}>
          {feedback}
        </p>
      )}
      {error && (
        <p style={{ fontSize: "var(--text-xs)", color: "var(--sicro-danger)", margin: 0 }}>
          {error}
        </p>
      )}

      {loading ? (
        <p className={styles.empty}>Carregando cálculos…</p>
      ) : calcs.length === 0 ? (
        <p className={styles.empty}>
          Nenhum cálculo de velocidade nesta ocorrência. Faça um no módulo de
          Vídeo (aba <strong>Velocidade</strong>) primeiro.
        </p>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {calcs.map((c) => (
            <div
              key={c.id}
              style={{
                border: "1px solid var(--sicro-border)",
                borderRadius: "var(--radius-sm)",
                padding: 8,
                display: "flex",
                flexDirection: "column",
                gap: 4,
              }}
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "baseline",
                  gap: 8,
                }}
              >
                <strong style={{ fontSize: "var(--text-sm)", color: "var(--sicro-fg)" }}>
                  {c.velocity_kmh.toFixed(1)} km/h
                </strong>
                <span
                  style={{
                    fontSize: 11,
                    color: "var(--sicro-fg-dim)",
                    fontFamily: "var(--font-mono)",
                  }}
                >
                  {c.points.length} pts
                  {c.mc_seed != null ? " · Monte Carlo" : ""}
                </span>
              </div>
              <span style={{ fontSize: 11, color: "var(--sicro-fg-dim)" }}>
                {formatDateTime(c.created_at)}
              </span>
              <Button
                variant="primary"
                size="sm"
                onClick={() => void handleInsert(c)}
                disabled={busy}
              >
                Inserir no laudo
              </Button>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
