/**
 * Inspector — right-side panel for the laudo editor.
 *
 * Tabs:
 *   1. Validações — DocumentWarning list from the validator.
 *   2. Estrutura — outline of headings.
 *   3. Evidências — Inserir foto/croqui/frame/storyboard/dado/tabela (MVP 4).
 *   4. Cabeçalho — institutional header configuration.
 *   5. Página   — page margins (MVP 2 ajuste runtime 1.2).
 *   6. Dados    — metadata of the SicroDoc envelope (id, template, timestamps).
 */

import { useEffect, useState, type CSSProperties } from "react";
import type { Editor } from "@tiptap/react";
import {
  AlertTriangle,
  Boxes,
  Gauge,
  Info,
  ListTree,
  Ruler,
} from "lucide-react";
import {
  findInstitutionalTemplate,
  formatCm,
  marginsInCm,
  resolveEffectiveMargins,
  resolvePageNumber,
  PAGE_NUMBER_FONTS,
  type DocumentWarning,
  type SicroDoc,
  type SicroDocPageNumber,
  type PageNumberAlign,
} from "../document-engine";
import { useLaudoStore } from "../store/laudoStore";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { formatDateTime } from "@core/formatters";
import { toSicroError } from "@core/errors";
import { EvidencePanel } from "./evidence/EvidencePanel";
import { NavigationPanel } from "./NavigationPanel";
import { SpeedMethodologyPanel } from "./SpeedMethodologyPanel";
import { DistanceMethodologyPanel } from "./DistanceMethodologyPanel";
import { HeaderTemplatesDialog } from "./HeaderTemplatesDialog";
import styles from "./Inspector.module.css";

interface InspectorProps {
  doc: SicroDoc | null;
  /** TipTap editor instance — necessário para a aba "Evidências" dispatchar inserts. */
  editor?: Editor | null;
  /** Workspace ativo — passado para os commands list*/
  workspacePath?: string | null;
  /** UUID do laudo aberto — usado para `evidence_links.target_id`. */
  laudoId?: string | null;
}

/**
 * F4.1 — Inspector lateral simplificado a 2 abas: foco em PROVAS.
 *
 * Validações, Estilos, Cabeçalho, Página e Dados saíram daqui e viram
 * popovers acessíveis pela barra superior (`EditorMenuBar`). Sem esse
 * corte, abas excedentes simplesmente saíam de tela (sem scroll
 * horizontal) e o perito perdia acesso a painéis antigos.
 */
type Tab = "outline" | "evidence" | "speed" | "distance";

export function Inspector({
  doc,
  editor = null,
  workspacePath = null,
  laudoId = null,
}: InspectorProps) {
  // F4.1 — `doc` permanece na API por compatibilidade do caller, mas o
  // Inspector agora só lida com Estrutura + Evidências (que dependem do
  // `editor` para extrair outline e do `workspacePath`/`laudoId` para
  // inserir evidências). Painéis que liam `doc` (validação/cabeçalho/
  // página/dados) migraram para a barra superior — `EditorMenuBar`.
  void doc;
  const [tab, setTab] = useState<Tab>("outline");
  const activeOccurrence = useWorkspaceStore((s) => s.activeOccurrence);

  return (
    <aside className={styles.inspector} aria-label="Inspetor do laudo">
      <div className={styles.tabs} role="tablist">
        <TabButton
          active={tab === "outline"}
          onClick={() => setTab("outline")}
          icon={<ListTree size={14} />}
          label="Estrutura"
        />
        <TabButton
          active={tab === "evidence"}
          onClick={() => setTab("evidence")}
          icon={<Boxes size={14} />}
          label="Evidências"
        />
        <TabButton
          active={tab === "speed"}
          onClick={() => setTab("speed")}
          icon={<Gauge size={14} />}
          label="Velocidade"
        />
        <TabButton
          active={tab === "distance"}
          onClick={() => setTab("distance")}
          icon={<Ruler size={14} />}
          label="Distância"
        />
      </div>

      <div className={styles.body}>
        {/* F4 — NavigationPanel: outline clicável + numeração automática. */}
        {tab === "outline" && <NavigationPanel editor={editor ?? null} />}
        {tab === "evidence" && (
          <EvidencePanel
            editor={editor}
            workspacePath={workspacePath}
            laudoId={laudoId}
            occurrence={activeOccurrence}
          />
        )}
        {tab === "speed" && (
          <SpeedMethodologyPanel editor={editor} workspacePath={workspacePath} />
        )}
        {tab === "distance" && (
          <DistanceMethodologyPanel editor={editor} workspacePath={workspacePath} />
        )}
      </div>
    </aside>
  );
}

