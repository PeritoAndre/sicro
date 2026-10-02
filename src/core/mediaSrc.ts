/**
 * URL de arquivo local só para `<video>` / `<audio>` (fetch continua no asset protocol).
 * No Linux o GStreamer do WebKitGTK só lê http(s)/blob/file — pelo asset protocol
 * dá "MediaError 4"; por isso `file://` (o lib.rs registra o esquema `tauri` como local).
 */
import { convertFileSrc } from "@tauri-apps/api/core";

const IS_LINUX =
  typeof navigator !== "undefined" &&
  /Linux/.test(navigator.userAgent) &&
  !/Android/.test(navigator.userAgent);

/** `/home/a/b c.mp4` → `file:///home/a/b%20c.mp4`. */
export function toFileUrl(absPath: string): string {
  return "file://" + absPath.split("/").map(encodeURIComponent).join("/");
}

export function mediaSrc(absPath: string): string {
  return IS_LINUX ? toFileUrl(absPath) : convertFileSrc(absPath);
}
