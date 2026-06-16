import type { ReactNode } from "react";
import { EmptyState } from "../EmptyState/EmptyState";
import styles from "./NoOccurrenceState.module.css";

interface NoOccurrenceStateProps {
  /** Ícone do módulo (ex.: <Film size={36} strokeWidth={1.5} />). */
  icon: ReactNode;
  /**
   * Nome do módulo para a frase padrão ("…para usar o módulo {moduleName}.").
   * Ignorado se `description` for fornecida.
   */
  moduleName?: string;
  /** Descrição custom — sobrescreve a frase padrão. */
  description?: ReactNode;
  /** Ações opcionais (ex.: alternar para um modo que não exige ocorrência). */
  actions?: ReactNode;
}

/**
 * Tela PADRÃO de "nenhuma ocorrência aberta", compartilhada por todos os
 * módulos que dependem de uma ocorrência ativa. Centraliza o card `EmptyState`
 * no viewport do módulo — fonte única para manter Dossiê, Laudos, Croquis,
 * Vídeos, Áudios, Imagens e Documentoscopia visualmente idênticos.
 */
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
