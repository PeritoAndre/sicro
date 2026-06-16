/**
 * O3 — Hook que conecta o evento `onDragDropEvent` do Tauri ao editor de
 * laudo. Mantém o estado da operação (`idle | hover | uploading | done |
 * error`), as coordenadas atuais do mouse durante o hover (pra orientar
 * o overlay visual), e os resultados da última importação.
 *
 * O hook NÃO insere as fotos no editor — ele só importa pra workspace e
 * dispara o callback `onImported(photos, dropPosition)`. Quem decide
 * onde inserir é o EditorPage (via `view.posAtCoords`).
 *
 * Tauri 2.x: `getCurrentWebview().onDragDropEvent` retorna uma função
 * de unsubscribe. Cuidamos disso no cleanup do useEffect.
 *
 * Quando ENABLED é falso, o hook não registra o listener — útil pra
 * desabilitar drag-and-drop em modo "header edit" ou "leitura".
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type {
  ImportedPhoto,
  PhotoImportResult,
} from "@domain/photo_drop";

export type DragState =
  | "idle"
  | "hover"
  | "uploading"
  | "done"
  | "error";

export interface DragPosition {
  /** X em pixels relativo à viewport (clientX). */
  x: number;
  /** Y em pixels relativo à viewport (clientY). */
  y: number;
}

export interface UseDragDropPhotosOptions {
  /** True quando o drag-and-drop deve estar ativo. Quando false,
   *  nenhum listener é registrado. */
  enabled: boolean;
  /** Workspace ativo (path absoluto). */
  workspacePath: string | null;
  /** Laudo ativo (id). */
  laudoId: string | null;
  /** Chamado APÓS a importação backend ter sucesso. O caller insere
   *  as fotos no editor na posição informada (que é a posição do
   *  mouse no momento do drop). */
  onImported: (photos: ImportedPhoto[], dropPos: DragPosition) => void;
  /** Chamado quando alguma foto do lote falha ou o command rejeita.
   *  O caller decide UI (toast, dialog, etc). */
  onErrors?: (errors: PhotoImportResult["errors"]) => void;
}

export interface DragDropPhotosState {
  /** Estado atual da operação. */
  state: DragState;
  /** Coordenadas do mouse durante hover/drop. `null` em outros estados. */
  position: DragPosition | null;
  /** Última mensagem de erro humana (quando state === "error"). */
  errorMessage: string | null;
  /** Limpa um estado terminal (done | error). Volta pra idle. */
  reset: () => void;
}

export function useDragDropPhotos(
  options: UseDragDropPhotosOptions,
): DragDropPhotosState {
  const { enabled, workspacePath, laudoId, onImported, onErrors } = options;

  const [state, setState] = useState<DragState>("idle");
  const [position, setPosition] = useState<DragPosition | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // Refs pra evitar reassinatura do listener quando o caller passar
  // callbacks inline.
  const onImportedRef = useRef(onImported);
  const onErrorsRef = useRef(onErrors);
  onImportedRef.current = onImported;
  onErrorsRef.current = onErrors;

  const reset = useCallback(() => {
    setState("idle");
    setPosition(null);
    setErrorMessage(null);
  }, []);

  useEffect(() => {
    if (!enabled || !workspacePath || !laudoId) {
      setState("idle");
      setPosition(null);
      return;
    }

    let unlisten: (() => void) | null = null;
    let cancelled = false;

    const setup = async () => {
      try {
        const webview = getCurrentWebview();
        // Tauri 2.x event: { type: "enter" | "over" | "leave" | "drop",
        //   paths?: string[], position: { x, y } }
        const subscription = await webview.onDragDropEvent(async (event) => {
          const payload = event.payload;
          // Tauri usa `payload.type` (não `event.event`) pra distinguir.
          switch (payload.type) {
            case "enter":
            case "over": {
              setState("hover");
              if (payload.position) {
                setPosition({
                  x: payload.position.x,
                  y: payload.position.y,
                });
              }
              break;
            }
            case "leave": {
              setState("idle");
              setPosition(null);
              break;
            }
            case "drop": {
              const filePaths = Array.isArray(payload.paths)
                ? payload.paths
                : [];
              const dropPos = payload.position
                ? {
                    x: payload.position.x,
                    y: payload.position.y,
                  }
                : { x: 0, y: 0 };
              setPosition(dropPos);
              if (filePaths.length === 0) {
                setState("idle");
                return;
              }
              setState("uploading");
              setErrorMessage(null);
              try {
                const result = await commands.importDraggedPhotosToLaudo(
                  workspacePath,
                  laudoId,
                  filePaths,
                );
                if (result.imported.length > 0) {
                  onImportedRef.current(result.imported, dropPos);
                }
                if (result.errors.length > 0 && onErrorsRef.current) {
                  onErrorsRef.current(result.errors);
                }
                setState(
                  result.imported.length > 0 ? "done" : "error",
                );
                if (result.imported.length === 0 && result.errors.length > 0) {
                  setErrorMessage(
                    `Nenhuma foto válida no lote (${result.errors.length} arquivo(s) rejeitado(s)).`,
                  );
                }
              } catch (err) {
                const e = toSicroError(err);
                setErrorMessage(e.message);
                setState("error");
              }
              break;
            }
          }
        });
        if (cancelled) {
          subscription();
        } else {
          unlisten = subscription;
        }
      } catch (err) {
        // Provavelmente o webview não está acessível (ex: testes vitest).
        // Falha silenciosa — o drag-and-drop não funciona, mas o app
        // segue.
        // eslint-disable-next-line no-console
        console.warn("[useDragDropPhotos] couldn't register listener", err);
      }
    };
    void setup();

    return () => {
      cancelled = true;
      if (unlisten) {
        try {
          unlisten();
        } catch {
          // best-effort
        }
      }
    };
  }, [enabled, workspacePath, laudoId]);

  return { state, position, errorMessage, reset };
}
