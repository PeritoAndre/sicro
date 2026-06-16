/**
 * AiManagerCard — gerenciador de IA (Fase 2.1) nas Configurações.
 *
 * Baixa o motor whisper.cpp + modelos SOB DEMANDA (catálogo curado), com barra
 * de progresso e verificação de hash no backend, e auto-configura os caminhos
 * em AppSettings. Opt-in, offline-first. "Verificar atualizações" só informa.
 */

import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Bot, CheckCircle2, Download, RefreshCw, Trash2 } from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { useSettingsStore } from "@stores/settingsStore";
import type {
  AiCatalog,
  AiProgress,
  AiStatus,
  AiUpdateInfo,
  CatalogItem,
} from "@domain/ai";
import styles from "./AiManagerCard.module.css";

function prettyMB(bytes: number): string {
  return `${(bytes / 1024 / 1024).toFixed(0)} MB`;
}

export function AiManagerCard() {
  const [catalog, setCatalog] = useState<AiCatalog | null>(null);
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [progress, setProgress] = useState<Record<string, AiProgress>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [update, setUpdate] = useState<AiUpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);

  const reloadStatus = useCallback(async () => {
    try {
      setStatus(await commands.getAiStatus());
    } catch {
      /* best-effort */
    }
  }, []);

  useEffect(() => {
    void commands
      .getAiCatalog()
      .then(setCatalog)
      .catch((e) => setError(toSicroError(e).message));
    void reloadStatus();
  }, [reloadStatus]);

  useEffect(() => {
    let un: (() => void) | undefined;
    void listen<AiProgress>("ai-download-progress", (e) => {
      setProgress((prev) => ({ ...prev, [e.payload.id]: e.payload }));
    }).then((u) => {
      un = u;
    });
    return () => un?.();
  }, []);

  const install = async (item: CatalogItem) => {
    setBusy(item.id);
    setError(null);
    setProgress((p) => ({ ...p, [item.id]: { id: item.id, received: 0, total: 0 } }));
    try {
      const st = await commands.installAiAsset(item.id);
      setStatus(st);
      // A tela de degravação passa a enxergar os caminhos auto-configurados.
      await useSettingsStore.getState().load();
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
      setProgress((p) => {
        const n = { ...p };
        delete n[item.id];
        return n;
      });
    }
  };

  const remove = async (item: CatalogItem) => {
    setBusy(item.id);
    setError(null);
    try {
      setStatus(await commands.removeAiAsset(item.id));
      await useSettingsStore.getState().load();
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
    }
  };

  const checkUpdates = async () => {
    setChecking(true);
    setError(null);
    try {
      setUpdate(await commands.checkAiUpdates());
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setChecking(false);
    }
  };

  // OPT-IN: baixa a última release do motor e troca pela instalada (§13: o
  // perito escolhe; a versão usada fica registrada no log da degravação).
  const updateEngine = async () => {
    const installedBuild = catalog?.items.find(
      (i) => i.kind === "build" && !!status?.whisper_bin_path.includes(i.id),
    );
    const pid = installedBuild?.id ?? "whisper-update";
    setBusy(pid);
    setError(null);
    setProgress((p) => ({ ...p, [pid]: { id: pid, received: 0, total: 0 } }));
    try {
      setStatus(await commands.updateWhisperEngine());
      await useSettingsStore.getState().load();
      setUpdate(await commands.checkAiUpdates());
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(null);
      setProgress((p) => {
        const n = { ...p };
        delete n[pid];
        return n;
      });
    }
  };

  const isInstalled = (item: CatalogItem): boolean => {
    if (!status) return false;
    if (item.kind === "build") {
      return status.whisper_ok && status.whisper_bin_path.includes(item.id);
    }
    return status.installed_models.some((m) => m.filename === item.filename);
  };

  const builds = catalog?.items.filter((i) => i.kind === "build") ?? [];
  // "Modelos" inclui os de transcrição e o VAD (anti-alucinação).
  const models = catalog?.items.filter((i) => i.kind !== "build") ?? [];

  return (
    <section className={styles.card}>
      <div className={styles.head}>
        <Bot size={18} aria-hidden />
        <h2 className={styles.title}>Degravação por IA (local)</h2>
      </div>
      <p className={styles.desc}>
        Opcional e 100% offline. Baixe o motor whisper.cpp e um modelo uma vez — o
        SICRO configura tudo sozinho. A degravação é sempre um{" "}
        <strong>rascunho</strong> que você revisa; nada é instalado ou trocado
        automaticamente.
      </p>

      <div className={styles.statusRow}>
        <span className={status?.whisper_ok ? styles.ok : styles.off}>
          {status?.whisper_ok && <CheckCircle2 size={13} aria-hidden />} Motor:{" "}
          {status?.whisper_ok
            ? `instalado (${status.whisper_version || "?"})`
            : "não instalado"}
        </span>
        <span className={status?.model_ok ? styles.ok : styles.off}>
          {status?.model_ok && <CheckCircle2 size={13} aria-hidden />} Modelo:{" "}
          {status?.model_ok
            ? (status.model_path.split(/[\\/]/).pop() ?? "ok")
            : "nenhum"}
        </span>
      </div>

      {error && <div className={styles.error}>{error}</div>}

      <h3 className={styles.group}>Motor (whisper.cpp)</h3>
      {builds.map((item) => (
        <AssetRow
          key={item.id}
          item={item}
          catalog={catalog}
          installed={isInstalled(item)}
          busy={busy}
          progress={progress[item.id]}
          updateAvailable={!!update?.update_available}
          updateLatest={update?.latest}
          onInstall={() => void install(item)}
          onRemove={() => void remove(item)}
          onUpdate={() => void updateEngine()}
        />
      ))}

      <h3 className={styles.group}>Modelos</h3>
      {models.map((item) => (
        <AssetRow
          key={item.id}
          item={item}
          catalog={catalog}
          installed={isInstalled(item)}
          busy={busy}
          progress={progress[item.id]}
          onInstall={() => void install(item)}
          onRemove={() => void remove(item)}
        />
      ))}

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
              ? `Nova versão do whisper.cpp disponível: ${update.latest} (você tem ${update.current}).`
              : `Você está na versão mais recente (${update.current}).`}
          </span>
        )}
      </div>
    </section>
  );
}

