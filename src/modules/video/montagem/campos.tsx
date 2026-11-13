import { useEffect, useState } from "react";
import { parseTimeInput } from "../editor/format";
import styles from "./MontagemView.module.css";

const segFmt = (s: number) => (Number.isInteger(s) ? String(s) : s.toFixed(3).replace(/0+$/, ""));

/** Tempo em segundos ou "1:02.5"; confirma ao sair do campo ou no Enter. */
export function TempoInput({ valor, onChange, fps }: { valor: number; onChange: (v: number) => void; fps?: number | null }) {
  const [txt, setTxt] = useState(segFmt(valor));
  const [ruim, setRuim] = useState(false);
  useEffect(() => setTxt(segFmt(valor)), [valor]);
  const confirmar = () => {
    const v = parseTimeInput(txt, fps);
    if (v == null) {
      setRuim(true);
      return;
    }
    setRuim(false);
    if (v !== valor) onChange(v);
    else setTxt(segFmt(valor));
  };
  return (
    <input
      className={`${styles.input} ${ruim ? styles.bad : ""}`}
      value={txt}
      onChange={(e) => setTxt(e.target.value)}
      onBlur={confirmar}
      onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      inputMode="decimal"
    />
  );
}

export function NumInput({ valor, onChange, min, max, passo = 0.1 }: { valor: number; onChange: (v: number) => void; min?: number; max?: number; passo?: number }) {
  return (
    <input
      className={styles.input}
      type="number"
      step={passo}
      min={min}
      max={max}
      value={valor}
      onChange={(e) => Number.isFinite(e.target.valueAsNumber) && onChange(e.target.valueAsNumber)}
    />
  );
}

