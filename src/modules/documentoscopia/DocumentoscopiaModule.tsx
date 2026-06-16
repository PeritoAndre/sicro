/**
 * Módulo Documentoscopia — OCR, layout documental, extração de campos,
 * integridade, comparação e apoio à análise documentoscópica.
 *
 * Fundação (Fase 1): importação real (cópia + hash + metadados), visualizador,
 * fluxo de OCR (motor mock rotulado, troca futura por sidecar), extração
 * heurística de campos, integridade/metadados/histórico e envio (quadro) ao
 * laudo. §13: ferramenta de APOIO — nunca conclui falsidade/autenticidade.
 */

import { useEffect, useMemo, useState, type ReactNode } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  FileOutput,
  FileSearch,
  FileStack,
  FileText,
  FlaskConical,
  GitCompare,
  Image as ImageIcon,
  Layers,
  RefreshCw,
  ScanText,
  ShieldCheck,
  Trash2,
} from "lucide-react";

import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { NoOccurrenceState } from "@components/NoOccurrenceState/NoOccurrenceState";
import {
  selectActiveWorkspacePath,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import {
  selectSelectedDocument,
  useDocumentsStore,
} from "@stores/documentsStore";
import {
  docStatusInfo,
  docTypeLabel,
  type DocumentCaseFile,
} from "@domain/documentoscopia";
import { DocWorkbench } from "./DocWorkbench";
import { ConfrontoWorkbench } from "./ConfrontoWorkbench";
import { RelatorioView } from "./RelatorioView";
import styles from "./DocumentoscopiaModule.module.css";

const IMAGE_EXTS = ["png", "jpg", "jpeg", "tif", "tiff", "bmp", "webp"];

type Filter = "all" | "pdf" | "image" | "pending";

function prettyBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}

