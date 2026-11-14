/**
 * Guarda de saída da Edição (§8.1, §8.4; contratos (g) 15): antes de trocar de módulo, de caso ou de fechar a
 * janela, grava o pendente. Se a gravação falha, pergunta; "Descartar" fecha o projeto sem gravar.
 */

import { askConfirm } from "@components/Dialog/ask";
import { useEdicaoStore } from "./store/edicaoStore";

/** true = pode sair. */
export async function guardaDeSaida(): Promise<boolean> {
  if (!useEdicaoStore.getState().aberta) return true;
  try {
    await useEdicaoStore.getState().descarregar();
    return true;
  } catch {
    // não gravou: o perito decide
  }
  const erro = useEdicaoStore.getState().erro;
  const sair = await askConfirm({ title: "Não salvou", message: erro?.frase || undefined, confirmLabel: "Descartar", danger: true });
  if (!sair) return false;
  await useEdicaoStore
    .getState()
    .fechar({ descartar: true })
    .catch(() => {});
  return true;
}
