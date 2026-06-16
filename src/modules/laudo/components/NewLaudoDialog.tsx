/**
 * NewLaudoDialog — pede título + template institucional ao criar um laudo novo
 * (SICRO 3.0).
 *
 * O laudo nasce como um `.docx` (sem editor in-app): este diálogo escolhe o
 * TÍTULO e o template institucional (cabeçalho/timbre). O store
 * (`createLaudoDocx`) monta o esqueleto inicial via `buildStarterEnvelope` e
 * renderiza o `.docx`. Quando só há um template institucional registrado, o
 * seletor visual fica escondido — não faz sentido pedir escolha entre uma
 * opção só.
 */

import { useEffect, useState, type FormEvent } from "react";
import { Dialog } from "@components/Dialog/Dialog";
import { Button } from "@components/Button/Button";
import { useLaudoStore } from "../store/laudoStore";
import {
  INSTITUTIONAL_TEMPLATES,
  findInstitutionalTemplate,
  type OccurrenceContext,
} from "../document-engine";
import { toSicroError } from "@core/errors";
import type { Laudo } from "@domain/laudo";
import styles from "./NewLaudoDialog.module.css";

const DEFAULT_TEMPLATE_ID = INSTITUTIONAL_TEMPLATES[0]!.id;

interface NewLaudoDialogProps {
  open: boolean;
  workspacePath: string;
  suggestedTitle: string;
  /** Mantido por compatibilidade com o caller; não usado no fluxo `.docx`
   *  (a ocorrência vem do workspaceStore dentro do `createLaudoDocx`). */
  occurrence: OccurrenceContext | null;
  onClose: () => void;
  onCreated: (laudo: Laudo) => void;
}

export function NewLaudoDialog({
  open,
  suggestedTitle,
  onClose,
  onCreated,
}: NewLaudoDialogProps) {
  const createLaudoDocx = useLaudoStore((s) => s.createLaudoDocx);
  const isMutating = useLaudoStore((s) => s.isMutating);

  const [title, setTitle] = useState(suggestedTitle);
  const [templateId, setTemplateId] = useState<string>(DEFAULT_TEMPLATE_ID);
  const [numeroLaudo, setNumeroLaudo] = useState("");
  const [error, setError] = useState<string | null>(null);

  // Quando só há um template institucional registrado, o seletor visual fica
  // escondido — não faz sentido pedir escolha entre uma opção só.
  const showTemplatePicker = INSTITUTIONAL_TEMPLATES.length > 1;

  const close = () => {
    setError(null);
    setTitle(suggestedTitle);
    setTemplateId(DEFAULT_TEMPLATE_ID);
    setNumeroLaudo("");
    onClose();
  };

  // Re-semeia os campos sempre que o diálogo ABRE — não a cada render. O reset
  // anterior era condicionado a `title === ""`, então reinjetava o título toda
  // vez que o campo ficava vazio (impossível apagar o nome com Backspace). Agora
  // o reseed só ocorre na transição fechado→aberto.
  useEffect(() => {
    if (open) {
      setTitle(suggestedTitle);
      setTemplateId(DEFAULT_TEMPLATE_ID);
      setNumeroLaudo("");
      setError(null);
    }
    // Intencional: dependemos só de `open`. Incluir `suggestedTitle` reseedaria
    // no meio da digitação caso o pai recomputasse a sugestão.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);

    const trimmed = title.trim() || "Laudo sem título";
    const numero = numeroLaudo.trim();
    try {
      const laudo = await createLaudoDocx(trimmed, templateId, {
        numeroLaudo: numero || undefined,
      });
      onCreated(laudo);
      close();
    } catch (err) {
      setError(toSicroError(err).message);
    }
  };

  return (
    <Dialog
      open={open}
      title="Novo laudo"
      onClose={close}
      footer={
        <>
          <Button variant="ghost" onClick={close} disabled={isMutating}>
            Cancelar
          </Button>
          <Button
            variant="primary"
            form="new-laudo-form"
            type="submit"
            disabled={isMutating}
          >
            {isMutating ? "Criando…" : "Criar laudo"}
          </Button>
        </>
      }
    >
      <form id="new-laudo-form" className={styles.form} onSubmit={submit}>
        <div className={styles.field}>
          <label htmlFor="laudo-title" className={styles.label}>
            Título do laudo
          </label>
          <input
            id="laudo-title"
            type="text"
            className={styles.input}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            autoFocus
          />
        </div>

        <div className={styles.field}>
          <label htmlFor="laudo-numero" className={styles.label}>
            Número do laudo (opcional)
          </label>
          <input
            id="laudo-numero"
            type="text"
            className={styles.input}
            value={numeroLaudo}
            onChange={(e) => setNumeroLaudo(e.target.value)}
            placeholder="ex.: 12345/2026"
          />
          <span className={styles.hint}>
            Em branco, o número vem da ocorrência (quando houver).
          </span>
        </div>

        {showTemplatePicker ? (
          <div className={styles.field}>
            <span className={styles.label}>Modelo institucional</span>
            <div className={styles.templateList} role="radiogroup">
              {INSTITUTIONAL_TEMPLATES.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  role="radio"
                  aria-checked={templateId === t.id}
                  className={`${styles.templateOption} ${
                    templateId === t.id ? styles.templateOptionActive : ""
                  }`}
                  onClick={() => setTemplateId(t.id)}
                >
                  <span className={styles.templateName}>{t.name}</span>
                </button>
              ))}
            </div>
          </div>
        ) : (
          // Único modelo: só mostra o nome como informação.
          <div className={styles.field}>
            <span className={styles.label}>Modelo institucional</span>
            <div className={styles.singleTemplateInfo}>
              <strong>{findInstitutionalTemplate(DEFAULT_TEMPLATE_ID).name}</strong>
            </div>
          </div>
        )}

        {error && <div className={styles.error}>{error}</div>}
      </form>
    </Dialog>
  );
}
