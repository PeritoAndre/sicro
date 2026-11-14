/**
 * Raiz da Edição (§8.1): abre a edição corrente do caso (criada se não houver) e guarda a saída: a navegação só
 * segue depois de `descarregar()`. Se a gravação falhar, fica na tela com o ícone "Não salvou" (a oferta de
 * fechar sem gravar é da guarda da janela e do inspetor: contratos (g) 15). Ao desmontar (trocar de modo), grava o
 * pendente e deixa o projeto aberto: voltar é imediato.
 */

import { useEffect, useState } from "react";
import { Loader2, TriangleAlert } from "lucide-react";
import { selectActiveWorkspacePath, useWorkspaceStore } from "@stores/workspaceStore";
import { useNavGuard } from "@app/navGuard";
import { useEdicaoStore } from "./store/edicaoStore";
import { EditorEdicao } from "./EditorEdicao";
import styles from "./EditorEdicao.module.css";

/** Guarda de saída: grava o pendente; false se não gravou (a tela fica). */
export async function guardaDeSaida(): Promise<boolean> {
  try {
    await useEdicaoStore.getState().descarregar();
    return true;
  } catch {
    return false;
  }
}

export function EdicaoRaiz() {
  const ws = useWorkspaceStore(selectActiveWorkspacePath);
  const aberta = useEdicaoStore((s) => s.aberta);
  const wsAberto = useEdicaoStore((s) => s.ws);
  const erro = useEdicaoStore((s) => s.erro);
  const [falhou, setFalhou] = useState(false);

  useEffect(() => {
    if (!ws) return;
    if (aberta && wsAberto === ws) return;
    setFalhou(false);
    useEdicaoStore
      .getState()
      .abrirCorrente(ws)
      .catch(() => setFalhou(true));
  }, [ws]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    useNavGuard.getState().register(guardaDeSaida);
    return () => {
      useNavGuard.getState().unregister();
      void useEdicaoStore
        .getState()
        .descarregar()
        .catch(() => {});
    };
  }, []);

  if (!ws) return null;
  if (aberta && wsAberto === ws) return <EditorEdicao />;
  return (
    <div className={styles.editor} data-raiz="">
      <span className={styles.carregando} title={falhou ? erro?.frase || undefined : undefined}>
        {falhou ? <TriangleAlert size={22} className={styles.erro} aria-hidden /> : <Loader2 size={22} className={styles.gira} aria-hidden />}
      </span>
    </div>
  );
}
