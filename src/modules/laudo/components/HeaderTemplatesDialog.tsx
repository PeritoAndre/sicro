/**
 * HeaderTemplatesDialog — gerenciador de "Cabeçalhos Salvos".
 *
 * Aplica um cabeçalho ao laudo atual (setHeader + altura), salva o cabeçalho
 * atual como um novo modelo (outra unidade/órgão), renomeia e exclui. O modelo
 * "padrão" (builtin) é definido em código e sempre aparece; os demais ficam em
 * `app-settings.json` (global, reutilizável entre laudos).
 */

import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { Check, Pencil, Plus, Stamp, Trash2, X } from "lucide-react";
import { useSettingsStore } from "@stores/settingsStore";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type { JSONContent } from "@tiptap/core";
import type { HeaderTemplate, HeaderTemplateContent } from "@domain/app_settings";
import { useLaudoStore } from "../store/laudoStore";
import {
  collectEvidencePaths,
  embedAssetsAsPortable,
  loadEvidenceAssets,
  type SicroDoc,
  type SicroDocHeader,
} from "../document-engine";
import {
  ensureAndLoadHeaderTemplates,
  isBuiltinTemplate,
  isHeaderContentEmpty,
  newTemplateId,
} from "../headerTemplates";
import styles from "./HeaderTemplatesDialog.module.css";

interface Props {
  open: boolean;
  onClose: () => void;
  doc: SicroDoc;
  workspacePath: string;
}

