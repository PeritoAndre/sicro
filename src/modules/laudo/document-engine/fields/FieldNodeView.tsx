/**
 * FieldNodeView — pílula de campo automático renderizada como React no editor.
 *
 * Lê o `attrs.field` do node + occurrence/metadata dos stores e mostra o
 * VALOR RESOLVIDO (não o placeholder `{key}`). Reativo: quando o perito edita
 * o `metadata` (no Inspector ou no painel Campos) ou troca de ocorrência, a
 * pílula atualiza automaticamente.
 *
 * Estados visuais:
 *   - resolvido (verde): tem valor → mostra o valor com label do campo no title.
 *   - pendente (laranja): valor vazio → mostra `{key}` chip pra o perito ver
 *     o que ainda precisa preencher.
 */

import { useMemo } from "react";
import {
  NodeViewWrapper,
  type NodeViewProps,
  type ReactNodeViewRendererOptions,
} from "@tiptap/react";
import { useLaudoStore } from "../../store/laudoStore";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { findField } from "./catalog";
import { resolveDefinition } from "./resolver";

export function FieldNodeView({ node }: NodeViewProps) {
  const key = (node.attrs["field"] as string) || "";
  // Selectors granulares pra evitar rerender da pílula a cada keystroke do
  // editor (o doc inteiro muda, mas metadata só muda em saves explícitos).
  const metadata = useLaudoStore((s) => s.currentDoc?.metadata ?? null);
  const occurrence = useWorkspaceStore((s) => s.activeOccurrence);

  const { value, label } = useMemo(() => {
    const def = findField(key);
    if (!def) {
      return { value: "", label: key };
    }
    const v = resolveDefinition(def, {
      metadata: metadata as Record<string, unknown> | null,
      occurrence: occurrence as unknown as Record<string, unknown> | null,
    }).trim();
    return { value: v, label: def.label };
  }, [key, metadata, occurrence]);

  const resolved = value.length > 0;
  return (
    <NodeViewWrapper
      as="span"
      className={
        resolved
          ? "sicro-field sicro-field-resolved"
          : "sicro-field sicro-field-placeholder"
      }
      data-field={key}
      data-resolved={resolved ? "true" : "false"}
      title={
        resolved ? `${label} (campo automático)` : `${label} — sem valor ainda`
      }
      contentEditable={false}
    >
      {resolved ? value : `{${key}}`}
    </NodeViewWrapper>
  );
}

/**
 * Re-exporta o tipo das opções pra simplificar o uso em `addNodeView`.
 * (Re-export curto evita um import a mais nos chamadores.)
 */
export type FieldNodeViewOptions = ReactNodeViewRendererOptions;