function TabButton({
  active,
  onClick,
  icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      className={`${styles.tab} ${active ? styles.tabActive : ""}`}
      onClick={onClick}
    >
      <span style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
        {icon} {label}
      </span>
    </button>
  );
}

/**
 * F4.1 — `ValidationPanel`, `HeaderPanel`, `PagePanel`, `MetaPanel`
 * agora são exportados para que a barra superior do laudo possa
 * renderizá-los dentro de popovers. O Inspector lateral, F4.1+, foca
 * em provas: só "Estrutura" + "Evidências".
 */
export function ValidationPanel({
  warnings,
  hasDoc,
}: {
  warnings: DocumentWarning[];
  hasDoc: boolean;
}) {
  if (!hasDoc) {
    return <p className={styles.empty}>Abra um laudo para validar.</p>;
  }
  if (warnings.length === 0) {
    return (
      <>
        <h3 className={styles.sectionTitle}>Validações</h3>
        <p className={styles.empty}>
          ✓ Nenhum alerta. O laudo passa em todas as verificações.
        </p>
      </>
    );
  }

  // F9 — Agrupa por categoria + ordena por severidade.
  const byCat: Record<string, DocumentWarning[]> = {};
  for (const w of warnings) {
    const k = w.category ?? "outros";
    if (!byCat[k]) byCat[k] = [];
    byCat[k].push(w);
  }
  // Ordem visual das categorias (mais importante primeiro).
  const order: Array<string> = [
    "estrutura",
    "conteudo",
    "campos",
    "evidencia",
    "revisao",
    "finalizacao",
    "outros",
  ];
  const sevRank = (s: DocumentWarning["severity"]): number =>
    s === "error" ? 0 : s === "warning" ? 1 : 2;

  const errorCount = warnings.filter((w) => w.severity === "error").length;
  const warnCount = warnings.filter((w) => w.severity === "warning").length;
  const infoCount = warnings.filter((w) => w.severity === "info").length;

  return (
    <>
      <h3 className={styles.sectionTitle}>Validações</h3>
      <div
        style={{
          display: "flex",
          gap: 8,
          fontSize: 11,
          marginBottom: 8,
          flexWrap: "wrap",
        }}
      >
        {errorCount > 0 && (
          <span style={{ color: "#b91c1c", fontWeight: 600 }}>
            ● {errorCount} erro{errorCount === 1 ? "" : "s"}
          </span>
        )}
        {warnCount > 0 && (
          <span style={{ color: "#d97706", fontWeight: 600 }}>
            ● {warnCount} alerta{warnCount === 1 ? "" : "s"}
          </span>
        )}
        {infoCount > 0 && (
          <span style={{ color: "#0369a1" }}>
            ● {infoCount} info
          </span>
        )}
      </div>
      {order
        .map((k) => ({ k, list: byCat[k] ?? [] }))
        .filter(({ list }) => list.length > 0)
        .map(({ k: cat, list }) => (
          <div key={cat} style={{ marginBottom: 10 }}>
            <div
              style={{
                fontSize: 10,
                fontWeight: 700,
                letterSpacing: "0.06em",
                textTransform: "uppercase",
                color: "var(--sicro-fg-dim)",
                marginBottom: 4,
                paddingBottom: 2,
                borderBottom: "1px solid var(--sicro-divider)",
              }}
            >
              {categoryLabel(cat)} ({list.length})
            </div>
            <ul className={styles.warningList}>
              {list
                .sort((a, b) => sevRank(a.severity) - sevRank(b.severity))
                .map((w) => (
                  <li
                    key={w.id}
                    className={`${styles.warning} ${
                      w.severity === "info" ? styles.info : ""
                    }`}
                    style={
                      w.severity === "error"
                        ? {
                            borderLeftColor: "#b91c1c",
                            background: "rgba(220, 38, 38, 0.06)",
                          }
                        : undefined
                    }
                  >
                    <span
                      className={styles.warningMessage}
                      style={{
                        display: "inline-flex",
                        gap: 6,
                        alignItems: "center",
                      }}
                    >
                      {w.severity === "info" ? (
                        <Info size={12} />
                      ) : (
                        <AlertTriangle size={12} />
                      )}
                      {w.message}
                    </span>
                    {w.hint && (
                      <span className={styles.warningHint}>{w.hint}</span>
                    )}
                  </li>
                ))}
            </ul>
          </div>
        ))}
    </>
  );
}

