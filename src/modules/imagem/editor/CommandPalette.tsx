/**
 * W13.6 — Paleta de comandos (⌘K / Ctrl+K) do editor de imagem.
 *
 * Overlay central com busca acento-insensível sobre TODAS as ações do editor:
 * trocar ferramenta, mudar o modo do painel (Realçar/Analisar/Anotar), adicionar
 * filtros do catálogo, alternar réguas, enquadrar/zoom, salvar e exportar.
 *
 * Navegação 100% por teclado (↑/↓ + Enter + Esc) — o perito não precisa caçar
 * o botão; digita a intenção ("falsificação", "borda", "salvar") e executa.
 * É só um atalho para ações que já existem na UI — não cria capacidade nova
 * nem altera a evidência (§13).
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { CornerDownLeft, Search } from "lucide-react";
import styles from "./CommandPalette.module.css";

export interface PaletteCommand {
  id: string;
  label: string;
  group: string;
  /** Atalho ou descrição curta exibida à direita. */
  hint?: string;
  /** Sinônimos/intenção para a busca. */
  keywords?: string;
  run: () => void;
}

/** Minúsculas + sem acentos, para casar "falsificacao" com "falsificação". */
function norm(s: string): string {
  return s
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();
}

interface Props {
  commands: PaletteCommand[];
  onClose: () => void;
}

/** Renderizada apenas quando aberta (o pai monta/desmonta). */
export function CommandPalette({ commands, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const filtered = useMemo(() => {
    const q = norm(query.trim());
    if (!q) return commands;
    return commands.filter((c) =>
      norm(`${c.label} ${c.group} ${c.hint ?? ""} ${c.keywords ?? ""}`).includes(q),
    );
  }, [query, commands]);

  // Foca a busca ao montar.
  useEffect(() => {
    const t = setTimeout(() => inputRef.current?.focus(), 0);
    return () => clearTimeout(t);
  }, []);

  // Reseta o item ativo quando a busca muda.
  useEffect(() => setActive(0), [query]);

  // Mantém o item ativo visível.
  useEffect(() => {
    const el = listRef.current?.children[active] as HTMLElement | undefined;
    el?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const run = (c: PaletteCommand | undefined) => {
    if (!c) return;
    onClose();
    c.run();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(a + 1, Math.max(0, filtered.length - 1)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(a - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(filtered[active]);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  };

  return (
    <div className={styles.overlay} onMouseDown={onClose} role="presentation">
      <div
        className={styles.dialog}
        onMouseDown={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Paleta de comandos"
      >
        <div className={styles.searchRow}>
          <Search size={15} />
          <input
            ref={inputRef}
            type="text"
            value={query}
            placeholder="Buscar comando ou filtro…"
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            aria-label="Buscar comando"
          />
          <kbd className={styles.kbd}>Esc</kbd>
        </div>

        <ul className={styles.list} ref={listRef}>
          {filtered.length === 0 && (
            <li className={styles.empty}>Nenhum comando encontrado.</li>
          )}
          {filtered.map((c, i) => (
            <li key={c.id}>
              <button
                type="button"
                className={`${styles.item} ${i === active ? styles.itemActive : ""}`}
                onMouseEnter={() => setActive(i)}
                onClick={() => run(c)}
              >
                <span className={styles.itemGroup}>{c.group}</span>
                <span className={styles.itemLabel}>{c.label}</span>
                {c.hint && <kbd className={styles.kbd}>{c.hint}</kbd>}
              </button>
            </li>
          ))}
        </ul>

        <footer className={styles.footer}>
          <span>
            <CornerDownLeft size={11} /> executar
          </span>
          <span>↑ ↓ navegar</span>
          <span>{filtered.length} comando(s)</span>
        </footer>
      </div>
    </div>
  );
}
