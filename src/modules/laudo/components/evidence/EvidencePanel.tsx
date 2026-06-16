/**
 * EvidencePanel — "Inserir Evidência" tab of the laudo Inspector (MVP 4).
 *
 * Six sub-tabs that mirror the data sources of the workspace:
 *   - Dados     → campos da ocorrência → insertSystemData
 *   - Fotos     → MediaAsset[]         → insertFigure(kind:"image")
 *   - Croquis   → Croqui[]             → insertFigure(kind:"croqui")
 *   - Vídeo     → VideoMedia[]         → insertFigure(kind:"video_frame") | insertStoryboardFromVideo
 *   - Dossiê    → FieldNote[]          → insertContent (paragraphs)
 *   - Tabelas   → checklist/vestígios/medições → insertEvidenceTable
 *
 * Cada inserção:
 *   1. Constrói os atributos de procedência;
 *   2. Despacha o command TipTap correspondente no editor;
 *   3. Chama `commands.recordEvidenceLink(...)` para gravar uma linha em
 *      `evidence_links` (índice / audit log paralelo aos atributos do node).
 *
 * Sem AI, sem interpretação, sem cálculo automático — só inserção
 * estruturada e rastreável.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import type { Editor } from "@tiptap/react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  ClipboardList,
  FileImage,
  FileSpreadsheet,
  Film,
  Fingerprint,
  Image as ImageIcon,
  ImageOff,
  Layers,
  ListTodo,
  Map as MapIcon,
  NotebookText,
  Ruler,
  StickyNote,
  Type,
  X,
} from "lucide-react";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type { MediaAsset } from "@domain/import";
import type { Croqui } from "@domain/croqui";
import type {
  VideoBundle,
  VideoMedia,
  VideoStoryboardFrame,
} from "@domain/video";
import type {
  ChecklistItem,
  FieldNote,
  Measurement,
  Trace,
} from "@domain/dossie";
import type { Occurrence } from "@domain/occurrence";
import type {
  EvidenceSourceKind,
  RecordEvidenceLinkInput,
} from "@domain/evidence";
import { formatDateTime } from "@core/formatters";
import styles from "./EvidencePanel.module.css";

interface EvidencePanelProps {
  editor: Editor | null;
  workspacePath: string | null;
  laudoId: string | null;
  /** Ocorrência ativa — fonte para a sub-aba "Dados". */
  occurrence: Occurrence | null;
}

type SubTab =
  | "dados"
  | "fotos"
  | "croquis"
  | "video"
  | "dossie"
  | "tabelas"
  | "indicios";