function categoryLabel(cat: string): string {
  switch (cat) {
    case "estrutura":
      return "Estrutura";
    case "conteudo":
      return "Conteúdo";
    case "campos":
      return "Campos automáticos";
    case "evidencia":
      return "Evidências";
    case "revisao":
      return "Revisão";
    case "finalizacao":
      return "Finalização";
    default:
      return "Outros";
  }
}

// NOTA F4: `OutlinePanel` legado foi substituído por `NavigationPanel`
// (clicável, numerado, reativo ao cursor). Helper `buildOutline` também
// foi removido — `extractOutline` em `document-engine/sections` é o
// substituto canônico.

// ---------------------------------------------------------------------------
// Header configuration panel (MVP 2 ajuste runtime + N13)

export function HeaderPanel({ doc }: { doc: SicroDoc | null }) {
  const activeWorkspacePath = useWorkspaceStore((s) => s.activeWorkspacePath);
  const activeOccurrence = useWorkspaceStore((s) => s.activeOccurrence);
  const updateMetadata = useLaudoStore((s) => s.updateMetadata);
  // N13 — Controles do cabeçalho Word-style. O painel agora resume o
  // estado do novo header (enabled/altura) e o que aparece no editor;
  // edição do conteúdo continua acontecendo INLINE via double-click ou
  // botão "Editar no editor" abaixo (que ativa o modo no laudoStore).
  const setEditingRegion = useLaudoStore((s) => s.setEditingRegion);
  const setHeader = useLaudoStore((s) => s.setHeader);
  const headerEnabled = doc?.header?.enabled ?? false;
  const headerHeightCm = doc?.layout?.header_height_cm ?? 2.5;

  // Local drafts for the editable metadata fields. Sync to `doc.metadata`
  // whenever the underlying laudo changes.
  const initialNumeroLaudo =
    (doc?.metadata?.numero_laudo as string | undefined) ?? "";
  const initialSetor = (doc?.metadata?.setor as string | undefined) ?? "";
  const [numeroLaudo, setNumeroLaudo] = useState(initialNumeroLaudo);
  const [setor, setSetor] = useState(initialSetor);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [templatesOpen, setTemplatesOpen] = useState(false);

  useEffect(() => {
    setNumeroLaudo(initialNumeroLaudo);
    setSetor(initialSetor);
  }, [initialNumeroLaudo, initialSetor]);

  if (!doc) {
    return (
      <p className={styles.empty}>
        Abra um laudo para configurar o cabeçalho institucional.
      </p>
    );
  }

  const commit = async (key: "numero_laudo" | "setor", value: string) => {
    if (!activeWorkspacePath) return;
    const current =
      key === "numero_laudo" ? initialNumeroLaudo : initialSetor;
    if (value.trim() === current.trim()) return;
    try {
      await updateMetadata(activeWorkspacePath, { [key]: value.trim() });
      setFeedback(`Campo "${labelOf(key)}" salvo.`);
      setTimeout(() => setFeedback(null), 2500);
    } catch (err) {
      setFeedback(`Falha: ${toSicroError(err).message}`);
    }
  };

  const toggleHeader = async () => {
    if (!activeWorkspacePath || !doc?.header) return;
    try {
      await setHeader(activeWorkspacePath, {
        ...doc.header,
        enabled: !doc.header.enabled,
      });
    } catch (err) {
      setFeedback(`Falha: ${toSicroError(err).message}`);
    }
  };

  const startEditingHeader = () => {
    setEditingRegion("header");
  };

  return (
    <>
      <h3 className={styles.sectionTitle}>Cabeçalho</h3>

      {templatesOpen && activeWorkspacePath && (
        <HeaderTemplatesDialog
          open
          onClose={() => setTemplatesOpen(false)}
          doc={doc}
          workspacePath={activeWorkspacePath}
        />
      )}

      {/* N13 — Bloco "Cabeçalho Word-style" */}
      <div className={styles.headerStateBlock}>
        <div className={styles.headerStateRow}>
          <span className={styles.headerStateLabel}>Status</span>
          <span
            className={
              headerEnabled
                ? styles.headerBadgeOn
                : styles.headerBadgeOff
            }
          >
            {headerEnabled ? "Ativado" : "Desativado"}
          </span>
        </div>
        <div className={styles.headerStateRow}>
          <span className={styles.headerStateLabel}>Altura</span>
          <span className={styles.headerStateValue}>
            {headerHeightCm.toFixed(1)} cm
          </span>
        </div>
        <div className={styles.headerStateActions}>
          <button
            type="button"
            className={styles.headerBtnPrimary}
            onClick={() => setTemplatesOpen(true)}
            disabled={!activeWorkspacePath}
            title="Aplicar, salvar e escolher cabeçalhos salvos (criador de cabeçalho)"
          >
            Cabeçalhos salvos
          </button>
          <button
            type="button"
            className={styles.headerBtnSecondary}
            onClick={toggleHeader}
            title={
              headerEnabled
                ? "Desativa o cabeçalho (conteúdo permanece salvo)"
                : "Ativa o cabeçalho com o conteúdo já configurado"
            }
          >
            {headerEnabled ? "Desativar" : "Ativar"}
          </button>
          <button
            type="button"
            className={styles.headerBtnPrimary}
            onClick={startEditingHeader}
            disabled={!headerEnabled}
            title="Entra em modo de edição do cabeçalho (mesmo que duplo clique no laudo)"
          >
            Editar no editor
          </button>
        </div>
        <p className={styles.headerStateHint}>
          Conteúdo do cabeçalho aplica em <strong>todas as páginas</strong>.
          Para editar o texto, dê duplo clique no topo de qualquer página
          ou use o botão acima.
        </p>
      </div>

      <h4 className={styles.subSectionTitle}>
        Metadados do laudo (usados pelos campos automáticos)
      </h4>
      <p
        style={{
          fontSize: "var(--text-xs)",
          color: "var(--sicro-fg-muted)",
          lineHeight: 1.5,
          marginTop: 0,
        }}
      >
        Estes campos alimentam <code>{"{numero_laudo}"}</code>,{" "}
        <code>{"{setor}"}</code> etc. dentro do cabeçalho e do corpo.
      </p>

      <div className={styles.headerField}>
        <label htmlFor="hf-numero-laudo">Número do laudo</label>
        <input
          id="hf-numero-laudo"
          type="text"
          value={numeroLaudo}
          onChange={(e) => setNumeroLaudo(e.target.value)}
          onBlur={(e) => void commit("numero_laudo", e.target.value)}
          placeholder="Ex.: 12345/2026"
        />
      </div>

      <div className={styles.headerField}>
        <label htmlFor="hf-setor">Setor / departamento</label>
        <input
          id="hf-setor"
          type="text"
          value={setor}
          onChange={(e) => setSetor(e.target.value)}
          onBlur={(e) => void commit("setor", e.target.value)}
          placeholder="Ex.: DC/PCIAP"
        />
      </div>

      <div className={styles.headerReadOnly}>
        <h4 className={styles.subSectionTitle}>Do registro da ocorrência</h4>
        <p
          style={{
            fontSize: "var(--text-xs)",
            color: "var(--sicro-fg-dim)",
            margin: "0 0 var(--space-2)",
          }}
        >
          Editar estes campos na Home, ao criar/editar a ocorrência.
        </p>
        <dl className={styles.metaList}>
          <dt>BO nº</dt>
          <dd>{activeOccurrence?.numero_bo ?? "—"}</dd>
          <dt>Tipo de perícia</dt>
          <dd>{activeOccurrence?.tipo_pericia ?? "—"}</dd>
          <dt>Município</dt>
          <dd>{activeOccurrence?.municipio ?? "—"}</dd>
        </dl>
      </div>

      {feedback && (
        <p
          style={{
            fontSize: "var(--text-xs)",
            color: feedback.startsWith("Falha")
              ? "var(--sicro-danger)"
              : "var(--sicro-success)",
            margin: 0,
          }}
        >
          {feedback}
        </p>
      )}
    </>
  );
}

