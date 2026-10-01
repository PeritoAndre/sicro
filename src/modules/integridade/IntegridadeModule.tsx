/**
 * Integridade — a Central de Provas do caso em tela própria (no 3.x morava no
 * Dossiê). Lista tudo o que o caso guarda, confere existência e SHA-256 no
 * disco e gera o relatório de integridade. Abre pelo "Verificar integridade"
 * do Início.
 */
import { ShieldCheck } from "lucide-react";
import { useWorkspaceStore, selectActiveWorkspacePath } from "@stores/workspaceStore";
import { NoOccurrenceState } from "@components/NoOccurrenceState/NoOccurrenceState";
import { IntegridadePanel } from "@modules/evidencias/IntegridadePanel";
import styles from "./IntegridadeModule.module.css";

export function IntegridadeModule() {
  const ws = useWorkspaceStore(selectActiveWorkspacePath);
  if (!ws) {
    return (
      <NoOccurrenceState
        icon={<ShieldCheck size={44} strokeWidth={1.2} />}
        moduleName="Integridade"
      />
    );
  }
  return (
    <div className={styles.wrap}>
      <header className={styles.head}>
        <div>
          <h1>
            <ShieldCheck size={16} aria-hidden /> Integridade do caso
          </h1>
          <p>
            Tudo o que o caso guarda, conferido no disco (existência e SHA-256), com relatório
            auditável. Só leitura: nada aqui altera as provas.
          </p>
        </div>
      </header>
      <IntegridadePanel workspacePath={ws} />
    </div>
  );
}
