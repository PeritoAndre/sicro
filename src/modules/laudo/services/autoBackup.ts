/**
 * autoBackup — serviço de auto-backup local persistido em IndexedDB.
 *
 * F12.4 — Diferente dos snapshots (que são manuais, em-doc), o auto-
 * backup roda em background, captura periodicamente, e fica em
 * IndexedDB (cross-session). Útil para "crash recovery": se o app
 * crashar antes do save, o user recupera do auto-backup.
 *
 * Buffer rolling de 10 entradas por laudo (chaveado por laudoId).
 */

import type { JSONContent } from "@tiptap/core";

const DB_NAME = "sicro-laudo-autobackup";
const DB_VERSION = 1;
const STORE_NAME = "backups";
const MAX_BACKUPS_PER_LAUDO = 10;

export interface AutoBackupEntry {
  laudoId: string;
  capturedAt: string; // ISO
  content: JSONContent;
  wordCount?: number;
}

let dbPromise: Promise<IDBDatabase> | null = null;

function openDB(): Promise<IDBDatabase> {
  if (dbPromise) return dbPromise;
  dbPromise = new Promise((resolve, reject) => {
    if (typeof indexedDB === "undefined") {
      reject(new Error("IndexedDB not available"));
      return;
    }
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onerror = () => reject(req.error);
    req.onsuccess = () => resolve(req.result);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        const store = db.createObjectStore(STORE_NAME, {
          keyPath: "id",
          autoIncrement: true,
        });
        store.createIndex("laudoId", "laudoId", { unique: false });
        store.createIndex("capturedAt", "capturedAt", { unique: false });
      }
    };
  });
  return dbPromise;
}

/** Salva uma nova entrada de auto-backup e poda excessos. */
export async function saveAutoBackup(entry: AutoBackupEntry): Promise<void> {
  try {
    const db = await openDB();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, "readwrite");
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
      tx.objectStore(STORE_NAME).add(entry);
    });
    // Poda: mantém só os 10 mais recentes deste laudo.
    await pruneAutoBackups(entry.laudoId);
  } catch (err) {
    // eslint-disable-next-line no-console
    console.warn("[autoBackup] save failed", err);
  }
}

/** Lista backups de um laudo, mais recente primeiro. */
export async function listAutoBackups(laudoId: string): Promise<
  Array<AutoBackupEntry & { id: number }>
> {
  try {
    const db = await openDB();
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, "readonly");
      const store = tx.objectStore(STORE_NAME);
      const index = store.index("laudoId");
      const req = index.getAll(laudoId);
      req.onerror = () => reject(req.error);
      req.onsuccess = () => {
        const all = (req.result ?? []) as Array<
          AutoBackupEntry & { id: number }
        >;
        // Ordena descendente por capturedAt.
        all.sort((a, b) => b.capturedAt.localeCompare(a.capturedAt));
        resolve(all);
      };
    });
  } catch (err) {
    // eslint-disable-next-line no-console
    console.warn("[autoBackup] list failed", err);
    return [];
  }
}

/** Remove backups mais antigos que o limite por laudo. */
export async function pruneAutoBackups(laudoId: string): Promise<void> {
  try {
    const all = await listAutoBackups(laudoId);
    if (all.length <= MAX_BACKUPS_PER_LAUDO) return;
    const toDelete = all.slice(MAX_BACKUPS_PER_LAUDO);
    const db = await openDB();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, "readwrite");
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
      const store = tx.objectStore(STORE_NAME);
      for (const e of toDelete) {
        store.delete(e.id);
      }
    });
  } catch {
    /* defensive */
  }
}

/** Deleta TODOS os backups de um laudo (chamado quando o laudo é excluído). */
export async function clearAutoBackups(laudoId: string): Promise<void> {
  try {
    const all = await listAutoBackups(laudoId);
    if (all.length === 0) return;
    const db = await openDB();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, "readwrite");
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
      const store = tx.objectStore(STORE_NAME);
      for (const e of all) {
        store.delete(e.id);
      }
    });
  } catch {
    /* defensive */
  }
}