function labelOf(key: "numero_laudo" | "setor"): string {
  return key === "numero_laudo" ? "Número do laudo" : "Setor / departamento";
}

// ---------------------------------------------------------------------------
// Page (margins) panel — MVP 2 ajuste runtime 1.2

export function PagePanel({ doc }: { doc: SicroDoc | null }) {
  const activeWorkspacePath = useWorkspaceStore((s) => s.activeWorkspacePath);
  const updateLayout = useLaudoStore((s) => s.updateLayout);

  const template = doc
    ? findInstitutionalTemplate(doc.layout?.institutional_template)
    : null;
  const effective = doc
    ? marginsInCm(resolveEffectiveMargins(doc, template))
    : { top: 3, right: 2, bottom: 2.5, left: 3.5 };

  const [top, setTop] = useState(String(effective.top));
  const [right, setRight] = useState(String(effective.right));
  const [bottom, setBottom] = useState(String(effective.bottom));
  const [left, setLeft] = useState(String(effective.left));
  const [feedback, setFeedback] = useState<string | null>(null);

  useEffect(() => {
    setTop(String(effective.top));
    setRight(String(effective.right));
    setBottom(String(effective.bottom));
    setLeft(String(effective.left));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc?.layout?.page?.margins, doc?.layout?.institutional_template]);

  if (!doc) {
    return (
      <p className={styles.empty}>Abra um laudo para configurar a página.</p>
    );
  }

  const commit = async (
    side: "top" | "right" | "bottom" | "left",
    raw: string,
  ) => {
    if (!activeWorkspacePath) return;
    const parsed = parseCmInput(raw);
    if (parsed == null) {
      setFeedback(`Valor inválido em "${labelOfSide(side)}". Use cm (ex.: 2.5).`);
      return;
    }
    if (parsed < 0 || parsed > 8) {
      setFeedback(
        `Margem ${labelOfSide(side)} fora do intervalo aceito (0–8 cm).`,
      );
      return;
    }
    const previous = effective[side];
    if (Math.abs(previous - parsed) < 0.001) return; // no-op
    try {
      await updateLayout(activeWorkspacePath, {
        page: {
          margins: {
            top: side === "top" ? formatCm(parsed) : formatCm(effective.top),
            right:
              side === "right" ? formatCm(parsed) : formatCm(effective.right),
            bottom:
              side === "bottom"
                ? formatCm(parsed)
                : formatCm(effective.bottom),
            left:
              side === "left" ? formatCm(parsed) : formatCm(effective.left),
          },
        },
      });
      setFeedback(`Margem ${labelOfSide(side)} = ${formatCm(parsed)} salva.`);
      setTimeout(() => setFeedback(null), 2500);
    } catch (err) {
      setFeedback(`Falha: ${toSicroError(err).message}`);
    }
  };

  const resetToTemplate = async () => {
    if (!activeWorkspacePath || !template) return;
    try {
      await updateLayout(activeWorkspacePath, {
        page: { margins: undefined },
      } as never);
      setFeedback("Margens restauradas para o template institucional.");
      setTimeout(() => setFeedback(null), 2500);
    } catch (err) {
      setFeedback(`Falha: ${toSicroError(err).message}`);
    }
  };

  const isOverride = !!doc.layout?.page?.margins;
  const orientation: "portrait" | "landscape" =
    doc.layout?.orientation === "landscape" ? "landscape" : "portrait";

  const handleOrientation = async (next: "portrait" | "landscape") => {
    if (next === orientation) return;
    if (!activeWorkspacePath) return;
    try {
      // updateLayout faz merge top-level; passamos orientation diretamente.
      await updateLayout(activeWorkspacePath, { orientation: next });
      setFeedback(
        next === "landscape"
          ? "Orientação alterada para paisagem."
          : "Orientação alterada para retrato.",
      );
      setTimeout(() => setFeedback(null), 2500);
    } catch (err) {
      setFeedback(`Falha: ${toSicroError(err).message}`);
    }
  };

  return (
    <>
      <h3 className={styles.sectionTitle}>
        Página (A4 {orientation === "landscape" ? "paisagem" : "retrato"})
      </h3>
      <p
        style={{
          fontSize: "var(--text-xs)",
          color: "var(--sicro-fg-muted)",
          lineHeight: 1.5,
          marginTop: 0,
        }}
      >
        Margens em centímetros. A folha A4 no editor passa a respeitar estes
        valores imediatamente. PDF e DOCX (via `@page` e `PageMargin`) usam os
        mesmos valores ao exportar.
      </p>

      {/* F3 — Toggle orientação retrato/paisagem. */}
      <div className={styles.orientationGroup} role="radiogroup" aria-label="Orientação">
        <button
          type="button"
          role="radio"
          aria-checked={orientation === "portrait"}
          className={`${styles.orientationBtn} ${orientation === "portrait" ? styles.orientationBtnActive : ""}`}
          onClick={() => void handleOrientation("portrait")}
        >
          Retrato
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={orientation === "landscape"}
          className={`${styles.orientationBtn} ${orientation === "landscape" ? styles.orientationBtnActive : ""}`}
          onClick={() => void handleOrientation("landscape")}
        >
          Paisagem
        </button>
      </div>

      <div className={styles.marginGrid}>
        <MarginField
          label="Superior"
          value={top}
          onChange={setTop}
          onCommit={(v) => void commit("top", v)}
        />
        <MarginField
          label="Direita"
          value={right}
          onChange={setRight}
          onCommit={(v) => void commit("right", v)}
        />
        <MarginField
          label="Inferior"
          value={bottom}
          onChange={setBottom}
          onCommit={(v) => void commit("bottom", v)}
        />
        <MarginField
          label="Esquerda"
          value={left}
          onChange={setLeft}
          onCommit={(v) => void commit("left", v)}
        />
      </div>

      {isOverride && template && (
        <button
          type="button"
          className={styles.resetBtn}
          onClick={() => void resetToTemplate()}
        >
          Restaurar margens do template {template.name}
        </button>
      )}

      {feedback && (
        <p
          style={{
            fontSize: "var(--text-xs)",
            color: feedback.startsWith("Falha") || feedback.includes("inválido") || feedback.includes("fora")
              ? "var(--sicro-danger)"
              : "var(--sicro-success)",
            margin: 0,
          }}
        >
          {feedback}
        </p>
      )}

      <div className={styles.headerReadOnly}>
        <h4 className={styles.subSectionTitle}>Origem das margens</h4>
        <p
          style={{
            fontSize: "var(--text-xs)",
            color: "var(--sicro-fg-dim)",
            margin: 0,
          }}
        >
          {isOverride
            ? "Os valores acima foram definidos para este laudo (override)."
            : `Os valores acima vêm do template institucional ${template?.name ?? "padrão"}.`}
        </p>
      </div>

      <PageNumberSection doc={doc} />
    </>
  );
}

