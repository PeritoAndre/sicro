/**
 * StylesPanel — painel lateral do Inspector com a galeria de estilos
 * documentais do laudo.
 *
 * F4 — UI principal de aplicação de estilos. Substitui o dropdown
 * simples "Texto/Título/Seção/Subseção" da toolbar para os casos onde
 * o perito quer ver TODOS os estilos disponíveis com preview visual e
 * atalhos de teclado.
 *
 * Layout:
 *   - Header com label + ação "Limpar estilo".
 *   - Seção "Estrutura" com Título 1-3, Subtítulo, Seção técnica.
 *   - Seção "Pericial" com Quesito, Resposta, Legenda, Citação,
 *     Observação, Conclusão, Advertência, Assinatura.
 *
 * Cada item:
 *   - Mostra label + descrição curta + atalho (quando definido).
 *   - Renderiza um "chip" com preview visual do estilo (cor de fundo,
 *     borda esquerda, peso de fonte) para reconhecimento rápido.
 *   - Destaca o estilo ativo do bloco no cursor.
 *   - Aplica via `applyLaudoStyle` ao clicar.
 */

import { useEffect, useState } from "react";
import type { Editor } from "@tiptap/react";
import {
  applyLaudoStyle,
  getCurrentLaudoStyle,
  laudoStylesByCategory,
  removeLaudoStyle,
  type LaudoStyleDefinition,
  type LaudoStyleId,
} from "../document-engine";
import styles from "./StylesPanel.module.css";

interface StylesPanelProps {
  editor: Editor | null;
}

export function StylesPanel({ editor }: StylesPanelProps) {
  const grouped = laudoStylesByCategory();
  const [current, setCurrent] = useState<LaudoStyleId | null>(null);

  // Reage à mudança de seleção/conteúdo para atualizar o destaque do
  // estilo ativo. TipTap dispara `selectionUpdate` quando o cursor move
  // e `transaction` em qualquer mudança — basta um.
  useEffect(() => {
    if (!editor) {
      setCurrent(null);
      return undefined;
    }
    const sync = () => setCurrent(getCurrentLaudoStyle(editor));
    sync();
    editor.on("selectionUpdate", sync);
    editor.on("transaction", sync);
    return () => {
      editor.off("selectionUpdate", sync);
      editor.off("transaction", sync);
    };
  }, [editor]);

  if (!editor) {
    return (
      <p className={styles.empty}>
        Abra um laudo para aplicar estilos documentais.
      </p>
    );
  }

  const apply = (id: LaudoStyleId) => {
    applyLaudoStyle(editor, id);
  };
  const clear = () => removeLaudoStyle(editor);

  return (
    <>
      <h3 className={styles.sectionTitle}>Estilos documentais</h3>
      <p className={styles.intro}>
        Clique para aplicar um estilo ao bloco onde está o cursor. Os
        estilos servem tanto à exibição no editor quanto à exportação
        PDF/DOCX.
      </p>

      <button
        type="button"
        className={styles.clearBtn}
        onClick={clear}
        disabled={!current}
        title="Volta o bloco ao estilo padrão"
      >
        Limpar estilo {current ? `(${current})` : ""}
      </button>

      <div className={styles.group}>
        <div className={styles.groupTitle}>Estrutura</div>
        {grouped.estrutura.map((def) => (
          <StyleItem
            key={def.id}
            def={def}
            active={current === def.id}
            onApply={() => apply(def.id)}
          />
        ))}
      </div>

      <div className={styles.group}>
        <div className={styles.groupTitle}>Pericial</div>
        {grouped.pericial.map((def) => (
          <StyleItem
            key={def.id}
            def={def}
            active={current === def.id}
            onApply={() => apply(def.id)}
          />
        ))}
      </div>
    </>
  );
}

interface StyleItemProps {
  def: LaudoStyleDefinition;
  active: boolean;
  onApply: () => void;
}

function StyleItem({ def, active, onApply }: StyleItemProps) {
  // Constrói o preview inline a partir do objeto `preview` do catálogo.
  const previewStyle: React.CSSProperties = {
    fontWeight: def.preview?.fontWeight,
    fontSize: def.preview?.fontSize,
    fontStyle: def.preview?.fontStyle,
    color: def.preview?.color,
    background: def.preview?.background,
    textAlign: def.preview?.textAlign,
    borderLeft: def.preview?.borderLeft,
    paddingLeft: def.preview?.borderLeft ? 6 : undefined,
  };

  return (
    <button
      type="button"
      className={`${styles.item} ${active ? styles.itemActive : ""}`}
      onClick={onApply}
      title={def.description}
    >
      <span className={styles.itemLabel}>
        {def.label}
        {def.shortcut && (
          <span className={styles.itemShortcut}>{def.shortcut}</span>
        )}
      </span>
      <span className={styles.itemPreview} style={previewStyle}>
        {def.label}
      </span>
      <span className={styles.itemDesc}>{def.description}</span>
    </button>
  );
}
