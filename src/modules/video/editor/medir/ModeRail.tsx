/** Trilho dos modos da análise de vídeo, como o do croqui. */

import { Gauge, Play, Ruler, ScanSearch } from "lucide-react";
import type { Modo } from "./medirStore";
import styles from "./Medir.module.css";

const MODOS: { id: Modo; label: string; icon: typeof Play; key: string; title: string }[] = [
  { id: "assistir", label: "Assistir", icon: Play, key: "1", title: "Reprodutor, eventos e quadros (Alt+1)" },
  { id: "velocidade", label: "Velocidade", icon: Gauge, key: "2", title: "Calcular a velocidade de um veículo (Alt+2)" },
  { id: "distancia", label: "Distância", icon: Ruler, key: "3", title: "Medir uma distância no chão (Alt+3)" },
  { id: "pessoas", label: "Rastreio", icon: ScanSearch, key: "4", title: "Numerar e acompanhar o que aparece na cena: pessoas, veículos e manchas marcadas à mão (Alt+4)" },
];

export function ModeRail({ modo, onChange }: { modo: Modo; onChange: (m: Modo) => void }) {
  return (
    <nav className={styles.rail} aria-label="Modos da análise">
      {MODOS.map(({ id, label, icon: Icon, key, title }) => (
        <button
          key={id}
          type="button"
          className={`${styles.railBtn} ${modo === id ? styles.railOn : ""}`}
          aria-pressed={modo === id}
          onClick={() => onChange(id)}
          title={title}
        >
          <Icon size={18} aria-hidden />
          {label}
          <kbd className={styles.key}>{key}</kbd>
        </button>
      ))}
    </nav>
  );
}
