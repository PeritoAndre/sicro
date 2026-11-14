/**
 * Raiz da Edição (§8.1): abre a edição corrente do caso (criada se não houver) e guarda a saída: a navegação só
 * segue depois de `descarregar()` (se a gravação falhar, a guarda pergunta; ver `guarda.ts`). Hospedada no
 * `VideoModule` ao lado da análise: escondida (`visivel` falso), fica montada e sem atalhos. Ao desmontar (sair do
 * módulo, trocar de caso), grava o pendente e deixa o projeto aberto: voltar é imediato.
 */

import { useEffect, useState } from "react";
import { Loader2, TriangleAlert } from "lucide-react";
import { selectActiveWorkspacePath, useWorkspaceStore } from "@stores/workspaceStore";
import { useNavGuard } from "@app/navGuard";
import { useEdicaoStore } from "./store/edicaoStore";
import { EditorEdicao } from "./EditorEdicao";
import { guardaDeSaida } from "./guarda";
import styles from "./EditorEdicao.module.css";

export { guardaDeSaida };

export function EdicaoRaiz({ visivel = true }: { visivel?: boolean } = {}) {
  const ws = useWorkspaceStore(selectActiveWorkspacePath);
  const aberta = useEdicaoStore((s) => s.aberta);
  const wsAberto = useEdicaoStore((s) => s.ws);
  const erro = useEdicaoStore((s) => s.erro);
  const [falhou, setFalhou] = useState(false);

  // na frente sem projeto do caso (primeira vez, ou o aberto foi excluído na Central): abre a corrente
  const temProjeto = !!aberta && wsAberto === ws;
  useEffect(() => {
    if (!ws || !visivel || temProjeto) return;
    setFalhou(false);
    useEdicaoStore
      .getState()
      .abrirCorrente(ws)
      .catch(() => setFalhou(true));
  }, [ws, visivel, temProjeto]);

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
  if (temProjeto) return <EditorEdicao ativo={visivel} />;
  return (
    <div className={styles.editor} data-raiz="">
      <span className={styles.carregando} title={falhou ? erro?.frase || undefined : undefined}>
        {falhou ? <TriangleAlert size={22} className={styles.erro} aria-hidden /> : <Loader2 size={22} className={styles.gira} aria-hidden />}
      </span>
    </div>
  );
}