export function DocumentoscopiaModule() {
  const ws = useWorkspaceStore(selectActiveWorkspacePath);
  const documents = useDocumentsStore((s) => s.documents);
  const loading = useDocumentsStore((s) => s.loading);
  const error = useDocumentsStore((s) => s.error);
  const load = useDocumentsStore((s) => s.load);
  const select = useDocumentsStore((s) => s.select);
  const importFile = useDocumentsStore((s) => s.importFile);
  const remove = useDocumentsStore((s) => s.remove);
  const applyUpdated = useDocumentsStore((s) => s.applyUpdated);
  const clearError = useDocumentsStore((s) => s.clearError);
  const selected = useDocumentsStore(selectSelectedDocument);

  const [importing, setImporting] = useState(false);
  const [filter, setFilter] = useState<Filter>("all");
  const [phase, setPhase] = useState<"exame" | "confronto" | "relatorio">(
    "exame",
  );

  useEffect(() => {
    if (ws) void load(ws);
  }, [ws, load]);

  const handleImport = async (kind: "pdf" | "image") => {
    if (!ws) return;
    const picked = await openFileDialog({
      multiple: false,
      filters: [
        kind === "pdf"
          ? { name: "PDF", extensions: ["pdf"] }
          : { name: "Imagem", extensions: IMAGE_EXTS },
      ],
    });
    if (typeof picked !== "string") return;
    setImporting(true);
    await importFile(ws, picked);
    setPhase("exame");
    setImporting(false);
  };

  const handleGenerateSample = async () => {
    if (!ws) return;
    setImporting(true);
    try {
      const abs = await commands.generateElaTestSample(ws);
      await importFile(ws, abs);
      setPhase("exame");
    } finally {
      setImporting(false);
    }
  };

  const filtered = useMemo(() => {
    return documents.filter((d) => {
      if (filter === "pdf") return d.file_type === "pdf";
      if (filter === "image") return d.file_type === "image";
      if (filter === "pending")
        return d.status === "importado" || d.status === "ocr_pendente";
      return true;
    });
  }, [documents, filter]);

  if (!ws) {
    return (
      <NoOccurrenceState
        icon={<FileStack size={36} strokeWidth={1.5} />}
        moduleName="Documentoscopia"
      />
    );
  }

  // Tela inicial (sem documentos): só o herói central, sem a barra de fases nem
  // o rodapé — a navegação Exame/Confronto/Relatório só aparece depois que há
  // documento. Importar pelo próprio herói leva direto ao Exame.
  if (documents.length === 0 && !loading) {
    return (
      <div className={styles.wrap}>
        {error && (
          <div className={styles.errorBanner}>
            <AlertTriangle size={14} /> {error.message}
            <button className={styles.bannerClose} onClick={clearError}>
              ×
            </button>
          </div>
        )}
        <DocEmptyState importing={importing} onImport={handleImport} />
      </div>
    );
  }

  return (
    <div className={styles.wrap}>
      <header className={styles.topBar}>
        <div className={styles.title}>
          <h1>
            <FileStack size={17} aria-hidden /> Documentoscopia{" "}
            <span className={styles.assist}>assistida</span>
          </h1>
          <p className={styles.subtitle}>
            OCR, layout, extração de campos, integridade e apoio à verificação
            documental. Ferramenta de apoio — a conclusão é sempre do perito.
          </p>
        </div>
        <div className={styles.headActions}>
          <div className={styles.modeSwitch} role="tablist" aria-label="Fase do exame">
            <button
              role="tab"
              aria-selected={phase === "exame"}
              data-active={phase === "exame"}
              onClick={() => setPhase("exame")}
            >
              <ScanText size={14} /> Exame
            </button>
            <button
              role="tab"
              aria-selected={phase === "confronto"}
              data-active={phase === "confronto"}
              onClick={() => setPhase("confronto")}
            >
              <GitCompare size={14} /> Confronto
            </button>
            <button
              role="tab"
              aria-selected={phase === "relatorio"}
              data-active={phase === "relatorio"}
              onClick={() => setPhase("relatorio")}
            >
              <FileOutput size={14} /> Relatório
            </button>
          </div>
          <Button
            variant="secondary"
            size="sm"
            leftIcon={<FileText size={14} />}
            onClick={() => void handleImport("pdf")}
            disabled={importing}
          >
            Importar PDF
          </Button>
          <Button
            variant="secondary"
            size="sm"
            leftIcon={<ImageIcon size={14} />}
            onClick={() => void handleImport("image")}
            disabled={importing}
          >
            Importar imagem
          </Button>
          <Button
            variant="ghost"
            size="sm"
            leftIcon={<FlaskConical size={14} />}
            onClick={() => void handleGenerateSample()}
            disabled={importing}
            title="Cria e importa uma imagem-teste com adulteração conhecida, para validar o ELA (controle positivo)"
          >
            Amostra de teste
          </Button>
          <Button
            variant="ghost"
            size="sm"
            leftIcon={<RefreshCw size={14} />}
            onClick={() => void load(ws)}
          >
            Atualizar
          </Button>
        </div>
      </header>

      {error && (
        <div className={styles.errorBanner}>
          <AlertTriangle size={14} /> {error.message}
          <button className={styles.bannerClose} onClick={clearError}>
            ×
          </button>
        </div>
      )}

      {phase === "confronto" ? (
        <div className={styles.confrontoArea}>
          <ConfrontoWorkbench ws={ws} documents={documents} />
        </div>
      ) : phase === "relatorio" ? (
        selected ? (
          <div className={styles.relatorioArea}>
            <RelatorioView key={selected.id} ws={ws} doc={selected} />
          </div>
        ) : (
          <div className={styles.centerEmpty}>
            <FileOutput size={40} strokeWidth={1.2} />
            <p>Selecione um documento no Exame para gerar o relatório.</p>
          </div>
        )
      ) : (
        <div className={styles.body}>
          <aside className={styles.list}>
            <div className={styles.listFilters}>
              {(
                [
                  ["all", "Todos"],
                  ["pdf", "PDF"],
                  ["image", "Imagem"],
                  ["pending", "Pendentes"],
                ] as [Filter, string][]
              ).map(([f, label]) => (
                <button
                  key={f}
                  className={styles.filterChip}
                  data-active={filter === f}
                  onClick={() => setFilter(f)}
                >
                  {label}
                </button>
              ))}
            </div>
            <div className={styles.listScroll}>
              {filtered.length === 0 ? (
                <p className={styles.emptyLine}>Nenhum documento neste filtro.</p>
              ) : (
                filtered.map((d) => (
                  <DocCard
                    key={d.id}
                    doc={d}
                    active={selected?.id === d.id}
                    onSelect={() => select(d.id)}
                    onDelete={() => void remove(ws, d.id)}
                  />
                ))
              )}
            </div>
          </aside>

          {selected ? (
            <DocWorkbench
              key={selected.id}
              ws={ws}
              doc={selected}
              onDocChanged={applyUpdated}
              onDeleted={() => void remove(ws, selected.id)}
            />
          ) : (
            <div className={styles.centerEmpty}>
              <FileStack size={40} strokeWidth={1.2} />
              <p>Selecione um documento à esquerda.</p>
            </div>
          )}
        </div>
      )}

      <footer className={styles.statusBar}>
        <span className={styles.statusItem}>
          {documents.length} documento{documents.length === 1 ? "" : "s"}
        </span>
        {selected && (
          <>
            <span className={styles.statusItem}>
              {docTypeLabel(selected.doc_type)}
            </span>
            <span className={styles.statusItem}>
              {selected.page_count || "—"} pág.
            </span>
            <span className={styles.statusItem}>
              {prettyBytes(selected.size_bytes)}
            </span>
            <span className={`${styles.statusItem} ${styles.mono}`}>
              {selected.sha256.slice(0, 16)}…
            </span>
          </>
        )}
        <span className={styles.statusSpacer} />
        <span className={styles.statusItem}>Motor OCR: mock</span>
      </footer>
    </div>
  );
}

