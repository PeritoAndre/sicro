/**
 * KeyboardShortcutsDialog — modal listando todos os atalhos do laudo.
 *
 * F12.5 — Aberto via tecla `?` ou botão de status bar. Lista completa
 * dos atalhos organizados em seções.
 */

import { useEffect } from "react";
import { Keyboard, X } from "lucide-react";
import styles from "./KeyboardShortcutsDialog.module.css";

interface KeyboardShortcutsDialogProps {
  open: boolean;
  onClose: () => void;
}

interface Shortcut {
  keys: string;
  label: string;
}

interface Section {
  title: string;
  items: Shortcut[];
}

const SECTIONS: Section[] = [
  {
    title: "Documento",
    items: [
      { keys: "Ctrl + S", label: "Salvar laudo" },
      { keys: "Ctrl + Z", label: "Desfazer" },
      { keys: "Ctrl + Shift + Z", label: "Refazer" },
      { keys: "Ctrl + F", label: "Localizar" },
      { keys: "Ctrl + H", label: "Localizar e substituir" },
      { keys: "Ctrl + P", label: "Exportar PDF" },
    ],
  },
  {
    title: "Formatação",
    items: [
      { keys: "Ctrl + B", label: "Negrito" },
      { keys: "Ctrl + I", label: "Itálico" },
      { keys: "Ctrl + U", label: "Sublinhado" },
      { keys: "Ctrl + Shift + X", label: "Tachado" },
    ],
  },
  {
    title: "Estilos do laudo (Ctrl + Alt)",
    items: [
      { keys: "Ctrl + Alt + 0", label: "Normal (parágrafo)" },
      { keys: "Ctrl + Alt + 1", label: "Título 1 (principal)" },
      { keys: "Ctrl + Alt + 2", label: "Título 2 (seção)" },
      { keys: "Ctrl + Alt + 3", label: "Título 3 (subseção)" },
      { keys: "Ctrl + Alt + 4", label: "Subtítulo" },
      { keys: "Ctrl + Alt + 5", label: "Seção técnica" },
      { keys: "Ctrl + Alt + 6", label: "Quesito" },
      { keys: "Ctrl + Alt + 7", label: "Resposta" },
    ],
  },
  {
    title: "Visualização",
    items: [
      { keys: "Ctrl + Wheel", label: "Zoom in/out" },
      { keys: "Ctrl + +", label: "Zoom in" },
      { keys: "Ctrl + -", label: "Zoom out" },
      { keys: "Ctrl + 0", label: "Zoom 100%" },
      { keys: "Esc", label: "Fechar localizar / sair do modo Foco" },
    ],
  },
  {
    title: "Réguas",
    items: [
      { keys: "Arrastar handle", label: "Ajustar margem em tempo real" },
    ],
  },
  {
    title: "Ajuda",
    items: [
      { keys: "?", label: "Abrir este menu" },
      { keys: "Esc", label: "Fechar este menu" },
    ],
  },
];

export function KeyboardShortcutsDialog({
  open,
  onClose,
}: KeyboardShortcutsDialogProps) {
  // Esc fecha.
  useEffect(() => {
    if (!open) return undefined;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      className={styles.overlay}
      role="dialog"
      aria-modal="true"
      aria-labelledby="shortcuts-title"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className={styles.dialog}>
        <header className={styles.header}>
          <strong id="shortcuts-title">
            <Keyboard size={16} /> Atalhos de teclado
          </strong>
          <button
            type="button"
            className={styles.closeBtn}
            onClick={onClose}
            aria-label="Fechar"
          >
            <X size={14} />
          </button>
        </header>
        <div className={styles.body}>
          {SECTIONS.map((section) => (
            <div key={section.title} className={styles.section}>
              <h3 className={styles.sectionTitle}>{section.title}</h3>
              <table className={styles.shortcutTable}>
                <tbody>
                  {section.items.map((s) => (
                    <tr key={s.keys}>
                      <td className={styles.keys}>
                        {s.keys.split(" + ").map((k, i) => (
                          <span key={i}>
                            {i > 0 && <span className={styles.plus}>+</span>}
                            <kbd className={styles.kbd}>{k}</kbd>
                          </span>
                        ))}
                      </td>
                      <td className={styles.label}>{s.label}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
