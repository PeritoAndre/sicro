/** Trilho dos modos do Vídeo (§8.1): os cinco da análise e a Edição; mora no `VideoModule` e vale nas duas telas. */

import { Gauge, Play, Ruler, ScanSearch, Scissors, Wand2 } from "lucide-react";
import type { Modo } from "./medirStore";
import styles from "./Medir.module.css";

export type ModoTrilho = Modo | "edicao";

const MODOS: { id: ModoTrilho; label: string; icon: typeof Play; key: string; title: string }[] = [
  { id: "assistir", label: "Assistir", icon: Play, key: "1", title: "Reprodutor, eventos e quadros (Alt+1)" },
  { id: "velocidade", label: "Velocidade", icon: Gauge, key: "2", title: "Calcular a velocidade de um veículo (Alt+2)" },
  { id: "distancia", label: "Distância", icon: Ruler, key: "3", title: "Medir uma distância no chão (Alt+3)" },
  { id: "pessoas", label: "Rastreio", icon: ScanSearch, key: "4", title: "Numerar e acompanhar o que aparece na cena: pessoas, veículos e manchas marcadas à mão (Alt+4)" },
  { id: "realce", label: "Realce", icon: Wand2, key: "5", title: "Ler melhor a imagem: média de quadros, super-resolução, estabilização, lente e perspectiva (Alt+5)" },
  { id: "edicao", label: "Edição", icon: Scissors, key: "6", title: "Edição (Alt+6)" },
];

export function ModeRail({ modo, onChange }: { modo: ModoTrilho; onChange: (m: ModoTrilho) => void }) {
  return (
    <nav className={styles.rail} aria-label="Modos da análise" data-trilho="">
      {MODOS.map(({ id, label, icon: Icon, key, title }) => (
        <button
          key={id}
          type="button"
          className={`${styles.railBtn} ${modo === id ? styles.railOn : ""}`}
          aria-pressed={modo === id}
          onClick={() => onChange(id)}
          title={title}
          data-modo={id}
        >
          <Icon size={18} aria-hidden />
          {label}
          <kbd className={styles.key}>{key}</kbd>
        </button>
      ))}
    </nav>
  );
}
