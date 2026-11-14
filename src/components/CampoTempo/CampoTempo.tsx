/**
 * Campo de tempo da Edição (§3.1): mostra `HH:MM:SS,mmm` e o número do quadro; aceita `12,5`, `1:02,5` e
 * `#312`. ↑/↓ andam um quadro (pela tabela ou pela taxa da saída), Shift+↑/↓ um segundo.
 * O ícone diz a base: tempo da fonte ou do programa.
 */

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Clapperboard, Film } from "lucide-react";
import type { Pts, Racional, Seg, TabelaQuadros } from "@domain/edicao";
import { formatarTempo, lerTempo, passoQuadro, quadroDe, type BaseTempo, type RefTempo } from "./tempoTexto";
import styles from "./CampoTempo.module.css";

interface Props {
  label?: ReactNode;
  value: Seg | null;
  /** `pts` só quando o quadro veio da tabela (#n ou setas na base "fonte"). */
  onChange: (t: Seg, pts: Pts | null) => void;
  base: BaseTempo;
  tabela?: TabelaQuadros | null;
  fps?: Racional | null;
  min?: Seg;
  max?: Seg;
  disabled?: boolean;
  onEnter?: () => void;
  onEscape?: () => void;
  autoFocus?: boolean;
  className?: string;
}

export function CampoTempo({ label, value, onChange, base, tabela, fps, min, max, disabled, onEnter, onEscape, autoFocus, className }: Props) {
  const ref: RefTempo = { base, tabela, fps };
  const fmt = (v: Seg | null) => (v === null ? "" : formatarTempo(v));
  const [texto, setTexto] = useState(fmt(value));
  const [invalido, setInvalido] = useState(false);
  useEffect(() => {
    setTexto(fmt(value));
    setInvalido(false);
  }, [value]); // eslint-disable-line react-hooks/exhaustive-deps
  const dentro = (t: Seg) => (min === undefined || t >= min) && (max === undefined || t <= max);
  const emitir = (t: Seg, pts: Pts | null) => {
    if (!dentro(t)) return false;
    onChange(t, pts);
    setTexto(formatarTempo(t));
    return true;
  };
  const confirmar = () => {
    if (texto === fmt(value)) return;
    const lido = lerTempo(texto, ref);
    if (lido && emitir(lido.t, lido.pts)) setInvalido(false);
    else if (texto.trim() === "") setTexto(fmt(value));
    else setInvalido(true);
  };
  const quadro = value === null ? null : quadroDe(value, ref);
  const id = useRef(`ct-${Math.random().toString(36).slice(2, 8)}`).current;
  const Icone = base === "fonte" ? Film : Clapperboard;
  return (
    <div className={`${styles.campo} ${className ?? ""}`}>
      {label && (
        <span id={id} className={styles.rotulo}>
          {label}
        </span>
      )}
      <span className={`${styles.caixa} ${invalido ? styles.invalido : ""} ${disabled ? styles.desligado : ""}`}>
        <Icone size={14} className={styles.base} aria-hidden />
        <input
          type="text"
          inputMode="decimal"
          spellCheck={false}
          value={texto}
          disabled={disabled}
          autoFocus={autoFocus}
          aria-labelledby={label ? id : undefined}
          aria-invalid={invalido || undefined}
          onChange={(e) => {
            setTexto(e.target.value);
            setInvalido(false);
          }}
          onBlur={confirmar}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              confirmar();
              onEnter?.();
            } else if (e.key === "Escape") {
              setTexto(fmt(value));
              setInvalido(false);
              onEscape?.();
            } else if ((e.key === "ArrowUp" || e.key === "ArrowDown") && value !== null) {
              e.preventDefault();
              const sentido = e.key === "ArrowUp" ? 1 : -1;
              const p = e.shiftKey ? { t: value + sentido, pts: null } : passoQuadro(value, sentido, ref);
              emitir(p.t, p.pts);
            }
          }}
        />
        {quadro !== null && <span className={styles.quadro}>#{quadro}</span>}
      </span>
    </div>
  );
}
