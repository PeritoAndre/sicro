/**
 * Auto-backup ao fechar/trocar a ocorrência: backup geral incremental para a
 * pasta configurada, sem bloquear o app. Fechar o APP com um caso aberto não
 * dispara (não seguramos o encerramento).
 */

import { commands } from "@core/commands";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { pushToast, dismissToast } from "@/components/toast/toastStore";
import type { CaseIndexEntry } from "@domain/case_index";

const DEST_KEY = "sicro.globalBackup.destination";
const AUTO_KEY = "sicro.globalBackup.autoOnClose";
const LAST_KEY = "sicro.globalBackup.lastAuto";

/** Mesma chave do card de Backup geral. */
function getBackupDestination(): string | null {
  return localStorage.getItem(DEST_KEY);
}

/** Ligado por padrão; só AGE quando há um destino configurado. */
export function isAutoBackupOnCloseEnabled(): boolean {
  return localStorage.getItem(AUTO_KEY) !== "0";
}

export function setAutoBackupOnClose(enabled: boolean): void {
  localStorage.setItem(AUTO_KEY, enabled ? "1" : "0");
}

/** ISO da última vez que o auto-backup concluiu (ou null). */
export function getLastAutoBackupAt(): string | null {
  return localStorage.getItem(LAST_KEY);
}

function caseLabel(e: CaseIndexEntry): string {
  const parts = [
    e.numero_bo ? `BO ${e.numero_bo}` : null,
    e.tipo_pericia,
    e.municipio,
  ].filter((p): p is string => !!p);
  return parts.length
    ? parts.join(" — ")
    : `Ocorrência ${e.workspace_id.slice(0, 8)}`;
}

let running = false;

/** No-op silencioso se não há destino ou a opção está desligada. */
async function runAutoBackup(): Promise<void> {
  if (running) return;
  const dest = getBackupDestination();
  if (!dest || !isAutoBackupOnCloseEnabled()) return;
  running = true;
  let toastId: number | null = null;
  try {
    const idx = await commands.getCaseIndex();
    if (idx.length === 0) return;
    toastId = pushToast("progress", "Backup automático em andamento…", {
      title: "Backup",
    });
    const cases = idx.map((e) => ({
      workspace_path: e.workspace_path,
      label: caseLabel(e),
    }));
    const rep = await commands.generateGlobalBackup(cases, dest);
    localStorage.setItem(LAST_KEY, new Date().toISOString());
    if (toastId !== null) {
      dismissToast(toastId);
      toastId = null;
    }
    pushToast(
      "success",
      rep.backed_up > 0
        ? `Backup automático: ${rep.backed_up} caso(s) atualizado(s) na pasta de backup.`
        : "Backup automático: tudo já estava em dia.",
      { title: "Backup" },
    );
  } catch (e) {
    if (toastId !== null) dismissToast(toastId);
    pushToast("error", `Falha no backup automático: ${String(e)}`, {
      title: "Backup",
    });
  } finally {
    running = false;
  }
}

/** Dispara ao sair de uma ocorrência; devolve o unsubscribe. */
export function installAutoBackupWatcher(): () => void {
  return useWorkspaceStore.subscribe((state, prev) => {
    const left = prev.activeWorkspacePath;
    if (left && left !== state.activeWorkspacePath) {
      void runAutoBackup();
    }
  });
}