function DocCard({
  doc,
  active,
  onSelect,
  onDelete,
}: {
  doc: DocumentCaseFile;
  active: boolean;
  onSelect: () => void;
  onDelete: () => void;
}) {
  const status = docStatusInfo(doc.status);
  return (
    <div
      className={styles.docCard}
      data-active={active}
      onClick={onSelect}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") onSelect();
      }}
    >
      <span className={styles.docIcon}>
        {doc.file_type === "pdf" ? (
          <FileText size={18} />
        ) : (
          <ImageIcon size={18} />
        )}
      </span>
      <div className={styles.docCardMain}>
        <span className={styles.docCardTitle} title={doc.original_filename}>
          {doc.title}
        </span>
        <span className={styles.docCardMeta}>
          {docTypeLabel(doc.doc_type)} · {doc.page_count || "—"} pág.
        </span>
      </div>
      <span className={styles.badge} data-tone={status.tone}>
        {status.label}
      </span>
      <button
        className={styles.docCardDelete}
        title="Remover"
        onClick={(e) => {
          e.stopPropagation();
          onDelete();
        }}
      >
        <Trash2 size={13} />
      </button>
    </div>
  );
}

function DocEmptyState({
  importing,
  onImport,
}: {
  importing: boolean;
  onImport: (kind: "pdf" | "image") => void;
}) {
  const explain: { icon: ReactNode; title: string; desc: string }[] = [
    {
      icon: <ScanText size={18} />,
      title: "OCR técnico",
      desc: "Texto por página/região, com confiança e revisão humana.",
    },
    {
      icon: <Layers size={18} />,
      title: "Layout documental",
      desc: "Blocos, tabelas, assinaturas, carimbos, QR/código de barras.",
    },
    {
      icon: <FileSearch size={18} />,
      title: "Extração de campos",
      desc: "CPF, CNPJ, placa, chassi, datas, valores, processo — revisáveis.",
    },
    {
      icon: <ShieldCheck size={18} />,
      title: "Integridade e metadados",
      desc: "Hash, metadados, histórico. O original nunca é alterado.",
    },
    {
      icon: <GitCompare size={18} />,
      title: "Comparação documental",
      desc: "Questionado × padrão: layout, campos, dimensões (em evolução).",
    },
    {
      icon: <FileOutput size={18} />,
      title: "Envio ao laudo",
      desc: "Quadro técnico com proveniência, hash e campos revisados.",
    },
  ];
  return (
    <div className={styles.emptyWrap}>
      <div className={styles.emptyHero}>
        <FileStack size={44} strokeWidth={1.2} />
        <h2 className={styles.emptyTitle}>Documentoscopia assistida</h2>
        <p className={styles.emptySub}>
          Importe PDFs ou imagens para OCR, extração de campos, análise de
          layout e apoio à verificação documental. Tudo com cadeia de custódia e
          preservação do original.
        </p>
        <div className={styles.emptyCta}>
          <Button
            variant="primary"
            leftIcon={<FileText size={15} />}
            onClick={() => onImport("pdf")}
            disabled={importing}
          >
            Importar PDF
          </Button>
          <Button
            variant="secondary"
            leftIcon={<ImageIcon size={15} />}
            onClick={() => onImport("image")}
            disabled={importing}
          >
            Importar imagem
          </Button>
        </div>
      </div>
      <div className={styles.emptyCards}>
        {explain.map((c) => (
          <div key={c.title} className={styles.explainCard}>
            <span className={styles.explainIcon}>{c.icon}</span>
            <div>
              <div className={styles.explainTitle}>{c.title}</div>
              <div className={styles.explainDesc}>{c.desc}</div>
            </div>
          </div>
        ))}
      </div>
      <p className={styles.methodNote}>
        Os recursos de OCR, extração e análise constituem apoio
        técnico-computacional. A interpretação e a conclusão documentoscópica
        dependem de avaliação humana pelo perito responsável.
      </p>
    </div>
  );
}
