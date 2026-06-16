/**
 * SignatureBadge — pílula visual indicando o tipo de assinatura
 * digital aplicada a um laudo.
 *
 * H — Aparece na lista de laudos quando `laudo.signature_type` está
 * presente. Cores:
 *   - gov_br → verde "Assinado gov.br" (com ícone ShieldCheck)
 *   - A1/A3  → azul "Assinado ICP-Brasil"
 *   - mock   → cinza "Assinatura mock"
 */

import { ShieldCheck } from "lucide-react";
import styles from "./SignatureBadge.module.css";

interface Props {
  type: "gov_br" | "sigdocs" | "A1" | "A3" | "mock" | null | undefined;
  /** Compacta (sem ícone, texto curto) ou normal. */
  compact?: boolean;
}

export function SignatureBadge({ type, compact = false }: Props) {
  if (!type) return null;
  const variant =
    type === "gov_br"
      ? "govBr"
      : type === "sigdocs"
        ? "sigdocs"
        : type === "mock"
          ? "mock"
          : "icp";
  const label =
    type === "gov_br"
      ? "Assinado gov.br"
      : type === "sigdocs"
        ? "Assinado SIGDOCS"
        : type === "mock"
          ? "Mock"
          : `Assinado ${type}`;
  const short =
    type === "gov_br"
      ? "gov.br"
      : type === "sigdocs"
        ? "SIGDOCS"
        : type === "mock"
          ? "mock"
          : type;

  return (
    <span
      className={`${styles.badge} ${styles[variant]}`}
      title={label}
      aria-label={label}
    >
      <ShieldCheck size={compact ? 10 : 12} />
      {compact ? short : label}
    </span>
  );
}
