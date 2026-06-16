/**
 * CaseHeader — identificação do caso (cabeçalho do Dossiê), SEMPRE editável.
 *
 * Casos de expediente (áudio/vídeo) nascem no Desktop, sem coleta de campo — o
 * perito preenche e corrige tudo aqui, e o que ficar gravado é a PALAVRA FINAL.
 * §13: isto é metadado do caso, não a prova; as provas continuam imutáveis na
 * Central de Provas e o pacote .sicroapp original nunca é alterado.
 */
import { useEffect, useState } from "react";
import { Check, Pencil, X } from "lucide-react";

import { Button } from "@components/Button/Button";
import { toSicroError } from "@core/errors";
import { useShortcuts } from "@core/useShortcuts";
import { useWorkspaceStore } from "@stores/workspaceStore";
import type {
  Occurrence,
  OccurrenceEdit,
  OccurrenceStatus,
} from "@domain/occurrence";

import styles from "./DossieModule.module.css";

const STATUS: { value: OccurrenceStatus; label: string }[] = [
  { value: "aberta", label: "Aberta" },
  { value: "em_andamento", label: "Em andamento" },
  { value: "concluida", label: "Concluída" },
  { value: "arquivada", label: "Arquivada" },
];

interface FormState {
  numero_bo: string;
  requisicao: string;
  protocolo: string;
  delegacia: string;
  tipo_pericia: string;
  natureza: string;
  resultado: string;
  status: OccurrenceStatus;
  municipio: string;
  bairro: string;
  logradouro: string;
  referencia: string;
  latitude: string;
  longitude: string;
}

function toForm(o: Occurrence): FormState {
  return {
    numero_bo: o.numero_bo ?? "",
    requisicao: o.requisicao ?? o.oficio ?? "",
    protocolo: o.protocolo ?? "",
    delegacia: o.delegacia ?? "",
    tipo_pericia: o.tipo_pericia ?? "",
    natureza: o.natureza ?? "",
    resultado: o.resultado ?? "",
    status: o.status,
    municipio: o.municipio ?? "",
    bairro: o.bairro ?? "",
    logradouro: o.logradouro ?? "",
    referencia: o.referencia ?? "",
    latitude: o.latitude != null ? String(o.latitude) : "",
    longitude: o.longitude != null ? String(o.longitude) : "",
  };
}

function parseNum(s: string): number | null {
  const t = s.trim().replace(",", ".");
  if (!t) return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : null;
}

function toEdit(f: FormState, peritos: string[]): OccurrenceEdit {
  return {
    numero_bo: f.numero_bo,
    requisicao: f.requisicao,
    protocolo: f.protocolo,
    delegacia: f.delegacia,
    tipo_pericia: f.tipo_pericia,
    natureza: f.natureza,
    resultado: f.resultado,
    status: f.status,
    municipio: f.municipio,
    bairro: f.bairro,
    logradouro: f.logradouro,
    referencia: f.referencia,
    latitude: parseNum(f.latitude),
    longitude: parseNum(f.longitude),
    peritos,
  };
}

/** Campos de texto da identificação (ordem de exibição). */
const IDENT: { key: keyof FormState; label: string; placeholder?: string }[] = [
  { key: "numero_bo", label: "BO" },
  { key: "requisicao", label: "Requisição / Ofício" },
  { key: "protocolo", label: "Protocolo" },
  { key: "delegacia", label: "Unidade policial" },
  {
    key: "tipo_pericia",
    label: "Tipo de perícia",
    placeholder: "Ex.: Sinistro de Trânsito",
  },
  { key: "natureza", label: "Natureza" },
  { key: "resultado", label: "Resultado" },
];
const LOCAL: { key: keyof FormState; label: string }[] = [
  { key: "municipio", label: "Município" },
  { key: "bairro", label: "Bairro" },
  { key: "logradouro", label: "Logradouro" },
  { key: "referencia", label: "Referência" },
];

