import { useEffect, useState, type FormEvent } from "react";
import { open as openDirDialog } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, Folder } from "lucide-react";
import { Dialog } from "@components/Dialog/Dialog";
import { Button } from "@components/Button/Button";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { useSettingsStore } from "@stores/settingsStore";
import { toSicroError } from "@core/errors";
import { detectSyncedFolder } from "@core/syncedFolder";
import { MUNICIPIOS_AP, TIPOS_PERICIA } from "@domain/pericia";
import type { NewOccurrenceInput, LoadedOccurrence } from "@domain/occurrence";
import styles from "./NewOccurrenceDialog.module.css";

interface NewOccurrenceDialogProps {
  open: boolean;
  onClose: () => void;
  onCreated: (loaded: LoadedOccurrence) => void;
}

interface FormState {
  numero_bo: string;
  protocolo: string;
  oficio: string;
  tipo_pericia: string;
  municipio: string;
  peritos: string;
  parent_directory: string;
}

const empty: FormState = {
  numero_bo: "",
  protocolo: "",
  oficio: "",
  tipo_pericia: "",
  municipio: "",
  peritos: "",
  parent_directory: "",
};

export function NewOccurrenceDialog({
  open,
  onClose,
  onCreated,
}: NewOccurrenceDialogProps) {
  const create = useWorkspaceStore((s) => s.createOccurrence);
  const isMutating = useWorkspaceStore((s) => s.isMutating);
  const municipioAtuacao = useSettingsStore(
    (s) => s.settings.profile.municipio_atuacao,
  );
  const [form, setForm] = useState<FormState>(empty);
  const [error, setError] = useState<string | null>(null);
  const syncedService = detectSyncedFolder(form.parent_directory);

  // Ao abrir, pré-preenche o Município com o município de atuação do perfil
  // (Configurações → Perfil do perito). Editável; só preenche se estiver vazio.
  useEffect(() => {
    if (open && municipioAtuacao) {
      setForm((prev) =>
        prev.municipio ? prev : { ...prev, municipio: municipioAtuacao },
      );
    }
  }, [open, municipioAtuacao]);

  const close = () => {
    setForm(empty);
    setError(null);
    onClose();
  };

  const setField = <K extends keyof FormState>(field: K, value: FormState[K]) => {
    setForm((prev) => ({ ...prev, [field]: value }));
  };

  const pickDirectory = async () => {
    try {
      const selected = await openDirDialog({
        directory: true,
        multiple: false,
        title: "Selecione a pasta onde o workspace .sicro será criado",
      });
      if (typeof selected === "string") {
        setField("parent_directory", selected);
      }
    } catch (err) {
      setError(toSicroError(err).message);
    }
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);

    const peritos = form.peritos
      .split(",")
      .map((p) => p.trim())
      .filter(Boolean);

    const input: NewOccurrenceInput = {
      numero_bo: form.numero_bo.trim() || null,
      protocolo: form.protocolo.trim() || null,
      oficio: form.oficio.trim() || null,
      tipo_pericia: form.tipo_pericia.trim() || null,
      municipio: form.municipio.trim() || null,
      peritos,
      parent_directory: form.parent_directory.trim() || null,
    };

    try {
      const loaded = await create(input);
      onCreated(loaded);
      close();
    } catch (err) {
      setError(toSicroError(err).message);
    }
  };

  return (
    <Dialog
      open={open}
      title="Nova ocorrência"
      onClose={close}
      footer={
        <>
          <Button variant="ghost" onClick={close} disabled={isMutating}>
            Cancelar
          </Button>
          <Button
            variant="primary"
            form="new-occurrence-form"
            type="submit"
            disabled={isMutating}
          >
            {isMutating ? "Criando…" : "Criar ocorrência"}
          </Button>
        </>
      }
    >
      <form id="new-occurrence-form" className={styles.form} onSubmit={submit}>
        {/* PROTOCOLO — o "coração" do laudo: o nº do laudo é o nº que o ofício
            recebe no protocolo. Em destaque, no topo. */}
        <div
          className={styles.field}
          style={{
            padding: "var(--space-3)",
            border: "1px solid var(--sicro-accent)",
            borderRadius: "var(--radius-md)",
            background: "var(--sicro-accent-soft)",
          }}
        >
          <label htmlFor="protocolo" className={styles.label}>
            Número do Protocolo <strong>(nº do laudo)</strong>
          </label>
          <input
            id="protocolo"
            type="text"
            className={styles.input}
            placeholder="Ex.: 2026/000123"
            value={form.protocolo}
            onChange={(e) => setField("protocolo", e.target.value)}
            autoFocus
          />
          <p className={styles.hint}>
            Gerado pela Polícia Científica no registro — é o identificador do laudo.
          </p>
        </div>

        <div className={styles.field}>
          <label htmlFor="oficio" className={styles.label}>
            Número do Ofício{" "}
            <span style={{ opacity: 0.6, fontWeight: 400 }}>(opcional)</span>
          </label>
          <input
            id="oficio"
            type="text"
            className={styles.input}
            placeholder="Ex.: 1234/2026"
            value={form.oficio}
            onChange={(e) => setField("oficio", e.target.value)}
          />
          <p className={styles.hint}>
            Número que a Polícia Civil deu ao ofício de requisição (origem externa) —
            diferente do protocolo.
          </p>
        </div>

        <div className={styles.row}>
          <div className={styles.field}>
            <label htmlFor="tipo_pericia" className={styles.label}>
              Tipo de perícia
            </label>
            <input
              id="tipo_pericia"
              type="text"
              list="tipos-pericia"
              className={styles.input}
              placeholder="Selecione ou digite…"
              value={form.tipo_pericia}
              onChange={(e) => setField("tipo_pericia", e.target.value)}
            />
            <datalist id="tipos-pericia">
              {TIPOS_PERICIA.map((t) => (
                <option key={t} value={t} />
              ))}
            </datalist>
          </div>
          <div className={styles.field}>
            <label htmlFor="municipio" className={styles.label}>
              Município
            </label>
            <select
              id="municipio"
              className={styles.input}
              value={form.municipio}
              onChange={(e) => setField("municipio", e.target.value)}
            >
              <option value="">Selecione…</option>
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
        </div>

        <div className={styles.field}>
          <label htmlFor="numero_bo" className={styles.label}>
            Número do BO{" "}
            <span style={{ opacity: 0.6, fontWeight: 400 }}>(opcional)</span>
          </label>
          <input
            id="numero_bo"
            type="text"
            className={styles.input}
            placeholder="12345/2026"
            value={form.numero_bo}
            onChange={(e) => setField("numero_bo", e.target.value)}
          />
        </div>

        <div className={styles.field}>
          <label htmlFor="peritos" className={styles.label}>
            Peritos (separados por vírgula)
          </label>
          <input
            id="peritos"
            type="text"
            className={styles.input}
            placeholder="Ex.: André Barroso, João Silva"
            value={form.peritos}
            onChange={(e) => setField("peritos", e.target.value)}
          />
        </div>

        <div className={styles.field}>
          <label htmlFor="parent_directory" className={styles.label}>
            Pasta onde o workspace será criado
          </label>
          <div className={styles.parentDirRow}>
            <input
              id="parent_directory"
              type="text"
              className={styles.input}
              placeholder="(opcional — padrão: pasta local SICRO\Casos)"
              value={form.parent_directory}
              onChange={(e) => setField("parent_directory", e.target.value)}
            />
            <Button
              variant="secondary"
              onClick={pickDirectory}
              leftIcon={<Folder size={14} />}
              type="button"
            >
              Escolher
            </Button>
          </div>
          {syncedService && (
            <div
              style={{
                marginTop: "var(--space-2)",
                padding: "var(--space-2) var(--space-3)",
                fontSize: "var(--text-xs)",
                lineHeight: 1.5,
                color: "var(--sicro-warning)",
                background: "rgba(224, 163, 58, 0.1)",
                border: "1px solid rgba(224, 163, 58, 0.35)",
                borderRadius: "var(--radius-md)",
                display: "flex",
                gap: 8,
                alignItems: "flex-start",
              }}
            >
              <AlertTriangle
                size={14}
                style={{ flexShrink: 0, marginTop: 1 }}
                aria-hidden
              />
              <span>
                Esta pasta parece estar no <strong>{syncedService}</strong>.
                Guardar o caso <em>vivo</em> numa pasta sincronizada é arriscado —
                o sync pode corromper o banco. Prefira uma pasta{" "}
                <strong>local</strong> e use o backup pra nuvem (o{" "}
                <code>.sicrobackup</code> sincroniza sem risco).
              </span>
            </div>
          )}
          <p className={styles.hint}>
            Será criada uma pasta <code>.sicro</code> dentro deste diretório
            (com <code>manifest.json</code>, <code>sicro.sqlite</code> e assets).
            Vazio = pasta <strong>local</strong> padrão do SICRO (recomendado); a
            redundância em nuvem é feita por backup, não sincronizando o banco.
          </p>
        </div>

        {error && <div className={styles.error}>{error}</div>}
      </form>
    </Dialog>
  );
}
