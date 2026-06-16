/**
 * NewLaudoDialog — pede título do laudo (e número opcional) ao criar.
 *
 * Hoje só há um template ("Documento em branco" + cabeçalho oficial
 * automático), então o seletor de templates só aparece quando há
 * mais de um cadastrado em `TEMPLATES`. Para o caso comum, o usuário
 * só preenche título e clica criar.
 *
 * Cabeçalho oficial:
 *   - `layout.institutional_template = "pca_padrao_v1"` é gravado
 *     junto com o doc inicial.
 *   - `header.enabled = true` e `header.content` é semeado a partir
 *     do template institucional (brand lines, subtitle, metadata
 *     em uma linha só).
 *   Isso garante que ao abrir o laudo, o usuário JÁ VÊ o cabeçalho
 *   oficial sem precisar configurar nada manualmente.
 */

import { useEffect, useState, type FormEvent } from "react";
import { Dialog } from "@components/Dialog/Dialog";
import { Button } from "@components/Button/Button";
import { useLaudoStore } from "../store/laudoStore";
import {
  TEMPLATES,
  findTemplate,
  emptyHeaderContent,
  type OccurrenceContext,
  type SicroDocHeader,
} from "../document-engine";
import { toSicroError } from "@core/errors";
import type { Laudo } from "@domain/laudo";
import styles from "./NewLaudoDialog.module.css";

const DEFAULT_TEMPLATE_ID = "documento_em_branco";

interface NewLaudoDialogProps {
  open: boolean;
  workspacePath: string;
  suggestedTitle: string;
  occurrence: OccurrenceContext | null;
  onClose: () => void;
  onCreated: (laudo: Laudo) => void;
}

export function NewLaudoDialog({
  open,
  workspacePath,
  suggestedTitle,
  occurrence,
  onClose,
  onCreated,
}: NewLaudoDialogProps) {
  const createLaudo = useLaudoStore((s) => s.createLaudo);
  const isMutating = useLaudoStore((s) => s.isMutating);

  const [title, setTitle] = useState(suggestedTitle);
  const [templateId, setTemplateId] = useState<string>(DEFAULT_TEMPLATE_ID);
  const [numeroLaudo, setNumeroLaudo] = useState("");
  const [error, setError] = useState<string | null>(null);

  // Quando só há um template registrado, o seletor visual fica
  // escondido — não faz sentido pedir escolha entre uma opção só.
  const showTemplatePicker = TEMPLATES.length > 1;

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
    const template = findTemplate(templateId);
    const initialContent = template.build(trimmed, occurrence);
    const trimmedNumero = numeroLaudo.trim();
    const initialMetadata = trimmedNumero
      ? { numero_laudo: trimmedNumero }
      : undefined;

    // Documento em branco (como no Word): cabeçalho DESLIGADO e vazio.
    // Sem timbre, sem brand lines, sem rodapé institucional. O template
    // institucional default (`em_branco`) já zera margens/rodapé; não
    // gravamos `institutional_template` no doc (o fallback resolve pra
    // `BLANK_V1`). O timbre será reconstruído depois, nos modelos.
    const initialHeader: SicroDocHeader = {
      enabled: false,
      content: emptyHeaderContent(),
    };

    try {
      const laudo = await createLaudo(
        workspacePath,
        { title: trimmed, template_id: templateId },
        initialContent,
        initialMetadata,
        { header: initialHeader },
      );
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
            placeholder="Ex.: 12345/2026"
          />
        </div>

        {showTemplatePicker ? (
          <div className={styles.field}>
            <span className={styles.label}>Modelo</span>
            <div className={styles.templateList} role="radiogroup">
              {TEMPLATES.map((t) => (
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
                  <span className={styles.templateDescription}>
                    {t.description}
                  </span>
                  <span className={styles.templateCategory}>{t.category}</span>
                </button>
              ))}
            </div>
          </div>
        ) : (
          // Único modelo: só mostra a descrição como informação.
          <div className={styles.field}>
            <span className={styles.label}>Modelo</span>
            <div className={styles.singleTemplateInfo}>
              <strong>{findTemplate(DEFAULT_TEMPLATE_ID).name}</strong>
              <span>{findTemplate(DEFAULT_TEMPLATE_ID).description}</span>
            </div>
          </div>
        )}

        {error && <div className={styles.error}>{error}</div>}
      </form>
    </Dialog>
  );
}