/**
 * Configuração da numeração de página "Folha X de Y" impressa no CABEÇALHO do
 * PDF. O número é o contador NATIVO do Chromium (por página); aqui o perito
 * escolhe o texto (tokens {n}/{total}), a fonte, o tamanho, a cor e o
 * alinhamento. Só aparece no PDF quando o laudo usa os campos {page}/{pages}.
 */
function PageNumberSection({ doc }: { doc: SicroDoc }) {
  const activeWorkspacePath = useWorkspaceStore((s) => s.activeWorkspacePath);
  const updateLayout = useLaudoStore((s) => s.updateLayout);
  const cfg = resolvePageNumber(doc);

  const [format, setFormat] = useState(cfg.format);
  const [feedback, setFeedback] = useState<string | null>(null);

  useEffect(() => {
    setFormat(cfg.format);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc.layout?.page_number?.format]);

  const save = async (patch: Partial<SicroDocPageNumber>) => {
    if (!activeWorkspacePath) return;
    try {
      await updateLayout(activeWorkspacePath, {
        page_number: { ...cfg, ...patch },
      });
      setFeedback("Numeração de página atualizada.");
      setTimeout(() => setFeedback(null), 2000);
    } catch (err) {
      setFeedback(`Falha: ${toSicroError(err).message}`);
    }
  };

  const aligns: { value: PageNumberAlign; label: string }[] = [
    { value: "left", label: "Esquerda" },
    { value: "center", label: "Centro" },
    { value: "right", label: "Direita" },
  ];

  return (
    <div className={styles.headerReadOnly}>
      <h4 className={styles.subSectionTitle}>Numeração de página (PDF)</h4>
      <p
        style={{
          fontSize: "var(--text-xs)",
          color: "var(--sicro-fg-dim)",
          margin: "0 0 8px",
          lineHeight: 1.5,
        }}
      >
        Aparece no cabeçalho do PDF quando o laudo usa os campos{" "}
        <code>{"{page}"}</code>/<code>{"{pages}"}</code>. Use{" "}
        <code>{"{n}"}</code> (página atual) e <code>{"{total}"}</code> (total).
      </p>

      <label
        style={{ display: "block", fontSize: "var(--text-xs)", marginBottom: 8 }}
      >
        <span style={{ color: "var(--sicro-fg-muted)" }}>Texto</span>
        <input
          type="text"
          value={format}
          onChange={(e) => setFormat(e.target.value)}
          onBlur={() => {
            if (format.trim() !== cfg.format) void save({ format: format.trim() });
          }}
          placeholder="Folha {n} de {total}"
          style={pnInputStyle}
        />
      </label>

      <div style={{ display: "flex", gap: 8, marginBottom: 8 }}>
        <label style={{ flex: 1, fontSize: "var(--text-xs)" }}>
          <span style={{ color: "var(--sicro-fg-muted)" }}>Fonte</span>
          <select
            value={cfg.font_family}
            onChange={(e) => void save({ font_family: e.target.value })}
            style={pnInputStyle}
          >
            {PAGE_NUMBER_FONTS.map((f) => (
              <option key={f} value={f}>
                {f}
              </option>
            ))}
          </select>
        </label>
        <label style={{ width: 70, fontSize: "var(--text-xs)" }}>
          <span style={{ color: "var(--sicro-fg-muted)" }}>Tam. (pt)</span>
          <input
            type="number"
            min={5}
            max={24}
            value={cfg.size_pt}
            onChange={(e) => {
              const v = Number(e.target.value);
              if (v >= 5 && v <= 24) void save({ size_pt: v });
            }}
            style={pnInputStyle}
          />
        </label>
        <label style={{ width: 52, fontSize: "var(--text-xs)" }}>
          <span style={{ color: "var(--sicro-fg-muted)" }}>Cor</span>
          <input
            type="color"
            value={cfg.color}
            onChange={(e) => void save({ color: e.target.value })}
            style={{ ...pnInputStyle, padding: 2, height: 28, cursor: "pointer" }}
          />
        </label>
      </div>

      <div role="radiogroup" aria-label="Alinhamento" style={{ display: "flex", gap: 4 }}>
        {aligns.map((a) => (
          <button
            key={a.value}
            type="button"
            role="radio"
            aria-checked={cfg.align === a.value}
            onClick={() => void save({ align: a.value })}
            className={`${styles.orientationBtn} ${cfg.align === a.value ? styles.orientationBtnActive : ""}`}
            style={{ flex: 1 }}
          >
            {a.label}
          </button>
        ))}
      </div>

      {feedback && (
        <p
          style={{
            fontSize: "var(--text-xs)",
            color: feedback.startsWith("Falha")
              ? "var(--sicro-danger)"
              : "var(--sicro-success)",
            margin: "8px 0 0",
          }}
        >
          {feedback}
        </p>
      )}
    </div>
  );
}

