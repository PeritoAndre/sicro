/**
 * FeedbackButton — botão "Feedback" na Home.
 *
 * Abre um diálogo com dois caminhos de contato:
 *   1. Issues do projeto no GitHub (reportar bug / sugerir);
 *   2. E-mail do software (copiar ou abrir no app de e-mail).
 *
 * Usa o `plugin-shell` para abrir URL/`mailto:` no navegador/app padrão (não
 * dentro do app) e o `plugin-clipboard-manager` para copiar o e-mail.
 */

import { useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { open as openShell } from "@tauri-apps/plugin-shell";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  Check,
  Copy,
  ExternalLink,
  Github,
  Mail,
  MessageSquare,
  X,
} from "lucide-react";
import { Button } from "@components/Button/Button";
import { pushToast } from "@/components/toast/toastStore";

const ISSUES_URL = "https://github.com/PeritoAndre/SICRO-desktop/issues";
const SUPPORT_EMAIL = "andre.barroso@policiacientifica.ap.gov.br";

export function FeedbackButton() {
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);

  const openIssues = () => {
    void openShell(ISSUES_URL).catch(() =>
      pushToast("error", "Não consegui abrir o navegador."),
    );
  };

  const openEmail = () => {
    const subject = encodeURIComponent("Feedback — SICRO Desktop");
    void openShell(`mailto:${SUPPORT_EMAIL}?subject=${subject}`).catch(() =>
      pushToast("warn", "Sem app de e-mail padrão. Copie o endereço."),
    );
  };

  const copyEmail = async () => {
    try {
      await writeText(SUPPORT_EMAIL);
    } catch {
      try {
        await navigator.clipboard.writeText(SUPPORT_EMAIL);
      } catch {
        pushToast("error", "Não consegui copiar.");
        return;
      }
    }
    setCopied(true);
    pushToast("success", "E-mail copiado.");
    window.setTimeout(() => setCopied(false), 1500);
  };

  return (
    <>
      <Button
        variant="ghost"
        size="sm"
        leftIcon={<MessageSquare size={14} />}
        onClick={() => setOpen(true)}
      >
        Feedback
      </Button>

      {open &&
        createPortal(
          <div style={overlay} role="presentation" onClick={() => setOpen(false)}>
            <div
              style={card}
              role="dialog"
              aria-modal="true"
              aria-label="Feedback e suporte"
              onClick={(e) => e.stopPropagation()}
            >
              <header style={head}>
                <div style={headTitle}>
                  <MessageSquare size={16} aria-hidden />
                  <h2 style={h2}>Feedback e suporte</h2>
                </div>
                <button
                  type="button"
                  style={closeBtn}
                  onClick={() => setOpen(false)}
                  aria-label="Fechar"
                >
                  <X size={16} />
                </button>
              </header>

              <p style={note}>
                Achou um problema ou tem uma sugestão? Fale com a gente — toda
                observação ajuda a melhorar o SICRO.
              </p>

              <button type="button" style={optionBtn} onClick={openIssues}>
                <Github size={18} aria-hidden />
                <span style={optionText}>
                  <strong>Reportar ou sugerir no GitHub</strong>
                  <span style={optionSub}>Abre as issues do projeto no navegador</span>
                </span>
                <ExternalLink size={15} style={{ opacity: 0.6 }} aria-hidden />
              </button>

              <div style={emailRow}>
                <Mail size={18} aria-hidden />
                <span style={optionText}>
                  <strong>E-mail do software</strong>
                  <code style={emailCode} title={SUPPORT_EMAIL}>
                    {SUPPORT_EMAIL}
                  </code>
                </span>
                <button
                  type="button"
                  style={iconBtn}
                  onClick={() => void copyEmail()}
                  title="Copiar e-mail"
                  aria-label="Copiar e-mail"
                >
                  {copied ? <Check size={15} /> : <Copy size={15} />}
                </button>
                <button
                  type="button"
                  style={iconBtn}
                  onClick={openEmail}
                  title="Abrir no aplicativo de e-mail"
                  aria-label="Abrir no aplicativo de e-mail"
                >
                  <ExternalLink size={15} />
                </button>
              </div>
            </div>
          </div>,
          document.body,
        )}
    </>
  );
}

// --- estilos inline (componente auto-contido) -----------------------------

const overlay: CSSProperties = {
  position: "fixed",
  inset: 0,
  zIndex: 1000,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  background: "rgba(0, 0, 0, 0.55)",
  padding: "var(--space-4)",
};
const card: CSSProperties = {
  width: "100%",
  maxWidth: 440,
  display: "flex",
  flexDirection: "column",
  gap: "var(--space-3)",
  background: "var(--sicro-surface-1)",
  border: "1px solid var(--sicro-divider)",
  borderRadius: "var(--radius-lg)",
  boxShadow: "0 18px 50px rgba(0, 0, 0, 0.45)",
  padding: "var(--space-5)",
};
const head: CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: "var(--space-2)",
};
const headTitle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "var(--space-2)",
  color: "var(--sicro-accent)",
};
const h2: CSSProperties = {
  margin: 0,
  fontSize: "var(--text-lg)",
  fontWeight: 600,
  color: "var(--sicro-fg)",
};
const closeBtn: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  width: 28,
  height: 28,
  border: "none",
  background: "transparent",
  color: "var(--sicro-fg-muted)",
  borderRadius: "var(--radius-sm)",
  cursor: "pointer",
};
const note: CSSProperties = {
  margin: 0,
  fontSize: "var(--text-sm)",
  lineHeight: 1.5,
  color: "var(--sicro-fg-dim)",
};
const optionBtn: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "var(--space-3)",
  width: "100%",
  textAlign: "left",
  padding: "var(--space-3)",
  background: "var(--sicro-surface-2)",
  border: "1px solid var(--sicro-divider)",
  borderRadius: "var(--radius-md)",
  color: "var(--sicro-fg)",
  cursor: "pointer",
  font: "inherit",
};
const optionText: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 2,
  flex: 1,
  minWidth: 0,
};
const optionSub: CSSProperties = {
  fontSize: "var(--text-xs)",
  color: "var(--sicro-fg-dim)",
};
const emailRow: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "var(--space-3)",
  padding: "var(--space-3)",
  background: "var(--sicro-surface-2)",
  border: "1px solid var(--sicro-divider)",
  borderRadius: "var(--radius-md)",
  color: "var(--sicro-fg)",
};
const emailCode: CSSProperties = {
  fontSize: "var(--text-xs)",
  color: "var(--sicro-fg-dim)",
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
};
const iconBtn: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  width: 30,
  height: 30,
  flexShrink: 0,
  border: "1px solid var(--sicro-divider)",
  background: "var(--sicro-surface-1)",
  color: "var(--sicro-fg-muted)",
  borderRadius: "var(--radius-sm)",
  cursor: "pointer",
};
