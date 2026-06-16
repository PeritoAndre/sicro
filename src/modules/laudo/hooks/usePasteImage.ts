/**
 * T — Paste (Ctrl+V) de fotos no editor de laudo.
 *
 * Espelho do `useDragDropPhotos` mas pra clipboard. Cobre dois casos:
 *   1. Bitmap raw do clipboard (screenshot do Windows ↦ Ctrl+V, "copy
 *      image" no browser ↦ Ctrl+V).
 *   2. Arquivo copiado no Windows Explorer (Ctrl+C numa foto ↦ Ctrl+V),
 *      que chega no `DataTransfer.files` como `File` object.
 *
 * Por que listener DOM e não `editorProps.handlePaste`?
 *   - `editorProps` é capturado no `useEditor()`, então closures-stale
 *     com workspacePath/laudoId que mudam fora do ciclo do laudo seriam
 *     problemáticas (precisaria refs, etc.).
 *   - Listener DOM com `capture: true` roda ANTES do handler do
 *     ProseMirror, então um `event.preventDefault()` na fase de captura
 *     bloqueia a inserção default do PM. Isso é o que queremos: o PM
 *     NÃO deve tentar inserir nada — nós controlamos a inserção via
 *     `insertFigure` no callback `onImported`.
 *
 * O hook NÃO insere as fotos no editor — só importa pra workspace e
 * dispara `onImported(photos)`. Quem decide onde inserir é o
 * LaudoEditorView (na seleção atual do cursor, em vez de coordenadas
 * do mouse como no drop).
 *
 * Filename hints:
 *   - Pra `File` (Explorer copy): `file.name` (já tem extensão).
 *   - Pra bitmap raw: inventa `pasted-<timestamp>.<ext>` onde `<ext>`
 *     vem do MIME (image/png → png, image/jpeg → jpg). Default png
 *     se MIME for desconhecido.
 *
 * Quando ENABLED é falso, o hook não registra o listener — útil pra
 * desabilitar em modo "leitura" ou enquanto o editor não existe.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type { Editor } from "@tiptap/react";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type {
  ImportedPhoto,
  PastedPhotoInput,
  PhotoImportResult,
} from "@domain/photo_drop";

export type PasteState = "idle" | "uploading" | "done" | "error";

export interface UsePasteImageOptions {
  /** True quando o paste deve estar ativo. Quando false, nenhum
   *  listener é registrado. */
  enabled: boolean;
  /** Editor TipTap. O listener é attachado em `editor.view.dom`. */
  editor: Editor | null;
  /** Workspace ativo (path absoluto). */
  workspacePath: string | null;
  /** Laudo ativo (id). */
  laudoId: string | null;
  /** Chamado APÓS o backend ter sucesso. O caller insere as fotos na
   *  posição atual do cursor via `insertFigure`. */
  onImported: (photos: ImportedPhoto[]) => void;
  /** Chamado quando alguma foto do lote falha ou o command rejeita. */
  onErrors?: (errors: PhotoImportResult["errors"]) => void;
}

export interface PasteImageState {
  state: PasteState;
  errorMessage: string | null;
  reset: () => void;
}

/** MIME → extensão (sem ponto). Cobre o que o backend
 *  `ALLOWED_EXTENSIONS` aceita. Default `png` quando desconhecido —
 *  o backend reproduz o sniff de formato no `read_metadata`, então o
 *  arquivo gravado vai ter o conteúdo certo mesmo se a extensão
 *  escolhida for grosseira. */
function mimeToExtension(mime: string): string {
  const lower = mime.toLowerCase();
  if (lower === "image/jpeg" || lower === "image/jpg") return "jpg";
  if (lower === "image/png") return "png";
  if (lower === "image/webp") return "webp";
  if (lower === "image/gif") return "gif";
  if (lower === "image/bmp") return "bmp";
  if (lower === "image/tiff" || lower === "image/tif") return "tif";
  return "png";
}

/** ArrayBuffer → base64 (sem prefixo `data:`). Pra payload Tauri. */
function arrayBufferToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  // Chunked pra não estourar argumento máx do `String.fromCharCode`
  // em buffers grandes (>1MB).
  const chunkSize = 0x8000;
  let binary = "";
  for (let i = 0; i < bytes.length; i += chunkSize) {
    binary += String.fromCharCode.apply(
      null,
      Array.from(bytes.subarray(i, i + chunkSize)),
    );
  }
  return btoa(binary);
}

/**
 * Extrai as imagens do `DataTransfer` do evento `paste`.
 *
 * Usa `files` OU `items` — NUNCA os dois juntos. Um bitmap colado do clipboard
 * (screenshot, "copiar imagem" do navegador) costuma aparecer nos DOIS, e como
 * cada `item.getAsFile()` cria um `File` NOVO, mesclar os dois inseria a foto
 * DUPLICADA (o dedup por referência não reconhece objetos distintos de mesmo
 * conteúdo). Por isso: se `files` já traz imagem, usa só ela; senão, cai para
 * `items` (caso do bitmap raw que aparece apenas ali).
 */