export function HeaderTemplatesDialog({ open, onClose, doc, workspacePath }: Props) {
  const loaded = useSettingsStore((s) => s.loaded);
  const setHeader = useLaudoStore((s) => s.setHeader);
  const updateLayout = useLaudoStore((s) => s.updateLayout);

  const [all, setAll] = useState<HeaderTemplate[]>([]);
  const [loadingList, setLoadingList] = useState(true);
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [newName, setNewName] = useState("");
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameText, setRenameText] = useState("");

  // Ao abrir: garante settings carregadas, migra o antigo
  // `app-settings.json → header_templates` pra pasta dedicada (one-shot),
  // materializa o builtin e carrega a lista da pasta.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    void (async () => {
      if (!loaded) await useSettingsStore.getState().load();
      setLoadingList(true);
      const legacy =
        useSettingsStore.getState().settings.header_templates ?? [];
      const list = await ensureAndLoadHeaderTemplates(legacy);
      if (cancelled) return;
      setAll(list);
      setLoadingList(false);
      // O campo legado agora vive na pasta — zera pra não re-migrar.
      if (legacy.length > 0) {
        const s = useSettingsStore.getState();
        void s.persist({ ...s.settings, header_templates: [] }).catch(() => {});
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [open, loaded]);

  // Esc fecha o diálogo. Click fora do card foi REMOVIDO (fechava sem querer);
  // só fecha clicando no X ou Esc.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  const reload = async () => {
    try {
      setAll(await ensureAndLoadHeaderTemplates());
    } catch {
      /* mantém a lista atual */
    }
  };

  const apply = async (tpl: HeaderTemplate) => {
    setBusy(true);
    setError(null);
    try {
      const header: SicroDocHeader = {
        content: tpl.content as unknown as SicroDocHeader["content"],
        enabled: true,
      };
      await setHeader(workspacePath, header);
      await updateLayout(workspacePath, {
        header_height_cm: tpl.header_height_cm ?? 2.5,
      });
      setFeedback(`Cabeçalho "${tpl.name}" aplicado ao laudo.`);
      // Não fechamos automaticamente — o perito fecha no X ou Esc quando
      // quiser (assim ele já volta ao laudo e vê o cabeçalho aplicado, mas o
      // diálogo continua disponível pra ele Salvar atual depois de editar).
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(false);
    }
  };

  const saveCurrentAsNew = async () => {
    const name = newName.trim();
    if (!name) return;
    const content = doc.header?.content as unknown as
      | HeaderTemplateContent
      | undefined;
    if (isHeaderContentEmpty(content)) {
      setError(
        'O cabeçalho atual do laudo está VAZIO. Para salvar um novo modelo: (1) clique em "Aplicar" no padrão acima para inserir um cabeçalho no laudo; (2) clique sobre a área do cabeçalho na página A4 e edite o que precisar; (3) volte aqui e clique "Salvar atual".',
      );
      return;
    }
    setBusy(true);
    setError(null);
    try {
      // Torna o cabeçalho PORTÁTIL: embute os brasões como data URI e remove o
      // vínculo com a pasta do caso. Assim o modelo é GLOBAL de verdade —
      // funciona em qualquer laudo/caso/máquina (e pode ser compartilhado com
      // colegas de outros setores). Best-effort: se o embed falhar, salva com
      // os paths originais (ainda funciona dentro deste workspace).
      let portableContent = content as JSONContent;
      try {
        const paths = collectEvidencePaths(portableContent);
        if (paths.size > 0) {
          const assets = await loadEvidenceAssets(workspacePath, paths);
          portableContent = embedAssetsAsPortable(portableContent, assets);
        }
      } catch {
        /* mantém o conteúdo original se o embed dos brasões falhar */
      }
      const tpl: HeaderTemplate = {
        id: newTemplateId(),
        name,
        content: portableContent as HeaderTemplateContent,
        header_height_cm: doc.layout?.header_height_cm ?? 2.5,
        created_at: new Date().toISOString(),
      };
      await commands.saveHeaderTemplate(tpl);
      await reload();
      setNewName("");
      setFeedback(`Cabeçalho "${name}" salvo na biblioteca.`);
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(false);
    }
  };

  const commitRename = async (id: string) => {
    const name = renameText.trim();
    setRenamingId(null);
    if (!name) return;
    setBusy(true);
    setError(null);
    try {
      const target = all.find((t) => t.id === id);
      if (target) {
        await commands.saveHeaderTemplate({ ...target, name });
        await reload();
      }
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(false);
    }
  };

  const remove = async (id: string) => {
    setBusy(true);
    setError(null);
    try {
      await commands.deleteHeaderTemplate(id);
      await reload();
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy(false);
    }
  };

  return createPortal(
    <div className={styles.overlay}>
      {/*
        Diálogo BLINDADO contra fechamentos acidentais: NÃO fecha em clique no
        overlay (causava "qualquer clique fecha" quando outros componentes do
        laudo — header inline, atalhos globais — vazavam eventos). Só fecha em
        X ou Esc. stopPropagation no card mantém cliques/focus dentro do
        diálogo, sem disparar handlers do laudo lá embaixo.
      */}
      <div
        className={styles.card}
        role="dialog"
        aria-modal="true"
        aria-label="Cabeçalhos Salvos"
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(e) => e.stopPropagation()}
      >
        <header className={styles.head}>
          <div className={styles.headTitle}>
            <Stamp size={16} aria-hidden />
            <h2>Cabeçalhos Salvos</h2>
          </div>
          <button
            type="button"
            className={styles.close}
            onClick={onClose}
            aria-label="Fechar"
          >
            <X size={16} />
          </button>
        </header>

        <p className={styles.note}>
          Aplique um cabeçalho ao laudo ou salve o cabeçalho atual como um novo
          modelo (ex.: outra unidade ou órgão). O modelo <strong>padrão</strong>{" "}
          é provisório até o cabeçalho oficial definitivo (com os brasões) ser
          carregado.
        </p>
        <p className={styles.note}>
          <strong>Para criar um cabeçalho do zero:</strong> clique em{" "}
          <em>Aplicar</em> no padrão abaixo → edite o cabeçalho no topo do laudo
          (clique sobre a área do cabeçalho na página) → volte aqui, dê um nome
          e clique <em>Salvar atual</em>.
        </p>

        <div className={styles.list}>
          {loadingList && (
            <p className={styles.note}>Carregando cabeçalhos…</p>
          )}
          {all.map((tpl) => {
            const builtin = isBuiltinTemplate(tpl);
            const renaming = renamingId === tpl.id;
            return (
              <div key={tpl.id} className={styles.row}>
                {renaming ? (
                  <input
                    className={styles.renameInput}
                    value={renameText}
                    autoFocus
                    onChange={(e) => setRenameText(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void commitRename(tpl.id);
                      if (e.key === "Escape") setRenamingId(null);
                    }}
                    onBlur={() => void commitRename(tpl.id)}
                  />
                ) : (
                  <span className={styles.name}>
                    {tpl.name}
                    {builtin && <span className={styles.badge}>padrão</span>}
                  </span>
                )}
                <div className={styles.rowActions}>
                  <button
                    type="button"
                    className={styles.btnPrimary}
                    disabled={busy}
                    onClick={() => void apply(tpl)}
                    title="Aplicar este cabeçalho ao laudo"
                  >
                    <Check size={13} /> Aplicar
                  </button>
                  {!builtin && (
                    <>
                      <button
                        type="button"
                        className={styles.btnIcon}
                        disabled={busy}
                        title="Renomear"
                        onClick={() => {
                          setRenamingId(tpl.id);
                          setRenameText(tpl.name);
                        }}
                      >
                        <Pencil size={13} />
                      </button>
                      <button
                        type="button"
                        className={styles.btnIconDanger}
                        disabled={busy}
                        title="Excluir cabeçalho salvo"
                        onClick={() => void remove(tpl.id)}
                      >
                        <Trash2 size={13} />
                      </button>
                    </>
                  )}
                </div>
              </div>
            );
          })}
        </div>

        <div className={styles.saveRow}>
          <input
            className={styles.saveInput}
            placeholder="Nome do novo cabeçalho (ex.: Delegacia X)"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void saveCurrentAsNew();
            }}
          />
          <button
            type="button"
            className={styles.btnPrimary}
            disabled={busy || !newName.trim()}
            onClick={() => void saveCurrentAsNew()}
            title="Salva o cabeçalho atual do laudo como um novo modelo"
          >
            <Plus size={13} /> Salvar atual
          </button>
        </div>

        {feedback && <p className={styles.feedback}>{feedback}</p>}
        {error && <p className={styles.error}>{error}</p>}
      </div>
    </div>,
    document.body,
  );
}
