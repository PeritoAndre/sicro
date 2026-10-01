/**
 * CaseDataDialog — os dados "de cadastro" do caso aberto: nome, protocolo,
 * ofício, BO, tipo de perícia, município, peritos. Tudo opcional — o Início
 * só pede o nome; o resto entra aqui quando (e se) fizer falta.
 *
 * `update_occurrence` sobrescreve todos os campos editáveis, por isso o patch
 * parte de `editFromOccurrence` (os campos que não aparecem aqui — bairro,
 * natureza, coordenadas de importação… — seguem como estão).
 */

import { useEffect, useState, type FormEvent } from "react";
import { FolderOpen } from "lucide-react";
import { Dialog } from "@components/Dialog/Dialog";
import { Button } from "@components/Button/Button";
import { pushToast } from "@/components/toast/toastStore";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { formatDateTime } from "@core/formatters";
import {
  selectActiveOccurrence,
  selectActiveWorkspacePath,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import { MUNICIPIOS_AP, TIPOS_PERICIA } from "@domain/pericia";
import { editFromOccurrence, type Occurrence } from "@domain/occurrence";
import styles from "./CaseDataDialog.module.css";

interface Props {
  open: boolean;
  onClose: () => void;
}

interface FormState {
  titulo: string;
  protocolo: string;
  oficio: string;
  numero_bo: string;
  tipo_pericia: string;
  municipio: string;
  peritos: string;
}

function fromOccurrence(o: Occurrence): FormState {
  return {
    titulo: o.titulo ?? "",
    protocolo: o.protocolo ?? "",
    oficio: o.oficio ?? "",
    numero_bo: o.numero_bo ?? "",
    tipo_pericia: o.tipo_pericia ?? "",
    municipio: o.municipio ?? "",
    peritos: o.peritos.join(", "),
  };
}

const empty: FormState = {
  titulo: "",
  protocolo: "",
  oficio: "",
  numero_bo: "",
  tipo_pericia: "",
  municipio: "",
  peritos: "",
};

export function CaseDataDialog({ open, onClose }: Props) {
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const update = useWorkspaceStore((s) => s.updateActiveOccurrence);
  const isMutating = useWorkspaceStore((s) => s.isMutating);
  const [form, setForm] = useState<FormState>(empty);
  const [error, setError] = useState<string | null>(null);

  // Cada abertura parte do caso como está gravado.
  const occurrenceId = occurrence?.id;
  useEffect(() => {
    if (!open) return;
    const o = useWorkspaceStore.getState().activeOccurrence;
    setForm(o ? fromOccurrence(o) : empty);
    setError(null);
  }, [open, occurrenceId]);

  const setField = <K extends keyof FormState>(field: K, value: FormState[K]) => {
    setForm((prev) => ({ ...prev, [field]: value }));
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!occurrence) return;
    setError(null);
    const peritos = form.peritos
      .split(",")
      .map((p) => p.trim())
      .filter(Boolean);
    try {
      await update({
        ...editFromOccurrence(occurrence),
        titulo: form.titulo.trim() || null,
        protocolo: form.protocolo.trim() || null,
        oficio: form.oficio.trim() || null,
        numero_bo: form.numero_bo.trim() || null,
        tipo_pericia: form.tipo_pericia.trim() || null,
        municipio: form.municipio.trim() || null,
        peritos,
      });
      pushToast("success", "Dados do caso salvos.");
      onClose();
    } catch (err) {
      setError(toSicroError(err).message);
    }
  };

  const reveal = () => {
    if (!workspacePath) return;
    void commands
      .revealPathInExplorer(workspacePath)
      .catch((err) => pushToast("error", toSicroError(err).message));
  };

  if (!occurrence) return null;

  return (
    <Dialog
      open={open}
      title="Dados do caso"
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={isMutating}>
            Cancelar
          </Button>
          <Button variant="primary" form="case-data-form" type="submit" disabled={isMutating}>
            {isMutating ? "Salvando…" : "Salvar"}
          </Button>
        </>
      }
    >
      <form id="case-data-form" className={styles.form} onSubmit={submit}>
        <div className={styles.field}>
          <label htmlFor="cd-titulo" className={styles.label}>
            Nome do caso
          </label>
          <input
            id="cd-titulo"
            type="text"
            className={styles.input}
            placeholder="Ex.: Laudo 63404/26, Km 09 Duca Serra"
            value={form.titulo}
            onChange={(e) => setField("titulo", e.target.value)}
            autoFocus
            spellCheck={false}
          />
          <p className={styles.hint}>
            É como o caso aparece no Início e na barra do topo. Vazio = BO, tipo e
            município (ou a data de criação).
          </p>
        </div>

        <div className={styles.row}>
          <div className={styles.field}>
            <label htmlFor="cd-protocolo" className={styles.label}>
              Protocolo (nº do laudo)
            </label>
            <input
              id="cd-protocolo"
              type="text"
              className={styles.input}
              placeholder="Ex.: 2026/000123"
              value={form.protocolo}
              onChange={(e) => setField("protocolo", e.target.value)}
            />
          </div>
          <div className={styles.field}>
            <label htmlFor="cd-oficio" className={styles.label}>
              Ofício
            </label>
            <input
              id="cd-oficio"
              type="text"
              className={styles.input}
              placeholder="Ex.: 1234/2026"
              value={form.oficio}
              onChange={(e) => setField("oficio", e.target.value)}
            />
          </div>
        </div>

        <div className={styles.row}>
          <div className={styles.field}>
            <label htmlFor="cd-bo" className={styles.label}>
              BO
            </label>
            <input
              id="cd-bo"
              type="text"
              className={styles.input}
              placeholder="12345/2026"
              value={form.numero_bo}
              onChange={(e) => setField("numero_bo", e.target.value)}
            />
          </div>
          <div className={styles.field}>
            <label htmlFor="cd-tipo" className={styles.label}>
              Tipo de perícia
            </label>
            <input
              id="cd-tipo"
              type="text"
              list="cd-tipos-pericia"
              className={styles.input}
              placeholder="Selecione ou digite…"
              value={form.tipo_pericia}
              onChange={(e) => setField("tipo_pericia", e.target.value)}
            />
            <datalist id="cd-tipos-pericia">
              {TIPOS_PERICIA.map((t) => (
                <option key={t} value={t} />
              ))}
            </datalist>
          </div>
        </div>

        <div className={styles.row}>
          <div className={styles.field}>
            <label htmlFor="cd-municipio" className={styles.label}>
              Município
            </label>
            <select
              id="cd-municipio"
              className={styles.input}
              value={form.municipio}
              onChange={(e) => setField("municipio", e.target.value)}
            >
              <option value="">—</option>
              {form.municipio && !MUNICIPIOS_AP.includes(form.municipio) && (
                <option value={form.municipio}>{form.municipio}</option>
              )}
              {MUNICIPIOS_AP.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          </div>
          <div className={styles.field}>
            <label htmlFor="cd-peritos" className={styles.label}>
              Peritos (vírgula entre nomes)
            </label>
            <input
              id="cd-peritos"
              type="text"
              className={styles.input}
              placeholder="Ex.: André Barroso, João Silva"
              value={form.peritos}
              onChange={(e) => setField("peritos", e.target.value)}
            />
          </div>
        </div>

        <div className={styles.meta}>
          <span>Criado em {formatDateTime(occurrence.created_at)}</span>
          {workspacePath && (
            <button type="button" className={styles.metaBtn} onClick={reveal} title={workspacePath}>
              <FolderOpen size={13} aria-hidden /> Abrir pasta
            </button>
          )}
        </div>

        {error && <div className={styles.error}>{error}</div>}
      </form>
    </Dialog>
  );
}
