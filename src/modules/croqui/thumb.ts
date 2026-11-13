/** Miniatura da lista: PNG ~480 px em Sistema/croquis/thumbs/<id>.png. Falha só vai para o console. */

import { convertFileSrc } from "@tauri-apps/api/core";
import { commands } from "@core/commands";
import { useCroquiStore } from "./store/croquiStore";
import { caminhoDoCaso } from "@core/caminhoDoCaso";

export const THUMB_LARGURA = 480;

function carregar(src: string): Promise<HTMLImageElement> {
  return new Promise((ok, erro) => {
    const im = new Image();
    im.onload = () => ok(im);
    im.onerror = erro;
    im.src = src;
  });
}

/** Reduz um data URL para a largura da miniatura (sobre fundo branco). */
async function reduzir(dataUrl: string): Promise<string> {
  const im = await carregar(dataUrl);
  const w = Math.min(THUMB_LARGURA, im.width);
  const h = Math.max(1, Math.round((im.height * w) / im.width));
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const cx = c.getContext("2d");
  if (!cx) throw new Error("canvas 2d indisponível");
  cx.fillStyle = "#ffffff";
  cx.fillRect(0, 0, w, h);
  cx.drawImage(im, 0, 0, w, h);
  return c.toDataURL("image/png");
}

/** Dispara sem bloquear: `gerar` devolve o data URL da folha (pode ser grande). */
export function salvarMiniatura(
  workspacePath: string,
  croquiId: string,
  gerar: () => string | null | Promise<string | null>,
): void {
  void (async () => {
    try {
      const url = await gerar();
      if (!url) return;
      const png = await reduzir(url);
      await commands.saveCroquiThumbnail(workspacePath, croquiId, png.split(",")[1] ?? png);
      useCroquiStore.getState().bumpThumb(croquiId);
    } catch (err) {
      console.warn("miniatura do croqui não gerada", err);
    }
  })();
}

export function urlMiniatura(workspacePath: string, croquiId: string, versao: string): string {
  return `${convertFileSrc(caminhoDoCaso(workspacePath, `Sistema/croquis/thumbs/${croquiId}.png`))}?t=${encodeURIComponent(versao)}`;
}
