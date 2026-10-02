import type { ReactNode } from "react";
import { EmptyState } from "../EmptyState/EmptyState";
import styles from "./NoOccurrenceState.module.css";

interface NoOccurrenceStateProps {
  icon: ReactNode;
  /** Entra na frase padrão; ignorado se `description` for dada. */
  moduleName?: string;
  description?: ReactNode;
  actions?: ReactNode;
}

/** Tela padrão de "nenhuma ocorrência aberta", igual em todos os módulos. */
export function NoOccurrenceState({
  icon,
  moduleName,
  description,
  actions,
}: NoOccurrenceStateProps) {
  const desc =
    description ??
    `Abra ou crie uma ocorrência na tela Início para usar o módulo ${moduleName ?? ""}.`;
  return (
    <div className={styles.center}>
      <EmptyState
        icon={icon}
        title="Nenhuma ocorrência aberta"
        description={desc}
        actions={actions}
      />
    </div>
  );
}