const pnInputStyle: CSSProperties = {
  width: "100%",
  boxSizing: "border-box",
  marginTop: 3,
  padding: "4px 7px",
  fontSize: 12,
  color: "var(--sicro-fg)",
  background: "var(--sicro-surface)",
  border: "1px solid var(--sicro-border)",
  borderRadius: "var(--radius-sm)",
  outline: "none",
};

function MarginField({
  label,
  value,
  onChange,
  onCommit,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  onCommit: (v: string) => void;
}) {
  return (
    <label className={styles.marginField}>
      <span>{label}</span>
      <div className={styles.marginInputWrap}>
        <input
          type="text"
          inputMode="decimal"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onBlur={(e) => onCommit(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              (e.target as HTMLInputElement).blur();
            }
          }}
        />
        <span className={styles.marginUnit}>cm</span>
      </div>
    </label>
  );
}

function labelOfSide(side: "top" | "right" | "bottom" | "left"): string {
  switch (side) {
    case "top":
      return "Superior";
    case "right":
      return "Direita";
    case "bottom":
      return "Inferior";
    case "left":
      return "Esquerda";
  }
}

/** Accepts "2", "2.5", "2,5", "25mm", "2cm" etc. Returns cm or null. */
function parseCmInput(raw: string): number | null {
  const trimmed = raw.trim().replace(",", ".");
  if (!trimmed) return null;
  const m = trimmed.match(/^(-?\d+(?:\.\d+)?)\s*(cm|mm)?$/i);
  if (!m) return null;
  const n = Number(m[1]);
  if (Number.isNaN(n)) return null;
  const unit = (m[2] ?? "cm").toLowerCase();
  return unit === "mm" ? n / 10 : n;
}

export function MetaPanel({ doc }: { doc: SicroDoc | null }) {
  if (!doc) {
    return <p className={styles.empty}>Abra um laudo para ver os metadados.</p>;
  }
  return (
    <>
      <h3 className={styles.sectionTitle}>Metadados</h3>
      <dl className={styles.metaList}>
        <dt>document_id</dt>
        <dd>{doc.document_id}</dd>
        <dt>occurrence_id</dt>
        <dd>{doc.occurrence_id}</dd>
        <dt>template_id</dt>
        <dd>{doc.template_id}</dd>
        <dt>schema_version</dt>
        <dd>{doc.schema_version}</dd>
        <dt>criado em</dt>
        <dd>{formatDateTime(doc.created_at)}</dd>
        <dt>atualizado em</dt>
        <dd>{formatDateTime(doc.updated_at)}</dd>
      </dl>
    </>
  );
}

// F4 — `buildOutline` removido. Use `extractOutline` em `document-engine/sections`.