export function EvidencePanel({
  editor,
  workspacePath,
  laudoId,
  occurrence,
}: EvidencePanelProps) {
  const [tab, setTab] = useState<SubTab>("fotos");

  if (!editor || !workspacePath || !laudoId) {
    return (
      <p className={styles.empty}>
        Abra um laudo para inserir evidências.
      </p>
    );
  }

  const ctx: InsertionContext = { editor, workspacePath, laudoId };

  return (
    <div className={styles.root}>
      <div className={styles.subTabs} role="tablist">
        <SubTabBtn
          icon={<Type size={11} />}
          label="Dados"
          active={tab === "dados"}
          onClick={() => setTab("dados")}
        />
        <SubTabBtn
          icon={<ImageIcon size={11} />}
          label="Fotos"
          active={tab === "fotos"}
          onClick={() => setTab("fotos")}
        />
        <SubTabBtn
          icon={<MapIcon size={11} />}
          label="Croquis"
          active={tab === "croquis"}
          onClick={() => setTab("croquis")}
        />
        <SubTabBtn
          icon={<Film size={11} />}
          label="Vídeo"
          active={tab === "video"}
          onClick={() => setTab("video")}
        />
        <SubTabBtn
          icon={<StickyNote size={11} />}
          label="Dossiê"
          active={tab === "dossie"}
          onClick={() => setTab("dossie")}
        />
        <SubTabBtn
          icon={<FileSpreadsheet size={11} />}
          label="Tabelas"
          active={tab === "tabelas"}
          onClick={() => setTab("tabelas")}
        />
        <SubTabBtn
          icon={<Fingerprint size={11} />}
          label="Indícios"
          active={tab === "indicios"}
          onClick={() => setTab("indicios")}
        />
      </div>

      {tab === "dados" && <DadosSub ctx={ctx} occurrence={occurrence} />}
      {tab === "fotos" && <FotosSub ctx={ctx} />}
      {tab === "croquis" && <CroquisSub ctx={ctx} />}
      {tab === "video" && <VideoSub ctx={ctx} />}
      {tab === "dossie" && <DossieSub ctx={ctx} />}
      {tab === "tabelas" && <TabelasSub ctx={ctx} />}
      {tab === "indicios" && <IndiciosSub ctx={ctx} />}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-tab 7: Indícios (heatmaps de documentoscopia → figure[kind=image])

function IndiciosSub({ ctx }: { ctx: InsertionContext }) {
  const [items, setItems] = useState<
    { relative_path: string; file_name: string; created_at: string }[]
  >([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [feedback, isError, fire] = useFeedback();

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    commands
      .listDocIndicios(ctx.workspacePath)
      .then((data) => {
        if (!cancelled) setItems(data);
      })
      .catch((err) => {
        if (!cancelled) setLoadError(toSicroError(err).message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [ctx.workspacePath]);

  const insert = async (it: { relative_path: string; file_name: string; created_at: string }) => {
    const src = assetSrc(ctx.workspacePath, it.relative_path) ?? "";
    const caption =
      "Indício documentoscópico (mapa indicativo — requer exame humano; não conclui adulteração).";
    const ok = ctx.editor
      .chain()
      .focus()
      .insertFigure({
        src,
        kind: "image",
        alt: it.file_name,
        caption,
        evidence_id: it.relative_path,
        evidence_kind: "photo",
        relative_path: it.relative_path,
        source_hash: "",
        metadata_json: JSON.stringify({
          tipo: "indicio_documentoscopia",
          file: it.file_name,
          created_at: it.created_at,
        }),
      })
      .run();
    if (!ok) {
      fire("Falha ao inserir o indício.", true);
      return;
    }
    await recordLink(ctx.workspacePath, ctx.laudoId, {
      source_kind: "photo",
      relative_path: it.relative_path,
      metadata_json: JSON.stringify({ tipo: "indicio_documentoscopia", file: it.file_name }),
    });
    fire("Indício inserido como figura.");
  };

  if (loading) return <p className={styles.empty}>Carregando indícios…</p>;
  if (loadError)
    return <p className={`${styles.feedback} ${styles.feedbackError}`}>{loadError}</p>;
  if (items.length === 0) {
    return (
      <p className={styles.empty}>
        Nenhum indício salvo. Gere um na Documentoscopia (aba Indícios → Enviar ao
        laudo).
      </p>
    );
  }

  return (
    <>
      <p className={styles.hint}>
        Mapas indicativos (ELA, ruído, copy-move) — inserir como figura numerada.
        São <strong>indícios, não conclusões</strong> (§13).
      </p>
      <div className={styles.list}>
        {items.map((it) => (
          <button
            key={it.relative_path}
            type="button"
            onClick={() => void insert(it)}
            title="Inserir como figura no laudo"
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              width: "100%",
              padding: 6,
              border: "1px solid var(--sicro-border)",
              borderRadius: "var(--radius-md)",
              background: "var(--sicro-surface-1)",
              color: "var(--sicro-fg)",
              cursor: "pointer",
              textAlign: "left",
            }}
          >
            <img
              src={assetSrc(ctx.workspacePath, it.relative_path) ?? ""}
              alt=""
              style={{
                width: 44,
                height: 44,
                objectFit: "cover",
                borderRadius: 4,
                flexShrink: 0,
                background: "var(--sicro-surface-2)",
              }}
            />
            <span style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>
              <span
                style={{
                  fontSize: "var(--text-xs)",
                  fontWeight: 600,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {it.file_name}
              </span>
              <span style={{ fontSize: 10, color: "var(--sicro-fg-faint)" }}>
                {it.created_at ? formatDateTime(it.created_at) : ""}
              </span>
            </span>
          </button>
        ))}
      </div>
      {feedback && (
        <p className={`${styles.feedback} ${isError ? styles.feedbackError : ""}`}>
          {feedback}
        </p>
      )}
    </>
  );
}

// ---------------------------------------------------------------------------
// Helpers / context

interface InsertionContext {
  editor: Editor;
  workspacePath: string;
  laudoId: string;
}

function SubTabBtn({
  icon,
  label,
  active,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      className={`${styles.subTab} ${active ? styles.subTabActive : ""}`}
      onClick={onClick}
    >
      <span style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
        {icon} {label}
      </span>
    </button>
  );
}

function assetSrc(workspacePath: string, relativePath: string): string | null {
  const sep = workspacePath.includes("\\") ? "\\" : "/";
  const abs = `${workspacePath}${sep}${relativePath.replace(/\//g, sep)}`;
  try {
    return convertFileSrc(abs);
  } catch {
    return null;
  }
}

/** Best-effort recordEvidenceLink: never throws to the UI, only logs. */
async function recordLink(
  workspacePath: string,
  laudoId: string,
  partial: Omit<RecordEvidenceLinkInput, "target_type" | "target_id">,
): Promise<void> {
  try {
    await commands.recordEvidenceLink(workspacePath, {
      target_type: "laudo",
      target_id: laudoId,
      ...partial,
    });
  } catch (err) {
    // eslint-disable-next-line no-console
    console.warn(
      `[laudo] recordEvidenceLink failed for ${partial.source_kind}:`,
      toSicroError(err).message,
    );
  }
}

function useFeedback(): [string | null, boolean, (msg: string, isError?: boolean) => void] {
  const [text, setText] = useState<string | null>(null);
  const [isError, setIsError] = useState(false);
  const fire = useCallback((msg: string, asError = false) => {
    setText(msg);
    setIsError(asError);
    if (!asError) {
      setTimeout(() => setText(null), 2400);
    }
  }, []);
  return [text, isError, fire];
}

function Feedback({ text, isError }: { text: string | null; isError: boolean }) {
  if (!text) return null;
  return (
    <p className={`${styles.feedback} ${isError ? styles.feedbackError : ""}`}>
      {text}
    </p>
  );
}

// ---------------------------------------------------------------------------
// Sub-tab 1: Dados (Occurrence fields → systemData)

interface OccurrenceField {
  key: string;
  label: string;
  value: string;
}

function buildOccurrenceFields(occ: Occurrence | null): OccurrenceField[] {
  if (!occ) return [];
  const fields: { key: keyof Occurrence; label: string }[] = [
    { key: "numero_bo", label: "BO nº" },
    { key: "protocolo", label: "Protocolo" },
    { key: "requisicao", label: "Requisição" },
    { key: "oficio", label: "Ofício" },
    { key: "tipo_pericia", label: "Tipo de perícia" },
    { key: "natureza", label: "Natureza" },
    { key: "municipio", label: "Município" },
    { key: "bairro", label: "Bairro" },
    { key: "logradouro", label: "Logradouro" },
    { key: "referencia", label: "Referência" },
    { key: "data_fato", label: "Data do fato" },
    { key: "data_acionamento", label: "Acionamento" },
    { key: "data_chegada", label: "Chegada ao local" },
  ];
  const out: OccurrenceField[] = [];
  for (const f of fields) {
    const v = occ[f.key];
    if (typeof v === "string" && v.trim().length > 0) {
      const display = f.key.startsWith("data_") ? formatDateTime(v) : v;
      out.push({ key: String(f.key), label: f.label, value: display });
    }
  }
  return out;
}

function DadosSub({
  ctx,
  occurrence,
}: {
  ctx: InsertionContext;
  occurrence: Occurrence | null;
}) {
  const fields = useMemo(() => buildOccurrenceFields(occurrence), [occurrence]);
  const [feedback, isError, fire] = useFeedback();

  const insert = async (f: OccurrenceField) => {
    const ok = ctx.editor
      .chain()
      .focus()
      .insertSystemData({
        source: "occurrence",
        field: f.key,
        value: f.value,
      })
      .run();
    if (!ok) {
      fire("Falha ao inserir no editor.", true);
      return;
    }
    await recordLink(ctx.workspacePath, ctx.laudoId, {
      source_kind: "occurrence_field",
      metadata_json: JSON.stringify({ field: f.key, label: f.label, value: f.value }),
    });
    fire(`${f.label} inserido.`);
  };

  if (fields.length === 0) {
    return (
      <p className={styles.empty}>
        A ocorrência não tem campos preenchidos para inserir.
      </p>
    );
  }

  return (
    <>
      <p className={styles.hint}>
        Inserir um campo como dado do sistema vinculado à ocorrência. O
        valor aparece destacado (revisão pendente) até o perito confirmar.
      </p>
      <div className={styles.list}>
        {fields.map((f) => (
          <div key={f.key} className={styles.fieldRow}>
            <div>
              <span className={styles.fieldLabel}>{f.label}</span>
              <span className={styles.fieldValue}>{f.value}</span>
            </div>
            <button
              type="button"
              className={styles.insertBtn}
              onClick={() => void insert(f)}
            >
              Inserir
            </button>
          </div>
        ))}
      </div>
      <Feedback text={feedback} isError={isError} />
    </>
  );
}

// ---------------------------------------------------------------------------
// Sub-tab 2: Fotos (MediaAsset → figure[kind=image])

function FotosSub({ ctx }: { ctx: InsertionContext }) {
  const [photos, setPhotos] = useState<MediaAsset[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [feedback, isError, fire] = useFeedback();

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    commands
      .listDossiePhotos(ctx.workspacePath)
      .then((data) => {
        if (!cancelled) setPhotos(data);
      })
      .catch((err) => {
        if (!cancelled) setLoadError(toSicroError(err).message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [ctx.workspacePath]);

  const insert = async (photo: MediaAsset) => {
    const src = assetSrc(ctx.workspacePath, photo.relative_path) ?? "";
    const caption = photo.caption?.trim() || photo.original_filename || "Figura.";
    const ok = ctx.editor
      .chain()
      .focus()
      .insertFigure({
        src,
        kind: "image",
        alt: caption,
        caption,
        evidence_id: photo.id,
        evidence_kind: "photo",
        relative_path: photo.relative_path,
        source_hash: photo.sha256,
        metadata_json: JSON.stringify({
          category: photo.category,
          captured_at: photo.captured_at,
          mime_type: photo.mime_type,
          original_id: photo.original_id,
        }),
      })
      .run();
    if (!ok) {
      fire("Falha ao inserir a figura.", true);
      return;
    }
    await recordLink(ctx.workspacePath, ctx.laudoId, {
      source_kind: "photo",
      media_asset_id: photo.id,
      relative_path: photo.relative_path,
      source_hash: photo.sha256,
      metadata_json: JSON.stringify({
        original_id: photo.original_id,
        category: photo.category,
        caption: photo.caption,
      }),
    });
    fire(`Foto ${photo.original_id ?? photo.id.slice(0, 6)} inserida.`);
  };

  if (loading) return <p className={styles.empty}>Carregando fotos…</p>;
  if (loadError)
    return <p className={`${styles.feedback} ${styles.feedbackError}`}>{loadError}</p>;
  if (photos.length === 0) {
    return (
      <p className={styles.empty}>
        Nenhuma foto importada neste workspace.
      </p>
    );
  }

  return (
    <>
      <p className={styles.hint}>
        Inserir como figura numerada. O caminho é preservado relativo ao
        workspace; o `.sicrodoc` continua textual.
      </p>
      <div className={styles.list}>
        {photos.map((p) => (
          <PhotoRow
            key={p.id}
            photo={p}
            workspacePath={ctx.workspacePath}
            onInsert={() => void insert(p)}
          />
        ))}
      </div>
      <Feedback text={feedback} isError={isError} />
    </>
  );
}

function PhotoRow({
  photo,
  workspacePath,
  onInsert,
}: {
  photo: MediaAsset;
  workspacePath: string;
  onInsert: () => void;
}) {
  const src = useMemo(
    () => assetSrc(workspacePath, photo.relative_path),
    [workspacePath, photo.relative_path],
  );
  const [failed, setFailed] = useState(false);
  return (
    <div className={styles.item}>
      <div className={styles.itemPreview}>
        {src && !failed ? (
          <img
            src={src}
            alt={photo.caption ?? photo.id}
            className={styles.thumb}
            onError={() => setFailed(true)}
            loading="lazy"
          />
        ) : (
          <div className={styles.thumbPlaceholder}>
            <ImageOff size={14} />
          </div>
        )}
        <div className={styles.itemBody}>
          <strong>{photo.original_id ?? photo.id.slice(0, 8)}</strong>
          {photo.caption && <span>{photo.caption}</span>}
          {photo.category && (
            <span className={styles.itemHint}>{photo.category}</span>
          )}
        </div>
      </div>
      <div className={styles.actions}>
        <button type="button" className={styles.insertBtn} onClick={onInsert}>
          <FileImage size={11} /> Inserir foto
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-tab 3: Croquis (Croqui → figure[kind=croqui])

function CroquisSub({ ctx }: { ctx: InsertionContext }) {
  const [items, setItems] = useState<Croqui[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [feedback, isError, fire] = useFeedback();

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    commands
      .listCroquis(ctx.workspacePath)
      .then((data) => {
        if (!cancelled) setItems(data);
      })
      .catch((err) => {
        if (!cancelled) setLoadError(toSicroError(err).message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [ctx.workspacePath]);

  const insert = async (cr: Croqui) => {
    const pngPath = cr.last_export_relative_path;
    if (!pngPath) {
      fire(
        `Croqui "${cr.title}" ainda não tem PNG exportado. Abra o croqui, clique em "Exportar PNG" e tente novamente.`,
        true,
      );
      return;
    }
    const src = assetSrc(ctx.workspacePath, pngPath) ?? "";
    const isCorpo = cr.kind === "corporal";
    const isPlanta = cr.kind === "planta";
    const caption =
      cr.title ||
      (isCorpo
        ? "Carta de lesões."
        : isPlanta
          ? "Planta esquemática."
          : "Croqui.");
    const ok = ctx.editor
      .chain()
      .focus()
      .insertFigure({
        src,
        // Renderiza/exporta como figura PNG igual ao croqui de via; só a
        // PROCEDÊNCIA muda (corpo / viário / planta) pra registro/auditoria.
        kind: "croqui",
        alt: caption,
        caption,
        evidence_id: cr.id,
        evidence_kind: isCorpo ? "corpo" : isPlanta ? "planta" : "croqui",
        relative_path: pngPath,
        source_hash: null,
        metadata_json: JSON.stringify({
          croqui_kind: cr.kind,
          croqui_status: cr.status,
          schema_version: cr.schema_version,
          updated_at: cr.updated_at,
        }),
      })
      .run();
    if (!ok) {
      fire("Falha ao inserir o croqui.", true);
      return;
    }
    await recordLink(ctx.workspacePath, ctx.laudoId, {
      source_kind: "croqui",
      croqui_id: cr.id,
      relative_path: pngPath,
      metadata_json: JSON.stringify({
        croqui_title: cr.title,
        croqui_status: cr.status,
      }),
    });
    fire(`Croqui "${cr.title}" inserido.`);
  };

  if (loading) return <p className={styles.empty}>Carregando croquis…</p>;
  if (loadError)
    return <p className={`${styles.feedback} ${styles.feedbackError}`}>{loadError}</p>;
  if (items.length === 0) {
    return (
      <p className={styles.empty}>Nenhum croqui criado nesta ocorrência.</p>
    );
  }

  return (
    <>
      <p className={styles.hint}>
        A figura usa o PNG exportado mais recente do croqui. Se o croqui
        for editado, o perito deve re-exportar e re-inserir.
      </p>
      <div className={styles.list}>
        {items.map((cr) => (
          <CroquiRow
            key={cr.id}
            croqui={cr}
            workspacePath={ctx.workspacePath}
            onInsert={() => void insert(cr)}
          />
        ))}
      </div>
      <Feedback text={feedback} isError={isError} />
    </>
  );
}

function CroquiRow({
  croqui,
  workspacePath,
  onInsert,
}: {
  croqui: Croqui;
  workspacePath: string;
  onInsert: () => void;
}) {
  const src = useMemo(
    () =>
      croqui.last_export_relative_path
        ? assetSrc(workspacePath, croqui.last_export_relative_path)
        : null,
    [workspacePath, croqui.last_export_relative_path],
  );
  const [failed, setFailed] = useState(false);
  const hasPng = !!croqui.last_export_relative_path;
  return (
    <div className={styles.item}>
      <div className={styles.itemPreview}>
        {src && !failed ? (
          <img
            src={src}
            alt={croqui.title}
            className={styles.thumb}
            onError={() => setFailed(true)}
            loading="lazy"
          />
        ) : (
          <div className={styles.thumbPlaceholder}>
            <MapIcon size={14} />
          </div>
        )}
        <div className={styles.itemBody}>
          <strong>
            {croqui.kind === "corporal" && (
              <span
                style={{
                  background: "#7c3aed",
                  color: "#fff",
                  fontSize: 10,
                  fontWeight: 700,
                  padding: "1px 6px",
                  borderRadius: 6,
                  marginRight: 6,
                  verticalAlign: "middle",
                }}
              >
                Corporal
              </span>
            )}
            {croqui.title}
          </strong>
          <span className={styles.itemHint}>
            {croqui.status} · atualizado {formatDateTime(croqui.updated_at)}
          </span>
          {!hasPng && (
            <span className={styles.itemHint}>Sem PNG exportado ainda.</span>
          )}
        </div>
      </div>
      <div className={styles.actions}>
        <button
          type="button"
          className={styles.insertBtn}
          onClick={onInsert}
          disabled={!hasPng}
          title={
            hasPng
              ? "Inserir o último PNG exportado do croqui"
              : "Abra o croqui e clique em Exportar PNG primeiro"
          }
        >
          <MapIcon size={11} /> Inserir croqui
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-tab 4: Vídeo (frames individuais e storyboard completo)

function VideoSub({ ctx }: { ctx: InsertionContext }) {
  const [items, setItems] = useState<VideoMedia[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [active, setActive] = useState<VideoBundle | null>(null);
  const [opening, setOpening] = useState(false);
  const [feedback, isError, fire] = useFeedback();

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    commands
      .listVideoMedia(ctx.workspacePath)
      .then((data) => {
        if (!cancelled) setItems(data);
      })
      .catch((err) => {
        if (!cancelled) setLoadError(toSicroError(err).message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [ctx.workspacePath]);

  const openBundle = async (m: VideoMedia) => {
    setOpening(true);
    try {
      const bundle = await commands.openVideoMedia(ctx.workspacePath, m.id);
      setActive(bundle);
    } catch (err) {
      fire(toSicroError(err).message, true);
    } finally {
      setOpening(false);
    }
  };

  const insertFrame = async (frame: VideoStoryboardFrame) => {
    const src = assetSrc(ctx.workspacePath, frame.output_path) ?? "";
    const caption =
      frame.caption?.trim() || frame.title?.trim() || "Frame extraído.";
    const ok = ctx.editor
      .chain()
      .focus()
      .insertFigure({
        src,
        kind: "video_frame",
        alt: caption,
        caption,
        evidence_id: frame.id,
        evidence_kind: "video_frame",
        relative_path: frame.output_path,
        source_hash: null,
        metadata_json: JSON.stringify({
          media_hash: frame.media_hash,
          event_id: frame.event_id,
          requested_timestamp_s: frame.requested_timestamp_s,
          actual_timestamp_s: frame.actual_timestamp_s,
          delta_s: frame.delta_s,
          pts: frame.pts,
          time_base: frame.time_base,
          observed_frame_index: frame.observed_frame_index,
        }),
      })
      .run();
    if (!ok) {
      fire("Falha ao inserir o frame.", true);
      return;
    }
    await recordLink(ctx.workspacePath, ctx.laudoId, {
      source_kind: "video_frame",
      video_media_hash: frame.media_hash,
      video_event_id: frame.event_id,
      video_storyboard_frame_id: frame.id,
      relative_path: frame.output_path,
      metadata_json: JSON.stringify({
        title: frame.title,
        caption: frame.caption,
        timestamp_s: frame.actual_timestamp_s ?? frame.requested_timestamp_s,
      }),
    });
    fire(`Frame ${frame.title || frame.id.slice(0, 6)} inserido.`);
  };

  const insertStoryboardAll = async (bundle: VideoBundle) => {
    if (bundle.storyboard.length === 0) {
      fire("Vídeo sem frames coletados.", true);
      return;
    }
    const items = bundle.storyboard.map((f) => ({
      src: assetSrc(ctx.workspacePath, f.output_path),
      timestamp: formatTimestampMs(
        f.actual_timestamp_s ?? f.requested_timestamp_s,
      ),
      frame_label: f.observed_frame_index != null
        ? `Frame: ${f.observed_frame_index}`
        : f.title || `Frame: ${f.id.slice(0, 6)}`,
      description: f.caption?.trim() || f.notes?.trim() || "",
      storyboard_frame_id: f.id,
      event_id: f.event_id ?? undefined,
      media_hash: f.media_hash,
      pts: f.pts,
      time_base: f.time_base,
      relative_path: f.output_path,
    }));
    const ok = ctx.editor
      .chain()
      .focus()
      .insertStoryboardFromVideo({
        caption: `Sequência observada — ${bundle.media.filename}`,
        media_hash: bundle.media.sha256,
        items,
      })
      .run();
    if (!ok) {
      fire("Falha ao inserir o storyboard.", true);
      return;
    }
    await recordLink(ctx.workspacePath, ctx.laudoId, {
      source_kind: "video_storyboard",
      video_media_hash: bundle.media.sha256,
      metadata_json: JSON.stringify({
        media_filename: bundle.media.filename,
        frame_count: bundle.storyboard.length,
        frame_ids: bundle.storyboard.map((f) => f.id),
      }),
    });
    fire(`Storyboard com ${items.length} frame(s) inserido.`);
  };

  if (loading) return <p className={styles.empty}>Carregando vídeos…</p>;
  if (loadError)
    return <p className={`${styles.feedback} ${styles.feedbackError}`}>{loadError}</p>;
  if (items.length === 0 && !active) {
    return (
      <p className={styles.empty}>
        Nenhum vídeo registrado nesta ocorrência.
      </p>
    );
  }

  return (
    <>
      {!active && (
        <>
          <p className={styles.hint}>
            Selecionar um vídeo abre seus frames; cada frame pode ir como
            figura individual ou compor um storyboard completo.
          </p>
          <div className={styles.list}>
            {items.map((m) => (
              <div key={m.id} className={styles.item}>
                <div className={styles.itemPreview}>
                  <div className={styles.thumbPlaceholder}>
                    <Film size={14} />
                  </div>
                  <div className={styles.itemBody}>
                    <strong>{m.filename}</strong>
                    <span className={styles.itemHint}>
                      {m.duration_s != null
                        ? `${m.duration_s.toFixed(1)} s`
                        : "—"}
                      {m.width && m.height
                        ? ` · ${m.width}×${m.height}`
                        : ""}
                    </span>
                  </div>
                </div>
                <div className={styles.actions}>
                  <button
                    type="button"
                    className={styles.insertBtnGhost}
                    onClick={() => void openBundle(m)}
                    disabled={opening}
                  >
                    <Layers size={11} /> Abrir frames
                  </button>
                </div>
              </div>
            ))}
          </div>
        </>
      )}

      {active && (
        <div className={styles.bundle}>
          <div className={styles.bundleHeader}>
            <strong>{active.media.filename}</strong>
            <button
              type="button"
              className={styles.closeBtn}
              onClick={() => setActive(null)}
              aria-label="Fechar"
            >
              <X size={14} />
            </button>
          </div>
          <div className={styles.actions}>
            <button
              type="button"
              className={styles.insertBtn}
              onClick={() => void insertStoryboardAll(active)}
              disabled={active.storyboard.length === 0}
            >
              <Layers size={11} /> Inserir storyboard completo
              {active.storyboard.length > 0 && ` (${active.storyboard.length})`}
            </button>
          </div>
          {active.storyboard.length === 0 ? (
            <p className={styles.empty}>
              Nenhum frame coletado para este vídeo. Abra o módulo Vídeo
              para extrair frames.
            </p>
          ) : (
            <div className={styles.frameList}>
              {active.storyboard.map((f) => (
                <VideoFrameRow
                  key={f.id}
                  frame={f}
                  workspacePath={ctx.workspacePath}
                  onInsert={() => void insertFrame(f)}
                />
              ))}
            </div>
          )}
        </div>
      )}

      <Feedback text={feedback} isError={isError} />
    </>
  );
}

function VideoFrameRow({
  frame,
  workspacePath,
  onInsert,
}: {
  frame: VideoStoryboardFrame;
  workspacePath: string;
  onInsert: () => void;
}) {
  const src = useMemo(
    () => assetSrc(workspacePath, frame.output_path),
    [workspacePath, frame.output_path],
  );
  const [failed, setFailed] = useState(false);
  const time = frame.actual_timestamp_s ?? frame.requested_timestamp_s;
  return (
    <div className={styles.frameItem}>
      {src && !failed ? (
        <img
          src={src}
          alt={frame.title}
          onError={() => setFailed(true)}
          loading="lazy"
        />
      ) : (
        <div
          style={{
            width: 40,
            height: 24,
            background: "#0a0a0a",
            borderRadius: 2,
          }}
        />
      )}
      <div className={styles.frameInfo}>
        <strong>{frame.title || frame.id.slice(0, 8)}</strong>
        <small>{formatTimestampMs(time)}</small>
      </div>
      <button type="button" className={styles.frameBtn} onClick={onInsert}>
        Inserir frame
      </button>
    </div>
  );
}

function formatTimestampMs(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "00:00:00.000";
  const total = Math.max(0, seconds);
  const hh = Math.floor(total / 3600);
  const mm = Math.floor((total % 3600) / 60);
  const ss = Math.floor(total % 60);
  const ms = Math.round((total - Math.floor(total)) * 1000);
  return `${String(hh).padStart(2, "0")}:${String(mm).padStart(2, "0")}:${String(ss).padStart(2, "0")}.${String(ms).padStart(3, "0")}`;
}

// ---------------------------------------------------------------------------
// Sub-tab 5: Dossiê → FieldNote → paragraph (com referência)

function DossieSub({ ctx }: { ctx: InsertionContext }) {
  const [notes, setNotes] = useState<FieldNote[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [feedback, isError, fire] = useFeedback();

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    commands
      .listDossieNotes(ctx.workspacePath)
      .then((data) => {
        if (!cancelled) setNotes(data);
      })
      .catch((err) => {
        if (!cancelled) setLoadError(toSicroError(err).message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [ctx.workspacePath]);

  const insert = async (note: FieldNote) => {
    const text = note.text?.trim();
    if (!text) {
      fire("Anotação sem texto.", true);
      return;
    }
    // Note: o vínculo com a field_note original é gravado em `evidence_links`.
    // O parágrafo no `.sicrodoc` fica como texto comum — não inflamos o schema
    // do Paragraph com attrs específicos só para essa origem.
    const ok = ctx.editor
      .chain()
      .focus()
      .insertContent({
        type: "paragraph",
        content: [{ type: "text", text }],
      })
      .run();
    if (!ok) {
      fire("Falha ao inserir o parágrafo.", true);
      return;
    }
    await recordLink(ctx.workspacePath, ctx.laudoId, {
      source_kind: "field_note",
      field_note_id: note.id,
      metadata_json: JSON.stringify({
        category: note.category,
        priority: note.priority,
        note_created_at: note.note_created_at,
      }),
    });
    fire("Anotação inserida.");
  };

  if (loading) return <p className={styles.empty}>Carregando anotações…</p>;
  if (loadError)
    return <p className={`${styles.feedback} ${styles.feedbackError}`}>{loadError}</p>;
  if (notes.length === 0) {
    return <p className={styles.empty}>Nenhuma anotação de campo importada.</p>;
  }

  return (
    <>
      <p className={styles.hint}>
        Insere o texto da anotação como parágrafo no laudo. A referência
        para a `field_notes.id` original é guardada em `evidence_links`.
      </p>
      <div className={styles.list}>
        {notes.map((n) => (
          <div key={n.id} className={styles.item}>
            <div className={styles.itemBody}>
              <strong>{n.category ?? "Anotação"}</strong>
              <span style={{ whiteSpace: "pre-wrap", lineHeight: 1.4 }}>
                {(n.text ?? "").slice(0, 160)}
                {(n.text ?? "").length > 160 ? "…" : ""}
              </span>
              {n.note_created_at && (
                <span className={styles.itemHint}>
                  {formatDateTime(n.note_created_at)}
                </span>
              )}
            </div>
            <div className={styles.actions}>
              <button
                type="button"
                className={styles.insertBtn}
                onClick={() => void insert(n)}
              >
                <NotebookText size={11} /> Inserir anotação
              </button>
            </div>
          </div>
        ))}
      </div>
      <Feedback text={feedback} isError={isError} />
    </>
  );
}

// ---------------------------------------------------------------------------
// Sub-tab 6: Tabelas (checklist / vestígios / medições → evidenceTable)

function TabelasSub({ ctx }: { ctx: InsertionContext }) {
  const [feedback, isError, fire] = useFeedback();
  const [working, setWorking] = useState<EvidenceSourceKind | null>(null);

  const insertChecklist = async () => {
    setWorking("checklist_table");
    try {
      const data = await commands.listDossieChecklist(ctx.workspacePath);
      if (data.length === 0) {
        fire("Checklist vazio neste dossiê.", true);
        return;
      }
      const columns = [
        { key: "category", label: "Categoria" },
        { key: "question", label: "Pergunta" },
        { key: "answer", label: "Resposta" },
        { key: "note", label: "Observação" },
      ];
      const rows = data.map((c: ChecklistItem) => ({
        category: c.category ?? "—",
        question: c.question,
        answer: humanAnswer(c.answer),
        note: c.note ?? "",
      }));
      const ok = ctx.editor
        .chain()
        .focus()
        .insertEvidenceTable({
          kind: "checklist",
          title: "Checklist técnico",
          columns,
          rows,
          metadata_json: JSON.stringify({ items_count: data.length }),
        })
        .run();
      if (!ok) {
        fire("Falha ao inserir a tabela.", true);
        return;
      }
      await recordLink(ctx.workspacePath, ctx.laudoId, {
        source_kind: "checklist_table",
        metadata_json: JSON.stringify({
          items_count: data.length,
          item_ids: data.map((c) => c.id),
        }),
      });
      fire(`Checklist inserido (${data.length} itens).`);
    } catch (err) {
      fire(toSicroError(err).message, true);
    } finally {
      setWorking(null);
    }
  };

  const insertTraces = async () => {
    setWorking("traces_table");
    try {
      const data = await commands.listDossieTraces(ctx.workspacePath);
      if (data.length === 0) {
        fire("Sem vestígios registrados.", true);
        return;
      }
      const columns = [
        { key: "identifier", label: "Identificador" },
        { key: "type", label: "Tipo" },
        { key: "description", label: "Descrição" },
        { key: "location", label: "Localização" },
        { key: "dimensions", label: "Dim." },
      ];
      const rows = data.map((t: Trace) => ({
        identifier: t.identifier ?? "—",
        type: t.type ?? "—",
        description: t.description ?? "",
        location: t.location_description ?? "",
        dimensions: formatDimensions(t.length, t.width, t.unit),
      }));
      const ok = ctx.editor
        .chain()
        .focus()
        .insertEvidenceTable({
          kind: "traces",
          title: "Vestígios",
          columns,
          rows,
          metadata_json: JSON.stringify({ items_count: data.length }),
        })
        .run();
      if (!ok) {
        fire("Falha ao inserir a tabela.", true);
        return;
      }
      await recordLink(ctx.workspacePath, ctx.laudoId, {
        source_kind: "traces_table",
        metadata_json: JSON.stringify({
          items_count: data.length,
          item_ids: data.map((t) => t.id),
        }),
      });
      fire(`Vestígios inseridos (${data.length} itens).`);
    } catch (err) {
      fire(toSicroError(err).message, true);
    } finally {
      setWorking(null);
    }
  };

  const insertMeasurements = async () => {
    setWorking("measurements_table");
    try {
      const data = await commands.listDossieMeasurements(ctx.workspacePath);
      if (data.length === 0) {
        fire("Sem medições registradas.", true);
        return;
      }
      const columns = [
        { key: "label", label: "Identificação" },
        { key: "between", label: "Entre" },
        { key: "value", label: "Valor" },
        { key: "method", label: "Método" },
        { key: "note", label: "Observação" },
      ];
      const rows = data.map((m: Measurement) => ({
        label: m.label ?? "—",
        between: [m.point_a, m.point_b].filter(Boolean).join(" → ") || "—",
        value:
          m.value != null
            ? `${m.value.toFixed(2)} ${m.unit ?? ""}`.trim()
            : "—",
        method: m.method ?? "—",
        note: m.note ?? "",
      }));
      const ok = ctx.editor
        .chain()
        .focus()
        .insertEvidenceTable({
          kind: "measurements",
          title: "Medições",
          columns,
          rows,
          metadata_json: JSON.stringify({ items_count: data.length }),
        })
        .run();
      if (!ok) {
        fire("Falha ao inserir a tabela.", true);
        return;
      }
      await recordLink(ctx.workspacePath, ctx.laudoId, {
        source_kind: "measurements_table",
        metadata_json: JSON.stringify({
          items_count: data.length,
          item_ids: data.map((m) => m.id),
        }),
      });
      fire(`Medições inseridas (${data.length} itens).`);
    } catch (err) {
      fire(toSicroError(err).message, true);
    } finally {
      setWorking(null);
    }
  };

  return (
    <>
      <p className={styles.hint}>
        A tabela é gerada a partir do Dossiê e tratada como bloco
        imutável. Se for preciso corrigir um item, edite no Dossiê e
        insira de novo.
      </p>
      <div className={styles.tableButtons}>
        <button
          type="button"
          className={styles.insertBtnGhost}
          onClick={() => void insertChecklist()}
          disabled={working !== null}
        >
          <ListTodo size={11} /> Inserir tabela de checklist
        </button>
        <button
          type="button"
          className={styles.insertBtnGhost}
          onClick={() => void insertTraces()}
          disabled={working !== null}
        >
          <ClipboardList size={11} /> Inserir tabela de vestígios
        </button>
        <button
          type="button"
          className={styles.insertBtnGhost}
          onClick={() => void insertMeasurements()}
          disabled={working !== null}
        >
          <Ruler size={11} /> Inserir tabela de medições
        </button>
      </div>
      <Feedback text={feedback} isError={isError} />
    </>
  );
}

function humanAnswer(a: string): string {
  switch (a) {
    case "sim":
      return "Sim";
    case "nao":
      return "Não";
    case "nao_se_aplica":
      return "Não se aplica";
    case "nao_verificado":
      return "Não verificado";
    default:
      return a;
  }
}

function formatDimensions(
  length: number | null,
  width: number | null,
  unit: string | null,
): string {
  const u = unit ?? "";
  if (length != null && width != null)
    return `${length.toFixed(2)}×${width.toFixed(2)} ${u}`.trim();
  if (length != null) return `${length.toFixed(2)} ${u}`.trim();
  if (width != null) return `${width.toFixed(2)} ${u}`.trim();
  return "—";
}

