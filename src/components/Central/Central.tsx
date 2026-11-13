/** Central: título com traço, lâminas para criar e a lista "Neste caso". */

import type { MouseEvent, ReactNode } from "react";
import { Plus, Trash2 } from "lucide-react";
import { formatDateTime } from "@core/formatters";
import styles from "./Central.module.css";

export interface CentralLamina {
  id: string;
  nome: string;
  arte: string;
  onClick: () => void;
}

export interface CentralItem {
  id: string;
  titulo: string;
  /** Conteúdo da miniatura 16:10 (img, ícone…). */
  miniatura: ReactNode;
  tipo: string;
  resumo?: string;
  /** ISO; formatado à direita. */
  quando: string;
  onAbrir: () => void;
  onExcluir?: () => void;
  excluirRotulo?: string;
}

export function CentralMini({ children }: { children: ReactNode }) {
  return <span className={styles.miniVazia}>{children}</span>;
}

export function Central({
  titulo,
  laminas,
  itens,
  carregando,
  busy,
  erro,
  children,
}: {
  titulo: string;
  laminas: CentralLamina[];
  itens: CentralItem[];
  carregando?: boolean;
  busy?: boolean;
  erro?: string | null;
  /** Diálogos e afins, fora do fluxo. */
  children?: ReactNode;
}) {
  const vazio = !carregando && itens.length === 0;
  return (
    <div className={`${styles.wrap} ${vazio ? styles.vazio : ""}`}>
      <div className={styles.container}>
        <h1 className={styles.title}>{titulo}</h1>
        <div
          className={styles.laminas}
          data-n={laminas.length}
          style={{ ["--n" as string]: laminas.length }}
        >
          {laminas.map((l) => (
            <button key={l.id} type="button" className={styles.lamina} onClick={l.onClick} disabled={busy}>
              <span className={styles.arte}>
                <img src={l.arte} alt="" />
              </span>
              <span className={styles.nome}>
                {l.nome}
                <Plus size={20} />
              </span>
            </button>
          ))}
        </div>
        {erro && <p className={styles.error}>{erro}</p>}

        {!vazio && (
          <section className={styles.lista}>
            <h2 className={styles.sectionTitle}>Neste caso</h2>
            {carregando && itens.length === 0 ? (
              <p className={styles.dim}>Carregando…</p>
            ) : (
              <ul className={styles.list}>
                {itens.map((c) => (
                  <li key={c.id} className={styles.row}>
                    <button
                      type="button"
                      className={styles.rowBtn}
                      onClick={c.onAbrir}
                      disabled={busy}
                      aria-label={`Abrir ${c.titulo}`}
                    >
                      <span className={styles.mini}>{c.miniatura}</span>
                      <span className={styles.rowMain}>
                        <strong className={styles.rowTitle}>{c.titulo}</strong>
                        <span className={styles.rowMeta}>
                          <span className={styles.chip}>{c.tipo}</span>
                          {c.resumo}
                        </span>
                      </span>
                      <span className={styles.quando}>{formatDateTime(c.quando)}</span>
                    </button>
                    {c.onExcluir && (
                      <button
                        type="button"
                        className={styles.deleteBtn}
                        title={c.excluirRotulo ?? "Excluir"}
                        aria-label={`${c.excluirRotulo ?? "Excluir"} ${c.titulo}`}
                        disabled={busy}
                        onClick={(e: MouseEvent<HTMLButtonElement>) => {
                          e.stopPropagation();
                          c.onExcluir?.();
                        }}
                      >
                        <Trash2 size={14} />
                      </button>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </section>
        )}
      </div>
      {children}
    </div>
  );
}
