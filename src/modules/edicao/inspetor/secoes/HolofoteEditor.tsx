/**
 * Holofote (§2.1, §5.4): "Forma" (Retângulo, Elipse, Polígono) · "Margem" · "Borda suave" · "Inverter" ·
 * "Escurecer" · "Dessaturar". "De", "Até", "Marcar" e o rastreio são da linha da marcação.
 * Dono: P2.6 (contratos-fase2.md). Abertura: casca com `HOLOFOTE_PRONTO = false`; até lá o holofote aparece só para
 * leitura (registro.ts) e "Adicionar" não o oferece.
 */

import type { ContextoClipe, EfHolofote, Id } from "@domain/edicao";
import type { EditorEfeitoProps } from "../registro";

/** O P2.6 liga ao entregar: "Adicionar → Holofote" aparece e o editor abre. */
export const HOLOFOTE_PRONTO = false;

/** Holofote novo com os padrões, de olho num rastreio criado junto por `adicionarHolofote`. */
export function novoHolofote(ctx: ContextoClipe): EfHolofote {
  void ctx;
  throw new Error("nao_suportado: holofote");
}

/** "Adicionar → Holofote": cria o rastreio e o efeito no clipe; devolve o id do efeito (null = nada criado). */
export function adicionarHolofote(clipeId: Id, rotulo: string): Promise<Id | null> {
  void clipeId;
  void rotulo;
  return Promise.resolve(null);
}

export function HolofoteEditor(p: EditorEfeitoProps<EfHolofote>) {
  void p;
  return null;
}
