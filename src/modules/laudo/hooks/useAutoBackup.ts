/**
 * useAutoBackup — captura snapshots periódicos no IndexedDB.
 *
 * F12.4 — Roda em background a cada `intervalMs` (default 30s) E ao
 * unmount. Buffer rolling de 10 por laudo (gerenciado pelo service).
 *
 * Independente do sistema de snapshots manuais — esse vive no doc.
 * O auto-backup vive no IndexedDB do navegador, ideal pra crash recovery.
 */

import { useEffect, useRef } from "react";
import type { Editor } from "@tiptap/react";
import { saveAutoBackup } from "../services/autoBackup";

interface UseAutoBackupOptions {
  editor: Editor | null;
  laudoId: string | null;
  intervalMs?: number;
  enabled?: boolean;
}

export function useAutoBackup({
  editor,
  laudoId,
  intervalMs = 30_000,
  enabled = true,
}: UseAutoBackupOptions): void {
  const lastContentRef = useRef<string>("");

  useEffect(() => {
    if (!editor || !laudoId || !enabled) return undefined;

    const tick = () => {
      const json = editor.getJSON();
      const serialized = JSON.stringify(json);
      // Skip se nada mudou desde último backup.
      if (serialized === lastContentRef.current) return;
      lastContentRef.current = serialized;
      const text = editor.getText();
      const wordCount = text.trim()
        ? text.trim().split(/\s+/).length
        : 0;
      void saveAutoBackup({
        laudoId,
        capturedAt: new Date().toISOString(),
        content: json,
        wordCount,
      });
    };

    // Captura inicial após pequeno delay (deixa o editor settle).
    const initial = window.setTimeout(tick, 5_000);
    const interval = window.setInterval(tick, intervalMs);

    return () => {
      window.clearTimeout(initial);
      window.clearInterval(interval);
      // Captura final no unmount (best-effort).
      try {
        tick();
      } catch {
        /* defensive */
      }
    };
  }, [editor, laudoId, intervalMs, enabled]);
}
