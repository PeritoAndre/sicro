/** Avanço da cópia de gravador em curso (P4.6b): fase por ícone, barra, porcentagem e Cancelar (fora do registro). */

import { Copy, FolderInput, ListChecks, X } from "lucide-react";
import { Button } from "@components/Button/Button";
import { cancelarImportacao, useImportacaoGravador, type FaseImportacao } from "./gravador";
import styles from "./ImportacaoGravador.module.css";

const FASES: Record<FaseImportacao, { rotulo: string; Icone: typeof Copy }> = {
  copia: { rotulo: "Cópia", Icone: Copy },
  prova: { rotulo: "Prova", Icone: ListChecks },
  registro: { rotulo: "Registro", Icone: FolderInput },
};

export function ImportacaoGravador() {
  const { fase, fracao } = useImportacaoGravador();
  if (!fase) return null;
  const { rotulo, Icone } = FASES[fase];
  const pct = fracao == null ? null : Math.floor(fracao * 100);
  return (
    <div className={styles.painel} role="status" data-importacao-gravador={fase}>
      <span className={styles.fase} title={rotulo}>
        <Icone size={14} aria-label={rotulo} />
      </span>
      <div
        className={styles.barra}
        role="progressbar"
        aria-label={rotulo}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={pct ?? undefined}
        data-indefinida={pct == null || undefined}
      >
        <div className={styles.cheio} style={pct == null ? undefined : { width: `${pct}%` }} />
      </div>
      <span className={styles.pct}>{pct == null ? "" : `${pct}%`}</span>
      {fase !== "registro" && (
        <Button variant="secondary" size="sm" leftIcon={<X size={13} />} onClick={cancelarImportacao}>
          Cancelar
        </Button>
      )}
    </div>
  );
}
