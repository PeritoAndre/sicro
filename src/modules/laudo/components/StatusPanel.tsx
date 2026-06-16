/**
 * StatusPanel — controles de status do laudo (F9).
 *
 * Status:
 *   - "rascunho"     (default) — totalmente editável.
 *   - "em_revisao"   — editável mas exibe banner amarelo "em revisão";
 *                       comentários e revisões esperam ação do perito.
 *   - "final"        — bloqueado para edição; selo SHA-256 + timestamp + autor.
 *
 * Transições permitidas:
 *   rascunho   → em_revisao
 *   em_revisao → rascunho | final
 *   final      → em_revisao (com confirmação: invalida o selo)
 *
 * Quando o status muda para "final", computamos um SHA-256 do conteúdo
 * serializado (JSON.stringify) e gravamos `doc.finalization`. Esse hash
 * permite verificar que o documento não foi alterado após finalização.
 *
 * O `LaudoEditorView` reage ao status via prop `mode` — se status===final,
 * deve forçar `mode="leitura"` (ou similar) para impedir edição.
 */

import { useEffect, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Edit3,
  FileCheck2,
  ShieldCheck,
} from "lucide-react";
import {
  loadBrandingAssets,
  normalizeEvidenceSrcsForSave,
  renderSicroDocToHtml,
  validateSicroDoc,
  type SicroDoc,
  type SicroDocStatus,
  type SicroDocSignature,
} from "../document-engine";
import { commands } from "@core/commands";
import { useLaudoStore } from "../store/laudoStore";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { DigitalSignatureDialog } from "./DigitalSignatureDialog";
import styles from "./StatusPanel.module.css";

interface StatusPanelProps {
  doc: SicroDoc | null;
}

