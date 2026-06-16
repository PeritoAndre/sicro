/**
 * Card de Configurações — Gerenciador de OCR (Documentoscopia).
 *
 * Motor: RapidOCR/PaddleOCR (PP-OCRv5, ONNX Runtime **embutido** no app —
 * offline, sem Python). O perito baixa, com um clique, o **pacote latino**
 * (det + rec-latino + dicionário, ~13 MB) que cobre PT/ES/EN/FR/IT… Fonte
 * oficial (Apache-2.0); progresso + verificação por hash no backend.
 */

import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { CheckCircle2, Download, FileScan, RefreshCw, Trash2 } from "lucide-react";

import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type {
  OcrCatalog,
  OcrPackItem,
  OcrProgress,
  OcrStatus,
  OcrUpdateInfo,
} from "@domain/ocr";
import styles from "./AiManagerCard.module.css";

export function OcrManagerCard() {
  const [catalog, setCatalog] = useState<OcrCatalog | null>(null);
  const [status, setStatus] = useState<OcrStatus | null>(null);
  const [progress, setProgress] = useState<OcrProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [update, setUpdate] = useState<OcrUpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [c, s] = await Promise.all([
        commands.getOcrCatalog(),
        commands.getOcrStatus(),
      ]);
      setCatalog(c);
      setStatus(s);
    } catch (e) {
      setError(toSicroError(e).message);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let un: (() => void) | undefined;
    void listen<OcrProgress>("ocr-download-progress", (e) => {
      setProgress(e.payload);
    }).then((u) => {
      un = u;
    });
    return () => un?.();
  }, []);

  const install = async (item: OcrPackItem) => {
    setBusy(true);
    setError(null);
    setProgress({ id: item.id, received: 0, total: 0 });
    try {
      setStatus(await commands.installOcrAsset(item.id));
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const remove = async (item: OcrPackItem) => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await commands.removeOcrAsset(item.id));
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(false);
    }
  };

  // OPT-IN (§13): só consulta a release mais nova do oar-ocr e informa.
  const checkUpdates = async () => {
    setChecking(true);
    setError(null);
    try {
      setUpdate(await commands.checkOcrUpdates());
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setChecking(false);
    }
  };

  // OPT-IN: baixa o pacote da última release e re-verifica. §13: ação do perito;
  // a versão usada fica registrada (AppSettings.ocr.ocr_version).
  const updateModels = async (item: OcrPackItem) => {
    setBusy(true);
    setError(null);
    setProgress({ id: item.id, received: 0, total: 0 });
    try {
      setStatus(await commands.updateOcrModels());
      setUpdate(await commands.checkOcrUpdates());
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const pack = catalog?.items[0];
  const ready = status?.models_ready ?? false;

  return (
    <div className={styles.card}>
      <div className={styles.head}>
        <FileScan size={16} aria-hidden />
        <span className={styles.title}>Motor de OCR (Documentoscopia)</span>
      </div>
      <p className={styles.desc}>
        O OCR transforma documentos escaneados/fotos em texto, com um{" "}
        {/* Motor: RapidOCR/PaddleOCR (PP-OCRv5, ONNX Runtime embutido). UI neutra. */}
        <strong>motor de OCR embutido</strong> — 100% offline, sem dependências
        externas. Baixe uma vez o pacote de idiomas latino (cobre português, espanhol,
        inglês…). PDFs digitais nem precisam de OCR. O motor acompanha as
        atualizações do SICRO e o pacote de modelos é curado — “Verificar
        atualizações” só sugere troca quando há um pacote compatível mais novo.
      </p>

      <div className={styles.statusRow}>
        <span className={status?.engine_ready ? styles.ok : styles.off}>
          <CheckCircle2 size={13} /> Motor: {status?.engine_label || "embutido"}
        </span>
        <span className={ready ? styles.ok : styles.off}>
          {ready ? "Modelos instalados" : "Modelos não baixados"}
        </span>
      </div>

      {error && <div className={styles.error}>{error}</div>}

      <div className={styles.group}>Pacote de modelos</div>
      {pack && (
        <div className={styles.row}>
          <div className={styles.rowMain}>
            <span className={styles.rowLabel}>
              {pack.label}{" "}
              {ready && <span className={styles.tagOk}>instalado</span>}
            </span>
            <span className={styles.rowNote}>
              {pack.note} · ~{pack.approx_mb} MB
            </span>
            {progress && (
              <div className={styles.progress}>
                <div
                  className={styles.progressFill}
                  style={{
                    width: progress.total
                      ? `${Math.round((progress.received / progress.total) * 100)}%`
                      : "40%",
                  }}
                />
                <span className={styles.progressTxt}>
                  {Math.round(progress.received / 1e6)}
                  {progress.total ? `/${Math.round(progress.total / 1e6)}` : ""} MB
                </span>
              </div>
            )}
          </div>
          <div className={styles.rowActions}>
            {ready ? (
              <>
                {update?.update_available && (
                  <Button
                    variant="primary"
                    size="sm"
                    leftIcon={<Download size={14} />}
                    onClick={() => void updateModels(pack)}
                    disabled={busy}
                    title={`Atualizar para ${update.latest}`}
                  >
                    {busy ? "Atualizando…" : "Atualizar"}
                  </Button>
                )}
                <Button
                  variant="ghost"
                  size="sm"
                  leftIcon={<Trash2 size={14} />}
                  onClick={() => void remove(pack)}
                  disabled={busy}
                >
                  Remover
                </Button>
              </>
            ) : (
              <Button
                variant="secondary"
                size="sm"
                leftIcon={<Download size={14} />}
                onClick={() => void install(pack)}
                disabled={busy}
              >
                {busy ? "Baixando…" : "Baixar"}
              </Button>
            )}
          </div>
        </div>
      )}

      <div className={styles.footer}>
        <Button
          variant="ghost"
          size="sm"
          leftIcon={<RefreshCw size={13} />}
          onClick={() => void checkUpdates()}
          disabled={checking}
        >
          {checking ? "Verificando…" : "Verificar atualizações"}
        </Button>
        {update && (
          <span className={styles.updateInfo}>
            {update.update_available
              ? `Nova versão dos modelos (oar-ocr) disponível: ${update.latest} (você tem ${update.current}).`
              : `Você está na versão mais recente (${update.current}).`}
          </span>
        )}
      </div>
    </div>
  );
}
