/**
 * Proveniência da mídia (F6; P1.10): "Formato do gravador", "Observação", "Desvio do relógio (s)" e "Gravador",
 * gravados na análise (`video_proveniencia_salvar`) e copiados para o `MidiaRef` quando a mídia entra numa edição
 * (`store/coleta.ts` e "Trecho…"/"Dos eventos…" chamam aqui). Erro de leitura sobe para quem chama: nunca vira
 * "não informado".
 */

import { commands } from "@core/commands";
import type { MidiaRef, ProvenienciaMidia } from "@domain/edicao";

/** Tetos do texto livre: os de `video_proveniencia_commands.rs` (`MAX_OBS`, `MAX_GRAVADOR`). */
export const MAX_OBS = 4000;
export const MAX_GRAVADOR = 200;

export type ProvenienciaDoRef = Pick<MidiaRef, "recebido" | "relogio_desvio_s" | "gravador">;

/** Confere a resposta do backend; fora do formato é erro (um `undefined` no `MidiaRef` sumiria do JSON gravado). */
export function conferirProveniencia(p: unknown): ProvenienciaMidia {
  const o = (typeof p === "object" && p !== null ? p : {}) as Record<string, unknown>;
  const n = o.recebido_nativo;
  const d = o.relogio_desvio_s;
  const g = o.gravador;
  if (
    (n !== null && typeof n !== "boolean") ||
    typeof o.recebido_obs !== "string" ||
    (d !== null && !(typeof d === "number" && Number.isFinite(d))) ||
    (g !== null && typeof g !== "string")
  ) {
    throw new Error("item_invalido: proveniencia");
  }
  return { recebido_nativo: n as boolean | null, recebido_obs: o.recebido_obs, relogio_desvio_s: d as number | null, gravador: g as string | null };
}

/** Nada informado nos quatro campos. */
export function provenienciaVazia(p: ProvenienciaMidia): boolean {
  return p.recebido_nativo === null && p.recebido_obs === "" && p.relogio_desvio_s === null && (p.gravador ?? "") === "";
}

export function paraMidiaRef(p: ProvenienciaMidia): ProvenienciaDoRef {
  return {
    recebido: { nativo: p.recebido_nativo, obs: p.recebido_obs },
    relogio_desvio_s: p.relogio_desvio_s,
    gravador: p.gravador === "" ? null : p.gravador,
  };
}

/** Desvio com vírgula decimal; "" = não informado. */
export function textoDoDesvio(d: number | null): string {
  return d === null ? "" : String(d).replace(".", ",");
}
/** null = não informado; undefined = texto que não é número. */
export function desvioDoTexto(t: string): number | null | undefined {
  const s = t.trim().replace("−", "-").replace(",", ".");
  if (s === "") return null;
  // só decimal simples: `Number` aceitaria "0x10", "1e3" e "Infinity"
  if (!/^[+-]?(\d+\.?\d*|\.\d+)$/.test(s)) return undefined;
  const n = Number(s);
  // grande ou pequeno demais voltaria à tela em notação de expoente, que o campo não lê
  return Number.isFinite(n) && !String(n).includes("e") ? n : undefined;
}

/** Campos do `MidiaRef` que vêm da proveniência gravada da mídia `mediaId`; null = nada gravado. Rejeita se a
 *  leitura falhar. */
export async function provenienciaDaMidia(ws: string, mediaId: string): Promise<ProvenienciaDoRef | null> {
  const p = conferirProveniencia(await commands.videoProveniencia(ws, mediaId));
  return provenienciaVazia(p) ? null : paraMidiaRef(p);
}
