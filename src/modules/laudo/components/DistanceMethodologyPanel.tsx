/**
 * DistanceMethodologyPanel — aba "Distância" do Inspetor do laudo.
 *
 * Espelha o SpeedMethodologyPanel: lista as medições de distância já feitas na
 * ocorrência (módulo de Vídeo, aba Medições) e, para a medição escolhida,
 * insere no laudo — como TEXTO editável — a "Seção de Metodologia — Estimativa
 * de Distância" gerada por `buildDistanceMethodologyContent`, via
 * `editor.chain().focus().insertContent(...)`.
 *
 * Conteúdo é texto (heading + parágrafos) → exporta confiável em HTML/PDF/DOCX
 * apesar da limitação de imagem do DOCX (KNOWN_LIMITATIONS §1).
 */

import { useEffect, useState } from "react";
import type { Editor } from "@tiptap/react";
import { Ruler } from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { formatDateTime } from "@core/formatters";
import type { VideoDistanceMeasurement } from "@domain/video_distance";
import { buildDistanceMethodologyContent } from "../document-engine/distance-methodology";
import styles from "./Inspector.module.css";

interface Props {
  editor: Editor | null;
  workspacePath: string | null;
}

export function DistanceMethodologyPanel({ editor, workspacePath }: Props) {
  const [items, setItems] = useState<VideoDistanceMeasurement[]>([]);
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
      .listDistanceMeasurementsForOccurrence(workspacePath)
      .then((list) => {
        if (!cancelled) setItems(list);
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

  const handleInsert = async (m: VideoDistanceMeasurement) => {
    if (!editor || !workspacePath) return;
    setError(null);
    setBusy(true);
    try {
      // Calibração é best-effort: se não recuperar, o gerador degrada com uma
      // nota explícita em vez de inventar dados.
      let calibration = null;
      try {
        calibration = await commands.getSpeedCalibration(
          workspacePath,
          m.calibration_id,
        );
      } catch {
        calibration = null;
      }
      const content = buildDistanceMethodologyContent(m, calibration);
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
          <Ruler size={14} /> Metodologia de distância
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
        metodologia de uma medição de distância feita no módulo de Vídeo —
        método, resultado (estimativa + intervalo, quando houver), ressalvas
        completas e dados de reprodutibilidade.
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
        <p className={styles.empty}>Carregando medições…</p>
      ) : items.length === 0 ? (
        <p className={styles.empty}>
          Nenhuma medição de distância nesta ocorrência. Faça uma no módulo de
          Vídeo (aba <strong>Medições</strong>) primeiro.
        </p>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {items.map((m) => (
            <div
              key={m.id}
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
                  {m.distance_m.toFixed(2).replace(".", ",")} m
                </strong>
                <span
                  style={{
                    fontSize: 11,
                    color: "var(--sicro-fg-dim)",
                    fontFamily: "var(--font-mono)",
                  }}
                >
                  {m.mc_seed != null ? "Monte Carlo" : "pontual"}
                </span>
              </div>
              <span style={{ fontSize: 11, color: "var(--sicro-fg-dim)" }}>
                {formatDateTime(m.created_at)}
              </span>
              <Button
                variant="primary"
                size="sm"
                onClick={() => void handleInsert(m)}
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
