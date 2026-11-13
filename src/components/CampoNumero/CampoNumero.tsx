/** Campo numérico do SICRO: − e + grandes (segurar repete, acelerando), digitação livre, unidade ao lado. */

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Minus, Plus } from "lucide-react";
import styles from "./CampoNumero.module.css";

interface Props {
  label?: ReactNode;
  value: number;
  onChange: (v: number) => void;
  step?: number;
  min?: number;
  max?: number;
  unidade?: string;
  /** Casas decimais mostradas (o valor interno não é arredondado além disso). */
  casas?: number;
  disabled?: boolean;
  onEnter?: () => void;
  onEscape?: () => void;
  autoFocus?: boolean;
  className?: string;
}

export function CampoNumero({ label, value, onChange, step = 1, min, max, unidade, casas, disabled, onEnter, onEscape, autoFocus, className }: Props) {
  const dec = casas ?? (Number.isInteger(step) ? 0 : 2);
  const fmt = (v: number) => (Number.isFinite(v) ? v.toFixed(dec).replace(".", ",") : "");
  const limitar = (v: number) => Math.min(max ?? Infinity, Math.max(min ?? -Infinity, v));
  const [texto, setTexto] = useState(fmt(value));
  useEffect(() => setTexto(fmt(value)), [value, dec]); // eslint-disable-line react-hooks/exhaustive-deps
  const aplicar = (t: string) => {
    setTexto(t);
    const n = Number(t.replace(",", "."));
    if (t.trim() !== "" && Number.isFinite(n) && n === limitar(n)) onChange(n);
  };
  const confirmar = () => {
    const n = Number(texto.replace(",", "."));
    if (texto.trim() !== "" && Number.isFinite(n)) onChange(limitar(n));
    else setTexto(fmt(value));
  };
  const id = useRef(`cn-${Math.random().toString(36).slice(2, 8)}`).current;
  return (
    <div className={`${styles.campo} ${className ?? ""}`}>
      {label && (
        <span id={id} className={styles.rotulo}>
          {label}
        </span>
      )}
      <span className={`${styles.caixa} ${disabled ? styles.desligado : ""}`}>
        <BotaoPasso delta={-step} value={value} limitar={limitar} onChange={onChange} disabled={disabled} rotulo="menos">
          <Minus size={14} />
        </BotaoPasso>
        <input
          type="text"
          inputMode="decimal"
          value={texto}
          disabled={disabled}
          autoFocus={autoFocus}
          aria-labelledby={label ? id : undefined}
          onChange={(e) => aplicar(e.target.value)}
          onBlur={confirmar}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              confirmar();
              onEnter?.();
            } else if (e.key === "Escape") onEscape?.();
            else if (e.key === "ArrowUp") {
              e.preventDefault();
              onChange(limitar(value + step));
            } else if (e.key === "ArrowDown") {
              e.preventDefault();
              onChange(limitar(value - step));
            }
          }}
        />
        {unidade && <span className={styles.unidade}>{unidade}</span>}
        <BotaoPasso delta={step} value={value} limitar={limitar} onChange={onChange} disabled={disabled} rotulo="mais">
          <Plus size={14} />
        </BotaoPasso>
      </span>
    </div>
  );
}

/** Clique dá um passo; segurar repete, cada vez mais rápido. */
function BotaoPasso({ delta, value, limitar, onChange, disabled, rotulo, children }: { delta: number; value: number; limitar: (v: number) => number; onChange: (v: number) => void; disabled?: boolean; rotulo: string; children: ReactNode }) {
  const valorRef = useRef(value);
  valorRef.current = value;
  const timer = useRef<number | null>(null);
  const parar = () => {
    if (timer.current != null) window.clearTimeout(timer.current);
    timer.current = null;
  };
  const passo = () => {
    // Evita 0,1 + 0,2 = 0,30000000000000004 na tela.
    const v = limitar(Number((valorRef.current + delta).toFixed(6)));
    valorRef.current = v;
    onChange(v);
  };
  const segurar = () => {
    passo();
    let espera = 400;
    const laco = () => {
      passo();
      espera = Math.max(40, espera * 0.8);
      timer.current = window.setTimeout(laco, espera);
    };
    timer.current = window.setTimeout(laco, espera);
  };
  useEffect(() => parar, []);
  return (
    <button
      type="button"
      className={styles.btn}
      aria-label={rotulo}
      disabled={disabled}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        segurar();
      }}
      onPointerUp={parar}
      onPointerLeave={parar}
      onPointerCancel={parar}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          passo();
        }
      }}
    >
      {children}
    </button>
  );
}
