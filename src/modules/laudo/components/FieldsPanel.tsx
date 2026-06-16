/**
 * FieldsPanel — painel "{} Campos" da barra do laudo.
 *
 * Faz duas coisas (reformulação tipo Word):
 *   1. INSERIR: o botão "+" insere a pílula do campo no cursor (`{key}`), que
 *      no editor aparece já com o valor resolvido.
 *   2. EDITAR: cada campo tem um input. O que o perito digita vira um OVERRIDE
 *      LOCAL do laudo (`metadata[key]`) — muda o valor da pílula só naquele
 *      laudo, sem tocar na ocorrência (fonte de verdade do caso). Em branco =
 *      usa o valor herdado do caso (mostrado como placeholder).
 *
 * Campos computados (data atual, contador de página) não são editáveis —
 * mostram o valor/explicação em modo leitura.
 */

import type { Editor } from "@tiptap/react";
import { useMemo, useState } from "react";
import { AlertCircle, Plus, RotateCcw } from "lucide-react";
import {
  FIELD_GROUPS,
  fieldsByGroup,
  findMissingRequiredFields,
  groupLabel,
  LAUDO_FIELDS,
  resolveFromSource,
  type FieldResolveContext,
  type LaudoFieldDefinition,
  type SicroDoc,
} from "../document-engine";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { useLaudoStore } from "../store/laudoStore";
import { pushToast } from "@/components/toast/toastStore";
import styles from "./FieldsPanel.module.css";

interface FieldsPanelProps {
  editor: Editor | null;
  doc: SicroDoc | null;
}

function isEditable(def: LaudoFieldDefinition): boolean {
  return def.source.kind !== "system" && def.source.kind !== "page_counter";
}

export function FieldsPanel({ editor, doc }: FieldsPanelProps) {
  const activeOccurrence = useWorkspaceStore((s) => s.activeOccurrence);
  const activeWorkspacePath = useWorkspaceStore((s) => s.activeWorkspacePath);
  const updateMetadata = useLaudoStore((s) => s.updateMetadata);

  // Buffer de edição por campo (commit no blur/Enter). undefined = "ainda não
  // tocado" → semeia do override salvo (metadata[key]).
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState<string | null>(null);

  const metadata = useMemo(
    () => (doc?.metadata ?? {}) as Record<string, unknown>,
    [doc?.metadata],
  );

  const ctx = useMemo<FieldResolveContext>(
    () => ({
      metadata,
      occurrence: activeOccurrence as unknown as
        | Record<string, unknown>
        | undefined,
    }),
    [metadata, activeOccurrence],
  );

  const missing = useMemo(
    () => findMissingRequiredFields(ctx, LAUDO_FIELDS),
    [ctx],
  );

  const overrideOf = (key: string): string => {
    const v = metadata[key];
    return v == null ? "" : String(v);
  };
  const draftOf = (key: string): string => drafts[key] ?? overrideOf(key);

  const insert = (def: LaudoFieldDefinition) => {
    if (!editor) return;
    editor.chain().focus().insertFieldPlaceholder(def.key).run();
  };

  const commit = async (def: LaudoFieldDefinition, raw: string) => {
    const value = raw.trim();
    if (value === overrideOf(def.key)) return; // nada mudou
    if (!activeWorkspacePath) {
      pushToast("warn", "Abra um laudo para editar os campos.");
      return;
    }
    setSaving(def.key);
    try {
      await updateMetadata(activeWorkspacePath, { [def.key]: value });
    } catch (e) {
      pushToast("error", `Não foi possível salvar o campo: ${String(e)}`);
    } finally {
      setSaving(null);
    }
  };

  const clearOverride = async (def: LaudoFieldDefinition) => {
    setDrafts((d) => ({ ...d, [def.key]: "" }));
    await commit(def, "");
  };

  if (!editor) {
    return (
      <p className={styles.empty}>
        Abra um laudo para inserir e editar campos automáticos.
      </p>
    );
  }

  return (
    <>
      <h3 className={styles.sectionTitle}>Campos automáticos</h3>
      <p className={styles.intro}>
        O botão <strong>+</strong> insere a pílula no cursor (ex:{" "}
        <code>{`{numero_bo}`}</code>). Edite o valor abaixo para
        personalizá-lo <strong>só neste laudo</strong> — em branco, usa o valor
        do caso.
      </p>

      {missing.length > 0 && (
        <div className={styles.warning}>
          <AlertCircle size={13} />
          <div>
            <strong>{missing.length}</strong> campo(s) obrigatório(s) ainda sem
            valor:&nbsp;
            {missing.map((f) => f.label).join(", ")}.
          </div>
        </div>
      )}

      {FIELD_GROUPS.map((group) => {
        const items = fieldsByGroup(group);
        if (items.length === 0) return null;
        return (
          <div key={group} className={styles.group}>
            <div className={styles.groupTitle}>{groupLabel(group)}</div>
            {items.map((def) => {
              const editable = isEditable(def);
              const sourceValue = resolveFromSource(def, ctx).trim();
              const hasOverride = overrideOf(def.key).trim().length > 0;
              const missingThis = def.required === true && !sourceValue && !hasOverride;
              return (
                <div
                  key={def.key}
                  className={`${styles.item} ${missingThis ? styles.itemMissing : ""}`}
                >
                  <div className={styles.itemHeader}>
                    <span className={styles.itemLabel}>
                      {def.label}
                      {def.required && (
                        <span
                          className={styles.itemRequiredDot}
                          title="Obrigatório"
                          aria-label="Obrigatório"
                        />
                      )}
                    </span>
                    <span className={styles.itemHeaderRight}>
                      <code className={styles.itemKey}>{`{${def.key}}`}</code>
                      <button
                        type="button"
                        className={styles.insertBtn}
                        title="Inserir esta pílula no cursor"
                        onClick={() => insert(def)}
                      >
                        <Plus size={13} />
                      </button>
                    </span>
                  </div>

                  {editable ? (
                    <>
                      <input
                        className={styles.fieldInput}
                        value={draftOf(def.key)}
                        placeholder={sourceValue || "(sem valor)"}
                        disabled={saving === def.key}
                        onChange={(e) =>
                          setDrafts((d) => ({
                            ...d,
                            [def.key]: e.target.value,
                          }))
                        }
                        onKeyDown={(e) => {
                          if (e.key === "Enter") {
                            e.preventDefault();
                            (e.target as HTMLInputElement).blur();
                          }
                        }}
                        onBlur={(e) => void commit(def, e.target.value)}
                      />
                      {hasOverride && (
                        <button
                          type="button"
                          className={styles.resetBtn}
                          title="Remover personalização e voltar ao valor do caso"
                          onClick={() => void clearOverride(def)}
                        >
                          <RotateCcw size={11} /> usar o valor do caso
                        </button>
                      )}
                    </>
                  ) : (
                    <div className={styles.computedValue}>
                      {def.source.kind === "page_counter"
                        ? "Numerado automaticamente na exportação."
                        : sourceValue || "—"}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        );
      })}
    </>
  );
}
