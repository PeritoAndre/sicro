/**
 * StatusBar — bottom strip with discreet technical info.
 *
 * doc 03 §6.6 — zoom/coordinates/page/selection/jobs/save status/errors.
 * Spike A surfaces only workspace path + last error.
 */

import { useWorkspaceStore } from "@stores/workspaceStore";
import styles from "./StatusBar.module.css";

export function StatusBar() {
  const path = useWorkspaceStore((s) => s.activeWorkspacePath);
  const error = useWorkspaceStore((s) => s.lastError);

  return (
    <footer className={styles.bar}>
      <span className={styles.item}>
        <span
          className={`${styles.dot} ${path ? styles.dotOk : styles.dotIdle}`}
          aria-hidden
        />
        {path ? "workspace ativo" : "nenhum workspace"}
      </span>
      {path && (
        <span className={styles.item} title={path}>
          {compactPath(path)}
        </span>
      )}
      <div className={styles.spacer} />
      {error && <span className={styles.error}>{error.message}</span>}
    </footer>
  );
}

function compactPath(p: string, max = 80): string {
  if (p.length <= max) return p;
  return "…" + p.slice(-(max - 1));
}