function collectImageFiles(dt: DataTransfer | null): File[] {
  if (!dt) return [];

  // 1) DataTransfer.files — Explorer copy entrega aqui; a maioria dos
  //    navegadores/clipboard de bitmap raw TAMBÉM duplica a imagem aqui.
  const fromFiles: File[] = [];
  if (dt.files && dt.files.length > 0) {
    for (let i = 0; i < dt.files.length; i++) {
      const f = dt.files[i];
      if (f && f.type.startsWith("image/")) fromFiles.push(f);
    }
  }
  if (fromFiles.length > 0) return fromFiles;

  // 2) Fallback: bitmap raw que aparece SÓ em `items` (kind="file",
  //    type="image/*"); `getAsFile()` produz um File sintético.
  const fromItems: File[] = [];
  if (dt.items && dt.items.length > 0) {
    for (let i = 0; i < dt.items.length; i++) {
      const item = dt.items[i];
      if (!item) continue;
      if (item.kind === "file" && item.type.startsWith("image/")) {
        const f = item.getAsFile();
        if (f) fromItems.push(f);
      }
    }
  }
  return fromItems;
}

export function usePasteImage(options: UsePasteImageOptions): PasteImageState {
  const { enabled, editor, workspacePath, laudoId, onImported, onErrors } =
    options;

  const [state, setState] = useState<PasteState>("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // Refs pra evitar reassinatura do listener quando o caller passar
  // callbacks inline.
  const onImportedRef = useRef(onImported);
  const onErrorsRef = useRef(onErrors);
  const workspacePathRef = useRef(workspacePath);
  const laudoIdRef = useRef(laudoId);
  onImportedRef.current = onImported;
  onErrorsRef.current = onErrors;
  workspacePathRef.current = workspacePath;
  laudoIdRef.current = laudoId;

  const reset = useCallback(() => {
    setState("idle");
    setErrorMessage(null);
  }, []);

  useEffect(() => {
    if (!enabled || !editor) return;
    const view = editor.view;
    if (!view || !view.dom) return;
    const dom = view.dom as HTMLElement;

    const handler = (evt: Event) => {
      const event = evt as ClipboardEvent;
      const files = collectImageFiles(event.clipboardData);
      if (files.length === 0) return; // não é foto, deixa o PM fazer paste normal.

      // PREVENT DEFAULT em captura: bloqueia tanto o PM quanto qualquer
      // outro handler downstream de tentar inserir a imagem por conta
      // própria. A inserção é nossa via `insertFigure` no onImported.
      event.preventDefault();
      event.stopPropagation();

      const ws = workspacePathRef.current;
      const lid = laudoIdRef.current;
      if (!ws || !lid) {
        setErrorMessage("Sem workspace/laudo ativo.");
        setState("error");
        return;
      }

      setState("uploading");
      setErrorMessage(null);

      // Async no IIFE — listener não pode ser async direto (o
      // preventDefault precisa rolar síncrono).
      void (async () => {
        try {
          const ts = Date.now();
          const payload: PastedPhotoInput[] = [];
          for (let i = 0; i < files.length; i++) {
            const f = files[i];
            if (!f) continue;
            const buf = await f.arrayBuffer();
            const bytes_base64 = arrayBufferToBase64(buf);
            // Se o `File` veio do Explorer, `.name` traz o nome
            // original com extensão. Se veio do bitmap raw, `.name` é
            // tipicamente "image.png" ou vazio — caímos no fallback.
            const hasGoodName = !!f.name && /\.[a-z0-9]+$/i.test(f.name);
            const filename = hasGoodName
              ? f.name
              : `pasted-${ts}-${i + 1}.${mimeToExtension(f.type)}`;
            payload.push({ bytes_base64, filename });
          }
          const result = await commands.importPastedPhotosToLaudo(
            ws,
            lid,
            payload,
          );
          if (result.imported.length > 0) {
            onImportedRef.current(result.imported);
          }
          if (result.errors.length > 0 && onErrorsRef.current) {
            onErrorsRef.current(result.errors);
          }
          if (result.imported.length > 0) {
            setState("done");
          } else if (result.errors.length > 0) {
            setErrorMessage(
              `Nenhuma foto válida no clipboard (${result.errors.length} rejeitada(s)).`,
            );
            setState("error");
          } else {
            setState("idle");
          }
        } catch (err) {
          const e = toSicroError(err);
          setErrorMessage(e.message);
          setState("error");
        }
      })();
    };

    // capture: true — listener roda na fase de captura, ANTES do
    // handler do ProseMirror que vive na fase de bubble. Isso garante
    // que nosso preventDefault aborte o paste default do PM.
    dom.addEventListener("paste", handler, { capture: true });
    return () => {
      dom.removeEventListener("paste", handler, { capture: true });
    };
  }, [enabled, editor]);

  return { state, errorMessage, reset };
}
