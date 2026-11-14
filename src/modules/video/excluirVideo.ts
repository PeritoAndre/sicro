/**
 * Diálogo de exclusão de vídeo (§4): antes de excluir mostra as edições que usam o vídeo (`edicao_usos`): título,
 * clipes e exportações de cada uma. O projeto continua abrindo e acusa `midia_ausente`; as exportações ficam.
 */

import { askConfirm } from "@components/Dialog/ask";
import { commands } from "@core/commands";
import type { UsoMidia } from "@domain/edicao";
import { textoClipes } from "@modules/edicao/sequencia/contas";

/** Uma linha por edição: "Título · 3 clipes · 1 exportação". */
export function linhaDeUso(u: UsoMidia): string {
  const partes = [u.titulo, textoClipes(u.n_clipes)];
  if (u.saidas > 0) partes.push(`${u.saidas} ${u.saidas === 1 ? "exportação" : "exportações"}`);
  return partes.join(" · ");
}

export async function confirmarExclusaoVideo(ws: string, sha256: string): Promise<boolean> {
  let usos: UsoMidia[] = [];
  try {
    usos = await commands.edicaoUsos(ws, sha256);
  } catch {
    // sem a lista de usos o diálogo segue como antes
  }
  const linhas = ["Excluir este vídeo e tudo que foi marcado nele?", ...(usos.length ? ["", ...usos.map(linhaDeUso)] : [])];
  return askConfirm({ title: "Excluir vídeo", message: linhas.join("\n"), confirmLabel: "Excluir", danger: true });
}