export function CaseHeader({ occurrence }: { occurrence: Occurrence }) {
  const update = useWorkspaceStore((s) => s.updateActiveOccurrence);
  const [editing, setEditing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [form, setForm] = useState<FormState>(() => toForm(occurrence));

  // Atalho: entrar no modo de edição (Alt+E por padrão; customizável).
  useShortcuts({ "dossie.editIdentificacao": () => setEditing(true) });

  // Re-semeia ao trocar de caso (mas não enquanto o perito edita).
  useEffect(() => {
    if (!editing) setForm(toForm(occurrence));
  }, [occurrence, editing]);

  const setField = (k: keyof FormState, v: string) =>
    setForm((f) => ({ ...f, [k]: v }));

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      await update(toEdit(form, occurrence.peritos));
      setEditing(false);
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setSaving(false);
    }
  };

  const cancel = () => {
    setForm(toForm(occurrence));
    setError(null);
    setEditing(false);
  };

  const coord =
    occurrence.latitude != null && occurrence.longitude != null
      ? `${occurrence.latitude.toFixed(6)}, ${occurrence.longitude.toFixed(6)}`
      : null;

  return (
    <section className={styles.caseHeader}>
      <div className={styles.chHead}>
        <span className={styles.chTitle}>Identificação do caso</span>
        {editing ? (
          <div className={styles.chActions}>
            <Button
              size="sm"
              variant="ghost"
              leftIcon={<X size={13} />}
              onClick={cancel}
              disabled={saving}
            >
              Cancelar
            </Button>
            <Button
              size="sm"
              variant="primary"
              leftIcon={<Check size={13} />}
              onClick={() => void save()}
              disabled={saving}
            >
              {saving ? "Salvando…" : "Salvar"}
            </Button>
          </div>
        ) : (
          <Button
            size="sm"
            variant="ghost"
            leftIcon={<Pencil size={13} />}
            onClick={() => setEditing(true)}
          >
            Editar
          </Button>
        )}
      </div>

      {error && <div className={styles.errorBanner}>{error}</div>}

      {editing ? (
        <div className={styles.chGrid}>
          {IDENT.map((fld) => (
            <label key={fld.key} className={styles.chField}>
              <span className={styles.chLabel}>{fld.label}</span>
              <input
                className={styles.chInput}
                value={form[fld.key] as string}
                placeholder={fld.placeholder ?? "—"}
                onChange={(e) => setField(fld.key, e.target.value)}
              />
            </label>
          ))}
          <label className={styles.chField}>
            <span className={styles.chLabel}>Status</span>
            <select
              className={styles.chInput}
              value={form.status}
              onChange={(e) => setField("status", e.target.value)}
            >
              {STATUS.map((s) => (
                <option key={s.value} value={s.value}>
                  {s.label}
                </option>
              ))}
            </select>
          </label>
          {LOCAL.map((fld) => (
            <label key={fld.key} className={styles.chField}>
              <span className={styles.chLabel}>{fld.label}</span>
              <input
                className={styles.chInput}
                value={form[fld.key] as string}
                placeholder="—"
                onChange={(e) => setField(fld.key, e.target.value)}
              />
            </label>
          ))}
          <label className={styles.chField}>
            <span className={styles.chLabel}>Latitude</span>
            <input
              className={styles.chInput}
              value={form.latitude}
              inputMode="decimal"
              placeholder="—"
              onChange={(e) => setField("latitude", e.target.value)}
            />
          </label>
          <label className={styles.chField}>
            <span className={styles.chLabel}>Longitude</span>
            <input
              className={styles.chInput}
              value={form.longitude}
              inputMode="decimal"
              placeholder="—"
              onChange={(e) => setField("longitude", e.target.value)}
            />
          </label>
        </div>
      ) : (
        <div className={styles.chGrid}>
          <ViewField label="BO" value={occurrence.numero_bo} />
          <ViewField
            label="Requisição / Ofício"
            value={occurrence.requisicao ?? occurrence.oficio}
          />
          <ViewField label="Protocolo" value={occurrence.protocolo} />
          <ViewField label="Unidade policial" value={occurrence.delegacia} />
          <ViewField label="Tipo de perícia" value={occurrence.tipo_pericia} />
          <ViewField label="Natureza" value={occurrence.natureza} />
          <ViewField label="Resultado" value={occurrence.resultado ?? null} />
          <ViewField
            label="Status"
            value={
              STATUS.find((s) => s.value === occurrence.status)?.label ??
              occurrence.status
            }
          />
          <ViewField label="Município" value={occurrence.municipio} />
          <ViewField label="Bairro" value={occurrence.bairro} />
          <ViewField label="Logradouro" value={occurrence.logradouro} />
          <ViewField label="Referência" value={occurrence.referencia} />
          <ViewField label="Coordenada" value={coord} />
        </div>
      )}
    </section>
  );
}

function ViewField({
  label,
  value,
}: {
  label: string;
  value: string | null | undefined;
}) {
  return (
    <div className={styles.chField}>
      <span className={styles.chLabel}>{label}</span>
      <span className={styles.chValue}>
        {value ? value : <span className={styles.chDim}>—</span>}
      </span>
    </div>
  );
}