export function StatusPanel({ doc }: StatusPanelProps) {
  const activeWorkspacePath = useWorkspaceStore((s) => s.activeWorkspacePath);
  const setStatus = useLaudoStore((s) => s.setStatus);

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notes, setNotes] = useState("");
  // F12.11 — Modal de assinatura digital.
  const [signatureDialogOpen, setSignatureDialogOpen] = useState(false);

  const current = (doc?.status ?? "rascunho") as SicroDocStatus;
  const finalization = doc?.finalization ?? null;

  // F12.11 — Quando o usuário concluir o dialog, atualiza o
  // `finalization.signature` no documento via setStatus (re-salva).
  const handleSigned = async (signature: SicroDocSignature) => {
    if (!activeWorkspacePath || !finalization) return;
    try {
      await setStatus(activeWorkspacePath, "final", {
        ...finalization,
        signature,
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  // F9 — Computa o número de erros (bloqueante). Não impedem save mas
  // bloqueiam transição → final.
  const [errorCount, setErrorCount] = useState(0);
  useEffect(() => {
    if (!doc) return;
    const warnings = validateSicroDoc(doc);
    setErrorCount(warnings.filter((w) => w.severity === "error").length);
  }, [doc]);

  if (!doc) {
    return (
      <p className={styles.empty}>Abra um laudo para gerenciar o status.</p>
    );
  }

  const transitionTo = async (next: SicroDocStatus) => {
    if (!activeWorkspacePath) return;
    setBusy(true);
    setError(null);
    try {
      if (next === "final") {
        if (errorCount > 0) {
          setError(
            "Não é possível finalizar: há erros de validação pendentes. Resolva-os no painel Validações.",
          );
          setBusy(false);
          return;
        }
        // Computa o hash do conteúdo no momento da finalização.
        const json = JSON.stringify({ content: doc.content, metadata: doc.metadata });
        const hash = await sha256Hex(json);
        await setStatus(activeWorkspacePath, "final", {
          finalized_at: new Date().toISOString(),
          finalized_by: "Perito",
          content_hash: hash,
          notes: notes.trim() || undefined,
        });
      } else {
        await setStatus(activeWorkspacePath, next);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <h3 className={styles.sectionTitle}>
        <ShieldCheck size={14} /> Status do laudo
      </h3>

      <div className={styles.statusCard}>
        <div className={styles.statusLabel}>
          {current === "rascunho" && (
            <>
              <Edit3 size={14} /> Rascunho
            </>
          )}
          {current === "em_revisao" && (
            <>
              <AlertTriangle size={14} /> Em revisão
            </>
          )}
          {current === "final" && (
            <>
              <FileCheck2 size={14} /> Final · Bloqueado
            </>
          )}
        </div>
        <p className={styles.statusDesc}>{describe(current)}</p>
        {current === "final" && finalization && (
          <div className={styles.finalizationCard}>
            <div className={styles.finRow}>
              <strong>Finalizado em:</strong>{" "}
              {prettyDate(finalization.finalized_at)}
            </div>
            <div className={styles.finRow}>
              <strong>Por:</strong> {finalization.finalized_by}
            </div>
            <div className={styles.finRow}>
              <strong>Hash:</strong>
              <code className={styles.hash} title={finalization.content_hash}>
                {finalization.content_hash.slice(0, 16)}…
              </code>
            </div>
            {finalization.notes && (
              <div className={styles.finRow}>
                <strong>Notas:</strong> {finalization.notes}
              </div>
            )}
            {/* F12.11 — Status da assinatura digital. */}
            {finalization.signature ? (
              <div className={styles.finRow}>
                <strong>Assinatura digital:</strong>{" "}
                <span style={{ color: "#4ade80" }}>
                  ✓ {finalization.signature.signer_name}
                </span>{" "}
                <small style={{ color: "var(--sicro-fg-dim)" }}>
                  ({finalization.signature.type} ·{" "}
                  {prettyDate(finalization.signature.signed_at)})
                </small>
              </div>
            ) : (
              <button
                type="button"
                className={styles.secondaryBtn}
                style={{ marginTop: 8 }}
                onClick={() => setSignatureDialogOpen(true)}
                disabled={busy}
              >
                <ShieldCheck size={13} /> Assinar digitalmente (A3/A1)
              </button>
            )}
          </div>
        )}
      </div>

      {error && <div className={styles.errorBanner}>{error}</div>}

      {errorCount > 0 && (
        <p className={styles.warning}>
          ⚠ {errorCount} erro(s) bloqueante(s) impedem a finalização.
        </p>
      )}

      <div className={styles.sectionLabel}>Ações</div>
      {current === "rascunho" && (
        <button
          type="button"
          className={styles.primaryBtn}
          onClick={() => transitionTo("em_revisao")}
          disabled={busy}
        >
          Enviar para revisão
        </button>
      )}
      {current === "em_revisao" && (
        <>
          <button
            type="button"
            className={styles.secondaryBtn}
            onClick={() => transitionTo("rascunho")}
            disabled={busy}
          >
            Voltar para rascunho
          </button>
          <div style={{ height: 4 }} />
          <textarea
            rows={2}
            className={styles.textarea}
            placeholder="Notas opcionais para o selo de finalização (revisado por X, parecer Y…)"
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            disabled={busy}
          />
          <button
            type="button"
            className={styles.finalBtn}
            onClick={() => transitionTo("final")}
            disabled={busy || errorCount > 0}
            title={
              errorCount > 0
                ? "Resolva os erros antes de finalizar"
                : "Finalizar e gerar selo SHA-256"
            }
          >
            <CheckCircle2 size={12} /> Finalizar laudo
          </button>
        </>
      )}
      {current === "final" && (
        <button
          type="button"
          className={styles.dangerBtn}
          onClick={() => {
            if (
              window.confirm(
                "Reabrir o laudo INVALIDA o selo de finalização. Tem certeza?",
              )
            ) {
              void transitionTo("em_revisao");
            }
          }}
          disabled={busy}
        >
          Reabrir para revisão
        </button>
      )}

      {/* F12.11 + H — Dialog de assinatura digital. Só montado quando aberto. */}
      {finalization && doc?.document_id && activeWorkspacePath && (
        <DigitalSignatureDialog
          open={signatureDialogOpen}
          finalization={finalization}
          workspacePath={activeWorkspacePath}
          laudoId={doc.document_id}
          onExportPdfForSigning={async () => {
            // H — Gera o PDF do laudo (mesmo pipeline do ExportMenu) para
            // que o perito faça upload no portal gov.br. Retornamos o
            // caminho relativo + absoluto pra UI mostrar e copiar.
            try {
              const branding = await loadBrandingAssets();
              const portableContent = normalizeEvidenceSrcsForSave(
                doc.content,
              );
              const html = renderSicroDocToHtml(
                { ...doc, content: portableContent },
                {
                  fullDocument: true,
                  branding,
                  evidenceAssets: null,
                  occurrence: null,
                },
              );
              const result = await commands.exportLaudoPdf(
                activeWorkspacePath,
                doc.document_id,
                html,
              );
              // O backend devolve relative_path; absoluto = workspace +
              // separador + relativo. No Windows, perito copia esse path.
              const sep = activeWorkspacePath.includes("\\") ? "\\" : "/";
              const abs = `${activeWorkspacePath}${sep}${result.relative_path.replace(
                /\//g,
                sep,
              )}`;
              return {
                relative_path: result.relative_path,
                absolute_path: abs,
              };
            } catch (err) {
              setError(
                err instanceof Error ? err.message : String(err),
              );
              return null;
            }
          }}
          onClose={() => setSignatureDialogOpen(false)}
          onSigned={(sig) => void handleSigned(sig)}
        />
      )}
    </>
  );
}

function describe(s: SicroDocStatus): string {
  switch (s) {
    case "rascunho":
      return "Editável. Use este status durante a redação do laudo.";
    case "em_revisao":
      return "Editável, mas marcado como revisão. Comentários e revisões aguardam ação.";
    case "final":
      return "Bloqueado para edição. Selo digital SHA-256 garante integridade.";
  }
}

function prettyDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString("pt-BR", {
      day: "2-digit",
      month: "2-digit",
      year: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}

/** SHA-256 via WebCrypto. Retorna hex string. */
async function sha256Hex(input: string): Promise<string> {
  if (typeof crypto === "undefined" || !crypto.subtle) {
    // Fallback: substring hash (não criptograficamente seguro!).
    let h = 0;
    for (let i = 0; i < input.length; i++) {
      h = (h << 5) - h + input.charCodeAt(i);
      h |= 0;
    }
    return `fallback-${(h >>> 0).toString(16).padStart(8, "0")}`;
  }
  const buf = new TextEncoder().encode(input);
  const hashBuf = await crypto.subtle.digest("SHA-256", buf);
  return Array.from(new Uint8Array(hashBuf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}
