import type { OccurrenceStatus } from "@domain/occurrence";
import styles from "./StatusPill.module.css";

interface StatusPillProps {
  status: OccurrenceStatus;
}

const labels: Record<OccurrenceStatus, string> = {
  aberta: "Aberta",
  em_andamento: "Em andamento",
  concluida: "Concluída",
  arquivada: "Arquivada",
};

export function StatusPill({ status }: StatusPillProps) {
  return (
    <span className={`${styles.pill} ${styles[status] ?? ""}`}>
      {labels[status] ?? status}
    </span>
  );
}
