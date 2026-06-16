/**
 * SummaryPanel — Sumário + Lista de figuras + Lista de tabelas (F10).
 *
 * 3 ações principais:
 *   1. Inserir sumário (TOC)        — gera bullet-list dos titulo_1/2/3.
 *   2. Inserir lista de figuras     — bullet-list das figuras numeradas.
 *   3. Inserir lista de tabelas     — bullet-list das tabelas numeradas.
 *
 * Comportamento:
 *   - Cada ação insere o bloco gerado no cursor atual.
 *   - As listas são "snapshots" — não se atualizam automaticamente. O perito
 *     pode regerar via o mesmo botão quando o documento crescer.
 *   - As listas são prefixadas por um título (Sumário, Lista de figuras…).
 *
 * Auto-update: rastrear posição do documento e re-inserir é fora do escopo
 * desta versão MVP (exigiria um Mark/Decoration com identidade própria).
 */

import { useMemo } from "react";
import type { Editor } from "@tiptap/react";
import type { JSONContent } from "@tiptap/core";
import { ListChecks, ListOrdered, ListTree, Plus } from "lucide-react";
import {
  buildFigureList,
  buildTableList,
  extractFigures,
  extractOutline,
  extractTables,
  numberOutline,
  type SicroDoc,
} from "../document-engine";
import styles from "./SummaryPanel.module.css";

interface SummaryPanelProps {
  editor: Editor | null;
  doc: SicroDoc | null;
}

export function SummaryPanel({ editor, doc }: SummaryPanelProps) {
  const outline = useMemo(
    () => (doc ? numberOutline(extractOutline(doc.content)) : []),
    [doc],
  );
  const figures = useMemo(
    () => (doc ? buildFigureList(extractFigures(doc.content)) : []),
    [doc],
  );
  const tables = useMemo(
    () => (doc ? buildTableList(extractTables(doc.content)) : []),
    [doc],
  );

  const handleInsertSummary = () => {
    if (!editor) return;
    if (outline.length === 0) {
      window.alert("Nenhum título encontrado para o sumário.");
      return;
    }
    // Insere o NÓ DINÂMICO de sumário (não um snapshot estático). Ele já
    // renderiza o próprio título "SUMÁRIO" + a lista, atualiza sozinho quando os
    // títulos mudam e, na exportação, vira um índice de verdade (TOC nativo no
    // DOCX/PDF-LibreOffice, com números de página; lista no PDF do navegador).
    editor.chain().focus().insertContent({ type: "dynamicSummary" }).run();
  };

  const handleInsertFigures = () => {
    if (!editor) return;
    if (figures.length === 0) {
      window.alert("Nenhuma figura encontrada no documento.");
      return;
    }
    const content: JSONContent[] = [
      {
        type: "heading",
        attrs: { level: 2, "data-laudo-style": "titulo_2" },
        content: [{ type: "text", text: "LISTA DE FIGURAS" }],
      },
      {
        type: "bulletList",
        content: figures.map((f) => ({
          type: "listItem",
          content: [
            {
              type: "paragraph",
              content: [
                {
                  type: "text",
                  text: `${f.label} — ${f.caption || "(sem legenda)"}`,
                },
              ],
            },
          ],
        })),
      },
    ];
    editor.chain().focus().insertContent(content).run();
  };

  const handleInsertTables = () => {
    if (!editor) return;
    if (tables.length === 0) {
      window.alert("Nenhuma tabela encontrada no documento.");
      return;
    }
    const content: JSONContent[] = [
      {
        type: "heading",
        attrs: { level: 2, "data-laudo-style": "titulo_2" },
        content: [{ type: "text", text: "LISTA DE TABELAS" }],
      },
      {
        type: "bulletList",
        content: tables.map((t) => ({
          type: "listItem",
          content: [
            {
              type: "paragraph",
              content: [
                {
                  type: "text",
                  text: `${t.label} (${t.rowCount}×${t.colCount}) — ${
                    t.firstCell || "(sem título)"
                  }`,
                },
              ],
            },
          ],
        })),
      },
    ];
    editor.chain().focus().insertContent(content).run();
  };

  if (!editor) {
    return <p className={styles.empty}>Abra um laudo para gerar listas.</p>;
  }

  return (
    <>
      <h3 className={styles.sectionTitle}>
        <ListTree size={14} /> Sumário & listas
      </h3>
      <p className={styles.intro}>
        Os blocos abaixo geram listas estáticas a partir do conteúdo atual.
        Reaplique quando o documento mudar para mantê-las em dia.
      </p>

      <div className={styles.actionRow}>
        <button
          type="button"
          className={styles.action}
          onClick={handleInsertSummary}
        >
          <Plus size={11} />
          <div>
            <strong>Sumário</strong>
            <span>{outline.length} título(s)</span>
          </div>
        </button>
        <button
          type="button"
          className={styles.action}
          onClick={handleInsertFigures}
        >
          <ListOrdered size={11} />
          <div>
            <strong>Lista de figuras</strong>
            <span>{figures.length} figura(s)</span>
          </div>
        </button>
        <button
          type="button"
          className={styles.action}
          onClick={handleInsertTables}
        >
          <ListChecks size={11} />
          <div>
            <strong>Lista de tabelas</strong>
            <span>{tables.length} tabela(s)</span>
          </div>
        </button>
      </div>
    </>
  );
}
