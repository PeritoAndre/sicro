/** Seletor do que detectar: as 80 classes do COCO em português, por grupo, com busca e atalhos. */
import { useMemo, useState } from "react";
import type { ClasseInfo } from "@domain/video_pessoas";
import styles from "./PessoasPanel.module.css";

interface Props {
  catalogo: ClasseInfo[];
  selecionadas: number[];
  onChange: (ids: number[]) => void;
  disabled?: boolean;
}

const semAcento = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

/** Resumo curto da escolha para o cabeçalho recolhido. */
export function resumoClasses(catalogo: ClasseInfo[], sel: number[]): string {
  if (catalogo.length > 0 && sel.length >= catalogo.length) return "Tudo (80)";
  if (sel.length === 1) return catalogo.find((c) => c.id === sel[0])?.nome ?? "1 classe";
  const pv = idsDosGrupos(catalogo, ["Pessoas", "Veículos"]);
  if (pv.length > 0 && sel.length === pv.length && pv.every((i) => sel.includes(i))) return "Pessoas e veículos";
  return `${sel.length} classes`;
}

export const idsDosGrupos = (catalogo: ClasseInfo[], grupos: string[]) =>
  catalogo.filter((c) => grupos.includes(c.grupo)).map((c) => c.id);

export function ClassesSeletor({ catalogo, selecionadas, onChange, disabled }: Props) {
  const [busca, setBusca] = useState("");
  const sel = useMemo(() => new Set(selecionadas), [selecionadas]);
  const q = semAcento(busca.trim());
  const grupos = useMemo(() => {
    const m = new Map<string, ClasseInfo[]>();
    for (const c of catalogo) {
      if (q && !semAcento(c.nome).includes(q)) continue;
      m.set(c.grupo, [...(m.get(c.grupo) ?? []), c]);
    }
    return [...m.entries()];
  }, [catalogo, q]);

  const alternar = (ids: number[], ligar: boolean) => {
    const novo = new Set(sel);
    for (const i of ids) {
      if (ligar) novo.add(i);
      else novo.delete(i);
    }
    // sempre fica ao menos uma classe
    if (novo.size > 0) onChange([...novo].sort((a, b) => a - b));
  };

  return (
    <div className={styles.classes}>
      <div className={styles.line}>
        <button type="button" className={styles.chip} disabled={disabled} onClick={() => onChange([0])}>
          Só pessoas
        </button>
        <button
          type="button"
          className={styles.chip}
          disabled={disabled}
          onClick={() => onChange(idsDosGrupos(catalogo, ["Pessoas", "Veículos"]))}
        >
          Pessoas e veículos
        </button>
        <button type="button" className={styles.chip} disabled={disabled} onClick={() => onChange(catalogo.map((c) => c.id))}>
          Tudo
        </button>
      </div>
      <input
        className={styles.search}
        type="search"
        placeholder="buscar classe"
        value={busca}
        onChange={(e) => setBusca(e.target.value)}
      />
      <div className={styles.groups}>
        {grupos.map(([nome, itens]) => {
          const ids = itens.map((c) => c.id);
          const n = ids.filter((i) => sel.has(i)).length;
          return (
            <details key={nome} open={q !== "" || nome === "Pessoas" || nome === "Veículos"}>
              <summary className={styles.groupHead}>
                <input
                  type="checkbox"
                  checked={n === ids.length}
                  ref={(el) => {
                    if (el) el.indeterminate = n > 0 && n < ids.length;
                  }}
                  disabled={disabled}
                  aria-label={`Marcar todo o grupo ${nome}`}
                  onClick={(e) => e.stopPropagation()}
                  onChange={(e) => alternar(ids, e.target.checked)}
                />
                <span>{nome}</span>
                <span className={styles.dim}>
                  {n}/{ids.length}
                </span>
              </summary>
              <div className={styles.groupBody}>
                {itens.map((c) => (
                  <label key={c.id}>
                    <input type="checkbox" checked={sel.has(c.id)} disabled={disabled} onChange={(e) => alternar([c.id], e.target.checked)} />
                    {c.nome}
                  </label>
                ))}
              </div>
            </details>
          );
        })}
        {grupos.length === 0 && <span className={styles.dim}>Nenhuma classe com esse nome.</span>}
      </div>
    </div>
  );
}
