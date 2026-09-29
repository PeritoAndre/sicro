/**
 * URL de arquivo local para `<video>` / `<audio>`.
 *
 * - Windows (WebView2/Chromium): asset protocol do Tauri, como o resto do app.
 * - Linux (WebKitGTK): `file://`. O player de mídia do WebKitGTK (GStreamer)
 *   só lê http(s), blob e file — pelo asset protocol ele falha com
 *   "MediaError 4" mesmo com o codec instalado. O `lib.rs` registra o esquema
 *   `tauri` como local no setup para a página poder abrir `file://`.
 *   (No `tauri dev` a página é http://localhost e o file:// fica bloqueado.)
 *
 * Só para elementos de mídia: `fetch` (ex.: forma de onda) continua no asset
 * protocol, que funciona em todas as plataformas.
 */
import { convertFileSrc } from "@tauri-apps/api/core";

const IS_LINUX =
  typeof navigator !== "undefined" &&
  /Linux/.test(navigator.userAgent) &&
  !/Android/.test(navigator.userAgent);

/** `/home/a/b c.mp4` → `file:///home/a/b%20c.mp4` (cada trecho escapado). */
export function toFileUrl(absPath: string): string {
  return "file://" + absPath.split("/").map(encodeURIComponent).join("/");
}

export function mediaSrc(absPath: string): string {
  return IS_LINUX ? toFileUrl(absPath) : convertFileSrc(absPath);
}