function AssetRow({
  item,
  catalog,
  installed,
  busy,
  progress,
  updateAvailable,
  updateLatest,
  onInstall,
  onRemove,
  onUpdate,
}: {
  item: CatalogItem;
  catalog: AiCatalog | null;
  installed: boolean;
  busy: string | null;
  progress?: AiProgress;
  updateAvailable?: boolean;
  updateLatest?: string;
  onInstall: () => void;
  onRemove: () => void;
  onUpdate?: () => void;
}) {
  const isBusy = busy === item.id;
  const recommended = item.kind === "build" && item.gpu && !!catalog?.gpu_detected;
  const pct =
    progress && progress.total > 0
      ? Math.round((progress.received / progress.total) * 100)
      : null;

  return (
    <div className={styles.row}>
      <div className={styles.rowMain}>
        <div className={styles.rowLabel}>
          {item.label}
          {recommended && <span className={styles.tag}>recomendado p/ sua GPU</span>}
          {installed && <span className={styles.tagOk}>instalado</span>}
        </div>
        <div className={styles.rowNote}>
          ≈ {item.approx_mb} MB · {item.note}
        </div>
        {isBusy && (
          <div className={styles.progress}>
            <div className={styles.progressFill} style={{ width: `${pct ?? 0}%` }} />
            <span className={styles.progressTxt}>
              {pct != null ? `${pct}%` : "baixando…"}
              {progress && progress.total > 0
                ? ` (${prettyMB(progress.received)} / ${prettyMB(progress.total)})`
                : ""}
            </span>
          </div>
        )}
      </div>
      <div className={styles.rowActions}>
        {installed ? (
          <>
            {item.kind === "build" && updateAvailable && onUpdate && (
              <Button
                variant="primary"
                size="sm"
                leftIcon={<Download size={13} />}
                onClick={onUpdate}
                disabled={busy !== null}
                title={updateLatest ? `Atualizar para ${updateLatest}` : "Atualizar motor"}
              >
                {isBusy ? "Atualizando…" : "Atualizar"}
              </Button>
            )}
            <Button
              variant="ghost"
              size="sm"
              leftIcon={<Trash2 size={13} />}
              onClick={onRemove}
              disabled={busy !== null}
            >
              Remover
            </Button>
          </>
        ) : (
          <Button
            variant="secondary"
            size="sm"
            leftIcon={<Download size={13} />}
            onClick={onInstall}
            disabled={busy !== null}
          >
            {isBusy ? "Baixando…" : "Baixar"}
          </Button>
        )}
      </div>
    </div>
  );
}
