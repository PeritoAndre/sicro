/**
 * Auto-backup ao FECHAR/TROCAR a ocorrência (Fase 2b do DR).
 *
 * Quando o perito sai de uma ocorrência (volta pra Home ou abre outra) e existe
 * uma "pasta de backup" configurada + a opção ligada, dispara o BACKUP GERAL
 * INCREMENTAL para essa pasta. Como é incremental, re-zipa só o que mudou — na
 * prática, o caso que acabou de fechar — e atualiza o snapshot de config.
 *
 * §13: não bloqueia o app (fire-and-forget, com guarda contra concorrência); o
 * backup é estático (sincroniza com segurança); o workspace original local nunca
 * é tocado. Honesto: avisa por toast e registra a hora do último auto-backup.
 *
 * Limite conhecido: cobre fechar/trocar a ocorrência. Fechar o APP com um caso
 * aberto não dispara (não bloqueamos o encerramento) — feche a ocorrência ou
 * use o backup manual antes de sair.
 */

import { commands } from "@core/commands";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { pushToast, dismissToast } from "@/components/toast/toastStore";
import type { CaseIndexEntry } from "@domain/case_index";

const DEST_KEY = "sicro.globalBackup.destination";
const AUTO_KEY = "sicro.globalBackup.autoOnClose";
const LAST_KEY = "sicro.globalBackup.lastAuto";

/** Pasta de backup configurada (mesma chave do card de Backup geral). */
export function getBackupDestination(): string | null {
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

/**
 * Roda o backup geral incremental para a pasta de backup configurada.
 * No-op silencioso se não há destino ou a opção está desligada.
 */
export async function runAutoBackup(): Promise<void> {
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

/**
 * Instala o observador: ao SAIR de uma ocorrência (activeWorkspacePath deixa de
 * apontar para o caso anterior), dispara o auto-backup. Retorna a função de
 * cleanup (unsubscribe).
 */
export function installAutoBackupWatcher(): () => void {
  return useWorkspaceStore.subscribe((state, prev) => {
    const left = prev.activeWorkspacePath;
    if (left && left !== state.activeWorkspacePath) {
      void runAutoBackup();
    }
  });
}
