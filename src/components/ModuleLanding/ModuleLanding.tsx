/**
 * ModuleLanding — tela inicial padrão dos módulos (padrão "ouro" extraído da
 * Documentoscopia). Herói centralizado: ícone grande + título + subtítulo +
 * ações primárias, uma grade de cards explicando as capacidades, e um rodapé
 * com a nota metodológica (§13 — apoio técnico, decisão humana).
 *
 * Usada por Laudo, Croqui, Vídeo, Áudio e Imagem para uniformizar a entrada
 * de cada módulo. `children` (opcional) entra entre os cards e o rodapé —
 * útil para listar itens já existentes logo abaixo do herói.
 */
import type { ReactNode } from "react";
import styles from "./ModuleLanding.module.css";

export interface ModuleLandingFeature {
  icon: ReactNode;
  title: string;
  desc: string;
}

interface Props {
  /** Ícone grande do herói (ex.: lucide com size ~44, strokeWidth 1.2). */
  icon: ReactNode;
  title: string;
  subtitle: string;
  /** Botões de ação primária (ex.: Importar / Nova análise). */
  actions?: ReactNode;
  /** Cards de capacidades (3–6). */
  features?: ModuleLandingFeature[];
  /** Nota metodológica do rodapé (§13). */
  note?: string;
  /** Conteúdo extra (ex.: lista de itens existentes) entre cards e rodapé. */
  children?: ReactNode;
}

export function ModuleLanding({
  icon,
  title,
  subtitle,
  actions,
  features,
  note,
  children,
}: Props) {
  return (
    <div className={styles.wrap}>
      <div className={styles.hero}>
        <span className={styles.heroIcon}>{icon}</span>
        <h2 className={styles.title}>{title}</h2>
        <p className={styles.sub}>{subtitle}</p>
        {actions && <div className={styles.cta}>{actions}</div>}
      </div>
      {features && features.length > 0 && (
        <div className={styles.cards}>
          {features.map((c) => (
            <div key={c.title} className={styles.card}>
              <span className={styles.cardIcon}>{c.icon}</span>
              <div>
                <div className={styles.cardTitle}>{c.title}</div>
                <div className={styles.cardDesc}>{c.desc}</div>
              </div>
            </div>
          ))}
        </div>
      )}
      {children}
      {note && <p className={styles.note}>{note}</p>}
    </div>
  );
}
